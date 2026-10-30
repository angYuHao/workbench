//! 手写 Win32 之外的那层 shell 动作：把文件扔进回收站。
//!
//! 系统给回收站留的是 COM 的 `IFileOperation`（要自己实现 sink 接口、处理 COM 线程模型），
//! 比这一页需要的多一点太多；这里改用 `Microsoft.VisualBasic.FileIO.FileSystem` 的删除重载，
//! 它内部走的就是同一个 SHFileOperation。代价是要起一个 powershell 进程（一次删一批，不按条起），
//! 以及机器上用组策略锁死 PowerShell 时会失败——那种情况界面得能改走「彻底删除」。
//!
//! 脚本用 `-EncodedCommand` 传（Base64 的 UTF-16LE）：路径里什么引号、反引号、中文都有，
//! 走命令行参数拼装太容易出错，整段编码过去最稳。
use std::os::windows::process::CommandExt;
use std::process::Command;

use crate::error::{AppError, AppResult};

/// CREATE_NO_WINDOW：删文件不该闪出一个黑框
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// UTF-16LE 再 Base64。windows 绑定那边没有现成的 base64，`-EncodedCommand` 只认这一种写法
fn encoded_command(script: &str) -> String {
    let units: Vec<u8> = script
        .encode_utf16()
        .flat_map(|u| [u as u8, (u >> 8) as u8])
        .collect();
    let mut out = String::with_capacity(units.len().div_ceil(3) * 4);
    for chunk in units.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(char::from(B64[((n >> 18) & 63) as usize]));
        out.push(char::from(B64[((n >> 12) & 63) as usize]));
        if chunk.len() > 1 {
            out.push(char::from(B64[((n >> 6) & 63) as usize]));
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(char::from(B64[(n & 63) as usize]));
        } else {
            out.push('=');
        }
    }
    out
}

/// 单引号字符串字面量，PowerShell 的规则是把 `'` 翻倍
fn quote(path: &str) -> String {
    format!("'{}'", path.replace('\'', "''"))
}

/// 把若干路径送进回收站。空列表直接算成功（不起了进程空转一次）。
/// 返回的 Err 是 powershell 的 stderr 摘要，界面把它原样说给用户，同时提示可以改走彻底删除。
pub fn to_recycle_bin(paths: &[String]) -> AppResult<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let items = paths.iter().map(|p| quote(p)).collect::<Vec<_>>().join(",");
    let script = format!(
        "$ErrorActionPreference='Stop';Add-Type -AssemblyName Microsoft.VisualBasic;$items=@({items});\
foreach($i in $items){{if(Test-Path -LiteralPath $i -PathType Container){{[Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory($i,'OnlyErrorDialogs','SendToRecycleBin')}}else{{[Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile($i,'OnlyErrorDialogs','SendToRecycleBin')}}}}"
    );
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded_command(&script)])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| AppError::msg(format!("起 powershell 失败：{e}")))?;
    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(AppError::msg(format!(
        "送不进回收站{detail}（这台机器可能锁了 PowerShell，可以改用「彻底删除」）"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `-EncodedCommand` 要的就是 Base64(UTF-16LE)。这三条期望值是拿本机 PowerShell 的
    /// `[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes(...))` 对拍出来的，不是手算的
    #[test]
    fn encoded_command_matches_the_powershell_encoding() {
        assert_eq!(encoded_command("A"), "QQA=");
        assert_eq!(
            encoded_command("Get-Process"),
            "RwBlAHQALQBQAHIAbwBjAGUAcwBzAA=="
        );
        // 中文路径：UTF-16 单元不是 ASCII，也不能塌成问号（桌 = U+684C）
        assert_eq!(encoded_command("桌"), "TGg=");
    }

    #[test]
    fn single_quotes_in_a_path_are_doubled() {
        assert_eq!(quote("C:\\a\\it's.txt"), "'C:\\a\\it''s.txt'");
    }
}
