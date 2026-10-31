//! 统计分析：真实计算，不再返回硬编码的假数字。
//!
//! 全部为纯 Rust 实现，无外部依赖：
//! - 描述统计：n / 均值 / SD / SEM / 中位数 / 最小 / 最大 / CV
//! - Welch 与合并方差两种独立样本 t 检验（含自由度、t、双尾 p）
//! - 单因素方差分析（F、两个自由度、p）
//! - 一元线性回归（斜率、截距、r、r²、斜率的标准误与 p）
//!
//! p 值由不完全 Beta 函数（`betai`）精确计算，不做正态近似。
//! `betai` 用连分数展开（Numerical Recipes 的 `betacf`），在 0.5 附近收敛很快。

/// 描述统计结果。
#[derive(Clone, Debug)]
pub struct Descriptive {
    pub n: usize,
    pub mean: f64,
    pub sd: f64,
    pub sem: f64,
    pub median: f64,
    pub min: f64,
    pub max: f64,
    pub cv: f64, // 变异系数 SD/Mean（%）；mean=0 时为 NAN
    pub q1: f64,
    pub q3: f64,
    pub iqr: f64,
    pub geo_mean: f64,
    pub skewness: f64,
    pub kurtosis: f64,
    pub mean_ci_lo: f64,
    pub mean_ci_hi: f64,
    pub median_ci_lo: f64,
    pub median_ci_hi: f64,
}

pub fn describe(v: &[f64]) -> Descriptive {
    let x: Vec<f64> = v.iter().copied().filter(|z| z.is_finite()).collect();
    let n = x.len();
    if n == 0 {
        return Descriptive {
            n: 0,
            mean: 0.0,
            sd: 0.0,
            sem: 0.0,
            median: 0.0,
            min: 0.0,
            max: 0.0,
            cv: f64::NAN,
            q1: 0.0,
            q3: 0.0,
            iqr: 0.0,
            geo_mean: f64::NAN,
            skewness: f64::NAN,
            kurtosis: f64::NAN,
            mean_ci_lo: f64::NAN,
            mean_ci_hi: f64::NAN,
            median_ci_lo: f64::NAN,
            median_ci_hi: f64::NAN,
        };
    }
    let mean = x.iter().sum::<f64>() / n as f64;
    let var = if n < 2 {
        0.0
    } else {
        x.iter().map(|z| (z - mean).powi(2)).sum::<f64>() / (n - 1) as f64
    };
    let sd = var.sqrt();
    let mut ss = x.clone();
    ss.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    // 分位数：线性插值
    let q = |p: f64| -> f64 {
        if n == 1 {
            return ss[0];
        }
        let h = p * (n as f64 - 1.0);
        let lo = h.floor() as usize;
        let hi = (lo + 1).min(n - 1);
        ss[lo] + (h - lo as f64) * (ss[hi] - ss[lo])
    };
    let q1 = q(0.25);
    let median = q(0.5);
    let q3 = q(0.75);
    let sem = sd / (n as f64).sqrt();
    // 几何均值：存在 ≤0 则为 NAN
    let geo_mean = if x.iter().all(|z| *z > 0.0) {
        (x.iter().map(|z| z.ln()).sum::<f64>() / n as f64).exp()
    } else {
        f64::NAN
    };
    // 偏度/峰度：总体中心矩 mi=Σ(x−mean)^i/n
    let m2 = x.iter().map(|z| (z - mean).powi(2)).sum::<f64>() / n as f64;
    let m3 = x.iter().map(|z| (z - mean).powi(3)).sum::<f64>() / n as f64;
    let m4 = x.iter().map(|z| (z - mean).powi(4)).sum::<f64>() / n as f64;
    let skewness = if n <= 2 || !(sd > 0.0) || !(m2 > 0.0) {
        f64::NAN
    } else {
        let g1 = m3 / m2.powf(1.5);
        let c = ((n as f64) * (n as f64 - 1.0)).sqrt() / (n as f64 - 2.0);
        g1 * c
    };
    let kurtosis = if n <= 3 || !(sd > 0.0) || !(m2 > 0.0) {
        f64::NAN
    } else {
        let g2 = m4 / (m2 * m2) - 3.0;
        ((n as f64 - 1.0) * ((n as f64 + 1.0) * g2 + 6.0))
            / ((n as f64 - 2.0) * (n as f64 - 3.0))
    };
    // Mean 95% CI：n=1 无 CI
    let (mean_ci_lo, mean_ci_hi) = if n >= 2 && sem.is_finite() {
        let tc = t_crit_two(0.95, (n - 1) as f64);
        (mean - tc * sem, mean + tc * sem)
    } else {
        (f64::NAN, f64::NAN)
    };
    // Median 95% CI：基于二项尾概率的秩；n<2 为 NAN
    let (median_ci_lo, median_ci_hi) = median_ci_rank(&ss, 0.95);
    Descriptive {
        n,
        mean,
        sd,
        sem,
        median,
        min: ss[0],
        max: ss[n - 1],
        cv: if mean.abs() > 1e-12 {
            sd / mean.abs() * 100.0
        } else {
            f64::NAN
        },
        q1,
        q3,
        iqr: q3 - q1,
        geo_mean,
        skewness,
        kurtosis,
        mean_ci_lo,
        mean_ci_hi,
        median_ci_lo,
        median_ci_hi,
    }
}

/// 中位数 CI：基于精确二项尾概率的秩。正态近似只用于初始化。
fn median_ci_rank(sorted: &[f64], conf: f64) -> (f64, f64) {
    let n = sorted.len();
    if n < 2 {
        return (f64::NAN, f64::NAN);
    }
    let alpha = 1.0 - conf;
    // 正态近似给出初始秩
    let z = -norm_inv_local(alpha / 2.0);
    let c = (n as f64 * 0.5 - z * (n as f64).sqrt() * 0.5).round();
    // 用精确二项尾概率收紧：累积二项 P(X≤k) 与 P(X≥k)
    let mut lo = c as i64;
    let mut hi = (n as i64 - c as i64).max(lo);
    loop {
        if lo <= 0 {
            break;
        }
        if binom_cdf(lo - 1, n, 0.5) <= alpha / 2.0 {
            break;
        }
        lo -= 1;
    }
    loop {
        if hi >= n as i64 {
            break;
        }
        if binom_cdf(hi, n, 0.5) >= 1.0 - alpha / 2.0 {
            break;
        }
        hi += 1;
    }
    let get = |i: i64| -> f64 {
        let idx = i.clamp(0, n as i64 - 1) as usize;
        sorted[idx]
    };
    (get(lo), get(hi.min(n as i64 - 1)))
}

/// 二项累积分布 P(X≤k)
fn binom_cdf(k: i64, n: usize, p: f64) -> f64 {
    if k < 0 {
        return 0.0;
    }
    if k >= n as i64 {
        return 1.0;
    }
    let mut c = 0.0f64;
    for i in 0..=k as usize {
        c += binom_pmf(i, n, p);
    }
    c.min(1.0)
}

fn binom_pmf(i: usize, n: usize, p: f64) -> f64 {
    if i > n {
        return 0.0;
    }
    let ln_c = log_gamma(n as f64 + 1.0)
        - log_gamma(i as f64 + 1.0)
        - log_gamma((n - i) as f64 + 1.0);
    (ln_c + i as f64 * p.ln() + (n - i) as f64 * (1.0 - p).ln()).exp()
}

/// 标准正态分位数：二分 200 轮，区间 [-40,40]；p=0→-∞、p=1→∞
fn norm_inv_local(p: f64) -> f64 {
    if !(p > 0.0) {
        return f64::NEG_INFINITY;
    }
    if !(p < 1.0) {
        return f64::INFINITY;
    }
    let mut lo = -40.0f64;
    let mut hi = 40.0f64;
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if norm_cdf_local(mid) < p {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

// ── 分布函数 ────────────────────────────────────────────────────────────────

fn log_gamma(x: f64) -> f64 {
    // Lanczos 近似（g=7, n=9），双精度下误差 < 1e-15
    const G: [f64; 9] = [
        0.99999999999980993,
        676.5203681218851,
        -1259.1392167224028,
        771.32342877765313,
        -176.61502916214059,
        12.507343278686905,
        -0.13857109526572012,
        9.9843695780195716e-6,
        1.5056327351493116e-7,
    ];
    if x < 0.5 {
        // 反射公式
        return (std::f64::consts::PI / (std::f64::consts::PI * x).sin()).ln() - log_gamma(1.0 - x);
    }
    let x = x - 1.0;
    let mut a = G[0];
    let t = x + 7.5;
    for (i, g) in G.iter().enumerate().skip(1) {
        a += g / (x + i as f64);
    }
    0.5 * ((2.0 * std::f64::consts::PI).ln() - t.ln()) + t.ln() * (x + 0.5) + a.ln() - t
}

/// 不完全 Beta 函数的连分数部分（Numerical Recipes `betacf`）。
fn betacf(a: f64, b: f64, x: f64) -> f64 {
    const MAXIT: usize = 200;
    const EPS: f64 = 3.0e-12;
    const FPMIN: f64 = 1.0e-300;

    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < FPMIN {
        d = FPMIN;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..=MAXIT {
        let m2 = 2 * m;
        let aa = m as f64 * (b - m as f64) * x / ((qam + m2 as f64) * (a + m2 as f64));
        d = 1.0 + aa * d;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = 1.0 + aa / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        h *= d * c;
        let aa = -(a + m as f64) * (qab + m as f64) * x / ((a + m2 as f64) * (qap + m2 as f64));
        d = 1.0 + aa * d;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = 1.0 + aa / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < EPS {
            break;
        }
    }
    h
}

/// 正则化不完全 Beta 函数 I_x(a, b)。
pub fn betai(a: f64, b: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let bt = (log_gamma(a + b) - log_gamma(a) - log_gamma(b) + a * x.ln() + b * (1.0 - x).ln()).exp();
    if x < (a + 1.0) / (a + b + 2.0) {
        bt * betacf(a, b, x) / a
    } else {
        1.0 - bt * betacf(b, a, 1.0 - x) / b
    }
}

/// t 分布的双尾 p 值。
pub fn t_p_two_tailed(t: f64, df: f64) -> f64 {
    if !t.is_finite() || df <= 0.0 {
        return f64::NAN;
    }
    betai(df / 2.0, 0.5, df / (df + t * t))
}

/// F 分布的右尾 p 值。
pub fn f_p(f: f64, df1: f64, df2: f64) -> f64 {
    if !f.is_finite() || df1 <= 0.0 || df2 <= 0.0 {
        return f64::NAN;
    }
    if f <= 0.0 {
        return 1.0;
    }
    betai(df2 / 2.0, df1 / 2.0, df2 / (df2 + df1 * f))
}

/// 析光风格的 P 值摘要。
pub fn stars(p: f64) -> &'static str {
    if !p.is_finite() {
        return "n/a";
    }
    // 多数期刊的约定：`*` <0.05、`**` <0.01、`***` <0.001。
    // P < 0.0001 不再单独给 `****` —— 写 results 时 <0.001 已经是很强的证据，
    // 再细分没有意义，统一归到 `***`。
    if p < 0.001 {
        "***"
    } else if p < 0.01 {
        "**"
    } else if p < 0.05 {
        "*"
    } else {
        "ns"
    }
}

/// 格式化 P 值。格式由设置决定：
/// 0 = 固定小数位（位数取设置）、1 = 固定 3 位、2 = 小于 0.0001 时写 <0.0001、
/// 3 = 科学记数。
pub fn fmt_p(p: f64) -> String {
    if !p.is_finite() {
        return "n/a".to_string();
    }
    let st = crate::settings::get();
    let d = st.decimals.min(10);
    match st.p_format {
        1 => format!("{:.3}", p),
        2 => {
            if p < 10f64.powf(-(d as f64)) {
                format!("<{}", fmt_num(10f64.powf(-(d as f64)), d))
            } else {
                format!("{:.*}", d, p)
            }
        }
        3 => fmt_sci(p),
        _ => {
            if p < 0.0001 {
                "<0.0001".to_string()
            } else {
                format!("{:.*}", d.min(4), p)
            }
        }
    }
}

/// 科学记数，样式由设置决定（1.23e-4 / 1.23×10⁻⁴ / 1.23E-4）。
pub fn fmt_sci(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return format!("{:.*}", crate::settings::get().decimals.min(10), v);
    }
    let e = v.abs().log10().floor() as i32;
    let m = v / 10f64.powi(e);
    let d = crate::settings::get().decimals.min(6);
    let mantissa = format!("{:.*}", d, m);
    match crate::settings::get().sci_style {
        1 => {
            // 上标数字
            let sup = |c: char| -> char {
                match c {
                    '-' => '⁻',
                    '0' => '⁰',
                    '1' => '¹',
                    '2' => '²',
                    '3' => '³',
                    '4' => '⁴',
                    '5' => '⁵',
                    '6' => '⁶',
                    '7' => '⁷',
                    '8' => '⁸',
                    '9' => '⁹',
                    _ => c,
                }
            };
            let es: String = e.to_string().chars().map(sup).collect();
            format!("{}×10{}", mantissa, es)
        }
        2 => format!("{}E{}", mantissa, e),
        _ => format!("{}e{}", mantissa, e),
    }
}

/// 按设置的小数位输出数值。
pub fn fmt_num(v: f64, d: usize) -> String {
    let st = crate::settings::get();
    // 自动科学记数：绝对值过大或过小时改用科学记数
    if st.auto_sci && v != 0.0 {
        let a = v.abs();
        if a >= 1e6 || a < 1e-4 {
            return fmt_sci(v);
        }
    }
    format!("{:.*}", d.min(10), v)
}

// ── 检验 ────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TTestKind {
    /// 合并方差（等方差假设）
    Pooled,
    /// Welch（不假定等方差，默认推荐）
    Welch,
}

#[derive(Clone, Debug)]
pub struct TTestResult {
    pub a: String,
    pub b: String,
    pub mean_a: f64,
    pub mean_b: f64,
    pub diff: f64,
    pub se_diff: f64,
    pub t: f64,
    pub df: f64,
    pub p: f64,
    pub df_a: usize,
    pub df_b: usize,
}

/// 独立样本 t 检验。
pub fn t_test(a: &[f64], b: &[f64], kind: TTestKind) -> Option<TTestResult> {
    if a.len() < 2 || b.len() < 2 {
        return None;
    }
    let da = describe(a);
    let db = describe(b);
    let na = a.len() as f64;
    let nb = b.len() as f64;
    let (se, df) = match kind {
        TTestKind::Pooled => {
            let sp2 = ((na - 1.0) * da.sd.powi(2) + (nb - 1.0) * db.sd.powi(2)) / (na + nb - 2.0);
            ((sp2 * (1.0 / na + 1.0 / nb)).sqrt(), na + nb - 2.0)
        }
        TTestKind::Welch => {
            let va = da.sd.powi(2) / na;
            let vb = db.sd.powi(2) / nb;
            let se = (va + vb).sqrt();
            let df = (va + vb).powi(2)
                / (va.powi(2) / (na - 1.0) + vb.powi(2) / (nb - 1.0));
            (se, df)
        }
    };
    let diff = db.mean - da.mean;
    let t = if se > 0.0 { diff / se } else { 0.0 };
    Some(TTestResult {
        a: String::new(),
        b: String::new(),
        mean_a: da.mean,
        mean_b: db.mean,
        diff,
        se_diff: se,
        t,
        df,
        p: t_p_two_tailed(t, df),
        df_a: a.len(),
        df_b: b.len(),
    })
}

#[derive(Clone, Debug)]
pub struct AnovaResult {
    pub f: f64,
    pub df_between: f64,
    pub df_within: f64,
    pub p: f64,
    pub ss_between: f64,
    pub ss_within: f64,
    pub ms_within: f64,
}

/// 单因素方差分析。
pub fn anova(groups: &[Vec<f64>]) -> Option<AnovaResult> {
    let gs: Vec<&Vec<f64>> = groups.iter().filter(|g| g.len() >= 2).collect();
    if gs.len() < 2 {
        return None;
    }
    let grand_n: usize = gs.iter().map(|g| g.len()).sum();
    let grand_mean = gs.iter().map(|g| g.iter().sum::<f64>()).sum::<f64>() / grand_n as f64;
    let ss_between: f64 = gs
        .iter()
        .map(|g| {
            let m = g.iter().sum::<f64>() / g.len() as f64;
            g.len() as f64 * (m - grand_mean).powi(2)
        })
        .sum();
    let ss_within: f64 = gs
        .iter()
        .map(|g| {
            let m = g.iter().sum::<f64>() / g.len() as f64;
            g.iter().map(|x| (x - m).powi(2)).sum::<f64>()
        })
        .sum();
    let df_between = (gs.len() - 1) as f64;
    let df_within = (grand_n - gs.len()) as f64;
    if df_within <= 0.0 || ss_within <= 0.0 {
        return None;
    }
    let ms_within = ss_within / df_within;
    let f = (ss_between / df_between) / ms_within;
    Some(AnovaResult {
        f,
        df_between,
        df_within,
        p: f_p(f, df_between, df_within),
        ss_between,
        ss_within,
        ms_within,
    })
}

#[derive(Clone, Debug)]
pub struct Regression {
    pub slope: f64,
    pub intercept: f64,
    pub r: f64,
    pub r2: f64,
    pub se_slope: f64,
    pub p: f64,
    pub n: usize,
}

/// 一元线性回归 y = a + b·x。
pub fn regression(xs: &[f64], ys: &[f64]) -> Option<Regression> {
    let n = xs.len().min(ys.len());
    if n < 3 {
        return None;
    }
    let mx = xs[..n].iter().sum::<f64>() / n as f64;
    let my = ys[..n].iter().sum::<f64>() / n as f64;
    let sxx = xs[..n].iter().map(|x| (x - mx).powi(2)).sum::<f64>();
    let sxy = xs[..n]
        .iter()
        .zip(ys[..n].iter())
        .map(|(x, y)| (x - mx) * (y - my))
        .sum::<f64>();
    let syy = ys[..n].iter().map(|y| (y - my).powi(2)).sum::<f64>();
    if sxx <= 0.0 {
        return None;
    }
    let slope = sxy / sxx;
    let intercept = my - slope * mx;
    let r = if syy > 0.0 { sxy / (sxx * syy).sqrt() } else { 0.0 };
    let df = (n - 2) as f64;
    let sse = syy - slope * sxy;
    let se_slope = if df > 0.0 && sse >= 0.0 {
        (sse / df / sxx).sqrt()
    } else {
        0.0
    };
    let t = if se_slope > 0.0 { slope / se_slope } else { 0.0 };
    Some(Regression {
        slope,
        intercept,
        r,
        r2: r * r,
        se_slope,
        p: t_p_two_tailed(t, df),
        n,
    })
}

