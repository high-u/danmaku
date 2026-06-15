use std::cell::RefCell;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcCommand, ExitCode, Stdio};
use std::rc::Rc;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use gio::prelude::*;
use gtk4::cairo;
use gtk4::gdk;
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow};
use rand::seq::IteratorRandom;
use rand::RngExt;
use serde::{Deserialize, Serialize};

mod overlay_x11;

const APP_ID: &str = "io.github.danmaku.gui";

// デフォルト値。設定ファイル (~/.config/danmaku/config.toml) があれば上書きされる。
const DEFAULT_LANES: usize = 16;
const DEFAULT_IDLE_TIMEOUT_MIN: u64 = 30;
const MIN_LANES: u32 = 1;
const MAX_LANES: u32 = 128;
const DEFAULT_BASE_SPEED: f64 = 250.0; // px/sec
const SPEED_JITTER: f64 = 0.3; // ±30%
const SPAWN_GAP_SEC: f64 = 1.5; // 同レーンに新弾を出してよい最小間隔
const PAYLOAD_STAGGER_MS: u64 = 250; // 同一受信内メッセージの最大ずらし
const FONT_LANE_RATIO: f64 = 0.6; // フォント絶対高 / レーン高

#[derive(Parser, Debug)]
#[command(name = "danmaku", about = "Transparent danmaku overlay for Linux/X11.")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run the overlay (default when no subcommand is given).
    Serve {
        /// Display index (from `gdk::Display::monitors()`).
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

#[derive(Debug, Serialize, Deserialize)]
struct Payload {
    messages: Vec<String>,
}

// 設定ファイル。正常にパースできた場合のみ採用し、欠けたキーは個別のデフォルト値で
// 埋める。未知のキーは無視。ファイルが無い/読めない/壊れている場合は全項目デフォルト。
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
    Some(PathBuf::from(home).join(".config").join("danmaku").join("config.toml"))
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

struct Bullet {
    text: String,
    lane: usize,
    speed: f64,
    start_time: Instant,
}

struct DanmakuState {
    screen: u32,
    lanes: usize,
    base_speed: f64,
    bullets: Vec<Bullet>,
    last_spawn_at: Vec<Option<Instant>>, // length == lanes
    last_activity: Instant,              // 最後に弾幕を受信した時刻 (アイドル終了判定用)
}

impl DanmakuState {
    fn new(screen: u32, lanes: usize) -> Self {
        Self {
            screen,
            lanes,
            base_speed: DEFAULT_BASE_SPEED,
            bullets: Vec::new(),
            last_spawn_at: vec![None; lanes],
            last_activity: Instant::now(),
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Serve { screen: 0 }) {
        Command::Serve { screen } => run_serve(screen),
        Command::Send { screen, messages } => run_send(screen, Payload { messages }),
    }
}

fn run_serve(screen: u32) -> ExitCode {
    let config = load_config();
    let lanes = config.lanes.clamp(MIN_LANES, MAX_LANES) as usize;
    let idle_timeout_min = config.idle_timeout_min;
    let debug_background = config.debug_background;

    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(move |app| {
        build_ui(app, screen, lanes, idle_timeout_min, debug_background)
    });
    // GTK に引数を解釈させない（clap で消費済み）
    let code = app.run_with_args::<&str>(&[]);
    ExitCode::from(u8::from(code))
}

fn run_send(screen: u32, payload: Payload) -> ExitCode {
    let count = payload.messages.len();
    let mut line = match serde_json::to_string(&payload) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("danmaku: failed to serialize payload: {e}");
            return ExitCode::FAILURE;
        }
    };
    line.push('\n');

    let path = match socket_path(screen) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("danmaku: {e}");
            return ExitCode::FAILURE;
        }
    };
    // serve が居れば即送信。居なければ自動起動して socket が立つのを待つ。
    let mut stream = match UnixStream::connect(&path) {
        Ok(s) => s,
        Err(_) => {
            if let Err(e) = spawn_serve(screen) {
                eprintln!("danmaku: failed to launch serve: {e}");
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
    println!("sent {count} message(s) to screen {screen}");
    ExitCode::SUCCESS
}

// 自分自身を `serve --screen N` として、親から切り離した新セッションで起動する。
// 二重起動は先勝ち: 2 つ目の serve は socket 使用中で自滅するため害はない。
fn spawn_serve(screen: u32) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let mut cmd = ProcCommand::new(exe);
    cmd.arg("serve")
        .arg("--screen")
        .arg(screen.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // setsid で制御端末から切り離し、send 終了後も生き残れるようにする。
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

// serve 起動直後の socket が listen 可能になるまで接続を試行する。
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
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

fn build_ui(
    app: &Application,
    screen: u32,
    lanes: usize,
    idle_timeout_min: u64,
    debug_background: bool,
) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("danmaku")
        .decorated(false)
        .resizable(false)
        .build();

    // 背景。通常は完全透明。debug_background のときだけ領域確認用の薄青を敷く。
    let css = gtk4::CssProvider::new();
    let window_bg = if debug_background {
        "rgba(100, 160, 220, 0.08)"
    } else {
        "transparent"
    };
    css.load_from_string(&format!(
        "window {{ background: {window_bg}; }} window > * {{ background: transparent; }}"
    ));
    gtk4::style_context_add_provider_for_display(
        &gdk::Display::default().expect("no display"),
        &css,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let state = Rc::new(RefCell::new(DanmakuState::new(screen, lanes)));

    let drawing = gtk4::DrawingArea::new();
    drawing.set_hexpand(true);
    drawing.set_vexpand(true);

    let state_for_draw = state.clone();
    drawing.set_draw_func(move |area, cr, w, h| {
        draw_bullets(&state_for_draw.borrow(), area, cr, w, h);
    });

    window.set_child(Some(&drawing));

    let monitor = match monitor_for_screen(screen) {
        Some(m) => m,
        None => {
            eprintln!("danmaku: monitor #{screen} not found; aborting");
            app.quit();
            return;
        }
    };
    let geom = monitor.geometry();
    let target_w = geom.width();
    let target_h = (geom.height() as f64 * 0.75) as i32;
    window.set_default_size(target_w, target_h);

    window.connect_realize(|win| {
        let Some(surface) = win.surface() else { return };
        let region = cairo::Region::create();
        surface.set_input_region(Some(&region));
        overlay_x11::declare_overlay_states(&surface);
    });

    window.connect_map(move |win| {
        let Some(surface) = win.surface() else { return };
        overlay_x11::reassert_overlay_states(&surface);
        move_to_monitor_center(&surface, screen);
    });

    let state_for_tick = state.clone();
    drawing.add_tick_callback(move |area, _clock| {
        // 画面から完全に外れた弾を除去
        let w = area.width() as f64;
        let mut st = state_for_tick.borrow_mut();
        let now = Instant::now();
        st.bullets.retain(|b| {
            if b.start_time > now {
                return true;
            }
            let elapsed = now.duration_since(b.start_time).as_secs_f64();
            // text 幅は知らないので保守的に 2000px 進むまで保持
            elapsed * b.speed < (w + 2000.0)
        });
        drop(st);
        area.queue_draw();
        glib::ControlFlow::Continue
    });

    window.present();

    // socket listener を起動
    match start_socket_listener(state.clone()) {
        Ok(path) => eprintln!("danmaku: listening on {}", path.display()),
        Err(e) => {
            eprintln!("danmaku: failed to start socket listener: {e}");
            app.quit();
        }
    }

    // アイドル終了: 最終弾幕から idle_timeout_min 分を過ぎたら自動終了。0 で無効。
    if idle_timeout_min > 0 {
        let timeout = Duration::from_secs(idle_timeout_min * 60);
        let state_for_idle = state.clone();
        let app_for_idle = app.clone();
        glib::timeout_add_seconds_local(30, move || {
            let idle = state_for_idle.borrow().last_activity.elapsed();
            if idle >= timeout {
                eprintln!(
                    "danmaku: idle for {}min; shutting down",
                    idle_timeout_min
                );
                app_for_idle.quit();
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }
    // 注: マシン終了時の SIGTERM はデフォルト動作で即終了し、シャットダウンを
    // 妨げない。残った socket は tmpfs 上にあり再起動時に掃除されるため、
    // 明示的なシグナルハンドラは置かない。
}

fn draw_bullets(state: &DanmakuState, area: &gtk4::DrawingArea, cr: &cairo::Context, w: i32, h: i32) {
    cr.set_operator(cairo::Operator::Clear);
    cr.paint().ok();
    cr.set_operator(cairo::Operator::Over);

    let now = Instant::now();
    let w_f = w as f64;
    let h_f = h as f64;
    let lanes = state.lanes;

    let lane_h = h_f / lanes as f64;
    let font_px = lane_h * FONT_LANE_RATIO;

    for bullet in &state.bullets {
        if bullet.start_time > now {
            continue;
        }
        let elapsed = now.duration_since(bullet.start_time).as_secs_f64();

        let layout = area.create_pango_layout(Some(&bullet.text));
        let mut font = pango::FontDescription::from_string("Sans Bold");
        font.set_absolute_size(font_px * pango::SCALE as f64);
        layout.set_font_description(Some(&font));

        let (ink, _logical) = layout.pixel_extents();
        let lane_center = lane_y(bullet.lane, h_f, lanes) + lane_h / 2.0;
        let x = w_f - elapsed * bullet.speed;
        // show_layout の y はレイアウト原点。ink rect の中心が lane_center に来るよう逆算。
        let y = lane_center - ink.y() as f64 - ink.height() as f64 / 2.0;

        cr.move_to(x, y);
        pangocairo::functions::layout_path(cr, &layout);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.85);
        cr.set_line_width(4.0);
        cr.stroke().ok();

        cr.move_to(x, y);
        cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
        pangocairo::functions::show_layout(cr, &layout);
    }
}

fn lane_y(lane: usize, h: f64, lanes: usize) -> f64 {
    (lane as f64) * (h / lanes as f64)
}

fn spawn_messages(state: &mut DanmakuState, messages: &[String]) {
    let mut rng = rand::rng();
    let now = Instant::now();
    for msg in messages {
        let free: Vec<usize> = (0..state.lanes)
            .filter(|&i| {
                state.last_spawn_at[i]
                    .map(|t| now.duration_since(t).as_secs_f64() >= SPAWN_GAP_SEC)
                    .unwrap_or(true)
            })
            .collect();
        let lane = if let Some(&l) = free.iter().choose(&mut rng) {
            l
        } else {
            let l = rng.random_range(0..state.lanes);
            eprintln!(
                "danmaku: no free lane; overlapping on lane {l}: {msg:?}"
            );
            l
        };
        let speed_factor = 1.0 + rng.random_range(-SPEED_JITTER..SPEED_JITTER);
        let speed = state.base_speed * speed_factor;
        let stagger_ms = rng.random_range(0..PAYLOAD_STAGGER_MS);
        let start_time = now + Duration::from_millis(stagger_ms);
        state.last_spawn_at[lane] = Some(start_time);
        state.bullets.push(Bullet {
            text: msg.clone(),
            lane,
            speed,
            start_time,
        });
    }
}

fn socket_path(screen: u32) -> Result<PathBuf, String> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .ok_or_else(|| "XDG_RUNTIME_DIR is not set".to_string())?;
    Ok(PathBuf::from(dir).join(format!("danmaku-{screen}.sock")))
}

fn ensure_socket_available(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    // 既存ファイルが生きた socket か確認
    match UnixStream::connect(path) {
        Ok(_) => Err(format!(
            "socket {} is already in use by another process",
            path.display()
        )),
        Err(_) => {
            // 死んでいる → unlink
            std::fs::remove_file(path)
                .map_err(|e| format!("failed to unlink stale socket {}: {e}", path.display()))
        }
    }
}

fn start_socket_listener(state: Rc<RefCell<DanmakuState>>) -> Result<PathBuf, String> {
    let screen = state.borrow().screen;
    let path = socket_path(screen)?;
    ensure_socket_available(&path)?;

    let listener = gio::SocketListener::new();
    let address = gio::UnixSocketAddress::new(&path);
    listener
        .add_address(
            &address,
            gio::SocketType::Stream,
            gio::SocketProtocol::Default,
            None::<&glib::Object>,
        )
        .map_err(|e| format!("add_address failed: {e}"))?;

    let listener_clone = listener.clone();
    glib::MainContext::default().spawn_local(async move {
        loop {
            match listener_clone.accept_future().await {
                Ok((conn, _src)) => {
                    let state = state.clone();
                    glib::MainContext::default().spawn_local(async move {
                        handle_connection(conn, state).await;
                    });
                }
                Err(e) => {
                    eprintln!("danmaku: accept failed: {e}");
                    break;
                }
            }
        }
    });

    Ok(path)
}

async fn handle_connection(conn: gio::SocketConnection, state: Rc<RefCell<DanmakuState>>) {
    let input = conn.input_stream();
    let reader = gio::DataInputStream::new(&input);
    loop {
        match reader.read_line_future(glib::Priority::default()).await {
            Ok(Some(line)) => {
                let line_str = String::from_utf8_lossy(&line).to_string();
                process_line(&line_str, &state);
            }
            Ok(None) => break, // EOF
            Err(e) => {
                eprintln!("danmaku: read failed: {e}");
                break;
            }
        }
    }
}

fn process_line(line: &str, state: &Rc<RefCell<DanmakuState>>) {
    let payload: Payload = match serde_json::from_str(line) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("danmaku: JSON parse failed: {e}; line={line:?}");
            return;
        }
    };
    let mut st = state.borrow_mut();
    spawn_messages(&mut st, &payload.messages);
    st.last_activity = Instant::now();
}

// 指定された screen 番号 (gdk::Display::monitors() のインデックス) のモニタを取得する。
fn monitor_for_screen(screen: u32) -> Option<gdk::Monitor> {
    gdk::Display::default()?
        .monitors()
        .item(screen)
        .and_then(|o| o.downcast::<gdk::Monitor>().ok())
}

// マップ後にウィンドウをモニタ中央 (縦方向) へ移動する。
fn move_to_monitor_center(surface: &gdk::Surface, screen: u32) {
    let Some((xdisplay, xid)) = overlay_x11::x11_handles(surface) else {
        return;
    };
    let Some(monitor) = monitor_for_screen(screen) else {
        return;
    };
    let geom = monitor.geometry();
    let h = (geom.height() as f64 * 0.75) as i32;
    let x = geom.x();
    let y = geom.y() + (geom.height() - h) / 2;
    unsafe {
        x11::xlib::XMoveWindow(xdisplay, xid, x, y);
    }
}
