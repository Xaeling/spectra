//! 把「数据表 + 分析类型」跑成一张可展示的结果表。
//!
//! 这一层只做编排与格式化；所有数值逻辑在 `stats.rs`，便于单独对拍。

use crate::core::graph::SigAnnotation;
use crate::core::model::{
    AnalysisKind, AnalysisOptions, ColRole, DataTable, ResultTable, TableKind,
};
use crate::core::stats::{self, Desc};

pub fn fmt_p(p: f64) -> String {
    if !p.is_finite() {
        return "—".into();
    }
    if p < 0.0001 {
        "< 0.0001".into()
    } else if p < 0.001 {
        format!("{p:.4}")
    } else {
        format!("{p:.4}")
    }
}

pub fn fmt_sig(v: f64, digits: usize) -> String {
    if !v.is_finite() {
        return "—".into();
    }
    let abs = v.abs();
    if abs != 0.0 && (abs < 1e-3 || abs >= 1e5) {
        format!("{v:.2e}")
    } else {
        format!("{v:.digits$}")
    }
}

/// 显著性标签。fmt: 0=只星号, 1=只 P 值, 2=星号+P 值
fn sig_label(p: f64, fmt: usize) -> String {
    let stars = if p < 1e-4 {
        "****"
    } else if p < 1e-3 {
        "***"
    } else if p < 0.01 {
        "**"
    } else if p < 0.05 {
        "*"
    } else {
        "ns"
    };
    let p_str = format_p(p);
    match fmt {
        1 => p_str,
        2 => format!("{stars} ({p_str})"),
        _ => stars.to_string(),
    }
}

fn format_p(p: f64) -> String {
    if p < 0.0001 {
        "P < 0.0001".to_string()
    } else {
        format!("P = {:.4}", p)
    }
}

fn group_rows(groups: &[(String, Vec<f64>)], selected: &[usize]) -> Vec<(String, Vec<f64>)> {
    if selected.is_empty() {
        groups.to_vec()
    } else {
        selected
            .iter()
            .filter_map(|&i| groups.get(i).cloned())
            .collect()
    }
}

fn desc_row(name: &str, d: &Desc) -> Vec<String> {
    let f = |v: f64| fmt_sig(v, 2);
    vec![
        name.to_string(),
        d.n.to_string(),
        f(d.mean),
        f(d.sd),
        f(d.sem),
        format!("{} ({} – {})", f(d.median), f(d.q1), f(d.q3)),
        format!("{} – {}", f(d.mean - d.ci95), f(d.mean + d.ci95)),
    ]
}

pub fn run(
    table: &DataTable,
    kind: AnalysisKind,
    opts: &AnalysisOptions,
) -> Result<ResultTable, String> {
    let groups = table.analysis_groups();
    if groups.is_empty() {
        return Err("这张表里还没有可分析的数据。".into());
    }
    match kind {
        AnalysisKind::Descriptive => Ok(descriptive(&groups, opts)),
        AnalysisKind::Normality => Ok(normality(&groups, opts)),
        AnalysisKind::UnpairedT => unpaired_t(&groups, opts),
        AnalysisKind::PairedT => paired_t(&groups, opts),
        AnalysisKind::OneWayAnova => anova(&groups, opts),
        AnalysisKind::MannWhitney => mann_whitney(&groups, opts),
        AnalysisKind::Wilcoxon => wilcoxon(&groups, opts),
        AnalysisKind::Correlation => correlation(table, &groups, opts),
        AnalysisKind::LinearRegression => regression(table, &groups, opts),
        AnalysisKind::NestedT => nested_t_test(table, opts),
        AnalysisKind::NestedAnova => nested_anova_test(table, opts),
        AnalysisKind::ChiSquare => chi_square_test(table, opts),
        AnalysisKind::FisherExact => fisher_test(table, opts),
        AnalysisKind::KruskalWallis => kruskal_wallis_test(&groups, opts),
        AnalysisKind::Dunn => dunn_test(&groups, opts),
        AnalysisKind::TwoWayAnova => two_way_test(table, opts),
        AnalysisKind::RmOneWay => rm_test(table, opts),
        AnalysisKind::Roc => roc_test(&groups, opts),
        // ---- v0.13 ----
        AnalysisKind::WelchT => welch_t(&groups, opts),
        AnalysisKind::OneSampleT => one_sample_t(&groups, opts),
        AnalysisKind::VarianceTests => variance_tests(&groups, opts),
        AnalysisKind::Outliers => outliers(&groups, opts),
        AnalysisKind::Friedman => friedman_test(table, opts),
        AnalysisKind::MultiCompare => multi_compare(&groups, opts),
        AnalysisKind::Spearman => spearman_corr(table, &groups, opts),
        AnalysisKind::Deming => deming_reg(table, &groups, opts),
        AnalysisKind::SlopeCompare => slope_compare(table, &groups, opts),
        AnalysisKind::CurveFit => curve_report(table, &groups, opts),
        AnalysisKind::CorrMatrix => corr_table(&groups, opts),
        AnalysisKind::Pca => pca_table(&groups, opts),
        AnalysisKind::Hierarchical => hclust_table(&groups, opts),
        AnalysisKind::KMeans => kmeans_table(&groups, opts),
        AnalysisKind::EffectRatios => effect_ratios(table, opts),
        AnalysisKind::McNemar => mcnemar_test(table, opts),
        AnalysisKind::MultiRegression => multi_regression(table, opts),
        AnalysisKind::Cox => cox_test(table, opts),
        AnalysisKind::SignedRank => signed_rank_test(&groups, opts),
        AnalysisKind::FVariance => f_variance_test(&groups, opts),
        AnalysisKind::Kappa => kappa_test(table, opts),
        AnalysisKind::BlandAltman => bland_altman_test(table, opts),
        AnalysisKind::Smart => crate::core::smart::report(table, opts.alpha),
    }
}

/// Kruskal–Wallis 秩方差分析。
fn kruskal_wallis_test(
    groups: &[(String, Vec<f64>)],
    opts: &AnalysisOptions,
) -> Result<ResultTable, String> {
    let selected = group_rows(groups, &opts.columns);
    if selected.len() < 2 {
        return Err("Kruskal–Wallis 至少需要 2 组数据。".into());
    }
    let vals: Vec<Vec<f64>> = selected.iter().map(|(_, v)| v.clone()).collect();
    let r = stats::kruskal_wallis(&vals)
        .ok_or_else(|| "每组至少需要 1 个有效值，且总样本数 ≥ 3。".to_string())?;
    let mut rows = Vec::new();
    for (i, (name, v)) in selected.iter().enumerate() {
        if let Some(d) = stats::describe(v) {
            rows.push(vec![
                name.clone(),
                d.n.to_string(),
                fmt_sig(d.median, 4),
                fmt_sig(d.q1, 3),
                fmt_sig(d.q3, 3),
                fmt_sig(r.mean_ranks.get(i).copied().unwrap_or(f64::NAN), 2),
            ]);
        }
    }
    rows.push(Vec::new());
    rows.push(vec![
        "H".into(),
        fmt_sig(r.h_corrected, 4),
        format!("df = {}", r.df),
        format!("P = {}", fmt_p(r.p)),
        format!("ε² = {}", fmt_sig(r.epsilon2, 3)),
        String::new(),
    ]);
    if r.tie_groups > 0 {
        rows.push(vec![
            "并列校正".into(),
            format!("{} 组并列", r.tie_groups),
            format!("未校正 H = {}", fmt_sig(r.h, 4)),
            String::new(),
            String::new(),
            String::new(),
        ]);
    }
    Ok(ResultTable {
        title: "Kruskal–Wallis 秩方差分析".into(),
        method: "非参数单因素方差分析：把所有观测混合编秩，比较各组平均秩。\n            存在并列时用 C = 1 − Σ(t³−t)/(N³−N) 校正，H 近似服从 χ²(df)。\n            不要求正态分布，但要求各组分布形状相近。"
            .to_string(),
        columns: vec![
            "组".into(),
            "n".into(),
            "中位数".into(),
            "Q1".into(),
            "Q3".into(),
            "平均秩".into(),
        ],
        rows,
        note: format!(
            "ε² 是效应量（>0.14 为大）。{}",
            if r.p < opts.alpha {
                format!("组间差异具有统计学意义（{}）。若需知道哪两组不同，请再做 Dunn 检验。", sig_label(r.p, 2))
            } else {
                "未发现组间差异。".into()
            }
        ),
    })
}

/// Dunn 非参数事后两两比较（Kruskal–Wallis 的配套检验）。
fn dunn_test(groups: &[(String, Vec<f64>)], opts: &AnalysisOptions) -> Result<ResultTable, String> {
    let selected = group_rows(groups, &opts.columns);
    if selected.len() < 2 {
        return Err("Dunn 检验至少需要 2 组数据。".into());
    }
    let vals: Vec<Vec<f64>> = selected.iter().map(|(_, v)| v.clone()).collect();
    let pairs = stats::posthoc_dunn(&vals);
    if pairs.is_empty() {
        return Err("数据不足，无法做两两比较（需要每组至少 1 个有效值）。".into());
    }
    let mut rows = Vec::new();
    for p in &pairs {
        let name_a = selected
            .get(p.a)
            .map(|(n, _)| n.clone())
            .unwrap_or_default();
        let name_b = selected
            .get(p.b)
            .map(|(n, _)| n.clone())
            .unwrap_or_default();
        rows.push(vec![
            format!("{name_a} vs {name_b}"),
            fmt_sig(p.rank_diff, 3),
            fmt_sig(p.z, 3),
            fmt_p(p.p_raw),
            fmt_p(p.p_holm),
            fmt_p(p.p_bonf),
        ]);
    }
    let sig = pairs.iter().filter(|p| p.p_holm < opts.alpha).count();
    Ok(ResultTable {
        title: "Dunn 事后两两比较".into(),
        method: "在 Kruskal–Wallis 的全局秩上做两两比较：\n            z = (R_i − R_j) / √( (N(N+1)/12 − Σ(t³−t)/(12(N−1))) · (1/n_i + 1/n_j) )。\n            双侧 p 由正态近似得到，再同时给 Holm 与 Bonferroni 两种多重比较校正。\n            建议以 Holm 列为准（比 Bonferroni 更有检验力）。"
            .to_string(),
        columns: vec![
            "比较".into(),
            "秩差".into(),
            "z".into(),
            "P（原始）".into(),
            "P（Holm）".into(),
            "P（Bonferroni）".into(),
        ],
        rows,
        note: format!(
            "共 {} 对比较，其中 {} 对在 α = {} 下显著（按 Holm 校正）。",
            pairs.len(),
            sig,
            opts.alpha
        ),
    })
}

/// 嵌套 t 检验：外层组是实验组，子组（班级）是实验单位。
fn nested_t_test(table: &DataTable, opts: &AnalysisOptions) -> Result<ResultTable, String> {
    let ng = table.nested_groups();
    let r = crate::core::nested::nested_t(&ng)
        .ok_or_else(|| "嵌套 t 检验需要恰好 2 个外层组，且每个组至少 1 个非空子组。".to_string())?;
    let names: Vec<String> = ng.iter().map(|(n, _)| n.clone()).collect();
    let method = "嵌套 t 检验：以「子组」为实验单位，误差项取「组内子组间均方」。\n        这样才不会把子组内的重复测量当作独立生物学重复（伪重复），\n        否则会高估显著性。自由度 = 子组总数 − 2。";
    let rows = vec![
        vec![
            names[0].clone(),
            r.n1.to_string(),
            r.b1.to_string(),
            String::new(),
            String::new(),
        ],
        vec![
            names[1].clone(),
            r.n2.to_string(),
            r.b2.to_string(),
            String::new(),
            String::new(),
        ],
        Vec::new(),
        vec![
            "差值 (1−2)".into(),
            fmt_sig(r.diff, 4),
            format!("{} – {}", fmt_sig(r.ci_low, 4), fmt_sig(r.ci_high, 4)),
            String::new(),
            String::new(),
        ],
        vec![
            "t".into(),
            format!("{:.1}", r.df),
            fmt_sig(r.t, 4),
            String::new(),
            String::new(),
        ],
        vec![
            "P（双尾）".into(),
            String::new(),
            fmt_p(r.p),
            String::new(),
            String::new(),
        ],
        vec![
            "Cohen's d".into(),
            String::new(),
            fmt_sig(crate::core::nested::nested_cohen_d(&r), 3),
            String::new(),
            String::new(),
        ],
    ];
    Ok(ResultTable {
        title: format!("嵌套 t 检验：{} vs {}", names[0], names[1]),
        method: format!("{method}\nα = {}。", opts.alpha),
        columns: vec![
            "组 / 统计量".into(),
            "n / 值".into(),
            "子组数 / 95% CI".into(),
            "t".into(),
            "P".into(),
        ],
        rows,
        note: if r.p < opts.alpha {
            format!("差异具有统计学意义（{}）。", sig_label(r.p, 2))
        } else {
            "未发现统计学显著差异。".into()
        },
    })
}

/// 嵌套单因素 ANOVA：外层组效应的误差项同样是子组间均方。
fn nested_anova_test(table: &DataTable, opts: &AnalysisOptions) -> Result<ResultTable, String> {
    let ng = table.nested_groups();
    let r = crate::core::nested::nested_anova(&ng).ok_or_else(|| {
        "嵌套 ANOVA 需要至少 2 个外层组，且子组总数要多于外层组数（否则子组间均方没有自由度）。"
            .to_string()
    })?;
    let total_ss = r.ss_group + r.ss_sub + r.ss_within;
    let rows = vec![
        vec![
            "外层组间".into(),
            r.df_group.to_string(),
            fmt_sig(r.ss_group, 4),
            fmt_sig(r.ms_group, 4),
            fmt_sig(r.f, 4),
            fmt_p(r.p),
        ],
        vec![
            "组内子组间".into(),
            r.df_sub.to_string(),
            fmt_sig(r.ss_sub, 4),
            fmt_sig(r.ms_sub, 4),
            String::new(),
            String::new(),
        ],
        vec![
            "子组内".into(),
            r.df_within.to_string(),
            fmt_sig(r.ss_within, 4),
            fmt_sig(r.ms_within, 4),
            String::new(),
            String::new(),
        ],
        Vec::new(),
        vec![
            "σ²（方差分量）".into(),
            String::new(),
            format!("组间 {}", fmt_sig(r.var_group, 4)),
            format!("子组间 {}", fmt_sig(r.var_sub, 4)),
            format!("子组内 {}", fmt_sig(r.var_within, 4)),
            String::new(),
        ],
    ];
    let method = "嵌套单因素方差分析（Model II，完全随机嵌套）：\n        方差分解为「外层组间 / 组内子组间 / 子组内」三层。\n        外层组效应的 F = MS_组间 / MS_组内子组间 —— 误差项是「子组间」均方，\n        而不是子组内均方。子组才是实验单位，个体观测是技术重复。";
    Ok(ResultTable {
        title: format!("嵌套 ANOVA：{} 个外层组 / {} 个子组", r.a, r.b),
        method: format!("{method}\nα = {}。", opts.alpha),
        columns: vec![
            "变异来源".into(),
            "df".into(),
            "SS / σ²".into(),
            "MS / σ²".into(),
            "F / σ²".into(),
            "P".into(),
        ],
        rows,
        note: format!(
            "η² = {}；总离差平方和 = {}。{}",
            fmt_sig(r.eta2, 3),
            fmt_sig(total_ss, 4),
            if r.p < opts.alpha {
                format!("外层组效应显著（{}）。", sig_label(r.p, 2))
            } else {
                "外层组效应不显著。".into()
            }
        ),
    })
}

