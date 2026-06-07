# macOS 版を Linux 版に合わせる改修計画

`apps/danmaku-gui-macos` を `apps/danmaku-linux` の概念・機能に揃えるための改修計画。

## 方針

- ゴール: macOS 版 (`danmaku-gui-macos`) を Linux 版 (`danmaku-linux`) の概念・機能・コマンド体系に合わせる。
- **原則: 機能・概念は Linux に合わせるが、OS がアプリをどう扱うか（プロセス常駐方式・Dock/タスクバー可視性・アクティベーションポリシー・ウィンドウ管理など）は各 OS の思想に従う。** Dock に出さないのも「Linux に合わせるため」ではなく「macOS でバックグラウンド常駐補助はエージェントにするのが正しいから」という理由で行う（結果が一致するだけ）。
- 描画方式 (Core Animation の宣言的アニメーション vs Linux の手動 tick) は **揃えない**。これは上記原則の一例で、OS のお作法に沿った内部実装の差であり、振る舞い (速度ジッタ=duration、stagger=beginTime) は既に等価。IMPLEMENTATION.md 0.3 の判断を踏襲する。
- 以下の確定事項は対話で決定済み。

### 確定した決定事項

| 項目 | 決定 |
|------|------|
| バイナリ/コマンド名 | `danmaku-gui` → `danmaku` に統一 |
| 設定ファイルパス | Linux と同じ `XDG_CONFIG_HOME` → `~/.config/danmaku/config.toml` |
| send 独自オプション (`--color`/`--speed`/`--size`) | **削除**（Linux に無く、かつ serve 側で実際に使われていないデッドオプション） |
| マルチモニタ (`--screen`) | **今回実装**（Linux は対応済みのため合わせる） |
| アイドル自動終了 | **追加**（Linux と同じ） |
| send の serve 自動起動 | **追加**（Linux と同じ） |

---

## 差分一覧（macOS 現状 → Linux に合わせた後）

### 1. コマンド/バイナリ名の統一

- 現状: バイナリ名 `danmaku-gui`、clap `name = "danmaku-gui"`、全ログプレフィックス `danmaku-gui:`。
- 変更:
  - `Cargo.toml` に `[[bin]] name = "danmaku"` を追加（ディレクトリ名 `danmaku-gui-macos` は OS 識別のため維持。IMPLEMENTATION.md 2 節の方針どおり、ソース管理上の名前と成果物名を分離）。
  - clap `#[command(name = "danmaku", ...)]` に変更。about も Linux に倣い `"Transparent danmaku overlay for macOS."` 等へ。
  - `eprintln!`/`println!` のプレフィックスを `danmaku:` に統一。

### 2. 設定ファイル対応（新規）

Linux の `Config` 機構を移植する。

- 追加する構造体・関数（Linux `main.rs` 66-105 行と同等）:
  - `Config { lanes: u32, idle_timeout_min: u64, debug_background: bool }` + `Default` 実装。
  - `config_path()` → `XDG_CONFIG_HOME` があればそれ、無ければ `$HOME/.config` を見て `danmaku/config.toml` を返す。
  - `load_config()` → 読めない/壊れている場合は全項目デフォルト。
- 追加する定数（Linux に合わせる）:
  - `DEFAULT_LANES = 16`、`DEFAULT_IDLE_TIMEOUT_MIN = 30`、`MIN_LANES = 1`、`MAX_LANES = 128`。
- 依存追加: `Cargo.toml` に `toml`（Linux と同バージョン）を追加。
- 設定は `run_serve` 起動時に読み込む。

### 3. レーン数を設定可能化

- 現状: `MAX_LINES: usize = 8` ハードコード。`DanmakuState::new()` と `lane_y` がこれに依存。
- 変更:
  - `MAX_LINES` 定数を廃止し、起動時に `config.lanes.clamp(MIN_LANES, MAX_LANES)` で確定した `lanes: usize` を持ち回る。
  - `DanmakuState` に `lanes: usize` フィールドを追加（`last_spawn_at` の長さもこれに連動）。Linux の `DanmakuState` 124 行と同様。
  - `lane_y(lane, height, lanes)` に lanes を引数で渡す形へ変更。
  - `spawn_messages` のレーン探索 `0..MAX_LINES` を `0..state.lanes` に変更。
  - **フォントをレーン高さ連動にする**（考え方を Linux に揃える。完全同値である必要はない）: 固定 `FONT_SIZE = 48.0` を廃止し、Linux と同じ `FONT_LANE_RATIO = 0.6` を導入。`font_size = (content_size.height / lanes) * FONT_LANE_RATIO` を起動時に算出して `NSFont` を生成し、その値を `setFontSize` とテキスト計測 (`measure_text` 用フォント・`text_h`) まで一貫して使う（計測フォントと描画サイズがズレると弾の位置計算が崩れるため、単一の font_size を引き回す）。

