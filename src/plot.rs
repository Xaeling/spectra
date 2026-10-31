//! 图表绘制：同一套绘图逻辑，位图预览 / SVG 矢量导出双端复用。
//!
//! # 为什么不用 ChartBuilder / ChartContext
//! 析光的默认外观是「Plain Frame + 向内刻度 + 0.5pt 轴线 + 组间留白较大的柱」，
//! plotters 自带 mesh 是向外刻度。这里自己算像素坐标、用 plotters 图元直接画，
//! 才能逐像素对齐目标，同时避开 ChartContext 的复杂泛型。
//!
//! # 双端复用
//! 核心是 `draw_chart<DB: DrawingBackend>()`：
//! - 预览：`BitMapBackend::with_buffer` → `Image::from_rgba`（charis 侧）
//! - 导出：`SVGBackend::new(path, …)` → 真矢量，文字为文本对象
//! 两者共用 100% 绘图代码，只换后端。
//!
//! # 科研级规范
//! - 柱体：内部 Alpha=0.85（混白底后自然变浅）；边框 = HSL 压暗（明度 ×0.40、饱和 ×3.0），
//!   线宽 1.5–2.0pt。见 `colormap::shade()`。
//! - 散点：**双层绘制** —— 底层大圆用 shade() 的深色，上层小圆用填充色，形成清晰描边环。
//! - 误差棒：线条与 T 型端帽**严格跟随柱边框色**，不是填充色。
//! - 抖动：黄金比确定性抖动，避免同一柱上的点完全重合。
//! - 显著性括号：起点基于**所有数据最高点**动态上移，多层逐层累加，绝不压住柱子/误差棒。
//! - Y 轴标题：真 90° 逆时针旋转（自下而上），轴心居中。
//! - 单位：括号式 `名称 (单位)` / 斜杠式 `名称 / 单位`；物理量符号斜体、单位正体。

use charis_ui::image::Image;
use plotters::backend::SVGBackend;
use plotters::coord::Shift;
use plotters::prelude::*;
use plotters::drawing::DrawingAreaErrorKind;
use plotters::style::{FontDesc, FontStyle};

use crate::colormap::{self, Rgba};
use crate::palette::*;

// ── 数据模型 ────────────────────────────────────────────────────────────────

// 62 种图型与渲染族分派体系（前 10 个判别值与旧版一致，老工程文件不失效）。
pub use crate::chartkind::{ChartKind, Family};

/// 轴标题的单位排版方式（对应文档 §3.2 的 `unit_format`）。
#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum UnitStyle {
    /// 括号式：`量名称 (单位)` —— IUPAC / ISO 80000 / Nature 默认
    Paren,
    /// 斜杠式：`量名称 / 单位`，仅在有量符号时推荐
    Slash,
    /// 逗号式：`量名称, 单位`（旧式）
    Comma,
    /// 不显示单位
    None,
}

impl UnitStyle {
    pub fn to_i32(self) -> i32 {
        match self {
            UnitStyle::Paren => UNIT_FMT_PAREN,
            UnitStyle::Slash => UNIT_FMT_SLASH,
            UnitStyle::Comma => UNIT_FMT_COMMA,
            UnitStyle::None => UNIT_FMT_NONE,
        }
    }
    pub fn from_i32(v: i32) -> Self {
        match v {
            UNIT_FMT_SLASH => UnitStyle::Slash,
            UNIT_FMT_COMMA => UnitStyle::Comma,
            UNIT_FMT_NONE => UnitStyle::None,
            _ => UnitStyle::Paren,
        }
    }
    /// 旧接口：半角括号式。
    #[allow(dead_code)]
    pub fn join(self, unit: &str) -> String {
        self.join_with(unit, BRACKET_HALF)
    }
    /// 拼出「单位那一段」（不含量名称）。
    ///
    /// **全角括号不补空格** —— 全角括号自带间距，再加半角空格会出现
    /// 一个难看的空隙（`浓度 （mmol/L）`）。
    pub fn join_with(self, unit: &str, bracket: i32) -> String {
        let u = unit.trim();
        if u.is_empty() {
            return String::new();
        }
        match self {
            UnitStyle::None => String::new(),
            UnitStyle::Paren => {
                if bracket == BRACKET_FULL {
                    format!("（{}）", u)
                } else {
                    format!(" ({})", u)
                }
            }
            UnitStyle::Slash => {
                // 含斜杠的单位（mol/L）再套斜杠会读成两个除号，加括号
                if u.contains('/') {
                    format!(" / ({})", u)
                } else {
                    format!(" / {}", u)
                }
            }
            UnitStyle::Comma => format!(", {}", u),
        }
    }
}

/// 单位格式常量（与文档 §3.2 对齐）。
pub const UNIT_FMT_PAREN: i32 = 0;
pub const UNIT_FMT_SLASH: i32 = 1;
pub const UNIT_FMT_COMMA: i32 = 2;
pub const UNIT_FMT_NONE: i32 = 3;
/// 半角括号 `()`
#[allow(dead_code)]
pub const BRACKET_HALF: i32 = 0;
/// 全角括号 `（）`（GB 3100）
pub const BRACKET_FULL: i32 = 1;

/// 旧接口：`名称 (单位)`。
#[allow(dead_code)]
pub fn with_unit(title: &str, unit: &str) -> String {
    let t = title.trim();
    let u = unit.trim().trim_start_matches('(').trim_end_matches(')');
    if t.is_empty() {
        if u.is_empty() {
            String::new()
        } else {
            format!("({})", u)
        }
    } else if u.is_empty() {
        t.to_string()
    } else {
        format!("{} ({})", t, u)
    }
}

/// 正式的轴标题拼装（绘图区 / 位图导出 / matplotlib 三条路径共用）。
///
/// 验收（文档 §3.2）：
/// `浓度//mmol/L/括号式/半角` → `浓度 (mmol/L)`
/// `浓度//mmol/L/括号式/全角` → `浓度（mmol/L）`（不补空格）
/// `浓度/c/mol/L/斜杠式`      → `浓度 c / (mol/L)`
/// `浓度/c/mol/L/逗号式`      → `浓度 c, mol/L`
/// `浓度//mmol/L/不显示`      → `浓度`
pub fn axis_title(
    name: &str,
    symbol: &str,
    unit: &str,
    unit_format: i32,
    bracket_style: i32,
) -> String {
    let name = name.trim();
    let sym = symbol.trim();
    let u = unit.trim();
    let fmt = UnitStyle::from_i32(unit_format);
    let mut head = String::new();
    if !name.is_empty() {
        head.push_str(name);
    }
    if !sym.is_empty() {
        if !head.is_empty() {
            head.push(' ');
        }
        head.push_str(sym);
    }
    let tail = fmt.join_with(u, bracket_style);
    if tail.is_empty() {
        head
    } else if head.is_empty() {
        tail.trim().to_string()
    } else {
        format!("{}{}", head, tail)
    }
}

/// 单位输入校验提示（文档 §3.5，19 个用例）。
///
/// 只提示、不改写 —— 用户的写法可能有意为之。
pub fn unit_hints(unit: &str, name: &str) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    let u = unit.trim();
    let n = name.trim();
    if u.is_empty() {
        return out;
    }
    // 名称框里写了带括号的单位 → 填错框了
    if n.contains('(') && n.contains(')') && n.chars().any(|c| c.is_ascii_alphabetic()) {
        out.push("单位应填在「单位」框，不要写在名称里");
    }
    // 斜杠只能出现一次（mg/kg/d 是错的）
    if u.matches('/').count() > 1 {
        out.push("斜杠只能出现一次，复合单位请用括号，如 mg/(kg·d)");
    }
    // 升的符号必须大写 L
    if has_litre_issue(u) {
        out.push("升的符号应为大写 L（如 mmol/L、mL）");
    }
    // 摩尔浓度用 M / mM / uM → 建议改 mol/L 系
    if has_molar_issue(u) {
        out.push("建议改用 mol/L 系（如 mmol/L、μmol/L）");
    }
    out
}

/// 小写 l 结尾**且**去掉 l 后是合法 SI 词头 → 判为「该写 L 却写了 l」。
/// `mmol`、`mol` 的 l 是摩尔词尾，不能误报：`mmol` 去掉 l 是 `mmo`，不是词头。
fn has_litre_issue(u: &str) -> bool {
    for tok in u.split(|c: char| !c.is_ascii_alphanumeric()) {
        if tok.is_empty() {
            continue;
        }
        if tok == "l" {
            return true;
        }
        if tok.ends_with('l') && tok.len() >= 2 {
            let rest = &tok[..tok.len() - 1];
            if rest.chars().all(|c| c.is_ascii_lowercase()) && is_si_prefix(rest) {
                return true;
            }
        }
    }
    false
}

/// `M` / `mM` / `uM` / `μM` / `nM` / `pM`：摩尔浓度的旧写法，建议改 mol/L 系。
///
/// 判据是「以大写 M 结尾，且前面是合法 SI 词头或空」——
/// `mol/L`、`mL`、`mmol/L` 都不以 M 结尾，不会误报。
fn has_molar_issue(u: &str) -> bool {
    let t = u.trim();
    if !t.ends_with('M') || t.contains('/') {
        return false;
    }
    let rest = t[..t.len() - 1].to_lowercase();
    is_si_prefix(&rest) || rest.is_empty()
}

fn is_si_prefix(s: &str) -> bool {
    matches!(s, "m" | "n" | "u" | "μ" | "p" | "k" | "d" | "c" | "")
}

/// 千位分隔符：`1234567` → `1,234,567`（只处理整数部分，保留负号与小数）。
pub fn group_thousands(s: &str) -> String {
    let (sign, body) = match s.strip_prefix('-') {
        Some(b) => ("-", b),
        None => ("", s),
    };
    let (int, frac) = match body.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (body, None),
    };
    if int.len() <= 3 {
        return s.to_string();
    }
    let mut out = String::new();
    for (i, c) in int.chars().enumerate() {
        let from_end = int.len() - i;
        if i > 0 && from_end % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    match frac {
        Some(f) => format!("{}{}.{}", sign, out, f),
        None => format!("{}{}", sign, out),
    }
}

/// 刻度格式化（文档 §2.3）。
///
/// - 大数加千位分隔符
/// - **非零但会舍成 0 的值不显示 `0`**，改走科学记数
/// - 极小 / 极大值自动科学记数
pub fn fmt_tick_full(v: f64, step: f64) -> String {
    if !v.is_finite() {
        return String::new();
    }
    if v == 0.0 {
        return "0".to_string();
    }
    let a = v.abs();
    // 极大 / 极小 → 科学记数
    if a >= 1e6 || a < 1e-4 {
        return crate::stats::fmt_sci(v);
    }
    let digits = if step <= 0.0 || !step.is_finite() {
        4usize
    } else if step >= 1.0 {
        0usize
    } else {
        (-step.log10()).ceil().clamp(0.0, 6.0) as usize
    };
    let raw = format!("{:.*}", digits, v);
    // 非零却舍成了 0 → 补科学记数，不能让读者以为这里有 0
    if raw.trim_start_matches('-').trim_start_matches('0').trim_start_matches('.').is_empty()
        || raw.parse::<f64>().map(|x| x == 0.0).unwrap_or(false)
    {
        return crate::stats::fmt_sci(v);
    }
    let trimmed = if raw.contains('.') {
        raw.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        raw
    };
    group_thousands(&trimmed)
}

/// 把 `[min,max]` 向外吸附到整刻度边界（文档 §2.2）。
///
/// 数据 0.3–7.8 → 0–8；刻度数接近 target；数据全相等时退化成固定跨度。
pub fn snap_to_ticks(min: f64, max: f64, log: bool, target: usize) -> (f64, f64) {
    if !min.is_finite() || !max.is_finite() {
        return (0.0, 1.0);
    }
    if log {
        let lo = if min > 0.0 { min } else { 1.0 };
        let hi = if max > lo { max } else { lo * 10.0 };
        return (lo, hi);
    }
    if (max - min).abs() < 1e-12 {
        // 全相等：退化成 ±1 的固定跨度，不崩溃
        let c = if min == 0.0 { 0.0 } else { min };
        return (c - 1.0, c + 1.0);
    }
    let t = target.max(2);
    let raw = (max - min) / t as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let step = if norm <= 1.0 {
        mag
    } else if norm <= 2.0 {
        2.0 * mag
    } else if norm <= 5.0 {
        5.0 * mag
    } else {
        10.0 * mag
    };
    let lo = (min / step).floor() * step;
    let hi = (max / step).ceil() * step;
    if (hi - lo).abs() < 1e-12 {
        (lo, lo + step)
    } else {
        (lo, hi)
    }
}

/// 灰度方案的灰阶（文档 §4.3）：6 档 40/75/110/145/180/215。
pub fn gray_for(i: usize) -> Rgba {
    let step = (i % 6) as f32;
    let g = (40.0 + step * 35.0).round().clamp(20.0, 225.0) as u8;
    [g, g, g, 255]
}

/// 线型：实线 / 虚线 / 点线（文档 §4.4 的 DASHES）。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Dash {
    Solid,
    Dashed,
    Dotted,
}

impl Dash {
    pub fn from_usize(v: usize) -> Self {
        match v {
            1 => Dash::Dashed,
            2 => Dash::Dotted,
            _ => Dash::Solid,
        }
    }
    /// 把一条线段按线型切成若干子段（用于手绘虚线）。
    pub fn segments(self, a: (f64, f64), b: (f64, f64)) -> Vec<((f64, f64), (f64, f64))> {
        let (on, off) = match self {
            Dash::Solid => return vec![(a, b)],
            Dash::Dashed => (6.0f64, 4.0),
            Dash::Dotted => (2.0f64, 3.0),
        };
        let dx = b.0 - a.0;
        let dy = b.1 - a.1;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-9 {
            return vec![(a, b)];
        }
        let (ux, uy) = (dx / len, dy / len);
        let mut out = Vec::new();
        let mut d = 0.0f64;
        let mut drawing = true;
        while d < len {
            let seg = if drawing { on } else { off };
            let e = (d + seg).min(len);
            if drawing {
                out.push((
                    (a.0 + ux * d, a.1 + uy * d),
                    (a.0 + ux * e, a.1 + uy * e),
                ));
            }
            d = e;
            drawing = !drawing;
        }
        out
    }
}

/// 柱宽：厘米 → 类目索引单位（文档 §5.1）。
///
/// `ppcm` 是「每厘米多少像素」，含缩放与超采样 —— 不能硬编码 96 DPI，
/// 否则高 DPI 或放大预览时，屏幕上的实际宽度不等于标称厘米。
pub fn cm_width_in_index(plot_w_px: f64, span_index: f64, ppcm: f64, cm: f32) -> f64 {
    let cm = if cm > 0.05 { cm as f64 } else { 0.8 };
    let ppcm = if ppcm > 1.0 { ppcm } else { PX_PER_CM_96 as f64 };
    let span = span_index.abs().max(1e-9);
    let wpx = cm * ppcm;
    (wpx * span / plot_w_px.max(1.0)).min(span * 0.98).max(span * 0.02)
}

/// 1 厘米对应的像素数（96 dpi 基准）。
pub const PX_PER_CM_96: f32 = 37.795_275;

/// 轴断裂标记所需的两段短平行线（文档 §2.5）。
pub fn axis_break_segs(x: f64, y0: f64, y1: f64) -> Vec<((f64, f64), (f64, f64))> {
    let d = (y1 - y0).abs() * 0.045;
    vec![
        ((x - 3.0, y0 + d), (x + 3.0, y0 - d)),
        ((x - 3.0, y0 + d * 3.0), (x + 3.0, y0 - d * 3.0)),
    ]
}


/// 显著性括号的高度定位模式。
#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BracketMode {
    /// 自动分层：基于数据最高点逐层累加（默认，最安全）
    Auto,
    /// 绝对高度：直接指定数据坐标上的 Y 值
    Absolute,
    /// 相对柱子：最高点 × 一个倍率
    Relative,
}

impl BracketMode {
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            BracketMode::Auto => "自动分层",
            BracketMode::Absolute => "绝对高度",
            BracketMode::Relative => "相对柱子",
        }
    }
}

/// 一个数据集的统计量（柱默认画均值 + SD 误差棒 + 散点叠加）。
#[derive(Clone)]
pub struct GroupStat {
    pub name: String,
    pub mean: f64,
    pub sd: f64,
    pub n: usize,
    /// 原始值，用于散点叠加
    pub values: Vec<f64>,
}

/// 一组显著性对比（括号 + 星号）。
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Comparison {
    pub a: usize,
    pub b: usize,
    /// 标注文本：`*` / `**` / `***` / `ns` / `P = 0.0032`
    pub label: String,
    /// 自动算出的**校正后** P 值（枚举全部组对 + 多重比较校正）。
    ///
    /// 只用于「显示 P 值」和改组号时重新定级；用户手写了标注不会覆盖它，
    /// 所以手写内容永远是优先的。反序列化旧文件时为 None。
    pub p: Option<f64>,
}

/// 一次绘图的全部输入。
#[derive(Clone)]
pub struct ChartView {
    pub kind: ChartKind,
    pub groups: Vec<GroupStat>,
    pub title: String,
    // 轴标题：名称 + 单位分开存，排版时才决定斜体/正体与括号/斜杠
    pub x_name: String,
    pub x_unit: String,
    pub y_name: String,
    pub y_unit: String,
    pub unit_style: UnitStyle,
    /// 物理量符号是否斜体（单位恒为正体）
    pub use_italic: bool,
    /// 量符号（斜体显示），如 `c`、`t`；单位恒为正体
    pub x_sym: String,
    pub y_sym: String,
    /// 括号样式：0 半角 ()，1 全角 （）（GB 3100）
    pub bracket_style: usize,
    /// 柱宽（厘米）。0 表示用面板的分数值
    pub bar_width_cm: f32,
    /// Y 轴不从 0 开始时画断裂标记
    pub axis_break: bool,
    /// 启用右 Y 轴（双轴）
    pub right_axis: bool,
    /// 从哪个数据集起走右轴；`right_axis` 为假时忽略
    pub right_from: usize,
    /// 图注（画在 X 轴标题下方）
    pub caption: String,
    /// 网格线型：0 实线，1 虚线，2 点线
    pub grid_style: usize,
    /// 折线线型：0 实线，1 虚线，2 点线
    pub dash: usize,
    pub show_error: bool,
    pub show_scatter: bool,
    pub show_legend: bool,
    /// 反转图例顺序（Change ▸ 反转图例顺序）
    pub reverse_legend: bool,
    pub show_bracket: bool,
    /// 括号高度模式
    pub bracket_mode: BracketMode,
    /// Absolute 模式下的 Y 值 / Relative 模式下的倍率
    pub bracket_value: f64,
    /// 配色方案在 `COLORMAPS` 中的索引（0 = Okabe-Ito，色盲友好）
    pub colormap: usize,
    /// 显著性对比（按层级顺序绘制）
    pub comparisons: Vec<Comparison>,
    /// 图形标注（归一化坐标，后端无关）
    pub annotations: Vec<crate::model::Annotation>,
    // ── 符号与散点 ────────────────────────────────────────────────────────
    /// 散点是否跟随柱子配色；关闭后使用 `symbol_color_idx`
    pub symbol_follows_bar: bool,
    /// 符号专用色在 `SYMBOL_COLOR_PALETTE` 中的索引
    pub symbol_color_idx: usize,
    /// 散点半径（像素）
    pub symbol_size: f32,
    /// 抖动幅度（0 = 完全不抖，1 = 铺满柱宽的一半）
    pub jitter: f32,
    /// 全局缩放（字号、线宽随之放大；导出高分辨率图时用）
    pub scale: f32,
    /// 边框样式：L 型（只画左 + 下，析光默认）/ 全框
    pub frame: FrameStyle,
    /// 随机种子（散点抖动相位；Change ▸ 重新模拟 会换一个）
    pub seed: u64,
    // ── ② 数据 ────────────────────────────────────────────────────────────
    /// 误差棒量：0 = SD，1 = SEM，2 = 95% CI
    pub err_kind: usize,
    /// 误差棒方向：0 = 双向，1 = 只向上，2 = 只向下
    pub err_dir: usize,
    /// 误差棒线宽（px）
    pub err_width: f32,
    /// 把各组均值连成折线
    pub connect: bool,
    // ── ③ 元素 ────────────────────────────────────────────────────────────
    /// 柱宽 / 每组可用宽度
    pub bar_width: f32,
    /// 柱体填充图案：0 实心，1 斜线，2 交叉，3 横纹
    pub bar_pattern: usize,
    /// 基线强制为 0；关闭时从数据最小值起画
    pub bar_zero: bool,
    /// 折线图 / 面积图：曲线下方填充
    pub area_fill: bool,
    // ── ④ 坐标轴 ──────────────────────────────────────────────────────────
    /// 水平网格线
    pub grid_on: bool,
    /// Y 轴对数刻度（数据含 ≤0 时自动退回线性）
    pub log_y: bool,
    // ── ⑤ 配色 ────────────────────────────────────────────────────────────
    /// 灰度模式：所有色序转灰（单色印刷 / 期刊要求）
    pub grayscale: bool,
    // ── ⑦ 图例与线条 ──────────────────────────────────────────────────────
    /// 图例位置：0 = 右侧外，1 = 右上内，2 = 左上内
    pub legend_pos: usize,
    /// 全局线宽倍率
    pub line_width: f32,
    // ── 散点 ──────────────────────────────────────────────────────────────
    /// 符号形状：0 圆，1 方，2 三角，3 菱形
    pub symbol_shape: usize,
    /// 散点描边环宽（px）
    pub symbol_border: f32,
    /// 散点内部不透明度（0–1）
    pub symbol_alpha: f32,
    // ── ① 图形 ────────────────────────────────────────────────────────────
    /// 图标题加粗 / 倾斜
    pub title_bold: bool,
    pub title_italic: bool,
    // ── ⑧ 图类型专属 ──────────────────────────────────────────────────────
    /// 直方图 / 热图的分箱数
    pub bins: usize,
    /// 环形图内径比例（0 = 实心饼图）
    pub pie_inner: f32,
    /// 起始角（度，0 = 12 点方向顺时针）
    pub pie_start: f32,
    /// 热图色阶自动取值
    pub heat_auto: bool,
    /// 热图色阶下限 / 上限（`heat_auto` 关闭时生效）
    pub heat_lo: f64,
    pub heat_hi: f64,
    /// 森林图参考线（通常为 1 或 0）
    pub forest_ref: f64,
    /// 生存曲线画删失标记
    pub surv_censor: bool,
    /// 百分比点阵图（华夫图）的列数
    pub waffle_cols: usize,
    /// 100% 堆叠：各柱归一化到等高
    pub stacked_100: bool,
    /// 分面图共享同一套轴范围
    pub facet_shared: bool,
    /// 箱线样式：0 带须，1 只画盒
    pub box_style: usize,
}

