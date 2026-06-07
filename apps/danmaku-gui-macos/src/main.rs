// Phase 8: danmaku-gui (macOS)
//
// 単一バイナリで以下 2 つの動作モードを持つ:
//   danmaku-gui              引数なし or `serve` → 常駐 (透過オーバーレイ + socket listener)
//   danmaku-gui send "..."   → 常駐インスタンスに送信して即終了
//
// 内部 IPC: Unix domain socket ($TMPDIR/danmaku.sock)。
// 描画駆動: CATextLayer + CABasicAnimation を弾ごとに 1 個 (宣言的、macOS 流)。
//
// 動作確認用に背景をわずかに着色して領域を可視化する。本実装に進む際は BACKGROUND_TINT を None に戻す。

use std::cell::RefCell;
use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt; // PoC: setsid 自己再起動
use std::path::{Path, PathBuf};
use std::process::{Command as ProcCommand, ExitCode, Stdio};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant}; // PoC: wait_for_socket

use block2::RcBlock;
use clap::{Parser, Subcommand};
use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSFont,
    NSFontAttributeName, NSPanel, NSScreen, NSScreenSaverWindowLevel, NSStringDrawing,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_core_foundation::CFType;
use objc2_foundation::{NSDictionary, NSNumber, NSPoint, NSRect, NSSize, NSString, NSTimer};
use objc2_quartz_core::{CABasicAnimation, CALayer, CAMediaTiming, CATextLayer};
use rand::seq::IndexedRandom;
use rand::RngExt;
use serde::{Deserialize, Serialize};

// debug_background=true のときに表示する確認用の薄い背景色。
const BACKGROUND_TINT: (f64, f64, f64, f64) = (0.39, 0.63, 0.86, 0.08);

// デフォルト値。設定ファイル (~/.config/danmaku/config.toml) があれば上書きされる。
const DEFAULT_LANES: usize = 16;
const DEFAULT_IDLE_TIMEOUT_MIN: u64 = 30;
const MIN_LANES: u32 = 1;
const MAX_LANES: u32 = 128;
const BASE_SPEED: f64 = 250.0; // px/sec
const SPEED_JITTER: f64 = 0.3; // ±30%
const SPAWN_GAP_SEC: f64 = 1.5; // 同レーンの最小再使用間隔
const PAYLOAD_STAGGER_MAX_SEC: f64 = 0.25; // 同一ペイロード内のずらし最大値
const FONT_LANE_RATIO: f64 = 0.6; // フォント絶対高 / レーン高
const TICK_INTERVAL_SEC: f64 = 0.1; // メインスレッド側のポーリング周期
const IDLE_CHECK_INTERVAL_SEC: f64 = 30.0; // アイドル判定専用タイマーの周期

// ============================================================================
// CLI
// ============================================================================

#[derive(Parser, Debug)]
#[command(name = "danmaku", about = "Transparent danmaku overlay for macOS.")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run the overlay (default when no subcommand is given).
    Serve {
        /// Display index (from `NSScreen::screens()`).
        #[arg(long, default_value_t = 0)]
        screen: u32,
    },
    /// Send messages to the overlay (starting it if needed) and exit.
    Send {
        /// Target display index.
        #[arg(long, default_value_t = 0)]
        screen: u32,
        /// Messages to display (one or more).
        #[arg(required = true)]
        messages: Vec<String>,
    },
}

// ============================================================================
// 共有 JSON ペイロード
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
struct Payload {
    messages: Vec<String>,
}

// ============================================================================
// 設定ファイル (~/.config/danmaku/config.toml)
// ============================================================================

