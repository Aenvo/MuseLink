# MuseLink

[简体中文](./README.md)

<img src="./assets/muselink-logo.png" alt="MuseLink app icon" width="200">

MuseLink is a Windows desktop connector that lets Muse search, read, and write local files within the scope you authorize. Grant access to specific folders or select local drives. The app guides you through setting up an HTTPS endpoint, generating a dedicated token, and running your first file-access test.

## Get started

MuseLink is currently available to build from source. You'll need Windows, WebView2, and a tunnel client of your choice. Tailscale Funnel is the default. Install [Tailscale for Windows](https://tailscale.com/download/windows) and sign in, then follow the in-app guide to authorize Funnel.

1. Launch the app and select **Start setup** on **Overview**.
2. Choose the folders or drives to authorize. The drive list defaults to all available local fixed drives. Expand the list to narrow the selection. Once saved, Muse can search, read, and write within that scope.
3. Generate a Muse token, start the connection, and verify the public endpoint.
4. Send the generated connection instructions to Muse. Open the security card from Muse and paste the dedicated token into it.
5. Test with a non-sensitive file. Check the contents returned by Muse and finish setup.

Use **Settings → Display language** to switch between Simplified Chinese and English. The app remembers your choice across restarts.

## Connections and file access

- **Access scope** covers selected folders and their subfolders, or selected local fixed drives. Reads and writes are also subject to the current Windows account's permissions.
- **File search** uses a background index. If indexing is still running after you save permissions, try again later. You can check or rebuild the index in **File Access**.
- **Public forwarding** supports Tailscale Funnel, Cloudflare quick tunnels, and Cloudflare named tunnels. A named tunnel requires an available domain in your Cloudflare account. Update the connection in Muse if switching providers changes the public URL.
- **Connection controls** let you start or pause the connection and check its status and access logs. Closing the window sends the app to the system tray. Use the tray to reopen or quit the app.

## Credentials and security

The dedicated token grants access to the files you authorize. Enter it only in the Muse security card. To stop access, pause the connection or revoke the token under **File Access → Muse file tools**. Update the security card after rotating the token. After the public health check passes, run a file operation in Muse to verify the integration.

## Develop and build

You'll need Node.js 22.12 or later, pnpm, the Rust MSVC toolchain, Visual Studio C++ Build Tools, and WebView2. Run these commands from the repository root to start development.

```powershell
pnpm -C muse-connector install --frozen-lockfile
pnpm -C muse-connector tauri dev
```

Development mode uses separate local settings and a simulated connection. These commands build the frontend, run the Rust tests, and build a Windows NSIS installer locally.

```powershell
pnpm -C muse-connector build
cargo test --manifest-path muse-connector/src-tauri/Cargo.toml --locked
pnpm -C muse-connector tauri build --bundles nsis
```

The installer is written to `muse-connector/src-tauri/target/release/bundle/nsis/`. Run `pnpm -C muse-connector tauri build --no-bundle` to build `muse-connector/src-tauri/target/release/muse-connector.exe`.

## Acknowledgments

MuseLink's legacy file API is based on the [muse-file-bridge](https://github.com/TTNAN/muse-file-bridge) v0.3 protocol. See [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md) for the upstream MIT license notice.
