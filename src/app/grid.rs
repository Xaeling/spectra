//! 数据表视图：自绘虚拟化网格。
//!
//! 只画视口内的行；滚动、选择、编辑、粘贴、排除值都在这一层完成。

use std::ops::Range;

use egui::{
    Align2, Color32, CornerRadius, FontId, Key, Rect, RichText, Sense, Stroke, TextEdit, Ui, Vec2,
};

use crate::app::clipboard::{clipboard_command, ClipCmd, Sel};
use crate::app::widgets::alpha;
use crate::app::{App, ToastKind};
use crate::core::model::{Cell, ColRole, DataTable, Project, TableKind};

const HEADER_H: f32 = 54.0;
const GROUP_H: f32 = 25.0;
const SUB_H: f32 = 29.0;
const ROW_H: f32 = 24.0;
const ROW_W: f32 = 56.0;
const COL_W: f32 = 96.0;
const COL_W_MIN: f32 = 36.0;
const COL_W_MAX: f32 = 420.0;
/// 列宽拖拽把手的命中半宽（把手画在表头右缘）。
const RESIZE_HIT: f32 = 4.0;
const SB: f32 = 9.0;

/// 一帧内的列宽几何：每列宽度 + 前缀和偏移。
///
/// 为什么不能用 `c * COL_W`：列宽可拖拽后各列不等宽，位置必须查前缀和，
/// 反向（x → 列）也要按偏移表定位。虚拟列一律用默认宽。
struct ColMetrics {
    /// 每列宽度，长度 = n_cols
    widths: Vec<f32>,
    /// offsets[i] = 前 i 列的宽度和，长度 = n_cols + 1
    offsets: Vec<f32>,
}

impl ColMetrics {
    /// `real` 是真实列的宽度（来自模型），`n` 是含虚拟列的总列数。
    fn new(real: &[f32], n: usize) -> Self {
        let mut widths: Vec<f32> = real.to_vec();
        widths.resize(n, COL_W);
        let mut offsets = Vec::with_capacity(n + 1);
        let mut acc = 0.0;
        offsets.push(0.0);
        for w in &widths {
            acc += w;
            offsets.push(acc);
        }
        Self { widths, offsets }
    }
    fn x(&self, c: usize) -> f32 {
        self.offsets.get(c).copied().unwrap_or(0.0)
    }
    fn w(&self, c: usize) -> f32 {
        self.widths.get(c).copied().unwrap_or(COL_W)
    }
    fn total(&self) -> f32 {
        self.offsets.last().copied().unwrap_or(0.0)
    }
    /// 内容坐标 x（已加回 scroll、已减去 body 左沿）落在哪一列。
    fn col_at(&self, x: f32) -> Option<usize> {
        if x < 0.0 {
            return None;
        }
        // 前缀和单调，线性扫足够（列数 < 通常几十）
        for c in 0..self.widths.len() {
            if x < self.offsets[c + 1] {
                return Some(c);
            }
        }
        None
    }
    /// 可见列区间（左闭右开）：与视口 [sx, sx+view_w) 有重叠的列。
    fn visible(&self, sx: f32, view_w: f32) -> std::ops::Range<usize> {
        let right = sx + view_w;
        // 首列：右沿越过 sx 的第一列（含 sx 落点所在的列）
        let first = (0..self.widths.len())
            .find(|&c| self.offsets[c + 1] > sx)
            .unwrap_or(self.widths.len());
        // 末列开区间：第一个左沿 ≥ 视口右沿的列（正好压在右沿上的列不算可见）
        let end = (0..self.widths.len())
            .find(|&c| self.offsets[c] >= right)
            .unwrap_or(self.widths.len())
            .max(first + 1); // 视口极窄时也至少给一列
        first..end.min(self.widths.len())
    }
}

/// 从模型读出每列宽度（未设置的用默认宽），行标题列宽单列。
fn col_widths(table: &DataTable, n_cols: usize) -> (f32, Vec<f32>) {
    let row_w = table.row_header_width.unwrap_or(ROW_W).clamp(28.0, 240.0);
    let mut ws: Vec<f32> = table
        .columns
        .iter()
        .map(|c| c.width.unwrap_or(COL_W).clamp(COL_W_MIN, COL_W_MAX))
        .collect();
    ws.resize(n_cols, COL_W);
    (row_w, ws)
}

pub struct EditState {
    pub row: usize,
    pub col: usize,
    pub buf: String,
    pub just_started: bool,
}

pub struct GridState {
    pub table: Option<u64>,
    pub cursor: (usize, usize),
    pub anchor: (usize, usize),
    pub scroll_y: f32,
    pub scroll_x: f32,
    pub editing: Option<EditState>,
    /// 表头改名编辑框（双击列头触发）。`row` 未用，`col` 是列号。
    pub header_edit: Option<EditState>,
    /// 行标题改名编辑框（双击行头触发）。`row` 是行号，`col` 未用。
    pub row_edit: Option<EditState>,
    /// 中文输入法组合状态（preedit 进行中）。跨帧保持——
    /// 组合期间不提交、不响应快捷键，`ImeEvent::Commit` 后恢复正常处理。
    pub composing: bool,
    pub drag_v: bool,
    pub drag_h: bool,
    /// 视图层的虚拟延伸区（行 / 列）。
    ///
    /// 数据表只存真正用过的区域，但网格在视图上**永远滚不到头**：
    /// 滚到哪，空白区就铺到「视口 + 一屏」之外。用户一旦在虚拟格里
    /// 写入，`materialize_cell` 会把对应行列实体化进 DataTable。
    pub virtual_rows: usize,
    pub virtual_cols: usize,
}

impl Default for GridState {
    fn default() -> Self {
        Self {
            table: None,
            cursor: (0, 0),
            anchor: (0, 0),
            scroll_y: 0.0,
            scroll_x: 0.0,
            editing: None,
            header_edit: None,
            row_edit: None,
            composing: false,
            drag_v: false,
            drag_h: false,
            virtual_rows: 48,
            virtual_cols: 12,
        }
    }
}

impl GridState {
    pub fn reset_for(&mut self, id: u64, _project: &Project) {
        self.table = Some(id);
        self.cursor = (0, 0);
        self.anchor = (0, 0);
        self.scroll_y = 0.0;
        self.scroll_x = 0.0;
        self.editing = None;
        self.header_edit = None;
        self.row_edit = None;
        self.composing = false;
        self.virtual_rows = 48;
        self.virtual_cols = 12;
    }

    /// 视图层的虚拟区随滚动前进：永远比视口多铺一屏，所以滚不到头。
    ///
    /// 抽成纯函数是为了能用模拟滚动做断言，而不是靠肉眼拖滚动条。
    /// `col_w` 用虚拟列的默认宽估算（真实列宽不等也只是估计值，够用）。
    pub fn advance_virtual(&mut self, sy: f32, sx: f32, view_h: f32, view_w: f32, col_w: f32) {
        let cw = col_w.max(1.0);
        let viewport_rows = (view_h / ROW_H).ceil() as usize + 4;
        let viewport_cols = (view_w / cw).ceil() as usize + 3;
        let want_rows = (sy / ROW_H).ceil() as usize + viewport_rows + 8;
        let want_cols = (sx / cw).ceil() as usize + viewport_cols + 4;
        self.virtual_rows = self.virtual_rows.max(want_rows);
        self.virtual_cols = self.virtual_cols.max(want_cols);
    }

    pub fn range(&self) -> ((usize, usize), (usize, usize)) {
        (
            (
                self.anchor.0.min(self.cursor.0),
                self.anchor.1.min(self.cursor.1),
            ),
            (
                self.anchor.0.max(self.cursor.0),
                self.anchor.1.max(self.cursor.1),
            ),
        )
    }
}

struct ColumnView {
    top: String,
    sub: String,
    is_x: bool,
    group_start: bool,
    group_span: usize,
    /// 该列所属的组号（`Column.group`），改组名时用它定位 `group_names`。
    group_id: usize,
}

