import type { ComponentType } from "react";
import type { LucideIcon } from "lucide-react";
import { Eraser, ShieldCheck, Wand2 } from "lucide-react";
import TotpTool from "./totp/TotpTool";
import DesktopOrganizeTool from "./desktop-organize/DesktopOrganizeTool";
import DiskCleanupTool from "./disk-cleanup/DiskCleanupTool";

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
  {
    id: "disk-cleanup",
    name: "C 盘磁盘清理",
    description: "扫描临时文件、更新缓存、缩略图缓存、错误报告与回收站，勾选后安全清理 C 盘垃圾",
    keywords: ["c盘", "磁盘", "清理", "垃圾", "临时文件", "缓存", "回收站", "空间", "cleanup", "disk"],
    icon: Eraser,
    component: DiskCleanupTool,
  },
];

export function findTool(id?: string): Tool | undefined {
  return id ? TOOLS.find((t) => t.id === id) : undefined;
}
