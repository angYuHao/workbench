import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { SELF, type Domain } from "../../api/notify";
import { openUrl } from "../../api/bookmarks";
import { launchApp } from "../../api/desktop";
import { useBookmarksStore } from "../../stores/bookmarks";
import { useDesktopStore } from "../../stores/desktop";
import { message } from "../../ui";
import Favicon from "../../shared/Favicon";
import IconWithFallback from "../../shared/IconWithFallback";

/** 一条可打开的东西：名字、副标题、图标、点开它做什么。两栏共用这一个形状 */
interface Row {
  id: number;
  name: string;
  sub: string;
  icon: ReactNode;
  open: () => void;
}

/** 大小写不敏感的子串匹配，名字和副标题都算 */
function hit(needle: string, ...fields: Array<string | null>) {
  if (!needle) return true;
  return fields.some((f) => (f ?? "").toLowerCase().includes(needle));
}

/**
 * 顶边抽屉：一块能打字的面板，左边全部应用、右边全部网址，各自滚动。
 *
 * 数据读的是两个现成 store（不是另开接口），所以主窗口那边改了分类、加了收藏，
 * 这边靠 stores/sync.ts 的域订阅自己跟上；使用次数是第三档 `usage`，只有这儿在乎。
 *
 * 打开一条之后就把面板收起来：搜它是为了去别的地方干活，留着挡屏幕没道理。
 */
export default function DrawerStrip() {
  const [query, setQuery] = useState("");
  const input = useRef<HTMLInputElement>(null);
  const bookmarks = useBookmarksStore((s) => s.bookmarks);
  const apps = useDesktopStore((s) => s.apps);

  useEffect(() => {
    const loadBoth = () => {
      void useBookmarksStore.getState().load();
      void useDesktopStore.getState().loadApps();
    };
    loadBoth();
    const subscribed = listen<{ domain: Domain; from: string }>("data-changed", (e) => {
      if (e.payload.from === SELF || e.payload.domain !== "usage") return;
      loadBoth();
    });
    return () => void subscribed.then((unlisten) => unlisten());
  }, []);

  // 面板是划出来就立刻能打字：每次拿到焦点都把光标放到搜索框里
  useEffect(() => {
    const focusIn = () => input.current?.focus();
    focusIn();
    window.addEventListener("focus", focusIn);
    return () => window.removeEventListener("focus", focusIn);
  }, []);

  const hide = () => void getCurrentWindow().hide().catch(() => undefined);

  const close = (run: () => Promise<unknown>) => {
    run()
      .then(hide)
      .catch((e: unknown) => message.error(String(e)));
  };

  const q = query.trim().toLowerCase();

  const appRows = useMemo(
    () =>
      [...apps]
        .sort((a, b) => b.useCount - a.useCount)
        .filter((a) => hit(q, a.alias, a.name, a.description))
        .map<Row>((a) => ({
          id: a.id,
          name: a.alias || a.name,
          sub: a.description ?? "",
          icon: <IconWithFallback name={a.name} iconPath={a.iconPath} size={22} />,
          open: () => close(() => launchApp(a.id)),
        })),
    [apps, q],
  );

  const linkRows = useMemo(
    () =>
      [...bookmarks]
        .sort((a, b) => b.useCount - a.useCount)
        .filter((b) => hit(q, b.title, b.url, b.description))
        .map<Row>((b) => ({
          id: b.id,
          name: b.title || b.url,
          sub: b.description ?? b.url,
          icon: <Favicon url={b.url} />,
          open: () => close(() => openUrl(b.url, b.id)),
        })),
    [bookmarks, q],
  );

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      hide();
      return;
    }
    if (e.key !== "Enter") return;
    // 回车开第一条：应用优先，因为「打开一个程序」比「打开一个网页」更像在顶边干的事。
    // 这里不能写 a[0]?.open() ?? b[0]?.open() —— open() 返回 void，?? 会把两条都开一遍
    const first = appRows[0] ?? linkRows[0];
    if (first) first.open();
  };

  return (
    <div className="drawer">
      <div className="drawer-panel">
        <div className="drawer-search">
          <input
            ref={input}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder="搜应用和网址，回车开第一个，Esc 收起"
            spellCheck={false}
          />
          <span className="drawer-count">
            {appRows.length} + {linkRows.length}
          </span>
        </div>
        <div className="drawer-cols">
          <Column title="应用" rows={appRows} empty={q ? "没搜到应用" : "还没有应用：去主窗口同步一次"} />
          <Column title="网页" rows={linkRows} empty={q ? "没搜到网址" : "还没有收藏：去主窗口导入收藏夹"} />
        </div>
      </div>
    </div>
  );
}

function Column({ title, rows, empty }: { title: string; rows: Row[]; empty: string }) {
  return (
    <div className="drawer-col">
      <div className="drawer-col-title">
        {title}
        <span className="drawer-col-n">{rows.length}</span>
      </div>
      <div className="drawer-list">
        {rows.length ? (
          rows.map((r) => (
            <div className="drawer-row" key={r.id} onClick={r.open} title={r.sub ? `${r.name} — ${r.sub}` : r.name}>
              <span className="drawer-ico">{r.icon}</span>
              <span className="drawer-row-text">
                <span className="drawer-row-name">{r.name}</span>
                {!!r.sub && <span className="drawer-row-sub">{r.sub}</span>}
              </span>
            </div>
          ))
        ) : (
          <div className="drawer-empty">{empty}</div>
        )}
      </div>
    </div>
  );
}
