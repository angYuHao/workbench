//! 不用花钱的判断：先撞分类规则，再撞本机记忆。
//! 规则命中的条目根本不进提示词，也就不会把私密目录里的文件名送出去。


use crate::models::{DesktopRule, SmartCategorizeItem, SmartCategorizeResult};
use crate::services::desktop::{extension_of, match_category};


/// 规则没说清的信号。落到「其他」的才值得花 token 去问模型，
/// 也顺手把私密目录里的文件名留在本机——规则命中的条目根本不进提示词。
pub(crate) const FALLBACK: &str = "其他";

/// 先用设置里的分类规则过一遍，分成「规则已判定」和「要问模型」两堆。
pub fn by_rules(
    items: &[SmartCategorizeItem],
    rules: &[DesktopRule],
) -> (Vec<SmartCategorizeResult>, Vec<SmartCategorizeItem>) {
    let mut ruled = Vec::new();
    let mut rest = Vec::new();
    for item in items {
        let category = rule_hit(item, rules);
        if category == FALLBACK {
            rest.push(item.clone());
        } else {
            ruled.push(SmartCategorizeResult {
                id: item.id,
                name: item.name.clone(),
                suggested_category: category.clone(),
                reason: format!("命中分类规则「{category}」"),
            });
        }
    }
    (ruled, rest)
}

/// 口径和整理桌面时一致：显示名去掉扩展名，扩展名取真实目标的
pub(crate) fn rule_hit(item: &SmartCategorizeItem, rules: &[DesktopRule]) -> String {
    let target = item
        .target
        .as_deref()
        .filter(|t| !t.is_empty())
        .unwrap_or(&item.name);
    let ext = extension_of(std::path::Path::new(target));
    match_category(&stem(&item.name), &ext, rules)
}

/// 记忆用的键就是去掉扩展名的显示名：`Loomy.lnk` 和 `Loomy` 是同一个软件
pub fn stem(name: &str) -> String {
    std::path::Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string())
}

/// 先看本机记忆：判过一次就记住的软件，不再花模型和搜索的钱。
/// 回（记住的行, 还要判的行）
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
