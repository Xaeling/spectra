//! 出版规范校验：把「科研配图规范」变成可执行的检查，而不是靠人眼看。
//!
//! 硬指标来源（见 `docs/FIGSPEC.md`）：
//! - 图内最小字号 ≥ 7 pt（按最终印刷尺寸计）——常见错误是在大画布上作图，
//!   缩到单栏后字看不清；
//! - 最细线 ≥ 0.25 pt，坐标轴 0.5–1 pt——过细的线印刷时会断；
//! - 单栏 ≈ 85 mm、双栏 ≈ 170 mm；位图 ≥ 300 dpi；
//! - 色盲友好：避免红绿对比（约 8% 男性红绿色觉障碍），
//!   建议蓝-橙、蓝-红，或用 viridis 类连续色阶。
//!
//! 这里的 `audit()` 只**报告**问题、给出修法，不擅自改用户的选择：
//! 比如用户坚持用红绿配色时，给警告但不替换，因为颜色本身承载语义。

use crate::core::color::Rgba;
use crate::core::graph::{ChartKind, GraphSpec};
use crate::plot::theme::Theme;

/// 图内最小字号（pt）。
pub const MIN_FONT_PT: f32 = 7.0;
/// 最细可见线宽（pt）。低于此值印刷会断线。
pub const MIN_LINE_PT: f32 = 0.25;
/// 坐标轴线宽的推荐区间（pt）。
pub const AXIS_WIDTH_RANGE: (f32, f32) = (0.5, 1.0);
/// 单栏宽度（mm）与双栏宽度（mm）。
pub const COLUMN_MM: (f32, f32) = (85.0, 170.0);
/// 位图最低分辨率（dpi）。
pub const MIN_DPI: u32 = 300;

/// 1 pt = 1/72 inch；1 inch = 25.4 mm。
pub fn pt_to_mm(pt: f32) -> f32 {
    pt / 72.0 * 25.4
}

/// 问题严重程度。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// 违反硬指标，投稿会被退回或印刷出问题
    Error,
    /// 建议修改，影响可读性 / 可访问性
    Warn,
    /// 提示性信息（例如画布比双栏还宽）
    Info,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Self::Error => "必须改",
            Self::Warn => "建议改",
            Self::Info => "提示",
        }
    }
}

/// 一条规范检查结果。
#[derive(Clone, Debug)]
pub struct Issue {
    pub level: Level,
    /// 涉及的项，例如「字号 · 刻度」
    pub field: String,
    /// 现状
    pub message: String,
    /// 怎么改
    pub fix: String,
}

/// 校验一份图表规格。
///
/// `width_pt` / `height_pt` 是画布的逻辑尺寸（pt），用来判断栏宽与字号是否达标。
pub fn audit(spec: &GraphSpec, theme: &Theme, width_pt: f32, height_pt: f32) -> Vec<Issue> {
    audit_series(spec, theme, width_pt, height_pt, None)
}

