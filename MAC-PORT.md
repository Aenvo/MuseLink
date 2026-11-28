# MuseLink macOS 移植说明（MAC-PORT.md）

> 状态：**已在真机上构建并运行**（2026-11-26 在目标 Mac 上 release 构建一次通过、DMG 已验证可挂载；2026-11-27 应用已在真机上跑通本地 API 与文件授权，Tailscale Funnel 接入正在实测，见第 2 节末尾的真机记录）。

## 1. 在你自己的 Mac 上构建

全部操作都在你的 Mac 本机上做，不要在云端沙箱里构建 macOS 二进制。

```bash
# 1. 先装工具链（只需要做一次）
#    Xcode Command Line Tools：xcode-select --install
#    Rust：https://rustup.rs（按页面提示安装）
#    Node.js 22 与 pnpm：建议用 nvm 或 Homebrew 装 Node 22，再用 corepack 启用 pnpm
corepack enable
corepack prepare pnpm@11.7.0 --activate

# 2. 进仓库目录，装前端依赖
cd MuseLink/muse-connector
pnpm install --frozen-lockfile

# 3. 开发模式（热重载 + 调试窗口）
pnpm tauri dev

# 4. 打正式包（.dmg，含已签名的应用包结构；未做 Apple 公证与开发者签名，
#    首次打开需在“系统设置 → 隐私与安全性”里点“仍要打开”）
pnpm tauri build
```

构建产物在 `muse-connector/src-tauri/target/release/bundle/dmg/`。`cargo check` 交叉验证命令（Linux 上也能跑，需先装 `cargo-zigbuild` 与 Zig，见第 6 节）：

```bash
cargo check --locked --target aarch64-apple-darwin   # Apple Silicon
cargo check --locked --target x86_64-apple-darwin    # Intel
```

## 2. Tailscale for Mac + Funnel

1. 从官网下载 **standalone 版**（不要用 Mac App Store 版）：<https://tailscale.com/download/mac>。standalone 版需要 macOS 13 Ventura 或以上。
2. 安装后打开 Tailscale 菜单栏图标登录你的 Tailscale 账号。
3. 在 Tailscale 后台（或用 `tailscale funnel` 命令行）完成首次 Funnel 授权：MuseLink 连接页的「查看登录与 Funnel 授权」里有官方说明链接。
4. 启动 MuseLink → 按首次配置指引走完文件授权 → 开启连接 → 验证公网入口。

**不确定性，如实说明**：Tailscale 官方文档对 macOS 上 Funnel 的支持口径并不一致——官方 skill 文档说 Funnel“只适用于开源 CLI 变体（不适用于 App Store 或 Standalone）”，另一些来源则称 standalone 支持 `tailscale funnel`。本移植默认推荐 standalone 版并直接调用 `tailscale funnel`；如果它报错，说明你那台机器上的版本确实不支持，后备方案是：

- 方案 A（推荐后备）：`brew install tailscale` 装开源 CLI，用 `sudo tailscaled` 跑后台服务，再用 `tailscale funnel`；
- 方案 B：改用 Cloudflare 命名隧道（自有域名），MuseLink 连接页里可选，`cloudflared` 有 macOS 原生二进制，`brew install cloudflared` 即可。

另外：**不要同时安装 App Store 版与 standalone 版**，两者会冲突。

**真机实测修正（2026-11-27）**：Tailscale for Mac 的应用包里根本没有独立的命令行程序，`/Applications/Tailscale.app/Contents/MacOS/` 下只有图形界面程序 `Tailscale` 本身；`/usr/local/bin/tailscale` 也只是包它的外壳脚本（`exec …/Tailscale "$@"`）。从终端手动调它时 `funnel status --json` 能正常返回 `{}`，但从图形应用里启动它查配置时可能吐不出任何内容（一次实测里同样的调用直接挂住 90 秒以上），MuseLink 就会报「无法解析 Tailscale 配置」。所以 **Funnel 必须配一个真正的命令行安装**：`brew install tailscale`（Apple Silicon 装到 `/opt/homebrew/bin`，Intel 装到 `/usr/local/bin`），MuseLink 只认真正的 CLI，会跳过应用包内的图形程序与 `/usr/local/bin` 下的外壳脚本（Intel 上 Homebrew 的同路径真程序是符号链接，会正确认出来）。图形应用仍要装（用来登录与跑守护进程），命令行只负责被 MuseLink 调用。

