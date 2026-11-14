//! 嵌套 / 层级重复的统计检验。
//!
//! 嵌套设计的特殊之处：**子组（班级）才是实验单位，个体观测是技术重复**。
//! 因此检验外层组（教学方法）效应时，误差项必须用「组内子组间的均方」，
//! 而不是「子组内的均方」。若照普通 t 检验 / ANOVA 把每个个体当作独立观测，
//! 会把技术重复当成生物学重复，**夸大显著性**（伪重复）。
//!
//! 口径参照 GraphPad Prism 的 Nested t test / Nested one-way ANOVA，
//! 以及 Sokal & Rohlf《Biometry》的 Model II（完全随机嵌套）方差分析。

use crate::core::stats::{f_sf, mean, t_p, t_quantile};

/// 嵌套单因素 ANOVA 的结果。
///
/// 方差分解为三层：外层组间 → 组内子组间 → 子组内。
#[derive(Clone, Debug)]
pub struct NestedAnova {
    /// 外层组数
    pub a: usize,
    /// 子组总数
    pub b: usize,
    /// 观测总数
    pub n: usize,
    pub ss_group: f64,
    pub ss_sub: f64,
    pub ss_within: f64,
    pub df_group: f64,
    pub df_sub: f64,
    pub df_within: f64,
    pub ms_group: f64,
    pub ms_sub: f64,
    pub ms_within: f64,
    /// 外层组效应的 F = MS_group / MS_sub
    pub f: f64,
    pub p: f64,
    /// 效应量 η² = SS_group / SS_total
    pub eta2: f64,
    /// 方差分量（σ²），负估计被截断为 0
    pub var_group: f64,
    pub var_sub: f64,
    pub var_within: f64,
}

/// 嵌套 t 检验的结果（两个外层组）。
#[derive(Clone, Debug)]
pub struct NestedT {
    pub t: f64,
    pub df: f64,
    pub p: f64,
    /// 两组均值之差
    pub diff: f64,
    pub se: f64,
    pub ci_low: f64,
    pub ci_high: f64,
    /// 各组的观测数与子组数
    pub n1: usize,
    pub n2: usize,
    pub b1: usize,
    pub b2: usize,
    /// 用作误差项的「组内子组间均方」
    pub ms_sub: f64,
}

/// 一层嵌套数据：`(外层组名, [每个内层子组的重复值])`。
pub type NestedGroups = Vec<(String, Vec<Vec<f64>>)>;

/// 校验并清理嵌套数据：去掉空子组与非有限值。
/// 返回每个外层组的**非空**子组，保证后续 df 计算不会被空组污染。
fn clean(groups: &NestedGroups) -> Vec<Vec<Vec<f64>>> {
    groups
        .iter()
        .map(|(_, subs)| {
            subs.iter()
                .map(|v| {
                    v.iter()
                        .copied()
                        .filter(|x| x.is_finite())
                        .collect::<Vec<f64>>()
                })
                .filter(|v| !v.is_empty())
                .collect()
        })
        .filter(|subs: &Vec<Vec<f64>>| !subs.is_empty())
        .collect()
}

/// 嵌套单因素 ANOVA。
///
/// `groups[i]` = 第 i 个外层组，其元素是该组内各子组的重复值向量。
/// 至少需要 2 个外层组，且总子组数 > 外层组数（否则子组间均方无自由度）。
pub fn nested_anova(groups: &NestedGroups) -> Option<NestedAnova> {
    let g = clean(groups);
    let a = g.len();
    if a < 2 {
        return None;
    }
    let b: usize = g.iter().map(|s| s.len()).sum();
    if b <= a {
        return None;
    }
    let n: usize = g.iter().flatten().map(|v| v.len()).sum();
    if n <= b {
        return None;
    }

    // 三个层级的均值
    let grand = g.iter().flatten().flat_map(|v| v.iter()).sum::<f64>() / n as f64;

    let mut ss_group = 0.0;
    let mut ss_sub = 0.0;
    let mut ss_within = 0.0;

    for subs in &g {
        let ni: usize = subs.iter().map(|v| v.len()).sum();
        if ni == 0 {
            continue;
        }
        let gi_mean = subs.iter().flatten().copied().sum::<f64>() / ni as f64;
        ss_group += ni as f64 * (gi_mean - grand).powi(2);
        for v in subs {
            let mij = mean(v);
            ss_sub += v.len() as f64 * (mij - gi_mean).powi(2);
            ss_within += v.iter().map(|x| (x - mij).powi(2)).sum::<f64>();
        }
    }

    let df_group = (a - 1) as f64;
    let df_sub = (b - a) as f64;
    let df_within = (n - b) as f64;
    let ms_group = ss_group / df_group;
    let ms_sub = ss_sub / df_sub;
    let ms_within = if df_within > 0.0 {
        ss_within / df_within
    } else {
        0.0
    };

    // 检验外层组效应时，误差项是**组内子组间的均方**。
    let f = if ms_sub > 0.0 {
        ms_group / ms_sub
    } else {
        f64::INFINITY
    };
    let p = if ms_sub > 0.0 {
        f_sf(f, df_group, df_sub)
    } else {
        0.0
    };
    let ss_total = ss_group + ss_sub + ss_within;
    let eta2 = if ss_total > 0.0 {
        ss_group / ss_total
    } else {
        0.0
    };

    // 方差分量（不平衡设计用近似公式）
    let n0 = harmonic_subgroup_size(&g);
    let var_within = ms_within;
    let var_sub = ((ms_sub - ms_within) / n0).max(0.0);
    let var_group = ((ms_group - ms_sub) / n0).max(0.0);

    Some(NestedAnova {
        a,
        b,
        n,
        ss_group,
        ss_sub,
        ss_within,
        df_group,
        df_sub,
        df_within,
        ms_group,
        ms_sub,
        ms_within,
        f,
        p,
        eta2,
        var_group,
        var_sub,
        var_within,
    })
}

