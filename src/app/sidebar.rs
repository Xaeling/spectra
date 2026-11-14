//! 左侧项目浏览器：数据表 / 分析 / 图表，搜索与新建。
//!
//! 列内只有一条左边界（`PAD_X`）与一条右边界（`宽 - PAD_X`）：
//! 章节标题、行徽标、行文字、底部按钮全部贴这两条线。

use egui::{
    Align, Align2, Color32, FontId, Layout, Rect, ScrollArea, Sense, Stroke, TextEdit, Ui, Vec2,
};

use crate::app::form::panel_header;
use crate::app::theme::{metric, radius, text, Palette};
use crate::app::widgets::{self, surface};
use crate::app::{App, Sheet, SidebarEdit};
use crate::core::model::TableKind;

const ROW_H: f32 = 34.0;

struct RowData {
    sheet: Sheet,
    title: String,
    sub: String,
    badge: String,
    color: Color32,
}

/// 侧栏一行右键菜单可用的操作。
#[derive(Clone, Copy, PartialEq)]
pub enum RowAction {
    Rename,
    Duplicate,
    Delete,
}

/// 一次交互结果：左键选中 / 右键菜单动作（二者至多各一）。
struct RowHit {
    select: bool,
    action: Option<RowAction>,
    commit: Option<(Sheet, String)>,
}

pub fn table_badge(kind: TableKind) -> (&'static str, Color32) {
    match kind {
        TableKind::Xy => ("XY", Color32::from_rgb(0x5B, 0x6C, 0xFF)),
        TableKind::Column => ("柱", Color32::from_rgb(0x17, 0xB8, 0xA6)),
        TableKind::Grouped => ("组", Color32::from_rgb(0xFF, 0x7A, 0x59)),
        TableKind::Contingency => ("列", Color32::from_rgb(0xB0, 0x7C, 0xFF)),
        TableKind::Survival => ("生", Color32::from_rgb(0xE8, 0x5D, 0x9E)),
        TableKind::PartsOfWhole => ("构", Color32::from_rgb(0xF2, 0xB8, 0x4B)),
        TableKind::MultipleVariables => ("变", Color32::from_rgb(0x4C, 0x9F, 0x70)),
        TableKind::Nested => ("嵌", Color32::from_rgb(0x8A, 0x7B, 0xD0)),
    }
}

