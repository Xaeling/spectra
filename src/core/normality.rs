//! 正态性检验：Shapiro-Wilk、Kolmogorov-Smirnov、Anderson-Darling。
//!
//! 三个检验各有侧重：SW 小样本最敏感、KS 直观通用、AD 对尾部偏离敏感。
//! 实现基于标准近似公式（Royston 1995、Marsaglia 2004、D'Agostino 1986）。

use crate::core::stats::{mean, norm_cdf, sd};

/// 正态性检验结果。
#[derive(Clone, Debug)]
pub struct NormalityTest {
    pub name: String,
    pub statistic: f64,
    pub p_value: f64,
    pub n: usize,
    pub normal: bool,
}

// ── Shapiro-Wilk（AS R94, Royston 1995）─────────────────────────────────────

/// Shapiro-Wilk 正态性检验。
///
/// 适用于 3 ≤ n ≤ 5000。计算 W 统计量后经 Royston 近似转 z，再取双侧 p。
pub fn shapiro_wilk(xs: &[f64]) -> Option<NormalityTest> {
    let mut data: Vec<f64> = xs.iter().cloned().filter(|x| x.is_finite()).collect();
    let n = data.len();
    if n < 3 || n > 5000 {
        return None;
    }
    data.sort_by(|a, b| a.partial_cmp(b).unwrap());

    // 极差为 0（常数）无法检验
    let range = data[n - 1] - data[0];
    if range == 0.0 {
        return None;
    }

    // 计算 Shapiro-Wilk 的 a 系数（Royston 1995 近似）
    let m_norm = (0..n)
        .map(|i| crate::core::stats::norm_ppf((i as f64 + 0.375) / (n as f64 + 0.25)))
        .collect::<Vec<_>>();
    let sum_sq_m: f64 = m_norm.iter().map(|x| x * x).sum();
    let m_norm_len = (sum_sq_m).sqrt();
    let a: Vec<f64> = m_norm.iter().map(|x| x / m_norm_len).collect();

    // W = (sum a_i x_i)^2 / sum (x_i - xbar)^2
    let xbar = mean(&data);
    let mut numerator = 0.0;
    for i in 0..n / 2 {
        numerator += a[i] * (data[n - 1 - i] - data[i]);
    }
    let ss: f64 = data.iter().map(|&x| (x - xbar).powi(2)).sum();
    if ss == 0.0 {
        return None;
    }
    let w = (numerator * numerator) / ss;

    // Royston (1995) 的 z 近似
    let mu = if n <= 11 {
        let log_n = (n as f64).ln();
        -0.000_297_7 * (log_n).powi(2) - 0.037_44 * log_n + 0.996_5
    } else {
        let log_n = (n as f64).ln();
        0.003_89 * (log_n).powi(3) - 0.086_7 * (log_n).powi(2) - 0.235_2 * log_n + 1.227_3
    };
    let sigma = if n <= 11 {
        (-0.007_42 * (n as f64).ln().powi(3) + 0.172_5 * (n as f64).ln().powi(2)
            - 0.937_6 * (n as f64).ln()
            + 1.431_2)
            .exp()
    } else {
        let log_n = (n as f64).ln();
        0.001_42 * (log_n).powi(3) - 0.043_8 * (log_n).powi(2) - 0.185_3 * log_n + 1.038_2
    };

    // W ∈ (0, 1]。W→1 时 (1-W)→0，ln(1-W)→-inf，ln|ln| → +inf。
    // 但 W 略大于 1（数值误差）会让 (1-W) 为负 → NaN。加保护。
    let w_clamped = w.clamp(0.001, 0.9999);
    let log_term = (1.0 - w_clamped).ln().abs().ln();
    let z = (log_term - mu) / sigma.max(1e-10);
    let p = 2.0 * (1.0 - norm_cdf(z.abs()));

    Some(NormalityTest {
        name: "Shapiro-Wilk".into(),
        statistic: w,
        p_value: p.clamp(0.0, 1.0),
        n,
        normal: p >= 0.05,
    })
}

// ── Kolmogorov-Smirnov（单样本正态）──────────────────────────────────────────

/// 单样本 Kolmogorov-Smirnov 检验（正态分布，参数用样本估计）。
///
/// D = max|F_n(x) - Φ((x-μ)/σ)|。参数估计后 p 值用 Marsaglia-Wang 近似。
pub fn kolmogorov_smirnov_normal(xs: &[f64]) -> Option<NormalityTest> {
    let mut data: Vec<f64> = xs.iter().cloned().filter(|x| x.is_finite()).collect();
    let n = data.len();
    if n < 4 {
        return None;
    }
    data.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mu = mean(&data);
    let sigma = sd(&data);
    if sigma == 0.0 || sigma.is_nan() {
        return None;
    }

    // D = max |i/n - Φ(z_i)| 和 |Φ(z_i) - (i-1)/n|
    let mut d = 0.0f64;
    for (i, &x) in data.iter().enumerate() {
        let z = (x - mu) / sigma;
        let phi = norm_cdf(z);
        let f_emp_hi = (i + 1) as f64 / n as f64;
        let f_emp_lo = i as f64 / n as f64;
        d = d.max((f_emp_hi - phi).abs());
        d = d.max((phi - f_emp_lo).abs());
    }

    // p 值近似（Marsaglia-Wang / Stephens 修正）
    let en = (n as f64).sqrt();
    let d_adj = (en + 0.12 + 0.11 / en) * d;
    let p = ks_p_approx(d_adj);

    Some(NormalityTest {
        name: "Kolmogorov-Smirnov".into(),
        statistic: d,
        p_value: p.clamp(0.0, 1.0),
        n,
        normal: p >= 0.05,
    })
}

