//! A suspension bridge at golden hour. Every part of the bridge is a path that
//! draws itself along its length with a glowing head: the towers rise from the
//! water, the cables run anchor to anchor over both towers, and the suspenders
//! drop once the cable has passed. The water mirrors the sky, sun and bridge.
//!
//! ```text
//! cargo run --release --example bridge
//! ```
//!
//! Keys: R rebuild · Esc quit

use std::f32::consts::PI;

use rusteezy::{
    Config, Frame, Grid, Key, Particle, ParticleSystem, Rgb, Rng, Scene,
    ease::smoothstep,
    glam::Vec3,
    rng::{hash01, hash2},
};

// --- bridge geometry (bridge space: u along the span, v across it, y up) ---
const HALF_SPAN: f32 = 1.75; // deck ends
const TOWER_U: f32 = 0.75; // towers at ±TOWER_U
const HALF_WIDTH: f32 = 0.085; // the two cable planes / tower legs at v = ±HALF_WIDTH
const DECK_Y: f32 = 0.30;
const TOWER_TOP: f32 = 1.05;
const CABLE_TOP: f32 = 1.03;
const CABLE_LOW: f32 = 0.34;
const STRUTS: [f32; 5] = [0.22, 0.55, 0.72, 0.88, 1.03];
const YAW: f32 = 0.45; // bridge rotated so the left end is nearer
const CENTER: Vec3 = Vec3::new(0.15, 0.0, 3.0);

// --- camera (off-axis: the horizon stays at a fixed screen height) ---
const CAM_Y: f32 = 0.06;
const FOCAL: f32 = 1.1;
const HORIZON: f32 = 0.62; // fraction of screen height

// --- timeline (seconds since the build started) ---
const LEG_START: f32 = 0.2;
const LEG_SPEED: f32 = 0.55;
const DECK_START: f32 = 1.4;
const DECK_SPEED: f32 = 0.95;
const CABLE_START: f32 = 2.2;
const CABLE_SPEED: f32 = 1.45;
const SUSPENDER_SPEED: f32 = 0.7;

const ORANGE: u32 = 0xc0362c; // "international orange"

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Leg,
    Strut,
    Cable,
    Deck,
    Truss,
    Suspender,
}

/// A polyline that draws itself from its first point at `speed` world units/s.
struct Stroke {
    pts: Vec<Vec3>,
    cum: Vec<f32>,
    start: f32,
    speed: f32,
    /// Half-thickness in world units (0 = one cell wide).
    radius: f32,
    kind: Kind,
}

impl Stroke {
    fn new(pts: Vec<Vec3>, start: f32, speed: f32, radius: f32, kind: Kind) -> Self {
        let mut cum = Vec::with_capacity(pts.len());
        let mut acc = 0.0;
        for (i, p) in pts.iter().enumerate() {
            if i > 0 {
                acc += p.distance(pts[i - 1]);
            }
            cum.push(acc);
        }
        Self { pts, cum, start, speed, radius, kind }
    }

    fn total(&self) -> f32 {
        *self.cum.last().unwrap()
    }

    /// How much of the stroke is drawn at scene time `ts`.
    fn drawn(&self, ts: f32) -> f32 {
        ((ts - self.start) * self.speed).clamp(0.0, self.total())
    }

    fn at(&self, s: f32) -> Vec3 {
        let i = self.cum.partition_point(|&c| c < s).clamp(1, self.pts.len() - 1);
        let (c0, c1) = (self.cum[i - 1], self.cum[i]);
        let f = if c1 > c0 { ((s - c0) / (c1 - c0)).clamp(0.0, 1.0) } else { 0.0 };
        self.pts[i - 1].lerp(self.pts[i], f)
    }

    /// Scene time at which the head reaches arc length `s`.
    fn time_at(&self, s: f32) -> f32 {
        self.start + s / self.speed
    }
}

/// Bridge space → world space.
fn world(u: f32, v: f32, y: f32) -> Vec3 {
    let (s, c) = YAW.sin_cos();
    Vec3::new(CENTER.x + u * c - v * s, y, CENTER.z + u * s + v * c)
}

