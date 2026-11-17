//! Glyph atlas: rasterizes a fixed glyph set once into a single-channel texture.

use std::collections::HashMap;

/// Characters always included (printable ASCII is added automatically).
pub const DEFAULT_EXTRA: &str = "★☆✦✧·•●○◆◇°░▒▓█─│╱╲";

pub struct GlyphAtlas {
    pub cell_w: u32,
    pub cell_h: u32,
    pub cols: u32,
    pub rows: u32,
    /// R8 coverage, `cols*cell_w` wide, `rows*cell_h` tall.
    pub pixels: Vec<u8>,
    ascii: [u16; 128],
    map: HashMap<char, u16>,
    fallback: u16,
}

impl GlyphAtlas {
    /// Rasterize printable ASCII plus `extra` at `px` pixels.
    /// Characters missing from the font are skipped and map to `?`.
    pub fn new(font_bytes: &[u8], px: f32, extra: &str) -> Result<Self, String> {
        Self::with_fallbacks(font_bytes, &[], px, extra)
    }

    /// Like [`new`](Self::new), but glyphs missing from the primary font are taken
    /// from the first fallback font that has them. Cell size and baseline always
    /// come from the primary font.
    pub fn with_fallbacks(primary: &[u8], fallbacks: &[&[u8]], px: f32, extra: &str) -> Result<Self, String> {
        let fonts = std::iter::once(primary)
            .chain(fallbacks.iter().copied())
            .map(|b| fontdue::Font::from_bytes(b, fontdue::FontSettings::default()).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        let font = &fonts[0];
        let lm = font.horizontal_line_metrics(px).ok_or("font has no horizontal metrics")?;
        let cell_w = font.metrics('M', px).advance_width.ceil().max(1.0) as u32;
        let cell_h = (lm.ascent - lm.descent).ceil().max(1.0) as u32;
        let baseline = lm.ascent.round() as i32;

        // (char, index of the font that provides it)
        let mut chars: Vec<(char, usize)> = (32u8..127).map(|b| (b as char, 0)).collect();
        for c in extra.chars() {
            if chars.iter().any(|&(x, _)| x == c) {
                continue;
            }
            if let Some(fi) = fonts.iter().position(|f| f.lookup_glyph_index(c) != 0) {
                chars.push((c, fi));
            }
        }

        let cols = 16u32;
        let rows = (chars.len() as u32).div_ceil(cols);
        let aw = (cols * cell_w) as usize;
        let mut pixels = vec![0u8; aw * (rows * cell_h) as usize];
        let mut ascii = [0u16; 128];
        let mut map = HashMap::new();

        for (slot, &(c, fi)) in chars.iter().enumerate() {
            let font = &fonts[fi];
            let slot = slot as u16;
            if (c as u32) < 128 {
                ascii[c as usize] = slot;
            } else {
                map.insert(c, slot);
            }
            if c == ' ' {
                continue;
            }
            // Shrink glyphs that don't fit the cell (some symbols, fallback fonts).
            let m = font.metrics(c, px);
            let fit_w = if m.width as u32 > cell_w { cell_w as f32 / m.width as f32 } else { 1.0 };
            let fit_h = if m.height as u32 > cell_h { cell_h as f32 / m.height as f32 } else { 1.0 };
            let gpx = px * fit_w.min(fit_h);
            let (m, bitmap) = font.rasterize(c, gpx);
            let ox = ((slot as u32 % cols) * cell_w) as i32;
            let oy = ((slot as u32 / cols) * cell_h) as i32;
            let gx = m.xmin.max(0).min(cell_w as i32 - m.width as i32).max(0);
            let gy = baseline - (m.ymin + m.height as i32);
            for y in 0..m.height as i32 {
                let ty = gy + y;
                if ty < 0 || ty >= cell_h as i32 {
                    continue;
                }
                for x in 0..m.width as i32 {
                    let tx = gx + x;
                    if tx < 0 || tx >= cell_w as i32 {
                        continue;
                    }
                    let v = bitmap[(y * m.width as i32 + x) as usize];
                    pixels[((oy + ty) as usize) * aw + (ox + tx) as usize] = v;
                }
            }
        }

        let fallback = ascii['?' as usize];
        Ok(Self { cell_w, cell_h, cols, rows, pixels, ascii, map, fallback })
    }

    pub fn width(&self) -> u32 {
        self.cols * self.cell_w
    }

    pub fn height(&self) -> u32 {
        self.rows * self.cell_h
    }

    /// Atlas slot for a character (`?` if it wasn't rasterized).
    #[inline]
    pub fn slot(&self, c: char) -> u16 {
        let u = c as u32;
        if u < 128 {
            if u < 32 { self.fallback } else { self.ascii[u as usize] }
        } else {
            *self.map.get(&c).unwrap_or(&self.fallback)
        }
    }

    pub fn has(&self, c: char) -> bool {
        (c as u32) < 128 || self.map.contains_key(&c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CASCADIA: &[u8] = include_bytes!("../assets/fonts/CascadiaMono-Regular.ttf");
    const DEJAVU: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono.ttf");

    #[test]
    fn cascadia_alone_lacks_stars() {
        let atlas = GlyphAtlas::new(CASCADIA, 20.0, DEFAULT_EXTRA).unwrap();
        assert!(!atlas.has('★'));
        assert!(atlas.has('█'));
    }

    #[test]
    fn fallback_font_fills_the_gaps() {
        let atlas = GlyphAtlas::with_fallbacks(CASCADIA, &[DEJAVU], 20.0, DEFAULT_EXTRA).unwrap();
        let missing: String = DEFAULT_EXTRA.chars().filter(|&c| !atlas.has(c)).collect();
        assert!(missing.is_empty(), "missing glyphs: {missing}");
        // Cell size comes from the primary font.
        let primary = GlyphAtlas::new(CASCADIA, 20.0, "").unwrap();
        assert_eq!((atlas.cell_w, atlas.cell_h), (primary.cell_w, primary.cell_h));
        assert!(atlas.cell_h > atlas.cell_w);
    }
}
