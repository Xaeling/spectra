//! 桌面应用壳：状态机、布局、命令与对话框。

pub mod canvas;
pub mod clipboard;
pub mod dialogs;
pub mod form;
pub mod grid;
pub mod i18n;
pub mod icons;
pub mod inspector;
pub mod modals;
pub mod results;
pub mod settings;
pub mod settings_ui;
pub mod sidebar;
pub mod statusbar;
pub mod theme;
pub mod topbar;
pub mod widgets;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use eframe::CreationContext;
use egui::{Color32, Rect, Vec2};

use crate::core::analysis;
use crate::core::graph::GraphSpec;
use crate::core::model::{Analysis, AnalysisKind, AnalysisOptions, Project, TableKind};
use crate::core::sample::sample_project;
use crate::io::{self, ExportFormat};
use crate::plot::text::FontBook;

use self::canvas::ChartPanel;
use self::grid::GridState;
use self::theme::Palette;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sheet {
    Table(u64),
    Analysis(u64),
    Graph(u64),
    /// 多图拼版页。
    Layout(u64),
}

impl Sheet {
    pub fn id(self) -> u64 {
        match self {
            Self::Table(id) | Self::Analysis(id) | Self::Graph(id) | Self::Layout(id) => id,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub until: f64,
}

/// 侧栏行内改名：直接覆盖标题文字，不再弹窗。
pub struct SidebarEdit {
    pub sheet: Sheet,
    pub buf: String,
    pub just_started: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Confirm {
    New,
    Open,
    Close,
}

pub struct ExportState {
    pub open: bool,
    pub format: ExportFormat,
    pub width_in: f32,
    pub dpi: u32,
    pub path: String,
    pub status: Option<String>,
}

impl Default for ExportState {
    fn default() -> Self {
        let s = settings::get();
        Self {
            open: false,
            format: match s.export_format {
                1 => ExportFormat::Jpeg,
                2 => ExportFormat::Tiff,
                3 => ExportFormat::Svg,
                4 => ExportFormat::Pdf,
                _ => ExportFormat::Png,
            },
            width_in: 6.5,
            dpi: s.export_dpi.clamp(72, 1200) as u32,
            path: String::new(),
            status: None,
        }
    }
}

pub struct App {
    pub project: Project,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub sel: Option<Sheet>,
    pub dark: bool,
    pub pal: Palette,
    pub fonts: Arc<FontBook>,

    pub grid: GridState,
    pub chart: ChartPanel,

    pub undo: Vec<Project>,
    pub redo: Vec<Project>,
    pub chart_rev: u64,
    /// 曲线模型全排名缓存：`(数据指纹, 按 AICc 升序的全部拟合)`。
    /// 检查器每帧都会跑这段逻辑，没有缓存的话每帧都要重跑几十组 LM。
    pub fit_cache: Option<(u64, Vec<crate::core::curve::CurveFit>)>,
    /// 表检查器「计算列」输入缓冲（列名 / 公式）。
    pub calc_name: String,
    pub calc_formula: String,

    pub sidebar_query: String,
    pub show_sidebar: bool,
    pub show_inspector: bool,

    pub export: ExportState,
    pub command_open: bool,
    pub new_table_dialog_open: bool,
    /// 侧栏行内改名状态。
    pub sidebar_edit: Option<SidebarEdit>,
    /// 拖放/关联/命令行带进来的待打开文件；脏工程时先过保存确认。
    pub pending_open: Option<PathBuf>,
    pub command_query: String,
    pub command_focus: bool,
    pub about_open: bool,
    pub settings_ui: crate::app::settings_ui::SettingsUi,
    pub confirm: Option<Confirm>,
    pub allow_close: bool,
    pub toasts: Vec<Toast>,

    pub started: Instant,
    pub last_saved: Option<Instant>,
    last_title: String,

    /// 开发期截图：`STATAURA_SHOT=out.png` 时等到第 N 帧后截图并退出。
    shot: Option<ShotJob>,
}

struct ShotJob {
    path: String,
    target: u32,
    frame: u32,
    sent: bool,
    done: Arc<std::sync::atomic::AtomicBool>,
}

fn shot_job() -> Option<ShotJob> {
    let path = std::env::var("STATAURA_SHOT").ok()?;
    let target = std::env::var("STATAURA_SHOT_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(45);
    Some(ShotJob {
        path,
        target,
        frame: 0,
        sent: false,
        done: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    })
}

pub(crate) fn has_project_ext(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("aura" | "prism")
    )
}

pub(crate) fn is_supported_project_path(path: &Path) -> bool {
    path.is_file() && has_project_ext(path)
}

impl App {
    pub fn new(cc: &CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let s = settings::get();
        theme::apply_theme_mode(&cc.egui_ctx, s.theme);
        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::SetTheme(match s.theme {
                0 => egui::SystemTheme::Light,
                1 => egui::SystemTheme::Dark,
                _ => egui::SystemTheme::SystemDefault,
            }));
        let dark = cc.egui_ctx.theme() == egui::Theme::Dark;
        let pal = theme::palette_for(dark, s.accent);
        let fonts = FontBook::shared();
        // 空项目启动（无示例数据）；STATAURA_START=sample 可恢复示例
        let use_sample = std::env::var("STATAURA_START")
            .map(|v| v.eq_ignore_ascii_case("sample"))
            .unwrap_or(false);
        let mut project = if use_sample {
            sample_project()
        } else {
            crate::core::model::Project::empty("未命名项目")
        };
        let start_graph = std::env::var("STATAURA_START")
            .map(|v| v.eq_ignore_ascii_case("graph"))
            .unwrap_or(false);
        let start_empty = std::env::var("STATAURA_START")
            .map(|v| v.eq_ignore_ascii_case("empty"))
            .unwrap_or(false);
        let start_analysis = std::env::var("STATAURA_START")
            .map(|v| v.eq_ignore_ascii_case("analysis"))
            .unwrap_or(false);
        let start_layout = std::env::var("STATAURA_START")
            .map(|v| v.eq_ignore_ascii_case("layout"))
            .unwrap_or(false);
        let first = if start_empty {
            None
        } else if start_layout {
            // 截图/冒烟用：直接建一张拼版页（收进示例工程的前 4 张图）。
            let id = project.add_layout();
            Some(Sheet::Layout(id))
        } else if start_analysis {
            // 截图 / 冒烟用：`STATAURA_ANALYSIS=<序号>` 指定打开第几份分析结果
            // （默认第一份）。与 STATAURA_GRAPH 同一套路。
            let idx = std::env::var("STATAURA_ANALYSIS")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(0);
            project
                .analyses
                .get(idx)
                .or_else(|| project.analyses.first())
                .map(|a| Sheet::Analysis(a.id))
                .or_else(|| project.tables.first().map(|t| Sheet::Table(t.id)))
        } else if start_graph {
            let idx = std::env::var("STATAURA_GRAPH")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1);
            project
                .graphs
                .get(idx)
                .or_else(|| project.graphs.first())
                .map(|g| Sheet::Graph(g.id))
                .or_else(|| project.tables.first().map(|t| Sheet::Table(t.id)))
        } else {
            project.tables.first().map(|t| Sheet::Table(t.id))
        };
        let mut grid = GridState::default();
        if let Some(Sheet::Table(id)) = first {
            grid.reset_for(id, &project);
        }
        let mut app = Self {
            project,
            path: None,
            dirty: false,
            sel: first,
            dark,
            pal,
            chart: ChartPanel::new(fonts.clone()),
            fonts,
            grid,
            undo: Vec::new(),
            redo: Vec::new(),
            chart_rev: 1,
            fit_cache: None,
            calc_name: String::new(),
            calc_formula: String::new(),
            sidebar_query: String::new(),
            show_sidebar: true,
            show_inspector: true,
            export: ExportState::default(),
            command_open: false,
            new_table_dialog_open: false,
            sidebar_edit: None,
            pending_open: None,
            command_query: String::new(),
            command_focus: false,
            about_open: false,
            settings_ui: crate::app::settings_ui::SettingsUi::default(),
            confirm: None,
            allow_close: false,
            toasts: Vec::new(),
            started: Instant::now(),
            last_saved: None,
            last_title: String::new(),
            shot: shot_job(),
        };
        // 视觉回归：`STATAURA_PALETTE=Viridis` 直接把当前图切成指定配色。
        if let Ok(key) = std::env::var("STATAURA_PALETTE") {
            let k = key.trim();
            if let Some(p) = crate::core::palette::PaletteId::ALL
                .iter()
                .copied()
                .find(|p| p.label().contains(k) || format!("{p:?}").eq_ignore_ascii_case(k))
            {
                if let Some(gid) = app.current_graph_id() {
                    if let Some(g) = app.project.graph_mut(gid) {
                        g.spec.palette = p;
                    }
                    app.chart_rev = app.chart_rev.wrapping_add(1);
                }
            }
        }
        // 视觉回归：`STATAURA_ANALYZE=NestedAnova` 在指定表格上跑一次分析并展示结果。
        if let Ok(key) = std::env::var("STATAURA_ANALYZE") {
            let k = key.trim();
            if let Some(kind) = crate::core::model::AnalysisKind::ALL
                .iter()
                .copied()
                .find(|a| format!("{a:?}").eq_ignore_ascii_case(k) || a.label() == k)
            {
                let idx = std::env::var("STATAURA_TABLE")
                    .ok()
                    .and_then(|v| v.parse::<usize>().ok());
                if let Some(idx) = idx {
                    if let Some(t) = app.project.tables.get(idx).cloned() {
                        let mut opts = crate::core::model::AnalysisOptions::default();
                        opts.alpha = settings::get().alpha as f64;
                        if let Ok(res) = crate::core::analysis::run(&t, kind, &opts) {
                            let id = app.project.new_id();
                            app.project.analyses.push(crate::core::model::Analysis {
                                id,
                                name: format!("{} · {}", t.name, kind.label()),
                                table: t.id,
                                kind,
                                options: opts,
                                result: res,
                            });
                            app.sel = Some(Sheet::Analysis(id));
                        }
                    }
                }
            }
        }

        // 视觉回归：`STATAURA_KIND=Violin` 直接把当前图切成指定图型。
        if let Ok(key) = std::env::var("STATAURA_KIND") {
            if let Some(kind) = crate::core::graph::ChartKind::from_key(&key) {
                if let Some(gid) = app.current_graph_id() {
                    if let Some(g) = app.project.graph_mut(gid) {
                        g.spec.kind = kind;
                        if g.spec.title.is_empty() {
                            g.spec.title = g.name.clone();
                        }
                    }
                    app.chart_rev = app.chart_rev.wrapping_add(1);
                }
            }
        }
        // 视觉回归时直接打开某个浮层：`STATAURA_PANEL=export|command|about`
        match std::env::var("STATAURA_PANEL").unwrap_or_default().as_str() {
            "export" => {
                app.open_export();
                app.export.open = true;
            }
            "command" => {
                app.command_open = true;
                app.command_focus = false;
            }
            "about" => app.about_open = true,
            _ => {}
        }
        // 文件关联 / “打开方式”/拖到 exe 上时，路径在 argv[1]。
        if let Some(path) = std::env::args_os()
            .skip(1)
            .map(PathBuf::from)
            .find(|p| is_supported_project_path(p))
        {
            app.request_open_path(path);
        }
        app
    }

    // -----------------------------------------------------------------------
    // 选择与快照
    // -----------------------------------------------------------------------

    pub fn select(&mut self, sheet: Sheet) {
        if self.sel == Some(sheet) {
            return;
        }
        self.sel = Some(sheet);
        match sheet {
            Sheet::Table(id) => self.grid.reset_for(id, &self.project),
            Sheet::Graph(_) | Sheet::Layout(_) => {
                self.chart_rev = self.chart_rev.wrapping_add(1);
            }
            Sheet::Analysis(_) => {}
        }
    }

    pub fn snapshot(&mut self) {
        self.undo.push(self.project.clone());
        if self.undo.len() > 80 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.project, prev));
            self.chart_rev = self.chart_rev.wrapping_add(1);
            self.dirty = true;
            self.fix_selection();
            self.toast("已撤销", ToastKind::Info);
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.project, next));
            self.chart_rev = self.chart_rev.wrapping_add(1);
            self.dirty = true;
            self.fix_selection();
            self.toast("已重做", ToastKind::Info);
        }
    }

    fn fix_selection(&mut self) {
        let valid = match self.sel {
            Some(Sheet::Table(id)) => self.project.table(id).is_some(),
            Some(Sheet::Graph(id)) => self.project.graph(id).is_some(),
            Some(Sheet::Analysis(id)) => self.project.analysis(id).is_some(),
            Some(Sheet::Layout(id)) => self.project.layout(id).is_some(),
            None => false,
        };
        if !valid {
            self.sel = self.project.tables.first().map(|t| Sheet::Table(t.id));
        }
        if let Some(Sheet::Table(id)) = self.sel {
            self.grid.reset_for(id, &self.project);
        }
    }

    pub fn bump_chart(&mut self) {
        self.chart_rev = self.chart_rev.wrapping_add(1);
        self.dirty = true;
    }

    pub fn current_table_id(&self) -> Option<u64> {
        match self.sel {
            Some(Sheet::Table(id)) => Some(id),
            Some(Sheet::Graph(id)) => self.project.graph(id).map(|g| g.table),
            Some(Sheet::Analysis(id)) => self.project.analysis(id).map(|a| a.table),
            // 拼版页没有唯一数据表；返回第一个面板的表（供「分析」等入口定位）。
            Some(Sheet::Layout(id)) => self
                .project
                .layout(id)
                .and_then(|l| l.panels.first())
                .and_then(|gid| self.project.graph(*gid))
                .map(|g| g.table),
            None => self.project.tables.first().map(|t| t.id),
        }
    }

    pub fn current_graph_id(&self) -> Option<u64> {
        match self.sel {
            Some(Sheet::Graph(id)) => Some(id),
            Some(Sheet::Layout(_)) => None,
            _ => self
                .project
                .graphs
                .iter()
                .find(|g| Some(g.table) == self.current_table_id())
                .map(|g| g.id),
        }
    }

    pub fn current_spec(&self) -> Option<&GraphSpec> {
        self.current_graph_id()
            .and_then(|id| self.project.graph(id))
            .map(|g| &g.spec)
    }

    pub fn spec_mut(&mut self) -> Option<&mut GraphSpec> {
        let id = self.current_graph_id()?;
        self.project.graph_mut(id).map(|g| &mut g.spec)
    }

    // -----------------------------------------------------------------------
    // 操作
    // -----------------------------------------------------------------------

    pub fn new_project(&mut self) {
        self.pending_open = None;
        if self.dirty {
            self.confirm = Some(Confirm::New);
            return;
        }
        self.new_project_now();
    }

    pub fn new_project_now(&mut self) {
        self.project = Project::empty("未命名工程");
        self.path = None;
        self.dirty = false;
        self.sel = None;
        self.undo.clear();
        self.redo.clear();
        self.grid = GridState::default();
        self.chart.reset();
        self.toast("已新建工程", ToastKind::Info);
    }

    /// 文件关联 / 拖放 / 命令行共用的入口：脏工程先确认，避免静默丢修改。
    pub fn request_open_path(&mut self, path: PathBuf) {
        if !is_supported_project_path(&path) {
            self.toast("只支持 .aura 或 .prism 文件", ToastKind::Info);
            return;
        }
        if self.dirty {
            self.pending_open = Some(path);
            self.confirm = Some(Confirm::Open);
        } else {
            self.load_path(path);
        }
    }

    /// 保存确认通过后继续打开：有 pending 就打开它，否则走文件选择框。
    pub fn proceed_open(&mut self) {
        self.confirm = None;
        if let Some(path) = self.pending_open.take() {
            self.load_path(path);
        } else {
            self.open_project_now();
        }
    }

    pub fn open_project(&mut self) {
        self.pending_open = None;
        if self.dirty {
            self.confirm = Some(Confirm::Open);
            return;
        }
        self.open_project_now();
    }

    /// 每帧消费拖放进窗口的文件（egui 只在落下的那一帧给出）。
    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let paths: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if paths.is_empty() {
            return;
        }
        if let Some(path) = paths.into_iter().find(|p| is_supported_project_path(p)) {
            self.request_open_path(path);
        } else {
            self.toast("拖入的文件不是 .aura 或 .prism", ToastKind::Info);
        }
    }

    fn paint_drop_hint(&self, ui: &egui::Ui, full: Rect) {
        let pal = self.pal;
        let painter = ui.painter();
        painter.rect_stroke(
            full.shrink(2.0),
            egui::CornerRadius::same(12),
            egui::Stroke::new(2.0, pal.accent),
            egui::StrokeKind::Inside,
        );
        let card = Rect::from_center_size(full.center(), Vec2::new(360.0, 74.0));
        painter.rect_filled(card, egui::CornerRadius::same(14), pal.panel);
        painter.rect_stroke(
            card,
            egui::CornerRadius::same(14),
            egui::Stroke::new(1.0, pal.border),
            egui::StrokeKind::Inside,
        );
        painter.text(
            card.center(),
            egui::Align2::CENTER_CENTER,
            "松开以打开 .aura / .prism",
            egui::FontId::proportional(16.0),
            pal.ink,
        );
    }

    pub fn open_project_now(&mut self) {
        let mut dlg = rfd::FileDialog::new();
        let od = settings::get().open_dir;
        if !od.is_empty() {
            dlg = dlg.set_directory(od);
        }
        let Some(path) = dlg
            .add_filter("所有支持格式", &["aura", "prism"])
            .add_filter("StatAura 工程", &["aura"])
            .add_filter("GraphPad Prism", &["prism"])
            .pick_file()
        else {
            return;
        };
        self.load_path(path);
    }

    pub fn load_path(&mut self, path: PathBuf) {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if ext == "prism" {
            // .prism 文件：解析为多张数据表
            match crate::core::prism::read_prism(&path) {
                Ok(tables) if !tables.is_empty() => {
                    let name = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Prism 导入".into());
                    let mut project = crate::core::model::Project::empty(name);
                    for (i, (tname, mut table)) in tables.into_iter().enumerate() {
                        table.id = project.new_id();
                        table.name = tname;
                        let _ = i;
                        project.tables.push(table);
                    }
                    self.project = project;
                    self.path = Some(path);
                    self.dirty = false;
                    self.undo.clear();
                    self.redo.clear();
                    self.sel = self.project.tables.first().map(|t| Sheet::Table(t.id));
                    if let Some(Sheet::Table(id)) = self.sel {
                        self.grid.reset_for(id, &self.project);
                    }
                    self.chart.reset();
                    self.chart_rev = self.chart_rev.wrapping_add(1);
                    self.toast("Prism 文件已导入", ToastKind::Success);
                }
                Ok(_) => self.toast("Prism 文件中没有数据表", ToastKind::Info),
                Err(e) => self.toast(format!("打开失败：{e}"), ToastKind::Error),
            }
            return;
        }

        match io::load_project(&path) {
            Ok(project) => {
                self.project = project;
                self.path = Some(path);
                self.dirty = false;
                self.undo.clear();
                self.redo.clear();
                self.sel = self.project.tables.first().map(|t| Sheet::Table(t.id));
                if let Some(Sheet::Table(id)) = self.sel {
                    self.grid.reset_for(id, &self.project);
                }
                self.chart.reset();
                self.chart_rev = self.chart_rev.wrapping_add(1);
                self.toast("工程已打开", ToastKind::Success);
            }
            Err(e) => self.toast(format!("打开失败：{e}"), ToastKind::Error),
        }
    }

    pub fn save(&mut self) {
        match self.path.clone() {
            Some(p) => self.save_to(p),
            None => self.save_as(),
        }
    }

    pub fn save_as(&mut self) {
        let default = format!("{}.aura", self.project.name);
        let mut dlg = rfd::FileDialog::new();
        let sd = settings::get().save_dir;
        if !sd.is_empty() {
            dlg = dlg.set_directory(sd);
        }
        let Some(path) = dlg
            .add_filter("StatAura 工程", &["aura"])
            .add_filter("GraphPad Prism", &["prism"])
            .set_file_name(default)
            .save_file()
        else {
            return;
        };
        self.save_to(path);
    }

    fn save_to(&mut self, path: PathBuf) {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let result = if ext == "prism" {
            // 保存为 .prism 格式
            let tables: Vec<(String, crate::core::model::DataTable)> = self
                .project
                .tables
                .iter()
                .map(|t| (t.name.clone(), t.clone()))
                .collect();
            crate::core::prism::write_prism(&path, &tables)
        } else {
            io::save_project(&self.project, &path)
        };

        match result {
            Ok(()) => {
                self.path = Some(path);
                self.dirty = false;
                self.last_saved = Some(Instant::now());
                self.toast("工程已保存", ToastKind::Success);
            }
            Err(e) => self.toast(format!("保存失败：{e}"), ToastKind::Error),
        }
    }

    pub fn import_csv(&mut self) {
        let mut dlg = rfd::FileDialog::new();
        let od = settings::get().open_dir;
        if !od.is_empty() {
            dlg = dlg.set_directory(od);
        }
        let Some(path) = dlg.add_filter("CSV", &["csv", "tsv", "txt"]).pick_file() else {
            return;
        };
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "导入数据".into());
        match io::import_csv(&path, &name) {
            Ok(mut table) => {
                self.snapshot();
                table.id = self.project.new_id();
                let id = table.id;
                self.project.tables.push(table);
                self.select(Sheet::Table(id));
                self.dirty = true;
                self.toast("CSV 已导入", ToastKind::Success);
            }
            Err(e) => self.toast(format!("导入失败：{e}"), ToastKind::Error),
        }
    }

    pub fn add_table(&mut self, kind: TableKind) {
        self.snapshot();
        let id = self.project.add_table(kind, kind.label());
        // 按设置的默认行列数调整新表
        let s = settings::get();
        if let Some(t) = self.project.table_mut(id) {
            let want_rows = s.grid_rows.clamp(5, 100_000);
            let want_cols = s.grid_cols.clamp(2, 52);
            for col in t.columns.iter_mut() {
                col.cells
                    .resize(want_rows, crate::core::model::Cell::empty());
            }
            // 列数：模板已建了默认列，只在不够时补
            while t.columns.len() < want_cols {
                let ci = t.columns.len();
                let name = format!("Group {}", (b'A' + ci as u8) as char);
                let mut c = crate::core::model::Column::new(name, crate::core::model::ColRole::Y);
                c.group = ci;
                c.cells = vec![crate::core::model::Cell::empty(); want_rows];
                t.columns.push(c);
            }
            t.group_names = t.columns.iter().map(|c| c.name.clone()).collect();
        }
        self.select(Sheet::Table(id));
        self.dirty = true;
    }

    // ---- 侧栏右键菜单操作 ----

    /// 删除某个对象（可撤销：先 snapshot）。
    pub fn delete_sheet(&mut self, sheet: Sheet) {
        let name = self.project.item_name(sheet).unwrap_or_default();
        self.snapshot();
        let ok = match sheet {
            Sheet::Table(id) => self.project.remove_table(id),
            Sheet::Analysis(id) => self.project.remove_analysis(id),
            Sheet::Graph(id) => self.project.remove_graph(id),
            Sheet::Layout(id) => self.project.remove_layout(id),
        };
        if ok {
            // 若删除的正是当前选中项，清空选中
            if self.sel == Some(sheet) {
                self.sel = None;
            }
            self.dirty = true;
            self.toast(format!("已删除「{name}」（Ctrl+Z 撤销）"), ToastKind::Info);
        }
    }

    /// 复制对象（表格 / 图表）。
    pub fn duplicate_sheet(&mut self, sheet: Sheet) {
        self.snapshot();
        let new_id = match sheet {
            Sheet::Table(id) => self.project.duplicate_table(id),
            Sheet::Graph(id) => self.project.duplicate_graph(id),
            _ => None,
        };
        if let Some(nid) = new_id {
            let new_sheet = match sheet {
                Sheet::Table(_) => Sheet::Table(nid),
                _ => Sheet::Graph(nid),
            };
            self.select(new_sheet);
            self.dirty = true;
            self.toast("已复制", ToastKind::Info);
        }
    }

    /// 打开重命名对话框。
    pub fn open_rename(&mut self, sheet: Sheet) {
        let cur = self.project.item_name(sheet).unwrap_or_default();
        self.sidebar_edit = Some(SidebarEdit {
            sheet,
            buf: cur,
            just_started: true,
        });
    }

    /// 提交侧栏行内改名（空名 / 同名不落快照）。
    pub fn commit_sidebar_rename(&mut self, sheet: Sheet, name: &str) {
        let name = name.trim();
        if name.is_empty() || self.project.item_name(sheet).as_deref() == Some(name) {
            return;
        }
        self.snapshot();
        self.project.rename_item(sheet, name);
        self.dirty = true;
    }

    pub fn add_graph(&mut self) {
        let Some(table) = self.current_table_id() else {
            self.toast("先选择一张数据表", ToastKind::Info);
            return;
        };
        self.snapshot();
        let id = self.project.add_graph_for(table);
        // 应用图表默认值（设置 → 新图）
        let s = settings::get();
        if let Some(g) = self.project.graph_mut(id) {
            g.spec.show_points = s.def_points;
            g.spec.error = if s.def_error {
                crate::core::graph::ErrorKind::Sd
            } else {
                crate::core::graph::ErrorKind::None
            };
        }
        self.select(Sheet::Graph(id));
        self.chart_rev = self.chart_rev.wrapping_add(1);
        self.dirty = true;
    }

    /// 新建多图拼版页：默认把工程里现有的图全部收进去。
    pub fn add_layout_page(&mut self) {
        if self.project.graphs.is_empty() {
            self.toast("先用「作图」建几张图，再拼版", ToastKind::Info);
            return;
        }
        self.snapshot();
        let id = self.project.add_layout();
        self.select(Sheet::Layout(id));
        self.chart_rev = self.chart_rev.wrapping_add(1);
        self.dirty = true;
        self.toast("已新建拼版页", ToastKind::Success);
    }

    pub fn run_analysis(&mut self, kind: AnalysisKind) {
        let Some(table_id) = self.current_table_id() else {
            self.toast("先选择一张数据表", ToastKind::Info);
            return;
        };
        let Some(table) = self.project.table(table_id) else {
            return;
        };
        let table = table.clone();
        let mut opts = AnalysisOptions::default();
        opts.alpha = settings::get().alpha as f64;
        if kind.needs_two_groups() {
            let g = table.analysis_groups();
            if g.len() < 2 {
                self.toast("这项检验需要至少两个组", ToastKind::Error);
                return;
            }
            opts.columns = vec![0, 1];
        }
        // 多组分析（ANOVA / Kruskal–Wallis / Dunn）默认用全部组；
        // 两组检验只取前两组，避免把第 3 组以后的数据静默忽略掉。
        if kind == AnalysisKind::OneWayAnova || kind.is_rank_family() {
            opts.columns = (0..table.analysis_groups().len()).collect();
        }
        let result = match analysis::run(&table, kind, &opts) {
            Ok(r) => r,
            Err(e) => {
                self.toast(e, ToastKind::Error);
                return;
            }
        };
        self.snapshot();
        let id = self.project.new_id();
        let name = format!("{} · {}", table.name, kind.label());
        self.project.analyses.push(Analysis {
            id,
            name,
            table: table_id,
            kind,
            options: opts,
            result,
        });
        self.select(Sheet::Analysis(id));
        self.dirty = true;
        self.toast("分析完成", ToastKind::Success);
    }

    pub fn auto_significance(&mut self) {
        let Some(gid) = self.current_graph_id() else {
            return;
        };
        let Some(g) = self.project.graph(gid) else {
            return;
        };
        let table_id = g.table;
        let Some(table) = self.project.table(table_id) else {
            return;
        };
        let anns = analysis::auto_significance(table, settings::get().bracket_label_fmt);
        self.snapshot();
        if let Some(g) = self.project.graph_mut(gid) {
            g.spec.annotations = anns;
        }
        self.bump_chart();
        self.toast("已按统计结果生成显著性标注", ToastKind::Success);
    }

    pub fn toggle_theme(&mut self, ctx: &egui::Context) {
        // 以 settings.theme 为唯一真相，在浅(0)/深(1)间翻转。
        // 以前这里只翻 self.dark 却仍调 apply_theme_mode(旧 settings.theme)，
        // 导致 egui visuals（内建控件=菜单/下拉/弹窗）还是浅色，而自绘 UI 用 self.pal 深色：
        // 菜单黑字压在黑底上不可见、弹窗/下拉残留白底。现在三者一致。
        let next = if self.dark { 0 } else { 1 };
        if let Ok(mut g) = settings::live().lock() {
            g.theme = next;
            let _ = g.save();
        }
        theme::apply_theme_mode(ctx, next);
        self.dark = ctx.theme() == egui::Theme::Dark;
        self.pal = theme::palette_for(self.dark, settings::get().accent);
        ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(if next == 1 {
            egui::SystemTheme::Dark
        } else {
            egui::SystemTheme::Light
        }));
    }

    pub fn toast(&mut self, text: impl Into<String>, kind: ToastKind) {
        let now = self.started.elapsed().as_secs_f64();
        self.toasts.push(Toast {
            text: text.into(),
            kind,
            until: now + 3.2,
        });
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    // -----------------------------------------------------------------------
    // 帧
    // -----------------------------------------------------------------------

    /// 让 `self.pal` 始终跟 egui 实际解析出的主题走（跟随系统时 OS 改了也生效）。
    fn sync_theme(&mut self, ctx: &egui::Context) {
        let dark = ctx.theme() == egui::Theme::Dark;
        if dark != self.dark {
            self.dark = dark;
            self.pal = theme::palette_for(dark, settings::get().accent);
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.command_open {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                self.command_open = false;
            }
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::K)) {
            self.command_open = true;
            self.command_query.clear();
            self.command_focus = true;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
            self.save();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::O)) {
            self.open_project();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::N)) {
            self.new_project();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::E)) {
            self.open_export();
        }
        // 撤销 / 重做：全局（不依赖表格选区），用 key_pressed + 手动查 modifier。
        // 复制/剪切/粘贴/清除/A 全选**不在这里**——它们在 grid_body 里用当帧最新选区处理，
        // 避免「帧首读上一帧状态」导致少复制行。
        if self.grid.editing.is_none()
            && self.grid.header_edit.is_none()
            && self.grid.row_edit.is_none()
        {
            let ctrl = ctx.input(|i| i.modifiers.ctrl || i.modifiers.command);
            let shift = ctx.input(|i| i.modifiers.shift);
            let pressed = |k: egui::Key| ctx.input(|i| i.key_pressed(k));
            if ctrl && !shift && pressed(egui::Key::Z) {
                self.undo();
            }
            if (ctrl && pressed(egui::Key::Y)) || (ctrl && shift && pressed(egui::Key::Z)) {
                self.redo();
            }
        }
    }

    pub fn open_export(&mut self) {
        if self.current_graph_id().is_none() {
            self.toast("先选择一张图", ToastKind::Info);
            return;
        }
        if self.export.path.is_empty() {
            let stem = self
                .current_spec()
                .map(|s| s.title.clone())
                .unwrap_or_default();
            let stem = if stem.is_empty() {
                "figure".into()
            } else {
                stem
            };
            let dir = self
                .path
                .as_ref()
                .and_then(|p| p.parent())
                .map(|p| p.to_path_buf())
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));
            self.export.path = dir
                .join(format!("{}.{}", sanitize(&stem), self.export.format.ext()))
                .display()
                .to_string();
        }
        self.export.open = true;
        self.export.status = None;
    }

    fn autosave_if_due(&mut self) {
        if !self.dirty || self.path.is_none() {
            return;
        }
        let s = settings::get();
        // 按设置控制自动备份：未开启则不备份；间隔 = backup_min 分钟
        if !s.auto_backup {
            return;
        }
        let interval_secs = (s.backup_min.clamp(1, 240) * 60) as f32;
        let due = self
            .last_saved
            .map(|t| t.elapsed().as_secs_f32() > interval_secs)
            .unwrap_or(false);
        if due {
            let p = self.path.clone().unwrap();
            if io::save_project(&self.project, &p).is_ok() {
                self.last_saved = Some(Instant::now());
                self.toast("已自动备份", ToastKind::Info);
            }
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| self.project.name.clone());
        let title = format!("StatAura — {}{}", name, if self.dirty { " ●" } else { "" });
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }

    fn toasts_ui(&mut self, ctx: &egui::Context) {
        let now = self.started.elapsed().as_secs_f64();
        self.toasts.retain(|t| t.until > now);
        if self.toasts.is_empty() {
            return;
        }
        let mut area = egui::Area::new(egui::Id::unique("toasts"))
            .anchor(egui::Align2::RIGHT_BOTTOM, Vec2::new(-18.0, -40.0))
            .order(egui::Order::Foreground)
            .interactable(false);
        area = area.constrain(true);
        area.show(ctx, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                for t in self.toasts.iter().rev() {
                    let (fg, bg) = match t.kind {
                        ToastKind::Info => (self.pal.ink, self.pal.panel),
                        ToastKind::Success => (Color32::WHITE, self.pal.success),
                        ToastKind::Error => (Color32::WHITE, self.pal.danger),
                    };
                    egui::Frame::new()
                        .fill(bg)
                        .corner_radius(9)
                        .inner_margin(egui::Margin::symmetric(14, 9))
                        .stroke(egui::Stroke::new(1.0, self.pal.border))
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(&t.text).size(12.0).color(fg));
                        });
                }
            });
        });
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

