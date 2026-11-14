//! 自绘基础件：连续曲率圆角、表面、文本与按钮。
//!
//! 这里只放「与业务无关」的绘制原语。所有面板都必须通过
//! [`surface`] / [`tile`] / [`pill`] 画底，才能保证圆角阶梯与留白一致。

use std::f32::consts::PI;

use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Response, RichText, Sense, Stroke,
    Ui, Vec2,
};

use crate::app::theme::{radius, text, Palette};

/// 超椭圆指数：`0.5` 是正方形，`1.0` 是正圆。0.72 的曲率变化比圆弧更连续，
/// 小尺寸下几乎看不出差别，大尺寸（卡片 / 窗口）则明显更「顺」。
const EXPONENT: f32 = 0.72;
/// 每段圆角的采样点数：6 段已经看不出折线，再多只会增加顶点数。
const QUALITY: usize = 6;

/// 连续曲率圆角矩形的轮廓点（顺时针）。`radii` 依次是左上、右上、右下、左下。
pub fn smooth_rect_path(rect: Rect, radii: [f32; 4]) -> Vec<Pos2> {
    let max_r = (rect.width().min(rect.height()) * 0.5).max(0.0);
    let r = radii.map(|v| v.clamp(0.0, max_r));
    let mut pts = Vec::with_capacity(4 * (QUALITY + 1));

    let centers = [
        (
            egui::pos2(rect.left() + r[0], rect.top() + r[0]),
            PI,
            1.5 * PI,
            r[0],
        ),
        (
            egui::pos2(rect.right() - r[1], rect.top() + r[1]),
            1.5 * PI,
            2.0 * PI,
            r[1],
        ),
        (
            egui::pos2(rect.right() - r[2], rect.bottom() - r[2]),
            0.0,
            0.5 * PI,
            r[2],
        ),
        (
            egui::pos2(rect.left() + r[3], rect.bottom() - r[3]),
            0.5 * PI,
            PI,
            r[3],
        ),
    ];

    for (c, a0, a1, rr) in centers {
        if rr <= 0.01 {
            pts.push(c);
            continue;
        }
        pts.extend(arc_points(c, rr, a0, a1));
    }
    pts
}

/// 单段圆角采样：超椭圆曲线，曲率连续，比正圆在转角处更「顺」。
pub fn arc_points(center: Pos2, r: f32, a0: f32, a1: f32) -> Vec<Pos2> {
    let mut pts = Vec::with_capacity(QUALITY + 1);
    for i in 0..=QUALITY {
        let t = a0 + (a1 - a0) * (i as f32 / QUALITY as f32);
        let (s, co) = t.sin_cos();
        let x = co.signum() * co.abs().powf(EXPONENT) * r;
        let y = s.signum() * s.abs().powf(EXPONENT) * r;
        pts.push(egui::pos2(center.x + x, center.y + y));
    }
    pts
}

/// 把已经画好的方角内容「啃」成圆角：在四个角填充背景色。
///
/// 位图 / 表格这类只能方角绘制的内容，用它来贴合外层圆角卡片。
pub fn punch_corners(painter: &egui::Painter, rect: Rect, radius: f32, bg: Color32) {
    let r = radius
        .min(rect.width() * 0.5)
        .min(rect.height() * 0.5)
        .max(0.0);
    if r <= 0.6 || bg.a() == 0 {
        return;
    }
    let corners = [
        (
            egui::pos2(rect.left() + r, rect.top() + r),
            PI,
            1.5 * PI,
            rect.left_top(),
        ),
        (
            egui::pos2(rect.right() - r, rect.top() + r),
            1.5 * PI,
            2.0 * PI,
            rect.right_top(),
        ),
        (
            egui::pos2(rect.right() - r, rect.bottom() - r),
            0.0,
            0.5 * PI,
            rect.right_bottom(),
        ),
        (
            egui::pos2(rect.left() + r, rect.bottom() - r),
            0.5 * PI,
            PI,
            rect.left_bottom(),
        ),
    ];
    for (c, a0, a1, corner) in corners {
        let mut pts = Vec::with_capacity(QUALITY + 2);
        pts.push(corner);
        pts.extend(arc_points(c, r, a0, a1));
        painter.add(egui::Shape::convex_polygon(pts, bg, Stroke::NONE));
    }
}

/// 画一块「表面」：圆角矩形填充 + 可选描边（描边永远在内侧，不改变外轮廓）。
pub fn surface(painter: &egui::Painter, rect: Rect, radius: f32, fill: Color32, stroke: Stroke) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let sw = if stroke.is_empty() { 0.0 } else { stroke.width };
    let inset = sw * 0.5;
    let inner = rect.shrink(inset);
    let r = (radius - inset).max(0.0);
    let pts = smooth_rect_path(inner, [r; 4]);
    painter.add(egui::Shape::convex_polygon(pts, fill, stroke));
}

