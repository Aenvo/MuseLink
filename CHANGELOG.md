# 更新日志

[简体中文](CHANGELOG.md) · [English](CHANGELOG.en.md) · [日本語](CHANGELOG.ja.md) · [返回 README](README.md)

本文件记录 MuseLink / Muse连接器 的功能更新、安全加固与问题修复，随版本发布与推送同步维护。

## [Unreleased]

### 安全与稳定性加固
- **发布产物增加构建来源证明**（[#16](https://github.com/Aenvo/MuseLink/pull/16)，感谢 [@KammyAugust11](https://github.com/KammyAugust11)）：发布工作流为 Windows / Linux / macOS 三个平台的发布资产生成 GitHub 构建来源证明（build provenance attestation），用户可通过 `gh attestation verify` 校验下载的二进制确实由本仓库源码在 GitHub Actions 中构建，未被替换或篡改。

## [v0.3.2] - 2026-11-28

### 优化与新增
- **首次启动自动匹配系统语言（非中 / 日环境默认英文）**：新用户首次打开将自动识别系统语言（中文简繁归属简体中文、日语显示日本語、其余语言统一回退 English），在「设置 → 常规」手动切换后保持固定。
  - **三平台原生系统语言探测**：Windows 通过 `GetUserPreferredUILanguages` / `GetUserDefaultLocaleName` 读取显示语言偏好；macOS 通过 `NSLocale.preferredLanguages` 读取系统语言列表（不依赖 Finder 启动缺失的 `LANG`）；Linux 按 `LC_ALL` → `LC_MESSAGES` → `LANG` 与 `LANGUAGE` 规范解析（`C` / `POSIX` 稳定回退英文）。
  - **无闪屏与首次配置保护**：首次自动探测仅在内存生效，不提前写入 `desktop-state.json`，避免干扰新用户首次配置引导；同步将窗口默认标题、HTML 初始属性、前端首帧兜底与 Windows NSIS 安装器回退语言调整为按系统优先并默认英文。
- **更新日志阅读分层与原生日语日志支持**：应用内「关于 → 更新日志 / 更新详情」弹窗默认只展示精炼重点并支持按版本一键展开细节，日语界面改为直接展示原生日语更新日志。
  - 新增 `CHANGELOG.ja.md`，日语界面弹窗与 GitHub 链接不再回退英文。
  - 发布与 CI 工作流升级为自动扫描仓库全部 `CHANGELOG*.md` 与 `README*.md`，自动校验多语言版本一致性并生成全语种 GitHub Release 说明，便于后续零配置扩展新语言。

### 安全与稳定性加固
- **旧版 `/api` 兼容接口改为默认关闭并支持在「设置 → 兼容性」按需开启**（[#10](https://github.com/Aenvo/MuseLink/pull/10)，感谢 [@KammyAugust11](https://github.com/KammyAugust11)）：新安装与已迁移至新版 Muse 专用令牌的用户默认仅暴露 `/agent/v1`，大幅收紧公网令牌暴露面；纯旧版 File Bridge 存量用户升级后自动保持兼容不中断。
  - **精准升级迁移与首次配置保护**：未显式配置 `legacy_api_enabled` 时，仅对已存在旧版 `config.json` + `token` 且尚未启用 `agent-token` 的纯旧版用户自动保持开启（不提前写入 `desktop-state.json`，保留首次引导状态）；文件夹模式新用户不再自动生成旧版 `config.json` 与 `token`。
  - **设置页「兼容性」卡片与完整闭环**：新增「设置 → 兼容性」开关（仅允许在已有旧配置或已保存「指定文件夹」授权时开启，不向整机磁盘模式放开）以及旧版文件令牌轮换与目录定位入口；关闭时保留本地旧令牌文件，且在双表面均关闭时自动暂停连接。
  - **表面对齐与限流/统计一致性**：关闭旧表面时所有 `/api/*` 在鉴权前直接返回 404 且不污染概览文件活动统计，同时保留未鉴权限流保护；内置归属健康探针改为优先校验 `/agent/v1/roots` 协议体，接管旧系统（`takeover_legacy`）时自动开启旧表面并在回滚时还原。
- **扩大默认敏感凭证保护范围**（[#9](https://github.com/Aenvo/MuseLink/pull/9)，感谢 [@KammyAugust11](https://github.com/KammyAugust11)）：新增对主流云/容器凭证目录与浏览器登录数据目录的默认保护，即使开启整盘或主目录授权也不会被读取或搜索。
  - `.npmrc`、`.pypirc` 由仅写保护升级为读 / 搜索 / 写全面保护，并新增 `.docker`、`.kube`、`.azure` 与 gcloud 凭证目录默认保护。
  - 新增各平台浏览器凭证库默认保护：Windows（Chrome / Edge `User Data`、Firefox `Profiles`）、macOS（Application Support 下 Chrome / Edge / Firefox，配合既有 `~/Library/Keychains` 保护）、Linux（`google-chrome` / `chromium` / `microsoft-edge`、`.mozilla/firefox`）。

## [v0.3.1] - 2026-11-27

### 问题修复与跨平台适配
- **修复全平台「复制专用令牌」失败的问题**（[#12](https://github.com/Aenvo/MuseLink/pull/12)）：macOS 改用原生隐私剪贴板、Linux 接入系统剪贴板工具、Windows 增加竞态重试，并透出具体失败原因。
  - **macOS 原生隐私剪贴板**：通过 AppKit `NSPasteboard` 写入 `org.nspasteboard.ConcealedType`、`org.nspasteboard.TransientType` 标记并启用 `CurrentHostOnly`，防止令牌进入剪贴板历史或通用剪贴板跨设备同步。
  - **Linux 剪贴板支持**：支持通过 `wl-copy` / `xclip` / `xsel` 标准输入安全写入系统剪贴板（不经命令行参数传递令牌，并隔离后台守护进程输出管道）。
  - **Windows 剪贴板稳定性与隐私加固**：为 `OpenClipboard` 增加短退避重试，缓解剪贴板历史或第三方工具瞬时占用的竞态失败，并补全 4 字节 `DWORD(0)` 隐私标记（`CanIncludeInClipboardHistory`、`CanUploadClipboardData`）。
  - **透出真实错误诊断**：新手指引与接入说明卡片复制令牌失败时展示具体原因（同步中 / 英 / 日三语），同时保留「定位令牌文件」兜底入口。
- **修复 macOS / Linux「管理本机磁盘（整机授权）」返回 403 且无法读取桌面的问题**（[#12](https://github.com/Aenvo/MuseLink/pull/12)）：支持识别系统根目录 `/`，补齐敏感路径保护并优化 macOS 权限引导。
  - **实现 Unix 本地根目录发现与校验**：`fixed_local_roots()` 在 macOS 与 Linux 上返回本机系统根目录 `root`（`/`），更新磁盘别名校验与前端磁盘选择器（「系统根目录（/）」），并拒绝空的整机根映射，修复 `/agent/v1/roots` 为空与 `/agent/v1/locations/desktop` 返回 403 的问题。
  - **补齐 macOS / Linux 安全保护与搜索索引边界**：新增 macOS `~/Library/Keychains` 全保护及系统目录、开机自启项（`~/Library/LaunchAgents`、`.config/autostart`）、登录 Shell 脚本（`.zprofile` / `.zshenv` / `.bash_profile`）与 `.app` 应用包的默认写保护；后台搜索索引自动跳过 macOS `/System`、`/Library`、`/Applications`、`/usr`、`/bin`、`/sbin`、`/private`、`~/Library`（避免触发 iCloud 与 App 容器隐私弹窗风暴）、照片图库、`/dev` 及非本地挂载卷，以及 Linux `/proc`、`/sys`、`/dev`、`/run`（不影响直接按路径读取）。
  - **优化 macOS 隐私权限（TCC）体验与接口报错**：预热索引前主动探测桌面 / 文稿 / 下载以消除弹窗间长等待；macOS 整机授权界面新增「打开完全磁盘访问权限设置」一键直达按钮；更新三语接入说明桌面表述，并在 `PermissionDenied` 时返回带操作引导的 HTTP 403。
- **修正文档中 Windows 沙箱边界描述并同步三语元数据**（[#11](https://github.com/Aenvo/MuseLink/pull/11)，感谢 [@KammyAugust11](https://github.com/KammyAugust11)）：更正 README 中沙箱超时机制与 `Everyone` / `RESTRICTED` SID 写入范围说明，并同步中 / 英 / 日三语 README 顶部简介、平台徽章与包元数据。

## [v0.3.0] - 2026-11-27

### 新增功能
- **应用内自动更新与内嵌更新日志**：支持启动静默检查、签名验签一键更新（Windows 安装版 & Linux AppImage），以及应用内查看同步更新日志。
  - **一键检查与重启更新**：通过 GitHub Releases 静态 `latest.json` 与编译期固化 Minisign 公钥验签（不走受限 REST API）；右下角弹出可关闭的新版本提醒，下载验签期间保持连接不断流并展示绿色进度条，确认安装后安全停服（含释放 Windows `Everything.exe` 文件锁）并自动重启；Windows 便携版、Linux `.deb` 与 macOS / 源码构建通过同一 `latest.json` 提供版本提醒、更新详情与下载直达。
  - **内嵌更新日志弹窗**：「关于」页提供「更新日志」「GitHub」「检查更新」入口，弹窗随界面语言自动切换、本地秒开并后台同步 GitHub 最新日志，标题栏附外链直达 GitHub。
  - **三平台自动发布流水线**：新增 `.github/workflows/release.yml` 与 `generate-updater-manifest.ps1`，打 `v*` 标签后并行构建与校验三平台产物并生成 `latest.json` 与 `SHA256SUMS.txt`。
- **支持 Linux 与 macOS（Apple Silicon & Intel）**（[#3](https://github.com/Aenvo/MuseLink/pull/3)、[#7](https://github.com/Aenvo/MuseLink/pull/7)，感谢 [@eightman999](https://github.com/eightman999)；解决 [#6](https://github.com/Aenvo/MuseLink/issues/6)）：现可在三平台使用完整文件 API 与 Tailscale / Cloudflare 隧道配置，并提供预构建安装包。
  - **Linux 支持**：覆盖文件 API（搜索 / 列表 / 读取 / 写入 / 建目录）与隧道配置，提供签名 AppImage 与 `.deb` 包；本机命令执行仍仅支持 Windows。
  - **macOS 支持**：提供 Universal `.dmg` 与 `.app.zip` 预构建包，自动发现 `Tailscale.app` 与 Homebrew 安装；未做 Apple 公证，首次打开需在「系统设置 → 隐私与安全性」中放行；本机命令执行仍仅支持 Windows。
- **新增日语界面（`ja`）**（[#8](https://github.com/Aenvo/MuseLink/pull/8)，感谢 [@eightman999](https://github.com/eightman999)）：界面文案、托盘、接入说明与 README 全面支持日语，可在「设置 → 常规」切换。

## [v0.2.2] - 2026-11-27

### 优化与新增
- **大幅提升文件读取上限，新增分片读取与原始二进制直传**（解决 [#4](https://github.com/Aenvo/MuseLink/issues/4)）：整文件读取上限提至 10 MiB，大文件与图片支持分片及无膨胀高速传输。
  - **整文件 JSON 读取上限提升至 10 MiB**：与写入上限对齐，现有配置即可直接读取高分辨率图片；超限返回 HTTP `413` 并附带分片与 `raw` 建议字段；旧版 `/api/read` 保持 2 MiB 兼容。
  - **支持 `offset` / `limit` 分片读取（无总文件大小硬上限）**：在同一已校验操作系统句柄上按字节偏移分段读取，JSON 默认 2 MiB（单片上限 4 MiB），返回 `etag` 并支持 `if_match` / `If-Match`（中途文件变更返回 HTTP `409`）。
  - **支持 `raw=1` 原始二进制直传**：直接返回原始字节流与对应 `Content-Type`，消除 Base64 33% 膨胀（经公网隧道实测提速约 40%，单次上限 32 MiB，支持与分片组合），附带 `ETag` 与 `X-File-*` 响应头。
  - **更新 Muse 接入说明（Prompt）**：同步指导 Muse 读取图片/二进制优先使用 `raw=1` 并对大文件按片重试。

## [v0.2.1] - 2026-11-25

### 优化与新增
- **桌面界面全面改版与信息架构重构**：统一控件视觉与间距，理顺连接、文件权限、日志与设置四大页面结构。
  - **组件视觉升级**：`Switch` 开启态升级为 `#48C52C` 绿色配白色滑块；`Input`、`Select` 与公网地址框统一为透明描边风格；日志搜索框内置放大镜图标；危险操作使用红色语义强调。
  - **连接页与首次配置重构**：顶部独立「指引 / 开始指引」卡片，设备名称改为行内布局，「Muse 接入说明」整合至连接页底部，首次配置第 3 步并排展示「复制接入说明」与「复制专用令牌」。
  - **文件权限与日志页聚焦**：「保存授权」提至卡片标题栏右侧；日志页将时间范围筛选提至一级单行筛选条，右上角聚焦单一「暂停同步 / 继续同步」按钮。
  - **设置页重组为「常规」与「数据」**：统一收纳本机数据、文件索引、日志数据与本机令牌维护入口。
  - **优化启动与托盘行为**：开机自启默认关闭，开启后通过 `--autostart` 静默启动到托盘，手动启动始终打开主窗口；新增可配置的「关闭时最小化到托盘」（默认开启）。
- **命令审批支持实时长轮询**：审批通过、拒绝或命令结束后立即返回结果，无需切回聊天手动通知 Muse。
  - `GET /agent/v1/exec/{job_id}` 新增 `?wait_ms=` 参数（默认 `0`、上限 `25000` ms、推荐 `20000` ms），异步等待不占用请求槽位与权限锁。
  - 同步更新 Muse 接入说明，指导持续轮询至终态并内置 `expired` / `denied` 与轮询上限兜底；审批倒计时归零后自动禁用按钮防误点。
- **新增可筛选日志导出与目录直达**：支持按范围、结果、时间与关键词导出脱敏后的 `JSONL` 或 `CSV`，并可一键定位本地日志文件。
  - 入口位于「设置 → 数据 → 日志数据」，支持「清除日志」「导出日志」与「打开目录」（定位 10 MiB 轮转日志），导出使用 UTF-8 BOM 并做双重令牌脱敏兜底。
- **移除不可靠的 Cloudflare 临时隧道并补齐 MIT 开源协议**：统一聚焦 Tailscale Funnel 与 Cloudflare 命名隧道。
  - 旧配置中曾保存的 `cloudflare_quick` 会自动平滑回退至 Tailscale Funnel；新增根目录 [LICENSE](LICENSE)（MIT License）并同步至包元数据、README 与发布包。

### 问题修复
- **修复后台孙进程继承管道导致命令任务卡在 `running` 的问题**：主进程退出后立即通过 Windows `JobObject` 回收残留子进程树，避免管道写端未关闭阻塞读取线程。

## [v0.2.0] - 2026-11-25

### 新增功能
- **受控本机命令执行（Windows，默认关闭）**：开启后允许 Muse 在已授权项目子目录内运行开发工具与受控 Shell，全程受写受限沙箱与本机审批保护。
  - 在「文件权限 → Muse 文件工具」新增总开关与风险确认窗，提供 `/agent/v1/exec`、状态查询与取消接口（禁止直接对盘符根目录执行）。
  - **每工作区独立 Windows 写受限沙箱**：基于 `CreateRestrictedToken(WRITE_RESTRICTED)` 与按工作区规范路径 SHA-256 派生的独立 SID，仅允许写入目标项目子目录与单任务临时目录，配合 `JobObject` 施加内存、进程数与超时限制，失败即拒不降级。
  - **三档风险分流与自动置顶审批窗**：常见只读及构建/测试命令自动放行；首次在项目内使用 Shell、变更依赖或 `git push` 时自动唤醒置顶审批窗（120 秒超时自动拒绝，支持单次允许或按项目 4 小时会话免打扰）；触碰敏感目录、跨目录（`..`）、强推、强制删除、系统管理工具、混淆脚本或 `unsandboxed` 时强制单次确认。
- **设备命名与改名回执**（[#1](https://github.com/Aenvo/MuseLink/pull/1)）：支持多台电脑设置独立设备名称，自动生成改名说明与 Muse 回执确认流程。
- **日志筛选与关于页更新日志入口**：日志页新增「命令执行」筛选，「关于」页新增「更新日志」直达入口。

### 安全与稳定性加固
- **句柄级文件校验与敏感路径分级保护**（[#2](https://github.com/Aenvo/MuseLink/pull/2)）：读取与原子写入全程经打开的 OS 句柄核对真实路径，防符号链接/联接点逃逸与 TOCTOU 替换，并默认保护凭证与系统关键路径。
  - 默认屏蔽 `~/.muse-bridge`、`~/.ssh`、`~/.aws`、`~/.gnupg` 凭证目录，并对系统目录、启动项、Shell Profile、`.gitconfig` 及程序自身目录启用写保护。
  - 通过 Windows `JobObject` 托管 `Everything.exe`、`cloudflared` 与命令子进程，退出或关闭时回收整棵进程树。
  - 审计与运行日志自动脱敏 `mfa_` 令牌；切换文件夹/磁盘授权时保留已有边界；修复 base64 写入 10 MiB 校验、收紧 CSP、升级 `url` 修复 `RUSTSEC-2026-0592`；未开启命令执行时统一返回 HTTP 403 `exec_disabled`。

### 工程与测试
- **新增 Windows CI 自动化测试工作流**：覆盖格式化门禁、前端构建、Rust 单元测试、符号链接/联接点逃逸与 Windows 内核沙箱测试。

## [v0.1.0] - 2026-11-24

### 首个版本发布
- **首次配置引导与文件授权**：支持指定文件夹或本机固定磁盘授权，后台建立 Everything 文件名索引并支持按需重建。
- **Muse 专用令牌与文件接口**：提供独立 `/agent/v1` 接口与 `agent-token`（生成、复制、定位、轮换、撤销），支持搜索、列目录、读取、写入与建目录。
- **公网隧道、托盘管理与 Windows 分发**：默认支持 Tailscale Funnel（可选 Cloudflare 隧道），提供系统托盘控制、日志统计、中英双语界面及便携版 ZIP / NSIS 安装包。
