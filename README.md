# AI Quota Monitor

Claude Code / Codex のクォータ使用状況を監視するデスクトップ & Web ツール。

- Web 版: `node server.js` で起動するブラウザ向け SPA
- Electron 版: デスクトップアプリ (Windows バイナリ配布想定)

## 機能

- 複数アカウント対応 (Claude Code / Codex それぞれ複数登録可)
- ステータス自動分類 (ok / warning / critical / exhausted) とカード左端の色分け表示
- 通知設定パネル: 悪化・回復時のデスクトップ通知 (critical/exhausted, warning, 回復の各トグル)
- 閾値カスタマイズ: warning / critical の % をユーザが変更可能 (exhausted は 100% 固定)
- 閾値変更時にカードのステータスが即座に再分類・再描画され、遷移通知も発火する
- ミニマル表示: ダブルクリックでカードのみのコンパクト表示に切替
- ミニマル表示はウィンドウ幅に応じてカラム数が自動増減 (auto-fill grid)
- トークンのあるアカウントは起動直後からプレースホルダーカードを表示

## 主要方針

- Web 版は継続サポートする。
- Electron 版は `nodeIntegration: false` / `contextIsolation: true` / `sandbox: true`。
- レンダラには `preload` 経由で必要 API のみ公開する。
- Electron のトークンは平文保存せず OS キーチェーン (`keytar`) に保存する。
- 通知設定・閾値は Electron ストア (`userData/accounts.json`) に永続化する (Web 版は sessionStorage)。
- ステータス分類はレンダラー側に一元化し、閾値変更が即座にカード表示と通知に反映される。
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
- 保存済みトークンは再起動後に入力欄へ平文再表示せず、伏せ字ダミー表示で存在のみ示す。
- ポーリング状態 (実行中/停止中 + 間隔) を永続化し、再起動時は即時取得して通常間隔で再開する。
- 通知設定・閾値は Electron ストアに永続化され、ウィンドウ再生成やアプリ再起動後も維持される。
- ボタン・入力欄など操作対象以外のどこでもダブルクリックでミニマル表示に切替できる。Electron ではミニマル時にタイトルバー/メニューバーを非表示にする。
- ミニマル表示の最小サイズ・初回サイズを自動計算し、ユーザが変更したサイズは保持する。
- ミニマル表示ではウィンドウ幅に応じて自動的にカラム数が増減する。
- ミニマル表示は最前面 (always-on-top) 表示にする。

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
- ボタン・入力欄以外の場所をダブルクリックするとミニマル表示/通常表示を切り替える。
- ミニマル表示ではカード以外の UI を隠し、コンパクト表示に最適化される。
- ミニマル表示中は画面内ドラッグでウィンドウを移動できる。

## Windows バイナリ作成

```bash
npm run dist:win
```

生成物は `dist/` 配下に出力される。
インストーラは NSIS の対話型モード (one-click 無効) で、ユーザの明示操作を必須にしている。
デスクトップショートカット / スタートメニュー登録は既定で ON。

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
ai-quota-monitor/
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
├── package.json
└── README.md
```
