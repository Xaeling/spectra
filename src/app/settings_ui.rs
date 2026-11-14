//! 设置对话框 UI（移植自 Spectra ui/settings.rs，适配 egui）。
//!
//! 七类分页：外观 / 视图 / 文件 / 图表 / 分析 / 导出 / 关于。
//! 草稿模式：改的是 `draft`，点「确认/应用」才落 `live`。

use crate::app::settings::{self, Edit, Settings};
use crate::app::theme::Palette;
use eframe::egui::{self, Align2, Color32, RichText, Slider, Vec2};

pub const TABS: [&str; 7] = ["外观", "视图", "文件", "图表", "分析", "导出", "关于"];

/// 设置对话框状态。
pub struct SettingsUi {
    pub open: bool,
    pub tab: usize,
    pub draft: Settings,
}

impl Default for SettingsUi {
    fn default() -> Self {
        Self {
            open: false,
            tab: 0,
            draft: Settings::default(),
        }
    }
}

impl SettingsUi {
    pub fn open(&mut self) {
        self.draft = settings::get();
        self.open = true;
        self.tab = 0;
    }

    /// 渲染设置对话框。返回 true 表示已确认/应用（调用方应落盘）。
    pub fn show(&mut self, ctx: &egui::Context, pal: &Palette) -> bool {
        if !self.open {
            return false;
        }
        let mut confirmed = false;
        let mut applied = false;
        let mut cancelled = false;

        egui::Window::new("设置")
            .id(egui::Id::unique("settings_dialog"))
            .collapsible(false)
            .resizable(true)
            .default_width(640.0)
            .default_height(480.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                // 分页标签栏
                ui.horizontal(|ui| {
                    for (i, name) in TABS.iter().enumerate() {
                        let selected = self.tab == i;
                        let text = if selected {
                            RichText::new(*name).strong().color(pal.accent_ink)
                        } else {
                            RichText::new(*name).color(pal.muted)
                        };
                        if ui.selectable_label(selected, text).clicked() {
                            self.tab = i;
                        }
                    }
                });
                ui.separator();

                // 分页内容
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let draft = &mut self.draft;
                    match self.tab {
                        0 => appearance_tab(ui, draft, pal),
                        1 => view_tab(ui, draft, pal),
                        2 => files_tab(ui, draft, pal),
                        3 => chart_tab(ui, draft, pal),
                        4 => analysis_tab(ui, draft, pal),
                        5 => export_tab(ui, draft, pal),
                        _ => about_tab(ui),
                    }
                });

                ui.separator();
                ui.horizontal(|ui| {
                    let btn_size = egui::Vec2::new(72.0, 28.0);
                    if ui.add_sized(btn_size, egui::Button::new("确认")).clicked() {
                        confirmed = true;
                    }
                    if ui.add_sized(btn_size, egui::Button::new("应用")).clicked() {
                        applied = true;
                    }
                    if ui.add_sized(btn_size, egui::Button::new("取消")).clicked() {
                        cancelled = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_sized(egui::Vec2::new(88.0, 28.0), egui::Button::new("恢复默认"))
                            .clicked()
                        {
                            self.draft = Settings::default();
                        }
                    });
                });
            });

        if confirmed || applied {
            settings::replace(self.draft.clone());
            settings::save_current();
        }
        if confirmed || cancelled {
            self.open = false;
        }
        confirmed || applied
    }
}

// ── 各分页 ──────────────────────────────────────────────────────────────────

fn appearance_tab(ui: &mut egui::Ui, s: &mut Settings, pal: &Palette) {
    section(ui, "语言 / Language", pal);
    ui.horizontal(|ui| {
        for (i, name) in ["中文", "English"].iter().enumerate() {
            if ui.selectable_label(s.language == i, *name).clicked() {
                settings::apply(s, Edit::Language(i));
            }
        }
    });

    section(ui, "主题", pal);
    ui.horizontal(|ui| {
        for (i, name) in settings::THEMES.iter().enumerate() {
            if ui.selectable_label(s.theme == i, *name).clicked() {
                settings::apply(s, Edit::Theme(i));
            }
        }
    });

    section(ui, "强调色", pal);
    ui.horizontal(|ui| {
        for (i, hex) in settings::ACCENTS.iter().enumerate() {
            let c = parse_hex(hex);
            let selected = s.accent == i;
            let size = Vec2::splat(24.0);
            let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
            ui.painter().rect_filled(rect, 6.0, c);
            if selected {
                ui.painter().rect_stroke(
                    rect.expand(2.0),
                    6.0,
                    egui::Stroke::new(2.0, pal.ink),
                    egui::StrokeKind::Outside,
                );
            }
            if resp.clicked() {
                settings::apply(s, Edit::Accent(i));
            }
        }
    });

    section(ui, "外观", pal);
    slider_row(
        ui,
        "缩放",
        s.font_scale,
        0.85,
        1.60,
        pal,
        Edit::FontScale,
        s,
    );
    slider_row(ui, "圆角", s.radius, 0.0, 2.0, pal, Edit::Radius, s);
    slider_row(ui, "不透明度", s.opacity, 0.5, 1.0, pal, Edit::Opacity, s);
    check_row(
        ui,
        "高对比模式",
        s.high_contrast,
        pal,
        Edit::HighContrast,
        s,
    );
    check_row(ui, "动画", s.animate, pal, Edit::Animate, s);
    check_row(ui, "无障碍增强", s.a11y, pal, Edit::A11y, s);
}

