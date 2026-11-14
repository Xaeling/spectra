//! 安装器界面：固定逻辑尺寸、深色渐变 + 轻量动画，所有布局随 DPI 缩放。

use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame,
    Layout, Margin, Rect, RichText, Sense, Stroke, TextStyle, ThemePreference, UiBuilder, Vec2,
};

use crate::install::{self, Msg};
use crate::Args;

const BG0: Color32 = Color32::from_rgb(8, 12, 24);
const BG1: Color32 = Color32::from_rgb(24, 31, 56);
const CARD: Color32 = Color32::from_rgba_premultiplied(18, 24, 43, 236);
const BORDER: Color32 = Color32::from_rgb(48, 58, 92);
const INK: Color32 = Color32::from_rgb(235, 240, 255);
const MUTED: Color32 = Color32::from_rgb(158, 170, 198);
const ACCENT: Color32 = Color32::from_rgb(110, 132, 255);
const OK: Color32 = Color32::from_rgb(60, 210, 160);
const DANGER: Color32 = Color32::from_rgb(255, 120, 128);

const SANS: &[u8] = include_bytes!("../../assets/fonts/MiSans-Regular.ttf");
const LOGO: &[u8] = include_bytes!("../../assets/icon.png");

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Install,
    Installing,
    Done,
    Uninstall,
    Uninstalling,
    Uninstalled,
    Error,
}

pub struct SetupApp {
    args: Args,
    page: Page,
    dir: String,
    progress: f32,
    status: String,
    rx: Option<Receiver<Msg>>,
    error: String,
    launch: bool,
    logo: egui::TextureHandle,
    started: f64,
}

