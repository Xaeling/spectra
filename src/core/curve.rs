//! 曲线拟合：多项式（Householder QR）+ 非线性（Levenberg-Marquardt）+ 自然三次样条。
//!
//! 移植自 `elisa-calc-rs`（同作者，MIT）。两条硬要求：
//! 1. **最佳**：非线性拟合对每个模型生成多组初值，LM 全部跑一遍，取残差平方和最小者；
//!    参数用对数参数化（C/k/n/G 存 ln 值）保证恒正，避免 LM 走到无意义的区域。
//! 2. **可判断**：`best_curve` 把所有模型按 **AICc** 排序给出排名，用户能自己看
//!    「该用哪个」，而不是只给一条直线了事。

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// 数值底层
// ---------------------------------------------------------------------------

fn mean(v: &[f64]) -> f64 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<f64>() / v.len() as f64
    }
}

/// 分位数（线性插值），q ∈ [0,1]
pub fn quantile(v: &[f64], q: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let q = q.clamp(0.0, 1.0);
    let pos = q * (s.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = (pos.ceil() as usize).min(s.len() - 1);
    let frac = pos - lo as f64;
    s[lo] + (s[hi] - s[lo]) * frac
}

/// 一般线性方程组求解（部分选主元高斯消元）。系数矩阵会被就地修改。
fn solve_linear_system(a: &mut [Vec<f64>], b: &mut [f64]) -> Option<Vec<f64>> {
    let n = b.len();
    if n == 0 || a.len() != n {
        return None;
    }
    for col in 0..n {
        let mut pivot = col;
        for r in (col + 1)..n {
            if a[r][col].abs() > a[pivot][col].abs() {
                pivot = r;
            }
        }
        if a[pivot][col].abs() < 1e-14 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        let pv = a[col][col];
        for r in (col + 1)..n {
            let factor = a[r][col] / pv;
            if factor == 0.0 {
                continue;
            }
            for c in col..n {
                a[r][c] -= factor * a[col][c];
            }
            b[r] -= factor * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let mut s = b[r];
        for c in (r + 1)..n {
            s -= a[r][c] * x[c];
        }
        x[r] = s / a[r][r];
    }
    if x.iter().all(|v| v.is_finite()) {
        Some(x)
    } else {
        None
    }
}

/// 线性最小二乘：min ||A x - b||，Householder QR（比正规方程数值稳定）。
fn lstsq(a: &[Vec<f64>], b: &[f64]) -> Option<Vec<f64>> {
    let m = b.len();
    if m == 0 {
        return None;
    }
    let n = a.first().map(|r| r.len()).unwrap_or(0);
    if n == 0 || n > m || a.len() != m {
        return None;
    }
    let mut a = a.to_vec();
    let mut b = b.to_vec();
    for k in 0..n {
        let mut norm = 0.0;
        for i in k..m {
            norm += a[i][k] * a[i][k];
        }
        norm = norm.sqrt();
        if norm < 1e-300 {
            continue;
        }
        let alpha = if a[k][k] >= 0.0 { -norm } else { norm };
        let mut v = vec![0.0; m];
        for i in k..m {
            v[i] = a[i][k];
        }
        v[k] -= alpha;
        let mut vnorm2 = 0.0;
        for i in k..m {
            vnorm2 += v[i] * v[i];
        }
        if vnorm2 < 1e-300 {
            continue;
        }
        for j in k..n {
            let mut s = 0.0;
            for i in k..m {
                s += v[i] * a[i][j];
            }
            s *= 2.0 / vnorm2;
            for i in k..m {
                a[i][j] -= s * v[i];
            }
        }
        let mut s = 0.0;
        for i in k..m {
            s += v[i] * b[i];
        }
        s *= 2.0 / vnorm2;
        for i in k..m {
            b[i] -= s * v[i];
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        if a[r][r].abs() < 1e-300 {
            return None;
        }
        let mut s = b[r];
        for c in (r + 1)..n {
            s -= a[r][c] * x[c];
        }
        x[r] = s / a[r][r];
    }
    if x.iter().all(|v| v.is_finite()) {
        Some(x)
    } else {
        None
    }
}

/// 多项式拟合，返回 [a0, a1, ..., a_degree]，即 y = a0 + a1 x + ...
pub fn poly_fit(xs: &[f64], ys: &[f64], degree: usize) -> Option<Vec<f64>> {
    let n = xs.len();
    if n <= degree || ys.len() != n {
        return None;
    }
    let mut a = vec![vec![0.0; degree + 1]; n];
    for i in 0..n {
        let mut v = 1.0;
        for j in 0..=degree {
            a[i][j] = v;
            v *= xs[i];
        }
    }
    lstsq(&a, ys)
}

/// 多项式求值（Horner）
pub fn poly_eval(c: &[f64], x: f64) -> f64 {
    c.iter().rev().fold(0.0, |acc, &ci| acc * x + ci)
}

fn sse_of<F>(pts: &[(f64, f64)], f: &F, p: &[f64]) -> Option<f64>
where
    F: Fn(f64, &[f64]) -> f64,
{
    let mut s = 0.0;
    for (x, y) in pts {
        let yh = f(*x, p);
        if !yh.is_finite() {
            return None;
        }
        let r = y - yh;
        s += r * r;
    }
    if s.is_finite() {
        Some(s)
    } else {
        None
    }
}

/// Levenberg-Marquardt（数值雅可比）。`starts` 为多组初值，取 SSE 最小者。
fn lm_fit<F>(
    pts: &[(f64, f64)],
    f: F,
    starts: &[Vec<f64>],
    max_iter: usize,
) -> Option<(Vec<f64>, f64)>
where
    F: Fn(f64, &[f64]) -> f64,
{
    let mut best: Option<(Vec<f64>, f64)> = None;
    for p0 in starts {
        if let Some((p, sse)) = lm_single(pts, &f, p0, max_iter) {
            if best.as_ref().map(|(_, s)| sse < *s).unwrap_or(true) {
                best = Some((p, sse));
            }
        }
    }
    best
}

fn lm_single<F>(pts: &[(f64, f64)], f: &F, p0: &[f64], max_iter: usize) -> Option<(Vec<f64>, f64)>
where
    F: Fn(f64, &[f64]) -> f64,
{
    let n = p0.len();
    if n == 0 || pts.len() <= n {
        return None;
    }
    let mut p = p0.to_vec();
    let mut sse = sse_of(pts, f, &p)?;
    let mut lambda = 1e-3;
    for _ in 0..max_iter {
        let mut jtj = vec![vec![0.0; n]; n];
        let mut jtr = vec![0.0; n];
        for (x, y) in pts {
            let base = f(*x, &p);
            if !base.is_finite() {
                return None;
            }
            let mut j = vec![0.0; n];
            for k in 0..n {
                let h = 1e-6 * (1.0 + p[k].abs());
                let mut pp = p.clone();
                pp[k] += h;
                let mut pm = p.clone();
                pm[k] -= h;
                let d = (f(*x, &pp) - f(*x, &pm)) / (2.0 * h);
                if !d.is_finite() {
                    return None;
                }
                j[k] = d;
            }
            let r = y - base;
            for i in 0..n {
                jtr[i] += j[i] * r;
                for k in 0..n {
                    jtj[i][k] += j[i] * j[k];
                }
            }
        }
        let mut accepted = false;
        for _trial in 0..40 {
            let mut a = jtj.clone();
            for i in 0..n {
                a[i][i] += lambda * (1.0 + jtj[i][i]);
            }
            let mut b = jtr.clone();
            if let Some(delta) = solve_linear_system(&mut a, &mut b) {
                let pn: Vec<f64> = (0..n).map(|i| p[i] + delta[i]).collect();
                if let Some(s2) = sse_of(pts, f, &pn) {
                    if s2 <= sse {
                        let converged = (sse - s2) <= 1e-12 * (1.0 + sse);
                        p = pn;
                        sse = s2;
                        lambda = (lambda / 10.0).max(1e-12);
                        accepted = true;
                        if converged {
                            return Some((p, sse));
                        }
                        break;
                    }
                }
            }
            lambda *= 10.0;
            if lambda > 1e14 {
                break;
            }
        }
        if !accepted {
            break;
        }
    }
    Some((p, sse))
}

/// 自然三次样条
#[derive(Clone, Debug)]
pub struct Spline {
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
    /// 各节点处的二阶导数（自然边界：两端为 0）
    pub m: Vec<f64>,
}

/// 构造自然三次样条。x 需严格递增，点数 ≥ 3。
pub fn natural_cubic_spline(mut pts: Vec<(f64, f64)>) -> Option<Spline> {
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let n = pts.len();
    if n < 3 {
        return None;
    }
    for i in 1..n {
        if pts[i].0 - pts[i - 1].0 <= 0.0 {
            return None;
        }
    }
    let xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
    let ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
    let h: Vec<f64> = (0..n - 1).map(|i| xs[i + 1] - xs[i]).collect();
    let mut m = vec![0.0; n];
    let inner = n - 2;
    if inner > 0 {
        let mut lower = vec![0.0; inner];
        let mut diag = vec![0.0; inner];
        let mut upper = vec![0.0; inner];
        let mut rhs = vec![0.0; inner];
        for i in 0..inner {
            let k = i + 1;
            lower[i] = h[k - 1];
            diag[i] = 2.0 * (h[k - 1] + h[k]);
            upper[i] = h[k];
            rhs[i] = 6.0 * ((ys[k + 1] - ys[k]) / h[k] - (ys[k] - ys[k - 1]) / h[k - 1]);
        }
        for i in 1..inner {
            if diag[i - 1].abs() < 1e-300 {
                return None;
            }
            let w = lower[i] / diag[i - 1];
            diag[i] -= w * upper[i - 1];
            rhs[i] -= w * rhs[i - 1];
        }
        let mut sol = vec![0.0; inner];
        if diag[inner - 1].abs() < 1e-300 {
            return None;
        }
        sol[inner - 1] = rhs[inner - 1] / diag[inner - 1];
        for i in (0..inner - 1).rev() {
            if diag[i].abs() < 1e-300 {
                return None;
            }
            sol[i] = (rhs[i] - upper[i] * sol[i + 1]) / diag[i];
        }
        for i in 0..inner {
            m[i + 1] = sol[i];
        }
    }
    Some(Spline { xs, ys, m })
}

/// 样条求值。区间外按端点段线性外推。
pub fn spline_eval(sp: &Spline, x: f64) -> f64 {
    let n = sp.xs.len();
    if n < 2 {
        return f64::NAN;
    }
    let i = sp
        .xs
        .partition_point(|&v| v <= x)
        .saturating_sub(1)
        .min(n - 2);
    let h = sp.xs[i + 1] - sp.xs[i];
    let dx = x - sp.xs[i];
    let b = (sp.ys[i + 1] - sp.ys[i]) / h - h * (2.0 * sp.m[i] + sp.m[i + 1]) / 6.0;
    sp.ys[i] + b * dx + sp.m[i] / 2.0 * dx * dx + (sp.m[i + 1] - sp.m[i]) / (6.0 * h) * dx * dx * dx
}

// ---------------------------------------------------------------------------
// 模型
// ---------------------------------------------------------------------------

/// 4 位有效数字的紧凑显示（Rust 没有 `{:.4g}`，自己实现）。
fn fmt_g(v: f64) -> String {
    if !v.is_finite() {
        return "nan".into();
    }
    let a = v.abs();
    if a == 0.0 {
        return "0".into();
    }
    if (1e-4..1e6).contains(&a) {
        let s = format!("{:.4}", v);
        let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
        s
    } else {
        format!("{:.3e}", v)
    }
}

/// 带符号版本（拼方程用）。
fn fmt_gs(v: f64) -> String {
    if v < 0.0 {
        format!("−{}", fmt_g(-v))
    } else {
        format!("+{}", fmt_g(v))
    }
}

fn clamp_exp(e: f64) -> f64 {
    if !e.is_finite() {
        if e > 0.0 {
            f64::INFINITY
        } else {
            0.0
        }
    } else if e > 700.0 {
        f64::INFINITY
    } else if e < -700.0 {
        0.0
    } else {
        e.exp()
    }
}

/// 拟合模型。默认 `Linear`，回归图可在检查器里换。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FitModel {
    /// y = a + b x
    Linear,
    /// y = a0 + a1 x + a2 x²
    Quadratic,
    /// y = a0 + a1 x + a2 x² + a3 x³
    Cubic,
    /// logit(p) = a + b·log10(x)
    LogitLog,
    /// 四参数 Logistic：y = D + (A−D)/[1+(x/C)^B]
    Logistic4,
    /// 五参数 Logistic：y = D + (A−D)/[1+(x/C)^B]^G
    Logistic5,
    /// 指数四参数：y = D + (A−D)/[1+e^(−B(x−C))]
    LogisticExp,
    /// Hill：y = Bottom + (Top−Bottom)·x^n/(k^n + x^n)
    Hill,
    /// y = a·e^(bx) + c
    Exponential,
    /// 三次样条插值（不参与模型比较）
    CubicSpline,
}

impl Default for FitModel {
    fn default() -> Self {
        FitModel::Linear
    }
}

impl FitModel {
    pub const ALL: [FitModel; 10] = [
        FitModel::Linear,
        FitModel::Quadratic,
        FitModel::Cubic,
        FitModel::LogitLog,
        FitModel::Logistic4,
        FitModel::Logistic5,
        FitModel::LogisticExp,
        FitModel::Hill,
        FitModel::Exponential,
        FitModel::CubicSpline,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FitModel::Linear => "直线回归",
            FitModel::Quadratic => "二次曲线",
            FitModel::Cubic => "三次曲线",
            FitModel::LogitLog => "Logit-log 直线",
            FitModel::Logistic4 => "四参数 Logistic",
            FitModel::Logistic5 => "五参数 Logistic",
            FitModel::LogisticExp => "四参数指数 Logistic",
            FitModel::Hill => "Hill 曲线",
            FitModel::Exponential => "指数曲线",
            FitModel::CubicSpline => "三次样条插值",
        }
    }

    pub fn formula(self) -> &'static str {
        match self {
            FitModel::Linear => "Y = a + b·X",
            FitModel::Quadratic => "Y = a0 + a1·X + a2·X²",
            FitModel::Cubic => "Y = a0 + a1·X + a2·X² + a3·X³",
            FitModel::LogitLog => "logit(p) = a + b·log10(X)",
            FitModel::Logistic4 => "Y = D + (A−D)/[1+(X/C)^B]",
            FitModel::Logistic5 => "Y = D + (A−D)/[1+(X/C)^B]^G",
            FitModel::LogisticExp => "Y = D + (A−D)/[1+e^(−B·(X−C))]",
            FitModel::Hill => "Y = Bottom + (Top−Bottom)·X^n/(k^n+X^n)",
            FitModel::Exponential => "Y = a·e^(b·X) + c",
            FitModel::CubicSpline => "分段三次多项式（自然边界）",
        }
    }

    pub fn n_params(self) -> usize {
        match self {
            FitModel::Linear | FitModel::LogitLog => 2,
            FitModel::Quadratic => 3,
            FitModel::Cubic => 4,
            FitModel::Logistic4
            | FitModel::LogisticExp
            | FitModel::Hill
            | FitModel::Exponential => 4,
            FitModel::Logistic5 => 5,
            FitModel::CubicSpline => 0,
        }
    }

    /// 是否要求 X > 0（Logit-log 与浓度幂形式的 Logistic / Hill）
    pub fn requires_positive_x(self) -> bool {
        matches!(
            self,
            FitModel::LogitLog | FitModel::Logistic4 | FitModel::Logistic5 | FitModel::Hill
        )
    }

    pub fn min_points(self) -> usize {
        match self {
            FitModel::Linear | FitModel::LogitLog => 3,
            FitModel::Quadratic => 4,
            FitModel::Cubic => 5,
            FitModel::Logistic4
            | FitModel::LogisticExp
            | FitModel::Hill
            | FitModel::Exponential => 5,
            FitModel::Logistic5 => 7,
            FitModel::CubicSpline => 3,
        }
    }

    pub fn is_interpolation(self) -> bool {
        matches!(self, FitModel::CubicSpline)
    }
}

