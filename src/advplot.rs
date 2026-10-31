//! 高级统计图：把 plotters-statistical 的 series 接进析光的绘图管线。
//!
//! # 为什么选 plotters-statistical 而不是 ruviz / starsight
//!
//! | 库 | 引擎 | 能否接 |
//! |---|---|---|
//! | **plotters-statistical 0.2** | plotters 原生 Series | ✅ 同一引擎 |
//! | ruviz 0.14 | 自带渲染引擎 | ✗ 拿不到像素缓冲，中文未知 |
//! | starsight 0.3 | tiny-skia / SVG | ✗ 同上 |
//!
//! 只有它是 plotters 的 Series，走 `chart.draw_series(...)`，与 plotters
//! 自带的 `LineSeries` 用法一致。因此中文字体注册、位图预览缓冲、
//! SVG 导出三条管线完全不受影响。
//!
//! # 关键设计：**不调用 `configure_mesh().draw()`**
//!
//! plotters 默认 mesh 是「四面全框 + 向外刻度 + 默认 sans-serif 字体」，
//! 与析光的学术外观（L 型框 + 向内刻度 + 宋体/ Times New Roman）冲突。
//!
//! 这里的做法是：
//! 1. 用 `ChartBuilder` 建系，**只为拿到 `ChartContext` 给库的 series 用**；
//! 2. 用 `margin_*` 把绘图区精确钉到指定像素矩形；
//! 3. **不画 mesh**，坐标轴、刻度、标签全部自己画（复用 plot.rs 的排版规范）。
//!
//! 于是「库的统计计算 + 我们的学术外观」两者兼得：既不是自己瞎画箱线/小提琴，
//! 也不会为了用库而退回 plotters 的默认外观。
//!
//! # 覆盖范围
//!
//! 库里 13 种，这里做了 5 种。其余需要数据模型根本没有的字段
//! （ROC/PR/校准/增益要二分类标签、残差图要成对拟合值、相关矩阵要多变量矩阵），
//! 不做假开关。

use plotters::prelude::*;
use plotters::coord::Shift;
use plotters::drawing::DrawingArea;

use crate::chartkind::ChartKind;
use crate::fonts;
use crate::palette::CHART_FONT;
use crate::plot::{blit_rotated_ccw, raster_text, ChartView};

/// 是否由本模块渲染（draw_chart 开头据此提前返回）。
pub fn handles(view: &ChartView) -> bool {
    matches!(
        view.kind,
        ChartKind::Box | ChartKind::Violin | ChartKind::Heatmap | ChartKind::Ecdf | ChartKind::QqPlot
    )
}

/// 绘图区矩形（像素）。用 margin_* 精确钉住，绘图区之外留给自己画轴。
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

pub fn render<DB: DrawingBackend>(
    view: &ChartView,
    root: &DrawingArea<DB, Shift>,
) -> Result<(), String> {
    let (w, h) = root.dim_in_pixel();
    if w < 80 || h < 80 {
        return Err("画布过小".into());
    }
    let sc = view.scale.max(0.5) as f64;
    let fs = CHART_FONT as f64 * sc;
    let ink = RGBColor(26, 26, 26);

    // 画布恒白：科研图底色不跟随 UI 深浅模式
    root.fill(&WHITE).map_err(|e| format!("{:?}", e))?;

    // 留白：上给标题，下给 X 轴标题，左给 Y 轴标题 + 刻度数字，右给图例
    let r = Rect {
        left: (58.0 * sc) as i32 + 8,
        top: (26.0 * sc) as i32 + (if view.title.is_empty() { 0.0 } else { fs * 1.8 } as i32),
        right: w as i32 - 14,
        bottom: h as i32 - (46.0 * sc) as i32,
    };
    if r.right <= r.left + 10 || r.bottom <= r.top + 10 {
        return Err("绘图区过小".into());
    }

    match view.kind {
        ChartKind::Box | ChartKind::Violin => draw_groups(root, view, &r, fs, ink),
        ChartKind::Heatmap => draw_heatmap(root, view, &r, fs, ink),
        ChartKind::Ecdf => draw_ecdf(root, view, &r, fs, ink),
        ChartKind::QqPlot => draw_qq(root, view, &r, fs, ink),
        _ => Err("不是高级图型".into()),
    }
}