/// 边框样式。析光默认是 **L 型框**（左 + 下），不是四面全框。
#[derive(Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum FrameStyle {
    /// L 型：只画左轴与下轴
    #[default]
    L,
    /// 全框：四面都画
    Box,
}

impl Default for ChartView {
    fn default() -> Self {
        Self {
            kind: ChartKind::Bar,
            groups: Vec::new(),
            title: String::new(),
            x_name: String::new(),
            x_unit: String::new(),
            y_name: String::new(),
            y_unit: String::new(),
            unit_style: UnitStyle::Paren,
            use_italic: true,
            x_sym: String::new(),
            y_sym: String::new(),
            bracket_style: 0,
            bar_width_cm: 0.0,
            axis_break: true,
            right_axis: false,
            right_from: 0,
            caption: String::new(),
            grid_style: 0,
            dash: 0,
            show_error: true,
            show_scatter: true,
            show_legend: true,
            reverse_legend: false,
            show_bracket: false,
            bracket_mode: BracketMode::Auto,
            bracket_value: 1.15,
            colormap: 0,
            comparisons: Vec::new(),
            annotations: Vec::new(),
            symbol_follows_bar: true,
            symbol_color_idx: 0,
            symbol_size: CHART_DOT_R as f32,
            jitter: 0.42,
            scale: 1.0,
            frame: FrameStyle::L,
            seed: 0x9E3779B9,
            err_kind: 0,
            err_dir: 0,
            err_width: CHART_ERROR_PX as f32,
            connect: false,
            bar_width: CHART_BAR_WIDTH_RATIO,
            bar_pattern: 0,
            bar_zero: true,
            area_fill: false,
            grid_on: false,
            log_y: false,
            grayscale: false,
            legend_pos: 0,
            line_width: 1.0,
            symbol_shape: 0,
            symbol_border: 1.5,
            symbol_alpha: 1.0,
            title_bold: true,
            title_italic: false,
            bins: 8,
            pie_inner: 0.0,
            pie_start: 0.0,
            heat_auto: true,
            heat_lo: 0.0,
            heat_hi: 1.0,
            forest_ref: 0.0,
            surv_censor: true,
            waffle_cols: 10,
            stacked_100: false,
            facet_shared: true,
            box_style: 0,
        }
    }
}

impl ChartView {
    /// 轴标题切成若干片段：(文本, 是否斜体)。符号斜体、单位正体。
    pub fn pieces(&self, axis: char) -> Vec<(String, bool)> {
        let (name, unit) = if axis == 'x' {
            (&self.x_name, &self.x_unit)
        } else {
            (&self.y_name, &self.y_unit)
        };
        let sym = if axis == 'x' { &self.x_sym } else { &self.y_sym };
        let full = axis_title(name, sym, unit, self.unit_style.to_i32(), self.bracket_style as i32);
        if full.is_empty() {
            return Vec::new();
        }
        // 分段：名称正体 → 量符号斜体 → 单位正体。
        // 不能整串斜体 —— 那样单位也斜了，违反 IUPAC / ISO 80000。
        let mut v = Vec::new();
        let mut rest = full.as_str();
        // 先定位量符号：优先按「空格 + 符号」匹配，
        // 避免名称里恰好含相同字符时错位（如名称 “cc” 与符号 “c”）。
        if !sym.is_empty() && self.use_italic {
            let pat = format!(" {}", sym.trim());
            let hit = full.find(&pat).map(|i| i + 1).or_else(|| full.find(sym.trim()));
            if let Some(i) = hit {
                let (a, b) = full.split_at(i);
                if !a.is_empty() {
                    v.push((a.to_string(), false));
                }
                let sym_len = sym.trim().len();
                let (c, d) = b.split_at(sym_len.min(b.len()));
                v.push((c.to_string(), true));
                rest = d;
            }
        }
        if !rest.is_empty() {
            v.push((rest.to_string(), false));
        }
        v
    }
}

// ── 几何计算（绘图与导出共用，保证两端像素一致）─────────────────────────────

/// Y 轴（或等价主数值轴）的语义。
///
/// 不同图表类型的纵轴含义完全不同 —— 直方图是**频数**、生存曲线是
/// **概率**、森林图的效应量画在**横轴**上。统一按「数值」处理会让刻度
/// 和实际画的量对不上（例如直方图柱高按频数画，左边却标着数值刻度）。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum YMode {
    /// 普通数值轴：Bar / Scatter / Line / Box / Violin
    Value,
    /// 频数轴：Histogram
    Count,
    /// 概率轴 0..1：Survival
    Prob,
    /// 效应量轴（沿 X 方向，Y 轴是组名）：Forest
    Effect,
    /// 无数值轴：Pie / Heatmap
    None,
}

pub struct Geom {
    pub h: i32,
    /// 右 Y 轴：哪些组走右轴（`right_axis` 关闭时全为空）
    pub on_right: Vec<bool>,
    pub right_min: f64,
    pub right_max: f64,
    pub right_ticks: Vec<f64>,
    pub right_step: f64,
    /// 走右轴的组数（0 = 右轴不生效）
    pub right_from_count: usize,
    /// 每厘米像素数（含缩放与超采样）。柱宽以厘米指定时用它换算，
    /// 不能硬编码 96 dpi —— 高 DPI 或放大预览下柱宽就不准了。
    pub ppcm: f64,
    pub left: i32,
    pub right: i32,
    pub top: i32,
    pub bottom: i32,
    pub y_min: f64,
    pub y_max: f64,
    pub ticks: Vec<f64>,
    /// 每组占的槽宽
    pub slot: f64,
    /// 所有数据（含误差棒顶端）的最高值 —— 显著性括号的基准
    #[allow(dead_code)]
    pub top_val: f64,
    /// Y 轴是否走对数刻度（数据含 ≤0 时自动退回线性）
    pub log: bool,
    /// 纵轴的语义（决定刻度怎么标、以及要不要标）
    pub y_mode: YMode,
}

pub fn geom(view: &ChartView, w: u32, h: u32) -> Geom {
    let (w, h) = (w.max(80) as i32, h.max(80) as i32);
    let left = (CHART_MARGIN_L * view.scale) as i32;
    // 右侧：图例移到绘图区外面后，要按图例宽度预留，否则会被裁掉
    let legend_w = if view.show_legend && view.legend_pos == 0 && !view.groups.is_empty() {
        let widest = view
            .groups
            .iter()
            .map(|gr| text_w(&gr.name, CHART_FONT as f64 * view.scale as f64))
            .fold(0.0f64, f64::max) as i32;
        ((10.0 + 10.0 + 5.0) * view.scale) as i32 + widest + ((14.0 * view.scale) as i32)
    } else {
        0
    };
    // 右轴启用时右侧留白从 14px 增到 76px —— 否则右轴标签会画到画布外
    // （文档 §2.6）。不带右轴时不浪费这 62px。
    let right_pad_extra = if (view.right_axis || view.kind.force_dual_y())
        && view.right_from < view.groups.len()
    {
        (62.0 * view.scale.max(0.5)) as i32
    } else {
        0
    };
    let right = w - ((CHART_MARGIN_R * view.scale) as i32 + legend_w + right_pad_extra);
    // 顶部：给图标题留出独立的一行，避免与显著性括号挤在一起
    let title_h = if view.title.is_empty() {
        0
    } else {
        (((CHART_FONT + 2.0) as f64 * view.scale as f64) * 1.6) as i32
    };
    // ── 为显著性括号把画布「往上扩」────────────────────────────────────────
    // 关键：**不能靠压缩绘图区腾地方**。top 下移 → bottom 不动 → 绘图区变矮 →
    // 纵轴被压短，那正是「加了标记 Y 轴就变样」。
    //
    // 正确做法是渲染入口把画布高度**加上同一个 bracket_extra_h**（见 render_rgba /
    // export_png / export_svg）。于是 bottom 跟着下移同样的量，
    //   bottom' - top' = (bottom + R) - (top + R) = 原绘图区高度
    // 绘图区分毫不变，纵轴像素范围、刻度、柱子高度开/关显著性完全一致，
    // 多出来的空间全部落在绘图区上方 —— 也就是「往上面扩」。
    let bracket_reserve = bracket_extra_h(view);
    let bottom = h - (CHART_MARGIN_B * view.scale) as i32;
    let top = ((CHART_MARGIN_T + 2.0) * view.scale) as i32 + title_h + bracket_reserve;
    // 预留量再大也不能把绘图区压没：至少留 40px 给数据
    let top = top.min(bottom - 40).max(0);

    let n = view.groups.len().max(1);
    let slot = (right - left) as f64 / n as f64;

    // 按**渲染族**分派，而不是按 62 个变体逐个列 —— 新增变体自动继承族的行为
    let y_mode = match view.kind.family() {
        Family::Histogram => YMode::Count,
        Family::Survival => YMode::Prob,
        Family::Forest => YMode::Effect,
        Family::Pie | Family::Heatmap | Family::Contingency | Family::Facet
        | Family::NumberAtRisk | Family::Radar | Family::Waffle | Family::Contour
        | Family::Ridgeline => YMode::None,
        _ => YMode::Value,
    };

    // ── 按轴的语义分别定范围 ────────────────────────────────────────────
    let (y_min, y_max_raw, top_val) = match y_mode {
        // 生存概率恒为 0..1，与数据大小无关
        YMode::Prob => (0.0f64, 1.0f64, 1.0f64),
        // 饼图 / 热图没有数值轴
        YMode::None => (0.0f64, 1.0f64, 1.0f64),
        // 直方图：纵轴是**频数**（落入各箱的计数），不是数据值
        YMode::Count => {
            let cmax = hist_bins(view).map(|r| r.4).unwrap_or(1).max(1) as f64;
            (0.0f64, cmax * 1.12, cmax)
        }
        // 森林图：效应量沿横轴，范围要同时容纳所有 CI 与参考线
        YMode::Effect => {
            let mut lo = f64::INFINITY;
            let mut hi = f64::NEG_INFINITY;
            for grp in &view.groups {
                let e = ci95(grp);
                lo = lo.min(grp.mean - e).min(view.forest_ref);
                hi = hi.max(grp.mean + e).max(view.forest_ref);
            }
            if !lo.is_finite() || !hi.is_finite() || hi <= lo {
                (0.0, 1.0, 1.0)
            } else {
                let pad = (hi - lo) * 0.12;
                (lo - pad, hi + pad, hi)
            }
        }
        YMode::Value => {
            // 箱线 / 小提琴按**原始数据**取范围。
            // 若沿用 mean±SD，长尾和离群点会被裁掉 —— 而离群点恰恰是要展示的东西。
            let by_data = matches!(view.kind.family(), Family::Box | Family::Violin);
            let mut low_val = 0.0f64;
            let mut top_val = 0.0f64;
            for gr in &view.groups {
                if by_data {
                    for v in &gr.values {
                        if *v > top_val {
                            top_val = *v;
                        }
                        if *v < low_val {
                            low_val = *v;
                        }
                    }
                    continue;
                }
                let e = err_of(view, gr.mean, gr.sd, gr.n);
                let mut m = if view.show_error { gr.mean + e } else { gr.mean };
                for v in &gr.values {
                    if *v > m {
                        m = *v;
                    }
                    if *v < low_val {
                        low_val = *v;
                    }
                }
                if view.show_error {
                    let lo = gr.mean - e;
                    if lo < low_val {
                        low_val = lo;
                    }
                }
                if m > top_val {
                    top_val = m;
                }
            }
            if by_data {
                // 上下各留 6% 余量，否则最高点会贴到框线上
                let span = (top_val - low_val).max(1e-9);
                top_val += span * 0.06;
                low_val -= span * 0.06;
            }
            let y_min = if view.bar_zero { 0.0f64 } else { low_val.min(0.0) };
            let hi = if top_val <= 0.0 { 1.0 } else { top_val };
            (y_min, hi * 1.15, hi)
        }
    };

    // 对数刻度：只要有一个值 ≤ 0 就退回线性，否则 ln 会出 -inf
    let use_log = view.log_y && y_min > 0.0 && y_max_raw > 0.0;

    // 显著性括号**不再参与定 y_max**。
    // 以前的做法是先算出括号需要的顶部空间再抬高 y_max，结果就是「一加显著性，
    // 纵轴刻度整个变了，柱子高度也跟着变」—— 这等于改了数据的视觉表达。
    // 现在括号是画在坐标轴上方的**覆盖层**（见 bracket_boxes），纵轴只由数据决定。

    // 把范围向外吸附到整齐的整刻度边界：数据 0.3–7.8 → 0–8。
    // 但**绝不能把 y_min 吸到 0 以下 / 让基线跑掉** —— 所以只对上界做吸附，
    // 下界保持原样。上界吸附也只取「更整齐且不小于数据上界」的那个值。
    let (y_min2, y_max2) = match y_mode {
        YMode::Prob | YMode::None => (y_min, y_max_raw),
        YMode::Count => (0.0, snap_to_ticks(0.0, y_max_raw, false, 5).1.max(y_max_raw)),
        _ => (y_min, snap_to_ticks(y_min, y_max_raw, use_log, 6).1.max(y_max_raw)),
    };

    // ── 右 Y 轴（文档 §2.6）──────────────────────────────────────────────
    // 从 `right_from` 起的数据集用独立刻度，其余仍走左轴。
    let mut on_right = vec![false; view.groups.len()];
    let mut right_from_count = 0usize;
    // `force_dual_y()`：图型本身语义就是双轴（DualYAxis），此时不需要用户
    // 再手动开 right_axis —— 后半组自动走右轴。
    let forced = view.kind.force_dual_y();
    if (view.right_axis || forced) && view.right_from < view.groups.len() {
        for i in view.right_from..view.groups.len() {
            on_right[i] = true;
            right_from_count += 1;
        }
    }
    let (right_min, right_max, right_ticks, right_step) = if right_from_count > 0 {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for (i, gr) in view.groups.iter().enumerate() {
            if !on_right[i] {
                continue;
            }
            let e = err_of(view, gr.mean, gr.sd, gr.n);
            lo = lo.min(gr.mean - e).min(gr.mean);
            hi = hi.max(gr.mean + e).max(gr.mean);
            for v in &gr.values {
                lo = lo.min(*v);
                hi = hi.max(*v);
            }
        }
        if !lo.is_finite() || !hi.is_finite() || hi <= lo {
            (0.0, 1.0, vec![0.0, 1.0], 1.0)
        } else {
            let (a, b) = snap_to_ticks(lo, hi, false, 6);
            let t = nice_ticks(a, b, 6);
            let st = t.get(1).map(|x| x - t[0]).unwrap_or((b - a) / 5.0);
            (a, b, t, st)
        }
    } else {
        (0.0, 1.0, Vec::new(), 1.0)
    };

    // `reversed_y()`：纵轴反向（大值在下）。只需交换上下界，
    // y_of 的线性映射会自动跟着翻转，不需要改动下游任何绘制代码。
    let (y_min2, y_max2) = if view.kind.reversed_y() {
        (y_max2, y_min2)
    } else {
        (y_min2, y_max2)
    };

    let ticks = match y_mode {
        YMode::Prob => vec![0.0, 0.25, 0.5, 0.75, 1.0],
        YMode::None => Vec::new(),
        // 频数刻度必须是整数：标「3.7 个计数」没有意义
        YMode::Count => {
            let t = nice_ticks(0.0, y_max2, 5);
            let mut out: Vec<f64> = t.into_iter().map(|v| v.round().max(0.0)).collect();
            out.dedup();
            if out.is_empty() {
                vec![0.0, 1.0]
            } else {
                out
            }
        }
        _ => nice_ticks(y_min2, y_max2, 6),
    };
    let y_max = ticks.last().copied().unwrap_or(y_max2);

    Geom {
        h, left, right, top, bottom,
        ppcm: PX_PER_CM_96 as f64 * view.scale.max(0.01) as f64,
        on_right,
        right_min,
        right_max,
        right_ticks,
        right_step,
        right_from_count,
        y_min: y_min2, y_max, ticks, slot, top_val,
        log: use_log, y_mode,
    }
}

/// 柱子的半宽（类目索引单位）。
///
/// 优先用**厘米**指定（文档 §5.1）：与析光一致，柱宽和散布宽度同步变化，
/// 且在高 DPI / 缩放预览下屏幕上的实际尺寸仍等于标称厘米。
/// `bar_width_cm` 为 0 时退回面板的分数值。
pub fn bar_half(g: &Geom, view: &ChartView) -> f64 {
    let span = 1.0f64;
    let half = if view.bar_width_cm > 0.05 {
        cm_width_in_index(
            (g.right - g.left).max(1) as f64,
            span,
            g.ppcm,
            view.bar_width_cm,
        ) / 2.0
    } else {
        span * (view.bar_width.clamp(0.10, 0.98) as f64) / 2.0
    };
    half.max(1.0).min(span * 0.49)
}

/// 抖动序列的第 n 项，落在 [0,1)。
///
/// **用黄金比低差异序列，不用伪随机**：
/// - 低差异序列保证在 [-1,1] 上**均匀铺满**，不会出现伪随机常见的
///   「某处挤成一坨、某处空一片」；数值本身均匀时也不会塌成一条竖线。
/// - 结果是**确定**的（不含 rand），所以每帧一致，不会闪烁。
/// - 与 Y 值**不相关**：相位只取决于序号，不取决于数值。
///   若数据恰好按 Y 排序，偏移量也不会跟着连成一条斜带 ——
///   因为序号里混入了组号（乘一个与黄金比互质的大质数）打散了顺序。
fn frac(seed: u64, group: usize, k: usize) -> f64 {
    const PHI: f64 = 0.618_033_988_749_894_9; // 黄金比共轭
    // 997 与黄金比无公度，能把「组号 × 组内序号」摊开成一维低差异序列
    let n = (seed % 7919) as f64 + (group as f64) * 997.0 + (k as f64);
    let x = n * PHI;
    x - x.floor()
}

/// 组 i 的中心 X（像素）。
pub fn x_of(g: &Geom, i: usize) -> f64 {
    g.left as f64 + g.slot * (i as f64 + 0.5)
}

/// 数值 → Y 像素。
pub fn y_of(g: &Geom, v: f64) -> f64 {
    // 对数刻度：把 y_min / y_max / v 一起取 ln 再线性映射。
    // 这是「先变换后映射」，与 Prism 的 log axis 一致。
    let (lo, hi, x) = if g.log {
        let f = |t: f64| t.max(1e-6).ln();
        (f(g.y_min), f(g.y_max), f(v))
    } else {
        (g.y_min, g.y_max, v)
    };
    g.bottom as f64 - (x - lo) / (hi - lo) * (g.bottom - g.top) as f64
}

