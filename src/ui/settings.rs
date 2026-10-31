//! 设置窗口（析光的 Preferences）。
//!
//! 结构对标原实现的规范：自建标题条（恒占满窗口宽度，左侧「设置」、右侧 × 关闭），
//! 底部三按钮 **确认**（保存 + 关闭）/ **应用**（保存不关闭）/ **关闭**（仅关闭）。
//! 左侧分类：常规 / 外观 / 语言 / 分析。

use charis_ui::prelude::*;

use crate::app::Msg;
use crate::export::ExportBackend;
use crate::i18n::{self, K, Lang};
use crate::settings::{Edit, Settings};
use crate::palette::*;
use std::time::Instant;
use crate::ui::{check, hsep, prop_row, radio, section, tool_button, Glyph};

/// 设置面板的分类（顺序固定，与 Settings 菜单一一对应）。
///
/// 菜单项 = 这里的一项，点了直接跳到对应页，所以顺序必须一致。
pub const TABS: [K; 7] = [
    K::SettingsAppearanceView, // 0 外观
    K::ViewSettings,           // 1 视图设置
    K::FilesPrinting,          // 2 文件与打印
    K::NewGraphSettings,       // 3 新建图表
    K::AnalysisSettings,       // 4 分析
    K::FileLocations,          // 5 文件位置
    K::SendToOffice,           // 6 发送到 Office
];

/// 作为覆盖层绘制的设置窗口：背景背板 + 居中面板。
pub fn settings_overlay(
    lang: Lang,
    tab: usize,
    draft: &Settings,
    backend: ExportBackend,
    open_at: Instant,
) -> Element<Msg> {
    // dim = false：不铺灰底（那层灰会把底层界面整个盖成一块灰方块）。
    // 遮罩只负责「点外部 = 关闭」，面板自身是不透明的。
    backdrop(Msg::CloseSettings, false)
        .size_full()
        .center()
        .child(settings_dialog(lang, tab, draft, backend, open_at))
}

