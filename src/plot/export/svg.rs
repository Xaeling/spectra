//! SVG 导出：场景 IR → 纯路径 SVG（文字已转曲，不依赖字体）。

use crate::core::color::Rgba;
use crate::plot::scene::{LineCap, LineJoin, Path, Scene, Seg, Stroke};

fn f(v: f32) -> String {
    let mut s = format!("{v:.3}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s == "-0" {
        s = "0".into();
    }
    s
}

fn path_d(path: &Path) -> String {
    let mut d = String::new();
    for seg in &path.segs {
        match seg {
            Seg::Move(p) => {
                d.push_str(&format!("M{} {}", f(p.x), f(p.y)));
            }
            Seg::Line(p) => {
                d.push_str(&format!("L{} {}", f(p.x), f(p.y)));
            }
            Seg::Quad(c, p) => {
                d.push_str(&format!("Q{} {} {} {}", f(c.x), f(c.y), f(p.x), f(p.y)));
            }
            Seg::Cubic(a, b, p) => {
                d.push_str(&format!(
                    "C{} {} {} {} {} {}",
                    f(a.x),
                    f(a.y),
                    f(b.x),
                    f(b.y),
                    f(p.x),
                    f(p.y)
                ));
            }
            Seg::Close => d.push('Z'),
        }
        d.push(' ');
    }
    d
}

fn hex(c: Rgba) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b)
}

fn opacity(c: Rgba) -> String {
    if c.a == 255 {
        String::new()
    } else {
        format!("{:.3}", c.a as f32 / 255.0)
    }
}

fn stroke_attrs(st: &Stroke) -> String {
    let mut out = format!(
        " stroke=\"{}\" stroke-width=\"{}\"",
        hex(st.color),
        f(st.width)
    );
    let op = opacity(st.color);
    if !op.is_empty() {
        out.push_str(&format!(" stroke-opacity=\"{op}\""));
    }
    let cap = match st.cap {
        LineCap::Butt => "butt",
        LineCap::Round => "round",
        LineCap::Square => "square",
    };
    let join = match st.join {
        LineJoin::Miter => "miter",
        LineJoin::Round => "round",
        LineJoin::Bevel => "bevel",
    };
    out.push_str(&format!(
        " stroke-linecap=\"{cap}\" stroke-linejoin=\"{join}\""
    ));
    if !st.dash.is_empty() {
        let pattern: Vec<String> = st.dash.iter().map(|v| f(*v)).collect();
        out.push_str(&format!(" stroke-dasharray=\"{}\"", pattern.join(",")));
    }
    out
}

pub fn to_svg(scene: &Scene, width_pt: f32, height_pt: f32) -> String {
    let mut out = String::with_capacity(64 * 1024);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}pt\" height=\"{}pt\" viewBox=\"0 0 {} {}\">\n",
        f(width_pt),
        f(height_pt),
        f(scene.width),
        f(scene.height)
    ));
    out.push_str(&format!(
        "<rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"{}\"/>\n",
        f(scene.width),
        f(scene.height),
        hex(scene.bg)
    ));
    for op in &scene.ops {
        if op.path.is_empty() {
            continue;
        }
        let d = path_d(&op.path);
        if d.trim().is_empty() {
            continue;
        }
        let mut attrs = String::new();
        if let Some(fill) = op.fill {
            attrs.push_str(&format!(" fill=\"{}\"", hex(fill)));
            let o = opacity(fill);
            if !o.is_empty() {
                attrs.push_str(&format!(" fill-opacity=\"{o}\""));
            }
        } else {
            attrs.push_str(" fill=\"none\"");
        }
        if let Some(st) = &op.stroke {
            attrs.push_str(&stroke_attrs(st));
        }
        out.push_str(&format!("<path d=\"{d}\"{attrs}/>\n"));
    }
    out.push_str("</svg>\n");
    out
}
