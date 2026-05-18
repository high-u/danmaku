# danmaku

開発時の実行方法のメモ。コマンドはすべてリポジトリのルートディレクトリで実行する想定。

## GUI を起動する

`danmaku` (serve) は常駐型なので、ターミナルを 1 枚使って起動しっぱなしにしておく。

```
cargo run --release --manifest-path apps/danmaku-linux/Cargo.toml
```

再ビルドのたびに止めて起動し直すのが面倒なので、`cargo install` ではなく `cargo run` で回すのが楽。

## コマンドをインストールする

`getscreens` と `danmaku` は、スキルやシェルから `which` で見つかる必要があるので `~/.cargo/bin` に入れる。

```
cargo install --path apps/getscreens
cargo install --path apps/danmaku-linux
```

検証中で起動を早めたいときはデバッグビルドにする。

```
cargo install --path apps/getscreens --debug
cargo install --path apps/danmaku-linux --debug
```

ソースを更新したら、同じコマンドを再実行すれば上書きされる。

## スキル動作確認時の推奨プロンプト

ローカル LLM の手抜きを抑えたいとき用。短すぎるプロンプト（例: 「弾幕を出して」）で揺れるなら、明示的に書く方が安定する。

```
danmaku スキルを使ってください。
N 回ターンを回して、各ターンで getscreens → 画像 Read → コメント生成 → danmaku send → sleep を bash で実行してください。
途中で「実行する」「待機します」のような宣言で止まらず、次のターンの getscreens まで自分で進めてください。
```

「N」は実際に回したい回数で置き換える。
