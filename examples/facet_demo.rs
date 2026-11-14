//! 分面示例：分类图与 XY 图各一张。
//!
//! 用法：`cargo run --example facet_demo`
//! 产物：`docs/eval/facet-cat.png`、`docs/eval/facet-xy.png`。
//!
//! 关键点：每个面板**各自画一套 Y 刻度**（同一值域压进 1/n 的高度，
//! 沿用整幅映射会让第二个面板的标签错位一整个面板）。

use stataura::core::graph::{ChartKind, ErrorKind, GraphSpec};
use stataura::core::sample::sample_project;
use stataura::plot::{render, FontBook};

fn main() {
    let p = sample_project();
    let fonts = FontBook::load_builtin();
    std::fs::create_dir_all("docs/eval").ok();

    // ---- 分类图：每组一面板（共享 Y 值域）----
    let t = p.tables.iter().find(|t| t.name == "体重变化").unwrap();
    let mut spec = GraphSpec::default_for(t);
    spec.kind = ChartKind::Violin;
    spec.facet = true;
    spec.error = ErrorKind::Sem;
    spec.show_points = true;
    spec.title = "分类图分面（小提琴，每组一面板）".into();
    let r = render(&spec, t, 468.0, 318.0, 3.0, &fonts);
    let (w, h, rgba) = stataura::plot::raster::rasterize(&r.scene);
    let img = image::RgbaImage::from_raw(w, h, rgba).unwrap();
    img.save("docs/eval/facet-cat.png").unwrap();
    println!(
        "wrote docs/eval/facet-cat.png  越界={} 条",
        r.violations.len()
    );

    // ---- XY 图：两个数据集各占一条横向面板，共享 X 与 Y ----
    let t = p.tables.iter().find(|t| t.name == "剂量–反应").unwrap();
    let mut spec = GraphSpec::default_for(t);
    spec.kind = ChartKind::Regression;
    spec.facet = true;
    spec.title = "XY 分面（两个数据集，共享坐标轴）".into();
    let r = render(&spec, t, 468.0, 318.0, 3.0, &fonts);
    let (w, h, rgba) = stataura::plot::raster::rasterize(&r.scene);
    let img = image::RgbaImage::from_raw(w, h, rgba).unwrap();
    img.save("docs/eval/facet-xy.png").unwrap();
    println!(
        "wrote docs/eval/facet-xy.png  越界={} 条",
        r.violations.len()
    );
}
