//! 首次启动的示例工程：真实感数据、4 张图、3 份分析结果。
//! 所有随机数都是确定性的，保证截图与视觉回归可复现。

use crate::core::analysis;
use crate::core::graph::PlotThemeId;
use crate::core::graph::{ChartKind, ErrorKind, GraphSpec, LegendPos, SigAnnotation};
use crate::core::model::{
    Analysis, AnalysisKind, AnalysisOptions, Cell, ColRole, Column, DataTable, Project, TableKind,
};
use crate::core::palette::PaletteId;

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
    }
    fn gauss(&mut self) -> f64 {
        let u1 = self.next().max(1e-12);
        let u2 = self.next();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

pub fn sample_project() -> Project {
    let mut p = Project::empty("示例 · 药物对体重与生存的影响");

    let xy_id = build_dose_table(&mut p);
    let bw_id = build_weight_table(&mut p);
    let surv_id = build_survival_table(&mut p);
    let grouped_id = build_grouped_table(&mut p);
    let parts_id = build_parts_table(&mut p);
    let nested_id = build_nested_table(&mut p);
    let cont_id = build_contingency_table(&mut p);

    let graphs = [
        build_dose_graph(&mut p, xy_id),
        build_weight_graph(&mut p, bw_id),
        build_survival_graph(&mut p, surv_id),
        build_grouped_graph(&mut p, grouped_id),
        build_extra_graph(
            &mut p,
            bw_id,
            "体重变化 · 小提琴",
            ChartKind::Violin,
            "体重变化",
        ),
        build_extra_graph(
            &mut p,
            parts_id,
            "人群构成 · 环形图",
            ChartKind::Donut,
            "人群构成",
        ),
        build_extra_graph(
            &mut p,
            nested_id,
            "教学方法 · 箱线",
            ChartKind::Box,
            "考核成绩",
        ),
        build_extra_graph(
            &mut p,
            cont_id,
            "疗效构成 · 百分比堆叠",
            ChartKind::StackedBar100,
            "疗效构成",
        ),
    ];

    let mut add_analysis = |name: &str, table: u64, kind: AnalysisKind, opts: AnalysisOptions| {
        let result = analysis::run(p.table(table).unwrap(), kind, &opts).unwrap_or_else(|e| {
            crate::core::model::ResultTable {
                title: kind.label().into(),
                method: String::new(),
                columns: vec!["错误".into()],
                rows: vec![vec![e]],
                note: String::new(),
            }
        });
        let id = p.new_id();
        p.analyses.push(Analysis {
            id,
            name: name.into(),
            table,
            kind,
            options: opts,
            result,
        });
        id
    };

    add_analysis(
        "体重 · 描述统计",
        bw_id,
        AnalysisKind::Descriptive,
        AnalysisOptions::default(),
    );
    add_analysis(
        "体重 · 单因素 ANOVA",
        bw_id,
        AnalysisKind::OneWayAnova,
        AnalysisOptions::default(),
    );
    add_analysis(
        "体重 · Control vs 高剂量",
        bw_id,
        AnalysisKind::UnpairedT,
        AnalysisOptions {
            columns: vec![0, 2],
            ..Default::default()
        },
    );
    add_analysis(
        "剂量–反应 · 线性回归",
        xy_id,
        AnalysisKind::LinearRegression,
        AnalysisOptions::default(),
    );
    // 智能分析：一份「全指标」报告（概览 → 前提 → 主检验 → 事后 → 结论）。
    add_analysis(
        "体重 · 智能分析",
        bw_id,
        AnalysisKind::Smart,
        AnalysisOptions::default(),
    );

    let _ = graphs;
    p.notes = "示例工程：拖动画布右侧的检查器，所有改动实时生效。".into();
    p
}

/// 嵌套表：两种教学方法（外层）× 每方法两个班级（内层），
/// 每个班级 6 名学生。列按外层分组，列名写出内层子组。
fn build_nested_table(p: &mut Project) -> u64 {
    let id = p.add_table(TableKind::Nested, "教学方法 · 嵌套");
    let t = p.table_mut(id).unwrap();
    let mut rng = Rng::new(0xC0FFEE);
    let names = [
        "方法 A · 1 班",
        "方法 A · 2 班",
        "方法 B · 1 班",
        "方法 B · 2 班",
    ];
    let base = [72.0, 76.0, 81.0, 84.0];
    let mut cols = Vec::new();
    for (g, name) in names.iter().enumerate() {
        let mut c = Column::new(*name, ColRole::Y);
        c.group = g / 2;
        for _ in 0..6 {
            c.cells.push(Cell::number(base[g] + rng.gauss() * 3.6));
        }
        cols.push(c);
    }
    let tname = t.name.clone();
    *t = DataTable {
        id,
        name: tname,
        kind: TableKind::Nested,
        columns: cols,
        group_names: vec!["方法 A".into(), "方法 B".into()],
        row_headers: (1..=6).map(|i| format!("学生 {i}")).collect(),
        row_header_width: None,
        notes: "两层嵌套重复：班级嵌套在教学方法内。适合嵌套 t 检验 / 嵌套 ANOVA。".into(),
    };
    id
}

/// 列联表：两种治疗 × 三种结局的**频数**（整数）。
/// 适合卡方独立性检验与 Fisher 精确检验。
fn build_contingency_table(p: &mut Project) -> u64 {
    let id = p.add_table(TableKind::Contingency, "疗效 · 列联表");
    let t = p.table_mut(id).unwrap();
    let rows = ["显著缓解", "部分缓解", "无效"];
    let data = [[38.0, 12.0], [22.0, 26.0], [8.0, 34.0]];
    let tname = t.name.clone();
    let mut cols = Vec::new();
    for (j, cname) in ["新疗法", "常规治疗"].iter().enumerate() {
        let mut c = Column::new(*cname, ColRole::Y);
        c.group = j;
        c.cells = data.iter().map(|row| Cell::number(row[j])).collect();
        cols.push(c);
    }
    *t = DataTable {
        id,
        name: tname,
        kind: TableKind::Contingency,
        columns: cols,
        group_names: vec!["新疗法".into(), "常规治疗".into()],
        row_headers: rows.iter().map(|s| s.to_string()).collect(),
        row_header_width: None,
        notes: "单元格是各结局的人数（非负整数）。适合卡方独立性检验 / Fisher 精确检验。".into(),
    };
    t.ensure_rows(rows.len());
    id
}

/// 构成表：每行一个互斥类别（饼 / 环的扇区）。
fn build_parts_table(p: &mut Project) -> u64 {
    let id = p.add_table(TableKind::PartsOfWhole, "人群构成");
    let t = p.table_mut(id).unwrap();
    let labels = ["18–29 岁", "30–44 岁", "45–59 岁", "60–74 岁", "75 岁以上"];
    let shares = [21.0, 27.0, 24.0, 18.0, 10.0];
    // 整表重建：`add_table` 会先塞一列空的默认列，留着会让饼图多出一个空扇区。
    let tname = t.name.clone();
    *t = DataTable {
        id,
        name: tname,
        kind: TableKind::PartsOfWhole,
        columns: Vec::new(),
        group_names: vec!["占比".into()],
        row_headers: labels.iter().map(|s| s.to_string()).collect(),
        row_header_width: None,
        notes: "互斥类别的合计；可直接用于饼图 / 环形图。".into(),
    };
    let mut c = Column::new("占比", ColRole::Y);
    c.cells = shares.iter().map(|v| Cell::number(*v)).collect();
    t.columns.push(c);
    t.ensure_rows(labels.len());
    id
}

/// 追加一张指定图型的示例图（示例工程用来展示新图型）。
fn build_extra_graph(p: &mut Project, table: u64, name: &str, kind: ChartKind, title: &str) -> u64 {
    let id = p.add_graph_for(table);
    if let Some(g) = p.graph_mut(id) {
        g.name = name.into();
        g.spec.kind = kind;
        g.spec.title = title.into();
        g.spec.show_legend = true;
    }
    id
}

fn build_dose_table(p: &mut Project) -> u64 {
    let id = p.add_table(TableKind::Xy, "剂量–反应");
    let t = p.table_mut(id).unwrap();
    let xs = [0.1, 0.3, 1.0, 3.0, 10.0, 30.0];
    let groups = [
        ("Vehicle", 5.0, 30.0, 3.1),
        ("低剂量", 8.0, 64.0, 3.0),
        ("高剂量", 10.0, 92.0, 2.9),
    ];
    let tname = t.name.clone();
    *t = DataTable {
        id,
        name: tname,
        kind: TableKind::Xy,
        columns: Vec::new(),
        group_names: groups.iter().map(|g| g.0.to_string()).collect(),
        row_headers: Vec::new(),
        row_header_width: None,
        notes: "X = 浓度 (µM)，每行 3 个重复。".into(),
    };
    let mut x = Column::new("浓度 (µM)", ColRole::X);
    x.cells = xs.iter().map(|v| Cell::number(*v)).collect();
    t.columns.push(x);
    let mut rng = Rng::new(20261110);
    for (g, (_, bottom, top, noise)) in groups.iter().enumerate() {
        let ec50 = 2.6;
        for rep in 0..3 {
            let mut c = Column::new(format!("Y{}", rep + 1), ColRole::Y);
            c.group = g;
            c.cells = xs
                .iter()
                .map(|&x| {
                    let y = bottom + (top - bottom) / (1.0 + (ec50 / x).powf(1.25));
                    Cell::number((y + rng.gauss() * noise).max(0.5))
                })
                .collect();
            t.columns.push(c);
        }
    }
    t.ensure_rows(xs.len());
    id
}

fn build_weight_table(p: &mut Project) -> u64 {
    let id = p.add_table(TableKind::Column, "体重变化");
    let t = p.table_mut(id).unwrap();
    let names = ["Control", "低剂量", "高剂量", "阳性药"];
    let means = [12.4, 18.1, 25.8, 30.2];
    let sds = [3.8, 4.2, 4.6, 3.3];
    let n = 10;
    for (g, name) in names.iter().enumerate() {
        t.columns[g].name = (*name).into();
        t.group_names[g] = (*name).into();
    }
    t.row_headers = (1..=n).map(|i| format!("{i}")).collect();
    t.ensure_rows(n);
    let mut rng = Rng::new(77_001);
    for (g, (m, s)) in means.iter().zip(sds.iter()).enumerate() {
        for row in 0..n {
            t.set_cell(row, g, Cell::number(m + rng.gauss() * s));
        }
    }
    t.notes = "每列一个组，n = 10；用于 t 检验与单因素 ANOVA。".into();
    id
}

fn build_survival_table(p: &mut Project) -> u64 {
    let id = p.add_table(TableKind::Survival, "生存分析");
    let t = p.table_mut(id).unwrap();
    let control = [
        (4.0, 1.0),
        (6.0, 1.0),
        (8.0, 0.0),
        (10.0, 1.0),
        (12.0, 0.0),
        (15.0, 1.0),
        (18.0, 0.0),
    ];
    let treated = [
        (9.0, 1.0),
        (12.0, 0.0),
        (15.0, 1.0),
        (18.0, 0.0),
        (22.0, 1.0),
        (26.0, 0.0),
        (30.0, 0.0),
    ];
    let n = control.len() + treated.len();
    let tname = t.name.clone();
    *t = DataTable {
        id,
        name: tname,
        kind: TableKind::Survival,
        columns: Vec::new(),
        group_names: vec!["Control".into(), "Treated".into()],
        row_headers: (1..=n).map(|i| format!("S{i}")).collect(),
        row_header_width: None,
        notes: "1 = 事件，0 = 删失；两组各 7 只。".into(),
    };
    let mut time = Column::new("时间 (天)", ColRole::X);
    time.cells = vec![Cell::empty(); n];
    t.columns.push(time);
    for g in 0..2 {
        let mut c = Column::new(if g == 0 { "Control" } else { "Treated" }, ColRole::Y);
        c.group = g;
        c.cells = vec![Cell::empty(); n];
        t.columns.push(c);
    }
    for (i, (time, event)) in control.iter().enumerate() {
        t.set_cell(i, 0, Cell::number(*time));
        t.set_cell(i, 1, Cell::number(*event));
    }
    for (i, (time, event)) in treated.iter().enumerate() {
        let row = control.len() + i;
        t.set_cell(row, 0, Cell::number(*time));
        t.set_cell(row, 2, Cell::number(*event));
    }
    id
}

fn build_grouped_table(p: &mut Project) -> u64 {
    let id = p.add_table(TableKind::Grouped, "性别 × 治疗");
    let t = p.table_mut(id).unwrap();
    let data = [
        [62.0, 71.0, 78.0],
        [58.0, 66.0, 74.0],
        [64.0, 73.0, 80.0],
        [59.0, 68.0, 75.0],
        [61.0, 70.0, 77.0],
        [57.0, 65.0, 72.0],
        [63.0, 72.0, 79.0],
        [60.0, 67.0, 73.0],
    ];
    let n = data.len();
    t.row_headers = vec![
        "男性".into(),
        "女性".into(),
        "男性".into(),
        "女性".into(),
        "男性".into(),
        "女性".into(),
        "男性".into(),
        "女性".into(),
    ];
    t.ensure_rows(n);
    for row in 0..n {
        for g in 0..3 {
            t.set_cell(row, g, Cell::number(data[row][g]));
        }
    }
    t.notes = "行为重复样本，列为三个处理组；分组柱展示均值 ± SEM。".into();
    id
}

fn build_dose_graph(p: &mut Project, table: u64) -> u64 {
    let id = p.add_graph_for(table);
    let g = p.graph_mut(id).unwrap();
    g.name = "剂量–反应曲线".into();
    g.spec = GraphSpec {
        kind: ChartKind::Line,
        title: "剂量–反应".into(),
        show_title: true,
        x_title: "浓度 (µM)".into(),
        y_title: "响应 (% of max)".into(),
        theme: PlotThemeId::Aura,
        palette: PaletteId::Aura,
        x_log: true,
        y_log: false,
        show_legend: true,
        legend_pos: LegendPos::InsideTopLeft,
        error: ErrorKind::Sem,
        show_points: true,
        jitter: false,
        point_size: 5.6,
        line_width: 2.0,
        bar_width: 0.62,
        show_grid: false,
        marker: crate::core::graph::Marker::Circle,
        histogram_bins: 0,
        annotations: Vec::new(),
        x_range: None,
        y_range: Some([0.0, 100.0]),
        ..Default::default()
    };
    id
}

fn build_weight_graph(p: &mut Project, table: u64) -> u64 {
    let id = p.add_graph_for(table);
    let g = p.graph_mut(id).unwrap();
    g.name = "体重变化 · 柱状图".into();
    g.spec = GraphSpec {
        kind: ChartKind::Column,
        title: "药物对体重变化的影响".into(),
        show_title: true,
        x_title: String::new(),
        y_title: "体重变化 (g)".into(),
        theme: PlotThemeId::Aura,
        palette: PaletteId::Aura,
        x_log: false,
        y_log: false,
        show_legend: false,
        legend_pos: LegendPos::InsideTopRight,
        error: ErrorKind::Sem,
        show_points: true,
        jitter: true,
        point_size: 4.6,
        line_width: 1.6,
        bar_width: 0.58,
        show_grid: true,
        marker: crate::core::graph::Marker::Circle,
        histogram_bins: 0,
        annotations: vec![
            SigAnnotation {
                a: 0,
                b: 2,
                label: "***".into(),
                y: None,
            },
            SigAnnotation {
                a: 0,
                b: 3,
                label: "****".into(),
                y: None,
            },
            SigAnnotation {
                a: 1,
                b: 2,
                label: "*".into(),
                y: None,
            },
        ],
        x_range: None,
        y_range: None,
        ..Default::default()
    };
    id
}

fn build_survival_graph(p: &mut Project, table: u64) -> u64 {
    let id = p.add_graph_for(table);
    let g = p.graph_mut(id).unwrap();
    g.name = "生存曲线".into();
    g.spec = GraphSpec {
        kind: ChartKind::Survival,
        title: "Kaplan–Meier 生存曲线".into(),
        show_title: true,
        x_title: "时间 (天)".into(),
        y_title: "存活比例".into(),
        theme: PlotThemeId::Aura,
        palette: PaletteId::Aura,
        x_log: false,
        y_log: false,
        show_legend: true,
        legend_pos: LegendPos::InsideTopRight,
        error: ErrorKind::None,
        show_points: false,
        jitter: false,
        point_size: 5.0,
        line_width: 2.2,
        bar_width: 0.6,
        show_grid: false,
        marker: crate::core::graph::Marker::Circle,
        histogram_bins: 0,
        annotations: Vec::new(),
        x_range: Some([0.0, 32.0]),
        y_range: Some([0.0, 1.02]),
        ..Default::default()
    };
    id
}

fn build_grouped_graph(p: &mut Project, table: u64) -> u64 {
    let id = p.add_graph_for(table);
    let g = p.graph_mut(id).unwrap();
    g.name = "性别 × 治疗".into();
    g.spec = GraphSpec {
        kind: ChartKind::GroupedBar,
        title: "性别与治疗交互".into(),
        show_title: true,
        x_title: String::new(),
        y_title: "评分".into(),
        theme: PlotThemeId::Aura,
        palette: PaletteId::Aura,
        x_log: false,
        y_log: false,
        show_legend: true,
        legend_pos: LegendPos::InsideTopRight,
        error: ErrorKind::Sem,
        show_points: true,
        jitter: true,
        point_size: 3.6,
        line_width: 1.6,
        bar_width: 0.72,
        show_grid: true,
        marker: crate::core::graph::Marker::Circle,
        histogram_bins: 0,
        annotations: Vec::new(),
        x_range: None,
        y_range: None,
        ..Default::default()
    };
    id
}
