# danmaku-linux 開発時の動作確認

X11 上で透過オーバーレイ (serve) と送信 (send) を最短経路で確認する手順。コマンドはリポジトリのルートディレクトリから実行する想定。

`send` は serve が起動していなければ自動で起動するため、通常は `send` だけ叩けばよい。serve は最後の弾幕から一定時間 (デフォルト 30 分、設定で変更可) でアイドル自動終了する。

## 1. 常駐 (serve) を起動

ターミナル A:

```
cargo run --release --manifest-path apps/danmaku-linux/Cargo.toml
```

引数なしは `serve` と同義。透過オーバーレイが表示され、`danmaku: listening on <socket-path>` が stderr に出れば待受開始。serve だけ単体で起動することもできる (この場合も弾幕が来なければ 30 分でアイドル終了する)。

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

成功時 stdout に `sent 3 message(s) to screen 0` が出て即終了。serve 側の画面に複数本の弾幕が右→左に流れる。serve が動いていなければ自動起動され、立ち上がり次第そのまま流れる。

`--screen` を付けた serve に送る場合は send 側にも同じ番号を渡す:

```
cargo run --release --manifest-path apps/danmaku-linux/Cargo.toml -- send --screen 1 "..."
```

serve 側 screen と不一致なら drop される (stderr に `danmaku: screen mismatch ...`)。

## 3. 常駐検出 / socket 確認

```
pgrep -x danmaku
ls -l "${XDG_RUNTIME_DIR:?}/danmaku-0.sock"
```

socket は screen ごとに `danmaku-<screen>.sock`。

## 設定ファイル

`~/.config/danmaku/config.toml` (無ければ全項目デフォルト)。正常に読めた場合のみ採用し、壊れていれば黙ってデフォルトにフォールバックする。未知のキーは無視、欠けたキーは個別にデフォルト値で補う。

```toml
lanes = 16              # レーン数 (1-128 にクランプ)。デフォルト 16
idle_timeout_min = 30   # 最終弾幕からこの分数でアイドル自動終了。0 で無効。デフォルト 30
debug_background = false # 領域確認用の薄い背景色を表示 (開発用)。デフォルト false
```

設定は serve 起動時に読まれる (手動起動でも send による自動起動でも同じ)。意図的に CLI 引数には出していない。

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
| `danmaku: failed to launch serve: ...` | 自動起動に失敗 (実行ファイルが見つからない等)。`which danmaku` を確認 |
| `danmaku: serve did not become ready within 5s` | 自動起動した serve が時間内に待受開始しなかった。手動で `danmaku serve` を起動して原因を確認 |
| `danmaku: monitor #N not found; aborting` | `--screen N` が範囲外。`xrandr --listmonitors` でインデックス確認 |
| 透過オーバーレイが見えない | ウィンドウマネージャ / コンポジタが X11 か確認 (Wayland セッションは非対応) |

## 5. リリース手順 (ローカルビルド + 手動アップロード)

ビルド済みバイナリを GitHub Releases に上げて配布する。CI は使わず、ローカルでビルドして `gh` で添付する。

前提: 対象コミットが `main` にマージ済みであること。コマンドはリポジトリのルートから実行する。`X.Y.Z` は `apps/danmaku-linux/Cargo.toml` の `version` に合わせる。

```
# 1. main を最新化
git checkout main && git pull --ff-only

# 2. タグを打って push
git tag vX.Y.Z && git push origin vX.Y.Z

# 3. リリースビルド
cargo build --release --manifest-path apps/danmaku-linux/Cargo.toml

# 4. 配布アセット名にコピー (リネーム)
cp apps/danmaku-linux/target/release/danmaku /tmp/danmaku-linux-x86_64

# 5. Release を作成してアセットを添付
gh release create vX.Y.Z /tmp/danmaku-linux-x86_64 --title "vX.Y.Z" --notes "..."
```

確認:

```
gh release view vX.Y.Z --json tagName,assets -q '.tagName, .assets[].name'
```

利用者側のインストール手順は `README.md` を参照。

### 注意

- アセット名は `danmaku-linux-x86_64` 固定。`README.md` の `releases/latest/download/danmaku-linux-x86_64` がこの名前に依存している。変えるなら両方直す。
- **動作下限はビルドした環境に依存する**。ローカルが Ubuntu 24.04 なら glibc 2.39 / GTK 4.14 環境でのビルドになり、バイナリは glibc 2.39 以降でしか動かない。より古い distro まで対応したい場合は、GTK 4.12 以降を積んだ古め base のコンテナでビルドする (このローカル手順とは別途)。