/// 只画轮廓（用于 hover 描边、聚焦环）。
pub fn surface_stroke(painter: &egui::Painter, rect: Rect, radius: f32, stroke: Stroke) {
    let inset = stroke.width * 0.5;
    let pts = smooth_rect_path(rect.shrink(inset), [(radius - inset).max(0.0); 4]);
    painter.add(egui::Shape::closed_line(pts, stroke));
}

/// 柔和投影：同心圆角向外扩散，透明度递减；比单圈描边更像真实光晕。
pub fn soft_shadow(painter: &egui::Painter, rect: Rect, radius: f32, color: Color32) {
    let layers = 4;
    for i in (0..layers).rev() {
        let t = (i + 1) as f32 / layers as f32;
        let grow = 1.0 + t * 5.0;
        let a = (color.a() as f32 * (1.0 - t) * 0.9) as u8;
        if a == 0 {
            continue;
        }
        let c = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a);
        let pts = smooth_rect_path(rect.expand(grow * 0.5), [radius + grow * 0.5; 4]);
        painter.add(egui::Shape::closed_line(pts, Stroke::new(1.0, c)));
    }
}

/// 旧接口：`card()` 等价于 `surface()`。
pub fn card(painter: &egui::Painter, rect: Rect, fill: Color32, stroke: Stroke, radius: f32) {
    surface(painter, rect, radius, fill, stroke);
}

/// 旧的阴影接口。
pub fn shadow(painter: &egui::Painter, rect: Rect, radius: f32, color: Color32) {
    soft_shadow(painter, rect, radius, color);
}

/// 章节标题：上方一条发丝线 + 小号加粗标签。
pub fn section_label(ui: &mut Ui, label: &str, pal: &Palette) {
    section_label_with(ui, label, None, pal);
}

pub fn section_label_with(ui: &mut Ui, label: &str, count: Option<usize>, pal: &Palette) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 16.0), Sense::hover());
    let p = ui.painter();
    p.hline(
        rect.x_range(),
        rect.top() + 0.5,
        Stroke::new(1.0, pal.border_soft),
    );
    p.text(
        egui::pos2(rect.left(), rect.bottom() - 1.0),
        Align2::LEFT_BOTTOM,
        label,
        FontId::proportional(text::LABEL),
        pal.muted,
    );
    if let Some(n) = count {
        p.text(
            egui::pos2(
                rect.left() + measure(p, label, text::LABEL, true) + 6.0,
                rect.bottom() - 1.0,
            ),
            Align2::LEFT_BOTTOM,
            n.to_string(),
            FontId::proportional(text::MICRO),
            pal.faint,
        );
    }
}

/// 1px 分隔线。
pub fn hline(ui: &mut Ui, color: Color32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0, color);
}

/// 单行文本宽度（不换行）。
pub fn measure(painter: &egui::Painter, s: &str, size: f32, _strong: bool) -> f32 {
    painter
        .layout_no_wrap(
            s.to_owned(),
            FontId::proportional(size),
            Color32::PLACEHOLDER,
        )
        .size()
        .x
}

/// 在限定矩形里画一行可能被截断的文字。
pub fn clipped_text(ui: &Ui, rect: Rect, t: &str, font: FontId, color: Color32, align: Align2) {
    let painter = ui.painter_at(rect);
    let galley = painter.layout(t.to_owned(), font, color, rect.width().max(10.0));
    let size = galley.size();
    let pos = Pos2::new(
        match align.x() {
            egui::Align::Min => rect.left(),
            egui::Align::Center => rect.center().x - size.x / 2.0,
            egui::Align::Max => rect.right() - size.x,
        },
        match align.y() {
            egui::Align::Min => rect.top(),
            egui::Align::Center => rect.center().y - size.y / 2.0,
            egui::Align::Max => rect.bottom() - size.y,
        },
    );
    painter.galley(pos, galley, color);
}

/// 在固定矩形里画一行文字；超宽时截断加省略号。
pub fn paint_text_clipped(
    painter: &egui::Painter,
    rect: Rect,
    t: &str,
    font: FontId,
    color: Color32,
    right: bool,
    pad: f32,
) {
    let max = (rect.width() - pad * 2.0).max(6.0);
    let mut galley = painter.layout_no_wrap(t.to_owned(), font.clone(), color);
    if galley.size().x > max {
        let mut s = t.to_owned();
        while !s.is_empty() {
            s.pop();
            let candidate = format!("{s}…");
            let g = painter.layout_no_wrap(candidate, font.clone(), color);
            if g.size().x <= max {
                galley = g;
                break;
            }
        }
    }
    let x = if right {
        rect.right() - pad - galley.size().x
    } else {
        rect.left() + pad
    };
    let y = rect.center().y - galley.size().y / 2.0;
    painter.galley(egui::pos2(x, y), galley, color);
}

