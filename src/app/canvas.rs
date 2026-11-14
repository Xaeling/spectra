//! 图表画布：后台渲染 + GPU 纹理 + hover 读数。

use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use egui::{Align, Color32, FontId, Layout, Rect, RichText, Stroke, Ui, Vec2};

use crate::app::theme::{metric, radius, text};
use crate::app::widgets::{self};
use crate::app::App;
use crate::core::graph::{ChartKind, GraphSpec};
use crate::core::model::DataTable;
use crate::plot::chart::TitleSlot;
use crate::plot::raster::rasterize;
use crate::plot::text::FontBook;
use crate::plot::{Hit, Render};

pub const W_PT: f32 = 468.0;
pub const H_PT: f32 = 318.0;

/// 渲染任务：单图或整张拼版页，共用同一个后台渲染线程。
enum JobTask {
    Graph {
        spec: GraphSpec,
        table: DataTable,
    },
    Layout {
        page: crate::core::model::LayoutPage,
        panels: Vec<(GraphSpec, DataTable)>,
    },
}

struct Job {
    id: u64,
    task: JobTask,
    scale: f32,
}

struct Done {
    id: u64,
    render: Render,
    rgba: Vec<u8>,
    w: u32,
    h: u32,
    ms: f64,
}

pub struct ChartPanel {
    pub last_ms: f64,
    pub rendered_rev: u64,
    pending_rev: u64,
    pending_scale: f32,
    in_flight: Option<u64>,
    next_id: u64,
    job_tx: Sender<Job>,
    done_rx: Receiver<Done>,
    pub tex: Option<egui::TextureHandle>,
    pub tex_size: (u32, u32),
    pub hits: Vec<Hit>,
    pub plot: crate::plot::scene::Rect,
    pub display: Rect,
    pub axes: Option<(crate::plot::axis::Axis, crate::plot::axis::Axis)>,
    pub hover: Option<usize>,
    pub error: Option<String>,
    /// 缩放倍率（1.0 = 适应窗口）。
    pub zoom: f32,
    /// 平移偏移（屏幕像素）。
    pub pan: egui::Vec2,
    /// 标题外框（场景像素），点击编辑用。
    pub title_boxes: Vec<(TitleSlot, crate::plot::scene::Rect)>,
    /// 就地编辑中的标题：(槽位, 缓冲文本, 锚点外框-场景px)。
    ///
    /// 编辑期渲染会把该标题置空（防双重绘制），所以外框必须自己记着。
    pub editing: Option<(TitleSlot, String, crate::plot::scene::Rect)>,
    /// 编辑框刚打开、还没请求过焦点（焦点只能请求一次，每帧抢会打崩输入法）。
    pub edit_focus_pending: bool,
}

/// 以鼠标为中心缩放时的平移补偿：缩放前后**鼠标下的画面点不动**。
///
/// 抽成纯函数是为了能直接测这个不变量（手算坐标恒等式），不靠肉眼。
pub(crate) fn zoom_around(
    pan: egui::Vec2,
    zoom: f32,
    factor: f32,
    pointer: egui::Pos2,
    canvas_center: egui::Pos2,
) -> (egui::Vec2, f32) {
    let z_new = (zoom * factor).clamp(0.25, 8.0);
    let c_old = canvas_center + pan;
    let d = pointer - c_old;
    let pan_new = pointer - d * (z_new / zoom.max(1e-6)) - canvas_center;
    (pan_new, z_new)
}

/// 场景像素矩形 → 屏幕矩形（贴图按 img 拉伸，1 场景px = tw 分之一宽）。
fn scene_to_screen(img: Rect, tw: u32, th: u32, r: crate::plot::scene::Rect) -> Rect {
    let sx = img.width() / (tw as f32).max(1.0);
    let sy = img.height() / (th as f32).max(1.0);
    Rect::from_min_max(
        egui::pos2(img.left() + r.x * sx, img.top() + r.y * sy),
        egui::pos2(img.left() + (r.x + r.w) * sx, img.top() + (r.y + r.h) * sy),
    )
}

