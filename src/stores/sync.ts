import { listen } from "@tauri-apps/api/event";
import { SELF, type Domain } from "../api/notify";
import { CACHE_KEY, replayCache } from "../styles/apply";
import { useAppearanceStore } from "./appearance";
import { useBookmarksStore } from "./bookmarks";
import { useDesktopStore } from "./desktop";
import { useFilesStore } from "./files";
import { useSettingsStore } from "./settings";
import { useTotpStore } from "./totp";

type Reload = () => Promise<void>;

/** 文件页浏览的是整台机器，没有「哪些目录算我的」这层名单了：脏了就把当前看得到的那几颗标签重读一遍 */
async function reloadFiles(): Promise<void> {
  await useFilesStore.getState().reloadTabs();
}

/** 域 → 该重读哪些 store。全部复用各 store 现成的 load*，这里不写任何取数逻辑。
 *  categories 连规则一起重读：改名会顺带改写指向旧名的那几条规则。
 *  desktop 暂时没人订阅（整理日志是工具页挂载时自己读的），留着是为了以后抽屉要显示时不用回来补。 */
const LOADERS: Record<Domain, Reload[]> = {
  apps: [() => useDesktopStore.getState().loadApps()],
  bookmarks: [() => useBookmarksStore.getState().load()],
  categories: [() => useDesktopStore.getState().loadCategories(), () => useSettingsStore.getState().loadRules()],
  desktop: [],
  files: [reloadFiles],
  restore: [async () => window.location.reload()],
  rules: [() => useSettingsStore.getState().loadRules()],
  settings: [() => useSettingsStore.getState().loadLocalPrefs(), () => useAppearanceStore.getState().syncZoom()],
  totp: [() => useTotpStore.getState().load()],
  /** 使用次数：主窗口的列表不为此重排（每开一个网页就重读整张表是白工），
   *  盯它的是顶边那条抽屉，见 features/drawer/DrawerStrip.tsx。
   *  这里显式留空而不是漏掉这个键：新增一个域时会逼你在这里表一次态 */
  usage: [],
};

/**
 * 订阅别的窗口的写入。每个窗口各有一份 store、各有一个监听，所以不需要「共享状态」这种东西：
 * 库是唯一真相，窗口里全是缓存视图，跨窗口只传「哪块脏了」。
 *
 * 外观的 token 不走这条总线 —— 它已经有 localStorage 镜像，同源窗口之间的 `storage` 事件天生只通知
 * 别的窗口不通知自己，比绕一圈 IPC 更准也更省。**缩放是唯一的例外**：它是 webview 的属性、不在
 * token 里，而镜像回调里绝不能再写一次镜像（会把两个窗口写成无限往返），所以它只能走这条总线。
 */
export function startSync(): () => void {
  const pending = new Set<Domain>();
  let timer: number | undefined;

  const flush = () => {
    timer = undefined;
    const domains = [...pending];
    pending.clear();
    for (const domain of domains) {
      for (const reload of LOADERS[domain]) {
        // 某个 store 重读失败不该带走别的 store，也不该冒到控制台外面
        void reload().catch(() => undefined);
      }
    }
  };

  const subscribed = listen<{ domain: Domain; from: string }>("data-changed", (event) => {
    const { domain, from } = event.payload;
    if (from === SELF) return;
    if (!(domain in LOADERS)) return;
    /** 桌面整理一次能写几百条日志，按域攒一下，一轮只刷一次 */
    pending.add(domain);
    if (timer === undefined) timer = window.setTimeout(flush, 200);
  });

  const onStorage = (e: StorageEvent) => {
    if (e.key === CACHE_KEY) replayCache();
  };
  window.addEventListener("storage", onStorage);

  return () => {
    if (timer !== undefined) window.clearTimeout(timer);
    window.removeEventListener("storage", onStorage);
    void subscribed.then((unlisten) => unlisten());
  };
}
