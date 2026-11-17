//! 200,000 particles drifting through an animated, swirling flow field. They
//! leave fading trails, and each cell's glyph points along the local flow.
//!
//! ```text
//! cargo run --release --example flow [-- --particles 500000]
//! ```

#[path = "common/mod.rs"]
mod common;

use std::{f32::consts::TAU, time::Instant};

use common::{arg, smooth, stats, thousands};
use rayon::prelude::*;
use rusteezy::{Config, Frame, Rgb, Scene, rng::hash01};

/// Particle: x, y (in screen-height units), remaining life.
type P = [f32; 3];
/// Trail cell: r, g, b, vx, vy.
type T = [f32; 5];

const SPEED: f32 = 0.16;
const EMITTERS: usize = 12;

struct Flow {
    ps: Vec<P>,
    trail: Vec<T>,
    size: (usize, usize),
    frame: u32,
    palette: Vec<Rgb>,
    sim_ms: f32,
}

/// Divergence-free velocity: the curl of an animated stream function, so
/// particles swirl around without bunching up or thinning out.
fn velocity(x: f32, y: f32, t: f32) -> (f32, f32) {
    let (a, b) = (1.7 * x + 0.6 * t, 2.1 * y - 0.4 * t);
    let (c, d) = (2.9 * (x + y) + 0.9 * t, 1.3 * x - 2.2 * y);
    let (e, g) = (4.7 * x - 0.5 * t, 4.1 * y + 0.7 * t);
    let (sa, ca) = a.sin_cos();
    let (sb, cb) = b.sin_cos();
    let (sc, cc) = c.sin_cos();
    let (sd, cd) = d.sin_cos();
    let (se, ce) = e.sin_cos();
    let (sg, cg) = g.sin_cos();
    let dpsi_dx = 1.7 * ca * cb + 0.6 * (2.9 * cc * cd - 1.3 * sc * sd) + 0.35 * 4.7 * ce * sg;
    let dpsi_dy = -2.1 * sa * sb + 0.6 * (2.9 * cc * cd + 2.2 * sc * sd) + 0.35 * 4.1 * se * cg;
    (dpsi_dy, -dpsi_dx)
}

/// Emitter `e`'s position at time `t`: each drifts on its own slow loop.
fn emitter(e: usize, t: f32, aspect: f32) -> (f32, f32) {
    let k = e as f32;
    let x = aspect * (0.5 + 0.38 * (t * 0.11 + k * 2.4).sin());
    let y = 0.5 + 0.36 * (t * 0.083 + k * 1.7).cos();
    (x, y)
}

/// New particle near its emitter (particle `i` belongs to emitter `i % EMITTERS`).
fn respawn(i: usize, frame: u32, t: f32, aspect: f32) -> P {
    let h = |k: u32| hash01((i as u32).wrapping_mul(2_654_435_761) ^ frame.wrapping_mul(97 + k) ^ k);
    let (ex, ey) = emitter(i % EMITTERS, t, aspect);
    let (a, r) = (h(1) * TAU, h(2).sqrt() * 0.025);
    [ex + a.cos() * r, ey + a.sin() * r, 1.5 + h(3) * 5.0]
}

impl Flow {
    fn new(n: usize) -> Self {
        let stops = [0x4cc9f0, 0x7b2ff7, 0xf72585, 0xffb703];
        let palette = (0..EMITTERS)
            .map(|i| {
                let t = i as f32 / EMITTERS as f32 * 4.0;
                let k = t as usize;
                Rgb::hex(stops[k % 4]).lerp_oklab(Rgb::hex(stops[(k + 1) % 4]), t.fract())
            })
            .collect();
        Self { ps: vec![[0.0; 3]; n], trail: Vec::new(), size: (0, 0), frame: 0, palette, sim_ms: 0.0 }
    }
}

