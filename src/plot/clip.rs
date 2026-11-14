//! 绘图区裁剪：任何超出坐标框的几何都必须在进入场景前被切掉。
//!
//! 学术图里「线/面溢出坐标框」是最常见的硬伤：小提琴尾巴压到刻度、
//! 面积图盖住坐标轴标题、置信带跑到图外。这里统一用解析裁剪解决，
//! 而不是靠「让数据恰好落在范围内」。

use crate::plot::scene::{Pt, Rect};

/// 点是否在矩形内（含边界，带一点容差以免浮点抖动）。
#[inline]
fn inside(p: Pt, edge: usize, r: Rect) -> bool {
    const E: f32 = 1e-4;
    match edge {
        0 => p.x >= r.x - E,
        1 => p.x <= r.right() + E,
        2 => p.y >= r.y - E,
        _ => p.y <= r.bottom() + E,
    }
}

/// 线段与矩形边界的交点（调用前已知一端在内、一端在外）。
fn intersect(a: Pt, b: Pt, edge: usize, r: Rect) -> Pt {
    let (t, ok) = match edge {
        0 => (((r.x - a.x) / (b.x - a.x)), (b.x - a.x).abs() > 1e-9),
        1 => (((r.right() - a.x) / (b.x - a.x)), (b.x - a.x).abs() > 1e-9),
        2 => (((r.y - a.y) / (b.y - a.y)), (b.y - a.y).abs() > 1e-9),
        _ => (((r.bottom() - a.y) / (b.y - a.y)), (b.y - a.y).abs() > 1e-9),
    };
    if !ok || !t.is_finite() {
        return a;
    }
    a.lerp(b, t.clamp(0.0, 1.0))
}

/// 多边形裁剪（Sutherland–Hodgman，凸裁剪窗口）。
///
/// 输入是任意简单多边形（小提琴、置信带、面积填充都是），输出仍是一个
/// 闭合多边形；完全在框外时返回空。
pub fn polygon(poly: &[Pt], r: Rect) -> Vec<Pt> {
    if poly.len() < 3 {
        return Vec::new();
    }
    let mut out: Vec<Pt> = poly.to_vec();
    let mut buf: Vec<Pt> = Vec::with_capacity(out.len() + 4);
    for edge in 0..4 {
        if out.is_empty() {
            return Vec::new();
        }
        buf.clear();
        let n = out.len();
        for i in 0..n {
            let cur = out[i];
            let prev = out[(i + n - 1) % n];
            let cur_in = inside(cur, edge, r);
            let prev_in = inside(prev, edge, r);
            if cur_in {
                if !prev_in {
                    buf.push(intersect(prev, cur, edge, r));
                }
                buf.push(cur);
            } else if prev_in {
                buf.push(intersect(prev, cur, edge, r));
            }
        }
        std::mem::swap(&mut out, &mut buf);
    }
    // 去掉退化点：连续重复会让描边出现断点。
    out.dedup_by(|a, b| (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4);
    if out.len() < 3 {
        out.clear();
    }
    out
}

/// 线段裁剪（Liang–Barsky）：完全在外返回 `None`。
pub fn segment(r: Rect, a: Pt, b: Pt) -> Option<(Pt, Pt)> {
    if !a.x.is_finite() || !a.y.is_finite() || !b.x.is_finite() || !b.y.is_finite() {
        return None;
    }
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let p = [-dx, dx, -dy, dy];
    let q = [a.x - r.x, r.right() - a.x, a.y - r.y, r.bottom() - a.y];
    let mut t0 = 0.0_f32;
    let mut t1 = 1.0_f32;
    for i in 0..4 {
        if p[i].abs() < 1e-9 {
            if q[i] < 0.0 {
                return None;
            }
        } else {
            let t = q[i] / p[i];
            if p[i] < 0.0 {
                if t > t1 {
                    return None;
                }
                t0 = t0.max(t);
            } else {
                if t < t0 {
                    return None;
                }
                t1 = t1.min(t);
            }
        }
    }
    Some((
        Pt::new(a.x + t0 * dx, a.y + t0 * dy),
        Pt::new(a.x + t1 * dx, a.y + t1 * dy),
    ))
}

/// 折线裁剪：按「进入 / 离开」切成若干段可见折线。
///
/// 返回的每一段至少两个点，可直接描边；用于折线、阶梯线、KDE 轮廓。
pub fn polyline(pts: &[Pt], r: Rect) -> Vec<Vec<Pt>> {
    let mut runs: Vec<Vec<Pt>> = Vec::new();
    let mut cur: Vec<Pt> = Vec::new();
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        match segment(r, a, b) {
            Some((ca, cb)) => {
                // 上一段裁完的终点与这一段的起点不重合 → 断开成新的一段
                let contiguous = cur
                    .last()
                    .map(|l| (l.x - ca.x).abs() <= 1e-3 && (l.y - ca.y).abs() <= 1e-3)
                    .unwrap_or(false);
                if !contiguous {
                    if cur.len() >= 2 {
                        runs.push(std::mem::take(&mut cur));
                    }
                    cur.clear();
                    cur.push(ca);
                }
                cur.push(cb);
            }
            None => {
                if cur.len() >= 2 {
                    runs.push(std::mem::take(&mut cur));
                }
                cur.clear();
            }
        }
    }
    if cur.len() >= 2 {
        runs.push(cur);
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r() -> Rect {
        Rect::new(0.0, 0.0, 100.0, 100.0)
    }

    #[test]
    fn polygon_inside_is_unchanged() {
        let poly = vec![
            Pt::new(10.0, 10.0),
            Pt::new(90.0, 10.0),
            Pt::new(90.0, 90.0),
            Pt::new(10.0, 90.0),
        ];
        let out = polygon(&poly, r());
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn polygon_is_clipped_to_bounds() {
        // 上下都超出：小提琴尾巴的典型情形。
        let poly = vec![
            Pt::new(50.0, -40.0),
            Pt::new(70.0, -10.0),
            Pt::new(50.0, 140.0),
            Pt::new(30.0, -10.0),
        ];
        let out = polygon(&poly, r());
        assert!(!out.is_empty());
        for p in &out {
            assert!(p.x >= -0.01 && p.x <= 100.01, "{p:?}");
            assert!(p.y >= -0.01 && p.y <= 100.01, "{p:?}");
        }
    }

    #[test]
    fn polygon_outside_vanishes() {
        let poly = vec![
            Pt::new(200.0, 200.0),
            Pt::new(300.0, 200.0),
            Pt::new(300.0, 300.0),
        ];
        assert!(polygon(&poly, r()).is_empty());
    }

    #[test]
    fn segment_clip_known() {
        let a = Pt::new(-10.0, 50.0);
        let b = Pt::new(110.0, 50.0);
        let (ca, cb) = segment(r(), a, b).unwrap();
        assert!((ca.x - 0.0).abs() < 1e-3);
        assert!((cb.x - 100.0).abs() < 1e-3);
        assert!(segment(r(), Pt::new(200.0, 0.0), Pt::new(200.0, 50.0)).is_none());
    }

    #[test]
    fn polyline_splits_into_runs() {
        // 出去再回来，应切成两段。
        let pts = [
            Pt::new(10.0, 10.0),
            Pt::new(50.0, 200.0),
            Pt::new(90.0, 10.0),
        ];
        let runs = polyline(&pts, r());
        assert_eq!(runs.len(), 2, "{runs:?}");
        for run in &runs {
            assert!(run.len() >= 2);
            for p in run {
                assert!(p.y <= 100.01);
            }
        }
    }
}
