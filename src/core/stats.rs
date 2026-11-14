//! 统计内核：纯函数 + 可验证的数值实现。
//!
//! 约定：所有双尾 p 值；样本方差用 n-1；分位数用 R 的 type-7（与 Prism 一致）。

/// 描述统计。
#[derive(Clone, Debug, Default)]
pub struct Desc {
    pub n: usize,
    pub mean: f64,
    pub sd: f64,
    pub sem: f64,
    pub median: f64,
    pub q1: f64,
    pub q3: f64,
    pub min: f64,
    pub max: f64,
    pub ci95: f64,
}

pub fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return f64::NAN;
    }
    xs.iter().sum::<f64>() / xs.len() as f64
}

pub fn variance(xs: &[f64]) -> f64 {
    let n = xs.len();
    if n < 2 {
        return 0.0;
    }
    let m = mean(xs);
    xs.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (n - 1) as f64
}

pub fn sd(xs: &[f64]) -> f64 {
    variance(xs).sqrt()
}

pub fn sem(xs: &[f64]) -> f64 {
    if xs.len() < 2 {
        0.0
    } else {
        sd(xs) / (xs.len() as f64).sqrt()
    }
}

/// R type-7 分位数（与 Prism / numpy 默认一致）。
pub fn quantile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let h = (sorted.len() - 1) as f64 * p.clamp(0.0, 1.0);
    let lo = h.floor() as usize;
    let hi = h.ceil() as usize;
    let frac = h - lo as f64;
    sorted[lo] * (1.0 - frac) + sorted[hi.min(sorted.len() - 1)] * frac
}

pub fn describe(xs: &[f64]) -> Option<Desc> {
    let mut v: Vec<f64> = xs.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    let m = mean(&v);
    let s = sd(&v);
    let se = sem(&v);
    let tcrit = if n >= 2 {
        t_quantile(0.975, (n - 1) as f64)
    } else {
        0.0
    };
    Some(Desc {
        n,
        mean: m,
        sd: s,
        sem: se,
        median: quantile(&v, 0.5),
        q1: quantile(&v, 0.25),
        q3: quantile(&v, 0.75),
        min: v[0],
        max: v[n - 1],
        ci95: tcrit * se,
    })
}

pub fn skewness(xs: &[f64]) -> f64 {
    let n = xs.len() as f64;
    if n < 3.0 {
        return 0.0;
    }
    let m = mean(xs);
    let m2 = xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / n;
    if m2 <= 0.0 {
        return 0.0;
    }
    let m3 = xs.iter().map(|x| (x - m).powi(3)).sum::<f64>() / n;
    m3 / m2.powf(1.5)
}

pub fn kurtosis(xs: &[f64]) -> f64 {
    let n = xs.len() as f64;
    if n < 4.0 {
        return 0.0;
    }
    let m = mean(xs);
    let m2 = xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / n;
    if m2 <= 0.0 {
        return 0.0;
    }
    let m4 = xs.iter().map(|x| (x - m).powi(4)).sum::<f64>() / n;
    m4 / (m2 * m2)
}

// ---------------------------------------------------------------------------
// 特殊函数
// ---------------------------------------------------------------------------

/// Lanczos 近似下的 ln Γ(x)。
pub fn ln_gamma(x: f64) -> f64 {
    const G: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        return std::f64::consts::PI.ln()
            - (std::f64::consts::PI * x).sin().abs().ln()
            - ln_gamma(1.0 - x);
    }
    let x = x - 1.0;
    let mut a = G[0];
    let t = x + 7.5;
    for (i, g) in G.iter().enumerate().skip(1) {
        a += g / (x + i as f64);
    }
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
}

