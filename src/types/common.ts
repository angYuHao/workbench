/** 三个模块共用一张分类表，靠 type 区分。后端 `Category.type` 是 String，约束点在这里 */
export type ModuleType = "app" | "bookmark" | "file";

export interface Category {
  id: number;
  name: string;
  type: ModuleType;
  parentId: number | null;
  sort: number;
  icon: string | null;
}
