# danmaku-linux: フェーズ1 実装の整理

X11 + GNOME Shell (Mutter) 上で「透過 / クリックスルー / 最前面固定 / パネル可視」を満たすオーバーレイを実装するにあたり、現状のコードを「公式準拠」「一般的アプローチ」「ハック的」の3カテゴリに整理する。各項目には【事実】(一次情報の出典あり) と【解釈】(事実から導いた推論・選択) を明示する。

## 1. 公式情報に沿っている部分

### クリックスルー: `GdkSurface::set_input_region(empty_region)`
- 【事実】GDK4 公式 API。空 `cairo::Region` を渡せば入力イベントが透過する。
- 【解釈】GTK4 で透過オーバーレイを実装する場合、これが標準的な手段。Xlib の `XShapeCombineRectangles` を直接叩く必要はない。

### ウィンドウ背景の透過: CSS `background: transparent`
- 【事実】GTK4 は RGBA visual の確保を自動で行う (GTK3 と異なり `set_visual` 不要)。`window` セレクタの背景を `transparent` (または rgba) にすれば透過する。
- 【解釈】子ウィジェット (`window > *`) にも明示的に `background: transparent` を付ける構成にしているが、これは念のための予防策。

### EWMH `_NET_WM_STATE` の操作タイミング
- 【事実】EWMH 仕様 ar01s05 に「マップ前は `XChangeProperty` で直接書く / マップ後は `_NET_WM_STATE` の `ClientMessage` をルートウィンドウに `SubstructureRedirect | SubstructureNotify` マスク付きで送る」と規定。
- 【事実】現コードは両方を実装している (`set_x11_initial_wm_state` でマップ前 property、`send_x11_state_change_above` でマップ後 ClientMessage)。
- 【解釈】マップ前 property だけでは ABOVE が META_LAYER_TOP に反映されない場合があるため両方残している。

### Pango + pangocairo によるテキスト描画
- 【事実】日本語含む国際化テキストの描画は GTK エコシステムで Pango が標準。`layout_path` + Cairo `stroke` で縁取り → `show_layout` で本体、という順序は pangocairo の標準的な使い方。

### 再描画: `Widget::add_tick_callback` (`FrameClock`)
- 【事実】GTK4 公式の正攻法。`FrameClock` が VSync に同期したタイミングでコールバックを呼ぶ。コールバック内で `queue_draw()` を呼ぶことで毎フレーム再描画される。
- 【経緯】当初は `glib::timeout_add_local(16ms)` + `queue_draw` で実装していた。動作はしていたが、(a) 16ms 固定は 62.5fps 想定で実機リフレッシュレート (60/75/120/144Hz) と常にずれる、(b) VSync 非同期でティアリングのリスク、(c) ディスプレイがアイドルでも timer が走る、という事実があったため `add_tick_callback` に置換。実機で挙動差が無いことを確認済み。

### マップ後処理のトリガー: `Widget::connect_map`
- 【事実】GTK4 公式シグナル。ウィジェットがマップされた (X11 `MapNotify` 相当) 時に発火する。EWMH ar01s05 が要求する「マップ後の `_NET_WM_STATE` 変更は ClientMessage」のタイミングと意味的に一致する。
- 【事実】`connect_realize` は X11 ウィンドウ生成時のシグナルで、まだマップされていない。realize ≠ map。
- 【経緯】当初は `connect_realize` の中で `glib::idle_add_local_once` を 1 回挟んでマップ完了を待つ実装だった。実機では安定動作していたが、これは「メインループが暇になった瞬間」を待つだけでマップ完了を保証しない (タイミング依存) という事実があったため、`connect_map` に分割。realize では input_region と マップ前 property、map で ClientMessage と 位置指定 (MoveWindow)、と責務が明確になった。

## 2. 一般的なアプローチ (筋は通っているが選択の余地あり)

### `decorated(false)` + `resizable(false)`
- 【事実】どちらも GTK4 公式 API。
- 【事実】`resizable(false)` は WM_NORMAL_HINTS の `min_width == max_width`, `min_height == max_height` を書き込む。
- 【事実】Mutter `src/core/window.c` で `has_resize_func` の判定にこのヒントが使われ、min == max なら `has_resize_func = FALSE`。さらに `has_maximize_func` は `mwm_has_maximize_func && has_resize_func` で決まるため、`resizable(false)` だけで `has_maximize_func = FALSE` になる。
- 【解釈】auto-maximize 経路を二重に塞ぐ (面積比 < 80% + has_maximize_func = FALSE) ことで、初期化順序に依らず MAXIMIZED が付かない状態を作っている。これは設計判断。

