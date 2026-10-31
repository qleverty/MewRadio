use std::{collections::HashSet, fs, path::{Path, PathBuf}, time::{Duration, Instant}};

const WAIT: Duration = Duration::from_millis(500);
const KEYS: [&str; 3] = ["songs_disabled", "wmew_disabled", "voicelines_disabled"];

#[derive(Clone)]
pub struct Settings {
    pub chances: [u32; 3], // Songs, WMEW, Voicelines
    pub intros: bool,
    pub outros: bool,
    pub off: [HashSet<String>; 3],
}

impl Default for Settings {
    fn default() -> Self {
        Settings { chances: [25, 21, 54], intros: true, outros: true, off: Default::default() }
    }
}

impl Settings {
    fn parse(t: &str) -> Self {
        let mut s = Self::default();
        for l in t.lines() {
            let Some((k, v)) = l.split_once(':') else { continue };
            let v = v.trim();
            match k.trim() {
                "playing_chances" => {
                    let n: Vec<u32> = v.split('|').filter_map(|x| x.trim().parse().ok()).collect();
                    if n.len() == 3 && n.iter().sum::<u32>() == 100 && n[0] + n[1] >= 5 {
                        s.chances = [n[0], n[1], n[2]];
                    }
                }
                "intros" => s.intros = v != "0",
                "outros" => s.outros = v != "0",
                k => {
                    if let Some(i) = KEYS.iter().position(|&x| x == k) {
                        s.off[i] = v.split('|').map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect();
                    }
                }
            }
        }
        s
    }

    fn text(&self) -> String {
        let c = self.chances;
        let mut t = format!("v:1\nplaying_chances:{}|{}|{}\nintros:{}\noutros:{}\n", c[0], c[1], c[2], self.intros as u8, self.outros as u8);
        for (k, s) in KEYS.iter().zip(&self.off) {
            let mut v: Vec<_> = s.iter().map(String::as_str).collect();
            v.sort();
            t += &format!("{k}:{}\n", v.join("|"));
        }
        t
    }
}

fn write(p: &Path, s: &str) {
    let t = p.with_extension("tmp");
    if fs::write(&t, s).is_ok() {
        let _ = fs::rename(t, p);
    }
}

fn read(p: &Path) -> String {
    fs::read_to_string(p).unwrap_or_default().trim_start_matches('\u{feff}').to_string()
}

