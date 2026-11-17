//! The Christmas tree scene: build-up, garland, lights, star, snow and sparks.

use std::f32::consts::TAU;

use rusteezy::{
    Frame, Grid, Key, Particle, ParticleSystem, Rgb, Rng, Scene,
    ease::{self, smoothstep},
    glam::{Mat3, Vec3},
    rng::{hash01, hash2},
};

use crate::{
    garland::Garland,
    tree::{self, GROUND_Y, Surf, TIERS},
};

// --- timeline (seconds since build start) ---
const BUILD_START: f32 = 0.4;
const BUILD_DUR: f32 = 4.4;
const FRONT_FROM: f32 = GROUND_Y - 0.05;
const FRONT_TO: f32 = tree::TOP_Y + 0.08;
const GARLAND_START: f32 = 1.5;
const GARLAND_SPEED: f32 = 3.4; // world units per second along the strand
const GARLAND_LAG: f32 = 0.12; // head stays this far below the build front
const GARLAND_RADIUS: f32 = 0.035; // rope thickness (world units, half-width)
const STAR_START: f32 = BUILD_START + BUILD_DUR + 0.15;
const STAR_DROP: f32 = 1.0;
const STAR_Y: f32 = tree::TOP_Y + 0.1;
const ROT_SPEED: f32 = 0.38; // rad/s

const RAMP: [char; 10] = [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];

const SNOW: u8 = 0;
const SPARK: u8 = 1;
const SNOW_COUNT: usize = 1400;

struct Camera {
    pos: Vec3,
    fwd: Vec3,
    right: Vec3,
    up: Vec3,
    tan_half: f32,
}

impl Camera {
    fn new(pos: Vec3, target: Vec3, tan_half: f32) -> Self {
        let fwd = (target - pos).normalize();
        let right = fwd.cross(Vec3::Y).normalize();
        let up = right.cross(fwd);
        Self { pos, fwd, right, up, tan_half }
    }

    /// Ray direction through pixel (px, py) of a `w × h` pixel image.
    fn ray(&self, px: f32, py: f32, w: f32, h: f32) -> Vec3 {
        let sx = (px - w * 0.5) / (h * 0.5);
        let sy = (h * 0.5 - py) / (h * 0.5);
        (self.fwd + self.right * (sx * self.tan_half) + self.up * (sy * self.tan_half)).normalize()
    }

    /// World point → (px, py, distance). Pixel space keeps the cell aspect ratio correct.
    fn project(&self, p: Vec3, w: f32, h: f32) -> Option<(f32, f32, f32)> {
        let v = p - self.pos;
        let z = v.dot(self.fwd);
        if z < 0.05 {
            return None;
        }
        let sx = v.dot(self.right) / (z * self.tan_half);
        let sy = v.dot(self.up) / (z * self.tan_half);
        Some((w * 0.5 + sx * h * 0.5, h * 0.5 - sy * h * 0.5, v.length()))
    }
}

struct Ornament {
    p: Vec3,
    color: Rgb,
    phase: f32,
    speed: f32,
    ch: char,
}

/// Viewport info for one frame.
struct View {
    w: f32,
    h: f32,
    cw: f32,
    ch: f32,
}

impl View {
    fn cell(&self, px: f32, py: f32) -> (i32, i32) {
        ((px / self.cw).floor() as i32, (py / self.ch).floor() as i32)
    }
}

pub struct TreeScene {
    cam: Camera,
    garland: Garland,
    ornaments: Vec<Ornament>,
    particles: ParticleSystem,
    rng: Rng,
    /// Sim time when the current build started.
    t0: f64,
    sim_time: f64,
    spark_acc: f32,
    star_burst_done: bool,
    /// `None` = garland grows until the whole spiral is drawn;
    /// `Some(len)` = a fixed-length strand travels up the tree.
    visible_len: Option<f32>,
    bloom_on: bool,
}

impl TreeScene {
    pub fn new() -> Self {
        let mut rng = Rng::new(0x7ee5);
        let mut particles = ParticleSystem::new(12_000);
        for _ in 0..SNOW_COUNT {
            let y = rng.range(GROUND_Y, 3.4);
            particles.spawn(snowflake(&mut rng, y));
        }
        Self {
            cam: Camera::new(Vec3::new(0.0, 1.2, 6.0), Vec3::new(0.0, 0.92, 0.0), 0.255),
            garland: Garland::new(),
            ornaments: make_ornaments(&mut rng),
            particles,
            rng,
            t0: 0.0,
            sim_time: 0.0,
            spark_acc: 0.0,
            star_burst_done: false,
            visible_len: None,
            bloom_on: true,
        }
    }

