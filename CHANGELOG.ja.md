# 変更履歴

[简体中文](CHANGELOG.md) · [English](CHANGELOG.en.md) · [日本語](CHANGELOG.ja.md) · [README に戻る](README.ja.md)

このファイルは MuseLink の機能追加、セキュリティ強化、不具合修正を記録し、リポジトリのプッシュおよびリリースと同期して更新されます。

## [Unreleased]

### 不具合修正と改善
- **起動時の自動アップデート確認が途中でキャンセルされ新バージョン通知が表示されない不具合を修正**：macOS / Windows / Linux の全プラットフォームで起動時に自動で最新リリースを確認して通知するようになり、トレイ常駐中やスリープ／オフライン復帰後もクールダウン周期に従って再確認します。
  - フロントエンドの `useUpdater` が `snapshot` 全体へ依存したまま 4 秒タイマー実行前に `autoChecked` を立てていた競合を修正（起動直後の `bridge-snapshot` 通知や 1.5 秒のインデックスポーリングでクリーンアップが走り、タイマーが破棄されて初回確認が永久にスキップされていました）。
  - 純粋関数のスケジュール制御（`updater-schedule.ts`）を切り出し、ウィンドウ表示状態・クールダウン（成功後 6 時間／サイレント失敗後 10 分）・`online` 復帰再試行・フォーカス復帰確認を統合。トレイ格納中はタイマーを動かさず、サイレント確認失敗で既知の更新通知を消去せず、サイレント確認中の手動「アップデートを確認」は進行中リクエストを再利用して手動フィードバックへ昇格します。
  - 上部ナビゲーションの「このアプリについて」タブに新バージョン通知ドットを追加し、`updater.rs` に非 200 ステータス・読取中断・マニフェスト解析失敗の警告ログと、開発ビルド限定のクロスプラットフォーム検証用 `auto_update_check_enabled` フラグ（本番ビルドでは無効化）を整備しました。

## [v0.3.3] - 2026-11-28

### 改善と新機能
- **Tailscale Serve（Tailnet 内のみ・非公開）接続方式と HTTPS ポート設定を追加**：Muse 側と接続先が同一 Tailnet にある場合、「接続」から Tailscale Serve を選ぶだけでインターネットへ公開せず HTTPS 転送を自動管理でき、ポート 443 が他サービスで使用中の場合も別ポートへ変更できます。
  - `tailscale_serve` プロバイダーと `https_port` 設定（`tunnel.rs`、`profile.rs`、`service.rs`）を追加：Tailscale Funnel は `443`（既定）・`8443`・`10000`、Tailscale Serve は `443`・`8443`・`10000` および任意のポート（`1–65535`）に対応します。選択したポート単位で `--https=<port>` の起動・停止と排他チェックを行い、別ポートのサービスに影響を与えず、手動構成済みの同一ポート `tailscale serve` もそのまま引き継ぎます（443 はポート省略、その他は `:port` を保持）。
  - **誤公開を防ぐ双方向ガード**：`Tailscale Funnel` モードで選択ポートに同一ローカルポート向けの `tailscale serve` 設定を検出した場合は、自動で公開 Funnel へ昇格させず切り替えを案内します。逆に `Tailscale Serve` モードでも、そのポートで公開 Funnel が有効な場合は非公開モードとしての引き継ぎを拒否します。
  - 「接続」画面に HTTPS ポート選択、常時表示のプロバイダー説明、Tailnet 内接続ガイドカード（MagicDNS / HTTPS Certificates のツールチップと Tailscale DNS 管理画面へのリンク付き）を追加し、概要・初回設定ガイド・接続テストの 3 言語文言を統一しました。

