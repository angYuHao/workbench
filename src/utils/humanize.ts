/** 界面给人看的数字：字节数说成人话（文件页的容量、大小都读它） */
export function formatBytes(n: number | null | undefined): string {
  if (n === null || n === undefined) return "";
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = n / 1024;
  let i = 0;
  while (value >= 1024 && i < units.length - 1) {
    value /= 1024;
    i += 1;
  }
  return `${value >= 10 ? value.toFixed(0) : value.toFixed(1)} ${units[i]}`;
}

/** 修改时间说成人话：当年的只写月日时分，跨年才带年份（跟资源管理器一个口径） */
export function formatWhen(secs: number | null | undefined): string {
  if (!secs) return "";
  const d = new Date(secs * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  const md = `${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  if (d.getFullYear() === new Date().getFullYear()) return `${md} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  return `${d.getFullYear()}-${md}`;
}