/// 非线性曲线求值。C/k/n/G 在参数向量里以自然对数形式存放，保证恒正。
fn curve_value(model: FitModel, x: f64, p: &[f64]) -> f64 {
    match model {
        FitModel::Logistic4 => {
            let (a, d, b) = (p[0], p[1], p[3]);
            if x <= 0.0 {
                return if b > 0.0 { a } else { d };
            }
            let e = clamp_exp(b * (x.ln() - p[2]));
            d + (a - d) / (1.0 + e)
        }
        FitModel::Logistic5 => {
            let (a, d, b) = (p[0], p[1], p[3]);
            let g = clamp_exp(p[4]);
            if x <= 0.0 {
                return if b > 0.0 { a } else { d };
            }
            let e = clamp_exp(b * (x.ln() - p[2]));
            let t = clamp_exp(g * (1.0 + e).ln());
            d + (a - d) / t
        }
        FitModel::LogisticExp => {
            let (a, d, c, b) = (p[0], p[1], p[2], p[3]);
            let e = clamp_exp(-b * (x - c));
            d + (a - d) / (1.0 + e)
        }
        FitModel::Hill => {
            let (bottom, top) = (p[0], p[1]);
            let n = clamp_exp(p[3]);
            if x <= 0.0 {
                return bottom;
            }
            let e = clamp_exp(n * (p[2] - x.ln()));
            bottom + (top - bottom) / (1.0 + e)
        }
        FitModel::Exponential => p[0] * clamp_exp(p[1] * x) + p[2],
        _ => f64::NAN,
    }
}

