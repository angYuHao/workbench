//! 当前 Windows 用户的 DPAPI 加解密：模型密钥和 2FA 密钥的密文都靠它。
//! 换机器、换用户都解不开，所以这些密文只在原机上有效（能跨机迁移的只有口令备份那份）。
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
};

use crate::error::{AppError, AppResult};

fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    }
}

fn empty_blob() -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    }
}

/// 取回 Crypt*Data 写到堆上的结果，复制后立即 LocalFree。
unsafe fn consume(out: &CRYPT_INTEGER_BLOB) -> Vec<u8> {
    if out.pbData.is_null() {
        return Vec::new();
    }
    let slice = std::slice::from_raw_parts(out.pbData, out.cbData as usize);
    let copied = slice.to_vec();
    let _ = LocalFree(HLOCAL(out.pbData as *mut _));
    copied
}

/// 用当前 Windows 用户的凭据加密。密文只能在本机本用户解开。
pub fn protect(data: &[u8]) -> AppResult<Vec<u8>> {
    let desc: Vec<u16> = "workbench-totp"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut out = empty_blob();
    unsafe {
        let in_blob = blob(data);
        CryptProtectData(
            &in_blob,
            PCWSTR(desc.as_ptr()),
            None,
            None,
            None,
            0,
            &mut out,
        )
        .map_err(|e| AppError::msg(format!("加密密钥失败: {e}")))?;
        Ok(consume(&out))
    }
}

/// 解密 protect 产生的密文。换用户/换机器后失败属正常，调用方给出重新添加的提示。
pub fn unprotect(data: &[u8]) -> AppResult<Vec<u8>> {
    let mut out = empty_blob();
    unsafe {
        let in_blob = blob(data);
        CryptUnprotectData(
            &in_blob,
            Option::<*mut PWSTR>::None,
            None,
            None,
            None,
            0,
            &mut out,
        )
        .map_err(|_| AppError::msg("密钥无法解密，可能数据库来自其他机器或用户，请重新添加该账号"))?;
        Ok(consume(&out))
    }
}
