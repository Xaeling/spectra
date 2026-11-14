//! 表单排版：检查器、导出窗口共用同一套「标签列 + 控件列」网格。
//!
//! 规则只有三条，但必须严格遵守，否则就会出现「文字没对齐」：
//! 1. 所有行共用同一个左边界（内容区左边）和同一个控件列起点；
//! 2. 标签右对齐到标签列右边界，控件左对齐到控件列左边界；
//! 3. 行高、间距、字号只从 `theme::metric` / `theme::text` 取。

use std::ops::RangeInclusive;

use egui::{Align, Align2, FontId, Rect, Response, RichText, Sense, Stroke, Ui, Vec2};

use crate::app::theme::{metric, radius, text, Palette};
use crate::app::widgets::{self, surface};

pub struct Form<'a> {
    ui: &'a mut Ui,
    pal: Palette,
}

impl<'a> Form<'a> {
    pub fn new(ui: &'a mut Ui, pal: Palette) -> Self {
        ui.spacing_mut().item_spacing = Vec2::new(6.0, metric::ROW_GAP);
        Self { ui, pal }
    }

    pub fn ui(&mut self) -> &mut Ui {
        self.ui
    }

    pub fn width(&self) -> f32 {
        self.ui.available_width()
    }

    pub fn gap(&mut self, h: f32) {
        self.ui.add_space(h);
    }

    /// 章节标题（自动与上一节拉开距离，并带一条发丝线）。
    pub fn section(&mut self, title: &str) {
        self.ui.add_space(metric::SECTION_GAP - metric::ROW_GAP);
        widgets::section_label_with(self.ui, title, None, &self.pal);
        self.ui.add_space(metric::SECTION_LEAD - metric::ROW_GAP);
    }

    pub fn section_count(&mut self, title: &str, count: usize) {
        self.ui.add_space(metric::SECTION_GAP - metric::ROW_GAP);
        widgets::section_label_with(self.ui, title, Some(count), &self.pal);
        self.ui.add_space(metric::SECTION_LEAD - metric::ROW_GAP);
    }

    /// 说明文字：与内容左边界对齐，自动换行。
    pub fn note(&mut self, s: &str) {
        self.ui
            .label(RichText::new(s).size(text::MICRO).color(self.pal.faint));
    }

    /// 一行「标签 + 控件」。
    pub fn row<R>(&mut self, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
        self.row_h(label, metric::ROW_H, add)
    }

    pub fn row_h<R>(&mut self, label: &str, h: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
        let (lr, cr) = self.split(h);
        if !label.is_empty() {
            widgets::paint_text_clipped(
                self.ui.painter(),
                lr,
                label,
                FontId::proportional(text::LABEL),
                self.pal.muted,
                false,
                0.0,
            );
        }
        let pal = self.pal;
        self.ui
            .scope_builder(
                egui::UiBuilder::new()
                    .max_rect(cr)
                    .layout(egui::Layout::left_to_right(Align::Center)),
                |ui| {
                    ui.set_width(cr.width());
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let _ = pal;
                    add(ui)
                },
            )
            .inner
    }

    /// 整行控件（占满内容宽度，用于磁贴、色带、整宽按钮）。
    pub fn full<R>(&mut self, h: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
        let w = self.ui.available_width();
        let (rect, _) = self.ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
        self.ui
            .scope_builder(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(egui::Layout::left_to_right(Align::Center)),
                |ui| {
                    ui.set_width(rect.width());
                    add(ui)
                },
            )
            .inner
    }

    /// 下拉框：`add` 里只放 `selectable_value`。
    pub fn combo(&mut self, label: &str, current: &str, add: impl FnOnce(&mut Ui)) {
        self.row(label, |ui| {
            let w = ui.available_width();
            egui::ComboBox::from_id_salt(("form_combo", label))
                .width(w)
                // 弹层高度（不是按钮高度）：露出 5–6 个选项，翻起来不费劲。
                .height(metric::COMBO_POPUP_H)
                .selected_text(RichText::new(current).size(text::BODY))
                .show_ui(ui, |ui| {
                    ui.set_min_width(w - 12.0);
                    add(ui);
                });
        });
    }

