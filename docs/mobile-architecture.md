# AI Quota Monitor モバイル移植 アーキテクチャ設計

## 1. 現状の整理

### デスクトップ版の構成

```
Frontend (Vanilla JS / HTML / CSS)
  ↕  Tauri IPC (window.quotaApi)
Backend (Rust / Tauri 2.x)
  ├── api_client      … Anthropic / OpenAI Usage API 呼び出し
  ├── usage_parser     … レスポンス正規化 (UsageWindow 抽出)
  ├── oauth/           … PKCE フロー (Claude: 手動コード貼付, Codex: localhost callback)
  ├── token_store      … OS Keyring によるトークン保管 (チャンク対応)
  ├── token_refresh    … 期限前自動リフレッシュ
  ├── external_notify  … Discord / Pushover Webhook
  ├── store_repo       … accounts.json 永続化
  ├── validation       … 入力バリデーション / レートリミット
  └── window_ops       … ウィンドウモード切替 (normal / minimal)

データ永続化:
  - accounts.json (appData) … アカウント一覧・設定
  - OS Keyring              … トークン・リフレッシュトークン・有効期限
  - sessionStorage          … ランタイムキャッシュ (取得結果・履歴)
```

### 主要機能一覧

| 機能 | 概要 |
|------|------|
| クォータダッシュボード | Claude/Codex の利用率・リセット時刻をカード表示 |
| ポーリング | 30〜600 秒間隔で自動取得、手動取得ボタン |
| OAuth ログイン | Claude: ブラウザ → コード貼付、Codex: localhost:1455 callback |
| CLI 取込 | ~/.claude/.credentials.json からトークン読み込み |
| トークン自動リフレッシュ | 期限 5 分前に自動更新 |
| デスクトップ通知 | critical / exhausted / recovery 時にOS通知 |
| 外部通知 | Discord Webhook、Pushover |
| Usage JSON エクスポート | クォータスナップショットをファイル出力 |
| ウィンドウモード | normal / minimal 切替、ドラッグ移動 |
| デバッグパネル | 生 JSON レスポンス表示 |
| ログ | アプリ内イベントログ (最大 200 件) |

---

## 2. 移植方式の比較

### 候補

| 方式 | Rust 再利用率 | UI | 内輪配布 |
|------|-------------|-----|---------|
| **A. Tauri Mobile** | ◎ 90% | WebView | TestFlight / APK 直配布 |
| B. React Native + Rust FFI | ○ 70% | Native 部品 | TestFlight / APK |
| C. Flutter + flutter_rust_bridge | ○ 70% | Flutter Widget | 同上 |
| D. PWA | × 0% | ブラウザ | URL 共有のみ |
| E. ネイティブ (Swift/Kotlin) | × 0% | 完全ネイティブ | 同上 |

### 推奨: A. Tauri Mobile

選定理由:

1. **Rust バックエンドの直接再利用** — `api_client`, `usage_parser`, `oauth`, `token_store`, `token_refresh`, `validation`, `external_notify` はほぼそのまま使える。現コードベースの中核 (~2,500 行) を書き直す必要がない
2. **Tauri 2.x は iOS / Android を公式サポート** — 同一の `src-tauri/` プロジェクトからデスクトップとモバイルを両方ビルドできる
3. **開発者が Tauri に習熟済み** — 学習コスト最小
4. **内輪利用** — WebView の UI 品質で十分。App Store 審査不要 (TestFlight / APK サイドロード)

---

## 3. モバイル版で残す機能 / 落とす機能

### 残す (コア価値)

| 機能 | 変更点 |
|------|--------|
| クォータダッシュボード | モバイル向けレイアウトに変更 (1 列カード) |
| フォアグラウンドポーリング | そのまま (アプリ表示中のみ) |
| OAuth ログイン | ASWebAuthenticationSession (iOS) / Custom Tabs (Android) に変更 |
| 複数アカウント管理 | そのまま |
| トークン自動リフレッシュ | そのまま |
| ローカル通知 | tauri-plugin-notification はモバイル対応済み |
| 外部通知 (Discord / Pushover) | 設定 UI を簡略化して残す |

### 変更が必要

| 機能 | 対応方針 |
|------|---------|
| バックグラウンドポーリング | 後述 (§5) |
| Codex OAuth | localhost callback → カスタム URL スキーム (§4) |
| トークン保管 | keyring crate → iOS Keychain / Android KeyStore (§4) |

### 落とす

| 機能 | 理由 |
|------|------|
| ウィンドウモード (normal/minimal) | モバイルでは不要。単一ビュー |
| ウィンドウドラッグ・リサイズ | OS が管理 |
| ズーム制御 | OS のピンチズームで代替 |
| CLI 取込 | モバイルに CLI 環境がない |
| Usage JSON エクスポート | モバイルでの利用シーンが薄い |
| デバッグパネル | 開発中のみ必要。隠しジェスチャーで表示可能にしておけば十分 |

