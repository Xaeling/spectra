//! 分析体系：参数（AnalysisParams）+ 分析种类（AnalysisKind）+ 表类型路由。
//!
//! 三层结构：**表类型路由 → 分析装配 → 数值核心**。
//! - 路由层：`AnalysisKind::for_table()` 给出某表型可用的分析白名单
//! - 装配层：把表格投影成统计输入，调用 `stats::*` 得到结果
//! - 数值层：`stats` 模块（纯 Rust 分布库）
//!
//! # AnalysisParams 的契约
//! **零值必须等同于业务默认值**。所有 UI 控件最终收敛到这一个结构体，
//! 结果持久化时保存计算当时的快照，保证旧结果可一键复算。

use crate::model::TableType;

// ══════════════════════════════════════════════════════════════════════
// 一、参数体系
// ══════════════════════════════════════════════════════════════════════

/// 单因素 ANOVA 的多重比较方法。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum MultiMethod {
    Tukey,
    Holm,
    Bonferroni,
    Sidak,
    GamesHowell,
    Dunnett,
    Dunn,
}

impl MultiMethod {
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            MultiMethod::Tukey => "Tukey",
            MultiMethod::Holm => "Holm",
            MultiMethod::Bonferroni => "Bonferroni",
            MultiMethod::Sidak => "Šídák",
            MultiMethod::GamesHowell => "Games-Howell",
            MultiMethod::Dunnett => "Dunnett",
            MultiMethod::Dunn => "Dunn",
        }
    }
    #[allow(dead_code)]
    pub fn all() -> &'static [MultiMethod] {
        &[
            MultiMethod::Tukey,
            MultiMethod::Holm,
            MultiMethod::Bonferroni,
            MultiMethod::Sidak,
            MultiMethod::GamesHowell,
            MultiMethod::Dunnett,
            MultiMethod::Dunn,
        ]
    }
}

/// 非线性回归模型（内置 11 个）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ModelId {
    ExpDecay1,
    ExpDecay2,
    ExpAssociation1,
    ExpGrowth,
    MichaelisMenten,
    OneSiteBinding,
    DoseResponseLog,
    Sigmoid4PL,
    Gaussian,
    PowerLaw,
    Sine,
}

impl ModelId {
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            ModelId::ExpDecay1 => "指数衰减（单相）",
            ModelId::ExpDecay2 => "指数衰减（双相）",
            ModelId::ExpAssociation1 => "指数结合",
            ModelId::ExpGrowth => "指数增长",
            ModelId::MichaelisMenten => "Michaelis-Menten",
            ModelId::OneSiteBinding => "单位点结合",
            ModelId::DoseResponseLog => "剂量反应（logX）",
            ModelId::Sigmoid4PL => "四参数 S 型",
            ModelId::Gaussian => "高斯",
            ModelId::PowerLaw => "幂律",
            ModelId::Sine => "正弦",
        }
    }
    /// 参数个数（拟合要求观测数严格大于它）。
    pub fn n_params(self) -> usize {
        match self {
            // Y0, Plateau, K
            ModelId::ExpDecay1 | ModelId::ExpAssociation1 => 3,
            // Y0, K
            ModelId::ExpGrowth => 2,
            // SpanFast, KFast, SpanSlow, KSlow, Plateau
            ModelId::ExpDecay2 => 5,
            ModelId::MichaelisMenten | ModelId::OneSiteBinding => 2,
            ModelId::DoseResponseLog | ModelId::Sigmoid4PL => 4,
            ModelId::Gaussian => 3,
            ModelId::PowerLaw => 2,
            ModelId::Sine => 4,
        }
    }
    #[allow(dead_code)]
    pub fn all() -> &'static [ModelId] {
        &[
            ModelId::ExpDecay1,
            ModelId::ExpDecay2,
            ModelId::ExpAssociation1,
            ModelId::ExpGrowth,
            ModelId::MichaelisMenten,
            ModelId::OneSiteBinding,
            ModelId::DoseResponseLog,
            ModelId::Sigmoid4PL,
            ModelId::Gaussian,
            ModelId::PowerLaw,
            ModelId::Sine,
        ]
    }
}


// ══════════════════════════════════════════════════════════════════════
// 第八章 · 非线性回归：模型库（11 个模型 × 5 个方法）与 Levenberg-Marquardt
// 方程字符串、参数名、求值器、初值必须由同一个 ModelId 分支覆盖
// ══════════════════════════════════════════════════════════════════════

impl ModelId {
    /// 方程字符串，用于结果展示
    pub fn equation(self) -> &'static str {
        match self {
            ModelId::ExpDecay1 => "Y=(Y0-Plateau)*exp(-K*X)+Plateau",
            ModelId::ExpDecay2 => {
                "Y=SpanFast*exp(-KFast*X)+SpanSlow*exp(-KSlow*X)+Plateau"
            }
            ModelId::ExpAssociation1 => "Y=Y0+(Plateau-Y0)*(1-exp(-K*X))",
            ModelId::ExpGrowth => "Y=Y0*exp(K*X)",
            ModelId::MichaelisMenten => "Y=Vmax*X/(Km+X)",
            ModelId::OneSiteBinding => "Y=Bmax*X/(Kd+X)",
            ModelId::DoseResponseLog => {
                "Y=Bottom+(Top-Bottom)/(1+10^((LogEC50-X)*HillSlope))"
            }
            ModelId::Sigmoid4PL => "Y=Bottom+(Top-Bottom)/(1+(X/EC50)^(-HillSlope))",
            ModelId::Gaussian => "Y=Amplitude*exp(-0.5*((X-Mean)/SD)^2)",
            ModelId::PowerLaw => "Y=A*X^B",
            ModelId::Sine => "Y=Amplitude*sin(2*PI*X/Period+Phase)+Baseline",
        }
    }

    /// 参数名，用于结果表表头；顺序与 eval() 的位置参数一一对应
    pub fn param_names(self) -> &'static [&'static str] {
        match self {
            ModelId::ExpDecay1 => &["Y0", "Plateau", "K"],
            ModelId::ExpDecay2 => &["SpanFast", "KFast", "SpanSlow", "KSlow", "Plateau"],
            ModelId::ExpAssociation1 => &["Y0", "Plateau", "K"],
            ModelId::ExpGrowth => &["Y0", "K"],
            ModelId::MichaelisMenten => &["Vmax", "Km"],
            ModelId::OneSiteBinding => &["Bmax", "Kd"],
            ModelId::DoseResponseLog => &["Bottom", "Top", "LogEC50", "HillSlope"],
            ModelId::Sigmoid4PL => &["Bottom", "Top", "EC50", "HillSlope"],
            ModelId::Gaussian => &["Amplitude", "Mean", "SD"],
            ModelId::PowerLaw => &["A", "B"],
            ModelId::Sine => &["Amplitude", "Period", "Phase", "Baseline"],
        }
    }

    /// 求值器。参数顺序与 param_names() 一致。
    pub fn eval(self, x: f64, p: &[f64]) -> f64 {
        let g = |i: usize| p.get(i).copied().unwrap_or(f64::NAN);
        match self {
            ModelId::ExpDecay1 => (g(0) - g(1)) * (-g(2) * x).exp() + g(1),
            ModelId::ExpDecay2 => {
                g(0) * (-g(1) * x).exp() + g(2) * (-g(3) * x).exp() + g(4)
            }
            ModelId::ExpAssociation1 => g(0) + (g(1) - g(0)) * (1.0 - (-g(2) * x).exp()),
            ModelId::ExpGrowth => g(0) * (g(1) * x).exp(),
            ModelId::MichaelisMenten => g(0) * x / (g(1) + x),
            ModelId::OneSiteBinding => g(0) * x / (g(1) + x),
            ModelId::DoseResponseLog => {
                g(0) + (g(1) - g(0)) / (1.0 + 10f64.powf((g(2) - x) * g(3)))
            }
            ModelId::Sigmoid4PL => {
                let ec = g(2).abs().max(1e-6);
                g(0) + (g(1) - g(0)) / (1.0 + (x / ec).powf(-g(3)))
            }
            ModelId::Gaussian => {
                let sd = g(2).abs().max(1e-12);
                g(0) * (-0.5 * ((x - g(1)) / sd).powi(2)).exp()
            }
            ModelId::PowerLaw => g(0) * x.powf(g(1)),
            ModelId::Sine => {
                let per = if g(1).abs() < 1e-12 { 1.0 } else { g(1) };
                g(0) * (2.0 * std::f64::consts::PI * x / per + g(2)).sin() + g(3)
            }
        }
    }

    /// 初值规则。数据驱动，失败时由拟合器回退为全 1。
    pub fn initial(self, xs: &[f64], ys: &[f64]) -> Vec<f64> {
        let n = xs.len().min(ys.len());
        let v: Vec<(f64, f64)> = (0..n)
            .map(|i| (xs[i], ys[i]))
            .filter(|(a, b)| a.is_finite() && b.is_finite())
            .collect();
        if v.is_empty() {
            return vec![1.0; self.n_params()];
        }
        let xmin = v.iter().map(|(a, _)| *a).fold(f64::INFINITY, f64::min);
        let xmax = v.iter().map(|(a, _)| *a).fold(f64::NEG_INFINITY, f64::max);
        let span = (xmax - xmin).abs().max(1e-9);
        let xmid = 0.5 * (xmin + xmax);
        let ymin = v.iter().map(|(_, b)| *b).fold(f64::INFINITY, f64::min);
        let ymax = v.iter().map(|(_, b)| *b).fold(f64::NEG_INFINITY, f64::max);
        let first = v.first().map(|(_, b)| *b).unwrap_or(1.0);
        let last = v.last().map(|(_, b)| *b).unwrap_or(1.0);
        let ymean = v.iter().map(|(_, b)| *b).sum::<f64>() / v.len() as f64;
        match self {
            // Y0=首点、Plateau=末点、K=1/span
            ModelId::ExpDecay1 | ModelId::ExpAssociation1 => {
                vec![first, last, 1.0 / span]
            }
            // 快慢跨度各半、双 K
            ModelId::ExpDecay2 => vec![
                (ymax - ymin) * 0.5,
                1.0 / span * 4.0,
                (ymax - ymin) * 0.5,
                1.0 / span,
                ymin,
            ],
            ModelId::ExpGrowth => vec![first.max(1e-6), 1.0 / span],
            // 极值与 X 中点
            ModelId::MichaelisMenten | ModelId::OneSiteBinding => vec![ymax.abs().max(1e-6), xmid],
            // 极值、X 中点、斜率 1
            ModelId::DoseResponseLog => vec![ymin, ymax, xmid, 1.0],
            // 极值、X 中点夹紧 1e-6
            ModelId::Sigmoid4PL => vec![ymin, ymax, xmid.max(1e-6), 1.0],
            // 振幅、X 中点、20% 跨度
            ModelId::Gaussian => vec![(ymax - ymin).abs().max(1e-6), xmid, span * 0.2],
            ModelId::PowerLaw => vec![1.0, 1.0],
            // 半振幅、跨度、0、Y 均值
            ModelId::Sine => vec![(ymax - ymin) * 0.5, span, 0.0, ymean],
        }
    }
}

