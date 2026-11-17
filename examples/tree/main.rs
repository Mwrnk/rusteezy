//! A 3D Christmas tree that builds from the bottom up, with a spiral garland,
//! twinkling lights, a star, snow and sparks.
//!
//! ```text
//! cargo run --release --example tree [-- --strand]
//! ```
//!
//! Keys: R rebuild · G garland mode (grow / strand) · B bloom · F1 HUD · Esc quit

mod garland;
mod scene;
mod tree;

use rusteezy::{Config, cli::flag};

fn main() {
    let config = Config { width: 1280.0, height: 860.0, ..Config::new("rusteezy — tree") }.with_args();
    let scene = scene::TreeScene::new().with_strand(flag("--strand").then_some(3.0));
    if let Err(e) = rusteezy::run(config, scene) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