// ── 箱线图 / 小提琴图 ───────────────────────────────────────────────────────

/// 两者都是「分类 X 轴 + 数值 Y 轴」，共用同一套坐标系与轴绘制。
fn draw_groups<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    view: &ChartView,
    r: &Rect,
    fs: f64,
    ink: RGBColor,
) -> Result<(), String> {
    let groups: Vec<(f64, Vec<f64>)> = view
        .groups
        .iter()
        .enumerate()
        .filter(|(_, g)| !g.values.is_empty())
        .map(|(i, g)| (i as f64 + 1.0, g.values.clone()))
        .collect();
    if groups.is_empty() {
        return Err("没有可用的数据组".into());
    }

    let (lo, hi) = span(&groups.iter().flat_map(|g| g.1.iter().copied()).collect::<Vec<_>>());
    // 上留 8% 余量给显著性标注，下从 0 起（学术惯例）
    let pad = (hi - lo).max(1e-9) * 0.08;
    let y_lo = if view.bar_zero { 0.0f64.min(lo) } else { lo - pad };
    let y_hi = hi + pad;
    let x_lo = 0.5;
    let x_hi = groups.len() as f64 + 0.5;

    let mut chart = ChartBuilder::on(root)
        .margin_top(r.top)
        .margin_bottom((root.dim_in_pixel().1 as i32) - r.bottom)
        .margin_left(r.left)
        .margin_right((root.dim_in_pixel().0 as i32) - r.right)
        .build_cartesian_2d(x_lo..x_hi, y_lo..y_hi)
        .map_err(|e| format!("{:?}", e))?;

    if view.kind == ChartKind::Box {
        chart
            .draw_series(
                plotters_statistical::BoxPlotSeries::from_samples(groups.clone())
                    .map_err(|e| format!("{:?}", e))?,
            )
            .map_err(|e| format!("{:?}", e))?;
    } else {
        chart
            .draw_series(
                plotters_statistical::ViolinPlotSeries::from_samples(groups.clone())
                    .map_err(|e| format!("{:?}", e))?
                    // box_style == 0 → 带须含离群值，内嵌盒与之对应
                    .show_box(view.box_style == 0),
            )
            .map_err(|e| format!("{:?}", e))?;
    }
    drop(chart);

    // ── 自己画轴（不调用 configure_mesh）──
    let ticks = nice_ticks(y_lo, y_hi, 5);
    let names: Vec<String> = view
        .groups
        .iter()
        .filter(|g| !g.values.is_empty())
        .map(|g| g.name.clone())
        .collect();
    let cats: Vec<(f64, String)> = (0..names.len())
        .map(|i| ((i + 1) as f64, names[i].clone()))
        .collect();

    frame(root, r, ink)?;
    y_axis(root, view, r, y_lo, y_hi, &ticks, fs, ink)?;
    x_axis_cat(root, r, x_lo, x_hi, &cats, fs, ink)?;
    titles(root, view, r, fs, ink)?;
    root.present().map_err(|e| format!("{:?}", e))
}

// ── 热图 ────────────────────────────────────────────────────────────────────

