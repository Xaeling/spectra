use serde::{Deserialize, Serialize};

/// 与 UI / 渲染无关的颜色表示（sRGB + alpha）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    pub const WHITE: Self = Self::rgb(255, 255, 255);
    pub const BLACK: Self = Self::rgb(0, 0, 0);

    pub fn hex(s: &str) -> Self {
        let s = s.trim_start_matches('#');
        let v = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).unwrap_or(0);
        match s.len() {
            6 => Self::rgb(v(0), v(2), v(4)),
            8 => Self::rgba(v(0), v(2), v(4), v(6)),
            _ => Self::BLACK,
        }
    }

    pub fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    /// 与白色按 alpha 预混，得到不透明色（导出时避免依赖混合器行为）。
    pub fn over_white(self) -> Self {
        let a = self.a as f32 / 255.0;
        let mix = |c: u8| (c as f32 * a + 255.0 * (1.0 - a)).round() as u8;
        Self::rgb(mix(self.r), mix(self.g), mix(self.b))
    }

    pub fn over(self, bg: Rgba) -> Self {
        let a = self.a as f32 / 255.0;
        let mix = |f: u8, b: u8| (f as f32 * a + b as f32 * (1.0 - a)).round() as u8;
        Self::rgb(mix(self.r, bg.r), mix(self.g, bg.g), mix(self.b, bg.b))
    }

    /// 变暗（用于描边、强调）。保色相，避免直接乘 RGB 发黑。
    pub fn darken(self, amount: f32) -> Self {
        let (h, s, l) = rgb_to_hsl(self.r, self.g, self.b);
        let l = (l * (1.0 - amount)).clamp(0.0, 1.0);
        let s = (s * 1.05).clamp(0.0, 1.0);
        let (r, g, b) = hsl_to_rgb(h, s, l);
        Self::rgba(r, g, b, self.a)
    }

    /// 变亮（用于 hover、底色）。
    pub fn lighten(self, amount: f32) -> Self {
        let (h, s, l) = rgb_to_hsl(self.r, self.g, self.b);
        let l = (l + (1.0 - l) * amount).clamp(0.0, 1.0);
        let (r, g, b) = hsl_to_rgb(h, s, l);
        Self::rgba(r, g, b, self.a)
    }

    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

pub fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    if s <= f32::EPSILON {
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let conv = |mut t: f32| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        let v = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        (v * 255.0).round() as u8
    };
    (conv(h + 1.0 / 3.0), conv(h), conv(h - 1.0 / 3.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let c = Rgba::hex("#5B6CFF");
        assert_eq!((c.r, c.g, c.b), (0x5B, 0x6C, 0xFF));
        assert_eq!(c.to_hex(), "#5B6CFF");
    }

    #[test]
    fn premix_white() {
        let c = Rgba::rgba(0, 0, 0, 128).over_white();
        assert!((c.r as i32 - 127).abs() <= 1);
    }

    #[test]
    fn hsl_roundtrip() {
        for hex in ["#E64B35", "#4DBBD5", "#009E73", "#7A8B99"] {
            let c = Rgba::hex(hex);
            let (h, s, l) = rgb_to_hsl(c.r, c.g, c.b);
            let (r, g, b) = hsl_to_rgb(h, s, l);
            assert!((r as i32 - c.r as i32).abs() <= 1, "{hex} r {r} {}", c.r);
            assert!((g as i32 - c.g as i32).abs() <= 1, "{hex} g {g} {}", c.g);
            assert!((b as i32 - c.b as i32).abs() <= 1, "{hex} b {b} {}", c.b);
        }
    }
}
