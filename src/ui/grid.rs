//! 数据表网格：双层表头 + 行号列 + 行标题列 + 虚拟滚动 + 单元格内直接输入。
//!
//! 析光表格的视觉要点（逐条还原）：
//! - 整表纯白，**没有**斑马纹（这是与 Excel 最明显的区别）
//! - 双层表头：上层列组（Control / Treated），下层子列（A:Y1 / A:Y2）
//!   无子列的表型（Column / Contingency / Survival / PartsOfWhole / MultipleVariables）
//!   下层留空，只显示上层
//! - 第一列是行标题列（XY / Grouped / Contingency / PartsOfWhole / Nested 有内容）
//! - 网格线是 1px 极细冷灰 #BFC3CA，不是黑色
//! - 光标格：蓝色边框；鼠标悬停格：浅灰底（跟随鼠标）
//! - 排除值：蓝色斜体（析光 规范）
//!
//! # 无限行的关键
//! 虚拟列表的行内容**直接查稀疏矩阵**（`snap.get_cell(r, c)`），
//! 不预建任何「行窗口」。所以 `rows_hint` 是多少就渲染多少行，
//! 跳到第 1,000,000 行也只多一个整数，内存只随**已写入的单元格**增长。

use std::sync::Arc;

use charis_ui::prelude::*;

use crate::app::{Msg, RenameTarget};
use crate::i18n::Lang;
use crate::model::{frac_digits, CellValue, DataTable, SpectraTable};
use crate::palette::*;

/// 估算一段文本在 `data_font()` 下的显示宽度（像素）。
/// ASCII 按 0.55em、CJK 按 1em —— 与 plot.rs 的 text_w 同一套口径。
pub fn text_width(s: &str) -> f32 {
    s.chars()
        .map(|c| if c.is_ascii() { data_font() * 0.55 } else { data_font() })
        .sum::<f32>()
}

/// **列宽自适应**：某列任意一个单元格的内容超出默认列宽时，整列加宽到能完整显示。
///
/// 扫描已写入的单元格（稀疏层，只遍历真实数据，空表几乎零开销），
/// 取该列最大内容宽度；表头文字也参与比较，避免列名被截断。
fn col_widths(
    t: &DataTable,
    snap: &TableSnap,
    cols: usize,
    rows: usize,
    dp: &[Option<usize>],
) -> Vec<f32> {
    let mut w = vec![COL_W; cols.max(1)];
    // 表头：数据集名 / 子列名
    for (di, ds) in t.table.data_sets.iter().enumerate() {
        let start = t.table.dataset_start_col(di);
        let hw = text_width(&ds.name) + 26.0; // 留出字母标记与内边距
        if di < cols && hw / ds.width().max(1) as f32 > w[start.min(cols - 1)] {
            // 组名跨多个子列，平分到每列
            let per = hw / ds.width().max(1) as f32;
            for k in 0..ds.width() {
                if start + k < cols {
                    w[start + k] = w[start + k].max(per);
                }
            }
        }
        for (si, sub) in ds.sub_columns.iter().enumerate() {
            if start + si < cols {
                let sw = text_width(&sub.name) + 18.0;
                w[start + si] = w[start + si].max(sw);
            }
        }
    }
    // 单元格内容
    for r in 0..rows {
        for c in 0..cols {
            let v = snap.get_cell(r, c);
            if matches!(v, CellValue::Empty) {
                continue;
            }
            let need = text_width(&v.display_dp(dp.get(c).copied().flatten())) + 12.0;
            if need > w[c] {
                w[c] = need;
            }
        }
    }
    // 上限：防止某个超长文本把表格撑得无法使用
    for x in w.iter_mut() {
        *x = x.clamp(COL_W, 320.0);
    }
    w
}

