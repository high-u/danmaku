# danmaku-macos 開発時の動作確認

macOS 上で透過オーバーレイ (serve) と送信 (send) を最短経路で確認する手順。コマンドはリポジトリのルートディレクトリから実行する想定。

`send` は serve が起動していなければ自動で起動するため、通常は `send` だけ叩けばよい (setsid 自己再起動)。serve は最後の弾幕から一定時間 (デフォルト 30 分、設定で変更可) でアイドル自動終了する。常駐プロセスは `.accessory` として動くため Dock には出ない。

## 1. 送信 (send) で弾幕を流す

```
cargo run --release --manifest-path apps/macos/Cargo.toml -- send "ハロー" "テスト" "弾幕"
```

成功時 stdout に `sent 3 message(s) to screen 0` が出て即終了。serve が動いていなければ自動起動され、立ち上がり次第そのまま流れる。serve を直接起動しての確認はしない (実利用は send 経由)。

`--screen` を付ける場合は send 側に番号を渡す (0 始まり):

```
cargo run --release --manifest-path apps/macos/Cargo.toml -- send --screen 1 "..."
```

> **フェーズ3b 保留**: `--screen` は socket 分離・serve 自動起動までは反映されるが、描画先モニタの切り替えは未実装で、描画は常にメインディスプレイに出る。

## 2. 常駐検出 / socket 確認

```
pgrep -x danmaku
ls -l "${TMPDIR%/}/danmaku-0.sock"
```

socket は screen ごとに `$TMPDIR/danmaku-<screen>.sock`。

## 設定ファイル

`~/.config/danmaku/config.toml` (無ければ全項目デフォルト)。Linux 版と同一。

```toml
lanes = 16              # レーン数 (1-128 にクランプ)。デフォルト 16
idle_timeout_min = 30   # 最終弾幕からこの分数でアイドル自動終了。0 で無効。デフォルト 30
debug_background = false # 領域確認用の薄い背景色を表示 (開発用)。デフォルト false
```

設定は serve 起動時に読まれる (send による自動起動でも同じ)。

## 3. インストール後の確認

```
cargo build --release --manifest-path apps/macos/Cargo.toml
cp apps/macos/target/release/danmaku ~/.local/bin/
which danmaku
danmaku send "確認用コメント"
```

## 4. リリース手順 (ローカルビルド + 手動アップロード)

ビルド済みバイナリを GitHub Releases に上げて配布する。CI は使わず、ローカルでビルドして `gh` で添付する。**1 つのタグに両 OS のバイナリを同梱する**方針。

前提: 対象コミットが `main` にマージ済みであること。コマンドはリポジトリのルートから実行する。`X.Y.Z` は各 `Cargo.toml` の `version` に合わせる (リポジトリ全体で揃える)。

```
# 1. main を最新化
git checkout main && git pull --ff-only

# 2. タグを打って push
git tag vX.Y.Z && git push origin vX.Y.Z

# 3. macOS バイナリをリリースビルド (Mac ネイティブ)
cargo build --release --manifest-path apps/macos/Cargo.toml

# 4. 配布アセット名にコピー (リネーム)
cp apps/macos/target/release/danmaku /tmp/danmaku-macos-aarch64

# 5. Linux バイナリを用意する
#    Linux のコードに変更が無い場合は、既存リリースのアセットを流用してよい
#    (バイナリは自身のバージョンを名乗らないため、版ズレは観測されない)。
curl -L -o /tmp/danmaku-linux-x11-x86_64 \
  https://github.com/high-u/danmaku/releases/latest/download/danmaku-linux-x11-x86_64
#    Linux のコードを変更した場合は Linux 環境 (実機 or Mac 上の Linux コンテナ) で
#    再ビルドする。詳細は apps/linux-x11/DEV.md を参照。

# 6. Release を作成して両 OS のアセットを添付
gh release create vX.Y.Z /tmp/danmaku-macos-aarch64 /tmp/danmaku-linux-x11-x86_64 \
  --title "vX.Y.Z" --notes "..."
```

確認:

```
gh release view vX.Y.Z --json tagName,assets -q '.tagName, .assets[].name'
```

利用者側のインストール手順は `README.md` を参照。

### 注意

- アセット名は `danmaku-macos-aarch64` 固定。`README.md` の `releases/latest/download/danmaku-macos-aarch64` がこの名前に依存している。変えるなら両方直す。
- バイナリは未署名。`curl` 取得ならば Gatekeeper の隔離属性が付かずそのまま動く。
- **macOS バイナリは macOS でしかビルドできない** (AppKit 等の Apple フレームワークにリンクするため)。逆に Linux バイナリは Linux/コンテナでビルドする。クロスビルドは現状運用していない。
- 現状バイナリは自身のバージョンを名乗らない (`--version` 無し)。版のトレーサビリティが必要になったら `#[command(version)]` 導入を検討するが、その場合はリリースごとに全 OS 再ビルドが必要になる。
