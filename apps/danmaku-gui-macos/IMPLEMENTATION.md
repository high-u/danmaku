# danmaku-gui-macos: フェーズ8 実装の整理

macOS 上で「透過 / クリックスルー / 最前面 / 全 Space 表示 / 弾幕スクロール / socket 経由送信」を満たすオーバーレイを実装するにあたり、設計判断を「公式準拠」「一般的アプローチ」「ハック的」「対話で決定した設計変更」に整理する。各項目には【事実】(一次情報の出典あり) と【解釈】(事実から導いた推論・選択) を明示する。

## 0. 対話で決定した設計変更 (経緯記録)

このフェーズは設計判断が複数回ひっくり返った。後から経緯を辿れるように主要な変更点を残す。

### 0.1 言語: Swift → Rust + objc2

- 【事実】SPECS 初版では macOS 側を Swift + `swift` CLI (Xcode 不要) で実装する想定だった。
- 【事実】macOS 26 (Tahoe) + Command Line Tools 26.5.0 環境で `swift package init` 直後の最小 Package.swift すら `swift build` が `Undefined symbols: PackageDescription.Package.__allocating_init(name:..., swiftLanguageVersions: [SwiftVersion]?, ...)` でリンク失敗する。`nm` で `libPackageDescription.dylib` を確認すると同名 init は `swiftLanguageVersions: [SwiftLanguageMode]?` でしか存在せず、`.swiftinterface` ヘッダ宣言と dylib 実装が型レベルでズレている (Apple 配布物の不整合)。
- 【事実】swift-tools-version を 5.9 / 6.0 / 6.1 / 6.2 のいずれにしても同じシンボル不一致でリンク失敗する (manifest 言語バージョンに依存しない)。
- 【事実】CLT 26.5 は 2026-05 時点で最新版で、`softwareupdate` 経由で更新済み。
- 【事実】上記の根本回避には (a) Xcode 本体 (App Store, 15GB 超) を導入、(b) swift.org 公式 toolchain を別途インストール (1〜2GB、`TOOLCHAINS` 環境変数で切替) のいずれかが必要。
- 【事実】Rust + `objc2-app-kit` v0.3.2 (累計 2860 万 DL / 直近 90 日 1045 万 DL / 228 個の crate が依存) は Apple SDK ヘッダから自動生成された型付き API。winit / wgpu / Tauri / Servo / Slint が採用。
- 【解釈】「軽量・標準・安全・無用なハック回避・標準以上の安心はない」というプロジェクト方針に対し、Xcode 15GB 導入や swift.org toolchain 併用はいずれも「重い」「Apple 公式外」になる。一方 Rust + objc2 は本プロジェクトの他コンポーネント (`danmaku-gui-linux` / `danmaku-cli` / `getscreens`) と同じ Rust + Cargo に揃い、Apple toolchain への依存自体を回避できる。
- 【解釈】Rust + objc2 は「Apple 公式の標準」ではないが、「Rust エコシステム内の事実上の標準」であり、winit / wgpu 等の中核ライブラリが採用していることから「ハック」とは性格が異なる。

### 0.2 アーキテクチャ: `danmaku-cli` 独立バイナリ → `danmaku-gui send` サブコマンド統合

- 【事実】SPECS 初版は `danmaku-cli` (送信専用、クロス OS) と `danmaku-gui-*` (常駐、OS 別) の 2 バイナリ構成だった。
- 【事実】socket は「常駐プロセス ↔ ephemeral 送信プロセス」の通信のために存在しており、両者が同じバイナリの異なるサブコマンドであっても (どちらも別プロセスである限り) socket は不可避。
- 【事実】SPECS 内の「差し替え可能性」(SPECS 278 行) は SKILL.md が叩くコマンド名の問題で、別バイナリにすべき根拠ではない。
- 【解釈】専用クライアントを別バイナリ化する実利が薄い (バイナリ 1 個減る程度) ため、macOS 側を `danmaku-gui` 単一バイナリ + `serve` / `send` サブコマンドに統合した。socket は GUI 内部の実装詳細に降格し、利用者・AI エージェントは `danmaku-gui send "..."` だけを叩く。
- 【解釈】Linux 側の追従 (Phase 7) で同じ統合形に揃える方針。

### 0.3 描画駆動: 手動 tick → CABasicAnimation 個別

- 【事実】Linux 版 (`danmaku-gui-linux`) は `add_tick_callback` で毎フレーム手動更新する方式。
- 【解釈】macOS では各弾を CABasicAnimation 1 個で駆動する宣言的方式を採用。理由は (a) Core Animation は GPU 同期で滑らかに描画する、(b) 速度ジッタは duration 差、ペイロード内 stagger は `beginTime` で表現でき、Linux 版の振る舞いをほぼ満たせる、(c) macOS のお作法 (手書き run loop ではなく CA を信頼する) に沿う方が将来の OS 変更に強い。Linux 版と完全同一の構造に揃える必要性は無いと判断した。

### 0.4 スレッド境界: dispatch_async ではなく channel + NSTimer (block2) ポーリング

