//! 四步流水线的测试。放在这里而不是每个子文件一份：
//! 大部分用例要连着看「规则拦了什么、模型收到什么、merge 又拦回什么」才说清意图。

use super::client::{log_request, reply_budget, MAX_REPLY_TOKENS, MIN_REPLY_TOKENS};
use crate::models::DesktopRule;
use super::lookup::evidence_prompt;
use super::lookup::pick_by_keyword;
use super::parse::parse_rows;

    use super::*;

    fn item(id: i64, name: &str, target: Option<&str>, url: Option<&str>) -> SmartCategorizeItem {
        SmartCategorizeItem {
            id,
            name: name.into(),
            target: target.map(Into::into),
            url: url.map(Into::into),
        }
    }

    fn cats(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    fn row(id: i64, category: &str, reason: &str) -> One {
        One { id, category: category.into(), reason: reason.into() }
    }

    #[test]
    fn bookmark_prompt_is_not_logged_verbatim() {
        let prompt = "要判断的书签：[{\"地址\":\"https://intranet.example/private/doc\"}]";

        assert_eq!(log_request(prompt, false), prompt, "分类请求照常记，正文里已经没有本地路径");
        let redacted = log_request(prompt, true);
        assert!(!redacted.contains("intranet"), "{redacted}");
        assert!(redacted.contains("不写进日志"), "{redacted}");
    }

    #[test]
    fn prompt_hands_the_model_names_ids_and_existing_categories() {
        let items = vec![
            item(1, "PyCharm 2024.1", Some("D:/Program Files/PyCharm/pycharm64.exe"), None),
            item(2, "掘金", None, Some("https://juejin.cn/")),
        ];
        let prompt = user_prompt(&items, &cats(&["开发工具", "文档"]));

        assert!(prompt.contains("已有分类：开发工具、文档"), "{prompt}");
        assert!(prompt.contains("\"id\":1"), "{prompt}");
        assert!(prompt.contains("PyCharm 2024.1"), "{prompt}");
        assert!(prompt.contains("https://juejin.cn/"), "{prompt}");
    }

    /// 装在哪盘、哪个目录，模型用不上，也不该出本机
    #[test]
    fn local_paths_stay_out_of_the_prompt() {
        let deep = format!("C:/Users/me/Desktop/私密/{}", "明细/".repeat(40));
        let prompt = user_prompt(
            &[item(1, "PayCharm", Some(&deep), None), item(2, "x", Some(r"D:\a b\c.exe"), None)],
            &cats(&["其他"]),
        );

        assert!(!prompt.contains("target"), "{prompt}");
        assert!(!prompt.contains("私密"), "本地路径漏进提示词了");
        assert!(!prompt.contains("c.exe"), "{prompt}");
        assert!(prompt.chars().count() < 200, "长路径没被挡住：{}", prompt.chars().count());
    }

    #[test]
    fn no_list_tells_the_model_to_pick_few_of_its_own() {
        let prompt = user_prompt(&[item(7, "Obsidian", Some("C:/x/obsidian.exe"), None)], &[]);

        assert!(prompt.contains("不给现成分类"), "{prompt}");
        assert!(prompt.contains("8 个封顶"), "{prompt}");
        assert!(prompt.contains("宁宽勿细"), "{prompt}");
        assert!(!prompt.contains("target"), "{prompt}");
    }

    /// 桌面报已有分类（排掉规则占用的），书签什么都不报 —— 让 AI 自己分类
    #[test]
    fn only_desktop_shows_the_model_existing_categories() {
        let names = cats(&["开发工具", "文档", "AI工具"]);
        let reserved = cats(&["文档"]);

        assert_eq!(
            names_for_model("app", &names, &reserved),
            vec!["开发工具".to_string(), "AI工具".to_string()]
        );
        assert!(names_for_model("bookmark", &names, &reserved).is_empty(), "书签不该看到现有分类");
    }

    #[test]
    fn long_urls_get_cut_before_the_request() {
        let deep = format!("https://e.com/{}", "a/b/".repeat(300));
        let prompt = user_prompt(&[item(1, "x", None, Some(&deep))], &cats(&["其他"]));
        assert!(prompt.chars().count() < 400, "提示词被长网址撑爆了");
    }

    #[test]
    fn the_prompt_asks_for_as_few_categories_as_possible() {
        let system = system_prompt(&[]);
        assert!(system.contains("分类总数越少越好"), "{system}");
        assert!(system.contains("不要造只装得下一个条目的分类"), "{system}");
    }

    /// 规则占的名字要从候选里剔掉，还要在提示词里点名列禁
    #[test]
    fn rule_owned_names_are_out_of_the_model_view_and_named_as_banned() {
        let reserved = cats(&["文档", "视频"]);
        let open = names_for_model("app", &cats(&["文档", "开发工具", "视频"]), &reserved);
        assert_eq!(open, vec!["开发工具".to_string()], "{open:?}");

        let system = system_prompt(&reserved);
        assert!(system.contains("以下分类名归本地规则管，你一个都不许用（不许复用也不许新建同名）：文档、视频"), "{system}");
        // 没有规则时不提这茬，书签那边用不上
        assert!(!system_prompt(&[]).contains("归本地规则管"));
    }

    #[test]
    fn a_rule_owned_category_the_model_insists_on_is_blocked_locally() {
        let items = vec![item(1, "a", None, None), item(2, "b", None, None)];
        let rows = vec![row(1, "文档", "文本文件"), row(2, "开发工具", "IDE")];
        let merged = merge(&items, &rows, &cats(&["文档"]));

        assert_eq!(merged[0].suggested_category, "", "规则占用的分类没拦住");
        assert!(merged[0].reason.contains("归本地规则管"), "{}", merged[0].reason);
        assert!(merged[0].reason.contains("文档"), "{}", merged[0].reason);
        assert_eq!(merged[1].suggested_category, "开发工具");
        assert_eq!(merged[1].reason, "IDE");
    }

    #[test]
    fn the_reply_budget_grows_with_the_list() {
        assert_eq!(reply_budget(1), MIN_REPLY_TOKENS);
        assert_eq!(reply_budget(43), 6880);
        assert_eq!(reply_budget(400), MAX_REPLY_TOKENS);
    }

    #[test]
    fn a_cut_off_reply_keeps_the_rows_it_finished() {
        // 整份一次提交后，回复有可能被额度截在数组半路：完整的几条要收下
        let text = r#"[{"id":1,"category":"开发工具","reason":"IDE"},{"id":2,"category":"影音娱乐"#;
        let rows = parse_rows(text).expect("截断的尾巴不该拖垮整份回复");
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].category, "开发工具");
    }

    #[test]
    fn rows_line_up_with_items_by_id_not_by_order() {
        let items = vec![item(1, "a", None, None), item(2, "b", None, None), item(3, "c", None, None)];
        // 模型换了顺序、少答一条、又给了一条空分类
        let rows = vec![row(3, "影音娱乐", "播放器"), row(2, "  ", "说不上来")];
        let merged = merge(&items, &rows, &[]);

        assert_eq!(merged[0].suggested_category, "");
        assert!(merged[0].reason.contains("手动选"), "{}", merged[0].reason);
        assert_eq!(merged[2].suggested_category, "影音娱乐");
        assert!(merged[1].suggested_category.is_empty());
        assert!(merged[1].reason.contains("空分类"), "{}", merged[1].reason);
    }

    /// 不分批了：整份一次请求，服务端只该被连上一回
    #[tokio::test]
    async fn the_whole_list_goes_out_in_one_request() {
        let items: Vec<_> = (1..=44).map(|i| item(i, "x", None, None)).collect();
        let reply = r#"{"choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"[{\"id\":1,\"category\":\"开发工具\",\"reason\":\"IDE\"}]"}}]}"#;
        let (port, server) = counting_server(reply);
        let target = ChatTarget {
            base: format!("http://127.0.0.1:{port}/v1"),
            key: Some("sk-test".into()),
            model: "glm".into(),
        };

        let rows = classify(&target, &items, &cats(&["开发工具"]), &[])
            .await
            .expect("44 条一起发，一次请求就该分完");
        assert_eq!(rows.len(), 44, "{rows:?}");
        assert_eq!(server.join().unwrap(), 1, "还是分批发了");
    }

    /// 收下第一个连接并回一次固定正文，再等等看有没有第二个连接冒出来
    fn counting_server(reply: &str) -> (u16, std::thread::JoinHandle<usize>) {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let reply = reply.to_string();
        let handle = std::thread::spawn(move || {
            let Ok((mut sock, _)) = listener.accept() else { return 0 };
            let mut hits = 1;
            sock.set_read_timeout(Some(std::time::Duration::from_millis(400)))
                .unwrap();
            let mut sink = [0u8; 4096];
            while sock.read(&mut sink).map(|n| n > 0).unwrap_or(false) {}
            let _ = sock.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                )
                .as_bytes(),
            );
            let _ = sock.flush();
            drop(sock);

            let _ = listener.set_nonblocking(true);
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
            while std::time::Instant::now() < deadline {
                match listener.accept() {
                    Ok(_) => hits += 1,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    }
                    Err(_) => break,
                }
            }
            hits
        });
        (port, handle)
    }

    /// rig 给我们的结构化输出到底长什么样、非标准网关它认不认——这条用真 HTTP 打一次就知道。
    /// 顺带钉住一条实测结论：rig 对 finish_reason 是硬要求，缺这个字段的网关会被判失败。
    #[tokio::test]
    async fn typed_run_against_a_plain_openai_reply() {
        let body = r#"{"choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"[{\"id\":1,\"category\":\"开发工具\",\"reason\":\"IDE\"}]"}}]}"#;
        let (port, server) = reply_once(body);
        let target = ChatTarget {
            base: format!("http://127.0.0.1:{port}/v1"),
            key: Some("sk-test".into()),
            model: "m".into(),
        };
        let items = vec![item(1, "PyCharm", None, None), item(2, "未知", None, None)];
        let rows = classify(&target, &items, &cats(&["开发工具"]), &[])
            .await
            .expect("接口给 200 + 标准 OpenAI 报文，rig 应该能跑通");

        assert_eq!(rows.len(), 2, "{rows:?}");
        assert_eq!(rows[0].suggested_category, "开发工具", "{rows:?}");
        assert_eq!(rows[0].reason, "IDE", "{rows:?}");
        assert!(rows[1].suggested_category.is_empty(), "{rows:?}");
        assert!(rows[1].reason.contains("手动选"), "{rows:?}");
        let _ = server.join();
    }

    /// 一次连接、一条固定 200 响应；先把请求读干净再回，避免半路挂断
    fn reply_once(body: &str) -> (u16, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let body = body.to_string();
        let handle = std::thread::spawn(move || {
            let Ok((mut sock, _)) = listener.accept() else { return };
            sock.set_read_timeout(Some(std::time::Duration::from_millis(400)))
                .unwrap();
            let mut sink = [0u8; 4096];
            while sock.read(&mut sink).map(|n| n > 0).unwrap_or(false) {}
            let _ = sock.write_all(
                format!(
                    "HTTP/1.1 200 OK
Content-Type: application/json
Content-Length: {}
Connection: close

{body}",
                    body.len()
                )
                .as_bytes(),
            );
            let _ = sock.flush();
        });
        (port, handle)
    }

    #[test]
    fn parses_the_reply_shapes_models_actually_send() {
        let rows = parse_rows(r#"[{"id":1,"category":"开发工具","reason":"IDE"}]"#).unwrap();
        assert_eq!(rows[0].category, "开发工具");

        let rows = parse_rows("好的，结果如下：\n```json\n[{\"id\":2,\"category\":\"影音娱乐\"}]\n```").unwrap();
        assert_eq!(rows[0].id, 2);
        // reason 可以缺，category 才是关键
        assert_eq!(rows[0].reason, "");

        let rows = parse_rows(r#"{"items":[{"id":3,"category":"文档"}]}"#).unwrap();
        assert_eq!(rows[0].id, 3);

        assert!(parse_rows("这个我不确定，你看名字判断吧").is_none());
    }

    /// 上线过的错：rig 把请求挪到 Responses 线，正文多出 include 字段，
    /// CodeArts 上游回 malformed_json.400。这条钉住「只发标准 chat 字段」。
    #[tokio::test]
    async fn the_request_stays_on_the_plain_chat_wire() {
        let reply = r#"{"choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"[{\"id\":1,\"category\":\"开发工具\",\"reason\":\"IDE\"}]"}}]}"#;
        let (port, server) = capturing_server(reply);
        let target = ChatTarget {
            base: format!("http://127.0.0.1:{port}/v1"),
            key: Some("sk-test".into()),
            model: "glm".into(),
        };

        let rows = classify(
            &target,
            &[item(1, "PyCharm", Some(r"C:\Users\me\Desktop\私密\2月报表.exe"), None)],
            &cats(&["开发工具"]),
            &cats(&["文档"]),
        )
        .await
        .expect("标准 chat 报文应该能跑通");
        assert_eq!(rows[0].suggested_category, "开发工具", "{rows:?}");

        let req = server.join().unwrap();
        assert!(req.starts_with("POST /v1/chat/completions"), "路径不对：{req}");
        assert!(req.contains("\"max_tokens\""), "max_tokens 没发出去：{req}");
        assert!(!req.contains("include"), "混进了 Responses 字段：{req}");
        assert!(!req.contains("response_format"), "还在要结构化输出：{req}");
        assert!(!req.contains("reasoning"), "还在发 reasoning 开关：{req}");
        assert!(
            req.contains(r#""thinking":{"type":"disabled"}"#),
            "思考没关掉，还会被截断：{req}"
        );
        assert!(req.contains("归本地规则管"), "规则占用名字的禁令没发出去：{req}");
        assert!(!req.contains("target"), "还在发本地路径字段：{req}");
        assert!(
            !req.contains("报表") && !req.contains("Desktop"),
            "本地路径漏进请求体了：{req}"
        );
    }

    /// 一条连接：收下整个请求，回 200 + 给定正文，把请求原文交出去
    fn capturing_server(reply: &str) -> (u16, std::thread::JoinHandle<String>) {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let reply = reply.to_string();
        let handle = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            sock.set_read_timeout(Some(std::time::Duration::from_millis(400))).unwrap();
            let mut raw: Vec<u8> = Vec::new();
            let mut buf = [0u8; 4096];
            while let Ok(n) = sock.read(&mut buf) {
                if n == 0 {
                    break;
                }
                raw.extend_from_slice(&buf[..n]);
            }
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                reply.len()
            );
            let _ = sock.write_all(head.as_bytes());
            let _ = sock.flush();
            String::from_utf8_lossy(&raw).to_string()
        });
        (port, handle)
    }

    fn rule(category: &str, keywords: Option<&str>, exts: Option<&str>) -> DesktopRule {
        DesktopRule {
            id: 1,
            category: category.into(),
            keywords: keywords.map(Into::into),
            extensions: exts.map(Into::into),
            enabled: true,
            sort: 0,
        }
    }

    #[test]
    fn rule_hits_never_reach_the_model() {
        let rules = vec![
            rule("文档", None, Some(".xlsx,.pdf")),
            rule("开发工具", Some("pycharm,vscode"), None),
        ];
        let items = vec![
            item(1, "报表.xlsx", Some("C:/Desktop/私密/报表.xlsx"), None),
            item(2, "PyCharm 2023.2.lnk", Some("F:/Program/PyCharm/pycharm64.exe"), None),
            item(3, "Loomy.lnk", Some("F:/Program/Loomy/loomy.exe"), None),
        ];
        let (ruled, rest) = by_rules(&items, &rules);

        // 报表被 .xlsx 规则判成「文档」，连文件名都不出本机
        assert_eq!(ruled.len(), 2, "{ruled:?}");
        assert_eq!(ruled[0].suggested_category, "文档");
        assert!(ruled[0].reason.contains("分类规则"), "{}", ruled[0].reason);
        assert_eq!(ruled[1].suggested_category, "开发工具");
        assert_eq!(rest.len(), 1, "{rest:?}");
        assert_eq!(rest[0].name, "Loomy.lnk");
    }

    #[test]
    fn without_rules_everything_goes_to_the_model() {
        let items = vec![item(1, "a.docx", None, None)];
        let (ruled, rest) = by_rules(&items, &[]);
        assert!(ruled.is_empty());
        assert_eq!(rest.len(), 1);
    }

    #[test]
    fn keyword_needs_two_hits_to_claim_a_category() {
        let candidates = cats(&["开发工具", "AI办公", "笔记软件"]);
        // 「AI」和「办公」都出现了才算对上
        assert_eq!(
            pick_by_keyword("Loomy 是一款 AI 原生的办公 Agent，能写文档", &candidates).as_deref(),
            Some("AI办公")
        );
        // 只中一个片段不算：那是巧合不是判断
        assert_eq!(pick_by_keyword("一款工具类应用", &candidates), None);
        assert_eq!(pick_by_keyword("完全无关的一段话", &candidates), None);
        // 命中多的赢
        assert_eq!(
            pick_by_keyword("IDE 集成开发环境，写代码的开发工具", &candidates).as_deref(),
            Some("开发工具")
        );
    }

    /// 单字或双字的分类名没有两个片段可数，整体出现即算
    #[test]
    fn short_category_names_count_as_one_hit() {
        assert_eq!(pick_by_keyword("这是个办公套件的官网", &cats(&["办公"])).as_deref(), Some("办公"));
        assert_eq!(pick_by_keyword("这是个办公套件的官网", &cats(&["AI"])), None);
        assert_eq!(pick_by_keyword("AI 驱动的工具", &cats(&["AI"])).as_deref(), Some("AI"));
    }

    #[test]
    fn remembered_software_skips_the_model() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE ai_memory (name TEXT PRIMARY KEY COLLATE NOCASE, category TEXT NOT NULL, evidence TEXT, source TEXT NOT NULL, at INTEGER NOT NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ai_memory VALUES ('Loomy', 'AI办公', 'Loomy（loomy.ai）', 'search', 1)",
            [],
        )
        .unwrap();

        let (known, rest) = by_memory(
            &conn,
            &[item(1, "Loomy.lnk", Some("F:/Loomy/Loomy.exe"), None), item(2, "新软件.lnk", None, None)],
        );

        assert_eq!(known.len(), 1, "{known:?}");
        assert_eq!(known[0].suggested_category, "AI办公");
        assert!(known[0].reason.contains("本机记着"), "{}", known[0].reason);
        assert!(known[0].reason.contains("Loomy（loomy.ai）"), "{}", known[0].reason);
        assert_eq!(rest.len(), 1, "没记过的还要留给模型");
        assert_eq!(stem("Loomy.lnk"), "Loomy");
    }

    #[test]
    fn evidence_reaches_the_prompt_and_paths_do_not() {
        let rows = vec![(
            item(1, "Loomy", Some(r"C:\Users\me\Desktop\私密\报表.exe"), None),
            "- Loomy官网（loomy.ai）AI 原生办公 Agent".to_string(),
        )];
        let prompt = evidence_prompt(&rows, &cats(&["AI办公"]));

        assert!(prompt.contains("「Loomy」的资料："), "{prompt}");
        assert!(prompt.contains("Loomy官网"), "{prompt}");
        assert!(!prompt.contains("target"), "{prompt}");
        assert!(!prompt.contains("报表"), "本地路径漏进了带资料的提示词");
    }

