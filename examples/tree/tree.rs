//! Tree geometry (in tree space: Y up, trunk on the Y axis) and per-cell ray tracing.

use rusteezy::glam::Vec3;

pub const GROUND_Y: f32 = -0.3;
pub const TRUNK_R: f32 = 0.12;
pub const TRUNK_TOP: f32 = 0.32;
pub const TOP_Y: f32 = 2.05;

/// One cone tier: base at `yb` with radius `rb`, apex at `ya`.
#[derive(Clone, Copy)]
pub struct Tier {
    pub yb: f32,
    pub ya: f32,
    pub rb: f32,
}

impl Tier {
    pub fn k(&self) -> f32 {
        self.rb / (self.ya - self.yb)
    }

    pub fn radius(&self, y: f32) -> Option<f32> {
        (y >= self.yb && y <= self.ya).then(|| self.k() * (self.ya - y))
    }
}

pub const TIERS: [Tier; 3] = [
    Tier { yb: 0.15, ya: 1.25, rb: 0.95 },
    Tier { yb: 0.70, ya: 1.70, rb: 0.72 },
    Tier { yb: 1.15, ya: TOP_Y, rb: 0.50 },
];

/// Outer skin radius of the foliage at height `y` (0 where there is none).
pub fn envelope(y: f32) -> f32 {
    TIERS.iter().filter_map(|t| t.radius(y)).fold(0.0, f32::max)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Surf {
    Foliage {
        tier: usize,
    },
    /// Glowing cross-section at the build front.
    Cap,
    Underside,
    Trunk,
}

#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub t: f32,
    /// Hit point (tree space).
    pub p: Vec3,
    /// Outward normal (tree space).
    pub n: Vec3,
    pub surf: Surf,
}

/// Trace a ray (tree space, `rd` normalized) against the visible part of the
/// tree: everything at or below `front` (the rising build edge).
pub fn trace(ro: Vec3, rd: Vec3, front: f32) -> Option<Hit> {
    let mut best: Option<Hit> = None;
    let mut consider = |h: Hit| {
        if h.t > 1e-4 && best.is_none_or(|b| h.t < b.t) {
            best = Some(h);
        }
    };

    for (i, tier) in TIERS.iter().enumerate() {
        if tier.yb > front {
            continue;
        }
        let k2 = tier.k() * tier.k();
        let h = tier.ya - ro.y;
        let a = rd.x * rd.x + rd.z * rd.z - k2 * rd.y * rd.y;
        let b = 2.0 * (ro.x * rd.x + ro.z * rd.z + k2 * h * rd.y);
        let c = ro.x * ro.x + ro.z * ro.z - k2 * h * h;
        let disc = b * b - 4.0 * a * c;
        if disc >= 0.0 && a.abs() > 1e-8 {
            let sq = disc.sqrt();
            for t in [(-b - sq) / (2.0 * a), (-b + sq) / (2.0 * a)] {
                let p = ro + rd * t;
                if p.y >= tier.yb && p.y <= tier.ya && p.y <= front {
                    let n = Vec3::new(p.x, k2 * (tier.ya - p.y), p.z).normalize_or(Vec3::Y);
                    consider(Hit { t, p, n, surf: Surf::Foliage { tier: i } });
                }
            }
        }
        if rd.y.abs() > 1e-6 {
            // Underside disc of the tier.
            let t = (tier.yb - ro.y) / rd.y;
            let p = ro + rd * t;
            if p.x * p.x + p.z * p.z <= tier.rb * tier.rb {
                consider(Hit { t, p, n: Vec3::NEG_Y, surf: Surf::Underside });
            }
            // Cap at the build front.
            if front > tier.yb && front < tier.ya {
                let t = (front - ro.y) / rd.y;
                let p = ro + rd * t;
                let r = tier.k() * (tier.ya - front);
                if p.x * p.x + p.z * p.z <= r * r {
                    consider(Hit { t, p, n: Vec3::Y, surf: Surf::Cap });
                }
            }
        }
    }

    // Trunk cylinder.
    let a = rd.x * rd.x + rd.z * rd.z;
    if a > 1e-8 {
        let b = 2.0 * (ro.x * rd.x + ro.z * rd.z);
        let c = ro.x * ro.x + ro.z * ro.z - TRUNK_R * TRUNK_R;
        let disc = b * b - 4.0 * a * c;
        if disc >= 0.0 {
            let t = (-b - disc.sqrt()) / (2.0 * a);
            let p = ro + rd * t;
            if p.y >= GROUND_Y && p.y <= TRUNK_TOP.min(front) {
                consider(Hit { t, p, n: Vec3::new(p.x, 0.0, p.z).normalize_or(Vec3::X), surf: Surf::Trunk });
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_from_front_hits_lower_tier() {
        let h = trace(Vec3::new(0.0, 0.5, 6.0), Vec3::NEG_Z, 10.0).unwrap();
        assert!(matches!(h.surf, Surf::Foliage { tier: 0 }));
        assert!((h.p.z - TIERS[0].radius(0.5).unwrap()).abs() < 1e-3);
        assert!(h.n.z > 0.5);
    }

    #[test]
    fn unbuilt_tree_is_invisible() {
        assert!(trace(Vec3::new(0.0, 1.0, 6.0), Vec3::NEG_Z, -1.0).is_none());
    }

    #[test]
    fn cap_appears_at_front() {
        let ro = Vec3::new(0.0, 3.0, 0.2);
        let h = trace(ro, Vec3::NEG_Y, 0.9).unwrap();
        assert_eq!(h.surf, Surf::Cap);
        assert!((h.p.y - 0.9).abs() < 1e-4);
    }

    #[test]
    fn envelope_is_max_of_tiers() {
        assert!((envelope(0.7) - 0.72).abs() < 1e-4);
        assert_eq!(envelope(2.5), 0.0);
    }
}
