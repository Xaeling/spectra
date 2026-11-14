use serde::{Deserialize, Serialize};

use crate::core::graph::GraphSpec;

pub const FORMAT_VERSION: u32 = 1;

/// Prism 的七种数据表。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TableKind {
    Xy,
    Column,
    Grouped,
    Contingency,
    Survival,
    PartsOfWhole,
    MultipleVariables,
    /// 两层嵌套 / 层级重复：外层分组包住内层子组，重复值纵向堆叠在子列里。
    Nested,
}

/// 子列（重复值）的排列方式 —— 决定表格「长相」的核心。
///
/// 口径来自 Prism 官方说明：XY / Grouped 的重复值横向并排，
/// Nested 的重复值纵向堆叠进子列，Column 表没有子列（重复值直接堆在列里），
/// 其余表型根本不录入重复值。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubLayout {
    /// 无子列
    None,
    /// 并排：重复值横向排开（XY、Grouped）
    SideBySide,
    /// 堆叠：重复值纵向堆叠（Nested）
    Stacked,
}

impl TableKind {
    pub const ALL: [TableKind; 8] = [
        Self::Xy,
        Self::Column,
        Self::Grouped,
        Self::Contingency,
        Self::Survival,
        Self::PartsOfWhole,
        Self::MultipleVariables,
        Self::Nested,
    ];

    // ---- 表格格式规则（单一事实来源）----------------------------------
    // 网格的列头、行标题、单元格输入校验都从这里取，不再各写一套 if。

    /// 子列排列方式。
    pub fn sub_layout(self) -> SubLayout {
        match self {
            Self::Xy | Self::Grouped => SubLayout::SideBySide,
            Self::Nested => SubLayout::Stacked,
            _ => SubLayout::None,
        }
    }

    /// 是否有子列（只有 XY / Grouped / Nested）。
    pub fn has_subcolumns(self) -> bool {
        self.sub_layout() != SubLayout::None
    }

    /// 首列是否为 X 列（XY 与 Survival）。
    pub fn first_is_x(self) -> bool {
        matches!(self, Self::Xy | Self::Survival)
    }

    /// 是否显示行标题列。Column / Survival / MultipleVariables 没有行标题。
    pub fn has_row_titles(self) -> bool {
        !matches!(
            self,
            Self::Column | Self::Survival | Self::MultipleVariables
        )
    }

    /// 单元格是否只允许整数（列联表存的是频数）。
    pub fn integer_only(self) -> bool {
        matches!(self, Self::Contingency)
    }

    /// 单元格是否允许文本（多变量表的分类变量 / 标签变量）。
    pub fn allows_text(self) -> bool {
        matches!(self, Self::MultipleVariables)
    }

    /// 该表型能否新增列 / 组（列联表与构成表的结构固定）。
    pub fn can_add_column(self) -> bool {
        !matches!(self, Self::PartsOfWhole | Self::Survival)
    }

    /// 校验说明，供输入框下方提示。
    pub fn input_rule(self) -> &'static str {
        match self {
            Self::Contingency => "列联表只接受非负整数频数",
            Self::Survival => "时间列填数值；结局列填 1（事件）或 0（删失）",
            Self::MultipleVariables => "每列一个变量，可填数值或文本标签",
            Self::Nested => "重复值纵向堆叠进子列；外层分组包住内层子组",
            _ => "数值单元格；右键可排除异常值",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Xy => "XY 表",
            Self::Column => "柱状表",
            Self::Grouped => "分组表",
            Self::Contingency => "列联表",
            Self::Survival => "生存表",
            Self::PartsOfWhole => "构成表",
            Self::MultipleVariables => "多变量表",
            Self::Nested => "嵌套表",
        }
    }

    pub fn en(self) -> &'static str {
        match self {
            Self::Xy => "XY",
            Self::Column => "Column",
            Self::Grouped => "Grouped",
            Self::Contingency => "Contingency",
            Self::Survival => "Survival",
            Self::PartsOfWhole => "Parts of whole",
            Self::MultipleVariables => "Multiple variables",
            Self::Nested => "Nested",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Xy => "X 与一个或多个 Y。适合时间序列、剂量-反应、相关与回归。",
            Self::Column => "每列一个组，重复测量纵向堆叠。适合 t 检验、ANOVA、箱线图。",
            Self::Grouped => "两个分组变量交叉。行为组 1，列为组 2。适合双因素比较与分组柱。",
            Self::Contingency => "分类计数。适合卡方、Fisher 精确检验。",
            Self::Survival => "时间 + 事件状态。适合 Kaplan–Meier 曲线。",
            Self::PartsOfWhole => "互斥类别求和。适合饼图、构成图。",
            Self::MultipleVariables => {
                "每个案例一行、每个变量一列，变量可为文本。适合相关与多变量分析。"
            }
            Self::Nested => {
                "外层分组包住内层子组，重复值纵向堆叠进子列。适合嵌套 t 检验 / 嵌套 ANOVA。"
            }
        }
    }
}

/// 单元格：数值 / 文本 / 空，外加 Prism 的「排除」标记。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Cell {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub excluded: bool,
}

fn is_false(v: &bool) -> bool {
    !*v
}

impl Cell {
    pub fn empty() -> Self {
        Self::default()
    }
    pub fn number(v: f64) -> Self {
        Self {
            num: Some(v),
            ..Default::default()
        }
    }
    pub fn label(v: impl Into<String>) -> Self {
        Self {
            text: Some(v.into()),
            ..Default::default()
        }
    }
    pub fn is_empty(&self) -> bool {
        self.num.is_none() && self.text.is_none()
    }
    pub fn value(&self) -> Option<f64> {
        if self.excluded {
            None
        } else {
            self.num.filter(|v| v.is_finite())
        }
    }
    pub fn raw_value(&self) -> Option<f64> {
        self.num.filter(|v| v.is_finite())
    }
    pub fn display(&self) -> String {
        if let Some(n) = self.num {
            format_number(n)
        } else if let Some(t) = &self.text {
            t.clone()
        } else {
            String::new()
        }
    }
}

