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

const BACKGROUND_TINT: Option<(f64, f64, f64, f64)> = Some((0.39, 0.63, 0.86, 0.08));

const MAX_LINES: usize = 8;
const BASE_SPEED: f64 = 250.0; // px/sec
const SPEED_JITTER: f64 = 0.3; // ±30%
const SPAWN_GAP_SEC: f64 = 1.5; // 同レーンの最小再使用間隔
const PAYLOAD_STAGGER_MAX_SEC: f64 = 0.25; // 同一ペイロード内のずらし最大値
const FONT_SIZE: f64 = 48.0;
const TICK_INTERVAL_SEC: f64 = 0.1; // メインスレッド側のポーリング周期

// ============================================================================
// CLI
// ============================================================================

#[derive(Parser, Debug)]
#[command(name = "danmaku-gui", about = "macOS 用透過オーバーレイ弾幕表示 (serve / send 統合)")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// 常駐モード (引数なしと同義)。透過オーバーレイを表示し socket で送信を待つ。
    Serve,
    /// 常駐インスタンスに JSON ペイロードを送信して即終了。
    Send {
        /// 表示先ディスプレイ番号
        #[arg(long, default_value_t = 0)]
        screen: u32,
        /// 文字色 (常駐側の設定を上書き)
        #[arg(long)]
        color: Option<String>,
        /// 速度倍率 (常駐側の設定を上書き)
        #[arg(long)]
        speed: Option<f64>,
        /// フォントサイズ (常駐側の設定を上書き)
        #[arg(long)]
        size: Option<u32>,
        /// 表示する文字列 (1 個以上)
        #[arg(required = true)]
        messages: Vec<String>,
    },
}

// ============================================================================
// 共有 JSON ペイロード
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
struct Payload {
    screen: u32,
    messages: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    speed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    size: Option<u32>,
}

// ============================================================================
// entry
// ============================================================================

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => run_serve(),
        Command::Send {
            screen,
            color,
            speed,
            size,
            messages,
        } => run_send(Payload {
            screen,
            messages,
            color,
            speed,
            size,
        }),
    }
}

// ============================================================================
// send モード
// ============================================================================

fn run_send(payload: Payload) -> ExitCode {
    let mut line = match serde_json::to_string(&payload) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("danmaku-gui: failed to serialize payload: {e}");
            return ExitCode::FAILURE;
        }
    };
    line.push('\n');

    let path = socket_path();
    let mut stream = match UnixStream::connect(&path) {
        Ok(s) => s,
        Err(_) => {
            // PoC: serve 未起動なら自分自身を切り離して起動し、socket を待つ。
            eprintln!("danmaku-gui: serve not running, spawning (PoC)…");
            if let Err(e) = spawn_serve() {
                eprintln!("danmaku-gui: failed to spawn serve: {e}");
                return ExitCode::FAILURE;
            }
            match wait_for_socket(&path, Duration::from_secs(5)) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("danmaku-gui: {e}");
                    return ExitCode::FAILURE;
                }
            }
        }
    };
    if let Err(e) = stream.write_all(line.as_bytes()) {
        eprintln!("danmaku-gui: failed to write to {}: {e}", path.display());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

// PoC: 自分自身を `serve` として、親から切り離した新セッションで起動する。
// （Linux 版 spawn_serve の setsid 方式が macOS でも通用するかの検証用）
fn spawn_serve() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let mut cmd = ProcCommand::new(exe);
    cmd.arg("serve")
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
    bullets: Vec<Bullet>,
    last_spawn_at: Vec<Option<f64>>,
}

impl DanmakuState {
    fn new() -> Self {
        Self {
            bullets: Vec::new(),
            last_spawn_at: vec![None; MAX_LINES],
        }
    }
}

