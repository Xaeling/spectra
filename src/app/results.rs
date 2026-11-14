//! 统计结果视图：标题 + 方法说明 + 结果表。
//!
//! 表格列宽按内容测量，数字列右对齐、文本列左对齐；表头跟随列的对齐方式，
//! 这样「表头 / 数值」的右边缘才落在同一条竖线上。

use egui::{Align, Align2, FontId, Layout, Rect, RichText, ScrollArea, Sense, Stroke, Ui, Vec2};

use crate::app::theme::{metric, radius, text, Palette};
use crate::app::widgets::{self, surface};
use crate::app::{App, Sheet, ToastKind};
use crate::core::model::ResultTable;

impl App {
    pub fn results_view(&mut self, ui: &mut Ui, rect: Rect, id: u64) {
        let pal = self.pal;
        ui.painter().rect_filled(rect, 0, pal.bg);
        let Some(analysis) = self.project.analysis(id).cloned() else {
            return;
        };
        let source_name = self
            .project
            .table(analysis.table)
            .map(|t| t.name.clone())
            .unwrap_or_default();

        let pad = metric::CONTENT_PAD;
        let head = Rect::from_min_size(
            egui::pos2(rect.left() + pad, rect.top() + 12.0),
            Vec2::new(rect.width() - pad * 2.0, 44.0),
        );
        ui.painter().text(
            egui::pos2(head.left(), head.center().y - 10.0),
            Align2::LEFT_CENTER,
            analysis.result.title.clone(),
            FontId::proportional(15.5),
            pal.ink,
        );
        ui.painter().text(
            egui::pos2(head.left(), head.center().y + 9.0),
            Align2::LEFT_CENTER,
            format!("{}  ·  来源：{}", analysis.kind.label(), source_name),
            FontId::proportional(text::META),
            pal.faint,
        );

        crate::app::area_public(ui, head, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::Btn::new("作图")
                    .fill(pal.accent)
                    .fg(egui::Color32::WHITE)
                    .strong(true)
                    .radius(radius::CONTROL)
                    .height(30.0)
                    .show(ui)
                    .clicked()
                {
                    self.snapshot();
                    let gid = self.project.add_graph_for(analysis.table);
                    self.select(Sheet::Graph(gid));
                    self.chart_rev = self.chart_rev.wrapping_add(1);
                    self.dirty = true;
                }
                ui.add_space(6.0);
                if widgets::Btn::new("复制表格")
                    .fill(pal.panel)
                    .fg(pal.ink)
                    .stroke(Stroke::new(1.0, pal.border))
                    .radius(radius::CONTROL)
                    .height(30.0)
                    .show(ui)
                    .clicked()
                {
                    self.copy_result(id);
                }
            });
        });

        // 卡片按内容自适应高度：高度与 `result_table` 用同一套几何（含换行）。
        let avail = (rect.width() - pad * 2.0 - 2.0).max(80.0);
        let table_h = result_height(ui, &analysis.result, avail);
        let note_h = if analysis.result.note.is_empty() {
            0.0
        } else {
            30.0
        };
        // 方法说明 + 表格 + 注释 + 上下内边距。
        let content_h = 14.0 + 20.0 + 6.0 + 10.0 + table_h + 6.0 + 10.0 + note_h + 10.0;
        let panel = Rect::from_min_max(
            egui::pos2(rect.left() + pad, head.bottom() + 12.0),
            egui::pos2(
                rect.right() - pad,
                (head.bottom() + 12.0 + content_h).min(rect.bottom() - 16.0),
            ),
        );
        surface(
            ui.painter(),
            panel,
            radius::CARD,
            pal.panel,
            Stroke::new(1.0, pal.border),
        );
        let inner = panel.shrink(1.0);

        crate::app::area_public(ui, inner, |ui| {
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.add_space(metric::PAD_X);
                ui.label(
                    RichText::new(&analysis.result.method)
                        .size(text::LABEL)
                        .color(pal.muted),
                );
            });
            ui.add_space(10.0);
            result_table(
                ui,
                &analysis.result,
                &pal,
                inner.width() - metric::PAD_X * 2.0,
            );
            ui.add_space(10.0);
            if !analysis.result.note.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(metric::PAD_X);
                    ui.label(
                        RichText::new(&analysis.result.note)
                            .size(text::MICRO + 0.5)
                            .color(pal.faint),
                    );
                });
            }
            ui.add_space(14.0);
        });
    }

    fn copy_result(&mut self, id: u64) {
        let Some(a) = self.project.analysis(id) else {
            return;
        };
        let mut s = String::new();
        s.push_str(&a.result.columns.join("\t"));
        s.push('\n');
        for row in &a.result.rows {
            s.push_str(&row.join("\t"));
            s.push('\n');
        }
        if let Ok(mut cb) = arboard::Clipboard::new() {
            let _ = cb.set_text(s);
            self.toast("结果已复制到剪贴板", ToastKind::Info);
        }
    }
}
// ---------------------------------------------------------------------------
// 三线表
// ---------------------------------------------------------------------------

