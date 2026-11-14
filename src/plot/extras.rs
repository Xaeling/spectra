//! 分布图（直方 / ECDF / QQ）、生存曲线、热图。

use crate::core::color::Rgba;
use crate::core::graph::ChartKind;
use crate::core::model::{format_number, TableKind};
use crate::core::stats;
use crate::plot::axis::Axis;
use crate::plot::chart::{
    build_groups, draw_axes, draw_grid, draw_legend, draw_marker, frame_layout, xpx, ypx, Ctx,
    LegendItem, LegendKind,
};
use crate::plot::scene::{LineCap, LineJoin, Path, Pt, Rect, Stroke};
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
            if i == 0 { 12.0 } else { 9.5 },
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

// ---------------------------------------------------------------------------
// 直方 / ECDF / QQ
// ---------------------------------------------------------------------------

pub(crate) fn render_distribution(
    ctx: &mut Ctx,
    table: &crate::core::model::DataTable,
    kind: ChartKind,
) -> (Axis, Axis) {
    let groups = build_groups(ctx, table);
    if groups.is_empty() {
        return fallback(ctx, "这张表里还没有可绘制的数据");
    }
    match kind {
        ChartKind::Histogram => render_hist(ctx, &groups),
        ChartKind::Ecdf => render_ecdf(ctx, &groups),
        ChartKind::Qq => render_qq(ctx, &groups),
        _ => fallback(ctx, "不支持的分布图"),
    }
}

fn render_hist(ctx: &mut Ctx, groups: &[crate::plot::chart::GroupData]) -> (Axis, Axis) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for g in groups {
        for v in &g.values {
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
    }
    if !(hi > lo) {
        return fallback(ctx, "数据范围为零，无法分箱");
    }
    // Freedman–Diaconis，失败时退回 Sturges
    let n_all: usize = groups.iter().map(|g| g.values.len()).sum();
    let mut bins = ctx.spec.histogram_bins;
    if bins == 0 {
        let mut merged: Vec<f64> = groups
            .iter()
            .flat_map(|g| g.values.iter().copied())
            .collect();
        merged.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let iqr = stats::quantile(&merged, 0.75) - stats::quantile(&merged, 0.25);
        let fd = if iqr > 0.0 {
            (hi - lo) / (2.0 * iqr / (n_all as f64).powf(1.0 / 3.0))
        } else {
            0.0
        };
        let sturges = 1.0 + (n_all as f64).log2();
        bins = fd.round().clamp(3.0, 60.0) as usize;
        if !(fd >= 3.0) {
            bins = sturges.round().clamp(3.0, 40.0) as usize;
        }
    }
    let bins = bins.clamp(2, 80);
    let width = (hi - lo) / bins as f64;
    let edges: Vec<f64> = (0..=bins).map(|i| lo + width * i as f64).collect();
    let counts: Vec<Vec<usize>> = groups
        .iter()
        .map(|g| {
            let mut c = vec![0usize; bins];
            for v in &g.values {
                let idx =
                    (((v - lo) / width).floor() as isize).clamp(0, bins as isize - 1) as usize;
                c[idx] += 1;
            }
            c
        })
        .collect();
    // A5 第 2 条：纵轴必须是 **Density**（count / (n × bin_width)），不是 Count。
    // 各组样本量不同时，频数柱子会误导读者以为「样本多 = 更集中」。
    let density: Vec<Vec<f64>> = groups
        .iter()
        .enumerate()
        .map(|(gi, g)| {
            let n = g.values.len().max(1) as f64;
            counts[gi].iter().map(|&c| c as f64 / (n * width)).collect()
        })
        .collect();
    let ymax = density
        .iter()
        .flatten()
        .copied()
        .fold(0.0_f64, f64::max)
        .max(1e-9);

    let x = Axis::linear(lo, hi);
    let y = Axis::linear(0.0, ymax * 1.12);
    let xticks = x.ticks(6);
    let yticks = y.ticks(5);

    ctx.legend = if groups.len() > 1 {
        groups
            .iter()
            .enumerate()
            .map(|(i, g)| LegendItem {
                label: g.name.clone(),
                color: ctx.color(i),
                kind: LegendKind::Square,
            })
            .collect()
    } else {
        Vec::new()
    };

    let x_title = if ctx.spec.x_title.is_empty() {
        ctx.spec.y_title.clone()
    } else {
        ctx.spec.x_title.clone()
    };
    let f = frame_layout(ctx, x, y, xticks, yticks, None, &x_title, "密度");
    draw_grid(ctx, &f);
    ctx.begin_data(&f);

    let k = groups.len();
    let slot = (xpx(&f, edges[1]) - xpx(&f, edges[0])).max(1.0);
    for (bi, bin) in edges.windows(2).enumerate() {
        let bx = xpx(&f, bin[0]);
        for (gi, g) in groups.iter().enumerate() {
            let c = counts[gi][bi];
            if c == 0 {
                continue;
            }
            // 单组是连续分箱，柱子必须相接无缝；多组并排时才留一点缝区分。
            let inner = if k > 1 { 0.97 } else { 1.0 };
            let inset = if k > 1 { 0.015 } else { 0.0 };
            let w = slot / k as f32 * inner;
            let x0 = bx + gi as f32 * slot / k as f32 + slot / k as f32 * inset;
            let y0 = ypx(&f, 0.0);
            let dv = density[gi][bi];
            let yv = ypx(&f, dv);
            let rect = Rect::new(x0, yv, w, y0 - yv);
            ctx.check_rect("直方柱", rect, ctx.s * 0.3);
            let color = g.color;
            ctx.scene.rect(
                rect,
                // A5-3：柱填充 alpha 0.45 + **不透明**边框。
                Some(color.with_alpha(115)),
                Some(Stroke::new((ctx.s * 0.5).max(0.3), color)),
            );
            ctx.hits.push(crate::plot::chart::Hit {
                at: Pt::new(x0 + w / 2.0, yv),
                title: g.name.clone(),
                lines: vec![
                    format!("区间 {} – {}", fmt(bin[0]), fmt(bin[1])),
                    format!("密度 = {:.4}（频数 {c}）", dv),
                ],
            });
        }
    }

    // A5 第 3 / 5 条：密度曲线叠在柱上，线宽 1.2–1.8 pt、不透明、同色系更深。
    // 曲线用与柱面积一致的口径（密度），所以两者可直接对照。
    let grid_n = 128;
    let gx: Vec<f64> = (0..=grid_n)
        .map(|i| lo + (hi - lo) * i as f64 / grid_n as f64)
        .collect();
    for (gi, g) in groups.iter().enumerate() {
        let curve = stats::kde(&g.values, &gx);
        let mut path = Path::new();
        let mut first = true;
        for (k, &xv) in gx.iter().enumerate() {
            let p = Pt::new(xpx(&f, xv), ypx(&f, curve[k]));
            if !f.plot.contains(p) {
                continue;
            }
            if first {
                path.move_to(p);
                first = false;
            } else {
                path.line_to(p);
            }
        }
        if !first {
            let mut st = Stroke::new((ctx.s * 1.4).max(0.6), ctx.color(gi).darken(0.28));
            st.cap = LineCap::Round;
            st.join = LineJoin::Round;
            ctx.scene.stroke(path, st);
        }
    }

    draw_axes(ctx, &f, &x_title, "密度");
    draw_legend(ctx, &f);
    ctx.plot = f.plot;
    (x, y)
}

