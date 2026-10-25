import { invoke } from "@tauri-apps/api/core";

/** 哪块数据脏了。订阅端（stores/sync.ts）按这个粒度决定该重读哪些 store。 */
export type Domain =
  | "apps"
  | "bookmarks"
  | "categories"
  | "desktop"
  | "files"
  | "restore"
  | "rules"
  | "settings"
  | "totp"
  /** 只是「某条被点开了一次」：主窗口的列表不为此重排（订阅表里没这个域），
   *  但顶边抽屉靠它把「常用」的顺序刷新 */
  | "usage";

/** 这个窗口自己的编号。广播会回到发送方，订阅端拿它把自己那条挑掉。
 *  用随机串而不是窗口 label：省一次 IPC 读数，也不依赖第二个窗口建起来以后 label 叫什么。 */
export const SELF = Math.random().toString(36).slice(2);

/**
 * 会改库的调用一律走这里：命令成功返回后补一发广播，告诉其他窗口这块脏了。
 *
 * 只读的调用继续用 invoke —— 广播是一次实打实的 IPC，不该花在看数据上。
 * 第二个窗口没建之前这条通道近乎无感（自己发的那条会被挑掉），但它一旦存在，
 * 加窗口就只是多一个订阅者，不用回来补这套。
 */
export function write<T>(cmd: string, domain: Domain, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, args).then((result) => {
    /** 广播失败不能把这趟判成写失败：库里已经改了，最坏是别的窗口晚一步 */
    void invoke("broadcast_change", { domain, from: SELF }).catch(() => undefined);
    return result;
  });
}