/// 三线表线宽：顶线 / 底线 1.5 pt（≈2 逻辑像素），栏目线 0.75 pt（≈1）。
///
/// 依据（GB/T 7713 与主流期刊排版规范）：三线表只保留顶线、栏目线、底线；
/// 顶线与底线为粗线且等粗，栏目线为细线；**三条线贯穿表格自身的宽度**，
/// 不加竖线、不逐行画线。栏目线过粗会「切断表头与表身」的层级。
const RULE_TOP_BOTTOM: f32 = 2.0;
const RULE_HEADER: f32 = 1.0;
/// 单元格左右留白：比内容稍宽一点，避免「字贴线」。
const CELL_PAD_X: f32 = 11.0;
const CELL_PAD_Y: f32 = 6.0;
/// 块内上下留白（卡底到顶线 / 底线到卡底）。
const BLOCK_PAD_Y: f32 = 8.0;
const CAP_H: f32 = 20.0;
const BLOCK_GAP: f32 = 10.0;
const ROW_MIN_H: f32 = 22.0;

struct BlockView<'a> {
    cap: Option<String>,
    head: Option<Vec<String>>,
    /// 表头是否来自本块的第一行（来自 `columns` 时不能跳行，否则会吞掉一条数据）
    head_from_rows: bool,
    rows: Vec<&'a Vec<String>>,
}

impl<'a> BlockView<'a> {
    /// 本块要画哪一行表头：块自带的优先；否则当本块列数与全局列数一致时，
    /// 沿用全局列名（多块报告里每块都带表头，读者不用回头找列义）。
    fn header<'b>(&'b self, columns: &'b [String]) -> Option<&'b [String]> {
        if let Some(h) = &self.head {
            return Some(h.as_slice());
        }
        if !columns.is_empty()
            && self
                .rows
                .first()
                .map(|r| r.len() == columns.len())
                .unwrap_or(false)
        {
            return Some(columns);
        }
        None
    }
}

fn is_num_cell(c: &str) -> bool {
    !c.is_empty()
        && c.chars()
            .all(|ch| ch.is_ascii_digit() || ".-+eE<> %".contains(ch))
        && c.chars().any(|ch| ch.is_ascii_digit())
}

/// 一行「全文字、至少两格」→ 像是块自己的表头行。
fn looks_like_header(r: &Vec<String>) -> bool {
    let cells: Vec<&String> = r.iter().filter(|c| !c.is_empty()).collect();
    cells.len() >= 2 && cells.iter().all(|c| !is_num_cell(c))
}