fn betacf(a: f64, b: f64, x: f64) -> f64 {
    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < 1e-30 {
        d = 1e-30;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..=300 {
        let m = m as f64;
        let m2 = 2.0 * m;
        let aa = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < 1e-30 {
            d = 1e-30;
        }
        c = 1.0 + aa / c;
        if c.abs() < 1e-30 {
            c = 1e-30;
        }
        d = 1.0 / d;
        h *= d * c;
        let aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < 1e-30 {
            d = 1e-30;
        }
        c = 1.0 + aa / c;
        if c.abs() < 1e-30 {
            c = 1e-30;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 1e-12 {
            break;
        }
    }
    h
}

/// 正则不完全 beta 函数 I_x(a,b)。
pub fn betai(a: f64, b: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let bt = (ln_gamma(a + b) - ln_gamma(a) - ln_gamma(b) + a * x.ln() + b * (1.0 - x).ln()).exp();
    if x < (a + 1.0) / (a + b + 2.0) {
        bt * betacf(a, b, x) / a
    } else {
        1.0 - bt * betacf(b, a, 1.0 - x) / b
    }
}

/// 标准正态 CDF（erfc 近似，相对误差 < 1.2e-7）。
pub fn norm_cdf(z: f64) -> f64 {
    0.5 * erfc(-z / std::f64::consts::SQRT_2)
}

pub fn erfc(x: f64) -> f64 {
    let z = x.abs();
    let t = 1.0 / (1.0 + 0.5 * z);
    let ans = t
        * (-z * z - 1.265_512_23
            + t * (1.000_023_68
                + t * (0.374_091_96
                    + t * (0.096_784_18
                        + t * (-0.186_288_06
                            + t * (0.278_868_07
                                + t * (-1.135_203_98
                                    + t * (1.488_515_87
                                        + t * (-0.822_152_23 + t * 0.170_872_77)))))))))
            .exp();
    if x >= 0.0 {
        ans
    } else {
        2.0 - ans
    }
}

/// 标准正态分位数（Acklam 近似）。
pub fn norm_ppf(p: f64) -> f64 {
    if p <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    const A: [f64; 6] = [
        -3.969_683_028_665_376e1,
        2.209_460_984_245_205e2,
        -2.759_285_104_469_687e2,
        1.383_577_518_672_69e2,
        -3.066_479_806_614_716e1,
        2.506_628_277_459_239,
    ];
    const B: [f64; 5] = [
        -5.447_609_879_822_406e1,
        1.615_858_368_580_409e2,
        -1.556_989_798_598_866e2,
        6.680_131_188_771_972e1,
        -1.328_068_155_288_572e1,
    ];
    const C: [f64; 6] = [
        -7.784_894_002_430_293e-3,
        -3.223_964_580_411_365e-1,
        -2.400_758_277_161_838,
        -2.549_732_539_343_734,
        4.374_664_141_464_968,
        2.938_163_982_698_783,
    ];
    const D: [f64; 4] = [
        7.784_695_709_041_462e-3,
        3.224_671_290_700_398e-1,
        2.445_134_137_142_996,
        3.754_408_661_907_416,
    ];
    let low = p < 0.02425;
    let high = p > 1.0 - 0.02425;
    if low || high {
        let q = if low { p } else { 1.0 - p };
        let r = (-2.0 * q.ln()).sqrt();
        let mut x = (((((C[0] * r + C[1]) * r + C[2]) * r + C[3]) * r + C[4]) * r + C[5])
            / ((((D[0] * r + D[1]) * r + D[2]) * r + D[3]) * r + 1.0);
        if high {
            x = -x;
        }
        x
    } else {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    }
}

/// 学生 t 分布 CDF。
pub fn t_cdf(t: f64, df: f64) -> f64 {
    if !t.is_finite() || df <= 0.0 {
        return f64::NAN;
    }
    let x = df / (df + t * t);
    let tail = 0.5 * betai(df / 2.0, 0.5, x);
    if t >= 0.0 {
        1.0 - tail
    } else {
        tail
    }
}

/// 双尾 p 值。
pub fn t_p(t: f64, df: f64) -> f64 {
    if !t.is_finite() {
        return 0.0;
    }
    if df <= 0.0 {
        return f64::NAN;
    }
    betai(df / 2.0, 0.5, df / (df + t * t)).clamp(0.0, 1.0)
}

pub fn t_quantile(p: f64, df: f64) -> f64 {
    if df <= 0.0 {
        return f64::NAN;
    }
    if (p - 0.5).abs() < 1e-12 {
        return 0.0;
    }
    let mut lo = -200.0;
    let mut hi = 200.0;
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if t_cdf(mid, df) < p {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

pub fn f_sf(f: f64, d1: f64, d2: f64) -> f64 {
    if f <= 0.0 {
        return 1.0;
    }
    betai(d2 / 2.0, d1 / 2.0, d2 / (d2 + d1 * f)).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// 检验
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct TTest {
    pub t: f64,
    pub df: f64,
    pub p: f64,
    pub diff: f64,
    pub se: f64,
    pub ci_low: f64,
    pub ci_high: f64,
    pub cohen_d: f64,
    pub n1: usize,
    pub n2: usize,
}

pub fn t_test_unpaired(a: &[f64], b: &[f64], welch: bool) -> Option<TTest> {
    if a.len() < 2 || b.len() < 2 {
        return None;
    }
    let (n1, n2) = (a.len() as f64, b.len() as f64);
    let (m1, m2) = (mean(a), mean(b));
    let (v1, v2) = (variance(a), variance(b));
    let diff = m1 - m2;
    let (se, df) = if welch {
        let s1 = v1 / n1;
        let s2 = v2 / n2;
        let se = (s1 + s2).sqrt();
        let df = if se == 0.0 {
            n1 + n2 - 2.0
        } else {
            (s1 + s2).powi(2) / (s1 * s1 / (n1 - 1.0) + s2 * s2 / (n2 - 1.0))
        };
        (se, df)
    } else {
        let sp2 = ((n1 - 1.0) * v1 + (n2 - 1.0) * v2) / (n1 + n2 - 2.0);
        ((sp2 * (1.0 / n1 + 1.0 / n2)).sqrt(), n1 + n2 - 2.0)
    };
    let se = if se == 0.0 { f64::NAN } else { se };
    let t = diff / se;
    let p = if se.is_nan() { 1.0 } else { t_p(t, df) };
    let tc = t_quantile(0.975, df);
    let sp = (((n1 - 1.0) * v1 + (n2 - 1.0) * v2) / (n1 + n2 - 2.0)).sqrt();
    Some(TTest {
        t,
        df,
        p,
        diff,
        se,
        ci_low: diff - tc * se,
        ci_high: diff + tc * se,
        cohen_d: if sp > 0.0 { diff / sp } else { f64::NAN },
        n1: a.len(),
        n2: b.len(),
    })
}

pub fn t_test_paired(a: &[f64], b: &[f64]) -> Option<TTest> {
    let n = a.len().min(b.len());
    if n < 2 {
        return None;
    }
    let d: Vec<f64> = (0..n).map(|i| a[i] - b[i]).collect();
    let m = mean(&d);
    let s = sd(&d);
    let se = s / (n as f64).sqrt();
    let df = (n - 1) as f64;
    let t = if se > 0.0 { m / se } else { 0.0 };
    let p = if se > 0.0 { t_p(t, df) } else { 1.0 };
    let tc = t_quantile(0.975, df);
    Some(TTest {
        t,
        df,
        p,
        diff: m,
        se,
        ci_low: m - tc * se,
        ci_high: m + tc * se,
        cohen_d: if s > 0.0 { m / s } else { f64::NAN },
        n1: n,
        n2: n,
    })
}

#[derive(Clone, Debug, Default)]
pub struct Anova {
    pub ss_between: f64,
    pub ss_within: f64,
    pub df_between: f64,
    pub df_within: f64,
    pub ms_between: f64,
    pub ms_within: f64,
    pub f: f64,
    pub p: f64,
    pub eta2: f64,
    pub k: usize,
    pub n: usize,
}

pub fn anova_oneway(groups: &[Vec<f64>]) -> Option<Anova> {
    let groups: Vec<&Vec<f64>> = groups.iter().filter(|g| g.len() >= 2).collect();
    if groups.len() < 2 {
        return None;
    }
    let k = groups.len();
    let n: usize = groups.iter().map(|g| g.len()).sum();
    if n <= k {
        return None;
    }
    let grand = groups.iter().flat_map(|g| g.iter()).sum::<f64>() / n as f64;
    let mut ssb = 0.0;
    let mut ssw = 0.0;
    for g in &groups {
        let m = mean(g);
        ssb += g.len() as f64 * (m - grand).powi(2);
        ssw += g.iter().map(|x| (x - m).powi(2)).sum::<f64>();
    }
    let dfb = (k - 1) as f64;
    let dfw = (n - k) as f64;
    let msb = ssb / dfb;
    let msw = ssw / dfw;
    let f = if msw > 0.0 { msb / msw } else { f64::INFINITY };
    Some(Anova {
        ss_between: ssb,
        ss_within: ssw,
        df_between: dfb,
        df_within: dfw,
        ms_between: msb,
        ms_within: msw,
        f,
        p: if msw > 0.0 { f_sf(f, dfb, dfw) } else { 0.0 },
        eta2: if ssb + ssw > 0.0 {
            ssb / (ssb + ssw)
        } else {
            0.0
        },
        k,
        n,
    })
}

#[derive(Clone, Debug)]
pub struct Pair {
    pub a: usize,
    pub b: usize,
    pub diff: f64,
    pub t: f64,
    pub p_raw: f64,
    pub p_adj: f64,
}

/// 事后两两比较：Welch t + Holm 校正。
/// （Prism 默认 Tukey；这里给出更稳健的 Welch + Holm，结果表会注明方法。）
pub fn posthoc_holm(groups: &[Vec<f64>]) -> Vec<Pair> {
    let mut pairs = Vec::new();
    for i in 0..groups.len() {
        for j in (i + 1)..groups.len() {
            if let Some(t) = t_test_unpaired(&groups[i], &groups[j], true) {
                pairs.push(Pair {
                    a: i,
                    b: j,
                    diff: t.diff,
                    t: t.t,
                    p_raw: t.p,
                    p_adj: t.p,
                });
            }
        }
    }
    let m = pairs.len() as f64;
    let mut order: Vec<usize> = (0..pairs.len()).collect();
    order.sort_by(|&a, &b| pairs[a].p_raw.partial_cmp(&pairs[b].p_raw).unwrap());
    let mut running = 0.0_f64;
    for (rank, &idx) in order.iter().enumerate() {
        let adj = (pairs[idx].p_raw * (m - rank as f64)).min(1.0);
        running = running.max(adj);
        pairs[idx].p_adj = running;
    }
    pairs
}

/// Kruskal–Wallis 单因素秩方差分析结果。
#[derive(Clone, Debug)]
pub struct KruskalWallis {
    /// 未校正的 H 统计量
    pub h: f64,
    /// 并列校正后的 H = `H / C`（C < 1，所以校正后**更大**）
    pub h_corrected: f64,
    pub df: f64,
    pub p: f64,
    /// 效应量 ε² = H / (n − 1)
    pub epsilon2: f64,
    pub k: usize,
    pub n: usize,
    /// 并列组数（>0 时 H 做了校正）
    pub tie_groups: usize,
    /// 各组平均秩（结果表展示用）
    pub mean_ranks: Vec<f64>,
}

/// Kruskal–Wallis 检验：多组非参数比较。
///
/// `H = 12/(N(N+1)) · Σ n_i · R̄_i² − 3(N+1)`，再除以并列校正因子
/// `C = 1 − Σ(t³−t)/(N³−N)`。**并列必须校正**，否则 H 偏大、p 偏小。
pub fn kruskal_wallis(groups: &[Vec<f64>]) -> Option<KruskalWallis> {
    let valid: Vec<&Vec<f64>> = groups.iter().filter(|g| !g.is_empty()).collect();
    if valid.len() < 2 {
        return None;
    }
    let n: usize = valid.iter().map(|g| g.len()).sum();
    if n < 3 {
        return None;
    }
    let mut all: Vec<(f64, usize)> = Vec::with_capacity(n);
    for (gi, g) in valid.iter().enumerate() {
        for &v in g.iter() {
            all.push((v, gi));
        }
    }
    all.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut ranks = vec![0.0_f64; n];
    let mut tie_sum = 0.0_f64;
    let mut tie_groups = 0usize;
    let mut i = 0;
    while i < n {
        let mut j = i + 1;
        while j < n && (all[j].0 - all[i].0).abs() < 1e-12 {
            j += 1;
        }
        let t = (j - i) as f64;
        let avg = (i as f64 + 1.0 + j as f64) / 2.0;
        for k in i..j {
            ranks[k] = avg;
        }
        if t > 1.0 {
            tie_sum += t * t * t - t;
            tie_groups += 1;
        }
        i = j;
    }

    let mut rank_sum = vec![0.0_f64; valid.len()];
    for (k, (_, gi)) in all.iter().enumerate() {
        rank_sum[*gi] += ranks[k];
    }
    let nf = n as f64;
    let h_raw = 12.0 / (nf * (nf + 1.0))
        * (0..valid.len())
            .map(|g| {
                let ni = valid[g].len() as f64;
                let r = rank_sum[g] / ni;
                ni * r * r
            })
            .sum::<f64>()
        - 3.0 * (nf + 1.0);

    // 并列校正
    let denom = nf * nf * nf - nf;
    let c = if denom > 0.0 {
        1.0 - tie_sum / denom
    } else {
        1.0
    };
    let h = if c > 0.0 { h_raw / c } else { h_raw };
    let df = (valid.len() - 1) as f64;

    Some(KruskalWallis {
        h: h_raw,
        h_corrected: h,
        df,
        // 用校正后的 H 查 χ²(df)
        p: crate::core::contingency::chi2_sf(h.max(0.0), df),
        epsilon2: if nf > 1.0 { h / (nf - 1.0) } else { 0.0 },
        k: valid.len(),
        n,
        tie_groups,
        mean_ranks: (0..valid.len())
            .map(|g| rank_sum[g] / valid[g].len() as f64)
            .collect(),
    })
}

/// Dunn 事后两两比较结果。
#[derive(Clone, Debug)]
pub struct DunnPair {
    pub a: usize,
    pub b: usize,
    /// 两组平均秩之差
    pub rank_diff: f64,
    pub z: f64,
    pub p_raw: f64,
    /// Holm 校正后的 p
    pub p_holm: f64,
    /// Bonferroni 校正后的 p
    pub p_bonf: f64,
}

/// Dunn 检验：Kruskal–Wallis 之后的非参数两两比较。
///
/// 公式（Dunn 1964）：
/// ```text
/// z_ij = (R̄_i − R̄_j) / sqrt( (N(N+1)/12 − Σ(t³−t)/(12(N−1))) · (1/n_i + 1/n_j) )
/// ```
/// 其中 `t` 是每个并列组的元素个数——**并列必须做修正**，否则 p 值偏小。
/// 双侧 p = 2(1 − Φ(|z|))，再做 Holm / Bonferroni 多重比较校正。
pub fn posthoc_dunn(groups: &[Vec<f64>]) -> Vec<DunnPair> {
    let valid: Vec<(usize, &Vec<f64>)> = groups
        .iter()
        .enumerate()
        .filter(|(_, g)| !g.is_empty())
        .collect();
    if valid.len() < 2 {
        return Vec::new();
    }
    let n: usize = valid.iter().map(|(_, g)| g.len()).sum();
    if n < 3 {
        return Vec::new();
    }

    // 全局秩（并列取平均秩），同时记录并列组大小
    let mut all: Vec<(f64, usize)> = Vec::with_capacity(n);
    for (gi, (_, g)) in valid.iter().enumerate() {
        for &v in g.iter() {
            all.push((v, gi));
        }
    }
    all.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut ranks = vec![0.0_f64; n];
    let mut tie_sum = 0.0_f64; // Σ(t³ − t)
    let mut i = 0;
    while i < n {
        let mut j = i + 1;
        while j < n && (all[j].0 - all[i].0).abs() < 1e-12 {
            j += 1;
        }
        let t = (j - i) as f64;
        let avg = (i as f64 + 1.0 + j as f64) / 2.0;
        for k in i..j {
            ranks[k] = avg;
        }
        if t > 1.0 {
            tie_sum += t * t * t - t;
        }
        i = j;
    }
    // 每组的平均秩
    let mut rank_sum = vec![0.0_f64; valid.len()];
    for (k, (_, gi)) in all.iter().enumerate() {
        rank_sum[*gi] += ranks[k];
    }
    let nf = n as f64;
    let tie_term = tie_sum / (12.0 * (nf - 1.0));
    let base = nf * (nf + 1.0) / 12.0 - tie_term;
    if !(base > 0.0) {
        return Vec::new();
    }

    let mut pairs = Vec::new();
    for i in 0..valid.len() {
        for j in (i + 1)..valid.len() {
            let (ni, nj) = (valid[i].1.len() as f64, valid[j].1.len() as f64);
            if ni <= 0.0 || nj <= 0.0 {
                continue;
            }
            let ri = rank_sum[i] / ni;
            let rj = rank_sum[j] / nj;
            let se = (base * (1.0 / ni + 1.0 / nj)).sqrt();
            if !(se > 0.0) {
                continue;
            }
            let z = (ri - rj) / se;
            let p_raw = 2.0 * (1.0 - norm_cdf(z.abs()));
            pairs.push(DunnPair {
                a: valid[i].0,
                b: valid[j].0,
                rank_diff: ri - rj,
                z,
                p_raw,
                p_holm: p_raw,
                p_bonf: (p_raw * (valid.len() * (valid.len() - 1) / 2) as f64).min(1.0),
            });
        }
    }

    // Holm 逐步校正
    let m = pairs.len() as f64;
    let mut order: Vec<usize> = (0..pairs.len()).collect();
    order.sort_by(|&x, &y| {
        pairs[x]
            .p_raw
            .partial_cmp(&pairs[y].p_raw)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut running = 0.0_f64;
    for (rank, &idx) in order.iter().enumerate() {
        let adj = (pairs[idx].p_raw * (m - rank as f64)).min(1.0);
        running = running.max(adj);
        pairs[idx].p_holm = running;
    }
    pairs
}

/// Mann–Whitney U：无并列时精确（小样本），否则正态近似 + 并列校正。
pub fn mann_whitney(a: &[f64], b: &[f64]) -> Option<(f64, f64)> {
    if a.len() < 2 || b.len() < 2 {
        return None;
    }
    let mut all: Vec<(f64, usize)> = a
        .iter()
        .map(|&v| (v, 0))
        .chain(b.iter().map(|&v| (v, 1)))
        .collect();
    all.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
    let n = all.len();
    let mut ranks = vec![0.0; n];
    let mut ties = Vec::new();
    let mut i = 0;
    while i < n {
        let mut j = i + 1;
        while j < n && (all[j].0 - all[i].0).abs() < 1e-12 {
            j += 1;
        }
        let avg = ((i + 1) as f64 + j as f64) / 2.0;
        for k in i..j {
            ranks[k] = avg;
        }
        if j - i > 1 {
            ties.push((j - i) as f64);
        }
        i = j;
    }
    let r1: f64 = ranks
        .iter()
        .zip(all.iter())
        .filter(|(_, (_, g))| *g == 0)
        .map(|(r, _)| *r)
        .sum();
    let n1 = a.len() as f64;
    let n2 = b.len() as f64;
    let u1 = r1 - n1 * (n1 + 1.0) / 2.0;
    let u = u1.min(n1 * n2 - u1);

    let p = if ties.is_empty() && n1 <= 25.0 && n2 <= 25.0 {
        // 秩和 DP：从 1..n 中取 n1 个秩，统计秩和 <= r1 的组合数
        let total = (n1 + n2) as usize;
        let k = n1 as usize;
        let max_sum = total * (total + 1) / 2;
        let mut dp = vec![vec![0u64; max_sum + 1]; k + 1];
        dp[0][0] = 1;
        for rank in 1..=total {
            for j in (1..=k.min(rank)).rev() {
                for s in (rank..=max_sum).rev() {
                    if dp[j - 1][s - rank] > 0 {
                        dp[j][s] += dp[j - 1][s - rank];
                    }
                }
            }
        }
        let total_comb = dp[k].iter().sum::<u64>() as f64;
        let r = (r1 as usize).min(max_sum);
        let p_low: f64 = dp[k][..=r].iter().sum::<u64>() as f64 / total_comb;
        let p_high: f64 = 1.0 - (dp[k][..r].iter().sum::<u64>() as f64) / total_comb;
        (2.0 * p_low.min(p_high)).min(1.0)
    } else {
        let tie_term: f64 = ties.iter().map(|t| t.powi(3) - t).sum();
        let var = n1 * n2 / 12.0 * ((n1 + n2 + 1.0) - tie_term / ((n1 + n2) * (n1 + n2 - 1.0)));
        if var <= 0.0 {
            1.0
        } else {
            let z = (u - n1 * n2 / 2.0 + 0.5) / var.sqrt();
            (2.0 * norm_cdf(z)).min(1.0)
        }
    };
    Some((u, p))
}

/// Wilcoxon 符号秩：无并列小样本精确，否则正态近似。
pub fn wilcoxon(a: &[f64], b: &[f64]) -> Option<(f64, f64)> {
    let d: Vec<f64> = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| x - y)
        .filter(|v| v.abs() > 1e-12)
        .collect();
    let n = d.len();
    if n < 2 {
        return None;
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| d[i].abs().partial_cmp(&d[j].abs()).unwrap());
    let mut ranks = vec![0.0; n];
    let mut ties = Vec::new();
    let mut i = 0;
    while i < n {
        let mut j = i + 1;
        while j < n && (d[order[j]].abs() - d[order[i]].abs()).abs() < 1e-12 {
            j += 1;
        }
        let avg = ((i + 1) as f64 + j as f64) / 2.0;
        for k in i..j {
            ranks[order[k]] = avg;
        }
        if j - i > 1 {
            ties.push((j - i) as f64);
        }
        i = j;
    }
    let w_pos: f64 = (0..n).filter(|&i| d[i] > 0.0).map(|i| ranks[i]).sum();
    let w_neg: f64 = (0..n).filter(|&i| d[i] < 0.0).map(|i| ranks[i]).sum();
    let w = w_pos.min(w_neg);

    let p = if ties.is_empty() && n <= 30 {
        let max_sum = n * (n + 1) / 2;
        let mut dp = vec![vec![0u64; max_sum + 1]; n + 1];
        dp[0][0] = 1;
        for rank in 1..=n {
            for j in (1..=n).rev() {
                for s in (rank..=max_sum).rev() {
                    if dp[j - 1][s - rank] > 0 {
                        dp[j][s] += dp[j - 1][s - rank];
                    }
                }
            }
        }
        // W+ 是「每个秩以 1/2 概率被选中」的和：对任意子集大小求和。
        let mut dist = vec![0u64; max_sum + 1];
        for j in 0..=n {
            for s in 0..=max_sum {
                dist[s] += dp[j][s];
            }
        }
        let total: u64 = dist.iter().sum(); // = 2^n
        let lo = (w as usize).min(max_sum);
        let count: u64 = dist[..=lo].iter().sum();
        (2.0 * count as f64 / total as f64).min(1.0)
    } else {
        let mean_w = n as f64 * (n as f64 + 1.0) / 4.0;
        let tie_term: f64 = ties.iter().map(|t| t.powi(3) - t).sum();
        let var = n as f64 * (n as f64 + 1.0) * (2.0 * n as f64 + 1.0) / 24.0 - tie_term / 48.0;
        if var <= 0.0 {
            1.0
        } else {
            let z = (w - mean_w + 0.5) / var.sqrt();
            (2.0 * norm_cdf(z)).min(1.0)
        }
    };
    Some((w, p))
}

#[derive(Clone, Debug, Default)]
pub struct Correlation {
    pub r: f64,
    pub n: usize,
    pub p: f64,
    pub ci_low: f64,
    pub ci_high: f64,
}

fn pearson_raw(x: &[f64], y: &[f64]) -> Option<Correlation> {
    let n = x.len().min(y.len());
    if n < 3 {
        return None;
    }
    let mx = mean(&x[..n]);
    let my = mean(&y[..n]);
    let mut sxx = 0.0;
    let mut syy = 0.0;
    let mut sxy = 0.0;
    for i in 0..n {
        let dx = x[i] - mx;
        let dy = y[i] - my;
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    if sxx <= 0.0 || syy <= 0.0 {
        return None;
    }
    let r = (sxy / (sxx * syy).sqrt()).clamp(-1.0, 1.0);
    let df = (n - 2) as f64;
    let t = if r.abs() >= 1.0 {
        f64::INFINITY
    } else {
        r * (df / (1.0 - r * r)).sqrt()
    };
    let p = if t.is_infinite() { 0.0 } else { t_p(t, df) };
    // Fisher z 的 95% CI
    let z = 0.5 * ((1.0 + r) / (1.0 - r)).ln();
    let se = 1.0 / (n as f64 - 3.0).sqrt();
    let zc = 1.959_963_984_540_054;
    let (l, h) = ((z - zc * se).tanh(), (z + zc * se).tanh());
    Some(Correlation {
        r,
        n,
        p,
        ci_low: l,
        ci_high: h,
    })
}

pub fn pearson(x: &[f64], y: &[f64]) -> Option<Correlation> {
    pearson_raw(x, y)
}

fn rank(xs: &[f64]) -> Vec<f64> {
    let n = xs.len();
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&a, &b| xs[a].partial_cmp(&xs[b]).unwrap());
    let mut r = vec![0.0; n];
    let mut i = 0;
    while i < n {
        let mut j = i + 1;
        while j < n && (xs[idx[j]] - xs[idx[i]]).abs() < 1e-12 {
            j += 1;
        }
        let avg = ((i + 1) as f64 + j as f64) / 2.0;
        for k in i..j {
            r[idx[k]] = avg;
        }
        i = j;
    }
    r
}

pub fn spearman(x: &[f64], y: &[f64]) -> Option<Correlation> {
    let n = x.len().min(y.len());
    pearson_raw(&rank(&x[..n]), &rank(&y[..n]))
}

#[derive(Clone, Debug, Default)]
pub struct LinearReg {
    pub slope: f64,
    pub intercept: f64,
    pub r2: f64,
    pub n: usize,
    pub se_slope: f64,
    pub t: f64,
    pub p: f64,
    pub ci_low: f64,
    pub ci_high: f64,
}

pub fn linreg(x: &[f64], y: &[f64]) -> Option<LinearReg> {
    let n = x.len().min(y.len());
    if n < 3 {
        return None;
    }
    let mx = mean(&x[..n]);
    let my = mean(&y[..n]);
    let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let dx = x[i] - mx;
        let dy = y[i] - my;
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
    }
    if sxx <= 0.0 {
        return None;
    }
    let slope = sxy / sxx;
    let intercept = my - slope * mx;
    let ss_res = (syy - slope * sxy).max(0.0);
    let df = (n - 2) as f64;
    let mse = ss_res / df;
    let se_slope = if sxx > 0.0 {
        (mse / sxx).sqrt()
    } else {
        f64::NAN
    };
    let t = if se_slope > 0.0 {
        slope / se_slope
    } else {
        0.0
    };
    let p = if se_slope > 0.0 { t_p(t, df) } else { 1.0 };
    let tc = t_quantile(0.975, df);
    Some(LinearReg {
        slope,
        intercept,
        r2: if syy > 0.0 {
            (sxy * sxy) / (sxx * syy)
        } else {
            0.0
        },
        n,
        se_slope,
        t,
        p,
        ci_low: slope - tc * se_slope,
        ci_high: slope + tc * se_slope,
    })
}

/// 回归拟合结果（含绘制置信带所需的分量）。
#[derive(Clone, Copy, Debug)]
pub struct Fit {
    pub slope: f64,
    pub intercept: f64,
    pub r2: f64,
    pub n: usize,
    /// x 的均值与离差平方和（均值响应置信带用）
    pub mx: f64,
    pub sxx: f64,
    /// 残差标准误
    pub s: f64,
    /// 斜率检验的 p 值
    pub p: f64,
}

impl Fit {
    pub fn at(&self, x: f64) -> f64 {
        self.intercept + self.slope * x
    }

    /// 均值响应的 95% 置信半宽：`t * s * sqrt(1/n + (x-x̄)²/Sxx)`。
    ///
    /// 这是「回归线本身的置信带」，也是 Prism / ggplot `geom_smooth` 的默认口径；
    /// 预测区间会把 1/n 换成 1+1/n，带子宽得多，不适合画在图上。
    pub fn ci95(&self, x: f64, t_crit: f64) -> f64 {
        let n = self.n as f64;
        if !(self.sxx > 0.0) || !(self.s > 0.0) {
            return 0.0;
        }
        let se = self.s * (1.0 / n + (x - self.mx).powi(2) / self.sxx).sqrt();
        t_crit * se
    }
}

/// 最小二乘直线拟合（n ≥ 3）。与 [`linreg`] 同源，但保留绘制置信带需要的量。
pub fn fit_line(xs: &[f64], ys: &[f64]) -> Option<Fit> {
    let n = xs.len().min(ys.len());
    if n < 3 {
        return None;
    }
    let mx = mean(&xs[..n]);
    let my = mean(&ys[..n]);
    let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let dx = xs[i] - mx;
        let dy = ys[i] - my;
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
    }
    if !(sxx > 0.0) {
        return None;
    }
    let slope = sxy / sxx;
    let intercept = my - slope * mx;
    let ss_res = (syy - slope * sxy).max(0.0);
    let df = (n - 2) as f64;
    let s = (ss_res / df).sqrt();
    let se_slope = (s / sxx.sqrt()).max(0.0);
    let t = if se_slope > 0.0 {
        slope / se_slope
    } else {
        0.0
    };
    Some(Fit {
        slope,
        intercept,
        r2: if syy > 0.0 {
            (sxy * sxy) / (sxx * syy)
        } else {
            0.0
        },
        n,
        mx,
        sxx,
        s,
        p: if se_slope > 0.0 { t_p(t, df) } else { 1.0 },
    })
}

/// D'Agostino–Pearson K² 正态性检验（n ≥ 8）。
pub fn dagostino(xs: &[f64]) -> Option<(f64, f64)> {
    let n = xs.len();
    if n < 8 {
        return None;
    }
    let nf = n as f64;
    let g1 = skewness(xs);
    let b2 = kurtosis(xs);

    let y = g1 * ((nf + 1.0) * (nf + 3.0) / (6.0 * (nf - 2.0))).sqrt();
    let beta2 = 3.0 * (nf * nf + 27.0 * nf - 70.0) * (nf + 1.0) * (nf + 3.0)
        / ((nf - 2.0) * (nf + 5.0) * (nf + 7.0) * (nf + 9.0));
    let w2 = -1.0 + (2.0 * (beta2 - 1.0)).sqrt();
    let delta = 1.0 / (0.5 * w2.ln()).sqrt();
    let alpha = (2.0 / (w2 - 1.0)).sqrt();
    let z1 = delta * (y / alpha).asinh();

    let e = 3.0 * (nf - 1.0) / (nf + 1.0);
    let varb2 =
        24.0 * nf * (nf - 2.0) * (nf - 3.0) / ((nf + 1.0).powi(2) * (nf + 3.0) * (nf + 5.0));
    let xx = (b2 - e) / varb2.sqrt();
    let sqrt_beta1 = 6.0 * (nf * nf - 5.0 * nf + 2.0) / ((nf + 7.0) * (nf + 9.0))
        * (6.0 * (nf + 3.0) * (nf + 5.0) / (nf * (nf - 2.0) * (nf - 3.0))).sqrt();
    let a = 6.0
        + 8.0 / sqrt_beta1 * (2.0 / sqrt_beta1 + (1.0 + 4.0 / (sqrt_beta1 * sqrt_beta1)).sqrt());
    let z2 = ((1.0 - 2.0 / (9.0 * a))
        - ((1.0 - 2.0 / a) / (1.0 + xx * (2.0 / (a - 4.0)).sqrt())).cbrt())
        / (2.0 / (9.0 * a)).sqrt();

    let k2 = z1 * z1 + z2 * z2;
    Some((k2, (-k2 / 2.0).exp()))
}

/// 高斯核密度估计（Silverman 带宽），用于小提琴图。
/// Silverman 带宽（`0.9 · min(σ, IQR/1.34) · n^-0.2`）。
///
/// 退化情形（样本全相同、IQR 与 σ 都为 0）返回一个与量级相关的极小宽度，
/// 否则核密度会变成一个无限高的尖峰，小提琴退化成一条竖线。
pub fn kde_bandwidth(samples: &[f64]) -> f64 {
    let n = samples.len();
    if n < 2 {
        return 0.0;
    }
    let s = sd(samples);
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let iqr = quantile(&sorted, 0.75) - quantile(&sorted, 0.25);
    // 标准差为 0 时 IQR 也可能是 0，回退到数据量级。
    let fallback = sorted[0].abs().max(1.0) * 1e-3;
    let spread = if iqr > 0.0 { iqr / 1.34 } else { s };
    let spread = if spread > 0.0 && spread.is_finite() {
        spread
    } else {
        fallback
    };
    let h = 0.9 * spread * (n as f64).powf(-0.2);
    if h > 0.0 && h.is_finite() {
        h
    } else {
        fallback
    }
}

pub fn kde(samples: &[f64], grid: &[f64]) -> Vec<f64> {
    if samples.len() < 2 || grid.is_empty() {
        return vec![0.0; grid.len()];
    }
    let h = kde_bandwidth(samples).max(1e-12);
    grid.iter()
        .map(|&g| {
            let sum: f64 = samples
                .iter()
                .map(|&x| (-0.5 * ((g - x) / h).powi(2)).exp())
                .sum();
            sum / (samples.len() as f64 * h * (2.0 * std::f64::consts::PI).sqrt())
        })
        .collect()
}

/// 随机抖动（确定性）：同一行永远得到同样的偏移，重绘不会「跳」。
pub fn jitter(row: usize, seed: f64) -> f64 {
    let x = ((row as f64 + 1.0) * 12.9898 + seed * 78.233).sin() * 43_758.5453;
    let f = x - x.floor();
    f - 0.5
}

/// Cohen's d 的 95% CI（Hedges–Olkin 近似）：
/// SE = sqrt((n1+n2)/(n1·n2) + d²/(2(n1+n2)))，CI = d ± 1.96·SE。
pub fn cohen_d_ci(d: f64, n1: usize, n2: usize) -> (f64, f64, f64) {
    let (n1, n2) = (n1 as f64, n2 as f64);
    if n1 < 2.0 || n2 < 2.0 {
        return (d, f64::NAN, f64::NAN);
    }
    let se = ((n1 + n2) / (n1 * n2) + d * d / (2.0 * (n1 + n2))).sqrt();
    (d, d - 1.96 * se, d + 1.96 * se)
}

// ---------------------------------------------------------------------------
// 生存分析：log-rank 检验与风险比
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct LogRank {
    pub chi2: f64,
    pub df: usize,
    pub p: f64,
    /// 两组时的 log-rank 风险比估计 exp((O−E)/V) 及其 95% CI。
    /// 多组没有单一 HR，留空。
    pub hr: Option<f64>,
    pub ci_low: Option<f64>,
    pub ci_high: Option<f64>,
    pub n_events: usize,
}

/// log-rank（Mantel–Cox）检验。
///
/// 每组是 (时间, 是否事件) 的列表。在每个事件时刻做超几何期望/方差累积：
/// O_j = Σd_ij，E_j = Σd_i·n_ij/n_i，协方差 V_jl 同理；
/// 统计量取 U'V⁻¹U（k 组自由度 k−1；2 组退化为 (O₁−E₁)²/V）。
///
/// 两组时另给 HR = exp((O₁−E₁)/V)（log-rank 点估计），SE = 1/√V，
/// 95% CI = exp(ln HR ± 1.96·SE)。
pub fn log_rank(groups: &[Vec<(f64, bool)>]) -> Option<LogRank> {
    let k = groups.len();
    if k < 2 || groups.iter().any(|g| g.is_empty()) {
        return None;
    }
    // 所有事件时刻（升序、去重）
    let mut times: Vec<f64> = groups
        .iter()
        .flat_map(|g| g.iter().filter(|(_, e)| *e).map(|(t, _)| *t))
        .collect();
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    times.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
    if times.is_empty() {
        return None;
    }

    let mut o = vec![0.0_f64; k];
    let mut e = vec![0.0_f64; k];
    // 协方差（k×k）：V_jj = Σ d(n−d)·n_j(n−n_j)/(n²(n−1))；V_jl = −Σ d(n−d)·n_j·n_l/(n²(n−1))
    let mut v = vec![vec![0.0_f64; k]; k];
    let mut n_events = 0usize;

    for &t in &times {
        let mut nj = vec![0usize; k];
        let mut dj = vec![0usize; k];
        for (j, g) in groups.iter().enumerate() {
            nj[j] = g.iter().filter(|(tt, _)| *tt >= t).count();
            dj[j] = g
                .iter()
                .filter(|(tt, ee)| (*tt - t).abs() < 1e-12 && *ee)
                .count();
        }
        let n: f64 = nj.iter().map(|&x| x as f64).sum();
        let d: f64 = dj.iter().map(|&x| x as f64).sum();
        if n <= 1.0 || d == 0.0 {
            continue;
        }
        n_events += d as usize;
        for j in 0..k {
            o[j] += dj[j] as f64;
            e[j] += d * nj[j] as f64 / n;
            for l in 0..k {
                let term = d * (n - d) / (n * n * (n - 1.0));
                if j == l {
                    v[j][l] += term * nj[j] as f64 * (n - nj[j] as f64);
                } else {
                    v[j][l] -= term * nj[j] as f64 * nj[l] as f64;
                }
            }
        }
    }
    if n_events == 0 {
        return None;
    }

    // 2 组：chi2 = (O−E)²/V，另有 HR
    if k == 2 {
        let u = o[0] - e[0];
        let vv = v[0][0];
        if vv <= 0.0 {
            return None;
        }
        let chi2 = u * u / vv;
        let ln_hr = u / vv;
        let se = 1.0 / vv.sqrt();
        return Some(LogRank {
            chi2,
            df: 1,
            p: crate::core::contingency::chi2_sf(chi2, 1.0),
            hr: Some(ln_hr.exp()),
            ci_low: Some((ln_hr - 1.96 * se).exp()),
            ci_high: Some((ln_hr + 1.96 * se).exp()),
            n_events,
        });
    }

    // k 组：Q = U'V⁻¹U，只取前 k−1 个分量（降秩）
    let m = k - 1;
    let mut mat = vec![vec![0.0_f64; m]; m];
    for i in 0..m {
        for j in 0..m {
            mat[i][j] = v[i][j];
        }
    }
    let inv = mat_inv(&mat)?;
    let u: Vec<f64> = (0..m).map(|j| o[j] - e[j]).collect();
    let mut chi2 = 0.0;
    for i in 0..m {
        for j in 0..m {
            chi2 += u[i] * inv[i][j] * u[j];
        }
    }
    let chi2 = chi2.max(0.0);
    Some(LogRank {
        chi2,
        df: m,
        p: crate::core::contingency::chi2_sf(chi2, m as f64),
        hr: None,
        ci_low: None,
        ci_high: None,
        n_events,
    })
}

/// 小矩阵求逆（Gauss–Jordan，带部分主元）。矩阵奇异时返回 None。
fn mat_inv(m: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = m.len();
    if n == 0 {
        return None;
    }
    let mut a = m.to_vec();
    let mut id = vec![vec![0.0_f64; n]; n];
    for i in 0..n {
        id[i][i] = 1.0;
    }
    for col in 0..n {
        let mut piv = col;
        for r in col..n {
            if a[r][col].abs() > a[piv][col].abs() {
                piv = r;
            }
        }
        if a[piv][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, piv);
        id.swap(col, piv);
        let d = a[col][col];
        for j in 0..n {
            a[col][j] /= d;
            id[col][j] /= d;
        }
        for r in 0..n {
            if r == col {
                continue;
            }
            let f = a[r][col];
            if f == 0.0 {
                continue;
            }
            for j in 0..n {
                a[r][j] -= f * a[col][j];
                id[r][j] -= f * id[col][j];
            }
        }
    }
    Some(id)
}

// ---------------------------------------------------------------------------
// ANCOVA：分组斜率同质性（交互项）检验
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct AncovaInteraction {
    pub f: f64,
    pub df_num: f64,
    pub df_den: f64,
    pub p: f64,
    pub n: usize,
}

/// ANCOVA 交互项检验：各组回归斜率是否真有差异（规范 C1-4）。
///
/// 每组一条 y = a_j + b_j·x。全模型 2k 个参数（每组独立斜率），
/// 约简模型 k+1 个参数（公共斜率、每组独立截距）：
/// b = ΣSxy_j / ΣSxx_j（组内去心），a_j = ȳ_j − b·x̄_j。
/// F = ((RSS_r − RSS_f)/(k−1)) / (RSS_f/(n−2k))。
///
/// 斜率无显著差异时（p 大），读者有权知道「分组画线」只是展示，
/// 不是斜率真的不同——这个结论写进图注。
pub fn ancova_interaction(series: &[Vec<(f64, f64)>]) -> Option<AncovaInteraction> {
    let k = series.len();
    if k < 2 {
        return None;
    }
    let mut n = 0usize;
    let mut sxx = 0.0;
    let mut sxy = 0.0;
    let mut rss_f = 0.0;
    for g in series {
        if g.len() < 2 {
            return None;
        }
        n += g.len();
        let mx = mean(&g.iter().map(|p| p.0).collect::<Vec<_>>());
        let my = mean(&g.iter().map(|p| p.1).collect::<Vec<_>>());
        let (mut gx, mut gy) = (0.0, 0.0);
        for &(x, y) in g {
            gx += (x - mx) * (x - mx);
            gy += (x - mx) * (y - my);
        }
        sxx += gx;
        sxy += gy;
        // 组内拟合（每组独立斜率）→ 全模型 RSS
        let b = if gx > 0.0 { gy / gx } else { 0.0 };
        for &(x, y) in g {
            let resid = y - (my - b * mx + b * x);
            rss_f += resid * resid;
        }
    }
    let df_den = (n as f64) - 2.0 * k as f64;
    if df_den <= 0.0 || sxx <= 0.0 {
        return None;
    }
    // 公共斜率模型
    let b = sxy / sxx;
    let mut rss_r = 0.0;
    for g in series {
        let mx = mean(&g.iter().map(|p| p.0).collect::<Vec<_>>());
        let my = mean(&g.iter().map(|p| p.1).collect::<Vec<_>>());
        let a = my - b * mx;
        for &(x, y) in g {
            let resid = y - (a + b * x);
            rss_r += resid * resid;
        }
    }
    let df_num = (k - 1) as f64;
    // 退化：全模型完美拟合（RSS_f ≈ 0）。约简模型也完美 → 斜率无差异（F=0）；
    // 约简模型更差 → F = ∞（交互确定存在）。
    let scale = sxx.max(1.0);
    let f = if rss_f <= 1e-12 * scale {
        if rss_r <= 1e-12 * scale {
            0.0
        } else {
            f64::INFINITY
        }
    } else {
        ((rss_r - rss_f) / df_num) / (rss_f / df_den)
    };
    if !f.is_finite() && !f.is_infinite() {
        return None;
    }
    let f = f.max(0.0);
    let p = if f.is_infinite() {
        0.0
    } else {
        f_sf(f, df_num, df_den)
    };
    Some(AncovaInteraction {
        f,
        df_num,
        df_den,
        p,
        n,
    })
}

// ---------------------------------------------------------------------------
// 双因素方差分析（两因子固定效应，格内有重复）
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct TwoWayAnova {
    pub a: usize,
    pub b: usize,
    pub n: usize,
    pub ss_a: f64,
    pub ss_b: f64,
    pub ss_ab: f64,
    pub ss_err: f64,
    pub df_a: f64,
    pub df_b: f64,
    pub df_ab: f64,
    pub df_err: f64,
    pub f_a: f64,
    pub f_b: f64,
    pub f_ab: f64,
    pub p_a: f64,
    pub p_b: f64,
    pub p_ab: f64,
    /// 偏 η² = SS_效应 / (SS_效应 + SS_误差)
    pub eta_a: f64,
    pub eta_b: f64,
    pub eta_ab: f64,
}

/// 双因素方差分析：`cells[i][j]` = 因子 A 第 i 水平 × 因子 B 第 j 水平的全部观测。
///
/// 教科书分解（平衡时与经典公式一致；不平衡时为加权分解，各平方和仍精确闭合）：
/// SS_A、SS_B 由边际均值加权，SS_AxB = SS_格间 − SS_A − SS_B，SS_err 为格内残差。
pub fn two_way_anova(cells: &[Vec<Vec<f64>>]) -> Option<TwoWayAnova> {
    let a = cells.len();
    if a < 2 {
        return None;
    }
    let b = cells[0].len();
    if b < 2 {
        return None;
    }
    if cells.iter().any(|r| r.len() != b) {
        return None;
    }
    if cells.iter().flat_map(|r| r.iter()).any(|c| c.is_empty()) {
        return None; // 允许不平衡，但不允许空格子
    }
    let flat: Vec<f64> = cells
        .iter()
        .flat_map(|r| r.iter().flatten().copied())
        .collect();
    let n_tot = flat.len();
    let gm = mean(&flat);
    let mut ss_err = 0.0;
    let mut ss_cells = 0.0;
    for row in cells {
        for c in row {
            let m = mean(c);
            for &v in c {
                ss_err += (v - m) * (v - m);
            }
            ss_cells += c.len() as f64 * (m - gm) * (m - gm);
        }
    }
    let n_i: Vec<f64> = cells
        .iter()
        .map(|r| r.iter().map(|c| c.len()).sum::<usize>() as f64)
        .collect();
    let m_i: Vec<f64> = cells
        .iter()
        .zip(&n_i)
        .map(|(r, ni)| r.iter().flatten().sum::<f64>() / ni)
        .collect();
    let n_j: Vec<f64> = (0..b)
        .map(|j| cells.iter().map(|r| r[j].len()).sum::<usize>() as f64)
        .collect();
    let m_j: Vec<f64> = (0..b)
        .map(|j| cells.iter().map(|r| r[j].iter().sum::<f64>()).sum::<f64>() / n_j[j])
        .collect();
    let ss_a: f64 = (0..a).map(|i| n_i[i] * (m_i[i] - gm).powi(2)).sum();
    let ss_b: f64 = (0..b).map(|j| n_j[j] * (m_j[j] - gm).powi(2)).sum();
    let ss_ab = ss_cells - ss_a - ss_b;
    let df_a = (a - 1) as f64;
    let df_b = (b - 1) as f64;
    let df_ab = df_a * df_b;
    let df_err = (n_tot - a * b) as f64;
    if df_err < 1.0 {
        return None;
    }
    let ms_err = ss_err / df_err;
    let f_of = |ss: f64, df: f64| {
        if ms_err > 0.0 {
            (ss / df) / ms_err
        } else {
            0.0
        }
    };
    let f_a = f_of(ss_a, df_a);
    let f_b = f_of(ss_b, df_b);
    let f_ab = f_of(ss_ab, df_ab);
    let eta = |ss: f64| {
        if ss + ss_err > 0.0 {
            ss / (ss + ss_err)
        } else {
            0.0
        }
    };
    Some(TwoWayAnova {
        a,
        b,
        n: n_tot,
        ss_a,
        ss_b,
        ss_ab,
        ss_err,
        df_a,
        df_b,
        df_ab,
        df_err,
        f_a,
        f_b,
        f_ab,
        p_a: f_sf(f_a, df_a, df_err),
        p_b: f_sf(f_b, df_b, df_err),
        p_ab: f_sf(f_ab, df_ab, df_err),
        eta_a: eta(ss_a),
        eta_b: eta(ss_b),
        eta_ab: eta(ss_ab),
    })
}

// ---------------------------------------------------------------------------
// 单因素重复测量方差分析
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct RmAnova {
    pub subjects: usize,
    pub conditions: usize,
    pub ss_cond: f64,
    pub ss_subj: f64,
    pub ss_err: f64,
    pub df_cond: f64,
    pub df_subj: f64,
    pub df_err: f64,
    pub f: f64,
    pub p: f64,
    pub eta: f64,
    /// Greenhouse–Geisser ε（球形性偏离程度，1 = 满球形）
    pub gg_eps: f64,
    /// GG 校正后的自由度与 p（F 不变，df × ε）
    pub df_cond_gg: f64,
    pub df_err_gg: f64,
    pub p_gg: f64,
}

/// 单因素重复测量方差分析：`cols[t]` = 第 t 个处理下各受试者的观测（按受试者对齐）。
///
/// 平方和分解：SS_处理 + SS_受试者 + SS_残差 = SS_总。
/// 受试者作为自己的对照，个体间变异从误差里剥离（比单因素组间更灵敏）。
/// 未做球形性校正（Greenhouse–Geisser / Huynh–Feldt），结果按「满足球形性」解读。
pub fn rm_anova(cols: &[Vec<f64>]) -> Option<RmAnova> {
    let c = cols.len();
    if c < 2 {
        return None;
    }
    let s = cols[0].len();
    if s < 2 || cols.iter().any(|col| col.len() != s) {
        return None;
    }
    let vals: Vec<f64> = cols.iter().flatten().copied().collect();
    let gm = mean(&vals);
    let ss_tot: f64 = vals.iter().map(|v| (v - gm).powi(2)).sum();
    let m_c: Vec<f64> = cols.iter().map(|col| mean(col)).collect();
    let m_s: Vec<f64> = (0..s)
        .map(|i| cols.iter().map(|col| col[i]).sum::<f64>() / c as f64)
        .collect();
    let ss_cond = s as f64 * m_c.iter().map(|m| (m - gm).powi(2)).sum::<f64>();
    let ss_subj = c as f64 * m_s.iter().map(|m| (m - gm).powi(2)).sum::<f64>();
    let ss_err = ss_tot - ss_cond - ss_subj;
    let df_cond = (c - 1) as f64;
    let df_err = ((c - 1) * (s - 1)) as f64;
    let ms_err = ss_err / df_err;
    let f = if ms_err > 0.0 {
        (ss_cond / df_cond) / ms_err
    } else {
        0.0
    };
    let eta = if ss_cond + ss_err > 0.0 {
        ss_cond / (ss_cond + ss_err)
    } else {
        0.0
    };
    // 条件得分的协方差矩阵（无偏除数 n−1）→ Greenhouse–Geisser ε
    let mut cov = vec![vec![0.0; c]; c];
    for a in 0..c {
        for b in 0..c {
            let mut acc = 0.0;
            for i in 0..s {
                acc += (cols[a][i] - m_c[a]) * (cols[b][i] - m_c[b]);
            }
            cov[a][b] = acc / (s as f64 - 1.0);
        }
    }
    let gg_eps = gg_epsilon(&cov).unwrap_or(1.0);
    let df_cond_gg = df_cond * gg_eps;
    let df_err_gg = df_err * gg_eps;
    Some(RmAnova {
        subjects: s,
        conditions: c,
        ss_cond,
        ss_subj,
        ss_err,
        df_cond,
        df_subj: (s - 1) as f64,
        df_err,
        f,
        p: f_sf(f, df_cond, df_err),
        eta,
        gg_eps,
        df_cond_gg,
        df_err_gg,
        p_gg: f_sf(f, df_cond_gg, df_err_gg),
    })
}

// ---------------------------------------------------------------------------
// ROC 曲线与 AUC
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct Roc {
    /// AUC = P(阳性得分 > 阴性得分) + ½P(相等)（Mann–Whitney U / n⁺n⁻）
    pub auc: f64,
    pub se: f64,
    pub ci_low: f64,
    pub ci_high: f64,
    /// Youden J 最大处的界值（判阳：marker ≥ cutoff）
    pub cutoff: f64,
    pub sens: f64,
    pub spec: f64,
    pub youden: f64,
    pub n_pos: usize,
    pub n_neg: usize,
    /// 曲线点 (1−特异度, 灵敏度)，按界值从高到低
    pub points: Vec<(f64, f64)>,
}

/// ROC 分析：`pos` = 阳性组（病例）观测，`neg` = 阴性组（对照）观测。
///
/// - AUC 用秩统计精确计算（含 0.5 结分），不是梯形近似；
/// - 95% CI 用 Hanley–McNeil 近似标准误；
/// - 最佳界值取 Youden J = 灵敏度 + 特异度 − 1 的最大处（并列取界值较高者，
///   即更保守的那个）。
pub fn roc(pos: &[f64], neg: &[f64]) -> Option<Roc> {
    if pos.is_empty() || neg.is_empty() {
        return None;
    }
    let (n1, n0) = (pos.len(), neg.len());
    // AUC：逐对比较（含结分）
    let mut wins = 0.0;
    for &p in pos {
        for &n in neg {
            wins += if p > n {
                1.0
            } else if p == n {
                0.5
            } else {
                0.0
            };
        }
    }
    let auc = wins / (n1 as f64 * n0 as f64);
    // Hanley–McNeil 近似 SE（AUC=0/1 时退化为 0）
    let (q1, q2) = if auc >= 1.0 {
        (1.0, 1.0)
    } else if auc <= 0.0 {
        (0.0, 0.0)
    } else {
        (auc / (2.0 - auc), 2.0 * auc * auc / (1.0 + auc))
    };
    let se = ((auc * (1.0 - auc)
        + (n1 as f64 - 1.0) * (q1 - auc * auc)
        + (n0 as f64 - 1.0) * (q2 - auc * auc))
        / (n1 as f64 * n0 as f64))
        .max(0.0)
        .sqrt();

    // 全部候选界值 = 观测值并集；判阳规则 marker ≥ t
    let mut vals: Vec<f64> = pos.iter().chain(neg.iter()).copied().collect();
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    vals.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
    let sens_at = |t: f64| pos.iter().filter(|&&v| v >= t).count() as f64 / n1 as f64;
    let spec_at = |t: f64| neg.iter().filter(|&&v| v < t).count() as f64 / n0 as f64;
    let mut best: Option<(f64, f64, f64, f64)> = None; // (J, cutoff, sens, spec)
    for &t in &vals {
        let (se_, sp) = (sens_at(t), spec_at(t));
        let j = se_ + sp - 1.0;
        let better = match best {
            None => true,
            Some((bj, bt, _, _)) => {
                j > bj + 1e-12 || ((j - bj).abs() <= 1e-12 && t > bt) // 并列取高界值
            }
        };
        if better {
            best = Some((j, t, se_, sp));
        }
    }
    let (youden, cutoff, sens, spec) = best?;

    // 曲线点：界值从高到低（含 (0,0)→(1,1) 两端）
    let mut points: Vec<(f64, f64)> = vec![(0.0, 0.0)];
    for &t in vals.iter().rev() {
        points.push((1.0 - spec_at(t), sens_at(t)));
    }
    points.push((1.0, 1.0));

    Some(Roc {
        auc,
        se,
        ci_low: (auc - 1.96 * se).clamp(0.0, 1.0),
        ci_high: (auc + 1.96 * se).clamp(0.0, 1.0),
        cutoff,
        sens,
        spec,
        youden,
        n_pos: n1,
        n_neg: n0,
        points,
    })
}

/// Greenhouse–Geisser ε：由条件得分的协方差矩阵估计球形性偏离程度。
///
/// ε = tr(A)² / ((k−1)·tr(A²))，A = M·S·M（M = I − J/k），等价地
/// A_ij = S_ij − S_i· − S_·j + S_··。ε ∈ [1/(k−1), 1]；ε=1 球形成立，
/// 越小越偏离。手算基准：S=I₃ → ε=1；S=diag(1,1,4) → tr(A)=4、tr(A²)=10 → ε=0.8。
pub fn gg_epsilon(s: &[Vec<f64>]) -> Option<f64> {
    let k = s.len();
    if k < 2 || s.iter().any(|r| r.len() != k) {
        return None;
    }
    let row_mean = |i: usize| s[i].iter().sum::<f64>() / k as f64;
    let grand: f64 = s.iter().flat_map(|r| r.iter()).sum::<f64>() / (k * k) as f64;
    let mut a = vec![vec![0.0; k]; k];
    for i in 0..k {
        for j in 0..k {
            a[i][j] = s[i][j] - row_mean(i) - row_mean(j) + grand;
        }
    }
    let tr: f64 = (0..k).map(|i| a[i][i]).sum();
    let mut tr2 = 0.0;
    for i in 0..k {
        for j in 0..k {
            tr2 += a[i][j] * a[j][i];
        }
    }
    if tr2 <= 0.0 {
        return Some(1.0); // 退化（完全共线等）→ 按满球形处理
    }
    let eps = tr * tr / (((k - 1) as f64) * tr2);
    Some(eps.clamp(1.0 / (k - 1) as f64, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn t_two_sided_known() {
        assert!((t_p(1.0, 8.0) - 0.346_59).abs() < 1e-4);
        assert!((t_p(2.0, 10.0) - 0.073_39).abs() < 1e-4);
    }

    #[test]
    fn f_sf_known() {
        // F(3,20) 的 95% 分位约 3.0984
        assert!((f_sf(3.0984, 3.0, 20.0) - 0.05).abs() < 2e-4);
    }

    #[test]
    fn normal_quantile() {
        assert!((norm_ppf(0.975) - 1.959_964).abs() < 1e-4);
        assert!((norm_cdf(1.96) - 0.975).abs() < 1e-4);
    }

    #[test]
    fn unpaired_t_matches_hand_calc() {
        let a = [1.0, 2.0, 3.0, 4.0, 5.0];
        let b = [2.0, 3.0, 4.0, 5.0, 6.0];
        let r = t_test_unpaired(&a, &b, false).unwrap();
        assert!((r.t + 1.0).abs() < 1e-9);
        assert!((r.df - 8.0).abs() < 1e-9);
        assert!((r.p - 0.346_59).abs() < 1e-4);
    }

    #[test]
    fn paired_t_constant_diff() {
        let a = [2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        let b = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let r = t_test_paired(&a, &b).unwrap();
        assert!((r.diff - 1.0).abs() < 1e-12);
        assert!(r.se < 1e-12);
        assert_eq!(r.p, 1.0);
    }

    #[test]
    fn anova_identical_groups() {
        let g = vec![
            vec![1.0, 2.0, 3.0],
            vec![1.0, 2.0, 3.0],
            vec![1.0, 2.0, 3.0],
        ];
        let r = anova_oneway(&g).unwrap();
        assert!(r.f.abs() < 1e-12);
        assert!((r.p - 1.0).abs() < 1e-12);
    }

    #[test]
    fn anova_known_f() {
        let g = vec![
            vec![1.0, 2.0, 3.0, 4.0],
            vec![4.0, 5.0, 6.0, 7.0],
            vec![7.0, 8.0, 9.0, 10.0],
        ];
        let r = anova_oneway(&g).unwrap();
        // 手算：ssb=72, ssw=15, df 2/9 → F=21.6
        assert!((r.f - 21.6).abs() < 1e-9);
        assert!((r.p - 0.000_366).abs() < 1e-6);
    }

    #[test]
    fn mann_whitney_exact_small() {
        let (u, p) = mann_whitney(&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]).unwrap();
        assert!(u.abs() < 1e-12);
        assert!((p - 0.1).abs() < 1e-12);
    }

    #[test]
    fn wilcoxon_all_positive() {
        // 差值取 1..=10（互不相同），走精确分布；全为正 → W = 0。
        let b: Vec<f64> = (0..10).map(|v| v as f64).collect();
        let a: Vec<f64> = b
            .iter()
            .enumerate()
            .map(|(i, v)| v + (i + 1) as f64)
            .collect();
        let (_w, p) = wilcoxon(&a, &b).unwrap();
        assert!((p - 2.0 / 1024.0).abs() < 1e-12);
    }

    #[test]
    fn correlation_perfect() {
        let x: Vec<f64> = (0..10).map(|v| v as f64).collect();
        let y: Vec<f64> = x.iter().map(|v| 2.0 * v + 1.0).collect();
        let c = pearson(&x, &y).unwrap();
        assert!((c.r - 1.0).abs() < 1e-12);
        assert!(c.p < 1e-9);
        let lr = linreg(&x, &y).unwrap();
        assert!((lr.slope - 2.0).abs() < 1e-12);
        assert!((lr.intercept - 1.0).abs() < 1e-12);
    }

    #[test]
    fn dagostino_uniform_not_normal() {
        let x: Vec<f64> = (0..100).map(|v| v as f64).collect();
        let (_k, p) = dagostino(&x).unwrap();
        assert!(p < 0.01);
    }

    #[test]
    fn quantile_type7() {
        let v = [1.0, 2.0, 3.0, 4.0];
        assert!((quantile(&v, 0.25) - 1.75).abs() < 1e-12);
    }

    #[test]
    fn ln_gamma_basics() {
        assert!((ln_gamma(5.0) - (24.0f64).ln()).abs() < 1e-9);
        assert!((ln_gamma(0.5) - std::f64::consts::PI.sqrt().ln()).abs() < 1e-9);
    }
}

#[cfg(test)]
mod rank_tests {
    use super::*;

    /// 三组各 2 个、无并列，秩分别为 [1,2] [3,4] [5,6]。
    /// 手算：H = 12/(6·7)·(2·1.5² + 2·3.5² + 2·5.5²) − 3·7 = 4.5714
    fn clean_groups() -> Vec<Vec<f64>> {
        vec![vec![1.0, 2.0], vec![3.0, 4.0], vec![5.0, 6.0]]
    }

    #[test]
    fn KW统计量可手算复现() {
        let r = kruskal_wallis(&clean_groups()).expect("kw");
        assert!((r.h - 4.571_428_571).abs() < 1e-6, "H = {}", r.h);
        assert_eq!(r.df, 2.0);
        // df=2 时 χ² 上尾 = exp(−H/2)
        let expect_p = (-r.h / 2.0).exp();
        assert!((r.p - expect_p).abs() < 1e-9, "p = {}", r.p);
        assert_eq!(r.mean_ranks, vec![1.5, 3.5, 5.5]);
        assert_eq!(r.tie_groups, 0);
    }

    #[test]
    fn KW并列校正使H变小() {
        // 引入并列：第二、三组都出现 3.0
        let tied = vec![vec![1.0, 2.0], vec![3.0, 4.0], vec![3.0, 6.0]];
        let r = kruskal_wallis(&tied).expect("kw");
        assert!(r.tie_groups > 0, "应检测到并列");
        // C = 1 − Σ(t³−t)/(N³−N) < 1，所以校正后 H/C > H：
        // 并列压缩了秩的离散度，必须把 H 放大回去，否则 p 会偏大（过于保守）。
        assert!(
            r.h_corrected > r.h,
            "校正后 {} 应大于未校正 {}",
            r.h_corrected,
            r.h
        );
    }

    #[test]
    fn KW对强分离数据显著() {
        let g = vec![
            vec![1.0, 2.0, 1.5],
            vec![10.0, 11.0, 10.5],
            vec![20.0, 21.0, 20.5],
        ];
        let r = kruskal_wallis(&g).expect("kw");
        assert!(r.p < 0.05, "p = {}", r.p);
        assert!(r.epsilon2 > 0.5);
    }

    #[test]
    fn KW单组或空组返回None() {
        assert!(kruskal_wallis(&[vec![1.0, 2.0]]).is_none());
        assert!(kruskal_wallis(&[]).is_none());
        // 只有一个非空组
        assert!(kruskal_wallis(&[vec![1.0, 2.0], vec![]]).is_none());
    }

    #[test]
    fn Dunn两两比较数量正确() {
        let pairs = posthoc_dunn(&clean_groups());
        // 3 组 → C(3,2) = 3 对
        assert_eq!(pairs.len(), 3);
        for p in &pairs {
            assert!(p.p_raw >= 0.0 && p.p_raw <= 1.0);
            assert!(p.p_holm >= p.p_raw - 1e-12, "Holm 不应小于原始 p");
        }
    }

    #[test]
    fn Dunn的z值方向与秩差一致() {
        let pairs = posthoc_dunn(&clean_groups());
        for p in &pairs {
            // 秩差与 z 必须同号
            assert!(
                (p.rank_diff > 0.0) == (p.z > 0.0),
                "秩差 {} 与 z {} 方向不一致",
                p.rank_diff,
                p.z
            );
        }
    }

    #[test]
    fn Dunn对相邻组不显著对两端组显著() {
        // 三组分离很清楚：1-2 与 2-3 差别小，1-3 应更显著
        let g = vec![
            vec![1.0, 2.0, 3.0],
            vec![4.0, 5.0, 6.0],
            vec![7.0, 8.0, 9.0],
        ];
        let pairs = posthoc_dunn(&g);
        let end = pairs.iter().find(|p| p.a == 0 && p.b == 2).expect("端点对");
        let near = pairs.iter().find(|p| p.a == 0 && p.b == 1).expect("相邻对");
        assert!(end.z.abs() > near.z.abs());
    }

    #[test]
    fn Dunn空输入安全() {
        assert!(posthoc_dunn(&[]).is_empty());
        assert!(posthoc_dunn(&[vec![1.0]]).is_empty());
    }
}

#[cfg(test)]
mod fit_tests {
    use super::*;

    #[test]
    fn 完美线性拟合() {
        let xs = [1.0, 2.0, 3.0, 4.0, 5.0];
        let ys = [3.0, 5.0, 7.0, 9.0, 11.0];
        let f = fit_line(&xs, &ys).expect("fit");
        assert!((f.slope - 2.0).abs() < 1e-12);
        assert!((f.intercept - 1.0).abs() < 1e-12);
        assert!((f.r2 - 1.0).abs() < 1e-12);
        assert!(f.ci95(3.0, 2.0) < 1e-9, "完美拟合的置信带应为 0");
    }

    #[test]
    fn 置信带在两端更宽() {
        let xs = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let ys = [2.1, 3.9, 6.2, 7.8, 10.1, 12.2];
        let f = fit_line(&xs, &ys).expect("fit");
        let tc = t_quantile(0.975, (f.n - 2) as f64);
        let mid = f.ci95(f.mx, tc);
        let edge = f.ci95(1.0, tc);
        assert!(edge > mid, "edge {edge} 应大于中间 {mid}");
    }

    #[test]
    fn 样本不足返回空() {
        assert!(fit_line(&[1.0, 2.0], &[1.0, 2.0]).is_none());
        // x 全相同无法拟合
        assert!(fit_line(&[1.0, 1.0, 1.0], &[1.0, 2.0, 3.0]).is_none());
    }

    /// 双因素手算基准（平衡 2×3、格内 n=2 的加性数据）：
    /// SS_A=3, SS_B=32, SS_AxB=0, SS_err=3, F_A=6, F_B=32, F_AxB=0。
    #[test]
    fn 双因素ANOVA手算基准() {
        let cells = vec![
            vec![vec![1.0, 2.0], vec![3.0, 4.0], vec![5.0, 6.0]],
            vec![vec![2.0, 3.0], vec![4.0, 5.0], vec![6.0, 7.0]],
        ];
        let r = two_way_anova(&cells).expect("two-way");
        assert!((r.ss_a - 3.0).abs() < 1e-9, "SS_A={}", r.ss_a);
        assert!((r.ss_b - 32.0).abs() < 1e-9, "SS_B={}", r.ss_b);
        assert!(r.ss_ab.abs() < 1e-9, "加性数据 SS_AxB={}", r.ss_ab);
        assert!((r.ss_err - 3.0).abs() < 1e-9, "SS_err={}", r.ss_err);
        assert!((r.f_a - 6.0).abs() < 1e-9, "F_A={}", r.f_a);
        assert!((r.f_b - 32.0).abs() < 1e-9, "F_B={}", r.f_b);
        assert!(r.f_ab.abs() < 1e-9);
        assert!(r.p_b < 0.01 && r.p_a < 0.1);
    }

    /// 交互项存在时 SS_AxB > 0 且显著。
    #[test]
    fn 双因素ANOVA检出交互() {
        // 交叉模式：B 的效应在两个 A 水平上反号
        let cells = vec![
            vec![vec![1.0, 2.0], vec![5.0, 6.0]],
            vec![vec![5.0, 6.0], vec![1.0, 2.0]],
        ];
        let r = two_way_anova(&cells).unwrap();
        assert!(r.ss_ab > 0.0, "SS_AxB={}", r.ss_ab);
        assert!(r.p_ab < 0.05, "p_ab={}", r.p_ab);
    }

    /// 单因素重复测量手算基准：4 受试者 × 3 处理，
    /// SS_处理=12.5, SS_受试者=6, SS_残差=1.5, F=25。
    #[test]
    fn 重复测量ANOVA手算基准() {
        let cols = vec![
            vec![1.0, 2.0, 3.0, 1.0],
            vec![2.0, 3.0, 4.0, 3.0],
            vec![3.0, 4.0, 5.0, 5.0],
        ];
        let r = rm_anova(&cols).expect("rm");
        assert!((r.ss_cond - 12.5).abs() < 1e-9, "{}", r.ss_cond);
        assert!((r.ss_subj - 6.0).abs() < 1e-9, "{}", r.ss_subj);
        assert!((r.ss_err - 1.5).abs() < 1e-9, "{}", r.ss_err);
        assert!((r.f - 25.0).abs() < 1e-9, "F={}", r.f);
        assert!(r.p < 0.01);
    }

    /// ROC 手算基准：完全分离 → AUC = 1。
    #[test]
    fn roc完全分离AUC为1() {
        let r = roc(&[4.0, 5.0, 6.0], &[1.0, 2.0, 3.0]).unwrap();
        assert!((r.auc - 1.0).abs() < 1e-12, "{}", r.auc);
        // 界值取 4（阳性全部 ≥4、阴性全部 <4）：灵敏度=特异度=1
        assert!((r.cutoff - 4.0).abs() < 1e-12, "cutoff={}", r.cutoff);
        assert!((r.sens - 1.0).abs() < 1e-12 && (r.spec - 1.0).abs() < 1e-12);
        assert!((r.youden - 1.0).abs() < 1e-12);
    }

    /// ROC 手算基准：pos=[3,4,5], neg=[1,2,3] → 逐对 8.5/9 = 0.944…
    #[test]
    fn rocAUC按逐对比较手算() {
        let r = roc(&[3.0, 4.0, 5.0], &[1.0, 2.0, 3.0]).unwrap();
        assert!((r.auc - 8.5 / 9.0).abs() < 1e-12, "auc={}", r.auc);
        assert!(r.ci_low < r.auc && r.auc < r.ci_high);
    }

    /// ROC 手算基准：交叠数据 pos=[2,3,4], neg=[1,2,5] → 5.5/9 = 0.611…
    #[test]
    fn roc交叠数据低于随机太多() {
        let r = roc(&[2.0, 3.0, 4.0], &[1.0, 2.0, 5.0]).unwrap();
        assert!((r.auc - 5.5 / 9.0).abs() < 1e-12, "auc={}", r.auc);
    }

    /// Youden：pos=[3,4,5], neg=[1,2,3]。
    /// t=4：sens=2/3, spec=3/3 → J=2/3；t=3：sens=3/3, spec=2/3 → J=2/3 并列，
    /// 规则取高界值 → cutoff=4。
    #[test]
    fn youden并列取高界值() {
        let r = roc(&[3.0, 4.0, 5.0], &[1.0, 2.0, 3.0]).unwrap();
        assert!((r.cutoff - 4.0).abs() < 1e-12, "cutoff={}", r.cutoff);
        assert!((r.sens - 2.0 / 3.0).abs() < 1e-12);
        assert!((r.spec - 1.0).abs() < 1e-12);
    }

    /// GG ε 手算基准：S = I₃（所有对比方差相等，球形成立）→ ε = 1。
    #[test]
    fn gg_epsilon满球形为1() {
        let s = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![0.0, 0.0, 1.0],
        ];
        assert!((gg_epsilon(&s).unwrap() - 1.0).abs() < 1e-12);
    }

    /// GG ε 手算基准：S = diag(1,1,4) → A 的 tr=4、tr(A²)=10 → ε = 16/20 = 0.8。
    #[test]
    fn gg_epsilon手算基准08() {
        let s = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![0.0, 0.0, 4.0],
        ];
        assert!((gg_epsilon(&s).unwrap() - 0.8).abs() < 1e-12);
    }

    /// 手算例（两组各 2 人）：
    /// A: 事件 t=1,2；B: 删失 t=1、事件 t=2。
    /// t=1: n_A=n_B=2,n=4,d=1 → E_A=0.5, V_AA=1·3·2·2/(16·3)=0.25
    /// t=2: n_A=n_B=1,n=2,d=2 → E_A=1.0, V 贡献=0（d=n）
    /// O_A=2, E_A=1.5, u=0.5 → chi2=u²/V=1.0，HR=exp(u/V)=e²
    #[test]
    fn log_rank手算例() {
        let a = vec![(1.0, true), (2.0, true)];
        let b = vec![(1.0, false), (2.0, true)];
        let r = log_rank(&[a, b]).expect("log-rank");
        assert!((r.chi2 - 1.0).abs() < 1e-9, "chi2 = {}", r.chi2);
        assert_eq!(r.df, 1);
        let expect_p = crate::core::contingency::chi2_sf(1.0, 1.0);
        assert!((r.p - expect_p).abs() < 1e-12);
        let hr = r.hr.unwrap();
        assert!((hr - 2.0f64.exp()).abs() < 1e-9, "HR = {hr}");
        let (lo, hi) = (r.ci_low.unwrap(), r.ci_high.unwrap());
        assert!(lo < hr && hr < hi, "CI 应包住点估计 [{lo}, {hr}, {hi}]");
    }

    #[test]
    fn log_rank完全分离给出显著结果() {
        let a: Vec<(f64, bool)> = (0..6).map(|i| (i as f64 + 1.0, true)).collect();
        let b: Vec<(f64, bool)> = (0..6).map(|i| (i as f64 + 7.0, true)).collect();
        let r = log_rank(&[a, b]).unwrap();
        assert!(r.p < 0.001, "p = {}", r.p);
        // HR = exp((O_A−E_A)/V) 是 A 相对 B 的风险比：A 更早发生事件
        // = A 的瞬时风险更高 → HR(A/B) > 1（方向别记反）。
        assert!(r.hr.unwrap() > 1.0, "A 组更早事件 → HR(A/B) > 1");
    }

    #[test]
    fn log_rank多组给出整体检验() {
        let g: Vec<Vec<(f64, bool)>> = (0..3)
            .map(|k| {
                (0..8)
                    .map(|i| (i as f64 + 1.0 + k as f64 * 6.0, true))
                    .collect()
            })
            .collect();
        let r = log_rank(&g).unwrap();
        assert_eq!(r.df, 2);
        assert!(r.p < 0.01, "阶梯分离的三组 p = {}", r.p);
        assert!(r.hr.is_none(), "多组没有单一 HR");
    }

    /// 两组共线（斜率相同）→ 交互 p 大；斜率明显不同 → p 小。
    #[test]
    fn ancova交互项检出斜率差异() {
        // 加一点确定性微噪声（±0.1），走非退化的真实计算路径。
        let noise = |i: usize| if i % 2 == 0 { 0.1 } else { -0.1 };
        let parallel: Vec<Vec<(f64, f64)>> = vec![
            (0..10)
                .map(|i| (i as f64, i as f64 + 1.0 + noise(i)))
                .collect(),
            (0..10)
                .map(|i| (i as f64, i as f64 + 5.0 + noise(i)))
                .collect(),
        ];
        let r = ancova_interaction(&parallel).unwrap();
        assert!(r.p > 0.5, "平行线 p 应大，实际 {}", r.p);

        let crossing: Vec<Vec<(f64, f64)>> = vec![
            (0..10).map(|i| (i as f64, i as f64 + noise(i))).collect(),
            (0..10)
                .map(|i| (i as f64, 3.0 * i as f64 + noise(i)))
                .collect(),
        ];
        let r = ancova_interaction(&crossing).unwrap();
        assert!(r.p < 0.01, "斜率 1 vs 3 应显著，实际 F={} p={}", r.f, r.p);
    }

    /// Cohen's d 的 95% CI（Hedges–Olkin 近似）应包住点估计且关于它近似对称。
    #[test]
    fn cohen_d置信区间包住点估计() {
        let (d, lo, hi) = cohen_d_ci(1.2, 20, 20);
        assert!((d - 1.2).abs() < 1e-12, "点估计原样返回");
        assert!(lo < d && d < hi, "[{lo}, {hi}] 应包住 {d}");
        assert!((hi - d - (d - lo)).abs() < 1e-9, "近似 CI 对称");
    }

    #[test]
    fn kde_带宽退化有兜底() {
        let h = kde_bandwidth(&[5.0, 5.0, 5.0, 5.0]);
        assert!(h > 0.0 && h.is_finite());
        let normal = kde_bandwidth(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(normal > h);
    }
}