/// 行 = 组，列 = 值域分箱，值 = 该组落入该箱的频数。
fn draw_heatmap<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    view: &ChartView,
    r: &Rect,
    fs: f64,
    ink: RGBColor,
) -> Result<(), String> {
    let bins = view.bins.clamp(3, 24);
    let (mat, xlab, ylab) = bin_matrix(view, bins);
    if mat.is_empty() {
        return Err("没有可用的数据组".into());
    }
    let ncol = mat[0].len() as f64;
    let nrow = mat.len() as f64;

    // 色阶取当前配色方案的首末色，与柱状图观感一致。
    // 低频用浅色（接近白底）、高频用深色，方向符合「数量越多越重」的直觉。
    //
    // 注意：`plotters_statistical::Heatmap` 只实现 `Drawable`（整体一次画出），
    // 不是 `IntoIterator`，无法交给 `draw_series`，且会带上库默认的文字/边框样式。
    // 分箱与单元格绘制因此由我们自己完成；配色仍沿用当前方案，保证观感一致。
    let (c0, c1) = heat_ends(view.colormap);
    let cmap = LinearRamp::new(c0, c1);
    let cmax = mat.iter().flat_map(|r| r.iter().copied()).fold(0.0f64, f64::max);

    let cw = (r.right - r.left) as f64 / ncol;
    let chh = (r.bottom - r.top) as f64 / nrow;
    for (ri, row) in mat.iter().enumerate() {
        for (ci, v) in row.iter().copied().enumerate() {
            let t = if cmax > 0.0 { v / cmax } else { 0.0 };
            let c = cmap.at(t);
            let x0 = r.left + (ci as f64 * cw) as i32;
            let y0 = r.bottom - ((ri as f64 + 1.0) * chh) as i32;
            let x1 = r.left + ((ci as f64 + 1.0) * cw) as i32;
            let y1 = r.bottom - (ri as f64 * chh) as i32;
            root.draw(&Rectangle::new(
                [(x0, y0), (x1.max(x0 + 1), y1.max(y0 + 1))],
                ShapeStyle {
                    color: c.into(),
                    filled: true,
                    stroke_width: 0,
                },
            ))
            .map_err(|e| format!("{:?}", e))?;
        }
    }

    frame(root, r, ink)?;
    // Y 轴是组名（自上而下），X 轴是分箱区间
    y_axis_cat(root, r, &ylab, fs, ink)?;
    x_axis_cat(
        root,
        r,
        0.0,
        ncol,
        &(0..xlab.len()).map(|i| (i as f64 + 0.5, xlab[i].clone())).collect::<Vec<_>>(),
        fs,
        ink,
    )?;
    titles(root, view, r, fs, ink)?;
    root.present().map_err(|e| format!("{:?}", e))
}

/// 组 × 分箱频数矩阵。返回 (矩阵, X 标签, Y 标签)。
fn bin_matrix(view: &ChartView, bins: usize) -> (Vec<Vec<f64>>, Vec<String>, Vec<String>) {
    let gs: Vec<&Vec<f64>> = view
        .groups
        .iter()
        .filter(|g| !g.values.is_empty())
        .map(|g| &g.values)
        .collect();
    if gs.is_empty() {
        return (Vec::new(), Vec::new(), Vec::new());
    }
    let all: Vec<f64> = gs.iter().flat_map(|v| v.iter().copied()).collect();
    let (lo, hi) = span(&all);
    let width = ((hi - lo) / bins as f64).max(1e-9);
    let mut mat = Vec::new();
    for v in &gs {
        let mut row = vec![0.0f64; bins];
        for x in v.iter().copied() {
            let k = ((x - lo) / width).floor() as i64;
            let k = k.clamp(0, bins as i64 - 1) as usize;
            row[k] += 1.0;
        }
        mat.push(row);
    }
    let xlab = (0..bins)
        .map(|i| format!("{:.3}", lo + i as f64 * width))
        .collect();
    let ylab = view
        .groups
        .iter()
        .filter(|g| !g.values.is_empty())
        .map(|g| g.name.clone())
        .collect();
    (mat, xlab, ylab)
}

// ── ECDF / QQ ───────────────────────────────────────────────────────────────