fn settings_dialog(
    lang: Lang,
    tab: usize,
    draft: &Settings,
    backend: ExportBackend,
    open_at: Instant,
) -> Element<Msg> {
    // 形态渐变：尺寸 + 圆角 + 位移同时插值（见 ui::anim）
    let (mw, mh, mr, mdy, _done) = crate::ui::anim::morph(open_at, 700.0, 540.0);
    // ── 自建标题条：左侧标题 + 右侧 × ──
    let bar = row()
        .w_full()
        .h(28.0)
        .px(8.0)
        .bg(hex(window_bg()))
        .border_b(1.0, hex(hair()))
        .items_center()
        .justify_between()
        .child(text(i18n::t(lang, K::Settings)).font_size(ui_font()).semibold().color(hex(ink())))
        .child(
            div()
                .size(20.0, 20.0)
                .center()
                .hover(|s| s.bg(hex(danger())))
                .on_click(Msg::CloseSettings)
                .tooltip(i18n::t(lang, K::Close))
                .child(
                    canvas(|c, r| {
                        let ink = hex(ink());
                        let (x, y, w, h) = (r.origin().x, r.origin().y, r.w, r.h);
                        c.line(Point::new(x + w * 0.28, y + h * 0.28), Point::new(x + w * 0.72, y + h * 0.72), 1.4, ink);
                        c.line(Point::new(x + w * 0.72, y + h * 0.28), Point::new(x + w * 0.28, y + h * 0.72), 1.4, ink);
                    })
                    .size(10.0, 10.0),
                ),
        );

    // ── 左侧分类 ──
    let mut side = col().gap(1.0).w(112.0).p(6.0);
    for (i, k) in TABS.iter().enumerate() {
        side = side.child(
            row()
                .h(row_h() + 6.0)
                .px(6.0)
                .gap(6.0)
                .items_center()
                .rounded(r_sm())
                .when(i == tab, |e| e.bg(hex(accent_soft())))
                .hover(|s| s.bg(hex(family_highlight())))
                .on_click(Msg::SettingsTab(i))
                .child(
                    // 选中分类左侧一条强调竖条
                    div().w(2.0).h(row_h() - 2.0).bg(if i == tab { hex(accent()) } else { hex(panel_bg()) }),
                )
                .child(
                    text(i18n::t(lang, *k))
                        .font_size(ui_font())
                        .color(hex(ink()))
                        .nowrap(),
                ),
        );
    }

    // ── 右侧内容 ──
    let body = match tab {
        0 => appearance_tab(lang, draft),
        1 => view_settings_tab(lang, draft),
        2 => files_printing_tab(lang, draft, backend),
        3 => new_graph_tab(lang, draft),
        4 => analysis_settings_tab(lang, draft),
        5 => locations_tab(lang),
        _ => office_tab(lang, draft),
    };

    // 高度链条必须**每一环都确定**，滚动才会生效：
    //   外壳 col    → 固定 h(mh)（不是 max_h：max_h 下高度由内容决定，
    //                 内容一多就被裁掉，滚动区永远拿不到确定的可视高度）
    //   中间 row    → flex1() 吃掉剩余高度
    //   滚动区 div  → h_full() 跟满 row，scroll_y() 自带 min_height=0
    // 之前手算 body_h = mh - 28 - 38，只要有一处尺寸对不上（比如动画
    // 期间 mh 还在插值），可视高度就是错的，滚轮自然抓不到内容。
    col()
        // 不透明底色：对话框自身必须完全盖住底层界面，不能半透。
        .bg(hex(canvas_bg()))
        .border(1.0, hex(hair()))
        .w(mw)
        .h(mh)
        .mt(mdy)
        .rounded(mr)
        .shadow(crate::ui::shadow_pop())
        .child(bar)
        .child(
            row()
                .gap(0.0)
                .w_full()
                .flex1()
                .min_h(0.0)
                .child(side)
                .child(crate::ui::vsep())
                .child(
                    // 滚动容器必须是 **col()，不能用 div()**。
                    //
                    // charis 的滚动范围是这样算的（runtime 里对每个
                    // Behavior::Scroll 节点）：取它**直接子节点**的
                    // `location + size + margin` 的最大值，减去自身尺寸。
                    // 它不看孙节点，也不看溢出。
                    //
                    // 而 `div()` 默认是 **row**（flex row）。row 的交叉轴是
                    // 纵向，子节点默认 align-items: stretch —— 唯一的那个
                    // 子节点会被拉成和容器一样高，于是
                    // `content_size.h == 视口高`，max.y = 0，纵向永远滚不动。
                    //
                    // 换成 col() 后子节点沿纵向依次堆叠，
                    // content_size.h = 末尾子节点的 y + 高 = 真实内容高度。
                    // （检查器/导航器能滚动，正是因为它们用的是 col()。）
                    //
                    // padding 仍放在滚动容器**内部**，避免干扰可视高度。
                    col()
                        .flex1()
                        .min_h(0.0)
                        .min_w(0.0)
                        .scroll_y()
                        .child(div().w_full().p(10.0).child(col().w_full().gap(5.0).child(body))),
                ),
        )
        .child(hsep())
        .child(
            row()
                .h(38.0)
                .px(10.0)
                .gap(6.0)
                .items_center()
                .child(spacer())
                .child(button(i18n::t(lang, K::Ok)).on_click(Msg::SettingsOk))
                .child(button(i18n::t(lang, K::Apply)).on_click(Msg::SettingsApply))
                .child(button(i18n::t(lang, K::Close)).on_click(Msg::CloseSettings)),
        )
}


