//! A ray-marched 3D scene, traced per cell on every core: merging metaballs over
//! a reflective floor, with soft shadows, ambient occlusion, specular and fog.
//!
//! ```text
//! cargo run --release --example raymarch
//! ```

#[path = "common/mod.rs"]
mod common;

use std::time::Instant;

use common::{smooth, stats, thousands};
use rayon::prelude::*;
use rusteezy::{Config, Frame, Rgb, Scene, glam::Vec3};

const RAMP: [char; 10] = [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];
const FLOOR_Y: f32 = -0.75;
const MAX_DIST: f32 = 14.0;

struct Blob {
    center: Vec3,
    radius: f32,
    color: Vec3,
}

fn blobs(t: f32) -> [Blob; 4] {
    let c = |hex: u32| {
        let c = Rgb::hex(hex);
        Vec3::new(c.r, c.g, c.b)
    };
    [
        Blob {
            center: Vec3::new((t * 0.9).sin() * 0.9, 0.1 + 0.35 * (t * 1.3).sin(), (t * 0.7).cos() * 0.6),
            radius: 0.55,
            color: c(0xff5f6d),
        },
        Blob {
            center: Vec3::new(
                (t * 0.6 + 2.0).sin() * 1.0,
                0.2 + 0.3 * (t * 1.1 + 1.0).sin(),
                (t * 0.8 + 1.0).cos() * 0.8,
            ),
            radius: 0.5,
            color: c(0xffc371),
        },
        Blob {
            center: Vec3::new(
                (t * 0.75 + 4.0).sin() * 0.8,
                0.15 + 0.4 * (t * 0.9 + 2.0).sin(),
                (t * 0.5 + 3.0).cos() * 0.9,
            ),
            radius: 0.48,
            color: c(0x47e6c4),
        },
        Blob {
            center: Vec3::new((t * 0.5 + 1.0).cos() * 0.5, 0.55 + 0.25 * (t * 1.7).sin(), (t * 0.65 + 5.0).sin() * 0.5),
            radius: 0.38,
            color: c(0x7c6cff),
        },
    ]
}

/// Polynomial smooth minimum: merges surfaces with a rounded seam.
fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    b + (a - b) * h - k * h * (1.0 - h)
}

/// Distance to the blobs (the floor is a plane and is intersected analytically).
fn sdf(p: Vec3, bl: &[Blob; 4]) -> f32 {
    let mut d = f32::MAX;
    for b in bl {
        d = smin(d, p.distance(b.center) - b.radius, 0.42);
    }
    d
}

fn normal(p: Vec3, bl: &[Blob; 4]) -> Vec3 {
    let e = 0.002;
    let k =
        [Vec3::new(1.0, -1.0, -1.0), Vec3::new(-1.0, -1.0, 1.0), Vec3::new(-1.0, 1.0, -1.0), Vec3::new(1.0, 1.0, 1.0)];
    k.iter().fold(Vec3::ZERO, |n, &k| n + k * sdf(p + k * e, bl)).normalize()
}

/// All blobs stay inside this sphere, so rays that miss it skip marching entirely.
const BOUND_C: Vec3 = Vec3::new(0.0, 0.2, 0.0);
const BOUND_R: f32 = 2.1;

/// Entry and exit distances of a ray through the bounding sphere.
fn bounds(ro: Vec3, rd: Vec3) -> Option<(f32, f32)> {
    let oc = ro - BOUND_C;
    let b = oc.dot(rd);
    let disc = b * b - (oc.length_squared() - BOUND_R * BOUND_R);
    (disc > 0.0).then(|| (-b - disc.sqrt(), -b + disc.sqrt())).filter(|&(_, t1)| t1 > 0.0)
}

fn march(ro: Vec3, rd: Vec3, t_max: f32, bl: &[Blob; 4], steps: u32) -> Option<f32> {
    let (t0, t1) = bounds(ro, rd)?;
    let (mut t, end) = (t0.max(0.0), t1.min(t_max));
    for _ in 0..steps {
        if t > end {
            return None;
        }
        let d = sdf(ro + rd * t, bl);
        if d < 0.0015 * t.max(1.0) {
            return Some(t);
        }
        t += d;
    }
    None
}

fn floor_hit(ro: Vec3, rd: Vec3) -> f32 {
    if rd.y < -1e-4 { ((FLOOR_Y - ro.y) / rd.y).min(MAX_DIST) } else { MAX_DIST }
}

fn soft_shadow(p: Vec3, l: Vec3, bl: &[Blob; 4]) -> f32 {
    let Some((t0, t1)) = bounds(p, l) else { return 1.0 };
    let (mut res, mut t) = (1.0f32, t0.max(0.03));
    for _ in 0..24 {
        let h = sdf(p + l * t, bl);
        res = res.min(9.0 * h / t);
        t += h.clamp(0.02, 0.4);
        if res < 0.01 || t > t1 {
            break;
        }
    }
    res.clamp(0.0, 1.0)
}

fn ambient_occlusion(p: Vec3, n: Vec3, bl: &[Blob; 4]) -> f32 {
    let mut occ = 0.0;
    for i in 1..=4 {
        let h = 0.06 * i as f32;
        let q = p + n * h;
        occ += (h - sdf(q, bl).min(q.y - FLOOR_Y)) / i as f32;
    }
    (1.0 - occ * 2.5).clamp(0.0, 1.0)
}