/// 非线性拟合结果
#[derive(Clone, Debug)]
pub struct FitResult {
    pub model: ModelId,
    pub params: Vec<f64>,
    pub se: Vec<f64>,
    pub ci_lo: Vec<f64>,
    pub ci_hi: Vec<f64>,
    pub t: Vec<f64>,
    pub sse: f64,
    pub sst: f64,
    pub r2: f64,
    pub adj_r2: f64,
    pub aicc: f64,
    pub df: f64,
    pub n: usize,
    pub converged: bool,
}

/// AICc = n·ln(SSE/n) + 2K + 2K(K+1)/(n−K−1)，K 含方差参数故为 k+1
pub fn aicc_from_sse(sse: f64, n: usize, k: usize) -> f64 {
    let n = n as f64;
    let k = k as f64;
    if !(sse > 0.0) || n <= 0.0 {
        return f64::NAN;
    }
    let aic = n * (sse / n).ln() + 2.0 * k;
    let den = n - k - 1.0;
    if den <= 0.0 {
        return aic;
    }
    aic + 2.0 * k * (k + 1.0) / den
}

/// 高斯-约当求逆。奇异时返回 None。
fn inv_matrix(m: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let k = m.len();
    if k == 0 {
        return None;
    }
    let mut a: Vec<Vec<f64>> = m.to_vec();
    let mut inv = vec![vec![0.0f64; k]; k];
    for i in 0..k {
        inv[i][i] = 1.0;
    }
    for col in 0..k {
        // 选主元
        let mut piv = col;
        for r in col + 1..k {
            if a[r][col].abs() > a[piv][col].abs() {
                piv = r;
            }
        }
        if a[piv][col].abs() < 1e-300 {
            return None;
        }
        a.swap(col, piv);
        inv.swap(col, piv);
        let d = a[col][col];
        for j in 0..k {
            a[col][j] /= d;
            inv[col][j] /= d;
        }
        for r in 0..k {
            if r == col {
                continue;
            }
            let f = a[r][col];
            if f == 0.0 {
                continue;
            }
            for j in 0..k {
                a[r][j] -= f * a[col][j];
                inv[r][j] -= f * inv[col][j];
            }
        }
    }
    Some(inv)
}

/// 解线性方程组 A·x=b（部分选主元高斯消元）。奇异返回 None。
fn solve_linear(a: &[Vec<f64>], b: &[f64]) -> Option<Vec<f64>> {
    let k = b.len();
    if k == 0 || a.len() != k {
        return None;
    }
    let mut m: Vec<Vec<f64>> = a.iter().map(|r| r.to_vec()).collect();
    let mut rhs = b.to_vec();
    for col in 0..k {
        let mut piv = col;
        for r in col + 1..k {
            if m[r][col].abs() > m[piv][col].abs() {
                piv = r;
            }
        }
        if m[piv][col].abs() < 1e-300 {
            return None;
        }
        m.swap(col, piv);
        rhs.swap(col, piv);
        let d = m[col][col];
        for j in col..k {
            m[col][j] /= d;
        }
        rhs[col] /= d;
        for r in 0..k {
            if r == col {
                continue;
            }
            let f = m[r][col];
            if f == 0.0 {
                continue;
            }
            for j in col..k {
                m[r][j] -= f * m[col][j];
            }
            rhs[r] -= f * rhs[col];
        }
    }
    Some(rhs)
}

fn sse_of(model: ModelId, xs: &[f64], ys: &[f64], p: &[f64]) -> f64 {
    xs.iter()
        .zip(ys.iter())
        .map(|(x, y)| (y - model.eval(*x, p)).powi(2))
        .sum::<f64>()
}

/// Levenberg-Marquardt：Jacobian 用中心差分，收敛判据是相对 SSE 改善。
/// 观测数必须严格大于参数数 k；最多 500 轮；阻尼上限 1e12、下限 1e-12。
pub fn fit_nonlinear(xs: &[f64], ys: &[f64], model: ModelId) -> Option<FitResult> {
    let m = xs.len().min(ys.len());
    let v: Vec<(f64, f64)> = (0..m)
        .map(|i| (xs[i], ys[i]))
        .filter(|(a, b)| a.is_finite() && b.is_finite())
        .collect();
    let n = v.len();
    let k = model.n_params();
    if n <= k {
        return None;
    }
    let px: Vec<f64> = v.iter().map(|(a, _)| *a).collect();
    let py: Vec<f64> = v.iter().map(|(_, b)| *b).collect();

    let mut p = model.initial(&px, &py);
    if p.len() != k {
        p = vec![1.0; k];
    }
    let mut sse = sse_of(model, &px, &py, &p);
    if !sse.is_finite() {
        p = vec![1.0; k];
        sse = sse_of(model, &px, &py, &p);
        if !sse.is_finite() {
            return None;
        }
    }

    let ymean = py.iter().sum::<f64>() / n as f64;
    let sst = py.iter().map(|y| (y - ymean).powi(2)).sum::<f64>();

    let mut lam = 1e-3f64;
    let mut converged = false;
    for _ in 0..500 {
        // Jacobian：中心差分
        let mut j = vec![vec![0.0f64; k]; n];
        for i in 0..n {
            let r0 = model.eval(px[i], &p);
            for c in 0..k {
                let h = (p[c].abs().max(1e-6)) * 1e-6;
                let mut pp = p.clone();
                pp[c] += h;
                let mut pm = p.clone();
                pm[c] -= h;
                let rp = model.eval(px[i], &pp);
                let rm = model.eval(px[i], &pm);
                if rp.is_finite() && rm.is_finite() {
                    j[i][c] = (rp - rm) / (2.0 * h);
                } else if rp.is_finite() && r0.is_finite() {
                    j[i][c] = (rp - r0) / h;
                } else if rm.is_finite() && r0.is_finite() {
                    j[i][c] = (r0 - rm) / h;
                }
            }
        }
        // JᵀJ 与 Jᵀr
        let mut jtj = vec![vec![0.0f64; k]; k];
        for a in 0..k {
            for b in 0..k {
                let mut s = 0.0;
                for i in 0..n {
                    s += j[i][a] * j[i][b];
                }
                jtj[a][b] = s;
            }
        }
        let mut jtr = vec![0.0f64; k];
        for a in 0..k {
            let mut s = 0.0;
            for i in 0..n {
                let r = py[i] - model.eval(px[i], &p);
                s += j[i][a] * r;
            }
            jtr[a] = s;
        }
        // 阻尼：对角元乘 (1+λ) 并夹紧至 1e-14
        let mut a = jtj.clone();
        for c in 0..k {
            a[c][c] = (a[c][c] * (1.0 + lam)).max(1e-14);
        }
        let delta = match solve_linear(&a, &jtr) {
            Some(d) => d,
            None => {
                lam *= 10.0;
                if lam > 1e12 {
                    break;
                }
                continue;
            }
        };
        let mut pn = p.clone();
        for c in 0..k {
            pn[c] += delta[c];
        }
        let sse_new = sse_of(model, &px, &py, &pn);
        if sse_new < sse && sse_new.is_finite() {
            let rel = (sse - sse_new) / sse.max(1e-30);
            sse = sse_new;
            p = pn;
            lam = (lam / 10.0).max(1e-12);
            if rel < 1e-10 {
                converged = true;
                break;
            }
        } else {
            lam = (lam * 10.0).min(1e12);
            if lam >= 1e12 {
                break;
            }
        }
    }

    // 统计量
    let df = (n - k) as f64;
    let s2 = if df > 0.0 { sse / df } else { f64::NAN };
    // 协方差 = inv(JᵀJ)·s²
    let mut jtj = vec![vec![0.0f64; k]; k];
    for i in 0..n {
        let mut row = vec![0.0f64; k];
        for c in 0..k {
            let h = (p[c].abs().max(1e-6)) * 1e-6;
            let mut pp = p.clone();
            pp[c] += h;
            let mut pm = p.clone();
            pm[c] -= h;
            let rp = model.eval(px[i], &pp);
            let rm = model.eval(px[i], &pm);
            row[c] = if rp.is_finite() && rm.is_finite() {
                (rp - rm) / (2.0 * h)
            } else {
                0.0
            };
        }
        for a in 0..k {
            for b in 0..k {
                jtj[a][b] += row[a] * row[b];
            }
        }
    }
    let cov = inv_matrix(&jtj);
    let mut se = vec![f64::NAN; k];
    let mut ci_lo = vec![f64::NAN; k];
    let mut ci_hi = vec![f64::NAN; k];
    let mut tv = vec![f64::NAN; k];
    if let Some(c) = cov {
        if df > 0.0 && s2.is_finite() && s2 > 0.0 {
            let tc = crate::stats::t_crit_two(0.95, df);
            for i in 0..k {
                let v = c[i][i] * s2;
                if v > 0.0 {
                    let s = v.sqrt();
                    se[i] = s;
                    ci_lo[i] = p[i] - tc * s;
                    ci_hi[i] = p[i] + tc * s;
                    // 参数 t 仅在 SE>0 时计算
                    tv[i] = if s > 0.0 { p[i] / s } else { f64::NAN };
                }
            }
        }
    }
    let r2 = if sst > 0.0 { 1.0 - sse / sst } else { f64::NAN };
    let adj_r2 = if r2.is_finite() && (n as f64) > k as f64 {
        1.0 - (1.0 - r2) * (n as f64 - 1.0) / (n as f64 - k as f64)
    } else {
        f64::NAN
    };
    Some(FitResult {
        model,
        params: p,
        se,
        ci_lo,
        ci_hi,
        t: tv,
        sse,
        sst,
        r2,
        adj_r2,
        aicc: aicc_from_sse(sse, n, k + 1),
        df,
        n,
        converged,
    })
}

