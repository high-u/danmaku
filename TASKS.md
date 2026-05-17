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
| 7 | `danmaku-gui-linux` レーン上下マージン削除 + `danmaku-cli` を `danmaku-gui-linux send` に統合 | ⏳ 未着手 |
| 8 | `danmaku-gui-macos` (Rust + objc2, macOS 実機、`serve` / `send` 統合形) | ✅ 完了 |
| 9 | `danmaku-gui-macos` マルチスクリーン対応 (`--screen N` の serve 側) | ⏳ 未着手 |

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

## Phase 7: `danmaku-gui-linux` レーンマージン削除 + `danmaku-cli` 統合 ⏳

**想定ブランチ名:** `refactor/linux-cleanup`

**ゴール:** Linux 側を 2 つまとめて整える。

1. レーン y 座標計算からウィンドウ内側の上下 8% マージンを削除
2. `danmaku-cli` を `danmaku-gui-linux send` サブコマンドに統合し、macOS 側 (Phase 8) と同じ `danmaku-gui` 単一バイナリ + `serve` / `send` アーキテクチャに揃える

**背景 (1: マージン):** ウィンドウは既に画面高さの 75%・縦中央配置で、外側に十分な余白がある (これは X11/GTK4 でフルスクリーン扱いされる挙動の回避という Linux 固有の制約に由来)。にもかかわらず現状の `lane_y` (`apps/danmaku-gui-linux/src/main.rs:208-214`) は更に上下 8% を引いた 84% の領域に弾を詰めており、二重マージンになっている。SPECS / 設計議論で一度も出ていない値であり、画面端に流れない不自然さの原因にもなっている。**害悪**として削除する。

**背景 (2: 統合):** Phase 8 着手後の対話で「socket は GUI 内部の常駐 ↔ ephemeral 通信のため不可避であり、独自プロトコルの専用クライアントを別バイナリ化する実利は薄い」と判断し、macOS 側を統合形で実装した。Linux 側は当初 `danmaku-cli` 独立バイナリで実装済み (Phase 2) のため、追従が必要。Linux と macOS は開発機が異なるためフェーズを分ける。

**マージン削除:**

- [ ] `lane_y` を「ウィンドウ高 `h` を `max_lines` で等分し、各レーンに割り当てる」式に書き換える (内側マージンなし)
- [ ] 75% ウィンドウ + 縦中央配置の現状ロジック (`build_ui` 内の `target_h` / `move_to_monitor_center`) は据え置き
- [ ] 実機で目視確認 (画面上端・下端付近にも弾が流れること)

**コマンド統合:**

- [ ] `apps/danmaku-gui-linux` に `clap` でサブコマンド分岐を追加 (引数なし or `serve` → 常駐、`send "..."` → 送信して即終了)
- [ ] `send` サブコマンドの実装を `apps/danmaku-cli/src/main.rs` から移植 (socket 接続 + JSON 書き込み + 即終了。`socket_path` は `$XDG_RUNTIME_DIR/danmaku.sock` のまま)
- [ ] `Cargo.toml` に `[[bin]] name = "danmaku-gui"` を追加してバイナリ名を macOS 側と揃える
- [ ] `apps/danmaku-cli/` ディレクトリ削除
- [ ] 実機で目視確認 (`danmaku-gui send "..."` で弾幕が流れる、エラー時の挙動も `danmaku-cli` 相当)
- [ ] SPECS.md / TASKS.md / Phase 2 の表現を整理 (`danmaku-cli` 言及の置換)

---

## Phase 8: `danmaku-gui-macos` ✅

**ブランチ:** `app/danmaku-gui-macos`

**ゴール:** macOS で Linux 版と同等の透過クリックスルーオーバーレイを実現する。基本は Linux 版からの移植だが、Linux 固有の制約に由来する部分は macOS 側の妥当な手段で置き換える。**実装中に不明点・曖昧点が出たら都度確認する (勝手に決めない)**。

**前提 (確定済み):**

- 言語: **Rust + objc2 / objc2-app-kit / objc2-foundation** (Apple toolchain 非依存、`cargo` でビルド)
  - 初期想定は Swift だったが、Xcode 非導入で swift toolchain (CLT 同梱) の不具合を踏み、また「軽量・標準・安全」の方針と合致しないため Rust に変更
  - `objc2-app-kit` は Apple SDK ヘッダから自動生成された型付き API で、winit / wgpu / Tauri / Servo / Slint 等 Rust 主要 GUI 系が採用しているため、Rust 内の事実上の標準として扱える
  - 詳細経緯は SPECS.md `danmaku-gui-macos` 節を参照
- **`serve` / `send` サブコマンド統合形** で実装する (`danmaku-cli` を別バイナリにしない)。
  - `danmaku-gui` (引数なし or `serve`): 常駐、透過オーバーレイ表示 + socket listener
  - `danmaku-gui send "..."`: socket に書いて即終了
  - socket は内部実装詳細として閉じる
  - 経緯: Phase 8 着手後に「結局 socket は常駐 ↔ ephemeral 通信のため必須であり、独自プロトコルの専用クライアントを別バイナリ化する実利は薄い」と判断したため。SPECS.md `danmaku-gui` の送信モード節と全体構成節を参照
- socket パス: `$TMPDIR/danmaku.sock` (未設定なら `/tmp/danmaku.sock`)
- `--screen N`: `NSScreen.screens` の配列インデックスを採用 (Linux 版が `gdk::Display::monitors()` のインデックスを使うのと同じ思想)
- ウィンドウサイズ: 対象スクリーンの高さ 75%・縦中央 (Linux 版に合わせる。macOS には Linux のような制約はないが「合わせる」)
- レーン配置: ウィンドウ高を `max_lines` で等分。**内側上下マージンは設けない** (Phase 7 と同じ方針)

