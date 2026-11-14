//! 导出与关于窗口。
use crate::core::model::TableKind;

use std::path::PathBuf;

use egui::{Align2, Color32, FontId, RichText, Stroke, Vec2};

use crate::app::form::Form;
use crate::app::theme::{metric, radius, text, Palette};
use crate::app::widgets::{self, surface};
use crate::app::{sanitize, App, Sheet, ToastKind};
use crate::io::ExportFormat;
use crate::plot::render;

impl App {
    pub fn dialogs(&mut self, ctx: &egui::Context) {
        // 欢迎弹窗（空项目启动时）
        if self.new_table_dialog_open {
            self.welcome_dialog(ctx);
        }
        if self.export.open {
            let mut open = true;
            let pal = self.pal;
            let mut run = false;
            egui::Window::new("导出图表")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(520.0)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    // 固定内容宽度：窗口宽度由它决定，避免 available_width 自我放大。
                    ui.set_width(484.0);
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
                    let mut f = Form::new(ui, pal);
                    f.gap(2.0);
                    f.note("出版级输出：屏幕、位图与矢量共用同一份场景。");
                    f.gap(6.0);

                    // 格式：4 个等宽块，选中态用强调色实心。
                    let names: Vec<(ExportFormat, &str)> =
                        ExportFormat::ALL.iter().map(|f| (*f, f.label())).collect();
                    let cur = self.export.format;
                    let picked = f.row_h("格式", 34.0, |ui| {
                        let mut picked = None;
                        let total = ui.available_width();
                        let gap = 6.0;
                        let w = (total - gap * 3.0) / 4.0;
                        ui.spacing_mut().item_spacing.x = gap;
                        for (v, name) in &names {
                            let sel = cur == *v;
                            let (r, resp) =
                                ui.allocate_exact_size(Vec2::new(w, 32.0), egui::Sense::click());
                            let (bg, fg, st) = if sel {
                                (pal.accent, Color32::WHITE, Stroke::NONE)
                            } else if resp.hovered() {
                                (pal.hover, pal.ink, Stroke::new(1.0, pal.border))
                            } else {
                                (pal.panel_alt, pal.ink, Stroke::new(1.0, pal.border))
                            };
                            surface(ui.painter(), r, radius::CONTROL, bg, st);
                            ui.painter().text(
                                r.center(),
                                Align2::CENTER_CENTER,
                                *name,
                                FontId::proportional(text::BODY),
                                fg,
                            );
                            if resp.clicked() && !sel {
                                picked = Some(*v);
                            }
                        }
                        picked
                    });
                    if let Some(v) = picked {
                        self.export.format = v;
                        let p = PathBuf::from(&self.export.path);
                        if let Some(stem) = p.file_stem() {
                            let mut np = p.clone();
                            np.set_file_name(format!("{}.{}", stem.to_string_lossy(), v.ext()));
                            self.export.path = np.display().to_string();
                        }
                    }
                    f.note(self.export.format.hint());

                    f.section("尺寸");
                    f.row("宽度", |ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.export.width_in)
                                .range(2.0..=18.0)
                                .speed(0.05)
                                .suffix(" in"),
                        );
                        ui.label(
                            RichText::new(format!("约 {:.1} cm", self.export.width_in * 2.54))
                                .size(text::MICRO)
                                .color(pal.faint),
                        );
                    });
                    let mut dpi = self.export.dpi;
                    let dots = [
                        (72u32, "72"),
                        (150u32, "150"),
                        (300u32, "300"),
                        (600u32, "600"),
                    ];
                    if f.segmented("分辨率", &mut dpi, &dots) {
                        self.export.dpi = dpi;
                    }
                    if self.export.format.is_vector() {
                        f.note("矢量格式忽略 dpi，按 4× 精度生成路径。");
                    }
                    f.row("文件", |ui| {
                        // 从右往左排：先放按钮，文本框吃掉剩余宽度，按钮不会被挤扁。
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_sized(
                                    [58.0, metric::CONTROL_H],
                                    egui::Button::new(RichText::new("浏览…").size(text::LABEL))
                                        .corner_radius(radius::CONTROL as u8),
                                )
                                .clicked()
                            {
                                let ext = self.export.format.ext();
                                if let Some(p) = rfd::FileDialog::new()
                                    .add_filter(self.export.format.label(), &[ext])
                                    .set_file_name(format!(
                                        "{}.{}",
                                        sanitize(&self.export.path),
                                        ext
                                    ))
                                    .save_file()
                                {
                                    self.export.path = p.display().to_string();
                                }
                            }
                            let w = ui.available_width();
                            ui.add_sized(
                                [w, metric::CONTROL_H],
                                egui::TextEdit::singleline(&mut self.export.path)
                                    .font(FontId::proportional(text::BODY))
                                    .margin(Vec2::new(8.0, 4.0)),
                            );
                        });
                    });

                    let (pw, ph) = self.pixel_size();
                    f.gap(2.0);
                    f.note(&format!("输出尺寸：{pw} × {ph} px"));

                    f.section("执行");
                    let resp = f.row_h("", 32.0, |ui| {
                        if widgets::Btn::new("导出")
                            .fill(pal.accent)
                            .fg(Color32::WHITE)
                            .strong(true)
                            .radius(radius::CONTROL)
                            .height(32.0)
                            .min_width(96.0)
                            .show(ui)
                            .clicked()
                        {
                            run = true;
                        }
                        if let Some(s) = &self.export.status {
                            let color = if s.starts_with("失败") || s.starts_with("请") {
                                pal.danger
                            } else {
                                pal.success
                            };
                            ui.add_space(4.0);
                            ui.label(RichText::new(s).size(text::LABEL).color(color));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_sized(
                                    [68.0, 32.0],
                                    egui::Button::new(RichText::new("关闭").size(text::BODY))
                                        .corner_radius(radius::CONTROL as u8),
                                )
                                .clicked()
                            {
                                self.export.open = false;
                            }
                        });
                    });
                    let _ = resp;
                });
            self.export.open = open && self.export.open;
            if run {
                self.do_export();
            }
        }

        if let Some(confirm) = self.confirm {
            let pal = self.pal;
            let (title, body) = match confirm {
                crate::app::Confirm::New => ("新建工程", "当前工程有未保存的修改。要先保存吗？"),
                crate::app::Confirm::Open => ("打开工程", "当前工程有未保存的修改。要先保存吗？"),
                crate::app::Confirm::Close => {
                    ("退出 StatAura", "当前工程有未保存的修改。要先保存吗？")
                }
            };
            let mut action: u8 = 0;
            egui::Window::new(title)
                .collapsible(false)
                .resizable(false)
                .default_width(400.0)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.add_space(2.0);
                    ui.label(RichText::new(body).size(text::BODY).color(pal.ink));
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if widgets::Btn::new("保存")
                            .fill(pal.accent)
                            .fg(Color32::WHITE)
                            .strong(true)
                            .radius(radius::CONTROL)
                            .height(30.0)
                            .min_width(78.0)
                            .show(ui)
                            .clicked()
                        {
                            action = 1;
                        }
                        if widgets::Btn::new("不保存")
                            .fill(pal.panel_alt)
                            .fg(pal.ink)
                            .stroke(Stroke::new(1.0, pal.border))
                            .radius(radius::CONTROL)
                            .height(30.0)
                            .min_width(78.0)
                            .show(ui)
                            .clicked()
                        {
                            action = 2;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::Btn::new("取消")
                                .fill(Color32::TRANSPARENT)
                                .fg(pal.muted)
                                .radius(radius::CONTROL)
                                .height(30.0)
                                .min_width(62.0)
                                .show(ui)
                                .clicked()
                            {
                                action = 3;
                            }
                        });
                    });
                });
            match action {
                1 => {
                    self.save();
                    if !self.dirty {
                        match confirm {
                            crate::app::Confirm::New => {
                                self.confirm = None;
                                self.new_project_now();
                            }
                            crate::app::Confirm::Open => self.proceed_open(),
                            crate::app::Confirm::Close => {
                                self.confirm = None;
                                self.allow_close = true;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                    }
                }
                2 => {
                    self.confirm = None;
                    self.dirty = false;
                    match confirm {
                        crate::app::Confirm::New => self.new_project_now(),
                        crate::app::Confirm::Open => self.proceed_open(),
                        crate::app::Confirm::Close => {
                            self.allow_close = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                }
                3 => self.confirm = None,
                _ => {}
            }
        }

        if self.about_open {
            let mut open = self.about_open;
            let pal = self.pal;
            egui::Window::new("关于 StatAura")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(420.0)
                .show(ctx, |ui| {
                    about(ui, &pal);
                });
            self.about_open = open;
        }
    }

    fn pixel_size(&self) -> (u32, u32) {
        let scale = self.export.dpi as f32 / 72.0;
        let w = (self.export.width_in * 72.0 * scale).round() as u32;
        let h = (w as f32 * 318.0 / 468.0).round() as u32;
        (w, h)
    }

    fn do_export(&mut self) {
        // 单图与拼版页走同一导出管线；差别只在「渲染哪个 Render」。
        enum ExportJob {
            Graph {
                spec: crate::core::graph::GraphSpec,
                table: crate::core::model::DataTable,
            },
            Layout {
                page: crate::core::model::LayoutPage,
                panels: Vec<(crate::core::graph::GraphSpec, crate::core::model::DataTable)>,
            },
        }
        let job = if let Some(Sheet::Layout(lid)) = self.sel {
            let Some(page) = self.project.layout(lid).cloned() else {
                self.export.status = Some("拼版页不存在".into());
                return;
            };
            let panels: Vec<_> = page
                .panels
                .iter()
                .filter_map(|gid| {
                    let g = self.project.graph(*gid)?;
                    let t = self.project.table(g.table)?;
                    Some((g.spec.clone(), t.clone()))
                })
                .collect();
            if panels.is_empty() {
                self.export.status = Some("拼版页还没有可用面板".into());
                return;
            }
            ExportJob::Layout { page, panels }
        } else {
            let Some(gid) = self.current_graph_id() else {
                self.export.status = Some("没有选中图表".into());
                return;
            };
            let (spec, table) = {
                let Some(g) = self.project.graph(gid) else {
                    return;
                };
                let Some(t) = self.project.table(g.table) else {
                    self.export.status = Some("图表引用的数据表不存在".into());
                    return;
                };
                (g.spec.clone(), t.clone())
            };
            ExportJob::Graph { spec, table }
        };
        let path = PathBuf::from(self.export.path.trim());
        if path.as_os_str().is_empty() {
            self.export.status = Some("请填写导出路径".into());
            return;
        }
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let scale = if self.export.format.is_vector() {
            4.0
        } else {
            self.export.dpi as f32 / 72.0
        };
        let width_pt = self.export.width_in * 72.0;
        let started = std::time::Instant::now();
        let r = match job {
            ExportJob::Graph { spec, table } => {
                let height_pt = width_pt * 318.0 / 468.0;
                render(&spec, &table, width_pt, height_pt, scale, &self.fonts)
            }
            ExportJob::Layout { page, panels } => {
                // 拼版纵横比由面板数决定：保持面板 468×318 的比例逐格排。
                let (w_pt, _h_pt) = crate::plot::layout::layout_size(&page, 468.0, 318.0);
                let k = width_pt / w_pt;
                let refs: Vec<_> = panels.iter().map(|(sp, t)| (sp, t)).collect();
                crate::plot::layout::render_layout(
                    &page,
                    &refs,
                    468.0,
                    318.0,
                    scale * k,
                    &self.fonts,
                )
            }
        };
        let height_pt = r.height as f32 / (r.px_per_pt.max(0.05));
        match crate::io::export_chart(&r, self.export.format, width_pt, height_pt, &path) {
            Ok(()) => {
                let ms = started.elapsed().as_millis();
                self.export.status = Some(format!("已导出（{ms} ms）"));
                self.toast("图表已导出", ToastKind::Success);
            }
            Err(e) => {
                self.export.status = Some(format!("失败：{e}"));
                self.toast(format!("导出失败：{e}"), ToastKind::Error);
            }
        }
    }

    /// 新建表弹窗：直接选择表型创建。
    fn welcome_dialog(&mut self, ctx: &egui::Context) {
        let mut close = false;
        let mut create: Option<TableKind> = None;
        egui::Window::new("新建表格")
            .id(egui::Id::unique("new_table_dialog"))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.label("选择表格类型：");
                ui.add_space(8.0);
                for kind in TableKind::ALL {
                    if ui
                        .selectable_label(false, format!("  {}", kind.label()))
                        .clicked()
                    {
                        create = Some(kind);
                    }
                }
                ui.add_space(8.0);
                if ui.button("关闭").clicked() {
                    close = true;
                }
                ui.add_space(4.0);
            });
        if close {
            self.new_table_dialog_open = false;
        }
        if let Some(kind) = create {
            self.new_table_dialog_open = false;
            self.add_table(kind);
        }
    }
}