impl ChartPanel {
    pub fn new(fonts: Arc<FontBook>) -> Self {
        let (job_tx, job_rx) = crossbeam_channel::unbounded::<Job>();
        let (done_tx, done_rx) = crossbeam_channel::unbounded::<Done>();
        let f = fonts.clone();
        std::thread::Builder::new()
            .name("stataura-render".into())
            .spawn(move || {
                while let Ok(mut job) = job_rx.recv() {
                    // 只渲染最新一次请求，避免快速拖动时堆积。
                    while let Ok(newer) = job_rx.try_recv() {
                        job = newer;
                    }
                    let started = Instant::now();
                    let render = match &job.task {
                        JobTask::Graph { spec, table } => {
                            crate::plot::render(spec, table, W_PT, H_PT, job.scale, &f)
                        }
                        JobTask::Layout { page, panels } => {
                            let refs: Vec<(&GraphSpec, &DataTable)> =
                                panels.iter().map(|(sp, t)| (sp, t)).collect();
                            crate::plot::layout::render_layout(
                                page, &refs, W_PT, H_PT, job.scale, &f,
                            )
                        }
                    };
                    let (w, h, rgba) = rasterize(&render.scene);
                    let _ = done_tx.send(Done {
                        id: job.id,
                        render,
                        rgba,
                        w,
                        h,
                        ms: started.elapsed().as_secs_f64() * 1000.0,
                    });
                }
            })
            .ok();
        Self {
            last_ms: 0.0,
            rendered_rev: 0,
            pending_rev: 0,
            pending_scale: 0.0,
            in_flight: None,
            next_id: 1,
            job_tx,
            done_rx,
            tex: None,
            tex_size: (0, 0),
            hits: Vec::new(),
            plot: crate::plot::scene::Rect::default(),
            display: Rect::NOTHING,
            axes: None,
            hover: None,
            error: None,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            title_boxes: Vec::new(),
            editing: None,
            edit_focus_pending: false,
        }
    }

    pub fn reset(&mut self) {
        self.tex = None;
        self.rendered_rev = 0;
        self.pending_rev = 0;
        self.pending_scale = 0.0;
        self.hits.clear();
        self.display = Rect::NOTHING;
        self.error = None;
        self.editing = None;
    }

