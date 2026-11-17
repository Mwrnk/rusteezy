//! Easing curves and tweens.

use crate::color::Rgb;
use glam::{Vec2, Vec3};

pub mod ease {
    pub fn linear(t: f32) -> f32 {
        t
    }
    pub fn in_quad(t: f32) -> f32 {
        t * t
    }
    pub fn out_quad(t: f32) -> f32 {
        1.0 - (1.0 - t) * (1.0 - t)
    }
    pub fn in_out_quad(t: f32) -> f32 {
        if t < 0.5 { 2.0 * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(2) / 2.0 }
    }
    pub fn out_cubic(t: f32) -> f32 {
        1.0 - (1.0 - t).powi(3)
    }
    pub fn in_out_cubic(t: f32) -> f32 {
        if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }
    }
    pub fn in_out_sine(t: f32) -> f32 {
        -((std::f32::consts::PI * t).cos() - 1.0) / 2.0
    }
    pub fn out_back(t: f32) -> f32 {
        let c1 = 1.70158;
        let c3 = c1 + 1.0;
        1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
    }
    pub fn out_bounce(t: f32) -> f32 {
        let (n1, d1) = (7.5625, 2.75);
        if t < 1.0 / d1 {
            n1 * t * t
        } else if t < 2.0 / d1 {
            let t = t - 1.5 / d1;
            n1 * t * t + 0.75
        } else if t < 2.5 / d1 {
            let t = t - 2.25 / d1;
            n1 * t * t + 0.9375
        } else {
            let t = t - 2.625 / d1;
            n1 * t * t + 0.984375
        }
    }
    pub fn out_elastic(t: f32) -> f32 {
        if t <= 0.0 || t >= 1.0 {
            return t.clamp(0.0, 1.0);
        }
        let c4 = std::f32::consts::TAU / 3.0;
        2f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
    }
    /// Hermite smoothstep of `x` between `e0` and `e1`.
    pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
        let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }
}

/// Anything that can be interpolated.
pub trait Lerp: Copy {
    fn lerp_to(self, to: Self, t: f32) -> Self;
}

impl Lerp for f32 {
    fn lerp_to(self, to: Self, t: f32) -> Self {
        self + (to - self) * t
    }
}
impl Lerp for Vec2 {
    fn lerp_to(self, to: Self, t: f32) -> Self {
        self.lerp(to, t)
    }
}
impl Lerp for Vec3 {
    fn lerp_to(self, to: Self, t: f32) -> Self {
        self.lerp(to, t)
    }
}
/// Colors tween through OKLab so fades look even.
impl Lerp for Rgb {
    fn lerp_to(self, to: Self, t: f32) -> Self {
        self.lerp_oklab(to, t)
    }
}

/// A value animating from `from` to `to` over `[start, start+dur]` seconds.
#[derive(Clone, Copy)]
pub struct Tween<T: Lerp> {
    pub from: T,
    pub to: T,
    pub start: f64,
    pub dur: f32,
    pub ease: fn(f32) -> f32,
}

impl<T: Lerp> Tween<T> {
    pub fn new(from: T, to: T, start: f64, dur: f32, ease: fn(f32) -> f32) -> Self {
        Self { from, to, start, dur, ease }
    }

    /// Normalized, un-eased progress 0..1.
    pub fn progress(&self, now: f64) -> f32 {
        if self.dur <= 0.0 {
            return 1.0;
        }
        (((now - self.start) / self.dur as f64) as f32).clamp(0.0, 1.0)
    }

    pub fn value(&self, now: f64) -> T {
        self.from.lerp_to(self.to, (self.ease)(self.progress(now)))
    }

    pub fn done(&self, now: f64) -> bool {
        self.progress(now) >= 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tween_clamps_and_eases() {
        let tw = Tween::new(0.0f32, 10.0, 1.0, 2.0, ease::in_out_cubic);
        assert_eq!(tw.value(0.0), 0.0);
        assert!((tw.value(2.0) - 5.0).abs() < 1e-4);
        assert_eq!(tw.value(5.0), 10.0);
        assert!(tw.done(3.0));
    }

    #[test]
    fn easings_hit_endpoints() {
        for f in
            [ease::linear, ease::out_cubic, ease::in_out_cubic, ease::out_back, ease::out_bounce, ease::in_out_sine]
        {
            assert!(f(0.0).abs() < 1e-4);
            assert!((f(1.0) - 1.0).abs() < 1e-4);
        }
    }
}
