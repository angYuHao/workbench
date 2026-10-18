import { Ban, Plus } from "lucide-react";
import { Button, Input } from "../../ui";
import { fetchedAt } from "./shared";
import type { BlacklistEntry } from "../../types";

interface Props {
  entries: BlacklistEntry[];
  draft: string;
  onDraftChange: (value: string) => void;
  onAdd: () => void;
  onRemove: (pattern: string) => void;
}

/** 输入框里放的是用户原话（域名或完整网址），归一化由后端做，添加完由 shell 重读列表 */
export default function BlacklistSection({ entries, draft, onDraftChange, onAdd, onRemove }: Props) {
  return (
    <div className="settings-group">
      <div className="settings-line">
        <span className="settings-line-icon">
          <Ban size={16} strokeWidth={1.6} />
        </span>
        <div className="settings-line-text">
          <div className="settings-line-title">添加屏蔽规则</div>
          <div className="settings-line-desc">
            写 <b>douban.com</b> 这类域名会连 www、子域名和全部页面一起挡；写完整网址只挡这一条。带
            <b>?</b> 的不同查询串算不同页面，不会顺带被屏蔽。
          </div>
        </div>
      </div>
      <div style={{ display: "flex", gap: 8, marginBottom: 10 }}>
        <Input
          placeholder="域名 douban.com，或完整网址"
          value={draft}
          onChange={onDraftChange}
          onEnter={onAdd}
        />
        <Button type="primary" icon={<Plus size={14} />} onClick={onAdd}>
          添加
        </Button>
      </div>
      {entries.length === 0 && <div className="settings-empty">黑名单是空的</div>}
      {entries.map((b) => (
        <div className="settings-line" key={b.pattern}>
          <span className="settings-line-text">
            <div className="settings-line-title">{b.pattern}</div>
            <div className="settings-line-desc">
              {b.kind === "domain" ? "域名及其子域" : "仅这一条地址"} · 加于 {fetchedAt(b.createdAt)}
            </div>
          </span>
          <Button type="link" size="small" danger onClick={() => onRemove(b.pattern)}>
            取消
          </Button>
        </div>
      ))}
    </div>
  );
}