    /// Start in "strand" mode: a fixed-length garland travels up the tree.
    pub fn with_strand(mut self, len: Option<f32>) -> Self {
        self.visible_len = len;
        self
    }

    fn front_y(ts: f32) -> f32 {
        let p = ((ts - BUILD_START) / BUILD_DUR).clamp(0.0, 1.0);
        FRONT_FROM + (FRONT_TO - FRONT_FROM) * ease::in_out_cubic(p)
    }

    /// Arc length of the garland head at scene time `ts`, if it has started.
    fn garland_head(&self, ts: f32) -> Option<f32> {
        if ts < GARLAND_START {
            return None;
        }
        let gate = self.garland.s_at_y(Self::front_y(ts) - GARLAND_LAG);
        Some((GARLAND_SPEED * (ts - GARLAND_START)).min(gate).min(self.garland.total()))
    }

    fn rot(time: f64) -> Mat3 {
        Mat3::from_rotation_y((time as f32 * ROT_SPEED) % TAU)
    }

    fn rebuild(&mut self) {
        self.t0 = self.sim_time;
        self.star_burst_done = false;
        self.particles.particles.retain(|p| p.kind == SNOW);
    }

    // ---------------------------------------------------------------- update

    fn emit_garland_sparks(&mut self, dt: f32, ts: f32, time: f64) {
        let Some(s) = self.garland_head(ts) else { return };
        if s >= self.garland.total() - 1e-3 {
            return;
        }
        let head = Self::rot(time) * self.garland.at(s);
        self.spark_acc += dt * 80.0;
        while self.spark_acc >= 1.0 {
            self.spark_acc -= 1.0;
            let out = Vec3::new(head.x, 0.0, head.z).normalize_or(Vec3::Z);
            let vel = self.rng.unit_vec3() * self.rng.range(0.25, 0.9) + out * 0.45 + Vec3::Y * 0.5;
            let life = self.rng.range(0.45, 1.0);
            self.particles.spawn(spark(&mut self.rng, head, vel, life));
        }
    }

    fn star_burst(&mut self) {
        let c = Vec3::new(0.0, STAR_Y, 0.0);
        for _ in 0..320 {
            let dir = self.rng.unit_vec3();
            let vel = dir * self.rng.range(0.8, 2.6) + Vec3::Y * 0.6;
            let life = self.rng.range(0.8, 1.9);
            self.particles.spawn(spark(&mut self.rng, c, vel, life));
        }
    }

    // ------------------------------------------------------------------ draw