/// 右轴的映射（独立范围）。
pub fn right_y_of(g: &Geom, v: f64) -> f64 {
    let (lo, hi) = (g.right_min, g.right_max);
    if hi <= lo {
        return g.bottom as f64;
    }
    g.bottom as f64 - (v - lo) / (hi - lo) * (g.bottom - g.top) as f64
}

/// 按组选轴：走右轴的组用右轴刻度，其余用左轴。
///
/// 双轴图里如果所有系列都按左轴画，右轴就成了装饰 —— 那不如不做。
pub fn y_of_grp(g: &Geom, gi: usize, v: f64) -> f64 {
    if g.on_right.get(gi).copied().unwrap_or(false) {
        right_y_of(g, v)
    } else {
        y_of(g, v)
    }
}

/// 显著性括号的几何（像素坐标）：(xa, xb, y0臂底, y1横线, label)
///
/// **锚定数据**：括号的起点是被比较的两组里**更高的那一组的视觉顶端**（误差棒顶端
/// 与散点最大值取高者，与 geom 的 top_val 同源），再往上抬一点，然后逐层向上堆叠。
///
/// 之前是从绘图区顶边固定往下排 —— 那样括号的位置只跟画布有关、跟数据无关。
/// 一旦 y_min 不是 0（顶部留白比例变大）或图被缩小，括号就会悬在半空或插进柱子里，
/// 也就是看到的「错位」。
///
/// 同时它仍**不参与 y 轴定标**：纵轴刻度、柱子高度在开/关显著性时完全一致。
/// 显著性括号需要**额外扩出来的画布高度**（已含 `view.scale`，单位=画布像素）。
///
/// 渲染入口（render_rgba / export_png / export_svg）把画布高度加上它，
/// `geom` 里 top 同步加同样的量 —— bottom 因画布变高也加了同样的量，
/// 于是绘图区高度不变、纵轴不被压短，多出的空间全在绘图区上方。
///
/// 上限 12 层：再多层数会把画布无限拉高，超出可视带的由 bracket_boxes 截断。
/// 显著性括号实际需要几层 —— **横向不重叠的对比共享一层**。
///
/// 只依赖组索引区间，不需要 `Geom`：组在横轴上按索引单调排列，
/// 所以「像素区间是否相交」完全等价于「索引区间是否相交」。这样在
/// `geom` 之前（画布还没布局完）也能算出预留高度。
pub fn bracket_layers(view: &ChartView) -> usize {
    if !view.show_bracket || view.groups.is_empty() {
        return 0;
    }
    let mut items: Vec<(usize, usize)> = view
        .comparisons
        .iter()
        .filter(|c| c.a != c.b && c.a < view.groups.len() && c.b < view.groups.len())
        .map(|c| (c.a.min(c.b), c.a.max(c.b)))
        .collect();
    if items.is_empty() {
        return 0;
    }
    items.sort_unstable();
    let mut layers: Vec<Vec<(usize, usize)>> = Vec::new();
    for it in items {
        let mut li = 0usize;
        loop {
            if li == layers.len() {
                layers.push(Vec::new());
            }
            // 端点相接不算相交：A-B 与 B-C 共享 B 但不相交，可以同层
            let free = layers[li].iter().all(|&j| !(j.0 < it.1 && it.0 < j.1));
            if free {
                break;
            }
            li += 1;
        }
        layers[li].push(it);
    }
    layers.len()
}

pub fn bracket_extra_h(view: &ChartView) -> i32 {
    // ★ 按**层数**预留，不是按对比条数：A-B / B-C / C-D 这类链条同层，
    //   按条数预留会把画布白白拉高好几倍。
    let n = bracket_layers(view);
    if n == 0 {
        return 0;
    }
    let sc = view.scale.max(0.5);
    let fs = CHART_FONT * sc;
    // 与 bracket_boxes 的 step 同源：标签整高 + 竖臂 + 间隙
    let step = (fs * 1.8 + 5.0 * sc + 1.5 * sc).ceil();
    (n.min(12) as f32 * step) as i32
}

pub fn bracket_boxes(view: &ChartView, g: &Geom) -> Vec<(f64, f64, f64, f64, String)> {
    if !view.show_bracket || view.comparisons.is_empty() {
        return Vec::new();
    }
    // 饼图 / 雷达 / 热图这类没有数值纵轴的族，括号没有落点，直接不画
    if matches!(g.y_mode, YMode::None) {
        return Vec::new();
    }
    let sc = view.scale.max(0.5) as f64;
    let fs = CHART_FONT as f64 * sc;
    // 竖臂：固定像素长度，不再随数据量级变化（以前是 span×比例，数据一大臂就很长）
    let arm = 5.0 * sc;
    // 标签基线画在横线上方 fs*1.05 处，字顶再往上约 0.75 个 fs
    // → 一层标签实际占用 1.8*fs。必须和下面绘制时的基线偏移保持一致，
    //   否则 min_step 算小了，上一层的横线会压在本层的星号上。
    let lab_h = fs * 1.8;
    // 层间距 = 标签区 + 竖臂 + 一点间隙，否则下一层的臂会穿过上一层的星号
    let step = lab_h + arm + 1.5 * sc;

    // 顶部硬边界：标签允许探进上边距，但绝不能压到图标题。
    // ★ 这里必须按「标题底部」直接算，不能用 g.top 反推：
    // g.top 里包含了为括号预留的 bracket_reserve，用 g.top 反推会把预留量
    // 一起减掉，预留的空间等于白留（band 一点没变大），括号照样被裁掉。
    let title_h = if view.title.is_empty() {
        0.0
    } else {
        (CHART_FONT as f64 + 2.0) * sc * 1.6
    };
    let ceil = title_h + 2.0 * sc;

    // 三种模式决定「相对数据顶端再往上抬多少」：
    //   Auto     紧贴数据顶端（+3px 呼吸）
    //   Absolute 额外上抬 bracket_value 个数值单位（换算成像素）
    //   Relative 额外上抬 绘图区高度 × bracket_value
    let extra = match view.bracket_mode {
        BracketMode::Auto => 3.0 * sc,
        BracketMode::Absolute => {
            let span = (g.y_max - g.y_min).abs().max(1e-9);
            let px_per_v = (g.bottom - g.top) as f64 / span;
            3.0 * sc + (view.bracket_value * px_per_v).max(0.0)
        }
        BracketMode::Relative => {
            3.0 * sc + ((g.bottom - g.top) as f64 * view.bracket_value.max(0.0)).max(0.0)
        }
    };

    // 每组「视觉顶端」的像素 y（越小越靠上）。箱线/小提琴按原始数据的最大值，
    // 其余按 mean±误差棒顶端、再与散点最大值比较 —— 与 geom 的 top_val 同源。
    let by_data = matches!(view.kind.family(), Family::Box | Family::Violin);
    let top_px = |i: usize| -> f64 {
        let gr = &view.groups[i];
        let mut v = f64::NEG_INFINITY;
        if !by_data {
            v = if view.show_error {
                gr.mean + err_of(view, gr.mean, gr.sd, gr.n)
            } else {
                gr.mean
            };
        }
        for x in &gr.values {
            if *x > v {
                v = *x;
            }
        }
        if !v.is_finite() {
            v = gr.mean;
        }
        y_of_grp(g, i, v)
    };

    // ── 分层：横向范围不重叠的对比共享同一层 ────────────────────────────
    // 以前每个对比都从「自己的数据顶端」单独往上抬，于是 A-B 与 C-D 明明
    // 互不搭界也被拉开好几层，纵向空间白白浪费。现在先按横向区间做
    // **区间图着色**：区间不相交就放同一层 —— A-B 与 C-D 同层，
    // 连续的 A-B / B-C / C-D 也同层（区间端点相接不算相交）。
    let mut items: Vec<(f64, f64, f64, String)> = Vec::new();
    // (x0, x1, data_top, label)
    for c in view.comparisons.iter() {
        // 空列被过滤后组数会变少，越界的对比直接跳过（否则括号会画到绘图区外）
        if c.a >= view.groups.len() || c.b >= view.groups.len() || c.a == c.b {
            continue;
        }
        let (xa, xb) = (x_of(g, c.a), x_of(g, c.b));
        // ★ 端点就是两组的中心，不再向外扩：扩出去之后 A-B 与 B-C 会在
        //   共享的那根柱子上相交，本来能同层的被迫拆成两层。
        let (x0, x1) = (xa.min(xb), xa.max(xb));
        // 两组里更高的那组决定这一层能放多低
        let data_top = top_px(c.a).min(top_px(c.b));
        items.push((x0, x1, data_top, c.label.clone()));
    }
    if items.is_empty() {
        return Vec::new();
    }
    // 按左端点升序，贪心放进第一个不冲突的层（层 0 = 最贴近数据的一层）
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&p, &q| {
        items[p]
            .0
            .partial_cmp(&items[q].0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut layers: Vec<Vec<usize>> = Vec::new();
    for idx in order {
        let mut li = 0usize;
        loop {
            if li == layers.len() {
                layers.push(Vec::new());
            }
            // 端点相接不算相交：A-B 与 B-C 只在 B 的中心处相接
            let free = layers[li]
                .iter()
                .all(|&j| !(items[j].0 < items[idx].1 && items[idx].0 < items[j].1));
            if free {
                break;
            }
            li += 1;
        }
        layers[li].push(idx);
    }

    // ── 每层的 y：既要在本层覆盖的数据之上，也要给下一层留出标签空间 ──
    // 上界 = 标题底部（再往上就压到图标题）。
    let band_hi = ceil + lab_h;
    let mut layer_y: Vec<f64> = Vec::new();
    for li in 0..layers.len() {
        let dt = layers[li]
            .iter()
            .map(|&j| items[j].2)
            .fold(f64::INFINITY, f64::min);
        // 本层允许的最低位置：本层覆盖的最高数据顶端，再让出一个竖臂。
        // ★ 用**本层自己的** data_top，而不是全局最小值 —— 以前用全局值，
        //   矮柱那一层的横线就会被顶到高柱内部（「标记压在柱子上」）。
        let own = dt - extra - arm - 2.0 * sc;
        let y = if li == 0 {
            own
        } else {
            (layer_y[li - 1] - step).min(own)
        };
        layer_y.push(y);
    }
    // 超出可视带的层直接丢弃（layer_y 递减，一旦越界后面都不成立）
    let keep = layer_y.iter().take_while(|&&y| y >= band_hi).count();

    let mut out: Vec<(f64, f64, f64, f64, String)> = Vec::new();
    for li in 0..keep {
        for &j in layers[li].iter() {
            let y = layer_y[li];
            // (xa, xb, 竖臂底端 y0, 横线 y1, 标签)
            out.push((items[j].0, items[j].1, y + arm, y, items[j].3.clone()));
        }
    }
    out
}

// ── 颜色 / 样式 ─────────────────────────────────────────────────────────────

/// Rgba → plotters 颜色。带 alpha 时交给后端混合（白底上即「混白底后变浅」）。
fn pc(c: Rgba) -> RGBAColor {
    RGBAColor(c[0], c[1], c[2], c[3] as f64 / 255.0)
}

/// 描边样式（width 单位为像素）。
fn stroke(c: Rgba, width: u32) -> ShapeStyle {
    ShapeStyle { color: pc(c), filled: false, stroke_width: width }
}

/// 填充样式。
fn area_fill(c: Rgba) -> ShapeStyle {
    ShapeStyle { color: pc(c), filled: true, stroke_width: 0 }
}

fn rgb(hex: &str) -> RGBColor {
    let h = hex.trim_start_matches('#');
    let p = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    RGBColor(p(0), p(2), p(4))
}

// ── 文本 ────────────────────────────────────────────────────────────────────

/// 按内容选导出字体：含 CJK → 宋体，否则 → Times New Roman。
fn font_of(s: &str) -> &'static str {
    crate::fonts::font_for(s)
}

/// 字宽估算：ASCII 0.55em、CJK 1.0em。
pub(crate) fn text_w(s: &str, size: f64) -> f64 {
    s.chars()
        .map(|c| if c.is_ascii() { size * 0.55 } else { size })
        .sum()
}

fn fmt_tick(v: f64, step: f64) -> String {
    fmt_tick_full(v, step)
}

/// 把一段文字离屏栅格化（白底 + 不透明文字），返回 (RGBA 缓冲, 宽, 高)。
///
/// 用于实现 plotters 不支持的**真·文字旋转**：先在临时位图上正常排版，
/// 再整块旋转 90° 后逐像素贴回目标画布。
pub(crate) fn raster_text(s: &str, size: f64, fam: &str, color: RGBColor, italic: bool) -> Option<(Vec<u8>, u32, u32)> {
    let tw = (text_w(s, size).ceil() as u32).max(2) + 4;
    let th = ((size * 1.6).ceil() as u32).max(2) + 4;
    // plotters 的 with_buffer 固定写 RGB（3 字节/像素），不是 RGBA
    let mut buf = vec![0u8; (tw * th * 3) as usize];
    {
        let area = BitMapBackend::with_buffer(&mut buf, (tw, th)).into_drawing_area();
        area.fill(&WHITE).ok()?;
        // 用 FontDesc 的固有方法 style() 设斜体 —— 不依赖 BackendTextStyle trait
        let fd: FontDesc = (fam, size).into_font();
        let fd = if italic { fd.style(FontStyle::Italic) } else { fd };
        area.draw(&Text::new(s.to_string(), (2, 2), fd.color(&color))).ok()?;
        area.present().ok()?;
    }
    Some((buf, tw, th))
}

/// 把栅格块按 90° 逆时针旋转后，以 (cx, cy) 为中心贴到画布。
///
/// 屏幕坐标（y 向下）下，数学上的逆时针 90° 对应变换 (x,y) → (y, w-1-x)：
/// 阅读方向 +x 变成向上（自下而上读），字头方向 -y 变成向左 —— 即标准 Y 轴标题。
pub(crate) fn blit_rotated_ccw<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    buf: &[u8],
    tw: u32,
    th: u32,
    cx: i32,
    cy: i32,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    let off_x = cx - th as i32 / 2;
    let off_y = cy - tw as i32 / 2;
    for y in 0..th as i32 {
        for x in 0..tw as i32 {
            let i = ((y as u32 * tw + x as u32) * 3) as usize;
            let (r, g, b) = (buf[i], buf[i + 1], buf[i + 2]);
            // 跳过白底，只贴字形像素
            if r > 250 && g > 250 && b > 250 {
                continue;
            }
            let dx = off_x + y;
            let dy = off_y + (tw as i32 - 1 - x);
            root.draw_pixel((dx, dy), &RGBAColor(r, g, b, 1.0))?;
        }
    }
    Ok(())
}

/// 旋转 90° 的轴标题：逐片段栅格化后沿垂直方向拼接，保留各自的斜体/正体。
fn draw_rotated_label<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    pieces: &[(String, bool)],
    cx: i32,
    cy: i32,
    size: f64,
    ink: RGBColor,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    let widths: Vec<f64> = pieces.iter().map(|(s, _)| text_w(s, size)).collect();
    let total: f64 = widths.iter().sum();
    let mut cursor = cy as f64 + total / 2.0; // 自下而上
    for ((s, italic), w) in pieces.iter().zip(widths) {
        let center = (cursor - w / 2.0) as i32;
        if let Some((buf, tw, th)) = raster_text(s, size, font_of(s), ink, *italic) {
            blit_rotated_ccw(root, &buf, tw, th, cx, center)?;
        }
        cursor -= w;
    }
    Ok(())
}

/// 散点的 (内芯色, 描边色)：
/// - 跟随柱子：柱填充色 / 柱边框色
/// - 不跟随：符号专用色 / 该色压暗后的 shade()
/// 两种情况下都是「双层绘制」——底层大圆描边色、上层小圆填充色。
/// 符号配色：13 种模式。
///
/// | 模式 | 含义 |
/// |---|---|
/// | 跟随柱子 | 用该系列柱体的填充色 + 压暗边框色（默认） |
/// | 跟随外观 | 深色界面用白、浅色界面用黑，**实时**随系统翻转 |
/// | 1–12 | 具名色：黑 白 灰 红 橙 黄 绿 青 蓝 靛 紫 品红 |
///
/// 返回 (内芯色, 描边色)。
fn symbol_colors(view: &ChartView, _cm: usize, _i: usize, bar_edge: &Rgba, bar_fill: &Rgba) -> (Rgba, Rgba) {
    const FOLLOW_APPEARANCE: usize = 0;
    // 「跟随外观」：深色用白、浅色用黑。读的是当前解析后的主题，
    // 所以系统切换深浅后会立刻跟着变，不是启动时定死的。
    let appearance: Rgba = if crate::palette::dark() {
        [255, 255, 255, 255]
    } else {
        [26, 26, 26, 255]
    };
    if view.symbol_follows_bar {
        (*bar_fill, *bar_edge)
    } else if view.symbol_color_idx == FOLLOW_APPEARANCE {
        // 内芯用外观色本体，描边在深底上要更暗、在浅底上要更亮才分得清
        let edge = if crate::palette::dark() { [0, 0, 0, 255] } else { [255, 255, 255, 255] };
        (appearance, edge)
    } else {
        // 1–12 → 具名色表的 0–11
        let base = colormap::symbol_color(view.symbol_color_idx.saturating_sub(1));
        (
            tone(view, colormap::mix_white(base, 1.0)),
            tone(view, colormap::shade(base)),
        )
    }
}

// ── 核心：后端无关的绘图逻辑 ────────────────────────────────────────────────

/// 把图表画到任意绘图后端上。位图预览与 SVG 导出共用这一份实现。

/// 灰度模式：按亮度权重转成灰阶，保留 alpha（印刷 / 单色期刊要求）。
fn gray(c: Rgba) -> Rgba {
    // 量化到 6 档灰阶（40/75/110/145/180/215）。
    // 直接用亮度值会让相邻系列只差 2–3 级灰度，黑白打印根本分不开；
    // 量化后两端明度差足够，再叠填充图案就能可靠区分。
    let l = (0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32) as f32;
    let step = ((l - 40.0) / 35.0).round().clamp(0.0, 5.0) as usize;
    let g = gray_for(step);
    [g[0], g[1], g[2], c[3]]
}

/// 统一走一遍灰度开关，保证柱体 / 边框 / 散点 / 图例**同时**变灰。
/// 高斯核密度估计，返回 `[0,1] × [0,max]` 归一化的曲线点。
///
/// 带宽用 Silverman 经验法则 `h = 0.9 · min(sd, IQR/1.34) · n^(-1/5)`；
/// 样本量不足或完全相同时退化为单点，避免除零。
/// 与 violin 的轮廓共用同一套估计，保证同一份数据在两种图里形状一致。
pub fn density_curve(values: &[f64], steps: usize) -> Vec<(f64, f64)> {
    let v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
    let n = v.len();
    if n < 2 || steps < 2 {
        return vec![(0.5, 1.0)];
    }
    let mean = v.iter().sum::<f64>() / n as f64;
    let var = v.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / (n - 1) as f64;
    let sd = var.sqrt();
    let mut sorted = v.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let q = |p: f64| {
        let idx = (p * (n as f64 - 1.0)).round() as usize;
        sorted[idx.min(n - 1)]
    };
    let iqr = q(0.75) - q(0.25);
    let a = if iqr > 1e-12 { iqr / 1.34 } else { sd };
    let h = 0.9 * a.min(sd) * (n as f64).powf(-0.2);
    let h = if h > 1e-9 { h } else { 1e-9 };
    let lo = sorted[0] - 3.0 * h;
    let hi = sorted[n - 1] + 3.0 * h;
    let span = (hi - lo).max(1e-9);
    let mut out = Vec::with_capacity(steps);
    let mut dmax = 1e-12f64;
    for k in 0..steps {
        let x = lo + span * (k as f64 / (steps - 1) as f64);
        let mut d = 0.0f64;
        for y in &v {
            let z = (x - y) / h;
            d += (-0.5 * z * z).exp();
        }
        d /= n as f64 * h * (2.0 * std::f64::consts::PI).sqrt();
        dmax = dmax.max(d);
        out.push((k as f64 / (steps - 1) as f64, d));
    }
    out.into_iter().map(|(t, d)| (t, d / dmax)).collect()
}

fn tone(view: &ChartView, c: Rgba) -> Rgba {
    if view.grayscale { gray(c) } else { c }
}

/// 保留原色，只改不透明度（散点内部透明度用）。
fn with_alpha(c: Rgba, a: f32) -> Rgba {
    [c[0], c[1], c[2], (255.0 * a.clamp(0.0, 1.0)) as u8]
}

/// 误差棒的长度：SD / SEM / 95% CI。
///
/// CI 用 `1.96 × SEM`（正态近似）。n 很小时严格来说该用 t 分位数，
/// 但对「看图」这个用途差值在像素级以下，故不引入 t 分布表。
fn err_of(view: &ChartView, _mean: f64, sd: f64, n: usize) -> f64 {
    if n < 2 {
        return 0.0;
    }
    match view.err_kind {
        1 => sd / (n as f64).sqrt(),
        // 置信区间的 z 取设置里的置信水平（90 / 95 / 99）
        2 => crate::settings::get().z() * sd / (n as f64).sqrt(),
        _ => sd,
    }
    .max(0.0)
}

