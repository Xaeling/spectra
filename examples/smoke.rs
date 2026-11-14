//! 端到端冒烟：工程读写、统计、渲染、CSV、PNG/SVG/PDF 导出。

use std::path::PathBuf;

use stataura::core::analysis;
use stataura::core::model::{AnalysisKind, AnalysisOptions, Project};
use stataura::core::sample::sample_project;
use stataura::io::{self, ExportFormat};
use stataura::plot::{raster, render, FontBook};

fn main() -> anyhow::Result<()> {
    let dir = PathBuf::from("out/smoke");
    std::fs::create_dir_all(&dir)?;
    let fonts = FontBook::load_builtin();
    let project = sample_project();

    // 1) 工程往返
    let path = dir.join("roundtrip.aura");
    io::save_project(&project, &path)?;
    let loaded: Project = io::load_project(&path)?;
    assert_eq!(loaded.tables.len(), project.tables.len());
    assert_eq!(loaded.graphs.len(), project.graphs.len());
    assert_eq!(loaded.analyses.len(), project.analyses.len());
    println!(
        "project roundtrip ok: {} bytes",
        std::fs::metadata(&path)?.len()
    );

    // 2) 每种分析都能跑（XY 专用分析在 XY 表上，其余在柱状表上）
    for (ti, table) in loaded.tables.iter().enumerate() {
        if ti != 0 && ti != 1 {
            continue;
        }
        for kind in AnalysisKind::ALL {
            // 只跑「菜单里会出现」的分析：这份 smoke 的契约是
            // 「能点到的分析一定跑得出来」，不是「任意分析配任意表都能跑」。
            if !kind.supports(table.kind) {
                continue;
            }
            let mut opts = AnalysisOptions::default();
            let groups = table.analysis_groups();
            if kind.needs_two_groups() {
                if groups.len() < 2 {
                    continue;
                }
                opts.columns = vec![0, 1];
            }
            let r = analysis::run(table, kind, &opts);
            println!(
                "table {ti} analysis {kind:?}: {:?}",
                r.as_ref().map(|x| x.rows.len())
            );
            assert!(r.is_ok(), "{kind:?}");
        }
    }

    // 3) 每个图都渲染 + 导出
    for (i, g) in loaded.graphs.iter().enumerate() {
        let t = loaded.table(g.table).unwrap();
        let r = render(&g.spec, t, 468.0, 318.0, 2.4, &fonts);
        let (w, h, rgba) = raster::rasterize(&r.scene);
        image::save_buffer(
            dir.join(format!("g{i}.png")),
            &rgba,
            w,
            h,
            image::ColorType::Rgba8,
        )?;
        let svg = stataura::plot::export::to_svg(&r.scene, 468.0, 318.0);
        assert!(svg.starts_with("<?xml"));
        assert!(!svg.contains("<text"));
        assert!(svg.matches("<path").count() > 10);
        std::fs::write(dir.join(format!("g{i}.svg")), &svg)?;
        let pdf = stataura::plot::export::to_pdf(&r.scene, 468.0, 318.0);
        assert!(pdf.starts_with(b"%PDF-1.4"));
        std::fs::write(dir.join(format!("g{i}.pdf")), &pdf)?;
        println!(
            "graph {i}: {w}x{h}, svg {} KB, pdf {} KB",
            svg.len() / 1024,
            pdf.len() / 1024
        );
    }

    // 4) CSV 往返
    let csv = dir.join("weight.csv");
    io::export_csv(&loaded.tables[1], &csv)?;
    let imported = io::import_csv(&csv, "回读")?;
    assert_eq!(imported.columns.len(), loaded.tables[1].columns.len());
    println!("csv roundtrip ok: {} rows", imported.nrows());

    // 5) 两种主题的导出
    let t = &loaded.tables[1];
    let mut spec = gspec(&loaded, 1);
    spec.theme = stataura::core::graph::PlotThemeId::Nature;
    let r = render(&spec, t, 468.0, 318.0, 2.4, &fonts);
    let (_, _, rgba) = raster::rasterize(&r.scene);
    let _ = rgba;
    io::export_chart(&r, ExportFormat::Png, 468.0, 318.0, &dir.join("nature.png"))?;

    println!("SMOKE OK");
    Ok(())
}

fn gspec(p: &Project, i: usize) -> stataura::core::graph::GraphSpec {
    p.graphs[i].spec.clone()
}