fn view_tab(ui: &mut egui::Ui, s: &mut Settings, pal: &Palette) {
    section(ui, "数据网格", pal);
    slider_row(
        ui,
        "默认行数",
        s.grid_rows as f32,
        5.0,
        200.0,
        pal,
        Edit::GridRows,
        s,
    );
    slider_row(
        ui,
        "默认列数",
        s.grid_cols as f32,
        2.0,
        52.0,
        pal,
        Edit::GridCols,
        s,
    );
    slider_row(
        ui,
        "行标题宽度",
        s.nav_width,
        150.0,
        420.0,
        pal,
        Edit::NavWidth,
        s,
    );

    section(ui, "界面", pal);
    check_row(
        ui,
        "显示导航栏",
        s.show_navigator,
        pal,
        Edit::ShowNavigator,
        s,
    );
    check_row(
        ui,
        "显示检查器",
        s.show_inspector,
        pal,
        Edit::ShowInspector,
        s,
    );
    check_row(ui, "显示状态栏", s.show_status, pal, Edit::ShowStatus, s);
    slider_row(
        ui,
        "最近文件数",
        s.recent_count as f32,
        0.0,
        20.0,
        pal,
        Edit::RecentCount,
        s,
    );
    slider_row(
        ui,
        "动画速度 (ms)",
        s.anim_ms as f32,
        0.0,
        800.0,
        pal,
        Edit::AnimMs,
        s,
    );
}

fn files_tab(ui: &mut egui::Ui, s: &mut Settings, pal: &Palette) {
    section(ui, "文件", pal);
    text_row(ui, "默认打开目录", &mut s.open_dir, pal);
    text_row(ui, "默认保存目录", &mut s.save_dir, pal);
    text_row(ui, "作者", &mut s.author, pal);

    section(ui, "备份", pal);
    check_row(ui, "自动备份", s.auto_backup, pal, Edit::AutoBackup, s);
    slider_row(
        ui,
        "备份间隔 (分钟)",
        s.backup_min as f32,
        1.0,
        240.0,
        pal,
        Edit::BackupMin,
        s,
    );
    check_row(
        ui,
        "建表前先存盘",
        s.save_before_table,
        pal,
        Edit::SaveBeforeTable,
        s,
    );

    section(ui, "打印", pal);
    slider_row(ui, "页边距", s.margin, 0.0, 60.0, pal, Edit::Margin, s);
    text_row(ui, "页眉", &mut s.header, pal);
    text_row(ui, "页脚", &mut s.footer, pal);
}

fn chart_tab(ui: &mut egui::Ui, s: &mut Settings, pal: &Palette) {
    section(ui, "新建图表默认值", pal);
    ui.horizontal(|ui| {
        ui.label("图表类型：");
        egui::ComboBox::from_id_salt("def_kind")
            .selected_text(settings::GRID_KINDS[s.def_kind.min(7)])
            .show_ui(ui, |ui| {
                for (i, name) in settings::GRID_KINDS.iter().enumerate() {
                    if ui.selectable_label(s.def_kind == i, *name).clicked() {
                        settings::apply(s, Edit::DefKind(i));
                    }
                }
            });
    });
    check_row(ui, "显示散点", s.def_points, pal, Edit::DefPoints, s);
    check_row(ui, "显示连线", s.def_line, pal, Edit::DefLine, s);
    check_row(ui, "显示误差棒", s.def_error, pal, Edit::DefError, s);

    section(ui, "显著性标注", pal);
    ui.horizontal(|ui| {
        for (i, name) in settings::BRACKET_DEFAULTS.iter().enumerate() {
            if ui.selectable_label(s.def_bracket == i, *name).clicked() {
                settings::apply(s, Edit::DefBracket(i));
            }
        }
    });
    text_row(ui, "标注符号", &mut s.def_bracket_label, pal);
    ui.horizontal(|ui| {
        ui.label("标注格式：");
        for (i, name) in ["只星号", "只 P 值", "星号+P 值"].iter().enumerate() {
            if ui
                .selectable_label(s.bracket_label_fmt == i, *name)
                .clicked()
            {
                settings::apply(s, Edit::BracketLabelFmt(i));
            }
        }
    });
}

