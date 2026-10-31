//! 析光视觉常量：冷灰 UI 色板 + 尺寸标尺 + 默认数据集配色。
//!
//! 说明：GraphPad 官方从未发布 UI 设计规范或色板 HEX。本文件中的颜色来自对
//! 析光目标截图与教程截图的取色比对（见根目录 README 的「取色来源」），
//! 尺寸为按 96 dpi（1pt = 4/3 px）换算的估算值。用于「视觉近似复刻」。
//!
//! 若你手上有目标版本的真实截图，直接改这里的常量即可全站生效——所有 UI
//! 都从本文件取色，没有硬编码颜色。

// ── 冷灰 UI 色板（克制 / 现代版）────────────────────────────────────────────
// 保留了原有的辨识度（冷灰、带轻微蓝调），但整体提亮、降饱和：
// - 分隔线更浅 → 用**留白**而非线条划分区域，这是「克制」的关键
// - 强调色收敛到一个蓝，界面里不再出现第二种高饱和色
// - 圆角与投影统一走本文件的 token，保证全站一致
// ── 深色色板 ────────────────────────────────────────────────────────────────
// 与上面的浅色值**一一对应**：同名项在深色模式下取这里的 `_D` 值。
// 取色原则：不用纯黑（#000）做背景，用带蓝调的深灰，避免 OLED 上的
// 「黑块感」与边缘色带；文字用近白而非纯白，减轻长时间阅读的眩光。
//
// 例外：**图表画布不跟着变深**。科研图的底色必须是白（出版规范），
// 深色模式只改 UI 外壳，不改图表本身 —— plot.rs 里画布填的是 WHITE。
pub const WINDOW_BG_D: &str = "#171A21";
pub const PANEL_BG_D: &str = "#1F242D";
pub const TOOLBAR_BG_D: &str = "#1C212A";
pub const DIVIDER_D: &str = "#333A46";
pub const GRID_LINE_D: &str = "#262C36";
pub const HEADER_BG_D: &str = "#232935";
pub const TEXT_D: &str = "#E4E9F0";
pub const TEXT_DIM_D: &str = "#98A2B3";
pub const SEL_HIGHLIGHT_D: &str = "#28374D";
pub const FAMILY_HIGHLIGHT_D: &str = "#2E3E55";
pub const CELL_SEL_FILL_D: &str = "#2C4A6E";
pub const EXCLUDED_D: &str = "#6FA8F5";
pub const CANVAS_D: &str = "#1E232C";
pub const TOOL_HOVER_D: &str = "#2C333E";
pub const CELL_HOVER_D: &str = "#272D38";
pub const TOOL_PRESSED_D: &str = "#38414F";
// 深色界面上投影要用**纯黑**：浅灰投影在深底上根本看不见，
// 浮层会「贴」在背景里分不出层次。
pub const SHADOW_CARD_D: &str = "#000000";
pub const SHADOW_POP_D: &str = "#000000";
#[allow(dead_code)]
pub const SCRIM_D: &str = "#0B0E13";

// ── 深色开关 ────────────────────────────────────────────────────────────────
// 由 `app.rs` 每帧刷新一次（它已经算过 `settings.dark()`）。
//
// 这里刻意**不再**去读 `settings`：`palette` 的函数每帧会被调用上百次
// （每个单元格、每一行都要取色），每次都加一次锁会明显拖慢界面。
// 缓存成一个原子布尔，读起来是零成本的。
static DARK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_dark(v: bool) {
    DARK.store(v, std::sync::atomic::Ordering::Relaxed);
}

/// 当前是否为深色模式（浅色 / 深色 / 跟随系统 解析后的最终结果）。
pub fn dark() -> bool {
    DARK.load(std::sync::atomic::Ordering::Relaxed)
}

/// 按当前模式选值：深色取 `d`，浅色取 `l`。
fn pick(l: &'static str, d: &'static str) -> &'static str {
    if dark() { d } else { l }
}

