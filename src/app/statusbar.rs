//! 底部状态栏：当前位置、光标、渲染耗时、保存状态。
//!
//! 左右两簇分别贴 `BAR_PAD_X`，中间内容按测量结果顺排，
//! 不再使用 430 / 280 这类魔法偏移。

use egui::{Align2, FontId, Rect, Stroke, Ui};

use crate::app::theme::{metric, text};
use crate::app::{App, Sheet};

impl App {
    pub fn statusbar(&mut self, ui: &mut Ui, rect: Rect) {
        let pal = self.pal;
        ui.painter().rect_filled(rect, 0, pal.panel);
        ui.painter().hline(
            rect.x_range(),
            rect.top() + 0.5,
            Stroke::new(1.0, pal.border),
        );

        let left = match self.sel {
            Some(Sheet::Table(id)) => self.project.table(id).map(|t| {
                format!(
                    "{}  ·  {}  ·  {} 行 × {} 列",
                    t.name,
                    t.kind.label(),
                    t.nrows(),
                    t.columns.len()
                )
            }),
            Some(Sheet::Graph(id)) => self
                .project
                .graph(id)
                .map(|g| format!("{}  ·  {}", g.name, g.spec.kind.label())),
            Some(Sheet::Layout(id)) => self
                .project
                .layout(id)
                .map(|l| format!("{}  ·  拼版  ·  {} 面板", l.name, l.panels.len())),
            Some(Sheet::Analysis(id)) => self
                .project
                .analysis(id)
                .map(|a| format!("{}  ·  {}", a.name, a.kind.label())),
            None => Some("未选择工作表".into()),
        }
        .unwrap_or_default();

        let font = FontId::proportional(text::MICRO + 0.8);
        let p = ui.painter();
        let mut x = rect.left() + metric::BAR_PAD_X;
        // 分隔点：左右各 11px，全场统一，避免「有的地方宽有的地方窄」。
        let sep = |x: &mut f32| {
            p.circle_filled(egui::pos2(*x + 11.0, rect.center().y), 1.5, pal.faint);
            *x += 22.0;
        };

        // ---- 左簇：当前位置 ----
        let galley = p.layout_no_wrap(left, font.clone(), pal.muted);
        let w = galley.size().x;
        p.galley(
            egui::pos2(x, rect.center().y - galley.size().y / 2.0),
            galley,
            pal.muted,
        );
        x += w;

        // ---- 光标位置与表信息（仅数据表） ----
        if let Some(Sheet::Table(_)) = self.sel {
            let (r, c) = self.grid.cursor;
            let seg = format!("R{}:C{}", r + 1, c + 1);
            let g = p.layout_no_wrap(seg, font.clone(), pal.faint);
            sep(&mut x);
            p.galley(
                egui::pos2(x, rect.center().y - g.size().y / 2.0),
                g.clone(),
                pal.faint,
            );
            x += g.size().x;

            let extra = self
                .project
                .table(self.current_table_id().unwrap_or(0))
                .map(|t| {
                    let excluded = t
                        .columns
                        .iter()
                        .flat_map(|c| c.cells.iter())
                        .filter(|c| c.excluded)
                        .count();
                    format!(
                        "{} 组  ·  已排除 {} 个值",
                        t.analysis_groups().len(),
                        excluded
                    )
                })
                .unwrap_or_default();
            if !extra.is_empty() {
                sep(&mut x);
                p.text(
                    egui::pos2(x, rect.center().y),
                    Align2::LEFT_CENTER,
                    extra,
                    font.clone(),
                    pal.faint,
                );
            }
        }

        // ---- 右簇：保存状态 → 渲染耗时 → 版本 ----
        let right = format!(
            "渲染 {:.0} ms  ·  v{}",
            self.chart.last_ms,
            env!("CARGO_PKG_VERSION")
        );
        let rg = p.layout_no_wrap(right, font.clone(), pal.faint);
        let rw = rg.size().x;
        let right_x = rect.right() - metric::BAR_PAD_X;
        p.galley(
            egui::pos2(right_x - rw, rect.center().y - rg.size().y / 2.0),
            rg,
            pal.faint,
        );

        let state = if self.dirty { "未保存" } else { "已保存" };
        let sg = p.layout_no_wrap(state.to_owned(), font.clone(), pal.faint);
        let sw = sg.size().x;
        let dot_x = right_x - rw - 22.0 - sw - 9.0;
        p.circle_filled(
            egui::pos2(dot_x, rect.center().y),
            3.0,
            if self.dirty { pal.danger } else { pal.success },
        );
        p.galley(
            egui::pos2(dot_x + 9.0, rect.center().y - sg.size().y / 2.0),
            sg,
            pal.faint,
        );
    }
}
