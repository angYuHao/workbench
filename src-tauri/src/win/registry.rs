//! 注册表只读的一小层：开键（可选 32/64 视图）、列子键、读值。
//!
//! 只读不写：这一层给「应用」页当数据源用，改注册表不在它的职责里。
//! 常量与函数形状是从本地 `windows-0.58.0` 源码里查出来的（KEY_READ=131097、
//! KEY_WOW64_32KEY=512、KEY_WOW64_64KEY=256、ERROR_NO_MORE_ITEMS=259），别照记忆改。

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS, FILETIME,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CLASSES_ROOT,
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY, REG_BINARY,
    REG_DWORD, REG_MULTI_SZ, REG_SZ,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hive {
    Local,
    Current,
    /// 文件关联那棵合并树：`.txt` → 类型名 → `ShellNew`（资源管理器「新建」那张单子）都住这儿
    Classes,
}

/// 同一个键在 64 位机器上有两份视图：64 位程序装的、32 位程序装的（WOW6432Node）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    Native,
    Wow32,
}

impl Hive {
    fn handle(self) -> HKEY {
        match self {
            Hive::Local => HKEY_LOCAL_MACHINE,
            Hive::Current => HKEY_CURRENT_USER,
            Hive::Classes => HKEY_CLASSES_ROOT,
        }
    }

    /// 界面/日志里写的那个前缀
    pub fn label(self) -> &'static str {
        match self {
            Hive::Local => "HKLM",
            Hive::Current => "HKCU",
            Hive::Classes => "HKCR",
        }
    }
}

/// 打开的一个键：Drop 时关掉，调用方不用记着配对
pub struct Key {
    hkey: HKEY,
    path: String,
}