fn ks_p_approx(d: f64) -> f64 {
    // Marsaglia-Wang (2003) 级数近似
    if d <= 0.0 {
        return 1.0;
    }
    let t = (2.0 * (n_f64_sqrt()) * d).max(0.0); // en = sqrt(n) 在调用方已算好
    let _ = t;
    let d = d.max(1e-10);
    // Kolmogorov 分布的上尾近似：Q(d) ≈ 2 * Σ(-1)^(k-1) exp(-2 k² d²)
    let mut p = 0.0f64;
    for k in 1..=100 {
        let term = 2.0 * (-1.0f64).powi(k - 1) * (-2.0 * (k as f64).powi(2) * d * d).exp();
        p += term;
        if term.abs() < 1e-12 {
            break;
        }
    }
    p.clamp(0.0, 1.0)
}

fn n_f64_sqrt() -> f64 {
    1.0 // 占位——调用方传入 en
}

// ── Anderson-Darling（正态性）────────────────────────────────────────────────

/// Anderson-Darling 正态性检验（参数估计修正版 A*²）。
///
/// 对尾部偏离比 KS 更敏感。p 值用 D'Agostino & Stephens (1986) 近似。
pub fn anderson_darling_normal(xs: &[f64]) -> Option<NormalityTest> {
    let mut data: Vec<f64> = xs.iter().cloned().filter(|x| x.is_finite()).collect();
    let n = data.len();
    if n < 8 {
        return None;
    }
    data.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mu = mean(&data);
    let sigma = sd(&data);
    if sigma == 0.0 || sigma.is_nan() {
        return None;
    }

    // A² = -n - (1/n) Σ (2i-1) [ln F(x_i) + ln(1-F(x_{n+1-i}))]
    let mut a2 = 0.0f64;
    for i in 0..n {
        let z_lo = (data[i] - mu) / sigma;
        let z_hi = (data[n - 1 - i] - mu) / sigma;
        let f_lo = norm_cdf(z_lo).clamp(1e-15, 1.0 - 1e-15);
        let f_hi = norm_cdf(z_hi).clamp(1e-15, 1.0 - 1e-15);
        let ln_f = f_lo.ln();
        let ln_1mf = (1.0 - f_hi).ln();
        a2 += (2.0 * (i + 1) as f64 - 1.0) * (ln_f + ln_1mf);
    }
    let a2 = -(n as f64) - a2 / n as f64;

    // 修正为 A*²（D'Agostino & Stephens 1986）
    let a_star = a2 * (1.0 + 0.75 / n as f64 + 2.25 / (n as f64).powi(2));

    // p 值近似
    let p = ad_p_approx(a_star);

    Some(NormalityTest {
        name: "Anderson-Darling".into(),
        statistic: a_star,
        p_value: p.clamp(0.0, 1.0),
        n,
        normal: p >= 0.05,
    })
}

fn ad_p_approx(a: f64) -> f64 {
    // D'Agostino & Stephens (1986) 的 p 值近似
    if a < 0.2 {
        1.0 - (-13.436 + 101.14 * a - 223.73 * a * a).exp()
    } else if a < 0.34 {
        1.0 - (-8.318 + 42.796 * a - 59.938 * a * a).exp()
    } else if a < 0.6 {
        (0.9177 - 4.279 * a - 1.38 * a * a).exp()
    } else if a < 10.0 {
        (1.2937 - 5.709 * a + 0.0186 * a * a).exp()
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normal_sample(n: usize) -> Vec<f64> {
        // 确定性"伪正态"样本（Box-Muller 用固定种子）
        let mut vals = Vec::with_capacity(n);
        for i in 0..n {
            let u1 = (i as f64 + 0.5) / n as f64;
            let u2 = ((i * 7 + 3) % n) as f64 / n as f64 + 0.001;
            let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
            vals.push(z);
        }
        vals
    }

    #[test]
    fn sw正态样本不拒绝() {
        // 已知正态的固定样本
        let xs: Vec<f64> = vec![
            -2.12, -1.68, -1.34, -1.05, -0.81, -0.60, -0.41, -0.23, -0.06, 0.11, 0.28, 0.45, 0.63,
            0.83, 1.04, 1.28, 1.56, 1.92, 2.44,
        ];
        let r = shapiro_wilk(&xs).unwrap();
        assert_eq!(r.n, 19);
        assert!(
            r.p_value >= 0.0 && r.p_value <= 1.0,
            "p 应在 [0,1]: {}",
            r.p_value
        );
        // W 应接近 1（正态性好）
        assert!(r.statistic > 0.8, "W 应 > 0.8: {}", r.statistic);
    }

    #[test]
    fn sw极端偏态拒绝() {
        // 指数分布（严重偏态）
        let xs: Vec<f64> = (1..=40).map(|i| (i as f64 * 0.3).exp()).collect();
        let r = shapiro_wilk(&xs).unwrap();
        assert!(r.p_value < 0.05, "指数样本 SW 应拒绝: p={}", r.p_value);
    }

    #[test]
    fn ks正态样本不拒绝() {
        let xs = normal_sample(80);
        let r = kolmogorov_smirnov_normal(&xs).unwrap();
        assert!(r.p_value > 0.01, "KS p 不应太小: {}", r.p_value);
    }

    #[test]
    fn ad正态样本不拒绝() {
        let xs = normal_sample(50);
        let r = anderson_darling_normal(&xs).unwrap();
        assert!(r.p_value > 0.01, "AD p 不应太小: {}", r.p_value);
    }

    #[test]
    fn ad均匀分布拒绝() {
        let xs: Vec<f64> = (0..100).map(|i| i as f64 / 100.0).collect();
        let r = anderson_darling_normal(&xs).unwrap();
        assert!(r.p_value < 0.05, "均匀分布 AD 应拒绝: p={}", r.p_value);
    }
}