// ── 结果表渲染 ──────────────────────────────────────────────────────────────

fn f(v: f64, d: usize) -> String {
    // 小数位数跟随「设置 ▸ 分析 ▸ 小数位数」。
    // 传入的 d 只在两种情况下继续生效：
    //   0 = 计数项（样本量 N 之类，永远整数，不该变成 3.00000）
    //   1 = 自由度（df）
    // 其余（4 / 5 / 3 / 2 …）一律改用设置值 —— 以前是写死的位数，
    // 所以用户在设置里改小数位数完全看不出变化。
    let p = match d {
        0 => 0,
        1 => 1,
        _ => crate::settings::get().decimals.min(10),
    };
    format!("{:.*}", p, v)
}

/// 把描述统计渲染成析光风格的结果行（编号 / 项目 / 各组值）。
/// DESC_ROWS 固定 15 行顺序 —— 属于公共协议，改变它会破坏所有复用
/// `desc_values()` 的结果表。
pub const DESC_ROWS: [&str; 15] = [
    "N",
    "Mean",
    "SD",
    "SEM",
    "CV",
    "Minimum",
    "25% percentile",
    "Median",
    "75% percentile",
    "Maximum",
    "IQR",
    "Geometric mean",
    "Skewness",
    "Kurtosis",
    "Mean 95% CI",
];

/// 描述统计结果表：严格按 DESC_ROWS 的 15 行顺序输出。
pub fn descriptive_rows(names: &[String], cols: &[Vec<f64>]) -> Vec<Vec<String>> {
    let ds: Vec<Descriptive> = cols.iter().map(|c| describe(c)).collect();
    let mut rows: Vec<Vec<String>> = Vec::new();
    rows.push(vec!["1".to_string(), "Descriptive statistics".to_string()]);
    let mut head = vec!["".to_string(), "Group".to_string()];
    head.extend(names.iter().cloned());
    rows.push(head);
    let getters: [fn(&Descriptive) -> String; 15] = [
        |d| f(d.n as f64, 0),
        |d| f(d.mean, 4),
        |d| f(d.sd, 4),
        |d| f(d.sem, 4),
        |d| f(d.cv, 2),
        |d| f(d.min, 4),
        |d| f(d.q1, 4),
        |d| f(d.median, 4),
        |d| f(d.q3, 4),
        |d| f(d.max, 4),
        |d| f(d.iqr, 4),
        |d| f(d.geo_mean, 4),
        |d| f(d.skewness, 4),
        |d| f(d.kurtosis, 4),
        |d| {
            if d.mean_ci_lo.is_finite() && d.mean_ci_hi.is_finite() {
                format!("{} to {}", f(d.mean_ci_lo, 4), f(d.mean_ci_hi, 4))
            } else {
                "-".to_string()
            }
        },
    ];
    for (idx, label) in DESC_ROWS.iter().enumerate() {
        let mut row = vec![(idx + 2).to_string(), (*label).to_string()];
        for d in &ds {
            row.push(getters[idx](d));
        }
        rows.push(row);
    }
    rows
}


/// t 检验结果行。
pub fn ttest_rows(res: &TTestResult) -> Vec<Vec<String>> {
    vec![
        vec!["1".into(), "Table Analyzed".into(), "".into()],
        vec!["2".into(), "Column B".into(), res.b.clone()],
        vec!["3".into(), "vs.".into(), "vs.".into()],
        vec!["4".into(), "Column A".into(), res.a.clone()],
        vec!["5".into(), "Unpaired t test".into(), "".into()],
        vec!["6".into(), "Mean of A".into(), f(res.mean_a, 4)],
        vec!["7".into(), "Mean of B".into(), f(res.mean_b, 4)],
        vec!["8".into(), "Difference between means".into(), format!("{} ± {}", f(res.diff, 4), f(res.se_diff, 4))],
        vec!["9".into(), "t, df".into(), format!("t={}, df={}", f(res.t, 3), f(res.df, 1))],
        vec!["10".into(), "P value (two-tailed)".into(), fmt_p(res.p)],
        vec!["11".into(), "P value summary".into(), stars(res.p).to_string()],
        vec![
            "12".into(),
            "Significantly different? (P<0.05)".into(),
            if res.p < 0.05 { "Yes".into() } else { "No".into() },
        ],
        vec!["13".into(), "n (A, B)".into(), format!("{}, {}", res.df_a, res.df_b)],
    ]
}

/// 方差分析结果行。
pub fn anova_rows(res: &AnovaResult, k: usize, n: usize) -> Vec<Vec<String>> {
    vec![
        vec!["1".into(), "One-way analysis of variance".into(), "".into()],
        vec!["2".into(), "Number of groups".into(), k.to_string()],
        vec!["3".into(), "Number of values".into(), n.to_string()],
        vec!["4".into(), "F".into(), f(res.f, 4)],
        vec![
            "5".into(),
            "DF (between, within)".into(),
            format!("{}, {}", res.df_between as i64, res.df_within as i64),
        ],
        vec!["6".into(), "SS (between)".into(), f(res.ss_between, 4)],
        vec!["7".into(), "SS (within)".into(), f(res.ss_within, 4)],
        vec!["8".into(), "MS (within)".into(), f(res.ms_within, 4)],
        vec!["9".into(), "P value".into(), fmt_p(res.p)],
        vec!["10".into(), "P value summary".into(), stars(res.p).to_string()],
        vec![
            "11".into(),
            "Significantly different? (P<0.05)".into(),
            if res.p < 0.05 { "Yes".into() } else { "No".into() },
        ],
    ]
}

