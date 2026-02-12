# Electron Migration Tracker

最終更新: 2026-02-13 03:02 (JST)
目的: Electron 移行の「計画・メモ・進捗・残件」を単一ファイルで管理する。

## 運用ルール
- 実装着手前に `計画` を更新する。
- 実装完了ごとに `進捗` と `残件` を更新する。
- 方針変更があれば `決定事項` に追記し、理由を短く残す。
- 最終的には `README.md` に必要な粒度で情報を反映する。

## 決定事項 (確定)
- Web 版は引き続き動作可能な状態で残す。
- Electron は `nodeIntegration: false` / `contextIsolation: true` / `sandbox: true` を前提にする。
- Windowsユーザを優先する。
- 必要 API は `preload` 経由で最小公開する。
- トークンは平文保存しない。Electron では OS キーチェーン (`keytar`) を使う。
- 既存 `sessionStorage` トークンの移行処理は行わない (シンプルさ優先)。
- GitHub リポジトリは public 想定。
- 自動更新は今回未実装。将来計画として `README.md` に記載する。

## 計画 (フェーズ)
- [x] P0: 追跡ドキュメント (`electron_migration.md`) を作成
- [x] P1: 共有コア (`src/core`) に API 取得/整形ロジックを分離
- [x] P2: Electron main/preload を追加し、セキュア IPC を実装
- [x] P3: Renderer を Web/Electron 両対応のデータアクセス層に再構成
- [x] P4: `keytar` でトークン永続化 (Electron のみ)
- [x] P5: `electron-builder` で Windows バイナリ生成
- [x] P6: README を刷新 (実行手順、セキュリティ方針、将来自動更新計画)
- [ ] P7: 動作確認 (Web 回帰 + Electron 回帰 + ビルド確認)

## 進捗ログ
- 2026-02-13 00:32 JST
  - 本ドキュメントを作成。
  - 移行方針を確定事項として反映。
  - 次工程は P1 (共有コア分離) から開始。
- 2026-02-13 00:38 JST
  - P1-P4 を連続実装する方針で着手。
  - 優先順: 共有コア化 -> Electron main/preload -> Renderer 両対応 -> keytar 永続化。
- 2026-02-13 00:45 JST
  - `src/core` を追加し、upstream fetch / parser / normalized usage 生成を共通化。
  - `electron/main.js` `electron/preload.js` を追加し、`nodeIntegration: false` / `contextIsolation: true` / `sandbox: true` で構成。
  - Renderer を `window.quotaApi` 検知で Web/Electron 両対応化。
  - Electron のトークン保存を `keytar` 経由に変更 (renderer 永続化なし)。
  - `public/index.html` の inline script を `public/app.js` に分離。
  - `package.json` に `start:electron` / `dist:win` / `electron-builder` 設定を追加。
  - `README.md` を新構成に合わせて更新。
  - 実行環境に `node` が無く、P7 の実動作確認は未実施。
- 2026-02-13 00:51 JST
  - `npm audit` の high 5件は `electron-builder` 系依存と判明。
  - `electron-builder` を `^26.7.0` へ更新し、`npm audit` で 0 vulnerabilities を確認。
  - Node 18 環境では `EBADENGINE` 警告が出るため、Node 22 系利用を残件化。
- 2026-02-13 01:00 JST
  - Electron のトークン入力が即時クリアされる挙動を修正 (入力値を自動クリアしない)。
  - `public/index.html` に CSP を追加し、Electron の Security Warning を解消する構成に変更。
  - `dist:win` での `@electron/rebuild` エラー回避のため `build.npmRebuild=false` を追加。
  - `package.json` / `README.md` に Node `>=22.12.0` 要件を反映。
- 2026-02-13 01:08 JST
  - Windows `dist:win` で `winCodeSign` 展開時に symlink 権限不足で失敗する事象に対応。
  - `package.json` の `build.win` に `signAndEditExecutable=false` と `signExts=["!.exe"]` を設定し、署名処理を明示的にスキップ。
  - `package.json` に `author` を追加し、builder warning を解消。
  - `README.md` に「現状は非署名前提」であることと、将来署名時の権限要件を追記。
- 2026-02-13 01:09 JST
  - 方針変更により、`build.win` の `signAndEditExecutable/signExts` を削除して署名・exe編集経路を有効化。
  - `README.md` の「非署名前提」記述を削除し、`winCodeSign` エラー時は管理者実行 or Developer Mode の運用に統一。