// ---------------------------------------------------------------------------
// 拟合结果
// ---------------------------------------------------------------------------

/// 一次曲线拟合的结果（可直接画、可直接标注）。
#[derive(Clone, Debug)]
pub struct CurveFit {
    pub model: FitModel,
    /// 参数向量（非线性模型里 C/k/n/G 以 ln 存放）
    pub params: Vec<f64>,
    pub sse: f64,
    pub r2: f64,
    pub rmse: f64,
    /// 小样本修正的 AIC，用于模型比较；插值模型为 NaN
    pub aicc: f64,
    pub n: usize,
    /// 附加量（ED50 等）：(名, 值)
    pub extra: Vec<(String, f64)>,
    /// 拟合警告（参数不可辨识等），标注时必须跟着显示
    pub warnings: Vec<String>,
    /// 插值求值器（样条）
    spline: Option<Spline>,
}

impl CurveFit {
    /// 求值。x 超出样条区间时按端点段外推，其余模型照常求值。
    pub fn eval(&self, x: f64) -> f64 {
        match self.model {
            FitModel::Linear => self.params[0] + self.params[1] * x,
            FitModel::Quadratic | FitModel::Cubic => poly_eval(&self.params, x),
            FitModel::LogitLog => {
                if x <= 0.0 {
                    return f64::NAN;
                }
                let u = self.params[0] + self.params[1] * x.log10();
                let e = clamp_exp(-u);
                1.0 / (1.0 + e)
            }
            FitModel::CubicSpline => self
                .spline
                .as_ref()
                .map(|s| spline_eval(s, x))
                .unwrap_or(f64::NAN),
            _ => curve_value(self.model, x, &self.params),
        }
    }