/// 分析配置的**单一事实源**。
///
/// 零值 = 业务默认值，`Default` 实现即为规范默认值。
#[derive(Clone, Debug, PartialEq)]
pub struct AnalysisParams {
    /// 置信水平
    pub conf: f64,
    /// 显著性水平 α
    pub alpha: f64,
    /// 独立 t 检验是否用 Welch 校正
    pub welch: bool,
    /// 单样本 t 检验的原假设差值
    pub mu0: f64,
    /// 单因素 ANOVA 的多重比较方法
    pub method: MultiMethod,
    /// 列联表 2×2 是否做 Yates 连续性校正
    pub yates: bool,
    /// XY 系列分析使用的数据集索引
    pub dataset: usize,
    /// 两组比较的第一组索引
    pub group_a: usize,
    /// 两组比较的第二组索引
    pub group_b: usize,
    /// 相关分析是否用秩相关（false = Pearson）
    pub spearman: bool,
    /// 线性回归是否过原点
    pub through_origin: bool,
    /// 非线性回归模型
    pub model: ModelId,
    /// 是否触发两组斜率比较
    pub compare_slope: bool,
    /// Deming 回归的误差方差比 λ
    pub deming_ratio: f64,
    /// K 均值聚类数
    pub clusters: usize,
    /// 绘制/生成函数时的采样点数
    pub n_points: usize,
    /// 配对或重复测量设计
    pub paired: bool,
}

impl Default for AnalysisParams {
    fn default() -> Self {
        Self {
            conf: 0.95,
            alpha: 0.05,
            welch: true,
            mu0: 0.0,
            method: MultiMethod::Tukey,
            yates: true,
            dataset: 0,
            group_a: 0,
            group_b: 1,
            spearman: false,
            through_origin: false,
            model: ModelId::ExpDecay1,
            compare_slope: true,
            deming_ratio: 1.0,
            clusters: 3,
            n_points: 101,
            paired: false,
        }
    }
}

impl AnalysisParams {
    /// 置信水平的百分比形式（95.0）。
    #[allow(dead_code)]
    pub fn conf_pct(&self) -> f64 {
        self.conf * 100.0
    }
    /// 序列化为 `key=value` 行（存盘用）。
    #[allow(dead_code)]
    pub fn save_lines(&self) -> Vec<String> {
        vec![
            format!("conf={}", self.conf),
            format!("alpha={}", self.alpha),
            format!("welch={}", if self.welch { 1 } else { 0 }),
            format!("mu0={}", self.mu0),
            format!("method={}", self.method as i32),
            format!("yates={}", if self.yates { 1 } else { 0 }),
            format!("dataset={}", self.dataset),
            format!("group_a={}", self.group_a),
            format!("group_b={}", self.group_b),
            format!("spearman={}", if self.spearman { 1 } else { 0 }),
            format!("through_origin={}", if self.through_origin { 1 } else { 0 }),
            format!("model={}", self.model as i32),
            format!("compare_slope={}", if self.compare_slope { 1 } else { 0 }),
            format!("deming_ratio={}", self.deming_ratio),
            format!("clusters={}", self.clusters),
            format!("n_points={}", self.n_points),
            format!("paired={}", if self.paired { 1 } else { 0 }),
        ]
    }
    /// 从 `key=value` 行恢复（缺项用默认值）。
    #[allow(dead_code)]
    pub fn load_lines(lines: &[String]) -> Self {
        let mut p = Self::default();
        for l in lines {
            let Some((k, v)) = l.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "conf" => p.conf = v.parse().unwrap_or(p.conf),
                "alpha" => p.alpha = v.parse().unwrap_or(p.alpha),
                "welch" => p.welch = v == "1",
                "mu0" => p.mu0 = v.parse().unwrap_or(p.mu0),
                "method" => {
                    let i: i32 = v.parse().unwrap_or(0);
                    p.method = match i {
                        1 => MultiMethod::Holm,
                        2 => MultiMethod::Bonferroni,
                        3 => MultiMethod::Sidak,
                        4 => MultiMethod::GamesHowell,
                        5 => MultiMethod::Dunnett,
                        6 => MultiMethod::Dunn,
                        _ => MultiMethod::Tukey,
                    };
                }
                "yates" => p.yates = v == "1",
                "dataset" => p.dataset = v.parse().unwrap_or(p.dataset),
                "group_a" => p.group_a = v.parse().unwrap_or(p.group_a),
                "group_b" => p.group_b = v.parse().unwrap_or(p.group_b),
                "spearman" => p.spearman = v == "1",
                "through_origin" => p.through_origin = v == "1",
                "model" => {
                    let i: i32 = v.parse().unwrap_or(0);
                    p.model = ModelId::all()
                        .get(i as usize)
                        .copied()
                        .unwrap_or(ModelId::ExpDecay1);
                }
                "compare_slope" => p.compare_slope = v == "1",
                "deming_ratio" => p.deming_ratio = v.parse().unwrap_or(p.deming_ratio),
                "clusters" => p.clusters = v.parse().unwrap_or(p.clusters),
                "n_points" => p.n_points = v.parse().unwrap_or(p.n_points),
                "paired" => p.paired = v == "1",
                _ => {}
            }
        }
        p
    }
}

// ══════════════════════════════════════════════════════════════════════
// 二、分析种类
// ══════════════════════════════════════════════════════════════════════

/// 智能统计制图顾问的菜单 id。
///
/// **不能写成 `AnalysisKind::StatsAdvisor as usize`**：它是枚举的第一个变体，
/// 判别值恰好是 0，而菜单 id 0 是「描述统计」。菜单 id 来自
/// `TableType::analyses()`，与枚举判别值是两套独立编号，相等纯属巧合——
/// 直接比较的结果是「点描述统计却被顾问分支截获」。
/// 顾问改用这个不与任何菜单 id 冲突的专用值。
pub const ADVISOR_ID: usize = 100;