**Linux 版から移植するパラメータ・挙動:**

- `max_lines = 8` / `base_speed = 250 px/s` / `SPEED_JITTER = 0.3` / `SPAWN_GAP_SEC = 1.5` / `PAYLOAD_STAGGER_MS = 250`
- レーン空き判定 → 空きがあればランダム選択、なければ全レーンからランダムで重ねる (取りこぼし不可視化を避ける)
- 速度ジッタ、ペイロード内 stagger
- 文字: 白塗り + 黒縁取り (Linux 版は `Sans Bold 36` 相当)
- 受信 JSON ペイロードの `color` / `speed` / `size` は **GUI 側で未適用** (Linux 版と同じく Phase 3 / 設定ファイル導入時に対応)
- 画面外まで流れた弾は除去

**実装タスク:**

- [x] `apps/danmaku-gui-macos/` を Rust で `cargo init` (バイナリ)
- [x] 依存追加 (`cargo add objc2 objc2-app-kit objc2-foundation objc2-quartz-core objc2-core-foundation rand clap serde serde_json block2`、最新版)
- [x] 着手手順 #1 透過 NSPanel 最小実装 (画面 75% 中央配置 / 透過 / クリックスルー / 最前面 / 全 Space) — 実機確認済み
- [x] 着手手順 #2 CATextLayer + CABasicAnimation で 1 行スクロール — 実機確認済み
- [x] 着手手順 #3 レーン管理 + 複数弾 spawn 移植 (`max_lines=8` / `base_speed=250 px/s` / `SPEED_JITTER=0.3` / `SPAWN_GAP_SEC=1.5` / `PAYLOAD_STAGGER_MS=250` 相当) — 実機確認済み
- [x] 着手手順 #4 サブコマンド分岐 (`clap`): 引数なし or `serve` → 常駐、`send "..."` → 送信して即終了
- [x] 着手手順 #5 Unix domain socket listener (serve 内): `std::os::unix::net::UnixListener` を別スレッドで動かし、受信ペイロードをメインスレッドに channel + NSTimer (block2) で受け渡す
- [x] 既存 socket ファイルの扱い: Linux 版 `ensure_socket_available` と同じく接続試行 → 失敗なら unlink
- [x] 着手手順 #6 send サブコマンド実装: socket に JSON 1 行書いて即終了。失敗時 stderr + 非ゼロ終了
- [x] 改行区切り JSON 1 行を `serde_json` で `Payload` にデコード
- [x] 完了済み弾の layer cleanup (期限切れ層を NSTimer 内で superlayer から remove)
- [x] バイナリ名を `danmaku-gui` に揃える (`Cargo.toml` の `[[bin]] name = "danmaku-gui"`)
- [x] 実機で目視確認 (socket 経由 = `danmaku-gui send "..."` で弾幕が流れる、send は即終了、エラー時非ゼロ終了)
- [x] `apps/danmaku-gui-macos/IMPLEMENTATION.md` に NSPanel level / collectionBehavior の選定根拠と、Rust + objc2 採用経緯、サブコマンド統合の判断経緯、駆動方式の判断を記録

**Phase 9 (マルチスクリーン) に持ち越した項目:**

- `--screen N` の serve 側対応 (現状 `NSScreen::mainScreen()` ハードコード)
- 受信ペイロードの `screen` フィールドと serve 側 screen の一致チェック (Linux 版相当)

**Phase 6 / 設定ファイル導入時に対応する項目:**

- 受信 JSON の `color` / `speed` / `size` の上書き適用 (現状 dead_code)
- `BACKGROUND_TINT` を `None` に戻す (トレイアイコン等の常駐手がかりが揃ってから)
- 縁取り付き文字 (Linux 版相当の白塗り + 黒縁取り、現状は白塗りのみ)

---

## Phase 9: `danmaku-gui-macos` マルチスクリーン対応 ⏳

**想定ブランチ名:** `app/macos-multiscreen`

**ゴール:** macOS で `--screen N` を serve / send 両モードで対応し、複数モニタ環境で任意のディスプレイに弾幕を出せるようにする。

**背景:** Phase 8 では `NSScreen::mainScreen()` をハードコードしてシングルモニタで動作確認した。Linux 側は Phase 1 から `gdk::Display::monitors()` のインデックスで `--screen N` 対応済み (`danmaku-gui-linux` に `monitor_for_screen` あり)。Linux と macOS は開発機が異なるためフェーズを分ける。

- [ ] serve 側に `--screen N` (デフォルト 0) を追加。`NSScreen::screens(mtm).get(N)` でインデックス選択
- [ ] 不在 screen 番号なら stderr エラー + 非ゼロ終了 (Linux 版 `danmaku-gui-linux:118-121` と同挙動)
- [ ] 受信ペイロードの `screen` フィールドと serve 側 screen の一致チェック、不一致なら drop (Linux 版 `danmaku-gui-linux:336-343` 相当)
- [ ] パネル配置を `NSScreen.frame` ベースで計算 (現状 `screen_frame` を直接使っているが、副ディスプレイ座標系でも正しく動くこと)
- [ ] 実機で目視確認 (複数モニタ環境、各 screen 番号で対象モニタに弾幕が出ること)

---

## 未決事項（必要に応じて道中で確定）

- `getscreens` のクリーンナップ戦略（日付ローテーション / 何もしない / 既存古ファイル削除）
- `getscreens` のリサイズフィルタを可変にする（現在 Triangle 固定）。LLM への可読性とループ頻度の体感のトレードオフで利用者が選びたくなる可能性。実装は容易（`image::imageops::FilterType` を CLI フラグまたは設定ファイル経由で受ける）。当面は Triangle 固定で運用し、必要が出てから追加
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