fn analysis_tab(ui: &mut egui::Ui, s: &mut Settings, pal: &Palette) {
    section(ui, "统计", pal);
    ui.horizontal(|ui| {
        ui.label("置信水平：");
        for (i, name) in settings::CONF_LEVELS.iter().enumerate() {
            if ui.selectable_label(s.conf_level == i, *name).clicked() {
                settings::apply(s, Edit::ConfLevel(i));
            }
        }
    });
    slider_row(ui, "显著性 α", s.alpha, 0.001, 0.20, pal, Edit::Alpha, s);
    slider_row(
        ui,
        "小数位",
        s.decimals as f32,
        0.0,
        10.0,
        pal,
        Edit::Decimals,
        s,
    );

    section(ui, "P 值格式", pal);
    ui.horizontal(|ui| {
        for (i, name) in settings::P_FORMATS.iter().enumerate() {
            if ui.selectable_label(s.p_format == i, *name).clicked() {
                settings::apply(s, Edit::PFormat(i));
            }
        }
    });
    check_row(ui, "自动科学记数", s.auto_sci, pal, Edit::AutoSci, s);

    section(ui, "离群值", pal);
    ui.horizontal(|ui| {
        for (i, name) in settings::OUTLIER_METHODS.iter().enumerate() {
            if ui.selectable_label(s.outlier == i, *name).clicked() {
                settings::apply(s, Edit::Outlier(i));
            }
        }
    });

    section(ui, "随机种子", pal);
    ui.horizontal(|ui| {
        ui.label("种子：");
        let mut seed_str = s.seed.to_string();
        if ui
            .add(egui::TextEdit::singleline(&mut seed_str).desired_width(120.0))
            .changed()
        {
            settings::apply(s, Edit::Seed(seed_str));
        }
        // 自绘骰子图标（MiSans 不含 emoji 字形，用 Painter 画）
        let dice_size = Vec2::splat(22.0);
        let (rect, resp) = ui.allocate_exact_size(dice_size, egui::Sense::click());
        let painter = ui.painter();
        let stroke = egui::Stroke::new(1.5, pal.ink);
        // 骰子外框（圆角方块）
        painter.rect_stroke(rect.shrink(2.0), 4.0, stroke, egui::StrokeKind::Middle);
        // 五个圆点（骰子面"5"）
        let c = rect.center();
        let d = 4.0;
        for (dx, dy) in [(-d, -d), (d, -d), (-d, d), (d, d), (0.0, 0.0)] {
            painter.circle_filled(egui::pos2(c.x + dx, c.y + dy), 1.8, pal.ink);
        }
        if resp.clicked() {
            s.seed = settings::random_seed();
        }
        resp.on_hover_text("随机生成新种子");
    });
}

fn export_tab(ui: &mut egui::Ui, s: &mut Settings, pal: &Palette) {
    section(ui, "导出", pal);
    slider_row(
        ui,
        "默认 DPI",
        s.export_dpi as f32,
        72.0,
        1200.0,
        pal,
        |v| Edit::ExportDpi(v as usize),
        s,
    );
    slider_row(
        ui,
        "默认宽度 (px)",
        s.export_width,
        320.0,
        8000.0,
        pal,
        Edit::ExportWidth,
        s,
    );
    ui.horizontal(|ui| {
        ui.label("默认格式：");
        for (i, name) in ["PNG", "JPEG", "TIFF", "SVG", "PDF"].iter().enumerate() {
            if ui.selectable_label(s.export_format == i, *name).clicked() {
                settings::apply(s, Edit::ExportFormat(i));
            }
        }
    });
}

fn about_tab(ui: &mut egui::Ui) {
    ui.add_space(16.0);
    ui.vertical_centered(|ui| {
        ui.heading("StatAura");
        ui.label(format!("版本 {}", env!("CARGO_PKG_VERSION")));
        ui.label("科研数据统计与出版级绘图");
        ui.add_space(8.0);
        ui.label("设置文件位置：");
        ui.monospace(Settings::path().to_string_lossy().to_string());
        ui.add_space(8.0);
        ui.label("基于 Rust + egui + tiny-skia");
        ui.label("兼容 GraphPad Prism .prism 格式");
    });
}

// ── 辅助控件 ────────────────────────────────────────────────────────────────

fn section(ui: &mut egui::Ui, title: &str, pal: &Palette) {
    ui.add_space(8.0);
    ui.label(RichText::new(title).strong().size(13.0).color(pal.ink));
    ui.add_space(2.0);
}

fn slider_row(
    ui: &mut egui::Ui,
    label: &str,
    val: f32,
    min: f32,
    max: f32,
    pal: &Palette,
    edit: impl Fn(f32) -> Edit,
    s: &mut Settings,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(pal.muted));
        let mut v = val;
        if ui.add(Slider::new(&mut v, min..=max)).changed() {
            settings::apply(s, edit(v));
        }
    });
}

fn check_row(
    ui: &mut egui::Ui,
    label: &str,
    checked: bool,
    pal: &Palette,
    edit: impl Fn(bool) -> Edit,
    s: &mut Settings,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(pal.muted));
        let mut v = checked;
        if ui.checkbox(&mut v, "").changed() {
            settings::apply(s, edit(v));
        }
    });
}

fn text_row(ui: &mut egui::Ui, label: &str, val: &mut String, pal: &Palette) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(pal.muted));
        ui.add(egui::TextEdit::singleline(val).desired_width(280.0));
    });
}

fn parse_hex(hex: &str) -> Color32 {
    let h = hex.trim_start_matches('#');
    if h.len() >= 6 {
        let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(0);
        Color32::from_rgb(r, g, b)
    } else {
        Color32::GRAY
    }
}