    /// 单行输入框。
    pub fn entry(&mut self, label: &str, value: &mut String) -> Response {
        self.row(label, |ui| {
            let w = ui.available_width();
            ui.add_sized(
                [w, metric::CONTROL_H],
                egui::TextEdit::singleline(value)
                    .font(FontId::proportional(text::BODY))
                    .margin(Vec2::new(8.0, 4.0))
                    .align(Align2::LEFT_CENTER),
            )
        })
    }

    /// 滑块：轨道等宽，数值单独右对齐，避免数字位数不同导致右边缘参差。
    pub fn slider(
        &mut self,
        label: &str,
        value: &mut f64,
        range: RangeInclusive<f64>,
        decimals: usize,
    ) -> Response {
        const VALUE_W: f32 = 36.0;
        let (lr, cr) = self.split(metric::ROW_H);
        if !label.is_empty() {
            widgets::paint_text_clipped(
                self.ui.painter(),
                lr,
                label,
                FontId::proportional(text::LABEL),
                self.pal.muted,
                false,
                0.0,
            );
        }
        let track_w = (cr.width() - VALUE_W - 6.0).max(40.0);
        let resp = self
            .ui
            .scope_builder(
                egui::UiBuilder::new()
                    .max_rect(Rect::from_min_max(
                        cr.min,
                        egui::pos2(cr.left() + track_w, cr.bottom()),
                    ))
                    .layout(egui::Layout::left_to_right(Align::Center)),
                |ui| {
                    // 手柄半径 = 行高/2.5，压到 20 让圆点更轻，不与轨道抢视线。
                    ui.spacing_mut().interact_size.y = 20.0;
                    ui.spacing_mut().slider_rail_height = 5.0;
                    ui.spacing_mut().slider_width = track_w;
                    // 已填充段用强调色，读数方向一眼可见（仅作用于本滑块）。
                    ui.style_mut().visuals.selection.bg_fill = widgets::alpha(self.pal.accent, 110);
                    ui.add(
                        egui::Slider::new(value, range.clone())
                            .show_value(false)
                            .fixed_decimals(decimals),
                    )
                },
            )
            .inner;
        // 数值：右对齐到控件列右边界，等宽数字保证纵向对齐。
        self.ui.painter().text(
            egui::pos2(cr.right(), cr.center().y),
            Align2::RIGHT_CENTER,
            format!("{:.*}", decimals, *value),
            FontId::proportional(text::LABEL),
            self.pal.ink,
        );
        let _ = track_w;
        resp
    }

