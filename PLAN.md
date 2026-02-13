# トークンリニューアル対応 実装計画

## 現状の問題

現在のアプリは CLI (Claude Code / Codex) が管理するトークンを**ユーザーが手動コピー&ペースト**して利用している。

- `~/.claude/.credentials.json` や `~/.codex/auth.json` からユーザーが手動でトークンを取得
- アプリ内でのトークン更新(refresh)機能が一切ない
- トークン有効期限の追跡もない
- HTTP 401 が返ってもユーザーに再入力を促すだけ

## 調査結果サマリ

### Claude Code の OAuth

| 項目 | 値 |
|------|-----|
| Client ID | `9d1c250a-e61b-44d9-88ed-5944d1962f5e` |
| Authorization | `https://claude.ai/oauth/authorize` |
| Token | `https://console.anthropic.com/api/oauth/token` |
| PKCE | S256 |
| Redirect | `http://localhost:54545/callback` |
| Access Token 寿命 | 8時間 (28800秒) |
| Access Token prefix | `sk-ant-oat01-` |
| Refresh Token prefix | `sk-ant-ort01-` |
| Scope | `user:inference user:profile` |

**重要制約**: 2026年1月以降、Anthropic はサードパーティアプリの OAuth 利用をホワイトリスト制で制限している。
Claude Code の client_id を使った authorize リクエストは、ホワイトリスト外のアプリからは
"Unauthorized - This service has not been whitelisted" で拒否される可能性が高い。

### Codex の OAuth

| 項目 | 値 |
|------|-----|
| Client ID | `app_EMoamEEZ73f0CkXaXp7hrann` |
| Authorization | `https://auth.openai.com/oauth/authorize` |
| Token | `https://auth.openai.com/oauth/token` |
| PKCE | S256 |
| Redirect | `http://localhost:1455/auth/callback` (ポートは可変) |
| Refresh Token | Single-use rotation 方式 |
| Device Code | `https://auth.openai.com/codex/device` (RFC 8628) |

**重要制約**: Refresh Token Rotation が適用されるため、
リフレッシュトークンを使うと即座に無効化され新しいペアが発行される。
CLI と同じリフレッシュトークンを使うと、CLI 側のトークンが無効になる。

### 認証情報ファイル

**Claude** (`~/.claude/.credentials.json`):
```json
{
  "claudeAiOauth": {
    "accessToken": "sk-ant-oat01-...",
    "refreshToken": "sk-ant-ort01-...",
    "expiresAt": 1748658860401
  }
}
```

**Codex** (`~/.codex/auth.json`):
```json
{
  "auth_mode": "chatgpt",
  "tokens": {
    "access_token": "<JWT>",
    "refresh_token": "<opaque>",
    "expires_at": "2026-02-13T12:00:00Z"
  }
}
```

## 実装方針

3つの認証方式を段階的に実装する:

1. **CLI連携モード** (Phase 1) — CLI の認証情報ファイルを自動読み取り・監視
2. **独立OAuth ログイン** (Phase 2) — アプリ内でブラウザ認証フローを実行
3. **自動トークンリフレッシュ** (Phase 3) — refresh_token による自動更新

---

## Phase 1: CLI 連携モード (credential file auto-import)

### 概要
CLI が管理する認証情報ファイルを自動的に読み取り、ファイル変更を監視して
トークンが更新されたら自動で反映する。手動コピペを完全に不要にする。

### 実装内容

#### 1.1 認証情報ファイルリーダー (`src-tauri/src/credential_reader.rs`)

- `~/.claude/.credentials.json` のパース (Linux/macOS)
- macOS の場合は `security find-generic-password` 経由の Keychain 読み取りも対応
- `~/.codex/auth.json` のパース
- macOS の場合は Keychain ("Codex Auth") からの読み取りも対応
- `CLAUDE_CONFIG_DIR` 環境変数によるカスタムパス対応
- パース結果を `CredentialSet` 構造体で返す:
  ```rust
  struct CredentialSet {
      access_token: String,
      refresh_token: Option<String>,
      expires_at: Option<i64>,  // epoch millis
  }
  ```

#### 1.2 ファイルウォッチャー (`src-tauri/src/credential_watcher.rs`)

