//! 本机有哪些盘：文件页侧栏「此电脑」那一节的数据来源。
//! `GetDriveTypeW` 在 windows 0.58 的绑定里只回 u32、没配套常量，所以按 SDK 的定义就地写死这几个值。
use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
};

use crate::models::DriveInfo;

const DRIVE_UNKNOWN: u32 = 0;
const DRIVE_NO_ROOT_DIR: u32 = 1;
const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_FIXED: u32 = 3;
const DRIVE_REMOTE: u32 = 4;
const DRIVE_CDROM: u32 = 5;
const DRIVE_RAMDISK: u32 = 6;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 侧栏上给人看的那句话；卷标为空（没命名过的盘）时用它兜底
fn kind_name(kind: u32) -> &'static str {
    match kind {
        DRIVE_REMOVABLE => "可移动磁盘",
        DRIVE_FIXED => "本地磁盘",
        DRIVE_REMOTE => "网络驱动器",
        DRIVE_CDROM => "光盘",
        DRIVE_RAMDISK => "内存盘",
        _ => "磁盘",
    }
}

/// 卷标。取不到（盘没插、权限不够、光盘没刻名）就当没有，不报错
fn volume_label(root: &str) -> Option<String> {
    let wroot = wide(root);
    let mut buf = [0u16; 64];
    unsafe {
        GetVolumeInformationW(PCWSTR(wroot.as_ptr()), Some(&mut buf), None, None, None, None)
            .ok()?;
    }
    let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    let label = String::from_utf16_lossy(&buf[..end]);
    (!label.is_empty()).then_some(label)
}

/// (剩余, 总容量)。网络盘和光盘常拿不到，就拿不到
fn space(root: &str) -> (Option<u64>, Option<u64>) {
    let wroot = wide(root);
    let (mut free, mut total) = (0u64, 0u64);
    let ok =
        unsafe { GetDiskFreeSpaceExW(PCWSTR(wroot.as_ptr()), Some(&mut free), Some(&mut total), None) }
            .is_ok();
    if ok {
        (Some(free), Some(total))
    } else {
        (None, None)
    }
}

/// 按盘符顺序列出来。位图上有一位不代表真能用（读卡器空着、映射断了），
/// 所以还得看 `GetDriveTypeW` 说它是什么；未知和没根目录的两档直接跳过。
pub fn list_drives() -> Vec<DriveInfo> {
    let mask = unsafe { GetLogicalDrives() };
    let mut drives = Vec::new();
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let Some(letter) = char::from_u32(u32::from(b'A') + i) else {
            continue;
        };
        let root = format!("{letter}:\\");
        let wroot = wide(&root);
        let kind = unsafe { GetDriveTypeW(PCWSTR(wroot.as_ptr())) };
        if kind == DRIVE_UNKNOWN || kind == DRIVE_NO_ROOT_DIR {
            continue;
        }
        let (free, total) = space(&root);
        drives.push(DriveInfo {
            // 侧栏点它要落到一个能 read_dir 的路径上，盘符根用 `C:\` 最稳
            path: root.clone(),
            letter: letter.to_string(),
            label: volume_label(&root),
            kind: kind_name(kind).to_string(),
            is_removable: kind == DRIVE_REMOVABLE || kind == DRIVE_CDROM,
            free,
            total,
        });
    }
    drives
}
