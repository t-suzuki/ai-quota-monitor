# AI Quota Monitor

Claude Code / Codex のクォータ使用状況をブラウザからモニタリングするサービス。  
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
│ /api/claude  │ │ /api/codex   │   Vercel Edge Functions
│ (CORS proxy) │ │ (CORS proxy) │   ステートレス・ログなし
└──────┬───────┘ └──────┬───────┘
       ▼               ▼
  Anthropic API   ChatGPT API
```

**設計原則:**
- サーバーはトークンを**保存しない** (メモリにも残らないEdge Function)
- ポーリングはブラウザ側 (サーバーコストほぼゼロ)
- トークンはブラウザの `sessionStorage` に保持 (タブを閉じると消える)

## デプロイ

### Vercel (推奨)

```bash
npm i -g vercel
cd quota-monitor
vercel
```

### ローカル開発

```bash
npm run dev
# → http://localhost:4173
```

注:
- `npm run dev` は `public/` と `/api/*` を同時に配信するローカルサーバーです。
- `npm run dev:static` は静的確認用 (`public/` のみ) です。
- `npx vercel dev` は Project Settings の影響を受けるため、必要な場合のみ利用してください。

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
# 旧形式の環境では ~/.config/claude/credentials.json の場合あり
```

**Windows (PowerShell):**
```powershell
(Get-Content "$env:USERPROFILE\.claude\.credentials.json" -Raw | ConvertFrom-Json).claudeAiOauth.accessToken
```

**API エンドポイント:** `GET https://api.anthropic.com/api/oauth/usage`  
`anthropic-beta: oauth-2025-04-20` ヘッダーが必須

レスポンス例:
```json
{
  "five_hour": { "utilization": 12.5, "resets_at": "2025-11-04T04:59:59Z" },
  "seven_day": { "utilization": 35.0, "resets_at": "2025-11-06T03:59:59Z" },
  "seven_day_opus": { "utilization": 0.0, "resets_at": null },
  "seven_day_sonnet": { "utilization": 0.0, "resets_at": null },
  "extra_usage": { "is_enabled": false, "utilization": null }
}
```

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

主に以下の形式で返ります:
```json
{
  "rate_limit": {
    "allowed": true,
    "limit_reached": false,
    "primary_window": { "used_percent": 0, "limit_window_seconds": 18000, "reset_at": 1770922775 },
    "secondary_window": { "used_percent": 11, "limit_window_seconds": 604800, "reset_at": 1771404122 }
  },
  "code_review_rate_limit": {
    "allowed": true,
    "limit_reached": false,
    "primary_window": { "used_percent": 0, "limit_window_seconds": 604800, "reset_at": 1771509575 },
    "secondary_window": null
  },
  "additional_rate_limits": null,
  "credits": { "has_credits": false, "balance": "0" }
}
```

このモニターは上記形式 (`rate_limit` / `code_review_rate_limit`) を直接パースし、表示します。

**代替経路 (ローカルのみ):**
```bash
# codex app-server の JSON-RPC で取得
codex app-server
# → {"method":"account/rateLimits/read","id":1}
# ← primary/secondary window + credits
```

## Codex の調査結果

Codex CLIのクォータ取得経路は3つ確認済み:

| 経路 | 方式 | Web利用 |
|------|------|---------|
| `codex app-server` JSON-RPC | `account/rateLimits/read` メソッド | ❌ ローカルのみ |
| Web API | `GET chatgpt.com/backend-api/wham/usage` + Bearer token | ✅ プロキシ経由 |
| HTTPレスポンスヘッダー | API呼び出し時のヘッダーから `parse_rate_limit_snapshot()` | ❌ 内部のみ |

Web APIはCodexBarが実際に使用している方式で、`~/.codex/auth.json` の `access_token` で認証。

ソースコード上の構造:
```rust
RateLimitSnapshot {
    primary: RateLimitWindow { used_percent, window_minutes, resets_at },   // 5時間
    secondary: RateLimitWindow { used_percent, window_minutes, resets_at }, // 週間
    credits: CreditsSnapshot { ... },
}
```

## プロキシのセキュリティ

- Edge Function は完全ステートレス (リクエスト処理後メモリ解放)
- トークンはログに記録されない
- 固定の upstream URL のみにリクエスト転送 (オープンリレーにならない)
- CORS ヘッダーで制御

## トラブルシュート

- `Claude` が `401` で `OAuth authentication is currently not supported.` を返す場合:
  - `anthropic-beta: oauth-2025-04-20` ヘッダーが不足している可能性があります。
  - このリポジトリの `api/claude.js` / `dev-server.js` は対応済みです。
- `Codex` が `403` で HTML (`Unable to load site`) を返す場合:
  - `chatgpt.com` 側でデプロイ先IP (例: Vercel egress) が WAF/Cloudflare によりブロックされている可能性があります。
  - トークン不正とは限りません。ローカル実行 (`npm run dev`) では通る場合があります。

## ファイル構成

```
quota-monitor/
├── api/
│   ├── claude.js     # Anthropic API プロキシ (Edge)
│   └── codex.js      # ChatGPT API プロキシ (Edge)
├── public/
│   └── index.html    # SPA ダッシュボード
├── vercel.json       # Vercel設定
├── package.json
└── README.md
```

## TODO

- [x] Codex `wham/usage` のレスポンス形式を実環境で確認
- [ ] トークンの自動リフレッシュ (refresh_token 対応)
- [ ] Gemini CLI 対応
- [ ] Webhooks (Slack/Discord) — Vercel Cron で定期実行する場合
- [ ] ユーザーごとのトークン暗号化 (WebCrypto)