    fn request(&mut self, rev: u64, spec: &GraphSpec, table: &DataTable, scale: f32) {
        if rev == self.rendered_rev && (scale - self.pending_scale).abs() < 0.001 {
            return;
        }
        if rev == self.pending_rev && (scale - self.pending_scale).abs() < 0.001 {
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        let job = Job {
            id,
            task: JobTask::Graph {
                spec: spec.clone(),
                table: table.clone(),
            },
            scale,
        };
        if self.job_tx.send(job).is_err() {
            self.error = Some("渲染线程已退出".into());
        }
        self.pending_rev = rev;
        self.pending_scale = scale;
        self.in_flight = Some(id);
    }

    /// 请求整张拼版页的渲染（与单图同线程，latest-wins）。
    fn request_layout(
        &mut self,
        rev: u64,
        page: &crate::core::model::LayoutPage,
        panels: Vec<(GraphSpec, DataTable)>,
        scale: f32,
    ) {
        if rev == self.rendered_rev && (scale - self.pending_scale).abs() < 0.001 {
            return;
        }
        if rev == self.pending_rev && (scale - self.pending_scale).abs() < 0.001 {
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        let job = Job {
            id,
            task: JobTask::Layout {
                page: page.clone(),
                panels,
            },
            scale,
        };
        if self.job_tx.send(job).is_err() {
            self.error = Some("渲染线程已退出".into());
        }
        self.pending_rev = rev;
        self.pending_scale = scale;
        self.in_flight = Some(id);
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(done) = self.done_rx.try_recv() {
            if Some(done.id) != self.in_flight {
                continue;
            }
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [done.w as usize, done.h as usize],
                &done.rgba,
            );
            self.tex = Some(ctx.load_texture(
                format!("chart-{}", done.id),
                image,
                egui::TextureOptions::LINEAR,
            ));
            self.tex_size = (done.w, done.h);
            self.hits = done.render.hits;
            self.title_boxes = done.render.title_boxes;
            self.plot = done.render.plot;
            self.axes = Some((done.render.x, done.render.y));
            self.last_ms = done.ms;
            self.rendered_rev = self.pending_rev;
            self.in_flight = None;
            self.error = None;
        }
        if self.in_flight.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
}

impl App {
    pub fn chart_view(&mut self, ui: &mut Ui, rect: Rect, id: u64) {
        let pal = self.pal;
        ui.painter().rect_filled(rect, 0, pal.bg);
        let Some(g) = self.project.graph(id) else {
            return;
        };
        let name = g.name.clone();
        let table_id = g.table;
        let spec = g.spec.clone();
        let kind = spec.kind;
        let (table_name, table_kind) = self
            .project
            .table(table_id)
            .map(|t| (t.name.clone(), Some(t.kind)))
            .unwrap_or_default();

        // ---- 顶部工具条：标题左对齐内容边，按钮右对齐内容边 ----
        let pad = metric::CONTENT_PAD;
        let head = Rect::from_min_size(
            egui::pos2(rect.left() + pad, rect.top() + 12.0),
            Vec2::new(rect.width() - pad * 2.0, 44.0),
        );
        ui.painter().text(
            egui::pos2(head.left(), head.center().y - 10.0),
            egui::Align2::LEFT_CENTER,
            &name,
            FontId::proportional(15.5),
            pal.ink,
        );
        ui.painter().text(
            egui::pos2(head.left(), head.center().y + 9.0),
            egui::Align2::LEFT_CENTER,
            format!("{}  ·  数据表：{}", kind.label(), table_name),
            FontId::proportional(text::META),
            pal.faint,
        );

        crate::app::area_public(ui, head, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::Btn::new("导出图表")
                    .fill(pal.accent)
                    .fg(Color32::WHITE)
                    .strong(true)
                    .radius(radius::CONTROL)
                    .height(30.0)
                    .show(ui)
                    .clicked()
                {
                    self.open_export();
                }
                ui.add_space(6.0);
                if widgets::Btn::new("新建拼版")
                    .fill(pal.panel)
                    .fg(pal.ink)
                    .stroke(Stroke::new(1.0, pal.border))
                    .radius(radius::CONTROL)
                    .height(30.0)
                    .show(ui)
                    .clicked()
                {
                    self.add_layout_page();
                }
                ui.add_space(6.0);
                if widgets::Btn::new("显著性标注")
                    .fill(pal.panel)
                    .fg(pal.ink)
                    .stroke(Stroke::new(1.0, pal.border))
                    .radius(radius::CONTROL)
                    .height(30.0)
                    .show(ui)
                    .clicked()
                {
                    self.auto_significance();
                }
                ui.add_space(6.0);
                let mut switched: Option<ChartKind> = None;
                // 只列出当前表格类型支持的图型；按族分成子菜单，空族不出现。
                let groups = table_kind.map(ChartKind::groups_for).unwrap_or_default();
                ui.menu_button(
                    RichText::new(format!("图型 · {}", kind.label())).size(text::BODY),
                    |ui| {
                        ui.set_min_width(160.0);
                        for (fam, kinds) in &groups {
                            ui.menu_button(*fam, |ui| {
                                ui.set_min_width(150.0);
                                for k in kinds {
                                    if ui.selectable_label(kind == *k, k.label()).clicked() {
                                        switched = Some(*k);
                                        ui.close();
                                    }
                                }
                            });
                        }
                    },
                );
                // 缩放/平移非默认时给指示与「适应」复位。
                if (self.chart.zoom - 1.0).abs() > 0.01 || self.chart.pan != egui::Vec2::ZERO {
                    if ui.button(RichText::new("适应").size(text::BODY)).clicked() {
                        self.chart.zoom = 1.0;
                        self.chart.pan = egui::Vec2::ZERO;
                    }
                    ui.label(
                        RichText::new(format!("缩放 {:.0}%", self.chart.zoom * 100.0))
                            .size(text::META)
                            .color(pal.faint),
                    );
                }
                if let Some(k) = switched {
                    let multi = self
                        .project
                        .table(table_id)
                        .map(|t| t.analysis_groups().len() > 1)
                        .unwrap_or(false);
                    if let Some(s) = self.project.graph_mut(id).map(|g| &mut g.spec) {
                        s.kind = k;
                        if multi
                            && matches!(k, ChartKind::Histogram | ChartKind::Ecdf | ChartKind::Qq)
                        {
                            s.show_legend = true;
                        }
                    }
                    self.bump_chart();
                }
            });
        });

        // 画布
        let canvas = Rect::from_min_max(
            egui::pos2(rect.left() + pad, head.bottom() + 12.0),
            egui::pos2(rect.right() - pad, rect.bottom() - 18.0),
        );
        let avail = canvas.shrink(14.0);
        let scale = (avail.width() / W_PT).min(avail.height() / H_PT).max(0.2);
        let fit = Vec2::new(W_PT * scale, H_PT * scale);

        // 缩放/平移交互：滚轮平移、Ctrl+滚轮缩放、拖拽平移、双击（标题编辑/适应）
        let (mut img, dbl) = self.view_input(ui, canvas, fit);
        if let Some(p) = dbl {
            if !self.begin_title_edit(p, img, id) {
                self.chart.zoom = 1.0;
                self.chart.pan = egui::Vec2::ZERO;
                img = Rect::from_center_size(canvas.center(), fit);
            }
        }

