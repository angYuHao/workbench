import { useState } from "react";
import { CheckSquare, Pencil, Plus, Square, Trash2 } from "lucide-react";
import { Button, Checkbox, Input, Modal, message, pressable } from "../ui";

export interface CategoryNode {
  id: number;
  name: string;
  count: number;
}

import { carriesItem, draggedItemId } from "./dragItem";

/**
 * 分类树按「这个分类底下的条目一共被点开过多少次」排：常用的分类排前面。
 * 并列（多数是一起没被点过）时保持传进来的顺序，也就是分类自己的 sort 序 —— Array.sort 是稳定的。
 */
export function orderByUsage<T extends { categoryId: number | null; useCount: number }>(
  nodes: CategoryNode[],
  items: T[],
): CategoryNode[] {
  const sums = new Map<number, number>();
  for (const it of items) {
    if (it.categoryId !== null) sums.set(it.categoryId, (sums.get(it.categoryId) ?? 0) + it.useCount);
  }
  return [...nodes].sort((a, b) => (sums.get(b.id) ?? 0) - (sums.get(a.id) ?? 0));
}

interface Props {
  categories: CategoryNode[];
  selected: number | null;
  onSelect: (id: number | null) => void;
  /** 传了才出现「新建分类」，名字重不重复在这里就拦掉，落库交给调用方 */
  onCreate?: (name: string) => Promise<void>;
  /** 传了才在每一项上给改名按钮；规则里出现、库里还没有的分类（id 为 0）不给 */
  onRename?: (node: CategoryNode, name: string) => Promise<void>;
  /** 传了才能把条目拖到分类上；只给真的有 id 的分类当落点 */
  onDropItem?: (node: CategoryNode, itemId: number) => Promise<void>;
  /** 传了才在每一项上给删除按钮（count > 0 时由调用方决定怎么提示） */
  onDelete?: (node: CategoryNode) => void;
  /** 传了才出现「批量删除」：多选后一次交给调用方，怎么确认由它决定 */
  onDeleteMany?: (nodes: CategoryNode[]) => void;
  /** 「全部」的真实条数：各分类之和会把未分类的项目漏掉，所以由调用方给 */
  total?: number;
}

export default function CategoryTree({
  categories,
  selected,
  onSelect,
  onCreate,
  onRename,
  onDropItem,
  onDelete,
  onDeleteMany,
  total,
}: Props) {
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<CategoryNode | null>(null);
  const [batching, setBatching] = useState(false);
  const [picked, setPicked] = useState<string[]>([]);
  /** 只有规则、还没有分类行的伪分类 id 全是 0，按 id 选会互相撞，所以批量选择按行认一个唯一键 */
  const pickKey = (c: CategoryNode) => (c.id > 0 ? `#${c.id}` : `@${c.name}`);
  const pickedNodes = categories.filter((c) => picked.includes(pickKey(c)));
  const [draft, setDraft] = useState("");
  const shownTotal = total ?? categories.reduce((sum, c) => sum + c.count, 0);

  const close = () => {
    setAdding(false);
    setEditing(null);
    setDraft("");
  };

  const submit = async () => {
    const name = draft.trim();
    if (!name) {
      message.error("先起个名字");
      return false;
    }
    if (categories.some((c) => c.id !== editing?.id && c.name.toLowerCase() === name.toLowerCase())) {
      message.error(`「${name}」已经存在`);
      return false;
    }
    // 调用方报错就别关窗口：名字留在框里，改一下就能重试
    try {
      if (editing) await onRename?.(editing, name);
      else await onCreate?.(name);
    } catch (e) {
      message.error(String(e));
      return false;
    }
    close();
    return true;
  };

  return (
    <>
      <div className="stack">
        {onDeleteMany && (
          <Button
            type="text"
            size="small"
            icon={batching ? <CheckSquare size={14} /> : <Square size={14} />}
            onClick={() => {
              setBatching(!batching);
              setPicked([]);
            }}
            style={{ justifyContent: "flex-start", color: "var(--color-text-secondary)", marginBottom: 6 }}
          >
            {batching ? "退出批量删除" : "批量删除分类"}
          </Button>
        )}
        {batching && (
          <Button
            type="text"
            size="small"
            danger
            disabled={pickedNodes.length === 0}
            icon={<Trash2 size={14} />}
            onClick={() => onDeleteMany?.(pickedNodes)}
            style={{ justifyContent: "flex-start", marginBottom: 6 }}
          >
            删除选中({pickedNodes.length})
          </Button>
        )}
        {!batching && onCreate && (
          <Button
            type="text"
            size="small"
            icon={<Plus size={14} />}
            onClick={() => setAdding(true)}
            style={{ justifyContent: "flex-start", color: "var(--color-text-secondary)", marginBottom: 6 }}
          >
            新建分类
          </Button>
        )}
        <Item active={selected === null} label="全部" count={shownTotal} onClick={() => onSelect(null)} />
        {categories.map((c) => (
          <Item
            key={`${c.id}:${c.name}`}
            active={selected === c.id}
            label={c.name}
            count={c.count}
            onClick={() => onSelect(c.id)}
            onRename={
              onRename && c.id > 0
                ? () => {
                    setEditing(c);
                    setDraft(c.name);
                  }
                : undefined
            }
            onDropItem={!batching && onDropItem && c.id > 0 ? (itemId) => onDropItem(c, itemId) : undefined}
            pick={
              batching
                ? {
                    checked: picked.includes(pickKey(c)),
                    toggle: () =>
                      setPicked((prev) =>
                        prev.includes(pickKey(c)) ? prev.filter((x) => x !== pickKey(c)) : [...prev, pickKey(c)],
                      ),
                  }
                : undefined
            }
            onDelete={onDelete ? () => onDelete(c) : undefined}
          />
        ))}
      </div>

      <Modal
        title={editing ? `重命名「${editing.name}」` : "新建分类"}
        open={adding || !!editing}
        onCancel={close}
        onOk={() => void submit()}
        okText={editing ? "保存" : "添加"}
        width={360}
      >
        <Input
          autoFocus
          placeholder="分类名，如：办公软件"
          value={draft}
          onChange={setDraft}
          onEnter={() => void submit()}
        />
      </Modal>
    </>
  );
}

