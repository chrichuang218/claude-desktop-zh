# 贡献指南

感谢你愿意为 Claude zh-CN 启动器贡献力量！在开始之前，请先阅读以下约定。

## 行为准则

请保持友善、尊重、专业。任何形式的骚扰与人身攻击均不被接受。

## 开发环境准备

需要安装：

- Node.js ≥ 18
- Rust（stable 工具链，≥ 1.77）
- Microsoft C++ Build Tools（用于编译 Tauri 后端）
- Microsoft Edge WebView2 Runtime（运行时需要）

```bash
git clone <repo-url>
cd claude-desktop-zh
npm install
```

## 常用命令

```bash
# 启动前端 + Tauri 开发模式（热重载）
npm run tauri dev

# 仅启动前端预览（浏览器，用于调整界面）
npm run dev

# 类型检查 + 生产构建
npm run build

# 代码检查
npm run lint

# 检查 Rust 后端
cd src-tauri && cargo check
```

## 提交规范

- 使用清晰的提交信息，建议格式：`类型: 简述`，如 `feat: 新增深色模式`、`fix: 修复刷新闪烁`、`docs: 更新 README`。
- 一个 PR 聚焦一件事，便于评审与回滚。
- 提交前请确认 `npm run lint`、`npm run build` 与 `cargo check` 均通过。

## 核心脚本依赖

本启动器的大部分能力依赖外部核心脚本 `cc_desktop_zh_cn_windows.py`。开发时若没有该脚本，应用会以「未找到核心脚本」状态运行，界面与交互仍可正常预览。

## 国际化与文案

当前以中文用户为主要受众，界面文案以内联中文为主，暂不引入 i18n 框架。修改面向用户的文案时请保持措辞简洁、友好。

## 提交 Issue / PR

- Bug 反馈请使用 Issue 模板，附上系统版本、复现步骤与日志面板截图。
- 功能建议请说明使用场景与预期效果。
- PR 请关联对应 Issue，并描述实现思路与测试方式。
