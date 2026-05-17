// Phase 8 着手手順 #3: レーン管理 + 複数弾 spawn を Linux 版から移植。
// 駆動方式: CABasicAnimation を弾ごとに 1 個 (宣言的、macOS 流)。
// 検証のため起動時に 12 メッセージを spawn (max_lines=8 を超え、重なり挙動も確認)。
//
// 動作確認用に背景をわずかに着色して領域を可視化する。本実装に進む際は BACKGROUND_TINT を None に戻す。

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSFont,
    NSFontAttributeName, NSPanel, NSScreen, NSScreenSaverWindowLevel, NSStringDrawing,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{NSDictionary, NSNumber, NSPoint, NSRect, NSSize, NSString};
use objc2_quartz_core::{CABasicAnimation, CALayer, CAMediaTiming, CATextLayer};
use rand::seq::IndexedRandom;
use rand::RngExt;

const BACKGROUND_TINT: Option<(f64, f64, f64, f64)> = Some((0.39, 0.63, 0.86, 0.08));

const MAX_LINES: usize = 8;
const BASE_SPEED: f64 = 250.0; // px/sec
const SPEED_JITTER: f64 = 0.3; // ±30%
const SPAWN_GAP_SEC: f64 = 1.5; // 同レーンの最小再使用間隔
const PAYLOAD_STAGGER_MAX_SEC: f64 = 0.25; // 同一ペイロード内のずらし最大値
const FONT_SIZE: f64 = 48.0;

// 検証用サンプルメッセージ (12 件: MAX_LINES=8 を超えてレーン重なりも確認)
const SAMPLE_MESSAGES: &[&str] = &[
    "弾幕テスト 1",
    "danmaku scroll OK",
    "Rust + objc2-app-kit",
    "CABasicAnimation",
    "右から左へ",
    "レーン管理移植",
    "速度ジッタ ±30%",
    "stagger 0-250ms",
    "9 番目: 重なり発生",
    "10 番目",
    "11 番目",
    "12 番目",
];

struct Bullet {
    #[allow(dead_code)] // Retained を保持して layer の寿命を延ばす目的
    layer: Retained<CATextLayer>,
}

struct DanmakuState {
    bullets: Vec<Bullet>,
    last_spawn_at: Vec<Option<f64>>, // CACurrentMediaTime 基準秒
}

impl DanmakuState {
    fn new() -> Self {
        Self {
            bullets: Vec::new(),
            last_spawn_at: vec![None; MAX_LINES],
        }
    }
}