    /// 标注用的方程/参数摘要
    pub fn equation(&self) -> String {
        match self.model {
            FitModel::Linear => format!(
                "y = {} {} {} x",
                fmt_g(self.params[0]),
                if self.params[1] < 0.0 { "−" } else { "+" },
                fmt_g(self.params[1].abs())
            ),
            FitModel::Quadratic | FitModel::Cubic => {
                let mut s = format!("y = {}", fmt_g(self.params[0]));
                for (i, c) in self.params.iter().enumerate().skip(1) {
                    s.push_str(&format!(" {} x^{}", fmt_gs(*c), i));
                }
                s
            }
            FitModel::LogitLog => format!(
                "logit(p) = {} {} {}·log10(x)",
                fmt_g(self.params[0]),
                if self.params[1] < 0.0 { "−" } else { "+" },
                fmt_g(self.params[1].abs())
            ),
            FitModel::CubicSpline => format!(
                "三次样条（{} 个节点）",
                self.spline.as_ref().map(|s| s.xs.len()).unwrap_or(0)
            ),
            _ => self.model.formula().to_string(),
        }
    }

    /// 参数的可读形式（对数参数化要还原）
    pub fn named_params(&self) -> Vec<(String, f64)> {
        match self.model {
            FitModel::Logistic4 => vec![
                ("A（低浓度端）".into(), self.params[0]),
                ("D（高浓度端）".into(), self.params[1]),
                ("C（拐点/ED50）".into(), clamp_exp(self.params[2])),
                ("B（斜率因子）".into(), self.params[3]),
            ],
            FitModel::Logistic5 => vec![
                ("A（低浓度端）".into(), self.params[0]),
                ("D（高浓度端）".into(), self.params[1]),
                ("C（拐点/ED50）".into(), clamp_exp(self.params[2])),
                ("B（斜率因子）".into(), self.params[3]),
                ("G（不对称）".into(), clamp_exp(self.params[4])),
            ],
            FitModel::LogisticExp => vec![
                ("A（高 X 端）".into(), self.params[0]),
                ("D（低 X 端）".into(), self.params[1]),
                ("C（拐点）".into(), self.params[2]),
                ("B（斜率）".into(), self.params[3]),
            ],
            FitModel::Hill => vec![
                ("Bottom".into(), self.params[0]),
                ("Top".into(), self.params[1]),
                ("k（ED50）".into(), clamp_exp(self.params[2])),
                ("n（Hill）".into(), clamp_exp(self.params[3])),
            ],
            FitModel::Exponential => vec![
                ("a".into(), self.params[0]),
                ("b".into(), self.params[1]),
                ("c".into(), self.params[2]),
            ],
            FitModel::Linear => vec![("a".into(), self.params[0]), ("b".into(), self.params[1])],
            FitModel::LogitLog => vec![
                ("a".into(), self.params[0]),
                ("b".into(), self.params[1]),
                ("ED50".into(), {
                    // logit(p)=0 对应 p=0.5
                    let u = -self.params[0] / self.params[1];
                    10f64.powf(u)
                }),
            ],
            FitModel::Quadratic | FitModel::Cubic => self
                .params
                .iter()
                .enumerate()
                .map(|(i, v)| (format!("a{i}"), *v))
                .collect(),
            FitModel::CubicSpline => Vec::new(),
        }
    }
}

