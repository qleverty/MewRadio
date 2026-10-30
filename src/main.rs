mod audio;
mod catalog;
mod radio;
mod rng;
mod settings;

use audio::Audio;
use catalog::*;
use eframe::egui::*;
use radio::{Entry, Kind, Radio};
use settings::Presets;
use std::{collections::VecDeque, path::PathBuf, time::{Duration, Instant}};

const fn c(h: u32) -> Color32 {
    Color32::from_rgb((h >> 16) as u8, (h >> 8) as u8, h as u8)
}
const BG: Color32 = c(0x1d1d1d);
const P2: Color32 = c(0x2f2f2f);
const TX: Color32 = c(0xdcdcd8);
const MU: Color32 = c(0x8d8982);
const SEL: Color32 = c(0x3a3a3a);

fn col(k: Kind) -> Color32 {
    match k {
        Kind::Song => c(0xa08f4f),
        Kind::Wmew => c(0xb27a82),
        Kind::Intro => c(0x7d7399),
        Kind::Voice => c(0x628c63),
    }
}

fn mm(s: f32) -> String {
    format!("{}:{:02}", s as u32 / 60, s as u32 % 60)
}

fn fatal(m: &str) -> ! {
    let _ = std::fs::write("error_message.txt", m);
    #[cfg(windows)]
    {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn MessageBoxW(h: isize, text: *const u16, cap: *const u16, kind: u32) -> i32;
        }
        let w = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
        unsafe { MessageBoxW(0, w(m).as_ptr(), w("MewRadio").as_ptr(), 0x10010) };
    }
    eprintln!("{m}");
    std::process::exit(1)
}

fn row(ui: &mut Ui, e: Option<&Entry>, cur: bool, past: bool) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.), Sense::hover());
    let Some(e) = e else { return };
    if cur {
        ui.painter().rect_filled(r, 0., SEL);
    }
    let tc = if past { MU } else { TX };
    let mut l = ui.new_child(UiBuilder::new().max_rect(r.with_max_x(r.right() - 44.)).layout(Layout::left_to_right(Align::Center)));
    l.allocate_ui_with_layout(vec2(92., 22.), Layout::left_to_right(Align::Center), |ui| {
        ui.add(Label::new(RichText::new(e.sub).color(col(e.kind))).truncate())
    });
    l.add(Label::new(RichText::new(&e.title).color(tc)).truncate());
    let mut d = ui.new_child(UiBuilder::new().max_rect(r).layout(Layout::right_to_left(Align::Center)));
    d.label(RichText::new(mm(e.dur)).color(tc));
}

struct App {
    radio: Radio,
    pre: Presets,
    audio: Audio,
    hist: VecDeque<Entry>,
    cur: Entry,
    up: VecDeque<Entry>,
    playing: bool,
    rep: bool,
    queued: bool, // next track is already in the sink
    mix: [u32; 2],
    tab: usize,
    sub: usize,
    seek: Option<f32>,
    rename: Option<String>,
    msg: Option<(&'static str, Instant)>,
}

impl App {
    fn new(root: PathBuf) -> Self {
        let cat = Catalog::scan(&root.join("Audio"));
        let pre = Presets::load(&root);
        if cat.songs_on(&Default::default()) == 0 {
            fatal("No songs found in the Audio folder (Songs, WMEW)");
        }
        let audio = Audio::new().unwrap_or_else(|e| fatal(&format!("No audio device: {e}")));
        audio.sink.set_volume(pre.vol);
        let mut radio = Radio::new(cat, &pre.cfg);
        let cur = radio.next();
        audio.add(&cur.path);
        let mut a = App { radio, pre, audio, hist: VecDeque::new(), cur, up: VecDeque::new(), playing: true, rep: false, queued: false, mix: [0; 2], tab: 0, sub: 0, seek: None, rename: None, msg: None };
        a.reload();
        a
    }

    fn fill(&mut self) {
        while self.up.len() < 24 {
            let e = self.radio.next();
            self.up.push_back(e);
        }
    }

