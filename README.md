# AI Quota Monitor

Claude Code / Codex のクォータ使用状況を監視するツール。

- Web 版: `node server.js` で起動するブラウザ向け SPA
- Electron 版: デスクトップアプリ (Windows バイナリ配布想定)

## 主要方針

- Web 版は継続サポートする。
- Electron 版は `nodeIntegration: false` / `contextIsolation: true` / `sandbox: true`。
- レンダラには `preload` 経由で必要 API のみ公開する。
- Electron のトークンは平文保存せず OS キーチェーン (`keytar`) に保存する。
- 自動更新は将来対応 (今回は未実装)。

## アーキテクチャ

### Web 版

```text
Browser SPA -> /api/claude,/api/codex (Node server proxy) -> Upstream APIs
```

- トークンはブラウザ側セッション保存。
- `server.js` は静的配信 + API プロキシ。

### Electron 版

```text
Renderer (SPA)
  -> preload (contextBridge)
  -> ipcMain (main process)
  -> keytar + upstream fetch
```

- トークンは `keytar` 管理 (アカウント metadata は `userData/accounts.json`)。
- レンダラはトークン平文を永続化しない。
- 保存済みトークンは再起動後に入力欄へ再表示しない (placeholder の「保存済み」で状態表示)。
- ポーリング状態 (実行中/停止中・次回更新までの残り時間算出に必要な時刻) を永続化し、再起動後に復元する。
- 余白ダブルクリックでミニマル表示 (カードのみ) に切替できる。Electron ではミニマル時にタイトルバー/メニューバーを非表示にする。
- ミニマル表示の最小サイズ・初回サイズを自動計算し、ユーザが変更したサイズは保持する。

## セットアップ

Node.js と npm が必要。
推奨: Node.js `22.x` (最低 `22.12.0`)。

```bash
npm install
```

## 実行

### Web 版

```bash
npm run start:web
# または npm start
# -> http://localhost:4173
```

ポート変更:

```bash
PORT=8080 npm run start:web
```

### Electron 版

```bash
npm run start:electron
```

操作メモ:
- 余白をダブルクリックするとミニマル表示/通常表示を切り替える。
- ミニマル表示ではカード以外の UI を隠し、コンパクト表示に最適化される。

## Windows バイナリ作成

```bash
npm run dist:win
```

生成物は `dist/` 配下に出力される。

`@electron/rebuild` 系のエラーが出る場合は、Node.js バージョンが古い可能性が高いため `22.x` へ更新する。
`winCodeSign` 展開時に symlink 作成エラーが出る場合は、Windows 側を管理者実行するか Developer Mode を有効化して再実行する。

## トークン取得方法

### Claude Code

- macOS

```bash
security find-generic-password -s "Claude Code-credentials" -w \
  | python3 -c "import sys,json; print(json.loads(sys.stdin.read())['claudeAiOauth']['accessToken'])"
```

- Linux

```bash
cat ~/.claude/.credentials.json | jq -r '.claudeAiOauth.accessToken'
```

- Windows (PowerShell)

```powershell
(Get-Content "$env:USERPROFILE\.claude\.credentials.json" -Raw | ConvertFrom-Json).claudeAiOauth.accessToken
```

### Codex (OpenAI)

- Linux / macOS

```bash
cat ~/.codex/auth.json | jq -r '.tokens.access_token'
```

- macOS (Keychain)

```bash
security find-generic-password -s "Codex Auth" -w | jq -r '.tokens.access_token'
```

- Windows (PowerShell)

```powershell
(Get-Content "$env:USERPROFILE\.codex\auth.json" -Raw | ConvertFrom-Json).tokens.access_token
```

## トラブルシュート

- Claude が `401`: トークン期限切れの可能性。
- Codex が `403` + HTML: upstream の edge/WAF でブロックされた可能性。

## 自動更新 (計画)

今回は未実装。将来は public GitHub repository の Releases を配布源として、Electron 自動更新を導入予定。

## 構成

```text
quota-monitor/
├── electron/
│   ├── main.js
│   └── preload.js
├── src/
│   └── core/
│       ├── parsers.js
│       ├── usage-clients.js
│       └── usage-service.js
├── public/
│   ├── index.html
│   └── app.js
├── server.js
├── electron_migration.md
├── package.json
└── README.md
```
