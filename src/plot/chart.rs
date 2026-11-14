//! 图表构建：版式 → 坐标轴 → 数据系列 → 图例 → 显著性标注。
//!
//! 输出是一份 `Scene`（纯路径），屏幕/PNG/SVG/PDF 共用。

use std::f32::consts::PI;

use crate::core::color::Rgba;
use crate::core::graph::{ChartKind, ErrorKind, Family, GraphSpec, LegendPos, Marker};
use crate::core::model::{format_number, ColRole, DataTable, TableKind};
use crate::core::stats;
use crate::plot::axis::{Axis, Tick};
use crate::plot::clip;
use crate::plot::scene::{LineCap, LineJoin, Path, Pt, Rect, Scene, Stroke};
use crate::plot::text::{FontBook, HAlign, VAlign};
use crate::plot::theme::Theme;

pub const WHITE: Rgba = Rgba::WHITE;

#[derive(Clone, Debug)]
pub struct Hit {
    pub at: Pt,
    pub title: String,
    pub lines: Vec<String>,
}

/// 标题槽位：画布上点击标题直接编辑，写回 `GraphSpec` 的对应字段。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TitleSlot {
    /// 图标题（spec.title）
    Main,
    /// X 轴标题（spec.x_title）
    X,
    /// Y 轴标题（spec.y_title）
    Y,
}

#[derive(Clone, Debug)]
pub struct Render {
    pub scene: Scene,
    pub plot: Rect,
    pub x: Axis,
    pub y: Axis,
    pub hits: Vec<Hit>,
    pub width: u32,
    pub height: u32,
    pub px_per_pt: f32,
    /// 几何自检发现的问题（越出绘图区的数据图元）。
    ///
    /// 对应 Nature `figure-style` §9.1 的 render-then-verify：光看渲染图像
    /// 很难发现「点飞到框外 2px」这类错误，所以在**画的时候**就记录。
    /// 正常应为空；`examples/gallery.rs` 与 `plot::qa` 的测试都会断言这一点。
    pub violations: Vec<String>,
    /// 标题文字的外框（场景像素），供画布点击编辑定位。
    pub title_boxes: Vec<(TitleSlot, Rect)>,
    /// 图例外框（场景像素），供画布拖拽定位。None = 未显示图例。
    pub legend_box: Option<Rect>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegendKind {
    Square,
    Marker,
    Line,
    LineMarker,
    /// 虚线 / 点划线：多组时序与 ECDF 的「第二编码」
    Dashed,
    DashDot,
}

/// 多组曲线/ECDF 的线型表：颜色不能是唯一编码（规范 G5 / A7 第 3 条）。
///
/// 只给「同一坐标系里多条线」用；柱、点、面积不适用。
pub(crate) fn series_dash(i: usize) -> &'static [f32] {
    match i % 4 {
        0 => &[],
        1 => &[5.0, 3.0],
        2 => &[2.0, 2.5],
        _ => &[7.0, 3.0, 2.0, 3.0],
    }
}

pub(crate) fn legend_kind_for_line(i: usize, with_marker: bool) -> LegendKind {
    if with_marker {
        return LegendKind::LineMarker;
    }
    match i % 4 {
        1 => LegendKind::Dashed,
        2 | 3 => LegendKind::DashDot,
        _ => LegendKind::Line,
    }
}

#[derive(Clone)]
pub(crate) struct LegendItem {
    pub label: String,
    pub color: Rgba,
    pub kind: LegendKind,
}

pub(crate) struct Ctx<'a> {
    pub spec: &'a GraphSpec,
    pub theme: Theme,
    pub fonts: &'a FontBook,
    pub scene: Scene,
    pub s: f32,
    pub w: f32,
    pub h: f32,
    pub colors: Vec<Rgba>,
    pub hits: Vec<Hit>,
    pub legend: Vec<LegendItem>,
    pub plot: Rect,
    /// 几何自检记录；`plot` 尚未定稿时（面积为零）不检查。
    pub viol: Vec<String>,
    /// 当前是否在画绘图区内的数据图元（图例、轴标签等不参与检查）。
    pub in_data: bool,
    /// 底部图注统计行（G4/E2-5：检验全名、双侧、校正、效应量、95% CI、log-rank/HR）。
    /// 有内容时布局为其让出一行，画在 x 轴标题下方。
    pub note: Option<String>,
    /// 文字外框记录（文本碰撞检测）。只记轴对齐的：旋转文字的 AABB 会把
    /// 倾斜排布的相邻标签误报成重叠，而它们本来就是靠倾斜避让的。
    pub texts: Vec<(String, Rect)>,
    /// 标题外框（画布点击编辑用）。
    pub title_boxes: Vec<(TitleSlot, Rect)>,
    /// 图例外框（画布拖拽用）。
    pub legend_box: Option<Rect>,
}

impl<'a> Ctx<'a> {
    pub fn px(&self, v: f32) -> f32 {
        v * self.s
    }

    pub fn measure(&self, text: &str, size_pt: f32) -> f32 {
        self.fonts
            .measure(text, self.s * size_pt, self.theme.family)
    }

