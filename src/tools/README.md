# 工具箱

一个工具一个目录，`registry.ts` 是唯一清单。加工具三步：

1. 建 `src/tools/<名字>/`，入口组件接收 `ToolProps`（只有 `onBack`，返回按钮由它自己画）。
2. 在 `registry.ts` 的 `TOOLS` 里登记一条：`id`、`name`、`description`、`keywords`、`icon`、`component`。
3. 路由是 `#/tools/<id>`，**只认 `id`，不认目录名**（现在 2FA 就是 `id: "2fa"` 配目录 `totp/`）。

## 目录约定

```
tools/
  registry.ts     清单 + Tool 接口，别在别处硬编码工具列表
  shared/         两个以上工具共用的能力才放这里
  totp/           一个工具一个目录，内部文件按角色命名：
                    <Name>Tool.tsx  入口
                    AddDialog.tsx   弹窗
                    use<Name>.ts    逻辑
                    其余按职责（import.ts / otpauth.ts）
```

## 三条硬规矩

- **禁止 `tools/A` 直接 import `tools/B` 里的文件。** 第二个工具要用某个能力时，把它提到 `shared/`，改两边引用——不要跨工具伸手。`shared/qr.ts` 就是这么来的（2FA 的截图/图片解二维码，别的工具也会用）。
- `shared/` 不放"以后可能用得上"的东西。只有一个调用方时，它留在工具自己目录里。
- 工具不自己开 IPC 通道：`invoke` 一律走 `src/api/` 里现成的包装（抓屏是 `api/screen.ts`，2FA 是 `api/totp.ts`）。权限在 `src-tauri/capabilities/default.json` 里按窗口 label 给，**新工具要新权限得显式加，不会继承**。