/// 透明色。
pub fn alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

/// 在两个颜色之间按比例混合。
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(
        l(a.r(), b.r()),
        l(a.g(), b.g()),
        l(a.b(), b.b()),
        l(a.a(), b.a()),
    )
}

pub fn dot(ui: &Ui, at: Pos2, r: f32, color: Color32) {
    ui.painter().circle_filled(at, r, color);
}

// ---------------------------------------------------------------------------
// 按钮
// ---------------------------------------------------------------------------

/// 统一按钮：自绘，圆角与内边距都走设计令牌。
pub struct Btn<'a> {
    text: &'a str,
    fill: Color32,
    fg: Color32,
    stroke: Stroke,
    radius: f32,
    height: f32,
    min_w: f32,
    strong: bool,
    full: bool,
}

impl<'a> Btn<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            fill: Color32::TRANSPARENT,
            fg: Color32::PLACEHOLDER,
            stroke: Stroke::NONE,
            radius: radius::CONTROL,
            height: 30.0,
            min_w: 0.0,
            strong: false,
            full: false,
        }
    }

    pub fn fill(mut self, c: Color32) -> Self {
        self.fill = c;
        self
    }

    pub fn fg(mut self, c: Color32) -> Self {
        self.fg = c;
        self
    }

    pub fn stroke(mut self, s: Stroke) -> Self {
        self.stroke = s;
        self
    }

    pub fn radius(mut self, r: f32) -> Self {
        self.radius = r;
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    pub fn min_width(mut self, w: f32) -> Self {
        self.min_w = w;
        self
    }

    pub fn strong(mut self, on: bool) -> Self {
        self.strong = on;
        self
    }

    pub fn full(mut self, on: bool) -> Self {
        self.full = on;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let fg = if self.fg == Color32::PLACEHOLDER {
            ui.visuals().text_color()
        } else {
            self.fg
        };
        let galley =
            ui.painter()
                .layout_no_wrap(self.text.to_owned(), FontId::proportional(text::BODY), fg);
        let w = if self.full {
            ui.available_width()
        } else {
            (galley.size().x + 24.0).max(self.min_w)
        };
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, self.height), Sense::click());
        let fill = if resp.hovered() && self.fill.a() > 0 {
            mix(self.fill, fg, 0.06)
        } else {
            self.fill
        };
        if fill.a() > 0 || !self.stroke.is_empty() {
            surface(ui.painter(), rect, self.radius, fill, self.stroke);
        }
        ui.painter().galley(
            rect.center() - galley.size() / 2.0,
            galley,
            if self.strong { fg } else { fg },
        );
        resp
    }
}

/// 图标按钮：固定方寸，hover 才出现底色，保证不挤压相邻元素。
pub fn icon_button(
    ui: &mut Ui,
    icon: crate::app::icons::Icon,
    tip: &str,
    pal: &Palette,
) -> Response {
    let size = 28.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if resp.hovered() {
        surface(ui.painter(), rect, radius::CONTROL, pal.hover, Stroke::NONE);
    }
    let color = if resp.is_pointer_button_down_on() {
        pal.accent
    } else if resp.hovered() {
        pal.ink
    } else {
        pal.muted
    };
    crate::app::icons::draw(ui.painter(), rect.shrink(7.0), icon, color, pal.panel);
    resp.on_hover_text(tip)
}

/// 图型磁贴：3 列等宽，图标 + 短名，选中态用强调色描边。
pub fn kind_tile(
    ui: &mut Ui,
    kind: crate::core::graph::ChartKind,
    selected: bool,
    size: Vec2,
    pal: &Palette,
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let bg = if selected {
        pal.accent_soft
    } else if resp.hovered() {
        pal.hover
    } else {
        pal.panel_alt
    };
    let stroke = if selected {
        Stroke::new(1.0, pal.accent)
    } else {
        Stroke::new(1.0, pal.border)
    };
    // 磁贴嵌在检查器内容区里，圆角比外层卡片小一档，保持同心。
    surface(
        ui.painter(),
        rect,
        radius::nested(radius::CARD, 2.0),
        bg,
        stroke,
    );
    let color = if selected { pal.accent_ink } else { pal.muted };
    let icon_rect = Rect::from_center_size(
        egui::pos2(rect.center().x, rect.top() + size.y * 0.4),
        Vec2::new(28.0, 18.0),
    );
    crate::app::icons::draw(
        ui.painter(),
        icon_rect,
        crate::app::icons::chart_icon(kind),
        color,
        bg,
    );
    ui.painter().text(
        egui::pos2(rect.center().x, rect.bottom() - 10.0),
        Align2::CENTER_CENTER,
        kind.short(),
        FontId::proportional(text::MICRO),
        color,
    );
    resp.on_hover_text(kind.label())
}

