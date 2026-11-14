//! 自绘矢量图标（不依赖字体是否含符号）。

use egui::{Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, Vec2};

use crate::core::graph::{ChartKind, Marker};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Undo,
    Redo,
    Theme,
    Command,
    Plus,
    Download,
    Trash,
    Export,
    Settings,
    Scatter,
    Line,
    SmoothLine,
    StepLine,
    AreaLine,
    Regression,
    Column,
    GroupedBar,
    StackedBar,
    StackedBar100,
    HorizontalBar,
    Lollipop,
    DotPlot,
    BeforeAfter,
    Box,
    Violin,
    Ridgeline,
    Histogram,
    Ecdf,
    Qq,
    Survival,
    Heatmap,
    Pie,
    Donut,
}

pub fn chart_icon(kind: ChartKind) -> Icon {
    match kind {
        ChartKind::Roc => Icon::Scatter,
        ChartKind::Forest => Icon::Lollipop,
        ChartKind::Scatter => Icon::Scatter,
        ChartKind::Line => Icon::Line,
        ChartKind::SmoothLine => Icon::SmoothLine,
        ChartKind::StepLine => Icon::StepLine,
        ChartKind::AreaLine => Icon::AreaLine,
        ChartKind::Regression => Icon::Regression,
        ChartKind::Column => Icon::Column,
        ChartKind::GroupedBar => Icon::GroupedBar,
        ChartKind::StackedBar => Icon::StackedBar,
        ChartKind::StackedBar100 => Icon::StackedBar100,
        ChartKind::HorizontalBar => Icon::HorizontalBar,
        ChartKind::Lollipop => Icon::Lollipop,
        ChartKind::DotPlot => Icon::DotPlot,
        ChartKind::BeforeAfter => Icon::BeforeAfter,
        ChartKind::Box => Icon::Box,
        ChartKind::Violin => Icon::Violin,
        ChartKind::Ridgeline => Icon::Ridgeline,
        ChartKind::Histogram => Icon::Histogram,
        ChartKind::Ecdf => Icon::Ecdf,
        ChartKind::Qq => Icon::Qq,
        ChartKind::Survival => Icon::Survival,
        ChartKind::Heatmap => Icon::Heatmap,
        ChartKind::Pie => Icon::Pie,
        ChartKind::Donut => Icon::Donut,
    }
}

fn map(rect: Rect, x: f32, y: f32) -> Pos2 {
    Pos2::new(
        rect.left() + rect.width() * x,
        rect.top() + rect.height() * y,
    )
}

fn line(p: &Painter, rect: Rect, pts: &[(f32, f32)], color: Color32, w: f32) {
    let points: Vec<Pos2> = pts.iter().map(|(x, y)| map(rect, *x, *y)).collect();
    p.add(Shape::line(points, Stroke::new(w, color)));
}