---

## 4. アーキテクチャ詳細

### 全体構成

```
┌────────────────────────────────────────────────────────┐
│             Mobile Frontend (WebView)                  │
│                                                        │
│  HTML/CSS/JS (モバイル専用レイアウト)                      │
│  ├── dashboard.html   … カード一覧 + ステータス          │
│  ├── accounts.html    … アカウント管理                   │
│  ├── settings.html    … 通知・ポーリング設定              │
│  └── tauri-bridge.js  … IPC (既存を再利用)               │
│                                                        │
│  ナビゲーション: 下部タブバー (3タブ)                      │
│    [ダッシュボード] [アカウント] [設定]                    │
└───────────────┬────────────────────────────────────────┘
                │ Tauri Commands (IPC)
                ↓
┌────────────────────────────────────────────────────────┐
│           Rust Backend (src-tauri/)                     │
│                                                        │
│  ★ 再利用モジュール (変更なし〜軽微)                      │
│  ├── api_client.rs         … そのまま                   │
│  ├── usage_parser.rs       … そのまま                   │
│  ├── token_refresh.rs      … そのまま                   │
│  ├── validation.rs         … そのまま                   │
│  ├── external_notify.rs    … そのまま                   │
│  ├── store_repo.rs         … そのまま (パスは Tauri が解決)│
│  ├── error.rs              … そのまま                   │
│  ├── commands.rs           … そのまま                   │
│  ├── usage_commands.rs     … そのまま                   │
│  ├── settings_commands.rs  … そのまま                   │
│  ├── notification_commands.rs … そのまま                │
│  └── export_commands.rs    … そのまま (モバイルでは無効化) │
│                                                        │
│  ★ 変更が必要なモジュール                                │
│  ├── oauth/                                            │
│  │   ├── claude.rs   … redirect_uri をカスタムスキームに │
│  │   └── codex.rs    … localhost callback を廃止、       │
│  │                     カスタムスキーム + Deep Link に    │
│  ├── token_store.rs  … keyring → モバイルセキュアストレージ│
│  ├── oauth_commands.rs  … モバイル OAuth フロー対応      │
│  └── main.rs         … window_ops 削除、モバイル初期化追加│
│                                                        │
│  ★ 削除するモジュール                                    │
│  ├── window_ops.rs        … 不要                        │
│  └── window_commands.rs   … 不要                        │
└────────────────────────────────────────────────────────┘
```

### 4.1 OAuth フローの変更

デスクトップ版との最大の差異。モバイルでは localhost サーバーを立てられない。

**Claude OAuth:**

```
現在: ブラウザ → redirect_uri (platform.claude.com) → ユーザーがコードをコピペ
モバイル: 同じフロー。コード貼付は維持可能
         (ペーストボード連携で UX 改善の余地あり)
```

Claude の OAuth は元々「ブラウザでログイン → コード文字列をアプリに貼る」という手動フローなので、モバイルでもそのまま動作する。むしろクリップボード連携でペースト検知すればデスクトップより楽になる可能性がある。

**Codex OAuth:**

```
現在:  ブラウザ → localhost:1455/auth/callback → アプリが受信
モバイル: ブラウザ → カスタム URL スキーム → アプリが受信
```

```
変更箇所:
1. redirect_uri を "ai-quota-monitor://auth/callback" に変更
2. callback_server.rs (TCP リスナー) を廃止
3. Tauri の deep-link プラグイン (tauri-plugin-deep-link) でコールバック受信
4. Info.plist / AndroidManifest.xml に URL スキーム登録
```

ただし、OpenAI 側の OAuth クライアント設定で新しい redirect_uri を許可リストに
追加する必要がある。既存の client_id (`app_EMoamEEZ73f0CkXaXp7hrann`) は
Codex CLI のものを借用しているため、カスタムスキームの redirect_uri が
受理されない可能性がある。その場合は Codex OAuth 自体をモバイル版では
見送り、Claude のみのサポートとする判断もありうる。

### 4.2 トークン保管

```
デスクトップ: keyring crate → Windows Credential Manager / macOS Keychain / Linux Secret Service
モバイル:     tauri-plugin-store (暗号化) または プラットフォーム API 直接呼び出し
```

選択肢:

| 方式 | iOS | Android | 備考 |
|------|-----|---------|------|
| **keyring crate** | Keychain (対応済み) | KeyStore (要確認) | 既存コードをそのまま使えれば最良 |
| tauri-plugin-store | ファイル暗号化 | ファイル暗号化 | Tauri 公式、セキュリティレベルは OS ネイティブより低い |
| プラットフォーム固有 FFI | Security.framework | AndroidKeyStore API | 最もセキュア、実装コスト高 |

