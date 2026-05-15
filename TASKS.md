# Danmaku 開発タスク

セッションをまたいで開発を進めるためのフェーズ管理。詳細仕様は [SPECS.md](./SPECS.md) を参照。

各フェーズはセッションを分けて取り組む。1 セッションで複数フェーズに踏み込まない。

## 進捗サマリ

| Phase | 内容 | 状態 |
|---|---|---|
| 1 | `danmaku-gui-linux` 最小プロト (X11 透過オーバーレイ実証) | ✅ 完了 |
| 2 | `danmaku-cli` + socket 通信 + 複数行ランダム配置 | ✅ 完了 |
| 3 | 設定ファイル (`~/.config/danmaku/config.toml`) 読み込み | ⏳ 未着手 |
| 4 | `getscreens` シェルスクリプト | ⏳ 未着手 |
| 5 | `skills/danmaku/SKILL.md` (インストール手順 / cron 設定 / 振る舞い指示) | ⏳ 未着手 |
| 6 | トレイアイコン | ⏳ 未着手 |
| 7 | `danmaku-gui-macos` (Swift, macOS 実機) | ⏳ 未着手 |

---

## Phase 1: `danmaku-gui-linux` 最小プロト ✅

**ブランチ:** `app/danmaku-gui-linux`（マージ済み想定）

**ゴール:** X11 で透過 + クリックスルー + 常時最前面 + 文字スクロールが成立することを実証する。最大リスクの検証。

- [x] Cargo プロジェクト初期化（`gtk4` / `gdk4-x11` / `pangocairo` / `x11` 最新安定）
- [x] 透明 fullscreen ウィンドウ（RGBA visual + CSS 背景透過）
- [x] クリックスルー（空 `input_region`）
- [x] 常時最前面（EWMH `_NET_WM_STATE_ABOVE` を ClientMessage で適用）
- [x] 右→左に文字をスクロール（pangocairo で日本語対応）
- [x] 実機で目視確認

実装の整理 (公式準拠 / 一般的アプローチ / ハック的の分類、事実と解釈の区別、Mutter ソース参照箇所) は [apps/danmaku-gui-linux/IMPLEMENTATION.md](./apps/danmaku-gui-linux/IMPLEMENTATION.md) に記録。

---

## Phase 2: `danmaku-cli` + socket 通信 + 複数行ランダム配置 ✅

**ブランチ:** `app/danmaku-cli`

**ゴール:** シェルから `danmaku-cli "..."` を叩けば、`danmaku-gui-linux` 上で複数本がランダムに流れる状態にする。

- [x] `apps/danmaku-cli/` を Rust で `cargo init`
- [x] CLI 引数仕様: `--screen N`（デフォルト 0）、複数文字列を位置引数で
- [x] socket パス: `$XDG_RUNTIME_DIR/danmaku.sock`（Linux）。未設定はエラー終了
- [x] JSON 1 行 (改行区切り) を送って即終了:
      `{"screen": 0, "messages": ["a","b"], "color": "white", "speed": 1.0}`
- [x] 失敗時 stderr + 非ゼロ終了
- [x] `danmaku-gui-linux` 側に Unix socket listener を追加（`gio::SocketListener` を `MainContext::spawn_local`）
- [x] 受信メッセージごとに「弾」を spawn（レーン制、空きレーンランダム選択 / 速度ランダム / 出現タイミングずらし）
- [x] 空きレーン無しの場合は全レーンからランダム選択して重ねる（破棄しない: 取りこぼしの不可視化を避ける）
- [x] 動作確認

**Phase 3 に持ち越した項目:**

- 受信 JSON の `color` / `speed` / `size` は GUI 側で未適用（dead_code）。設定ファイル導入と合わせて適用
- `max_lines` / `base_speed` / フォント等は GUI 側でハードコード暫定値。設定ファイルから読む
- ログは `eprintln!` ベース。レベル分け（log/tracing クレート）は当面不要、運用観察で必要性が出てから検討

---

## Phase 3: 設定ファイル読み込み ⏳

**想定ブランチ名:** `app/config`

**ゴール:** `~/.config/danmaku/config.toml` で見た目・速度・行数上限などを変更できる。

