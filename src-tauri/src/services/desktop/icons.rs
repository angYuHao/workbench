//! 图标缓存与路径写法。这几件小事被 `services::installed` 和扫描共用，
//! 所以放在 desktop 这一层的最底下。

use std::path::Path;

use crate::error::AppResult;

pub(crate) fn get_icon_dir() -> AppResult<std::path::PathBuf> {
    Ok(crate::paths::app_dir()?.join("icons"))
}

/// 抽图标要按「桌面上看到的那张」为准，所以候选顺序是：**快捷方式自己 → 它解析出的目标**。
/// 顺序反了就会踩 Ollama 那种坑：快捷方式用 IconLocation 指着 app.ico，而 `ollama app.exe`
/// 自己压根没内嵌图标，抽解析目标只能拿到 Windows 的白纸占位图。
/// 走 .lnk 时 shell 已经把 IconLocation（含指向 dll 第 N 个图标的写法）算好了，不用我们再解。
/// 缓存键按「真正用的那个写法」算，换写法就是换键，不会串图。
pub(crate) fn extract_icon(icon_dir: &Path, candidates: &[&str]) -> Option<String> {
    let mut tried: Vec<&str> = Vec::new();
    for target in candidates.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if tried.contains(&target) {
            continue;
        }
        tried.push(target);
        if !Path::new(target).exists() {
            continue;
        }
        let out = icon_dir.join(format!("{:x}.png", fnv1a(target.as_bytes())));
        let png = out.to_string_lossy().to_string();
        if out.exists() {
            return Some(png);
        }
        if crate::win::icon::extract_icon_to_png(target, &out.to_string_lossy()).is_ok() {
            return Some(png);
        }
        // 抽失败可能留下半截文件：不删掉的话，下次一看「文件在」就当成功了
        let _ = std::fs::remove_file(&out);
    }
    None
}

/// 小写扩展名（不含点），无扩展名时为空串
pub(crate) fn extension_of(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// .lnk 解析为真实启动目标；解析失败或非快捷方式时就是文件自身路径
pub(crate) fn launch_target(path: &Path) -> String {
    let own = path.to_string_lossy().to_string();
    if extension_of(path) == "lnk" {
        crate::win::lnk::resolve_lnk(&own).unwrap_or(own)
    } else {
        own
    }
}

/// 规则里逗号分隔的字段，逐项去空白并转小写
fn fnv1a(data: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for b in data {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}
