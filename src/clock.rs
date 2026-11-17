//! Fixed-step simulation clock, decoupled from the render rate.

/// Accumulates real frame time and hands out whole simulation steps.
/// `alpha()` is how far the render time sits between the last two steps.
pub struct FixedClock {
    step: f64,
    acc: f64,
    time: f64,
    max_frame: f64,
}

impl FixedClock {
    pub fn new(hz: f64) -> Self {
        Self { step: 1.0 / hz, acc: 0.0, time: 0.0, max_frame: 0.25 }
    }

    pub fn step(&self) -> f64 {
        self.step
    }

    /// Simulation time (seconds) at the last completed step.
    pub fn time(&self) -> f64 {
        self.time
    }

    /// Feed a real frame delta; returns how many fixed steps to run now.
    /// Long stalls are clamped so the sim never spirals.
    pub fn advance(&mut self, real_dt: f64) -> u32 {
        self.acc += real_dt.clamp(0.0, self.max_frame);
        let mut n = 0;
        // Small tolerance so accumulated rounding doesn't drop a step.
        while self.acc >= self.step - 1e-9 {
            self.acc = (self.acc - self.step).max(0.0);
            self.time += self.step;
            n += 1;
        }
        n
    }

    /// 0..1 fraction of a step since the last completed step.
    pub fn alpha(&self) -> f32 {
        (self.acc / self.step) as f32
    }

    /// Smooth render time: last step time plus the fraction.
    pub fn render_time(&self) -> f64 {
        self.time + self.acc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_are_frame_rate_independent() {
        let mut a = FixedClock::new(240.0);
        let mut b = FixedClock::new(240.0);
        let (mut na, mut nb) = (0, 0);
        for _ in 0..60 {
            na += a.advance(1.0 / 60.0);
        }
        for _ in 0..144 {
            nb += b.advance(1.0 / 144.0);
        }
        assert!((na as i32 - 240).abs() <= 1, "{na}");
        assert!((nb as i32 - 240).abs() <= 1, "{nb}");
    }

    #[test]
    fn stall_is_clamped() {
        let mut c = FixedClock::new(100.0);
        assert_eq!(c.advance(10.0), 25);
    }
}
