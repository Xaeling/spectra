//! 右侧检查器：按当前工作表类型切换内容。
//!
//! # 关键规则：表格界面不显示绘图选项
//! - 当前是**数据表** → 显示「数据表检查器」：表结构、描述统计、真实分析
//! - 当前是**图表** → 显示「图形检查器」：坐标轴、配色、符号、显著性、导出
//! 两者互斥，不会像以前那样在数据表里堆一堆用不上的绘图配置。
//!
//! # 三原则
//! 1. **分组明确**：每区只碰自己那份状态
//! 2. **逻辑隔离**：数据表与图表配置完全分开
//! 3. **实时联动**：任何改动 → Msg → update → 重绘 → 下一帧即为新结果

use charis_ui::prelude::*;

use crate::app::Msg;
use crate::colormap::{self, Rgba};
use crate::export::ExportBackend;
use crate::i18n::{self, K, Lang};
use crate::plot::FrameStyle;
use crate::model::{Comparison, DataTable, GraphSheet};
use crate::palette::*;
use crate::plot::{BracketMode, ChartKind};
use crate::ui::{check, glyph, hsep, prop_row, radio, section, shadow_panel_left, Glyph};

/// 星号预设（析光的显著性标注惯例）。
pub const STAR_PRESETS: [&str; 5] = ["ns", "*", "**", "***", "P 值"];
/// `STAR_PRESETS` 里表示「直接显示 P 值」的那一项。
pub const SIG_P_OPT: &str = "P 值";

/// 入口：按当前工作表类型分派。
pub fn inspector(
    lang: Lang,
    is_graph: bool,
    table: Option<&DataTable>,
    graph: Option<&GraphSheet>,
    names: &[String],
    backend: ExportBackend,
    analysis: usize,
    welch: bool,
    canvas_h: f32,
    reverse_legend: bool,
) -> Element<Msg> {
    if is_graph {
        match graph {
            Some(g) => graph_inspector(lang, g, names, backend, canvas_h, reverse_legend),
            None => empty_panel(lang),
        }
    } else {
        match table {
            Some(t) => data_inspector(lang, t, analysis, welch),
            None => empty_panel(lang),
        }
    }
}

fn empty_panel(lang: Lang) -> Element<Msg> {
    let t = |k: K| i18n::t(lang, k);
    col()
        .w(INSPECTOR_W)
        .h_full()
        .bg(hex(panel_bg()))
        .border_l(1.0, hex(hair()))
        .p(12.0)
        .child(
            text(t(K::NoComparisons))
                .font_size(ui_font_sm())
                .color(hex(dim())),
        )
}

// ═══════════════════════════════════════════════════════════════════════════
// 一、数据表检查器（表格界面显示）
// ═══════════════════════════════════════════════════════════════════════════

fn data_inspector(lang: Lang, t: &DataTable, analysis: usize, welch: bool) -> Element<Msg> {
    col()
        .w(INSPECTOR_W)
        .h_full()
        .bg(hex(panel_bg()))
        .border_l(1.0, hex(hair()))
        .shadow(shadow_panel_left())
        .px(12.0)
        .py(10.0)
        .gap(12.0)
        .child(panel_header(glyph(Glyph::Grid), i18n::t(lang, K::InspectorTableTitle)))
        .child(
            col()
                .gap(10.0)
                .w_full()
                .flex1()
                .scroll_y()
                // ── 表结构 ────────────────────────────────────────────
                .child(section(i18n::t(lang, K::SectionStructure)))
                .child(structure_block(lang, t))
                // ── 描述统计（真实计算）────────────────────────────────
                .child(section(i18n::t(lang, K::SectionStats)))
                .child(stats_block(lang, t))
                .child(hsep())
                // ── 分析（真实计算，产出结果表）────────────────────────
                .child(section(i18n::t(lang, K::Analyze)))
                .child(analysis_block(lang, t, analysis, welch)),
        )
}

fn structure_block(lang: Lang, t: &DataTable) -> Element<Msg> {
    let tt = t.table.table_type;
    let type_name = type_label(tt, lang);
    let ncols = t.table.data_sets.len();
    let nsub = t.table.data_sets.first().map(|d| d.sub_columns.len()).unwrap_or(0);

    col()
        .gap(5.0)
        .w_full()
        .child(prop_row(
            i18n::t(lang, K::TableTypeLabel),
            text(type_name).font_size(ui_font()).color(hex(ink())),
        ))
        .child(prop_row(
            i18n::t(lang, K::DataSetsLabel),
            text(format!("{}", ncols)).font_size(ui_font()).color(hex(ink())),
        ))
        .child(prop_row(
            i18n::t(lang, K::SubColsLabel),
            text(format!("{}", nsub)).font_size(ui_font()).color(hex(ink())),
        ))
        .child(prop_row(
            i18n::t(lang, K::RowsLabel),
            text(format!("{}", t.view_rows)).font_size(ui_font()).color(hex(ink())),
        ))
        // 结构与数据都能真的改，不只是显示
        .child(
            row()
                .gap(4.0)
                .w_full()
                .child(mini_button(i18n::t(lang, K::AddDataSet), Msg::AddDataSet))
                .child(mini_button(i18n::t(lang, K::RemoveLastDataSet), Msg::RemoveLastDataSet)),
        )
        .child(
            row()
                .gap(4.0)
                .w_full()
                .child(mini_button(i18n::t(lang, K::AddSubColumn), Msg::AddSubColumn))
                .child(mini_button(
                    i18n::t(lang, K::RemoveLastSubColumn),
                    Msg::RemoveLastSubColumn,
                )),
        )
        .child(
            row()
                .gap(4.0)
                .w_full()
                .child(mini_button(i18n::t(lang, K::InsertRow), Msg::InsertRow))
                .child(mini_button(i18n::t(lang, K::DeleteRow), Msg::DeleteRow))
                .child(mini_button(i18n::t(lang, K::ClearCells), Msg::ClearCells)),
        )
}

