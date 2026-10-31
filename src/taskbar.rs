// prev / play-pause / next buttons in the taskbar thumbnail (windows only)
use crate::shell::Cmd;
use eframe::egui::Context;
use std::{cell::Cell, ffi::c_void, sync::mpsc::Sender};
use windows::{
    Win32::{Foundation::*, Graphics::Gdi::*, System::Com::*, UI::{Shell::*, WindowsAndMessaging::*}},
    core::w,
};

struct State {
    list: ITaskbarList3,
    icons: [HICON; 4], // prev, play, pause, next
    msg: u32,
    tx: Sender<Cmd>,
    ctx: Context,
    playing: Cell<bool>,
}

pub struct Taskbar {
    hwnd: HWND,
    st: &'static State,
}

fn buttons(st: &State) -> [THUMBBUTTON; 3] {
    let playing = st.playing.get();
    let mk = |id: u32, icon: HICON, tip: &str| {
        let mut b = THUMBBUTTON { dwMask: THB_ICON | THB_TOOLTIP | THB_FLAGS, iId: id, iBitmap: 0, hIcon: icon, szTip: [0; 260], dwFlags: THBF_ENABLED };
        for (i, u) in tip.encode_utf16().take(259).enumerate() {
            b.szTip[i] = u;
        }
        b
    };
    [
        mk(0, st.icons[0], "Previous"),
        mk(1, st.icons[if playing { 2 } else { 1 }], if playing { "Pause" } else { "Play" }),
        mk(2, st.icons[3], "Next"),
    ]
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM, _id: usize, data: usize) -> LRESULT {
    let st = unsafe { &*(data as *const State) };
    if msg == st.msg {
        let _ = unsafe { st.list.ThumbBarAddButtons(hwnd, &buttons(st)) };
    } else if msg == WM_COMMAND && (w.0 >> 16) as u32 == THBN_CLICKED {
        let _ = st.tx.send(match w.0 & 0xffff {
            0 => Cmd::Prev,
            1 => Cmd::Toggle,
            _ => Cmd::Next,
        });
        st.ctx.request_repaint();
        return LRESULT(0);
    }
    unsafe { DefSubclassProc(hwnd, msg, w, l) }
}

// 16x16 white glyphs drawn by hand so no need files
fn icon(kind: usize) -> HICON {
    let tri = |p: (f32, f32), a: (f32, f32), b: (f32, f32), c: (f32, f32)| {
        let s = |p: (f32, f32), q: (f32, f32), r: (f32, f32)| (p.0 - r.0) * (q.1 - r.1) - (q.0 - r.0) * (p.1 - r.1);
        let (d1, d2, d3) = (s(p, a, b), s(p, b, c), s(p, c, a));
        !((d1 < 0. || d2 < 0. || d3 < 0.) && (d1 > 0. || d2 > 0. || d3 > 0.))
    };
    let bar = |p: (f32, f32), x0: f32, x1: f32| p.0 >= x0 && p.0 <= x1 && p.1 >= 3. && p.1 <= 13.;
    let mut px = [0u32; 256];
    for (i, v) in px.iter_mut().enumerate() {
        let p = ((i % 16) as f32 + 0.5, (i / 16) as f32 + 0.5);
        let on = match kind {
            0 => bar(p, 3., 5.) || tri(p, (13., 3.), (13., 13.), (5.5, 8.)),
            1 => tri(p, (4., 2.5), (4., 13.5), (13., 8.)),
            2 => bar(p, 3.5, 6.5) || bar(p, 9.5, 12.5),
            _ => bar(p, 11., 13.) || tri(p, (3., 3.), (3., 13.), (10.5, 8.)),
        };
        *v = if on { 0xFFFF_FFFF } else { 0 };
    }
    let mask = [0u8; 32];
    unsafe {
        let color = CreateBitmap(16, 16, 1, 32, Some(px.as_ptr() as *const c_void));
        let m = CreateBitmap(16, 16, 1, 1, Some(mask.as_ptr() as *const c_void));
        let h = CreateIconIndirect(&ICONINFO { fIcon: true.into(), xHotspot: 0, yHotspot: 0, hbmMask: m, hbmColor: color }).unwrap_or_default();
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(m.into());
        h
    }
}

impl Taskbar {
    pub fn new(hwnd: isize, tx: Sender<Cmd>, ctx: Context) -> Option<Self> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let list: ITaskbarList3 = CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER).ok()?;
            list.HrInit().ok()?;
            let msg = RegisterWindowMessageW(w!("TaskbarButtonCreated"));
            let st: &'static State = Box::leak(Box::new(State { list, icons: [0, 1, 2, 3].map(icon), msg, tx, ctx, playing: Cell::new(true) }));
            let hwnd = HWND(hwnd as *mut c_void);
            if !SetWindowSubclass(hwnd, Some(proc), 1, st as *const State as usize).as_bool() {
                return None;
            }
            let _ = st.list.ThumbBarAddButtons(hwnd, &buttons(st));
            Some(Taskbar { hwnd, st })
        }
    }

    pub fn set_playing(&self, playing: bool) {
        if self.st.playing.replace(playing) != playing {
            let _ = unsafe { self.st.list.ThumbBarUpdateButtons(self.hwnd, &buttons(self.st)) };
        }
    }
}
