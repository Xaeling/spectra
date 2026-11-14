//! 表格剪贴板 —— 复制 / 剪切 / 粘贴 / 清除的**唯一实现**。
//!
//! 设计原则（重写目的：消灭之前「复制少一行 / 粘贴无效」的混乱）：
//!
//! 1. **单一入口**：对外只有 `clip_copy` / `clip_cut` / `clip_paste` / `clip_clear`
//!    四个方法。键盘快捷键、右键菜单**都只调这四个**，不存在第二份实现。
//! 2. **纯逻辑与副作用分离**：`selection_to_tsv` / `tsv_dims` 是纯函数（可单测），
//!    只有 `write_clipboard` / `read_clipboard` 接触系统剪贴板。
//! 3. **选区一律含端点**：`Sel { r0,c0,r1,c1 }` 两端都在内（inclusive），
//!    行数 = `r1 - r0 + 1`。全模块统一，不再出现 +1/-1 的 off-by-one。
//! 4. **光标与选区解耦**：复制只读选区、粘贴写光标，互不干扰。

use crate::app::{App, Sheet, ToastKind};
use crate::core::model::{Cell, DataTable};

/// 矩形选区：左上 `(r0,c0)` → 右下 `(r1,c1)`，**两端都含**。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sel {
    pub r0: usize,
    pub c0: usize,
    pub r1: usize,
    pub c1: usize,
}

impl Sel {
    /// 由任意两个角（anchor / cursor，无序）构造，自动取 min/max。
    pub fn corners(a: (usize, usize), b: (usize, usize)) -> Self {
        Self {
            r0: a.0.min(b.0),
            c0: a.1.min(b.1),
            r1: a.0.max(b.0),
            c1: a.1.max(b.1),
        }
    }

    /// 单格选区。
    pub fn single(r: usize, c: usize) -> Self {
        Self {
            r0: r,
            c0: c,
            r1: r,
            c1: c,
        }
    }

    /// 整行。
    pub fn row(r: usize, last_col: usize) -> Self {
        Self {
            r0: r,
            c0: 0,
            r1: r,
            c1: last_col,
        }
    }

    /// 整列。
    pub fn col(c: usize, last_row: usize) -> Self {
        Self {
            r0: 0,
            c0: c,
            r1: last_row,
            c1: c,
        }
    }

    /// 行数（含端点）。
    pub fn rows(&self) -> usize {
        self.r1.saturating_sub(self.r0) + 1
    }

    /// 列数（含端点）。
    pub fn cols(&self) -> usize {
        self.c1.saturating_sub(self.c0) + 1
    }
}

/// 选区 → TSV 文本（每行以 `\n` 结尾）。纯函数，无副作用。
///
/// 空格子导出为空串；列间用 `\t`；行间用 `\n`。末尾保留一个换行，
/// 与 Excel / 剪贴板习惯一致。
pub fn selection_to_tsv(t: &DataTable, sel: Sel) -> String {
    let mut out = String::new();
    for r in sel.r0..=sel.r1 {
        for c in sel.c0..=sel.c1 {
            if c > sel.c0 {
                out.push('\t');
            }
            out.push_str(&t.cell(r, c).map(|x| x.display()).unwrap_or_default());
        }
        out.push('\n');
    }
    out
}

/// 统计 TSV 的行列数。末尾单个换行**不算**一行（`"a\nb\n"` = 2 行）。
///
/// 中间空行必须计入行数（否则粘贴会少行）。
pub fn tsv_dims(text: &str) -> (usize, usize) {
    let norm = text.replace("\r\n", "\n");
    let mut lines: Vec<&str> = norm.split('\n').collect();
    if matches!(lines.last(), Some(&"")) {
        lines.pop();
    }
    if lines.is_empty() {
        return (1, 1);
    }
    let rows = lines.len();
    let cols = lines
        .iter()
        .map(|l| l.split('\t').count())
        .max()
        .unwrap_or(1)
        .max(1);
    (rows, cols)
}

