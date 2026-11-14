//! 扩展图型：堆叠柱 / 横向柱 / 前后对比 / 山脊图 / 饼与环。
//!
//! 每个渲染器都遵守同一条约定：**数据不满足前提时给出说明，不画
//! 「看起来像图」的东西**；所有几何在进入场景前裁剪到坐标框内。
//! 坐标映射与 plotters 的做法一致 —— 先确定定义域，再做值→像素映射，
//! 而不是先画再硬塞进画布。

use std::f32::consts::PI;

use crate::core::color::Rgba;
use crate::core::graph::ChartKind;
use crate::core::model::{format_number, ColRole, DataTable, TableKind};
use crate::core::stats;
use crate::plot::axis::Axis;
use crate::plot::chart::{
    build_groups, draw_axes, draw_grid, draw_legend, draw_marker, frame_layout, xpx, ypx, Ctx,
    Frame, GroupData, LegendItem, LegendKind,
};
use crate::plot::clip;
use crate::plot::scene::{LineJoin, Path, Pt, Rect, Stroke};
use crate::plot::text::{HAlign, VAlign};

fn fmt(v: f64) -> String {
    format_number(v)
}

fn fallback(ctx: &mut Ctx, msg: &str) -> (Axis, Axis) {
    let x = Axis::linear(0.0, 1.0);
    let y = Axis::linear(0.0, 1.0);
    ctx.plot = Rect::new(
        ctx.s * 20.0,
        ctx.s * 20.0,
        (ctx.w - ctx.s * 40.0).max(1.0),
        (ctx.h - ctx.s * 40.0).max(1.0),
    );
    let lines: Vec<&str> = msg.split('\n').collect();
    let lh = ctx.px(17.0);
    let y0 = ctx.h / 2.0 - lh * (lines.len() as f32 - 1.0) / 2.0;
    for (i, line) in lines.iter().enumerate() {
        ctx.text(
            line,
            Pt::new(ctx.w / 2.0, y0 + i as f32 * lh),
            if i == 0 { 11.0 } else { 9.5 },
            if i == 0 {
                ctx.theme.ink
            } else {
                ctx.theme.muted
            },
            HAlign::Center,
            VAlign::Middle,
            0.0,
            false,
        );
    }
    (x, y)
}

/// 分组表 / 柱状表 → 「类别 × 系列」的均值矩阵。
///
/// - 有行标题（分组表、列联表）：每行一个类别；
/// - 没有行标题（柱状表）：整张表只有一个类别，各列是并列的系列。
pub(crate) struct Matrix {
    pub cats: Vec<String>,
    pub series: Vec<String>,
    /// `values[series][cat]`
    pub values: Vec<Vec<Option<f64>>>,
    pub counts: Vec<Vec<usize>>,
}

pub(crate) fn category_matrix(table: &DataTable) -> Option<Matrix> {
    let cols: Vec<&crate::core::model::Column> = table
        .columns
        .iter()
        .filter(|c| c.role == ColRole::Y)
        .collect();
    if cols.is_empty() || table.nrows() == 0 {
        return None;
    }
    let has_headers = table.row_headers.iter().any(|h| !h.is_empty());
    let mut cats: Vec<String> = Vec::new();
    let mut rows_of: Vec<Vec<usize>> = Vec::new();
    if has_headers {
        for r in 0..table.nrows() {
            let name = table
                .row_headers
                .get(r)
                .filter(|s| !s.is_empty())
                .cloned()
                .unwrap_or_else(|| format!("{}", r + 1));
            match cats.iter().position(|c| c == &name) {
                Some(i) => rows_of[i].push(r),
                None => {
                    cats.push(name);
                    rows_of.push(vec![r]);
                }
            }
        }
    } else {
        cats.push("合计".into());
        rows_of.push((0..table.nrows()).collect());
    }
    if cats.is_empty() {
        return None;
    }
    let mut values = Vec::with_capacity(cols.len());
    let mut counts = Vec::with_capacity(cols.len());
    for c in &cols {
        let mut mv = Vec::with_capacity(cats.len());
        let mut mc = Vec::with_capacity(cats.len());
        for rows in &rows_of {
            let v: Vec<f64> = rows
                .iter()
                .filter_map(|&r| c.cells.get(r).and_then(|x| x.value()))
                .collect();
            mc.push(v.len());
            mv.push(if v.is_empty() {
                None
            } else {
                Some(stats::mean(&v))
            });
        }
        values.push(mv);
        counts.push(mc);
    }
    Some(Matrix {
        series: cols.iter().map(|c| c.name.clone()).collect(),
        cats,
        values,
        counts,
    })
}

// ---------------------------------------------------------------------------
// 堆叠柱
// ---------------------------------------------------------------------------