pub fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect();
    let t = cleaned.trim();
    if t.is_empty() {
        "figure".into()
    } else {
        t.chars().take(80).collect()
    }
}

fn area<R>(ui: &mut egui::Ui, rect: Rect, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(rect).id_salt((
            "area",
            rect.left() as i32,
            rect.top() as i32,
        )),
        |ui| {
            ui.set_clip_rect(rect);
            add(ui)
        },
    )
    .inner
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        self.pal.bg.to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.sync_theme(&ctx);
        self.handle_dropped_files(&ctx);
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm = Some(Confirm::Close);
        }
        self.shortcuts(&ctx);
        self.chart.poll(&ctx);
        self.autosave_if_due();
        self.update_title(&ctx);

        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0, self.pal.bg);

        let top = Rect::from_min_size(full.min, Vec2::new(full.width(), theme::TOPBAR_H));
        let status = Rect::from_min_size(
            egui::pos2(full.left(), full.bottom() - theme::STATUS_H),
            Vec2::new(full.width(), theme::STATUS_H),
        );
        let body = Rect::from_min_max(
            egui::pos2(full.left(), top.bottom()),
            egui::pos2(full.right(), status.top()),
        );

        let sidebar_w = if self.show_sidebar {
            theme::SIDEBAR_W
        } else {
            0.0
        };
        let inspector_w = if self.show_inspector {
            theme::INSPECTOR_W
        } else {
            0.0
        };
        let sidebar = Rect::from_min_size(body.min, Vec2::new(sidebar_w, body.height()));
        let inspector = Rect::from_min_size(
            egui::pos2(body.right() - inspector_w, body.top()),
            Vec2::new(inspector_w, body.height()),
        );
        let center = Rect::from_min_max(
            egui::pos2(body.left() + sidebar_w, body.top()),
            egui::pos2(body.right() - inspector_w, body.bottom()),
        );

        self.topbar(ui, top);
        if self.show_sidebar {
            self.sidebar(ui, sidebar);
        }
        self.center(ui, center);
        if self.show_inspector {
            self.inspector_view(ui, inspector);
        }
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            self.paint_drop_hint(ui, full);
        }
        self.statusbar(ui, status);
        // 模态遮罩必须在所有弹窗之前画（Order::Middle < Window 的 Foreground）
        self.modal_overlay(&ctx);
        self.dialogs(&ctx);
        self.modals(&ctx);
        // 设置对话框：应用后立即生效
        if self.settings_ui.show(&ctx, &self.pal) {
            let s = settings::get();
            // 主题（浅色/深色/跟随系统）
            theme::apply_theme_mode(&ctx, s.theme);
            ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(match s.theme {
                0 => egui::SystemTheme::Light,
                1 => egui::SystemTheme::Dark,
                _ => egui::SystemTheme::SystemDefault,
            }));
            self.dark = ctx.theme() == egui::Theme::Dark;
            self.pal = theme::palette_for(self.dark, s.accent);
            // 面板显隐同步
            self.show_sidebar = s.show_navigator;
            self.show_inspector = s.show_inspector;
        }
        self.toasts_ui(&ctx);
        self.shot_tick(&ctx);
    }
}