        let rev = self.chart_rev;
        // 清晰度：贴图分辨率 = 显示分辨率 × 设备像素比（DPR），做到设备像素 1:1
        // —— 之前漏乘 DPR，高分屏上贴图比显示区域小，整个发虚。
        // 上限 10 px/pt（468pt 宽 ≈ 4680px，4K 级），再高纹理开销不值。
        let dpr = ui.ctx().pixels_per_point();
        let rs = (scale * self.chart.zoom * dpr).clamp(0.3, 10.0);
        if let Some(t) = self.project.table(table_id) {
            // 编辑中的标题置成空格：布局留白不变（不会引起整图回流），
            // 但不画出字形——真正显示的是就地编辑框里的活文本，避免双重绘制。
            let mut spec_r = spec.clone();
            if let Some((slot, _, _)) = &self.chart.editing {
                match slot {
                    TitleSlot::Main => spec_r.title = " ".into(),
                    TitleSlot::X => spec_r.x_title = " ".into(),
                    TitleSlot::Y => spec_r.y_title = " ".into(),
                }
            }
            self.chart.request(rev, &spec_r, t, rs);
        }
        if let Some(tex) = self.chart.tex.clone() {
            let r = radius::CARD;
            // 图片裁剪在画布视口内：拖动/缩放时不得压到侧栏与检查器上
            // （图片在自己那一层，界面上其他面板永远在它上面/之外）。
            let cp = ui.painter().with_clip_rect(canvas);
            widgets::soft_shadow(&cp, img, r, pal.shadow);
            widgets::surface(&cp, img, r, Color32::WHITE, Stroke::NONE);
            cp.image(
                tex.id(),
                img,
                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            // 位图只能方角绘制：把四角「啃」成圆角，再补一圈描边。
            widgets::punch_corners(&cp, img, r, pal.bg);
            widgets::surface_stroke(&cp, img, r, Stroke::new(1.0, pal.border));
            self.chart.display = img;
            self.hover_tooltip(ui, img);
            self.title_hover_cursor(ui, img);
            self.title_editor(ui, img, id, scale);
        } else {
            ui.painter().text(
                canvas.center(),
                egui::Align2::CENTER_CENTER,
                if self.chart.error.is_some() {
                    "渲染失败"
                } else {
                    "正在渲染…"
                },
                FontId::proportional(13.0),
                pal.muted,
            );
        }
    }

