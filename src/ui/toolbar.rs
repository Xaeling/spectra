//! 功能区（Ribbon）：窗口顶部横向排列的八个大类卡片。
//!
//! # 与旧版的区别
//!
//! 旧版是 16 个小组（析光 / File / Sheet / … / Help），其中大量按钮点了没有
//! 实际行为，属于「看着有、按下去没反应」。现在按功能区说明文档重排成八个
//! 语义大类，每个条目都绑到一条**已实现**的动作上。
//!
//! # 排列规则（与文档一致）
//!
//! - 1~2 个条目 → 大按钮（图标在上、文字在下，纵向占满两行）
//! - ≥3 个条目 → 小图标两行，第一行 `(n+1)/2` 个，其余第二行
//! - 带 `▾` 的按钮：点图标执行主操作，点箭头弹出菜单
//! - 卡片底部居中显示大类名称
//!
//! # 菜单为什么挂在窗口根
//!
//! 下拉窗格要盖在下方内容之上，必须脱离 flex 流（否则会把工作区挤窄，
//! 和右键菜单之前的表现一样）。所以箭头按下时只把「哪个菜单 + 卡片左边缘
//! 的 x」发进 `Msg::OpenRibbonMenu`，真正的浮层由 `app::view` 在窗口根上
//! 用 `absolute()` 绘制。x 在构建卡片时就能算出来，不需要运行时查询布局。

use charis_ui::prelude::*;
use charis_ui::style::Shadow;

use crate::app::{Msg, RibbonMenu};
use crate::i18n::{self, K, Lang};
use crate::palette::*;
use crate::ui::motion;

const ICON: f32 = 24.0;
const BIG_W: f32 = 62.0;
const BIG_H: f32 = 57.0;
const ARROW_W: f32 = 12.0;
/// 卡片之间的水平间距。
const CARD_GAP: f32 = 6.0;
/// 整行左右外边距。
const ROW_PAD: f32 = 6.0;
/// 下拉窗格相对窗口顶部的 y：菜单栏 + 卡片区之下。
pub const RIBBON_MENU_TOP: f32 = 104.0;

