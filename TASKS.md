# Danmaku 開発タスク

セッションをまたいで開発を進めるためのフェーズ管理。詳細仕様は [SPECS.md](./SPECS.md) を参照。

各フェーズはセッションを分けて取り組む。1 セッションで複数フェーズに踏み込まない。

## 進捗サマリ

| Phase | 内容 | 状態 |
|---|---|---|
| 1 | `danmaku-gui-linux` 最小プロト (X11 透過オーバーレイ実証) | ✅ 完了 |
| 2 | `danmaku-cli` + socket 通信 + 複数行ランダム配置 | ✅ 完了 |
| 3 | 設定ファイル (`~/.config/danmaku/config.toml`) 読み込み | ⏳ 未着手 |
| 4 | `getscreens` (Rust + maim ハイブリッド、JSON 配列出力) | ✅ 完了 |
| 5 | `skills/danmaku/SKILL.md` (Agent Skills 仕様準拠 / ループ指示) + 開発者向け README | 🔄 動作確認まで完了・検証残 |
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

## Phase 4: `getscreens` (Rust + maim ハイブリッド) ✅

**ブランチ:** `app/getscreens`

**ゴール:** スクリーンショットを保存し、ファイルパスを JSON 配列で stdout に返す独立コマンド。弾幕の存在は知らない。

**方式:** ハイブリッド (Rust バイナリ + ネイティブツール委譲)。xcap クレートは docs.rs の最新版ビルド失敗と最終成功版とのバージョン乖離が大きく信頼性に懸念があるため不採用。OS ごとに別実装する方針は GUI 部分と同じ。

- [x] `apps/getscreens/` を Rust で `cargo init`
- [x] CLI 引数仕様（最低限）: `--dir <PATH> --size <PX>`、既定でメインモニターのみ取得
  - `--dir` 未指定時のデフォルト: `$XDG_RUNTIME_DIR/getscreens`（未設定なら `/tmp/getscreens`）
  - `--size` 未指定時は縮小なし（maim 出力をそのまま使う）
- [x] **拡張オプション（将来実装、まずは未実装で出す）**: `--all`（全モニター）、`--screen N`（指定モニター）
- [x] Linux 実装:
  - モニター列挙: `xrandr` 出力をパースしプライマリを特定
  - キャプチャ: `maim` を `std::process::Command` で呼び出し PNG を取得
  - リサイズ: `image` クレート（純 Rust）で長辺 `--size` に縮小。ImageMagick 依存なし。フィルタは Triangle 固定（可変化は未決事項に記載）
- [x] 出力: stdout に JSON 配列 1 行
  - `[{"screen": 0, "path": "...", "timestamp": "20260516-120000"}]`
  - 配列は将来 `--all` で複数要素になる前提（メインのみでも常に配列）
- [x] ファイル名: `YYYYMMDD-HHMMSS.png`（時系列ソート可能）
- [x] 失敗時: stderr にエラー、stdout は空、非ゼロ終了
- [x] 必要パッケージのチェック（`maim` / `xrandr` 不在時は親切なエラー）
- [x] 単独で動くことを確認（弾幕とは独立）

**macOS は Phase 7 と合わせて別途**: `screencapture -x` + モニター列挙手段（未決）で同じ JSON 契約を満たす。

---

## Phase 5: Agent Skill (`skills/danmaku/SKILL.md`) 🔄

**ブランチ名:** `app/danmaku-skill`

**ゴール:** 「今から N 分間スクショを見てコメントを流して」「3 回コメントを流して」のような指示でループが回る状態にする。定期実行 (cron / systemd timer) は使わない。

### 実施した方針転換

着手時の想定との差分:

- **環境構築手順は SKILL.md ではなく `README.md` に分離**。SKILL.md は「ループのレシピ」だけに絞り、ビルド・インストール・GUI 起動は人間向け README が担う。コーダーアプリは起動時に SKILL.md を一覧として常時保持するため、SKILL.md を短く保つメリットが大きい
- **`compatibility` フィールドは未使用**。OS / X11 / GTK4 のような実行環境制約は SKILL.md に書いても LLM の挙動を変えないため。前提の存在自体は本文の「前提チェック」で `which` / `pgrep` により実証
- **`references/` 分割は未実施**。SKILL.md が約 120 行 / 250 ワードに収まり、Anthropic ガイドの「5000 ワード未満」「500 行未満」に余裕で収まったため分割不要
- **コマンド配線は symlink で対応**。`.qwen/skills/danmaku` および `.opencode/skills/danmaku` は `skills/danmaku/` への相対シンボリックリンク。本体を 1 箇所に保ちつつ複数のコーダーアプリから参照させる