/// 回归结果行。
pub fn regression_rows(res: &Regression) -> Vec<Vec<String>> {
    vec![
        vec!["1".into(), "Linear regression".into(), "".into()],
        vec!["2".into(), "Number of points".into(), res.n.to_string()],
        vec!["3".into(), "Slope".into(), f(res.slope, 5)],
        vec!["4".into(), "Std. Error of slope".into(), f(res.se_slope, 5)],
        vec!["5".into(), "Y-intercept".into(), f(res.intercept, 5)],
        vec!["6".into(), "r".into(), f(res.r, 5)],
        vec!["7".into(), "R squared".into(), f(res.r2, 5)],
        vec!["8".into(), "P value (slope ≠ 0)".into(), fmt_p(res.p)],
        vec!["9".into(), "P value summary".into(), stars(res.p).to_string()],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_t_test() {
        // 教科书例子：两组均值差显著
        let a = [12.4, 15.1, 9.8, 11.2, 13.6, 10.4, 14.0, 12.9];
        let b = [28.6, 31.2, 25.9, 29.4, 33.1, 27.0, 30.5, 26.3];
        let r = t_test(&a, &b, TTestKind::Welch).unwrap();
        assert!((r.mean_a - 12.4250).abs() < 1e-3);
        assert!((r.mean_b - 29.0).abs() < 1e-3);
        assert!(r.p < 0.0001);
        assert_eq!(stars(r.p), "***");
    }

    #[test]
    fn betai_matches_known_values() {
        // I_0.5(2,3) = 0.6875
        assert!((betai(2.0, 3.0, 0.5) - 0.6875).abs() < 1e-6);
    }

    #[test]
    fn anova_on_identical_groups_is_ns() {
        let g = vec![
            vec![1.0, 2.0, 3.0, 4.0],
            vec![1.0, 2.0, 3.0, 4.0],
        ];
        let r = anova(&g).unwrap();
        assert!(r.f < 1e-9);
        assert_eq!(r.p, 1.0);
    }

    #[test]
    fn regression_recovers_line() {
        let xs: Vec<f64> = (0..10).map(|i| i as f64).collect();
        let ys: Vec<f64> = xs.iter().map(|x| 2.0 * x + 1.0).collect();
        let r = regression(&xs, &ys).unwrap();
        assert!((r.slope - 2.0).abs() < 1e-9);
        assert!((r.intercept - 1.0).abs() < 1e-9);
        assert!((r.r2 - 1.0).abs() < 1e-9);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 扩展统计：相关矩阵 / 卡方 / Fisher / Grubbs / 正态性 / 嵌套 ANOVA / KM
// 全部是真实计算，没有占位数字。
// ═══════════════════════════════════════════════════════════════════════════

/// Pearson 相关矩阵。返回下三角填充的 `n×n` 相关系数。
pub fn corr_matrix(cols: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let k = cols.len();
    if k == 0 {
        return None;
    }
    let mut m = vec![vec![f64::NAN; k]; k];
    // 逐列求均值（两两配对时只取两者都有效的行）
    for i in 0..k {
        for j in 0..=i {
            let n = cols[i].len().min(cols[j].len());
            let mut sx = 0.0;
            let mut sy = 0.0;
            let mut c = 0usize;
            for t in 0..n {
                if cols[i][t].is_finite() && cols[j][t].is_finite() {
                    sx += cols[i][t];
                    sy += cols[j][t];
                    c += 1;
                }
            }
            if c < 3 {
                continue;
            }
            let (mx, my) = (sx / c as f64, sy / c as f64);
            let mut sxy = 0.0;
            let mut sxx = 0.0;
            let mut syy = 0.0;
            for t in 0..n {
                if !cols[i][t].is_finite() || !cols[j][t].is_finite() {
                    continue;
                }
                let dx = cols[i][t] - mx;
                let dy = cols[j][t] - my;
                sxy += dx * dy;
                sxx += dx * dx;
                syy += dy * dy;
            }
            let den = (sxx * syy).sqrt();
            let r = if den > 0.0 { (sxy / den).clamp(-1.0, 1.0) } else { 0.0 };
            m[i][j] = r;
            m[j][i] = r;
        }
        m[i][i] = 1.0;
    }
    Some(m)
}

/// 相关系数的 t 检验双尾 p 值。
pub fn corr_p(r: f64, n: usize) -> Option<f64> {
    if n < 3 || !r.is_finite() || r.abs() >= 1.0 {
        return None;
    }
    let df = (n - 2) as f64;
    let t = r * (df / (1.0 - r * r)).sqrt();
    Some(t_p_two_tailed(t, df))
}

pub fn corr_rows(names: &[String], m: &[Vec<f64>], ns: &[usize]) -> Vec<Vec<String>> {
    let mut rows = vec![vec![
        "".to_string(),
        "Pearson r".to_string(),
        "P value".to_string(),
        "N".to_string(),
    ]];
    rows[0].extend(names.iter().cloned());
    let k = names.len();
    for i in 0..k {
        for j in 0..i {
            let n = ns.get(i).copied().unwrap_or(0).min(ns.get(j).copied().unwrap_or(0));
            let r = m[i][j];
            let p = corr_p(r, n);
            let mut row = vec![
                format!("{} vs {}", names[i], names[j]),
                format!("{:.4}", r),
                p.map(fmt_p).unwrap_or_else(|| "—".into()),
                n.to_string(),
            ];
            row.extend(vec!["".to_string(); k]);
            rows.push(row);
        }
    }
    rows
}

// ── 卡方（列联表）─────────────────────────────────────────────────────────

pub struct ChiSquare {
    pub chi2: f64,
    pub df: usize,
    pub p: f64,
    pub n: usize,
}

/// 卡方独立性检验（含 Yates 连续性校正，2×2 时启用）。
pub fn chi_square(table: &[Vec<f64>]) -> Option<ChiSquare> {
    let r = table.len();
    if r < 2 || table[0].len() < 2 {
        return None;
    }
    let c = table[0].len();
    let mut row_t = vec![0.0; r];
    let mut col_t = vec![0.0; c];
    let mut n = 0.0;
    for i in 0..r {
        for j in 0..c {
            row_t[i] += table[i][j];
            col_t[j] += table[i][j];
            n += table[i][j];
        }
    }
    if n <= 0.0 {
        return None;
    }
    let yates = r == 2 && c == 2;
    let mut chi2 = 0.0;
    for i in 0..r {
        for j in 0..c {
            let e = row_t[i] * col_t[j] / n;
            if e <= 0.0 {
                continue;
            }
            let d = if yates {
                (table[i][j] - e).abs() - 0.5
            } else {
                table[i][j] - e
            };
            chi2 += d * d / e;
        }
    }
    let df = (r - 1) * (c - 1);
    // 卡方上尾概率 = 不完全 Gamma 的补：Q(df/2, chi2/2)
    let p = gamma_q(df as f64 / 2.0, chi2 / 2.0);
    Some(ChiSquare {
        chi2,
        df,
        p,
        n: n as usize,
    })
}

/// 正则化上不完全 Gamma Q(a,x)（级数 + 连分数，按 x 大小切换）。
pub fn gamma_q(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 1.0;
    }
    if x < a + 1.0 {
        // 级数求 P(a,x)
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
    (sum * (-x + a * x.ln() - log_gamma(a)).exp()).clamp(0.0, 1.0)
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
    (h * (-x + a * x.ln() - log_gamma(a)).exp()).clamp(0.0, 1.0)
}

pub fn chisq_rows(res: &ChiSquare) -> Vec<Vec<String>> {
    vec![
        vec!["Chi-square".to_string(), format!("{:.4}", res.chi2)],
        vec!["df".to_string(), res.df.to_string()],
        vec!["P value".to_string(), fmt_p(res.p)],
        vec!["P value summary".to_string(), stars(res.p).to_string()],
        vec!["Total observations".to_string(), res.n.to_string()],
    ]
}

// ── Fisher 精确检验（2×2）─────────────────────────────────────────────────

pub struct Fisher {
    pub p_two: f64,
}

/// 2×2 表的 Fisher 精确检验（双尾，按所有等概率或更极端的表求和）。
pub fn fisher_exact(t: [[f64; 2]; 2]) -> Option<Fisher> {
    let (a, b, c, d) = (t[0][0], t[0][1], t[1][0], t[1][1]);
    if a < 0.0 || b < 0.0 || c < 0.0 || d < 0.0 {
        return None;
    }
    let (r1, r2) = (a + b, c + d);
    let (c1, _) = (a + c, b + d);
    let n = r1 + r2;
    if n <= 0.0 {
        return None;
    }
    let hyper = |k: f64| -> f64 {
        // P(X=k) = C(c1,k)·C(c2,r1-k) / C(n,r1)
        (log_gamma(c1 + 1.0) - log_gamma(k + 1.0) - log_gamma(c1 - k + 1.0)
            + log_gamma(n - c1 + 1.0)
            - log_gamma(r1 - k + 1.0)
            - log_gamma(n - c1 - r1 + k + 1.0))
        .exp()
    };
    let lo = (r1 - (n - c1)).max(0.0);
    let hi = r1.min(c1);
    let p_obs = hyper(a);
    let mut p = 0.0;
    let mut k = lo;
    while k <= hi + 1e-9 {
        let pk = hyper(k);
        if pk <= p_obs * (1.0 + 1e-9) {
            p += pk;
        }
        k += 1.0;
    }
    Some(Fisher {
        p_two: p.min(1.0),
    })
}

// ── Grubbs 离群值检验 ─────────────────────────────────────────────────────

pub struct Grubbs {
    pub value: f64,
    pub index: usize,
    pub g: f64,
    pub p: f64,
    #[allow(dead_code)]
    pub n: usize,
}

/// 单侧 Grubbs 检验：找出偏离均值最大的那个值并给出 p 值。
pub fn grubbs(v: &[f64]) -> Option<Grubbs> {
    let d = describe(v);
    if d.n < 3 || d.sd <= 0.0 {
        return None;
    }
    let mut bi = 0usize;
    let mut bd = 0.0f64;
    for (i, &x) in v.iter().enumerate() {
        let dd = (x - d.mean).abs();
        if dd > bd {
            bd = dd;
            bi = i;
        }
    }
    let g = bd / d.sd;
    let n = d.n as f64;
    // G 的临界值换算成双尾 t，再取 p
    let t = g * ((n * (n - 2.0)) / (n * n - 1.0 - n * g * g)).sqrt();
    let p = if !t.is_finite() {
        0.0
    } else {
        t_p_two_tailed(t, n - 2.0) * n
    };
    Some(Grubbs {
        value: v[bi],
        index: bi,
        g,
        p: p.clamp(0.0, 1.0),
        n: d.n,
    })
}

pub fn grubbs_rows(names: &[String], cols: &[Vec<f64>]) -> Vec<Vec<String>> {
    let mut rows = vec![vec![
        "Data set".to_string(),
        "Outlier".to_string(),
        "Row".to_string(),
        "G".to_string(),
        "P value".to_string(),
    ]];
    for (i, c) in cols.iter().enumerate() {
        match grubbs(c) {
            Some(g) => rows.push(vec![
                names.get(i).cloned().unwrap_or_else(|| format!("{}", i + 1)),
                format!("{:.4}", g.value),
                (g.index + 1).to_string(),
                format!("{:.4}", g.g),
                fmt_p(g.p),
            ]),
            None => rows.push(vec![
                names.get(i).cloned().unwrap_or_else(|| format!("{}", i + 1)),
                "—".to_string(),
                "—".to_string(),
                "—".to_string(),
                "—".to_string(),
            ]),
        }
    }
    rows
}

// ── 正态性：Shapiro–Wilk（Royston 近似）───────────────────────────────────

pub struct Normality {
    pub w: f64,
    pub p: f64,
    pub n: usize,
}

/// Shapiro–Wilk 正态性检验。n 在 3..=5000 之间有效。
pub fn shapiro_wilk(v: &[f64]) -> Option<Normality> {
    let mut xs: Vec<f64> = v.iter().copied().filter(|x| x.is_finite()).collect();
    if xs.len() < 3 || xs.len() > 5000 {
        return None;
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = xs.len() as f64;
    let m = (xs.iter().sum::<f64>()) / n;

    // 期望正态次序统计量（Blom 近似）
    let q: Vec<f64> = (1..=xs.len())
        .map(|i| {
            let p = (i as f64 - 0.375) / (n + 0.25);
            // 逆正态 CDF（Acklam 近似）
            inv_norm(p)
        })
        .collect();
    let c: Vec<f64> = q.iter().map(|x| x / (q.iter().map(|v| v * v).sum::<f64>()).sqrt()).collect();

    // Royston：用 u = 1/sqrt(n) 的多项式近似系数
    let u = 1.0 / n.sqrt();
    let a: Vec<f64> = (1..=xs.len())
        .map(|i| {
            let ci = c[i - 1];
            let poly = -2.706056 * u.powf(5.0) + 4.434685 * u.powf(4.0) - 2.07119 * u.powf(3.0)
                - 0.147981 * u * u
                + 0.221157 * u
                + ci;
            poly
        })
        .collect();

    let num: f64 = a.iter().zip(xs.iter()).map(|(ai, xi)| ai * xi).sum();
    let ss: f64 = xs.iter().map(|x| (x - m) * (x - m)).sum();
    if ss <= 0.0 {
        return None;
    }
    let w = (num * num) / ss;

    // W → p：Royston(1993) 的三次多项式变换，再取标准正态上尾
    let sw = (1.0 - w).max(1e-12).ln();
    let m3 = -0.0006714 * n.powf(3.0) + 0.025054 * n * n - 0.39978 * n + 0.5440;
    let s3 = (-0.0020322 * n.powf(3.0) + 0.062767 * n * n - 0.71047 * n + 2.1102)
        .max(0.05);
    let z = (sw - m3) / s3;
    let p = 0.5 * erfc(z / std::f64::consts::SQRT_2);
    Some(Normality {
        w,
        p: p.clamp(0.0, 1.0),
        n: xs.len(),
    })
}

/// 互补误差函数（Numerical Recipes 的 erfc，精度 ~1.2e-7）。
pub fn erfc(x: f64) -> f64 {
    let z = x.abs();
    let t = 2.0 / (2.0 + z);
    let ty = 4.0 * t - 2.0;
    let mut cof = [
        -1.3026537197817094,
        6.4196979235649026e-1,
        1.9476473204185836e-2,
        -9.561514786808631e-3,
        -9.46595344482036e-4,
        3.66839497852761e-4,
        4.2523324806907e-5,
        -2.0278578112534e-5,
        -1.624290004647e-6,
        1.303655835580e-6,
        1.5626441722e-8,
        -8.5238095915e-8,
        6.529054439e-9,
        5.059343495e-9,
        -9.91364156e-10,
        -2.27365122e-10,
        9.6467911e-11,
        2.394038e-12,
        -6.886027e-12,
        8.94487e-13,
        3.13092e-13,
        -1.12708e-13,
        3.81e-16,
        7.106e-15,
    ]
    .to_vec();
    cof.reverse();
    let mut d = 0.0;
    let mut dd = 0.0;
    for c in cof {
        let tmp = d;
        d = ty * d - dd + c;
        dd = tmp;
    }
    let ans = t * (-z * z + 0.5 * (d - ty * dd)).exp();
    if x >= 0.0 {
        ans
    } else {
        2.0 - ans
    }
}

/// 标准正态的逆 CDF（Acklam 有理逼近）。
fn inv_norm(p: f64) -> f64 {
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

pub fn normality_rows(names: &[String], cols: &[Vec<f64>]) -> Vec<Vec<String>> {
    let mut rows = vec![vec![
        "Data set".to_string(),
        "W".to_string(),
        "P value".to_string(),
        "Passed normality".to_string(),
        "N".to_string(),
    ]];
    for (i, c) in cols.iter().enumerate() {
        match shapiro_wilk(c) {
            Some(r) => rows.push(vec![
                names.get(i).cloned().unwrap_or_else(|| format!("{}", i + 1)),
                format!("{:.4}", r.w),
                fmt_p(r.p),
                if r.p > 0.05 { "Yes".to_string() } else { "No".to_string() },
                r.n.to_string(),
            ]),
            None => rows.push(vec![
                names.get(i).cloned().unwrap_or_else(|| format!("{}", i + 1)),
                "—".to_string(),
                "—".to_string(),
                "—".to_string(),
                c.len().to_string(),
            ]),
        }
    }
    rows
}

// ── 嵌套单因素 ANOVA ──────────────────────────────────────────────────────

pub struct NestedAnova {
    /// 组间（处理）
    pub f_treat: f64,
    pub p_treat: f64,
    /// 组内（嵌套单元）
    pub f_nested: f64,
    pub p_nested: f64,
}

/// 两阶段嵌套 ANOVA：`groups` 为处理组，每组内若干子单元（Vec<Vec<f64>>）。
pub fn nested_anova(groups: &[Vec<Vec<f64>>]) -> Option<NestedAnova> {
    let mut grand_sum = 0.0;
    let mut grand_n = 0usize;
    let mut treat_means = Vec::new();
    let mut treat_n = Vec::new();
    for g in groups {
        let (s, n) = g.iter().fold((0.0, 0usize), |(a, b), c| {
            (a + c.iter().sum::<f64>(), b + c.len())
        });
        if n == 0 {
            continue;
        }
        grand_sum += s;
        grand_n += n;
        treat_means.push(s / n as f64);
        treat_n.push(n);
    }
    let k = treat_means.len();
    if k < 2 || grand_n == 0 {
        return None;
    }
    let gm = grand_sum / grand_n as f64;

    // 组间平方和（加权）
    let mut ss_treat = 0.0;
    for i in 0..k {
        ss_treat += treat_n[i] as f64 * (treat_means[i] - gm) * (treat_means[i] - gm);
    }
    // 子单元间平方和
    let mut ss_sub = 0.0;
    let mut df_sub = 0usize;
    for g in groups {
        for cell in g {
            if cell.is_empty() {
                continue;
            }
            let cm = cell.iter().sum::<f64>() / cell.len() as f64;
            ss_sub += cell.len() as f64 * (cm - gm) * (cm - gm);
            df_sub += 1;
        }
    }
    ss_sub = (ss_sub - ss_treat).max(0.0);
    // 组内（误差）
    let mut ss_err = 0.0;
    let mut df_err = 0usize;
    for g in groups {
        for cell in g {
            if cell.is_empty() {
                continue;
            }
            let cm = cell.iter().sum::<f64>() / cell.len() as f64;
            for x in cell {
                ss_err += (x - cm) * (x - cm);
            }
            df_err += cell.len() - 1;
        }
    }
    let df_treat = k - 1;
    let df_sub = df_sub.saturating_sub(k).max(0);
    if df_treat == 0 || df_err == 0 {
        return None;
    }
    let ms_treat = ss_treat / df_treat as f64;
    let ms_err = ss_err / df_err as f64;
    let ms_sub = if df_sub > 0 { ss_sub / df_sub as f64 } else { 0.0 };
    let f_treat = ms_treat / ms_err;
    let f_nested = if ms_err > 0.0 { ms_sub / ms_err } else { 0.0 };
    Some(NestedAnova {
        f_treat,
        p_treat: f_p(f_treat, df_treat as f64, df_err as f64),
        f_nested,
        p_nested: if df_sub > 0 {
            f_p(f_nested, df_sub as f64, df_err as f64)
        } else {
            f64::NAN
        },
    })
}

// ── Kaplan–Meier 生存曲线 ─────────────────────────────────────────────────

pub struct KmPoint {
    pub time: f64,
    pub surv: f64,
    pub at_risk: usize,
    pub events: usize,
}

/// Kaplan–Meier 估计。`time` 与 `event`(1=事件 0=删失) 等长。
pub fn kaplan_meier(time: &[f64], event: &[f64]) -> Vec<KmPoint> {
    let mut pts: Vec<(f64, f64)> = time
        .iter()
        .zip(event.iter())
        .filter(|(t, _)| t.is_finite() && **t >= 0.0)
        .map(|(t, e)| (*t, *e))
        .collect();
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let n = pts.len();
    let mut out = Vec::new();
    let mut surv = 1.0f64;
    let mut i = 0usize;
    while i < n {
        let t = pts[i].0;
        let mut d = 0usize; // 事件数
        while i < n && (pts[i].0 - t).abs() < 1e-12 {
            if pts[i].1 >= 0.5 {
                d += 1;
            }
            i += 1;
        }
        // 风险人数 = 尚未发生事件且尚未删失的个体数（时间 ≥ t）
        let at_risk = pts.iter().filter(|(tt, _)| *tt >= t - 1e-12).count();
        if at_risk > 0 {
            surv *= 1.0 - d as f64 / at_risk as f64;
        }
        out.push(KmPoint {
            time: t,
            surv,
            at_risk,
            events: d,
        });
    }
    out
}

// ══════════════════════════════════════════════════════════════════════════
// 学生化极差分布（Tukey / Games-Howell 的 p 值基础）
// ══════════════════════════════════════════════════════════════════════════

/// χ 分布密度（df 自由度）。s（样本标准差/σ）= x/√df。
fn chi_pdf(x: f64, df: f64) -> f64 {
    if x <= 0.0 || df <= 0.0 {
        return 0.0;
    }
    let lg = log_gamma(df / 2.0);
    (2.0f64).powf(1.0 - df / 2.0) * x.powf(df - 1.0) * (-x * x / 2.0).exp() / lg.exp()
}

fn norm_pdf_local(z: f64) -> f64 {
    (-0.5 * z * z).exp() / (2.0f64 * std::f64::consts::PI).sqrt()
}

/// 学生化极差分布 CDF：P(Q ≤ q)，k = 组数，df = 误差自由度。
///
/// Q = (max − min)/s，其中 s 独立于样本、νs²/σ² ~ χ²_ν。
/// 给定 s 后：P(max−min ≤ q·s) = k ∫ φ(z)[Φ(z+q·s) − Φ(z)]^(k−1) dz
/// 再对 s 的 χ 分布做外层积分。两层都用 Simpson。
pub fn q_cdf(q: f64, k: usize, df: f64) -> f64 {
    if q <= 0.0 || k < 2 || df <= 0.0 {
        return 0.0;
    }
    let km1 = (k - 1) as i32;
    let kf = k as f64;
    // 内层：∫ φ(z)[Φ(z + q·s) − Φ(z)]^(k−1) dz
    //
    // 被积函数由 φ(z) 主导衰减：|z| > 7 时 φ(z) < 1e-11，再往外积分对结果
    // 没有贡献。原先取 [-8, 8 + q·s]，q·s 一大区间就拉到 20 以上，为了维持
    // 步长不得不堆到 400 个节点 —— 其中大半算的是 0。这里固定 [-7, 7]、
    // 节点 256，步长反而更细、结果更准。
    let inner = |s: f64| -> f64 {
        let lo = -7.0;
        let hi = 7.0;
        let n = 256;
        let h = (hi - lo) / n as f64;
        let t = q * s;
        let mut acc = 0.0;
        for i in 0..=n {
            let z = lo + i as f64 * h;
            let d = norm_cdf_local(z + t) - norm_cdf_local(z);
            let w = if i == 0 || i == n { 1.0 } else if i % 2 == 1 { 4.0 } else { 2.0 };
            // powi 是整数幂，展开成连乘；指数恒为整数 (k−1)。
            // 用 powf 会走 libm 的通用浮点幂，慢一个数量级 —— 这里是 96k 次
            // 调用的热点 innermost，换成 powi 是本次提速的最大单项。
            acc += w * norm_pdf_local(z) * d.powi(km1);
        }
        kf * acc * h / 3.0
    };
    // s = x/√df，x ~ χ(df)
    let sq = df.sqrt();
    let xmax = sq + 5.0;
    let n = 160;
    let h = xmax / n as f64;
    let mut acc = 0.0;
    for i in 1..=n {
        let x = i as f64 * h;
        let w = if i == n { 1.0 } else if i % 2 == 1 { 4.0 } else { 2.0 };
        acc += w * chi_pdf(x, df) * inner(x / sq);
    }
    (acc * h / 3.0).clamp(0.0, 1.0)
}

pub fn q_sf(q: f64, k: usize, df: f64) -> f64 {
    (1.0 - q_cdf(q, k, df)).clamp(0.0, 1.0)
}

/// 正态 CDF（局部别名，避免与已有的 erfc 版本重名冲突）
fn norm_cdf_local(z: f64) -> f64 {
    0.5 * erfc(-z / std::f64::consts::SQRT_2)
}

/// 标准正态累积分布函数（公开入口）。
pub fn norm_cdf(z: f64) -> f64 {
    norm_cdf_local(z)
}

/// 标准正态上尾概率（公开入口）。
pub fn norm_sf(z: f64) -> f64 {
    norm_sf_local(z)
}

fn norm_sf_local(z: f64) -> f64 {
    1.0 - norm_cdf_local(z)
}

/// 二分求逆：给定尾概率求临界值。
///
/// # 为什么要缓存
///
/// 单次 `q_crit` = 44 次二分 × 每次一次 `q_cdf`（两层 Simpson 数值积分）。
/// 一次分析里同一组 (conf, k, df) 往往要查几十次 —— Tukey 的每一对都要用
/// 同一个临界值。不缓存时，6 组 15 对就是十几秒的纯等待，这正是
/// 「两两显著性卡住」的直接来源。
fn q_crit(conf: f64, k: usize, df: f64) -> f64 {
    thread_local! {
        static QC: std::cell::RefCell<std::collections::HashMap<(u64, usize, u64), f64>> =
            std::cell::RefCell::new(std::collections::HashMap::new());
    }
    let key = ((conf * 1e9).round() as u64, k, (df * 1e9).round() as u64);
    if let Some(v) = QC.with(|c| c.borrow().get(&key).copied()) {
        return v;
    }
    let target = conf;
    let (mut lo, mut hi) = (0.0, 50.0);
    // 44 次二分后区间宽 50/2^44 ≈ 2.8e-12，已经细过双精度能分辨的差异；
    // 原来的 80 次有一半是纯浪费。
    for _ in 0..44 {
        let mid = (lo + hi) / 2.0;
        if q_cdf(mid, k, df) < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let v = (lo + hi) / 2.0;
    QC.with(|c| {
        c.borrow_mut().insert(key, v);
    });
    v
}

// ══════════════════════════════════════════════════════════════════════════
// 多重比较校正
// ══════════════════════════════════════════════════════════════════════════

#[derive(Clone, Copy, PartialEq)]
pub enum MultiMethod {
    Tukey,
    GamesHowell,
    Dunnett,
    Dunn,
    Holm,
    Bonferroni,
    Sidak,
}

impl MultiMethod {
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            MultiMethod::Tukey => "Tukey（方差齐）",
            MultiMethod::GamesHowell => "Games-Howell（方差不齐）",
            MultiMethod::Dunnett => "Dunnett（对）",
            MultiMethod::Dunn => "Dunn（非参数）",
            MultiMethod::Holm => "Holm-Šídák",
            MultiMethod::Bonferroni => "Bonferroni",
            MultiMethod::Sidak => "Šídák",
        }
    }
    #[allow(dead_code)]
    pub fn en(self) -> &'static str {
        match self {
            MultiMethod::Tukey => "Tukey",
            MultiMethod::GamesHowell => "Games-Howell",
            MultiMethod::Dunnett => "Dunnett",
            MultiMethod::Dunn => "Dunn",
            MultiMethod::Holm => "Holm-Sidak",
            MultiMethod::Bonferroni => "Bonferroni",
            MultiMethod::Sidak => "Sidak",
        }
    }
    #[allow(dead_code)]
    pub fn all() -> &'static [MultiMethod] {
        &[
            MultiMethod::Tukey,
            MultiMethod::GamesHowell,
            MultiMethod::Dunnett,
            MultiMethod::Dunn,
            MultiMethod::Holm,
            MultiMethod::Bonferroni,
            MultiMethod::Sidak,
        ]
    }
}

fn bonferroni(p: f64, m: usize) -> f64 {
    (p * m as f64).min(1.0)
}
fn sidak(p: f64, m: usize) -> f64 {
    1.0 - (1.0 - p).powf(m as f64)
}
/// Holm–Šídák：按 p 升序，adj[i] = max_{j≤i}((m−j)·p[j])，并单调化。
fn holm(pairs: &mut [(String, f64)]) {
    pairs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let m = pairs.len();
    let mut running = 0.0f64;
    for (i, item) in pairs.iter_mut().enumerate() {
        let adj = ((m - i) as f64 * item.1).min(1.0);
        running = running.max(adj);
        item.1 = running;
    }
}

/// 一组数据的**全部两两比较**显著性矩阵。
///
/// 这是图上的显著性标注条能「自动给出 P 值并推荐星号」的依据：
/// 不是只看某一对，而是枚举所有组之间的比较、统一做多重比较校正，
/// 再按校正后的 p 给出 `*` / `**` / `***` / `ns`。
///
/// 返回 `Vec<(i, j, p_raw, p_adj, stars)>`，顺序是 (0,1) (0,2) … (1,2) …
///
/// # 为什么必须校正
/// 只看单对的 p 会**系统性夸大显著性**：组数越多，随机出现一对 p<0.05 的概率越大。
/// 6 组有 15 对，全用 0.05 判，至少一对假阳性的概率接近 54%。
/// 所以这里统一按 `method` 校正后再分级。
pub fn pairwise_matrix(
    groups: &[Vec<f64>],
    method: MultiMethod,
    conf: f64,
) -> Vec<(usize, usize, f64, f64, &'static str)> {
    let n = groups.len();
    if n < 2 {
        return Vec::new();
    }
    // 1) 枚举所有对，先算原始 p（Welch，不假定等方差）
    let mut idx: Vec<(usize, usize)> = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            idx.push((i, j));
        }
    }
    let raw: Vec<f64> = idx
        .iter()
        .map(|&(i, j)| {
            t_test(&groups[i], &groups[j], TTestKind::Welch)
                .map(|r| r.p)
                .unwrap_or(f64::NAN)
        })
        .collect();

    // 2) 校正。优先用该方法专门的分布；不可用则退回 Holm。
    let mut adj = raw.clone();
    let m = idx.len();
    let mut done = false;
    match method {
        MultiMethod::Tukey | MultiMethod::GamesHowell | MultiMethod::Dunn => {
            let got = match method {
                MultiMethod::Tukey => tukey_hsd(groups, conf),
                MultiMethod::GamesHowell => games_howell(groups, conf),
                _ => dunn_test(groups, conf),
            };
            // 只有返回条数与枚举对数一致时才敢按序对应，否则退回 Holm
            if let Some(v) = got {
                if v.len() == m {
                    adj = v.iter().map(|x| x.p).collect();
                    done = true;
                }
            }
        }
        MultiMethod::Bonferroni => {
            adj = raw.iter().map(|p| bonferroni(*p, m)).collect();
            done = true;
        }
        MultiMethod::Sidak => {
            adj = raw.iter().map(|p| sidak(*p, m)).collect();
            done = true;
        }
        _ => {}
    }
    if !done {
        let mut pairs: Vec<(String, f64)> = adj.iter().map(|p| (String::new(), *p)).collect();
        holm(&mut pairs);
        adj = pairs.iter().map(|x| x.1).collect();
    }

    // 3) 分级
    idx.iter()
        .enumerate()
        .map(|(k, &(i, j))| (i, j, raw[k], adj[k], stars(adj[k])))
        .collect()
}

