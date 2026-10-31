//! 分析结果报告：把零散统计量组织成「区块」序列。
//!
//! # 为什么要有这一层
//! 之前每个分析只吐一份扁平的 `Vec<Vec<String>>`，一行一个数字，
//! 结果页看起来像调试输出而不像分析报告。实际的分析报告是有结构的：
//! 先是**结果摘要**（检验统计量、P 值、结论），再是**分析表**（平方和/
//! 自由度/均方），接着**描述统计**、**前提检验**（方差齐性）、**事后多重比较**。
//!
//! 这一层负责把统计量组装成这种结构，渲染层（`app::result_view`）按
//! 区块标题画分隔与表头，得到与专业统计软件一致的多段式报告。
//!
//! # 编码约定
//! `flatten()` 把区块序列化成扁平行，用两个前缀标记结构：
//! - 首格以 `# ` 开头 → 区块标题
//! - 首格以 `## ` 开头 → 表头行
//! 两个前缀都占首格，渲染时剥掉前缀后显示，因此不会与真实数据冲突
//! （数值和组名都不会以 `#` 开头）。

use crate::stats;

/// 一个报告区块（对应结果页里的一段）。
pub struct Block {
    /// 区块标题，如「方差分析表」
    pub title: String,
    /// 列标题；为空表示该区块只有标题（纯说明段）
    pub cols: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// 区块末尾的脚注行，如「4 groups, 15 values in total」。
    /// 渲染成弱化小字，且不参与列宽计算（脚注常常很长，
    /// 一参与就会把整张表的列宽撑爆）。
    pub notes: Vec<String>,
}

impl Block {
    pub fn new(title: &str, cols: &[&str], rows: Vec<Vec<String>>) -> Self {
        Self {
            title: title.to_string(),
            cols: cols.iter().map(|s| s.to_string()).collect(),
            rows,
            notes: Vec::new(),
        }
    }
    /// 带脚注的区块。
    pub fn with_notes(
        title: &str,
        cols: &[&str],
        rows: Vec<Vec<String>>,
        notes: Vec<String>,
    ) -> Self {
        Self {
            notes,
            ..Self::new(title, cols, rows)
        }
    }
    /// 两列「项目 → 值」的摘要段。
    pub fn kv<S: Into<String>>(title: &str, pairs: Vec<(S, String)>) -> Self {
        Self::new(
            title,
            &["项目", "数值"],
            pairs.into_iter().map(|(k, v)| vec![k.into(), v]).collect(),
        )
    }
    /// 两列摘要段 + 末尾脚注。
    pub fn kv_notes<S: Into<String>>(
        title: &str,
        pairs: Vec<(S, String)>,
        notes: Vec<String>,
    ) -> Self {
        Self {
            notes,
            ..Self::kv(title, pairs)
        }
    }
}

/// 把区块序列序列化成扁平行（供 `ResultSheet.rows` 与 CSV 导出复用）。
pub fn flatten(blocks: &[Block]) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = Vec::new();
    for b in blocks {
        out.push(vec![format!("# {}", b.title)]);
        if !b.cols.is_empty() {
            let mut head: Vec<String> = b.cols.clone();
            head[0] = format!("## {}", head[0]);
            out.push(head);
        }
        for r in &b.rows {
            out.push(r.clone());
        }
        for n in &b.notes {
            out.push(vec![format!("#> {}", n)]);
        }
        out.push(Vec::new());
    }
    if out.last() == Some(&Vec::new()) {
        out.pop();
    }
    out
}

// ── 数值格式化 ─────────────────────────────────────────────────────────────

fn dec() -> usize {
    crate::settings::get().decimals.min(10)
}

/// 数值；非有限值显示 `—`，避免把 NAN/INF 渲染成 0 造成误读。
pub fn num(v: f64) -> String {
    if v.is_finite() {
        stats::fmt_num(v, dec())
    } else {
        "—".to_string()
    }
}

pub fn pval(v: f64) -> String {
    if v.is_finite() {
        stats::fmt_p(v)
    } else {
        "—".to_string()
    }
}

fn pct(v: f64) -> String {
    if v.is_finite() {
        format!("{:.2}%", v * 100.0)
    } else {
        "—".to_string()
    }
}

fn conf_label(conf: f64) -> String {
    format!("{:.0}%", conf * 100.0)
}

/// 星号换成中文词（`stats::stars` 给的是 ns / * / ** / ***）。
fn sig_word(p: f64) -> String {
    let st = stats::stars(p).to_string();
    match st.as_str() {
        "ns" => "不显著".to_string(),
        "*" => "显著".to_string(),
        "**" => "极显著".to_string(),
        _ => "极极显著".to_string(),
    }
}

/// 「是否显著不同」的 Yes/No 写法，把判据一并给出。
fn yes_no_p(p: f64, alpha: f64) -> String {
    if !p.is_finite() {
        return "—".to_string();
    }
    if p < alpha {
        format!("Yes (P < {})", alpha)
    } else {
        format!("No (P \u{2265} {})", alpha)
    }
}

/// 结论文字：按 α 给出「显著 / 不显著」。
fn verdict(p: f64, alpha: f64) -> String {
    if !p.is_finite() {
        return "无法判定".to_string();
    }
    if p < alpha {
        format!("显著（P < {}）", alpha)
    } else {
        format!("不显著（P ≥ {}）", alpha)
    }
}

/// F 分布的临界值：对上尾概率做二分。
///
/// 没有解析逆函数，二分足够（200 次迭代收敛到浮点精度）。
#[allow(dead_code)]
pub fn f_crit(alpha: f64, df1: f64, df2: f64) -> f64 {
    if !(df1 > 0.0) || !(df2 > 0.0) || !(alpha > 0.0) {
        return f64::NAN;
    }
    let (mut lo, mut hi) = (0.0f64, 1.0e6f64);
    for _ in 0..200 {
        let mid = (lo + hi) * 0.5;
        if stats::f_p(mid, df1, df2) > alpha {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) * 0.5
}

// ── 通用段 ─────────────────────────────────────────────────────────────────

/// 变异系数（比例）。`stats::Descriptive::cv` 是百分数，而报告表里
/// 按专业软件的惯例写成比例（0.1162 而不是 11.62%）。
fn cv_ratio(d: &stats::Descriptive) -> f64 {
    if d.cv.is_finite() {
        d.cv / 100.0
    } else {
        f64::NAN
    }
}

/// 描述统计表：**统计量成行、组成列**。
///
/// 专业软件的描述统计表一律是这个方向——左边一列是指标名，
/// 右边每列一个组。反过来（组在行）在读多组时列数会随组数暴涨，
/// 而指标数是固定的，所以指标成行才是稳定的排布。
pub fn describe_block(names: &[String], cols: &[Vec<f64>], conf: f64) -> Block {
    let cl = conf_label(conf);
    let ds: Vec<stats::Descriptive> = cols.iter().map(|c| stats::describe(c)).collect();
    // describe 内部的均值 CI 固定按 95% 算，这里按当前置信水平重算，
    // 否则改了置信水平而 CI 不变，是自相矛盾的结果。
    let cis: Vec<(f64, f64)> = ds
        .iter()
        .map(|d| {
            if d.n >= 2 && d.sem.is_finite() {
                let tc = stats::t_crit_two(conf, (d.n - 1) as f64);
                (d.mean - tc * d.sem, d.mean + tc * d.sem)
            } else {
                (f64::NAN, f64::NAN)
            }
        })
        .collect();

    let mut head: Vec<String> = vec!["统计量".to_string()];
    for (i, _) in ds.iter().enumerate() {
        head.push(
            names
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("组 {}", i + 1)),
        );
    }

    // 每一行 = 一个统计量在所有组上的取值。
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut push_stat = |label: &str, vals: Vec<String>| {
        let mut r = vec![label.to_string()];
        r.extend(vals);
        rows.push(r);
    };

    push_stat("样本量 n", ds.iter().map(|d| d.n.to_string()).collect());
    push_stat("均值 x̄", ds.iter().map(|d| num(d.mean)).collect());
    push_stat("标准差 s", ds.iter().map(|d| num(d.sd)).collect());
    push_stat("标准误 SE", ds.iter().map(|d| num(d.sem)).collect());
    push_stat("变异系数 CV", ds.iter().map(|d| num(cv_ratio(d))).collect());
    push_stat("最小值 Min", ds.iter().map(|d| num(d.min)).collect());
    push_stat("25% 分位数 Q1", ds.iter().map(|d| num(d.q1)).collect());
    push_stat("中位数 M", ds.iter().map(|d| num(d.median)).collect());
    push_stat("75% 分位数 Q3", ds.iter().map(|d| num(d.q3)).collect());
    push_stat("最大值 Max", ds.iter().map(|d| num(d.max)).collect());
    push_stat("四分位距 IQR", ds.iter().map(|d| num(d.iqr)).collect());
    push_stat("几何均值 G", ds.iter().map(|d| num(d.geo_mean)).collect());
    push_stat("偏度 g₁", ds.iter().map(|d| num(d.skewness)).collect());
    push_stat("峰度 g₂", ds.iter().map(|d| num(d.kurtosis)).collect());
    let mean_ci_key = format!("均值 {} CI", cl);
    push_stat(
        mean_ci_key.as_str(),
        ds.iter()
            .zip(cis.iter())
            .map(|(_, (lo, hi))| {
                if lo.is_finite() && hi.is_finite() {
                    format!("{} CI: [{}, {}]", cl, num(*lo), num(*hi))
                } else {
                    "—".to_string()
                }
            })
            .collect(),
    );

    Block::new(
        &format!("描述性统计 ({} CI)", cl),
        &head.iter().map(|x| x.as_str()).collect::<Vec<&str>>(),
        rows,
    )
}

/// 分布形状段（偏度 / 峰度 / 几何均值 / 中位数 CI）。
pub fn shape_block(names: &[String], cols: &[Vec<f64>], conf: f64) -> Block {
    let cl = conf_label(conf);
    let k_mlo = format!("中位数 {} CI 下限", cl);
    let k_mhi = format!("中位数 {} CI 上限", cl);
    let mut rows = Vec::new();
    for (i, c) in cols.iter().enumerate() {
        let d = stats::describe(c);
        rows.push(vec![
            names.get(i).cloned().unwrap_or_else(|| format!("组 {}", i + 1)),
            d.n.to_string(),
            num(d.skewness),
            num(d.kurtosis),
            num(d.geo_mean),
            num(d.median_ci_lo),
            num(d.median_ci_hi),
        ]);
    }
    Block::new(
        "分布形状",
        &[
            "组",
            "样本量",
            "偏度",
            "超额峰度",
            "几何均值",
            k_mlo.as_str(),
            k_mhi.as_str(),
        ],
        rows,
    )
}

/// 方差齐性检验段（Bartlett + Levene/Brown–Forsythe）。
///
/// 两个都给：Bartlett 对正态敏感，Brown–Forsythe 对非正态稳健，
/// 结论不一致时正是需要用户注意的信号，不该只报一个。
pub fn variance_block(cols: &[Vec<f64>], alpha: f64) -> Block {
    let mut rows = Vec::new();
    if let Some(b) = stats::bartlett(cols) {
        rows.push(vec![
            "Bartlett 检验".to_string(),
            format!("χ² = {}", num(b.chi2)),
            num(b.df),
            pval(b.p),
            verdict(b.p, alpha),
        ]);
    }
    if let Some(f) = stats::brown_forsythe(cols) {
        rows.push(vec![
            "Brown–Forsythe（基于中位数）".to_string(),
            format!("F = {}", num(f.f)),
            format!("{}, {}", num(f.df1), num(f.df2)),
            pval(f.p),
            verdict(f.p, alpha),
        ]);
    }
    if let Some(l) = stats::levene(cols, false) {
        rows.push(vec![
            "Levene 检验（均值）".to_string(),
            format!("F = {}", num(l.f)),
            "".to_string(),
            pval(l.p),
            verdict(l.p, alpha),
        ]);
    }
    Block::new(
        "方差齐性检验",
        &["检验", "统计量", "自由度", "P 值", "结论"],
        rows,
    )
}