fn draw_ecdf<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    view: &ChartView,
    r: &Rect,
    fs: f64,
    ink: RGBColor,
) -> Result<(), String> {
    let data = first_values(view)?;
    let (lo, hi) = span(&data);
    let pad = (hi - lo).max(1e-9) * 0.08;

    // ECDF 的阶梯点自己算：库返回的 `EcdfData` 结构与我们的绘制不互通，
    // 而 ECDF 本身只是一步排序 + 累计计数，没有复杂统计，自算更可控。
    let pts = ecdf_points(&data);
    step_path(root, r, lo - pad, hi + pad, 0.0, 1.0, &pts, ink)?;

    frame(root, r, ink)?;
    y_axis(root, view, r, 0.0, 1.0, &[0.0, 0.25, 0.5, 0.75, 1.0], fs, ink)?;
    let xt = nice_ticks(lo - pad, hi + pad, 5);
    x_axis_num(root, r, lo - pad, hi + pad, &xt, fs, ink)?;
    titles(root, view, r, fs, ink)?;
    root.present().map_err(|e| format!("{:?}", e))
}

fn draw_qq<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    view: &ChartView,
    r: &Rect,
    fs: f64,
    ink: RGBColor,
) -> Result<(), String> {
    let data = first_values(view)?;
    let (lo, hi) = span(&data);
    // QQ 的两轴都是分位数：正态理论分位数会明显超出样本极值，
    // 所以余量要给到 50%，否则参考线两端会被裁掉。
    let pad = (hi - lo).max(1e-9) * 0.5;

    // `QqPlot` 同样只实现 `Drawable`。理论分位数用库的 `norm_ppf`
    // （比自写近似更准），点与参考线由自己画。
    let pts = qq_points(&data);
    // 参考线：y = mean + sd * x（位置-尺度族的稳健拟合线）
    let (m, sd) = mean_sd(&data);
    ref_line(root, r, lo - pad, hi + pad, m, sd, ink)?;
    for (x, y) in &pts {
        let px = r.left + ((x - (lo - pad)) / ((hi + pad) - (lo - pad)).max(1e-9) * (r.right - r.left) as f64) as i32;
        let py = r.bottom - ((y - (lo - pad)) / ((hi + pad) - (lo - pad)).max(1e-9) * (r.bottom - r.top) as f64) as i32;
        let c = crate::colormap::color_of(view.colormap, 0);
        root.draw(&Circle::new(
            (px, py),
            3,
            ShapeStyle {
                color: RGBColor(c[0], c[1], c[2]).into(),
                filled: true,
                stroke_width: 1,
            },
        ))
        .map_err(|e| format!("{:?}", e))?;
    }

    frame(root, r, ink)?;
    let t = nice_ticks(lo - pad, hi + pad, 5);
    y_axis(root, view, r, lo - pad, hi + pad, &t, fs, ink)?;
    x_axis_num(root, r, lo - pad, hi + pad, &t, fs, ink)?;
    titles(root, view, r, fs, ink)?;
    root.present().map_err(|e| format!("{:?}", e))
}

fn first_values(view: &ChartView) -> Result<Vec<f64>, String> {
    view.groups
        .iter()
        .find(|g| g.values.len() >= 2)
        .map(|g| g.values.clone())
        .ok_or_else(|| "需要至少一组、且每组不少于 2 个数值".to_string())
}

// ── 坐标轴：全部自己画，不依赖 plotters mesh ────────────────────────────────

/// L 型框：只画左轴 + 下轴。四面全框会显得像 Excel。
fn frame<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    r: &Rect,
    ink: RGBColor,
) -> Result<(), String> {
    let st = ShapeStyle {
        color: ink.into(),
        filled: false,
        stroke_width: 1,
    };
    root.draw(&PathElement::new(
        vec![(r.left, r.top), (r.left, r.bottom), (r.right, r.bottom)],
        st,
    ))
    .map_err(|e| format!("{:?}", e))
}

