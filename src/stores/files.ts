import { create } from "zustand";
import * as api from "../api/files";
import { previewKind } from "../utils/fileKind";
import type { FileNode, FileRecord, ReadTextResult } from "../types";

interface FilesStore {
  roots: string[];
  nodes: FileNode[];
  currentRoot: string | null;
  record: FileRecord | null;
  preview: ReadTextResult | null;
  loadRoots: () => Promise<string[]>;
  addRoot: (path: string) => Promise<void>;
  removeRoot: (path: string) => Promise<void>;
  loadNodes: (root: string) => Promise<void>;
  select: (node: FileNode) => Promise<void>;
  saveMeta: (path: string, alias: string | null, description: string | null) => Promise<void>;
}

export const useFilesStore = create<FilesStore>((set, get) => ({
  roots: [],
  nodes: [],
  currentRoot: null,
  record: null,
  preview: null,
  loadRoots: async () => {
    const roots = await api.getManagedFolders();
    set({ roots });
    return roots;
  },
  addRoot: async (path) => {
    const roots = await api.addManagedFolder(path);
    set({ roots });
    await get().loadNodes(path);
  },
  removeRoot: async (path) => {
    const roots = await api.removeManagedFolder(path);
    set({ roots });
    const nextRoot = roots[0];
    if (nextRoot) await get().loadNodes(nextRoot);
    else set({ currentRoot: null, nodes: [] });
  },
  loadNodes: async (root) => {
    const nodes = await api.listFiles(root);
    set({ nodes, currentRoot: root });
  },
  select: async (node) => {
    const record = await api.getFile(node.path);
    /** 只有 markdown 要走这条 IPC：它需要后端转好的字符串。文本/代码由 TextPreview 自己取字节解码 */
    const kind = previewKind(node.name);
    const needsText = !node.isDir && kind === "markdown";
    const preview = needsText ? await api.readTextFile(node.path).catch(() => null) : null;
    set({ record, preview });
  },
  saveMeta: async (path, alias, description) => {
    const record = await api.saveFileMeta(path, alias, description);
    set((state) => ({
      nodes: state.nodes.map((n) =>
        n.path === path ? { ...n, alias: record.alias, description: record.description } : n,
      ),
      record: state.record?.path === path ? record : state.record,
    }));
  },
}));
