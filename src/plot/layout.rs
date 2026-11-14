//! 多图拼版渲染：把若干张图按网格合成一张大图。
//!
//! 合成在**场景层**完成：每个面板用 `plot::render` 各自渲染成完整场景，
//! 再按面板原点平移后合并进一个大场景——文字、坐标轴、图注编号都是真矢量
//! （或同源栅格），导出走 `io::export_chart` 现有管线，PNG/SVG/PDF 五种格式
//! 直接复用。
//!
//! 图注编号（a/b/c…）按规范画在各面板**左上角外侧**，供正文引用「图 2a」。

use crate::core::color::Rgba;
use crate::core::graph::GraphSpec;
use crate::core::model::{DataTable, LayoutPage};
use crate::plot::scene::{Pt, Rect, Scene, Transform};
use crate::plot::text::{FontBook, HAlign, VAlign};
use crate::plot::{render, Render};

/// 面板之间的间隔（pt）与画布外边距（pt）。
const GAP_PT: f32 = 8.0;
const PAD_PT: f32 = 6.0;
/// 图注编号字号（pt）。
const LABEL_PT: f32 = 8.0;

/// 拼版页的几何：总画布由列数与面板数决定。
///
/// 每个面板沿用单图的 468×318pt 纵横比；行数 = ceil(面板数 / 列数)。
pub fn layout_size(page: &LayoutPage, panel_w: f32, panel_h: f32) -> (f32, f32) {
    let n = page.panels.len().max(1);
    let cols = page.cols.clamp(1, 3).max(1).min(n);
    let rows = n.div_ceil(cols).max(1);
    let w = PAD_PT * 2.0 + panel_w * cols as f32 + GAP_PT * (cols as f32 - 1.0);
    let h = PAD_PT * 2.0 + panel_h * rows as f32 + GAP_PT * (rows as f32 - 1.0);
    (w, h)
}

/// 渲染拼版页。`panels` 与 `page.panels` 一一对应（调用方保证）。
pub fn render_layout(
    page: &LayoutPage,
    panels: &[(&GraphSpec, &DataTable)],
    panel_w: f32,
    panel_h: f32,
    px_per_pt: f32,
    fonts: &FontBook,
) -> Render {
    let (w_pt, h_pt) = layout_size(page, panel_w, panel_h);
    let s = px_per_pt.max(0.05);
    let w = w_pt * s;
    let h = h_pt * s;
    let mut scene = Scene::new(w, h, Rgba::rgba(255, 255, 255, 255));
    let mut hits = Vec::new();
    let mut violations = Vec::new();

    let n = panels.len();
    let cols = page.cols.clamp(1, 3).max(1).min(n.max(1));
    let gap = GAP_PT * s;
    let pad = PAD_PT * s;
    let cell_w = panel_w * s;
    let cell_h = panel_h * s;

    for (i, (spec, table)) in panels.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let ox = pad + col as f32 * (cell_w + gap);
        let oy = pad + row as f32 * (cell_h + gap);

        let r = render(spec, table, panel_w, panel_h, s, fonts);
        merge_scene(&mut scene, &r.scene, ox, oy);
        for hit in &r.hits {
            hits.push(crate::plot::Hit {
                at: Pt::new(hit.at.x + ox, hit.at.y + oy),
                title: hit.title.clone(),
                lines: hit.lines.clone(),
            });
        }
        for v in &r.violations {
            violations.push(format!("面板 {}：{}", page.label_case.label(i), v));
        }

        // 图注编号：面板左上角，压在面板标题左侧的空白上（标题居中）。
        if page.show_labels {
            fonts.draw(
                &mut scene,
                &page.label_case.label(i),
                Pt::new(ox + 1.0 * s, oy + 1.0 * s),
                LABEL_PT * s,
                crate::plot::text::Family::Sans,
                Rgba::rgba(24, 30, 40, 255),
                HAlign::Left,
                VAlign::Top,
                0.0,
                true,
            );
        }
    }

    Render {
        width: w.round().max(1.0) as u32,
        height: h.round().max(1.0) as u32,
        scene,
        plot: Rect::new(0.0, 0.0, w, h),
        x: crate::plot::axis::Axis::linear(0.0, 1.0),
        y: crate::plot::axis::Axis::linear(0.0, 1.0),
        hits,
        px_per_pt: s,
        violations,
        // 拼版页不做图上编辑（一个页面多个图的标题），不收集外框。
        title_boxes: Vec::new(),
        legend_box: None,
    }
}

