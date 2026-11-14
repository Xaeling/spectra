//! 场景级诊断：渲染后遍历图元，用数值定位绘制问题。
//!
//! 用法：`cargo run --release --example diag`
//!
//! 不看图，直接问场景「你画了什么」：
//! - 每个图型画了多少图元、用了哪些颜色；
//! - 绘图区里的纯白填充块（过大会读成「挖空」）；
//! - 标记点是实心还是空心、落在哪里；
//! - 回归置信带覆盖多宽、是填充还是描边。

use stataura::core::graph::{ChartKind, ErrorKind, GraphSpec};
use stataura::core::model::{DataTable, Project};
use stataura::core::sample::sample_project;
use stataura::plot::scene::{Op, Path, Pt, Seg};
use stataura::plot::{render, FontBook, Render};

fn pick<'a>(p: &'a Project, kind: ChartKind) -> &'a DataTable {
    let by = |name: &str| p.tables.iter().find(|t| t.name == name);
    match kind {
        ChartKind::Scatter
        | ChartKind::Line
        | ChartKind::SmoothLine
        | ChartKind::StepLine
        | ChartKind::AreaLine
        | ChartKind::Regression => by("剂量–反应"),
        ChartKind::GroupedBar | ChartKind::StackedBar | ChartKind::StackedBar100 => {
            by("性别 × 治疗")
        }
        ChartKind::Survival => by("生存分析"),
        ChartKind::Heatmap => by("性别 × 治疗"),
        ChartKind::Pie | ChartKind::Donut => by("人群构成"),
        _ => by("体重变化"),
    }
    .or_else(|| p.tables.first())
    .expect("sample")
}

fn bbox(path: &Path) -> (f32, f32, f32, f32) {
    let (mut x0, mut y0, mut x1, mut y1) = (
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    );
    for s in &path.segs {
        let p: Pt = match s {
            Seg::Move(p) | Seg::Line(p) => *p,
            Seg::Quad(c, p) => Pt::new((c.x + p.x) / 2.0, (c.y + p.y) / 2.0),
            Seg::Cubic(c1, c2, p) => Pt::new((c1.x + c2.x + p.x) / 3.0, (c1.y + c2.y + p.y) / 3.0),
            Seg::Close => continue,
        };
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    (x0, y0, x1, y1)
}

fn hex(c: stataura::core::color::Rgba) -> String {
    format!("#{:02X}{:02X}{:02X}@{:03}", c.r, c.g, c.b, c.a)
}

fn inside(r: &Render, b: (f32, f32, f32, f32), tol: f32) -> bool {
    b.0 >= r.plot.x - tol
        && b.2 <= r.plot.right() + tol
        && b.1 >= r.plot.y - tol
        && b.3 <= r.plot.bottom() + tol
}

/// 小方块笔画 = 标记点或空心圈
fn small(op: &Op) -> Option<(f32, f32, f32, f32)> {
    let b = bbox(&op.path);
    let (w, h) = (b.2 - b.0, b.3 - b.1);
    if w > 0.5 && h > 0.5 && w <= 14.0 && h <= 14.0 && (w - h).abs() < 3.0 {
        Some(b)
    } else {
        None
    }
}

fn main() {
    let p = sample_project();
    let fonts = FontBook::load_builtin();

    for kind in [
        ChartKind::Regression,
        ChartKind::Box,
        ChartKind::Violin,
        ChartKind::Line,
    ] {
        let t = pick(&p, kind);
        let mut spec = GraphSpec::default_for(t);
        spec.kind = kind;
        spec.error = if kind.shows_error() {
            ErrorKind::Sem
        } else {
            ErrorKind::None
        };
        spec.show_legend = true;
        let r: Render = render(&spec, t, 468.0, 318.0, 2.4, &fonts);

        println!("\n===== {} （表：{}）", kind.label(), t.name);
        println!(
            "图元 {}  绘图区 x {:.0}..{:.0} y {:.0}..{:.0}",
            r.scene.ops.len(),
            r.plot.x,
            r.plot.right(),
            r.plot.y,
            r.plot.bottom()
        );

        // 1) 纯白填充块（过大会读成「挖空」）
        let whites: Vec<_> = r
            .scene
            .ops
            .iter()
            .filter_map(|op| {
                let f = op.fill?;
                // 「洞」的判据：纯白填充 **且没有描边**。有描边的白块是箱体，
                // 是正常的图元（小提琴内嵌的 IQR 箱就是这样）。
                if f.r > 250
                    && f.g > 250
                    && f.b > 250
                    && op.stroke.is_none()
                    && inside(&r, bbox(&op.path), 1.0)
                {
                    Some(bbox(&op.path))
                } else {
                    None
                }
            })
            .collect();
        println!("  无描边白块（会读成「挖空」）{} 个", whites.len());
        for b in whites.iter().take(4) {
            println!(
                "     · {:.0}×{:.0}px @({:.0},{:.0})",
                b.2 - b.0,
                b.3 - b.1,
                b.0,
                b.1
            );
        }

        // 2) 标记：实心（数据点）/ 空心（离群点）
        let mut solid = 0;
        let mut hollow = 0;
        let mut solid_pts = Vec::new();
        for op in &r.scene.ops {
            let Some(b) = small(op) else { continue };
            if !inside(&r, b, 1.0) {
                continue; // 图例里的示例标记
            }
            if op.fill.is_some() {
                solid += 1;
                solid_pts.push(((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0));
            } else if op.stroke.is_some() {
                hollow += 1;
            }
        }
        println!("  实心标记 {} 个，空心圈 {} 个", solid, hollow);
        if !solid_pts.is_empty() {
            let s: Vec<String> = solid_pts
                .iter()
                .take(8)
                .map(|(x, y)| format!("({x:.0},{y:.0})"))
                .collect();
            println!("     实心点位置：{}", s.join(" "));
        }

        // 3) 大范围半透明填充（叠色风险）
        let bands: Vec<_> = r
            .scene
            .ops
            .iter()
            .filter_map(|op| {
                let f = op.fill?;
                let b = bbox(&op.path);
                if f.a < 200 && (b.2 - b.0) > r.plot.w * 0.4 {
                    Some((hex(f), b.2 - b.0))
                } else {
                    None
                }
            })
            .collect();
        println!("  宽幅半透明填充 {} 个", bands.len());
        for (c, w) in &bands {
            println!("     · {c} 宽 {w:.0}px（绘图区宽 {:.0}px）", r.plot.w);
        }

        // 4) 小提琴/箱线的结构元素：细横线（中位线/须帽）+ 细竖线（须）
        if matches!(kind, ChartKind::Violin | ChartKind::Box) {
            let mut hlines = 0;
            let mut vlines = 0;
            for op in &r.scene.ops {
                if op.fill.is_some() || op.stroke.is_none() {
                    continue;
                }
                let b = bbox(&op.path);
                let (w, h) = (b.2 - b.0, b.3 - b.1);
                if !inside(&r, b, 1.0) {
                    continue;
                }
                if w > 4.0 && h < 4.0 {
                    hlines += 1;
                } else if h > 20.0 && w < 4.0 {
                    vlines += 1;
                }
            }
            println!(
                "  细横线（中位/须帽）{} 条，细竖线（须）{} 条",
                hlines, vlines
            );
        }
    }
}