/// 分析入口。每种分析只在其 `for_table()` 允许的表型上出现。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnalysisKind {
    /// 智能统计制图推荐（决策层入口）。
    ///
    /// 只对能稳定取出「每组原始值」的三种表给入口（Column / Grouped / XY）。
    /// 其他表型点了必然跑不出来，所以不给入口 —— 见 `for_table()` 的插入规则。
    StatsAdvisor,

    // ── Column 表（列表）──
    ColumnDescribe,
    ColumnOneWayAnova,
    ColumnTTest,
    ColumnWelchTTest,
    ColumnOneSampleT,
    ColumnPairedT,
    ColumnMannWhitney,
    ColumnWilcoxon,
    ColumnKruskalWallis,
    ColumnFriedman,
    ColumnNormality,
    ColumnOutlier,
    ColumnBartlett,
    ColumnLevene,
    ColumnFTestVariance,
    ColumnMultiCompare,
    ColumnRatioTTest,

    // ── XY 表 ──
    XyLinearRegression,
    XyCorrelation,
    XySpearman,
    XyNonlinearFit,
    XyDeming,
    XyCompareSlopes,
    XyAuc,
    XySmooth,
    XyInterpolate,
    XyTransform,
    XyNormalize,
    XyRemoveBaseline,
    XyColumnMath,
    XyTranspose,
    XySimulate,

    // ── Grouped 表（分组）──
    GroupedTwoWayAnova,
    GroupedDescribe,
    GroupedOneWayAnova,
    GroupedCorrelation,
    GroupedRegression,
    GroupedPca,
    GroupedHierarchical,
    GroupedKmeans,
    GroupedNestedAnova,
    GroupedTransform,
    GroupedNormalize,
    GroupedRemoveBaseline,
    GroupedColumnMath,
    GroupedTotalScore,

    // ── Contingency 表（列联）──
    ContingencyChiSquare,
    ContingencyFisher,
    ContingencyOddsRatio,
    ContingencyRisk,
    ContingencyMcnemar,
    ContingencyPartsCompare,
    ContingencySimulate,

    // ── Survival 表（生存）──
    SurvivalKm,
    SurvivalLogRank,
    SurvivalCox,
    SurvivalHazardRatio,

    // ── Parts 表（整体分解）──
    PartsCompareObservedExpected,
    PartsTotalScore,

    // ── Multiple 表（多变量）──
    MultiDescribe,
    MultiCorrelation,
    MultiLinearRegression,
    MultiOutlier,
    MultiPca,
    MultiHierarchical,
    MultiKmeans,
    MultiCox,

    // ── Nested 表（嵌套）──
    NestedOneWayAnova,
    NestedTTest,
    NestedDescribe,
    NestedNormality,
    NestedOutlier,

    XyRemoveRows,
    XyTotalScore,
    XyGenerateCurve,
    XyResidualPlot,
    XyBootstrapCI,
    XyFitCompare,
    ColumnTransform,
    ColumnNormalize,
    ColumnRemoveRows,
    ColumnSimulate,
    GroupedRemoveRows,
    GroupedTranspose,
    GroupedSimulate,
    GroupedRatioTTest,
    ContingencyKappa,}