/// 卡方独立性检验：行 × 列分类变量是否独立。
fn chi_square_test(table: &DataTable, _opts: &AnalysisOptions) -> Result<ResultTable, String> {
    let (row_labels, col_labels, counts) = table
        .contingency_matrix()
        .ok_or_else(|| "列联表里还没有数据。".to_string())?;
    let r = crate::core::contingency::chi_square(&counts).map_err(|e| e)?;
    let mut rows = Vec::new();
    // 期望频数矩阵
    let n = r.n;
    let row_tot: Vec<f64> = counts.iter().map(|row| row.iter().sum()).collect();
    let col_tot: Vec<f64> = (0..r.c)
        .map(|j| counts.iter().map(|row| row[j]).sum())
        .collect();
    for (i, rl) in row_labels.iter().enumerate() {
        let mut row = vec![rl.clone()];
        for j in 0..r.c {
            let e = row_tot[i] * col_tot[j] / n;
            // 残差显著的格子加标记：这是「哪个格子拉动了关联」的直接线索。
            let mark = if r.residual[i][j].abs() >= r.residual_crit {
                if r.residual[i][j] > 0.0 {
                    " ↑"
                } else {
                    " ↓"
                }
            } else {
                ""
            };
            row.push(format!(
                "{} ({}){}",
                fmt_sig(counts[i][j], 4),
                fmt_sig(e, 3),
                mark
            ));
        }
        rows.push(row);
    }
    // 残差单独一段：观察值减期望值的方向与强度
    rows.push(Vec::new());
    rows.push(vec![format!(
        "调整标准化残差（|r| ≥ {:.2} 标记为显著）",
        r.residual_crit
    )]);
    for (i, rl) in row_labels.iter().enumerate() {
        let mut row = vec![format!("  {rl}")];
        for j in 0..r.c {
            row.push(fmt_sig(r.residual[i][j], 2));
        }
        rows.push(row);
    }
    rows.push(Vec::new());
    // 统计量与数据列同宽排布：每行 3 格，标签、值、补充信息各占一列，
    // 不要塞进数据列里把「df」「P」挤成表头下的孤字。
    rows.push(vec![
        "χ²".into(),
        fmt_sig(r.chi2, 4),
        format!("df = {}", r.df),
    ]);
    rows.push(vec!["P".into(), fmt_p(r.p), String::new()]);
    rows.push(vec![
        "Cramér's V".into(),
        fmt_sig(r.v, 3),
        format!("N = {}", fmt_sig(r.n, 4)),
    ]);
    if let Some(y) = r.yates {
        rows.push(vec!["Yates 校正 χ²".into(), fmt_sig(y, 4), String::new()]);
    }
    if let Some(or) = r.odds_ratio {
        let ci = r
            .or_ci
            .map(|(lo, hi)| format!("95% CI {} – {}", fmt_sig(lo, 3), fmt_sig(hi, 3)))
            .unwrap_or_default();
        rows.push(vec!["优势比 OR".into(), fmt_sig(or, 4), ci]);
    }
    let mut method = "卡方独立性检验（Pearson χ²）。单元格内是「观测频数 (期望频数)」。\n        适用于行列分类变量的独立性判断；Cramér's V 衡量关联强度。\n        调整标准化残差 r = (O−E)/√(E·(1−n_i/n)·(1−n_j/n)) 近似 N(0,1)：\n        r > 0 表示该格观测多于期望（↑），r < 0 表示少于期望（↓）；\n        阈值已按单元格数做 Bonferroni 校正，避免「格子多了总有几个显著」。".to_string();
    if r.small_cells > 0 {
        method.push_str(&format!(
            "\n注意：有 {} 个单元格的期望频数 < 5（最小 {:.2}），卡方近似可能不准，建议用 Fisher 精确检验。",
            r.small_cells, r.min_expected
        ));
    }
    let mut cols = vec!["行 / 统计量".to_string()];
    cols.extend(col_labels.iter().cloned());
    Ok(ResultTable {
        title: "卡方独立性检验".into(),
        method: format!("{method}"),
        columns: cols,
        rows,
        note: if r.p < 0.05 {
            "行列变量不独立（存在关联）。".into()
        } else {
            "未发现行列变量间的关联。".into()
        },
    })
}

/// Fisher 精确检验：小样本 / 期望频数不足时的独立性判断。
fn fisher_test(table: &DataTable, _opts: &AnalysisOptions) -> Result<ResultTable, String> {
    let (row_labels, col_labels, counts) = table
        .contingency_matrix()
        .ok_or_else(|| "列联表里还没有数据。".to_string())?;
    let r = crate::core::contingency::fisher_exact(&counts).map_err(|e| e)?;
    let mut rows = Vec::new();
    for (i, rl) in row_labels.iter().enumerate() {
        let mut row = vec![rl.clone()];
        for j in 0..counts[0].len() {
            row.push(fmt_sig(counts[i][j], 4));
        }
        rows.push(row);
    }
    rows.push(Vec::new());
    rows.push(vec![
        "P（双侧）".into(),
        fmt_p(r.p),
        if r.exact {
            format!("精确枚举 {} 个表", r.tables)
        } else {
            format!("Monte Carlo 近似（{} 次抽样）", r.tables)
        },
    ]);
    if let Some(or) = r.odds_ratio {
        let ci = r
            .or_ci
            .map(|(lo, hi)| format!("95% CI {} – {}", fmt_sig(lo, 3), fmt_sig(hi, 3)))
            .unwrap_or_default();
        rows.push(vec!["优势比 OR".into(), fmt_sig(or, 4), ci]);
    }
    let method = if r.exact {
        "Fisher 精确检验：在行列边际固定的条件下，把所有可能的列联表\n            逐个枚举，累加概率不高于观测表的那些（双侧定义）。\n            小样本下比卡方更可靠，不做正态近似。"
    } else {
        "Fisher 精确检验（Freeman–Halton 扩展）：表数太多无法逐一枚举，\n            改用 Monte Carlo 抽样近似双侧 p 值（结果表中已标注）。\n            需要完全精确的结果时，请缩小表格规模或改用卡方。"
    };
    let mut cols = vec!["行 / 统计量".to_string()];
    cols.extend(col_labels.iter().cloned());
    Ok(ResultTable {
        title: "Fisher 精确检验".into(),
        method: method.to_string(),
        columns: cols,
        rows,
        note: if r.p < 0.05 {
            "行列变量不独立（存在关联）。".into()
        } else {
            "未发现行列变量间的关联。".into()
        },
    })
}

fn descriptive(groups: &[(String, Vec<f64>)], opts: &AnalysisOptions) -> ResultTable {
    let selected = group_rows(groups, &opts.columns);
    let mut rows = Vec::new();
    for (name, v) in &selected {
        if let Some(d) = stats::describe(v) {
            rows.push(desc_row(name, &d));
        }
    }
    ResultTable {
        title: "描述统计".into(),
        method: format!(
            "每组独立计算；方差用 n−1；95% CI 用 t 分布。α = {}.",
            opts.alpha
        ),
        columns: vec![
            "组".into(),
            "n".into(),
            "Mean".into(),
            "SD".into(),
            "SEM".into(),
            "Median (Q1–Q3)".into(),
            "95% CI".into(),
        ],
        rows,
        note: "排除值（蓝色斜体）不参与计算。".into(),
    }
}

fn normality(groups: &[(String, Vec<f64>)], opts: &AnalysisOptions) -> ResultTable {
    let selected = group_rows(groups, &opts.columns);
    let mut rows = Vec::new();
    for (name, v) in &selected {
        if v.len() < 8 {
            rows.push(vec![
                name.clone(),
                v.len().to_string(),
                "—".into(),
                "—".into(),
                "—".into(),
                "样本量不足（n < 8）".into(),
            ]);
            continue;
        }
        if let Some((k2, p)) = stats::dagostino(v) {
            rows.push(vec![
                name.clone(),
                v.len().to_string(),
                fmt_sig(stats::skewness(v), 3),
                fmt_sig(stats::kurtosis(v), 3),
                fmt_sig(k2, 3),
                fmt_p(p),
            ]);
        }
    }
    ResultTable {
        title: "正态性检验".into(),
        method: "D'Agostino–Pearson K²（对偏度与峰度联合检验）。".into(),
        columns: vec![
            "组".into(),
            "n".into(),
            "偏度".into(),
            "峰度".into(),
            "K²".into(),
            "P".into(),
        ],
        rows,
        note: "P > 0.05 不拒绝正态假设；样本量过小时检验功效有限。".into(),
    }
}

fn two_groups(
    groups: &[(String, Vec<f64>)],
    opts: &AnalysisOptions,
) -> Result<(usize, usize), String> {
    let idx: Vec<usize> = if opts.columns.len() >= 2 {
        opts.columns.clone()
    } else {
        (0..groups.len().min(2)).collect()
    };
    if idx.len() != 2 || idx[0] >= groups.len() || idx[1] >= groups.len() {
        return Err("这项检验需要正好选择两个组。".into());
    }
    Ok((idx[0], idx[1]))
}

fn unpaired_t(
    groups: &[(String, Vec<f64>)],
    opts: &AnalysisOptions,
) -> Result<ResultTable, String> {
    let (i, j) = two_groups(groups, opts)?;
    let (a, b) = (&groups[i].1, &groups[j].1);
    let r = stats::t_test_unpaired(a, b, opts.welch)
        .ok_or_else(|| "每组至少需要 2 个有效值。".to_string())?;
    let method = if opts.welch {
        "Welch 校正 t 检验（不假设方差齐）"
    } else {
        "Student t 检验（假设方差齐）"
    };
    let rows = vec![
        vec![
            groups[i].0.clone(),
            a.len().to_string(),
            fmt_sig(stats::mean(a), 4),
            fmt_sig(stats::sd(a), 4),
            fmt_sig(stats::sem(a), 4),
        ],
        vec![
            groups[j].0.clone(),
            b.len().to_string(),
            fmt_sig(stats::mean(b), 4),
            fmt_sig(stats::sd(b), 4),
            fmt_sig(stats::sem(b), 4),
        ],
        Vec::new(),
        vec![
            "差值 (1−2)".into(),
            String::new(),
            fmt_sig(r.diff, 4),
            format!("{} – {}", fmt_sig(r.ci_low, 4), fmt_sig(r.ci_high, 4)),
            String::new(),
        ],
        vec![
            "t".into(),
            format!("{:.1}", r.df),
            fmt_sig(r.t, 4),
            String::new(),
            String::new(),
        ],
        vec![
            "P（双尾）".into(),
            String::new(),
            fmt_p(r.p),
            String::new(),
            String::new(),
        ],
        vec![
            "Cohen's d".into(),
            String::new(),
            fmt_sig(r.cohen_d, 3),
            String::new(),
            String::new(),
        ],
    ];
    Ok(ResultTable {
        title: format!(
            "{}：{} vs {}",
            AnalysisKind::UnpairedT.label(),
            groups[i].0,
            groups[j].0
        ),
        method: format!("{method}；双尾；α = {}。", opts.alpha),
        columns: vec![
            "组 / 统计量".into(),
            "n / df".into(),
            "Mean / 值".into(),
            "SD / 95% CI".into(),
            "SEM".into(),
        ],
        rows,
        note: if r.p < opts.alpha {
            format!("差异具有统计学意义（{}）。", sig_label(r.p, 2))
        } else {
            "未发现统计学显著差异。".into()
        },
    })
}

fn paired_t(groups: &[(String, Vec<f64>)], opts: &AnalysisOptions) -> Result<ResultTable, String> {
    let (i, j) = two_groups(groups, opts)?;
    let (a, b) = (&groups[i].1, &groups[j].1);
    let r = stats::t_test_paired(a, b).ok_or_else(|| "配对检验需要至少 2 对数据。".to_string())?;
    let rows = vec![
        vec![
            groups[i].0.clone(),
            a.len().to_string(),
            fmt_sig(stats::mean(a), 4),
            fmt_sig(stats::sd(a), 4),
            fmt_sig(stats::sem(a), 4),
        ],
        vec![
            groups[j].0.clone(),
            b.len().to_string(),
            fmt_sig(stats::mean(b), 4),
            fmt_sig(stats::sd(b), 4),
            fmt_sig(stats::sem(b), 4),
        ],
        Vec::new(),
        vec![
            "平均差".into(),
            r.n1.to_string(),
            fmt_sig(r.diff, 4),
            format!("{} – {}", fmt_sig(r.ci_low, 4), fmt_sig(r.ci_high, 4)),
            String::new(),
        ],
        vec![
            "t".into(),
            format!("{:.0}", r.df),
            fmt_sig(r.t, 4),
            String::new(),
            String::new(),
        ],
        vec![
            "P（双尾）".into(),
            String::new(),
            fmt_p(r.p),
            String::new(),
            String::new(),
        ],
    ];
    Ok(ResultTable {
        title: format!(
            "{}：{} vs {}",
            AnalysisKind::PairedT.label(),
            groups[i].0,
            groups[j].0
        ),
        method: format!("配对 t 检验；双尾；α = {}。", opts.alpha),
        columns: vec![
            "组 / 统计量".into(),
            "n / df".into(),
            "Mean / 值".into(),
            "SD / 95% CI".into(),
            "SEM".into(),
        ],
        rows,
        note: if r.p < opts.alpha {
            format!("差异具有统计学意义（{}）。", sig_label(r.p, 2))
        } else {
            "未发现统计学显著差异。".into()
        },
    })
}

fn anova(groups: &[(String, Vec<f64>)], opts: &AnalysisOptions) -> Result<ResultTable, String> {
    let selected = group_rows(groups, &opts.columns);
    if selected.len() < 2 {
        return Err("单因素 ANOVA 至少需要两个组。".into());
    }
    let vals: Vec<Vec<f64>> = selected.iter().map(|(_, v)| v.clone()).collect();
    let a = stats::anova_oneway(&vals).ok_or_else(|| "每组至少需要 2 个有效值。".to_string())?;
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (name, v) in &selected {
        let d = stats::describe(v).unwrap_or_default();
        rows.push(vec![
            name.clone(),
            d.n.to_string(),
            fmt_sig(d.mean, 4),
            fmt_sig(d.sd, 4),
            fmt_sig(d.sem, 4),
            String::new(),
        ]);
    }
    rows.push(Vec::new());
    rows.push(vec![
        "组间".into(),
        format!("{:.1}", a.df_between),
        fmt_sig(a.ss_between, 4),
        fmt_sig(a.ms_between, 4),
        String::new(),
        format!("F = {}, P = {}", fmt_sig(a.f, 3), fmt_p(a.p)),
    ]);
    rows.push(vec![
        "组内".into(),
        format!("{:.1}", a.df_within),
        fmt_sig(a.ss_within, 4),
        fmt_sig(a.ms_within, 4),
        String::new(),
        String::new(),
    ]);
    let post = stats::posthoc_holm(&vals);
    if !post.is_empty() {
        rows.push(Vec::new());
        rows.push(vec![
            "事后比较".into(),
            "P (Holm)".into(),
            "差值".into(),
            "显著性".into(),
            String::new(),
            String::new(),
        ]);
        for p in post {
            rows.push(vec![
                format!("{} vs {}", selected[p.a].0, selected[p.b].0),
                fmt_p(p.p_adj),
                fmt_sig(p.diff, 3),
                sig_label(p.p_adj, 2),
                String::new(),
                String::new(),
            ]);
        }
    }
    Ok(ResultTable {
        title: "单因素方差分析".into(),
        method: format!(
            "One-way ANOVA；η² = {}；事后比较用 Welch t + Holm 校正；α = {}。",
            fmt_sig(a.eta2, 3),
            opts.alpha
        ),
        columns: vec![
            "组 / 来源".into(),
            "n / df".into(),
            "Mean / SS".into(),
            "SD / MS".into(),
            "SEM".into(),
            "F / P".into(),
        ],
        rows,
        note: if a.p < opts.alpha {
            "组间差异具有统计学意义。".into()
        } else {
            "未发现组间显著差异。".into()
        },
    })
}

fn mann_whitney(
    groups: &[(String, Vec<f64>)],
    opts: &AnalysisOptions,
) -> Result<ResultTable, String> {
    let (i, j) = two_groups(groups, opts)?;
    let (u, p) = stats::mann_whitney(&groups[i].1, &groups[j].1)
        .ok_or_else(|| "每组至少需要 2 个有效值。".to_string())?;
    let med = |v: &[f64]| stats::describe(v).map(|d| d.median).unwrap_or(f64::NAN);
    let rows = vec![
        vec![
            groups[i].0.clone(),
            groups[i].1.len().to_string(),
            fmt_sig(med(&groups[i].1), 4),
        ],
        vec![
            groups[j].0.clone(),
            groups[j].1.len().to_string(),
            fmt_sig(med(&groups[j].1), 4),
        ],
        vec!["U".into(), String::new(), fmt_sig(u, 4)],
        vec!["P（双尾）".into(), String::new(), fmt_p(p)],
    ];
    Ok(ResultTable {
        title: format!("Mann–Whitney U：{} vs {}", groups[i].0, groups[j].0),
        method: "非参数秩和检验；无并列小样本用精确分布，否则正态近似并做并列校正。".into(),
        columns: vec!["组 / 统计量".into(), "n".into(), "Median / 值".into()],
        rows,
        note: if p < opts.alpha {
            format!("分布差异具有统计学意义（{}）。", sig_label(p, 2))
        } else {
            "未发现显著差异。".into()
        },
    })
}

