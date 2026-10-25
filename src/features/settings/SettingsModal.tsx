import { useEffect, useMemo, useState } from "react";
import { Ban, Bot, DatabaseBackup, Filter, Palette, Rocket, Search } from "lucide-react";
import { useSettingsStore } from "../../stores/settings";
import { deleteAiModel, getAiModels, probeAiModel, openAiLog, readAiLog } from "../../api/aiModels";
import { clearAiMemory, getAiMemory } from "../../api/smart";
import {
  addBookmarkBlacklist,
  clearPageCache,
  getBookmarkBlacklist,
  getPageCacheCount,
  removeBookmarkBlacklist,
} from "../../api/bookmarks";
import { Input, Modal, ModalConfirm, message } from "../../ui";
import AppearanceSection from "./AppearanceSection";
import BlacklistSection from "./BlacklistSection";
import AppFilterSection from "./AppFilterSection";
import DataSection from "./DataSection";
import ModelEditModal from "./ModelEditModal";
import ModelsSection from "./ModelsSection";
import StartupSection from "./StartupSection";
import { blankModel } from "./shared";
import type { AiLogView, AiMemory, AiModel, BlacklistEntry } from "../../types";

const SECTIONS = [
  {
    key: "models",
    group: "模型",
    label: "大模型",
    desc: "任何 OpenAI 兼容接口都可以：填地址、模型名和密钥，没有预设厂商可选",
    icon: Bot,
    words: "模型 api key 密钥 接口 openai chatgpt deepseek 大模型 记忆 联网 搜索 日志 log",
  },
  {
    key: "appFilter",
    group: "应用",
    label: "应用过滤",
    desc: "同步时跳过系统应用、驱动和运行环境（注册表和开始菜单两遍都管），也能自己加模式",
    icon: Filter,
    words: "过滤 屏蔽 忽略 系统应用 驱动 运行环境 注册表 已安装 同步 名单 filter",
  },
  {
    key: "blacklist",
    group: "网页",
    label: "网址黑名单",
    desc: "命中的网址不在网页管理里显示，导入收藏夹时也会跳过",
    icon: Ban,
    words: "黑名单 屏蔽 拉黑 域名 网址 black list",
  },
  {
    key: "appearance",
    group: "通用",
    label: "外观",
    desc: "换风格、调主色和密度，点一下当场生效；存下来的风格还能导出成 JSON 发给别人",
    icon: Palette,
    words: "外观 主题 风格 颜色 配色 深色 浅色 主色 圆角 密度 字号 阴影 导轨 背景 质感 theme style color accent dark light radius density",
  },
  {
    key: "startup",
    group: "通用",
    label: "启动",
    desc: "开机自启，以及点窗口关闭键时的去向",
    icon: Rocket,
    words: "开机 自启动 登录 windows 托盘 关闭 隐藏 退出",
  },
  {
    key: "data",
    group: "通用",
    label: "数据",
    desc: "导出整个库，或从备份文件恢复",
    icon: DatabaseBackup,
    words: "备份 恢复 导出 db 数据库",
  },
] as const;

type SectionKey = (typeof SECTIONS)[number]["key"];

/**
 * 设置窗口的外壳：左边分组导航 + 搜索，右边一次只显示一个分组。
 *
 * 这里只留「跨分组的状态」：当前分组、搜索词、以及打开时一次性预取的那几份数据
 * （模型、本机记忆、网页缓存、黑名单）。分组内部的表单和临时状态都在各自文件里。
 */
