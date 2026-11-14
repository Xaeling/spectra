//! 曲线拟合 / 双 Y 轴 / 分面 的效果渲染（`cargo run --release --example curve_demo`）。

use stataura::core::curve::FitModel;
use stataura::core::graph::{ChartKind, ErrorKind, GraphSpec};
use stataura::core::sample::sample_project;
use stataura::plot::{raster, render, FontBook};

fn main() {
    let p = sample_project();
    let t = p.tables.iter().find(|t| t.name == "剂量–反应").unwrap();
    let fonts = FontBook::load_builtin();
    std::fs::create_dir_all("docs/eval").unwrap();

    let mut spec = GraphSpec::default_for(t);
    spec.kind = ChartKind::Regression;
    spec.error = ErrorKind::None;
    spec.title = "四参数 Logistic 拟合".into();
    spec.y_title = "响应 (% of max)".into();
    spec.fit_model = FitModel::Logistic4;
    let r = render(&spec, t, 468.0, 318.0, 2.4, &fonts);
    let (w, h, rgba) = raster::rasterize(&r.scene);
    image::save_buffer(
        "docs/eval/fit-logistic4.png",
        &rgba,
        w,
        h,
        image::ExtendedColorType::Rgba8,
    )
    .unwrap();
    println!("fit-logistic4.png  越界 {}", r.violations.len());

    let mut s2 = GraphSpec::default_for(t);
    s2.kind = ChartKind::Line;
    s2.error = ErrorKind::None;
    s2.title = "双 Y 轴".into();
    s2.dual_y = true;
    s2.y2_title = "右轴量纲".into();
    let r2 = render(&s2, t, 468.0, 318.0, 2.4, &fonts);
    let (w, h, rgba) = raster::rasterize(&r2.scene);
    image::save_buffer(
        "docs/eval/dual-y.png",
        &rgba,
        w,
        h,
        image::ExtendedColorType::Rgba8,
    )
    .unwrap();
    println!("dual-y.png  越界 {}", r2.violations.len());

    let mut s3 = GraphSpec::default_for(t);
    s3.kind = ChartKind::Line;
    s3.error = ErrorKind::Sem;
    s3.title = "分面（共享坐标轴）".into();
    s3.facet = true;
    let r3 = render(&s3, t, 468.0, 318.0, 2.4, &fonts);
    let (w, h, rgba) = raster::rasterize(&r3.scene);
    image::save_buffer(
        "docs/eval/facets.png",
        &rgba,
        w,
        h,
        image::ExtendedColorType::Rgba8,
    )
    .unwrap();
    println!("facets.png  越界 {}", r3.violations.len());
}
