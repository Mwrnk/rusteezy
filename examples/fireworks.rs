//! Fireworks over a city skyline: rockets, bursts, trails and crackle.
//! Shows the particle system (gravity, drag, per-particle color) and bloom.
//!
//! ```text
//! cargo run --release --example fireworks
//! ```
//!
//! Keys: Space launch a volley · Esc quit

use std::f32::consts::TAU;

use rusteezy::{
    Config, Frame, Key, NamedKey, Particle, ParticleSystem, Rgb, Rng, Scene,
    glam::Vec3,
    rng::{hash01, hash2},
};

const ROCKET: u8 = 0;
const SPARK: u8 = 1;
const TRAIL: u8 = 2;
/// World units: the screen is 2 units tall, y = 0 at the skyline.
const GRAVITY: f32 = -0.55;

struct Fireworks {
    ps: ParticleSystem,
    rng: Rng,
    next_launch: f64,
    pending_volley: u32,
    /// Half the screen width in world units (updated each frame).
    half_w: f32,
    /// Light from recent bursts, used to tint the sky and buildings.
    flash: Rgb,
}

impl Fireworks {
    fn new() -> Self {
        Self {
            ps: ParticleSystem::new(30_000),
            rng: Rng::new(7),
            next_launch: 0.3,
            pending_volley: 0,
            half_w: 1.6,
            flash: Rgb::BLACK,
        }
    }

    fn launch(&mut self) {
        let palette = [0xff3b5c, 0xffc23d, 0x3dd9ff, 0x9d6bff, 0x5cff8a, 0xff7ad9, 0xffffff];
        let x = self.rng.range(-0.75, 0.75) * self.half_w;
        let vel = Vec3::new(self.rng.range(-0.1, 0.1), self.rng.range(1.15, 1.5), 0.0);
        let mut p = Particle::new(Vec3::new(x, 0.0, 0.0), vel, 4.0);
        p.kind = ROCKET;
        p.color = Rgb::hex(palette[(self.rng.f32() * palette.len() as f32) as usize % palette.len()]);
        p.seed = self.rng.f32();
        self.ps.spawn(p);
    }

    fn explode(&mut self, pos: Vec3, color: Rgb) {
        let style = self.rng.f32();
        let accent = Rgb::hex(0xfff2c0);
        let n = self.rng.range(110.0, 170.0) as usize;
        for _ in 0..n {
            // A shell throws sparks out on a sphere; seen from the side that's a
            // bloom with a bright rim. Rings stay flat circles.
            let sphere = self.rng.unit_vec3();
            let a = self.rng.range(0.0, TAU);
            let (dir, speed, life, drag) = if style < 0.25 {
                (Vec3::new(a.cos(), a.sin(), 0.0), 1.05 + self.rng.range(-0.03, 0.03), self.rng.range(1.0, 1.4), 1.5) // ring
            } else if style < 0.45 {
                (Vec3::new(sphere.x, sphere.y, 0.0), 0.95 + self.rng.range(-0.08, 0.08), self.rng.range(2.4, 3.4), 1.7) // willow
            } else {
                (Vec3::new(sphere.x, sphere.y, 0.0), 1.2 + self.rng.range(-0.08, 0.08), self.rng.range(1.2, 1.9), 1.15) // peony
            };
            let mut p = Particle::new(pos, dir * speed, life);
            p.kind = SPARK;
            p.drag = drag;
            p.seed = self.rng.f32();
            p.color = if (0.25..0.45).contains(&style) {
                Rgb::hex(0xffb347) // willows are gold
            } else {
                color.lerp_oklab(accent, self.rng.f32() * 0.35)
            };
            self.ps.spawn(p);
        }
        self.flash = self.flash + color * 0.35;
    }
}

