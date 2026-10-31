//! 菜单栏：八个顶层菜单 —— File / Edit / View / Insert / Analyze / Change / Settings / Help。
//!
//! 与原版 Spectra 规范一致的几点：
//! - **Analyze 是动态菜单**：按当前选中表的类型生成条目，未选中表时显示提示。
//! - **Settings 的条目就是设置窗口左侧的分类**，点了直接跳到对应页。
//! - 复选项（冻结表 / 面板开关 / 高亮颜色）带 `check()`，单选带 `radio()`。
//! - 所有文案走 `i18n::t(lang, K::…)`，切语言即时重建。

use charis_ui::prelude::*;

use crate::app::Msg;
use crate::i18n::{self, K, Lang};

/// 高亮表颜色（与析光一致）：黄 / 红 / 蓝 / 绿 / 紫 / 橙 / 灰 / 无。
#[allow(dead_code)]
const HIGHLIGHTS: [(&str, &str); 8] = [
    ("#FFF2B2", "Yellow"),
    ("#F8C8C8", "Red"),
    ("#C6DCF5", "Blue"),
    ("#C9E8C9", "Green"),
    ("#DCCCE8", "Purple"),
    ("#FBD9AE", "Orange"),
    ("#DCDCDC", "Gray"),
    ("#FFFFFF", "None"),
];

/// 缩放档位。
const ZOOMS: [f32; 8] = [0.10, 0.50, 0.75, 1.00, 1.50, 2.00, 4.00, 6.00];

pub fn menus(
    lang: Lang,
    st: &MenuState,
    methods: &[(usize, &'static str, &'static str)],
) -> Vec<Menu<Msg>> {
    let t = |k: K| i18n::t(lang, k);
    vec![
        file_menu(lang, t, st),
        edit_menu(lang, t, st),
        view_menu(lang, t, st),
        insert_menu(lang, t),
        analyze_menu(lang, t, methods),
        change_menu(lang, t),
        settings_menu(lang, t),
        help_menu(lang, t),
    ]
}

/// 菜单需要读取的应用状态（复选 / 单选的当前值）。
#[derive(Clone, Copy, Default)]
pub struct MenuState {
    pub freeze_table: bool,
    pub highlight: usize,
    pub zoom: f32,
    pub main_toolbar: bool,
    pub navigator: bool,
    pub inspector: bool,
    pub bottom_toolbar: bool,
    pub rulers: bool,
    pub grid: bool,
    pub row_titles: bool,
    pub column_titles: bool,
}

// ══ 1. File ════════════════════════════════════════════════════════════════

fn file_menu(lang: Lang, t: impl Fn(K) -> &'static str + Copy, _st: &MenuState) -> Menu<Msg> {
    let _ = lang;
    Menu::new(
        t(K::MenuFile),
        vec![
            MenuItem::action(t(K::NewProject), Msg::NewProject),
            MenuItem::action(t(K::NewDataTableDots), Msg::NewDataTable).shortcut("Ctrl+N"),
            MenuItem::action(t(K::NewGraphDots), Msg::NewGraph),
            MenuItem::Separator,
            MenuItem::action(t(K::OpenDots), Msg::OpenProject).shortcut("Ctrl+O"),
            MenuItem::action(t(K::CloseSheet), Msg::CloseSheet).shortcut("Ctrl+W"),
            MenuItem::action(t(K::CloseAll), Msg::CloseAll),
            MenuItem::Separator,
            MenuItem::action(t(K::SaveDots), Msg::SaveProject).shortcut("Ctrl+S"),
            MenuItem::action(t(K::SaveAsDots), Msg::SaveAs).shortcut("Ctrl+Shift+S"),
            MenuItem::submenu(
                t(K::SaveSelected),
                vec![
                    MenuItem::action(t(K::SaveCurTable), Msg::SaveTableCsv),
                    MenuItem::action(t(K::SaveResults), Msg::SaveResultsCsv),
                    MenuItem::action(t(K::SaveCurGraph), Msg::SaveGraphSvg),
                ],
            ),
            MenuItem::Separator,
            MenuItem::action(t(K::ImportDots), Msg::ImportCsv),
            MenuItem::action(t(K::ImportSpectra), Msg::OpenProject),
            MenuItem::action(t(K::ExportSpectra), Msg::SaveAs),
            MenuItem::action(t(K::MergeDots), Msg::OpenProject),
            MenuItem::Separator,
            MenuItem::action(t(K::ExportDots), Msg::ExportChart).shortcut("Ctrl+Shift+E"),
            MenuItem::action(t(K::ExportMpl), Msg::ExportMatplotlib),
            MenuItem::action(t(K::ExportCsv), Msg::SaveTableCsv),
            MenuItem::Separator,
            MenuItem::action(t(K::PrintSetup), Msg::PrintSetup),
            MenuItem::action(t(K::PrintDots), Msg::PrintChart).shortcut("Ctrl+P"),
            MenuItem::submenu(
                t(K::SendMail),
                vec![
                    MenuItem::action(t(K::AsAttachment), Msg::SendChart),
                    MenuItem::action(t(K::ToPowerPoint), Msg::SendToPpt).shortcut("Ctrl+H"),
                    MenuItem::action(t(K::ToWord), Msg::SendToWord).shortcut("Ctrl+K"),
                ],
            ),
            MenuItem::submenu(
                t(K::CloudEln),
                vec![
                    MenuItem::action(t(K::SpectraCloud), Msg::CloudSpectra),
                    MenuItem::action(t(K::LabArchives), Msg::ExportBundle),
                    MenuItem::action(t(K::Luma), Msg::CloudLuma),
                ],
            ),
            MenuItem::Separator,
            MenuItem::action(t(K::CheckCompat), Msg::CheckCompat),
            MenuItem::action(t(K::RunScript), Msg::RunScript),
            MenuItem::Separator,
            MenuItem::action(t(K::ExitSpectra), Msg::Quit),
        ],
    )
}

