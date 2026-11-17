//! A spiral galaxy of 300,000 stars, all re-projected and splatted into the grid
//! every frame across every CPU core. Density picks the glyph; the core blooms.
//!
//! ```text
//! cargo run --release --example galaxy [-- --stars 1000000]
//! ```

use std::{f32::consts::PI, time::Instant};

#[path = "common/mod.rs"]
mod common;

use common::{arg, smooth, stats, thousands};
use rayon::prelude::*;
use rusteezy::{Config, Frame, Rgb, Rng, Scene, ease::smoothstep};

const RAMP: [char; 10] = [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];

struct Star {
    r: f32,
    theta: f32,
    y: f32,
    /// Angular speed: inner stars orbit faster (differential rotation).
    omega: f32,
    color: [f32; 3],
}

struct Galaxy {
    stars: Vec<Star>,
    /// Per-cell accumulated (r, g, b, brightness).
    accum: Vec<[f32; 4]>,
    splat_ms: f32,
}

/// Roughly normal, mean 0, sd ≈ 1.
fn gauss(rng: &mut Rng) -> f32 {
    (rng.f32() + rng.f32() + rng.f32() + rng.f32() - 2.0) * 1.7
}

fn rgb(hex: u32, k: f32) -> [f32; 3] {
    let c = Rgb::hex(hex) * k;
    [c.r, c.g, c.b]
}

impl Galaxy {
    fn new(n: usize) -> Self {
        let mut rng = Rng::new(2026);
        let stars = (0..n)
            .map(|_| {
                if rng.f32() < 0.1 {
                    // Bulge: a warm, dense core.
                    let r = rng.f32().sqrt() * rng.f32() * 0.42;
                    Star {
                        r,
                        theta: rng.range(0.0, 2.0 * PI),
                        y: gauss(&mut rng) * 0.06 * (1.0 - r / 0.4),
                        omega: 0.5,
                        color: rgb(if rng.f32() < 0.6 { 0xffd9a0 } else { 0xffb36b }, 1.2),
                    }
                } else {
                    // Disk: two logarithmic spiral arms with scatter.
                    let arm = if rng.f32() < 0.5 { 0.0 } else { PI };
                    let r = (0.16 - rng.f32().max(1e-4).ln() * 0.36).min(1.7);
                    let spread = gauss(&mut rng) * 0.28 / (0.6 + r);
                    let kind = rng.f32();
                    let color = match kind {
                        k if k < 0.04 => rgb(0xff6b9d, 1.8), // star-forming regions
                        k if k < 0.25 => rgb(0xfff0c8, 1.0),
                        _ => rgb(0xb8d0ff, 1.0),
                    };
                    Star {
                        r: r * (1.0 + gauss(&mut rng) * 0.04),
                        theta: arm + 2.2 * (r / 0.16).ln() + spread,
                        y: gauss(&mut rng) * 0.025,
                        omega: 0.08 + 0.05 / (r + 0.25),
                        color,
                    }
                }
            })
            .collect();
        Self { stars, accum: Vec::new(), splat_ms: 0.0 }
    }
}

impl Scene for Galaxy {
    fn draw(&mut self, f: &mut Frame) {
        let start = Instant::now();
        f.settings.bloom_intensity = 0.6;
        let (gw, gh) = f.size();
        let (w, h) = (gw as f32 * f.cell_w, gh as f32 * f.cell_h);
        let cells = gw * gh;
        let t = f.time as f32;

        // Camera: tilted view that slowly rocks.
        let tilt = 1.1 + 0.18 * (t * 0.23).sin();
        let (st, ct) = tilt.sin_cos();
        let (dist, focal) = (3.0, 1.55);
        let (cw, ch) = (f.cell_w, f.cell_h);

        // Splat every star into a per-chunk buffer, then sum the buffers.
        let chunk = self.stars.len() / (rayon::current_num_threads() * 2) + 1;
        let accum = self
            .stars
            .par_chunks(chunk)
            .map(|stars| {
                let mut buf = vec![[0.0f32; 4]; cells];
                for s in stars {
                    let th = s.theta + s.omega * t;
                    let (x, z) = (s.r * th.cos(), s.r * th.sin());
                    let y2 = s.y * ct - z * st;
                    let z2 = s.y * st + z * ct;
                    let k = focal / (dist - z2);
                    let px = w * 0.5 + x * k * h * 0.5;
                    let py = h * 0.5 - y2 * k * h * 0.5;
                    if px < 0.0 || py < 0.0 || px >= w || py >= h {
                        continue;
                    }
                    let i = (py / ch) as usize * gw + (px / cw) as usize;
                    let a = &mut buf[i.min(cells - 1)];
                    a[0] += s.color[0];
                    a[1] += s.color[1];
                    a[2] += s.color[2];
                    a[3] += 1.0;
                }
                buf
            })
            .reduce_with(|mut a, b| {
                for (x, y) in a.iter_mut().zip(&b) {
                    for c in 0..4 {
                        x[c] += y[c];
                    }
                }
                a
            })
            .unwrap_or_default();
        self.accum = accum;

        // Density → glyph, average color → fg, very dense → glow.
        let k = cells as f32 / self.stars.len() as f32 * 0.55;
        let space = Rgb::hex(0x020208);
        for (cell, a) in f.grid.cells_mut().iter_mut().zip(&self.accum) {
            cell.bg = space;
            if a[3] <= 0.0 {
                continue;
            }
            let lum = 1.0 - (-a[3] * k).exp();
            let c = Rgb::new(a[0] / a[3], a[1] / a[3], a[2] / a[3]);
            cell.ch = RAMP[((lum * 10.0) as usize).clamp(1, 9)];
            cell.fg = c * (0.25 + 0.85 * lum);
            cell.bg = space + c * (lum * lum * 0.12);
            cell.glow = smoothstep(0.93, 1.0, lum) * 0.9;
        }

        let ms = start.elapsed().as_secs_f32() * 1000.0;
        self.splat_ms = smooth(self.splat_ms, ms);
        stats(
            f,
            &format!(
                " galaxy · {} stars · {} threads · splat + shade {:.2} ms ",
                thousands(self.stars.len()),
                rayon::current_num_threads(),
                self.splat_ms
            ),
        );
    }
}

fn main() -> Result<(), String> {
    let n = arg("--stars").unwrap_or(300_000);
    rusteezy::run(Config::new("rusteezy — galaxy").with_args(), Galaxy::new(n))
}