pub(crate) fn render_stacked(ctx: &mut Ctx, table: &DataTable, kind: ChartKind) -> (Axis, Axis) {
    let Some(m) = category_matrix(table) else {
        return fallback(ctx, kind.requires());
    };
    let k = m.series.len();
    let n = m.cats.len();
    let norm = kind == ChartKind::StackedBar100;

    // 每个类别的堆叠总量
    let totals: Vec<f64> = (0..n)
        .map(|c| {
            (0..k)
                .filter_map(|s| m.values[s][c])
                .filter(|v| *v > 0.0)
                .sum::<f64>()
        })
        .collect();
    let ymax = if norm {
        100.0
    } else {
        totals.iter().copied().fold(0.0, f64::max)
    };
    // 负值必须向零线以下堆叠，轴下界取「各类别负值累计的最小和」。
    // 以前直接 `if v <= 0 { continue }` 把负段丢掉，是静默的错误绘图。
    let mut neg_totals: Vec<f64> = vec![0.0; n];
    for c in 0..n {
        for s in 0..k {
            if let Some(v) = m.values[s][c] {
                if v < 0.0 {
                    neg_totals[c] += v;
                }
            }
        }
    }
    let ymin = neg_totals.iter().copied().fold(0.0, f64::min);
    // 百分比堆叠对负值没有定义：不是画错，而是直接说明。
    if norm && neg_totals.iter().any(|v| *v < 0.0) {
        return fallback(ctx, "百分比堆叠不能包含负值，请改用普通堆叠或分组柱");
    }
    let has_negative = neg_totals.iter().any(|v| *v < 0.0);
    if totals.iter().all(|t| *t <= 0.0) && !has_negative {
        return fallback(ctx, "堆叠图需要至少一段非零值");
    }
    let x = Axis::linear(-0.5, n as f64 - 0.5);
    // 百分比堆叠固定 0–100，别再乘 1.08 把刻度搞成 0/27/54/81/108。
    let mut y = if norm {
        Axis::linear(0.0, 100.0)
    } else {
        Axis::linear(ymin.min(0.0), ymax * 1.08)
    };
    if let Some([lo, hi]) = ctx.spec.y_range {
        if hi > lo {
            y.min = lo;
            y.max = hi;
        }
    }
    let yticks = y.ticks(6);

    ctx.legend = if ctx.spec.show_legend {
        (0..k)
            .map(|s| LegendItem {
                label: m.series[s].clone(),
                color: ctx.color(s),
                kind: LegendKind::Square,
            })
            .collect()
    } else {
        Vec::new()
    };
    let y_title = if norm {
        "占比 (%)".to_string()
    } else {
        ctx.spec.y_title.clone()
    };
    let f = frame_layout(
        ctx,
        x,
        y,
        Vec::new(),
        yticks,
        Some(m.cats.clone()),
        "",
        &y_title,
    );
    draw_grid(ctx, &f);
    ctx.begin_data(&f);

    let slot = f.plot.w / n.max(1) as f32;
    let group_w = ctx.spec.bar_width.clamp(0.25, 0.95);
    let half = (slot * group_w as f32 * 0.5).max(1.0);
    let bar_alpha = ctx.theme.bar_alpha;

    for c in 0..n {
        let cx = xpx(&f, c as f64);
        let mut pos_base = 0.0_f64;
        let mut neg_base = 0.0_f64;
        for s in 0..k {
            let Some(v) = m.values[s][c] else { continue };
            if v == 0.0 {
                continue;
            }
            let seg = if norm {
                v / totals[c].max(1e-12) * 100.0
            } else {
                v
            };
            let (y0, y1) = if seg > 0.0 {
                let a = ypx(&f, pos_base);
                let b = ypx(&f, pos_base + seg);
                pos_base += seg;
                (a, b)
            } else {
                let a = ypx(&f, neg_base);
                let b = ypx(&f, neg_base + seg);
                neg_base += seg;
                (a, b)
            };
            let rect = Rect::new(cx - half, y0.min(y1), half * 2.0, (y0 - y1).abs());
            ctx.check_rect("堆叠段", rect, ctx.s * 0.3);
            let color = ctx.color(s);
            let stroke = Stroke::new((ctx.s * 0.55).max(0.25), color.darken(0.22));
            ctx.scene
                .rect(rect, Some(color.with_alpha(bar_alpha)), Some(stroke));
            if (y0 - y1).abs() > ctx.px(9.0) {
                ctx.text(
                    &format!("{:.0}", seg),
                    Pt::new(cx, (y0 + y1) / 2.0),
                    ctx.theme.font_tick - 1.0,
                    crate::plot::chart::ink_on(color),
                    HAlign::Center,
                    VAlign::Middle,
                    0.0,
                    false,
                );
            }
            ctx.hits.push(crate::plot::chart::Hit {
                at: Pt::new(cx, (y0 + y1) / 2.0),
                title: format!("{} · {}", m.series[s], m.cats[c]),
                lines: vec![
                    format!("{} = {}", if norm { "占比" } else { "均值" }, fmt(seg)),
                    format!("n = {}", m.counts[s][c]),
                ],
            });
        }
    }

    draw_axes(ctx, &f, "", &y_title);
    draw_legend(ctx, &f);
    ctx.plot = f.plot;
    (x, f.y)
}

// ---------------------------------------------------------------------------
// 横向柱
// ---------------------------------------------------------------------------

