//! 2FA 账户的口令加密备份：把 DPAPI 锁在本机的密钥，包成一个可以带到别的机器上的文件。
//!
//! 格式是一个纯文本 JSON：头部（算法名、迭代数、盐、nonce）明文可读，密文与认证标签 hex 编码。
//! 头部同时充当 AES-GCM 的附加认证数据（AAD），所以改动迭代数、盐或 nonce 都会让标签校验失败，
//! 防止有人把口令校验强度悄悄调低后再让人输口令。

use std::collections::HashSet;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::models::TotpAccountInput;
use crate::services::totp;
use crate::win::cng;

const FORMAT: &str = "workbench-totp-backup";
const VERSION: u32 = 1;
const KDF: &str = "pbkdf2-hmac-sha256";
const CIPHER: &str = "aes-256-gcm";

/// 本机实测（`pbkdf2_cost_is_measured`，走 OS 原生实现）：30 万次约 47ms，60 万次约 96ms，
/// 所以取 100 万次≈160ms —— 高于 OWASP 对 PBKDF2-HMAC-SHA256 的 60 万下限，
/// 又只在用户点导出/导入时各付一次。
pub const ITERATIONS: u64 = 1_000_000;
/// 文件里的迭代数由外部提供，超出这个范围一律拒绝：过低等于没有保护，
/// 过高会让导入端拿着数据库锁卡住几十秒。按上面的实测 100 万≈160ms，
/// 上限 500 万就是最多≈0.8 秒 —— 本机自己写的文件是 100 万，永远够用。
const ITERATIONS_RANGE: (u64, u64) = (100_000, 5_000_000);

const SALT_LEN: usize = 16;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    issuer: String,
    account: String,
    secret: String,
    algorithm: String,
    digits: i64,
    period: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Header {
    format: String,
    version: u32,
    kdf: String,
    cipher: String,
    iterations: u64,
    salt: String,
    nonce: String,
    ciphertext: String,
    tag: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub added: usize,
    pub skipped: usize,
}

/// 头部里参与认证的部分。字段值取自文件本身，所以任何一处被改都会让标签校验失败。
fn aad_of(h: &Header) -> Vec<u8> {
    format!(
        "{FORMAT}|{v}|{kdf}|{cipher}|{iter}|{salt}|{nonce}",
        v = h.version,
        kdf = h.kdf,
        cipher = h.cipher,
        iter = h.iterations,
        salt = h.salt,
        nonce = h.nonce
    )
    .into_bytes()
}

/// 导出全部账户。返回文件内容与条数。
pub fn export(conn: &Connection, passphrase: &str) -> AppResult<(String, usize)> {
    if passphrase.is_empty() {
        return Err(AppError::msg("请设置一个备份口令"));
    }
    let accounts = totp::get_accounts(conn)?;
    if accounts.is_empty() {
        return Err(AppError::msg("还没有 2FA 账户可导出"));
    }

    let mut entries = Vec::with_capacity(accounts.len());
    for a in &accounts {
        // 有一条密钥解不开就整体失败：静默少导出的备份，等到换机那天才发现是残缺的
        let secret = totp::reveal_secret(conn, a.id).map_err(|_| {
            AppError::msg(format!(
                "「{} / {}」的密钥无法解密（数据库可能来自别的机器或用户），请先重新添加该账户",
                a.issuer, a.account
            ))
        })?;
        entries.push(Entry {
            issuer: a.issuer.clone(),
            account: a.account.clone(),
            secret,
            algorithm: a.algorithm.clone(),
            digits: a.digits,
            period: a.period,
        });
    }

    let count = entries.len();
    let plaintext = serde_json::to_vec(&entries)?;
    Ok((seal_payload(plaintext, passphrase)?, count))
}

/// 加密码文并拼出文件内容。接手明文的所有权，只为了用完能把它抹掉一次：
/// 密钥在这里以 base32 明文存在过，虽然 entries 里的副本还在托管堆上，至少这个缓冲区不必等回收。
fn seal_payload(mut plaintext: Vec<u8>, passphrase: &str) -> AppResult<String> {
    let salt = cng::random_bytes(SALT_LEN)?;
    let nonce = cng::random_bytes(cng::NONCE_LEN)?;
    let mut key = cng::pbkdf2_sha256(passphrase.as_bytes(), &salt, ITERATIONS, cng::KEY_LEN)?;

    let mut header = Header {
        format: FORMAT.into(),
        version: VERSION,
        kdf: KDF.into(),
        cipher: CIPHER.into(),
        iterations: ITERATIONS,
        salt: hex_encode(&salt),
        nonce: hex_encode(&nonce),
        ciphertext: String::new(),
        tag: String::new(),
    };
    let (ct, tag) = cng::seal(&key, &nonce, &aad_of(&header), &plaintext)?;
    key.fill(0);
    plaintext.fill(0);

    header.ciphertext = hex_encode(&ct);
    header.tag = hex_encode(&tag);
    Ok(serde_json::to_string_pretty(&header)?)
}