/// 事后多重比较段。
pub fn pairwise_block(pw: &[stats::Pairwise], method: stats::MultiMethod, conf: f64) -> Block {
    let cl = conf_label(conf);
    let k_lo = format!("{} CI 下限", cl);
    let k_hi = format!("{} CI 上限", cl);
    let stat_name = "统计量";

    let mut rows = Vec::new();
    for p in pw {
        rows.push(vec![
            p.label.clone(),
            num(p.diff),
            num(p.lo),
            num(p.hi),
            num(p.stat),
            pval(p.p),
            stats::stars(p.p).to_string(),
        ]);
    }
    let method_label = match method {
        stats::MultiMethod::Tukey => "Tukey (all pairs)",
        stats::MultiMethod::GamesHowell => "Games-Howell (all pairs)",
        stats::MultiMethod::Dunnett => "Dunnett (vs. control)",
        stats::MultiMethod::Dunn => "Dunn (all pairs)",
        stats::MultiMethod::Holm => "Holm-Šidák (all pairs)",
        stats::MultiMethod::Sidak => "Šidák (all pairs)",
        _ => "Bonferroni (all pairs)",
    };
    Block::with_notes(
        &format!("多重比较 ({})", method_label),
        &[
            "比较",
            "均值差 Δ",
            k_lo.as_str(),
            k_hi.as_str(),
            stat_name,
            "校正后 P",
            "显著性",
        ],
        rows,
        vec![format!(
            "{} confidence interval, corrected with {}",
            cl, method_label
        )],
    )
}

/// 显著性标注段：把每个对比折成图上要画的星号。
///
/// 和多重比较表分开成段，是因为它的用途不同——这张表是**照着往图上标**
/// 的，不需要 CI 和统计量，只要「哪两组、标几颗星」。
pub fn significance_block(pw: &[stats::Pairwise]) -> Block {
    let rows: Vec<Vec<String>> = pw
        .iter()
        .map(|p| {
            let st = stats::stars(p.p).to_string();
            vec![
                p.label.clone(),
                if st == "ns" { "ns".to_string() } else { st.clone() },
                if st == "ns" { "不显著".to_string() } else { "显著".to_string() },
            ]
        })
        .collect();
    Block::new("显著性标注", &["比较", "标注", "含义"], rows)
}

// ── 各分析的完整报告 ───────────────────────────────────────────────────────

/// 0 描述统计
pub fn descriptive_report(names: &[String], cols: &[Vec<f64>], conf: f64) -> Vec<Block> {
    let total_n: usize = cols.iter().map(|c| c.len()).sum();
    let mut out = vec![Block::kv_notes(
        "结果摘要",
        vec![
            ("分析数据集", names.join("、")),
            ("数据规模", format!("{} groups, {} values", cols.len(), total_n)),
            ("置信水平", conf_label(conf)),
        ],
        vec![format!("{} groups, {} values in total", cols.len(), total_n)],
    )];
    out.push(describe_block(names, cols, conf));
    out.push(shape_block(names, cols, conf));
    out
}

/// 1 独立样本 t 检验
pub fn ttest_report(
    r: &stats::TTestResult,
    names: &[String],
    cols: &[Vec<f64>],
    welch: bool,
    conf: f64,
    alpha: f64,
) -> Vec<Block> {
    let cl = conf_label(conf);
    let method = if welch { "Welch's t test" } else { "Student's t test" };
    // 差值 CI
    let (lo, hi) = if r.se_diff.is_finite() && r.df > 0.0 {
        let tc = stats::t_crit_two(conf, r.df);
        (r.diff - tc * r.se_diff, r.diff + tc * r.se_diff)
    } else {
        (f64::NAN, f64::NAN)
    };
    // Cohen's d（合并 SD）
    let s1 = stats::describe(&cols[0]).sd;
    let s2 = if cols.len() > 1 { stats::describe(&cols[1]).sd } else { f64::NAN };
    let n1 = r.df_a as f64 + 1.0;
    let n2 = r.df_b as f64 + 1.0;
    let pooled = stats::pooled_sd(s1, cols.get(0).map(|c| c.len()).unwrap_or(0), s2, cols.get(1).map(|c| c.len()).unwrap_or(0));
    let d = if pooled > 0.0 && pooled.is_finite() {
        r.diff / pooled
    } else {
        f64::NAN
    };
    // Hedges 修正因子
    let g = if d.is_finite() {
        let j = 1.0 - 3.0 / (4.0 * (n1 + n2) - 9.0);
        d * j
    } else {
        f64::NAN
    };
    // 效应量 r²  = t²/(t²+df)
    let r2 = if r.t.is_finite() && r.df > 0.0 {
        r.t * r.t / (r.t * r.t + r.df)
    } else {
        f64::NAN
    };

    let ci_key = format!("差值 {} CI", cl);
    let k = cols.len();
    let n: usize = cols.iter().map(|c| c.len()).sum();
    let bf = stats::brown_forsythe(cols);
    let bt = stats::bartlett(cols);
    let mut kv: Vec<(&str, String)> = vec![
        ("分析数据集", names.join("、")),
        ("数据规模", format!("{} groups, {} values", k, n)),
        ("分布假设", "正态分布".to_string()),
        ("显著性水平 α", format!("{:.2}", alpha)),
        ("检验方法", method.to_string()),
        ("A 组均值", num(r.mean_a)),
        ("B 组均值", num(r.mean_b)),
        ("差值（B − A）", num(r.diff)),
        ("差值标准误", num(r.se_diff)),
        ("t 值", num(r.t)),
        ("自由度 df", num(r.df)),
        (ci_key.as_str(), format!("{} 至 {}", num(lo), num(hi))),
        ("P 值", pval(r.p)),
        ("显著性", sig_word(r.p)),
        ("均值是否显著不同", yes_no_p(r.p, alpha)),
        ("效应量 R²", num(r2)),
    ];
    if let Some(b) = &bf {
        kv.push((
            "Brown–Forsythe F（组间自由度, 组内自由度）",
            format!("{}({:.0}, {:.0})", num(b.f), b.df1, b.df2),
        ));
        kv.push(("Brown–Forsythe P 值", pval(b.p)));
        kv.push(("标准差是否显著不同", yes_no_p(b.p, alpha)));
    }
    if let Some(b) = &bt {
        kv.push(("Bartlett χ²", num(b.chi2)));
        kv.push(("Bartlett P 值", pval(b.p)));
    }
    let out_summary = format!(
        "t = {}，df = {}，P = {}",
        num(r.t),
        num(r.df),
        pval(r.p)
    );
    let mut out = vec![Block::kv_notes("结果摘要", kv, vec![out_summary])];
    let mut fit = vec![Block::new(
        "各组统计量",
        &["组", "样本量", "均值", "标准差", "标准误", "中位数", "最小值", "最大值"],
        {
            let mut rows = Vec::new();
            for (i, c) in cols.iter().take(2).enumerate() {
                let d = stats::describe(c);
                rows.push(vec![
                    names.get(i).cloned().unwrap_or_else(|| format!("组 {}", i + 1)),
                    d.n.to_string(),
                    num(d.mean),
                    num(d.sd),
                    num(d.sem),
                    num(d.median),
                    num(d.min),
                    num(d.max),
                ]);
            }
            rows
        },
    )];
    out.append(&mut fit);
    out.push(Block::kv(
        "效应量",
        vec![
            ("Cohen's d", num(d)),
            ("Hedges' g", num(g)),
            ("合并标准差", num(pooled)),
            ("R²", num(r2)),
        ],
    ));
    if cols.len() >= 2 {
        out.push(variance_block(&cols[..2], alpha));
    }
    out
}

/// 2 单因素 ANOVA —— 结果页的样板（摘要 / 分析表 / 描述 / 方差齐性 / 多重比较）
pub fn anova_report(
    names: &[String],
    cols: &[Vec<f64>],
    conf: f64,
    alpha: f64,
    method: stats::MultiMethod,
) -> Vec<Block> {
    let r = match stats::anova(cols) {
        Some(r) => r,
        None => return Vec::new(),
    };
    let k = cols.len();
    let n: usize = cols.iter().map(|c| c.len()).sum();
    let ss_total = r.ss_between + r.ss_within;
    let r2 = if ss_total > 0.0 { r.ss_between / ss_total } else { f64::NAN };
    let ms_between = if r.df_between > 0.0 { r.ss_between / r.df_between } else { f64::NAN };
    let bf = stats::brown_forsythe(cols);
    let bt = stats::bartlett(cols);

    // ── 结果摘要 ──────────────────────────────────────────────────
    // 方差齐性的两项（Brown–Forsythe / Bartlett）放在摘要里，而不是
    // 单独成段：它们是 ANOVA 的前提检验，读报告时紧接着 F、P 看才顺。
    let mut kv: Vec<(&str, String)> = vec![
        ("分析数据集", names.join("、")),
        ("数据规模", format!("{} groups, {} values", k, n)),
        ("分布假设", "正态分布".to_string()),
        ("显著性水平 α", format!("{:.2}", alpha)),
        ("F 值", num(r.f)),
        ("P 值", pval(r.p)),
        ("显著性", sig_word(r.p)),
        ("均值是否显著不同", yes_no_p(r.p, alpha)),
        ("决定系数 R²", num(r2)),
    ];
    if let Some(b) = &bf {
        kv.push((
            "Brown–Forsythe F（组间自由度, 组内自由度）",
            format!("{}({:.0}, {:.0})", num(b.f), b.df1, b.df2),
        ));
        kv.push(("Brown–Forsythe P 值", pval(b.p)));
        kv.push(("显著性", sig_word(b.p)));
        // 「标准差是否显著不同」以 Brown–Forsythe 为准：它用中位数，
        // 对非正态稳健；Bartlett 对非正态极敏感，容易假阳性。
        kv.push(("标准差是否显著不同", yes_no_p(b.p, alpha)));
    }
    if let Some(b) = &bt {
        kv.push(("Bartlett χ²", num(b.chi2)));
        kv.push(("Bartlett P 值", pval(b.p)));
    }
    let summary = format!(
        "F({:.0}, {:.0}) = {}，P = {}，R² = {}",
        r.df_between,
        r.df_within,
        num(r.f),
        pval(r.p),
        num(r2)
    );
    let mut out = vec![Block::kv_notes("结果摘要", kv, vec![summary])];

    // ── 方差分析表 ────────────────────────────────────────────────
    let df_total = r.df_between + r.df_within;
    out.push(Block::with_notes(
        "方差分析表",
        &["变异来源", "平方和 SS", "自由度 df", "均方 MS", "F 值", "P 值"],
        vec![
            vec![
                "组间（处理）".to_string(),
                num(r.ss_between),
                num(r.df_between),
                num(ms_between),
                num(r.f),
                pval(r.p),
            ],
            vec![
                "组内（残差）".to_string(),
                num(r.ss_within),
                num(r.df_within),
                num(r.ms_within),
                "".to_string(),
                "".to_string(),
            ],
            vec![
                "总计".to_string(),
                num(ss_total),
                num(df_total),
                "".to_string(),
                "".to_string(),
                "".to_string(),
            ],
        ],
        vec![format!("{} groups, {} values in total", k, n)],
    ));

    out.push(describe_block(names, cols, conf));

    // ── 事后多重比较 ──────────────────────────────────────────────
    if k >= 3 {
        let pw = match method {
            stats::MultiMethod::GamesHowell => stats::games_howell(cols, conf),
            stats::MultiMethod::Dunnett => stats::dunnett(cols, conf),
            stats::MultiMethod::Dunn => stats::dunn_test(cols, conf),
            _ => stats::tukey_hsd(cols, conf),
        };
        if let Some(pw) = pw {
            out.push(pairwise_block(&pw, method, conf));
            out.push(significance_block(&pw));
        }
    }
    out
}