fn render_ecdf(ctx: &mut Ctx, groups: &[crate::plot::chart::GroupData]) -> (Axis, Axis) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for g in groups {
        for v in &g.values {
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
    }
    let x = Axis::from_values([lo, hi], false, false);
    let y = Axis::linear(0.0, 1.0);
    let xticks = x.ticks(6);
    let yticks = vec![
        tick(0.0, "0"),
        tick(0.25, "0.25"),
        tick(0.5, "0.50"),
        tick(0.75, "0.75"),
        tick(1.0, "1.00"),
    ];
    ctx.legend = if groups.len() > 1 {
        groups
            .iter()
            .enumerate()
            .map(|(i, g)| LegendItem {
                label: g.name.clone(),
                color: ctx.color(i),
                kind: crate::plot::chart::legend_kind_for_line(i, false),
            })
            .collect()
    } else {
        Vec::new()
    };
    let x_title = if ctx.spec.x_title.is_empty() {
        ctx.spec.y_title.clone()
    } else {
        ctx.spec.x_title.clone()
    };
    let f = frame_layout(ctx, x, y, xticks, yticks, None, &x_title, "累积比例");
    draw_grid(ctx, &f);
    ctx.begin_data(&f);

    for (gi, g) in groups.iter().enumerate() {
        let mut v = g.values.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = v.len() as f64;
        let color = ctx.color(gi);
        // ECDF 是右连续阶梯：在数据轴最左端保持 0、最右端保持 1。
        // 不补这两段，曲线会「悬」在图中间，读不出 P0 / P100。
        let mut path = Path::new();
        path.move_to(Pt::new(xpx(&f, x.min), ypx(&f, 0.0)));
        let mut prev_y = 0.0;
        for (i, xv) in v.iter().enumerate() {
            let yv = (i as f64 + 1.0) / n;
            let px = xpx(&f, *xv);
            path.line_to(Pt::new(px, ypx(&f, prev_y)));
            path.line_to(Pt::new(px, ypx(&f, yv)));
            prev_y = yv;
        }
        path.line_to(Pt::new(xpx(&f, x.max), ypx(&f, 1.0)));
        let mut st = Stroke::new((ctx.s * ctx.spec.line_width as f32).max(0.6), color);
        st.cap = LineCap::Round;
        if groups.len() > 1 {
            st.dash = crate::plot::chart::series_dash(gi)
                .iter()
                .map(|d| d * ctx.s)
                .collect();
        }
        ctx.scene.push(path, None, Some(st));
    }

    draw_axes(ctx, &f, &x_title, "累积比例");
    draw_legend(ctx, &f);
    ctx.plot = f.plot;
    (x, y)
}