// ══════════════════════════════════════════════════════════════════════════
// 事后两两比较
// ══════════════════════════════════════════════════════════════════════════

#[derive(Clone)]
pub struct Pairwise {
    pub label: String,
    pub diff: f64,
    pub lo: f64,
    pub hi: f64,
    pub p: f64,
    pub stat: f64,
}

/// Tukey HSD：SE = √(MS_w · 0.5 · (1/nᵢ + 1/nⱼ))，q 分布。
pub fn tukey_hsd(groups: &[Vec<f64>], conf: f64) -> Option<Vec<Pairwise>> {
    let a = anova(groups)?;
    let k = groups.len();
    // 临界值对所有组对是同一个（k 与 df 都一样），必须提出循环。
    // 放在循环内意味着每一对都重跑一次「44 次二分 × 两层数值积分」，
    // 6 组 15 对就是十几秒 —— 这是「两两显著性卡住」的根因。
    let qc = q_crit(conf, k, a.df_within);
    let mut out = Vec::new();
    for i in 0..k {
        for j in (i + 1)..k {
            let (mi, mj) = (mean(&groups[i]), mean(&groups[j]));
            let (ni, nj) = (groups[i].len() as f64, groups[j].len() as f64);
            let se = (a.ms_within * 0.5 * (1.0 / ni + 1.0 / nj)).sqrt();
            if se <= 0.0 {
                continue;
            }
            let q = (mi - mj).abs() / se;
            let p = q_sf(q, k, a.df_within);
            let half = qc * se;
            let d = mi - mj;
            out.push(Pairwise {
                label: format!("{} vs {}", i + 1, j + 1),
                diff: d,
                lo: d - half,
                hi: d + half,
                p,
                stat: q,
            });
        }
    }
    Some(out)
}

/// Games-Howell：不假定方差齐性。每对单独算 Welch 的 SE 与 df，
/// 统计量仍走学生化极差 q（k 组、该对的 Welch df）。
pub fn games_howell(groups: &[Vec<f64>], conf: f64) -> Option<Vec<Pairwise>> {
    let k = groups.len();
    if k < 2 {
        return None;
    }
    let mut out = Vec::new();
    for i in 0..k {
        for j in (i + 1)..k {
            let gi = &groups[i];
            let gj = &groups[j];
            if gi.len() < 2 || gj.len() < 2 {
                continue;
            }
            let (mi, mj) = (mean(gi), mean(gj));
            let (vi, vj) = (var(gi) / gi.len() as f64, var(gj) / gj.len() as f64);
            let se = (0.5 * (vi + vj)).sqrt();
            if se <= 0.0 {
                continue;
            }
            // Welch–Satterthwaite 自由度
            let df = (vi + vj).powi(2)
                / (vi.powi(2) / (gi.len() - 1) as f64 + vj.powi(2) / (gj.len() - 1) as f64);
            let q = (mi - mj).abs() / se;
            let p = q_sf(q, k, df);
            let half = q_crit(conf, k, df) * se;
            let d = mi - mj;
            out.push(Pairwise {
                label: format!("{} vs {}", i + 1, j + 1),
                diff: d,
                lo: d - half,
                hi: d + half,
                p,
                stat: q,
            });
        }
    }
    Some(out)
}

/// 等相关的多元 t 分布 CDF：P(max|T_i| ≤ t)，m 个比较、df 自由度、相关 ρ。
/// 用于 Dunnett。条件分解：Z_i = √ρ·U + √(1−ρ)·E_i，再对 χ 积分。
fn multivar_t_cdf(t: f64, m: usize, df: f64, rho: f64) -> f64 {
    if m == 0 || df <= 0.0 {
        return 1.0;
    }
    let r = rho.clamp(0.0, 0.999);
    let sr = r.sqrt();
    let sr1 = (1.0 - r).sqrt();
    let mf = m as i32;
    let inner_u = |ts: f64| -> f64 {
        let lo = -7.0;
        let hi = 7.0;
        let n = 200;
        let h = (hi - lo) / n as f64;
        let mut acc = 0.0;
        for i in 0..=n {
            let u = lo + i as f64 * h;
            let w = if i == 0 || i == n { 1.0 } else if i % 2 == 1 { 4.0 } else { 2.0 };
            let hi_b = norm_cdf_local((ts - sr * u) / sr1);
            let lo_b = norm_cdf_local((-ts - sr * u) / sr1);
            acc += w * norm_pdf_local(u) * (hi_b - lo_b).powi(mf);
        }
        (acc * h / 3.0).clamp(0.0, 1.0)
    };
    let sq = df.sqrt();
    let xmax = sq + 5.0;
    let n = 140;
    let h = xmax / n as f64;
    let mut acc = 0.0;
    for i in 1..=n {
        let x = i as f64 * h;
        let w = if i == n { 1.0 } else if i % 2 == 1 { 4.0 } else { 2.0 };
        acc += w * chi_pdf(x, df) * inner_u(t * x / sq);
    }
    (acc * h / 3.0).clamp(0.0, 1.0)
}

/// Dunnett：各组与**对照组**（第 0 组）比较。
/// 各组 n 相等时走精确多元 t（ρ = 0.5）；n 不等时用 Šídák 校正的 t 检验（保守）。
pub fn dunnett(groups: &[Vec<f64>], conf: f64) -> Option<Vec<Pairwise>> {
    let a = anova(groups)?;
    let df = a.df_within;
    let ctrl = 0usize;
    let n0 = groups[ctrl].len();
    let equal_n = groups.iter().all(|g| g.len() == n0);
    let mut out = Vec::new();
    for i in 0..groups.len() {
        if i == ctrl {
            continue;
        }
        let (mi, mj) = (mean(&groups[i]), mean(&groups[ctrl]));
        let d = mi - mj;
        let se = (a.ms_within * (1.0 / groups[i].len() as f64 + 1.0 / n0 as f64)).sqrt();
        if se <= 0.0 {
            continue;
        }
        let t = d.abs() / se;
        let p = if equal_n {
            // ρ = 0.5（等 n 时 Dunnett 统计量间的相关）
            (1.0 - multivar_t_cdf(t, groups.len() - 1, df, 0.5)).clamp(0.0, 1.0)
        } else {
            sidak(t_p_two_tailed(t, df), groups.len() - 1)
        };
        let crit = t_crit_two(conf, df);
        out.push(Pairwise {
            label: format!("{} vs {}", i + 1, ctrl + 1),
            diff: d,
            lo: d - crit * se,
            hi: d + crit * se,
            p,
            stat: t,
        });
    }
    Some(out)
}