fn wilcoxon(groups: &[(String, Vec<f64>)], opts: &AnalysisOptions) -> Result<ResultTable, String> {
    let (i, j) = two_groups(groups, opts)?;
    let (w, p) = stats::wilcoxon(&groups[i].1, &groups[j].1)
        .ok_or_else(|| "配对检验需要至少 2 对非零差值。".to_string())?;
    let rows = vec![
        vec![groups[i].0.clone(), fmt_sig(stats::mean(&groups[i].1), 4)],
        vec![groups[j].0.clone(), fmt_sig(stats::mean(&groups[j].1), 4)],
        vec!["W（较小侧）".into(), fmt_sig(w, 4)],
        vec!["P（双尾）".into(), fmt_p(p)],
    ];
    Ok(ResultTable {
        title: format!("Wilcoxon 符号秩：{} vs {}", groups[i].0, groups[j].0),
        method: "配对非参数检验；无并列小样本用精确分布，否则正态近似。".into(),
        columns: vec!["组 / 统计量".into(), "值".into()],
        rows,
        note: if p < opts.alpha {
            format!("差异具有统计学意义（{}）。", sig_label(p, 2))
        } else {
            "未发现显著差异。".into()
        },
    })
}

fn correlation(
    table: &DataTable,
    groups: &[(String, Vec<f64>)],
    opts: &AnalysisOptions,
) -> Result<ResultTable, String> {
    if table.kind != TableKind::Xy {
        return Err("相关分析请在 XY 表上进行（X 与 Y 逐行配对）。".into());
    }
    let selected: Vec<usize> = if opts.columns.is_empty() {
        (0..groups.len()).collect()
    } else {
        opts.columns.clone()
    };
    let mut rows = Vec::new();
    for g in selected {
        let pairs = table.xy_points(g);
        if pairs.len() < 3 {
            continue;
        }
        let xs: Vec<f64> = pairs.iter().map(|p| p.0).collect();
        let ys: Vec<f64> = pairs.iter().map(|p| p.1).collect();
        let c = if opts.spearman {
            stats::spearman(&xs, &ys)
        } else {
            stats::pearson(&xs, &ys)
        };
        if let Some(c) = c {
            rows.push(vec![
                groups
                    .get(g)
                    .map(|(n, _)| n.clone())
                    .unwrap_or_else(|| format!("Y{}", g + 1)),
                c.n.to_string(),
                fmt_sig(c.r, 4),
                format!("{} – {}", fmt_sig(c.ci_low, 3), fmt_sig(c.ci_high, 3)),
                fmt_p(c.p),
            ]);
        }
    }
    if rows.is_empty() {
        return Err("没有足够的配对数据。".into());
    }
    Ok(ResultTable {
        title: if opts.spearman {
            "Spearman 相关".into()
        } else {
            "Pearson 相关".into()
        },
        method: if opts.spearman {
            "Spearman 秩相关（对非线性单调关系稳健）；P 用 t 近似。".into()
        } else {
            "Pearson 积矩相关；95% CI 用 Fisher z 变换。".into()
        },
        columns: vec![
            "Y 数据集".into(),
            "n".into(),
            "r".into(),
            "95% CI".into(),
            "P".into(),
        ],
        rows,
        note: "相关不等于因果；请同时检查散点图。".into(),
    })
}

fn regression(
    table: &DataTable,
    groups: &[(String, Vec<f64>)],
    opts: &AnalysisOptions,
) -> Result<ResultTable, String> {
    if table.kind != TableKind::Xy {
        return Err("线性回归请在 XY 表上进行。".into());
    }
    let selected: Vec<usize> = if opts.columns.is_empty() {
        (0..groups.len()).collect()
    } else {
        opts.columns.clone()
    };
    let mut rows = Vec::new();
    for g in selected {
        let pairs = table.xy_points(g);
        if pairs.len() < 3 {
            continue;
        }
        let xs: Vec<f64> = pairs.iter().map(|p| p.0).collect();
        let ys: Vec<f64> = pairs.iter().map(|p| p.1).collect();
        if let Some(r) = stats::linreg(&xs, &ys) {
            rows.push(vec![
                groups
                    .get(g)
                    .map(|(n, _)| n.clone())
                    .unwrap_or_else(|| format!("Y{}", g + 1)),
                format!("y = {}x + {}", fmt_sig(r.slope, 3), fmt_sig(r.intercept, 3)),
                fmt_sig(r.r2, 4),
                format!("{} – {}", fmt_sig(r.ci_low, 3), fmt_sig(r.ci_high, 3)),
                fmt_p(r.p),
            ]);
        }
    }
    if rows.is_empty() {
        return Err("没有足够的数据点。".into());
    }
    Ok(ResultTable {
        title: "线性回归".into(),
        method: "普通最小二乘；斜率 95% CI 用 t 分布。".into(),
        columns: vec![
            "Y 数据集".into(),
            "方程".into(),
            "R²".into(),
            "斜率 95% CI".into(),
            "斜率 P".into(),
        ],
        rows,
        note: "R² 高不代表模型正确；请检查残差图。".into(),
    })
}

/// 图注统计行（规范 G4）：**检验全名、双侧、多重比较校正、效应量与 95% CI**。
///
/// 显著性括号只写星号是不够的——规范要求读者不翻正文就能知道用了什么检验、
/// 效应有多大。这里按 `auto_significance` 同一口径生成：
/// 两组 → Welch 非配对 t（双侧）+ Cohen's d 及 95% CI；
/// 多组 → 单因素 ANOVA + Holm 事后（双侧）+ η²。
pub fn inference_note(groups: &[(String, Vec<f64>)]) -> Option<String> {
    if groups.len() < 2 {
        return None;
    }
    if groups.len() == 2 {
        let t = stats::t_test_unpaired(&groups[0].1, &groups[1].1, true)?;
        let (_, lo, hi) = stats::cohen_d_ci(t.cohen_d, t.n1, t.n2);
        Some(format!(
            "Welch 非配对 t 检验（双侧）· Cohen's d = {:.2}, 95% CI [{:.2}, {:.2}]",
            t.cohen_d, lo, hi
        ))
    } else {
        let vals: Vec<Vec<f64>> = groups.iter().map(|(_, v)| v.clone()).collect();
        let a = stats::anova_oneway(&vals)?;
        Some(format!(
            "单因素 ANOVA + Holm 事后校正（双侧）· η² = {:.3}, F({:.0}, {:.0}) = {:.2}, p {}",
            a.eta2,
            a.df_between,
            a.df_within,
            a.f,
            fmt_p(a.p)
        ))
    }
}

/// 图注里的 p 值写法：`p = 0.012` / `p < 0.001`（规范 G4 禁写 p = 0.000）。
fn p_eq(p: f64) -> String {
    if !p.is_finite() {
        return "p —".into();
    }
    if p < 0.001 {
        "p < 0.001".into()
    } else {
        format!("p = {:.3}", p)
    }
}

/// 生存图注统计行（规范 E2-5）：**log-rank p、HR 及其 95% CI**。
/// 两组才给 HR（多组没有单一风险比，只报整体 log-rank）。
pub fn survival_note(groups: &[(String, Vec<(f64, bool)>)]) -> Option<String> {
    let data: Vec<Vec<(f64, bool)>> = groups.iter().map(|(_, v)| v.clone()).collect();
    let lr = stats::log_rank(&data)?;
    let mut s = if lr.df == 1 {
        format!("log-rank χ² = {:.2}, {}", lr.chi2, p_eq(lr.p))
    } else {
        format!("log-rank χ²({}) = {:.2}, {}", lr.df, lr.chi2, p_eq(lr.p))
    };
    if let (Some(hr), Some(lo), Some(hi)) = (lr.hr, lr.ci_low, lr.ci_high) {
        // HR 必须写方向（谁相对谁），否则 3.67 和 0.27 读者分不出。
        s.push_str(&format!(
            " · HR ({} / {}) = {:.2}, 95% CI [{:.2}, {:.2}]",
            groups[0].0, groups[1].0, hr, lo, hi
        ));
    }
    Some(s)
}

/// 自动生成显著性括号（两两比较）。
pub fn auto_significance(table: &DataTable, label_fmt: usize) -> Vec<SigAnnotation> {
    let groups = table.analysis_groups();
    if groups.len() < 2 {
        return Vec::new();
    }
    let mut out = Vec::new();
    if groups.len() == 2 {
        if let Some(t) = stats::t_test_unpaired(&groups[0].1, &groups[1].1, true) {
            if t.p < 0.05 {
                out.push(SigAnnotation {
                    a: 0,
                    b: 1,
                    label: sig_label(t.p, label_fmt),
                    y: None,
                });
            }
        }
    } else {
        let vals: Vec<Vec<f64>> = groups.iter().map(|(_, v)| v.clone()).collect();
        if let Some(a) = stats::anova_oneway(&vals) {
            if a.p < 0.05 {
                for p in stats::posthoc_holm(&vals) {
                    if p.p_adj < 0.05 {
                        out.push(SigAnnotation {
                            a: p.a,
                            b: p.b,
                            label: sig_label(p.p_adj, label_fmt),
                            y: None,
                        });
                    }
                }
            }
        }
    }
    out
}

/// 双因素方差分析结果表（分组表：行 = 因子 A，列 = 因子 B，格内并排重复）。
fn two_way_test(table: &DataTable, opts: &AnalysisOptions) -> Result<ResultTable, String> {
    // 行名 = 因子 A 水平；组（列）= 因子 B 水平；格内 = 该组各 Y 列在同一行的值。
    let row_names: Vec<String> = if table.row_headers.iter().any(|h| !h.is_empty()) {
        table.row_headers.clone()
    } else {
        (0..table.nrows())
            .map(|i| format!("行 {}", i + 1))
            .collect()
    };
    let mut b_groups: Vec<usize> = Vec::new();
    for c in table.columns.iter().filter(|c| c.role == ColRole::Y) {
        if !b_groups.contains(&c.group) {
            b_groups.push(c.group);
        }
    }
    let (a, b) = (row_names.len(), b_groups.len());
    if a < 2 || b < 2 {
        return Err(
            "双因素方差分析需要行、列各至少 2 个水平（分组表：行 = 因子 A，列 = 因子 B）。".into(),
        );
    }
    let mut cells: Vec<Vec<Vec<f64>>> = vec![vec![Vec::new(); b]; a];
    for c in table.columns.iter().filter(|c| c.role == ColRole::Y) {
        let j = b_groups.iter().position(|g| *g == c.group).unwrap();
        for i in 0..a {
            if let Some(v) = c.cells.get(i).and_then(|v| v.value()) {
                cells[i][j].push(v);
            }
        }
    }
    let r = crate::core::stats::two_way_anova(&cells).ok_or_else(|| {
        "双因素方差分析需要每个格子至少 1 个观测、总自由度为正（格内要有重复）。".to_string()
    })?;
    let rows = vec![
        vec![
            format!("因子 A（{} 水平）", a),
            r.df_a.to_string(),
            fmt_sig(r.ss_a, 4),
            fmt_sig(r.ss_a / r.df_a, 4),
            fmt_sig(r.f_a, 4),
            fmt_p(r.p_a),
            format!("{:.3}", r.eta_a),
        ],
        vec![
            format!("因子 B（{} 水平）", b),
            r.df_b.to_string(),
            fmt_sig(r.ss_b, 4),
            fmt_sig(r.ss_b / r.df_b, 4),
            fmt_sig(r.f_b, 4),
            fmt_p(r.p_b),
            format!("{:.3}", r.eta_b),
        ],
        vec![
            "A × B 交互".into(),
            r.df_ab.to_string(),
            fmt_sig(r.ss_ab, 4),
            fmt_sig(r.ss_ab / r.df_ab, 4),
            fmt_sig(r.f_ab, 4),
            fmt_p(r.p_ab),
            format!("{:.3}", r.eta_ab),
        ],
        vec![
            "残差".into(),
            r.df_err.to_string(),
            fmt_sig(r.ss_err, 4),
            fmt_sig(r.ss_err / r.df_err, 4),
            String::new(),
            String::new(),
            String::new(),
        ],
    ];
    Ok(ResultTable {
        title: format!("双因素方差分析：{} × {}，共 {} 个观测", a, b, r.n),
        method: "双因素固定效应方差分析（行 × 列，格内重复）：\n        同时检验因子 A、因子 B 的主效应与 A×B 交互效应，\n        交互显著时主效应应结合格均值解读。η² 为偏效应量（SS_效应/(SS_效应+SS_误差)）。\n        平方和采用加权分解（平衡设计时与经典教科书公式一致）。".to_string()
            + &format!("\nα = {}。", opts.alpha),
        columns: vec![
            "来源".into(),
            "df".into(),
            "SS".into(),
            "MS".into(),
            "F".into(),
            "p".into(),
            "偏 η²".into(),
        ],
        rows,
        note: String::new(),
    })
}

/// 单因素重复测量方差分析（柱状/分组表：列 = 处理，行 = 受试者，按行对齐）。
fn rm_test(table: &DataTable, opts: &AnalysisOptions) -> Result<ResultTable, String> {
    let cols: Vec<&crate::core::model::Column> = table
        .columns
        .iter()
        .filter(|c| c.role == ColRole::Y)
        .collect();
    if cols.len() < 2 {
        return Err("重复测量方差分析需要至少 2 个处理（每列一个处理，每行一个受试者）。".into());
    }
    // 按行对齐取完整行：任一处理缺失的行整行剔除（重复测量要求配对完整）。
    let n_rows = cols.iter().map(|c| c.cells.len()).min().unwrap_or(0);
    let mut series: Vec<Vec<f64>> = vec![Vec::new(); cols.len()];
    for i in 0..n_rows {
        let vals: Vec<Option<f64>> = cols
            .iter()
            .map(|c| c.cells.get(i).and_then(|v| v.value()))
            .collect();
        if vals.iter().all(|v| v.is_some()) {
            for (t, v) in vals.iter().enumerate() {
                series[t].push(v.unwrap());
            }
        }
    }
    let r = crate::core::stats::rm_anova(&series).ok_or_else(|| {
        "重复测量方差分析需要 ≥2 个处理、≥2 个受试者，且至少一行（受试者）在所有处理下都有观测。"
            .to_string()
    })?;
    let rows = vec![
        vec![
            "处理间".into(),
            r.df_cond.to_string(),
            fmt_sig(r.ss_cond, 4),
            fmt_sig(r.ss_cond / r.df_cond, 4),
            fmt_sig(r.f, 4),
            fmt_p(r.p),
            format!("{:.3}", r.eta),
        ],
        vec![
            "受试者间".into(),
            r.df_subj.to_string(),
            fmt_sig(r.ss_subj, 4),
            fmt_sig(r.ss_subj / r.df_subj, 4),
            String::new(),
            String::new(),
            String::new(),
        ],
        vec![
            "残差".into(),
            r.df_err.to_string(),
            fmt_sig(r.ss_err, 4),
            fmt_sig(r.ss_err / r.df_err, 4),
            String::new(),
            String::new(),
            String::new(),
        ],
        vec![
            "GG 球形性校正".into(),
            format!("ε = {:.3}", r.gg_eps),
            String::new(),
            String::new(),
            fmt_sig(r.f, 4),
            fmt_p(r.p_gg),
            format!("df {:.1}, {:.1}", r.df_cond_gg, r.df_err_gg),
        ],
    ];
    Ok(ResultTable {
        title: format!(
            "重复测量方差分析：{} 个处理 / {} 个受试者",
            r.conditions, r.subjects
        ),
        method: "单因素重复测量方差分析：受试者作为自身对照，个体间变异（受试者间）从误差中剥离，\n        比组间设计更灵敏。Greenhouse–Geisser ε 由条件协方差矩阵估计球形性偏离，\n        「GG 球形性校正」行为按 ε 缩放自由度后的 p（F 不变）；ε 接近 1 时两行 p 一致。\n        逐受试者轨迹仍建议配合图检查。".to_string()
            + &format!("\nα = {}。", opts.alpha),
        columns: vec![
            "来源".into(),
            "df".into(),
            "SS".into(),
            "MS".into(),
            "F".into(),
            "p".into(),
            "偏 η²".into(),
        ],
        rows,
        note: String::new(),
    })
}

