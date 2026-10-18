use std::path::Path;

use notify::{recommended_watcher, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter};

use crate::error::AppResult;

pub struct FileWatcher {
    app: AppHandle,
    inner: Option<RecommendedWatcher>,
}

impl FileWatcher {
    pub fn new(app: AppHandle, folders: &[String]) -> AppResult<Self> {
        let mut fw = Self { app, inner: None };
        fw.update(folders);
        Ok(fw)
    }

    pub fn update(&mut self, folders: &[String]) {
        let app = self.app.clone();
        let mut watcher = recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            if res.is_ok() {
                let _ = app.emit("files-changed", ());
            }
        })
        .ok();
        if let Some(w) = watcher.as_mut() {
            for f in folders {
                let _ = w.watch(Path::new(f), RecursiveMode::Recursive);
            }
        }
        self.inner = watcher;
    }
}