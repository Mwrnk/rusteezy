//! A spinning torus built from lit surface points, composited with the grid's
//! depth buffer. Brightness picks the glyph; a specular highlight blooms.
//!
//! ```text
//! cargo run --release --example torus
//! ```

use std::f32::consts::TAU;

use rusteezy::{
    Config, Frame, Rgb, Scene,
    glam::{Mat3, Vec3},
};

const RAMP: [char; 10] = [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];
const MAJOR: f32 = 1.0;
const MINOR: f32 = 0.42;
const CAM_DIST: f32 = 4.2;

struct Torus;

fn ring_color(a: f32) -> Rgb {
    let stops = [Rgb::hex(0xff5f6d), Rgb::hex(0xffc371), Rgb::hex(0x47e6c4), Rgb::hex(0x7c6cff)];
    let t = (a / TAU).rem_euclid(1.0) * 4.0;
    let i = t as usize % 4;
    stops[i].lerp_oklab(stops[(i + 1) % 4], t.fract())
}

impl Scene for Torus {
    fn draw(&mut self, f: &mut Frame) {
        let (gw, gh) = f.size();
        let (w, h) = (gw as f32 * f.cell_w, gh as f32 * f.cell_h);
        let t = f.time as f32;
        let rot = Mat3::from_rotation_z(0.45 * (t * 0.5).sin())
            * Mat3::from_rotation_x(1.05 + 0.3 * (t * 0.7).sin())
            * Mat3::from_rotation_y(t * 0.9);
        let light = Vec3::new(-0.5, 0.7, 0.6).normalize();
        let scale = h * 0.95; // pixels per unit at distance 1

        // Background: soft vertical gradient.
        let (top, bottom) = (Rgb::hex(0x05060f), Rgb::hex(0x10142a));
        for row in 0..gh as i32 {
            let c = top.lerp_oklab(bottom, row as f32 / gh as f32);
            for col in 0..gw as i32 {
                f.grid.get_mut(col, row).unwrap().bg = c;
            }
        }

        // Enough samples that neighbouring points land in neighbouring cells.
        let ring_px = TAU * (MAJOR + MINOR) * scale / (CAM_DIST - MAJOR - MINOR);
        let n_ring = ((ring_px / f.cell_w) * 1.6) as usize;
        let n_tube = ((TAU * MINOR * scale / (CAM_DIST - MAJOR - MINOR) / f.cell_w) * 1.6) as usize;

        for i in 0..n_ring {
            let a = i as f32 / n_ring as f32 * TAU;
            let (sa, ca) = a.sin_cos();
            let base = ring_color(a);
            for j in 0..n_tube {
                let b = j as f32 / n_tube as f32 * TAU;
                let (sb, cb) = b.sin_cos();
                let n = rot * Vec3::new(ca * cb, sb, sa * cb);
                let p = rot * Vec3::new(ca * (MAJOR + MINOR * cb), MINOR * sb, sa * (MAJOR + MINOR * cb));
                let z = CAM_DIST - p.z;
                let px = w * 0.5 + p.x * scale / z;
                let py = h * 0.5 - p.y * scale / z;
                let view = Vec3::new(-p.x, -p.y, CAM_DIST - p.z).normalize();
                if n.dot(view) <= 0.0 {
                    continue; // back face
                }
                let diffuse = n.dot(light).max(0.0);
                let spec = (2.0 * n.dot(light) * n - light).dot(view).max(0.0).powi(24);
                let lum = (0.12 + 0.88 * diffuse).min(1.0);
                let ch = RAMP[((lum * 10.0) as usize).clamp(1, 9)];
                let fg = base * (0.25 + 0.95 * lum) + Rgb::WHITE * spec;
                f.grid.plot((px / f.cell_w) as i32, (py / f.cell_h) as i32, z, ch, fg, spec * 2.5);
            }
        }
    }
}

fn main() -> Result<(), String> {
    rusteezy::run(Config::new("rusteezy — torus").with_args(), Torus)
}