    fn draw_world(&self, grid: &mut Grid, v: &View, time: f64, ts: f32) {
        let front = Self::front_y(ts);
        let rot_inv = Self::rot(time).transpose();
        let light_world = Vec3::new(-0.55, 0.75, 0.65).normalize();
        let light = rot_inv * light_world;
        let ro = rot_inv * self.cam.pos;
        let sky_top = Rgb::hex(0x03040c);
        let sky_bot = Rgb::hex(0x0d1236);
        let twinkle_t = (time * 1.3) as f32;

        for row in 0..grid.height() as i32 {
            let py = (row as f32 + 0.5) * v.ch;
            let sky = sky_top.lerp_oklab(sky_bot, (py / v.h).powf(1.4));
            for col in 0..grid.width() as i32 {
                let px = (col as f32 + 0.5) * v.cw;
                let rd_world = self.cam.ray(px, py, v.w, v.h);
                let rd = rot_inv * rd_world;
                let hit = tree::trace(ro, rd, front);

                // Ground (world space, so its texture doesn't spin with the tree).
                let ground_t =
                    if rd_world.y < -1e-4 { (GROUND_Y - self.cam.pos.y) / rd_world.y } else { f32::INFINITY };
                let ground_first = hit.is_none_or(|h| ground_t < h.t);
                let cell = grid.get_mut(col, row).unwrap();
                cell.bg = sky;

                if let Some(h) = hit.filter(|_| !ground_first) {
                    let (ch, fg, bg, glow) = shade_tree(&h, rd, light, front, time);
                    cell.ch = ch;
                    cell.fg = fg;
                    cell.bg = bg;
                    cell.glow = glow;
                    grid.set_depth(col, row, h.t);
                    continue;
                }
                if ground_t.is_finite() {
                    let gp = self.cam.pos + rd_world * ground_t;
                    let r = (gp.x * gp.x + gp.z * gp.z).sqrt();
                    if r < 3.4 {
                        let edge = 1.0 - smoothstep(2.3, 3.4, r);
                        let shadow = 0.45 + 0.55 * smoothstep(0.35, 1.2, r);
                        let n = hash2(col, row); // camera is fixed, so per-cell noise is stable
                        let lum = (0.2 + 0.3 * n) * shadow * edge;
                        cell.bg = sky.lerp(Rgb::hex(0x161d3c) * shadow, edge * 0.75);
                        if n > 0.985 {
                            // Glints in the snow.
                            let tw = 0.5 + 0.5 * (twinkle_t * 3.0 + n * 900.0).sin();
                            cell.ch = '*';
                            cell.fg = Rgb::hex(0xe8f0ff) * (0.5 + 0.7 * tw) * edge;
                            cell.glow = 0.6 * tw * edge;
                        } else if n > 0.55 {
                            cell.ch = if n > 0.85 {
                                ':'
                            } else if n > 0.7 {
                                ','
                            } else {
                                '.'
                            };
                            cell.fg = Rgb::hex(0xbcd0ff) * (0.15 + lum);
                        }
                        grid.set_depth(col, row, ground_t);
                        continue;
                    }
                }
                // Sky with faint twinkling stars.
                let hs = hash2(col, row);
                if hs < 0.012 && py < v.h * 0.75 {
                    let tw = 0.5 + 0.5 * (twinkle_t * (1.0 + hs * 300.0) + hs * 1000.0).sin();
                    cell.ch = if hs < 0.003 { '+' } else { '.' };
                    cell.fg = Rgb::new(0.55, 0.62, 0.85) * (0.25 + 0.6 * tw);
                }
            }
        }
    }

    fn draw_ornaments(&self, grid: &mut Grid, v: &View, time: f64, ts: f32) {
        let front = Self::front_y(ts);
        let rot = Self::rot(time);
        let t = time as f32;
        for o in &self.ornaments {
            if front < o.p.y + 0.02 {
                continue;
            }
            let pop = smoothstep(0.0, 0.3, front - o.p.y);
            let tw = 0.5 + 0.5 * (t * o.speed + o.phase).sin();
            let tw = tw * tw * tw;
            let Some((px, py, d)) = self.cam.project(rot * o.p, v.w, v.h) else { continue };
            let (cx, cy) = v.cell(px, py);
            let fg = o.color * (0.75 + 0.6 * tw);
            let glow = 0.5 + 1.6 * tw + (1.0 - pop) * 3.0;
            grid.plot(cx, cy, d - 0.07, o.ch, fg, glow);
        }
    }

