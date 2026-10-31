//! 表格右键上下文菜单。
//!
//! charis 的 `Element::on_context_menu` 会在右键时给出**指针位置**，
//! 我们在那里画一个自绘浮层。之所以不用框架内置菜单：这里需要的是
//! 「就地、随开随关、不抢焦点」的轻量菜单，自绘反而更可控。
//!
//! 菜单项按作用域区分：
//! - 单元格：复制 / 剪切 / 粘贴 / 清除 / 全选 / 插入行 / 删除行
//! - 行标题：选中整行 / 插入行 / 删除行 / 重命名行
//! - 列标题：选中整列 / 新增数据集 / 删除数据集 / 重命名列

use charis_ui::prelude::*;
use charis_ui::style::Shadow;

use crate::app::{CtxAction, CtxMenuState, CtxScope, Msg};
use crate::i18n::{self, K, Lang};
use crate::palette::*;
use crate::ui::motion;

/// 菜单项高度与宽度。宽度取 176 是为了让最长的「插入行 / 删除行」不折行。
const ITEM_H: f32 = 24.0;
const MENU_W: f32 = 168.0;

pub fn context_menu(lang: Lang, m: CtxMenuState) -> Element<Msg> {
    let items: Vec<CtxAction> = match m.scope {
        CtxScope::Cell => vec![
            CtxAction::Copy,
            CtxAction::Cut,
            CtxAction::Paste,
            CtxAction::Clear,
            CtxAction::SelectAll,
            CtxAction::InsertRow,
            CtxAction::DeleteRow,
        ],
        CtxScope::Row => vec![
            CtxAction::SelectRow,
            CtxAction::Copy,
            CtxAction::InsertRow,
            CtxAction::DeleteRow,
            CtxAction::RenameRow,
        ],
        CtxScope::Col => vec![
            CtxAction::SelectCol,
            CtxAction::Copy,
            CtxAction::AddDataSet,
            CtxAction::RemoveDataSet,
            CtxAction::RenameCol,
        ],
        // 导航器条目：按文件夹给出能做的动作。
        // Sheet 作用域下 CtxMenuState 的 col/row 存的是 (文件夹, 索引)，
        // 所以这里读它们来决定菜单内容 —— 不读就会漏掉文件夹信息。
        CtxScope::Sheet => {
            let folder = m.col;
            match folder {
                0 => vec![
                    CtxAction::OpenSheet,
                    CtxAction::RenameSheet,
                    CtxAction::DuplicateSheet,
                    CtxAction::ToggleFreezeSheet,
                    CtxAction::DeleteSheet,
                ],
                3 => vec![
                    CtxAction::OpenSheet,
                    CtxAction::RenameSheet,
                    CtxAction::DuplicateSheet,
                    CtxAction::ToggleFreezeSheet,
                    CtxAction::ExportGraphSheet,
                    CtxAction::DeleteSheet,
                ],
                _ => vec![
                    CtxAction::OpenSheet,
                    CtxAction::RenameSheet,
                    CtxAction::DeleteSheet,
                ],
            }
        }
    };

    // 定位：**必须用绝对定位**。
    //
    // 之前用 ml/mt 偏移、后来改用占位块 div().w(x)，两种做法都在 flex 流内，
    // 都会让这一层的宽度变成 x + 菜单宽 —— 而它的父容器正是
    // 「导航器 | 表格 | 检查器」那一整行，一被撑宽右边栏就被挤到左边。
    //
    // absolute() 让元素脱离 flex 流，自身尺寸完全不参与父级布局计算，
    // 因此无论鼠标在多靠右的位置，都不可能影响右侧栏。
    let x = m.x.max(0.0);
    let y = m.y.max(0.0);

    // 入场：整块从上方 8px 处「落」下来，带 OutBack 过冲（重量感），
    // 位移用 translate（绘制期偏移，不参与布局），所以不会挤动周围元素。
    let (po, pdy) = motion::panel_enter(m.opened_at);

    let mut menu = col()
        .w(MENU_W)
        .bg(hex(canvas_bg()))
        .rounded(r_sm())
        .border(1.0, hex(hair()))
        .shadow(Shadow::new(0.0, 4.0, 16.0, 0.0, hex(shadow_pop_color())))
        .p(3.0)
        .opacity(po)
        .translate(0.0, pdy)
        // 尺寸/位置变化走 FLIP（子元素跟随），而不是每帧改 height 触发重排
        .animate_layout(motion::layout_secs())
        .easing(motion::OUT_BACK);
    for (i, a) in items.into_iter().enumerate() {
        menu = menu.child(item(lang, a, m.opened_at, i, m.frozen));
    }

    // 整屏透明捕获层：点空白即关闭。
    //
    // **这一层自己也必须 absolute**。
    // 之前只把内部 menu 设成 absolute，外层仍是 `size_full()` 的正常 flex 子项：
    // view() 里它是 `div().size_full().child(body).child(菜单层)` 的第二个 child，
    // 而 `div()` 默认按 row 排列 —— 两个 child 并排，外层吃掉一整份宽度，
    // 于是 body（导航器|表格|检查器）被挤窄，检查器被推到左边。
    // 把捕获层整体 absolute 后，它完全脱离 flex 流，不再参与父级布局计算。
    div()
        .size_full()
        .on_click(Msg::CloseCtxMenu)
        .child(menu.absolute().left(x).top(y))
        .absolute()
        .left(0.0)
        .top(0.0)
}


fn item(
    lang: Lang,
    a: CtxAction,
    opened_at: std::time::Instant,
    i: usize,
    frozen: bool,
) -> Element<Msg> {
    let label = i18n::t(lang, match a {
        CtxAction::Copy => K::Copy,
        CtxAction::Cut => K::Cut,
        CtxAction::Paste => K::Paste,
        CtxAction::Clear => K::ClearCells,
        CtxAction::SelectAll => K::SelectAll,
        CtxAction::SelectCol => K::SelectCol,
        CtxAction::SelectRow => K::SelectRow,
        CtxAction::InsertRow => K::InsertRow,
        CtxAction::DeleteRow => K::DeleteRow,
        CtxAction::AddDataSet => K::AddDataSet,
        CtxAction::RemoveDataSet => K::RemoveLastDataSet,
        CtxAction::RenameCol => K::RenameCol,
        CtxAction::RenameRow => K::RenameRow,
        CtxAction::OpenSheet => K::CtxOpen,
        CtxAction::RenameSheet => K::CtxRename,
        CtxAction::DuplicateSheet => K::CtxDuplicate,
        CtxAction::ToggleFreezeSheet => {
            // 已冻结的显示「解冻」，未冻结显示「冻结」。
            if frozen {
                K::CtxUnfreeze
            } else {
                K::CtxFreeze
            }
        }
        CtxAction::DeleteSheet => K::CtxDelete,
        CtxAction::ExportGraphSheet => K::CtxExportGraph,
    });
    // Stagger：第 i 项延迟 i × 0.06 s 从左侧 8px 滑入。
    // Charis 没有 transition-delay，按文档的近似做法自己算进度。
    let (io, idx) = motion::item_enter(opened_at, i);
    div()
        .w(MENU_W)
        .h(ITEM_H)
        .px(10.0)
        .items_center()
        .rounded(r_sm())
        .on_click(Msg::CtxMenu(a))
        .opacity(io)
        .translate(idx, 0.0)
        .hover(|s| s.bg(hex(tool_hover())))
        .transition(motion::secs(motion::D_CONTROL))
        .easing(motion::std_easing())
        .child(text(label).font_size(ui_font()).color(hex(ink())))
}
