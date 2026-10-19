//! 不花钱的那一步：撞本机记忆。判过一次就记住的软件，不再花模型和搜索的钱。
//! 分类规则不参与：那条只管桌面上文件挪去哪个文件夹，跟「这台机器装了什么程序」是两回事。

use crate::models::{SmartCategorizeItem, SmartCategorizeResult};

/// 记忆用的键就是去掉扩展名的显示名：`Loomy.lnk` 和 `Loomy` 是同一个软件
pub fn stem(name: &str) -> String {
    std::path::Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string())
}

/// 先看本机记忆：分成「已经记着」和「还要判」两堆。
pub fn by_memory(
    conn: &rusqlite::Connection,
    items: &[SmartCategorizeItem],
) -> (Vec<SmartCategorizeResult>, Vec<SmartCategorizeItem>) {
    let mut known = Vec::new();
    let mut rest = Vec::new();
    for item in items {
        match crate::services::ai::memory::lookup(conn, &stem(&item.name)).ok().flatten() {
            Some(hit) => known.push(SmartCategorizeResult {
                id: item.id,
                name: item.name.clone(),
                suggested_category: hit.category.clone(),
                reason: if hit.evidence.is_empty() {
                    format!("本机记着「{category}」", category = hit.category)
                } else {
                    format!("本机记着：{}（{hit_source}）", hit.evidence, hit_source = hit.source)
                },
            }),
            None => rest.push(item.clone()),
        }
    }
    (known, rest)
}
