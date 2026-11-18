# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- `bridge` example: a suspension bridge at golden hour that draws itself along
  its length, over water that mirrors the sky, sun and bridge.
- Performance examples, multi-threaded with rayon (a dev-dependency only):
  `galaxy` (300k stars), `flow` (200k particles), `raymarch` (per-cell ray
  marching with shadows, AO and reflections) and `life` (Game of Life on a dense grid).

## [0.1.0]

First release.

- Character grid with per-cell glyph, foreground, background, glow and a CPU depth buffer.
- wgpu renderer: the whole grid in one instanced draw, an HDR target, a dual-filter
  bloom chain and ACES tonemapping.
- Glyph atlas with fallback fonts. Cascadia Mono and DejaVu Sans Mono are bundled
  behind the `default-font` feature.
- Fixed-step clock, easing curves, tweens and OKLab color interpolation.
- Particle system with drag, per-particle color and render interpolation.
- Shared command-line flags for PNG capture, deterministic recording and an
  off-screen benchmark.
- Examples: hello, crab, tree, fireworks, fire, plasma, torus, starfield.
