//! The character grid: the core primitive of the library.

use crate::color::Rgb;

/// One character cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub ch: char,
    /// Glyph color (linear RGB).
    pub fg: Rgb,
    /// Background color (linear RGB).
    pub bg: Rgb,
    /// Emission. 0 = plain, >0 brightens the glyph past 1.0 so it feeds bloom.
    pub glow: f32,
}

impl Cell {
    pub const EMPTY: Cell = Cell { ch: ' ', fg: Rgb::BLACK, bg: Rgb::BLACK, glow: 0.0 };
}

/// A `w × h` grid of cells plus a per-cell depth buffer used while compositing
/// (smaller = closer). Depth is a CPU-side aid and is never uploaded.
pub struct Grid {
    w: usize,
    h: usize,
    cells: Vec<Cell>,
    depth: Vec<f32>,
}

impl Grid {
    pub fn new(w: usize, h: usize) -> Self {
        Self { w, h, cells: vec![Cell::EMPTY; w * h], depth: vec![f32::INFINITY; w * h] }
    }

    pub fn width(&self) -> usize {
        self.w
    }

    pub fn height(&self) -> usize {
        self.h
    }

    pub fn resize(&mut self, w: usize, h: usize) {
        self.w = w;
        self.h = h;
        self.cells.resize(w * h, Cell::EMPTY);
        self.depth.resize(w * h, f32::INFINITY);
    }

    /// Reset every cell to a blank with the given background, and clear depth.
    pub fn clear(&mut self, bg: Rgb) {
        self.cells.fill(Cell { bg, ..Cell::EMPTY });
        self.depth.fill(f32::INFINITY);
    }

    #[inline]
    fn index(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            None
        } else {
            Some(y as usize * self.w + x as usize)
        }
    }

    pub fn get(&self, x: i32, y: i32) -> Option<&Cell> {
        self.index(x, y).map(|i| &self.cells[i])
    }

    pub fn get_mut(&mut self, x: i32, y: i32) -> Option<&mut Cell> {
        self.index(x, y).map(|i| &mut self.cells[i])
    }

    pub fn set(&mut self, x: i32, y: i32, cell: Cell) {
        if let Some(i) = self.index(x, y) {
            self.cells[i] = cell;
        }
    }

    pub fn depth(&self, x: i32, y: i32) -> f32 {
        self.index(x, y).map_or(f32::NEG_INFINITY, |i| self.depth[i])
    }

    pub fn set_depth(&mut self, x: i32, y: i32, z: f32) {
        if let Some(i) = self.index(x, y) {
            self.depth[i] = z;
        }
    }

    /// Depth-tested write of glyph + fg + glow (background is kept).
    /// Returns true if the write passed the depth test.
    pub fn plot(&mut self, x: i32, y: i32, z: f32, ch: char, fg: Rgb, glow: f32) -> bool {
        let Some(i) = self.index(x, y) else { return false };
        if z > self.depth[i] {
            return false;
        }
        self.depth[i] = z;
        let c = &mut self.cells[i];
        c.ch = ch;
        c.fg = fg;
        c.glow = glow;
        true
    }

    /// Unconditional overlay write of glyph + fg + glow (no depth test or update).
    pub fn overlay(&mut self, x: i32, y: i32, ch: char, fg: Rgb, glow: f32) {
        if let Some(c) = self.get_mut(x, y) {
            c.ch = ch;
            c.fg = fg;
            c.glow = glow;
        }
    }

    /// Write a string left-to-right as an overlay.
    pub fn put_str(&mut self, x: i32, y: i32, s: &str, fg: Rgb, bg: Option<Rgb>) {
        for (i, ch) in s.chars().enumerate() {
            if let Some(c) = self.get_mut(x + i as i32, y) {
                c.ch = ch;
                c.fg = fg;
                c.glow = 0.0;
                if let Some(bg) = bg {
                    c.bg = bg;
                }
            }
        }
    }

    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    pub fn cells_mut(&mut self) -> &mut [Cell] {
        &mut self.cells
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plot_respects_depth() {
        let mut g = Grid::new(4, 4);
        g.clear(Rgb::BLACK);
        assert!(g.plot(1, 1, 5.0, 'a', Rgb::WHITE, 0.0));
        assert!(!g.plot(1, 1, 6.0, 'b', Rgb::WHITE, 0.0));
        assert!(g.plot(1, 1, 4.0, 'c', Rgb::WHITE, 0.0));
        assert_eq!(g.get(1, 1).unwrap().ch, 'c');
    }

    #[test]
    fn out_of_bounds_is_ignored() {
        let mut g = Grid::new(2, 2);
        g.set(-1, 0, Cell::EMPTY);
        g.put_str(1, 1, "hello", Rgb::WHITE, None);
        assert_eq!(g.get(1, 1).unwrap().ch, 'h');
        assert!(g.get(2, 1).is_none());
    }
}