- 2026-02-13 01:13 JST
  - Electron でトークン永続化が不安定に見える問題への対策を実施。
  - `electron/main.js` の `fetchUsage` でアカウント metadata も補完保存するよう変更し、poll 経由でも次回起動時に復元可能化。
  - `public/app.js` の設定保存失敗をログ化し、無言失敗を解消。
  - `public/app.js` で keychain 保存状態の placeholder 表示を更新し、保存状態を視認しやすく改善。
  - トークン未設定時に setup パネルを開く処理を共通化 (`ensureSetupOpenIfMissingToken`)。
  - `README.md` に「保存済みトークンを再表示しない仕様」を追記。
- 2026-02-13 01:36 JST
  - ポーリング状態の永続化を Electron 側ストアに接続 (`quota:get/set-polling-state`)。
  - 再起動時にポーリング再開・残り時間復元が動くよう、renderer 初期化処理を更新。
  - 余白ダブルクリックでミニマル表示を切替える UI を追加。
  - ミニマル表示時はカードのみ表示し、Electron ではフレーム/メニューバー非表示モードと連動。
  - ミニマル表示の最小サイズ (カード1枚基準) と初回推奨サイズ (全カード収容) を計算して main へ渡す実装を追加。
  - `README.md` にミニマル表示とポーリング復元仕様を反映。
- 2026-02-13 02:02 JST
  - ポーリング復元方針を変更し、残り時間は保持しない設計へ更新。
  - 再起動時は「active + interval」のみ見て即時取得で再開するよう renderer を調整。
  - ミニマル表示でのウィンドウドラッグ移動を追加 (`quota:set-window-position`)。
  - ミニマル -> 通常へのダブルクリック復帰判定を緩和し、カード領域でも反応するよう修正。
- 2026-02-13 02:16 JST
  - ミニマルドラッグ時にサイズが変わる問題に対応し、ドラッグ中はサイズを固定して位置のみ更新するよう修正。
  - ミニマル時のダブルクリック復帰対象をさらに拡張し、コンテナ外の背景でも復帰可能に調整。
  - ミニマル時のカード幅上限を導入し、ウィンドウ幅に追従して過剰に横伸びしないように修正。
- 2026-02-13 02:28 JST
  - ミニマル最小幅計算を調整し、カード幅上限 (`--minimal-card-width`) を超えて最小幅が肥大化しないよう修正。
  - ミニマル表示を最前面表示に変更 (`alwaysOnTop`)。
  - Windows では `backgroundMaterial: acrylic` と renderer 側の半透明スタイルで磨りガラス風表示を試行する構成を追加。
- 2026-02-13 02:36 JST
  - 磨りガラス風表示は撤回し、Electron 側の透明化/アクリル設定と renderer 側の blur 表現を削除。
  - ミニマル時カード幅をさらに縮小 (`--minimal-card-width: 420px`) し、カード間隔・内側余白も詰めてコンパクト化。
- 2026-02-13 02:41 JST
  - 通常モードで container 外背景のダブルクリックが反応しない問題を修正。
  - ミニマル切替判定から `.container` 制約を外し、非入力領域なら画面下部背景でも切替可能にした。
- 2026-02-13 03:02 JST
  - 保存済みトークンの表示を placeholder 文言から伏せ字ダミー値表示へ変更。
  - 伏せ字ダミー値が keychain を上書きしないよう、renderer 収集時にマスク判定して除外する処理を追加。
  - アプリバージョンを `1.0.0` に設定し、タイトル横へ動的表示を追加 (Electron: `getVersion`, Web: `/api/version`)。

## 残件 (Open Items)
- P7: Node/npm がある環境で `npm install` -> `npm run start:web` / `npm run start:electron` / `npm run dist:win` を実機確認。
- `build.npmRebuild=false` で作成した配布物で keytar が正常に動作するか Windows 実機で検証。
- Windows で `dist:win` 実行時に symlink 権限が必要な環境では、管理者実行または Developer Mode を前提運用とする。
- ミニマル表示切替の UX を実機で確認 (余白ダブルクリック判定、最小サイズ、初回サイズ、サイズ記憶)。
- Electron 再起動跨ぎでポーリング復元が想定通りか確認 (即時取得されること、次回ポーリング間隔)。
- Electron の通知を renderer `Notification` のまま継続するか、main 側 Notification API に寄せるか判断。
- Windows 配布物を `nsis` のみで固定するか、portable 併用するか判断。

## メモ
- `public/index.html` のロジックは `public/app.js` へ分割済み。
- 既存 Web の API エンドポイント (`/api/claude`, `/api/codex`) は維持予定。
- 初回は「実行可能な統合」を優先し、見た目改善は後段に回す。

## 直近アクション
1. Node/npm を導入した環境で P7 の回帰確認を実施する。
2. Windows 実機で `dist` 生成物の起動確認を実施する。
3. 必要なら通知方式と配布ターゲットを確定し README に反映する。