- `notify` クレート (v6) でファイル変更を監視
- 変更検知時に認証情報を再読み取り
- 読み取ったトークンを keyring に自動保存
- フロントエンドにイベント通知 (`credential-updated`)
- デバウンス処理 (500ms) で連続変更に対応

#### 1.3 Tauri コマンド追加

- `import_cli_credentials(service)` — CLI 認証情報を手動インポート
- `start_credential_watch(service)` — ファイル監視開始
- `stop_credential_watch(service)` — ファイル監視停止
- `get_credential_status(service)` — CLI認証情報の状態取得 (有無, 有効期限)

#### 1.4 UI 変更

- アカウント行に「CLIからインポート」ボタン追加
- CLI 認証情報の状態表示 (検出済み / 未検出 / 期限切れ)
- 「自動同期」トグル (ファイルウォッチャーの ON/OFF)
- トークン有効期限の表示

#### 1.5 バックエンド変更

- `validation.rs`: `auth.openai.com` と `console.anthropic.com` を allowlist に追加
- `token_store.rs`: refresh_token の保存対応 (別キー `{service}:{id}:refresh`)
- `token_store.rs`: expires_at の保存対応 (別キー `{service}:{id}:expires`)
- `Cargo.toml`: `notify = "6"`, `dirs = "5"` 追加

---

## Phase 2: 独立 OAuth ログイン

### 概要
アプリ内でブラウザベースの OAuth PKCE フローを実行し、CLI に依存しない
独自のトークンペアを取得する。

### 2.1 Codex OAuth (実現可能性: 高)

- PKCE (S256) で `code_verifier` / `code_challenge` を生成
- ローカル HTTP サーバー (ポート動的割当) を起動してコールバック受信
- ブラウザを開いて `https://auth.openai.com/oauth/authorize` へリダイレクト
- コールバックで authorization code を受取り、token endpoint で交換
- access_token + refresh_token を keyring に保存
- **Device Code Flow** もサポート (headless 環境向け)

実装:
- `src-tauri/src/oauth/mod.rs` — OAuth モジュール
- `src-tauri/src/oauth/pkce.rs` — PKCE 生成
- `src-tauri/src/oauth/callback_server.rs` — ローカルHTTPサーバー
- `src-tauri/src/oauth/codex.rs` — Codex 固有のフロー
- `src-tauri/src/oauth/device_code.rs` — Device Code Flow

追加依存:
- `sha2` — PKCE のSHA-256計算
- `base64` — base64url エンコード
- `rand` — ランダム生成
- `tokio` — 非同期ランタイム (既にreqwest経由で入っている可能性)
- `axum` or `tiny_http` — ローカルコールバックサーバー

#### 2.2 Claude OAuth (実現可能性: 中〜低)

同様のPKCEフローだが、Anthropic のサードパーティ制限により
ブロックされる可能性がある。

対応方針:
- まず実装して実際に試す
- ブロックされた場合は Phase 1 の CLI 連携モードをデフォルトに
- `claude.ai/oauth/authorize` のコールバックフォーマットが特殊 (`code#state`) なので注意
- 将来 Anthropic が OAuth App 登録を公開した場合に独自 client_id に切替可能な設計にする

#### 2.3 UI: ログインフロー

- アカウント設定に「ブラウザでログイン」ボタン追加
- ログイン中のステータス表示 (ブラウザで認証待ち...)
- ログイン成功/失敗のフィードバック
- 認証方式の切替 (CLI連携 / OAuth ログイン / 手動トークン入力)

---

## Phase 3: 自動トークンリフレッシュ

### 概要
保存済みの refresh_token を使って、access_token の有効期限前に
自動的にトークンを更新する。

### 3.1 リフレッシュエンジン (`src-tauri/src/token_refresh.rs`)

- アクセストークンの有効期限を監視
- 期限の5分前にリフレッシュを実行
- Claude: `POST https://console.anthropic.com/api/oauth/token`
  ```
  grant_type=refresh_token
  refresh_token=sk-ant-ort01-...
  client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e
  ```