/// 0 外观：主题（含 1 秒轮询的跟随系统）、字体族与字号、导入 TTF、
/// 高对比、圆角、动画、窗口不透明度、无障碍、强调色。
fn appearance_tab(lang: Lang, s: &Settings) -> Element<Msg> {
    let t = |k: K| i18n::t(lang, k);
    let mut c = col()
        .gap(5.0)
        .w_full()
        .child(section(t(K::SettingsAppearance)))
        .child(label(t(K::SetTheme)));
    for (i, name) in Settings::themes(lang).iter().enumerate() {
        c = c.child(radio(s.theme == i, name.as_str(), Msg::Edit(Edit::Theme(i))));
    }
    c = c
        .child(hint(crate::theme::system_hint()))
        .child(section(t(K::SetFontFamily)))
        .child(pick_list(
            Settings::font_families(lang),
            Some(s.font_family),
            |i| Msg::Edit(Edit::FontFamily(i)),
        ).w_full())
        .child(slider_row(
            t(K::SetFontScale),
            format!("{:.0}%", s.font_scale * 100.0),
            slider(s.font_scale, 0.85, 1.60).on_change(|v| Msg::Edit(Edit::FontScale(v))),
        ))
        .child(prop_row(
            t(K::CustomFontName),
            text_input(s.custom_font.clone(), |v| Msg::Edit(Edit::CustomFont(v))).w_full(),
        ))
        .child(hint(t(K::ImportTtfHint)))
        .child(check(
            s.high_contrast,
            t(K::SetHighContrast),
            Msg::Edit(Edit::HighContrast(!s.high_contrast)),
        ))
        .child(slider_row(
            t(K::SetRadius),
            format!("{:.2}×", s.radius),
            slider(s.radius, 0.0, 2.0).on_change(|v| Msg::Edit(Edit::Radius(v))),
        ))
        .child(check(
            s.animate,
            t(K::SetAnimate),
            Msg::Edit(Edit::Animate(!s.animate)),
        ))
        .child(slider_row(
            t(K::SetOpacity),
            format!("{:.0}%", s.opacity * 100.0),
            slider(s.opacity, 0.5, 1.0).on_change(|v| Msg::Edit(Edit::Opacity(v))),
        ))
        .child(check(s.a11y, t(K::SetA11y), Msg::Edit(Edit::A11y(!s.a11y))))
        .child(section(t(K::SetAccent)))
        .child(
            row()
                .gap(4.0)
                .items_center()
                .child(swatch_row(&Settings::accents()))
                .child(pick_list(
                    Settings::accent_names(lang),
                    Some(s.accent),
                    |i| Msg::Edit(Edit::Accent(i)),
                )),
        );
    c
}

/// 1 视图设置：网格行列、导航器宽度、面板显隐、最近文件数、动画速度、缩放策略。
fn view_settings_tab(lang: Lang, s: &Settings) -> Element<Msg> {
    let t = |k: K| i18n::t(lang, k);
    col()
        .gap(5.0)
        .w_full()
        .child(section(t(K::ViewSettings)))
        .child(slider_row(
            t(K::SetGridRows),
            format!("{}", s.grid_rows),
            slider(s.grid_rows as f32, 5.0, 200.0).on_change(|v| Msg::Edit(Edit::GridRows(v))),
        ))
        .child(slider_row(
            t(K::SetGridCols),
            format!("{}", s.grid_cols),
            slider(s.grid_cols as f32, 2.0, 64.0).on_change(|v| Msg::Edit(Edit::GridCols(v))),
        ))
        .child(hint(t(K::GridRowsHint)))
        .child(slider_row(
            t(K::SetNavWidth),
            format!("{:.0}", s.nav_width),
            slider(s.nav_width, 150.0, 420.0).on_change(|v| Msg::Edit(Edit::NavWidth(v))),
        ))
        .child(check(
            s.show_navigator,
            t(K::SetShowNav),
            Msg::Edit(Edit::ShowNavigator(!s.show_navigator)),
        ))
        .child(check(
            s.show_inspector,
            t(K::SetShowInspector),
            Msg::Edit(Edit::ShowInspector(!s.show_inspector)),
        ))
        .child(check(
            s.show_status,
            t(K::SetShowStatus),
            Msg::Edit(Edit::ShowStatus(!s.show_status)),
        ))
        .child(slider_row(
            t(K::SetRecentCount),
            format!("{}", s.recent_count),
            slider(s.recent_count as f32, 0.0, 20.0)
                .on_change(|v| Msg::Edit(Edit::RecentCount(v))),
        ))
        .child(slider_row(
            t(K::SetAnimSpeed),
            format!("{} ms", s.anim_ms),
            slider(s.anim_ms as f32, 0.0, 600.0).on_change(|v| Msg::Edit(Edit::AnimMs(v))),
        ))
        .child(prop_row(
            t(K::SetZoomPolicy),
            pick_list(
                Settings::zoom_policies(lang),
                Some(s.zoom_policy),
                |i| Msg::Edit(Edit::ZoomPolicy(i)),
            ),
        ))
}

