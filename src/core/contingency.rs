//! 列联表（R×C 计数表）的独立性检验。
//!
//! 口径对齐 GraphPad Prism：
//! - **卡方独立性检验**：给出期望频数、χ²、df、p、Cramér's V；
//!   单元格期望频数过小时提示改用 Fisher。
//! - **Fisher 精确检验**：2×2 走超几何分布逐表求和（双侧），
//!   R×C 走 Freeman–Halton 扩展（同样求和，但把表数限制住，超限用
//!   Monte Carlo 近似并明确标注）。
//! - 2×2 额外给**优势比 OR** 与 Woolf 95% CI、Yates 连续性校正的 χ²。
//!
//! 这里刻意不「糊」：期望频数不足、表格退化、零单元格等情形都给出
//! 明确说明，而不是返回一个看起来正常的 p 值。

use crate::core::stats::ln_gamma;

/// 卡方独立性检验结果。
#[derive(Clone, Debug)]
pub struct ChiSquare {
    pub chi2: f64,
    pub df: f64,
    pub p: f64,
    /// Cramér's V（关联强度，0–1）
    pub v: f64,
    pub n: f64,
    pub r: usize,
    pub c: usize,
    /// 期望频数 < 5 的单元格个数（≥1 时卡方近似不可靠）
    pub small_cells: usize,
    /// 最小期望频数
    pub min_expected: f64,
    /// Yates 连续性校正的 χ²（仅 2×2）
    pub yates: Option<f64>,
    /// 优势比（仅 2×2，且无零单元格）
    pub odds_ratio: Option<f64>,
    /// Woolf 95% CI（仅 2×2）
    pub or_ci: Option<(f64, f64)>,
    /// 调整标准化残差矩阵（Approach: Agresti 的 adjusted residuals）。
    ///
    /// `r_ij = (O − E) / sqrt(E · (1 − n_i/n) · (1 − n_j/n))`，近似 N(0,1)。
    /// `|r| > 2` 说明该格子显著地在拉动「行列不独立」这一结论。
    pub residual: Vec<Vec<f64>>,
    /// 残差的显著性阈值（按 Bonferroni 校正到全表单元格数）
    pub residual_crit: f64,
}

/// Fisher 精确检验结果。
#[derive(Clone, Debug)]
pub struct Fisher {
    pub p: f64,
    /// 是否为精确求和（false = Monte Carlo 近似）
    pub exact: bool,
    /// Monte Carlo 近似时的抽样数
    pub samples: usize,
    pub odds_ratio: Option<f64>,
    pub or_ci: Option<(f64, f64)>,
    /// 精确检验时枚举到的列联表个数
    pub tables: usize,
}

/// 校验并清理计数矩阵：要求至少 2×2，非负、有限。
fn validate(counts: &[Vec<f64>]) -> Result<(Vec<Vec<f64>>, usize, usize), String> {
    let r = counts.len();
    if r < 2 {
        return Err("列联表至少需要 2 行".into());
    }
    let c = counts[0].len();
    if c < 2 {
        return Err("列联表至少需要 2 列".into());
    }
    let mut out = Vec::with_capacity(r);
    for row in counts {
        if row.len() != c {
            return Err("各行的列数必须一致".into());
        }
        let mut rr = Vec::with_capacity(c);
        for &v in row {
            if !v.is_finite() {
                return Err("计数不能是空值 / NaN".into());
            }
            if v < 0.0 {
                return Err("计数不能是负数".into());
            }
            rr.push(v.round());
        }
        out.push(rr);
    }
    let n: f64 = out.iter().flatten().sum();
    if n <= 0.0 {
        return Err("总频数必须大于 0".into());
    }
    Ok((out, r, c))
}

