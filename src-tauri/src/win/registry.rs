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
    RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER,
    HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY, REG_DWORD, REG_MULTI_SZ,
    REG_SZ,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hive {
    Local,
    Current,
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
        }
    }

    /// 界面/日志里写的那个前缀
    pub fn label(self) -> &'static str {
        match self {
            Hive::Local => "HKLM",
            Hive::Current => "HKCU",
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
        // 当前用户那份也在， per-user 安装的东西只登记在这儿
        assert!(Key::open(Hive::Current, sub, Arch::Native).is_some());
    }

    /// 打不开的键回 None，不 panic
    #[test]
    fn a_missing_key_yields_nothing() {
        assert!(Key::open(Hive::Current, r"Software\这个键不存在呀", Arch::Native).is_none());
        assert!(Key::open(Hive::Local, r"SOFTWARE\Microsoft\ nonexistent", Arch::Wow32).is_none());
    }
}