pub const WINDOW_BG: &str = "#F2F4F7"; // 窗口背景 / 导航器 / 菜单栏
pub const PANEL_BG: &str = "#F8F9FB"; // 面板内空白（导航器留白、检查器）
pub const TOOLBAR_BG: &str = "#F6F7F9"; // 主工具栏 / 底部工具栏
pub const DIVIDER: &str = "#E1E5EA"; // 面板间、工具栏组间分隔线（1px）
pub const GRID_LINE: &str = "#EBEEF2"; // 数据表网格线（极细 1px）
pub const HEADER_BG: &str = "#F1F3F6"; // 表头 / 行号列底色
pub const TEXT: &str = "#111827"; // 主文本（近黑，带蓝调，不刺眼）
pub const TEXT_DIM: &str = "#6E7885"; // 次要文本（说明、占位符）
pub const SEL_HIGHLIGHT: &str = "#E4EEFB"; // 导航器：当前激活表单
pub const FAMILY_HIGHLIGHT: &str = "#DAE7F8"; // 导航器：关联表单悬停/粗体
pub const CELL_SEL_FILL: &str = "#D3E5FB"; // 表格：选中单元格填充
#[allow(dead_code)]
pub const ACCENT: &str = "#2B6FD4"; // 品牌蓝：Create 按钮、+New 链接、选中边框
#[allow(dead_code)]
pub const ACCENT_SOFT: &str = "#E9F1FD"; // 强调色浅底（激活标签、卡片底）
pub const EXCLUDED: &str = "#3B7DD8"; // 排除值（析光用蓝色斜体）
pub const CANVAS: &str = "#FFFFFF"; // 图表画布 / 单元格白底
pub const TOOL_HOVER: &str = "#E6EDF6"; // 工具栏按钮悬停
pub const CELL_HOVER: &str = "#F0F4F9"; // 单元格：跟随鼠标的高亮格
pub const TOOL_PRESSED: &str = "#D5E2F4"; // 工具栏按钮按下

// ── 投影（克制的浅阴影，只用于浮层与卡片）──────────────────────────────────
// 说明：不用黑色——浅冷灰投影在浅色界面上更「高级」且不会脏。
pub const SHADOW_CARD: &str = "#B7BECB"; // 卡片 / 面板：1px 位移 + 3px 模糊
pub const SHADOW_POP: &str = "#98A1B0"; // 对话框：10px 位移 + 28px 模糊
/// 模态遮罩。**刻意不透明**：弹窗底下的样例数据必须被完全盖住，
/// 否则会出现「新建表格的界面」和「表格选择对话框」两层叠在一起的观感。
/// 半透明需要 alpha 通道，这里不敢赌 charis 的 hex() 支持 8 位，
/// 所以直接用实心灰 —— 覆盖是确定的，视觉上也仍读作「压暗的底层」。
#[allow(dead_code)]
pub const SCRIM: &str = "#7C8695";

// ── 圆角 token ──────────────────────────────────────────────────────────────
// 同心圆角：嵌套时**外层半径 = 内层半径 + 间距**，两角才是同一圆心的同心弧，
// 否则视觉上会出现「外圆内尖」或「外尖内圆」的错位感。
// 间距统一取 3px，于是 R_MD = R_SM + 3、R_LG = R_MD + 4。
pub const R_SM: f32 = 3.0; // 内层：色块、小标签、嵌套控件
pub const R_MD: f32 = 6.0; // 中层：按钮、输入框、卡片（内含 R_SM）
pub const R_LG: f32 = 10.0; // 外层：对话框、大面板（内含 R_MD）
/// 嵌套一层的同心外圆角：内层 R_SM(3) + 间距 3 = 6
#[allow(dead_code)]
pub const R_INSET: f32 = 6.0;

// ── 尺寸标尺（逻辑像素；9pt = 12px, 10pt = 13.33px @96dpi）────────────────────
pub const MENU_H: f32 = 24.0; // 菜单栏
#[allow(dead_code)]
pub const TOOLBAR_H: f32 = 44.0; // 主工具栏参考高（ribbon 组：24 图标 + 组名 + 内边距）
#[allow(dead_code)]
pub const NAV_W: f32 = 240.0; // 导航器宽度（默认 210–240）
pub const INSPECTOR_W: f32 = 430.0; // 右侧控制面板（绘图配置的灵魂，分区 + 滚动）
                                   // 320 时「标签 + 控件」行会被挤掉右侧，384 才能完整显示
