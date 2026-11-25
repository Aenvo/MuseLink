# MuseLink Windows 候选验收

候选为 `b5fcc273d95ee3f1f1d9ef3a09963eaaa314acd6`，应用版本 `0.1.0`。本机在 Windows 10 x64（19045）上验收。GitHub Windows CI 通过 85 项 Rust 测试和 1 项 TCP 契约测试，含长路径、目录联接和符号链接用例。[Windows CI](https://github.com/Aenvo/MuseLink/actions/runs/45406667057)

## 构建与安装

- 便携 ZIP 和 NSIS 安装包通过 SHA-256 与内容校验。两个包均包含主程序、Everything、ES 和第三方许可。Everything 与 ES 的 Authenticode 签名有效，MuseLink 主程序和 NSIS 安装包为未签名候选。
- 便携版从 ZIP 解压后成功启动。在隔离配置中确认首次保存所选文件夹时默认允许搜索、读取和写入，中文路径写入成功。暂停释放了本地监听，重新开启及重启后可再次读取样本。令牌轮换后旧令牌返回 `401`、新令牌返回 `200`，撤销后返回 `401`。
- 当前用户静默安装旧版 `0.1.0` 后保存独立样本授权并生成测试令牌。以同版本候选覆盖后，安装的主程序匹配候选负载，授权配置和令牌逐字节保留。静默卸载后核对了候选主程序和卸载注册项的移除。
- 官方 Everything 命名 IPC 用例通过，覆盖 Unicode 文件名、目录作用域与分页。

| 候选产物 | SHA-256 |
| --- | --- |
| `MuseLink-0.1.0-b5fcc27-windows-x64-portable.zip` | `c427b4f0a75fea416fa4ad0c8c570c7ea7b95a03260b756f42781179c0f7d6c1` |
| `MuseLink-0.1.0-b5fcc27-windows-x64-setup.exe` | `9e089ec43570293598803bfe728264d9c9693a70977202d556d6799207a10930` |

## 真实公网连接

在暂停的测试配置中选定 Cloudflare 临时隧道，再由已安装的候选程序开启连接。通过公网 HTTPS 调用 `/agent/v1`，在唯一授权的隔离样本目录中完成搜索、列目录、读取、建目录及中文文件写入，并核对了落盘内容。无令牌返回 `401`，未知目录别名返回 `403`。暂停后确认 `cloudflared` 进程和本机监听均已停止，公网地址随后返回 `502`。

## Muse 端到端复验

2026-11-25 使用 `eb8375f` 的服务代码在独立验收实例上复验。Muse 通过安全卡片接入 Tailscale Funnel，只授权一个样本目录。Muse 发起搜索、读取、创建目录、写入、回读和列目录，六次鉴权请求均返回 `200`。Muse 返回的源文件内容与本机一致，写入回执实际落盘并回读为 `MUSE-E2E-OK`。

暂停后确认验收端口停止监听、Funnel 映射关闭、专用令牌撤销。Muse 两次重新读取均超时，且本机审计没有收到对应请求。

## 待确认

- `To be confirmed`。Cloudflare 命名隧道，以及打包版应用内选择提供方和启停 Tailscale Funnel。
- `To be confirmed`。交互式安装、跨版本升级、全新系统上的 WebView2 安装、通过托盘正常退出和 Everything 子进程回收。