/// 一个卡片里的条目。
enum Item {
    /// 小图标，点击执行
    Icon(crate::ui::Glyph, &'static str, Msg),
    /// 小图标 + 下拉箭头
    Split(crate::ui::Glyph, &'static str, Msg, RibbonMenu),
    /// 大按钮：只有图标，尺寸比小图标大，纵向占满两行
    Big(crate::ui::Glyph, &'static str, Msg),
}

/// 一个卡片：名称、条目、宽度（用于算菜单的 x）。
struct Card {
    name: &'static str,
    items: Vec<Item>,
    w: f32,
}

/// `active` 是当前保持按下状态的工具（保留兼容，功能区里只有分析类会用）。
pub fn toolbar(lang: Lang, _active: &'static str) -> Element<Msg> {
    let t = |k: K| i18n::t(lang, k);
    let cards = cards(&t);

    let mut bar = row().gap(CARD_GAP).items_center().px(ROW_PAD).py(2.0);
    let mut x = ROW_PAD;
    for c in &cards {
        bar = bar.child(card(c, x));
        x += c.w + CARD_GAP;
    }
    bar = bar.child(spacer());

    col()
        .w_full()
        .bg(hex(toolbar_bg()))
        .child(bar.scroll_x())
        .child(crate::ui::hsep())
}

fn cards(t: &impl Fn(K) -> &'static str) -> Vec<Card> {
    use crate::ui::Glyph as G;
    vec![
        // 一、New & File —— 3 个小图标，两行（上 2 / 下 1）
        Card {
            name: t(K::RibbonFile),
            items: vec![
                Item::Split(G::Plus, t(K::TipNewTableGraph), Msg::NewProject, RibbonMenu::New),
                Item::Icon(G::Folder, t(K::TipOpen), Msg::OpenProject),
                Item::Split(G::Floppy, t(K::TipSave), Msg::SaveProject, RibbonMenu::Save),
            ],
            w: 2.0 * (ICON + ARROW_W) + 10.0,
        },
        // 二、Table —— 4 个小图标，两行各 2
        Card {
            name: t(K::RibbonTable),
            items: vec![
                Item::Split(G::Highlight, t(K::TipHighlightTable), Msg::SetHighlight(7), RibbonMenu::HighlightTable),
                Item::Icon(G::Freeze, t(K::TipFreeze), Msg::ToggleFreeze),
                Item::Split(G::Plus, t(K::TipNewTable), Msg::NewDataTable, RibbonMenu::NewTable),
                Item::Icon(G::Bars, t(K::TipNewGraph), Msg::NewGraph),
            ],
            w: 2.0 * (ICON + ARROW_W) + 10.0,
        },
        // 三、Undo/Redo —— 2 个大按钮
        Card {
            name: t(K::TbUndo),
            items: vec![
                Item::Big(G::Undo, t(K::TipUndo), Msg::DoUndo),
                Item::Big(G::Redo, t(K::TipRedo), Msg::DoRedo),
            ],
            w: 2.0 * BIG_W + 10.0,
        },
        // 四、Clipboard —— 5 个小图标，两行（上 3 / 下 2）
        Card {
            name: t(K::TbClipboard),
            items: vec![
                Item::Icon(G::Paste, t(K::TipPaste), Msg::PasteCell),
                Item::Icon(G::Cut, t(K::TipCut), Msg::CutCell),
                Item::Icon(G::Copy, t(K::TipCopy), Msg::CopyCell),
                Item::Icon(G::Copy, t(K::TipCopySel), Msg::CopyCell),
                Item::Split(G::Paste, t(K::TipPasteSpecial), Msg::PasteCell, RibbonMenu::Paste),
            ],
            w: 3.0 * ICON + ARROW_W + 14.0,
        },
        // 五、Analysis —— 2 个大按钮
        Card {
            name: t(K::TbAnalysis),
            items: vec![
                Item::Big(G::Analyze, t(K::TipAnalyze), Msg::RunAnalysis),
                Item::Big(G::Wand, t(K::TipMagic), Msg::MagicApply),
            ],
            w: 2.0 * BIG_W + 10.0,
        },
        // 六、Change —— 5 个小图标，两行（上 3 / 下 2）
        Card {
            name: t(K::TbChange),
            items: vec![
                Item::Split(G::Insert, t(K::TipInsert), Msg::InsertRow, RibbonMenu::Insert),
                Item::Split(G::Delete, t(K::TipDelete), Msg::DeleteRow, RibbonMenu::Delete),
                Item::Split(G::Highlight, t(K::TipHighlightCell), Msg::SetHighlight(7), RibbonMenu::HighlightCell),
                Item::Icon(G::Format, t(K::TipFormat), Msg::Tool("format")),
                Item::Icon(G::Decimal, t(K::TipDecimal), Msg::Tool("decimal")),
            ],
            w: 3.0 * ICON + 3.0 * ARROW_W + 16.0,
        },
        // 七、Data —— 3 个小图标，两行（上 2 / 下 1）
        Card {
            name: t(K::RibbonData),
            items: vec![
                Item::Split(G::Sort, t(K::TipSort), Msg::ReverseRowOrder, RibbonMenu::Sort),
                Item::Icon(G::Series, t(K::TipSeries), Msg::CreateSeries),
                Item::Icon(G::Exclude, t(K::TipExclude), Msg::ExcludeValues),
            ],
            w: 2.0 * ICON + ARROW_W + 12.0,
        },
        // 八、Import —— 1 个大按钮
        Card {
            name: t(K::TbImport),
            items: vec![Item::Big(G::Import, t(K::TipImport), Msg::ImportCsv)],
            w: BIG_W + 10.0,
        },
    ]
}

fn card(c: &Card, x: f32) -> Element<Msg> {
    let n = c.items.len();
    let big = n > 0 && n <= 2;
    let body = if big {
        c.items.iter().enumerate().fold(
            row().gap(2.0).items_center(),
            |r, (_i, it)| r.child(render_item(it, x)),
        )
    } else {
        // ≥3 个：小图标两行，第一行 (n+1)/2 个
        let split_at = (n + 1) / 2;
        let mut rows = col().gap(1.0);
        for chunk in c.items.chunks(split_at) {
            let mut r = row().gap(2.0).items_center();
            for it in chunk {
                r = r.child(render_small(it, x));
            }
            rows = rows.child(r);
        }
        rows
    };

    col()
        .w(c.w)
        .px(5.0)
        .py(3.0)
        .gap(2.0)
        .rounded(r_md())
        .border(1.0, hex(hair()))
        .bg(hex(canvas_bg()))
        .child(body)
        .child(
            div()
                .w_full()
                .center()
                .child(
                    text(c.name)
                        .font_size(10.0)
                        .color(hex(dim()))
                        .nowrap(),
                ),
        )
}

fn render_item(it: &Item, x: f32) -> Element<Msg> {
    match it {
        Item::Big(g, tip, m) => big_icon(*g, tip, m.clone()),
        _ => render_small(it, x),
    }
}

/// 大按钮：只有图标，纵向占满两行高。
///
/// 功能区里**所有按钮都只有图标**，不画任何文字；说明完全靠悬浮提示给出。
/// 大按钮与小图标的区别只在尺寸：条目数 1~2 时用大按钮，≥3 时用小图标。
fn big_icon(g: crate::ui::Glyph, tip: &'static str, m: Msg) -> Element<Msg> {
    div()
        .size(BIG_W, BIG_H)
        .center()
        .rounded(r_sm())
        .hover(|s| s.bg(hex(tool_hover())))
        .on_click(m)
        .tooltip(tip)
        .child(crate::ui::glyph_at(g, 28.0))
}

fn render_small(it: &Item, x: f32) -> Element<Msg> {
    match it {
        Item::Icon(g, tip, m) => div()
            .size(ICON, ICON)
            .rounded(r_sm())
            .hover(|s| s.bg(hex(tool_hover())))
            .on_click(m.clone())
            .tooltip(*tip)
            .center()
            .child(crate::ui::glyph(*g)),
        Item::Split(g, tip, m, menu) => {
            let menu = *menu;
            row()
                .gap(0.0)
                .child(
                    div()
                        .size(ICON, ICON)
                        .rounded(r_sm())
                        .hover(|s| s.bg(hex(tool_hover())))
                        .on_click(m.clone())
                        .tooltip(*tip)
                        .center()
                        .child(crate::ui::glyph(*g)),
                )
                .child(
                    div()
                        .size(ARROW_W, ICON)
                        .rounded(r_sm())
                        .hover(|s| s.bg(hex(tool_hover())))
                        // x 是这张卡片左边缘的位置：菜单要对齐到这里，
                        // 在构建卡片时就能算出，不需要运行时查询布局。
                        .on_click(Msg::OpenRibbonMenu(menu, x))
                        .center()
                        .child(crate::ui::glyph_at(crate::ui::Glyph::Chevron, 8.0)),
                )
        }
        Item::Big(g, tip, m) => big_icon(*g, tip, m.clone()),
    }
}

// ══════════════════════════════════════════════════════════════════════════
// 下拉窗格
// ══════════════════════════════════════════════════════════════════════════

const MI_W: f32 = 224.0;
const MI_H: f32 = 24.0;

/// 窗格里的一行：要么是可点的项，要么是分隔线。
enum Mi {
    Item(&'static str, Msg, bool),
    Sep,
}

pub fn ribbon_menu_popup(
    lang: Lang,
    kind: RibbonMenu,
    x: f32,
    opened_at: std::time::Instant,
) -> Element<Msg> {
    let t = |k: K| i18n::t(lang, k);
    // 高亮色板：索引 7 是「无」，0..6 依次为红橙黄绿青蓝紫。
    let hl = || {
        vec![
            Mi::Item(t(K::MiHighlightNone), Msg::SetHighlight(7), true),
            Mi::Item(t(K::MiHighlightRed), Msg::SetHighlight(0), true),
            Mi::Item(t(K::MiHighlightOrange), Msg::SetHighlight(1), true),
            Mi::Item(t(K::MiHighlightYellow), Msg::SetHighlight(2), true),
            Mi::Item(t(K::MiHighlightGreen), Msg::SetHighlight(3), true),
            Mi::Item(t(K::MiHighlightCyan), Msg::SetHighlight(4), true),
            Mi::Item(t(K::MiHighlightBlue), Msg::SetHighlight(5), true),
            Mi::Item(t(K::MiHighlightPurple), Msg::SetHighlight(6), true),
        ]
    };
    let items: Vec<Mi> = match kind {
        RibbonMenu::New => vec![
            Mi::Item(t(K::MiNewProject), Msg::NewProject, true),
            Mi::Item(t(K::MiNewDataTable), Msg::NewDataTable, true),
        ],
        RibbonMenu::Save => vec![
            Mi::Item(t(K::Save), Msg::SaveProject, true),
            Mi::Item(t(K::SaveAs), Msg::SaveAs, true),
            Mi::Item(t(K::MiSaveFamily), Msg::SaveFamily, true),
            Mi::Item(t(K::MiSaveExample), Msg::SaveExample, true),
            Mi::Item(t(K::MiSaveTemplate), Msg::SaveTemplate, true),
            Mi::Item(t(K::MiSaveMethod), Msg::SaveMethod, true),
            Mi::Item(t(K::MiDiff), Msg::Tool("save_diff"), true),
            Mi::Sep,
            Mi::Item(t(K::MiSaveCopy), Msg::SaveAs, true),
            Mi::Item(t(K::MiRenameFile), Msg::SaveAs, true),
        ],
        RibbonMenu::HighlightTable | RibbonMenu::HighlightCell => hl(),
        RibbonMenu::NewTable => vec![
            Mi::Item(t(K::MiNewTableGraph), Msg::NewDataTable, true),
            Mi::Item(t(K::MiNewTableNoGraph), Msg::NewDataTable, true),
            Mi::Item(t(K::MiNewInfo), Msg::Tool("info"), true),
            Mi::Item(t(K::MiNewAnalysis), Msg::NewAnalysis, true),
            Mi::Item(t(K::MiNewGraphExisting), Msg::NewGraph, true),
            Mi::Sep,
            Mi::Item(t(K::MiDuplicateTable), Msg::DuplicateTable, true),
            Mi::Item(t(K::MiDuplicateFamily), Msg::DuplicateFamily, true),
            Mi::Item(t(K::MiWand), Msg::MagicApply, true),
            Mi::Sep,
            Mi::Item(t(K::MiMergeFile), Msg::MergeFile, true),
        ],
        RibbonMenu::Paste => vec![
            Mi::Item(t(K::MiPasteData), Msg::PasteCell, true),
            Mi::Item(t(K::MiPasteEmbedded), Msg::PasteEmbedded, true),
            Mi::Item(t(K::MiPasteLinked), Msg::PasteLinked, true),
            Mi::Sep,
            Mi::Item(t(K::MiPasteTransposed), Msg::PasteTransposed, true),
            Mi::Item(t(K::MiPasteTransposedEmbedded), Msg::PasteTransposedEmbedded, true),
            Mi::Item(t(K::MiPasteTransposedLinked), Msg::PasteTransposedLinked, true),
            Mi::Sep,
            Mi::Item(t(K::MiPasteSpecialDots), Msg::PasteSpecial, true),
        ],
        RibbonMenu::Insert => vec![
            Mi::Item(t(K::MiInsertRow), Msg::InsertRow, true),
            Mi::Item(t(K::MiInsertCol), Msg::InsertCol, true),
            Mi::Item(t(K::MiInsertDataSet), Msg::AddDataSet, true),
        ],
        RibbonMenu::Delete => vec![
            Mi::Item(t(K::MiDeleteRow), Msg::DeleteRow, true),
            Mi::Item(t(K::MiDeleteCol), Msg::DeleteCol, true),
            Mi::Item(t(K::MiDeleteDataSet), Msg::DeleteDataSet, true),
        ],
        RibbonMenu::Sort => vec![
            Mi::Item(t(K::MiSortByX), Msg::SortByX, true),
            Mi::Item(t(K::MiSortByRowTitle), Msg::SortByRowTitle, true),
            Mi::Item(t(K::MiReverseRows), Msg::ReverseRowOrder, true),
        ],
    };

    // 入场：整块下落 + OutBack 过冲（与右键菜单同一套）
    let (po, pdy) = motion::panel_enter(opened_at);

    let mut menu = col()
        .w(MI_W)
        .bg(hex(canvas_bg()))
        .rounded(r_sm())
        .border(1.0, hex(hair()))
        .shadow(Shadow::new(0.0, 4.0, 16.0, 0.0, hex(shadow_pop_color())))
        .p(3.0)
        .opacity(po)
        .translate(0.0, pdy)
        .animate_layout(motion::layout_secs())
        .easing(motion::OUT_BACK);
    for (i, it) in items.into_iter().enumerate() {
        menu = menu.child(match it {
            Mi::Sep => div()
                .w(MI_W - 6.0)
                .h(1.0)
                .bg(hex(hair()))
                .opacity(motion::item_enter(opened_at, i).0),
            Mi::Item(label, m, on) => mi_row(label, m, on, opened_at, i),
        });
    }

    // 与右键菜单同一套做法：整屏捕获层与菜单本身都要 absolute，
    // 否则这一层会作为 flex 子项吃掉宽度、把工作区挤窄。
    div()
        .size_full()
        .on_click(Msg::CloseRibbonMenu)
        .child(
            menu
                .absolute()
                .left(x.max(0.0))
                .top(RIBBON_MENU_TOP),
        )
        .absolute()
        .left(0.0)
        .top(0.0)
}

fn mi_row(
    label: &'static str,
    m: Msg,
    on: bool,
    opened_at: std::time::Instant,
    i: usize,
) -> Element<Msg> {
    // Stagger：逐项从左侧 8px 滑入，间隔 0.06 s
    let (io, idx) = motion::item_enter(opened_at, i);
    let e = div()
        .w(MI_W)
        .h(MI_H)
        .px(10.0)
        .items_center()
        .rounded(r_sm())
        .opacity(io)
        .translate(idx, 0.0)
        .child(
            text(label)
                .font_size(ui_font())
                .color(hex(if on { ink() } else { dim() }))
                .nowrap(),
        );
    if on {
        e.hover(|s| s.bg(hex(tool_hover())))
            .on_click(m)
            .transition(motion::secs(motion::D_CONTROL))
            .easing(motion::std_easing())
    } else {
        e
    }
}