/// **每列的小数位**：取该列所有数值里「最少需要几位小数」的最大值。
///
/// 规则（用户要求）：
/// - 列里出现 `6.3879854`（7 位），同列的 `6.3` 就补成 `6.3000000` —— 整列对齐
/// - 一列全是整数 / 没数据时返回 `None`，**不补零**（默认不添加小数位数）
/// - 与设置里的「小数点位数」**无关**：那是分析输出的精度，表格显示只跟输入走
fn col_decimals(snap: &TableSnap, cols: usize, rows: usize) -> Vec<Option<usize>> {
    let mut m = vec![0usize; cols.max(1)];
    let mut any = vec![false; cols.max(1)];
    for r in 0..rows {
        for c in 0..cols {
            if let CellValue::Number(x) = snap.get_cell(r, c) {
                let d = frac_digits(x);
                if d > m[c] {
                    m[c] = d;
                }
                any[c] = true;
            }
        }
    }
    m.iter()
        .zip(any.iter())
        .map(|(d, has)| if *has && *d > 0 { Some(*d) } else { None })
        .collect()
}

/// 只拦截导航键与 Ctrl 组合键，其余按键返回 None 交回输入框，
/// 否则普通字符会被吞掉，单元格就打不进字了。
/// **全局**键盘钩子（挂在窗口根容器上）。
///
/// 为什么需要它：网格自己也有 `on_key_capture`，但那要求网格持有焦点。
/// 用户点过检查器的输入框、或者刚切回数据表页时，焦点并不在网格上，
/// 于是 Ctrl+C / V / X / Z / Y 全部静默失效 —— 这是快捷键「时灵时不灵」的真因。
///
/// 钩子只转发 **组合键**，普通打字一律放过，不会干扰任何输入框。
/// 事件被子孙消费后不会冒泡到这里，所以不会重复触发。
pub fn global_key_hook(e: &KeyEvent) -> Option<Msg> {
    if e.mods.ctrl || e.mods.alt {
        Some(Msg::GridKey(e.clone()))
    } else {
        None
    }
}

fn grid_key_hook(e: &KeyEvent) -> Option<Msg> {
    let nav = matches!(
        e.key,
        Key::Up
            | Key::Down
            | Key::Left
            | Key::Right
            | Key::Enter
            | Key::Tab
            | Key::Escape
            | Key::PageUp
            | Key::PageDown
            | Key::Home
            | Key::End
            | Key::Delete
            | Key::Backspace
    );
    if nav || e.mods.ctrl || e.mods.alt {
        Some(Msg::GridKey(e.clone()))
    } else {
        None
    }
}

/// 当前表的只读快照。
///
/// 用 Arc 让虚拟列表闭包 O(1) 捕获；因为直接查稀疏矩阵，
/// 无需按行数预分配——这是「无限行」成立的前提。
pub type TableSnap = Arc<SpectraTable>;

pub fn snapshot(t: &DataTable) -> TableSnap {
    Arc::new(t.table.clone())
}

