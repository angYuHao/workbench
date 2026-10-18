use tauri::State;

use crate::db::AppState;
use crate::models::{TotpAccount, TotpAccountInput};
use crate::services::totp;
use crate::services::totp_backup::{self, ImportReport};

#[tauri::command]
pub fn get_totp_accounts(state: State<AppState>) -> Result<Vec<TotpAccount>, String> {
    let conn = state.conn();
    totp::get_accounts(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_totp_account(state: State<AppState>, account: TotpAccountInput) -> Result<TotpAccount, String> {
    let conn = state.conn();
    totp::save_account(&conn, &account).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_totp_account(state: State<AppState>, id: i64) -> Result<(), String> {
    let conn = state.conn();
    totp::delete_account(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn totp_code(state: State<AppState>, id: i64) -> Result<String, String> {
    let conn = state.conn();
    totp::now_code(&conn, id).map_err(|e| e.to_string())
}

/// 查看/备份密钥，只在用户主动点开时才解密
#[tauri::command]
pub fn totp_secret(state: State<AppState>, id: i64) -> Result<String, String> {
    let conn = state.conn();
    totp::reveal_secret(&conn, id).map_err(|e| e.to_string())
}

/// 单次导入接受的备份上限，正常几十个账户远用不到；主要是防止拿一个超大文件来卡住派生。
const MAX_BACKUP_BYTES: u64 = 4 * 1024 * 1024;

/// 用口令把全部 2FA 账户加密写入 dest，返回条数。
/// 异步是为了让 PBKDF2 那一两亿次哈希跑在工作线程上，不冻住主线程。
#[tauri::command]
pub async fn export_totp_backup(
    state: State<'_, AppState>,
    dest: String,
    passphrase: String,
) -> Result<usize, String> {
    let (text, count) = {
        let conn = state.conn();
        totp_backup::export(&conn, &passphrase).map_err(|e| e.to_string())?
    };
    std::fs::write(&dest, text).map_err(|e| format!("写入 {dest} 失败: {e}"))?;
    Ok(count)
}

/// 从 src 读备份、用口令解密并导入，返回新增/跳过条数。
#[tauri::command]
pub async fn import_totp_backup(
    state: State<'_, AppState>,
    src: String,
    passphrase: String,
) -> Result<ImportReport, String> {
    let size = std::fs::metadata(&src).map_err(|e| e.to_string())?.len();
    if size > MAX_BACKUP_BYTES {
        return Err(format!("备份文件过大（{size} 字节），已拒绝"));
    }
    let text = std::fs::read_to_string(&src).map_err(|e| format!("读取 {src} 失败: {e}"))?;
    let conn = state.conn();
    totp_backup::import(&conn, &text, &passphrase).map_err(|e| e.to_string())
}
