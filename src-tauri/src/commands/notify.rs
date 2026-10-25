use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize)]
struct Changed {
    domain: String,
    /// 谁发的。广播会回到发送方自己那个窗口，而发送方的界面本来就已经就地刷过了，
    /// 再收一次只会白闪一下 loading —— 所以带上来源，订阅端据此跳过自己。
    from: String,
}

/// 前端写完一笔数据之后的广播：只带域名，不带数据。
///
/// 为什么传域名而不是传数据：每个窗口都有一份自己的 store 和同一个库，收到「这块脏了」
/// 之后自己重读就行。这样丢一次广播不会让谁看到错的数据，只是晚一步——下一次写入或
/// 重新开窗都会纠正它。跨窗口同步因此不需要序号、不需要重放、不需要断线补偿。
///
/// 磁盘侧的变更走 filewatcher.rs 里那条 `files-changed`，两者互不替代：
/// 那条是「文件被外部改了」，这条是「某个界面刚写了库」。
#[tauri::command]
pub fn broadcast_change(app: AppHandle, domain: String, from: String) {
    let _ = app.emit("data-changed", Changed { domain, from });
}
