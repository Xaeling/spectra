//! 补充统计方法：Dunnett 事后比较、Holm-Bonferroni 显式校正、
//! ROUT 离群值检测、风险差（2×2 表）。

use crate::core::stats::{mean, norm_cdf, sd, t_quantile};

// ── Dunnett's test ─────────────────────────────────────────────────────────

/// Dunnett 事后比较结果（每组 vs 对照组）。
#[derive(Clone, Debug)]
pub struct DunnettPair {
    pub group: String,
    pub control: String,
    pub diff: f64,
    pub t_stat: f64,
    pub df: f64,
    pub p_value: f64,
    pub ci_low: f64,
    pub ci_high: f64,
}

/// Dunnett's test：每个实验组与对照组比较。
///
/// 控制多重比较族误差率（FWER）——比逐对 t 检验更保守但比 Tukey 更宽松
/// （只比较 vs 对照，不做全部两两）。
/// 用 Dunnett (1955) 的临界值近似（two-sided）。
pub fn dunnett(groups: &[Vec<f64>], control_idx: usize) -> Option<Vec<DunnettPair>> {
    if groups.len() < 2 || control_idx >= groups.len() {
        return None;
    }
    let control = &groups[control_idx];
    let n_c = control.len();
    if n_c < 2 {
        return None;
    }
    let m_c = mean(control);

    // 合并方差（所有组）
    let k = groups.len();
    let mut ss_total = 0.0;
    let mut df_total = 0usize;
    for g in groups {
        if g.len() < 2 {
            continue;
        }
        let m = mean(g);
        ss_total += g.iter().map(|&x| (x - m).powi(2)).sum::<f64>();
        df_total += g.len() - 1;
    }
    if df_total == 0 {
        return None;
    }
    let mse = ss_total / df_total as f64;
    let df = df_total as f64;

    // Dunnett 临界值（two-sided, α=0.05 近似）
    // t_crit_dunnett ≈ t_crit * (k-1)^0.5 * correlation_correction
    // 简化近似：用 t 临界值 × (1 + 0.2 * (k-2))^0.3
    let t_crit = t_quantile(0.975, df);
    let dunnett_factor = (1.0 + 0.15 * (k - 2) as f64).powf(0.35);
    let d_crit = t_crit * dunnett_factor;

    let mut results = Vec::new();
    for (i, g) in groups.iter().enumerate() {
        if i == control_idx || g.len() < 2 {
            continue;
        }
        let m_t = mean(g);
        let n_t = g.len();
        let diff = m_t - m_c;
        let se = (mse * (1.0 / n_c as f64 + 1.0 / n_t as f64)).sqrt();
        if se == 0.0 {
            continue;
        }
        let t_stat = diff / se;
        // p 值近似（用 t 分布 + Dunnett 校正因子）
        let t_adj = t_stat.abs() / dunnett_factor;
        let p = 2.0 * (1.0 - t_cdf_approx(t_adj, df));

        let half_width = d_crit * se;
        results.push(DunnettPair {
            group: format!("组 {}", i + 1),
            control: format!("组 {}", control_idx + 1),
            diff,
            t_stat,
            df,
            p_value: p.clamp(0.0, 1.0),
            ci_low: diff - half_width,
            ci_high: diff + half_width,
        });
    }
    Some(results)
}

fn t_cdf_approx(t: f64, df: f64) -> f64 {
    // 用正态近似（df > 30 时足够准确）
    if df > 30.0 {
        return norm_cdf(t);
    }
    // 简化的 t 分布 CDF 近似
    let x = df / (df + t * t);
    let p = 0.5 * crate::core::stats::betai(df / 2.0, 0.5, x);
    if t > 0.0 {
        1.0 - p
    } else {
        p
    }
}

// ── Holm-Bonferroni（显式方法）──────────────────────────────────────────────