pub const BOTTOM_BAR_H: f32 = 28.0; // 底部工具栏
#[allow(dead_code)]
pub const STATUS_H: f32 = 22.0; // 状态栏
pub const ROW_H: f32 = 26.0; // 表格行高（原 18 偏低，内容挤在一起）
pub const HEADER_ROW_H: f32 = 24.0; // 双层表头每层高
pub const ROWNO_W: f32 = 40.0; // 行号列宽
pub const ROWTITLE_W: f32 = 110.0; // 行标题列宽（析光的第一列）
pub const COL_W: f32 = 76.0; // 默认列宽
pub const NAV_INDENT: f32 = 16.0; // 导航器表单条目缩进
#[allow(dead_code)]
pub const GROUP_GAP: f32 = 6.0; // 工具栏组分隔线两侧留白

// 字号：收敛为一个 3 级标尺（正文 / 次要 / 强调），避免字号泛滥
// ── 字号：全站只有 3 档，且都不小于 12px ──────────────────────────────────
// 层次靠**字重 + 颜色 + 间距**表达，而不是再切一档更小的字。
pub const UI_FONT: f32 = 12.0; // 正文：菜单、导航器、按钮、状态栏
pub const UI_FONT_SM: f32 = 12.0; // 次要：与正文同号，靠 TEXT_DIM 颜色降权
pub const UI_FONT_XS: f32 = 12.0; // 极次要：同号 + 更淡的颜色
pub const UI_FONT_LG: f32 = 15.0; // 标题：窗口标题、对话框主标题
pub const DATA_FONT: f32 = 13.0; // 数据表正文
pub const CHART_FONT: f32 = 12.0; // 图表文字（析光默认 Arial 8–12pt）

// ── 运行时取值（跟随设置）───────────────────────────────────────────────────
// 上面的常量是「设计默认值」。下面这组函数读 `settings` 的实时值并返回
// 实际该用的值 —— UI 全部改调函数，于是「字号、圆角、强调色、高对比」
// 这几项设置能立刻生效，而不必重启。
//
// 全部返回 `&'static str`（取自上面的常量表），所以不存在生命周期问题。

/// 强调色（设置里可切换 8 种）。
pub fn accent() -> &'static str {
    let i = crate::settings::with(|s| s.accent).min(ACCENTS.len() - 1);
    ACCENTS.get(i).copied().unwrap_or("#2B6FD4")
}

/// 强调色的浅底（与 `accent()` 配对，必须一起换，否则会出现「蓝框浅绿底」）。
pub fn accent_soft() -> &'static str {
    let i = crate::settings::with(|s| s.accent).min(ACCENT_SOFTS.len() - 1);
    // 必须区分深浅：浅底上是淡色块，深底上必须是暗色块。
    // 之前这里只有一组浅色，深色模式下「选中/高亮」就是一片白。
    let t = if dark() { ACCENT_SOFTS_D } else { ACCENT_SOFTS };
    t.get(i).copied().unwrap_or(if dark() { "#243044" } else { "#E9F1FD" })
}

const ACCENTS: [&str; 8] = [
    "#2B6FD4", "#0F766E", "#7C3AED", "#B91C1C", "#C2410C", "#15803D", "#8E24AA", "#334155",
];
const ACCENT_SOFTS: [&str; 8] = [
    "#E9F1FD", "#E3F5F2", "#F1E9FE", "#FBE9E9", "#FDECE0", "#E5F4EA", "#F7E9FB", "#E8EBEF",
];

/// 深色下的强调浅色（选中项 / 高亮块的底色），与 `ACCENT_SOFTS` 一一对应。
const ACCENT_SOFTS_D: [&str; 8] = [
    "#243044", "#1E3B38", "#332A47", "#452A2C", "#453024", "#1F3A29", "#3A2745", "#2A2F38",
];

/// UI 缩放：字号、行高、间距都乘它。
pub fn ui_scale() -> f32 {
    crate::settings::with(|s| s.font_scale.clamp(0.85, 1.60))
}