impl Scene for Flow {
    fn draw(&mut self, f: &mut Frame) {
        let start = Instant::now();
        let (gw, gh) = f.size();
        let cells = gw * gh;
        let aspect = gw as f32 * f.cell_w / (gh as f32 * f.cell_h);
        let t = f.time as f32;
        if self.size != (gw, gh) {
            self.size = (gw, gh);
            self.trail = vec![[0.0; 5]; cells];
            let frame = self.frame;
            self.ps.par_iter_mut().enumerate().for_each(|(i, p)| {
                *p = respawn(i, frame, t, aspect);
                p[2] *= hash01(i as u32); // stagger lifetimes so emitters don't pulse
            });
        }
        let dt = f.dt.min(1.0 / 20.0);
        self.frame = self.frame.wrapping_add(1);
        let frame = self.frame;

        // Advect every particle (parallel).
        self.ps.par_iter_mut().enumerate().for_each(|(i, p)| {
            let (vx, vy) = velocity(p[0] * 1.3, p[1] * 1.3, t * 0.35);
            p[0] += vx * SPEED * dt;
            p[1] += vy * SPEED * dt;
            p[2] -= dt;
            if p[2] <= 0.0 || p[0] < 0.0 || p[1] < 0.0 || p[0] >= aspect || p[1] >= 1.0 {
                *p = respawn(i, frame, t, aspect);
            }
        });

        // Deposit color and direction into per-chunk buffers, then merge.
        let (cw_n, ch_n) = (gw as f32 / aspect, gh as f32);
        let palette = &self.palette;
        let chunk = self.ps.len() / (rayon::current_num_threads() * 2) + 1;
        let deposit = self
            .ps
            .par_chunks(chunk)
            .enumerate()
            .map(|(ci, ps)| {
                let mut buf = vec![[0.0f32; 5]; cells];
                for (j, p) in ps.iter().enumerate() {
                    let i = ci * chunk + j;
                    let (vx, vy) = velocity(p[0] * 1.3, p[1] * 1.3, t * 0.35);
                    let col = ((p[0] * cw_n) as usize).min(gw - 1);
                    let row = ((p[1] * ch_n) as usize).min(gh - 1);
                    let c = palette[i % EMITTERS];
                    let fade = p[2].min(1.0); // fade in/out over the last second of life
                    let b = &mut buf[row * gw + col];
                    b[0] += c.r * fade;
                    b[1] += c.g * fade;
                    b[2] += c.b * fade;
                    b[3] += vx;
                    b[4] += vy;
                }
                buf
            })
            .reduce_with(|mut a, b| {
                for (x, y) in a.iter_mut().zip(&b) {
                    for k in 0..5 {
                        x[k] += y[k];
                    }
                }
                a
            })
            .unwrap_or_default();

        // Trails decay over time; new deposits add on top.
        let decay = (-dt * 2.5).exp();
        let k = cells as f32 / self.ps.len() as f32 * 0.22;
        let bg = Rgb::hex(0x04030a);
        for ((cell, tr), d) in f.grid.cells_mut().iter_mut().zip(self.trail.iter_mut()).zip(&deposit) {
            for c in 0..5 {
                tr[c] = tr[c] * decay + d[c];
            }
            let energy = tr[0] + tr[1] + tr[2];
            cell.bg = bg;
            if energy < 1e-3 {
                continue;
            }
            let lum = 1.0 - (-energy * k).exp();
            let c = Rgb::new(tr[0], tr[1], tr[2]) * (1.0 / energy) * 2.2;
            cell.ch = if lum < 0.18 {
                '.'
            } else {
                let a = (-tr[4]).atan2(tr[3]).to_degrees().rem_euclid(180.0);
                match a {
                    a if !(22.5..157.5).contains(&a) => '-',
                    a if a < 67.5 => '/',
                    a if a < 112.5 => '|',
                    _ => '\\',
                }
            };
            cell.fg = c * (0.25 + lum);
            cell.bg = bg + c * (lum * 0.08);
            cell.glow = lum.powi(4) * 1.3;
        }

        self.sim_ms = smooth(self.sim_ms, start.elapsed().as_secs_f32() * 1000.0);
        stats(
            f,
            &format!(
                " flow · {} particles · {} threads · advect + splat + shade {:.2} ms ",
                thousands(self.ps.len()),
                rayon::current_num_threads(),
                self.sim_ms
            ),
        );
    }
}

fn main() -> Result<(), String> {
    let n = arg("--particles").unwrap_or(200_000);
    rusteezy::run(Config::new("rusteezy — flow").with_args(), Flow::new(n))
}