    fn advance(&mut self) {
        let n = self.up.pop_front().unwrap();
        self.hist.push_back(std::mem::replace(&mut self.cur, n));
        if self.hist.len() > 64 {
            self.hist.pop_front();
        }
        self.queued = false;
        self.fill();
    }

    fn play(&mut self) {
        self.audio.play(&self.cur.path, self.playing);
        self.queued = false;
    }

    // settings changed: rebuild the future (keeps the track queued)
    fn reload(&mut self) {
        if self.radio.cat.songs_on(&self.pre.cfg.off) == 0 {
            self.pre.cfg.off = Default::default();
        }
        let c = self.pre.cfg.chances;
        self.mix = [c[0], c[0] + c[1]];
        self.radio.apply(&self.pre.cfg);
        self.up.truncate(self.queued as usize);
        self.fill();
    }

    fn apply(&mut self) {
        self.pre.touch(true);
        self.reload();
    }

    fn warn(&mut self, m: &'static str) {
        self.msg = Some((m, Instant::now()));
    }

    fn tick(&mut self, ctx: &Context) {
        let n = self.audio.sink.len();
        if self.playing && (n == 0 || self.queued && n == 1) {
            if self.rep && n == 0 {
                self.audio.add(&self.cur.path);
            } else {
                self.advance();
                if n == 0 {
                    self.audio.add(&self.cur.path);
                }
            }
        } else if !self.queued && !self.rep && n == 1 && self.cur.dur - self.audio.sink.get_pos().as_secs_f32() < 3. {
            self.audio.add(&self.up[0].path);
            self.queued = true;
        }
        if self.pre.tick() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        if self.playing {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn queue(&mut self, ui: &mut Ui) {
        ui.strong("Queue");
        ui.spacing_mut().item_spacing.y = 0.;
        let n = (ui.available_height() / 22.).floor().max(5.) as usize;
        let ci = ((n as f32 * 0.4).round() as i32 - 1).max(0);
        for i in 0..n as i32 {
            let d = i - ci;
            if d < 0 {
                row(ui, self.hist.iter().rev().nth(-d as usize - 1), false, true);
            } else if d == 0 {
                row(ui, Some(&self.cur), true, false);
            } else {
                row(ui, self.up.get(d as usize - 1), false, false);
            }
        }
    }

    fn now(&mut self, ui: &mut Ui) {
        let (k, dur) = (self.cur.kind, self.cur.dur.max(0.1));
        let pos = self.audio.sink.get_pos().as_secs_f32();
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.visuals_mut().selection.bg_fill = col(k);
            ui.horizontal(|ui| {
                ui.colored_label(col(k), self.cur.sub);
                ui.add(Label::new(RichText::new(&self.cur.title).strong().size(18.)).truncate());
            });
            let mut p = self.seek.unwrap_or(pos).min(dur);
            ui.spacing_mut().slider_width = ui.available_width();
            let r = ui.add(Slider::new(&mut p, 0.0..=dur).show_value(false).trailing_fill(true));
            if r.dragged() {
                self.seek = Some(p);
            }
            if r.drag_stopped() {
                let _ = self.audio.sink.try_seek(Duration::from_secs_f32(p));
                self.seek = None;
            }
            ui.horizontal(|ui| {
                ui.label(mm(p));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.label(mm(dur)));
            });
            ui.horizontal(|ui| {
                if ui.button("Prev").clicked() {
                    if pos > 3. || self.hist.is_empty() {
                        self.play();
                    } else {
                        self.up.push_front(self.cur.clone());
                        self.cur = self.hist.pop_back().unwrap();
                        self.play();
                    }
                }
                if ui.button(if self.playing { "Pause" } else { "Play" }).clicked() {
                    self.playing = !self.playing;
                    if self.playing { self.audio.sink.play() } else { self.audio.sink.pause() }
                }
                if ui.button("Next").clicked() {
                    self.advance();
                    self.play();
                }
                ui.toggle_value(&mut self.rep, "Repeat");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().slider_width = 100.;
                    let mut v = self.pre.vol;
                    if ui.add(Slider::new(&mut v, 0.0..=1.0).show_value(false)).changed() {
                        self.pre.vol = v;
                        self.audio.sink.set_volume(v);
                        self.pre.touch(false);
                    }
                    ui.label("Volume");
                });
            });
        });
    }

    fn mix_panel(&mut self, ui: &mut Ui) {
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                for (i, (n, k)) in [("Songs", Kind::Song), ("WMEW", Kind::Wmew), ("Voicelines", Kind::Voice)].into_iter().enumerate() {
                    if i > 0 {
                        ui.label(RichText::new("|").color(MU));
                    }
                    ui.colored_label(col(k), n);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Reset").clicked() {
                        self.mix = [25, 46];
                        self.set_mix();
                    }
                });
            });
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.), Sense::hover());
            let x = |v: u32| r.left() + r.width() * v as f32 / 100.;
            let m = self.mix;
            for (a, b, k) in [(0, m[0], Kind::Song), (m[0], m[1], Kind::Wmew), (m[1], 100, Kind::Voice)] {
                let s = Rect::from_x_y_ranges(x(a)..=x(b), r.y_range());
                ui.painter().rect_filled(s, 0., col(k).gamma_multiply(0.8));
                if b - a >= 7 {
                    ui.painter().text(s.center(), Align2::CENTER_CENTER, format!("{}%", b - a), FontId::proportional(12.), BG);
                }
            }
            for i in 0..2 {
                let h = Rect::from_center_size(pos2(x(self.mix[i]), r.center().y), vec2(8., 34.));
                let resp = ui.interact(h, Id::new(("h", i)), Sense::drag());
                if let Some(p) = resp.interact_pointer_pos().filter(|_| resp.dragged()) {
                    let v = ((p.x - r.left()) / r.width() * 100.).round().clamp(0., 100.) as u32;
                    self.mix[i] = if i == 0 { v.min(self.mix[1]) } else { v.max(self.mix[0]).max(5) };
                }
                if resp.drag_stopped() {
                    self.set_mix();
                }
                let hc = if resp.hovered() || resp.dragged() { Color32::WHITE } else { TX };
                ui.painter().rect_filled(h.shrink2(vec2(1., 4.)), 0., hc);
            }
        });
    }

    fn set_mix(&mut self) {
        let m = self.mix;
        let new = [m[0], m[1] - m[0], 100 - m[1]];
        if new != self.pre.cfg.chances {
            self.pre.cfg.chances = new;
            self.apply();
        }
    }

    // (key, name, extra, duration) of what the library shows
    fn rows(&self) -> Vec<(String, String, String, f32)> {
        let cat = &self.radio.cat;
        if self.tab < 2 {
            cat.songs[self.tab].iter().map(|s| (s.t.file.clone(), s.t.name.clone(), format!("{}i/{}o", s.intros.len(), s.outros.len()), s.t.dur)).collect()
        } else {
            cat.voices[self.sub].iter().map(|t| (vkey(self.sub, t), t.name.clone(), String::new(), t.dur)).collect()
        }
    }

    fn keys(&self) -> Vec<String> {
        self.rows().into_iter().map(|r| r.0).collect()
    }

    fn set_off(&mut self, keys: Vec<String>, on: bool) {
        let off = &mut self.pre.cfg.off[self.tab];
        for k in keys {
            if on { off.remove(&k) } else { off.insert(k) };
        }
        if self.radio.cat.songs_on(&self.pre.cfg.off) == 0 {
            let g = usize::from(self.radio.cat.songs[0].is_empty());
            self.pre.cfg.off[g].remove(&self.radio.cat.songs[g][0].t.file);
            self.warn("At least one song must stay enabled");
        }
        self.apply();
    }

    fn library(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let names = ["Songs", "WMEW", "Voicelines"];
            ComboBox::from_id_salt("cat").selected_text(names[self.tab]).show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate() {
                    ui.selectable_value(&mut self.tab, i, *n);
                }
            });
            if self.tab == 2 {
                ComboBox::from_id_salt("sub").selected_text(VOICES[self.sub]).show_ui(ui, |ui| {
                    for (i, n) in VOICES.iter().enumerate() {
                        ui.selectable_value(&mut self.sub, i, *n);
                    }
                });
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("None").clicked() {
                    let k = self.keys();
                    self.set_off(k, false);
                }
                if ui.button("All").clicked() {
                    let k = self.keys();
                    self.set_off(k, true);
                }
                if ui.checkbox(&mut self.pre.cfg.outros, "Outros").changed() {
                    self.apply();
                }
                if ui.checkbox(&mut self.pre.cfg.intros, "Intros").changed() {
                    self.apply();
                }
            });
        });
        ui.visuals_mut().selection.bg_fill = col([Kind::Song, Kind::Wmew, Kind::Voice][self.tab]);
        ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            for (key, name, extra, dur) in self.rows() {
                let mut on = !self.pre.cfg.off[self.tab].contains(&key);
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut on, name).changed() {
                        self.set_off(vec![key], on);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(mm(dur)).color(MU));
                        ui.label(RichText::new(extra).color(MU));
                    });
                });
            }
        });
    }

    fn presets(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("Preset:");
            let mut done = None;
            if let Some(t) = &mut self.rename {
                let r = ui.add_sized([240., ui.spacing().interact_size.y], TextEdit::singleline(t).id(Id::new("ren")));
                if r.lost_focus() {
                    done = Some(!ui.input(|i| i.key_pressed(Key::Escape)));
                }
            } else {
                let mut sel = self.pre.cur;
                ComboBox::from_id_salt("pre").width(240.).selected_text(&self.pre.names[sel]).show_ui(ui, |ui| {
                    for (i, n) in self.pre.names.iter().enumerate() {
                        ui.selectable_value(&mut sel, i, n);
                    }
                });
                if sel != self.pre.cur {
                    self.pre.select(sel);
                    self.reload();
                }
            }
            if let Some(ok) = done {
                let t = self.rename.take().unwrap();
                if ok && !self.pre.rename(&t) {
                    self.warn("Invalid or taken name");
                }
            }
            if let Some((m, t)) = self.msg {
                if t.elapsed() < Duration::from_millis(2200) {
                    ui.colored_label(c(0xc08a6a), m);
                    ui.ctx().request_repaint_after(Duration::from_millis(2200));
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add_enabled(self.pre.names.len() > 1, Button::new("Delete")).clicked() {
                    self.rename = None;
                    self.pre.delete();
                    self.reload();
                }
                if ui.button("Rename").clicked() {
                    self.rename = Some(self.pre.names[self.pre.cur].clone());
                    ui.memory_mut(|m| m.request_focus(Id::new("ren")));
                }
                if ui.button("New").clicked() {
                    self.rename = None;
                    self.pre.new_preset();
                    self.reload();
                }
            });
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &Context, _: &mut eframe::Frame) {
        self.tick(ctx);
        SidePanel::left("q").exact_width(372.).resizable(false).show(ctx, |ui| self.queue(ui));
        TopBottomPanel::bottom("p").show(ctx, |ui| self.presets(ui));
        CentralPanel::default().show(ctx, |ui| {
            self.now(ui);
            self.mix_panel(ui);
            self.library(ui);
        });
    }

    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        self.pre.flush();
    }
}

fn main() -> eframe::Result {
    std::panic::set_hook(Box::new(|i| fatal(&i.to_string())));
    let root = std::env::args().nth(1).map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok()?.parent().map(PathBuf::from))
        .unwrap_or_default();
    let opt = eframe::NativeOptions {
        viewport: ViewportBuilder::default().with_inner_size([950., 650.]).with_resizable(false),
        ..Default::default()
    };
    eframe::run_native("MewRadio", opt, Box::new(move |cc| {
        let mut v = Visuals::dark();
        v.panel_fill = BG;
        v.widgets.inactive.weak_bg_fill = P2;
        v.widgets.noninteractive.fg_stroke.color = TX;
        v.widgets.inactive.fg_stroke.color = TX;
        v.handle_shape = style::HandleShape::Rect { aspect_ratio: 0.5 };
        v.window_corner_radius = 0.into();
        v.menu_corner_radius = 0.into();
        for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.corner_radius = 0.into();
        }
        cc.egui_ctx.set_visuals(v);
        Ok(Box::new(App::new(root)))
    }))
}