/// 从文件内容导入。已有的 issuer+账户 会跳过而不是覆盖，重复导入不会打乱现有库。
/// 要么整批写入要么一条不写，避免中途报错留下半份备份。
pub fn import(conn: &Connection, text: &str, passphrase: &str) -> AppResult<ImportReport> {
    if passphrase.is_empty() {
        return Err(AppError::msg("请输入备份口令"));
    }
    let header: Header =
        serde_json::from_str(text).map_err(|_| AppError::msg("这不是有效的 2FA 备份文件"))?;
    if header.format != FORMAT || header.version != VERSION {
        return Err(AppError::msg(format!(
            "备份格式不匹配（需要 {FORMAT} v{VERSION}）"
        )));
    }
    if header.kdf != KDF || header.cipher != CIPHER {
        return Err(AppError::msg("备份用了本版本不支持的算法"));
    }
    if !(ITERATIONS_RANGE.0..=ITERATIONS_RANGE.1).contains(&header.iterations) {
        return Err(AppError::msg(format!(
            "备份里的迭代数 {} 不在可接受范围内",
            header.iterations
        )));
    }

    let salt = hex_decode(&header.salt)?;
    let nonce = hex_decode(&header.nonce)?;
    let ct = hex_decode(&header.ciphertext)?;
    let mut tag = hex_decode(&header.tag)?;
    if nonce.len() != cng::NONCE_LEN || tag.len() != cng::TAG_LEN {
        return Err(AppError::msg("备份文件长度字段异常"));
    }

    let mut key =
        cng::pbkdf2_sha256(passphrase.as_bytes(), &salt, header.iterations, cng::KEY_LEN)?;
    let mut plain = match cng::open(&key, &nonce, &aad_of(&header), &ct, &tag) {
        Ok(p) => p,
        Err(_) => {
            key.fill(0);
            tag.fill(0);
            // 口令错、文件被改过、AAD 里任一场域被换，到这里都收敛成同一句话
            return Err(AppError::msg("口令不正确，或备份文件已损坏"));
        }
    };
    key.fill(0);
    tag.fill(0);

    let entries: Vec<Entry> = match serde_json::from_slice(&plain) {
        Ok(e) => {
            plain.fill(0);
            e
        }
        Err(e) => {
            plain.fill(0);
            return Err(e.into());
        }
    };
    apply_entries(conn, &entries)
}