推奨: まず keyring crate がモバイルで動作するか検証する。
動作しない場合は tauri-plugin-store で代替する (内輪利用なので許容範囲)。

### 4.3 フロントエンド (モバイル UI)

既存の Vanilla JS を活かしつつ、モバイル向けの HTML/CSS を別途用意する。

```
public/
  ├── index.html          … デスクトップ用 (既存)
  ├── mobile.html         … モバイル用エントリポイント (新規)
  ├── mobile-styles.css   … モバイル用スタイル (新規)
  ├── app.js              … 既存 (デスクトップ用)
  ├── mobile-app.js       … モバイル用 (新規、app.js を簡略化)
  ├── tauri-bridge.js     … 共用 (変更なし)
  ├── ui-logic.js         … 共用 (変更なし)
  └── account-ui.js       … 共用 (変更なし、呼び出し側で不要部分をスキップ)
```

**画面構成 (3 タブ):**

```
┌─────────────────────────────────────────┐
│  AI Quota Monitor            [取得] [●] │  ← ヘッダー (取得ボタン + ポーリング状態)
├─────────────────────────────────────────┤
│                                         │
│  ┌─────────────────────────────────┐    │
│  │ Claude Code — Account 1        │    │
│  │ ████████████░░░  78%  (5h)     │    │
│  │ ██████░░░░░░░░░  42%  (7d)     │    │
│  │ リセット: 2h 15m               │    │
│  │ 状態: ⚠ warning                │    │
│  └─────────────────────────────────┘    │
│                                         │
│  ┌─────────────────────────────────┐    │
│  │ Codex — Account 1              │    │
│  │ ██████████████░  92%           │    │
│  │ リセット: 45m                   │    │
│  │ 状態: 🔴 critical              │    │
│  └─────────────────────────────────┘    │
│                                         │
├─────────────────────────────────────────┤
│  [ダッシュボード]  [アカウント]  [設定]    │  ← 下部タブバー
└─────────────────────────────────────────┘
```

デスクトップ版の minimal モードに近い「カード一覧」をデフォルトとし、
タップで詳細 (全ウィンドウ表示・リセットカウントダウン) を展開する。

---

## 5. バックグラウンドポーリングと通知

モバイル最大の技術課題。iOS / Android ともにバックグラウンド実行を厳しく制限している。

### 5.1 制約

| プラットフォーム | API | 最小間隔 | 信頼性 |
|----------------|-----|---------|--------|
| iOS | BGAppRefreshTaskRequest | ~15 分 (OS が決定) | 低 (OS が最適化) |
| Android | WorkManager (periodic) | 15 分 | 中 (Doze モード影響) |

現行のデスクトップ版は 30 秒〜10 分間隔でポーリングしているが、
モバイルのバックグラウンドではこの頻度は実現できない。

### 5.2 戦略

```
フォアグラウンド: 既存ロジックそのまま (30秒〜10分間隔)
バックグラウンド: OS のバックグラウンドタスク API で 15〜30 分間隔
                 → 状態変化時にローカル通知を発行
```

実装方針:

1. **フォアグラウンド** — `app.js` のポーリングループをそのまま使う。
   タイマー (`setInterval`) はアプリがフォアグラウンドにいる限り動作する。

2. **バックグラウンド** — Tauri のモバイルプラグインまたは
   ネイティブコード (Swift/Kotlin) で BGTaskScheduler / WorkManager を登録。
   Rust のコアロジック (`fetch_normalized_usage` → 状態判定 → 通知) を呼び出す。

3. **通知** — `tauri-plugin-notification` がモバイルでローカル通知に対応しているので、
   バックグラウンドタスク内でそのまま使える。

### 5.3 代替案: 外部通知で代替

バックグラウンドポーリングの実装が困難な場合、デスクトップ版を常時稼働させ、
Discord / Pushover で通知を受ける運用でも内輪利用なら十分成り立つ。
この場合モバイル版は「フォアグラウンドで見るダッシュボード」に徹し、
バックグラウンド機能は一切実装しない。これが最も開発コストが低い。

---

## 6. ビルド・配布

### 6.1 ビルド

```bash
# iOS (macOS 必要)
npm run tauri ios build

# Android
npm run tauri android build
```

Tauri 2.x の `tauri ios init` / `tauri android init` で
Xcode プロジェクト / Android Gradle プロジェクトが生成される。

### 6.2 配布 (内輪向け)

| プラットフォーム | 方法 | 制限 |
|----------------|------|------|
| iOS | TestFlight (Apple Developer Program $99/年) | テスター 10,000 人まで |
| iOS | Ad Hoc (デバイス UDID 登録) | 100 台まで |
| Android | APK 直接配布 (GitHub Releases 等) | 制限なし |

App Store / Google Play への公開は不要。

---

## 7. プロジェクト構成の変更