/// 真实描述统计：每个数据集一列，逐项算出 n / 均值 / SD / SEM / …
fn stats_block(lang: Lang, t: &DataTable) -> Element<Msg> {
    let ds: Vec<crate::stats::Descriptive> = (0..t.table.data_sets.len())
        .map(|i| crate::stats::describe(&t.table.flat_values(i)))
        .collect();

    // 小数位数跟随「设置 ▸ 分析 ▸ 小数位数」，不再写死 4 位 / 2 位。
    // fn 指针数组：同签名闭包在 Rust 里类型各不相同，无法进数组
    let items: [(K, fn(&crate::stats::Descriptive) -> String); 8] = [
        (K::StatN, |d| d.n.to_string()),
        (K::StatMean, |d| format!("{:.*}", crate::settings::get().decimals.min(10), d.mean)),
        (K::StatSd, |d| format!("{:.*}", crate::settings::get().decimals.min(10), d.sd)),
        (K::StatSem, |d| format!("{:.*}", crate::settings::get().decimals.min(10), d.sem)),
        (K::StatMedian, |d| format!("{:.*}", crate::settings::get().decimals.min(10), d.median)),
        (K::StatMin, |d| format!("{:.*}", crate::settings::get().decimals.min(10), d.min)),
        (K::StatMax, |d| format!("{:.*}", crate::settings::get().decimals.min(10), d.max)),
        (K::StatCv, |d| format!("{:.*}", crate::settings::get().decimals.min(10), d.cv)),
    ];

    let mut header = row().gap(4.0).w_full();
    header = header.child(text("").font_size(ui_font_sm()).w(78.0));
    for d in t.table.data_sets.iter() {
        header = header.child(
            text(d.name.clone())
                .font_size(ui_font_sm())
                .semibold()
                .color(hex(ink()))
                .flex1(),
        );
    }

    let mut c = col().gap(1.0).w_full().child(header);
    for (k, get) in items.iter() {
        let mut r = row().gap(4.0).w_full();
        // 标签列宽度必须容纳最长的统计项名（如 Std. Deviation）。
        // 之前固定 78px，长标签直接被截断 —— 看起来就是「格子太窄」。
        r = r.child(
            text(i18n::t(lang, *k))
                .font_size(ui_font_sm())
                .color(hex(dim()))
                .w(118.0),
        );
        for d in ds.iter() {
            // min_w(0)：flex1 默认 min-width:auto 会被内容撑开，
            // 导致同一行里中间那格被两侧挤窄、文字看不全。
            r = r.child(
                text(get(d))
                    .font_size(ui_font_sm())
                    .color(hex(ink()))
                    .flex1()
                    .min_w(0.0),
            );
        }
        c = c.child(r);
    }
    c
}

fn analysis_block(lang: Lang, t: &DataTable, analysis: usize, welch: bool) -> Element<Msg> {
    // 清单**按当前表的类型动态生成**，与顶部「分析」菜单同源
    // （`TableType::analyses()`），避免两处各写一份、改一处漏一处。
    // 智能推荐的 id 是 ADVISOR_ID(=100) 而不是枚举判别值，
    // 所以这里不能再有任何 `min(3)` 之类的夹取 —— 夹一下就变成线性回归了。
    let opts = t.table.table_type.analyses();
    let mut list = col().gap(1.0).w_full().min_w(0.0);
    for (id, zh, en) in opts.iter() {
        let name = if lang == Lang::En { *en } else { *zh };
        list = list.child(radio(analysis == *id, name, Msg::SetAnalysis(*id)));
    }
    // 当前选中项若不在清单里（例如从别的类型的表切过来），
    // 补一行说明，避免出现「一个都没选中」的空状态。
    if !opts.iter().any(|(id, _, _)| *id == analysis) {
        list = list.child(
            text(format!("（当前：id {}）", analysis))
                .font_size(ui_font() - 1.0)
                .color(hex(dim())),
        );
    }
    let mut c = col()
        .gap(5.0)
        .w_full()
        .child(prop_row(i18n::t(lang, K::AnalysisKind), list));

    // t 检验才需要选方差假设
    if analysis == 1 {
        c = c.child(prop_row(
            "",
            col()
                .gap(1.0)
                .child(radio(!welch, i18n::t(lang, K::EqualVariance), Msg::SetWelch(false)))
                .child(radio(welch, i18n::t(lang, K::WelchLabel), Msg::SetWelch(true))),
        ));
    }

    c.child(button(i18n::t(lang, K::RunAnalysis)).on_click(Msg::RunAnalysis).w_full())
}

// ═══════════════════════════════════════════════════════════════════════════
// 二、图形检查器（仅在图表界面显示）
// ═══════════════════════════════════════════════════════════════════════════