/// 参数 `rows` / `cols` 是**视口**的行列数：行与列都无限，
/// 这里渲染多少只影响显示窗口，光标往右 / 往下走会自动增长，
/// 写到任意一格都会把结构补齐到那一格。
pub fn data_grid(
    lang: Lang,
    t: &DataTable,
    snap: &TableSnap,
    rows: usize,
    cols: usize,
    sel: Option<(usize, usize, usize, usize)>,
    cursor: (usize, usize),
    editing: bool,
    draft: &str,
    hover: Option<(usize, usize)>,
    renaming: Option<RenameTarget>,
    rename_draft: &str,
    // 是否可以请求焦点。**延迟到帧初**才为 true（见 app.rs update 顶部「坑 2」）：
    // 渲染途中改焦点状态会被当帧布局覆盖，所以进入编辑的第一帧不抢焦点，
    // 下一帧再 autofocus。
    autofocus: bool,
) -> Element<Msg> {
    let s = snap.clone();
    let excl = t.excluded.clone();
    let has_titles = t.table.table_type.has_row_titles();
    let titles = t.table.row_titles.clone();
    let draft = draft.to_string();
    // 列是无限的：至少渲染 `cols` 列（视口），已有结构更宽就按实际的来
    let ncols = t.phys_cols().max(cols).max(1);
    // 列宽自适应：内容超出默认列宽就整列加宽
    // 小数位按「列内最长」自动对齐，与设置里的显示位数无关
    let dp = col_decimals(snap, ncols, rows);
    let widths = col_widths(t, snap, ncols, rows, &dp);
    let widths2 = std::sync::Arc::new(widths.clone());
    let dp2 = std::sync::Arc::new(dp);
    let total_w: f32 = widths.iter().sum();

    // 表格总宽：行号 +（可选）行标题 + 数据列。
    // 之前没有横向滚动容器，窗口一窄右边的列就被裁掉——这是「只显示几列」的原因。
    let total = ROWNO_W
        + if has_titles { ROWTITLE_W } else { 0.0 }
        + total_w
        + 2.0;

    // key 稳定：切换行/列时保留滚动位置与输入光标
    div()
        .key(("grid", t.name.clone()))
        .size_full()
        .bg(hex(canvas_bg()))
        // 键盘导航：focusable 让网格可获焦，on_key_capture 抢在 text_input 之前
        // 处理方向键 / Enter / Tab，实现「移动即可继续输入」
        .focusable()
        .autofocus()
        .on_key_capture(grid_key_hook)
        // 横向滚动包住表头 + 数据区，两者同步平移
        .scroll_both()
        .child(
            // `min_w(total)` 而不是 `w(total)`：列数少时内容会撑满剩余宽度
            // （表格区占满左、右面板之间的全部空间），列数多时又能横向滚动。
            col()
                .w_full()
                .min_w(total)
                .h_full()
                // 表头不随数据纵向滚动
                .child(
                    group_header(lang, t, ncols, &widths, renaming, rename_draft, sel).w_full(),
                )
                .child(sub_header(t, ncols, &widths, renaming, rename_draft, sel).w_full())
                .child(
                    virtual_list(rows, move |r| {
                        grid_row(
                            &s, r, ncols, &titles, has_titles, cursor, editing, &draft, hover, &excl,
                            &widths2, &dp2, sel, autofocus,
                        )
                    })
                    .flex1()
                    .item_height(row_h())
                    .w_full()
                    .min_w(total),
                ),
        )
}

