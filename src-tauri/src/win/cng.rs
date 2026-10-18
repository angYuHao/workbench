//! CNG（bcrypt.dll）封装：口令派生密钥 + AES-256-GCM + 系统随机数。
//! 用它而不是引 crate，是因为 crates.io 在这台机器上拉不动，而这些原语 Windows 自带。

use std::ffi::c_void;

use windows::core::PCWSTR;
use windows::Win32::Security::Cryptography::{
    BCryptCloseAlgorithmProvider, BCryptDecrypt, BCryptDeriveKeyPBKDF2, BCryptDestroyKey,
    BCryptEncrypt, BCryptGenRandom, BCryptGenerateSymmetricKey, BCryptOpenAlgorithmProvider,
    BCryptSetProperty, BCRYPT_ALG_HANDLE, BCRYPT_ALG_HANDLE_HMAC_FLAG, BCRYPT_AES_ALGORITHM,
    BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO, BCRYPT_CHAINING_MODE, BCRYPT_CHAIN_MODE_GCM, BCRYPT_FLAGS,
    BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS, BCRYPTGENRANDOM_FLAGS, BCRYPT_KEY_HANDLE,
    BCRYPT_RNG_ALGORITHM, BCRYPT_SHA256_ALGORITHM,
};

use crate::error::{AppError, AppResult};

pub const KEY_LEN: usize = 32;
pub const NONCE_LEN: usize = 12;
pub const TAG_LEN: usize = 16;

/// BCryptOpenAlgorithmProvider 的 RAII 包装：句柄必须先于密钥释放，且 `?` 提前返回也得关掉。
struct Alg(BCRYPT_ALG_HANDLE);

impl Drop for Alg {
    fn drop(&mut self) {
        unsafe {
            let _ = BCryptCloseAlgorithmProvider(self.0, 0);
        }
    }
}

struct Key(BCRYPT_KEY_HANDLE);

impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = BCryptDestroyKey(self.0);
        }
    }
}

fn check(status: windows::Win32::Foundation::NTSTATUS, what: &str) -> AppResult<()> {
    status
        .ok()
        .map_err(|e| AppError::msg(format!("{what}失败: {e}")))
}

/// BCryptSetProperty 的输入要「含结尾 0 的 UTF-16 原始字节」。windows-core 0.58 的 PCWSTR
/// 没有 as_bytes()，所以按常量自带的指针把它展开，避免手抄字面量造成漂移。
fn wide_of(s: PCWSTR) -> Vec<u16> {
    let mut out = Vec::new();
    unsafe {
        let mut p = s.0;
        while let Some(&unit) = p.as_ref() {
            if unit == 0 {
                out.push(0);
                break;
            }
            out.push(unit);
            p = p.add(1);
        }
    }
    out
}

fn u16_bytes(v: &[u16]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 2) }
}

fn open_alg(id: PCWSTR, flags: BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS) -> AppResult<Alg> {
    let mut handle = BCRYPT_ALG_HANDLE::default();
    unsafe {
        check(
            BCryptOpenAlgorithmProvider(&mut handle, id, None, flags),
            "打开算法提供程序",
        )?;
    }
    Ok(Alg(handle))
}

/// 用系统 CSPRNG 填充。口令相关的盐值和 nonce 都从这里取。
pub fn random_bytes(n: usize) -> AppResult<Vec<u8>> {
    let rng = open_alg(BCRYPT_RNG_ALGORITHM, BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS(0))?;
    let mut buf = vec![0u8; n];
    unsafe {
        check(
            BCryptGenRandom(rng.0, &mut buf, BCRYPTGENRANDOM_FLAGS(0)),
            "生成随机数",
        )?;
    }
    Ok(buf)
}

/// PBKDF2-HMAC-SHA256。CNG 会把 1~999 次迭代抬到 1000，所以调用方给的迭代数应当远高于它。
pub fn pbkdf2_sha256(password: &[u8], salt: &[u8], iterations: u64, out_len: usize) -> AppResult<Vec<u8>> {
    let hmac = open_alg(BCRYPT_SHA256_ALGORITHM, BCRYPT_ALG_HANDLE_HMAC_FLAG)?;
    let mut out = vec![0u8; out_len];
    unsafe {
        check(
            BCryptDeriveKeyPBKDF2(hmac.0, Some(password), Some(salt), iterations, &mut out, 0),
            "口令派生密钥",
        )?;
    }
    Ok(out)
}

/// 组 GCM 的模式信息。cbSize 和 dwInfoVersion 必须手填（C 侧是 BCRYPT_INIT_AUTH_MODE_INFO 宏）。
fn auth_info<'a>(
    nonce: &'a mut [u8],
    aad: &'a [u8],
    tag: &'a mut [u8],
    data_len: usize,
) -> BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO {
    BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO {
        cbSize: std::mem::size_of::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>() as u32,
        dwInfoVersion: 1,
        pbNonce: nonce.as_mut_ptr(),
        cbNonce: nonce.len() as u32,
        pbAuthData: aad.as_ptr() as *mut u8,
        cbAuthData: aad.len() as u32,
        pbTag: tag.as_mut_ptr(),
        cbTag: tag.len() as u32,
        pbMacContext: std::ptr::null_mut(),
        cbMacContext: 0,
        cbAAD: 0,
        cbData: data_len as u64,
        dwFlags: 0,
    }
}