/// 把 `rows` 按空行切成块，并识别「单格标题行」与「块自带表头行」。
fn block_views(t: &ResultTable) -> Vec<BlockView<'_>> {
    let mut blocks: Vec<Vec<&Vec<String>>> = Vec::new();
    let mut cur: Vec<&Vec<String>> = Vec::new();
    for r in &t.rows {
        if r.is_empty() {
            if !cur.is_empty() {
                blocks.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(r);
        }
    }
    if !cur.is_empty() {
        blocks.push(cur);
    }

    blocks
        .into_iter()
        .enumerate()
        .map(|(bi, mut b)| {
            let mut cap = None;
            if b.first()
                .map(|r| r.iter().filter(|c| !c.is_empty()).count() == 1)
                .unwrap_or(false)
            {
                let row = b.remove(0);
                cap = Some(
                    row.iter()
                        .find(|c| !c.is_empty())
                        .cloned()
                        .unwrap_or_default(),
                );
            }
            // 首块用全局列名；后续块只看「是不是真有一行表头」：
            // 全文字、至少两格、且块里再没有别的全文字行（否则那是数据行，
            // 例如「观测 1 / 簇 1」这种两列全文本的表身）。
            let mut head = if bi == 0 && !t.columns.is_empty() {
                Some(t.columns.clone())
            } else {
                None
            };
            let mut head_from_rows = false;
            if head.is_none()
                && b.len() >= 3
                && looks_like_header(b[0])
                && !b[1..].iter().any(|r| looks_like_header(r))
            {
                head = Some(b.remove(0).clone());
                head_from_rows = true;
            }
            BlockView {
                cap,
                head,
                head_from_rows,
                rows: b,
            }
        })
        .collect()
}

fn table_fonts() -> (FontId, FontId, FontId) {
    (
        // 正文
        FontId::proportional(text::LABEL + 0.3),
        // 表头（栏目名）
        FontId::proportional(text::MICRO),
        // 表题：与正文同级（规范：表题宋体五号加粗，不因是标题就缩号）
        FontId::proportional(text::LABEL),
    )
}

fn single_w(p: &egui::Painter, t: &str, f: &FontId) -> f32 {
    p.layout_no_wrap(t.to_owned(), f.clone(), egui::Color32::WHITE)
        .size()
        .x
}

/// 行高倍率（与字体度量无关地稳定：1.35 倍字号足够容纳中文与下标）。
fn line_h(f: &FontId) -> f32 {
    f.size * 1.35
}