fn run_serve() -> ExitCode {
    let mtm = match MainThreadMarker::new() {
        Some(m) => m,
        None => {
            eprintln!("danmaku-gui: must run on main thread");
            return ExitCode::FAILURE;
        }
    };

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    // socket listener を別スレッドで起動。
    // パネルを作る前に socket を確保することで、二重起動の負け側が UI を出さずに即終了できる。
    let (tx, rx) = mpsc::channel::<Payload>();
    let socket_path_buf = socket_path();
    match start_listener(socket_path_buf.clone(), tx) {
        Ok(()) => eprintln!("danmaku-gui: listening on {}", socket_path_buf.display()),
        Err(e) => {
            eprintln!("danmaku-gui: failed to start listener: {e}");
            return ExitCode::FAILURE;
        }
    }

    let screen = match NSScreen::mainScreen(mtm) {
        Some(s) => s,
        None => {
            eprintln!("danmaku-gui: no main screen");
            return ExitCode::FAILURE;
        }
    };
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

    let bg = match BACKGROUND_TINT {
        Some((r, g, b, a)) => NSColor::colorWithCalibratedRed_green_blue_alpha(r, g, b, a),
        None => NSColor::clearColor(),
    };
    panel.setBackgroundColor(Some(&bg));

    // contentView を layer-hosting にして root layer を取得
    let Some(content) = panel.contentView() else {
        eprintln!("danmaku-gui: panel has no contentView");
        return ExitCode::FAILURE;
    };
    content.setWantsLayer(true);
    let Some(root_layer) = content.layer() else {
        eprintln!("danmaku-gui: contentView has no layer");
        return ExitCode::FAILURE;
    };

    panel.orderFrontRegardless();

    // メインスレッドの状態
    let state = Rc::new(RefCell::new(DanmakuState::new()));
    let content_size = content.frame().size;
    let contents_scale = screen.backingScaleFactor();
    let font = NSFont::boldSystemFontOfSize(FONT_SIZE);

    // NSTimer で channel を drain + 期限切れ弾 cleanup
    schedule_tick(state, root_layer, rx, content_size, contents_scale, font);

    eprintln!(
        "danmaku-gui: serving ({}x{})",
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
) {
    // NSTimer の block は main thread でのみ実行されるので、Rc / Receiver を closure に move して問題ない
    let block = RcBlock::new(move |_timer: std::ptr::NonNull<NSTimer>| {
        // 受信した全ペイロードを drain
        while let Ok(payload) = rx.try_recv() {
            let mut st = state.borrow_mut();
            spawn_messages(
                &payload.messages,
                &mut st,
                &root_layer,
                content_size,
                contents_scale,
                &font,
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
                    eprintln!("danmaku-gui: accept failed: {e}");
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
                eprintln!("danmaku-gui: JSON parse failed: {e}; line={line:?}");
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

fn socket_path() -> PathBuf {
    let dir = std::env::var_os("TMPDIR").unwrap_or_else(|| "/tmp".into());
    PathBuf::from(dir).join("danmaku.sock")
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
) {
    let mut rng = rand::rng();
    let now = unsafe { CACurrentMediaTime() };
    for msg in messages {
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
            eprintln!("danmaku-gui: no free lane; overlapping on lane {l}: {msg:?}");
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
            speed,
            begin_time,
            root_layer,
            content_size,
            contents_scale,
            font,
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
    speed: f64,
    begin_time: f64,
    root_layer: &CALayer,
    content_size: NSSize,
    contents_scale: f64,
    font: &NSFont,
) -> (Retained<CATextLayer>, f64) {
    let ns_text = NSString::from_str(text);
    let text_size = measure_text(&ns_text, font);
    let text_w = text_size.width.max(1.0);
    let text_h = text_size.height.max(FONT_SIZE);

    let text_layer: Retained<CATextLayer> = CATextLayer::new();
    unsafe {
        text_layer.setString(Some(&*ns_text));
    }
    {
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

    let y = lane_y(lane, content_size.height);
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

fn lane_y(lane: usize, height: f64) -> f64 {
    // Phase 7 方針: 内側マージン無し、全高を MAX_LINES で等分し各レーンの中央を返す
    let slot_h = height / MAX_LINES as f64;
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