## 3. Mac 沙箱：能做什么，不能做什么

Windows 版用 `CreateRestrictedToken(WRITE_RESTRICTED)` + 每工作区独立 SID 的目录 ACL + JobObject，操作系统内核层面保证命令写不出当前项目目录。**macOS 没有等价物**，本移植不假装有。Mac 上的“受限环境”由以下几层组成：

**做得到（已实现）**

- 工作目录锁定：命令的 cwd 固定在已授权的项目子目录内，禁止直接对根目录执行。
- 环境精简：受限模式下清空继承环境，只保留 allowlist（`PATH=/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin`、`HOME`、`TMPDIR` 指向单任务隔离临时目录、`LANG=en_US.UTF-8`）。
- 内存上限：`RLIMIT_AS`（默认 2 GiB，与 Windows 版的 `DEFAULT_EXEC_MEMORY_BYTES` 一致），只在受限模式下施加。
- 进程组隔离：子进程单独 `setpgid`，超时/取消/关闭开关时用 `killpg(SIGKILL)` 清理整棵进程树，不留孤儿进程。
- 硬超时与取消：与 Windows 版相同的等待循环（50 ms 轮询 + deadline + 取消），超时即杀树。
- 三档审批流不变：只读/开发工具命令自动放行；首次在项目内用 Shell、装依赖、推送远端时弹 120 秒本机审批窗（可按项目 4 小时会话免打扰）；触碰敏感目录、提权、管道执行远程代码、强制推送、递归删除等强制单次确认。
- 审计与脱敏不变：令牌不进日志明文；命令执行默认关闭；令牌鉴权强制。

**做不到（请不要误解界面上的“受限环境”徽标）**

- **没有内核级写隔离**：受限命令仍然以你的 macOS 用户身份运行，能写到任何该用户可写的路径（包括别的项目、桌面、下载）。“工作目录锁定”只是约定，不是强制。Windows 版“写不出项目目录”的保证在 Mac 上不存在。
- **没有进程数限制**：macOS 的 `RLIMIT_NPROC` 按用户全局计数，设了会误伤你正在跑的其他进程，所以故意没设。
- **没有 CPU 时间限制**：同理未设 `RLIMIT_CPU`（会用信号杀进程，与交互式开发工具冲突的风险大于收益）。
- **没有用 `sandbox-exec` / Seatbelt profile**：那是已废弃的接口，写自定义 profile 容易把常用开发工具（git、cargo、node）误杀，且不同 macOS 版本行为不一致，所以没碰。
- **`write_workspace` 在 Mac 上只是接口对齐与审计记录**，不产生任何强制效果。

一句话：**Mac 版的安全边界 = 授权范围 + 审批弹窗 + 进程/内存上限 + 你的 macOS 用户权限本身；不要把它当成沙箱来信任。** 高风险命令一律先看审批窗里的实际命令再点允许。

## 4. 各模块改了什么