fn graph_inspector(
    lang: Lang,
    g: &GraphSheet,
    names: &[String],
    backend: ExportBackend,
    canvas_h: f32,
    reverse_legend: bool,
) -> Element<Msg> {
    // 面板按「①图形 ②数据 ③元素 ④坐标轴 ⑤配色 ⑥显著性 ⑦图例与线条
    // ⑧图类型专属 ⑨尺寸与导出」九组排列 —— 分类维度统一按元素/用途走，
    // 只有第 ⑧ 组按当前图类型动态显示。
    //
    // 两组硬重复已消除：图表类型只有①这一处入口；误差棒/散点也只在一处。
    // 空分组不渲染（留一个空标题比没有标题更糟），所以某些组会整组消失。
    let t = |k: K| i18n::t(lang, k);
    let mut body = col().gap(10.0).w_full().flex1().scroll_y();

    // ══ ① 图形 ═════════════════════════════════════════════════════════════
    body = body
        .child(section(t(K::SectionGraphType)))
        .child(prop_row(
            t(K::PropName),
            text_input(g.name.clone(), Msg::SetGraphName).w_full(),
        ))
        .child(prop_row(
            t(K::ChartTitle),
            text_input(g.title.clone(), Msg::SetTitle).w_full(),
        ))
        .child(
            row()
                .gap(10.0)
                .child(check(g.title_bold, t(K::TitleBold), Msg::ToggleTitleBold))
                .child(check(g.title_italic, t(K::TitleItalic), Msg::ToggleTitleItalic)),
        )
        // 图表类型的**唯一入口**（原 Style > Appearance 的重复入口已删除）。
        // 62 种类型用下拉 + 分类前缀，单选会占掉大半屏。
        .child(prop_row(
            t(K::PropType),
            pick_list(
                kind_names(lang),
                Some(g.kind.to_usize()),
                Msg::SetChartKind,
            ).w_full(),
        ))
        .child(slider_row(
            t(K::CanvasH),
            format!("{:.0} px", canvas_h),
            slider(canvas_h, 120.0, 600.0)
                .on_change(Msg::SetCanvasH),
        ))
        .child(hsep());

    // ══ ② 数据 ═════════════════════════════════════════════════════════════
    body = body
        .child(section(t(K::SectionData)))
        .child(check(g.show_error, t(K::ShowErrorBars), Msg::ToggleError))
        .child(prop_row(
            t(K::ErrKind),
            pick_list(
                vec![t(K::ErrSD), t(K::ErrSEM), t(K::ErrCI95)],
                Some(g.err_kind.min(2)),
                Msg::SetErrKind,
            ).w_full(),
        ))
        .child(prop_row(
            t(K::ErrDir),
            pick_list(
                vec![t(K::ErrBoth), t(K::ErrUp), t(K::ErrDown)],
                Some(g.err_dir.min(2)),
                Msg::SetErrDir,
            ).w_full(),
        ))
        .child(slider_row(
            t(K::ErrWidth),
            format!("{:.1}", g.err_width),
            slider(g.err_width, 0.5, 6.0)
                .on_change(Msg::SetErrWidth),
        ))
        .child(check(g.show_scatter, t(K::ShowPoints), Msg::ToggleScatter))
        .child(check(g.connect, t(K::Connect), Msg::ToggleConnect))
        // 散点全套：形状 / 大小 / 描边 / 透明度 / 抖动（原 Style>Symbol 与
        // 顶层 Symbols 两处合并为一处）
        .child(prop_row(
            t(K::SymShape),
            pick_list(
                vec![
                    t(K::ShapeCircle),
                    t(K::ShapeSquare),
                    t(K::ShapeTriangle),
                    t(K::ShapeDiamond),
                ],
                Some(g.symbol_shape.min(3)),
                Msg::SetSymbolShape,
            ).w_full(),
        ))
        .child(symbol_block(lang, g))
        .child(slider_row(
            t(K::SymBorder),
            format!("{:.1}", g.symbol_border),
            slider(g.symbol_border, 0.0, 5.0)
                .on_change(Msg::SetSymbolBorder),
        ))
        .child(slider_row(
            t(K::SymAlpha),
            format!("{:.2}", g.symbol_alpha),
            slider(g.symbol_alpha, 0.1, 1.0)
                .on_change(Msg::SetSymbolAlpha),
        ))
        .child(hsep());

    // ══ ③ 元素 ═════════════════════════════════════════════════════════════
    body = body
        .child(section(t(K::SectionElements)))
        .child(slider_row(
            t(K::BarWidth),
            format!("{:.2}", g.bar_width),
            slider(g.bar_width, 0.15, 0.95)
                .on_change(Msg::SetBarWidth),
        ))
        .child(prop_row(
            t(K::BarPattern),
            pick_list(
                vec![t(K::PatSolid), t(K::PatDiag), t(K::PatCross), t(K::PatHori)],
                Some(g.bar_pattern.min(3)),
                Msg::SetBarPattern,
            ).w_full(),
        ))
        .child(check(g.bar_zero, t(K::BarZero), Msg::ToggleBarZero))
        .child(check(g.area_fill, t(K::AreaFill), Msg::ToggleAreaFill))
        .child(slider_row(
            t(K::BarWidthCm),
            format!("{:.2}", g.bar_width_cm),
            slider(g.bar_width_cm, 0.0, 3.0)
                .on_change(Msg::SetBarWidthCm),
        ))
        .child(prop_row(
            t(K::LineDash),
            pick_list(
                vec![t(K::LineSolid), t(K::LineDashed), t(K::LineDotted)],
                Some(g.dash.min(2)),
                Msg::SetDash,
            ).w_full(),
        ))
        .child(prop_row(
            t(K::Caption),
            text_input(g.caption.clone(), Msg::SetCaption).w_full(),
        ))
        .child(hsep());

    // ══ ④ 坐标轴 ═══════════════════════════════════════════════════════════
    body = body
        .child(section(t(K::SectionAxes)))
        .child(axis_block(lang, g))
        .child(hsep())
        .child(section(t(K::SectionFrame)))
        .child(radio(g.frame == FrameStyle::L, t(K::FrameL), Msg::SetFrame(0)))
        .child(radio(
            g.frame == FrameStyle::Box,
            t(K::FrameBox),
            Msg::SetFrame(1),
        ))
        .child(check(g.grid_on, t(K::GridLines), Msg::ToggleGrid))
        .child(prop_row(
            t(K::GridStyle),
            pick_list(
                vec![t(K::LineSolid), t(K::LineDashed), t(K::LineDotted)],
                Some(g.grid_style.min(2)),
                Msg::SetGridStyle,
            ).w_full(),
        ))
        .child(check(g.log_y, t(K::LogY), Msg::ToggleLogY))
        .child(check(g.axis_break, t(K::AxisBreak), Msg::ToggleAxisBreak))
        .child(check(g.right_axis, t(K::RightAxis), Msg::ToggleRightAxis))
        .child(slider_row(
            t(K::RightFrom),
            format!("{}", g.right_from + 1),
            slider(g.right_from as f32 + 1.0, 1.0, 12.0)
                .on_change(|v| Msg::SetRightFrom((v as usize).saturating_sub(1))),
        ))
        .child(unit_hint_row(lang, g))
        .child(hsep());

    // ══ ⑤ 配色 ═════════════════════════════════════════════════════════════
    body = body
        .child(section(t(K::SectionPalette)))
        .child(palette_block(lang, g))
        .child(check(g.grayscale, t(K::Grayscale), Msg::ToggleGrayscale))
        .child(hsep());

    // ══ ⑥ 显著性标注条 ═════════════════════════════════════════════════════
    body = body
        .child(section(t(K::SectionSignificance)))
        .child(bracket_block(lang, g, names))
        .child(hsep());

    // ══ ⑦ 图例与线条 ═══════════════════════════════════════════════════════
    body = body
        .child(section(t(K::SectionLegendLine)))
        .child(check(g.show_legend, t(K::ShowLegend), Msg::ToggleLegend))
        .child(prop_row(
            t(K::LegendPos),
            pick_list(
                vec![t(K::LegendRight), t(K::LegendTopRight), t(K::LegendTopLeft)],
                Some(g.legend_pos.min(2)),
                Msg::SetLegendPos,
            ).w_full(),
        ))
        .child(check(
            reverse_legend,
            t(K::ReverseLegendOrder),
            Msg::ReverseLegend,
        ))
        .child(slider_row(
            t(K::LineWidth),
            format!("{:.2}×", g.line_width),
            slider(g.line_width, 0.25, 5.0)
                .on_change(Msg::SetLineWidth),
        ))
        .child(annotation_block(lang, g))
        .child(hsep());

    // ══ ⑧ 图类型专属 ═══════════════════════════════════════════════════════
    // 该图型没有任何专属控件时（多数图型如此）整组不渲染，不留空标题。
    if let Some(ks) = kind_specific_block(lang, g) {
        body = body
            .child(section(t(K::SectionKindSpecific)))
            .child(ks)
            .child(hsep());
    }

    // ══ ⑨ 尺寸与导出 ═══════════════════════════════════════════════════════
    body = body
        .child(section(t(K::SectionSizeExport)))
        .child(export_block(lang, g, backend));

    col()
        .w(INSPECTOR_W)
        .min_w(0.0)
        .h_full()
        .bg(hex(panel_bg()))
        .border_l(1.0, hex(hair()))
        .shadow(shadow_panel_left())
        .px(12.0)
        .py(10.0)
        .gap(12.0)
        .child(panel_header(glyph(Glyph::Bars), t(K::InspectorTitle)))
        .child(body)
}

