# MuseLink 协作指南

- `muse-connector/src/` 是 React 界面，`muse-connector/src-tauri/` 是 Tauri 和 Rust 后端。优先复用现有组件、状态模型和命令。
- 界面文案通过 `muse-connector/src/lib/i18n.tsx` 和 `muse-connector/src/lib/messages.ts` 管理。更新页面、引导或反馈文案时同步中英文。对外使用英文名 MuseLink 和中文名 Muse连接器。
- 维护文件授权与令牌边界。新版 `/agent/v1` 使用独立令牌，文件访问受已保存的授权范围和 Windows 账户权限约束。变更权限、路径解析或令牌处理时，用隔离测试目录和测试凭证验证。
- 本地开发使用 `pnpm -C muse-connector tauri dev` 的隔离配置。界面变更运行 `pnpm -C muse-connector build`。Rust 变更运行 `cargo test --manifest-path muse-connector/src-tauri/Cargo.toml --locked`。原生窗口、隧道或文件操作的验收需要在对应入口验证。
- 更改用户流程、安装或构建命令时，同步更新 `README.md` 和 `README.en.md`。文档使用仓库相对路径，提交前排除本机配置、凭证、日志、内部笔记和构建产物。
