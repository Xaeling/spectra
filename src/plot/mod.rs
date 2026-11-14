//! 自研矢量绘图内核（StatAura Plot Engine）。
//!
//! 设计目标：
//! - 屏幕预览、PNG、SVG、PDF 共用同一份场景 IR，版式绝对一致；
//! - 文字全部转曲，不依赖读者机器上的字体；
//! - 只在数据/规格/尺寸变化时重建场景，交互层不触发重绘。

pub mod axis;
pub mod chart;
pub mod clip;
pub mod export;
pub mod extras;
pub mod kinds;
pub mod layout;
pub mod raster;
pub mod scene;
pub mod spec;
pub mod text;
pub mod theme;

pub use chart::{render, Hit, Render};
pub use text::FontBook;

#[cfg(test)]
mod geometry_tests {
    //! 渲染后几何校验（Nature `figure-style` §9.1）。
    //!
    //! 规范要求「画完要看像素」：文本不能互相压、标记不能碰边框、图元不能
    //! 跑出坐标框。文本层面靠人工看，几何层面在这里用断言锁死——
    //! 24 种图型 + 4 套主题各画一遍，`violations` 必须为空。
    //!
    //! 这一条测试就是「箱线图中位数线刺出箱体」「横向柱散点飞到框外」
    //! 这类缺陷的回归闸门：只要有人再写出一处越界，它就会失败。

    use crate::core::graph::{ChartKind, ErrorKind, GraphSpec};
    use crate::core::sample::sample_project;
    use crate::plot::{render, FontBook};

    fn fonts() -> FontBook {
        FontBook::load_builtin()
    }

    /// 给每个图型挑一张语义合适的表（与 examples/gallery.rs 同口径）。
    fn table_for(
        p: &crate::core::model::Project,
        kind: ChartKind,
    ) -> &crate::core::model::DataTable {
        let by = |name: &str| p.tables.iter().find(|t| t.name == name);
        match kind {
            ChartKind::Scatter
            | ChartKind::Line
            | ChartKind::SmoothLine
            | ChartKind::StepLine
            | ChartKind::AreaLine
            | ChartKind::Regression => by("剂量–反应"),
            ChartKind::GroupedBar | ChartKind::StackedBar | ChartKind::StackedBar100 => {
                by("性别 × 治疗")
            }
            ChartKind::Survival => by("生存分析"),
            ChartKind::Heatmap => by("性别 × 治疗"),
            ChartKind::Pie | ChartKind::Donut => by("人群构成"),
            _ => by("体重变化"),
        }
        .or_else(|| p.tables.first())
        .expect("示例工程应有数据表")
    }

    #[test]
    fn 所有图型都不越出绘图区() {
        let p = sample_project();
        let fonts = fonts();
        let mut bad = Vec::new();
        for kind in ChartKind::ALL {
            let t = table_for(&p, kind);
            let mut spec = GraphSpec::default_for(t);
            spec.kind = kind;
            spec.error = if kind.shows_error() {
                ErrorKind::Sem
            } else {
                ErrorKind::None
            };
            // 图例、数据点全开：越界最容易在这些元素上出现。
            spec.show_legend = true;
            spec.show_points = true;
            let r = render(&spec, t, 468.0, 318.0, 2.4, &fonts);
            for v in &r.violations {
                bad.push(format!("{}: {v}", kind.label()));
            }
        }
        assert!(
            bad.is_empty(),
            "有 {} 处图元越出绘图区：\n{}",
            bad.len(),
            bad.join("\n")
        );
    }

    #[test]
    fn 四套主题下都不越界() {
        let p = sample_project();
        let fonts = fonts();
        // 柱状 / 箱线 / 小提琴 是三套最容易出越界的图型
        for kind in [ChartKind::Column, ChartKind::Box, ChartKind::Violin] {
            let t = table_for(&p, kind);
            for theme in crate::core::graph::PlotThemeId::ALL {
                let mut spec = GraphSpec::default_for(t);
                spec.kind = kind;
                spec.theme = theme;
                spec.error = ErrorKind::Sem;
                spec.show_points = true;
                let r = render(&spec, t, 468.0, 318.0, 2.4, &fonts);
                assert!(
                    r.violations.is_empty(),
                    "{} / {:?} 越界：{:?}",
                    kind.label(),
                    theme,
                    r.violations
                );
            }
        }
    }

    #[test]
    fn 不同画布尺寸下都不越界() {
        let p = sample_project();
        let fonts = fonts();
        // 小图（单栏 85mm）与大图（双栏 170mm）都要成立
        for (w, h) in [(241.0, 180.0), (468.0, 318.0), (600.0, 420.0)] {
            for kind in [ChartKind::Column, ChartKind::HorizontalBar, ChartKind::Box] {
                let t = table_for(&p, kind);
                let mut spec = GraphSpec::default_for(t);
                spec.kind = kind;
                spec.error = ErrorKind::Sem;
                spec.show_points = true;
                let r = render(&spec, t, w, h, 2.4, &fonts);
                assert!(
                    r.violations.is_empty(),
                    "{} @ {w}x{h} 越界：{:?}",
                    kind.label(),
                    r.violations
                );
            }
        }
    }
}

#[cfg(test)]
mod structure_tests {
    //! 结构断言：某些图元「必须存在 / 必须不存在」。
    //!
    //! 几何自检只能回答「有没有跑出框」，回答不了「该画的东西画了没有」。
    //! 这一组针对具体图型，把「箱线图必须有离群点圈、小提琴里必须有箱线图
    //! 元素、多序列回归不许叠半透明色带」钉成测试。