impl App {
    /// 开发期截图钩子：`STATAURA_SHOT=out.png`，等版面稳定后截图、写盘、退出。
    ///
    /// 用来给 UI 改动做视觉回归，不参与正常运行（环境变量不存在时零开销）。
    fn shot_tick(&mut self, ctx: &egui::Context) {
        let Some(job) = self.shot.as_mut() else {
            return;
        };
        if job.done.load(std::sync::atomic::Ordering::Relaxed) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        job.frame += 1;
        if job.frame < job.target {
            ctx.request_repaint();
            return;
        }
        if job.sent {
            ctx.request_repaint();
            return;
        }
        job.sent = true;
        let path = job.path.clone();
        let flag = job.done.clone();
        ctx.request_screenshot(move |image| {
            let w = image.size[0];
            let h = image.size[1];
            let mut buf = Vec::with_capacity(w * h * 4);
            for px in image.pixels.iter() {
                buf.extend_from_slice(&[px.r(), px.g(), px.b(), px.a()]);
            }
            match image::save_buffer(
                &path,
                &buf,
                w as u32,
                h as u32,
                image::ExtendedColorType::Rgba8,
            ) {
                Ok(()) => eprintln!("shot saved: {path} ({w}x{h})"),
                Err(e) => eprintln!("shot failed: {e}"),
            }
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        });
        ctx.request_repaint();
    }
}