function Item({
  active,
  label,
  count,
  onClick,
  onRename,
  onDropItem,
  onDelete,
  pick,
}: {
  active: boolean;
  label: string;
  count: number;
  onClick: () => void;
  onRename?: () => void;
  onDropItem?: (itemId: number) => Promise<void>;
  onDelete?: () => void;
  /** 批量删除模式下的勾选态；给了它就不响应"点开筛选" */
  pick?: { checked: boolean; toggle: () => void };
}) {
  const [over, setOver] = useState(false);
  const activate = pick ? pick.toggle : onClick;

  return (
    <div
      className={`nav-item${active ? " active" : ""}${over ? " drop-target" : ""}`}
      onClick={activate}
      {...pressable(activate)}
      onDragOver={
        onDropItem
          ? (e) => {
              // 拖来的得是自家的条目，别的（比如选中的文字）不当落点
              if (!carriesItem(e)) return;
              e.preventDefault();
              e.dataTransfer.dropEffect = "move";
              if (!over) setOver(true);
            }
          : undefined
      }
      onDragLeave={onDropItem ? () => setOver(false) : undefined}
      onDrop={
        onDropItem
          ? (e) => {
              e.preventDefault();
              setOver(false);
              const id = draggedItemId(e);
              if (id !== null) void onDropItem(id);
            }
          : undefined
      }
    >
      {pick && (
        /** 复选框自己会 toggle 一次；不拦住冒泡，整行的 onClick 会再 toggle 一次，等于点了没反应 */
        <span onClick={(e) => e.stopPropagation()}>
          <Checkbox checked={pick.checked} onChange={pick.toggle} />
        </span>
      )}
      <span className="ellipsis">{label}</span>
      <span className="label">{count}</span>
      {!pick && onRename && (
        <Pencil
          className="nav-item-edit icon-button"
          size={13}
          aria-label={`重命名「${label}」`}
          {...pressable(onRename)}
          onClick={(e) => {
            e.stopPropagation();
            onRename();
          }}
        />
      )}
      {!pick && onDelete && (
        <Trash2
          className="nav-item-del icon-button"
          size={13}
          aria-label={`删除「${label}」`}
          {...pressable(onDelete)}
          onClick={(e) => {
            e.stopPropagation();
            onDelete();
          }}
        />
      )}
    </div>
  );
}
