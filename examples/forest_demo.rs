use stataura::core::graph::{ChartKind, ErrorKind, GraphSpec};
use stataura::core::model::{Cell, ColRole, Column, DataTable, TableKind};
use stataura::plot::{render, FontBook};

fn main() {
    let mut t = DataTable::template(TableKind::MultipleVariables, String::from("meta"), 0);
    t.columns.clear();
    t.row_headers = vec![
        "Smith 2019".into(),
        "Chen 2020".into(),
        "Kumar 2021".into(),
        "Garcia 2022".into(),
        "本研究".into(),
    ];
    // 列1=效应量 列2=CI下限 列3=CI上限 列4=权重
    let data: [(&str, [f64; 5]); 4] = [
        ("效应量", [-0.32, 0.45, -0.10, 0.22, 0.18]),
        ("CI下限", [-0.71, 0.12, -0.44, -0.05, 0.02]),
        ("CI上限", [0.07, 0.78, 0.24, 0.49, 0.34]),
        ("权重", [12.0, 25.0, 18.0, 30.0, 22.0]),
    ];
    for (name, vals) in data.iter() {
        let mut col = Column::new(*name, ColRole::Y);
        col.cells = vals.iter().map(|v| Cell::number(*v)).collect();
        t.columns.push(col);
    }
    let mut spec = GraphSpec::default_for(&t);
    spec.kind = ChartKind::Forest;
    spec.error = ErrorKind::None;
    spec.title = "森林图（效应量与 95% CI）".into();
    spec.x_title = "效应量".into();
    let fonts = FontBook::load_builtin();
    let r = render(&spec, &t, 468.0, 318.0, 3.0, &fonts);
    let (w, h, rgba) = stataura::plot::raster::rasterize(&r.scene);
    let img = image::RgbaImage::from_raw(w, h, rgba).unwrap();
    std::fs::create_dir_all("docs/eval").ok();
    img.save("docs/eval/forest.png").unwrap();
    println!("wrote docs/eval/forest.png  越界={} 条", r.violations.len());
}