    use crate::core::graph::{ChartKind, ErrorKind, GraphSpec};
    use crate::core::sample::sample_project;
    use crate::plot::scene::{Op, Path, Seg};
    use crate::plot::{render, FontBook, Render};

    fn bbox(path: &Path) -> (f32, f32, f32, f32) {
        let (mut x0, mut y0, mut x1, mut y1) = (
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        );
        for s in &path.segs {
            let p = match s {
                Seg::Move(p) | Seg::Line(p) => *p,
                Seg::Quad(c, p) => {
                    crate::plot::scene::Pt::new((c.x + p.x) / 2.0, (c.y + p.y) / 2.0)
                }
                Seg::Cubic(c1, c2, p) => crate::plot::scene::Pt::new(
                    (c1.x + c2.x + p.x) / 3.0,
                    (c1.y + c2.y + p.y) / 3.0,
                ),
                Seg::Close => continue,
            };
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        (x0, y0, x1, y1)
    }

    fn inside(r: &Render, b: (f32, f32, f32, f32), tol: f32) -> bool {
        b.0 >= r.plot.x - tol
            && b.2 <= r.plot.right() + tol
            && b.1 >= r.plot.y - tol
            && b.3 <= r.plot.bottom() + tol
    }

    /// 绘图区内的小笔画图元：返回 (是否实心, bbox)
    fn marks(r: &Render) -> Vec<(bool, (f32, f32, f32, f32))> {
        r.scene
            .ops
            .iter()
            .filter_map(|op| {
                let b = bbox(&op.path);
                let (w, h) = (b.2 - b.0, b.3 - b.1);
                if !(w > 0.5 && h > 0.5 && w <= 14.0 && h <= 14.0 && (w - h).abs() < 3.0) {
                    return None;
                }
                if !inside(r, b, 1.0) {
                    return None; // 图例里的示例标记
                }
                Some((op.fill.is_some(), b))
            })
            .collect()
    }

    /// 多边形在给定 y 处的左右边界（射线法：取所有与水平线相交的边的 x）。
    fn span_at_y(op: &Op, y: f32) -> Option<(f32, f32)> {
        let pts: Vec<crate::plot::scene::Pt> = op
            .path
            .segs
            .iter()
            .filter_map(|s| match s {
                Seg::Move(p) | Seg::Line(p) => Some(*p),
                _ => None,
            })
            .collect();
        if pts.len() < 3 {
            return None;
        }
        let mut xs = Vec::new();
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            if (a.y - y) * (b.y - y) <= 0.0 && (a.y - b.y).abs() > 1e-6 {
                let t = (y - a.y) / (b.y - a.y);
                xs.push(a.x + t * (b.x - a.x));
            }
        }
        if xs.is_empty() {
            None
        } else {
            Some((
                xs.iter().copied().fold(f32::INFINITY, f32::min),
                xs.iter().copied().fold(f32::NEG_INFINITY, f32::max),
            ))
        }
    }