impl SetupApp {
    pub fn new(cc: &eframe::CreationContext<'_>, args: Args) -> Self {
        install_fonts(&cc.egui_ctx);
        style(&cc.egui_ctx);
        if args.ui_scale > 0.01 && (args.ui_scale - 1.0).abs() > 0.001 {
            cc.egui_ctx.set_zoom_factor(args.ui_scale);
        }
        let page = if args.uninstall {
            Page::Uninstall
        } else {
            Page::Install
        };
        let dir = args
            .dir
            .clone()
            .unwrap_or_else(|| {
                if args.uninstall {
                    install::current_dir_or_default()
                } else {
                    install::default_dir()
                }
            })
            .display()
            .to_string();
        Self {
            page,
            dir,
            progress: 0.0,
            status: String::new(),
            rx: None,
            error: String::new(),
            launch: true,
            logo: load_logo(&cc.egui_ctx),
            started: cc.egui_ctx.input(|i| i.time),
            args,
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.rx.take() else { return };
        let mut finished = false;
        loop {
            match rx.try_recv() {
                Ok(Msg::Progress(p, text)) => {
                    self.progress = p;
                    self.status = text;
                }
                Ok(Msg::Done(Ok(()))) => {
                    finished = true;
                    self.page = if self.args.uninstall {
                        Page::Uninstalled
                    } else {
                        Page::Done
                    };
                    ctx.request_repaint();
                }
                Ok(Msg::Done(Err(e))) => {
                    finished = true;
                    self.error = e;
                    self.page = Page::Error;
                    ctx.request_repaint();
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    finished = true;
                    if self.page == Page::Installing || self.page == Page::Uninstalling {
                        self.error = "安装线程意外结束".into();
                        self.page = Page::Error;
                    }
                    break;
                }
            }
        }
        if !finished {
            self.rx = Some(rx);
        }
    }

    fn start_install(&mut self, ctx: &egui::Context) {
        let dir = PathBuf::from(self.dir.trim());
        if self.dir.trim().is_empty() {
            self.error = "安装目录不能为空".into();
            self.page = Page::Error;
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        self.rx = Some(rx);
        self.page = Page::Installing;
        self.progress = 0.0;
        self.status = "准备安装".into();
        std::thread::spawn(move || {
            let result = install::install(&dir, Some(&tx)).map_err(|e| format!("{e:#}"));
            let _ = tx.send(Msg::Done(result));
        });
        ctx.request_repaint();
    }

    fn start_uninstall(&mut self, ctx: &egui::Context) {
        let dir = PathBuf::from(self.dir.trim());
        let (tx, rx) = std::sync::mpsc::channel();
        self.rx = Some(rx);
        self.page = Page::Uninstalling;
        self.progress = 0.0;
        self.status = "准备卸载".into();
        std::thread::spawn(move || {
            let result = install::uninstall(&dir, Some(&tx)).map_err(|e| format!("{e:#}"));
            let _ = tx.send(Msg::Done(result));
        });
        ctx.request_repaint();
    }

    fn header(&self, ui: &mut egui::Ui, subtitle: &str) {
        ui.horizontal(|ui| {
            ui.add(egui::Image::new(&self.logo).fit_to_exact_size(Vec2::splat(54.0)));
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new("StatAura").size(29.0).strong().color(INK));
                ui.label(RichText::new(subtitle).size(14.0).color(MUTED));
            });
        });
    }

    fn path_row(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("安装位置").size(14.0).color(MUTED));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let w = (ui.available_width() - 82.0).max(120.0);
            ui.add_sized(
                [w, 34.0],
                egui::TextEdit::singleline(&mut self.dir)
                    .font(FontId::proportional(15.0))
                    .margin(Vec2::new(9.0, 6.0)),
            );
            if ui
                .add_sized([72.0, 34.0], egui::Button::new("浏览…"))
                .clicked()
            {
                if let Some(p) = rfd::FileDialog::new()
                    .set_directory(&self.dir)
                    .pick_folder()
                {
                    self.dir = p.display().to_string();
                }
            }
        });
        ui.add_space(4.0);
        ui.label(
            RichText::new("默认安装到 Program Files；需要管理员权限。")
                .size(12.5)
                .color(MUTED),
        );
    }

    fn progress_bar(&self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width().min(560.0), 10.0),
            Sense::hover(),
        );
        ui.painter()
            .rect_filled(rect, CornerRadius::same(5), Color32::from_rgb(30, 38, 62));
        let w = rect.width() * self.progress.clamp(0.0, 1.0);
        if w > 0.5 {
            let fill = Rect::from_min_size(rect.min, Vec2::new(w, rect.height()));
            ui.painter()
                .rect_filled(fill, CornerRadius::same(5), ACCENT);
            let t = ctx.input(|i| i.time) as f32;
            let x = rect.left() + (t * 90.0) % (rect.width().max(1.0));
            let shine =
                Rect::from_center_size(egui::pos2(x, rect.center().y), Vec2::new(26.0, 10.0))
                    .intersect(fill);
            if shine.width() > 1.0 {
                ui.painter().rect_filled(
                    shine,
                    CornerRadius::same(5),
                    Color32::from_rgba_unmultiplied(255, 255, 255, 70),
                );
            }
        }
    }

    fn install_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.header(ui, "科研统计 · 出版级绘图 · 离线安装");
        ui.add_space(22.0);
        card(ui, |ui| {
            self.path_row(ui);
            ui.add_space(14.0);
            ui.label(
                RichText::new("将安装 StatAura、卸载程序，并创建桌面快捷方式与 .aura 文件关联。")
                    .size(13.5)
                    .color(MUTED),
            );
        });
        ui.add_space(22.0);
        ui.label(
            RichText::new(format!("版本 {}", install::APP_VERSION))
                .size(12.5)
                .color(MUTED),
        );
        footer(ui, |ui| {
            if ui
                .add_sized(
                    [122.0, 36.0],
                    egui::Button::new(RichText::new("开始安装").size(15.0).strong())
                        .fill(ACCENT)
                        .corner_radius(CornerRadius::same(9)),
                )
                .clicked()
            {
                self.start_install(ctx);
            }
            if ui
                .add_sized(
                    [82.0, 36.0],
                    egui::Button::new(RichText::new("退出").size(15.0))
                        .fill(Color32::TRANSPARENT)
                        .corner_radius(CornerRadius::same(9)),
                )
                .clicked()
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }

    fn installing_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.header(ui, "正在安装 StatAura");
        ui.add_space(44.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(format!("{:.0}%", self.progress * 100.0))
                    .size(44.0)
                    .strong()
                    .color(INK),
            );
            ui.add_space(12.0);
            self.progress_bar(ui, ctx);
            ui.add_space(12.0);
            ui.label(RichText::new(&self.status).size(14.0).color(MUTED));
        });
        ui.add_space(34.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("首次安装会写入 Program Files，请保持窗口打开。")
                    .size(12.5)
                    .color(MUTED),
            );
        });
    }

    fn done_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.header(ui, "安装完成");
        ui.add_space(26.0);
        let t = (ctx.input(|i| i.time) - self.started) as f32;
        ui.vertical_centered(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(78.0), Sense::hover());
            let p = ui.painter();
            p.circle_stroke(rect.center(), 34.0, Stroke::new(2.0, OK));
            let a = (t * 2.6).min(1.0);
            let c = rect.center();
            p.line_segment(
                [
                    egui::pos2(c.x - 15.0, c.y + 1.0),
                    egui::pos2(c.x - 4.0, c.y + 12.0),
                ],
                Stroke::new(
                    3.0,
                    Color32::from_rgba_unmultiplied(60, 210, 160, (a * 255.0) as u8),
                ),
            );
            p.line_segment(
                [
                    egui::pos2(c.x - 4.0, c.y + 12.0),
                    egui::pos2(c.x + 17.0, c.y - 12.0),
                ],
                Stroke::new(
                    3.0,
                    Color32::from_rgba_unmultiplied(60, 210, 160, (a * 255.0) as u8),
                ),
            );
            ui.add_space(12.0);
            ui.label(
                RichText::new("StatAura 已安装")
                    .size(24.0)
                    .strong()
                    .color(INK),
            );
            ui.add_space(6.0);
            ui.label(RichText::new(&self.dir).size(13.0).color(MUTED));
        });
        ui.add_space(18.0);
        ui.vertical_centered(|ui| {
            ui.checkbox(&mut self.launch, "完成后启动 StatAura");
        });
        footer(ui, |ui| {
            if ui
                .add_sized(
                    [102.0, 36.0],
                    egui::Button::new(RichText::new("完成").size(15.0).strong())
                        .fill(ACCENT)
                        .corner_radius(CornerRadius::same(9)),
                )
                .clicked()
            {
                if self.launch {
                    let _ =
                        std::process::Command::new(PathBuf::from(&self.dir).join("stataura.exe"))
                            .spawn();
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }

    fn uninstall_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.header(ui, "卸载 StatAura");
        ui.add_space(24.0);
        card(ui, |ui| {
            ui.label(RichText::new("将删除：").size(14.0).color(INK));
            ui.add_space(4.0);
            for line in [
                "· 安装目录中的程序与卸载器",
                "· 桌面 / 开始菜单快捷方式",
                "· .aura 文件关联与应用注册",
            ] {
                ui.label(RichText::new(line).size(13.5).color(MUTED));
            }
            ui.add_space(8.0);
            ui.label(
                RichText::new("你的工程文件和个人设置不会被删除。")
                    .size(12.5)
                    .color(MUTED),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!("目录：{}", self.dir))
                    .size(12.0)
                    .color(MUTED),
            );
        });
        footer(ui, |ui| {
            if ui
                .add_sized(
                    [100.0, 36.0],
                    egui::Button::new(RichText::new("卸载").size(15.0).strong())
                        .fill(DANGER)
                        .corner_radius(CornerRadius::same(9)),
                )
                .clicked()
            {
                self.start_uninstall(ctx);
            }
            if ui
                .add_sized(
                    [82.0, 36.0],
                    egui::Button::new(RichText::new("取消").size(15.0)).fill(Color32::TRANSPARENT),
                )
                .clicked()
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }

    fn uninstalling_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.header(ui, "正在卸载 StatAura");
        ui.add_space(46.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(format!("{:.0}%", self.progress * 100.0))
                    .size(42.0)
                    .strong()
                    .color(INK),
            );
            ui.add_space(12.0);
            self.progress_bar(ui, ctx);
            ui.add_space(12.0);
            ui.label(RichText::new(&self.status).size(14.0).color(MUTED));
        });
    }

    fn uninstalled_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.header(ui, "卸载完成");
        ui.add_space(56.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("StatAura 已卸载")
                    .size(24.0)
                    .strong()
                    .color(INK),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new("快捷方式和 .aura 关联已清理，工程文件保留。")
                    .size(13.5)
                    .color(MUTED),
            );
        });
        footer(ui, |ui| {
            if ui
                .add_sized(
                    [100.0, 36.0],
                    egui::Button::new(RichText::new("关闭").size(15.0).strong()).fill(ACCENT),
                )
                .clicked()
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }

    fn error_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.header(ui, "安装未完成");
        ui.add_space(28.0);
        Frame::NONE
            .fill(Color32::from_rgba_premultiplied(52, 22, 32, 220))
            .stroke(Stroke::new(1.0, DANGER))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::same(18))
            .show(ui, |ui| {
                ui.label(RichText::new(&self.error).size(14.0).color(INK));
            });
        footer(ui, |ui| {
            if ui
                .add_sized(
                    [96.0, 36.0],
                    egui::Button::new(RichText::new("返回").size(15.0)).fill(ACCENT),
                )
                .clicked()
            {
                self.page = if self.args.uninstall {
                    Page::Uninstall
                } else {
                    Page::Install
                };
            }
            if ui
                .add_sized(
                    [82.0, 36.0],
                    egui::Button::new(RichText::new("退出").size(15.0)).fill(Color32::TRANSPARENT),
                )
                .clicked()
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }
}

fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    Frame::NONE
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(20))
        .show(ui, |ui| {
            ui.set_width(ui.available_width().max(1.0));
            add(ui);
        });
}

fn footer(ui: &mut egui::Ui, right: impl FnOnce(&mut egui::Ui)) {
    let r = Rect::from_min_size(
        egui::pos2(ui.max_rect().left(), ui.max_rect().bottom() - 40.0),
        Vec2::new(ui.max_rect().width(), 40.0),
    );
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(r)
            .layout(Layout::right_to_left(Align::Center)),
        |ui| right(ui),
    );
}

impl eframe::App for SetupApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        BG0.to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.viewport().close_requested())
            && matches!(self.page, Page::Installing | Page::Uninstalling)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        self.poll(&ctx);
        let full = ui.max_rect();
        paint_bg(ui, full, ctx.input(|i| i.time) as f32);
        let content = Rect::from_min_max(
            full.min + Vec2::new(36.0, 28.0),
            full.max - Vec2::new(36.0, 24.0),
        );
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(content)
                .layout(Layout::top_down(Align::Min)),
            |ui| match self.page {
                Page::Install => self.install_page(ui, &ctx),
                Page::Installing => self.installing_page(ui, &ctx),
                Page::Done => self.done_page(ui, &ctx),
                Page::Uninstall => self.uninstall_page(ui, &ctx),
                Page::Uninstalling => self.uninstalling_page(ui, &ctx),
                Page::Uninstalled => self.uninstalled_page(ui, &ctx),
                Page::Error => self.error_page(ui, &ctx),
            },
        );
        if matches!(
            self.page,
            Page::Installing | Page::Uninstalling | Page::Done
        ) {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts
        .font_data
        .insert("misans".to_owned(), Arc::new(FontData::from_static(SANS)));
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "misans".to_owned());
    ctx.set_fonts(fonts);
}

