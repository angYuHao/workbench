import { invoke } from "@tauri-apps/api/core";
import { write } from "./notify";
import type { Category, DesktopRule, ModuleType } from "../types";

export function getCategories(type: ModuleType) {
  return invoke<Category[]>("get_categories", { type });
}

/** 分类的增删改一律广播 categories：改名会顺带改写指向旧名的规则，
 *  所以订阅端收到这个域会连规则一起重读，不在这里发两趟 */
export function saveCategory(category: Category) {
  return write<Category>("save_category", "categories", { category });
}

/** 改名会顺带把指向旧名的分类规则一起指过来；返回被改写的规则条数，同类型里重名会失败 */
export function renameCategory(id: number, name: string) {
  return write<number>("rename_category", "categories", { id, name });
}

/** 返回被移回「未分类」的项目数；alsoRules 为真时连带删掉指向这个名字的分类规则 */
export function deleteCategory(id: number, alsoRules = false) {
  return write<number>("delete_category", "categories", { id, alsoRules });
}

/** 批量删分类：一个事务要么全删要么不删。回 [退回未分类的项目数, 被一起删掉的规则条数] */
export function deleteCategories(ids: number[], alsoRules = false) {
  return write<[number, number]>("delete_categories", "categories", { ids, alsoRules });
}

export function getRules() {
  return invoke<DesktopRule[]>("get_rules");
}

export function saveRule(rule: DesktopRule) {
  return write<DesktopRule>("save_rule", "rules", { rule });
}

export function deleteRule(id: number) {
  return write<void>("delete_rule", "rules", { id });
}