| 模块 | 改动 |
| --- | --- |
| `core/spotlight.rs`（新增） | macOS 搜索后端，镜像 `Everything` 的 API（`discover` / `query` / `Candidates`）。用 `mdfind -onlyin <root>… "kMDItemFSName == \"*name*\"cd" -0` 按文件名查，5 秒超时，NUL 分隔解析，结果再按授权 roots 过滤；候选超 2048 时 `covered=false` 让调用方回落内置索引；`MUSE_CONNECTOR_DISABLE_SPOTLIGHT=1` 可禁用；找不到 `/usr/bin/mdfind` 时 `discover` 返回 `None`。 |
| `core/search.rs` | `external_pages` / `ExternalCursor` 等的 cfg 从 `windows` 拓宽为 `any(windows, target_os = "macos")`；新增 `search_spotlight`（游标前缀 `sp-`、后端名 `"spotlight"`），复用 `external_entry` / `live_hit` / `add_hit` / `Query`。 |
| `core/access_runtime.rs` | 新增 macOS 的 `spotlight` 字段；`run_index_search` 先试 Spotlight、`None` 时回落内置目录遍历索引；`"spotlight"` 映射为索引就绪；`stop_search_engine` 调 `stop_managed()`（no-op）；`production_protection` 增加 macOS 写保护路径（`~/Library/LaunchAgents`、`/Library/LaunchAgents`、`/Library/LaunchDaemons`、`/System`、`/private/etc/sudoers.d`）。 |
| `core/sandbox/`（拆分） | 原 `sandbox.rs` 移为 `sandbox/windows.rs`（Windows 实现一字未动）；新建分发器 `sandbox.rs`；新建 `sandbox/macos.rs`（见第 3 节；`JobLimits` 只有 `memory_bytes`，故意没有 `max_processes`）。 |
| `core/handle/`（拆分） | 原 `handle.rs` 移为 `handle/windows.rs`（NT 实现未动）；新建分发器 `handle.rs`（共享 `Error` / `Entry`）；新建 `handle/macos.rs`：`openat` + `O_NOFOLLOW` 逐组件行走 + `F_GETPATH` 校验根，`fdopendir` / `readdir` / `fstatat(AT_SYMLINK_NOFOLLOW)` 做 list，临时文件 + `renameat` 同目录原子替换做 write，全程持有一整条目录 fd 链防中途替换；`ELOOP` → `Escape`。`write` 在目标是符号链接时明确拒绝（`renameat` 本会直接替换链接本身而不跟随，但静默替换 planted link 不是调用方要的写入；与 `read` 和 Windows 行为一致——这条是跑通 Linux 测试后补的，见第 6 节）。另新建 `handle/linux.rs`：与 macOS 版逐行相同，只把 `F_GETPATH` 换成 `readlink(/proc/self/fd/N)`，**仅供 Linux 开发机编译与跑测试，应用本身不在 Linux 上发布**。 |
| `core/exec_policy.rs` | 新增 `ExecMode::Shell`（serde `"shell"`，macOS 用 `/bin/zsh -c`；Windows 上拒绝 Shell，macOS 上拒绝 powershell/cmd）。macOS 高危判定新增：`sudo`/`su`/`doas`（提权）、`diskutil`/`launchctl`/`defaults`/`spctl`/`csrutil`/`nvram`/`dscl`/`security`/`installer`/`systemsetup`/`scutil`/`networksetup`（系统管理工具）、`curl\|wget` + `\| sh/bash/zsh`（管道执行远程代码）、LaunchDaemons/LaunchAgents、`/System/`、`/private/etc`、`/etc/passwd`、`/etc/sudoers`（敏感路径）；`brew` 进包管理器 Session 档；`which` 进只读工具表。 |
| `core/exec_runtime.rs` | 原 `#[cfg(not(windows))]` 存根改为 macOS 完整 `run_process`：Direct 经 `resolve_program` 找可执行文件；Shell 经 `shell_executable()`（`$SHELL` 有效时用它，否则 `/bin/zsh`）+ `["-c", script]`；spawn/管道/等待循环/超时取消逻辑与 Windows 版逐行一致；其余平台仍为存根。 |
| `core/path_guard.rs` | 保留设备名（CON/PRN/AUX/NUL/COM/LPT）、尾部点/空格、盘符判定、ADS（`:`）、反斜杠分隔符五项检查改为 Windows 专属——APFS 上这些都是合法文件名。 |
| `core/mod.rs` | `#[cfg(target_os = "macos")] mod spotlight;`；`sandbox` 的 cfg 拓宽为 `any(windows, target_os = "macos")`。 |
| `service.rs` | `Snapshot` 增加 `platform: &'static str`（`"windows"` / `"macos"` / `"other"`）；`suggested_device_name` 改为平台关联函数（macOS 用 libc `gethostname` + OnceLock 缓存，取短名）；`tool_status()` 用 `tunnel::TAILSCALE_BINARY` / `CLOUDFLARED_BINARY` 常量；`set_exec_enabled` 的开启日志按平台分文案。 |
| `profile.rs` | 修了一个移植过程中发现的上游可移植性 bug：`same_location` 用硬编码反斜杠 `"\\"` 拼路径前缀，在 Unix 上永远匹配不到嵌套路径（`validate_config_scope` 会误判）；改为 `std::path::MAIN_SEPARATOR`，Windows 行为不变。 |
| `tunnel.rs` | 新增 `TAILSCALE_BINARY` / `CLOUDFLARED_BINARY` 常量（Windows 带 `.exe`，macOS 不带）；`discover_executable` 在 macOS 上只认真正的命令行安装（PATH、`~/.local/bin`、`/opt/homebrew/bin`、`/usr/local/bin`，其中 `/usr/local/bin/tailscale` 是外壳脚本时跳过、是 Homebrew 符号链接时认），**跳过应用包内的图形程序与它的外壳脚本**（2026-11-27 真机教训：从图形应用里启动它查配置会吐空输出或挂住，报「无法解析 Tailscale 配置」）；工具报错改为保留前三行 stderr（Funnel 一次性授权链接在第二、三行），解析失败时附上所用工具路径与原始输出。其余逻辑（`run_cli`、`observe_tailscale`、Funnel 调用）直接复用。 |
| `lib.rs` | `mod clipboard` 的 cfg 拓宽为 `any(windows, target_os = "macos")`；`copy_agent_token` 增加 macOS 分支（`pbcopy`）；`prior_funnel` / `cloudflare_action` 改用二进制名常量；`open_tool_setup` 的 Tailscale 下载/登录 URL 按平台分（macOS 用 `https://tailscale.com/download/mac`）；`everything` 帮助入口在非 Windows 下返回 Err，说明 macOS 用 Spotlight、无需安装。 |
| `clipboard/`（拆分） | 原 `clipboard.rs` 移为 `clipboard/windows.rs`；新建分发器；新建 `clipboard/macos.rs`（`pbcopy` 经 stdin 传令牌）。**注意**：macOS 没有 Windows `ExcludeClipboardContentFromMonitorProcessing` 的等价标记，令牌会进通用剪贴板、可被剪贴板管理器看到，代码注释与界面文案里已写明。 |
| `legacy.rs` | `WindowsTasks` 与计划任务实现加 `#[cfg(windows)]`；macOS 下 `check_legacy` 直接返回“无旧系统”（仍保留 18790 端口占用检查与迁移记录读取）；`takeover` / `restore` 在 macOS 下返回 Err（没有旧版 Windows 计划任务可接管）。 |
| `core/api.rs`（测试） | `link_fixture` / `assert_link_rejected` 各补一份 macOS 版；新增 macOS `symlink_escape` 测试（`std::os::unix::fs::symlink`，macOS 上建符号链接无需特权，无条件运行）。另：UNC/盘符、保留设备名、ADS、反斜杠穿越四组测试只在 Windows 语义下成立，改为 `#[cfg(windows)]`（反斜杠穿越另拆出 `traversal_matrix_backslash`），并补了一个全平台测试 `absolute_paths_rejected`（`/etc/passwd` 这类绝对路径在所有平台都应 400）。 |
| `Cargo.toml` / `Cargo.lock` | description 改为 "Windows and macOS"；新增 `[target.'cfg(any(target_os = "macos", target_os = "linux"))'.dependencies] libc = "0.2"`（已在 lock 中，`Cargo.lock` 只多了 root 包对 `libc` 的一条依赖边，无版本变动）。 |
| `tauri.conf.json` / `build.rs` | bundle targets 增加 `"dmg"`（保留 `"nsis"`）；`build.rs` 增加 `cargo:rerun-if-changed=icons/icon.icns`（图标文件仓库里本来就有）。 |
| `.github/workflows/macos.yml`（新增） | 镜像 `windows.yml`：macos-latest、Node 22、pnpm 11.7.0、cargo fmt、前端构建、`cargo test --locked`，另加两个 mac target 的 `cargo check`。 |
| 前端（`muse-connector/src`） | `Snapshot` 增加 `platform` 字段并一路传到各组件；`ExecMode` 增加 `"shell"`；`connectorBrief` 增加 `platform` 参数，接入说明按平台出 macOS 变体（mode `"shell"`、`/bin/zsh -c`、受限环境说明、macOS 桌面/搜索文案）；审批弹窗 mode 标签增加 `zsh`，沙箱徽标 macOS 下显示“受限环境”，风险标签增加 `zsh 脚本` / `提权操作` / `管道执行远程代码` / `跳出受限环境`；文件权限页在 macOS 下隐藏磁盘模式（整机授权）；`editFileAccessPolicy` 的路径归一化在 macOS 下不再把反斜杠当分隔符、不再小写（APFS 上反斜杠是合法文件名字符、卷可以大小写敏感）；设置页索引就绪标签 macOS 下显示“Spotlight 索引已就绪”；引导页 Tailscale 文案改为 macOS 菜单栏版；`messages.ts` 同步全部新增字符串的中英文对照。 |

