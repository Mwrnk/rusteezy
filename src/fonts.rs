//! Bundled fonts (feature `default-font`, on by default).
//!
//! - **Cascadia Mono** — primary font. © Microsoft, SIL Open Font License 1.1.
//! - **DejaVu Sans Mono** — fallback for glyphs Cascadia lacks (★ ✦ …).
//!   Bitstream Vera license; DejaVu changes are public domain.
//!
//! The license texts ship with the crate in `assets/fonts/`.

/// Cascadia Mono Regular (TTF).
pub const CASCADIA_MONO: &[u8] = include_bytes!("../assets/fonts/CascadiaMono-Regular.ttf");

/// DejaVu Sans Mono (TTF).
pub const DEJAVU_SANS_MONO: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono.ttf");