#[allow(clippy::too_many_arguments)]
fn grid_row(
    s: &TableSnap,
    r: usize,
    ncols: usize,
    titles: &[String],
    has_titles: bool,
    cursor: (usize, usize),
    editing: bool,
    draft: &str,
    hover: Option<(usize, usize)>,
    excl: &[(usize, usize)],
    widths: &std::sync::Arc<Vec<f32>>,
    dp: &std::sync::Arc<Vec<Option<usize>>>,
    sel: Option<(usize, usize, usize, usize)>,
    autofocus: bool,
) -> Element<Msg> {
    let mut row_e = row().gap(0.0).h(row_h()).w_full();

    // 行号列
    // 该行落在选区内时整条染色 —— 大范围框选时能一眼看出选了哪些行
    let row_in_sel = match sel {
        Some((_, r0, _, r1)) => r >= r0.min(r1) && r <= r0.max(r1),
        None => false,
    };
    row_e = row_e.child(
        div()
            .w(ROWNO_W)
            .h(row_h())
            .bg(hex(if row_in_sel { cell_sel_fill() } else { header_bg() }))
            .border_r(1.0, hex(hair()))
            .border_b(1.0, hex(grid_line()))
            .child(
                row()
                    .px(4.0)
                    .items_center()
                    .child(spacer())
                    .child(text(format!("{}", r + 1)).font_size(ui_font_sm()).color(hex(dim()))),
            ),
    );

    // 行标题列（析光的第一列）。无行标题的表型**不渲染这一列**，
    // 省下 110px 给数据列，也避免留一条永远无法输入的死列。
    if has_titles {
        let rt = titles.get(r).cloned().unwrap_or_default();
        let title_cursor = cursor.0 == usize::MAX && cursor.1 == r;
        let body: Element<Msg> = if title_cursor && editing {
            text_input(draft.to_string(), Msg::CellInput)
                .key(("rt", r))
                .when(autofocus, |e| e.autofocus())
                .font_size(data_font())
                .w(ROWTITLE_W - 6.0)
                .h(row_h() - 2.0)
                // 编辑态必须放过 Delete / Backspace，否则退格被吞掉、永远删不掉字符
            .on_key_capture(move |e: &KeyEvent| {
                if editing && matches!(e.key, Key::Delete | Key::Backspace) {
                    None
                } else {
                    grid_key_hook(e)
                }
            })
        } else {
            row()
                .px(4.0)
                .items_center()
                .center()
                .child(text(rt).font_size(data_font()).color(hex(ink())))
        };
        row_e = row_e.child(
            div()
                .w(ROWTITLE_W)
                .h(row_h())
                .center()
                .bg(if title_cursor {
                    hex(cell_sel_fill())
                } else {
                    hex(header_bg())
                })
                .border_r(1.0, hex(hair()))
                .border_b(1.0, hex(grid_line()))
                .when(title_cursor, |e| e.border(1.0, hex(accent())))
                // 单击 = 选中整行；双击 = 编辑行标题（和列标题行为一致）
                .on_click(Msg::PickRow(r))
                .on_double_click(Msg::CellClick(usize::MAX, r))
                // 拖动跨行标题 = 连续选多行；右键 = 行操作菜单
                .on_drag(move |e| match e.phase {
                    DragPhase::Start => Msg::RowDragStart(r),
                    DragPhase::Move => Msg::RowDragMove(r),
                    DragPhase::End => Msg::RowDragEnd(r),
                })
                .on_context_menu(move |p| Msg::RowContextMenu(r, p))
                // 拖动经过其它行标题时靠 hover 扩展选区：
                // charis 的 DragEvent 始终发往「按下时那个元素」，
                // 不给落在哪一格，所以跨格只能由 hover 补。
                // update 里 `if !self.sel_drag { return; }` 保证非拖动时无副作用。
                .on_hover(move |inside| {
                    if inside {
                        Msg::RowDragMove(r)
                    } else {
                        Msg::None
                    }
                })
                .child(body),
        );
    }

    for c in 0..ncols {
        let cw = widths.get(c).copied().unwrap_or(COL_W);
        row_e = row_e.child(cell(
            s.get_cell(r, c),
            c,
            r,
            cursor,
            editing,
            draft,
            hover,
            excl.contains(&(c, r)),
            cw,
            dp.get(c).copied().flatten(),
            sel,
            autofocus,
        ));
    }
    row_e
}