    fn draw_garland(&self, grid: &mut Grid, v: &View, time: f64, ts: f32) {
        let Some(head) = self.garland_head(ts) else { return };
        let rot = Self::rot(time);
        let ds = 0.008;
        let tail = self.visible_len.map_or(0.0, |l| (head - l).max(0.0));
        let gold = Rgb::hex(0xffc94d) * 1.25;
        let hot = Rgb::new(1.0, 0.97, 0.88) * 1.6;
        let red = Rgb::hex(0xff2a3c) * 1.3;
        let blink = (time * 9.0) as u32;
        // Screen pixels per world unit at distance d.
        let px_per_unit = |d: f32| v.h * 0.5 / (d * self.cam.tan_half);
        let min_cell = v.cw.min(v.ch);

        let mut idx = (tail / ds).ceil() as u32;
        let mut prev: Option<(f32, f32)> = None;
        let mut last_center = (i32::MIN, i32::MIN);
        loop {
            let s = idx as f32 * ds;
            if s > head {
                break;
            }
            idx += 1;
            let Some((px, py, d)) = self.cam.project(rot * self.garland.at(s), v.w, v.h) else { continue };
            let Some((qx, qy)) = prev.replace((px, py)) else { continue };
            let (dx, dy) = (px - qx, py - qy);
            let len = (dx * dx + dy * dy).sqrt().max(1e-4);
            let (tx, ty) = (dx / len, dy / len);

            let a = (-dy).atan2(dx).to_degrees().rem_euclid(180.0);
            let stroke = if !(22.5..157.5).contains(&a) {
                '~'
            } else if a < 67.5 {
                '/'
            } else if a < 112.5 {
                '|'
            } else {
                '\\'
            };
            let behind = head - s;
            // Samples are denser than cells; only sweep the width once per center cell
            // (the head is always drawn so it stays crisp).
            let center = v.cell(px, py);
            if center == last_center && behind >= 0.05 {
                continue;
            }
            last_center = center;
            let heat = (-behind * 2.4).exp();
            let base = gold.lerp_oklab(hot, heat);
            let sparkle = hash01(idx ^ blink.wrapping_mul(2_654_435_761)) > 0.93;

            // Sweep across the strand (perpendicular on screen) to give it width.
            let radius_px = GARLAND_RADIUS * px_per_unit(d);
            let steps = (radius_px / (min_cell * 0.5)).ceil().max(1.0) as i32;
            for k in -steps..=steps {
                let u = k as f32 / steps as f32; // -1..1 across the rope
                let off = u * radius_px;
                let (cx, cy) = v.cell(px - ty * off, py + tx * off);
                let shade = 1.0 - 0.5 * u * u; // round rope: bright core, darker edges
                let (mut ch, mut fg, mut glow) = (stroke, base * shade, (0.35 + 2.4 * heat) * shade);
                if k == 0 {
                    if idx.is_multiple_of(41) {
                        (ch, fg, glow) = ('•', red, 0.9 + heat);
                    }
                    if behind < 0.05 {
                        (ch, fg, glow) = ('*', hot, 3.2);
                    }
                } else if u.abs() > 0.6 && hash01(idx.wrapping_mul(31) ^ (k as u32) ^ blink) > 0.9 {
                    // Tinsel glints along the edges.
                    (ch, glow) = ('*', glow + 1.2);
                }
                if sparkle {
                    glow += 1.6 * shade;
                }
                grid.plot(cx, cy, d - 0.05, ch, fg, glow);
            }
        }
    }

    fn draw_star(&self, grid: &mut Grid, v: &View, time: f64, ts: f32, atlas_has_star: bool) {
        if ts < STAR_START {
            return;
        }
        let p = ((ts - STAR_START) / STAR_DROP).clamp(0.0, 1.0);
        let y = 3.4 + (STAR_Y - 3.4) * ease::out_bounce(p);
        let Some((px, py, _)) = self.cam.project(Vec3::new(0.0, y, 0.0), v.w, v.h) else { return };
        let (cx, cy) = v.cell(px, py);
        let t = time as f32;
        let landed = ts - (STAR_START + STAR_DROP);
        let flash = if landed > 0.0 { 4.0 * (-landed * 2.5).exp() } else { 0.0 };
        let gold = Rgb::hex(0xfff0a8) * 1.4;
        let pulse = 0.5 + 0.5 * (t * 2.6).sin();
        grid.overlay(cx, cy, if atlas_has_star { '★' } else { '*' }, gold, 2.4 + 0.8 * pulse + flash);

        // Rays alternate between a plus and an x.
        let ray = gold * 0.8;
        let g = 0.6 + 0.9 * pulse + flash * 0.5;
        if (t * 1.3).sin() > 0.0 {
            for (dx, dy, ch) in [(-1i32, 0, '-'), (1, 0, '-'), (-2, 0, '-'), (2, 0, '-'), (0, -1, '|'), (0, 1, '|')] {
                let fall = if dx.abs() == 2 { 0.5 } else { 1.0 };
                grid.overlay(cx + dx, cy + dy, ch, ray, g * fall);
            }
        } else {
            for (dx, dy, ch) in [(-1, -1, '\\'), (1, 1, '\\'), (1, -1, '/'), (-1, 1, '/')] {
                grid.overlay(cx + dx, cy + dy, ch, ray, g * 0.8);
            }
        }
    }

