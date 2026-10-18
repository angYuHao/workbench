//! AI 调用日志：每次真发出去的请求都在文件里留两行，出问题时不用靠猜。
//! 位置 `%APPDATA%\workbench\ai.log`；攒到 660 行就剪回最近 600 行（一条记录固定三行，
//! 也就是留最近 200 次调用），应用启动时再剪一次，所以它不会一直涨。
//! 设置里只读最近 100 行，不用打开整个文件。
//! 密钥不进日志：只记地址、模型名、状态码、耗时和请求/响应的开头。

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use crate::error::{AppError, AppResult};
use crate::models::LogView;

/** 行数到这个数才动手剪，避免刚剪完下一条又来剪一遍 */
const TRIM_AT_LINES: usize = 660;
/** 动手就剪到只剩这么多行 */
const KEEP_LINES: usize = 600;
/** 设置里显示的行数 */
pub const VIEW_LINES: usize = 100;
const BODY_CHARS: usize = 2000;
const REQUEST_CHARS: usize = 160;

/// 一次调用的记录。`status` 为 0 表示框架没交出状态码（rig 就是这样），
/// `response` 这时放错误原文。
pub struct Call<'a> {
    pub tag: &'a str,
    pub url: &'a str,
    pub model: &'a str,
    pub status: u16,
    pub elapsed_ms: u128,
    pub ok: bool,
    pub request: &'a str,
    pub response: &'a str,
}

pub fn path() -> PathBuf {
    // 测试也走这条代码路径，但不能把假记录写进用户真实的日志文件里
    #[cfg(test)]
    {
        std::env::temp_dir().join("workbench-ai-test.log")
    }
    #[cfg(not(test))]
    {
        dirs::data_dir()
            .map(|d| d.join("workbench").join("ai.log"))
            .unwrap_or_else(|| PathBuf::from("ai.log"))
    }
}

/// 日志的读写要排队。`record` 是从好几个 async 命令里并发调的，一边整文件重写一边追加，
/// Windows 上会撞共享冲突，而失败是被 `let _ =` 吞掉的 —— 那就等于从此不再裁剪了。
static LOG_LOCK: Mutex<()> = Mutex::new(());

/// 中毒也照用：这里的 panic 只会来自日志本身，没必要因此把日志功能一起锁死
fn log_guard() -> MutexGuard<'static, ()> {
    LOG_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn record(call: &Call) {
    let entry = format_entry(call);
    let _guard = log_guard();
    trim_locked();
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path()) {
        let _ = file.write_all(entry.as_bytes());
        let _ = file.flush();
    }
}

/// 一行结论 + 请求开头 + 响应开头，正文都压成一行，一条记录永远占三行
fn format_entry(c: &Call) -> String {
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let status = if c.status == 0 {
        // rig 不交出状态码；真连不上时下面的错误原文会写清楚，这里别谎报
        "无状态码".to_string()
    } else {
        format!("HTTP {}", c.status)
    };
    format!(
        "[{}] {}  {}  {}  model={}  {}  {}ms\n    ⇒ {}\n    ⇐ {}\n",
        stamp,
        if c.ok { "成功" } else { "失败" },
        c.tag,
        c.url,
        c.model,
        status,
        c.elapsed_ms,
        flat(&clip(c.request, REQUEST_CHARS)),
        flat(&clip(c.response, BODY_CHARS)),
    )
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut cut: String = s.chars().take(max).collect();
    cut.push('…');
    cut
}

fn flat(s: &str) -> String {
    s.replace("\r\n", " ").replace(['\n', '\r'], " ")
}

/// 剪掉更早的记录。启动时来一次，之后每次写日志前也来一次，所以不需要额外的定时器。
pub fn cleanup() {
    let _guard = log_guard();
    trim_locked();
}

fn trim_locked() {
    let Ok(text) = fs::read_to_string(path()) else { return };
    if let Some(next) = trimmed(&text) {
        let _ = fs::write(path(), next);
    }
}

