// radnomiser as in the game (xoshiro256+)
pub struct Rng([u64; 4]);

impl Rng {
    pub fn new() -> Self {
        let mut s = std::time::UNIX_EPOCH.elapsed().map_or(1, |d| d.as_nanos() as u64);
        Rng([(); 4].map(|_| {
            s = s.wrapping_add(0x9E3779B97F4A7C15);
            let z = (s ^ s >> 30).wrapping_mul(0xBF58476D1CE4E5B9);
            let z = (z ^ z >> 27).wrapping_mul(0x94D049BB133111EB);
            z ^ z >> 31
        }))
    }

    pub fn f(&mut self) -> f64 {
        let s = &mut self.0;
        let r = s[0].wrapping_add(s[3]);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        (r >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.f() * n as f64) as usize
    }
}