/// 2 文件与打印：目录、日期格式、作者、自动备份、页边距、页眉页脚。
fn files_printing_tab(lang: Lang, s: &Settings, backend: ExportBackend) -> Element<Msg> {
    let t = |k: K| i18n::t(lang, k);
    let dir = |v: &str| {
        if v.is_empty() {
            "(程序目录)".to_string()
        } else {
            v.to_string()
        }
    };
    col()
        .gap(5.0)
        .w_full()
        .child(section(t(K::FilesPrinting)))
        .child(prop_row(
            t(K::SetOpenDir),
            text_input(dir(&s.open_dir), |v| Msg::Edit(Edit::OpenDir(v))).w_full(),
        ))
        .child(prop_row(
            t(K::SetSaveDir),
            text_input(dir(&s.save_dir), |v| Msg::Edit(Edit::SaveDir(v))).w_full(),
        ))
        .child(prop_row(
            t(K::SetDateFmt),
            pick_list(
                Settings::date_formats(lang),
                Some(s.date_fmt),
                |i| Msg::Edit(Edit::DateFmt(i)),
            ),
        ))
        .child(prop_row(
            t(K::SetDateSep),
            pick_list(Settings::date_seps(), Some(s.date_sep), |i| {
                Msg::Edit(Edit::DateSep(i))
            }),
        ))
        .child(prop_row(
            t(K::SetAuthor),
            text_input(s.author.clone(), |v| Msg::Edit(Edit::Author(v))).w_full(),
        ))
        .child(check(
            s.auto_backup,
            t(K::SetAutoBackup),
            Msg::Edit(Edit::AutoBackup(!s.auto_backup)),
        ))
        .child(slider_row(
            t(K::SetBackupMin),
            format!("{}", s.backup_min),
            slider(s.backup_min as f32, 1.0, 120.0)
                .on_change(|v| Msg::Edit(Edit::BackupMin(v))),
        ))
        .child(hint(t(K::BackupHint)))
        .child(check(
            s.save_before_table,
            t(K::SetSaveBeforeTable),
            Msg::Edit(Edit::SaveBeforeTable(!s.save_before_table)),
        ))
        .child(slider_row(
            t(K::SetMargin),
            format!("{:.0}", s.margin),
            slider(s.margin, 0.0, 60.0).on_change(|v| Msg::Edit(Edit::Margin(v))),
        ))
        .child(prop_row(
            t(K::SetHeader),
            text_input(s.header.clone(), |v| Msg::Edit(Edit::Header(v))).w_full(),
        ))
        .child(prop_row(
            t(K::SetFooter),
            text_input(s.footer.clone(), |v| Msg::Edit(Edit::Footer(v))).w_full(),
        ))
        .child(section(t(K::SettingsExport)))
        .child(export_tab(lang, backend))
}

/// 3 新建图表：各图型的默认参数、配色、默认色序、点/线/误差棒、标注条默认结构。
fn new_graph_tab(lang: Lang, s: &Settings) -> Element<Msg> {
    let t = |k: K| i18n::t(lang, k);
    col()
        .gap(5.0)
        .w_full()
        .child(section(t(K::NewGraphSettings)))
        .child(hint(t(K::DefGraphHint)))
        .child(prop_row(
            t(K::SetDefKind),
            pick_list(Settings::chart_kinds(lang), Some(s.def_kind), |i| {
                Msg::Edit(Edit::DefKind(i))
            }),
        ))
        .child(prop_row(
            t(K::SetDefColormap),
            pick_list(
                crate::colormap::colormap_names(lang == Lang::Zh),
                Some(s.def_colormap),
                |i| Msg::Edit(Edit::DefColormap(i)),
            ),
        ))
        .child(slider_row(
            t(K::SetDefColorIdx),
            format!("{}", s.def_color_idx + 1),
            slider(s.def_color_idx as f32, 0.0, 11.0)
                .on_change(|v| Msg::Edit(Edit::DefColorIdx(v as usize))),
        ))
        .child(check(
            s.def_points,
            t(K::SetDefPoints),
            Msg::Edit(Edit::DefPoints(!s.def_points)),
        ))
        .child(check(
            s.def_line,
            t(K::SetDefLine),
            Msg::Edit(Edit::DefLine(!s.def_line)),
        ))
        .child(check(
            s.def_error,
            t(K::SetDefError),
            Msg::Edit(Edit::DefError(!s.def_error)),
        ))
        .child(prop_row(
            t(K::SetDefBracket),
            pick_list(
                Settings::bracket_defaults(lang),
                Some(s.def_bracket),
                |i| Msg::Edit(Edit::DefBracket(i)),
            ),
        ))
        .child(prop_row(
            t(K::SetDefBracketLabel),
            text_input(s.def_bracket_label.clone(), |v| {
                Msg::Edit(Edit::DefBracketLabel(v))
            })
            .w_full(),
        ))
}