/// 画一个**实心**符号（形状由 `symbol_shape` 决定）。
///
/// 散点的双层描边靠调用两次实现：外层大一号用深色、内层小一号用填充色，
/// 所以这里只负责「按形状画一个实心块」。
fn draw_shape<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    view: &ChartView,
    p: (i32, i32),
    r: i32,
    style: ShapeStyle,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    let (x, y) = p;
    match view.symbol_shape {
        1 => {
            root.draw(&Polygon::new(
                vec![(x - r, y - r), (x + r, y - r), (x + r, y + r), (x - r, y + r)],
                style,
            ))?;
        }
        2 => {
            root.draw(&Polygon::new(
                vec![(x, y - r), (x + r, y + r), (x - r, y + r)],
                style,
            ))?;
        }
        3 => {
            root.draw(&Polygon::new(
                vec![(x, y - r), (x + r, y), (x, y + r), (x - r, y)],
                style,
            ))?;
        }
        _ => {
            root.draw(&Circle::new(p, r, style))?;
        }
    }
    Ok(())
}

/// 柱体内部的图案填充（斜线 / 交叉 / 横纹）。
///
/// 线段端点都先按矩形边界求交再画，所以**不会溢出柱体** ——
/// 直接画整条斜线会糊到柱外面。
fn hatch<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    view: &ChartView,
    x0: i32,
    x1: i32,
    ytop: i32,
    ybot: i32,
    pattern: usize,
    style: ShapeStyle,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    if pattern == 0 || x1 <= x0 || ybot <= ytop { return Ok(()); }
    let step = (5.0 * view.scale.max(0.5)).max(2.0) as i32;
    let h = ybot - ytop;
    let w = x1 - x0;
    // 斜线 "/": y = ybot - (x - x0) + c
    let diag = |c: i32| -> Option<((i32, i32), (i32, i32))> {
        let xa = (x0 + c).max(x0);
        let xb = (ybot + x0 + c - ytop).min(x1);
        if xb <= xa { return None; }
        Some(((xa, ybot + x0 + c - xa), (xb, ybot + x0 + c - xb)))
    };
    // 反斜线 "\": y = ytop + (x - x0) - c
    let bdiag = |c: i32| -> Option<((i32, i32), (i32, i32))> {
        let xa = (x0 + c).max(x0);
        let xb = (x0 + c + h).min(x1);
        if xb <= xa { return None; }
        Some(((xa, ytop + xa - x0 - c), (xb, ytop + xb - x0 - c)))
    };
    match pattern {
        3 => {
            let mut y = ytop + step;
            while y < ybot {
                root.draw(&PathElement::new(vec![(x0, y), (x1, y)], style.clone()))?;
                y += step;
            }
        }
        2 => {
            let mut c = -h;
            while c < w + h {
                if let Some((a, b)) = diag(c) {
                    root.draw(&PathElement::new(vec![a, b], style.clone()))?;
                }
                if let Some((a, b)) = bdiag(c) {
                    root.draw(&PathElement::new(vec![a, b], style.clone()))?;
                }
                c += step;
            }
        }
        _ => {
            let mut c = -h;
            while c < w + h {
                if let Some((a, b)) = diag(c) {
                    root.draw(&PathElement::new(vec![a, b], style.clone()))?;
                }
                c += step;
            }
        }
    }
    Ok(())
}

/// 直方图分箱结果：`(下界, 上界, 箱数, 每组的计数, 计数上限)`。
///
/// geom() 与 draw_kind_specific 共用这一份 —— 否则轴范围和柱高会用两套
/// 算法，出现「柱高已经顶到框顶、刻度却只到一半」的错乱。
fn hist_bins(view: &ChartView) -> Option<(f64, f64, usize, Vec<Vec<usize>>, usize)> {
    let all: Vec<f64> = view.groups.iter().flat_map(|gr| gr.values.clone()).collect();
    if all.is_empty() {
        return None;
    }
    let lo = all.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = all.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !(hi > lo) {
        return None;
    }
    let nb = view.bins.clamp(1, 64);
    let counts: Vec<Vec<usize>> = view
        .groups
        .iter()
        .map(|gr| histogram(&gr.values, lo, hi, nb))
        .collect();
    let cmax = counts
        .iter()
        .flat_map(|c| c.iter().copied())
        .max()
        .unwrap_or(1)
        .max(1);
    Some((lo, hi, nb, counts, cmax))
}

/// 森林图的 95% 置信区间半宽。
///
/// 森林图的横线**必须**是 CI（mean ± 1.96·SEM），不能拿 SD 代替 ——
/// SD 描述个体离散程度，CI 描述均值估计的精度，两者相差 √n 倍。
/// 用 SD 画出来的横线会离谱地长。
fn ci95(grp: &GroupStat) -> f64 {
    if grp.n < 2 {
        return 0.0;
    }
    (1.96 * grp.sd / (grp.n as f64).sqrt()).max(0.0)
}

/// 分位数（线性插值，与 Excel 的 QUARTILE.INC 一致）：
/// `q` ∈ [0,1]。空数据返回 0。
fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let pos = q * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = (lo + 1).min(sorted.len() - 1);
    sorted[lo] + (pos - lo as f64) * (sorted[hi] - sorted[lo])
}

/// 高斯核密度估计。在 `grid` 上每个点求密度。
fn kde(sorted: &[f64], grid: &[f64], bw: f64) -> Vec<f64> {
    let n = sorted.len() as f64;
    grid.iter()
        .map(|&x| {
            let mut acc = 0.0f64;
            for &v in sorted {
                let z = (x - v) / bw;
                acc += (-0.5 * z * z).exp();
            }
            acc / (n * bw * (2.0 * std::f64::consts::PI).sqrt())
        })
        .collect()
}

/// Silverman 经验带宽；数据过于集中时给一个下限，避免 bw = 0。
fn kde_bw(sorted: &[f64]) -> f64 {
    let n = sorted.len() as f64;
    if n < 2.0 {
        return 1.0;
    }
    let m: f64 = sorted.iter().sum::<f64>() / n;
    let var = sorted.iter().map(|v| (v - m) * (v - m)).sum::<f64>() / (n - 1.0);
    let sd = var.sqrt();
    let iqr = quantile(sorted, 0.75) - quantile(sorted, 0.25);
    let a = if iqr > 1e-12 { iqr / 1.349 } else { sd };
    (0.9 * a * n.powf(-0.2)).max(1e-6)
}

/// 把一组值按 `bins` 分箱统计频数。
fn histogram(values: &[f64], lo: f64, hi: f64, bins: usize) -> Vec<usize> {
    let mut out = vec![0usize; bins.max(1)];
    if hi <= lo || bins == 0 {
        return out;
    }
    for &v in values {
        let t = ((v - lo) / (hi - lo)).clamp(0.0, 0.999999);
        out[(t * bins as f64) as usize] += 1;
    }
    out
}

/// Kaplan-Meier 阶梯。
///
/// 现有数据层只有「数值」没有事件指示器，因此把每个值视为一次**事件**；
/// `surv_censor` 只控制是否画标记点。真要做删失需要在数据层加事件列。
fn km(sorted: &[f64]) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = vec![(0.0, 1.0)];
    let n = sorted.len();
    let mut s = 1.0f64;
    for (i, &t) in sorted.iter().enumerate() {
        let at_risk = (n - i) as f64;
        s *= 1.0 - 1.0 / at_risk;
        out.push((t, s));
    }
    out
}

/// ⑧ 图类型专属：箱线 / 小提琴 / 直方 / 饼环 / 热图 / 森林 / 生存。
///
/// 这些类型各有自己的坐标含义，所以不复用 `y_of`，而是在绘图区矩形内
/// 各自映射 —— 混用会让直方图的频数被当成数值画到 0..100 的轴上。
// ── 路径生成：阶梯 / 平滑 ──────────────────────────────────────────────────

/// 阶梯路径：每一段拆成「先横后竖」两段，得到阶梯状的连线。
pub fn step_path(pts: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let mut out: Vec<(i32, i32)> = Vec::with_capacity(pts.len() * 2);
    if pts.is_empty() {
        return out;
    }
    out.push(pts[0]);
    for w in pts.windows(2) {
        // 先横到下一个点的 x，再竖到它的 y —— 这是「右连续」阶梯
        out.push((w[1].0, w[0].1));
        out.push(w[1]);
    }
    out
}

/// Catmull-Rom 平滑：在每两个原始点之间插 `seg` 个采样点。
///
/// 用 Catmull-Rom 而不是三次 Bezier，是因为它**必定经过所有原始点** ——
/// 折线图经过数据点是硬要求，Bezier 做不到（它只经过首尾）。
pub fn smooth_path(pts: &[(i32, i32)], seg: usize) -> Vec<(i32, i32)> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut out: Vec<(i32, i32)> = Vec::with_capacity(pts.len() * seg);
    let n = pts.len();
    for i in 0..n - 1 {
        let p0 = pts[if i == 0 { 0 } else { i - 1 }];
        let p1 = pts[i];
        let p2 = pts[i + 1];
        let p3 = pts[if i + 2 >= n { n - 1 } else { i + 2 }];
        for k in 0..seg {
            let t = k as f64 / seg as f64;
            let t2 = t * t;
            let t3 = t2 * t;
            let x = 0.5
                * ((2.0 * p1.0 as f64)
                    + (-p0.0 as f64 + p2.0 as f64) * t
                    + (2.0 * p0.0 as f64 - 5.0 * p1.0 as f64 + 4.0 * p2.0 as f64 - p3.0 as f64) * t2
                    + (-p0.0 as f64 + 3.0 * p1.0 as f64 - 3.0 * p2.0 as f64 + p3.0 as f64) * t3);
            let y = 0.5
                * ((2.0 * p1.1 as f64)
                    + (-p0.1 as f64 + p2.1 as f64) * t
                    + (2.0 * p0.1 as f64 - 5.0 * p1.1 as f64 + 4.0 * p2.1 as f64 - p3.1 as f64) * t2
                    + (-p0.1 as f64 + 3.0 * p1.1 as f64 - 3.0 * p2.1 as f64 + p3.1 as f64) * t3);
            out.push((x.round() as i32, y.round() as i32));
        }
    }
    out.push(pts[n - 1]);
    out
}

/// 取首组的折线路径（供 `AreaBetween` 作下界）。
fn first_path(view: &ChartView, g: &Geom, cur: usize) -> Option<Vec<(i32, i32)>> {
    if cur == 0 || view.groups.is_empty() {
        return None;
    }
    let grp = &view.groups[0];
    Some(
        grp.values
            .iter()
            .map(|v| (x_of(g, 0) as i32, y_of_grp(g, 0, *v) as i32))
            .collect(),
    )
}

/// 散点内芯色（供前后对比图的端点复用柱形散点的配色规则）。
fn fi_of(view: &ChartView, cm: usize, i: usize) -> Rgba {
    tone(view, colormap::fill_solid(cm, i))
}

/// 前后对比的配对方式：相邻两两组对（0-1, 2-3, …）。
///
/// 组数为奇数时最后一组自成一对，只画点不连线。
fn before_after_pairs(view: &ChartView) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let n = view.groups.len();
    let mut i = 0;
    while i + 1 < n {
        out.push((i, i + 1));
        i += 2;
    }
    if n % 2 == 1 {
        out.push((n - 1, n - 1));
    }
    out
}

// ── 柱形变体：横向 / 点图堆叠 ──────────────────────────────────────────────

/// 横向柱状图：类目沿纵轴排列，柱沿横轴延伸。
///
/// 这里不复用纵向柱的代码，而是独立算一遍 —— 纵向柱的 x/y 语义是写死的，
/// 强行「转置」会让误差棒与散点一起转错方向。
fn draw_bar_horizontal<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    view: &ChartView,
    g: &Geom,
    cm: usize,
    bw: u32,
    ew: u32,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    let n = view.groups.len().max(1);
    let row_h = (g.bottom - g.top) as f64 / n as f64;
    let bar_h = row_h * 0.6;
    let x_base = g.left;
    let vmax = view
        .groups
        .iter()
        .map(|x| x.mean)
        .fold(0.0f64, f64::max)
        .max(1e-9);
    for (i, grp) in view.groups.iter().enumerate() {
        let cy = g.top as f64 + row_h * (i as f64 + 0.5);
        let y0 = (cy - bar_h / 2.0) as i32;
        let y1 = (cy + bar_h / 2.0) as i32;
        let x_end = x_base + ((grp.mean / vmax) * (g.right - g.left) as f64) as i32;
        let f = tone(view, colormap::fill_of(cm, i));
        let e = tone(view, colormap::edge_of(cm, i));
        root.draw(&Rectangle::new([(x_base, y0), (x_end, y1)], area_fill(f)))?;
        // 三边：贴基线（左）那条不画
        let eb = stroke(e, bw);
        root.draw(&PathElement::new(vec![(x_base, y0), (x_end, y0)], eb.clone()))?;
        root.draw(&PathElement::new(vec![(x_base, y1), (x_end, y1)], eb.clone()))?;
        root.draw(&PathElement::new(vec![(x_end, y0), (x_end, y1)], eb))?;
        if view.show_error && grp.n > 1 {
            let ea = err_of(view, grp.mean, grp.sd, grp.n);
            let hi = ((grp.mean + ea) / vmax * (g.right - g.left) as f64) as i32 + x_base;
            let lo = (((grp.mean - ea).max(0.0)) / vmax * (g.right - g.left) as f64) as i32 + x_base;
            let cap = (bar_h * CHART_ERR_CAP_RATIO as f64 / 2.0) as i32;
            let es = stroke(e, ew);
            root.draw(&PathElement::new(vec![(hi, cy as i32), (lo, cy as i32)], es.clone()))?;
            root.draw(&PathElement::new(vec![(hi, cy as i32 - cap), (hi, cy as i32 + cap)], es.clone()))?;
            root.draw(&PathElement::new(vec![(lo, cy as i32 - cap), (lo, cy as i32 + cap)], es))?;
        }
    }
    Ok(())
}

/// 点图（Wilkinson dot plot）：每个观测值一个点，相同分箱内的点纵向堆叠。
///
/// 与「柱形散点」的区别：柱形散点的 y 是真实数值、x 是抖动；
/// 点图的 y 由**堆叠序号**决定，所以点数直接可读 —— 这是点图的语义。
fn draw_dot_stack<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    view: &ChartView,
    g: &Geom,
    cm: usize,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    let sc = view.scale.max(0.1);
    let r = (view.symbol_size.max(0.5) * sc).max(1.5);
    let step = r * 2.0;
    let cols = if view.kind == ChartKind::PercentageDot {
        view.waffle_cols.max(1)
    } else {
        1
    };
    for (i, grp) in view.groups.iter().enumerate() {
        let e = tone(view, colormap::edge_of(cm, i));
        let fi = tone(view, colormap::fill_solid(cm, i));
        let (sc2, se2) = symbol_colors(view, cm, i, &e, &fi);
        let cx = x_of(g, i);
        let mut counts: Vec<usize> = vec![0; cols.max(1)];
        // 点图的 y 由堆叠序号决定，不用数值本身 —— 所以这里只要计数
        for _ in &grp.values {
            // 华夫图：按列轮转铺点；普通点图：单列堆叠
            let c = counts
                .iter()
                .enumerate()
                .min_by_key(|(_, c)| **c)
                .map(|(i, _)| i)
                .unwrap_or(0);
            let k = counts[c];
            counts[c] += 1;
            let ox = if cols > 1 {
                (c as f64 - (cols - 1) as f64 / 2.0) * step as f64
            } else {
                0.0
            };
            let px = (cx + ox) as i32;
            let py = (g.bottom as f64 - (k as f64 + 0.5) * step as f64) as i32;
            if py < g.top {
                continue;
            }
            let ring = (view.symbol_border.max(0.0) * sc).max(1.0) as i32;
            draw_shape(root, view, (px, py), r as i32 + ring, area_fill(se2))?;
            draw_shape(root, view, (px, py), r as i32, area_fill(with_alpha(sc2, view.symbol_alpha)))?;
        }
    }
    Ok(())
}

// ── 列联图：马赛克 / 卡方关联 ──────────────────────────────────────────────

/// 马赛克图：每组一个矩形，宽度 ∝ 该组占比，纵向铺满。
///
/// 卡方关联图在此之上叠加偏差阴影：观察值显著高于期望时加深。
fn draw_mosaic<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    view: &ChartView,
    g: &Geom,
    cm: usize,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    let total: f64 = view
        .groups
        .iter()
        .map(|x| if x.mean > 0.0 { x.mean } else { 0.0 })
        .sum();
    if total <= 0.0 {
        return Ok(());
    }
    let expect = total / view.groups.len().max(1) as f64;
    let span = (g.right - g.left) as f64;
    let mut x = g.left as f64;
    for (i, grp) in view.groups.iter().enumerate() {
        let v = if grp.mean > 0.0 { grp.mean } else { 0.0 };
        let w = span * (v / total);
        let f = tone(view, colormap::fill_of(cm, i));
        let e = tone(view, colormap::edge_of(cm, i));
        root.draw(&Rectangle::new(
            [(x as i32, g.top), ((x + w) as i32, g.bottom)],
            area_fill(f),
        ))?;
        let eb = stroke(e, 1);
        root.draw(&PathElement::new(
            vec![(x as i32, g.top), (x as i32, g.bottom)],
            eb.clone(),
        ))?;
        root.draw(&PathElement::new(
            vec![((x + w) as i32, g.top), ((x + w) as i32, g.bottom)],
            eb.clone(),
        ))?;
        root.draw(&PathElement::new(
            vec![(x as i32, g.top), ((x + w) as i32, g.top)],
            eb.clone(),
        ))?;
        root.draw(&PathElement::new(
            vec![(x as i32, g.bottom), ((x + w) as i32, g.bottom)],
            eb,
        ))?;
        // 卡方关联图：观察明显高于期望时叠一层斜线（偏差方向的可视化）
        if view.kind == ChartKind::ChiSquareAssoc && expect > 0.0 {
            let dev = (v - expect) / expect;
            if dev.abs() > 0.10 {
                hatch(
                    root,
                    view,
                    x as i32,
                    (x + w) as i32,
                    g.top,
                    g.bottom,
                    if dev > 0.0 { 1 } else { 2 },
                    stroke(e, 1),
                )?;
            }
        }
        x += w;
    }
    Ok(())
}

// ── 分面：小倍数网格 ───────────────────────────────────────────────────────

/// 分面图：每组占一个子图，子图内画一根简化的柱。
///
/// `facet_shared` 为真时所有子图用同一套轴范围（便于横向比较），
/// 为假时每个子图按自己的数据自适应（便于看组内形状）。
fn draw_facet<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    view: &ChartView,
    g: &Geom,
    cm: usize,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    let n = view.groups.len().max(1);
    // 网格列数：接近正方形排布
    let cols = (n as f64).sqrt().ceil() as usize;
    let rows = (n + cols - 1) / cols;
    let cw = (g.right - g.left) as f64 / cols as f64;
    let ch = (g.bottom - g.top) as f64 / rows as f64;
    let shared_max = view
        .groups
        .iter()
        .map(|x| x.mean)
        .fold(0.0f64, f64::max)
        .max(1e-9);
    for (i, grp) in view.groups.iter().enumerate() {
        let r = i / cols;
        let c = i % cols;
        let x0 = g.left as f64 + c as f64 * cw;
        let y0 = g.top as f64 + r as f64 * ch;
        // 子图内边距
        let pad = 6.0;
        let ix0 = (x0 + pad) as i32;
        let ix1 = (x0 + cw - pad) as i32;
        let iy0 = (y0 + pad) as i32;
        let iy1 = (y0 + ch - pad) as i32;
        // 子图边框
        let ink = rgb(chart_ink());
        root.draw(&PathElement::new(
            vec![(ix0, iy0), (ix1, iy0), (ix1, iy1), (ix0, iy1), (ix0, iy0)],
            stroke([200, 200, 200, 255], 1),
        ))?;
        // 子图内的柱
        let hi = if view.facet_shared { shared_max } else { grp.mean.max(1e-9) };
        let h = ((iy1 - iy0) as f64 * (grp.mean / hi).clamp(0.0, 1.0)) as i32;
        let f = tone(view, colormap::fill_of(cm, i));
        let e = tone(view, colormap::edge_of(cm, i));
        if h > 0 {
            root.draw(&Rectangle::new(
                [(ix0 + 2, iy1 - h), (ix1 - 2, iy1)],
                area_fill(f),
            ))?;
            let eb = stroke(e, 1);
            root.draw(&PathElement::new(
                vec![(ix0 + 2, iy1 - h), (ix0 + 2, iy1)],
                eb.clone(),
            ))?;
            root.draw(&PathElement::new(
                vec![(ix1 - 2, iy1 - h), (ix1 - 2, iy1)],
                eb.clone(),
            ))?;
            root.draw(&PathElement::new(vec![(ix0 + 2, iy1 - h), (ix1 - 2, iy1 - h)], eb))?;
        }
        // 子图标题（组名），画在子图顶部
        if !grp.name.is_empty() {
            let fs = (CHART_FONT as f64 * view.scale.max(0.5) as f64 * 0.8).max(7.0);
            root.draw(&Text::new(
                grp.name.clone(),
                (ix0 + 3, iy0 + 2),
                (font_of(&grp.name), fs).into_font().color(&ink),
            ))?;
        }
    }
    Ok(())
}