impl AnalysisKind {
    pub fn zh(self) -> &'static str {
        match self {
            AnalysisKind::StatsAdvisor => "智能统计制图推荐",
            AnalysisKind::ColumnDescribe => "描述统计",
            AnalysisKind::ColumnOneWayAnova => "单因素 ANOVA",
            AnalysisKind::ColumnTTest => "独立样本 t 检验",
            AnalysisKind::ColumnWelchTTest => "Welch t 检验",
            AnalysisKind::ColumnOneSampleT => "单样本 t 检验",
            AnalysisKind::ColumnPairedT => "配对 t 检验",
            AnalysisKind::ColumnMannWhitney => "Mann-Whitney 检验",
            AnalysisKind::ColumnWilcoxon => "Wilcoxon 符号秩",
            AnalysisKind::ColumnKruskalWallis => "Kruskal-Wallis 检验",
            AnalysisKind::ColumnFriedman => "Friedman 检验",
            AnalysisKind::ColumnNormality => "正态性检验",
            AnalysisKind::ColumnOutlier => "离群值检测",
            AnalysisKind::ColumnBartlett => "Bartlett 方差齐性",
            AnalysisKind::ColumnLevene => "Levene 方差齐性",
            AnalysisKind::ColumnFTestVariance => "两样本 F 方差检验",
            AnalysisKind::ColumnMultiCompare => "多重比较",
            AnalysisKind::ColumnRatioTTest => "比值 t 检验",

            AnalysisKind::XyLinearRegression => "线性回归",
            AnalysisKind::XyCorrelation => "Pearson 相关",
            AnalysisKind::XySpearman => "Spearman 相关",
            AnalysisKind::XyNonlinearFit => "非线性拟合",
            AnalysisKind::XyDeming => "Deming 回归",
            AnalysisKind::XyCompareSlopes => "比较两条斜率",
            AnalysisKind::XyAuc => "曲线下面积 AUC",
            AnalysisKind::XySmooth => "平滑曲线",
            AnalysisKind::XyInterpolate => "插值",
            AnalysisKind::XyTransform => "数据变换",
            AnalysisKind::XyNormalize => "归一化",
            AnalysisKind::XyRemoveBaseline => "去除基线",
            AnalysisKind::XyColumnMath => "列运算",
            AnalysisKind::XyTranspose => "转置 XY",
            AnalysisKind::XySimulate => "模拟数据",

            AnalysisKind::GroupedTwoWayAnova => "双因素 ANOVA",
            AnalysisKind::GroupedDescribe => "描述统计",
            AnalysisKind::GroupedOneWayAnova => "单因素 ANOVA",
            AnalysisKind::GroupedCorrelation => "相关矩阵",
            AnalysisKind::GroupedRegression => "线性回归",
            AnalysisKind::GroupedPca => "主成分分析",
            AnalysisKind::GroupedHierarchical => "层次聚类",
            AnalysisKind::GroupedKmeans => "K 均值聚类",
            AnalysisKind::GroupedNestedAnova => "嵌套 ANOVA",
            AnalysisKind::GroupedTransform => "数据变换",
            AnalysisKind::GroupedNormalize => "归一化",
            AnalysisKind::GroupedRemoveBaseline => "去除基线",
            AnalysisKind::GroupedColumnMath => "列运算",
            AnalysisKind::GroupedTotalScore => "总分",

            AnalysisKind::ContingencyChiSquare => "卡方检验",
            AnalysisKind::ContingencyFisher => "Fisher 精确检验",
            AnalysisKind::ContingencyOddsRatio => "比值比 OR",
            AnalysisKind::ContingencyRisk => "相对风险 RR",
            AnalysisKind::ContingencyMcnemar => "McNemar 检验",
            AnalysisKind::ContingencyPartsCompare => "观察/期望比较",
            AnalysisKind::ContingencySimulate => "模拟数据",

            AnalysisKind::SurvivalKm => "Kaplan-Meier 生存",
            AnalysisKind::SurvivalLogRank => "Log-rank 检验",
            AnalysisKind::SurvivalCox => "Cox 回归",
            AnalysisKind::SurvivalHazardRatio => "风险比 HR",

            AnalysisKind::PartsCompareObservedExpected => "观察/期望比较",
            AnalysisKind::PartsTotalScore => "总分",

            AnalysisKind::MultiDescribe => "描述统计",
            AnalysisKind::MultiCorrelation => "相关矩阵",
            AnalysisKind::MultiLinearRegression => "线性回归",
            AnalysisKind::MultiOutlier => "离群值检测",
            AnalysisKind::MultiPca => "主成分分析",
            AnalysisKind::MultiHierarchical => "层次聚类",
            AnalysisKind::MultiKmeans => "K 均值聚类",
            AnalysisKind::MultiCox => "Cox 回归",

            AnalysisKind::NestedOneWayAnova => "嵌套单因素 ANOVA",
            AnalysisKind::NestedTTest => "嵌套 t 检验",
            AnalysisKind::NestedDescribe => "描述统计",
            AnalysisKind::NestedNormality => "正态性检验",
            AnalysisKind::NestedOutlier => "离群值检测",
            AnalysisKind::XyRemoveRows => "删除行",
            AnalysisKind::XyTotalScore => "总分",
            AnalysisKind::XyGenerateCurve => "生成曲线",
            AnalysisKind::XyResidualPlot => "残差分析",
            AnalysisKind::XyBootstrapCI => "自助法置信区间",
            AnalysisKind::XyFitCompare => "拟合比较",
            AnalysisKind::ColumnTransform => "转换",
            AnalysisKind::ColumnNormalize => "归一化",
            AnalysisKind::ColumnRemoveRows => "删除行",
            AnalysisKind::ColumnSimulate => "模拟数据",
            AnalysisKind::GroupedRemoveRows => "删除行",
            AnalysisKind::GroupedTranspose => "转置",
            AnalysisKind::GroupedSimulate => "模拟数据",
            AnalysisKind::GroupedRatioTTest => "比率 t 检验",
            AnalysisKind::ContingencyKappa => "Kappa 一致性",
        }
    }

    #[allow(dead_code)]
    pub fn en(self) -> &'static str {
        match self {
            AnalysisKind::StatsAdvisor => "Statistics advisor",
            AnalysisKind::ColumnDescribe => "Descriptive statistics",
            AnalysisKind::ColumnOneWayAnova => "One-way ANOVA",
            AnalysisKind::ColumnTTest => "Unpaired t test",
            AnalysisKind::ColumnWelchTTest => "Welch t test",
            AnalysisKind::ColumnOneSampleT => "One-sample t test",
            AnalysisKind::ColumnPairedT => "Paired t test",
            AnalysisKind::ColumnMannWhitney => "Mann-Whitney test",
            AnalysisKind::ColumnWilcoxon => "Wilcoxon signed-rank",
            AnalysisKind::ColumnKruskalWallis => "Kruskal-Wallis test",
            AnalysisKind::ColumnFriedman => "Friedman test",
            AnalysisKind::ColumnNormality => "Normality tests",
            AnalysisKind::ColumnOutlier => "Outlier detection",
            AnalysisKind::ColumnBartlett => "Bartlett's test",
            AnalysisKind::ColumnLevene => "Levene's test",
            AnalysisKind::ColumnFTestVariance => "F test of variance",
            AnalysisKind::ColumnMultiCompare => "Multiple comparisons",
            AnalysisKind::ColumnRatioTTest => "Ratio t test",

            AnalysisKind::XyLinearRegression => "Linear regression",
            AnalysisKind::XyCorrelation => "Pearson correlation",
            AnalysisKind::XySpearman => "Spearman correlation",
            AnalysisKind::XyNonlinearFit => "Nonlinear fit",
            AnalysisKind::XyDeming => "Deming regression",
            AnalysisKind::XyCompareSlopes => "Compare slopes",
            AnalysisKind::XyAuc => "Area under curve",
            AnalysisKind::XySmooth => "Smooth curve",
            AnalysisKind::XyInterpolate => "Interpolate",
            AnalysisKind::XyTransform => "Transform",
            AnalysisKind::XyNormalize => "Normalize",
            AnalysisKind::XyRemoveBaseline => "Remove baseline",
            AnalysisKind::XyColumnMath => "Column math",
            AnalysisKind::XyTranspose => "Transpose X/Y",
            AnalysisKind::XySimulate => "Simulate data",

            AnalysisKind::GroupedTwoWayAnova => "Two-way ANOVA",
            AnalysisKind::GroupedDescribe => "Descriptive statistics",
            AnalysisKind::GroupedOneWayAnova => "One-way ANOVA",
            AnalysisKind::GroupedCorrelation => "Correlation matrix",
            AnalysisKind::GroupedRegression => "Linear regression",
            AnalysisKind::GroupedPca => "PCA",
            AnalysisKind::GroupedHierarchical => "Hierarchical clustering",
            AnalysisKind::GroupedKmeans => "K-means clustering",
            AnalysisKind::GroupedNestedAnova => "Nested ANOVA",
            AnalysisKind::GroupedTransform => "Transform",
            AnalysisKind::GroupedNormalize => "Normalize",
            AnalysisKind::GroupedRemoveBaseline => "Remove baseline",
            AnalysisKind::GroupedColumnMath => "Column math",
            AnalysisKind::GroupedTotalScore => "Total score",

            AnalysisKind::ContingencyChiSquare => "Chi-square test",
            AnalysisKind::ContingencyFisher => "Fisher's exact test",
            AnalysisKind::ContingencyOddsRatio => "Odds ratio",
            AnalysisKind::ContingencyRisk => "Relative risk",
            AnalysisKind::ContingencyMcnemar => "McNemar test",
            AnalysisKind::ContingencyPartsCompare => "Observed vs expected",
            AnalysisKind::ContingencySimulate => "Simulate data",

            AnalysisKind::SurvivalKm => "Kaplan-Meier",
            AnalysisKind::SurvivalLogRank => "Log-rank test",
            AnalysisKind::SurvivalCox => "Cox regression",
            AnalysisKind::SurvivalHazardRatio => "Hazard ratio",

            AnalysisKind::PartsCompareObservedExpected => "Observed vs expected",
            AnalysisKind::PartsTotalScore => "Total score",

            AnalysisKind::MultiDescribe => "Descriptive statistics",
            AnalysisKind::MultiCorrelation => "Correlation matrix",
            AnalysisKind::MultiLinearRegression => "Linear regression",
            AnalysisKind::MultiOutlier => "Outlier detection",
            AnalysisKind::MultiPca => "PCA",
            AnalysisKind::MultiHierarchical => "Hierarchical clustering",
            AnalysisKind::MultiKmeans => "K-means clustering",
            AnalysisKind::MultiCox => "Cox regression",

            AnalysisKind::NestedOneWayAnova => "Nested one-way ANOVA",
            AnalysisKind::NestedTTest => "Nested t test",
            AnalysisKind::NestedDescribe => "Descriptive statistics",
            AnalysisKind::NestedNormality => "Normality tests",
            AnalysisKind::NestedOutlier => "Outlier detection",
            AnalysisKind::XyRemoveRows => "Remove rows",
            AnalysisKind::XyTotalScore => "Total score",
            AnalysisKind::XyGenerateCurve => "Generate curve",
            AnalysisKind::XyResidualPlot => "Residual plot",
            AnalysisKind::XyBootstrapCI => "Bootstrap CI",
            AnalysisKind::XyFitCompare => "Compare fits",
            AnalysisKind::ColumnTransform => "Transform",
            AnalysisKind::ColumnNormalize => "Normalize",
            AnalysisKind::ColumnRemoveRows => "Remove rows",
            AnalysisKind::ColumnSimulate => "Simulate data",
            AnalysisKind::GroupedRemoveRows => "Remove rows",
            AnalysisKind::GroupedTranspose => "Transpose",
            AnalysisKind::GroupedSimulate => "Simulate data",
            AnalysisKind::GroupedRatioTTest => "Ratio t test",
            AnalysisKind::ContingencyKappa => "Kappa agreement",
        }
    }

    /// 是否已有真实实现（false = 选择时明确提示"未实现"，不给假数字）。
    #[allow(dead_code)]
    pub fn implemented(self) -> bool {
        match self {
            // ── 已实现 ──
            AnalysisKind::ColumnDescribe
            | AnalysisKind::ColumnOneWayAnova
            | AnalysisKind::ColumnTTest
            | AnalysisKind::ColumnWelchTTest
            | AnalysisKind::ColumnMannWhitney
            | AnalysisKind::ColumnWilcoxon
            | AnalysisKind::ColumnKruskalWallis
            | AnalysisKind::ColumnFriedman
            | AnalysisKind::ColumnNormality
            | AnalysisKind::ColumnOutlier
            | AnalysisKind::ColumnLevene
            | AnalysisKind::ColumnMultiCompare
            | AnalysisKind::ColumnOneSampleT
            | AnalysisKind::ColumnPairedT
            | AnalysisKind::XyLinearRegression
            | AnalysisKind::XyCorrelation
            | AnalysisKind::XySpearman
            | AnalysisKind::GroupedTwoWayAnova
            | AnalysisKind::GroupedDescribe
            | AnalysisKind::GroupedOneWayAnova
            | AnalysisKind::GroupedCorrelation
            | AnalysisKind::GroupedRegression
            | AnalysisKind::GroupedNestedAnova
            | AnalysisKind::ContingencyChiSquare
            | AnalysisKind::ContingencyFisher
            | AnalysisKind::ContingencyOddsRatio
            | AnalysisKind::ContingencyRisk
            | AnalysisKind::ContingencyPartsCompare
            | AnalysisKind::SurvivalKm
            | AnalysisKind::PartsCompareObservedExpected
            | AnalysisKind::MultiDescribe
            | AnalysisKind::MultiCorrelation
            | AnalysisKind::MultiLinearRegression
            | AnalysisKind::MultiOutlier
            | AnalysisKind::NestedOneWayAnova
            | AnalysisKind::NestedDescribe
            | AnalysisKind::NestedNormality
            | AnalysisKind::NestedOutlier
            | AnalysisKind::StatsAdvisor => true,

            // ── 尚未实现 ──
            _ => false,
        }
    }

    /// 该分析所属的分类名（用于菜单分组）。
    #[allow(dead_code)]
    pub fn category(self) -> &'static str {
        match self {
            AnalysisKind::ColumnDescribe
            | AnalysisKind::ColumnOneWayAnova
            | AnalysisKind::ColumnTTest
            | AnalysisKind::ColumnWelchTTest
            | AnalysisKind::ColumnOneSampleT
            | AnalysisKind::ColumnPairedT
            | AnalysisKind::ColumnMannWhitney
            | AnalysisKind::ColumnWilcoxon
            | AnalysisKind::ColumnKruskalWallis
            | AnalysisKind::ColumnFriedman
            | AnalysisKind::ColumnNormality
            | AnalysisKind::ColumnOutlier
            | AnalysisKind::ColumnBartlett
            | AnalysisKind::ColumnLevene
            | AnalysisKind::ColumnFTestVariance
            | AnalysisKind::ColumnMultiCompare
            | AnalysisKind::ColumnRatioTTest => "列表分析",

            AnalysisKind::XyLinearRegression
            | AnalysisKind::XyCorrelation
            | AnalysisKind::XySpearman
            | AnalysisKind::XyNonlinearFit
            | AnalysisKind::XyDeming
            | AnalysisKind::XyCompareSlopes
            | AnalysisKind::XyAuc => "XY 分析",

            AnalysisKind::XyTransform
            | AnalysisKind::XyNormalize
            | AnalysisKind::XyRemoveBaseline
            | AnalysisKind::XyColumnMath
            | AnalysisKind::XyTranspose
            | AnalysisKind::XySmooth
            | AnalysisKind::XyInterpolate
            | AnalysisKind::GroupedTransform
            | AnalysisKind::GroupedNormalize
            | AnalysisKind::GroupedRemoveBaseline
            | AnalysisKind::GroupedColumnMath
            | AnalysisKind::GroupedTotalScore => "数据操作",

            AnalysisKind::XySimulate | AnalysisKind::ContingencySimulate => "模拟数据",

            AnalysisKind::GroupedTwoWayAnova
            | AnalysisKind::GroupedDescribe
            | AnalysisKind::GroupedOneWayAnova => "分组分析",

            AnalysisKind::GroupedCorrelation
            | AnalysisKind::GroupedRegression
            | AnalysisKind::MultiDescribe
            | AnalysisKind::MultiCorrelation
            | AnalysisKind::MultiLinearRegression
            | AnalysisKind::MultiOutlier => "多变量分析",

            AnalysisKind::GroupedPca
            | AnalysisKind::GroupedHierarchical
            | AnalysisKind::GroupedKmeans
            | AnalysisKind::MultiPca
            | AnalysisKind::MultiHierarchical
            | AnalysisKind::MultiKmeans => "聚类分析",

            AnalysisKind::GroupedNestedAnova
            | AnalysisKind::NestedOneWayAnova
            | AnalysisKind::NestedTTest
            | AnalysisKind::NestedDescribe
            | AnalysisKind::NestedNormality
            | AnalysisKind::NestedOutlier => "嵌套分析",

            AnalysisKind::ContingencyChiSquare
            | AnalysisKind::ContingencyFisher
            | AnalysisKind::ContingencyOddsRatio
            | AnalysisKind::ContingencyRisk
            | AnalysisKind::ContingencyMcnemar => "列联表分析",

            AnalysisKind::ContingencyPartsCompare
            | AnalysisKind::PartsCompareObservedExpected
            | AnalysisKind::PartsTotalScore => "整体分解分析",

            AnalysisKind::SurvivalKm
            | AnalysisKind::SurvivalLogRank
            | AnalysisKind::SurvivalCox
            | AnalysisKind::SurvivalHazardRatio
            | AnalysisKind::MultiCox => "生存分析",

            AnalysisKind::StatsAdvisor => "智能推荐",
            AnalysisKind::XyRemoveRows => "Data manipulation",
            AnalysisKind::XyTotalScore => "Data manipulation",
            AnalysisKind::XyGenerateCurve => "Generate curve",
            AnalysisKind::XyResidualPlot => "XY analyses",
            AnalysisKind::XyBootstrapCI => "XY analyses",
            AnalysisKind::XyFitCompare => "XY analyses",
            AnalysisKind::ColumnTransform => "Data manipulation",
            AnalysisKind::ColumnNormalize => "Data manipulation",
            AnalysisKind::ColumnRemoveRows => "Data manipulation",
            AnalysisKind::ColumnSimulate => "Simulate data",
            AnalysisKind::GroupedRemoveRows => "Data manipulation",
            AnalysisKind::GroupedTranspose => "Data manipulation",
            AnalysisKind::GroupedSimulate => "Simulate data",
            AnalysisKind::GroupedRatioTTest => "Grouped analyses",
            AnalysisKind::ContingencyKappa => "Contingency table analyses",
        }
    }

    /// 某表型可用的分析白名单（可用分析由表结构决定，不由数据内容猜测）。
    #[allow(dead_code)]
    pub fn for_table(tt: TableType) -> &'static [AnalysisKind] {
        // 顾问只对三种表给入口，并插到列表第 0 位。
        // 理由：它需要「每组的原始值列表」，只有 Column / Grouped / XY
        // 三种结构能稳定取出，其余表型点了必然跑不出来。
        match tt {
            TableType::XY => &[
                AnalysisKind::XyLinearRegression,
                AnalysisKind::XyCorrelation,
                AnalysisKind::XySpearman,
                AnalysisKind::XyNonlinearFit,
                AnalysisKind::XyDeming,
                AnalysisKind::XyCompareSlopes,
                AnalysisKind::XyAuc,
                AnalysisKind::XySmooth,
                AnalysisKind::XyInterpolate,
                AnalysisKind::XyTransform,
                AnalysisKind::XyNormalize,
                AnalysisKind::XyRemoveBaseline,
                AnalysisKind::XyColumnMath,
                AnalysisKind::XyTranspose,
                AnalysisKind::XySimulate,
                AnalysisKind::XyRemoveRows,
                AnalysisKind::XyTotalScore,
                AnalysisKind::XyGenerateCurve,
                AnalysisKind::XyResidualPlot,
                AnalysisKind::XyBootstrapCI,
                AnalysisKind::XyFitCompare,
            ],
            TableType::Column => &[
                AnalysisKind::ColumnDescribe,
                AnalysisKind::ColumnOneWayAnova,
                AnalysisKind::ColumnTTest,
                AnalysisKind::ColumnWelchTTest,
                AnalysisKind::ColumnOneSampleT,
                AnalysisKind::ColumnPairedT,
                AnalysisKind::ColumnMannWhitney,
                AnalysisKind::ColumnWilcoxon,
                AnalysisKind::ColumnKruskalWallis,
                AnalysisKind::ColumnFriedman,
                AnalysisKind::ColumnNormality,
                AnalysisKind::ColumnOutlier,
                AnalysisKind::ColumnBartlett,
                AnalysisKind::ColumnLevene,
                AnalysisKind::ColumnFTestVariance,
                AnalysisKind::ColumnMultiCompare,
                AnalysisKind::ColumnRatioTTest,
                AnalysisKind::ColumnTransform,
                AnalysisKind::ColumnNormalize,
                AnalysisKind::ColumnRemoveRows,
                AnalysisKind::ColumnSimulate,
            ],
            TableType::Grouped => &[
                AnalysisKind::GroupedTwoWayAnova,
                AnalysisKind::GroupedDescribe,
                AnalysisKind::GroupedOneWayAnova,
                AnalysisKind::GroupedCorrelation,
                AnalysisKind::GroupedRegression,
                AnalysisKind::GroupedPca,
                AnalysisKind::GroupedHierarchical,
                AnalysisKind::GroupedKmeans,
                AnalysisKind::GroupedNestedAnova,
                AnalysisKind::GroupedTransform,
                AnalysisKind::GroupedNormalize,
                AnalysisKind::GroupedRemoveBaseline,
                AnalysisKind::GroupedColumnMath,
                AnalysisKind::GroupedTotalScore,
                AnalysisKind::GroupedRemoveRows,
                AnalysisKind::GroupedTranspose,
                AnalysisKind::GroupedSimulate,
                AnalysisKind::GroupedRatioTTest,
            ],
            TableType::Contingency => &[
                AnalysisKind::ContingencyChiSquare,
                AnalysisKind::ContingencyFisher,
                AnalysisKind::ContingencyOddsRatio,
                AnalysisKind::ContingencyRisk,
                AnalysisKind::ContingencyMcnemar,
                AnalysisKind::ContingencyPartsCompare,
                AnalysisKind::ContingencySimulate,
                AnalysisKind::ContingencyKappa,
            ],
            TableType::Survival => &[
                AnalysisKind::SurvivalKm,
                AnalysisKind::SurvivalLogRank,
                AnalysisKind::SurvivalCox,
                AnalysisKind::SurvivalHazardRatio,
            ],
            TableType::PartsOfWhole => &[
                AnalysisKind::PartsCompareObservedExpected,
                AnalysisKind::PartsTotalScore,
            ],
            TableType::MultipleVariables => &[
                AnalysisKind::MultiDescribe,
                AnalysisKind::MultiCorrelation,
                AnalysisKind::MultiLinearRegression,
                AnalysisKind::MultiOutlier,
                AnalysisKind::MultiPca,
                AnalysisKind::MultiHierarchical,
                AnalysisKind::MultiKmeans,
                AnalysisKind::MultiCox,
            ],
            TableType::Nested => &[
                AnalysisKind::NestedOneWayAnova,
                AnalysisKind::NestedTTest,
                AnalysisKind::NestedDescribe,
                AnalysisKind::NestedNormality,
                AnalysisKind::NestedOutlier,
            ],
        }
    }

    /// 八条硬映射：建表后的默认分析，选"该表型必有足够数据可算"的那一项。
    /// `for_table()` 的「顾问版」：在第 0 位插入智能顾问。
    #[allow(dead_code)]
    pub fn for_table_with_advisor(tt: TableType) -> Vec<AnalysisKind> {
        let mut v = Self::for_table(tt).to_vec();
        match tt {
            TableType::Column | TableType::Grouped | TableType::XY => {
                v.insert(0, AnalysisKind::StatsAdvisor);
            }
            _ => {}
        }
        v
    }

    #[allow(dead_code)]
    pub fn default_for(tt: TableType) -> AnalysisKind {
        match tt {
            TableType::XY => AnalysisKind::XyLinearRegression,
            TableType::Column => AnalysisKind::ColumnOneWayAnova,
            TableType::Grouped => AnalysisKind::GroupedTwoWayAnova,
            TableType::Contingency => AnalysisKind::ContingencyChiSquare,
            TableType::Survival => AnalysisKind::SurvivalKm,
            TableType::PartsOfWhole => AnalysisKind::PartsCompareObservedExpected,
            TableType::MultipleVariables => AnalysisKind::MultiDescribe,
            TableType::Nested => AnalysisKind::NestedOneWayAnova,
        }
    }

    /// 按分类归并，返回 `(分类名, 该分类下的分析)`。
    #[allow(dead_code)]
    pub fn for_table_grouped(tt: TableType) -> Vec<(&'static str, Vec<AnalysisKind>)> {
        let mut out: Vec<(&'static str, Vec<AnalysisKind>)> = Vec::new();
        for k in Self::for_table(tt) {
            let c = k.category();
            if let Some(e) = out.iter_mut().find(|(n, _)| *n == c) {
                e.1.push(*k);
            } else {
                out.push((c, vec![*k]));
            }
        }
        out
    }

    /// 全量列表（用于反查 id）。
    pub fn all() -> &'static [AnalysisKind] {
        &[
            // Column
            AnalysisKind::ColumnDescribe,
            AnalysisKind::ColumnOneWayAnova,
            AnalysisKind::ColumnTTest,
            AnalysisKind::ColumnWelchTTest,
            AnalysisKind::ColumnOneSampleT,
            AnalysisKind::ColumnPairedT,
            AnalysisKind::ColumnMannWhitney,
            AnalysisKind::ColumnWilcoxon,
            AnalysisKind::ColumnKruskalWallis,
            AnalysisKind::ColumnFriedman,
            AnalysisKind::ColumnNormality,
            AnalysisKind::ColumnOutlier,
            AnalysisKind::ColumnBartlett,
            AnalysisKind::ColumnLevene,
            AnalysisKind::ColumnFTestVariance,
            AnalysisKind::ColumnMultiCompare,
            AnalysisKind::ColumnRatioTTest,
            // XY
            AnalysisKind::XyLinearRegression,
            AnalysisKind::XyCorrelation,
            AnalysisKind::XySpearman,
            AnalysisKind::XyNonlinearFit,
            AnalysisKind::XyDeming,
            AnalysisKind::XyCompareSlopes,
            AnalysisKind::XyAuc,
            AnalysisKind::XySmooth,
            AnalysisKind::XyInterpolate,
            AnalysisKind::XyTransform,
            AnalysisKind::XyNormalize,
            AnalysisKind::XyRemoveBaseline,
            AnalysisKind::XyColumnMath,
            AnalysisKind::XyTranspose,
            AnalysisKind::XySimulate,
            // Grouped
            AnalysisKind::GroupedTwoWayAnova,
            AnalysisKind::GroupedDescribe,
            AnalysisKind::GroupedOneWayAnova,
            AnalysisKind::GroupedCorrelation,
            AnalysisKind::GroupedRegression,
            AnalysisKind::GroupedPca,
            AnalysisKind::GroupedHierarchical,
            AnalysisKind::GroupedKmeans,
            AnalysisKind::GroupedNestedAnova,
            AnalysisKind::GroupedTransform,
            AnalysisKind::GroupedNormalize,
            AnalysisKind::GroupedRemoveBaseline,
            AnalysisKind::GroupedColumnMath,
            AnalysisKind::GroupedTotalScore,
            // Contingency
            AnalysisKind::ContingencyChiSquare,
            AnalysisKind::ContingencyFisher,
            AnalysisKind::ContingencyOddsRatio,
            AnalysisKind::ContingencyRisk,
            AnalysisKind::ContingencyMcnemar,
            AnalysisKind::ContingencyPartsCompare,
            AnalysisKind::ContingencySimulate,
            // Survival
            AnalysisKind::SurvivalKm,
            AnalysisKind::SurvivalLogRank,
            AnalysisKind::SurvivalCox,
            AnalysisKind::SurvivalHazardRatio,
            // Parts
            AnalysisKind::PartsCompareObservedExpected,
            AnalysisKind::PartsTotalScore,
            // Multiple
            AnalysisKind::MultiDescribe,
            AnalysisKind::MultiCorrelation,
            AnalysisKind::MultiLinearRegression,
            AnalysisKind::MultiOutlier,
            AnalysisKind::MultiPca,
            AnalysisKind::MultiHierarchical,
            AnalysisKind::MultiKmeans,
            AnalysisKind::MultiCox,
            // Nested
            AnalysisKind::NestedOneWayAnova,
            AnalysisKind::NestedTTest,
            AnalysisKind::NestedDescribe,
            AnalysisKind::NestedNormality,
            AnalysisKind::NestedOutlier,
            AnalysisKind::XyRemoveRows,
            AnalysisKind::XyTotalScore,
            AnalysisKind::XyGenerateCurve,
            AnalysisKind::XyResidualPlot,
            AnalysisKind::XyBootstrapCI,
            AnalysisKind::XyFitCompare,
            AnalysisKind::ColumnTransform,
            AnalysisKind::ColumnNormalize,
            AnalysisKind::ColumnRemoveRows,
            AnalysisKind::ColumnSimulate,
            AnalysisKind::GroupedRemoveRows,
            AnalysisKind::GroupedTranspose,
            AnalysisKind::GroupedSimulate,
            AnalysisKind::GroupedRatioTTest,
            AnalysisKind::ContingencyKappa,
        ]
    }

    /// 在 `all()` 中的索引（存盘用）。
    #[allow(dead_code)]
    pub fn id(self) -> usize {
        Self::all().iter().position(|k| *k == self).unwrap_or(0)
    }

    /// 由索引反查。
    #[allow(dead_code)]
    pub fn from_id(i: usize) -> AnalysisKind {
        Self::all().get(i).copied().unwrap_or(AnalysisKind::ColumnDescribe)
    }
}

