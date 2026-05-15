use gtk4::cairo;
use gtk4::gdk;
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow};
use std::time::Instant;

const APP_ID: &str = "io.github.danmaku.gui";

fn main() -> glib::ExitCode {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("danmaku")
        .decorated(false)
        .resizable(false)
        .build();

    // 背景を透明に
    let css = gtk4::CssProvider::new();
    css.load_from_string("window, window > * { background: transparent; }");
    gtk4::style_context_add_provider_for_display(
        &gdk::Display::default().expect("no display"),
        &css,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let drawing = gtk4::DrawingArea::new();
    drawing.set_hexpand(true);
    drawing.set_vexpand(true);

    let start = Instant::now();
    let text = "こんにちは弾幕です — Phase1 透過オーバーレイ実証".to_string();

    drawing.set_draw_func(move |area, cr, w, h| {
        // 透明クリア
        cr.set_operator(cairo::Operator::Clear);
        cr.paint().ok();
        cr.set_operator(cairo::Operator::Over);

        // Pango でレイアウトを組む（日本語対応）
        let layout = area.create_pango_layout(Some(&text));
        let mut font = pango::FontDescription::from_string("Sans Bold 36");
        font.set_absolute_size(36.0 * pango::SCALE as f64);
        layout.set_font_description(Some(&font));

        let (text_w_pango, _) = layout.size();
        let text_w = text_w_pango as f64 / pango::SCALE as f64;

        let elapsed = start.elapsed().as_secs_f64();
        let speed = 250.0; // px/sec
        let total = w as f64 + text_w;
        let progress = (elapsed * speed) % total;
        let x = w as f64 - progress;
        let y = (h as f64) * 0.25;

        // 縁取り（読みやすさ）
        cr.move_to(x, y);
        pangocairo::functions::layout_path(cr, &layout);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.85);
        cr.set_line_width(4.0);
        cr.stroke().ok();

        // 本体
        cr.move_to(x, y);
        cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
        pangocairo::functions::show_layout(cr, &layout);
    });

    window.set_child(Some(&drawing));

    // モニタサイズを取得してウィンドウを画面いっぱいに
    if let Some(display) = gdk::Display::default() {
        let monitors = display.monitors();
        if let Some(monitor) = monitors.item(0).and_then(|o| o.downcast::<gdk::Monitor>().ok()) {
            let geom = monitor.geometry();
            window.set_default_size(geom.width(), geom.height());
        }
    }
    window.fullscreen();

    window.connect_realize(|win| {
        let Some(surface) = win.surface() else { return };

        // クリックスルー: 空 input region
        let region = cairo::Region::create();
        surface.set_input_region(Some(&region));

        // マップ前の初期 _NET_WM_STATE を property で書く
        set_x11_initial_wm_state(&surface);

        // マップ後に ClientMessage で ABOVE を明示的に ADD する
        // (EWMH 仕様: マップ済みウィンドウの state 変更はルートへの ClientMessage)
        // idle で main loop の手すきに走らせれば、その時点でマップ済み
        let surface_clone = surface.clone();
        glib::idle_add_local_once(move || {
            send_x11_state_change_above(&surface_clone);
        });
    });

    let drawing_clone = drawing.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
        drawing_clone.queue_draw();
        glib::ControlFlow::Continue
    });

    window.present();
}

fn x11_handles(surface: &gdk::Surface) -> Option<(*mut x11::xlib::Display, x11::xlib::Window)> {
    use gdk4_x11::X11Display;
    use glib::translate::ToGlibPtr;

    let x11_surface = surface.downcast_ref::<gdk4_x11::X11Surface>()?;
    let x11_display = x11_surface.display().downcast_ref::<X11Display>().cloned()?;
    unsafe {
        let xdisplay =
            gdk4_x11::ffi::gdk_x11_display_get_xdisplay(x11_display.to_glib_none().0)
                as *mut x11::xlib::Display;
        let xid = gdk4_x11::ffi::gdk_x11_surface_get_xid(x11_surface.to_glib_none().0)
            as x11::xlib::Window;
        Some((xdisplay, xid))
    }
}

fn set_x11_initial_wm_state(surface: &gdk::Surface) {
    let Some((xdisplay, xid)) = x11_handles(surface) else {
        eprintln!("not an X11 surface/display");
        return;
    };
    unsafe {
        let wm_state = atom(xdisplay, "_NET_WM_STATE");
        let above = atom(xdisplay, "_NET_WM_STATE_ABOVE");
        let sticky = atom(xdisplay, "_NET_WM_STATE_STICKY");
        let skip_taskbar = atom(xdisplay, "_NET_WM_STATE_SKIP_TASKBAR");
        let skip_pager = atom(xdisplay, "_NET_WM_STATE_SKIP_PAGER");
        let fullscreen = atom(xdisplay, "_NET_WM_STATE_FULLSCREEN");

        let states = [above, sticky, skip_taskbar, skip_pager, fullscreen];
        x11::xlib::XChangeProperty(
            xdisplay,
            xid,
            wm_state,
            x11::xlib::XA_ATOM,
            32,
            x11::xlib::PropModeReplace,
            states.as_ptr() as *const u8,
            states.len() as i32,
        );
        x11::xlib::XFlush(xdisplay);
    }
}

// EWMH ar01s05: マップ済みウィンドウの _NET_WM_STATE 変更は
// ルートウィンドウ宛て ClientMessage を SubstructureRedirect/Notify Mask で送る。
fn send_x11_state_change_above(surface: &gdk::Surface) {
    let Some((xdisplay, xid)) = x11_handles(surface) else { return };
    unsafe {
        let wm_state = atom(xdisplay, "_NET_WM_STATE");
        let above = atom(xdisplay, "_NET_WM_STATE_ABOVE");
        let root = x11::xlib::XDefaultRootWindow(xdisplay);

        let mut data = x11::xlib::ClientMessageData::new();
        data.set_long(0, 1); // _NET_WM_STATE_ADD
        data.set_long(1, above as i64);
        data.set_long(2, 0);
        data.set_long(3, 1); // source: normal application
        data.set_long(4, 0);

        let mut event = x11::xlib::XEvent {
            client_message: x11::xlib::XClientMessageEvent {
                type_: x11::xlib::ClientMessage,
                serial: 0,
                send_event: 1,
                display: xdisplay,
                window: xid,
                message_type: wm_state,
                format: 32,
                data,
            },
        };

        x11::xlib::XSendEvent(
            xdisplay,
            root,
            0,
            x11::xlib::SubstructureRedirectMask | x11::xlib::SubstructureNotifyMask,
            &mut event,
        );
        x11::xlib::XFlush(xdisplay);
    }
}

unsafe fn atom(d: *mut x11::xlib::Display, name: &str) -> x11::xlib::Atom {
    let c = std::ffi::CString::new(name).unwrap();
    unsafe { x11::xlib::XInternAtom(d, c.as_ptr(), 0) }
}