// ── 风险人数表 ─────────────────────────────────────────────────────────────

/// 风险人数表：沿时间轴在若干时间点下方标注「仍处风险中的人数」。
///
/// 生存分析的配套表格 —— 读者靠它判断尾部的估计是否还可靠
/// （人数太少时，曲线末端的波动没有意义）。
fn draw_number_at_risk<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    view: &ChartView,
    g: &Geom,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    let ink = rgb(chart_ink());
    let fs = (CHART_FONT as f64 * view.scale.max(0.5) as f64 * 0.85).max(7.0);
    // 表画在绘图区下方，行数 = 组数
    let row_h = (fs * 1.5) as i32;
    let mut y = g.bottom + row_h;
    for grp in view.groups.iter() {
        let n = grp.values.len();
        // 行首是组名，后面是各时间点的在险人数（递减）
        let label = format!("{}", grp.name);
        root.draw(&Text::new(
            label,
            (g.left, y),
            (font_of(&grp.name), fs).into_font().color(&ink),
        ))?;
        let cols = 5usize;
        for c in 0..cols {
            let at_risk = (n as f64 * (1.0 - c as f64 / cols as f64)).round() as usize;
            let x = g.left + ((c + 1) as i32) * ((g.right - g.left) / (cols as i32 + 1));
            let s = format!("{}", at_risk);
            let wpx = text_w(&s, fs) as i32;
            root.draw(&Text::new(
                s,
                (x - wpx / 2, y),
                (font_of("0"), fs).into_font().color(&ink),
            ))?;
        }
        y += row_h;
    }
    Ok(())
}

fn draw_kind_specific<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    view: &ChartView,
    g: &Geom,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    let sc = view.scale.max(0.1);
    let fs = CHART_FONT as f64 * sc as f64;
    // 图表专用墨色：不受 UI 深浅模式影响（白底画布上必须恒为深色）
    let ink = rgb(chart_ink());
    let lwm = view.line_width.clamp(0.25, 6.0);
    let bw = (CHART_BAR_BORDER_PX as f32 * sc * lwm).max(1.0) as u32;
    let ew = (view.err_width.max(0.5) * sc * lwm).max(1.0) as u32;
    let pxw = (g.right - g.left) as f64;
    let pxh = (g.bottom - g.top) as f64;
    if pxw <= 4.0 || pxh <= 4.0 {
        return Ok(());
    }
    let cm = view.colormap;

    match view.kind.family() {
        // ── 箱线图：Y 轴仍是数值，复用 y_of ──────────────────────────────
        Family::Box => {
            for (i, grp) in view.groups.iter().enumerate() {
                let mut vs: Vec<f64> = grp.values.clone();
                vs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                if vs.is_empty() {
                    continue;
                }
                let (q1, q2, q3) = (
                    quantile(&vs, 0.25),
                    quantile(&vs, 0.5),
                    quantile(&vs, 0.75),
                );
                let iqr = q3 - q1;
                // Tukey 须：超出 1.5×IQR 的点算离群值，单独画点
                let lo_f = q1 - 1.5 * iqr;
                let hi_f = q3 + 1.5 * iqr;
                let lo_w = vs.iter().copied().find(|v| *v >= lo_f).unwrap_or(q1);
                let hi_w = vs.iter().rev().copied().find(|v| *v <= hi_f).unwrap_or(q3);
                let e = tone(view, colormap::edge_of(cm, i));
                let f = tone(view, colormap::fill_of(cm, i));
                let cx = x_of(g, i);
                let half = (bar_half(&g, view) * g.slot).max(2.0);
                let (x0, x1) = (cx - half, cx + half);
                let yq1 = y_of(g, q1) as i32;
                let yq3 = y_of(g, q3) as i32;
                // 箱体
                root.draw(&Rectangle::new([(x0 as i32, yq3), (x1 as i32, yq1)], area_fill(f)))?;
                let eb = stroke(e, bw);
                for seg in [
                    ((x0 as i32, yq3), (x1 as i32, yq3)),
                    ((x1 as i32, yq3), (x1 as i32, yq1)),
                    ((x1 as i32, yq1), (x0 as i32, yq1)),
                    ((x0 as i32, yq1), (x0 as i32, yq3)),
                ] {
                    root.draw(&PathElement::new(vec![seg.0, seg.1], eb.clone()))?;
                }
                // 中位数线加粗一点，便于读出
                let yq2 = y_of(g, q2) as i32;
                root.draw(&PathElement::new(
                    vec![(x0 as i32, yq2), (x1 as i32, yq2)],
                    stroke(e, (bw * 2).max(2)),
                ))?;
                // 须：`box_style == 1` 时只画盒不画须
                if view.box_style == 0 {
                    root.draw(&PathElement::new(
                        vec![(cx as i32, yq1), (cx as i32, y_of(g, lo_w) as i32)],
                        eb.clone(),
                    ))?;
                    root.draw(&PathElement::new(
                        vec![(cx as i32, yq3), (cx as i32, y_of(g, hi_w) as i32)],
                        eb.clone(),
                    ))?;
                    let cap = (half * 0.4) as i32;
                    for yy in [y_of(g, lo_w) as i32, y_of(g, hi_w) as i32] {
                        root.draw(&PathElement::new(
                            vec![(cx as i32 - cap, yy), (cx as i32 + cap, yy)],
                            eb.clone(),
                        ))?;
                    }
                }
                // 离群值：惯例是**空心**圆点。实心会被误读成「这里数据密集」，
                // 而它其实只是单个远离主体的点。
                let oring = (view.symbol_size.max(0.5) * sc as f32).max(2.0) as i32;
                for v in &vs {
                    if view.box_style == 0 && (*v < lo_f || *v > hi_f) {
                        let p = (cx as i32, y_of(g, *v) as i32);
                        root.draw(&Circle::new(p, oring, area_fill([255, 255, 255, 255])))?;
                        root.draw(&Circle::new(p, oring, stroke(e, bw.max(1))))?;
                    }
                }
            }
        }

        // ── 小提琴：KDE 轮廓左右对称 ────────────────────────────────────
        Family::Violin => {
            // 先在所有组上取统一的密度上限，否则各组轮廓不可比
            let grids: Vec<Vec<f64>> = view
                .groups
                .iter()
                .map(|grp| {
                    let mut vs: Vec<f64> = grp.values.clone();
                    vs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                    if vs.is_empty() {
                        return Vec::new();
                    }
                    let n = 48;
                    let lo = vs[0];
                    let hi = vs[vs.len() - 1];
                    let span = (hi - lo).max(1e-9);
                    (0..n)
                        .map(|k| lo + span * (k as f64 / (n - 1) as f64))
                        .collect()
                })
                .collect();
            let dmax = view
                .groups
                .iter()
                .zip(grids.iter())
                .filter_map(|(grp, grid)| {
                    if grp.values.is_empty() || grid.is_empty() {
                        return None;
                    }
                    let mut vs: Vec<f64> = grp.values.clone();
                    vs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                    Some(kde(&vs, grid, kde_bw(&vs)).into_iter().fold(0.0f64, f64::max))
                })
                .fold(0.0f64, f64::max)
                .max(1e-12);

            for (i, grp) in view.groups.iter().enumerate() {
                if grp.values.is_empty() || grids[i].is_empty() {
                    continue;
                }
                let mut vs: Vec<f64> = grp.values.clone();
                vs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let dens = kde(&vs, &grids[i], kde_bw(&vs));
                let e = tone(view, colormap::edge_of(cm, i));
                let f = tone(view, colormap::fill_of(cm, i));
                let cx = x_of(g, i);
                let half = bar_half(&g, view) * g.slot;
                let mut right: Vec<(i32, i32)> = Vec::new();
                let mut left: Vec<(i32, i32)> = Vec::new();
                for (k, x) in grids[i].iter().enumerate() {
                    let w = (dens[k] / dmax) * half;
                    let y = y_of(g, *x) as i32;
                    right.push(((cx + w) as i32, y));
                    left.push(((cx - w) as i32, y));
                }
                let mut poly = right.clone();
                for p in left.iter().rev() {
                    poly.push(*p);
                }
                root.draw(&Polygon::new(poly.clone(), area_fill(f)))?;
                root.draw(&PathElement::new(poly, stroke(e, bw)))?;
                // 中位数线
                let q2 = quantile(&vs, 0.5);
                root.draw(&PathElement::new(
                    vec![
                        ((cx - half * 0.5) as i32, y_of(g, q2) as i32),
                        ((cx + half * 0.5) as i32, y_of(g, q2) as i32),
                    ],
                    stroke([40, 40, 40, 255], (bw * 2).max(2)),
                ))?;
            }
        }

        // ── 直方图：Y 轴是**频数**，不是数值 ────────────────────────────
        Family::Histogram => {
            let Some((_lo, _hi, nb, counts, _cmax)) = hist_bins(view) else {
                return Ok(());
            };
            let ng = view.groups.len().max(1);
            // 关键：X 轴是**值域**，先按 nb 分成箱；每个箱内 n 组并排。
            // 之前按「每组占 pxw/n 的一段、段内再分箱」来画，等于把值域
            // 重复画了 n 遍，分箱边界标签会完全对不上。
            let cellw = pxw / nb as f64;
            let bwid = cellw / ng as f64;
            for k in 0..nb {
                for (i, _grp) in view.groups.iter().enumerate() {
                    let c = counts[i][k];
                    if c == 0 {
                        continue;
                    }
                    let e = tone(view, colormap::edge_of(cm, i));
                    let f = tone(view, colormap::fill_of(cm, i));
                    let x0 = (g.left as f64 + cellw * k as f64 + bwid * i as f64) as i32;
                    let x1 = (x0 as f64 + bwid) as i32;
                    // 用 y_of 而不是手工比例：这样柱高与左侧频数刻度同源，
                    // 不会出现「柱顶到框了、刻度才一半」。
                    let ytop = y_of(&g, c as f64) as i32;
                    root.draw(&Rectangle::new([(x0, ytop), (x1, g.bottom)], area_fill(f)))?;
                    root.draw(&PathElement::new(
                        vec![(x0, ytop), (x0, g.bottom), (x1, g.bottom), (x1, ytop)],
                        stroke(e, bw),
                    ))?;
                }
            }
        }

        // ── 饼图 / 环形图 ────────────────────────────────────────────────
        Family::Pie => {
            let total: f64 = view.groups.iter().map(|gr| gr.mean.max(0.0)).sum();
            if total <= 0.0 {
                return Ok(());
            }
            let cxp = (g.left + g.right) as f64 / 2.0;
            let cyp = (g.top + g.bottom) as f64 / 2.0;
            let rad = pxw.min(pxh) / 2.0 * 0.9;
            // `donut()` 旗标：环形图即便用户没调内径，也要有洞 —— 否则
            // Donut 画出来和 Pie 一模一样，等于这个图型不存在。
            let pi = if view.kind.donut() {
                view.pie_inner.clamp(0.35, 0.9)
            } else {
                view.pie_inner.clamp(0.0, 0.9)
            };
            let inner = rad * pi as f64;
            let mut a0 = view.pie_start.clamp(0.0, 360.0) as f64 - 90.0; // 12 点起
            let ar = |a: f64| a.to_radians();
            for (i, grp) in view.groups.iter().enumerate() {
                let frac = grp.mean.max(0.0) / total;
                let a1 = a0 + frac * 360.0;
                let f = tone(view, colormap::fill_of(cm, i));
                let e = tone(view, colormap::edge_of(cm, i));
                // 扇形：圆心 + 外弧 + 内弧（环形时反向走内弧）
                let steps = ((a1 - a0) / 4.0).max(2.0) as i32;
                let mut poly: Vec<(i32, i32)> = vec![(
                    (cxp + inner * ar(a0).cos()) as i32,
                    (cyp + inner * ar(a0).sin()) as i32,
                )];
                for k in 0..=steps {
                    let a = a0 + (a1 - a0) * (k as f64 / steps as f64);
                    poly.push((
                        (cxp + rad * ar(a).cos()) as i32,
                        (cyp + rad * ar(a).sin()) as i32,
                    ));
                }
                if inner > 0.5 {
                    for k in (0..=steps).rev() {
                        let a = a0 + (a1 - a0) * (k as f64 / steps as f64);
                        poly.push((
                            (cxp + inner * ar(a).cos()) as i32,
                            (cyp + inner * ar(a).sin()) as i32,
                        ));
                    }
                }
                root.draw(&Polygon::new(poly.clone(), area_fill(f)))?;
                root.draw(&PathElement::new(poly, stroke(e, bw)))?;
                a0 = a1;
            }
        }

        // ── 热图：组 × 分箱 ──────────────────────────────────────────────
        Family::Heatmap => {
            let all: Vec<f64> = view.groups.iter().flat_map(|gr| gr.values.clone()).collect();
            if all.is_empty() {
                return Ok(());
            }
            let lo = all.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = all.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let nb = view.bins.clamp(1, 64);
            let counts: Vec<Vec<usize>> = view
                .groups
                .iter()
                .map(|gr| histogram(&gr.values, lo, hi, nb))
                .collect();
            let (vlo, vhi) = if view.heat_auto {
                let mx = counts
                    .iter()
                    .flat_map(|c| c.iter().copied())
                    .max()
                    .unwrap_or(1)
                    .max(1) as f64;
                (0.0, mx)
            } else {
                (view.heat_lo.min(view.heat_hi), view.heat_lo.max(view.heat_hi))
            };
            let span = (vhi - vlo).max(1e-9);
            let rows = view.groups.len().max(1);
            let cw = pxw / nb as f64;
            let ch = pxh / rows as f64;
            for (r, row) in counts.iter().enumerate() {
                for (c, v) in row.iter().enumerate() {
                    let t = ((*v as f64 - vlo) / span).clamp(0.0, 1.0);
                    // 色阶取**当前**配色方案的首色 → 末色线性插值。
                    // 色数必须按当前 cm 查；写死成 COLORMAPS[0] 会让
                    // 8 色方案的末端色取到第 6 色（甚至越界回退）。
                    let ncol = colormap::COLORMAPS
                        .get(cm)
                        .map(|c| c.colors.len())
                        .unwrap_or(1)
                        .max(1);
                    let a = tone(view, colormap::edge_of(cm, 0));
                    let b = tone(view, colormap::edge_of(cm, ncol - 1));
                    let col = [
                        (a[0] as f64 + (b[0] as f64 - a[0] as f64) * t) as u8,
                        (a[1] as f64 + (b[1] as f64 - a[1] as f64) * t) as u8,
                        (a[2] as f64 + (b[2] as f64 - a[2] as f64) * t) as u8,
                        255,
                    ];
                    let x0 = (g.left as f64 + cw * c as f64) as i32;
                    let y0 = (g.top as f64 + ch * r as f64) as i32;
                    root.draw(&Rectangle::new(
                        [(x0, y0), ((x0 + cw as i32), (y0 + ch as i32))],
                        area_fill(col),
                    ))?;
                }
                // 行标签：热图属于 YMode::None，主流程不会画组名，得自己标
                let ry = (g.top as f64 + ch * (r as f64 + 0.5)) as i32;
                root.draw(&Text::new(
                    view.groups[r].name.clone(),
                    (
                        g.left - 6 - text_w(&view.groups[r].name, fs) as i32,
                        ry - (fs * 0.55) as i32,
                    ),
                    (font_of(&view.groups[r].name), fs).into_font().color(&ink),
                ))?;
            }
        }

        // ── 森林图：X = 效应量，Y = 组 ───────────────────────────────────
        Family::Forest => {
            if view.groups.is_empty() {
                return Ok(());
            }
            // 逆方差权重 w = 1/SE²（SE = SD/√n）。
            // 方块**面积** ∝ 权重，所以边长 ∝ √权重 —— 精确的研究方块更大。
            let se_of = |gr: &GroupStat| -> f64 {
                if gr.n < 2 {
                    return f64::INFINITY;
                }
                let s = gr.sd / (gr.n as f64).sqrt();
                if !(s > 0.0) {
                    f64::INFINITY
                } else {
                    s
                }
            };
            let ws: Vec<f64> = view
                .groups
                .iter()
                .map(|gr| {
                    let se = se_of(gr);
                    if se.is_finite() && se > 0.0 {
                        1.0 / (se * se)
                    } else {
                        0.0
                    }
                })
                .collect();
            let wmax = ws.iter().copied().fold(0.0f64, f64::max).max(1e-12);
            let wsum: f64 = ws.iter().sum();

            // 行布局：每组一行，末尾再加一行放汇总菱形
            let nrow = view.groups.len() + 1;
            let rh = pxh / nrow as f64;
            // 横轴范围由 geom() 按 YMode::Effect 算好，这里直接复用，
            // 保证参考线、CI 端点、刻度三者同源。
            let span = (g.y_max - g.y_min).max(1e-12);
            let fx = |v: f64| g.left as f64 + (v - g.y_min) / span * pxw;

            // 无效线（no-effect line）：虚线用短线段近似
            let rx = fx(view.forest_ref) as i32;
            let mut y = g.top;
            let dash = (10.0 * sc) as i32;
            while y < g.bottom {
                root.draw(&PathElement::new(
                    vec![(rx, y), (rx, (y + dash / 2).min(g.bottom))],
                    stroke([150, 150, 150, 255], 1),
                ))?;
                y += dash;
            }

            for (i, grp) in view.groups.iter().enumerate() {
                let e = ci95(grp);
                let cy = (g.top as f64 + rh * (i as f64 + 0.5)) as i32;
                let col = tone(view, colormap::edge_of(cm, i));
                let (x0, x1) = (fx(grp.mean - e) as i32, fx(grp.mean + e) as i32);
                let xm = fx(grp.mean) as i32;
                root.draw(&PathElement::new(vec![(x0, cy), (x1, cy)], stroke(col, bw)))?;
                // CI 两端的短竖线（端帽）
                for xx in [x0, x1] {
                    root.draw(&PathElement::new(
                        vec![(xx, cy - (rh * 0.18) as i32), (xx, cy + (rh * 0.18) as i32)],
                        stroke(col, bw),
                    ))?;
                }
                let side = (rh * 0.30 * (ws[i] / wmax).sqrt()).max(2.0) as i32;
                root.draw(&Rectangle::new(
                    [(xm - side, cy - side), (xm + side, cy + side)],
                    area_fill(col),
                ))?;
                // 左侧组名
                root.draw(&Text::new(
                    grp.name.clone(),
                    (g.left - 6 - text_w(&grp.name, fs) as i32, cy - (fs * 0.55) as i32),
                    (font_of(&grp.name), fs).into_font().color(&ink),
                ))?;
            }

            // 汇总菱形：森林图的标志。中心 = 合并效应量，宽度 = 其 95% CI。
            if wsum > 0.0 {
                let pooled = view
                    .groups
                    .iter()
                    .zip(ws.iter())
                    .map(|(gr, w)| w * gr.mean)
                    .sum::<f64>()
                    / wsum;
                let pse = (1.0 / wsum).sqrt();
                let (plo, phi) = (pooled - 1.96 * pse, pooled + 1.96 * pse);
                let cy = (g.top as f64 + rh * (view.groups.len() as f64 + 0.5)) as i32;
                let dh = (rh * 0.34) as i32;
                let dm = [
                    (fx(plo) as i32, cy),
                    (fx(pooled) as i32, cy - dh),
                    (fx(phi) as i32, cy),
                    (fx(pooled) as i32, cy + dh),
                ];
                root.draw(&Polygon::new(dm.to_vec(), area_fill([60, 60, 60, 255])))?;
                root.draw(&PathElement::new(
                    vec![dm[0], dm[1], dm[2], dm[3], dm[0]],
                    stroke([20, 20, 20, 255], 1),
                ))?;
                root.draw(&Text::new(
                    "Summary".to_string(),
                    (g.left - 6 - text_w("Summary", fs) as i32, cy - (fs * 0.55) as i32),
                    (font_of("Summary"), fs).into_font().color(&ink),
                ))?;
            }

            // 横轴（效应量）刻度：森林图的数值轴在横向，必须自己画
            let axis_tick = stroke([90, 90, 90, 255], 1);
            for k in 0..=4 {
                let v = g.y_min + span * (k as f64 / 4.0);
                let lbl = format!("{:.2}", v);
                let xx = fx(v) as i32;
                root.draw(&PathElement::new(
                    vec![(xx, g.bottom), (xx, g.bottom + 4)],
                    axis_tick.clone(),
                ))?;
                root.draw(&Text::new(
                    lbl.clone(),
                    (xx - text_w(&lbl, fs) as i32 / 2, g.bottom + (fs * 0.45) as i32),
                    (font_of(&lbl), fs).into_font().color(&ink),
                ))?;
            }
        }

        // ── 生存曲线：Kaplan-Meier 阶梯 ──────────────────────────────────
        Family::Survival => {
            let all: Vec<f64> = view.groups.iter().flat_map(|gr| gr.values.clone()).collect();
            if all.is_empty() {
                return Ok(());
            }
            let tmax = all.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            if tmax <= 0.0 {
                return Ok(());
            }
            let fx = |t: f64| g.left as f64 + (t / tmax) * pxw;
            for (i, grp) in view.groups.iter().enumerate() {
                let mut vs: Vec<f64> = grp.values.clone();
                vs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let curve = km(&vs);
                let col = tone(view, colormap::edge_of(cm, i));
                // 阶梯：每个事件点先水平走到该时刻、再竖直下降。
                // 所以同一个 x 要插两次（先保持上一高度，再落到新高度）。
                let mut pts: Vec<(i32, i32)> = Vec::new();
                for (k, (t, sv)) in curve.iter().enumerate() {
                    let x = fx(*t) as i32;
                    let y = y_of(g, *sv) as i32;
                    if k > 0 {
                        pts.push((x, pts[pts.len() - 1].1));
                    }
                    pts.push((x, y));
                }
                root.draw(&PathElement::new(pts.clone(), stroke(col, bw)))?;
                // 删失标记：KM 图的惯例是在曲线上画短竖线（tick）。
                // 数据层没有事件指示器，这里对所有观测点都画 ——
                // 语义上等价于「每个点都是一次观测」。
                if view.surv_censor {
                    let tick = (4.0 * sc).max(2.0) as i32;
                    for (t, sv) in &curve {
                        let x = fx(*t) as i32;
                        let y = y_of(g, *sv) as i32;
                        root.draw(&PathElement::new(
                            vec![(x, y - tick), (x, y + tick)],
                            stroke(col, bw.max(1)),
                        ))?;
                    }
                }
            }
        }

        // ── 前后对比：配对连线 ──────────────────────────────────────────
        // 每两组配成一对，同对之间连一条线，线的斜率方向即「升高 / 降低」。
        // 这是 Before-After 图的核心语义，比并排两根柱子直观得多。
        Family::BeforeAfter => {
            let pairs = before_after_pairs(view);
            for (ai, bi) in pairs {
                if ai >= view.groups.len() || bi >= view.groups.len() {
                    continue;
                }
                let a = &view.groups[ai];
                let b = &view.groups[bi];
                let xa = x_of(g, ai) as i32;
                let xb = x_of(g, bi) as i32;
                // 连线：用组色（深色边框色）保证在白底上看得清
                let col = tone(view, colormap::edge_of(cm, ai));
                let ew = (view.err_width.max(0.5) * sc * lwm).max(1.0) as u32;
                let lw = stroke(col, ew);
                for (k, av) in a.values.iter().enumerate() {
                    let bv = match b.values.get(k) {
                        Some(v) => *v,
                        None => continue,
                    };
                    let ya = y_of_grp(g, ai, *av) as i32;
                    let yb = y_of_grp(g, bi, bv) as i32;
                    root.draw(&PathElement::new(vec![(xa, ya), (xb, yb)], lw.clone()))?;
                    // 两端的点：与柱形散点同一套双层画法
                    let (sc, se) = symbol_colors(view, cm, ai, &col, &fi_of(view, cm, ai));
                    let r = (view.symbol_size.max(0.5) * view.scale.max(0.5)) as i32;
                    let ring = (view.symbol_border.max(0.0) * view.scale.max(0.5)).max(1.0) as i32;
                    draw_shape(root, view, (xa, ya), r + ring, area_fill(se))?;
                    draw_shape(root, view, (xa, ya), r, area_fill(with_alpha(sc, view.symbol_alpha)))?;
                    let (sc2, se2) = symbol_colors(view, cm, bi, &col, &fi_of(view, cm, bi));
                    draw_shape(root, view, (xb, yb), r + ring, area_fill(se2))?;
                    draw_shape(root, view, (xb, yb), r, area_fill(with_alpha(sc2, view.symbol_alpha)))?;
                }
            }
        }

        // ── 列联图：马赛克 / 卡方关联 ───────────────────────────────────
        // 马赛克图把每组画成一个矩形，面积 ∝ 该组均值，横向铺满、纵向居中堆叠。
        // 卡方关联图在此之上叠加「观察 vs 期望」的偏差阴影。
        Family::Contingency => {
            draw_mosaic(root, view, g, cm)?;
        }

        // ── 分面：小倍数网格 ────────────────────────────────────────────
        // 每组占一个子图，各自画一条简化的柱。共享轴时所有子图用同一范围。
        Family::Facet => {
            draw_facet(root, view, g, cm)?;
        }

        // ── 风险人数表 ──────────────────────────────────────────────────
        // 生存分析配套：在每个时间点下方标注「仍处风险中的人数」。
        Family::NumberAtRisk => {
            draw_number_at_risk(root, view, g)?;
        }

        // ── 哑铃图：两点一线，两端点 + 连线 ────────────────────────────
        Family::Dumbbell => {
            for (i, grp) in view.groups.iter().enumerate() {
                if grp.values.len() < 2 {
                    continue;
                }
                let cx = x_of(g, i);
                // 低端取最小值、高端取最大值，线段表示跨度
                let lo = grp.values.iter().copied().fold(f64::INFINITY, f64::min);
                let hi = grp.values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                let y1 = y_of(g, lo);
                let y2 = y_of(g, hi);
                let c = tone(view, colormap::color_of(cm, i));
                root.draw(&PathElement::new(
                    vec![(cx as i32, y1 as i32), (cx as i32, y2 as i32)],
                    stroke(c, (ew as f32 * 1.5).max(1.0) as u32),
                ))?;
                for y in [y1, y2] {
                    root.draw(&Circle::new(
                        (cx as i32, y as i32),
                        (5.0 * sc).max(2.0) as i32,
                        area_fill(tone(view, colormap::fill_of(cm, i))),
                    ))?;
                    root.draw(&Circle::new(
                        (cx as i32, y as i32),
                        (5.0 * sc).max(2.0) as i32,
                        stroke(c, bw),
                    ))?;
                }
            }
        }

        // ── 山脊图：每组一条密度曲线，纵向叠放且轻微重叠 ────────────────
        Family::Ridgeline => {
            let n = view.groups.len().max(1);
            let band = pxh / n as f64;
            // 全组统一密度上限，否则各组高度不可比 —— 山脊图的价值就在组间比较
            let mut dmax = 1e-9f64;
            let mut curves: Vec<Vec<(f64, f64)>> = Vec::new();
            for grp in &view.groups {
                let c = density_curve(&grp.values, 48);
                for (_, d) in &c {
                    dmax = dmax.max(*d);
                }
                curves.push(c);
            }
            for (i, grp) in view.groups.iter().enumerate() {
                // 山脊图自上而下：第 0 组在最上面
                let base = g.top as f64 + (i as f64 + 1.0) * band;
                let amp = band * 1.45; // 相邻带轻微重叠，形成"山脊"层次
                let c = &curves[i];
                if c.len() < 2 {
                    continue;
                }
                let mut poly: Vec<(i32, i32)> = Vec::new();
                for (x, d) in c {
                    let px = g.left as f64 + x * pxw;
                    let py = base - (d / dmax) * amp;
                    poly.push((px as i32, py as i32));
                }
                poly.push((g.right, base as i32));
                poly.push((g.left, base as i32));
                let f = tone(view, colormap::fill_of(cm, i));
                let e = tone(view, colormap::edge_of(cm, i));
                root.draw(&Polygon::new(poly.clone(), area_fill(f)))?;
                root.draw(&PathElement::new(poly, stroke(e, bw)))?;
                // 组名标在带的左端
                let fd = (crate::fonts::font_for(&grp.name), fs).into_font();
                root.draw(&Text::new(
                    grp.name.clone(),
                    (g.left + 4, (base - band * 0.35) as i32),
                    fd.color(&rgb(chart_ink())),
                ))?;
            }
        }

        // ── 雷达图：多变量极坐标，每组一条闭合折线 ──────────────────────
        Family::Radar => {
            let n = view.groups.len().max(3);
            let cx = (g.left + g.right) as f64 / 2.0;
            let cy = (g.top + g.bottom) as f64 / 2.0;
            let rad = pxw.min(pxh) / 2.0 * 0.85;
            // 网格：4 圈 + n 条辐条
            for k in 1..=4 {
                let rr = rad * k as f64 / 4.0;
                let mut ring = Vec::new();
                for j in 0..=n {
                    let a = -std::f64::consts::FRAC_PI_2
                        + j as f64 / n as f64 * std::f64::consts::TAU;
                    ring.push(((cx + rr * a.cos()) as i32, (cy + rr * a.sin()) as i32));
                }
                root.draw(&PathElement::new(ring, stroke([200, 202, 208, 255], 1)))?;
            }
            for j in 0..n {
                let a =
                    -std::f64::consts::FRAC_PI_2 + j as f64 / n as f64 * std::f64::consts::TAU;
                root.draw(&PathElement::new(
                    vec![
                        (cx as i32, cy as i32),
                        ((cx + rad * a.cos()) as i32, (cy + rad * a.sin()) as i32),
                    ],
                    stroke([200, 202, 208, 255], 1),
                ))?;
            }
            // 归一化：各组按整体最大值缩到同一半径尺度
            let vmax = view
                .groups
                .iter()
                .map(|gr| gr.mean.abs())
                .fold(1e-9f64, f64::max);
            for (i, grp) in view.groups.iter().enumerate() {
                // 单组一根辐条一个值：用该组的分位数铺开
                let vs: Vec<f64> = if grp.values.len() >= 2 {
                    grp.values.clone()
                } else {
                    vec![grp.mean]
                };
                let mut poly = Vec::new();
                for (j, v) in vs.iter().enumerate() {
                    let a = -std::f64::consts::FRAC_PI_2
                        + j as f64 / vs.len() as f64 * std::f64::consts::TAU;
                    let rr = rad * (v.abs() / vmax).clamp(0.0, 1.0);
                    poly.push(((cx + rr * a.cos()) as i32, (cy + rr * a.sin()) as i32));
                }
                let c = tone(view, colormap::color_of(cm, i));
                root.draw(&Polygon::new(
                    poly.clone(),
                    area_fill(tone(view, colormap::fill_of(cm, i))),
                ))?;
                root.draw(&PathElement::new(poly, stroke(c, bw)))?;
            }
        }

        // ── 华夫图：10×10 格子，每格代表 1% ────────────────────────────
        Family::Waffle => {
            let total: f64 = view.groups.iter().map(|gr| gr.mean.max(0.0)).sum();
            if total <= 0.0 {
                return Ok(());
            }
            let cols = 10usize;
            let rows = 10usize;
            let cw = pxw / cols as f64;
            let chh = pxh / rows as f64;
            let gap = 1.0 * sc as f64;
            let mut filled = 0usize;
            // 逐格填充，按各组占比连续分配 —— 视觉上是"堆叠的百分比"
            for (i, grp) in view.groups.iter().enumerate() {
                let need = ((grp.mean.max(0.0) / total) * 100.0).round() as usize;
                let c = tone(view, colormap::fill_of(cm, i));
                for _ in 0..need {
                    if filled >= rows * cols {
                        break;
                    }
                    // 自左下角起，逐列向上填
                    let cidx = filled / rows;
                    let ridx = rows - 1 - (filled % rows);
                    let x0 = g.left as f64 + cidx as f64 * cw + gap;
                    let y0 = g.top as f64 + ridx as f64 * chh + gap;
                    root.draw(&Rectangle::new(
                        [
                            (x0 as i32, y0 as i32),
                            ((x0 + cw - gap * 2.0) as i32, (y0 + chh - gap * 2.0) as i32),
                        ],
                        area_fill(c),
                    ))?;
                    filled += 1;
                }
            }
        }

        // ── 等高线：网格化 +  marching squares 简化版 ───────────────────
        Family::Contour => {
            // 以「组序号 × 值分箱」构造二维场
            let nb = view.bins.clamp(3, 24);
            let all: Vec<f64> = view.groups.iter().flat_map(|gr| gr.values.clone()).collect();
            if all.is_empty() {
                return Ok(());
            }
            let vlo = all.iter().copied().fold(f64::INFINITY, f64::min);
            let vhi = all.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let ng = view.groups.len().max(1);
            // 场：field[col][row] = 该组落入该值箱的频数
            let mut field = vec![vec![0.0f64; ng]; nb];
            for (gi, grp) in view.groups.iter().enumerate() {
                for v in &grp.values {
                    let k = (((*v - vlo) / (vhi - vlo).max(1e-9)) * nb as f64)
                        .floor()
                        .clamp(0.0, nb as f64 - 1.0) as usize;
                    field[k][gi] += 1.0;
                }
            }
            let fmax = field.iter().flat_map(|r| r.iter().copied()).fold(1e-9, f64::max);
            // 8 条等值线
            for li in 1..=8 {
                let level = fmax * li as f64 / 9.0;
                let c = colormap::color_of(cm, li % 8);
                // marching squares：逐格判断四角是否跨越等值面，跨则连中点
                for a in 0..nb.saturating_sub(1) {
                    for b in 0..ng.saturating_sub(1) {
                        let v = |da: usize, db: usize| field[a + da][b + db];
                        let x0 = g.left as f64 + a as f64 / (nb as f64 - 1.0).max(1.0) * pxw;
                        let x1 = g.left as f64 + (a + 1) as f64 / (nb as f64 - 1.0).max(1.0) * pxw;
                        let y0 = g.top as f64 + b as f64 / (ng as f64 - 1.0).max(1.0) * pxh;
                        let y1 = g.top as f64 + (b + 1) as f64 / (ng as f64 - 1.0).max(1.0) * pxh;
                        let mx = (x0 + x1) / 2.0;
                        let my = (y0 + y1) / 2.0;
                        // 四条边，跨等值面就在该边中点放一点；相邻点连成线段
                        let mut pts: Vec<(f64, f64)> = Vec::new();
                        if (v(0, 0) - level).signum() != (v(1, 0) - level).signum() {
                            pts.push((mx, y0));
                        }
                        if (v(1, 0) - level).signum() != (v(1, 1) - level).signum() {
                            pts.push((x1, my));
                        }
                        if (v(1, 1) - level).signum() != (v(0, 1) - level).signum() {
                            pts.push((mx, y1));
                        }
                        if (v(0, 1) - level).signum() != (v(0, 0) - level).signum() {
                            pts.push((x0, my));
                        }
                        for pk in pts.chunks(2) {
                            if pk.len() == 2 {
                                root.draw(&PathElement::new(
                                    vec![
                                        (pk[0].0 as i32, pk[0].1 as i32),
                                        (pk[1].0 as i32, pk[1].1 as i32),
                                    ],
                                    stroke(tone(view, c), bw),
                                ))?;
                            }
                        }
                    }
                }
            }
        }

        // Bar / Scatter / Line 在 draw_chart 主体里处理
        Family::Bar | Family::Scatter | Family::Line => {}
    }
    Ok(())
}


