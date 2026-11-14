//! 字体排版：把文字转成矢量路径。
//!
//! 出版图不能依赖读者机器上装了什么字体，所以 PNG / SVG / PDF 的文字全部转曲。
//! 中英混排时逐个字符做 fallback（Times 没有 CJK 时落到 MiSans）。

use std::sync::Arc;

use ab_glyph::{Font, FontVec, GlyphId, OutlineCurve};

use crate::core::color::Rgba;
use crate::plot::scene::{LineCap, Path, Pt, Scene, Stroke, Transform};

pub const SANS_BYTES: &[u8] = include_bytes!("../../assets/fonts/MiSans-Regular.ttf");
pub const SERIF_BYTES: &[u8] = include_bytes!("../../assets/fonts/TimesNewRoman.ttf");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Sans,
    Serif,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VAlign {
    Top,
    Middle,
    Baseline,
    Bottom,
}

pub struct FontBook {
    sans: FontVec,
    serif: FontVec,
}

impl FontBook {
    pub fn load_builtin() -> Self {
        let sans = FontVec::try_from_vec(SANS_BYTES.to_vec()).expect("内置 MiSans 字体损坏");
        let serif = FontVec::try_from_vec(SERIF_BYTES.to_vec()).expect("内置 Times 字体损坏");
        Self { sans, serif }
    }

    pub fn shared() -> Arc<Self> {
        Arc::new(Self::load_builtin())
    }

    fn primary(&self, family: Family) -> &FontVec {
        match family {
            Family::Sans => &self.sans,
            Family::Serif => &self.serif,
        }
    }

    /// 逐字符选择字体：首选字体缺字时回退到 MiSans。
    fn glyph(&self, family: Family, ch: char) -> (&FontVec, GlyphId) {
        let font = self.primary(family);
        let gid = font.glyph_id(ch);
        if gid.0 != 0 {
            (font, gid)
        } else {
            let gid = self.sans.glyph_id(ch);
            (&self.sans, gid)
        }
    }

    pub fn ascent(&self, size: f32, family: Family) -> f32 {
        let f = self.primary(family);
        let upm = f.units_per_em().unwrap_or(1000.0);
        f.ascent_unscaled() / upm * size
    }

    pub fn descent(&self, size: f32, family: Family) -> f32 {
        let f = self.primary(family);
        let upm = f.units_per_em().unwrap_or(1000.0);
        -f.descent_unscaled() / upm * size
    }

    pub fn line_height(&self, size: f32, family: Family) -> f32 {
        (self.ascent(size, family) + self.descent(size, family)).max(size * 1.1)
    }

    /// 文本宽度（pt/px，取决于 size 单位）。
    pub fn measure(&self, text: &str, size: f32, family: Family) -> f32 {
        let mut pen = 0.0;
        let mut prev: Option<(usize, GlyphId)> = None;
        for ch in text.chars() {
            let (font, gid) = self.glyph(family, ch);
            let upm = font.units_per_em().unwrap_or(1000.0);
            let scale = size / upm;
            if let Some((pid, pgid)) = prev {
                if pid == font_id(font) {
                    pen += font.kern_unscaled(pgid, gid) * scale;
                }
            }
            pen += font.h_advance_unscaled(gid) * scale;
            prev = Some((font_id(font), gid));
        }
        pen
    }

    /// 生成以 (0,0) 为基线起点的文字轮廓。
    pub fn path(&self, text: &str, size: f32, family: Family) -> Path {
        let mut path = Path::new();
        let mut pen = 0.0_f32;
        let mut prev: Option<(usize, GlyphId)> = None;
        for ch in text.chars() {
            let (font, gid) = self.glyph(family, ch);
            let upm = font.units_per_em().unwrap_or(1000.0);
            let scale = size / upm;
            if let Some((pid, pgid)) = prev {
                if pid == font_id(font) {
                    pen += font.kern_unscaled(pgid, gid) * scale;
                }
            }
            if let Some(outline) = font.outline(gid) {
                append_outline(&mut path, &outline.curves, pen, scale);
            }
            pen += font.h_advance_unscaled(gid) * scale;
            prev = Some((font_id(font), gid));
        }
        path
    }

    /// 把文字画进场景。
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        scene: &mut Scene,
        text: &str,
        at: Pt,
        size: f32,
        family: Family,
        color: Rgba,
        halign: HAlign,
        valign: VAlign,
        rotation: f32,
        bold: bool,
    ) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let width = self.measure(text, size, family);
        let baseline = match valign {
            VAlign::Top => at.y + self.ascent(size, family),
            VAlign::Middle => at.y + (self.ascent(size, family) - self.descent(size, family)) / 2.0,
            VAlign::Baseline => at.y,
            VAlign::Bottom => at.y - self.descent(size, family),
        };
        let start_x = match halign {
            HAlign::Left => at.x,
            HAlign::Center => at.x - width / 2.0,
            HAlign::Right => at.x - width,
        };
        let mut path = self.path(text, size, family);
        let mut t = Transform::translate(start_x, baseline);
        if rotation.abs() > 1e-6 {
            let r = Transform::around(at, rotation);
            t = r.then(&t);
        }
        path.transform(&t);
        scene.fill(path.clone(), color);
        if bold {
            let mut stroke = Stroke::new((size * 0.032).max(0.22), color);
            stroke.cap = LineCap::Round;
            scene.stroke(path, stroke);
        }
        width
    }
}

fn font_id(f: &FontVec) -> usize {
    f as *const FontVec as usize
}

fn append_outline(path: &mut Path, curves: &[OutlineCurve], pen: f32, scale: f32) {
    let map = |p: ab_glyph::Point| Pt::new(pen + p.x * scale, -p.y * scale);
    let mut current: Option<Pt> = None;
    for curve in curves {
        match curve {
            OutlineCurve::Line(a, b) => {
                let a = map(*a);
                let b = map(*b);
                if current != Some(a) {
                    path.move_to(a);
                }
                path.line_to(b);
                current = Some(b);
            }
            OutlineCurve::Quad(a, c, b) => {
                let a = map(*a);
                let c = map(*c);
                let b = map(*b);
                if current != Some(a) {
                    path.move_to(a);
                }
                path.quad_to(c, b);
                current = Some(b);
            }
            OutlineCurve::Cubic(a, c1, c2, b) => {
                let a = map(*a);
                let c1 = map(*c1);
                let c2 = map(*c2);
                let b = map(*b);
                if current != Some(a) {
                    path.move_to(a);
                }
                path.cubic_to(c1, c2, b);
                current = Some(b);
            }
        }
    }
    // 字形本身是闭合轮廓；补一个 Close 让填充规则生效。
    path.close();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_ascii_and_cjk() {
        let f = FontBook::load_builtin();
        assert!(f.measure("Hello", 10.0, Family::Sans) > 20.0);
        assert!(f.measure("中文", 10.0, Family::Sans) > 15.0);
        // Times 没有 CJK，应当回退而不是量出 0
        assert!(f.measure("中文", 10.0, Family::Serif) > 15.0);
    }

    #[test]
    fn outlines_are_not_empty() {
        let f = FontBook::load_builtin();
        let p = f.path("Ag", 12.0, Family::Serif);
        assert!(!p.segs.is_empty());
        let b = p.bounds().unwrap();
        assert!(b.w > 5.0 && b.h > 5.0);
    }
}
