# AI Quota Monitor

Claude Code / Codex のクォータ使用状況をブラウザからモニタリングするツール。
Claude / Codex はそれぞれ複数アカウントを登録でき、表示名をつけて個別に監視できる。

## アーキテクチャ

```
┌─────────────────────────────────────┐
│  ブラウザ (SPA)                       │
│  ├ トークン: sessionStorage (任意)     │
│  ├ ポーリング: setInterval            │
│  ├ 閾値判定 + Browser Notifications   │
│  └ ダッシュボード描画                   │
└──────┬───────────────┬──────────────┘
       │ x-quota-token │
       ▼               ▼
┌──────────────┐ ┌──────────────┐
│ /api/claude  │ │ /api/codex   │   Node.js サーバー
│ (CORS proxy) │ │ (CORS proxy) │   ステートレス・ログなし
└──────┬───────┘ └──────┬───────┘
       ▼               ▼
  Anthropic API   ChatGPT API
```

**設計原則:**
- サーバーはトークンを**保存しない**
- ポーリングはブラウザ側 (サーバーコストほぼゼロ)
- トークンはブラウザの `sessionStorage` に保持 (タブを閉じると消える)
- 外部依存パッケージなし (Node.js 標準モジュールのみ)

## 起動

```bash
node server.js
# → http://localhost:4173
```

ポートは環境変数で変更可能:

```bash
PORT=8080 node server.js
```

## トークン取得方法

UI では Claude / Codex それぞれ `+ 追加` でアカウント行を増やせます。
各行に任意の表示名を付けると、ダッシュボード上で `サービス名: 表示名` として区別されます。

### Claude Code

OAuth トークンが必要。以下のいずれかで取得:

**macOS:**
```bash
security find-generic-password -s "Claude Code-credentials" -w \
  | python3 -c "import sys,json; print(json.loads(sys.stdin.read())['claudeAiOauth']['accessToken'])"
```

**Linux:**
```bash
cat ~/.claude/.credentials.json | jq -r '.claudeAiOauth.accessToken'
```

**Windows (PowerShell):**
```powershell
(Get-Content "$env:USERPROFILE\.claude\.credentials.json" -Raw | ConvertFrom-Json).claudeAiOauth.accessToken
```

**API エンドポイント:** `GET https://api.anthropic.com/api/oauth/usage`
`anthropic-beta: oauth-2025-04-20` ヘッダーが必須

### Codex (OpenAI)

ChatGPT OAuth の access_token が必要:

```bash
cat ~/.codex/auth.json | jq -r '.tokens.access_token'
```

macOS (Keychain保存の場合):
```bash
security find-generic-password -s "Codex Auth" -w | jq -r '.tokens.access_token'
```

Windows (PowerShell):
```powershell
(Get-Content "$env:USERPROFILE\.codex\auth.json" -Raw | ConvertFrom-Json).tokens.access_token
```

**API エンドポイント:** `GET https://chatgpt.com/backend-api/wham/usage`

## トラブルシュート

- `Claude` が `401` を返す場合:
  - トークンが期限切れの可能性があります。再取得してください。
- `Codex` が `403` で HTML を返す場合:
  - `chatgpt.com` 側で WAF/Cloudflare によりブロックされている可能性があります。
  - トークン不正とは限りません。

## ファイル構成

```
quota-monitor/
├── server.js         # Node.js サーバー (静的配信 + APIプロキシ)
├── public/
│   └── index.html    # SPA ダッシュボード
├── package.json
└── README.md
```