fn write_clipboard(text: &str) -> bool {
    arboard::Clipboard::new()
        .map(|mut c| c.set_text(text.to_string()).is_ok())
        .unwrap_or(false)
}

fn read_clipboard() -> Option<String> {
    arboard::Clipboard::new()
        .ok()
        .and_then(|mut c| c.get_text().ok())
}

/// 剪贴板诊断日志（append 到 %TEMP%\stataura_clip.log）。
/// 只在复制/粘贴/切换选区时写几行，量极小；用于排查「选区错位」。
fn debug_log(msg: &str) {
    use std::io::Write;
    let path = std::env::temp_dir().join("stataura_clip.log");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(f, "{msg}");
    }
}

/// 一帧内识别到的剪贴板指令。
#[derive(Debug, Clone)]
pub enum ClipCmd {
    Copy,
    Cut,
    /// `Some(text)` = 事件里带了文本；`None` = 需要回退去系统剪贴板读。
    Paste(Option<String>),
    Clear,
    SelectAll,
}

/// 从当帧输入事件里识别剪贴板指令。
///
/// **为什么不用 `i.modifiers` + `key_pressed`**：`i.modifiers` 是帧首的修饰键
/// 快照，快速按下并松开 Ctrl 时，读到它的时候 Ctrl 可能已经松开 → `command=false`
/// → 整个分支被跳过（表现为「粘贴完全没反应」）。
///
/// 优先级：
/// 1. egui 原生事件 `Event::Copy` / `Event::Cut` / `Event::Paste(text)` ——
///    由后端直接从系统剪贴板生成，自带文本，最可靠。
/// 2. 回退到 `Event::Key`，用**事件自带**的 `modifiers` 判断（不看 `i.modifiers`）。
pub fn clipboard_command(ctx: &egui::Context) -> Option<ClipCmd> {
    ctx.input(|i| {
        // 1) 原生剪贴板事件
        for e in &i.events {
            match e {
                egui::Event::Copy => return Some(ClipCmd::Copy),
                egui::Event::Cut => return Some(ClipCmd::Cut),
                egui::Event::Paste(t) => return Some(ClipCmd::Paste(Some(t.clone()))),
                _ => {}
            }
        }
        // 2) Key 事件回退（事件自带 modifiers）
        for e in &i.events {
            if let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = e
            {
                let c = modifiers.ctrl || modifiers.command || modifiers.mac_cmd;
                match (key, c) {
                    (egui::Key::C, true) => return Some(ClipCmd::Copy),
                    (egui::Key::X, true) => return Some(ClipCmd::Cut),
                    (egui::Key::V, true) => return Some(ClipCmd::Paste(None)),
                    (egui::Key::A, true) => return Some(ClipCmd::SelectAll),
                    (egui::Key::Delete, false) | (egui::Key::Backspace, false) => {
                        return Some(ClipCmd::Clear)
                    }
                    _ => {}
                }
            }
        }
        None
    })
}

impl App {
    /// 当前是否为表格视图，是则返回 `(表 id, 选区)`。
    pub fn table_selection(&self) -> Option<(u64, Sel)> {
        match self.sel {
            Some(Sheet::Table(id)) if self.project.table(id).is_some() => {
                let sel = Sel::corners(self.grid.anchor, self.grid.cursor);
                debug_log(&format!(
                    "[table_selection] id={id} anchor={:?} cursor={:?} sel=({},{})-({},{}) rows={} cols={} nrows={}",
                    self.grid.anchor,
                    self.grid.cursor,
                    sel.r0,
                    sel.c0,
                    sel.r1,
                    sel.c1,
                    sel.rows(),
                    sel.cols(),
                    self.project.table(id).map(|t| t.nrows()).unwrap_or(0),
                ));
                Some((id, sel))
            }
            _ => {
                debug_log(&format!(
                    "[table_selection] NOT a table: sel={:?}",
                    self.sel
                ));
                None
            }
        }
    }