#[allow(clippy::too_many_arguments)]
fn cell(
    v: CellValue,
    c: usize,
    r: usize,
    cursor: (usize, usize),
    editing: bool,
    draft: &str,
    hover: Option<(usize, usize)>,
    excluded: bool,
    cw: f32,
    // 该列的小数位（按列内最长自动得出）；None = 不补零
    dp: Option<usize>,
    sel: Option<(usize, usize, usize, usize)>,
    autofocus: bool,
) -> Element<Msg> {
    let is_cursor = cursor == (c, r);
    let is_hover = hover == Some((c, r)) && !is_cursor;
    let in_sel = match sel {
        Some((c0, r0, c1, r1)) => {
            let (c0, c1) = (c0.min(c1), c0.max(c1));
            let (r0, r1) = (r0.min(r1), r0.max(r1));
            c >= c0 && c <= c1 && r >= r0 && r <= r1
        }
        None => false,
    };

    let body: Element<Msg> = if is_cursor && editing {
        // 光标格：直接可输入。key 随坐标变化 → 每次移动都是新输入框，
        // autofocus 让它自动拿到焦点，因此「移动完可以立马输入」。
        text_input(draft.to_string(), Msg::CellInput)
            .key(("cell", c, r))
            .when(autofocus, |e| e.autofocus())
            .font_size(data_font())
            .w(cw - 4.0)
            .h(row_h() - 2.0)
            // 关键：输入框拿到焦点后，父容器的 on_key_capture 收不到按键，
            // 所以这里再挂一次 —— 否则编辑态下 Ctrl+C/V/X/Z/Y 全部失效。
            // 编辑态必须放过 Delete / Backspace，否则退格被吞掉、永远删不掉字符
            .on_key_capture(move |e: &KeyEvent| {
                if editing && matches!(e.key, Key::Delete | Key::Backspace) {
                    None
                } else {
                    grid_key_hook(e)
                }
            })
    } else {
        let mut t = text(v.display_dp(dp)).font_size(data_font());
        t = if excluded {
            // 注意：这里必须用完整路径。`excluded` 在本函数里是 bool 形参，
            // 会遮蔽 palette::excluded 这个函数。
            t.color(hex(crate::palette::excluded())).italic()
        } else {
            t.color(hex(ink()))
        };
        // 所有内容一律**单元格正中**（用户要求），不再按数值/文本区分左右对齐
        row().px(4.0).items_center().center().child(t)
    };

    // 选区外框：只画落在矩形边界上的那些边（规范 §4）。
    // 单格选区不画外框 —— 否则和光标格的内描边重复成双层框。
    let (sel_c0, sel_r0, sel_c1, sel_r1) = sel.unwrap_or((0, 0, 0, 0));
    let multi = sel.map(|(a, b, c2, d)| c2 != a || d != b).unwrap_or(false);
    let on_left = in_sel && multi && c == sel_c0.min(sel_c1);
    let on_right = in_sel && multi && c == sel_c0.max(sel_c1);
    let on_top = in_sel && multi && r == sel_r0.min(sel_r1);
    let on_bottom = in_sel && multi && r == sel_r0.max(sel_r1);

    div()
        .w(cw)
        .h(row_h())
        .center()
        .bg(if is_hover {
            hex(cell_hover())
        } else if in_sel {
            hex(cell_sel_fill())
        } else {
            hex(canvas_bg())
        })
        .border_r(1.0, hex(grid_line()))
        .border_b(1.0, hex(grid_line()))
        .when(on_left, |e| e.border_l(1.6, hex(accent())))
        .when(on_right, |e| e.border_r(1.6, hex(accent())))
        .when(on_top, |e| e.border_t(1.6, hex(accent())))
        .when(on_bottom, |e| e.border_b(1.6, hex(accent())))
        // 光标格：蓝色边框（析光的当前单元格标记）
        .when(is_cursor, |e| e.border(1.0, hex(accent())))
        // 跟随鼠标的高亮格
        .on_hover(move |inside| {
            Msg::HoverCell(if inside { Some((c, r)) } else { None })
        })
        // ── 真正的「按住拖动框选」──────────────────────────────────────
        // charis 的 on_drag 会依次给出 Start / Move / End 三个阶段，
        // 且事件始终发往「按下时那个元素」。所以：
        //   Start → 以本格为锚点，进入框选
        //   Move  → 交给 on_hover 扩展（拖动经过的格子会触发 hover，
        //           而 DragEvent 只给 delta、不给落在哪一格）
        //   End   → 松开，结束框选
        // 这样不再需要「先按 Ctrl+B 进入框选模式」这种权宜做法。
        .on_drag(move |e| match e.phase {
            DragPhase::Start => Msg::SelDragStart(c, r),
            DragPhase::Move => Msg::SelDragMove(e.delta),
            DragPhase::End => Msg::SelDragEnd,
        })
        // 右键：在指针处弹出上下文菜单（复制/粘贴/剪切/插入删除行列…）
        .on_context_menu(move |p| Msg::CellContextMenu(c, r, p))
        .on_click(Msg::CellClick(c, r))
        .child(body)
}

/// 一列一列的「(起始物理列, 跨越列数)」清单 —— 上层组名与下层子列名
/// **共用同一份**，两边宽度必然对齐（之前各算各的，出现过大块左偏）。
struct Span {
    start: usize,
    width: usize,
    /// 数据集索引；虚拟列为 None
    ds: Option<usize>,
    /// 上层显示名（虚拟列用推算出的字母）
    name: String,
    /// 该 span 的总像素宽度
    px: f32,
}

