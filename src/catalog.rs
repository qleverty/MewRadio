use std::{collections::HashSet, fs::{self, File}, io::{Read, Seek, SeekFrom}, path::{Path, PathBuf}};

pub const VOICES: [&str; 5] = ["Commercials", "Commercial Breaks", "Transitions", "DJ Segments", "Station IDs"];

pub struct Track {
    pub name: String,
    pub file: String,
    pub path: PathBuf,
    pub dur: f32,
}

pub struct Song {
    pub t: Track,
    pub intros: Vec<PathBuf>,
    pub outros: Vec<PathBuf>,
}

pub struct Catalog {
    pub songs: [Vec<Song>; 2], // Songs, WMEW
    pub voices: [Vec<Track>; 5],
}

pub fn vkey(f: usize, t: &Track) -> String {
    format!("{}/{}", VOICES[f], t.file)
}

pub fn duration(p: &Path) -> f32 {
    (|| {
        let mut f = File::open(p).ok()?;
        let mut h = [0; 44];
        f.read_exact(&mut h).ok()?;
        let rate = u32::from_le_bytes(h[40..].try_into().ok()?);
        f.seek(SeekFrom::End(-65536)).map(drop).or_else(|_| f.rewind()).ok()?;
        let mut b = vec![];
        f.read_to_end(&mut b).ok()?;
        let i = b.windows(4).rposition(|w| w == b"OggS")?;
        let g = u64::from_le_bytes(b.get(i + 6..i + 14)?.try_into().ok()?);
        Some(g as f32 / rate as f32)
    })()
    .unwrap_or(0.)
}

fn oggs(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut v: Vec<_> = fs::read_dir(dir).into_iter().flatten().flatten()
        .filter_map(|e| Some((e.file_name().into_string().ok()?, e.path())))
        .filter(|(f, _)| f.to_lowercase().ends_with(".ogg"))
        .collect();
    v.sort_by_key(|(f, _)| f.to_lowercase());
    v
}

fn track(f: String, path: PathBuf) -> Track {
    Track { name: f[..f.len() - 4].into(), dur: duration(&path), file: f, path }
}

fn tag(n: &str) -> Option<(&str, bool)> {
    let (b, s) = n.rsplit_once('.')?;
    let (out, d) = match s.strip_prefix("intro") {
        Some(d) => (false, d),
        None => (true, s.strip_prefix("outro")?),
    };
    d.bytes().all(|c| c.is_ascii_digit()).then_some((b, out))
}

fn songs(dir: &Path) -> Vec<Song> {
    let (mut v, mut extra) = (vec![], vec![]);
    for (f, p) in oggs(dir) {
        match tag(&f[..f.len() - 4]) {
            Some((b, out)) => extra.push((b.to_string(), out, p)),
            None => v.push(Song { t: track(f, p), intros: vec![], outros: vec![] }),
        }
    }
    for (b, out, p) in extra {
        if let Some(s) = v.iter_mut().find(|s| s.t.name == b) {
            if out { s.outros.push(p) } else { s.intros.push(p) }
        }
    }
    v
}

impl Catalog {
    pub fn scan(root: &Path) -> Self {
        Catalog {
            songs: ["Songs", "WMEW"].map(|d| songs(&root.join(d))),
            voices: VOICES.map(|d| oggs(&root.join("Voicelines").join(d)).into_iter().map(|(f, p)| track(f, p)).collect()),
        }
    }

    pub fn songs_on(&self, off: &[HashSet<String>; 3]) -> usize {
        (0..2).map(|g| self.songs[g].iter().filter(|s| !off[g].contains(&s.t.file)).count()).sum()
    }
}