### 4. デバッグ背景の設定化

- 現状: `BACKGROUND_TINT: Option<(...)> = Some((0.39, 0.63, 0.86, 0.08))` ハードコードで**常時点灯**。
- 変更: `config.debug_background` が `true` のときだけ着色、デフォルト（false）は `NSColor::clearColor()` で完全透過。Linux の `debug_background` 261-265 行と同じ意味。
- `BACKGROUND_TINT` 定数は色値だけ残し、適用可否を config で制御する形にする。

### 5. send の serve 自動起動（新規）

Linux の `spawn_serve` + `wait_for_socket`（202-243 行）を移植する。

- 現状: `run_send` は接続失敗で即 `FAILURE`。
- 変更:
  - `UnixStream::connect` 失敗時に、自分自身を `serve --screen N` として親から切り離して起動。
  - 切り離し方法: Linux は `pre_exec` + `libc::setsid()`。macOS でも `std::os::unix::process::CommandExt::pre_exec` + `libc::setsid()` が同様に使える（要 `libc` 依存追加）。stdin/stdout/stderr は `Stdio::null()`。
  - 起動後 `wait_for_socket(path, 5s)` で socket が listen 可能になるまでポーリング接続。
  - **macOS 固有の注意**: serve は GUI（メインスレッドで `NSApplication.run()`）。自分自身を別プロセスとして起動するので、新プロセス側で通常どおり `run_serve` が走る。アクティベーションポリシーは `.accessory`（= macOS のバックグラウンドエージェントのお作法。`.app` 化するなら Info.plist `LSUIElement`）。結果として Dock には出ないが、これは Linux に合わせるための抑制ではなく macOS の常駐補助プロセスの正しい姿。要実機検証（後述）。
  - **手段の留意**: `setsid` 自己再起動は Linux 流の実現手段。「ユーザーが手動で serve を起動しなくてよい」という機能は合わせるが、macOS ネイティブな実現手段は launchd（LaunchAgent）。PoC（フェーズ 0）で setsid 方式が通用するか確認し、不可なら LaunchAgent への切り替えを検討する。
- 成功時に Linux と同じく `println!("sent {count} message(s) to screen {screen}")` を出力。

### 6. アイドル自動終了（新規）

Linux の idle timeout（344-362 行）を移植する。

- 現状: 無し（serve は手動終了まで常駐）。
- 変更:
  - `DanmakuState` に `last_activity` 相当を追加し、ペイロード受信時刻を更新（macOS は `CACurrentMediaTime()` 基準で保持）。
  - `idle_timeout_min > 0` のとき、定期チェックで「最終受信からの経過 ≥ timeout」なら `NSApplication.terminate` 等で終了。
  - チェック周期は NSTimer（既存の tick とは別に 30 秒間隔、または既存 tick 内で経過判定）。Linux は 30 秒間隔。
  - `last_activity` の更新箇所: 現状は別スレッドの listener → channel → メインスレッドの tick で drain している。`spawn_messages` を呼んだ直後（tick 内）に更新するのが安全。

### 7. Payload の簡素化（独自オプション削除）

- 現状の `Payload`: `{ screen, messages, color, speed, size }`。`color/speed/size` は送信されるが serve 側で未使用（デッド）。
- 変更: Linux に合わせて `Payload { messages: Vec<String> }` のみへ。
  - `screen` はソケットパスで表現する（後述 8 節）ので Payload から除外。
  - `--color`/`--speed`/`--size` の clap オプションと関連フィールドを削除。
  - `Send` サブコマンドは Linux と同じく `{ screen: u32 (default 0), messages: Vec<String> (required) }` に。

### 8. マルチモニタ対応（新規実装）

Linux の per-screen socket + 対象モニタ配置に合わせる。

- ソケットパス:
  - 現状: `socket_path()` → `$TMPDIR/danmaku.sock` 固定。
  - 変更: `socket_path(screen: u32)` → `$TMPDIR/danmaku-{screen}.sock`（未設定時 `/tmp`）。Linux の `danmaku-{screen}.sock`（XDG_RUNTIME_DIR）に概念対応。
- `Serve` サブコマンド: `{ screen: u32 (default 0) }` を受け取る（現状は引数なし）。
- 対象モニタ選択（**フェーズ3b で保留中**）:
  - 現状: `NSScreen::mainScreen()` 固定（配線後もここは未変更）。
  - 変更（保留）: `NSScreen::screens(mtm)` の `screen` 番目を選択（範囲外なら Linux と同じくエラーログを出して abort）。Linux の `monitor_for_screen`（622 行）に対応。
  - パネルの frame をその screen の `frame()` 基準で計算（縦 75% / 縦中央は現状ロジックを流用）。
  - 保留理由: 開発環境が単一モニタで視認確認できないため。配線（socket/spawn）は 3a で実施済み。
