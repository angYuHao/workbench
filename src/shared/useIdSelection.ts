import { useCallback, useState } from "react";

export interface IdSelection {
  active: boolean;
  selectedIds: Set<number>;
  start: () => void;
  stop: () => void;
  clear: () => void;
  toggle: (id: number) => void;
  selectAll: (ids: number[]) => void;
}

export function useIdSelection(): IdSelection {
  const [active, setActive] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());

  const clear = useCallback(() => setSelectedIds(new Set()), []);
  const selectAll = useCallback((ids: number[]) => setSelectedIds(new Set(ids)), []);
  const start = useCallback(() => setActive(true), []);
  const stop = useCallback(() => {
    setActive(false);
    setSelectedIds(new Set());
  }, []);
  const toggle = useCallback((id: number) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  return { active, selectedIds, start, stop, clear, toggle, selectAll };
}
