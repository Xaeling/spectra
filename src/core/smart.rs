//! 智能分析：一张表 → 一份「全指标」报告。
//!
//! 设计目标与 Prism 的「Analyze all」不同：不是把菜单里的分析全跑一遍堆给用户，
//! 而是按**统计决策链**给出**该表型应有的全部指标**，并且每一步都写清判读：
//!
//! 1. 数据概览（每组 n、中心与离散度、区间）；
//! 2. 前提检验（正态性、方差齐性、离群值）——决定后面用哪条路；
//! 3. 主检验（按前提自动选择参数 / 非参数、等方差 / 不等方差）；
//! 4. 事后比较（只在对应用得上的时候给）；
//! 5. 结论与建议（把上面的数字翻译成一句话）。
//!
//! 输出统一为三列：`项目 | 结果 | 说明`，块与块之间用空行分隔——渲染层会把
//! 每块画成独立的圆角卡（见 `app/results.rs`）。

use crate::core::infer;
use crate::core::model::{ColRole, DataTable, ResultTable, TableKind};
use crate::core::stats;

type Rows = Vec<Vec<String>>;

fn f(v: f64, d: usize) -> String {
    if !v.is_finite() {
        return "—".into();
    }
    let a = v.abs();
    if a != 0.0 && (a < 1e-3 || a >= 1e5) {
        format!("{v:.2e}")
    } else {
        format!("{v:.d$}")
    }
}

fn p_txt(p: f64) -> String {
    if !p.is_finite() {
        return "—".into();
    }
    if p < 0.0001 {
        "< 0.0001".into()
    } else {
        format!("{p:.4}")
    }
}

fn row(a: impl Into<String>, b: impl Into<String>, c: impl Into<String>) -> Vec<String> {
    vec![a.into(), b.into(), c.into()]
}

fn block(rows: &mut Rows, title: &str) {
    if !rows.is_empty() {
        rows.push(Vec::new());
    }
    rows.push(vec![title.to_string()]);
}

fn verdict(sig: bool) -> &'static str {
    if sig {
        "差异有统计学意义"
    } else {
        "未发现差异"
    }
}

/// 效应量大小的口语化判读（Cohen 惯例）。
fn eta_word(e: f64) -> &'static str {
    if e >= 0.14 {
        "大"
    } else if e >= 0.06 {
        "中"
    } else if e >= 0.01 {
        "小"
    } else {
        "可忽略"
    }
}

fn d_word(d: f64) -> &'static str {
    let a = d.abs();
    if a >= 0.8 {
        "大"
    } else if a >= 0.5 {
        "中"
    } else if a >= 0.2 {
        "小"
    } else {
        "可忽略"
    }
}

pub fn report(table: &DataTable, alpha: f64) -> Result<ResultTable, String> {
    let (title, rows) = match table.kind {
        TableKind::Column => column_report(table, alpha)?,
        TableKind::Xy => xy_report(table, alpha)?,
        TableKind::Grouped => grouped_report(table, alpha)?,
        TableKind::Contingency => contingency_report(table, alpha)?,
        TableKind::Survival => survival_report(table, alpha)?,
        TableKind::MultipleVariables => multi_report(table, alpha)?,
        TableKind::Nested => nested_report(table, alpha)?,
        TableKind::PartsOfWhole => parts_report(table, alpha)?,
    };
    Ok(ResultTable {
        title,
        method: format!(
            "按统计决策链自动执行：数据概览 → 前提检验（正态性 / 方差齐性 / 离群值）→ \
             主检验（按前提选参数或非参数、等方差或不等方差）→ 事后比较 → 结论。\n\
             每一步的判读都写在「说明」列，α = {alpha}。\n\
             需要某一步的完整表格（例如 ANOVA 的 SS/df/MS 明细）时，再单独运行对应分析。"
        ),
        columns: vec!["项目".into(), "结果".into(), "说明".into()],
        rows,
        note: "智能分析只做「推荐路径」，不会替你改数据；离群值是否剔除、用参数还是非参数，最终要结合实验设计判断。".into(),
    })
}

// ---------------------------------------------------------------------------
// 柱状表：组间比较
// ---------------------------------------------------------------------------

