import { useMemo } from "react";
import { renameCategory, saveCategory } from "../api/categories";
import { message } from "../ui";
import { orderByUsage, type CategoryNode } from "./CategoryTree";
import type { Category, ModuleType } from "../types";

/** 脚手架要用的就这三样：挂在哪个分类、自己是什么 id、被打开过几次（分类树按它排） */
interface Item {
  id: number;
  categoryId: number | null;
  useCount: number;
}

interface Options<T> {
  /** 三个模块共用一张分类表，靠它区分 */
  type: ModuleType;
  categories: Category[];
  items: T[];
  /** 分类名要显示成什么：桌面是去掉扩展名的程序名，书签是网页标题 */
  labelOf: (item: T) => string;
  /** 改归类要把整条写回去（后端是整行更新），所以由调用方给完整的 save */
  save: (item: T) => Promise<unknown>;
  /** 增删改名之后刷新；桌面是两条一起，书签是 store 的 load */
  reload: () => Promise<void>;
  /** 树上额外挂的分类名（桌面：规则在用但还没建分类的）。要传稳定引用，否则每次渲染都重算 */
  ghostNames?: string[];
  /** 改名顺带改写了分类规则时，提示语要说清几条；不给就只报「已改名」 */
  renamed?: (touched: number, name: string) => string;
  /** 有的 store 的 save 自己会刷新（书签），有的不会（桌面）；后者在这里补 */
  afterMove?: () => Promise<void>;
}

/**
 * 分类树那一圈的公共部分：建树 + 计数 + 按使用次数排、新建、改名、拖到分类上改归属。
 * 删除不在这里——桌面要问「要不要连分类规则一起删」、还要处理只有规则没有分类的幽灵节点，
 * 书签那边是完全另一套文案，硬并只会把两边的差异藏进一个开关参数里。
 */
export function useCategoryScaffold<T extends Item>({
  type,
  categories,
  items,
  labelOf,
  save,
  reload,
  ghostNames,
  renamed,
  afterMove,
}: Options<T>) {
  const catNodes = useMemo(() => {
    const nodes: CategoryNode[] = categories.map((c) => ({
      id: c.id,
      name: c.name,
      count: items.filter((i) => i.categoryId === c.id).length,
    }));
    for (const name of ghostNames ?? []) {
      if (!nodes.some((n) => n.name === name)) nodes.push({ id: 0, name, count: 0 });
    }
    return orderByUsage(nodes, items);
  }, [categories, items, ghostNames]);

  const addCategory = async (name: string) => {
    const sort = Math.max(0, ...categories.map((c) => c.sort)) + 1;
    await saveCategory({ id: 0, name, type, parentId: null, sort, icon: null });
    message.success(`已添加分类「${name}」`);
    await reload();
  };

  const renameCat = async (node: CategoryNode, name: string) => {
    const touched = await renameCategory(node.id, name);
    message.success(renamed ? renamed(touched, name) : `已改名为「${name}」`);
    await reload();
  };

  /** 拖到分类上就是改归属；已经是这个分类了就不动，也不报一次成功的假消息 */
  const dropOnCategory = async (node: CategoryNode, itemId: number) => {
    const item = items.find((i) => i.id === itemId);
    if (!item || item.categoryId === node.id) return;
    try {
      await save({ ...item, categoryId: node.id });
      message.success(`已把「${labelOf(item)}」归入「${node.name}」`);
      if (afterMove) await afterMove();
    } catch (e) {
      message.error(String(e));
    }
  };

  return { catNodes, addCategory, renameCat, dropOnCategory };
}
