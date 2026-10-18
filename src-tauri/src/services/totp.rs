//! 2FA（RFC 6238 TOTP）账户存储与动态码生成。
//! 仓库里拿不到 hmac/sha1/base32 这些 crate，算法都在本文件内实现，
//! 由 tests 模块用 RFC 2202 / RFC 6238 的标准向量校验。

use rusqlite::{params, Connection};

use crate::error::{AppError, AppResult};
use crate::models::{TotpAccount, TotpAccountInput};
use crate::win::dpapi;

const ACCOUNT_COLUMNS: &str = "id, issuer, account, algorithm, digits, period, sort, created_at";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashAlg {
    Sha1,
    Sha256,
    Sha512,
}

impl HashAlg {
    fn parse(name: &str) -> AppResult<Self> {
        match name.to_ascii_lowercase().as_str() {
            "sha1" => Ok(HashAlg::Sha1),
            "sha256" => Ok(HashAlg::Sha256),
            "sha512" => Ok(HashAlg::Sha512),
            other => Err(AppError::msg(format!("不支持的算法: {other}"))),
        }
    }

    fn name(self) -> &'static str {
        match self {
            HashAlg::Sha1 => "sha1",
            HashAlg::Sha256 => "sha256",
            HashAlg::Sha512 => "sha512",
        }
    }

    /// HMAC 分组长度（SHA-512 是 128 字节，其余 64 字节）
    fn block_size(self) -> usize {
        match self {
            HashAlg::Sha512 => 128,
            _ => 64,
        }
    }

    fn digest(self, data: &[u8]) -> Vec<u8> {
        use sha2::Digest;
        match self {
            HashAlg::Sha1 => sha1(data).to_vec(),
            HashAlg::Sha256 => sha2::Sha256::digest(data).to_vec(),
            HashAlg::Sha512 => sha2::Sha512::digest(data).to_vec(),
        }
    }
}

// ---------------------------------------------------------------- 账户 CRUD