fn column_report(table: &DataTable, alpha: f64) -> Result<(String, Rows), String> {
    let groups: Vec<(String, Vec<f64>)> = table.analysis_groups();
    let groups: Vec<(String, Vec<f64>)> =
        groups.into_iter().filter(|(_, v)| v.len() >= 2).collect();
    if groups.len() < 2 {
        return Err("智能分析至少需要 2 组、每组 ≥2 个有效值。".into());
    }
    let vals: Vec<Vec<f64>> = groups.iter().map(|(_, v)| v.clone()).collect();
    let k = groups.len();
    let n_total: usize = vals.iter().map(|v| v.len()).sum();
    let mut rows: Rows = Vec::new();

    // ---- 一、概览 ----
    block(&mut rows, "一、数据概览");
    for (name, v) in &groups {
        let d = stats::describe(v).ok_or_else(|| format!("「{name}」的有效值不足。"))?;
        rows.push(row(
            format!("{name}（n = {}）", d.n),
            format!("{} ± {}", f(d.mean, 3), f(d.sd, 3)),
            format!(
                "中位数 {}（{} – {}）；95% CI {} – {}；范围 {} – {}",
                f(d.median, 3),
                f(d.q1, 3),
                f(d.q3, 3),
                f(d.mean - d.ci95, 3),
                f(d.mean + d.ci95, 3),
                f(d.min, 3),
                f(d.max, 3)
            ),
        ));
    }

    // ---- 二、前提 ----
    block(&mut rows, "二、前提检验");
    let mut all_normal = true;
    for (name, v) in &groups {
        match stats::dagostino(v) {
            Some((k2, p)) => {
                if p < alpha {
                    all_normal = false;
                }
                rows.push(row(
                    format!("{name} · 正态性"),
                    format!("K² = {}，P = {}", f(k2, 3), p_txt(p)),
                    if p < alpha {
                        "偏离正态 → 该组按非参数处理"
                    } else {
                        "未偏离正态"
                    },
                ));
            }
            None => {
                all_normal = false;
                rows.push(row(
                    format!("{name} · 正态性"),
                    "样本量不足",
                    "n < 8，D'Agostino 检验不可靠；保守起见按非参数处理",
                ));
            }
        }
        // Shapiro-Wilk（n ≥ 3 时可用，比 D'Agostino 对小样本更敏感）
        if let Some(sw) = crate::core::normality::shapiro_wilk(v) {
            rows.push(row(
                format!("{name} · Shapiro-Wilk"),
                format!("W = {}，P = {}", f(sw.statistic, 4), p_txt(sw.p_value)),
                if sw.p_value < alpha {
                    "SW 检出偏离正态"
                } else {
                    "SW 未检出偏离"
                },
            ));
        }
        // Anderson-Darling（对尾部偏离敏感）
        if let Some(ad) = crate::core::normality::anderson_darling_normal(v) {
            rows.push(row(
                format!("{name} · Anderson-Darling"),
                format!("A*² = {}，P = {}", f(ad.statistic, 3), p_txt(ad.p_value)),
                if ad.p_value < alpha {
                    "AD 侧重尾部：尾部偏离正态"
                } else {
                    "AD 未检出偏离"
                },
            ));
        }
    }
    let lev = infer::levene(&vals, false);
    let bf = infer::levene(&vals, true);
    let var_equal = lev.as_ref().map(|l| l.p >= alpha).unwrap_or(true);
    if let Some(l) = lev {
        rows.push(row(
            "Levene（组均值）",
            format!(
                "F({:.0}, {:.0}) = {}，P = {}",
                l.df1,
                l.df2,
                f(l.stat, 3),
                p_txt(l.p)
            ),
            if l.p < alpha {
                "方差不齐 → 用 Welch t / Games–Howell"
            } else {
                "方差齐"
            },
        ));
    }
    if let Some(b) = bf {
        rows.push(row(
            "Brown–Forsythe（中位数）",
            format!(
                "F({:.0}, {:.0}) = {}，P = {}",
                b.df1,
                b.df2,
                f(b.stat, 3),
                p_txt(b.p)
            ),
            "偏态数据以这一行为准",
        ));
    }
    let mut out_n = 0usize;
    for (name, v) in &groups {
        if let Some(g) = infer::grubbs(v, alpha) {
            let hit = g.g > g.critical;
            if hit {
                out_n += 1;
            }
            rows.push(row(
                format!("{name} · 离群值"),
                format!("G = {}（临界 {}）", f(g.g, 3), f(g.critical, 3)),
                if hit {
                    format!(
                        "检出候选 {}（P = {}），先查来源再决定取舍",
                        f(g.value, 4),
                        p_txt(g.p)
                    )
                } else {
                    "未检出".to_string()
                },
            ));
        }
        // ROUT 离群值检测（FDR 控制，比 Grubbs 更稳健）
        if let Some(rt) = crate::core::posthoc::route_detect(v, alpha) {
            if rt.n_outliers > 0 {
                out_n += rt.n_outliers;
                rows.push(row(
                    format!("{name} · ROUT 离群值"),
                    format!("检出 {} 个", rt.n_outliers),
                    format!(
                        "位置 {:?}；Q ≤ {}；建议核对原始数据后决定是否排除",
                        rt.indices,
                        f(alpha, 3)
                    ),
                ));
            } else {
                rows.push(row(
                    format!("{name} · ROUT 离群值"),
                    "未检出",
                    "FDR 控制下无离群值",
                ));
            }
        }
    }

    // ---- 三、主检验 ----
    block(&mut rows, "三、主检验");
    let significant: bool;
    let main_name: String;
    if k == 2 {
        if all_normal {
            let welch = !var_equal;
            let t = stats::t_test_unpaired(&vals[0], &vals[1], welch)
                .ok_or_else(|| "两组样本不足，无法做 t 检验。".to_string())?;
            significant = t.p < alpha;
            main_name = if welch {
                "Welch t 检验（不等方差）".into()
            } else {
                "非配对 t 检验（合并方差）".into()
            };
            rows.push(row(
                &main_name,
                format!("t({:.1}) = {}，P = {}", t.df, f(t.t, 3), p_txt(t.p)),
                format!(
                    "差值 {}（95% CI {} – {}），Cohen's d = {}（{}效应）；{}",
                    f(t.diff, 3),
                    f(t.ci_low, 3),
                    f(t.ci_high, 3),
                    f(t.cohen_d, 2),
                    d_word(t.cohen_d),
                    verdict(significant)
                ),
            ));
        } else {
            let (u, p) =
                stats::mann_whitney(&vals[0], &vals[1]).ok_or_else(|| "样本不足。".to_string())?;
            significant = p < alpha;
            main_name = "Mann–Whitney U".into();
            rows.push(row(
                &main_name,
                format!("U = {}，P = {}", f(u, 1), p_txt(p)),
                format!(
                    "{}；中位数之差 {}（非参数检验不报均值差）",
                    verdict(significant),
                    f(
                        stats::describe(&vals[0])
                            .map(|d| d.median)
                            .unwrap_or(f64::NAN)
                            - stats::describe(&vals[1])
                                .map(|d| d.median)
                                .unwrap_or(f64::NAN),
                        3
                    )
                ),
            ));
        }
    } else if all_normal {
        let a = stats::anova_oneway(&vals).ok_or_else(|| "各组样本不足。".to_string())?;
        significant = a.p < alpha;
        main_name = "单因素方差分析".into();
        rows.push(row(
            &main_name,
            format!(
                "F({:.0}, {:.0}) = {}，P = {}",
                a.df_between,
                a.df_within,
                f(a.f, 3),
                p_txt(a.p)
            ),
            format!(
                "η² = {}（{}效应）；{}",
                f(a.eta2, 3),
                eta_word(a.eta2),
                verdict(significant)
            ),
        ));
        if !var_equal {
            rows.push(row(
                "方差齐性提示",
                "方差不齐",
                "F 检验在方差不齐时偏松；事后比较请读 Games–Howell 列",
            ));
        }
    } else {
        let h = stats::kruskal_wallis(&vals).ok_or_else(|| "各组样本不足。".to_string())?;
        significant = h.p < alpha;
        main_name = "Kruskal–Wallis".into();
        rows.push(row(
            &main_name,
            format!(
                "H({:.0}) = {}，P = {}",
                h.df,
                f(h.h_corrected, 3),
                p_txt(h.p)
            ),
            format!(
                "ε² = {}（{}效应）；{}",
                f(h.epsilon2, 3),
                eta_word(h.epsilon2),
                verdict(significant)
            ),
        ));
    }

    // ---- 四、事后 ----
    if k > 2 {
        block(&mut rows, "四、事后比较");
        if !significant {
            rows.push(row(
                "事后比较",
                "整体检验不显著",
                "不做两两比较；单看某一对容易出假阳性",
            ));
        } else if all_normal {
            let tk = infer::tukey_hsd(&vals, 1.0 - alpha);
            let gh = infer::games_howell(&vals, 1.0 - alpha);
            if let Some(tk) = tk {
                for pair in tk {
                    let q = gh
                        .as_ref()
                        .and_then(|g| g.iter().find(|x| x.a == pair.a && x.b == pair.b));
                    rows.push(row(
                        format!("{} vs {}", groups[pair.a].0, groups[pair.b].0),
                        format!(
                            "差值 {}（{} – {}）",
                            f(pair.diff, 3),
                            f(pair.ci_lo, 3),
                            f(pair.ci_hi, 3)
                        ),
                        format!(
                            "Tukey P = {}；Games–Howell P = {}；{}",
                            p_txt(pair.p),
                            q.map(|x| p_txt(x.p)).unwrap_or_else(|| "—".into()),
                            if pair.p < alpha {
                                "显著"
                            } else {
                                "不显著"
                            }
                        ),
                    ));
                }
            }
            // Dunnett：各组 vs 第一组（对照）
            let dun = crate::core::posthoc::dunnett(&vals, 0);
            if let Some(dun_pairs) = dun {
                for dp in &dun_pairs {
                    rows.push(row(
                        format!("{} vs 对照", dp.group),
                        format!(
                            "差值 {}（{} – {}）",
                            f(dp.diff, 3),
                            f(dp.ci_low, 3),
                            f(dp.ci_high, 3)
                        ),
                        format!(
                            "Dunnett P = {}；{}",
                            p_txt(dp.p_value),
                            if dp.p_value < alpha {
                                "显著"
                            } else {
                                "不显著"
                            }
                        ),
                    ));
                }
            }
            // Holm-Bonferroni 校正（收集所有 Tukey 的 p 值统一校正）
            if let Some(tk) = infer::tukey_hsd(&vals, 1.0 - alpha) {
                let raw_p: Vec<f64> = tk.iter().map(|x| x.p).collect();
                let adj_p = crate::core::posthoc::holm_bonferroni(&raw_p);
                for (pair, adj) in tk.iter().zip(adj_p.iter()) {
                    rows.push(row(
                        format!("{} vs {}（Holm）", groups[pair.a].0, groups[pair.b].0),
                        format!("校正后 P = {}", p_txt(*adj)),
                        format!(
                            "Holm-Bonferroni 校正；{}",
                            if *adj < alpha { "显著" } else { "不显著" }
                        ),
                    ));
                }
            }
        } else {
            for pair in stats::posthoc_dunn(&vals) {
                rows.push(row(
                    format!("{} vs {}", groups[pair.a].0, groups[pair.b].0),
                    format!("z = {}，平均秩差 {}", f(pair.z, 3), f(pair.rank_diff, 2)),
                    format!(
                        "Holm P = {}；{}",
                        p_txt(pair.p_holm),
                        if pair.p_holm < alpha {
                            "显著"
                        } else {
                            "不显著"
                        }
                    ),
                ));
            }
        }
    }

    // ---- 五、结论 ----
    block(&mut rows, "五、结论与建议");
    let path = format!(
        "{} 组、共 {} 个观测；正态性{}，方差{}",
        k,
        n_total,
        if all_normal { "通过" } else { "未通过" },
        if var_equal { "齐" } else { "不齐" }
    );
    rows.push(row("路径", path, format!("因此主检验用「{main_name}」")));
    rows.push(row(
        "结论",
        if significant {
            "组间存在差异"
        } else {
            "未见组间差异"
        },
        if significant && k > 2 {
            "差异具体落在哪几组看第四块；报告时给出效应量与 95% CI，不只给 P"
        } else if significant {
            "报告均值差及其 95% CI 与效应量；P 值单独不足以说明大小"
        } else {
            "不能据此宣称「没有差异」——只说明当前样本不足以检出；需要时做等效性检验"
        },
    ));
    if out_n > 0 {
        rows.push(row(
            "离群值",
            format!("{out_n} 组检出候选"),
            "先核查记录/仪器；剔除要在方法里写明，并做敏感性分析",
        ));
    }
    rows.push(row(
        "进一步分析",
        "—",
        if k > 2 {
            "两两比较的完整表用「多重比较（Tukey / G–H）」；非参数事后用「Dunn」；对照实验用「Dunnett」"
        } else {
            "若为同一批对象的两次测量，请改用「配对 t 检验」"
        },
    ));
    // 变换建议（前提不满足时）
    if !all_normal || !var_equal {
        rows.push(row(
            "数据变换建议",
            "前提不满足",
            "尝试 ln(x) / √x / 1/x 变换后重跑分析；偏态数据可试 Box-Cox 自动最优 λ；\
             秩变换可保留单调关系但丢失原始单位",
        ));
    }

    Ok((format!("智能分析：{k} 组比较（n = {n_total}）"), rows))
}

