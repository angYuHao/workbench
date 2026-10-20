import { useEffect, useState } from "react";
import { FlaskConical, Filter, Plus, X } from "lucide-react";
import {
  addAppFilter,
  getAppFilters,
  previewInstalledApps,
  removeAppFilter,
} from "../../api/desktop";
import { getSettings, setSetting } from "../../api/settings";
import { Button, Input, Space, Switch, message } from "../../ui";
import type { SyncReport } from "../../types";

/** 「过滤系统应用」开关存在 settings 那张 KV 里；没写过就当开着（后端同口径） */
const SYSTEM_KEY = "filter_system_apps";

/**
 * 应用过滤名单：同步时命中就不收——两遍采集（注册表卸载项、开始菜单快捷方式）都受它管。
 * 一条模式就是个小写子串，程序名、安装路径、注册表键名里任意一段对上就算。
 */
export default function AppFilterSection() {
  const [patterns, setPatterns] = useState<string[]>([]);
  const [system, setSystem] = useState(true);
  const [draft, setDraft] = useState("");
  const [report, setReport] = useState<SyncReport | null>(null);
  const [probing, setProbing] = useState(false);

  const reload = async () => {
    const [list, settings] = await Promise.all([getAppFilters(), getSettings()]);
    setPatterns(list);
    setSystem(settings[SYSTEM_KEY] !== "0");
  };

  useEffect(() => {
    reload().catch((e) => message.error(String(e)));
  }, []);

  const handleToggleSystem = async (on: boolean) => {
    setSystem(on);
    try {
      await setSetting(SYSTEM_KEY, on ? "1" : "0");
      message.success(on ? "之后同步会跳过系统应用、驱动和运行环境" : "这些不再跳过，下次同步会一起收进来");
      setReport(null);
    } catch (e) {
      setSystem(!on);
      message.error(String(e));
    }
  };

  const handleAdd = async () => {
    const raw = draft.trim();
    if (!raw) {
      message.error("先写点东西：程序名里的一段，或路径里的一段");
      return;
    }
    try {
      const added = await addAppFilter(raw);
      if (!added) {
        message.info(`「${raw}」已经在名单里了`);
        return;
      }
      setDraft("");
      setReport(null);
      await reload();
      message.success(`已加入过滤：${raw}`);
    } catch (e) {
      message.error(String(e));
    }
  };

  const handleRemove = async (pattern: string) => {
    try {
      await removeAppFilter(pattern);
      setReport(null);
      await reload();
      message.success(`已不再过滤「${pattern}」`);
    } catch (e) {
      message.error(String(e));
    }
  };

  const handlePreview = async () => {
    setProbing(true);
    try {
      setReport(await previewInstalledApps());
    } catch (e) {
      message.error(String(e));
    } finally {
      setProbing(false);
    }
  };

  return (
    <div className="settings-card">
      <div className="settings-line">
        <span className="settings-line-icon">
          <Filter size={16} strokeWidth={1.6} />
        </span>
        <div className="settings-line-text">
          <div className="settings-line-title">过滤系统应用、驱动和运行环境</div>
          <div className="settings-line-desc">
            注册表里那些「装过但不是给你用的东西」：明写自己是系统组件的、挂在别的程序底下的补丁、
            名字带 更新程序 / 运行库 / redistributable 的附属程序、厂商驱动包（名字里有 driver / 驱动 /
            chipset / ODBC 的），以及 .NET、DirectX、WebView2 这类给别人用的运行环境；装在 Windows
            目录里的（含微软商店的 WindowsApps）也算。自己装来用的开发工具不算：Python、Node.js、
            Audacity 这种都不会被挡。第三方软件也不因为图标住在 <b>C:\Windows\Installer</b> 就被当成系统应用。
          </div>
        </div>
        <Switch checked={system} onChange={(on) => void handleToggleSystem(on)} />
      </div>

      <div className="settings-line">
        <span className="settings-line-text">
          <div className="settings-line-title">手动过滤</div>
          <div className="settings-line-desc">
            一条就是一小段文字，不分大小写：程序名里含它、或者安装路径里含它，同步时就不收。
            比如写 <b>help</b> 挡帮助文档、写 <b>F:\Program\Games</b> 挡整个目录。
            名单只影响同步，已经收进来的卡片不会被删掉或藏起来。
          </div>
        </span>
      </div>
      <div style={{ display: "flex", gap: 8, margin: "0 0 10px" }}>
        <Input
          placeholder="程序名里的一段，或路径里的一段"
          value={draft}
          onChange={setDraft}
          onEnter={() => void handleAdd()}
        />
        <Button type="primary" icon={<Plus size={14} />} onClick={() => void handleAdd()}>
          添加
        </Button>
      </div>
      {patterns.length === 0 && <div className="settings-empty">手动名单是空的，只按上面那个开关过滤</div>}
      {patterns.map((p) => (
        <div className="settings-line" key={p}>
          <span className="settings-line-text">
            <div className="settings-line-title">{p}</div>
          </span>
          <Button type="link" size="small" danger onClick={() => void handleRemove(p)}>
            移除
          </Button>
        </div>
      ))}

      <div className="settings-line">
        <span className="settings-line-icon">
          <FlaskConical size={16} strokeWidth={1.6} />
        </span>
        <div className="settings-line-text">
          <div className="settings-line-title">试算一下</div>
          <div className="settings-line-desc">
            拿现在的名单重扫一遍（注册表卸载项 + 开始菜单快捷方式），只报数不写库：看看会收多少、又挡了谁。
          </div>
          {report && (
            <div className="settings-line-desc">
              扫到 {report.scanned} 条 · 会收 {report.inserted} 个 · 被挡 {report.filtered} 个 ·
              库里已有 {report.already} 个，同步后一共 {report.total} 个
              {report.unresolved > 0 && <> · {report.unresolved} 条认不出要打开什么，不收</>}
              {report.samples.length > 0 && (
                <>
                  <br />
                  挡掉的：{report.samples.join("、")}
                  {report.filtered > report.samples.length ? ` 等 ${report.filtered} 个` : ""}
                </>
              )}
            </div>
          )}
        </div>
        <Space>
          {report && (
            <Button type="text" size="small" icon={<X size={14} />} onClick={() => setReport(null)}>
              收起
            </Button>
          )}
          <Button size="small" loading={probing} onClick={() => void handlePreview()}>
            试算
          </Button>
        </Space>
      </div>
    </div>
  );
}
