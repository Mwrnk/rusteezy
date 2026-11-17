//! The rusteezy crab: the logo bitmap (`assets/logo/crab.txt`) rendered as
//! character blocks. It side-steps, snaps its claws and blinks, and a shine
//! sweeps across its shell while bubbles rise.
//!
//! ```text
//! cargo run --release --example crab
//! ```

use std::f32::consts::TAU;

use rusteezy::{
    Config, Frame, Particle, ParticleSystem, Rgb, Rng, Scene,
    ease::{self, smoothstep},
    glam::Vec3,
    rng::hash2,
};

const CRAB: &str = include_str!("../assets/logo/crab.txt");
const TITLE: &str = "rusteezy";
const SUBTITLE: &str = "a gpu ascii renderer";
/// Every motion repeats with this period, so recordings loop cleanly.
const LOOP: f32 = 6.0;

struct Crab {
    rows: Vec<Vec<char>>,
    bubbles: ParticleSystem,
    rng: Rng,
    width: f32,
}

fn pixel_color(c: char) -> Option<Rgb> {
    Some(Rgb::hex(match c {
        'O' => 0xff6b2c,
        'L' => 0xffa266,
        'D' => 0xb8401a,
        'W' => 0xffffff,
        'K' => 0x1b1b1f,
        _ => return None,
    }))
}

impl Crab {
    fn new() -> Self {
        let rows = CRAB.lines().filter(|l| !l.is_empty() && !l.starts_with('#')).map(|l| l.chars().collect()).collect();
        Self { rows, bubbles: ParticleSystem::new(400), rng: Rng::new(9), width: 120.0 }
    }

    /// The logo pixel at (x, y) at loop time `t`, with claw snaps and blinks applied.
    fn pixel(&self, x: usize, y: usize, t: f32) -> char {
        let c = self.rows[y][x];
        let w = self.rows[y].len();
        let snap = (t % 2.0) < 0.22; // claws close twice per loop
        let blink = (t - 3.3).abs() < 0.08;
        match c {
            '.' if snap && y == 0 && (x == 3 || x == w - 4) => 'O',
            'W' | 'K' if blink => 'O',
            c => c,
        }
    }
}

impl Scene for Crab {
    fn update(&mut self, dt: f32, time: f64) {
        let t = time as f32;
        if self.rng.f32() < dt * 6.0 {
            let x = self.rng.range(0.0, self.width);
            let mut p = Particle::new(Vec3::new(x, 0.0, 0.0), Vec3::new(0.0, self.rng.range(4.0, 8.0), 0.0), 6.0);
            p.seed = self.rng.f32();
            p.size = self.rng.f32();
            self.bubbles.spawn(p);
        }
        self.bubbles.update(dt, |p| Vec3::new(3.0 * (t * 2.5 + p.seed * TAU).sin(), 0.0, 0.0));
    }