/// 数值 Y 轴：向内刻度 + 刻度数字 + 旋转 90° 的轴标题。
fn y_axis<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    view: &ChartView,
    r: &Rect,
    lo: f64,
    hi: f64,
    ticks: &[f64],
    fs: f64,
    ink: RGBColor,
) -> Result<(), String> {
    let st = ShapeStyle {
        color: ink.into(),
        filled: false,
        stroke_width: 1,
    };
    let h = (r.bottom - r.top) as f64;
    for t in ticks {
        if *t < lo || *t > hi {
            continue;
        }
        let y = r.bottom - ((t - lo) / (hi - lo).max(1e-9) * h) as i32;
        // 向内刻度
        root.draw(&PathElement::new(vec![(r.left, y), (r.left + 4, y)], st.clone()))
            .map_err(|e| format!("{:?}", e))?;
        let s = crate::plot::fmt_tick_full(*t, (hi - lo) / 5.0);
        let fd: FontDesc = (fonts::font_for(&s), fs).into_font();
        let w = crate::plot::text_w(&s, fs);
        root.draw(&Text::new(s, (r.left - 8 - w as i32, y - (fs * 0.5) as i32), fd.color(&ink)))
            .map_err(|e| format!("{:?}", e))?;
    }
    // Y 轴标题：真 90° 旋转（复用 plot.rs 的离屏栅格化）
    let yname = axis_name(&view.y_name, &view.y_unit, &view.y_sym, view.unit_style);
    if !yname.is_empty() {
        if let Some((buf, tw, th)) = raster_text(&yname, fs, fonts::font_for(&yname), ink, false) {
            let cx = (r.left as f64 - 34.0) as i32;
            let cy = ((r.top + r.bottom) / 2) as i32;
            blit_rotated_ccw(root, &buf, tw, th, cx, cy).ok();
        }
    }
    Ok(())
}

/// 分类 Y 轴（热图的行名）。
fn y_axis_cat<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    r: &Rect,
    labels: &[String],
    fs: f64,
    ink: RGBColor,
) -> Result<(), String> {
    let n = labels.len().max(1) as f64;
    let h = (r.bottom - r.top) as f64 / n;
    for (i, s) in labels.iter().enumerate() {
        let y = r.bottom - ((i as f64 + 0.5) * h) as i32;
        let fd: FontDesc = (fonts::font_for(s), fs).into_font();
        let w = crate::plot::text_w(s, fs);
        root.draw(&Text::new(
            s.clone(),
            (r.left - 8 - w as i32, y - (fs * 0.5) as i32),
            fd.color(&ink),
        ))
        .map_err(|e| format!("{:?}", e))?;
    }
    Ok(())
}

/// 分类 X 轴：刻度在类中心，标签是组名（可以做垂直省略）。
fn x_axis_cat<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    r: &Rect,
    lo: f64,
    hi: f64,
    cats: &[(f64, String)],
    fs: f64,
    ink: RGBColor,
) -> Result<(), String> {
    let st = ShapeStyle {
        color: ink.into(),
        filled: false,
        stroke_width: 1,
    };
    let w = (r.right - r.left) as f64;
    for (p, s) in cats {
        let x = r.left + ((p - lo) / (hi - lo).max(1e-9) * w) as i32;
        root.draw(&PathElement::new(vec![(x, r.bottom), (x, r.bottom - 4)], st.clone()))
            .map_err(|e| format!("{:?}", e))?;
        let tw = crate::plot::text_w(s, fs);
        let fd: FontDesc = (fonts::font_for(s), fs).into_font();
        root.draw(&Text::new(
            s.clone(),
            (x - (tw * 0.5) as i32, r.bottom + 6),
            fd.color(&ink),
        ))
        .map_err(|e| format!("{:?}", e))?;
    }
    Ok(())
}

/// 数值 X 轴。
fn x_axis_num<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    r: &Rect,
    lo: f64,
    hi: f64,
    ticks: &[f64],
    fs: f64,
    ink: RGBColor,
) -> Result<(), String> {
    let cats: Vec<(f64, String)> = ticks
        .iter()
        .map(|t| (*t, crate::plot::fmt_tick_full(*t, (hi - lo) / 5.0)))
        .collect();
    x_axis_cat(root, r, lo, hi, &cats, fs, ink)
}

