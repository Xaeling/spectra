//! 命令面板与空状态页。

use egui::{Align, Align2, FontId, Layout, Rect, RichText, Sense, Stroke, Vec2};

use crate::app::theme::{radius, text, Palette};
use crate::app::widgets::{self, surface};
use crate::app::{App, ToastKind};
use crate::core::graph::ChartKind;
use crate::core::model::{AnalysisKind, TableKind};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Command {
    NewProject,
    Open,
    Save,
    Export,
    ImportCsv,
    NewGraph,
    NewLayout,
    AutoSig,
    Undo,
    Redo,
    ToggleTheme,
    ToggleInspector,
    ToggleSidebar,
    About,
    Analyze(AnalysisKind),
    NewTable(TableKind),
    GraphKind(ChartKind),
}

impl App {
    /// 是否有模态弹窗打开（决定是否画遮罩）。
    fn any_modal_open(&self) -> bool {
        self.command_open
            || self.new_table_dialog_open
            || self.about_open
            || self.settings_ui.open
            || self.confirm.is_some()
            || self.export.open
    }

    /// 模态遮罩：半透明灰色背景，拦截所有底层点击。
    /// 用 Order::Middle——Window 默认 Foreground，遮罩在它下面。
    pub fn modal_overlay(&mut self, ctx: &egui::Context) {
        if !self.any_modal_open() {
            return;
        }
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::unique("modal_overlay"))
            .fixed_pos(screen.min)
            .order(egui::Order::Middle)
            .interactable(true)
            .show(ctx, |ui| {
                let (rect, resp) = ui.allocate_exact_size(screen.size(), egui::Sense::click());
                ui.painter()
                    .rect_filled(rect, 0.0, egui::Color32::from_black_alpha(100));
                // 点遮罩 = 关闭最上层弹窗
                if resp.clicked() {
                    if self.new_table_dialog_open {
                        self.new_table_dialog_open = false;
                    } else if self.command_open {
                        self.command_open = false;
                    } else if self.about_open {
                        self.about_open = false;
                    } else if self.settings_ui.open {
                        self.settings_ui.open = false;
                    }
                }
            });
    }

    pub fn modals(&mut self, ctx: &egui::Context) {
        // 遮罩已在 mod.rs 的 dialogs() 之前画（保证在弹窗下面）
        // 命令面板
        if self.command_open {
            self.command_palette(ctx);
        }
    }

    fn command_palette(&mut self, ctx: &egui::Context) {
        let pal = self.pal;
        let mut exec: Option<Command> = None;
        let mut close = false;
        egui::Window::new("命令面板")
            .id(egui::Id::unique("command_palette"))
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .default_width(560.0)
            .anchor(Align2::CENTER_TOP, Vec2::new(0.0, 96.0))
            .show(ctx, |ui| {
                // 搜索框：自绘底 + 无框输入，内边距与列表行一致。
                let w = ui.available_width();
                let (search, _) = ui.allocate_exact_size(Vec2::new(w, 34.0), Sense::hover());
                surface(
                    ui.painter(),
                    search,
                    radius::CONTROL,
                    pal.panel_alt,
                    Stroke::new(1.0, pal.border),
                );
                let resp = ui.put(
                    Rect::from_min_max(
                        egui::pos2(search.left() + 12.0, search.top() + 1.0),
                        egui::pos2(search.right() - 10.0, search.bottom() - 1.0),
                    ),
                    egui::TextEdit::singleline(&mut self.command_query)
                        .hint_text("输入命令，例如「导出」「ANOVA」…")
                        .frame(egui::Frame::NONE)
                        .font(FontId::proportional(13.0)),
                );
                if self.command_focus {
                    resp.request_focus();
                    self.command_focus = false;
                }
                ui.add_space(8.0);

                let q = self.command_query.trim().to_lowercase();
                let mut entries = Vec::new();
                // 命令面板只列出当前表格类型能用的「切换图型」命令。
                let table_kind = self
                    .current_table_id()
                    .and_then(|id| self.project.table(id))
                    .map(|t| t.kind);
                for (cmd, label, hint, keys) in commands(table_kind) {
                    let hay = format!("{label} {hint} {keys}").to_lowercase();
                    if q.is_empty() || hay.contains(&q) {
                        entries.push((cmd, label, hint, keys));
                    }
                }
                // 标签列宽取最长项，说明文字与快捷键都落在固定列上。
                let label_w = entries
                    .iter()
                    .map(|(_, l, _, _)| {
                        ui.painter()
                            .layout_no_wrap(
                                l.to_string(),
                                FontId::proportional(text::BODY + 0.5),
                                pal.ink,
                            )
                            .size()
                            .x
                    })
                    .fold(0.0_f32, f32::max)
                    .clamp(56.0, 180.0);

                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter)) && !q.is_empty();
                egui::ScrollArea::vertical()
                    .max_height(340.0)
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 1.0;
                        for (i, (cmd, label, hint, keys)) in entries.iter().enumerate() {
                            let selected = enter && i == 0;
                            let (rect, resp) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), 32.0),
                                Sense::click(),
                            );
                            if selected || resp.hovered() {
                                surface(
                                    ui.painter(),
                                    rect,
                                    radius::CONTROL - 1.0,
                                    pal.accent_soft,
                                    Stroke::NONE,
                                );
                            }
                            ui.painter().text(
                                egui::pos2(rect.left() + 12.0, rect.center().y),
                                Align2::LEFT_CENTER,
                                *label,
                                FontId::proportional(text::BODY + 0.5),
                                pal.ink,
                            );
                            ui.painter().text(
                                egui::pos2(rect.left() + 12.0 + label_w + 18.0, rect.center().y),
                                Align2::LEFT_CENTER,
                                *hint,
                                FontId::proportional(text::MICRO + 0.5),
                                pal.faint,
                            );
                            ui.painter().text(
                                egui::pos2(rect.right() - 12.0, rect.center().y),
                                Align2::RIGHT_CENTER,
                                *keys,
                                FontId::monospace(text::MICRO),
                                pal.faint,
                            );
                            if resp.clicked() || (selected && enter) {
                                exec = Some(*cmd);
                                close = true;
                            }
                        }
                        if entries.is_empty() {
                            ui.add_space(12.0);
                            ui.label(
                                RichText::new("没有匹配的命令")
                                    .size(text::BODY)
                                    .color(pal.faint),
                            );
                        }
                        ui.add_space(8.0);
                    });
                // 底部快捷键提示：与内容左边界对齐，字重更轻。
                let w = ui.available_width();
                let (foot, _) = ui.allocate_exact_size(Vec2::new(w, 18.0), Sense::hover());
                ui.painter().hline(
                    foot.x_range(),
                    foot.top() + 0.5,
                    Stroke::new(1.0, pal.border_soft),
                );
                ui.painter().text(
                    egui::pos2(foot.left() + 2.0, foot.center().y + 3.0),
                    Align2::LEFT_CENTER,
                    "Enter 执行　·　Esc 关闭　·　输入可匹配命令名与说明",
                    FontId::proportional(text::MICRO),
                    pal.faint,
                );
            });
        if close {
            self.command_open = false;
        }
        if let Some(cmd) = exec {
            self.run_command(cmd, ctx);
        }
    }

    pub fn empty_state(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let pal = self.pal;
        ui.painter().rect_filled(rect, 0, pal.bg);
        let card_w = (rect.width() - 48.0).clamp(460.0, 780.0);
        let card = Rect::from_center_size(rect.center(), Vec2::new(card_w, 424.0));
        widgets::soft_shadow(ui.painter(), card, radius::PANEL, pal.shadow);
        surface(
            ui.painter(),
            card,
            radius::PANEL,
            pal.panel,
            Stroke::new(1.0, pal.border),
        );
        crate::app::area_public(ui, card.shrink(28.0), |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("开始一张新图").size(22.0).color(pal.ink));
                ui.label(
                    RichText::new("选择数据表类型，录入数据后即可做统计与出版图。")
                        .size(text::BODY)
                        .color(pal.muted),
                );
                ui.add_space(16.0);
                let mut chosen: Option<TableKind> = None;
                let gap = 10.0;
                let cols = 4.0;
                let bw = ((card_w - 56.0) - gap * (cols - 1.0)) / cols;
                for row in TableKind::ALL.chunks(cols as usize) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        for kind in row {
                            let (r, resp) =
                                ui.allocate_exact_size(Vec2::new(bw, 54.0), Sense::click());
                            let bg = if resp.hovered() {
                                pal.accent_soft
                            } else {
                                pal.panel_alt
                            };
                            let st = if resp.hovered() {
                                Stroke::new(1.0, pal.accent)
                            } else {
                                Stroke::new(1.0, pal.border)
                            };
                            surface(ui.painter(), r, radius::CARD, bg, st);
                            ui.painter().text(
                                egui::pos2(r.center().x, r.center().y - 7.0),
                                Align2::CENTER_CENTER,
                                kind.label(),
                                FontId::proportional(text::BODY),
                                pal.ink,
                            );
                            ui.painter().text(
                                egui::pos2(r.center().x, r.center().y + 9.0),
                                Align2::CENTER_CENTER,
                                kind.en(),
                                FontId::proportional(text::MICRO),
                                pal.faint,
                            );
                            if resp.clicked() {
                                chosen = Some(*kind);
                            }
                        }
                    });
                    ui.add_space(6.0);
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if widgets::Btn::new("导入 CSV")
                        .fill(pal.accent)
                        .fg(egui::Color32::WHITE)
                        .strong(true)
                        .radius(radius::CONTROL)
                        .height(32.0)
                        .min_width(112.0)
                        .show(ui)
                        .clicked()
                    {
                        self.import_csv();
                    }
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("或从「文件 → 打开工程」继续之前的工作")
                            .size(text::LABEL)
                            .color(pal.faint),
                    );
                });
                if let Some(k) = chosen {
                    self.add_table(k);
                }
                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.label(
                        RichText::new("提示：Ctrl+K 打开命令面板")
                            .size(text::MICRO + 0.5)
                            .color(pal.faint),
                    );
                });
            });
        });
        let _ = ToastKind::Info;
    }
}