/// 分类轴在左、数值轴在下的框架。
///
/// `frame_layout` 只处理「X 分类」，横向图必须另算边距：
/// 左边距由最长的组名决定，否则组名会被裁掉。
pub(crate) fn frame_horizontal(
    ctx: &Ctx,
    labels: &[String],
    x: Axis,
    xticks: Vec<crate::plot::axis::Tick>,
    x_title: &str,
) -> Frame {
    let s = ctx.s;
    let tick_fs = ctx.theme.font_tick;
    let label_w = labels
        .iter()
        .map(|l| ctx.measure(l, tick_fs))
        .fold(0.0, f32::max);
    let title_h = if ctx.spec.show_title && !ctx.spec.title.is_empty() {
        s * ctx.theme.font_title * 1.35 + s * 7.0
    } else {
        0.0
    };
    let x_title_h = if x_title.is_empty() {
        0.0
    } else {
        s * ctx.theme.font_axis_title * 1.45 + s * 5.0
    };
    let left = s * 12.0 + label_w + s * 8.0;
    let right = s * 14.0;
    let top = s * 10.0 + title_h;
    let bottom = s * 9.0
        + x_title_h
        + s * ctx.theme.tick_len
        + s * 3.0
        + ctx.fonts.line_height(s * tick_fs, ctx.theme.family)
        + s * 4.0;
    let plot = Rect::new(
        left,
        top,
        (ctx.w - left - right).max(s * 30.0),
        (ctx.h - top - bottom).max(s * 30.0),
    );
    Frame {
        plot,
        x,
        y: Axis::linear(0.0, 1.0),
        xticks,
        yticks: Vec::new(),
        cat: Some(labels.to_vec()),
        cat_rotate: false,
    }
}

pub(crate) fn render_horizontal(ctx: &mut Ctx, table: &DataTable) -> (Axis, Axis) {
    let groups = build_groups(ctx, table);
    if groups.is_empty() {
        return fallback(ctx, ChartKind::HorizontalBar.requires());
    }
    let n = groups.len();
    let error = ctx.spec.error;

    // 轴域必须覆盖**全部观测值**，不能只按「均值 ± SEM」定：
    // SEM 只描述均值的不确定度，单个观测完全可以落在它之外。
    // 之前只用 mean±SEM，散点就被画到了坐标框左边的标签区（读起来像负数区）。
    let mut xv = Vec::new();
    for g in &groups {
        xv.push(g.desc.mean);
        xv.extend(g.values.iter().copied());
        if let Some(e) = err_for(error, g) {
            xv.push(g.desc.mean + e);
            xv.push(g.desc.mean - e);
        }
    }
    xv.push(0.0);
    let x = Axis::from_values(xv, ctx.spec.x_log, true);
    let xticks = x.ticks(6);
    let x_title = if ctx.spec.x_title.is_empty() {
        ctx.spec.y_title.clone()
    } else {
        ctx.spec.x_title.clone()
    };
    let labels: Vec<String> = groups.iter().map(|g| g.name.clone()).collect();
    let f = frame_horizontal(ctx, &labels, x, xticks.clone(), &x_title);
    ctx.begin_data(&f);

    // 网格（竖线）
    if ctx.spec.show_grid {
        for t in &f.xticks {
            if !t.major || t.label.is_empty() {
                continue;
            }
            let px = f.plot.x + t.norm as f32 * f.plot.w;
            ctx.line(
                Pt::new(px, f.plot.y),
                Pt::new(px, f.plot.bottom()),
                0.55,
                ctx.theme.grid,
                if ctx.theme.grid_dash {
                    &[3.0, 3.0]
                } else {
                    &[]
                },
            );
        }
    }
    // 轴：左 + 下
    ctx.spine(
        Pt::new(f.plot.x, f.plot.y),
        Pt::new(f.plot.x, f.plot.bottom()),
        ctx.theme.axis_width,
        ctx.theme.axis,
    );
    ctx.spine(
        Pt::new(f.plot.x, f.plot.bottom()),
        Pt::new(f.plot.right(), f.plot.bottom()),
        ctx.theme.axis_width,
        ctx.theme.axis,
    );
    // 刻度与标签
    let tick_len = ctx.s * ctx.theme.tick_len;
    for t in &f.xticks {
        if !t.major {
            continue;
        }
        let px = f.plot.x + t.norm as f32 * f.plot.w;
        ctx.line(
            Pt::new(px, f.plot.bottom()),
            Pt::new(px, f.plot.bottom() + tick_len),
            ctx.theme.axis_width * 0.9,
            ctx.theme.axis,
            &[],
        );
        if !t.label.is_empty() {
            ctx.text(
                &t.label,
                Pt::new(px, f.plot.bottom() + tick_len + ctx.px(3.0)),
                ctx.theme.font_tick,
                ctx.theme.ink,
                HAlign::Center,
                VAlign::Top,
                0.0,
                false,
            );
        }
        let _ = &xticks;
    }
    if !x_title.is_empty() {
        ctx.text(
            &x_title,
            Pt::new(f.plot.center().x, ctx.h - ctx.px(3.0)),
            ctx.theme.font_axis_title,
            ctx.theme.ink,
            HAlign::Center,
            VAlign::Bottom,
            0.0,
            false,
        );
    }
    if ctx.spec.show_title && !ctx.spec.title.is_empty() {
        let title = ctx.spec.title.clone();
        ctx.text(
            &title,
            Pt::new(ctx.w / 2.0, ctx.px(6.0)),
            ctx.theme.font_title,
            ctx.theme.ink,
            HAlign::Center,
            VAlign::Top,
            0.0,
            true,
        );
    }

    // 行高：每组一个槽，柱子占 62%
    let slot = f.plot.h / n.max(1) as f32;
    let bar_h = (slot * ctx.spec.bar_width.clamp(0.25, 0.95) as f32).max(2.0);
    // 对数轴上 0 无定义，基线退到轴的下界（否则 norm(0) 返回 NaN，柱子整体消失）。
    let base_val = if x.log {
        x.min
    } else {
        0.0_f64.clamp(x.min, x.max)
    };
    let zero = f.plot.x + x.norm(base_val) as f32 * f.plot.w;
    let bar_alpha = ctx.theme.bar_alpha;

    for (i, g) in groups.iter().enumerate() {
        let cy = f.plot.y + slot * (i as f32 + 0.5);
        // 组名
        ctx.text(
            &g.name,
            Pt::new(f.plot.x - ctx.px(6.0), cy),
            ctx.theme.font_tick,
            ctx.theme.ink,
            HAlign::Right,
            VAlign::Middle,
            0.0,
            false,
        );
        let xm = f.plot.x + x.norm(g.desc.mean) as f32 * f.plot.w;
        let color = g.color;
        let rect = Rect::new(
            zero.min(xm),
            cy - bar_h / 2.0,
            (xm - zero).abs().max(0.6),
            bar_h,
        );
        ctx.check_rect("横向柱体", rect, ctx.s * 0.3);
        let mut stroke = Stroke::new((ctx.s * 0.6).max(0.25), color.darken(0.2));
        stroke.join = LineJoin::Miter;
        ctx.scene
            .rect(rect, Some(color.with_alpha(bar_alpha)), Some(stroke));
        if let Some(e) = err_for(error, g) {
            let ex = (x.norm(g.desc.mean + e) - x.norm(g.desc.mean)) as f32 * f.plot.w;
            let cap = ctx.px(3.2);
            let c = color.darken(0.25);
            ctx.line(
                Pt::new(xm - ex, cy),
                Pt::new(xm + ex, cy),
                (ctx.s * 0.9).max(0.3),
                c,
                &[],
            );
            for dx in [-ex, ex] {
                ctx.line(
                    Pt::new(xm + dx, cy - cap),
                    Pt::new(xm + dx, cy + cap),
                    (ctx.s * 0.9).max(0.3),
                    c,
                    &[],
                );
            }
        }
        if ctx.spec.show_points {
            for (row, v) in g.values.iter().enumerate() {
                let j = stats::jitter(row, i as f64 * 2.9) as f32 * (bar_h * 0.3);
                // 与纵向柱状图保持一致：超出绘图区的点不画（而不是画到框外），
                // 轴域已包含全部观测，正常情况下不会有漏网的点。
                let nv = x.norm(*v);
                if !(0.0..=1.0).contains(&nv) {
                    continue;
                }
                let px = f.plot.x + nv as f32 * f.plot.w;
                draw_marker(
                    ctx,
                    Pt::new(px, cy + j),
                    ctx.spec.point_size as f32 * 0.75,
                    crate::core::graph::Marker::Circle,
                    color.with_alpha(200),
                    true,
                );
            }
        }
        ctx.hits.push(crate::plot::chart::Hit {
            at: Pt::new(xm, cy),
            title: g.name.clone(),
            lines: vec![
                format!("n = {}", g.desc.n),
                format!("mean = {} ± {}", fmt(g.desc.mean), fmt(g.desc.sem)),
                format!("median = {}", fmt(g.desc.median)),
            ],
        });
    }

    ctx.plot = f.plot;
    (x, Axis::linear(-0.5, n as f64 - 0.5))
}