/// Color the blobs by blending each one's color with a falloff on its distance.
fn blob_color(p: Vec3, bl: &[Blob; 4]) -> Vec3 {
    let (mut c, mut wsum) = (Vec3::ZERO, 0.0);
    for b in bl {
        let w = (-(p.distance(b.center) - b.radius).max(0.0) * 9.0).exp();
        c += b.color * w;
        wsum += w;
    }
    c / wsum.max(1e-4)
}

const LIGHT: Vec3 = Vec3::new(0.55, 0.75, -0.36);
const SKY: Vec3 = Vec3::new(0.012, 0.014, 0.035);

/// Shade a ray; `bounce` allows one reflection off the floor.
fn shade(ro: Vec3, rd: Vec3, bl: &[Blob; 4], bounce: bool) -> (Vec3, f32) {
    let t_floor = floor_hit(ro, rd);
    let blob = march(ro, rd, t_floor, bl, if bounce { 72 } else { 40 });
    let t = blob.unwrap_or(t_floor);
    if t >= MAX_DIST {
        return (SKY, 0.0);
    }
    let p = ro + rd * t;
    let l = LIGHT.normalize();

    let mut color;
    let mut glow = 0.0;
    if blob.is_none() {
        // Floor: checkerboard, blob shadows, contact occlusion, one reflection.
        let n = Vec3::Y;
        let checker = ((p.x * 1.2).floor() + (p.z * 1.2).floor()).rem_euclid(2.0);
        let base = Vec3::splat(0.035 + 0.03 * checker);
        let shadow = soft_shadow(p + n * 0.01, l, bl);
        let ao = ambient_occlusion(p, n, bl);
        color = base * (0.2 * ao + 0.9 * n.dot(l).max(0.0) * shadow);
        if bounce {
            let r = rd - 2.0 * rd.dot(n) * n;
            let (refl, _) = shade(p + n * 0.01, r, bl, false);
            let fresnel = 0.25 + 0.5 * (1.0 + rd.dot(n)).powi(3);
            color += refl * fresnel * ao;
        }
    } else {
        let n = normal(p, bl);
        let shadow = soft_shadow(p + n * 0.01, l, bl);
        let ao = ambient_occlusion(p, n, bl);
        let diffuse = n.dot(l).max(0.0) * shadow;
        let spec = n.dot((l - rd).normalize()).max(0.0).powi(48) * shadow;
        let base = blob_color(p, bl);
        let rim = (1.0 + rd.dot(n)).powi(3) * 0.5;
        color = base * (0.12 * ao + 1.1 * diffuse) + Vec3::splat(spec * 1.2) + base * rim * ao;
        glow = spec * 2.2;
    }
    let fog = 1.0 - (-0.012 * t * t).exp();
    (color.lerp(SKY, fog), glow * (1.0 - fog))
}

struct Raymarch {
    trace_ms: f32,
}

impl Scene for Raymarch {
    fn draw(&mut self, f: &mut Frame) {
        let start = Instant::now();
        let (gw, gh) = f.size();
        let (w, h) = (gw as f32 * f.cell_w, gh as f32 * f.cell_h);
        let t = f.time as f32;
        let bl = blobs(t);

        // Orbiting camera.
        let a = t * 0.22;
        let ro = Vec3::new(a.sin() * 3.6, 1.0 + 0.25 * (t * 0.3).sin(), a.cos() * 3.6);
        let fwd = (Vec3::new(0.0, 0.0, 0.0) - ro).normalize();
        let right = fwd.cross(Vec3::Y).normalize();
        let up = right.cross(fwd);
        let tan_half = 0.55;
        let (cw, ch) = (f.cell_w, f.cell_h);

        // One ray per cell, one row per task.
        f.grid.cells_mut().par_chunks_mut(gw).enumerate().for_each(|(row, cells)| {
            let py = (row as f32 + 0.5) * ch;
            for (col, cell) in cells.iter_mut().enumerate() {
                let px = (col as f32 + 0.5) * cw;
                let sx = (px - w * 0.5) / (h * 0.5) * tan_half;
                let sy = (h * 0.5 - py) / (h * 0.5) * tan_half;
                let rd = (fwd + right * sx + up * sy).normalize();
                let (c, glow) = shade(ro, rd, &bl, true);
                let c = Rgb::new(c.x, c.y, c.z);
                let lum = c.luminance().powf(0.6).min(1.0);
                cell.ch = RAMP[((lum * 10.0) as usize).clamp(0, 9)];
                cell.fg = c * 1.8;
                cell.bg = c * 0.35;
                cell.glow = glow;
            }
        });

        self.trace_ms = smooth(self.trace_ms, start.elapsed().as_secs_f32() * 1000.0);
        stats(
            f,
            &format!(
                " raymarch · {} rays + shadows + AO + reflections · {} threads · {:.2} ms ",
                thousands(gw * gh),
                rayon::current_num_threads(),
                self.trace_ms
            ),
        );
    }
}

fn main() -> Result<(), String> {
    let config = Config { font_px: 7.0, ..Config::new("rusteezy — raymarch") }.with_args();
    rusteezy::run(config, Raymarch { trace_ms: 0.0 })
}