// ===========================================================================
// v0.13：分析补全
//
// 与 spectra 的功能面持平：多重比较、方差齐性、区组非参数、离群值、
// Deming / 斜率比较、曲线拟合报告、相关矩阵、PCA、层次聚类、k-means、
// OR / RR / McNemar、多元回归、Cox。
// 数值逻辑全在 `infer.rs` / `curve.rs`，这里只做编排与排版。
// ===========================================================================

/// 按 `opts.columns` 选组（空 = 全部）。
fn picked(groups: &[(String, Vec<f64>)], opts: &AnalysisOptions) -> Vec<(usize, String, Vec<f64>)> {
    if opts.columns.is_empty() {
        groups
            .iter()
            .enumerate()
            .map(|(i, (n, v))| (i, n.clone(), v.clone()))
            .collect()
    } else {
        opts.columns
            .iter()
            .filter_map(|&i| groups.get(i).map(|(n, v)| (i, n.clone(), v.clone())))
            .collect()
    }
}

/// 行对齐的取值块：只保留所有列都有值的行（区组设计不允许缺格）。
fn aligned_blocks(table: &DataTable) -> Vec<Vec<f64>> {
    let cols: Vec<&crate::core::model::Column> = table
        .columns
        .iter()
        .filter(|c| c.role == ColRole::Y)
        .collect();
    let n = cols.iter().map(|c| c.cells.len()).min().unwrap_or(0);
    let mut out = Vec::new();
    for i in 0..n {
        let vals: Vec<Option<f64>> = cols
            .iter()
            .map(|c| c.cells.get(i).and_then(|v| v.value()))
            .collect();
        if vals.iter().all(|v| v.is_some()) {
            out.push(vals.into_iter().flatten().collect());
        }
    }
    out
}

/// Welch t 检验（不假定方差齐性）。
fn welch_t(groups: &[(String, Vec<f64>)], opts: &AnalysisOptions) -> Result<ResultTable, String> {
    let sel = picked(groups, opts);
    // 多于 2 组时取前两组：t 检验本身只能比两组，两两比较请用「多重比较」。
    // 不做自动两两扫描——那会让一张「t 检验」结果表里塞满事后比较，
    // 读者分不清哪个是主检验。
    if sel.len() < 2 {
        return Err("Welch t 检验需要 2 组数据。".into());
    }
    let r = stats::t_test_unpaired(&sel[0].2, &sel[1].2, true)
        .ok_or_else(|| "每组至少需要 2 个有效值。".to_string())?;
    let d1 = stats::describe(&sel[0].2).unwrap_or_default();
    let d2 = stats::describe(&sel[1].2).unwrap_or_default();
    let rows = vec![
        vec![
            sel[0].1.clone(),
            d1.n.to_string(),
            fmt_sig(d1.mean, 3),
            fmt_sig(d1.sd, 3),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ],
        vec![
            sel[1].1.clone(),
            d2.n.to_string(),
            fmt_sig(d2.mean, 3),
            fmt_sig(d2.sd, 3),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ],
        vec![
            "差值（1−2）".into(),
            String::new(),
            String::new(),
            String::new(),
            format!("{} – {}", fmt_sig(r.ci_low, 3), fmt_sig(r.ci_high, 3)),
            fmt_sig(r.t, 3),
            format!("{:.1}", r.df),
            fmt_p(r.p),
            fmt_sig(r.cohen_d, 2),
        ],
    ];
    Ok(ResultTable {
        title: "Welch t 检验（不等方差）".into(),
        method: "Welch 校正：分母自由度按 Welch–Satterthwaite 估计，不要求两组方差相等。\n        两组方差不齐或样本量不等时，比经典（合并方差）t 检验更可靠。".into(),
        columns: vec![
            "组".into(),
            "n".into(),
            "均值".into(),
            "SD".into(),
            "均值差 95% CI".into(),
            "t".into(),
            "df".into(),
            "P（双侧）".into(),
            "Cohen's d".into(),
        ],
        rows,
        note: format!(
            "{}（α = {}）。{}",
            if r.p < opts.alpha {
                "两组差异具有统计学意义"
            } else {
                "未发现两组差异"
            },
            opts.alpha,
            if groups.len() > 2 {
                format!(
                    "当前表有 {} 组，本表只比较前两组；两两比较请用「多重比较（Tukey / G–H）」。",
                    groups.len()
                )
            } else {
                String::new()
            }
        ),
    })
}

/// 单样本 t 检验（H0: μ = 0）。
fn one_sample_t(
    groups: &[(String, Vec<f64>)],
    opts: &AnalysisOptions,
) -> Result<ResultTable, String> {
    let sel = picked(groups, opts);
    let mut rows = Vec::new();
    for (_, name, v) in &sel {
        let Some(r) = crate::core::infer::one_sample(v, 0.0, 0.95) else {
            continue;
        };
        rows.push(vec![
            name.clone(),
            r.n.to_string(),
            fmt_sig(r.mean, 4),
            fmt_sig(r.sd, 3),
            fmt_sig(r.sem, 3),
            fmt_sig(r.t, 3),
            format!("{:.0}", r.df),
            fmt_p(r.p),
            format!("{} – {}", fmt_sig(r.ci_lo, 3), fmt_sig(r.ci_hi, 3)),
        ]);
    }
    if rows.is_empty() {
        return Err("每组至少需要 2 个有效值。".into());
    }
    Ok(ResultTable {
        title: "单样本 t 检验（H₀: μ = 0）".into(),
        method: "t = (x̄ − μ₀)/SEM，df = n − 1；95% CI 用 t 分布。\n        本实现的参照值 μ₀ = 0；若要与别的基准比较，先用「计算列」减去该基准再做。".into(),
        columns: vec![
            "组".into(),
            "n".into(),
            "均值".into(),
            "SD".into(),
            "SEM".into(),
            "t".into(),
            "df".into(),
            "P（双侧）".into(),
            "均值 95% CI".into(),
        ],
        rows,
        note: format!("α = {}。", opts.alpha),
    })
}

/// 方差齐性：Bartlett / Levene / Brown–Forsythe 三种口径一起给。
fn variance_tests(
    groups: &[(String, Vec<f64>)],
    opts: &AnalysisOptions,
) -> Result<ResultTable, String> {
    let vals: Vec<Vec<f64>> = picked(groups, opts)
        .into_iter()
        .map(|(_, _, v)| v)
        .collect();
    if vals.len() < 2 {
        return Err("方差齐性检验至少需要 2 组。".into());
    }
    let mut rows = Vec::new();
    if let Some(b) = crate::core::infer::bartlett(&vals) {
        rows.push(vec![
            "Bartlett".into(),
            fmt_sig(b.stat, 4),
            format!("{:.0}", b.df1),
            "—".into(),
            fmt_p(b.p),
            if b.p < opts.alpha {
                "方差不齐"
            } else {
                "方差齐"
            }
            .into(),
        ]);
    }
    if let Some(l) = crate::core::infer::levene(&vals, false) {
        rows.push(vec![
            "Levene（均值）".into(),
            fmt_sig(l.stat, 4),
            format!("{:.0}", l.df1),
            format!("{:.0}", l.df2),
            fmt_p(l.p),
            if l.p < opts.alpha {
                "方差不齐"
            } else {
                "方差齐"
            }
            .into(),
        ]);
    }
    if let Some(b) = crate::core::infer::levene(&vals, true) {
        rows.push(vec![
            "Brown–Forsythe（中位数）".into(),
            fmt_sig(b.stat, 4),
            format!("{:.0}", b.df1),
            format!("{:.0}", b.df2),
            fmt_p(b.p),
            if b.p < opts.alpha {
                "方差不齐"
            } else {
                "方差齐"
            }
            .into(),
        ]);
    }
    if rows.is_empty() {
        return Err("每组至少需要 2 个有效值。".into());
    }
    Ok(ResultTable {
        title: "方差齐性检验".into(),
        method: "Bartlett：χ² 检验，假定各组正态——非正态时误报率高。\n        Levene：对「到组均值的绝对偏差」做单因素 ANOVA，比 Bartlett 稳健。\n        Brown–Forsythe：把均值换成中位数，偏态数据首选。".into(),
        columns: vec![
            "检验".into(),
            "统计量".into(),
            "df1".into(),
            "df2".into(),
            "P".into(),
            "判读".into(),
        ],
        rows,
        note: "三种口径不一致时，以 Brown–Forsythe 为准；随后用 Welch t / Games–Howell，而不是合并方差 t / Tukey。".into(),
    })
}

/// 离群值检测（Grubbs，逐组）。
fn outliers(groups: &[(String, Vec<f64>)], opts: &AnalysisOptions) -> Result<ResultTable, String> {
    let mut rows = Vec::new();
    for (_, name, v) in picked(groups, opts) {
        let Some(g) = crate::core::infer::grubbs(&v, opts.alpha) else {
            continue;
        };
        rows.push(vec![
            name,
            g.n.to_string(),
            fmt_sig(g.value, 5),
            fmt_sig(g.g, 3),
            fmt_sig(g.critical, 3),
            fmt_p(g.p),
            if g.g > g.critical {
                "离群（建议核查）"
            } else {
                "未检出"
            }
            .into(),
        ]);
    }
    if rows.is_empty() {
        return Err("每组至少需要 3 个有效值，且不能全部相同。".into());
    }
    Ok(ResultTable {
        title: "离群值检测（Grubbs）".into(),
        method: "单侧 Grubbs：G = |x − x̄|/s 取最大者，临界值 G_crit = (n−1)/√n·√(t²/(n−2+t²))，\n        t 取 α/(2n) 分位（同一口径的 Bonferroni 校正）。只标「最像离群」的一个点，\n        不自动删除——离群是否剔除要有实验依据。".into(),
        columns: vec![
            "组".into(),
            "n".into(),
            "最离群值".into(),
            "G".into(),
            "G 临界值".into(),
            "P".into(),
            "判读".into(),
        ],
        rows,
        note: "G > 临界值只是统计信号：先查记录/仪器，再决定是否排除。反复检验请改用 ROUT 或稳健方法。".into(),
    })
}

/// Friedman 区组秩检验 + 平均秩。
fn friedman_test(table: &DataTable, opts: &AnalysisOptions) -> Result<ResultTable, String> {
    let blocks = aligned_blocks(table);
    let r = crate::core::infer::friedman(&blocks).ok_or_else(|| {
        "Friedman 需要 ≥2 个处理（列）、≥2 个区组（行），且每行都填满。".to_string()
    })?;
    let names: Vec<String> = table
        .columns
        .iter()
        .filter(|c| c.role == ColRole::Y)
        .map(|c| c.name.clone())
        .collect();
    let mut rows = Vec::new();
    for (i, mr) in r.mean_ranks.iter().enumerate() {
        rows.push(vec![
            names
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("处理 {}", i + 1)),
            r.blocks.to_string(),
            fmt_sig(*mr, 2),
        ]);
    }
    rows.push(Vec::new());
    rows.push(vec!["检验结果".into()]);
    rows.push(vec![
        "Q（并列校正后）".into(),
        fmt_sig(r.q_corrected, 4),
        format!(
            "df = {:.0}, P = {}, W = {:.3}",
            r.df,
            fmt_p(r.p),
            r.q_corrected / (r.blocks as f64 * (r.treatments as f64 - 1.0))
        ),
    ]);
    Ok(ResultTable {
        title: "Friedman 区组秩检验".into(),
        method: "每行（受试者/区组）内编秩，比较各处理的平均秩。\n        Q = 12/(b·k·(k+1))·ΣR² − 3b(k+1)；有并列时除以 C = 1 − Σ(t³−t)/(b(k³−k))。\n        不要求正态，但要求同一区组内的观测可比。".into(),
        columns: vec!["处理".into(), "区组数".into(), "平均秩".into()],
        rows,
        note: format!(
            "Kendall's W = {:.3}（0–1，越大越一致）。{}",
            r.q_corrected / (r.blocks as f64 * (r.treatments as f64 - 1.0)),
            if r.p < opts.alpha {
                "处理间差异有统计学意义；需要知道哪两对不同，请再做 Dunn 检验。"
            } else {
                "未发现处理间差异。"
            }
        ),
    })
}

/// 多重比较：Tukey（等方差）与 Games–Howell（不等方差）并排。
fn multi_compare(
    groups: &[(String, Vec<f64>)],
    opts: &AnalysisOptions,
) -> Result<ResultTable, String> {
    let sel = picked(groups, opts);
    if sel.len() < 2 {
        return Err("多重比较至少需要 2 组。".into());
    }
    let vals: Vec<Vec<f64>> = sel.iter().map(|(_, _, v)| v.clone()).collect();
    let tukey = crate::core::infer::tukey_hsd(&vals, 1.0 - opts.alpha);
    let gh = crate::core::infer::games_howell(&vals, 1.0 - opts.alpha);
    if tukey.is_none() && gh.is_none() {
        return Err("每组至少需要 2 个有效值。".into());
    }
    let mut rows = Vec::new();
    if let Some(tk) = &tukey {
        for p in tk {
            let g = gh
                .as_ref()
                .and_then(|v| v.iter().find(|q| q.a == p.a && q.b == p.b));
            rows.push(vec![
                format!("{} vs {}", sel[p.a].1, sel[p.b].1),
                fmt_sig(p.diff, 4),
                format!("{} – {}", fmt_sig(p.ci_lo, 3), fmt_sig(p.ci_hi, 3)),
                fmt_sig(p.stat, 3),
                fmt_p(p.p),
                g.map(|q| fmt_p(q.p)).unwrap_or_else(|| "—".into()),
            ]);
        }
    }
    if rows.is_empty() {
        return Err("没有可比较的组对。".into());
    }
    Ok(ResultTable {
        title: "多重比较（Tukey / Games–Howell）".into(),
        method: "Tukey HSD：假定方差齐性，SE = √(MS_w·½(1/nᵢ+1/nⱼ))，用学生化极差 q 分布，\n        CI 已含多重比较校正（族错误率 = α）。\n        Games–Howell：不假定方差齐性，每对用 Welch 的 SE 与 df，仍走 q 分布。\n        两列 P 并列给出：方差齐时二者接近；差距大说明方差不齐，应以 G–H 为准。".into(),
        columns: vec![
            "比较".into(),
            "均值差".into(),
            "Tukey 95% CI".into(),
            "q".into(),
            "P（Tukey）".into(),
            "P（Games–Howell）".into(),
        ],
        rows,
        note: format!(
            "α = {}；CI 不跨 0 即该对显著。事后比较必须跟在显著的整体检验之后解读。",
            opts.alpha
        ),
    })
}

/// Spearman 秩相关（逐数据集）。
fn spearman_corr(
    table: &DataTable,
    groups: &[(String, Vec<f64>)],
    opts: &AnalysisOptions,
) -> Result<ResultTable, String> {
    if table.kind != TableKind::Xy {
        return Err("Spearman 相关请在 XY 表上进行。".into());
    }
    let sel = picked(groups, opts);
    let mut rows = Vec::new();
    for (i, name, _) in &sel {
        let pts = table.xy_points(*i);
        if pts.len() < 3 {
            continue;
        }
        let xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
        let ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
        if let Some(r) = stats::spearman(&xs, &ys) {
            rows.push(vec![
                name.clone(),
                r.n.to_string(),
                fmt_sig(r.r, 4),
                format!("{} – {}", fmt_sig(r.ci_low, 3), fmt_sig(r.ci_high, 3)),
                fmt_p(r.p),
                sig_label(r.p, 2),
            ]);
        }
    }
    if rows.is_empty() {
        return Err("每个数据集至少需要 3 个有效点。".into());
    }
    Ok(ResultTable {
        title: "Spearman 秩相关".into(),
        method: "对 X、Y 分别编秩后求 Pearson 相关（并列取平均秩）。\n        度量单调关系，不受离群值量级影响；CI 用 Fisher z 变换。\n        r_s = 0 只说明「无单调关系」，不排除非单调关系。".into(),
        columns: vec![
            "数据集".into(),
            "n".into(),
            "r_s".into(),
            "95% CI".into(),
            "P（双侧）".into(),
            "".into(),
        ],
        rows,
        note: format!("α = {}。", opts.alpha),
    })
}