pub fn draw(p: &Painter, rect: Rect, icon: Icon, color: Color32, bg: Color32) {
    let r = rect.shrink2(Vec2::new(rect.width() * 0.18, rect.height() * 0.18));
    let w = (rect.width() * 0.075).max(1.0);
    match icon {
        Icon::Undo | Icon::Redo => {
            let flip = icon == Icon::Redo;
            let fx = |x: f32| if flip { 1.0 - x } else { x };
            let pts: Vec<(f32, f32)> = (0..=14)
                .map(|i| {
                    let t = i as f32 / 14.0;
                    let ang = std::f32::consts::PI * (0.15 + 1.55 * t);
                    (fx(0.5 + 0.42 * ang.cos()), 0.55 - 0.38 * ang.sin())
                })
                .collect();
            line(p, r, &pts, color, w);
            let tip = pts.last().copied().unwrap();
            let a = map(r, tip.0, tip.1);
            let d = r.width() * 0.28;
            let dx = if flip { -d } else { d };
            p.add(Shape::line(
                vec![a, Pos2::new(a.x + dx, a.y - d * 0.15)],
                Stroke::new(w, color),
            ));
            p.add(Shape::line(
                vec![a, Pos2::new(a.x + dx * 0.35, a.y - d * 0.75)],
                Stroke::new(w, color),
            ));
        }
        Icon::Theme => {
            let c = map(r, 0.5, 0.5);
            p.circle_stroke(c, r.width() * 0.34, Stroke::new(w, color));
            p.add(Shape::convex_polygon(
                (0..=10)
                    .map(|i| {
                        let t = i as f32 / 10.0;
                        let ang = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * t;
                        Pos2::new(
                            c.x + r.width() * 0.34 * ang.cos(),
                            c.y + r.width() * 0.34 * ang.sin(),
                        )
                    })
                    .collect(),
                color,
                Stroke::NONE,
            ));
        }
        Icon::Command => {
            let rr = Rect::from_center_size(
                map(r, 0.5, 0.5),
                Vec2::new(r.width() * 0.9, r.height() * 0.72),
            );
            p.rect_stroke(
                rr,
                CornerRadius::same(2),
                Stroke::new(w, color),
                egui::StrokeKind::Inside,
            );
            for i in 0..3 {
                let x = rr.left() + rr.width() * (0.2 + 0.3 * i as f32);
                p.circle_filled(Pos2::new(x, rr.top() + rr.height() * 0.38), w * 0.9, color);
            }
            p.add(Shape::line(
                vec![
                    Pos2::new(
                        rr.left() + rr.width() * 0.18,
                        rr.bottom() - rr.height() * 0.25,
                    ),
                    Pos2::new(
                        rr.right() - rr.width() * 0.18,
                        rr.bottom() - rr.height() * 0.25,
                    ),
                ],
                Stroke::new(w, color),
            ));
        }
        Icon::Plus => {
            line(p, r, &[(0.5, 0.15), (0.5, 0.85)], color, w);
            line(p, r, &[(0.15, 0.5), (0.85, 0.5)], color, w);
        }
        Icon::Download => {
            line(p, r, &[(0.5, 0.1), (0.5, 0.6)], color, w);
            line(p, r, &[(0.28, 0.42), (0.5, 0.64), (0.72, 0.42)], color, w);
            line(p, r, &[(0.18, 0.82), (0.82, 0.82)], color, w);
        }
        Icon::Trash => {
            line(p, r, &[(0.25, 0.25), (0.75, 0.25)], color, w);
            line(p, r, &[(0.4, 0.15), (0.6, 0.15)], color, w);
            line(
                p,
                r,
                &[(0.32, 0.25), (0.36, 0.85), (0.64, 0.85), (0.68, 0.25)],
                color,
                w,
            );
        }
        Icon::Export => {
            line(p, r, &[(0.5, 0.85), (0.5, 0.3)], color, w);
            line(p, r, &[(0.28, 0.52), (0.5, 0.28), (0.72, 0.52)], color, w);
            line(p, r, &[(0.2, 0.1), (0.8, 0.1)], color, w);
        }
        Icon::Settings => {
            p.circle_stroke(map(r, 0.5, 0.5), r.width() * 0.18, Stroke::new(w, color));
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                let c = map(r, 0.5, 0.5);
                p.add(Shape::line(
                    vec![
                        Pos2::new(
                            c.x + 0.3 * r.width() * a.cos(),
                            c.y + 0.3 * r.width() * a.sin(),
                        ),
                        Pos2::new(
                            c.x + 0.42 * r.width() * a.cos(),
                            c.y + 0.42 * r.width() * a.sin(),
                        ),
                    ],
                    Stroke::new(w, color),
                ));
            }
        }
        Icon::Scatter => {
            for (x, y, rr) in [
                (0.28, 0.7, 0.075),
                (0.5, 0.35, 0.06),
                (0.72, 0.55, 0.08),
                (0.4, 0.55, 0.05),
            ] {
                p.circle_filled(map(r, x, y), r.width() * rr, color);
            }
        }
        Icon::Line => {
            line(
                p,
                r,
                &[(0.1, 0.75), (0.35, 0.45), (0.6, 0.55), (0.9, 0.2)],
                color,
                w,
            );
            for (x, y) in [(0.1, 0.75), (0.35, 0.45), (0.6, 0.55), (0.9, 0.2)] {
                p.circle_filled(map(r, x, y), r.width() * 0.05, color);
            }
        }
        Icon::Column => {
            for (x, h) in [(0.18, 0.35), (0.46, 0.6), (0.74, 0.45)] {
                let rr = Rect::from_min_max(map(r, x, 1.0 - h), map(r, x + 0.17, 0.95));
                p.rect_filled(rr, CornerRadius::same(1), color);
            }
        }
        Icon::GroupedBar => {
            for (x, h) in [(0.12, 0.35), (0.28, 0.55), (0.52, 0.45), (0.68, 0.7)] {
                let rr = Rect::from_min_max(map(r, x, 1.0 - h), map(r, x + 0.12, 0.95));
                p.rect_filled(rr, CornerRadius::same(1), color);
            }
        }
        Icon::Box => {
            let rr = Rect::from_min_max(map(r, 0.25, 0.35), map(r, 0.75, 0.65));
            p.rect_stroke(
                rr,
                CornerRadius::same(1),
                Stroke::new(w, color),
                egui::StrokeKind::Inside,
            );
            line(p, r, &[(0.5, 0.1), (0.5, 0.35)], color, w);
            line(p, r, &[(0.5, 0.65), (0.5, 0.9)], color, w);
            line(p, r, &[(0.35, 0.1), (0.65, 0.1)], color, w);
            line(p, r, &[(0.35, 0.9), (0.65, 0.9)], color, w);
        }
        Icon::Violin => {
            let c = map(r, 0.5, 0.5);
            let mut pts = Vec::new();
            for i in 0..=16 {
                let t = i as f32 / 16.0;
                let y = 0.08 + 0.84 * t;
                let wv = (0.42 * (t * std::f32::consts::PI).sin()).max(0.02);
                pts.push(Pos2::new(c.x + r.width() * wv, map(r, 0.5, y).y));
            }
            for i in (0..=16).rev() {
                let t = i as f32 / 16.0;
                let y = 0.08 + 0.84 * t;
                let wv = (0.42 * (t * std::f32::consts::PI).sin()).max(0.02);
                pts.push(Pos2::new(c.x - r.width() * wv, map(r, 0.5, y).y));
            }
            p.add(Shape::convex_polygon(
                pts,
                color.gamma_multiply(0.55),
                Stroke::new(w, color),
            ));
        }
        Icon::Histogram => {
            let hs = [0.25, 0.5, 0.8, 0.45, 0.65, 0.3];
            let bw = 0.12;
            for (i, h) in hs.iter().enumerate() {
                let x = 0.12 + i as f32 * (bw + 0.02);
                let rr = Rect::from_min_max(map(r, x, 1.0 - h), map(r, x + bw, 0.95));
                p.rect_filled(rr, 0.0, color);
            }
        }
        Icon::Ecdf => {
            line(
                p,
                r,
                &[
                    (0.1, 0.9),
                    (0.3, 0.9),
                    (0.3, 0.65),
                    (0.55, 0.65),
                    (0.55, 0.4),
                    (0.8, 0.4),
                    (0.8, 0.12),
                ],
                color,
                w,
            );
        }
        Icon::Qq => {
            line(
                p,
                r,
                &[(0.12, 0.9), (0.9, 0.12)],
                color.gamma_multiply(0.6),
                w,
            );
            for (x, y) in [(0.25, 0.78), (0.45, 0.58), (0.62, 0.35), (0.78, 0.22)] {
                p.circle_filled(map(r, x, y), r.width() * 0.06, color);
            }
        }
        Icon::SmoothLine => {
            let pts: Vec<(f32, f32)> = (0..=24)
                .map(|i| {
                    let t = i as f32 / 24.0;
                    let y = 0.72 - 0.5 * (t * std::f32::consts::PI).sin() - 0.08 * t;
                    (0.1 + 0.8 * t, y)
                })
                .collect();
            line(p, r, &pts, color, w);
        }
        Icon::StepLine => {
            line(
                p,
                r,
                &[
                    (0.1, 0.82),
                    (0.35, 0.82),
                    (0.35, 0.55),
                    (0.6, 0.55),
                    (0.6, 0.28),
                    (0.9, 0.28),
                ],
                color,
                w,
            );
        }
        Icon::AreaLine => {
            let mut poly: Vec<Pos2> = Vec::new();
            poly.push(map(r, 0.1, 0.95));
            for (x, y) in [
                (0.1, 0.7),
                (0.32, 0.45),
                (0.55, 0.55),
                (0.75, 0.25),
                (0.9, 0.32),
            ] {
                poly.push(map(r, x, y));
            }
            poly.push(map(r, 0.9, 0.95));
            p.add(Shape::convex_polygon(
                poly,
                color.gamma_multiply(0.28),
                Stroke::NONE,
            ));
            line(
                p,
                r,
                &[
                    (0.1, 0.7),
                    (0.32, 0.45),
                    (0.55, 0.55),
                    (0.75, 0.25),
                    (0.9, 0.32),
                ],
                color,
                w,
            );
        }
        Icon::Regression => {
            p.add(Shape::convex_polygon(
                vec![
                    map(r, 0.08, 0.72),
                    map(r, 0.92, 0.34),
                    map(r, 0.92, 0.18),
                    map(r, 0.08, 0.56),
                ],
                color.gamma_multiply(0.22),
                Stroke::NONE,
            ));
            line(p, r, &[(0.08, 0.64), (0.92, 0.26)], color, w);
            for (x, y) in [(0.22, 0.58), (0.42, 0.5), (0.62, 0.42), (0.8, 0.3)] {
                p.circle_filled(map(r, x, y), r.width() * 0.055, color);
            }
        }
        Icon::StackedBar => {
            let mut y = 0.95;
            for (h, a) in [(0.24, 0.35), (0.26, 0.6), (0.2, 0.95)] {
                let rr = Rect::from_min_max(map(r, 0.2, y - h), map(r, 0.46, y));
                p.rect_filled(rr, 0.0, color.gamma_multiply(a));
                y -= h;
            }
            let mut y = 0.95;
            for (h, a) in [(0.18, 0.35), (0.34, 0.6), (0.3, 0.95)] {
                let rr = Rect::from_min_max(map(r, 0.56, y - h), map(r, 0.82, y));
                p.rect_filled(rr, 0.0, color.gamma_multiply(a));
                y -= h;
            }
        }
        Icon::StackedBar100 => {
            let mut y = 0.95;
            for (h, a) in [(0.27, 0.35), (0.27, 0.6), (0.27, 0.95)] {
                let rr = Rect::from_min_max(map(r, 0.2, y - h), map(r, 0.46, y));
                p.rect_filled(rr, 0.0, color.gamma_multiply(a));
                y -= h;
            }
            let mut y = 0.95;
            for (h, a) in [(0.32, 0.35), (0.22, 0.6), (0.27, 0.95)] {
                let rr = Rect::from_min_max(map(r, 0.56, y - h), map(r, 0.82, y));
                p.rect_filled(rr, 0.0, color.gamma_multiply(a));
                y -= h;
            }
        }
        Icon::HorizontalBar => {
            for (y, len) in [(0.2, 0.62), (0.45, 0.82), (0.7, 0.46)] {
                let rr = Rect::from_min_max(map(r, 0.1, y), map(r, 0.1 + len, y + 0.18));
                p.rect_filled(rr, CornerRadius::same(1), color);
            }
        }
        Icon::Lollipop => {
            for (x, h) in [(0.25, 0.42), (0.5, 0.7), (0.75, 0.52)] {
                line(p, r, &[(x, 0.95), (x, 1.0 - h)], color, w * 0.8);
                p.circle_filled(map(r, x, 1.0 - h), r.width() * 0.085, color);
            }
        }
        Icon::DotPlot => {
            for (x, n) in [(0.26, 3), (0.5, 5), (0.74, 2)] {
                for k in 0..n {
                    let y = 0.95 - k as f32 * 0.17;
                    p.circle_filled(map(r, x, y), r.width() * 0.055, color);
                }
            }
        }
        Icon::BeforeAfter => {
            for (y0, y1) in [(0.75, 0.3), (0.5, 0.55), (0.85, 0.62)] {
                line(
                    p,
                    r,
                    &[(0.28, y0), (0.72, y1)],
                    color.gamma_multiply(0.75),
                    w,
                );
                p.circle_filled(map(r, 0.28, y0), r.width() * 0.06, color);
                p.circle_filled(map(r, 0.72, y1), r.width() * 0.06, color);
            }
        }
        Icon::Ridgeline => {
            for (base, amp) in [(0.85, 0.22), (0.6, 0.26), (0.35, 0.2)] {
                let mut poly: Vec<Pos2> = vec![map(r, 0.08, base)];
                for i in 0..=16 {
                    let t = i as f32 / 16.0;
                    let y = base - amp * (t * std::f32::consts::PI).sin();
                    poly.push(map(r, 0.08 + 0.84 * t, y));
                }
                poly.push(map(r, 0.92, base));
                p.add(Shape::convex_polygon(
                    poly,
                    color.gamma_multiply(0.25),
                    Stroke::new(w * 0.8, color),
                ));
            }
        }
        Icon::Pie => {
            let c = map(r, 0.5, 0.5);
            let rad = r.width() * 0.4;
            p.circle_stroke(c, rad, Stroke::new(w * 1.1, color));
            p.add(Shape::line(
                vec![c, Pos2::new(c.x + rad, c.y)],
                Stroke::new(w, color),
            ));
            p.add(Shape::line(
                vec![c, Pos2::new(c.x, c.y - rad)],
                Stroke::new(w, color),
            ));
            let mut wedge = vec![c];
            for i in 0..=8 {
                let a = -std::f32::consts::FRAC_PI_2 * (i as f32 / 8.0);
                wedge.push(Pos2::new(c.x + rad * a.cos(), c.y + rad * a.sin()));
            }
            p.add(Shape::convex_polygon(
                wedge,
                color.gamma_multiply(0.3),
                Stroke::NONE,
            ));
        }
        Icon::Donut => {
            let c = map(r, 0.5, 0.5);
            let rad = r.width() * 0.36;
            p.circle_stroke(
                c,
                rad,
                Stroke::new(r.width() * 0.17, color.gamma_multiply(0.85)),
            );
        }
        Icon::Survival => {
            line(
                p,
                r,
                &[
                    (0.1, 0.15),
                    (0.35, 0.15),
                    (0.35, 0.45),
                    (0.6, 0.45),
                    (0.6, 0.7),
                    (0.9, 0.7),
                ],
                color,
                w,
            );
        }
        Icon::Heatmap => {
            for i in 0..3 {
                for j in 0..3 {
                    let a = 0.25 + 0.3 * ((i * 3 + j) as f32 / 8.0);
                    let rr = Rect::from_min_max(
                        map(r, 0.12 + j as f32 * 0.28, 0.12 + i as f32 * 0.28),
                        map(
                            r,
                            0.12 + j as f32 * 0.28 + 0.24,
                            0.12 + i as f32 * 0.28 + 0.24,
                        ),
                    );
                    p.rect_filled(rr, CornerRadius::same(1), color.gamma_multiply(a));
                }
            }
        }
    }
    let _ = bg;
}

