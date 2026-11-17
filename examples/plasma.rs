//! A classic sine plasma: a scalar field per cell mapped to glyphs and a
//! cycling OKLab palette, with glowing ridges.
//!
//! ```text
//! cargo run --release --example plasma
//! ```
//!
//! Keys: P next palette · Esc quit

use std::f32::consts::TAU;

use rusteezy::{Config, Frame, Key, Rgb, Scene};

const RAMP: [char; 10] = [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];

const PALETTES: [[u32; 4]; 3] = [
    [0x2b1055, 0xd53369, 0xffb75e, 0x31c6c4], // synthwave
    [0x061a40, 0x0353a4, 0x00c2a8, 0xb9f6ca], // deep sea
    [0x1b0f0a, 0x8a2c0d, 0xf0a202, 0xfef3c7], // ember
];

struct Plasma {
    palette: usize,
}

fn cycle(p: &[u32; 4], t: f32) -> Rgb {
    let t = t.rem_euclid(1.0) * 4.0;
    let i = t as usize % 4;
    Rgb::hex(p[i]).lerp_oklab(Rgb::hex(p[(i + 1) % 4]), t.fract())
}

impl Scene for Plasma {
    fn draw(&mut self, f: &mut Frame) {
        let (w, h) = f.size();
        let t = f.time as f32;
        let hp = h as f32 * f.cell_h; // normalize by height so the field isn't stretched
        let (cx, cy) = (0.9 + 0.5 * (t * 0.31).cos(), 0.5 + 0.3 * (t * 0.43).sin());
        let pal = &PALETTES[self.palette];
        for row in 0..h as i32 {
            for col in 0..w as i32 {
                let x = (col as f32 + 0.5) * f.cell_w / hp;
                let y = (row as f32 + 0.5) * f.cell_h / hp;
                let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
                let v = (x * 6.0 + t).sin()
                    + (y * 5.0 - t * 1.3).sin()
                    + ((x + y) * 4.0 + t * 0.7).sin()
                    + (d * 11.0 - t * 2.0).sin();
                let v = (v + 4.0) / 8.0; // 0..1
                let lum = 0.5 + 0.5 * (v * TAU * 2.0 + t * 0.8).sin();
                let c = cycle(pal, v + t * 0.05);
                let cell = f.grid.get_mut(col, row).unwrap();
                cell.ch = RAMP[((lum * 10.0) as usize).clamp(1, 9)];
                cell.fg = c * (0.35 + 0.65 * lum);
                cell.bg = c * 0.1;
                cell.glow = lum.powi(10) * 0.9;
            }
        }
    }

    fn key(&mut self, key: &Key) {
        if let Key::Character(c) = key
            && c.eq_ignore_ascii_case("p")
        {
            self.palette = (self.palette + 1) % PALETTES.len();
        }
    }

    fn hud(&self) -> String {
        "P palette".into()
    }
}

fn main() -> Result<(), String> {
    rusteezy::run(Config::new("rusteezy — plasma").with_args(), Plasma { palette: 0 })
}
