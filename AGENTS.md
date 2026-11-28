# MuseLink 协作指南

- `muse-connector/src/` 是 React 界面，`muse-connector/src-tauri/` 是 Tauri 和 Rust 后端。优先复用现有组件、状态模型和命令。
- 界面文案通过 `muse-connector/src/lib/i18n.tsx` 和 `muse-connector/src/lib/messages.ts` 管理。更新页面、引导、托盘或反馈文案时同步中、英、日三语（含 Rust 侧 `Language::text`）。对外使用英文名 MuseLink 和中文名 Muse连接器。
- 维护文件授权与令牌边界。新版 `/agent/v1` 使用独立令牌，文件访问受已保存的授权范围和当前系统账户权限约束。变更权限、路径解析或令牌处理时，用隔离测试目录和测试凭证验证。
- 文件操作经 `muse-connector/src-tauri/src/core/handle/`（`windows.rs` / `unix.rs`）：打开句柄后核对对象实际路径与属性，读取只通过该句柄完成，写入以已验证目录中的临时文件替换并在重命名前后复核。校验后对象发生变化时返回 409 `path changed; retry`，不要退回只按路径字符串操作。
- 本地开发使用 `pnpm -C muse-connector tauri dev` 的隔离配置；若 Agent 在非交互后台终端中直接启动该命令，原生 GUI 窗口可能无法显示在用户可见桌面且会占用 `1420` 端口，需人工预览界面时应提供命令或临时启动脚本由用户在交互桌面拉起，并确认无残留后台 dev 进程。界面变更运行 `pnpm -C muse-connector build`。Rust 变更运行 `cargo test --manifest-path muse-connector/src-tauri/Cargo.toml --locked`。原生窗口、隧道或文件操作的验收需要在对应入口验证。
- 更改支持平台、核心功能、权限/安全边界、用户流程、安装方式或构建命令时，必须通盘核对并同步更新仓库根目录所有语言版本的 `README*.md`（主文件 `README.md` 为简体中文，其余语言按 `README.<locale>.md` 命名，当前为 `README.md`、`README.en.md`、`README.ja.md`；逐项检查顶部标题与短描述、语言切换栏、平台徽章、界面预览、功能概览表、下载与安装、使用指南、权限与连接边界、源码构建章节）；涉及产品定位或支持平台变化时，一并检查 `muse-connector/src-tauri/Cargo.toml`、`muse-connector/package.json` 的 `description` 以及 GitHub 仓库 About / Topics，避免只改正文却漏改顶部简介、徽章或其他语种。文档使用仓库相对路径，提交前排除本机配置、凭证、日志、内部笔记和构建产物。
- 每次功能更新、问题修复或推送发布时，必须同步更新仓库根目录全部语言版本的 `CHANGELOG*.md`（主文件 `CHANGELOG.md` 为简体中文，其余语言按 `CHANGELOG.<locale>.md` 与 `README.<locale>.md` 成对命名，当前为 `CHANGELOG.md`、`CHANGELOG.en.md`、`CHANGELOG.ja.md`；按版本与日期记录用户可见的新增、优化、修复与安全变更），确保 GitHub 首页的「更新日志」入口与程序内嵌「关于 → 更新日志 / 更新详情」弹窗（启动内置兜底并后台同步 GitHub）内容始终一致。编写日志采用「单源两级分层」规范：**一级条目**只写面向用户的精炼重点（粗体标题 + 一句话核心结果，避免堆砌底层函数名或长路径枚举，作为程序内弹窗默认展示内容）；**二级缩进条目**用于补充技术机制、路径清单、配置与安全边界细节（仓库与 GitHub Release 完整保留，程序内弹窗默认收起并支持按版本一键「展开详情 / 收起详情」）。凡来自 PR 或解决 Issue 的变更，必须在全部语言版本的**一级条目**粗体标题后使用标准 Markdown 链接标注 PR / Issue 编号（如 `[#10](https://github.com/Aenvo/MuseLink/pull/10)`、`[#4](https://github.com/Aenvo/MuseLink/issues/4)`），社区贡献者 PR 一并署名致谢（中文 `（[#N](...)，感谢 [@用户名](https://github.com/用户名)）`、英文 `([#N](...) by [@username](https://github.com/username))`、日文 `（[#N](...) · [@username](https://github.com/username)）`），确保仓库日志、GitHub Release 正文与应用内更新弹窗三端均可展示且直接点击跳转。打 `v*` 发布标签前，必须先将本次变更从 `[Unreleased]` 定稿至全部 `CHANGELOG*.md` 的 `## [vX.Y.Z] - YYYY-MM-DD` 章节，确保 `latest.json` 与 GitHub Release 自动提取到完整多语言说明。
- 自动更新经 `muse-connector/src-tauri/src/updater.rs` 与 `muse-connector/src-tauri/tauri.conf.json` 编译期固化的 Minisign 公钥及 GitHub Releases 静态 `latest.json` 完成（正式构建禁止运行时环境变量覆盖更新源，客户端不调用受限的 `api.github.com`）；更新安装前必须经 `suspend_for_update` 停止连接、命令与托管 `Everything.exe`。推送 `v*` 标签会触发 `.github/workflows/release.yml`（需配置 Secret `TAURI_SIGNING_PRIVATE_KEY`，且 `tauri.conf.json`、`Cargo.toml`、`package.json` 与全部 `CHANGELOG*.md` 版本一致），前置校验并并行构建签名 Windows 与 Linux（Ubuntu 22.04 AppImage / `.deb`）资产以及 macOS Universal（`.dmg` / `.app.zip`）未公证包，并通过 `muse-connector/scripts/generate-release-notes.ps1 -FetchGitHubNotes` 自动扫描所有已发现语言日志，拼装三平台下载文件对照表、平台注意事项、全语种详细变更与 PR / 贡献者列表创建 Draft Release，同时在 GitHub Actions Summary 输出本版本合并 PR 与社区贡献者署名覆盖核对表（漏标时自动发出 `::warning::` 提醒；本地维护已有 Release 也可通过 `-Tag vX.Y.Z -PreviousTag vX.Y.W -FetchGitHubNotes` 一键重生成）。`.github/workflows/windows.yml` 与 `release.yml` 均会运行 `generate-release-notes.ps1 -ValidateSync` 自动校验 `README*.md` / `CHANGELOG*.md` 配对、已发布版本集合及跨语种 PR / Issue 引用编号一致性；未来新增任意语言时，按 `README.<locale>.md` + `CHANGELOG.<locale>.md` 成对添加（并在应用侧 `profile.rs`、`updater.rs`、`i18n.tsx`、`messages.ts`、`changelog-dialog.tsx` 接入），工作流即会自动识别并纳入 CI 校验与 Release 生成。正式发布 Draft（或维护已有 Release）前须核对多语言正文完整性与 PR 致谢清单，禁止直接发布仅含自动 PR 标题列表的极简 Release 说明。

