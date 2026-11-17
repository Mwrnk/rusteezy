//! A starfield that periodically jumps to warp speed: stars stretch into
//! streaks drawn with slope-matched glyphs, and the near ones bloom.
//!
//! ```text
//! cargo run --release --example starfield
//! ```

use rusteezy::{Config, Frame, Rgb, Rng, Scene, ease, rng::hash01};

const COUNT: usize = 1400;
const FAR: f32 = 6.0;

struct Star {
    x: f32,
    y: f32,
    z: f32,
    tint: f32,
}

struct Starfield {
    stars: Vec<Star>,
    rng: Rng,
    speed: f32,
}

/// Cruise, then a 3-second warp jump every 7 seconds.
fn speed_at(t: f32) -> f32 {
    let phase = t.rem_euclid(7.0);
    let jump = ease::smoothstep(3.5, 4.5, phase) * (1.0 - ease::smoothstep(6.0, 7.0, phase));
    0.9 + 13.0 * ease::in_out_cubic(jump)
}

impl Starfield {
    fn new() -> Self {
        let mut rng = Rng::new(42);
        let stars = (0..COUNT)
            .map(|_| Star { x: rng.range(-4.0, 4.0), y: rng.range(-2.5, 2.5), z: rng.range(0.1, FAR), tint: rng.f32() })
            .collect();
        Self { stars, rng, speed: 1.0 }
    }
}

fn slope_char(dx: f32, dy: f32) -> char {
    let a = (-dy).atan2(dx).to_degrees().rem_euclid(180.0);
    match a {
        a if !(22.5..157.5).contains(&a) => '-',
        a if a < 67.5 => '/',
        a if a < 112.5 => '|',
        _ => '\\',
    }
}

impl Scene for Starfield {
    fn update(&mut self, dt: f32, time: f64) {
        self.speed = speed_at(time as f32);
        for s in &mut self.stars {
            s.z -= self.speed * dt;
            if s.z < 0.05 {
                s.x = self.rng.range(-4.0, 4.0);
                s.y = self.rng.range(-2.5, 2.5);
                s.z = FAR;
                s.tint = self.rng.f32();
            }
        }
    }

    fn draw(&mut self, f: &mut Frame) {
        let (gw, gh) = f.size();
        let (w, h) = (gw as f32 * f.cell_w, gh as f32 * f.cell_h);
        let half = h * 0.5;
        let warp = ((self.speed - 0.9) / 13.0).clamp(0.0, 1.0);
        let project = |x: f32, y: f32, z: f32| (w * 0.5 + x / z * half, h * 0.5 - y / z * half);
        let t = f.time as f32;

        // Deep-space background that warms up during the jump.
        let bg = Rgb::hex(0x020309).lerp_oklab(Rgb::hex(0x0c0a2a), warp);
        for c in f.grid.cells_mut() {
            c.bg = bg;
        }

        for (i, s) in self.stars.iter().enumerate() {
            let (hx, hy) = project(s.x, s.y, s.z);
            // The streak's tail is where the star was a moment ago.
            let tail_z = (s.z + self.speed * 0.045).min(FAR);
            let (tx, ty) = project(s.x, s.y, tail_z);
            let near = 1.0 - s.z / FAR;
            let base = match s.tint {
                v if v < 0.15 => Rgb::hex(0xffb38a),
                v if v < 0.35 => Rgb::hex(0x9ec5ff),
                _ => Rgb::hex(0xe8eeff),
            };
            let twinkle = 0.75 + 0.25 * hash01(i as u32 ^ (t * 8.0) as u32);
            let color = base * (0.15 + 1.2 * near * near) * twinkle;
            let glow = near.powi(3) * (0.6 + 1.6 * warp);

            let (dx, dy) = (hx - tx, hy - ty);
            let steps = (dx.abs() / f.cell_w).max(dy.abs() / f.cell_h).ceil() as i32;
            if steps <= 1 {
                let ch = if near > 0.75 {
                    '*'
                } else if near > 0.4 {
                    '+'
                } else {
                    '.'
                };
                f.grid.overlay((hx / f.cell_w) as i32, (hy / f.cell_h) as i32, ch, color, glow);
                continue;
            }
            let ch = slope_char(dx, dy);
            for k in 0..=steps {
                let u = k as f32 / steps as f32; // 0 at tail, 1 at head
                let (px, py) = (tx + dx * u, ty + dy * u);
                f.grid.overlay((px / f.cell_w) as i32, (py / f.cell_h) as i32, ch, color * (0.3 + 0.7 * u), glow * u);
            }
        }
    }

    fn hud(&self) -> String {
        format!("speed {:.1}", self.speed)
    }
}

fn main() -> Result<(), String> {
    rusteezy::run(Config::new("rusteezy — starfield").with_args(), Starfield::new())
}