/// 4 分析：置信水平、α、小数位、P 值格式、科学记数、随机种子、
/// 残差图、报告格式、离群值检测、编码。
fn analysis_settings_tab(lang: Lang, s: &Settings) -> Element<Msg> {
    let t = |k: K| i18n::t(lang, k);
    col()
        .gap(5.0)
        .w_full()
        .child(section(t(K::AnalysisSettings)))
        .child(prop_row(
            t(K::SetConfLevel),
            pick_list(Settings::conf_levels(), Some(s.conf_level), |i| {
                Msg::Edit(Edit::ConfLevel(i))
            }),
        ))
        .child(slider_row(
            t(K::SetAlpha),
            format!("{:.3}", s.alpha),
            slider(s.alpha, 0.001, 0.20).on_change(|v| Msg::Edit(Edit::Alpha(v))),
        ))
        // 小数位数用**下拉**而不是滑块：滑块拖出来的中间态没有意义
        // （3.5 位不存在），而且松开手才知道落点，看不出选中的是几位。
        // 这里给出 0–10 共 11 个明确档位，选中项直接回显。
        .child(prop_row(
            t(K::SetDecimals),
            pick_list(
                (0..=10).map(|i| format!("{}", i)).collect::<Vec<String>>(),
                Some(s.decimals.min(10)),
                |i| Msg::Edit(Edit::Decimals(i as f32)),
            ),
        ))
        .child(prop_row(
            t(K::SetCopyDec),
            pick_list(
                vec![
                    i18n::t(lang, K::CopyDecAsIs).to_string(),
                    i18n::t(lang, K::CopyDec2).to_string(),
                    i18n::t(lang, K::CopyDec4).to_string(),
                    i18n::t(lang, K::CopyDec6).to_string(),
                ],
                Some(s.copy_decimals.min(3)),
                |i| Msg::Edit(Edit::CopyDecimals(i)),
            ),
        ))
        .child(prop_row(
            t(K::SetCopySep),
            pick_list(
                vec![
                    i18n::t(lang, K::CopySepDot).to_string(),
                    i18n::t(lang, K::CopySepComma).to_string(),
                ],
                Some(s.copy_decimal_sep.min(1)),
                |i| Msg::Edit(Edit::CopyDecSep(i)),
            ),
        ))
        .child(prop_row(
            t(K::SetCopyEx),
            pick_list(
                vec![
                    i18n::t(lang, K::CopyExStar).to_string(),
                    i18n::t(lang, K::CopyExBlank).to_string(),
                    i18n::t(lang, K::CopyExRaw).to_string(),
                ],
                Some(s.copy_excluded.min(2)),
                |i| Msg::Edit(Edit::CopyExcluded(i)),
            ),
        ))
        // 下面八项之前只有数据结构、没有界面控件 —— 改了存得住但点不到。
        .child(prop_row(
            t(K::SetPFormat),
            pick_list(Settings::p_formats(lang), Some(s.p_format), |i| {
                Msg::Edit(Edit::PFormat(i))
            }),
        ))
        .child(prop_row(
            t(K::SetSciStyle),
            pick_list(Settings::sci_styles(), Some(s.sci_style), |i| {
                Msg::Edit(Edit::SciStyle(i))
            }),
        ))
        .child(check(
            s.auto_sci,
            t(K::SetAutoSci),
            Msg::Edit(Edit::AutoSci(!s.auto_sci)),
        ))
        .child(check(
            s.residual,
            t(K::SetResidual),
            Msg::Edit(Edit::Residual(!s.residual)),
        ))
        .child(prop_row(
            t(K::SetReportFmt),
            pick_list(Settings::report_formats(lang), Some(s.report_fmt), |i| {
                Msg::Edit(Edit::ReportFmt(i))
            }),
        ))
        .child(prop_row(
            t(K::SetOutlier),
            pick_list(Settings::outlier_methods(lang), Some(s.outlier), |i| {
                Msg::Edit(Edit::Outlier(i))
            }),
        ))
        .child(prop_row(
            t(K::SetEncoding),
            pick_list(Settings::encodings(), Some(s.encoding), |i| {
                Msg::Edit(Edit::Encoding(i))
            }),
        ))
        // GBK 需要转码库，目前写盘仍是 UTF-8 —— 在这里讲清楚，
        // 不放一个「选了没反应」的开关。
        .child_if(s.encoding == 2, || hint(t(K::EncodingGbkWarn)))
        // 多重比较与非线性模型：决定分析结果用哪套算法
        // 随机种子归到分析组：它管的是可复现性（K-means 质心、散点抖动），
        // 跟显示格式毫无关系 —— 以前挂在「自动科学记数法」下面纯属放错地方。
        .child(prop_row(
            t(K::SetSeed),
            row()
                .gap(6.0)
                .items_center()
                .child(text_input(s.seed.to_string(), |v| Msg::Edit(Edit::Seed(v))).w(150.0))
                .child(tool_button(
                    Glyph::Dice,
                    i18n::t(lang, K::SetSeedRandom),
                    Msg::RandomizeSeed,
                    false,
                )),
        ))
        .child(hint(t(K::SetSeedHint)))
        .child(prop_row(
            t(K::SetMultiMethod),
            pick_list(
                crate::analysis::MultiMethod::all()
                    .iter()
                    .map(|m| m.label().to_string())
                    .collect::<Vec<String>>(),
                Some(crate::analysis::MultiMethod::all()
                    .iter()
                    .position(|m| *m == s.multi_method)
                    .unwrap_or(0)),
                |i| Msg::Edit(Edit::MultiMethod(i)),
            ),
        ))
        .child(prop_row(
            t(K::SetNonlinearModel),
            pick_list(
                crate::analysis::ModelId::all()
                    .iter()
                    .map(|m| m.label().to_string())
                    .collect::<Vec<String>>(),
                Some(crate::analysis::ModelId::all()
                    .iter()
                    .position(|m| *m == s.nl_model)
                    .unwrap_or(0)),
                |i| Msg::Edit(Edit::NonlinearModel(i)),
            ),
        ))
}