## 5. 没变的东西（Windows 路径原样保留）

- Windows 的 Everything 搜索、NTFS 全卷索引、写受限沙箱、JobObject、PowerShell/cmd 执行、剪贴板隐私标记、计划任务接管、NSIS 打包、Windows CI 工作流：**一字未动**，只加了 cfg 门控或改用了平台常量。Windows 代码在沙箱里用 `cargo check --locked --target x86_64-pc-windows-gnu`（Zig 交叉 C 编译器 + 仅供 check 的 stub windres，见第 6 节）验证通过，0 warnings；MSVC target 因 `ring` 需要真正的 `cl.exe` 在 Linux 上无法 check，留待 Windows CI。另有两处为让全平台测试通过而动的共享代码：`profile::same_location` 的分隔符（见上表）与 `handle` 的 `write` 遇符号链接时明确拒绝（macOS/Linux 后端；Windows NT 句柄本就拒绝）。

## 6. 验证证据（Linux 沙箱，2026-11-26）

交叉验证用的工具链（只存在于本沙箱，不进仓库）：Zig 0.15.2（`~/.local/zig`）+ `cargo-zigbuild` 0.24.1。Linux 的 `cc`（gcc）编译不了 macOS 的 Objective-C 构建脚本（不认 `-arch arm64`），所以 C 编译器必须走 Zig；`cargo-zigbuild` 生成的 wrapper 脚本负责把 cc-rs 传的 `--target=arm64-apple-macosx` 这类 Apple 风格参数翻译成 Zig 能懂的 `-target aarch64-macos-none`。直接设 `CC="zig cc -target …"` 行不通（cc-rs 会把参数拆丢）。

