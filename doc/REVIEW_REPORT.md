# AI Quota Monitor — コード・設計・セキュリティ レビューレポート

**レビュー日**: 2026-02-13
**対象バージョン**: v0.0.2 (ブランチ: `feat/move-to-tauri`)
**レビュアー**: Claude Code (自動レビュー)

---

## 目次

1. [プロジェクト概要](#1-プロジェクト概要)
2. [アーキテクチャ評価](#2-アーキテクチャ評価)
3. [セキュリティ監査](#3-セキュリティ監査)
4. [コード品質レビュー](#4-コード品質レビュー)
5. [テストカバレッジ分析](#5-テストカバレッジ分析)
6. [パフォーマンス評価](#6-パフォーマンス評価)
7. [依存関係分析](#7-依存関係分析)
8. [総合評価と推奨事項](#8-総合評価と推奨事項)

---

## 1. プロジェクト概要

| 項目 | 内容 |
|------|------|
| **名前** | AI Quota Monitor |
| **目的** | Claude Code / Codex の API クォータ使用量をリアルタイム監視するデスクトップアプリ |
| **技術スタック** | Tauri v2 + Vanilla JS + Rust |
| **フロントエンド** | HTML/CSS/JS（フレームワーク不使用） |
| **バックエンド** | Rust（Tauri コマンド） |
| **配布形式** | NSIS / MSI インストーラ、ZIP ポータブル |

### ファイル構成

| ファイル | 行数 | 役割 |
|---------|------|------|
| `src-tauri/src/main.rs` | ~1,439 | Tauri コマンド、API 通信、トークン管理、設定永続化 |
| `public/app.js` | ~829 | ポーリング、UI レンダリング、通知制御 |
| `public/ui-logic.js` | ~133 | ステータス分類、ユーティリティ関数 |
| `public/account-ui.js` | ~159 | アカウントフォーム管理 |
| `src/core/parsers.js` | ~180 | Claude/Codex API レスポンスパーサー |
| `src/core/usage-clients.js` | ~65 | HTTP クライアントラッパー |
| `src/core/usage-service.js` | ~50 | サービスアグリゲーター |
| `public/tauri-bridge.js` | ~26 | Tauri IPC ブリッジ |

---

## 2. アーキテクチャ評価

### 2.1 良い点

- **Tauri v2 採用**: Electron 比でメモリ・バイナリサイズ大幅削減。適切な選択
- **トークンの OS Keyring 保管**: プレーンテキスト保存を回避する正しい設計
- **API 通信を Rust 側で実施**: フロントエンドから直接外部 API を叩かず、Tauri コマンド経由で通信。CSP と整合性が取れている
- **DI パターンの採用**: `usage-clients.js` で fetch を注入可能にしており、テスタビリティが高い
- **純粋関数の分離**: `ui-logic.js` は副作用なしの純粋関数で構成され、テスト容易

### 2.2 改善が望まれる点

| 問題 | 重要度 | 詳細 |
|------|--------|------|
| **main.rs が巨大な単一ファイル** | 中 | ~1,439 行を1ファイルに集約。モジュール分割（`commands/`, `models/`, `store/`, `api/`）を推奨 |
| **app.js の責務過多** | 中 | ~829 行に状態管理・ポーリング・レンダリング・通知が混在。分離を推奨 |
| **グローバル状態への依存** | 中 | `app.js` の `state` オブジェクトがモジュール全体から直接変更される。状態変更の追跡が困難 |
| **DOM の全置換レンダリング** | 中 | `render()` が innerHTML で全カードを毎回再構築。仮想 DOM やデルタ更新の導入を検討 |

---

## 3. セキュリティ監査

### 3.1 評価サマリ

| カテゴリ | 評価 | コメント |
|----------|------|---------|
| トークン保管 | **A** | OS Keyring 利用。プレーンテキスト保存なし |
| HTTPS/TLS | **A** | 全 API 通信 HTTPS。`rustls-tls` 使用 |
| CSP ヘッダー | **B** | 制限的な CSP だが `unsafe-inline` あり |
| 入力バリデーション | **B** | 数値クランプ・文字列トリムあり。長さ制限なし |
| Tauri ケーパビリティ | **B** | `core:default` は広範。最小限に絞ることを推奨 |
| エラーハンドリング | **B-** | Rust 側は概ね良好。一部 silent failure あり |
| テストカバレッジ | **D** | Rust テストゼロ。結合テストなし |

### 3.2 重大度別セキュリティ所見

#### HIGH（高）

| # | 所見 | 場所 | 説明 |
|---|------|------|------|
| S-1 | **Bearer トークンのフォーマット未検証** | `main.rs:995` | `HeaderValue::from_str(&format!("Bearer {token}"))` でトークン文字列を検証せず HTTP ヘッダーに挿入。改行文字を含むトークンによるヘッダーインジェクションの可能性 |
| S-2 | **トークンのメモリ残留** | `main.rs:584-602` | Keyring から取得したトークンが `String` として保持され、使用後にゼロクリアされない。メモリダンプでの漏洩リスク |
| S-3 | **API エンドポイントのハードコード** | `main.rs:1004,1020` | DNS ハイジャック時にトークンが漏洩するリスク。証明書ピンニング未実装 |
| S-4 | **HTTP リクエストタイムアウト未設定** | `main.rs` | `reqwest::Client` にタイムアウトが設定されておらず、リクエストが無期限にハングする可能性 |

#### MEDIUM（中）

| # | 所見 | 場所 | 説明 |
|---|------|------|------|
| S-5 | **CSP `style-src 'unsafe-inline'`** | `tauri.conf.json`, `index.html` | インラインスタイルを許可。CSS インジェクションのリスク |
| S-6 | **Tauri ケーパビリティが広範** | `capabilities/default.json` | `core:default` は多数の権限を付与。`core:window:default` 等、必要最小限に制限すべき |
| S-7 | **API レート制限なし** | `main.rs:1220` | `fetch_usage` コマンドに呼び出し頻度制限がなく、上流 API への過剰アクセスの可能性 |
| S-8 | **エラーメッセージによる情報漏洩** | `main.rs:940-954` | 上流 API のエラーレスポンスをサニタイズせずフロントエンドに転送 |
| S-9 | **sessionStorage に生データ保存** | `app.js:198-202` | API レスポンスの生データを `sessionStorage` に JSON シリアライズして保存。暗号化なし |
| S-10 | **入力文字列の長さ制限なし** | `main.rs` 全般 | アカウント名・ID に最大長制限がない。極端に長い文字列で Keyring 操作や JSON 肥大化の問題 |

#### LOW（低）

| # | 所見 | 場所 | 説明 |
|---|------|------|------|
| S-11 | **アカウント ID に `Math.random()` 使用** | `account-ui.js:24` | 暗号学的に安全でない乱数。`crypto.getRandomValues()` を推奨 |
| S-12 | **CSP に `form-action`, `base-uri` 未指定** | `tauri.conf.json` | フォームアクションやベース URI の制限がない |
| S-13 | **Keyring キー名のセパレータ問題** | `main.rs:580-582` | `service:id` の連結で、ID に `:` が含まれるとキー衝突の可能性 |
| S-14 | **`delete_token` のエラー無視** | `main.rs:600` | `let _ = entry.delete_password();` で削除失敗を黙殺 |

### 3.3 CSP 詳細分析

```
default-src 'self';
script-src 'self';
style-src 'self' 'unsafe-inline';    ← 改善推奨
img-src 'self' data:;
connect-src 'self';
font-src 'self' data:;
```

- `connect-src 'self'` はフロントエンドからの直接外部通信をブロック。API 通信は Rust 側で行うため正当な設定
- `style-src 'unsafe-inline'` はダークテーマの CSS 変数に必要だが、外部スタイルシート化で除去可能
- `frame-ancestors`, `form-action`, `base-uri` ディレクティブの追加を推奨

---

## 4. コード品質レビュー

### 4.1 Rust バックエンド (`main.rs`)

#### 良い点
- `serde` による適切な型定義と直列化
- `Option`/`Result` の一貫した使用
- `unsafe` コードブロックなし
- 入力値のクランプ（`clamp_int`, `sanitize_string`）

#### 問題点

| 問題 | 重要度 | 場所 | 説明 |
|------|--------|------|------|
| エラー型が全て `String` | 中 | 全般 | `Result<T, String>` ではなく `thiserror` / `anyhow` による型付きエラーを推奨 |
| ストアの毎回ディスク I/O | 中 | 全般 | コマンド呼び出し毎に `accounts.json` を読み書き。インメモリキャッシュの検討 |
| `token` と `clear_token` の同時指定時の動作 | 低 | L1106-1115 | 両方指定時、セット→クリアの順で処理。排他制御または明示的ドキュメントが必要 |
| ウィンドウ座標の画面範囲検証なし | 低 | L376-377 | 最大 8192px まで許容するが、実画面サイズとの整合性チェックなし |

### 4.2 フロントエンド JavaScript

#### `app.js` — 主要な問題

| 問題 | 重要度 | 場所 | 説明 |
|------|--------|------|------|
| ポーリングの競合状態 | 高 | L608-616 | 初回 `pollAll()` 完了後にタイマー設定。長時間ポーリング時にスケジュールずれ |
| 全 DOM 再構築 | 中 | L366-425 | `render()` が `innerHTML` で全カード再構築。アカウント数増加時のパフォーマンス懸念 |
| マジックナンバー散在 | 中 | L306, 617, 224 | 5%閾値、1000ms間隔、200msタイムアウト等がリテラル値。名前付き定数化を推奨 |
| 初期化失敗時の継続実行 | 中 | L710-712 | Tauri 初期化エラーをログのみで無視し、アプリ実行を継続 |
| ワンタイムエラーログ抑制 | 低 | L140-142 | `didLogPersistError` 等でエラーログを1回限りに制限。持続的エラーが検知不能 |

#### `ui-logic.js` — 高品質

- 純粋関数のみで構成。テスタビリティ最高
- `classifyUtilization` で不正入力に `'ok'` を返す点のみ要検討（`'unknown'` が望ましい）

#### `account-ui.js`

| 問題 | 重要度 | 場所 | 説明 |
|------|--------|------|------|
| トークン入力が `type="text"` | 中 | L87 | `-webkit-text-security` に依存。`type="password"` を推奨 |
| 削除失敗時の DOM 先行更新 | 中 | L94-102 | `deleteAccount` 失敗時も DOM から行を削除してしまう |
| 重複アカウント名の検証なし | 低 | — | 同名アカウントの登録を防止する仕組みがない |

#### `parsers.js`

| 問題 | 重要度 | 場所 | 説明 |
|------|--------|------|------|
| 未知フォーマットの静寂な無視 | 低 | L47 | 未知の API レスポンス構造に対し空ウィンドウを返却。ログ出力を推奨 |
| `limit === 0` 時のゼロ除算 | 低 | L78-79 | チェック済みだがフォールバック値 `0` が妥当か要検討 |

#### `usage-clients.js`

| 問題 | 重要度 | 場所 | 説明 |
|------|--------|------|------|
| ネットワークエラーの未キャッチ | 中 | L29 | fetch のネットワークエラーがこのレイヤーで捕捉されない |
| OAuth ベータバージョン固定 | 低 | L46 | `oauth-2025-04-20` のハードコード。API バージョン廃止時の対応策なし |

---

## 5. テストカバレッジ分析

### 5.1 現状

| 領域 | テストファイル | 行数 | カバレッジ評価 |
|------|---------------|------|---------------|
| パーサー | `test/core/parsers.test.js` | 98 | 良好 |
| HTTP クライアント | `test/core/usage-clients.test.js` | 95 | 良好 |
| サービス層 | `test/core/usage-service.test.js` | 105 | 良好 |
| UI ロジック | `test/ui/ui-logic.test.js` | 171 | 優秀 |
| **Rust バックエンド** | **なし** | **0** | **未テスト** |
| **結合テスト** | **なし** | **0** | **未テスト** |

**テスト対コード比**: JS テスト 469行 / JS ソース ~1,429行 ≒ 33%（JS のみ）
**全体**: テスト 469行 / 全ソース ~2,868行 ≒ 16%

### 5.2 テスト品質

**良い点:**
- Node.js 組み込み `node:test` 使用（外部依存なし）
- `assert/strict` による厳密な比較
- DI パターンによるモック注入
- エッジケース（不正 JSON、欠損フィールド、未サポートフォーマット）のカバー
- トークンマスキングのセキュリティテストあり

**欠如しているテスト:**
- Rust ユニットテスト（`#[cfg(test)]` モジュール）
- Tauri コマンドハンドラのテスト
- Keyring 統合テスト
- ウィンドウ管理機能テスト
- ファイル I/O のテスト
- E2E テスト

---

## 6. パフォーマンス評価

| 問題 | 重要度 | 場所 | 説明 |
|------|--------|------|------|
| **毎秒のリングタイマー更新** | 中 | `app.js:617` | `setInterval(() => { updatePollRing(); updateCountdown(); }, 1000)` がウィンドウ非表示時も動作。`requestAnimationFrame` か Page Visibility API との併用を推奨 |
| **毎回の全 DOM 再構築** | 中 | `app.js:366-425` | アカウント・ウィンドウ数に比例して DOM 操作増加。デルタ更新の導入を推奨 |
| **ストアの毎回ディスク I/O** | 中 | `main.rs` | Tauri コマンド呼び出し毎に JSON ファイル読み書き。`Mutex<Store>` によるインメモリキャッシュ化を推奨 |
| **sessionStorage の全シリアライズ** | 低 | `app.js:198-202` | `state.services` と `state.rawResponses` を毎回 JSON.stringify。選択的保存を推奨 |
| **formatReset の繰り返し呼び出し** | 低 | `app.js:410` | レンダリング毎にロケール日時フォーマット実行。結果キャッシュを推奨 |

---

## 7. 依存関係分析

### 7.1 Rust 依存関係

| パッケージ | バージョン | ステータス | 備考 |
|-----------|-----------|-----------|------|
| `tauri` | 2.x (2.10.2) | 最新安定版 | 問題なし |
| `reqwest` | 0.12 (0.12.28) | 最新安定版 | `rustls-tls` 使用、`default-features = false` で良好 |
| `keyring` | 2.x | 安定版 | OS ネイティブ Keyring 使用 |
| `serde` | 1.x | 安定版 | 問題なし |
| `serde_json` | 1.x | 安定版 | 問題なし |

### 7.2 Node.js 依存関係

| パッケージ | バージョン | ステータス | 備考 |
|-----------|-----------|-----------|------|
| `@tauri-apps/cli` | ^2.0.0 (devDep) | 最新安定版 | ビルド用のみ |

**注目点:**
- 本番依存パッケージ **ゼロ**（JS 側）。攻撃面が極めて小さい
- `rustls-tls` 採用で OpenSSL 依存を排除。セキュリティ面で優良
- 既知の CVE は現時点で確認されず

### 7.3 懸念事項

- Cargo.toml のバージョン指定が広範（`"2"`, `"1"` 等）。`Cargo.lock` で固定されているが、明示的なパッチバージョン指定を検討
- `cargo audit` / `npm audit` を CI に組み込むことを推奨

---

## 8. 総合評価と推奨事項

### 8.1 総合スコア

| カテゴリ | スコア (5段階) | コメント |
|----------|:---:|---------|
| アーキテクチャ設計 | **4** | Tauri 採用、Rust 側 API 通信、Keyring 活用は優秀。モジュール分割が課題 |
| セキュリティ | **3.5** | トークン管理・CSP は良好。ヘッダーインジェクション・メモリ残留が課題 |
| コード品質 | **3.5** | 命名・構造は概ね良好。巨大ファイル・マジックナンバーが課題 |
| テスト | **2** | JS テストは優秀だが、Rust テスト・結合テストの欠如が致命的 |
| パフォーマンス | **3.5** | 通常利用では十分。DOM 全再構築とディスク I/O に改善余地 |
| 依存関係管理 | **4.5** | JS 依存ゼロ、Rust 依存最小限。非常に良好 |

### 8.2 推奨アクション（優先度順）

#### CRITICAL — 本番前に対応必須

1. **Rust ユニットテストの追加** (`main.rs`)
   - Keyring 操作、ファイル I/O、入力バリデーション、アカウント CRUD のテスト
   - 目標カバレッジ: 80%以上

2. **HTTP リクエストタイムアウトの設定**
   ```rust
   let client = reqwest::Client::builder()
       .timeout(std::time::Duration::from_secs(30))
       .build()?;
   ```

3. **Bearer トークンのフォーマット検証**
   - 英数字・ハイフン・アンダースコア・ドット・スラッシュのみ許可
   - 改行・制御文字を含むトークンを拒否

#### HIGH — 早期対応推奨

4. **`zeroize` クレートによるトークンのメモリクリア**
5. **Tauri ケーパビリティの最小化**（`core:default` → 個別権限の列挙）
6. **入力文字列の最大長制限**（256文字等）
7. **トークン入力を `type="password"` に変更** (`account-ui.js`)
8. **ポーリングの競合状態修正** (`app.js` — タイマー設定を初回ポーリング前に移動)

#### MEDIUM — 品質向上のため推奨

9. **`main.rs` のモジュール分割**（`commands/`, `models/`, `store/`, `api/`）
10. **型付きエラーの導入**（`thiserror` クレート）
11. **CSP から `unsafe-inline` 除去**（CSS 外部ファイル化）
12. **API コール頻度制限**（レートリミッター実装）
13. **Page Visibility API との連携**（非表示時のタイマー停止）
14. **`deleteAccount` の DOM 更新を成功時のみに修正**

#### LOW — 余裕があれば対応

15. ESLint 設定の導入
16. GitHub Actions CI/CD パイプライン構築
17. `cargo audit` / `npm audit` の CI 組み込み
18. SECURITY.md の作成
19. アカウント ID 生成に `crypto.getRandomValues()` 使用
20. CSP への `form-action 'none'`, `base-uri 'self'` 追加

---

## 9. 対応進捗メモ

### 2026-02-13 1回目更新

実施済み（中以上）:
- [x] **S-1 Bearer トークンのフォーマット検証**  
  `src-tauri/src/main.rs` にトークン検証を追加。改行/制御文字/非許可文字を拒否。
- [x] **S-4 HTTP リクエストタイムアウト未設定**  
  `reqwest::Client::builder().timeout(30s)` を適用。
- [x] **S-7 API レート制限なし**  
  `fetch_usage` に 500ms のサーバー側レート制限を追加（アカウント単位）。
- [x] **S-8 エラーメッセージ情報漏洩**  
  上流 API エラーの生文言転送を廃止し、HTTP ステータス別の定型メッセージへ変更。
- [x] **S-10 入力文字列の長さ制限なし**  
  アカウント ID / 名前 / トークンに長さと文字種の検証を追加。
- [x] **`account-ui.js` トークン入力 `type="text"`**  
  `type="password"` + `maxlength` を適用。
- [x] **`account-ui.js` 削除失敗時の DOM 先行更新**  
  サーバー削除失敗時は行を削除しない挙動へ変更。
- [x] **`app.js` ポーリング競合状態**  
  `pollInFlight` 制御を導入し、重複ポーリング実行を防止。
- [x] **`app.js` 初期化失敗時の継続実行**  
  初期化失敗時は UI を失敗状態にして処理を中断。
- [x] **`app.js` 毎秒タイマー更新（非表示時含む）**  
  `visibilitychange` と連携し、非表示時はリング更新タイマー停止。
- [x] **S-9 sessionStorage への生レスポンス保存**  
  `rawResponses` の sessionStorage 永続化を廃止。
- [x] **`usage-clients.js` ネットワークエラー未キャッチ**  
  fetch/text 読み出し失敗を明示的に補足し、文脈付きエラーに変換。

未対応（中以上、継続 / 1回目時点）:
- [ ] **S-3 API エンドポイントのハードニング（証明書ピンニング等）**
- [ ] **S-5 CSP `unsafe-inline` 除去**
- [ ] **S-6 Tauri ケーパビリティ最小化**
- [ ] **`main.rs` のモジュール分割 / 型付きエラー化 / I/O キャッシュ化**

### 2026-02-13 2回目更新

追加対応:
- [x] **S-2 トークンのメモリ残留**  
  `zeroize` を導入し、`fetch_usage` / `save_account` のトークン一時文字列と Keyring 取得トークンを使用後にゼロ化。
- [x] **S-14 `delete_token` のエラー無視（低）**  
  削除失敗を握りつぶさず、`NoEntry` 以外はエラーを返すよう修正（副次対応）。
- [x] **テスト拡張（JS/Rust）**  
  `usage-clients` / `usage-service` のテスト更新、および `main.rs` にバリデーション・レート制限のユニットテストを追加。

検証状況:
- [ ] **自動テスト実行**  
  この実行環境には `npm` と `cargo` が存在せず、`npm test` / `cargo test` は実行不可（`command not found`）。

### 2026-02-13 3回目更新

追加対応:
- [x] **S-5 CSP `style-src 'unsafe-inline'`**  
  `public/index.html` のインライン `<style>` を `public/styles.css` へ分離し、CSP を `style-src 'self'` に変更。  
  あわせて HTML/JS 内のインライン `style` 属性を除去（バー幅は CSS ユーティリティクラス化）。
- [x] **CSP 強化（低の副次対応）**  
  `form-action 'none'`, `base-uri 'self'`, `frame-ancestors 'none'` を `index.html` と `tauri.conf.json` に追加。
- [x] **S-3 API エンドポイントハードニング（部分対応）**  
  Rust/JS 双方で上流 URL の `https` と allowlist ホスト検証を追加。  
  さらに Rust 側 HTTP クライアントでリダイレクト追従を無効化し、トークンの他ホスト送出を抑止。

S-3 の残課題:
- [ ] 証明書ピンニング（採用しない方針）

### 2026-02-13 4回目更新

方針決定:
- [x] **S-3 証明書ピンニングは採用しない**  
  運用・更新コストと誤検知リスクを踏まえ、URL allowlist + HTTPS 強制 + リダイレクト無効化の現行対策を維持。

追加対応:
- [x] **S-6 Tauri ケーパビリティ最小化**  
  `src-tauri/capabilities/default.json` の `core:default` を廃止し、`core:app:default` / `core:event:default` / `core:webview:default` / `core:window:default` に分割。
- [x] **ストアの毎回ディスク I/O（中）**  
  `src-tauri/src/main.rs` に `STORE_CACHE` を導入。`read_store` でメモリキャッシュ優先、`write_store` でファイル書き込み後にキャッシュ同期。
- [x] **`main.rs` 分割着手（中）**  
  バリデーション/レート制限を `src-tauri/src/validation.rs` へ、利用率パーサを `src-tauri/src/usage_parser.rs` へ移動。

未対応（中以上、継続）:
- [ ] **`main.rs` のモジュール分割 / 型付きエラー化**

### 2026-02-13 5回目更新

追加対応:
- [x] **`main.rs` 分割を追加で実施（API 層）**  
  上流通信ロジックを `src-tauri/src/api_client.rs` に移動（`fetch_normalized_usage` / ヘッダー構築 / ステータスエラー整形）。
- [x] **型付きエラー化を部分導入**  
  `src-tauri/src/api_client.rs` に `ApiError`（`thiserror`）を導入し、API 層内部は `Result<_, ApiError>` で処理。  
  Tauri コマンド境界で `String` に変換して既存フロント互換を維持。

未対応（中以上、継続）:
- [ ] **`main.rs` のモジュール分割（store/window/commands の更なる分離）**
- [ ] **型付きエラーの全体展開（store/window/command 入出力まで）**

### 2026-02-13 6回目更新

追加対応:
- [x] **`main.rs` 分割を追加で実施（window 層）**  
  ウィンドウ操作ロジック（境界取得/サイズ/位置/モード適用）を `src-tauri/src/window_ops.rs` に移動。
- [x] **`main.rs` をさらに縮小**  
  `main.rs` を約 1278 行 → 約 1068 行まで削減（API + window + validation + parser の分離）。

未対応（中以上、継続）:
- [ ] **`main.rs` のモジュール分割（store/commands の更なる分離）**
- [ ] **型付きエラーの全体展開（store/window/command 入出力まで）**

### 2026-02-13 7回目更新

追加対応:
- [x] **`main.rs` 分割を追加で実施（token/keyring 層）**  
  サービス検証と Keyring 操作を `src-tauri/src/token_store.rs` に移動。
- [x] **`main.rs` をさらに縮小**  
  `main.rs` を約 1068 行 → 約 1036 行まで削減。

未対応（中以上、継続）:
- [ ] **`main.rs` のモジュール分割（store/commands の更なる分離）**
- [ ] **型付きエラーの全体展開（store/window/command 入出力まで）**

### 2026-02-13 8回目更新

追加対応:
- [x] **`main.rs` 分割を追加で実施（store 層）**  
  ストア正規化・永続化・キャッシュを `src-tauri/src/store_repo.rs` に移動。
- [x] **`window_ops` の依存先を分離後構成へ調整**  
  既定サイズ/境界サニタイズの参照を `store_repo` 経由へ統一。
- [x] **`main.rs` をさらに縮小**  
  `main.rs` を約 1036 行 → 約 725 行まで削減。

未対応（中以上、継続）:
- [ ] **`main.rs` のモジュール分割（commands の更なる分離）**
- [ ] **型付きエラーの全体展開（store/window/command 入出力まで）**

### 2026-02-13 9回目更新

追加対応:
- [x] **`main.rs` 分割を追加で実施（commands 層その1）**  
  アカウント系コマンドを `src-tauri/src/account_commands.rs`、設定系コマンドを `src-tauri/src/settings_commands.rs` へ移動。
- [x] **`main.rs` をさらに縮小**  
  `main.rs` を約 725 行 → 約 589 行まで削減。

未対応（中以上、継続）:
- [ ] **`main.rs` のモジュール分割（window/settings 以外の command 整理・最終整理）**
- [ ] **型付きエラーの全体展開（store/window/command 入出力まで）**

### 2026-02-13 10回目更新

追加対応:
- [x] **`main.rs` 分割を追加で実施（commands 層その2）**  
  ウィンドウ系コマンドを `src-tauri/src/window_commands.rs`、利用率取得コマンドを `src-tauri/src/usage_commands.rs` へ移動。
- [x] **`main.rs` をさらに縮小**  
  `main.rs` を約 589 行 → 約 427 行まで削減。

未対応（中以上、継続）:
- [ ] **`main.rs` のモジュール分割（最終整理: コマンド集約構成の整理）**
- [ ] **型付きエラーの全体展開（store/window/command 入出力まで）**

### 2026-02-13 11回目更新

追加対応:
- [x] **`main.rs` 分割の最終整理（コマンド集約）**  
  `tauri::command` ラッパーを `src-tauri/src/commands.rs` に集約し、`main.rs` は起動処理と型定義中心へ整理。  
  `main.rs` を約 427 行 → 約 347 行まで削減。
- [x] **型付きエラーの全体展開（内部層）**  
  `src-tauri/src/error.rs` を追加し、`AppError` / `AppResult` を導入。  
  `store_repo` / `token_store` / `validation` / `window_ops` / 各 command 実装を `Result<_, String>` から `AppResult<_>` へ移行。  
  Tauri コマンド境界（`commands.rs`）でのみ `String` へ変換してフロント互換を維持。
- [x] **`cargo test` でのビルドエラー修正**  
  `src-tauri/src/store_repo.rs` に `tauri::Manager` import を追加し、`AppHandle::path()` 解決エラー（E0599）へ対応。

継続事項:
- [x] **今回の継続対象（`main.rs` 分割最終整理 / 型付きエラー全体展開）は完了**

### 2026-02-13 12回目更新

追加対応:
- [x] **`app.js` のマジックナンバー整理（中）**  
  `public/app.js` の主要リテラル値（ポーリング間隔下限/既定値、履歴上限、ログ表示上限、通知しきい値、ミニマルウィンドウ寸法上限、ドラッグ境界など）を定数化。  
  挙動は維持しつつ、チューニング箇所の一元化と可読性を改善。

---

*以上*
