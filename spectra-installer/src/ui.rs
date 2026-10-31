//! eframe / egui 动画界面

use crate::flow::Msg;
use crate::model;
use eframe::egui;
use std::sync::mpsc::Receiver;
use std::time::Instant;

pub struct InstallerApp {
    uninstall: bool,
    progress: f32,
    step: String,
    logs: Vec<String>,
    rx: Receiver<Msg>,
    finished: Option<(bool, String)>,
    finish_at: Option<Instant>,
    _start: Instant,
}

/// egui 默认字体不含中日韩字形，必须补一个系统字体，否则中文会渲染成方框
fn setup_fonts(ctx: &egui::Context) {
    let candidates = [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\msyhl.ttc",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\simsun.ttc",
    ];
    for p in candidates {
        let Ok(bytes) = std::fs::read(p) else { continue };
        let mut fonts = egui::FontDefinitions::default();
        fonts
            .font_data
            .insert("cjk".to_owned(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
        if let Some(v) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            v.insert(0, "cjk".to_owned());
        }
        if let Some(v) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            v.insert(0, "cjk".to_owned());
        }
        ctx.set_fonts(fonts);
        break;
    }
}

impl InstallerApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        uninstall: bool,
        rx: Receiver<Msg>,
    ) -> Self {
        setup_fonts(&cc.egui_ctx);
        Self {
            uninstall,
            progress: 0.0,
            step: "准备中…".to_string(),
            logs: Vec::new(),
            rx,
            finished: None,
            finish_at: None,
            _start: Instant::now(),
        }
    }
}

impl eframe::App for InstallerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // eframe 0.31 起 App trait 的成员是 ui 而非 update，
        // 这里先取一份 Context 备用（Context 内部是 Arc，clone 很便宜）
        let ctx = ui.ctx().clone();
        // 先排空通道，避免借用冲突
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Progress(p, s) => {
                    self.progress = p;
                    self.step = s.clone();
                    self.logs.push(s);
                }
                Msg::Log(s) => self.logs.push(s),
                Msg::Finished(ok, s) => {
                    if ok {
                        self.progress = 1.0;
                    }
                    self.step = s.clone();
                    self.logs.push(s.clone());
                    self.finished = Some((ok, s));
                    self.finish_at = Some(Instant::now());
                }
            }
        }
        if self.logs.len() > 200 {
            let n = self.logs.len() - 200;
            self.logs.drain(..n);
        }

        let title = if self.uninstall { "正在卸载" } else { "正在安装" };
        let app = model::APP_NAME;

        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(14.0);
            ui.vertical_centered(|ui| {
                ui.heading(
                    egui::RichText::new(format!("{} {}", title, app))
                        .strong()
                        .size(20.0),
                );
            });
            ui.add_space(20.0);

            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.add(
                    egui::ProgressBar::new(self.progress)
                        .show_percentage()
                        .animate(true)
                        .desired_width(ui.available_width() - 16.0),
                );
            });
            ui.add_space(12.0);

            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(&self.step);
            });
            ui.add_space(10.0);
            ui.separator();

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.add_space(4.0);
                    for l in &self.logs {
                        ui.horizontal(|ui| {
                            ui.add_space(8.0);
                            ui.label(egui::RichText::new(l).weak().size(12.0));
                        });
                    }
                });
        });

        // 无用户输入时也要持续重绘，动画才“活”起来
        ctx.request_repaint();

        // 结束后稍等一会再关窗，让最后一条日志能被看到
        if let Some(at) = self.finish_at {
            if at.elapsed().as_secs_f32() > 1.8 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}