- Codex: `POST https://auth.openai.com/oauth/token`
  ```
  grant_type=refresh_token
  refresh_token=<stored_refresh_token>
  client_id=app_EMoamEEZ73f0CkXaXp7hrann
  ```
- 新しいトークンペアを keyring に保存
- フロントエンドに通知

### 3.2 リフレッシュ失敗時のフォールバック

- 401 応答 → ユーザーに再ログインを促す
- ネットワークエラー → exponential backoff でリトライ (最大3回)
- CLI 連携モード有効時 → CLI の認証情報ファイルから再読み取り
- すべて失敗 → UI にエラー表示、手動対応を促す

### 3.3 Polling 時の自動リフレッシュ統合

- `fetch_normalized_usage()` が 401 を返した場合:
  1. refresh_token が保存されていれば自動リフレッシュ試行
  2. 成功したら新トークンでリトライ
  3. 失敗したら CLI 認証情報ファイルから読み取り試行
  4. それでも失敗したらエラー表示

### 3.4 注意事項

- **CLI 連携モードの場合**: refresh_token は使わない (CLI のトークンを無効化してしまう)。
  代わりにファイルウォッチャーで CLI の更新を検知する。
- **OAuth ログインモードの場合**: 独自に取得した refresh_token を使って自動更新。
  CLI とは独立したセッションなので干渉しない。

---

## ファイル変更一覧

### 新規ファイル (Rust)
| ファイル | 説明 |
|---------|------|
| `src-tauri/src/credential_reader.rs` | CLI 認証情報ファイル読み取り |
| `src-tauri/src/credential_watcher.rs` | ファイル変更監視 |
| `src-tauri/src/token_refresh.rs` | 自動トークンリフレッシュ |
| `src-tauri/src/oauth/mod.rs` | OAuth モジュールルート |
| `src-tauri/src/oauth/pkce.rs` | PKCE 生成 |
| `src-tauri/src/oauth/callback_server.rs` | ローカルHTTPサーバー |
| `src-tauri/src/oauth/codex.rs` | Codex OAuth フロー |
| `src-tauri/src/oauth/claude.rs` | Claude OAuth フロー |
| `src-tauri/src/oauth/device_code.rs` | Device Code Flow |

### 既存ファイル変更
| ファイル | 変更内容 |
|---------|---------|
| `Cargo.toml` | 依存追加: notify, dirs, sha2, base64, rand, tiny_http |
| `src-tauri/src/main.rs` | 新モジュール宣言, 新コマンド登録, setup でウォッチャー起動 |
| `src-tauri/src/token_store.rs` | refresh_token / expires_at の保存・取得 |
| `src-tauri/src/validation.rs` | OAuth エンドポイントを allowlist 追加 |
| `src-tauri/src/api_client.rs` | 401 時のリフレッシュ統合 |
| `src-tauri/src/commands.rs` | 新コマンドハンドラー追加 |
| `src-tauri/src/account_commands.rs` | CLI インポート連携 |
| `src-tauri/tauri.conf.json` | shell-open permission (ブラウザ起動) |
| `public/index.html` | ログインボタン, ステータス表示追加 |
| `public/app.js` | ログインフロー, 自動同期 UI, 期限表示 |
| `public/account-ui.js` | 認証方式切替 UI |
| `public/styles.css` | 新規 UI 要素のスタイル |

---

## 実装優先順位

**Phase 1 (CLI連携)** が最も低リスク・高効果。CLIがインストール済みの全ユーザーに
即座に恩恵がある。ここを最優先で実装する。

Phase 2 (OAuth) は Codex 側から着手し、成功したら Claude 側を試す。
サードパーティ制限でブロックされる可能性があるため、Phase 1 が必ず動く設計にする。

Phase 3 (自動リフレッシュ) は Phase 2 で独自トークンを取得した場合にのみ
有効。Phase 1 (CLI連携) では CLI 側のリフレッシュに任せる。

---

## 今回の実装スコープ

このPRでは **Phase 1 (CLI連携モード)** を実装する。

1. CLI 認証情報ファイルの自動読み取り
2. ファイル監視による自動更新
3. UI: インポートボタン、自動同期、期限表示
4. 401 エラー時の CLI 認証情報再読み取り
