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
| 7 | `danmaku-gui-linux` レーンレイアウト修正 + `danmaku-cli` を `danmaku-gui send` に統合 | ✅ 完了 |
| 8 | `danmaku-gui-macos` (Rust + objc2, macOS 実機、`serve` / `send` 統合形) | ✅ 完了 |
| 9 | `danmaku-gui-macos` マルチスクリーン対応 (`--screen N` の serve 側) | ⏳ 未着手 |
| 10 | `danmaku-cli` 削除 + `danmaku-linux` リネーム + バイナリ名 `danmaku` 統一 (Linux 側) + ドキュメント追従 | ✅ 完了 |
| 11 | `danmaku-gui-macos` レーンレイアウト追従 (Phase 7 と同等) | ⏳ 未着手 |
| 12 | `danmaku-linux` help 英語化 + N プロセス・マルチスクリーン + `--lanes` + `max_lines`→`lanes` 改名 | ⏳ 未着手 |

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

実装の整理 (公式準拠 / 一般的アプローチ / ハック的の分類、事実と解釈の区別、Mutter ソース参照箇所) は [apps/danmaku-linux/IMPLEMENTATION.md](./apps/danmaku-linux/IMPLEMENTATION.md) に記録。

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

## Phase 7: `danmaku-gui-linux` レーンレイアウト修正 + `danmaku-cli` 統合 ⏳

**ブランチ:** `refactor/integrate-cli-into-gui-linux`

**ゴール:** Linux 側を 2 つまとめて整える。

1. レーン配置を「内側マージン削除 + フォントサイズをレーン高から相対決定 + テキストをレーン中央に配置」に修正
2. `danmaku-cli` を `danmaku-gui-linux send` サブコマンドに統合し、macOS 側 (Phase 8) と同じ `danmaku-gui` 単一バイナリ + `serve` / `send` アーキテクチャに揃える

**背景 (1: レーンレイアウト):** ウィンドウは既に画面高さの 75%・縦中央配置で、外側に十分な余白がある (これは X11/GTK4 でフルスクリーン扱いされる挙動の回避という Linux 固有の制約に由来)。にもかかわらず現状の `lane_y` (`apps/danmaku-linux/src/main.rs:208-214`) は更に上下 8% を引いた 84% の領域に弾を詰めており、二重マージンになっている。加えてフォントサイズが絶対値 (36px) で固定され、テキスト y はレーン上端基準のため、`max_lines` 等分時に最終レーンの下に大きな空きが生じ上下非対称になっていた。マージン削除だけでは症状が残るため、同根の問題として一括で修正する: ① 内側マージン削除、② フォントサイズをレーン高から相対 (`lane_h * 0.55〜0.65` を実機で詰める)、③ pango の `pixel_size()` で実描画高を取りレーン中央に配置。

**背景 (2: 統合):** Phase 8 着手後の対話で「socket は GUI 内部の常駐 ↔ ephemeral 通信のため不可避であり、独自プロトコルの専用クライアントを別バイナリ化する実利は薄い」と判断し、macOS 側を統合形で実装した。Linux 側は当初 `danmaku-cli` 独立バイナリで実装済み (Phase 2) のため、追従が必要。Linux と macOS は開発機が異なるためフェーズを分ける。

**レーンレイアウト修正:**

- [x] `lane_y` を「ウィンドウ高 `h` を `max_lines` で等分」に書き換える (内側マージンなし)
- [x] フォントサイズを `lane_h * 0.6` で動的決定 (定数 `FONT_LANE_RATIO`)
- [x] テキスト y を ink rect 中央が `lane_center` に一致するよう逆算 (`pango::Layout::pixel_extents()` の ink 値を利用)
- [x] `DEFAULT_MAX_LINES` を 8 → 16 に変更 (賑やかさ向上)
- [x] 75% ウィンドウ + 縦中央配置の現状ロジック (`build_ui` 内の `target_h` / `move_to_monitor_center`) は据え置き
- [x] 実機で目視確認 (上下対称、レーン中央通過)

**コマンド統合:**

- [x] `apps/danmaku-gui-linux` に `clap` でサブコマンド分岐を追加 (引数なし or `serve` → 常駐、`send "..."` → 送信して即終了)
- [x] `send` サブコマンドの実装を `apps/danmaku-cli/src/main.rs` から移植 (socket 接続 + JSON 書き込み + 即終了。`socket_path` は `$XDG_RUNTIME_DIR/danmaku.sock` のまま)
- [x] `Cargo.toml` に `[[bin]] name = "danmaku-gui"` を追加してバイナリ名を macOS 側と揃える
- [x] 実機で目視確認 (`danmaku-gui send "..."` で弾幕が流れる、エラー時の挙動も `danmaku-cli` 相当)
- [x] SPECS.md / TASKS.md の構造的言及を整理 (`danmaku-cli` の役割書き換え。SKILL.md / README.md / `apps/danmaku-cli/` 削除は Phase 10 で対応)