pub fn area_public<R>(ui: &mut egui::Ui, rect: Rect, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    area(ui, rect, add)
}

impl App {
    fn center(&mut self, ui: &mut egui::Ui, rect: Rect) {
        match self.sel {
            Some(Sheet::Table(id)) if self.project.table(id).is_some() => {
                self.table_view(ui, rect, id)
            }
            Some(Sheet::Analysis(id)) if self.project.analysis(id).is_some() => {
                self.results_view(ui, rect, id)
            }
            Some(Sheet::Graph(id)) if self.project.graph(id).is_some() => {
                self.chart_view(ui, rect, id)
            }
            Some(Sheet::Layout(id)) if self.project.layout(id).is_some() => {
                self.layout_view(ui, rect, id)
            }
            _ => self.empty_state(ui, rect),
        }
    }
}

#[cfg(test)]
mod open_path_tests {
    use super::has_project_ext;
    use std::path::Path;

    #[test]
    fn project_extensions_are_case_insensitive() {
        assert!(has_project_ext(Path::new("a.aura")));
        assert!(has_project_ext(Path::new("a.AURA")));
        assert!(has_project_ext(Path::new("a.prism")));
        assert!(has_project_ext(Path::new("a.Prism")));
        assert!(!has_project_ext(Path::new("a.csv")));
        assert!(!has_project_ext(Path::new("aura")));
    }
}