fn err_for(kind: crate::core::graph::ErrorKind, g: &GroupData) -> Option<f64> {
    use crate::core::graph::ErrorKind as E;
    if g.desc.n < 2 {
        return None;
    }
    match kind {
        E::None => None,
        E::Sd => Some(g.desc.sd),
        E::Sem => Some(g.desc.sem),
        E::Ci95 => Some(stats::t_quantile(0.975, (g.desc.n - 1) as f64) * g.desc.sem),
    }
}

// ---------------------------------------------------------------------------
// 前后对比
// ---------------------------------------------------------------------------

pub(crate) fn render_before_after(ctx: &mut Ctx, table: &DataTable) -> (Axis, Axis) {
    let cols: Vec<&crate::core::model::Column> = table
        .columns
        .iter()
        .filter(|c| c.role == ColRole::Y)
        .collect();
    // 取前两列作为「前 / 后」：X 轴会写出这两列的列名，用的是哪两列一目了然。
    if cols.len() < 2 {
        return fallback(ctx, "前后对比图至少需要两列（前 / 后）");
    }
    let n = table.nrows();
    let pairs: Vec<(f64, f64)> = (0..n)
        .filter_map(|r| {
            let a = cols[0].cells.get(r).and_then(|c| c.value())?;
            let b = cols[1].cells.get(r).and_then(|c| c.value())?;
            Some((a, b))
        })
        .collect();
    if pairs.len() < 2 {
        return fallback(ctx, "至少需要两行配对数据");
    }

    let mut yv = Vec::new();
    for (a, b) in &pairs {
        yv.push(*a);
        yv.push(*b);
    }
    let x = Axis::linear(-0.45, 1.45);
    let mut y = Axis::from_values(yv, ctx.spec.y_log, false);
    if let Some([lo, hi]) = ctx.spec.y_range {
        if hi > lo {
            y.min = lo;
            y.max = hi;
        }
    }
    let labels = vec![cols[0].name.clone(), cols[1].name.clone()];
    let yticks = y.ticks(6);
    let up = ctx.color(1);
    let down = ctx.color(3);
    ctx.legend = if ctx.spec.show_legend {
        vec![
            LegendItem {
                label: "升高".into(),
                color: up,
                kind: LegendKind::Square,
            },
            LegendItem {
                label: "下降".into(),
                color: down,
                kind: LegendKind::Square,
            },
        ]
    } else {
        Vec::new()
    };
    let f = frame_layout(
        ctx,
        x,
        y,
        Vec::new(),
        yticks,
        Some(labels),
        "",
        &ctx.spec.y_title.clone(),
    );
    draw_grid(ctx, &f);
    ctx.begin_data(&f);

    let x0 = xpx(&f, 0.0);
    let x1 = xpx(&f, 1.0);
    for (a, b) in &pairs {
        let pa = Pt::new(x0, ypx(&f, *a));
        let pb = Pt::new(x1, ypx(&f, *b));
        if !pa.y.is_finite() || !pb.y.is_finite() {
            continue;
        }
        let color = if b >= a { up } else { down };
        ctx.line(pa, pb, 1.1, color.with_alpha(150), &[]);
        for p in [pa, pb] {
            if f.plot.contains(p) {
                draw_marker(
                    ctx,
                    p,
                    ctx.spec.point_size as f32 * 0.8,
                    crate::core::graph::Marker::Circle,
                    color,
                    true,
                );
            }
        }
        ctx.hits.push(crate::plot::chart::Hit {
            at: Pt::new((pa.x + pb.x) / 2.0, (pa.y + pb.y) / 2.0),
            title: format!("{} → {}", fmt(*a), fmt(*b)),
            lines: vec![format!("变化 = {}", fmt(b - a))],
        });
    }

    draw_axes(ctx, &f, "", &ctx.spec.y_title.clone());
    draw_legend(ctx, &f);
    ctx.plot = f.plot;
    (x, f.y)
}