/// 62 种图型的下拉项文本：`分类 / 图型`。
///
/// 带分类前缀是为了在 62 项的长列表里能靠前缀定位，
/// 否则近义变体（柱状图 / 分组柱状图 / 堆叠柱状图）很难区分。
fn kind_names(lang: Lang) -> Vec<String> {
    ChartKind::all()
        .iter()
        .map(|k| {
            let cat = if lang == Lang::Zh {
                ChartKind::category_name_zh(k.category())
            } else {
                ChartKind::category_name(k.category())
            };
            let name = if lang == Lang::Zh {
                k.label_zh()
            } else {
                k.label()
            };
            format!("{} / {}", cat, name)
        })
        .collect()
}

/// 第 ⑧ 组：只显示与当前图表类型相关的控件，其余整组不渲染。
///
/// 由 [`ChartKind`] 的旗标方法驱动（`needs_bins` / `needs_inner` / …），
/// 而不是按 62 个变体逐个 match —— 新增图型只要标对了旗标，这里自动跟上。
fn kind_specific_block(lang: Lang, g: &GraphSheet) -> Option<Element<Msg>> {
    let t = |k: K| i18n::t(lang, k);
    let k = g.kind;
    // 用 Vec 收集而不是一路 .child() 链下去：只有收集完才知道这一组到底有
    // 没有控件。空的时候返回 None，连上面的 section 标题一起不渲染 ——
    // 留一个「图类型专属」的空标题比没有标题更糟（用户会以为加载失败了）。
    // 早先是直接返回空 col()，于是绝大多数图型下这里永远是一个空标题。
    let mut items: Vec<Element<Msg>> = Vec::new();

    // 分箱数：直方图 / 热图 / 拟合曲线阶数
    if k.needs_bins() {
        items.push(slider_row(
            t(K::Bins),
            format!("{}", g.bins),
            slider(g.bins as f32, 1.0, 40.0).on_change(Msg::SetBinsF),
        ));
    }
    // 饼 / 环：内径与起始角
    if k.needs_pie() || k.needs_inner() {
        items.push(slider_row(
            t(K::PieInner),
            format!("{:.2}", g.pie_inner),
            slider(g.pie_inner, 0.0, 0.9).on_change(Msg::SetPieInner),
        ));
        items.push(slider_row(
            t(K::PieStart),
            format!("{:.0}\u{b0}", g.pie_start),
            slider(g.pie_start, 0.0, 360.0).on_change(Msg::SetPieStart),
        ));
    }
    // 热图与矩阵
    if k.needs_heat() {
        items.push(check(g.heat_auto, t(K::HeatAuto), Msg::ToggleHeatAuto));
    }
    // 参考线：森林图 / 火山图
    if k.needs_ref() {
        items.push(prop_row(
            t(K::ForestRef),
            text_input(format!("{}", g.forest_ref), |s| {
                Msg::SetForestRef(s.parse().unwrap_or(0.0))
            })
            .w_full(),
        ));
    }
    // 生存
    if k.needs_survival() {
        items.push(check(g.surv_censor, t(K::SurvCensor), Msg::ToggleSurvCensor));
    }
    // 分面：共享轴
    if k.needs_facet() {
        items.push(check(g.facet_shared, t(K::FacetShared), Msg::ToggleFacetShared));
    }
    // 点阵列数（华夫图）
    if k.needs_waffle() {
        items.push(slider_row(
            t(K::WaffleCols),
            format!("{}", g.waffle_cols),
            slider(g.waffle_cols as f32, 1.0, 40.0).on_change(Msg::SetWaffleColsF),
        ));
    }
    // 箱线：须的画法；小提琴：柱宽即轮廓胖瘦
    if k.needs_box_style() {
        items.push(radio(g.box_style == 0, t(K::BoxWhiskerOn), Msg::SetBoxStyle(0)));
        items.push(radio(g.box_style == 1, t(K::BoxWhiskerOff), Msg::SetBoxStyle(1)));
    }
    if k.needs_box_style() || k.needs_violin() {
        items.push(
            text(t(K::TipBarWidthAffects))
                .font_size(ui_font_sm())
                .color(hex(dim())),
        );
    }
    // 语义提示：该图型对数据结构有要求时给出，避免用户误解
    if let Some(cav) = k.caveat() {
        items.push(
            text(cav.to_string())
                .font_size(ui_font_sm())
                .color(hex(dim())),
        );
    }

    if items.is_empty() {
        return None;
    }
    let mut c = col().gap(5.0).w_full();
    for it in items {
        c = c.child(it);
    }
    Some(c)
}
/// 滑块行：标签 + 数值 + 滑块（数值实时更新，松手才重绘）。
fn slider_row(label: &str, value: String, sl: Element<Msg>) -> Element<Msg> {
    prop_row(
        label,
        col()
            .gap(0.0)
            .w_full()
            .child(sl.w_full())
            .child(text(value).font_size(ui_font_sm()).color(hex(dim()))),
    )
}