pub fn accent_button(ui: &mut Ui, label: &str, pal: &Palette) -> Response {
    Btn::new(label)
        .fill(pal.accent)
        .fg(Color32::WHITE)
        .strong(true)
        .radius(radius::CONTROL)
        .height(30.0)
        .show(ui)
}

pub fn subtle_button(ui: &mut Ui, label: &str, pal: &Palette) -> Response {
    Btn::new(label)
        .fill(pal.panel_alt)
        .fg(pal.ink)
        .stroke(Stroke::new(1.0, pal.border))
        .radius(radius::CONTROL)
        .height(28.0)
        .show(ui)
}

/// 小徽标：文字 + 圆角底。
pub fn chip(ui: &mut Ui, t: &str, fg: Color32, bg: Color32) {
    let galley = ui
        .painter()
        .layout_no_wrap(t.to_owned(), FontId::proportional(text::MICRO), fg);
    let size = Vec2::new(galley.size().x + 12.0, 18.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    surface(ui.painter(), rect, radius::CHIP, bg, Stroke::NONE);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, fg);
}

pub fn tag_text(ui: &mut Ui, t: &str, color: Color32) {
    ui.label(RichText::new(t).size(text::META).color(color));
}

/// 表单区右侧的对齐参考线（调试用：`STATAURA_GRID=1` 时显示）。
pub fn debug_grid(ui: &Ui, rect: Rect, left: f32, right: f32, pal: &Palette) {
    let p = ui.painter_at(rect);
    for x in [left, right] {
        p.vline(x, rect.y_range(), Stroke::new(1.0, alpha(pal.danger, 90)));
    }
}

/// egui 原生圆角：把 `f32` 圆角转成 widget 需要的 `CornerRadius`。
pub fn cr(r: f32) -> CornerRadius {
    CornerRadius::same(r.round().clamp(0.0, 255.0) as u8)
}

/// 让后续控件在水平方向填满，并统一垂直居中。
pub fn center_row<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), crate::app::theme::metric::ROW_H),
        Sense::hover(),
    );
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(Align::Center)),
        |ui| add(ui),
    )
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_min_size(egui::pos2(10.0, 20.0), Vec2::new(120.0, 60.0))
    }

    #[test]
    fn 圆角路径不越界() {
        let r = rect();
        for radius in [0.0, 3.0, 8.0, 16.0, 40.0, 999.0] {
            let pts = smooth_rect_path(r, [radius; 4]);
            assert!(!pts.is_empty());
            for p in pts {
                assert!(p.x >= r.left() - 0.01 && p.x <= r.right() + 0.01, "{p:?}");
                assert!(p.y >= r.top() - 0.01 && p.y <= r.bottom() + 0.01, "{p:?}");
            }
        }
    }

    #[test]
    fn 圆角半径被限制在边长一半以内() {
        // 半径大于一半时会被夹紧，避免自交。
        let r = rect();
        let big = smooth_rect_path(r, [500.0; 4]);
        let half = smooth_rect_path(r, [30.0; 4]); // 60/2 = 30
        assert_eq!(big.len(), half.len());
        for (a, b) in big.iter().zip(half.iter()) {
            assert!((a.x - b.x).abs() < 0.01 && (a.y - b.y).abs() < 0.01);
        }
    }

    #[test]
    fn 零圆角退化为矩形四角() {
        let r = rect();
        let pts = smooth_rect_path(r, [0.0; 4]);
        assert_eq!(pts.len(), 4);
        assert_eq!(pts[0], egui::pos2(r.left(), r.top()));
        assert_eq!(pts[2], egui::pos2(r.right(), r.bottom()));
    }

    #[test]
    fn 连续曲率比正圆更鼓() {
        // 超椭圆在 45° 处落在「正圆之外、外接方形之内」：这正是圆角看起来更
        // 饱满、过渡更连续的原因，同时不会越出矩形。
        let c = egui::pos2(0.0, 0.0);
        let arc = arc_points(c, 10.0, PI, 1.5 * PI);
        let mid = arc[arc.len() / 2];
        let d = (mid.x * mid.x + mid.y * mid.y).sqrt();
        assert!(d > 10.0 * 0.7071 + 0.05, "mid distance {d}");
        assert!(d < 10.0 * 1.4143, "mid distance {d}");
        assert!(mid.x < 0.0 && mid.y < 0.0);
    }
}
