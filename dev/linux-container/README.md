# danmaku-linux 開発コンテナ (macOS 上)

macOS 上の Docker で Ubuntu + XFCE デスクトップを動かし、VNC 越しに `danmaku-linux` の透過オーバーレイ / クリックスルーの挙動を確認するための環境。

```text
Ubuntu container
  Xvnc(:1, 実Xディスプレイ + VNC:5901) → XFCE(startxfce4, xfwm4 内蔵コンポジタ) → 待機
Mac → 画面共有.app で vnc://localhost:5901
```

- `danmaku-linux` は GTK4 + X11 依存。実 X ディスプレイと合成器が要るため、XFCE のデスクトップ環境上で動かす。
- VNC を使うのは macOS 標準の「画面共有」がそのまま VNC クライアントになるため。

## 前提

- Apple Silicon (arm64)。Docker Desktop / colima などの Docker 環境。
- ポート 5901 が空いていること。

## 起動

リポジトリのルートから:

```sh
docker compose -f dev/linux-container/compose.yaml up -d --build
```

`danmaku-dev: VNC ready on :5901` が出れば準備完了。

## 画面を見る (Mac 標準アプリ)

Finder → 「移動」→「サーバへ接続」(⌘K) →

```text
vnc://localhost:5901
```

パスワードは `danmaku`。接続すると XFCE デスクトップが表示される。
弾幕はこの画面に流れる。

## 開発のしかた

ビルドと実行 (`cargo` / `danmaku`) は必ずコンテナ内で動く (Linux/X11 依存のため)。編集とコマンド発行は macOS 側からでも、コンテナ内からでも、どちらでもよい。弾幕を見るのは常に「画面共有」の窓。

`serve` が常駐 (透過オーバーレイ)、`send` がそこへメッセージを送る。コードを変えたら `serve` を再起動して反映する。

### A. macOS 側の端末から操作する

コンテナに入らず、Mac の端末から `docker compose ... exec` で実行する。

```sh
# 常駐を起動 (バックグラウンド)
docker compose -f dev/linux-container/compose.yaml exec -d danmaku-dev \
  cargo run --manifest-path apps/danmaku-linux/Cargo.toml -- serve

# 弾幕を送る
docker compose -f dev/linux-container/compose.yaml exec danmaku-dev \
  cargo run --manifest-path apps/danmaku-linux/Cargo.toml -- send "ハロー" "テスト"

# コードを変えたら serve を止めて起動し直す
docker compose -f dev/linux-container/compose.yaml exec danmaku-dev pkill -x danmaku
```

毎回打つのが長いので、Mac の `~/.zshrc` にエイリアスを置くと短くなる。

```sh
alias dk='docker compose -f /Users/p789/Github/danmaku/dev/linux-container/compose.yaml exec danmaku-dev cargo run --manifest-path apps/danmaku-linux/Cargo.toml --'
# 以後:  dk serve   /   dk send "あ" "い"
```

### B. Linux 側 (コンテナ内) の端末から操作する

コンテナに入って直接叩く。入り方は2通りで、どちらでもよい。

- Mac の端末から `docker compose -f dev/linux-container/compose.yaml exec danmaku-dev bash`
- 画面共有の XFCE デスクトップで、下のドックのターミナルを開く

入った後の操作:

```sh
cd /workspace

# 常駐
cargo run --manifest-path apps/danmaku-linux/Cargo.toml -- serve &

# 弾幕を送る
cargo run --manifest-path apps/danmaku-linux/Cargo.toml -- send "ハロー" "テスト"

# コードを変えたら止めて起動し直す
pkill -x danmaku
```

### `danmaku` コマンドとして使う

PATH に通せば `cargo run ...` の代わりに `danmaku` で呼べる。コンテナ内で:

```sh
cargo install --path apps/danmaku-linux   # ~/.cargo/bin/danmaku を生成
danmaku serve &
danmaku send "ハロー" "テスト"
```

ソースを変えたら `cargo install --path apps/danmaku-linux` をやり直すと `danmaku` が更新される。`cargo install` で入れた `danmaku` はコンテナを作り直すと消えるので、再度 install する。

## 後始末

```sh
docker compose -f dev/linux-container/compose.yaml down
```

`target/` と cargo キャッシュはボリュームに残るのでビルドはやり直しにならない。
ボリュームごと消すなら `down -v`。

## 補足 / 制限

- VNC パスワードは固定値 `danmaku`。localhost publish 前提の開発用。外部公開しないこと。変更するには compose の環境変数 `VNC_PASSWORD` を設定する。
- XFCE は Ubuntu 標準の GNOME とは別のデスクトップ環境。パネルやウィンドウ管理まわりの挙動は GNOME と一致しない。
- `XDG_RUNTIME_DIR` はイメージ内で `/tmp/runtime-root` に設定済み (`danmaku-<screen>.sock` の置き場所)。