    // ---- 对外四个动作：快捷键 + 右键菜单统一走这里 ----

    /// 复制当前选区到系统剪贴板。
    pub fn clip_copy(&mut self) -> bool {
        let Some((id, sel)) = self.table_selection() else {
            return false;
        };
        self.copy_region(id, sel)
    }

    /// 剪切当前选区（复制 + 清除）。
    pub fn clip_cut(&mut self) -> bool {
        let Some((id, sel)) = self.table_selection() else {
            return false;
        };
        if self.copy_region(id, sel) {
            self.clear_region(id, sel);
            self.toast("已剪切", ToastKind::Info);
            true
        } else {
            false
        }
    }

    /// 粘贴到当前光标位置（文本从系统剪贴板读取）。
    pub fn clip_paste(&mut self) -> bool {
        self.clip_paste_opt(None)
    }

    /// 粘贴。`text` 为 `Some` 时直接用（来自 `Event::Paste`），否则读系统剪贴板。
    pub fn clip_paste_opt(&mut self, text: Option<String>) -> bool {
        let Some((id, sel)) = self.table_selection() else {
            debug_log("[clip_paste] ABORT: 当前不是表格");
            return false;
        };
        let text = match text {
            Some(t) => t,
            None => match read_clipboard() {
                Some(t) => t,
                None => {
                    debug_log("[clip_paste] ABORT: read_clipboard 返回 None");
                    self.toast("剪贴板不可用", ToastKind::Error);
                    return false;
                }
            },
        };
        if text.is_empty() {
            debug_log("[clip_paste] ABORT: 剪贴板文本为空");
            self.toast("剪贴板为空", ToastKind::Info);
            return false;
        }
        // 粘贴原点是**选区左上角**（Excel 口径），不是 cursor——
        // 拖选后 cursor 在右下角，若按 cursor 粘会贴到意想不到的位置。
        let (r, c) = (sel.r0, sel.c0);
        let (rows, cols) = tsv_dims(&text);
        debug_log(&format!(
            "[clip_paste] origin=({r},{c}) sel=({},{})-({},{}) text_len={} dims={}x{} text={:?}",
            sel.r0,
            sel.c0,
            sel.r1,
            sel.c1,
            text.len(),
            rows,
            cols,
            text.chars().take(120).collect::<String>(),
        ));
        // 需要的话把行/列实体化（结构固定的表插不了列，超出的列会被跳过）。
        let _ = self.materialize_cell(id, r + rows - 1, c + cols - 1);
        self.snapshot();
        let (r0, c0, r1, c1) = match self.project.table_mut(id) {
            Some(t) => {
                let rc = t.paste_tsv(r, c, &text);
                t.recompute();
                rc
            }
            None => return false,
        };
        // 粘贴后自动把选区扩展到刚粘贴的整块（智能选区）。
        self.grid.anchor = (r0, c0);
        self.grid.cursor = (r1, c1);
        self.dirty = true;
        self.bump_chart();
        self.toast(
            format!("已粘贴 {} 行 × {} 列", r1 - r0 + 1, c1 - c0 + 1),
            ToastKind::Info,
        );
        true
    }

    /// 全选当前表格。
    pub fn clip_select_all(&mut self) {
        if let Some((id, _)) = self.table_selection() {
            if let Some(t) = self.project.table(id) {
                let nr = t.nrows();
                let nc = t.columns.len();
                self.grid.anchor = (0, 0);
                self.grid.cursor = (nr.saturating_sub(1), nc.saturating_sub(1));
            }
        }
    }

    /// 清除当前选区内容。
    pub fn clip_clear(&mut self) -> bool {
        let Some((id, sel)) = self.table_selection() else {
            return false;
        };
        self.clear_region(id, sel);
        true
    }

    // ---- 内部：显式 (id, sel) 版本，供右键菜单的整行/整列复制 ----