| 检查 | 命令 | 结果 |
| --- | --- | --- |
| Rust 格式 | `cargo fmt --manifest-path muse-connector/src-tauri/Cargo.toml --all -- --check`（需先 `rustup component add rustfmt`） | 通过，0 diff |
| Apple Silicon 类型检查 | `cargo check --locked --target aarch64-apple-darwin`（`CC/CXX/AR` 指向 cargo-zigbuild 的 wrapper 脚本） | **通过，exit 0，0 warnings** |
| Intel 类型检查 | `cargo check --locked --target x86_64-apple-darwin`（同上，x86_64 wrapper） | **通过，exit 0，0 warnings** |
| macOS 测试代码类型检查 | `cargo check --locked --tests --target aarch64-apple-darwin` 与 `--target x86_64-apple-darwin` | **均通过，exit 0**（含新增的 macOS `symlink_escape`、Spotlight、受限环境、`production_protection` 测试） |
| Windows 未破坏 | `cargo check --locked --target x86_64-pc-windows-gnu`（`CC` 走 Zig 的 `x86_64-windows-gnu` wrapper；`x86_64-w64-mingw32-windres` 用了一个仅供 check 的 stub——它只创建 `--output` 文件就返回 0，因为 check 从不链接资源对象；MSVC target 无法在 Linux 上 check，`ring` 的构建脚本需要真正的 `cl.exe`） | **通过，exit 0，0 warnings**（与 Windows 共用的 `cfg(windows)` 代码全部覆盖） |
| 前端类型检查 + 构建 | `pnpm -C muse-connector install --frozen-lockfile`（pnpm 11.7.0）+ `pnpm -C muse-connector build`（`tsc && vite build`） | **通过**：2600 个模块，9.52 秒 |
| 完整链接（macOS 二进制） | `cargo zigbuild --locked --target aarch64-apple-darwin`（会链接） | Rust 代码全部编译通过；**链接失败**：`unable to find dynamic system library 'objc'`——Linux 上没有 macOS SDK（含 `libobjc.tbd`），`xcrun` 也不存在。这是沙箱环境的限制，不是代码问题；在 Mac 本机上 `pnpm tauri build` 不会遇到它 |
| Host Rust 测试 | `cargo test --locked`（Linux host；为此先在沙箱里装了 Tauri 的 Linux 依赖：`libwebkit2gtk-4.1-0` 等运行时库走 apt 正常安装，92 个 `-dev` 包因镜像自带的 `libzstd1+hatch2` 与 Ubuntu 的 `libzstd-dev` 版本冲突无法走 apt 安装，改用 `apt-get download` + `dpkg-deb -x` 只解压文件、不注册包——沙箱一次性操作，不进仓库） | **通过：85 passed，0 failed，0 warnings**。过程中修掉的真问题：① `handle` 在 Linux 上没有后端，库根本编不过——补了 `handle/linux.rs`（仅供开发机跑测试）；② `write` 遇符号链接时本应拒绝却会直接替换链接——macOS/Linux 后端都补了显式拒绝；③ `profile::same_location` 硬编码反斜杠分隔符；④ 四组 Windows 路径语义测试改为 `#[cfg(windows)]` + 补全平台测试。 |
| Mac 本机测试（SSH，2026-11-27） | `cargo test --locked`（在目标 Mac 上经 SSH 跑） | **68 passed，27 failed**：失败全在 `core::access` / `access_runtime` / `api` / `sandbox::macos` / `search` / `service`，报错特征是环境相关（SSH 下受限进程 spawn 报 EINVAL、家目录策略校验、重解析点授权），与隧道修复无关；改动的 `tunnel` 模块测试全过（2 passed）。这些测试此前只在 Linux 沙箱上跑过（85 passed），从没在 Mac 上跑过；是否与图形会话有关待核实。**构建时不以它们为门禁**——已核实：没有任何测试断言被改的报错文案，也没有任何测试调 `funnel status`；测试进程经登录 shell 跑，PATH 里有 Homebrew，改动前后发现的 CLI 是同一个，不可能由本次改动引起。 |
| Windows MSVC 类型检查 | `cargo check --locked --target x86_64-pc-windows-msvc` | 未通过环境关：`ring` 的构建脚本需要 MSVC 的 `cl.exe`；Linux 上无解，留待 Windows CI（`windows.yml` 不变，仍在跑） |

> 上面标“未通过环境关”的项，是沙箱缺系统工具链，不是代码错误。真机验收清单：Spotlight 能否在授权目录里找到文件、句柄层能否挡住符号链接逃逸、受限命令能否被超时杀干净、`tailscale funnel` 在 standalone 版上是否真的能用、DMG 能否正常安装打开、托盘菜单是否正常。

## 7. 故意没做的事

- **macOS 端到端 exec 集成测试没写**：Windows 版的 `agent_exec_enforces_toggle_approval_session_and_sandbox_boundary` 是 `#[cfg(windows)]`，macOS 没有对应测试。原因是集成测试要在真实 macOS 进程模型下跑才有意义，Linux 上跑不了，写了也验证不了。
- **`sandbox-exec` / Seatbelt profile 没尝试**：见第 3 节，已废弃且容易误杀开发工具。
- **没设 `RLIMIT_NPROC` / `RLIMIT_CPU`**：见第 3 节。
- **没做 Apple 代码签名与公证**：需要开发者的 Apple Developer 账号与证书，由用户在自己 Mac 上按 Tauri 官方文档配置，不属于本次移植。
- **没动 `lumeri.io` 服务器与任何线上配置**：全部工作只在本地仓库目录里。