/// 卡方独立性检验。
pub fn chi_square(counts: &[Vec<f64>]) -> Result<ChiSquare, String> {
    let (obs, r, c) = validate(counts)?;
    let n: f64 = obs.iter().flatten().sum();
    let row_tot: Vec<f64> = obs.iter().map(|row| row.iter().sum()).collect();
    let col_tot: Vec<f64> = (0..c).map(|j| obs.iter().map(|row| row[j]).sum()).collect();

    let mut chi2 = 0.0;
    let mut yates = 0.0;
    let mut small_cells = 0usize;
    let mut min_expected = f64::INFINITY;
    for i in 0..r {
        for j in 0..c {
            let e = row_tot[i] * col_tot[j] / n;
            if !(e > 0.0) {
                return Err("存在期望频数为 0 的单元格（空行 / 空列），请去掉后再检验".into());
            }
            min_expected = min_expected.min(e);
            if e < 5.0 {
                small_cells += 1;
            }
            let d = obs[i][j] - e;
            chi2 += d * d / e;
            if r == 2 && c == 2 {
                // Yates 连续性校正：|O−E| 减 0.5
                let dy = d.abs() - 0.5;
                yates += dy.max(0.0).powi(2) / e;
            }
        }
    }
    let df = ((r - 1) * (c - 1)) as f64;
    let v = if n > 0.0 {
        (chi2 / (n * (r.min(c) - 1) as f64)).sqrt()
    } else {
        0.0
    };
    let or = odds_ratio_2x2(&obs, r, c);

    // 调整标准化残差
    let mut residual = vec![vec![0.0; c]; r];
    for i in 0..r {
        for j in 0..c {
            let e = row_tot[i] * col_tot[j] / n;
            if e > 0.0 {
                let denom = e * (1.0 - row_tot[i] / n) * (1.0 - col_tot[j] / n);
                residual[i][j] = if denom > 0.0 {
                    (obs[i][j] - e) / denom.sqrt()
                } else {
                    0.0
                };
            }
        }
    }
    // Bonferroni 校正到单元格数：(r×c) 个检验，α = 0.05
    let cells = (r * c) as f64;
    let alpha_cell = 0.05 / cells.max(1.0);
    let residual_crit = norm_ppf_upper(alpha_cell / 2.0);

    Ok(ChiSquare {
        chi2,
        df,
        p: chi2_sf(chi2, df),
        v,
        n,
        r,
        c,
        small_cells,
        min_expected,
        yates: if r == 2 && c == 2 { Some(yates) } else { None },
        odds_ratio: or.map(|x| x.0),
        or_ci: or.map(|x| x.1),
        residual,
        residual_crit,
    })
}

/// 标准正态上侧分位数（给残差的 Bonferroni 阈值用）。
fn norm_ppf_upper(p: f64) -> f64 {
    crate::core::stats::norm_ppf(1.0 - p.clamp(1e-12, 0.5))
}

/// 2×2 表的优势比与 Woolf 95% CI。
fn odds_ratio_2x2(obs: &[Vec<f64>], r: usize, c: usize) -> Option<(f64, (f64, f64))> {
    if r != 2 || c != 2 {
        return None;
    }
    let (a, b, cc, d) = (obs[0][0], obs[0][1], obs[1][0], obs[1][1]);
    if a <= 0.0 || b <= 0.0 || cc <= 0.0 || d <= 0.0 {
        // 零单元格使 OR 与 CI 无定义；这里不做加 0.5 的修补，宁可不给值。
        return None;
    }
    let or = (a * d) / (b * cc);
    let se = (1.0 / a + 1.0 / b + 1.0 / cc + 1.0 / d).sqrt();
    let l = or.ln();
    Some((or, ((l - 1.96 * se).exp(), (l + 1.96 * se).exp())))
}