    fn draw_particles(&self, grid: &mut Grid, v: &View, alpha: f32) {
        let hot = Rgb::new(1.0, 0.95, 0.75);
        let gold = Rgb::hex(0xffa526);
        let ember = Rgb::hex(0xc81e0a);
        let flake = Rgb::new(0.82, 0.88, 1.0);
        for p in &self.particles.particles {
            let Some((px, py, d)) = self.cam.project(p.render_pos(alpha), v.w, v.h) else { continue };
            let (cx, cy) = v.cell(px, py);
            match p.kind {
                SNOW => {
                    let ch = if d < 4.3 {
                        '*'
                    } else if d < 5.6 {
                        '+'
                    } else if d < 6.9 {
                        '·'
                    } else {
                        '.'
                    };
                    let b = (1.35 - (d - 3.0) * 0.17).clamp(0.3, 1.2) * (0.8 + 0.4 * p.seed);
                    grid.plot(cx, cy, d, ch, flake * b, if d < 4.3 { 0.25 } else { 0.0 });
                }
                _ => {
                    let t = p.t();
                    let c =
                        if t < 0.3 { hot.lerp_oklab(gold, t / 0.3) } else { gold.lerp_oklab(ember, (t - 0.3) / 0.7) };
                    let fade = (1.0 - t).powf(0.7);
                    let ch = if t < 0.25 {
                        '*'
                    } else if t < 0.5 {
                        '+'
                    } else if t < 0.75 {
                        '·'
                    } else {
                        '.'
                    };
                    grid.plot(cx, cy, d - 0.02, ch, c * (0.4 + fade), 2.6 * (1.0 - t) * (1.0 - t) + 0.15);
                }
            }
        }
    }
}

impl Scene for TreeScene {
    fn update(&mut self, dt: f32, time: f64) {
        self.sim_time = time;
        let ts = (time - self.t0) as f32;
        let t = time as f32;

        self.emit_garland_sparks(dt, ts, time);
        if !self.star_burst_done && ts >= STAR_START + STAR_DROP {
            self.star_burst_done = true;
            self.star_burst();
        }

        self.particles.update(dt, |p| match p.kind {
            SNOW => {
                let wind = Vec3::new(
                    0.28 * (0.4 * t + p.pos.y * 1.3 + p.seed * 6.0).sin() + 0.08 * (1.7 * t + p.pos.z).sin(),
                    0.0,
                    0.12 * (0.6 * t + p.pos.x).cos(),
                );
                Vec3::new(0.0, -0.5, 0.0) + wind
            }
            _ => Vec3::new(0.0, -2.3, 0.0),
        });

        // Recycle snow that reached the ground or drifted out.
        let rng = &mut self.rng;
        for p in &mut self.particles.particles {
            if p.kind == SNOW && (p.pos.y < GROUND_Y || p.pos.x.abs() > 4.0) {
                *p = snowflake(rng, 3.4);
            }
        }
    }

    fn draw(&mut self, f: &mut Frame) {
        f.settings.bloom_intensity = if self.bloom_on { 0.95 } else { 0.0 };
        f.settings.clear = Rgb::hex(0x03040c);
        let v = View {
            w: f.grid.width() as f32 * f.cell_w,
            h: f.grid.height() as f32 * f.cell_h,
            cw: f.cell_w,
            ch: f.cell_h,
        };
        let ts = (f.time - self.t0) as f32;
        let has_star = f.atlas.has('★');
        self.draw_world(f.grid, &v, f.time, ts);
        self.draw_ornaments(f.grid, &v, f.time, ts);
        self.draw_garland(f.grid, &v, f.time, ts);
        self.draw_particles(f.grid, &v, f.alpha);
        self.draw_star(f.grid, &v, f.time, ts, has_star);
    }

    fn key(&mut self, key: &Key) {
        if let Key::Character(c) = key {
            match c.to_lowercase().as_str() {
                "r" => self.rebuild(),
                "g" => {
                    self.visible_len = if self.visible_len.is_some() { None } else { Some(3.0) };
                    self.rebuild();
                }
                "b" => self.bloom_on = !self.bloom_on,
                _ => {}
            }
        }
    }

    fn hud(&self) -> String {
        format!(
            "{} particles | garland: {} | R rebuild · G garland mode · B bloom · F1 hud",
            self.particles.len(),
            if self.visible_len.is_some() { "strand" } else { "grow" },
        )
    }
}