fn panel_header(icon: Element<Msg>, title: &'static str) -> Element<Msg> {
    row()
        .items_center()
        .gap(6.0)
        .child(icon)
        .child(text(title).font_size(ui_font()).semibold().color(hex(ink())))
}

fn axis_block(lang: Lang, g: &GraphSheet) -> Element<Msg> {
    col()
        .gap(5.0)
        .w_full()
        .child(prop_row(
            i18n::t(lang, K::AxisX),
            row()
                .gap(4.0)
                .child(
                    text_input(g.x_name.clone(), Msg::SetXName).w_full()
                        .placeholder(i18n::t(lang, K::PropName))
                        .w(84.0),
                )
                .child(
                    text_input(g.x_sym.clone(), Msg::SetXSym).w_full()
                        .placeholder(i18n::t(lang, K::XSymbol))
                        .w(86.0),
                )
                .child(
                    text_input(g.x_unit.clone(), Msg::SetXUnit).w_full()
                        .placeholder(i18n::t(lang, K::PropUnit))
                        .w(66.0),
                ),
        ))
        .child(prop_row(
            i18n::t(lang, K::AxisY),
            row()
                .gap(4.0)
                .child(
                    text_input(g.y_name.clone(), Msg::SetYName).w_full()
                        .placeholder(i18n::t(lang, K::PropName))
                        .w(84.0),
                )
                .child(
                    text_input(g.y_sym.clone(), Msg::SetYSym).w_full()
                        .placeholder(i18n::t(lang, K::YSymbol))
                        .w(86.0),
                )
                .child(
                    text_input(g.y_unit.clone(), Msg::SetYUnit).w_full()
                        .placeholder(i18n::t(lang, K::PropUnit))
                        .w(66.0),
                ),
        ))
        .child(prop_row(
            i18n::t(lang, K::BracketStyle),
            pick_list(
                vec![
                    i18n::t(lang, K::BracketHalf).to_string(),
                    i18n::t(lang, K::BracketFull).to_string(),
                ],
                Some(g.bracket_style.min(1)),
                Msg::SetBracketStyle,
            ).w_full(),
        ))
        .child(prop_row(
            i18n::t(lang, K::UnitStyleLabel),
            pick_list(
                vec![
                    i18n::t(lang, K::UnitParen).to_string(),
                    i18n::t(lang, K::UnitSlash).to_string(),
                    i18n::t(lang, K::UnitComma).to_string(),
                    i18n::t(lang, K::UnitNone).to_string(),
                ],
                Some(g.unit_style.to_i32() as usize),
                Msg::SetUnitStyle,
            ).w_full(),
        ))
        .child(check(
            g.use_italic,
            i18n::t(lang, K::ItalicSymbol),
            Msg::ToggleItalic,
        ))
}

