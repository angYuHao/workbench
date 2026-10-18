import { Sparkles } from "lucide-react";
import { Button, Checkbox, Input, Modal, Select, Table, Tag, Tooltip } from "../../ui";
import type { BookmarkAnalysis } from "./useBookmarkAnalysis";
import type { BookmarkAnalysisRow } from "../../types";

interface Props {
  analysis: BookmarkAnalysis;
}

/** 来源写成人话：抓到的是网页自己说的，模型那条是猜的；走过本机缓存的要标出来 */
function sourceTag(row: BookmarkAnalysisRow) {
  if (!row.name.trim()) {
    // 后端把失败原因写进了 source，列宽只有 104，别整条塞进去
    if (row.source.startsWith("模型没接上")) {
      return (
        <Tooltip title={row.source} placement="left">
          <Tag color="danger">模型没接上</Tag>
        </Tooltip>
      );
    }
    return <Tag color="warning">没判出来</Tag>;
  }
  const cached = row.source.includes("缓存");
  // 模型先猜上、但网页还没抓到的，要看得出「再跑一轮可能变了」
  const guess = row.source.startsWith("model") && row.queued ? "模型猜·未抓" : "";
  const base = row.source.startsWith("model")
    ? guess || "模型判断"
    : row.source.startsWith("github")
      ? "GitHub API"
      : row.source.startsWith("page")
        ? "网页自带"
        : row.source;
  return <Tag color={row.source.startsWith("model") ? "warning" : "success"}>{cached ? `${base}·缓存` : base}</Tag>;
}

export default function BookmarkAnalysisModal({ analysis }: Props) {
  const {
    open,
    rows,
    models,
    modelId,
    dropping,
    pending,
    loading,
    refresh,
    setRefresh,
    close,
    run,
    setModel,
    setName,
    setSummary,
    toggleDrop,
    apply,
  } = analysis;

  const filled = rows.filter((r) => r.name.trim()).length;
  const blank = rows.length - filled;
  const dupes = rows.filter((r) => r.duplicate).length;
  const fromCache = rows.filter((r) => r.source.includes("缓存")).length;
  const fromWeb = rows.filter((r) => /^(github|page)$/.test(r.source)).length;
  const fromModel = rows.filter((r) => r.source.startsWith("model")).length;
  const waiting = rows.filter((r) => r.queued && r.name.trim()).length;

  return (
    <Modal
      title="网址智能分析"
      open={open}
      onCancel={close}
      onOk={apply}
      okText={dropping.length ? `应用并删除 ${dropping.length} 条重复` : "应用"}
      width={920}
    >
      <div className="smart-bar">
        <span className="smart-bar-label">用哪个模型</span>
        <Select
          size="small"
          style={{ width: 240 }}
          value={modelId}
          placeholder={models.length ? "选一个模型" : "没有可用模型"}
          options={models.map((m) => ({ label: `${m.name} · ${m.model}`, value: m.id }))}
          onChange={(v) => setModel(Number(v))}
        />
        <Button size="small" type="primary" icon={<Sparkles size={14} />} loading={loading} onClick={() => void run()}>
          {rows.length ? "再跑一轮" : "开始分析"}
        </Button>
        <Checkbox checked={refresh} onChange={setRefresh} label="忽略缓存重抓" />
      </div>

      <div className="smart-foot">
        分析的是当前筛出来的这些：换分类或改搜索词再点，就只分析这一批。先读本机缓存，再抓网页（GitHub 走它的 API，你机器上 github.com 网页连不通但 API 通），
        都抓不到才交给模型；一轮最多抓 40 个网页、GitHub 60 次，抓到的存本机，再点一次只补没排到的。
      </div>

      {!!rows.length && (
        <Table
          dataSource={rows}
          rowKey={(r) => r.id}
          size="small"
          layout="fixed"
          scroll={{ y: 380 }}
          columns={[
            {
              title: "网址",
              dataIndex: "url",
              width: 200,
              ellipsis: true,
              render: (v: string) => (
                <Tooltip title={v} placement="left">
                  <span className="smart-reason">{v}</span>
                </Tooltip>
              ),
            },
            {
              title: "名称",
              dataIndex: "name",
              width: 190,
              render: (_: unknown, r: BookmarkAnalysisRow) => (
                <Input
                  size="small"
                  maxLength={40}
                  placeholder={r.oldName ? `原：${r.oldName}` : "该叫什么"}
                  value={r.name}
                  onChange={(v) => setName(r.id, v)}
                />
              ),
            },
            {
              title: "简介（50 字以内）",
              dataIndex: "summary",
              render: (_: unknown, r: BookmarkAnalysisRow) => (
                <Input
                  size="small"
                  maxLength={50}
                  placeholder="一句话，可留空"
                  value={r.summary}
                  onChange={(v) => setSummary(r.id, v)}
                />
              ),
            },
            {
              title: "来源",
              dataIndex: "source",
              width: 104,
              render: (_: unknown, r: BookmarkAnalysisRow) => sourceTag(r),
            },
            {
              title: "重复",
              dataIndex: "duplicate",
              width: 64,
              render: (_: unknown, r: BookmarkAnalysisRow) =>
                r.duplicate ? (
                  <Checkbox checked={dropping.includes(r.id)} onChange={() => toggleDrop(r.id)} label={r.keep ? "留" : "删"} />
                ) : (
                  <span className="smart-reason">—</span>
                ),
            },
          ]}
        />
      )}

      {!!rows.length && (
        <div className="smart-foot">
          共 {rows.length} 条：缓存 {fromCache}，本轮抓到 {fromWeb}，模型补 {fromModel}
          {waiting > 0 && `，另有 ${waiting} 条的名字是模型猜的（网页还没抓到，再跑一轮会用真信息顶掉）`}
          {blank > 0 && `；${blank} 条没判出来，名称空着不写库`}
          {dupes > 0 && `；重复地址 ${dupes} 条，勾了要删 ${dropping.length} 条`}
        </div>
      )}

      {!rows.length && !loading && (
        <div className="settings-empty">
          这批 {pending} 条，点「开始分析」才跑。同一个域名下不同页面会尽量给出能区分的名称（比如仓库的不同文件、文档的不同章节）。
        </div>
      )}
    </Modal>
  );
}