/// 要不要动手、动手后文件该长成什么样：攒到 TRIM_AT_LINES 行才剪，一次剪回 KEEP_LINES 行，
/// 省得刚清完下一条又来重写整个文件。纯函数，不碰文件才好测。
fn trimmed(text: &str) -> Option<String> {
    if text.lines().count() <= TRIM_AT_LINES {
        return None;
    }
    let mut out = format!("[更早的记录已清掉，只留最近 {} 行]\n\n", KEEP_LINES);
    for line in kept_tail(text, KEEP_LINES) {
        out.push_str(line);
        out.push('\n');
    }
    Some(out)
}

/// 给设置界面看：最近 `limit` 行、文件路径、文件一共多少行
pub fn tail(limit: usize) -> AppResult<LogView> {
    // 读也要排队：正被整文件重写时读到半截，界面就演出一段看不懂的记录
    let _guard = log_guard();
    let file = path();
    let text = if file.exists() {
        Some(fs::read_to_string(&file).map_err(|e| AppError::msg(format!("读日志失败：{e}")))?)
    } else {
        None
    };
    Ok(assemble(&file, text.as_deref(), limit))
}

/// 把文件正文摊成界面要看的样子。纯函数：没有文件也先回路径，界面好在原地解释
fn assemble(file: &std::path::Path, text: Option<&str>, limit: usize) -> LogView {
    LogView {
        path: file.display().to_string(),
        lines: text
            .map(|t| kept_tail(t, limit).iter().map(|s| s.to_string()).collect())
            .unwrap_or_default(),
        total: text.unwrap_or("").lines().count(),
    }
}

