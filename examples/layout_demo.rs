//! 拼版页样张：2×2 四面板 + a/b/c/d 图注编号，验证场景合成与编号落位。
//!
//! 运行：`cargo run --release --example layout_demo`

use stataura::core::graph::{ChartKind, ErrorKind, GraphSpec, LegendPos};
use stataura::core::model::{LabelCase, LayoutPage};
use stataura::core::sample::sample_project;
use stataura::plot::layout::render_layout;
use stataura::plot::raster::rasterize;
use stataura::plot::text::FontBook;

fn main() {
    let proj = sample_project();
    let fonts = FontBook::load_builtin();

    // 四个面板：柱状、折线、箱线、生存（同一示例工程里的不同表/图型）。
    let mut panels: Vec<(GraphSpec, &stataura::core::model::DataTable)> = Vec::new();
    let pick = ["体重变化", "剂量–反应", "体重变化", "生存分析"];
    let kinds = [
        ChartKind::Column,
        ChartKind::Line,
        ChartKind::Box,
        ChartKind::Survival,
    ];
    for (name, kind) in pick.iter().zip(kinds.iter()) {
        let t = proj.tables.iter().find(|t| t.name == *name).unwrap();
        let mut spec = GraphSpec::default_for(t);
        spec.kind = *kind;
        spec.error = if kind.shows_error() {
            ErrorKind::Sem
        } else {
            ErrorKind::None
        };
        spec.legend_pos = LegendPos::OutsideRight;
        panels.push((spec, t));
    }

    let page = LayoutPage {
        id: 1,
        name: "拼版样张".into(),
        cols: 2,
        panels: vec![1, 2, 3, 4],
        show_labels: true,
        label_case: LabelCase::Lower,
    };

    let started = std::time::Instant::now();
    let refs: Vec<(&GraphSpec, &stataura::core::model::DataTable)> =
        panels.iter().map(|(s, t)| (s, *t)).collect();
    let r = render_layout(&page, &refs, 468.0, 318.0, 2.0, &fonts);
    let (w, h, rgba) = rasterize(&r.scene);
    let path = "docs/eval/layout-2x2.png";
    std::fs::create_dir_all("docs/eval").ok();
    image::save_buffer(path, &rgba, w, h, image::ExtendedColorType::Rgba8).unwrap();
    println!(
        "wrote {path}  {w}x{h}  ops={}  violations={}  {} ms",
        r.scene.ops.len(),
        r.violations.len(),
        started.elapsed().as_millis()
    );
    for v in &r.violations {
        println!("  - {v}");
    }
}
