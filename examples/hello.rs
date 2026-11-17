//! The smallest rusteezy program: a glowing, pulsing word.
//!
//! ```text
//! cargo run --release --example hello
//! ```

use rusteezy::{Config, Frame, Rgb, Scene};

struct Hello;

impl Scene for Hello {
    fn draw(&mut self, f: &mut Frame) {
        let (w, h) = f.size();
        let text = "hello, rusteezy";
        let x = (w as i32 - text.len() as i32) / 2;
        let pulse = 0.5 + 0.5 * (f.time as f32 * 3.0).sin();
        for (i, ch) in text.chars().enumerate() {
            // Hue sweeps across the word; `glow` feeds the bloom pass.
            let t = i as f32 / text.len() as f32;
            let color = Rgb::hex(0xff6a3d).lerp_oklab(Rgb::hex(0xffd23f), t);
            f.grid.overlay(x + i as i32, h as i32 / 2, ch, color, 0.5 + 1.5 * pulse);
        }
    }
}

fn main() -> Result<(), String> {
    rusteezy::run(Config::new("hello").with_args(), Hello)
}