### 完了項目

- [x] [Agent Skills 仕様](https://agentskills.io/specification) 準拠（`name`, `description`, frontmatter `---` 区切り、XML angle brackets 不使用）
- [x] SKILL.md 本文の章立て:
  - 用語（ターン数 / コメント数の定義）
  - 前提チェック（`which getscreens` / `which danmaku-cli` / `pgrep -f danmaku-gui-linux`）
  - 利用者指示の解釈（ターン数または継続時間 / 観点 / 間隔）
  - 1 ターンの 4 ステップ（getscreens → 画像読み込み → danmaku-cli → sleep）
  - ターン数分繰り返す手順（ターン K/N の自己宣言を含む）
  - コメントの作り方（キャラクター / 長さ / 個数 / 差分参照 / 機密情報の保護）
  - エラー時の挙動
- [x] 開発者向け `README.md` 作成（GUI の `cargo run`、`getscreens` / `danmaku-cli` の `cargo install --path`、小型モデル向け推奨プロンプト例）
- [x] `danmaku-cli` の成功時メッセージ出力 (`sent N message(s) to screen ...`) を追加し、エージェントが成功/失敗を判別可能に
- [x] opencode + `unsloth/Qwen3.6-35B-A3B-GGUF` で 3 ターンループの動作確認（getscreens / 画像読み込み / コメント送出 / sleep の繰り返しが成立）
- [x] opencode + Gemma 4B 系での簡易動作確認（ループは回るが画像読み込みスキップ等の揺れあり、調整余地は記録済み）
- [x] opencode の model 設定で `modalities.input: ["text", "image"]` を宣言しないと vision attachment が剥がれる挙動を特定し対処手順を残す

### 持ち越し

- [ ] `skills-ref validate ./skills/danmaku` でフロントマターを検証（コマンド導入後）
- [ ] PDF p.30 推奨の triggering テスト: パラフレーズ（言い換え）で発火するか、無関係クエリ（「PDF を要約して」「コードレビューして」等）で誤発火しないか
- [ ] コメント数の上限見直し（現状 1〜5、利用者の体感期待値は 3〜7）
- [ ] Phase 3 完了後、設定ファイルからの上書き経路を SKILL.md に追記

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
- `getscreens` のリサイズフィルタを可変にする（現在 Triangle 固定）。LLM への可読性とループ頻度の体感のトレードオフで利用者が選びたくなる可能性。実装は容易（`image::imageops::FilterType` を CLI フラグまたは設定ファイル経由で受ける）。当面は Triangle 固定で運用し、必要が出てから追加
- macOS の socket パス規約
- macOS でのモニター列挙手段（`system_profiler` 解析か別手段か）
- トレイメニューの項目
- ログ・履歴機能の要否
- AI エージェント側のコンテキスト肥大化対策（毎ターンのスクショ読みでコンテキストが膨らむことへの懸念）。**当面は対策を入れず、まず素のループ動作で問題の出方を観察する**。検討候補: スキル内でのサブエージェント利用、コンテキスト圧縮、セッション管理機能の活用。実際に問題が顕在化してから取り組む
- AI エージェント側のコンテキスト肥大化対策（サブエージェント / コンテキスト圧縮 / セッション管理 — Phase 5 動作検証後に評価）

## 運用ルール

- フェーズ着手時に新ブランチを切る（`app/xxx` または `feature/xxx`）
- 1 セッション = 1 フェーズを目安。スコープを越えそうなら次セッションへ送る
- フェーズ完了時にこのファイルの該当チェックを `[x]` に更新してコミット
- [SPECS.md の絶対ルール](./SPECS.md#開発ルール絶対) を守る:
  1. 依存はコマンドでインストール、最新の安定版
  2. バグ改修は最新の公式情報・ソースを確認した上で行う