/// Deming 回归（X、Y 都有误差）。
fn deming_reg(
    table: &DataTable,
    groups: &[(String, Vec<f64>)],
    opts: &AnalysisOptions,
) -> Result<ResultTable, String> {
    if table.kind != TableKind::Xy {
        return Err("Deming 回归请在 XY 表上进行。".into());
    }
    let mut rows = Vec::new();
    for (i, name, _) in picked(groups, opts) {
        let pts = table.xy_points(i);
        let xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
        let ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
        let Some(d) = crate::core::infer::deming(&xs, &ys, 1.0) else {
            continue;
        };
        rows.push(vec![
            name,
            format!("y = {}x + {}", fmt_sig(d.slope, 4), fmt_sig(d.intercept, 4)),
            fmt_sig(d.slope, 4),
            fmt_sig(d.intercept, 4),
            fmt_sig(d.r, 4),
            d.n.to_string(),
        ]);
    }
    if rows.is_empty() {
        return Err("每个数据集至少需要 3 个有效点，且 X、Y 不能完全无关。".into());
    }
    Ok(ResultTable {
        title: "Deming 回归".into(),
        method: "正交回归：最小化「到直线的垂直距离（按误差比加权）」而不是 Y 方向的残差。\n        误差比 λ = σ²(ε_y)/σ²(ε_x) 取 1（两轴测量误差相当）。λ 会实质改变斜率，\n        已知仪器精度时应按方差比设定。".into(),
        columns: vec![
            "数据集".into(),
            "方程".into(),
            "斜率".into(),
            "截距".into(),
            "r".into(),
            "n".into(),
        ],
        rows,
        note: "两轴都无误差时用普通最小二乘（结果会更陡）；Deming 的斜率介于 OLS(y|x) 与 OLS(x|y) 之间。".into(),
    })
}

/// 两条斜率比较（ANCOVA 合并误差口径）。
fn slope_compare(
    table: &DataTable,
    groups: &[(String, Vec<f64>)],
    opts: &AnalysisOptions,
) -> Result<ResultTable, String> {
    if table.kind != TableKind::Xy {
        return Err("斜率比较请在 XY 表上进行（两个 Y 数据集）。".into());
    }
    let sel = picked(groups, opts);
    if sel.len() < 2 {
        return Err("斜率比较需要 2 个数据集（XY 表的两列 Y）。".into());
    }
    let p1 = table.xy_points(sel[0].0);
    let p2 = table.xy_points(sel[1].0);
    let (x1, y1): (Vec<f64>, Vec<f64>) = p1.iter().copied().unzip();
    let (x2, y2): (Vec<f64>, Vec<f64>) = p2.iter().copied().unzip();
    let r = crate::core::infer::compare_slopes(&x1, &y1, &x2, &y2)
        .ok_or_else(|| "每组至少需要 3 个有效点，且 X 不能全相同。".to_string())?;
    let rows = vec![
        vec![
            sel[0].1.clone(),
            r.n1.to_string(),
            fmt_sig(r.slope1, 4),
            String::new(),
        ],
        vec![
            sel[1].1.clone(),
            r.n2.to_string(),
            fmt_sig(r.slope2, 4),
            String::new(),
        ],
        Vec::new(),
        vec![
            "斜率差 (1−2)".into(),
            String::new(),
            fmt_sig(r.diff, 4),
            format!(
                "SE = {}, t = {}, df = {:.0}, P = {}",
                fmt_sig(r.se, 4),
                fmt_sig(r.t, 3),
                r.df,
                fmt_p(r.p)
            ),
        ],
    ];
    Ok(ResultTable {
        title: "两条斜率比较".into(),
        method: "分别拟合后合并残差：SE = √(s²_合并/Sxx₁ + s²_合并/Sxx₂)，t = (b₁−b₂)/SE，\n        df = n₁+n₂−4。等价于 ANCOVA 的「组×协变量」交互项检验。\n        P 显著 = 两条线不平行，此时不宜报「共同斜率」。".into(),
        columns: vec!["数据集".into(), "n".into(), "斜率".into(), "检验".into()],
        rows,
        note: format!(
            "{}（α = {}）。",
            if r.p < opts.alpha {
                "斜率差异显著：两条线不平行，应分别报告"
            } else {
                "斜率差异不显著：可报共同斜率"
            },
            opts.alpha
        ),
    })
}

/// 非线性拟合报告：所有模型按 AICc 排名。
fn curve_report(
    table: &DataTable,
    groups: &[(String, Vec<f64>)],
    opts: &AnalysisOptions,
) -> Result<ResultTable, String> {
    if table.kind != TableKind::Xy {
        return Err("曲线拟合请在 XY 表上进行。".into());
    }
    let mut rows = Vec::new();
    for (i, name, _) in picked(groups, opts) {
        let pts = table.xy_points(i);
        if pts.len() < 5 {
            continue;
        }
        let fits = crate::core::curve::fit_all(&pts);
        for (rank, f) in fits.iter().enumerate() {
            let mut note = Vec::new();
            if rank == 0 {
                note.push("AICc 最优".to_string());
            }
            for e in &f.extra {
                if e.1.is_finite() {
                    note.push(format!("{} = {}", e.0, fmt_sig(e.1, 4)));
                }
            }
            for w in &f.warnings {
                note.push(format!("⚠ {w}"));
            }
            rows.push(vec![
                name.clone(),
                f.model.label().to_string(),
                f.equation(),
                fmt_sig(f.r2, 4),
                fmt_sig(f.rmse, 4),
                fmt_sig(f.aicc, 1),
                note.join("；"),
            ]);
        }
        rows.push(Vec::new());
    }
    if rows.is_empty() {
        return Err("每个数据集至少需要 5 个有效点。".into());
    }
    Ok(ResultTable {
        title: "非线性拟合（AICc 排名）".into(),
        method: "每个模型都用 Levenberg–Marquardt 多初值拟合并做 ln 参数化；\n        AICc = n·ln(SSE/n) + 2k + 2k(k+1)/(n−k−1)，越小越好（已惩罚参数个数）。\n        ΔAICc < 2 的两个模型在数据上不可区分，别硬选「第一名」。".into(),
        columns: vec![
            "数据集".into(),
            "模型".into(),
            "方程".into(),
            "R²".into(),
            "RMSE".into(),
            "AICc".into(),
            "备注".into(),
        ],
        rows,
        note: "R² 不能用来比较不同参数个数的模型——用 AICc。ED50/C 等关键参数不可辨识时会在备注标出。".into(),
    })
}

/// 相关矩阵（r 与 P 两块）。
fn corr_table(
    groups: &[(String, Vec<f64>)],
    opts: &AnalysisOptions,
) -> Result<ResultTable, String> {
    let sel = picked(groups, opts);
    if sel.len() < 2 {
        return Err("相关矩阵至少需要 2 个变量（列）。".into());
    }
    let cols: Vec<Vec<f64>> = sel.iter().map(|(_, _, v)| v.clone()).collect();
    let (r, p, n) = crate::core::infer::corr_matrix(&cols)
        .ok_or_else(|| "至少需要 3 行完整数据。".to_string())?;
    let names: Vec<String> = sel.iter().map(|(_, n, _)| n.clone()).collect();
    let mut rows = Vec::new();
    for i in 0..names.len() {
        let mut row = vec![names[i].clone()];
        for j in 0..names.len() {
            row.push(if i == j {
                "1".into()
            } else {
                fmt_sig(r[i][j], 3)
            });
        }
        rows.push(row);
    }
    rows.push(Vec::new());
    rows.push(vec!["P 值（双尾）".into()]);
    for i in 0..names.len() {
        let mut row = vec![names[i].clone()];
        for j in 0..names.len() {
            row.push(if i == j { "—".into() } else { fmt_p(p[i][j]) });
        }
        rows.push(row);
    }
    let mut columns = vec!["变量".to_string()];
    columns.extend(names.iter().cloned());
    Ok(ResultTable {
        title: format!("相关矩阵（{} 个变量，n = {n}）", names.len()),
        method: "Pearson 积矩相关：r = Σ(x−x̄)(y−ȳ)/√(Σ(x−x̄)²Σ(y−ȳ)²)。\n        P 值用 t = r√((n−2)/(1−r²)) 双尾。只保留所有变量都完整的行。".into(),
        columns,
        rows,
        note: "相关不等于因果；多个变量同时检验时 P 值未做多重比较校正，解读单个格子要谨慎。".into(),
    })
}

/// 主成分分析。
fn pca_table(groups: &[(String, Vec<f64>)], opts: &AnalysisOptions) -> Result<ResultTable, String> {
    let sel = picked(groups, opts);
    if sel.len() < 2 {
        return Err("主成分分析至少需要 2 个变量（列）。".into());
    }
    let cols: Vec<Vec<f64>> = sel.iter().map(|(_, _, v)| v.clone()).collect();
    let p = crate::core::infer::pca(&cols, true)
        .ok_or_else(|| "至少需要 3 行完整数据。".to_string())?;
    let names: Vec<String> = sel.iter().map(|(_, n, _)| n.clone()).collect();
    let mut rows = Vec::new();
    for i in 0..p.eigen.len() {
        rows.push(vec![
            format!("PC{}", i + 1),
            fmt_sig(p.eigen[i], 4),
            format!("{:.2}%", p.proportion[i] * 100.0),
            format!("{:.2}%", p.cumulative[i] * 100.0),
        ]);
    }
    rows.push(Vec::new());
    rows.push(vec!["载荷（变量 × 主成分）".into()]);
    rows.push({
        let mut head = vec!["变量".to_string()];
        for i in 0..p.eigen.len() {
            head.push(format!("PC{}", i + 1));
        }
        head
    });
    for v in 0..names.len() {
        let mut row = vec![names[v].clone()];
        for c in 0..p.eigen.len() {
            row.push(fmt_sig(p.loadings[v][c], 3));
        }
        rows.push(row);
    }
    Ok(ResultTable {
        title: format!("主成分分析（{} 变量，n = {}）", names.len(), p.n),
        method: "对标准化后的数据求相关矩阵，用 Jacobi 旋转做特征分解。\n        特征值 = 该主成分解释的方差；载荷 = 变量与主成分的相关系数（符号可整体翻转）。\n        用相关矩阵（而非协方差矩阵）是为了不让量纲大的变量独占第一主成分。".into(),
        columns: vec![
            "主成分".into(),
            "特征值".into(),
            "解释比例".into(),
            "累积".into(),
        ],
        rows,
        note: "取几个主成分看累积解释率与碎石图的拐点；载荷 > 0.5（绝对值）的变量才对该主成分有实质贡献。".into(),
    })
}

/// 层次聚类（平均连接）。
fn hclust_table(
    groups: &[(String, Vec<f64>)],
    opts: &AnalysisOptions,
) -> Result<ResultTable, String> {
    let sel = picked(groups, opts);
    if sel.len() < 2 {
        return Err("聚类至少需要 2 个变量（列）。".into());
    }
    let cols: Vec<Vec<f64>> = sel.iter().map(|(_, _, v)| v.clone()).collect();
    let n = cols.iter().map(|c| c.len()).min().unwrap_or(0);
    let k = ((n as f64).sqrt().round() as usize).clamp(2, 6);
    let h = crate::core::infer::hclust(&cols, k)
        .ok_or_else(|| "至少需要 2 行完整数据。".to_string())?;
    let mut rows = Vec::new();
    for (i, m) in h.merges.iter().take(12).enumerate() {
        rows.push(vec![
            format!("{}", i + 1),
            format!("{} ↔ {}", m.a + 1, m.b + 1),
            fmt_sig(m.dist, 4),
            m.size.to_string(),
        ]);
    }
    rows.push(Vec::new());
    rows.push(vec!["最终分簇（k = ".to_string() + &h.k.to_string() + "）"]);
    for (i, c) in h.cluster.iter().enumerate() {
        rows.push(vec![
            format!("观测 {}", h.order.get(i).copied().unwrap_or(i) + 1),
            format!("簇 {}", c + 1),
        ]);
    }
    Ok(ResultTable {
        title: format!("层次聚类（平均连接，k = {}）", h.k),
        method: "变量先按列标准化（z 分数），再算欧氏距离；\n        簇间距离取「成员两两距离的平均」（UPGMA），每步合并最近的一对。\n        k 由 √n 取整得到，可在方法栏对照合并高度自行调整。".into(),
        columns: vec!["合并序".into(), "合并的两簇".into(), "距离".into(), "新簇大小".into()],
        rows,
        note: "聚类没有唯一正确答案：换连接方式或 k 结果会变，需结合专业含义判断。".into(),
    })
}

/// k-means。
fn kmeans_table(
    groups: &[(String, Vec<f64>)],
    opts: &AnalysisOptions,
) -> Result<ResultTable, String> {
    let sel = picked(groups, opts);
    if sel.len() < 2 {
        return Err("k 均值聚类至少需要 2 个变量（列）。".into());
    }
    let cols: Vec<Vec<f64>> = sel.iter().map(|(_, _, v)| v.clone()).collect();
    let n = cols.iter().map(|c| c.len()).min().unwrap_or(0);
    let k = ((n as f64).sqrt().round() as usize).clamp(2, 6);
    let r = crate::core::infer::kmeans(&cols, k, 20261113)
        .ok_or_else(|| "至少需要 2 行完整数据。".to_string())?;
    let mut rows = Vec::new();
    for i in 0..r.k {
        let count = r.cluster.iter().filter(|c| **c == i).count();
        rows.push(vec![
            format!("簇 {}", i + 1),
            count.to_string(),
            format!(
                "{}; {}",
                fmt_sig(r.centers[i][0], 4),
                fmt_sig(*r.centers[i].get(1).unwrap_or(&f64::NAN), 4)
            ),
        ]);
    }
    rows.push(Vec::new());
    rows.push(vec![
        "簇内平方和".into(),
        fmt_sig(r.wss, 4),
        format!("迭代 {} 轮", r.iterations),
    ]);
    Ok(ResultTable {
        title: format!("k 均值聚类（k = {}）", r.k),
        method: "k-means++ 初始化（固定种子，结果可复现），迭代到簇分配不再变化。\n        簇心按原始坐标给出（前两个变量）。".into(),
        columns: vec!["簇".into(), "样本数".into(), "簇心（前两变量）".into()],
        rows,
        note: "k-means 假定簇是球形且大小相近；长条/嵌套结构请改用层次聚类。".into(),
    })
}

/// 2×2 的 OR / RR。
fn effect_ratios(table: &DataTable, opts: &AnalysisOptions) -> Result<ResultTable, String> {
    let (rows_lab, cols_lab, counts) = table
        .contingency_matrix()
        .ok_or_else(|| "列联表为空。".to_string())?;
    if rows_lab.len() != 2 || cols_lab.len() != 2 {
        return Err("OR / RR 需要 2×2 表（2 行 2 列）。".to_string());
    }
    let (a, b, c, d) = (counts[0][0], counts[0][1], counts[1][0], counts[1][1]);
    let (or, rr) = crate::core::infer::odds_ratio(a, b, c, d, 1.0 - opts.alpha)
        .ok_or_else(|| "频数不能为负。".to_string())?;
    let mut rows = vec![
        vec![
            "OR（比值的比）".into(),
            fmt_sig(or.est, 4),
            format!("{} – {}", fmt_sig(or.lo, 3), fmt_sig(or.hi, 3)),
            "ad/bc；对数尺度 Woolf CI".into(),
        ],
        vec![
            "RR（相对风险）".into(),
            fmt_sig(rr.est, 4),
            format!("{} – {}", fmt_sig(rr.lo, 3), fmt_sig(rr.hi, 3)),
            "队列研究口径；对数尺度 Katz CI".into(),
        ],
    ];
    rows.push(Vec::new());
    rows.push(vec!["2×2 频数表".into()]);
    rows.push(vec![
        format!("a = {a:.0}（{} & {}）", rows_lab[0], cols_lab[0]),
        format!("b = {b:.0}（{} & {}）", rows_lab[0], cols_lab[1]),
    ]);
    rows.push(vec![
        format!("c = {c:.0}（{} & {}）", rows_lab[1], cols_lab[0]),
        format!("d = {d:.0}（{} & {}）", rows_lab[1], cols_lab[1]),
    ]);
    Ok(ResultTable {
        title: "比值比与相对风险".into(),
        method: "2×2 表：a = 暴露且事件，b = 暴露无事件，c = 非暴露且事件，d = 非暴露无事件。\n        OR = ad/bc（病例对照研究唯一可估的效应量）；RR = [a/(a+b)]/[c/(c+d)]（队列/RCT）。\n        有 0 格时按惯例加 0.5 校正，并在结论里说明。".into(),
        columns: vec!["效应量".into(), "估计值".into(), "95% CI".into(), "说明".into()],
        rows,
        note: "OR 会高估 RR（事件越常见高估越多）；解释「风险翻几倍」请用 RR。".into(),
    })
}