impl App {
    pub fn sidebar(&mut self, ui: &mut Ui, rect: Rect) {
        let pal = self.pal;
        ui.painter().rect_filled(rect, 0, pal.sidebar);
        ui.painter().vline(
            rect.right() - 0.5,
            rect.y_range(),
            Stroke::new(1.0, pal.border),
        );

        let head = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 46.0));
        let count =
            self.project.tables.len() + self.project.analyses.len() + self.project.graphs.len();
        let head_h = panel_header(ui, head, "项目浏览器", &format!("{count} 项"), &pal);

        // ---- 搜索框 ----
        let search = Rect::from_min_size(
            egui::pos2(rect.left() + metric::PAD_X, rect.top() + head_h + 12.0),
            Vec2::new(rect.width() - metric::PAD_X * 2.0, 30.0),
        );
        let hovered = ui
            .ctx()
            .pointer_hover_pos()
            .map(|p| search.contains(p))
            .unwrap_or(false);
        surface(
            ui.painter(),
            search,
            radius::CONTROL,
            if hovered { pal.panel } else { pal.panel_alt },
            Stroke::new(1.0, if hovered { pal.muted } else { pal.border }),
        );
        // 放大镜：两笔就够，避免小尺寸下糊成一团。
        let g = egui::pos2(search.left() + 13.0, search.center().y - 1.0);
        ui.painter()
            .circle_stroke(g, 4.0, Stroke::new(1.3, pal.faint));
        ui.painter().line_segment(
            [
                egui::pos2(g.x + 3.0, g.y + 3.0),
                egui::pos2(g.x + 6.0, g.y + 6.0),
            ],
            Stroke::new(1.3, pal.faint),
        );
        let mut q = std::mem::take(&mut self.sidebar_query);
        ui.put(
            Rect::from_min_max(
                egui::pos2(search.left() + 24.0, search.top() + 1.0),
                egui::pos2(search.right() - 8.0, search.bottom() - 1.0),
            ),
            TextEdit::singleline(&mut q)
                .hint_text("搜索表、分析、图…")
                .frame(egui::Frame::NONE)
                .align(Align2::LEFT_CENTER)
                .font(FontId::proportional(text::BODY)),
        );
        self.sidebar_query = q;

        let query = self.sidebar_query.trim().to_lowercase();
        let mut rows: Vec<RowData> = Vec::new();
        for t in &self.project.tables {
            let (badge, color) = table_badge(t.kind);
            rows.push(RowData {
                sheet: Sheet::Table(t.id),
                title: t.name.clone(),
                sub: t.kind.label().into(),
                badge: badge.into(),
                color,
            });
        }
        for a in &self.project.analyses {
            rows.push(RowData {
                sheet: Sheet::Analysis(a.id),
                title: a.name.clone(),
                sub: a.kind.label().into(),
                badge: "Σ".into(),
                color: Color32::from_rgb(0x3D, 0x4A, 0x5C),
            });
        }
        for g in &self.project.graphs {
            rows.push(RowData {
                sheet: Sheet::Graph(g.id),
                title: g.name.clone(),
                sub: g.spec.kind.label().into(),
                badge: "图".into(),
                color: pal.accent,
            });
        }
        for l in &self.project.layouts {
            rows.push(RowData {
                sheet: Sheet::Layout(l.id),
                title: l.name.clone(),
                sub: format!("拼版 · {} 面板", l.panels.len()),
                badge: "版".into(),
                color: Color32::from_rgb(0x8B, 0x5C, 0xF6),
            });
        }
        if !query.is_empty() {
            rows.retain(|r| {
                r.title.to_lowercase().contains(&query) || r.sub.to_lowercase().contains(&query)
            });
        }

        if let Some(e) = &self.sidebar_edit {
            if !rows.iter().any(|r| r.sheet == e.sheet) {
                self.sidebar_edit = None;
            }
        }

        let bottom_h = 74.0;
        let list = Rect::from_min_max(
            egui::pos2(rect.left() + metric::PAD_X, search.bottom() + 6.0),
            egui::pos2(
                rect.right() - metric::PAD_X,
                rect.bottom() - bottom_h - metric::PAD_Y,
            ),
        );
        let mut clicked: Option<Sheet> = None;
        let mut add_kind: Option<TableKind> = None;
        let mut add_layout_flag = false;
        let mut ctx_ops: Vec<(Sheet, RowAction)> = Vec::new();
        let mut renames: Vec<(Sheet, String)> = Vec::new();

        crate::app::area_public(ui, list, |ui| {
            ScrollArea::vertical()
                .id_salt("sidebar_scroll")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(list.width());
                    ui.spacing_mut().item_spacing.y = 2.0;

                    let (tables, analyses, graphs, layouts): (
                        Vec<&RowData>,
                        Vec<&RowData>,
                        Vec<&RowData>,
                        Vec<&RowData>,
                    ) = (
                        rows.iter()
                            .filter(|r| matches!(r.sheet, Sheet::Table(_)))
                            .collect(),
                        rows.iter()
                            .filter(|r| matches!(r.sheet, Sheet::Analysis(_)))
                            .collect(),
                        rows.iter()
                            .filter(|r| matches!(r.sheet, Sheet::Graph(_)))
                            .collect(),
                        rows.iter()
                            .filter(|r| matches!(r.sheet, Sheet::Layout(_)))
                            .collect(),
                    );

                    section(ui, "数据表", tables.len(), &self.pal, |ui| {
                        for kind in TableKind::ALL {
                            if ui.button(kind.label()).clicked() {
                                add_kind = Some(kind);
                                ui.close();
                            }
                        }
                    });
                    for r in &tables {
                        let hit = row(
                            ui,
                            r,
                            self.sel,
                            &self.pal,
                            &mut self.sidebar_edit,
                            self.grid.composing,
                        );
                        if hit.select {
                            clicked = Some(r.sheet);
                        }
                        if let Some(a) = hit.action {
                            ctx_ops.push((r.sheet, a));
                        }
                        if let Some(c) = hit.commit {
                            renames.push(c);
                        }
                    }
                    if tables.is_empty() {
                        hint(ui, "还没有数据表", &self.pal);
                    }

                    ui.add_space(10.0);
                    section(ui, "分析结果", analyses.len(), &self.pal, |_ui| {});
                    for r in &analyses {
                        let hit = row(
                            ui,
                            r,
                            self.sel,
                            &self.pal,
                            &mut self.sidebar_edit,
                            self.grid.composing,
                        );
                        if hit.select {
                            clicked = Some(r.sheet);
                        }
                        if let Some(a) = hit.action {
                            ctx_ops.push((r.sheet, a));
                        }
                        if let Some(c) = hit.commit {
                            renames.push(c);
                        }
                    }
                    if analyses.is_empty() {
                        hint(ui, "选中数据表 →「分析」", &self.pal);
                    }

                    ui.add_space(10.0);
                    section(ui, "图表", graphs.len(), &self.pal, |_ui| {});
                    for r in &graphs {
                        let hit = row(
                            ui,
                            r,
                            self.sel,
                            &self.pal,
                            &mut self.sidebar_edit,
                            self.grid.composing,
                        );
                        if hit.select {
                            clicked = Some(r.sheet);
                        }
                        if let Some(a) = hit.action {
                            ctx_ops.push((r.sheet, a));
                        }
                        if let Some(c) = hit.commit {
                            renames.push(c);
                        }
                    }
                    if graphs.is_empty() {
                        hint(ui, "选中数据表 →「图表」", &self.pal);
                    }

                    ui.add_space(10.0);
                    section(ui, "拼版", layouts.len(), &self.pal, |ui| {
                        if ui.button("新建拼版页").clicked() {
                            add_layout_flag = true;
                            ui.close();
                        }
                    });
                    for r in &layouts {
                        let hit = row(
                            ui,
                            r,
                            self.sel,
                            &self.pal,
                            &mut self.sidebar_edit,
                            self.grid.composing,
                        );
                        if hit.select {
                            clicked = Some(r.sheet);
                        }
                        if let Some(a) = hit.action {
                            ctx_ops.push((r.sheet, a));
                        }
                        if let Some(c) = hit.commit {
                            renames.push(c);
                        }
                    }
                    if layouts.is_empty() {
                        hint(ui, "把多张图拼成一张大图（a/b/c 图注）", &self.pal);
                    }
                    // 入口要显眼：始终给一行可点的按钮，不藏在「＋」弹层里。
                    if action_row(ui, "＋ 新建拼版页", &self.pal) {
                        add_layout_flag = true;
                    }
                    ui.add_space(12.0);
                });
        });

        if let Some(kind) = add_kind {
            self.add_table(kind);
        }
        if add_layout_flag {
            self.add_layout_page();
        }
        if let Some(sheet) = clicked {
            self.select(sheet);
        }
        for (sheet, name) in renames {
            self.commit_sidebar_rename(sheet, &name);
        }
        // 处理右键菜单动作
        for (sheet, action) in ctx_ops {
            match action {
                RowAction::Rename => self.open_rename(sheet),
                RowAction::Duplicate => self.duplicate_sheet(sheet),
                RowAction::Delete => self.delete_sheet(sheet),
            }
        }

        // ---- 底部操作 ----
        let bottom = Rect::from_min_size(
            egui::pos2(rect.left() + metric::PAD_X, rect.bottom() - bottom_h),
            Vec2::new(rect.width() - metric::PAD_X * 2.0, 60.0),
        );
        crate::app::area_public(ui, bottom, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                if widgets::Btn::new("新建数据表")
                    .fill(pal.accent)
                    .fg(Color32::WHITE)
                    .strong(true)
                    .radius(radius::CONTROL)
                    .height(32.0)
                    .full(true)
                    .show(ui)
                    .clicked()
                {
                    self.add_table(TableKind::Column);
                }
                if widgets::Btn::new("导入 CSV")
                    .fill(pal.panel)
                    .fg(pal.ink)
                    .stroke(Stroke::new(1.0, pal.border))
                    .radius(radius::CONTROL)
                    .height(30.0)
                    .full(true)
                    .show(ui)
                    .clicked()
                {
                    self.import_csv();
                }
            });
        });
    }
}

