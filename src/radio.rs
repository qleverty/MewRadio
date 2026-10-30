use crate::{catalog::*, rng::Rng, settings::Settings};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq)]
pub enum Kind { Song, Wmew, Intro, Voice }

#[derive(Clone)]
pub struct Entry {
    pub kind: Kind,
    pub title: String,
    pub sub: &'static str,
    pub path: PathBuf,
    pub dur: f32,
}

// radio.gon state machine, repeats in next[] are weights
static G: [(&str, &[&str]); 9] = [
    ("begin", &["call_letter", "intro", "song", "song", "song"]),
    ("song", &["intro", "outro", "transitional", "commercial_intro", "dj_segment", "call_letter", "song", "song"]),
    ("intro", &["song"]),
    ("outro", &["song"]),
    ("transitional", &["intro", "song", "song"]),
    ("commercial_intro", &["commercial"]),
    ("commercial", &["intro", "song", "call_letter"]),
    ("dj_segment", &["intro", "song", "call_letter"]),
    ("call_letter", &["intro", "song"]),
];

const FOLDERS: [&str; 5] = ["commercial", "commercial_intro", "transitional", "dj_segment", "call_letter"];

type Ref = (usize, usize);

fn shuffle(a: &mut [usize], r: &mut Rng) {
    for m in (2..=a.len()).rev() {
        a.swap(r.below(m), m - 1);
    }
}

struct Bag {
    it: Vec<usize>,
    i: usize,
}

impl Bag {
    fn new(mut it: Vec<usize>, r: &mut Rng) -> Option<Self> {
        shuffle(&mut it, r);
        (!it.is_empty()).then_some(Bag { it, i: 0 })
    }

    fn next(&mut self, r: &mut Rng) -> usize {
        let n = self.it.len();
        let x = self.it[self.i];
        self.i += 1;
        if self.i >= n {
            // lap done, partial reshuffle so recent ones dont come first
            let h = n.div_ceil(2);
            shuffle(&mut self.it[..h], r);
            shuffle(&mut self.it[h..], r);
            shuffle(&mut self.it[n.div_ceil(4)..(n * 3).div_ceil(4).min(n)], r);
            self.i = 0;
        }
        x
    }
}

pub struct Radio {
    pub cat: Catalog,
    cfg: Settings,
    rng: Rng,
    bags: [Option<Bag>; 7], // songs, wmew and voice folders
    st: &'static str,
    pend: Option<Ref>, // next song, picked early for its intro
    last: Option<Ref>,
}

impl Radio {
    pub fn new(cat: Catalog, cfg: &Settings) -> Self {
        let mut r = Radio { cat, cfg: cfg.clone(), rng: Rng::new(), bags: Default::default(), st: "", pend: None, last: None };
        r.apply(cfg);
        r
    }

    pub fn apply(&mut self, cfg: &Settings) {
        self.cfg = cfg.clone();
        for g in 0..2 {
            let v = (0..self.cat.songs[g].len()).filter(|&i| !cfg.off[g].contains(&self.cat.songs[g][i].t.file)).collect();
            self.bags[g] = Bag::new(v, &mut self.rng);
        }
        for f in 0..5 {
            let v = (0..self.cat.voices[f].len()).filter(|&i| !cfg.off[2].contains(&vkey(f, &self.cat.voices[f][i]))).collect();
            self.bags[2 + f] = Bag::new(v, &mut self.rng);
        }
        self.pend = self.pend.filter(|&(g, i)| !cfg.off[g].contains(&self.cat.songs[g][i].t.file));
    }

    fn song(&mut self) -> Option<Ref> {
        let w = |g: usize| if self.bags[g].is_some() { self.cfg.chances[g] as f64 } else { 0. };
        let (a, b) = (w(0), w(1));
        let mut g = if a + b > 0. { usize::from(self.rng.f() * (a + b) >= a) } else { usize::from(self.bags[0].is_none()) };
        if self.bags[g].is_none() {
            g = usize::from(self.bags[0].is_none());
        }
        Some((g, self.bags[g].as_mut()?.next(&mut self.rng)))
    }

    fn entry(&self, (g, i): Ref) -> Entry {
        let t = &self.cat.songs[g][i].t;
        Entry { kind: [Kind::Song, Kind::Wmew][g], title: t.name.clone(), sub: ["Songs", "WMEW"][g], path: t.path.clone(), dur: t.dur }
    }

    // by default 25|21|54
    fn k(&self) -> f64 {
        let c = self.cfg.chances;
        let s = (c[0] + c[1]) as f64;
        if s > 0. { c[2] as f64 / s / (54.3 / 45.7) } else { 0. }
    }

    fn pick(&mut self, l: &[&'static str]) -> &'static str {
        let k = self.k();
        let w = |x: &str| if x == "song" { 1. } else { k };
        let mut r = self.rng.f() * l.iter().map(|&x| w(x)).sum::<f64>();
        for &x in l {
            r -= w(x);
            if r < 0. {
                return x;
            }
        }
        l[l.len() - 1]
    }

    fn choose(&mut self, s: &str) -> Option<Entry> {
        if s == "song" {
            let r = self.pend.take().or_else(|| self.song())?;
            self.last = Some(r);
            return Some(self.entry(r));
        }
        if s == "intro" || s == "outro" {
            let intro = s == "intro";
            if intro && self.pend.is_none() {
                self.pend = self.song();
            }
            let (g, i) = if intro { self.pend } else { self.last }?;
            let sg = &self.cat.songs[g][i];
            let v = if intro { &sg.intros } else { &sg.outros };
            if v.is_empty() || !(if intro { self.cfg.intros } else { self.cfg.outros }) {
                return None;
            }
            let j = self.rng.below(v.len());
            return Some(Entry { kind: Kind::Intro, title: format!("{} {s} {}", sg.t.name, j + 1), sub: if intro { "Intro" } else { "Outro" }, path: v[j].clone(), dur: duration(&v[j]) });
        }
        let f = FOLDERS.iter().position(|&x| x == s)?;
        let i = self.bags[2 + f].as_mut()?.next(&mut self.rng);
        let t = &self.cat.voices[f][i];
        Some(Entry { kind: Kind::Voice, title: t.name.clone(), sub: VOICES[f], path: t.path.clone(), dur: t.dur })
    }

    pub fn next(&mut self) -> Entry {
        for _ in 0..100 {
            self.st = match G.iter().find(|g| g.0 == self.st) {
                Some(g) => self.pick(g.1),
                None => "begin",
            };
            if let Some(e) = self.choose(self.st) {
                return e;
            }
        }
        self.st = "";
        let r = self.song().expect("no songs");
        self.entry(r)
    }
}