**コミット粒度:** ① レーンレイアウト修正、② コマンド統合 の 2 コミットに分ける。

**拡張案 (本フェーズ対象外、タスク化もしない):** 文字数連動またはランダムで 3 サイズ可変 (例: `lane_h * 3/6, 4/6, 5/6`)。賑やかさ向上が目的で本フェーズのスコープを越える。必要が出てから検討。

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

## Phase 10: `danmaku-cli` 削除 + `danmaku-linux` リネーム + バイナリ名統一 + ドキュメント追従 ✅

**ブランチ:** `refactor/linux-rename-after-cli-merge`

**ゴール:** Phase 7 でコマンド体系が `danmaku send` に統合されたことを受け、Linux 側のディレクトリ・バイナリ名・ドキュメントを最終形に揃える。macOS 側は触らない (Phase 11 で追従)。

**背景:** Phase 7 は実装変更 (サブコマンド統合) とコードベース内の最低限の文言整理に集中した。利用者の体験面 (SKILL.md のレシピ、README の手順)、ディレクトリ構成 (旧 `apps/danmaku-cli/`, `apps/danmaku-gui-linux/`)、バイナリ名の最終形 (`danmaku`) への追従を本フェーズで一括で行う。

- [x] `apps/danmaku-cli/` ディレクトリ削除
- [x] `apps/danmaku-gui-linux/` → `apps/danmaku-linux/` にリネーム (`git mv`)
- [x] `Cargo.toml`: パッケージ名 `danmaku-gui-linux` → `danmaku-linux`、バイナリ名 `danmaku-gui` → `danmaku`
- [x] `src/main.rs`: `clap` の `name` 属性とログプレフィクスを `danmaku` に統一
- [x] `cargo build --release` でビルド成功確認
- [x] `skills/danmaku/SKILL.md` の `danmaku-cli` 言及を `danmaku send` に置換、`pgrep -f danmaku-gui-linux` を `pgrep -x danmaku` に
- [x] `README.md` の手順を `apps/danmaku-linux` / `danmaku` バイナリに更新
- [x] `SPECS.md` の Linux 側および両 OS 共通の表現を `danmaku` 統一に書き換え (macOS 固有節は据え置き)
- [x] `.qwen/skills/danmaku` / `.opencode/skills/danmaku` の symlink は `skills/danmaku` を指したままで影響なし (確認済み)
- [ ] opencode + ローカル LLM で 3 ターンループ再動作確認 (Phase 5 と同条件) — 実機で別途

**Phase 11 に持ち越し:** macOS 側 (`apps/danmaku-gui-macos/`) のディレクトリ名・バイナリ名 (`danmaku-gui` → `danmaku`) 追従。

---

## Phase 11: `danmaku-gui-macos` レーンレイアウト追従 ⏳

**想定ブランチ名:** `refactor/macos-lane-layout`

**ゴール:** Phase 7 で Linux 側に入れた「内側マージン削除 + フォントサイズをレーン高から相対決定 + テキストをレーン中央に配置」を macOS 側にも適用する。

**背景:** Phase 8 で Linux 版のパラメータ (`max_lines=8` / 絶対 font size 等) をそのまま移植したため、macOS 側にも同じ上下非対称・絶対値依存の問題がある。Linux と macOS は開発機が異なるためフェーズを分ける (Phase 7 と Phase 9 の分離方針と同じ)。

- [ ] レーン y 計算を Linux 版と同等の「ウィンドウ高等分 + テキスト中央配置」に変更
- [ ] フォントサイズを `lane_h * k` で動的決定 (Phase 7 で詰めた係数を流用)
- [ ] 実機で目視確認 (上下対称であること)

---

## Phase 12: `danmaku-linux` help 英語化 + N プロセス・マルチスクリーン + `--lanes` + 改名 ⏳

**想定ブランチ名:** `feature/linux-multiscreen-lanes-help`

**ゴール:** `apps/danmaku-linux` の CLI 表面 (help / フラグ / マルチスクリーン挙動 / レーン数指定) を実用形に揃え、内部識別子も「レーン」用語に統一する。

**背景:**