/// 标题（顶部居中）与 X 轴标题（底部居中）。
fn titles<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    view: &ChartView,
    r: &Rect,
    fs: f64,
    ink: RGBColor,
) -> Result<(), String> {
    if !view.title.is_empty() {
        let tfs = fs + 2.0;
        let tw = crate::plot::text_w(&view.title, tfs);
        let x = ((r.left + r.right) / 2) as i32 - (tw * 0.5) as i32;
        let mut fd: FontDesc = (fonts::font_for(&view.title), tfs).into_font();
        if view.title_bold {
            fd = fd.style(FontStyle::Bold);
        }
        root.draw(&Text::new(view.title.clone(), (x, 6), fd.color(&ink)))
            .map_err(|e| format!("{:?}", e))?;
    }
    let xname = axis_name(&view.x_name, &view.x_unit, &view.x_sym, view.unit_style);
    if !xname.is_empty() {
        let tw = crate::plot::text_w(&xname, fs);
        let x = ((r.left + r.right) / 2) as i32 - (tw * 0.5) as i32;
        let fd: FontDesc = (fonts::font_for(&xname), fs).into_font();
        root.draw(&Text::new(xname, (x, r.bottom + (fs * 1.9) as i32), fd.color(&ink)))
            .map_err(|e| format!("{:?}", e))?;
    }
    Ok(())
}

/// 轴标题排版：名称（符号斜体）+ 单位，四种格式见 plot.rs §3.2。
fn axis_name(
    name: &str,
    unit: &str,
    _sym: &str,
    style: crate::plot::UnitStyle,
) -> String {
    if unit.is_empty() {
        return name.to_string();
    }
    match style {
        crate::plot::UnitStyle::Paren => format!("{} ({})", name, unit),
        crate::plot::UnitStyle::Slash => format!("{} / {}", name, unit),
        crate::plot::UnitStyle::Comma => format!("{}, {}", name, unit),
        crate::plot::UnitStyle::None => name.to_string(),
    }
}

// ── 数值辅助 ────────────────────────────────────────────────────────────────

fn span(v: &[f64]) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for x in v.iter().copied() {
        if x.is_finite() {
            lo = lo.min(x);
            hi = hi.max(x);
        }
    }
    if !lo.is_finite() {
        (0.0, 1.0)
    } else if (hi - lo).abs() < 1e-12 {
        (lo - 0.5, hi + 0.5)
    } else {
        (lo, hi)
    }
}

/// 取整刻度：步长落到 1/2/5 × 10^n。
fn nice_ticks(lo: f64, hi: f64, want: usize) -> Vec<f64> {
    if !lo.is_finite() || !hi.is_finite() || hi <= lo {
        return vec![lo, hi];
    }
    let want = want.max(2);
    let raw = (hi - lo) / want as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let step = if norm < 1.5 {
        1.0
    } else if norm < 3.0 {
        2.0
    } else if norm < 7.0 {
        5.0
    } else {
        10.0
    } * mag;
    let mut v = Vec::new();
    let mut x = (lo / step).ceil() * step;
    while x <= hi + step * 1e-9 && v.len() < 64 {
        if x >= lo - step * 1e-9 {
            v.push(x);
        }
        x += step;
    }
    if v.is_empty() {
        v.push(lo);
    }
    v
}


/// 热图色阶的两端色：低频取方案首色（浅），高频取方案末色（深）。
fn heat_ends(map: usize) -> ([u8; 4], [u8; 4]) {
    let cm = &crate::colormap::COLORMAPS[map.min(crate::colormap::COLORMAPS.len() - 1)];
    let n = cm.colors.len();
    (cm.colors[0], cm.colors[n - 1])
}

/// 线性色阶（RGB 空间插值，与 `GradientColorMap` 的策略一致）。
struct LinearRamp {
    a: [u8; 4],
    b: [u8; 4],
}

impl LinearRamp {
    fn new(a: [u8; 4], b: [u8; 4]) -> Self {
        Self { a, b }
    }
    fn at(&self, t: f64) -> RGBColor {
        let t = t.clamp(0.0, 1.0) as f32;
        RGBColor(
            (self.a[0] as f32 + (self.b[0] as f32 - self.a[0] as f32) * t) as u8,
            (self.a[1] as f32 + (self.b[1] as f32 - self.a[1] as f32) * t) as u8,
            (self.a[2] as f32 + (self.b[2] as f32 - self.a[2] as f32) * t) as u8,
        )
    }
}

