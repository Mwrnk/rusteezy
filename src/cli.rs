//! Standard command-line flags, shared by every example.
//!
//! | Flag | Effect |
//! |---|---|
//! | `--font <px>` | font size in logical pixels |
//! | `--size <W>x<H>` | window size in logical pixels |
//! | `--hud` / `--no-hud` | show / hide the stats line (F1 toggles) |
//! | `--uncapped` | no vsync |
//! | `--verbose` | print GPU adapter info |
//! | `--bench [frames]` | off-screen benchmark, then quit |
//! | `--shots <dir> <t1,t2,…>` | save PNGs at those times, then quit |
//! | `--record <file.rgb>` | record raw RGB frames (see [`Record`]) |
//! | `--fps <n>` / `--start <s>` / `--seconds <s>` | recording options |

use std::path::{Path, PathBuf};

use crate::app::{Config, Record};

impl Config {
    /// Apply the standard flags from `std::env::args()`. Unknown flags are ignored,
    /// so examples can add their own.
    pub fn with_args(self) -> Self {
        self.apply_args(std::env::args().skip(1))
    }

    /// Apply the standard flags from an explicit argument list.
    pub fn apply_args<I: IntoIterator<Item = String>>(mut self, args: I) -> Self {
        let args: Vec<String> = args.into_iter().collect();
        let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
        let has = |flag: &str| args.iter().any(|a| a == flag);

        if let Some(px) = value("--font").and_then(|v| v.parse().ok()) {
            self.font_px = px;
        }
        if let Some((w, h)) = value("--size").and_then(|v| parse_size(&v)) {
            self.width = w;
            self.height = h;
        }
        if has("--hud") {
            self.show_hud = true;
        }
        if has("--no-hud") {
            self.show_hud = false;
        }
        if has("--uncapped") {
            self.vsync = false;
        }
        if has("--verbose") {
            self.verbose = true;
        }
        if has("--bench") {
            self.bench_frames = Some(value("--bench").and_then(|v| v.parse().ok()).unwrap_or(1200));
        }
        if let Some(i) = args.iter().position(|a| a == "--shots")
            && let (Some(dir), Some(times)) = (args.get(i + 1), args.get(i + 2))
        {
            self.captures = times
                .split(',')
                .filter_map(|t| t.parse::<f64>().ok())
                .map(|t| (t, Path::new(dir).join(format!("shot_{t:05.2}.png"))))
                .collect();
            self.exit_after_captures = !self.captures.is_empty();
        }
        if let Some(path) = value("--record") {
            self.record = Some(Record {
                path: PathBuf::from(path),
                fps: value("--fps").and_then(|v| v.parse().ok()).unwrap_or(30),
                start: value("--start").and_then(|v| v.parse().ok()).unwrap_or(0.0),
                seconds: value("--seconds").and_then(|v| v.parse().ok()).unwrap_or(5.0),
            });
        }
        self
    }
}

/// True if `flag` was passed on the command line.
pub fn flag(flag: &str) -> bool {
    std::env::args().any(|a| a == flag)
}

fn parse_size(s: &str) -> Option<(f64, f64)> {
    let (w, h) = s.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(args: &str) -> Config {
        Config::default().apply_args(args.split_whitespace().map(String::from))
    }

    #[test]
    fn parses_common_flags() {
        let c = cfg("--font 8 --size 800x500 --uncapped --hud --custom-flag");
        assert_eq!(c.font_px, 8.0);
        assert_eq!((c.width, c.height), (800.0, 500.0));
        assert!(!c.vsync && c.show_hud);
    }

    #[test]
    fn parses_record_and_shots() {
        let c = cfg("--record out.rgb --fps 24 --start 1.5 --seconds 4 --shots dir 1,2.5");
        let r = c.record.unwrap();
        assert_eq!((r.fps, r.start, r.seconds), (24, 1.5, 4.0));
        assert_eq!(c.captures.len(), 2);
        assert!(c.exit_after_captures);
    }

    #[test]
    fn bench_defaults() {
        assert_eq!(cfg("--bench").bench_frames, Some(1200));
        assert_eq!(cfg("--bench 50").bench_frames, Some(50));
        assert_eq!(cfg("").bench_frames, None);
    }
}
