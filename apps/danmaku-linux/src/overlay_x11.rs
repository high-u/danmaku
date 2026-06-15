//! X11 オーバーレイの機構 (mechanism) を 1 か所に隔離するモジュール。
//!
//! 中核 (`main.rs`) はここへ「どんなオーバーレイにしたいか (意図)」だけを伝え、
//! EWMH の atom 操作や ClientMessage 送信といった X11 固有の手続きには触れない。
//!
//! ## なぜ高レイヤー (GTK4/GDK4) でなく生 xlib なのか
//!
//! GTK3 にあった `set_keep_above` / `set_skip_taskbar_hint` /
//! `set_skip_pager_hint` / `stick` は GTK4 で削除された。GDK4 の
//! `ToplevelState` は WM が返す状態を読む read-only で、これらを *要求する*
//! setter は存在しない (確認済み: gdk4 0.11)。よって `_NET_WM_STATE` を
//! 要求する手段は EWMH (生 xlib) しか残っていない。

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;

// オーバーレイとして望む _NET_WM_STATE 群。
// ABOVE: 最前面 / STICKY: 全ワークスペースに表示 /
// SKIP_TASKBAR・SKIP_PAGER: タスクバー・ページャの一覧に出さない。
const OVERLAY_STATES: [&str; 4] = [
    "_NET_WM_STATE_ABOVE",
    "_NET_WM_STATE_STICKY",
    "_NET_WM_STATE_SKIP_TASKBAR",
    "_NET_WM_STATE_SKIP_PAGER",
];

/// map 前に、オーバーレイとして望む WM 状態をウィンドウのプロパティへ宣言する。
/// EWMH では map 時に WM がこのプロパティを読む。realize で呼ぶ。
pub(crate) fn declare_overlay_states(surface: &gdk::Surface) {
    let Some((xdisplay, xid)) = x11_handles(surface) else {
        eprintln!("danmaku: not an X11 surface; cannot declare WM states");
        return;
    };
    unsafe {
        let wm_state = atom(xdisplay, "_NET_WM_STATE");
        let states: Vec<x11::xlib::Atom> =
            OVERLAY_STATES.iter().map(|n| atom(xdisplay, n)).collect();
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
    }
}

/// map 後に、望む WM 状態を改めて要求する。map で呼ぶ。
///
/// 一部の WM (Mutter 等) は map 時に初期プロパティをリセットするため、
/// map 後に EWMH ClientMessage (ar01s05) で再要求しないと状態が乗らない。
/// `declare_overlay_states` と同じ 4 状態すべてを再要求する。
pub(crate) fn reassert_overlay_states(surface: &gdk::Surface) {
    let Some((xdisplay, xid)) = x11_handles(surface) else {
        return;
    };
    unsafe {
        let wm_state = atom(xdisplay, "_NET_WM_STATE");
        let root = x11::xlib::XDefaultRootWindow(xdisplay);
        for name in OVERLAY_STATES {
            let st = atom(xdisplay, name);
            let mut data = x11::xlib::ClientMessageData::new();
            data.set_long(0, 1); // _NET_WM_STATE_ADD
            data.set_long(1, st as i64);
            data.set_long(2, 0);
            data.set_long(3, 1); // source: normal application
            data.set_long(4, 0);

            let mut event = x11::xlib::XEvent {
                client_message: x11::xlib::XClientMessageEvent {
                    type_: x11::xlib::ClientMessage,
                    serial: 0,
                    send_event: 0,
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
        }
    }
}

/// gdk Surface から xlib の Display ポインタとウィンドウ ID を取り出す。
/// X11 機構の内部実装詳細。他の X11 手続き (モニタ配置等) からも使う。
pub(crate) fn x11_handles(
    surface: &gdk::Surface,
) -> Option<(*mut x11::xlib::Display, x11::xlib::Window)> {
    use gdk4_x11::X11Display;
    use glib::translate::ToGlibPtr;

    let x11_surface = surface.downcast_ref::<gdk4_x11::X11Surface>()?;
    let x11_display = x11_surface.display().downcast_ref::<X11Display>().cloned()?;
    unsafe {
        let xdisplay = gdk4_x11::ffi::gdk_x11_display_get_xdisplay(x11_display.to_glib_none().0)
            as *mut x11::xlib::Display;
        let xid = gdk4_x11::ffi::gdk_x11_surface_get_xid(x11_surface.to_glib_none().0)
            as x11::xlib::Window;
        Some((xdisplay, xid))
    }
}

unsafe fn atom(d: *mut x11::xlib::Display, name: &str) -> x11::xlib::Atom {
    let c = std::ffi::CString::new(name).unwrap();
    unsafe { x11::xlib::XInternAtom(d, c.as_ptr(), 0) }
}