/// 3 线性回归
pub fn regression_report(
    r: &stats::Regression,
    xs: &[f64],
    ys: &[f64],
    conf: f64,
    alpha: f64,
) -> Vec<Block> {
    let cl = conf_label(conf);
    let n = r.n;
    let df = (n as f64) - 2.0;
    // 回归的方差分析表：SS_reg = r²·SS_total
    let my = ys.iter().sum::<f64>() / n as f64;
    let sst: f64 = ys.iter().map(|y| (y - my) * (y - my)).sum();
    let ssr = r.r2 * sst;
    let sse = sst - ssr;
    let msr = ssr;
    let mse = if df > 0.0 { sse / df } else { f64::NAN };
    let f = if mse > 0.0 && mse.is_finite() { msr / mse } else { f64::NAN };
    let p_f = if f.is_finite() { stats::f_p(f, 1.0, df) } else { f64::NAN };
    let syx = if df > 0.0 { (sse / df).sqrt() } else { f64::NAN };
    let tc = stats::t_crit_two(conf, df);
    let slope_lo = r.slope - tc * r.se_slope;
    let slope_hi = r.slope + tc * r.se_slope;
    let mx = xs.iter().sum::<f64>() / n as f64;
    let sxx: f64 = xs.iter().map(|x| (x - mx) * (x - mx)).sum();
    // 截距标准误：SE_b²·Σx²/n
    let se_int = if sxx > 0.0 && n > 0 {
        r.se_slope * (sxx / n as f64).sqrt()
    } else {
        f64::NAN
    };
    let int_lo = r.intercept - tc * se_int;
    let int_hi = r.intercept + tc * se_int;
    let t_slope = if r.se_slope > 0.0 { r.slope / r.se_slope } else { f64::NAN };

    let mut out = vec![Block::kv(
        "结果摘要",
        vec![
            ("分析方法", "Simple linear regression".to_string()),
            ("回归方程", format!("Y = {} + {}·X", num(r.intercept), num(r.slope))),
            ("观测数", n.to_string()),
            ("Pearson 相关系数 r", num(r.r)),
            ("R²", num(r.r2)),
            ("调整后 R²", num(1.0 - (1.0 - r.r2) * ((n as f64) - 1.0) / df.max(1.0))),
            ("残差标准差（Sy.x）", num(syx)),
            ("F 值（1, df）", num(f)),
            ("P 值", pval(r.p)),
            ("P 值摘要", stats::stars(r.p).to_string()),
            ("结论（α）", verdict(r.p, alpha)),
        ],
    )];

    out.push(Block::new(
        "方差分析表",
        &["变异来源", "平方和", "自由度", "均方", "F 值", "P 值"],
        vec![
            vec![
                "回归".to_string(),
                num(ssr),
                "1".to_string(),
                num(msr),
                num(f),
                pval(p_f),
            ],
            vec![
                "残差".to_string(),
                num(sse),
                num(df),
                num(mse),
                "".to_string(),
                "".to_string(),
            ],
            vec![
                "总计".to_string(),
                num(sst),
                num((n as f64) - 1.0),
                "".to_string(),
                "".to_string(),
                "".to_string(),
            ],
        ],
    ));

    let k_clo = format!("{} CI 下限", cl);
    let k_chi = format!("{} CI 上限", cl);
    out.push(Block::new(
        "系数",
        &[
            "项",
            "估计值",
            "标准误",
            "t 值",
            "P 值",
            k_clo.as_str(),
            k_chi.as_str(),
        ],
        vec![
            vec![
                "斜率".to_string(),
                num(r.slope),
                num(r.se_slope),
                num(t_slope),
                pval(r.p),
                num(slope_lo),
                num(slope_hi),
            ],
            vec![
                "截距".to_string(),
                num(r.intercept),
                num(se_int),
                "".to_string(),
                "".to_string(),
                num(int_lo),
                num(int_hi),
            ],
        ],
    ));
    out
}

/// 4 嵌套 ANOVA
#[allow(dead_code)]
pub fn nested_report(names: &[String], r: &stats::NestedAnova, cols: &[Vec<f64>], alpha: f64) -> Vec<Block> {
    vec![
        Block::kv(
            "结果摘要",
            vec![
                ("分析方法", "Nested one-way ANOVA".to_string()),
                ("组数", cols.len().to_string()),
                ("F 值（处理）", num(r.f_treat)),
                ("P 值（处理）", pval(r.p_treat)),
                ("显著性（处理）", stats::stars(r.p_treat).to_string()),
                ("结论（处理）", verdict(r.p_treat, alpha)),
                ("F 值（嵌套）", num(r.f_nested)),
                ("P 值（嵌套）", pval(r.p_nested)),
                ("显著性（嵌套）", stats::stars(r.p_nested).to_string()),
                ("结论（嵌套）", verdict(r.p_nested, alpha)),
            ],
        ),
        Block::new(
            "方差分析表",
            &["变异来源", "F 值", "P 值", "显著性", "结论"],
            vec![
                vec![
                    "处理（组间）".to_string(),
                    num(r.f_treat),
                    pval(r.p_treat),
                    stats::stars(r.p_treat).to_string(),
                    verdict(r.p_treat, alpha),
                ],
                vec![
                    "嵌套（组内）".to_string(),
                    num(r.f_nested),
                    pval(r.p_nested),
                    stats::stars(r.p_nested).to_string(),
                    verdict(r.p_nested, alpha),
                ],
            ],
        ),
        describe_block(names, cols, 0.95),
    ]
}

/// 5 相关矩阵
pub fn corr_report(names: &[String], m: &[Vec<f64>], ns: &[usize], alpha: f64) -> Vec<Block> {
    let k = names.len();
    let mut rrows: Vec<Vec<String>> = Vec::new();
    let mut prows: Vec<Vec<String>> = Vec::new();
    let mut nrows: Vec<Vec<String>> = Vec::new();
    for i in 0..k {
        let mut rr = vec![names[i].clone()];
        let mut pr = vec![names[i].clone()];
        let mut nr = vec![names[i].clone()];
        for j in 0..k {
            if i == j {
                rr.push("1".to_string());
                pr.push("—".to_string());
                nr.push(ns.get(i).copied().unwrap_or(0).to_string());
                continue;
            }
            let r = m[i][j];
            let n = ns.get(i).copied().unwrap_or(0).min(ns.get(j).copied().unwrap_or(0));
            rr.push(num(r));
            let p = stats::corr_p(r, n).unwrap_or(f64::NAN);
            pr.push(pval(p));
            nr.push(n.to_string());
        }
        rrows.push(rr);
        prows.push(pr);
        nrows.push(nr);
    }
    let head: Vec<String> = std::iter::once("变量".to_string())
        .chain(names.iter().cloned())
        .collect();
    let mut out = vec![
        Block { notes: Vec::new(),
            title: "相关矩阵（Pearson r）".to_string(),
            cols: head.clone(),
            rows: rrows,
        },
        Block { notes: Vec::new(),
            title: "P 值（双尾）".to_string(),
            cols: head.clone(),
            rows: prows,
        },
        Block { notes: Vec::new(),
            title: "配对样本量".to_string(),
            cols: head.clone(),
            rows: nrows,
        },
    ];
    // 逐对明细
    let mut pairs: Vec<Vec<String>> = Vec::new();
    for i in 0..k {
        for j in (i + 1)..k {
            let r = m[i][j];
            let n = ns.get(i).copied().unwrap_or(0).min(ns.get(j).copied().unwrap_or(0));
            let p = stats::corr_p(r, n).unwrap_or(f64::NAN);
            pairs.push(vec![
                format!("{} vs {}", names[i], names[j]),
                num(r),
                num(r * r),
                n.to_string(),
                pval(p),
                stats::stars(p).to_string(),
                verdict(p, alpha),
            ]);
        }
    }
    out.push(Block::new(
        "逐对明细",
        &["配对", "Pearson 相关系数 r", "r²", "样本量", "P 值", "显著性", "结论"],
        pairs,
    ));
    out
}

/// 6 正态性检验（四种方法并列）
pub fn normality_report(names: &[String], cols: &[Vec<f64>], alpha: f64) -> Vec<Block> {
    let mut rows = Vec::new();
    for (i, c) in cols.iter().enumerate() {
        let nm = names.get(i).cloned().unwrap_or_else(|| format!("组 {}", i + 1));
        let sw = stats::shapiro_wilk(c);
        let da = stats::dagostino(c);
        let ad = stats::anderson_darling(c);
        let lf = stats::lilliefors(c);
        let mk = |o: Option<stats::Normality>| -> (String, String, String) {
            match o {
                Some(v) => (num(v.w), pval(v.p), stats::stars(v.p).to_string()),
                None => ("—".to_string(), "—".to_string(), "—".to_string()),
            }
        };
        let (a, b, c3) = mk(sw);
        let (d, e, f) = mk(da);
        let (g, h, i2) = mk(ad);
        let (j, k, l) = mk(lf);
        rows.push(vec![
            nm, a, b, c3, d, e, f, g, h, i2, j, k, l,
        ]);
    }
    vec![
        Block::kv(
            "显著性",
            vec![
                ("分析方法", "正态性检验".to_string()),
                ("显著性水平", format!("α = {}", alpha)),
                ("说明", "四种检验并列：检验效能与敏感性各有差异".to_string()),
            ],
        ),
        Block::new(
            "正态性检验",
            &[
                "组",
                "Shapiro–Wilk W",
                "SW P 值",
                "SW 显著性",
                "D'Agostino K²",
                "DA P 值",
                "DA 显著性",
                "Anderson–Darling A²",
                "AD P 值",
                "AD 显著性",
                "Lilliefors D",
                "LF P 值",
                "LF 显著性",
            ],
            rows,
        ),
    ]
}

/// 7 离群值（Grubbs）
pub fn grubbs_report(names: &[String], cols: &[Vec<f64>], alpha: f64) -> Vec<Block> {
    let mut rows = Vec::new();
    for (i, c) in cols.iter().enumerate() {
        let nm = names.get(i).cloned().unwrap_or_else(|| format!("组 {}", i + 1));
        match stats::grubbs(c) {
            Some(g) => rows.push(vec![
                nm,
                num(g.value),
                (g.index + 1).to_string(),
                num(g.g),
                pval(g.p),
                stats::stars(g.p).to_string(),
                verdict(g.p, alpha),
            ]),
            None => rows.push(vec![nm, "—".to_string(), "—".to_string(), "—".to_string(), "—".to_string(), "—".to_string(), "样本量不足或方差为 0".to_string()]),
        }
    }
    vec![
        Block::kv(
            "显著性",
            vec![
                ("分析方法", "Grubbs' test for outliers".to_string()),
                ("显著性水平", format!("α = {}", alpha)),
                ("说明", "每组仅检验最极端的一个值".to_string()),
            ],
        ),
        Block::new(
            "离群值检验",
            &["组", "极端值", "行", "G", "P 值", "显著性", "结论"],
            rows,
        ),
    ]
}