fn style(ctx: &egui::Context) {
    ctx.set_theme(ThemePreference::Dark);
    ctx.all_styles_mut(|s| {
        s.text_styles
            .insert(TextStyle::Body, FontId::proportional(15.0));
        s.text_styles
            .insert(TextStyle::Button, FontId::proportional(15.0));
        s.text_styles
            .insert(TextStyle::Small, FontId::proportional(12.5));
        s.spacing.item_spacing = Vec2::new(8.0, 8.0);
        s.spacing.button_padding = Vec2::new(14.0, 7.0);
        s.visuals.panel_fill = BG0;
        s.visuals.window_fill = BG0;
        s.visuals.widgets.inactive.corner_radius = CornerRadius::same(8);
        s.visuals.widgets.hovered.corner_radius = CornerRadius::same(8);
        s.visuals.widgets.active.corner_radius = CornerRadius::same(8);
        s.visuals.selection.bg_fill = ACCENT;
    });
}

fn load_logo(ctx: &egui::Context) -> egui::TextureHandle {
    let img = image::load_from_memory(LOGO)
        .expect("内置图标损坏")
        .to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    ctx.load_texture("setup-logo", color, egui::TextureOptions::LINEAR)
}

fn paint_bg(ui: &egui::Ui, rect: Rect, t: f32) {
    let p = ui.painter();
    p.rect_filled(rect, 0, BG0);
    let strips = 24;
    for i in 0..strips {
        let f = i as f32 / strips as f32;
        let y0 = rect.top() + rect.height() * f;
        let y1 = rect.top() + rect.height() * (i + 1) as f32 / strips as f32 + 1.0;
        let c = lerp_color(BG0, BG1, f);
        p.rect_filled(
            Rect::from_min_max(egui::pos2(rect.left(), y0), egui::pos2(rect.right(), y1)),
            0,
            c,
        );
    }
    for i in 0..6 {
        let phase = i as f32 * 1.7;
        let x = rect.left() + (rect.width() * (0.5 + 0.42 * (t * 0.13 + phase).sin()));
        let y = rect.top() + rect.height() * (0.44 + 0.28 * (t * 0.11 + phase * 1.3).cos());
        let r = 90.0 + 28.0 * (t * 0.2 + phase).sin();
        p.circle_filled(
            egui::pos2(x, y),
            r,
            Color32::from_rgba_unmultiplied(100, 120, 255, 13),
        );
    }
    // 顶部一条极淡的 accent 光带，给固定布局一点呼吸。
    let glow = Rect::from_min_max(
        egui::pos2(rect.left(), rect.top()),
        egui::pos2(rect.right(), rect.top() + 2.0),
    );
    p.rect_filled(glow, 0, Color32::from_rgba_unmultiplied(110, 132, 255, 150));
}

fn lerp_color(a: Color32, b: Color32, f: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f) as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}