pub fn format_number(n: f64) -> String {
    if !n.is_finite() {
        return "—".into();
    }
    if n == 0.0 {
        return "0".into();
    }
    let abs = n.abs();
    if (1e-4..1e7).contains(&abs) {
        let mut s = format!("{:.4}", n);
        while s.contains('.') && s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
        s
    } else {
        let s = format!("{:.4e}", n);
        // 1.2300e-5 → 1.23e-5
        if let Some((m, e)) = s.split_once('e') {
            let m = m.trim_end_matches('0').trim_end_matches('.');
            format!("{m}e{e}")
        } else {
            s
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColRole {
    X,
    Y,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    #[serde(default)]
    pub group: usize,
    pub role: ColRole,
    pub cells: Vec<Cell>,
    /// 计算列公式（如 `{体重}/({身高}^2)`）。有公式时 cells 由 `recompute()` 生成。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula: Option<String>,
    /// 列宽（像素）。`None` = 用默认宽；拖拽调整后持久化到工程文件。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
}

impl Column {
    pub fn new(name: impl Into<String>, role: ColRole) -> Self {
        Self {
            name: name.into(),
            group: 0,
            role,
            cells: Vec::new(),
            formula: None,
            width: None,
        }
    }
}

/// 一张数据表。
///
/// 统一约定：XY / Survival 的第 0 列是 X（`role == X`），其余为 Y；
/// 其他表型没有 X 列，行标题单独存在 `row_headers`。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DataTable {
    pub id: u64,
    pub name: String,
    pub kind: TableKind,
    pub columns: Vec<Column>,
    /// 组 / 数据集名称（XY 的 Group A，Grouped 的列条件…）
    #[serde(default)]
    pub group_names: Vec<String>,
    /// 行标题：分组表的组 1、构成表的类别名、生存表的受试者编号…
    #[serde(default)]
    pub row_headers: Vec<String>,
    /// 行标题列的宽度（像素）。`None` = 用默认宽。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_header_width: Option<f32>,
    #[serde(default)]
    pub notes: String,
}

impl DataTable {
    pub fn template(kind: TableKind, name: impl Into<String>, id: u64) -> Self {
        let mut t = Self {
            id,
            name: name.into(),
            kind,
            columns: Vec::new(),
            group_names: Vec::new(),
            row_headers: Vec::new(),
            row_header_width: None,
            notes: String::new(),
        };
        match kind {
            TableKind::Xy => {
                // 新建表一律空白：X 列不预填数据，用户自己录入/粘贴。
                let mut x = Column::new("X", ColRole::X);
                x.cells = vec![Cell::empty(); 8];
                t.columns.push(x);
                t.group_names = vec!["Group A".into(), "Group B".into()];
                for (g, _) in t.group_names.clone().iter().enumerate() {
                    let mut c = Column::new("Y1", ColRole::Y);
                    c.group = g;
                    c.cells = vec![Cell::empty(); 8];
                    t.columns.push(c);
                }
                t.row_headers = Vec::new();
            }
            TableKind::Column => {
                for g in 0..4 {
                    let mut c =
                        Column::new(format!("Group {}", (b'A' + g as u8) as char), ColRole::Y);
                    c.group = g;
                    c.cells = vec![Cell::empty(); 8];
                    t.columns.push(c);
                }
                t.group_names = t.columns.iter().map(|c| c.name.clone()).collect();
                t.row_headers = (1..=8).map(|i| format!("Subject {i}")).collect();
            }
            TableKind::Grouped => {
                t.row_headers = vec!["Male".into(), "Female".into()];
                for (g, name) in ["Control", "Drug A", "Drug B"].iter().enumerate() {
                    let mut c = Column::new(*name, ColRole::Y);
                    c.group = g;
                    c.cells = vec![Cell::empty(); 2];
                    t.columns.push(c);
                }
                t.group_names = t.columns.iter().map(|c| c.name.clone()).collect();
            }
            TableKind::Contingency => {
                t.row_headers = vec!["Yes".into(), "No".into()];
                for name in ["Control", "Treated"] {
                    let mut c = Column::new(name, ColRole::Y);
                    c.cells = vec![Cell::empty(); 2];
                    t.columns.push(c);
                }
                t.group_names = t.columns.iter().map(|c| c.name.clone()).collect();
            }
            TableKind::Survival => {
                let mut x = Column::new("Time", ColRole::X);
                x.cells = vec![Cell::empty(); 10];
                t.columns.push(x);
                // 每列一个组，group 必须各自独立——之前全默认 group=0，
                // 导致 Treated 的显示名也走 group_name(0)="Control"，改名全乱。
                for (g, name) in ["Control", "Treated"].iter().enumerate() {
                    let mut c = Column::new(*name, ColRole::Y);
                    c.group = g;
                    c.cells = vec![Cell::empty(); 10];
                    t.columns.push(c);
                }
                t.group_names = vec!["Control".into(), "Treated".into()];
                t.row_headers = (1..=10).map(|i| format!("S{i}")).collect();
            }
            TableKind::PartsOfWhole => {
                let mut c = Column::new("Value", ColRole::Y);
                c.cells = vec![Cell::empty(); 5];
                t.columns.push(c);
                t.row_headers = ["A", "B", "C", "D", "E"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
            }
            TableKind::MultipleVariables => {
                for name in ["Age", "BMI", "Score"] {
                    let mut c = Column::new(name, ColRole::Y);
                    c.cells = vec![Cell::empty(); 6];
                    t.columns.push(c);
                }
                t.group_names = t.columns.iter().map(|c| c.name.clone()).collect();
                t.row_headers = (1..=6).map(|i| format!("Case {i}")).collect();
            }
            TableKind::Nested => {
                // 外层 2 个分组 × 每层 2 个子组；每个子组用 2 个堆叠子列
                // 表示重复测量。行标题写内层子组名。
                t.group_names = vec!["方法 A".into(), "方法 B".into()];
                for (g, gname) in t.group_names.clone().iter().enumerate() {
                    for (sub, sname) in ["班级 1", "班级 2"].iter().enumerate() {
                        let mut c = Column::new(format!("{gname} · {sname}"), ColRole::Y);
                        c.group = g;
                        c.cells = vec![Cell::empty(); 6];
                        t.columns.push(c);
                        let _ = sub;
                    }
                }
                t.row_headers = (1..=6).map(|i| format!("重复 {i}")).collect();
            }
        }
        t.ensure_rows(t.nrows());
        t
    }

    pub fn nrows(&self) -> usize {
        let mut n = self.row_headers.len();
        for c in &self.columns {
            n = n.max(c.cells.len());
        }
        n
    }

    pub fn ensure_rows(&mut self, n: usize) {
        if self.row_headers.len() < n {
            let start = self.row_headers.len();
            self.row_headers.extend((start..n).map(|_| String::new()));
        }
        for c in &mut self.columns {
            if c.cells.len() < n {
                c.cells.resize(n, Cell::empty());
            }
        }
    }

    pub fn x_index(&self) -> Option<usize> {
        self.columns.iter().position(|c| c.role == ColRole::X)
    }

    pub fn x_values(&self) -> Vec<f64> {
        match self.x_index() {
            Some(i) => self.columns[i]
                .cells
                .iter()
                .map(|c| c.raw_value().unwrap_or(f64::NAN))
                .collect(),
            None => Vec::new(),
        }
    }

    pub fn set_cell(&mut self, row: usize, col: usize, cell: Cell) {
        if col >= self.columns.len() {
            return;
        }
        if row >= self.columns[col].cells.len() {
            self.ensure_rows(row + 1);
        }
        self.columns[col].cells[row] = cell;
    }

    pub fn cell(&self, row: usize, col: usize) -> Option<&Cell> {
        self.columns.get(col)?.cells.get(row)
    }

    pub fn set_excluded(&mut self, row: usize, col: usize, excluded: bool) {
        if let Some(cell) = self.columns.get_mut(col).and_then(|c| c.cells.get_mut(row)) {
            cell.excluded = excluded;
        }
    }

    /// 一个组（数据列）的可用数值，自动排除空白与排除值。
    pub fn column_values(&self, col: usize) -> Vec<f64> {
        self.columns
            .get(col)
            .map(|c| c.cells.iter().filter_map(|v| v.value()).collect())
            .unwrap_or_default()
    }

    /// 嵌套数据：`(外层组名, [每个内层子组的重复值])`。
    ///
    /// 列按 `group` 归到外层组，同组内的**每一列**就是一个内层子组（班级），
    /// 列内的行是该子组的重复测量。空子组被剔除，方便直接喂给嵌套检验。
    pub fn nested_groups(&self) -> Vec<(String, Vec<Vec<f64>>)> {
        let mut out: Vec<(String, Vec<Vec<f64>>)> = Vec::new();
        for c in self.columns.iter().filter(|c| c.role == ColRole::Y) {
            let name = self.group_name(c.group);
            let vals: Vec<f64> = c
                .cells
                .iter()
                .filter_map(|v| v.value())
                .filter(|v| v.is_finite())
                .collect();
            if vals.is_empty() {
                continue;
            }
            match out.iter_mut().find(|(g, _)| g == &name) {
                Some((_, subs)) => subs.push(vals),
                None => out.push((name, vec![vals])),
            }
        }
        out
    }

    /// 列联表：`(行标题, 列标题, 计数矩阵)`。
    ///
    /// 行标题取 `row_headers`（空则用行号），列标题取列名。
    /// 空单元格记 0（列联表里空白即频数 0）。
    pub fn contingency_matrix(&self) -> Option<(Vec<String>, Vec<String>, Vec<Vec<f64>>)> {
        let cols: Vec<&Column> = self
            .columns
            .iter()
            .filter(|c| c.role == ColRole::Y)
            .collect();
        if cols.is_empty() || self.nrows() == 0 {
            return None;
        }
        let rows = self.nrows();
        let row_labels: Vec<String> = (0..rows)
            .map(|r| {
                self.row_headers
                    .get(r)
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .unwrap_or_else(|| format!("行 {}", r + 1))
            })
            .collect();
        let col_labels: Vec<String> = cols.iter().map(|c| c.name.clone()).collect();
        let counts: Vec<Vec<f64>> = (0..rows)
            .map(|r| {
                cols.iter()
                    .map(|c| c.cells.get(r).and_then(|x| x.value()).unwrap_or(0.0))
                    .map(|v| if v.is_finite() && v > 0.0 { v } else { 0.0 })
                    .collect()
            })
            .collect();
        Some((row_labels, col_labels, counts))
    }

    pub fn group_indices(&self) -> Vec<usize> {
        let mut out = Vec::new();
        for c in self.columns.iter() {
            if c.role == ColRole::Y && !out.contains(&c.group) {
                out.push(c.group);
            }
        }
        out
    }

    pub fn group_name(&self, group: usize) -> String {
        self.group_names
            .get(group)
            .cloned()
            .or_else(|| {
                self.columns
                    .iter()
                    .find(|c| c.group == group && c.role == ColRole::Y)
                    .map(|c| c.name.clone())
            })
            .unwrap_or_else(|| format!("Group {}", group + 1))
    }

    /// 统计分析的组：XY 按数据集合并重复列；其余表一列一组。
    /// 重算所有计算列（`Column.formula` 非空的列）。
    ///
    /// 每次表格数值变更后调用（网格编辑、粘贴、导入）；按行求值，
    /// 求值失败或非有限数的格子置空——**不写 NaN**，下游统计自然跳过。
    pub fn recompute(&mut self) {
        for ci in 0..self.columns.len() {
            let Some(formula) = self.columns[ci].formula.clone() else {
                continue;
            };
            // 源列快照（名字 → 每行数值）：避免求值期间借住 self.columns
            let sources: Vec<(String, Vec<Option<f64>>)> = self
                .columns
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != ci)
                .map(|(_, c)| (c.name.clone(), c.cells.iter().map(|v| v.num).collect()))
                .collect();
            let nrows = self
                .nrows()
                .max(sources.iter().map(|(_, v)| v.len()).max().unwrap_or(0));
            let mut out = Vec::with_capacity(nrows);
            for row in 0..nrows {
                let resolve = |name: &str| {
                    sources
                        .iter()
                        .find(|(n, _)| n == name)
                        .and_then(|(_, v)| v.get(row).copied().flatten())
                };
                match crate::core::expr::eval(&formula, &resolve) {
                    Ok(v) => out.push(Cell::number(v)),
                    Err(_) => out.push(Cell::empty()),
                }
            }
            self.columns[ci].cells = out;
        }
    }

    /// 新增一个计算列：先校验公式能求值（第一行），再挂列并整列重算。
    pub fn add_computed_column(&mut self, name: &str, formula: &str) -> Result<usize, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("列名不能为空".into());
        }
        if self.columns.iter().any(|c| c.name == name) {
            return Err(format!("已存在名为「{name}」的列"));
        }
        // 试算：用第一行有值的数据验证公式（空表只校验语法）
        let sources: Vec<(String, Vec<Option<f64>>)> = self
            .columns
            .iter()
            .map(|c| (c.name.clone(), c.cells.iter().map(|v| v.num).collect()))
            .collect();
        let row = (0..self.nrows()).find(|i| {
            sources
                .iter()
                .any(|(_, v)| v.get(*i).copied().flatten().is_some())
        });
        if let Some(row) = row {
            let resolve = |n: &str| {
                sources
                    .iter()
                    .find(|(nm, _)| nm == n)
                    .and_then(|(_, v)| v.get(row).copied().flatten())
            };
            crate::core::expr::eval(formula, &resolve)?;
        } else {
            // 空表：只做语法校验（借一个恒返回 1 的解析器不足以验列名，跳过列名校验）
            crate::core::expr::eval(formula, &|_| Some(1.0))?;
        }
        let mut col = Column::new(name, ColRole::Y);
        col.formula = Some(formula.trim().to_string());
        self.columns.push(col);
        self.recompute();
        Ok(self.columns.len() - 1)
    }

    pub fn analysis_groups(&self) -> Vec<(String, Vec<f64>)> {
        match self.kind {
            TableKind::Xy | TableKind::Nested => {
                let mut groups: Vec<(usize, Vec<f64>)> = Vec::new();
                for c in self.columns.iter().filter(|c| c.role == ColRole::Y) {
                    let entry = match groups.iter_mut().find(|(g, _)| *g == c.group) {
                        Some(e) => e,
                        None => {
                            groups.push((c.group, Vec::new()));
                            groups.last_mut().unwrap()
                        }
                    };
                    entry.1.extend(c.cells.iter().filter_map(|v| v.value()));
                }
                groups
                    .into_iter()
                    .map(|(g, v)| (self.group_name(g), v))
                    .collect()
            }
            _ => self
                .columns
                .iter()
                .filter(|c| c.role == ColRole::Y)
                .map(|c| {
                    (
                        if self
                            .group_names
                            .get(c.group)
                            .map(|s| !s.is_empty())
                            .unwrap_or(false)
                            && self.kind != TableKind::Column
                        {
                            self.group_name(c.group)
                        } else {
                            c.name.clone()
                        },
                        c.cells.iter().filter_map(|v| v.value()).collect(),
                    )
                })
                .collect(),
        }
    }

    /// XY 表中某个数据集的 (x, y_mean) 点；重复子列取行均值。
    pub fn xy_points(&self, group: usize) -> Vec<(f64, f64)> {
        let Some(xi) = self.x_index() else {
            return Vec::new();
        };
        let xs = &self.columns[xi].cells;
        let cols: Vec<&Column> = self
            .columns
            .iter()
            .filter(|c| c.role == ColRole::Y && c.group == group)
            .collect();
        let mut pts = Vec::new();
        for row in 0..self.nrows() {
            let Some(x) = xs.get(row).and_then(|c| c.value()) else {
                continue;
            };
            let ys: Vec<f64> = cols
                .iter()
                .filter_map(|c| c.cells.get(row).and_then(|v| v.value()))
                .collect();
            if ys.is_empty() {
                continue;
            }
            let mean = ys.iter().sum::<f64>() / ys.len() as f64;
            pts.push((x, mean));
        }
        pts
    }

    /// 某个组的**全部原始观测** (x, y)，不做行内聚合。
    ///
    /// `xy_rows` 会把同一行的多个重复列压成均值 ± SD，那适合画「均值带误差」；
    /// 但散点图和回归拟合必须用原始观测——用均值拟合会人为抹掉组内变异，
    /// R² 虚高、置信带偏窄，图上也会少掉大半的点。
    pub fn xy_observations(&self, group: usize) -> Vec<(f64, f64)> {
        let Some(xi) = self.x_index() else {
            return Vec::new();
        };
        let xs = &self.columns[xi].cells;
        let cols: Vec<&Column> = self
            .columns
            .iter()
            .filter(|c| c.role == ColRole::Y && c.group == group)
            .collect();
        let mut out = Vec::new();
        for row in 0..self.nrows() {
            let Some(x) = xs.get(row).and_then(|c| c.value()) else {
                continue;
            };
            for c in &cols {
                if let Some(y) = c.cells.get(row).and_then(|v| v.value()) {
                    out.push((x, y));
                }
            }
        }
        out
    }

    /// XY 某一组在每行的均值、SD 与有效重复数（用于误差棒）。
    pub fn xy_rows(&self, group: usize) -> Vec<(f64, f64, Option<f64>, usize)> {
        let Some(xi) = self.x_index() else {
            return Vec::new();
        };
        let xs = &self.columns[xi].cells;
        let cols: Vec<&Column> = self
            .columns
            .iter()
            .filter(|c| c.role == ColRole::Y && c.group == group)
            .collect();
        let mut out = Vec::new();
        for row in 0..self.nrows() {
            let Some(x) = xs.get(row).and_then(|c| c.value()) else {
                continue;
            };
            let ys: Vec<f64> = cols
                .iter()
                .filter_map(|c| c.cells.get(row).and_then(|v| v.value()))
                .collect();
            if ys.is_empty() {
                continue;
            }
            let mean = ys.iter().sum::<f64>() / ys.len() as f64;
            let sd = if ys.len() >= 2 {
                let var =
                    ys.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (ys.len() - 1) as f64;
                Some(var.sqrt())
            } else {
                None
            };
            out.push((x, mean, sd, ys.len()));
        }
        out
    }

    pub fn parts(&self) -> Vec<(String, f64)> {
        let Some(col) = self.columns.first() else {
            return Vec::new();
        };
        col.cells
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                let v = c.value()?;
                let label = self
                    .row_headers
                    .get(i)
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .unwrap_or_else(|| format!("{}", i + 1));
                Some((label, v))
            })
            .collect()
    }

    /// 生存：每个状态列一组 (time, event)。
    pub fn survival_groups(&self) -> Vec<(String, Vec<(f64, bool)>)> {
        let Some(xi) = self.x_index() else {
            return Vec::new();
        };
        let times = self.columns[xi].cells.clone();
        self.columns
            .iter()
            .filter(|c| c.role == ColRole::Y)
            .map(|c| {
                let mut v = Vec::new();
                for row in 0..self.nrows() {
                    let Some(t) = times.get(row).and_then(|c| c.value()) else {
                        continue;
                    };
                    let Some(code) = c.cells.get(row).and_then(|x| x.value()) else {
                        continue;
                    };
                    v.push((t, code > 0.5));
                }
                (c.name.clone(), v)
            })
            .collect()
    }

    pub fn matrix(&self) -> Vec<Vec<f64>> {
        (0..self.nrows())
            .map(|r| {
                self.columns
                    .iter()
                    .filter(|c| c.role == ColRole::Y)
                    .map(|c| c.cells.get(r).and_then(|v| v.value()).unwrap_or(f64::NAN))
                    .collect()
            })
            .collect()
    }

    pub fn col_index_of_group(&self, group: usize) -> Vec<usize> {
        self.columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.role == ColRole::Y && c.group == group)
            .map(|(i, _)| i)
            .collect()
    }

    pub fn is_xy(&self) -> bool {
        self.kind == TableKind::Xy
    }

    pub fn insert_row(&mut self, at: usize) {
        let at = at.min(self.row_headers.len());
        self.row_headers.insert(at, String::new());
        for c in &mut self.columns {
            if at <= c.cells.len() {
                c.cells.insert(at, Cell::empty());
            } else {
                c.cells.push(Cell::empty());
            }
        }
    }

    pub fn remove_row(&mut self, at: usize) {
        if at < self.row_headers.len() {
            self.row_headers.remove(at);
        }
        for c in &mut self.columns {
            if at < c.cells.len() {
                c.cells.remove(at);
            }
        }
    }

    /// 在第 `at` 列前插入一列（`at == len` 即追加到末尾）。
    ///
    /// 结构固定的表型（列联、构成、生存等）不允许插列——调用方应先查
    /// `TableKind::can_add_column()`。组号沿用被插入位置的列，保证分组不乱。
    pub fn insert_col(&mut self, at: usize, name: &str, role: ColRole) {
        let at = at.min(self.columns.len());
        let nrows = self.nrows();
        let group = self
            .columns
            .get(at)
            .map(|c| c.group)
            .or_else(|| self.columns.last().map(|c| c.group))
            .unwrap_or(0);
        let mut c = Column::new(name, role);
        c.group = group;
        c.cells = vec![Cell::empty(); nrows];
        self.columns.insert(at, c);
    }

    /// 删除第 `at` 列。X 列（`role == X`）不删——删了表型就不成立了。
    pub fn remove_col(&mut self, at: usize) {
        if at >= self.columns.len() {
            return;
        }
        if self.columns[at].role == ColRole::X {
            return;
        }
        self.columns.remove(at);
    }

    /// 删除整组：把 `group == gid` 的所有列一次删掉。
    ///
    /// 结构固定的表（每组子列数必须一致，如嵌套表 3 个 Y 一组）不能单列增删，
    /// 但**整组删**不破坏「每组 N 列」的不变量——这就是「删一大格」。
    pub fn remove_group(&mut self, gid: usize) {
        self.columns
            .retain(|c| c.group != gid || c.role == ColRole::X);
    }

    /// 设置单元格文本（粘贴 / 输入统一入口）。
    ///
    /// 按表格格式规则校验，避免把非法值悄悄存进表里：
    /// - 列联表只允许**非负整数**频数；
    /// - 生存表的 Y 列只允许 0（删失）/ 1（事件）；
    /// - 除多变量表外不接受文本（存成文本会破坏统计与绘图）。
    /// 非法输入保留为空，由 UI 的提示告诉用户原因。
    pub fn set_text(&mut self, row: usize, col: usize, text: &str) -> bool {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            self.set_cell(row, col, Cell::empty());
            return true;
        }
        let numeric = trimmed.replace(',', "").parse::<f64>().ok();

        // 多变量表允许文本型的分类 / 标签变量。
        if numeric.is_none() {
            if self.kind.allows_text() && !self.kind.integer_only() {
                self.set_cell(row, col, Cell::label(trimmed));
                return true;
            }
            self.set_cell(row, col, Cell::empty());
            return false;
        }
        let mut v = numeric.unwrap();
        if self.kind.integer_only() {
            if v < 0.0 || (v - v.round()).abs() > 1e-9 {
                self.set_cell(row, col, Cell::empty());
                return false;
            }
            v = v.round();
        }
        // 生存表的 Y 列是事件指示（0/1）；X 列是时间，允许任意非负数。
        if self.kind == TableKind::Survival && col != self.x_index().unwrap_or(0) {
            if v != 0.0 && v != 1.0 {
                self.set_cell(row, col, Cell::empty());
                return false;
            }
        }
        self.set_cell(row, col, Cell::number(v));
        true
    }

    /// 从 TSV 粘贴，返回影响到的行/列范围（用于选中反馈）。
    pub fn paste_tsv(
        &mut self,
        row: usize,
        col: usize,
        text: &str,
    ) -> (usize, usize, usize, usize) {
        let mut max_r = row;
        let mut max_c = col;
        // 按行切分：只剥掉**末尾那一个**换行造成的空串，
        // 中间/其它空行必须保留（否则整行数据会丢，行数变少）。
        let normalized = text.replace("\r\n", "\n");
        let mut lines: Vec<&str> = normalized.split('\n').collect();
        if matches!(lines.last(), Some(&"")) {
            lines.pop();
        }
        for (ri, line) in lines.iter().enumerate() {
            let r = row + ri;
            max_r = max_r.max(r);
            for (ci, field) in line.split('\t').enumerate() {
                let c = col + ci;
                if c >= self.columns.len() {
                    continue;
                }
                let _ = self.set_text(r, c, field);
                max_c = max_c.max(c);
            }
        }
        self.ensure_rows(max_r + 1);
        (row, col, max_r, max_c)
    }

    /// 唯一列名（复制组/列时用）。
    pub fn unique_group_name(&self, base: &str) -> String {
        let mut name = base.to_string();
        let mut i = 2;
        while self.group_names.iter().any(|n| n == &name) {
            name = format!("{base} {i}");
            i += 1;
        }
        name
    }

    pub fn add_group(&mut self, base_name: &str) {
        let name = self.unique_group_name(base_name);
        let group = self
            .columns
            .iter()
            .filter(|c| c.role == ColRole::Y)
            .map(|c| c.group + 1)
            .max()
            .unwrap_or(0);
        self.group_names.push(name.clone());
        let mut c = Column::new("Y1", ColRole::Y);
        c.group = group;
        c.cells = vec![Cell::empty(); self.nrows().max(1)];
        self.columns.push(c);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResultTable {
    pub title: String,
    pub method: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    #[serde(default)]
    pub note: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalysisKind {
    Descriptive,
    Normality,
    UnpairedT,
    PairedT,
    OneWayAnova,
    MannWhitney,
    Wilcoxon,
    Correlation,
    LinearRegression,
    /// 嵌套 t 检验（两外层组，子组为实验单位）
    NestedT,
    /// 嵌套单因素 ANOVA
    NestedAnova,
    /// 卡方独立性检验
    ChiSquare,
    /// Fisher 精确检验
    FisherExact,
    /// Kruskal–Wallis 秩方差分析
    KruskalWallis,
    /// Dunn 非参数事后两两比较
    Dunn,
    /// 双因素方差分析（行因子 × 列因子，格内重复）
    TwoWayAnova,
    /// 单因素重复测量方差分析（同一受试者跨处理）
    RmOneWay,
    /// ROC 曲线与 AUC（两组：阳性 vs 阴性）
    Roc,
    // ---- v0.13 补全 ----
    /// Welch t 检验（不等方差）
    WelchT,
    /// 单样本 t 检验（H₀: μ = 0）
    OneSampleT,
    /// 方差齐性（Bartlett / Levene / Brown–Forsythe）
    VarianceTests,
    /// 离群值（Grubbs）
    Outliers,
    /// Friedman 区组秩检验
    Friedman,
    /// 多重比较（Tukey / Games–Howell）
    MultiCompare,
    /// Spearman 秩相关
    Spearman,
    /// Deming 回归（X、Y 均有误差）
    Deming,
    /// 两条斜率比较
    SlopeCompare,
    /// 非线性拟合报告（AICc 排名）
    CurveFit,
    /// 相关矩阵
    CorrMatrix,
    /// 主成分分析
    Pca,
    /// 层次聚类
    Hierarchical,
    /// k 均值聚类
    KMeans,
    /// 比值比 / 相对风险
    EffectRatios,
    /// McNemar 配对检验
    McNemar,
    /// 多元线性回归
    MultiRegression,
    /// Cox 比例风险回归
    Cox,
    /// 单样本 Wilcoxon 符号秩（H₀: 中位数 = 0）
    SignedRank,
    /// 两样本 F 方差比
    FVariance,
    /// Cohen's Kappa（2×2 一致性）
    Kappa,
    /// Bland–Altman 一致性
    BlandAltman,
    /// 智能分析：按决策链给出该表型的全部推荐指标
    Smart,
}

/// 分析菜单的分组（按问题类型，而不是按表型）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalysisGroup {
    Smart,
    Describe,
    Compare,
    Correlate,
    Multivariate,
    Contingency,
    Survival,
}

impl AnalysisGroup {
    pub const ALL: [AnalysisGroup; 7] = [
        Self::Smart,
        Self::Describe,
        Self::Compare,
        Self::Correlate,
        Self::Multivariate,
        Self::Contingency,
        Self::Survival,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Smart => "智能分析",
            Self::Describe => "描述与分布",
            Self::Compare => "组间比较",
            Self::Correlate => "相关与回归",
            Self::Multivariate => "降维与聚类",
            Self::Contingency => "列联与配对",
            Self::Survival => "生存分析",
        }
    }
}

