# MuseLink · Muse连接器

[English](./README.en.md)

<img src="./assets/muselink-logo.png" alt="MuseLink 应用图标" width="200">

MuseLink 是一款 Windows 桌面连接器，让 Muse 在你授权的范围内搜索、读取和写入本机文件。你可以授权指定文件夹，也可以选择本机磁盘。应用会引导你配置 HTTPS 入口、生成专用令牌并完成第一次文件访问测试。

## 开始使用

MuseLink 目前可从源码构建。使用时需要 Windows、WebView2 和你选择的隧道工具。默认连接方式是 Tailscale Funnel。安装 [Tailscale for Windows](https://tailscale.com/download/windows) 并登录，后续授权可按应用内引导操作。

1. 启动应用，在「概览」点击「开始引导」。
2. 选择授权文件夹或磁盘。磁盘列表默认选择所有可用的本机固定磁盘，展开后可调整。保存后，Muse 可在所选范围搜索和读写。
3. 生成 Muse 专用令牌，启动连接并验证公网入口。
4. 把应用生成的接入说明发给 Muse。打开 Muse 发来的安全卡片，把专用令牌粘贴到卡片里。
5. 用一份无敏感内容的文件试读，检查 Muse 返回的内容并完成引导。

在「设置 → 界面语言」可以切换简体中文或 English。语言选择会保留到下次启动。

## 连接和文件权限

- **授权范围**支持指定文件夹及其子目录，或选定的本机固定磁盘。实际读写还受当前 Windows 账户权限约束。
- **文件搜索**依赖后台索引。刚保存授权时，索引可能还在建立，稍后可重试。索引状态和重新建立入口在「文件权限」。
- **公网转发**支持 Tailscale Funnel、Cloudflare 临时隧道和 Cloudflare 命名隧道。命名隧道需要 Cloudflare 账户下的可用域名。切换后的公网地址若有变化，需要在 Muse 更新连接。
- **连接管理**可在应用中启动或暂停连接，查看连接状态和访问日志。关闭窗口会收进系统托盘。可以从托盘重新打开或退出应用。

## 凭证与安全

专用令牌可用于访问你已授权的文件。请只把令牌填进 Muse 的安全卡片。需要停用访问时，可以暂停连接，或在「文件权限 → Muse 文件工具」撤销令牌。轮换令牌后，要更新安全卡片。公网健康检查通过后，仍要在 Muse 实际执行一次文件操作，确认接入成功。

## 开发与构建

需要 Node.js 22.12 或更新版本、pnpm、Rust MSVC 工具链、Visual Studio C++ Build Tools 和 WebView2。从仓库根目录运行下面的命令启动开发环境。

```powershell
pnpm -C muse-connector install --frozen-lockfile
pnpm -C muse-connector tauri dev
```

开发模式使用独立的本机配置和模拟连接。以下命令分别用于构建前端、运行 Rust 测试和构建 Windows NSIS 安装包。

```powershell
pnpm -C muse-connector build
cargo test --manifest-path muse-connector/src-tauri/Cargo.toml --locked
pnpm -C muse-connector tauri build --bundles nsis
```

安装包输出在 `muse-connector/src-tauri/target/release/bundle/nsis/`。使用 `pnpm -C muse-connector tauri build --no-bundle` 可生成 `muse-connector/src-tauri/target/release/muse-connector.exe`。

## 致谢

MuseLink 的旧版文件接口基于 [muse-file-bridge](https://github.com/TTNAN/muse-file-bridge) v0.3 协议。上游 MIT 许可证声明见 [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)。