/// 按像素宽度手动换行。
///
/// **为什么不用 `LayoutJob` 的自动换行**：`halign = Center` 时 egui 会把 galley
/// 宽度取成 `max_width`，再在内部居中——一旦某行略超宽，首字会被 galley 左边界
/// 切掉（「Control」显示成「ontrol」）。这里自己切行，每行都保证 ≤ max_w，
/// 再用 `Align2::CENTER_CENTER` 逐行画，位置完全可预测。
fn wrap_lines(p: &egui::Painter, text: &str, f: &FontId, max_w: f32) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    if single_w(p, text, f) <= max_w {
        return vec![text.to_string()];
    }
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut token = String::new();
    let is_word = |c: char| {
        c.is_ascii_alphanumeric()
            || matches!(c, '.' | '%' | '-' | '_' | '/' | ':' | '+' | '(' | ')')
    };
    for ch in text.chars() {
        if is_word(ch) {
            token.push(ch);
            if !cur.is_empty() && single_w(p, &format!("{cur}{token}"), f) > max_w {
                lines.push(std::mem::take(&mut cur));
            }
            continue;
        }
        // 词结束：整词落下（放不下就换行，不把词劈开）
        if !token.is_empty() {
            if !cur.is_empty() && single_w(p, &format!("{cur}{token}"), f) > max_w {
                lines.push(std::mem::take(&mut cur));
            }
            cur.push_str(&token);
            token.clear();
        }
        // 再加这个非词字符（中文逐字可断）
        if !cur.is_empty() && single_w(p, &format!("{cur}{ch}"), f) > max_w {
            lines.push(std::mem::take(&mut cur));
        }
        if ch == ' ' && cur.is_empty() {
            continue;
        }
        cur.push(ch);
    }
    if !token.is_empty() {
        if !cur.is_empty() && single_w(p, &format!("{cur}{token}"), f) > max_w {
            lines.push(std::mem::take(&mut cur));
        }
        cur.push_str(&token);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// 逐块算列宽：**该列最宽的内容 + 两侧留白**，不拉伸、不平均分配。
fn col_widths(
    p: &egui::Painter,
    v: &BlockView,
    columns: &[String],
    font: &FontId,
    hfont: &FontId,
    max_col: f32,
) -> Vec<f32> {
    // 表头按**实际会画的那一行**算宽度：后续块沿用全局列名时，
    // 列宽必须至少容得下列名，否则「组 / 来源」会被拆成两行。
    // （表题在表格外，不参与列宽。）
    let header: Option<&[String]> = v.header(columns);
    let n = v
        .rows
        .iter()
        .map(|r| r.len())
        .max()
        .unwrap_or(0)
        .max(header.map(|h| h.len()).unwrap_or(0));
    (0..n)
        .map(|i| {
            let mut w: f32 = 0.0;
            if let Some(h) = header {
                if let Some(c) = h.get(i) {
                    w = w.max(single_w(p, c, hfont));
                }
            }
            for r in &v.rows {
                if let Some(c) = r.get(i) {
                    w = w.max(single_w(p, c, font));
                }
            }
            (w + CELL_PAD_X * 2.0).clamp(44.0, max_col)
        })
        .collect()
}

fn row_height(p: &egui::Painter, row: &Vec<String>, widths: &[f32], font: &FontId) -> f32 {
    let lh = line_h(font);
    let mut lines = 1usize;
    for (i, c) in row.iter().enumerate() {
        let w = widths.get(i).copied().unwrap_or(60.0) - CELL_PAD_X * 2.0;
        if c.is_empty() || w <= 4.0 {
            continue;
        }
        lines = lines.max(wrap_lines(p, c, font, w).len());
    }
    (lines as f32 * lh + CELL_PAD_Y * 2.0).max(ROW_MIN_H)
}

fn head_height(p: &egui::Painter, head: &[String], widths: &[f32], hfont: &FontId) -> f32 {
    let lh = line_h(hfont);
    let mut lines = 1usize;
    for (i, c) in head.iter().enumerate() {
        let w = widths.get(i).copied().unwrap_or(60.0) - CELL_PAD_X * 2.0;
        if c.is_empty() || w <= 4.0 {
            continue;
        }
        lines = lines.max(wrap_lines(p, c, hfont, w).len());
    }
    (lines as f32 * lh + CELL_PAD_Y * 2.0).max(ROW_MIN_H)
}

fn block_height(
    p: &egui::Painter,
    v: &BlockView,
    widths: &[f32],
    font: &FontId,
    hfont: &FontId,
    columns: &[String],
) -> f32 {
    BLOCK_PAD_Y * 2.0
        + if v.cap.is_some() { CAP_H } else { 0.0 }
        + v.header(columns)
            .map(|h| head_height(p, h, widths, hfont))
            .unwrap_or(0.0)
        + v.rows
            .iter()
            .map(|r| row_height(p, r, widths, font))
            .sum::<f32>()
}

/// 结果表需要的高度（供外层卡片自适应；与 `result_table` 同一套几何）。
pub(crate) fn result_height(ui: &Ui, t: &ResultTable, avail: f32) -> f32 {
    let p = ui.painter();
    let (font, hfont, _) = table_fonts();
    let max_col = (avail * 0.58).clamp(120.0, 520.0);
    let views = block_views(t);
    let mut h = 0.0;
    for v in &views {
        let w = col_widths(p, v, &t.columns, &font, &hfont, max_col);
        h += block_height(p, v, &w, &font, &hfont, &t.columns);
    }
    h + BLOCK_GAP * views.len().saturating_sub(1) as f32
}

/// 统计结果表：**三线表** + 每块一个圆角卡底。
///
/// - 三条线：顶线 1.5 pt、栏目线 0.75 pt、底线 1.5 pt；无竖线、无行线。
/// - 线只跨**表格自身**的列宽（不拉到面板宽度），块之间用卡底分隔。
/// - 列宽 = 该列最宽内容 + 两侧留白，不拉伸填满；内容全部居中。
fn result_table(ui: &mut Ui, table: &ResultTable, pal: &Palette, avail: f32) {
    let (font, hfont, capfont) = table_fonts();
    let views = block_views(table);
    let max_col = (avail * 0.58).clamp(120.0, 520.0);
    let (widths, heights): (Vec<Vec<f32>>, Vec<f32>) = {
        let p = ui.painter();
        let widths: Vec<Vec<f32>> = views
            .iter()
            .map(|v| col_widths(p, v, &table.columns, &font, &hfont, max_col))
            .collect();
        let heights: Vec<f32> = views
            .iter()
            .zip(widths.iter())
            .map(|(v, w)| block_height(p, v, w, &font, &hfont, &table.columns))
            .collect();
        (widths, heights)
    };
    let total_h: f32 =
        heights.iter().sum::<f32>() + BLOCK_GAP * views.len().saturating_sub(1) as f32;
    let total_w: f32 = widths
        .iter()
        .map(|w| w.iter().sum::<f32>())
        .fold(60.0, f32::max);

    ScrollArea::both()
        .id_salt("results_scroll")
        .max_height(total_h + 12.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(total_w, total_h.max(1.0) + 2.0), Sense::hover());
            let p = ui.painter_at(rect);
            let mut y = rect.top();

            for (bi, v) in views.iter().enumerate() {
                let w = &widths[bi];
                let wsum: f32 = w.iter().sum();
                // 表题在**表格外**、顶线之上（GB/T 7713：表序、表题排在顶线的上方，
                // 不能塞进卡片/顶线以内）。
                let mut top = y;
                if let Some(cap) = &v.cap {
                    p.text(
                        egui::pos2(rect.left(), top + CAP_H * 0.5),
                        Align2::LEFT_CENTER,
                        cap,
                        capfont.clone(),
                        pal.muted,
                    );
                    top += CAP_H;
                }
                let card_h = heights[bi] - if v.cap.is_some() { CAP_H } else { 0.0 };
                let card =
                    Rect::from_min_size(egui::pos2(rect.left(), top), Vec2::new(wsum, card_h));
                // 卡底只是「分区」，不是第四条线：用底色，不加描边。
                surface(&p, card, 6.0, pal.bg, Stroke::NONE);

                let rule_x = egui::Rangef::new(card.left() + 2.0, card.right() - 2.0);
                p.hline(
                    rule_x,
                    card.top() + 3.0,
                    Stroke::new(RULE_TOP_BOTTOM, pal.ink.gamma_multiply(0.72)),
                );

                let mut cy = card.top() + BLOCK_PAD_Y;
                let mut idx = 0usize;

                if let Some(head) = v.header(&table.columns) {
                    let hh = head_height(&p, head, w, &hfont);
                    let mut x = card.left();
                    for (i, c) in head.iter().enumerate() {
                        let cw = w.get(i).copied().unwrap_or(0.0);
                        let r = Rect::from_min_size(egui::pos2(x, cy), Vec2::new(cw, hh));
                        draw_cell(&p, r, c, &hfont, pal.muted);
                        x += cw;
                    }
                    cy += hh;
                    p.hline(
                        rule_x,
                        cy,
                        Stroke::new(RULE_HEADER, pal.ink.gamma_multiply(0.62)),
                    );
                    if v.head_from_rows {
                        idx += 1;
                    }
                }

                for row in &v.rows[idx..] {
                    let rh = row_height(&p, row, w, &font);
                    let mut x = card.left();
                    for (i, c) in row.iter().enumerate() {
                        let cw = w.get(i).copied().unwrap_or(0.0);
                        let r = Rect::from_min_size(egui::pos2(x, cy), Vec2::new(cw, rh));
                        draw_cell(&p, r, c, &font, pal.ink);
                        x += cw;
                    }
                    cy += rh;
                }

                p.hline(
                    rule_x,
                    card.bottom() - 3.0,
                    Stroke::new(RULE_TOP_BOTTOM, pal.ink.gamma_multiply(0.72)),
                );
                y = card.bottom() + BLOCK_GAP;
            }
        });
}

/// 单元格内容：水平 + 垂直居中，超宽按词/字换行（不截断成省略号）。
fn draw_cell(p: &egui::Painter, r: Rect, text: &str, font: &FontId, color: egui::Color32) {
    if text.is_empty() {
        return;
    }
    let lines = wrap_lines(p, text, font, r.width() - CELL_PAD_X * 2.0);
    if lines.is_empty() {
        return;
    }
    let lh = line_h(font);
    let total = lines.len() as f32 * lh;
    let mut y = r.center().y - total / 2.0 + lh / 2.0;
    for line in &lines {
        p.text(
            egui::pos2(r.center().x, y),
            Align2::CENTER_CENTER,
            line,
            font.clone(),
            color,
        );
        y += lh;
    }
}
