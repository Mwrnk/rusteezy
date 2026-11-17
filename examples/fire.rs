//! A heat-diffusion fire with rising embers. Every cell is a little simulation;
//! heat maps to glyph density, an OKLab color ramp and glow.
//!
//! ```text
//! cargo run --release --example fire
//! ```

use rusteezy::{Config, Frame, Particle, ParticleSystem, Rgb, Rng, Scene, ease::smoothstep, glam::Vec3};

const RAMP: [char; 10] = [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];
const STEP: f32 = 1.0 / 45.0;

struct Fire {
    heat: Vec<f32>,
    w: usize,
    h: usize,
    rng: Rng,
    acc: f32,
    time: f32,
    embers: ParticleSystem,
}

impl Fire {
    fn new() -> Self {
        Self { heat: Vec::new(), w: 0, h: 0, rng: Rng::new(3), acc: 0.0, time: 0.0, embers: ParticleSystem::new(4000) }
    }

    fn step(&mut self) {
        let (w, h) = (self.w, self.h);
        if w < 3 || h < 3 {
            return;
        }
        // Fuel: the bottom two rows flicker between warm and white-hot,
        // in patches that drift sideways.
        for y in h - 2..h {
            for x in 0..w {
                let patch = ((x as f32 * 0.08 + self.time * 0.7).sin() * 0.5 + 0.5) * 0.35;
                self.heat[y * w + x] = (0.65 + patch + self.rng.range(-0.25, 0.25)).clamp(0.0, 1.0);
            }
        }
        // Each cell takes the average of the cells below it, minus some cooling.
        // Rows are processed top-down, so "below" still holds last step's values.
        let cool = 1.6 / h as f32;
        let wind = (self.time * 0.4).sin();
        for y in 0..h - 2 {
            for x in 0..w {
                let xd = (x as i32 + if self.rng.f32() < 0.25 * wind.abs() { wind.signum() as i32 } else { 0 })
                    .clamp(0, w as i32 - 1) as usize;
                let l = xd.saturating_sub(1);
                let r = (xd + 1).min(w - 1);
                let b1 = (y + 1) * w;
                let b2 = (y + 2) * w;
                let avg = (self.heat[b1 + l] + self.heat[b1 + xd] + self.heat[b1 + r] + self.heat[b2 + xd]) * 0.25;
                self.heat[y * w + x] = (avg - cool * self.rng.range(0.2, 1.4)).max(0.0);
            }
        }
    }
}

/// black → ember red → orange → yellow → white
fn fire_color(t: f32) -> Rgb {
    let stops = [
        (0.0, Rgb::hex(0x000000)),
        (0.25, Rgb::hex(0x5c0b02)),
        (0.5, Rgb::hex(0xd23a0b)),
        (0.72, Rgb::hex(0xff9022)),
        (0.9, Rgb::hex(0xffe27a)),
        (1.0, Rgb::hex(0xfffbe8)),
    ];
    for pair in stops.windows(2) {
        let ((t0, c0), (t1, c1)) = (pair[0], pair[1]);
        if t <= t1 {
            return c0.lerp_oklab(c1, ((t - t0) / (t1 - t0)).clamp(0.0, 1.0));
        }
    }
    stops[stops.len() - 1].1
}

impl Scene for Fire {
    fn update(&mut self, dt: f32, _time: f64) {
        self.acc += dt;
        while self.acc >= STEP {
            self.acc -= STEP;
            self.time += STEP;
            self.step();
            // Embers rise from the hottest spots near the base.
            if self.w > 0 {
                for _ in 0..3 {
                    let x = self.rng.range(0.0, self.w as f32);
                    let y = self.h as f32 * self.rng.range(0.55, 0.9);
                    let mut p =
                        Particle::new(Vec3::new(x, y, 0.0), Vec3::new(self.rng.range(-3.0, 3.0), -12.0, 0.0), 2.5);
                    p.life = self.rng.range(1.2, 2.8);
                    p.drag = 0.6;
                    p.seed = self.rng.f32();
                    self.embers.spawn(p);
                }
            }
        }
        let t = self.time;
        // Embers move in cell units: buoyancy up, a little swirl.
        self.embers.update(dt, |p| Vec3::new(4.0 * (t * 2.0 + p.seed * 20.0).sin(), -6.0, 0.0));
    }

    fn draw(&mut self, f: &mut Frame) {
        let (w, h) = f.size();
        if (w, h) != (self.w, self.h) {
            self.w = w;
            self.h = h;
            self.heat = vec![0.0; w * h];
        }
        for y in 0..h {
            for x in 0..w {
                let v = self.heat[y * w + x];
                let cell = f.grid.get_mut(x as i32, y as i32).unwrap();
                let c = fire_color(v);
                cell.bg = c * 0.12;
                if v > 0.04 {
                    cell.ch = RAMP[((v * 10.0) as usize).clamp(1, 9)];
                    cell.fg = c * 1.25;
                    cell.glow = smoothstep(0.55, 1.0, v) * 1.6;
                }
            }
        }
        for p in &self.embers.particles {
            let pos = p.render_pos(f.alpha);
            let age = p.t();
            let c = Rgb::hex(0xffd27a).lerp_oklab(Rgb::hex(0xc02a08), age);
            let ch = if age < 0.4 {
                '*'
            } else if age < 0.75 {
                '·'
            } else {
                '.'
            };
            f.grid.overlay(pos.x as i32, pos.y as i32, ch, c * (1.4 - age), 1.6 * (1.0 - age));
        }
    }

    fn hud(&self) -> String {
        format!("{} embers", self.embers.len())
    }
}

fn main() -> Result<(), String> {
    rusteezy::run(Config::new("rusteezy — fire").with_args(), Fire::new())
}
