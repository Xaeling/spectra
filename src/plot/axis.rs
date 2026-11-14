//! 坐标轴：线性 / 对数、反向、1-2-5 取整刻度。

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Axis {
    pub min: f64,
    pub max: f64,
    pub log: bool,
    pub reverse: bool,
}

#[derive(Clone, Debug)]
pub struct Tick {
    pub value: f64,
    /// 0..1（未反向的归一化位置）
    pub norm: f64,
    pub label: String,
    pub major: bool,
}

impl Axis {
    pub fn linear(min: f64, max: f64) -> Self {
        Self {
            min,
            max,
            log: false,
            reverse: false,
        }
    }

    pub fn from_values(
        values: impl IntoIterator<Item = f64>,
        log: bool,
        include_zero: bool,
    ) -> Self {
        let vals: Vec<f64> = values
            .into_iter()
            .filter(|v| v.is_finite() && (!log || *v > 0.0))
            .collect();
        if vals.is_empty() {
            return if log {
                Self {
                    min: 0.1,
                    max: 10.0,
                    log: true,
                    reverse: false,
                }
            } else {
                Self::linear(0.0, 1.0)
            };
        }
        let mut lo = vals.iter().copied().fold(f64::INFINITY, f64::min);
        let mut hi = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if log {
            let (llo, lhi) = (lo.log10(), hi.log10());
            let span = (lhi - llo).max(0.08);
            let pad = span * 0.05 + 0.02;
            lo = 10f64.powf(llo - pad);
            hi = 10f64.powf(lhi + pad);
        } else {
            let data_min = lo;
            if include_zero {
                lo = lo.min(0.0);
            }
            if (hi - lo).abs() < 1e-12 {
                let d = hi.abs().max(1.0) * 0.15;
                lo -= d;
                hi += d;
            } else {
                let span = hi - lo;
                let pad = span * 0.06;
                lo -= pad;
                hi += pad;
                if include_zero && data_min >= 0.0 {
                    lo = 0.0;
                }
            }
        }
        Self {
            min: lo,
            max: hi,
            log,
            reverse: false,
        }
    }

    pub fn norm(&self, v: f64) -> f64 {
        if !v.is_finite() {
            return f64::NAN;
        }
        let n = if self.log {
            if v <= 0.0 || self.min <= 0.0 {
                return f64::NAN;
            }
            (v.log10() - self.min.log10()) / (self.max.log10() - self.min.log10())
        } else {
            (v - self.min) / (self.max - self.min)
        };
        if self.reverse {
            1.0 - n
        } else {
            n
        }
    }

    pub fn value_at(&self, norm: f64) -> f64 {
        let n = if self.reverse { 1.0 - norm } else { norm };
        if self.log {
            10f64.powf(self.min.log10() + n * (self.max.log10() - self.min.log10()))
        } else {
            self.min + n * (self.max - self.min)
        }
    }

    pub fn ticks(&self, target: usize) -> Vec<Tick> {
        if self.log {
            self.log_ticks(target)
        } else {
            self.linear_ticks(target)
        }
    }

    fn linear_ticks(&self, target: usize) -> Vec<Tick> {
        let span = self.max - self.min;
        if !(span > 0.0) || !span.is_finite() {
            return Vec::new();
        }
        let raw = span / target.max(2) as f64;
        let step = nice_step(raw);
        let dec = decimals_for_step(step);
        let mut ticks = Vec::new();
        let start = (self.min / step).ceil();
        let end = (self.max / step).floor();
        if end - start > 200.0 {
            return Vec::new();
        }
        let mut i = start;
        while i <= end {
            let v = i * step;
            let mut vv = v;
            if vv.abs() < step * 1e-9 {
                vv = 0.0;
            }
            let label = format_tick(vv, dec, step, span);
            ticks.push(Tick {
                value: vv,
                norm: self.norm(vv),
                label,
                major: true,
            });
            i += 1.0;
        }
        ticks
    }