/// 残差视图：把分组数据转成「拟合值 × 残差」的散点。
///
/// 拟合值取组内均值，残差 = 原始值 − 组内均值 —— 这正是单因素 ANOVA
/// 的残差定义。转成散点后复用现有的散点渲染，不需要新写绘制代码。
///
/// 残差图关心的是**离散程度是否随拟合值变化**（异方差）与**是否正态**，
/// 所以横轴必须是拟合值而不是组序号：按组画的话看不出趋势。
fn residual_view(view: &ChartView) -> ChartView {
    let mut v = view.clone();
    let mut xs: Vec<f64> = Vec::new();
    let mut ys: Vec<f64> = Vec::new();
    for grp in &view.groups {
        for x in &grp.values {
            if x.is_finite() {
                xs.push(grp.mean);
                ys.push(*x - grp.mean);
            }
        }
    }
    if xs.is_empty() {
        return v;
    }
    // 每组转成一个「残差组」：值全是残差，位置按拟合值排序。
    //
    // 为什么按拟合值排序：残差图要看残差的离散程度是否**随拟合值增大**
    // 而变化（异方差）。若按原始组序排，横轴与拟合值无关，就看不出趋势。
    let mut pairs: Vec<(f64, Vec<f64>)> = view
        .groups
        .iter()
        .map(|g| (g.mean, g.values.iter().map(|x| *x - g.mean).collect()))
        .collect();
    pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    v.groups = pairs
        .into_iter()
        .enumerate()
        .map(|(_i, (fit, res))| {
            let n = res.len();
            let m = res.iter().sum::<f64>() / n.max(1) as f64;
            let var =
                res.iter().map(|y| (y - m) * (y - m)).sum::<f64>() / (n - 1).max(1) as f64;
            GroupStat {
                name: format!("{:.3}", fit),
                mean: m,
                sd: var.sqrt(),
                n,
                values: res,
            }
        })
        .collect();
    let _ = &mut xs;
    let _ = &ys;
    v.show_bracket = false;
    v
}