impl AnalysisKind {
    pub const ALL: [AnalysisKind; 41] = [
        Self::Smart,
        Self::Descriptive,
        Self::Normality,
        Self::UnpairedT,
        Self::PairedT,
        Self::OneWayAnova,
        Self::MannWhitney,
        Self::Wilcoxon,
        Self::Correlation,
        Self::LinearRegression,
        Self::NestedT,
        Self::NestedAnova,
        Self::ChiSquare,
        Self::FisherExact,
        Self::KruskalWallis,
        Self::Dunn,
        Self::TwoWayAnova,
        Self::RmOneWay,
        Self::Roc,
        Self::WelchT,
        Self::OneSampleT,
        Self::VarianceTests,
        Self::Outliers,
        Self::Friedman,
        Self::MultiCompare,
        Self::Spearman,
        Self::Deming,
        Self::SlopeCompare,
        Self::CurveFit,
        Self::CorrMatrix,
        Self::Pca,
        Self::Hierarchical,
        Self::KMeans,
        Self::EffectRatios,
        Self::McNemar,
        Self::MultiRegression,
        Self::Cox,
        Self::SignedRank,
        Self::FVariance,
        Self::Kappa,
        Self::BlandAltman,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Descriptive => "描述统计",
            Self::Normality => "正态性检验",
            Self::UnpairedT => "非配对 t 检验",
            Self::PairedT => "配对 t 检验",
            Self::OneWayAnova => "单因素方差分析",
            Self::MannWhitney => "Mann–Whitney U",
            Self::Wilcoxon => "Wilcoxon 符号秩",
            Self::Correlation => "相关性分析",
            Self::LinearRegression => "线性回归",
            Self::NestedT => "嵌套 t 检验",
            Self::NestedAnova => "嵌套 ANOVA",
            Self::ChiSquare => "卡方独立性检验",
            Self::FisherExact => "Fisher 精确检验",
            Self::KruskalWallis => "Kruskal–Wallis 检验",
            Self::Dunn => "Dunn 事后比较",
            Self::TwoWayAnova => "双因素方差分析",
            Self::RmOneWay => "重复测量方差分析",
            Self::Roc => "ROC 曲线与 AUC",
            Self::WelchT => "Welch t 检验",
            Self::OneSampleT => "单样本 t 检验",
            Self::VarianceTests => "方差齐性检验",
            Self::Outliers => "离群值检测（Grubbs）",
            Self::Friedman => "Friedman 检验",
            Self::MultiCompare => "多重比较（Tukey / G–H）",
            Self::Spearman => "Spearman 相关",
            Self::Deming => "Deming 回归",
            Self::SlopeCompare => "比较两条斜率",
            Self::CurveFit => "非线性拟合（AICc）",
            Self::CorrMatrix => "相关矩阵",
            Self::Pca => "主成分分析",
            Self::Hierarchical => "层次聚类",
            Self::KMeans => "K 均值聚类",
            Self::EffectRatios => "比值比 / 相对风险",
            Self::McNemar => "McNemar 检验",
            Self::MultiRegression => "多元线性回归",
            Self::Cox => "Cox 比例风险回归",
            Self::SignedRank => "Wilcoxon 符号秩（单样本）",
            Self::FVariance => "F 方差比检验",
            Self::Kappa => "Cohen's Kappa",
            Self::BlandAltman => "Bland–Altman 一致性",
            Self::Smart => "智能分析（全指标）",
        }
    }

    /// 菜单分组：同一组的问题放在一起，而不是按表型堆成一条长清单。
    pub fn group(self) -> AnalysisGroup {
        match self {
            Self::Smart => AnalysisGroup::Smart,
            Self::Descriptive | Self::Normality | Self::Outliers => AnalysisGroup::Describe,
            Self::Spearman
            | Self::Correlation
            | Self::LinearRegression
            | Self::Deming
            | Self::SlopeCompare
            | Self::CurveFit
            | Self::Roc => AnalysisGroup::Correlate,
            Self::CorrMatrix
            | Self::Pca
            | Self::Hierarchical
            | Self::KMeans
            | Self::MultiRegression => AnalysisGroup::Multivariate,
            Self::ChiSquare
            | Self::FisherExact
            | Self::EffectRatios
            | Self::McNemar
            | Self::Kappa => AnalysisGroup::Contingency,
            Self::Cox => AnalysisGroup::Survival,
            _ => AnalysisGroup::Compare,
        }
    }

    /// 该分析能否在给定数据表上做（与图型过滤同一思路：只列能诚实做的）。
    pub fn supports(self, t: TableKind) -> bool {
        use TableKind as T;
        match self {
            // 描述统计 / 正态性 / 智能分析：任何有数值列的表都行
            Self::Descriptive | Self::Normality | Self::Smart => true,
            // XY 独有的回归 / 相关
            Self::Correlation | Self::LinearRegression => t == T::Xy,
            // 嵌套检验：只有嵌套表有「外层组 × 内层子组」结构
            Self::NestedT | Self::NestedAnova => t == T::Nested,
            // 计数表检验：列联表才有频数语义
            Self::ChiSquare | Self::FisherExact => t == T::Contingency,
            // 双因素：分组表就是「行因子 × 列因子 × 格内重复」的布局
            Self::TwoWayAnova => t == T::Grouped,
            // 重复测量：列=处理、行=受试者（柱状/分组表都行）
            Self::RmOneWay => matches!(t, T::Column | T::Grouped),
            // ROC：需要两组连续观测（阳性/阴性）
            Self::Roc => matches!(t, T::Column | T::Grouped | T::Xy | T::MultipleVariables),
            // 单组重复观测的组间比较：每列一组
            Self::WelchT
            | Self::OneSampleT
            | Self::VarianceTests
            | Self::MultiCompare
            | Self::SignedRank
            | Self::FVariance
            | Self::BlandAltman => {
                matches!(t, T::Column | T::Grouped | T::Xy | T::MultipleVariables)
            }
            // Friedman：列=处理、行=受试者，按行对齐才成立
            Self::Friedman => matches!(t, T::Column | T::Grouped),
            // 离群值：逐列（组）检测
            Self::Outliers => matches!(t, T::Column | T::Grouped | T::Xy | T::MultipleVariables),
            // XY 家族：一个 X、多个 Y
            Self::Spearman | Self::Deming | Self::SlopeCompare | Self::CurveFit => t == T::Xy,
            // 多变量方法：列 = 变量、行 = 同一个体的多项测量。
            // 只在「多变量表」上开放——把柱状表的「各组观测」当成变量做 PCA，
            // 等于把不同受试者的读数配成对，结果没有意义。
            Self::CorrMatrix | Self::Pca | Self::Hierarchical | Self::KMeans => {
                t == T::MultipleVariables
            }
            // 四格表 / 配对二分类
            Self::EffectRatios | Self::McNemar | Self::Kappa => t == T::Contingency,
            // 多元回归：第一列因变量，其余自变量
            Self::MultiRegression => t == T::MultipleVariables,
            // Cox：时间 + 事件列
            Self::Cox => t == T::Survival,
            // 其余组间比较：需要「每列一组重复观测」
            _ => matches!(
                t,
                T::Xy | T::Column | T::Grouped | T::MultipleVariables | T::Nested
            ),
        }
    }

    /// 指定表格类型可用的分析列表（菜单直接用，空则提示）。
    pub fn available_for(t: TableKind) -> Vec<AnalysisKind> {
        Self::ALL
            .iter()
            .copied()
            .filter(|a| a.supports(t))
            .collect()
    }

    pub fn needs_two_groups(self) -> bool {
        matches!(
            self,
            Self::UnpairedT | Self::PairedT | Self::MannWhitney | Self::Wilcoxon
        )
    }

    pub fn is_xy(self) -> bool {
        matches!(self, Self::Correlation | Self::LinearRegression)
    }

    /// 是否针对嵌套表。
    pub fn is_nested(self) -> bool {
        matches!(self, Self::NestedT | Self::NestedAnova)
    }

    /// 是否针对列联表。
    pub fn is_contingency(self) -> bool {
        matches!(self, Self::ChiSquare | Self::FisherExact)
    }

    /// 是否属于「多组非参数比较」族（需要 3 组以上才有事后比较的意义）。
    pub fn is_rank_family(self) -> bool {
        matches!(self, Self::KruskalWallis | Self::Dunn)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnalysisOptions {
    #[serde(default)]
    pub columns: Vec<usize>,
    #[serde(default = "default_alpha")]
    pub alpha: f64,
    #[serde(default)]
    pub welch: bool,
    #[serde(default)]
    pub spearman: bool,
}

fn default_alpha() -> f64 {
    0.05
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            columns: Vec::new(),
            alpha: 0.05,
            welch: true,
            spearman: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Analysis {
    pub id: u64,
    pub name: String,
    pub table: u64,
    pub kind: AnalysisKind,
    pub options: AnalysisOptions,
    pub result: ResultTable,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Graph {
    pub id: u64,
    pub name: String,
    pub table: u64,
    pub spec: GraphSpec,
}

/// 图注编号样式（规范 G9/多图面板：a/b/c 或 A/B/C 或 1/2/3）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LabelCase {
    Lower,
    Upper,
    Number,
}

impl Default for LabelCase {
    fn default() -> Self {
        Self::Lower
    }
}

impl LabelCase {
    /// 第 i 个面板的编号（1 起）。
    pub fn label(self, i: usize) -> String {
        match self {
            Self::Lower => {
                // 超过 26 个面板后自然扩成 aa/ab…，拼版很少见，够用。
                let mut n = i;
                let mut t = String::new();
                loop {
                    t.insert(0, (b'a' + (n % 26) as u8) as char);
                    if n < 26 {
                        break;
                    }
                    n = n / 26 - 1;
                }
                t
            }
            Self::Upper => Self::Lower.label(i).to_uppercase(),
            Self::Number => format!("{}", i + 1),
        }
    }
}

/// 多图拼版页（Layout）：把若干张已存在的图按网格拼成一张大图。
///
/// 面板按 `panels` 的顺序从左到右、从上到下排布；每个面板左上角标
/// 图注编号（a/b/c…），供正文引用「图 2a」。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayoutPage {
    pub id: u64,
    pub name: String,
    /// 列数（1..=3）。行数由面板数推算。
    #[serde(default = "default_layout_cols")]
    pub cols: usize,
    /// 各面板引用的图 id，按阅读顺序排列。
    #[serde(default)]
    pub panels: Vec<u64>,
    /// 是否显示图注编号。
    #[serde(default = "default_true")]
    pub show_labels: bool,
    #[serde(default)]
    pub label_case: LabelCase,
}

fn default_layout_cols() -> usize {
    2
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub format: u32,
    pub name: String,
    pub tables: Vec<DataTable>,
    #[serde(default)]
    pub analyses: Vec<Analysis>,
    #[serde(default)]
    pub graphs: Vec<Graph>,
    #[serde(default)]
    pub layouts: Vec<LayoutPage>,
    #[serde(default)]
    pub next_id: u64,
    #[serde(default)]
    pub notes: String,
}

impl Project {
    pub fn empty(name: impl Into<String>) -> Self {
        Self {
            format: FORMAT_VERSION,
            name: name.into(),
            tables: Vec::new(),
            analyses: Vec::new(),
            graphs: Vec::new(),
            layouts: Vec::new(),
            next_id: 1,
            notes: String::new(),
        }
    }

    pub fn new_id(&mut self) -> u64 {
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        id
    }

    pub fn table(&self, id: u64) -> Option<&DataTable> {
        self.tables.iter().find(|t| t.id == id)
    }

    pub fn table_mut(&mut self, id: u64) -> Option<&mut DataTable> {
        self.tables.iter_mut().find(|t| t.id == id)
    }

    pub fn graph(&self, id: u64) -> Option<&Graph> {
        self.graphs.iter().find(|g| g.id == id)
    }

    pub fn graph_mut(&mut self, id: u64) -> Option<&mut Graph> {
        self.graphs.iter_mut().find(|g| g.id == id)
    }

    pub fn analysis(&self, id: u64) -> Option<&Analysis> {
        self.analyses.iter().find(|a| a.id == id)
    }

    pub fn add_table(&mut self, kind: TableKind, name: impl Into<String>) -> u64 {
        let id = self.new_id();
        let name = self.unique_table_name(name.into());
        self.tables.push(DataTable::template(kind, name, id));
        id
    }

    pub fn unique_table_name(&self, base: String) -> String {
        if !self.tables.iter().any(|t| t.name == base) {
            return base;
        }
        let mut i = 2;
        loop {
            let candidate = format!("{base} {i}");
            if !self.tables.iter().any(|t| t.name == candidate) {
                return candidate;
            }
            i += 1;
        }
    }

    // ---- 侧栏右键菜单：删除 / 复制 / 重命名 ----

    /// 删除数据表（连带删掉引用它的分析/图表/拼版面板）。
    pub fn remove_table(&mut self, id: u64) -> bool {
        let n = self.tables.len();
        self.tables.retain(|t| t.id != id);
        let removed = self.tables.len() != n;
        if removed {
            self.analyses.retain(|a| a.table != id);
            let dead_graphs: Vec<u64> = self
                .graphs
                .iter()
                .filter(|g| g.table == id)
                .map(|g| g.id)
                .collect();
            self.graphs.retain(|g| g.table != id);
            for l in &mut self.layouts {
                l.panels.retain(|p| !dead_graphs.contains(p));
            }
            self.layouts.retain(|l| !l.panels.is_empty());
        }
        removed
    }

    pub fn remove_analysis(&mut self, id: u64) -> bool {
        let n = self.analyses.len();
        self.analyses.retain(|a| a.id != id);
        self.analyses.len() != n
    }

    pub fn remove_graph(&mut self, id: u64) -> bool {
        let n = self.graphs.len();
        self.graphs.retain(|g| g.id != id);
        let removed = self.graphs.len() != n;
        if removed {
            for l in &mut self.layouts {
                l.panels.retain(|p| *p != id);
            }
            self.layouts.retain(|l| !l.panels.is_empty());
        }
        removed
    }

    pub fn remove_layout(&mut self, id: u64) -> bool {
        let n = self.layouts.len();
        self.layouts.retain(|l| l.id != id);
        self.layouts.len() != n
    }

    /// 复制整张表（新 id + 唯一名 + 副本后缀）。
    pub fn duplicate_table(&mut self, id: u64) -> Option<u64> {
        let mut t = self.table(id)?.clone();
        let new_id = self.new_id();
        t.id = new_id;
        t.name = self.unique_table_name(format!("{} 副本", t.name));
        self.tables.push(t);
        Some(new_id)
    }

    /// 复制图表（指向同一数据表）。
    pub fn duplicate_graph(&mut self, id: u64) -> Option<u64> {
        let mut g = self.graph(id)?.clone();
        let new_id = self.new_id();
        g.id = new_id;
        g.name = self.unique_graph_name(format!("{} 副本", g.name));
        self.graphs.push(g);
        Some(new_id)
    }

    pub fn unique_graph_name(&self, base: String) -> String {
        if !self.graphs.iter().any(|g| g.name == base) {
            return base;
        }
        let mut i = 2;
        loop {
            let candidate = format!("{base} {i}");
            if !self.graphs.iter().any(|g| g.name == candidate) {
                return candidate;
            }
            i += 1;
        }
    }

    /// 给任意对象重命名，冲突时自动加后缀。
    pub fn rename_item(&mut self, sheet: crate::app::Sheet, name: &str) {
        use crate::app::Sheet;
        let base = name.trim().to_string();
        if base.is_empty() {
            return;
        }
        // 计算唯一名（排除自身），避免「重命名成同名」被加后缀。
        match sheet {
            Sheet::Table(id) => {
                let mut new_name = base.clone();
                let mut i = 2;
                while self.tables.iter().any(|t| t.id != id && t.name == new_name) {
                    new_name = format!("{base} {i}");
                    i += 1;
                }
                if let Some(t) = self.table_mut(id) {
                    t.name = new_name;
                }
            }
            Sheet::Graph(id) => {
                let mut new_name = base.clone();
                let mut i = 2;
                while self.graphs.iter().any(|g| g.id != id && g.name == new_name) {
                    new_name = format!("{base} {i}");
                    i += 1;
                }
                if let Some(g) = self.graph_mut(id) {
                    g.name = new_name;
                }
            }
            Sheet::Analysis(id) => {
                if let Some(a) = self.analyses.iter_mut().find(|a| a.id == id) {
                    a.name = base;
                }
            }
            Sheet::Layout(id) => {
                if let Some(l) = self.layouts.iter_mut().find(|l| l.id == id) {
                    l.name = base;
                }
            }
        }
    }

    /// 读取任意对象当前名称。
    pub fn item_name(&self, sheet: crate::app::Sheet) -> Option<String> {
        use crate::app::Sheet;
        match sheet {
            Sheet::Table(id) => self.table(id).map(|t| t.name.clone()),
            Sheet::Graph(id) => self.graph(id).map(|g| g.name.clone()),
            Sheet::Analysis(id) => self
                .analyses
                .iter()
                .find(|a| a.id == id)
                .map(|a| a.name.clone()),
            Sheet::Layout(id) => self
                .layouts
                .iter()
                .find(|l| l.id == id)
                .map(|l| l.name.clone()),
        }
    }

    pub fn add_graph_for(&mut self, table: u64) -> u64 {
        let id = self.new_id();
        let table_name = self
            .table(table)
            .map(|t| t.name.clone())
            .unwrap_or_else(|| "Graph".into());
        let name = format!("{table_name} · 图");
        let mut spec = GraphSpec::default();
        if let Some(t) = self.table(table) {
            spec = GraphSpec::default_for(t);
        }
        self.graphs.push(Graph {
            id,
            name,
            table,
            spec,
        });
        id
    }

    pub fn layout(&self, id: u64) -> Option<&LayoutPage> {
        self.layouts.iter().find(|l| l.id == id)
    }

    pub fn layout_mut(&mut self, id: u64) -> Option<&mut LayoutPage> {
        self.layouts.iter_mut().find(|l| l.id == id)
    }

    /// 新建拼版页，默认把当前所有图收进去。
    pub fn add_layout(&mut self) -> u64 {
        let id = self.new_id();
        let n = self.layouts.len() + 1;
        self.layouts.push(LayoutPage {
            id,
            name: if n > 1 {
                format!("拼版 {n}")
            } else {
                "拼版".into()
            },
            cols: 2,
            panels: self.graphs.iter().map(|g| g.id).collect(),
            show_labels: true,
            label_case: LabelCase::default(),
        });
        id
    }

    pub fn sheet_label(&self, id: u64) -> Option<String> {
        self.table(id).map(|t| t.name.clone()).or_else(|| {
            self.graphs
                .iter()
                .find(|g| g.id == id)
                .map(|g| g.name.clone())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_are_rectangular() {
        for kind in TableKind::ALL {
            let t = DataTable::template(kind, "T", 1);
            let n = t.nrows();
            assert!(n > 0, "{kind:?}");
            for c in &t.columns {
                assert_eq!(c.cells.len(), n, "{kind:?} col {}", c.name);
            }
            assert!(t.columns.iter().any(|c| c.role == ColRole::Y), "{kind:?}");
        }
    }

    #[test]
    fn excluded_values_are_skipped() {
        let mut t = DataTable::template(TableKind::Column, "T", 1);
        t.set_cell(0, 0, Cell::number(1.0));
        t.set_cell(1, 0, Cell::number(2.0));
        t.set_excluded(1, 0, true);
        assert_eq!(t.column_values(0), vec![1.0]);
    }

    #[test]
    fn non_contiguous_values_are_counted() {
        // 列型：3 个数不连在一起（中间有空行），都要算进去。
        let mut t = DataTable::template(TableKind::Column, "T", 1);
        t.set_cell(0, 0, Cell::number(1.0));
        t.set_cell(3, 0, Cell::number(2.0));
        t.set_cell(7, 0, Cell::number(3.0));
        assert_eq!(t.column_values(0), vec![1.0, 2.0, 3.0], "空行不应吞掉数据");
        let g = t.analysis_groups();
        assert_eq!(g[0].1, vec![1.0, 2.0, 3.0], "analysis_groups 也要数到 3");
    }

    #[test]
    fn xy_points_with_gaps_count_valid_pairs() {
        // XY：X、Y 在有空行的行上成对，3 对都应收集到。
        let mut t = DataTable::template(TableKind::Xy, "T", 1);
        t.set_cell(0, 0, Cell::number(1.0)); // X
        t.set_cell(0, 1, Cell::number(10.0)); // Y
        t.set_cell(4, 0, Cell::number(2.0));
        t.set_cell(4, 1, Cell::number(20.0));
        t.set_cell(9, 0, Cell::number(3.0));
        t.set_cell(9, 1, Cell::number(30.0));
        let pts = t.xy_points(0);
        assert_eq!(pts, vec![(1.0, 10.0), (2.0, 20.0), (3.0, 30.0)]);
    }

    #[test]
    fn paste_tsv_parses_numbers_and_text() {
        let mut t = DataTable::template(TableKind::MultipleVariables, "T", 1);
        t.paste_tsv(0, 0, "1\t2\tfoo\n3\t4\tbar");
        assert_eq!(t.cell(0, 2).unwrap().text.as_deref(), Some("foo"));
        assert_eq!(t.cell(1, 1).unwrap().num, Some(4.0));
    }

    #[test]
    fn number_format_is_readable() {
        assert_eq!(format_number(1.2300), "1.23");
        assert_eq!(format_number(0.0000123), "1.23e-5");
        assert_eq!(format_number(12345678.0), "1.2346e7");
    }
}

#[cfg(test)]
mod table_format_tests {
    use super::*;

    #[test]
    fn 八种表型齐全且按Prism顺序() {
        assert_eq!(TableKind::ALL.len(), 8);
        assert_eq!(TableKind::ALL[7], TableKind::Nested);
        for k in TableKind::ALL {
            assert!(!k.label().is_empty());
            assert!(!k.en().is_empty());
            assert!(!k.description().is_empty());
        }
    }

    #[test]
    fn 子列规则与官方说明一致() {
        assert_eq!(TableKind::Xy.sub_layout(), SubLayout::SideBySide);
        assert_eq!(TableKind::Grouped.sub_layout(), SubLayout::SideBySide);
        assert_eq!(TableKind::Nested.sub_layout(), SubLayout::Stacked);
        assert_eq!(TableKind::Column.sub_layout(), SubLayout::None);
        for k in [
            TableKind::Contingency,
            TableKind::PartsOfWhole,
            TableKind::Survival,
            TableKind::MultipleVariables,
        ] {
            assert_eq!(k.sub_layout(), SubLayout::None, "{k:?} 不该有子列");
        }
    }

    #[test]
    fn 行标题与首列X规则() {
        assert!(TableKind::Xy.first_is_x());
        assert!(TableKind::Survival.first_is_x());
        assert!(!TableKind::Column.first_is_x());
        for k in [
            TableKind::Column,
            TableKind::Survival,
            TableKind::MultipleVariables,
        ] {
            assert!(!k.has_row_titles(), "{k:?} 不该有行标题列");
        }
        assert!(TableKind::Grouped.has_row_titles());
        assert!(TableKind::Nested.has_row_titles());
        assert!(TableKind::Contingency.has_row_titles());
    }

    #[test]
    fn 列联表只收非负整数() {
        let mut t = DataTable::template(TableKind::Contingency, "T", 1);
        assert!(t.set_text(0, 0, "12"));
        assert_eq!(t.cell(0, 0).unwrap().value(), Some(12.0));
        assert!(!t.set_text(0, 0, "3.7"), "小数应被拒绝");
        assert!(!t.set_text(0, 0, "-2"), "负数应被拒绝");
        assert!(!t.set_text(0, 0, "abc"), "文本应被拒绝");
        assert!(t.cell(0, 0).unwrap().is_empty(), "非法输入应留空");
    }

    #[test]
    fn 生存表结局列只收0或1() {
        let mut t = DataTable::template(TableKind::Survival, "T", 1);
        assert!(t.set_text(0, 0, "8.5"), "时间列可填任意数值");
        assert!(t.set_text(0, 1, "1"));
        assert!(t.set_text(0, 1, "0"));
        assert!(!t.set_text(0, 1, "2"), "结局只能是 0/1");
        assert!(!t.set_text(0, 1, "0.5"));
    }

    #[test]
    fn 多变量表允许文本标签() {
        let mut t = DataTable::template(TableKind::MultipleVariables, "T", 1);
        assert!(t.set_text(0, 0, "Female"));
        assert_eq!(t.cell(0, 0).unwrap().text.as_deref(), Some("Female"));
        let mut c = DataTable::template(TableKind::Column, "T", 1);
        assert!(!c.set_text(0, 0, "Female"), "普通数值表不该收文本");
    }

    #[test]
    fn 嵌套表模板与聚合() {
        let t = DataTable::template(TableKind::Nested, "T", 1);
        assert_eq!(t.kind, TableKind::Nested);
        assert_eq!(t.group_names.len(), 2);
        assert_eq!(t.columns.len(), 4);
        assert!(t.nrows() >= 6);
        // 内层子列按外层分组聚合，图上只出现两个外层组
        let mut g = t.clone();
        for (i, c) in g.columns.iter_mut().enumerate() {
            for (r, cell) in c.cells.iter_mut().enumerate() {
                *cell = Cell::number((i * 10 + r) as f64);
            }
        }
        assert_eq!(g.analysis_groups().len(), 2);
    }

    #[test]
    fn 结构固定的表型不能加列() {
        assert!(!TableKind::PartsOfWhole.can_add_column());
        assert!(!TableKind::Survival.can_add_column());
        assert!(TableKind::Column.can_add_column());
        assert!(TableKind::Nested.can_add_column());
    }
}