fn tick(value: f64, label: &str) -> crate::plot::axis::Tick {
    crate::plot::axis::Tick {
        value,
        norm: value,
        label: label.into(),
        major: true,
    }
}

fn render_qq(ctx: &mut Ctx, groups: &[crate::plot::chart::GroupData]) -> (Axis, Axis) {
    let mut theoretical = Vec::new();
    let mut samples = Vec::new();
    for g in groups {
        let mut v = g.values.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = v.len() as f64;
        for (i, s) in v.iter().enumerate() {
            theoretical.push(stats::norm_ppf((i as f64 + 0.5) / n));
            samples.push(*s);
        }
    }
    let mut tx = theoretical.clone();
    tx.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let x = Axis::from_values(tx.clone(), false, false);
    let y = Axis::from_values(samples.clone(), false, false);
    let xticks = x.ticks(6);
    let yticks = y.ticks(6);
    ctx.legend = if groups.len() > 1 {
        groups
            .iter()
            .enumerate()
            .map(|(i, g)| LegendItem {
                label: g.name.clone(),
                color: ctx.color(i),
                // 画的是散点，图例就该是点而不是色块。
                kind: LegendKind::Marker,
            })
            .collect()
    } else {
        Vec::new()
    };
    let f = frame_layout(ctx, x, y, xticks, yticks, None, "理论分位数", "样本分位数");
    draw_grid(ctx, &f);
    ctx.begin_data(&f);

    // 标准正态参考线用「第一 / 第三四分位数」拟合，比 mean±sd 更稳健：
    // slope = (Q3−Q1)/(z3−z1)，intercept = Q1 − slope·z1。
    // 这是 ggplot2 stat_qq_line 的口径；离群点不会把参考线拽歪。
    let zq = stats::norm_ppf(0.75);
    for (gi, g) in groups.iter().enumerate() {
        let mut v = g.values.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = v.len() as f64;
        let spread = g.desc.q3 - g.desc.q1;
        let slope = if zq > 0.0 { spread / (2.0 * zq) } else { 0.0 };
        let intercept = g.desc.q1 + slope * zq;
        if slope > 0.0 {
            let a = Pt::new(xpx(&f, x.min), ypx(&f, intercept + slope * x.min));
            let b = Pt::new(xpx(&f, x.max), ypx(&f, intercept + slope * x.max));
            if let Some((ca, cb)) = crate::plot::chart::clip_segment_public(f.plot, a, b) {
                ctx.line(ca, cb, 0.9, ctx.color(gi).with_alpha(150), &[4.0, 3.0]);
            }
        }
        let color = ctx.color(gi);
        for (i, s) in v.iter().enumerate() {
            let q = stats::norm_ppf((i as f64 + 0.5) / n);
            let p = Pt::new(xpx(&f, q), ypx(&f, *s));
            if f.plot.contains(p) {
                draw_marker(
                    ctx,
                    p,
                    ctx.spec.point_size as f32 * 0.82,
                    ctx.spec.marker,
                    color.with_alpha(220),
                    ctx.theme.point_edge,
                );
            }
        }
    }

    draw_axes(ctx, &f, "理论分位数", "样本分位数");
    draw_legend(ctx, &f);
    ctx.plot = f.plot;
    (x, y)
}

// ---------------------------------------------------------------------------
// 生存曲线
// ---------------------------------------------------------------------------