pub fn draw_chart<DB: DrawingBackend>(
    view: &ChartView,
    root: &DrawingArea<DB, Shift>,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    // ── 高级统计图：交给 plotters-statistical（见 advplot 模块说明）──────
    // 必须在 geom() 之前分派：这些图的轴语义（ECDF 的 Y 是概率、QQ 的轴是
    // 分位数）与主流程的 YMode 完全不同，走主流程会得到错误的刻度。
    // 残差图：转成散点视图后再走主流程（轴语义就是普通的数值×数值）
    if view.kind == crate::chartkind::ChartKind::Residual {
        let rv = residual_view(view);
        return draw_chart(&rv, root);
    }

    if crate::advplot::handles(view) {
        // 失败就什么都不画（调用方已按 Ok 处理），不回退手绘 ——
        // 手绘路径没有这两种图的实现，回退只会画出空白坐标系。
        let _ = crate::advplot::render(view, root);
        return Ok(());
    }

    let (w, h) = root.dim_in_pixel();
    let g = geom(view, w, h);
    if g.right <= g.left || g.bottom <= g.top {
        return Ok(());
    }
    let sc = view.scale.max(0.1);
    root.fill(&WHITE)?;

    // 图表专用墨色：不受 UI 深浅模式影响（白底画布上必须恒为深色）
    let ink = rgb(chart_ink());
    let fs = CHART_FONT as f64 * sc as f64;
    let title_fs = (CHART_FONT + 2.0) as f64 * sc as f64;
    // 全局线宽倍率：柱边框 / 误差棒 / 轴线 / 折线一起缩放
    let lwm = view.line_width.clamp(0.25, 6.0);
    let bw = (CHART_BAR_BORDER_PX as f32 * sc * lwm).max(1.0) as u32;
    let ew = (view.err_width.max(0.5) * sc * lwm).max(1.0) as u32;
    let tl = (CHART_TICK_LEN * sc) as i32;
    let axis = stroke([26, 26, 26, 255], (CHART_AXIS_PX as f32 * sc * lwm).max(1.0) as u32);

    // ── 1. 坐标框 ──────────────────────────────────────────────────────────
    // 析光默认是 **L 型框**（左轴 + 下轴），不是四面全框。
    // 这是「学术感」最容易被忽略的一处：四面框会显得像 Excel。
    match view.frame {
        FrameStyle::L => {
            root.draw(&PathElement::new(
                vec![(g.left, g.top), (g.left, g.bottom), (g.right, g.bottom)],
                axis.clone(),
            ))?;
        }
        FrameStyle::Box => {
            root.draw(&PathElement::new(
                vec![
                    (g.left, g.top),
                    (g.right, g.top),
                    (g.right, g.bottom),
                    (g.left, g.bottom),
                    (g.left, g.top),
                ],
                axis.clone(),
            ))?;
        }
    }

    // 右 Y 轴（双轴，文档 §2.6）：从 `right_from` 起的数据集走右轴刻度。
    // 启用时右侧留白已经在 geom 里加大，标签不会画到画布外。
    if view.right_axis && g.right_from_count > 0 {
        let r_axis = stroke([60, 64, 72, 255], (view.line_width * sc).max(0.5) as u32);
        root.draw(&PathElement::new(
            vec![(g.right, g.top), (g.right, g.bottom)],
            r_axis.clone(),
        ))?;
        for v in &g.right_ticks {
            let y = right_y_of(&g, *v) as i32;
            root.draw(&PathElement::new(
                vec![(g.right, y), (g.right + tl, y)],
                r_axis.clone(),
            ))?;
            let s = fmt_tick(*v, g.right_step);
            let rfd: FontDesc = (font_of(&s), fs).into_font();
            root.draw(&Text::new(s, (g.right + tl + 3, y - (fs * 0.55) as i32), rfd.color(&ink)))?;
        }
    }

    // 水平网格线：与刻度同高，颜色比轴线淡得多，只做读数辅助。
    // 线型可选实线 / 虚线 / 点线（文档 §2.7）。
    if view.grid_on {
        let gs = stroke([214, 219, 226, 255], 1);
        let dash = Dash::from_usize(view.grid_style);
        for v in &g.ticks {
            let y = y_of(&g, *v) as i32;
            for (a, b) in dash.segments((g.left as f64, y as f64), (g.right as f64, y as f64)) {
                root.draw(&PathElement::new(
                    vec![(a.0 as i32, a.1 as i32), (b.0 as i32, b.1 as i32)],
                    gs.clone(),
                ))?;
            }
        }
    }

    // 轴断裂标记：纵轴不从 0 开始时，提示读者「这里被截断了」，
    // 否则会被误读成基线就是 0（文档 §2.5）。
    if view.axis_break
        && matches!(g.y_mode, YMode::Value | YMode::Count)
        && g.y_min > 1e-9
        && !g.log
    {
        let bs = stroke([90, 96, 106, 255], (1.5 * view.scale.max(0.5)) as u32);
        for (a, b) in axis_break_segs(g.left as f64, g.bottom as f64, g.top as f64) {
            root.draw(&PathElement::new(
                vec![(a.0 as i32, a.1 as i32), (b.0 as i32, b.1 as i32)],
                bs.clone(),
            ))?;
        }
    }

    // ── Y 轴刻度 ────────────────────────────────────────────────────────
    // 森林图的数值轴在横向上、纵轴是组名；饼图与热图没有数值轴。
    // 这两种都跳过，否则会在左边标出一列与图形无关的数字。
    if matches!(g.y_mode, YMode::Value | YMode::Count | YMode::Prob) {
        let step = if g.ticks.len() > 1 {
            g.ticks[1] - g.ticks[0]
        } else {
            (g.y_max - g.y_min) / 5.0
        };
        for v in &g.ticks {
            let y = y_of(&g, *v) as i32;
            root.draw(&PathElement::new(vec![(g.left, y), (g.left + tl, y)], axis.clone()))?;
            let s = fmt_tick(*v, step);
            root.draw(&Text::new(
                s.clone(),
                (g.left - tl - 4 - text_w(&s, fs) as i32, y - (fs * 0.55) as i32),
                (font_of(&s), fs).into_font().color(&ink),
            ))?;
        }
    }

    // ── X 轴 ────────────────────────────────────────────────────────────
    match g.y_mode {
        // 柱形 / 散点 / 折线 / 箱线 / 小提琴：X 是分组，标组名
        YMode::Value => {
            for (i, grp) in view.groups.iter().enumerate() {
                let x = x_of(&g, i) as i32;
                root.draw(&PathElement::new(vec![(x, g.bottom), (x, g.bottom - tl)], axis.clone()))?;
                let wpx = text_w(&grp.name, fs) as i32;
                root.draw(&Text::new(
                    grp.name.clone(),
                    (x - wpx / 2, g.bottom + tl + 3),
                    (font_of(&grp.name), fs).into_font().color(&ink),
                ))?;
            }
        }
        // 直方图：X 是**分箱区间**，标各箱的边界值，不是组名
        YMode::Count => {
            if let Some((lo, hi, nb, _, _)) = hist_bins(view) {
                let pxw = (g.right - g.left) as f64;
                for k in 0..=nb {
                    let frac = k as f64 / nb as f64;
                    let v = lo + (hi - lo) * frac;
                    let x = (g.left as f64 + pxw * frac) as i32;
                    root.draw(&PathElement::new(
                        vec![(x, g.bottom), (x, g.bottom - tl / 2)],
                        axis.clone(),
                    ))?;
                    let s = fmt_tick(v, (hi - lo) / nb as f64);
                    let wpx = text_w(&s, fs) as i32;
                    root.draw(&Text::new(
                        s.clone(),
                        (x - wpx / 2, g.bottom + tl + 3),
                        (font_of(&s), fs).into_font().color(&ink),
                    ))?;
                }
            }
        }
        // 生存曲线：X 是**时间**
        YMode::Prob => {
            let tmax = view
                .groups
                .iter()
                .flat_map(|gr| gr.values.iter().copied())
                .fold(0.0f64, f64::max);
            if tmax > 0.0 {
                let pxw = (g.right - g.left) as f64;
                for k in 0..=4 {
                    let frac = k as f64 / 4.0;
                    let v = tmax * frac;
                    let x = (g.left as f64 + pxw * frac) as i32;
                    root.draw(&PathElement::new(
                        vec![(x, g.bottom), (x, g.bottom - tl)],
                        axis.clone(),
                    ))?;
                    let s = fmt_tick(v, tmax / 4.0);
                    let wpx = text_w(&s, fs) as i32;
                    root.draw(&Text::new(
                        s.clone(),
                        (x - wpx / 2, g.bottom + tl + 3),
                        (font_of(&s), fs).into_font().color(&ink),
                    ))?;
                }
            }
        }
        // 森林图的横轴（效应量）与纵轴（组名）都在 draw_kind_specific 内画；
        // 饼图与热图没有轴
        YMode::Effect | YMode::None => {}
    }

    // ── 2. 数据系列 ────────────────────────────────────────────────────────
    // 每个数据集一对颜色：
    //   fill_of    → 内部色（基础色 + Alpha 0.85，混白底后变浅）
    //   edge_of    → 边框色（HSL 明度 ×0.40、饱和 ×3.0 压暗，100% 不透明）
    // 散点跟随柱体（外环 = 边框色、内芯 = 填充色）；误差棒颜色严格 = 边框色。
    let cm = view.colormap;
    let bar_w = bar_half(&g, view) * g.slot;
    let zero = y_of(&g, g.y_min);

    match view.kind.family() {
        Family::Bar => {
            // 五种柱形变体由旗标驱动，共用下面这一段循环：
            //   横向   → 交换两轴后画（draw_bar_horizontal）
            //   点图   → 按值分箱逐点堆叠（draw_dot_stack）
            //   堆叠   → 所有组堆成一根柱，各组占一段
            //   棒棒糖 → 细杆 + 顶端圆点
            //   纯点   → 只画顶端一个点（Cleveland）
            if view.kind.horizontal() {
                draw_bar_horizontal(root, view, &g, cm, bw, ew)?;
            } else if view.kind.dot_stack() {
                draw_dot_stack(root, view, &g, cm)?;
            } else {
                // 堆叠时先算好每组的 [下沿, 上沿] 区间。
                // 归一化到 100% 时把各段按占比缩放，使总高 = 轴高。
                let stacked = view.kind.stacked();
                let norm100 = view.kind.stacked_100();
                let total: f64 = view
                    .groups
                    .iter()
                    .map(|x| if x.mean > 0.0 { x.mean } else { 0.0 })
                    .sum();
                let mut cum = 0.0f64;
                let mut spans: Vec<(f64, f64)> = Vec::with_capacity(view.groups.len());
                for grp in &view.groups {
                    let v = if grp.mean > 0.0 { grp.mean } else { 0.0 };
                    let h = if norm100 && total > 0.0 {
                        v / total * (g.y_max - g.y_min)
                    } else {
                        v
                    };
                    spans.push((g.y_min + cum, g.y_min + cum + h));
                    cum += h;
                }

                for (i, grp) in view.groups.iter().enumerate() {
                    let f = tone(view, colormap::fill_of(cm, i));     // 内部 85%
                    let e = tone(view, colormap::edge_of(cm, i));     // 边框（shade 压暗）
                    let fi = tone(view, colormap::fill_solid(cm, i)); // 散点内芯
                    // 堆叠：所有组挤在绘图区中心的一根柱里
                    let cx = if stacked {
                        (g.left + g.right) as f64 / 2.0
                    } else {
                        x_of(&g, i)
                    };
                    // 棒棒糖：杆很窄（柱宽的 12%），纯点图则完全不画杆
                    let half: f64 = if view.kind.lollipop() {
                        ((3.0 * sc).max(1.5)) as f64
                    } else {
                        bar_w / 2.0
                    };
                    let x0 = (cx - half) as i32;
                    let x1 = (cx + half) as i32;
                    let ytop = if stacked {
                        y_of(&g, spans[i].1) as i32
                    } else {
                        y_of_grp(&g, i, grp.mean) as i32
                    };
                    let ybot = if stacked {
                        y_of(&g, spans[i].0) as i32
                    } else if g.on_right.get(i).copied().unwrap_or(false) {
                        right_y_of(&g, g.right_min) as i32
                    } else {
                        zero as i32
                    };

                    // Cleveland 点图只画点，不画柱体
                    if !view.kind.dot_only() {
                        // 内部填充 85%（不描边）
                        root.draw(&Rectangle::new([(x0, ytop), (x1, ybot)], area_fill(f)))?;
                        // 边框只画**左、右、上**三条：底边压在 X 轴上，画出来会与轴线重成一条粗线
                        // （析光的柱体也是不画底边的）。
                        // 关键：用 PathElement 手画而不是 Rectangle::stroke ——
                        // Rectangle 的描边会在四角留出线宽一半的缺口，导出放大后很明显。
                        let eb = stroke(e, bw);
                        if stacked {
                            // 堆叠柱：中间各段的「底边」是相邻段，必须画，否则段与段之间会露白缝
                            root.draw(&PathElement::new(vec![(x0, ybot), (x0, ytop)], eb.clone()))?;
                            root.draw(&PathElement::new(vec![(x1, ybot), (x1, ytop)], eb.clone()))?;
                            root.draw(&PathElement::new(vec![(x0, ytop), (x1, ytop)], eb.clone()))?;
                            root.draw(&PathElement::new(vec![(x0, ybot), (x1, ybot)], eb))?;
                        } else {
                            root.draw(&PathElement::new(vec![(x0, ybot), (x0, ytop)], eb.clone()))?;
                            root.draw(&PathElement::new(vec![(x1, ybot), (x1, ytop)], eb.clone()))?;
                            root.draw(&PathElement::new(vec![(x0, ytop), (x1, ytop)], eb))?;
                        }
                        // 图案填充：线段端点已在柱体内求交，不会溢出
                        if view.bar_pattern != 0 {
                            hatch(root, view, x0, x1, ytop, ybot, view.bar_pattern, stroke(e, 1))?;
                        }
                    }

                    // 误差棒：mean ± SD，带 T 型端帽，颜色严格跟随柱边框色
                    if view.show_error && grp.n > 1 && !stacked {
                        let ea = err_of(view, grp.mean, grp.sd, grp.n);
                        // 下端必须夹在轴下限：mean - err 为负时会画到横轴**下面**，
                        // 那截误差棒就是凭空多出来的（用户没输过负数，不该出现）。
                        let lo_v = (grp.mean - ea).max(g.y_min);
                        let yhi = y_of_grp(&g, i, grp.mean + ea) as i32;
                        let ylo = y_of_grp(&g, i, lo_v) as i32;
                        let cap = (bar_w * CHART_ERR_CAP_RATIO as f64 / 2.0) as i32;
                        let eb = stroke(e, ew);
                        // 方向：0 双向 / 1 只向上 / 2 只向下
                        if view.err_dir != 2 {
                            root.draw(&PathElement::new(
                                vec![(cx as i32, yhi), (cx as i32, y_of_grp(&g, i, grp.mean) as i32)],
                                eb.clone(),
                            ))?;
                            root.draw(&PathElement::new(
                                vec![(cx as i32 - cap, yhi), (cx as i32 + cap, yhi)],
                                eb.clone(),
                            ))?;
                        }
                        if view.err_dir != 1 {
                            root.draw(&PathElement::new(
                                vec![(cx as i32, y_of_grp(&g, i, grp.mean) as i32), (cx as i32, ylo)],
                                eb.clone(),
                            ))?;
                            root.draw(&PathElement::new(
                                vec![(cx as i32 - cap, ylo), (cx as i32 + cap, ylo)],
                                eb,
                            ))?;
                        }
                    }

                    // 棒棒糖 / 纯点图：在柱顶画一个圆点（双层描边，与柱形散点同一套画法）
                    if view.kind.lollipop() || view.kind.dot_only() {
                        let (sc2, se2) = symbol_colors(view, cm, i, &e, &fi);
                        let rr = ((view.symbol_size.max(0.5) * 1.6) * view.scale.max(0.5)) as i32;
                        let ring = (view.symbol_border.max(0.0) * view.scale.max(0.5)).max(1.0) as i32;
                        let p = (cx as i32, ytop);
                        draw_shape(root, view, p, rr + ring, area_fill(se2))?;
                        draw_shape(root, view, p, rr, area_fill(with_alpha(sc2, view.symbol_alpha)))?;
                    }

                    // ── 柱形散点图的散点 ──────────────────────────────────────
                    // 这才是「符号」：把该组的**每一个重复值**作为点，叠加在柱子上，
                    // X 方向确定性抖动避免重合。点的纵坐标 = 该重复值的真实数值，
                    // 不是随便撒在柱顶附近的装饰。
                    if view.show_scatter && !stacked {
                        let (sc2, se2) = symbol_colors(view, cm, i, &e, &fi);
                        let r = (view.symbol_size.max(0.5) * view.scale.max(0.5)) as i32;
                        let ring = (view.symbol_border.max(0.0) * view.scale.max(0.5)).max(1.0) as i32;
                        // 抖动只用柱宽的一部分，保证点不会溢出柱体
                        let amp = bar_w * 0.5 * view.jitter.clamp(0.0, 1.0) as f64;
                        for (k, v) in grp.values.iter().enumerate() {
                            let t = frac(view.seed, i, k) - 0.5;
                            // 超出 Y 轴范围的点不画（例如负的重复值会落到横轴下面）
                            let (lo, hi) = if g.on_right.get(i).copied().unwrap_or(false) {
                                (g.right_min, g.right_max)
                            } else {
                                (g.y_min, g.y_max)
                            };
                            if *v < lo || *v > hi {
                                continue;
                            }
                            let px = cx + t * amp;
                            let py = y_of_grp(&g, i, *v);
                            let p = (px as i32, py as i32);
                            // 双层：外层深色（= 柱边框色）+ 内层填充色实心
                            draw_shape(root, view, p, r + ring, area_fill(se2))?;
                            draw_shape(root, view, p, r, area_fill(with_alpha(sc2, view.symbol_alpha)))?;
                        }
                    }
                }
            }
        }
        Family::Box
        | Family::Violin
        | Family::Histogram
        | Family::Pie
        | Family::Heatmap
        | Family::Forest
        | Family::Survival
        | Family::BeforeAfter
        | Family::Contingency
        | Family::Facet
        | Family::NumberAtRisk
        | Family::Dumbbell
        | Family::Ridgeline
        | Family::Radar
        | Family::Waffle
        | Family::Contour => {
            draw_kind_specific(root, view, &g)?;
        }
        Family::Scatter | Family::Line => {
            for (i, grp) in view.groups.iter().enumerate() {
                let f = tone(view, colormap::fill_of(cm, i));
                let e = tone(view, colormap::edge_of(cm, i));
                let fi = tone(view, colormap::fill_solid(cm, i));
                // 折线 / 散点**不做横向抖动**。
                //
                // 抖动的意义是「同组多个重复值重合时分得开」，只对叠加在柱子上的
                // 散点成立（见上方柱形散点分支）。折线与散点图的 X 是位置本身，
                // 横向随机偏移会把点撒到别处 —— 点看起来比数据多、折线被扭歪，
                // 是纯粹的误导。这里固定为 0，不受 jitter 影响。
                let amp = 0.0f64;
                let pts: Vec<(i32, i32)> = grp
                    .values
                    .iter()
                    .enumerate()
                    .map(|(k, v)| {
                        let t = frac(view.seed, i, k) - 0.5;
                        (x_of(&g, i) as i32 + (t * amp) as i32, y_of_grp(&g, i, *v) as i32)
                    })
                    .collect();
                if view.kind.family() == Family::Line && pts.len() > 1 {
                    // ── 折线路径 ──────────────────────────────────────────
                    // 旧实现只连首尾两点，中间的点全被丢掉 —— 那不是折线。
                    // 这里按旗标生成三种路径：直线段 / Catmull-Rom 平滑 / 阶梯。
                    let path: Vec<(i32, i32)> = if view.kind.step() {
                        step_path(&pts)
                    } else if view.kind.smooth() {
                        smooth_path(&pts, 12)
                    } else {
                        pts.clone()
                    };

                    // 面积填充：先铺底再画线，线不会被盖住。
                    // 四种填充的差别在基线：
                    //   AreaFill / AucFill → 轴底；PeakFill → 只填高于轴底的部分；
                    //   AreaBetween → 与第一条线的路径围成的区域。
                    let want_area = view.kind.area() || view.area_fill;
                    if want_area && path.len() > 1 {
                        if view.kind == ChartKind::AreaBetween {
                            // 区间填充：需要另一条线，这里用「首组的路径」作下界
                            if let Some(other) = first_path(view, &g, i) {
                                let mut poly = path.clone();
                                for p in other.iter().rev() {
                                    poly.push(*p);
                                }
                                if poly.len() > 2 {
                                    root.draw(&Polygon::new(poly, area_fill(f)))?;
                                }
                            }
                        } else if view.kind == ChartKind::PeakFill {
                            // 峰填充：把低于轴底的段裁掉，只填「峰」
                            let base = zero as i32;
                            let mut poly: Vec<(i32, i32)> = path
                                .iter()
                                .copied()
                                .filter(|p| p.1 <= base)
                                .collect();
                            if poly.len() > 2 {
                                poly.push((poly[poly.len() - 1].0, base));
                                poly.push((poly[0].0, base));
                                root.draw(&Polygon::new(poly, area_fill(f)))?;
                            }
                        } else {
                            let mut poly: Vec<(i32, i32)> = path.clone();
                            poly.push((path[path.len() - 1].0, g.bottom));
                            poly.push((path[0].0, g.bottom));
                            root.draw(&Polygon::new(poly, area_fill(f)))?;
                        }
                    }

                    // 线型：实线 / 虚线 / 点线（文档 §4.4 DASHES）
                    let d = Dash::from_usize(view.dash);
                    let lw = stroke(e, bw);
                    // 逐段画：虚线/点线的分段逻辑才能作用到每一段
                    for w in path.windows(2) {
                        let (a, b) = (w[0], w[1]);
                        if d == Dash::Solid {
                            root.draw(&PathElement::new(
                                vec![(a.0, a.1), (b.0, b.1)],
                                lw.clone(),
                            ))?;
                        } else {
                            for seg in d.segments((a.0 as f64, a.1 as f64), (b.0 as f64, b.1 as f64)) {
                                root.draw(&PathElement::new(
                                    vec![(seg.0 .0 as i32, seg.0 .1 as i32), (seg.1 .0 as i32, seg.1 .1 as i32)],
                                    lw.clone(),
                                ))?;
                            }
                        }
                    }
                }
                // 纯折线图（LineOnly）不画点
                if !view.kind.line_only() {
                    let (sc, se) = symbol_colors(view, cm, i, &e, &fi);
                    let r = (view.symbol_size.max(0.5) * view.scale.max(0.5)) as i32;
                    let ring = (view.symbol_border.max(0.0) * view.scale.max(0.5)).max(1.0) as i32;
                    for p in &pts {
                        // 同样的双层：外环深色 + 内芯填充色
                        draw_shape(root, view, *p, r + ring, area_fill(se))?;
                        draw_shape(root, view, *p, r, area_fill(with_alpha(sc, view.symbol_alpha)))?;
                    }
                }
            }
        }
    }

    // ── 2.5 连线：把各组均值串起来（「显示点 / 连线」里的连线）─────────────
    if view.connect && view.groups.len() > 1 {
        let pts: Vec<(i32, i32)> = view
            .groups
            .iter()
            .enumerate()
            .map(|(i, grp)| (x_of(&g, i) as i32, y_of_grp(&g, i, grp.mean) as i32))
            .collect();
        root.draw(&PathElement::new(pts, stroke([70, 70, 70, 255], bw)))?;
    }

    // ── 3. 显著性括号：坐标轴上方 / 顶部空白的覆盖层，逐层向下排布 ───────────
    for (xa, xb, y0, y1, label) in bracket_boxes(view, &g) {
        let (xa, xb, y0, y1) = (xa as i32, xb as i32, y0 as i32, y1 as i32);
        let br = stroke([26, 26, 26, 255], ew.max(1));
        root.draw(&PathElement::new(vec![(xa, y0), (xa, y1)], br.clone()))?;
        root.draw(&PathElement::new(vec![(xa, y1), (xb, y1)], br.clone()))?;
        root.draw(&PathElement::new(vec![(xb, y1), (xb, y0)], br))?;
        if !label.is_empty() {
            let lw = text_w(&label, fs) as i32;
            root.draw(&Text::new(
                label.clone(),
                ((xa + xb) / 2 - lw / 2, y1 - (fs * 1.05) as i32),
                (font_of(&label), fs).into_font().color(&ink),
            ))?;
        }
    }

    // ── 3.5 图形标注：归一化坐标 → 绘图区像素 ──────────────────────────────
    // 画在数据之后、轴标题之前，所以标注会盖在柱子上（与析光一致）。
    for a in &view.annotations {
        let px = |u: f32| g.left as f64 + u as f64 * (g.right - g.left) as f64;
        let py = |v: f32| g.bottom as f64 - v as f64 * (g.bottom - g.top) as f64;
        let ink = stroke([26, 26, 26, 255], 1);
        match a.kind {
            crate::model::AnnKind::Line | crate::model::AnnKind::Arrow => {
                let (x0, y0) = (px(a.x) as i32, py(a.y) as i32);
                let (x1, y1) = (px(a.x2) as i32, py(a.y2) as i32);
                root.draw(&PathElement::new(vec![(x0, y0), (x1, y1)], ink.clone()))?;
                if a.kind == crate::model::AnnKind::Arrow {
                    // 两条短斜线组成箭头头部
                    let ang = ((y1 - y0) as f64).atan2((x1 - x0) as f64);
                    let l = 7.0;
                    for d in [2.6_f64, -2.6_f64] {
                        let ex = x1 as f64 - l * (ang + d).cos();
                        let ey = y1 as f64 - l * (ang + d).sin();
                        root.draw(&PathElement::new(
                            vec![(x1, y1), (ex as i32, ey as i32)],
                            ink.clone(),
                        ))?;
                    }
                }
            }
            crate::model::AnnKind::Text => {
                let size = a.size.max(4.0) as f64 * sc as f64;
                root.draw(&Text::new(
                    a.text.clone(),
                    (px(a.x) as i32, py(a.y) as i32),
                    (font_of(&a.text), size).into_font().color(&ink.color),
                ))?;
            }
        }
    }

    // ── 4. 轴标题与图标题 ──────────────────────────────────────────────────
    // X 轴标题：水平居中于绘图区下方
    // 每段再按中/英切成子片段，各自用宋体或 Times New Roman
    let xp: Vec<(String, bool)> = view
        .pieces('x')
        .iter()
        .flat_map(|(s, it)| crate::fonts::runs(s, *it))
        .collect();
    if !xp.is_empty() {
        let total: f64 = xp.iter().map(|(s, _)| text_w(s, title_fs)).sum();
        let mut x = ((g.left + g.right) / 2) as f64 - total / 2.0;
        let y = g.bottom + tl * 2 + (fs * 1.5) as i32;
        for (s, italic) in &xp {
            let fd: FontDesc = (font_of(s), title_fs).into_font();
            let fd = if *italic { fd.style(FontStyle::Italic) } else { fd };
            root.draw(&Text::new(s.clone(), (x as i32, y), fd.color(&ink)))?;
            x += text_w(s, title_fs);
        }
    }
    // 图注：X 轴标题下方一行，字号比轴标题小（文档 §9）
    if !view.caption.is_empty() {
        let cap_fs = (CHART_FONT as f64 - 1.0).max(8.0) * sc as f64;
        let total = text_w(&view.caption, cap_fs);
        let x = ((g.left + g.right) / 2) as f64 - total / 2.0;
        let y = g.bottom + tl * 2 + (fs * 1.5) as i32 + (title_fs * 1.4) as i32;
        let cfd: FontDesc = (font_of(&view.caption), cap_fs).into_font();
        root.draw(&Text::new(view.caption.clone(), (x as i32, y), cfd.color(&ink)))?;
    }
    // Y 轴标题：逆时针 90°（自下而上），轴心居中
    let yp: Vec<(String, bool)> = view
        .pieces('y')
        .iter()
        .flat_map(|(s, it)| crate::fonts::runs(s, *it))
        .collect();
    if !yp.is_empty() {
        let cx = (6.0 * sc as f64) as i32 + (fs * 0.8) as i32;
        let cy = (g.top + g.bottom) / 2;
        draw_rotated_label(root, &yp, cx, cy, title_fs, ink)?;
    }
    // 图标题：整幅图的最上方（绘图区之上、画布顶端），与显著性标注彻底分开。
    // 标题占的高度由 geom 的 CHART_TITLE_H 预留，所以不会压到括号。
    if !view.title.is_empty() {
        let wpx = text_w(&view.title, title_fs) as i32;
        // plotters 的 FontStyle 是单选枚举，无法同时 Bold + Italic，
        // 因此优先级：倾斜 > 加粗 > 常规（期刊里倾斜通常用于物种名等）
        let fd: FontDesc = (font_of(&view.title), title_fs).into_font();
        let fd = if view.title_italic {
            fd.style(FontStyle::Italic)
        } else if view.title_bold {
            fd.style(FontStyle::Bold)
        } else {
            fd
        };
        // ★ 标题不再钉死在画布顶端。
        // 以前固定画在 y = 2·sc，而显著性括号是从**数据**顶端往上排的，
        // 层数一少（或 y_max 余量一大）标题与最高的括号之间就空出一大条，
        // 看着像「标题飘在半空」。现在标题跟着最高的括号走：
        // 底部贴在最高标签上方 3px，再多也不越过 Y 轴顶端（g.top 之上）。
        let scf = sc as f64;
        // 与 geom 里预留标题高度的算式同源（CHART_FONT+2 再乘 1.6 行高）
        let title_h = ((CHART_FONT as f64 + 2.0) * scf * 1.6) as i32;
        let lab_h = CHART_FONT as f64 * scf * 1.8;
        let top_bracket = bracket_boxes(view, &g)
            .iter()
            .map(|b| b.3)
            .fold(f64::INFINITY, f64::min);
        let title_bottom = if top_bracket.is_finite() {
            // 标签画在横线 y1 上方 lab_h 处（见上面括号绘制），所以标签顶 = y1 - lab_h
            (top_bracket - lab_h - 3.0 * scf).min(g.top as f64 - 2.0 * scf)
        } else {
            // 没有显著性标记：紧贴 Y 轴顶端上方
            g.top as f64 - 2.0 * scf
        };
        // 基线取标题矩形顶部，再夹住不越过画布上边缘
        let ty = (title_bottom - title_h as f64).max(2.0 * scf) as i32;
        root.draw(&Text::new(
            view.title.clone(),
            ((g.left + g.right) / 2 - wpx / 2, ty),
            fd.color(&ink),
        ))?;
    }

    // ── 5. 图例：放在绘图区**右侧外面**（X 轴长度之外），不压住数据 ──────────
    if view.show_legend && !view.groups.is_empty() {
        let bw2 = (10.0 * sc) as i32;
        let lh = (fs * 1.35) as i32;
        // 纵向居中于绘图区
        let total_h = lh * view.groups.len() as i32;
        let mut y = (g.top + g.bottom) / 2 - total_h / 2;
        // 位置：0 = 右侧外（geom 已预留宽度）/ 1 = 右上内 / 2 = 左上内
        let widest = view
            .groups
            .iter()
            .map(|gr| text_w(&gr.name, fs))
            .fold(0.0f64, f64::max) as i32;
        let x = match view.legend_pos {
            1 => (g.right - widest - bw2 - (6.0 * sc) as i32).max(g.left),
            2 => g.left + (6.0 * sc) as i32,
            _ => g.right + (10.0 * sc) as i32,
        };
        if view.legend_pos != 0 {
            y = g.top + (6.0 * sc) as i32;
        }
        // 反转图例顺序：索引镜像，颜色仍与该数据集绑定
        let order: Vec<usize> = if view.reverse_legend {
            (0..view.groups.len()).rev().collect()
        } else {
            (0..view.groups.len()).collect()
        };
        for i in order {
            let grp = match view.groups.get(i) {
                Some(g) => g,
                None => continue,
            };
            // 图例色块与柱体同规则：内部 85% + 边框 shade()
            let f = tone(view, colormap::fill_of(cm, i));
            let e = tone(view, colormap::edge_of(cm, i));
            root.draw(&Rectangle::new([(x, y), (x + bw2, y + bw2)], area_fill(f)))?;
            root.draw(&Rectangle::new([(x, y), (x + bw2, y + bw2)], stroke(e, 1)))?;
            root.draw(&Text::new(
                grp.name.clone(),
                (x + bw2 + 5, y - 1),
                (font_of(&grp.name), fs).into_font().color(&ink),
            ))?;
            y += lh;
        }
    }

    root.present()?;
    Ok(())
}