fn spans(t: &DataTable, widths: &[f32], cols: usize) -> Vec<Span> {
    let mut out = Vec::new();
    let mut phys = 0usize;
    for (di, ds) in t.table.data_sets.iter().enumerate() {
        if phys >= cols {
            break;
        }
        // 关键：宽度裁到剩余列数。
        // 不裁的话最后一个 span 会越过 cols，表头就比数据行多出几列，
        // 表现成「表头对齐了，但网格线和列错位」。
        let w = ds.width().max(1).min(cols - phys);
        let px: f32 = (0..w).map(|k| widths.get(phys + k).copied().unwrap_or(COL_W)).sum();
        out.push(Span {
            start: phys,
            width: w,
            ds: Some(di),
            name: ds.name.clone(),
            px,
        });
        phys += w;
    }
    // 虚拟列：右边继续按字母排下去（Z 之后是 AA、AB…）
    while phys < cols {
        let (name, _) = t.table.col_label(phys);
        let w = t.table.default_sub_width().max(1).min(cols - phys);
        let px: f32 = (0..w).map(|k| widths.get(phys + k).copied().unwrap_or(COL_W)).sum();
        out.push(Span { start: phys, width: w, ds: None, name, px });
        phys += w;
    }
    out
}

/// 列区间 [a, b] 是否与选区有交集（表头整条染色的判据）。
fn col_in_sel(sel: Option<(usize, usize, usize, usize)>, a: usize, b: usize) -> bool {
    match sel {
        Some((c0, _, c1, _)) => {
            if c0 == usize::MAX || c1 == usize::MAX {
                return false; // 行标题列没有对应的表头块
            }
            a.max(c0.min(c1)) <= b.min(c0.max(c1))
        }
        None => false,
    }
}

fn group_header(
    _lang: Lang,
    t: &DataTable,
    _cols: usize,
    widths: &Vec<f32>,
    renaming: Option<RenameTarget>,
    draft: &str,
    sel: Option<(usize, usize, usize, usize)>,
) -> Element<Msg> {
    let mut r = row().h(HEADER_ROW_H).gap(0.0);
    r = r.child(header_corner(ROWNO_W));
    if t.table.table_type.has_row_titles() {
        r = r.child(header_corner(ROWTITLE_W));
    }
    for sp in spans(t, widths, _cols) {
        let editing = match (sp.ds, renaming) {
            (Some(di), Some(RenameTarget::DataSet(x))) => di == x,
            _ => false,
        };
        let body: Element<Msg> = if editing {
            // 双击后原地编辑列名
            text_input(draft.to_string(), Msg::RenameInput)
                .key(("ds", sp.start))
                .autofocus()
                .font_size(ui_font())
                .w(sp.px - 8.0)
                .h(HEADER_ROW_H - 4.0)
                .on_key_capture(move |e: &KeyEvent| match e.key {
                    Key::Enter => Some(Msg::RenameCommit),
                    Key::Escape => Some(Msg::RenameCancel),
                    _ => None,
                })
        } else {
            row()
                .px(4.0)
                .items_center()
                .child(text(sp.name.clone()).font_size(ui_font()).color(hex(ink())).nowrap())
                .child(spacer())
                .child(
                    text(sp.ds.map(|d| t.table.dataset_letter(d)).unwrap_or_default())
                        .font_size(ui_font_sm())
                        .color(hex(dim())),
                )
        };
        r = r.child(
            div()
                .w(sp.px)
                .h(HEADER_ROW_H)
                .bg(hex(if col_in_sel(sel, sp.start, sp.start + sp.width.saturating_sub(1)) {
                    cell_sel_fill()
                } else {
                    header_bg()
                }))
                .border_r(1.0, hex(hair()))
                .border_b(1.0, hex(hair()))
                // 单击 = 选中整列；双击 = 改名（大块）
                .on_click(Msg::PickCols(sp.start, sp.start + sp.width.saturating_sub(1)))
                // 拖动跨列标题 = 连续选多列；右键 = 列操作菜单
                .on_drag(move |e| match e.phase {
                    DragPhase::Start => Msg::ColDragStart(sp.start),
                    DragPhase::Move => Msg::ColDragMove(sp.start + sp.width.saturating_sub(1)),
                    DragPhase::End => Msg::ColDragEnd(sp.start),
                })
                .on_context_menu(move |p| {
                    Msg::ColContextMenu(sp.start + sp.width.saturating_sub(1), p)
                })
                // 同上：跨列标题拖动靠 hover 扩展
                .on_hover(move |inside| {
                    if inside {
                        Msg::ColDragMove(sp.start + sp.width.saturating_sub(1))
                    } else {
                        Msg::None
                    }
                })
                .on_double_click(match sp.ds {
                    Some(d) => Msg::RenameStart(RenameTarget::DataSet(d)),
                    None => Msg::None,
                })
                .child(body),
        );
    }
    r.child(spacer())
}