/// 6 发送到 Office：标题、作者、样式、配色、字体名与字号、导出宽度。
fn office_tab(lang: Lang, s: &Settings) -> Element<Msg> {
    let t = |k: K| i18n::t(lang, k);
    col()
        .gap(5.0)
        .child(prop_row(
            t(K::SetOfficeTitle),
            text_input(s.office_title.clone(), |v| Msg::Edit(Edit::OfficeTitle(v))).w_full(),
        ))
        .child(prop_row(
            t(K::SetOfficeAuthor),
            text_input(s.office_author.clone(), |v| Msg::Edit(Edit::OfficeAuthor(v)))
                .w_full(),
        ))
        .child(prop_row(
            t(K::SetOfficeStyle),
            pick_list(Settings::office_styles(lang), Some(s.office_style), |i| {
                Msg::Edit(Edit::OfficeStyle(i))
            }),
        ))
        .child(prop_row(
            t(K::SetOfficeColormap),
            pick_list(
                crate::colormap::colormap_names(lang == Lang::Zh),
                Some(s.office_colormap),
                |i| Msg::Edit(Edit::OfficeColormap(i)),
            ),
        ))
        .child(prop_row(
            t(K::SetOfficeFont),
            text_input(s.office_font.clone(), |v| Msg::Edit(Edit::OfficeFont(v)))
                .w_full(),
        ))
        .child(slider_row(
            t(K::SetOfficeFontSize),
            format!("{:.0}", s.office_font_size),
            slider(s.office_font_size, 6.0, 48.0)
                .on_change(|v| Msg::Edit(Edit::OfficeFontSize(v))),
        ))
        .child(slider_row(
            t(K::SetExportWidth),
            format!("{:.0}", s.export_width),
            slider(s.export_width, 320.0, 4000.0)
                .on_change(|v| Msg::Edit(Edit::ExportWidth(v))),
        ))
}

