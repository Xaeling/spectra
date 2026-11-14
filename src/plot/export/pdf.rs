//! PDF 导出：自写最小 PDF 1.4 写入器。
//!
//! 只用到 m/l/c/h、rg/RG、w/J/j/d、f/S/B 与 ExtGState 透明度；
//! 全部文字已经转成路径，所以文件在任何机器上版式一致。

use std::collections::BTreeMap;

use crate::core::color::Rgba;
use crate::plot::scene::{LineCap, LineJoin, Scene, Seg};

fn n(v: f32) -> String {
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

fn rgb(c: Rgba) -> String {
    format!(
        "{} {} {}",
        n(c.r as f32 / 255.0),
        n(c.g as f32 / 255.0),
        n(c.b as f32 / 255.0)
    )
}

fn path_ops(scene: &Scene) -> String {
    let mut out = String::new();
    for op in &scene.ops {
        if op.path.is_empty() {
            continue;
        }
        let mut cur = (0.0_f32, 0.0_f32);
        let mut start = (0.0_f32, 0.0_f32);
        for seg in &op.path.segs {
            match seg {
                Seg::Move(p) => {
                    out.push_str(&format!("{} {} m\n", n(p.x), n(p.y)));
                    cur = (p.x, p.y);
                    start = cur;
                }
                Seg::Line(p) => {
                    out.push_str(&format!("{} {} l\n", n(p.x), n(p.y)));
                    cur = (p.x, p.y);
                }
                Seg::Quad(c, p) => {
                    let c1 = (
                        cur.0 + 2.0 / 3.0 * (c.x - cur.0),
                        cur.1 + 2.0 / 3.0 * (c.y - cur.1),
                    );
                    let c2 = (p.x + 2.0 / 3.0 * (c.x - p.x), p.y + 2.0 / 3.0 * (c.y - p.y));
                    out.push_str(&format!(
                        "{} {} {} {} {} {} c\n",
                        n(c1.0),
                        n(c1.1),
                        n(c2.0),
                        n(c2.1),
                        n(p.x),
                        n(p.y)
                    ));
                    cur = (p.x, p.y);
                }
                Seg::Cubic(a, b, p) => {
                    out.push_str(&format!(
                        "{} {} {} {} {} {} c\n",
                        n(a.x),
                        n(a.y),
                        n(b.x),
                        n(b.y),
                        n(p.x),
                        n(p.y)
                    ));
                    cur = (p.x, p.y);
                }
                Seg::Close => {
                    out.push_str("h\n");
                    cur = start;
                }
            }
        }

        let has_fill = op.fill.is_some();
        let has_stroke = op.stroke.is_some();
        if !has_fill && !has_stroke {
            continue;
        }
        if let Some(fill) = op.fill {
            out.push_str(&format!("{} rg\n", rgb(fill)));
        }
        if let Some(st) = &op.stroke {
            out.push_str(&format!("{} RG\n", rgb(st.color)));
            out.push_str(&format!("{} w\n", n(st.width.max(0.01))));
            let cap = match st.cap {
                LineCap::Butt => 0,
                LineCap::Round => 1,
                LineCap::Square => 2,
            };
            let join = match st.join {
                LineJoin::Miter => 0,
                LineJoin::Round => 1,
                LineJoin::Bevel => 2,
            };
            out.push_str(&format!("{cap} J\n{join} j\n"));
            if st.dash.is_empty() {
                out.push_str("[] 0 d\n");
            } else {
                let mut dashes = st.dash.clone();
                if dashes.len() % 2 == 1 {
                    let last = *dashes.last().unwrap();
                    dashes.push(last);
                }
                let list: Vec<String> = dashes.iter().map(|v| n(*v)).collect();
                out.push_str(&format!("[{}] 0 d\n", list.join(" ")));
            }
        }
        out.push_str(match (has_fill, has_stroke) {
            (true, true) => "B\n",
            (true, false) => "f\n",
            (false, true) => "S\n",
            _ => unreachable!(),
        });
    }
    out
}

/// 关键：PDF 坐标原点在左下，用 CTM 做 y 翻转，使场景 px 坐标可直接使用。
fn content(
    scene: &Scene,
    width_pt: f32,
    height_pt: f32,
    gs_names: &BTreeMap<(u8, u8), String>,
) -> String {
    let k = if scene.width > 0.0 {
        width_pt / scene.width
    } else {
        1.0
    };
    let mut out = String::new();
    out.push_str("q\n");
    out.push_str(&format!("{} 0 0 -{} 0 {} cm\n", n(k), n(k), n(height_pt)));

    // 每个 op 单独设置 ExtGState，避免状态泄漏
    for op in &scene.ops {
        if op.path.is_empty() {
            continue;
        }
        let key = (
            op.fill.map(|c| c.a).unwrap_or(255),
            op.stroke.as_ref().map(|s| s.color.a).unwrap_or(255),
        );
        if let Some(name) = gs_names.get(&key) {
            out.push_str(&format!("/{name} gs\n"));
        }
        out.push_str(&path_ops_single(op));
    }
    out.push_str("Q\n");
    out
}

fn path_ops_single(op: &crate::plot::scene::Op) -> String {
    let scene = Scene {
        width: 0.0,
        height: 0.0,
        bg: Rgba::WHITE,
        ops: vec![op.clone()],
    };
    path_ops(&scene)
}

pub fn to_pdf(scene: &Scene, width_pt: f32, height_pt: f32) -> Vec<u8> {
    // 收集透明度组合
    let mut gs: BTreeMap<(u8, u8), String> = BTreeMap::new();
    gs.insert((255, 255), "GS0".into());
    let mut gs_list: Vec<(u8, u8)> = vec![(255, 255)];
    for op in &scene.ops {
        let key = (
            op.fill.map(|c| c.a).unwrap_or(255),
            op.stroke.as_ref().map(|s| s.color.a).unwrap_or(255),
        );
        if !gs.contains_key(&key) {
            let name = format!("GS{}", gs_list.len());
            gs.insert(key, name);
            gs_list.push(key);
        }
    }

    let stream = content(scene, width_pt, height_pt, &gs);
    let gs_objs: Vec<String> = gs_list
        .iter()
        .map(|(ca, ca_stroke)| {
            format!(
                "<< /Type /ExtGState /ca {} /CA {} >>",
                n(*ca as f32 / 255.0),
                n(*ca_stroke as f32 / 255.0)
            )
        })
        .collect();

    let gs_base = 5usize; // 1 catalog, 2 pages, 3 page, 4 contents
    let mut resources = String::from("/ProcSet [/PDF]");
    if !gs_objs.is_empty() {
        resources.push_str(" /ExtGState << ");
        for (i, key) in gs_list.iter().enumerate() {
            if let Some(name) = gs.get(key) {
                resources.push_str(&format!("/{name} {} 0 R ", gs_base + i));
            }
        }
        resources.push_str(">>");
    }

    let mut objs: Vec<String> = Vec::new();
    objs.push("<< /Type /Catalog /Pages 2 0 R >>".into());
    objs.push("<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into());
    objs.push(format!(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << {} >> /Contents 4 0 R >>",
        n(width_pt),
        n(height_pt),
        resources
    ));
    objs.push(format!(
        "<< /Length {} >>\nstream\n{}endstream",
        stream.len(),
        stream
    ));
    for obj in &gs_objs {
        objs.push(obj.clone());
    }

    let mut pdf: Vec<u8> = Vec::with_capacity(stream.len() + 4096);
    pdf.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets = vec![0usize; objs.len() + 1];
    for (i, obj) in objs.iter().enumerate() {
        offsets[i + 1] = pdf.len();
        pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", i + 1, obj).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for off in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Producer (StatAura) >>\nstartxref\n{xref}\n%%EOF\n",
            objs.len() + 1
        )
        .as_bytes(),
    );
    pdf
}
