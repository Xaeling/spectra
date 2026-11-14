//! 推断补全（v0.13）：多重比较、方差齐性、区组非参数、离群值、Deming、
//! 相关矩阵、主成分、聚类、OR/RR/McNemar、多元回归、Cox。
//!
//! 与 `stats.rs` 的分工：`stats.rs` 放「最常用、被绘图直接调用」的那批
//! （t / ANOVA / 秩和 / 相关 / 回归 / KM / ROC）；这里放分析菜单里的
//! 进阶检验。两者共用同一套分布函数（`t_p`、`f_sf`、`ln_gamma`），
//! 不重复实现数值积分，避免「同一个 p 值两条路径算出两个数」。
//!
//! 所有函数都是纯函数：输入 `&[f64]`，输出结构体；不读全局、不打印。

use crate::core::stats::{f_sf, ln_gamma, norm_cdf, t_p, t_quantile};

// ---------------------------------------------------------------------------
// 分布：χ² 与 学生化极差
// ---------------------------------------------------------------------------

/// χ² 上尾概率 P(X > x)。
pub fn chi2_sf(x: f64, df: f64) -> f64 {
    if x <= 0.0 {
        return 1.0;
    }
    if df <= 0.0 {
        return f64::NAN;
    }
    gamma_q(df / 2.0, x / 2.0)
}

/// 正则化不完全 gamma 的上尾 Q(a, x)（连分式 / 级数两条分支）。
pub fn gamma_q(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 1.0;
    }
    if x < a + 1.0 {
        1.0 - gamma_p_series(a, x)
    } else {
        gamma_q_cf(a, x)
    }
}

fn gamma_p_series(a: f64, x: f64) -> f64 {
    let mut sum = 1.0 / a;
    let mut term = sum;
    for n in 1..500 {
        term *= x / (a + n as f64);
        sum += term;
        if term.abs() < sum.abs() * 1e-14 {
            break;
        }
    }
    (sum * (-x + a * x.ln() - ln_gamma(a)).exp()).clamp(0.0, 1.0)
}