/// 子组大小的调和平均（用于不平衡设计的方差分量估计）。
fn harmonic_subgroup_size(g: &[Vec<Vec<f64>>]) -> f64 {
    let mut count = 0usize;
    let mut sum_recip = 0.0;
    for subs in g {
        for v in subs {
            if !v.is_empty() {
                count += 1;
                sum_recip += 1.0 / v.len() as f64;
            }
        }
    }
    if count == 0 || sum_recip <= 0.0 {
        1.0
    } else {
        count as f64 / sum_recip
    }
}

/// 嵌套 t 检验：比较两个外层组的均值。
///
/// 误差项同样是「组内子组间的均方」，自由度 = 子组总数 − 2。
/// 这样才算把子组当作实验单位，避免伪重复。
pub fn nested_t(groups: &NestedGroups) -> Option<NestedT> {
    let g = clean(groups);
    if g.len() != 2 {
        return None;
    }
    let (s1, s2) = (&g[0], &g[1]);
    if s1.len() < 1 || s2.len() < 1 {
        return None;
    }
    // 子组是实验单位：两组子组数之和 − 2
    let b1 = s1.len();
    let b2 = s2.len();
    let df = (b1 + b2 - 2) as f64;
    if df <= 0.0 {
        return None;
    }
    let n1: usize = s1.iter().map(|v| v.len()).sum();
    let n2: usize = s2.iter().map(|v| v.len()).sum();
    if n1 == 0 || n2 == 0 {
        return None;
    }

    // 外层组均值（按子组内观测数加权）
    let m1 = s1.iter().flatten().copied().sum::<f64>() / n1 as f64;
    let m2 = s2.iter().flatten().copied().sum::<f64>() / n2 as f64;

    // 组内子组间均方：合并两组的 SS_sub
    let mut ss_sub = 0.0;
    for (subs, gi_mean) in [(s1, m1), (s2, m2)] {
        for v in subs {
            let mij = mean(v);
            ss_sub += v.len() as f64 * (mij - gi_mean).powi(2);
        }
    }
    let ms_sub = ss_sub / df;
    if !(ms_sub > 0.0) {
        return None;
    }

    // 子组是实验单位：标准误按子组数计算
    let se = (ms_sub * (1.0 / b1 as f64 + 1.0 / b2 as f64)).sqrt();
    if !(se > 0.0) {
        return None;
    }
    let diff = m1 - m2;
    let t = diff / se;
    let p = t_p(t, df);
    let tc = t_quantile(1.0 - 0.025, df);

    Some(NestedT {
        t,
        df,
        p,
        diff,
        se,
        ci_low: diff - tc * se,
        ci_high: diff + tc * se,
        n1,
        n2,
        b1,
        b2,
        ms_sub,
    })
}