pub(crate) fn render_survival(
    ctx: &mut Ctx,
    table: &crate::core::model::DataTable,
) -> (Axis, Axis) {
    if table.kind != TableKind::Survival {
        return fallback(ctx, "生存曲线需要「生存表」（时间 + 事件状态）");
    }
    let groups = table.survival_groups();
    if groups.is_empty() {
        return fallback(ctx, "生存表里还没有数据");
    }
    let mut tmax = 1.0_f64;
    for (_, data) in &groups {
        for (t, _) in data {
            tmax = tmax.max(*t);
        }
    }
    let x = Axis::linear(0.0, tmax * 1.03);
    let y = Axis::linear(0.0, 1.02);
    let xticks = x.ticks(6);
    let yticks = vec![
        tick(0.0, "0"),
        tick(0.25, "0.25"),
        tick(0.5, "0.50"),
        tick(0.75, "0.75"),
        tick(1.0, "1.00"),
    ];
    ctx.legend = groups
        .iter()
        .enumerate()
        .map(|(i, (name, _))| LegendItem {
            label: name.clone(),
            color: ctx.color(i),
            kind: LegendKind::Line,
        })
        .collect();
    let x_title = if ctx.spec.x_title.is_empty() {
        "时间".into()
    } else {
        ctx.spec.x_title.clone()
    };
    // E2-5：图注必须报 log-rank p、HR 及其 95% CI。
    ctx.set_note(crate::core::analysis::survival_note(&groups));
    let mut f = frame_layout(ctx, x, y, xticks, yticks, None, &x_title, "存活比例");
    // E2 第 3 条：生存曲线下方必须有「风险人数表」（numbers at risk），
    // 否则读者无法判断后期的生存率还剩多少样本支撑。
    // 从绘图区底部匀出几行，而不是把表压到轴标题上。
    let risk_h = ctx.px(11.0) * (groups.len() as f32 + 1.0) + ctx.px(8.0);
    f.plot.h = (f.plot.h - risk_h).max(ctx.px(30.0));
    draw_grid(ctx, &f);
    ctx.begin_data(&f);

    for (gi, (_, data)) in groups.iter().enumerate() {
        let points = km(data);
        let color = ctx.color(gi);
        let mut path = Path::new();
        let mut first = true;
        for (t, s) in &points {
            let p = Pt::new(xpx(&f, *t), ypx(&f, *s));
            if first {
                path.move_to(p);
                first = false;
            } else {
                path.line_to(p);
            }
        }
        let mut st = Stroke::new((ctx.s * ctx.spec.line_width as f32).max(0.7), color);
        st.cap = LineCap::Round;
        ctx.scene.push(path, None, Some(st));

        // 删失标记：在曲线上画小竖线
        for (t, s) in censor_positions(data, &points) {
            let p = Pt::new(xpx(&f, t), ypx(&f, s));
            if f.plot.contains(p) {
                ctx.line(
                    Pt::new(p.x, p.y - ctx.px(2.6)),
                    Pt::new(p.x, p.y + ctx.px(2.6)),
                    1.1,
                    color,
                    &[],
                );
            }
        }
    }

    draw_axes(ctx, &f, &x_title, "存活比例");

    // 风险人数表：每个 X 刻度处，统计「时间 ≥ 该刻度」且仍未发生事件/删失的人数。
    // 这里按「尚未退出随访」计数（t ≥ 刻度即仍在风险集），与 KM 的分母口径一致。
    let lh = ctx
        .fonts
        .line_height(ctx.s * ctx.theme.font_tick, ctx.theme.family);
    let y0 = f.plot.bottom() + ctx.px(ctx.theme.tick_len) + ctx.px(3.0) + lh + ctx.px(5.0);
    let small = (ctx.theme.font_tick - 0.8).max(5.0);
    ctx.text(
        "风险人数",
        Pt::new(f.plot.x - ctx.px(4.0), y0),
        small,
        ctx.theme.ink,
        HAlign::Right,
        VAlign::Middle,
        0.0,
        false,
    );
    for (gi, (name, data)) in groups.iter().enumerate() {
        let row_y = y0 + ctx.px(11.0) * (gi as f32 + 1.0);
        ctx.text(
            name,
            Pt::new(f.plot.x - ctx.px(4.0), row_y),
            small,
            ctx.color(gi),
            HAlign::Right,
            VAlign::Middle,
            0.0,
            false,
        );
        for t in &f.xticks {
            if !t.major || t.label.is_empty() {
                continue;
            }
            let at_risk = data.iter().filter(|(tt, _)| *tt >= t.value).count();
            ctx.text(
                &at_risk.to_string(),
                Pt::new(xpx(&f, t.value), row_y),
                small,
                ctx.theme.ink,
                HAlign::Center,
                VAlign::Middle,
                0.0,
                false,
            );
        }
    }

    draw_legend(ctx, &f);
    ctx.plot = f.plot;
    (x, y)
}