export default function SettingsModal({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { loadAutostart, loadLocalPrefs } = useSettingsStore();
  const [tab, setTab] = useState<SectionKey>("models");
  const [keyword, setKeyword] = useState("");
  const [memory, setMemory] = useState<AiMemory[]>([]);
  const [pageCache, setPageCache] = useState(0);
  const [blacklist, setBlacklist] = useState<BlacklistEntry[]>([]);
  const [blockDraft, setBlockDraft] = useState("");
  const [models, setModels] = useState<AiModel[]>([]);
  const [aiLog, setAiLog] = useState<AiLogView | null>(null);
  const [modelEdit, setModelEdit] = useState<AiModel | null>(null);
  const [probingId, setProbingId] = useState<number | null>(null);

  useEffect(() => {
    if (!open) return;
    loadAutostart();
    loadLocalPrefs();
    setKeyword("");
    setAiLog(null);
    refreshModels();
    getAiMemory().then(setMemory).catch(() => setMemory([]));
    getPageCacheCount().then(setPageCache).catch(() => setPageCache(0));
    loadBlacklist();
  }, [open]);

  const shown = useMemo(() => {
    const k = keyword.trim().toLowerCase();
    if (!k) return [...SECTIONS];
    return SECTIONS.filter((s) => (s.label + s.desc + s.group + s.words).toLowerCase().includes(k));
  }, [keyword]);

  const groups = useMemo(() => [...new Set(SECTIONS.map((s) => s.group))], []);
  const active = SECTIONS.find((s) => s.key === tab && shown.some((x) => x.key === tab)) ?? shown[0];

  const refreshModels = () => getAiModels().then(setModels).catch((e: unknown) => message.error(String(e)));

  const loadBlacklist = () => getBookmarkBlacklist().then(setBlacklist).catch(() => setBlacklist([]));

  const handleProbeRow = async (m: AiModel) => {
    setProbingId(m.id);
    try {
      const r = await probeAiModel({ id: m.id, baseUrl: "" });
      if (r.ok) message.success(`${m.name}：${r.message}`);
      else message.error(`${m.name}：${r.message}`);
    } catch (e) {
      message.error(String(e));
    } finally {
      setProbingId(null);
    }
  };

  // 日志就在设置里看，只读后端给的最近那些行；整文件太长，要翻历史才丢给编辑器
  const handleShowLog = async () => {
    try {
      setAiLog(await readAiLog());
    } catch (e) {
      message.error(String(e));
    }
  };

  const handleOpenWholeLog = async () => {
    try {
      message.success(`日志在 ${await openAiLog()}`);
    } catch (e) {
      message.error(String(e));
    }
  };

  const handleAddBlock = async () => {
    const raw = blockDraft.trim();
    if (!raw) {
      message.error("先写个域名或完整网址");
      return;
    }
    try {
      const added = await addBookmarkBlacklist(raw);
      setBlockDraft("");
      await loadBlacklist();
      message.success(
        added.kind === "domain" ? `已屏蔽 ${added.pattern} 及其所有子页面` : `已屏蔽这一条地址（不挡同站其他页面）`
      );
    } catch (e) {
      message.error(String(e));
    }
  };

  const handleRemoveBlock = async (pattern: string) => {
    await removeBookmarkBlacklist(pattern);
    await loadBlacklist();
    message.success("已取消拉黑，原来那些书签会重新出现");
  };

  const handleClearPageCache = () => {
    ModalConfirm({
      title: "清空网页缓存？",
      content: `存着的 ${pageCache} 条网页信息会没掉：下次分析这些地址要重新抓网页、重新花 GitHub 的额度。`,
      onOk: async () => {
        const n = await clearPageCache();
        setPageCache(0);
        message.success(`已清掉 ${n} 条`);
      },
    });
  };

  const handleClearMemory = () => {
    ModalConfirm({
      title: "清空本机记忆？",
      content: `记着的 ${memory.length} 个软件下次都要重新判：先问模型，判不出来再联网搜。`,
      onOk: async () => {
        const n = await clearAiMemory();
        setMemory([]);
        message.success(`已清掉 ${n} 条`);
      },
    });
  };

  const handleDeleteModel = (m: AiModel) => {
    ModalConfirm({
      title: `删除模型「${m.name}」？`,
      content: "只删这条本机配置。",
      onOk: async () => {
        await deleteAiModel(m.id);
        refreshModels();
        message.success("已删除");
      },
    });
  };

  return (
    <>
      <Modal title="设置" open={open} onCancel={onClose} footer={null} width={880} bodyStyle={{ padding: 0 }}>
        <div className="settings-shell">
          <nav className="settings-nav">
            <Input
              allowClear
              prefix={<Search size={14} />}
              placeholder="搜索设置"
              value={keyword}
              onChange={setKeyword}
            />
            {groups.map((group) => {
              const items = shown.filter((s) => s.group === group);
              if (!items.length) return null;
              return (
                <div key={group}>
                  <div className="settings-nav-group">{group}</div>
                  {items.map((s) => (
                    <button
                      key={s.key}
                      type="button"
                      className={s.key === active?.key ? "settings-nav-item on" : "settings-nav-item"}
                      onClick={() => setTab(s.key)}
                    >
                      <s.icon size={16} strokeWidth={1.6} />
                      {s.label}
                    </button>
                  ))}
                </div>
              );
            })}
            {!shown.length && <div className="settings-nav-empty">没有匹配的设置</div>}
          </nav>

          <div className="settings-main">
            {!active ? (
              <div className="settings-nav-empty">换个词试试，比如「备份」「过滤」</div>
            ) : (
              <>
                <h3 className="settings-h">{active.label}</h3>
                <p className="settings-sub">{active.desc}</p>

                {active.key === "models" && (
                  <ModelsSection
                    models={models}
                    probingId={probingId}
                    aiLog={aiLog}
                    memory={memory}
                    pageCache={pageCache}
                    onAddModel={() => setModelEdit(blankModel)}
                    onEditModel={(m) => setModelEdit(m)}
                    onProbe={handleProbeRow}
                    onDeleteModel={handleDeleteModel}
                    onShowLog={() => void handleShowLog()}
                    onOpenWholeLog={() => void handleOpenWholeLog()}
                    onHideLog={() => setAiLog(null)}
                    onClearMemory={handleClearMemory}
                    onClearPageCache={handleClearPageCache}
                  />
                )}
                {active.key === "appearance" && <AppearanceSection />}
                {active.key === "appFilter" && <AppFilterSection />}
                {active.key === "blacklist" && (
                  <BlacklistSection
                    entries={blacklist}
                    draft={blockDraft}
                    onDraftChange={setBlockDraft}
                    onAdd={() => void handleAddBlock()}
                    onRemove={(pattern) => void handleRemoveBlock(pattern)}
                  />
                )}
                {active.key === "startup" && <StartupSection />}
                {active.key === "data" && <DataSection />}
              </>
            )}
          </div>
        </div>
      </Modal>

      {modelEdit && <ModelEditModal model={modelEdit} onClose={() => setModelEdit(null)} onModelsChanged={refreshModels} />}
    </>
  );
}