- send 側: `--screen` を socket パスと、自動起動する `serve --screen N` に伝搬（**3a で実施済み**）。

### 9. 細部のログ/メッセージ整合

- send 成功時の標準出力、各種 `eprintln!` 文言を Linux 版のトーンに合わせる（プレフィックス `danmaku:`、idle 終了ログ等）。

---

## 影響を受けるファイル

- `apps/danmaku-gui-macos/src/main.rs` — 上記すべての本体改修。
- `apps/danmaku-gui-macos/Cargo.toml` — `[[bin]] name = "danmaku"`、`toml`・`libc` 依存追加。
- `apps/danmaku-gui-macos/IMPLEMENTATION.md` — マルチモニタ「未対応」記述の更新、独自オプション削除の反映（任意だが整合のため推奨）。
- （任意）`apps/danmaku-gui-macos/README.md` 相当 — Linux README に倣い、設定・自動起動・アイドル終了・`--screen` を記載。現状 macOS 側に README は無いため、Linux README をベースに新規作成を検討。

## 実装順序（推奨）

1. コマンド/バイナリ名統一 + ログプレフィックス（2 節以降の土台、小さく安全）。
2. Payload 簡素化・独自オプション削除（7 節）。
3. 設定ファイル対応 + レーン数可変化 + debug_background 化（2/3/4 節）。
4. アイドル自動終了（6 節）。
5. マルチモニタ対応（8 節）。
6. send の serve 自動起動（5 節）。
7. ログ整合・ドキュメント更新（9 節 + IMPLEMENTATION/README）。

各ステップ後に `cargo build` で確認。

## 要実機検証（macOS でのみ確認可能）

- send からの serve 自動起動（`setsid` + 自己再起動で GUI が正しく立ち上がるか、`.accessory` で Dock に出ないか）。
- `wait_for_socket` のタイムアウト 5 秒で GUI 初期化が間に合うか。
- マルチモニタでの `--screen N` 配置（NSScreen のインデックスと物理配置の対応）。
- アイドル自動終了が `NSApplication` のラン中に確実に発火するか。
- debug_background=false での完全透過。

## 未解決/留意点

- `NSApplication.terminate` をアイドル終了に使う場合の終了コード扱い（Linux は `app.quit()` → `ExitCode`）。macOS の終了経路に合わせる。
- 自己再起動時の実行パス取得は `std::env::current_exe()`（Linux と同じ）。.app バンドル化や launchd（LaunchAgent）化した場合の挙動は将来検討（現状は素のバイナリ前提）。
- 設定パス `~/.config/danmaku/config.toml` は、本来 macOS ネイティブなら `~/Library/Application Support`／`~/Library/Preferences` だが、「手編集する設定ファイルとしては `~/.config` が実態的」という判断で**意図的に Linux 流へ寄せた唯一の例外**（対話で決定済み）。上記「OS の扱いは OS に従う」原則の例外であることを明記しておく。

---

## ToDo（フェーズ分け）

実現性の不確実性（特に send からの serve 自動起動）を考慮し、**PoC を最優先**で前倒しする。PoC の結果で配布形態（素のバイナリ or .app バンドル）が変わり得るため、確実な土台整備を先に進めつつ、自動起動だけ早期に切り出して検証する。

### フェーズ 0: PoC — send からの serve 自動起動（最優先・ブロッカー）

目的: 「`send` が親から切り離して spawn した子プロセスが、WindowServer に接続して NSPanel を表示できるか」を、本実装に入る前に確かめる。ここがダメなら設計（起動方式・配布形態）から見直す。

- [x] 現状コードのまま、`run_send` の接続失敗時に `setsid` + `current_exe()` で自分自身を `serve` として spawn する最小実装を仮で入れる。
- [x] 端末を閉じた／別プロセスから叩いた状態で、子 serve がオーバーレイを表示できるか実機確認。→ 別ターミナルでも表示 OK、端末を閉じても serve 生存（`ps` で PPID=1）。
- [x] `wait_for_socket(5s)` で GUI 初期化に間に合うか確認。→ 問題なし。
- [x] **ブロッカー判定（これのみが合否）**: 親から切り離した子 serve が WindowServer に接続し、オーバーレイを描画できるか。→ **成功**。setsid 方式でフェーズ 1 以降を進める。
- [x] **情報確認（合否ゲートではない）**: `.accessory` ポリシーで Dock に出ないこと。→ Dock 非表示を確認。
- 付随対応: serve の socket 確保（`ensure_socket_available` + `bind`）をパネル生成より前へ移動。二重起動の負け側が UI を出さず即終了するようにし、一過性のパネル重なり（背景が一瞬濃くなる症状）を解消（確定的な改善のため残置）。
- 留意: 上記の PoC コードには `// PoC:` マーカーと `spawning (PoC)…` ログを残置。**フェーズ 4 の本実装昇格時に除去する**。