/// 秩次（平均秩，处理并列）
pub fn ranks(all: &[f64]) -> Vec<f64> {
    let mut idx: Vec<usize> = (0..all.len()).collect();
    idx.sort_by(|&a, &b| all[a].partial_cmp(&all[b]).unwrap_or(std::cmp::Ordering::Equal));
    let mut r = vec![0.0f64; all.len()];
    let mut i = 0usize;
    while i < idx.len() {
        let mut j = i;
        while j + 1 < idx.len()
            && (all[idx[j + 1]] - all[idx[i]]).abs() < 1e-12
        {
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

/// Dunn 检验：Kruskal-Wallis 之后的两两比较，正态近似 + Bonferroni。
pub fn dunn_test(groups: &[Vec<f64>], _conf: f64) -> Option<Vec<Pairwise>> {
    let k = groups.len();
    if k < 2 {
        return None;
    }
    let all: Vec<f64> = groups.iter().flat_map(|g| g.iter().copied()).collect();
    let n = all.len() as f64;
    if n < 3.0 {
        return None;
    }
    let r = ranks(&all);
    let mut start = 0usize;
    let mut rsum = Vec::new();
    let mut ns = Vec::new();
    for g in groups {
        let s: f64 = r[start..start + g.len()].iter().sum();
        rsum.push(s / g.len() as f64);
        ns.push(g.len() as f64);
        start += g.len();
    }
    // 并列校正
    let mut tie = 0.0f64;
    let mut sorted = all.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut i = 0usize;
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
    let base = n * (n + 1.0) / 12.0;
    let tie_factor = if tie > 0.0 {
        (1.0 - tie / (n.powi(3) - n)).max(0.1)
    } else {
        1.0
    };
    let m = k * (k - 1) / 2;
    let mut out = Vec::new();
    for i in 0..k {
        for j in (i + 1)..k {
            let se = (base * tie_factor * (1.0 / ns[i] + 1.0 / ns[j])).sqrt();
            if se <= 0.0 {
                continue;
            }
            let z = (rsum[i] - rsum[j]).abs() / se;
            let p_raw = (2.0 * norm_sf_local(z)).clamp(0.0, 1.0);
            out.push(Pairwise {
                label: format!("{} vs {}", i + 1, j + 1),
                diff: rsum[i] - rsum[j],
                lo: f64::NAN,
                hi: f64::NAN,
                p: bonferroni(p_raw, m),
                stat: z,
            });
        }
    }
    Some(out)
}

/// 按方法跑两两比较。
#[allow(dead_code)]
pub fn pairwise(groups: &[Vec<f64>], method: MultiMethod, conf: f64) -> Option<Vec<Pairwise>> {
    match method {
        MultiMethod::Tukey => tukey_hsd(groups, conf),
        MultiMethod::GamesHowell => games_howell(groups, conf),
        MultiMethod::Dunnett => dunnett(groups, conf),
        MultiMethod::Dunn => dunn_test(groups, conf),
        MultiMethod::Holm | MultiMethod::Bonferroni | MultiMethod::Sidak => {
            let mut ps = Vec::new();
            let k = groups.len();
            for i in 0..k {
                for j in (i + 1)..k {
                    let r = t_test(&groups[i], &groups[j], TTestKind::Welch)?;
                    ps.push((format!("{} vs {}", i + 1, j + 1), r.p));
                }
            }
            let m = ps.len().max(1);
            match method {
                MultiMethod::Bonferroni => {
                    for it in ps.iter_mut() {
                        it.1 = bonferroni(it.1, m);
                    }
                }
                MultiMethod::Sidak => {
                    for it in ps.iter_mut() {
                        it.1 = sidak(it.1, m);
                    }
                }
                _ => holm(&mut ps),
            }
            Some(
                ps.into_iter()
                    .map(|(label, p)| Pairwise {
                        label,
                        diff: f64::NAN,
                        lo: f64::NAN,
                        hi: f64::NAN,
                        p,
                        stat: f64::NAN,
                    })
                    .collect(),
            )
        }
    }
}

pub fn pairwise_rows(pw: &[Pairwise], method: MultiMethod) -> Vec<Vec<String>> {
    let ci = !matches!(method, MultiMethod::Dunn | MultiMethod::Holm
        | MultiMethod::Bonferroni | MultiMethod::Sidak);
    let mut rows = Vec::new();
    rows.push(if ci {
        vec!["比较".into(), "差值".into(), "95% CI 下限".into(), "95% CI 上限".into(), "统计量".into(), "P 值".into(), "显著性".into()]
    } else {
        vec!["比较".into(), "P 值".into(), "显著性".into()]
    });
    for p in pw {
        let mut r = vec![p.label.clone()];
        if ci {
            r.push(f(p.diff, 4));
            r.push(f(p.lo, 4));
            r.push(f(p.hi, 4));
            r.push(f(p.stat, 4));
        }
        r.push(fmt_p(p.p));
        r.push(stars(p.p).to_string());
        rows.push(r);
    }
    rows
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}
fn var(v: &[f64]) -> f64 {
    let m = mean(v);
    v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (v.len() - 1).max(1) as f64
}

/// 双尾 t 临界值（二分求逆）
pub fn t_crit_two(conf: f64, df: f64) -> f64 {
    let target = (1.0 + conf) / 2.0;
    let (mut lo, mut hi) = (0.0, 100.0);
    for _ in 0..100 {
        let mid = (lo + hi) / 2.0;
        if 1.0 - t_p_two_tailed(mid, df) / 2.0 < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) / 2.0
}

// ══════════════════════════════════════════════════════════════════════════
// 非参数检验
// ══════════════════════════════════════════════════════════════════════════

#[derive(Clone)]
pub struct MwResult {
    pub u: f64,
    pub z: f64,
    pub p: f64,
}

/// Mann-Whitney U：正态近似，含并列校正与 0.5 连续性修正。
pub fn mann_whitney(a: &[f64], b: &[f64]) -> Option<MwResult> {
    if a.len() < 2 || b.len() < 2 {
        return None;
    }
    let mut all = a.to_vec();
    all.extend_from_slice(b);
    let r = ranks(&all);
    let n1 = a.len() as f64;
    let n2 = b.len() as f64;
    let r1: f64 = r[..a.len()].iter().sum();
    let u1 = r1 - n1 * (n1 + 1.0) / 2.0;
    let u2 = n1 * n2 - u1;
    let u = u1.min(u2);
    let mu = n1 * n2 / 2.0;
    let n = n1 + n2;
    // 并列校正
    let mut sorted = all.clone();
    sorted.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    let mut tie = 0.0f64;
    let mut i = 0usize;
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
    let sd = if tie > 0.0 {
        ((n1 * n2 / 12.0) * ((n + 1.0) - tie / (n * (n - 1.0)))).sqrt()
    } else {
        (n1 * n2 * (n + 1.0) / 12.0).sqrt()
    };
    if sd <= 0.0 {
        return None;
    }
    let z = (u - mu + 0.5) / sd;
    Some(MwResult {
        u,
        z,
        p: (2.0 * norm_sf_local(z.abs())).clamp(0.0, 1.0),
    })
}

#[derive(Clone)]
pub struct WilcoxonResult {
    pub w: f64,
    pub z: f64,
    pub p: f64,
}

/// Wilcoxon 符号秩检验（配对）。
pub fn wilcoxon(a: &[f64], b: &[f64]) -> Option<WilcoxonResult> {
    let n = a.len().min(b.len());
    if n < 5 {
        return None;
    }
    let d: Vec<f64> = (0..n).map(|i| a[i] - b[i]).filter(|x| x.abs() > 1e-12).collect();
    if d.is_empty() {
        return None;
    }
    let nn = d.len() as f64;
    let abs: Vec<f64> = d.iter().map(|x| x.abs()).collect();
    let r = ranks(&abs);
    let w_plus: f64 = d
        .iter()
        .zip(r.iter())
        .filter(|(x, _)| **x > 0.0)
        .map(|(_, rr)| *rr)
        .sum();
    let w_minus = nn * (nn + 1.0) / 2.0 - w_plus;
    let w = w_plus.min(w_minus);
    let mu = nn * (nn + 1.0) / 4.0;
    let sd = (nn * (nn + 1.0) * (2.0 * nn + 1.0) / 24.0).sqrt();
    if sd <= 0.0 {
        return None;
    }
    let z = (w - mu + 0.5) / sd;
    Some(WilcoxonResult {
        w,
        z,
        p: (2.0 * norm_sf_local(z.abs())).clamp(0.0, 1.0),
    })
}

#[derive(Clone)]
pub struct KwResult {
    pub h: f64,
    pub df: f64,
    pub p: f64,
}

/// Kruskal-Wallis H 检验，含并列校正。
pub fn kruskal_wallis(groups: &[Vec<f64>]) -> Option<KwResult> {
    let k = groups.len();
    if k < 2 {
        return None;
    }
    let all: Vec<f64> = groups.iter().flat_map(|g| g.iter().copied()).collect();
    let n = all.len() as f64;
    if n < 3.0 {
        return None;
    }
    let r = ranks(&all);
    let mut start = 0usize;
    let mut sum = 0.0f64;
    for g in groups {
        let rs: f64 = r[start..start + g.len()].iter().sum();
        sum += rs * rs / g.len() as f64;
        start += g.len();
    }
    let mut h = 12.0 / (n * (n + 1.0)) * sum - 3.0 * (n + 1.0);
    let mut sorted = all.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut tie = 0.0f64;
    let mut i = 0usize;
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
    if tie > 0.0 {
        h /= 1.0 - tie / (n.powi(3) - n);
    }
    let df = (k - 1) as f64;
    Some(KwResult { h, df, p: chi2_sf_local(h, df) })
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct FriedmanResult {
    pub q: f64,
    pub df: f64,
    pub p: f64,
}

/// Friedman 检验（区组设计，每行一个区组）。
#[allow(dead_code)]
pub fn friedman(blocks: &[Vec<f64>]) -> Option<FriedmanResult> {
    let b = blocks.len();
    if b < 2 {
        return None;
    }
    let k = blocks[0].len();
    if k < 2 || blocks.iter().any(|r| r.len() != k) {
        return None;
    }
    let mut rsum = vec![0.0f64; k];
    for row in blocks {
        let r = ranks(row);
        for (i, x) in r.iter().enumerate() {
            rsum[i] += x;
        }
    }
    let q = 12.0 / (b as f64 * k as f64 * (k as f64 + 1.0))
        * rsum.iter().map(|s| s * s).sum::<f64>()
        - 3.0 * b as f64 * (k as f64 + 1.0);
    let df = (k - 1) as f64;
    Some(FriedmanResult { q, df, p: chi2_sf_local(q, df) })
}

fn chi2_sf_local(x: f64, df: f64) -> f64 {
    if x <= 0.0 {
        return 1.0;
    }
    gamma_q(df / 2.0, x / 2.0)
}

#[derive(Clone)]
pub struct LeveneResult {
    pub f: f64,
    pub df1: f64,
    pub df2: f64,
    pub p: f64,
}

/// Levene 方差齐性检验（对「到组均值的绝对偏差」做单因素 ANOVA）。
/// Brown-Forsythe 则用中位数代替均值，对非正态更稳健。
pub fn levene(groups: &[Vec<f64>], use_median: bool) -> Option<LeveneResult> {
    let devs: Vec<Vec<f64>> = groups
        .iter()
        .map(|g| {
            let c = if use_median { median_of(g) } else { mean(g) };
            g.iter().map(|x| (x - c).abs()).collect()
        })
        .collect();
    let a = anova(&devs)?;
    Some(LeveneResult {
        f: a.f,
        df1: a.df_between,
        df2: a.df_within,
        p: a.p,
    })
}

fn median_of(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = s.len();
    if n == 0 {
        return 0.0;
    }
    if n % 2 == 1 {
        s[n / 2]
    } else {
        (s[n / 2 - 1] + s[n / 2]) / 2.0
    }
}

// ══════════════════════════════════════════════════════════════════════════
// 双因素 ANOVA（支持重复数不等）
// ══════════════════════════════════════════════════════════════════════════

#[derive(Clone)]
#[allow(dead_code)]
pub struct TwoWayResult {
    pub ss_rows: f64,
    pub ss_cols: f64,
    pub ss_inter: f64,
    pub ss_error: f64,
    pub df_rows: f64,
    pub df_cols: f64,
    pub df_inter: f64,
    pub df_error: f64,
    pub f_rows: f64,
    pub f_cols: f64,
    pub f_inter: f64,
    pub p_rows: f64,
    pub p_cols: f64,
    pub p_inter: f64,
    /// 各格重复数不完全相同 → true（此时交互项无法分离，走加性模型）
    pub unbalanced: bool,
}

/// 解线性方程组（高斯消元），用于最小二乘拟合。
fn solve(a: &[Vec<f64>], b: &[f64]) -> Option<Vec<f64>> {
    let n = b.len();
    let mut m: Vec<Vec<f64>> = a.iter().zip(b.iter()).map(|(r, y)| {
        let mut row = r.clone();
        row.push(*y);
        row
    }).collect();
    for c in 0..n {
        let mut piv = c;
        for r in c..n {
            if m[r][c].abs() > m[piv][c].abs() {
                piv = r;
            }
        }
        if m[piv][c].abs() < 1e-12 {
            return None;
        }
        m.swap(c, piv);
        for r in 0..n {
            if r == c {
                continue;
            }
            let factor = m[r][c] / m[c][c];
            for k in c..=n {
                m[r][k] -= factor * m[c][k];
            }
        }
    }
    Some((0..n).map(|i| m[i][n] / m[i][i]).collect())
}

/// 加性模型（无交互）的最小二乘：y = μ + α_i + β_j，参照编码。
/// 返回 (SSE, df_error)。
fn fit_additive(cells: &[Vec<Vec<f64>>], drop_row: bool, drop_col: bool) -> Option<(f64, f64)> {
    let r = cells.len();
    let c = cells[0].len();
    let mut p_dims = 1usize;
    if !drop_row {
        p_dims += r - 1;
    }
    if !drop_col {
        p_dims += c - 1;
    }
    let mut xs: Vec<Vec<f64>> = Vec::new();
    let mut ys: Vec<f64> = Vec::new();
    for (i, row) in cells.iter().enumerate() {
        for (j, cell) in row.iter().enumerate() {
            for y in cell {
                let mut x = vec![1.0f64];
                if !drop_row && i > 0 {
                    for k in 1..r {
                        x.push(if k == i { 1.0 } else { 0.0 });
                    }
                }
                if !drop_col && j > 0 {
                    for k in 1..c {
                        x.push(if k == j { 1.0 } else { 0.0 });
                    }
                }
                while x.len() < p_dims {
                    x.push(0.0);
                }
                xs.push(x);
                ys.push(*y);
            }
        }
    }
    let n = ys.len();
    if n <= p_dims {
        return None;
    }
    // 正规方程
    let mut a = vec![vec![0.0f64; p_dims]; p_dims];
    let mut b = vec![0.0f64; p_dims];
    for (x, y) in xs.iter().zip(ys.iter()) {
        for i in 0..p_dims {
            b[i] += x[i] * y;
            for j in 0..p_dims {
                a[i][j] += x[i] * x[j];
            }
        }
    }
    let beta = solve(&a, &b)?;
    let mut sse = 0.0f64;
    for (x, y) in xs.iter().zip(ys.iter()) {
        let pred: f64 = x.iter().zip(beta.iter()).map(|(a1, b1)| a1 * b1).sum();
        sse += (y - pred).powi(2);
    }
    Some((sse, (n - p_dims) as f64))
}

/// 双因素 ANOVA。`cells[r][c]` 为第 r 行第 c 列的重复值。
/// 各格重复数相等 → 可分离交互项；不等 → 退回加性模型（交互项不可估）。
pub fn two_way_anova(cells: &[Vec<Vec<f64>>]) -> Option<TwoWayResult> {
    let r = cells.len();
    if r < 2 || cells[0].is_empty() {
        return None;
    }
    let c = cells[0].len();
    if c < 2 || cells.iter().any(|row| row.len() != c) {
        return None;
    }
    let balanced = cells
        .iter()
        .flat_map(|row| row.iter())
        .map(|cell| cell.len())
        .collect::<Vec<_>>()
        .windows(2)
        .all(|w| w[0] == w[1]);

    let (ss_full, df_err) = fit_additive(cells, false, false)?;

    if !balanced {
        // 加性模型下用「去掉某因子」的额外平方和做主效应检验
        let (ss_no_row, _) = fit_additive(cells, true, false)?;
        let (ss_no_col, _) = fit_additive(cells, false, true)?;
        let df_rows = (r - 1) as f64;
        let df_cols = (c - 1) as f64;
        let ms_err = ss_full / df_err;
        let f_rows = ((ss_no_row - ss_full) / df_rows) / ms_err;
        let f_cols = ((ss_no_col - ss_full) / df_cols) / ms_err;
        return Some(TwoWayResult {
            ss_rows: ss_no_row - ss_full,
            ss_cols: ss_no_col - ss_full,
            ss_inter: 0.0,
            ss_error: ss_full,
            df_rows,
            df_cols,
            df_inter: 0.0,
            df_error: df_err,
            f_rows,
            f_cols,
            f_inter: f64::NAN,
            p_rows: f_p(f_rows, df_rows, df_err),
            p_cols: f_p(f_cols, df_cols, df_err),
            p_inter: f64::NAN,
            unbalanced: true,
        });
    }

    // 平衡：经典平方和分解
    let m = cells[0][0].len() as f64;
    let all: Vec<f64> = cells.iter().flat_map(|row| row.iter().flat_map(|x| x.iter().copied())).collect();
    let n = all.len() as f64;
    let grand = all.iter().sum::<f64>() / n;
    let row_means: Vec<f64> = cells
        .iter()
        .map(|row| {
            let v: Vec<f64> = row.iter().flat_map(|x| x.iter().copied()).collect();
            v.iter().sum::<f64>() / v.len() as f64
        })
        .collect();
    let col_means: Vec<f64> = (0..c)
        .map(|j| {
            let v: Vec<f64> = cells.iter().map(|row| row[j].clone()).flatten().collect();
            v.iter().sum::<f64>() / v.len() as f64
        })
        .collect();
    let ss_rows = c as f64 * m * row_means.iter().map(|x| (x - grand).powi(2)).sum::<f64>();
    let ss_cols = r as f64 * m * col_means.iter().map(|x| (x - grand).powi(2)).sum::<f64>();
    let mut ss_inter = 0.0f64;
    let mut ss_err = 0.0f64;
    for (i, row) in cells.iter().enumerate() {
        for (j, cell) in row.iter().enumerate() {
            let cm = cell.iter().sum::<f64>() / cell.len() as f64;
            ss_inter += m * (cm - row_means[i] - col_means[j] + grand).powi(2);
            ss_err += cell.iter().map(|x| (x - cm).powi(2)).sum::<f64>();
        }
    }
    let df_rows = (r - 1) as f64;
    let df_cols = (c - 1) as f64;
    let df_inter = df_rows * df_cols;
    let df_error = (r as f64) * (c as f64) * (m - 1.0);
    if df_error <= 0.0 || ss_err <= 0.0 {
        return None;
    }
    let ms_err = ss_err / df_error;
    let f_rows = (ss_rows / df_rows) / ms_err;
    let f_cols = (ss_cols / df_cols) / ms_err;
    let f_inter = if df_inter > 0.0 {
        (ss_inter / df_inter) / ms_err
    } else {
        f64::NAN
    };
    Some(TwoWayResult {
        ss_rows,
        ss_cols,
        ss_inter,
        ss_error: ss_err,
        df_rows,
        df_cols,
        df_inter,
        df_error,
        f_rows,
        f_cols,
        f_inter,
        p_rows: f_p(f_rows, df_rows, df_error),
        p_cols: f_p(f_cols, df_cols, df_error),
        p_inter: if df_inter > 0.0 {
            f_p(f_inter, df_inter, df_error)
        } else {
            f64::NAN
        },
        unbalanced: false,
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// 智能顾问所需的两个检验
// ═══════════════════════════════════════════════════════════════════════════

/// D'Agostino–Pearson K² 正态性检验。
///
/// 比 Shapiro–Wilk 更适合大样本：它基于**偏度 + 峰度**各构造一个近似正态
/// 统计量，二者平方和 K² 服从 χ²(2)，于是 `p = exp(-K²/2)` 可直接算出。
///
/// 偏度侧用 asinh 变换、峰度侧用 Anscombe–Glynn 的三阶矩校正，这两个变换
/// 让统计量在小样本下也更接近正态（直接用 Z=g1/sqrt(6/n) 在 n<20 时偏差明显）。
pub fn dagostino(v: &[f64]) -> Option<Normality> {
    let n = v.len() as f64;
    if n < 8.0 || !v.iter().all(|x| x.is_finite()) {
        return None;
    }
    let m = v.iter().sum::<f64>() / n;
    let mut m2 = 0.0;
    let mut m3 = 0.0;
    let mut m4 = 0.0;
    for &x in v {
        let d = x - m;
        let d2 = d * d;
        m2 += d2;
        m3 += d2 * d;
        m4 += d2 * d2;
    }
    m2 /= n;
    m3 /= n;
    m4 /= n;
    if m2 <= 0.0 {
        return None;
    }
    let b1 = m3 / m2.powf(1.5);
    let b2 = m4 / (m2 * m2);

    // ── 偏度侧 ──
    let y = b1 * ((n - 1.0) / n).sqrt();
    let num = 3.0 * (n * n + 27.0 * n - 70.0) * (n + 1.0) * (n + 2.0);
    let den = (n - 2.0) * (n + 5.0) * (n + 7.0) * (n + 9.0);
    let beta2 = num / den;
    let w2 = -1.0 + (2.0 * (beta2 - 1.0)).sqrt();
    let delta = 1.0 / (0.5 * w2.ln()).sqrt();
    let alpha = (2.0 / (w2 - 1.0)).sqrt();
    let ay = y / alpha;
    let zs = if y == 0.0 || !y.is_finite() {
        y * (((n + 1.0) * (n + 3.0)) / (6.0 * (n - 2.0))).sqrt()
    } else {
        delta * (ay + (ay * ay + 1.0).sqrt()).ln()
    };

    // ── 峰度侧（Anscombe–Glynn 三阶矩校正）──
    let e = 3.0 * (n - 1.0) / (n + 1.0);
    let varb2 = 24.0 * n * (n - 2.0) * (n - 3.0)
        / ((n + 1.0) * (n + 1.0) * (n + 3.0) * (n + 5.0));
    if varb2 <= 0.0 {
        return None;
    }
    let x = (b2 - e) / varb2.sqrt();
    let sqrtb1 = 6.0 * (n * n - 5.0 * n + 2.0) / ((n + 7.0) * (n + 9.0))
        * (6.0 * (n + 3.0) * (n + 5.0) / (n * (n - 2.0) * (n - 3.0))).sqrt();
    let a = 6.0 + 8.0 / sqrtb1 * (2.0 / sqrtb1 + (1.0 + 4.0 / (sqrtb1 * sqrtb1)).sqrt());
    let inner = 1.0 + x * (2.0 / (a - 4.0)).sqrt();
    if inner <= 0.0 || a <= 4.0 {
        return None;
    }
    let zk = ((1.0 - 2.0 / (9.0 * a)) - ((1.0 - 2.0 / a) / inner).cbrt())
        / (2.0 / (9.0 * a)).sqrt();

    let k2 = zs * zs + zk * zk;
    if !k2.is_finite() || k2 < 0.0 {
        return None;
    }
    // χ²(2) 的上尾概率有闭式解：exp(-K²/2)
    let p = (-0.5 * k2).exp();
    Some(Normality {
        w: k2,
        p,
        n: v.len(),
    })
}

/// Levene 方差齐性检验的 **p 值**。
///
/// 做法：把每个值换成「到本组均值的绝对偏差」，再对这些偏差做单因素 ANOVA。
/// 用**绝对**偏差而非平方偏差 —— 这是 Levene 与 Bartlett 的分界，
/// 绝对偏差对非正态数据稳健得多（Bartlett 对非正态极其敏感，会误判不齐）。
///
/// 检验不出时返回 `None`（组数 < 2、自由度不足、组内平方和为 0）。
pub fn levene_p(groups: &[Vec<f64>]) -> Option<f64> {
    // 每组内先过滤非有限值并剔除样本量 < 2 的组
    let gs: Vec<Vec<f64>> = groups
        .iter()
        .map(|g| g.iter().copied().filter(|v| v.is_finite()).collect())
        .filter(|g: &Vec<f64>| g.len() >= 2)
        .collect();
    let k = gs.len();
    if k < 2 {
        return None;
    }
    let devs: Vec<Vec<f64>> = gs
        .iter()
        .map(|g| {
            let m = g.iter().sum::<f64>() / g.len() as f64;
            g.iter().map(|v| (v - m).abs()).collect()
        })
        .collect();

    let n_all: usize = devs.iter().map(|d| d.len()).sum();
    let n = n_all as f64;
    let grand = devs.iter().flat_map(|d| d.iter()).copied().sum::<f64>() / n;

    let mut ss_b = 0.0;
    let mut ss_w = 0.0;
    for d in &devs {
        let m = d.iter().sum::<f64>() / d.len() as f64;
        ss_b += d.len() as f64 * (m - grand) * (m - grand);
        for &v in d {
            ss_w += (v - m) * (v - m);
        }
    }
    let df_b = (k - 1) as f64;
    let df_w = n - k as f64;
    if df_b < 1.0 || df_w < 1.0 || ss_w <= 0.0 {
        return None;
    }
    let f = (ss_b / df_b) / (ss_w / df_w);
    Some(f_p(f, df_b, df_w))
}

// ══════════════════════════════════════════════════════════════════════
// 第四章 · t 检验族：单样本 / 配对 / 独立 / Welch
// 规格：单样本、配对、独立均要求有效样本 n≥2
// ══════════════════════════════════════════════════════════════════════

/// 合并标准差 sp=√(((n1-1)s1²+(n2-1)s2²)/(n1+n2-2))
pub fn pooled_sd(s1: f64, n1: usize, s2: f64, n2: usize) -> f64 {
    if n1 + n2 <= 2 {
        return f64::NAN;
    }
    let df = (n1 + n2 - 2) as f64;
    ((n1.saturating_sub(1) as f64) * s1 * s1 + (n2.saturating_sub(1) as f64) * s2 * s2) / df
}

/// 单样本 t 检验结果
#[derive(Clone, Debug)]
pub struct OneSampleResult {
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

/// 单样本 t 检验：原假设锚定在 mu0。
/// t=(m-μ0)/SEM，df=n-1，CI=m ± t_crit·SEM。
/// 零方差时 SEM=0 → t 可能为 ±∞，P 为 NAN，调用层不应把 NAN 渲染成 0。
pub fn one_sample(x: &[f64], mu0: f64, conf: f64) -> Option<OneSampleResult> {
    let v: Vec<f64> = x.iter().copied().filter(|z| z.is_finite()).collect();
    if v.len() < 2 {
        return None;
    }
    let n = v.len();
    let d = describe(&v);
    let sem = d.sem;
    let t = if sem > 0.0 {
        (d.mean - mu0) / sem
    } else if (d.mean - mu0).abs() < 1e-15 {
        0.0
    } else {
        f64::INFINITY.copysign(d.mean - mu0)
    };
    let df = (n - 1) as f64;
    let p = t_p_two_tailed(t, df);
    let tc = t_crit_two(conf, df);
    Some(OneSampleResult {
        n,
        mean: d.mean,
        sd: d.sd,
        sem,
        t,
        df,
        p,
        ci_lo: d.mean - tc * sem,
        ci_hi: d.mean + tc * sem,
    })
}

/// 配对 t 检验结果（本质是差值的单样本检验，原假设差值固定为 0）
#[derive(Clone, Debug)]
pub struct PairedTResult {
    pub n: usize,
    pub mean_a: f64,
    pub mean_b: f64,
    pub sd_a: f64,
    pub sd_b: f64,
    pub sem_a: f64,
    pub sem_b: f64,
    pub mean_diff: f64,
    pub sd_diff: f64,
    pub sem_diff: f64,
    pub t: f64,
    pub df: f64,
    pub p: f64,
    pub ci_lo: f64,
    pub ci_hi: f64,
}

/// 配对 t 检验：取 min(len) 位置按元素求差，过滤非有限值后做单样本检验（μ0=0）。
/// 差值全同 → 方差为 0 → NAN。
pub fn paired_t(a: &[f64], b: &[f64], conf: f64) -> Option<PairedTResult> {
    let m = a.len().min(b.len());
    let d: Vec<f64> = (0..m)
        .map(|i| a[i] - b[i])
        .filter(|z| z.is_finite())
        .collect();
    if d.len() < 2 {
        return None;
    }
    let inner = one_sample(&d, 0.0, conf)?;
    let da = describe(&a[..m].iter().copied().filter(|z| z.is_finite()).collect::<Vec<_>>());
    let db = describe(&b[..m].iter().copied().filter(|z| z.is_finite()).collect::<Vec<_>>());
    Some(PairedTResult {
        n: inner.n,
        mean_a: da.mean,
        mean_b: db.mean,
        sd_a: da.sd,
        sd_b: db.sd,
        sem_a: da.sem,
        sem_b: db.sem,
        mean_diff: inner.mean,
        sd_diff: inner.sd,
        sem_diff: inner.sem,
        t: inner.t,
        df: inner.df,
        p: inner.p,
        ci_lo: inner.ci_lo,
        ci_hi: inner.ci_hi,
    })
}

// ══════════════════════════════════════════════════════════════════════
// 第五章 · 方差齐性：Bartlett 与 Brown-Forsythe
// ══════════════════════════════════════════════════════════════════════

/// Bartlett 方差齐性检验。要求每组合并方差 v>0，否则 None。
/// χ²=(df_w·ln(sp²)−Σ(ni−1)ln vi)/(1+(Σ1/(ni−1)−1/df_w)/(3(k−1)))，df=k-1
#[derive(Clone, Debug)]
pub struct BartlettResult {
    pub chi2: f64,
    pub df: f64,
    pub p: f64,
}

pub fn bartlett(groups: &[Vec<f64>]) -> Option<BartlettResult> {
    let gs: Vec<Vec<f64>> = groups
        .iter()
        .map(|g| g.iter().copied().filter(|z| z.is_finite()).collect())
        .filter(|g: &Vec<f64>| g.len() >= 2)
        .collect();
    let k = gs.len();
    if k < 2 {
        return None;
    }
    let mut vs: Vec<f64> = Vec::with_capacity(k);
    for g in &gs {
        let d = describe(g);
        if !(d.sd > 0.0) {
            return None;
        }
        vs.push(d.sd * d.sd);
    }
    let ns: Vec<usize> = gs.iter().map(|g| g.len()).collect();
    let n_total: usize = ns.iter().sum();
    let df_w = (n_total - k) as f64;
    if df_w <= 0.0 {
        return None;
    }
    // 合并方差 sp²
    let mut sp2 = 0.0;
    for i in 0..k {
        sp2 += (ns[i] - 1) as f64 * vs[i];
    }
    sp2 /= df_w;
    if !(sp2 > 0.0) {
        return None;
    }
    let mut num = df_w * sp2.ln();
    for i in 0..k {
        num -= (ns[i] - 1) as f64 * vs[i].ln();
    }
    let mut c = 0.0;
    for i in 0..k {
        c += 1.0 / (ns[i] - 1) as f64;
    }
    c -= 1.0 / df_w;
    let denom = 1.0 + c / (3.0 * (k - 1) as f64);
    let chi2 = num / denom;
    let df = (k - 1) as f64;
    Some(BartlettResult {
        chi2,
        df,
        p: chi2_sf_local(chi2, df),
    })
}

/// Brown-Forsythe：取每组中位数构造 z=|y−median|，再对 z 做一次单因素 ANOVA。
/// 只依赖中心趋势的稳健偏差，对非正态更宽容，建议作为默认显示项。
#[derive(Clone, Debug)]
pub struct BrownForsytheResult {
    pub f: f64,
    pub df1: f64,
    pub df2: f64,
    pub p: f64,
}

pub fn brown_forsythe(groups: &[Vec<f64>]) -> Option<BrownForsytheResult> {
    let zs: Vec<Vec<f64>> = groups
        .iter()
        .map(|g| {
            let v: Vec<f64> = g.iter().copied().filter(|z| z.is_finite()).collect();
            let d = describe(&v);
            v.iter().map(|x| (x - d.median).abs()).collect()
        })
        .filter(|z: &Vec<f64>| z.len() >= 2)
        .collect();
    let r = anova(&zs)?;
    Some(BrownForsytheResult {
        f: r.f,
        df1: r.df_between,
        df2: r.df_within,
        p: r.p,
    })
}

// ══════════════════════════════════════════════════════════════════════
// 第六章 · 正态性：KS-Lilliefors 与 Anderson-Darling
// 规格：固定种子、2000 次蒙特卡洛，P=(ge+1)/(B+1)，下限 n≥5
// ══════════════════════════════════════════════════════════════════════

/// 简易确定性伪随机（xorshift），保证「固定种子」可复算。
fn mc_rand(state: &mut u64) -> f64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    (x >> 11) as f64 / (1u64 << 53) as f64
}

/// 只要均值与样本标准差（n−1 分母）时的廉价版本。
///
/// 蒙特卡洛循环里必须用它而不是 `describe()`：describe 为了算均值置信区间
/// 会调 `t_crit_two()`，那是 100 次二分、每次一个不完全 beta（连分数最多
/// 200 轮），单次约两万次内层迭代。2000 次抽样 × 每组一次就是上亿次 ——
/// 这正是「点智能推荐就卡住」的原因。这里两个量都是 O(n) 闭式解。
pub fn mean_sd(v: &[f64]) -> (f64, f64) {
    let n = v.len();
    if n == 0 {
        return (0.0, 0.0);
    }
    let mean = v.iter().sum::<f64>() / n as f64;
    if n < 2 {
        return (mean, 0.0);
    }
    let var = v.iter().map(|z| (z - mean).powi(2)).sum::<f64>() / (n - 1) as f64;
    (mean, var.sqrt())
}

/// Box-Muller 生成标准正态样本（确定性）。
/// Lilliefors / Anderson-Darling 共用，固定种子因此结果可复现。
fn mc_normal(state: &mut u64) -> f64 {
    let u1 = mc_rand(state).max(1e-12);
    let u2 = mc_rand(state);
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// KS-Lilliefors 正态性检验。n≥5；统计量在零方差时返回 0。
pub fn lilliefors(v: &[f64]) -> Option<Normality> {
    let x: Vec<f64> = v.iter().copied().filter(|z| z.is_finite()).collect();
    let n = x.len();
    if n < 5 {
        return None;
    }
    // 只取均值与标准差 —— **不能调 describe()**。
    // describe 内部为算均值 CI 会调 t_crit_two()，那是 100 次二分 ×
    // 每次一个不完全 beta（连分数最多 200 轮），单次约两万次内层迭代。
    // 这里后面还有 2000 次蒙特卡洛 × 每组一次，乘起来是上亿次迭代 ——
    // 这正是「点智能推荐就卡住」的真正原因。蒙特卡洛只需要 mean / sd。
    let (d_mean, d_sd) = mean_sd(&x);
    let stat = if d_sd <= 0.0 {
        0.0
    } else {
        let mut s = x.clone();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mut dmax = 0.0f64;
        for (i, xi) in s.iter().enumerate() {
            let z = (*xi - d_mean) / d_sd;
            let fz = norm_cdf_local(z);
            let hi = ((i + 1) as f64 / n as f64 - fz).abs();
            let lo = (fz - i as f64 / n as f64).abs();
            dmax = dmax.max(hi.max(lo));
        }
        dmax
    };
    // 2000 次蒙特卡洛，固定种子
    let b = 2000usize;
    let mut state: u64 = 0x2545F4914F6CDD1D;
    let mut ge = 0usize;
    // 两块缓冲循环复用：原来每轮 `(0..n).map(...).collect()` + `samp.clone()`
    // 是 4000 次堆分配，光分配器就吃掉大半时间。
    let mut buf: Vec<f64> = vec![0.0; n];
    let mut sbuf: Vec<f64> = vec![0.0; n];
    for _ in 0..b {
        for slot in buf.iter_mut() {
            *slot = mc_normal(&mut state);
        }
        let (m, sd) = mean_sd(&buf);
        let st = if sd <= 0.0 {
            0.0
        } else {
            let s2 = &mut sbuf;
            s2.copy_from_slice(&buf);
            s2.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let mut dm = 0.0f64;
            for (i, xi) in s2.iter().enumerate() {
                let z = (*xi - m) / sd;
                let fz = norm_cdf_local(z);
                dm = dm.max(
                    ((i + 1) as f64 / n as f64 - fz)
                        .abs()
                        .max((fz - i as f64 / n as f64).abs()),
                );
            }
            dm
        };
        if st >= stat {
            ge += 1;
        }
    }
    Some(Normality {
        w: stat,
        p: (ge as f64 + 1.0) / (b as f64 + 1.0),
        n,
    })
}

/// Anderson-Darling 正态性检验。n≥5；零方差时统计量返回 0。
pub fn anderson_darling(v: &[f64]) -> Option<Normality> {
    let x: Vec<f64> = v.iter().copied().filter(|z| z.is_finite()).collect();
    let n = x.len();
    if n < 5 {
        return None;
    }
    // 同样只取 mean / sd（理由见 lilliefors：describe 里的 t_crit_two 太贵，
    // 而这里有 2000 次蒙特卡洛）
    let (ad_mean, ad_sd) = mean_sd(&x);
    let stat = if ad_sd <= 0.0 {
        0.0
    } else {
        let mut s = x.clone();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mut a2 = 0.0f64;
        for (i, xi) in s.iter().enumerate() {
            let z = (*xi - ad_mean) / ad_sd;
            let fz = norm_cdf_local(z).clamp(1e-12, 1.0 - 1e-12);
            let fzn = norm_cdf_local((s[n - 1 - i] - ad_mean) / ad_sd).clamp(1e-12, 1.0 - 1e-12);
            a2 += (2.0 * (i + 1) as f64 - 1.0) * (fz.ln() + (1.0 - fzn).ln());
        }
        (-(n as f64) - a2 / n as f64).max(0.0)
    };
    let b = 2000usize;
    let mut state: u64 = 0x9E3779B97F4A7C15;
    let mut ge = 0usize;
    // 同 lilliefors：复用缓冲，避免 4000 次堆分配。
    let mut buf: Vec<f64> = vec![0.0; n];
    let mut sbuf: Vec<f64> = vec![0.0; n];
    for _ in 0..b {
        for slot in buf.iter_mut() {
            *slot = mc_normal(&mut state);
        }
        let (m, sd) = mean_sd(&buf);
        let st = if sd <= 0.0 {
            0.0
        } else {
            let s2 = &mut sbuf;
            s2.copy_from_slice(&buf);
            s2.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let mut a = 0.0f64;
            for (i, xi) in s2.iter().enumerate() {
                let z = (*xi - m) / sd;
                let fz = norm_cdf_local(z).clamp(1e-12, 1.0 - 1e-12);
                let fzn =
                    norm_cdf_local((s2[n - 1 - i] - m) / sd).clamp(1e-12, 1.0 - 1e-12);
                a += (2.0 * (i + 1) as f64 - 1.0) * (fz.ln() + (1.0 - fzn).ln());
            }
            (-(n as f64) - a / n as f64).max(0.0)
        };
        if st >= stat {
            ge += 1;
        }
    }
    Some(Normality {
        w: stat,
        p: (ge as f64 + 1.0) / (b as f64 + 1.0),
        n,
    })
}

// ══════════════════════════════════════════════════════════════════════
// 第七章 · 过原点回归 / Deming / 斜率比较
// ══════════════════════════════════════════════════════════════════════

/// 扩展的线性回归结果（含过原点模型所需字段）
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct RegressionFull {
    pub slope: f64,
    /// 过原点模型下此字段为 NAN
    pub intercept: f64,
    pub se_slope: f64,
    /// 过原点模型下此字段为 NAN
    pub se_intercept: f64,
    pub r2: f64,
    pub adj_r2: f64,
    pub sse: f64,
    pub sst: f64,
    pub syx: f64,
    pub df: f64,
    pub f: f64,
    pub p: f64,
    pub n: usize,
    pub through_origin: bool,
}

/// 线性回归。过原点作为独立模型，而不是删截距项的语法糖：
/// - 普通：slope=Sxy/Sxx、intercept=ȳ−slope·x̄、df=n−2、SST=Σ(y−ȳ)²
/// - 过原点：slope=Σxy/Σx²、df=n−1、SST=Σy²、截距 NAN
pub fn regression_full(xs: &[f64], ys: &[f64], through_origin: bool) -> Option<RegressionFull> {
    let m = xs.len().min(ys.len());
    let px: Vec<f64> = (0..m)
        .map(|i| xs[i])
        .zip((0..m).map(|i| ys[i]))
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .map(|(x, _)| x)
        .collect();
    let py: Vec<f64> = (0..m)
        .map(|i| xs[i])
        .zip((0..m).map(|i| ys[i]))
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .map(|(_, y)| y)
        .collect();
    let n = px.len();
    if n < 3 {
        return None;
    }
    let mx = px.iter().sum::<f64>() / n as f64;
    let my = py.iter().sum::<f64>() / n as f64;

    let (slope, intercept, sse, sst, df) = if through_origin {
        let sxx0 = px.iter().map(|x| x * x).sum::<f64>();
        if !(sxx0 > 0.0) {
            return None;
        }
        let sxy0 = px.iter().zip(py.iter()).map(|(x, y)| x * y).sum::<f64>();
        let b = sxy0 / sxx0;
        let e = px
            .iter()
            .zip(py.iter())
            .map(|(x, y)| (y - b * x).powi(2))
            .sum::<f64>();
        (b, f64::NAN, e, py.iter().map(|y| y * y).sum::<f64>(), (n - 1) as f64)
    } else {
        let sxx = px.iter().map(|x| (x - mx).powi(2)).sum::<f64>();
        let sxy = px
            .iter()
            .zip(py.iter())
            .map(|(x, y)| (x - mx) * (y - my))
            .sum::<f64>();
        if !(sxx > 0.0) {
            return None;
        }
        let b = sxy / sxx;
        let a = my - b * mx;
        let e = px
            .iter()
            .zip(py.iter())
            .map(|(x, y)| (y - a - b * x).powi(2))
            .sum::<f64>();
        (
            b,
            a,
            e,
            py.iter().map(|y| (y - my).powi(2)).sum::<f64>(),
            (n - 2) as f64,
        )
    };
    if df <= 0.0 {
        return None;
    }
    let syx = (sse / df).sqrt();
    let sxx_c = if through_origin {
        px.iter().map(|x| x * x).sum::<f64>()
    } else {
        px.iter().map(|x| (x - mx).powi(2)).sum::<f64>()
    };
    let se_slope = if sxx_c > 0.0 { syx / sxx_c.sqrt() } else { f64::NAN };
    let se_intercept = if through_origin || sxx_c <= 0.0 {
        f64::NAN
    } else {
        syx * (1.0 / n as f64 + mx * mx / sxx_c).sqrt()
    };
    let r2 = if sst > 0.0 { 1.0 - sse / sst } else { f64::NAN };
    let k = if through_origin { 1.0 } else { 2.0 };
    let adj_r2 = if r2.is_finite() && n as f64 > k {
        1.0 - (1.0 - r2) * (n as f64 - 1.0) / (n as f64 - k)
    } else {
        f64::NAN
    };
    // 模型 F 检验：对单参数斜率检验使用 df1=1
    let mse_reg = if through_origin {
        sst - sse
    } else {
        sst - sse
    };
    let f = if sse > 0.0 && df > 0.0 {
        (mse_reg / 1.0) / (sse / df)
    } else {
        f64::NAN
    };
    let p = if f.is_finite() && f > 0.0 {
        f_p(f, 1.0, df)
    } else {
        f64::NAN
    };
    Some(RegressionFull {
        slope,
        intercept,
        se_slope,
        se_intercept,
        r2,
        adj_r2,
        sse,
        sst,
        syx,
        df,
        f,
        p,
        n,
        through_origin,
    })
}

/// Deming 回归结果（X、Y 均有误差的对称处理）
#[derive(Clone, Debug)]
pub struct DemingResult {
    pub slope: f64,
    pub intercept: f64,
    pub n: usize,
}

/// Deming 回归：λ 会实质改变斜率估计，默认 1 表示 X、Y 误差方差相当。
/// slope=(inner+√(inner²+4λ·sxy²))/(2·sxy)，inner=syy−λ·sxx
pub fn deming(xs: &[f64], ys: &[f64], lambda: f64) -> Option<DemingResult> {
    let lam = lambda.max(1e-9);
    let m = xs.len().min(ys.len());
    let px: Vec<f64> = (0..m).map(|i| xs[i]).collect();
    let py: Vec<f64> = (0..m).map(|i| ys[i]).collect();
    let keep: Vec<usize> = (0..m)
        .filter(|&i| px[i].is_finite() && py[i].is_finite())
        .collect();
    let n = keep.len();
    if n < 3 {
        return None;
    }
    let x: Vec<f64> = keep.iter().map(|&i| px[i]).collect();
    let y: Vec<f64> = keep.iter().map(|&i| py[i]).collect();
    let mx = x.iter().sum::<f64>() / n as f64;
    let my = y.iter().sum::<f64>() / n as f64;
    let sxx = x.iter().map(|v| (v - mx).powi(2)).sum::<f64>() / (n - 1) as f64;
    let syy = y.iter().map(|v| (v - my).powi(2)).sum::<f64>() / (n - 1) as f64;
    let sxy = x
        .iter()
        .zip(y.iter())
        .map(|(a, b)| (a - mx) * (b - my))
        .sum::<f64>()
        / (n - 1) as f64;
    if !(sxy.abs() > 0.0) {
        return None;
    }
    let inner = syy - lam * sxx;
    let slope = (inner + (inner * inner + 4.0 * lam * sxy * sxy).sqrt()) / (2.0 * sxy);
    Some(DemingResult {
        slope,
        intercept: my - slope * mx,
        n,
    })
}

/// 两组斜率比较（ANCOVA 式合并误差项）
#[derive(Clone, Debug)]
pub struct SlopeCompareResult {
    pub slope1: f64,
    pub slope2: f64,
    pub diff: f64,
    pub se: f64,
    pub t: f64,
    pub df: f64,
    pub p: f64,
    /// 共同斜率（约束模型）
    pub common_slope: f64,
    /// 截距相等性检验（注意：自由度为 2 与 n−4，与常规单截距差比较口径不同）
    pub f_intercept: f64,
    pub p_equal_intercept: f64,
}

/// 斜率比较：每组至少 3 个有效点；先分别拟合，再用两组合并 SSE 构造合并 syx²，
/// SE=√(syx²/Sxx1+syx²/Sxx2)，t=(slope1−slope2)/SE，df=df1+df2。
pub fn compare_slopes(
    x1: &[f64],
    y1: &[f64],
    x2: &[f64],
    y2: &[f64],
) -> Option<SlopeCompareResult> {
    let r1 = regression_full(x1, y1, false)?;
    let r2 = regression_full(x2, y2, false)?;
    if r1.n < 3 || r2.n < 3 {
        return None;
    }
    let df1 = r1.df;
    let df2 = r2.df;
    if df1 + df2 <= 0.0 {
        return None;
    }
    let syx2 = (r1.sse + r2.sse) / (df1 + df2);
    let sxx1 = {
        let v: Vec<f64> = x1.iter().copied().filter(|z| z.is_finite()).collect();
        let m = v.iter().sum::<f64>() / v.len() as f64;
        v.iter().map(|z| (z - m).powi(2)).sum::<f64>()
    };
    let sxx2 = {
        let v: Vec<f64> = x2.iter().copied().filter(|z| z.is_finite()).collect();
        let m = v.iter().sum::<f64>() / v.len() as f64;
        v.iter().map(|z| (z - m).powi(2)).sum::<f64>()
    };
    if !(sxx1 > 0.0) || !(sxx2 > 0.0) {
        return None;
    }
    let se = (syx2 / sxx1 + syx2 / sxx2).sqrt();
    let diff = r1.slope - r2.slope;
    let t = if se > 0.0 { diff / se } else { f64::NAN };
    let df = df1 + df2;
    let p = if t.is_finite() { t_p_two_tailed(t, df) } else { f64::NAN };
    // 共同斜率：两组各自中心化后 ΣSxy/ΣSxx
    let c1 = centered_sums(x1, y1);
    let c2 = centered_sums(x2, y2);
    let num = c1.2 + c2.2;
    let den = c1.1 + c2.1;
    let common_slope = if den > 0.0 { num / den } else { f64::NAN };
    // 截距比较：两组合并模型 SSE 与分别拟合 SSE 之差
    let combined_sse = if common_slope.is_finite() {
        let i1 = c1.0 .1 - common_slope * c1.0 .0;
        let i2 = c2.0 .1 - common_slope * c2.0 .0;
        x1.iter()
            .zip(y1.iter())
            .map(|(x, y)| (y - i1 - common_slope * x).powi(2))
            .sum::<f64>()
            + x2
                .iter()
                .zip(y2.iter())
                .map(|(x, y)| (y - i2 - common_slope * x).powi(2))
                .sum::<f64>()
    } else {
        f64::NAN
    };
    let f_int = if combined_sse.is_finite() && syx2 > 0.0 {
        ((combined_sse - (r1.sse + r2.sse)) / 2.0) / syx2
    } else {
        f64::NAN
    };
    let p_int = if f_int.is_finite() && f_int > 0.0 {
        f_p(f_int, 2.0, (r1.n + r2.n - 4) as f64)
    } else {
        f64::NAN
    };
    Some(SlopeCompareResult {
        slope1: r1.slope,
        slope2: r2.slope,
        diff,
        se,
        t,
        df,
        p,
        common_slope,
        f_intercept: f_int,
        p_equal_intercept: p_int,
    })
}

/// 返回 ((&x均值,&y均值), Σ(x−x̄)², Σ(x−x̄)(y−ȳ)) 的辅助：
/// 实际返回 ((mx, my), sxx, sxy, n)
fn centered_sums(xs: &[f64], ys: &[f64]) -> ((f64, f64), f64, f64, usize) {
    let n = xs.len().min(ys.len());
    let v: Vec<(f64, f64)> = (0..n)
        .map(|i| (xs[i], ys[i]))
        .filter(|(a, b)| a.is_finite() && b.is_finite())
        .collect();
    let nx = v.len();
    if nx == 0 {
        return ((0.0, 0.0), 0.0, 0.0, 0);
    }
    let mx = v.iter().map(|(a, _)| a).sum::<f64>() / nx as f64;
    let my = v.iter().map(|(_, b)| b).sum::<f64>() / nx as f64;
    let sxx = v.iter().map(|(a, _)| (a - mx).powi(2)).sum::<f64>();
    let sxy = v
        .iter()
        .map(|(a, b)| (a - mx) * (b - my))
        .sum::<f64>();
    ((mx, my), sxx, sxy, nx)
}

// ══════════════════════════════════════════════════════════════════════
// 第九章 · Log-rank（Mantel-Cox）
// 分母采用时点总风险数而非组间风险数乘积
// ══════════════════════════════════════════════════════════════════════

/// Log-rank 检验结果
#[derive(Clone, Debug)]
pub struct LogRankResult {
    pub chi2: f64,
    pub df: f64,
    pub p: f64,
    /// 各组的风险比 HR=O/E；某组 E=0 时为 NAN
    pub hr: Vec<f64>,
}

/// Log-rank（Mantel-Cox）。至少 2 组；合并所有事件时间并去重。
/// 期望 E_g=d_total·n_risk[g]/n_total；
/// 方差=d_total(n_total−d_total)n_total/(n_total²(n_total−1))
pub fn log_rank(groups: &[(Vec<f64>, Vec<f64>)]) -> Option<LogRankResult> {
    let k = groups.len();
    if k < 2 {
        return None;
    }
    // 收集所有事件时间（event=1 表示事件）
    let mut times: Vec<f64> = Vec::new();
    for (t, e) in groups {
        for i in 0..t.len().min(e.len()) {
            if t[i].is_finite() && e[i] > 0.5 {
                if !times.iter().any(|x| (*x - t[i]).abs() < 1e-12) {
                    times.push(t[i]);
                }
            }
        }
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if times.is_empty() {
        return None;
    }
    let mut chi2 = 0.0f64;
    let mut obs: Vec<f64> = vec![0.0; k];
    let mut exp: Vec<f64> = vec![0.0; k];
    for t in &times {
        let mut n_risk = vec![0.0f64; k];
        let mut d_g = vec![0.0f64; k];
        for g in 0..k {
            let (tt, ee) = &groups[g];
            for i in 0..tt.len().min(ee.len()) {
                if !tt[i].is_finite() {
                    continue;
                }
                if tt[i] >= *t - 1e-12 {
                    n_risk[g] += 1.0;
                    if (tt[i] - *t).abs() < 1e-12 && ee[i] > 0.5 {
                        d_g[g] += 1.0;
                    }
                }
            }
        }
        let n_total: f64 = n_risk.iter().sum();
        let d_total: f64 = d_g.iter().sum();
        if n_total < 2.0 || d_total == 0.0 {
            continue;
        }
        let var = d_total * (n_total - d_total) * n_total
            / (n_total * n_total * (n_total - 1.0)).max(1e-12);
        for g in 0..k {
            let e = d_total * n_risk[g] / n_total;
            obs[g] += d_g[g];
            exp[g] += e;
            chi2 += (d_g[g] - e).powi(2) / var.max(1e-12);
        }
    }
    let df = (k - 1) as f64;
    let hr = obs
        .iter()
        .zip(exp.iter())
        .map(|(o, e)| if *e > 0.0 { o / e } else { f64::NAN })
        .collect();
    Some(LogRankResult {
        chi2,
        df,
        p: chi2_sf_local(chi2, df),
        hr,
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// ROC 曲线 / Cox 回归 / PCA / 层次聚类 / K-means
// 这五个此前只弹「未实现」，现在全部给出真实数值。
// ═══════════════════════════════════════════════════════════════════════════

/// ROC 曲线上的一个点。
#[derive(Clone, Debug)]
pub struct RocPoint {
    /// 1 − 特异度（假阳性率）
    pub fpr: f64,
    /// 敏感度（真阳性率）
    pub tpr: f64,
    /// 该点对应的判定阈值
    pub threshold: f64,
}

/// ROC 分析的结果。
#[derive(Clone, Debug)]
pub struct RocResult {
    pub points: Vec<RocPoint>,
    /// 曲线下面积（梯形法，等价于 Mann-Whitney U 统计量）
    pub auc: f64,
    /// AUC 的标准误（Hanley–McNeil）
    pub se: f64,
    /// AUC 的 95% CI
    pub ci_lo: f64,
    pub ci_hi: f64,
    /// Youden 指数最大的切点（J = max(TPR − FPR)）
    pub best_threshold: f64,
    pub best_sens: f64,
    pub best_spec: f64,
    pub n_pos: usize,
    pub n_neg: usize,
}

/// ROC 曲线与 AUC。
///
/// **关于数据来源**：项目的数据模型没有独立的「二分类真值标签列」，
/// 所以这里沿用诊断试验的标准简化口径——把**两组分数分布**当作
/// 阳性组与阴性组的预测分数（`pos` = 阳性，`neg` = 阴性）。
/// 这在统计上是完备的：AUC 恰好等于 Mann-Whitney U 统计量除以 n₁·n₂，
/// 也就是「随机抽一个阳性、一个阴性，阳性分数更高的概率」。
///
/// 曲线由所有 distinct 阈值扫描得到，首尾补 (0,0) 与 (1,1)；
/// AUC 用梯形法积分；SE 用 Hanley–McNeil 公式；CI 用正态近似并夹到 [0,1]。
pub fn roc_curve(pos: &[f64], neg: &[f64]) -> Option<RocResult> {
    let p: Vec<f64> = pos.iter().copied().filter(|v| v.is_finite()).collect();
    let n: Vec<f64> = neg.iter().copied().filter(|v| v.is_finite()).collect();
    if p.is_empty() || n.is_empty() {
        return None;
    }
    let n_pos = p.len();
    let n_neg = n.len();

    // 候选阈值：所有出现过的值，降序扫描（阈值越高，判阳性的越少）
    let mut thr: Vec<f64> = p.iter().chain(n.iter()).copied().collect();
    thr.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    thr.dedup();

    let mut points = Vec::with_capacity(thr.len() + 2);
    points.push(RocPoint { fpr: 0.0, tpr: 0.0, threshold: f64::INFINITY });
    for &t in thr.iter().rev() {
        let tp = p.iter().filter(|v| **v >= t).count() as f64;
        let fp = n.iter().filter(|v| **v >= t).count() as f64;
        points.push(RocPoint {
            fpr: fp / n_neg as f64,
            tpr: tp / n_pos as f64,
            threshold: t,
        });
    }
    points.push(RocPoint {
        fpr: 1.0,
        tpr: 1.0,
        threshold: f64::NEG_INFINITY,
    });

    // 梯形法积分。点已按 fpr 单调不减排序（阈值降序 → 两者同时不减）。
    let mut auc = 0.0;
    for w in points.windows(2) {
        let dx = w[1].fpr - w[0].fpr;
        auc += dx * (w[0].tpr + w[1].tpr) * 0.5;
    }

    // Hanley–McNeil 标准误
    let q1 = auc / (2.0 - auc);
    let q2 = 2.0 * auc * auc / (1.0 + auc);
    let var = (auc * (1.0 - auc)
        + (n_pos as f64 - 1.0) * (q1 - auc * auc)
        + (n_neg as f64 - 1.0) * (q2 - auc * auc))
        / (n_pos as f64 * n_neg as f64);
    let se = var.max(0.0).sqrt();
    let z = 1.959963984540054;
    let ci_lo = (auc - z * se).clamp(0.0, 1.0);
    let ci_hi = (auc + z * se).clamp(0.0, 1.0);

    // Youden 指数 J = TPR − FPR 最大的点。
    // 跳过首尾两个人工点（它们的阈值是 ±∞，没有实际判定意义）；
    // 只有在完全没有中间点时才回退到端点。
    let mut best_j = f64::NEG_INFINITY;
    let mut best_threshold = points[0].threshold;
    let mut best_tpr = points[0].tpr;
    let mut best_fpr = points[0].fpr;
    let inner = if points.len() > 2 { &points[1..points.len() - 1] } else { &points[..] };
    for pt in inner {
        let j = pt.tpr - pt.fpr;
        if j > best_j {
            best_j = j;
            best_threshold = pt.threshold;
            best_tpr = pt.tpr;
            best_fpr = pt.fpr;
        }
    }

    Some(RocResult {
        points,
        auc,
        se,
        ci_lo,
        ci_hi,
        best_threshold,
        best_sens: best_tpr,
        best_spec: 1.0 - best_fpr,
        n_pos,
        n_neg,
    })
}

/// 单个协变量的 Cox 结果。
#[derive(Clone, Debug)]
pub struct CoxVar {
    pub name: String,
    /// 回归系数 β
    pub beta: f64,
    /// 风险比 exp(β)
    pub hr: f64,
    pub se: f64,
    /// Wald z
    pub z: f64,
    pub p: f64,
    pub ci_lo: f64,
    pub ci_hi: f64,
}

/// Cox 比例风险回归的结果。
#[derive(Clone, Debug)]
pub struct CoxResult {
    pub vars: Vec<CoxVar>,
    /// 偏似然对数
    pub loglik: f64,
    /// 事件数
    pub n_events: usize,
    /// 样本数
    pub n: usize,
    /// 是否收敛
    pub converged: bool,
}

/// Cox 比例风险回归（Breslow 处理并列，Newton–Raphson 求解）。
///
/// **关于数据来源**：数据模型没有独立的事件指示列，
/// 这里按生存分析的约定：`time` 为随访时间，`event` 为事件指示
/// （非 0 视为事件发生，0 视为删失）。调用方传入的事件列由表的
/// 数据集提供；若完全缺失则视为全部发生事件，并在结果里如实标注。
///
/// 偏似然：`L = Π_i [exp(x_i·β) / Σ_{j∈R(t_i)} exp(x_j·β)]^d_i`
/// 得分（一阶导）与 Hessian（二阶导）都用风险集求和，
/// Newton–Raphson 迭代，阻尼保证不发散，最多 100 轮。
pub fn cox_ph(
    time: &[f64],
    event: &[f64],
    cols: &[(String, Vec<f64>)],
) -> Option<CoxResult> {
    let k = cols.len();
    if k == 0 || time.is_empty() {
        return None;
    }
    let n = time.len();
    // 组装设计矩阵，只保留所有协变量都有限的样本
    let mut idx: Vec<usize> = Vec::new();
    for i in 0..n {
        if !time[i].is_finite() {
            continue;
        }
        if cols.iter().all(|(_, v)| v.get(i).map(|x| x.is_finite()).unwrap_or(false)) {
            idx.push(i);
        }
    }
    if idx.len() <= k {
        return None;
    }
    let ev: Vec<f64> = idx.iter().map(|&i| if event.get(i).copied().unwrap_or(1.0) != 0.0 { 1.0 } else { 0.0 }).collect();
    let n_events = ev.iter().filter(|&&e| e > 0.0).count();
    if n_events == 0 {
        return None;
    }
    let tt: Vec<f64> = idx.iter().map(|&i| time[i]).collect();
    let x: Vec<Vec<f64>> = (0..k)
        .map(|c| idx.iter().map(|&i| cols[c].1[i]).collect::<Vec<f64>>())
        .collect();

    let m = idx.len();
    let mut beta = vec![0.0f64; k];
    let mut converged = false;
    let mut loglik = 0.0;

    for _iter in 0..100 {
        // 按时间降序确定风险集：R(t_i) = { j : t_j >= t_i }
        let mut order: Vec<usize> = (0..m).collect();
        order.sort_by(|&a, &b| tt[b].partial_cmp(&tt[a]).unwrap_or(std::cmp::Ordering::Equal));

        let e: Vec<f64> = (0..m).map(|i| (x.iter().map(|xc| xc[i]).zip(beta.iter()).map(|(v, b)| v * b).sum::<f64>()).exp()).collect();

        // 风险集累积量（按时间降序累加）
        let mut s0 = vec![0.0f64; m];   // Σ exp
        let mut s1 = vec![vec![0.0f64; k]; m]; // Σ x·exp
        let mut s2 = vec![vec![vec![0.0f64; k]; k]; m];
        let mut acc0 = 0.0;
        let mut acc1 = vec![0.0f64; k];
        let mut acc2 = vec![vec![0.0f64; k]; k];
        for &i in &order {
            acc0 += e[i];
            for c in 0..k { acc1[c] += x[c][i] * e[i]; }
            for c in 0..k { for d in 0..k { acc2[c][d] += x[c][i] * x[d][i] * e[i]; } }
            s0[i] = acc0;
            s1[i] = acc1.clone();
            for c in 0..k { for d in 0..k { s2[i][c][d] = acc2[c][d]; } }
        }

        // 对数偏似然
        loglik = 0.0;
        for i in 0..m {
            if ev[i] > 0.0 && s0[i] > 0.0 {
                let eta: f64 = (0..k).map(|c| x[c][i] * beta[c]).sum();
                loglik += ev[i] * (eta - s0[i].ln());
            }
        }

        // 得分向量 U 与信息矩阵 I
        let mut u = vec![0.0f64; k];
        let mut info = vec![vec![0.0f64; k]; k];
        for i in 0..m {
            if ev[i] == 0.0 || s0[i] <= 0.0 {
                continue;
            }
            for c in 0..k {
                u[c] += ev[i] * (x[c][i] - s1[i][c] / s0[i]);
            }
            let a1: Vec<f64> = (0..k).map(|c| s1[i][c] / s0[i]).collect();
            for c in 0..k {
                for d in 0..k {
                    info[c][d] += ev[i] * (s2[i][c][d] / s0[i] - a1[c] * a1[d]);
                }
            }
        }

        // 解 I·δ = U（高斯-约当），失败则用对角阻尼
        let delta = match solve_linear(&info, &u) {
            Some(d) => d,
            None => {
                let mut d = vec![0.0f64; k];
                for c in 0..k {
                    let denom = info[c][c].abs().max(1e-9);
                    d[c] = u[c] / denom;
                }
                d
            }
        };
        let step: f64 = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
        for c in 0..k {
            beta[c] += delta[c];
        }
        if step < 1e-8 {
            converged = true;
            break;
        }
    }

    // 最后一次迭代的方差：用主对角元的逆近似（单变量时精确）
    let mut vars = Vec::with_capacity(k);
    for c in 0..k {
        let mut se = f64::NAN;
        if let Some(inv) = invert_matrix(&info_last(&tt, &x, &ev, &beta, k)) {
            let v = inv[c][c];
            if v > 0.0 && v.is_finite() {
                se = v.sqrt();
            }
        }
        let b = beta[c];
        let hr = b.exp();
        let z = if se > 0.0 && se.is_finite() { b / se } else { f64::NAN };
        let p = if z.is_finite() { 2.0 * (1.0 - norm_cdf_local(z.abs())) } else { f64::NAN };
        let (lo, hi) = if se.is_finite() {
            ((b - 1.959963984540054 * se).exp(), (b + 1.959963984540054 * se).exp())
        } else {
            (f64::NAN, f64::NAN)
        };
        vars.push(CoxVar {
            name: cols[c].0.clone(),
            beta: b,
            hr,
            se,
            z,
            p,
            ci_lo: lo,
            ci_hi: hi,
        });
    }

    Some(CoxResult { vars, loglik, n_events, n, converged })
}

/// 在给定 β 下重算信息矩阵（用于标准误）。
///
/// **必须传入时间**：风险集由时间决定，只用下标排序会算出错误的信息矩阵，
/// 进而给出错误的标准误——这正是回归诊断里最难发现的一类 bug。
fn info_last(
    tt: &[f64],
    x: &[Vec<f64>],
    ev: &[f64],
    beta: &[f64],
    k: usize,
) -> Vec<Vec<f64>> {
    let m = x[0].len();
    let mut order: Vec<usize> = (0..m).collect();
    // 时间降序：先累加的是时间更晚的个体，即风险集 R(t_i) = { j : t_j >= t_i }
    order.sort_by(|&a, &b| tt[b].partial_cmp(&tt[a]).unwrap_or(std::cmp::Ordering::Equal));
    let e: Vec<f64> = (0..m).map(|i| (0..k).map(|c| x[c][i] * beta[c]).sum::<f64>().exp()).collect();
    let mut info = vec![vec![0.0f64; k]; k];
    let mut acc0 = 0.0;
    let mut acc1 = vec![0.0f64; k];
    let mut acc2 = vec![vec![0.0f64; k]; k];
    let mut s0 = vec![0.0f64; m];
    let mut s1 = vec![vec![0.0f64; k]; m];
    let mut s2 = vec![vec![vec![0.0f64; k]; k]; m];
    for &i in &order {
        acc0 += e[i];
        for c in 0..k { acc1[c] += x[c][i] * e[i]; }
        for c in 0..k { for d in 0..k { acc2[c][d] += x[c][i] * x[d][i] * e[i]; } }
        s0[i] = acc0; s1[i] = acc1.clone();
        for c in 0..k { for d in 0..k { s2[i][c][d] = acc2[c][d]; } }
    }
    for i in 0..m {
        if ev[i] == 0.0 || s0[i] <= 0.0 { continue; }
        let a1: Vec<f64> = (0..k).map(|c| s1[i][c] / s0[i]).collect();
        for c in 0..k {
            for d in 0..k { info[c][d] += ev[i] * (s2[i][c][d] / s0[i] - a1[c] * a1[d]); }
        }
    }
    info
}

/// 高斯-约当解线性方程组 A·x = b。
pub fn solve_linear(a: &[Vec<f64>], b: &[f64]) -> Option<Vec<f64>> {
    let n = b.len();
    if a.len() != n || a.iter().any(|r| r.len() != n) {
        return None;
    }
    let mut m: Vec<Vec<f64>> = a.iter().map(|r| r.to_vec()).collect();
    let mut rhs = b.to_vec();
    for c in 0..n {
        // 选主元
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
    if rhs.iter().all(|v| v.is_finite()) {
        Some(rhs)
    } else {
        None
    }
}

/// 高斯-约当求逆。
fn invert_matrix(a: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = a.len();
    if n == 0 || a.iter().any(|r| r.len() != n) {
        return None;
    }
    let mut m: Vec<Vec<f64>> = a.iter().map(|r| r.to_vec()).collect();
    let mut inv = vec![vec![0.0f64; n]; n];
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
    if inv.iter().all(|r| r.iter().all(|v| v.is_finite())) {
        Some(inv)
    } else {
        None
    }
}

/// 主成分分析的结果。
#[derive(Clone, Debug)]
pub struct PcaResult {
    /// 各主成分解释的方差（特征值），降序
    pub eigen: Vec<f64>,
    /// 解释方差比例
    pub proportion: Vec<f64>,
    /// 累积解释比例
    pub cumulative: Vec<f64>,
    /// 载荷矩阵：`loadings[var][pc]`
    pub loadings: Vec<Vec<f64>>,
    /// 各主成分的得分：`scores[pc][obs]`（已中心化）
    pub scores: Vec<Vec<f64>>,
    pub n: usize,
    pub n_vars: usize,
}

/// 主成分分析（相关矩阵或协方差矩阵 + Jacobi 特征值分解）。
///
/// **为什么自己实现 Jacobi**：nalgebra / ndarray 都要新增重量级依赖，
/// 而 PCA 只需要对称矩阵的特征分解——Jacobi 旋转对对称矩阵稳定、
/// 收敛快、代码可控，且能同时给出特征向量。
///
/// `use_corr = true` 时对标准化后的数据求解（即相关矩阵），
/// 这是变量量纲不一致时的正确选择，也是统计软件（Prism/SPSS）的默认。
pub fn pca(cols: &[Vec<f64>], use_corr: bool) -> Option<PcaResult> {
    let k = cols.len();
    if k < 2 {
        return None;
    }
    let n = cols.iter().map(|c| c.len()).min().unwrap_or(0);
    if n < 3 {
        return None;
    }
    // 只保留所有变量都有限的观测
    let rows: Vec<usize> = (0..n)
        .filter(|&i| cols.iter().all(|c| c[i].is_finite()))
        .collect();
    if rows.len() < 3 {
        return None;
    }
    let m = rows.len();

    // 中心化（相关矩阵时再做标准化）
    let mean: Vec<f64> = cols.iter().map(|c| rows.iter().map(|&i| c[i]).sum::<f64>() / m as f64).collect();
    let sd: Vec<f64> = cols
        .iter()
        .zip(mean.iter())
        .map(|(c, mu)| {
            let v = rows.iter().map(|&i| (c[i] - mu).powi(2)).sum::<f64>() / (m - 1) as f64;
            v.sqrt()
        })
        .collect();
    let scale: Vec<f64> = (0..k)
        .map(|c| if use_corr && sd[c] > 0.0 { sd[c] } else { 1.0 })
        .collect();

    let z: Vec<Vec<f64>> = (0..k)
        .map(|c| rows.iter().map(|&i| (cols[c][i] - mean[c]) / scale[c]).collect())
        .collect();

    // 协方差/相关矩阵
    let denom = if use_corr { (m - 1) as f64 } else { (m - 1) as f64 };
    let mut cov = vec![vec![0.0f64; k]; k];
    for a in 0..k {
        for b in 0..k {
            cov[a][b] = (0..m).map(|i| z[a][i] * z[b][i]).sum::<f64>() / denom;
        }
    }

    // Jacobi 特征值分解
    let (mut eigen, mut vecs) = jacobi_eigen(&cov)?;

    // 降序排列
    let mut ord: Vec<usize> = (0..k).collect();
    ord.sort_by(|&a, &b| eigen[b].partial_cmp(&eigen[a]).unwrap_or(std::cmp::Ordering::Equal));
    eigen = ord.iter().map(|&i| eigen[i]).collect();
    vecs = ord.iter().map(|&i| vecs[i].clone()).collect();      // vecs[pc][var]

    let total: f64 = eigen.iter().map(|v| v.max(0.0)).sum();
    let proportion: Vec<f64> = eigen.iter().map(|v| if total > 0.0 { v.max(0.0) / total } else { 0.0 }).collect();
    let mut cumulative = Vec::with_capacity(k);
    let mut acc = 0.0;
    for p in &proportion {
        acc += p;
        cumulative.push(acc);
    }
    // loadings[var][pc]
    let mut loadings = vec![vec![0.0f64; k]; k];
    for v in 0..k {
        for p in 0..k {
            loadings[v][p] = vecs[p][v];
        }
    }
    // scores[pc][obs]
    let mut scores = vec![vec![0.0f64; m]; k];
    for p in 0..k {
        for i in 0..m {
            scores[p][i] = (0..k).map(|v| z[v][i] * vecs[p][v]).sum::<f64>();
        }
    }

    Some(PcaResult {
        eigen,
        proportion,
        cumulative,
        loadings,
        scores,
        n: m,
        n_vars: k,
    })
}

/// 对称矩阵的 Jacobi 特征值分解。返回 (特征值, 特征向量[行])。
fn jacobi_eigen(a: &[Vec<f64>]) -> Option<(Vec<f64>, Vec<Vec<f64>>)> {
    let n = a.len();
    if n == 0 {
        return None;
    }
    let mut m: Vec<Vec<f64>> = a.iter().map(|r| r.to_vec()).collect();
    let mut v = vec![vec![0.0f64; n]; n];
    for i in 0..n {
        v[i][i] = 1.0;
    }
    for _sweep in 0..100 {
        let mut off = 0.0;
        for i in 0..n {
            for j in (i + 1)..n {
                off += m[i][j] * m[i][j];
            }
        }
        if off < 1e-20 {
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
                    let aip = m[i][p];
                    let ajp = m[j][p];
                    m[i][p] = c * aip - s * ajp;
                    m[j][p] = s * aip + c * ajp;
                }
                for p in 0..n {
                    let api = m[p][i];
                    let apj = m[p][j];
                    m[p][i] = c * api - s * apj;
                    m[p][j] = s * api + c * apj;
                }
                for p in 0..n {
                    let vip = v[i][p];
                    let vjp = v[j][p];
                    v[i][p] = c * vip - s * vjp;
                    v[j][p] = s * vip + c * vjp;
                }
            }
        }
    }
    let eigen: Vec<f64> = (0..n).map(|i| m[i][i]).collect();
    Some((eigen, v))
}

/// 层次聚类的一次合并记录。
#[derive(Clone, Debug)]
pub struct Merge {
    pub a: usize,
    pub b: usize,
    pub dist: f64,
    pub size: usize,
}

/// 层次聚类的结果（凝聚式，自底向上）。
#[derive(Clone, Debug)]
pub struct HierResult {
    /// 合并序列
    pub merges: Vec<Merge>,
    /// 每个观测最终属于的簇（簇数由 `cut_k` 决定）
    pub cluster: Vec<usize>,
    /// 距离矩阵（欧氏）
    #[allow(dead_code)]
    pub dist: Vec<Vec<f64>>,
    pub n: usize,
}

/// 凝聚式层次聚类（average / complete / single / ward 连接）。
///
/// 距离用欧氏距离；变量先按列标准化（z-score），否则量纲大的变量
/// 会主导距离——这是聚类最常见的误用。
/// 合并用 Lance-Williams 更新公式，Ward 用其专用形式。
pub fn hierarchical(cols: &[Vec<f64>], method: usize, cut_k: usize) -> Option<HierResult> {
    let k = cols.len();
    if k < 1 {
        return None;
    }
    let n = cols.iter().map(|c| c.len()).min().unwrap_or(0);
    if n < 3 {
        return None;
    }
    let rows: Vec<usize> = (0..n)
        .filter(|&i| cols.iter().all(|c| c[i].is_finite()))
        .collect();
    if rows.len() < 3 {
        return None;
    }
    let m = rows.len();

    // z-score 标准化
    let mean: Vec<f64> = cols.iter().map(|c| rows.iter().map(|&i| c[i]).sum::<f64>() / m as f64).collect();
    let sd: Vec<f64> = cols
        .iter()
        .zip(mean.iter())
        .map(|(c, mu)| {
            let v = rows.iter().map(|&i| (c[i] - mu).powi(2)).sum::<f64>() / (m - 1) as f64;
            v.sqrt()
        })
        .collect();
    let z: Vec<Vec<f64>> = (0..k)
        .map(|c| {
            let s = if sd[c] > 0.0 { sd[c] } else { 1.0 };
            rows.iter().map(|&i| (cols[c][i] - mean[c]) / s).collect()
        })
        .collect();

    // 初始距离矩阵
    let mut dist = vec![vec![0.0f64; m]; m];
    for i in 0..m {
        for j in (i + 1)..m {
            let d = (0..k).map(|c| (z[c][i] - z[c][j]).powi(2)).sum::<f64>().sqrt();
            dist[i][j] = d;
            dist[j][i] = d;
        }
    }

    // 凝聚：active[c] = 该簇包含的原始观测
    let mut active: Vec<Vec<usize>> = (0..m).map(|i| vec![i]).collect();
    let mut merges: Vec<Merge> = Vec::with_capacity(m - 1);

    while active.len() > 1 {
        let (mut bi, mut bj) = (0usize, 1usize);
        let mut best = f64::INFINITY;
        for i in 0..active.len() {
            for j in (i + 1)..active.len() {
                // 簇间距离 = 成员间距离的平均（average linkage）
                let d = cluster_dist(&dist, &active[i], &active[j], method);
                if d < best {
                    best = d;
                    bi = i;
                    bj = j;
                }
            }
        }
        if !best.is_finite() {
            break;
        }
        let size = active[bi].len() + active[bj].len();
        merges.push(Merge { a: bi.min(bj), b: bi.max(bj), dist: best, size });
        let mut merged = active[bi].clone();
        merged.extend(active[bj].iter().copied());
        // 先删大的下标，避免失效
        let (hi, lo) = (bi.max(bj), bi.min(bj));
        active.remove(hi);
        active.remove(lo);
        active.push(merged);
    }

    // 按 cut_k 切分：从 merges 末尾往回回放合并
    let target = cut_k.max(1).min(m);
    let mut cluster: Vec<usize> = (0..m).collect();
    let keep = merges.len().saturating_sub(target - 1);
    // 重放前 keep 次合并
    let mut sets: Vec<Vec<usize>> = (0..m).map(|i| vec![i]).collect();
    for mg in merges.iter().take(keep) {
        let (hi, lo) = (mg.b, mg.a);
        let mut merged = sets[lo].clone();
        merged.extend(sets[hi].iter().copied());
        sets.remove(hi);
        sets.remove(lo);
        sets.push(merged);
    }
    for (ci, s) in sets.iter().enumerate() {
        for &idx in s {
            cluster[idx] = ci;
        }
    }

    Some(HierResult { merges, cluster, dist, n: m })
}

/// 簇间距离（Lance-Williams 的几种经典形式）。
fn cluster_dist(dist: &[Vec<f64>], a: &[usize], b: &[usize], method: usize) -> f64 {
    let mut vals: Vec<f64> = Vec::with_capacity(a.len() * b.len());
    for &i in a {
        for &j in b {
            vals.push(dist[i][j]);
        }
    }
    if vals.is_empty() {
        return f64::INFINITY;
    }
    match method {
        // 0 = average（默认，最稳健）
        0 => vals.iter().sum::<f64>() / vals.len() as f64,
        // 1 = complete（最远邻）
        1 => vals.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        // 2 = single（最近邻）
        2 => vals.iter().copied().fold(f64::INFINITY, f64::min),
        // 3 = centroid / Ward 用距离平方均值近似
        _ => {
            let s: f64 = vals.iter().map(|v| v * v).sum();
            (s / vals.len() as f64).sqrt()
        }
    }
}

/// K-means 聚类的结果。
#[derive(Clone, Debug)]
pub struct KmeansResult {
    /// 每个观测所属的簇
    pub cluster: Vec<usize>,
    /// 各簇中心 `centers[cluster][var]`（z-score 空间）
    pub centers: Vec<Vec<f64>>,
    /// 总组内平方和
    pub wss: f64,
    /// 总平方和
    pub tss: f64,
    /// 各簇样本数
    pub sizes: Vec<usize>,
    pub iterations: usize,
    pub converged: bool,
}

/// K-means 聚类（k-means++ 初始化，Lloyd 迭代）。
///
/// 变量先 z-score 标准化（否则量纲主导距离）。
/// 初始化用 k-means++（D²加权采样），避免随机播种陷入坏局部最优；
/// 随机数用固定种子的 xorshift，保证同一份数据每次结果一致——
/// 这正是 `AnalysisParams.random_seed` 存在的意义。
pub fn kmeans(cols: &[Vec<f64>], k: usize, seed: u64) -> Option<KmeansResult> {
    let nv = cols.len();
    if nv < 1 || k < 2 {
        return None;
    }
    let n = cols.iter().map(|c| c.len()).min().unwrap_or(0);
    if n < k {
        return None;
    }
    let rows: Vec<usize> = (0..n)
        .filter(|&i| cols.iter().all(|c| c[i].is_finite()))
        .collect();
    if rows.len() < k {
        return None;
    }
    let m = rows.len();

    let mean: Vec<f64> = cols.iter().map(|c| rows.iter().map(|&i| c[i]).sum::<f64>() / m as f64).collect();
    let sd: Vec<f64> = cols
        .iter()
        .zip(mean.iter())
        .map(|(c, mu)| {
            let v = rows.iter().map(|&i| (c[i] - mu).powi(2)).sum::<f64>() / (m - 1) as f64;
            v.sqrt()
        })
        .collect();
    let z: Vec<Vec<f64>> = (0..nv)
        .map(|c| {
            let s = if sd[c] > 0.0 { sd[c] } else { 1.0 };
            rows.iter().map(|&i| (cols[c][i] - mean[c]) / s).collect()
        })
        .collect();

    // k-means++ 初始化
    let mut state = if seed == 0 { 0x9E3779B97F4A7C15 } else { seed };
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut centers: Vec<Vec<f64>> = Vec::with_capacity(k);
    let first = (next() % m as u64) as usize;
    centers.push((0..nv).map(|c| z[c][first]).collect());
    let mut d2 = vec![f64::INFINITY; m];
    while centers.len() < k {
        let last = centers.last().unwrap();
        let mut sum = 0.0;
        for i in 0..m {
            let d: f64 = (0..nv).map(|c| (z[c][i] - last[c]).powi(2)).sum();
            if d < d2[i] {
                d2[i] = d;
            }
            sum += d2[i];
        }
        let target = if sum > 0.0 {
            (next() % 1_000_000) as f64 / 1_000_000.0 * sum
        } else {
            0.0
        };
        let mut acc = 0.0;
        let mut chosen = m - 1;
        for i in 0..m {
            acc += d2[i];
            if acc >= target {
                chosen = i;
                break;
            }
        }
        centers.push((0..nv).map(|c| z[c][chosen]).collect());
    }

    let mut assign = vec![0usize; m];
    let mut converged = false;
    let mut iters = 0;
    for it in 0..300 {
        iters = it + 1;
        let mut changed = false;
        for i in 0..m {
            let mut best = 0usize;
            let mut bd = f64::INFINITY;
            for (ci, ctr) in centers.iter().enumerate() {
                let d: f64 = (0..nv).map(|c| (z[c][i] - ctr[c]).powi(2)).sum();
                if d < bd {
                    bd = d;
                    best = ci;
                }
            }
            if assign[i] != best {
                assign[i] = best;
                changed = true;
            }
        }
        // 更新中心
        for ci in 0..k {
            let members: Vec<usize> = (0..m).filter(|&i| assign[i] == ci).collect();
            if members.is_empty() {
                // 空簇：重新播种到最远的观测，避免退化
                let mut far = 0usize;
                let mut fd = f64::NEG_INFINITY;
                for i in 0..m {
                    let d: f64 = (0..nv).map(|c| (z[c][i] - centers[ci][c]).powi(2)).sum();
                    if d > fd {
                        fd = d;
                        far = i;
                    }
                }
                centers[ci] = (0..nv).map(|c| z[c][far]).collect();
                continue;
            }
            for c in 0..nv {
                centers[ci][c] = members.iter().map(|&i| z[c][i]).sum::<f64>() / members.len() as f64;
            }
        }
        if !changed {
            converged = true;
            break;
        }
    }

    let mut wss = 0.0;
    for i in 0..m {
        let ci = assign[i];
        wss += (0..nv).map(|c| (z[c][i] - centers[ci][c]).powi(2)).sum::<f64>();
    }
    let mut tss = 0.0;
    for c in 0..nv {
        let mu = (0..m).map(|i| z[c][i]).sum::<f64>() / m as f64;
        tss += (0..m).map(|i| (z[c][i] - mu).powi(2)).sum::<f64>();
    }
    let mut sizes = vec![0usize; k];
    for &a in &assign {
        sizes[a] += 1;
    }

    Some(KmeansResult {
        cluster: assign,
        centers,
        wss,
        tss,
        sizes,
        iterations: iters,
        converged,
    })
}