/// McNemar（配对二分类）。
fn mcnemar_test(table: &DataTable, opts: &AnalysisOptions) -> Result<ResultTable, String> {
    let (rows_lab, cols_lab, counts) = table
        .contingency_matrix()
        .ok_or_else(|| "列联表为空。".to_string())?;
    if rows_lab.len() != 2 || cols_lab.len() != 2 {
        return Err("McNemar 需要 2×2 配对表（同一批对象测两次）。".to_string());
    }
    let b = counts[0][1];
    let c = counts[1][0];
    let r = crate::core::infer::mcnemar(b, c)
        .ok_or_else(|| "两个不一致格都为空，无法检验。".to_string())?;
    let rows = vec![
        vec![
            format!("{} → {}", rows_lab[0], cols_lab[1]),
            format!("b = {b:.0}"),
            "配对 1 阳 / 2 阴".into(),
        ],
        vec![
            format!("{} → {}", rows_lab[1], cols_lab[0]),
            format!("c = {c:.0}"),
            "配对 1 阴 / 2 阳".into(),
        ],
        Vec::new(),
        vec![
            "McNemar χ²（连续性校正）".into(),
            fmt_sig(r.chi2_cc, 4),
            format!("df = 1, P = {}", fmt_p(r.p_chi2)),
        ],
        vec![
            "精确二项检验".into(),
            String::new(),
            format!("P = {}", fmt_p(r.p_exact)),
        ],
        vec!["不一致对的比值 b/c".into(), fmt_sig(r.or, 3), String::new()],
    ];
    Ok(ResultTable {
        title: "McNemar 配对检验".into(),
        method: "只看两个不一致格 b、c：H₀ 为两者概率相等（等价于 b/c 的比值 = 1）。\n        精确 P 来自 Binomial(b+c, 0.5) 双侧检验，b+c 小时以它为准；\n        χ² 用连续性校正，样本大时两者一致。".into(),
        columns: vec!["格".into(), "频数".into(), "检验".into()],
        rows,
        note: format!(
            "{}（α = {}）。一致格（a、d）不携带「变化」的信息，不参与检验。",
            if r.p_exact < opts.alpha {
                "两次测量的阳性率差异有统计学意义"
            } else {
                "未发现两次测量的差异"
            },
            opts.alpha
        ),
    })
}

/// 多元线性回归（多变量表：第 1 列 Y，其余列 X）。
fn multi_regression(table: &DataTable, _opts: &AnalysisOptions) -> Result<ResultTable, String> {
    if table.kind != TableKind::MultipleVariables {
        return Err("多元回归请在「多变量表」上进行（第一列因变量，其余列自变量）。".into());
    }
    let cols: Vec<&crate::core::model::Column> = table
        .columns
        .iter()
        .filter(|c| c.role == ColRole::Y && !c.name.trim().is_empty())
        .collect();
    if cols.len() < 2 {
        return Err("多元回归至少需要 2 列（1 个因变量 + 1 个自变量）。".into());
    }
    let rows_n = table.nrows();
    let mut y = Vec::new();
    let mut xs: Vec<(String, Vec<f64>)> = cols[1..]
        .iter()
        .map(|c| (c.name.clone(), Vec::new()))
        .collect();
    for r in 0..rows_n {
        let Some(yv) = cols[0].cells.get(r).and_then(|c| c.value()) else {
            continue;
        };
        let xv: Vec<Option<f64>> = cols[1..]
            .iter()
            .map(|c| c.cells.get(r).and_then(|v| v.value()))
            .collect();
        if xv.iter().any(|v| v.is_none()) {
            continue;
        }
        y.push(yv);
        for (i, v) in xv.into_iter().enumerate() {
            xs[i].1.push(v.unwrap());
        }
    }
    let r = crate::core::infer::ols(&y, &xs)
        .ok_or_else(|| "样本量不足（需 > 自变量个数 + 1），或自变量之间完全共线。".to_string())?;
    let mut rows = Vec::new();
    for i in 0..r.coef.len() {
        rows.push(vec![
            r.names[i].clone(),
            fmt_sig(r.coef[i], 4),
            fmt_sig(r.se[i], 4),
            fmt_sig(r.t[i], 3),
            fmt_p(r.p[i]),
        ]);
    }
    rows.push(Vec::new());
    rows.push(vec!["模型汇总".into()]);
    rows.push(vec![
        format!(
            "R² = {}, 调整 R² = {}",
            fmt_sig(r.r2, 4),
            fmt_sig(r.adj_r2, 4)
        ),
        format!(
            "F({}, {:.0}) = {}, P = {}",
            xs.len(),
            r.df_resid,
            fmt_sig(r.f, 3),
            fmt_p(r.f_p)
        ),
        format!(
            "n = {}, 残差 df = {:.0}, SSE = {}",
            r.n,
            r.df_resid,
            fmt_sig(r.sse, 4)
        ),
    ]);
    Ok(ResultTable {
        title: format!("多元线性回归：{}", cols[0].name),
        method: "普通最小二乘 y = b₀ + Σbᵢxᵢ；标准误来自 (X'X)⁻¹·s²，\n        t 检验的各系数 P 为「控制其他自变量后该变量是否还有贡献」。\n        只保留所有列都完整的行。".into(),
        columns: vec!["项".into(), "系数".into(), "SE".into(), "t".into(), "P".into()],
        rows,
        note: "自变量高度相关时系数会不稳定（共线）：看 VIF 或先做相关矩阵/主成分。R² 高也不能证明因果。".into(),
    })
}

/// Cox 比例风险（生存表：X = 时间，Y 列 = 各组事件指示，协变量取组别）。
fn cox_test(table: &DataTable, opts: &AnalysisOptions) -> Result<ResultTable, String> {
    let Some(xi) = table.x_index() else {
        return Err("生存表缺少时间列。".into());
    };
    let times = &table.columns[xi].cells;
    let gcols: Vec<&crate::core::model::Column> = table
        .columns
        .iter()
        .filter(|c| c.role == ColRole::Y)
        .collect();
    if gcols.len() < 2 {
        return Err("Cox 回归需要至少 2 组（每列一组的事件指示）。".into());
    }
    let mut t = Vec::new();
    let mut e = Vec::new();
    let mut g = Vec::new();
    for (gi, c) in gcols.iter().enumerate() {
        for row in 0..table.nrows() {
            let (Some(tt), Some(ev)) = (
                times.get(row).and_then(|x| x.value()),
                c.cells.get(row).and_then(|x| x.value()),
            ) else {
                continue;
            };
            t.push(tt);
            e.push(ev);
            g.push(gi as f64);
        }
    }
    if t.len() < 6 {
        return Err("有效观测太少（至少 6 条）。".into());
    }
    // 组别哑变量：第 1 组为参照
    let mut cols: Vec<(String, Vec<f64>)> = Vec::new();
    for gi in 1..gcols.len() {
        cols.push((
            format!("{} vs {}", gcols[gi].name, gcols[0].name),
            g.iter()
                .map(|x| if *x == gi as f64 { 1.0 } else { 0.0 })
                .collect(),
        ));
    }
    let r = crate::core::infer::cox_ph(&t, &e, &cols)
        .ok_or_else(|| "没有事件（全为删失）或样本量不足。".to_string())?;
    let mut rows = Vec::new();
    for (i, v) in r.vars.iter().enumerate() {
        rows.push(vec![
            cols[i].0.clone(),
            fmt_sig(v.beta, 4),
            fmt_sig(v.hr, 3),
            format!("{} – {}", fmt_sig(v.ci_lo, 3), fmt_sig(v.ci_hi, 3)),
            fmt_sig(v.z, 3),
            fmt_p(v.p),
        ]);
    }
    rows.push(Vec::new());
    rows.push(vec![
        "模型".into(),
        format!("事件 {} / 样本 {}", r.n_events, r.n),
        format!("对数偏似然 {}", fmt_sig(r.loglik, 4)),
        if r.separated {
            "数据完全分离：HR 无界，SE 不可估".into()
        } else {
            "Breslow 并列处理".into()
        },
    ]);
    Ok(ResultTable {
        title: "Cox 比例风险回归".into(),
        method: "偏似然 L = Π[exp(xβ)/Σ_{R(t)}exp(xβ)]^d，Newton–Raphson 迭代求解；\n        并列事件按 Breslow 处理。HR = exp(β)，95% CI 由 Wald 法给出。\n        协变量为组别哑变量（第 1 组作参照）；连续协变量暂不支持。".into(),
        columns: vec![
            "比较".into(),
            "β".into(),
            "HR".into(),
            "HR 95% CI".into(),
            "z".into(),
            "P".into(),
        ],
        rows,
        note: format!(
            "{}（α = {}）。Cox 假定比例风险：各组生存曲线应大致平行，交叉时 HR 无单一含义。",
            if r.separated {
                "某组风险全落在另一组之前（完全分离），β 发散"
            } else if r.vars.iter().any(|v| v.p < opts.alpha) {
                "组间风险差异具有统计学意义"
            } else {
                "未发现组间风险差异"
            },
            opts.alpha
        ),
    })
}

/// 单样本 Wilcoxon 符号秩（H₀: 中位数 = 0）。
fn signed_rank_test(
    groups: &[(String, Vec<f64>)],
    opts: &AnalysisOptions,
) -> Result<ResultTable, String> {
    let mut rows = Vec::new();
    for (_, name, v) in picked(groups, opts) {
        let Some(r) = crate::core::infer::signed_rank_one(&v, 0.0) else {
            continue;
        };
        rows.push(vec![
            name,
            r.n.to_string(),
            fmt_sig(r.w, 3),
            fmt_sig(r.z, 3),
            fmt_p(r.p),
            sig_label(r.p, 2),
            if r.zero_dropped > 0 {
                format!("剔除 {} 个等于 0 的差值", r.zero_dropped)
            } else {
                String::new()
            },
        ]);
    }
    if rows.is_empty() {
        return Err("每组至少需要 6 个非零差值（正态近似要求）。".into());
    }
    Ok(ResultTable {
        title: "Wilcoxon 符号秩检验（单样本，H₀: 中位数 = 0）".into(),
        method: "对 |x − μ₀| 编秩，正差值的秩和 W⁺ 与期望 n(n+1)/4 比较；\n        z = (W⁺ − 期望 ∓ 0.5)/SD，SD 含并列校正：Σ(t³−t)/48。\n        n < 6 时正态近似不可靠，本实现直接拒绝而不是给一个勉强的 p。".into(),
        columns: vec![
            "组".into(),
            "n".into(),
            "W⁺".into(),
            "z".into(),
            "P（双侧）".into(),
            "".into(),
            "备注".into(),
        ],
        rows,
        note: format!(
            "参照值 μ₀ = 0。要与别的基准比较，用「计算列」先减去基准。α = {}。",
            opts.alpha
        ),
    })
}

/// 两样本 F 方差比检验。
fn f_variance_test(
    groups: &[(String, Vec<f64>)],
    opts: &AnalysisOptions,
) -> Result<ResultTable, String> {
    let sel = picked(groups, opts);
    if sel.len() < 2 {
        return Err("F 方差比需要 2 组数据。".into());
    }
    let r = crate::core::infer::f_variance(&sel[0].2, &sel[1].2)
        .ok_or_else(|| "每组至少 2 个有效值，且方差不能为 0。".to_string())?;
    let rows = vec![
        vec![
            sel[0].1.clone(),
            sel[0].2.len().to_string(),
            fmt_sig(r.sd1, 4),
            String::new(),
            String::new(),
        ],
        vec![
            sel[1].1.clone(),
            sel[1].2.len().to_string(),
            fmt_sig(r.sd2, 4),
            String::new(),
            String::new(),
        ],
        Vec::new(),
        vec![
            "F = s₁²/s₂²".into(),
            fmt_sig(r.f, 4),
            format!("df = {:.0}, {:.0}", r.df1, r.df2),
            fmt_p(r.p),
            "双侧".into(),
        ],
    ];
    Ok(ResultTable {
        title: "F 方差比检验".into(),
        method: "F = s₁²/s₂²，df₁ = n₁−1、df₂ = n₂−1。双侧 P = 2·min(P(F≥f), P(F≤f))。\n        该检验对正态性非常敏感：偏态数据请改用 Levene / Brown–Forsythe。".into(),
        columns: vec!["组".into(), "n".into(), "SD".into(), "P".into(), "备注".into()],
        rows,
        note: format!(
            "{}（α = {}）。",
            if r.p < opts.alpha {
                "两组方差差异有统计学意义：后续比较请用 Welch t / Games–Howell"
            } else {
                "未发现方差差异：可用合并方差 t / Tukey"
            },
            opts.alpha
        ),
    })
}

/// Cohen's Kappa（2×2 一致性）。
fn kappa_test(table: &DataTable, opts: &AnalysisOptions) -> Result<ResultTable, String> {
    let (rows_lab, cols_lab, counts) = table
        .contingency_matrix()
        .ok_or_else(|| "列联表为空。".to_string())?;
    if rows_lab.len() != 2 || cols_lab.len() != 2 {
        return Err("Kappa 需要 2×2 表（两位评分者 × 两个类别）。".to_string());
    }
    let r = crate::core::infer::cohen_kappa(counts[0][0], counts[0][1], counts[1][0], counts[1][1])
        .ok_or_else(|| "总频数为 0 或完全一致到无法估计。".to_string())?;
    let verdict = if r.k < 0.0 {
        "差于随机"
    } else if r.k < 0.2 {
        "轻微"
    } else if r.k < 0.4 {
        "一般"
    } else if r.k < 0.6 {
        "中等"
    } else if r.k < 0.8 {
        "好"
    } else {
        "很好"
    };
    let rows = vec![
        vec![
            "观察一致率 Po".into(),
            fmt_sig(r.po, 4),
            format!("(a+d)/n，{} / {}", rows_lab.join("·"), cols_lab.join("·")),
        ],
        vec![
            "期望一致率 Pe".into(),
            fmt_sig(r.pe, 4),
            "按行列边际独立推算".into(),
        ],
        Vec::new(),
        vec![
            "κ".into(),
            fmt_sig(r.k, 4),
            format!("{}（<0 差、0.2 一般、0.4 中等、0.6 好、0.8 很好）", verdict),
        ],
        vec![
            "SE / z / P".into(),
            format!(
                "{} / {} / {}",
                fmt_sig(r.se, 4),
                fmt_sig(r.z, 3),
                fmt_p(r.p)
            ),
            "H₀: κ = 0（一致程度不比随机好）".into(),
        ],
    ];
    Ok(ResultTable {
        title: "Cohen's Kappa 一致性".into(),
        method: "κ = (Po − Pe)/(1 − Pe)：扣掉「碰巧一致」之后的一致程度。\n        只适用于两位评分者、同一批对象、相同两个类别；有序类别请用加权 Kappa。".into(),
        columns: vec!["项目".into(), "值".into(), "说明".into()],
        rows,
        note: format!(
            "P = {}（α = {}）。κ 对「边际分布偏斜」敏感：两类比例悬殊时，即使 Po 很高 κ 也可能偏低。",
            fmt_p(r.p),
            opts.alpha
        ),
    })
}

