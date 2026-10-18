use rusqlite::{params, Connection};
use crate::models::AiModelInput;
use crate::win::dpapi;
use super::target::first_model_id;
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute(
            "CREATE TABLE ai_models (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               name TEXT NOT NULL,
               base_url TEXT NOT NULL,
               model TEXT NOT NULL,
               key_enc BLOB,
               sort INTEGER DEFAULT 0,
               created_at DATETIME DEFAULT CURRENT_TIMESTAMP
             )",
            [],
        )
        .unwrap();
        // 拉回来的模型列表挂在 settings 那张 KV 上
        c.execute("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT)", [])
            .unwrap();
        c
    }

    fn input(id: i64, key: Option<&str>) -> AiModelInput {
        AiModelInput {
            id,
            name: "内网模型".into(),
            base_url: "https://gw.local/fy29/v1/".into(),
            model: "fy29-chat".into(),
            api_key: key.map(Into::into),
            sort: Some(0),
        }
    }

    fn stored_key(c: &Connection, id: i64) -> Option<Vec<u8>> {
        c.query_row(
            "SELECT key_enc FROM ai_models WHERE id = ?1",
            params![id],
            |r| r.get::<_, Option<Vec<u8>>>(0),
        )
        .unwrap()
    }

    #[test]
    fn saves_and_never_returns_the_key() {
        let c = conn();
        let saved = save_model(&c, &input(0, Some("sk-secret"))).unwrap();
        assert!(saved.has_key);
        // 尾部斜杠被去掉，密钥不出现在返回体里
        assert_eq!(saved.base_url, "https://gw.local/fy29/v1");
        assert!(!format!("{saved:?}").contains("sk-secret"));
        assert_eq!(
            dpapi::unprotect(&stored_key(&c, saved.id).unwrap()).unwrap(),
            b"sk-secret"
        );
    }

    #[test]
    fn empty_key_on_edit_keeps_the_old_one() {
        let c = conn();
        let first = save_model(&c, &input(0, Some("sk-a"))).unwrap();
        let before = stored_key(&c, first.id).unwrap();

        let kept = save_model(&c, &input(first.id, None)).unwrap();
        assert!(kept.has_key);
        assert_eq!(stored_key(&c, first.id).unwrap(), before);

        let replaced = save_model(&c, &input(first.id, Some("sk-b"))).unwrap();
        assert!(replaced.has_key);
        assert_ne!(stored_key(&c, first.id).unwrap(), before);
        assert_eq!(
            dpapi::unprotect(&stored_key(&c, first.id).unwrap()).unwrap(),
            b"sk-b"
        );
    }

    #[test]
    fn rejects_blank_or_schemeless_fields() {
        let c = conn();
        assert!(save_model(&c, &input(0, None)).is_ok());

        let mut no_name = input(0, None);
        no_name.name = "  ".into();
        assert!(save_model(&c, &no_name).is_err());

        let mut no_scheme = input(0, None);
        no_scheme.base_url = "gw.local/v1".into();
        assert!(save_model(&c, &no_scheme).is_err());
    }

    #[test]
    fn delete_removes_only_that_row() {
        let c = conn();
        let a = save_model(&c, &input(0, Some("k1"))).unwrap();
        let mut b = input(0, None);
        b.name = "另一家".into();
        let b = save_model(&c, &b).unwrap();

        delete_model(&c, a.id).unwrap();
        let left = get_models(&c).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, b.id);
        assert!(!left[0].has_key);
    }

    #[test]
    fn the_test_target_falls_back_to_the_stored_row() {
        let c = conn();
        let saved = save_model(&c, &input(0, Some("sk-stored"))).unwrap();

        // 只给 id：地址、密钥、模型名都从库里取
        let t = call_target(&c, Some(saved.id), "", None, None).unwrap();
        assert_eq!((t.base.as_str(), t.model.as_str()), ("https://gw.local/fy29/v1", "fy29-chat"));
        assert_eq!(t.key.as_deref(), Some("sk-stored"));

        // 表单里改了地址和模型、密钥留空 → 新地址新模型 + 旧密钥
        let t = call_target(&c, Some(saved.id), "https://other/v1/", None, Some("fy29-mini")).unwrap();
        assert_eq!(t.base, "https://other/v1");
        assert_eq!(t.model, "fy29-mini");
        assert_eq!(t.key.as_deref(), Some("sk-stored"));

        // 没地址又没 id，问都不用问接口
        assert!(call_target(&c, None, "", Some("k"), Some("m")).is_err());
        // 模型名两处都没有 → 直接说，不发请求
        assert!(call_target(&c, None, "http://x/v1", Some("k"), None).is_err());
    }

    #[test]
    fn listing_needs_no_model_name_yet() {
        // 新增表单里模型名还空着也要能拉列表（下拉框就是用来填它的）
        let c = conn();
        let t = list_target(&c, None, "http://127.0.0.1:9/v1", Some("k")).unwrap();
        assert!(t.model.is_empty());
        assert_eq!(t.base, "http://127.0.0.1:9/v1");
    }

    /// 用户填的是 `http://host/v1`，而 rig 方言里把路径写成 `/v1/models`——拼法不对就会打成
    /// `/v1/v1/models`。这条断言真正发出去的请求行，不靠猜。
    #[tokio::test]
    async fn requests_use_the_paths_an_openai_gateway_expects() {
        let models = r#"{"data":[{"id":"fy29-chat"}]}"#;
        let chat = r#"{"choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"好"}}]}"#;
        let (port, server) = two_shot_server(models, chat);
        let target = ChatTarget {
            base: format!("http://127.0.0.1:{port}/v1"),
            key: Some("sk-x".into()),
            model: "fy29-chat".into(),
        };

        assert_eq!(fetch_models(&target).await.unwrap(), ["fy29-chat"]);
        assert!(probe(&target).await.unwrap().ok);

        let lines = server.join().unwrap();
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(
            lines[0].starts_with("GET /v1/models"),
            "模型列表打错了路径：{}",
            lines[0]
        );
        assert!(
            lines[1].starts_with("POST /v1/chat/completions"),
            "补全打错了路径：{}",
            lines[1]
        );
    }

    /// 网关挂在子路径下（`https://gw/fy29/v1`）时，前缀必须跟着走。
    /// 绝对路径的 URL 拼接会把它吞掉，这条就是钉住这点。
    #[tokio::test]
    async fn a_gateway_mounted_under_a_prefix_keeps_its_prefix() {
        let models = r#"{"data":[{"id":"fy29-chat"}]}"#;
        let chat = r#"{"choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"好"}}]}"#;
        let (port, server) = two_shot_server(models, chat);
        let target = ChatTarget {
            base: format!("http://127.0.0.1:{port}/fy29/v1"),
            key: Some("sk-x".into()),
            model: "fy29-chat".into(),
        };

        assert_eq!(fetch_models(&target).await.unwrap(), ["fy29-chat"]);
        assert!(probe(&target).await.unwrap().ok);

        let lines = server.join().unwrap();
        assert_eq!(lines[0], "GET /fy29/v1/models HTTP/1.1", "子路径被吞了：{lines:?}");
        assert_eq!(
            lines[1],
            "POST /fy29/v1/chat/completions HTTP/1.1",
            "子路径被吞了：{lines:?}"
        );
    }

    /// 两次连接：第一次回模型列表，第二次回补全，各自交出收到的请求行
    fn two_shot_server(models_body: &str, chat_body: &str) -> (u16, std::thread::JoinHandle<Vec<String>>) {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let models = models_body.to_string();
        let chat = chat_body.to_string();
        let handle = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for body in [models, chat] {
                let Ok((mut sock, _)) = listener.accept() else { break };
                sock.set_read_timeout(Some(std::time::Duration::from_millis(400))).unwrap();
                let mut raw: Vec<u8> = Vec::new();
                let mut buf = [0u8; 4096];
                while let Ok(n) = sock.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    raw.extend_from_slice(&buf[..n]);
                }
                let line = String::from_utf8_lossy(&raw)
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_string();
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(reply.as_bytes());
                let _ = sock.flush();
                seen.push(line);
            }
            seen
        });
        (port, handle)
    }

    #[test]
    fn agent_falls_back_to_the_first_configured_model() {
        let c = conn();
        assert!(target_for(&c, None).is_err());
        assert!(first_model_id(&c).is_err());

        let first = save_model(&c, &input(0, Some("sk-first"))).unwrap();
        let mut second = input(0, None);
        second.name = "第二个".into();
        second.sort = Some(5);
        let second = save_model(&c, &second).unwrap();

        // 没指定、传 0（新增态）、或者那个模型已被删，都用列表第一个
        for id in [None, Some(0), Some(999)] {
            let t = target_for(&c, id).unwrap();
            assert_eq!(
                (t.base.as_str(), t.model.as_str()),
                ("https://gw.local/fy29/v1", "fy29-chat")
            );
        }
        let chosen = target_for(&c, Some(second.id)).unwrap();
        assert!(chosen.key.is_none(), "没填密钥的那条不该有密钥");

        // 密钥能取到，但不能出现在 Debug 里
        let named = target_for(&c, Some(first.id)).unwrap();
        assert_eq!(named.key.as_deref(), Some("sk-first"));
        assert!(!format!("{named:?}").contains("sk-first"), "{named:?}");
    }

    /// 点一次「获取模型」拉回来的列表要留在本机，别再让人家重打网络
    #[test]
    fn the_fetched_list_survives_a_restart() {
        let c = conn();
        let saved = save_model(&c, &input(0, Some("sk-1"))).unwrap();
        assert!(
            saved.remote_models.is_empty() && saved.remote_at.is_none(),
            "新建的配置不该带缓存"
        );

        remember_models(&c, saved.id, &["fy29-chat".into(), "glm-x".into()]).unwrap();

        let again = get_models(&c).unwrap().remove(0);
        assert_eq!(again.remote_models, vec!["fy29-chat".to_string(), "glm-x".to_string()]);
        assert!(again.remote_at.unwrap() > 0, "没记下是哪一次获取的");
    }

    #[test]
    fn a_different_endpoint_drops_the_stale_list() {
        let c = conn();
        let saved = save_model(&c, &input(0, Some("sk-1"))).unwrap();
        remember_models(&c, saved.id, &["a".into()]).unwrap();

        // 只改名字：列表还是这个接口给的，留着
        let mut renamed = input(saved.id, None);
        renamed.name = "换个叫法".into();
        assert_eq!(save_model(&c, &renamed).unwrap().remote_models.len(), 1);

        // 换接口地址：再留着就是在骗人
        let mut moved = input(saved.id, None);
        moved.base_url = "https://other.local/v1".into();
        assert!(save_model(&c, &moved).unwrap().remote_models.is_empty());
    }

    #[test]
    fn deleting_a_model_also_drops_its_cached_list() {
        let c = conn();
        let saved = save_model(&c, &input(0, Some("sk-1"))).unwrap();
        remember_models(&c, saved.id, &["a".into()]).unwrap();

        delete_model(&c, saved.id).unwrap();
        let left: i64 = c
            .query_row("SELECT COUNT(*) FROM settings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0, "配置删了，那份列表还占在 settings 里");
    }