fn aes_key(key: &[u8]) -> AppResult<(Alg, Key)> {
    if key.len() != KEY_LEN {
        return Err(AppError::msg(format!("AES-256 需要 {KEY_LEN} 字节密钥")));
    }
    let alg = open_alg(BCRYPT_AES_ALGORITHM, BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS(0))?;
    let mode = wide_of(BCRYPT_CHAIN_MODE_GCM);
    let mut handle = BCRYPT_KEY_HANDLE::default();
    unsafe {
        check(
            BCryptSetProperty(
                alg.0.into(),
                BCRYPT_CHAINING_MODE,
                u16_bytes(&mode),
                0,
            ),
            "设置 GCM 链接模式",
        )?;
        check(
            BCryptGenerateSymmetricKey(alg.0, &mut handle, None, key, 0),
            "生成对称密钥",
        )?;
    }
    Ok((alg, Key(handle)))
}

/// 返回 (密文, 认证标签)。tag 要随密文一起存，解密时校验。
pub fn seal(key: &[u8], nonce: &[u8], aad: &[u8], plaintext: &[u8]) -> AppResult<(Vec<u8>, Vec<u8>)> {
    let (_alg, key_handle) = aes_key(key)?;
    let mut nonce_buf = nonce.to_vec();
    let mut tag = vec![0u8; TAG_LEN];
    let mut out = vec![0u8; plaintext.len()];
    let mut written = 0u32;
    unsafe {
        let mut info = auth_info(&mut nonce_buf, aad, &mut tag, plaintext.len());
        check(
            BCryptEncrypt(
                key_handle.0,
                Some(plaintext),
                Some(&mut info as *mut _ as *const c_void),
                None,
                Some(&mut out),
                &mut written,
                BCRYPT_FLAGS(0),
            ),
            "加密",
        )?;
    }
    out.truncate(written as usize);
    Ok((out, tag))
}

/// 标签对不上就说明口令错了或文件被改过，此时解出来的字节一律不可信。
pub fn open(key: &[u8], nonce: &[u8], aad: &[u8], ciphertext: &[u8], tag: &[u8]) -> AppResult<Vec<u8>> {
    if tag.len() != TAG_LEN {
        return Err(AppError::msg("认证标签长度不对"));
    }
    let (_alg, key_handle) = aes_key(key)?;
    let mut nonce_buf = nonce.to_vec();
    let mut tag_buf = tag.to_vec();
    let mut out = vec![0u8; ciphertext.len()];
    let mut written = 0u32;
    unsafe {
        let mut info = auth_info(&mut nonce_buf, aad, &mut tag_buf, ciphertext.len());
        check(
            BCryptDecrypt(
                key_handle.0,
                Some(ciphertext),
                Some(&mut info as *mut _ as *const c_void),
                None,
                Some(&mut out),
                &mut written,
                BCRYPT_FLAGS(0),
            ),
            "解密",
        )?;
    }
    out.truncate(written as usize);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn pbkdf2_matches_known_vector() {
        // 与 Node/OpenSSL 的 crypto.pbkdf2Sync("password","salt",4096,32,"sha256") 比对
        let dk = pbkdf2_sha256(b"password", b"salt", 4096, 32).unwrap();
        assert_eq!(
            hex(&dk),
            "c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a"
        );
        assert_eq!(dk.len(), KEY_LEN);
    }

    #[test]
    fn seal_matches_openssl_vector() {
        // GCM 在 (key, nonce, aad, plaintext) 固定时输出确定，可逐字节对照 OpenSSL
        let key = [7u8; KEY_LEN];
        let nonce = hex_bytes("00112233445566778899aabb");
        let aad = b"header-bytes".to_vec();
        let plain = b"issuer=GitHub&secret=JBSWY3DPEHPK3PXP".to_vec();

        let (ct, tag) = seal(&key, &nonce, &aad, &plain).unwrap();
        assert_eq!(
            hex(&ct),
            "c4feb4ea1925c6bdc5fcc9cf940983f33ba5310baed89dee9452f86b01b92171af120a1b6d"
        );
        assert_eq!(hex(&tag), "d7a6e2cf84996b28fa11501d712650eb");
    }

    fn hex_bytes(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn seal_open_roundtrip_and_tamper_detection() {
        let key = [7u8; KEY_LEN];
        let nonce = random_bytes(NONCE_LEN).unwrap();
        let aad = b"header-bytes".to_vec();
        let plain = b"issuer=GitHub&secret=JBSWY3DPEHPK3PXP".to_vec();

        let (ct, tag) = seal(&key, &nonce, &aad, &plain).unwrap();
        assert_eq!(ct.len(), plain.len());
        assert_ne!(ct, plain);
        assert_eq!(open(&key, &nonce, &aad, &ct, &tag).unwrap(), plain);

        // 改一个字节的密文、换 AAD、换 nonce 都必须被标签拦住
        let mut bad = ct.clone();
        bad[0] ^= 1;
        assert!(open(&key, &nonce, &aad, &bad, &tag).is_err());
        assert!(open(&key, &nonce, b"different", &ct, &tag).is_err());
        let other_nonce = random_bytes(NONCE_LEN).unwrap();
        assert!(open(&key, &other_nonce, &aad, &ct, &tag).is_err());
        assert!(open(&key, &nonce, &aad, &ct, &[0u8; TAG_LEN]).is_err());
    }

    #[test]
    fn random_bytes_are_distinct() {
        let a = random_bytes(16).unwrap();
        let b = random_bytes(16).unwrap();
        assert_eq!(a.len(), 16);
        assert_ne!(a, b);
    }
}