/// 带「实际系列数」的校验。
///
/// 色盲检查只应看**图上真正用到的颜色**：8 色方案里若只画 3 组，
/// 第 6、7 色的红绿冲突与这张图无关，误报会让提示失去可信度。
pub fn audit_series(
    spec: &GraphSpec,
    theme: &Theme,
    width_pt: f32,
    _height_pt: f32,
    series: Option<usize>,
) -> Vec<Issue> {
    let mut out = Vec::new();

    // ---- 字号 ----
    // 字号是按 pt 定义的，而 pt 本身就是印刷单位，所以直接比阈值即可。
    let fonts = [
        ("标题", theme.font_title),
        ("轴标题", theme.font_axis_title),
        ("刻度", theme.font_tick),
        ("图例", theme.font_legend),
        ("标注", theme.font_annotation),
    ];
    for (name, size) in fonts {
        if size < MIN_FONT_PT {
            out.push(Issue {
                level: Level::Error,
                field: format!("字号 · {name}"),
                message: format!("{size:.1} pt，低于 {MIN_FONT_PT} pt 下限"),
                fix: format!("把{name}字号提到 ≥ {MIN_FONT_PT} pt（印刷后最小可读尺寸）"),
            });
        }
    }
    // 信息性提示：画布越大，缩到单栏后字号被压得越小。
    let width_mm = pt_to_mm(width_pt);
    if width_mm > COLUMN_MM.1 + 5.0 {
        out.push(Issue {
            level: Level::Info,
            field: "画布宽度".into(),
            message: format!("{width_mm:.0} mm，超过双栏 {:.0} mm", COLUMN_MM.1),
            fix: "投稿通常需要缩到单栏 / 双栏，缩后请按最终尺寸复查字号".into(),
        });
    }

    // ---- 线宽 ----
    if theme.axis_width < AXIS_WIDTH_RANGE.0 {
        out.push(Issue {
            level: Level::Warn,
            field: "线宽 · 坐标轴".into(),
            message: format!(
                "{:.2} pt，低于推荐的 {:.1} pt",
                theme.axis_width, AXIS_WIDTH_RANGE.0
            ),
            fix: "坐标轴建议 0.5–1 pt，太细印刷时会断".into(),
        });
    } else if theme.axis_width > AXIS_WIDTH_RANGE.1 {
        out.push(Issue {
            level: Level::Warn,
            field: "线宽 · 坐标轴".into(),
            message: format!(
                "{:.2} pt，高于推荐的 {:.1} pt",
                theme.axis_width, AXIS_WIDTH_RANGE.1
            ),
            fix: "坐标轴过粗会压过数据，建议回到 0.5–1 pt".into(),
        });
    }
    if (spec.line_width as f32) < MIN_LINE_PT {
        out.push(Issue {
            level: Level::Error,
            field: "线宽 · 数据线".into(),
            message: format!("{:.2} pt，低于 {MIN_LINE_PT} pt 下限", spec.line_width),
            fix: "把线宽提到 ≥ 0.25 pt".into(),
        });
    }

    // ---- 色盲友好 ----
    if spec.kind.uses_multiple_series() {
        let mut colors = spec.palette.colors();
        if let Some(n) = series {
            if n > 0 && n < colors.len() {
                colors.truncate(n);
            }
        }
        if let Some((i, j, note)) = red_green_conflict(&colors) {
            out.push(Issue {
                level: Level::Warn,
                field: "配色 · 色盲友好".into(),
                message: format!("第 {} 与第 {} 色构成红绿对比（{note}）", i + 1, j + 1),
                fix: "红绿色觉障碍者约 8%，建议改蓝-橙 / 蓝-红，或换成 viridis 类色阶".into(),
            });
        }
    }

    // ---- 轴标题 ----
    if !matches!(
        spec.kind,
        ChartKind::Pie | ChartKind::Donut | ChartKind::Heatmap
    ) {
        if spec.y_title.trim().is_empty() {
            out.push(Issue {
                level: Level::Warn,
                field: "轴标题 · Y".into(),
                message: "Y 轴标题为空".into(),
                fix: "写清「量的名称/单位」，例如「体重/(g)」；无单位时也要写量名".into(),
            });
        } else if !has_unit(&spec.y_title) {
            out.push(Issue {
                level: Level::Info,
                field: "轴标题 · Y".into(),
                message: format!("「{}」未标单位", spec.y_title),
                fix: "建议写成「体重/(g)」或「浓度/(mg·L⁻¹)」；无量纲量可忽略".into(),
            });
        }
    }

    // ---- 分辨率（导出时的提示，不阻断）----
    out
}

/// 轴标题是否含单位：出现括号 / 斜杠 / 常见单位符号即认为有。
fn has_unit(title: &str) -> bool {
    const HINTS: [&str; 12] = [
        "(", "（", "/", "%", "‰", "°C", "mg", "g", "kg", "mL", "mm", "s)",
    ];
    HINTS.iter().any(|h| title.contains(h))
}

/// 检测一组颜色里是否存在「红 vs 绿」的对比对。
///
/// 判据：两者色相分别落在红区（≈ 350°–25°）与绿区（≈ 80°–160°），
/// 且彩度都够高（>0.35）——低饱和的粉/浅绿不算强红绿对比。
fn red_green_conflict(colors: &[Rgba]) -> Option<(usize, usize, &'static str)> {
    let hs: Vec<(f32, f32)> = colors.iter().map(|c| hue_sat(*c)).collect();
    for i in 0..hs.len() {
        for j in (i + 1)..hs.len() {
            let (hi, si) = hs[i];
            let (hj, sj) = hs[j];
            if si < 0.35 || sj < 0.35 {
                continue;
            }
            let red_i = is_red(hi);
            let red_j = is_red(hj);
            let green_i = is_green(hi);
            let green_j = is_green(hj);
            if (red_i && green_j) || (red_j && green_i) {
                return Some((i, j, "红-绿"));
            }
        }
    }
    None
}

fn is_red(h: f32) -> bool {
    !(25.0..350.0).contains(&h)
}

fn is_green(h: f32) -> bool {
    (80.0..160.0).contains(&h)
}