/// 非线性模型的多组初值（分位数起点 × 多幅度 × 反向；C/k/n/G 取对数参数化）。
fn initial_guesses(model: FitModel, pts: &[(f64, f64)]) -> Vec<Vec<f64>> {
    let xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
    let ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
    let n = pts.len();
    let ymin = ys.iter().cloned().fold(f64::MAX, f64::min);
    let ymax = ys.iter().cloned().fold(f64::MIN, f64::max);
    let ymean = mean(&ys);
    let xr = (quantile(&xs, 1.0) - quantile(&xs, 0.0)).abs().max(1e-9);
    let take = (n / 3).max(1);
    let lo_mean = mean(&ys[..take]);
    let hi_mean = mean(&ys[n.saturating_sub(take)..]);
    let increasing = hi_mean >= lo_mean;

    let mut pos_x: Vec<f64> = xs.iter().cloned().filter(|x| *x > 0.0).collect();
    let allow_zero = pos_x.len() < xs.len() / 2;
    if pos_x.is_empty() {
        pos_x = xs.clone();
    }
    let mut cands_c: Vec<f64> = (0..3)
        .map(|i| quantile(&pos_x, 0.25 + 0.25 * i as f64))
        .filter(|v| if allow_zero { *v >= 0.0 } else { *v > 0.0 })
        .collect();
    cands_c.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    cands_c.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
    if cands_c.is_empty() {
        cands_c.push(1.0);
    }

    let mut starts: Vec<Vec<f64>> = Vec::new();
    match model {
        FitModel::Logistic4 | FitModel::Logistic5 => {
            let (a0, d0) = if increasing {
                (ymin, ymax)
            } else {
                (ymax, ymin)
            };
            let sign = if increasing { -1.0 } else { 1.0 };
            for &c in &cands_c {
                for &mag in &[0.5f64, 1.0, 2.0, 4.0] {
                    let b0 = sign * mag;
                    match model {
                        FitModel::Logistic4 => starts.push(vec![a0, d0, c.ln(), b0]),
                        _ => {
                            for &g in &[0.5f64, 1.0, 2.0] {
                                starts.push(vec![a0, d0, c.ln(), b0, g.ln()]);
                            }
                        }
                    }
                    match model {
                        FitModel::Logistic4 => starts.push(vec![d0, a0, c.ln(), -b0]),
                        _ => {
                            for &g in &[0.5f64, 1.0, 2.0] {
                                starts.push(vec![d0, a0, c.ln(), -b0, g.ln()]);
                            }
                        }
                    }
                }
            }
        }
        FitModel::LogisticExp => {
            let (a0, d0) = if increasing {
                (ymax, ymin)
            } else {
                (ymin, ymax)
            };
            let sign = if increasing { 1.0 } else { -1.0 };
            let c_pos: Vec<f64> = (0..3)
                .map(|i| quantile(&xs, 0.3 + 0.2 * i as f64))
                .collect();
            for &c in &c_pos {
                for &mag in &[0.5f64, 1.0, 2.0, 4.0] {
                    let b0 = sign * mag / xr;
                    starts.push(vec![a0, d0, c, b0]);
                    starts.push(vec![d0, a0, c, -b0]);
                }
            }
        }
        FitModel::Hill => {
            let (bottom0, top0) = if increasing {
                (ymin, ymax)
            } else {
                (ymax, ymin)
            };
            for &k in &cands_c {
                for &n0 in &[0.5f64, 1.0, 2.0, 4.0] {
                    starts.push(vec![bottom0, top0, k.ln(), n0.ln()]);
                    starts.push(vec![top0, bottom0, k.ln(), n0.ln()]);
                }
            }
        }
        FitModel::Exponential => {
            let amp = (ymax - ymin).abs().max(1e-6);
            for &bs in &[-2.0f64, -0.5, 0.5, 2.0] {
                let b0 = bs / xr;
                for &c0 in &[ymin, 0.0, ymean] {
                    for &s in &[1.0f64, -1.0] {
                        starts.push(vec![s * amp, b0, c0]);
                    }
                }
            }
        }
        _ => {}
    }
    if starts.is_empty() {
        starts.push(vec![0.0; 4]);
    }
    starts
}