### GDK API と生 Xlib の混在
- 【事実】GDK4 には EWMH `_NET_WM_STATE_ABOVE` を直接トグルする API が無い (GTK3 の `set_keep_above` 相当は廃止)。位置指定 API もウィンドウシステム任せ。
- 【解釈】「公式 API でできないこと」を Xlib で補う方針。これ自体は X11 アプリでよくある構造だが、後述のリスク (GTK4 による上書き) を孕む。

### `XClientMessageEvent.send_event` の値
- 【事実】Xlib では `XClientMessageEvent.send_event` フィールドは **X サーバが配送時に上書き**する。`XSendEvent` 経由のイベントは受信側に `send_event = True` で届き、サーバ生成イベントは `False` で届く。送信側がフィールドに何を入れても機能的差異は無い。
- 【事実】EWMH 仕様 ar01s05 の C コード例は `send_event` の値を特に指定していない (`XEvent` 全体を 0 初期化してから必要フィールドだけ埋めるパターン)。
- 【事実】Rust の名前付き構造体リテラルは全フィールド必須なので「設定しない」は文法上不可。`send_event: 0` が C で言うところの「初期化しない」と等価。
- 【事実】受信側 (WM・アプリ) には「届いた合成イベント (`send_event == True`) を無視する」というポリシーを持つ実装が存在する (X11 のセキュリティ史的経緯)。ただし EWMH の `_NET_WM_STATE` ClientMessage は仕様上クライアントが送るものとして定義されており、EWMH 準拠 WM (Mutter/KWin/Openbox 等) は受理する。これは送信側の `send_event` 値とは無関係。
- 【解釈】現コードでは `send_event: 0` を採用。意味は「設定していない」の意。実機 (Mutter) で挙動差が無いことを確認済み。

## 3. ハック的 / 公式準拠ではない部分

### 横幅 = モニタ幅、縦 = モニタ高の 75% (中央寄せ)
- 【事実】Mutter `src/core/window.c` line 122 で `#define MAX_UNMAXIMIZED_WINDOW_AREA .8`。line 2416 で `if (window_area > work_area_area * MAX_UNMAXIMIZED_WINDOW_AREA)` を条件にした auto-maximize 判定がある。コメントにも "Windows that cover an area greater then this size are automaximized on map" と明記。
- 【事実】75% は 80% を下回るため面積比の判定では auto-maximize されない。
- 【解釈】「全画面に流したい」要求に対する妥協。本来はモニタ全域を覆いたいが、80% 制約を超えると MAXIMIZED が付き ABOVE レイヤーから外れる。「縦 75% 中央寄せ」はユーザの選好で決めた値であり、技術的な必然ではない (50% でも 79% でも auto-maximize 回避という目的は果たす)。
- 【解釈】将来 `resizable(false)` のみで auto-maximize が確実に塞げる (has_maximize_func = FALSE) ことが実機で十分検証されれば、面積制約を撤廃してモニタ全域に拡張する余地はある。

### 確認用の半透明青背景 (CSS `rgba(100, 160, 220, 0.08)`)
- 【事実】見た目の検証用。本番では `transparent` に戻す。
- 【解釈】コード中にコメントで「本番では戻す」と明記済み。フェーズ1 完了前に戻す。

## 検証済み / 未検証

### 検証済み (実機で挙動確認済み)
- 透過レンダリング
- クリックスルー (パネル / 他アプリへのクリックが透過)
- 最前面固定 (他アプリにフォーカスが移っても弾幕が裏に回らない)
- パネル (上下バー) 可視を維持
- 横幅 = モニタ幅 で端から端まで文字が流れる

### 未検証
- 縦 75% が実際に画面の縦方向 75% を占めているか (背景半透明色で目視確認待ち)
- マルチモニタ環境での挙動 (現状 monitor index 0 固定)
- 他デスクトップ環境 (KDE/XFCE/i3 等) での挙動

## Mutter ソースの参照箇所 (一次情報)

- `src/core/window.c:122` — `#define MAX_UNMAXIMIZED_WINDOW_AREA .8`
- `src/core/window.c:2402-2416` — auto-maximize 判定 (`auto_maximize && first_time && has_maximize_func && window_area > work_area * 0.8`)
- `src/core/window.c:3374-3399` — unmaximize 時のリサイズ規則 (同じ定数を使用)
- `meta_window_get_default_layer` — `wm_state_above && !MAXIMIZED → META_LAYER_TOP`

EWMH 仕様: <https://specifications.freedesktop.org/wm-spec/wm-spec-latest.html> ar01s05 (Application Window Properties)