/// 正文 / 次要 / 极次要 / 标题 / 数据 字号，全部跟随 UI 缩放。
pub fn ui_font() -> f32 {
    UI_FONT * ui_scale()
}
pub fn ui_font_sm() -> f32 {
    UI_FONT_SM * ui_scale()
}
pub fn ui_font_xs() -> f32 {
    UI_FONT_XS * ui_scale()
}
pub fn ui_font_lg() -> f32 {
    UI_FONT_LG * ui_scale()
}
pub fn data_font() -> f32 {
    DATA_FONT * ui_scale()
}

/// 圆角：设置里给的是倍率，三个 token 等比缩放，保持同心关系。
pub fn radius_scale() -> f32 {
    crate::settings::with(|s| s.radius.clamp(0.0, 2.0))
}
pub fn r_sm() -> f32 {
    R_SM * radius_scale()
}
pub fn r_md() -> f32 {
    R_MD * radius_scale()
}
pub fn r_lg() -> f32 {
    R_LG * radius_scale()
}
#[allow(dead_code)]
pub fn r_inset() -> f32 {
    R_INSET * radius_scale()
}

// ── 随深浅模式切换的取色函数 ────────────────────────────────────────────────
// 每个都对应上面一对常量（浅色原常量 + `_D`）。UI 一律调函数，
// 于是「外观 → 主题」切换后整站立刻变色，不必重启。
pub fn window_bg() -> &'static str {
    pick(WINDOW_BG, WINDOW_BG_D)
}
pub fn panel_bg() -> &'static str {
    pick(PANEL_BG, PANEL_BG_D)
}
pub fn toolbar_bg() -> &'static str {
    pick(TOOLBAR_BG, TOOLBAR_BG_D)
}
pub fn grid_line() -> &'static str {
    pick(GRID_LINE, GRID_LINE_D)
}
pub fn header_bg() -> &'static str {
    pick(HEADER_BG, HEADER_BG_D)
}
/// 结果表的斑马纹底色。比画布底略深一档，用于隔行区分，
/// 深色下同理（比深底略亮），否则条纹会消失或反过来刺眼。
pub fn zebra_bg() -> &'static str {
    pick("#F7F8FA", "#252830")
}
pub fn sel_highlight() -> &'static str {
    pick(SEL_HIGHLIGHT, SEL_HIGHLIGHT_D)
}
pub fn family_highlight() -> &'static str {
    pick(FAMILY_HIGHLIGHT, FAMILY_HIGHLIGHT_D)
}
pub fn cell_sel_fill() -> &'static str {
    pick(CELL_SEL_FILL, CELL_SEL_FILL_D)
}
pub fn excluded() -> &'static str {
    pick(EXCLUDED, EXCLUDED_D)
}
/// 危险操作的 hover 底色（删除类按钮）。深色下必须换成暗红，
/// 否则浅红块会在深底上刺眼。
pub fn danger_soft() -> &'static str {
    pick("#F3D6D6", "#4A2B2E")
}

/// 提示性文字（琥珀）。深色下换成亮琥珀，深琥珀在深底上对比度不足。
pub fn warn_text() -> &'static str {
    pick("#B45309", "#F0B429")
}

/// 关闭/删除按钮的 hover 底色。深色下换暗红。
pub fn danger() -> &'static str {
    pick("#E06C75", "#8C4A4F")
}

pub fn canvas_bg() -> &'static str {
    pick(CANVAS, CANVAS_D)
}
pub fn tool_hover() -> &'static str {
    pick(TOOL_HOVER, TOOL_HOVER_D)
}
pub fn cell_hover() -> &'static str {
    pick(CELL_HOVER, CELL_HOVER_D)
}
pub fn tool_pressed() -> &'static str {
    pick(TOOL_PRESSED, TOOL_PRESSED_D)
}
pub fn shadow_card_color() -> &'static str {
    pick(SHADOW_CARD, SHADOW_CARD_D)
}
pub fn shadow_pop_color() -> &'static str {
    pick(SHADOW_POP, SHADOW_POP_D)
}