/// 嵌套 t 检验的效应量（Cohen's d，但用「组内子组间」标准差做分母）。
pub fn nested_cohen_d(r: &NestedT) -> f64 {
    if r.ms_sub > 0.0 {
        r.diff / r.ms_sub.sqrt()
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 两方法 × 每方法 4 班 × 每班 4 人。
    /// 子组数给足，检验才有自由度（df_sub = 8−2 = 6）。
    fn demo() -> NestedGroups {
        vec![
            (
                "A".into(),
                vec![
                    vec![10.0, 11.0, 12.0, 11.5],
                    vec![12.0, 13.0, 12.5, 13.5],
                    vec![11.0, 12.0, 11.5, 12.5],
                    vec![13.0, 14.0, 13.5, 14.5],
                ],
            ),
            (
                "B".into(),
                vec![
                    vec![20.0, 21.0, 20.5, 21.5],
                    vec![22.0, 23.0, 22.5, 23.5],
                    vec![21.0, 22.0, 21.5, 22.5],
                    vec![23.0, 24.0, 23.5, 24.5],
                ],
            ),
        ]
    }

    #[test]
    fn 嵌套ANOVA的自由度分解正确() {
        let r = nested_anova(&demo()).expect("anova");
        assert_eq!(r.a, 2);
        assert_eq!(r.b, 8);
        assert_eq!(r.n, 32);
        assert_eq!(r.df_group, 1.0);
        assert_eq!(r.df_sub, 6.0);
        assert_eq!(r.df_within, 24.0);
        // 三层 SS 相加应等于总离差平方和
        let d = demo();
        let total: f64 = d
            .iter()
            .flat_map(|(_, subs)| subs.iter().flat_map(|v| v.iter()))
            .copied()
            .sum::<f64>()
            / 32.0;
        let mut sst = 0.0;
        for (_, subs) in &d {
            for v in subs {
                for x in v {
                    sst += (x - total).powi(2);
                }
            }
        }
        assert!((r.ss_group + r.ss_sub + r.ss_within - sst).abs() < 1e-9);
    }

    #[test]
    fn 嵌套ANOVA对组间差异敏感() {
        let r = nested_anova(&demo()).expect("anova");
        // A 组均值约 12，B 组约 22：组间远大于子组间
        assert!(r.f > 10.0, "f = {}", r.f);
        assert!(r.p < 0.01, "p = {}", r.p);
        assert!(r.eta2 > 0.5);
    }

    #[test]
    fn 嵌套ANOVA拒绝空子组污染() {
        let mut d = demo();
        d[0].1.push(Vec::new()); // 空子组
        let r = nested_anova(&d).expect("anova");
        assert_eq!(r.b, 8, "空子组不应计入");
    }

    #[test]
    fn 子组数不足时返回None() {
        // 每组只有 1 个子组 → b = 2 = a，子组间均方无自由度
        let d = vec![
            ("A".into(), vec![vec![1.0, 2.0]]),
            ("B".into(), vec![vec![3.0, 4.0]]),
        ];
        assert!(nested_anova(&d).is_none());
    }

    #[test]
    fn 嵌套t检验用子组当实验单位() {
        let r = nested_t(&demo()).expect("t");
        assert_eq!(r.b1, 4);
        assert_eq!(r.b2, 4);
        assert_eq!(r.df, 6.0, "自由度 = 子组总数 − 2");
        // 若按个体当实验单位，df 会是 30、t 会大得多；这里 df=6 才是嵌套的正确口径。
        assert!(r.t.abs() > 3.0, "t = {}", r.t);
        assert!(r.p < 0.05, "p = {}", r.p);
        assert!(r.diff.abs() > 5.0);
        assert!(r.ci_low * r.ci_high > 0.0, "CI 不应跨 0");
    }

    #[test]
    fn 嵌套t检验要求恰好两个外层组() {
        let d = vec![
            ("A".into(), vec![vec![1.0, 2.0]]),
            ("B".into(), vec![vec![3.0, 4.0]]),
            ("C".into(), vec![vec![5.0, 6.0]]),
        ];
        assert!(nested_t(&d).is_none());
    }

    #[test]
    fn 嵌套t检验对伪重复稳健() {
        // 每组 3 个子组、每子组 6 个体；子组均值几乎相同（10.0 / 10.2 / 10.1），
        // 但组内个体散布很大。若把 18 个体当独立观测，普通 t 检验会「显著」；
        // 嵌套 t 检验以子组为实验单位，组间差异相对于子组间波动应当不显著。
        let d = vec![
            (
                "A".into(),
                vec![
                    vec![1.0, 20.0, 2.0, 19.0, 3.0, 15.0],
                    vec![4.0, 17.0, 5.0, 16.0, 6.0, 12.0],
                    vec![7.0, 14.0, 8.0, 13.0, 9.0, 10.0],
                ],
            ),
            (
                "B".into(),
                vec![
                    vec![2.0, 21.0, 3.0, 18.0, 4.0, 16.0],
                    vec![5.0, 18.0, 6.0, 15.0, 7.0, 11.0],
                    vec![8.0, 13.0, 9.0, 12.0, 10.0, 11.0],
                ],
            ),
        ];
        let r = nested_t(&d).expect("t");
        assert_eq!(r.df, 4.0);
        // 子组均值只差 0.1，ms_sub 也不大，但 t 应当远小于 2
        assert!(
            r.p > 0.05,
            "伪重复下不应显著：t = {:.3}, p = {:.4}",
            r.t,
            r.p
        );
    }
}