    fn log_ticks(&self, target: usize) -> Vec<Tick> {
        if self.min <= 0.0 || self.max <= 0.0 {
            return Vec::new();
        }
        let lo = self.min.log10().floor() as i32;
        let hi = self.max.log10().ceil() as i32;
        let decades = (hi - lo).max(1);
        let label_all = decades <= 2;
        let mut ticks = Vec::new();
        for e in lo..=hi {
            let base = 10f64.powi(e);
            for m in 1..=9 {
                let v = base * m as f64;
                if v < self.min * 0.999 || v > self.max * 1.001 {
                    continue;
                }
                let major = m == 1 || (label_all && (m == 2 || m == 5));
                if !major && (target < 6 || decades > 6) {
                    continue;
                }
                let label = if major {
                    format_log_tick(v, e, m)
                } else {
                    String::new()
                };
                ticks.push(Tick {
                    value: v,
                    norm: self.norm(v),
                    label,
                    major,
                });
            }
        }
        ticks
    }

    pub fn reversed(mut self) -> Self {
        self.reverse = !self.reverse;
        self
    }
}

pub fn nice_step(raw: f64) -> f64 {
    if !(raw > 0.0) || !raw.is_finite() {
        return 1.0;
    }
    let exp = raw.log10().floor();
    let base = 10f64.powf(exp);
    let f = raw / base;
    let nf = if f <= 1.5 {
        1.0
    } else if f <= 3.5 {
        2.0
    } else if f <= 7.5 {
        5.0
    } else {
        10.0
    };
    nf * base
}

pub fn decimals_for_step(step: f64) -> usize {
    if !(step > 0.0) {
        return 2;
    }
    let d = -step.log10().floor();
    d.clamp(0.0, 8.0) as usize
}

fn format_tick(v: f64, dec: usize, step: f64, span: f64) -> String {
    let abs = v.abs();
    // 很大 / 很小的数字用科学计数法，避免刻度标签过长。
    if abs >= 1e6 || (abs > 0.0 && abs < 1e-4) {
        let s = format!("{v:.3e}");
        if let Some((m, e)) = s.split_once('e') {
            let m = m.trim_end_matches('0').trim_end_matches('.');
            let e: i32 = e.parse().unwrap_or(0);
            return format!("{m}e{e}");
        }
        return s;
    }
    let mut s = format!("{v:.dec$}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" {
        s = "0".into();
    }
    // 防止 0.30000000000000004 这类浮点噪声：span 很小时多留一位
    if !s.contains('.') && span < 1e-9 && step < 1e-9 {
        s = format!("{v:.6}");
    }
    s
}

fn format_log_tick(v: f64, exp: i32, m: i32) -> String {
    if m == 1 {
        if (-3..=4).contains(&exp) {
            if exp >= 0 {
                format!("{}", 10i64.pow(exp as u32))
            } else {
                let mut s = format!("{v}");
                while s.len() > 2 && s.ends_with('0') {
                    s.pop();
                }
                s
            }
        } else {
            format!("1e{exp}")
        }
    } else if (-2..=3).contains(&exp) {
        let mut s = format!("{v}");
        while s.contains('.') && s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
        s
    } else {
        format!("{m}e{exp}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_steps() {
        assert_eq!(nice_step(0.9), 1.0);
        assert_eq!(nice_step(1.3), 1.0);
        assert_eq!(nice_step(3.0), 2.0);
        assert_eq!(nice_step(7.0), 5.0);
        assert!((nice_step(0.021) - 0.02).abs() < 1e-12);
    }

    #[test]
    fn linear_ticks_cover_range() {
        let a = Axis::linear(0.0, 42.0);
        let ticks = a.ticks(6);
        assert!(ticks.len() >= 5);
        assert!(ticks.iter().any(|t| (t.value - 0.0).abs() < 1e-9));
        assert!(ticks.iter().any(|t| (t.value - 40.0).abs() < 1e-9));
        assert_eq!(ticks[1].label, "5");
    }

    #[test]
    fn log_ticks_decades() {
        let a = Axis {
            min: 0.1,
            max: 100.0,
            log: true,
            reverse: false,
        };
        let ticks = a.ticks(6);
        let majors: Vec<&str> = ticks
            .iter()
            .filter(|t| t.major)
            .map(|t| t.label.as_str())
            .collect();
        assert_eq!(majors, vec!["0.1", "1", "10", "100"]);
        assert!((a.norm(1.0) - 1.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn auto_pads() {
        let a = Axis::from_values([1.0, 2.0, 3.0], false, false);
        assert!(a.min < 1.0 && a.max > 3.0);
        let b = Axis::from_values([1.0, 2.0, 3.0], false, true);
        assert_eq!(b.min, 0.0);
    }
}