fn about(ui: &mut egui::Ui, pal0: &Palette) {
    let pal = *pal0;
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        let (logo, _) = ui.allocate_exact_size(Vec2::splat(34.0), egui::Sense::hover());
        surface(
            ui.painter(),
            logo,
            radius::CONTROL,
            pal.accent,
            Stroke::NONE,
        );
        ui.painter().text(
            logo.center(),
            Align2::CENTER_CENTER,
            "S",
            FontId::proportional(19.0),
            Color32::WHITE,
        );
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(RichText::new("StatAura").size(19.0).color(pal.ink));
            ui.label(
                RichText::new(format!(
                    "科研统计与出版级绘图 · v{}",
                    env!("CARGO_PKG_VERSION")
                ))
                .size(text::META)
                .color(pal.muted),
            );
        });
    });
    ui.add_space(12.0);
    ui.label(
        RichText::new(
            "数据处理、统计检验与出版图在同一条工作流里完成。\
             图表由自研矢量引擎渲染，PNG / SVG / PDF 版式一致，文字全部转曲。",
        )
        .size(text::LABEL)
        .color(pal.muted),
    );
    ui.add_space(10.0);
    ui.label(
        RichText::new("Rust · egui · tiny-skia · ab_glyph")
            .size(text::MICRO)
            .color(pal.faint),
    );
    ui.add_space(12.0);
    let mut f = Form::new(ui, pal);
    f.section("快捷键");
    for (k, v) in [
        ("Ctrl+K", "命令面板"),
        ("Ctrl+S", "保存"),
        ("Ctrl+E", "导出"),
        ("Ctrl+Z / Ctrl+Y", "撤销 / 重做"),
        ("Enter / Tab", "网格内确认并移动"),
    ] {
        f.row(k, |ui| {
            ui.label(RichText::new(v).size(text::LABEL).color(pal.muted));
        });
    }
}