/// 阶梯折线（ECDF）。每个样本点处跳变一次。
fn step_path<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    r: &Rect,
    x_lo: f64,
    x_hi: f64,
    y_lo: f64,
    y_hi: f64,
    pts: &[(f64, f64)],
    ink: RGBColor,
) -> Result<(), String> {
    if pts.len() < 2 {
        return Ok(());
    }
    let w = (r.right - r.left) as f64;
    let h = (r.bottom - r.top) as f64;
    let px = |x: f64| r.left + ((x - x_lo) / (x_hi - x_lo).max(1e-9) * w) as i32;
    let py = |y: f64| r.bottom - ((y - y_lo) / (y_hi - y_lo).max(1e-9) * h) as i32;
    let mut path = Vec::new();
    path.push((px(pts[0].0), py(y_lo)));
    for (i, (x, y)) in pts.iter().enumerate() {
        // 竖直跳变到本层
        path.push((px(*x), py(*y)));
        // 水平延伸到下一个样本点，形成阶梯
        if i + 1 < pts.len() {
            path.push((px(pts[i + 1].0), py(*y)));
        }
    }
    // 末端水平拖到右边界
    if let Some(&(_, ly)) = path.last() {
        path.push((px(x_hi), ly));
    }
    root.draw(&PathElement::new(
        path,
        ShapeStyle {
            color: ink.into(),
            filled: false,
            stroke_width: 2,
        },
    ))
    .map_err(|e| format!("{:?}", e))
}

/// QQ 图的点：(理论分位数, 样本分位数)。
fn qq_points(data: &[f64]) -> Vec<(f64, f64)> {
    let mut v = data.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    if n < 2 {
        return Vec::new();
    }
    v.iter()
        .enumerate()
        .map(|(i, y)| {
            // (i - 0.5) / n 是标准做法：让秩的范围关于 0.5 对称，
            // 避免最后一个点落在 p=1 上（norm_ppf(1) = +∞）。
            let p = (i as f64 + 0.5) / n as f64;
            (plotters_statistical::stats::normal::norm_ppf(p), *y)
        })
        .collect()
}

fn mean_sd(v: &[f64]) -> (f64, f64) {
    let n = v.len() as f64;
    if n < 1.0 {
        return (0.0, 1.0);
    }
    let m = v.iter().sum::<f64>() / n;
    let var = v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (n - 1.0).max(1.0);
    (m, var.sqrt())
}

/// QQ 参考线 y = m + sd * x，两端按可视区裁剪。
fn ref_line<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    r: &Rect,
    lo: f64,
    hi: f64,
    m: f64,
    sd: f64,
    ink: RGBColor,
) -> Result<(), String> {
    let w = (r.right - r.left) as f64;
    let h = (r.bottom - r.top) as f64;
    let px = |x: f64| r.left + ((x - lo) / (hi - lo).max(1e-9) * w) as i32;
    let py = |y: f64| r.bottom - ((y - lo) / (hi - lo).max(1e-9) * h) as i32;
    root.draw(&PathElement::new(
        vec![(px(lo), py(m + sd * lo)), (px(hi), py(m + sd * hi))],
        ShapeStyle {
            color: ink.into(),
            filled: false,
            stroke_width: 1,
        },
    ))
    .map_err(|e| format!("{:?}", e))
}

/// ECDF 阶梯点：排序后按累计比例递增。
fn ecdf_points(data: &[f64]) -> Vec<(f64, f64)> {
    let mut v: Vec<f64> = data.iter().copied().filter(|x| x.is_finite()).collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len() as f64;
    if n < 1.0 {
        return Vec::new();
    }
    v.iter()
        .enumerate()
        .map(|(i, x)| (*x, (i as f64 + 1.0) / n))
        .collect()
}
