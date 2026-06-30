# 更新日志

本项目遵循 [语义化版本](https://semver.org/lang/zh-CN/)，变更记录参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)。

## [0.1.0] - 2026-06-29

### 新增

- 暗色毛玻璃优先的全新视觉系统，支持浅色 / 深色一键切换并跟随系统偏好。
- 设计 token 体系（语义层 + 原始色板），统一圆角、阴影、间距与字体层级。
- 推荐卡四态渐变（ready / missing / update / repair）与平滑过渡动效。
- 加载骨架屏、操作进度旋转图标、日志进场动画，状态切换更顺滑。
- 键盘快捷键：`Enter` 打开、`R` 刷新、`U` 检查更新、`F` 修复、`Esc` 重置筛选。
- 日志面板增强：时间戳、按状态筛选（全部 / 错误 / 成功）、一键复制、清空。
- 状态项可交互：异常项点击可复制定位说明与修复建议。
- 无障碍：`aria-live` / `aria-busy` / `role=status|alert`、高对比焦点轮廓、`prefers-reduced-motion` 兜底。
- 窗口失焦后重新获得焦点时静默刷新状态。
- 应用品牌图标（深绿渐变 + Claude 闪电 + 中文印章），生成全平台图标集。
- 可分发安装包配置：NSIS / MSI 双目标、自动引导 WebView2、完整图标与元数据。
- Tauri 2 capability 声明与严格 CSP。
- MIT 许可证、CHANGELOG、CONTRIBUTING、CI 与 Issue 模板。

### 变更

- `productName`、安装器标题、`.bat` 兜底输出、`Cargo.toml` 描述全部汉化。
- 版本号统一为 `0.1.0`。
- 后端硬编码开发机路径改为基于 `%USERPROFILE%` 的通用探测。
- 简化 `check_update` 命令逻辑。

### 移除

- 未引用的脚手架资源（`react.svg` / `vite.svg` / `hero.png`）。