fn gamma_q_cf(a: f64, x: f64) -> f64 {
    let tiny = 1e-300;
    let mut b = x + 1.0 - a;
    let mut c = 1.0 / tiny;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..500 {
        let an = -(i as f64) * (i as f64 - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < tiny {
            d = tiny;
        }
        c = b + an / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 1e-14 {
            break;
        }
    }
    (h * (-x + a * x.ln() - ln_gamma(a)).exp()).clamp(0.0, 1.0)
}

fn norm_pdf(z: f64) -> f64 {
    (-0.5 * z * z).exp() / (2.0 * std::f64::consts::PI).sqrt()
}

/// χ 分布密度（ν 自由度，s = x/√ν 为样本标准差）。
fn chi_pdf(x: f64, df: f64) -> f64 {
    if x <= 0.0 || df <= 0.0 {
        return 0.0;
    }
    (2.0f64).powf(1.0 - df / 2.0) * x.powf(df - 1.0) * (-x * x / 2.0).exp()
        / ln_gamma(df / 2.0).exp()
}

/// 学生化极差 CDF P(Q ≤ q)：k 组、误差自由度 df。
///
/// Q = (max − min)/s，s 与样本独立且 νs²/σ² ~ χ²_ν。给定 s：
/// P(max−min ≤ q·s) = k∫φ(z)[Φ(z+q·s) − Φ(z)]^(k−1)dz，再对 s 的 χ 分布积分。
/// 两层都用 Simpson；内层固定 [−7, 7]（φ 在 7 以外 < 1e-11，多算无益）。
pub fn q_cdf(q: f64, k: usize, df: f64) -> f64 {
    if q <= 0.0 || k < 2 || df <= 0.0 {
        return 0.0;
    }
    let km1 = (k - 1) as i32;
    let kf = k as f64;
    let inner = |s: f64| -> f64 {
        let (lo, hi, n) = (-7.0, 7.0, 200);
        let h = (hi - lo) / n as f64;
        let shift = q * s;
        let mut acc = 0.0;
        for i in 0..=n {
            let z = lo + i as f64 * h;
            let d = norm_cdf(z + shift) - norm_cdf(z);
            let w = if i == 0 || i == n {
                1.0
            } else if i % 2 == 1 {
                4.0
            } else {
                2.0
            };
            acc += w * norm_pdf(z) * d.powi(km1);
        }
        kf * acc * h / 3.0
    };
    let sq = df.sqrt();
    let xmax = sq + 5.0;
    let n = 140;
    let h = xmax / n as f64;
    let mut acc = 0.0;
    for i in 1..=n {
        let x = i as f64 * h;
        let w = if i == n {
            1.0
        } else if i % 2 == 1 {
            4.0
        } else {
            2.0
        };
        acc += w * chi_pdf(x, df) * inner(x / sq);
    }
    (acc * h / 3.0).clamp(0.0, 1.0)
}

pub fn q_sf(q: f64, k: usize, df: f64) -> f64 {
    (1.0 - q_cdf(q, k, df)).clamp(0.0, 1.0)
}

/// 学生化极差临界值（二分求逆 + 线程内缓存）。
///
/// 一次 ANOVA 里同一组 (conf, k, df) 会被查很多次（每对比较一次），
/// 单次求逆是 40 次二分 × 两层积分——不缓存时 6 组 15 对要十几秒。
pub fn q_crit(conf: f64, k: usize, df: f64) -> f64 {
    use std::cell::RefCell;
    use std::collections::HashMap;
    thread_local! {
        static CACHE: RefCell<HashMap<(u64, usize, u64), f64>> = RefCell::new(HashMap::new());
    }
    let key = ((conf * 1e9).round() as u64, k, (df * 1e9).round() as u64);
    if let Some(v) = CACHE.with(|c| c.borrow().get(&key).copied()) {
        return v;
    }
    let (mut lo, mut hi) = (0.0, 60.0);
    for _ in 0..44 {
        let mid = (lo + hi) / 2.0;
        if q_cdf(mid, k, df) < conf {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let v = (lo + hi) / 2.0;
    CACHE.with(|c| {
        c.borrow_mut().insert(key, v);
    });
    v
}

// ---------------------------------------------------------------------------
// 多重比较
// ---------------------------------------------------------------------------

/// 一对比较的结果。
#[derive(Clone, Debug)]
pub struct Pairwise {
    pub a: usize,
    pub b: usize,
    pub diff: f64,
    pub se: f64,
    pub ci_lo: f64,
    pub ci_hi: f64,
    pub stat: f64,
    pub p: f64,
}

fn anova_parts(groups: &[Vec<f64>]) -> Option<(f64, f64, usize, usize)> {
    // (MS_within, df_within, k, n)
    let gs: Vec<&Vec<f64>> = groups.iter().filter(|g| g.len() >= 2).collect();
    if gs.len() < 2 {
        return None;
    }
    let k = gs.len();
    let n: usize = gs.iter().map(|g| g.len()).sum();
    if n <= k {
        return None;
    }
    let grand = gs.iter().flat_map(|g| g.iter()).sum::<f64>() / n as f64;
    let mut ssw = 0.0;
    for g in &gs {
        let m = g.iter().sum::<f64>() / g.len() as f64;
        ssw += g.iter().map(|x| (x - m).powi(2)).sum::<f64>();
        let _ = grand;
    }
    let dfw = (n - k) as f64;
    Some((ssw / dfw, dfw, k, n))
}

/// Tukey HSD（假定方差齐性）：SE = √(MS_w·½·(1/nᵢ+1/nⱼ))，q 分布。
pub fn tukey_hsd(groups: &[Vec<f64>], conf: f64) -> Option<Vec<Pairwise>> {
    let (msw, dfw, k, _) = anova_parts(groups)?;
    let gs: Vec<&Vec<f64>> = groups.iter().filter(|g| g.len() >= 2).collect();
    let qc = q_crit(conf, k, dfw);
    let mut out = Vec::new();
    for i in 0..gs.len() {
        for j in (i + 1)..gs.len() {
            let mi = gs[i].iter().sum::<f64>() / gs[i].len() as f64;
            let mj = gs[j].iter().sum::<f64>() / gs[j].len() as f64;
            let se = (msw * 0.5 * (1.0 / gs[i].len() as f64 + 1.0 / gs[j].len() as f64)).sqrt();
            if !(se > 0.0) {
                continue;
            }
            let d = mi - mj;
            let q = d.abs() / se;
            out.push(Pairwise {
                a: i,
                b: j,
                diff: d,
                se,
                ci_lo: d - qc * se,
                ci_hi: d + qc * se,
                stat: q,
                p: q_sf(q, k, dfw),
            });
        }
    }
    Some(out)
}

/// Games–Howell（不假定方差齐性）：每对用 Welch 的 SE 与 df，仍走 q 分布。
pub fn games_howell(groups: &[Vec<f64>], conf: f64) -> Option<Vec<Pairwise>> {
    let gs: Vec<&Vec<f64>> = groups.iter().filter(|g| g.len() >= 2).collect();
    let k = gs.len();
    if k < 2 {
        return None;
    }
    let var = |g: &Vec<f64>| {
        let m = g.iter().sum::<f64>() / g.len() as f64;
        g.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (g.len() - 1) as f64
    };
    let mut out = Vec::new();
    for i in 0..k {
        for j in (i + 1)..k {
            let (ni, nj) = (gs[i].len() as f64, gs[j].len() as f64);
            let (vi, vj) = (var(gs[i]) / ni, var(gs[j]) / nj);
            let se = (0.5 * (vi + vj)).sqrt();
            if !(se > 0.0) {
                continue;
            }
            let df = (vi + vj).powi(2) / (vi.powi(2) / (ni - 1.0) + vj.powi(2) / (nj - 1.0));
            let mi = gs[i].iter().sum::<f64>() / ni;
            let mj = gs[j].iter().sum::<f64>() / nj;
            let d = mi - mj;
            let q = d.abs() / se;
            let half = q_crit(conf, k, df) * se;
            out.push(Pairwise {
                a: i,
                b: j,
                diff: d,
                se,
                ci_lo: d - half,
                ci_hi: d + half,
                stat: q,
                p: q_sf(q, k, df),
            });
        }
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// 方差齐性与离群值
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct VarianceTest {
    pub stat: f64,
    pub df1: f64,
    pub df2: f64,
    pub p: f64,
}

/// Bartlett 检验：方差齐性，对**非正态很敏感**（正态前提不成立时误报率高）。
pub fn bartlett(groups: &[Vec<f64>]) -> Option<VarianceTest> {
    let gs: Vec<Vec<f64>> = groups
        .iter()
        .map(|g| g.iter().copied().filter(|z| z.is_finite()).collect())
        .filter(|g: &Vec<f64>| g.len() >= 2)
        .collect();
    let k = gs.len();
    if k < 2 {
        return None;
    }
    let ns: Vec<f64> = gs.iter().map(|g| g.len() as f64).collect();
    let n: f64 = ns.iter().sum();
    let dfw = n - k as f64;
    let vars: Vec<f64> = gs
        .iter()
        .map(|g| {
            let m = g.iter().sum::<f64>() / g.len() as f64;
            g.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (g.len() - 1) as f64
        })
        .collect();
    if vars.iter().any(|v| !(*v > 0.0)) {
        return None;
    }
    let sp2 = gs
        .iter()
        .zip(vars.iter())
        .map(|(g, v)| (g.len() - 1) as f64 * v)
        .sum::<f64>()
        / dfw;
    let mut num = dfw * sp2.ln();
    for (ni, v) in ns.iter().zip(vars.iter()) {
        num -= (ni - 1.0) * v.ln();
    }
    let c = ns.iter().map(|ni| 1.0 / (ni - 1.0)).sum::<f64>() - 1.0 / dfw;
    let denom = 1.0 + c / (3.0 * (k - 1) as f64);
    let chi2 = num / denom;
    let df = (k - 1) as f64;
    Some(VarianceTest {
        stat: chi2,
        df1: df,
        df2: f64::NAN,
        p: chi2_sf(chi2, df),
    })
}

/// Levene：对 |y − 组中心| 做单因素 ANOVA。`use_median` = Brown–Forsythe。
pub fn levene(groups: &[Vec<f64>], use_median: bool) -> Option<VarianceTest> {
    let devs: Vec<Vec<f64>> = groups
        .iter()
        .map(|g| {
            let mut s = g.clone();
            s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let c = if use_median {
                crate::core::stats::quantile(&s, 0.5)
            } else {
                s.iter().sum::<f64>() / s.len().max(1) as f64
            };
            g.iter().map(|x| (x - c).abs()).collect()
        })
        .filter(|z: &Vec<f64>| z.len() >= 2)
        .collect();
    let k = devs.len();
    if k < 2 {
        return None;
    }
    let (msw, dfw, kk, n) = anova_parts(&devs)?;
    let grand = devs.iter().flat_map(|g| g.iter()).sum::<f64>() / n as f64;
    let mut ssb = 0.0;
    for g in &devs {
        let m = g.iter().sum::<f64>() / g.len() as f64;
        ssb += g.len() as f64 * (m - grand).powi(2);
    }
    let df1 = (kk - 1) as f64;
    let msb = ssb / df1;
    let f = if msw > 0.0 { msb / msw } else { f64::INFINITY };
    Some(VarianceTest {
        stat: f,
        df1,
        df2: dfw,
        p: f_sf(f, df1, dfw),
    })
}

#[derive(Clone, Debug)]
pub struct Grubbs {
    /// 被判定（或最像）离群的那个值
    pub value: f64,
    /// 在输入切片里的下标
    pub index: usize,
    pub g: f64,
    pub p: f64,
    pub n: usize,
    pub critical: f64,
}

/// 单侧 Grubbs：偏离均值最大的点。
///
/// 临界值用精确公式 G_crit = (n−1)/√n · √(t²/(n−2+t²))，t = t_{α/(2n), n−2}；
/// 双侧 α 通过把单侧显著性除以 n 校正。
pub fn grubbs(v: &[f64], alpha: f64) -> Option<Grubbs> {
    let xs: Vec<f64> = v.iter().copied().filter(|z| z.is_finite()).collect();
    let n = xs.len();
    if n < 3 {
        return None;
    }
    let m = xs.iter().sum::<f64>() / n as f64;
    let sd = (xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (n - 1) as f64).sqrt();
    if !(sd > 0.0) {
        return None;
    }
    let (mut bi, mut bd) = (0usize, 0.0f64);
    for (i, &x) in xs.iter().enumerate() {
        if (x - m).abs() > bd {
            bd = (x - m).abs();
            bi = i;
        }
    }
    let g = bd / sd;
    let nf = n as f64;
    let t = t_quantile(1.0 - alpha / (2.0 * nf), nf - 2.0);
    let critical = (nf - 1.0) / nf.sqrt() * (t * t / (nf - 2.0 + t * t)).sqrt();
    // p：由 G 反算 t，再乘 n（Bonferroni，与临界值同一口径）
    let tt = if g * g * nf / (nf - 1.0) < 1.0 {
        g * ((nf * (nf - 2.0)) / (nf * nf - 1.0 - nf * g * g)).sqrt()
    } else {
        f64::INFINITY
    };
    let p = if tt.is_finite() {
        (t_p(tt, nf - 2.0) * nf).min(1.0)
    } else {
        0.0
    };
    Some(Grubbs {
        value: xs[bi],
        index: bi,
        g,
        p,
        n,
        critical,
    })
}

// ---------------------------------------------------------------------------
// 单样本 / 区组非参数
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct OneSample {
    pub n: usize,
    pub mean: f64,
    pub sd: f64,
    pub sem: f64,
    pub t: f64,
    pub df: f64,
    pub p: f64,
    pub ci_lo: f64,
    pub ci_hi: f64,
}

/// 单样本 t：H0: μ = mu0。t = (m − μ0)/SEM。
pub fn one_sample(x: &[f64], mu0: f64, conf: f64) -> Option<OneSample> {
    let v: Vec<f64> = x.iter().copied().filter(|z| z.is_finite()).collect();
    let n = v.len();
    if n < 2 {
        return None;
    }
    let mean = v.iter().sum::<f64>() / n as f64;
    let sd = (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1) as f64).sqrt();
    let sem = sd / (n as f64).sqrt();
    let df = (n - 1) as f64;
    let t = if sem > 0.0 {
        (mean - mu0) / sem
    } else if (mean - mu0).abs() < 1e-12 {
        0.0
    } else {
        f64::INFINITY.copysign(mean - mu0)
    };
    let p = if sem > 0.0 { t_p(t, df) } else { 1.0 };
    let tc = t_quantile(0.5 + conf / 2.0, df);
    Some(OneSample {
        n,
        mean,
        sd,
        sem,
        t,
        df,
        p,
        ci_lo: mean - tc * sem,
        ci_hi: mean + tc * sem,
    })
}

#[derive(Clone, Debug)]
pub struct Friedman {
    /// 未校正的 Q
    pub q: f64,
    /// 并列校正后的 Q
    pub q_corrected: f64,
    pub df: f64,
    pub p: f64,
    /// 各处理（列）的平均秩
    pub mean_ranks: Vec<f64>,
    pub blocks: usize,
    pub treatments: usize,
}

/// Friedman 检验：每行一个区组（受试者），每列一个处理。
///
/// 行内编秩，Q = 12/(b·k·(k+1)) Σ R_j² − 3b(k+1)；
/// 有并列时除以 C = 1 − Σ(t³−t)/(b·(k³−k))（与 Kendall's W 同源）。
pub fn friedman(blocks: &[Vec<f64>]) -> Option<Friedman> {
    let rows: Vec<&Vec<f64>> = blocks
        .iter()
        .filter(|r| r.iter().all(|v| v.is_finite()))
        .collect();
    let b = rows.len();
    if b < 2 {
        return None;
    }
    let k = rows[0].len();
    if k < 2 || rows.iter().any(|r| r.len() != k) {
        return None;
    }
    let mut rsum = vec![0.0f64; k];
    let mut tie_corr = 0.0;
    for row in &rows {
        let r = ranks(row);
        for (i, x) in r.iter().enumerate() {
            rsum[i] += x;
        }
        // 行内并列
        let mut s = (*row).clone();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mut i = 0;
        while i < s.len() {
            let mut j = i;
            while j + 1 < s.len() && (s[j + 1] - s[i]).abs() < 1e-12 {
                j += 1;
            }
            let t = (j - i + 1) as f64;
            if t > 1.0 {
                tie_corr += t.powi(3) - t;
            }
            i = j + 1;
        }
    }
    let bf = b as f64;
    let kf = k as f64;
    let q = 12.0 / (bf * kf * (kf + 1.0)) * rsum.iter().map(|s| s * s).sum::<f64>()
        - 3.0 * bf * (kf + 1.0);
    let denom = 1.0 - tie_corr / (bf * (kf.powi(3) - kf));
    let corrected = if denom > 1e-9 { q / denom } else { q };
    let df = (k - 1) as f64;
    Some(Friedman {
        q,
        q_corrected: corrected,
        df,
        p: chi2_sf(corrected, df),
        mean_ranks: rsum.iter().map(|s| s / bf).collect(),
        blocks: b,
        treatments: k,
    })
}

/// 平均秩（并列取平均）。
pub fn ranks(v: &[f64]) -> Vec<f64> {
    let mut idx: Vec<usize> = (0..v.len()).collect();
    idx.sort_by(|&a, &b| v[a].partial_cmp(&v[b]).unwrap_or(std::cmp::Ordering::Equal));
    let mut r = vec![0.0; v.len()];
    let mut i = 0;
    while i < idx.len() {
        let mut j = i;
        while j + 1 < idx.len() && (v[idx[j + 1]] - v[idx[i]]).abs() < 1e-12 {
            j += 1;
        }
        let avg = (i + j) as f64 / 2.0 + 1.0;
        for k in i..=j {
            r[idx[k]] = avg;
        }
        i = j + 1;
    }
    r
}

// ---------------------------------------------------------------------------
// 回归：Deming / 斜率比较
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Deming {
    pub slope: f64,
    pub intercept: f64,
    pub n: usize,
    pub r: f64,
}

/// Deming 回归（正交 / 主轴回归）：X 与 Y 都有测量误差时用，
/// λ = σ²(ε_y)/σ²(ε_x)（默认 1）。**λ 会实质改变斜率**，不是无害参数。
pub fn deming(xs: &[f64], ys: &[f64], lambda: f64) -> Option<Deming> {
    let lam = lambda.max(1e-9);
    let n = xs.len().min(ys.len());
    let pts: Vec<(f64, f64)> = (0..n)
        .map(|i| (xs[i], ys[i]))
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .collect();
    let m = pts.len();
    if m < 3 {
        return None;
    }
    let mx = pts.iter().map(|p| p.0).sum::<f64>() / m as f64;
    let my = pts.iter().map(|p| p.1).sum::<f64>() / m as f64;
    let sxx = pts.iter().map(|p| (p.0 - mx).powi(2)).sum::<f64>() / (m - 1) as f64;
    let syy = pts.iter().map(|p| (p.1 - my).powi(2)).sum::<f64>() / (m - 1) as f64;
    let sxy = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum::<f64>() / (m - 1) as f64;
    if !(sxy.abs() > 0.0) {
        return None;
    }
    let inner = syy - lam * sxx;
    let slope = (inner + (inner * inner + 4.0 * lam * sxy * sxy).sqrt()) / (2.0 * sxy);
    let r = sxy / (sxx * syy).sqrt();
    Some(Deming {
        slope,
        intercept: my - slope * mx,
        n: m,
        r,
    })
}

#[derive(Clone, Debug)]
pub struct SlopeCompare {
    pub slope1: f64,
    pub slope2: f64,
    pub diff: f64,
    pub se: f64,
    pub t: f64,
    pub df: f64,
    pub p: f64,
    pub n1: usize,
    pub n2: usize,
}

/// 两组斜率比较（ANCOVA 的合并误差项口径）：
/// SE = √(s²_合并/Sxx₁ + s²_合并/Sxx₂)，t = (b₁−b₂)/SE，df = df₁+df₂。
pub fn compare_slopes(x1: &[f64], y1: &[f64], x2: &[f64], y2: &[f64]) -> Option<SlopeCompare> {
    let fit = |x: &[f64], y: &[f64]| -> Option<(f64, f64, f64, f64, usize)> {
        let pts: Vec<(f64, f64)> = x
            .iter()
            .copied()
            .zip(y.iter().copied())
            .filter(|(a, b)| a.is_finite() && b.is_finite())
            .collect();
        let n = pts.len();
        if n < 3 {
            return None;
        }
        let mx = pts.iter().map(|p| p.0).sum::<f64>() / n as f64;
        let my = pts.iter().map(|p| p.1).sum::<f64>() / n as f64;
        let sxx: f64 = pts.iter().map(|p| (p.0 - mx).powi(2)).sum();
        let sxy: f64 = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
        if !(sxx > 0.0) {
            return None;
        }
        let b = sxy / sxx;
        let a = my - b * mx;
        let sse: f64 = pts.iter().map(|p| (p.1 - a - b * p.0).powi(2)).sum();
        Some((b, a, sse, sxx, n))
    };
    let (b1, _, sse1, sxx1, n1) = fit(x1, y1)?;
    let (b2, _, sse2, sxx2, n2) = fit(x2, y2)?;
    let df = (n1 + n2) as f64 - 4.0;
    if df <= 0.0 {
        return None;
    }
    let syx2 = (sse1 + sse2) / df;
    let se = (syx2 / sxx1 + syx2 / sxx2).sqrt();
    if !(se > 0.0) {
        return None;
    }
    let diff = b1 - b2;
    let t = diff / se;
    Some(SlopeCompare {
        slope1: b1,
        slope2: b2,
        diff,
        se,
        t,
        df,
        p: t_p(t, df),
        n1,
        n2,
    })
}

// ---------------------------------------------------------------------------
// 多变量：相关矩阵 / PCA / 聚类 / OLS
// ---------------------------------------------------------------------------

/// 相关矩阵（Pearson）：返回 (r, p)，p 用 t = r√((n−2)/(1−r²)) 双侧。
pub fn corr_matrix(cols: &[Vec<f64>]) -> Option<(Vec<Vec<f64>>, Vec<Vec<f64>>, usize)> {
    let k = cols.len();
    if k < 2 {
        return None;
    }
    let n = cols.iter().map(|c| c.len()).min()?;
    let rows: Vec<usize> = (0..n)
        .filter(|&i| cols.iter().all(|c| c[i].is_finite()))
        .collect();
    let m = rows.len();
    if m < 3 {
        return None;
    }
    let mut r = vec![vec![0.0; k]; k];
    let mut p = vec![vec![0.0; k]; k];
    let means: Vec<f64> = cols
        .iter()
        .map(|c| rows.iter().map(|&i| c[i]).sum::<f64>() / m as f64)
        .collect();
    let sds: Vec<f64> = cols
        .iter()
        .zip(means.iter())
        .map(|(c, mu)| {
            (rows.iter().map(|&i| (c[i] - mu).powi(2)).sum::<f64>() / (m - 1) as f64).sqrt()
        })
        .collect();
    for a in 0..k {
        for b in 0..k {
            if a == b {
                r[a][b] = 1.0;
                p[a][b] = 0.0;
                continue;
            }
            let cov = rows
                .iter()
                .map(|&i| (cols[a][i] - means[a]) * (cols[b][i] - means[b]))
                .sum::<f64>()
                / (m - 1) as f64;
            let rr = if sds[a] > 0.0 && sds[b] > 0.0 {
                cov / (sds[a] * sds[b])
            } else {
                f64::NAN
            };
            r[a][b] = rr;
            p[a][b] = if rr.is_finite() && rr.abs() < 1.0 {
                let t = rr * ((m as f64 - 2.0) / (1.0 - rr * rr)).sqrt();
                t_p(t, (m - 2) as f64)
            } else {
                0.0
            };
        }
    }
    Some((r, p, m))
}

#[derive(Clone, Debug)]
pub struct Pca {
    pub eigen: Vec<f64>,
    pub proportion: Vec<f64>,
    pub cumulative: Vec<f64>,
    /// loadings[变量][主成分]
    pub loadings: Vec<Vec<f64>>,
    /// scores[主成分][观测]
    pub scores: Vec<Vec<f64>>,
    pub n: usize,
    pub vars: usize,
}

/// 主成分分析（相关矩阵 + Jacobi 特征分解）。
///
/// 用相关矩阵（标准化）而不是协方差矩阵：变量量纲不同时，协方差矩阵会让
/// 「数值大的变量」独占第一主成分，那是量纲的假象。
pub fn pca(cols: &[Vec<f64>], use_corr: bool) -> Option<Pca> {
    let k = cols.len();
    if k < 2 {
        return None;
    }
    let n = cols.iter().map(|c| c.len()).min()?;
    let rows: Vec<usize> = (0..n)
        .filter(|&i| cols.iter().all(|c| c[i].is_finite()))
        .collect();
    let m = rows.len();
    if m < 3 {
        return None;
    }
    let mean: Vec<f64> = cols
        .iter()
        .map(|c| rows.iter().map(|&i| c[i]).sum::<f64>() / m as f64)
        .collect();
    let sd: Vec<f64> = cols
        .iter()
        .zip(mean.iter())
        .map(|(c, mu)| {
            (rows.iter().map(|&i| (c[i] - mu).powi(2)).sum::<f64>() / (m - 1) as f64).sqrt()
        })
        .collect();
    let scale: Vec<f64> = (0..k)
        .map(|i| if use_corr && sd[i] > 0.0 { sd[i] } else { 1.0 })
        .collect();
    let z: Vec<Vec<f64>> = (0..k)
        .map(|c| {
            rows.iter()
                .map(|&i| (cols[c][i] - mean[c]) / scale[c])
                .collect()
        })
        .collect();
    let mut cov = vec![vec![0.0; k]; k];
    for a in 0..k {
        for b in 0..k {
            cov[a][b] = (0..m).map(|i| z[a][i] * z[b][i]).sum::<f64>() / (m - 1) as f64;
        }
    }
    let (mut eigen, vecs) = jacobi_eigen(&cov)?;
    let mut ord: Vec<usize> = (0..k).collect();
    ord.sort_by(|&a, &b| {
        eigen[b]
            .partial_cmp(&eigen[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    eigen = ord.iter().map(|&i| eigen[i]).collect();
    let total: f64 = eigen.iter().map(|v| v.max(0.0)).sum();
    let proportion: Vec<f64> = eigen
        .iter()
        .map(|v| if total > 0.0 { v.max(0.0) / total } else { 0.0 })
        .collect();
    let mut cumulative = Vec::with_capacity(k);
    let mut acc = 0.0;
    for p in &proportion {
        acc += p;
        cumulative.push(acc);
    }
    let mut loadings = vec![vec![0.0; k]; k];
    for v in 0..k {
        for p in 0..k {
            loadings[v][p] = vecs[ord[p]][v];
        }
    }
    let mut scores = vec![vec![0.0; m]; k];
    for p in 0..k {
        for i in 0..m {
            scores[p][i] = (0..k).map(|v| z[v][i] * vecs[ord[p]][v]).sum::<f64>();
        }
    }
    Some(Pca {
        eigen,
        proportion,
        cumulative,
        loadings,
        scores,
        n: m,
        vars: k,
    })
}

/// 对称矩阵的 Jacobi 特征分解 → (特征值, 特征向量按行)。
fn jacobi_eigen(a: &[Vec<f64>]) -> Option<(Vec<f64>, Vec<Vec<f64>>)> {
    let n = a.len();
    if n == 0 {
        return None;
    }
    let mut m: Vec<Vec<f64>> = a.to_vec();
    let mut v = vec![vec![0.0; n]; n];
    for i in 0..n {
        v[i][i] = 1.0;
    }
    for _ in 0..100 {
        let off: f64 = (0..n)
            .map(|i| (i + 1..n).map(|j| m[i][j] * m[i][j]).sum::<f64>())
            .sum();
        if off < 1e-22 {
            break;
        }
        for i in 0..n {
            for j in (i + 1)..n {
                if m[i][j].abs() < 1e-18 {
                    continue;
                }
                let theta = (m[j][j] - m[i][i]) / (2.0 * m[i][j]);
                let t = if theta >= 0.0 { 1.0 } else { -1.0 }
                    / (theta.abs() + (theta * theta + 1.0).sqrt());
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for p in 0..n {
                    let (aip, ajp) = (m[i][p], m[j][p]);
                    m[i][p] = c * aip - s * ajp;
                    m[j][p] = s * aip + c * ajp;
                }
                for p in 0..n {
                    let (api, apj) = (m[p][i], m[p][j]);
                    m[p][i] = c * api - s * apj;
                    m[p][j] = s * api + c * apj;
                }
                for p in 0..n {
                    let (vip, vjp) = (v[i][p], v[j][p]);
                    v[i][p] = c * vip - s * vjp;
                    v[j][p] = s * vip + c * vjp;
                }
            }
        }
    }
    Some(((0..n).map(|i| m[i][i]).collect(), v))
}

#[derive(Clone, Debug)]
pub struct Merge {
    pub a: usize,
    pub b: usize,
    pub dist: f64,
    pub size: usize,
}

#[derive(Clone, Debug)]
pub struct Hclust {
    pub merges: Vec<Merge>,
    /// 切成 k 簇时每个观测的簇号
    pub cluster: Vec<usize>,
    pub k: usize,
    pub n: usize,
    /// 各观测的原始标签顺序
    pub order: Vec<usize>,
}

/// 凝聚式层次聚类（平均连接，欧氏距离，变量先标准化）。
///
/// 标准化是必须的：不标准化时「量纲大的变量」独自决定距离，聚类结果
/// 只是那张量纲表的排序。
pub fn hclust(cols: &[Vec<f64>], k: usize) -> Option<Hclust> {
    let k_vars = cols.len();
    if k_vars < 1 {
        return None;
    }
    let n = cols.iter().map(|c| c.len()).min()?;
    let rows: Vec<usize> = (0..n)
        .filter(|&i| cols.iter().all(|c| c[i].is_finite()))
        .collect();
    let m = rows.len();
    if m < 2 {
        return None;
    }
    let mut z: Vec<Vec<f64>> = Vec::with_capacity(k_vars);
    for c in cols {
        let vals: Vec<f64> = rows.iter().map(|&i| c[i]).collect();
        let mu = vals.iter().sum::<f64>() / m as f64;
        let sd = (vals.iter().map(|x| (x - mu).powi(2)).sum::<f64>() / (m - 1) as f64).sqrt();
        z.push(
            vals.iter()
                .map(|x| if sd > 0.0 { (x - mu) / sd } else { 0.0 })
                .collect(),
        );
    }
    let dist = |i: usize, j: usize| -> f64 {
        (0..k_vars)
            .map(|c| (z[c][i] - z[c][j]).powi(2))
            .sum::<f64>()
            .sqrt()
    };
    // 簇：每个元素一个簇，簇间距离用平均连接（UPGMA）
    let mut members: Vec<Vec<usize>> = (0..m).map(|i| vec![i]).collect();
    let mut merges = Vec::new();
    let mut assign = vec![0usize; m];
    let target_k = k.clamp(1, m);
    while members.len() > target_k {
        let mut best = (0usize, 1usize, f64::INFINITY);
        for i in 0..members.len() {
            for j in (i + 1)..members.len() {
                let mut acc = 0.0;
                for &x in &members[i] {
                    for &y in &members[j] {
                        acc += dist(x, y);
                    }
                }
                let d = acc / (members[i].len() * members[j].len()) as f64;
                if d < best.2 {
                    best = (i, j, d);
                }
            }
        }
        let (i, j, d) = best;
        if !d.is_finite() {
            break;
        }
        let b = members.remove(j);
        let a = members.remove(i);
        let size = a.len() + b.len();
        merges.push(Merge {
            a: a[0],
            b: b[0],
            dist: d,
            size,
        });
        let mut merged = a;
        merged.extend(b);
        members.insert(i, merged);
    }
    for (cid, mset) in members.iter().enumerate() {
        for &i in mset {
            assign[i] = cid;
        }
    }
    Some(Hclust {
        merges,
        cluster: assign,
        k: members.len(),
        n: m,
        order: rows,
    })
}

#[derive(Clone, Debug)]
pub struct KMeans {
    /// 每个观测的簇号
    pub cluster: Vec<usize>,
    /// 簇心（标准化后的坐标）
    pub centers: Vec<Vec<f64>>,
    /// 簇内平方和
    pub wss: f64,
    pub k: usize,
    pub n: usize,
    pub iterations: usize,
}

/// k-means（k-means++ 初始化，固定种子 → 结果可复现）。
pub fn kmeans(cols: &[Vec<f64>], k: usize, seed: u64) -> Option<KMeans> {
    let k_vars = cols.len();
    if k_vars < 1 {
        return None;
    }
    let n = cols.iter().map(|c| c.len()).min()?;
    let rows: Vec<usize> = (0..n)
        .filter(|&i| cols.iter().all(|c| c[i].is_finite()))
        .collect();
    let m = rows.len();
    let kk = k.clamp(1, m);
    if m < 2 || kk < 1 {
        return None;
    }
    let pts: Vec<Vec<f64>> = (0..m)
        .map(|i| (0..k_vars).map(|c| cols[c][rows[i]]).collect())
        .collect();
    let d2 = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum::<f64>();
    // k-means++
    let mut state = seed | 1;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f64 / (1u64 << 53) as f64
    };
    let mut centers: Vec<Vec<f64>> = vec![pts[(next() * m as f64) as usize % m].clone()];
    while centers.len() < kk {
        let dists: Vec<f64> = pts
            .iter()
            .map(|p| {
                centers
                    .iter()
                    .map(|c| d2(p, c))
                    .fold(f64::INFINITY, f64::min)
            })
            .collect();
        let total: f64 = dists.iter().sum();
        let mut pick = 0usize;
        if total > 0.0 {
            let mut target = next() * total;
            for (i, d) in dists.iter().enumerate() {
                target -= d;
                if target <= 0.0 {
                    pick = i;
                    break;
                }
            }
        }
        centers.push(pts[pick].clone());
    }
    let mut assign = vec![usize::MAX; m];
    let mut iterations = 0;
    for _ in 0..100 {
        iterations += 1;
        let mut changed = false;
        for (i, p) in pts.iter().enumerate() {
            let mut bi = 0;
            let mut bd = f64::INFINITY;
            for (c, ctr) in centers.iter().enumerate() {
                let d = d2(p, ctr);
                if d < bd {
                    bd = d;
                    bi = c;
                }
            }
            if assign[i] != bi {
                assign[i] = bi;
                changed = true;
            }
        }
        let mut sums = vec![vec![0.0; k_vars]; centers.len()];
        let mut counts = vec![0usize; centers.len()];
        for (i, p) in pts.iter().enumerate() {
            counts[assign[i]] += 1;
            for c in 0..k_vars {
                sums[assign[i]][c] += p[c];
            }
        }
        for c in 0..centers.len() {
            if counts[c] > 0 {
                for v in 0..k_vars {
                    centers[c][v] = sums[c][v] / counts[c] as f64;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let wss = pts
        .iter()
        .enumerate()
        .map(|(i, p)| d2(p, &centers[assign[i]]))
        .sum();
    Some(KMeans {
        cluster: assign,
        centers,
        wss,
        k: kk,
        n: m,
        iterations,
    })
}

/// 多元线性回归（含截距）。
#[derive(Clone, Debug)]
pub struct Ols {
    pub names: Vec<String>,
    pub coef: Vec<f64>,
    pub se: Vec<f64>,
    pub t: Vec<f64>,
    pub p: Vec<f64>,
    pub r2: f64,
    pub adj_r2: f64,
    pub f: f64,
    pub f_p: f64,
    pub n: usize,
    pub df_resid: f64,
    pub sse: f64,
}

/// OLS：y ~ 1 + x₁ + x₂ + …，正规方程 + 高斯消元，标准误来自 (X'X)⁻¹。
pub fn ols(y: &[f64], xs: &[(String, Vec<f64>)]) -> Option<Ols> {
    let p = xs.len();
    if p == 0 {
        return None;
    }
    let n = y.len().min(xs.iter().map(|(_, v)| v.len()).min()?);
    let rows: Vec<usize> = (0..n)
        .filter(|&i| y[i].is_finite() && xs.iter().all(|(_, v)| v[i].is_finite()))
        .collect();
    let m = rows.len();
    if m <= p + 1 {
        return None;
    }
    let cols = p + 1;
    let mut xmat: Vec<Vec<f64>> = Vec::with_capacity(m);
    let mut yv = Vec::with_capacity(m);
    for &i in &rows {
        let mut r = Vec::with_capacity(cols);
        r.push(1.0);
        for (_, v) in xs {
            r.push(v[i]);
        }
        xmat.push(r);
        yv.push(y[i]);
    }
    let mut xtx = vec![vec![0.0; cols]; cols];
    let mut xty = vec![0.0; cols];
    for (r, &yy) in xmat.iter().zip(yv.iter()) {
        for a in 0..cols {
            xty[a] += r[a] * yy;
            for b in 0..cols {
                xtx[a][b] += r[a] * r[b];
            }
        }
    }
    let inv = invert(&xtx)?;
    let coef: Vec<f64> = (0..cols)
        .map(|a| (0..cols).map(|b| inv[a][b] * xty[b]).sum())
        .collect();
    let mut sse = 0.0;
    let mut sst = 0.0;
    let mean_y = yv.iter().sum::<f64>() / m as f64;
    for (r, &yy) in xmat.iter().zip(yv.iter()) {
        let fit: f64 = r.iter().zip(coef.iter()).map(|(a, b)| a * b).sum();
        sse += (yy - fit).powi(2);
        sst += (yy - mean_y).powi(2);
    }
    let df_resid = (m - cols) as f64;
    let s2 = sse / df_resid;
    let se: Vec<f64> = (0..cols)
        .map(|a| (s2 * inv[a][a]).max(0.0).sqrt())
        .collect();
    let tvals: Vec<f64> = (0..cols)
        .map(|a| {
            if se[a] > 0.0 {
                coef[a] / se[a]
            } else {
                f64::NAN
            }
        })
        .collect();
    let pvals: Vec<f64> = tvals
        .iter()
        .map(|t| {
            if t.is_finite() {
                t_p(*t, df_resid)
            } else {
                f64::NAN
            }
        })
        .collect();
    let r2 = if sst > 0.0 { 1.0 - sse / sst } else { 0.0 };
    let adj = 1.0 - (1.0 - r2) * (m as f64 - 1.0) / df_resid;
    let df_model = p as f64;
    let ms_model = if df_model > 0.0 {
        (sst - sse).max(0.0) / df_model
    } else {
        0.0
    };
    let f = if s2 > 0.0 {
        ms_model / s2
    } else {
        f64::INFINITY
    };
    let mut names = vec!["截距".to_string()];
    names.extend(xs.iter().map(|(n, _)| n.clone()));
    Some(Ols {
        names,
        coef,
        se,
        t: tvals,
        p: pvals,
        r2,
        adj_r2: adj,
        f,
        f_p: f_sf(f, df_model, df_resid),
        n: m,
        df_resid,
        sse,
    })
}

// ---------------------------------------------------------------------------
// 四格表：OR / RR / McNemar
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct RatioEffect {
    pub name: &'static str,
    pub est: f64,
    pub lo: f64,
    pub hi: f64,
    pub log_est: f64,
    pub se: f64,
}

/// 2×2 表：a=暴露且事件，b=暴露无事件，c=非暴露且事件，d=非暴露无事件。
///
/// OR = ad/bc；RR = [a/(a+b)] / [c/(c+d)]。
/// 置信区间走对数尺度（Woolf / Katz），含 0 格时按惯例加 0.5 校正。
pub fn odds_ratio(a: f64, b: f64, c: f64, d: f64, conf: f64) -> Option<(RatioEffect, RatioEffect)> {
    let (mut a, mut b, mut c, mut d) = (a, b, c, d);
    if [a, b, c, d].iter().any(|v| *v < 0.0) {
        return None;
    }
    if a == 0.0 || b == 0.0 || c == 0.0 || d == 0.0 {
        a += 0.5;
        b += 0.5;
        c += 0.5;
        d += 0.5;
    }
    let or = a * d / (b * c);
    let se_or = (1.0 / a + 1.0 / b + 1.0 / c + 1.0 / d).sqrt();
    let (r1, r2) = (a / (a + b), c / (c + d));
    if !(r2 > 0.0) {
        return None;
    }
    let rr = r1 / r2;
    let se_rr = (1.0 / a - 1.0 / (a + b) + 1.0 / c - 1.0 / (c + d))
        .max(0.0)
        .sqrt();
    let z = norm_ppf_local(0.5 + conf / 2.0);
    let mk = |name: &'static str, est: f64, se: f64| RatioEffect {
        name,
        est,
        lo: (est.ln() - z * se).exp(),
        hi: (est.ln() + z * se).exp(),
        log_est: est.ln(),
        se,
    };
    Some((mk("OR", or, se_or), mk("RR", rr, se_rr)))
}

#[derive(Clone, Debug)]
pub struct Mcnemar {
    /// b: 配对 1 阳 / 2 阴；c: 配对 1 阴 / 2 阳
    pub b: f64,
    pub c: f64,
    /// 精确二项检验的双侧 p
    pub p_exact: f64,
    /// 连续性校正 χ²
    pub chi2_cc: f64,
    pub p_chi2: f64,
    pub or: f64,
}

/// McNemar（配对二分类）：只关心不一致对 b、c。
pub fn mcnemar(b: f64, c: f64) -> Option<Mcnemar> {
    if b < 0.0 || c < 0.0 || b + c == 0.0 {
        return None;
    }
    let n = b + c;
    let k = b.min(c);
    // 精确：P = 2 · Σ_{i≤k} C(n,i) 0.5^n，封顶 1
    let mut tail = 0.0;
    let mut logc = 0.0;
    for i in 0..=(k as usize) {
        logc = if i == 0 {
            0.0
        } else {
            logc + ((n as usize - i + 1) as f64).ln() - (i as f64).ln()
        };
        tail += logc.exp() * 0.5f64.powf(n);
    }
    let p_exact = (2.0 * tail).min(1.0);
    let chi2_cc = ((b - c).abs() - 1.0).max(0.0).powi(2) / n;
    Some(Mcnemar {
        b,
        c,
        p_exact,
        chi2_cc,
        p_chi2: chi2_sf(chi2_cc, 1.0),
        or: if c > 0.0 { b / c } else { f64::INFINITY },
    })
}

/// 标准正态分位数（Acklam 近似，精度 ~1e-9）。
fn norm_ppf_local(p: f64) -> f64 {
    let p = p.clamp(1e-12, 1.0 - 1e-12);
    let a = [
        -3.969683028665376e+01,
        2.209460984245205e+02,
        -2.759285104469687e+02,
        1.383577518672690e+02,
        -3.066479806614716e+01,
        2.506628277459239e+00,
    ];
    let b = [
        -5.447609879822406e+01,
        1.615858368580409e+02,
        -1.556989798598866e+02,
        6.680131188771972e+01,
        -1.328068155288572e+01,
    ];
    let c = [
        -7.784894002430293e-03,
        -3.223964580411365e-01,
        -2.400758277161838e+00,
        -2.549732539343734e+00,
        4.374664141464968e+00,
        2.938163982698783e+00,
    ];
    let d = [
        7.784695709041462e-03,
        3.224671290700398e-01,
        2.445134137142996e+00,
        3.754408661907416e+00,
    ];
    let pl = 0.02425;
    if p < pl {
        let q = (-2.0 * p.ln()).sqrt();
        (((((c[0] * q + c[1]) * q + c[2]) * q + c[3]) * q + c[4]) * q + c[5])
            / ((((d[0] * q + d[1]) * q + d[2]) * q + d[3]) * q + 1.0)
    } else if p <= 1.0 - pl {
        let q = p - 0.5;
        let r = q * q;
        (((((a[0] * r + a[1]) * r + a[2]) * r + a[3]) * r + a[4]) * r + a[5]) * q
            / (((((b[0] * r + b[1]) * r + b[2]) * r + b[3]) * r + b[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((c[0] * q + c[1]) * q + c[2]) * q + c[3]) * q + c[4]) * q + c[5])
            / ((((d[0] * q + d[1]) * q + d[2]) * q + d[3]) * q + 1.0)
    }
}

// ---------------------------------------------------------------------------
// Cox 比例风险
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct CoxVar {
    pub name: String,
    pub beta: f64,
    pub hr: f64,
    pub se: f64,
    pub z: f64,
    pub p: f64,
    pub ci_lo: f64,
    pub ci_hi: f64,
}

#[derive(Clone, Debug)]
pub struct Cox {
    pub vars: Vec<CoxVar>,
    pub loglik: f64,
    pub n_events: usize,
    pub n: usize,
    pub converged: bool,
    /// 数据完全分离：β 发散、SE 不可估（HR 无界）
    pub separated: bool,
}

/// Cox 比例风险回归（Breslow 处理并列，Newton–Raphson）。
///
/// 偏似然：L = Π_i [exp(x_iβ) / Σ_{j∈R(t_i)} exp(x_jβ)]^{d_i}。
/// 得分与信息矩阵都按风险集累加；解 I·δ = U，步长 < 1e-8 判收敛。
///
/// **信息矩阵必须用同一份 β 的最后一轮结果**：用错 β 重算会给出错误的标准误，
/// 这类错误 p 值看起来「正常」，最难发现。
pub fn cox_ph(time: &[f64], event: &[f64], cols: &[(String, Vec<f64>)]) -> Option<Cox> {
    let k = cols.len();
    if k == 0 {
        return None;
    }
    let n = time.len();
    let idx: Vec<usize> = (0..n)
        .filter(|&i| {
            time[i].is_finite()
                && event.get(i).copied().unwrap_or(1.0).is_finite()
                && cols
                    .iter()
                    .all(|(_, v)| v.get(i).map(|x| x.is_finite()).unwrap_or(false))
        })
        .collect();
    if idx.len() <= k {
        return None;
    }
    let tt: Vec<f64> = idx.iter().map(|&i| time[i]).collect();
    let ev: Vec<f64> = idx
        .iter()
        .map(|&i| {
            if event.get(i).copied().unwrap_or(1.0) != 0.0 {
                1.0
            } else {
                0.0
            }
        })
        .collect();
    let n_events = ev.iter().filter(|&&e| e > 0.0).count();
    if n_events == 0 {
        return None;
    }
    let m = idx.len();
    let x: Vec<Vec<f64>> = cols
        .iter()
        .map(|(_, v)| idx.iter().map(|&i| v[i]).collect())
        .collect();
    let mut beta = vec![0.0; k];
    let mut info = vec![vec![0.0; k]; k];
    let mut loglik = 0.0;
    let mut converged = false;

    for _ in 0..100 {
        // 风险集：按时间降序累加，R(t_i) = {j : t_j ≥ t_i}
        let mut order: Vec<usize> = (0..m).collect();
        order.sort_by(|&a, &b| {
            tt[b]
                .partial_cmp(&tt[a])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let e: Vec<f64> = (0..m)
            .map(|i| (0..k).map(|c| x[c][i] * beta[c]).sum::<f64>().exp())
            .collect();
        let mut s0 = vec![0.0; m];
        let mut s1 = vec![vec![0.0; k]; m];
        let mut s2 = vec![vec![vec![0.0; k]; k]; m];
        let mut a0 = 0.0;
        let mut a1 = vec![0.0; k];
        let mut a2 = vec![vec![0.0; k]; k];
        for &i in &order {
            a0 += e[i];
            for c in 0..k {
                a1[c] += x[c][i] * e[i];
            }
            for c in 0..k {
                for d in 0..k {
                    a2[c][d] += x[c][i] * x[d][i] * e[i];
                }
            }
            s0[i] = a0;
            s1[i] = a1.clone();
            for c in 0..k {
                s2[i][c].copy_from_slice(&a2[c]);
            }
        }
        loglik = 0.0;
        let mut u = vec![0.0; k];
        info = vec![vec![0.0; k]; k];
        for i in 0..m {
            if ev[i] == 0.0 || s0[i] <= 0.0 {
                continue;
            }
            let eta: f64 = (0..k).map(|c| x[c][i] * beta[c]).sum();
            loglik += eta - s0[i].ln();
            let mean: Vec<f64> = (0..k).map(|c| s1[i][c] / s0[i]).collect();
            for c in 0..k {
                u[c] += x[c][i] - mean[c];
            }
            for c in 0..k {
                for d in 0..k {
                    info[c][d] += s2[i][c][d] / s0[i] - mean[c] * mean[d];
                }
            }
        }
        let delta = solve(&info, &u)
            .unwrap_or_else(|| (0..k).map(|c| u[c] / info[c][c].abs().max(1e-9)).collect());
        for c in 0..k {
            beta[c] += delta[c] * 0.9; // 轻微阻尼，防止极端数据下震荡
        }
        if delta.iter().map(|v| v * v).sum::<f64>().sqrt() < 1e-8 {
            converged = true;
            break;
        }
    }

    // 完全分离（某组的风险全落在同一时段）时 β 会发散、信息矩阵趋于奇异。
    // 这时**照实给出结果并把 SE 留空**，而不是假装收敛：
    // 「HR 无界」本身就是结论，不能用一个漂亮的假数字盖过去。
    let inv = invert(&info);
    let separated = !converged
        || beta.iter().any(|b| b.abs() > 10.0)
        || inv
            .as_ref()
            .map(|i| (0..k).any(|c| !(i[c][c] > 0.0)))
            .unwrap_or(true);
    let z975 = 1.959963984540054;
    let mut vars = Vec::with_capacity(k);
    for c in 0..k {
        let se = match &inv {
            Some(i) if i[c][c] > 0.0 => i[c][c].sqrt(),
            _ => f64::NAN,
        };
        let b = beta[c];
        let z = if se.is_finite() && se > 0.0 {
            b / se
        } else {
            f64::NAN
        };
        vars.push(CoxVar {
            name: cols[c].0.clone(),
            beta: b,
            hr: b.exp(),
            se,
            z,
            p: if z.is_finite() {
                2.0 * (1.0 - norm_cdf(z.abs()))
            } else {
                f64::NAN
            },
            ci_lo: (b - z975 * se).exp(),
            ci_hi: (b + z975 * se).exp(),
        });
    }
    Some(Cox {
        vars,
        loglik,
        n_events,
        n: m,
        converged,
        separated,
    })
}

// ---------------------------------------------------------------------------
// 线性代数
// ---------------------------------------------------------------------------

/// 高斯消元（列主元）解 A·x = b；奇异返回 None。
pub fn solve(a: &[Vec<f64>], b: &[f64]) -> Option<Vec<f64>> {
    let n = b.len();
    if a.len() != n || a.iter().any(|r| r.len() != n) {
        return None;
    }
    let mut m: Vec<Vec<f64>> = a.to_vec();
    let mut rhs = b.to_vec();
    for c in 0..n {
        let mut piv = c;
        for r in (c + 1)..n {
            if m[r][c].abs() > m[piv][c].abs() {
                piv = r;
            }
        }
        if m[piv][c].abs() < 1e-12 {
            return None;
        }
        m.swap(c, piv);
        rhs.swap(c, piv);
        let d = m[c][c];
        for j in c..n {
            m[c][j] /= d;
        }
        rhs[c] /= d;
        for r in 0..n {
            if r == c {
                continue;
            }
            let f = m[r][c];
            if f == 0.0 {
                continue;
            }
            for j in c..n {
                m[r][j] -= f * m[c][j];
            }
            rhs[r] -= f * rhs[c];
        }
    }
    Some(rhs)
}

/// 矩阵求逆（Gauss–Jordan）；奇异返回 None。
pub fn invert(a: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = a.len();
    if n == 0 || a.iter().any(|r| r.len() != n) {
        return None;
    }
    let mut m: Vec<Vec<f64>> = a.to_vec();
    let mut inv = vec![vec![0.0; n]; n];
    for i in 0..n {
        inv[i][i] = 1.0;
    }
    for c in 0..n {
        let mut piv = c;
        for r in (c + 1)..n {
            if m[r][c].abs() > m[piv][c].abs() {
                piv = r;
            }
        }
        if m[piv][c].abs() < 1e-12 {
            return None;
        }
        m.swap(c, piv);
        inv.swap(c, piv);
        let d = m[c][c];
        for j in 0..n {
            m[c][j] /= d;
            inv[c][j] /= d;
        }
        for r in 0..n {
            if r == c {
                continue;
            }
            let f = m[r][c];
            if f == 0.0 {
                continue;
            }
            for j in 0..n {
                m[r][j] -= f * m[c][j];
                inv[r][j] -= f * inv[c][j];
            }
        }
    }
    Some(inv)
}

// ---------------------------------------------------------------------------
// 补充：符号秩（vs μ₀）/ F 方差比 / Kappa / Bland–Altman
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct SignedRank {
    pub n: usize,
    /// 正差值的秩和 W⁺
    pub w: f64,
    pub z: f64,
    pub p: f64,
    /// 与 μ₀ 相等、按惯例剔除的观测数
    pub zero_dropped: usize,
}

/// Wilcoxon 符号秩（单样本，H₀: 中位数 = μ₀）。
///
/// 正态近似 + 连续性校正 + 并列方差校正。n < 6 不给结果：
/// 精确分布表未实现， approximations 会明显偏，宁可不做。
pub fn signed_rank_one(v: &[f64], mu0: f64) -> Option<SignedRank> {
    let all: Vec<f64> = v.iter().copied().filter(|x| x.is_finite()).collect();
    let zero_dropped = all.iter().filter(|x| (**x - mu0).abs() < 1e-12).count();
    let d: Vec<f64> = all
        .into_iter()
        .map(|x| x - mu0)
        .filter(|x| x.abs() >= 1e-12)
        .collect();
    if d.len() < 6 {
        return None;
    }
    let abs: Vec<f64> = d.iter().map(|x| x.abs()).collect();
    let r = ranks(&abs);
    let w: f64 = r
        .iter()
        .zip(d.iter())
        .filter(|(_, x)| **x > 0.0)
        .map(|(ri, _)| *ri)
        .sum();
    let n = d.len() as f64;
    let mean = n * (n + 1.0) / 4.0;
    // 并列校正：Σ(t³−t)/48 从方差里扣
    let mut sorted = abs.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut tie = 0.0;
    let mut i = 0;
    while i < sorted.len() {
        let mut j = i;
        while j + 1 < sorted.len() && (sorted[j + 1] - sorted[i]).abs() < 1e-12 {
            j += 1;
        }
        let t = (j - i + 1) as f64;
        if t > 1.0 {
            tie += t.powi(3) - t;
        }
        i = j + 1;
    }
    let var = n * (n + 1.0) * (2.0 * n + 1.0) / 24.0 - tie / 48.0;
    if !(var > 0.0) {
        return None;
    }
    let diff = w - mean;
    let cc = 0.5 * diff.signum();
    let z = (diff - cc) / var.sqrt();
    Some(SignedRank {
        n: d.len(),
        w,
        z,
        p: (2.0 * (1.0 - norm_cdf(z.abs()))).clamp(0.0, 1.0),
        zero_dropped,
    })
}

#[derive(Clone, Debug)]
pub struct FVar {
    pub f: f64,
    pub df1: f64,
    pub df2: f64,
    pub p: f64,
    pub sd1: f64,
    pub sd2: f64,
}

/// 两样本 F 方差比检验（双侧）：F = s₁²/s₂²。
///
/// 双侧 p = 2·min(P(F ≥ f), P(F ≤ f))，封顶 1。
pub fn f_variance(a: &[f64], b: &[f64]) -> Option<FVar> {
    if a.len() < 2 || b.len() < 2 {
        return None;
    }
    let v1 = crate::core::stats::variance(a);
    let v2 = crate::core::stats::variance(b);
    if !(v1 > 0.0) || !(v2 > 0.0) {
        return None;
    }
    let f = v1 / v2;
    let (df1, df2) = ((a.len() - 1) as f64, (b.len() - 1) as f64);
    let upper = f_sf(f, df1, df2);
    let p = (2.0 * upper.min(1.0 - upper)).min(1.0);
    Some(FVar {
        f,
        df1,
        df2,
        p,
        sd1: v1.sqrt(),
        sd2: v2.sqrt(),
    })
}

#[derive(Clone, Debug)]
pub struct Kappa {
    pub po: f64,
    pub pe: f64,
    pub k: f64,
    pub se: f64,
    pub z: f64,
    pub p: f64,
}

/// Cohen's Kappa（2×2 一致性）：κ = (Po − Pe)/(1 − Pe)。
///
/// 判读惯例：<0 差、0–0.2 轻微、0.2–0.4 一般、0.4–0.6 中等、0.6–0.8 好、>0.8 很好。
pub fn cohen_kappa(a: f64, b: f64, c: f64, d: f64) -> Option<Kappa> {
    let n = a + b + c + d;
    if n <= 0.0 || [a, b, c, d].iter().any(|v| *v < 0.0) {
        return None;
    }
    let po = (a + d) / n;
    let pe = ((a + b) * (a + c) + (c + d) * (b + d)) / (n * n);
    if !(pe < 1.0) {
        return None;
    }
    let k = (po - pe) / (1.0 - pe);
    let se = (po * (1.0 - po) / (n * (1.0 - pe).powi(2))).sqrt();
    let z = if se > 0.0 { k / se } else { f64::NAN };
    Some(Kappa {
        po,
        pe,
        k,
        se,
        z,
        p: if z.is_finite() {
            (2.0 * (1.0 - norm_cdf(z.abs()))).clamp(0.0, 1.0)
        } else {
            f64::NAN
        },
    })
}

#[derive(Clone, Debug)]
pub struct BlandAltman {
    pub n: usize,
    pub mean_diff: f64,
    pub sd_diff: f64,
    /// 一致性界限下/上限（mean ∓ 1.96·SD）
    pub loa_lo: f64,
    pub loa_hi: f64,
    pub se_mean: f64,
}

/// Bland–Altman 一致性分析：两次测量的差值 vs 均值。
pub fn bland_altman(a: &[f64], b: &[f64]) -> Option<BlandAltman> {
    let n = a.len().min(b.len());
    let diffs: Vec<f64> = (0..n)
        .map(|i| a[i] - b[i])
        .filter(|d| d.is_finite())
        .collect();
    let m = diffs.len();
    if m < 3 {
        return None;
    }
    let mean_diff = diffs.iter().sum::<f64>() / m as f64;
    let sd_diff = crate::core::stats::sd(&diffs);
    if !(sd_diff > 0.0) {
        return None;
    }
    Some(BlandAltman {
        n: m,
        mean_diff,
        sd_diff,
        loa_lo: mean_diff - 1.96 * sd_diff,
        loa_hi: mean_diff + 1.96 * sd_diff,
        se_mean: sd_diff / (m as f64).sqrt(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn 卡方上尾对上教科书值() {
        // χ²(1) 0.95 分位 = 3.841 → 上尾 0.05
        assert!(
            close(chi2_sf(3.841, 1.0), 0.05, 2e-3),
            "{}",
            chi2_sf(3.841, 1.0)
        );
        // χ²(2) 的中位数 = 2·ln2 ≈ 1.386 → 上尾 0.5
        assert!(close(chi2_sf(1.3863, 2.0), 0.5, 5e-3));
    }

    #[test]
    fn 学生化极差临界值与自由度一致() {
        // 自洽性：q_crit 的 CDF 必须等于目标置信度
        let q = q_crit(0.95, 3, 12.0);
        assert!(q > 3.0 && q < 5.5, "q={q}");
        assert!(close(q_cdf(q, 3, 12.0), 0.95, 1e-3));
        // k 越大临界值越大（多重比较越保守）
        assert!(q_crit(0.95, 6, 12.0) > q_crit(0.95, 3, 12.0));
    }

    #[test]
    fn tukey与games_howell方向一致且ci含差值() {
        let g = vec![
            vec![10.0, 11.0, 10.5, 10.2, 10.8],
            vec![12.0, 12.5, 11.8, 12.2, 12.4],
            vec![10.1, 9.9, 10.4, 10.0, 10.3],
        ];
        let tk = tukey_hsd(&g, 0.95).unwrap();
        assert_eq!(tk.len(), 3);
        // 1 vs 2 差异大，p 应显著
        let p12 = tk.iter().find(|p| p.a == 0 && p.b == 1).unwrap();
        assert!(p12.p < 0.01, "p={}", p12.p);
        assert!(p12.ci_lo <= p12.diff && p12.diff <= p12.ci_hi);
        // 0 vs 2 差异小，p 不显著
        let p02 = tk.iter().find(|p| p.a == 0 && p.b == 2).unwrap();
        assert!(p02.p > 0.05, "p={}", p02.p);
        let gh = games_howell(&g, 0.95).unwrap();
        let q12 = gh.iter().find(|p| p.a == 0 && p.b == 1).unwrap();
        assert!(q12.p < 0.01);
    }

    #[test]
    fn 方差齐性检验能分辨同方差与异方差() {
        let same = vec![
            vec![1.0, 2.0, 3.0, 4.0, 5.0],
            vec![2.0, 3.0, 4.0, 5.0, 6.0],
            vec![0.5, 1.5, 2.5, 3.5, 4.5],
        ];
        let het = vec![
            vec![1.0, 2.0, 3.0, 4.0, 5.0],
            vec![10.0, 20.0, 30.0, 40.0, 50.0],
            vec![0.5, 1.5, 2.5, 3.5, 4.5],
        ];
        assert!(bartlett(&same).unwrap().p > 0.05);
        assert!(bartlett(&het).unwrap().p < 0.01);
        assert!(levene(&same, false).unwrap().p > 0.05);
        assert!(levene(&het, false).unwrap().p < 0.05);
        assert!(levene(&het, true).unwrap().p < 0.05);
    }

    #[test]
    fn friedman手算基准() {
        // 3 个区组、3 个处理，每行秩都是 [1,2,3]：
        // Q = 12/(3·3·4)·(3²+6²+9²) − 3·3·4 = 6，df = 2
        let blocks = vec![
            vec![1.0, 2.0, 3.0],
            vec![1.0, 2.0, 3.0],
            vec![1.0, 2.0, 3.0],
        ];
        let r = friedman(&blocks).unwrap();
        assert!(close(r.q, 6.0, 1e-9), "Q={}", r.q);
        assert_eq!(r.df, 2.0);
        assert!(close(r.p, 0.049787, 1e-4), "p={}", r.p);
        assert_eq!(r.mean_ranks, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn 单样本t对上直接公式() {
        let v = vec![12.0, 14.0, 13.0, 15.0, 11.0];
        let r = one_sample(&v, 10.0, 0.95).unwrap();
        let m = 13.0;
        let sd = 1.5811388300841898;
        let t = (m - 10.0) / (sd / 5f64.sqrt());
        assert!(close(r.mean, m, 1e-12));
        assert!(close(r.t, t, 1e-9), "t={} vs {}", r.t, t);
        assert!(r.p < 0.02 && r.p > 0.001);
        assert!(r.ci_lo < m && r.ci_hi > m);
    }

    #[test]
    fn grubbs抓得到明显离群点() {
        let mut v: Vec<f64> = (1..=10).map(|i| i as f64).collect();
        v.push(60.0);
        let g = grubbs(&v, 0.05).unwrap();
        assert_eq!(g.value, 60.0);
        assert!(g.g > g.critical, "G={} crit={}", g.g, g.critical);
        let clean: Vec<f64> = (1..=11).map(|i| i as f64).collect();
        let g2 = grubbs(&clean, 0.05).unwrap();
        assert!(g2.g < g2.critical, "均匀数据不应判离群");
    }

    #[test]
    fn deming还原已知直线() {
        let x = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let y: Vec<f64> = x.iter().map(|v| 2.0 * v + 1.0).collect();
        let d = deming(&x, &y, 1.0).unwrap();
        assert!(close(d.slope, 2.0, 1e-9), "slope={}", d.slope);
        assert!(close(d.intercept, 1.0, 1e-9));
    }

    #[test]
    fn 斜率比较能分辨平行与交叉() {
        let x1 = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        // 带一点残差，否则两组的残差平方和都是 0、SE=0（无误差可估）
        let noise = [0.05, -0.04, 0.03, -0.02, 0.04, -0.05, 0.02, -0.03];
        let y1: Vec<f64> = x1
            .iter()
            .zip(noise.iter())
            .map(|(v, e)| 2.0 * v + e)
            .collect();
        let x2 = x1.clone();
        let y2: Vec<f64> = x2
            .iter()
            .zip(noise.iter().rev())
            .map(|(v, e)| 2.0 * v + 5.0 + e)
            .collect();
        let same = compare_slopes(&x1, &y1, &x2, &y2).unwrap();
        assert!(same.diff.abs() < 0.02, "diff={}", same.diff);
        assert!(same.p > 0.05, "平行线不该显著：p={}", same.p);

        let y3: Vec<f64> = x2
            .iter()
            .zip(noise.iter())
            .map(|(v, e)| 3.5 * v + e)
            .collect();
        let diff = compare_slopes(&x1, &y1, &x2, &y3).unwrap();
        assert!(close(diff.diff, -1.5, 0.02), "diff={}", diff.diff);
        assert!(diff.p < 0.001, "p={}", diff.p);
    }

    #[test]
    fn pca第一主成分解释完全相关() {
        let a: Vec<f64> = (1..=10).map(|i| i as f64).collect();
        let b: Vec<f64> = a.iter().map(|v| 3.0 * v + 2.0).collect();
        let p = pca(&[a, b], true).unwrap();
        assert!(close(p.proportion[0], 1.0, 1e-6), "p1={}", p.proportion[0]);
        assert!(close(p.cumulative[1], 1.0, 1e-6));
        // 载荷同号（正相关）
        assert!(p.loadings[0][0] * p.loadings[1][0] > 0.0);
    }

    #[test]
    fn 聚类能分开两团() {
        let a = vec![1.0, 1.1, 0.9, 1.05, 10.0, 10.2, 9.9, 10.1];
        let b = vec![2.0, 2.1, 1.9, 2.05, 20.0, 20.2, 19.9, 20.1];
        let h = hclust(&[a.clone(), b.clone()], 2).unwrap();
        assert_eq!(h.k, 2);
        let first = h.cluster[0];
        assert!(h.cluster[..4].iter().all(|c| *c == first));
        assert!(h.cluster[4..].iter().all(|c| *c != first));
        let km = kmeans(&[a, b], 2, 42).unwrap();
        let f = km.cluster[0];
        assert!(km.cluster[..4].iter().all(|c| *c == f));
        assert!(km.cluster[4..].iter().all(|c| *c != f));
        assert!(km.wss.is_finite());
    }

    #[test]
    fn 四格表效应量() {
        // a=20 b=10 c=5 d=25 → OR = 20·25/(10·5) = 10，RR = (20/30)/(5/30) = 4
        let (or, rr) = odds_ratio(20.0, 10.0, 5.0, 25.0, 0.95).unwrap();
        assert!(close(or.est, 10.0, 1e-9), "OR={}", or.est);
        assert!(close(rr.est, 4.0, 1e-9), "RR={}", rr.est);
        assert!(or.lo < or.est && or.hi > or.est);
        assert!(rr.lo < rr.est && rr.hi > rr.est);
        // 零格：加 0.5 后仍给出有限估计，而不是 ±∞
        let (or2, _) = odds_ratio(0.0, 10.0, 5.0, 25.0, 0.95).unwrap();
        assert!(or2.est.is_finite() && or2.est > 0.0);
    }

    #[test]
    fn mcnemar精确检验() {
        // b=10, c=2 → 2·P(X≤2 | n=12, p=.5) = 2·79/4096 ≈ 0.03857
        let r = mcnemar(10.0, 2.0).unwrap();
        assert!(close(r.p_exact, 0.038574, 1e-5), "p={}", r.p_exact);
        assert!(r.p_chi2 < 0.05);
        // b=c 时不显著
        assert!(mcnemar(5.0, 5.0).unwrap().p_exact > 0.99);
    }

    #[test]
    fn ols还原精确平面() {
        let x1 = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let x2 = vec![2.0, 1.0, 4.0, 3.0, 6.0, 5.0, 8.0, 7.0];
        let y: Vec<f64> = x1
            .iter()
            .zip(x2.iter())
            .map(|(a, b)| 1.0 + 2.0 * a + 3.0 * b)
            .collect();
        let r = ols(&y, &[("x1".to_string(), x1), ("x2".to_string(), x2)]).unwrap();
        assert!(close(r.coef[0], 1.0, 1e-8), "b0={}", r.coef[0]);
        assert!(close(r.coef[1], 2.0, 1e-8));
        assert!(close(r.coef[2], 3.0, 1e-8));
        assert!(close(r.r2, 1.0, 1e-10), "R²={}", r.r2);
    }

    #[test]
    fn cox风险因素方向正确() {
        // 时间交错（有重叠随访）→ MLE 有限；处理组整体更早发生事件 → HR > 1
        let time = vec![1.0, 3.0, 5.0, 7.0, 2.0, 4.0, 6.0, 9.0];
        let event = vec![1.0; 8];
        let grp = vec![1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0];
        let r = cox_ph(&time, &event, &[("组".to_string(), grp)]).unwrap();
        assert!(r.vars[0].hr > 1.0, "HR={}", r.vars[0].hr);
        assert!(r.vars[0].se.is_finite(), "SE 应可估");
        assert!(r.vars[0].p.is_finite());
        assert!(r.converged && !r.separated, "应正常收敛");
    }

    #[test]
    fn cox完全分离时如实报告无界() {
        // 处理组的事件全部早于对照组 → β 发散；不能伪装成有限 HR
        let time = vec![1.0, 2.0, 3.0, 4.0, 8.0, 9.0, 10.0, 12.0];
        let event = vec![1.0; 8];
        let grp = vec![1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0];
        let r = cox_ph(&time, &event, &[("组".to_string(), grp)]).unwrap();
        assert!(r.separated, "应识别为完全分离");
        assert!(r.vars[0].se.is_nan(), "分离时 SE 必须留空而不是编一个数");
        assert!(r.vars[0].hr > 1.0);
    }

    #[test]
    fn 线性代数基本性质() {
        let a = vec![vec![4.0, 7.0], vec![2.0, 6.0]];
        let inv = invert(&a).unwrap();
        // A·A⁻¹ = I
        for i in 0..2 {
            for j in 0..2 {
                let v = a[i][0] * inv[0][j] + a[i][1] * inv[1][j];
                assert!(close(v, if i == j { 1.0 } else { 0.0 }, 1e-12));
            }
        }
        let x = solve(&a, &[1.0, 2.0]).unwrap();
        assert!(close(4.0 * x[0] + 7.0 * x[1], 1.0, 1e-12));
        assert!(invert(&[vec![0.0, 0.0], vec![0.0, 0.0]]).is_none());
    }

    #[test]
    fn 符号秩对系统偏移敏感() {
        // 全部为正差 → W⁺ 接近最大，p 很小
        let v: Vec<f64> = (1..=10).map(|i| i as f64 + 5.0).collect();
        let r = signed_rank_one(&v, 5.0).unwrap();
        assert!(r.w > 40.0, "W={}", r.w);
        assert!(r.p < 0.01, "p={}", r.p);
        // 一半正一半负 → 不显著
        let v2 = vec![-4.0, -3.0, -2.0, -1.0, 1.0, 2.0, 3.0, 4.0, 5.0, -5.0];
        assert!(signed_rank_one(&v2, 0.0).unwrap().p > 0.5);
        // 与 μ₀ 相等的值被剔除并计数
        let v3 = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        assert_eq!(signed_rank_one(&v3, 0.0).unwrap().zero_dropped, 1);
    }

    #[test]
    fn f方差比与方向一致() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        let r = f_variance(&a, &b).unwrap();
        assert!(close(r.f, 0.01, 1e-9), "F={}", r.f);
        assert!(r.p < 0.05, "p={}", r.p);
        // 方差比互换 → F 取倒数、p 相同
        let r2 = f_variance(&b, &a).unwrap();
        assert!(close(r.f * r2.f, 1.0, 1e-9));
        assert!(close(r.p, r2.p, 1e-9));
    }

    #[test]
    fn kappa对上教科书值() {
        // 2×2 完全一致（b=c=0）→ κ = 1
        let k = cohen_kappa(20.0, 0.0, 0.0, 20.0).unwrap();
        assert!(close(k.k, 1.0, 1e-12));
        // 完全随机一致：a=b=c=d → Po=0.5, Pe=0.5 → κ=0
        let k0 = cohen_kappa(10.0, 10.0, 10.0, 10.0).unwrap();
        assert!(close(k0.k, 0.0, 1e-12), "κ={}", k0.k);
        // 手算：a=20,b=5,c=5,d=20 → Po=0.8, Pe=0.5 → κ=0.6
        let km = cohen_kappa(20.0, 5.0, 5.0, 20.0).unwrap();
        assert!(close(km.k, 0.6, 1e-12), "κ={}", km.k);
        assert!(km.p < 0.01);
    }

    #[test]
    fn bland_altman给出1_96倍sd的界限() {
        let a = vec![10.0, 12.0, 14.0, 16.0, 18.0, 20.0];
        let b = vec![9.5, 12.5, 13.0, 16.8, 17.2, 20.5];
        let r = bland_altman(&a, &b).unwrap();
        let diffs: Vec<f64> = a.iter().zip(b.iter()).map(|(x, y)| x - y).collect();
        let m = diffs.iter().sum::<f64>() / 6.0;
        let sd = crate::core::stats::sd(&diffs);
        assert!(close(r.mean_diff, m, 1e-12));
        assert!(close(r.sd_diff, sd, 1e-12));
        assert!(close(r.loa_lo, m - 1.96 * sd, 1e-12));
        assert!(close(r.loa_hi, m + 1.96 * sd, 1e-12));
        assert!(r.loa_lo < r.mean_diff && r.mean_diff < r.loa_hi);
        // 差值完全相同时 SD=0，无法定义界限 → None
        assert!(bland_altman(&a, &a.iter().map(|v| v + 1.0).collect::<Vec<_>>()).is_none());
    }
}
