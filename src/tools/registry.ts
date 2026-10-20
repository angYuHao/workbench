import type { ComponentType } from "react";
import type { LucideIcon } from "lucide-react";
import { ShieldCheck, Wand2 } from "lucide-react";
import TotpTool from "./totp/TotpTool";
import DesktopOrganizeTool from "./desktop-organize/DesktopOrganizeTool";

export interface ToolProps {
  onBack: () => void;
}

interface Tool {
  id: string;
  name: string;
  description: string;
  keywords: string[];
  icon: LucideIcon;
  component: ComponentType<ToolProps>;
}

// 加工具只需两步：在 src/tools/<id>/ 写一个接收 onBack 的组件，然后在这里登记一条。
export const TOOLS: Tool[] = [
  {
    id: "2fa",
    name: "2FA 认证管理",
    description: "保存 GitHub、Google 等账号的 TOTP 密钥，在本机生成动态验证码",
    keywords: ["2fa", "totp", "otp", "验证码", "动态口令", "认证", "安全", "扫码", "截图", "导入", "github", "google"],
    icon: ShieldCheck,
    component: TotpTool,
  },
  {
    id: "desktop-organize",
    name: "桌面整理",
    description: "扫描桌面按分类规则归进「桌面整理」文件夹，事后可以整体搬回原处",
    keywords: ["桌面", "整理", "快捷方式", "分类规则", "关键词", "扩展名", "恢复", "清理", "organize"],
    icon: Wand2,
    component: DesktopOrganizeTool,
  },
];

export function findTool(id?: string): Tool | undefined {
  return id ? TOOLS.find((t) => t.id === id) : undefined;
}