impl Scene for Fireworks {
    fn update(&mut self, dt: f32, time: f64) {
        if time >= self.next_launch {
            self.launch();
            self.next_launch = time + self.rng.range(0.25, 0.75) as f64;
        }
        if self.pending_volley > 0 && self.rng.f32() < dt * 20.0 {
            self.pending_volley -= 1;
            self.launch();
        }

        self.ps.update(dt, |p| match p.kind {
            ROCKET => Vec3::new(0.0, GRAVITY * 1.6, 0.0),
            SPARK => Vec3::new(0.0, GRAVITY * 0.55, 0.0),
            _ => Vec3::new(0.0, GRAVITY * 0.1, 0.0),
        });

        // Rockets leave trails and burst near the top of their arc.
        let mut bursts = Vec::new();
        let mut trails = Vec::new();
        for p in &mut self.ps.particles {
            if p.kind != ROCKET {
                continue;
            }
            if p.vel.y < 0.12 {
                bursts.push((p.pos, p.color));
                p.age = p.life; // removed on the next update
            } else {
                trails.push(p.pos);
            }
        }
        for (pos, color) in bursts {
            self.explode(pos, color);
        }
        for pos in trails {
            let vel = Vec3::new(self.rng.range(-0.05, 0.05), self.rng.range(-0.2, -0.05), 0.0);
            let mut t = Particle::new(pos, vel, self.rng.range(0.2, 0.45));
            t.kind = TRAIL;
            t.color = Rgb::hex(0xffa040);
            self.ps.spawn(t);
        }

        let decay = (-dt * 3.5).exp();
        self.flash = self.flash * decay;
    }

    fn draw(&mut self, f: &mut Frame) {
        let (gw, gh) = f.size();
        let (w, h) = (gw as f32 * f.cell_w, gh as f32 * f.cell_h);
        let unit = h * 0.5; // pixels per world unit
        let ground = h * 0.86; // skyline y in pixels
        self.half_w = w * 0.5 / unit;
        let to_cell = |p: Vec3| (((w * 0.5 + p.x * unit) / f.cell_w) as i32, ((ground - p.y * unit) / f.cell_h) as i32);
        let t = f.time as f32;

        // Sky and skyline.
        let sky_top = Rgb::hex(0x02030a);
        let sky_low = Rgb::hex(0x1a1238);
        let flash = self.flash;
        let ground_row = (ground / f.cell_h) as i32;
        let mut building_top = 0;
        for col in 0..gw as i32 {
            if col % 5 == 0 {
                let block = col / 5;
                let tall = hash2(block, 7);
                building_top = ground_row - (2.0 + tall * tall * 9.0) as i32;
            }
            for row in 0..gh as i32 {
                let v = row as f32 / gh as f32;
                let cell = f.grid.get_mut(col, row).unwrap();
                cell.bg = sky_top.lerp_oklab(sky_low, v * v) + flash * (0.12 * v);
                if row >= building_top {
                    let lit = hash2(col, row) < 0.16 && row < ground_row + 3;
                    cell.bg = Rgb::hex(0x07070f) + flash * 0.05;
                    if lit {
                        let warm = 0.5 + 0.5 * hash2(row, col);
                        cell.ch = '.';
                        cell.fg = Rgb::hex(0xffcf73) * (0.45 + 0.4 * warm);
                    }
                }
            }
        }

        for p in &self.ps.particles {
            let (cx, cy) = to_cell(p.render_pos(f.alpha));
            if cy >= ground_row {
                continue;
            }
            match p.kind {
                ROCKET => f.grid.overlay(cx, cy, '|', Rgb::hex(0xffe2b0), 1.4),
                TRAIL => f.grid.overlay(cx, cy, '.', p.color * (1.0 - p.t()), 0.4 * (1.0 - p.t())),
                _ => {
                    let age = p.t();
                    // Crackle: late sparks flicker on and off.
                    if age > 0.6 && hash01((p.seed * 1e6) as u32 ^ (t * 24.0) as u32) > 0.55 {
                        continue;
                    }
                    let fade = (1.0 - age).powf(1.5);
                    let ch = match age {
                        a if a < 0.25 => '*',
                        a if a < 0.5 => '+',
                        a if a < 0.8 => '·',
                        _ => '.',
                    };
                    f.grid.overlay(cx, cy, ch, p.color * (0.35 + 1.1 * fade), 2.2 * fade * fade + 0.1);
                }
            }
        }
    }

    fn key(&mut self, key: &Key) {
        if let Key::Named(NamedKey::Space) = key {
            self.pending_volley += 6;
        }
    }

    fn hud(&self) -> String {
        format!("{} particles | Space volley", self.ps.len())
    }
}

fn main() -> Result<(), String> {
    rusteezy::run(Config::new("rusteezy — fireworks").with_args(), Fireworks::new())
}