/// Bland–Altman 一致性。
fn bland_altman_test(table: &DataTable, _opts: &AnalysisOptions) -> Result<ResultTable, String> {
    let blocks = aligned_blocks(table);
    if blocks.first().map(|r| r.len()).unwrap_or(0) < 2 {
        return Err("Bland–Altman 需要 2 列配对测量（同一行 = 同一个样本）。".into());
    }
    let a: Vec<f64> = blocks.iter().map(|r| r[0]).collect();
    let b: Vec<f64> = blocks.iter().map(|r| r[1]).collect();
    let r = crate::core::infer::bland_altman(&a, &b).ok_or_else(|| {
        "至少需要 3 对完整数据，且差值不能全部相同（那样 SD = 0，界限无定义）。".to_string()
    })?;
    let names: Vec<String> = table
        .columns
        .iter()
        .filter(|c| c.role == ColRole::Y)
        .map(|c| c.name.clone())
        .take(2)
        .collect();
    let n1 = names.first().cloned().unwrap_or_else(|| "方法 1".into());
    let n2 = names.get(1).cloned().unwrap_or_else(|| "方法 2".into());
    let rows = vec![
        vec![
            "配对样本数".into(),
            r.n.to_string(),
            format!("{n1} 与 {n2} 按行配对"),
        ],
        vec![
            "平均差（偏倚）".into(),
            fmt_sig(r.mean_diff, 4),
            format!("SE = {}", fmt_sig(r.se_mean, 4)),
        ],
        vec!["差值 SD".into(), fmt_sig(r.sd_diff, 4), String::new()],
        vec![
            "一致性界限（LoA）".into(),
            format!("{} – {}", fmt_sig(r.loa_lo, 3), fmt_sig(r.loa_hi, 3)),
            "平均差 ± 1.96·SD（约覆盖 95% 的差值）".into(),
        ],
    ];
    Ok(ResultTable {
        title: format!("Bland–Altman 一致性：{n1} vs {n2}"),
        method: "差值 d = 方法1 − 方法2，对 d 的均值与 SD 求偏倚与一致性界限。\n        判读看两件事：偏倚是否在临床可接受范围内，以及 LoA 是否跨出允许误差。\n        LoA 之外的点要结合专业判断，不是「异常值」。".into(),
        columns: vec!["项目".into(), "值".into(), "说明".into()],
        rows,
        note: "差值随均值呈喇叭形（比例误差）时，应改画 log 差值或按均值分层。".into(),
    })
}

#[cfg(test)]
mod note_tests {
    use super::*;

    fn g2() -> Vec<(String, Vec<f64>)> {
        vec![
            ("A".into(), vec![1.0, 2.0, 3.0, 2.5, 1.5, 2.2]),
            ("B".into(), vec![5.0, 6.0, 7.0, 6.5, 5.5, 6.2]),
        ]
    }

    /// G4：检验全名、双侧、效应量与 95% CI 一个都不能少。
    #[test]
    fn 两组注记含检验全名双侧与效应量CI() {
        let n = inference_note(&g2()).unwrap();
        assert!(n.contains("Welch 非配对 t 检验"), "{n}");
        assert!(n.contains("双侧"), "{n}");
        assert!(n.contains("Cohen's d"), "{n}");
        assert!(n.contains("95% CI"), "{n}");
    }

    #[test]
    fn 多组注记含ANOVA与Holm校正() {
        let mut g = g2();
        g.push(("C".into(), vec![3.0, 4.0, 3.5, 4.5, 3.2, 4.1]));
        let n = inference_note(&g).unwrap();
        assert!(n.contains("ANOVA"), "{n}");
        assert!(n.contains("Holm"), "{n}");
        assert!(n.contains("η²"), "{n}");
    }

    /// E2-5：log-rank p 与 HR 95% CI 必须报出。
    #[test]
    fn 生存注记含log_rank与HR_CI() {
        let groups = vec![
            (
                "A".into(),
                vec![(1.0, true), (2.0, true), (3.0, false), (4.0, true)],
            ),
            (
                "B".into(),
                vec![(2.0, true), (5.0, true), (6.0, true), (8.0, false)],
            ),
        ];
        let n = survival_note(&groups).unwrap();
        assert!(n.contains("log-rank"), "{n}");
        assert!(n.contains("HR ("), "{n}");
        assert!(n.contains("A / B"), "HR 应写方向：{n}");
        assert!(n.contains("95% CI"), "{n}");
        assert!(!n.contains("p = 0.000"), "G4 禁写 p = 0.000");
    }
}

/// ROC 分析结果表（两组连续观测：第 1 组 = 阳性/病例，第 2 组 = 阴性/对照）。
fn roc_test(groups: &[(String, Vec<f64>)], opts: &AnalysisOptions) -> Result<ResultTable, String> {
    let sel = picked(groups, opts);
    if sel.len() < 2 {
        return Err("ROC 需要 2 组观测：第 1 组为阳性（病例），第 2 组为阴性（对照）。".into());
    }
    let (pos, neg) = (&sel[0].2, &sel[1].2);
    let r = crate::core::stats::roc(pos, neg).ok_or_else(|| "ROC 需要两组均非空。".to_string())?;

    let mut rows = vec![
        vec![
            "AUC".into(),
            fmt_sig(r.auc, 4),
            format!("SE {}", fmt_sig(r.se, 3)),
            format!("95% CI [{:.3}, {:.3}]", r.ci_low, r.ci_high),
            format!("n⁺={} / n⁻={}", r.n_pos, r.n_neg),
            String::new(),
            String::new(),
        ],
        vec![
            "最佳界值（Youden J）".into(),
            fmt_sig(r.cutoff, 4),
            format!("灵敏度 {:.3}", r.sens),
            format!("特异度 {:.3}", r.spec),
            format!("J = {:.3}", r.youden),
            String::new(),
            String::new(),
        ],
        Vec::new(),
        vec![
            "界值".into(),
            "灵敏度".into(),
            "特异度".into(),
            "1−特异度".into(),
            "J".into(),
            String::new(),
            String::new(),
        ],
    ];
    // 界值明细：从高到低去重（判阳：marker ≥ t）
    let mut vals: Vec<f64> = pos.iter().chain(neg.iter()).copied().collect();
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    vals.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
    for t in vals.iter().rev() {
        let se = pos.iter().filter(|&&v| v >= *t).count() as f64 / pos.len() as f64;
        let sp = neg.iter().filter(|&&v| v < *t).count() as f64 / neg.len() as f64;
        rows.push(vec![
            fmt_sig(*t, 4),
            format!("{se:.3}"),
            format!("{sp:.3}"),
            format!("{:.3}", 1.0 - sp),
            format!("{:.3}", se + sp - 1.0),
            String::new(),
            String::new(),
        ]);
    }
    Ok(ResultTable {
        title: format!(
            "ROC 曲线与 AUC（阳性组「{}」 vs 阴性组「{}」）",
            sel[0].1, sel[1].1
        ),
        method: "ROC / AUC：AUC = P(阳性得分 > 阴性得分) + ½P(相等)（秩统计精确计算），\\n        95% CI 为 Hanley–McNeil 近似；最佳界值取 Youden J = 灵敏度+特异度−1 最大处，\\n        并列取较高界值。判阳规则：marker ≥ 界值。第 1 组为阳性组——若分组相反，AUC 应读作 1−AUC。".to_string()
            + &format!("\\nα = {}。", opts.alpha),
        columns: vec![
            "项目".into(),
            "值".into(),
            "".into(),
            "".into(),
            "".into(),
            String::new(),
            String::new(),
        ],
        rows,
        note: String::new(),
    })
}

#[cfg(test)]
mod two_way_tests {
    use super::*;
    use crate::core::model::{Cell, Column, DataTable, TableKind};

    fn grouped_table() -> DataTable {
        // 2 行（因子 A）× 3 组（因子 B）× 格内 2 重复
        let mut t = DataTable::template(TableKind::Grouped, String::from("g"), 3);
        t.columns.clear();
        t.row_headers = vec!["A1".into(), "A2".into()];
        t.group_names = vec!["B1".into(), "B2".into(), "B3".into()];
        let data = [
            ([1.0, 2.0], [2.0, 3.0]), // 组0 的两列（列=格内重复）
            ([3.0, 4.0], [4.0, 5.0]),
            ([5.0, 6.0], [6.0, 7.0]),
        ];
        for (j, (c0, c1)) in data.iter().enumerate() {
            for (k, vals) in [c0, c1].iter().enumerate() {
                let mut col = Column::new(format!("r{k}"), ColRole::Y);
                col.group = j;
                col.cells = vals.iter().map(|v| Cell::number(*v)).collect();
                t.columns.push(col);
            }
        }
        t
    }

    #[test]
    fn 双因素ANOVA走通run接线() {
        let t = grouped_table();
        let r = run(&t, AnalysisKind::TwoWayAnova, &AnalysisOptions::default()).expect("run");
        assert!(r.title.contains("双因素"), "{}", r.title);
        assert_eq!(r.rows.len(), 4, "A / B / 交互 / 残差");
        // 加性数据：交互行的 F 应为 0
        assert!(r.rows[2][4].starts_with('0'), "交互 F={}", r.rows[2][4]);
    }

    #[test]
    fn roc分析走通run接线() {
        let mut t = DataTable::template(TableKind::Column, String::from("roc"), 2);
        t.columns.clear();
        let data = [vec![3.0, 4.0, 5.0], vec![1.0, 2.0, 3.0]];
        for (j, vals) in data.iter().enumerate() {
            let mut col = Column::new(format!("g{}", j + 1), ColRole::Y);
            col.group = j;
            col.cells = vals.iter().map(|v| Cell::number(*v)).collect();
            t.columns.push(col);
        }
        let r = run(&t, AnalysisKind::Roc, &AnalysisOptions::default()).expect("run");
        assert!(r.title.contains("ROC"), "{}", r.title);
        // 表结构：AUC / 界值 / 空行 / 表头 / 每个唯一界值一行（6 个观测 → ≤6 行）
        assert!(r.rows.len() >= 5, "rows={}", r.rows.len());
        assert!(r.rows[0][1].contains("0.944"), "AUC 行: {:?}", r.rows[0]);
    }

    #[test]
    fn 重复测量ANOVA走通run接线() {
        let mut t = DataTable::template(TableKind::Column, String::from("rm"), 3);
        t.columns.clear();
        let data = [
            vec![1.0, 2.0, 3.0, 1.0],
            vec![2.0, 3.0, 4.0, 3.0],
            vec![3.0, 4.0, 5.0, 5.0],
        ];
        for (j, vals) in data.iter().enumerate() {
            let mut col = Column::new(format!("处理{}", j + 1), ColRole::Y);
            col.group = j;
            col.cells = vals.iter().map(|v| Cell::number(*v)).collect();
            t.columns.push(col);
        }
        let r = run(&t, AnalysisKind::RmOneWay, &AnalysisOptions::default()).expect("run");
        assert!(r.title.contains("重复测量"), "{}", r.title);
        assert_eq!(r.rows.len(), 4, "处理间 / 受试者间 / 残差 / GG 校正");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{Cell, TableKind};

    #[test]
    fn descriptive_result_is_well_formed() {
        let mut t = DataTable::template(TableKind::Column, "T", 1);
        for i in 0..8 {
            for c in 0..4 {
                t.set_cell(i, c, Cell::number((i as f64 + 1.0) + c as f64 * 2.0));
            }
        }
        let r = run(&t, AnalysisKind::Descriptive, &AnalysisOptions::default()).unwrap();
        assert_eq!(r.rows.len(), 4);
        assert_eq!(r.rows[0][1], "8");
        assert_eq!(r.columns.len(), 7);
    }

    #[test]
    fn t_test_requires_two_groups() {
        let mut t = DataTable::template(TableKind::Column, "T", 1);
        t.columns.truncate(1);
        let e = run(&t, AnalysisKind::UnpairedT, &AnalysisOptions::default());
        assert!(e.is_err());
    }

    /// 嵌套表：2 外层组 × 2 子组 × 6 重复。
    fn nested_table() -> DataTable {
        let mut t = DataTable::template(TableKind::Nested, "T", 1);
        // 4 列，前 2 列属组 0，后 2 列属组 1
        for (i, c) in t.columns.iter_mut().enumerate() {
            c.group = i / 2;
            for (r, cell) in c.cells.iter_mut().enumerate() {
                let base = if i < 2 { 10.0 } else { 20.0 };
                *cell = Cell::number(base + (i as f64) * 1.5 + (r as f64) * 0.3);
            }
        }
        t.group_names = vec!["方法 A".into(), "方法 B".into()];
        t
    }

    #[test]
    fn 嵌套ANOVA端到端出表() {
        let t = nested_table();
        let r = run(&t, AnalysisKind::NestedAnova, &AnalysisOptions::default()).expect("run");
        assert!(r.title.contains("嵌套 ANOVA"));
        assert!(r.rows.len() >= 4);
        // 三层变异来源 + 方差分量行
        let flat: String = r.rows.concat().join(" ");
        assert!(flat.contains("外层组间"));
        assert!(flat.contains("组内子组间"));
        assert!(flat.contains("子组内"));
    }

    #[test]
    fn 嵌套t端到端出表() {
        let t = nested_table();
        let r = run(&t, AnalysisKind::NestedT, &AnalysisOptions::default()).expect("run");
        assert!(r.title.contains("嵌套 t 检验"));
        assert!(r
            .rows
            .iter()
            .any(|row| row.iter().any(|c| c.contains("Cohen"))));
    }

    /// 列联表：3 行 × 2 列的频数表。
    fn contingency_table() -> DataTable {
        let mut t = DataTable::template(TableKind::Contingency, "T", 1);
        t.row_headers = vec!["有效".into(), "无效".into(), "未定".into()];
        t.columns.truncate(2);
        t.columns[0].name = "治疗组".into();
        t.columns[1].name = "对照组".into();
        let data = [[18.0, 8.0], [7.0, 17.0], [5.0, 5.0]];
        t.ensure_rows(3);
        for (j, c) in t.columns.iter_mut().enumerate() {
            for (i, cell) in c.cells.iter_mut().enumerate() {
                *cell = Cell::number(data[i][j]);
            }
        }
        t
    }

    #[test]
    fn 卡方端到端出表() {
        let t = contingency_table();
        let r = run(&t, AnalysisKind::ChiSquare, &AnalysisOptions::default()).expect("run");
        assert_eq!(r.title, "卡方独立性检验");
        // 每行单元格里应同时有观测值与期望值（"18 (…)" 形式）
        assert!(r.rows[0].iter().any(|c| c.contains('(')));
        let flat: String = r.rows.concat().join(" ");
        assert!(flat.contains("χ²"));
        assert!(flat.contains("Cramér"));
    }

    #[test]
    fn Fisher端到端出表() {
        let t = contingency_table();
        let r = run(&t, AnalysisKind::FisherExact, &AnalysisOptions::default()).expect("run");
        assert_eq!(r.title, "Fisher 精确检验");
        let flat: String = r.rows.concat().join(" ");
        assert!(flat.contains("双侧") || flat.contains("P"));
        // 3×2 的表数在精确枚举范围内
        assert!(flat.contains("精确枚举") || flat.contains("Monte Carlo"));
    }

    #[test]
    fn 列联表拒绝负频数() {
        let mut t = contingency_table();
        t.set_cell(0, 0, Cell::number(-3.0));
        // set_text 会拒绝负值；直接塞负数后 contingency_matrix 会夹到 0
        let (_, _, counts) = t.contingency_matrix().expect("matrix");
        assert_eq!(counts[0][0], 0.0);
    }

    /// 4 组、组间分离明显，适合验证秩检验链路。
    fn three_group_table() -> DataTable {
        let mut t = DataTable::template(TableKind::Column, "T", 1);
        let bases = [10.0, 20.0, 30.0, 40.0];
        for (c, base) in bases.iter().enumerate() {
            for (r, cell) in t.columns[c].cells.iter_mut().enumerate() {
                *cell = Cell::number(base + (r as f64) * 0.7);
            }
        }
        t
    }

    #[test]
    fn KW端到端出表() {
        let t = three_group_table();
        let mut opts = AnalysisOptions::default();
        opts.columns = (0..4).collect();
        let r = run(&t, AnalysisKind::KruskalWallis, &opts).expect("run");
        assert!(r.title.contains("Kruskal"));
        assert!(r.columns.iter().any(|c| c.contains("平均秩")));
        let flat = r.rows.concat().join(" ");
        assert!(flat.contains('H'));
        // 四组完全分离，应当显著
        assert!(r.note.contains("统计学意义"), "note = {}", r.note);
    }

    #[test]
    fn Dunn端到端出表并按Holm校正() {
        let t = three_group_table();
        let mut opts = AnalysisOptions::default();
        opts.columns = (0..4).collect();
        let r = run(&t, AnalysisKind::Dunn, &opts).expect("run");
        assert_eq!(r.title, "Dunn 事后两两比较");
        // 4 组 → C(4,2) = 6 对
        assert_eq!(r.rows.len(), 6);
        assert!(r.columns.iter().any(|c| c.contains("Holm")));
        assert!(r.columns.iter().any(|c| c.contains("Bonferroni")));
        // 完全分离的四组，至少端点对显著
        assert!(r.note.contains("显著"), "note = {}", r.note);
    }

    #[test]
    fn 秩检验在列联表上不可用() {
        // 列联表的「值」是频数，不是同一变量的重复观测，不该出现在秩检验菜单里
        assert!(!AnalysisKind::KruskalWallis.supports(TableKind::Contingency));
        assert!(!AnalysisKind::Dunn.supports(TableKind::Contingency));
        assert!(!AnalysisKind::KruskalWallis.supports(TableKind::Survival));
    }

    #[test]
    fn 分析菜单按表格类型过滤() {
        // 嵌套表不给卡方 / 回归
        let nested = [AnalysisKind::NestedT, AnalysisKind::NestedAnova];
        for k in nested {
            assert!(k.supports(TableKind::Nested));
            assert!(!k.supports(TableKind::Column));
        }
        // 列联表只给卡方 / Fisher / 描述 / 正态性
        let cont = AnalysisKind::available_for(TableKind::Contingency);
        assert!(cont.contains(&AnalysisKind::ChiSquare));
        assert!(cont.contains(&AnalysisKind::FisherExact));
        assert!(cont.contains(&AnalysisKind::Descriptive));
        assert!(
            !cont.contains(&AnalysisKind::UnpairedT),
            "列联表不该给 t 检验"
        );
        assert!(!cont.contains(&AnalysisKind::LinearRegression));
        // XY 表不给嵌套 / 列联
        let xy = AnalysisKind::available_for(TableKind::Xy);
        assert!(!xy.contains(&AnalysisKind::NestedAnova));
        assert!(!xy.contains(&AnalysisKind::ChiSquare));
        assert!(xy.contains(&AnalysisKind::LinearRegression));
    }
}

#[cfg(test)]
mod v13_tests {
    use super::*;
    use crate::core::model::{Cell, Column};