- 【事実】CALayer の操作はメインスレッドで行うのが安全 (Apple 公式)。socket の accept は別スレッドで動かしたい。
- 【事実】Rust から libdispatch にアクセスする標準的な手段は (a) `dispatch2` クレート、(b) libc 経由で生 `dispatch_async_f` を叩く、(c) NSTimer + block2 で main thread からポーリング、のいずれか。
- 【解釈】弾幕は LLM 生成コメントが入力源で、100ms ポーリングの遅延は実害なし。最小 FFI で済み、追加クレートが `block2` 1 個で済むため (c) を採用。`std::thread` で UnixListener を回し、`std::sync::mpsc::channel` で main thread へ送る。NSTimer (100ms) が channel を drain → `spawn_messages` を呼ぶ、と同時に期限切れの CATextLayer を `removeFromSuperlayer` する。

## 1. 公式情報に沿っている部分

### 透過 NSPanel: `backgroundColor = .clear` + `isOpaque = false` + `hasShadow = false`

- 【事実】Apple Developer Documentation `NSWindow` 項に明記されている標準手順。`NSPanel` は `NSWindow` のサブクラス。
- 【解釈】GTK + X11 のような特殊操作 (RGBA visual, EWMH ClientMessage) は不要。OS が標準で透過を扱う。

### クリックスルー: `ignoresMouseEvents = true`

- 【事実】Apple Developer Documentation `NSWindow.ignoresMouseEvents` 項。`true` にするとマウスイベントが背後のウィンドウに透過する。
- 【解釈】Linux の `set_input_region(empty_region)` と意味的に等価で、こちらは 1 プロパティで済む。

### 最前面: `level = NSScreenSaverWindowLevel`

- 【事実】Apple Developer Documentation `NSWindow.level` 項。`NSScreenSaverWindowLevel` (`CGWindowLevelForKey(.screenSaverWindow)` 相当) は標準の最前面レベル。
- 【解釈】通常のアプリより上、メニューバーよりも上 (フルスクリーンアプリの上に乗せる必要があるため screenSaver レベルを選択)。

### 全 Space + フルスクリーンアプリ上に表示: `collectionBehavior`

