//! 数据变换与归一化工具。
//!
//! 常见科研数据变换：对数、平方根、倒数、Box-Cox、Z-score、
//! Min-Max、百分比对照、秩变换。

/// 变换类型。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Transform {
    /// 不变换
    None,
    /// 自然对数 ln(x)
    Log,
    /// 以 10 为底 log10(x)
    Log10,
    /// 平方根 sqrt(x)
    Sqrt,
    /// 倒数 1/x
    Reciprocal,
    /// 平方 x²
    Square,
    /// Z-score 标准化 (x - mean) / sd
    ZScore,
    /// Min-Max 归一化到 [0, 1]
    MinMax,
    /// 百分比对照：除以第一列均值 × 100
    PercentOfControl,
    /// 秩变换（1..n）
    Rank,
    /// Box-Cox 变换（需 lambda）
    BoxCox(f64),
}

impl Transform {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "不变换",
            Self::Log => "ln(x)",
            Self::Log10 => "log10(x)",
            Self::Sqrt => "√x",
            Self::Reciprocal => "1/x",
            Self::Square => "x²",
            Self::ZScore => "Z-score",
            Self::MinMax => "Min-Max 归一化",
            Self::PercentOfControl => "百分比对照",
            Self::Rank => "秩变换",
            Self::BoxCox(_) => "Box-Cox",
        }
    }
}

/// 对一列数据施加变换。空值保持不变。
pub fn transform_col(xs: &[f64], t: Transform) -> Vec<f64> {
    match t {
        Transform::None => xs.to_vec(),
        Transform::Log => xs
            .iter()
            .map(|&x| if x > 0.0 { x.ln() } else { f64::NAN })
            .collect(),
        Transform::Log10 => xs
            .iter()
            .map(|&x| if x > 0.0 { x.log10() } else { f64::NAN })
            .collect(),
        Transform::Sqrt => xs
            .iter()
            .map(|&x| if x >= 0.0 { x.sqrt() } else { f64::NAN })
            .collect(),
        Transform::Reciprocal => xs
            .iter()
            .map(|&x| if x != 0.0 { 1.0 / x } else { f64::NAN })
            .collect(),
        Transform::Square => xs.iter().map(|&x| x * x).collect(),
        Transform::ZScore => {
            let m = crate::core::stats::mean(xs);
            let s = crate::core::stats::sd(xs);
            if s == 0.0 || s.is_nan() {
                return xs.to_vec();
            }
            xs.iter().map(|&x| (x - m) / s).collect()
        }
        Transform::MinMax => {
            let lo = xs.iter().cloned().fold(f64::INFINITY, f64::min);
            let hi = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let range = hi - lo;
            if range == 0.0 {
                return vec![0.5; xs.len()];
            }
            xs.iter().map(|&x| (x - lo) / range).collect()
        }
        Transform::PercentOfControl => {
            // 控制组 = 均值（调用方传入 control_mean）
            // 这里简化：用当前列均值做对照
            let m = crate::core::stats::mean(xs);
            if m == 0.0 {
                return xs.to_vec();
            }
            xs.iter().map(|&x| x / m * 100.0).collect()
        }
        Transform::Rank => {
            let mut indexed: Vec<(usize, f64)> =
                xs.iter().enumerate().map(|(i, &x)| (i, x)).collect();
            indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            let mut result = vec![0.0; xs.len()];
            for (rank, (idx, _)) in indexed.iter().enumerate() {
                result[*idx] = (rank + 1) as f64;
            }
            result
        }
        Transform::BoxCox(lambda) => xs
            .iter()
            .map(|&x| {
                if lambda.abs() < 1e-10 {
                    if x > 0.0 {
                        x.ln()
                    } else {
                        f64::NAN
                    }
                } else if x > 0.0 {
                    (x.powf(lambda) - 1.0) / lambda
                } else {
                    f64::NAN
                }
            })
            .collect(),
    }
}

/// 百分比对照：相对于指定控制组均值。
pub fn percent_of_control(xs: &[f64], control_mean: f64) -> Vec<f64> {
    if control_mean == 0.0 {
        return xs.to_vec();
    }
    xs.iter().map(|&x| x / control_mean * 100.0).collect()
}

/// 自动选择最优 Box-Cox lambda（最大似然，网格搜索）。
pub fn boxcox_optimal_lambda(xs: &[f64]) -> f64 {
    let valid: Vec<f64> = xs.iter().cloned().filter(|&x| x > 0.0).collect();
    if valid.len() < 3 {
        return 1.0;
    }
    let n = valid.len() as f64;
    let mut best_lambda = 1.0;
    let mut best_ll = f64::NEG_INFINITY;

    for i in -20..=20 {
        let lambda = i as f64 * 0.1;
        let transformed: Vec<f64> = valid
            .iter()
            .map(|&x| {
                if lambda.abs() < 1e-10 {
                    x.ln()
                } else {
                    (x.powf(lambda) - 1.0) / lambda
                }
            })
            .collect();
        let _m = crate::core::stats::mean(&transformed);
        let var = crate::core::stats::variance(&transformed);
        if var <= 0.0 {
            continue;
        }
        // 对数似然（近似）
        let ll =
            -n / 2.0 * (var).ln() + (lambda - 1.0) * valid.iter().map(|&x| x.ln()).sum::<f64>();
        if ll > best_ll {
            best_ll = ll;
            best_lambda = lambda;
        }
    }
    best_lambda
}

/// 转置矩阵（行列互换）。用于「转置表」操作。
pub fn transpose(matrix: &[Vec<f64>]) -> Vec<Vec<f64>> {
    if matrix.is_empty() {
        return vec![];
    }
    let ncols = matrix[0].len();
    (0..ncols)
        .map(|c| matrix.iter().map(|row| row[c]).collect())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 对数变换() {
        let xs = vec![1.0, 10.0, 100.0];
        let out = transform_col(&xs, Transform::Log10);
        assert!((out[0] - 0.0).abs() < 1e-10);
        assert!((out[1] - 1.0).abs() < 1e-10);
        assert!((out[2] - 2.0).abs() < 1e-10);
    }

    #[test]
    fn zscore标准化() {
        let xs = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let out = transform_col(&xs, Transform::ZScore);
        let m = out.iter().sum::<f64>() / out.len() as f64;
        assert!(m.abs() < 1e-10, "均值应为 0");
        let var = out.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (out.len() - 1) as f64;
        assert!((var.sqrt() - 1.0).abs() < 1e-10, "标准差应为 1");
    }

    #[test]
    fn minmax归一化() {
        let xs = vec![10.0, 20.0, 30.0];
        let out = transform_col(&xs, Transform::MinMax);
        assert!((out[0] - 0.0).abs() < 1e-10);
        assert!((out[1] - 0.5).abs() < 1e-10);
        assert!((out[2] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn 秩变换() {
        let xs = vec![30.0, 10.0, 20.0];
        let out = transform_col(&xs, Transform::Rank);
        assert_eq!(out, vec![3.0, 1.0, 2.0]);
    }

    #[test]
    fn 转置() {
        let m = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        let t = transpose(&m);
        assert_eq!(t, vec![vec![1.0, 3.0], vec![2.0, 4.0]]);
    }

    #[test]
    fn boxcox最优lambda() {
        // 指数数据 → log 变换最优（lambda≈0）
        let xs: Vec<f64> = (1..=20).map(|i| (i as f64 * 0.5).exp()).collect();
        let lam = boxcox_optimal_lambda(&xs);
        assert!(lam.abs() < 0.5, "指数数据 lambda 应接近 0, got {lam}");
    }
}
