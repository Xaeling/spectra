//! 左侧导航器：Search 框 + 5 个文件夹。
//!
//! 析光的导航器规则（这里逐条还原）：
//! - 五个文件夹顺序固定：Data Tables / Info / Results / Graphs / Layouts
//! - 当前表背景高亮；与当前表关联的表用粗体；冻结的表用斜体
//! - 每个文件夹底部有 "+ New …" 蓝色链接
//!
//! 高度策略：表列表 `flex1 + scroll_y` 吃掉全部剩余空间。

use charis_ui::prelude::*;

use crate::app::{Msg, SpectraApp};
use crate::i18n::{self, K, Lang};
use crate::model::FOLDERS;

/// 导航器里不再显示的文件夹：「信息」。
///
/// 只影响渲染，不影响编号 —— `Active(folder, index)` 与已存工程里的引用
/// 都按原编号走，硬把 FOLDERS 缩成 3 项会让它们整体错位。
const FOLDER_INFO: usize = 1;
use crate::palette::*;
use crate::ui::{folder_icon, glyph_at, triangle, Glyph};

pub fn navigator(app: &SpectraApp) -> Element<Msg> {
    let lang = app.lang;
    let t = |k: K| i18n::t(lang, k);
    let mut list = col().gap(0.0).w_full();

    for (fi, key) in FOLDERS.iter().enumerate() {
        // 「信息」文件夹整个不再显示。信息表是 Prism 的时代产物（手写实验备注），
        // 析光里没有对应功能，留着只会在导航器里多一个永远用不到的分组。
        // 这里只跳过渲染、不动 folder 编号 —— 1 仍是 Info，
        // 否则 Active(folder,index) 与已存工程里的引用会整体错位。
        if fi == FOLDER_INFO {
            continue;
        }
        let open = app.open_folders[fi];
        let count = app.folder_count(fi);

        // 文件夹头：三角 + 文件夹图标 + 名称
        list = list.child(
            row()
                .h(row_h() + 1.0)
                .pl(6.0)
                .gap(5.0)
                .items_center()
                .hover(|s| s.bg(hex(accent_soft())))
                .on_click(Msg::ToggleFolder(fi))
                .child(triangle(open, 8.0))
                .child(folder_icon(13.0))
                .child(text(t(*key)).font_size(ui_font()).color(hex(ink())).semibold()),
        );

        if open {
            for i in 0..count {
                if !app.matches_search(fi, i) {
                    continue;
                }
                let label = app.sheet_name(fi, i);
                let selected = app.active.folder() == fi && app.active.index() == i;
                let related = app.is_related(fi, i);
                let frozen = app.is_frozen(fi, i);
                let renaming = app.sheet_renaming == Some((fi, i));
                list = list.child(sheet_row(
                    &label, fi, i, selected, related, frozen, renaming, &app.rename_draft,
                ));
            }
            // 文件夹底部的 "+ New …" 链接（析光的蓝色引导链接）
            let (link, msg) = new_link(lang, fi);
            // 导航器底部是「+ New …」引导链接
            let link = format!("+ {}", link);
            list = list.child(
                row()
                    .h(row_h())
                    .pl(NAV_INDENT + 6.0)
                    .items_center()
                    .hover(|s| s.bg(hex(family_highlight())))
                    .on_click(msg)
                    .child(text(link).font_size(ui_font()).color(hex(accent()))),
            );
        }
    }

    col()
        .w(nav_w())
        .h_full()
        .bg(hex(window_bg()))
        .child(
            // 顶部 Search（析光 用 Search 过滤表单名）
            row()
                .px(6.0)
                .py(5.0)
                .gap(4.0)
                .items_center()
                .child(glyph_at(Glyph::Search, 13.0))
                .child(
                    text_input(app.search.clone(), Msg::Search)
                        .placeholder(t(K::Search))
                        .w_full(),
                ),
        )
        .child(list.flex1().scroll_y().px(2.0))
}

