//! 图型样张渲染 + **视觉回归基线**。
//!
//! ```text
//! cargo run --release --example gallery                  # 渲染到 out/（人工检视）
//! cargo run --release --example gallery -- render <目录>  # 同上，指定目录
//! cargo run --release --example gallery -- update        # 写基线到 docs/baseline/
//! cargo run --release --example gallery -- check         # 与基线逐像素比对，不一致退出码 1
//! ```
//!
//! 比对是**逐像素精确**的（RGBA8 全通道）：渲染路径没有随机性、没有时间依赖，
//! 本机两次渲染逐字节一致，所以精确比对既可靠又最有约束力——任何一处错位、
//! 颜色漂移、标签换行变化都会被抓到。
//!
//! 比对失败时会在 `docs/_baseline_fail/` 留下：
//! - `<名字>.png` —— 当前渲染（新样子）
//! - `<名字>.diff.png` —— 差异可视化（不一致的像素标成洋红，其余转灰）
//!
//! 基线覆盖：26 种图型 × 语义合适的表、4 套绘图主题、分面（同框 / 自由 Y /
//! XY 分面）、图内标注开与关、4 种误差棒。这些都是踩过坑的地方。

use stataura::core::graph::{ChartKind, ErrorKind, GraphSpec, PlotThemeId};
use stataura::core::model::{DataTable, Project};
use stataura::core::sample::sample_project;
use stataura::plot::{raster, render, FontBook};

const BASELINE_DIR: &str = "docs/baseline";
const FAIL_DIR: &str = "docs/_baseline_fail";
/// 导出用的像素倍率：基线取 2.4 与样张一致，看得清也抓得住错位。
const SCALE: f32 = 2.4;
const W: f32 = 468.0;
const H: f32 = 318.0;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(|s| s.as_str()).unwrap_or("render");
    let out = match mode {
        "render" => args.get(1).cloned().unwrap_or_else(|| "out".to_string()),
        "update" => BASELINE_DIR.to_string(),
        "check" => BASELINE_DIR.to_string(),
        _ => {
            eprintln!("用法:");
            eprintln!("  gallery                 渲染到 out/");
            eprintln!("  gallery render <目录>    渲染到指定目录");
            eprintln!("  gallery update          写基线到 {BASELINE_DIR}/");
            eprintln!("  gallery check           与基线比对，不一致退出码 1");
            std::process::exit(2);
        }
    };
    std::fs::create_dir_all(&out).unwrap();

    let fonts = FontBook::load_builtin();
    let p = sample_project();
    let cases = build_cases(&p);

    if mode == "check" {
        let _ = std::fs::remove_dir_all(FAIL_DIR);
        std::fs::create_dir_all(FAIL_DIR).unwrap();
    }

    let mut bad = 0usize;
    for (name, spec, table) in &cases {
        let r = render(spec, table, W, H, SCALE, &fonts);
        let (w, h, rgba) = raster::rasterize(&r.scene);
        let path = format!("{out}/{name}.png");

        match mode {
            "check" => {
                if let Err(msg) = check(&path, &rgba, w, h, name) {
                    bad += 1;
                    println!("{name}  ✗ {msg}");
                } else {
                    println!("{name}  ✓");
                }
            }
            _ => {
                image::save_buffer(&path, &rgba, w, h, image::ExtendedColorType::Rgba8).unwrap();
                if r.violations.is_empty() {
                    println!("{path}  {w}x{h}  {} hits  ✓ 无越界", r.hits.len());
                } else {
                    println!("{path}  {w}x{h}  ✗ {} 处越界", r.violations.len());
                    for v in r.violations.iter().take(12) {
                        println!("    - {v}");
                    }
                    bad += 1;
                }
            }
        }
    }

    if mode == "check" {
        if bad == 0 {
            println!("\n全部 {} 张与基线一致。", cases.len());
        } else {
            println!(
                "\n{bad} / {} 张与基线不一致，差异图在 {FAIL_DIR}/。",
                cases.len()
            );
            println!("确认新样子正确后运行 `gallery update` 刷新基线。");
            std::process::exit(1);
        }
    } else if bad > 0 {
        std::process::exit(1);
    }
}

/// 逐像素比对：尺寸或任一通道不同都算不一致，并产出 diff 图。
fn check(baseline_path: &str, rgba: &[u8], w: u32, h: u32, name: &str) -> Result<(), String> {
    let base = image::open(baseline_path)
        .map_err(|_| format!("基线缺失（先运行 gallery update）"))?
        .to_rgba8();
    if base.width() != w || base.height() != h {
        save_fail(name, rgba, w, h, None);
        return Err(format!(
            "尺寸变了 {}x{} → {}x{}",
            base.width(),
            base.height(),
            w,
            h
        ));
    }
    let b = base.as_raw();
    if b == rgba {
        return Ok(());
    }
    let diff: Vec<u8> = rgba
        .chunks_exact(4)
        .zip(b.chunks_exact(4))
        .flat_map(|(n, o)| {
            if n == o {
                [n[0] / 3 + 100, n[1] / 3 + 100, n[2] / 3 + 100, 255]
            } else {
                // 洋红 = 不一致像素；灰底 = 一致像素，便于看差异落在哪
                [255, 0, 255, 255]
            }
        })
        .collect();
    save_fail(name, rgba, w, h, Some(&diff));
    let ndiff = rgba
        .chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(n, o)| n != o)
        .count();
    Err(format!("{ndiff} 个像素不一致"))
}

