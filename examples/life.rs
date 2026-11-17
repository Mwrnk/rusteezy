//! Conway's Game of Life on a dense grid (small font = lots of cells), with
//! newborn cells glowing and dead ones leaving embers. Space toggles turbo.
//!
//! ```text
//! cargo run --release --example life [-- --font 4]
//! ```

#[path = "common/mod.rs"]
mod common;

use std::time::Instant;

use common::{smooth, stats, thousands};
use rayon::prelude::*;
use rusteezy::{Config, Frame, Key, NamedKey, Rgb, Rng, Scene, rng::hash2};

struct Life {
    w: usize,
    h: usize,
    alive: Vec<u8>,
    next: Vec<u8>,
    /// Generations alive (saturating).
    age: Vec<u16>,
    /// Ember brightness left behind by dead cells.
    heat: Vec<f32>,
    rng: Rng,
    acc: f32,
    gens_per_sec: f32,
    generation: u64,
    step_ms: f32,
    population: usize,
}

impl Life {
    fn new() -> Self {
        Self {
            w: 0,
            h: 0,
            alive: Vec::new(),
            next: Vec::new(),
            age: Vec::new(),
            heat: Vec::new(),
            rng: Rng::new(1970),
            acc: 0.0,
            gens_per_sec: 30.0,
            generation: 0,
            step_ms: 0.0,
            population: 0,
        }
    }

    fn reset(&mut self, w: usize, h: usize) {
        (self.w, self.h) = (w, h);
        let n = w * h;
        self.alive = (0..n).map(|_| u8::from(self.rng.f32() < 0.32)).collect();
        self.next = vec![0; n];
        self.age = vec![0; n];
        self.heat = vec![0.0; n];
    }

    /// Drop a random soup somewhere to keep things lively.
    fn sprinkle(&mut self) {
        let (cx, cy) = ((self.rng.f32() * self.w as f32) as usize, (self.rng.f32() * self.h as f32) as usize);
        for dy in 0..14 {
            for dx in 0..28 {
                let (x, y) = ((cx + dx) % self.w, (cy + dy) % self.h);
                if self.rng.f32() < 0.45 {
                    self.alive[y * self.w + x] = 1;
                }
            }
        }
    }

    fn step(&mut self) {
        let start = Instant::now();
        let (w, h) = (self.w, self.h);
        let cur = &self.alive;
        // Next generation, one row per task (the grid wraps around).
        self.next.par_chunks_mut(w).enumerate().for_each(|(y, out)| {
            let up = ((y + h - 1) % h) * w;
            let mid = y * w;
            let down = ((y + 1) % h) * w;
            for x in 0..w {
                let l = (x + w - 1) % w;
                let r = (x + 1) % w;
                let n = cur[up + l]
                    + cur[up + x]
                    + cur[up + r]
                    + cur[mid + l]
                    + cur[mid + r]
                    + cur[down + l]
                    + cur[down + x]
                    + cur[down + r];
                out[x] = u8::from(n == 3 || (n == 2 && cur[mid + x] == 1));
            }
        });
        std::mem::swap(&mut self.alive, &mut self.next);
        // Ages and embers.
        let prev = &self.next;
        self.population = self
            .alive
            .par_iter()
            .zip(prev.par_iter())
            .zip(self.age.par_iter_mut().zip(self.heat.par_iter_mut()))
            .map(|((&now, &was), (age, heat))| {
                if now == 1 {
                    *age = if was == 1 { age.saturating_add(1) } else { 0 };
                } else {
                    if was == 1 {
                        *heat = 1.0;
                    }
                    *heat *= 0.86;
                }
                now as usize
            })
            .sum();
        self.generation += 1;
        if self.generation.is_multiple_of(90) || self.population < self.w * self.h / 40 {
            self.sprinkle();
        }
        self.step_ms = smooth(self.step_ms, start.elapsed().as_secs_f32() * 1000.0);
    }
}

impl Scene for Life {
    fn update(&mut self, dt: f32, _time: f64) {
        if self.w == 0 {
            return;
        }
        self.acc += dt;
        let step = 1.0 / self.gens_per_sec;
        while self.acc >= step {
            self.acc -= step;
            self.step();
        }
    }

    fn draw(&mut self, f: &mut Frame) {
        let (w, h) = f.size();
        if (w, h) != (self.w, self.h) {
            self.reset(w, h);
        }
        let young = Rgb::hex(0xeafcff);
        let mid = Rgb::hex(0x37c4ff);
        let old = Rgb::hex(0x5b3cff);
        let ember = Rgb::hex(0xff6a3d);
        let bg = Rgb::hex(0x05060d);
        let cells = f.grid.cells_mut();
        for (i, cell) in cells.iter_mut().enumerate() {
            cell.bg = bg;
            if self.alive[i] == 1 {
                let a = self.age[i] as f32;
                let (ch, c, glow) = match self.age[i] {
                    0 => ('@', young, 1.3),
                    1..=5 => ('#', young.lerp_oklab(mid, a / 6.0), 0.5),
                    _ => ('o', mid.lerp_oklab(old, ((a - 6.0) / 40.0).min(1.0)), 0.1),
                };
                cell.ch = ch;
                cell.fg = c;
                cell.glow = glow;
            } else if self.heat[i] > 0.03 {
                let k = self.heat[i];
                cell.ch = if hash2(i as i32, 3) < 0.5 { '.' } else { ',' };
                cell.fg = ember * (0.3 + k);
                cell.glow = k * 0.5;
            }
        }
        stats(
            f,
            &format!(
                " life · {}×{} = {} cells · {:.0} gen/s · step {:.2} ms · pop {} · Space turbo ",
                w,
                h,
                thousands(w * h),
                self.gens_per_sec,
                self.step_ms,
                thousands(self.population),
            ),
        );
    }

    fn key(&mut self, key: &Key) {
        if let Key::Named(NamedKey::Space) = key {
            self.gens_per_sec = if self.gens_per_sec > 30.0 { 30.0 } else { 240.0 };
        }
    }
}

fn main() -> Result<(), String> {
    let config = Config { font_px: 5.0, ..Config::new("rusteezy — life") }.with_args();
    rusteezy::run(config, Life::new())
}
