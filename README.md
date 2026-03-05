# AI Quota Monitor

Claude Code / Codex のクォータ使用状況を監視する Tauri デスクトップアプリ。

## 機能

- 複数アカウント対応 (Claude Code / Codex それぞれ複数登録可)
- ステータス自動分類 (`ok` / `warning` / `critical` / `exhausted`) とカード左端の色分け表示
- 通知設定パネル: 悪化・回復時のデスクトップ通知
- 使用量JSON出力: 取得した使用量/リセット時刻などをJSONファイルに書き出し (外部監視向け)
- 閾値カスタマイズ: `warning` / `critical` の % を変更可能 (`exhausted` は 100% 固定)
- ダブルクリックでミニマル表示 (カードのみ) に切替
- ミニマル表示はウィンドウ幅に応じてカラム数が自動増減
- 保存済みトークンの伏せ字表示 (平文再表示なし)

## 主要方針

- フロントは `window.quotaApi` のみを利用し、Tauri コマンド経由で backend と通信する。
- トークンは平文保存せず OS キーチェーンに保存する。
- 設定 (`pollInterval`, 通知閾値, ウィンドウ状態など) は `appData/accounts.json` に永続化する。

## 使用量JSON出力

取得した使用量情報を、外部ソフトが監視しやすいようにJSONファイルとして書き出します（取得のたびに上書き）。

- UI: `💾 使用量JSON出力`
- パス指定:
  - 絶対パス: そのまま書き込み
  - 相対パス: アプリデータフォルダ配下に保存
- `出力先ファイルパス` が空の状態では `有効` にできません（安全のため no-op）

### フォーマット

```json
{
  "schemaVersion": 1,
  "appName": "AI Quota Monitor",
  "appVersion": "0.0.4",
  "generatedAt": "2026-02-14T12:34:56Z",
  "fetchedAt": "2026-02-14T12:34:56.789Z",
  "entries": [
    {
      "service": "claude",
      "id": "account1",
      "name": "My Claude",
      "hasToken": true,
      "label": "Claude Code: My Claude",
      "status": "ok",
      "windows": [
        {
          "name": "5時間",
          "utilization": 12.3,
          "resetsAt": "2026-02-14T15:00:00Z",
          "status": "ok",
          "windowSeconds": 18000
        }
      ],
      "error": null
    }
  ]
}
```

補足:
- `entries[].windows[].resetsAt` は upstream により「文字列/数値/null」があり得ます（そのまま格納します）。
- `entries[].status` は `ok|warning|critical|exhausted|error|unknown` のいずれかになります。

## アーキテクチャ

```text
Renderer (public/*.js)
  -> tauri-bridge.js
  -> Tauri invoke command (Rust)
  -> keyring + upstream fetch
```

- `src/core` は既存の JS テスト資産として保持。
- 実行時の API 取得・ストア管理は `src-tauri/src/main.rs` で実装。

## セットアップ

Node.js と Rust ツールチェーンが必要。
推奨: Node.js `22.x` (最低 `22.12.0`)。

```bash
npm install
```

## 実行

```bash
npm start
# または
npm run start:tauri
```

操作メモ:
- ボタン・入力欄以外をダブルクリックでミニマル表示/通常表示を切替。
- ミニマル表示中は画面内ドラッグでウィンドウ移動可能。

## テスト

```bash
npm test
```

`node:test` ベースで、JS ロジックの単体検証を行う。

## ビルド

```bash
npm run build:tauri
```

Windows では `src-tauri/target/release/bundle/` に NSIS / MSI インストーラが出力される。

Zip 配布物も作る場合:

```bash
npm run build:zip
```

`src-tauri/target/release/bundle/zip/AI-Quota-Monitor-<version>-windows-x64.zip` が出力される。

## リリース手順

### 1. バージョンを上げる

以下の3ファイルのバージョン番号を揃えて更新する:

| ファイル | 箇所 |
|---|---|
| `package.json` | `"version": "x.y.z"` |
| `src-tauri/Cargo.toml` | `version = "x.y.z"` |
| `src-tauri/tauri.conf.json` | `"version": "x.y.z"` |

### 2. テストを実行

```bash
npm test
```

### 3. ビルド & Zip 作成

```bash
npm run build:zip
```

成功すると `src-tauri/target/release/bundle/zip/AI-Quota-Monitor-<version>-windows-x64.zip` が生成される。

インストーラのみ必要な場合:

```bash
npm run build:tauri
# → src-tauri/target/release/bundle/ に NSIS / MSI が出力される
```

### 4. コミット & タグ

```bash
git add package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json
git commit -m "v<version>"
git tag v<version>
git push origin main --tags
```

### 5. GitHub Release

- `Releases` → `Draft a new release` でタグを選択
- Zip ファイルをアタッチしてリリースを公開する

---

## ログイン方法 (推奨)

アプリ内の `🔗 URLコピー` でログインURLをコピーして OAuth ログインできます。トークンは OS のキーチェーンに保存され、`refresh_token` / 有効期限が取れている場合は期限前に自動更新されます。

### Claude Code

1. アカウント行の `🔗 URLコピー` を押す
2. ブラウザで認証を完了する
3. 表示された `code#state`（またはリダイレクト先URL全体）を、アプリのプロンプトに貼り付ける

補足:
- `📥 CLI取込` ボタンで `~/.claude/.credentials.json` から取り込みもできます（OAuth ログインがうまくいかない場合のフォールバック）。

### Codex (OpenAI)

1. アカウント行の `🔗 URLコピー` を押す
2. ブラウザで認証を完了する
3. ローカルコールバック (`http://localhost:1455/auth/callback`) を受け取ると自動でログイン完了する

補足:
- ポート `1455` を使用します。Codex CLI 等で使用中だとログインに失敗するので、先に終了してください。

## 手動トークン (デバッグ/フォールバック)

手動でアクセストークンを貼り付けて使うこともできます（この場合、有効期限/refresh 情報が無いので自動更新は効きません）。

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
├── public/
│   ├── app.js
│   ├── account-ui.js
│   ├── tauri-bridge.js
│   ├── index.html
│   └── ui-logic.js
├── src-tauri/
│   ├── src/main.rs
│   ├── tauri.conf.json
│   └── Cargo.toml
├── src/
│   └── core/
│       ├── parsers.js
│       ├── usage-clients.js
│       └── usage-service.js
├── test/
│   ├── core/
│   └── ui/
└── package.json
```