/// 8 卡方独立性检验
pub fn chisq_report(m: &[Vec<f64>], r: &stats::ChiSquare, alpha: f64) -> Vec<Block> {
    let rows_n = m.len();
    let cols_n = m.first().map(|r| r.len()).unwrap_or(0);
    let row_t: Vec<f64> = (0..rows_n).map(|i| m[i].iter().sum::<f64>()).collect();
    let col_t: Vec<f64> = (0..cols_n).map(|j| (0..rows_n).map(|i| m[i][j]).sum::<f64>()).collect();
    let total: f64 = row_t.iter().sum();

    // 期望频数与每格对卡方的贡献
    let mut exp_rows: Vec<Vec<String>> = Vec::new();
    let mut contrib: Vec<Vec<String>> = Vec::new();
    for i in 0..rows_n {
        let mut e = vec![format!("行 {}", i + 1)];
        let mut c = vec![format!("行 {}", i + 1)];
        for j in 0..cols_n {
            let eij = if total > 0.0 { row_t[i] * col_t[j] / total } else { 0.0 };
            e.push(num(eij));
            let o = m[i][j];
            let cc = if eij > 0.0 { (o - eij) * (o - eij) / eij } else { 0.0 };
            c.push(num(cc));
        }
        e.push(num(row_t[i]));
        c.push(num(row_t[i]));
        exp_rows.push(e);
        contrib.push(c);
    }
    let mut head: Vec<String> = (0..cols_n).map(|j| format!("列 {}", j + 1)).collect();
    head.insert(0, "".to_string());
    head.push("总计".to_string());

    // 关联度量
    let min_dim = rows_n.min(cols_n);
    let phi = if total > 0.0 && min_dim > 1 {
        (r.chi2 / total).sqrt()
    } else {
        f64::NAN
    };
    let cramers_v = if min_dim > 1 && phi.is_finite() {
        phi / ((min_dim - 1) as f64).sqrt()
    } else {
        f64::NAN
    };
    let contingency = if r.chi2.is_finite() && total > 0.0 {
        (r.chi2 / (r.chi2 + total)).sqrt()
    } else {
        f64::NAN
    };

    vec![
        Block::kv(
            "结果摘要",
            vec![
                ("分析方法", "Chi-square test of independence".to_string()),
                ("卡方", format!("χ² = {}", num(r.chi2))),
                ("自由度", format!("df = {}", r.df)),
                ("总观测数", r.n.to_string()),
                ("P 值", pval(r.p)),
                ("P 值摘要", stats::stars(r.p).to_string()),
                ("结论（α）", verdict(r.p, alpha)),
                ("Phi", num(phi)),
                ("Cramér's V", num(cramers_v)),
                ("列联系数", num(contingency)),
            ],
        ),
        Block { notes: Vec::new(),
            title: "观察频数".to_string(),
            cols: head.clone(),
            rows: {
                let mut v = Vec::new();
                for i in 0..rows_n {
                    let mut r0 = vec![format!("行 {}", i + 1)];
                    for j in 0..cols_n {
                        r0.push(num(m[i][j]));
                    }
                    r0.push(num(row_t[i]));
                    v.push(r0);
                }
                v
            },
        },
        Block { notes: Vec::new(),
            title: "期望频数".to_string(),
            cols: head.clone(),
            rows: exp_rows,
        },
        Block { notes: Vec::new(),
            title: "卡方贡献".to_string(),
            cols: head.clone(),
            rows: contrib,
        },
    ]
}

/// 12 Fisher 精确检验
pub fn fisher_report(m: &[Vec<f64>], r: &stats::Fisher, alpha: f64) -> Vec<Block> {
    let (a, b, c, d) = (
        m.get(0).and_then(|r| r.get(0)).copied().unwrap_or(0.0),
        m.get(0).and_then(|r| r.get(1)).copied().unwrap_or(0.0),
        m.get(1).and_then(|r| r.get(0)).copied().unwrap_or(0.0),
        m.get(1).and_then(|r| r.get(1)).copied().unwrap_or(0.0),
    );
    let or = if b > 0.0 && c > 0.0 { (a * d) / (b * c) } else { f64::NAN };
    // Woolf 对数 OR 的标准误，用于 OR 的 CI
    let se = if a > 0.0 && b > 0.0 && c > 0.0 && d > 0.0 {
        (1.0 / a + 1.0 / b + 1.0 / c + 1.0 / d).sqrt()
    } else {
        f64::NAN
    };
    let (lo, hi) = if or > 0.0 && se.is_finite() {
        let l = or.ln();
        ((l - 1.959963984540054 * se).exp(), (l + 1.959963984540054 * se).exp())
    } else {
        (f64::NAN, f64::NAN)
    };
    vec![
        Block::kv(
            "结果摘要",
            vec![
                ("分析方法", "Fisher's exact test".to_string()),
                ("P 值（双尾）", pval(r.p_two)),
                ("P 值摘要", stats::stars(r.p_two).to_string()),
                ("结论（α）", verdict(r.p_two, alpha)),
                ("比值比 OR", num(or)),
                ("OR 95% CI", format!("{} 至 {}", num(lo), num(hi))),
            ],
        ),
        Block { notes: Vec::new(),
            title: "列联表".to_string(),
            cols: vec!["".to_string(), "列 1".to_string(), "列 2".to_string(), "总计".to_string()],
            rows: vec![
                vec!["行 1".to_string(), num(a), num(b), num(a + b)],
                vec!["行 2".to_string(), num(c), num(d), num(c + d)],
                vec!["总计".to_string(), num(a + c), num(b + d), num(a + b + c + d)],
            ],
        },
    ]
}

/// 13 相对风险 / 比值比
pub fn risk_report(m: &[Vec<f64>], conf: f64, alpha: f64) -> Vec<Block> {
    let g = |i: usize, j: usize| -> f64 { m.get(i).and_then(|r| r.get(j)).copied().unwrap_or(0.0) };
    let (a, b, c, d) = (g(0, 0), g(0, 1), g(1, 0), g(1, 1));
    let r1 = if (a + b) > 0.0 { a / (a + b) } else { f64::NAN };
    let r2 = if (c + d) > 0.0 { c / (c + d) } else { f64::NAN };
    let rr = if r2 > 0.0 && r1.is_finite() { r1 / r2 } else { f64::NAN };
    let or = if b > 0.0 && c > 0.0 { (a * d) / (b * c) } else { f64::NAN };
    let se_ln_rr = if a > 0.0 && b > 0.0 && c > 0.0 && d > 0.0 {
        (1.0 / a - 1.0 / (a + b) + 1.0 / c - 1.0 / (c + d)).sqrt()
    } else {
        f64::NAN
    };
    let se_ln_or = if a > 0.0 && b > 0.0 && c > 0.0 && d > 0.0 {
        (1.0 / a + 1.0 / b + 1.0 / c + 1.0 / d).sqrt()
    } else {
        f64::NAN
    };
    let z = stats::t_crit_two(conf, 1e9);
    let ci = |v: f64, se: f64| -> (f64, f64) {
        if v > 0.0 && se.is_finite() {
            let l = v.ln();
            ((l - z * se).exp(), (l + z * se).exp())
        } else {
            (f64::NAN, f64::NAN)
        }
    };
    let (rr_lo, rr_hi) = ci(rr, se_ln_rr);
    let (or_lo, or_hi) = ci(or, se_ln_or);
    let cl = conf_label(conf);
    let rr_key = format!("RR {} CI", cl);
    let or_key = format!("OR {} CI", cl);
    // 卡方（无校正）用于 P 值
    let n = a + b + c + d;
    let chi2 = if n > 0.0 {
        let e11 = (a + b) * (a + c) / n;
        let e12 = (a + b) * (b + d) / n;
        let e21 = (c + d) * (a + c) / n;
        let e22 = (c + d) * (b + d) / n;
        let f = |o: f64, e: f64| if e > 0.0 { (o - e) * (o - e) / e } else { 0.0 };
        f(a, e11) + f(b, e12) + f(c, e21) + f(d, e22)
    } else {
        f64::NAN
    };
    let p = if chi2.is_finite() { stats::f_p(chi2, 1.0, 1e9) } else { f64::NAN };
    vec![
        Block::kv(
            "结果摘要",
            vec![
                ("分析方法", "风险度量".to_string()),
                ("第 1 组风险", num(r1)),
                ("第 2 组风险", num(r2)),
                ("相对危险度 RR", num(rr)),
                (rr_key.as_str(), format!("{} 至 {}", num(rr_lo), num(rr_hi))),
                ("比值比 OR", num(or)),
                (or_key.as_str(), format!("{} 至 {}", num(or_lo), num(or_hi))),
                ("卡方", num(chi2)),
                ("P 值", pval(p)),
                ("结论（α）", verdict(p, alpha)),
            ],
        ),
        Block { notes: Vec::new(),
            title: "列联表".to_string(),
            cols: vec!["".to_string(), "事件".to_string(), "非事件".to_string(), "总计".to_string()],
            rows: vec![
                vec!["第 1 组".to_string(), num(a), num(b), num(a + b)],
                vec!["第 2 组".to_string(), num(c), num(d), num(c + d)],
                vec!["总计".to_string(), num(a + c), num(b + d), num(n)],
            ],
        },
    ]
}

/// 14 Kaplan–Meier
pub fn km_report(pts: &[stats::KmPoint], conf: f64, alpha: f64) -> Vec<Block> {
    let mut rows = Vec::new();
    for p in pts {
        rows.push(vec![
            num(p.time),
            num(p.surv),
            p.at_risk.to_string(),
            p.events.to_string(),
        ]);
    }
    // 中位生存期：生存率首次 ≤0.5 的时间
    let median = pts.iter().find(|p| p.surv <= 0.5).map(|p| p.time).unwrap_or(f64::NAN);
    let final_surv = pts.last().map(|p| p.surv).unwrap_or(f64::NAN);
    vec![
        Block::kv(
            "结果摘要",
            vec![
                ("分析方法", "Kaplan–Meier 生存估计".to_string()),
                ("时间点数", pts.len().to_string()),
                ("中位生存时间", num(median)),
                ("末时间点生存率", num(final_surv)),
                ("置信水平", conf_label(conf)),
                ("显著性水平", format!("α = {}", alpha)),
                ("说明", "缺少删失指示列：所有观测均按事件处理".to_string()),
            ],
        ),
        Block::new(
            "生存表",
            &["时间", "生存率", "在险人数", "事件数"],
            rows,
        ),
    ]
}

/// 21–24 独立的事后多重比较入口
pub fn pairwise_report(cols: &[Vec<f64>], method: stats::MultiMethod, conf: f64) -> Vec<Block> {
    let pw = match method {
        stats::MultiMethod::GamesHowell => stats::games_howell(cols, conf),
        stats::MultiMethod::Dunnett => stats::dunnett(cols, conf),
        stats::MultiMethod::Dunn => stats::dunn_test(cols, conf),
        _ => stats::tukey_hsd(cols, conf),
    };
    let mut out = Vec::new();
    if let Some(a) = stats::anova(cols) {
        out.push(Block::kv(
            "结果摘要",
            vec![
                ("分析方法", "多重比较".to_string()),
                ("总体 F 值", format!("F({:.0}, {:.0}) = {}", a.df_between, a.df_within, num(a.f))),
                ("总体 P 值", pval(a.p)),
                ("比较次数", format!("{}", cols.len() * (cols.len() - 1) / 2)),
            ],
        ));
    }
    if let Some(pw) = pw {
        out.push(pairwise_block(&pw, method, conf));
    }
    out
}