    /// 多图拼版页画布：整页渲染（场景合成）后整体贴图，与单图同一套
    /// 后台渲染 / hover 命中管线。
    pub fn layout_view(&mut self, ui: &mut Ui, rect: Rect, id: u64) {
        let pal = self.pal;
        ui.painter().rect_filled(rect, 0, pal.bg);
        // 克隆出来：后面要对 self.chart / self.project 做可变操作，
        // 不能让 page 一直借着 self.project。
        let Some(page) = self.project.layout(id).cloned() else {
            return;
        };
        let name = page.name.clone();
        let cols = page.cols;
        // 收集面板：引用的图 + 各自的数据表（缺失的跳过并在副标题提示）。
        let mut panels: Vec<(GraphSpec, DataTable)> = Vec::new();
        let mut missing = 0usize;
        for gid in &page.panels {
            match self.project.graph(*gid) {
                Some(g) => match self.project.table(g.table) {
                    Some(t) => panels.push((g.spec.clone(), t.clone())),
                    None => missing += 1,
                },
                None => missing += 1,
            }
        }
        let n_ok = panels.len();

        // ---- 顶部工具条（与 chart_view 同构）----
        let pad = metric::CONTENT_PAD;
        let head = Rect::from_min_size(
            egui::pos2(rect.left() + pad, rect.top() + 12.0),
            Vec2::new(rect.width() - pad * 2.0, 44.0),
        );
        ui.painter().text(
            egui::pos2(head.left(), head.center().y - 10.0),
            egui::Align2::LEFT_CENTER,
            &name,
            FontId::proportional(15.5),
            pal.ink,
        );
        let rows = n_ok.div_ceil(cols.max(1)).max(1);
        let mut sub = format!("拼版  ·  {} 面板  ·  {}×{} 网格", n_ok, rows, cols);
        if missing > 0 {
            sub.push_str(&format!("  ·  {missing} 个面板引用丢失"));
        }
        ui.painter().text(
            egui::pos2(head.left(), head.center().y + 9.0),
            egui::Align2::LEFT_CENTER,
            sub,
            FontId::proportional(text::META),
            pal.muted,
        );

        // ---- 画布 ----
        let canvas = Rect::from_min_max(
            egui::pos2(rect.left() + pad, head.bottom() + 12.0),
            egui::pos2(rect.right() - pad, rect.bottom() - 18.0),
        );
        let avail = canvas.shrink(14.0);
        // 拼版页纵横比由面板数决定，按整页尺寸缩放（不是单图的 468×318）。
        let (w_pt, h_pt) = crate::plot::layout::layout_size(
            &page,
            crate::app::canvas::W_PT,
            crate::app::canvas::H_PT,
        );
        let scale = (avail.width() / w_pt).min(avail.height() / h_pt).max(0.2);
        let fit = Vec2::new(w_pt * scale, h_pt * scale);
        // 与单图共用缩放/平移；拼版不做标题编辑，双击 = 适应窗口。
        let (mut img, dbl) = self.view_input(ui, canvas, fit);
        if dbl.is_some() {
            self.chart.zoom = 1.0;
            self.chart.pan = egui::Vec2::ZERO;
            img = Rect::from_center_size(canvas.center(), fit);
        }

        if panels.is_empty() {
            ui.painter().text(
                canvas.center(),
                egui::Align2::CENTER_CENTER,
                "这张拼版页还没有可用面板——在右侧检查器里添加图",
                FontId::proportional(13.0),
                pal.muted,
            );
            return;
        }

        let rev = self.chart_rev;
        // 设备像素 1:1（同 chart_view 的清晰度口径）。
        let dpr = ui.ctx().pixels_per_point();
        let rs = (scale * self.chart.zoom * dpr).clamp(0.3, 10.0);
        self.chart.request_layout(rev, &page, panels, rs);
        if let Some(tex) = self.chart.tex.clone() {
            let r = radius::CARD;
            // 同 chart_view：裁剪在画布视口内，不压侧栏/检查器。
            let cp = ui.painter().with_clip_rect(canvas);
            widgets::soft_shadow(&cp, img, r, pal.shadow);
            widgets::surface(&cp, img, r, Color32::WHITE, Stroke::NONE);
            cp.image(
                tex.id(),
                img,
                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            widgets::punch_corners(&cp, img, r, pal.bg);
            widgets::surface_stroke(&cp, img, r, Stroke::new(1.0, pal.border));
            self.chart.display = img;
            self.hover_tooltip(ui, img);
        } else {
            ui.painter().text(
                canvas.center(),
                egui::Align2::CENTER_CENTER,
                if self.chart.error.is_some() {
                    "渲染失败"
                } else {
                    "正在渲染…"
                },
                FontId::proportional(13.0),
                pal.muted,
            );
        }
    }

    /// 画布缩放/平移交互：滚轮平移、Ctrl+滚轮（或触控板捏合）缩放、拖拽平移。
    /// 返回 (当前图框, 本次双击位置)。双击语义由调用方决定（编辑标题/适应窗口）。
    fn view_input(&mut self, ui: &mut Ui, canvas: Rect, fit: Vec2) -> (Rect, Option<egui::Pos2>) {
        let rect_at =
            |pan: egui::Vec2, zoom: f32| Rect::from_center_size(canvas.center() + pan, fit * zoom);
        let hover = ui.ctx().pointer_hover_pos();
        let over = hover.map(|p| canvas.contains(p)).unwrap_or(false);
        if over {
            let (scroll, zd) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            if (zd - 1.0).abs() > 1e-4 {
                let ptr = hover.unwrap_or(canvas.center());
                let (pan, z) =
                    zoom_around(self.chart.pan, self.chart.zoom, zd, ptr, canvas.center());
                self.chart.pan = pan;
                self.chart.zoom = z;
            } else if scroll != egui::Vec2::ZERO {
                self.chart.pan += scroll;
            }
        }
        let mut img = rect_at(self.chart.pan, self.chart.zoom);
        // 拖拽平移 / 双击：注册在画布上（不影响 hover 读数）。
        let resp = ui.interact(
            canvas,
            ui.scope_id().with("chart_canvas_view"),
            egui::Sense::click_and_drag(),
        );
        if resp.dragged() {
            self.chart.pan += resp.drag_delta();
            img = rect_at(self.chart.pan, self.chart.zoom);
        }
        let dbl = if resp.double_clicked() {
            resp.interact_pointer_pos()
        } else {
            None
        };
        (img, dbl)
    }

    /// 鼠标悬在标题上时显示文本光标（暗示可双击编辑）。
    fn title_hover_cursor(&mut self, ui: &mut Ui, img: Rect) {
        if self.chart.editing.is_some() {
            return;
        }
        let Some(p) = ui.ctx().pointer_hover_pos() else {
            return;
        };
        if !img.contains(p) {
            return;
        }
        let (tw, th) = self.chart.tex_size;
        if tw == 0 {
            return;
        }
        let tp = egui::pos2(
            (p.x - img.left()) * tw as f32 / img.width(),
            (p.y - img.top()) * th as f32 / img.height(),
        );
        for (_, r) in &self.chart.title_boxes {
            let m = 3.0;
            if tp.x >= r.x - m && tp.x <= r.x + r.w + m && tp.y >= r.y - m && tp.y <= r.y + r.h + m
            {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                return;
            }
        }
    }

    /// 双击落在标题上则进入就地编辑；返回是否命中。
    fn begin_title_edit(&mut self, p: egui::Pos2, img: Rect, graph_id: u64) -> bool {
        let (tw, th) = self.chart.tex_size;
        if tw == 0 || th == 0 {
            return false;
        }
        let tp = egui::pos2(
            (p.x - img.left()) * tw as f32 / img.width(),
            (p.y - img.top()) * th as f32 / img.height(),
        );
        let hit = self.chart.title_boxes.iter().find_map(|(slot, r)| {
            let m = 3.0;
            if tp.x >= r.x - m && tp.x <= r.x + r.w + m && tp.y >= r.y - m && tp.y <= r.y + r.h + m
            {
                Some((*slot, *r))
            } else {
                None
            }
        });
        let Some((slot, anchor)) = hit else {
            return false;
        };
        let cur = self
            .project
            .graph(graph_id)
            .map(|g| match slot {
                TitleSlot::Main => g.spec.title.clone(),
                TitleSlot::X => g.spec.x_title.clone(),
                TitleSlot::Y => g.spec.y_title.clone(),
            })
            .unwrap_or_default();
        self.chart.editing = Some((slot, cur, anchor));
        self.chart.edit_focus_pending = true;
        // 进入编辑：立刻重渲染（编辑期该标题置空），让位给就地编辑框。
        self.bump_chart();
        true
    }

    /// 就地编辑框——**长得和图上的字一模一样**，只是多了根光标。
    ///
    /// 做法：编辑期渲染把该标题置成空格（见 chart_view 的抑制逻辑），真正显示的
    /// 文字就是这个无框 TextEdit——MiSans 同字体、字号按「图上字号 × 缩放」换算、
    /// 颜色同墨色，位置贴着原标题的锚点（居中槽位按中心对称扩展）。
    ///
    /// 输入法要点（踩过坑）：
    /// - **焦点只在打开时请求一次**：每帧 request_focus 会把输入法打崩（闪烁、
    ///   打不进字）；
    /// - **候选框弹出会短暂夺走窗口焦点**：这不算「点别处」，不能当成失焦提交，
    ///   否则编辑框一开输入法就自己关掉。窗口失焦期间保持编辑，回来还焦点。
    fn title_editor(&mut self, ui: &mut Ui, img: Rect, graph_id: u64, fit_scale: f32) {
        let Some((slot, _, anchor)) = self.chart.editing.clone() else {
            return;
        };
        // 字号：屏幕上 = 图上字号 × fit × zoom（与设备像素比无关）。
        let (font_pt, ink) = self
            .project
            .graph(graph_id)
            .map(|g| {
                let t = crate::plot::theme::Theme::for_id(g.spec.theme);
                let pt = match slot {
                    TitleSlot::Main => t.font_title,
                    _ => t.font_axis_title,
                };
                (pt, t.ink)
            })
            .unwrap_or((13.0, crate::core::color::Rgba::BLACK));
        let sz = (font_pt * fit_scale * self.chart.zoom).max(8.0);
        let font = FontId::proportional(sz);
        let ink32 = Color32::from_rgba_unmultiplied(ink.r, ink.g, ink.b, 255);

        let (tw, th) = self.chart.tex_size;
        let scr = scene_to_screen(img, tw, th, anchor);
        let mut value = self
            .chart
            .editing
            .as_ref()
            .map(|(_, v, _)| v.clone())
            .unwrap_or_default();

        // 边界：编辑框（含横过来的 Y 轴标题）不许超出图表卡片。
        let bounds = img.shrink(2.0);
        let text_w = ui
            .painter()
            .layout_no_wrap(value.clone(), font.clone(), ink32)
            .size()
            .x;
        let edit_w = ((text_w + sz * 1.2).max(sz * 3.0)).min(bounds.width());
        let line_h = sz * 1.7;
        // 主/X 标题顶对齐；Y 标题是竖排，编辑态横排，压在竖排字的几何中心上。
        let mut pos = match slot {
            TitleSlot::Y => {
                egui::pos2(scr.center().x - edit_w / 2.0, scr.center().y - line_h / 2.0)
            }
            _ => egui::pos2(scr.center().x - edit_w / 2.0, scr.top() - sz * 0.15),
        };
        pos.x = pos
            .x
            .clamp(bounds.left(), (bounds.right() - edit_w).max(bounds.left()));
        pos.y = pos
            .y
            .clamp(bounds.top(), (bounds.bottom() - line_h).max(bounds.top()));

        // 掩护旧字：抑制渲染是异步的，落地前底图上还印着原标题（Y 轴是竖排的，
        // 会和横排编辑文字叠花）。用卡片底色（恒白）把「旧字 + 新编辑框」整块垫底。
        let patch = Rect::from_two_pos(
            egui::pos2(scr.left().min(pos.x) - 5.0, scr.top().min(pos.y) - 5.0),
            egui::pos2(
                scr.right().max(pos.x + edit_w) + 5.0,
                scr.bottom().max(pos.y + line_h) + 5.0,
            ),
        )
        .intersect(bounds);
        ui.painter().rect_filled(patch, 0.0, Color32::WHITE);

        let mut commit: Option<String> = None;
        let mut cancel = false;
        let window_focused = ui.input(|i| i.focused);
        egui::Area::new(egui::Id::unique("stataura_title_edit"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut value)
                        .font(font)
                        .text_color(ink32)
                        .frame(egui::Frame::NONE)
                        .background_color(Color32::TRANSPARENT)
                        .desired_width(edit_w)
                        .margin(egui::Margin::same(0)),
                );
                // 焦点：只在打开时请求一次；窗口被输入法候选框夺走后，
                // 回来时把焦点还给编辑框（不能每帧抢，见方法注释）。
                if self.chart.edit_focus_pending {
                    resp.request_focus();
                    self.chart.edit_focus_pending = false;
                } else if !window_focused {
                    resp.request_focus();
                }
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                if esc {
                    cancel = true;
                } else if window_focused && (enter || resp.lost_focus()) {
                    // 点别处 = 失焦 = 自动完成更改；窗口失焦（输入法弹窗）不算。
                    commit = Some(value.clone());
                }
            });