// ---------------------------------------------------------------------------
// XY 表
// ---------------------------------------------------------------------------

fn xy_report(table: &DataTable, alpha: f64) -> Result<(String, Rows), String> {
    let mut rows: Rows = Vec::new();
    let n_sets = table.group_names.len();
    if n_sets == 0 {
        return Err("XY 表里没有数据集。".into());
    }
    block(&mut rows, "一、数据概览");
    let mut usable: Vec<(usize, String, Vec<(f64, f64)>)> = Vec::new();
    for g in 0..n_sets {
        let pts = table.xy_points(g);
        if pts.len() < 3 {
            continue;
        }
        let name = table.group_name(g);
        let (mut xlo, mut xhi, mut ylo, mut yhi) = (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        );
        for &(x, y) in &pts {
            xlo = xlo.min(x);
            xhi = xhi.max(x);
            ylo = ylo.min(y);
            yhi = yhi.max(y);
        }
        rows.push(row(
            format!("{name}（n = {}）", pts.len()),
            format!(
                "X {} – {}；Y {} – {}",
                f(xlo, 3),
                f(xhi, 3),
                f(ylo, 3),
                f(yhi, 3)
            ),
            "X 有重复时按行取 Y 均值（拟合用原始观测）",
        ));
        usable.push((g, name, pts));
    }
    if usable.is_empty() {
        return Err("每个数据集至少需要 3 个有效点。".into());
    }

    block(&mut rows, "二、相关");
    for (_, name, pts) in &usable {
        let xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
        let ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
        if let Some(r) = stats::pearson(&xs, &ys) {
            rows.push(row(
                format!("{name} · Pearson"),
                format!(
                    "r = {}（95% CI {} – {}），P = {}",
                    f(r.r, 3),
                    f(r.ci_low, 3),
                    f(r.ci_high, 3),
                    p_txt(r.p)
                ),
                if r.p < alpha {
                    "线性相关显著"
                } else {
                    "线性相关不显著"
                },
            ));
        }
        if let Some(s) = stats::spearman(&xs, &ys) {
            rows.push(row(
                format!("{name} · Spearman"),
                format!("r_s = {}，P = {}", f(s.r, 3), p_txt(s.p)),
                "度量单调关系，不受离群值量级影响",
            ));
        }
    }

    block(&mut rows, "三、线性回归");
    for (_, name, pts) in &usable {
        let xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
        let ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
        if let Some(r) = stats::linreg(&xs, &ys) {
            rows.push(row(
                format!("{name} · 方程"),
                format!("y = {}x + {}", f(r.slope, 4), f(r.intercept, 4)),
                format!(
                    "R² = {}；斜率 95% CI {} – {}；P = {}",
                    f(r.r2, 4),
                    f(r.ci_low, 4),
                    f(r.ci_high, 4),
                    p_txt(r.p)
                ),
            ));
        }
    }

    block(&mut rows, "四、模型比较（AICc）");
    for (_, name, pts) in &usable {
        let fits = crate::core::curve::fit_all(pts);
        if fits.is_empty() {
            continue;
        }
        let best = fits[0].aicc;
        for (rank, fit) in fits.iter().take(3).enumerate() {
            rows.push(row(
                format!("{name} · {}", fit.model.label()),
                format!("AICc = {}（Δ = {}）", f(fit.aicc, 1), f(fit.aicc - best, 1)),
                format!(
                    "{}；R² = {}，RMSE = {}{}",
                    fit.equation(),
                    f(fit.r2, 4),
                    f(fit.rmse, 4),
                    if rank == 0 { "；当前最优" } else { "" }
                ),
            ));
        }
        if fits.len() > 1 {
            let delta = fits[1].aicc - best;
            rows.push(row(
                format!("{name} · 选型"),
                if delta < 2.0 {
                    "最优与次优不可区分"
                } else {
                    "最优模型明显更好"
                },
                if delta < 2.0 {
                    "ΔAICc < 2：按专业含义选更简单的那个（奥卡姆剃刀）"
                } else {
                    "非线性模型更贴合时，报告方程与参数，而不是硬套直线"
                },
            ));
        }
    }

    block(&mut rows, "五、离群值");
    for (_, name, pts) in &usable {
        let ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
        if let Some(g) = infer::grubbs(&ys, alpha) {
            let hit = g.g > g.critical;
            rows.push(row(
                format!("{name} · Y 方向"),
                format!("G = {}（临界 {}）", f(g.g, 3), f(g.critical, 3)),
                if hit {
                    format!(
                        "候选点 Y = {}（P = {}），检查是否为录入/仪器异常",
                        f(g.value, 4),
                        p_txt(g.p)
                    )
                } else {
                    "未检出".to_string()
                },
            ));
        }
    }

    block(&mut rows, "六、结论与建议");
    let best_model = usable
        .iter()
        .filter_map(|(_, _, pts)| crate::core::curve::fit_all(pts).into_iter().next())
        .next();
    rows.push(row(
        "总体",
        format!("{} 个数据集", usable.len()),
        if let Some(m) = best_model {
            format!(
                "AICc 最优模型为「{}」；若各组机制不同，分组拟合比合并拟合更诚实",
                m.model.label()
            )
        } else {
            "模型均未收敛，先检查数据范围与重复点".to_string()
        },
    ));
    rows.push(row(
        "报告建议",
        "—",
        "斜率必须带 95% CI；R² 高不代表模型正确，配合残差图看；外推超出 X 范围要明确标注",
    ));
    Ok((format!("智能分析：XY（{} 个数据集）", usable.len()), rows))
}

