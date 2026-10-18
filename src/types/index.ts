/**
 * 前后端共用的类型，按功能域分文件，这里只做汇总。
 * 全仓库仍然写 `from "../types"`，所以拆文件不需要改任何调用点；
 * 新增一个域就多一行 `export *`。
 *
 * 对面是 `src-tauri/src/models/`：字段名、可空性必须两边一致（后端 `rename_all = "camelCase"`），
 * 少一个字段就是运行时 undefined，所以这里不写可选来迁就后端的缺字段。
 */
export * from "./ai";
export * from "./bookmarks";
export * from "./common";
export * from "./desktop";
export * from "./files";
export * from "./totp";
