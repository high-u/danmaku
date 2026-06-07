# danmaku (macOS)

画面に透過オーバーレイを重ね、ニコニコ動画風の弾幕コメントを右から左へ流す常駐アプリ。`danmaku send "コメント"` を叩くだけで画面に弾幕が流れる。

## 前提条件

実行する環境が満たすべき条件:

- **macOS**
- ビルドする場合は **Rust ツールチェイン**（`rustup` 経由の stable）と Xcode Command Line Tools（`xcode-select --install`）

## インストール

現状 macOS 版のビルド済みバイナリ配布は無いため、ソースからビルドする。

```
git clone https://github.com/high-u/danmaku.git
cd danmaku/apps/danmaku-gui-macos
cargo build --release
```

成果物は `target/release/danmaku`。`PATH` の通った場所に置く:

```
mkdir -p ~/.local/bin && cp target/release/danmaku ~/.local/bin/
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
- 常駐プロセスは macOS の補助プロセス (`.accessory`) として動くため Dock には出ない。

特定の画面を指定する場合は `--screen`（`0` 始まり）を付ける:

```
danmaku send --screen 1 "別の画面に出す"
```

> **注意（現状の制約）**: `--screen` は内部のソケット分離・常駐プロセスの起動までは反映されるが、
> 実際の描画先モニタの切り替えは未実装で、描画は常にメインディスプレイに出る。
> マルチモニタへの実配置はマルチモニタ環境での検証を伴って今後対応する。

## 設定（任意）

挙動は `~/.config/danmaku/config.toml` で変更できる（無ければすべてデフォルト）。

```toml
lanes = 16              # 弾幕のレーン数 (1-128)。デフォルト 16
idle_timeout_min = 30   # 最終弾幕からこの分数で自動終了。0 で無効。デフォルト 30
debug_background = false # 表示領域確認用の薄い背景 (開発用)。デフォルト false
```

設定は serve の起動時に読まれる。

## 設計の詳細

実装上の設計判断（透過 NSPanel / Core Animation 描画 / スレッド境界など）は [IMPLEMENTATION.md](IMPLEMENTATION.md) を参照。