/// Kaplan–Meier 阶梯点（含 (0,1) 起点）。
fn km(data: &[(f64, bool)]) -> Vec<(f64, f64)> {
    let mut rows: Vec<(f64, bool)> = data
        .iter()
        .copied()
        .filter(|(t, _)| t.is_finite() && *t >= 0.0)
        .collect();
    rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let n = rows.len();
    let mut at_risk = n as f64;
    let mut s = 1.0;
    let mut out = vec![(0.0, 1.0)];
    let mut i = 0;
    while i < rows.len() {
        let t = rows[i].0;
        let mut events = 0.0;
        let mut censored = 0.0;
        while i < rows.len() && (rows[i].0 - t).abs() < 1e-9 {
            if rows[i].1 {
                events += 1.0;
            } else {
                censored += 1.0;
            }
            i += 1;
        }
        if at_risk > 0.0 && events > 0.0 {
            let next = s * (1.0 - events / at_risk);
            out.push((t, s)); // 水平段到事件时间
            out.push((t, next)); // 垂直下落
            s = next;
        }
        at_risk -= events + censored;
    }
    // 曲线延伸到最后一个观测（含删失），删失标记才不会悬空。
    if let Some(&(last_t, _)) = out.last() {
        if let Some((max_t, _)) = rows.last() {
            if *max_t > last_t {
                out.push((*max_t, s));
            }
        }
    }
    out
}