impl Key {
    /// 打不开就回 None：权限不够、键不存在都是常事，不该把整次同步带走
    pub fn open(hive: Hive, sub_key: &str, arch: Arch) -> Option<Key> {
        let wide: Vec<u16> = sub_key.encode_utf16().chain(std::iter::once(0)).collect();
        let flags = match arch {
            Arch::Native => KEY_READ | KEY_WOW64_64KEY,
            Arch::Wow32 => KEY_READ | KEY_WOW64_32KEY,
        };
        let mut out = HKEY(std::ptr::null_mut());
        let code = unsafe {
            RegOpenKeyExW(hive.handle(), PCWSTR(wide.as_ptr()), 0, flags, &mut out)
        };
        if code != ERROR_SUCCESS {
            return None;
        }
        Some(Key { hkey: out, path: format!("{}\\{}", hive.label(), sub_key) })
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    /// 相对这个键再往下开一层。注册表里同一个东西有好几种放法（扩展名自己身上、
    /// 它指的类上、它下面那个长得像类名的子键上），一层层跳比拿字符串拼整条路径不容易拼错
    pub fn child(&self, sub_key: &str) -> Option<Key> {
        let wide: Vec<u16> = sub_key.encode_utf16().chain(std::iter::once(0)).collect();
        let mut out = HKEY(std::ptr::null_mut());
        let code = unsafe {
            RegOpenKeyExW(self.hkey, PCWSTR(wide.as_ptr()), 0, KEY_READ, &mut out)
        };
        if code != ERROR_SUCCESS {
            return None;
        }
        Some(Key { hkey: out, path: format!("{}\\{}", self.path, sub_key) })
    }

    /// 直接子键的名字。某个子键读不了就跳过它，不要因为一条烂项丢掉整片
    pub fn child_names(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut buf = [0u16; 1024];
        let mut index = 0u32;
        loop {
            // 传入的是缓冲区容量（含结尾空字符），出来的是实际字符数（不含）
            let mut len = buf.len() as u32;
            let mut ft = FILETIME::default();
            let code = unsafe {
                RegEnumKeyExW(
                    self.hkey,
                    index,
                    PWSTR(buf.as_mut_ptr()),
                    &mut len,
                    None,
                    PWSTR::null(),
                    None,
                    Some(&mut ft),
                )
            };
            if code == ERROR_NO_MORE_ITEMS {
                break;
            }
            if code == ERROR_SUCCESS {
                let take = (len as usize).min(buf.len());
                out.push(String::from_utf16_lossy(&buf[..take]));
            }
            index += 1;
            // 键名理论上不超过 255 字符，这里纯粹防一个报错却永不结束的循环
            if index > 100_000 {
                break;
            }
        }
        out
    }

    /// 字符串类的值（REG_SZ / REG_EXPAND_SZ / REG_MULTI_SZ）。
    /// 环境变量留着不展开，交给调用方——只有路径那边需要展开，测试里更好写。
    pub fn string(&self, value: &str) -> Option<String> {
        let name: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        let mut kind = REG_SZ;
        let mut size = 0u32;
        let code = unsafe {
            RegQueryValueExW(
                self.hkey,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut kind),
                None,
                Some(&mut size),
            )
        };
        if code != ERROR_SUCCESS || size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        let mut got = size;
        let code = unsafe {
            RegQueryValueExW(
                self.hkey,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut kind),
                Some(buf.as_mut_ptr()),
                Some(&mut got),
            )
        };
        // 探测和读取之间值被人改大了：这种情况直接放弃这一项，别拿半截当完整
        if code != ERROR_SUCCESS && code != ERROR_MORE_DATA || got > size {
            return None;
        }
        let units: Vec<u16> = buf[..(got as usize).min(buf.len())]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        match kind {
            REG_DWORD => None,
            REG_MULTI_SZ => {
                let parts = String::from_utf16_lossy(&units)
                    .split('\0')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join("; ");
                Some(parts)
            }
            _ => {
                let s = String::from_utf16_lossy(&units).trim_matches('\0').to_string();
                if s.trim().is_empty() {
                    None
                } else {
                    Some(s)
                }
            }
        }
    }

    /// 这个值在不在（不看内容、也不管是不是空串）。`ShellNew\NullFile` 就是一个空的 REG_SZ，
    /// 用 string() 判会把它当成「没有」——那张「新建 > 文本文档」就是这么丢的
    pub fn has_value(&self, value: &str) -> bool {
        let name: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            RegQueryValueExW(self.hkey, PCWSTR(name.as_ptr()), None, None, None, None) == ERROR_SUCCESS
        }
    }

    pub fn dword(&self, value: &str) -> Option<u32> {
        let name: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        let mut kind = REG_SZ;
        let mut buf = [0u8; 4];
        let mut size = buf.len() as u32;
        let code = unsafe {
            RegQueryValueExW(
                self.hkey,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut kind),
                Some(buf.as_mut_ptr()),
                Some(&mut size),
            )
        };
        if code != ERROR_SUCCESS || kind != REG_DWORD || size < 4 {
            return None;
        }
        Some(u32::from_le_bytes(buf))
    }

    /// REG_BINARY 原样读出。`ShellNew` 的 `Data` 多数就是这个型（一小段模板字节，
    /// 建出来的文件带正确的表头，双击能被对应程序认出来）
    pub fn binary(&self, value: &str) -> Option<Vec<u8>> {
        let name: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        let mut kind = REG_BINARY;
        let mut size = 0u32;
        let code = unsafe {
            RegQueryValueExW(
                self.hkey,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut kind),
                None,
                Some(&mut size),
            )
        };
        if code != ERROR_SUCCESS || size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        let mut got = size;
        let code = unsafe {
            RegQueryValueExW(
                self.hkey,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut kind),
                Some(buf.as_mut_ptr()),
                Some(&mut got),
            )
        };
        // 和 string() 同一个道理：探测与读取之间被改大了就丢掉这一项，别拿半截当完整
        if code != ERROR_SUCCESS && code != ERROR_MORE_DATA || got > size || kind != REG_BINARY {
            return None;
        }
        buf.truncate(got as usize);
        Some(buf)
    }
}

/// 十六进制字符串形式的模板数据：老写法把 `Data` 存成 REG_EXPAND_SZ 的一串 hex（"0001020A…"），
/// 每行 8 个字符、行内不留空格。读不出来就回 None，让调用方按「这条建不出来」处理。
pub fn parse_hex(raw: &str) -> Option<Vec<u8>> {
    let digits: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    if digits.is_empty() || !digits.len().is_multiple_of(2) || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = Vec::with_capacity(digits.len() / 2);
    let bytes = digits.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let pair = std::str::from_utf8(&bytes[i..i + 2]).ok()?;
        out.push(u8::from_str_radix(pair, 16).ok()?);
        i += 2;
    }
    Some(out)
}

impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.hkey);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每个 Windows 上都一定存在、且一定有个字符串值的键：拿它当只读层的冒烟靶子
    #[test]
    fn it_reads_a_key_that_is_always_there() {
        let key = Key::open(
            Hive::Local,
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            Arch::Native,
        );
        let Some(key) = key else {
            panic!("连系统版本键都打不开，这层的打开方式有问题");
        };
        assert!(key.path().starts_with(r"HKLM\SOFTWARE"), "{}", key.path());
        let name = key.string("ProductName");
        assert!(name.as_deref().unwrap_or("").len() > 3, "{name:?}");
        let build = key.dword("CurrentBuildNumber").or_else(|| key.string("CurrentBuildNumber").and_then(|s| s.trim().parse::<u32>().ok()));
        assert!(build.unwrap_or(0) > 10000, "系统版本号读成 {build:?}");
        // 不存在的值不该编出东西来
        assert_eq!(key.string("这个值肯定不存在"), None);
    }

    /// 32/64 两个视图都得能开：Uninstall 键的正副本就分住两边
    #[test]
    fn both_registry_views_open() {
        let sub = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
        for arch in [Arch::Native, Arch::Wow32] {
            let key = Key::open(Hive::Local, sub, arch);
            assert!(key.is_some(), "{arch:?} 视图打不开");
            let kids = key.unwrap().child_names();
            println!("{arch:?} 视图下有 {} 个子键", kids.len());
        }
        // 当前用户那份**不一定存在**：per-user 装的程序才登记在这儿，一台干净机器（CI runner
        // 就是）一个都没有，这个键也就没有。要求它存在等于在断言「这台机器装过软件」——那是
        // 机器的性质，不是这层的性质；隔壁 a_missing_key_yields_nothing 说的正是「打不开回 None」，
        // 两条对着写才不自相矛盾。所以有就顺手报个数，没有就跳过去。
        match Key::open(Hive::Current, sub, Arch::Native) {
            Some(key) => println!("HKCU 那份有 {} 个子键", key.child_names().len()),
            None => eprintln!("HKCU 没有 Uninstall 键（这台机器没登记 per-user 程序），跳过"),
        }
    }

    /// 打不开的键回 None，不 panic
    #[test]
    fn a_missing_key_yields_nothing() {
        assert!(Key::open(Hive::Current, r"Software\这个键不存在呀", Arch::Native).is_none());
        assert!(Key::open(Hive::Local, r"SOFTWARE\Microsoft\ nonexistent", Arch::Wow32).is_none());
    }

    /// HKCR 是文件关联那棵合并树，「新建」那张单子全靠它：能开、能列、`.txt` 读得到
    #[test]
    fn the_classes_hive_reads_the_extension_tree() {
        let root = Key::open(Hive::Classes, "", Arch::Native);
        assert!(root.is_some(), "HKCR 打不开，这层的合并视图就没法用");
        let kids = root.unwrap().child_names();
        assert!(
            kids.iter().any(|k| k == ".txt"),
            "HKCR 列出来 {} 个键却没有 .txt，枚举那条路有问题",
            kids.len()
        );
        let txt = Key::open(Hive::Classes, ".txt", Arch::Native);
        assert!(txt.is_some(), ".txt 每个 Windows 都有，读不到就是这层的开法不对");
        // 注意这里不断言 `.txt\ShellNew` 存在：现代 Windows 把它挂在类键上（本机是
        // txtfilelegacy 那条链上根本没有，装了 WPS 的类型才挂在 `.docx\WPS.Docx.6\ShellNew`）。
        // 那张单子到底有几种是机器的性质，services::shellnew 那边验
    }

    /// hex 字符串那版模板数据：空格和换行是排版用的，不算内容
    #[test]
    fn hex_template_data_parses_with_or_without_whitespace() {
        assert_eq!(parse_hex("0001020A").unwrap(), vec![0x00, 0x01, 0x02, 0x0A]);
        assert_eq!(parse_hex("0001 020A\n").unwrap(), vec![0x00, 0x01, 0x02, 0x0A]);
        // 半个字节、非 hex、空串都不是能用的模板
        assert!(parse_hex("001").is_none());
        assert!(parse_hex("zz").is_none());
        assert!(parse_hex("").is_none());
    }
}