fn section(ui: &mut Ui, title: &str, count: usize, pal: &Palette, menu: impl FnOnce(&mut Ui)) {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 26.0), Sense::hover());
    ui.painter().text(
        egui::pos2(rect.left(), rect.bottom() - 6.0),
        Align2::LEFT_BOTTOM,
        title,
        FontId::proportional(text::MICRO + 0.5),
        pal.faint,
    );
    let label_w = ui
        .painter()
        .layout_no_wrap(
            title.to_owned(),
            FontId::proportional(text::MICRO + 0.5),
            pal.faint,
        )
        .size()
        .x;
    ui.painter().text(
        egui::pos2(rect.left() + label_w + 6.0, rect.bottom() - 6.0),
        Align2::LEFT_BOTTOM,
        count.to_string(),
        FontId::proportional(text::MICRO),
        pal.faint,
    );

    // 只有能新增内容的分类才给「＋」，避免出现点了没反应的按钮。
    // （拼版曾被这行挡住：菜单里的「新建拼版」永远弹不出来，等于没有入口。）
    if !matches!(title, "数据表" | "拼版") {
        return;
    }
    let plus = Rect::from_center_size(
        egui::pos2(rect.right() - 10.0, rect.center().y + 2.0),
        Vec2::splat(20.0),
    );
    let hit = ui.interact(
        plus,
        ui.scope_id().with(("sec_plus", title)),
        Sense::click(),
    );
    if hit.hovered() {
        surface(ui.painter(), plus, radius::CHIP, pal.hover, Stroke::NONE);
    }
    ui.painter().text(
        plus.center(),
        Align2::CENTER_CENTER,
        "＋",
        FontId::proportional(11.0),
        if hit.hovered() { pal.ink } else { pal.faint },
    );
    let _ = resp;
    egui::Popup::menu(&hit).show(|ui| menu(ui));
}

