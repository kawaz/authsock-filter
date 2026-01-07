# authsock-filter プロジェクト固有のレビュー観点

このファイルは `/thorough-review` 実行時に各レビュワーが参照するプロジェクト固有の観点です。

## プロジェクト概要

SSH Agent のプロキシとして動作し、フィルタリング機能を提供するCLIツール（Rust製）。

## セキュリティレビュー固有観点

### SSH Agent プロトコル
- `src/protocol/` でのパース処理
- バッファオーバーフロー、整数オーバーフロー、不正なメッセージ処理
- 上流SSH Agentからのレスポンス検証（信頼境界）

### Unixソケット
- `src/agent/server.rs` でのソケット作成
- パーミッション設定（他ユーザーからのアクセス防止）
- シンボリックリンク攻撃

### パス・環境変数
- `shellexpand` 使用箇所でのインジェクション
- パストラバーサル
- `src/utils/path.rs`

### 設定ファイル
- `src/config/mod.rs`, `file.rs` でのTOMLパース

## アーキテクチャレビュー固有観点

### 主要モジュール
- `src/agent/` - プロキシ実装（server, proxy, upstream）
- `src/filter/` - フィルター評価
- `src/protocol/` - SSH Agentプロトコル
- `src/cli/` - CLI処理
- `src/config/` - 設定管理

### CLI/Config整合性
- CLI引数 → Config → CLI の往復変換で情報が失われないか
- `src/cli/args.rs`, `src/config/mod.rs`
- フィルター形式: `Vec<Vec<String>>`（外側=OR、内側=AND）
- 同一パスの `--socket` マージロジック

## エラーハンドリング固有観点

### inode監視
- `src/cli/commands/run.rs` での監視処理の堅牢性

### 接続管理
- 上流SSH Agentへの接続タイムアウト
- `src/agent/upstream.rs`

## UXレビュー固有観点

### 初心者向け
- SSH Agentの仕組みを知らないユーザー向けの説明
- フィルターの「AND/OR」概念
- 「upstream」用語の説明

### エキスパート向け
- launchd/systemd統合手順
- 設定ファイルの移植性

## テストカバレッジ固有観点

### 重点テスト対象
- プロトコルパース（不正なメッセージ）
- フィルター評価ロジック
- inode監視機能
- 上流Agent切断時の動作

## 主要ファイル一覧

```
src/
├── lib.rs              # モジュール構成
├── error.rs            # エラー型
├── agent/
│   ├── server.rs       # ソケットサーバー
│   ├── proxy.rs        # プロキシ処理
│   └── upstream.rs     # 上流Agent接続
├── protocol/
│   ├── mod.rs
│   ├── message.rs      # メッセージ型
│   └── codec.rs        # エンコード/デコード
├── filter/
│   ├── mod.rs          # フィルター評価
│   ├── github.rs       # GitHub API連携
│   └── keyfile.rs      # キーファイル
├── config/
│   ├── mod.rs
│   └── file.rs         # 設定ファイル
├── cli/
│   ├── mod.rs
│   ├── args.rs         # CLI引数定義
│   └── commands/
│       └── run.rs      # メイン実行（inode監視含む）
└── utils/
    └── path.rs         # パス処理
```