    /// 复选行：方框与内容左边界对齐，与章节标题同一条竖线。
    pub fn check(&mut self, label: &str, value: &mut bool) -> Response {
        let w = self.ui.available_width();
        let (rect, mut resp) = self
            .ui
            .allocate_exact_size(Vec2::new(w, 24.0), Sense::click());
        let box_r = Rect::from_center_size(
            egui::pos2(rect.left() + 7.5, rect.center().y),
            Vec2::splat(15.0),
        );
        let (fill, stroke) = if *value {
            (self.pal.accent, Stroke::new(1.0, self.pal.accent))
        } else if resp.hovered() {
            (self.pal.panel, Stroke::new(1.0, self.pal.muted))
        } else {
            (self.pal.panel, Stroke::new(1.0, self.pal.border))
        };
        surface(self.ui.painter(), box_r, radius::CHIP, fill, stroke);
        if *value {
            let c = box_r.center();
            self.ui.painter().add(egui::Shape::line(
                vec![
                    egui::pos2(c.x - 3.4, c.y + 0.2),
                    egui::pos2(c.x - 1.0, c.y + 2.7),
                    egui::pos2(c.x + 3.6, c.y - 2.6),
                ],
                Stroke::new(1.6, egui::Color32::WHITE),
            ));
        }
        self.ui.painter().text(
            egui::pos2(box_r.right() + 9.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(text::BODY),
            self.pal.ink,
        );
        if resp.clicked() {
            *value = !*value;
            resp.mark_changed();
        }
        resp
    }

    /// 两个复选并排，省一半纵向空间。
    pub fn check_pair(&mut self, a: (&str, &mut bool), b: (&str, &mut bool)) {
        let w = self.ui.available_width();
        let (rect, _) = self
            .ui
            .allocate_exact_size(Vec2::new(w, 24.0), Sense::hover());
        let half = w / 2.0;
        let left = Rect::from_min_max(rect.min, egui::pos2(rect.left() + half, rect.bottom()));
        let right = Rect::from_min_max(egui::pos2(rect.left() + half, rect.top()), rect.max);
        self.check_at(left, a.0, a.1);
        self.check_at(right, b.0, b.1);
    }

    fn check_at(&mut self, rect: Rect, label: &str, value: &mut bool) {
        let resp = self.ui.interact(
            rect,
            self.ui.scope_id().with((label, rect.left() as i32)),
            Sense::click(),
        );
        let box_r = Rect::from_center_size(
            egui::pos2(rect.left() + 7.5, rect.center().y),
            Vec2::splat(15.0),
        );
        let (fill, stroke) = if *value {
            (self.pal.accent, Stroke::new(1.0, self.pal.accent))
        } else if resp.hovered() {
            (self.pal.panel, Stroke::new(1.0, self.pal.muted))
        } else {
            (self.pal.panel, Stroke::new(1.0, self.pal.border))
        };
        surface(self.ui.painter(), box_r, radius::CHIP, fill, stroke);
        if *value {
            let c = box_r.center();
            self.ui.painter().add(egui::Shape::line(
                vec![
                    egui::pos2(c.x - 3.4, c.y + 0.2),
                    egui::pos2(c.x - 1.0, c.y + 2.7),
                    egui::pos2(c.x + 3.6, c.y - 2.6),
                ],
                Stroke::new(1.6, egui::Color32::WHITE),
            ));
        }
        self.ui.painter().text(
            egui::pos2(box_r.right() + 9.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(text::LABEL + 0.5),
            self.pal.ink,
        );
        if resp.clicked() {
            *value = !*value;
        }
    }

    /// 数值区间输入：`[min] – [max]`，两端固定宽度、右端对齐。
    pub fn range_row(&mut self, label: &str, r: &mut [f64; 2]) {
        let pal = self.pal;
        self.row(label, |ui| {
            let total = ui.available_width();
            let w = ((total - 26.0) / 2.0).max(48.0);
            ui.add_sized(
                [w, metric::CONTROL_H],
                egui::DragValue::new(&mut r[0]).speed(0.1).max_decimals(2),
            );
            ui.allocate_ui_with_layout(
                Vec2::new(26.0, metric::CONTROL_H),
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                |ui| {
                    ui.label(RichText::new("–").size(text::LABEL).color(pal.faint));
                },
            );
            ui.add_sized(
                [w, metric::CONTROL_H],
                egui::DragValue::new(&mut r[1]).speed(0.1).max_decimals(2),
            );
        });
    }

    /// 分段控件（等价于单选按钮组），选中块用强调色。
    pub fn segmented<T: PartialEq + Copy>(
        &mut self,
        label: &str,
        value: &mut T,
        items: &[(T, &str)],
    ) -> bool {
        let pal = self.pal;
        let mut changed = false;
        self.row(label, |ui| {
            let total = ui.available_width();
            let n = items.len().max(1) as f32;
            let gap = 4.0;
            let w = ((total - gap * (n - 1.0)) / n).max(28.0);
            ui.spacing_mut().item_spacing.x = gap;
            for (v, name) in items {
                let sel = *value == *v;
                let (rect, resp) =
                    ui.allocate_exact_size(Vec2::new(w, metric::CONTROL_H), Sense::click());
                let (bg, stroke) = if sel {
                    (pal.accent_soft, Stroke::new(1.0, pal.accent))
                } else if resp.hovered() {
                    (pal.hover, Stroke::new(1.0, pal.border))
                } else {
                    (pal.panel_alt, Stroke::new(1.0, pal.border))
                };
                surface(ui.painter(), rect, radius::CONTROL, bg, stroke);
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    *name,
                    FontId::proportional(text::LABEL),
                    if sel { pal.accent_ink } else { pal.muted },
                );
                if resp.clicked() && !sel {
                    *value = *v;
                    changed = true;
                }
            }
        });
        changed
    }

    /// 可点击的整宽行动作（左图标/文字 + 右箭头）。
    pub fn action(&mut self, label: &str, danger: bool) -> Response {
        let pal = self.pal;
        let (rect, resp) = self
            .ui
            .allocate_exact_size(Vec2::new(self.ui.available_width(), 30.0), Sense::click());
        let fg = if danger { pal.danger } else { pal.ink };
        let bg = if resp.hovered() {
            pal.hover
        } else {
            pal.panel_alt
        };
        surface(
            self.ui.painter(),
            rect,
            radius::CONTROL,
            bg,
            Stroke::new(1.0, pal.border),
        );
        self.ui.painter().text(
            egui::pos2(rect.left() + 11.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(text::BODY),
            fg,
        );
        resp
    }

    // -- 内部 --------------------------------------------------------------

    /// 把一行切成「标签列 / 控件列」，两列边界全表单一致。
    fn split(&mut self, h: f32) -> (Rect, Rect) {
        let w = self.ui.available_width();
        let (rect, _) = self.ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
        let lr = Rect::from_min_max(
            rect.min,
            egui::pos2(rect.left() + metric::LABEL_W, rect.bottom()),
        );
        let cr = Rect::from_min_max(
            egui::pos2(rect.left() + metric::LABEL_W + metric::GAP_X, rect.top()),
            rect.max,
        );
        (lr, cr)
    }
}

/// 可滚动内容区：内容宽度精确等于 `area.width()`，滚动条改画成右槽里的细指示条，
/// 这样「控件右边缘」不会被滚动条挤掉。
pub fn scroll_body<R>(ui: &mut Ui, area: Rect, pal: Palette, add: impl FnOnce(&mut Ui) -> R) -> R {
    let body = Rect::from_min_max(
        egui::pos2(area.left() + metric::PAD_X, area.top()),
        egui::pos2(area.right() - metric::PAD_X, area.bottom()),
    );
    let out = ui
        .scope_builder(
            egui::UiBuilder::new().max_rect(body).id_salt((
                "scroll_body",
                body.left() as i32,
                body.top() as i32,
            )),
            |ui| {
                ui.set_clip_rect(area);
                let mut scroll = egui::ScrollArea::vertical()
                    .id_salt(("scroll", body.left() as i32, body.top() as i32))
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .auto_shrink([false, false]);
                // 视觉回归用：把面板滚到底，方便截取「出版规范」这类尾部区块。
                if std::env::var("STATAURA_SCROLL_BOTTOM").is_ok() {
                    scroll = scroll.stick_to_bottom(true);
                }
                scroll.show(ui, |ui| {
                    ui.set_width(body.width());
                    let r = add(ui);
                    ui.add_space(metric::PAD_Y);
                    r
                })
            },
        )
        .inner;

    let content_h = out.content_size.y;
    let view_h = body.height();
    if content_h > view_h + 1.0 {
        let offset = out.state.clamped_offset().y;
        let track = Rect::from_min_max(
            egui::pos2(body.right() + 6.0, body.top() + 4.0),
            egui::pos2(body.right() + 9.0, body.bottom() - 4.0),
        );
        let ratio = (view_h / content_h).clamp(0.06, 1.0);
        let h = (track.height() * ratio).max(24.0);
        let y = track.top()
            + (track.height() - h) * (offset / (content_h - view_h).max(1.0)).clamp(0.0, 1.0);
        surface(
            ui.painter(),
            Rect::from_min_size(egui::pos2(track.left(), y), Vec2::new(track.width(), h)),
            track.width() / 2.0,
            widgets::alpha(pal.muted, 90),
            Stroke::NONE,
        );
    }
    out.inner
}

/// 面板头部：标题 + 右侧标签，底边一条发丝线。返回头部高度。
pub fn panel_header(ui: &Ui, rect: Rect, title: &str, tag: &str, pal: &Palette) -> f32 {
    const H: f32 = 46.0;
    let p = ui.painter();
    p.text(
        egui::pos2(rect.left() + metric::PAD_X, rect.top() + H * 0.5),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(text::BODY),
        pal.muted,
    );
    if !tag.is_empty() {
        let galley = p.layout_no_wrap(tag.to_owned(), FontId::proportional(text::MICRO), pal.muted);
        let w = galley.size().x + 14.0;
        let r = Rect::from_min_size(
            egui::pos2(rect.right() - metric::PAD_X - w, rect.top() + H * 0.5 - 9.0),
            Vec2::new(w, 18.0),
        );
        surface(
            p,
            r,
            radius::CHIP,
            pal.panel_alt,
            Stroke::new(1.0, pal.border),
        );
        p.galley(r.center() - galley.size() / 2.0, galley, pal.muted);
    }
    p.hline(
        rect.x_range(),
        rect.top() + H - 0.5,
        Stroke::new(1.0, pal.border_soft),
    );
    H
}