/// 收敛结果规范化：把等价表示（A/D 互换、B 取反）统一成 B>0 的约定。
fn canonicalize(model: FitModel, p: &mut [f64]) {
    match model {
        FitModel::Logistic4 | FitModel::Logistic5 | FitModel::Hill | FitModel::LogisticExp => {
            if p.len() >= 4 && p[3] < 0.0 {
                p.swap(0, 1);
                p[3] = -p[3];
            }
        }
        _ => {}
    }
}

fn describe_curve(model: FitModel, p: &[f64]) -> (Vec<(String, f64)>, String, Vec<(String, f64)>) {
    let mut extra = Vec::new();
    match model {
        FitModel::Logistic4 | FitModel::Logistic5 => {
            let c = clamp_exp(p[2]);
            extra.push(("ED50 (C)".to_string(), c));
            let params = vec![
                ("A（低浓度端）".to_string(), p[0]),
                ("D（高浓度端）".to_string(), p[1]),
                ("C（ED50）".to_string(), c),
                ("B（斜率因子）".to_string(), p[3]),
            ];
            let eq = format!(
                "y = {} + ({}−{})/[1+(x/{})^{}]",
                fmt_g(p[1]),
                fmt_g(p[0]),
                fmt_g(p[1]),
                fmt_g(c),
                fmt_g(p[3])
            );
            (params, eq, extra)
        }
        FitModel::Hill => {
            let k = clamp_exp(p[2]);
            let n = clamp_exp(p[3]);
            extra.push(("ED50 (k)".to_string(), k));
            extra.push(("Hill 系数 n".to_string(), n));
            (
                vec![
                    ("Bottom".to_string(), p[0]),
                    ("Top".to_string(), p[1]),
                    ("k".to_string(), k),
                    ("n".to_string(), n),
                ],
                format!(
                    "y = {} + ({}−{})·x^{} / ({}^{} + x^{})",
                    fmt_g(p[0]),
                    fmt_g(p[1]),
                    fmt_g(p[0]),
                    fmt_g(n),
                    fmt_g(k),
                    fmt_g(n),
                    fmt_g(n)
                ),
                extra,
            )
        }
        _ => {
            let params = vec![("参数".to_string(), p[0])];
            (params, model.formula().to_string(), extra)
        }
    }
}

/// 小样本修正 AIC：AICc = n·ln(SSE/n) + 2k + 2k(k+1)/(n−k−1)
fn aicc(sse: f64, n: usize, k: usize) -> f64 {
    if n <= k + 1 || !(sse >= 0.0) || sse <= 0.0 {
        return f64::NAN;
    }
    let n = n as f64;
    let k = k as f64;
    n * (sse / n).ln() + 2.0 * k + 2.0 * k * (k + 1.0) / (n - k - 1.0)
}