/// Fisher 精确检验。
///
/// - 2×2：精确求和，双侧按「概率不高于观测表」的定义。
/// - R×C：枚举所有边际固定的表（Freeman–Halton）；枚举数超过
///   `MAX_TABLES` 时改用 Monte Carlo 近似，并在结果里标注。
pub fn fisher_exact(counts: &[Vec<f64>]) -> Result<Fisher, String> {
    let (obs, r, c) = validate(counts)?;
    let or = odds_ratio_2x2(&obs, r, c);

    let p_obs = table_prob(&obs);
    if !p_obs.is_finite() {
        return Err("无法计算该表的超几何概率".into());
    }

    // 枚举所有边际相同的表，累计概率 ≤ 观测表概率者
    let row_tot: Vec<u64> = obs
        .iter()
        .map(|row| row.iter().sum::<f64>() as u64)
        .collect();
    let col_tot: Vec<u64> = (0..c)
        .map(|j| obs.iter().map(|row| row[j]).sum::<f64>() as u64)
        .collect();
    // 去掉全零行/列，避免枚举爆炸
    let keep_rows: Vec<usize> = (0..r).filter(|&i| row_tot[i] > 0).collect();
    let keep_cols: Vec<usize> = (0..c).filter(|&j| col_tot[j] > 0).collect();
    if keep_rows.len() < 2 || keep_cols.len() < 2 {
        return Err("有效行 / 列少于 2，无法做精确检验".into());
    }
    let rr: Vec<u64> = keep_rows.iter().map(|&i| row_tot[i]).collect();
    let cc: Vec<u64> = keep_cols.iter().map(|&j| col_tot[j]).collect();
    let n: u64 = rr.iter().sum();

    // 估算枚举规模，超限走 Monte Carlo
    let est = estimate_tables(&rr, &cc);
    if est <= MAX_TABLES {
        let mut acc = 0.0;
        let mut tables = 0usize;
        enumerate(&rr, &cc, n, 0, &mut Vec::new(), &mut |m| {
            tables += 1;
            let p = table_prob_u64(m);
            if p <= p_obs * (1.0 + 1e-9) {
                acc += p;
            }
        });
        Ok(Fisher {
            p: acc.clamp(0.0, 1.0),
            exact: true,
            samples: 0,
            odds_ratio: or.map(|x| x.0),
            or_ci: or.map(|x| x.1),
            tables,
        })
    } else {
        // Monte Carlo：按超几何随机生成边际相同的表
        let samples = MC_SAMPLES;
        let mut acc = 0usize;
        let mut seed = 0x9E37_79B9_7F4A_7C15u64.wrapping_add(n * 7 + r as u64);
        for _ in 0..samples {
            let m = sample_table(&rr, &cc, n, &mut seed);
            if table_prob_u64(&m) <= p_obs * (1.0 + 1e-9) {
                acc += 1;
            }
        }
        Ok(Fisher {
            p: (acc as f64 + 1.0) / (samples as f64 + 1.0),
            exact: false,
            samples,
            odds_ratio: or.map(|x| x.0),
            or_ci: or.map(|x| x.1),
            tables: samples,
        })
    }
}

const MAX_TABLES: u64 = 200_000;
const MC_SAMPLES: usize = 50_000;

/// 估算边际固定下可能的表数（用于决定走精确还是 Monte Carlo）。
fn estimate_tables(rr: &[u64], cc: &[u64]) -> u64 {
    // 粗略上界：∏_i C(row_i + c - 1, c - 1)，取对数累加防溢出
    let mut logn = 0.0f64;
    for &ri in rr {
        let k = cc.len() as f64 - 1.0;
        logn += ln_gamma(ri as f64 + k + 1.0) - ln_gamma(k + 1.0) - ln_gamma(ri as f64 + 1.0);
    }
    if logn > (MAX_TABLES as f64).ln() * 2.0 {
        u64::MAX
    } else {
        logn.exp().min(u64::MAX as f64) as u64
    }
}

/// 表的超几何概率：∏i ri! · ∏j cj! / (n! · ∏ij mij!)
fn table_prob(obs: &[Vec<f64>]) -> f64 {
    let m: Vec<Vec<u64>> = obs
        .iter()
        .map(|row| row.iter().map(|v| *v as u64).collect())
        .collect();
    table_prob_u64(&m)
}

fn table_prob_u64(m: &[Vec<u64>]) -> f64 {
    let c = m[0].len();
    let row_tot: Vec<u64> = m.iter().map(|row| row.iter().sum()).collect();
    let col_tot: Vec<u64> = (0..c).map(|j| m.iter().map(|row| row[j]).sum()).collect();
    let n: u64 = row_tot.iter().sum();

    let mut logp = 0.0;
    for &ri in &row_tot {
        logp += ln_fact(ri);
    }
    for &cj in &col_tot {
        logp += ln_fact(cj);
    }
    logp -= ln_fact(n);
    for row in m {
        for &v in row {
            logp -= ln_fact(v);
        }
    }
    logp.exp()
}

fn ln_fact(n: u64) -> f64 {
    ln_gamma(n as f64 + 1.0)
}

/// 枚举所有边际固定的 R×C 表，对每个表调用 `f`。
///
/// 按行优先顺序逐格填充：第 i 行第 j 格放 v，受三重约束——
/// 1. `v ≤ min(该行剩余, 该列剩余)`；
/// 2. 本行剩余量要留得下：`v ≥ 该行剩余 − 后续列容量之和`；
/// 3. 行末列必须恰好放完该行剩余（否则整行不闭合）。
/// 这样枚举出来的每张表都自动满足行列边际，不会漏也不会重。
fn enumerate(
    rr: &[u64],
    cc: &[u64],
    _n: u64,
    i: usize,
    cur: &mut Vec<Vec<u64>>,
    f: &mut dyn FnMut(&[Vec<u64>]),
) {
    if i == rr.len() {
        f(cur);
        return;
    }
    let c = cc.len();
    let mut row = vec![0u64; c];
    fill_cell(rr, cc, i, 0, &mut row, cur, f);
}