fn apply_entries(conn: &Connection, entries: &[Entry]) -> AppResult<ImportReport> {
    conn.execute_batch("BEGIN")?;
    match write_entries(conn, entries) {
        Ok(report) => {
            crate::db::commit(conn)?;
            Ok(report)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

fn write_entries(conn: &Connection, entries: &[Entry]) -> AppResult<ImportReport> {
    let mut report = ImportReport { added: 0, skipped: 0 };
    let mut seen = existing_keys(conn)?;

    for e in entries {
        if e.issuer.trim().is_empty() || e.secret.trim().is_empty() {
            return Err(AppError::msg("备份里有缺少服务方或密钥的条目，已中止导入"));
        }
        let pair = (
            e.issuer.trim().to_lowercase(),
            e.account.trim().to_lowercase(),
        );
        // 判重集合边写边长，文件内部自己重复的条目也不会插成两条
        if !seen.insert(pair) {
            report.skipped += 1;
            continue;
        }
        totp::save_account(
            conn,
            &TotpAccountInput {
                id: 0,
                issuer: e.issuer.clone(),
                account: e.account.clone(),
                secret: e.secret.clone(),
                algorithm: Some(e.algorithm.clone()),
                digits: Some(e.digits),
                period: Some(e.period),
            },
        )?;
        report.added += 1;
    }
    Ok(report)
}

fn existing_keys(conn: &Connection) -> AppResult<HashSet<(String, String)>> {
    Ok(totp::get_accounts(conn)?
        .into_iter()
        .map(|a| {
            (
                a.issuer.trim().to_lowercase(),
                a.account.trim().to_lowercase(),
            )
        })
        .collect())
}

fn hex_encode(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn hex_decode(s: &str) -> AppResult<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return Err(AppError::msg("备份里的十六进制字段长度不对"));
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(s.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let nibble = |c: u8| -> AppResult<u8> {
            match c {
                b'0'..=b'9' => Ok(c - b'0'),
                b'a'..=b'f' => Ok(c - b'a' + 10),
                b'A'..=b'F' => Ok(c - b'A' + 10),
                _ => Err(AppError::msg("备份里含非法的十六进制字符")),
            }
        };
        out.push(nibble(bytes[i])? << 4 | nibble(bytes[i + 1])?);
        i += 2;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("../db/schema.sql")).unwrap();
        conn
    }

    fn add(conn: &Connection, issuer: &str, account: &str, secret: &str) {
        totp::save_account(
            conn,
            &TotpAccountInput {
                id: 0,
                issuer: issuer.into(),
                account: account.into(),
                secret: secret.into(),
                algorithm: None,
                digits: None,
                period: None,
            },
        )
        .unwrap();
    }

    const S1: &str = "JBSWY3DPEHPK3PXP";
    const S2: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[test]
    fn export_then_import_restores_codes() {
        let src = db();
        add(&src, "GitHub", "alice", S1);
        add(&src, "Google", "bob@example.com", S2);
        let code_before = totp::code_at(&src, 1, 1700000000).unwrap();

        let (text, count) = export(&src, "correct horse").unwrap();
        assert_eq!(count, 2);
        // 文件正文里绝不能出现明文密钥
        assert!(!text.contains(S1));
        assert!(!text.contains(S2));

        let dst = db();
        let report = import(&dst, &text, "correct horse").unwrap();
        assert_eq!((report.added, report.skipped), (2, 0));
        assert_eq!(totp::code_at(&dst, 1, 1700000000).unwrap(), code_before);
        assert_eq!(totp::reveal_secret(&dst, 2).unwrap(), S2);
        // 参数随条目一起走：导出时记录的周期/位数不是靠导入端重新猜
        let g = totp::get_accounts(&dst)
            .unwrap()
            .into_iter()
            .find(|a| a.issuer == "GitHub")
            .unwrap();
        assert_eq!((g.algorithm.as_str(), g.digits, g.period), ("sha1", 6, 30));
    }

    #[test]
    fn wrong_passphrase_adds_nothing() {
        let src = db();
        add(&src, "GitHub", "alice", S1);
        let (text, _) = export(&src, "right").unwrap();

        let dst = db();
        let err = import(&dst, &text, "wrong").unwrap_err().to_string();
        assert!(err.contains("口令不正确"), "实际错误: {err}");
        assert_eq!(totp::get_accounts(&dst).unwrap().len(), 0);
    }

    fn header_of(text: &str) -> Header {
        serde_json::from_str(text).unwrap()
    }

    /// 测试里改条目内容再重新封装，需要先按原口令解出载荷。
    fn read_entries(text: &str, passphrase: &str) -> Vec<Entry> {
        let h = header_of(text);
        let mut key =
            cng::pbkdf2_sha256(passphrase.as_bytes(), &hex_decode(&h.salt).unwrap(), h.iterations, cng::KEY_LEN)
                .unwrap();
        let plain = cng::open(
            &key,
            &hex_decode(&h.nonce).unwrap(),
            &aad_of(&h),
            &hex_decode(&h.ciphertext).unwrap(),
            &hex_decode(&h.tag).unwrap(),
        )
        .unwrap();
        key.fill(0);
        serde_json::from_slice(&plain).unwrap()
    }

    #[test]
    fn tampering_is_rejected_by_the_auth_tag() {
        let src = db();
        add(&src, "GitHub", "alice", S1);
        let (text, _) = export(&src, "pw").unwrap();

        // 只改密文的一个字符
        let mut h = header_of(&text);
        let mut ct: Vec<char> = h.ciphertext.chars().collect();
        let last = ct.len() - 1;
        ct[last] = if ct[last] == '0' { '1' } else { '0' };
        h.ciphertext = ct.into_iter().collect();
        let dst = db();
        assert!(import(&dst, &serde_json::to_string(&h).unwrap(), "pw").is_err());

        // 只把迭代数调低一半（仍在接受范围内，密文原封不动）：派生出的密钥和 AAD 同时变，必须解不开
        let mut h = header_of(&text);
        h.iterations /= 2;
        let dst = db();
        assert!(import(&dst, &serde_json::to_string(&h).unwrap(), "pw").is_err());
    }

    #[test]
    fn rejects_weak_or_foreign_files() {
        let dst = db();
        let mut h = Header {
            format: FORMAT.into(),
            version: VERSION,
            kdf: KDF.into(),
            cipher: CIPHER.into(),
            iterations: ITERATIONS,
            salt: "00".repeat(SALT_LEN),
            nonce: "00".repeat(cng::NONCE_LEN),
            ciphertext: "00".into(),
            tag: "00".repeat(cng::TAG_LEN),
        };
        h.iterations = 10;
        let low = serde_json::to_string(&h).unwrap();
        // 迭代数过低时不该去派生密钥，直接拒
        assert!(import(&dst, &low, "pw")
            .unwrap_err()
            .to_string()
            .contains("迭代数"));
        // 高得离谱也一样要在派生密钥之前就拒掉，否则导入端着库锁跑几十秒，整应用都点不动
        h.iterations = 100_000_000;
        let high = serde_json::to_string(&h).unwrap();
        assert!(import(&dst, &high, "pw")
            .unwrap_err()
            .to_string()
            .contains("迭代数"));
        assert!(import(&dst, "not json at all", "pw").is_err());
        assert!(import(&dst, r#"{"format":"other"}"#, "pw").is_err());
        h.version = 9;
        assert!(import(&dst, &serde_json::to_string(&h).unwrap(), "pw").is_err());
        // 长度不对的字段不会送进 CNG
        h.version = VERSION;
        h.nonce = "00".into();
        assert!(import(&dst, &serde_json::to_string(&h).unwrap(), "pw").is_err());
    }

    #[test]
    fn import_skips_accounts_that_already_exist() {
        let src = db();
        add(&src, "GitHub", "alice", S1);
        let (text, _) = export(&src, "pw").unwrap();

        let dst = db();
        add(&dst, "github", "ALICE", "AAAAAAAAAAAAAAAA");
        // 大小写不敏感地判重，且不会覆盖已有账户的密钥
        let report = import(&dst, &text, "pw").unwrap();
        assert_eq!((report.added, report.skipped), (0, 1));
        assert_eq!(totp::reveal_secret(&dst, 1).unwrap(), "AAAAAAAAAAAAAAAA");
    }

    #[test]
    fn duplicated_entries_inside_one_file_are_added_once() {
        let src = db();
        add(&src, "GitHub", "alice", S1);
        let (text, _) = export(&src, "pw").unwrap();

        let mut entries = read_entries(&text, "pw");
        entries.push(entries[0].clone());
        let again = seal_payload(serde_json::to_vec(&entries).unwrap(), "pw").unwrap();

        let dst = db();
        let report = import(&dst, &again, "pw").unwrap();
        assert_eq!((report.added, report.skipped), (1, 1));
        assert_eq!(totp::get_accounts(&dst).unwrap().len(), 1);
    }

    #[test]
    fn a_bad_entry_leaves_the_database_untouched() {
        let src = db();
        add(&src, "GitHub", "alice", S1);
        let (text, _) = export(&src, "pw").unwrap();

        // 手工造一份「第二条非法」的备份：位数 99 会被 save_account 拒掉
        let mut entries = read_entries(&text, "pw");
        entries.push(Entry {
            issuer: "Broken".into(),
            account: "bob".into(),
            secret: S2.into(),
            algorithm: "sha1".into(),
            digits: 99,
            period: 30,
        });
        let bad = seal_payload(serde_json::to_vec(&entries).unwrap(), "pw").unwrap();

        let dst = db();
        assert!(import(&dst, &bad, "pw").is_err());
        assert_eq!(totp::get_accounts(&dst).unwrap().len(), 0);
    }

    #[test]
    fn empty_passphrase_and_empty_db_are_refused() {
        let src = db();
        assert!(export(&src, "").is_err());
        assert!(export(&src, "pw").is_err());
        let dst = db();
        assert!(import(&dst, "{}", "").is_err());
    }

    #[test]
    fn hex_roundtrip_and_validation() {
        let b = vec![0x00, 0x0f, 0xa5, 0xff];
        assert_eq!(hex_encode(&b), "000fa5ff");
        assert_eq!(hex_decode("000FA5FF").unwrap(), b);
        assert!(hex_decode("00f").is_err());
        assert!(hex_decode("zz").is_err());
    }

    /// 迭代数取值的依据。手动跑：cargo test --offline pbkdf2_cost -- --ignored --nocapture
    #[test]
    #[ignore]
    fn pbkdf2_cost_is_measured() {
        use std::time::Instant;
        for it in [100_000u64, 300_000, 600_000] {
            let salt = cng::random_bytes(SALT_LEN).unwrap();
            let t = Instant::now();
            let mut key = cng::pbkdf2_sha256(b"measured", &salt, it, cng::KEY_LEN).unwrap();
            println!("{it} 次迭代耗时 {:?}", t.elapsed());
            key.fill(0);
        }
    }
}