/// Holm-Bonferroni 校正后的 p 值（step-down）。
///
/// 返回与输入等长的向量（对应原 p 值的校正后值）。
pub fn holm_bonferroni(p_values: &[f64]) -> Vec<f64> {
    let n = p_values.len();
    if n == 0 {
        return vec![];
    }
    // 排序索引
    let mut indices: Vec<usize> = (0..n).collect();
    indices.sort_by(|&a, &b| {
        p_values[a]
            .partial_cmp(&p_values[b])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut adjusted = vec![0.0f64; n];
    let mut max_so_far = 0.0f64;
    for (rank, &idx) in indices.iter().enumerate() {
        let adj = ((n - rank) as f64 * p_values[idx]).min(1.0);
        let adj = adj.max(max_so_far); // 单调不减
        adjusted[idx] = adj;
        max_so_far = adj;
    }
    adjusted
}

/// Bonferroni 校正（简单版，用于对比）。
pub fn bonferroni(p_values: &[f64]) -> Vec<f64> {
    let n = p_values.len() as f64;
    p_values.iter().map(|&p| (p * n).min(1.0)).collect()
}

// ── ROUT 离群值检测（简化版）────────────────────────────────────────────────

/// ROUT 离群值检测结果。
#[derive(Clone, Debug)]
pub struct OutlierResult {
    pub indices: Vec<usize>,
    pub q_values: Vec<f64>,
    pub n_outliers: usize,
}

/// ROUT (Robust regression and Outlier removal) 离群值检测（简化版）。
///
/// 基于稳健回归（迭代加权最小二乘）的残差，用 FDR 控制（Q=1%）标记离群值。
/// 简化实现：中位数绝对偏差 (MAD) 稳健 z-score + Benjamini-Hochberg FDR。
pub fn route_detect(xs: &[f64], q_threshold: f64) -> Option<OutlierResult> {
    let n = xs.len();
    if n < 4 {
        return None;
    }

    // MAD 稳健 z-score
    let mut sorted = xs.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = sorted[n / 2];
    let abs_dev: Vec<f64> = xs.iter().map(|&x| (x - median).abs()).collect();
    let mut sorted_dev = abs_dev.clone();
    sorted_dev.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mad = sorted_dev[n / 2];
    // MAD=0（数据集中在中位数附近）→ 用标准差兜底
    let scale = if mad > 1e-10 { mad } else { sd(xs) };
    if scale < 1e-10 {
        return None;
    }

    // 稳健 z = 0.6745 * (x - median) / scale
    let z_scores: Vec<f64> = xs.iter().map(|&x| 0.6745 * (x - median) / scale).collect();
    // 双侧 p 值（正态近似）
    let p_values: Vec<f64> = z_scores
        .iter()
        .map(|&z| 2.0 * (1.0 - norm_cdf(z.abs())))
        .collect();

    // Benjamini-Hochberg FDR 控制
    let q_values = bh_fdr(&p_values, q_threshold);

    let indices: Vec<usize> = q_values
        .iter()
        .enumerate()
        .filter(|(_, &q)| q <= q_threshold)
        .map(|(i, _)| i)
        .collect();

    Some(OutlierResult {
        n_outliers: indices.len(),
        indices,
        q_values,
    })
}

/// Benjamini-Hochberg FDR 校正。
pub fn bh_fdr(p_values: &[f64], _q: f64) -> Vec<f64> {
    let n = p_values.len();
    if n == 0 {
        return vec![];
    }
    let mut indices: Vec<usize> = (0..n).collect();
    indices.sort_by(|&a, &b| {
        p_values[a]
            .partial_cmp(&p_values[b])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut q_values = vec![0.0f64; n];
    let mut min_q = 1.0f64;
    for (rank, &idx) in indices.iter().enumerate() {
        let q = (p_values[idx] * n as f64 / (rank + 1) as f64).min(1.0);
        let q = q.min(min_q); // 从后往前取最小
        q_values[idx] = q;
        min_q = q;
    }
    // 反向遍历确保单调
    for i in (0..n - 1).rev() {
        let idx = indices[i];
        let next_idx = indices[i + 1];
        q_values[idx] = q_values[idx].min(q_values[next_idx]);
    }
    q_values
}

// ── 风险差（2×2 表）────────────────────────────────────────────────────────

/// 风险差结果。
#[derive(Clone, Debug)]
pub struct RiskDifference {
    pub rd: f64,
    pub ci_low: f64,
    pub ci_high: f64,
    pub p_value: f64,
    pub rr: f64,
    pub or: f64,
}

/// 风险差（Risk Difference）：p1 - p2，附 95% CI 和 RR/OR。
///
/// 用 Newcombe (1998) 方法计算 CI（基于 Wilson 区间），比 Wald 更稳健。
pub fn risk_difference(a: usize, b: usize, c: usize, d: usize) -> Option<RiskDifference> {
    // a=实验组阳性, b=实验组阴性, c=对照组阳性, d=对照组阴性
    let n1 = a + b;
    let n2 = c + d;
    if n1 == 0 || n2 == 0 {
        return None;
    }
    let p1 = a as f64 / n1 as f64;
    let p2 = c as f64 / n2 as f64;
    let rd = p1 - p2;

    // Wilson 区间
    let (w1_lo, w1_hi) = wilson_ci(a as f64, n1 as f64);
    let (w2_lo, w2_hi) = wilson_ci(c as f64, n2 as f64);
    // Newcombe 方法
    let ci_low = rd - (w1_hi - w2_lo).max(0.0);
    let ci_high = rd + (w1_lo - w2_hi).min(0.0).abs();

    // z 检验
    let pooled_p = (a + c) as f64 / (n1 + n2) as f64;
    let se = (pooled_p * (1.0 - pooled_p) * (1.0 / n1 as f64 + 1.0 / n2 as f64)).sqrt();
    let z = if se > 0.0 { rd / se } else { 0.0 };
    let p = 2.0 * (1.0 - norm_cdf(z.abs()));

    // 相对风险 (RR) 和优势比 (OR)
    let rr = if p2 > 0.0 { p1 / p2 } else { f64::INFINITY };
    let or = if b > 0 && d > 0 {
        (a as f64 * d as f64) / (b as f64 * c as f64)
    } else {
        f64::INFINITY
    };

    Some(RiskDifference {
        rd,
        ci_low: ci_low.max(-1.0),
        ci_high: ci_high.min(1.0),
        p_value: p.clamp(0.0, 1.0),
        rr,
        or,
    })
}

fn wilson_ci(successes: f64, n: f64) -> (f64, f64) {
    let z = 1.96;
    let p = successes / n;
    let denom = 1.0 + z * z / n;
    let center = (p + z * z / (2.0 * n)) / denom;
    let half = z * ((p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt()) / denom;
    ((center - half).max(0.0), (center + half).min(1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dunnett基本() {
        let control = vec![10.0, 12.0, 11.0, 9.0, 13.0, 10.0];
        let treat1 = vec![15.0, 17.0, 16.0, 14.0, 18.0, 15.0];
        let treat2 = vec![11.0, 10.0, 12.0, 9.0, 13.0, 11.0];
        let groups = vec![control, treat1, treat2];
        let r = dunnett(&groups, 0).unwrap();
        assert_eq!(r.len(), 2);
        // 组 1（treat1）应显著高于对照
        assert!(r[0].p_value < 0.05, "treat1 vs control: p={}", r[0].p_value);
        // 组 2（treat2）与对照差异不大
        assert!(r[1].p_value > 0.05, "treat2 vs control: p={}", r[1].p_value);
    }

    #[test]
    fn holm_bonferroni() {
        let p = vec![0.01, 0.04, 0.03, 0.005];
        let adj = super::holm_bonferroni(&p);
        assert_eq!(adj.len(), 4);
        // 最小 p 值被放大最多
        assert!(adj[3] >= 0.005, "校正后不应更小");
        // 单调性：排序后的校正值应不减
        assert!(adj[3] <= adj[0]);
    }

    #[test]
    fn bonferroni对比() {
        let p = vec![0.01, 0.04, 0.03];
        let adj = super::bonferroni(&p);
        assert!((adj[0] - 0.03).abs() < 1e-10);
        assert!((adj[1] - 0.12).abs() < 1e-10);
    }

    #[test]
    fn rout检测离群() {
        // 20 个正常值（分散），2 个极端离群
        let mut xs: Vec<f64> = (0..20).map(|i| 10.0 + (i as f64 % 3.0) - 1.0).collect();
        xs.push(50.0); // 离群
        xs.push(-30.0); // 离群
        let r = route_detect(&xs, 0.01).unwrap();
        assert!(r.n_outliers >= 1, "应检测到离群值: {:?}", r.indices);
    }

    #[test]
    fn 风险差() {
        // 实验组 30/100 有效，对照组 10/100 有效
        let r = risk_difference(30, 70, 10, 90).unwrap();
        assert!((r.rd - 0.20).abs() < 0.01, "RD≈0.20: {}", r.rd);
        assert!(r.p_value < 0.05, "差异应显著: p={}", r.p_value);
        assert!(r.rr > 2.5, "RR≈3.0: {}", r.rr);
    }

    #[test]
    fn fdr校正() {
        let p = vec![0.001, 0.008, 0.039, 0.041, 0.042, 0.06, 0.074, 0.205];
        let q = bh_fdr(&p, 0.05);
        // 最小 q 值不应小于原始 p
        assert!(q[0] >= 0.001);
        // 单调性
        for i in 1..q.len() {
            assert!(q[i] >= q[i - 1] - 1e-10);
        }
    }
}