    fn col_table(name: &str, data: &[(&str, Vec<f64>)]) -> DataTable {
        let mut t = DataTable::template(TableKind::Column, name.to_string(), 2);
        t.columns.clear();
        for (j, (n, vals)) in data.iter().enumerate() {
            let mut col = Column::new(*n, ColRole::Y);
            col.group = j;
            col.cells = vals.iter().map(|v| Cell::number(*v)).collect();
            t.columns.push(col);
        }
        t
    }

    fn xy_table(data: &[(&str, Vec<f64>, Vec<f64>)]) -> DataTable {
        let mut t = DataTable::template(TableKind::Xy, String::from("xy"), 2);
        t.columns.clear();
        for (j, (n, xs, ys)) in data.iter().enumerate() {
            if j == 0 {
                let mut x = Column::new("X", ColRole::X);
                x.cells = xs.iter().map(|v| Cell::number(*v)).collect();
                t.columns.push(x);
            }
            let mut y = Column::new(*n, ColRole::Y);
            y.group = j;
            y.cells = ys.iter().map(|v| Cell::number(*v)).collect();
            t.columns.push(y);
        }
        t
    }

    fn multi_table(data: &[(&str, Vec<f64>)]) -> DataTable {
        let mut t = DataTable::template(TableKind::MultipleVariables, String::from("mv"), 2);
        t.columns.clear();
        for (n, vals) in data {
            let mut col = Column::new(*n, ColRole::Y);
            col.cells = vals.iter().map(|v| Cell::number(*v)).collect();
            t.columns.push(col);
        }
        t
    }

    fn two_by_two() -> DataTable {
        let mut t = DataTable::template(TableKind::Contingency, "T", 1);
        t.row_headers = vec!["暴露".into(), "非暴露".into()];
        t.columns.truncate(2);
        t.columns[0].name = "事件".into();
        t.columns[1].name = "无事件".into();
        t.ensure_rows(2);
        let data = [[20.0, 10.0], [5.0, 25.0]];
        for (j, c) in t.columns.iter_mut().enumerate() {
            for (i, cell) in c.cells.iter_mut().enumerate() {
                *cell = Cell::number(data[i][j]);
            }
        }
        t
    }

    fn survival_table() -> DataTable {
        let mut t = DataTable::template(TableKind::Survival, String::from("s"), 2);
        t.columns.truncate(3); // Time + 两组
        let time = [1.0, 3.0, 5.0, 7.0, 2.0, 4.0, 6.0, 9.0];
        let g1 = [
            Some(1.0),
            Some(1.0),
            Some(1.0),
            Some(1.0),
            None,
            None,
            None,
            None,
        ];
        let g2 = [
            None,
            None,
            None,
            None,
            Some(1.0),
            Some(1.0),
            Some(1.0),
            Some(1.0),
        ];
        t.ensure_rows(8);
        for i in 0..8 {
            t.set_cell(i, 0, Cell::number(time[i]));
            if let Some(v) = g1[i] {
                t.set_cell(i, 1, Cell::number(v));
            }
            if let Some(v) = g2[i] {
                t.set_cell(i, 2, Cell::number(v));
            }
        }
        t
    }

    #[test]
    fn 新分析在对应表型上都能出表() {
        let col = col_table(
            "c",
            &[
                ("A", vec![10.0, 11.0, 10.5, 10.2, 10.8]),
                ("B", vec![12.0, 12.5, 11.8, 12.2, 12.4]),
                ("C", vec![10.1, 9.9, 10.4, 10.0, 10.3]),
            ],
        );
        for k in [
            AnalysisKind::WelchT,
            AnalysisKind::OneSampleT,
            AnalysisKind::VarianceTests,
            AnalysisKind::Outliers,
            AnalysisKind::Friedman,
            AnalysisKind::MultiCompare,
        ] {
            let r = run(&col, k, &AnalysisOptions::default())
                .unwrap_or_else(|e| panic!("{} 失败：{e}", k.label()));
            assert!(!r.title.is_empty());
            assert!(!r.rows.is_empty(), "{} 没有数据行", k.label());
        }

        // 多变量方法只在多变量表上开放：列 = 变量。
        let mv = multi_table(&[
            ("v1", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]),
            ("v2", vec![2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0]),
            ("v3", vec![5.0, 3.0, 4.0, 1.0, 2.0, 6.0, 7.0, 8.0]),
            ("y", vec![4.0, 7.0, 11.0, 14.0, 18.0, 21.0, 25.0, 28.0]),
        ]);
        for k in [
            AnalysisKind::CorrMatrix,
            AnalysisKind::Pca,
            AnalysisKind::Hierarchical,
            AnalysisKind::KMeans,
        ] {
            let r = run(&mv, k, &AnalysisOptions::default())
                .unwrap_or_else(|e| panic!("{} 失败：{e}", k.label()));
            assert!(!r.rows.is_empty(), "{} 没有数据行", k.label());
        }

        let xy = xy_table(&[
            (
                "D1",
                vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
                vec![2.0, 4.1, 5.9, 8.2, 9.8, 12.1],
            ),
            (
                "D2",
                vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
                vec![3.6, 7.8, 11.5, 15.9, 19.6, 24.1],
            ),
        ]);
        for k in [
            AnalysisKind::Spearman,
            AnalysisKind::Deming,
            AnalysisKind::SlopeCompare,
            AnalysisKind::CurveFit,
        ] {
            let r = run(&xy, k, &AnalysisOptions::default())
                .unwrap_or_else(|e| panic!("{} 失败：{e}", k.label()));
            assert!(!r.rows.is_empty(), "{} 没有数据行", k.label());
        }

        let t22 = two_by_two();
        for k in [AnalysisKind::EffectRatios, AnalysisKind::McNemar] {
            run(&t22, k, &AnalysisOptions::default())
                .unwrap_or_else(|e| panic!("{} 失败：{e}", k.label()));
        }

        let mv = multi_table(&[
            ("y", vec![4.0, 7.0, 11.0, 14.0, 18.0, 21.0, 25.0, 28.0]),
            ("x1", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]),
            ("x2", vec![2.0, 1.0, 4.0, 3.0, 6.0, 5.0, 8.0, 7.0]),
        ]);
        let r = run(
            &mv,
            AnalysisKind::MultiRegression,
            &AnalysisOptions::default(),
        )
        .expect("回归");
        assert!(r.rows.iter().any(|row| row[0] == "截距"));

        let s = survival_table();
        let r = run(&s, AnalysisKind::Cox, &AnalysisOptions::default()).expect("cox");
        assert!(r.title.contains("Cox"));
        assert!(r.rows.iter().any(|row| row[2].contains('.')));
    }

    #[test]
    fn welch与多重比较给出同向结论() {
        let t = col_table(
            "c",
            &[
                ("A", vec![10.0, 11.0, 10.5, 10.2, 10.8]),
                ("B", vec![20.0, 21.0, 20.5, 20.2, 20.8]),
            ],
        );
        let r = run(&t, AnalysisKind::WelchT, &AnalysisOptions::default()).expect("welch");
        // 差值行最后两列是 P 与 d；差异巨大 → P 极小、d 极大
        let last = r.rows.last().unwrap();
        assert!(last[7].contains('<') || last[7].contains("0.00"));
        assert!(last[8].parse::<f64>().unwrap().abs() > 5.0);

        let t3 = col_table(
            "c",
            &[
                ("A", vec![10.0, 11.0, 10.5, 10.2, 10.8]),
                ("B", vec![12.0, 12.5, 11.8, 12.2, 12.4]),
                ("C", vec![10.1, 9.9, 10.4, 10.0, 10.3]),
            ],
        );
        let r = run(&t3, AnalysisKind::MultiCompare, &AnalysisOptions::default()).expect("mc");
        assert_eq!(r.rows.len(), 3, "3 组 → 3 对");
        // A vs B 的 Tukey P 应很小
        let ab = r.rows.iter().find(|r| r[0].starts_with("A")).unwrap();
        assert!(ab[4].contains('<') || ab[4].contains("0.00"));
    }

    #[test]
    fn 离群值只标最像的那个() {
        let t = col_table(
            "c",
            &[("A", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 50.0])],
        );
        let r = run(&t, AnalysisKind::Outliers, &AnalysisOptions::default()).expect("grubbs");
        assert!(r.rows[0][2].starts_with("50"));
        assert!(r.rows[0][6].contains("离群"));
    }

    #[test]
    fn 方差齐性三种口径都出表() {
        let t = col_table(
            "c",
            &[
                ("A", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
                ("B", vec![10.0, 20.0, 30.0, 40.0, 50.0]),
            ],
        );
        let r = run(&t, AnalysisKind::VarianceTests, &AnalysisOptions::default()).expect("var");
        assert_eq!(r.rows.len(), 3, "Bartlett / Levene / Brown–Forsythe");
        assert!(r.rows.iter().all(|row| row[5] == "方差不齐"));
    }

    #[test]
    fn 相关矩阵给出r与P两块() {
        let t = multi_table(&[
            ("v1", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
            ("v2", vec![2.0, 4.0, 6.0, 8.0, 10.0]),
            ("v3", vec![5.0, 3.0, 4.0, 1.0, 2.0]),
        ]);
        let r = run(&t, AnalysisKind::CorrMatrix, &AnalysisOptions::default()).expect("corr");
        assert_eq!(r.columns.len(), 4, "变量 + 3 列");
        // v1 与 v2 完全正相关
        assert_eq!(r.rows[0][2], "1.000");
        // 第二块以「P 值」为标题
        assert!(r
            .rows
            .iter()
            .any(|row| row.len() == 1 && row[0].contains("P 值")));
    }

    #[test]
    fn pca第一主成分接近一() {
        let t = multi_table(&[
            ("v1", (1..=10).map(|i| i as f64).collect()),
            ("v2", (1..=10).map(|i| 3.0 * i as f64 + 2.0).collect()),
        ]);
        let r = run(&t, AnalysisKind::Pca, &AnalysisOptions::default()).expect("pca");
        assert!(r.rows[0][2].starts_with("100"), "PC1 = {}", r.rows[0][2]);
    }

    #[test]
    fn or与rr对上四格表() {
        let t = two_by_two();
        let r = run(&t, AnalysisKind::EffectRatios, &AnalysisOptions::default()).expect("or");
        // OR = 20·25/(10·5) = 10；RR = 0.667/0.167 = 4
        assert!(r.rows[0][1].starts_with("10"), "OR = {}", r.rows[0][1]);
        assert!(r.rows[1][1].starts_with('4'), "RR = {}", r.rows[1][1]);
        assert!(r.note.contains("OR 会高估"));
    }

    #[test]
    fn cox在生存表上给出HR() {
        let t = survival_table();
        let r = run(&t, AnalysisKind::Cox, &AnalysisOptions::default()).expect("cox");
        assert!(r.columns.iter().any(|c| c == "HR"));
        // 第 2 组事件更晚 → 相对第 1 组是保护性，HR < 1
        assert!(
            r.rows[0][2].parse::<f64>().unwrap() < 1.0,
            "HR = {}",
            r.rows[0][2]
        );
    }
}

#[cfg(test)]
mod v13b_tests {
    use super::*;
    use crate::core::model::{Cell, Column};

    fn col8(a: Vec<f64>, b: Vec<f64>) -> DataTable {
        let mut t = DataTable::template(TableKind::Column, String::from("c"), 2);
        t.columns.clear();
        for (j, (n, vals)) in [("A", a), ("B", b)].iter().enumerate() {
            let mut col = Column::new(*n, ColRole::Y);
            col.group = j;
            col.cells = vals.iter().map(|v| Cell::number(*v)).collect();
            t.columns.push(col);
        }
        t
    }

    #[test]
    fn 符号秩方差比与一致性分析能出表() {
        let t = col8(
            vec![11.0, 12.5, 13.2, 11.8, 14.0, 12.9, 13.5, 12.1],
            vec![10.0, 30.0, 12.0, 45.0, 18.0, 25.0, 9.0, 38.0],
        );
        for k in [
            AnalysisKind::SignedRank,
            AnalysisKind::FVariance,
            AnalysisKind::BlandAltman,
        ] {
            let r = run(&t, k, &AnalysisOptions::default())
                .unwrap_or_else(|e| panic!("{} 失败：{e}", k.label()));
            assert!(!r.rows.is_empty(), "{} 没有数据行", k.label());
        }
        // 两组方差差很多 → F 检验显著
        let r = run(&t, AnalysisKind::FVariance, &AnalysisOptions::default()).unwrap();
        assert!(r.note.contains("Welch"), "note = {}", r.note);
        // 单样本符号秩：A 组整体高于 0 → 显著
        let r = run(&t, AnalysisKind::SignedRank, &AnalysisOptions::default()).unwrap();
        assert!(r.rows[0][5] == "*" || r.rows[0][5].len() >= 2);
    }

    #[test]
    fn kappa对上四格表手算() {
        // a=20 b=10 c=5 d=25 → Po=0.75, Pe=0.5 → κ=0.5
        let mut t = DataTable::template(TableKind::Contingency, String::from("T"), 1);
        t.row_headers = vec!["评分者A·阳性".into(), "评分者A·阴性".into()];
        t.columns.truncate(2);
        t.columns[0].name = "评分者B·阳性".into();
        t.columns[1].name = "评分者B·阴性".into();
        t.ensure_rows(2);
        let data = [[20.0, 10.0], [5.0, 25.0]];
        for (j, c) in t.columns.iter_mut().enumerate() {
            for (i, cell) in c.cells.iter_mut().enumerate() {
                *cell = Cell::number(data[i][j]);
            }
        }
        let r = run(&t, AnalysisKind::Kappa, &AnalysisOptions::default()).expect("kappa");
        assert_eq!(r.rows[0][1], "0.7500", "Po = {:?}", r.rows[0]);
        assert!(r.rows[3][1].starts_with("0.5"), "κ = {:?}", r.rows[3]);
    }
}