fn save_fail(name: &str, rgba: &[u8], w: u32, h: u32, diff: Option<&[u8]>) {
    let _ = image::save_buffer(
        &format!("{FAIL_DIR}/{name}.png"),
        rgba,
        w,
        h,
        image::ExtendedColorType::Rgba8,
    );
    if let Some(d) = diff {
        let _ = image::save_buffer(
            &format!("{FAIL_DIR}/{name}.diff.png"),
            d,
            w,
            h,
            image::ExtendedColorType::Rgba8,
        );
    }
}

/// 全部基线用例：图型 × 表 + 主题 + 分面 + 标注 + 误差棒。
/// 每个用例都要「语义合适」——图型用错数据表，基线就没意义了。
fn build_cases(p: &Project) -> Vec<(String, GraphSpec, &DataTable)> {
    let mut out = Vec::new();

    // ── 26 种图型 ──
    for (idx, kind) in ChartKind::ALL.iter().enumerate() {
        let table = pick(p, *kind);
        let mut spec = GraphSpec::default_for(table);
        spec.kind = *kind;
        spec.title = format!("{}（{}）", kind.label(), kind.family_label());
        spec.y_title = match table.name.as_str() {
            "剂量–反应" => "响应 (% of max)".into(),
            "体重变化" => "体重变化 (g)".into(),
            _ => String::new(),
        };
        spec.error = if kind.shows_error() {
            ErrorKind::Sem
        } else {
            ErrorKind::None
        };
        spec.show_legend = matches!(table.name.as_str(), "性别 × 治疗" | "人群构成" | "生存分析");
        out.push((format!("{idx:02}_{kind:?}"), spec, table));
    }

    // ── 4 套绘图主题（同一张柱状图） ──
    let body = p.tables.iter().find(|t| t.name == "体重变化").unwrap();
    for (i, theme) in [
        PlotThemeId::Aura,
        PlotThemeId::Prism,
        PlotThemeId::Nature,
        PlotThemeId::Mono,
    ]
    .into_iter()
    .enumerate()
    {
        let mut spec = GraphSpec::default_for(body);
        spec.kind = ChartKind::Column;
        spec.theme = theme;
        spec.title = format!("主题 · {}", theme.label());
        spec.error = ErrorKind::Sem;
        out.push((format!("theme_{i}"), spec, body));
    }

    // ── 分面：这里踩过「刻度错位」「折线连点」两个坑，必须有基线 ──
    let grouped = p.tables.iter().find(|t| t.name == "性别 × 治疗").unwrap();
    for (i, (facet, free_y, kind)) in [
        (true, false, ChartKind::Column),
        (true, true, ChartKind::Column),
        (true, false, ChartKind::Scatter),
    ]
    .into_iter()
    .enumerate()
    {
        let mut spec = GraphSpec::default_for(grouped);
        spec.kind = kind;
        spec.facet = facet;
        spec.facet_free_y = free_y;
        spec.title = format!(
            "分面 · {}{}",
            kind.label(),
            if free_y {
                "（自由 Y）"
            } else {
                "（共享 Y）"
            }
        );
        spec.error = ErrorKind::Sem;
        out.push((format!("facet_{i}"), spec, grouped));
    }
    // XY 分面 + 回归：基线里必须只有散点 + 拟合线，不能有点之间的连线
    let xy = p.tables.iter().find(|t| t.name == "剂量–反应").unwrap();
    let mut spec = GraphSpec::default_for(xy);
    spec.kind = ChartKind::Regression;
    spec.facet = true;
    spec.title = "分面 · 回归（不应连点）".into();
    spec.y_title = "响应 (% of max)".into();
    out.push(("facet_regression".into(), spec, xy));

    // ── 图内标注开 / 关：关掉后方程、R²、p、图注都不该出现 ──
    for (i, on) in [(0, true), (1, false)] {
        let mut spec = GraphSpec::default_for(xy);
        spec.kind = ChartKind::Regression;
        spec.annotations_visible = on;
        spec.title = if on {
            "标注 · 开".into()
        } else {
            "标注 · 关".into()
        };
        spec.y_title = "响应 (% of max)".into();
        out.push((format!("annot_{i}"), spec, xy));
    }

    // ── 4 种误差棒：不显示 / SD / SEM / 95% CI ──
    for (i, e) in ErrorKind::ALL.iter().enumerate() {
        let mut spec = GraphSpec::default_for(body);
        spec.kind = ChartKind::Column;
        spec.error = *e;
        spec.title = format!("误差棒 · {}", e.label());
        out.push((format!("error_{i}"), spec, body));
    }

    out
}

/// 给每个图型挑一张语义合适的数据表。
fn pick<'a>(p: &'a Project, kind: ChartKind) -> &'a DataTable {
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
    .expect("sample project has tables")
}