// ══ 2. Edit ════════════════════════════════════════════════════════════════

fn edit_menu(lang: Lang, t: impl Fn(K) -> &'static str + Copy, st: &MenuState) -> Menu<Msg> {
    Menu::new(
        t(K::MenuEdit),
        vec![
            MenuItem::action(t(K::Undo), Msg::DoUndo).shortcut("Ctrl+Z"),
            MenuItem::action(t(K::Redo), Msg::DoRedo).shortcut("Ctrl+Y"),
            MenuItem::Separator,
            MenuItem::action(t(K::Cut), Msg::CutCell).shortcut("Ctrl+X"),
            MenuItem::action(t(K::Copy), Msg::CopyCell).shortcut("Ctrl+C"),
            MenuItem::action(t(K::Paste), Msg::PasteCell).shortcut("Ctrl+V"),
            MenuItem::action(t(K::PasteTranspose), Msg::PasteTransposed).shortcut("Ctrl+Shift+T"),
            MenuItem::action(t(K::PasteSpecial), Msg::PasteSpecial).shortcut("Ctrl+Shift+V"),
            MenuItem::Separator,
            MenuItem::submenu(
                t(K::SelectMenu),
                vec![
                    MenuItem::action(t(K::SelectAll), Msg::SelectAll).shortcut("Ctrl+A"),
                    MenuItem::action(t(K::SelectColumn), Msg::SelectColumn),
                    MenuItem::action(t(K::SelectRow), Msg::SelectRow),
                ],
            ),
            MenuItem::submenu(
                t(K::SortMenu),
                vec![
                    MenuItem::action(t(K::SortByX), Msg::SortByX),
                    MenuItem::action(t(K::SortByRowTitle), Msg::SortByRowTitle),
                    MenuItem::action(t(K::ReverseRowOrder), Msg::ReverseRowOrder),
                ],
            ),
            MenuItem::Separator,
            MenuItem::action(t(K::ExcludeValues), Msg::ExcludeValues).shortcut("Ctrl+E"),
            MenuItem::action(t(K::ClearDel), Msg::ClearCells).shortcut("Del"),
            MenuItem::action(t(K::DeleteRowsCols), Msg::DeleteRow),
            MenuItem::action(t(K::DeleteTable), Msg::DeleteTable),
            MenuItem::action(t(K::TableOrder), Msg::TableOrder),
            MenuItem::Separator,
            MenuItem::check(t(K::FreezeTable), st.freeze_table, Msg::ToggleFreeze),
            MenuItem::action(t(K::RenameTable), Msg::RenameTable),
            MenuItem::submenu(t(K::HighlightTable), highlight_items(lang, st.highlight)),
            MenuItem::Separator,
            MenuItem::action(t(K::LinkDots), Msg::LinkTable),
            MenuItem::action(t(K::DataObject), Msg::DataObject),
            MenuItem::action(t(K::UnlinkCalc), Msg::UnlinkCalc),
        ],
    )
}

/// 高亮表：8 个单选（黄 / 红 / 蓝 / 绿 / 紫 / 橙 / 灰 / 无）。
fn highlight_items(lang: Lang, cur: usize) -> Vec<MenuItem<Msg>> {
    let keys = [
        K::HlYellow,
        K::HlRed,
        K::HlBlue,
        K::HlGreen,
        K::HlPurple,
        K::HlOrange,
        K::HlGray,
        K::HlNone,
    ];
    keys.iter()
        .enumerate()
        .map(|(i, k)| MenuItem::check(i18n::t(lang, *k), cur == i, Msg::SetHighlight(i)))
        .collect()
}

