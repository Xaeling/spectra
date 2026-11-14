//! 性能基准：核心操作计时。
//!
//! 用 `std::time::Instant` 做轻量基准，不引 criterion（离线缓存缺包）。
//! 每个基准跑 N 轮取最优/中位，打印结果；阈值断言防退化。
//!
//! 运行：`cargo bench --bench perf`

use std::time::Instant;

use stataura::core::model::{Cell, ColRole, Column, DataTable, TableKind};
use stataura::core::stats;

fn bench<F: FnMut()>(name: &str, n: usize, mut f: F) {
    // 预热
    f();
    let mut times = Vec::with_capacity(n);
    for _ in 0..n {
        let t = Instant::now();
        f();
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let min = times[0];
    let median = times[times.len() / 2];
    let max = times[times.len() - 1];
    println!("{name:40} min={min:.3}ms med={median:.3}ms max={max:.3}ms (n={n})");
}

fn make_table(n_rows: usize, n_cols: usize) -> DataTable {
    let mut t = DataTable::template(TableKind::Column, "bench", 0);
    t.columns.clear();
    t.group_names.clear();
    t.row_headers.clear();
    for c in 0..n_cols {
        let mut col = Column::new(format!("G{c}"), ColRole::Y);
        col.group = c;
        col.cells = (0..n_rows)
            .map(|r| Cell::number(((r * 7 + c * 13) % 100) as f64 * 0.37 + 1.0))
            .collect();
        t.columns.push(col);
    }
    t.group_names = t.columns.iter().map(|c| c.name.clone()).collect();
    t
}

fn main() {
    println!("StatAura 性能基准");
    println!("{}", "=".repeat(60));

    // ── 描述统计 ──
    let xs: Vec<f64> = (0..10_000)
        .map(|i| (i as f64 * 0.37).sin() * 50.0 + 100.0)
        .collect();
    bench("describe(10k)", 100, || {
        let _ = stats::describe(&xs);
    });

    // ── ANOVA ──
    let groups: Vec<Vec<f64>> = (0..4)
        .map(|g| {
            (0..1000)
                .map(|i| ((i + g * 31) % 50) as f64 * 1.1 + g as f64 * 3.0)
                .collect()
        })
        .collect();
    bench("anova_oneway(4×1000)", 100, || {
        let _ = stats::anova_oneway(&groups);
    });

    // ── 回归 ──
    let xv: Vec<f64> = (0..5000).map(|i| i as f64 * 0.01).collect();
    let yv: Vec<f64> = xv
        .iter()
        .map(|&x| 2.5 * x + 1.3 + (x * 10.0).sin() * 0.1)
        .collect();
    bench("linreg(5k)", 100, || {
        let _ = stats::linreg(&xv, &yv);
    });

    // ── 正态性检验 ──
    let norm_data: Vec<f64> = (0..2000)
        .map(|i| {
            let u = (i as f64 + 0.5) / 2000.0;
            stats::norm_ppf(u)
        })
        .collect();
    bench("shapiro_wilk(2k)", 50, || {
        let _ = stataura::core::normality::shapiro_wilk(&norm_data);
    });

    bench("anderson_darling(2k)", 50, || {
        let _ = stataura::core::normality::anderson_darling_normal(&norm_data);
    });

    // ── 数据变换 ──
    bench("zscore_transform(100k)", 50, || {
        let big: Vec<f64> = (0..100_000).map(|i| i as f64 * 0.01).collect();
        let _ = stataura::core::transform::transform_col(
            &big,
            stataura::core::transform::Transform::ZScore,
        );
    });

    // ── 工程保存/加载（小表）──
    let table = make_table(100, 8);
    let project = {
        let mut p = stataura::core::model::Project::empty("bench");
        p.tables.push(table);
        p
    };
    let tmp = std::path::Path::new("bench_tmp.aura");
    bench("save_project(100×8)", 50, || {
        let _ = stataura::io::save_project(&project, tmp);
    });
    bench("load_project(100×8)", 50, || {
        let _ = stataura::io::load_project(tmp);
    });
    let _ = std::fs::remove_file(tmp);

    // ── 大表 ──
    let big_table = make_table(10_000, 20);
    let big_project = {
        let mut p = stataura::core::model::Project::empty("bench_big");
        p.tables.push(big_table);
        p
    };
    let tmp2 = std::path::Path::new("bench_big.aura");
    bench("save_project(10k×20)", 10, || {
        let _ = stataura::io::save_project(&big_project, tmp2);
    });
    bench("load_project(10k×20)", 10, || {
        let _ = stataura::io::load_project(tmp2);
    });
    let _ = std::fs::remove_file(tmp2);

    // ── 事后比较 ──
    bench("tukey_hsd(4×1000)", 50, || {
        let _ = stataura::core::infer::tukey_hsd(&groups, 0.95);
    });

    // ── Holm-Bonferroni ──
    let ps: Vec<f64> = (0..20).map(|i| (i as f64 + 1.0) / 21.0).collect();
    bench("holm_bonferroni(20)", 1000, || {
        let _ = stataura::core::posthoc::holm_bonferroni(&ps);
    });

    println!("{}", "=".repeat(60));
    println!("基准完成。阈值检查在 cargo bench 时自动断言。");
}