/// Height of a main cable at position `u` along the span.
fn cable_y(u: f32) -> f32 {
    let a = u.abs();
    if a <= TOWER_U {
        CABLE_LOW + (CABLE_TOP - CABLE_LOW) * (a / TOWER_U).powi(2)
    } else {
        let s = (a - TOWER_U) / (HALF_SPAN - TOWER_U);
        CABLE_TOP + (DECK_Y + 0.01 - CABLE_TOP) * s - 0.12 * (PI * s).sin()
    }
}

struct View {
    w: f32,
    h: f32,
    cw: f32,
    ch: f32,
}

impl View {
    /// World point → screen pixel and distance from the camera.
    fn project(&self, p: Vec3) -> Option<(f32, f32, f32)> {
        let rel = p - Vec3::new(0.0, CAM_Y, 0.0);
        if rel.z < 0.05 {
            return None;
        }
        let (sx, sy) = (rel.x / rel.z, rel.y / rel.z);
        Some((self.w * 0.5 + sx * FOCAL * self.h, self.h * HORIZON - sy * FOCAL * self.h, rel.length()))
    }

    fn cell(&self, px: f32, py: f32) -> (i32, i32) {
        ((px / self.cw).floor() as i32, (py / self.ch).floor() as i32)
    }

    fn horizon_px(&self) -> f32 {
        self.h * HORIZON
    }