fn same(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

// preset name is a file name, so follow windows rules
pub fn ok_name(n: &str) -> bool {
    let u = n.split('.').next().unwrap_or("").to_uppercase();
    let dev = ["CON", "PRN", "AUX", "NUL"].contains(&u.as_str())
        || (u.len() == 4 && (u.starts_with("COM") || u.starts_with("LPT")) && !u.ends_with('0') && u.ends_with(|c: char| c.is_ascii_digit()));
    !n.is_empty() && n.len() <= 64 && !dev && !n.ends_with(['.', ' '])
        && !n.contains(['|', ':', '?', '*', '"', '<', '>', '\\', '/']) && !n.chars().any(char::is_control)
}

pub struct Presets {
    dir: PathBuf,
    state: PathBuf,
    pub names: Vec<String>,
    pub cur: usize,
    pub cfg: Settings,
    pub vol: f32, // gain, 0..2 (0-200%)
    pub win: Option<[f32; 4]>, // width, height, x, y
    pub split: f32, // width of the queue panel
    dirty: Option<(Instant, bool)>,
}

impl Presets {
    pub fn load(root: &Path) -> Self {
        let dir = root.join("Presets");
        let _ = fs::create_dir_all(&dir);
        let mut names: Vec<String> = fs::read_dir(&dir).into_iter().flatten().flatten()
            .filter_map(|e| e.file_name().into_string().ok()?.strip_suffix(".txt").map(String::from))
            .filter(|n| ok_name(n))
            .collect();
        names.sort_by_key(|n| n.to_lowercase());
        let state = root.join("state.txt");
        let t = read(&state);
        let (mut want, mut vol, mut win, mut split) = (String::new(), 1., None, 372.);
        for l in t.lines() {
            let Some((k, v)) = l.split_once(':') else { continue };
            let n: Vec<f32> = v.split('|').filter_map(|x| x.trim().parse().ok()).collect();
            match (k.trim(), n.as_slice()) {
                ("preset", _) => want = v.trim().into(),
                ("volume", [v]) => vol = (v / 100.).clamp(0., 2.),
                ("window", [w, h, x, y]) if *w >= 300. && *h >= 200. && x.abs() < 30000. && y.abs() < 30000. => win = Some([*w, *h, *x, *y]),
                ("split", [s]) if *s >= 100. => split = *s,
                _ => {}
            }
        }
        let cur = names.iter().position(|n| same(n, &want)).unwrap_or(0);
        let mut p = Presets { dir, state, names, cur, cfg: Settings::default(), vol, win, split, dirty: None };
        if p.names.is_empty() {
            p.cur = p.add("Unnamed");
        }
        p.cfg = Settings::parse(&read(&p.path()));
        p.save_state();
        p
    }

    fn path(&self) -> PathBuf {
        self.dir.join(format!("{}.txt", self.names[self.cur]))
    }

    fn save_state(&self) {
        let mut t = format!("preset:{}\nvolume:{}\nsplit:{}\n", self.names[self.cur], (self.vol * 100.).round(), self.split);
        if let Some([w, h, x, y]) = self.win {
            t += &format!("window:{w}|{h}|{x}|{y}\n");
        }
        write(&self.state, &t);
    }

    fn add(&mut self, n: &str) -> usize {
        write(&self.dir.join(format!("{n}.txt")), &Settings::default().text());
        self.names.push(n.into());
        self.sort(n)
    }

    // sort and return where `n` ended up
    fn sort(&mut self, n: &str) -> usize {
        self.names.sort_by_key(|x| x.to_lowercase());
        self.names.iter().position(|x| x == n).unwrap()
    }

    pub fn touch(&mut self, preset: bool) {
        let p = preset || self.dirty.is_some_and(|d| d.1);
        self.dirty = Some((Instant::now(), p));
    }

    // true while a save is still waiting
    pub fn tick(&mut self) -> bool {
        match self.dirty {
            Some((t, _)) if t.elapsed() >= WAIT => {
                self.flush();
                false
            }
            d => d.is_some(),
        }
    }

    pub fn flush(&mut self) {
        let Some((_, p)) = self.dirty.take() else { return };
        if p {
            write(&self.path(), &self.cfg.text());
        }
        self.save_state();
    }

    pub fn select(&mut self, i: usize) {
        self.flush();
        self.cur = i;
        self.cfg = Settings::parse(&read(&self.path()));
        self.save_state();
    }

    pub fn new_preset(&mut self) {
        self.flush();
        let (mut n, mut k) = ("Unnamed".to_string(), 2);
        while self.names.iter().any(|x| same(x, &n)) {
            n = format!("Unnamed {k}");
            k += 1;
        }
        self.cur = self.add(&n);
        self.cfg = Settings::default();
        self.save_state();
    }

    pub fn rename(&mut self, n: &str) -> bool {
        self.flush();
        let n = n.trim();
        if n == self.names[self.cur] {
            return true;
        }
        let taken = self.names.iter().any(|x| same(x, n)) && !same(&self.names[self.cur], n);
        if !ok_name(n) || taken || fs::rename(self.path(), self.dir.join(format!("{n}.txt"))).is_err() {
            return false;
        }
        self.names[self.cur] = n.into();
        self.cur = self.sort(n);
        self.save_state();
        true
    }

    pub fn delete(&mut self) {
        if self.names.len() < 2 {
            return;
        }
        self.dirty = None;
        let _ = fs::remove_file(self.path());
        self.names.remove(self.cur);
        self.cur = self.cur.min(self.names.len() - 1);
        self.cfg = Settings::parse(&read(&self.path()));
        self.save_state();
    }
}