### フェーズ 1: OS 非依存の土台整備（PoC と並行可）

純粋 Rust ロジックで macOS 依存が薄く、確実に進められる部分。PoC の結果に左右されない。

- [x] コマンド/バイナリ名統一（`danmaku`）＋ログプレフィックス `danmaku:`（1 節）。
- [x] Payload 簡素化・独自オプション削除（7 節）。
- [x] 設定ファイル対応 `Config`/`config_path`/`load_config`＋定数移植＋`toml` 依存追加（2 節）。
- [x] レーン数可変化（`MAX_LINES` 廃止 → `state.lanes`）（3 節）。
- [x] フォントをレーン高さ連動化（`FONT_SIZE` 廃止 → `FONT_LANE_RATIO`）（3 節）。実機で lanes 2/8/16 のフォント変化を確認。
- [x] `debug_background` 設定化（デフォルト透過）（4 節）。
- [x] 各ステップ後 `cargo build`。実機で透過・レーン可変・背景設定・フォント連動を視認確認済み。
- 補足: `screen` 引数は受け取るのみで未配線（socket 切替はフェーズ3）。`libc`/`toml` は `cargo add`（最新解決）で追加。

### フェーズ 2: serve 側の振る舞い拡張

GUI 本体に手を入れるが、方式は実績あり（NSTimer 等）でリスク低め。

- [x] アイドル自動終了（`last_activity` 更新 + アイドル判定専用 NSTimer（30秒周期）+ `std::process::exit` 終了経路）（6 節）。
  - 判定（ドメイン）とタイマー機構（周期・ライフサイクル）を別責務として分離し、描画/ドレインの tick には相乗りさせない。`idle_timeout_min == 0` のときはタイマーを生成しない（＝自動終了しない、を構造で表現）。
- [x] 自動終了発火を確認（`send` 自動起動経路、`idle_timeout_min=1`/`3` で常駐継続→アイドル終了を確認）。

### フェーズ 3: マルチモニタ対応

> **方針（重要）**: 開発環境にマルチモニタが無く、画面選択ロジックは視認確認できない。
> そこで **3a（socket/`--screen` の配線）は実施**し、**3b（実際の画面選択）は保留**する。
> 配線だけ先に入れる理由: フェーズ4 の自動起動（`serve --screen N` を spawn）が screen を渡す
> 前提のため、ここを通しておかないとフェーズ4で spawn 経路を再度触ることになるから。
> 保留中の状態: `--screen N` は socket パス（`danmaku-{screen}.sock`）と spawn には反映されるが、
> 描画先は常に `mainScreen`（screen 番号で実際のモニタは切り替わらない）。

#### フェーズ 3a: 配線（実施）

- [x] `socket_path(screen)` を `danmaku-{screen}.sock` 化（8 節）。
- [x] `Serve { screen }` / `Send { screen, messages }` の `--screen` を `socket_path` / 自動起動 `serve --screen N` まで配線。
- [x] 単一モニタ（screen=0）で回帰確認。

#### フェーズ 3b: 画面選択（保留 — マルチモニタ環境が用意でき次第）

- [ ] `NSScreen::screens()` から対象選択・パネル配置（範囲外は abort）。※現状は `mainScreen` 固定。
- [ ] 実機（マルチモニタ）で `--screen` 配置を確認。番号の意味が環境依存である点を README に明記。

### フェーズ 4: 自動起動の本実装（PoC を反映）

- [x] フェーズ 0 の PoC 結果に沿って `spawn_serve` + `wait_for_socket` を本実装に昇格（PoC マーカー除去、setsid 方式で確定）（5 節）。
- [x] send 成功時の `sent N message(s) to screen N` 出力（9 節）。
- [x] 自動起動 → 弾幕表示の一連を実機確認（未起動→`launching it`→送信、既存起動時は即送信、いずれも `sent N message(s)` 出力を確認）。

### フェーズ 5: ドキュメント整合

- [ ] `IMPLEMENTATION.md` のマルチモニタ「未対応」記述・独自オプション削除を反映。
- [ ] Linux README をベースに macOS 版 README を作成（設定・自動起動・アイドル終了・`--screen`）。

### 依存関係メモ

- フェーズ 0（PoC）はフェーズ 4 の前提。並行してフェーズ 1〜3 は進められる（自動起動に依存しない send は「serve を手動起動しておけば」テスト可能）。
- フェーズ 4 はフェーズ 0 の判定が出てから着手。フェーズ 5 は最後。
