# AI Quota Monitor

Claude Code / Codex のクォータ使用状況を監視する Electron デスクトップアプリ。

## 機能

- 複数アカウント対応 (Claude Code / Codex それぞれ複数登録可)
- ステータス自動分類 (`ok` / `warning` / `critical` / `exhausted`) とカード左端の色分け表示
- 通知設定パネル: 悪化・回復時のデスクトップ通知
- 閾値カスタマイズ: `warning` / `critical` の % を変更可能 (`exhausted` は 100% 固定)
- ダブルクリックでミニマル表示 (カードのみ) に切替
- ミニマル表示はウィンドウ幅に応じてカラム数が自動増減
- 保存済みトークンの伏せ字表示 (平文再表示なし)

## 主要方針

- Electron は `nodeIntegration: false` / `contextIsolation: true` / `sandbox: true`。
- レンダラには `preload` 経由で必要 API のみ公開する。
- トークンは平文保存せず OS キーチェーン (`keytar`) に保存する。
- 設定 (`pollInterval`, 通知閾値, ウィンドウ状態など) は Electron ストア (`userData/accounts.json`) に永続化する。
- 自動更新は将来対応 (今回は未実装)。

## アーキテクチャ

```text
Renderer (SPA)
  -> preload (contextBridge)
  -> ipcMain (main process)
  -> keytar + upstream fetch
```

- `src/core` に upstream 取得とパース処理を集約。
- renderer は `window.quotaApi` 経由で main process と通信。
- ポーリング状態は再起動時に復元し、即時取得して通常間隔で再開。

## セットアップ

Node.js と npm が必要。
推奨: Node.js `22.x` (最低 `22.12.0`)。

```bash
npm install
```

## 実行

```bash
npm start
# または
npm run start:electron
```

操作メモ:
- ボタン・入力欄以外をダブルクリックでミニマル表示/通常表示を切替。
- ミニマル表示中は画面内ドラッグでウィンドウ移動可能。

## テスト

```bash
npm test
```

`node:test` ベースで、API 取得処理はモック注入で検証する。

## Windows バイナリ作成

```bash
npm run dist:win
```

生成物は `dist/` 配下に出力される。
インストーラは NSIS の対話型モード (one-click 無効) で、ユーザの明示操作を必須にしている。

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

## 構成

```text
ai-quota-monitor/
├── electron/
│   ├── main.js
│   ├── preload.js
│   └── store.js
├── public/
│   ├── app.js
│   ├── account-ui.js
│   ├── index.html
│   └── ui-logic.js
├── src/
│   └── core/
│       ├── parsers.js
│       ├── usage-clients.js
│       └── usage-service.js
├── test/
│   ├── core/
│   │   ├── parsers.test.js
│   │   ├── usage-clients.test.js
│   │   └── usage-service.test.js
│   └── ui/
│       └── ui-logic.test.js
├── package.json
└── README.md
```