// 正常にパースできた場合のみ採用し、欠けたキーは個別のデフォルト値で埋める。
// 未知のキーは無視。ファイルが無い/読めない/壊れている場合は全項目デフォルト。
#[derive(Debug, Deserialize)]
#[serde(default)]
struct Config {
    /// レーン数 (1-128 にクランプ)。
    lanes: u32,
    /// 最終弾幕からこの分数を過ぎたら自動終了。0 で無効 (終了しない)。
    idle_timeout_min: u64,
    /// 領域確認用の薄い背景色を表示する (開発用)。
    debug_background: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            lanes: DEFAULT_LANES as u32,
            idle_timeout_min: DEFAULT_IDLE_TIMEOUT_MIN,
            debug_background: false,
        }
    }
}

fn config_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(dir).join("danmaku").join("config.toml"));
    }
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join(".config")
            .join("danmaku")
            .join("config.toml"),
    )
}

fn load_config() -> Config {
    let Some(path) = config_path() else {
        return Config::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Config::default();
    };
    toml::from_str::<Config>(&text).unwrap_or_default()
}

// ============================================================================
// entry
// ============================================================================

fn main() -> ExitCode {
    let cli = Cli::parse();
    // フェーズ3a: screen を socket パス / 自動起動まで配線。
    // フェーズ3b（実際の画面選択）は保留中で、描画先は mainScreen 固定。
    match cli.command.unwrap_or(Command::Serve { screen: 0 }) {
        Command::Serve { screen } => run_serve(screen),
        Command::Send { screen, messages } => run_send(screen, Payload { messages }),
    }
}

// ============================================================================
// send モード
// ============================================================================