fn commands(
    table_kind: Option<TableKind>,
) -> Vec<(Command, &'static str, &'static str, &'static str)> {
    let mut v = vec![
        (Command::NewProject, "新建工程", "清空当前工程", ""),
        (Command::Open, "打开工程", "载入 .aura 文件", "Ctrl+O"),
        (Command::Save, "保存工程", "写入当前文件", "Ctrl+S"),
        (
            Command::Export,
            "导出图表",
            "PNG / TIFF / SVG / PDF",
            "Ctrl+E",
        ),
        (Command::ImportCsv, "导入 CSV", "从表格软件粘贴数据", ""),
        (Command::NewGraph, "新建图表", "用当前数据表作图", ""),
        (
            Command::NewLayout,
            "新建拼版页",
            "把多张图拼成一张大图（a/b/c 图注）",
            "",
        ),
        (Command::AutoSig, "自动显著性标注", "按统计检验生成括号", ""),
        (Command::Undo, "撤销", "", "Ctrl+Z"),
        (Command::Redo, "重做", "", "Ctrl+Y"),
        (Command::ToggleTheme, "切换深色模式", "", ""),
        (Command::ToggleInspector, "显示 / 隐藏检查器", "", ""),
        (Command::ToggleSidebar, "显示 / 隐藏侧栏", "", ""),
        (Command::About, "关于 StatAura", "", ""),
    ];
    if let Some(tk) = table_kind {
        for k in AnalysisKind::ALL.iter().copied().filter(|a| a.supports(tk)) {
            v.push((Command::Analyze(k), "分析", k.label(), ""));
        }
    }
    for k in TableKind::ALL {
        v.push((Command::NewTable(k), "新建数据表", k.label(), ""));
    }
    if let Some(tk) = table_kind {
        for k in ChartKind::ALL.iter().copied().filter(|k| k.supports(tk)) {
            v.push((Command::GraphKind(k), "切换图型", k.label(), ""));
        }
    }
    v
}

/// 供将来复用的 hover 底：图标命中区统一用同一个圆角。
#[allow(dead_code)]
fn hover_pad(ui: &egui::Ui, rect: Rect, pal: &Palette) {
    surface(ui.painter(), rect, radius::CONTROL, pal.hover, Stroke::NONE);
}