### セキュリティと安定性の強化
- **コマンド承認における難読化フラグとダウンロード＆即時実行（download cradle）の検知を強化**（[#15](https://github.com/Aenvo/MuseLink/pull/15) · [@KammyAugust11](https://github.com/KammyAugust11)）：PowerShell の `-EncodedCommand` 省略形およびダウンロード内容を直接実行するパターンを検知し、セッション許可ではなく常に単回承認を必須化しました。
  - `-ec` および長めの `-EncodedCommand` 前方一致（`-enc`・`-enco` など）を常時検知しつつ、短い省略形（`-e`・`-en`）は `powershell` / `pwsh` 呼び出しの引数に限定し、`pip install -e .`・`rg -e`・`node -e` などの通常の開発コマンドの誤検知を防ぎました。
  - 新規リスクタグ `download_cradle` と 3 言語の承認ダイアログ表示を追加し、`Net.WebClient`（`DownloadString` / `DownloadData` / `DownloadFile`）や `Invoke-WebRequest` / `iwr` / `Invoke-RestMethod` / `irm` / `curl` / `wget` と `iex`・インタプリターへのパイプ実行を検知します（ファイルへの単純なダウンロードは影響を受けません）。
- **コマンド実行子プロセスから認証情報系の環境変数を既定で除去**（[#14](https://github.com/Aenvo/MuseLink/pull/14) · [@KammyAugust11](https://github.com/KammyAugust11)）：サンドボックス子プロセスの環境構築時に、一般的なサードパーティ製トークンやシークレット系の環境変数を自動で除外し、ホスト環境の認証情報が実行コマンドから読み取られるのを防ぎます。
  - プラットフォーム非依存の `core/env_scrub.rs` を追加して `sandbox.rs` の `build_clean_env` に組み込み、変数名のみを大文字・小文字区別なしで判定します（`GITHUB_TOKEN`・`AWS_SECRET_ACCESS_KEY`・`OPENAI_API_KEY` などの一覧と `TOKEN` / `SECRET` / `PASSWORD` / `PASSWD` / `API_KEY` / `APIKEY` / `PRIVATE_KEY` / `ACCESS_KEY` / `CREDENTIAL` を含む変数名を除外し、値は一切読み取り・記録しません。`PATH`・`SystemRoot`・`TEMP` などの必須変数は維持されます）。
- **リリース成果物にビルド来歴証明（Build Provenance Attestation）を追加**（[#16](https://github.com/Aenvo/MuseLink/pull/16) · [@KammyAugust11](https://github.com/KammyAugust11)）：リリースワークフローで Windows / Linux / macOS の各配布アセットに対する GitHub ビルド来歴証明を生成するようになり、ダウンロードしたバイナリが本リポジトリのソースコードから GitHub Actions 上でビルドされ、差し替えや改ざんが行われていないことを `gh attestation verify` で検証できるようになりました。

## [v0.3.2] - 2026-11-28

### 改善と新機能
- **初回起動時のシステム言語自動検出（中国語・日本語以外は英語を既定）**：初回起動時に OS の表示言語を自動判定し（簡体字・繁体字中国語は簡体字中国語、日本語は日本語、それ以外の言語環境は English を既定表示）、「設定 → 一般」で手動変更した後は選択した言語を固定保持します。
  - **3 プラットフォームのネイティブ言語取得**：Windows は `GetUserPreferredUILanguages` / `GetUserDefaultLocaleName`、macOS は `NSLocale.preferredLanguages`（Finder 起動時に `LANG` が未設定でも動作）、Linux は `LC_ALL` → `LC_MESSAGES` → `LANG` および `LANGUAGE`（`C` / `POSIX` 時は無視して英語へフォールバック）で判定します。
  - **初回セットアップ整合性と英語フォールバック統一**：自動検出結果は設定保存前には `desktop-state.json` へ先行書き込みせず `fresh_setup` フローを維持し、ウィンドウ初期タイトル・HTML 既定言語・スナップショット読込前フォールバック・Windows NSIS インストーラー既定フォールバックも英語基準に統一しました。
- **アプリ内変更履歴の階層表示と日本語ログのネイティブ対応**：「このアプリについて → 変更履歴 / リリースノート」はデフォルトで要点を簡潔に表示し、バージョンごとにワンクリックで詳細を展開できるようになりました。日本語 UI でも日本語の変更履歴を直接表示します。
  - `CHANGELOG.ja.md` を追加し、日本語 UI のアプリ内ダイアログおよび GitHub リンクでの英語フォールバックを解消しました。
  - CI・リリースワークフローをすべての `CHANGELOG*.md`・`README*.md` を自動検出する構成へ拡張し、言語間のバージョン整合性チェックと全言語リリースノートの自動生成に対応しました。

### セキュリティと安定性の強化
- **旧 `/api` 互換インターフェースを既定オフ化し、「設定 → 互換性」から必要に応じて有効化可能に**（[#10](https://github.com/Aenvo/MuseLink/pull/10) · [@KammyAugust11](https://github.com/KammyAugust11)）：新規インストールおよび `/agent/v1` 利用中の環境では既定で旧 `/api/*` を 404 として公開トークンの露出面を縮小し、旧 File Bridge のみを利用中の環境はアップグレード後も自動で互換性を維持します。
  - **既存ユーザーの精密移行と初回状態の保護**：`desktop-state.json` に `legacy_api_enabled` が未記録の場合、既存の `config.json`・`token` があり `agent-token` 未発行の旧 File Bridge 環境のみ自動でオンを維持します（`desktop-state.json` は先行作成しません）。新規の指定フォルダー設定では旧 `config.json` / `token` を生成しなくなりました。
  - **「設定 → 互換性」カードとライフサイクル整備**：「設定 → 互換性」に互換スイッチ（既存の旧設定または指定フォルダー権限がある場合のみ有効化でき、ドライブ全体モードへは拡張しません）と旧トークンのローテーション／フォルダー表示を追加。オフ時もローカルの旧トークンファイルは保持し、有効な接続面がなくなった場合は自動で接続を一時停止します。
  - **ヘルスチェック整合とレート制限・統計の一貫性**：無効化された `/api/*` は認証前に 404 を返してファイルアクティビティ統計を汚染せず、未認証レート制限は維持します。内蔵ヘルスチェックは Agent トークンがある場合に `/agent/v1/roots` の JSON 応答を検証し、旧サービスの引き継ぎ時は旧 API を自動で有効化（ロールバック時に復元）します。
- **デフォルトの認証情報保護範囲を拡大**（[#9](https://github.com/Aenvo/MuseLink/pull/9) · [@KammyAugust11](https://github.com/KammyAugust11)）：主要なクラウド・コンテナ認証情報フォルダやブラウザのログイン情報ストアをデフォルトで読み取り・検索・書き込みから保護し、ドライブ全体やホームディレクトリの許可時にも漏えいを防ぎます。
  - `.npmrc`・`.pypirc` を書き込み保護のみから全面保護へ引き上げ、`.docker`・`.kube`・`.azure`・gcloud 認証情報ディレクトリを追加しました。
  - 各プラットフォームのブラウザ認証情報ストアを保護対象に追加しました：Windows（Chrome / Edge の `User Data`・Firefox の `Profiles`）、macOS（既存の `~/Library/Keychains` に加え Application Support 配下の Chrome / Edge / Firefox）、Linux（`google-chrome` / `chromium` / `microsoft-edge`・`.mozilla/firefox`）。

## [v0.3.1] - 2026-11-27

### 不具合修正とクロスプラットフォーム対応
- **全プラットフォームで「専用トークンをコピー」が失敗する問題を修正**（[#12](https://github.com/Aenvo/MuseLink/pull/12)）：macOS のネイティブ秘密クリップボード、Linux のシステムクリップボード連携、Windows の競合リトライおよび詳細な失敗表示を実装しました。
  - **macOS ネイティブ秘密クリップボード**：AppKit の `NSPasteboard` に `org.nspasteboard.ConcealedType`・`org.nspasteboard.TransientType` と `CurrentHostOnly` を付与し、履歴ツールへの記録やユニバーサルクリップボード同期を防止しました。
  - **Linux クリップボード対応**：`wl-copy` / `xclip` / `xsel` の標準入力経由で安全に書き込み、コマンドライン引数へのトークン露出とバックグラウンドデーモンによるハングを防止しました。
  - **Windows クリップボードの安定性とプライバシー強化**：`OpenClipboard` に短いバックオフリトライを追加して一時占有による競合を緩和し、`ExcludeClipboardContentFromMonitorProcessing`・`CanIncludeInClipboardHistory`・`CanUploadClipboardData` を 4 バイト `DWORD(0)` 形式で統一しました。
  - **詳細な失敗原因の表示**：初回ガイドと接続カードで失敗原因を 3 言語表示し、「トークンファイルの場所を開く」フォールバック導線を維持しました。
- **macOS / Linux の「ローカルドライブ管理（フルアクセス）」で HTTP 403 になりデスクトップを読み取れない問題を修正**（[#12](https://github.com/Aenvo/MuseLink/pull/12)）：システムルート `/` の検出、Unix 機密パス保護、macOS プライバシー設定導線を整備しました。
  - **Unix ローカルルート検出と検証**：macOS / Linux の `fixed_local_roots()` で `root`（`/`）を返すようにし、エイリアス検証とドライブ選択 UI（「システムルート（/）」）を更新、空ルートマップをフェイルクローズ拒否して `/agent/v1/roots` と `/agent/v1/locations/desktop` の 403 を解消しました。
  - **macOS / Linux の機密パス保護と安全な検索除外境界**：macOS の `~/Library/Keychains` を全面保護し、システムディレクトリ・自動起動（`~/Library/LaunchAgents`・`~/.config/autostart`）・ログインシェル設定（`.zprofile` / `.zshenv` / `.bash_profile`）・`.app` バンドルをデフォルト書き込み保護にしました。バックグラウンド検索からは macOS の `/System`・`/Library`・`/Applications`・`/usr`・`/bin`・`/sbin`・`/private`・`~/Library`（iCloud や他アプリコンテナの TCC 確認連鎖を回避）・写真ライブラリ・`/dev`・非ローカル `/Volumes/*`、Linux の `/proc`・`/sys`・`/dev`・`/run` を除外しました（直接パス読み取りは可能）。
  - **macOS プライバシー権限（TCC）体験とエラー応答の改善**：インデックス前にデスクトップ・書類・ダウンロードを事前プローブし、「フルディスクアクセス設定を開く」ワンクリックボタンを追加。3 言語の接続手順表記を中立化し、`PermissionDenied` を操作案内付きの HTTP 403 にマッピングしました。
- **README の Windows サンドボックス境界記述を修正し 3 言語メタデータを同期**（[#11](https://github.com/Aenvo/MuseLink/pull/11) · [@KammyAugust11](https://github.com/KammyAugust11)）：サンドボックスのタイムアウト制御と `Everyone` / `RESTRICTED` SID の書き込み範囲の記述を正確化し、日英中のヘッダー・プラットフォームバッジ・パッケージメタデータを同期しました。

## [v0.3.0] - 2026-11-27

### 新機能
- **アプリ内自動更新と同期変更履歴ダイアログ**：起動時のサイレント確認、署名検証付きのワンクリック更新（Windows インストーラー版 & Linux AppImage）、アプリ内での変更履歴閲覧に対応しました。
  - **ワンクリック確認と再起動更新**：静的 `latest.json` とコンパイル時固定の Minisign 公開鍵で検証（レート制限のある REST API は不使用）。右下の通知カードと緑色の進捗バーで既存接続を保ったままダウンロードし、サービス（Windows 同梱 `Everything.exe` を含む）を安全に停止して自動再起動します。Windows ポータブル版・Linux `.deb`・macOS / ソースビルドも同じ `latest.json` で通知・詳細表示・直行リンクを提供します。
  - **アプリ内変更履歴ダイアログ**：「このアプリについて」に「変更履歴」「GitHub」「更新を確認」を配置し、同梱コピーで即時表示しつつバックグラウンドで GitHub 最新ログへ同期します。
  - **3 プラットフォーム自動リリースパイプライン**：`.github/workflows/release.yml` と `generate-updater-manifest.ps1` を追加し、`v*` タグで 3 プラットフォームのビルド・テストと `latest.json`・`SHA256SUMS.txt` 生成を自動化しました。
- **Linux および macOS（Apple Silicon & Intel）に対応**（[#3](https://github.com/Aenvo/MuseLink/pull/3)・[#7](https://github.com/Aenvo/MuseLink/pull/7) · [@eightman999](https://github.com/eightman999)、[#6](https://github.com/Aenvo/MuseLink/issues/6) を解決）：3 プラットフォームすべてでファイル API と Tailscale / Cloudflare トンネル設定が動作し、ビルド済みパッケージを提供します。
  - **Linux 対応**：ファイル API（検索 / 一覧 / 読み取り / 書き込み / フォルダ作成）とトンネル設定をカバーし、署名済み AppImage と `.deb` を提供します（ローカルコマンド実行は Windows 専用）。
  - **macOS 対応**：Universal `.dmg` / `.app.zip` を提供し、`Tailscale.app` と Homebrew を自動検出します。Apple 公証未実施のため初回起動時は「システム設定 → プライバシーとセキュリティ」で許可が必要です（ローカルコマンド実行は Windows 専用）。
- **日本語ローカライズ（`ja`）を追加**（[#8](https://github.com/Aenvo/MuseLink/pull/8) · [@eightman999](https://github.com/eightman999)）：UI 全体、システムトレイ、Muse 接続手順、日本語 README に対応し、「設定 → 一般」から切り替えられます。

## [v0.2.2] - 2026-11-27

### 改善と新機能
- **ファイル読み取り上限の引き上げ、チャンク読み取り・生バイナリ直接転送に対応**（[#4](https://github.com/Aenvo/MuseLink/issues/4) を解決）：単一ファイル上限を 10 MiB に拡大し、大容量ファイルや高解像度画像の高速・分割転送に対応しました。
  - **単一ファイル JSON 読み取り上限を 10 MiB に拡大**：書き込み上限と揃え、既存設定のまま高解像度画像を読み取れるようにしました。超過時はチャンク・`raw` 推奨値を含む HTTP `413` を返し、旧 `/api/read` は 2 MiB 互換を維持します。
  - **`offset` / `limit` チャンク読み取り（総サイズ上限なし）**：検証済み OS ハンドルから分割読み取り（JSON 標準 2 MiB・最大 4 MiB）し、`etag` と `if_match` / `If-Match` をサポート（途中変更時は HTTP `409`）。
  - **`raw=1` 生バイナリ直接転送**：Base64 の 33% 膨張を排除し（トンネル経由で実測約 40% 高速化、1 リクエスト最大 32 MiB、チャンク併用可）、`ETag` と `X-File-*` ヘッダーを返却します。
  - **Muse 接続手順を更新**：画像・バイナリは `raw=1` を優先し、大容量ファイルは分割リトライするよう案内を更新しました。

## [v0.2.1] - 2026-11-25

### 改善と新機能
- **デスクトップ UI の全面刷新と情報設計の再構築**：ビジュアルを統一し、接続・ファイル権限・ログ・設定の 4 画面構成を整理しました。
  - **コンポーネント刷新**：有効時の `Switch` を `#48C52C` グリーン＋白ノブに変更し、`Input`・`Select`・公開アドレス欄を透明アウトラインに統一、ログ検索欄に虫眼鏡アイコンを追加し危険操作を赤色で強調しました。
  - **接続ページと初回ガイド刷新**：上部に独立した「ガイド」カードを配置し、デバイス名を行内化、「Muse 接続手順」を接続ページ下部と初回ガイド Step 3 に整理しました。
  - **ファイル権限とログページの整理**：「権限を保存」をヘッダー右側へ移し、ログページは期間フィルターを含む 1 行バーと「同期を一時停止 / 再開」単一ボタンに集約しました。
  - **設定を「一般」と「データ」に再編成**：ローカルデータ、ファイルインデックス、ログ、トークン管理を「データ」に統合しました。
  - **起動とトレイ挙動の改善**：自動起動はデフォルトオフで `--autostart` によりトレイへサイレント起動、手動起動時は常にメインウィンドウを表示し、「閉じる時にトレイへ最小化」（デフォルトオン）を追加しました。
- **コマンド承認と実行状態のリアルタイムロングポーリング**：承認・拒否・完了時に即座に応答が返り、チャットで手動通知する必要がなくなりました。
  - `GET /agent/v1/exec/{job_id}` に `?wait_ms=`（デフォルト `0`、上限 `25000` ms、推奨 `20000` ms）を追加し、スロットやロックを占有せず非同期待機します。
  - 接続手順を継続ポーリング対応へ更新し、承認カウントダウン 0 秒時のボタン無効化による誤クリック防止を追加しました。
- **絞り込みログエクスポートとフォルダ直行**：範囲・結果・期間・キーワードで絞り込んだマスク済みログを `JSONL` / `CSV` で書き出せます。
  - 「設定 → データ → ログデータ」に「ログを消去」「ログをエクスポート」（UTF-8 BOM＋二重トークンマスキング）「フォルダを開く」（10 MiB ローテーションログ）を追加しました。
- **Cloudflare 一時トンネルを削除し MIT ライセンスを整備**：接続方式を Tailscale Funnel と Cloudflare 名前付きトンネルに集約しました。
  - 旧設定の `cloudflare_quick` は Tailscale Funnel へ自動フォールバックします。ルートに [LICENSE](LICENSE)（MIT）を追加し各メタデータと配布物へ同期しました。

### 不具合修正
- **孫プロセスがパイプを継承してジョブが `running` のまま停止する問題を修正**：メインコマンド終了直後に Windows `JobObject` で残留子プロセスツリーを回収し、EOF 待ちの永久ブロックを解消しました。

## [v0.2.0] - 2026-11-25

### 新機能
- **制御付きローカルコマンド実行（Windows・デフォルトオフ）**：許可済みプロジェクトサブディレクトリ内で、書き込み制限付きサンドボックスと承認ダイアログのもと開発ツールやシェルを実行できます。
  - 「ファイル権限 → Muse ファイルツール」に総スイッチとリスク確認を追加し、`/agent/v1/exec`・状態確認・キャンセル API を提供（ドライブルート直下は拒否）。
  - **ワークスペースごとの書き込み制限付きサンドボックス**：`CreateRestrictedToken(WRITE_RESTRICTED)` と正規パス SHA-256 由来の固有 SID により対象サブディレクトリと専用一時領域以外への書き込みを遮断し、`JobObject` のメモリ・プロセス数・タイムアウト制限とフェイルクローズを強制します。
  - **3 段階リスク分類と最前面承認ダイアログ**：読み取りや標準ビルド・テストは自動許可、初回シェル利用・依存変更・`git push` は 120 秒の承認ダイアログ（単発または 4 時間セッション許可）を表示し、機密パス・`..`・強制プッシュ・再帰削除・管理ツール・難読化スクリプト・`unsandboxed` は毎回単発承認を必須とします。
- **デバイス命名と名称変更レシート**（[#1](https://github.com/Aenvo/MuseLink/pull/1)）：複数 PC に固有のデバイス名を設定でき、名称変更手順と Muse 確認レシートを生成できます。
- **コマンド実行ログフィルターと変更履歴ボタン**：ログに「コマンド実行」フィルターを、「このアプリについて」に「変更履歴」ボタンを追加しました。

### セキュリティと安定性の強化
- **ハンドルベースのファイル検証と機密パス保護**（[#2](https://github.com/Aenvo/MuseLink/pull/2)）：OS ハンドルで実パスと属性を再検証してシンボリックリンク脱出・TOCTOU 差し替えを防御し、認証情報とシステムパスをデフォルト保護します。
  - `~/.muse-bridge`・`~/.ssh`・`~/.aws`・`~/.gnupg` をデフォルト遮断し、システムディレクトリ・スタートアップ・シェルプロファイル・`.gitconfig`・アプリディレクトリを書き込み保護にしました。
  - `Everything.exe`・`cloudflared`・コマンド子プロセスを Windows `JobObject` で管理し終了時に一括回収します。
  - `mfa_` トークンのログマスキング、モード切替時の権限保持、Base64 書き込み 10 MiB 検証修正、CSP 厳格化、`RUSTSEC-2026-0592` 対応、機能オフ時の HTTP 403 `exec_disabled` を整備しました。

### エンジニアリングとテスト
- **Windows GitHub Actions CI を追加**：フォーマット、フロントエンドビルド、Rust テスト、シンボリックリンク脱出テスト、Windows カーネルサンドボックステストを自動化しました。

## [v0.1.0] - 2026-11-24

### 初回リリース
- **初回ガイドとファイル権限設定**：特定フォルダまたは固定ドライブの許可とバックグラウンドの Everything ファイル名インデックス構築に対応しました。
- **Muse 専用トークンとファイル API**：独立した `/agent/v1` と `agent-token`（発行・コピー・場所を開く・ローテーション・失効）で検索・一覧・読み取り・書き込み・フォルダ作成をサポートしました。
- **公開トンネル・トレイ管理・Windows 配布**：Tailscale Funnel 標準対応（Cloudflare トンネル対応）、システムトレイ操作、アクセスログ、中英 UI、ポータブル ZIP / NSIS インストーラーを提供しました。
