#[cfg(windows)]
use {eframe::egui::Context, std::time::Duration};
use std::sync::mpsc::{Receiver, channel};

#[cfg_attr(not(windows), allow(dead_code))]
pub enum Cmd { Prev, Play, Pause, Toggle, Next }

pub struct Shell {
    pub rx: Receiver<Cmd>,
    sent: Option<(String, bool)>,
    #[cfg(windows)]
    smtc: Option<souvlaki::MediaControls>,
    #[cfg(windows)]
    bar: Option<crate::taskbar::Taskbar>,
}

impl Shell {
    #[allow(unused_variables)]
    pub fn new(cc: &eframe::CreationContext) -> Self {
        let (tx, rx) = channel();
        #[cfg(windows)]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            let hwnd = match cc.window_handle().map(|h| h.as_raw()) {
                Ok(RawWindowHandle::Win32(h)) => h.hwnd.get(),
                _ => return Shell { rx, sent: None, smtc: None, bar: None },
            };
            let smtc = Self::media(hwnd, tx.clone(), cc.egui_ctx.clone());
            let bar = crate::taskbar::Taskbar::new(hwnd, tx, cc.egui_ctx.clone());
            return Shell { rx, sent: None, smtc, bar };
        }
        #[cfg(not(windows))]
        Shell { rx, sent: None }
    }

    #[cfg(windows)]
    fn media(hwnd: isize, tx: std::sync::mpsc::Sender<Cmd>, ctx: Context) -> Option<souvlaki::MediaControls> {
        use souvlaki::{MediaControlEvent as E, MediaControls, PlatformConfig};
        let mut m = MediaControls::new(PlatformConfig { dbus_name: "mewradio", display_name: "MewRadio", hwnd: Some(hwnd as *mut _) }).ok()?;
        m.attach(move |e| {
            let c = match e {
                E::Play => Cmd::Play,
                E::Pause | E::Stop => Cmd::Pause,
                E::Toggle => Cmd::Toggle,
                E::Next => Cmd::Next,
                E::Previous => Cmd::Prev,
                _ => return,
            };
            let _ = tx.send(c);
            ctx.request_repaint();
        })
        .ok()?;
        Some(m)
    }

    #[allow(unused_variables)]
    pub fn update(&mut self, title: &str, sub: &str, dur: f32, pos: f32, playing: bool) {
        if self.sent.as_ref().is_some_and(|(t, p)| t == title && *p == playing) {
            return;
        }
        self.sent = Some((title.into(), playing));
        #[cfg(windows)]
        {
            use souvlaki::{MediaMetadata, MediaPlayback, MediaPosition};
            if let Some(m) = &mut self.smtc {
                let _ = m.set_metadata(MediaMetadata { title: Some(title), artist: Some(sub), duration: Some(Duration::from_secs_f32(dur)), ..Default::default() });
                let progress = Some(MediaPosition(Duration::from_secs_f32(pos)));
                let _ = m.set_playback(if playing { MediaPlayback::Playing { progress } } else { MediaPlayback::Paused { progress } });
            }
            if let Some(b) = &self.bar {
                b.set_playing(playing);
            }
        }
    }
}