    fn draw(&mut self, f: &mut Frame) {
        let (gw, gh) = f.size();
        self.width = gw as f32;
        let time = f.time as f32;
        let t = time % LOOP;
        let sand_row = gh as i32 - (gh as i32 / 7).max(2);

        // Water with drifting caustics, and a sand floor.
        let (deep, shallow) = (Rgb::hex(0x02121f), Rgb::hex(0x06324a));
        for row in 0..gh as i32 {
            for col in 0..gw as i32 {
                let v = row as f32 / gh as f32;
                let (x, y) = (col as f32 * 0.5, row as f32);
                let caustic =
                    (((x + y) * 0.23 + time * 0.9).sin() + ((x - y) * 0.19 - time * 0.7).sin()).max(0.0) * 0.5;
                let cell = f.grid.get_mut(col, row).unwrap();
                cell.bg = shallow.lerp_oklab(deep, v) + Rgb::hex(0x0a3a4a) * (caustic * 0.22 * (1.0 - v));
                if row >= sand_row {
                    let n = hash2(col, row);
                    cell.bg = Rgb::hex(0x2a2014).lerp_oklab(Rgb::hex(0x14100a), (row - sand_row) as f32 / 4.0);
                    cell.ch = if n < 0.3 {
                        '.'
                    } else if n < 0.38 {
                        ','
                    } else {
                        ' '
                    };
                    cell.fg = Rgb::hex(0x8a7350) * 0.6;
                }
            }
        }

        // Crab placement: each logo pixel is 2k columns × k rows (square on screen).
        let (lw, lh) = (self.rows[0].len() as i32, self.rows.len() as i32);
        let k = ((gw as f32 * 0.5 / (lw * 2) as f32).min(gh as f32 * 0.45 / lh as f32)).floor().max(1.0) as i32;
        let walk = (t / LOOP * TAU).sin() * gw as f32 * 0.16;
        let stepping = ((t * 4.0) as i32) % 2;
        let bob = stepping;
        let x0 = gw as i32 / 2 - lw * k + walk as i32;
        let y0 = sand_row - lh * k - 1 + bob;

        // Soft shadow on the sand.
        for col in (x0 + k)..(x0 + lw * 2 * k - k) {
            if let Some(c) = f.grid.get_mut(col, sand_row) {
                c.bg = c.bg * 0.55;
            }
        }

        let sheen_pos = ease::in_out_sine((t % 3.0) / 3.0) * (lw + lh) as f32 * 1.6 - lh as f32;
        for y in 0..lh as usize {
            for x in 0..lw as usize {
                let c = self.pixel(x, y, t);
                let Some(color) = pixel_color(c) else { continue };
                // Legs (bottom two rows) shuffle sideways in alternate steps.
                let leg_shift =
                    if y as i32 >= lh - 2 { if (x < lw as usize / 2) == (stepping == 0) { 1 } else { -1 } } else { 0 };
                let sheen = if matches!(c, 'O' | 'L') {
                    1.0 - smoothstep(0.0, 1.6, ((x + y) as f32 - sheen_pos).abs())
                } else {
                    0.0
                };
                let fg = color.lerp_oklab(Rgb::hex(0xfff1dc), sheen * 0.6);
                let glow = if c == 'W' { 0.35 } else { sheen * 1.3 };
                for dy in 0..k {
                    for dx in 0..2 * k {
                        let col = x0 + x as i32 * 2 * k + dx + leg_shift;
                        let row = y0 + y as i32 * k + dy;
                        if let Some(cell) = f.grid.get_mut(col, row) {
                            cell.ch = if sheen > 0.4 { '█' } else { '▓' };
                            cell.fg = fg;
                            cell.bg = color * 0.55;
                            cell.glow = glow;
                        }
                    }
                }
            }
        }

        // Bubbles.
        for p in &self.bubbles.particles {
            let pos = p.render_pos(f.alpha);
            let row = sand_row - pos.y as i32;
            let ch = if p.size > 0.7 {
                'o'
            } else if p.size > 0.35 {
                '°'
            } else {
                '.'
            };
            f.grid.overlay(pos.x as i32, row, ch, Rgb::hex(0x9fe8ff) * 0.8, 0.35);
        }

        // Title, typed out once per loop, then glowing.
        let ty = (y0 - 3 * k.max(1)).clamp(1, 2);
        let typed = ((t - 0.3) * 12.0).max(0.0) as usize;
        let tx = (gw as i32 - TITLE.len() as i32) / 2;
        let pulse = 0.5 + 0.5 * (t / LOOP * TAU * 2.0).sin();
        for (i, ch) in TITLE.chars().take(typed).enumerate() {
            let color = Rgb::hex(0xff6b2c).lerp_oklab(Rgb::hex(0xffc35a), i as f32 / TITLE.len() as f32);
            f.grid.overlay(tx + i as i32, ty, ch, color * 1.2, 0.6 + 0.8 * pulse);
        }
        if typed >= TITLE.len() {
            let sx = (gw as i32 - SUBTITLE.len() as i32) / 2;
            let fade = smoothstep(1.2, 2.0, t);
            f.grid.put_str(sx, ty + 1, SUBTITLE, Rgb::hex(0x9fb8c8) * fade, None);
        }
    }
}

fn main() -> Result<(), String> {
    rusteezy::run(Config::new("rusteezy — crab").with_args(), Crab::new())
}