// ══ 3. View ════════════════════════════════════════════════════════════════

fn view_menu(lang: Lang, t: impl Fn(K) -> &'static str + Copy, st: &MenuState) -> Menu<Msg> {
    Menu::new(
        t(K::MenuView),
        vec![
            MenuItem::submenu(t(K::ZoomMenu), zoom_items(lang, st.zoom)),
            MenuItem::Separator,
            MenuItem::action(t(K::DataTablesView), Msg::GoToDataTable),
            MenuItem::action(t(K::Library), Msg::GoToLibrary),
            MenuItem::Separator,
            MenuItem::check(t(K::MainToolbar), st.main_toolbar, Msg::ToggleMainToolbar),
            MenuItem::check(t(K::Navigator), st.navigator, Msg::ToggleNavigator),
            MenuItem::check(t(K::GraphInspector), st.inspector, Msg::ToggleInspector),
            MenuItem::check(t(K::BottomToolbarView), st.bottom_toolbar, Msg::ToggleBottomBar),
            MenuItem::Separator,
            MenuItem::check(t(K::Rulers), st.rulers, Msg::ToggleRulers),
            MenuItem::check(t(K::GridView), st.grid, Msg::ToggleGridView),
            MenuItem::check(t(K::RowTitlesView), st.row_titles, Msg::ToggleRowTitles),
            MenuItem::check(t(K::ColumnTitlesView), st.column_titles, Msg::ToggleColTitles),
            MenuItem::Separator,
            MenuItem::action(t(K::RestoreNotif), Msg::RestoreNotif),
        ],
    )
}

/// 缩放：适应页面 / 适应图表 / 放大 / 缩小 + 8 个档位单选。
fn zoom_items(lang: Lang, cur: f32) -> Vec<MenuItem<Msg>> {
    let t = |k: K| i18n::t(lang, k);
    let mut v = vec![
        MenuItem::action(t(K::ZoomFitPage), Msg::ZoomFitPage),
        MenuItem::action(t(K::ZoomFitChart), Msg::ZoomFitChart),
        MenuItem::action(t(K::ZoomIn), Msg::ZoomIn).shortcut("Ctrl+="),
        MenuItem::action(t(K::ZoomOut), Msg::ZoomOut).shortcut("Ctrl+-"),
        MenuItem::Separator,
    ];
    for z in ZOOMS {
        v.push(MenuItem::check(
            format!("{}%", (z * 100.0) as i32),
            (cur - z).abs() < 1e-6,
            Msg::SetZoom(z),
        ));
    }
    v
}

// ══ 4. Insert ══════════════════════════════════════════════════════════════

fn insert_menu(lang: Lang, t: impl Fn(K) -> &'static str + Copy) -> Menu<Msg> {
    let _ = lang;
    Menu::new(
        t(K::MenuInsert),
        vec![
            MenuItem::action(t(K::NewTableGraph), Msg::NewDataTable),
            MenuItem::action(t(K::NewTableNoGraph), Msg::NewDataTable),
            MenuItem::action(t(K::NewInfo), Msg::NewInfo),
            MenuItem::action(t(K::NewAnalysisDots), Msg::NewAnalysis),
            MenuItem::action(t(K::GraphFromExisting), Msg::NewGraph),
            MenuItem::Separator,
            MenuItem::action(t(K::DuplicateTable), Msg::DuplicateTable),
            MenuItem::action(t(K::DuplicateSeries), Msg::DuplicateSeries),
            MenuItem::action(t(K::MagicExample), Msg::MagicApply),
            MenuItem::Separator,
            MenuItem::action(t(K::RowsColsDots), Msg::InsertRow),
            MenuItem::action(t(K::InsertCol), Msg::InsertCol),
            MenuItem::action(t(K::CreateSeries), Msg::CreateSeries),
            MenuItem::action(t(K::CalcVariable), Msg::CalcVariable),
            MenuItem::Separator,
            MenuItem::action(t(K::RiskTable), Msg::RiskTable),
            MenuItem::action(t(K::EmbedTable), Msg::EmbedTable),
            MenuItem::action(t(K::InsertChar), Msg::InsertChar),
            MenuItem::action(t(K::InsertConst), Msg::InsertConst),
            MenuItem::action(t(K::InsertObject), Msg::InsertObject),
            MenuItem::action(t(K::FloatingNote), Msg::AddText),
        ],
    )
}