pub fn marker(p: &Painter, at: Pos2, r: f32, m: Marker, color: Color32) {
    match m {
        Marker::Circle => {
            p.circle_filled(at, r, color);
        }
        Marker::Square => {
            p.rect_filled(
                Rect::from_center_size(at, Vec2::splat(r * 1.7)),
                CornerRadius::same(1),
                color,
            );
        }
        Marker::Triangle => {
            p.add(Shape::convex_polygon(
                vec![
                    Pos2::new(at.x, at.y - r),
                    Pos2::new(at.x + r * 0.95, at.y + r * 0.75),
                    Pos2::new(at.x - r * 0.95, at.y + r * 0.75),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Marker::Diamond => {
            p.add(Shape::convex_polygon(
                vec![
                    Pos2::new(at.x, at.y - r * 1.15),
                    Pos2::new(at.x + r * 0.9, at.y),
                    Pos2::new(at.x, at.y + r * 1.15),
                    Pos2::new(at.x - r * 0.9, at.y),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Marker::Cross => {
            p.add(Shape::line(
                vec![Pos2::new(at.x - r, at.y - r), Pos2::new(at.x + r, at.y + r)],
                Stroke::new(r * 0.5, color),
            ));
            p.add(Shape::line(
                vec![Pos2::new(at.x - r, at.y + r), Pos2::new(at.x + r, at.y - r)],
                Stroke::new(r * 0.5, color),
            ));
        }
    }
}