fn fill_cell(
    rr: &[u64],
    cc: &[u64],
    i: usize,
    j: usize,
    row: &mut [u64],
    cur: &mut Vec<Vec<u64>>,
    f: &mut dyn FnMut(&[Vec<u64>]),
) {
    let c = cc.len();
    if j == c {
        cur.push(row.to_vec());
        enumerate(rr, cc, 0, i + 1, cur, f);
        cur.pop();
        return;
    }
    let row_used: u64 = row[..j].iter().sum();
    let row_left = rr[i].saturating_sub(row_used);
    let col_used: u64 = cur.iter().map(|r| r[j]).sum();
    let col_left = cc[j].saturating_sub(col_used);

    // 后续各列还能吃掉多少（按列剩余容量合计）
    let cap_after: u64 = (j + 1..c)
        .map(|k| {
            let cu: u64 = cur.iter().map(|r| r[k]).sum();
            cc[k].saturating_sub(cu)
        })
        .sum();

    let max_v = row_left.min(col_left);
    // 本行剩下的量必须能在后续列里放得下
    let min_v = row_left.saturating_sub(cap_after);
    if min_v > max_v {
        return;
    }
    for v in min_v..=max_v {
        row[j] = v;
        fill_cell(rr, cc, i, j + 1, row, cur, f);
    }
    row[j] = 0;
}

/// 按超几何随机生成一张边际固定的表（Monte Carlo 用）。
fn sample_table(rr: &[u64], cc: &[u64], n: u64, seed: &mut u64) -> Vec<Vec<u64>> {
    let r = rr.len();
    let c = cc.len();
    let mut m = vec![vec![0u64; c]; r];
    let mut col_left = cc.to_vec();
    for i in 0..r {
        let mut row_left = rr[i];
        for j in 0..c.saturating_sub(1) {
            if row_left == 0 || col_left[j] == 0 {
                continue;
            }
            // 超几何：从 n_left 中抽 row_left，其中成功类 col_left[j]
            let n_left: u64 = col_left.iter().sum();
            let k = hypergeom_sample(col_left[j], n_left - col_left[j], row_left, seed);
            m[i][j] = k;
            row_left -= k;
            col_left[j] -= k;
        }
        m[i][c - 1] = row_left.min(col_left[c - 1]);
        col_left[c - 1] -= m[i][c - 1];
    }
    let _ = n;
    m
}

/// 从超几何分布抽一个值：N 中有 K 个成功，抽 n 个，取成功个数。
fn hypergeom_sample(k_good: u64, k_bad: u64, n_draw: u64, seed: &mut u64) -> u64 {
    let lo = n_draw.saturating_sub(k_bad).min(k_good);
    let hi = n_draw.min(k_good);
    // 累积概率
    let total = k_good + k_bad;
    if total == 0 {
        return 0;
    }
    let mut acc = 0.0;
    let u = next_f64(seed);
    for x in lo..=hi {
        let logp = ln_fact(k_good) - ln_fact(x) - ln_fact(k_good - x) + ln_fact(k_bad)
            - ln_fact(n_draw - x)
            - ln_fact(k_bad + x - n_draw)
            - (ln_fact(total) - ln_fact(n_draw) - ln_fact(total - n_draw));
        acc += logp.exp();
        if acc >= u {
            return x;
        }
    }
    hi
}

fn next_f64(seed: &mut u64) -> f64 {
    // xorshift64*
    let mut x = *seed;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *seed = x;
    let r = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
    (r >> 11) as f64 / (1u64 << 53) as f64
}

/// 卡方分布上尾概率 P(X > x)，df 可为非整数。
/// 用正则化不完全伽马函数：Q(df/2, x/2)。
pub fn chi2_sf(x: f64, df: f64) -> f64 {
    if !(x > 0.0) {
        return 1.0;
    }
    if !(df > 0.0) {
        return 0.0;
    }
    gamma_q(df / 2.0, x / 2.0)
}

/// 正则化上不完全伽马 Q(a, x)。
fn gamma_q(a: f64, x: f64) -> f64 {
    if x < 0.0 || a <= 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return 1.0;
    }
    if x < a + 1.0 {
        // 用级数算 P，再 1 − P
        1.0 - gamma_p_series(a, x)
    } else {
        gamma_q_contfrac(a, x)
    }
}