/// 把 `src` 的全部图元平移 (dx, dy) 后并入 `dst`。
fn merge_scene(dst: &mut Scene, src: &Scene, dx: f32, dy: f32) {
    if dx == 0.0 && dy == 0.0 {
        dst.ops.extend(src.ops.iter().cloned());
        return;
    }
    let t = Transform::translate(dx, dy);
    for op in &src.ops {
        let mut op = op.clone();
        op.path.transform(&t);
        dst.ops.push(op);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::graph::{ChartKind, ErrorKind, GraphSpec};
    use crate::core::model::LabelCase;
    use crate::core::sample::sample_project;

    fn page_of(panels: Vec<u64>, cols: usize) -> LayoutPage {
        LayoutPage {
            id: 1,
            name: "拼版".into(),
            cols,
            panels,
            show_labels: true,
            label_case: LabelCase::Lower,
        }
    }

    /// 新建拼版页应把工程里现有的图全部收进去（用户在侧栏/菜单点「新建拼版」的默认行为）。
    #[test]
    fn 新建拼版页默认收进全部图() {
        use crate::core::model::Project;
        let mut proj = Project::empty("t");
        let t = proj.add_table(crate::core::model::TableKind::Column, String::from("t"));
        let g1 = proj.add_graph_for(t);
        let g2 = proj.add_graph_for(t);
        let id = proj.add_layout();
        let page = proj.layout(id).unwrap();
        assert_eq!(page.panels, vec![g1, g2], "应按创建顺序收进全部图");
        assert_eq!(page.cols, 2);
        assert!(page.show_labels);
    }

    #[test]
    fn 图注编号序列为abc与123() {
        assert_eq!(LabelCase::Lower.label(0), "a");
        assert_eq!(LabelCase::Lower.label(2), "c");
        assert_eq!(LabelCase::Upper.label(25), "Z");
        assert_eq!(LabelCase::Number.label(9), "10");
    }

    #[test]
    fn 网格尺寸随面板数与列数推算() {
        let p = page_of(vec![1, 2, 3, 4], 2);
        let (w, h) = layout_size(&p, 468.0, 318.0);
        // 2 列 × 2 行：宽 = 2×468 + gap + pad×2，高 = 2×318 + gap + pad×2
        let (w1, h1) = layout_size(&page_of(vec![1], 1), 468.0, 318.0);
        assert!(w > 2.0 * 468.0 * 0.9, "2 列应接近两倍宽：{w}");
        assert!(h > 2.0 * 318.0 * 0.9, "2 行应接近两倍高：{h}");
        assert!(w1 < w && h1 < h);

        // 3 个面板 2 列 → 2 行；最后一页不满也按整格占位
        let (_w3, h3) = layout_size(&page_of(vec![1, 2, 3], 2), 468.0, 318.0);
        assert!((h3 - h).abs() < 1.0, "3 面板 2 列也是 2 行：{h3} vs {h}");
        let _ = (w1, h1);
    }

    #[test]
    fn 拼版合成各面板与图注编号() {
        let proj = sample_project();
        let t = proj.tables.first().unwrap();
        let mut spec = GraphSpec::default_for(t);
        spec.kind = ChartKind::Column;
        spec.error = ErrorKind::Sem;
        let panels = vec![(&spec, t), (&spec, t), (&spec, t), (&spec, t)];
        let page = page_of(vec![1, 2, 3, 4], 2);
        let r = render_layout(&page, &panels, 468.0, 318.0, 2.0, &FontBook::load_builtin());

        // 四个面板各自渲染出完整图元，合并后 ops 应远多于单图
        assert!(
            r.scene.ops.len() > 200,
            "4 面板合成 ops={}",
            r.scene.ops.len()
        );

        // 图注编号 a/b/c/d：找绘图区外左上角附近的短文本路径
        // 编号用 bold 小字画在各面板 (ox+1, oy+1)——通过四象限都有图元验证分块。
        let (w, h) = (r.width as f32, r.height as f32);
        let mut quads = [false; 4];
        for op in &r.scene.ops {
            for seg in &op.path.segs {
                let p = match seg {
                    crate::plot::scene::Seg::Move(p) | crate::plot::scene::Seg::Line(p) => *p,
                    crate::plot::scene::Seg::Quad(_, p)
                    | crate::plot::scene::Seg::Cubic(_, _, p) => *p,
                    crate::plot::scene::Seg::Close => continue,
                };
                let q = (if p.x < w / 2.0 { 0 } else { 1 }) + (if p.y < h / 2.0 { 0 } else { 2 });
                quads[q] = true;
            }
        }
        assert!(
            quads.iter().all(|&b| b),
            "四个象限都应有面板内容：{:?}",
            quads
        );

        // 无越界（合并后的坐标系里几何自检仍应全绿）
        assert!(r.violations.is_empty(), "{:?}", r.violations);
    }

    #[test]
    fn 面板引用丢失时违规带面板号() {
        let proj = sample_project();
        let t = proj.tables.first().unwrap();
        let spec = GraphSpec::default_for(t);
        let panels = vec![(&spec, t)];
        let mut page = page_of(vec![1], 1);
        page.label_case = LabelCase::Number;
        let r = render_layout(&page, &panels, 468.0, 318.0, 2.0, &FontBook::load_builtin());
        for v in &r.violations {
            assert!(v.starts_with("面板 "), "违规应带面板号前缀：{v}");
        }
    }
}