/// 30 单样本 t 检验
pub fn onesample_report(r: &stats::OneSampleResult, mu0: f64, conf: f64, alpha: f64) -> Vec<Block> {
    let cl = conf_label(conf);
    let ci_key = format!("均值 {} CI", cl);
    let d = if r.sd > 0.0 { (r.mean - mu0) / r.sd } else { f64::NAN };
    vec![
        Block::kv(
            "结果摘要",
            vec![
                ("分析方法", "One-sample t test".to_string()),
                ("假设均值 μ0", num(mu0)),
                ("样本量", r.n.to_string()),
                ("均值", num(r.mean)),
                ("标准差", num(r.sd)),
                ("标准误", num(r.sem)),
                ("与 μ0 的差值", num(r.mean - mu0)),
                ("t 值", format!("t = {}", num(r.t))),
                ("自由度", format!("df = {}", num(r.df))),
                ("P 值", pval(r.p)),
                ("P 值摘要", stats::stars(r.p).to_string()),
                ("结论（α）", verdict(r.p, alpha)),
                (ci_key.as_str(), format!("{} 至 {}", num(r.ci_lo), num(r.ci_hi))),
                ("Cohen's d", num(d)),
            ],
        ),
        Block::new(
            "描述性统计",
            &["项目", "数值"],
            vec![
                vec!["样本量".to_string(), r.n.to_string()],
                vec!["均值".to_string(), num(r.mean)],
                vec!["标准差".to_string(), num(r.sd)],
                vec!["标准误".to_string(), num(r.sem)],
            ],
        ),
    ]
}

/// 31 配对 t 检验
pub fn paired_report(r: &stats::PairedTResult, conf: f64, alpha: f64) -> Vec<Block> {
    let cl = conf_label(conf);
    let ci_key = format!("差值 {} CI", cl);
    let d = if r.sd_diff > 0.0 { r.mean_diff / r.sd_diff } else { f64::NAN };
    let r_bis = if r.sd_a > 0.0 && r.sd_b > 0.0 {
        // 配对数据的相关系数由差值方差反推：SD_diff² = SD_a² + SD_b² − 2·r·SD_a·SD_b
        let num2 = r.sd_a * r.sd_a + r.sd_b * r.sd_b - r.sd_diff * r.sd_diff;
        let den = 2.0 * r.sd_a * r.sd_b;
        if den > 0.0 { num2 / den } else { f64::NAN }
    } else {
        f64::NAN
    };
    vec![
        Block::kv(
            "结果摘要",
            vec![
                ("分析方法", "Paired t test".to_string()),
                ("配对数", r.n.to_string()),
                ("A 组均值", num(r.mean_a)),
                ("B 组均值", num(r.mean_b)),
                ("均值差（B − A）", num(r.mean_diff)),
                ("差值标准差", num(r.sd_diff)),
                ("差值标准误", num(r.sem_diff)),
                ("t 值", format!("t = {}", num(r.t))),
                ("自由度", format!("df = {}", num(r.df))),
                ("P 值", pval(r.p)),
                ("P 值摘要", stats::stars(r.p).to_string()),
                ("结论（α）", verdict(r.p, alpha)),
                (ci_key.as_str(), format!("{} 至 {}", num(r.ci_lo), num(r.ci_hi))),
                ("配对相关系数（r）", num(r_bis)),
                ("配对 Cohen's d", num(d)),
            ],
        ),
        Block::new(
            "各组统计量",
            &["组", "样本量", "均值", "标准差", "标准误"],
            vec![
                vec!["A 组".to_string(), r.n.to_string(), num(r.mean_a), num(r.sd_a), num(r.sem_a)],
                vec!["B 组".to_string(), r.n.to_string(), num(r.mean_b), num(r.sd_b), num(r.sem_b)],
                vec!["差值".to_string(), r.n.to_string(), num(r.mean_diff), num(r.sd_diff), num(r.sem_diff)],
            ],
        ),
    ]
}

/// 32 Deming 回归
pub fn deming_report(r: &stats::DemingResult, lambda: f64, xs: &[f64], ys: &[f64]) -> Vec<Block> {
    let n = r.n;
    let m = xs.len().min(ys.len());
    let mx: f64 = xs[..m].iter().sum::<f64>() / m as f64;
    let my: f64 = ys[..m].iter().sum::<f64>() / m as f64;
    let sxx: f64 = xs[..m].iter().map(|x| (x - mx).powi(2)).sum();
    let syy: f64 = ys[..m].iter().map(|y| (y - my).powi(2)).sum();
    let sxy: f64 = xs[..m].iter().zip(ys[..m].iter()).map(|(x, y)| (x - mx) * (y - my)).sum();
    // 注意：不要命名为 r —— 会遮蔽参数 `r: &DemingResult`，导致 r.slope 报 E0610
    let pearson_r = if sxx > 0.0 && syy > 0.0 { sxy / (sxx * syy).sqrt() } else { f64::NAN };
    vec![
        Block::kv(
            "结果摘要",
            vec![
                ("分析方法", "Deming regression".to_string()),
                ("观测数", n.to_string()),
                ("斜率", num(r.slope)),
                ("截距", num(r.intercept)),
                ("回归方程", format!("Y = {} + {}·X", num(r.intercept), num(r.slope))),
                ("λ（方差比）", num(lambda.max(1e-9))),
                ("Pearson 相关系数 r", num(pearson_r)),
            ],
        ),
        Block::new(
            "方差分量",
            &["项目", "数值"],
            vec![
                vec!["Sxx".to_string(), num(sxx)],
                vec!["Syy".to_string(), num(syy)],
                vec!["Sxy".to_string(), num(sxy)],
            ],
        ),
    ]
}

/// 33 比较两条回归直线的斜率
pub fn slopes_report(r: &stats::SlopeCompareResult, alpha: f64) -> Vec<Block> {
    vec![
        Block::kv(
            "结果摘要",
            vec![
                ("分析方法", "回归斜率比较".to_string()),
                ("斜率 1", num(r.slope1)),
                ("斜率 2", num(r.slope2)),
                ("差值", num(r.diff)),
                ("差值标准误", num(r.se)),
                ("t 值", num(r.t)),
                ("自由度", num(r.df)),
                ("P 值（斜率相等）", pval(r.p)),
                ("P 值摘要", stats::stars(r.p).to_string()),
                ("结论（α）", verdict(r.p, alpha)),
                ("共同斜率", num(r.common_slope)),
                ("F 值（截距相等）", num(r.f_intercept)),
                ("P 值（截距相等）", pval(r.p_equal_intercept)),
                ("显著性（截距）", stats::stars(r.p_equal_intercept).to_string()),
                ("结论（截距）", verdict(r.p_equal_intercept, alpha)),
            ],
        ),
        Block::new(
            "斜率",
            &["数据集", "斜率"],
            vec![
                vec!["1".to_string(), num(r.slope1)],
                vec!["2".to_string(), num(r.slope2)],
                vec!["共同（约束）".to_string(), num(r.common_slope)],
            ],
        ),
    ]
}

/// 34 / 15 生存相关的 log-rank
pub fn logrank_report(r: &stats::LogRankResult, names: &[String], alpha: f64) -> Vec<Block> {
    let mut rows = Vec::new();
    for (i, hr) in r.hr.iter().enumerate() {
        rows.push(vec![
            names.get(i).cloned().unwrap_or_else(|| format!("组 {}", i + 1)),
            num(*hr),
        ]);
    }
    vec![
        Block::kv(
            "结果摘要",
            vec![
                ("分析方法", "Log-rank (Mantel–Cox) test".to_string()),
                ("卡方", format!("χ² = {}", num(r.chi2))),
                ("自由度", format!("df = {}", num(r.df))),
                ("P 值", pval(r.p)),
                ("P 值摘要", stats::stars(r.p).to_string()),
                ("结论（α）", verdict(r.p, alpha)),
                ("组数", r.hr.len().to_string()),
            ],
        ),
        Block::new("风险比 HR（O/E）", &["组", "HR"], rows),
    ]
}

/// 11 Bartlett 方差齐性
pub fn bartlett_report(cols: &[Vec<f64>], names: &[String], alpha: f64) -> Vec<Block> {
    let mut sd_rows = Vec::new();
    for (i, c) in cols.iter().enumerate() {
        let d = stats::describe(c);
        sd_rows.push(vec![
            names.get(i).cloned().unwrap_or_else(|| format!("组 {}", i + 1)),
            d.n.to_string(),
            num(d.sd),
            num(d.sd * d.sd),
        ]);
    }
    let mut out = Vec::new();
    if let Some(b) = stats::bartlett(cols) {
        out.push(Block::kv(
            "结果摘要",
            vec![
                ("分析方法", "Bartlett 检验".to_string()),
                ("卡方", format!("χ² = {}", num(b.chi2))),
                ("自由度", format!("df = {}", num(b.df))),
                ("P 值", pval(b.p)),
                ("P 值摘要", stats::stars(b.p).to_string()),
                ("结论（α）", verdict(b.p, alpha)),
                ("说明", "原假设为方差相等".to_string()),
            ],
        ));
    }
    if let Some(f) = stats::brown_forsythe(cols) {
        out.push(Block::kv(
            "Brown–Forsythe（稳健替代）",
            vec![
                ("F 值", num(f.f)),
                ("自由度", format!("{}, {}", num(f.df1), num(f.df2))),
                ("P 值", pval(f.p)),
                ("结论（α）", verdict(f.p, alpha)),
            ],
        ));
    }
    out.push(Block::new(
        "标准差",
        &["组", "样本量", "标准差", "方差"],
        sd_rows,
    ));
    out
}

/// 9 非线性回归
pub fn nonlinear_report(r: &crate::analysis::FitResult, xs: &[f64], ys: &[f64], alpha: f64) -> Vec<Block> {
    let names = r.model.param_names();
    // 兜底名必须是本函数内的 String，再借 &str —— 不能 unwrap_or_else(|| format!(..))，那样返回 String 而元素是 &str
    let fallback: Vec<String> = (0..r.params.len()).map(|i| format!("P{}", i + 1)).collect();
    let mut coef = Vec::new();
    for (i, p) in r.params.iter().enumerate() {
        let nm: &str = names.get(i).copied().unwrap_or_else(|| fallback[i].as_str());
        coef.push(vec![
            nm.to_string(),
            num(*p),
            num(r.se.get(i).copied().unwrap_or(f64::NAN)),
            num(r.t.get(i).copied().unwrap_or(f64::NAN)),
            num(r.ci_lo.get(i).copied().unwrap_or(f64::NAN)),
            num(r.ci_hi.get(i).copied().unwrap_or(f64::NAN)),
        ]);
    }
    let sst = r.sst;
    vec![
        Block::kv(
            "结果摘要",
            vec![
                ("分析方法", "Nonlinear regression".to_string()),
                ("模型", r.model.equation().to_string()),
                ("观测数", r.n.to_string()),
                ("参数个数", r.params.len().to_string()),
                ("自由度", num(r.df)),
                ("残差平方和", num(r.sse)),
                ("Sy.x（残差标准差）", num((r.sse / r.df.max(1.0)).sqrt())),
                ("R²", num(r.r2)),
                ("调整后 R²", num(r.adj_r2)),
                ("AICc", num(r.aicc)),
                ("显著性水平", format!("α = {}", alpha)),
                ("是否收敛", if r.converged { "是".to_string() } else { "否（结果不可靠）".to_string() }),
            ],
        ),
        Block::new(
            "参数",
            &["参数", "估计值", "标准误", "t 值", "95% CI 下限", "95% CI 上限"],
            coef,
        ),
        Block::new(
            "拟合值与实测值",
            &["X", "Y 实测值", "Y 预测值", "残差"],
            {
                let m = xs.len().min(ys.len());
                (0..m)
                    .map(|i| {
                        let pred = r.model.eval(xs[i], r.params.as_slice());
                        vec![num(xs[i]), num(ys[i]), num(pred), num(ys[i] - pred)]
                    })
                    .collect()
            },
        ),
        Block::kv(
            "拟合优度",
            vec![
                ("SST", num(sst)),
                ("SSE", num(r.sse)),
                ("R²", num(r.r2)),
            ],
        ),
    ]
}