fn row(
    ui: &mut Ui,
    r: &RowData,
    selected: Option<Sheet>,
    pal: &Palette,
    edit: &mut Option<SidebarEdit>,
    composing: bool,
) -> RowHit {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, ROW_H), Sense::click());
    let is_sel = selected == Some(r.sheet);
    if is_sel {
        surface(
            ui.painter(),
            rect,
            radius::nested(radius::CARD, 2.0),
            pal.accent_soft,
            Stroke::NONE,
        );
        let bar = Rect::from_min_size(
            egui::pos2(rect.left(), rect.top() + 8.0),
            Vec2::new(3.0, rect.height() - 16.0),
        );
        surface(ui.painter(), bar, 1.5, pal.accent, Stroke::NONE);
    } else if resp.hovered() {
        surface(
            ui.painter(),
            rect,
            radius::nested(radius::CARD, 2.0),
            pal.hover,
            Stroke::NONE,
        );
    }

    let badge = Rect::from_min_size(
        egui::pos2(rect.left() + 11.0, rect.center().y - 9.0),
        Vec2::new(24.0, 18.0),
    );
    surface(
        ui.painter(),
        badge,
        radius::CHIP,
        widgets::alpha(r.color, if is_sel { 42 } else { 26 }),
        Stroke::NONE,
    );
    ui.painter().text(
        badge.center(),
        Align2::CENTER_CENTER,
        &r.badge,
        FontId::proportional(text::MICRO + 0.5),
        r.color,
    );

    let text_x = badge.right() + 9.0;
    let text_right = rect.right() - 8.0;
    let title_rect = Rect::from_min_max(
        egui::pos2(text_x, rect.center().y - 16.0),
        egui::pos2(text_right, rect.center().y + 1.0),
    );

    // 行内改名：文字直接就地变编辑框，样式/位置与标题完全一致。
    let is_editing = edit.as_ref().is_some_and(|e| e.sheet == r.sheet);
    let mut commit = None;
    if is_editing {
        let mut e = edit.take().expect("checked above");
        let just = e.just_started;
        e.just_started = false;
        let edit_id = ui.make_persistent_id(("sidebar_rename", r.sheet.id()));
        if just {
            let mut st =
                egui::text_edit::TextEditState::load(ui.ctx(), edit_id).unwrap_or_default();
            st.cursor
                .set_char_range(Some(egui::text::CCursorRange::select_all_str(&e.buf)));
            st.store(ui.ctx(), edit_id);
        }
        let rr = ui.put(
            title_rect,
            TextEdit::singleline(&mut e.buf)
                .frame(egui::Frame::NONE)
                .font(FontId::proportional(text::BODY + 0.2))
                .text_color(if is_sel { pal.accent_ink } else { pal.ink })
                .margin(egui::Margin::ZERO)
                .id(edit_id),
        );
        if just {
            rr.request_focus();
        }
        ui.painter().hline(
            title_rect.x_range(),
            title_rect.bottom() + 1.0,
            Stroke::new(1.0, pal.accent),
        );
        let enter = !composing && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let esc = !composing && ui.input(|i| i.key_pressed(egui::Key::Escape));
        if esc {
            // 取消，保留原名
        } else if enter || (rr.lost_focus() && !composing) {
            commit = Some((r.sheet, e.buf));
        } else {
            *edit = Some(e);
        }
    } else {
        widgets::paint_text_clipped(
            ui.painter(),
            title_rect,
            &r.title,
            FontId::proportional(text::BODY + 0.2),
            if is_sel { pal.accent_ink } else { pal.ink },
            false,
            0.0,
        );
    }

    widgets::paint_text_clipped(
        ui.painter(),
        Rect::from_min_max(
            egui::pos2(text_x, rect.center().y + 1.0),
            egui::pos2(text_right, rect.center().y + 16.0),
        ),
        &r.sub,
        FontId::proportional(text::MICRO),
        pal.faint,
        false,
        0.0,
    );

    let select = resp.clicked();
    let mut action: Option<RowAction> = None;
    if resp.double_clicked() && !is_editing {
        action = Some(RowAction::Rename);
    }
    let can_dup = matches!(r.sheet, Sheet::Table(_) | Sheet::Graph(_));
    resp.context_menu(|ui| {
        ui.set_min_width(140.0);
        if ui.button("重命名").clicked() {
            action = Some(RowAction::Rename);
            ui.close();
        }
        if can_dup && ui.button("复制").clicked() {
            action = Some(RowAction::Duplicate);
            ui.close();
        }
        ui.separator();
        if ui
            .button(egui::RichText::new("删除").color(pal.danger))
            .clicked()
        {
            action = Some(RowAction::Delete);
            ui.close();
        }
    });
    RowHit {
        select,
        action,
        commit,
    }
}

/// 可点击的动作行（比藏在「＋」弹层里的按钮显眼，侧栏里的主要入口用它）。
fn action_row(ui: &mut Ui, t: &str, pal: &Palette) -> bool {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 26.0), Sense::click());
    let hot = resp.hovered();
    if hot {
        surface(ui.painter(), rect, radius::CHIP, pal.hover, Stroke::NONE);
    }
    ui.painter().text(
        egui::pos2(rect.left() + 6.0, rect.center().y),
        Align2::LEFT_CENTER,
        t,
        FontId::proportional(text::MICRO + 1.0),
        if hot { pal.accent } else { pal.muted },
    );
    resp.clicked()
}

fn hint(ui: &mut Ui, t: &str, pal: &Palette) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 20.0), Sense::hover());
    ui.painter().text(
        egui::pos2(rect.left() + 2.0, rect.center().y),
        Align2::LEFT_CENTER,
        t,
        FontId::proportional(text::MICRO + 0.5),
        pal.faint,
    );
}

/// 未使用但保留：Layout 在 section 里用于将来右侧内联操作。
const _: fn() = || {
    let _ = Layout::left_to_right(Align::Center);
};