/// RGB → (色相 0–360, 彩度 0–1)。
fn hue_sat(c: Rgba) -> (f32, f32) {
    let r = c.r as f32 / 255.0;
    let g = c.g as f32 / 255.0;
    let b = c.b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let s = if max <= 0.0 { 0.0 } else { d / max };
    if d.abs() < 1e-6 {
        return (0.0, s);
    }
    let h = if (max - r).abs() < 1e-6 {
        60.0 * (((g - b) / d) % 6.0)
    } else if (max - g).abs() < 1e-6 {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    (h, s)
}

/// 导出分辨率是否达标（导出对话框用）。
pub fn dpi_ok(dpi: u32) -> bool {
    dpi >= MIN_DPI
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::graph::PlotThemeId;

    fn spec() -> GraphSpec {
        let mut s = GraphSpec::default();
        s.y_title = "体重/(g)".into();
        s
    }

    #[test]
    fn 规范常量与文献一致() {
        assert_eq!(MIN_FONT_PT, 7.0);
        assert_eq!(MIN_LINE_PT, 0.25);
        assert_eq!(AXIS_WIDTH_RANGE, (0.5, 1.0));
        assert_eq!(COLUMN_MM, (85.0, 170.0));
        assert_eq!(MIN_DPI, 300);
        // pt → mm：72 pt = 1 inch = 25.4 mm
        assert!((pt_to_mm(72.0) - 25.4).abs() < 1e-4);
    }

    #[test]
    fn 现有四套主题字号全部达标() {
        for id in PlotThemeId::ALL {
            let t = Theme::for_id(id);
            for (name, size) in [
                ("title", t.font_title),
                ("axis_title", t.font_axis_title),
                ("tick", t.font_tick),
                ("legend", t.font_legend),
                ("annotation", t.font_annotation),
            ] {
                assert!(
                    size >= MIN_FONT_PT,
                    "{:?} 的 {} = {} pt 低于下限",
                    id,
                    name,
                    size
                );
            }
            assert!(
                t.axis_width >= AXIS_WIDTH_RANGE.0 && t.axis_width <= AXIS_WIDTH_RANGE.1,
                "{:?} 轴宽 {} 不在推荐区间",
                id,
                t.axis_width
            );
        }
    }

    #[test]
    fn 达标规格不报错() {
        let issues = audit(&spec(), &Theme::for_id(PlotThemeId::Aura), 468.0, 318.0);
        let errors: Vec<_> = issues.iter().filter(|i| i.level == Level::Error).collect();
        assert!(errors.is_empty(), "不应有硬错误：{errors:?}");
    }

    #[test]
    fn 字号过小被拦下() {
        let mut t = Theme::for_id(PlotThemeId::Nature);
        t.font_tick = 6.0;
        let issues = audit(&spec(), &t, 468.0, 318.0);
        assert!(issues
            .iter()
            .any(|i| i.level == Level::Error && i.field.contains("刻度")));
    }

    #[test]
    fn 线宽过细被拦下() {
        let mut s = spec();
        s.line_width = 0.1;
        let issues = audit(&s, &Theme::for_id(PlotThemeId::Aura), 468.0, 318.0);
        assert!(issues
            .iter()
            .any(|i| i.level == Level::Error && i.field.contains("数据线")));
    }

    #[test]
    fn 画布超过双栏给提示() {
        let issues = audit(&spec(), &Theme::for_id(PlotThemeId::Aura), 600.0, 400.0);
        assert!(issues.iter().any(|i| i.field.contains("画布宽度")));
    }

    #[test]
    fn 红绿配色被识别() {
        let (h, s) = hue_sat(Rgba::hex("#D62728"));
        assert!(is_red(h) && s > 0.35, "红 {} {}", h, s);
        let (h2, s2) = hue_sat(Rgba::hex("#2CA02C"));
        assert!(is_green(h2) && s2 > 0.35, "绿 {} {}", h2, s2);
        let c = vec![Rgba::hex("#D62728"), Rgba::hex("#2CA02C")];
        assert!(red_green_conflict(&c).is_some());
    }

    #[test]
    fn 蓝橙配色不误报() {
        // Okabe-Ito 第 1、2 色是蓝与橙，属于推荐的色盲友好组合
        let c = vec![Rgba::hex("#0072B2"), Rgba::hex("#D55E00")];
        assert!(red_green_conflict(&c).is_none());
    }

    #[test]
    fn 灰阶不误报() {
        let c = vec![Rgba::hex("#4D4D4D"), Rgba::hex("#999999")];
        assert!(red_green_conflict(&c).is_none(), "低彩度灰不应算红绿冲突");
    }

    #[test]
    fn 轴标题单位识别() {
        assert!(has_unit("体重/(g)"));
        assert!(has_unit("浓度（mg·L⁻¹）"));
        // 比例 / 计数是无量纲量，不要求单位
        assert!(!has_unit("存活比例"));
        assert!(has_unit("频数 (n)"));
        assert!(!has_unit("体重"));
    }

    #[test]
    fn 空Y轴标题给警告() {
        let mut s = spec();
        s.y_title.clear();
        let issues = audit(&s, &Theme::for_id(PlotThemeId::Aura), 468.0, 318.0);
        assert!(issues.iter().any(|i| i.field.contains("轴标题")));
    }
}
