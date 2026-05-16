# danmaku

開発時の実行方法のメモ。コマンドはすべてリポジトリのルートディレクトリで実行する想定。

## GUI を起動する

`danmaku-gui-linux` は常駐型なので、ターミナルを 1 枚使って起動しっぱなしにしておく。

```
cargo run --release --manifest-path apps/danmaku-gui-linux/Cargo.toml
```

再ビルドのたびに止めて起動し直すのが面倒なので、`cargo install` ではなく `cargo run` で回すのが楽。

## コマンドをインストールする

`getscreens` と `danmaku-cli` は、スキルやシェルから `which` で見つかる必要があるので `~/.cargo/bin` に入れる。

```
cargo install --path apps/getscreens
cargo install --path apps/danmaku-cli
```

検証中で起動を早めたいときはデバッグビルドにする。

```
cargo install --path apps/getscreens --debug
cargo install --path apps/danmaku-cli --debug
```

ソースを更新したら、同じコマンドを再実行すれば上書きされる。
