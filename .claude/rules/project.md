# authsock-filter プロジェクトルール

## コミット・プッシュ前のチェック

gitコミットやプッシュを行う前に、以下を必ず実行すること：

```bash
cargo fmt
cargo clippy
cargo test
```

これらがすべてパスすることを確認してからコミット・プッシュを行う。
