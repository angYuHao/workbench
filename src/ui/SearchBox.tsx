import { Search } from "lucide-react";
import { Input } from "./Input";

/** 工具栏上那个搜索框：五个页面都是「一个放大镜 + 220 宽 + 可清空」，只有提示语不一样。
    样式和尺寸写在这里，改宽度只改一处。 */
export function SearchBox({
  placeholder,
  value,
  onChange,
  style,
}: {
  placeholder: string;
  value: string;
  onChange: (v: string) => void;
  style?: React.CSSProperties;
}) {
  return (
    <Input
      allowClear
      prefix={<Search size={14} />}
      placeholder={placeholder}
      style={style ?? { width: 220 }}
      value={value}
      onChange={onChange}
    />
  );
}