- [ ] TOML スキーマ（`[display]` セクション: `font_size` / `font_family` / `color` / `opacity` / `speed` / `max_lines`、`[socket]` セクション: `path`）
- [ ] `danmaku-gui-linux` 起動時に読み込み（無ければデフォルト）
- [ ] CLI フラグ (`--color`, `--speed`, `--size`) で上書きできるようにする
- [ ] 設定変更時の挙動: 再起動で反映（ホットリロードは不要）

---

## Phase 4: `getscreens` シェルスクリプト ⏳

**想定ブランチ名:** `app/getscreens`

**ゴール:** 指定画面のスクリーンショットを保存する独立コマンド。弾幕の存在は知らない。

- [ ] `apps/getscreens/getscreens` (shebang つきシェルスクリプト)
- [ ] 引数仕様: `--screen N --dir <PATH> --size <PX>`
- [ ] Linux 実装: `scrot` または `maim` + `convert` でリサイズ
- [ ] ファイル名: `YYYYMMDD-HHMMSS.png`（時系列ソート可能）
- [ ] 必要パッケージのチェック（無ければ親切なエラー）
- [ ] 単独で動くことを確認（弾幕とは独立）

---

## Phase 5: Agent Skill (`skills/danmaku/SKILL.md`) ⏳

**想定ブランチ名:** `skill/danmaku`

**ゴール:** 利用者が `gh skill install` してエージェントに「環境構築して」と頼めば、依存インストールから cron 設定までが完了する状態にする。

- [ ] `skills/danmaku/SKILL.md` の章立て:
  - 弾幕の用途と前提（ローカル LLM、X11、シェル実行可能エージェント）
  - 各コマンドの使い方（`danmaku-cli` / `getscreens`）
  - `danmaku-gui-linux` のビルド・インストール手順
  - `~/.config/danmaku/config.toml` の生成手順
  - systemd user service で `danmaku-gui-linux` を自動起動
  - 画面ごとの cron 設定生成手順（`getscreens && AI エージェント CLI` 直列）
  - AI エージェントへの自律的振る舞い指示（過去スクショ参照、観察観点、コメント生成）
- [ ] 1 ユーザで実際にスキルから環境構築できることを確認

---

## Phase 6: トレイアイコン ⏳

**想定ブランチ名:** `feature/tray`

**ゴール:** ユーザが `danmaku-gui-linux` の起動を視覚的に認識でき、停止操作ができる。

- [ ] `ksni` クレートで AppIndicator
- [ ] メニュー項目（要件確定）: 例 `表示 ON/OFF`、`終了`
- [ ] アイコン素材

---

## Phase 7: `danmaku-gui-macos` ⏳

**想定ブランチ名:** `app/danmaku-gui-macos`

**ゴール:** macOS で Linux 版と同等の透過クリックスルーオーバーレイを実現。

- [ ] `apps/danmaku-gui-macos/` を Swift Package で初期化（Xcode 不要、`swift` CLI）
- [ ] `NSPanel` + `NSWindow.level = .screenSaver`
- [ ] socket パス（`$TMPDIR/danmaku.sock` 等、要決定）
- [ ] SwiftUI で弾幕描画
- [ ] 実機で確認

---

## 未決事項（必要に応じて道中で確定）

- `getscreens` のクリーンナップ戦略（日付ローテーション / 何もしない / 既存古ファイル削除）
- cron 行の具体的フォーマット（AI エージェント CLI の起動コマンド表現）
- macOS の socket パス規約
- トレイメニューの項目
- ログ・履歴機能の要否

## 運用ルール

- フェーズ着手時に新ブランチを切る（`app/xxx` または `feature/xxx`）
- 1 セッション = 1 フェーズを目安。スコープを越えそうなら次セッションへ送る
- フェーズ完了時にこのファイルの該当チェックを `[x]` に更新してコミット
- [SPECS.md の絶対ルール](./SPECS.md#開発ルール絶対) を守る:
  1. 依存はコマンドでインストール、最新の安定版
  2. バグ改修は最新の公式情報・ソースを確認した上で行う