    /// 复制指定区域到系统剪贴板。返回是否成功。
    pub fn copy_region(&mut self, id: u64, sel: Sel) -> bool {
        let Some(t) = self.project.table(id) else {
            return false;
        };
        let tsv = selection_to_tsv(t, sel);
        debug_log(&format!(
            "[copy_region] sel=({},{})-({},{}) rows={} tsv_len={} tsv={:?}",
            sel.r0,
            sel.c0,
            sel.r1,
            sel.c1,
            sel.rows(),
            tsv.len(),
            tsv.chars().take(120).collect::<String>(),
        ));
        if write_clipboard(&tsv) {
            self.toast(
                format!("已复制 {} 行 × {} 列", sel.rows(), sel.cols()),
                ToastKind::Info,
            );
            true
        } else {
            self.toast("复制失败（剪贴板不可用）", ToastKind::Error);
            false
        }
    }

    /// 清除指定区域内容（可撤销）。
    pub fn clear_region(&mut self, id: u64, sel: Sel) {
        self.snapshot();
        if let Some(t) = self.project.table_mut(id) {
            for r in sel.r0..=sel.r1 {
                for c in sel.c0..=sel.c1 {
                    t.set_cell(r, c, Cell::empty());
                }
            }
            t.recompute();
        }
        self.dirty = true;
        self.bump_chart();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{Cell, ColRole, Column, TableKind};

    fn table(rows: usize, cols: usize) -> DataTable {
        let mut t = DataTable {
            id: 1,
            name: "t".into(),
            kind: TableKind::Xy,
            columns: Vec::new(),
            group_names: Vec::new(),
            row_headers: Vec::new(),
            row_header_width: None,
            notes: String::new(),
        };
        for c in 0..cols {
            let mut col = Column::new(format!("C{c}"), ColRole::Y);
            col.cells = vec![Cell::empty(); rows];
            t.columns.push(col);
        }
        for r in 0..rows {
            for c in 0..cols {
                t.set_cell(r, c, Cell::number((r * 10 + c) as f64));
            }
        }
        t
    }

    #[test]
    fn sel_corners_normalises() {
        let s = Sel::corners((5, 3), (1, 7));
        assert_eq!((s.r0, s.c0, s.r1, s.c1), (1, 3, 5, 7));
        assert_eq!(s.rows(), 5);
        assert_eq!(s.cols(), 5);
    }

    #[test]
    fn sel_single_is_one_cell() {
        let s = Sel::single(2, 3);
        assert_eq!((s.rows(), s.cols()), (1, 1));
    }

    #[test]
    fn tsv_roundtrip_keeps_all_rows() {
        let t = table(3, 2);
        let s = Sel::corners((0, 0), (2, 1));
        let tsv = selection_to_tsv(&t, s);
        // 3 行 × 2 列，末尾一个换行
        assert_eq!(tsv, "0\t1\n10\t11\n20\t21\n");
        assert_eq!(tsv_dims(&tsv), (3, 2));
    }

    #[test]
    fn tsv_dims_counts_empty_middle_rows() {
        // 中间空行必须保留
        assert_eq!(tsv_dims("a\n\nc\n"), (3, 1));
    }

    #[test]
    fn tsv_dims_ignores_single_trailing_newline() {
        assert_eq!(tsv_dims("a\nb\n"), (2, 1));
        assert_eq!(tsv_dims("a\nb"), (2, 1));
    }

    #[test]
    fn tsv_dims_single_column_single_row() {
        assert_eq!(tsv_dims("42\n"), (1, 1));
    }

    #[test]
    fn selection_export_includes_last_row_and_col() {
        let t = table(4, 3);
        let s = Sel::corners((1, 1), (3, 2));
        let tsv = selection_to_tsv(&t, s);
        let lines: Vec<&str> = tsv.trim_end_matches('\n').split('\n').collect();
        assert_eq!(lines.len(), 3, "应含第 1..=3 行共 3 行");
        assert_eq!(lines[2], "31\t32", "最后一行最后一列不能丢");
    }
}
