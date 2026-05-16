# Danmaku 開発タスク

セッションをまたいで開発を進めるためのフェーズ管理。詳細仕様は [SPECS.md](./SPECS.md) を参照。

各フェーズはセッションを分けて取り組む。1 セッションで複数フェーズに踏み込まない。

## 進捗サマリ

| Phase | 内容 | 状態 |
|---|---|---|
| 1 | `danmaku-gui-linux` 最小プロト (X11 透過オーバーレイ実証) | ✅ 完了 |
| 2 | `danmaku-cli` + socket 通信 + 複数行ランダム配置 | ✅ 完了 |
| 3 | 設定ファイル (`~/.config/danmaku/config.toml`) 読み込み | ⏳ 未着手 |
| 4 | `getscreens` (Rust + maim ハイブリッド、JSON 配列出力) | ⏳ 未着手 |
| 5 | `skills/danmaku/SKILL.md` (Agent Skills 仕様準拠 / インストール手順 / ループ指示) | ⏳ 未着手 |
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

## Phase 4: `getscreens` (Rust + maim ハイブリッド) ⏳

**想定ブランチ名:** `app/getscreens`

**ゴール:** スクリーンショットを保存し、ファイルパスを JSON 配列で stdout に返す独立コマンド。弾幕の存在は知らない。

**方式:** ハイブリッド (Rust バイナリ + ネイティブツール委譲)。xcap クレートは docs.rs の最新版ビルド失敗と最終成功版とのバージョン乖離が大きく信頼性に懸念があるため不採用。OS ごとに別実装する方針は GUI 部分と同じ。

- [ ] `apps/getscreens/` を Rust で `cargo init`
- [ ] CLI 引数仕様（最低限）: `--dir <PATH> --size <PX>`、既定でメインモニターのみ取得
- [ ] **拡張オプション（将来実装、まずは未実装で出す）**: `--all`（全モニター）、`--screen N`（指定モニター）
- [ ] Linux 実装:
  - モニター列挙: `xrandr` 出力をパースしプライマリを特定
  - キャプチャ: `maim` を `std::process::Command` で呼び出し PNG を取得
  - リサイズ: `image` クレート（純 Rust）で長辺 `--size` に縮小。ImageMagick 依存なし
- [ ] 出力: stdout に JSON 配列 1 行
  - `[{"screen": 0, "path": "...", "timestamp": "20260516-120000"}]`
  - 配列は将来 `--all` で複数要素になる前提（メインのみでも常に配列）
- [ ] ファイル名: `YYYYMMDD-HHMMSS.png`（時系列ソート可能）
- [ ] 失敗時: stderr にエラー、stdout は空、非ゼロ終了
- [ ] 必要パッケージのチェック（`maim` / `xrandr` 不在時は親切なエラー）
- [ ] 単独で動くことを確認（弾幕とは独立）

**macOS は Phase 7 と合わせて別途**: `screencapture -x` + モニター列挙手段（未決）で同じ JSON 契約を満たす。

---

## Phase 5: Agent Skill (`skills/danmaku/SKILL.md`) ⏳

**想定ブランチ名:** `skill/danmaku`

**ゴール:** 利用者がスキルをインストールしてエージェントに「環境構築して」と頼めば依存と各アプリのビルドが整い、「今から N 分間、スクショ取得とコメントを繰り返して。danmaku スキルを使うこと」のような指示でループが回る状態にする。定期実行 (cron / systemd timer) は使わない。

- [ ] [Agent Skills 仕様](https://agentskills.io/specification) 準拠を確認
  - `name: danmaku`（親ディレクトリ名と一致）
  - `description` に「何をするか」と「いつ使うか」を具体的キーワード込みで記述
  - `compatibility` に X11 / GTK4 / maim / xrandr 等の要件を明記
  - 本文は 500 行未満、詳細は `references/` 配下に分割
- [ ] `skills/danmaku/SKILL.md` の章立て:
  - 弾幕の用途と前提（ローカル LLM、X11、シェル実行可能エージェント）
  - 各コマンドの使い方（`danmaku-cli` / `getscreens` の JSON 出力の読み方）
  - `danmaku-gui-linux` のビルド・インストール手順
  - **`danmaku-gui-linux` の手動起動手順案内**（自動起動は当面想定しない。動作検証しつつ将来の組み込み是非を判断）
  - `~/.config/danmaku/config.toml` の生成手順
  - AI エージェントへの自律的振る舞い指示:
    - 利用者の自然言語指示（時間・観点）の解釈方法
    - 1 ターンの流れ: `getscreens` → JSON のパスを画像読み込み機能でロード → コメント生成 → `danmaku-cli` 実行
    - 過去スクショ参照、観察観点、コメント生成方針
    - 終了条件の判断
- [ ] `skills-ref validate ./skills/danmaku` でフロントマターを検証
- [ ] 1 ユーザで実際にスキルから環境構築 + ループ実行できることを確認

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
