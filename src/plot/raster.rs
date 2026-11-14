//! tiny-skia 光栅后端：场景 → RGBA。

use tiny_skia::{
    FillRule as SkFillRule, LineCap as SkLineCap, LineJoin as SkLineJoin, Paint, PathBuilder,
    Pixmap, Shader, Stroke as SkStroke, StrokeDash, Transform as SkTransform,
};

use crate::core::color::Rgba;
use crate::plot::scene::{LineCap, LineJoin, Path, Scene, Seg};

fn sk_color(c: Rgba) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(c.r, c.g, c.b, c.a)
}

fn sk_cap(c: LineCap) -> SkLineCap {
    match c {
        LineCap::Butt => SkLineCap::Butt,
        LineCap::Round => SkLineCap::Round,
        LineCap::Square => SkLineCap::Square,
    }
}

fn sk_join(j: LineJoin) -> SkLineJoin {
    match j {
        LineJoin::Miter => SkLineJoin::Miter,
        LineJoin::Round => SkLineJoin::Round,
        LineJoin::Bevel => SkLineJoin::Bevel,
    }
}

pub fn to_sk_path(path: &Path) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    for seg in &path.segs {
        match seg {
            Seg::Move(p) => pb.move_to(p.x, p.y),
            Seg::Line(p) => pb.line_to(p.x, p.y),
            Seg::Quad(c, p) => pb.quad_to(c.x, c.y, p.x, p.y),
            Seg::Cubic(c1, c2, p) => pb.cubic_to(c1.x, c1.y, c2.x, c2.y, p.x, p.y),
            Seg::Close => pb.close(),
        }
    }
    pb.finish()
}

pub fn rasterize(scene: &Scene) -> (u32, u32, Vec<u8>) {
    let w = scene.width.round().max(1.0) as u32;
    let h = scene.height.round().max(1.0) as u32;
    let mut pixmap = match Pixmap::new(w, h) {
        Some(p) => p,
        None => return (w, h, vec![255; (w * h * 4) as usize]),
    };
    pixmap.fill(sk_color(scene.bg));

    for op in &scene.ops {
        let Some(path) = to_sk_path(&op.path) else {
            continue;
        };
        if let Some(fill) = op.fill {
            let mut paint = Paint::default();
            paint.shader = Shader::SolidColor(sk_color(fill));
            paint.anti_alias = true;
            pixmap.fill_path(
                &path,
                &paint,
                SkFillRule::Winding,
                SkTransform::identity(),
                None,
            );
        }
        if let Some(stroke) = &op.stroke {
            if !(stroke.width > 0.0) {
                continue;
            }
            let mut paint = Paint::default();
            paint.shader = Shader::SolidColor(sk_color(stroke.color));
            paint.anti_alias = true;
            let mut s = SkStroke {
                width: stroke.width,
                ..Default::default()
            };
            s.line_cap = sk_cap(stroke.cap);
            s.line_join = sk_join(stroke.join);
            if !stroke.dash.is_empty() {
                let mut dashes = stroke.dash.clone();
                if dashes.len() % 2 == 1 {
                    let last = *dashes.last().unwrap();
                    dashes.push(last);
                }
                s.dash = StrokeDash::new(dashes, 0.0);
            }
            pixmap.stroke_path(&path, &paint, &s, SkTransform::identity(), None);
        }
    }
    (w, h, pixmap.data().to_vec())
}
