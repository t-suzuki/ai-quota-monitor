# トークンリニューアル対応 実装計画

## 現状の問題

元々は CLI (Claude Code / Codex) が管理するトークンを**ユーザーが手動コピー&ペースト**して利用していたが、
現在はアプリ内で OAuth ログインでき、`refresh_token` / 有効期限が取れている場合は期限前に自動更新できる。

- `~/.claude/.credentials.json` や `~/.codex/auth.json` からユーザーが手動でトークンを取得
- 手動トークン貼り付けは引き続き可能（ただしこの場合は有効期限/refresh 情報が無いので自動更新できない）

## 調査結果サマリ

### Claude Code の OAuth

| 項目 | 値 |
|------|-----|
| Client ID | `9d1c250a-e61b-44d9-88ed-5944d1962f5e` |
| Authorization | `https://claude.ai/oauth/authorize` (`code=true`) |
| Token | `https://platform.claude.com/v1/oauth/token` |
| PKCE | S256 |
| Redirect | `https://platform.claude.com/oauth/code/callback` |
| Access Token 寿命 | 8時間 (28800秒) |
| Access Token prefix | `sk-ant-oat01-` |
| Refresh Token prefix | `sk-ant-ort01-` |
| Scope | `org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers` |

補足:
- Claude の認可後に表示されるコードが `code#state` 形式になるため、アプリ側は `state` を含めて token exchange を行う必要がある。

### Codex の OAuth

| 項目 | 値 |
|------|-----|
| Client ID | `app_EMoamEEZ73f0CkXaXp7hrann` |
| Authorization | `https://auth.openai.com/oauth/authorize` |
| Token | `https://auth.openai.com/oauth/token` |
| PKCE | S256 |
| Redirect | `http://localhost:1455/auth/callback` (固定) |
| Refresh Token | Single-use rotation 方式 |
| Extra query | `id_token_add_organizations=true`, `codex_cli_simplified_flow=true`, `originator=codex_cli_rs` |

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

1. **CLI 取り込み** (Phase 1) — CLI が管理する認証情報を手動取り込み（現状: Claude のみ）
2. **OAuth ログイン** (Phase 2) — アプリ内でブラウザ認証フローを実行
3. **自動トークンリフレッシュ** (Phase 3) — refresh_token による自動更新

---

## Phase 1: CLI 連携モード (credential file auto-import)

### 概要
CLI が管理する認証情報を取り込む方式。現在は **Claude のみ** `~/.claude/.credentials.json` から手動取り込みを提供している。
ファイルウォッチによる自動同期は将来拡張の余地あり。

### 実装内容

#### 1.1 Tauri コマンド（実装済み）

- `import_claude_cli_credentials(service, id)` — `~/.claude/.credentials.json` を読み取り、トークンを保存

#### 1.4 UI 変更

- Claude アカウント行に `📥 CLI取込` ボタン追加（`~/.claude/.credentials.json`）
- OAuth ログインの結果/エラーの表示

#### 1.5 バックエンド変更

- `validation.rs`: `auth.openai.com` を allowlist に追加
- `token_store.rs`: `refresh_token` / `expires_at` の保存対応

---

## Phase 2: 独立 OAuth ログイン

### 概要
アプリ内でブラウザベースの OAuth PKCE フローを実行し、CLI に依存しない
独自のトークンペアを取得する。

### 2.1 Codex OAuth (実現可能性: 高)

- PKCE (S256) で `code_verifier` / `code_challenge` を生成
- ローカル HTTP コールバック (`http://localhost:1455/auth/callback`) を待ち受け
- ブラウザを開いて `https://auth.openai.com/oauth/authorize` へリダイレクト（追加クエリあり）
- コールバックで authorization code を受取り、token endpoint で交換
- access_token + refresh_token を keyring に保存

実装:
- `src-tauri/src/oauth/mod.rs` — OAuth モジュール
- `src-tauri/src/oauth/pkce.rs` — PKCE 生成
- `src-tauri/src/oauth/callback_server.rs` — ローカルHTTPサーバー
- `src-tauri/src/oauth/codex.rs` — Codex 固有のフロー

#### 2.2 Claude OAuth (実現可能性: 中〜低)

同様のPKCEフローだが、Anthropic のサードパーティ制限により
ブロックされる可能性がある。

対応方針:
- まず実装して実際に試す
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
- 期限の5分前にリフレッシュを実行（フロントエンドが `get_token_status` を見て発火）
- Claude: `POST https://platform.claude.com/v1/oauth/token` (JSON)
- Codex: `POST https://auth.openai.com/oauth/token`
- 新しいトークンペアを keyring に保存
- フロントエンドに通知

### 3.2 リフレッシュ失敗時のフォールバック

- 401 応答 → ユーザーに再ログインを促す
- ネットワークエラー → exponential backoff でリトライ (最大3回)
- すべて失敗 → UI にエラー表示、手動対応を促す

### 3.3 Polling 時の自動リフレッシュ統合

- ポーリング前に `get_token_status` を確認し、期限が近く `refresh_token` がある場合は `refresh_token` を実行する。
- 現状は 401 検知後の自動リトライは未対応（必要なら今後追加する）。

### 3.4 注意事項

- **CLI 由来の認証情報**: refresh_token rotation のあるサービス（例: Codex）では、CLI と同じ refresh_token を使うと CLI 側のトークンが無効化される可能性がある。
  推奨はアプリ内 `🔐 ログイン` で独立トークンを取得すること。
- **OAuth ログインモードの場合**: 独自に取得した refresh_token を使って自動更新。
  CLI とは独立したセッションなので干渉しない。

---

## ファイル変更一覧

### 新規ファイル (Rust)
| ファイル | 説明 |
|---------|------|
| `src-tauri/src/token_refresh.rs` | 自動トークンリフレッシュ |
| `src-tauri/src/oauth/mod.rs` | OAuth モジュールルート |
| `src-tauri/src/oauth/pkce.rs` | PKCE 生成 |
| `src-tauri/src/oauth/callback_server.rs` | ローカルHTTPサーバー |
| `src-tauri/src/oauth/codex.rs` | Codex OAuth フロー |
| `src-tauri/src/oauth/claude.rs` | Claude OAuth フロー |

### 既存ファイル変更
| ファイル | 変更内容 |
|---------|---------|
| `Cargo.toml` | 依存追加: sha2, base64, rand, tokio, open |
| `src-tauri/src/main.rs` | OAuth/リフレッシュ関連のコマンド登録、定数整理 |
| `src-tauri/src/token_store.rs` | refresh_token / expires_at の保存・取得 |
| `src-tauri/src/validation.rs` | OAuth エンドポイントを allowlist 追加 |
| `src-tauri/src/commands.rs` | 新コマンドハンドラー追加 |
| `src-tauri/src/oauth_commands.rs` | OAuth ログイン, Claude CLI 取り込み, トークン状態取得 |
| `src-tauri/tauri.conf.json` | shell-open permission (ブラウザ起動) |
| `public/index.html` | ログインボタン, ステータス表示追加 |
| `public/app.js` | ログインフロー, 自動同期 UI, 期限表示 |
| `public/account-ui.js` | 認証方式切替 UI |
| `public/styles.css` | 新規 UI 要素のスタイル |

---

## 今後の課題

1. 401 検知後の `refresh_token` 自動リトライ（サービスごとに安全性を評価）
2. Codex の CLI 取り込み（実装するなら refresh_token rotation に注意）
3. Claude のログイン UX 改善（`prompt()` ではなくUIダイアログ化）