/// 实验设计：配对（含重复测量）或独立样本。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Design {
    Paired,
    Independent,
}

/// 顾问的诊断结果：决策树的全部输入依据。
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct Diagnosis {
    pub design: Design,
    /// 有效组数（已剔除样本量不足的组）
    pub num_groups: usize,
    /// 各组有效样本量
    pub group_n: Vec<usize>,
    /// 所有组都通过正态性检验
    pub is_normal: bool,
    /// 各组正态性检验的 p 值
    pub normality_p: Vec<f64>,
    /// 方差齐性检验通过
    pub equal_variance: bool,
    /// 方差齐性检验的 p 值
    pub variance_p: f64,
}

impl Diagnosis {
    /// 按文档步骤诊断：
    /// 1) 过滤非有限值
    /// 2) 剔除样本量 <2 的组
    /// 3) 组数 <2 直接返回 None（无法比较）
    /// 4) 逐组正态性：优先 D'Agostino–Pearson（n≥20），否则 Lilliefors（n≥5）
    /// 5) 方差齐性：Levene（对非正态稳健，优于 Bartlett）
    ///
    /// 两处「算不出来」的处理：正态性未知时按「不报警」，
    /// 方差齐性未知时按「齐」——都是保守方向，不因样本量小就误判。
    pub fn diagnose(cols: &[Vec<f64>], design: Design, alpha: f64) -> Option<Diagnosis> {
        let groups: Vec<Vec<f64>> = cols
            .iter()
            .map(|c| c.iter().copied().filter(|v| v.is_finite()).collect())
            .filter(|c: &Vec<f64>| c.len() >= 2)
            .collect();
        if groups.len() < 2 {
            return None;
        }
        let num_groups = groups.len();
        let group_n = groups.iter().map(|g| g.len()).collect::<Vec<_>>();

        let mut normality_p = Vec::with_capacity(num_groups);
        let mut is_normal = true;
        for g in &groups {
            let p = crate::stats::dagostino(g)
                .or_else(|| crate::stats::lilliefors(g))
                .map(|r| r.p)
                .unwrap_or(1.0);
            if !(p > alpha) {
                is_normal = false;
            }
            normality_p.push(p);
        }

        let variance_p = crate::stats::levene_p(&groups).unwrap_or(1.0);
        let equal_variance = variance_p > alpha;

        Some(Diagnosis {
            design,
            num_groups,
            group_n,
            is_normal,
            normality_p,
            equal_variance,
            variance_p,
        })
    }
}