/// 5 文件位置：可执行文件与源文件路径（只读，仅供核对）。
fn locations_tab(lang: Lang) -> Element<Msg> {
    let t = |k: K| i18n::t(lang, k);
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "-".to_string());
    // 编译期写死的源码根目录：发布版里不含源码，这里明确标注。
    let src_dir = env!("CARGO_MANIFEST_DIR").to_string();
    // 设置文件的位置：以前它落在工作目录里（cwd 随启动方式变化），现在固定到
    // 用户可写的配置目录。把路径摆出来，用户才能找到、备份、或删掉恢复默认。
    let cfg_path = crate::settings::Settings::path()
        .to_string_lossy()
        .to_string();
    let (cfg_label, cfg_hint) = match lang {
        Lang::Zh => ("设置文件路径", "删除此文件即可恢复默认设置"),
        Lang::En => ("Settings file", "Delete this file to reset all settings"),
    };
    col()
        .gap(5.0)
        .child(prop_row(t(K::SetExecPath), text(exe).font_size(ui_font()).color(hex(dim()))))
        .child(prop_row(
            t(K::SetSourcePath),
            text(src_dir).font_size(ui_font()).color(hex(dim())),
        ))
        .child(
            col()
                .gap(0.0)
                .child(prop_row(
                    cfg_label,
                    text(cfg_path).font_size(ui_font()).color(hex(dim())),
                ))
                .child(hint(cfg_hint)),
        )
}

/// 导出后端切换：Plotters SVG / LaTeX TikZ / Python Matplotlib。
///
/// 当 plotters 在文字旋转与单位混排上达不到期刊要求时，切到 TikZ 即可；
/// 数据结构与配色规范完全复用，只换代码生成器。
fn export_tab(lang: Lang, backend: ExportBackend) -> Element<Msg> {
    let t = |k: K| i18n::t(lang, k);
    let mut c = col()
        .gap(5.0)
        .w_full()
        .child(label(t(K::ExportBackendLabel)));
    for (i, b) in ExportBackend::ALL.iter().enumerate() {
        c = c.child(
            col()
                .gap(0.0)
                .child(radio(
                    *b == backend,
                    match *b {
                        ExportBackend::PlottersSvg => t(K::ExportPlottersSvg),
                        ExportBackend::TikZ => t(K::ExportTikZ),
                        ExportBackend::Matplotlib => t(K::ExportMatplotlib),
                    },
                    Msg::SetExportBackend(i),
                ))
                .child(hint(b.hint())),
        );
    }
    c
}

// ── 小工具 ──────────────────────────────────────────────────────────────────

fn label(s: &str) -> Element<Msg> {
    text(s.to_string()).font_size(ui_font()).semibold().color(hex(ink()))
}

fn hint(s: &str) -> Element<Msg> {
    text(s.to_string()).font_size(ui_font_sm()).color(hex(dim()))
}

#[allow(dead_code)]
fn path_text(s: &str) -> Element<Msg> {
    text(s.to_string()).font_size(ui_font_sm()).color(hex(dim()))
}

/// 一行色块预览（强调色候选）。
fn swatch_row(colors: &[&'static str]) -> Element<Msg> {
    let mut r = row().gap(3.0).items_center();
    for c in colors {
        r = r.child(
            div()
                .size(12.0, 12.0)
                .rounded(r_sm())
                .bg(hex(*c))
                .border(1.0, hex(hair())),
        );
    }
    r
}

/// 滑块行：标签 + 当前值 + 滑块本体。
fn slider_row(label: &str, value: String, control: Element<Msg>) -> Element<Msg> {
    row()
        .gap(6.0)
        .items_center()
        .w_full()
        .child(div().w(120.0).child(
            text(label.to_string()).font_size(ui_font()).color(hex(ink())).nowrap(),
        ))
        .child(div().w(52.0).child(
            text(value).font_size(ui_font_sm()).color(hex(dim())),
        ))
        .child(div().flex1().child(control))
}
