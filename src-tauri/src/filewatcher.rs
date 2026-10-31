use std::collections::HashMap;
use std::path::PathBuf;

use notify::{recommended_watcher, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter};

use crate::error::AppResult;

/// 文件页那几列目录的监视器。
///
/// 每次 `update` 只动增减的那几条：早先每来一次清单就把 watcher 整个拆了重建，
/// 标签一多，光重建就够卡一下。目录本身也只盯第一层 —— 列里摆的是直接子项，
/// 深处再变也不影响这一屏。
pub struct FileWatcher {
    app: AppHandle,
    inner: Option<RecommendedWatcher>,
    /// 小写路径 → 实际挂上去的路径。Windows 的路径大小写不敏感，同一个目录两种写法要认成一条
    watched: HashMap<String, PathBuf>,
}

impl FileWatcher {
    pub fn new(app: AppHandle, folders: &[String]) -> AppResult<Self> {
        let mut fw = Self { app, inner: None, watched: HashMap::new() };
        fw.update(folders);
        Ok(fw)
    }

    fn ensure_watcher(&mut self) {
        if self.inner.is_some() {
            return;
        }
        let app = self.app.clone();
        self.inner = recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            let Ok(event) = res else { return };
            // 光是读一下文件（杀毒、索引）不算变化，别让界面跟着重扫
            if matches!(event.kind, notify::EventKind::Access(_)) {
                return;
            }
            let _ = app.emit("files-changed", ());
        })
        .ok();
    }

    pub fn update(&mut self, folders: &[String]) {
        let wanted: HashMap<String, PathBuf> = folders
            .iter()
            .filter(|f| !f.is_empty())
            .map(|f| (f.to_lowercase(), PathBuf::from(f)))
            .collect();
        self.ensure_watcher();
        let FileWatcher { inner, watched, .. } = self;
        let Some(watcher) = inner.as_mut() else { return };
        watched.retain(|key, path| {
            if wanted.contains_key(key) {
                return true;
            }
            let _ = watcher.unwatch(path);
            false
        });
        for (key, path) in wanted {
            if watched.contains_key(&key) {
                continue;
            }
            if watcher.watch(&path, RecursiveMode::NonRecursive).is_ok() {
                watched.insert(key, path);
            }
        }
    }
}