/// 顾问的推荐结果。
#[derive(Clone, Debug)]
pub struct Recommendation {
    pub chart_type: &'static str,
    pub test_method: &'static str,
    pub data_expression: &'static str,
    pub warning: Option<&'static str>,
    /// 必须叠加原始散点
    pub must_overlay_points: bool,
    /// 禁用带均值误差棒的柱状图
    pub forbid_mean_bar: bool,
}

impl Recommendation {
    /// 决策树：按「组数 × 设计 × 正态性 × 方差齐性」推理。
    pub fn recommend(d: &Diagnosis) -> Self {
        // 三个通用标志先算：非正态 → 禁柱状图；正态且独立且多组 → 必须叠加散点
        let (warning, must_overlay_points, forbid_mean_bar) = if !d.is_normal {
            (
                Some("数据非正态：切勿使用带均值误差棒的柱状图，这会严重扭曲偏态数据的真实分布。"),
                false,
                true,
            )
        } else if !matches!(d.design, Design::Paired) && d.num_groups >= 2 {
            (
                Some("若使用柱状图，请务必叠加原始散点（Jitter），否则会掩盖真实分布。"),
                true,
                false,
            )
        } else {
            (None, false, false)
        };

        let (chart_type, test_method, data_expression) = if d.num_groups == 2 {
            match (d.design, d.is_normal, d.equal_variance) {
                (Design::Paired, true, _) => (
                    "配对折线图（Before-After）",
                    "配对样本 t 检验",
                    "均值 ± SD 或 95% CI",
                ),
                (Design::Paired, false, _) => (
                    "配对折线图（带中位数）/ 箱线图",
                    "Wilcoxon 符号秩检验",
                    "中位数（IQR）",
                ),
                (Design::Independent, true, true) => (
                    "柱状图（带散点）/ 箱线图",
                    "独立样本 t 检验（Student）",
                    "均值 ± SD",
                ),
                (Design::Independent, true, false) => (
                    "柱状图（带散点）/ 箱线图",
                    "Welch's t 检验",
                    "均值 ± SD",
                ),
                (Design::Independent, false, _) => (
                    "箱线图 / 小提琴图",
                    "Mann–Whitney U 检验",
                    "中位数（IQR）",
                ),
            }
        } else {
            match (d.design, d.is_normal, d.equal_variance) {
                (Design::Paired, true, _) => (
                    "重复测量折线图（Spaghetti）",
                    "重复测量单因素 ANOVA + 事后检验",
                    "均值 ± SD",
                ),
                (Design::Paired, false, _) => (
                    "分组箱线图",
                    "Friedman 检验",
                    "中位数（IQR）",
                ),
                (Design::Independent, true, true) => (
                    "分组柱状图（带散点）",
                    "单因素 ANOVA + Tukey",
                    "均值 ± SD",
                ),
                (Design::Independent, true, false) => (
                    "分组箱线图",
                    "Welch's ANOVA + Games-Howell",
                    "均值 ± SD",
                ),
                (Design::Independent, false, _) => (
                    "分组箱线图 / 小提琴图",
                    "Kruskal–Wallis + Dunn's",
                    "中位数（IQR）",
                ),
            }
        };
        Self {
            chart_type,
            test_method,
            data_expression,
            warning,
            must_overlay_points,
            forbid_mean_bar,
        }
    }
}

