import { useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { ArrowLeft } from "lucide-react";
import { findTool, TOOLS } from "./registry";
import { Button, Empty, pressable, SearchBox } from "../ui";

export default function ToolsView() {
  const { toolId } = useParams();
  const navigate = useNavigate();
  const [keyword, setKeyword] = useState("");
  const visible = useMemo(() => {
    const k = keyword.toLowerCase();
    return TOOLS.filter(
      (t) =>
        !k ||
        t.name.toLowerCase().includes(k) ||
        t.description.toLowerCase().includes(k) ||
        t.keywords.some((w) => w.includes(k)),
    );
  }, [keyword]);

  const tool = findTool(toolId);
  if (toolId && !tool) {
    return (
      <div className="page">
        <div className="toolbar">
          <Button type="text" icon={<ArrowLeft size={16} />} onClick={() => navigate("/tools")}>
            工具
          </Button>
          <h2 className="toolbar-title">未找到该工具</h2>
        </div>
        <div className="content-pane">
          <Empty description="工具可能已被移除，返回工具列表查看其他内置工具" />
        </div>
      </div>
    );
  }

  if (tool) {
    const ToolComponent = tool.component;
    return <ToolComponent onBack={() => navigate("/tools")} />;
  }

  return (
    <div className="page">
      <div className="toolbar">
        <h2 className="toolbar-title">工具</h2>
        <SearchBox placeholder="搜索内置工具" value={keyword} onChange={setKeyword} />
      </div>
      <div className="content-pane">
        {visible.length === 0 ? (
          <Empty description="没有匹配的工具" />
        ) : (
          <div className="tool-grid">
            {visible.map((t) => {
              const Icon = t.icon;
              return (
                <div
                  key={t.id}
                  className="card card-hover tool-card"
                  onClick={() => navigate(`/tools/${t.id}`)}
                  {...pressable(() => navigate(`/tools/${t.id}`))}
                >
                  <div className="tool-card-icon">
                    <Icon size={22} strokeWidth={1.5} />
                  </div>
                  <div className="stack" style={{ flex: 1, minWidth: 0 }}>
                    <div className="ellipsis" style={{ fontWeight: 600 }}>
                      {t.name}
                    </div>
                    <div className="tool-card-desc">{t.description}</div>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
