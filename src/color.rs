//! Linear-light RGB color with sRGB and OKLab helpers.

use std::ops::{Add, Mul};

/// A color in **linear** RGB. Values above 1.0 are allowed (HDR).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb::new(0.0, 0.0, 0.0);
    pub const WHITE: Rgb = Rgb::new(1.0, 1.0, 1.0);

    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    /// From 8-bit sRGB components (what color pickers give you).
    pub fn srgb8(r: u8, g: u8, b: u8) -> Self {
        Self::new(srgb_to_linear(r as f32 / 255.0), srgb_to_linear(g as f32 / 255.0), srgb_to_linear(b as f32 / 255.0))
    }

    /// From a `0xRRGGBB` sRGB hex value.
    pub fn hex(rgb: u32) -> Self {
        Self::srgb8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
    }

    pub fn scale(self, k: f32) -> Self {
        Self::new(self.r * k, self.g * k, self.b * k)
    }

    /// Plain linear-RGB interpolation.
    pub fn lerp(self, o: Rgb, t: f32) -> Self {
        Self::new(self.r + (o.r - self.r) * t, self.g + (o.g - self.g) * t, self.b + (o.b - self.b) * t)
    }

    /// Perceptually even interpolation through OKLab.
    pub fn lerp_oklab(self, o: Rgb, t: f32) -> Self {
        let a = self.to_oklab();
        let b = o.to_oklab();
        Self::from_oklab([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t])
    }

    pub fn luminance(self) -> f32 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }

    pub fn max_component(self) -> f32 {
        self.r.max(self.g).max(self.b)
    }

    pub fn to_oklab(self) -> [f32; 3] {
        let l = 0.412_221_46 * self.r + 0.536_332_55 * self.g + 0.051_445_995 * self.b;
        let m = 0.211_903_5 * self.r + 0.680_699_5 * self.g + 0.107_396_96 * self.b;
        let s = 0.088_302_46 * self.r + 0.281_718_85 * self.g + 0.629_978_7 * self.b;
        let (l, m, s) = (l.max(0.0).cbrt(), m.max(0.0).cbrt(), s.max(0.0).cbrt());
        [
            0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
            1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
            0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
        ]
    }

    pub fn from_oklab(lab: [f32; 3]) -> Self {
        let l = lab[0] + 0.396_337_78 * lab[1] + 0.215_803_76 * lab[2];
        let m = lab[0] - 0.105_561_346 * lab[1] - 0.063_854_17 * lab[2];
        let s = lab[0] - 0.089_484_18 * lab[1] - 1.291_485_5 * lab[2];
        let (l, m, s) = (l * l * l, m * m * m, s * s * s);
        Self::new(
            4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
            -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s,
            -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
        )
    }
}

impl Add for Rgb {
    type Output = Rgb;
    fn add(self, o: Rgb) -> Rgb {
        Rgb::new(self.r + o.r, self.g + o.g, self.b + o.b)
    }
}

impl Mul<f32> for Rgb {
    type Output = Rgb;
    fn mul(self, k: f32) -> Rgb {
        self.scale(k)
    }
}

impl Mul<Rgb> for Rgb {
    type Output = Rgb;
    fn mul(self, o: Rgb) -> Rgb {
        Rgb::new(self.r * o.r, self.g * o.g, self.b * o.b)
    }
}

pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oklab_roundtrip() {
        let c = Rgb::new(0.2, 0.5, 0.9);
        let back = Rgb::from_oklab(c.to_oklab());
        assert!((c.r - back.r).abs() < 1e-4);
        assert!((c.g - back.g).abs() < 1e-4);
        assert!((c.b - back.b).abs() < 1e-4);
    }

    #[test]
    fn lerp_endpoints() {
        let a = Rgb::hex(0x102030);
        let b = Rgb::hex(0xffcc00);
        let s = a.lerp_oklab(b, 0.0);
        let e = a.lerp_oklab(b, 1.0);
        assert!((s.r - a.r).abs() < 1e-4 && (e.g - b.g).abs() < 1e-4);
    }
}