    /// 设图注统计行——**受「图内标注」开关控制**。
    ///
    /// 关掉标注时不设，布局也就不为它留那一行（否则会留下一条空白）。
    pub fn set_note(&mut self, text: impl Into<Option<String>>) {
        if self.spec.annotations_visible {
            self.note = text.into();
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn text(
        &mut self,
        text: &str,
        at: Pt,
        size_pt: f32,
        color: Rgba,
        h: HAlign,
        v: VAlign,
        rotation: f32,
        bold: bool,
    ) -> f32 {
        let size = self.s * size_pt;
        let fonts = self.fonts;
        // 文本碰撞检测：记下这行文字的近似外框（§9.1 的「标签互相压」）。
        if !text.is_empty() && rotation.abs() < 0.01 {
            let w = self.measure(text, size_pt);
            let lh = fonts.line_height(size, self.theme.family);
            let x0 = match h {
                HAlign::Left => at.x,
                HAlign::Center => at.x - w / 2.0,
                HAlign::Right => at.x - w,
            };
            let y0 = match v {
                VAlign::Top => at.y,
                VAlign::Middle => at.y - lh / 2.0,
                VAlign::Baseline | VAlign::Bottom => at.y - lh,
            };
            self.texts
                .push((text.to_string(), Rect::new(x0, y0, w, lh)));
        }
        fonts.draw(
            &mut self.scene,
            text,
            at,
            size,
            self.theme.family,
            color,
            h,
            v,
            rotation,
            bold,
        )
    }

    /// 画「标题类」文字并记录外框（画布点击编辑用）。
    ///
    /// 外框按锚点对齐推算；90° 旋转（Y 轴标题）时宽高互换——编辑框要落在
    /// 视觉上的那行字上，而不是未旋转的假想框。
    pub(crate) fn text_slot(
        &mut self,
        slot: TitleSlot,
        text: &str,
        at: Pt,
        size_pt: f32,
        h: HAlign,
        v: VAlign,
        rotation: f32,
        bold: bool,
    ) {
        if text.is_empty() {
            return;
        }
        let size = self.s * size_pt;
        let w = self.measure(text, size_pt);
        let lh = self.fonts.line_height(size, self.theme.family);
        let vert = (rotation.abs() - std::f32::consts::FRAC_PI_2).abs() < 0.25;
        let (bw, bh) = if vert { (lh, w) } else { (w, lh) };
        let cx = match h {
            HAlign::Left => at.x + w / 2.0,
            HAlign::Center => at.x,
            HAlign::Right => at.x - w / 2.0,
        };
        let cy = match v {
            VAlign::Top => at.y + lh / 2.0,
            VAlign::Middle => at.y,
            VAlign::Baseline | VAlign::Bottom => at.y - lh / 2.0,
        };
        self.title_boxes
            .push((slot, Rect::new(cx - bw / 2.0, cy - bh / 2.0, bw, bh)));
        self.text(text, at, size_pt, self.theme.ink, h, v, rotation, bold);
    }

    pub fn line(&mut self, a: Pt, b: Pt, width_pt: f32, color: Rgba, dash: &[f32]) {
        let mut st = Stroke::new((self.s * width_pt).max(0.35), color);
        st.cap = LineCap::Round;
        if !dash.is_empty() {
            st.dash = dash.iter().map(|d| d * self.s).collect();
        }
        self.scene.line(a, b, st);
    }

    /// 坐标轴、边框这类**端点必须精确落位**的线。
    ///
    /// `line()` 用圆头端帽，端点会多出半个线宽，四条边框拼起来每个角都会
    /// 冒出一个小凸起；坐标轴伸出框外也是同理。这里改成平头端帽。
    pub fn spine(&mut self, a: Pt, b: Pt, width_pt: f32, color: Rgba) {
        let mut st = Stroke::new((self.s * width_pt).max(0.35), color);
        st.cap = LineCap::Butt;
        self.scene.line(a, b, st);
    }

    pub fn color(&self, i: usize) -> Rgba {
        self.colors[i % self.colors.len()]
    }

    // ---- 几何自检（Nature figure-style §9.1） ----
    //
    // 只检查「数据图元」：点、柱、面、线。轴、刻度、轴标题本来就该在绘图区
    // 之外，不参与检查。容差取半个描边宽 + 1px：描边是居中绘制的，
    // 一条压在框线上的边看起来仍在框内。

    fn qa_ready(&self) -> bool {
        self.in_data && self.plot.w > 1.0 && self.plot.h > 1.0
    }

    /// 进入「绘制数据图元」阶段：此后图元位置参与几何自检。
    pub fn begin_data(&mut self, f: &Frame) {
        self.plot = f.plot;
        self.in_data = true;
    }

    pub fn check_pt(&mut self, what: &str, p: Pt, tol: f32) {
        if !self.qa_ready() {
            return;
        }
        if !p.x.is_finite() || !p.y.is_finite() {
            self.viol.push(format!("{what}: 坐标为 NaN/Inf"));
            return;
        }
        let r = self.plot;
        let over = (r.x - p.x)
            .max(p.x - r.right())
            .max(r.y - p.y)
            .max(p.y - r.bottom());
        if over > tol + 1.0 {
            self.viol.push(format!(
                "{what} 越出绘图区 {:.1}px：点 ({:.1}, {:.1})，绘图区 x {:.1}..{:.1} / y {:.1}..{:.1}",
                over,
                p.x,
                p.y,
                r.x,
                r.right(),
                r.y,
                r.bottom()
            ));
        }
    }

    pub fn check_rect(&mut self, what: &str, rect: Rect, tol: f32) {
        if !self.qa_ready() {
            return;
        }
        let r = self.plot;
        let over = (r.x - rect.x)
            .max(rect.right() - r.right())
            .max(r.y - rect.y)
            .max(rect.bottom() - r.bottom());
        if over > tol + 1.0 {
            self.viol.push(format!(
                "{what} 越出绘图区 {:.1}px：矩形 x {:.1}..{:.1} / y {:.1}..{:.1}，绘图区 x {:.1}..{:.1} / y {:.1}..{:.1}",
                over,
                rect.x,
                rect.right(),
                rect.y,
                rect.bottom(),
                r.x,
                r.right(),
                r.y,
                r.bottom()
            ));
        }
    }

    pub fn check_poly(&mut self, what: &str, pts: &[Pt]) {
        for p in pts {
            self.check_pt(what, *p, 0.5);
        }
    }
}

/// 主入口：给定图表规格与数据表，生成场景。
pub fn render(
    spec: &GraphSpec,
    table: &DataTable,
    width_pt: f32,
    height_pt: f32,
    px_per_pt: f32,
    fonts: &FontBook,
) -> Render {
    let theme = Theme::for_id(spec.theme);
    let colors = palette_colors(spec);
    let s = px_per_pt.max(0.05);
    let scene = Scene::new(width_pt * s, height_pt * s, WHITE);
    let mut ctx = Ctx {
        spec,
        theme,
        fonts,
        scene,
        s,
        w: width_pt * s,
        h: height_pt * s,
        colors,
        hits: Vec::new(),
        legend: Vec::new(),
        plot: Rect::default(),
        viol: Vec::new(),
        in_data: false,
        note: None,
        texts: Vec::new(),
        title_boxes: Vec::new(),
        legend_box: None,
    };

    let kind = spec.kind;
    // 兜底：即使是从旧工程文件读入的「图型 / 表格类型」非法组合，也只给说明，
    // 绝不画一张看起来像图、其实语义错误的图形。
    let (x, y) = if !kind.supports(table.kind) {
        fallback(
            &mut ctx,
            &format!(
                "{} 不能用于「{}」表\n{}",
                kind.label(),
                table.kind.label(),
                kind.requires()
            ),
        )
    } else {
        match kind.family() {
            Family::Xy => render_xy(&mut ctx, table, kind),
            Family::Category => match kind {
                ChartKind::GroupedBar => render_grouped(&mut ctx, table),
                ChartKind::StackedBar | ChartKind::StackedBar100 => {
                    crate::plot::kinds::render_stacked(&mut ctx, table, kind)
                }
                ChartKind::HorizontalBar => crate::plot::kinds::render_horizontal(&mut ctx, table),
                ChartKind::BeforeAfter => crate::plot::kinds::render_before_after(&mut ctx, table),
                ChartKind::Ridgeline => crate::plot::kinds::render_ridgeline(&mut ctx, table),
                _ => render_categorical(&mut ctx, table, kind),
            },
            Family::Distribution => crate::plot::extras::render_distribution(&mut ctx, table, kind),
            Family::Special => match kind {
                ChartKind::Roc => crate::plot::extras::render_roc(&mut ctx, table),
                ChartKind::Forest => crate::plot::extras::render_forest(&mut ctx, table),
                ChartKind::Survival => crate::plot::extras::render_survival(&mut ctx, table),
                ChartKind::Heatmap => crate::plot::extras::render_heatmap(&mut ctx, table),
                _ => crate::plot::kinds::render_pie(&mut ctx, table, kind),
            },
        }
    };

    // §9.1 文本碰撞：标签互相压、绘图区内的标注越出边框。
    for v in text_collisions(&ctx.texts, ctx.plot) {
        ctx.viol.push(v);
    }

    Render {
        width: ctx.scene.width.round().max(1.0) as u32,
        height: ctx.scene.height.round().max(1.0) as u32,
        scene: ctx.scene,
        plot: ctx.plot,
        x,
        y,
        hits: ctx.hits,
        px_per_pt: s,
        violations: ctx.viol,
        title_boxes: ctx.title_boxes,
        legend_box: ctx.legend_box,
    }
}

/// 文字外框的收缩量：字体度量含侧边距，缩掉 1px 去噪，避免贴边误报。
const TEXT_SHRINK: f32 = 1.0;

/// 文本碰撞检测（§9.1 的文字层）。
///
/// 两条规则：
/// 1. **标签互相压**：两行轴对齐文字的外框仍有实质交叠（去噪后 > 1px）；
/// 2. **绘图区内的标注越出边框**：中心落在绘图区里的文字必须整个在框内
///    （框外的刻度标签、轴标题、风险表本就在框外，不参与）。
pub(crate) fn text_collisions(texts: &[(String, Rect)], plot: Rect) -> Vec<String> {
    let mut out = Vec::new();
    let shrink = |r: Rect| {
        Rect::new(
            r.x + TEXT_SHRINK,
            r.y + TEXT_SHRINK,
            (r.w - 2.0 * TEXT_SHRINK).max(0.0),
            (r.h - 2.0 * TEXT_SHRINK).max(0.0),
        )
    };
    let label = |t: &str| {
        let mut it = t.chars();
        let head: String = it.by_ref().take(12).collect();
        if it.next().is_some() {
            format!("{head}…")
        } else {
            head
        }
    };
    for i in 0..texts.len() {
        for j in (i + 1)..texts.len() {
            let a = shrink(texts[i].1);
            let b = shrink(texts[j].1);
            if a.w <= 0.0 || a.h <= 0.0 || b.w <= 0.0 || b.h <= 0.0 {
                continue;
            }
            let ox = a.right().min(b.right()) - a.x.max(b.x);
            let oy = a.bottom().min(b.bottom()) - a.y.max(b.y);
            if ox > 1.0 && oy > 1.0 {
                out.push(format!(
                    "文字重叠：「{}」与「{}」交叠 {:.0}×{:.0}px",
                    label(&texts[i].0),
                    label(&texts[j].0),
                    ox,
                    oy
                ));
            }
        }
    }
    if plot.w > 1.0 && plot.h > 1.0 {
        let tol = 1.5;
        for (t, r) in texts {
            let cx = r.x + r.w / 2.0;
            let cy = r.y + r.h / 2.0;
            let anchor_inside =
                cx > plot.x && cx < plot.right() && cy > plot.y && cy < plot.bottom();
            if !anchor_inside {
                continue;
            }
            if r.x < plot.x - tol
                || r.y < plot.y - tol
                || r.right() > plot.right() + tol
                || r.bottom() > plot.bottom() + tol
            {
                out.push(format!(
                    "标注文字「{}」越出绘图区边框（{:.0}×{:.0}px @ {:.0},{:.0}）",
                    label(t),
                    r.w,
                    r.h,
                    r.x,
                    r.y
                ));
            }
        }
    }
    out
}

fn palette_colors(spec: &GraphSpec) -> Vec<Rgba> {
    let mut colors = spec.palette.colors();
    if spec.theme == crate::core::graph::PlotThemeId::Mono {
        colors = crate::core::palette::PaletteId::Grayscale.colors();
    }
    colors
}

// ---------------------------------------------------------------------------
// 数据准备
// ---------------------------------------------------------------------------

pub(crate) struct GroupData {
    pub name: String,
    pub color: Rgba,
    pub values: Vec<f64>,
    pub desc: stats::Desc,
}

pub(crate) fn build_groups(ctx: &Ctx, table: &DataTable) -> Vec<GroupData> {
    table
        .analysis_groups()
        .into_iter()
        .enumerate()
        .filter_map(|(i, (name, values))| {
            let desc = stats::describe(&values)?;
            Some(GroupData {
                name,
                color: ctx.color(i),
                values,
                desc,
            })
        })
        .collect()
}

struct XySeries {
    name: String,
    color: Rgba,
    /// 逐行聚合后的均值 ± SD（折线、误差棒用）
    rows: Vec<(f64, f64, Option<f64>, usize)>,
    /// 全部原始观测（散点、拟合用）
    raw: Vec<(f64, f64)>,
}

fn build_xy(ctx: &Ctx, table: &DataTable) -> Vec<XySeries> {
    table
        .group_indices()
        .into_iter()
        .enumerate()
        .filter_map(|(i, g)| {
            let rows = table.xy_rows(g);
            if rows.is_empty() {
                None
            } else {
                Some(XySeries {
                    name: table.group_name(g),
                    color: ctx.color(i),
                    raw: table.xy_observations(g),
                    rows,
                })
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 版式
// ---------------------------------------------------------------------------

pub(crate) struct Frame {
    pub plot: Rect,
    pub x: Axis,
    pub y: Axis,
    pub xticks: Vec<Tick>,
    pub yticks: Vec<Tick>,
    pub cat: Option<Vec<String>>,
    pub cat_rotate: bool,
}

/// 单条序列超过这么多个观测，就不再逐点画标记（只画线）。
///
/// 阈值取 120：120 个点以内的符号还看得清、也压不住曲线；再密就只剩一团。
/// 想强行显示标记，把点调成「散点图」（不连线）即可。
pub(crate) const MAX_MARKERS: usize = 120;

pub(crate) fn frame_layout(
    ctx: &Ctx,
    x: Axis,
    y: Axis,
    xticks: Vec<Tick>,
    yticks: Vec<Tick>,
    cat: Option<Vec<String>>,
    x_title: &str,
    y_title: &str,
) -> Frame {
    let s = ctx.s;
    let tick_fs = ctx.theme.font_tick;
    let tick_lh = ctx.fonts.line_height(s * tick_fs, ctx.theme.family);
    let pad_l = s * 12.0;
    let pad_r = s * 10.0;
    let pad_t = s * 10.0;
    let pad_b = s * 9.0;

    let ytick_w = yticks
        .iter()
        .map(|t| ctx.measure(&t.label, tick_fs))
        .fold(0.0, f32::max);
    let ytitle_h = if y_title.is_empty() {
        0.0
    } else {
        s * ctx.theme.font_axis_title * 1.45 + s * 3.0
    };
    let title_h = if ctx.spec.show_title && !ctx.spec.title.is_empty() {
        s * ctx.theme.font_title * 1.35 + s * 7.0
    } else {
        0.0
    };
    let xtitle_h = if x_title.is_empty() {
        0.0
    } else {
        s * ctx.theme.font_axis_title * 1.45 + s * 5.0
    };

    let legend_outside = ctx.spec.show_legend && legend_pos_for(ctx) == LegendPos::OutsideRight;
    let (legend_w, _) = legend_dims(ctx);
    let mut right = pad_r
        + s * 9.0
        + if legend_outside {
            legend_w + s * 10.0
        } else {
            0.0
        };
    // 刻度朝外：标签左移一个刻度长，左边距相应加宽。
    let tick_len_px = s * ctx.theme.tick_len;
    let left = pad_l + ytick_w + s * 6.0 + ytitle_h + tick_len_px;
    // 图注统计行（规范 G4/E2-5 要求的检验/效应量说明）占一行。
    let note_h = if ctx.note.is_some() { s * 7.5 } else { 0.0 };
    let mut bottom =
        pad_b + xtitle_h + note_h + s * ctx.theme.tick_len + s * 3.0 + tick_lh + s * 4.0;
    // 底部图例要预留空间（画在 x 轴标题下方）
    if legend_pos_for(ctx) == LegendPos::Bottom {
        let (_, legend_h) = legend_dims(ctx);
        bottom += legend_h + s * 4.0;
    }
    let top = pad_t + title_h;

    let mut cat_rotate = false;
    if let Some(labels) = &cat {
        let maxw = labels
            .iter()
            .map(|l| ctx.measure(l, tick_fs))
            .fold(0.0, f32::max);
        let est = (ctx.w - left - right).max(s * 40.0);
        let spacing = est / labels.len().max(1) as f32;
        if maxw > spacing * 0.92 {
            cat_rotate = true;
            bottom += maxw * 0.60 + s * 4.0;
            right += maxw * 0.45;
        }
    } else if let Some(last) = xticks.last() {
        right += ctx.measure(&last.label, tick_fs) * 0.5;
    }

    let plot_w = (ctx.w - left - right).max(s * 30.0);
    let plot_h = (ctx.h - top - bottom).max(s * 30.0);
    let plot = Rect::new(left, top, plot_w, plot_h);

    Frame {
        plot,
        x,
        y,
        xticks,
        yticks,
        cat,
        cat_rotate,
    }
}

pub(crate) fn xpx(f: &Frame, v: f64) -> f32 {
    f.plot.x + f.x.norm(v) as f32 * f.plot.w
}

pub(crate) fn ypx(f: &Frame, v: f64) -> f32 {
    f.plot.y + (1.0 - f.y.norm(v) as f32) * f.plot.h
}

/// 用**给定轴**做 Y 映射（双 Y 轴：第二组起走右轴）。
pub(crate) fn ypx_axis(f: &Frame, axis: &crate::plot::axis::Axis, v: f64) -> f32 {
    f.plot.y + (1.0 - axis.norm(v) as f32) * f.plot.h
}

// ---------------------------------------------------------------------------
// 网格 / 坐标轴 / 标题 / 图例
// ---------------------------------------------------------------------------

pub(crate) fn draw_grid(ctx: &mut Ctx, f: &Frame) {
    if !ctx.spec.show_grid {
        return;
    }
    let color = ctx.theme.grid;
    let dash: &[f32] = if ctx.theme.grid_dash {
        &[3.0, 3.0]
    } else {
        &[]
    };
    for t in &f.xticks {
        if !t.major || t.label.is_empty() {
            continue;
        }
        let x = xpx(f, t.value);
        ctx.line(
            Pt::new(x, f.plot.y),
            Pt::new(x, f.plot.bottom()),
            0.55,
            color,
            dash,
        );
    }
    for t in &f.yticks {
        if !t.major || t.label.is_empty() {
            continue;
        }
        let y = ypx(f, t.value);
        ctx.line(
            Pt::new(f.plot.x, y),
            Pt::new(f.plot.right(), y),
            0.55,
            color,
            dash,
        );
    }
}

pub(crate) fn draw_axes(ctx: &mut Ctx, f: &Frame, x_title: &str, y_title: &str) {
    // 坐标轴与刻度本就在绘图区之外，不参与几何自检。
    ctx.in_data = false;
    let s = ctx.s;
    let axis_color = ctx.theme.axis;
    let tick_len = s * ctx.theme.tick_len;

    if ctx.theme.closed_frame {
        // 上/右边框用浅色，左/下坐标轴用深色：既有 Prism 的闭合感，又有现代层级。
        ctx.spine(
            Pt::new(f.plot.x, f.plot.y),
            Pt::new(f.plot.right(), f.plot.y),
            ctx.theme.axis_width * 0.85,
            ctx.theme.frame,
        );
        ctx.spine(
            Pt::new(f.plot.right(), f.plot.y),
            Pt::new(f.plot.right(), f.plot.bottom()),
            ctx.theme.axis_width * 0.85,
            ctx.theme.frame,
        );
        ctx.spine(
            Pt::new(f.plot.x, f.plot.y),
            Pt::new(f.plot.x, f.plot.bottom()),
            ctx.theme.axis_width,
            axis_color,
        );
        ctx.spine(
            Pt::new(f.plot.x, f.plot.bottom()),
            Pt::new(f.plot.right(), f.plot.bottom()),
            ctx.theme.axis_width,
            axis_color,
        );
    } else {
        ctx.spine(
            Pt::new(f.plot.x, f.plot.y),
            Pt::new(f.plot.x, f.plot.bottom()),
            ctx.theme.axis_width,
            axis_color,
        );
        ctx.spine(
            Pt::new(f.plot.x, f.plot.bottom()),
            Pt::new(f.plot.right(), f.plot.bottom()),
            ctx.theme.axis_width,
            axis_color,
        );
    }

    // 刻度线：**朝外**。
    //
    // 朝内时刻度会压在柱子和数据点上（柱状图底边一圈小竖线就是它），
    // 而且和误差棒、须端混在一起分不清。Nature / Prism 的默认都是朝外。
    if f.cat.is_none() {
        for t in &f.xticks {
            let x = xpx(f, t.value);
            let l = if t.major { tick_len } else { tick_len * 0.55 };
            ctx.spine(
                Pt::new(x, f.plot.bottom()),
                Pt::new(x, f.plot.bottom() + l),
                ctx.theme.axis_width * 0.9,
                axis_color,
            );
        }
    }
    for t in &f.yticks {
        let y = ypx(f, t.value);
        let l = if t.major { tick_len } else { tick_len * 0.55 };
        ctx.spine(
            Pt::new(f.plot.x, y),
            Pt::new(f.plot.x - l, y),
            ctx.theme.axis_width * 0.9,
            axis_color,
        );
    }

    // X 刻度标签
    let label_y = f.plot.bottom() + tick_len + s * 3.0;
    if let Some(labels) = &f.cat {
        for (i, label) in labels.iter().enumerate() {
            let x = xpx(f, i as f64);
            if f.cat_rotate {
                ctx.text(
                    label,
                    Pt::new(x + s * 1.0, label_y + s * 1.5),
                    ctx.theme.font_tick,
                    ctx.theme.ink,
                    HAlign::Right,
                    VAlign::Top,
                    -0.60,
                    false,
                );
            } else {
                ctx.text(
                    label,
                    Pt::new(x, label_y),
                    ctx.theme.font_tick,
                    ctx.theme.ink,
                    HAlign::Center,
                    VAlign::Top,
                    0.0,
                    false,
                );
            }
        }
    } else {
        for t in &f.xticks {
            if !t.major || t.label.is_empty() {
                continue;
            }
            ctx.text(
                &t.label,
                Pt::new(xpx(f, t.value), label_y),
                ctx.theme.font_tick,
                ctx.theme.ink,
                HAlign::Center,
                VAlign::Top,
                0.0,
                false,
            );
        }
    }

    // Y 刻度标签（刻度朝外，标签要让开刻度长度）
    for t in &f.yticks {
        if !t.major || t.label.is_empty() {
            continue;
        }
        ctx.text(
            &t.label,
            Pt::new(f.plot.x - tick_len - s * 3.0, ypx(f, t.value)),
            ctx.theme.font_tick,
            ctx.theme.ink,
            HAlign::Right,
            VAlign::Middle,
            0.0,
            false,
        );
    }

    draw_titles(ctx, f, x_title, y_title);
}

/// 轴标题 / 图标题 / 图注行。
///
/// 分面与常规图共用：它们只差「刻度和轴线怎么画」，标题位置完全一样。
fn draw_titles(ctx: &mut Ctx, f: &Frame, x_title: &str, y_title: &str) {
    let s = ctx.s;
    // 轴标题（有图注统计行时上移让位）
    let note_h = if ctx.note.is_some() { s * 7.5 } else { 0.0 };
    if !x_title.is_empty() {
        ctx.text_slot(
            TitleSlot::X,
            x_title,
            Pt::new(f.plot.center().x, ctx.h - s * 3.0 - note_h),
            ctx.theme.font_axis_title,
            HAlign::Center,
            VAlign::Bottom,
            0.0,
            false,
        );
    }
    // 图注统计行：G4/E2-5 要求的检验全名/双侧/校正/效应量/95%CI/log-rank/HR。
    // 单行小字，靠画布左下，不进绘图区、不遮数据。
    if let Some(note) = ctx.note.clone() {
        ctx.text(
            &note,
            Pt::new(f.plot.x, ctx.h - s * 1.5),
            (ctx.theme.font_tick - 1.1).max(4.6),
            ctx.theme.muted,
            HAlign::Left,
            VAlign::Bottom,
            0.0,
            false,
        );
    }
    if !y_title.is_empty() {
        ctx.text_slot(
            TitleSlot::Y,
            y_title,
            Pt::new(s * 9.0, f.plot.center().y),
            ctx.theme.font_axis_title,
            HAlign::Center,
            VAlign::Middle,
            -PI / 2.0,
            false,
        );
    }

    // 图标题
    let title = ctx.spec.title.clone();
    if ctx.spec.show_title && !title.is_empty() {
        ctx.text_slot(
            TitleSlot::Main,
            &title,
            Pt::new(ctx.w / 2.0, s * 6.0),
            ctx.theme.font_title,
            HAlign::Center,
            VAlign::Top,
            0.0,
            true,
        );
    }
}

/// 压在有填充色块上的文字该用什么颜色。
///
/// Nature `figure-style` §5.5：标记内的文字要 ≥4.5:1 对比度。白字在浅色
/// 填充（青绿、琥珀、淡紫）上只有 2–3:1，读起来发飘；按相对亮度在白与
/// 近黑之间二选一，取对比度更高的那个。
pub(crate) fn ink_on(fill: Rgba) -> Rgba {
    let lum = (0.2126 * fill.r as f32 + 0.7152 * fill.g as f32 + 0.0722 * fill.b as f32) / 255.0;
    let white = 1.05 / (lum + 0.05);
    let black = (lum + 0.05) / 0.05;
    if white >= black {
        Rgba::rgba(255, 255, 255, 246)
    } else {
        Rgba::rgba(24, 30, 40, 246)
    }
}

/// 实际生效的图例位置。
///
/// 填充类图型的色块会占到右上角，`InsideTopRight` 会把图例压在一根柱子上。
/// 这类图型统一改到右侧外部；布局阶段与绘制阶段必须用同一个判断，
/// 否则会出现「预留了空间却没画在外面」或反之。
pub(crate) fn legend_pos_for(ctx: &Ctx) -> LegendPos {
    if ctx.spec.legend_pos == LegendPos::InsideTopRight && ctx.spec.kind.filled_marks() {
        LegendPos::OutsideRight
    } else {
        ctx.spec.legend_pos
    }
}

pub(crate) fn legend_dims(ctx: &Ctx) -> (f32, f32) {
    if !ctx.spec.show_legend || ctx.legend.is_empty() {
        return (0.0, 0.0);
    }
    let s = ctx.s;
    let fs = ctx.theme.font_legend;
    let row_h = s * fs * 1.65;
    if legend_pos_for(ctx) == LegendPos::Bottom {
        // 底部横排：所有项并排，总宽 = 各项宽度之和 + 间距
        let mut w = 0.0_f32;
        for item in &ctx.legend {
            w += ctx.measure(&item.label, fs) + s * 26.0; // 色块/线 + 文字 + 间距
        }
        w += s * 10.0;
        return (w, row_h + s * 8.0);
    }
    let mut w = 0.0_f32;
    for item in &ctx.legend {
        w = w.max(ctx.measure(&item.label, fs) + s * 26.0);
    }
    (w + s * 10.0, ctx.legend.len() as f32 * row_h + s * 8.0)
}

pub(crate) fn draw_legend(ctx: &mut Ctx, f: &Frame) {
    if !ctx.spec.show_legend || ctx.legend.is_empty() {
        return;
    }
    // 图例可能落在绘图区外（OutsideRight），同样不参与数据图元自检。
    let saved_plot = ctx.plot;
    ctx.in_data = false;
    let s = ctx.s;
    let fs = ctx.theme.font_legend;
    let row_h = s * fs * 1.65;
    let (w, h) = legend_dims(ctx);
    let items = ctx.legend.clone();

    let (x, y) = match legend_pos_for(ctx) {
        LegendPos::InsideTopRight => (f.plot.right() - w - s * 6.0, f.plot.y + s * 6.0),
        LegendPos::InsideTopLeft => (f.plot.x + s * 6.0, f.plot.y + s * 6.0),
        LegendPos::OutsideRight => (f.plot.right() + s * 10.0, f.plot.y),
        // 底部横排：画在全部 x 轴元素（刻度/标签/标题/图注）之后。
        // 从画布底边上推——确保不覆盖 x 轴标签。
        LegendPos::Bottom => {
            let y = ctx.h - h - s * 2.0;
            (f.plot.center().x - w / 2.0, y)
        }
    };

    // 记录图例外框（画布拖拽定位用）
    ctx.legend_box = Some(Rect::new(x, y, w, h));

    if legend_pos_for(ctx) != LegendPos::OutsideRight {
        let bg = Rgba::rgba(255, 255, 255, 238);
        ctx.scene.rect(
            Rect::new(x, y, w, h),
            Some(bg),
            Some(Stroke::new(s * 0.5, Rgba::rgba(210, 218, 228, 220))),
        );
    }
    let horizontal = legend_pos_for(ctx) == LegendPos::Bottom;
    let mut cursor_x = x + s * 5.0; // 横排时推进的 x 光标
    for (i, item) in items.iter().enumerate() {
        // 竖排：每项独占一行；横排（Bottom）：所有项并排
        let (mx, cy, tx) = if horizontal {
            let cy = y + h * 0.5;
            let mx = cursor_x + s * 5.0;
            let tx = cursor_x + s * 16.0;
            cursor_x += ctx.measure(&item.label, fs) + s * 26.0;
            (mx, cy, tx)
        } else {
            let cy = y + s * 4.0 + row_h * (i as f32 + 0.5);
            let mx = x + s * 9.0;
            let tx = x + s * 18.0;
            (mx, cy, tx)
        };
        match item.kind {
            LegendKind::Square => {
                let r = Rect::new(mx - s * 3.0, cy - s * 3.0, s * 6.0, s * 6.0);
                ctx.scene.rect(r, Some(item.color), None);
            }
            LegendKind::Marker => {
                draw_marker(
                    ctx,
                    Pt::new(mx, cy),
                    3.8,
                    Marker::Circle,
                    item.color,
                    ctx.theme.point_edge,
                );
            }
            LegendKind::Line
            | LegendKind::LineMarker
            | LegendKind::Dashed
            | LegendKind::DashDot => {
                let dash: Vec<f32> = match item.kind {
                    LegendKind::Dashed => vec![s * 3.0, s * 2.0],
                    LegendKind::DashDot => vec![s * 4.0, s * 1.6, s * 1.2, s * 1.6],
                    _ => Vec::new(),
                };
                ctx.line(
                    Pt::new(mx - s * 5.0, cy),
                    Pt::new(mx + s * 5.0, cy),
                    1.6,
                    item.color,
                    &dash,
                );
                if item.kind == LegendKind::LineMarker {
                    draw_marker(ctx, Pt::new(mx, cy), 3.2, Marker::Circle, item.color, true);
                }
            }
        }
        ctx.text(
            &item.label,
            Pt::new(tx, cy),
            fs,
            ctx.theme.ink,
            HAlign::Left,
            VAlign::Middle,
            0.0,
            false,
        );
    }
    ctx.plot = saved_plot;
}

// ---------------------------------------------------------------------------
// 图元
// ---------------------------------------------------------------------------

/// 空心标记：只描边、不填充。
///
/// 箱线图与小提琴的**离群点**必须用它。实心圆和数据点长得一模一样，
/// 读者会以为那里多了一个观测（曾经就被当成「箱线图下面怎么还有个散点」）。
/// 空心圈是「这个点被单独挑出来了」的通用记号。
pub(crate) fn draw_open_marker(ctx: &mut Ctx, at: Pt, size_pt: f32, color: Rgba) {
    let d = ctx.px(size_pt).max(1.2);
    let r = d / 2.0;
    ctx.check_pt("离群点", at, r);
    let st = Stroke::new((ctx.s * 0.65).max(0.3), color);
    ctx.scene.push(Path::circle(at, r), None, Some(st));
}

pub(crate) fn draw_marker(
    ctx: &mut Ctx,
    at: Pt,
    size_pt: f32,
    marker: Marker,
    fill: Rgba,
    outline: bool,
) {
    let d = ctx.px(size_pt).max(1.0);
    let r = d / 2.0;
    // §3.1：标记点不得触碰/越过边框，容差取半径。
    ctx.check_pt("标记点", at, r);
    let stroke = if outline {
        Some(Stroke::new((ctx.s * 0.55).max(0.3), fill.darken(0.25)))
    } else {
        None
    };
    let path = match marker {
        Marker::Circle => Path::circle(at, r),
        Marker::Square => Path::rect(Rect::new(at.x - r, at.y - r, d, d)),
        Marker::Triangle => Path::polygon(&[
            Pt::new(at.x, at.y - r),
            Pt::new(at.x + r * 0.95, at.y + r * 0.72),
            Pt::new(at.x - r * 0.95, at.y + r * 0.72),
        ]),
        Marker::Diamond => Path::polygon(&[
            Pt::new(at.x, at.y - r * 1.1),
            Pt::new(at.x + r * 0.82, at.y),
            Pt::new(at.x, at.y + r * 1.1),
            Pt::new(at.x - r * 0.82, at.y),
        ]),
        Marker::Cross => {
            let mut st = Stroke::new((ctx.s * 0.95).max(0.5), fill);
            st.cap = LineCap::Round;
            ctx.scene.line(
                Pt::new(at.x - r, at.y - r),
                Pt::new(at.x + r, at.y + r),
                st.clone(),
            );
            ctx.scene
                .line(Pt::new(at.x - r, at.y + r), Pt::new(at.x + r, at.y - r), st);
            return;
        }
    };
    ctx.scene.push(path, Some(fill), stroke);
}

pub(crate) fn draw_error_bar(
    ctx: &mut Ctx,
    x: f32,
    y: f32,
    err: f32,
    color: Rgba,
    width_pt: f32,
    cap_pt: f32,
) {
    if !(err > 0.0) || !err.is_finite() {
        return;
    }
    let cap = ctx.px(cap_pt) / 2.0;
    ctx.line(
        Pt::new(x, y - err),
        Pt::new(x, y + err),
        width_pt,
        color,
        &[],
    );
    ctx.line(
        Pt::new(x - cap, y - err),
        Pt::new(x + cap, y - err),
        width_pt,
        color,
        &[],
    );
    ctx.line(
        Pt::new(x - cap, y + err),
        Pt::new(x + cap, y + err),
        width_pt,
        color,
        &[],
    );
}

fn err_value(kind: ErrorKind, sd: Option<f64>, n: usize) -> Option<f64> {
    let sd = sd?;
    if n < 2 {
        return None;
    }
    let se = sd / (n as f64).sqrt();
    match kind {
        ErrorKind::None => None,
        ErrorKind::Sd => Some(sd),
        ErrorKind::Sem => Some(se),
        ErrorKind::Ci95 => Some(stats::t_quantile(0.975, (n - 1) as f64) * se),
    }
}

pub(crate) fn clip_segment_public(r: Rect, a: Pt, b: Pt) -> Option<(Pt, Pt)> {
    clip_segment(r, a, b)
}

fn clip_segment(r: Rect, a: Pt, b: Pt) -> Option<(Pt, Pt)> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let p = [-dx, dx, -dy, dy];
    let q = [a.x - r.x, r.right() - a.x, a.y - r.y, r.bottom() - a.y];
    let mut t0 = 0.0_f32;
    let mut t1 = 1.0_f32;
    for i in 0..4 {
        if p[i].abs() < 1e-9 {
            if q[i] < 0.0 {
                return None;
            }
        } else {
            let t = q[i] / p[i];
            if p[i] < 0.0 {
                if t > t1 {
                    return None;
                }
                t0 = t0.max(t);
            } else {
                if t < t0 {
                    return None;
                }
                t1 = t1.min(t);
            }
        }
    }
    Some((
        Pt::new(a.x + t0 * dx, a.y + t0 * dy),
        Pt::new(a.x + t1 * dx, a.y + t1 * dy),
    ))
}

fn fmt(v: f64) -> String {
    format_number(v)
}

// ---------------------------------------------------------------------------
// XY：散点 / 折线
// ---------------------------------------------------------------------------

/// Catmull-Rom 插值：穿过每个观测点，采样成折线（裁剪与描边都用得上）。
///
/// 用「穿过点」而不是最小二乘平滑，是因为平滑曲线图（Prism 的
/// Interpolate / Spline）语义就是「插值」；要做拟合请用回归图。
/// 单调保形三次插值（PCHIP / Fritsch–Carlson）。
///
/// 与 Catmull-Rom 一样**穿过每个观测点**，但斜率经过限幅：只要相邻区间
/// 单调，插值结果就不会过冲到数据范围之外，也不会画出数据里不存在的
/// 峰和谷。这是出版级「平滑曲线」应有的性质 —— 曲线是插值，不是拟合，
/// 更不能凭空制造极值。
///
/// 参考：Fritsch & Carlson, *Monotone Piecewise Cubic Interpolation*,
/// SIAM J. Numer. Anal. 17 (1980)；ggplot2 / Prism 的 spline 也按此口径。
pub(crate) fn monotone_spline(points: &[Pt], samples_per_span: usize) -> Vec<Pt> {
    let n = points.len();
    if n < 2 {
        return points.to_vec();
    }
    // x 必须严格递增；有重复或非有限值时退化为折线，不制造伪曲线。
    let mut h = Vec::with_capacity(n - 1);
    let mut d = Vec::with_capacity(n - 1);
    for w in points.windows(2) {
        let dx = w[1].x - w[0].x;
        if !(dx > 1e-6) || !dx.is_finite() {
            return points.to_vec();
        }
        h.push(dx);
        d.push((w[1].y - w[0].y) / dx);
    }

    // 节点斜率：端点用单侧割线，内部用按区间宽度加权的调和平均；
    // 相邻割线异号（局部极值）时置 0，保证不越过该点。
    let mut m = vec![0.0f32; n];
    m[0] = d[0];
    m[n - 1] = d[n - 2];
    for i in 1..n - 1 {
        if d[i - 1] * d[i] <= 0.0 {
            m[i] = 0.0;
        } else {
            let (h0, h1) = (h[i - 1], h[i]);
            let w1 = 2.0 * h1 + h0;
            let w2 = h1 + 2.0 * h0;
            m[i] = (w1 + w2) / (w1 / d[i - 1] + w2 / d[i]);
        }
    }

    let steps = samples_per_span.max(2);
    let mut out = Vec::with_capacity(n * steps);
    out.push(points[0]);
    for i in 0..n - 1 {
        let hi = h[i];
        let di = d[i];
        // Fritsch–Carlson 限幅：把 (α, β) 约束进 α²+β² ≤ 9 的圆内。
        let (mut a, mut b) = if di.abs() < 1e-12 {
            (0.0, 0.0)
        } else {
            (m[i] / di, m[i + 1] / di)
        };
        let sq = a * a + b * b;
        if sq > 9.0 {
            let t = 3.0 / sq.sqrt();
            a *= t;
            b *= t;
        }
        let (m0, m1) = (a * di, b * di);
        let (p0, p1) = (points[i], points[i + 1]);
        for k in 1..=steps {
            let t = k as f32 / steps as f32;
            let (t2, t3) = (t * t, t * t * t);
            let y = (2.0 * t3 - 3.0 * t2 + 1.0) * p0.y
                + (t3 - 2.0 * t2 + t) * hi * m0
                + (-2.0 * t3 + 3.0 * t2) * p1.y
                + (t3 - t2) * hi * m1;
            out.push(Pt::new(p0.x + hi * t, y));
        }
    }
    out
}

/// 阶梯线（step-after）：先水平再竖直，与 survival/ECDF 的约定一致。
pub(crate) fn step_polyline(points: &[Pt]) -> Vec<Pt> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let mut out = Vec::with_capacity(points.len() * 2);
    out.push(points[0]);
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        out.push(Pt::new(b.x, a.y));
        out.push(b);
    }
    out
}

/// 折线描边（先裁剪再画，保证不出坐标框）。
fn stroke_polyline(ctx: &mut Ctx, f: &Frame, pts: &[Pt], width: f32, color: Rgba, dash: &[f32]) {
    for run in clip::polyline(pts, f.plot) {
        let mut path = Path::new();
        path.move_to(run[0]);
        for p in &run[1..] {
            path.line_to(*p);
        }
        let mut st = Stroke::new((ctx.s * width).max(0.5), color);
        st.cap = LineCap::Round;
        st.join = LineJoin::Round;
        if !dash.is_empty() {
            st.dash = dash.iter().map(|d| d * ctx.s).collect();
        }
        ctx.scene.stroke(path, st);
    }
}

fn render_xy(ctx: &mut Ctx, table: &DataTable, kind: ChartKind) -> (Axis, Axis) {
    let series = build_xy(ctx, table);
    // 分面（规范 G8：多组颜色堆在一起时必须分面）：每组一个面板，共享 X 与 Y 域。
    if ctx.spec.facet && series.len() > 1 {
        return render_faceted_xy(ctx, series, kind);
    }
    if series.is_empty() || table.kind != TableKind::Xy {
        return fallback(ctx, kind.requires());
    }
    let connect = matches!(
        kind,
        ChartKind::Line | ChartKind::SmoothLine | ChartKind::StepLine | ChartKind::AreaLine
    );

    // 回归：每个数据集一条拟合线（多个数据集时不合并，避免伪回归）。
    let fits: Vec<Option<stats::Fit>> = if kind.fits() {
        series
            .iter()
            .map(|s| {
                // 用**原始观测**拟合，而不是行均值：均值会把组内变异抹平，
                // 残差自由度凭空变大，R² 虚高、置信带偏窄。
                let (xs, ys): (Vec<f64>, Vec<f64>) = if s.raw.len() >= 2 {
                    s.raw.iter().copied().unzip()
                } else {
                    s.rows.iter().map(|r| (r.0, r.1)).unzip()
                };
                stats::fit_line(&xs, &ys)
            })
            .collect()
    } else {
        vec![None; series.len()]
    };
    if kind.fits() && fits.iter().all(|f| f.is_none()) {
        return fallback(ctx, kind.requires());
    }

    let mut xs_all: Vec<f64> = Vec::new();
    let mut ys_all: Vec<f64> = Vec::new();
    for s in &series {
        for &(x, y, err, _) in &s.rows {
            xs_all.push(x);
            ys_all.push(y);
            if let Some(e) = err {
                ys_all.push(y + e);
                ys_all.push(y - e);
            }
        }
        // 散点/回归画的是原始观测，它们可能超出均值 ± SD 的范围（n 小的时候
        // 尤其明显）。轴域不算上就会有点被 `contains` 过滤掉，图上凭空少点。
        if kind.fits() || !connect {
            for &(x, y) in &s.raw {
                xs_all.push(x);
                ys_all.push(y);
            }
        }
    }
    let all_positive = ys_all.iter().all(|v| *v >= 0.0);
    let area = kind.area();
    // 面积图的下边界要进定义域，否则填充块会被轴底切掉。
    if area && all_positive {
        ys_all.push(0.0);
    }
    // 曲线拟合：`fit_model` 决定用哪种模型（直线/多项式/Logistic/Hill/…）。
    // 每个序列独立拟合，用**原始观测**（不是行均值）。
    let curves: Vec<Option<crate::core::curve::CurveFit>> = if kind.fits() {
        series
            .iter()
            .map(|s| {
                let pts: Vec<(f64, f64)> = if s.raw.len() >= 2 {
                    s.raw.clone()
                } else {
                    s.rows.iter().map(|r| (r.0, r.1)).collect()
                };
                crate::core::curve::fit_curve(ctx.spec.fit_model, &pts)
            })
            .collect()
    } else {
        Vec::new()
    };

    // 双 Y 轴：第二个序列起映射到右侧轴（各自量纲，刻度独立）。
    let y2_axis: Option<crate::plot::axis::Axis> = if ctx.spec.dual_y && series.len() >= 2 {
        let mut vals = Vec::new();
        for s in &series[1..] {
            for &(_, y, e, _) in &s.rows {
                vals.push(y);
                if let Some(e) = e {
                    vals.push(y + e);
                    vals.push(y - e);
                }
            }
            for &(_, y) in &s.raw {
                vals.push(y);
            }
        }
        Some(axis_or_range(ctx.spec.y_log, None, vals, false))
    } else {
        None
    };

    let x = axis_or_range(ctx.spec.x_log, ctx.spec.x_range, xs_all, false);
    // 拟合线在两端可能略超出数据范围：把端点（含置信带）并入定义域。
    if kind.fits() {
        for fit in fits.iter().flatten() {
            for xv in [x.min, x.max] {
                let yv = fit.at(xv);
                let ci = fit.ci95(xv, stats::t_quantile(0.975, (fit.n - 2) as f64));
                ys_all.push(yv + ci);
                ys_all.push(yv - ci);
            }
        }
    }
    let y = axis_or_range(
        ctx.spec.y_log,
        ctx.spec.y_range,
        ys_all,
        area && all_positive,
    );
    let xticks = x.ticks(6);
    let yticks = y.ticks(6);

    ctx.legend = if ctx.spec.show_legend && series.len() > 1 {
        series
            .iter()
            .enumerate()
            .map(|(i, s)| LegendItem {
                // C1：图内必须给出 R²。多组时塞在图例里，不占绘图区。
                label: if kind.fits() {
                    match curves.get(i).and_then(|c| c.as_ref()) {
                        Some(cf) => format!("{} (R² = {:.3})", s.name, cf.r2),
                        None => s.name.clone(),
                    }
                } else {
                    s.name.clone()
                },
                color: s.color,
                kind: if connect || kind.fits() {
                    legend_kind_for_line(i, true)
                } else {
                    LegendKind::Marker
                },
            })
            .collect()
    } else {
        Vec::new()
    };

    let mut x_title = ctx.spec.x_title.clone();
    let mut y_title = ctx.spec.y_title.clone();
    // G2：对数轴必须在轴标题里写明底数，否则读者会把 10/100/1000 当成线性刻度。
    if x.log && !x_title.contains("log") {
        x_title.push_str(" (log₁₀)");
    }
    if y.log && !y_title.contains("log") {
        y_title.push_str(" (log₁₀)");
    }
    // 图注统计行（C1-2/C1-4）：斜率 b 及 95% CI；多组给 ANCOVA 交互检验。
    if kind.fits() {
        if series.len() == 1 {
            if let Some(fit) = fits.first().and_then(|f| *f) {
                let se = if fit.sxx > 0.0 {
                    fit.s / fit.sxx.sqrt()
                } else {
                    0.0
                };
                let tc = stats::t_quantile(0.975, (fit.n as f64 - 2.0).max(1.0));
                ctx.set_note(format!(
                    "b = {:.4}, 95% CI [{:.4}, {:.4}]",
                    fit.slope,
                    fit.slope - tc * se,
                    fit.slope + tc * se
                ));
            }
        } else {
            let sers: Vec<Vec<(f64, f64)>> = series
                .iter()
                .map(|s| {
                    if s.raw.len() >= 2 {
                        s.raw.clone()
                    } else {
                        s.rows.iter().map(|r| (r.0, r.1)).collect()
                    }
                })
                .collect();
            if let Some(ai) = stats::ancova_interaction(&sers) {
                let verdict = if ai.p < 0.05 {
                    "斜率组间差异显著"
                } else {
                    "斜率组间差异不显著，分组拟合仅作展示"
                };
                ctx.set_note(format!(
                    "ANCOVA 交互项 F({:.0}, {:.0}) = {:.2}, p = {:.3} · {}",
                    ai.df_num, ai.df_den, ai.f, ai.p, verdict
                ));
            }
        }
    }
    let mut f = frame_layout(ctx, x, y, xticks, yticks, None, &x_title, &y_title);
    // 右轴要放刻度标签与标题，先从绘图区匀出一条竖带，否则会画到画布外。
    if y2_axis.is_some() {
        let reserve = ctx.px(30.0);
        f.plot.w = (f.plot.w - reserve).max(ctx.px(30.0));
    }
    draw_grid(ctx, &f);
    ctx.begin_data(&f);

    // 双 Y 映射：第 0 组走左轴，其余走右轴（若启用）。
    let ymap = |si: usize, v: f64| -> f32 {
        match &y2_axis {
            Some(y2) if si > 0 => ypx_axis(&f, y2, v),
            _ => ypx(&f, v),
        }
    };

    let marker = ctx.spec.marker;
    let point_size = ctx.spec.point_size as f32;
    let line_w = ctx.spec.line_width as f32;
    let error = ctx.spec.error;
    // 密集序列不逐点画标记。
    //
    // 一条 600 点的曲线如果每个点都套一个圆，点会叠成「毛毛虫」：曲线本身
    // 被盖住，形状反而看不清（Prism 对多点 XY 默认也是关掉符号的）。
    // 顺带这也是主要的性能开销——每多一个标记就多一条矢量路径。
    let densest = series.iter().map(|s| s.rows.len()).max().unwrap_or(0);
    let show_points = (ctx.spec.show_points || !connect) && densest <= MAX_MARKERS;
    let baseline = ypx(&f, 0.0_f64.clamp(f.y.min, f.y.max));

    // 分三遍画：先所有填充、再所有线、最后所有点。
    // 交错画会让后一个数据集的置信带盖住前一个数据集的散点。
    // 面积填充的绘制顺序：**从大到小**，并且用接近不透明的填充。
    //
    // 半透明多层叠加会混成灰橄榄色（三条曲线在图上认不出哪条是哪条）；
    // 改成「大块先画、块间不透明」后，每层是干净的色带，而线在第二遍
    // 以满不透明度和标记单独再画一遍，所以即使曲线交叉也不会被填埋。
    let mut fill_order: Vec<usize> = (0..series.len()).collect();
    if area {
        fill_order.sort_by(|a, b| {
            let peak = |i: usize| {
                series[i]
                    .rows
                    .iter()
                    .map(|r| r.1)
                    .fold(f64::NEG_INFINITY, f64::max)
            };
            peak(*b)
                .partial_cmp(&peak(*a))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    for si in fill_order.iter().copied() {
        let s = &series[si];
        let pts: Vec<Pt> = s
            .rows
            .iter()
            .map(|&(xv, yv, _, _)| Pt::new(xpx(&f, xv), ypx(&f, yv)))
            .filter(|p| p.x.is_finite() && p.y.is_finite())
            .collect();

        if area && pts.len() >= 2 {
            let dense = monotone_spline(&pts, 8);
            let mut poly: Vec<Pt> = Vec::with_capacity(dense.len() + 2);
            poly.extend(dense.iter().copied());
            poly.push(Pt::new(dense.last().unwrap().x, baseline));
            poly.push(Pt::new(dense[0].x, baseline));
            ctx.check_poly("面积填充", &poly);
            let clipped = clip::polygon(&poly, f.plot);
            if clipped.len() >= 3 {
                ctx.scene.push(
                    Path::polygon(&clipped),
                    Some(s.color.with_alpha(226)),
                    // G5：半透明/色块填充必须有不透明描边，否则灰度打印会糊边。
                    Some(Stroke::new((ctx.s * 0.5).max(0.3), s.color)),
                );
            }
        }

        if kind.fits() {
            if let Some(fit) = fits[si] {
                let n_samp = 64;
                let tc = stats::t_quantile(0.975, (fit.n - 2) as f64);
                // 置信带只画在**数据覆盖的 x 区间**内。
                //
                // 原来按整个轴域（f.x.min..f.x.max）铺，轴两侧有 6% 留白，
                // 于是带子比数据长出十几像素——等于在暗示「这里还有外推」。
                let (xlo, xhi) = s
                    .rows
                    .iter()
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), r| {
                        (lo.min(r.0), hi.max(r.0))
                    });
                if !(xhi > xlo) {
                    continue;
                }
                let mut poly: Vec<Pt> = Vec::with_capacity(2 * (n_samp + 1));
                for i in 0..=n_samp {
                    let xv = xlo + (xhi - xlo) * i as f64 / n_samp as f64;
                    poly.push(Pt::new(
                        xpx(&f, xv),
                        ymap(si, fit.at(xv) + fit.ci95(xv, tc)),
                    ));
                }
                for i in (0..=n_samp).rev() {
                    let xv = xlo + (xhi - xlo) * i as f64 / n_samp as f64;
                    poly.push(Pt::new(
                        xpx(&f, xv),
                        ymap(si, fit.at(xv) - fit.ci95(xv, tc)),
                    ));
                }
                // 非线性模型的置信带需要 delta-method 协方差或 bootstrap，
                // 这里只对**线性**模型给解析均值响应 CI（t 分布）；
                // 其他模型只画拟合曲线，图内标注里说明。
                if ctx.spec.fit_model != crate::core::curve::FitModel::Linear {
                    continue;
                }
                let clipped = clip::polygon(&poly, f.plot);
                if clipped.len() >= 3 {
                    // 标准画法（seaborn regplot / ggplot geom_smooth）：
                    // 每组一条拟合线 + 一条**同色的 95% 置信带**，不分几组。
                    //
                    // 之前为了防「三条带子叠成灰」，把散点压灰、把带改成虚线描边，
                    // 那是把回归图画成了别的图。真正让带子糊的是原先进带宽铺满
                    // 整个轴域（含数据外留白）、三条几乎完全重合；现在只覆盖数据
                    // 区间，再给带加一圈同色细描边，重叠处也能分辨边界。
                    let st = Stroke::new((ctx.s * 0.45).max(0.25), s.color.with_alpha(160));
                    // alpha 取 42：三条带叠在一起仍能各归各色。再深，
                    // 20–60 区间会糊成一片灰褐色。
                    ctx.scene.push(
                        Path::polygon(&clipped),
                        Some(s.color.with_alpha(42)),
                        Some(st),
                    );
                }
            }
        }
    }

    for (si, s) in series.iter().enumerate() {
        // ---- 误差棒 ----
        if error != ErrorKind::None && !kind.fits() {
            for &(xv, yv, sd, n) in &s.rows {
                if let Some(e) = err_value(error, sd, n) {
                    let x_px = xpx(&f, xv);
                    let y_px = ymap(si, yv);
                    let e_px = (ymap(si, yv + e) - y_px).abs();
                    if x_px >= f.plot.x - 2.0 && x_px <= f.plot.right() + 2.0 && y_px.is_finite() {
                        draw_error_bar(ctx, x_px, y_px, e_px, s.color.darken(0.15), 0.8, 2.6);
                    }
                }
            }
        }

        let pts: Vec<Pt> = s
            .rows
            .iter()
            .map(|&(xv, yv, _, _)| Pt::new(xpx(&f, xv), ypx(&f, yv)))
            .filter(|p| p.x.is_finite() && p.y.is_finite())
            .collect();

        // ---- 折线 ----
        if connect && pts.len() >= 2 {
            let line: Vec<Pt> = match kind {
                ChartKind::SmoothLine | ChartKind::AreaLine => monotone_spline(&pts, 10),
                ChartKind::StepLine => step_polyline(&pts),
                _ => pts.clone(),
            };
            // 多组折线：颜色 + 线型双编码（A2 第 3 条）
            let dash = if series.len() > 1 && !kind.fits() {
                series_dash(si)
            } else {
                &[]
            };
            stroke_polyline(ctx, &f, &line, line_w, s.color, dash);
        }

        // ---- 拟合曲线：只画在数据覆盖的 x 区间内，不外推 ----
        if kind.fits() {
            if let Some(cf) = curves.get(si).and_then(|c| c.as_ref()) {
                let (xlo, xhi) = s
                    .rows
                    .iter()
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), r| {
                        (lo.min(r.0), hi.max(r.0))
                    });
                // 采样画曲线：直线也走这条路，曲线才不会被画成直线。
                // X 跨数量级时按对数等距采样，S 形拐点才不会被跳过。
                let log_x = ctx.spec.x_log;
                let mut path = Path::new();
                let mut first = true;
                const STEPS: usize = 64;
                for i in 0..=STEPS {
                    let t = i as f64 / STEPS as f64;
                    let xv = if log_x && xlo > 0.0 && xhi > 0.0 {
                        10f64.powf(xlo.log10() + (xhi.log10() - xlo.log10()) * t)
                    } else {
                        xlo + (xhi - xlo) * t
                    };
                    let yv = cf.eval(xv);
                    if !yv.is_finite() {
                        continue;
                    }
                    let p = Pt::new(xpx(&f, xv), ymap(si, yv));
                    if !p.x.is_finite() || !p.y.is_finite() {
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
                    let mut st = Stroke::new((ctx.s * line_w).max(0.6), s.color);
                    st.cap = LineCap::Round;
                    st.join = LineJoin::Round;
                    ctx.scene.stroke(path, st);
                }
            }
        }
    }

    // ---- 第三遍：散点 ----
    //
    // C1 第 4 条：组数 ≥ 3 时，多组彩色散点 + 多条彩带会在同一坐标系里互相
    // 穿透（这正是「颜色叠成一团」的成因）。规范允许的替代方案之一是把散点
    // 压成中性灰、只让拟合线承担颜色编码——这里就取这条。
    if show_points {
        for (si, s) in series.iter().enumerate() {
            // 散点用**所属组的颜色**：点、线、带是同一实体的三种表达，
            // 颜色必须贯穿（§4.1）。曾经把 ≥3 组的散点压成中性灰，图例却是
            // 彩色，点线对不上，整张图就「不对」了。
            let point_color = s.color.with_alpha(195);
            // 回归/散点画**每一个原始观测**；折线类仍画均值点（曲线连的是均值）。
            let pts: Vec<(f64, f64)> = if (kind.fits() || !connect) && !s.raw.is_empty() {
                s.raw.clone()
            } else {
                s.rows.iter().map(|r| (r.0, r.1)).collect()
            };
            for &(xv, yv) in &pts {
                let p = Pt::new(xpx(&f, xv), ymap(si, yv));
                if !p.x.is_finite() || !p.y.is_finite() || !f.plot.contains(p) {
                    continue;
                }
                draw_marker(
                    ctx,
                    p,
                    point_size,
                    marker,
                    point_color,
                    ctx.theme.point_edge,
                );
                ctx.hits.push(Hit {
                    at: p,
                    title: s.name.clone(),
                    lines: vec![format!("x = {}", fmt(xv)), format!("y = {}", fmt(yv))],
                });
            }
        }
    }

    // G3：误差棒必须注明度量，禁止「不写度量」。放在绘图区左上空白处。
    if error != ErrorKind::None && !kind.fits() && ctx.spec.annotations_visible {
        ctx.text(
            &format!("误差棒 = {}", error.label()),
            Pt::new(f.plot.x + ctx.px(3.0), f.plot.y + ctx.px(2.0)),
            (ctx.theme.font_tick - 0.8).max(5.0),
            ctx.theme.muted,
            HAlign::Left,
            VAlign::Top,
            0.0,
            false,
        );
    }

    // ---- 拟合摘要 ----
    //
    // C1-2：图内必须给方程、R²、p、n。放在**空白象限**（规范要求「文字放空白
    // 象限，不得遮挡点」）：正斜率放左上、负斜率放右下，这两个角与数据错开。
    // 多组时逐组一行，颜色与各自拟合线一致。
    if kind.fits() && !series.is_empty() && ctx.spec.annotations_visible {
        let rising = fits
            .first()
            .and_then(|f| *f)
            .map(|f| f.slope >= 0.0)
            .unwrap_or(true);
        let (x, y, h, v) = if rising {
            (
                f.plot.x + ctx.px(4.0),
                f.plot.y + ctx.px(3.0),
                HAlign::Left,
                VAlign::Top,
            )
        } else {
            (
                f.plot.right() - ctx.px(4.0),
                f.plot.bottom() - ctx.px(3.0),
                HAlign::Right,
                VAlign::Bottom,
            )
        };
        let lh = ctx.px(9.0);
        let mut row = 0.0_f32;
        for (i, cfit) in curves.iter().enumerate() {
            let Some(cf) = cfit else { continue };
            if series.len() > 1 && i >= 4 {
                break; // 组太多就不逐个列了，图例里还有 R²
            }
            // 图上只留规范 C1-2 要求的四样：方程 / R² / p / n。
            // ED50 等参数已含在方程里（`x/3.2` 的 3.2 就是 ED50），再单列一遍是重复；
            // 参数不可辨识的长警告句属于分析说明，挪到检查器里——印在图上既碍观感
            // 也不是图内标注要求的元素。
            // 线性模型给斜率的精确 p（规范 G4：禁 p=0.000，写 < 0.001）。
            let p_str = if ctx.spec.fit_model == crate::core::curve::FitModel::Linear {
                fits.get(i)
                    .and_then(|f| *f)
                    .map(|f| format!("  p {}", p_text(f.p)))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            // 多组时图例已带每组 R²，图内只标 R² 与 p，行行短、不遮数据。
            let txt = if series.len() == 1 {
                format!(
                    "{}    R² = {:.3}    n = {}{}",
                    cf.equation(),
                    cf.r2,
                    cf.n,
                    p_str
                )
            } else {
                format!("{}  R² = {:.3}{}", series[i].name, cf.r2, p_str)
            };
            // 负斜率时从下往上排，避免文字顶到 x 轴标题
            let dy = if rising { row } else { -row };
            ctx.text(
                &txt,
                Pt::new(x, y + dy),
                if series.len() == 1 {
                    ctx.theme.font_legend
                } else {
                    (ctx.theme.font_tick - 0.8).max(5.2)
                },
                if series.len() == 1 {
                    ctx.theme.ink
                } else {
                    series[i].color
                },
                h,
                v,
                0.0,
                false,
            );
            row += lh;
        }
    }

    draw_axes(ctx, &f, &x_title, &y_title);

    // ---- 右侧 Y 轴（双 Y 轴） ----
    // 两条轴各自量纲、各自刻度。规范 G2 不建议双 Y 轴（会掩盖量纲差异），
    // 但这是用户显式要求的选项，图注里必须写明两条轴各自的单位。
    if let Some(y2) = &y2_axis {
        let tick_len = ctx.px(ctx.theme.tick_len);
        let axis_color = ctx.theme.axis;
        ctx.spine(
            Pt::new(f.plot.right(), f.plot.y),
            Pt::new(f.plot.right(), f.plot.bottom()),
            ctx.theme.axis_width,
            axis_color,
        );
        for t in y2.ticks(6) {
            if !t.major {
                continue;
            }
            let y = ypx_axis(&f, y2, t.value);
            ctx.spine(
                Pt::new(f.plot.right(), y),
                Pt::new(f.plot.right() + tick_len, y),
                ctx.theme.axis_width * 0.9,
                axis_color,
            );
            if !t.label.is_empty() {
                ctx.text(
                    &t.label,
                    Pt::new(f.plot.right() + tick_len + ctx.px(3.0), y),
                    ctx.theme.font_tick,
                    ctx.theme.ink,
                    HAlign::Left,
                    VAlign::Middle,
                    0.0,
                    false,
                );
            }
        }
        if !ctx.spec.y2_title.is_empty() {
            ctx.text(
                &ctx.spec.y2_title,
                Pt::new(ctx.w - ctx.px(6.0), f.plot.center().y),
                ctx.theme.font_axis_title,
                ctx.theme.ink,
                HAlign::Center,
                VAlign::Middle,
                -90.0,
                false,
            );
        }
    }

    draw_legend(ctx, &f);
    ctx.plot = f.plot;
    (x, y)
}

// ---------------------------------------------------------------------------
// 分面：逐面板坐标轴
// ---------------------------------------------------------------------------

/// 面板内的 Y 映射：值 → 该面板里的像素。
fn lane_y(lane: &Rect, ax: &crate::plot::axis::Axis, v: f64) -> f32 {
    lane.y + (1.0 - ax.norm(v) as f32) * lane.h
}

/// 一个面板该放几条刻度、标签放不下时丢哪几条。
///
/// 面板高度只有整幅的 1/n，沿用整幅的 6 条刻度会让标签叠在一起；
/// 反过来只画 2 条又会丢掉「中间值」的参照。按高度定条数，
/// 再按**最小字距**过滤，保证任何画布尺寸下都不叠字。
fn facet_ticks(ax: &crate::plot::axis::Axis, lane_h: f32, ctx: &Ctx) -> Vec<Tick> {
    let font = ctx.s * ctx.theme.font_tick;
    // 目标密度按**像素**算，而不是按「整幅的 1/n」：面板矮的时候刻度必须变稀，
    // 但也不能稀到一个面板只剩一个数（那等于没有刻度）。
    let want = ((lane_h / (font * 2.2)) as usize).clamp(2, 6);
    let min_gap = font * 1.5;
    let mut out: Vec<Tick> = Vec::new();
    // 刻度按值升序给出，像素 y 是**递减**的（值越大越靠上），所以间距要取前一条减当前。
    let mut last = f32::INFINITY;
    for t in ax.ticks(want) {
        if !t.major || t.label.is_empty() {
            continue;
        }
        let y = (1.0 - t.norm as f32) * lane_h;
        if last - y < min_gap {
            continue;
        }
        last = y;
        out.push(t);
    }
    out
}

/// 分面的网格与坐标轴：**每个面板一套 Y 刻度**。
///
/// 为什么不能沿用整幅的 `draw_axes`：分面把同一段 Y 值域压进每个面板的高度，
/// 整幅刻度只对第一个面板的位置成立，其余面板的标签会整体错位一整个面板——
/// 读者照标签读出的数是错的。「共享尺度」指的是**值域相同**，不是「只画一套标签」。
/// 竖直网格线跨面板共用（X 共享），水平网格线逐面板画。
fn draw_facet_axes(
    ctx: &mut Ctx,
    f: &Frame,
    lanes: &[Rect],
    axes: &[crate::plot::axis::Axis],
    ticks: &[Vec<Tick>],
    x_title: &str,
    y_title: &str,
) {
    ctx.in_data = false;
    let s = ctx.s;
    let tick_len = s * ctx.theme.tick_len;
    let axis_color = ctx.theme.axis;

    // 竖直网格（X 共享，一画到底）
    if ctx.spec.show_grid {
        let dash: &[f32] = if ctx.theme.grid_dash {
            &[3.0, 3.0]
        } else {
            &[]
        };
        for t in &f.xticks {
            if !t.major || t.label.is_empty() {
                continue;
            }
            let x = xpx(f, t.value);
            ctx.line(
                Pt::new(x, f.plot.y),
                Pt::new(x, f.plot.bottom()),
                0.55,
                ctx.theme.grid,
                dash,
            );
        }
    }

    for (i, lane) in lanes.iter().enumerate() {
        let ax = &axes[i];
        // 左轴（每个面板都要，否则读者不知道该按哪条刻度读）
        ctx.spine(
            Pt::new(f.plot.x, lane.y),
            Pt::new(f.plot.x, lane.bottom()),
            ctx.theme.axis_width,
            axis_color,
        );
        if ctx.theme.closed_frame {
            ctx.spine(
                Pt::new(f.plot.right(), lane.y),
                Pt::new(f.plot.right(), lane.bottom()),
                ctx.theme.axis_width * 0.85,
                ctx.theme.frame,
            );
        }
        for t in &ticks[i] {
            let y = lane_y(lane, ax, t.value);
            ctx.spine(
                Pt::new(f.plot.x, y),
                Pt::new(f.plot.x - tick_len, y),
                ctx.theme.axis_width * 0.9,
                axis_color,
            );
            ctx.text(
                &t.label,
                Pt::new(f.plot.x - tick_len - s * 3.0, y),
                ctx.theme.font_tick,
                ctx.theme.ink,
                HAlign::Right,
                VAlign::Middle,
                0.0,
                false,
            );
            if ctx.spec.show_grid {
                let dash: &[f32] = if ctx.theme.grid_dash {
                    &[3.0, 3.0]
                } else {
                    &[]
                };
                ctx.line(
                    Pt::new(f.plot.x, y),
                    Pt::new(f.plot.right(), y),
                    0.55,
                    ctx.theme.grid,
                    dash,
                );
            }
        }
    }

    // 水平轴的底边只画在最下面一个面板上（也只有它的底部是真正的基线）
    if let Some(last) = lanes.last() {
        ctx.spine(
            Pt::new(f.plot.x, last.bottom()),
            Pt::new(f.plot.right(), last.bottom()),
            ctx.theme.axis_width,
            axis_color,
        );
        for t in &f.xticks {
            let x = xpx(f, t.value);
            ctx.spine(
                Pt::new(x, last.bottom()),
                Pt::new(x, last.bottom() + tick_len),
                ctx.theme.axis_width * 0.9,
                axis_color,
            );
        }
        for t in &f.xticks {
            if !t.major || t.label.is_empty() {
                continue;
            }
            ctx.text(
                &t.label,
                Pt::new(xpx(f, t.value), last.bottom() + tick_len + s * 3.0),
                ctx.theme.font_tick,
                ctx.theme.ink,
                HAlign::Center,
                VAlign::Top,
                0.0,
                false,
            );
        }
    }

    draw_titles(ctx, f, x_title, y_title);
}

/// 分面：多组各占一个小面板，**共享 X 轴与 Y 域**（规范 G8）。
///
/// 这是「多组颜色堆在一张图里互相穿透」的替代方案：读者一眼能比较各组形状，
/// 又不被重叠的颜色搅浑。每组一条曲线 + 自己的散点，左上角写组名。
fn render_faceted_xy(ctx: &mut Ctx, series: Vec<XySeries>, kind: ChartKind) -> (Axis, Axis) {
    let mut xs_all = Vec::new();
    let mut ys_all = Vec::new();
    for s in &series {
        for &(x, y, e, _) in &s.rows {
            xs_all.push(x);
            ys_all.push(y);
            if let Some(e) = e {
                ys_all.push(y + e);
                ys_all.push(y - e);
            }
        }
        for &(x, y) in &s.raw {
            xs_all.push(x);
            ys_all.push(y);
        }
    }
    let x = axis_or_range(ctx.spec.x_log, ctx.spec.x_range, xs_all, false);
    let y = axis_or_range(ctx.spec.y_log, ctx.spec.y_range, ys_all, false);
    let xticks = x.ticks(6);
    let x_title = ctx.spec.x_title.clone();
    let y_title = ctx.spec.y_title.clone();
    let n = series.len();
    // 分面 Y 轴：默认所有面板共用同一个值域（scales = "fixed"，小倍数的标准做法，
    // 同一个高度代表同一个值，才能跨面板比高低）；打开「自由 Y」后每个面板按
    // 自己的数据定量程——只在量级差很多、只想比形状时用，图注要写明。
    let axes: Vec<crate::plot::axis::Axis> = if ctx.spec.facet_free_y {
        series
            .iter()
            .map(|s| {
                let mut v: Vec<f64> = Vec::new();
                for &(xv, yv, e, _) in &s.rows {
                    let _ = xv;
                    v.push(yv);
                    if let Some(e) = e {
                        v.push(yv + e);
                        v.push(yv - e);
                    }
                }
                for &(_, yv) in &s.raw {
                    v.push(yv);
                }
                axis_or_range(ctx.spec.y_log, ctx.spec.y_range, v, false)
            })
            .collect()
    } else {
        vec![y; n]
    };
    // 边距要按「最宽的那条刻度标签」留，否则自由 Y 时高数量级的面板会把标签挤出画布。
    let est_lane = (ctx.h * 0.62 / n as f32).max(ctx.px(10.0));
    let mut margin_ticks: Vec<Tick> = Vec::new();
    for ax in &axes {
        margin_ticks.extend(facet_ticks(ax, est_lane, ctx));
    }
    let f = frame_layout(ctx, x, y, xticks, margin_ticks, None, &x_title, &y_title);

    // 面板间隙必须容得下一条刻度标签：标签以刻度线为中心上下各探出半字高，
    // 间隙小于字高时，上一面板的「0」会和下一面板的顶格标签外框相交。
    let tick_font = ctx.s * ctx.theme.font_tick;
    let gap = (ctx.px(3.0)).max(tick_font * 1.15);
    let row_h = (f.plot.h - gap * (n as f32 - 1.0)).max(ctx.px(12.0)) / n as f32;
    let lanes: Vec<Rect> = (0..n)
        .map(|i| {
            Rect::new(
                f.plot.x,
                f.plot.y + i as f32 * (row_h + gap),
                f.plot.w,
                row_h,
            )
        })
        .collect();
    let tick_sets: Vec<Vec<Tick>> = axes.iter().map(|ax| facet_ticks(ax, row_h, ctx)).collect();
    draw_facet_axes(ctx, &f, &lanes, &axes, &tick_sets, &x_title, &y_title);
    // 坐标轴画完再开数据自检（draw_facet_axes 会把 in_data 置 false）。
    ctx.begin_data(&f);

    let line_w = ctx.spec.line_width as f32;
    let point_size = ctx.spec.point_size as f32;
    let connect = matches!(
        kind,
        ChartKind::Line | ChartKind::SmoothLine | ChartKind::StepLine | ChartKind::AreaLine
    );
    let show_points = ctx.spec.show_points || !connect;

    for (i, s) in series.iter().enumerate() {
        let panel = lanes[i];
        // 每个面板单独自检：点/线跑到隔壁面板里属于布局错误。
        ctx.plot = panel;
        let ax = &axes[i];
        // 每个面板用自己的 y 映射（共享值时即同一值域 → 可直接比较）
        let xmap = |v: f64| f.plot.x + x.norm(v) as f32 * f.plot.w;
        let ymap = |v: f64| lane_y(&panel, ax, v);

        // 面板分隔线（除了第一行顶部）
        if i > 0 {
            ctx.spine(
                Pt::new(f.plot.x, panel.y - gap / 2.0),
                Pt::new(f.plot.right(), panel.y - gap / 2.0),
                0.5,
                ctx.theme.grid,
            );
        }
        // 组名（面板左上，同组色）
        ctx.text(
            &s.name,
            Pt::new(panel.x + ctx.px(3.0), panel.y + ctx.px(1.5)),
            ctx.theme.font_tick,
            s.color,
            HAlign::Left,
            VAlign::Top,
            0.0,
            false,
        );

        // 拟合曲线
        if kind.fits() {
            if let Some(cf) = crate::core::curve::fit_curve(
                ctx.spec.fit_model,
                &if s.raw.len() >= 2 {
                    s.raw.clone()
                } else {
                    s.rows.iter().map(|r| (r.0, r.1)).collect()
                },
            ) {
                let (xlo, xhi) = s
                    .rows
                    .iter()
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), r| {
                        (lo.min(r.0), hi.max(r.0))
                    });
                let mut path = Path::new();
                let mut first = true;
                for k in 0..=48 {
                    let t = k as f64 / 48.0;
                    let xv = xlo + (xhi - xlo) * t;
                    let yv = cf.eval(xv);
                    if !yv.is_finite() {
                        continue;
                    }
                    let p = Pt::new(xmap(xv), ymap(yv));
                    if first {
                        path.move_to(p);
                        first = false;
                    } else {
                        path.line_to(p);
                    }
                }
                if !first {
                    let mut st = Stroke::new((ctx.s * line_w).max(0.6), s.color);
                    st.cap = LineCap::Round;
                    st.join = LineJoin::Round;
                    ctx.scene.stroke(path, st);
                }
            }
        }

        // 折线（只画在自己面板内）。回归/散点**不连点**：连起来会让人以为
        // 相邻观测之间有插值，而真实关系只由拟合线表达（与非分面同口径）。
        let pts: Vec<Pt> = s
            .rows
            .iter()
            .map(|&(xv, yv, _, _)| Pt::new(xmap(xv), ymap(yv)))
            .filter(|p| p.x.is_finite() && p.y.is_finite())
            .collect();
        if connect && pts.len() >= 2 {
            for run in clip::polyline(&pts, panel) {
                let mut path = Path::new();
                path.move_to(run[0]);
                for p in &run[1..] {
                    path.line_to(*p);
                }
                let mut st = Stroke::new((ctx.s * line_w).max(0.5), s.color);
                st.cap = LineCap::Round;
                st.join = LineJoin::Round;
                ctx.scene.stroke(path, st);
            }
        }

        // 散点
        if show_points {
            let dots: Vec<(f64, f64)> = if s.raw.len() >= 2 {
                s.raw.clone()
            } else {
                s.rows.iter().map(|r| (r.0, r.1)).collect()
            };
            for &(xv, yv) in &dots {
                let p = Pt::new(xmap(xv), ymap(yv));
                if panel.contains(p) {
                    draw_marker(
                        ctx,
                        p,
                        point_size,
                        ctx.spec.marker,
                        s.color.with_alpha(195),
                        ctx.theme.point_edge,
                    );
                }
            }
        }
    }

    draw_legend(ctx, &f);
    ctx.plot = f.plot;
    (x, y)
}

/// p 值的排版形式（图注里用 `< 0.001` 而不是一串 0）。
pub(crate) fn p_text(p: f64) -> String {
    if !p.is_finite() {
        return "= —".into();
    }
    if p < 0.001 {
        "< 0.001".into()
    } else if p < 0.01 {
        format!("= {p:.3}")
    } else {
        format!("= {p:.2}")
    }
}

fn axis_or_range(log: bool, range: Option<[f64; 2]>, values: Vec<f64>, include_zero: bool) -> Axis {
    let mut axis = Axis::from_values(values, log, include_zero);
    if let Some([lo, hi]) = range {
        if hi > lo && lo.is_finite() && hi.is_finite() {
            axis.min = lo;
            axis.max = hi;
            axis.log = log;
        }
    }
    axis
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
// 分类图：柱状 / 箱线 / 小提琴
// ---------------------------------------------------------------------------

/// 小提琴的几何：**观测范围**内的核密度（截断式，不回推尾巴）。
///
/// 口径对齐主流实现：ggplot2 `geom_violin(trim = TRUE)`（默认）与
/// seaborn `violinplot(cut = 0)` —— 密度只在 `[min, max]` 上估计，
/// 两端用余弦窗收敛到零。这样得到三个硬性质：
///
/// 1. 轮廓永远不超出数据范围，也就永远不可能越出 Y 轴；
/// 2. 两端是自然收尖的，不是被坐标框「切平」的；
/// 3. Y 轴只需要按数据范围定，不会被看不见的核尾巴撑高。
///
/// 反过来的做法（`cut = 2`，把核外推两个带宽）会让小提琴看起来像在
/// 数据范围之外还有观测，属于误导读者的画法，这里不采用。
pub(crate) struct ViolinShape {
    /// 可见支撑（= 该组观测的最小 / 最大值）
    pub lo: f64,
    pub hi: f64,
    pub xs: Vec<f64>,
    pub dens: Vec<f64>,
    pub peak: f64,
}

/// 小提琴内嵌箱体的宽度，占小提琴**最宽处**的比例。
///
/// 规范 A1 第 2 条：15%–20%，超过 25% 判不合格。之前取 36%，箱体把琴身
/// 顶掉了一大块，读起来像「挖了个洞」。
const VIOLIN_BOX_FRAC: f32 = 0.175;

/// 画 KDE 的最小样本量（规范 G10 / A1 第 6 条）。
///
/// n < 10 时核密度不稳定（带宽比数据跨度还大），规范要求**禁止**画小提琴，
/// 降级为「原始点 + 中位数与四分位」并显式声明。
pub const KDE_MIN_N: usize = 10;

/// 小提琴叠加原始散点的样本量上限（规范 A1 第 3 条：n < 20 强制必选）。
const VIOLIN_POINTS_MAX_N: usize = 20;

/// 端点收敛窗的宽度（占整段支撑的比例）。
///
/// 窗口太窄会在「尖端」下方留下一个明显的折角肩部；8% 时密度是从端点
/// 平滑升上来的，轮廓不会出现突兀的拐点。
///
/// 密度在数据极值处通常不为零，直接截断会得到平头；用升余弦窗在两端
/// 8% 的区间里把密度平滑压到 0，轮廓就收成尖，同时严格留在观测范围内。
const VIOLIN_TAPER: f64 = 0.08;

pub(crate) fn violin_shape(values: &[f64], grid_n: usize) -> Option<ViolinShape> {
    let vals: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    // G10：n < 10 禁止 KDE。返回 None，由调用方降级成「箱线 + 原始点」。
    if vals.len() < KDE_MIN_N {
        return None;
    }
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for v in &vals {
        lo = lo.min(*v);
        hi = hi.max(*v);
    }
    if !(hi > lo) || !lo.is_finite() || !hi.is_finite() {
        // 全部取值相同：给一个极小的对称区间，退化成一条细线而不是空图。
        let d = lo.abs().max(1.0) * 0.05;
        lo -= d;
        hi += d;
    }

    // 网格严格铺在 [min, max] 上，不做任何外推。
    let steps = grid_n.max(16);
    let grid: Vec<f64> = (0..=steps)
        .map(|i| lo + (hi - lo) * i as f64 / steps as f64)
        .collect();
    let mut dens = stats::kde(&vals, &grid);
    // 余弦窗：t=0 / 1 处权重为 0，向内平滑升到 1，端点因此收成尖。
    for (i, d) in dens.iter_mut().enumerate() {
        let t = i as f64 / steps as f64;
        let w = if t < VIOLIN_TAPER {
            let u = t / VIOLIN_TAPER;
            0.5 - 0.5 * (std::f64::consts::PI * u).cos()
        } else if t > 1.0 - VIOLIN_TAPER {
            let u = (1.0 - t) / VIOLIN_TAPER;
            0.5 - 0.5 * (std::f64::consts::PI * u).cos()
        } else {
            1.0
        };
        *d *= w;
    }
    // 峰值在加窗之后重算：半宽按「实际画出来的最宽处」归一，
    // 小提琴才不会被端点窗口整体削瘦。
    let peak = dens.iter().copied().fold(0.0, f64::max);
    if !(peak > 0.0) {
        return None;
    }
    Some(ViolinShape {
        lo,
        hi,
        xs: grid,
        dens,
        peak,
    })
}

/// 分类图分面（G8）：多组各占一条横向面板、共享 Y 域，每条面板只画一个组。
///
/// 支持「单标记类」图型（柱 / 箱 / 小提琴 / 棒棒糖 / 点阵）；堆叠、前后对比等
/// 必须跨组同框比较的图型不走分面。组名在各面板左上（同组色）。
fn render_faceted_categorical(
    ctx: &mut Ctx,
    groups: Vec<GroupData>,
    kind: ChartKind,
) -> (Axis, Axis) {
    let n = groups.len();
    // Y 域与非分面同口径（含均值 ± SEM 与小提琴支撑）
    let mut yvals = Vec::new();
    for g in &groups {
        yvals.extend(g.values.iter().copied());
        yvals.push(g.desc.mean);
        yvals.push(g.desc.mean + g.desc.sem);
        yvals.push(g.desc.mean - g.desc.sem);
        if kind == ChartKind::Violin {
            if let Some(v) = violin_shape(&g.values, 128) {
                yvals.push(v.lo);
                yvals.push(v.hi);
            }
        }
    }
    let include_zero = matches!(kind, ChartKind::Column | ChartKind::Lollipop);
    let shared_y = axis_or_range(ctx.spec.y_log, ctx.spec.y_range, yvals, include_zero);
    let x = Axis::linear(-0.5, 0.5);
    let y_title = ctx.spec.y_title.clone();
    // 与 XY 分面同口径：默认共享值域（可直接比高低），自由 Y 时逐面板定量程。
    let axes: Vec<crate::plot::axis::Axis> = if ctx.spec.facet_free_y {
        groups
            .iter()
            .map(|g| {
                let mut v = g.values.clone();
                v.push(g.desc.mean + g.desc.sem);
                v.push(g.desc.mean - g.desc.sem);
                if let Some(sh) = violin_shape(&g.values, 64) {
                    v.push(sh.lo);
                    v.push(sh.hi);
                }
                axis_or_range(ctx.spec.y_log, ctx.spec.y_range, v, include_zero)
            })
            .collect()
    } else {
        vec![shared_y; n]
    };
    let est_lane = (ctx.h * 0.62 / n as f32).max(ctx.px(10.0));
    let mut margin_ticks: Vec<Tick> = Vec::new();
    for ax in &axes {
        margin_ticks.extend(facet_ticks(ax, est_lane, ctx));
    }
    let f = frame_layout(
        ctx,
        x,
        shared_y,
        Vec::new(),
        margin_ticks,
        None,
        "",
        &y_title,
    );

    // 面板间隙必须容得下一条刻度标签：标签以刻度线为中心上下各探出半字高，
    // 间隙小于字高时，上一面板的「0」会和下一面板的顶格标签外框相交。
    let tick_font = ctx.s * ctx.theme.font_tick;
    let gap = (ctx.px(3.0)).max(tick_font * 1.15);
    let row_h = (f.plot.h - gap * (n as f32 - 1.0)).max(ctx.px(12.0)) / n as f32;
    let lanes: Vec<Rect> = (0..n)
        .map(|i| {
            Rect::new(
                f.plot.x,
                f.plot.y + i as f32 * (row_h + gap),
                f.plot.w,
                row_h,
            )
        })
        .collect();
    let tick_sets: Vec<Vec<Tick>> = axes.iter().map(|ax| facet_ticks(ax, row_h, ctx)).collect();
    draw_facet_axes(ctx, &f, &lanes, &axes, &tick_sets, "", &y_title);
    // 轴画完再开数据自检（draw_facet_axes 会把 in_data 置 false），否则
    // 分面图里的柱体/琴身会绕过「越界」检查。
    ctx.begin_data(&f);
    let cx = f.plot.center().x;

    for (i, g) in groups.iter().enumerate() {
        let lane = lanes[i];
        // 每个面板单独做越界自检：柱体/琴身跑到隔壁面板里才算越界，
        // 用整幅 f.plot 检查会把「串面板」这种错误放过去。
        ctx.plot = lane;
        let ax = &axes[i];
        let color = ctx.color(i);
        // 每个面板自己的 y 映射：共享值时同一值域压进面板高度，组间仍可直接比较
        // （共享值域直接画会让各组小提琴叠在一起）。
        let ly = |v: f64| lane_y(&lane, ax, v);
        let baseline = ly(0.0_f64.clamp(ax.min, ax.max));
        // 组名（面板左上，同组色）
        ctx.text(
            &g.name,
            Pt::new(lane.x + ctx.px(2.0), lane.y + ctx.px(1.0)),
            ctx.theme.font_tick,
            color,
            HAlign::Left,
            VAlign::Top,
            0.0,
            false,
        );
        let e = err_value(ctx.spec.error, Some(g.desc.sd), g.desc.n);
        let cap = ctx.px(3.0);
        match kind {
            ChartKind::Column | ChartKind::Lollipop => {
                let bw = (row_h * 0.62).min(f.plot.w * 0.30).max(ctx.px(6.0));
                if kind == ChartKind::Column {
                    let ym = ly(g.desc.mean);
                    let rect =
                        Rect::new(cx - bw / 2.0, baseline.min(ym), bw, (baseline - ym).abs());
                    ctx.check_rect("分面柱体", rect, ctx.s * 0.3);
                    ctx.scene.rect(rect, Some(color.with_alpha(226)), None);
                } else {
                    ctx.spine(
                        Pt::new(cx, baseline),
                        Pt::new(cx, ly(g.desc.mean)),
                        (ctx.s * 1.6).max(0.8),
                        color,
                    );
                    draw_marker(
                        ctx,
                        Pt::new(cx, ly(g.desc.mean)),
                        ctx.spec.point_size as f32,
                        ctx.spec.marker,
                        color,
                        true,
                    );
                }
                if let Some(ev) = e {
                    let (y1, y2) = (ly(g.desc.mean + ev), ly(g.desc.mean - ev));
                    ctx.spine(
                        Pt::new(cx, y1),
                        Pt::new(cx, y2),
                        (ctx.s * 1.1).max(0.6),
                        color.darken(0.2),
                    );
                    ctx.spine(
                        Pt::new(cx - cap, y1),
                        Pt::new(cx + cap, y1),
                        (ctx.s * 1.1).max(0.6),
                        color.darken(0.2),
                    );
                    ctx.spine(
                        Pt::new(cx - cap, y2),
                        Pt::new(cx + cap, y2),
                        (ctx.s * 1.1).max(0.6),
                        color.darken(0.2),
                    );
                }
            }
            ChartKind::Box => {
                let bw = (row_h * 0.42).min(f.plot.w * 0.22).max(ctx.px(5.0));
                let mut vs = g.values.clone();
                vs.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let iqr = g.desc.q3 - g.desc.q1;
                let (lo_f, hi_f) = (g.desc.q1 - 1.5 * iqr, g.desc.q3 + 1.5 * iqr);
                let w_lo = vs
                    .iter()
                    .cloned()
                    .find(|v| *v >= lo_f)
                    .unwrap_or(g.desc.min);
                let w_hi = vs
                    .iter()
                    .cloned()
                    .rev()
                    .find(|v| *v <= hi_f)
                    .unwrap_or(g.desc.max);
                // 须 + 帽
                let (y_lo, y_hi) = (ly(w_lo), ly(w_hi));
                ctx.spine(
                    Pt::new(cx, y_lo),
                    Pt::new(cx, y_hi),
                    (ctx.s * 0.9).max(0.5),
                    color.darken(0.2),
                );
                for yy in [y_lo, y_hi] {
                    ctx.spine(
                        Pt::new(cx - bw * 0.35, yy),
                        Pt::new(cx + bw * 0.35, yy),
                        (ctx.s * 0.9).max(0.5),
                        color.darken(0.2),
                    );
                }
                // IQR 箱（与琴身同色口径：同色填充 + 更深描边）+ 中位线
                let (y_q1, y_q3) = (ly(g.desc.q1), ly(g.desc.q3));
                let rect = Rect::new(
                    cx - bw / 2.0,
                    y_q3.min(y_q1),
                    bw,
                    (y_q1 - y_q3).abs().max(1.0),
                );
                ctx.check_rect("分面箱体", rect, ctx.s * 0.5);
                ctx.scene.rect(
                    rect,
                    Some(color.with_alpha(96)),
                    Some(Stroke::new((ctx.s * 0.8).max(0.35), color.darken(0.48))),
                );
                let ym = ly(g.desc.median);
                ctx.spine(
                    Pt::new(cx - bw / 2.0, ym),
                    Pt::new(cx + bw / 2.0, ym),
                    (ctx.s * 1.4).max(0.7),
                    color.darken(0.62),
                );
                // 离群点空心圈
                for &v in vs.iter().filter(|v| **v < lo_f || **v > hi_f) {
                    draw_open_marker(
                        ctx,
                        Pt::new(cx, ly(v)),
                        ctx.spec.point_size as f32 * 0.85,
                        color,
                    );
                }
            }
            ChartKind::Violin => {
                if let Some(shape) = violin_shape(&g.values, 128) {
                    let max_half = (row_h * 0.42).min(f.plot.w * 0.20).max(ctx.px(3.0));
                    let mut poly: Vec<Pt> = Vec::new();
                    for t in 0..=32 {
                        let v = shape.lo + (shape.hi - shape.lo) * t as f64 / 32.0;
                        poly.push(Pt::new(cx - violin_half_at(&shape, v, max_half), ly(v)));
                    }
                    for t in (0..=32).rev() {
                        let v = shape.lo + (shape.hi - shape.lo) * t as f64 / 32.0;
                        poly.push(Pt::new(cx + violin_half_at(&shape, v, max_half), ly(v)));
                    }
                    let clipped = clip::polygon(&poly, f.plot);
                    if clipped.len() >= 3 {
                        let st = Stroke::new((ctx.s * 0.85).max(0.4), color.darken(0.15));
                        ctx.scene.push(
                            Path::polygon(&clipped),
                            Some(color.with_alpha(96)),
                            Some(st),
                        );
                    }
                    // 迷你箱线（须 + IQR 箱 + 中位线）
                    let bw = (max_half * 0.36).clamp(ctx.px(3.0), ctx.px(8.0));
                    let (y_q1, y_q3) = (ly(g.desc.q1), ly(g.desc.q3));
                    ctx.spine(
                        Pt::new(cx, ly(g.desc.min)),
                        Pt::new(cx, ly(g.desc.max)),
                        (ctx.s * 0.8).max(0.4),
                        color.darken(0.5),
                    );
                    let rect = Rect::new(
                        cx - bw / 2.0,
                        y_q3.min(y_q1),
                        bw,
                        (y_q1 - y_q3).abs().max(1.0),
                    );
                    ctx.scene.rect(
                        rect,
                        Some(color.with_alpha(96)),
                        Some(Stroke::new((ctx.s * 0.8).max(0.35), color.darken(0.48))),
                    );
                    let ym = ly(g.desc.median);
                    ctx.spine(
                        Pt::new(cx - bw / 2.0, ym),
                        Pt::new(cx + bw / 2.0, ym),
                        (ctx.s * 1.5).max(0.8),
                        color.darken(0.62),
                    );
                }
            }
            _ => {
                // DotPlot：点 + 误差棒
                if let Some(ev) = e {
                    let (y1, y2) = (ly(g.desc.mean + ev), ly(g.desc.mean - ev));
                    ctx.spine(
                        Pt::new(cx, y1),
                        Pt::new(cx, y2),
                        (ctx.s * 1.1).max(0.6),
                        color.darken(0.2),
                    );
                }
                draw_marker(
                    ctx,
                    Pt::new(cx, ly(g.desc.mean)),
                    ctx.spec.point_size as f32,
                    ctx.spec.marker,
                    color,
                    true,
                );
            }
        }
        // 原始观测（A3-5：n ≤ 30 必须画；或用户勾选）——抖动约束在面板内
        if (ctx.spec.show_points || g.desc.n <= 30) && kind != ChartKind::Violin
            || (kind == ChartKind::Violin && g.desc.n < 20)
        {
            let jitter_w = (row_h * 0.22).min(f.plot.w * 0.12);
            for (row, v) in g.values.iter().enumerate() {
                let jx = crate::core::stats::jitter(row, i as f64 * 3.7) as f32 * jitter_w;
                let p = Pt::new(cx + jx, ly(*v));
                if lane.contains(p) {
                    draw_marker(
                        ctx,
                        p,
                        ctx.spec.point_size as f32 * 0.7,
                        crate::core::graph::Marker::Circle,
                        color.with_alpha(170),
                        true,
                    );
                }
            }
        }
    }

    // 自检时逐面板用过 ctx.plot；对外仍报整幅绘图区。
    ctx.plot = f.plot;
    (f.x, f.y)
}

fn render_categorical(ctx: &mut Ctx, table: &DataTable, kind: ChartKind) -> (Axis, Axis) {
    let groups = build_groups(ctx, table);
    if groups.is_empty() {
        return fallback(ctx, kind.requires());
    }
    // 分面（G8）：单标记类图型多组时逐组小面板；跨组同框才有意义的图型不拆。
    // 开不开由**开关**决定：规范 G8 建议 > 6 组时分面，但强制分面会让用户
    // 无法回到「同框对比」，所以只在检查器里提示，不替用户做主。
    if ctx.spec.facet
        && groups.len() > 1
        && matches!(
            kind,
            ChartKind::Column
                | ChartKind::Box
                | ChartKind::Violin
                | ChartKind::Lollipop
                | ChartKind::DotPlot
        )
    {
        return render_faceted_categorical(ctx, groups, kind);
    }
    let n = groups.len();
    let x = Axis::linear(-0.5, n as f64 - 0.5);
    let bar_w = ctx.spec.bar_width.clamp(0.18, 0.95) as f32;

    // 形状先算：小提琴的支撑等于该组观测范围，Y 轴只需要按数据定，
    // 不必再为「看不见的核尾巴」预留空间。
    let violins: Vec<Option<ViolinShape>> = if kind == ChartKind::Violin {
        groups
            .iter()
            .map(|g| violin_shape(&g.values, 128))
            .collect()
    } else {
        (0..n).map(|_| None).collect()
    };

    let mut yvals = Vec::new();
    for g in &groups {
        yvals.extend(g.values.iter().copied());
        yvals.push(g.desc.mean);
        yvals.push(g.desc.mean + g.desc.sem);
        yvals.push(g.desc.mean - g.desc.sem);
    }
    // 支撑 = 观测范围，已包含在 yvals 里；只需补上「全相同样本」退化时
    // 那一小段对称区间，其余情况不额外撑大坐标轴。
    for v in violins.iter().flatten() {
        yvals.push(v.lo);
        yvals.push(v.hi);
    }
    let include_zero = matches!(kind, ChartKind::Column | ChartKind::Lollipop);
    let y = axis_or_range(ctx.spec.y_log, ctx.spec.y_range, yvals, include_zero);

    // 图注统计行（G4/G3）：检验全名、双侧、校正、效应量与 95% CI，外加各组 n。
    // 只在画显著性括号时给（括号的星号本身不构成完整报告）。
    if ctx.spec.annotations_visible && !ctx.spec.annotations.is_empty() {
        let pairs: Vec<(String, Vec<f64>)> = groups
            .iter()
            .map(|g| (g.name.clone(), g.values.clone()))
            .collect();
        if let Some(note) = crate::core::analysis::inference_note(&pairs) {
            let ns: Vec<String> = groups.iter().map(|g| g.desc.n.to_string()).collect();
            ctx.set_note(format!("{} · n = {}", note, ns.join(", ")));
        }
    }

    // 图形本体的最高点：含误差棒、小提琴尾巴、箱线须。
    // 显著性括号要放在它之上，否则会压在小提琴的尖上。
    let mut data_max = f64::NEG_INFINITY;
    for (i, g) in groups.iter().enumerate() {
        let mut t = g.desc.max.max(g.desc.mean + g.desc.sem);
        if let Some(v) = &violins[i] {
            t = t.max(v.hi);
        }
        if kind == ChartKind::Box {
            let iqr = g.desc.q3 - g.desc.q1;
            t = t.max(g.desc.max.min(g.desc.q3 + 1.5 * iqr));
        }
        data_max = data_max.max(t);
    }
    let want_ann = ctx.spec.annotations_visible
        && !ctx.spec.annotations.is_empty()
        && ctx.spec.y_range.is_none();

    let labels: Vec<String> = groups.iter().map(|g| g.name.clone()).collect();
    ctx.legend.clear();
    let mut f = frame_layout(
        ctx,
        x,
        y,
        Vec::new(),
        y.ticks(6),
        Some(labels.clone()),
        "",
        &ctx.spec.y_title.clone(),
    );
    // 显著性括号按「每层 step px」预留高度。
    //
    // 这里必须**迭代**：扩轴会改变 y 跨度，进而改变「1 个数据单位 = 多少像素」，
    // 用扩轴前的比例一次算完总会差一点点——最上面那条括号就会顶到框上。
    // 每次按当前布局重新算「还差多少像素」，最多三轮即收敛。
    if want_ann {
        for _ in 0..3 {
            let n = ctx.spec.annotations.len() as f32;
            // 层间距 + 数据顶到第一层的间距 + 最上层星号自身的高度
            let need_px = sig_step(ctx) * (n - 1.0) + ctx.px(5.0) + sig_step(ctx) * 1.15;
            let span = (f.y.max - f.y.min).max(1e-9) as f32;
            let px_per_val = f.plot.h / span;
            if !(px_per_val > 0.0) {
                break;
            }
            let have_px = (f.y.max - data_max).max(0.0) as f32 * px_per_val;
            if have_px + 0.5 >= need_px {
                break;
            }
            f.y.max = data_max + (need_px / px_per_val) as f64;
            f = frame_layout(
                ctx,
                x,
                f.y,
                Vec::new(),
                f.y.ticks(6),
                Some(labels.clone()),
                "",
                &ctx.spec.y_title.clone(),
            );
        }
    }
    draw_grid(ctx, &f);
    ctx.begin_data(&f);

    let bar_alpha = ctx.theme.bar_alpha;
    let error = ctx.spec.error;
    let slot = f.plot.w / n.max(1) as f32;

    for (i, g) in groups.iter().enumerate() {
        let cx = xpx(&f, i as f64);
        let color = g.color;
        match kind {
            ChartKind::Column => {
                let half = bar_w * 0.5 * slot;
                let y0 = ypx(&f, 0.0_f64.clamp(f.y.min, f.y.max));
                let ym = ypx(&f, g.desc.mean);
                let rect = Rect::new(cx - half, y0.min(ym), half * 2.0, (y0 - ym).abs());
                ctx.check_rect("柱体", rect, ctx.s * 0.35);
                let mut stroke = Stroke::new((ctx.s * 0.7).max(0.3), color.darken(0.18));
                stroke.join = LineJoin::Miter;
                ctx.scene
                    .rect(rect, Some(color.with_alpha(bar_alpha)), Some(stroke));
                if let Some(e) = err_value(error, Some(g.desc.sd), g.desc.n) {
                    let epx = (ypx(&f, g.desc.mean + e) - ym).abs();
                    draw_error_bar(ctx, cx, ym, epx, color.darken(0.25), 1.0, 4.5);
                }
                if ctx.spec.show_points {
                    let jitter_amp = half * 0.9;
                    for (row, v) in g.values.iter().enumerate() {
                        let jx = stats::jitter(row, i as f64 * 3.7) as f32 * jitter_amp;
                        let p = Pt::new(cx + jx, ypx(&f, *v));
                        if p.y < f.plot.y - 1.0 || p.y > f.plot.bottom() + 1.0 {
                            continue;
                        }
                        draw_marker(
                            ctx,
                            p,
                            ctx.spec.point_size as f32 * 0.78,
                            Marker::Circle,
                            color.with_alpha(205),
                            true,
                        );
                    }
                }
                ctx.hits.push(Hit {
                    at: Pt::new(cx, ym),
                    title: g.name.clone(),
                    lines: vec![
                        format!("n = {}", g.desc.n),
                        format!("mean = {} ± {}", fmt(g.desc.mean), fmt(g.desc.sem)),
                        format!("median = {}", fmt(g.desc.median)),
                    ],
                });
            }
            ChartKind::Lollipop => {
                // 基线 → 均值一根细杆，端点画实心点；误差棒叠在点上。
                let y0 = ypx(&f, 0.0_f64.clamp(f.y.min, f.y.max));
                let ym = ypx(&f, g.desc.mean);
                ctx.line(
                    Pt::new(cx, y0),
                    Pt::new(cx, ym),
                    1.6,
                    color.with_alpha(190),
                    &[],
                );
                draw_marker(
                    ctx,
                    Pt::new(cx, ym),
                    ctx.spec.point_size as f32 * 1.1,
                    Marker::Circle,
                    color,
                    true,
                );
                if let Some(e) = err_value(error, Some(g.desc.sd), g.desc.n) {
                    let epx = (ypx(&f, g.desc.mean + e) - ym).abs();
                    draw_error_bar(ctx, cx, ym, epx, color.darken(0.25), 0.9, 3.4);
                }
                ctx.hits.push(Hit {
                    at: Pt::new(cx, ym),
                    title: g.name.clone(),
                    lines: vec![
                        format!("n = {}", g.desc.n),
                        format!("mean = {} ± {}", fmt(g.desc.mean), fmt(g.desc.sem)),
                    ],
                });
            }
            ChartKind::DotPlot => {
                draw_dot_plot(ctx, &f, cx, g, color, slot);
            }
            ChartKind::Box => {
                draw_box(ctx, &f, cx, g, color, n);
                // A3-5：每组 n ≤ 30 必须叠加抖动原始点。
                if g.values.len() <= 30 && ctx.spec.show_points {
                    draw_jitter_points(ctx, &f, cx, g, color, slot * 0.16, i as f64 * 3.7);
                }
                ctx.hits.push(Hit {
                    at: Pt::new(cx, ypx(&f, g.desc.median)),
                    title: g.name.clone(),
                    lines: vec![
                        format!("n = {}", g.desc.n),
                        format!("median = {}", fmt(g.desc.median)),
                        format!("Q1–Q3 = {} – {}", fmt(g.desc.q1), fmt(g.desc.q3)),
                    ],
                });
            }
            ChartKind::Violin => {
                if let Some(shape) = &violins[i] {
                    draw_violin(ctx, &f, cx, g, shape, color, bar_w, slot);
                    // A1-3：n < 20 必须叠原始散点，且点要被约束在轮廓内。
                    if g.values.len() < VIOLIN_POINTS_MAX_N {
                        let half = violin_half(slot, bar_w);
                        for (row, v) in g.values.iter().enumerate() {
                            // 该值处琴身的半宽（按密度插值），散点只落在这段里。
                            let w = violin_half_at(shape, *v, half);
                            // 连点自身的半径一起扣掉，保证整个圆都落在轮廓内
                            //（A1-3 `constrain_to_violin`）。
                            let dot_r = ctx.px(ctx.spec.point_size as f32 * 0.55) / 2.0;
                            let room = (w - dot_r).max(0.0);
                            let jx = stats::jitter(row, i as f64 * 5.1) as f32 * room * 0.9;
                            let p = Pt::new(cx + jx, ypx(&f, *v));
                            if p.y < f.plot.y - 1.0 || p.y > f.plot.bottom() + 1.0 {
                                continue;
                            }
                            draw_marker(
                                ctx,
                                p,
                                ctx.spec.point_size as f32 * 0.55,
                                Marker::Circle,
                                Rgba::rgba(0x33, 0x33, 0x33, 170),
                                false,
                            );
                        }
                    }
                    ctx.hits.push(Hit {
                        at: Pt::new(cx, ypx(&f, g.desc.median)),
                        title: g.name.clone(),
                        lines: vec![
                            format!("n = {}", g.desc.n),
                            format!("median = {}", fmt(g.desc.median)),
                            format!(
                                "范围 = {} – {}",
                                fmt(shape.lo.max(g.desc.min)),
                                fmt(shape.hi.min(g.desc.max))
                            ),
                        ],
                    });
                } else {
                    // G10 降级：n < 10（或数据退化）不画 KDE，改「箱线 + 原始点」。
                    draw_box(ctx, &f, cx, g, color, n);
                    draw_jitter_points(ctx, &f, cx, g, color, slot * 0.16, i as f64 * 3.7);
                }
            }
            _ => {}
        }
    }

    // G10：只要有一组 n < 10，就在图上写明降级原因（不允许静默降级）。
    if kind == ChartKind::Violin && ctx.spec.annotations_visible {
        let bad = groups.iter().filter(|g| g.values.len() < KDE_MIN_N).count();
        if bad > 0 {
            ctx.text(
                &format!("n < {KDE_MIN_N}：{bad} 组密度不稳定，已降级为箱线 + 原始点"),
                Pt::new(f.plot.x + ctx.px(3.0), f.plot.y + ctx.px(2.0)),
                ctx.theme.font_tick - 0.8,
                ctx.theme.muted,
                HAlign::Left,
                VAlign::Top,
                0.0,
                false,
            );
        }
    }

    let y_title = ctx.spec.y_title.clone();
    draw_axes(ctx, &f, "", &y_title);
    if ctx.spec.annotations_visible && !ctx.spec.annotations.is_empty() {
        draw_significance(ctx, &f, &labels, data_max);
    }
    ctx.plot = f.plot;
    (x, f.y)
}

/// 在一组数据上叠加抖动散点。
///
/// 规范 G3：每组 n ≤ 30 必须展示每一个原始观测；G7：抖动必须是确定性的
/// （`stats::jitter` 是哈希实现的伪随机，同一个值永远落在同一处）。
fn draw_jitter_points(
    ctx: &mut Ctx,
    f: &Frame,
    cx: f32,
    g: &GroupData,
    color: Rgba,
    half_w: f32,
    seed: f64,
) {
    if g.values.is_empty() {
        return;
    }
    for (row, v) in g.values.iter().enumerate() {
        let jx = stats::jitter(row, seed) as f32 * half_w;
        let p = Pt::new(cx + jx, ypx(f, *v));
        if p.y < f.plot.y - 1.0 || p.y > f.plot.bottom() + 1.0 {
            continue;
        }
        draw_marker(
            ctx,
            p,
            ctx.spec.point_size as f32 * 0.72,
            Marker::Circle,
            color.with_alpha(205),
            true,
        );
    }
}

/// 点阵图（Wilkinson dot plot）：按 Y 值分箱，箱内每个观测画一个点、左右对称堆叠。
///
/// 分箱宽度取「一个点的直径」，保证点不重叠；堆叠方向优先向外扩，
/// 超过槽宽时改为减小点径，而不是让点跑到相邻组里去。
fn draw_dot_plot(ctx: &mut Ctx, f: &Frame, cx: f32, g: &GroupData, color: Rgba, slot: f32) {
    if g.values.is_empty() {
        return;
    }
    let dot_pt = (ctx.spec.point_size as f32 * 0.9).clamp(2.0, 6.0);
    let dot_px = ctx.px(dot_pt);
    // Y 方向一个点的直径对应的数据高度
    let per_px = (f.y.max - f.y.min) / f.plot.h.max(1.0) as f64;
    let bin_h = (dot_px * 1.02) as f64 * per_px;
    if !(bin_h > 0.0) || !bin_h.is_finite() {
        return;
    }
    let mut bins: Vec<(f64, usize)> = Vec::new();
    let mut sorted = g.values.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    for v in &sorted {
        // 同一个箱用「箱中心」代表，避免相邻值各自成箱
        let key = (v / bin_h).floor();
        match bins.last_mut() {
            Some((k, c)) if (*k / bin_h).floor() == key => *c += 1,
            _ => bins.push((*v, 1)),
        }
    }
    let max_stack = bins.iter().map(|b| b.1).max().unwrap_or(1);
    let step = (dot_px * 0.86).max(1.0);
    let usable = (slot * 0.86).max(dot_px * 2.0);
    let step = step
        .min(usable / max_stack.max(1) as f32)
        .max(dot_px * 0.55);

    for (v, count) in &bins {
        let y = ypx(f, *v);
        if y < f.plot.y - dot_px || y > f.plot.bottom() + dot_px {
            continue;
        }
        for k in 0..*count {
            // 0, +1, -1, +2, -2 …
            let side = if k == 0 {
                0.0
            } else if k % 2 == 1 {
                ((k + 1) / 2) as f32
            } else {
                -((k / 2) as f32)
            };
            let p = Pt::new(cx + side * step, y);
            if p.x < f.plot.x - dot_px || p.x > f.plot.right() + dot_px {
                continue;
            }
            draw_marker(ctx, p, dot_pt, Marker::Circle, color.with_alpha(215), true);
        }
    }
    ctx.hits.push(Hit {
        at: Pt::new(cx, ypx(f, g.desc.median)),
        title: g.name.clone(),
        lines: vec![
            format!("n = {}", g.desc.n),
            format!("median = {}", fmt(g.desc.median)),
        ],
    });
}

/// Tukey 箱线图（Prism / ggplot2 口径）。
///
/// 两条硬规矩：
/// 1. 须只能延伸到**栅栏内最远的实际观测点**，不能停在 `Q1−1.5·IQR` 这个
///    计算出来的栅栏值上（那里往往根本没有数据）；
/// 2. 上须从 Q3 往上、下须从 Q1 往下——两者接反会让竖线穿过整个箱体。
fn draw_box(ctx: &mut Ctx, f: &Frame, cx: f32, g: &GroupData, color: Rgba, n: usize) {
    let width = (f.plot.w / n.max(1) as f32) * 0.52;
    let half = width / 2.0;
    let iqr = g.desc.q3 - g.desc.q1;
    let fence_lo = g.desc.q1 - 1.5 * iqr;
    let fence_hi = g.desc.q3 + 1.5 * iqr;
    let mut whisker_lo = g.desc.q1;
    let mut whisker_hi = g.desc.q3;
    for v in &g.values {
        if *v >= fence_lo {
            whisker_lo = whisker_lo.min(*v);
        }
        if *v <= fence_hi {
            whisker_hi = whisker_hi.max(*v);
        }
    }

    let q1 = ypx(f, g.desc.q1);
    let q3 = ypx(f, g.desc.q3);
    let med = ypx(f, g.desc.median);
    let wlo = ypx(f, whisker_lo);
    let whi = ypx(f, whisker_hi);
    let edge = color.darken(0.1);

    // 上须：Q3 → 上须端；下须：Q1 → 下须端。
    ctx.line(Pt::new(cx, q3), Pt::new(cx, whi), 1.0, edge, &[]);
    ctx.line(Pt::new(cx, q1), Pt::new(cx, wlo), 1.0, edge, &[]);
    // 须端横帽
    ctx.line(
        Pt::new(cx - half * 0.5, whi),
        Pt::new(cx + half * 0.5, whi),
        1.0,
        edge,
        &[],
    );
    ctx.line(
        Pt::new(cx - half * 0.5, wlo),
        Pt::new(cx + half * 0.5, wlo),
        1.0,
        edge,
        &[],
    );

    // 箱体 Q1–Q3 + 中位数线
    let rect = Rect::new(cx - half, q3.min(q1), width, (q1 - q3).abs().max(1.0));
    ctx.check_rect("箱体", rect, ctx.s * 0.5);
    ctx.scene.rect(
        rect,
        Some(color.with_alpha(70)),
        Some(Stroke::new(1.0, color.darken(0.2))),
    );
    // 中位数线必须**止于箱体内沿**：描边是居中绘制的，2pt 的线画到
    // `cx ± half` 会各多出 1pt，看起来比箱体还宽（曾经的真实缺陷）。
    let med_inset = ctx.px(2.0) / 2.0;
    let med_a = Pt::new(cx - half + med_inset, med);
    let med_b = Pt::new(cx + half - med_inset, med);
    ctx.line(med_a, med_b, 2.0, color.darken(0.2), &[]);

    // 栅栏外的观测单独画成离群点：**空心圈**，和数据点区分开。
    for v in &g.values {
        if *v < fence_lo - 1e-9 || *v > fence_hi + 1e-9 {
            draw_open_marker(ctx, Pt::new(cx, ypx(f, *v)), 2.6, color.darken(0.15));
        }
    }
}

/// 某个数据值处小提琴的半宽（像素）。
///
/// 散点必须被约束在轮廓内（规范 A1 第 3 条 `constrain_to_violin`），
/// 所以在该值对应的密度点上按比例取宽度，而不是全用最大半宽。
fn violin_half_at(shape: &ViolinShape, value: f64, max_half: f32) -> f32 {
    let n = shape.xs.len();
    if n < 2 {
        return 0.0;
    }
    let t = (value - shape.lo) / (shape.hi - shape.lo).max(1e-9);
    let idx = (t.clamp(0.0, 1.0) * (n - 1) as f64).round() as usize;
    let d = shape.dens.get(idx).copied().unwrap_or(0.0);
    // 不给最小宽度：端点处密度本来就该收到 0，硬塞 12% 会让散点跑出轮廓。
    ((d / shape.peak) as f32 * max_half).max(0.0)
}

/// 小提琴的最大半宽（像素）。箱体宽度与散点约束都从这里取，避免两处口径不一致。
fn violin_half(slot: f32, width_frac: f32) -> f32 {
    // A1：最大半宽 ≤ 0.4 个类目间距，避免相邻小提琴相撞。
    (slot * width_frac * 0.5).min(slot * 0.40).max(1.5)
}

fn draw_violin(
    ctx: &mut Ctx,
    f: &Frame,
    cx: f32,
    g: &GroupData,
    shape: &ViolinShape,
    color: Rgba,
    width_frac: f32,
    slot: f32,
) {
    // 半宽以「组槽宽」为基准：组多了自动变窄，不会互相压住。
    let half = violin_half(slot, width_frac);
    let mut outline: Vec<Pt> = Vec::with_capacity(shape.xs.len() * 2 + 2);
    let mut right: Vec<Pt> = Vec::with_capacity(shape.xs.len());
    for (i, xv) in shape.xs.iter().enumerate() {
        let y = ypx(f, *xv);
        if !y.is_finite() {
            continue;
        }
        let w = (shape.dens[i] / shape.peak) as f32 * half;
        outline.push(Pt::new(cx - w, y));
        right.push(Pt::new(cx + w, y));
    }
    right.reverse();
    outline.extend(right);
    // 轮廓可能超出坐标框（用户手动设了范围、或支撑略宽），先裁剪再画。
    ctx.check_poly("小提琴轮廓", &outline);
    let clipped = clip::polygon(&outline, f.plot);
    if clipped.len() >= 3 {
        let mut stroke = Stroke::new((ctx.s * 0.85).max(0.4), color.darken(0.15));
        stroke.join = LineJoin::Round;
        ctx.scene.push(
            Path::polygon(&clipped),
            Some(color.with_alpha(96)),
            Some(stroke),
        );
    }

    // ---- 内嵌迷你箱线图 ----
    //
    // 小提琴只表达「密度形状」，读者仍然需要中位数 / IQR / 范围这几个数。
    // 之前这里只画了一块纯白矩形，等于在琴身上挖了个洞，箱线图元素全丢了。
    // 现在按 Prism 的做法画完整的一套：须线 + IQR 箱 + 中位线 + 离群点。
    let iqr = g.desc.q3 - g.desc.q1;
    let fence_lo = g.desc.q1 - 1.5 * iqr;
    let fence_hi = g.desc.q3 + 1.5 * iqr;
    let mut wlo = g.desc.q1;
    let mut whi = g.desc.q3;
    for v in &g.values {
        if *v >= fence_lo {
            wlo = wlo.min(*v);
        }
        if *v <= fence_hi {
            whi = whi.max(*v);
        }
    }

    // 箱体宽度跟着琴身走：最多占最宽处的 36%，但不小于 3px、不大于 8px，
    // 组多的时候不会糊成一团，组少的时候也不会变成一根针。
    let bw = (half * VIOLIN_BOX_FRAC).clamp(ctx.px(1.2), ctx.px(5.0));
    let ink = color.darken(0.35);

    // ① 须线：从下须端穿过琴身到上须端。先画，随后被箱体压住中段。
    let (wlo_px, whi_px) = (ypx(f, wlo), ypx(f, whi));
    if wlo_px.is_finite() && whi_px.is_finite() {
        ctx.line(
            Pt::new(cx, wlo_px),
            Pt::new(cx, whi_px),
            (ctx.s * 0.7).max(0.35),
            ink.with_alpha(190),
            &[],
        );
        // 须端横帽（半个箱宽），读得出「范围到此为止」
        for y in [wlo_px, whi_px] {
            ctx.line(
                Pt::new(cx - bw * 0.55, y),
                Pt::new(cx + bw * 0.55, y),
                (ctx.s * 0.7).max(0.35),
                ink.with_alpha(190),
                &[],
            );
        }
    }

    // ② IQR 箱：Q1–Q3。
    //
    // 填充与琴身**同色**（不再用白块，白块会读成「挖了个洞」），
    // 只把边框加深一档来区分内外；中位线再深一档、粗一档。
    let q1 = ypx(f, g.desc.q1);
    let q3 = ypx(f, g.desc.q3);
    if q1.is_finite() && q3.is_finite() {
        let box_rect = Rect::new(cx - bw, q3.min(q1), bw * 2.0, (q1 - q3).abs().max(1.0));
        ctx.check_rect("四分位箱", box_rect, ctx.s * 0.5);
        ctx.scene.rect(
            box_rect,
            Some(color.with_alpha(96)),
            Some(Stroke::new((ctx.s * 0.8).max(0.35), color.darken(0.48))),
        );
    }

    // ③ 中位线：横穿箱体，比箱框粗一档
    let med = ypx(f, g.desc.median);
    if med.is_finite() {
        let inset = ctx.px(0.7);
        ctx.line(
            Pt::new(cx - bw + inset, med),
            Pt::new(cx + bw - inset, med),
            (ctx.s * 1.5).max(0.8),
            color.darken(0.62),
            &[],
        );
    }

    // ④ 离群点：栅栏外单独用空心圈标出
    for v in &g.values {
        if *v < fence_lo - 1e-9 || *v > fence_hi + 1e-9 {
            draw_open_marker(ctx, Pt::new(cx, ypx(f, *v)), 2.4, ink);
        }
    }
}

/// 显著性括号的层间距（像素）：预留高度与实际绘制必须用同一个值，
/// 否则「算了多少」和「用了多少」会差出一层。
pub(crate) fn sig_step(ctx: &Ctx) -> f32 {
    ctx.px(14.0)
}

fn draw_significance(ctx: &mut Ctx, f: &Frame, labels: &[String], data_max: f64) {
    if labels.len() < 2 {
        return;
    }
    let anns = ctx.spec.annotations.clone();
    if anns.is_empty() {
        return;
    }
    let step = sig_step(ctx);
    let start_y = ypx(f, data_max) - ctx.px(5.0);
    let tick = ctx.px(4.0);
    let ink = ctx.theme.ink;
    for (i, ann) in anns.iter().enumerate() {
        if ann.a >= labels.len() || ann.b >= labels.len() || ann.a == ann.b {
            continue;
        }
        let y = match ann.y {
            Some(v) => ypx(f, v),
            None => start_y - step * i as f32,
        };
        // 守住上边界：屏幕 y 越小越靠上，所以是 `max` 不是 `min`。
        // 用错方向会把所有括号压到同一条线上（曾经的真实 bug）。
        // 余量要够容纳横线上方的星号字高。
        let y = y.max(f.plot.y + ctx.px(12.0));
        if !y.is_finite() {
            continue;
        }
        let xa = xpx(f, ann.a as f64);
        let xb = xpx(f, ann.b as f64);
        ctx.line(Pt::new(xa, y + tick), Pt::new(xa, y), 0.9, ink, &[]);
        ctx.line(Pt::new(xa, y), Pt::new(xb, y), 0.9, ink, &[]);
        ctx.line(Pt::new(xb, y), Pt::new(xb, y + tick), 0.9, ink, &[]);
        ctx.text(
            &ann.label,
            Pt::new((xa + xb) / 2.0, y - ctx.px(2.0)),
            ctx.theme.font_annotation - 0.5,
            ink,
            HAlign::Center,
            VAlign::Baseline,
            0.0,
            true,
        );
    }
}

// ---------------------------------------------------------------------------
// 分组柱状图
// ---------------------------------------------------------------------------

fn render_grouped(ctx: &mut Ctx, table: &DataTable) -> (Axis, Axis) {
    if table.kind != TableKind::Grouped {
        return render_categorical(ctx, table, ChartKind::Column);
    }
    let cols: Vec<&crate::core::model::Column> = table
        .columns
        .iter()
        .filter(|c| c.role == ColRole::Y)
        .collect();
    if cols.is_empty() || table.nrows() == 0 {
        return fallback(ctx, "分组表还没有数据");
    }

    // 行标题相同的行视为同一类别的重复样本。
    let mut cats: Vec<String> = Vec::new();
    let mut rows_of: Vec<Vec<usize>> = Vec::new();
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
    let n_cat = cats.len();
    let mut stats: Vec<Vec<Option<stats::Desc>>> = Vec::new();
    for col in &cols {
        let mut per_cat = Vec::new();
        for rows in &rows_of {
            let v: Vec<f64> = rows
                .iter()
                .filter_map(|&r| col.cells.get(r).and_then(|c| c.value()))
                .collect();
            per_cat.push(stats::describe(&v));
        }
        stats.push(per_cat);
    }

    let mut ymax = 0.0_f64;
    let mut ymin = 0.0_f64;
    for st in &stats {
        for d in st.iter().flatten() {
            ymax = ymax.max(d.max).max(d.mean + d.sem);
            ymin = ymin.min(d.min);
        }
    }
    let x = Axis::linear(-0.5, n_cat as f64 - 0.5);
    let y = axis_or_range(ctx.spec.y_log, ctx.spec.y_range, vec![ymin, ymax], true);
    let yticks = y.ticks(6);

    ctx.legend = cols
        .iter()
        .enumerate()
        .map(|(i, c)| LegendItem {
            label: c.name.clone(),
            color: ctx.color(i),
            kind: LegendKind::Square,
        })
        .collect();
    let f = frame_layout(
        ctx,
        x,
        y,
        Vec::new(),
        yticks,
        Some(cats.clone()),
        "",
        &ctx.spec.y_title.clone(),
    );
    draw_grid(ctx, &f);
    ctx.begin_data(&f);

    let k = cols.len();
    let group_w = ctx.spec.bar_width.clamp(0.25, 0.95) as f32;
    let bar_w = group_w / k as f32;
    let error = ctx.spec.error;
    for cat in 0..n_cat {
        let cx = xpx(&f, cat as f64);
        let slot = f.plot.w / n_cat as f32;
        for (j, col) in cols.iter().enumerate() {
            let color = ctx.color(j);
            let offset = (j as f32 - (k as f32 - 1.0) / 2.0) * bar_w * slot;
            let x0 = cx + offset;
            let Some(d) = stats[j][cat].as_ref() else {
                continue;
            };
            if d.n == 0 {
                continue;
            }
            let y0 = ypx(&f, 0.0_f64.clamp(f.y.min, f.y.max));
            let ym = ypx(&f, d.mean);
            let rect = Rect::new(
                x0 - bar_w * slot * 0.45,
                y0.min(ym),
                bar_w * slot * 0.90,
                (y0 - ym).abs(),
            );
            ctx.check_rect("分组柱体", rect, ctx.s * 0.3);
            ctx.scene.rect(
                rect,
                Some(color.with_alpha(ctx.theme.bar_alpha)),
                Some(Stroke::new((ctx.s * 0.65).max(0.3), color.darken(0.18))),
            );
            if let Some(e) = err_value(error, Some(d.sd), d.n) {
                let epx = (ypx(&f, d.mean + e) - ym).abs();
                draw_error_bar(ctx, x0, ym, epx, color.darken(0.25), 0.9, 3.2);
            }
            if ctx.spec.show_points {
                for (ri, &r) in rows_of[cat].iter().enumerate() {
                    if let Some(v) = col.cells.get(r).and_then(|c| c.value()) {
                        let jx = stats::jitter(ri, j as f64 * 5.3) as f32 * (bar_w * slot * 0.35);
                        draw_marker(
                            ctx,
                            Pt::new(x0 + jx, ypx(&f, v)),
                            ctx.spec.point_size as f32 * 0.7,
                            Marker::Circle,
                            color.with_alpha(190),
                            true,
                        );
                    }
                }
            }
            ctx.hits.push(Hit {
                at: Pt::new(x0, ym),
                title: format!("{} · {}", col.name, cats[cat]),
                lines: vec![
                    format!("n = {}", d.n),
                    format!("mean = {} ± {}", fmt(d.mean), fmt(d.sem)),
                ],
            });
        }
    }

    let y_title = ctx.spec.y_title.clone();
    draw_axes(ctx, &f, "", &y_title);
    draw_legend(ctx, &f);
    ctx.plot = f.plot;
    (x, f.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 小提琴支撑严格等于观测范围() {
        // n ≥ KDE_MIN_N 才允许 KDE（规范 G10）；保持 min/max 不变。
        let v = vec![
            10.0, 10.5, 11.0, 11.4, 12.0, 12.5, 13.0, 14.0, 15.0, 16.0, 18.0,
        ];
        let s = violin_shape(&v, 128).expect("shape");
        // 截断式（trim = TRUE）：不向数据范围之外外推一个像素，
        // 因此轮廓永远不可能跑出按数据范围建立的 Y 轴。
        assert!((s.lo - 10.0).abs() < 1e-12, "lo={}", s.lo);
        assert!((s.hi - 18.0).abs() < 1e-12, "hi={}", s.hi);
        assert!(s.xs.iter().all(|x| *x >= s.lo - 1e-9 && *x <= s.hi + 1e-9));
        // 端点收敛到 0，多边形闭合成尖而不是平头。
        assert_eq!(s.dens[0], 0.0);
        assert_eq!(*s.dens.last().unwrap(), 0.0);
        // 峰值应在数据内部，而不是端点
        let peak_at = s.xs[s
            .dens
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap()
            .0];
        assert!(peak_at > 10.0 && peak_at < 18.0);
    }

    #[test]
    fn 全相同样本不会退化成尖峰() {
        let v = vec![5.0; 12];
        let s = violin_shape(&v, 64).expect("shape");
        assert!(s.hi > s.lo);
        assert!(s.peak.is_finite() && s.peak > 0.0);
    }

    #[test]
    fn 小提琴不会把Y轴撑开() {
        let v = vec![1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 4.2, 4.6, 5.0];
        let s = violin_shape(&v, 128).expect("shape");
        assert!(
            (s.hi - s.lo - 4.0).abs() < 1e-12,
            "support = [{}, {}]",
            s.lo,
            s.hi
        );
    }

    #[test]
    fn 小样本禁止KDE() {
        // G10 / A1-6：n < 10 不允许核密度，调用方必须降级为箱线 + 原始点。
        assert!(violin_shape(&[1.0, 2.0, 3.0, 4.0, 5.0], 128).is_none());
        assert!(violin_shape(&(0..10).map(|i| i as f64).collect::<Vec<_>>(), 128).is_some());
    }

    #[test]
    fn 阶梯线按先横后竖展开() {
        let pts = [Pt::new(0.0, 0.0), Pt::new(10.0, 5.0), Pt::new(20.0, 2.0)];
        let st = step_polyline(&pts);
        assert_eq!(st.len(), 5);
        assert_eq!(st[1], Pt::new(10.0, 0.0));
        assert_eq!(st[2], Pt::new(10.0, 5.0));
    }

    #[test]
    fn 样条穿过原始点() {
        let pts = [Pt::new(0.0, 0.0), Pt::new(10.0, 10.0), Pt::new(20.0, 0.0)];
        let c = monotone_spline(&pts, 8);
        assert!(c.len() > pts.len());
        // 首尾必须精确落在观测点上
        assert_eq!(c[0], pts[0]);
        assert_eq!(*c.last().unwrap(), pts[2]);
    }

    #[test]
    fn 单调数据不会过冲() {
        // Catmull-Rom 在「急升后走平」的数据上会冲出数据范围，
        // 画出不存在的峰；PCHIP 必须严格落在观测值的 y 区间内。
        let pts = [
            Pt::new(0.0, 100.0),
            Pt::new(10.0, 100.0),
            Pt::new(20.0, 0.0),
            Pt::new(30.0, 0.0),
            Pt::new(40.0, 0.0),
        ];
        let c = monotone_spline(&pts, 12);
        let lo = pts.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let hi = pts.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
        for p in &c {
            assert!(p.y >= lo - 0.01 && p.y <= hi + 0.01, "过冲: {p:?}");
        }
        for k in 0..pts.len() {
            assert!(
                c.iter()
                    .any(|p| { (p.y - pts[k].y).abs() < 0.01 && (p.x - pts[k].x).abs() < 0.01 }),
                "未经过点 {k}"
            );
        }
    }

    #[test]
    fn 非递增X退化为折线() {
        let pts = [Pt::new(10.0, 0.0), Pt::new(10.0, 5.0), Pt::new(20.0, 2.0)];
        assert_eq!(monotone_spline(&pts, 8).len(), pts.len());
    }

    #[test]
    fn p值排版() {
        assert_eq!(p_text(0.0002), "< 0.001");
        assert!(p_text(0.004).starts_with("= 0.00"));
    }

    /// 分面的刻度必须落在**自己那个面板**里。
    ///
    /// 这是曾经的真实 bug：面板把整段 Y 值域压进 1/n 的高度，而刻度标签沿用了
    /// 整幅的映射 —— 第二个面板的标签实际落在第一个面板的位置上，读者照标签
    /// 读出的数是错的。判据：每个刻度在面板内，且「值→像素」的比例尺与面板
    /// 高度一致（共享值域时各面板比例尺必须相同）。
    #[test]
    fn 分面刻度落在各自面板内() {
        let spec = GraphSpec::default();
        let fonts = FontBook::load_builtin();
        let ctx = Ctx {
            spec: &spec,
            theme: Theme::for_id(spec.theme),
            fonts: &fonts,
            scene: Scene::new(400.0, 400.0, WHITE),
            s: 2.4,
            w: 400.0,
            h: 400.0,
            colors: vec![Rgba::hex("#0072B2")],
            hits: Vec::new(),
            legend: Vec::new(),
            plot: Rect::default(),
            viol: Vec::new(),
            in_data: false,
            note: None,
            texts: Vec::new(),
            title_boxes: Vec::new(),
            legend_box: None,
        };
        let ax = Axis::linear(0.0, 100.0);
        let lanes = [
            Rect::new(100.0, 20.0, 200.0, 120.0),
            Rect::new(100.0, 150.0, 200.0, 120.0),
        ];
        let ticks = facet_ticks(&ax, lanes[0].h, &ctx);
        assert!(ticks.len() >= 2, "刻度太少：{}", ticks.len());

        for lane in &lanes {
            for t in &ticks {
                let y = lane_y(lane, &ax, t.value);
                assert!(
                    y >= lane.y - 0.01 && y <= lane.bottom() + 0.01,
                    "刻度 {} 落在面板外：y={y} lane=[{}, {}]",
                    t.value,
                    lane.y,
                    lane.bottom()
                );
            }
            // 比例尺：100 个单位对应整个面板高度
            let top = lane_y(lane, &ax, 100.0);
            let bottom = lane_y(lane, &ax, 0.0);
            assert!((bottom - top - lane.h).abs() < 0.01, "比例尺与面板高度不符");
        }
        // 同一个值在两个面板里的**相对**位置一致（共享值域 → 可直接比高低）
        let k0 = (lane_y(&lanes[0], &ax, 42.0) - lanes[0].y) / lanes[0].h;
        let k1 = (lane_y(&lanes[1], &ax, 42.0) - lanes[1].y) / lanes[1].h;
        assert!((k0 - k1).abs() < 1e-6, "{k0} vs {k1}");
        // 标签间距不能让相邻刻度叠字
        let min_gap = ctx.s * ctx.theme.font_tick * 1.5;
        for w in ticks.windows(2) {
            let d = (lane_y(&lanes[0], &ax, w[0].value) - lane_y(&lanes[0], &ax, w[1].value)).abs();
            assert!(d >= min_gap - 0.01, "刻度标签挤在一起：{d} < {min_gap}");
        }
    }
}