fn censor_positions(data: &[(f64, bool)], steps: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for (t, event) in data {
        if !*event {
            let mut s = 1.0;
            for (st, sv) in steps {
                if *st <= *t {
                    s = *sv;
                }
            }
            out.push((*t, s));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 热图
// ---------------------------------------------------------------------------

pub(crate) fn render_heatmap(ctx: &mut Ctx, table: &crate::core::model::DataTable) -> (Axis, Axis) {
    let cols: Vec<&crate::core::model::Column> = table
        .columns
        .iter()
        .filter(|c| c.role == crate::core::model::ColRole::Y)
        .collect();
    let rows = table.nrows();
    if cols.is_empty() || rows == 0 {
        return fallback(ctx, "热图需要至少一列数据");
    }
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for c in &cols {
        for v in c.cells.iter().filter_map(|x| x.value()) {
            min = min.min(v);
            max = max.max(v);
        }
    }
    if !min.is_finite() || !max.is_finite() {
        return fallback(ctx, "热图需要数值数据");
    }
    if (max - min).abs() < 1e-12 {
        max = min + 1.0;
    }
    let x = Axis::linear(-0.5, cols.len() as f64 - 0.5);
    // 行标题相同的行取均值，避免热图里出现重复标签。
    let mut row_labels: Vec<String> = Vec::new();
    let mut row_groups: Vec<Vec<usize>> = Vec::new();
    for r in 0..rows {
        let label = table
            .row_headers
            .get(r)
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or_else(|| format!("{}", r + 1));
        match row_labels.iter().position(|l| l == &label) {
            Some(i) => row_groups[i].push(r),
            None => {
                row_labels.push(label);
                row_groups.push(vec![r]);
            }
        }
    }
    let n_rows = row_labels.len();
    let mut matrix: Vec<Vec<f64>> = vec![vec![f64::NAN; cols.len()]; n_rows];
    for (ri, group) in row_groups.iter().enumerate() {
        for (ci, c) in cols.iter().enumerate() {
            let vals: Vec<f64> = group
                .iter()
                .filter_map(|&r| c.cells.get(r).and_then(|x| x.value()))
                .collect();
            if !vals.is_empty() {
                matrix[ri][ci] = vals.iter().sum::<f64>() / vals.len() as f64;
            }
        }
    }
    let rows = n_rows;
    let y = Axis::linear(-0.5, rows as f64 - 0.5).reversed();
    let col_labels: Vec<String> = cols.iter().map(|c| c.name.clone()).collect();
    let yticks: Vec<crate::plot::axis::Tick> = row_labels
        .iter()
        .enumerate()
        .map(|(i, l)| crate::plot::axis::Tick {
            value: i as f64,
            norm: y.norm(i as f64),
            label: l.clone(),
            major: true,
        })
        .collect();
    ctx.legend.clear();
    let mut f = frame_layout(ctx, x, y, Vec::new(), yticks, Some(col_labels), "", "");
    f.plot.w = (f.plot.w - ctx.px(66.0)).max(ctx.px(30.0));
    draw_grid(ctx, &f);
    ctx.begin_data(&f);
    ctx.plot = f.plot;

    let cw = f.plot.w / cols.len() as f32;
    let ch = f.plot.h / rows as f32;
    let base = ctx.color(0);
    for (ci, c) in cols.iter().enumerate() {
        for ri in 0..rows {
            let v = matrix[ri][ci];
            if !v.is_finite() {
                continue;
            }
            let t = ((v - min) / (max - min)).clamp(0.0, 1.0) as f32;
            let color = heat_color(base, t);
            let x0 = f.plot.x + ci as f32 * cw;
            let y0 = f.plot.y + ri as f32 * ch;
            let rect = Rect::new(
                x0 + ctx.px(0.6),
                y0 + ctx.px(0.6),
                (cw - ctx.px(1.2)).max(0.5),
                (ch - ctx.px(1.2)).max(0.5),
            );
            ctx.scene.rect(rect, Some(color), None);
            // §6.5：热图小到能读（< ~200 格）时，把数值印在每一格里。
            // 只靠色深浅读不出具体值，读者必须去对照色标才行。
            if rows * cols.len() <= 200 && cw >= ctx.px(16.0) && ch >= ctx.px(10.0) {
                // 文字压在填充上，对比度必须够（§5.5 ≥ 4.5:1）：
                // 深色格用白字，浅色格用近黑字。
                let lum =
                    0.2126 * color.r as f32 + 0.7152 * color.g as f32 + 0.0722 * color.b as f32;
                let ink = if lum < 132.0 {
                    Rgba::rgba(255, 255, 255, 245)
                } else {
                    Rgba::rgba(28, 34, 44, 240)
                };
                let size = if cw >= ctx.px(30.0) {
                    ctx.theme.font_tick
                } else {
                    (ctx.theme.font_tick - 1.0).max(5.5)
                };
                ctx.text(
                    &fmt(v),
                    Pt::new(rect.center().x, rect.center().y),
                    size,
                    ink,
                    HAlign::Center,
                    VAlign::Middle,
                    0.0,
                    false,
                );
            }
            ctx.hits.push(crate::plot::chart::Hit {
                at: Pt::new(x0 + cw / 2.0, y0 + ch / 2.0),
                title: format!("{} · {}", c.name, row_labels[ri]),
                lines: vec![format!("值 = {}", fmt(v))],
            });
        }
    }

    draw_axes(ctx, &f, "", "");
    // 色标
    let bar_w = ctx.px(11.0);
    let bx = f.plot.right() + ctx.px(16.0);
    let steps = 64;
    for i in 0..steps {
        let t = i as f32 / (steps - 1) as f32;
        let y0 = f.plot.bottom() - f.plot.h * (i as f32 / steps as f32);
        let rect = Rect::new(
            bx,
            y0 - f.plot.h / steps as f32,
            bar_w,
            f.plot.h / steps as f32 + 0.6,
        );
        ctx.scene.rect(rect, Some(heat_color(base, t)), None);
    }
    ctx.text(
        &fmt(max),
        Pt::new(bx + bar_w + ctx.px(3.0), f.plot.y),
        ctx.theme.font_tick,
        ctx.theme.ink,
        HAlign::Left,
        VAlign::Middle,
        0.0,
        false,
    );
    ctx.text(
        &fmt(min),
        Pt::new(bx + bar_w + ctx.px(3.0), f.plot.bottom()),
        ctx.theme.font_tick,
        ctx.theme.ink,
        HAlign::Left,
        VAlign::Middle,
        0.0,
        false,
    );
    (x, y)
}

fn heat_color(base: Rgba, t: f32) -> Rgba {
    let light = Rgba::hex("#F5F8FC");
    let dark = base.darken(0.42);
    if t < 0.55 {
        lerp(light, base, t / 0.55)
    } else {
        lerp(base, dark, (t - 0.55) / 0.45)
    }
}

fn lerp(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Rgba::rgba(f(a.r, b.r), f(a.g, b.g), f(a.b, b.b), 255)
}

/// ROC 曲线：单位正方形 [0,1]²，对角参考线 + 阶梯曲线 + Youden 最佳界值点。
///
/// 数据取前两组连续观测（第 1 组 = 阳性，第 2 组 = 阴性），与 `analysis::roc` 同口径。
pub(crate) fn render_roc(ctx: &mut Ctx, table: &crate::core::model::DataTable) -> (Axis, Axis) {
    if table.analysis_groups().len() < 2 {
        return fallback(ctx, "ROC 曲线需要两组连续观测：第 1 组阳性、第 2 组阴性");
    }
    use crate::core::stats;
    let groups = table.analysis_groups();
    let x = Axis::linear(0.0, 1.0);
    let y = Axis::linear(0.0, 1.0);
    let xticks = x.ticks(5);
    let yticks = y.ticks(5);
    let f = frame_layout(ctx, x, y, xticks, yticks, None, "1 − 特异度", "灵敏度");
    draw_grid(ctx, &f);
    ctx.begin_data(&f);

    // 对角参考线（随机猜测 = AUC 0.5），虚线、浅色。
    let mut dash = Stroke::new((ctx.s * 0.6).max(0.3), ctx.theme.axis);
    dash.dash = vec![ctx.px(3.0), ctx.px(2.5)];
    ctx.scene.line(
        Pt::new(xpx(&f, 0.0), ypx(&f, 0.0)),
        Pt::new(xpx(&f, 1.0), ypx(&f, 1.0)),
        dash,
    );

    if let (Some((_, pos)), Some((_, neg))) = (groups.first(), groups.get(1)) {
        if let Some(r) = stats::roc(pos, neg) {
            let color = ctx.color(0);
            // 阶梯式折线（ROC 的自然形态：界值只在观测处跳变）
            let mut path = Path::new();
            let mut prev: Option<Pt> = None;
            for &(fx, fy) in &r.points {
                let p = Pt::new(xpx(&f, fx), ypx(&f, fy));
                match prev {
                    None => path.move_to(p),
                    Some(q) => {
                        path.line_to(Pt::new(p.x, q.y)); // 先横
                        path.line_to(p); // 后竖
                    }
                }
                prev = Some(p);
            }
            let mut st = Stroke::new((ctx.s * 1.6).max(0.8), color);
            st.cap = LineCap::Round;
            st.join = LineJoin::Round;
            ctx.scene.stroke(path, st);

            // Youden 最佳界值点
            let px = xpx(&f, 1.0 - r.spec);
            let py = ypx(&f, r.sens);
            draw_marker(
                ctx,
                Pt::new(px, py),
                ctx.spec.point_size as f32 * 1.1,
                crate::core::graph::Marker::Circle,
                color,
                true,
            );
            ctx.check_pt("ROC 最佳界值点", Pt::new(px, py), ctx.px(3.0));

            // AUC 标注：右下（对角线下方）是空白区，不压曲线。
            ctx.text(
                &format!("AUC = {:.3}", r.auc),
                Pt::new(f.plot.right() - ctx.px(3.0), f.plot.bottom() - ctx.px(3.0)),
                ctx.theme.font_legend,
                ctx.theme.ink,
                HAlign::Right,
                VAlign::Bottom,
                0.0,
                true,
            );
        }
    }
    draw_axes(ctx, &f, "1 − 特异度", "灵敏度");
    (f.x, f.y)
}

/// 森林图：每行一个研究，横线 = 95% CI（带端帽），方块 = 效应量点估计
/// （有权重列时面积 ∝ 权重），虚线 = 无效线（值域含 0 时画）。
///
/// 数据（多变量表）：列1=效应量、列2=CI下限、列3=CI上限、可选列4=权重；
/// 行标题 = 研究名。
pub(crate) fn render_forest(ctx: &mut Ctx, table: &crate::core::model::DataTable) -> (Axis, Axis) {
    use crate::core::model::{ColRole, TableKind};
    if table.kind != TableKind::MultipleVariables {
        return fallback(
            ctx,
            "森林图需要「多变量表」：每行一个研究，列1=效应量、列2=CI下限、列3=CI上限",
        );
    }
    let ycols: Vec<&crate::core::model::Column> = table
        .columns
        .iter()
        .filter(|c| c.role == ColRole::Y)
        .collect();
    if ycols.len() < 3 {
        return fallback(ctx, "森林图需要至少 3 列：效应量、CI 下限、CI 上限");
    }
    let mut rows: Vec<(f64, f64, f64, f64)> = Vec::new(); // (est, lo, hi, weight)
    for i in 0..table.nrows() {
        let get = |c: usize| {
            ycols
                .get(c)
                .and_then(|col| col.cells.get(i))
                .and_then(|v| v.value())
        };
        if let (Some(e), Some(a), Some(b)) = (get(0), get(1), get(2)) {
            rows.push((e, a.min(b), a.max(b), get(3).unwrap_or(1.0)));
        }
    }
    if rows.is_empty() {
        return fallback(ctx, "森林图需要效应量与 CI 上下限齐全的观测行");
    }
    let labels: Vec<String> = (0..rows.len())
        .map(|i| {
            if table
                .row_headers
                .get(i)
                .map(|s| !s.is_empty())
                .unwrap_or(false)
            {
                table.row_headers[i].clone()
            } else {
                format!("研究 {}", i + 1)
            }
        })
        .collect();

    let mut vals: Vec<f64> = Vec::new();
    for &(_, lo, hi, _) in &rows {
        vals.push(lo);
        vals.push(hi);
    }
    let x = Axis::from_values(vals, ctx.spec.x_log, false);
    let xticks = x.ticks(6);
    let x_title = ctx.spec.x_title.clone();
    let mut f = crate::plot::kinds::frame_horizontal(ctx, &labels, x, xticks, &x_title);
    // 类目名由本函数画在左侧；置空 f.cat，否则 draw_axes 会把它们画到底部当 X 刻度。
    f.cat = None;
    draw_grid(ctx, &f);
    ctx.begin_data(&f);

    // 无效线：值域包含 0 时画（加性尺度的"无效应"参考）。
    if f.x.min <= 0.0 && f.x.max >= 0.0 {
        let mut dash = Stroke::new((ctx.s * 0.6).max(0.3), ctx.theme.axis);
        dash.dash = vec![ctx.px(3.0), ctx.px(2.5)];
        let x0 = f.plot.x + f.x.norm(0.0) as f32 * f.plot.w;
        ctx.scene
            .line(Pt::new(x0, f.plot.y), Pt::new(x0, f.plot.bottom()), dash);
    }

    let n = rows.len() as f32;
    let row_h = f.plot.h / n;
    let w_max = rows.iter().map(|r| r.3).fold(0.0_f64, f64::max).max(1.0);
    for (i, &(est, lo, hi, w)) in rows.iter().enumerate() {
        let cy = f.plot.y + (i as f32 + 0.5) * row_h;
        let xlo = f.plot.x + f.x.norm(lo) as f32 * f.plot.w;
        let xhi = f.plot.x + f.x.norm(hi) as f32 * f.plot.w;
        let xe = f.plot.x + f.x.norm(est) as f32 * f.plot.w;
        let color = ctx.color(0);
        // 研究名（左侧）
        ctx.text(
            &labels[i],
            Pt::new(f.plot.x - ctx.px(3.0), cy),
            ctx.theme.font_tick,
            ctx.theme.ink,
            HAlign::Right,
            VAlign::Middle,
            0.0,
            false,
        );
        // CI 横线 + 端帽
        let cap = row_h * 0.22;
        ctx.spine(
            Pt::new(xlo, cy),
            Pt::new(xhi, cy),
            (ctx.s * 1.1).max(0.6),
            color,
        );
        ctx.spine(
            Pt::new(xlo, cy - cap),
            Pt::new(xlo, cy + cap),
            (ctx.s * 1.1).max(0.6),
            color,
        );
        ctx.spine(
            Pt::new(xhi, cy - cap),
            Pt::new(xhi, cy + cap),
            (ctx.s * 1.1).max(0.6),
            color,
        );
        // 点估计方块：有权重列时边长 ∝ sqrt(权重)
        let sz = (row_h * 0.5 * (w / w_max).sqrt() as f32).clamp(ctx.px(3.0), row_h * 0.72);
        let rect = crate::plot::scene::Rect::new(xe - sz / 2.0, cy - sz / 2.0, sz, sz);
        ctx.check_rect("森林图点估计", rect, ctx.s * 0.5);
        ctx.scene.rect(rect, Some(color), None);
        // 文字碰撞检查用的虚拟外框（端帽不越界已有 check_rect 兜底）
        ctx.check_pt("森林图 CI 端点", Pt::new(xlo, cy), ctx.s * 0.5);
        ctx.check_pt("森林图 CI 端点", Pt::new(xhi, cy), ctx.s * 0.5);
    }

    draw_axes(ctx, &f, &x_title, "");
    (f.x, f.y)
}