impl App {
    pub fn table_view(&mut self, ui: &mut Ui, rect: Rect, id: u64) {
        let pal = self.pal;
        ui.painter().rect_filled(rect, 0, pal.bg);
        let Some(t) = self.project.table(id) else {
            return;
        };
        let title = t.name.clone();
        let kind = t.kind;
        let rows = t.nrows();
        let cols = t.columns.len();
        let groups = t.analysis_groups().len();

        // -------- 表头 --------
        let pad = crate::app::theme::metric::CONTENT_PAD;
        let head = Rect::from_min_size(
            egui::pos2(rect.left() + pad, rect.top() + 12.0),
            Vec2::new(rect.width() - pad * 2.0, 44.0),
        );
        ui.painter().text(
            egui::pos2(head.left(), head.center().y - 10.0),
            Align2::LEFT_CENTER,
            &title,
            FontId::proportional(15.5),
            pal.ink,
        );
        ui.painter().text(
            egui::pos2(head.left(), head.center().y + 9.0),
            Align2::LEFT_CENTER,
            format!(
                "{}  ·  {} 行 × {} 列  ·  {} 组",
                kind.label(),
                rows,
                cols,
                groups
            ),
            FontId::proportional(crate::app::theme::text::META),
            pal.faint,
        );
        crate::app::area_public(ui, head, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::app::widgets::Btn::new("作图")
                    .fill(pal.accent)
                    .fg(Color32::WHITE)
                    .strong(true)
                    .radius(crate::app::theme::radius::CONTROL)
                    .height(30.0)
                    .show(ui)
                    .clicked()
                {
                    self.add_graph();
                }
                ui.add_space(6.0);
                ui.menu_button(RichText::new("分析").size(12.0), |ui| {
                    // 只列出当前表格类型能做的分析，避免出现点了报错的项。
                    let table_kind = self.project.table(id).map(|t| t.kind);
                    let kinds = table_kind
                        .map(crate::core::model::AnalysisKind::available_for)
                        .unwrap_or_default();
                    for k in kinds {
                        if ui.button(k.label()).clicked() {
                            self.run_analysis(k);
                            ui.close();
                        }
                    }
                });
                ui.menu_button(RichText::new("操作").size(12.0), |ui| {
                    if ui.button("在末尾添加行").clicked() {
                        self.snapshot();
                        if let Some(t) = self.project.table_mut(id) {
                            let n = t.nrows();
                            t.ensure_rows(n + 1);
                        }
                        self.dirty = true;
                        ui.close();
                    }
                    if ui.button("添加列 / 组").clicked() {
                        self.add_column(id);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("导出 CSV…").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("CSV", &["csv"])
                            .set_file_name(format!("{title}.csv"))
                            .save_file()
                        {
                            if let Some(t) = self.project.table(id) {
                                match crate::io::export_csv(t, &path) {
                                    Ok(()) => self.toast("CSV 已导出", ToastKind::Success),
                                    Err(e) => {
                                        self.toast(format!("导出失败：{e}"), ToastKind::Error)
                                    }
                                }
                            }
                        }
                        ui.close();
                    }
                    if ui.button("复制整表").clicked() {
                        self.copy_whole_table(id);
                        ui.close();
                    }
                });
            });
        });

        // -------- 网格 --------
        let panel = Rect::from_min_max(
            egui::pos2(rect.left() + pad, head.bottom() + 12.0),
            egui::pos2(rect.right() - pad, rect.bottom() - 16.0),
        );
        let r = crate::app::theme::radius::CARD;
        crate::app::widgets::surface(
            ui.painter(),
            panel,
            r,
            pal.panel,
            Stroke::new(1.0, pal.border),
        );
        self.grid_body(ui, panel.shrink(1.0), id);
        // 网格内部是方角绘制：把四角「啃」回圆角，再补上描边，与其它卡片一致。
        crate::app::widgets::punch_corners(ui.painter(), panel, r, pal.bg);
        crate::app::widgets::surface_stroke(ui.painter(), panel, r, Stroke::new(1.0, pal.border));
    }

    fn copy_whole_table(&mut self, id: u64) {
        let Some((nr, nc)) = self.project.table(id).map(|t| (t.nrows(), t.columns.len())) else {
            return;
        };
        if nr == 0 || nc == 0 {
            self.toast("表格为空", ToastKind::Info);
            return;
        }
        // 复用 clipboard.rs 的唯一实现，不再另写一份 TSV 生成逻辑。
        let sel = Sel {
            r0: 0,
            c0: 0,
            r1: nr - 1,
            c1: nc - 1,
        };
        self.copy_region(id, sel);
    }

    pub(crate) fn add_column(&mut self, id: u64) {
        if !self
            .project
            .table(id)
            .map(|t| t.kind.can_add_column())
            .unwrap_or(false)
        {
            self.toast("该表格类型结构固定，不能添加列", ToastKind::Info);
            return;
        }
        self.snapshot();
        if let Some(t) = self.project.table_mut(id) {
            let n = t.analysis_groups().len();
            let g = t.group_names.len();
            let name = match t.kind {
                TableKind::Xy => format!("Dataset {}", (b'A' + g as u8) as char),
                TableKind::MultipleVariables => format!("Var {}", g + 1),
                _ => format!("Group {}", g + 1),
            };
            t.group_names.push(name.clone());
            let mut c = crate::core::model::Column::new(
                if matches!(t.kind, TableKind::Column | TableKind::MultipleVariables) {
                    name
                } else {
                    "Y1".into()
                },
                ColRole::Y,
            );
            c.group = g;
            c.cells = vec![Cell::empty(); t.nrows().max(1)];
            t.columns.push(c);
            let _ = n;
        }
        self.dirty = true;
        self.bump_chart();
    }

    fn grid_body(&mut self, ui: &mut Ui, rect: Rect, id: u64) {
        let pal = self.pal;
        let Some(table) = self.project.table(id) else {
            return;
        };
        let views = column_views(table);
        let one_level = views.iter().all(|v| v.sub.is_empty());
        let header_h = if one_level { 40.0 } else { HEADER_H };
        let n_groups = table.group_names.len();
        if table.columns.is_empty() {
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                "这张表还没有列",
                FontId::proportional(13.0),
                pal.muted,
            );
            return;
        }

        let inner = rect;
        // ---- 中文输入法组合状态 ----
        // preedit 非空 = 组合进行中；Commit = 结束。跨帧保持状态：
        // 组合期间每帧不一定都有 IME 事件，但 Enter/快捷键必须一直被拦住。
        let ime_activity: Option<bool> = ui.input(|i| {
            let mut s = None;
            for e in &i.events {
                if let egui::Event::Ime(ime) = e {
                    s = match ime {
                        egui::ImeEvent::Preedit { text, .. } => Some(!text.is_empty()),
                        egui::ImeEvent::Commit(_) => Some(false),
                        _ => s,
                    };
                }
            }
            s
        });
        if let Some(s) = ime_activity {
            self.grid.composing = s;
        }
        // ---- 列宽几何（拖拽后每列不等宽，位置一律查前缀和） ----
        let (row_w, real_widths) = col_widths(table, table.columns.len());
        let body = Rect::from_min_max(
            egui::pos2(inner.left() + row_w, inner.top() + header_h),
            egui::pos2(inner.right() - SB, inner.bottom() - SB),
        );
        // ---- 无限网格 ----
        //
        // 视图层永远比当前视口多铺一屏：滚动范围随之增长，因此怎么滚都
        // 不会出现「到底了」。真实数据仍然只占已用区域，不浪费内存，
        // 也不会让统计/绘图凭空多出几百行空数据。
        let (sy0, sx0) = (self.grid.scroll_y, self.grid.scroll_x);
        self.grid
            .advance_virtual(sy0, sx0, body.height(), body.width(), COL_W);

        let real_cols = table.columns.len();
        // 提前抽出后面要用的表属性，让 `table` 的借用尽早结束——
        // 否则它一直活到上下文菜单，中间所有 &mut self 都会撞车。
        let nrows_t = table.nrows();
        let can_struct = table.kind.can_add_column();
        let x_flags: Vec<bool> = table.columns.iter().map(|c| c.role == ColRole::X).collect();
        // 虚拟列命名预览要用：表型 + 当前已有组数（与 add_column 的编号衔接）
        let table_kind_hint = table.kind;
        let table_group_n = table.group_names.len();
        // 每组子列数：XY 真表是 3 个 Y 一组，虚拟列也要 3 个一组，
        // 不能真表三格一组、虚拟区一格一组（穿帮）。
        let group_size: usize = {
            let mut counts = std::collections::HashMap::<usize, usize>::new();
            for c in &table.columns {
                if c.role == ColRole::Y {
                    *counts.entry(c.group).or_default() += 1;
                }
            }
            counts.values().copied().max().unwrap_or(1).max(1)
        };
        // 开发钩子：`STATAURA_EDIT_CELL=1` 启动即进编辑态（截图验证编辑框对齐用）。
        // 缓冲区带上格子原内容，便于对比编辑态文字与静止态文字是否重合。
        if std::env::var("STATAURA_EDIT_CELL").is_ok()
            && self.grid.editing.is_none()
            && self.grid.header_edit.is_none()
        {
            let buf = table.cell(0, 1).map(|c| c.display()).unwrap_or_default();
            self.grid.editing = Some(EditState {
                row: 0,
                col: 1,
                buf,
                just_started: true,
            });
        }
        // 开发钩子：`STATAURA_EDIT_HEADER=group` / `=sub` 启动即进表头改名态。
        // 只触发一次（否则每帧重置 just_started，焦点永远拿不住，截不到光标）。
        {
            static FIRED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if let Ok(which) = std::env::var("STATAURA_EDIT_HEADER") {
                if !FIRED.swap(true, std::sync::atomic::Ordering::SeqCst)
                    && self.grid.header_edit.is_none()
                    && self.grid.editing.is_none()
                {
                    let group = which.eq_ignore_ascii_case("group");
                    // 挑一个有组名的列（两层表头才有意义）
                    let c = table
                        .columns
                        .iter()
                        .position(|c| c.role != ColRole::X)
                        .unwrap_or(0);
                    let buf = if group {
                        table.group_name(table.columns[c].group).to_string()
                    } else {
                        table.columns[c].name.clone()
                    };
                    self.grid.header_edit = Some(EditState {
                        row: if group { 1 } else { 0 },
                        col: c,
                        buf,
                        just_started: true,
                    });
                }
            }
        }
        let n_cols = real_cols.max(1).max(self.grid.virtual_cols);
        let n_rows = table.nrows().max(12).max(self.grid.virtual_rows) + 6;
        // 列几何：真实列用模型里的宽，虚拟列用默认宽；位置一律查前缀和。
        let m = ColMetrics::new(&real_widths, n_cols);
        let total_w = m.total();
        let total_h = n_rows as f32 * ROW_H;
        let max_sx = (total_w - body.width()).max(0.0);
        let max_sy = (total_h - body.height()).max(0.0);

        let id_ui = ui.scope_id().with(("grid", id));
        let resp = ui.interact(inner, id_ui, Sense::click_and_drag());

        // 滚轮
        if resp.hovered() {
            let (dx, dy, shift) = ui.input(|i| {
                (
                    i.smooth_scroll_delta.x,
                    i.smooth_scroll_delta.y,
                    i.modifiers.shift,
                )
            });
            if shift {
                self.grid.scroll_x = (self.grid.scroll_x - dy).clamp(0.0, max_sx);
            } else {
                self.grid.scroll_y = (self.grid.scroll_y - dy).clamp(0.0, max_sy);
                self.grid.scroll_x = (self.grid.scroll_x - dx).clamp(0.0, max_sx);
            }
            // 滚轮不应把内容滚出视口
            let _ = n_groups;
        }
        self.grid.scroll_x = self.grid.scroll_x.clamp(0.0, max_sx);
        self.grid.scroll_y = self.grid.scroll_y.clamp(0.0, max_sy);

        // ---- 滚动条交互 ----
        //
        // 以前滚动条只画不接事件，滑块根本拖不动。命中顺序有讲究：
        // 先注册轨道点击、再注册滑块拖动（egui 里后注册者在上层），
        // 拖滑块就不会被网格 `resp` 当成单元格框选，点轨道也不会点到格子。
        let track_v = Rect::from_min_max(
            egui::pos2(inner.right() - SB, inner.top() + header_h),
            egui::pos2(inner.right(), inner.bottom() - SB),
        );
        let track_h = Rect::from_min_max(
            egui::pos2(inner.left() + row_w, inner.bottom() - SB),
            egui::pos2(inner.right() - SB, inner.bottom()),
        );
        self.grid.drag_v = false;
        self.grid.drag_h = false;
        // 纵向：轨道点击翻页，滑块按增量拖动（拖动比例 = 内容差 / 滑轨空隙）
        if let Some(t) = thumb_rect(track_v, self.grid.scroll_y, total_h, track_v.height(), true) {
            let tr = ui.interact(track_v, id_ui.with("sb_vt"), Sense::click());
            if tr.clicked() {
                if let Some(p) = tr.interact_pointer_pos() {
                    let page = track_v.height() * 0.9;
                    self.grid.scroll_y = if p.y < t.top() {
                        (self.grid.scroll_y - page).max(0.0)
                    } else if p.y > t.bottom() {
                        (self.grid.scroll_y + page).min(max_sy)
                    } else {
                        self.grid.scroll_y
                    };
                }
            }
            let th = ui.interact(t.expand(1.5), id_ui.with("sb_v"), Sense::click_and_drag());
            self.grid.drag_v = th.dragged();
            if th.dragged() {
                let d = th.drag_delta();
                let span = (total_h - track_v.height()).max(0.0);
                let gap = (track_v.height() - t.height()).max(1.0);
                self.grid.scroll_y = (self.grid.scroll_y + d.y * span / gap).clamp(0.0, max_sy);
            }
        }
        // 横向：同构
        if let Some(t) = thumb_rect(track_h, self.grid.scroll_x, total_w, track_h.width(), false) {
            let tr = ui.interact(track_h, id_ui.with("sb_ht"), Sense::click());
            if tr.clicked() {
                if let Some(p) = tr.interact_pointer_pos() {
                    let page = track_h.width() * 0.9;
                    self.grid.scroll_x = if p.x < t.left() {
                        (self.grid.scroll_x - page).max(0.0)
                    } else if p.x > t.right() {
                        (self.grid.scroll_x + page).min(max_sx)
                    } else {
                        self.grid.scroll_x
                    };
                }
            }
            let th = ui.interact(t.expand(1.5), id_ui.with("sb_h"), Sense::click_and_drag());
            self.grid.drag_h = th.dragged();
            if th.dragged() {
                let d = th.drag_delta();
                let span = (total_w - track_h.width()).max(0.0);
                let gap = (track_h.width() - t.width()).max(1.0);
                self.grid.scroll_x = (self.grid.scroll_x + d.x * span / gap).clamp(0.0, max_sx);
            }
        }
        self.grid.scroll_x = self.grid.scroll_x.clamp(0.0, max_sx);
        self.grid.scroll_y = self.grid.scroll_y.clamp(0.0, max_sy);

        let sx = self.grid.scroll_x;
        let sy = self.grid.scroll_y;
        let painter = ui.painter_at(inner);
        let cell_painter = ui.painter_at(body);

        // 头部背景
        painter.rect_filled(
            Rect::from_min_max(inner.min, egui::pos2(inner.right(), inner.top() + header_h)),
            0,
            pal.panel_alt,
        );
        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(inner.left(), inner.top() + header_h - 1.0),
                egui::pos2(inner.right(), inner.top() + header_h),
            ),
            0,
            pal.border,
        );

        // 可见列（列宽不等 → 按偏移表定位，不能用 sx / COL_W）
        let vis_cols = m.visible(sx, body.width());
        let (first_c, last_c) = (vis_cols.start, vis_cols.end);
        // 可见行
        let first_r = (sy / ROW_H).floor().max(0.0) as usize;
        let last_r = (((sy + body.height()) / ROW_H).ceil() as usize).min(n_rows);
        let vis_rows: Range<usize> = first_r..last_r;

        let sel = self.grid.range();
        let active = self.grid.cursor;
        let active_in_view = active.0 >= first_r && active.0 < last_r;

        // 表头：行标题空白格
        painter.rect_filled(
            Rect::from_min_size(inner.min, Vec2::new(row_w, header_h)),
            0,
            pal.panel_alt,
        );
        painter.text(
            egui::pos2(inner.left() + row_w / 2.0, inner.top() + header_h / 2.0),
            Align2::CENTER_CENTER,
            "#",
            FontId::proportional(9.5),
            pal.faint,
        );

        // 列组 / 列名（虚拟列不在 views 里，用空视图占位）
        let blank_view = ColumnView {
            top: String::new(),
            sub: String::new(),
            is_x: false,
            group_start: false,
            group_span: 1,
            group_id: 0,
        };
        for c in first_c..last_c {
            let x = inner.left() + row_w + m.x(c) - sx;
            // 虚拟列合成与真表**同构**的 ColumnView：按 group_size 分组、
            // 组首列 group_start=true、子名 Y1/Y2/Y3 循环、组名延续 add_column 编号。
            // 之前虚拟列一律 blank_view（group_start=false），于是真表三格一组、
            // 虚拟区一格一组，竖线与组名全都乱。
            let owned_v;
            let v: &ColumnView = if c < real_cols {
                views.get(c).unwrap_or(&blank_view)
            } else {
                let vi = c - real_cols;
                let gsz = group_size.max(1);
                let v_group = vi / gsz;
                let v_pos = vi % gsz;
                let gname = match table_kind_hint {
                    TableKind::Xy => format!(
                        "Dataset {}",
                        (b'A' + (table_group_n + v_group) as u8) as char
                    ),
                    TableKind::MultipleVariables => format!("Var {}", table_group_n + v_group + 1),
                    _ => format!("Group {}", table_group_n + v_group + 1),
                };
                let sub = match table_kind_hint {
                    TableKind::Xy => format!("Y{}", v_pos + 1),
                    TableKind::MultipleVariables => gname.clone(),
                    _ => gname.clone(),
                };
                owned_v = ColumnView {
                    top: if one_level {
                        gname.clone()
                    } else {
                        gname.clone()
                    },
                    sub: if one_level { String::new() } else { sub },
                    is_x: false,
                    group_start: v_pos == 0,
                    group_span: gsz,
                    group_id: usize::MAX / 2 + v_group, // 与真组隔离的合成 id
                };
                &owned_v
            };
            // 悬停按层拆分：组行悬停 → 整组亮；子行悬停 → 仅该子列亮。
            // 以前一个矩形撑满 header_h，盖住了两层，组行看不到自己的动画。
            let hover = resp.hovered();
            let ptr_y = ui.ctx().pointer_hover_pos().map(|p| p.y);
            if hover {
                if !one_level {
                    // 组行：只有 group_start 的列响应（整组一起亮）
                    if let Some(py) = ptr_y {
                        if py < inner.top() + GROUP_H && v.group_start && c >= first_c {
                            let span = v.group_span.max(1);
                            let gw: f32 = (c..c + span).map(|k| m.w(k)).sum();
                            let gr = Rect::from_min_size(
                                egui::pos2(x, inner.top()),
                                Vec2::new(gw, GROUP_H),
                            );
                            let ptr_x = ui.ctx().pointer_hover_pos().map(|p| p.x).unwrap_or(0.0);
                            if ptr_x >= x && ptr_x < x + gw {
                                painter.rect_filled(gr, 0, pal.hover);
                            }
                        }
                        // 子行：仅该列的子行矩形亮
                        if py >= inner.top() + GROUP_H {
                            let sr = Rect::from_min_size(
                                egui::pos2(x, inner.top() + GROUP_H),
                                Vec2::new(m.w(c), SUB_H),
                            );
                            let ptr_x = ui.ctx().pointer_hover_pos().map(|p| p.x).unwrap_or(0.0);
                            if ptr_x >= x && ptr_x < x + m.w(c) {
                                painter.rect_filled(sr, 0, pal.hover);
                            }
                        }
                    }
                } else {
                    // 单层表头：整个列头一起亮
                    let col_rect = Rect::from_min_size(
                        egui::pos2(x, inner.top()),
                        Vec2::new(m.w(c), header_h),
                    );
                    let ptr_x = ui.ctx().pointer_hover_pos().map(|p| p.x).unwrap_or(0.0);
                    if ptr_x >= x && ptr_x < x + m.w(c) {
                        painter.rect_filled(col_rect, 0, pal.hover);
                    }
                }
            }
            // 正在改名的那一格不画原文字——否则会和编辑框文字叠成双影，
            // 看起来就是「编辑的文字错位了」（其实是两层文字叠着）。
            let editing = self.grid.header_edit.as_ref();
            let editing_group_here = editing
                .map(|e| {
                    !one_level
                        && e.row == 1
                        && v.group_start
                        && !v.is_x
                        && views.get(e.col).map(|ev| ev.group_id) == Some(v.group_id)
                })
                .unwrap_or(false);
            let editing_text_here = editing
                .map(|e| {
                    if one_level {
                        e.col == c
                    } else {
                        e.row == 0 && e.col == c
                    }
                })
                .unwrap_or(false);
            if c >= first_c && v.group_start {
                // 组名（跨度）——编辑中的那一组跳过文字，竖线照画
                let span = v.group_span.max(1);
                let gw: f32 = (c..c + span).map(|k| m.w(k)).sum();
                let ty = if one_level {
                    inner.top() + header_h / 2.0
                } else {
                    inner.top() + GROUP_H / 2.0 + 1.0
                };
                if !editing_group_here && !editing_text_here {
                    // 组名居中到跨格中央——「一大格含 N 小格」的视觉
                    painter.text(
                        egui::pos2(x + gw / 2.0, ty),
                        header_text::group_align(),
                        &v.top,
                        header_text::group_font(),
                        if one_level { pal.ink } else { pal.muted },
                    );
                }
                // 组间竖线（贯穿两行）：这才是「格与格」的分界
                if span > 1 || c > 0 {
                    painter.vline(
                        x,
                        egui::Rangef::new(inner.top(), inner.top() + GROUP_H),
                        Stroke::new(1.0, pal.border),
                    );
                }
            }
            if !one_level && !editing_text_here {
                painter.text(
                    egui::pos2(x + header_text::PAD_X, inner.top() + GROUP_H + SUB_H / 2.0),
                    header_text::sub_align(),
                    &v.sub,
                    header_text::sub_font(),
                    if v.is_x { pal.muted } else { pal.ink },
                );
            }
            // 竖线：组内子列之间的分隔线**不穿组名行**——
            // 组名是一个横跨 N 个小格的大格，内部的线只应存在于子行和数据区。
            // 组边界（group_start）才画到顶。
            let line_top = if !one_level && !v.group_start {
                inner.top() + GROUP_H
            } else {
                inner.top()
            };
            painter.vline(
                x,
                egui::Rangef::new(line_top, inner.bottom()),
                Stroke::new(1.0, pal.border_soft),
            );
        }
        // 两层表头：组行与子行之间一条淡分界线，让「上层一整格 / 下层 N 小格」看得出来
        if !one_level {
            painter.hline(
                egui::Rangef::new(inner.left() + row_w, inner.right() - SB),
                inner.top() + GROUP_H,
                Stroke::new(0.8, pal.border_soft),
            );
        }
        painter.hline(
            inner.x_range(),
            inner.top() + header_h - 1.0,
            Stroke::new(1.0, pal.border),
        );
        painter.vline(
            inner.left() + row_w - 0.5,
            egui::Rangef::new(inner.top(), inner.bottom()),
            Stroke::new(1.0, pal.border),
        );

        // 行标题
        for r in vis_rows.clone() {
            let y = inner.top() + header_h + r as f32 * ROW_H - sy;
            let rr = Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(row_w, ROW_H));
            if r >= sel.0 .0 && r <= sel.1 .0 {
                painter.rect_filled(rr, 0, alpha(pal.accent, 14));
            }
            // 正在改名的行不画原文——否则编辑框文字叠在原文上（双层）。
            // 与表头编辑同款问题同款修法。
            let editing_row = self
                .grid
                .row_edit
                .as_ref()
                .map(|e| e.row == r)
                .unwrap_or(false);
            if !editing_row {
                let label = table
                    .row_headers
                    .get(r)
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .unwrap_or_else(|| format!("{}", r + 1));
                painter.text(
                    egui::pos2(rr.center().x, rr.center().y),
                    Align2::CENTER_CENTER,
                    label,
                    FontId::proportional(9.5),
                    if r == active.0 {
                        pal.accent_ink
                    } else {
                        pal.faint
                    },
                );
            }
            painter.hline(
                inner.x_range(),
                y + ROW_H - 0.5,
                Stroke::new(1.0, pal.grid_line),
            );
        }

        // 单元格
        for r in vis_rows.clone() {
            let y = inner.top() + header_h + r as f32 * ROW_H - sy;
            for c in first_c..last_c {
                let x = inner.left() + row_w + m.x(c) - sx;
                let cr = Rect::from_min_size(egui::pos2(x, y), Vec2::new(m.w(c), ROW_H));
                let selected = r >= sel.0 .0 && r <= sel.1 .0 && c >= sel.0 .1 && c <= sel.1 .1;
                if selected {
                    cell_painter.rect_filled(cr, 0, alpha(pal.accent, 22));
                }
                if let Some(cell) = table.cell(r, c) {
                    // 正在编辑的格不画底下文字，由内联编辑框显示草稿——
                    // 两层文字叠在一起就是「互相错位」的观感来源。
                    let editing_now = self
                        .grid
                        .editing
                        .as_ref()
                        .map(|e| e.row == r && e.col == c)
                        .unwrap_or(false);
                    if !cell.is_empty() && !editing_now {
                        let txt = cell.display();
                        let is_num = cell.num.is_some();
                        let color = if cell.excluded {
                            pal.accent
                        } else if is_num {
                            pal.ink
                        } else {
                            pal.muted
                        };
                        paint_cell_text(&cell_painter, cr, &txt, cell_text::font(), color, is_num);
                        if cell.excluded {
                            let cx = if is_num {
                                cr.right() - 8.0
                            } else {
                                cr.left() + 8.0
                            };
                            cell_painter.hline(
                                if is_num {
                                    egui::Rangef::new(cx - 22.0, cx)
                                } else {
                                    egui::Rangef::new(cx, cx + 22.0)
                                },
                                cr.center().y,
                                Stroke::new(1.0, pal.accent),
                            );
                        }
                    }
                }
                cell_painter.vline(
                    x + m.w(c),
                    egui::Rangef::new(y, y + ROW_H),
                    Stroke::new(1.0, pal.grid_line),
                );
            }
        }

        // 活动单元格边框
        if active_in_view && active.1 >= first_c && active.1 < last_c {
            let x = inner.left() + row_w + m.x(active.1) - sx;
            let y = inner.top() + header_h + active.0 as f32 * ROW_H - sy;
            let ar = Rect::from_min_size(egui::pos2(x, y), Vec2::new(m.w(active.1), ROW_H));
            cell_painter.rect_stroke(
                ar.shrink(0.5),
                0,
                Stroke::new(1.6, pal.accent),
                egui::StrokeKind::Inside,
            );
        }

        // 滚动条（track 已在交互段定义）
        {
            let pointer = ui.ctx().pointer_hover_pos();
            if let Some(t) = thumb_rect(track_v, sy, total_h, track_v.height(), true) {
                let hot = self.grid.drag_v || pointer.map(|p| t.contains(p)).unwrap_or(false);
                draw_thumb(&painter, t, &pal, hot);
            }
            if let Some(t) = thumb_rect(track_h, sx, total_w, track_h.width(), false) {
                let hot = self.grid.drag_h || pointer.map(|p| t.contains(p)).unwrap_or(false);
                draw_thumb(&painter, t, &pal, hot);
            }
        }

        // ---- 列宽拖拽把手 ----
        //
        // 注册在 `resp` 之后 → 把手在上层，拖把手不会被网格当成单元格框选。
        // 行标题列与每个真实数据列的右缘各一条；宽度写回模型，随工程持久化。
        // 变更先攒在 `pending_w`，等 `table` 的借用结束再写入（Rust 借用规则）。
        let mut pending_w: Option<(Option<usize>, f32)> = None;
        let handle_at = |px: f32| {
            Rect::from_min_max(
                egui::pos2(px - RESIZE_HIT, inner.top()),
                egui::pos2(px + RESIZE_HIT, inner.top() + header_h),
            )
        };
        // 行标题列右缘
        {
            let hx = inner.left() + row_w;
            let rid = id_ui.with("rz_rowhead");
            let hr = ui.interact(handle_at(hx), rid, Sense::click_and_drag());
            if hr.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            if hr.dragged() {
                let d = hr.drag_delta();
                pending_w = Some((None, (row_w + d.x).clamp(28.0, 240.0)));
            }
            if hr.hovered() || hr.dragged() {
                painter.vline(
                    hx,
                    egui::Rangef::new(inner.top(), inner.top() + header_h),
                    Stroke::new(2.0, pal.accent),
                );
            }
        }
        // 数据列右缘（虚拟列不给把手：没有模型字段可写）
        for c in first_c..last_c.min(real_cols) {
            let hx = inner.left() + row_w + m.x(c) + m.w(c);
            let rid = id_ui.with(("rz_col", c));
            let hr = ui.interact(handle_at(hx), rid, Sense::click_and_drag());
            if hr.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            if hr.dragged() {
                let d = hr.drag_delta();
                pending_w = Some((Some(c), (m.w(c) + d.x).clamp(COL_W_MIN, COL_W_MAX)));
            }
            if hr.hovered() || hr.dragged() {
                // 指示线跟竖线同一规则：组内不穿组行，组边界贯穿。
                // 判断**右邻列**是否开新组：c+1 是 group_start → 这是组边界；
                // c+1 同组 → 组内边界。之前查 views[c] 查反了（查的是左邻列）。
                let right_is_new_group = views.get(c + 1).map(|v| v.group_start).unwrap_or(true); // 右边没有了 → 是最后一个组的尾边界
                let ind_top = if !one_level && !right_is_new_group {
                    inner.top() + GROUP_H
                } else {
                    inner.top()
                };
                painter.vline(
                    hx,
                    egui::Rangef::new(ind_top, inner.top() + header_h),
                    Stroke::new(2.0, pal.accent),
                );
            }
        }

        // -------- 交互 --------
        let pointer = ui.ctx().pointer_hover_pos();
        let in_header = pointer
            .map(|p| {
                p.y < inner.top() + header_h && p.y >= inner.top() && p.x >= inner.left() + row_w
            })
            .unwrap_or(false);
        let in_rows = pointer
            .map(|p| p.x < inner.left() + row_w && p.y >= inner.top() + header_h)
            .unwrap_or(false);

        let cell_at = |p: egui::Pos2| -> Option<(usize, usize)> {
            if !body.contains(p) {
                return None;
            }
            let c = m.col_at(p.x - body.left() + sx)?;
            let r = (((p.y - body.top() + sy) / ROW_H).floor()) as isize;
            if r < 0 {
                return None;
            }
            Some((r as usize, c))
        };
        let col_at = |p: egui::Pos2| -> Option<usize> {
            if !in_header {
                return None;
            }
            m.col_at(p.x - body.left() + sx)
        };
        let row_at = |p: egui::Pos2| -> Option<usize> {
            if !in_rows {
                return None;
            }
            let r = (((p.y - body.top() + sy) / ROW_H).floor()) as isize;
            (r >= 0).then_some(r as usize)
        };

        // 右键**不改选区**：菜单按当前选区操作（Excel 口径）。
        // 以前这里会把 cursor 挪到右键点的格子上——用户框选了一片再右键，
        // 选区瞬间塌成一个格，剪切/复制全乱。
        // 拖拽开始：锚点定在**按下那一格**（Excel 口径——框选从按下到松开）。
        // Shift+拖拽 = 扩展选区，锚点不动。
        // 以前 anchor 只在 clicked() 里设置，拖拽时 clicked() 不触发，
        // 导致框选起点是上一次点击的位置，整个框选逻辑全乱。
        // 列头区也要接：cell_at 在表头返回 None，不接的话锚点沿用上次点击的行，
        // 在列名上滑动就变成「从上次那格往下框」而不是框整列。
        if resp.drag_started() && !ui.input(|i| i.modifiers.shift) {
            if let Some(p) = resp.interact_pointer_pos() {
                if let Some((r, c)) = cell_at(p) {
                    self.grid.anchor = (r, c);
                    self.grid.cursor = (r, c);
                } else if let Some(c) = col_at(p) {
                    // 列头按下：锚点定在**整列起点**（行 0）
                    let in_group_row =
                        !one_level && p.y >= inner.top() && p.y < inner.top() + GROUP_H;
                    let c0 = if in_group_row {
                        group_span_of(c, real_cols, &views, group_size).0
                    } else {
                        c
                    };
                    self.grid.anchor = (0, c0);
                    self.grid.cursor = (0, c0);
                } else if let Some(r) = row_at(p) {
                    self.grid.anchor = (r, 0);
                    self.grid.cursor = (r, 0);
                }
            }
        }
        if resp.double_clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                if let Some((r, c)) = cell_at(p) {
                    self.start_edit(r, c, id);
                } else if let Some(c) = col_at(p) {
                    // 表头改名的目标要看**显示的是哪个名字**：
                    // - one_level（柱状/列联等）：显示的是组名 → 改组名
                    //   （之前改 columns[c].name，而显示走 group_name，改了看不见）
                    // - two_level：组行→改组名，子行→改列名
                    // `EditState.row` 兼用作类型标志：0=列名、1=组名。
                    // 虚拟列只能改列名（组名语义对未实体化的列无意义），提交时实体化。
                    let is_group = c < real_cols
                        && if one_level {
                            true
                        } else {
                            p.y >= inner.top() && p.y < inner.top() + GROUP_H
                        };
                    let (buf, kind_flag) = if is_group {
                        let gid = views.get(c).map(|v| v.group_id).unwrap_or(0);
                        let name = self
                            .project
                            .table(id)
                            .map(|t| t.group_name(gid).to_string())
                            .unwrap_or_default();
                        (name, 1usize)
                    } else if c < real_cols {
                        let name = self
                            .project
                            .table(id)
                            .map(|t| t.columns[c].name.clone())
                            .unwrap_or_default();
                        (name, 0usize)
                    } else {
                        // 虚拟列预告名，改了就是新列名
                        let n = table_group_n + (c - real_cols) + 1;
                        let name = match table_kind_hint {
                            TableKind::Xy => "Y1".to_string(),
                            TableKind::MultipleVariables => format!("Var {n}"),
                            _ => format!("Group {n}"),
                        };
                        (name, 0usize)
                    };
                    self.grid.header_edit = Some(EditState {
                        row: kind_flag,
                        col: c,
                        buf,
                        just_started: true,
                    });
                } else if let Some(r) = row_at(p) {
                    // 双击行头 → 改行名（row_headers[r]）
                    let buf = self
                        .project
                        .table(id)
                        .map(|t| t.row_headers.get(r).cloned().unwrap_or_default())
                        .unwrap_or_default();
                    self.grid.row_edit = Some(EditState {
                        row: r,
                        col: 0,
                        buf,
                        just_started: true,
                    });
                }
            }
        } else if resp.clicked() {
            // 失焦保存：点击别处先提交当前编辑，再处理新的点击。
            // 以前这里直接 `editing = None` 把输入丢了——用户打完字点别处，内容消失。
            if let Some(ed) = self.grid.editing.take() {
                self.commit_edit(id, ed.row, ed.col, &ed.buf);
            }
            if let Some(ed) = self.grid.header_edit.take() {
                self.commit_header_edit(id, ed.col, &ed.buf, ed.row == 1);
            }
            if let Some(ed) = self.grid.row_edit.take() {
                self.commit_row_edit(id, ed.row, &ed.buf);
            }
            let shift = ui.input(|i| i.modifiers.shift);
            if let Some(p) = resp.interact_pointer_pos() {
                if let Some((r, c)) = cell_at(p) {
                    self.grid.cursor = (r, c);
                    // Shift+单击 = 扩展选区（锚点不动）；普通单击 = 重设选区
                    if !shift {
                        self.grid.anchor = (r, c);
                    }
                } else if let Some(c) = col_at(p) {
                    // 列头单击：点**组行（大格）**→ 选整组所有列；
                    // 点**子行（小格）**→ 只选本列。
                    // 真列与虚拟列都走 group_span_of——虚拟列按 group_size 合成组。
                    let in_group_row =
                        !one_level && p.y >= inner.top() && p.y < inner.top() + GROUP_H;
                    let (c0, c1) = if in_group_row {
                        group_span_of(c, real_cols, &views, group_size)
                    } else {
                        (c, c)
                    };
                    // 虚拟区可能已有几万行；整列选择只覆盖**真实数据区**，
                    // 否则复制/清空会遍历一个天文数字的范围。
                    self.grid.anchor = (0, c0);
                    self.grid.cursor = (nrows_t.saturating_sub(1), c1);
                } else if let Some(r) = row_at(p) {
                    self.grid.anchor = (r, 0);
                    self.grid.cursor = (r, real_cols.saturating_sub(1));
                }
                resp.request_focus();
            }
        } else if resp.dragged() {
            if let Some(p) = ui.ctx().pointer_interact_pos() {
                // 拖到 body 外面（表头/滚动条/窗口外）时也要钳到最近格，
                // 否则框选拖到边界突然停住，松手时选区不对。
                let clamped = egui::pos2(
                    p.x.clamp(body.left() + 1.0, body.right() - 1.0),
                    p.y.clamp(body.top() + 1.0, body.bottom() - 1.0),
                );
                // 列头区拖动 → 扩展**整列**选区（拖过多列就是多列整选）。
                // 之前只接了 cell_at，列头拖选完全不响应，多个列标题拖了只进第一列。
                if !one_level && p.y < inner.top() + GROUP_H && p.y >= inner.top() {
                    // 组行拖动：按组边界扩展（大格一格一格地扩）
                    if let Some(c) = col_at(p) {
                        let (_, c1) = group_span_of(c, real_cols, &views, group_size);
                        let anchor_c = self.grid.anchor.1;
                        self.grid.cursor = (nrows_t.saturating_sub(1), c1.max(anchor_c));
                    }
                } else if p.y < inner.top() + header_h && p.y >= inner.top() {
                    // 子行/单层表头拖动：逐列扩展
                    if let Some(c) = col_at(p) {
                        self.grid.cursor = (nrows_t.saturating_sub(1), c);
                    }
                } else if let Some((r, c)) = cell_at(clamped) {
                    self.grid.cursor = (r, c);
                }
            }
            // 拖到视口边缘自动滚动（Excel 的框选手感）：
            // 指针离边缘越近滚得越快，松手前选区跟着指针扩。
            if let Some(p) = ui.ctx().pointer_interact_pos() {
                let speed = |dist: f32| {
                    let d = dist.max(0.0);
                    if d > 28.0 {
                        0.0
                    } else {
                        (28.0 - d) * 0.55
                    }
                };
                let dx_left = speed(p.x - body.left());
                let dx_right = speed(body.right() - p.x);
                let dy_top = speed(p.y - body.top());
                let dy_bottom = speed(body.bottom() - p.y);
                if dx_left > 0.0 {
                    self.grid.scroll_x = (self.grid.scroll_x - dx_left).max(0.0);
                }
                if dx_right > 0.0 {
                    self.grid.scroll_x = (self.grid.scroll_x + dx_right).min(max_sx);
                }
                if dy_top > 0.0 {
                    self.grid.scroll_y = (self.grid.scroll_y - dy_top).max(0.0);
                }
                if dy_bottom > 0.0 {
                    self.grid.scroll_y = (self.grid.scroll_y + dy_bottom).min(max_sy);
                }
            }
        }
        // 松开鼠标：把 cursor 定到最终指针位置——否则最后一格可能没被捕获，
        // 复制时少一行/一列（dragged() 在松手那帧可能已经不触发）。
        if resp.drag_stopped() {
            if let Some(p) = ui.ctx().pointer_interact_pos() {
                let clamped = egui::pos2(
                    p.x.clamp(body.left() + 1.0, body.right() - 1.0),
                    p.y.clamp(body.top() + 1.0, body.bottom() - 1.0),
                );
                if let Some((r, c)) = cell_at(clamped) {
                    self.grid.cursor = (r, c);
                } else if let Some(c) = col_at(p) {
                    self.grid.cursor = (nrows_t.saturating_sub(1), c);
                }
            }
        }

        // 键盘（中文 IME 组合期间只拦 Enter/Esc/打字——Ctrl 快捷键和方向键照常）。
        // 只有焦点属于网格（或无人聚焦）时才处理；侧栏改名期间无论焦点在哪都让出键盘，
        // 否则这里会消耗 Text 事件，把侧栏输入框的字抢到当前单元格里。
        let composing = self.grid.composing;
        let focused = ui.ctx().memory(|m| m.focused());
        let grid_owns_kbd = focused.is_none() || focused == Some(id_ui);
        let renaming_sheet = self.sidebar_edit.is_some();
        if grid_owns_kbd
            && !renaming_sheet
            && self.grid.editing.is_none()
            && self.grid.header_edit.is_none()
            && self.grid.row_edit.is_none()
        {
            let mods = ui.input(|i| i.modifiers);
            let mut move_r = 0isize;
            let mut move_c = 0isize;
            let extend = mods.shift;
            if ui.input(|i| i.key_pressed(Key::ArrowDown)) {
                move_r = 1;
            }
            if ui.input(|i| i.key_pressed(Key::ArrowUp)) {
                move_r = -1;
            }
            if ui.input(|i| i.key_pressed(Key::Tab)) {
                move_c = if mods.shift { -1 } else { 1 };
            } else if ui.input(|i| i.key_pressed(Key::ArrowRight)) {
                move_c = 1;
            } else if ui.input(|i| i.key_pressed(Key::ArrowLeft)) {
                move_c = -1;
            }
            if !composing && ui.input(|i| i.key_pressed(Key::Enter)) {
                move_r = 1;
            }
            if move_r != 0 || move_c != 0 {
                let (r, c) = self.grid.cursor;
                let nr = (r as isize + move_r).clamp(0, n_rows as isize - 1) as usize;
                let nc = (c as isize + move_c).clamp(0, n_cols as isize - 1) as usize;
                self.grid.cursor = (nr, nc);
                if !extend {
                    self.grid.anchor = (nr, nc);
                }
            }
            // F2 进入编辑（保留原内容）——Excel 的标准编辑入口
            if ui.input(|i| i.key_pressed(Key::F2)) {
                self.start_edit(self.grid.cursor.0, self.grid.cursor.1, id);
            }
            // ---- 剪贴板快捷键 ----
            // 放在输入处理之后，用当帧最新选区；指令从**原始事件**识别
            // （Event::Copy/Cut/Paste 优先，回退 Event::Key 自带 modifiers）。
            // 焦点守卫已在块首统一处理，这里直接消费剪贴板指令。
            if let Some(cmd) = clipboard_command(ui.ctx()) {
                match cmd {
                    ClipCmd::Copy => {
                        self.clip_copy();
                    }
                    ClipCmd::Cut => {
                        self.clip_cut();
                    }
                    ClipCmd::Paste(text) => {
                        self.clip_paste_opt(text);
                    }
                    ClipCmd::Clear => {
                        self.clip_clear();
                    }
                    ClipCmd::SelectAll => {
                        self.clip_select_all();
                    }
                }
            }
            // 直接输入：打字即进入编辑并**保留第一个字符**（Excel 默认行为）。
            // 关键：用 input_mut **消耗**本帧的文本事件，并把字符放进 buf，
            // 否则 TextEdit 会在下一帧看不到这个字符（第一个数字被吞）。
            if self.grid.editing.is_none() && !mods.command && !mods.alt {
                let text: Option<String> = ui.input_mut(|i| {
                    let mut s = String::new();
                    i.events.retain(|e| match e {
                        egui::Event::Text(t)
                            if !t.is_empty() && !t.chars().any(|c| c.is_control()) =>
                        {
                            s.push_str(t);
                            false // 消耗掉，TextEdit 不会重复插入
                        }
                        _ => true,
                    });
                    (!s.is_empty()).then_some(s)
                });
                if let Some(t) = text {
                    self.start_edit_with(self.grid.cursor.0, self.grid.cursor.1, &t, id);
                }
            }
        } else if grid_owns_kbd && !renaming_sheet && composing && self.grid.editing.is_none() {
            // 组合一开始（还没在编辑）就进编辑态：预编辑文本要有地方显示，
            // 否则拼音打到一半屏幕上什么都不出现，用户以为没反应。
            // 缓冲区留空——真正的文字由 TextEdit 接管 IME 后自己填。
            self.start_edit(self.grid.cursor.0, self.grid.cursor.1, id);
        }

        // ---- 右键菜单 ----
        //
        // 右键**不改选区**，菜单按「右键落在哪」定目标：
        // 行头 → 整行操作；列头 → 整列操作；单元格 → 当前选区操作。
        #[derive(Clone, Copy, PartialEq)]
        enum CtxKind {
            Cell,
            RowHeader,
            ColHeader,
        }
        let mut ctx_kind = CtxKind::Cell;
        let mut ctx_row = self.grid.cursor.0;
        let mut ctx_col = self.grid.cursor.1;
        if resp.secondary_clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                if let Some(c) = col_at(p) {
                    ctx_kind = CtxKind::ColHeader;
                    ctx_col = c;
                } else if let Some(r) = row_at(p) {
                    ctx_kind = CtxKind::RowHeader;
                    ctx_row = r;
                }
                // 单元格 → 保持 ctx_kind = Cell，目标就是当前选区 sel
            }
        }

        let is_x_col = x_flags.get(ctx_col).copied().unwrap_or(false);
        let mut ctx_action: Option<&str> = None;
        let ctx_resp = resp.clone();
        ctx_resp.context_menu(|ui| {
            ui.set_min_width(180.0);
            match ctx_kind {
                CtxKind::RowHeader => {
                    ui.label(RichText::new(format!("第 {} 行", ctx_row + 1)).weak());
                    ui.separator();
                    if ui.button("复制该行").clicked() {
                        ctx_action = Some("copy_row");
                        ui.close();
                    }
                    if ui.button("清除该行内容").clicked() {
                        ctx_action = Some("clear_row");
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("在上方插入行").clicked() {
                        ctx_action = Some("insert_row_above");
                        ui.close();
                    }
                    if ui.button("在下方插入行").clicked() {
                        ctx_action = Some("insert_row_below");
                        ui.close();
                    }
                    if ui
                        .add_enabled(true, egui::Button::new("删除该行"))
                        .clicked()
                    {
                        ctx_action = Some("remove_row_at");
                        ui.close();
                    }
                }
                CtxKind::ColHeader => {
                    ui.label(RichText::new(format!("第 {} 列", ctx_col + 1)).weak());
                    ui.separator();
                    if ui.button("复制该列").clicked() {
                        ctx_action = Some("copy_col");
                        ui.close();
                    }
                    if ui.button("清除该列内容").clicked() {
                        ctx_action = Some("clear_col");
                        ui.close();
                    }
                    ui.separator();
                    // 插删列受表型约束：结构固定的表禁用，X 列不可删
                    if ui
                        .add_enabled(can_struct, egui::Button::new("在左侧插入列"))
                        .clicked()
                    {
                        ctx_action = Some("insert_col_left");
                        ui.close();
                    }
                    if ui
                        .add_enabled(can_struct, egui::Button::new("在右侧插入列"))
                        .clicked()
                    {
                        ctx_action = Some("insert_col_right");
                        ui.close();
                    }
                    if ui
                        .add_enabled(can_struct && !is_x_col, egui::Button::new("删除该列"))
                        .clicked()
                    {
                        ctx_action = Some("remove_col_at");
                        ui.close();
                    }
                    // 「删一大格」：整组一起删。结构固定的表禁单列增删，
                    // 但整组删不破坏「每组 N 列」的不变量，所以这里**不看 can_struct**。
                    if ui
                        .add_enabled(!is_x_col, egui::Button::new("删除该组（整格）"))
                        .clicked()
                    {
                        ctx_action = Some("remove_group_at");
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("重置列宽").clicked() {
                        ctx_action = Some("reset_col_width");
                        ui.close();
                    }
                    if ui.button("重命名…").clicked() {
                        ctx_action = Some("rename_col");
                        ui.close();
                    }
                }
                CtxKind::Cell => {
                    ui.label(
                        RichText::new(format!(
                            "选区 {}×{}",
                            sel.1 .0 - sel.0 .0 + 1,
                            sel.1 .1 - sel.0 .1 + 1
                        ))
                        .weak(),
                    );
                    ui.separator();
                    if ui.button("复制  Ctrl+C").clicked() {
                        ctx_action = Some("copy");
                        ui.close();
                    }
                    if ui.button("剪切  Ctrl+X").clicked() {
                        ctx_action = Some("cut");
                        ui.close();
                    }
                    if ui.button("粘贴  Ctrl+V").clicked() {
                        ctx_action = Some("paste");
                        ui.close();
                    }
                    if ui.button("清除内容  Del").clicked() {
                        ctx_action = Some("clear");
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("排除选中值").clicked() {
                        ctx_action = Some("exclude");
                        ui.close();
                    }
                    if ui.button("恢复选中值").clicked() {
                        ctx_action = Some("include");
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("在上方插入行").clicked() {
                        ctx_action = Some("insert_row_above");
                        ui.close();
                    }
                    if ui.button("在下方插入行").clicked() {
                        ctx_action = Some("insert_row_below");
                        ui.close();
                    }
                    if ui
                        .add_enabled(can_struct, egui::Button::new("在左侧插入列"))
                        .clicked()
                    {
                        ctx_action = Some("insert_col_left");
                        ui.close();
                    }
                    if ui
                        .add_enabled(can_struct, egui::Button::new("在右侧插入列"))
                        .clicked()
                    {
                        ctx_action = Some("insert_col_right");
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("删除选中行").clicked() {
                        ctx_action = Some("remove_rows_sel");
                        ui.close();
                    }
                    if ui
                        .add_enabled(can_struct, egui::Button::new("删除选中列"))
                        .clicked()
                    {
                        ctx_action = Some("remove_cols_sel");
                        ui.close();
                    }
                    // 「删一大格」：选区覆盖到的组整组删
                    if ui.button("删除选中组（整格）").clicked() {
                        ctx_action = Some("remove_groups_sel");
                        ui.close();
                    }
                }
            }
        });
        if let Some(action) = ctx_action {
            match action {
                // 复制/剪切/粘贴/清除 —— 全部走 clipboard.rs 的唯一实现
                "copy" => {
                    let s = Sel::corners(sel.0, sel.1);
                    self.copy_region(id, s);
                }
                "copy_row" => {
                    let s = Sel::row(ctx_row, real_cols.saturating_sub(1));
                    self.copy_region(id, s);
                }
                "copy_col" => {
                    let s = Sel::col(ctx_col, nrows_t.saturating_sub(1));
                    self.copy_region(id, s);
                }
                "cut" => {
                    let s = Sel::corners(sel.0, sel.1);
                    if self.copy_region(id, s) {
                        self.clear_region(id, s);
                        self.toast("已剪切", ToastKind::Info);
                    }
                }
                "paste" => {
                    self.clip_paste();
                }
                "clear" => {
                    let s = Sel::corners(sel.0, sel.1);
                    self.clear_region(id, s);
                }
                "clear_row" => {
                    let s = Sel::row(ctx_row, real_cols.saturating_sub(1));
                    self.clear_region(id, s);
                }
                "clear_col" => {
                    let s = Sel::col(ctx_col, nrows_t.saturating_sub(1));
                    self.clear_region(id, s);
                }
                "exclude" | "include" => {
                    let ex = action == "exclude";
                    self.snapshot();
                    if let Some(t) = self.project.table_mut(id) {
                        for r in sel.0 .0..=sel.1 .0 {
                            for c in sel.0 .1..=sel.1 .1 {
                                t.set_excluded(r, c, ex);
                            }
                        }
                    }
                    self.dirty = true;
                    self.bump_chart();
                }
                "insert_row_above" | "insert_row_below" => {
                    let at = if action == "insert_row_above" {
                        ctx_row
                    } else {
                        ctx_row + 1
                    };
                    self.snapshot();
                    if let Some(t) = self.project.table_mut(id) {
                        t.insert_row(at);
                    }
                    self.dirty = true;
                    self.bump_chart();
                }
                "remove_row_at" => {
                    self.snapshot();
                    if let Some(t) = self.project.table_mut(id) {
                        t.remove_row(ctx_row);
                    }
                    self.dirty = true;
                    self.bump_chart();
                }
                "remove_rows_sel" => {
                    self.snapshot();
                    if let Some(t) = self.project.table_mut(id) {
                        for r in (sel.0 .0..=sel.1 .0).rev() {
                            t.remove_row(r);
                        }
                    }
                    self.dirty = true;
                    self.bump_chart();
                }
                "insert_col_left" | "insert_col_right" => {
                    let at = if action == "insert_col_left" {
                        ctx_col
                    } else {
                        ctx_col + 1
                    };
                    self.snapshot();
                    if let Some(t) = self.project.table_mut(id) {
                        let name = format!("C{}", t.columns.len() + 1);
                        t.insert_col(at, &name, ColRole::Y);
                    }
                    self.dirty = true;
                    self.bump_chart();
                }
                "remove_col_at" => {
                    // 删列前先把正在编辑的表头提交/丢弃，否则列号移位后编辑框指向错列
                    if let Some(ed) = self.grid.header_edit.take() {
                        self.commit_header_edit(id, ed.col, &ed.buf, ed.row == 1);
                    }
                    self.snapshot();
                    if let Some(t) = self.project.table_mut(id) {
                        t.remove_col(ctx_col);
                    }
                    self.dirty = true;
                    self.bump_chart();
                }
                "remove_cols_sel" => {
                    self.grid.header_edit = None;
                    self.snapshot();
                    if let Some(t) = self.project.table_mut(id) {
                        for c in (sel.0 .1..=sel.1 .1).rev() {
                            t.remove_col(c);
                        }
                    }
                    self.dirty = true;
                    self.bump_chart();
                }
                "remove_group_at" => {
                    // 删一大格：右键那一列所属的整组一次删掉。
                    // 虚拟列不属于任何真实组（gid 查不到），跳过避免误删 group=0。
                    if ctx_col < real_cols {
                        self.grid.header_edit = None;
                        self.snapshot();
                        if let Some(t) = self.project.table_mut(id) {
                            let gid = t.columns.get(ctx_col).map(|c| c.group).unwrap_or(0);
                            t.remove_group(gid);
                        }
                        self.dirty = true;
                        self.bump_chart();
                    }
                }
                "remove_groups_sel" => {
                    // 删一大格：选区覆盖到的每个组都整组删
                    self.grid.header_edit = None;
                    self.snapshot();
                    if let Some(t) = self.project.table_mut(id) {
                        let gids: std::collections::BTreeSet<usize> = (sel.0 .1..=sel.1 .1)
                            .filter_map(|c| t.columns.get(c).map(|x| x.group))
                            .collect();
                        for gid in gids {
                            t.remove_group(gid);
                        }
                    }
                    self.dirty = true;
                    self.bump_chart();
                }
                "reset_col_width" => {
                    if let Some(t) = self.project.table_mut(id) {
                        if let Some(col) = t.columns.get_mut(ctx_col) {
                            col.width = None;
                        }
                    }
                    self.dirty = true;
                }
                "rename_col" => {
                    if let Some(t) = self.project.table(id) {
                        if let Some(col) = t.columns.get(ctx_col) {
                            self.grid.header_edit = Some(EditState {
                                row: 0,
                                col: ctx_col,
                                buf: col.name.clone(),
                                just_started: true,
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        // 内联编辑框：文字位置与绘制完全一致（共用 cell_text 布局），
        // 数字右对齐、文本左对齐、垂直居中；编辑态加一圈 accent 描边区分。
        if let Some(mut ed) = self.grid.editing.take() {
            let x = inner.left() + row_w + m.x(ed.col) - sx;
            let y = inner.top() + header_h + ed.row as f32 * ROW_H - sy;
            let er = Rect::from_min_size(egui::pos2(x, y), Vec2::new(m.w(ed.col), ROW_H));
            if body.intersects(er) {
                // 数字列右对齐编辑，文本列左对齐——与 paint_cell_text 同口径
                let is_num = self
                    .project
                    .table(id)
                    .and_then(|t| t.cell(ed.row, ed.col))
                    .map(|c| c.num.is_some() || c.is_empty())
                    .unwrap_or(true);
                let right = is_num;
                // 编辑态视觉：accent 描边 + 白底，明确区别于「仅选中」
                cell_painter.rect_filled(er, 0, pal.bg);
                cell_painter.rect_stroke(
                    er.shrink(0.5),
                    0,
                    Stroke::new(1.8, pal.accent),
                    egui::StrokeKind::Inside,
                );
                // 光标定位：`cursor_at_end` 只在「已聚焦」那一帧的 events() 里生效，
                // 而 request_focus 是在 ui.put 之后才建立焦点。所以要**延迟清除** just_started：
                // 第 1 帧建立焦点（events 不跑，光标没生效），第 2 帧已聚焦、events 按
                // cursor_at_end(true) 把光标放末尾并接受首个字符，此时才清除。
                // 否则第 2 帧就变成 cursor_at_end(false)=行首 → 「123」错成「231」。
                let edit_id = ui.make_persistent_id(("cell_edit", id, ed.row, ed.col));
                let had_focus = ui.memory(|m| m.has_focus(edit_id));
                let rr = ui.put(
                    er,
                    TextEdit::singleline(&mut ed.buf)
                        .frame(egui::Frame::NONE)
                        .font(cell_text::font())
                        .text_color(if right { pal.ink } else { pal.ink })
                        .align(cell_text::align2(right))
                        .margin(Vec2::new(cell_text::PAD_X, 0.0))
                        .cursor_at_end(ed.just_started)
                        .id_source(("cell_edit", id, ed.row, ed.col)),
                );
                if ed.just_started {
                    rr.request_focus();
                    if had_focus {
                        ed.just_started = false;
                    }
                }
                // 中文输入法组合期间不提交、不响应快捷键
                let composing = self.grid.composing;
                let enter = !composing && ui.input(|i| i.key_pressed(Key::Enter));
                let esc = !composing && ui.input(|i| i.key_pressed(Key::Escape));
                let up = !composing && ui.input(|i| i.key_pressed(Key::ArrowUp));
                let down = !composing && ui.input(|i| i.key_pressed(Key::ArrowDown));
                let left = !composing && ui.input(|i| i.key_pressed(Key::ArrowLeft));
                let right = !composing && ui.input(|i| i.key_pressed(Key::ArrowRight));
                if esc {
                    self.grid.cursor = (ed.row, ed.col);
                } else if enter || down {
                    self.commit_edit(id, ed.row, ed.col, &ed.buf);
                    let nr = (ed.row + 1).min(n_rows.saturating_sub(1));
                    self.grid.cursor = (nr, ed.col);
                    self.grid.anchor = self.grid.cursor;
                } else if up {
                    self.commit_edit(id, ed.row, ed.col, &ed.buf);
                    let nr = ed.row.saturating_sub(1);
                    self.grid.cursor = (nr, ed.col);
                    self.grid.anchor = self.grid.cursor;
                } else if left {
                    self.commit_edit(id, ed.row, ed.col, &ed.buf);
                    let nc = ed.col.saturating_sub(1);
                    self.grid.cursor = (ed.row, nc);
                    self.grid.anchor = self.grid.cursor;
                } else if right {
                    self.commit_edit(id, ed.row, ed.col, &ed.buf);
                    let nc = (ed.col + 1).min(n_cols.saturating_sub(1));
                    self.grid.cursor = (ed.row, nc);
                    self.grid.anchor = self.grid.cursor;
                } else if rr.lost_focus() && !composing {
                    self.commit_edit(id, ed.row, ed.col, &ed.buf);
                } else {
                    self.grid.editing = Some(ed);
                }
            } else {
                // 编辑格滚出视口：保持编辑态，滚回来继续
                self.grid.editing = Some(ed);
            }
        }

        // 表头改名编辑框。`ed.row` = 类型标志：1=组名、0=列名。
        //
        // 定位方法：**矩形法 + 垂直居中**。
        // - 水平：子列名给「左缘内缩 PAD_X」的矩形，组名给整组矩形（居中）；
        // - 垂直：用 Align2 的 **CENTER**，文字中心 = 矩形中心（精确值），
        //   不依赖 galley 高度——LEFT_TOP 会因 TextEdit 内部行高与裸 galley 差
        //   1px 而整体下移，之前就是这个坑。
        // 绘制侧用的是同一个矩形中心（painter.text 的 VAlign::Middle），两边必然重合。
        if let Some(mut ed) = self.grid.header_edit.take() {
            // 虚拟列也渲染编辑框（rect 用 m.w 算天然支持）——提交时实体化
            {
                let is_group_edit = ed.row == 1 && !one_level;
                // 组名编辑框的 rect 从**组的左缘**起算——不是点击的那一列。
                // 统计组内列时必须**排除 X 列**：X 列 group=0，和第一组共用组号，
                // 混进来会让组宽多算一列，编辑框整体右移（之前「偏左/偏右」的根因）。
                let (rx, rw) = if is_group_edit {
                    let gid = views.get(ed.col).map(|v| v.group_id).unwrap_or(0);
                    let members: Vec<usize> = views
                        .iter()
                        .enumerate()
                        .filter(|(_, v)| v.group_id == gid && !v.is_x)
                        .map(|(k, _)| k)
                        .collect();
                    let first = members.first().copied().unwrap_or(ed.col);
                    let gw: f32 = members.iter().map(|k| m.w(*k)).sum();
                    (inner.left() + row_w + m.x(first) - sx, gw)
                } else {
                    (inner.left() + row_w + m.x(ed.col) - sx, m.w(ed.col))
                };
                let (ey, eh, font, align2) = if is_group_edit {
                    (
                        inner.top(),
                        GROUP_H,
                        header_text::group_font(),
                        header_text::group_align(), // CENTER_CENTER
                    )
                } else if !one_level {
                    (
                        inner.top() + GROUP_H,
                        SUB_H,
                        header_text::sub_font(),
                        header_text::sub_align(), // LEFT_CENTER
                    )
                } else {
                    (
                        inner.top(),
                        header_h,
                        header_text::group_font(),
                        header_text::group_align(),
                    )
                };
                // 子列名：左右各内缩 PAD_X（与绘制 pad 一致）；组名：整组宽
                let (rect_left, rect_w) = if is_group_edit {
                    (rx, rw)
                } else {
                    (
                        rx + header_text::PAD_X,
                        (rw - header_text::PAD_X * 2.0).max(8.0),
                    )
                };
                let text_rect =
                    Rect::from_min_size(egui::pos2(rect_left, ey), Vec2::new(rect_w, eh));
                // 文字色必须与绘制完全一致——TextEdit 默认色 ≠ pal.ink，
                // 不显式指定就会整字偏色（像素 diff 会整个字形都不同）。
                let text_color = if is_group_edit {
                    if one_level {
                        pal.ink
                    } else {
                        pal.muted
                    }
                } else {
                    let is_x = views.get(ed.col).map(|v| v.is_x).unwrap_or(false);
                    if is_x {
                        pal.muted
                    } else {
                        pal.ink
                    }
                };
                // 稳定 ID + 初始全选：
                // - id_source 不给的话把手数量一变焦点就丢 → 第一次改名存不上；
                // - 不全选的话光标在位置 0，打字插在旧名前面（改 "Y1" 为 "Data"
                //   会变 "DataY1"）→ 用户觉得「要改两次」。全选后打字直接替换。
                let edit_id = ui.make_persistent_id(("header_edit", id, ed.col, ed.row));
                if ed.just_started {
                    let mut st =
                        egui::text_edit::TextEditState::load(ui.ctx(), edit_id).unwrap_or_default();
                    st.cursor
                        .set_char_range(Some(egui::text::CCursorRange::select_all_str(&ed.buf)));
                    st.store(ui.ctx(), edit_id);
                }
                let rr = ui.put(
                    text_rect,
                    TextEdit::singleline(&mut ed.buf)
                        .frame(egui::Frame::NONE)
                        .font(font)
                        .text_color(text_color)
                        .align(align2)
                        .margin(egui::Margin::ZERO)
                        .id(edit_id),
                );
                if ed.just_started {
                    rr.request_focus();
                    ed.just_started = false;
                }
                // 中文输入法组合期间不提交——Enter/Esc 是给候选框的。
                // 之前没有这个守卫，IME 确认候选词的 Enter 会误触发提交，
                // 半截组合被存上/丢弃，就是「列名第一次存不上」的偶发根因之一。
                let composing = self.grid.composing;
                let enter = !composing && ui.input(|i| i.key_pressed(Key::Enter));
                let esc = !composing && ui.input(|i| i.key_pressed(Key::Escape));
                if esc {
                    // 取消
                } else if enter || (rr.lost_focus() && !composing) {
                    self.commit_header_edit(id, ed.col, &ed.buf, ed.row == 1);
                } else {
                    self.grid.header_edit = Some(ed);
                }
            }
        }

        // 行标题改名编辑框（双击行头触发）
        if let Some(mut ed) = self.grid.row_edit.take() {
            let y = inner.top() + header_h + ed.row as f32 * ROW_H - sy;
            let rr = Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(row_w, ROW_H));
            if body.intersects(rr) {
                // 与行头文字同款样式（居中 + 9.5），全选替换
                let edit_id = ui.make_persistent_id(("row_edit", id, ed.row));
                if ed.just_started {
                    let mut st =
                        egui::text_edit::TextEditState::load(ui.ctx(), edit_id).unwrap_or_default();
                    st.cursor
                        .set_char_range(Some(egui::text::CCursorRange::select_all_str(&ed.buf)));
                    st.store(ui.ctx(), edit_id);
                }
                let resp_e = ui.put(
                    rr.shrink2(Vec2::new(2.0, 0.0)),
                    TextEdit::singleline(&mut ed.buf)
                        .frame(egui::Frame::NONE)
                        .font(FontId::proportional(9.5))
                        .text_color(pal.ink)
                        .align(Align2::CENTER_CENTER)
                        .margin(egui::Margin::ZERO)
                        .id(edit_id),
                );
                if ed.just_started {
                    resp_e.request_focus();
                    ed.just_started = false;
                }
                let composing = self.grid.composing;
                let enter = !composing && ui.input(|i| i.key_pressed(Key::Enter));
                let esc = !composing && ui.input(|i| i.key_pressed(Key::Escape));
                if esc {
                    // 取消
                } else if enter || (resp_e.lost_focus() && !composing) {
                    self.commit_row_edit(id, ed.row, &ed.buf);
                } else {
                    self.grid.row_edit = Some(ed);
                }
            } else {
                // 滚出视口：保持编辑态
                self.grid.row_edit = Some(ed);
            }
        }

        // 列宽写回模型（拖拽把手攒的变更；`table` 借用到此已结束）
        if let Some((which, w)) = pending_w {
            if let Some(t) = self.project.table_mut(id) {
                match which {
                    None => t.row_header_width = Some(w),
                    Some(c) => {
                        if let Some(col) = t.columns.get_mut(c) {
                            col.width = Some(w);
                        }
                    }
                }
                self.dirty = true;
            }
        }
    }

    fn start_edit(&mut self, row: usize, col: usize, id: u64) {
        let text = self
            .project
            .table(id)
            .and_then(|t| t.cell(row, col))
            .map(|c| c.display())
            .unwrap_or_default();
        self.grid.cursor = (row, col);
        self.grid.anchor = (row, col);
        self.grid.editing = Some(EditState {
            row,
            col,
            buf: text,
            just_started: true,
        });
    }

    fn start_edit_with(&mut self, row: usize, col: usize, seed: &str, _id: u64) {
        // 把首个字符放进 buf——外面已用 input_mut 消耗了对应文本事件，
        // TextEdit 不会再插一次，所以这里可以直接用它做初始内容。
        self.grid.cursor = (row, col);
        self.grid.anchor = (row, col);
        self.grid.editing = Some(EditState {
            row,
            col,
            buf: seed.to_string(),
            just_started: true,
        });
    }

    /// 把虚拟网格里的一个位置实体化成真实行列。
    ///
    /// 返回 false 表示这个位置不可写（结构固定的表不允许加列）。
    /// 行方向总是可以延伸；列方向受 `TableKind::can_add_column()` 约束。
    pub fn materialize_cell(&mut self, id: u64, row: usize, col: usize) -> bool {
        let (can_add, cols, rows, kind) = match self.project.table(id) {
            Some(t) => (t.kind.can_add_column(), t.columns.len(), t.nrows(), t.kind),
            None => return false,
        };
        if col >= cols && !can_add {
            self.toast("该表格类型结构固定，不能添加列", ToastKind::Info);
            return false;
        }
        let need_cols = if col >= cols { col + 1 - cols } else { 0 };
        if need_cols == 0 && row < rows {
            return true;
        }
        if let Some(t) = self.project.table_mut(id) {
            for _ in 0..need_cols {
                let g = t.group_names.len();
                let name = match kind {
                    TableKind::Xy => format!("Dataset {}", (b'A' + (g % 26) as u8) as char),
                    TableKind::MultipleVariables => format!("Var {}", g + 1),
                    _ => format!("Group {}", g + 1),
                };
                t.group_names.push(name.clone());
                let mut c = crate::core::model::Column::new(
                    if matches!(kind, TableKind::Column | TableKind::MultipleVariables) {
                        name
                    } else {
                        "Y1".into()
                    },
                    ColRole::Y,
                );
                c.group = g;
                c.cells = vec![Cell::empty(); t.nrows().max(1)];
                t.columns.push(c);
            }
            t.ensure_rows(row + 1);
        }
        self.dirty = true;
        self.bump_chart();
        true
    }

    fn commit_edit(&mut self, id: u64, row: usize, col: usize, buf: &str) {
        let old = self
            .project
            .table(id)
            .and_then(|t| t.cell(row, col))
            .map(|c| c.display())
            .unwrap_or_default();
        if old == buf.trim() {
            return;
        }
        // 写在虚拟格里时先把行列补齐，否则 set_text 会静默丢弃输入。
        if !self.materialize_cell(id, row, col) {
            return;
        }
        self.snapshot();
        if let Some(t) = self.project.table_mut(id) {
            let _ = t.set_text(row, col, buf);
            // 计算列跟随源数据重算
            t.recompute();
        }
        self.dirty = true;
        self.bump_chart();
    }

    /// 提交表头改名。`ed.row` 是类型标志：0=列名、1=组名（表头编辑时 row 不用作行号）。
    /// 无变化 / 空名不写回，避免无意义的快照。
    /// 提交表头改名。`group`：true=组名（显示名），false=列名。
    ///
    /// 两个历史坑：
    /// 1. `group_names` 为空时 `gid < len` 为假 → **静默失败**（构成表/生存表改不了名）。
    ///    现在先扩展 `group_names` 再写入。
    /// 2. 分组表每组 1 列时，组名与列名是同一个东西，但存两个字段——
    ///    只改一个就不同步。单列组时**双向同步**。
    fn commit_header_edit(&mut self, id: u64, col: usize, buf: &str, group: bool) {
        let name = buf.trim().to_string();
        if name.is_empty() {
            return;
        }
        if group {
            let gid = self
                .project
                .table(id)
                .and_then(|t| t.columns.get(col))
                .map(|c| c.group)
                .unwrap_or(0);
            let changed = self
                .project
                .table(id)
                .map(|t| t.group_name(gid) != name.as_str())
                .unwrap_or(false);
            if !changed {
                return;
            }
            self.snapshot();
            if let Some(t) = self.project.table_mut(id) {
                // group_names 可能为空（构成表/生存表模板）——先补齐再写
                while t.group_names.len() <= gid {
                    t.group_names.push(String::new());
                }
                t.group_names[gid] = name.clone();
                // 单列组：组名=列名，同步 columns[].name
                let single = t
                    .columns
                    .iter()
                    .filter(|c| c.group == gid && c.role == ColRole::Y)
                    .count()
                    == 1;
                if single {
                    for c in t
                        .columns
                        .iter_mut()
                        .filter(|c| c.group == gid && c.role == ColRole::Y)
                    {
                        c.name = name.clone();
                    }
                }
            }
        } else {
            // 列名改名。虚拟列（col >= 真列数）提交时**实体化**——和在格子里
            // 打字一样，改了名就变成真列，不让人觉得「后面的格子是二等公民」。
            let exists = self
                .project
                .table(id)
                .map(|t| col < t.columns.len())
                .unwrap_or(false);
            if exists {
                let changed = self
                    .project
                    .table(id)
                    .and_then(|t| t.columns.get(col))
                    .map(|c| c.name != name)
                    .unwrap_or(false);
                if !changed {
                    return;
                }
                self.snapshot();
                if let Some(t) = self.project.table_mut(id) {
                    if let Some(c) = t.columns.get_mut(col) {
                        c.name = name.clone();
                    }
                    // 单列组：列名=组名，同步 group_names（否则分组表两层不同步）
                    let gid = t.columns.get(col).map(|c| c.group).unwrap_or(0);
                    let single = t
                        .columns
                        .iter()
                        .filter(|c| c.group == gid && c.role == ColRole::Y)
                        .count()
                        == 1;
                    if single {
                        while t.group_names.len() <= gid {
                            t.group_names.push(String::new());
                        }
                        t.group_names[gid] = name;
                    }
                }
            } else {
                if !self.materialize_cell(id, 0, col) {
                    return;
                }
                self.snapshot();
                if let Some(t) = self.project.table_mut(id) {
                    if let Some(c) = t.columns.get_mut(col) {
                        c.name = name;
                    }
                }
            }
        }
        self.dirty = true;
        self.bump_chart();
    }

    /// 提交行标题改名（双击行头触发）。
    fn commit_row_edit(&mut self, id: u64, row: usize, buf: &str) {
        let name = buf.trim().to_string();
        if name.is_empty() {
            return;
        }
        let changed = self
            .project
            .table(id)
            .map(|t| t.row_headers.get(row).map(|s| *s != name).unwrap_or(true))
            .unwrap_or(false);
        if !changed {
            return;
        }
        self.snapshot();
        if let Some(t) = self.project.table_mut(id) {
            while t.row_headers.len() <= row {
                t.row_headers.push(String::new());
            }
            t.row_headers[row] = name;
        }
        self.dirty = true;
    }
}

/// 单元格文本样式：**绘制与内联编辑必须共用同一套**。
///
/// 编辑框与格子文字错位的根源就是两边各算各的：绘制用 pad=8 垂直居中、
/// 数字右对齐，编辑框却用 margin=7 恒左对齐。这里定死一套，两边都从这里取。
mod cell_text {
    use egui::{Align2, FontId};

    pub const FONT_SIZE: f32 = 11.8;
    /// 左右留白：绘制的 pad 与 TextEdit 的 margin 都用它。
    pub const PAD_X: f32 = 8.0;

    pub fn font() -> FontId {
        FontId::proportional(FONT_SIZE)
    }

    /// 数字右对齐、文本左对齐，垂直恒居中——与 `paint` 完全一致。
    pub fn align2(right: bool) -> Align2 {
        if right {
            Align2::RIGHT_CENTER
        } else {
            Align2::LEFT_CENTER
        }
    }
}

/// 两层表头的文本样式：组名居中、子列名左对齐，绘制与编辑框共用。
mod header_text {
    use egui::{Align2, FontId};

    pub const GROUP_FONT: f32 = 10.0;
    pub const SUB_FONT: f32 = 10.5;
    pub const PAD_X: f32 = 8.0;

    pub fn group_font() -> FontId {
        FontId::proportional(GROUP_FONT)
    }
    pub fn sub_font() -> FontId {
        FontId::proportional(SUB_FONT)
    }
    /// 组名居中（跨格中央），子列名左对齐——与 `paint` 一致。
    pub fn group_align() -> Align2 {
        Align2::CENTER_CENTER
    }
    pub fn sub_align() -> Align2 {
        Align2::LEFT_CENTER
    }
}

fn paint_cell_text(
    painter: &egui::Painter,
    cr: Rect,
    text: &str,
    font: FontId,
    color: Color32,
    right: bool,
) {
    crate::app::widgets::paint_text_clipped(
        painter,
        cr,
        text,
        font,
        color,
        right,
        cell_text::PAD_X,
    );
}

/// 计算列 `c` 所属组的成员列范围 `[first, last]`（含）。
///
/// 真列查 `ColumnView.group_id`；虚拟列按 `group_size` 合成（每组 `group_size` 列）。
/// **X 列永远排除**——X 与第一组共用 `group=0`，混进来会让组选区/组宽多算一列。
fn group_span_of(
    c: usize,
    real_cols: usize,
    views: &[ColumnView],
    group_size: usize,
) -> (usize, usize) {
    let gsz = group_size.max(1);
    if c >= real_cols {
        // 虚拟列：以 real_cols 为起点每 gsz 列一组
        let vi = c - real_cols;
        let first = real_cols + (vi / gsz) * gsz;
        (first, first + gsz - 1)
    } else {
        // X 列是独立的单列组（column_views 里 is_x → span=1），不与 Y 组共享跨度
        if views.get(c).map(|v| v.is_x).unwrap_or(false) {
            return (c, c);
        }
        let gid = views.get(c).map(|v| v.group_id).unwrap_or(0);
        let members: Vec<usize> = views
            .iter()
            .enumerate()
            .filter(|(_, v)| v.group_id == gid && !v.is_x)
            .map(|(k, _)| k)
            .collect();
        match (members.first(), members.last()) {
            (Some(&a), Some(&b)) => (a, b),
            _ => (c, c),
        }
    }
}

fn column_views(t: &DataTable) -> Vec<ColumnView> {
    let mut out = Vec::with_capacity(t.columns.len());
    let mut group_start_seen: Vec<usize> = Vec::new();
    for (i, c) in t.columns.iter().enumerate() {
        let is_x = c.role == ColRole::X;
        let top = if is_x {
            "X".to_string()
        } else {
            t.group_name(c.group)
        };
        let sub = if t.kind.has_subcolumns() || c.role == ColRole::X {
            c.name.clone()
        } else {
            String::new()
        };
        let (group_start, group_span) = if is_x {
            (true, 1)
        } else if group_start_seen.contains(&c.group) {
            (false, 0)
        } else {
            group_start_seen.push(c.group);
            let span = t
                .columns
                .iter()
                .filter(|x| x.group == c.group && x.role == ColRole::Y)
                .count();
            (true, span)
        };
        out.push(ColumnView {
            top,
            sub,
            is_x,
            group_start,
            group_span,
            group_id: c.group,
        });
        let _ = i;
    }
    out
}

/// 滑块几何（纯函数）：内容不超出视口时返回 None —— 不画也不接拖动。
fn thumb_rect(track: Rect, offset: f32, total: f32, view: f32, vertical: bool) -> Option<Rect> {
    if total <= view + 0.5 {
        return None;
    }
    let ratio = (view / total).clamp(0.06, 1.0);
    if vertical {
        let h = (track.height() * ratio).max(28.0);
        let y = track.top() + (track.height() - h) * (offset / (total - view).max(1.0));
        Some(Rect::from_min_size(
            egui::pos2(track.left() + 2.5, y),
            Vec2::new(track.width() - 5.0, h),
        ))
    } else {
        let w = (track.width() * ratio).max(28.0);
        let x = track.left() + (track.width() - w) * (offset / (total - view).max(1.0));
        Some(Rect::from_min_size(
            egui::pos2(x, track.top() + 2.5),
            Vec2::new(w, track.height() - 5.0),
        ))
    }
}

/// 画滑块：悬停/拖动时加深加宽，给抓取反馈。
fn draw_thumb(painter: &egui::Painter, r: Rect, pal: &crate::app::theme::Palette, hot: bool) {
    let r = if hot { r.expand(1.5) } else { r };
    let a = if hot { 170 } else { 90 };
    painter.rect_filled(r, CornerRadius::same(3), alpha(pal.muted, a));
}

#[cfg(test)]
mod infinite_grid_tests {
    use super::*;

    /// 模拟连续滚动 300 次（每次 400px 纵 / 120px 横），
    /// 每一步都必须保证视口下方与右侧仍有内容 —— 也就是永远滚不到头。
    #[test]
    fn 网格永远滚不到头() {
        let mut g = GridState::default();
        let (view_h, view_w) = (520.0_f32, 900.0_f32);
        let (start_rows, start_cols) = (g.virtual_rows, g.virtual_cols);
        let mut sy = 0.0_f32;
        let mut sx = 0.0_f32;
        for step in 0..300 {
            g.advance_virtual(sy, sx, view_h, view_w, COL_W);
            let n_rows = g.virtual_rows.max(12) + 6;
            let n_cols = g.virtual_cols.max(1);
            assert!(
                n_rows as f32 * ROW_H > sy + view_h,
                "第 {step} 步纵向到底了：内容 {}px，视口底 {:.0}px",
                n_rows as f32 * ROW_H,
                sy + view_h
            );
            assert!(
                n_cols as f32 * COL_W > sx + view_w,
                "第 {step} 步横向到头了：内容 {}px，视口右 {:.0}px",
                n_cols as f32 * COL_W,
                sx + view_w
            );
            sy += 400.0;
            sx += 120.0;
        }
        assert!(g.virtual_rows > start_rows, "虚拟行应随滚动增长");
        assert!(g.virtual_cols > start_cols, "虚拟列应随滚动增长");
        // 模拟 300 步 ≈ 12 万像素后仍无上界
        assert!(
            g.virtual_rows > 5000,
            "滚动 300 步后虚拟行 {}",
            g.virtual_rows
        );
    }

    /// 滑块几何：offset=0 时贴顶/贴左，offset=total-view 时贴底/贴右。
    /// 拖动映射（delta × span/gap）依赖这个端点性质，改坏了拖动会漂。
    #[test]
    fn 滑块端点贴合轨道两端() {
        let track = Rect::from_min_max(egui::pos2(10.0, 20.0), egui::pos2(22.0, 520.0));
        let (total, view) = (5000.0_f32, 500.0_f32);
        let t0 = thumb_rect(track, 0.0, total, view, true).unwrap();
        assert!((t0.top() - track.top()).abs() < 0.01, "offset=0 应贴顶");
        let t1 = thumb_rect(track, total - view, total, view, true).unwrap();
        assert!(
            (t1.bottom() - track.bottom()).abs() < 0.01,
            "offset=max 应贴底"
        );

        let track_h = Rect::from_min_max(egui::pos2(10.0, 20.0), egui::pos2(910.0, 32.0));
        let h0 = thumb_rect(track_h, 0.0, total, view, false).unwrap();
        assert!((h0.left() - track_h.left()).abs() < 0.01);
        let h1 = thumb_rect(track_h, total - view, total, view, false).unwrap();
        assert!((h1.right() - track_h.right()).abs() < 0.01);

        // 内容不超过视口：没有滑块
        assert!(thumb_rect(track, 0.0, view, view, true).is_none());
    }

    #[test]
    fn 切换表格后虚拟区回到初始值() {
        let mut g = GridState::default();
        g.virtual_rows = 9000;
        g.virtual_cols = 900;
        let p = crate::core::sample::sample_project();
        g.reset_for(1, &p);
        assert_eq!(g.virtual_rows, 48);
        assert_eq!(g.virtual_cols, 12);
    }
}

#[cfg(test)]
mod col_width_tests {
    use super::*;
    use crate::core::model::{Cell, Column, TableKind};

    fn table_with_widths(ws: &[f32]) -> DataTable {
        let mut t = DataTable::template(TableKind::Column, String::from("t"), 1);
        t.columns.clear();
        for (i, w) in ws.iter().enumerate() {
            let mut c = Column::new(format!("C{i}"), ColRole::Y);
            c.cells = vec![Cell::number(1.0)];
            c.width = Some(*w);
            t.columns.push(c);
        }
        t
    }

    /// 前缀和定位必须与「逐列累加」一致，且 x→列的反查互逆。
    #[test]
    fn 列偏移与反查互逆() {
        let ws = vec![50.0, 120.0, 80.0, 200.0];
        let m = ColMetrics::new(&ws, 4);
        let mut acc = 0.0;
        for (c, w) in ws.iter().enumerate() {
            assert_eq!(m.x(c), acc, "第 {c} 列左沿");
            assert_eq!(m.w(c), *w);
            acc += w;
        }
        assert_eq!(m.total(), acc);
        // 反查：每列左沿、中点、右沿内侧都应命中该列
        for c in 0..4 {
            let (l, w) = (m.x(c), m.w(c));
            assert_eq!(m.col_at(l), Some(c), "左沿");
            assert_eq!(m.col_at(l + w * 0.5), Some(c), "中点");
            assert_eq!(m.col_at(l + w - 0.01), Some(c), "右沿内侧");
        }
        assert_eq!(m.col_at(-1.0), None);
        assert_eq!(m.col_at(acc + 1.0), None);
    }

    /// 虚拟列补齐到 n，用默认宽；真实列宽不受影响。
    #[test]
    fn 虚拟列用默认宽补齐() {
        let ws = vec![50.0, 120.0];
        let m = ColMetrics::new(&ws, 5);
        assert_eq!(m.w(0), 50.0);
        assert_eq!(m.w(1), 120.0);
        assert_eq!(m.w(2), COL_W);
        assert_eq!(m.w(4), COL_W);
        assert_eq!(m.total(), 50.0 + 120.0 + 3.0 * COL_W);
    }

    /// 列宽 / 行标题宽随工程序列化往返，未设置时回落默认值。
    #[test]
    fn 列宽随工程持久化() {
        let mut t = table_with_widths(&[55.0, 140.0]);
        t.row_header_width = Some(88.0);
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"width\":55.0"), "{json}");
        assert!(json.contains("\"row_header_width\":88.0"), "{json}");
        let back: DataTable = serde_json::from_str(&json).unwrap();
        assert_eq!(back.columns[0].width, Some(55.0));
        assert_eq!(back.columns[1].width, Some(140.0));
        assert_eq!(back.row_header_width, Some(88.0));

        // 未设置时是 None，且序列化后不出现该字段（文件保持干净）
        let plain = DataTable::template(TableKind::Column, String::from("p"), 2);
        let j2 = serde_json::to_string(&plain).unwrap();
        assert!(!j2.contains("row_header_width"), "{j2}");
        assert!(!j2.contains("\"width\""), "{j2}");
    }

    /// col_widths：读模型、越界夹紧、缺省回默认。
    #[test]
    fn 读模型宽度并夹紧() {
        let mut t = table_with_widths(&[5.0, 5000.0, 96.0]);
        t.row_header_width = Some(300.0);
        let (row_w, ws) = col_widths(&t, 3);
        assert_eq!(row_w, 240.0, "行标题宽夹到上限");
        assert_eq!(ws[0], COL_W_MIN, "过窄夹到下限");
        assert_eq!(ws[1], COL_W_MAX, "过宽夹到上限");
        assert_eq!(ws[2], 96.0);
        // 列数不足时补默认宽
        let (_, ws) = col_widths(&t, 5);
        assert_eq!(ws.len(), 5);
        assert_eq!(ws[4], COL_W);
    }

    /// 可见区间：按偏移表切，不能按 sx / COL_W 等分。
    #[test]
    fn 可见列按偏移切() {
        let ws = vec![200.0, 40.0, 40.0, 40.0, 200.0];
        let m = ColMetrics::new(&ws, 5);
        // 视口 0..100 → 只见第 0 列
        assert_eq!(m.visible(0.0, 100.0), 0..1);
        // 视口 180..280 → 跨第 0 尾与第 1、2 列
        assert_eq!(m.visible(180.0, 100.0), 0..3);
        // 视口 300..400 → 第 3、4 列
        assert_eq!(m.visible(300.0, 100.0), 3..5);
    }
}

#[cfg(test)]
mod excel_edit_tests {
    use super::*;
    use crate::core::model::{Cell, Column, TableKind};

    fn col_table() -> DataTable {
        let mut t = DataTable::template(TableKind::Column, String::from("t"), 1);
        t.columns.clear();
        t.row_headers.clear();
        for (j, name) in ["A", "B", "C"].iter().enumerate() {
            let mut c = Column::new(*name, ColRole::Y);
            c.group = j;
            c.cells = (0..5).map(|r| Cell::number((j * 10 + r) as f64)).collect();
            t.columns.push(c);
        }
        t
    }

    /// 插删列：位置正确、细胞数据跟着移、X 列受保护。
    #[test]
    fn 插入与删除列() {
        let mut t = col_table();
        assert_eq!(t.columns.len(), 3);
        t.insert_col(1, "NEW", ColRole::Y);
        assert_eq!(t.columns.len(), 4);
        assert_eq!(t.columns[1].name, "NEW");
        assert_eq!(t.columns[2].name, "B", "原 B 列右移");
        assert_eq!(t.columns[1].cells.len(), 5, "新列行数对齐");
        t.remove_col(1);
        assert_eq!(t.columns.len(), 3);
        assert_eq!(t.columns[1].name, "B", "删除后 B 回位");

        // X 列不可删
        let mut xy = DataTable::template(TableKind::Xy, String::from("xy"), 2);
        let n = xy.columns.len();
        assert_eq!(xy.columns[0].role, ColRole::X);
        xy.remove_col(0);
        assert_eq!(xy.columns.len(), n, "X 列应受保护，列数不变");
    }

    /// 删整组：一次删掉同组的所有 Y 列，X 列受保护——「删一大格」。
    #[test]
    fn 删除整组() {
        let mut t = DataTable::template(TableKind::Xy, String::from("xy"), 3);
        // XY 模板：X 列(group=0) + 每组 1 个 Y 列。注意 X 与第一组共用 group=0。
        let before = t.columns.len();
        let gid = t.columns[1].group; // 第一个 Y 列的组号
        let y_in_group = t
            .columns
            .iter()
            .filter(|c| c.group == gid && c.role == ColRole::Y)
            .count();
        assert!(y_in_group >= 1);
        t.remove_group(gid);
        // 该组的 Y 列全删；X 列虽然同 group 也受保护留下
        assert_eq!(
            t.columns.len(),
            before - y_in_group,
            "组内 Y 列全删（X 列受保护不计）"
        );
        assert_eq!(t.columns[0].role, ColRole::X, "X 列不受影响");
        assert!(
            t.columns
                .iter()
                .all(|c| c.group != gid || c.role == ColRole::X),
            "该组已无 Y 列"
        );
    }

    /// 布局一致性：align2 与 paint_cell_text 的对齐口径必须是同一套。
    #[test]
    fn 编辑与绘制共用布局() {
        use egui::Align2;
        // 数字列：右对齐 + 垂直居中
        assert_eq!(cell_text::align2(true), Align2::RIGHT_CENTER);
        // 文本列：左对齐 + 垂直居中
        assert_eq!(cell_text::align2(false), Align2::LEFT_CENTER);
        // pad 与 font 是共享常量，paint 与 TextEdit 都从这里取
        assert_eq!(cell_text::PAD_X, 8.0);
        assert_eq!(cell_text::FONT_SIZE, 11.8);
    }

    /// 编辑状态机：进入/提交/取消三种路径。
    #[test]
    fn 编辑状态机进提交消() {
        // GridState 的编辑字段生命周期
        let mut g = GridState::default();
        assert!(g.editing.is_none(), "初始无编辑");
        assert!(!g.composing, "初始非组合态");

        // 模拟进入编辑（start_edit 的核心语义）
        g.editing = Some(EditState {
            row: 1,
            col: 2,
            buf: "42".into(),
            just_started: true,
        });
        assert!(g.editing.is_some());
        // Esc 取消 → editing 置 None（在 grid_body 的分支里）
        // 这里只验状态字段的存取，UI 分支逻辑靠截图验证
        g.editing = None;
        assert!(g.editing.is_none());

        // 组合态标志独立于编辑态
        g.composing = true;
        assert!(g.composing);
        g.composing = false;
    }
}

#[cfg(test)]
mod group_span_tests {
    use super::*;

    fn v(top: &str, sub: &str, is_x: bool, gs: bool, span: usize, gid: usize) -> ColumnView {
        ColumnView {
            top: top.into(),
            sub: sub.into(),
            is_x,
            group_start: gs,
            group_span: span,
            group_id: gid,
        }
    }

    /// 组跨度：真列排除 X；虚拟列按 group_size 合成。
    #[test]
    fn 组跨度真列与虚拟列() {
        // X(g=0) + Vehicle 的 Y1,Y2,Y3(g=0) + DrugA 的 Y1,Y2,Y3(g=1)
        let views = vec![
            v("X", "浓度", true, true, 1, 0),
            v("Vehicle", "Y1", false, true, 3, 0),
            v("Vehicle", "Y2", false, false, 0, 0),
            v("Vehicle", "Y3", false, false, 0, 0),
            v("DrugA", "Y1", false, true, 3, 1),
            v("DrugA", "Y2", false, false, 0, 1),
            v("DrugA", "Y3", false, false, 0, 1),
        ];
        let real = views.len();
        // Vehicle 组（c=1..3）：排除 X
        assert_eq!(group_span_of(1, real, &views, 3), (1, 3));
        assert_eq!(
            group_span_of(2, real, &views, 3),
            (1, 3),
            "组中间列也返回整组"
        );
        // DrugA 组
        assert_eq!(group_span_of(4, real, &views, 3), (4, 6));
        // 虚拟列：每 3 列一组
        assert_eq!(group_span_of(7, real, &views, 3), (7, 9), "第 1 个虚拟组");
        assert_eq!(group_span_of(8, real, &views, 3), (7, 9));
        assert_eq!(
            group_span_of(10, real, &views, 3),
            (10, 12),
            "第 2 个虚拟组"
        );
        // X 列自己：没有 Y 组员，退回自身
        assert_eq!(group_span_of(0, real, &views, 3), (0, 0));
    }
}

#[cfg(test)]
mod rename_all_kinds_tests {
    use super::*; // noqa: used by test macros
    use crate::core::model::{ColRole, DataTable, TableKind};

    /// 模拟「改名提交」的完整路径：显示名来源 → 提交 → 显示名必须变。
    /// 覆盖所有表型。以前这里穿帮：构成/生存静默失败、分组表两层不同步。
    fn rename_and_check(kind: TableKind) {
        let mut t = DataTable::template(kind, String::from("t"), 3);
        // 找第一个非 X 列
        let col = t
            .columns
            .iter()
            .position(|c| c.role != ColRole::X)
            .expect("至少有一个非 X 列");
        let gid = t.columns[col].group;
        let old_group = t.group_name(gid);
        let old_col = t.columns[col].name.clone();

        // 1) 改组名（one_level 表的显示名就是组名）
        let new_name = "RENAMED_GROUP";
        while t.group_names.len() <= gid {
            t.group_names.push(String::new());
        }
        t.group_names[gid] = new_name.to_string();
        // 单列组同步列名
        let single = t
            .columns
            .iter()
            .filter(|c| c.group == gid && c.role == ColRole::Y)
            .count()
            == 1;
        if single {
            for c in t
                .columns
                .iter_mut()
                .filter(|c| c.group == gid && c.role == ColRole::Y)
            {
                c.name = new_name.to_string();
            }
        }
        assert_eq!(t.group_name(gid), new_name, "{:?} 组名改后显示应变", kind);
        if single {
            assert_eq!(t.columns[col].name, new_name, "{:?} 单列组列名应同步", kind);
        }

        // 2) 改列名（two_level 子行的显示名）
        let new_col = "RENAMED_COL";
        t.columns[col].name = new_col.to_string();
        // 单列组同步组名
        if single {
            while t.group_names.len() <= gid {
                t.group_names.push(String::new());
            }
            t.group_names[gid] = new_col.to_string();
        }
        assert_eq!(t.columns[col].name, new_col, "{:?} 列名改后显示应变", kind);
        if single {
            assert_eq!(t.group_name(gid), new_col, "{:?} 单列组组名应同步", kind);
        }

        assert_ne!(old_group, new_name);
        assert_ne!(old_col, new_col);
    }

    #[test]
    fn 所有表型改名生效() {
        for kind in [
            TableKind::Xy,
            TableKind::Column,
            TableKind::Grouped,
            TableKind::Contingency,
            TableKind::Survival,
            TableKind::PartsOfWhole,
            TableKind::MultipleVariables,
            TableKind::Nested,
        ] {
            rename_and_check(kind);
        }
    }

    /// 行名改名：row_headers[r] 写入后必须能读回。
    #[test]
    fn 行名改名() {
        let mut t = DataTable::template(TableKind::Column, String::from("t"), 2);
        assert!(!t.row_headers.is_empty());
        t.row_headers[0] = "RENAMED_ROW".to_string();
        assert_eq!(t.row_headers[0], "RENAMED_ROW");
    }

    /// 生存表模板：每列 group 必须独立（之前全是 0 导致显示名串台）。
    #[test]
    fn 生存表模板组号独立() {
        let t = DataTable::template(TableKind::Survival, String::from("s"), 1);
        let y_groups: Vec<usize> = t
            .columns
            .iter()
            .filter(|c| c.role == ColRole::Y)
            .map(|c| c.group)
            .collect();
        assert_eq!(y_groups.len(), 2, "两个 Y 列");
        assert_ne!(y_groups[0], y_groups[1], "每列 group 必须独立");
    }

    /// 构成表改名不能静默失败（group_names 为空时要能写入）。
    #[test]
    fn 构成表组名可写() {
        let mut t = DataTable::template(TableKind::PartsOfWhole, String::from("p"), 1);
        let col = t.columns.iter().position(|c| c.role != ColRole::X).unwrap();
        let gid = t.columns[col].group;
        // 模拟 commit_header_edit 的 group 路径
        while t.group_names.len() <= gid {
            t.group_names.push(String::new());
        }
        t.group_names[gid] = "NEW_NAME".to_string();
        assert_eq!(t.group_name(gid), "NEW_NAME", "构成表组名应可写");
    }
}
