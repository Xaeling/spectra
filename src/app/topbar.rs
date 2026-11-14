//! 顶栏：品牌、菜单、撤销/重做、主题、导出。

use egui::{Align, Align2, Color32, FontId, Layout, Rect, Sense, Stroke, Ui, Vec2};

use crate::app::icons::Icon;
use crate::app::modals::Command;
use crate::app::theme;
use crate::app::widgets::icon_button;
use crate::app::{App, ToastKind};
use crate::core::model::TableKind;

impl App {
    pub fn topbar(&mut self, ui: &mut Ui, rect: Rect) {
        let pal = self.pal;
        ui.painter().rect_filled(rect, 0, pal.panel);
        ui.painter().hline(
            rect.x_range(),
            rect.bottom() - 0.5,
            Stroke::new(1.0, pal.border),
        );

        crate::app::area_public(ui, rect, |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.horizontal_centered(|ui| {
                ui.add_space(theme::metric::BAR_PAD_X);
                // 品牌块：标志 + 两行文字，按字形高度手工排布，基线才稳。
                let (logo, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
                crate::app::widgets::surface(
                    ui.painter(),
                    logo,
                    theme::radius::CONTROL,
                    pal.accent,
                    Stroke::NONE,
                );
                ui.painter().text(
                    logo.center(),
                    Align2::CENTER_CENTER,
                    "S",
                    FontId::proportional(16.0),
                    Color32::WHITE,
                );
                ui.add_space(4.0);
                let title_gal = ui.painter().layout_no_wrap(
                    "StatAura".to_owned(),
                    FontId::proportional(13.5),
                    pal.ink,
                );
                let sub_gal = ui.painter().layout_no_wrap(
                    "科研统计 · 出版绘图".to_owned(),
                    FontId::proportional(theme::text::MICRO),
                    pal.faint,
                );
                let block_w = title_gal.size().x.max(sub_gal.size().x);
                let (block, _) = ui.allocate_exact_size(Vec2::new(block_w, 30.0), Sense::hover());
                let total = title_gal.size().y + 1.0 + sub_gal.size().y;
                let top = block.center().y - total * 0.5;
                ui.painter()
                    .galley(egui::pos2(block.left(), top), title_gal.clone(), pal.ink);
                ui.painter().galley(
                    egui::pos2(block.left(), top + title_gal.size().y + 1.0),
                    sub_gal,
                    pal.faint,
                );
                ui.add_space(10.0);
                // 竖向分隔线：把品牌与命令区分开，宽度只占 1px 的呼吸。
                let (sep, _) = ui.allocate_exact_size(Vec2::new(9.0, 20.0), Sense::hover());
                ui.painter().vline(
                    sep.center().x,
                    sep.y_range(),
                    Stroke::new(1.0, pal.border_soft),
                );
                ui.add_space(2.0);

                egui::MenuBar::new().ui(ui, |ui| {
                    ui.spacing_mut().button_padding = Vec2::new(8.0, 4.0);
                    self.menus(ui);
                });

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(theme::metric::BAR_PAD_X);
                    if crate::app::widgets::accent_button(ui, "导出", &pal).clicked() {
                        self.open_export();
                    }
                    ui.add_space(2.0);
                    icon_button(ui, Icon::Theme, "切换深色 / 浅色", &pal)
                        .clicked()
                        .then(|| {
                            let ctx = ui.ctx().clone();
                            self.toggle_theme(&ctx);
                        });
                    icon_button(ui, Icon::Command, "命令面板 (Ctrl+K)", &pal)
                        .clicked()
                        .then(|| {
                            self.command_open = true;
                            self.command_query.clear();
                            self.command_focus = true;
                        });
                    let redo = self.redo.len();
                    ui.add_enabled_ui(redo > 0, |ui| {
                        if icon_button(ui, Icon::Redo, "重做 (Ctrl+Y)", &pal).clicked() {
                            self.redo();
                        }
                    });
                    let undo = self.undo.len();
                    ui.add_enabled_ui(undo > 0, |ui| {
                        if icon_button(ui, Icon::Undo, "撤销 (Ctrl+Z)", &pal).clicked() {
                            self.undo();
                        }
                    });
                    ui.add_space(4.0);
                    let (sep, _) = ui.allocate_exact_size(Vec2::new(9.0, 20.0), Sense::hover());
                    ui.painter().vline(
                        sep.center().x,
                        sep.y_range(),
                        Stroke::new(1.0, pal.border_soft),
                    );
                });
            });
        });
    }

    fn menus(&mut self, ui: &mut Ui) {
        ui.menu_button("文件", |ui| {
            if ui.button("新建工程").clicked() {
                self.new_project();
                ui.close();
            }
            if ui.button("打开工程…").clicked() {
                self.open_project();
                ui.close();
            }
            ui.separator();
            if ui.button("保存").clicked() {
                self.save();
                ui.close();
            }
            if ui.button("另存为…").clicked() {
                self.save_as();
                ui.close();
            }
            ui.separator();
            if ui.button("导入 CSV…").clicked() {
                self.import_csv();
                ui.close();
            }
            if ui.button("导出图表…").clicked() {
                self.open_export();
                ui.close();
            }
            ui.separator();
            if ui.button("退出").clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });

        ui.menu_button("编辑", |ui| {
            if ui
                .add_enabled(!self.undo.is_empty(), egui::Button::new("撤销    Ctrl+Z"))
                .clicked()
            {
                self.undo();
                ui.close();
            }
            if ui
                .add_enabled(!self.redo.is_empty(), egui::Button::new("重做    Ctrl+Y"))
                .clicked()
            {
                self.redo();
                ui.close();
            }
            ui.separator();
            ui.menu_button("新建数据表", |ui| {
                for kind in TableKind::ALL {
                    if ui.button(kind.label()).clicked() {
                        self.add_table(kind);
                        ui.close();
                    }
                }
            });
        });

        ui.menu_button("分析", |ui| {
            use crate::core::model::{AnalysisGroup, AnalysisKind};
            let table_kind = self
                .current_table_id()
                .and_then(|id| self.project.table(id))
                .map(|t| t.kind);
            let kinds = table_kind
                .map(AnalysisKind::available_for)
                .unwrap_or_default();
            if kinds.is_empty() {
                ui.weak("当前数据表暂无可用分析");
            }
            // 按「问题类型」分组：36 种分析平铺成一条长清单没法扫读。
            for g in AnalysisGroup::ALL {
                let in_group: Vec<AnalysisKind> =
                    kinds.iter().copied().filter(|k| k.group() == g).collect();
                if in_group.is_empty() {
                    continue;
                }
                ui.menu_button(g.label(), |ui| {
                    for kind in in_group {
                        if ui.button(kind.label()).clicked() {
                            self.run_analysis(kind);
                            ui.close();
                        }
                    }
                });
            }
        });

        ui.menu_button("图表", |ui| {
            if ui.button("新建拼版页").clicked() {
                self.add_layout_page();
                ui.close();
            }
            if ui.button("用当前表新建图表").clicked() {
                self.add_graph();
                ui.close();
            }
            ui.separator();
            let current_kind = self.current_spec().map(|s| s.kind);
            let table_kind = self
                .current_table_id()
                .and_then(|id| self.project.table(id))
                .map(|t| t.kind);
            if let Some(mut next) = current_kind {
                let groups = table_kind
                    .map(crate::core::graph::ChartKind::groups_for)
                    .unwrap_or_default();
                ui.menu_button("图型", |ui| {
                    for (fam, kinds) in &groups {
                        ui.menu_button(*fam, |ui| {
                            for k in kinds {
                                if ui.selectable_label(next == *k, k.label()).clicked() {
                                    next = *k;
                                    ui.close();
                                }
                            }
                        });
                    }
                });
                if Some(next) != current_kind {
                    if let Some(s) = self.spec_mut() {
                        s.kind = next;
                    }
                    self.bump_chart();
                }
            }
            if ui.button("自动显著性标注").clicked() {
                self.auto_significance();
                ui.close();
            }
        });

        ui.menu_button("视图", |ui| {
            let mut sidebar = self.show_sidebar;
            let mut inspector = self.show_inspector;
            if ui.checkbox(&mut sidebar, "侧栏").changed() {
                self.show_sidebar = sidebar;
            }
            if ui.checkbox(&mut inspector, "检查器").changed() {
                self.show_inspector = inspector;
            }
        });

        ui.menu_button("设置", |ui| {
            if ui.button("偏好设置…").clicked() {
                self.settings_ui.open();
                ui.close();
            }
        });

        ui.menu_button("帮助", |ui| {
            if ui.button("命令面板    Ctrl+K").clicked() {
                self.command_open = true;
                self.command_query.clear();
                self.command_focus = true;
                ui.close();
            }
            if ui.button("关于 StatAura").clicked() {
                self.about_open = true;
                ui.close();
            }
        });
    }

    pub fn run_command(&mut self, cmd: Command, ctx: &egui::Context) {
        match cmd {
            Command::NewProject => self.new_project(),
            Command::Open => self.open_project(),
            Command::Save => self.save(),
            Command::Export => self.open_export(),
            Command::NewGraph => self.add_graph(),
            Command::NewLayout => self.add_layout_page(),
            Command::AutoSig => self.auto_significance(),
            Command::Undo => self.undo(),
            Command::Redo => self.redo(),
            Command::ToggleTheme => self.toggle_theme(ctx),
            Command::ToggleInspector => self.show_inspector = !self.show_inspector,
            Command::ToggleSidebar => self.show_sidebar = !self.show_sidebar,
            Command::ImportCsv => self.import_csv(),
            Command::About => self.about_open = true,
            Command::Analyze(kind) => self.run_analysis(kind),
            Command::NewTable(kind) => self.add_table(kind),
            Command::GraphKind(kind) => {
                if let Some(s) = self.spec_mut() {
                    s.kind = kind;
                }
                self.bump_chart();
            }
        }
        let _ = (ToastKind::Info, theme::GAP);
    }
}