/// 10 ROC 曲线
pub fn roc_report(r: &stats::RocResult, alpha: f64) -> Vec<Block> {
    let z = 1.959963984540054;
    let lo = (r.auc - z * r.se).clamp(0.0, 1.0);
    let hi = (r.auc + z * r.se).clamp(0.0, 1.0);
    // 与 0.5 比较的 z 检验
    let zt = if r.se > 0.0 { (r.auc - 0.5) / r.se } else { f64::NAN };
    let p = if zt.is_finite() { 2.0 * stats::norm_sf(zt.abs()) } else { f64::NAN };
    let mut pts = Vec::new();
    for q in &r.points {
        pts.push(vec![num(q.threshold), num(q.fpr), num(q.tpr)]);
    }
    vec![
        Block::kv(
            "结果摘要",
            vec![
                ("分析方法", "ROC 曲线".to_string()),
                ("阳性例数", r.n_pos.to_string()),
                ("阴性例数", r.n_neg.to_string()),
                ("AUC", num(r.auc)),
                ("标准误（Hanley–McNeil）", num(r.se)),
                ("95% CI", format!("{} 至 {}", num(lo), num(hi))),
                ("z 值（对比 AUC = 0.5）", num(zt)),
                ("P 值", pval(p)),
                ("P 值摘要", stats::stars(p).to_string()),
                ("结论（α）", verdict(p, alpha)),
                ("最佳阈值（Youden J）", num(r.best_threshold)),
                ("最佳切点灵敏度", pct(r.best_sens)),
                ("最佳切点特异度", pct(r.best_spec)),
            ],
        ),
        Block::new("ROC 曲线点", &["阈值", "1 − 特异度（FPR）", "灵敏度（TPR）"], pts),
    ]
}

/// 15 Cox 比例风险
pub fn cox_report(r: &stats::CoxResult, alpha: f64) -> Vec<Block> {
    let mut rows = Vec::new();
    for v in &r.vars {
        rows.push(vec![
            v.name.clone(),
            num(v.beta),
            num(v.hr),
            num(v.se),
            num(v.z),
            pval(v.p),
            num(v.ci_lo),
            num(v.ci_hi),
            stats::stars(v.p).to_string(),
        ]);
    }
    vec![
        Block::kv(
            "结果摘要",
            vec![
                ("分析方法", "Cox proportional hazards".to_string()),
                ("观测数", r.n.to_string()),
                ("事件数", r.n_events.to_string()),
                ("偏对数似然", num(r.loglik)),
                ("显著性水平", format!("α = {}", alpha)),
                ("是否收敛", if r.converged { "是".to_string() } else { "否（结果不可靠）".to_string() }),
            ],
        ),
        Block::new(
            "系数",
            &["变量", "β", "风险比 HR", "标准误", "Wald z", "P 值", "HR 95% CI 下限", "HR 95% CI 上限", "显著性"],
            rows,
        ),
    ]
}

/// 18 主成分分析
pub fn pca_report(r: &stats::PcaResult, names: &[String]) -> Vec<Block> {
    let mut eig = Vec::new();
    for i in 0..r.eigen.len() {
        eig.push(vec![
            format!("PC{}", i + 1),
            num(r.eigen[i]),
            pct(r.proportion[i]),
            pct(r.cumulative[i]),
        ]);
    }
    let mut load = Vec::new();
    for (vi, nm) in names.iter().enumerate() {
        let mut row = vec![nm.clone()];
        for pc in 0..r.eigen.len() {
            row.push(num(r.loadings.get(vi).and_then(|l| l.get(pc)).copied().unwrap_or(f64::NAN)));
        }
        load.push(row);
    }
    let head: Vec<String> = std::iter::once("变量".to_string())
        .chain((0..r.eigen.len()).map(|i| format!("PC{}", i + 1)))
        .collect();
    vec![
        Block::kv(
            "结果摘要",
            vec![
                ("分析方法", "Principal component analysis".to_string()),
                ("观测数", r.n.to_string()),
                ("变量数", r.n_vars.to_string()),
                ("分解基", "相关矩阵（z 分数标准化）".to_string()),
                ("PC1 解释比例", pct(r.proportion.first().copied().unwrap_or(f64::NAN))),
            ],
        ),
        Block::new("特征值", &["主成分", "特征值", "方差比例", "累积比例"], eig),
        Block { notes: Vec::new(), title: "载荷矩阵".to_string(), cols: head, rows: load },
    ]
}

/// 19 层次聚类
pub fn hier_report(r: &stats::HierResult, names: &[String], k: usize) -> Vec<Block> {
    let mut merges = Vec::new();
    for (i, m) in r.merges.iter().enumerate() {
        merges.push(vec![
            format!("第 {} 步", i + 1),
            names.get(m.a).cloned().unwrap_or_else(|| format!("{}", m.a + 1)),
            names.get(m.b).cloned().unwrap_or_else(|| format!("{}", m.b + 1)),
            num(m.dist),
            m.size.to_string(),
        ]);
    }
    let mut assign = Vec::new();
    for (i, c) in r.cluster.iter().enumerate() {
        assign.push(vec![
            names.get(i).cloned().unwrap_or_else(|| format!("{}", i + 1)),
            format!("簇 {}", c + 1),
        ]);
    }
    vec![
        Block::kv(
            "结果摘要",
            vec![
                ("分析方法", "Hierarchical clustering".to_string()),
                ("观测数", r.n.to_string()),
                ("连接方法", "平均连接".to_string()),
                ("距离", "z 分数欧氏距离".to_string()),
                ("切分为 k 簇", k.to_string()),
            ],
        ),
        Block::new("合并序列", &["步骤", "簇 A", "簇 B", "距离", "簇大小"], merges),
        Block::new("簇归属", &["观测", "簇"], assign),
    ]
}

/// 20 K-means
pub fn kmeans_report(r: &stats::KmeansResult, names: &[String], varnames: &[String]) -> Vec<Block> {
    let mut centers = Vec::new();
    for (ci, c) in r.centers.iter().enumerate() {
        let mut row = vec![format!("簇 {}", ci + 1), r.sizes.get(ci).copied().unwrap_or(0).to_string()];
        for v in c {
            row.push(num(*v));
        }
        centers.push(row);
    }
    let mut assign = Vec::new();
    for (i, c) in r.cluster.iter().enumerate() {
        assign.push(vec![
            names.get(i).cloned().unwrap_or_else(|| format!("{}", i + 1)),
            format!("簇 {}", c + 1),
        ]);
    }
    let head: Vec<String> = std::iter::once("簇".to_string())
        .chain(std::iter::once("簇大小".to_string()))
        .chain(varnames.iter().cloned())
        .collect();
    vec![
        Block::kv(
            "结果摘要",
            vec![
                ("分析方法", "K-means clustering".to_string()),
                ("簇数", r.centers.len().to_string()),
                ("迭代次数", r.iterations.to_string()),
                ("是否收敛", if r.converged { "是".to_string() } else { "否（未达到收敛上限前稳定）".to_string() }),
                ("簇内平方和合计", num(r.wss)),
                ("总平方和", num(r.tss)),
                ("方差解释率", pct(1.0 - if r.tss > 0.0 { r.wss / r.tss } else { f64::NAN })),
            ],
        ),
        Block { notes: Vec::new(), title: "簇中心（z 分数空间）".to_string(), cols: head, rows: centers },
        Block::new("簇归属", &["观测", "簇"], assign),
    ]
}

// ── 分派 ───────────────────────────────────────────────────────────────────

/// `analysis::MultiMethod` 与 `stats::MultiMethod` 是两个同名但独立的枚举
/// （前者是设置层、后者是算法层），变体集合相同但顺序不同，必须显式转换。
// ── 16 总分统计（Parts of whole）────────────────────────────────────────────
pub fn total_score_report(names: &[String], cols: &[Vec<f64>]) -> Vec<Block> {
    let sums: Vec<f64> = cols.iter().map(|c| c.iter().sum::<f64>()).collect();
    let total: f64 = sums.iter().sum();
    let k = cols.len();
    let mut rows = Vec::new();
    for (i, v) in sums.iter().enumerate() {
        let p = if total != 0.0 { *v / total * 100.0 } else { f64::NAN };
        rows.push(vec![
            names.get(i).cloned().unwrap_or_else(|| format!("组 {}", i + 1)),
            num(*v),
            pct(p / 100.0),
        ]);
    }
    rows.push(vec!["总计".to_string(), num(total), pct(if total != 0.0 { 1.0 } else { f64::NAN })]);
    let mut kv: Vec<(&str, String)> = vec![
        ("分析方法", "总分统计".to_string()),
        ("类别数", k.to_string()),
        ("总和", num(total)),
    ];
    if let Some((bi, bv)) = sums
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
    {
        kv.push((
            "最大类别",
            names.get(bi).cloned().unwrap_or_else(|| format!("组 {}", bi + 1)),
        ));
        kv.push(("最大类别占比", pct(if total != 0.0 { *bv / total } else { f64::NAN })));
    }
    vec![
        Block::kv("结果摘要", kv),
        Block::new("各部分占比", &["类别", "数值", "占比"], rows),
    ]
}

// ── 17 观察值与期望值比较（拟合优度卡方）──────────────────────────────────
pub fn gof_report(names: &[String], cols: &[Vec<f64>], alpha: f64) -> Vec<Block> {
    let obs: Vec<f64> = cols.iter().map(|c| c.iter().sum::<f64>()).collect();
    let k = obs.len();
    if k < 2 {
        return Vec::new();
    }
    let total: f64 = obs.iter().sum();
    let exp = if k > 0 { total / k as f64 } else { f64::NAN };
    let chi2: f64 = obs
        .iter()
        .map(|o| if exp > 0.0 { (o - exp) * (o - exp) / exp } else { 0.0 })
        .sum();
    let df = (k - 1) as f64;
    let p = stats::gamma_q(df / 2.0, chi2 / 2.0);
    let mut rows = Vec::new();
    for (i, o) in obs.iter().enumerate() {
        rows.push(vec![
            names.get(i).cloned().unwrap_or_else(|| format!("组 {}", i + 1)),
            num(*o),
            num(exp),
            num(*o - exp),
            num(if exp > 0.0 { (o - exp) * (o - exp) / exp } else { 0.0 }),
        ]);
    }
    rows.push(vec!["总计".to_string(), num(total), num(total), num(0.0), num(chi2)]);
    vec![
        Block::kv(
            "结果摘要",
            vec![
                ("分析方法", "观察值与期望值比较".to_string()),
                ("类别数", k.to_string()),
                ("总观测数", num(total)),
                ("期望值（等分）", num(exp)),
                ("卡方", num(chi2)),
                ("自由度", num(df)),
                ("P 值", pval(p)),
                ("P 值摘要", stats::stars(p).to_string()),
                ("结论（α）", verdict(p, alpha)),
            ],
        ),
        Block::new("观察值与期望值", &["类别", "观察值", "期望值", "差值", "卡方贡献"], rows),
    ]
}