/// 顾问专用的显著性阈值。
///
/// **注意**：这里用 `1 - conf` 而不是 `params.alpha`。
/// 两者是独立字段，默认是巧合一致（0.95 → 0.05），
/// 若用户把置信水平改成 99% 而 α 仍为 0.05，顾问会按 0.01 判显著。
/// 保留这个行为是因为顾问输出的是「区间与结论」，
/// 与置信水平同源才自洽（否则 99% CI 配 0.05 阈值，区间与结论会互相矛盾）。
pub fn advisor_alpha(p: &AnalysisParams) -> f64 {
    1.0 - p.conf
}

/// 把顾问诊断与推荐铺成结果表。
///
/// 全部依据都列出来（各组正态性 p、方差齐性 p），
/// 用户能直接看到「为什么推荐这个方法」。
#[allow(dead_code)]
pub fn advisor_rows(d: &Diagnosis, r: &Recommendation, alpha: f64) -> Vec<Vec<String>> {
    let fp = crate::stats::fmt_p;
    let mut rows: Vec<Vec<String>> = Vec::new();
    rows.push(vec!["组数".into(), d.num_groups.to_string()]);
    rows.push(vec![
        "设计".into(),
        if d.design == Design::Paired {
            "配对 / 重复测量"
        } else {
            "独立样本"
        }
        .into(),
    ]);
    rows.push(vec!["各组样本量".into(),
        d.group_n.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")]);
    rows.push(vec![
        "正态性".into(),
        if d.is_normal { "正态" } else { "非正态" }.into(),
    ]);
    rows.push(vec!["正态性 p（各组）".into(),
        d.normality_p.iter().map(|v| fp(*v)).collect::<Vec<_>>().join(", ")]);
    rows.push(vec!["方差齐性检验 p".into(), fp(d.variance_p)]);
    rows.push(vec![
        "方差齐性".into(),
        if d.equal_variance { "齐" } else { "不齐" }.into(),
    ]);
    rows.push(vec!["显著性阈值 α".into(), format!("{:.4}", alpha)]);
    rows.push(vec!["推荐图表".into(), r.chart_type.into()]);
    rows.push(vec!["推荐检验".into(), r.test_method.into()]);
    rows.push(vec!["推荐数据表达".into(), r.data_expression.into()]);
    // 已知缺口：Games-Howell 与 Dunn's 在部分表型下会回退，这里如实标注
    if r.test_method.contains("Games-Howell") || r.test_method.contains("Dunn's") {
        rows.push(vec![
            "说明".into(),
            "当前版本的多重比较已支持该方法；若下拉中不可用会回退到 Tukey。".into(),
        ]);
    }
    if let Some(w) = r.warning {
        rows.push(vec!["避坑提示".into(), w.into()]);
    }
    rows.push(vec![
        "必须叠加散点".into(),
        if r.must_overlay_points { "是" } else { "否" }.into(),
    ]);
    rows.push(vec![
        "禁用均值误差棒柱状图".into(),
        if r.forbid_mean_bar { "是" } else { "否" }.into(),
    ]);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_default_matches_spec() {
        let p = AnalysisParams::default();
        assert!((p.conf - 0.95).abs() < 1e-12);
        assert!((p.alpha - 0.05).abs() < 1e-12);
        assert!(p.welch);
        assert!(p.yates);
        assert_eq!(p.method, MultiMethod::Tukey);
        assert_eq!(p.model, ModelId::ExpDecay1);
        assert_eq!(p.clusters, 3);
        assert_eq!(p.n_points, 101);
        assert!((p.deming_ratio - 1.0).abs() < 1e-12);
        assert_eq!(p.group_a, 0);
        assert_eq!(p.group_b, 1);
    }

    #[test]
    fn params_roundtrip() {
        let mut p = AnalysisParams::default();
        p.conf = 0.99;
        p.alpha = 0.01;
        p.welch = false;
        p.method = MultiMethod::GamesHowell;
        p.model = ModelId::Sigmoid4PL;
        p.clusters = 5;
        let back = AnalysisParams::load_lines(&p.save_lines());
        assert_eq!(p, back);
    }

    #[test]
    fn eight_default_mappings() {
        assert_eq!(
            AnalysisKind::default_for(TableType::XY),
            AnalysisKind::XyLinearRegression
        );
        assert_eq!(
            AnalysisKind::default_for(TableType::Column),
            AnalysisKind::ColumnOneWayAnova
        );
        assert_eq!(
            AnalysisKind::default_for(TableType::Grouped),
            AnalysisKind::GroupedTwoWayAnova
        );
        assert_eq!(
            AnalysisKind::default_for(TableType::Contingency),
            AnalysisKind::ContingencyChiSquare
        );
        assert_eq!(
            AnalysisKind::default_for(TableType::Survival),
            AnalysisKind::SurvivalKm
        );
        assert_eq!(
            AnalysisKind::default_for(TableType::PartsOfWhole),
            AnalysisKind::PartsCompareObservedExpected
        );
        assert_eq!(
            AnalysisKind::default_for(TableType::MultipleVariables),
            AnalysisKind::MultiDescribe
        );
        assert_eq!(
            AnalysisKind::default_for(TableType::Nested),
            AnalysisKind::NestedOneWayAnova
        );
    }

    #[test]
    fn id_roundtrip() {
        for k in AnalysisKind::all() {
            assert_eq!(AnalysisKind::from_id(k.id()), *k);
        }
    }

    #[test]
    fn defaults_are_in_table_whitelist() {
        // 默认分析必须出现在该表型的白名单里，否则自动分析会落到不可用项
        for tt in [
            TableType::XY,
            TableType::Column,
            TableType::Grouped,
            TableType::Contingency,
            TableType::Survival,
            TableType::PartsOfWhole,
            TableType::MultipleVariables,
            TableType::Nested,
        ] {
            let d = AnalysisKind::default_for(tt);
            assert!(
                AnalysisKind::for_table(tt).contains(&d),
                "默认分析 {:?} 不在 {:?} 的白名单中",
                d,
                tt
            );
        }
    }
}
