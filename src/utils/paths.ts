/** Windows 路径在这里当字符串处理（分隔符两种都认），末尾斜杠一律先削掉再比 */
function trimSep(path: string): string {
  return path.replace(/[\\/]+$/, "");
}

/** 目录或文件的名字：路径最后一段 */
export function folderName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

/** 父目录；`stops` 里列的路径视为地板（不再往上走）。整台机器都能浏览，所以默认没有地板 */
export function parentOf(path: string, stops: string[] = []): string | null {
  if (stops.includes(path)) return null;
  const here = trimSep(path);
  const parent = here.replace(/[\\/][^\\/]*$/, "");
  if (!parent || parent === here) return null;
  /** 盘符根也是地板：`C:` 单拎出来在 Windows 里是「这个盘的当前目录」，拿去列目录会串地方，补回斜杠 */
  if (/^[a-z]:$/i.test(parent)) return `${parent}\\`;
  return parent;
}

/** path 是不是落在 root 里或它底下。盘符不区分大小写，比之前先转小写 */
export function isUnder(path: string, root: string): boolean {
  const p = trimSep(path).toLowerCase();
  const r = trimSep(root).toLowerCase();
  return p === r || p.startsWith(r + "\\") || p.startsWith(r + "/");
}

/** 两条路径是不是同一处：分隔符、盘符大小写、末尾斜杠都不算差别 */
export function samePath(a: string, b: string): boolean {
  return trimSep(a).replace(/\//g, "\\").toLowerCase() === trimSep(b).replace(/\//g, "\\").toLowerCase();
}

/**
 * 地址栏里手敲的那串变成能直接拿去列目录的路径：
 * 去掉「复制为路径」带上的整串引号，相对路径按当前目录补全，分隔符统一成反斜杠，
 * 末尾斜杠削掉（盘符根 `C:` 保留成 `C:\`，单拎一个 `C:` 在 Windows 里是「这个盘的当前目录」）。
 * 空串原样返回空，交给调用方当「没输入」处理。
 */
export function typedPath(input: string, base: string): string {
  let s = input.trim();
  if (s.length >= 2 && s.startsWith('"') && s.endsWith('"')) s = s.slice(1, -1).trim();
  if (!s) return "";
  s = s.replace(/\//g, "\\");
  if (!/^[a-zA-Z]:($|\\)/.test(s) && !s.startsWith("\\\\")) s = `${trimSep(base)}\\${s}`;
  if (/^[a-zA-Z]:$/.test(s)) return `${s}\\`;
  return s.replace(/\\+$/, "");
}
