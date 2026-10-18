import { Ban, CheckSquare, Trash2, X } from "lucide-react";
import { Button, Space } from "../ui";
import type { IdSelection } from "./useIdSelection";

interface Props {
  selection: IdSelection;
  /** 当前可见项的 id，用于全选 */
  allIds: number[];
  onDelete: (ids: number[]) => void;
  /** 传了才给「拉黑」按钮：多选拉黑走精确地址 */
  onBlock?: (ids: number[]) => void;
}

export default function SelectionBar({ selection, allIds, onDelete, onBlock }: Props) {
  const { selectedIds } = selection;
  const allSelected = selectedIds.size === allIds.length;

  return (
    <Space>
      <Button
        icon={<CheckSquare size={16} />}
        onClick={() => (allSelected ? selection.clear() : selection.selectAll(allIds))}
      >
        {allSelected ? "取消全选" : "全选"}
      </Button>
      <Button
        type="primary"
        danger
        icon={<Trash2 size={16} />}
        disabled={selectedIds.size === 0}
        onClick={() => onDelete(Array.from(selectedIds))}
      >
        删除({selectedIds.size})
      </Button>
      {onBlock && (
        <Button
          icon={<Ban size={16} />}
          disabled={selectedIds.size === 0}
          onClick={() => onBlock(Array.from(selectedIds))}
        >
          拉黑({selectedIds.size})
        </Button>
      )}
      <Button icon={<X size={16} />} onClick={selection.stop}>
        退出选择
      </Button>
    </Space>
  );
}