fn header_corner(w: f32) -> Element<Msg> {
    div()
        .w(w)
        .h(HEADER_ROW_H)
        .bg(hex(header_bg()))
        .border_r(1.0, hex(hair()))
        .border_b(1.0, hex(hair()))
}

/// 下层表头：子列名（A:Y1 / A:Y2 …）。宽度与上层**共用 spans**，保证左右对齐。
///
/// 小块（子列名）同样支持双击改名 —— 大块能改、小块不能改是不讲道理的。
fn sub_header(
    t: &DataTable,
    _cols: usize,
    widths: &Vec<f32>,
    renaming: Option<RenameTarget>,
    rename_draft: &str,
    sel: Option<(usize, usize, usize, usize)>,
) -> Element<Msg> {
    let mut r = row().h(HEADER_ROW_H).gap(0.0);
    r = r.child(header_corner(ROWNO_W));
    if t.table.table_type.has_row_titles() {
        r = r.child(header_corner(ROWTITLE_W));
    }
    for sp in spans(t, widths, _cols) {
        let letter = sp.ds.map(|d| t.table.dataset_letter(d)).unwrap_or_default();
        for k in 0..sp.width {
            let cw = widths.get(sp.start + k).copied().unwrap_or(COL_W);
            let label = match sp.ds {
                Some(d) => t
                    .table
                    .data_sets
                    .get(d)
                    .and_then(|ds| ds.sub_columns.get(k))
                    .map(|sub| format!("{}:{}", letter, sub.name))
                    .unwrap_or_default(),
                None => String::new(),
            };
            let editing = match (sp.ds, renaming) {
                (Some(di), Some(RenameTarget::Sub(d2, s2))) => di == d2 && k == s2,
                _ => false,
            };
            let body: Element<Msg> = if editing {
                text_input(rename_draft.to_string(), Msg::RenameInput)
                    .key(("sub", sp.start, k))
                    .autofocus()
                    .font_size(ui_font_sm())
                    .w(cw - 6.0)
                    .h(HEADER_ROW_H - 4.0)
                    .on_key_capture(|e: &KeyEvent| match e.key {
                        Key::Enter => Some(Msg::RenameCommit),
                        Key::Escape => Some(Msg::RenameCancel),
                        _ => None,
                    })
            } else {
                row()
                    .px(4.0)
                    .items_center()
                    .child(text(label).font_size(ui_font_sm()).color(hex(dim())))
            };
            r = r.child(
                div()
                    .w(cw)
                    .h(HEADER_ROW_H)
                    .bg(hex(if col_in_sel(sel, sp.start + k, sp.start + k) {
                        cell_sel_fill()
                    } else {
                        header_bg()
                    }))
                    .border_r(1.0, hex(hair()))
                    .border_b(1.0, hex(hair()))
                    // 单击 = 选中这一列；双击 = 改名（小块）
                    .on_click(Msg::PickCols(sp.start + k, sp.start + k))
                    .on_double_click(match sp.ds {
                        Some(d) => Msg::RenameStart(RenameTarget::Sub(d, k)),
                        None => Msg::None,
                    })
                    .child(body),
            );
        }
    }
    r.child(spacer())
}