/// 配色：下拉框（中文名）+ 当前方案的色块预览。
/// 26 行列表太长，收进选择框；选中后下方仍能看到实际色序。
fn palette_block(lang: Lang, g: &GraphSheet) -> Element<Msg> {
    let zh = matches!(lang, Lang::Zh);
    // 一套配色有多少色就放多少块：各方案 6–9 色，全部展示才看得出真实色序。
    let n_sw = colormap::COLORMAPS[g.colormap].colors.len();
    let sw_w = ((INSPECTOR_W - 96.0 - 8.0 * ((n_sw - 1) as f32)) / n_sw as f32).clamp(10.0, 18.0);
    let mut swatches = row().gap(3.0).w_full();
    for k in 0..n_sw {
        swatches = swatches.child(
            swatch(
                colormap::fill_solid(g.colormap, k),
                colormap::edge_of(g.colormap, k),
                sw_w,
            ),
        );
    }
    col()
        .gap(5.0)
        .w_full()
        .child(prop_row(
            i18n::t(lang, K::Colormap),
            pick_list(colormap::colormap_names(zh), Some(g.colormap), Msg::SetColormap).w_full(),
        ))
        .child(prop_row(i18n::t(lang, K::SwatchPreview), swatches))
}

/// 色块：内部（85% 混白）+ 深色边框，与柱体实际外观一致。
fn swatch(fill_c: Rgba, edge_c: Rgba, size: f32) -> Element<Msg> {
    let fh = colormap::to_hex(fill_c);
    let eh = colormap::to_hex(edge_c);
    canvas(move |c, r| {
        let (x, y, w, h) = (r.origin().x, r.origin().y, r.w, r.h);
        let ink = hex(eh.as_str());
        // 填充：一条粗线铺满，避免依赖 Rectangle 构造
        c.line(
            Point::new(x + 1.0, y + h * 0.5),
            Point::new(x + w - 1.0, y + h * 0.5),
            h - 2.0,
            hex(fh.as_str()),
        );
        c.line(Point::new(x, y), Point::new(x + w, y), 1.0, ink);
        c.line(Point::new(x + w, y), Point::new(x + w, y + h), 1.0, ink);
        c.line(Point::new(x + w, y + h), Point::new(x, y + h), 1.0, ink);
        c.line(Point::new(x, y + h), Point::new(x, y), 1.0, ink);
    })
    .size(size, size)
    .rounded(r_sm())
}

fn symbol_block(lang: Lang, g: &GraphSheet) -> Element<Msg> {
    let mut c = col()
        .gap(5.0)
        .w_full()
        // 防呆：跟随柱子时不露出符号色选择器
        .child(check(
            g.symbol_follows_bar,
            i18n::t(lang, K::SymbolFollowsBar),
            Msg::ToggleSymbolFollows,
        ));

    if !g.symbol_follows_bar {
        // 13 种模式：0 = 跟随外观（深色界面白、浅色界面黑，实时翻转），
        // 1..13 = 12 种具名色。下拉**挑选 + 色块预览**，不手填 HEX。
        let mut names: Vec<String> = vec![i18n::t(lang, K::SymbolFollowAppearance).to_string()];
        names.extend(colormap::symbol_color_names().iter().map(|x| x.to_string()));
        let preview: colormap::Rgba = if g.symbol_color_idx == 0 {
            if crate::palette::dark() {
                [255, 255, 255, 255]
            } else {
                [26, 26, 26, 255]
            }
        } else {
            colormap::symbol_color(g.symbol_color_idx.saturating_sub(1))
        };
        c = c.child(prop_row(
            i18n::t(lang, K::SymbolOwnColor),
            row()
                .gap(4.0)
                .child(swatch(
                    colormap::mix_white(preview, 1.0),
                    colormap::shade(preview),
                    16.0,
                ))
                .child(
                    pick_list(
                        names.clone(),
                        Some(g.symbol_color_idx.min(names.len() - 1)),
                        Msg::SetSymbolColor,
                    ).w_full()
                    .w_full(),
                ),
        ));
    }

    c.child(prop_row(
        i18n::t(lang, K::SymbolSize),
        col()
            .gap(0.0)
            .w_full()
            .child(
                slider(g.symbol_size, 0.5, 8.0)
                    .on_change(Msg::SetSymbolSize)
                    .w_full(),
            )
            .child(
                text(format!("{:.1} px", g.symbol_size))
                    .font_size(ui_font_sm())
                    .color(hex(dim())),
            ),
    ))
    .child(prop_row(
        i18n::t(lang, K::Jitter),
        col()
            .gap(0.0)
            .w_full()
            .child(slider(g.jitter, 0.0, 1.0).on_change(Msg::SetJitter).w_full())
            .child(
                text(format!("{:.2}", g.jitter))
                    .font_size(ui_font_sm())
                    .color(hex(dim())),
            ),
    ))
}