    fn sun(&self) -> (f32, f32, f32) {
        (self.w * 0.53, self.h * (HORIZON - 0.17), self.h * 0.045)
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

struct Bridge {
    strokes: Vec<Stroke>,
    /// Indices of the two main cables (they shed sparks while drawing).
    cables: [usize; 2],
    sparks: ParticleSystem,
    rng: Rng,
    t0: f64,
    sim_time: f64,
    spark_acc: f32,
}

impl Bridge {
    fn new() -> Self {
        let mut strokes = Vec::new();

        // Towers: two legs each, rising from the water; struts appear as legs reach them.
        for (ti, tu) in [-TOWER_U, TOWER_U].into_iter().enumerate() {
            for (si, v) in [-HALF_WIDTH, HALF_WIDTH].into_iter().enumerate() {
                let start = LEG_START + ti as f32 * 0.3 + si as f32 * 0.12;
                strokes.push(Stroke::new(
                    vec![world(tu, v, -0.02), world(tu, v, TOWER_TOP)],
                    start,
                    LEG_SPEED,
                    0.016,
                    Kind::Leg,
                ));
            }
            let legs_done = LEG_START + ti as f32 * 0.3 + 0.12;
            for y in STRUTS {
                let start = legs_done + (y + 0.02) / LEG_SPEED;
                strokes.push(Stroke::new(
                    vec![world(tu, -HALF_WIDTH, y), world(tu, HALF_WIDTH, y)],
                    start,
                    0.6,
                    0.008,
                    Kind::Strut,
                ));
            }
        }

        // Deck: three longitudinal lines plus the truss under the near edge, left to right.
        for (i, (v, y, kind)) in [
            (-HALF_WIDTH, DECK_Y, Kind::Deck),
            (0.0, DECK_Y, Kind::Deck),
            (HALF_WIDTH, DECK_Y, Kind::Deck),
            (HALF_WIDTH, DECK_Y - 0.035, Kind::Truss),
        ]
        .into_iter()
        .enumerate()
        {
            let pts = (0..=80).map(|k| world(-HALF_SPAN + 2.0 * HALF_SPAN * k as f32 / 80.0, v, y)).collect();
            strokes.push(Stroke::new(pts, DECK_START + i as f32 * 0.08, DECK_SPEED, 0.006, kind));
        }

        // Main cables: anchor → tower → sag → tower → anchor, one per side.
        let mut cables = [0; 2];
        for (i, v) in [-HALF_WIDTH, HALF_WIDTH].into_iter().enumerate() {
            let pts = (0..=400)
                .map(|k| {
                    let u = -HALF_SPAN + 2.0 * HALF_SPAN * k as f32 / 400.0;
                    world(u, v, cable_y(u))
                })
                .collect();
            cables[i] = strokes.len();
            strokes.push(Stroke::new(pts, CABLE_START + i as f32 * 0.18, CABLE_SPEED, 0.0, Kind::Cable));
        }

        // Suspenders: each drops from its cable once the cable's head has passed
        // and the deck below it exists.
        let n = (2.0 * HALF_SPAN / 0.07) as i32;
        for k in 1..n {
            let u = -HALF_SPAN + k as f32 * 0.07;
            if (u.abs() - TOWER_U).abs() < 0.05 || cable_y(u) - DECK_Y < 0.03 {
                continue;
            }
            for (i, v) in [-HALF_WIDTH, HALF_WIDTH].into_iter().enumerate() {
                let cable = &strokes[cables[i]];
                let s_cable = cable.cum[((u + HALF_SPAN) / (2.0 * HALF_SPAN) * 400.0).round() as usize];
                let deck_time = DECK_START + (u + HALF_SPAN) / DECK_SPEED;
                let start = cable.time_at(s_cable).max(deck_time) + 0.05;
                strokes.push(Stroke::new(
                    vec![world(u, v, cable_y(u)), world(u, v, DECK_Y)],
                    start,
                    SUSPENDER_SPEED,
                    0.0,
                    Kind::Suspender,
                ));
            }
        }

        Self {
            strokes,
            cables,
            sparks: ParticleSystem::new(4000),
            rng: Rng::new(1937),
            t0: 0.0,
            sim_time: 0.0,
            spark_acc: 0.0,
        }
    }

    fn build_end(&self) -> f32 {
        self.strokes.iter().map(|s| s.time_at(s.total())).fold(0.0, f32::max)
    }

    // ------------------------------------------------------------------ sky

    fn draw_sky(&self, grid: &mut Grid, v: &View, t: f32) {
        let hz = v.horizon_px();
        let (sun_x, sun_y, sun_r) = v.sun();
        let stops = [(0.0, 0x1c2350), (0.35, 0x6b3f7d), (0.7, 0xf2795a), (1.0, 0xffcf7a)];
        let sky_at = |f: f32| {
            for pair in stops.windows(2) {
                let ((a, ca), (b, cb)) = (pair[0], pair[1]);
                if f <= b {
                    return Rgb::hex(ca).lerp_oklab(Rgb::hex(cb), ((f - a) / (b - a)).clamp(0.0, 1.0));
                }
            }
            Rgb::hex(stops[3].1)
        };

        for row in 0..grid.height() as i32 {
            let py = (row as f32 + 0.5) * v.ch;
            if py > hz {
                break;
            }
            let f = py / hz;
            for col in 0..grid.width() as i32 {
                let px = (col as f32 + 0.5) * v.cw;
                let (dx, dy) = (px - sun_x, py - sun_y);
                let d = (dx * dx + dy * dy).sqrt();
                let halo = (-(d / (v.h * 0.18)).powi(2)).exp();
                let mut bg = sky_at(f) + Rgb::hex(0xffb347) * (halo * 0.35);
                let cell = grid.get_mut(col, row).unwrap();

                // Streaky clouds, lit from below, brighter near the sun.
                let n = (px * 0.006 + t * 0.04 + py * 0.02).sin() * (px * 0.013 - py * 0.05 + 1.3).sin()
                    + 0.5 * (px * 0.021 + py * 0.11 + t * 0.03).sin();
                let band = smoothstep(0.12, 0.3, f) * (1.0 - smoothstep(0.8, 0.95, f));
                if n * band > 0.62 {
                    let c = Rgb::hex(0xff8f70).lerp_oklab(Rgb::hex(0xffe0a8), halo);
                    cell.ch = if n > 0.9 { '=' } else { '-' };
                    cell.fg = c * (0.7 + 0.5 * f);
                    cell.glow = halo * 0.4;
                    bg = bg.lerp(c * 0.8, 0.08);
                } else if f < 0.3 && hash2(col, row) < 0.012 {
                    cell.ch = '.';
                    cell.fg = Rgb::new(0.6, 0.62, 0.8) * (0.3 - f);
                }

                // The sun: a hot disk with a soft ring.
                if d < sun_r {
                    cell.ch = '@';
                    cell.fg = Rgb::hex(0xfff6dc) * 1.4;
                    cell.glow = 2.6;
                    bg = Rgb::hex(0xffe4a0);
                } else if d < sun_r * 1.35 {
                    cell.ch = '*';
                    cell.fg = Rgb::hex(0xffc66b) * 1.2;
                    cell.glow = 1.2;
                }
                cell.bg = bg;
            }
        }

        // Hazy headlands on both sides, behind the bridge.
        for col in 0..grid.width() as i32 {
            let px = (col as f32 + 0.5) * v.cw;
            let x = px / v.w;
            let left =
                (1.0 - smoothstep(0.18, 0.4, x)) * (0.075 + 0.035 * (x * 9.0 + 1.0).sin() + 0.015 * (x * 31.0).sin());
            let right = smoothstep(0.7, 0.9, x) * (0.06 + 0.03 * (x * 11.0 + 2.0).sin() + 0.012 * (x * 37.0).sin());
            let top = hz - left.max(right) * v.h;
            for row in 0..grid.height() as i32 {
                let py = (row as f32 + 0.5) * v.ch;
                if py <= top || py > hz {
                    continue;
                }
                let depth = (py - top) / (hz - top).max(1.0);
                let c = Rgb::hex(0x5a3a5e).lerp_oklab(Rgb::hex(0x2a1a33), depth);
                let cell = grid.get_mut(col, row).unwrap();
                let rim = py - top < v.ch;
                cell.ch = if rim {
                    '^'
                } else if hash2(col, row) < 0.4 {
                    ':'
                } else {
                    '.'
                };
                cell.fg = if rim { Rgb::hex(0xff9a6a) * 0.8 } else { c * 1.3 };
                cell.bg = c;
                cell.glow = if rim { 0.25 } else { 0.0 };
            }
        }

        // A few gulls.
        for i in 0..4 {
            let fi = i as f32;
            let x = ((t * 0.025 + fi * 0.27).fract()) * v.w;
            let y = v.h * (0.18 + 0.07 * (i % 3) as f32) + (t * 1.3 + fi).sin() * v.ch;
            let flap = (t * 5.0 + fi * 2.0).sin() > 0.0;
            let (cx, cy) = v.cell(x, y);
            grid.overlay(cx, cy, if flap { 'v' } else { '-' }, Rgb::hex(0x2a1a2a), 0.0);
        }
    }

    // --------------------------------------------------------------- bridge

    fn draw_bridge(&self, grid: &mut Grid, v: &View, ts: f32) {
        let ds = 0.004;
        let hot = Rgb::hex(0xfff0c0) * 1.6;
        let min_cell = v.cw.min(v.ch);
        for st in &self.strokes {
            let drawn = st.drawn(ts);
            if drawn <= 0.0 {
                continue;
            }
            let building = drawn < st.total();
            let base = match st.kind {
                Kind::Suspender => Rgb::hex(0x8a2a22),
                Kind::Cable => Rgb::hex(0xd9583a),
                Kind::Truss => Rgb::hex(0x7a2219),
                _ => Rgb::hex(ORANGE),
            };
            let mut prev: Option<(f32, f32)> = None;
            let mut last_cell = (i32::MIN, i32::MIN);
            let steps = (drawn / ds).ceil() as usize;
            for k in 0..=steps {
                let s = (k as f32 * ds).min(drawn);
                let p = st.at(s);
                let Some((px, py, d)) = v.project(p) else { continue };
                let dir = match prev {
                    Some((qx, qy)) => (px - qx, py - qy),
                    None => match v.project(st.at((s + ds).min(st.total()))) {
                        Some((nx, ny, _)) => (nx - px, ny - py),
                        None => (0.0, -1.0),
                    },
                };
                prev = Some((px, py));
                let behind = drawn - s;
                let center = v.cell(px, py);
                if center == last_cell && behind > 0.02 {
                    continue;
                }
                last_cell = center;

                // Backlit by the low sun: darker low down, warmer up high.
                let shade = 0.45 + 0.55 * (p.y / TOWER_TOP).clamp(0.0, 1.0);
                let heat = if building { (-behind * 22.0).exp() } else { 0.0 };
                let fg = (base * (0.7 + 0.6 * shade)).lerp_oklab(hot, heat);
                let glow = heat * 2.6 + if st.kind == Kind::Cable { 0.15 } else { 0.0 };
                let ch = match st.kind {
                    Kind::Leg => '#',
                    Kind::Truss => '#',
                    Kind::Deck => '=',
                    _ => slope_char(dir.0, dir.1),
                };
                let ch = if heat > 0.5 { '*' } else { ch };

                // Thick parts sweep across the stroke direction on screen.
                let len = (dir.0 * dir.0 + dir.1 * dir.1).sqrt().max(1e-4);
                let (nx, ny) = (-dir.1 / len, dir.0 / len);
                let radius_px = st.radius * v.h * FOCAL / d;
                let n = (radius_px / (min_cell * 0.5)).round() as i32;
                for j in -n..=n {
                    let off = if n == 0 { 0.0 } else { j as f32 / n as f32 * radius_px };
                    let (cx, cy) = v.cell(px + nx * off, py + ny * off);
                    if grid.plot(cx, cy, d, ch, fg, glow) && st.radius > 0.01 {
                        grid.get_mut(cx, cy).unwrap().bg = base * (0.25 * shade);
                    }
                }
            }
        }
    }

    fn draw_traffic(&self, grid: &mut Grid, v: &View, ts: f32) {
        let fade = smoothstep(self.build_end(), self.build_end() + 1.0, ts);
        if fade <= 0.0 {
            return;
        }
        for i in 0..16 {
            let dir = if i % 2 == 0 { 1.0 } else { -1.0 };
            let lane = if i % 2 == 0 { 0.035 } else { -0.035 };
            let phase = hash01(i as u32 * 7919);
            let u = ((ts * 0.22 * dir + phase * 2.0 * HALF_SPAN).rem_euclid(2.0 * HALF_SPAN)) - HALF_SPAN;
            let Some((px, py, d)) = v.project(world(u, lane, DECK_Y + 0.008)) else { continue };
            let (cx, cy) = v.cell(px, py);
            let c = if dir > 0.0 { Rgb::hex(0xfff3c8) } else { Rgb::hex(0xff3b2f) };
            grid.plot(cx, cy, d - 0.01, '•', c * (1.2 * fade), 0.9 * fade);
        }
        // Blinking warning lights on the tower tops.
        for (i, tu) in [-TOWER_U, TOWER_U].into_iter().enumerate() {
            if (ts * 2.2 + i as f32 * 1.7).sin() > 0.2
                && let Some((px, py, d)) = v.project(world(tu, HALF_WIDTH, TOWER_TOP + 0.02))
            {
                let (cx, cy) = v.cell(px, py);
                grid.plot(cx, cy, d - 0.02, '•', Rgb::hex(0xff2a1a) * 1.4, 1.6 * fade);
            }
        }
    }

    fn draw_sparks(&self, grid: &mut Grid, v: &View, alpha: f32) {
        for p in &self.sparks.particles {
            let Some((px, py, d)) = v.project(p.render_pos(alpha)) else { continue };
            let (cx, cy) = v.cell(px, py);
            let age = p.t();
            let c = Rgb::hex(0xfff0b0).lerp_oklab(Rgb::hex(0xff5a1f), age);
            let ch = if age < 0.4 {
                '*'
            } else if age < 0.75 {
                '·'
            } else {
                '.'
            };
            grid.plot(cx, cy, d - 0.01, ch, c * (1.4 - age), 2.0 * (1.0 - age));
        }
    }

    // ---------------------------------------------------------------- water

    /// Rows below the horizon mirror everything above it, broken up by waves,
    /// with a glittering path under the sun.
    fn draw_water(&self, grid: &mut Grid, v: &View, t: f32) {
        let hz = v.horizon_px();
        let hz_row = hz / v.ch;
        let (sun_x, _, _) = v.sun();
        let sun_col = sun_x / v.cw;
        let deep = Rgb::hex(0x0d2a3f);
        let sunlight = Rgb::hex(0xfff1c4);
        let tick = (t * 6.0) as i32;
        for row in 0..grid.height() as i32 {
            let py = (row as f32 + 0.5) * v.ch;
            if py <= hz {
                continue;
            }
            let sy = (hz - py) / (FOCAL * v.h); // negative below the horizon
            let z = CAM_Y / -sy;
            let below = row as f32 - hz_row; // rows below the horizon
            for col in 0..grid.width() as i32 {
                let px = (col as f32 + 0.5) * v.cw;
                let sx = (px - v.w * 0.5) / (FOCAL * v.h);
                let dist = z * (sx * sx + sy * sy + 1.0).sqrt();
                if grid.depth(col, row) < dist {
                    continue; // the bridge (or a spark) is in front of the water here
                }
                let x = sx * z;
                let wave = 0.5 * (x * 7.0 + z * 3.0 - t * 1.6).sin()
                    + 0.3 * (x * 13.0 - z * 5.0 + t * 2.3).sin()
                    + 0.2 * (z * 11.0 + t * 1.1).sin();

                // Mirror: sample the cell reflected about the horizon, wobbled by the waves.
                let wob = 0.4 + 1.6 / z.max(0.4);
                let mrow = ((2.0 * hz_row - row as f32 - 1.0) + wave * wob * 0.6).round().min(hz_row - 1.0) as i32;
                let mcol = col + (wave * wob).round() as i32;
                let src = grid.get(mcol.clamp(0, grid.width() as i32 - 1), mrow.max(0)).copied();
                let (refl, refl_glow) = match src {
                    Some(c) if c.ch != ' ' => (c.fg.lerp(c.bg, 0.5), c.glow),
                    Some(c) => (c.bg, 0.0),
                    None => (deep, 0.0),
                };
                let near = smoothstep(0.0, 12.0, below);
                let color = refl.lerp_oklab(deep, 0.25 + 0.3 * near) * 0.85;

                let cell = grid.get_mut(col, row).unwrap();
                cell.ch = if wave > 0.35 {
                    '~'
                } else if wave > -0.25 {
                    '-'
                } else {
                    '.'
                };
                cell.fg = color * 1.1;
                cell.bg = color * 0.45;
                cell.glow = refl_glow * 0.35;

                // The sun's glitter path widens toward the viewer.
                let half_w = 1.0 + below * 0.55;
                let dcol = (col as f32 - sun_col).abs();
                if dcol < half_w {
                    let k = 1.0 - dcol / half_w;
                    let n = hash2(col * 31 + (x * 40.0) as i32, row * 17 + tick);
                    if n > 0.62 - 0.35 * k * k {
                        cell.ch = if n > 0.9 { '=' } else { '-' };
                        cell.fg = sunlight * (0.7 + 0.8 * k);
                        cell.glow = 0.4 + 1.8 * k * k;
                    }
                }
            }
        }
    }
}

impl Scene for Bridge {
    fn update(&mut self, dt: f32, time: f64) {
        self.sim_time = time;
        let ts = (time - self.t0) as f32;

        // Sparks fly off the cable heads while they're being strung.
        self.spark_acc += dt * 70.0;
        while self.spark_acc >= 1.0 {
            self.spark_acc -= 1.0;
            for &ci in &self.cables {
                let st = &self.strokes[ci];
                let drawn = st.drawn(ts);
                if drawn <= 0.0 || drawn >= st.total() {
                    continue;
                }
                let head = st.at(drawn);
                let vel = self.rng.unit_vec3() * self.rng.range(0.1, 0.45) + Vec3::Y * 0.25;
                let mut p = Particle::new(head, vel, self.rng.range(0.5, 1.1));
                p.drag = 1.2;
                self.sparks.spawn(p);
            }
        }
        self.sparks.update(dt, |_| Vec3::new(0.0, -1.6, 0.0));
        self.sparks.particles.retain(|p| p.pos.y > 0.0); // fizzle out in the water
    }

    fn draw(&mut self, f: &mut Frame) {
        let (gw, gh) = f.size();
        let v = View { w: gw as f32 * f.cell_w, h: gh as f32 * f.cell_h, cw: f.cell_w, ch: f.cell_h };
        let t = f.time as f32;
        let ts = (f.time - self.t0) as f32;
        f.settings.bloom_intensity = 0.85;
        self.draw_sky(f.grid, &v, t);
        self.draw_bridge(f.grid, &v, ts);
        self.draw_traffic(f.grid, &v, ts);
        self.draw_sparks(f.grid, &v, f.alpha);
        self.draw_water(f.grid, &v, t);
    }

    fn key(&mut self, key: &Key) {
        if let Key::Character(c) = key
            && c.eq_ignore_ascii_case("r")
        {
            self.t0 = self.sim_time;
            self.sparks.clear();
        }
    }

    fn hud(&self) -> String {
        format!("{} strokes · {} sparks · R rebuild", self.strokes.len(), self.sparks.len())
    }
}

fn main() -> Result<(), String> {
    let config = Config { font_px: 8.0, ..Config::new("rusteezy — golden gate") }.with_args();
    rusteezy::run(config, Bridge::new())
}