/// 一个表单条目。
fn sheet_row(
    label: &str,
    folder: usize,
    index: usize,
    selected: bool,
    related: bool,
    frozen: bool,
    renaming: bool,
    draft: &str,
) -> Element<Msg> {
    // 就地改名：这一行变成输入框。回车提交、Esc 取消，点别处也提交
    // （与表头改名共用 RenameCommit 那条提交路径）。
    if renaming {
        return row()
            .h(row_h())
            .pl(NAV_INDENT)
            .pr(4.0)
            .items_center()
            .child(
                text_input(draft.to_string(), Msg::RenameInput)
                    .placeholder(label.to_string())
                    .autofocus()
                    .w_full()
                    .on_key_capture(|e: &KeyEvent| match e.key {
                        Key::Enter => Some(Msg::RenameCommit),
                        Key::Escape => Some(Msg::RenameCancel),
                        _ => None,
                    }),
            );
    }

    let mut t = text(label.to_string()).font_size(ui_font()).color(hex(ink()));
    if related {
        t = t.semibold();
    }
    if frozen {
        t = t.italic().color(hex(excluded()));
    }

    let mut r = row()
        .h(row_h())
        .pl(NAV_INDENT + 6.0)
        .gap(5.0)
        .items_center()
        .when(selected, |e| e.bg(hex(accent_soft())))
        .hover(|s| s.bg(hex(family_highlight())))
        .on_click(Msg::OpenSheet(folder, index))
        // 右键：打开 / 重命名 / 副本 / 冻结 / 删除。
        // 与表格里的一样挂窗口根（见 ui/context.rs 的定位说明），
        // 所以这里只负责给出 (文件夹, 索引) 与指针位置。
        .on_context_menu(move |p| Msg::SheetContextMenu(folder, index, p));

    // 选中项用一条 2px 强调色竖条标记 —— 比整行加深色块更克制、更易扫读
    if selected {
        r = r.child(div().w(2.0).h(row_h() - 4.0).bg(hex(accent())).rounded(r_sm()));
    }
    r.child(small_sheet_icon(folder)).child(t)
}

/// 表单条目左侧的小图标：数据表=网格、结果=表格、图形=柱、布局=页面。
fn small_sheet_icon(folder: usize) -> Element<Msg> {
    canvas(move |c, r| {
        let ink = match folder {
            0 => hex("#5A6473"),
            2 => hex("#7A6BA8"),
            3 => hex("#2E7BC4"),
            _ => hex("#8A8F99"),
        };
        let (x, y, w, h) = (r.origin().x, r.origin().y, r.w, r.h);
        match folder {
            0 | 2 => {
                // 网格小表
                for k in 0..3 {
                    let yy = y + h * (0.25 + 0.25 * k as f32);
                    c.line(Point::new(x, yy), Point::new(x + w, yy), 1.0, ink);
                }
                c.line(
                    Point::new(x + w * 0.45, y + h * 0.15),
                    Point::new(x + w * 0.45, y + h * 0.85),
                    1.0,
                    ink,
                );
            }
            3 => {
                // 柱状图
                c.line(
                    Point::new(x + w * 0.3, y + h * 0.85),
                    Point::new(x + w * 0.3, y + h * 0.45),
                    2.0,
                    ink,
                );
                c.line(
                    Point::new(x + w * 0.68, y + h * 0.85),
                    Point::new(x + w * 0.68, y + h * 0.22),
                    2.0,
                    ink,
                );
            }
            _ => {
                // 页面
                c.line(
                    Point::new(x + w * 0.2, y + h * 0.12),
                    Point::new(x + w * 0.2, y + h * 0.88),
                    1.0,
                    ink,
                );
                c.line(
                    Point::new(x + w * 0.2, y + h * 0.12),
                    Point::new(x + w * 0.78, y + h * 0.12),
                    1.0,
                    ink,
                );
                c.line(
                    Point::new(x + w * 0.78, y + h * 0.12),
                    Point::new(x + w * 0.78, y + h * 0.88),
                    1.0,
                    ink,
                );
                c.line(
                    Point::new(x + w * 0.2, y + h * 0.88),
                    Point::new(x + w * 0.78, y + h * 0.88),
                    1.0,
                    ink,
                );
            }
        }
    })
    .size(12.0, 12.0)
}

/// 每个文件夹底部的蓝色新建链接（文案与析光一致）。
fn new_link(lang: Lang, folder: usize) -> (&'static str, Msg) {
    let t = |k: K| i18n::t(lang, k);
    match folder {
        0 => (t(K::LinkNewDataTable), Msg::NewDataTable),
        1 => (t(K::LinkNewInfo), Msg::Tool("new_info")),
        2 => (t(K::LinkNewAnalysis), Msg::NewAnalysis),
        3 => (t(K::LinkNewGraph), Msg::NewGraph),
        _ => (t(K::NewGraphDots), Msg::NewGraph),
    }
}



#[allow(dead_code)]
fn family_link(label: &'static str, msg: Msg) -> Element<Msg> {
    row()
        .h(row_h())
        .pl(6.0)
        .items_center()
        .hover(|s| s.bg(hex(family_highlight())))
        .on_click(msg)
        .child(text(label).font_size(ui_font()).color(hex(accent())))
}
