# Refactor Tracker

最終更新: 2026-02-13 (JST)
目的: Electron 専用化とテスト拡充を進め、可読性と保守性を改善する。

## 方針
- Web 版 (`server.js` + `/api/*` プロキシ) は廃止し、Electron 実行に一本化する。
- 主要ロジックはテスト可能な純粋関数・注入可能な依存へ寄せる。
- 既存挙動が常に正しい前提は置かず、問題が見つかれば同時修正する。

## 計画
- [done] P1: 現状把握と変更方針の確定
- [done] P2: Web 版コード・設定・ドキュメントの削除
- [done] P3: ロジック分割 (依存注入/純粋関数化) による見通し改善
- [done] P4: モック中心のテスト追加 (`node:test`)
- [in_progress] P5: テスト実行と残課題整理

## 進捗ログ
- 2026-02-13
  - `public/app.js` の Web/Electron 分岐、`server.js` 依存、`src/core` の重複/結合ポイントを調査。
  - 本トラッカー (`recfactor.md`) を作成。
  - Web 版を廃止:
    - `server.js` を削除。
    - `package.json` の `start:web` と web 前提の `start` を整理 (`start=electron`)。
    - `README.md` を Electron 専用の説明へ更新。
  - renderer の分岐整理:
    - `public/app.js` から Web fallback と `/api/*` fetch + 重複 parser を削除。
    - `window.quotaApi` 前提に一本化し、`pollAll` を `pollServiceAccounts` に分割。
  - core のテスト容易化:
    - `src/core/usage-service.js` を DI 対応 (`createUsageService`) に変更。
    - `src/core/usage-clients.js` を DI 対応 (`createUsageClient`) に変更。
  - テスト追加:
    - `test/core/parsers.test.js`
    - `test/core/usage-clients.test.js`
    - `test/core/usage-service.test.js`
  - 追加修正:
    - `electron/main.js` のミニマル高さバリデーションで幅閾値を使っていた箇所を修正 (`MINIMAL_FLOOR_H`)。
  - 検証状況:
    - この実行環境では `node` / `npm` コマンドが存在せず、`npm test` 実行は未完了。
  - テスト設定修正:
    - `npm test` が `node_modules_wsl/**/test` を拾って大量失敗していたため、`package.json` の test script を `node --test test` に変更し、プロジェクトテストのみ実行するよう制限。
    - Windows では `node --test test` がディレクトリ解決エラーになるため、`node --test \"test/**/*.test.js\"` へ更新。
  - UI挙動テスト拡充:
    - `public/ui-logic.js` を追加し、UIロジックを純粋関数として切り出し。
    - `public/app.js` で `UiLogic` を利用するよう変更 (伏せ字処理 / 状態遷移 / ポーリング進行計算)。
    - `test/ui/ui-logic.test.js` を追加し、以下を検証:
      - トークン伏せ字表示・マスク値の再送防止
      - 閾値変更時の再分類と最悪ステータス判定
      - 通知/ログの遷移条件 (critical, warning, recovery)
      - 経過率計算とポーリング残秒/色のタイミング判定

## メモ
- `public/app.js` が 1200 行超で、Web fallback と Electron 分岐が複雑化の主要因。
- `src/core` は DI 対応済みになったため、Upstream 実通信なしでロジック検証可能。
- 残件はローカル Node 環境での `npm test` 実行結果確認。
