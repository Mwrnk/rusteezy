//! Simple CPU particle system with interpolated positions.

use glam::Vec3;

use crate::color::Rgb;

#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub pos: Vec3,
    /// Position at the previous fixed step (for render interpolation).
    pub prev: Vec3,
    pub vel: Vec3,
    pub age: f32,
    pub life: f32,
    /// Linear drag coefficient (1/s).
    pub drag: f32,
    /// User tag (e.g. snow vs spark).
    pub kind: u8,
    /// Per-particle random value 0..1 for variation.
    pub seed: f32,
    pub size: f32,
    /// Base color (scenes decide how it changes over life).
    pub color: Rgb,
}

impl Particle {
    pub fn new(pos: Vec3, vel: Vec3, life: f32) -> Self {
        Self { pos, prev: pos, vel, age: 0.0, life, drag: 0.0, kind: 0, seed: 0.0, size: 1.0, color: Rgb::WHITE }
    }

    /// 0..1 fraction of life used.
    pub fn t(&self) -> f32 {
        if self.life.is_finite() { (self.age / self.life).clamp(0.0, 1.0) } else { 0.0 }
    }

    /// Position interpolated between the last two steps.
    pub fn render_pos(&self, alpha: f32) -> Vec3 {
        self.prev.lerp(self.pos, alpha)
    }
}

#[derive(Default)]
pub struct ParticleSystem {
    pub particles: Vec<Particle>,
    pub max: usize,
}

impl ParticleSystem {
    pub fn new(max: usize) -> Self {
        Self { particles: Vec::with_capacity(max), max }
    }

    pub fn len(&self) -> usize {
        self.particles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }

    pub fn spawn(&mut self, p: Particle) {
        if self.particles.len() < self.max {
            self.particles.push(p);
        }
    }

    pub fn clear(&mut self) {
        self.particles.clear();
    }

    /// Integrate one fixed step. `accel` returns the acceleration for a particle
    /// (gravity, wind, …). Dead particles are removed.
    pub fn update(&mut self, dt: f32, mut accel: impl FnMut(&Particle) -> Vec3) {
        for p in &mut self.particles {
            p.prev = p.pos;
            let a = accel(p) - p.vel * p.drag;
            p.vel += a * dt; // semi-implicit Euler
            p.pos += p.vel * dt;
            p.age += dt;
        }
        self.particles.retain(|p| p.age < p.life);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_and_dies() {
        let mut ps = ParticleSystem::new(10);
        ps.spawn(Particle::new(Vec3::ZERO, Vec3::ZERO, 1.0));
        for _ in 0..50 {
            ps.update(0.01, |_| Vec3::new(0.0, -10.0, 0.0));
        }
        assert!(ps.particles[0].pos.y < -1.0);
        for _ in 0..60 {
            ps.update(0.01, |_| Vec3::ZERO);
        }
        assert!(ps.is_empty());
    }

    #[test]
    fn drag_gives_terminal_velocity() {
        let mut ps = ParticleSystem::new(1);
        let mut p = Particle::new(Vec3::ZERO, Vec3::ZERO, f32::INFINITY);
        p.drag = 2.0;
        ps.spawn(p);
        for _ in 0..2000 {
            ps.update(0.005, |_| Vec3::new(0.0, -1.0, 0.0));
        }
        assert!((ps.particles[0].vel.y + 0.5).abs() < 1e-3);
    }
}