/// 标注列表：类型 / 字号 / 删除。位置在这里改不了，靠工具栏添加时错开。
fn annotation_block(lang: Lang, g: &GraphSheet) -> Element<Msg> {
    let mut c = col()
        .gap(4.0)
        .w_full()
        .child(
            row()
                .gap(4.0)
                .w_full()
                .child(mini_button(i18n::t(lang, K::AnnLine), Msg::AddLine))
                .child(mini_button(i18n::t(lang, K::AnnArrow), Msg::AddArrow))
                .child(mini_button(i18n::t(lang, K::AnnText), Msg::AddText)),
        )
        .child(
            row()
                .gap(4.0)
                .w_full()
                .child(mini_button(i18n::t(lang, K::AnnFontUp), Msg::AnnFontUp))
                .child(mini_button(i18n::t(lang, K::AnnFontDown), Msg::AnnFontDown))
                .child(
                    text(format!("{:.0} px", g.ann_size))
                        .font_size(ui_font_sm())
                        .color(hex(dim())),
                ),
        );

    if g.annotations.is_empty() {
        return c.child(
            text(i18n::t(lang, K::AnnNone))
                .font_size(ui_font_sm())
                .color(hex(dim())),
        );
    }
    for (i, a) in g.annotations.iter().enumerate() {
        c = c.child(
            row()
                .gap(4.0)
                .w_full()
                .items_center()
                .child(
                    text(format!("{}", a.kind.label()))
                        .font_size(ui_font_sm())
                        .color(hex(ink()))
                        .w(40.0),
                )
                .child(
                    text(if a.text.is_empty() {
                        format!("({:.2}, {:.2})", a.x, a.y)
                    } else {
                        a.text.clone()
                    })
                    .font_size(ui_font_sm())
                    .color(hex(dim()))
                    .flex1(),
                )
                .child(
                    div()
                        .px(3.0)
                        .hover(|s| s.bg(hex(danger_soft())))
                        .on_click(Msg::RemoveAnnotation(i))
                        .tooltip(i18n::t(lang, K::RemoveAnnotation))
                        .child(text("×").font_size(ui_font()).color(hex(ink()))),
                ),
        );
    }
    c
}

fn bracket_block(lang: Lang, g: &GraphSheet, names: &[String]) -> Element<Msg> {
    let mut c = col()
        .gap(5.0)
        .w_full()
        .child(check(
            g.show_bracket,
            i18n::t(lang, K::ShowBracket),
            Msg::ToggleBracket,
        ));

    if !g.show_bracket {
        return c;
    }

    c = c.child(prop_row(
        i18n::t(lang, K::BracketModeLabel),
        col()
            .gap(1.0)
            .child(radio(
                g.bracket_mode == BracketMode::Auto,
                i18n::t(lang, K::BracketAuto),
                Msg::SetBracketMode(0),
            ))
            .child(radio(
                g.bracket_mode == BracketMode::Absolute,
                i18n::t(lang, K::BracketAbsolute),
                Msg::SetBracketMode(1),
            ))
            .child(radio(
                g.bracket_mode == BracketMode::Relative,
                i18n::t(lang, K::BracketRelative),
                Msg::SetBracketMode(2),
            )),
    ));

    if g.bracket_mode != BracketMode::Auto {
        c = c.child(prop_row(
            i18n::t(lang, K::BracketValue),
            text_input(format!("{:.2}", g.bracket_value), Msg::SetBracketValue).w_full(),
        ));
    }

    // 标注写法：只星号时所有「极显著」的对都是 ****、看不出谁更强，
    // 所以默认「星号 + P 值」，P 值能直接比较显著程度。
    let fmt = crate::settings::with(|s| s.bracket_label_fmt);
    c = c.child(prop_row(
        i18n::t(lang, K::BracketLabelFmt),
        row()
            .gap(6.0)
            .child(radio(
                fmt == 0,
                i18n::t(lang, K::SigFmtStar),
                Msg::SetBracketLabelFmt(0),
            ))
            .child(radio(
                fmt == 1,
                i18n::t(lang, K::SigFmtP),
                Msg::SetBracketLabelFmt(1),
            ))
            .child(radio(
                fmt == 2,
                i18n::t(lang, K::SigFmtBoth),
                Msg::SetBracketLabelFmt(2),
            )),
    ));

    // 比较对表格（独立滚动区，比较对多时不撑爆面板）
    let mut table = col().gap(2.0).w_full();
    if g.comparisons.is_empty() {
        table = table.child(
            text(i18n::t(lang, K::NoComparisons))
                .font_size(ui_font_sm())
                .color(hex(dim())),
        );
    }
    for (i, cmp) in g.comparisons.iter().enumerate() {
        table = table.child(comparison_row(lang, i, cmp, names));
    }

    c.child(div().max_h(150.0).w_full().child(table.scroll_y()))
        .child(
            row()
                .gap(4.0)
                .items_center()
                .child(
                    div()
                        .px(4.0)
                        .py(2.0)
                        .hover(|s| s.bg(hex(sel_highlight())))
                        .on_click(Msg::AddComparison)
                        .child(
                            text(i18n::t(lang, K::AddComparison))
                                .font_size(ui_font())
                                .color(hex(accent())),
                        ),
                )
                .child(
                    div()
                        .px(4.0)
                        .py(2.0)
                        .hover(|s| s.bg(hex(sel_highlight())))
                        .on_click(Msg::AutoComparisons)
                        .child(
                            text(i18n::t(lang, K::AutoComparisons))
                                .font_size(ui_font())
                                .color(hex(accent())),
                        ),
                ),
        )
}