```
ai-quota-monitor/
  ├── public/
  │   ├── index.html           (既存: デスクトップ)
  │   ├── mobile.html          (新規: モバイルエントリ)
  │   ├── mobile-app.js        (新規: モバイル用メインロジック)
  │   ├── mobile-styles.css    (新規: モバイル用スタイル)
  │   ├── app.js               (既存: 変更なし)
  │   ├── tauri-bridge.js      (既存: 変更なし)
  │   ├── ui-logic.js          (既存: 変更なし)
  │   ├── account-ui.js        (既存: 変更なし)
  │   └── styles.css           (既存: 変更なし)
  ├── src-tauri/
  │   ├── src/
  │   │   ├── main.rs          (変更: #[cfg] でモバイル/デスクトップ分岐)
  │   │   ├── lib.rs           (新規: Tauri Mobile は lib.rs がエントリ)
  │   │   ├── api_client.rs    (変更なし)
  │   │   ├── usage_parser.rs  (変更なし)
  │   │   ├── token_store.rs   (変更: モバイル向け条件分岐を追加)
  │   │   ├── oauth/
  │   │   │   ├── claude.rs    (軽微な変更)
  │   │   │   └── codex.rs     (変更: カスタムスキーム対応)
  │   │   ├── commands.rs      (変更: window 系コマンドを除外)
  │   │   └── ...              (他は変更なし)
  │   ├── Cargo.toml           (変更: mobile feature flag 追加)
  │   ├── tauri.conf.json      (変更: モバイル設定追加)
  │   ├── gen/
  │   │   ├── apple/           (自動生成: Xcode プロジェクト)
  │   │   └── android/         (自動生成: Gradle プロジェクト)
  │   └── capabilities/
  │       └── mobile.json      (新規: モバイル権限定義)
  └── docs/
      └── mobile-architecture.md  (本ドキュメント)
```

### Cargo.toml の変更例

```toml
[features]
default = []
mobile = []  # モバイルビルド時に有効化

[dependencies]
# window_ops は desktop のみ
# keyring はモバイルでの動作を要検証
keyring = { version = "2", optional = true }

[target.'cfg(not(target_os = "ios"))'.dependencies]
[target.'cfg(not(target_os = "android"))'.dependencies]
```

---

## 8. 開発フェーズ

### Phase 1: 最小動作版 (ダッシュボード閲覧のみ)

- Tauri Mobile プロジェクト初期化 (`tauri ios init`, `tauri android init`)
- モバイル用 HTML/CSS/JS 作成 (ダッシュボードカード)
- 既存 Rust バックエンドで `fetch_usage` が動作することを確認
- トークンは手動ペースト (セキュアストレージ検証前は平文ファイル保存で仮実装)
- フォアグラウンドポーリングのみ

### Phase 2: 認証・セキュアストレージ

- モバイルでのセキュアストレージ確立 (keyring or 代替)
- Claude OAuth フロー (コード貼付) をモバイル UI で実装
- Codex OAuth (カスタム URL スキーム or 見送り判断)
- トークンリフレッシュの動作確認

### Phase 3: 通知・仕上げ

- ローカル通知 (フォアグラウンドでの状態遷移通知)
- バックグラウンドポーリング (実装可否を検証、困難なら見送り)
- 外部通知設定 UI
- アカウント管理画面
- 設定画面

---

## 9. リスクと対策

| リスク | 影響 | 対策 |
|--------|------|------|
| keyring crate がモバイルで動作しない | トークン保管方式の変更が必要 | Phase 1 初期に検証。tauri-plugin-store にフォールバック |
| Codex OAuth の redirect_uri 変更が不可 | Codex 対応不可 | Claude のみサポートとする。内輪利用なら許容範囲 |
| Tauri Mobile の安定性 | ビルドエラー・ランタイムクラッシュ | Tauri のバージョンを固定。デスクトップ版と同一バージョンを使用 |
| iOS バックグラウンド制約 | 通知が遅延・欠落 | フォアグラウンド利用を主とし、重要通知は Discord/Pushover で補完 |
| iOS 配布の手間 | TestFlight は Apple Developer Program が必要 | 内輪メンバーが Android を使えるなら APK 配布のみでも可 |

---

## 10. まとめ

- **方式**: Tauri Mobile (Tauri 2.x) を採用し、Rust バックエンドを最大限再利用
- **落とす機能**: ウィンドウモード、CLI 取込、JSON エクスポート、デバッグパネル
- **最大の技術課題**: OAuth コールバック方式の変更、バックグラウンドポーリング
- **現実的な割り切り**: バックグラウンド通知は外部通知 (Discord/Pushover) で代替し、モバイル版は「手元でサッと確認するダッシュボード」に徹するのが最もコスパが良い
- **Codex 対応**: OAuth redirect_uri の制約次第では Claude 専用とする判断もあり