pub fn get_accounts(conn: &Connection) -> AppResult<Vec<TotpAccount>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {ACCOUNT_COLUMNS} FROM totp_accounts ORDER BY issuer, account"
    ))?;
    let rows = stmt.query_map([], account_from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn account_from_row(r: &rusqlite::Row) -> rusqlite::Result<TotpAccount> {
    Ok(TotpAccount {
        id: r.get(0)?,
        issuer: r.get(1)?,
        account: r.get(2)?,
        algorithm: r.get(3)?,
        digits: r.get(4)?,
        period: r.get(5)?,
        sort: r.get(6)?,
        created_at: r.get(7)?,
    })
}

/// 新增或更新账户。更新时 secret 留空表示沿用原密钥。
pub fn save_account(conn: &Connection, input: &TotpAccountInput) -> AppResult<TotpAccount> {
    let issuer = input.issuer.trim();
    let account = input.account.trim();
    if issuer.is_empty() {
        return Err(AppError::msg("请填写服务方（Issuer），例如 GitHub"));
    }

    let alg = HashAlg::parse(input.algorithm.as_deref().unwrap_or("sha1"))?;
    let digits = input.digits.unwrap_or(6);
    if !(6..=8).contains(&digits) {
        return Err(AppError::msg("位数只支持 6 到 8 位"));
    }
    let period = input.period.unwrap_or(30);
    if !(1..=300).contains(&period) {
        return Err(AppError::msg("周期需在 1 到 300 秒之间"));
    }

    let secret = normalize_base32(&input.secret);
    if input.id > 0 {
        if !secret.is_empty() {
            let secret_enc = dpapi::protect(&base32_decode(&secret)?)?;
            conn.execute(
                "UPDATE totp_accounts
                 SET issuer = ?1, account = ?2, secret_enc = ?3, algorithm = ?4, digits = ?5, period = ?6
                 WHERE id = ?7",
                params![issuer, account, secret_enc, alg.name(), digits, period, input.id],
            )?;
        } else {
            conn.execute(
                "UPDATE totp_accounts
                 SET issuer = ?1, account = ?2, algorithm = ?3, digits = ?4, period = ?5
                 WHERE id = ?6",
                params![issuer, account, alg.name(), digits, period, input.id],
            )?;
        }
        return get_account(conn, input.id);
    }

    let key = base32_decode(&secret)?;
    if key.is_empty() {
        return Err(AppError::msg("请填写密钥，或粘贴 otpauth:// 链接"));
    }
    conn.execute(
        "INSERT INTO totp_accounts (issuer, account, secret_enc, algorithm, digits, period, sort)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
        params![issuer, account, dpapi::protect(&key)?, alg.name(), digits, period],
    )?;
    get_account(conn, conn.last_insert_rowid())
}

pub fn get_account(conn: &Connection, id: i64) -> AppResult<TotpAccount> {
    conn.query_row(
        &format!("SELECT {ACCOUNT_COLUMNS} FROM totp_accounts WHERE id = ?1"),
        params![id],
        account_from_row,
    )
    .map_err(Into::into)
}

pub fn delete_account(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM totp_accounts WHERE id = ?1", params![id])?;
    Ok(())
}

// ---------------------------------------------------------------- 动态码

/// 生成 `at`（Unix 秒）时刻的验证码。
pub fn code_at(conn: &Connection, id: i64, at: i64) -> AppResult<String> {
    let (secret_enc, algorithm, digits, period): (Vec<u8>, String, i64, i64) = conn.query_row(
        "SELECT secret_enc, algorithm, digits, period FROM totp_accounts WHERE id = ?1",
        params![id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    let alg = HashAlg::parse(&algorithm)?;
    let key = dpapi::unprotect(&secret_enc)?;
    let counter = (at.max(0) as u64) / period.max(1) as u64;
    Ok(totp_code(&key, alg, digits as u32, counter))
}

pub fn now_code(conn: &Connection, id: i64) -> AppResult<String> {
    code_at(conn, id, chrono::Utc::now().timestamp())
}

/// 显示密钥，便于用户在别的设备上重新录入。
pub fn reveal_secret(conn: &Connection, id: i64) -> AppResult<String> {
    let secret_enc: Vec<u8> = conn.query_row(
        "SELECT secret_enc FROM totp_accounts WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    Ok(base32_encode(&dpapi::unprotect(&secret_enc)?))
}

fn totp_code(key: &[u8], alg: HashAlg, digits: u32, counter: u64) -> String {
    // digits 是能从库里读回来的外部数据（恢复一个旧备份就会），而 `10u64.pow(20)` 直接溢出 panic，
    // panic 又发生在握着连接锁的地方。RFC 4226 本来就只有 6..=8，钳死它。
    let digits = digits.clamp(6, 8);
    let hash = hmac(alg, key, &counter.to_be_bytes());
    let offset = (hash[hash.len() - 1] & 0x0f) as usize;
    let mut bin = 0u64;
    for b in &hash[offset..offset + 4] {
        bin = (bin << 8) | *b as u64;
    }
    bin &= 0x7fff_ffff;
    format!("{:0width$}", bin % 10u64.pow(digits), width = digits as usize)
}

// ---------------------------------------------------------------- base32

/// 去掉站点二维码里常见的空格、连字符和补齐等号
fn normalize_base32(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '_' && *c != '=')
        .flat_map(|c| c.to_uppercase())
        .collect()
}

fn base32_value(c: char) -> Option<u32> {
    match c {
        'A'..='Z' => Some(c as u32 - 'A' as u32),
        '2'..='7' => Some(c as u32 - '2' as u32 + 26),
        _ => None,
    }
}

pub fn base32_decode(s: &str) -> AppResult<Vec<u8>> {
    let mut out = Vec::new();
    let mut acc: u32 = 0;
    let mut bits = 0;
    for c in normalize_base32(s).chars() {
        let value = base32_value(c)
            .ok_or_else(|| AppError::msg(format!("密钥含非法的 base32 字符: {c}")))?;
        acc = (acc << 5) | value;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

fn base32_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::new();
    let mut acc: u32 = 0;
    let mut bits = 0;
    for byte in data {
        acc = (acc << 8) | *byte as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((acc >> bits) & 0x1f) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((acc << (5 - bits)) & 0x1f) as usize] as char);
    }
    out
}

// ---------------------------------------------------------------- HMAC / SHA-1

fn hmac(alg: HashAlg, key: &[u8], msg: &[u8]) -> Vec<u8> {
    let block = alg.block_size();
    let mut padded = if key.len() > block {
        alg.digest(key)
    } else {
        key.to_vec()
    };
    padded.resize(block, 0);

    let inner: Vec<u8> = padded.iter().map(|b| b ^ 0x36).collect();
    let outer: Vec<u8> = padded.iter().map(|b| b ^ 0x5c).collect();

    let mut data = inner;
    data.extend_from_slice(msg);
    let inner_digest = alg.digest(&data);

    let mut final_input = outer;
    final_input.extend_from_slice(&inner_digest);
    alg.digest(&final_input)
}

fn sha1(data: &[u8]) -> [u8; 20] {
    let mut state = [
        0x6745_2301u32,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    let bit_len = (data.len() as u64) * 8;

    let mut padded = data.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks(64) {
        let mut w = [0u32; 80];
        for (i, word) in w.iter_mut().take(16).enumerate() {
            *word = u32::from_be_bytes(chunk[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let (mut a, mut b, mut c, mut d, mut e) = (
            state[0],
            state[1],
            state[2],
            state[3],
            state[4],
        );
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let next = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = next;
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e]) {
            *slot = slot.wrapping_add(value);
        }
    }

    let mut out = [0u8; 20];
    for (i, word) in state.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASCII_SECRET: &[u8] = b"12345678901234567890";

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn sha1_matches_rfc3174() {
        assert_eq!(hex(&sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(
            hex(&sha1(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hex(&sha1(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
        assert_eq!(
            hex(&sha1(&[b'a'; 1_000_000])),
            "34aa973cd4c4daa4f61eeb2bdbad27316534016f"
        );
    }

    #[test]
    fn hmac_matches_rfc2202_and_rfc4231() {
        let key = b"Jefe";
        let msg = b"what do ya want for nothing?";
        assert_eq!(
            hex(&hmac(HashAlg::Sha1, key, msg)),
            "effcdf6ae5eb2fa2d27416d5f184df9c259a7c79"
        );
        assert_eq!(
            hex(&hmac(HashAlg::Sha256, key, msg)),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        assert_eq!(
            hex(&hmac(HashAlg::Sha512, key, msg)),
            "164b7a7bfcf819e2e395fbe73b56e0a387bd64222e831fd610270cd7ea2505549758bf75c05a994a6d034f65f8f0e6fdcaeab1a34d4a6b4b636e070a38bce737"
        );
    }

    #[test]
    fn base32_roundtrip() {
        let encoded = base32_encode(ASCII_SECRET);
        assert_eq!(encoded, "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
        assert_eq!(
            base32_decode("gezdg nbvgy3tqo-jqgezdg nbvgy3tqo jq=====").unwrap(),
            ASCII_SECRET.to_vec()
        );
        assert!(base32_decode("not base32!!").is_err());
    }

    /// RFC 6238 附录 B：8 位标准向量
    #[test]
    fn totp_matches_rfc6238() {
        let cases = [
            (59u64, HashAlg::Sha1, "94287082"),
            (1111111109, HashAlg::Sha1, "07081804"),
            (1111111111, HashAlg::Sha1, "14050471"),
            (1234567890, HashAlg::Sha1, "89005924"),
            (2000000000, HashAlg::Sha1, "69279037"),
            (20000000000, HashAlg::Sha1, "65353130"),
        ];
        for (time, alg, expected) in cases {
            assert_eq!(
                totp_code(ASCII_SECRET, alg, 8, time / 30),
                expected,
                "t={time}"
            );
        }

        let sha256_secret = b"12345678901234567890123456789012";
        assert_eq!(
            totp_code(sha256_secret, HashAlg::Sha256, 8, 59 / 30),
            "46119246"
        );
        let sha512_secret = b"1234567890123456789012345678901234567890123456789012345678901234";
        assert_eq!(
            totp_code(sha512_secret, HashAlg::Sha512, 8, 59 / 30),
            "90693936"
        );
    }

    #[test]
    fn six_digit_code_is_zero_padded() {
        assert_eq!(totp_code(ASCII_SECRET, HashAlg::Sha1, 6, 0).len(), 6);
    }

    #[test]
    fn an_absurd_digit_count_from_the_database_cannot_panic() {
        // digits 是能从旧备份里读回来的外部数据，10u64.pow(20) 会溢出 panic，
        // 而它是在握着连接锁的地方被调的——一下就把整个应用点不动了
        for bad in [0u32, 1, 9, 25, 4_000_000_000] {
            let code = totp_code(ASCII_SECRET, HashAlg::Sha1, bad, 0);
            assert!((6..=8).contains(&code.len()), "digits={bad} 给出 {code}");
        }
    }

    /// 走真实表结构：密钥经 DPAPI 加密入库，取码/查看时再解回
    #[test]
    fn stores_secret_and_generates_code() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("../db/schema.sql")).unwrap();

        let input = TotpAccountInput {
            id: 0,
            issuer: "GitHub".into(),
            account: "alice".into(),
            secret: "gezdgnbvgy3tqojqgezdg nbvgy3tqo jq".into(),
            algorithm: None,
            digits: None,
            period: None,
        };
        let saved = save_account(&conn, &input).unwrap();
        assert_eq!((saved.issuer.as_str(), saved.digits, saved.period), ("GitHub", 6, 30));
        assert_eq!(code_at(&conn, saved.id, 59).unwrap(), "287082");
        assert_eq!(
            reveal_secret(&conn, saved.id).unwrap(),
            "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"
        );

        // 编辑时密钥留空则沿用原值
        let renamed = TotpAccountInput {
            id: saved.id,
            issuer: "Google".into(),
            account: "alice@gmail.com".into(),
            secret: String::new(),
            algorithm: None,
            digits: None,
            period: None,
        };
        save_account(&conn, &renamed).unwrap();
        assert_eq!(code_at(&conn, saved.id, 59).unwrap(), "287082");
        assert_eq!(get_accounts(&conn).unwrap().len(), 1);

        delete_account(&conn, saved.id).unwrap();
        assert!(code_at(&conn, saved.id, 59).is_err());
    }

    #[test]
    fn rejects_bad_input() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("../db/schema.sql")).unwrap();

        let base = TotpAccountInput {
            id: 0,
            issuer: "GitHub".into(),
            account: "alice".into(),
            secret: "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".into(),
            algorithm: Some("sha1".into()),
            digits: Some(6),
            period: Some(30),
        };

        type Tweak = fn(&mut TotpAccountInput);
        let cases: [(&str, Tweak); 6] = [
            ("空服务方", |i| i.issuer.clear()),
            ("非法 base32", |i| i.secret = "!!!!".into()),
            ("空密钥", |i| i.secret = "  ".into()),
            ("位数越界", |i| i.digits = Some(4)),
            ("周期为 0", |i| i.period = Some(0)),
            ("不支持的算法", |i| i.algorithm = Some("md5".into())),
        ];
        for (label, tweak) in cases {
            let mut invalid = base.clone();
            tweak(&mut invalid);
            assert!(save_account(&conn, &invalid).is_err(), "{label}");
        }
        assert!(get_accounts(&conn).unwrap().is_empty());
    }
}
