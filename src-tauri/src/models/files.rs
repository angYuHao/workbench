use serde::{Deserialize, Serialize};

/// 已入库的文件/文件夹记录
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRecord {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub alias: Option<String>,
    pub description: Option<String>,
    pub is_dir: bool,
    pub managed_root: Option<String>,
    pub category_id: Option<i64>,
    pub created_at: Option<String>,
}

/// 列目录时的一项：来自磁盘，别名和说明从库里补
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub alias: Option<String>,
    pub description: Option<String>,
    /// 字节数。目录没有可靠大小，所以可空（要算就得整棵树走一遍，不是这一层该干的事）
    pub size: Option<u64>,
    /// 最后修改时间，Unix 秒；文件系统不给就空着
    pub modified_at: Option<i64>,
    /// 带隐藏或系统属性的那几项：默认不摆出来，界面上那个开关才显示
    pub hidden: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<FileNode>>,
}

/// 一个盘符：文件页侧栏「此电脑」那一节
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveInfo {
    /// 能直接 read_dir 的根路径，形如 `C:\`
    pub path: String,
    pub letter: String,
    /// 卷标；没命名过的盘是空，界面拿 `kind` 兜底
    pub label: Option<String>,
    /// 「本地磁盘 / 可移动磁盘 / 网络驱动器 / 光盘 / 内存盘」
    pub kind: String,
    pub is_removable: bool,
    pub free: Option<u64>,
    pub total: Option<u64>,
}

/// 已知文件夹（桌面/下载/文档/图片/音乐/视频）：`name` 是给人看的那句话，
/// 由我们自己给中文，不用系统的英文名（那一串在本机 shell 里也是按用户语言走的）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownFolder {
    pub name: String,
    pub path: String,
}

/// 系统剪贴板上的那串文件。`cut` 为真就是「剪切」：粘完原件要挪走
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardFiles {
    pub paths: Vec<String>,
    pub cut: bool,
}

/// 文本预览：过大时只回开头，`truncated` 告诉界面后面还有
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadTextResult {
    pub content: String,
    pub truncated: bool,
}
