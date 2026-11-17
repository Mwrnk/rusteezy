//! Helpers shared by the performance examples (included with `#[path]`).
#![allow(dead_code)]

use rusteezy::{Frame, Rgb};

/// Write a stats line on the bottom row.
pub fn stats(f: &mut Frame, text: &str) {
    let row = f.grid.height() as i32 - 1;
    f.grid.put_str(1, row, text, Rgb::new(0.72, 0.76, 0.82), Some(Rgb::hex(0x0b0d14)));
}

/// `1234567` → `"1,234,567"`.
pub fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Value of `--flag <value>` on the command line.
pub fn arg<T: std::str::FromStr>(flag: &str) -> Option<T> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok())
}

/// Exponential moving average for smoothing timings.
pub fn smooth(prev: f32, next: f32) -> f32 {
    if prev == 0.0 { next } else { prev * 0.9 + next * 0.1 }
}