/// 主文本色。高对比下压到纯黑。
pub fn ink() -> &'static str {
    let base = pick(TEXT, TEXT_D);
    if crate::settings::with(|s| s.high_contrast) {
        // 高对比：深色界面上压到纯白，浅色界面上压到纯黑
        if dark() { "#FFFFFF" } else { "#000000" }
    } else {
        base
    }
}
/// 次要文本色。高对比下加深（次要信息反而要在高对比下更清楚）。
pub fn dim() -> &'static str {
    let base = pick(TEXT_DIM, TEXT_DIM_D);
    if crate::settings::with(|s| s.high_contrast) {
        if dark() { "#C8D0DC" } else { "#3C444F" }
    } else {
        base
    }
}
/// 分隔线。高对比下加深，否则线条会消失在背景里。
pub fn hair() -> &'static str {
    let base = pick(DIVIDER, DIVIDER_D);
    if crate::settings::with(|s| s.high_contrast) {
        // 深色界面上分隔线要**更亮**才看得见，浅色界面上则是更深
        if dark() { "#7A8492" } else { "#9AA2AE" }
    } else {
        base
    }
}

// ── 图表专用色（**不随深浅模式变化**）──────────────────────────────────────
// 科研图的底色恒为白（出版规范：黑白印刷、期刊投稿都要求白底），
// 所以图表里的文字、轴线、网格必须恒为深色。
// 若沿用 UI 的 `ink()`，深色模式下主文本会变成近白 —— 白底上画白字就全瞎了。
pub const CHART_INK: &str = "#1A1A1A"; // 图表主文本 / 刻度标签
#[allow(dead_code)]
pub const CHART_AXIS: &str = "#3A3F48"; // 轴线 / 边框
#[allow(dead_code)]
pub const CHART_GRID: &str = "#D6DAE0"; // 网格线

pub fn chart_ink() -> &'static str {
    CHART_INK
}
#[allow(dead_code)]
pub fn chart_axis() -> &'static str {
    CHART_AXIS
}
#[allow(dead_code)]
pub fn chart_grid() -> &'static str {
    CHART_GRID
}

/// 导航器宽度（设置里可调）。
pub fn nav_w() -> f32 {
    crate::settings::with(|s| s.nav_width.clamp(150.0, 420.0))
}

/// 表格行高，跟随 UI 缩放一起长。
pub fn row_h() -> f32 {
    ROW_H * ui_scale()
}

// ── 经典默认配色 ──────────────────────────────────────────────────────
// 仅用于欢迎对话框的预览缩略图等「示意」场合。
// 实际的柱体 / 散点 / 误差棒配色走 `colormap::COLORMAPS`（26 套内置方案），
// 内部色与边框色的差异由 `colormap::shade()` + 85% alpha 计算，见 colormap.rs。
#[allow(dead_code)]
pub const DS_FILL: [&str; 8] = [
    "#1A1A1A", "#009E73", "#948FD4", "#5C4F9D", "#E69F00", "#56B4E9", "#D55E00", "#CC79A7",
];

// ── 图表几何（析光默认：Plain Frame + 向内刻度 + 0.5pt 轴线）────────────────
pub const CHART_MARGIN_L: f32 = 62.0; // 含 Y 轴标题与刻度标签
pub const CHART_MARGIN_R: f32 = 18.0;
pub const CHART_MARGIN_T: f32 = 16.0;
pub const CHART_MARGIN_B: f32 = 44.0; // 含 X 轴标题与刻度标签
pub const CHART_TICK_LEN: f32 = 3.0; // 析光默认 Short
pub const CHART_AXIS_PX: i32 = 1; // 0.5pt ≈ 0.67px，取 1px 保证可见
pub const CHART_BAR_WIDTH_RATIO: f32 = 0.55; // 柱宽 / 每组可用宽度
pub const CHART_ERR_CAP_RATIO: f32 = 0.30; // 误差棒端帽 / 柱宽
// 柱体边框（内描边，与柱体像素范围完全重合）；误差棒 1pt；散点半径
pub const CHART_BAR_BORDER_PX: u32 = 2;
pub const CHART_ERROR_PX: u32 = 2;
pub const CHART_DOT_R: i32 = 2;
