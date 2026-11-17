//! Spiral garland parametrized by arc length.

use std::f32::consts::TAU;

use rusteezy::glam::Vec3;

use crate::tree::{TOP_Y, envelope};

pub const Y0: f32 = 0.14;
pub const Y1: f32 = 1.98;
pub const TURNS: f32 = 5.0;
const SAMPLES: usize = 3000;

/// Straight line through the tier rims; keeps the spiral from jumping radially.
fn hull(y: f32) -> f32 {
    0.99 * (TOP_Y + 0.01 - y) / (TOP_Y + 0.01 - 0.12)
}

pub struct Garland {
    pts: Vec<Vec3>,
    /// Cumulative arc length at each sample.
    cum: Vec<f32>,
}

impl Garland {
    pub fn new() -> Self {
        let mut pts = Vec::with_capacity(SAMPLES);
        let mut cum = Vec::with_capacity(SAMPLES);
        for i in 0..SAMPLES {
            let u = i as f32 / (SAMPLES - 1) as f32;
            let y = Y0 + (Y1 - Y0) * u;
            let phi = TURNS * TAU * u;
            let r = envelope(y).max(0.8 * hull(y)) + 0.035;
            let p = Vec3::new(r * phi.cos(), y, r * phi.sin());
            let l = pts.last().map_or(0.0, |q: &Vec3| cum.last().unwrap() + q.distance(p));
            pts.push(p);
            cum.push(l);
        }
        Self { pts, cum }
    }

    pub fn total(&self) -> f32 {
        *self.cum.last().unwrap()
    }

    /// Point (tree space) at arc length `s`.
    pub fn at(&self, s: f32) -> Vec3 {
        let s = s.clamp(0.0, self.total());
        let i = self.cum.partition_point(|&c| c < s).clamp(1, self.pts.len() - 1);
        let (c0, c1) = (self.cum[i - 1], self.cum[i]);
        let f = if c1 > c0 { (s - c0) / (c1 - c0) } else { 0.0 };
        self.pts[i - 1].lerp(self.pts[i], f)
    }

    /// Arc length at which the spiral reaches height `y`.
    pub fn s_at_y(&self, y: f32) -> f32 {
        let u = ((y - Y0) / (Y1 - Y0)).clamp(0.0, 1.0);
        let x = u * (SAMPLES - 1) as f32;
        let i = (x as usize).min(SAMPLES - 2);
        let f = x - i as f32;
        self.cum[i] + (self.cum[i + 1] - self.cum[i]) * f
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_length_is_monotonic_and_consistent() {
        let g = Garland::new();
        assert!(g.total() > 10.0);
        // Points one unit of arc apart are ~one unit apart in space (curve is gentle).
        let a = g.at(3.0);
        let b = g.at(3.05);
        assert!((a.distance(b) - 0.05).abs() < 0.005);
        assert!(g.s_at_y(Y0) == 0.0);
        assert!((g.s_at_y(Y1) - g.total()).abs() < 1e-3);
        assert!(g.at(g.s_at_y(1.0)).y - 1.0 < 0.01);
    }
}
