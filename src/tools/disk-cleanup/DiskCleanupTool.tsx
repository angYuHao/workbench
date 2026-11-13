import { useEffect, useMemo, useState } from "react";
import { ArrowLeft, Eraser, RefreshCw } from "lucide-react";
import { runCleanup, scanCleanup, type CleanupCategory } from "../../api/cleanup";
import { Button, Checkbox, Empty, ModalConfirm, Space, Spin, Table, message } from "../../ui";
import { formatBytes } from "../../utils/humanize";
import type { ToolProps } from "../registry";

/** 能勾的：位置可用、且确实有东西可清。空的或扫不动的摆出来但不让勾 */
const selectable = (r: CleanupCategory) => r.available && (r.bytes > 0 || r.files > 0);

/**
 * C 盘磁盘清理：只碰公认安全的几处垃圾位置，别的一律不碰。
 * 打开就扫一遍（只统计不动磁盘），勾上要清的几类，确认后后端逐个文件删，
 * 被占用/没权限的跳过并如实计数。回收站不可逆，确认窗口里单独红字提醒。
 */
export default function DiskCleanupTool({ onBack }: ToolProps) {
  const [rows, setRows] = useState<CleanupCategory[]>([]);
  const [scanning, setScanning] = useState(true);
  const [running, setRunning] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());

  const scan = async () => {
    setScanning(true);
    try {
      const fresh = await scanCleanup();
      setRows(fresh);
      // 重扫后把已经空了/扫不动的勾选去掉，免得按旧账清
      setSelected((prev) => new Set(fresh.filter((r) => prev.has(r.id) && selectable(r)).map((r) => r.id)));
    } catch (e) {
      message.error(String(e));
    } finally {
      setScanning(false);
    }
  };

  useEffect(() => {
    void scan();
  }, []);

  const selectableRows = useMemo(() => rows.filter(selectable), [rows]);
  const selectedRows = useMemo(() => rows.filter((r) => selected.has(r.id)), [rows, selected]);
  const selectedBytes = selectedRows.reduce((s, r) => s + r.bytes, 0);
  const selectedFiles = selectedRows.reduce((s, r) => s + r.files, 0);
  const allChecked = selectableRows.length > 0 && selectableRows.every((r) => selected.has(r.id));

  const toggle = (id: string, on: boolean) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });

  const handleClean = () => {
    const ids = selectedRows.map((r) => r.id);
    if (!ids.length) {
      message.info("先勾上要清理的项目");
      return;
    }
    const hasRecycle = selectedRows.some((r) => r.id === "recycle");
    ModalConfirm({
      title: "清理所选项目？",
      okText: `清理 ${ids.length} 项`,
      content: (
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          <div>
            将清理以下 {ids.length} 项，合计约 {formatBytes(selectedBytes)}：
          </div>
          <ul style={{ margin: 0, paddingLeft: 18 }}>
            {selectedRows.map((r) => (
              <li key={r.id}>
                {r.label}（{formatBytes(r.bytes)}）
              </li>
            ))}
          </ul>
          {hasRecycle && (
            <div style={{ color: "var(--color-danger)" }}>回收站清空后找不回来，请确认里面没有还要的东西。</div>
          )}
          <div className="label">被占用或没权限的文件会跳过，不会强删；清理只碰上面列出的位置。</div>
        </div>
      ),
      onOk: async () => {
        setRunning(true);
        try {
          const results = await runCleanup(ids);
          const freed = results.reduce((s, r) => s + r.freed_bytes, 0);
          const removed = results.reduce((s, r) => s + r.removed_files, 0);
          const skipped = results.reduce((s, r) => s + r.skipped_files, 0);
          const notes = results.map((r) => r.note).filter((n): n is string => !!n);
          message.success(
            `已清理 ${formatBytes(freed)}，删除 ${removed} 个文件` +
              (skipped ? `，跳过 ${skipped} 个（被占用或没权限）` : ""),
          );
          if (notes.length) message.error(notes.join("；"));
          setSelected(new Set());
          await scan();
        } finally {
          setRunning(false);
        }
      },
    });
  };

  return (
    <div className="page">
      <div className="toolbar">
        <Button type="text" icon={<ArrowLeft size={16} />} onClick={onBack}>
          工具
        </Button>
        <h2 className="toolbar-title toolbar-title-pane">C 盘磁盘清理</h2>
        <div className="toolbar-spacer" />
        <Space>
          <Button icon={<RefreshCw size={16} />} loading={scanning} onClick={() => void scan()}>
            重新扫描
          </Button>
          <Button
            type="primary"
            icon={<Eraser size={16} />}
            loading={running}
            disabled={!selectedRows.length}
            onClick={handleClean}
          >
            清理所选
          </Button>
        </Space>
      </div>

      <div className="content-pane">
        <div className="label" style={{ marginBottom: 12 }}>
          只清理公认安全的几处垃圾位置：临时文件、更新缓存、缩略图缓存、错误报告、回收站。扫描只统计不动磁盘；
          被占用或没权限的文件会跳过，不会强删，也不碰名单以外的任何位置。
        </div>

        {scanning ? (
          <div style={{ display: "flex", alignItems: "center", gap: 8, padding: 24 }}>
            <Spin />
            <span className="label">正在扫描各处垃圾位置…</span>
          </div>
        ) : rows.length === 0 ? (
          <Empty description="没有可清理的项目" />
        ) : (
          <>
            <Table
              dataSource={rows}
              rowKey={(r) => r.id}
              size="small"
              layout="fixed"
              columns={[
                {
                  title: (
                    <Checkbox
                      checked={allChecked}
                      disabled={!selectableRows.length}
                      ariaLabel="全选可清理项目"
                      onChange={(on) => setSelected(on ? new Set(selectableRows.map((r) => r.id)) : new Set())}
                    />
                  ),
                  width: 48,
                  render: (_: unknown, r: CleanupCategory) => (
                    <Checkbox
                      checked={selected.has(r.id)}
                      disabled={!selectable(r)}
                      ariaLabel={`清理 ${r.label}`}
                      onChange={(on) => toggle(r.id, on)}
                    />
                  ),
                },
                {
                  title: "项目",
                  dataIndex: "label",
                  render: (_: unknown, r: CleanupCategory) => (
                    <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
                      <span>{r.label}</span>
                      <span className="label">{r.desc}</span>
                      {r.note && (
                        <span className="label" style={{ color: "var(--color-warning-text)" }}>
                          {r.note}
                        </span>
                      )}
                    </div>
                  ),
                },
                {
                  title: "位置",
                  dataIndex: "hint",
                  width: 280,
                  ellipsis: true,
                  render: (v: string) => <span className="label">{v}</span>,
                },
                { title: "大小", dataIndex: "bytes", width: 100, render: (v: number) => formatBytes(v) },
                { title: "项数", dataIndex: "files", width: 80, render: (v: number) => v },
              ]}
            />
            <div className="label" style={{ marginTop: 12 }}>
              已选 {selectedRows.length} 项，合计约 {formatBytes(selectedBytes)}
              {selectedFiles ? `（${selectedFiles} 个文件/项）` : ""}
            </div>
          </>
        )}
      </div>
    </div>
  );
}