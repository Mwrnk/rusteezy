//! # rusteezy
//!
//! A GPU-accelerated ASCII renderer.
//!
//! You fill a [`Grid`] of character cells (glyph, foreground, background and a
//! `glow` amount). rusteezy draws the whole grid in **one instanced draw call**
//! into an HDR target, then runs a **bloom** chain so glowing cells bleed light
//! onto their neighbours. A terminal can't do that. Around that core you get:
//!
//! - a fixed-step clock, so animation is frame-rate independent ([`Scene::update`]),
//! - easing curves and [`Tween`]s, with colors interpolated in OKLab ([`Rgb::lerp_oklab`]),
//! - a small physics [`ParticleSystem`] with render interpolation,
//! - a per-cell depth buffer for compositing 3D scenes ([`Grid::plot`]),
//! - font fallback ([`GlyphAtlas::with_fallbacks`]) and bundled fonts,
//! - PNG capture, deterministic frame recording and an off-screen benchmark ([`cli`]).
//!
//! ## Quickstart
//!
//! ```no_run
//! use rusteezy::{Config, Frame, Rgb, Scene};
//!
//! struct Hello;
//!
//! impl Scene for Hello {
//!     fn draw(&mut self, f: &mut Frame) {
//!         let (w, h) = f.size();
//!         let text = "hello, rusteezy";
//!         let x = (w as i32 - text.len() as i32) / 2;
//!         let pulse = 0.5 + 0.5 * (f.time as f32 * 3.0).sin();
//!         for (i, ch) in text.chars().enumerate() {
//!             // Hue sweeps across the word; `glow` feeds the bloom pass.
//!             let t = i as f32 / text.len() as f32;
//!             let color = Rgb::hex(0xff6a3d).lerp_oklab(Rgb::hex(0xffd23f), t);
//!             f.grid.overlay(x + i as i32, h as i32 / 2, ch, color, 0.5 + 1.5 * pulse);
//!         }
//!     }
//! }
//!
//! fn main() -> Result<(), String> {
//!     rusteezy::run(Config::new("hello").with_args(), Hello)
//! }
//! ```
//!
//! See the `examples/` directory for more: fireworks, fire, plasma, a torus,
//! a starfield, the crab logo and a 3D Christmas tree.

pub mod app;
pub mod atlas;
pub mod cli;
pub mod clock;
pub mod color;
#[cfg(feature = "default-font")]
pub mod fonts;
pub mod grid;
pub mod particles;
pub mod png;
pub mod render;
pub mod rng;
pub mod tween;

pub use app::{Config, Frame, Record, Scene, run};
pub use atlas::GlyphAtlas;
pub use clock::FixedClock;
pub use color::Rgb;
pub use grid::{Cell, Grid};
pub use particles::{Particle, ParticleSystem};
pub use render::{RenderSettings, RenderTiming};
pub use rng::Rng;
pub use tween::{Tween, ease};

/// Re-exported so scenes can match on keys without depending on winit directly.
pub use winit::keyboard::{Key, NamedKey};

pub use glam;