fn comparison_row(lang: Lang, i: usize, cmp: &Comparison, names: &[String]) -> Element<Msg> {
    let opts = if names.is_empty() {
        vec!["A".to_string(), "B".to_string()]
    } else {
        names.to_vec()
    };
    row()
        .gap(3.0)
        .w_full()
        .items_center()
        .child(
            pick_list(
                opts.clone(),
                Some(cmp.a.min(opts.len() - 1)),
                move |v| Msg::EditComparison(i, Some(v), None, None),
            ).w_full()
            .w(74.0),
        )
        .child(text("vs").font_size(ui_font_sm()).color(hex(dim())))
        .child(
            pick_list(
                opts.clone(),
                Some(cmp.b.min(opts.len() - 1)),
                move |v| Msg::EditComparison(i, None, Some(v), None),
            ).w_full()
            .w(74.0),
        )
        .child(star_pick(i, &cmp.label, cmp.p))
        .child(
            div()
                .px(3.0)
                .hover(|s| s.bg(hex(danger_soft())))
                .on_click(Msg::RemoveComparison(i))
                .tooltip(i18n::t(lang, K::RemoveComparison))
                .child(text("×").font_size(ui_font()).color(hex(ink()))),
        )
}

fn star_pick(i: usize, current: &str, p: Option<f64>) -> Element<Msg> {
    let cur = current.to_string();
    let mut opts: Vec<String> = STAR_PRESETS.iter().map(|s| s.to_string()).collect();
    // 当前标签不在预设里（比如自动生成的「*** P = 0.0032」）时补进去，
    // 否则下拉框会把它显示成别的项、下一次点就被改掉。
    if !opts.iter().any(|s| *s == cur) {
        opts.push(cur.clone());
    }
    let sel = opts.iter().position(|s| *s == cur).unwrap_or(0);
    let opts2 = opts.clone();
    pick_list(opts, Some(sel), move |v| {
        let lab = if v < STAR_PRESETS.len() {
            if STAR_PRESETS[v] == SIG_P_OPT {
                match p {
                    Some(x) => format!("P = {}", crate::stats::fmt_p(x)),
                    None => "P".to_string(),
                }
            } else {
                STAR_PRESETS[v].to_string()
            }
        } else {
            opts2.get(v).cloned().unwrap_or_default()
        };
        Msg::EditComparison(i, None, None, Some(lab))
    }).w_full()
    .w(88.0)
}

/// ⑨ 组：只放导出入口与后端说明。
///
/// 以前这里还有一个「标题」输入框，和 ①图形 里的「图表标题」是同一个字段的
/// 第二份入口 —— 两处都能改同一张图的标题，改了一处另一处不刷新，看着像没生效。
/// 标题只保留 ① 那一处。
fn export_block(lang: Lang, _g: &GraphSheet, backend: ExportBackend) -> Element<Msg> {
    col()
        .gap(5.0)
        .w_full()
        // 只留一个入口：按下弹格式选择窗，再走系统保存对话框。
        // 之前这里并排「导出图形 / 导出 PNG」两个按钮，前者走设置里的后端
        // （svg / tex / py），后者把文件硬写到程序当前目录 —— 用户既不知道
        // 点了会出什么格式，也不知道文件去了哪。
        .child(
            button(i18n::t(lang, K::ExportChart))
                .on_click(Msg::OpenExportDialog)
                .w_full(),
        )
        .child(
            text(format!("{} · {}", backend.label(), backend.hint()))
                .font_size(ui_font_sm())
                .color(hex(dim())),
        )
}

// ── 小工具 ──────────────────────────────────────────────────────────────────

fn mini_button(label: &'static str, msg: Msg) -> Element<Msg> {
    div()
        .px(6.0)
        .py(3.0)
        .rounded(r_sm())
        .bg(hex(canvas_bg()))
        .border(1.0, hex(hair()))
        .hover(|s| s.bg(hex(accent_soft())))
        .on_click(msg)
        .child(text(label).font_size(ui_font_sm()).color(hex(ink())).nowrap())
}

/// 表类型的本地化名。
fn type_label(tt: crate::model::TableType, lang: Lang) -> &'static str {
    let i = match tt {
        crate::model::TableType::XY => 0,
        crate::model::TableType::Column => 1,
        crate::model::TableType::Grouped => 2,
        crate::model::TableType::Contingency => 3,
        crate::model::TableType::Survival => 4,
        crate::model::TableType::PartsOfWhole => 5,
        crate::model::TableType::MultipleVariables => 6,
        crate::model::TableType::Nested => 7,
    };
    let (nk, _) = crate::model::TABLE_KINDS[i];
    i18n::t(lang, nk)
}

/// 单位输入校验提示（文档 §3.5）：只提示、不改写用户输入。
fn unit_hint_row(lang: Lang, g: &GraphSheet) -> Element<Msg> {
    let mut hints: Vec<String> = crate::plot::unit_hints(&g.x_unit, &g.x_name)
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    for h in crate::plot::unit_hints(&g.y_unit, &g.y_name) {
        if !hints.iter().any(|x| x == h) {
            hints.push(h.to_string());
        }
    }
    if hints.is_empty() {
        return col().gap(0.0);
    }
    let mut c = col().gap(2.0).w_full().child(
        text(i18n::t(lang, K::UnitHintTitle))
            .font_size(12.0)
            .semibold()
            .color(hex(warn_text())),
    );
    for h in hints {
        c = c.child(text(h).font_size(12.0).color(hex(warn_text())));
    }
    c
}