        // 每帧把缓冲写回状态——否则下一帧从旧缓冲重新克隆，打的字全丢。
        if let Some((_, v, _)) = self.chart.editing.as_mut() {
            *v = value.clone();
        }
        if cancel {
            self.chart.editing = None;
            self.bump_chart(); // 恢复原标题的绘制
            return;
        }
        if let Some(t) = commit {
            self.snapshot();
            if let Some(g) = self.project.graph_mut(graph_id) {
                match slot {
                    TitleSlot::Main => g.spec.title = t,
                    TitleSlot::X => g.spec.x_title = t,
                    TitleSlot::Y => g.spec.y_title = t,
                }
            }
            self.chart.editing = None;
            self.bump_chart(); // 打完竖回去：重渲染画回旋转标题
            self.dirty = true;
        }
    }

    fn hover_tooltip(&mut self, ui: &mut Ui, img: Rect) {
        let Some(p) = ui.ctx().pointer_hover_pos() else {
            self.chart.hover = None;
            return;
        };
        if !img.contains(p) {
            self.chart.hover = None;
            return;
        }
        let (tw, th) = self.chart.tex_size;
        if tw == 0 {
            return;
        }
        let k = tw as f32 / img.width();
        let tp = egui::pos2((p.x - img.left()) * k, (p.y - img.top()) * k);
        let mut best: Option<(usize, f32)> = None;
        for (i, h) in self.chart.hits.iter().enumerate() {
            let d = ((h.at.x - tp.x).powi(2) + (h.at.y - tp.y).powi(2)).sqrt();
            if d < 12.0 * k && best.map(|(_, bd)| d < bd).unwrap_or(true) {
                best = Some((i, d));
            }
        }
        self.chart.hover = best.map(|(i, _)| i);
        let Some(i) = self.chart.hover else {
            return;
        };
        let h = &self.chart.hits[i];
        let pal = self.pal;
        let pad = 11.0;
        let title = h.title.clone();
        let lines: Vec<String> = h.lines.clone();

        // 尺寸测量
        let title_font = FontId::proportional(11.5);
        let line_font = FontId::proportional(10.5);
        let title_gal = ui
            .painter()
            .layout_no_wrap(title.clone(), title_font, pal.ink);
        let mut w = title_gal.size().x;
        let mut hgt = title_gal.size().y + 4.0;
        let mut galleys = Vec::new();
        for l in &lines {
            let g = ui
                .painter()
                .layout_no_wrap(l.clone(), line_font.clone(), pal.muted);
            w = w.max(g.size().x);
            hgt += g.size().y + 3.0;
            galleys.push((l.clone(), g));
        }
        w += pad * 2.0;
        hgt += pad * 2.0;
        let mut pos = p + Vec2::new(16.0, 14.0);
        if pos.x + w > img.right() + 5.0 {
            pos.x = p.x - w - 14.0;
        }
        if pos.y + hgt > img.bottom() + 5.0 {
            pos.y = p.y - hgt - 12.0;
        }
        let r = Rect::from_min_size(pos, Vec2::new(w, hgt));
        let rad = radius::CONTROL;
        widgets::soft_shadow(ui.painter(), r, rad, pal.shadow);
        widgets::surface(
            ui.painter(),
            r,
            rad,
            if pal.dark {
                pal.panel_alt
            } else {
                Color32::from_rgba_unmultiplied(255, 255, 255, 250)
            },
            Stroke::new(1.0, pal.border),
        );
        ui.painter().galley(
            egui::pos2(r.left() + pad, r.top() + pad),
            title_gal.clone(),
            pal.ink,
        );
        let mut y = r.top() + pad + title_gal.size().y + 4.0;
        for (_, g) in galleys {
            let gh = g.size().y;
            ui.painter()
                .galley(egui::pos2(r.left() + pad, y), g, pal.muted);
            y += gh + 3.0;
        }
        // 十字线
        ui.painter().circle_stroke(
            egui::pos2(img.left() + h.at.x / k, img.top() + h.at.y / k),
            4.5,
            Stroke::new(1.2, pal.accent),
        );
        let _ = th;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 缩放补偿的核心不变量：以鼠标为中心缩放时，**鼠标下的画面点不动**。
    #[test]
    fn 缩放以鼠标为中心画面点不动() {
        let center = egui::pos2(400.0, 300.0);
        let fit = Vec2::new(468.0, 318.0);
        let pan = egui::Vec2::new(10.0, -20.0);
        let zoom = 1.0_f32;
        let ptr = egui::pos2(500.0, 350.0);
        let img0 = Rect::from_center_size(center + pan, fit * zoom);
        let u0 = (
            (ptr.x - img0.left()) / img0.width(),
            (ptr.y - img0.top()) / img0.height(),
        );
        let (pan2, z2) = zoom_around(pan, zoom, 2.0, ptr, center);
        let img1 = Rect::from_center_size(center + pan2, fit * z2);
        let u1 = (
            (ptr.x - img1.left()) / img1.width(),
            (ptr.y - img1.top()) / img1.height(),
        );
        assert!(
            (u0.0 - u1.0).abs() < 1e-4 && (u0.1 - u1.1).abs() < 1e-4,
            "缩放前后鼠标下的相对位置应一致：{u0:?} vs {u1:?}"
        );
    }

    /// 连续放大再缩小应回到原位（往返一致）。
    #[test]
    fn 缩放往返保持平移() {
        let center = egui::pos2(300.0, 200.0);
        let ptr = egui::pos2(360.0, 240.0);
        let (p1, z1) = zoom_around(egui::Vec2::new(5.0, -8.0), 1.0, 2.0, ptr, center);
        let (p2, z2) = zoom_around(p1, z1, 0.5, ptr, center);
        assert!((z2 - 1.0).abs() < 1e-4, "往返后 zoom={z2}");
        assert!(
            (p2 - egui::Vec2::new(5.0, -8.0)).length() < 1e-3,
            "往返后 pan={p2:?}"
        );
    }

    #[test]
    fn 缩放倍率有上下界() {
        let center = egui::pos2(0.0, 0.0);
        let p = egui::pos2(10.0, 10.0);
        let (_, z) = zoom_around(egui::Vec2::ZERO, 4.0, 10.0, p, center);
        assert!(z <= 8.0, "上限 8x，实际 {z}");
        let (_, z) = zoom_around(egui::Vec2::ZERO, 0.5, 0.01, p, center);
        assert!(z >= 0.25, "下限 0.25x，实际 {z}");
    }
}