- help が日英混在で、特に `Send` の doc コメント「常駐側の設定を上書き」は実装に存在しない設定の上書きを謳っており嘘になっている (`apps/danmaku-linux/src/main.rs:46-63`)
- `Send` の `--color` / `--speed` / `--size` は CLI 側でパースし JSON で送信するが、GUI 側で未適用 (`Payload` の各フィールドは `#[allow(dead_code)]`)。「送れるが効かない」状態でドキュメントと実装が不整合
- マルチスクリーンは `--screen N` フラグ自体は serve / send 双方にあるが、socket パスが `$XDG_RUNTIME_DIR/danmaku.sock` 固定で **プロセス 1 つ前提**。serve 2 起動目は `ensure_socket_available` で拒否され、複数モニタ同時表示不可
- レーン数は `DEFAULT_MAX_LINES = 16` ハードコード。実機ごとの賑やかさ調整ができない
- `max_lines` / `DEFAULT_MAX_LINES` という識別子は Phase 7 以降の「レーン (lane)」用語と乖離。`lane_y` / `lane_h` 等の周辺識別子は lane に揃っているのに `max_lines` だけ残存

**プロセスモデルの判断:** N プロセス案 (1 serve = 1 画面 = 1 socket) を採用。

- serve / send が共通の `socket_path(screen)` 関数で `danmaku-{screen}.sock` を計算 → ファイルパス自体が暗黙のルーティングテーブルになる (中央レジストリ不要)
- 現状の `DanmakuState` / レーン管理 / window 構築コードを「1 画面前提」のまま保てる
- 「serve = 1 画面の透過オーバーレイ + 1 socket」という責務が SPECS.md の 3 系統疎結合と整合
- 起動側の複雑さ (画面分の `&` 起動) はシェルや systemd user unit で組み立てる領域。バイナリ側に複雑さを寄せない
- 単一画面利用者には完全に透過 (デフォルト `--screen 0` で従来通り)

**実装タスク:**

- [ ] **help 英語化**: `Cli::about` と各サブコマンドの doc コメントを英語に統一
- [ ] **Send の `--color` / `--speed` / `--size` 削除**: `Command::Send` のフィールド、`Payload` の `color` / `speed` / `size` フィールド、`run_send` の構築箇所、`#[allow(dead_code)]` 抑制も同時に削除
- [ ] **N プロセス・マルチスクリーン**:
  - `socket_path()` を `socket_path(screen: u32) -> Result<PathBuf, String>` に変更し `danmaku-{screen}.sock` を返す
  - `run_serve` で起動時 screen から socket パス決定
  - `run_send` で `--screen N` から同じパスを計算して `connect()`
  - `Payload` から `screen` フィールド削除 → `{ messages: [...] }` のみ
  - `process_line` の screen 不一致 drop ロジック削除 (ルーティングは socket パスで完結)
- [ ] **`--lanes` (serve のみ)**:
  - `clap` の `value_parser!(usize).range(1..=128)` で範囲制御 (0 と 129 以上はエラー)
  - デフォルト 16 (現状の `DEFAULT_MAX_LINES` 同値)
  - `DanmakuState::new` に lanes 引数を追加して `lanes` フィールドに反映
- [ ] **`max_lines` → `lanes` 改名**: `DEFAULT_MAX_LINES` → `DEFAULT_LANES`、`DanmakuState::max_lines` → `lanes`、`draw_bullets` / `spawn_messages` / `lane_y` の参照を一括更新
- [ ] **send エラー文言** (英語 + ヒント): `danmaku: failed to connect to <path>: <e>` に続けて `hint: is 'danmaku serve --screen <N>' running?` を出す
- [ ] `cargo build --release` でビルド成功確認
- [ ] `danmaku --help` / `danmaku serve --help` / `danmaku send --help` の出力目視
- [ ] `danmaku serve --lanes 0` / `--lanes 129` で範囲外エラー確認
- [ ] 実機で `--screen 0` / `--screen 1` 並列起動して `send --screen 0` / `--screen 1` の振り分け目視 (マルチモニタ実機がある場合)
- [ ] 未起動 screen への send で `hint:` 付きエラー文言確認

**スコープ外 (今フェーズで触らない):**

- `SKILL.md` / `SPECS.md` / `README.md` のマルチスクリーン仕様反映 — 実装後の運用確認を経て別途
- `danmaku-gui-macos` 側の追従 — Phase 11 / 別フェーズに集約
- `--color` / `--speed` / `--size` の再導入 — 必要性が出てから serve 側既定値 + 設定ファイルとして再設計

**コミット粒度:** ① help 英語化 + Send dead flag 削除、② socket パス screen 化 + Payload screen 削除、③ `--lanes` 導入 + `max_lines`→`lanes` 改名 の 3 コミットを想定。

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