// ── 26 Mann-Whitney U ──────────────────────────────────────────────────────
pub fn mannwhitney_report(names: &[String], cols: &[Vec<f64>], alpha: f64) -> Vec<Block> {
    if cols.len() < 2 {
        return Vec::new();
    }
    let r = match stats::mann_whitney(&cols[0], &cols[1]) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let da = stats::describe(&cols[0]);
    let db = stats::describe(&cols[1]);
    let na = names.get(0).cloned().unwrap_or_else(|| "A 组".to_string());
    let nb = names.get(1).cloned().unwrap_or_else(|| "B 组".to_string());
    vec![
        Block::kv(
            "结果摘要",
            vec![
                ("分析方法", "Mann-Whitney U 检验".to_string()),
                (na.as_str(), format!("n = {}", da.n)),
                (nb.as_str(), format!("n = {}", db.n)),
                ("A 组中位数", num(da.median)),
                ("B 组中位数", num(db.median)),
                ("U", num(r.u)),
                ("z 值", num(r.z)),
                ("P 值", pval(r.p)),
                ("显著性", sig_word(r.p)),
                ("结论（α）", verdict(r.p, alpha)),
            ],
        ),
        Block::new(
            "各组统计量",
            &["组", "样本量", "中位数", "均值", "标准差"],
            vec![
                vec![na.clone(), da.n.to_string(), num(da.median), num(da.mean), num(da.sd)],
                vec![nb.clone(), db.n.to_string(), num(db.median), num(db.mean), num(db.sd)],
            ],
        ),
    ]
}

// ── 27 Wilcoxon 符号秩 ─────────────────────────────────────────────────────
pub fn wilcoxon_report(names: &[String], cols: &[Vec<f64>], alpha: f64) -> Vec<Block> {
    if cols.len() < 2 {
        return Vec::new();
    }
    let r = match stats::wilcoxon(&cols[0], &cols[1]) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let da = stats::describe(&cols[0]);
    let db = stats::describe(&cols[1]);
    let na = names.get(0).cloned().unwrap_or_else(|| "A 组".to_string());
    let nb = names.get(1).cloned().unwrap_or_else(|| "B 组".to_string());
    vec![
        Block::kv(
            "结果摘要",
            vec![
                ("分析方法", "Wilcoxon 符号秩检验".to_string()),
                ("配对数", da.n.min(db.n).to_string()),
                ("A 组中位数", num(da.median)),
                ("B 组中位数", num(db.median)),
                ("W", num(r.w)),
                ("z 值", num(r.z)),
                ("P 值", pval(r.p)),
                ("显著性", sig_word(r.p)),
                ("结论（α）", verdict(r.p, alpha)),
            ],
        ),
        Block::new(
            "各组统计量",
            &["组", "样本量", "中位数", "均值", "标准差"],
            vec![
                vec![na.clone(), da.n.to_string(), num(da.median), num(da.mean), num(da.sd)],
                vec![nb.clone(), db.n.to_string(), num(db.median), num(db.mean), num(db.sd)],
            ],
        ),
    ]
}

// ── 28 Kruskal-Wallis ──────────────────────────────────────────────────────
pub fn kruskal_report(names: &[String], cols: &[Vec<f64>], alpha: f64) -> Vec<Block> {
    let r = match stats::kruskal_wallis(cols) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let n: usize = cols.iter().map(|c| c.len()).sum();
    let mut srows = Vec::new();
    for (i, c) in cols.iter().enumerate() {
        let d = stats::describe(c);
        srows.push(vec![
            names.get(i).cloned().unwrap_or_else(|| format!("组 {}", i + 1)),
            d.n.to_string(),
            num(d.median),
            num(d.mean),
            num(d.sd),
        ]);
    }
    vec![
        Block::kv(
            "结果摘要",
            vec![
                ("分析方法", "Kruskal-Wallis 检验".to_string()),
                ("分析数据集", names.join("、")),
                ("组数", cols.len().to_string()),
                ("总观测数", n.to_string()),
                ("H 值", num(r.h)),
                ("自由度", num(r.df)),
                ("P 值", pval(r.p)),
                ("显著性", sig_word(r.p)),
                ("分布是否显著不同", yes_no_p(r.p, alpha)),
                ("结论（α）", verdict(r.p, alpha)),
            ],
        ),
        Block::new("各组统计量", &["组", "样本量", "中位数", "均值", "标准差"], srows),
    ]
}

fn mm_conv(m: crate::analysis::MultiMethod) -> stats::MultiMethod {
    match m {
        crate::analysis::MultiMethod::Tukey => stats::MultiMethod::Tukey,
        crate::analysis::MultiMethod::Holm => stats::MultiMethod::Holm,
        crate::analysis::MultiMethod::Bonferroni => stats::MultiMethod::Bonferroni,
        crate::analysis::MultiMethod::Sidak => stats::MultiMethod::Sidak,
        crate::analysis::MultiMethod::GamesHowell => stats::MultiMethod::GamesHowell,
        crate::analysis::MultiMethod::Dunnett => stats::MultiMethod::Dunnett,
        crate::analysis::MultiMethod::Dunn => stats::MultiMethod::Dunn,
    }
}

/// 把 `run_analysis` 产出的扁平结果升级成多段式报告。
///
/// 只对**确实写了完整构建器**的分析生效；其余保持原样，再统一包一层
/// 区块标题，保证所有结果页的样式一致。这里不伪造内容——没有构建器的
/// 分析就是保持它原有的输出。
pub fn enrich(
    method: usize,
    names: &[String],
    cols: &[Vec<f64>],
    fallback: Vec<Vec<String>>,
    p: &crate::analysis::AnalysisParams,
) -> Vec<Vec<String>> {
    let blocks: Vec<Block> = match method {
        0 => descriptive_report(names, cols, p.conf),
        1 => {
            if cols.len() >= 2 {
                match stats::t_test(
                    &cols[0],
                    &cols[1],
                    if p.welch {
                        stats::TTestKind::Welch
                    } else {
                        stats::TTestKind::Pooled
                    },
                ) {
                    Some(mut r) => {
                        r.a = names.get(0).cloned().unwrap_or_else(|| "A 组".into());
                        r.b = names.get(1).cloned().unwrap_or_else(|| "B 组".into());
                        ttest_report(&r, names, cols, p.welch, p.conf, p.alpha)
                    }
                    None => Vec::new(),
                }
            } else {
                Vec::new()
            }
        }
        2 => anova_report(names, cols, p.conf, p.alpha, mm_conv(p.method)),
        3 => {
            let yi = if cols.len() > 1 { 1 } else { 0 };
            let xs = cols.first().cloned().unwrap_or_default();
            let ys = cols.get(yi).cloned().unwrap_or_default();
            let len = xs.len().min(ys.len());
            match stats::regression(&xs[..len], &ys[..len]) {
                Some(r) => regression_report(&r, &xs[..len], &ys[..len], p.conf, p.alpha),
                None => Vec::new(),
            }
        }
        5 => {
            let ns: Vec<usize> = cols.iter().map(|c| c.len()).collect();
            match stats::corr_matrix(cols) {
                Some(m) => corr_report(names, &m, &ns, p.alpha),
                None => Vec::new(),
            }
        }
        6 => normality_report(names, cols, p.alpha),
        7 => grubbs_report(names, cols, p.alpha),
        8 => {
            let m = flat_to_matrix(cols);
            match stats::chi_square(&m) {
                Some(r) => chisq_report(&m, &r, p.alpha),
                None => Vec::new(),
            }
        }
        9 => match crate::analysis::fit_nonlinear(
            cols.first().cloned().unwrap_or_default().as_slice(),
            cols.get(1).cloned().unwrap_or_default().as_slice(),
            p.model,
        ) {
            Some(r) => nonlinear_report(
                &r,
                cols.first().cloned().unwrap_or_default().as_slice(),
                cols.get(1).cloned().unwrap_or_default().as_slice(),
                p.alpha,
            ),
            None => Vec::new(),
        },
        10 => {
            if cols.len() >= 2 {
                match stats::roc_curve(&cols[0], &cols[1]) {
                    Some(r) => roc_report(&r, p.alpha),
                    None => Vec::new(),
                }
            } else {
                Vec::new()
            }
        }
        11 => bartlett_report(cols, names, p.alpha),
        12 => {
            let m = flat_to_matrix(cols);
            if m.len() == 2 && m[0].len() == 2 {
                match stats::fisher_exact([[m[0][0], m[0][1]], [m[1][0], m[1][1]]]) {
                    Some(r) => fisher_report(&m, &r, p.alpha),
                    None => Vec::new(),
                }
            } else {
                Vec::new()
            }
        }
        13 => {
            let m = flat_to_matrix(cols);
            risk_report(&m, p.conf, p.alpha)
        }
        14 => {
            let time = cols.first().cloned().unwrap_or_default();
            let ev = cols.get(1).cloned().unwrap_or_else(|| cols[0].clone());
            let len = time.len().min(ev.len());
            let pts = stats::kaplan_meier(&time[..len], &ev[..len]);
            km_report(&pts, p.conf, p.alpha)
        }
        15 => {
            // Cox：第 0 列为时间，第 1 列为事件指示，其余为协变量
            if cols.len() >= 3 {
                let time = &cols[0];
                let event = &cols[1];
                let covs: Vec<Vec<f64>> = cols[2..].to_vec();
                let mut vs: Vec<(String, Vec<f64>)> = Vec::new();
                for (i, c) in covs.iter().enumerate() {
                    vs.push((
                        names.get(i + 2).cloned().unwrap_or_else(|| format!("X{}", i + 1)),
                        c.clone(),
                    ));
                }
                match stats::cox_ph(time, event, &vs) {
                    Some(r) => cox_report(&r, p.alpha),
                    None => Vec::new(),
                }
            } else {
                Vec::new()
            }
        }
        18 => {
            let n_vars = cols.len();
            let n_obs = cols.iter().map(|c| c.len()).max().unwrap_or(0);
            let aligned: Vec<Vec<f64>> = (0..n_obs)
                .map(|r| (0..n_vars).map(|c| cols[c].get(r).copied().unwrap_or(f64::NAN)).collect())
                .collect();
            let by_var: Vec<Vec<f64>> = (0..n_vars)
                .map(|c| aligned.iter().map(|row| row[c]).filter(|v| v.is_finite()).collect())
                .collect();
            match stats::pca(&by_var, true) {
                Some(r) => pca_report(&r, names),
                None => Vec::new(),
            }
        }
        19 => {
            let n_obs = cols.iter().map(|c| c.len()).max().unwrap_or(0);
            let k = cols.len();
            let by_obs: Vec<Vec<f64>> = (0..n_obs)
                .map(|r| (0..k).map(|c| cols[c].get(r).copied().unwrap_or(f64::NAN)).collect())
                .collect();
            let by_var: Vec<Vec<f64>> = (0..k)
                .map(|c| by_obs.iter().map(|row| row[c]).filter(|v| v.is_finite()).collect())
                .collect();
            match stats::hierarchical(&by_var, 0, p.clusters.max(2)) {
                Some(r) => hier_report(&r, names, p.clusters.max(2)),
                None => Vec::new(),
            }
        }
        20 => {
            let n_obs = cols.iter().map(|c| c.len()).max().unwrap_or(0);
            let k = cols.len();
            let by_obs: Vec<Vec<f64>> = (0..n_obs)
                .map(|r| (0..k).map(|c| cols[c].get(r).copied().unwrap_or(f64::NAN)).collect())
                .collect();
            let by_var: Vec<Vec<f64>> = (0..k)
                .map(|c| by_obs.iter().map(|row| row[c]).filter(|v| v.is_finite()).collect())
                .collect();
            match stats::kmeans(&by_var, p.clusters.max(2), 42) {
                Some(r) => kmeans_report(&r, &obs_names(n_obs), names),
                None => Vec::new(),
            }
        }
        21 => pairwise_report(cols, stats::MultiMethod::Tukey, p.conf),
        22 => pairwise_report(cols, stats::MultiMethod::GamesHowell, p.conf),
        23 => pairwise_report(cols, stats::MultiMethod::Dunnett, p.conf),
        24 => pairwise_report(cols, stats::MultiMethod::Dunn, p.conf),
        25 => variance_report(cols, names, p.alpha),
        30 => match stats::one_sample(cols.first().cloned().unwrap_or_default().as_slice(), p.mu0, p.conf) {
            Some(r) => onesample_report(&r, p.mu0, p.conf, p.alpha),
            None => Vec::new(),
        },
        31 => {
            if cols.len() >= 2 {
                match stats::paired_t(&cols[0], &cols[1], p.conf) {
                    Some(r) => paired_report(&r, p.conf, p.alpha),
                    None => Vec::new(),
                }
            } else {
                Vec::new()
            }
        }
        32 => {
            if cols.len() >= 2 {
                let xs = &cols[0];
                let ys = &cols[1];
                match stats::deming(xs, ys, p.deming_ratio) {
                    Some(r) => deming_report(&r, p.deming_ratio, xs, ys),
                    None => Vec::new(),
                }
            } else {
                Vec::new()
            }
        }
        33 => {
            if cols.len() >= 4 {
                match stats::compare_slopes(&cols[0], &cols[1], &cols[2], &cols[3]) {
                    Some(r) => slopes_report(&r, p.alpha),
                    None => Vec::new(),
                }
            } else {
                Vec::new()
            }
        }
        34 => {
            if cols.len() >= 2 {
                let time = cols[0].clone();
                let groups: Vec<(Vec<f64>, Vec<f64>)> = cols[1..]
                    .iter()
                    .map(|g| (time.clone(), g.clone()))
                    .collect();
                match stats::log_rank(&groups) {
                    Some(r) => logrank_report(&r, &names[1..], p.alpha),
                    None => Vec::new(),
                }
            } else {
                Vec::new()
            }
        }
        16 => total_score_report(names, cols),
        17 => gof_report(names, cols, p.alpha),
        26 => mannwhitney_report(names, cols, p.alpha),
        27 => wilcoxon_report(names, cols, p.alpha),
        28 => kruskal_report(names, cols, p.alpha),
        _ => Vec::new(),
    };

    if blocks.is_empty() {
        // 没有构建器的分析：保持原有输出，但包一层区块以便样式统一
        return wrap_fallback(fallback);
    }
    flatten(&blocks)
}