fn run_send(screen: u32, payload: Payload) -> ExitCode {
    let mut line = match serde_json::to_string(&payload) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("danmaku: failed to serialize payload: {e}");
            return ExitCode::FAILURE;
        }
    };
    line.push('\n');

    let path = socket_path(screen);
    let mut stream = match UnixStream::connect(&path) {
        Ok(s) => s,
        Err(_) => {
            // PoC: serve 未起動なら自分自身を切り離して起動し、socket を待つ。
            eprintln!("danmaku: serve not running, spawning (PoC)…");
            if let Err(e) = spawn_serve(screen) {
                eprintln!("danmaku: failed to spawn serve: {e}");
                return ExitCode::FAILURE;
            }
            match wait_for_socket(&path, Duration::from_secs(5)) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("danmaku: {e}");
                    return ExitCode::FAILURE;
                }
            }
        }
    };
    if let Err(e) = stream.write_all(line.as_bytes()) {
        eprintln!("danmaku: failed to write to {}: {e}", path.display());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

// PoC: 自分自身を `serve` として、親から切り離した新セッションで起動する。
// （Linux 版 spawn_serve の setsid 方式が macOS でも通用するかの検証用）
fn spawn_serve(screen: u32) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let mut cmd = ProcCommand::new(exe);
    cmd.arg("serve")
        .arg("--screen")
        .arg(screen.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    cmd.spawn()?;
    Ok(())
}

// PoC: serve 起動直後の socket が listen 可能になるまで接続を試行する。
fn wait_for_socket(path: &Path, timeout: Duration) -> Result<UnixStream, String> {
    let start = Instant::now();
    loop {
        match UnixStream::connect(path) {
            Ok(s) => return Ok(s),
            Err(_) => {
                if start.elapsed() >= timeout {
                    return Err(format!(
                        "serve did not become ready within {}s",
                        timeout.as_secs()
                    ));
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

// ============================================================================
// serve モード
// ============================================================================

struct Bullet {
    layer: Retained<CATextLayer>,
    expire_time: f64, // CACurrentMediaTime 基準
}

struct DanmakuState {
    lanes: usize,
    bullets: Vec<Bullet>,
    last_spawn_at: Vec<Option<f64>>, // length == lanes
    last_activity: f64,              // 最後に弾幕を受信した時刻 (CACurrentMediaTime 基準)
}

impl DanmakuState {
    fn new(lanes: usize) -> Self {
        Self {
            lanes,
            bullets: Vec::new(),
            last_spawn_at: vec![None; lanes],
            last_activity: unsafe { CACurrentMediaTime() },
        }
    }
}

fn run_serve(screen: u32) -> ExitCode {
    let mtm = match MainThreadMarker::new() {
        Some(m) => m,
        None => {
            eprintln!("danmaku: must run on main thread");
            return ExitCode::FAILURE;
        }
    };

    let config = load_config();
    let lanes = config.lanes.clamp(MIN_LANES, MAX_LANES) as usize;

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    // socket listener を別スレッドで起動。
    // パネルを作る前に socket を確保することで、二重起動の負け側が UI を出さずに即終了できる。
    let (tx, rx) = mpsc::channel::<Payload>();
    let socket_path_buf = socket_path(screen);
    match start_listener(socket_path_buf.clone(), tx) {
        Ok(()) => eprintln!("danmaku: listening on {}", socket_path_buf.display()),
        Err(e) => {
            eprintln!("danmaku: failed to start listener: {e}");
            return ExitCode::FAILURE;
        }
    }

    // フェーズ3b 保留: 本来は NSScreen::screens()[screen] を選ぶが、開発環境が単一モニタで
    // 視認確認できないため mainScreen 固定。socket パス / spawn への screen 配線は 3a で実施済み。
    let _ = screen; // 描画先選択ではまだ未使用（socket パスにのみ反映済み）
    let display = match NSScreen::mainScreen(mtm) {
        Some(s) => s,
        None => {
            eprintln!("danmaku: no main screen");
            return ExitCode::FAILURE;
        }
    };
    let screen_frame = display.frame();
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

    let panel: Retained<NSPanel> = {
        let alloc = NSPanel::alloc(mtm);
        NSPanel::initWithContentRect_styleMask_backing_defer(
            alloc,
            panel_rect,
            NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
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

    let bg = if config.debug_background {
        let (r, g, b, a) = BACKGROUND_TINT;
        NSColor::colorWithCalibratedRed_green_blue_alpha(r, g, b, a)
    } else {
        NSColor::clearColor()
    };
    panel.setBackgroundColor(Some(&bg));

    // contentView を layer-hosting にして root layer を取得
    let Some(content) = panel.contentView() else {
        eprintln!("danmaku: panel has no contentView");
        return ExitCode::FAILURE;
    };
    content.setWantsLayer(true);
    let Some(root_layer) = content.layer() else {
        eprintln!("danmaku: contentView has no layer");
        return ExitCode::FAILURE;
    };

    panel.orderFrontRegardless();

    // メインスレッドの状態
    let state = Rc::new(RefCell::new(DanmakuState::new(lanes)));
    let content_size = content.frame().size;
    let contents_scale = display.backingScaleFactor();
    // フォントはレーン高さに連動させる (Linux と同じ考え方)。
    let font_size = (content_size.height / lanes as f64) * FONT_LANE_RATIO;
    let font = NSFont::boldSystemFontOfSize(font_size);

    // NSTimer で channel を drain + 期限切れ弾 cleanup
    schedule_tick(
        Rc::clone(&state),
        root_layer,
        rx,
        content_size,
        contents_scale,
        font,
        font_size,
    );

    // アイドル自動終了は専用タイマーに分離する。
    // 「タイマー機構（周期・ライフサイクル）」と「アイドル検出（ドメイン判定）」は別責務であり、
    // 描画/ドレインの tick に相乗りさせない (tick は活動がある時に回るもの、検出は活動が無いことを見るもの)。
    // idle_timeout_min == 0 のときはタイマーを作らない = 「自動終了しない」を構造で表現する。
    if config.idle_timeout_min > 0 {
        schedule_idle_check(state, config.idle_timeout_min);
    }

    eprintln!(
        "danmaku: serving ({}x{})",
        panel_rect.size.width as i64, panel_rect.size.height as i64
    );

    app.run();
    ExitCode::SUCCESS
}

fn schedule_tick(
    state: Rc<RefCell<DanmakuState>>,
    root_layer: Retained<CALayer>,
    rx: Receiver<Payload>,
    content_size: NSSize,
    contents_scale: f64,
    font: Retained<NSFont>,
    font_size: f64,
) {
    // NSTimer の block は main thread でのみ実行されるので、Rc / Receiver を closure に move して問題ない
    let block = RcBlock::new(move |_timer: std::ptr::NonNull<NSTimer>| {
        // 受信した全ペイロードを drain
        while let Ok(payload) = rx.try_recv() {
            let mut st = state.borrow_mut();
            st.last_activity = unsafe { CACurrentMediaTime() };
            spawn_messages(
                &payload.messages,
                &mut st,
                &root_layer,
                content_size,
                contents_scale,
                &font,
                font_size,
            );
        }
        // 期限切れ弾を superlayer から除去
        let now = unsafe { CACurrentMediaTime() };
        let mut st = state.borrow_mut();
        st.bullets.retain(|b| {
            if b.expire_time < now {
                b.layer.removeFromSuperlayer();
                false
            } else {
                true
            }
        });
    });
    unsafe {
        NSTimer::scheduledTimerWithTimeInterval_repeats_block(TICK_INTERVAL_SEC, true, &block);
    }
}

// アイドル自動終了専用タイマー。
// last_activity から idle_timeout_min 分を過ぎていたらプロセスを終了する。
// 周期 (IDLE_CHECK_INTERVAL_SEC) もライフサイクルもこのタイマー自身が所有し、tick には依存しない。
fn schedule_idle_check(state: Rc<RefCell<DanmakuState>>, idle_timeout_min: u64) {
    let timeout_sec = idle_timeout_min as f64 * 60.0;
    let block = RcBlock::new(move |_timer: std::ptr::NonNull<NSTimer>| {
        let now = unsafe { CACurrentMediaTime() };
        let last = state.borrow().last_activity;
        if now - last >= timeout_sec {
            eprintln!("danmaku: idle for {idle_timeout_min} min, shutting down");
            std::process::exit(0);
        }
    });
    unsafe {
        NSTimer::scheduledTimerWithTimeInterval_repeats_block(
            IDLE_CHECK_INTERVAL_SEC,
            true,
            &block,
        );
    }
}

// ============================================================================
// socket listener
// ============================================================================

fn start_listener(path: PathBuf, tx: mpsc::Sender<Payload>) -> Result<(), String> {
    ensure_socket_available(&path)?;
    let listener = UnixListener::bind(&path)
        .map_err(|e| format!("bind {}: {e}", path.display()))?;
    thread::spawn(move || {
        for accepted in listener.incoming() {
            match accepted {
                Ok(stream) => {
                    let tx = tx.clone();
                    thread::spawn(move || handle_connection(stream, tx));
                }
                Err(e) => {
                    eprintln!("danmaku: accept failed: {e}");
                }
            }
        }
    });
    Ok(())
}

fn handle_connection(stream: UnixStream, tx: mpsc::Sender<Payload>) {
    use std::io::{BufRead, BufReader};
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let Ok(line) = line else {
            return;
        };
        match serde_json::from_str::<Payload>(&line) {
            Ok(payload) => {
                if tx.send(payload).is_err() {
                    return; // メイン側が落ちている
                }
            }
            Err(e) => {
                eprintln!("danmaku: JSON parse failed: {e}; line={line:?}");
            }
        }
    }
}

fn ensure_socket_available(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    match UnixStream::connect(path) {
        Ok(_) => Err(format!(
            "socket {} is already in use by another process",
            path.display()
        )),
        Err(_) => std::fs::remove_file(path)
            .map_err(|e| format!("failed to unlink stale socket {}: {e}", path.display())),
    }
}

fn socket_path(screen: u32) -> PathBuf {
    let dir = std::env::var_os("TMPDIR").unwrap_or_else(|| "/tmp".into());
    PathBuf::from(dir).join(format!("danmaku-{screen}.sock"))
}

// ============================================================================
// 弾 spawn / 描画
// ============================================================================

fn spawn_messages(
    messages: &[String],
    state: &mut DanmakuState,
    root_layer: &CALayer,
    content_size: NSSize,
    contents_scale: f64,
    font: &NSFont,
    font_size: f64,
) {
    let mut rng = rand::rng();
    let lanes = state.lanes;
    let now = unsafe { CACurrentMediaTime() };
    for msg in messages {
        let free: Vec<usize> = (0..lanes)
            .filter(|&i| {
                state.last_spawn_at[i]
                    .map(|t| now - t >= SPAWN_GAP_SEC)
                    .unwrap_or(true)
            })
            .collect();
        let lane = if let Some(&l) = free.choose(&mut rng) {
            l
        } else {
            let l = rng.random_range(0..lanes);
            eprintln!("danmaku: no free lane; overlapping on lane {l}: {msg:?}");
            l
        };
        let speed_factor = 1.0 + rng.random_range(-SPEED_JITTER..SPEED_JITTER);
        let speed = BASE_SPEED * speed_factor;
        let stagger = rng.random_range(0.0..PAYLOAD_STAGGER_MAX_SEC);
        let begin_time = now + stagger;
        state.last_spawn_at[lane] = Some(begin_time);

        let (layer, duration) = create_bullet_layer(
            msg,
            lane,
            lanes,
            speed,
            begin_time,
            root_layer,
            content_size,
            contents_scale,
            font,
            font_size,
        );
        state.bullets.push(Bullet {
            layer,
            expire_time: begin_time + duration,
        });
    }
}

fn create_bullet_layer(
    text: &str,
    lane: usize,
    lanes: usize,
    speed: f64,
    begin_time: f64,
    root_layer: &CALayer,
    content_size: NSSize,
    contents_scale: f64,
    font: &NSFont,
    font_size: f64,
) -> (Retained<CATextLayer>, f64) {
    let ns_text = NSString::from_str(text);
    let text_size = measure_text(&ns_text, font);
    let text_w = text_size.width.max(1.0);
    let text_h = text_size.height.max(font_size);

    let text_layer: Retained<CATextLayer> = CATextLayer::new();
    unsafe {
        text_layer.setString(Some(&*ns_text));
    }
    {
        let cf_font: &CFType = unsafe { &*(font as *const NSFont as *const CFType) };
        unsafe { text_layer.setFont(Some(cf_font)) };
    }
    text_layer.setFontSize(font_size);
    text_layer.setForegroundColor(Some(&NSColor::whiteColor().CGColor()));
    text_layer.setContentsScale(contents_scale);
    text_layer.setBounds(NSRect {
        origin: NSPoint { x: 0.0, y: 0.0 },
        size: NSSize {
            width: text_w,
            height: text_h,
        },
    });

    let y = lane_y(lane, content_size.height, lanes);
    let start_x = content_size.width + text_w / 2.0;
    let end_x = -text_w / 2.0;
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
    text_layer.addAnimation_forKey(&anim, Some(&NSString::from_str("scroll")));

    (text_layer, duration)
}

fn lane_y(lane: usize, height: f64, lanes: usize) -> f64 {
    // 内側マージン無し、全高を lanes で等分し各レーンの中央を返す
    let slot_h = height / lanes as f64;
    slot_h * (lane as f64 + 0.5)
}

fn measure_text(text: &NSString, font: &NSFont) -> NSSize {
    let keys: [&NSString; 1] = [unsafe { NSFontAttributeName }];
    let values: [&objc2::runtime::AnyObject; 1] =
        [font as &NSFont as &objc2::runtime::AnyObject];
    let attrs = NSDictionary::from_slices(&keys, &values);
    unsafe { text.sizeWithAttributes(Some(&attrs)) }
}

// CACurrentMediaTime: CABasicAnimation.beginTime と同じ時間軸 (mach_absolute_time ベース)
unsafe extern "C" {
    fn CACurrentMediaTime() -> f64;
}