// ══ 5. Analyze（动态）══════════════════════════════════════════════════════

/// 按当前表的类型生成条目。未选中表时给出提示项。
fn analyze_menu(
    lang: Lang,
    t: impl Fn(K) -> &'static str + Copy,
    methods: &[(usize, &'static str, &'static str)],
) -> Menu<Msg> {
    let items = if methods.is_empty() {
        // 没有选中数据表：给一条可点的提示项（点了在状态栏再说一遍）
        vec![MenuItem::action(t(K::SelectTableFirst), Msg::HintInspector)]
    } else {
        methods
            .iter()
            .map(|(id, zh, en)| {
                let name = if lang == Lang::En { *en } else { *zh };
                MenuItem::action(name, Msg::RunAnalysisById(*id))
            })
            .collect()
    };
    Menu::new(t(K::MenuAnalyze), items)
}

// ══ 6. Change ══════════════════════════════════════════════════════════════

fn change_menu(lang: Lang, t: impl Fn(K) -> &'static str + Copy) -> Menu<Msg> {
    let _ = lang;
    Menu::new(
        t(K::MenuChange),
        vec![
            MenuItem::action(t(K::GraphTypeDots), Msg::CycleChartKind),
            MenuItem::action(t(K::FlipHorizontal), Msg::FlipHorizontal),
            MenuItem::Separator,
            MenuItem::action(t(K::RemoveReplaceDs), Msg::RemoveLastDataSet),
            MenuItem::action(t(K::AddDsDots), Msg::AddDataSet),
            MenuItem::action(t(K::DsOrder), Msg::DataSetOrder),
            MenuItem::action(t(K::ReverseDsOrder), Msg::ReverseDataSetOrder),
            MenuItem::Separator,
            MenuItem::action(t(K::DefineColorScheme), Msg::OpenColorSettings),
            MenuItem::Separator,
            MenuItem::action(t(K::FramesOrigin), Msg::HintInspector),
            MenuItem::action(t(K::XAxisDots), Msg::HintInspector),
            MenuItem::action(t(K::LeftYAxis), Msg::HintInspector),
            MenuItem::action(t(K::RightYAxis), Msg::HintInspector),
            MenuItem::action(t(K::AxisTitles), Msg::HintInspector),
            MenuItem::action(t(K::BarAppearance), Msg::HintInspector),
            MenuItem::Separator,
            MenuItem::action(t(K::ReverseLegend), Msg::ReverseLegend),
            MenuItem::Separator,
            MenuItem::action(t(K::AnalysisParams), Msg::AnalysisParams),
            MenuItem::action(t(K::Resimulate), Msg::Resimulate).shortcut("F9"),
            MenuItem::Separator,
            MenuItem::action(t(K::SelEmbedTable), Msg::EmbedTable),
            MenuItem::action(t(K::ObjectsDots), Msg::InsertObject),
            MenuItem::action(t(K::TextDots), Msg::AddText),
            MenuItem::Separator,
            MenuItem::action(t(K::ResizeGraph), Msg::ResizeGraph),
        ],
    )
}

// ══ 7. Settings ════════════════════════════════════════════════════════════

/// 菜单项即设置窗口左侧的分类，点了跳到对应页。
fn settings_menu(lang: Lang, t: impl Fn(K) -> &'static str + Copy) -> Menu<Msg> {
    let _ = lang;
    Menu::new(
        t(K::Settings),
        vec![
            MenuItem::action(t(K::SettingsAppearanceView), Msg::SettingsTab(0)),
            MenuItem::action(t(K::ViewSettings), Msg::SettingsTab(1)),
            MenuItem::action(t(K::FilesPrinting), Msg::SettingsTab(2)),
            MenuItem::action(t(K::NewGraphSettings), Msg::SettingsTab(3)),
            MenuItem::action(t(K::AnalysisSettings), Msg::SettingsTab(4)),
            MenuItem::action(t(K::FileLocations), Msg::SettingsTab(5)),
            MenuItem::action(t(K::SendToOffice), Msg::SettingsTab(6)),
        ],
    )
}

// ══ 8. Help ════════════════════════════════════════════════════════════════

fn help_menu(lang: Lang, t: impl Fn(K) -> &'static str + Copy) -> Menu<Msg> {
    let _ = lang;
    Menu::new(
        t(K::MenuHelp),
        vec![MenuItem::action(t(K::AboutSpectra), Msg::About)],
    )
}

/// 设置窗口的分类数（菜单项与之对应，见 settings.rs 的 `TABS`）。
#[allow(dead_code)]
pub const SETTINGS_TABS: usize = 7;
