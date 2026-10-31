use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};
use std::{fs::File, path::Path};

pub struct Audio {
    _dev: MixerDeviceSink,
    pub sink: Player,
}

impl Audio {
    pub fn new() -> Result<Self, String> {
        let dev = DeviceSinkBuilder::open_default_sink().map_err(|e| e.to_string())?;
        let sink = Player::connect_new(dev.mixer());
        Ok(Audio { _dev: dev, sink })
    }

    pub fn add(&self, p: &Path) {
        if let Some(d) = File::open(p).ok().and_then(|f| Decoder::try_from(f).ok()) {
            self.sink.append(d);
        }
    }

    pub fn play(&self, p: &Path, playing: bool) {
        self.sink.clear();
        self.add(p);
        if playing { self.sink.play() } else { self.sink.pause() }
    }
}