fn main() {
    let mtm = MainThreadMarker::new().expect("must run on main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let screen = NSScreen::mainScreen(mtm).expect("no main screen");
    let screen_frame = screen.frame();

    let target_h = screen_frame.size.height * 0.75;
    let target_y = screen_frame.origin.y + (screen_frame.size.height - target_h) / 2.0;
    let panel_rect = NSRect {
        origin: NSPoint {
            x: screen_frame.origin.x,
            y: target_y,
        },
        size: NSSize {
            width: screen_frame.size.width,
            height: target_h,
        },
    };

    let style_mask = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;

    let panel: Retained<NSPanel> = {
        let alloc = NSPanel::alloc(mtm);
        NSPanel::initWithContentRect_styleMask_backing_defer(
            alloc,
            panel_rect,
            style_mask,
            NSBackingStoreType::Buffered,
            false,
        )
    };

    panel.setLevel(NSScreenSaverWindowLevel);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    panel.setOpaque(false);
    panel.setHasShadow(false);
    panel.setIgnoresMouseEvents(true);
    panel.setHidesOnDeactivate(false);

    let bg = match BACKGROUND_TINT {
        Some((r, g, b, a)) => NSColor::colorWithCalibratedRed_green_blue_alpha(r, g, b, a),
        None => NSColor::clearColor(),
    };
    panel.setBackgroundColor(Some(&bg));

    let mut state = DanmakuState::new();
    spawn_test_messages(&panel, &mut state, mtm);

    panel.orderFrontRegardless();

    eprintln!(
        "danmaku-gui-macos: panel shown ({}x{}); spawned {} test bullets",
        panel_rect.size.width as i64,
        panel_rect.size.height as i64,
        state.bullets.len()
    );

    // state を main の最後まで生かす (Bullet 内の Retained<CATextLayer> を生存させるため)
    app.run();
    drop(state);
}

fn spawn_test_messages(panel: &NSPanel, state: &mut DanmakuState, mtm: MainThreadMarker) {
    let Some(content) = panel.contentView() else {
        return;
    };
    content.setWantsLayer(true);
    let Some(root_layer) = content.layer() else {
        return;
    };
    let content_size = content.frame().size;
    let scale = NSScreen::mainScreen(mtm)
        .map(|s| s.backingScaleFactor())
        .unwrap_or(2.0);
    let font = NSFont::boldSystemFontOfSize(FONT_SIZE);

    let messages: Vec<String> = SAMPLE_MESSAGES.iter().map(|s| s.to_string()).collect();
    spawn_messages(
        &messages,
        state,
        &root_layer,
        content_size,
        scale,
        &font,
    );
}

fn spawn_messages(
    messages: &[String],
    state: &mut DanmakuState,
    root_layer: &CALayer,
    content_size: NSSize,
    contents_scale: f64,
    font: &NSFont,
) {
    let mut rng = rand::rng();
    let now = unsafe { ca_current_media_time() };
    for msg in messages {
        // 空きレーン (last_spawn_at + SPAWN_GAP_SEC < now) を集める
        let free: Vec<usize> = (0..MAX_LINES)
            .filter(|&i| {
                state.last_spawn_at[i]
                    .map(|t| now - t >= SPAWN_GAP_SEC)
                    .unwrap_or(true)
            })
            .collect();
        let lane = if let Some(&l) = free.choose(&mut rng) {
            l
        } else {
            let l = rng.random_range(0..MAX_LINES);
            eprintln!("danmaku-gui-macos: no free lane; overlapping on lane {l}: {msg:?}");
            l
        };
        let speed_factor = 1.0 + rng.random_range(-SPEED_JITTER..SPEED_JITTER);
        let speed = BASE_SPEED * speed_factor;
        let stagger = rng.random_range(0.0..PAYLOAD_STAGGER_MAX_SEC);
        let begin_time = now + stagger;
        state.last_spawn_at[lane] = Some(begin_time);

        let bullet = create_bullet(
            msg,
            lane,
            speed,
            begin_time,
            root_layer,
            content_size,
            contents_scale,
            font,
        );
        state.bullets.push(bullet);
    }
}

fn create_bullet(
    text: &str,
    lane: usize,
    speed: f64,
    begin_time: f64,
    root_layer: &CALayer,
    content_size: NSSize,
    contents_scale: f64,
    font: &NSFont,
) -> Bullet {
    let ns_text = NSString::from_str(text);
    let text_size = measure_text(&ns_text, font);
    let text_w = text_size.width.max(1.0);
    let text_h = text_size.height.max(FONT_SIZE);

    let text_layer: Retained<CATextLayer> = CATextLayer::new();
    unsafe {
        text_layer.setString(Some(&*ns_text));
    }
    // CATextLayer.setFont は CFType (CTFont) を要求する。NSFont は toll-free bridged で
    // ある CTFontRef と同じメモリレイアウトなのでポインタキャストでよい。
    {
        use objc2_core_foundation::CFType;
        let cf_font: &CFType = unsafe { &*(font as *const NSFont as *const CFType) };
        unsafe { text_layer.setFont(Some(cf_font)) };
    }
    text_layer.setFontSize(FONT_SIZE);
    text_layer.setForegroundColor(Some(&NSColor::whiteColor().CGColor()));
    text_layer.setContentsScale(contents_scale);
    text_layer.setBounds(NSRect {
        origin: NSPoint { x: 0.0, y: 0.0 },
        size: NSSize {
            width: text_w,
            height: text_h,
        },
    });

    let y = lane_y(lane, content_size.height, text_h);
    let start_x = content_size.width + text_w / 2.0;
    let end_x = -text_w / 2.0;
    // 最終位置 (アニメーション完了後はここに留まる = 画面外)
    text_layer.setPosition(NSPoint { x: end_x, y });

    root_layer.addSublayer(&text_layer);

    let distance = start_x - end_x;
    let duration = distance / speed;

    let anim: Retained<CABasicAnimation> =
        CABasicAnimation::animationWithKeyPath(Some(&NSString::from_str("position.x")));
    let from = NSNumber::numberWithDouble(start_x);
    let to = NSNumber::numberWithDouble(end_x);
    unsafe {
        anim.setFromValue(Some(&*from));
        anim.setToValue(Some(&*to));
    }
    anim.setDuration(duration);
    anim.setBeginTime(begin_time);
    // 完了後に layer を消すのは今は未実装 (次フェーズ: 定期 prune を入れる)
    text_layer.addAnimation_forKey(&anim, Some(&NSString::from_str("scroll")));

    Bullet { layer: text_layer }
}

// レーン y 座標: ウィンドウ全高を MAX_LINES で等分し、各レーンの中央を返す。
// Phase 7 の方針 (内側マージン無し) を反映。
fn lane_y(lane: usize, height: f64, text_h: f64) -> f64 {
    let slot_h = height / MAX_LINES as f64;
    let slot_center = slot_h * (lane as f64 + 0.5);
    // CATextLayer の bounds 中央が position に来るので、垂直中央寄せ
    // ただし CATextLayer はテキストを top-aligned で描画するため、調整を入れる
    let _ = text_h;
    slot_center
}

fn measure_text(text: &NSString, font: &NSFont) -> NSSize {
    let keys: [&NSString; 1] = [unsafe { NSFontAttributeName }];
    let values: [&AnyObject; 1] = [font as &NSFont as &AnyObject];
    let attrs = NSDictionary::from_slices(&keys, &values);
    unsafe { text.sizeWithAttributes(Some(&attrs)) }
}

// CACurrentMediaTime: CABasicAnimation.beginTime と同じ時間軸 (mach_absolute_time ベース)
unsafe extern "C" {
    fn CACurrentMediaTime() -> f64;
}
unsafe fn ca_current_media_time() -> f64 {
    unsafe { CACurrentMediaTime() }
}