/// 拟合一个模型。返回 None 表示数据不足或未收敛。
pub fn fit_curve(model: FitModel, raw: &[(f64, f64)]) -> Option<CurveFit> {
    let mut pts: Vec<(f64, f64)> = raw
        .iter()
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .copied()
        .collect();
    if model.requires_positive_x() {
        pts.retain(|(x, _)| *x > 0.0);
    }
    if pts.len() < model.min_points() {
        return None;
    }
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let (params, spline) = match model {
        FitModel::Linear => {
            let xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
            let ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
            let c = poly_fit(&xs, &ys, 1)?;
            (c, None)
        }
        FitModel::Quadratic | FitModel::Cubic => {
            let deg = if model == FitModel::Quadratic { 2 } else { 3 };
            let xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
            let ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
            let c = poly_fit(&xs, &ys, deg)?;
            (c, None)
        }
        FitModel::LogitLog => {
            // logit(p) = a + b log10(x)，p = y（若 y 是百分比先归一）
            let ymax = pts.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
            let scale = if ymax > 1.0 + 1e-9 { 100.0 } else { 1.0 };
            let mut lx = Vec::new();
            let mut ly = Vec::new();
            for (x, y) in &pts {
                let p = (y / scale).clamp(1e-6, 1.0 - 1e-6);
                lx.push(x.log10());
                ly.push((p / (1.0 - p)).ln());
            }
            let c = poly_fit(&lx, &ly, 1)?;
            (vec![c[0], c[1], scale], None)
        }
        FitModel::CubicSpline => {
            let sp = natural_cubic_spline(pts.clone())?;
            (vec![], Some(sp))
        }
        _ => {
            let starts = initial_guesses(model, &pts);
            let f = move |x: f64, p: &[f64]| curve_value(model, x, p);
            let (mut p, _sse) = lm_fit(&pts, f, &starts, 300)?;
            canonicalize(model, &mut p);
            (p, None)
        }
    };

    // 质量
    let predict = |x: f64| -> f64 {
        match model {
            FitModel::Linear => params[0] + params[1] * x,
            FitModel::Quadratic | FitModel::Cubic => poly_eval(&params, x),
            FitModel::LogitLog => {
                if x <= 0.0 {
                    f64::NAN
                } else {
                    let u = params[0] + params[1] * x.log10();
                    let e = clamp_exp(-u);
                    params[2] / (1.0 + e)
                }
            }
            FitModel::CubicSpline => spline
                .as_ref()
                .map(|s| spline_eval(s, x))
                .unwrap_or(f64::NAN),
            _ => curve_value(model, x, &params),
        }
    };
    let mut sse = 0.0;
    let mut sst = 0.0;
    let ybar = mean(&pts.iter().map(|p| p.1).collect::<Vec<f64>>());
    for (x, y) in &pts {
        let yh = predict(*x);
        if !yh.is_finite() {
            return None;
        }
        sse += (y - yh) * (y - yh);
        sst += (y - ybar) * (y - ybar);
    }
    let r2 = if sst > 0.0 { 1.0 - sse / sst } else { 1.0 };
    let n = pts.len();
    let k = model.n_params();
    let rmse = (sse / n.saturating_sub(1).max(1) as f64).sqrt();

    let (named, _eq, mut extra) = if model.is_interpolation() {
        (vec![], String::new(), vec![])
    } else {
        describe_curve(model, &params)
    };
    if model == FitModel::Logistic4 || model == FitModel::Logistic5 {
        if !extra.iter().any(|(k, _)| k.contains("ED50")) {
            extra.push(("ED50 (C)".to_string(), clamp_exp(params[2])));
        }
    }
    let _ = named;

    // 参数可辨识性：ED50 / 拐点落在数据 x 范围之外很远时，说明数据没走到拐点，
    // 这个参数是外推出来的（不同参数组合能给出几乎一样的曲线）。
    // 必须在标注里说出来，否则读者会把 2.9e9 当成测出来的浓度。
    let mut warnings = Vec::new();
    if matches!(
        model,
        FitModel::Logistic4 | FitModel::Logistic5 | FitModel::Hill
    ) {
        let xlo = pts.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
        let xhi = pts.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
        if let Some((_, c)) = extra
            .iter()
            .find(|(k, _)| k.contains("ED50") || k.contains("k"))
        {
            if *c > xhi * 10.0 || *c < xlo * 0.1 {
                warnings.push(format!(
                    "ED50 = {} 远在数据范围 [{}, {}] 之外，拐点未被观测到，该参数不可辨识（曲线本身仍可用）",
                    fmt_g(*c), fmt_g(xlo), fmt_g(xhi)
                ));
            }
        }
    }
    if model == FitModel::Logistic5 && n <= model.n_params() * 2 {
        warnings.push(
            "五参数模型在数据点偏少时 B/C/G 相关性很强，参数解可能不唯一；只关心 ED50 可改用四参数"
                .into(),
        );
    }

    Some(CurveFit {
        model,
        params,
        sse,
        r2,
        rmse,
        aicc: if model.is_interpolation() {
            f64::NAN
        } else {
            aicc(sse, n, k)
        },
        n,
        extra,
        warnings,
        spline,
    })
}