// ---------------------------------------------------------------------------
// 山脊图
// ---------------------------------------------------------------------------

pub(crate) fn render_ridgeline(ctx: &mut Ctx, table: &DataTable) -> (Axis, Axis) {
    let groups = build_groups(ctx, table);
    if groups.is_empty() {
        return fallback(ctx, ChartKind::Ridgeline.requires());
    }
    let n = groups.len();
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for g in &groups {
        for v in &g.values {
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
    }
    if !(hi > lo) {
        return fallback(ctx, "数据范围为零，无法估计密度");
    }
    // 统一支撑：所有组共享同一段 X，密度才可比。
    let h = groups
        .iter()
        .map(|g| stats::kde_bandwidth(&g.values))
        .fold(0.0_f64, f64::max);
    let pad = if h > 0.0 { 3.0 * h } else { (hi - lo) * 0.05 };
    let glo = lo - pad;
    let ghi = hi + pad;
    let grid_n = 96;
    let grid: Vec<f64> = (0..=grid_n)
        .map(|i| glo + (ghi - glo) * i as f64 / grid_n as f64)
        .collect();
    // 密度在本段的**两端**收到 0：网格只比数据宽 3 个带宽，边缘密度还有
    // 1% 左右，直接截断会在两端留下一条竖直的切边（看起来像被裁掉）。
    // 用升余弦窗把首尾 6% 压到 0，山脊就会自然收敛成一条基线。
    let taper_frac = 0.06;
    let dens: Vec<Vec<f64>> = groups
        .iter()
        .map(|g| {
            let mut d = stats::kde(&g.values, &grid);
            for (i, v) in d.iter_mut().enumerate() {
                let t = i as f64 / grid_n as f64;
                let w = if t < taper_frac {
                    0.5 - 0.5 * (std::f64::consts::PI * (t / taper_frac)).cos()
                } else if t > 1.0 - taper_frac {
                    0.5 - 0.5 * (std::f64::consts::PI * ((1.0 - t) / taper_frac)).cos()
                } else {
                    1.0
                };
                *v *= w;
            }
            d
        })
        .collect();
    let peak = dens
        .iter()
        .flatten()
        .copied()
        .fold(0.0_f64, f64::max)
        .max(1e-12);

    let x = Axis::linear(glo, ghi);
    // 纵向：每组一个单位，曲线高 1.35 个单位 → 相邻组自然重叠。
    let overlap: f32 = 1.35;
    // 顶部要留够一整条曲线的高度：y.max = 最后一条基线 + 曲线高度，
    // 否则最上面那组的波峰会被坐标框切平。
    let y = Axis::linear(-0.35, n as f64 - 1.0 + overlap as f64 * 1.05);
    let xticks = x.ticks(6);
    let labels: Vec<String> = groups.iter().map(|g| g.name.clone()).collect();
    let f = frame_layout(
        ctx,
        x,
        y,
        xticks,
        Vec::new(),
        None,
        &ctx.spec.x_title.clone(),
        "",
    );
    // 自定义左边距：组名写在基线左侧。
    let mut f = f;
    let label_w = labels
        .iter()
        .map(|l| ctx.measure(l, ctx.theme.font_tick))
        .fold(0.0, f32::max);
    let need = ctx.px(12.0) + label_w + ctx.px(8.0);
    if need > f.plot.x {
        let d = need - f.plot.x;
        f.plot.x += d;
        f.plot.w = (f.plot.w - d).max(ctx.px(20.0));
    }
    ctx.begin_data(&f);

    // 网格（竖线，对应 X 刻度）
    if ctx.spec.show_grid {
        for t in &f.xticks {
            if !t.major || t.label.is_empty() {
                continue;
            }
            let px = xpx(&f, t.value);
            ctx.line(
                Pt::new(px, f.plot.y),
                Pt::new(px, f.plot.bottom()),
                0.55,
                ctx.theme.grid,
                if ctx.theme.grid_dash {
                    &[3.0, 3.0]
                } else {
                    &[]
                },
            );
        }
    }
    // 轴线
    ctx.spine(
        Pt::new(f.plot.x, f.plot.bottom()),
        Pt::new(f.plot.right(), f.plot.bottom()),
        ctx.theme.axis_width,
        ctx.theme.axis,
    );
    ctx.spine(
        Pt::new(f.plot.x, f.plot.y),
        Pt::new(f.plot.x, f.plot.bottom()),
        ctx.theme.axis_width,
        ctx.theme.axis,
    );
    let tick_len = ctx.px(ctx.theme.tick_len);
    for t in &f.xticks {
        if !t.major {
            continue;
        }
        let px = xpx(&f, t.value);
        ctx.line(
            Pt::new(px, f.plot.bottom()),
            Pt::new(px, f.plot.bottom() + tick_len),
            ctx.theme.axis_width * 0.9,
            ctx.theme.axis,
            &[],
        );
        if !t.label.is_empty() {
            ctx.text(
                &t.label,
                Pt::new(px, f.plot.bottom() + tick_len + ctx.px(3.0)),
                ctx.theme.font_tick,
                ctx.theme.ink,
                HAlign::Center,
                VAlign::Top,
                0.0,
                false,
            );
        }
    }
    if !ctx.spec.x_title.is_empty() {
        ctx.text(
            &ctx.spec.x_title.clone(),
            Pt::new(f.plot.center().x, ctx.h - ctx.px(3.0)),
            ctx.theme.font_axis_title,
            ctx.theme.ink,
            HAlign::Center,
            VAlign::Bottom,
            0.0,
            false,
        );
    }
    if ctx.spec.show_title && !ctx.spec.title.is_empty() {
        ctx.text(
            &ctx.spec.title.clone(),
            Pt::new(ctx.w / 2.0, ctx.px(6.0)),
            ctx.theme.font_title,
            ctx.theme.ink,
            HAlign::Center,
            VAlign::Top,
            0.0,
            true,
        );
    }

    // 自上而下画：后画的（下面的组）压在上面。
    // 反过来的话，下面那组的峰会钻到上面那组的填充底下被吃掉，
    // 图上只剩一个被削平的轮廓。
    let unit = f.plot.h / (y.max - y.min) as f32;
    for i in (0..groups.len()).rev() {
        let g = &groups[i];
        let base_val = i as f64;
        let color = g.color;
        let mut left: Vec<Pt> = Vec::with_capacity(grid_n + 1);
        let mut right: Vec<Pt> = Vec::with_capacity(grid_n + 1);
        for (k, xv) in grid.iter().enumerate() {
            let px = xpx(&f, *xv);
            let y0 = ypx(&f, base_val);
            let hh = (dens[i][k] / peak) as f32 * overlap * unit;
            left.push(Pt::new(px, y0));
            right.push(Pt::new(px, y0 - hh));
        }
        right.reverse();
        left.extend(right);
        ctx.check_poly("山脊轮廓", &left);
        let clipped = clip::polygon(&left, f.plot);
        if clipped.len() >= 3 {
            let mut stroke = Stroke::new((ctx.s * 0.8).max(0.35), color.darken(0.2));
            stroke.join = LineJoin::Round;
            // 半透明填充与下一层叠加会混出棕色，图上读不出「哪条是哪组」。
            // 用几乎不透明的浅色填充 + 饱和描边：层叠关系清楚，颜色也不串。
            let fill = {
                let c = color.lighten(0.55);
                c.with_alpha(246)
            };
            ctx.scene
                .push(Path::polygon(&clipped), Some(fill), Some(stroke));
        }
        // 基线与组名
        ctx.line(
            Pt::new(f.plot.x, ypx(&f, base_val)),
            Pt::new(f.plot.right(), ypx(&f, base_val)),
            (ctx.s * 0.55).max(0.25),
            ctx.theme.grid,
            &[],
        );
        ctx.text(
            &g.name,
            Pt::new(f.plot.x - ctx.px(6.0), ypx(&f, base_val)),
            ctx.theme.font_tick,
            ctx.theme.ink,
            HAlign::Right,
            VAlign::Middle,
            0.0,
            false,
        );
        ctx.hits.push(crate::plot::chart::Hit {
            at: Pt::new(f.plot.center().x, ypx(&f, base_val) - unit * 0.4),
            title: g.name.clone(),
            lines: vec![
                format!("n = {}", g.desc.n),
                format!("median = {}", fmt(g.desc.median)),
            ],
        });
    }

    ctx.plot = f.plot;
    (x, y)
}

// ---------------------------------------------------------------------------
// 饼 / 环
// ---------------------------------------------------------------------------

/// 扇区定义：名称 + 数值 + 取值口径说明。
fn pie_slices(table: &DataTable) -> Option<(Vec<String>, Vec<f64>, &'static str)> {
    let cols: Vec<&crate::core::model::Column> = table
        .columns
        .iter()
        .filter(|c| c.role == ColRole::Y)
        .collect();
    if cols.is_empty() {
        return None;
    }
    // 单列 + 行标题：每行是一个部分（构成表的标准形态）。
    if cols.len() == 1 {
        let mut names = Vec::new();
        let mut vals = Vec::new();
        for r in 0..table.nrows() {
            if let Some(v) = cols[0].cells.get(r).and_then(|c| c.value()) {
                names.push(
                    table
                        .row_headers
                        .get(r)
                        .filter(|s| !s.is_empty())
                        .cloned()
                        .unwrap_or_else(|| format!("{}", r + 1)),
                );
                vals.push(v);
            }
        }
        if vals.len() >= 2 {
            return Some((names, vals, "数值"));
        }
    }
    // 多列：每列一个扇区。计数表用合计，其他用均值。
    let count_like = matches!(table.kind, TableKind::Contingency | TableKind::PartsOfWhole);
    let mut names = Vec::new();
    let mut vals = Vec::new();
    for c in &cols {
        let v: Vec<f64> = c.cells.iter().filter_map(|x| x.value()).collect();
        if v.is_empty() {
            continue;
        }
        names.push(if table.kind == TableKind::Column {
            c.name.clone()
        } else {
            table.group_name(c.group)
        });
        vals.push(if count_like {
            v.iter().sum::<f64>()
        } else {
            stats::mean(&v)
        });
    }
    if vals.len() < 2 {
        return None;
    }
    Some((names, vals, if count_like { "合计" } else { "均值" }))
}

pub(crate) fn render_pie(ctx: &mut Ctx, table: &DataTable, kind: ChartKind) -> (Axis, Axis) {
    let Some((names, vals, unit)) = pie_slices(table) else {
        return fallback(ctx, kind.requires());
    };
    if vals.iter().any(|v| *v < 0.0) {
        return fallback(ctx, "饼图不能表示负值，请改用柱状图");
    }
    let total: f64 = vals.iter().sum();
    if !(total > 0.0) {
        return fallback(ctx, "所有扇区的值都是 0");
    }

    let donut = kind == ChartKind::Donut;
    let show_legend = ctx.spec.show_legend;
    let s = ctx.s;

    // 版式：左边饼、右边图例（图例宽度按最长项量）
    let legend_w = if show_legend {
        names
            .iter()
            .map(|n| ctx.measure(n, ctx.theme.font_legend) + s * 30.0)
            .fold(0.0, f32::max)
            + s * 12.0
    } else {
        0.0
    };
    let title_h = if ctx.spec.show_title && !ctx.spec.title.is_empty() {
        s * ctx.theme.font_title * 1.35 + s * 7.0
    } else {
        0.0
    };
    let box_w = (ctx.w - legend_w - s * 24.0).max(s * 40.0);
    let area = Rect::new(
        s * 12.0,
        s * 10.0 + title_h,
        box_w,
        ctx.h - s * 20.0 - title_h,
    );
    let radius = (area.w.min(area.h) * 0.5 - s * 6.0).max(s * 10.0);
    let center = area.center();
    if ctx.spec.show_title && !ctx.spec.title.is_empty() {
        ctx.text(
            &ctx.spec.title.clone(),
            Pt::new(ctx.w / 2.0, s * 6.0),
            ctx.theme.font_title,
            ctx.theme.ink,
            HAlign::Center,
            VAlign::Top,
            0.0,
            true,
        );
    }

    let inner = if donut { radius * 0.58 } else { 0.0 };
    let mut angle = -PI / 2.0;
    let mut legend: Vec<LegendItem> = Vec::new();
    for (i, (name, v)) in names.iter().zip(vals.iter()).enumerate() {
        let frac = v / total;
        let sweep = (frac * 2.0 * std::f64::consts::PI) as f32;
        if sweep <= 1e-6 {
            continue;
        }
        let color = ctx.color(i);
        let steps = ((sweep / 0.06).ceil() as usize).max(6);
        let mut poly: Vec<Pt> = Vec::with_capacity(steps + 2);
        for k in 0..=steps {
            let a = angle + sweep * k as f32 / steps as f32;
            poly.push(Pt::new(
                center.x + radius * a.cos(),
                center.y + radius * a.sin(),
            ));
        }
        if inner > 0.0 {
            for k in (0..=steps).rev() {
                let a = angle + sweep * k as f32 / steps as f32;
                poly.push(Pt::new(
                    center.x + inner * a.cos(),
                    center.y + inner * a.sin(),
                ));
            }
        } else {
            poly.push(center);
        }
        ctx.scene.push(
            Path::polygon(&poly),
            Some(color.with_alpha(ctx.theme.bar_alpha.max(200))),
            Some(Stroke::new((s * 0.8).max(0.4), Rgba::WHITE)),
        );
        // 百分比标签：只给足够大的扇区，避免文字互相压住
        if frac >= 0.06 {
            let mid = angle + sweep / 2.0;
            let lr = if inner > 0.0 {
                (radius + inner) / 2.0
            } else {
                radius * 0.66
            };
            ctx.text(
                &format!("{:.0}%", frac * 100.0),
                Pt::new(center.x + lr * mid.cos(), center.y + lr * mid.sin()),
                ctx.theme.font_legend,
                crate::plot::chart::ink_on(color),
                HAlign::Center,
                VAlign::Middle,
                0.0,
                true,
            );
        }
        let mid = angle + sweep / 2.0;
        let hr = if inner > 0.0 {
            (radius + inner) / 2.0
        } else {
            radius * 0.7
        };
        ctx.hits.push(crate::plot::chart::Hit {
            at: Pt::new(center.x + hr * mid.cos(), center.y + hr * mid.sin()),
            title: name.clone(),
            lines: vec![
                format!("{unit} = {}", fmt(*v)),
                format!("占比 = {:.1}%", frac * 100.0),
            ],
        });
        legend.push(LegendItem {
            label: format!("{}  {:.0}%", name, frac * 100.0),
            color,
            kind: LegendKind::Square,
        });
        angle += sweep;
    }

    // 环心总数
    if donut {
        ctx.text(
            &fmt(total),
            Pt::new(center.x, center.y - s * 2.0),
            ctx.theme.font_title,
            ctx.theme.ink,
            HAlign::Center,
            VAlign::Bottom,
            0.0,
            true,
        );
        ctx.text(
            "合计",
            Pt::new(center.x, center.y + s * 2.0),
            ctx.theme.font_tick,
            ctx.theme.muted,
            HAlign::Center,
            VAlign::Top,
            0.0,
            false,
        );
    }

    // 右侧图例（自绘，不占用 plot 矩形）
    if show_legend {
        let x = ctx.w - legend_w;
        let row_h = s * ctx.theme.font_legend * 1.75;
        let total_h = row_h * legend.len() as f32;
        let mut y = (ctx.h - total_h) / 2.0;
        for item in &legend {
            let cy = y + row_h / 2.0;
            ctx.scene.rect(
                Rect::new(x, cy - s * 2.8, s * 5.6, s * 5.6),
                Some(item.color),
                None,
            );
            ctx.text(
                &item.label,
                Pt::new(x + s * 10.0, cy),
                ctx.theme.font_legend,
                ctx.theme.ink,
                HAlign::Left,
                VAlign::Middle,
                0.0,
                false,
            );
            y += row_h;
        }
    }

    ctx.plot = area;
    let _ = show_legend;
    (Axis::linear(0.0, 1.0), Axis::linear(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{Cell, Column};

    fn table(kind: TableKind, cols: usize, rows: usize) -> DataTable {
        let mut t = DataTable {
            id: 1,
            name: "t".into(),
            kind,
            columns: Vec::new(),
            group_names: Vec::new(),
            row_headers: (0..rows).map(|i| format!("R{i}")).collect(),
            row_header_width: None,
            notes: String::new(),
        };
        for c in 0..cols {
            let mut col = Column::new(format!("C{c}"), ColRole::Y);
            col.group = c;
            col.cells = (0..rows)
                .map(|r| Cell::number((c * 10 + r) as f64))
                .collect();
            t.columns.push(col);
            t.group_names.push(format!("C{c}"));
        }
        t.ensure_rows(rows);
        t
    }

    #[test]
    fn 矩阵按行聚合并取均值() {
        let mut t = table(TableKind::Grouped, 3, 4);
        // R0/R2 同名 → 归为同一类别
        t.row_headers = vec!["A".into(), "B".into(), "A".into(), "B".into()];
        let m = category_matrix(&t).expect("matrix");
        assert_eq!(m.cats, vec!["A", "B"]);
        assert_eq!(m.series.len(), 3);
        // 系列 0：A = mean(0, 2) = 1，B = mean(1, 3) = 2
        assert!((m.values[0][0].unwrap() - 1.0).abs() < 1e-9);
        assert!((m.values[0][1].unwrap() - 2.0).abs() < 1e-9);
        assert_eq!(m.counts[0][0], 2);
    }

    #[test]
    fn 饼图扇区来自单列的行() {
        let mut t = table(TableKind::PartsOfWhole, 1, 4);
        t.row_headers = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        let (names, vals, _) = pie_slices(&t).expect("slices");
        assert_eq!(names, vec!["a", "b", "c", "d"]);
        assert_eq!(vals, vec![0.0, 1.0, 2.0, 3.0]);
    }

    #[test]
    fn 矩阵保留负值供双向堆叠() {
        let mut t = table(TableKind::Grouped, 2, 2);
        t.columns[1].cells[0] = Cell::number(-12.0);
        let m = category_matrix(&t).expect("matrix");
        assert!(
            (m.values[1][0].unwrap() + 12.0).abs() < 1e-9,
            "负值被丢弃了"
        );
    }

    #[test]
    fn 饼图拒绝负值() {
        let mut t = table(TableKind::Column, 2, 3);
        // 整列都是负值：均值才是负的（单点负值会被均值抹平）
        for (i, c) in t.columns[1].cells.iter_mut().enumerate() {
            *c = Cell::number(-1.0 - i as f64);
        }
        let (_, vals, _) = pie_slices(&t).expect("slices");
        assert!(vals.iter().any(|v| *v < 0.0), "{vals:?}");
    }
}
