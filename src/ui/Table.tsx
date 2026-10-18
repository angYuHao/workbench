import React from "react";

interface Column<T> {
  title: React.ReactNode;
  dataIndex?: string;
  width?: number | string;
  ellipsis?: boolean;
  render?: (value: any, record: T, index: number) => React.ReactNode;
}

interface TableProps<T> {
  dataSource: T[];
  rowKey: (record: T) => string | number;
  columns: Column<T>[];
  size?: "small" | "middle" | "large";
  /** 整行的事件原样转给 <tr>：点击、双击、以及键盘那一半（tabIndex / onKeyDown） */
  onRow?: (record: T) => React.HTMLAttributes<HTMLTableRowElement>;
  rowClassName?: (record: T) => string;
  scroll?: { y?: number };
  /** fixed = 列宽按声明走（没声明的列分剩余空间），配合 ellipsis 单元格做省略号；
     默认 auto 保持原行为，别的页面不受影响 */
  layout?: "auto" | "fixed";
}

export function Table<T extends Record<string, any>>({
  dataSource,
  rowKey,
  columns,
  size = "middle",
  onRow,
  rowClassName,
  scroll,
  layout = "auto",
}: TableProps<T>) {
  const cellPadding = size === "small" ? "6px 10px" : "10px 12px";

  return (
    <div
      style={{
        border: "1px solid var(--color-border)",
        borderRadius: "var(--radius-control)",
        overflow: "hidden",
        background: "var(--color-card)",
        fontSize: size === "small" ? 12 : 14,
      }}
    >
      <div style={{ overflow: scroll?.y ? "auto" : "visible", maxHeight: scroll?.y }}>
        <table style={{ width: "100%", borderCollapse: "collapse", tableLayout: layout }}>
          <thead>
            <tr>
              {columns.map((col, i) => (
                <th
                  key={i}
                  style={{
                    width: col.width,
                    padding: cellPadding,
                    textAlign: "left",
                    fontWeight: 600,
                    color: "var(--color-text)",
                    background: "var(--color-bg)",
                    borderBottom: "1px solid var(--color-border)",
                  }}
                >
                  {col.title}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {dataSource.map((record, idx) => {
              const rowProps = onRow?.(record) ?? {};
              const selected = (rowClassName?.(record) ?? "").includes("selected");
              return (
                <tr
                  key={rowKey(record)}
                  style={{
                    cursor: rowProps.onClick ? "pointer" : "default",
                    background: selected
                      ? "var(--color-primary-faint)"
                      : idx % 2
                        ? "var(--color-row-stripe)"
                        : "transparent",
                    borderBottom: "1px solid var(--color-border)",
                  }}
                  {...rowProps}
                >
                  {columns.map((col, ci) => {
                    const value = col.dataIndex ? record[col.dataIndex] : undefined;
                    const cell = col.render ? col.render(value, record, idx) : value;
                    return (
                      <td
                        key={ci}
                        className={col.ellipsis ? "ellipsis" : undefined}
                        title={col.ellipsis && typeof value === "string" ? value : undefined}
                        style={{
                          padding: cellPadding,
                          color: "var(--color-text)",
                          /** 200 是 auto 布局下唯一的「能不能截断」把手；fixed 布局已经由列宽管住，不能再夹死 */
                          maxWidth: col.ellipsis && layout === "auto" ? 200 : undefined,
                        }}
                      >
                        {cell ?? "—"}
                      </td>
                    );
                  })}
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}