- 【事実】Apple Developer Documentation `NSWindow.CollectionBehavior` 項。組合せは bitflag。本実装で採用:
  - `canJoinAllSpaces`: すべての Space に同時存在
  - `stationary`: Space 切替時に動かない (== overlay として固定)
  - `fullScreenAuxiliary`: フルスクリーンアプリの上に乗る (補助ウィンドウ扱い)
  - `ignoresCycle`: Cmd+` / Cmd+Tab のウィンドウサイクルから除外
- 【解釈】この 4 つの組合せが「弾幕オーバーレイ」のユースケースに最小十分。

### `nonactivatingPanel` style mask

- 【事実】Apple Developer Documentation `NSPanel` 項。`nonactivatingPanel` を指定すると、パネルが表示されてもアプリがアクティブ化しない (他アプリのフォーカスを奪わない)。
- 【解釈】overlay として最適。

### `accessory` activation policy

- 【事実】`NSApplication.setActivationPolicy(.accessory)`。Dock に出ない、メニューバーを乗っ取らない、Cmd+Tab に出ない。
- 【解釈】常駐 overlay の標準的なポリシー。`.regular` は通常アプリ、`.prohibited` は完全 UI 無し用。

### Unix domain socket と `$TMPDIR`

- 【事実】macOS では `$TMPDIR` がユーザ単位の一時ディレクトリを指す (`/var/folders/.../T/` 等)。複数ユーザログイン環境でも干渉しない。
- 【事実】Linux の `$XDG_RUNTIME_DIR` に対応する macOS の慣習が `$TMPDIR`。
- 【解釈】socket パスを `$TMPDIR/danmaku.sock` (未設定なら `/tmp/danmaku.sock`) にすることで、ユーザ単位の隔離が自然に得られる。

## 2. 一般的なアプローチ

### Core Animation で描画 (`CATextLayer` + `CABasicAnimation`)

- 【事実】NSView の `draw` をオーバーライドして毎フレーム再描画する方式と、CALayer ベースの宣言的アニメーション方式が選択肢。
- 【事実】CATextLayer は Apple 公式で AppKit/UIKit 両方に存在する単純テキスト描画レイヤ。
- 【事実】CABasicAnimation の `position.x` キーパスに `fromValue` / `toValue` を設定すると、Core Animation がフレーム間補間を担当する (GPU 同期)。
- 【解釈】Linux 版の手動 tick (`add_tick_callback`) と比べると構造は違うが、Core Animation を信頼する書き方は macOS のお作法。「速度ジッタは duration、stagger は beginTime」で振る舞いを表現できる。

### `Cargo.toml` `[[bin]] name = "danmaku-gui"`

- 【事実】Cargo はデフォルトでパッケージ名をバイナリ名にするが、`[[bin]]` セクションで明示できる。
- 【解釈】ソース管理上のディレクトリ名は OS を識別するため `danmaku-gui-macos` のままにし、ビルド成果物は OS 共通の `danmaku-gui` に揃える。Linux 機と macOS 機が同一インストールに共存することは無いので、PATH 上の名前衝突は起きない。

### `objc2-quartz-core` の features 制限

- 【事実】`objc2-quartz-core` v0.3.2 のデフォルトフィーチャは `objc2-metal` v0.3.2 を引き、しかし `objc2-metal` の crates.io 最新版は v0.3.1 で **upstream のバージョン不整合がある**。
- 【解釈】Metal は本プロジェクトで使わないため、`default-features = false` で必要なフィーチャ (`std` / `CAAnimation` / `CABase` / `CALayer` / `CAMediaTiming` / `CATextLayer` / `CATransaction` / `CoreAnimation` / `objc2-core-foundation` / `objc2-core-graphics`) のみ列挙して回避。

## 3. ハック的 / 公式準拠ではない部分

### CATextLayer.setFont への NSFont を toll-free bridging で渡す

- 【事実】`objc2-quartz-core` の `CATextLayer.setFont` シグネチャは `Option<&CFType>` (CTFont を想定)。
- 【事実】NSFont と CTFontRef は Apple のドキュメントで toll-free bridged と明記。同一メモリレイアウト。
- 【事実】本実装は `unsafe { &*(font as *const NSFont as *const CFType) }` のポインタキャストでブリッジしている。
- 【解釈】CTFont を `CTFontCreateWithFontDescriptor` 経由で作る方が型としては綺麗だが、toll-free bridging はそのために用意されている仕組みで、Apple 公式が認めた振る舞いの利用。
- 【解釈】将来 objc2 側が NSFont → CFType の安全な変換ヘルパを提供したら差し替える余地あり。

### テキスト幅の計測に `NSString.sizeWithAttributes`

- 【事実】`NSString.sizeWithAttributes` は AppKit の `NSStringDrawing` カテゴリ。アトリビュート付き文字列のレンダリング寸法を返す。
- 【事実】本来は `NSAttributedString` を使う方が attributed 描画と一貫するが、CATextLayer は内部で plain string を独自描画するため、寸法計測だけ AppKit を借りる形になる。
- 【解釈】CATextLayer 自体は寸法を返す API を持たない (`bounds` を呼び出し側が決める設計)。AppKit による外部計測が現実解。

### 確認用の半透明青背景 (`BACKGROUND_TINT`)

- 【事実】見た目の検証用。本番では `None` に戻して `NSColor.clearColor()` で完全透過にする。
- 【事実】コード中にコメントで「本実装に進む際は None に戻す」と明記済み。
- 【解釈】トレイアイコン (Phase 6) など常駐の手がかりが揃ってから消す予定。それまでは「動いているのか見える」状態を維持。

## 4. 検証済み / 未検証

### 検証済み (実機で挙動確認済み)

- 透過レンダリング
- クリックスルー
- 最前面固定
- 全 Space 表示
- 縦 75% / 横 100%・縦中央配置
- 複数弾レーン分散 + 速度ジッタ + stagger
- 重なり警告ログ (空きレーン無し時)
- `danmaku-gui send "..."` 経由で常駐側に弾幕が流れる
- send モードの正常終了 + 接続失敗時の非ゼロ終了

### 未検証

- マルチモニタ環境 (現状 `NSScreen.mainScreen()` 固定、`--screen N` の serve 側未対応 → Phase 8 とは別のフェーズで対応)
- 長時間連投時のメモリ挙動 (NSTimer 内の `removeFromSuperlayer` + `bullets.retain` で都度回収しているが、実機検証は短時間のみ)
- フルスクリーンアプリ (動画プレイヤ等) の上での表示

## 5. 参照したクレート

- [`objc2`](https://crates.io/crates/objc2) v0.6.4 — Objective-C runtime バインディング
- [`objc2-app-kit`](https://crates.io/crates/objc2-app-kit) v0.3.2 — AppKit (NSWindow / NSPanel / NSScreen / NSFont 他) 型付き API
- [`objc2-foundation`](https://crates.io/crates/objc2-foundation) v0.3.2 — Foundation (NSString / NSDictionary / NSNumber / NSTimer 他)
- [`objc2-quartz-core`](https://crates.io/crates/objc2-quartz-core) v0.3.2 — Core Animation (CALayer / CATextLayer / CABasicAnimation)
- [`objc2-core-foundation`](https://crates.io/crates/objc2-core-foundation) v0.3.2 — CFType (toll-free bridging に必要)
- [`block2`](https://crates.io/crates/block2) v0.6.2 — Objective-C block を Rust closure から作る
- [`clap`](https://crates.io/crates/clap) v4.6.1 (derive) — サブコマンド分岐
- [`serde`](https://crates.io/crates/serde) v1.0.228 (derive) + [`serde_json`](https://crates.io/crates/serde_json) v1.0.149 — Payload の JSON シリアライズ / デシリアライズ
- [`rand`](https://crates.io/crates/rand) v0.10.1 — レーン選択 / 速度ジッタ / stagger