/// 取最后 `max` 行，但一定从一个整条记录的开头切：正文那两行不带方括号，
/// 从中间切最上面就是半条看不懂的记录。
fn kept_tail(text: &str, max: usize) -> Vec<&str> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= max {
        return lines;
    }
    let hard = lines.len() - max;
    let mut start = hard;
    while start < lines.len() && !lines[start].starts_with('[') {
        start += 1;
    }
    // 一行开头都没找到（文件被别的程序改过）就退回按行数硬切，别把日志剪成空的
    lines[if start == lines.len() { hard } else { start }..].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call<'a>(response: &'a str, request: &'a str) -> Call<'a> {
        Call {
            tag: "AI分类 20条",
            url: "http://127.0.0.1:8326/v1/chat/completions",
            model: "codearts/glm-5.3-flash",
            status: 500,
            elapsed_ms: 30012,
            ok: false,
            request,
            response,
        }
    }

    #[test]
    fn entry_says_what_failed_in_one_glance() {
        let e = format_entry(&call(
            "{\"error\":{\"code\":\"internal\",\"message\":\"内部错误\"}}",
            "{\"model\":\"codearts/glm-5.3-flash\",\"max_tokens\":900,\"temperature\":0}",
        ));
        let lines: Vec<&str> = e.trim_end().lines().collect();
        assert_eq!(lines.len(), 3, "{e}");
        assert!(lines[0].contains("失败"), "{e}");
        assert!(lines[0].contains("AI分类 20条"), "{e}");
        assert!(lines[0].contains("HTTP 500"), "{e}");
        assert!(lines[0].contains("30012ms"), "{e}");
        assert!(lines[0].contains("/v1/chat/completions"), "{e}");
        // 请求和响应分开两行，参数对不对、上游说了什么一眼能看见
        assert!(lines[1].contains("\"max_tokens\":900"), "{e}");
        assert!(lines[2].contains("内部错误"), "{e}");
        assert!(e.starts_with('['), "{e}");
    }

    #[test]
    fn multiline_bodies_still_leave_exactly_three_lines() {
        let e = format_entry(&call("第一行\n第二行\r\n第三行", "{}"));
        let lines: Vec<&str> = e.trim_end().lines().collect();
        assert_eq!(lines.len(), 3, "{e}");
        assert!(e.contains("第一行 第二行 第三行"), "{e}");
    }

    #[test]
    fn a_missing_status_says_so_instead_of_blaming_the_network() {
        let mut c = call("连接被拒绝", "{}");
        c.status = 0;
        let e = format_entry(&c);
        assert!(e.contains("无状态码"), "{e}");
        assert!(e.contains("失败"), "{e}");
    }

    #[test]
    fn a_long_request_head_is_cut_but_never_split_mid_char() {
        let long = format!("{{\"names\":[{})}}", vec!["\"微信\""; 200].join(","));
        let e = format_entry(&call("{}", &long));
        let request_line = e.lines().nth(1).unwrap_or_default();
        assert!(request_line.chars().count() < REQUEST_CHARS + 40, "没截住：{request_line}");
        assert!(request_line.ends_with('…'), "{request_line}");
    }

    #[test]
    fn a_recorded_call_appends_to_the_file_and_carries_no_key() {
        let file = path();
        let _ = fs::remove_file(&file);
        record(&call("{\"error\":{\"message\":\"内部错误\"}}", "{\"model\":\"codearts/glm-5.3-flash\"}"));
        record(&call("第二次也要留着", "{}"));

        // 只断言自己写进去的两条：同一进程里别的测试也在真调 post()，行数会互相夹进来
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.contains("AI分类 20条"), "{text}");
        assert!(text.contains("HTTP 500"), "{text}");
        assert!(text.contains("第二次也要留着"), "{text}");
        assert!(!text.contains("Bearer"), "请求头里的密钥绝不能进日志");
        let _ = fs::remove_file(&file);
    }

    #[test]
    fn trimming_starts_on_a_whole_record_and_says_what_it_dropped() {
        // 8 条记录 = 24 行；只要 10 行，那就从 10 行之内最靠后的那个记录头开始
        let text = records(8);
        let kept = kept_tail(&text, 10);
        assert!(kept.len() <= 10 && kept.len() >= 9, "{:?}", kept.len());
        assert!(kept[0].starts_with('['), "半条记录当开头，看着像坏了：{}", kept[0]);
        assert_eq!(*kept.last().unwrap(), "    ⇐ 回7");
    }

    #[test]
    fn a_short_log_is_left_completely_alone() {
        // 没到触发线就不该重写文件——不然每次发请求都要整写一遍几百 KB
        assert!(trimmed(&records(20)).is_none());
    }

    #[test]
    fn a_long_log_is_cut_back_to_the_cap_and_keeps_the_newest() {
        // 250 条 = 750 行，越过 660 的触发线
        let next = trimmed(&records(250)).expect("该动手了");
        let body: Vec<&str> = next.lines().skip(2).collect(); // 前两行是提示和空行
        assert!(next.starts_with("[更早的记录已清掉"), "{next}");
        assert!(body[0].starts_with('['), "剪完最上面还是半条：{}", body[0]);
        assert!(body.len() <= KEEP_LINES, "没剪到位：{}", body.len());
        assert!(next.contains("回249"), "最新那条必须留下");
        assert!(!next.contains("[t0]"), "最旧的该没了");
    }

    #[test]
    fn the_view_counts_the_whole_file_but_only_shows_the_tail() {
        let text = records(50); // 150 行
        let view = assemble(std::path::Path::new("ai.log"), Some(&text), 100);
        assert_eq!(view.total, 150, "总数要说得出文件有多大");
        assert!(view.lines.len() <= 100 && view.lines.len() >= 99, "{}", view.lines.len());
        assert!(view.lines[0].starts_with('['));
        // 还没发过请求：路径照给，行数是空的
        let fresh = assemble(std::path::Path::new("ai.log"), None, 100);
        assert_eq!((fresh.lines.len(), fresh.total), (0, 0));
    }

    /// n 条假记录，一条固定三行，末行带序号方便认
    fn records(n: usize) -> String {
        (0..n)
            .map(|i| format!("[t{i}] 成功  AI分类  http://x  model=m  HTTP 200  1ms\n    ⇒ 请{i}\n    ⇐ 回{i}\n"))
            .collect()
    }
}