/// 观测名（聚类结果的观测标签用行号）。
fn obs_names(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("观测 {}", i + 1)).collect()
}

/// 把扁平的分组列还原成二维表（列联表用）。
fn flat_to_matrix(cols: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let rows = cols.first().map(|c| c.len()).unwrap_or(0);
    (0..rows)
        .map(|r| cols.iter().map(|c| c.get(r).copied().unwrap_or(0.0)).collect())
        .collect()
}

/// 方差齐性（菜单 id 25）的完整报告。
fn variance_report(cols: &[Vec<f64>], names: &[String], alpha: f64) -> Vec<Block> {
    let mut out = Vec::new();
    if let Some(l) = stats::levene(cols, false) {
        out.push(Block::kv(
            "结果摘要（Levene）",
            vec![
                ("分析方法", "Levene's test".to_string()),
                ("F 值", num(l.f)),
                ("自由度", format!("{}, {}", num(l.df1), num(l.df2))),
                ("P 值", pval(l.p)),
                ("结论（α）", verdict(l.p, alpha)),
            ],
        ));
    }
    out.push(bartlett_report(cols, names, alpha).remove(0));
    let mut all = out;
    all.push(variance_block(cols, alpha));
    all
}

/// 给没有构建器的结果套一层区块标题，样式与其它结果页一致。
pub fn wrap_fallback(rows: Vec<Vec<String>>) -> Vec<Vec<String>> {
    if rows.is_empty() {
        return rows;
    }
    // 第一行若已是表头（首格为空或为项目名），整体视为一个表格区块
    let mut out = vec![vec!["# 结果".to_string()]];
    if let Some(first) = rows.first() {
        let mut head = first.clone();
        if head.is_empty() {
            head.push("项目".to_string());
        }
        head[0] = format!("## {}", head[0]);
        out.push(head);
    }
    for r in rows.iter().skip(1) {
        out.push(r.clone());
    }
    out
}

// ── 智能统计制图顾问 ───────────────────────────────────────────────────────

/// 顾问报告的「正态性结论」：p 非有限时无法判定，不误导成「正态」。
fn norm_verdict(p: f64, alpha: f64) -> String {
    if !p.is_finite() {
        return "无法判定（样本量不足）".to_string();
    }
    if p < alpha {
        format!("非正态（P < {}）", alpha)
    } else {
        "未拒绝正态性".to_string()
    }
}

/// 智能统计制图顾问的完整报告。
///
/// 顾问是唯一「先看数据、再决定方法」的分析，所以报告的重点不是一个检验统计
/// 量，而是整条**决策链**：数据概况 → 正态性诊断 → 方差齐性 → 据此走哪条分支
/// → 推荐什么图、什么检验、怎么表达数据。每一步的依据都列出来，用户可以核对
/// 而不是盲信结论。
///
/// 正态性检验的选取与 `Diagnosis::diagnose` **完全同源**（先 D'Agostino–Pearson，
/// 失败再 Lilliefors），否则会出现「报告说非正态、决策树却当正态」的自相矛盾。
pub fn advisor_report(
    d: &crate::analysis::Diagnosis,
    r: &crate::analysis::Recommendation,
    names: &[String],
    cols: &[Vec<f64>],
    conf: f64,
    alpha: f64,
) -> Vec<Block> {
    use crate::analysis::Design;
    let mut out = Vec::new();

    // ── 1 数据概况 ────────────────────────────────────────────────────
    let total: usize = d.group_n.iter().sum();
    let min_n = d.group_n.iter().copied().min().unwrap_or(0);
    out.push(Block::kv(
        "数据概况",
        vec![
            ("组数", d.num_groups.to_string()),
            (
                "设计",
                if d.design == Design::Paired {
                    "配对 / 重复测量"
                } else {
                    "独立样本"
                }
                .to_string(),
            ),
            (
                "各组样本量",
                d.group_n
                    .iter()
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            ("总观测数", total.to_string()),
            ("最小组样本量", min_n.to_string()),
            ("显著性阈值 α", format!("{:.4}", alpha)),
            ("置信水平", conf_label(conf)),
        ],
    ));

    // ── 2 正态性诊断（逐组）────────────────────────────────────────────
    let mut nrows = Vec::new();
    for (i, c) in cols.iter().enumerate() {
        let nm = names.get(i).map(|x| x.as_str()).unwrap_or("—");
        let v: Vec<f64> = c.iter().copied().filter(|x| x.is_finite()).collect();
        let n = v.len();
        let (st_name, res) = match stats::dagostino(&v) {
            Some(x) => ("D'Agostino–Pearson K²", Some(x)),
            None => ("Lilliefors D", stats::lilliefors(&v)),
        };
        let (stat, p) = match res {
            Some(x) => (x.w, x.p),
            None => (f64::NAN, f64::NAN),
        };
        nrows.push(vec![
            nm.to_string(),
            n.to_string(),
            st_name.to_string(),
            num(stat),
            pval(p),
            norm_verdict(p, alpha),
        ]);
    }
    out.push(Block::new(
        "正态性诊断（逐组）",
        &["组", "样本量", "检验", "统计量", "P 值", "结论"],
        nrows,
    ));

    // ── 3 方差齐性 ────────────────────────────────────────────────────
    out.push(variance_block(cols, alpha));

    // ── 4 决策依据 ────────────────────────────────────────────────────
    out.push(Block::kv(
        "决策依据",
        vec![
            (
                "组数分支",
                if d.num_groups == 2 {
                    "两组"
                } else {
                    "三组及以上"
                }
                .to_string(),
            ),
            (
                "设计分支",
                if d.design == Design::Paired {
                    "配对 / 重复测量"
                } else {
                    "独立样本"
                }
                .to_string(),
            ),
            (
                "正态性（全部组）",
                if d.is_normal {
                    "满足"
                } else {
                    "不满足"
                }
                .to_string(),
            ),
            (
                "方差齐性",
                if d.equal_variance {
                    "满足"
                } else {
                    "不满足"
                }
                .to_string(),
            ),
            (
                "判定路径",
                format!(
                    "{} × {} × 正态{} × 方差{}",
                    if d.num_groups == 2 { "2 组" } else { "多组" },
                    if d.design == Design::Paired {
                        "配对"
                    } else {
                        "独立"
                    },
                    if d.is_normal { "✓" } else { "✗" },
                    if d.equal_variance { "✓" } else { "✗" },
                ),
            ),
        ],
    ));

    // ── 5 推荐方案 ────────────────────────────────────────────────────
    let mut plan = vec![
        ("推荐图表类型", r.chart_type.to_string()),
        ("推荐检验方法", r.test_method.to_string()),
        ("推荐数据表达", r.data_expression.to_string()),
    ];
    if d.num_groups >= 3 {
        plan.push((
            "推荐多重比较",
            if d.is_normal && d.equal_variance {
                "Tukey HSD"
            } else if d.is_normal {
                "Games-Howell"
            } else {
                "Dunn's test"
            }
            .to_string(),
        ));
    }
    out.push(Block::kv("推荐分析方案", plan));

    // ── 6 注意事项 ────────────────────────────────────────────────────
    let mut caut = vec![
        (
            "必须叠加原始散点",
            if r.must_overlay_points {
                "是"
            } else {
                "否"
            }
            .to_string(),
        ),
        (
            "禁用均值 ± 误差棒柱状图",
            if r.forbid_mean_bar {
                "是"
            } else {
                "否"
            }
            .to_string(),
        ),
    ];
    if let Some(w) = r.warning {
        caut.push(("避坑提示", w.to_string()));
    }
    out.push(Block::kv("注意事项", caut));

    // ── 7 描述统计 ────────────────────────────────────────────────────
    out.push(describe_block(names, cols, conf));

    // ── 8 按推荐方法给出实际检验结果 ──────────────────────────────────
    // 顾问不能只给建议不给数：这里按它推荐的检验真的跑一遍，
    // 用户可以直接看到结论，不必记住建议再回头去菜单里点。
    if d.num_groups >= 3 {
        let mm = if d.is_normal && d.equal_variance {
            stats::MultiMethod::Tukey
        } else if d.is_normal {
            stats::MultiMethod::GamesHowell
        } else {
            stats::MultiMethod::Dunn
        };
        let pw = match mm {
            stats::MultiMethod::GamesHowell => stats::games_howell(cols, conf),
            stats::MultiMethod::Dunn => stats::dunn_test(cols, conf),
            _ => stats::tukey_hsd(cols, conf),
        };
        if let Some(pw) = pw {
            out.push(pairwise_block(&pw, mm, conf));
        }
    } else if cols.len() >= 2 && d.is_normal && d.equal_variance {
        if let Some(mut t) = stats::t_test(&cols[0], &cols[1], stats::TTestKind::Pooled) {
            t.a = names.get(0).cloned().unwrap_or_else(|| "A 组".into());
            t.b = names.get(1).cloned().unwrap_or_else(|| "B 组".into());
            out.extend(ttest_report(&t, names, cols, false, conf, alpha));
        }
    }

    out
}
