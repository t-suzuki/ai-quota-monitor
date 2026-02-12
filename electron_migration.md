# Electron Migration Tracker

最終更新: 2026-02-13 00:32 (JST)
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
- [ ] P1: 共有コア (`src/core`) に API 取得/整形ロジックを分離
- [ ] P2: Electron main/preload を追加し、セキュア IPC を実装
- [ ] P3: Renderer を Web/Electron 両対応のデータアクセス層に再構成
- [ ] P4: `keytar` でトークン永続化 (Electron のみ)
- [ ] P5: `electron-builder` で Windows バイナリ生成
- [ ] P6: README を刷新 (実行手順、セキュリティ方針、将来自動更新計画)
- [ ] P7: 動作確認 (Web 回帰 + Electron 回帰 + ビルド確認)

## 進捗ログ
- 2026-02-13 00:32 JST
  - 本ドキュメントを作成。
  - 移行方針を確定事項として反映。
  - 次工程は P1 (共有コア分離) から開始。

## 残件 (Open Items)
- 共有コアの責務境界をどこまで切るか (fetch + parse までを想定)。
- Renderer の巨大 inline script を `public/app.js` へ分離するタイミング。
- Electron で通知 API をどう扱うか (現状の `Notification` を維持するか)。
- Windows 配布物のターゲット詳細 (`nsis` 単体か portable 併用か)。

## メモ
- 現行は `public/index.html` にロジック集中しているため、段階的分割が必要。
- 既存 Web の API エンドポイント (`/api/claude`, `/api/codex`) は維持予定。
- 初回は「実行可能な統合」を優先し、見た目改善は後段に回す。

## 直近アクション
1. `server.js` の Claude/Codex fetch + parse 周辺をコア化する設計を固める。
2. Electron 側の IPC 契約 (`quotaApi`) を先に固定する。
3. その後 Renderer 側を API アダプタで差し替える。