fn shade_tree(h: &tree::Hit, rd: Vec3, light: Vec3, front: f32, time: f64) -> (char, Rgb, Rgb, f32) {
    let p = h.p;
    match h.surf {
        Surf::Foliage { tier } => {
            // Looking at the inside of the shell (only while it's being built).
            let inside = h.n.dot(rd) > 0.0;
            let n = if inside { -h.n } else { h.n };
            let diffuse = n.dot(light).max(0.0);
            let phi = p.z.atan2(p.x);
            let v = (p.y * 7.0 + 0.18 * (phi * 5.0 + tier as f32).sin()).rem_euclid(1.0);
            let bough = smoothstep(0.0, 0.2, v) * (1.0 - smoothstep(0.55, 1.0, v));
            let needles = 0.5 + 0.5 * (phi * 23.0 + p.y * 41.0).sin() * (phi * 7.0 - p.y * 13.0).sin();
            let tex = 0.55 + 0.3 * bough + 0.15 * needles;
            let mut lum = (0.16 + 0.84 * diffuse) * tex;
            if inside {
                lum *= 0.45;
            }

            let deep = Rgb::hex(0x0a3a1a);
            let mid = Rgb::hex(0x1f7a3a);
            let hi = Rgb::hex(0x9be07a);
            let mut c = if lum < 0.5 { deep.lerp_oklab(mid, lum * 2.0) } else { mid.lerp_oklab(hi, (lum - 0.5) * 2.0) };
            c = c * 1.7;

            // Snow dusting along each tier's lower rim.
            let t = TIERS[tier];
            let rim = 1.0 - smoothstep(0.0, 0.09, (p.y - t.yb) / (t.ya - t.yb));
            if !inside && rim > 0.0 {
                let fleck = hash01((phi * 40.0).floor() as i32 as u32 ^ (tier as u32 * 977));
                if fleck > 0.45 {
                    c = c.lerp_oklab(Rgb::hex(0xe8f0ff), rim * 0.85);
                    lum = lum.max(0.55 * rim);
                }
            }

            let mut glow = 0.0;
            let fresh = 1.0 - smoothstep(0.0, 0.45, front - p.y);
            if fresh > 0.0 {
                c = c.lerp_oklab(Rgb::hex(0xc6ffd8) * 1.5, fresh * fresh);
                glow = fresh * fresh * 1.4;
            }
            let ch = RAMP[((lum * 10.0) as usize).clamp(1, 9)];
            (ch, c, c * 0.1, glow)
        }
        Surf::Cap => {
            let n = hash2((p.x * 30.0) as i32, (p.z * 30.0) as i32);
            let flick = 0.6 + 0.4 * ((time as f32) * 20.0 + n * 50.0).sin();
            (if n > 0.5 { '=' } else { '-' }, Rgb::hex(0xb8ffcc) * 1.4, Rgb::hex(0x0d3a20), 1.2 + flick)
        }
        Surf::Underside => (':', Rgb::hex(0x0c2a16), Rgb::hex(0x03100a), 0.0),
        Surf::Trunk => {
            let lum = 0.25 + 0.75 * h.n.dot(light).max(0.0);
            let c = Rgb::hex(0x6b4423) * (0.6 + lum);
            (if lum > 0.55 { '#' } else { '|' }, c, c * 0.15, 0.0)
        }
    }
}

fn snowflake(rng: &mut Rng, y: f32) -> Particle {
    let pos = Vec3::new(rng.range(-3.8, 3.8), y, rng.range(-2.5, 3.2));
    let mut p = Particle::new(pos, Vec3::new(0.0, -0.3, 0.0), f32::INFINITY);
    p.drag = 1.6;
    p.kind = SNOW;
    p.seed = rng.f32();
    p
}

fn spark(rng: &mut Rng, pos: Vec3, vel: Vec3, life: f32) -> Particle {
    let mut p = Particle::new(pos + rng.unit_vec3() * 0.02, vel, life);
    p.drag = 1.3;
    p.kind = SPARK;
    p.seed = rng.f32();
    p
}

fn make_ornaments(rng: &mut Rng) -> Vec<Ornament> {
    let palette = [
        Rgb::hex(0xff2a3c),
        Rgb::hex(0xffc22e),
        Rgb::hex(0x3d7bff),
        Rgb::hex(0xff5fd2),
        Rgb::hex(0xfff3d6),
        Rgb::hex(0x34e0ff),
    ];
    (0..80)
        .map(|i| {
            // More lights lower down, where the tree is wider.
            let y = 0.12 + 1.85 * (1.0 - rng.f32().sqrt());
            let phi = rng.range(0.0, TAU);
            let r = tree::envelope(y) + 0.015;
            Ornament {
                p: Vec3::new(r * phi.cos(), y, r * phi.sin()),
                color: palette[i % palette.len()] * 1.3,
                phase: rng.range(0.0, TAU),
                speed: rng.range(1.5, 4.5),
                ch: if i % 3 == 0 { '●' } else { 'o' },
            }
        })
        .collect()
}