## 验证与覆盖

CI 工作流 `.github/workflows/windows.yml` 使用 runner 预装的 Node 22、pnpm 11.7.0 与 stable MSVC 工具链，不额外安装 Rust。

| 项目 | 命令 | CI | 本机（无管理员） | 专用 Windows 环境 |
| --- | --- | --- | --- | --- |
| 前端构建 | `pnpm -C muse-connector build` | 执行 | 执行 | — |
| 格式门禁 | `cargo fmt --manifest-path muse-connector/src-tauri/Cargo.toml --all -- --check` | 执行 | 执行 | — |
| Rust 测试 | `cargo test --manifest-path muse-connector/src-tauri/Cargo.toml --locked` | 执行 | 执行 | — |
| TCP 契约 `tcp_contract` | 含在上面的 Rust 测试内 | 执行 | 执行 | — |
| `junction_escape` | 含在上面的 Rust 测试内 | 执行 | 执行（`mklink /J` 不需提权） | — |
| `symlink_escape` | 含在上面的 Rust 测试内 | 执行（工作流启用开发者模式并设置 `MUSE_CONNECTOR_REQUIRE_SYMLINK_TESTS=1`） | 开发者模式开启时执行；否则标记未覆盖 | 执行 |
| Everything 文件名 IPC `official_named_ipc_…` | `MUSE_CONNECTOR_EVERYTHING_QA=<官方二进制目录> cargo test --manifest-path muse-connector/src-tauri/Cargo.toml --locked -- --ignored` | 不执行 | 执行（2026-11-24 已通过） | — |
| Everything NTFS 全卷 `official_ntfs_…` | 同上，另需 `MUSE_CONNECTOR_NTFS_QA_DIRS=<每块固定盘一个可写目录>` 且 Everything Service 运行中 | 不执行 | 不执行 | 执行 |
| 真实隧道与 Muse 端到端 | 人工验收 | 不执行 | 不执行 | 人工 |

- `symlink_escape` 在缺少 `SeCreateSymbolicLinkPrivilege`（错误码 5 或 1314）时只标记未覆盖；开启 Windows 开发者模式后，非管理员账户也能真实执行。CI 通过环境变量把跳过转为失败，避免把「没跑」当成「通过」。
- 两个 Everything 用例都标记为 `#[ignore]`，需要官方 `Everything.exe` 与 `es.exe`（版本与 SHA256 见 `muse-connector/scripts/package-portable.ps1`）。NTFS 用例还会启动命名实例对每块固定盘做全卷索引，在开发者工作机上 I/O 开销过大，因此只在专用环境执行。