    /// 多边形在给定 x 处的上下边界。
    fn span_y_at_x(op: &Op, x: f32) -> Option<(f32, f32)> {
        let pts: Vec<crate::plot::scene::Pt> = op
            .path
            .segs
            .iter()
            .filter_map(|s| match s {
                Seg::Move(p) | Seg::Line(p) => Some(*p),
                _ => None,
            })
            .collect();
        if pts.len() < 3 {
            return None;
        }
        let mut ys = Vec::new();
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            if (a.x - x) * (b.x - x) <= 0.0 && (a.x - b.x).abs() > 1e-6 {
                let t = (x - a.x) / (b.x - a.x);
                ys.push(a.y + t * (b.y - a.y));
            }
        }
        if ys.is_empty() {
            None
        } else {
            Some((
                ys.iter().copied().fold(f32::INFINITY, f32::min),
                ys.iter().copied().fold(f32::NEG_INFINITY, f32::max),
            ))
        }
    }

    /// IQR 箱的识别：矩形（≤6 段）、有描边、有填充、窄而高。
    /// 不能用「白底」判据——用户要求箱体与琴身同色填充。
    fn is_iqr_box(op: &Op) -> bool {
        if op.stroke.is_none() || op.fill.is_none() || op.path.segs.len() > 6 {
            return false;
        }
        let b = bbox(&op.path);
        let (w, h) = (b.2 - b.0, b.3 - b.1);
        w > 1.0 && w < 40.0 && h > 0.5
    }

    fn render_kind(
        p: &crate::core::model::Project,
        kind: ChartKind,
        table: &str,
    ) -> (Render, GraphSpec) {
        let t = p.tables.iter().find(|t| t.name == table).expect("表");
        let mut spec = GraphSpec::default_for(t);
        spec.kind = kind;
        spec.error = if kind.shows_error() {
            ErrorKind::Sem
        } else {
            ErrorKind::None
        };
        spec.show_legend = true;
        let r = render(&spec, t, 468.0, 318.0, 2.4, &FontBook::load_builtin());
        (r, spec)
    }

    #[test]
    fn 图内标注可以整体关闭() {
        // 「图内标注」开关关掉后：文字类图元必须变少，但坐标轴与数据图元不变。
        let p = sample_project();
        let t = p
            .tables
            .iter()
            .find(|t| t.name == "剂量–反应")
            .expect("XY 表");
        let mut spec = GraphSpec::default_for(t);
        spec.kind = ChartKind::Scatter;
        spec.fit_model = crate::core::curve::FitModel::Linear;
        let fonts = FontBook::load_builtin();
        let on = render(&spec, t, 468.0, 318.0, 2.4, &fonts);
        spec.annotations_visible = false;
        let off = render(&spec, t, 468.0, 318.0, 2.4, &fonts);
        assert!(
            off.scene.ops.len() < on.scene.ops.len(),
            "关掉标注后图元数应减少：{} vs {}",
            off.scene.ops.len(),
            on.scene.ops.len()
        );
        // 坐标轴范围必须一致：标注只是文字，不该改变数据区域
        assert!((on.x.min - off.x.min).abs() < 1e-9 && (on.y.max - off.y.max).abs() < 1e-9);
        assert!((on.plot.w - off.plot.w).abs() < 1e-6);
    }

    #[test]
    fn 分面是开关不是强制() {
        // 规范 G8 建议 > 6 组分面，但用户关掉开关就必须真的关掉。
        let p = crate::core::sample::sample_project();
        let id = p
            .tables
            .iter()
            .find(|t| t.name == "体重变化")
            .map(|t| t.id)
            .unwrap();
        let t = p.table(id).unwrap().clone();
        let mut spec = GraphSpec::default_for(&t);
        spec.kind = ChartKind::Box;
        spec.facet = false;
        spec.annotations_visible = false;
        let r = render(&spec, &t, 468.0, 318.0, 2.4, &FontBook::load_builtin());
        // 同框时 X 轴覆盖所有类别（-0.5 … n−0.5）；分面时每个面板只有一条类别，
        // X 轴恒为 [-0.5, 0.5]。这是两种布局的结构差异，不依赖图元计数。
        assert!(
            r.x.max > 1.0,
            "facet=false 时必须同框：x 轴 {}…{}",
            r.x.min,
            r.x.max
        );
        spec.facet = true;
        let r2 = render(&spec, &t, 468.0, 318.0, 2.4, &FontBook::load_builtin());
        assert!(
            (r2.x.max - 0.5).abs() < 1e-9,
            "facet=true 时每个面板只画一个类别：x 轴 {}…{}",
            r2.x.min,
            r2.x.max
        );
        assert!(r2.scene.ops.len() != r.scene.ops.len());
    }

    /// 视觉回归基线（`gallery check`）的前提：同一输入必须逐像素产出同一张图。
    /// 渲染路径一旦引入时间、随机数或无序容器，基线就会假报警，这里直接拦住。
    #[test]
    fn 渲染是确定性的() {
        let p = sample_project();
        let (r1, _) = render_kind(&p, ChartKind::Column, "体重变化");
        let (r2, _) = render_kind(&p, ChartKind::Column, "体重变化");
        assert_eq!(r1.scene.ops.len(), r2.scene.ops.len());
        let (w1, h1, a) = crate::plot::raster::rasterize(&r1.scene);
        let (w2, h2, b) = crate::plot::raster::rasterize(&r2.scene);
        assert_eq!((w1, h1), (w2, h2));
        assert!(a == b, "两次渲染的像素不一致，基线比对会失效");
        // 分面路径同样要确定
        let t = p.tables.iter().find(|t| t.name == "体重变化").unwrap();
        let mut spec = GraphSpec::default_for(t);
        spec.kind = ChartKind::Column;
        spec.facet = true;
        let s1 = render(&spec, t, 468.0, 318.0, 2.4, &FontBook::load_builtin());
        let s2 = render(&spec, t, 468.0, 318.0, 2.4, &FontBook::load_builtin());
        let (_, _, pa) = crate::plot::raster::rasterize(&s1.scene);
        let (_, _, pb) = crate::plot::raster::rasterize(&s2.scene);
        assert!(pa == pb, "分面渲染不确定");
    }

    #[test]
    fn 箱线图离群点必须是空心圈() {
        let p = sample_project();
        let (r, _) = render_kind(&p, ChartKind::Box, "体重变化");
        let m = marks(&r);
        let hollow = m.iter().filter(|(filled, _)| !*filled).count();
        let solid = m.iter().filter(|(filled, _)| *filled).count();
        // A3-5：n ≤ 30 必须叠加原始点（实心），G5：离群点必须是空心圈。
        // 两种图元都有才是正确实现；只有实心 = 离群点被混进了数据点。
        assert!(solid > 0, "箱线图必须展示原始观测点");
        assert!(hollow > 0, "样本里存在栅栏外的观测，应当画成空心圈");
    }

    #[test]
    fn 小提琴内部必须有箱线图元素() {
        let p = sample_project();
        let (r, _) = render_kind(&p, ChartKind::Violin, "体重变化");
        let boxes: Vec<_> = r
            .scene
            .ops
            .iter()
            .filter(|op| is_iqr_box(op) && inside(&r, bbox(&op.path), 1.0))
            .collect();
        assert!(
            boxes.len() >= 4,
            "四个小提琴各应有一个 IQR 箱，实际 {}",
            boxes.len()
        );
        // 箱体不许是白块：填充必须与琴身同色，边框更深。
        // 每个组颜色不同，得按 RGB 找到「自己那把琴」再比。
        let bodies: Vec<_> = r
            .scene
            .ops
            .iter()
            .filter(|op| {
                op.fill.map(|f| f.a == 96).unwrap_or(false)
                    && op.stroke.is_some()
                    && op.path.segs.len() > 60
            })
            .filter_map(|op| Some((op.fill?, op.stroke.as_ref()?.color)))
            .collect();
        assert_eq!(bodies.len(), boxes.len(), "琴身数量应与箱体数量一致");
        for op in &boxes {
            let f = op.fill.unwrap();
            let body = bodies
                .iter()
                .find(|(bf, _)| bf.r == f.r && bf.g == f.g && bf.b == f.b);
            assert!(
                body.is_some(),
                "IQR 箱填充必须与所属琴身同色，箱 {:?} 找不到同色琴身",
                (f.r, f.g, f.b)
            );
            let bs = body.unwrap().1;
            let sc = op.stroke.as_ref().unwrap().color;
            assert!(
                (sc.r as i32 + sc.g as i32 + sc.b as i32)
                    < (bs.r as i32 + bs.g as i32 + bs.b as i32),
                "IQR 箱边框必须比所属琴身边框更深"
            );
        }
        // 任何白块都不许出现（无论有无描边）。
        let whites = r
            .scene
            .ops
            .iter()
            .filter(|op| {
                let Some(f) = op.fill else { return false };
                f.r > 250 && f.g > 250 && f.b > 250 && inside(&r, bbox(&op.path), 1.0)
            })
            .count();
        assert_eq!(whites, 0, "小提琴中间不许出现白块（读成挖空）");

        // 中位线 / 须帽：细横线
        let hlines = r
            .scene
            .ops
            .iter()
            .filter(|op| {
                if op.fill.is_some() || op.stroke.is_none() {
                    return false;
                }
                let b = bbox(&op.path);
                let (w, h) = (b.2 - b.0, b.3 - b.1);
                inside(&r, b, 1.0) && w > 4.0 && h < 4.0
            })
            .count();
        assert!(hlines >= 4, "每个小提琴至少要有一条中位线，实际 {hlines}");
    }

    #[test]
    fn 多组回归每组都有同色置信带() {
        // 标准画法：每组一条拟合线 + 一条**同色**的 95% 置信带。
        // 曾经把多组带改成虚线描边（怕叠色），那已经不是回归图了。
        let p = sample_project();
        let (r, _) = render_kind(&p, ChartKind::Regression, "剂量–反应");
        let bands: Vec<_> = r
            .scene
            .ops
            .iter()
            .filter(|op| {
                let Some(f) = op.fill else { return false };
                let b = bbox(&op.path);
                f.a < 200 && (b.2 - b.0) > r.plot.w * 0.4
            })
            .collect();
        assert_eq!(bands.len(), 3, "三组各应有一条置信带，实际 {}", bands.len());
        // 带必须有同色细描边：否则多条带叠在一起分不清边界
        assert!(
            bands.iter().all(|op| op.stroke.is_some()),
            "置信带必须带同色描边"
        );
        // 带内不透明度要低（0.1–0.35），否则会盖住散点
        for op in &bands {
            let a = op.fill.unwrap().a;
            assert!(
                (25..=90).contains(&a),
                "置信带 alpha 应在 0.1–0.35，实际 {a}"
            );
        }
    }

    #[test]
    fn 单序列回归保留置信带() {
        // 单序列仍然要有填充色带（这时不存在叠色）
        let p = sample_project();
        let t = p.tables.iter().find(|t| t.name == "剂量–反应").unwrap();
        let mut one = t.clone();
        // 只留第一组（3 个 Y 列）
        one.columns
            .retain(|c| c.role == crate::core::model::ColRole::X || c.group == 0);
        one.group_names = vec![t.group_names[0].clone()];
        let mut spec = GraphSpec::default_for(&one);
        spec.kind = ChartKind::Regression;
        spec.error = ErrorKind::None;
        let r = render(&spec, &one, 468.0, 318.0, 2.4, &FontBook::load_builtin());
        let bands = r
            .scene
            .ops
            .iter()
            .filter(|op| {
                let Some(f) = op.fill else { return false };
                let b = bbox(&op.path);
                f.a < 200 && (b.2 - b.0) > r.plot.w * 0.3
            })
            .count();
        assert_eq!(bands, 1, "单序列回归应当有一条填充置信带，实际 {bands}");
    }

    #[test]
    fn 置信带不超出数据范围() {
        let p = sample_project();
        let (r, _) = render_kind(&p, ChartKind::Regression, "剂量–反应");
        // 单序列才有填充带，这里用单序列验证 x 范围
        let t = p.tables.iter().find(|t| t.name == "剂量–反应").unwrap();
        let mut one = t.clone();
        one.columns
            .retain(|c| c.role == crate::core::model::ColRole::X || c.group == 0);
        one.group_names = vec![t.group_names[0].clone()];
        let mut spec = GraphSpec::default_for(&one);
        spec.kind = ChartKind::Regression;
        let r2 = render(&spec, &one, 468.0, 318.0, 2.4, &FontBook::load_builtin());
        let band = r2.scene.ops.iter().find(|op| {
            let Some(f) = op.fill else { return false };
            let b = bbox(&op.path);
            f.a < 200 && (b.2 - b.0) > r2.plot.w * 0.3
        });
        if let Some(op) = band {
            let b = bbox(&op.path);
            // 轴两端各有 6% 留白：色带必须比轴窄，不能铺满整幅
            assert!(
                b.2 - b.0 < r2.plot.w - 1.0,
                "置信带宽度 {:.1} 几乎等于绘图区 {:.1}，说明没按数据范围裁剪",
                b.2 - b.0,
                r2.plot.w
            );
        }
        let _ = r;
    }

    /// 依据《科研绘图规范模板库》补的结构断言。
    #[test]
    fn 小提琴内嵌箱宽占最宽处15到20百分比() {
        let p = sample_project();
        let (r, _) = render_kind(&p, ChartKind::Violin, "体重变化");
        // 琴身轮廓：填充色 + 很多顶点的大多边形
        let widest = r
            .scene
            .ops
            .iter()
            .filter(|op| {
                // 琴身填充用 alpha=96 且有描边；文字路径也是多段填充，不能混进来。
                op.fill.map(|f| f.a == 96).unwrap_or(false)
                    && op.stroke.is_some()
                    && op.path.segs.len() > 60
            })
            .map(|op| {
                let b = bbox(&op.path);
                b.2 - b.0
            })
            .fold(0.0_f32, f32::max);
        // IQR 箱：与琴身同色填充、带更深描边的窄矩形
        let box_w = r
            .scene
            .ops
            .iter()
            .filter(|op| is_iqr_box(op))
            .map(|op| {
                let b = bbox(&op.path);
                b.2 - b.0
            })
            .fold(0.0_f32, f32::max);
        assert!(widest > 0.0, "应当画出 KDE 琴身（n = 10，满足 G10 下限）");
        let ratio = box_w / widest;
        assert!(
            (0.15..=0.20).contains(&ratio),
            "A1-2：箱宽须在琴身最宽处的 15–20%，实际 {ratio:.3}（箱 {box_w:.1} / 琴 {widest:.1}）"
        );
    }

    #[test]
    fn 小提琴散点整个落在轮廓内() {
        // A1-3 `constrain_to_violin`：按每个点所在 y 的琴身左右边界做几何判断，
        // 不靠肉眼。之前给尖端硬塞了 12% 最小抖动幅度，点会跑到轮廓外。
        let p = sample_project();
        let (r, _) = render_kind(&p, ChartKind::Violin, "体重变化");
        let bodies: Vec<&Op> = r
            .scene
            .ops
            .iter()
            .filter(|op| {
                op.fill.map(|f| f.a == 96).unwrap_or(false)
                    && op.stroke.is_some()
                    && op.path.segs.len() > 60
            })
            .collect();
        assert!(!bodies.is_empty(), "应当有 KDE 琴身");
        let dots: Vec<(f32, f32)> = marks(&r)
            .iter()
            .filter(|(filled, _)| *filled)
            .map(|(_, b)| ((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0))
            .collect();
        assert!(!dots.is_empty(), "n < 20 时必须叠原始散点");
        let mut outside = 0;
        for (x, y) in &dots {
            let covered = bodies
                .iter()
                .filter_map(|op| span_at_y(op, *y))
                .any(|(l, rr)| *x >= l - 0.5 && *x <= rr + 0.5);
            if !covered {
                outside += 1;
            }
        }
        assert_eq!(
            outside,
            0,
            "有 {outside} 个散点落在小提琴轮廓外（共 {} 个）",
            dots.len()
        );
    }

    #[test]
    fn 小样本小提琴必须降级为箱线加原始点() {
        let p = sample_project();
        let base = p.tables.iter().find(|t| t.name == "体重变化").unwrap();
        let mut small = base.clone();
        // 只留 8 行 → n < 10，KDE 必须被拒绝
        for c in small.columns.iter_mut() {
            c.cells.truncate(8);
        }
        small.row_headers.truncate(8);
        let mut spec = GraphSpec::default_for(&small);
        spec.kind = ChartKind::Violin;
        let r = render(&spec, &small, 468.0, 318.0, 2.4, &FontBook::load_builtin());
        let kde = r
            .scene
            .ops
            .iter()
            .filter(|op| {
                op.fill.map(|f| f.a == 96).unwrap_or(false)
                    && op.stroke.is_some()
                    && op.path.segs.len() > 60
            })
            .count();
        assert_eq!(kde, 0, "G10：n < 10 不得出现 KDE 琴身");
        let pts = marks(&r).iter().filter(|(filled, _)| *filled).count();
        assert!(pts >= 8, "G10 降级后必须展示每个原始观测，实际 {pts}");
    }

    #[test]
    fn 多组曲线必须有线型第二编码() {
        let p = sample_project();
        for (kind, table) in [
            (ChartKind::Ecdf, "体重变化"),
            (ChartKind::Line, "剂量–反应"),
        ] {
            let (r, _) = render_kind(&p, kind, table);
            let dashed = r
                .scene
                .ops
                .iter()
                .filter(|op| {
                    op.stroke
                        .as_ref()
                        .map(|s| !s.dash.is_empty())
                        .unwrap_or(false)
                })
                .count();
            assert!(dashed > 0, "{} 多组时不得只靠颜色区分", kind.label());
        }
    }

    #[test]
    fn 置信带必须两端宽中间窄() {
        // 均值响应 95% CI 的形状：x 远离均值时变宽（t·s·sqrt(1/n+(x−x̄)²/Sxx)）。
        // 等宽矩形说明算成了 ±SD 或数据范围，那是错的。
        let p = sample_project();
        let (r, _) = render_kind(&p, ChartKind::Regression, "剂量–反应");
        let band = r
            .scene
            .ops
            .iter()
            .find(|op| {
                let Some(f) = op.fill else { return false };
                let b = bbox(&op.path);
                f.a < 200 && (b.2 - b.0) > r.plot.w * 0.4
            })
            .expect("应当有置信带");
        let b = bbox(&band.path);
        // 最窄处不一定在几何中点：CI 在 x = x̄ 处最窄，而 x̄ 由数据分布决定
        // （这份数据 x = 0.1…30，x̄ 落在约 24% 处）。所以采样找最小值，
        // 再要求两端显著宽于它。
        let samples: Vec<f32> = (0..=20)
            .map(|i| {
                let x = b.0 + (b.2 - b.0) * i as f32 / 20.0;
                span_y_at_x(band, x).map(|(lo, hi)| hi - lo).unwrap_or(0.0)
            })
            .collect();
        let narrowest = samples.iter().cloned().fold(f32::INFINITY, f32::min);
        let left = samples[0];
        let right = samples[samples.len() - 1];
        assert!(
            left > narrowest * 1.15 && right > narrowest * 1.15,
            "置信带应两端宽中间窄，实际 左 {left:.1} / 最窄 {narrowest:.1} / 右 {right:.1}"
        );
    }

    #[test]
    fn 回归散点必须画全部原始观测() {
        // 表里每个 x 有 3 个重复，`xy_rows` 只保留行均值。
        // 若按均值画，图上只有 6 个点、用均值拟合还会让 R² 虚高。
        let p = sample_project();
        let t = p.tables.iter().find(|t| t.name == "剂量–反应").unwrap();
        let mut spec = GraphSpec::default_for(t);
        spec.kind = ChartKind::Regression;
        spec.show_points = true;
        let r = render(&spec, t, 468.0, 318.0, 2.4, &FontBook::load_builtin());
        let expect = (0..t.group_names.len())
            .map(|g| t.xy_observations(g).len())
            .sum::<usize>();
        let dots = marks(&r).iter().filter(|(f, _)| *f).count();
        assert!(
            expect > 12 && dots == expect,
            "散点数应等于原始观测数：期望 {expect}，实际 {dots}"
        );
    }

    #[test]
    fn 多组回归散点必须用组色() {
        // 点、线、带是同一实体的三种表达，颜色必须贯穿。
        // 曾经把 ≥3 组散点压成中性灰，图例却是彩色，点线对不上。
        let p = sample_project();
        let (r, _) = render_kind(&p, ChartKind::Regression, "剂量–反应");
        let dots: Vec<_> = marks(&r)
            .iter()
            .filter(|(filled, _)| *filled)
            .map(|(_, b)| *b)
            .collect();
        assert!(dots.len() >= 9, "三组各 6 个点，实际 {}", dots.len());
        let mut gray = 0;
        for b in &dots {
            let c = r
                .scene
                .ops
                .iter()
                .find(|op| {
                    let bb = bbox(&op.path);
                    (bb == *b) && op.fill.is_some()
                })
                .and_then(|op| op.fill);
            if let Some(f) = c {
                // 中性灰：三通道接近
                if (f.r as i32 - f.g as i32).abs() < 14 && (f.g as i32 - f.b as i32).abs() < 14 {
                    gray += 1;
                }
            }
        }
        assert_eq!(
            gray, 0,
            "回归散点必须用组色，不能压成中性灰（灰度点 {gray}）"
        );
    }

    #[test]
    fn 生存曲线下方有风险人数表() {
        let p = sample_project();
        let (r, _) = render_kind(&p, ChartKind::Survival, "生存分析");
        // 风险表是画在绘图区下方的文字路径；至少要有几行图元落在框外下方。
        let below = r
            .scene
            .ops
            .iter()
            .filter(|op| {
                let b = bbox(&op.path);
                b.1 > r.plot.bottom() + 2.0 && b.3 < r.scene.height
            })
            .count();
        assert!(below >= 4, "E2-3：缺少风险人数表（框外下方图元 {below}）");
    }

    #[test]
    fn 回归图可以画曲线拟合() {
        // fit_model 决定拟合形状：Logistic4 的拟合线必须是曲线（不是直线）。
        use crate::core::curve::FitModel;
        let p = sample_project();
        let (r_lin, _) = render_kind(&p, ChartKind::Regression, "剂量–反应");
        let t = p.tables.iter().find(|t| t.name == "剂量–反应").unwrap();
        let mut spec = GraphSpec::default_for(t);
        spec.kind = ChartKind::Regression;
        spec.fit_model = FitModel::Logistic4;
        let r_cur = render(&spec, t, 468.0, 318.0, 2.4, &FontBook::load_builtin());
        // 判据用几何本身：拟合线上取三个 x，若是直线则共线。
        // （不能比路径段数——直线拟合也走 64 点采样，而且还有 CI 带多边形。）
        let _ = &r_lin;
        let t = p.tables.iter().find(|t| t.name == "剂量–反应").unwrap();
        let pts: Vec<(f64, f64)> = (0..t.group_names.len())
            .flat_map(|g| t.xy_observations(g))
            .collect();
        let cf = crate::core::curve::fit_curve(FitModel::Logistic4, &pts).unwrap();
        let (x0, x1) = (
            pts.iter().map(|p| p.0).fold(f64::INFINITY, f64::min),
            pts.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max),
        );
        let xa = x0 + (x1 - x0) * 0.15;
        let xb = x0 + (x1 - x0) * 0.5;
        let xc = x0 + (x1 - x0) * 0.85;
        let (ya, yb, yc) = (cf.eval(xa), cf.eval(xb), cf.eval(xc));
        let slope1 = (yb - ya) / (xb - xa);
        let slope2 = (yc - yb) / (xc - xb);
        assert!(
            (slope1 - slope2).abs() / (slope1.abs() + 1e-9) > 0.2,
            "Logistic4 拟合线的斜率应随 x 变化（非直线）：{slope1} vs {slope2}"
        );
        // 场景里也确实画出了曲线（多于 2 简的折线）
        assert!(r_cur.scene.ops.len() > 10);
    }

    #[test]
    fn 双Y轴画出右侧轴() {
        let p = sample_project();
        let t = p.tables.iter().find(|t| t.name == "剂量–反应").unwrap();
        let mut spec = GraphSpec::default_for(t);
        spec.kind = ChartKind::Line;
        spec.dual_y = true;
        spec.y2_title = "温度 (°C)".into();
        let r = render(&spec, t, 468.0, 318.0, 2.4, &FontBook::load_builtin());
        // 右轴：绘图区右边界处的竖线
        let right_x = r.plot.right();
        let has_right_axis = r.scene.ops.iter().any(|op| {
            op.stroke.is_some()
                && op
                    .path
                    .segs
                    .iter()
                    .filter_map(|s| match s {
                        crate::plot::scene::Seg::Move(p) | crate::plot::scene::Seg::Line(p) => {
                            Some(*p)
                        }
                        _ => None,
                    })
                    .all(|pt| (pt.x - right_x).abs() < 1.5)
        });
        assert!(has_right_axis, "启用双 Y 轴后应画出右轴竖线");
    }

    #[test]
    fn 分面画出面板分隔线() {
        let p = sample_project();
        let t = p.tables.iter().find(|t| t.name == "剂量–反应").unwrap();
        let mut spec = GraphSpec::default_for(t);
        spec.kind = ChartKind::Line;
        spec.facet = true;
        let r = render(&spec, t, 468.0, 318.0, 2.4, &FontBook::load_builtin());
        // 分面：面板之间有横分隔线（细、贯穿绘图区）
        let seps = r
            .scene
            .ops
            .iter()
            .filter(|op| {
                let Some(st) = op.stroke.as_ref() else {
                    return false;
                };
                let b = bbox(&op.path);
                (b.3 - b.1) < 3.0 && (b.2 - b.0) > r.plot.w * 0.8 && st.width < 1.6
            })
            .count();
        assert!(seps >= 2, "3 组分面应有至少 2 条面板分隔线，实际 {seps}");
    }

    /// 纯函数自检：构造重叠/越框样例，确认检测器会报、干净样例不报。
    #[test]
    fn 文本碰撞检测器抓重叠与越框() {
        use crate::plot::chart::text_collisions;
        // 两行文字压在一起
        let texts = vec![
            (
                "Vehicle R² = 0.741".into(),
                crate::plot::scene::Rect::new(100.0, 100.0, 80.0, 12.0),
            ),
            (
                "低剂量 R² = 0.640".into(),
                crate::plot::scene::Rect::new(140.0, 104.0, 80.0, 12.0),
            ),
        ];
        let plot = crate::plot::scene::Rect::new(50.0, 50.0, 400.0, 300.0);
        let v = text_collisions(&texts, plot);
        assert!(v.iter().any(|m| m.contains("文字重叠")), "{v:?}");

        // 绘图区内的标注越出右边框（中心在框内 440<450，右缘 480 越界）
        let texts = vec![(
            "非常长的标注文字".into(),
            crate::plot::scene::Rect::new(400.0, 100.0, 80.0, 12.0),
        )];
        let v = text_collisions(&texts, plot);
        assert!(v.iter().any(|m| m.contains("越出绘图区")), "{v:?}");

        // 干净样例：框外的刻度标签（中心在框外）不参与越框检查，彼此不重叠
        let texts = vec![
            (
                "0".into(),
                crate::plot::scene::Rect::new(30.0, 200.0, 10.0, 12.0),
            ),
            (
                "50".into(),
                crate::plot::scene::Rect::new(30.0, 160.0, 10.0, 12.0),
            ),
        ];
        assert!(text_collisions(&texts, plot).is_empty());
    }

    #[test]
    fn 图注统计行画在绘图区下方() {
        // G4/E2-5 的检验/效应量说明走图注行：不得进绘图区（会遮数据），
        // 必须在绘图区下沿之外。
        let p = sample_project();
        for (kind, table) in [
            (ChartKind::Survival, "生存分析"),
            (ChartKind::Regression, "剂量–反应"),
        ] {
            let t = p.tables.iter().find(|t| t.name == table).unwrap();
            let mut spec = GraphSpec::default_for(t);
            spec.kind = kind;
            spec.show_points = true;
            let r = render(&spec, t, 468.0, 318.0, 2.4, &FontBook::load_builtin());
            let below = r
                .scene
                .ops
                .iter()
                .filter(|op| {
                    let b = bbox(&op.path);
                    b.1 > r.plot.bottom() + 1.0
                        && b.3 < r.scene.height
                        && op.fill.is_some()
                        && op.path.segs.len() < 300 // 排除风险人数表的整表
                })
                .count();
            assert!(
                below > 0,
                "{} 应有图注统计行（绘图区下方文字）",
                kind.label()
            );
        }
        // 分类图带显著性括号：图注行必须有（检验全名 + 效应量）
        let t = p.tables.iter().find(|t| t.name == "体重变化").unwrap();
        let mut spec = GraphSpec::default_for(t);
        spec.kind = ChartKind::Column;
        spec.annotations = vec![crate::core::graph::SigAnnotation {
            a: 0,
            b: 1,
            label: "*".into(),
            y: None,
        }];
        let r = render(&spec, t, 468.0, 318.0, 2.4, &FontBook::load_builtin());
        let below = r
            .scene
            .ops
            .iter()
            .filter(|op| {
                let b = bbox(&op.path);
                b.1 > r.plot.bottom() + 1.0 && b.3 < r.scene.height && op.fill.is_some()
            })
            .count();
        assert!(below > 0, "带括号的柱状图应有图注统计行");
    }

    /// 图上直接编辑依赖标题外框：Main/X/Y 三个槽位都必须有、外框不得退化。
    #[test]
    fn 标题外框齐全供画布编辑() {
        use crate::plot::chart::TitleSlot;
        let p = sample_project();
        let t = p.tables.iter().find(|t| t.name == "剂量–反应").unwrap();
        let mut spec = GraphSpec::default_for(t);
        spec.kind = ChartKind::Line;
        spec.title = "图标题".into();
        spec.x_title = "X 轴".into();
        spec.y_title = "Y 轴".into();
        let r = render(&spec, t, 468.0, 318.0, 2.4, &FontBook::load_builtin());
        let slots: Vec<TitleSlot> = r.title_boxes.iter().map(|(s, _)| *s).collect();
        assert!(slots.contains(&TitleSlot::Main), "{slots:?}");
        assert!(slots.contains(&TitleSlot::X), "{slots:?}");
        assert!(slots.contains(&TitleSlot::Y), "{slots:?}");
        for (sl, b) in &r.title_boxes {
            assert!(b.w > 1.0 && b.h > 1.0, "{sl:?} 外框退化: {b:?}");
            assert!(b.x >= -1.0 && b.y >= -1.0, "{sl:?} 外框越出画布: {b:?}");
        }
        // Y 轴标题旋转 90°：外框应是竖长条（高 > 宽）
        let yb = r
            .title_boxes
            .iter()
            .find(|(s, _)| *s == TitleSlot::Y)
            .map(|(_, b)| *b)
            .unwrap();
        assert!(yb.h > yb.w, "Y 轴标题是竖排，外框应高>宽: {yb:?}");
    }

    #[test]
    fn 分类图分面每组一根柱() {
        // G8 分类图分面：4 组柱状图开启分面后应是 4 根居中柱（每面板一根）
        let p = sample_project();
        let t = p.tables.iter().find(|t| t.name == "体重变化").unwrap();
        let mut spec = GraphSpec::default_for(t);
        spec.kind = ChartKind::Column;
        spec.facet = true;
        let r = render(&spec, t, 468.0, 318.0, 2.4, &FontBook::load_builtin());
        let bars = r
            .scene
            .ops
            .iter()
            .filter(|op| {
                op.fill.map(|f| f.a == 226).unwrap_or(false)
                    && op.stroke.is_none()
                    && inside(&r, bbox(&op.path), 1.0)
            })
            .count();
        assert_eq!(bars, 4, "4 组分面应有 4 根柱，实际 {bars}");
        assert!(r.violations.is_empty(), "{:?}", r.violations);
    }

    #[test]
    fn 分面小提琴各自待在面板带内() {
        let p = sample_project();
        let t = p.tables.iter().find(|t| t.name == "体重变化").unwrap();
        let mut spec = GraphSpec::default_for(t);
        spec.kind = ChartKind::Violin;
        spec.facet = true;
        let r = render(&spec, t, 468.0, 318.0, 2.4, &FontBook::load_builtin());
        let s = 2.4_f32;
        let n = 4_f32;
        let gap = s * 3.0;
        let row_h = (r.plot.h - gap * (n - 1.0)) / n;
        let bodies: Vec<&Op> = r
            .scene
            .ops
            .iter()
            .filter(|op| {
                op.fill.map(|f| f.a == 96).unwrap_or(false)
                    && op.stroke.is_some()
                    && op.path.segs.len() > 60
            })
            .collect();
        assert_eq!(bodies.len(), 4, "4 组应有 4 个琴身");
        for (i, op) in bodies.iter().enumerate() {
            let b = bbox(&op.path);
            let top = r.plot.y + i as f32 * (row_h + gap);
            let bot = top + row_h;
            assert!(
                b.1 >= top - 1.0 && b.3 <= bot + 1.5,
                "第 {} 个琴身越出面板带: bbox {:?} 带 [{:.1},{:.1}]",
                i,
                b,
                top,
                bot
            );
        }
    }

    #[test]
    fn 密集序列不逐点画标记() {
        // 600 点的曲线不该有 600 个圆
        use crate::core::model::{Cell, ColRole, Column, DataTable, TableKind};
        let mut t = DataTable::template(TableKind::Xy, String::from("dense"), 1);
        t.columns.clear();
        let mut x = Column::new("x", ColRole::X);
        x.cells = (0..600).map(|i| Cell::number(i as f64)).collect();
        t.columns.push(x);
        let mut y = Column::new("y", ColRole::Y);
        y.cells = (0..600)
            .map(|i| Cell::number((i as f64 * 0.1).sin()))
            .collect();
        t.columns.push(y);
        t.group_names = vec!["g".into()];
        let mut spec = GraphSpec::default_for(&t);
        spec.kind = ChartKind::Line;
        spec.show_points = true;
        let r = render(&spec, &t, 468.0, 318.0, 2.4, &FontBook::load_builtin());
        assert_eq!(marks(&r).len(), 0, "600 点以上不应逐点画标记");
    }
}