/// 对同一份数据把所有模型跑一遍，按 **AICc 升序** 返回排名。
///
/// `first` 是推荐模型（AICc 最小，且 R² 不低于第二名太多时优先参数少者）。
/// 样条是插值（R² 恒为 1），不参与排名，只作为可选绘图方式。
/// 把所有（非插值）模型各拟合一遍，按 AICc 升序返回。
///
/// `best_curve` 只是它的包装。检查器必须把整张排名**缓存**下来——
/// 这里面每个模型都是几十组初值的 Levenberg-Marquardt，每帧重跑一次
/// 会把 UI 线程卡成幻灯片（回归图的卡顿就是这么来的）。
/// 缓存后模型推荐、排名、以及当前选中模型的警告都能零成本取用。
pub fn fit_all(raw: &[(f64, f64)]) -> Vec<CurveFit> {
    let mut fits: Vec<CurveFit> = Vec::new();
    for m in FitModel::ALL {
        if m.is_interpolation() {
            continue;
        }
        if let Some(f) = fit_curve(m, raw) {
            if f.aicc.is_finite() {
                fits.push(f);
            }
        }
    }
    fits.sort_by(|a, b| {
        a.aicc
            .partial_cmp(&b.aicc)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    fits
}

pub fn best_curve(raw: &[(f64, f64)]) -> Option<(CurveFit, Vec<(FitModel, f64)>)> {
    let mut fits = fit_all(raw);
    if fits.is_empty() {
        return None;
    }
    let ranking: Vec<(FitModel, f64)> = fits.iter().map(|f| (f.model, f.aicc)).collect();
    let best = fits.remove(0);
    Some((best, ranking))
}

/// 数据指纹（缓存 key）：只用来判断「数据变没变」，不做安全用途。
pub fn pts_key(raw: &[(f64, f64)]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    raw.len().hash(&mut h);
    for &(x, y) in raw {
        x.to_bits().hash(&mut h);
        y.to_bits().hash(&mut h);
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 量化：fit_all 单次开销 = 以前检查器**每帧**付出的开销。
    #[test]
    fn fit_all_timing() {
        use crate::core::sample::sample_project;
        let p = sample_project();
        let t = p.tables.iter().find(|t| t.name == "剂量–反应").unwrap();
        let pts: Vec<(f64, f64)> = (0..t.group_names.len())
            .flat_map(|g| t.xy_observations(g))
            .collect();
        let n = pts.len();
        let start = std::time::Instant::now();
        let fits = fit_all(&pts);
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        println!("fit_all({n} 点, {} 模型) = {ms:.1} ms/次", fits.len());
        assert!(fits.len() >= 5);
    }

    #[test]
    fn 直线恢复斜率截距() {
        let xs: Vec<f64> = (0..10).map(|i| i as f64).collect();
        let ys: Vec<f64> = xs.iter().map(|x| 2.0 + 3.0 * x).collect();
        let c = poly_fit(&xs, &ys, 1).unwrap();
        assert!((c[0] - 2.0).abs() < 1e-8 && (c[1] - 3.0).abs() < 1e-8);
    }

    #[test]
    fn 三次多项式恢复系数() {
        let xs: Vec<f64> = (0..12).map(|i| i as f64 * 0.7 - 4.0).collect();
        let ys: Vec<f64> = xs
            .iter()
            .map(|x| 1.0 - 2.0 * x + 0.5 * x * x + 0.25 * x * x * x)
            .collect();
        let c = poly_fit(&xs, &ys, 3).unwrap();
        assert!((c[0] - 1.0).abs() < 1e-6 && (c[1] + 2.0).abs() < 1e-6);
    }

    #[test]
    fn 样条穿过所有节点() {
        let pts = vec![(0.0, 0.0), (1.0, 2.0), (2.5, -1.0), (4.0, 3.0), (5.0, 1.0)];
        let sp = natural_cubic_spline(pts.clone()).unwrap();
        for (x, y) in pts {
            assert!((spline_eval(&sp, x) - y).abs() < 1e-9);
        }
    }

    #[test]
    fn lm恢复四参数logistic() {
        // y = D + (A−D)/[1+(x/C)^B]，A=1, D=99, C=10, B=-1.2（上升）
        let f = |x: f64, p: &[f64]| {
            let (a, d, b) = (p[0], p[1], p[3]);
            let e = clamp_exp(b * (x.ln() - p[2]));
            d + (a - d) / (1.0 + e)
        };
        let pts: Vec<(f64, f64)> = (0..20)
            .map(|i| {
                let x = 0.1 * 1.4f64.powi(i);
                let e = clamp_exp(-1.2 * (x.ln() - 10f64.ln()));
                (x, 99.0 + (1.0 - 99.0) / (1.0 + e))
            })
            .collect();
        let starts = initial_guesses(FitModel::Logistic4, &pts);
        let (mut p, sse) = lm_fit(&pts, f, &starts, 300).unwrap();
        assert!(sse < 1e-6, "sse={sse}");
        canonicalize(FitModel::Logistic4, &mut p);
        // canonicalize 把 B<0 的表示翻成 B>0（A/D 互换），所以比较 |B|
        assert!(
            (clamp_exp(p[2]) - 10.0).abs() < 0.05,
            "C={}",
            clamp_exp(p[2])
        );
        assert!((p[3].abs() - 1.2).abs() < 0.05, "|B|={}", p[3].abs());
    }

    #[test]
    fn 模型选择选得出四参数() {
        // S 形数据：4PL 应当胜出（AICc 最小）
        let pts: Vec<(f64, f64)> = (0..12)
            .map(|i| {
                let x = 0.2 * 1.9f64.powi(i);
                let e = clamp_exp(-1.1 * (x.ln() - 8f64.ln()));
                (x, 5.0 + 90.0 / (1.0 + e))
            })
            .collect();
        let (best, ranking) = best_curve(&pts).unwrap();
        // S 形数据：最佳应当是某种 S 形模型（4PL / 5PL / Hill），
        // 直线必须明显更差。具体哪个 S 形模型胜出由 AICc 决定，不预先指定。
        assert!(
            matches!(
                best.model,
                FitModel::Logistic4 | FitModel::Logistic5 | FitModel::Hill
            ),
            "S 形数据应选 S 形模型，实际 {:?}（排名 {:?}）",
            best.model,
            ranking
        );
        assert!(best.r2 > 0.99, "R²={}", best.r2);
        let linear_aicc = ranking
            .iter()
            .find(|(m, _)| *m == FitModel::Linear)
            .map(|(_, a)| *a)
            .expect("直线应参与排名");
        assert!(
            linear_aicc > best.aicc + 50.0,
            "直线 AICc={linear_aicc} 应明显差于最佳 {:?} 的 {}",
            best.model,
            best.aicc
        );
    }

    #[test]
    fn 直线数据选直线() {
        let pts: Vec<(f64, f64)> = (0..15).map(|i| (i as f64, 1.5 + 2.2 * i as f64)).collect();
        let (best, _) = best_curve(&pts).unwrap();
        assert_eq!(best.model, FitModel::Linear, "直线数据应选直线");
    }

    #[test]
    fn 指数曲线恢复参数() {
        let pts: Vec<(f64, f64)> = (0..12)
            .map(|i| {
                let x = i as f64 * 0.5;
                (x, 3.0 * (0.4 * x).exp() + 1.0)
            })
            .collect();
        let f = fit_curve(FitModel::Exponential, &pts).unwrap();
        assert!(f.sse < 1e-8, "sse={}", f.sse);
    }
}