// ---------------------------------------------------------------------------
// 分组表：双因素
// ---------------------------------------------------------------------------

fn grouped_report(table: &DataTable, alpha: f64) -> Result<(String, Rows), String> {
    let row_names: Vec<String> = if table.row_headers.iter().any(|h| !h.is_empty()) {
        table.row_headers.clone()
    } else {
        (0..table.nrows())
            .map(|i| format!("水平 {}", i + 1))
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
        return Err("分组表需要行、列各至少 2 个水平。".into());
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
    let mut rows: Rows = Vec::new();

    block(&mut rows, "一、设计概览");
    let total: usize = cells.iter().flatten().map(|c| c.len()).sum();
    rows.push(row(
        "设计",
        format!("{} × {}，共 {} 个观测", a, b, total),
        format!(
            "行因子 {} 个水平、列因子 {} 个水平；每格重复数 {}",
            a,
            b,
            cells.iter().flatten().map(|c| c.len()).min().unwrap_or(0)
        ),
    ));
    for (i, name) in row_names.iter().enumerate() {
        let ns: Vec<String> = cells[i].iter().map(|c| c.len().to_string()).collect();
        rows.push(row(
            format!("行 · {name}"),
            format!("n = {}", ns.join(" / ")),
            "每个列水平上的重复数（不一致时 ANOVA 用加权分解）",
        ));
    }

    block(&mut rows, "二、双因素方差分析");
    let inter_p: Option<f64> = match stats::two_way_anova(&cells) {
        Some(r) => {
            for (name, df, fv, p, eta) in [
                (
                    format!("行因子（{} 水平）", a),
                    r.df_a,
                    r.f_a,
                    r.p_a,
                    r.eta_a,
                ),
                (
                    format!("列因子（{} 水平）", b),
                    r.df_b,
                    r.f_b,
                    r.p_b,
                    r.eta_b,
                ),
                (
                    "交互（行 × 列）".to_string(),
                    r.df_ab,
                    r.f_ab,
                    r.p_ab,
                    r.eta_ab,
                ),
            ] {
                rows.push(row(
                    name,
                    format!(
                        "F({:.0}, {:.0}) = {}，P = {}",
                        df,
                        r.df_err,
                        f(fv, 3),
                        p_txt(p)
                    ),
                    format!(
                        "偏 η² = {}（{}效应）；{}",
                        f(eta, 3),
                        eta_word(eta),
                        verdict(p < alpha)
                    ),
                ));
            }
            rows.push(row(
                "残差",
                format!("df = {:.0}，SS = {}", r.df_err, f(r.ss_err, 4)),
                "误差项；完整 SS/MS 明细见「双因素方差分析」",
            ));
            Some(r.p_ab)
        }
        None => {
            // 每格只有 1 个观测：估不出交互与误差。此时标准做法是
            // 「无重复双因素（可加性）模型」——交互并入误差，不检验交互。
            let mut row_means = Vec::new();
            let mut col_means = Vec::new();
            for r in &cells {
                if r.iter().any(|c| c.is_empty()) {
                    return Err("每格必须都有观测。".into());
                }
                row_means.push(r.iter().map(|c| stats::mean(c)).sum::<f64>() / b as f64);
            }
            for j in 0..b {
                let all: Vec<f64> = cells.iter().flat_map(|r| r[j].iter().copied()).collect();
                col_means.push(stats::mean(&all));
            }
            let all: Vec<f64> = cells.iter().flatten().flatten().copied().collect();
            let grand = stats::mean(&all);
            let ss_total: f64 = all.iter().map(|v| (v - grand).powi(2)).sum();
            let ss_a: f64 = b as f64 * row_means.iter().map(|m| (m - grand).powi(2)).sum::<f64>();
            let ss_b: f64 = a as f64 * col_means.iter().map(|m| (m - grand).powi(2)).sum::<f64>();
            let ss_err = (ss_total - ss_a - ss_b).max(0.0);
            let (df_a, df_b) = ((a - 1) as f64, (b - 1) as f64);
            let df_err = df_a * df_b;
            if df_err <= 0.0 {
                return Err("无重复设计需要行列各 ≥2 个水平。".into());
            }
            let ms_err = ss_err / df_err;
            let mk = |name: String, ss: f64, df: f64| {
                let ms = ss / df;
                let fv = if ms_err > 0.0 {
                    ms / ms_err
                } else {
                    f64::INFINITY
                };
                let p = stats::f_sf(fv, df, df_err);
                row(
                    name,
                    format!(
                        "F({:.0}, {:.0}) = {}，P = {}",
                        df,
                        df_err,
                        f(fv, 3),
                        p_txt(p)
                    ),
                    format!(
                        "无重复设计：交互不可检验，已并入误差；{}",
                        verdict(p < alpha)
                    ),
                )
            };
            rows.push(mk(format!("行因子（{} 水平）", a), ss_a, df_a));
            rows.push(mk(format!("列因子（{} 水平）", b), ss_b, df_b));
            rows.push(row(
                "交互",
                "不可估计",
                "每格只有 1 个观测：需要重复才能分离交互与误差",
            ));
            rows.push(row(
                "残差",
                format!("df = {:.0}，SS = {}", df_err, f(ss_err, 4)),
                "此处残差 = 交互项（可加性假设）",
            ));
            None
        }
    };

    block(&mut rows, "三、边际均值");
    for j in 0..b {
        let col: Vec<f64> = cells
            .iter()
            .flat_map(|rowv| rowv[j].iter().copied())
            .collect();
        if let Some(d) = stats::describe(&col) {
            rows.push(row(
                format!("列水平 {}", j + 1),
                format!("{} ± {}（n = {}）", f(d.mean, 3), f(d.sem, 3), d.n),
                "跨行合并；交互显著时这些边际均值会掩盖格子差异",
            ));
        }
    }
    for i in 0..a {
        let col: Vec<f64> = cells[i].iter().flatten().copied().collect();
        if let Some(d) = stats::describe(&col) {
            rows.push(row(
                format!("行 · {}", row_names[i]),
                format!("{} ± {}（n = {}）", f(d.mean, 3), f(d.sem, 3), d.n),
                "跨列合并",
            ));
        }
    }

    block(&mut rows, "四、结论与建议");
    let inter = inter_p.map(|p| p < alpha);
    match inter {
        Some(true) => {
            rows.push(row(
                "交互",
                "显著",
                "两个因子的效应互相依赖 → 必须逐格（简单效应）解读，不能只看主效应",
            ));
            rows.push(row(
                "下一步",
                "—",
                "固定一个因子、在另一个因子的每个水平上做单因素 ANOVA + 事后比较",
            ));
        }
        Some(false) => {
            rows.push(row(
                "交互",
                "不显著",
                "两个因子独立起作用 → 可以分别报告主效应",
            ));
            rows.push(row(
                "下一步",
                "—",
                "对显著的主效应做边际均值的事后比较（Tukey）",
            ));
        }
        None => {
            rows.push(row(
                "交互",
                "不可检验",
                "无重复设计：只能假设可加性；补重复后应再检验交互",
            ));
            rows.push(row("下一步", "—", "在每格增加重复（≥2）后再做双因素 ANOVA"));
        }
    }
    Ok((format!("智能分析：分组表 {a}×{b}"), rows))
}

// ---------------------------------------------------------------------------
// 列联表
// ---------------------------------------------------------------------------

fn contingency_report(table: &DataTable, alpha: f64) -> Result<(String, Rows), String> {
    let (row_lab, col_lab, counts) = table
        .contingency_matrix()
        .ok_or_else(|| "列联表为空。".to_string())?;
    let mut rows: Rows = Vec::new();
    let total: f64 = counts.iter().flatten().sum();
    block(&mut rows, "一、数据概览");
    rows.push(row(
        "结构",
        format!("{} × {}，N = {:.0}", row_lab.len(), col_lab.len(), total),
        "列联表的行/列都是分类变量，表身是频数",
    ));
    for (i, name) in row_lab.iter().enumerate() {
        let s: f64 = counts[i].iter().sum();
        rows.push(row(
            format!("行 · {name}"),
            format!(
                "{s:.0}（{:.1}%）",
                if total > 0.0 { s / total * 100.0 } else { 0.0 }
            ),
            "行合计",
        ));
    }

    block(&mut rows, "二、独立性检验");
    let cs =
        crate::core::contingency::chi_square(&counts).map_err(|e| format!("卡方检验失败：{e}"))?;
    rows.push(row(
        "Pearson χ²",
        format!("χ²({:.0}) = {}，P = {}", cs.df, f(cs.chi2, 3), p_txt(cs.p)),
        format!(
            "Cramér's V = {}（关联强度）；最小期望频数 {}{}",
            f(cs.v, 3),
            f(cs.min_expected, 2),
            if cs.small_cells > 0 {
                format!("；{} 个格子期望 < 5，卡方近似偏乐观", cs.small_cells)
            } else {
                String::new()
            }
        ),
    ));
    if let Some(y) = cs.yates {
        rows.push(row(
            "Yates 校正 χ²",
            format!(
                "χ² = {}，P = {}",
                f(y, 3),
                p_txt(crate::core::infer::chi2_sf(y, 1.0))
            ),
            "仅 2×2；校正后更保守",
        ));
    }
    if let Ok(fs) = crate::core::contingency::fisher_exact(&counts) {
        rows.push(row(
            "Fisher 精确检验",
            format!("P = {}", p_txt(fs.p)),
            if fs.exact {
                "精确枚举，不依赖大样本近似"
            } else {
                "枚举超限，降级为 Monte Carlo（已标注）"
            },
        ));
    }

    if row_lab.len() == 2 && col_lab.len() == 2 {
        block(&mut rows, "三、2×2 专用指标");
        let (a, b, c, d) = (counts[0][0], counts[0][1], counts[1][0], counts[1][1]);
        if let Some((or, rr)) = infer::odds_ratio(a, b, c, d, 1.0 - alpha) {
            rows.push(row(
                "OR（比值比）",
                format!(
                    "{}（95% CI {} – {}）",
                    f(or.est, 3),
                    f(or.lo, 3),
                    f(or.hi, 3)
                ),
                "病例对照研究唯一可估的效应量",
            ));
            rows.push(row(
                "RR（相对风险）",
                format!(
                    "{}（95% CI {} – {}）",
                    f(rr.est, 3),
                    f(rr.lo, 3),
                    f(rr.hi, 3)
                ),
                "队列 / RCT 口径；OR 会高估 RR",
            ));
        }
        if let Some(k) = infer::cohen_kappa(a, b, c, d) {
            let word = if k.k < 0.2 {
                "轻微"
            } else if k.k < 0.4 {
                "一般"
            } else if k.k < 0.6 {
                "中等"
            } else if k.k < 0.8 {
                "好"
            } else {
                "很好"
            };
            rows.push(row(
                "Cohen's Kappa",
                format!("κ = {}（SE {}）", f(k.k, 3), f(k.se, 3)),
                format!("一致性{}；两位评分者同一批对象时才有意义", word),
            ));
        }
    }

    block(&mut rows, "四、哪个格子拉动了关联");
    let mut cells: Vec<(f64, usize, usize)> = Vec::new();
    for (i, r) in cs.residual.iter().enumerate() {
        for (j, v) in r.iter().enumerate() {
            if v.is_finite() && v.abs() >= cs.residual_crit {
                cells.push((*v, i, j));
            }
        }
    }
    cells.sort_by(|a, b| b.0.abs().partial_cmp(&a.0.abs()).unwrap());
    if cells.is_empty() {
        rows.push(row(
            "调整标准化残差",
            "无 |r| 超阈值的格子",
            format!(
                "阈值 {:.2}（Bonferroni 校正到 {} 格）",
                cs.residual_crit,
                row_lab.len() * col_lab.len()
            ),
        ));
    } else {
        for (v, i, j) in cells.iter().take(4) {
            rows.push(row(
                format!("{} × {}", row_lab[*i], col_lab[*j]),
                format!("残差 r = {}", f(*v, 2)),
                if *v > 0.0 {
                    "观测多于期望（↑）"
                } else {
                    "观测少于期望（↓）"
                },
            ));
        }
    }

    block(&mut rows, "五、结论与建议");
    rows.push(row(
        "结论",
        if cs.p < alpha {
            "行列不独立"
        } else {
            "未见关联"
        },
        if cs.p < alpha {
            "报告 χ²、df、P 与效应量（Cramér's V）；2×2 再给 OR/RR 及 CI"
        } else {
            "样本量不足或确实无关联；需要时做功效分析"
        },
    ));
    Ok((
        format!("智能分析：{}×{} 列联表", row_lab.len(), col_lab.len()),
        rows,
    ))
}

// ---------------------------------------------------------------------------
// 生存表
// ---------------------------------------------------------------------------

/// Kaplan–Meier 中位生存时间（首次 S(t) ≤ 0.5 的时刻）。
fn median_survival(data: &[(f64, bool)]) -> Option<f64> {
    let mut rows: Vec<(f64, bool)> = data.to_vec();
    rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let n_total = rows.len();
    if n_total == 0 {
        return None;
    }
    let mut at_risk = n_total as f64;
    let mut s = 1.0;
    let mut i = 0;
    while i < rows.len() {
        let t = rows[i].0;
        let mut d = 0.0;
        let mut c = 0.0;
        while i < rows.len() && (rows[i].0 - t).abs() < 1e-12 {
            if rows[i].1 {
                d += 1.0;
            } else {
                c += 1.0;
            }
            i += 1;
        }
        if at_risk > 0.0 && d > 0.0 {
            s *= 1.0 - d / at_risk;
            if s <= 0.5 {
                return Some(t);
            }
        }
        at_risk -= d + c;
    }
    None
}

fn survival_report(table: &DataTable, alpha: f64) -> Result<(String, Rows), String> {
    let gs = table.survival_groups();
    let gs: Vec<(String, Vec<(f64, bool)>)> =
        gs.into_iter().filter(|(_, v)| !v.is_empty()).collect();
    if gs.is_empty() {
        return Err("生存表里没有有效随访记录。".into());
    }
    let mut rows: Rows = Vec::new();
    block(&mut rows, "一、随访概览");
    for (name, data) in &gs {
        let ev = data.iter().filter(|(_, e)| *e).count();
        let cens = data.len() - ev;
        let med = median_survival(data);
        rows.push(row(
            format!("{name}（n = {}）", data.len()),
            format!("事件 {}，删失 {}", ev, cens),
            match med {
                Some(m) => format!(
                    "中位生存 {}；随访最长 {}",
                    f(m, 3),
                    f(data.iter().map(|d| d.0).fold(0.0, f64::max), 3)
                ),
                None => "未达到中位生存（生存率始终 > 50%）".to_string(),
            },
        ));
    }

    block(&mut rows, "二、组间比较");
    let datas: Vec<Vec<(f64, bool)>> = gs.iter().map(|(_, d)| d.clone()).collect();
    match stats::log_rank(&datas) {
        Some(lr) => {
            rows.push(row(
                "log-rank（Mantel–Cox）",
                format!("χ²({}) = {}，P = {}", lr.df, f(lr.chi2, 3), p_txt(lr.p)),
                format!("共 {} 个事件；{}", lr.n_events, verdict(lr.p < alpha)),
            ));
            if let (Some(hr), Some(lo), Some(hi)) = (lr.hr, lr.ci_low, lr.ci_high) {
                rows.push(row(
                    "风险比 HR",
                    format!("{}（95% CI {} – {}）", f(hr, 3), f(lo, 3), f(hi, 3)),
                    "第 1 组相对第 2 组；HR > 1 表示风险更高",
                ));
            }
        }
        None => {
            rows.push(row("log-rank", "无法计算", "需要至少 2 组、且事件数 > 0"));
        }
    }

    if gs.len() >= 2 {
        block(&mut rows, "三、Cox 比例风险");
        let mut t = Vec::new();
        let mut e = Vec::new();
        let mut g = Vec::new();
        for (gi, (_, data)) in gs.iter().enumerate() {
            for (tt, ev) in data {
                t.push(*tt);
                e.push(if *ev { 1.0 } else { 0.0 });
                g.push(gi as f64);
            }
        }
        let mut cols: Vec<(String, Vec<f64>)> = Vec::new();
        for gi in 1..gs.len() {
            cols.push((
                format!("{} vs {}", gs[gi].0, gs[0].0),
                g.iter()
                    .map(|x| if *x == gi as f64 { 1.0 } else { 0.0 })
                    .collect(),
            ));
        }
        match infer::cox_ph(&t, &e, &cols) {
            Some(r) => {
                for (i, v) in r.vars.iter().enumerate() {
                    rows.push(row(
                        &cols[i].0,
                        format!(
                            "HR = {}（{} – {}）",
                            f(v.hr, 3),
                            f(v.ci_lo, 3),
                            f(v.ci_hi, 3)
                        ),
                        format!(
                            "β = {}，z = {}，P = {}",
                            f(v.beta, 3),
                            f(v.z, 2),
                            p_txt(v.p)
                        ),
                    ));
                }
                if r.separated {
                    rows.push(row(
                        "注意",
                        "数据完全分离",
                        "某组事件全在另一组之前 → β 发散、HR 无界；不要报单一 HR",
                    ));
                }
            }
            None => {
                rows.push(row("Cox", "无法拟合", "事件数为 0 或样本量不足"));
            }
        }
    }

    block(&mut rows, "四、结论与建议");
    rows.push(row(
        "结论",
        "—",
        "生存分析必须同时报随访时长、删失比例与中位生存；只看 P 值会掩盖曲线交叉",
    ));
    rows.push(row(
        "假设检查",
        "比例风险",
        "各组生存曲线明显交叉时 HR 没有单一含义；此时更适合报告某时点的生存率差",
    ));
    Ok((format!("智能分析：生存（{} 组）", gs.len()), rows))
}

// ---------------------------------------------------------------------------
// 多变量表
// ---------------------------------------------------------------------------

fn multi_report(table: &DataTable, alpha: f64) -> Result<(String, Rows), String> {
    let cols: Vec<(String, Vec<f64>)> = table.analysis_groups();
    let cols: Vec<(String, Vec<f64>)> = cols.into_iter().filter(|(_, v)| v.len() >= 3).collect();
    if cols.len() < 2 {
        return Err("多变量表至少需要 2 个数值变量、每列 ≥3 行。".into());
    }
    let mut rows: Rows = Vec::new();

    block(&mut rows, "一、数据概览");
    rows.push(row(
        "变量数",
        format!("{} 个", cols.len()),
        "行 = 同一个体，列 = 变量；成对分析会自动取完整行",
    ));
    for (name, v) in &cols {
        if let Some(d) = stats::describe(v) {
            rows.push(row(
                name,
                format!("{} ± {}", f(d.mean, 3), f(d.sd, 3)),
                format!(
                    "中位数 {}（{} – {}）；n = {}",
                    f(d.median, 3),
                    f(d.q1, 3),
                    f(d.q3, 3),
                    d.n
                ),
            ));
        }
    }

    block(&mut rows, "二、变量间关联");
    let vals: Vec<Vec<f64>> = cols.iter().map(|(_, v)| v.clone()).collect();
    if let Some((r, p, n)) = infer::corr_matrix(&vals) {
        let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
        for i in 0..cols.len() {
            for j in (i + 1)..cols.len() {
                if r[i][j].is_finite() {
                    pairs.push((r[i][j], i, j));
                }
            }
        }
        pairs.sort_by(|a, b| b.0.abs().partial_cmp(&a.0.abs()).unwrap());
        for (rr, i, j) in pairs.iter().take(5) {
            rows.push(row(
                format!("{} × {}", cols[*i].0, cols[*j].0),
                format!("r = {}，P = {}", f(*rr, 3), p_txt(p[*i][*j])),
                if p[*i][*j] < alpha {
                    "显著"
                } else {
                    "不显著"
                },
            ));
        }
        rows.push(row(
            "说明",
            format!("n = {n}"),
            "只列 |r| 最大的 5 对；完整矩阵见「相关矩阵」",
        ));
    }

    block(&mut rows, "三、主成分");
    if let Some(pc) = infer::pca(&vals, true) {
        for i in 0..pc.eigen.len().min(3) {
            rows.push(row(
                format!("PC{}", i + 1),
                format!(
                    "特征值 {}，解释 {:.1}%",
                    f(pc.eigen[i], 3),
                    pc.proportion[i] * 100.0
                ),
                format!("累积 {:.1}%", pc.cumulative[i] * 100.0),
            ));
        }
        let k = pc
            .cumulative
            .iter()
            .position(|c| *c >= 0.8)
            .map(|i| i + 1)
            .unwrap_or(pc.eigen.len());
        rows.push(row(
            "建议成分数",
            format!("{k} 个"),
            "累积解释 ≥80% 的最小成分数；碎石图拐点也可参考",
        ));
    }

    block(&mut rows, "四、多元回归（第一列为因变量）");
    let y = &cols[0].1;
    let xs: Vec<(String, Vec<f64>)> = cols[1..].iter().cloned().collect();
    match infer::ols(y, &xs) {
        Some(r) => {
            for i in 1..r.coef.len() {
                rows.push(row(
                    format!("{} 的系数", r.names[i]),
                    format!("{}（SE {}）", f(r.coef[i], 4), f(r.se[i], 4)),
                    format!(
                        "t = {}，P = {}；{}",
                        f(r.t[i], 3),
                        p_txt(r.p[i]),
                        if r.p[i] < alpha {
                            "显著"
                        } else {
                            "不显著"
                        }
                    ),
                ));
            }
            rows.push(row(
                "模型",
                format!("R² = {}，调整 R² = {}", f(r.r2, 4), f(r.adj_r2, 4)),
                format!(
                    "F({}, {:.0}) = {}，P = {}；n = {}",
                    xs.len(),
                    r.df_resid,
                    f(r.f, 3),
                    p_txt(r.f_p),
                    r.n
                ),
            ));
        }
        None => {
            rows.push(row("回归", "无法拟合", "样本量不足或自变量完全共线"));
        }
    }

    block(&mut rows, "五、结论与建议");
    rows.push(row(
        "降维",
        "—",
        "变量多且相关强时，先看 PCA 再决定进回归的变量，能减轻共线",
    ));
    rows.push(row(
        "回归",
        "—",
        "R² 高不等于因果；自变量高度相关时系数不稳定，看相关矩阵或 VIF",
    ));
    Ok((format!("智能分析：多变量（{} 列）", cols.len()), rows))
}

// ---------------------------------------------------------------------------
// 嵌套表 / 构成表
// ---------------------------------------------------------------------------

fn nested_report(table: &DataTable, alpha: f64) -> Result<(String, Rows), String> {
    let raw = table.nested_groups();
    let groups: Vec<(String, Vec<Vec<f64>>)> = raw
        .into_iter()
        .filter(|(_, subs)| subs.iter().filter(|s| !s.is_empty()).count() >= 1)
        .collect();
    if groups.len() < 2 {
        return Err("嵌套表需要至少 2 个外层组。".into());
    }
    let mut rows: Rows = Vec::new();
    block(&mut rows, "一、设计概览");
    for (name, subs) in &groups {
        let bins: Vec<usize> = subs.iter().map(|s| s.len()).collect();
        let n: usize = bins.iter().sum();
        rows.push(row(
            name,
            format!("{} 个子组，{} 个观测", subs.len(), n),
            format!(
                "每子组 {}",
                bins.iter()
                    .map(|b| b.to_string())
                    .collect::<Vec<_>>()
                    .join(" / ")
            ),
        ));
    }
    block(&mut rows, "二、嵌套 ANOVA");
    match crate::core::nested::nested_anova(&groups) {
        Some(r) => {
            rows.push(row(
                format!("外层组（{}）", r.a),
                format!(
                    "F({:.0}, {:.0}) = {}",
                    r.df_group,
                    r.df_sub,
                    f(r.ms_group / r.ms_sub, 3)
                ),
                "以「子组间均方」作误差项：子组才是实验单位",
            ));
            rows.push(row(
                "子组（组内）",
                format!("{} 个子组", r.b),
                "子组间差异通常不解读——它只是误差来源",
            ));
            rows.push(row(
                "观测",
                format!("n = {}", r.n),
                "个体观测是技术重复，不单独当独立样本",
            ));
        }
        None => {
            rows.push(row(
                "嵌套 ANOVA",
                "无法计算",
                "需要每组至少 1 个子组、总共 ≥2 个子组",
            ));
        }
    }
    block(&mut rows, "三、结论与建议");
    rows.push(row(
        "为什么不能用普通 ANOVA",
        "伪重复",
        "把子组内的个体当独立观测会高估显著性；嵌套分析的误差项按子组算",
    ));
    rows.push(row(
        "下一步",
        "—",
        "需要两两比较时用「嵌套 t 检验」（仅 2 组）",
    ));
    let _ = alpha;
    Ok((format!("智能分析：嵌套（{} 组）", groups.len()), rows))
}

fn parts_report(table: &DataTable, alpha: f64) -> Result<(String, Rows), String> {
    let groups = table.analysis_groups();
    let vals: Vec<f64> = groups.iter().flat_map(|(_, v)| v.iter().copied()).collect();
    if vals.len() < 2 {
        return Err("构成表至少需要 2 个类别。".into());
    }
    let total: f64 = vals.iter().sum();
    if !(total > 0.0) {
        return Err("构成表的合计必须大于 0。".into());
    }
    let mut rows: Rows = Vec::new();
    block(&mut rows, "一、构成");
    for (name, v) in &groups {
        let s: f64 = v.iter().sum();
        rows.push(row(
            name,
            format!("{s:.0}（{:.1}%）", s / total * 100.0),
            format!("占全体的 {:.1}%", s / total * 100.0),
        ));
    }
    block(&mut rows, "二、分布均匀性（卡方拟合优度）");
    let k = vals.len() as f64;
    let e = total / k;
    let chi2: f64 = vals.iter().map(|o| (o - e).powi(2) / e).sum();
    let df = k - 1.0;
    rows.push(row(
        "χ² 拟合优度",
        format!(
            "χ²({:.0}) = {}，P = {}",
            df,
            f(chi2, 3),
            p_txt(infer::chi2_sf(chi2, df))
        ),
        format!(
            "期望每类 {:.1}；{}",
            e,
            verdict(infer::chi2_sf(chi2, df) < alpha)
        ),
    ));
    block(&mut rows, "三、结论与建议");
    rows.push(row(
        "用途",
        "—",
        "构成表看的是「份额」，不要用它比较绝对值；份额随总数变化会失真",
    ));
    Ok((format!("智能分析：构成（{} 类）", vals.len()), rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{Cell, Column};
    use crate::core::sample::sample_project;

    fn col_table(data: &[(&str, Vec<f64>)]) -> DataTable {
        let mut t = DataTable::template(TableKind::Column, String::from("c"), 2);
        t.columns.clear();
        for (j, (n, vals)) in data.iter().enumerate() {
            let mut col = Column::new(*n, ColRole::Y);
            col.group = j;
            col.cells = vals.iter().map(|v| Cell::number(*v)).collect();
            t.columns.push(col);
        }
        t
    }

    fn flat(rep: &ResultTable) -> String {
        rep.rows
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>()
            .join(" | ")
    }

    /// 每种表型都要有一份能读的报告，且统一为「项目 / 结果 / 说明」三列。
    #[test]
    fn 每种表型都有智能报告() {
        let p = sample_project();
        for t in &p.tables {
            let r = report(t, 0.05).unwrap_or_else(|e| panic!("{} 失败：{e}", t.name));
            assert_eq!(
                r.columns,
                vec!["项目".to_string(), "结果".to_string(), "说明".to_string()],
                "{} 的列结构不对",
                t.name
            );
            assert!(r.rows.len() >= 6, "{} 的报告太短", t.name);
            // 至少两个分块标题（单格行）
            let caps = r
                .rows
                .iter()
                .filter(|row| row.len() == 1 || row.iter().filter(|c| !c.is_empty()).count() == 1)
                .count();
            assert!(caps >= 2, "{} 缺少分块标题", t.name);
        }
    }

    #[test]
    fn 多变量表也能出报告() {
        let mut t = DataTable::template(TableKind::MultipleVariables, String::from("mv"), 2);
        t.columns.clear();
        let xs: Vec<f64> = (1..=10).map(|i| i as f64).collect();
        let ys: Vec<f64> = xs.iter().map(|v| 2.0 * v + 1.0).collect();
        let zs: Vec<f64> = xs.iter().map(|v| v * v).collect();
        for (i, (name, vals)) in [("y", &ys), ("x1", &xs), ("x2", &zs)].iter().enumerate() {
            let mut c = Column::new(*name, ColRole::Y);
            c.group = i;
            c.cells = vals.iter().map(|v| Cell::number(*v)).collect();
            t.columns.push(c);
        }
        let r = report(&t, 0.05).expect("multi report");
        let s = flat(&r);
        assert!(s.contains("主成分"), "{s}");
        assert!(s.contains("多元回归"), "{s}");
    }

    /// 决策链：正态 + 方差齐 → 参数检验；偏态 → 非参数。
    #[test]
    fn 决策链按前提选择检验() {
        let normal = col_table(&[
            ("A", vec![10.0, 11.0, 9.0, 10.5, 9.5, 10.2, 9.8, 10.1]),
            ("B", vec![12.0, 13.0, 11.0, 12.5, 11.5, 12.2, 11.8, 12.1]),
        ]);
        let r = report(&normal, 0.05).expect("normal");
        let s = flat(&r);
        assert!(s.contains("t 检验"), "正态两组应用 t 检验：{s}");
        assert!(!s.contains("Mann–Whitney"), "不应落到非参数：{s}");

        // 强偏态（指数分布形状）：D'Agostino 会拒绝正态
        let skew = col_table(&[
            ("A", vec![0.1, 0.2, 0.3, 0.5, 1.0, 3.0, 8.0, 30.0]),
            ("B", vec![0.2, 0.3, 0.4, 0.6, 1.2, 4.0, 9.0, 40.0]),
        ]);
        let r = report(&skew, 0.05).expect("skew");
        let s = flat(&r);
        assert!(s.contains("Mann–Whitney"), "偏态两组应用 Mann–Whitney：{s}");

        let three = col_table(&[
            ("A", vec![10.0, 11.0, 9.0, 10.5, 9.5, 10.2, 9.8, 10.1]),
            ("B", vec![12.0, 13.0, 11.0, 12.5, 11.5, 12.2, 11.8, 12.1]),
            ("C", vec![14.0, 15.0, 13.0, 14.5, 13.5, 14.2, 13.8, 14.1]),
        ]);
        let r = report(&three, 0.05).expect("three");
        let s = flat(&r);
        assert!(s.contains("单因素方差分析"), "{s}");
        assert!(s.contains("Tukey"), "多组显著应给事后比较：{s}");
        assert!(s.contains("η²"), "必须给效应量：{s}");
    }

    #[test]
    fn xy报告覆盖相关回归与模型比较() {
        let p = sample_project();
        let t = p.tables.iter().find(|t| t.name == "剂量–反应").unwrap();
        let r = report(t, 0.05).expect("xy");
        let s = flat(&r);
        for key in ["Pearson", "Spearman", "方程", "AICc", "离群值"] {
            assert!(s.contains(key), "缺少 {key}：{s}");
        }
    }

    #[test]
    fn 列联与生存报告给出效应量() {
        let p = sample_project();
        let ct = p
            .tables
            .iter()
            .find(|t| t.kind == TableKind::Contingency)
            .unwrap();
        let r = report(ct, 0.05).expect("contingency");
        let s = flat(&r);
        assert!(s.contains("χ²"), "{s}");
        assert!(s.contains("Cramér"), "{s}");

        let sv = p
            .tables
            .iter()
            .find(|t| t.kind == TableKind::Survival)
            .unwrap();
        let r = report(sv, 0.05).expect("survival");
        let s = flat(&r);
        assert!(s.contains("log-rank"), "{s}");
        assert!(s.contains("HR"), "{s}");
    }
}
