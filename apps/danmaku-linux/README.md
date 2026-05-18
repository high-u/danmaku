# danmaku-linux 開発時の動作確認

X11 上で透過オーバーレイ (serve) と送信 (send) を最短経路で確認する手順。コマンドはリポジトリのルートディレクトリから実行する想定。

## 1. 常駐 (serve) を起動

ターミナル A:

```
cargo run --release --manifest-path apps/danmaku-linux/Cargo.toml
```

引数なしは `serve` と同義。透過オーバーレイが表示され、`danmaku: listening on <socket-path>` が stderr に出れば待受開始。

別ディスプレイに出したい場合:

```
cargo run --release --manifest-path apps/danmaku-linux/Cargo.toml -- serve --screen 1
```

`--screen` は `gdk::Display::monitors()` のインデックス (0 始まり)。

## 2. 送信 (send) で弾幕を流す

ターミナル B:

```
cargo run --release --manifest-path apps/danmaku-linux/Cargo.toml -- send "ハロー" "テスト" "弾幕"
```

成功時 stdout に `sent 3 message(s) to screen 0` が出て即終了。serve 側の画面に複数本の弾幕が右→左に流れる。

`--screen` を付けた serve に送る場合は send 側にも同じ番号を渡す:

```
cargo run --release --manifest-path apps/danmaku-linux/Cargo.toml -- send --screen 1 "..."
```

serve 側 screen と不一致なら drop される (stderr に `danmaku: screen mismatch ...`)。

## 3. 常駐検出 / socket 確認

```
pgrep -x danmaku
ls -l "${XDG_RUNTIME_DIR:?}/danmaku.sock"
```

スキル側の前提チェックも `pgrep -x danmaku` を使う。

## 4. インストール後の確認

`cargo install` 経由で `~/.cargo/bin/danmaku` を入れたあとに、シェルから素のコマンド名で叩ける状態を確認する:

```
cargo install --path apps/danmaku-linux --debug
which danmaku
danmaku           # serve (Ctrl-C で停止)
danmaku send "確認用コメント"
```

## エラー時の典型パターン

| 症状 | 原因と確認 |
|---|---|
| `danmaku: failed to connect to ...` | serve が動いていない。`pgrep -x danmaku` で確認 |
| `danmaku: monitor #N not found; aborting` | `--screen N` が範囲外。`xrandr --listmonitors` でインデックス確認 |
| `danmaku: screen mismatch (got X, expected Y); dropping` | serve と send で `--screen` がずれている |
| 透過オーバーレイが見えない | ウィンドウマネージャ / コンポジタが X11 か確認 (Wayland セッションは非対応) |