fn gamma_p_series(a: f64, x: f64) -> f64 {
    let mut ap = a;
    let mut sum = 1.0 / a;
    let mut del = sum;
    for _ in 0..300 {
        ap += 1.0;
        del *= x / ap;
        sum += del;
        if del.abs() < sum.abs() * 1e-15 {
            break;
        }
    }
    sum * (-x + a * x.ln() - ln_gamma(a)).exp()
}

fn gamma_q_contfrac(a: f64, x: f64) -> f64 {
    const BIG: f64 = 1e300;
    const EPS: f64 = 1e-15;
    let mut b = x + 1.0 - a;
    let mut c = 1.0 / 1e-300;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..=300 {
        let an = -(i as f64) * (i as f64 - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < 1e-300 {
            d = 1e-300;
        }
        c = b + an / c;
        if c.abs() < 1e-300 {
            c = 1e-300;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < EPS {
            break;
        }
        let _ = BIG;
    }
    (-x + a * x.ln() - ln_gamma(a)).exp() * h
}

/// 交叉表的行列汇总，供结果表直接展示。
pub fn marginals(counts: &[Vec<f64>]) -> (Vec<f64>, Vec<f64>, f64) {
    let r = counts.len();
    let c = counts[0].len();
    let row_tot: Vec<f64> = counts.iter().map(|row| row.iter().sum()).collect();
    let col_tot: Vec<f64> = (0..c)
        .map(|j| counts.iter().map(|row| row[j]).sum())
        .collect();
    let n = row_tot.iter().sum();
    let _ = r;
    (row_tot, col_tot, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 经典 2×2：阿司匹林与心脏病（Prism 教程常用例）
    fn aspirin() -> Vec<Vec<f64>> {
        vec![
            vec![10.0, 20.0], // 心脏病：服药 / 对照
            vec![30.0, 40.0], // 正常
        ]
    }

    #[test]
    fn 卡方与Fisher在2x2上一致() {
        let d = vec![vec![20.0, 30.0], vec![30.0, 20.0]];
        let cs = chi_square(&d).expect("chi2");
        let fs = fisher_exact(&d).expect("fisher");
        assert!(cs.chi2 > 0.0);
        assert_eq!(cs.df, 1.0);
        // 两者应在同一量级（2×2 上 Fisher 略保守）
        assert!(
            (cs.p - fs.p).abs() < 0.15,
            "chi2 p={:.4} fisher p={:.4}",
            cs.p,
            fs.p
        );
    }

    #[test]
    fn 卡方对独立表给出大p值() {
        // 完美独立：比例一致
        let d = vec![vec![10.0, 20.0], vec![20.0, 40.0]];
        let cs = chi_square(&d).expect("chi2");
        assert!(cs.chi2 < 1e-9, "chi2 = {}", cs.chi2);
        assert!(cs.p > 0.99);
    }

    #[test]
    fn 卡方对强关联表给出小p值() {
        let d = vec![vec![50.0, 5.0], vec![5.0, 50.0]];
        let cs = chi_square(&d).expect("chi2");
        assert!(cs.p < 1e-6, "p = {}", cs.p);
        assert!(cs.v > 0.4);
        let or = cs.odds_ratio.expect("or");
        assert!(or > 10.0, "or = {or}");
    }

    #[test]
    fn 期望频数不足时给出提示() {
        let d = vec![vec![1.0, 2.0], vec![2.0, 1.0]];
        let cs = chi_square(&d).expect("chi2");
        assert!(cs.small_cells > 0);
        assert!(cs.min_expected < 5.0);
    }

    #[test]
    fn 空行导致期望为0时报错() {
        let d = vec![vec![0.0, 0.0], vec![5.0, 5.0]];
        assert!(chi_square(&d).is_err());
    }

    #[test]
    fn 负计数与非矩形被拒绝() {
        assert!(chi_square(&[vec![-1.0, 1.0], vec![1.0, 1.0]]).is_err());
        assert!(chi_square(&[vec![1.0, 1.0], vec![1.0]]).is_err());
    }

    #[test]
    fn 优势比与CI正确() {
        // OR = 25*75/(25*75) = 1 → CI 应含 1
        let d = vec![vec![25.0, 25.0], vec![25.0, 25.0]];
        let cs = chi_square(&d).expect("chi2");
        let or = cs.odds_ratio.expect("or");
        assert!((or - 1.0).abs() < 1e-9);
        let (lo, hi) = cs.or_ci.expect("ci");
        assert!(lo < 1.0 && hi > 1.0);
    }

    #[test]
    fn 零单元格不给OR() {
        let d = vec![vec![0.0, 10.0], vec![10.0, 10.0]];
        let cs = chi_square(&d).expect("chi2");
        assert!(cs.odds_ratio.is_none());
    }

    #[test]
    fn Fisher在2x2给出精确p值() {
        let fs = fisher_exact(&aspirin()).expect("fisher");
        assert!(fs.exact, "2×2 应走精确求和");
        assert!(fs.tables > 0);
        // 行边际 30/70、列边际 40/60，观测 a=10 接近期望 12，双侧 p 应较大
        assert!((0.3..=0.7).contains(&fs.p), "p = {}", fs.p);
        // 与卡方交叉验证：两者应同号同量级
        let cs = chi_square(&aspirin()).expect("chi2");
        assert!(
            (cs.p - fs.p).abs() < 0.2,
            "chi2 p={:.4} fisher p={:.4}",
            cs.p,
            fs.p
        );
    }

    #[test]
    fn Fisher在3x3也能跑() {
        let d = vec![
            vec![10.0, 20.0, 30.0],
            vec![20.0, 30.0, 40.0],
            vec![30.0, 40.0, 50.0],
        ];
        let fs = fisher_exact(&d).expect("fisher");
        assert!(fs.p > 0.0 && fs.p <= 1.0);
    }

    #[test]
    fn 残差平方和与卡方一致() {
        // 未调整的 Pearson 残差平方和恒等于 χ²。这里用调整残差做上界校验：
        // 调整残差的分母 ≤ √E，所以 |调整残差| ≥ |Pearson 残差|——
        // 但两者方向必须完全一致（同号）。
        let d = vec![vec![38.0, 12.0], vec![22.0, 26.0], vec![8.0, 34.0]];
        let cs = chi_square(&d).expect("chi2");
        let n: f64 = d.iter().flatten().sum();
        let row_tot: Vec<f64> = d.iter().map(|r| r.iter().sum()).collect();
        let col_tot: Vec<f64> = (0..2).map(|j| d.iter().map(|r| r[j]).sum()).collect();
        let mut pearson_ss = 0.0;
        for i in 0..3 {
            for j in 0..2 {
                let e = row_tot[i] * col_tot[j] / n;
                let pr = (d[i][j] - e) / e.sqrt();
                pearson_ss += pr * pr;
                // 调整残差与 Pearson 残差同号
                assert!(
                    (cs.residual[i][j] > 0.0) == (pr > 0.0),
                    "格 ({i},{j}) 方向不一致"
                );
            }
        }
        assert!(
            (pearson_ss - cs.chi2).abs() < 1e-9,
            "Σr² = {pearson_ss}, χ² = {}",
            cs.chi2
        );
    }

    #[test]
    fn 残差能指出强关联的格子() {
        // 左上与右下远高于期望，右上与左下远低于期望
        let d = vec![vec![50.0, 5.0], vec![5.0, 50.0]];
        let cs = chi_square(&d).expect("chi2");
        assert!(cs.residual[0][0] > 2.0, "左上应为强正残差");
        assert!(cs.residual[1][1] > 2.0, "右下应为强正残差");
        assert!(cs.residual[0][1] < -2.0, "右上应为强负残差");
        assert!(cs.residual[1][0] < -2.0, "左下应为强负残差");
        // 阈值随单元格数做 Bonferroni：4 格时应大于 2
        assert!(cs.residual_crit > 2.0 && cs.residual_crit < 3.0);
    }

    #[test]
    fn 独立表残差接近零() {
        let d = vec![vec![10.0, 20.0], vec![20.0, 40.0]];
        let cs = chi_square(&d).expect("chi2");
        for row in &cs.residual {
            for r in row {
                assert!(r.abs() < 1e-9, "独立表残差应为 0，实际 {r}");
            }
        }
    }

    #[test]
    fn 卡方分布尾概率数值正确() {
        // df=1, x=3.84 → p ≈ 0.05
        let p = chi2_sf(3.8414588, 1.0);
        assert!((p - 0.05).abs() < 1e-4, "p = {p}");
        // df=2, x=5.99 → p ≈ 0.05
        let p2 = chi2_sf(5.9914645, 2.0);
        assert!((p2 - 0.05).abs() < 1e-4, "p = {p2}");
        // df=1, x=0 → p = 1
        assert!((chi2_sf(0.0, 1.0) - 1.0).abs() < 1e-12);
    }
}
