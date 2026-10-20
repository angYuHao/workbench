import { useEffect, useState } from "react";
import { emptyBookmarkTrash, getBookmarkTrashCount, restoreBookmarkTrash } from "../../api/bookmarks";
import { ModalConfirm, message } from "../../ui";

/**
 * 回收表那一条：智能分析删掉的重复书签先进 `bookmark_trash`，所以 `revision` 传列表长度——
 * 列表一有变化就重新看一眼有没有得撤销。
 */
export default function useBookmarkTrash({
  revision,
  reload,
}: {
  revision: number;
  reload: () => Promise<void>;
}) {
  const [trash, setTrash] = useState(0);

  useEffect(() => {
    getBookmarkTrashCount().then(setTrash).catch(() => setTrash(0));
  }, [revision]);

  const restore = async () => {
    try {
      const n = await restoreBookmarkTrash();
      message.success(`已放回 ${n} 条书签，重复标记会重新出现`);
      setTrash(0);
      await reload();
    } catch (e) {
      message.error(String(e));
    }
  };

  const drop = () => {
    ModalConfirm({
      title: `确认不要这 ${trash} 条被删的重复书签？`,
      content: "清掉后就只能从数据库备份里找回了。",
      onOk: async () => {
        await emptyBookmarkTrash();
        setTrash(0);
        message.success("已清掉");
      },
    });
  };

  return { trash, restore, drop };
}
