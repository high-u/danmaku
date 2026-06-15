# danmaku (Linux/X11)

画面に透過オーバーレイを重ね、ニコニコ動画風の弾幕コメントを右から左へ流す常駐アプリ。`danmaku send "コメント"` を叩くだけで画面に弾幕が流れる。

## 前提条件

実行する環境が満たすべき条件:

- **Linux / x86_64**
- **X11 セッション**（Wayland セッションは非対応）
- **GTK 4.12 以降**のランタイム（多くのデスクトップ環境に同梱。`cairo` / `pango` / `glib` も GTK に付随）

## インストール

[Releases](https://github.com/high-u/danmaku/releases/latest) からビルド済みバイナリを取得して `PATH` の通った場所に置く:

```
curl -L -o danmaku https://github.com/high-u/danmaku/releases/latest/download/danmaku-linux-x11-x86_64
chmod +x danmaku
mkdir -p ~/.local/bin && mv danmaku ~/.local/bin/
```

`~/.local/bin` が `PATH` に含まれていること。確認:

```
which danmaku
```

## 実行例

```
danmaku send "ハロー" "テスト" "弾幕"
```

- 成功すると `sent 3 message(s) to screen 0` が出て即終了し、画面に弾幕が流れる。
- オーバーレイ本体 (serve) は **未起動なら自動で立ち上がる**。手動起動は不要。
- serve は最後の弾幕から **30 分**でアイドル自動終了する。

複数ディスプレイのうち特定の画面に出す場合は `--screen`（`0` 始まり）を付ける:

```
danmaku send --screen 1 "別の画面に出す"
```

画面インデックスは `xrandr --listmonitors` で確認できる。

## 設定（任意）

挙動は `~/.config/danmaku/config.toml` で変更できる（無ければすべてデフォルト）。

```toml
lanes = 16              # 弾幕のレーン数 (1-128)。デフォルト 16
idle_timeout_min = 30   # 最終弾幕からこの分数で自動終了。0 で無効。デフォルト 30
debug_background = false # 表示領域確認用の薄い背景 (開発用)。デフォルト false
```

設定は serve の起動時に読まれる。

## 開発・動作確認

ソースからの実行確認手順は [DEV.md](DEV.md) を参照。
