//! Tiny deterministic RNG and hash helpers (no external dependency).

/// PCG32-style generator.
#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut r = Rng(seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407));
        r.next_u32();
        r
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.0;
        self.0 = old.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Uniform in [0, 1).
    pub fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }

    /// Uniform direction on the unit sphere.
    pub fn unit_vec3(&mut self) -> glam::Vec3 {
        let z = self.range(-1.0, 1.0);
        let a = self.range(0.0, std::f32::consts::TAU);
        let r = (1.0 - z * z).sqrt();
        glam::Vec3::new(r * a.cos(), r * a.sin(), z)
    }
}

/// Integer hash → [0, 1).
pub fn hash01(x: u32) -> f32 {
    let mut h = x.wrapping_mul(0x9E37_79B1);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85EB_CA77);
    h ^= h >> 13;
    h = h.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 16;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// 2D integer hash → [0, 1).
pub fn hash2(x: i32, y: i32) -> f32 {
    hash01((x as u32).wrapping_mul(73_856_093) ^ (y as u32).wrapping_mul(19_349_663))
}
