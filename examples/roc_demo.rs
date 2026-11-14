use stataura::core::graph::{ChartKind, ErrorKind, GraphSpec};
use stataura::core::sample::sample_project;
use stataura::plot::{render, FontBook};

fn main() {
    let p = sample_project();
    let t = p.tables.iter().find(|t| t.name == "体重变化").unwrap();
    let mut spec = GraphSpec::default_for(t);
    spec.kind = ChartKind::Roc;
    spec.error = ErrorKind::None;
    spec.title = "诊断效能（体重变化）".into();
    let fonts = FontBook::load_builtin();
    let r = render(&spec, t, 468.0, 318.0, 3.0, &fonts);
    let (w, h, rgba) = stataura::plot::raster::rasterize(&r.scene);
    let img = image::RgbaImage::from_raw(w, h, rgba).unwrap();
    std::fs::create_dir_all("docs/eval").ok();
    img.save("docs/eval/roc.png").unwrap();
    println!("wrote docs/eval/roc.png  越界={} 条", r.violations.len());
}
