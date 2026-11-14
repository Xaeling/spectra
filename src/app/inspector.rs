//! 右侧检查器：图表样式、数据表属性、分析结果信息。
//!
//! 全栏只有一条左边界（内容区左沿）与一条控件列左沿；标签右对齐到标签列，
//! 控件右边缘与内容区右沿重合。

use egui::{Color32, FontId, Rect, RichText, Stroke, Ui, Vec2};

use crate::app::form::{self, Form};
use crate::app::theme::{metric, radius, text, Palette};
use crate::app::widgets::{self, kind_tile, surface};
use crate::app::{App, Sheet, ToastKind};
use crate::core::curve::{fit_all, pts_key, FitModel};
use crate::core::graph::{ChartKind, ErrorKind, LegendPos, Marker, PlotThemeId};

impl App {
    pub fn inspector_view(&mut self, ui: &mut Ui, rect: Rect) {
        let pal = self.pal;
        ui.painter().rect_filled(rect, 0, pal.panel);
        ui.painter().vline(
            rect.left() + 0.5,
            rect.y_range(),
            Stroke::new(1.0, pal.border),
        );
        let tag = match self.sel {
            Some(Sheet::Graph(_)) => "图表",
            Some(Sheet::Table(_)) => "数据表",
            Some(Sheet::Analysis(_)) => "分析",
            Some(Sheet::Layout(_)) => "拼版",
            None => "",
        };
        let head = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 46.0));
        let head_h = form::panel_header(ui, head, "检查器", tag, &pal);
        let body = Rect::from_min_max(egui::pos2(rect.left(), rect.top() + head_h), rect.max);

        form::scroll_body(ui, body, pal, |ui| match self.sel {
            Some(Sheet::Graph(_)) => self.graph_inspector(ui),
            Some(Sheet::Table(id)) => self.table_inspector(ui, id),
            Some(Sheet::Analysis(id)) => self.analysis_inspector(ui, id),
            Some(Sheet::Layout(id)) => self.layout_inspector(ui, id),
            None => {
                let mut f = Form::new(ui, pal);
                f.gap(6.0);
                f.note("在左侧选择一张数据表或图表开始。");
            }
        });
    }

    fn graph_inspector(&mut self, ui: &mut Ui) {
        let Some(gid) = self.current_graph_id() else {
            return;
        };
        let Some(orig) = self.project.graph(gid).map(|g| g.spec.clone()) else {
            return;
        };
        let table_id = self.project.graph(gid).map(|g| g.table);
        // 图型库的唯一过滤条件：当前图表绑定的数据表类型。
        let table_kind = table_id
            .and_then(|tid| self.project.table(tid))
            .map(|t| t.kind);
        let (gname, gkind, gtable) = self
            .project
            .graph(gid)
            .map(|g| (g.name.clone(), g.spec.kind.label(), g.table))
            .unwrap_or_default();
        let tname = self
            .project
            .table(gtable)
            .map(|t| t.name.clone())
            .unwrap_or_default();
        let mut spec = orig.clone();
        let pal = self.pal;
        let mut auto_clicked = false;
        let mut reset = false;

        {
            let mut f = Form::new(ui, pal);
            f.gap(2.0);
            heading(
                f.ui(),
                &gname,
                &format!("{}  ·  数据表：{}", gkind, tname),
                &pal,
            );
            f.gap(2.0);

            // ---- 图型：只列出当前表格类型能诚实表达的图型 ----
            let groups = table_kind
                .map(ChartKind::groups_for)
                .unwrap_or_else(|| Vec::new());
            f.section_count("图型", table_kind.map(ChartKind::count_for).unwrap_or(0));
            let gap = 6.0;
            let tile_h = 42.0;
            for (fam, kinds) in &groups {
                f.full(14.0, |ui| {
                    ui.painter().text(
                        egui::pos2(ui.max_rect().left(), ui.max_rect().top() + 1.0),
                        egui::Align2::LEFT_TOP,
                        *fam,
                        FontId::proportional(text::MICRO - 0.5),
                        pal.faint,
                    );
                });
                for chunk in kinds.chunks(4) {
                    f.full(tile_h, |ui| {
                        let avail = ui.available_width();
                        let n = chunk.len() as f32;
                        let tw = (avail - gap * (n - 1.0)) / n;
                        ui.spacing_mut().item_spacing.x = gap;
                        for k in chunk {
                            let sel = spec.kind == *k;
                            if kind_tile(ui, *k, sel, Vec2::new(tw, tile_h), &pal).clicked() {
                                spec.kind = *k;
                            }
                        }
                    });
                }
            }
            // 旧工程文件里可能存在「图型 / 表格」的非法组合：给出明路，
            // 而不是让用户对着一张报错的图束手无策。
            if let Some(tk) = table_kind {
                if !spec.kind.supports(tk) {
                    f.gap(4.0);
                    f.note(&format!(
                        "当前「{}」不适用于{}表，请改用上方可用图型。",
                        spec.kind.label(),
                        tk.label()
                    ));
                }
            }

            // ---- 数据 ----
            f.section("数据");
            if spec.kind.shows_error() {
                f.combo("误差棒", spec.error.label(), |ui| {
                    for e in ErrorKind::ALL {
                        ui.selectable_value(&mut spec.error, e, e.label());
                    }
                });
            }
            if spec.kind.points_toggleable() {
                // 序列太密时渲染器会自动只画线（点会叠成毛毛虫），这里说清楚，
                // 否则用户会以为「叠加散点」这个开关坏了。
                let densest = table_id
                    .and_then(|tid| self.project.table(tid))
                    .map(|t| {
                        t.analysis_groups()
                            .iter()
                            .map(|(_, v)| v.len())
                            .max()
                            .unwrap_or(0)
                    })
                    .unwrap_or(0);
                let too_dense = densest > crate::plot::chart::MAX_MARKERS;
                f.check("叠加散点", &mut spec.show_points);
                if too_dense {
                    f.note(&format!(
                        "单序列已有 {} 个点，超过 {} 个时只画线——逐点标记会叠在一起盖住曲线。",
                        densest,
                        crate::plot::chart::MAX_MARKERS
                    ));
                }
                if spec.show_points && spec.kind.jitter_toggleable() && !too_dense {
                    f.check("散点抖动", &mut spec.jitter);
                }
                f.slider("点大小", &mut spec.point_size, 1.0..=10.0, 1);
            } else if spec.kind.shows_points() {
                // 硬性规定画点（散点图本身 / 回归的原始观测）：不给开关，
                // 只说明点必在图上，避免用户以为开关失效。
                f.note("原始观测为该图型的固定元素，无需开关。");
            }
            // ---- 拟合 ----
            if spec.kind.fit_toggleable() {
                f.section("拟合模型");
                f.combo("模型", spec.fit_model.label(), |ui| {
                    for m in FitModel::ALL {
                        ui.selectable_value(&mut spec.fit_model, m, m.label());
                    }
                });
                f.note(spec.fit_model.formula());
                // 「怎么判断」：把所有模型按 AICc 跑一遍给出排名，推荐最优。
                //
                // **必须走缓存**：fit_all 是每个模型几十组初值的 LM，每帧重跑一次
                // 会把整个界面拖成幻灯片（回归图独有卡顿就是这段代码）。
                // 数据指纹不变就直接复用，换表/改数才重算。
                let pts: Vec<(f64, f64)> = table_id
                    .and_then(|tid| self.project.table(tid))
                    .map(|t| {
                        (0..t.group_names.len())
                            .flat_map(|g| t.xy_observations(g))
                            .collect()
                    })
                    .unwrap_or_default();
                if pts.len() >= 5 {
                    let key = pts_key(&pts);
                    if self.fit_cache.as_ref().map(|(k, _)| *k) != Some(key) {
                        self.fit_cache = Some((key, fit_all(&pts)));
                    }
                    if let Some((_, fits)) = &self.fit_cache {
                        if let Some(best) = fits.first() {
                            let top: Vec<String> = fits
                                .iter()
                                .take(3)
                                .map(|m| format!("{} (AICc {:.1})", m.model.label(), m.aicc))
                                .collect();
                            f.note(&format!(
                                "按 AICc 推荐：{}，R² = {:.4}。排名前三：{}",
                                best.model.label(),
                                best.r2,
                                top.join(" › ")
                            ));
                            if best.model != spec.fit_model {
                                f.note("当前选择与推荐不同，按你的选择画。");
                            }
                        }
                        // 参数不可辨识等警告也在这里给出——图上只留方程/R²/p/n，
                        // 长句印在图上既碍观感又不是规范要求的元素。
                        if let Some(cur) = fits.iter().find(|m| m.model == spec.fit_model) {
                            for w in &cur.warnings {
                                f.note(&format!("⚠ {w}"));
                            }
                        }
                    }
                }
                if spec.fit_model != FitModel::Linear {
                    f.note("非线性模型只画拟合曲线，不画置信带（需要 bootstrap 协方差，后续支持）；线性模型给 95% 均值响应带。");
                }
            }

            // ---- 双 Y 轴 ----
            if spec.kind.dual_y_toggleable() {
                f.section("双 Y 轴");
                f.check("启用右侧 Y 轴", &mut spec.dual_y);
                if spec.dual_y {
                    f.entry("右轴标题", &mut spec.y2_title);
                    f.note("第二个序列起映射到右轴；两条轴刻度独立，请在图注写明各自量纲。");
                }
            }

            // ---- 分面 ----
            if spec.kind.facet_toggleable() {
                let n_groups = table_id
                    .and_then(|tid| self.project.table(tid))
                    .map(|t| t.analysis_groups().len())
                    .unwrap_or(0);
                f.check("分面（每组一个小面板）", &mut spec.facet);
                if spec.facet {
                    f.note("多组各自成面板、共享 Y 尺度，避免颜色堆在一起（规范 G8 的做法）。");
                    f.check("自由 Y 轴（每面板）", &mut spec.facet_free_y);
                    if spec.facet_free_y {
                        f.note(
                            "每个面板按自己的数据定量程：形状清楚，但**高低不可直接比较**。只在各组量级差很多时用，图注要写明。",
                        );
                    } else {
                        f.note("所有面板共用同一 Y 值域：同一个高度 = 同一个值，可直接比高低。");
                        // 「Y 轴怎么定的」讲清楚：值域取自**全部面板的数据**（含误差棒、
                        // 小提琴支撑），不是各面板各自的范围。
                        if let Some((_, ax)) = self.chart.axes {
                            f.note(&format!(
                                "当前共享 Y 值域：{} – {}（由所有组的数据极值决定）。",
                                crate::core::model::format_number(ax.min),
                                crate::core::model::format_number(ax.max)
                            ));
                        }
                    }
                } else if n_groups > 6 {
                    f.note(&format!(
                        "规范 G8 建议：分组数 {n_groups} > 6 时分面，否则颜色会堆在一起互相穿透。开不开由你定。"
                    ));
                }
            }

            if spec.kind.shows_line() {
                let marks: Vec<(Marker, &str)> =
                    Marker::ALL.iter().map(|m| (*m, m.label())).collect();
                f.segmented("符号", &mut spec.marker, &marks);
                f.slider("线宽", &mut spec.line_width, 0.4..=4.0, 1);
            }
            if spec.kind.shows_width() {
                f.slider("柱宽", &mut spec.bar_width, 0.2..=0.95, 2);
            }
            f.note(spec.kind.requires());

            // ---- 样式 ----
            f.section("样式");
            f.combo("主题", spec.theme.label(), |ui| {
                for t in PlotThemeId::ALL {
                    ui.selectable_value(&mut spec.theme, t, t.label());
                }
            });
            f.combo("配色", spec.palette.label(), |ui| {
                // 31 套按分类分组显示，避免拉成一条长清单。
                for g in crate::core::palette::PaletteGroup::ALL {
                    let items = g.palettes();
                    if items.is_empty() {
                        continue;
                    }
                    ui.weak(g.label());
                    for p in items {
                        let sel = spec.palette == p;
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 5.0;
                            let (r, resp) =
                                ui.allocate_exact_size(Vec2::new(26.0, 12.0), egui::Sense::click());
                            // 该方案的前 4 色做色条预览
                            let cs = p.colors();
                            let n = cs.len().min(4) as f32;
                            let sw = r.width() / n;
                            for (k, c) in cs.iter().take(4).enumerate() {
                                surface(
                                    ui.painter(),
                                    Rect::from_min_size(
                                        egui::pos2(r.left() + sw * k as f32, r.top()),
                                        Vec2::new(sw, r.height()),
                                    ),
                                    0.0,
                                    Color32::from_rgb(c.r, c.g, c.b),
                                    Stroke::NONE,
                                );
                            }
                            if resp.clicked() {
                                spec.palette = p;
                                ui.close();
                            }
                            if ui.selectable_label(sel, p.label()).clicked() {
                                spec.palette = p;
                                ui.close();
                            }
                        });
                    }
                    ui.add_space(3.0);
                }
            });
            f.full(18.0, |ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                let colors = spec.palette.colors();
                let n = colors.len().min(10) as f32;
                let w = (ui.available_width() - 5.0 * (n - 1.0)) / n;
                for c in colors.iter().take(10) {
                    let (r, _) = ui.allocate_exact_size(Vec2::new(w, 18.0), egui::Sense::hover());
                    surface(
                        ui.painter(),
                        r,
                        radius::CHIP,
                        Color32::from_rgb(c.r, c.g, c.b),
                        Stroke::NONE,
                    );
                }
            });
            f.check("网格线", &mut spec.show_grid);
            f.check("图内标注", &mut spec.annotations_visible);
            if !spec.annotations_visible {
                f.note("已隐藏：方程 / R² / p / n、误差棒度量说明、显著性括号与图注统计行。数据与坐标轴不动。");
            }
            f.check("显示图例", &mut spec.show_legend);
            if spec.show_legend {
                f.combo("图例位置", spec.legend_pos.label(), |ui| {
                    for p in [
                        LegendPos::InsideTopRight,
                        LegendPos::InsideTopLeft,
                        LegendPos::OutsideRight,
                        LegendPos::Bottom,
                    ] {
                        ui.selectable_value(&mut spec.legend_pos, p, p.label());
                    }
                });
                // 填到右上角的色块会把图例压在数据上，这种组合会自动外置。
                // 控件的值没变，所以必须说清楚，否则用户会以为设置没生效。
                if spec.kind.filled_marks() && spec.legend_pos == LegendPos::InsideTopRight {
                    f.note("柱/面积类图型的色块会占住右上角，图例自动改到右侧外部（约 4pt 宽）。");
                }
            }

            // ---- 坐标轴 ----
            // 饼 / 环没有坐标轴，热图没有轴标题：这几类不显示该分组。
            let has_axes = !matches!(
                spec.kind,
                ChartKind::Pie | ChartKind::Donut | ChartKind::Heatmap
            );
            if has_axes {
                f.section("坐标轴");
                f.entry("X 轴标题", &mut spec.x_title);
                f.entry("Y 轴标题", &mut spec.y_title);
                f.check_pair(("X 对数轴", &mut spec.x_log), ("Y 对数轴", &mut spec.y_log));
                let mut auto_x = spec.x_range.is_none();
                if f.check("X 自动范围", &mut auto_x).changed() {
                    spec.x_range = if auto_x {
                        None
                    } else {
                        self.chart
                            .axes
                            .map(|(x, _)| [x.min, x.max])
                            .or(Some([0.0, 10.0]))
                    };
                }
                if let Some(r) = &mut spec.x_range {
                    f.range_row("X 范围", r);
                }
                let mut auto_y = spec.y_range.is_none();
                if f.check("Y 自动范围", &mut auto_y).changed() {
                    spec.y_range = if auto_y {
                        None
                    } else {
                        self.chart
                            .axes
                            .map(|(_, y)| [y.min, y.max])
                            .or(Some([0.0, 40.0]))
                    };
                }
                if let Some(r) = &mut spec.y_range {
                    f.range_row("Y 范围", r);
                }
            } else if matches!(spec.kind, ChartKind::Pie | ChartKind::Donut) {
                f.section("图例");
                f.check("显示图例与百分比", &mut spec.show_legend);
                f.note("扇区按数值占比分配角度；负值无法用饼图表达。");
            }

            // ---- 显著性 ----
            if has_axes {
                f.section_count("显著性标注", spec.annotations.len());
                if spec.annotations.is_empty() {
                    f.note("还没有括号。点下方按钮按检验结果自动添加。");
                } else {
                    let anns = spec.annotations.clone();
                    let mut remove = None;
                    for (i, a) in anns.iter().enumerate() {
                        f.full(26.0, |ui| {
                            let w = ui.available_width();
                            let (rect, resp) =
                                ui.allocate_exact_size(Vec2::new(w, 26.0), egui::Sense::click());
                            if resp.hovered() {
                                surface(ui.painter(), rect, radius::CHIP, pal.hover, Stroke::NONE);
                            }
                            ui.painter().text(
                                egui::pos2(rect.left() + 2.0, rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                format!("{} 与 {}  ·  {}", a.a + 1, a.b + 1, a.label),
                                FontId::proportional(text::LABEL),
                                pal.ink,
                            );
                            let btn = Rect::from_center_size(
                                egui::pos2(rect.right() - 10.0, rect.center().y),
                                Vec2::splat(20.0),
                            );
                            if ui
                                .interact(
                                    btn,
                                    ui.scope_id().with(("del_ann", i)),
                                    egui::Sense::click(),
                                )
                                .clicked()
                            {
                                remove = Some(i);
                            }
                            ui.painter().text(
                                btn.center(),
                                egui::Align2::CENTER_CENTER,
                                "×",
                                FontId::proportional(12.0),
                                if btn.contains(
                                    ui.ctx()
                                        .pointer_hover_pos()
                                        .unwrap_or(egui::pos2(-1.0, -1.0)),
                                ) {
                                    pal.danger
                                } else {
                                    pal.faint
                                },
                            );
                        });
                    }
                    if let Some(i) = remove {
                        spec.annotations.remove(i);
                    }
                }
                f.gap(6.0);
                if f.action("按统计结果自动生成", false).clicked() {
                    auto_clicked = true;
                }
                if !spec.annotations.is_empty() && f.action("清空全部标注", true).clicked() {
                    spec.annotations.clear();
                }
            }
            // ---- 出版规范：把「科研配图规范」变成可执行检查 ----
            f.section("出版规范");
            let theme = crate::plot::theme::Theme::for_id(spec.theme);
            // 用「这张图实际画了多少组」做色盲检查，而不是把整块调色板都算上。
            let series = table_id
                .and_then(|tid| self.project.table(tid))
                .map(|t| t.analysis_groups().len());
            let issues = crate::plot::spec::audit_series(
                &spec,
                &theme,
                crate::app::canvas::W_PT,
                crate::app::canvas::H_PT,
                series,
            );
            if issues.is_empty() {
                f.note("字号、线宽、配色、轴标题均符合投稿要求。");
            } else {
                for it in &issues {
                    let color = match it.level {
                        crate::plot::spec::Level::Error => pal.danger,
                        crate::plot::spec::Level::Warn => pal.warn,
                        crate::plot::spec::Level::Info => pal.faint,
                    };
                    f.full(20.0, |ui| {
                        let w = ui.available_width();
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(w, 20.0), egui::Sense::hover());
                        ui.painter().text(
                            egui::pos2(rect.left() + 2.0, rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            format!("{} · {}", it.level.label(), it.field),
                            FontId::proportional(text::LABEL),
                            color,
                        );
                    });
                    f.note(&it.message);
                    f.note(&format!("→ {}", it.fix));
                }
            }

            f.gap(10.0);
            if f.action("恢复默认样式", false).clicked() {
                reset = true;
            }
        }

        if reset {
            if let Some(t) = table_id.and_then(|tid| self.project.table(tid)) {
                spec = crate::core::graph::GraphSpec::default_for(t);
            }
        }
        if spec != orig {
            if let Some(g) = self.project.graph_mut(gid) {
                g.spec = spec;
            }
            self.bump_chart();
        }
        if auto_clicked {
            self.auto_significance();
        }
    }

    fn layout_inspector(&mut self, ui: &mut Ui, id: u64) {
        use crate::core::model::LabelCase;
        let pal = self.pal;
        let Some(page) = self.project.layout(id).cloned() else {
            return;
        };
        let mut f = Form::new(ui, pal);
        f.gap(6.0);
        f.section("拼版页");

        // 名称
        let mut name = page.name.clone();
        f.entry("名称", &mut name);
        if name != page.name {
            self.snapshot();
            if let Some(p) = self.project.layout_mut(id) {
                p.name = name;
            }
            self.bump_chart();
        }

        // 列数：1–3（规范 G8 拼版常见 2×2 / 2×3）
        let mut cols_f = page.cols.clamp(1, 3) as f64;
        f.slider("列数", &mut cols_f, 1.0..=3.0, 1);
        let cols = cols_f as usize;
        if cols != page.cols {
            self.snapshot();
            if let Some(p) = self.project.layout_mut(id) {
                p.cols = cols;
            }
            self.bump_chart();
        }

        f.section("图注编号");
        let mut show_labels = page.show_labels;
        f.check("面板左上角标 a/b/c", &mut show_labels);
        if show_labels != page.show_labels {
            self.snapshot();
            if let Some(p) = self.project.layout_mut(id) {
                p.show_labels = show_labels;
            }
            self.bump_chart();
        }
        // 本地 pick 由闭包原地改（与 graph_inspector 的 fit_model 同套路）。
        let mut pick = page.label_case;
        f.combo(
            "编号样式",
            match page.label_case {
                LabelCase::Lower => "a, b, c",
                LabelCase::Upper => "A, B, C",
                LabelCase::Number => "1, 2, 3",
            },
            |ui| {
                for (v, lab) in [
                    (LabelCase::Lower, "a, b, c"),
                    (LabelCase::Upper, "A, B, C"),
                    (LabelCase::Number, "1, 2, 3"),
                ] {
                    ui.selectable_value(&mut pick, v, lab);
                }
            },
        );
        if pick != page.label_case {
            self.snapshot();
            if let Some(p) = self.project.layout_mut(id) {
                p.label_case = pick;
            }
            self.bump_chart();
        }

        f.section("面板（按阅读顺序）");
        let graph_names: Vec<(u64, String)> = self
            .project
            .graphs
            .iter()
            .map(|g| (g.id, g.name.clone()))
            .collect();
        let mut remove: Option<usize> = None;
        let mut move_up: Option<usize> = None;
        let mut move_down: Option<usize> = None;
        for (i, gid) in page.panels.iter().enumerate() {
            let label = graph_names
                .iter()
                .find(|(id, _)| id == gid)
                .map(|(_, n)| n.clone())
                .unwrap_or_else(|| "（图已删除）".into());
            let tag = if page.show_labels {
                format!("{}. {}", page.label_case.label(i), label)
            } else {
                label
            };
            f.row(&format!("{}", i + 1), |ui| {
                ui.label(RichText::new(tag).size(text::BODY));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("✕").clicked() {
                        remove = Some(i);
                    }
                    if ui.small_button("↓").clicked() {
                        move_down = Some(i);
                    }
                    if ui.small_button("↑").clicked() {
                        move_up = Some(i);
                    }
                });
            });
        }

        // 添加面板
        let mut add: Option<u64> = None;
        f.row("添加", |ui| {
            egui::ComboBox::from_id_salt("layout_add_panel")
                .width(ui.available_width())
                .height(metric::COMBO_POPUP_H)
                .selected_text(RichText::new("选择一张图…").size(text::BODY))
                .show_ui(ui, |ui| {
                    for (gid, gname) in &graph_names {
                        if ui
                            .selectable_label(false, RichText::new(gname).size(text::BODY))
                            .clicked()
                        {
                            add = Some(*gid);
                        }
                    }
                });
        });

        if remove.is_some() || move_up.is_some() || move_down.is_some() || add.is_some() {
            self.snapshot();
            if let Some(p) = self.project.layout_mut(id) {
                if let Some(i) = remove {
                    p.panels.remove(i);
                }
                if let Some(i) = move_up {
                    if i > 0 {
                        p.panels.swap(i, i - 1);
                    }
                }
                if let Some(i) = move_down {
                    if i + 1 < p.panels.len() {
                        p.panels.swap(i, i + 1);
                    }
                }
                if let Some(gid) = add {
                    p.panels.push(gid);
                }
            }
            self.bump_chart();
        }

        f.note("面板按顺序从左到右、从上到下排布；图注编号供正文引用「图 2a」。");
    }

    fn table_inspector(&mut self, ui: &mut Ui, id: u64) {
        let pal = self.pal;
        let Some(t) = self.project.table(id) else {
            return;
        };
        let name = t.name.clone();
        let kind = t.kind;
        let rows = t.nrows();
        let cols = t.columns.len();
        let groups = t.analysis_groups().len();
        let excluded = t
            .columns
            .iter()
            .flat_map(|c| c.cells.iter())
            .filter(|c| c.excluded)
            .count();
        let notes = t.notes.clone();

        let mut f = Form::new(ui, pal);
        f.gap(2.0);
        heading(
            f.ui(),
            &name,
            &format!("{}  ·  {} 行 × {} 列", kind.label(), rows, cols),
            &pal,
        );
        f.gap(4.0);

        f.section("表属性");
        f.row("数据组", |ui| {
            ui.label(
                RichText::new(groups.to_string())
                    .size(text::BODY)
                    .color(pal.ink),
            );
        });
        f.row("排除值", |ui| {
            ui.label(
                RichText::new(excluded.to_string())
                    .size(text::BODY)
                    .color(if excluded > 0 {
                        pal.accent_ink
                    } else {
                        pal.ink
                    }),
            );
        });
        f.note(kind.input_rule());
        f.note("右键单元格可排除 / 恢复，双击直接编辑。");

        f.section("操作");
        if f.action("在末尾添加一行", false).clicked() {
            self.snapshot();
            if let Some(t) = self.project.table_mut(id) {
                let n = t.nrows();
                t.ensure_rows(n + 1);
            }
            self.dirty = true;
            self.bump_chart();
        }
        if f.action("添加列 / 组", false).clicked() {
            self.add_column(id);
        }
        if f.action("导出 CSV…", false).clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("CSV", &["csv"])
                .set_file_name(format!("{name}.csv"))
                .save_file()
            {
                if let Some(t) = self.project.table(id) {
                    match crate::io::export_csv(t, &path) {
                        Ok(()) => self.toast("CSV 已导出", ToastKind::Success),
                        Err(e) => self.toast(format!("导出失败：{e}"), ToastKind::Error),
                    }
                }
            }
        }
        if f.action("清空整表", true).clicked() {
            self.snapshot();
            if let Some(t) = self.project.table_mut(id) {
                for c in &mut t.columns {
                    for cell in &mut c.cells {
                        *cell = crate::core::model::Cell::empty();
                    }
                }
            }
            self.dirty = true;
            self.bump_chart();
        }

        f.section("计算列");
        // 现有计算列：名字 + 公式（只读展示，✕ 删除整列）
        let formula_cols: Vec<(usize, String, String)> = self
            .project
            .table(id)
            .map(|t| {
                t.columns
                    .iter()
                    .enumerate()
                    .filter_map(|(i, c)| c.formula.clone().map(|f| (i, c.name.clone(), f)))
                    .collect()
            })
            .unwrap_or_default();
        let mut del: Option<usize> = None;
        for (idx, nm, fm) in &formula_cols {
            f.row(nm, |ui| {
                ui.label(RichText::new(fm).size(text::META).color(pal.muted));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("✕").clicked() {
                        del = Some(*idx);
                    }
                });
            });
        }
        if formula_cols.is_empty() {
            f.note("公式如 {体重}/({身高}^2)：引用列用花括号，支持 + - * / % ^、括号与 ln/log10/sqrt/exp 等函数。");
        }
        f.entry("列名", &mut self.calc_name);
        f.entry("公式", &mut self.calc_formula);
        if f.action("添加计算列", false).clicked() {
            let name = self.calc_name.trim().to_string();
            let formula = self.calc_formula.trim().to_string();
            if name.is_empty() || formula.is_empty() {
                self.toast("请填写列名与公式", ToastKind::Info);
            } else {
                self.snapshot();
                let r = self
                    .project
                    .table_mut(id)
                    .map(|t| t.add_computed_column(&name, &formula));
                match r {
                    Some(Ok(_)) => {
                        self.calc_name.clear();
                        self.calc_formula.clear();
                        self.dirty = true;
                        self.bump_chart();
                        self.toast("已添加计算列", ToastKind::Success);
                    }
                    Some(Err(e)) => self.toast(format!("公式错误：{e}"), ToastKind::Error),
                    None => {}
                }
            }
        }
        if let Some(idx) = del {
            self.snapshot();
            if let Some(t) = self.project.table_mut(id) {
                if idx < t.columns.len() && t.columns.len() > 1 {
                    t.columns.remove(idx);
                    t.recompute();
                }
            }
            self.dirty = true;
            self.bump_chart();
        }

        f.section("备注");
        let mut n = notes;
        let w = f.width();
        let resp = f.ui().add_sized(
            [w, 72.0],
            egui::TextEdit::multiline(&mut n)
                .desired_width(w)
                .font(FontId::proportional(text::BODY))
                .margin(Vec2::new(8.0, 6.0)),
        );
        if resp.lost_focus() {
            if let Some(t) = self.project.table_mut(id) {
                if t.notes != n {
                    t.notes = n;
                    self.dirty = true;
                }
            }
        }
    }

    fn analysis_inspector(&mut self, ui: &mut Ui, id: u64) {
        let pal = self.pal;
        let Some(a) = self.project.analysis(id) else {
            return;
        };
        let name = a.name.clone();
        let method = a.result.method.clone();
        let note = a.result.note.clone();
        let source = a.table;
        let table_name = self
            .project
            .table(source)
            .map(|t| t.name.clone())
            .unwrap_or_default();

        let mut f = Form::new(ui, pal);
        f.gap(2.0);
        heading(f.ui(), &name, &format!("来源：{}", table_name), &pal);
        f.gap(4.0);

        f.section("方法");
        f.ui()
            .label(RichText::new(&method).size(text::BODY).color(pal.muted));
        if !note.is_empty() {
            f.gap(4.0);
            f.note(&note);
        }

        f.section("操作");
        if f.action("复制结果表", false).clicked() {
            if let Some(a) = self.project.analysis(id) {
                let mut s = String::new();
                s.push_str(&a.result.columns.join("\t"));
                s.push('\n');
                for row in &a.result.rows {
                    s.push_str(&row.join("\t"));
                    s.push('\n');
                }
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    let _ = cb.set_text(s);
                    self.toast("结果已复制", ToastKind::Info);
                }
            }
        }
        if f.action("用这张表作图", false).clicked() {
            self.snapshot();
            let gid = self.project.add_graph_for(source);
            self.select(Sheet::Graph(gid));
            self.chart_rev = self.chart_rev.wrapping_add(1);
            self.dirty = true;
        }
        if f.action("删除这份分析", true).clicked() {
            self.snapshot();
            self.project.analyses.retain(|a| a.id != id);
            self.sel = Some(Sheet::Table(source));
            self.dirty = true;
        }
    }
}

/// 内容区顶部的对象标题：主标题 + 一行元信息，共用同一条左边界。
fn heading(ui: &mut Ui, title: &str, meta: &str, pal: &Palette) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 40.0), egui::Sense::hover());
    let p = ui.painter();
    widgets::paint_text_clipped(
        p,
        Rect::from_min_size(rect.min, Vec2::new(w, 20.0)),
        title,
        FontId::proportional(text::TITLE),
        pal.ink,
        false,
        0.0,
    );
    widgets::paint_text_clipped(
        p,
        Rect::from_min_size(
            egui::pos2(rect.left(), rect.top() + 21.0),
            Vec2::new(w, 16.0),
        ),
        meta,
        FontId::proportional(text::META),
        pal.faint,
        false,
        0.0,
    );
}
