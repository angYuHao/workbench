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
///
/// 但键只认路径有个洞：程序就地更新后 `chrome.exe` 还是那个路径，图标早换了一张，
/// 而缓存里那张旧 PNG 会永远命中 —— 这就是「软件更新后图标不换」。
/// 所以命中缓存之前先问一句「源比这张图新吗」（[stale]），新就把同名文件重抽覆盖：
/// 卡片存的路径不变，下次加载自然就是新图，不用同步、不用改库、也不留孤儿文件。
pub(crate) fn extract_icon(icon_dir: &Path, candidates: &[&str]) -> Option<String> {
    let mut tried: Vec<&str> = Vec::new();
    // 全部候选都抽失败时，至少把还在的旧图给出去：旧图比字母块像话
    let mut fallback: Option<String> = None;
    for target in candidates.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if tried.contains(&target) {
            continue;
        }
        tried.push(target);
        if !Path::new(target).exists() {
            continue;
        }
        let stem = fnv1a(target.as_bytes());
        let out = icon_dir.join(format!("{stem:x}.png"));
        let png = out.to_string_lossy().to_string();
        if out.exists() {
            fallback.get_or_insert_with(|| png.clone());
            if !stale(candidates, &out) {
                return Some(png);
            }
        }
        // 先落到旁边，成功了再原子换过去：抽失败不该把还能用的旧图一起删掉
        let tmp = icon_dir.join(format!("{stem:x}.redraw.png"));
        if crate::win::icon::extract_icon_to_png(target, &tmp.to_string_lossy()).is_ok() {
            let _ = std::fs::rename(&tmp, &out);
            return Some(png);
        }
        let _ = std::fs::remove_file(&tmp);
    }
    fallback
}

/// 这一组候选里任何一个源文件比这张 PNG 新，就该重抽。
/// 用「任何一个」是因为真正变的往往是 exe，而缓存键是那个没动的 .lnk。
/// 取不到时间就当不用重抽：一次 stat 失败不该变成每次调用都重抽一遍。
fn stale(candidates: &[&str], png: &Path) -> bool {
    let drawn = png.metadata().and_then(|m| m.modified()).ok();
    candidates.iter().any(|src| {
        redraw(
            std::fs::metadata(src).and_then(|m| m.modified()).ok(),
            drawn,
        )
    })
}

/// 判「该不该重画」的那一步单独拆出来：文件时间没法在测试里可靠地摆前后，
/// 那就只测这个纯比较，取时间的那部分留在上面。
fn redraw(when: Option<std::time::SystemTime>, drawn: Option<std::time::SystemTime>) -> bool {
    matches!((when, drawn), (Some(a), Some(b)) if a > b)
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
pub(crate) fn fnv1a(data: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for b in data {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::{redraw, stale};
    use std::time::{Duration, SystemTime};

    fn at(secs: u64) -> Option<SystemTime> {
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
    }

    #[test]
    fn only_a_newer_source_asks_for_a_redraw() {
        assert!(redraw(at(200), at(100)), "源比图新就该重抽");
        assert!(!redraw(at(100), at(100)), "同一刻不算更新，否则每次调用都重抽");
        assert!(!redraw(at(50), at(100)), "源更旧不该重抽");
    }

    #[test]
    fn missing_time_info_never_asks_for_a_redraw() {
        // 读不到时间就当不用重画：一次 stat 失败不该变成每帧重抽一遍
        assert!(!redraw(None, at(100)));
        assert!(!redraw(at(200), None));
        assert!(!redraw(None, None));
    }

    #[test]
    fn a_cache_just_written_is_not_stale() {
        let dir = std::env::temp_dir().join(format!("wb-icons-{:x}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("app.exe");
        let png = dir.join("cache.png");
        // 源先落盘、图后落盘：图不比源旧，就不该被判过期
        std::fs::write(&src, b"exe").unwrap();
        std::fs::write(&png, b"png").unwrap();
        let names = [src.to_string_lossy().to_string()];
        assert!(!stale(
            &names.iter().map(String::as_str).collect::<Vec<_>>(),
            &png
        ));

        // 源都不在了也不该重抽（拿不到时间就是「维持现状」）
        assert!(!stale(&["Z:\\没有这个文件.exe"], &png));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