// ── 端 1：位图预览 ──────────────────────────────────────────────────────────

/// 渲染成 RGBA 位图，交给 charis 的 `Image::from_rgba` 显示。
pub fn render(view: &ChartView, w: u32, h: u32) -> Option<Image> {
    let (w, h) = (w.max(80), h.max(80));
    let (rw, rh, buf) = render_rgba(view, w, h)?;
    Image::from_rgba(rw, rh, &buf)
}

/// 渲染到内存 RGBA 缓冲，返回 `(宽, 高, 像素)`。
///
/// 导出弹窗里那些 plotters 自己不会写的格式（JPEG / TIFF / WebP / TGA…）
/// 都从这里拿到像素，再交给 `image` crate 编码。与预览 `render()` 同源，
/// 所以「预览什么样、导出就什么样」这条保证不会破。
pub fn render_rgba(view: &ChartView, w: u32, h: u32) -> Option<(u32, u32, Vec<u8>)> {
    let (w, h) = (w.max(80), h.max(80));
    // 显著性括号：把画布**加高**，而不是压缩绘图区（见 bracket_extra_h）。
    // geom 里 top 会加同样的量，bottom 因画布变高也加同样的量 → 绘图区不变。
    let h = h + bracket_extra_h(view).max(0) as u32;
    let n = (w as usize) * (h as usize);
    // with_buffer 只写 RGB，先按 3 字节/像素拿图，再扩成 RGBA
    let mut rgb = vec![0u8; n * 3];
    {
        let root = BitMapBackend::with_buffer(&mut rgb, (w, h)).into_drawing_area();
        draw_chart(view, &root).ok()?;
    }
    let mut buf = Vec::with_capacity(n * 4);
    for i in 0..n {
        buf.push(rgb[i * 3]);
        buf.push(rgb[i * 3 + 1]);
        buf.push(rgb[i * 3 + 2]);
        buf.push(255);
    }
    Some((w, h, buf))
}

// ── 端 2：PNG 位图导出 ──────────────────────────────────────────────────────

/// 导出为高分辨率 PNG。`scale` 为像素倍率（3.0 ≈ 300dpi 的投稿尺寸）。
///
/// 与预览共用 `draw_chart`，只把画布放大；字号与线宽由 `ChartView::scale` 自动跟随。
pub fn export_png(view: &ChartView, path: &str, scale: f32) -> Result<(), String> {
    let scale = scale.max(0.5);
    let w = ((EXPORT_W as f32) * scale) as u32;
    let h = ((EXPORT_H as f32) * scale) as u32;
    let mut v = replicate(view);
    v.scale = scale;
    // 显著性括号：画布往上扩（导出同样适用，且括号空间随 scale 一起放大）
    let h = h + bracket_extra_h(&v).max(0) as u32;
    // BitMapBackend::new 直接写盘（需要 plotters 的 bitmap_encoder 特性，默认已开）
    let root = BitMapBackend::new(path, (w, h)).into_drawing_area();
    draw_chart(&v, &root).map_err(|e| format!("绘图失败: {:?}", e))?;
    root.present().map_err(|e| format!("写入 PNG 失败: {:?}", e))?;
    Ok(())
}

/// ChartView 不可 Clone（含 Vec<GroupStat>），这里做一次浅复制供导出改写 scale。
fn replicate(view: &ChartView) -> ChartView {
    ChartView {
        kind: view.kind,
        groups: view.groups.iter().map(|g| GroupStat {
            name: g.name.clone(),
            mean: g.mean,
            sd: g.sd,
            n: g.n,
            values: g.values.clone(),
        }).collect(),
        title: view.title.clone(),
        x_name: view.x_name.clone(),
        x_unit: view.x_unit.clone(),
        y_name: view.y_name.clone(),
        y_unit: view.y_unit.clone(),
        unit_style: view.unit_style,
        use_italic: view.use_italic,
        x_sym: view.x_sym.clone(),
        y_sym: view.y_sym.clone(),
        bracket_style: view.bracket_style,
        bar_width_cm: view.bar_width_cm,
        axis_break: view.axis_break,
        right_axis: view.right_axis,
        right_from: view.right_from,
        caption: view.caption.clone(),
        grid_style: view.grid_style,
        dash: view.dash,
        show_error: view.show_error,
        show_scatter: view.show_scatter,
        show_legend: view.show_legend,
        show_bracket: view.show_bracket,
        bracket_mode: view.bracket_mode,
        annotations: view.annotations.clone(),
        reverse_legend: view.reverse_legend,
        bracket_value: view.bracket_value,
        colormap: view.colormap,
        comparisons: view.comparisons.clone(),
        symbol_follows_bar: view.symbol_follows_bar,
        symbol_color_idx: view.symbol_color_idx,
        symbol_size: view.symbol_size,
        jitter: view.jitter,
        scale: view.scale,
        frame: view.frame,
        seed: view.seed,
        err_kind: view.err_kind,
        err_dir: view.err_dir,
        err_width: view.err_width,
        connect: view.connect,
        bar_width: view.bar_width,
        bar_pattern: view.bar_pattern,
        bar_zero: view.bar_zero,
        area_fill: view.area_fill,
        grid_on: view.grid_on,
        log_y: view.log_y,
        grayscale: view.grayscale,
        legend_pos: view.legend_pos,
        line_width: view.line_width,
        symbol_shape: view.symbol_shape,
        symbol_border: view.symbol_border,
        symbol_alpha: view.symbol_alpha,
        title_bold: view.title_bold,
        title_italic: view.title_italic,
        bins: view.bins,
        pie_inner: view.pie_inner,
        pie_start: view.pie_start,
        heat_auto: view.heat_auto,
        heat_lo: view.heat_lo,
        heat_hi: view.heat_hi,
        forest_ref: view.forest_ref,
        surv_censor: view.surv_censor,
        waffle_cols: view.waffle_cols,
        stacked_100: view.stacked_100,
        facet_shared: view.facet_shared,
        box_style: view.box_style,
    }
}

/// 默认导出画布尺寸（800×600，投稿常见尺寸）。
pub const EXPORT_W: u32 = 800;
pub const EXPORT_H: u32 = 600;

/// 导出宽度（像素），来自设置；高度按 3:2 跟随。
pub fn export_w() -> u32 {
    crate::settings::get().export_width.clamp(320.0, 8000.0) as u32
}
pub fn export_h() -> u32 {
    (export_w() as f32 * EXPORT_H as f32 / EXPORT_W as f32) as u32
}

// ── 端 3：SVG 矢量导出 ──────────────────────────────────────────────────────

/// 导出为 SVG（真矢量，文字为文本对象）。与预览共用 `draw_chart`。
pub fn export_svg(view: &ChartView, path: &str, w: u32, h: u32) -> Result<(), String> {
    // 显著性括号：画布往上扩，与预览保持一致
    let h = h.max(80) + bracket_extra_h(view).max(0) as u32;
    let root = SVGBackend::new(path, (w.max(80), h)).into_drawing_area();
    draw_chart(view, &root).map_err(|e| format!("绘图失败: {:?}", e))?;
    root.present().map_err(|e| format!("写入 SVG 失败: {:?}", e))?;
    Ok(())
}

// ── 刻度算法 ────────────────────────────────────────────────────────────────

/// 1/2/5×10^k 整齐刻度。
fn nice_ticks(min: f64, max: f64, target: usize) -> Vec<f64> {
    if !(max > min) {
        return vec![min];
    }
    let raw = (max - min) / target.max(1) as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let step = if norm <= 1.0 {
        mag
    } else if norm <= 2.0 {
        2.0 * mag
    } else if norm <= 5.0 {
        5.0 * mag
    } else {
        10.0 * mag
    };
    let mut out = Vec::new();
    let mut x = (min / step).ceil() * step;
    while x <= max + step * 1e-9 {
        out.push(x);
        x += step;
    }
    if out.is_empty() {
        out.push(min);
    }
    out
}

// ── 规范自检 ────────────────────────────────────────────────────────────────
// 文档 §3.2 / §3.5 / §2.3 / §2.2 的验收用例直接写成测试，`cargo test` 可跑。

#[cfg(test)]
mod spec_tests {
    use super::*;

    #[test]
    fn axis_title_cases() {
        let p = UNIT_FMT_PAREN;
        assert_eq!(axis_title("浓度", "", "mmol/L", p, BRACKET_HALF), "浓度 (mmol/L)");
        assert_eq!(axis_title("浓度", "", "mmol/L", p, BRACKET_FULL), "浓度（mmol/L）");
        assert_eq!(axis_title("浓度", "c", "mol/L", UNIT_FMT_SLASH, BRACKET_HALF), "浓度 c / (mol/L)");
        assert_eq!(axis_title("浓度", "c", "mol/L", UNIT_FMT_COMMA, BRACKET_HALF), "浓度 c, mol/L");
        assert_eq!(axis_title("浓度", "", "mmol/L", UNIT_FMT_NONE, BRACKET_HALF), "浓度");
        assert_eq!(axis_title("", "", "mmol/L", p, BRACKET_HALF), "(mmol/L)");
        assert_eq!(axis_title("Time", "", "s", p, BRACKET_HALF), "Time (s)");
        assert_eq!(axis_title("Response", "", "a.u.", p, BRACKET_HALF), "Response (a.u.)");
        assert_eq!(axis_title("Temperature", "", "°C", p, BRACKET_HALF), "Temperature (°C)");
        // 旧接口
        assert_eq!(with_unit("浓度", "mmol/L"), "浓度 (mmol/L)");
        assert_eq!(with_unit("", "mmol/L"), "(mmol/L)");
        assert_eq!(with_unit("浓度", ""), "浓度");
    }

    #[test]
    fn unit_hint_cases() {
        let warn = |u: &str| !unit_hints(u, "").is_empty();
        // 该提示
        for u in ["mmol/l", "ml", "l", "mM", "uM", "M", "mg/kg/d"] {
            assert!(warn(u), "应提示: {}", u);
        }
        // 不该提示
        for u in ["mmol/L", "mL", "mol/L", "μmol/L", "mg/(kg·d)", "°C", "s", "a.u."] {
            assert!(!warn(u), "不应提示: {}", u);
        }
        // 单位填到了名称框
        assert!(unit_hints("mmol/L", "浓度(mmol/L)")
            .iter()
            .any(|h| h.contains("单位")));
    }

    #[test]
    fn tick_format_cases() {
        assert_eq!(fmt_tick_full(1234567.0, 1.0), "1,234,567");
        assert_eq!(fmt_tick_full(0.0, 1.0), "0");
        // 非零却会舍成 0 → 不能显示 "0"
        assert_ne!(fmt_tick_full(0.00004, 1.0), "0");
        assert!(fmt_tick_full(0.00004, 1.0).contains('e') || fmt_tick_full(0.00004, 1.0).contains('×'));
        assert_eq!(group_thousands("-1234567.5"), "-1,234,567.5");
    }

    #[test]
    fn snap_cases() {
        let (lo, hi) = snap_to_ticks(0.3, 7.8, false, 5);
        assert!(lo <= 0.3 && hi >= 7.8);
        assert_eq!(snap_to_ticks(5.0, 5.0, false, 5).0, 4.0);
        let (a, b) = snap_to_ticks(1.0, 1000.0, true, 5);
        assert!(a > 0.0 && b > a);
    }

    #[test]
    fn gray_ramp() {
        assert_eq!(gray_for(0), [40, 40, 40, 255]);
        assert_eq!(gray_for(5), [215, 215, 215, 255]);
        assert_eq!(gray_for(6), [40, 40, 40, 255]); // 循环
    }

    #[test]
    fn dash_segments() {
        let seg = Dash::Solid.segments((0.0, 0.0), (10.0, 0.0));
        assert_eq!(seg.len(), 1);
        let seg = Dash::Dashed.segments((0.0, 0.0), (100.0, 0.0));
        assert!(seg.len() > 3);
    }
}
