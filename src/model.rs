//! 析光工程数据模型：8 种科研表格、结果表、图表、导航器条目。
//!
//! # 表格架构（模板驱动 + 统一稀疏数据层）
//!
//! 两部分组成，缺一不可：
//!
//! 1. **工厂**：`SpectraTable::new_by_type(TableType, num_subcols)` 按类型生成
//!    精确初始模板（行标题、列数据集、子列及其排列方式）。
//! 2. **统一数据层**：`HashMap<usize, BTreeMap<usize, CellValue>>` —— 行主序稀疏矩阵。
//!    行号作键 → 天然支持无限行；行内用 `BTreeMap` → 列有序、只存有数据的列。
//!    读写完全隔离，零借用冲突。
//!
//! 关键区分（这是 8 种表「长相」的根源）：
//! - XY / Grouped 用**并排（Side-by-side）**子列 —— 重复值横向排开
//! - Column / Nested 用**堆叠（Stacked）**子列 —— 重复值纵向堆叠
//! - Contingency / Survival / PartsOfWhole / MultipleVariables **无子列**

use std::collections::{BTreeMap, HashMap};

use crate::i18n::K;
use crate::plot::{BracketMode, ChartKind, UnitStyle};

// 比较对类型定义在 plot.rs（绘图逻辑要用），这里公开重导出供 UI 直接引用
pub use crate::plot::Comparison;

/// 欢迎对话框左侧的 8 种表类型（顺序与析光一致）。
/// (名称 key, 说明 key) —— 显示时按当前语言取文案。
pub const TABLE_KINDS: [(K, K); 8] = [
    (K::TblXy, K::TblDescXy),
    (K::TblColumn, K::TblDescColumn),
    (K::TblGrouped, K::TblDescGrouped),
    (K::TblContingency, K::TblDescContingency),
    (K::TblSurvival, K::TblDescSurvival),
    (K::TblPartsOfWhole, K::TblDescPartsOfWhole),
    (K::TblMultipleVariables, K::TblDescMultipleVariables),
    (K::TblNested, K::TblDescNested),
];

/// Existing file 下的项。原本 5 项（打开 / LabArchives / 克隆图 / 图集 / 技巧），
/// 现按需求只保留「打开现有文件」——其余都要真正接入外部服务才成立，
/// 摆着点了没反应反而是噪音。
pub const EXISTING_ITEMS: [K; 1] = [K::OpenAFile];

// ── 单元格 ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CellValue {
    Number(f64),
    Text(String),
    Empty,
}

impl CellValue {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            CellValue::Number(x) => Some(*x),
            _ => None,
        }
    }

    /// 按文本解析：能解析成数字就是 Number，否则 Text，空串是 Empty。
    /// 粘贴 / 导入统一走这里，保证「列联表只存整数」等约束在入口生效。
    #[allow(dead_code)]
    pub fn parse(s: &str) -> Self {
        let t = s.trim();
        if t.is_empty() {
            return CellValue::Empty;
        }
        match t.parse::<f64>() {
            Ok(x) => CellValue::Number(x),
            Err(_) => CellValue::Text(t.to_string()),
        }
    }

    /// 默认显示格式：**原样保留输入内容**，不补零也不截断。
    ///
    /// `{}` 对 f64 给出「最短可 round-trip 的十进制」，即输入什么就显示什么：
    /// `6.3879854` 显示 `6.3879854`，`6.3` 显示 `6.3`，`7` 显示 `7`。
    /// 之前这里固定 `{:.1}`，会把 `6.3879854` 截成 `6.4` —— 输入内容被悄悄改掉。
    pub fn display(&self) -> String {
        match self {
            CellValue::Number(x) => num_short(*x),
            CellValue::Text(s) => s.clone(),
            CellValue::Empty => String::new(),
        }
    }

    /// 按**列内最长小数位**对齐后的显示文本。
    ///
    /// `dp = None` 时同 `display()`（不补零）；`dp = Some(n)` 时统一 n 位小数。
    /// 这一列里只要有 `6.3879854`，同列的 `6.3` 就会显示成 `6.3000000`。
    pub fn display_dp(&self, dp: Option<usize>) -> String {
        match (self, dp) {
            (CellValue::Number(x), Some(n)) => format!("{:.*}", n, x),
            (other, _) => other.display(),
        }
    }
}

/// 数值的「原样」文本：最短 round-trip 表示，不带多余的零。
///
/// 整数不带 `.0`；`-0.0` 归一成 `0`；非有限值（NaN / ±∞）显示「—」。
pub fn num_short(x: f64) -> String {
    if !x.is_finite() {
        return "—".to_string();
    }
    if x == 0.0 {
        return "0".to_string();
    }
    let s = format!("{}", x);
    // 极大 / 极小值会落到科学计数法，原样保留即可
    s
}

/// 这个数最少需要几位小数才能**精确**表示（去掉尾随零之后）。
///
/// 用于算「列内最长小数位」：`6.3879854` → 7，`6.3` → 1，`7` → 0。
/// 上限 15 —— f64 的有效十进制位就那么多，再多是无意义的噪声位。
pub fn frac_digits(x: f64) -> usize {
    if !x.is_finite() || x == 0.0 {
        return 0;
    }
    let s = format!("{}", x);
    // 科学计数法（1e-7 / 1.23e20）不参与对齐：它的小数位没有排版意义
    if s.contains('e') || s.contains('E') {
        return 0;
    }
    match s.find('.') {
        Some(p) => (s.len() - p - 1).min(15),
        None => 0,
    }
}

// ── 表类型 ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TableType {
    XY,
    Column,
    Grouped,
    Contingency,
    Survival,
    PartsOfWhole,
    MultipleVariables,
    Nested,
}

/// 子列排列方式 —— 决定表格「长相」的核心。
/// XY / Grouped 用**并排**（重复值横向排开），Nested 用**堆叠**（纵向堆叠）。
/// 其余类型没有子列，因此不需要这个标记。
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SubLayout {
    /// 并排：重复值横向排开（XY 的 Y 列、Grouped 的各数据集）
    SideBySide,
    /// 堆叠：重复值纵向堆叠（Nested），且强制至少 2 个子列
    Stacked,
}

impl TableType {
    /// 子列排列方式 —— 决定表格「长相」的核心。
    /// XY / Grouped 用**并排**（重复值横向排开），Nested 用**堆叠**（纵向堆叠）。
    pub fn sub_layout(self) -> SubLayout {
        match self {
            TableType::XY | TableType::Grouped => SubLayout::SideBySide,
            TableType::Nested => SubLayout::Stacked,
            _ => SubLayout::SideBySide, // 其余类型没有子列，取哪个都不影响
        }
    }

    /// 该类型是否支持子列（只有 XY / Grouped / Nested 有）。
    pub fn has_subcolumns(self) -> bool {
        matches!(self, TableType::XY | TableType::Grouped | TableType::Nested)
    }

    /// 该类型是否使用行标题列（对应析光的第一列）。
    /// Column / Survival / MultipleVariables 没有行标题。
    pub fn has_row_titles(self) -> bool {
        match self {
            TableType::Column | TableType::Survival | TableType::MultipleVariables => false,
            _ => true,
        }
    }

    /// 单元格是否只允许整数（列联表存的是频数）。
    pub fn integer_only(self) -> bool {
        matches!(self, TableType::Contingency)
    }

    /// 单元格是否允许文本（多变量表每列是一个变量，可能是分组名）。
    pub fn allows_text(self) -> bool {
        matches!(self, TableType::MultipleVariables)
    }

    /// 第 0 个数据集是否是 X 列（XY 与 Survival 的第一列是 X）。
    pub fn first_is_x(self) -> bool {
        matches!(self, TableType::XY | TableType::Survival)
    }

    /// 该表类型可用的分析方法 —— Analyze 动态菜单按此生成。
    ///
    /// 每项 = (方法 id, 中文名, 英文名)。id 决定 `run_analysis` 走哪条分支；
    /// 同一个 id 在不同类型下语义一致，所以实现只需写一次。
    /// 尚未实现的 id 会给出明确状态提示，而不是假数字。
    pub fn analyses(self) -> &'static [(usize, &'static str, &'static str)] {
        use TableType::*;
        match self {
            XY => &[
                (crate::analysis::ADVISOR_ID, "智能统计制图推荐", "Statistics advisor"),
                (3, "线性回归", "Linear regression"),
                (5, "相关矩阵", "Correlation matrix"),
                (0, "描述统计", "Descriptive statistics"),
                (6, "正态性检验", "Normality test"),
                (7, "离群值检验", "Outlier test"),
                (9, "非线性拟合", "Nonlinear fit"),
                (10, "ROC 曲线", "ROC curve"),
                (32, "Deming 回归", "Deming regression"),
                (33, "比较两条直线斜率", "Compare slopes"),
            ],
            Column => &[
                (crate::analysis::ADVISOR_ID, "智能统计制图推荐", "Statistics advisor"),
                (0, "描述统计", "Descriptive statistics"),
                (1, "独立样本 t 检验", "Unpaired t test"),
                (2, "单因素方差分析", "One-way ANOVA"),
                (21, "事后多重比较 Tukey", "Tukey HSD"),
                (22, "Games-Howell（方差不齐）", "Games-Howell"),
                (23, "Dunnett（对照比较）", "Dunnett"),
                (24, "Dunn（非参数两两）", "Dunn's test"),
                (25, "方差齐性 Levene / BF", "Levene / Brown-Forsythe"),
                (26, "Mann-Whitney U", "Mann-Whitney U"),
                (27, "Wilcoxon 符号秩", "Wilcoxon signed-rank"),
                (28, "Kruskal-Wallis", "Kruskal-Wallis"),
                (5, "相关矩阵", "Correlation matrix"),
                (6, "正态性检验", "Normality test"),
                (7, "离群值检验", "Outlier test"),
                (11, "Bartlett 方差齐性", "Bartlett's test"),
                (30, "单样本 t 检验", "One-sample t test"),
                (31, "配对 t 检验", "Paired t test"),
            ],
            Grouped => &[
                (crate::analysis::ADVISOR_ID, "智能统计制图推荐", "Statistics advisor"),
                (0, "描述统计", "Descriptive statistics"),
                (29, "双因素方差分析", "Two-way ANOVA"),
                (2, "单因素方差分析", "One-way ANOVA"),
                (21, "事后多重比较 Tukey", "Tukey HSD"),
                (22, "Games-Howell（方差不齐）", "Games-Howell"),
                (1, "独立样本 t 检验", "Unpaired t test"),
                (6, "正态性检验", "Normality test"),
                (7, "离群值检验", "Outlier test"),
            ],
            Contingency => &[
                (8, "卡方检验", "Chi-square test"),
                (12, "Fisher 精确检验", "Fisher's exact test"),
                (13, "相对风险与比值比", "Relative risk / Odds ratio"),
            ],
            Survival => &[
                (14, "Kaplan–Meier 生存分析", "Kaplan-Meier survival"),
                (15, "Cox 比例风险回归", "Cox proportional hazards"),
                (0, "描述统计", "Descriptive statistics"),
                (34, "Log-rank 检验", "Log-rank test"),
            ],
            PartsOfWhole => &[
                (16, "总分统计", "Total score"),
                (17, "观察值与期望值比较", "Observed vs expected"),
                (0, "描述统计", "Descriptive statistics"),
            ],
            MultipleVariables => &[
                (0, "描述统计", "Descriptive statistics"),
                (5, "相关矩阵", "Correlation matrix"),
                (3, "线性回归", "Linear regression"),
                (28, "Kruskal-Wallis", "Kruskal-Wallis"),
                (25, "方差齐性 Levene / BF", "Levene / Brown-Forsythe"),
                (7, "离群值检验", "Outlier test"),
                (18, "主成分分析 PCA", "PCA"),
                (19, "层次聚类", "Hierarchical clustering"),
                (20, "K-means 聚类", "K-means clustering"),
                (15, "Cox 比例风险回归", "Cox regression"),
            ],
            Nested => &[
                (4, "嵌套单因素方差分析", "Nested one-way ANOVA"),
                (21, "嵌套 t 检验", "Nested t test"),
                (0, "描述统计", "Descriptive statistics"),
                (6, "正态性检验", "Normality test"),
                (7, "离群值检验", "Outlier test"),
            ],
        }
    }
}

// ── 列元数据 ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SubColumnMeta {
    /// 例如 "Y1" / "1" / "Rep1"
    pub name: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DataSetMeta {
    /// 列名，如 "A" / "Control" / "Age"
    pub name: String,
    /// 并排或堆叠的子列
    pub sub_columns: Vec<SubColumnMeta>,
}

impl DataSetMeta {
    /// 该数据集占几个物理列（无子列时也占 1 列）。
    pub fn width(&self) -> usize {
        self.sub_columns.len().max(1)
    }
}

// ── 核心：SpectraTable ────────────────────────────────────────────────────────

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct SpectraTable {
    pub table_type: TableType,
    /// 行命名，对应析光的第一列
    pub row_titles: Vec<String>,
    /// 列及子列的元数据
    pub data_sets: Vec<DataSetMeta>,
    /// 核心数据层：行主序稀疏矩阵
    /// key: 行索引 -> value: { 物理列索引 -> 单元格值 }
    ///
    /// JSON 的对象键只能是字符串，而这里的键是 `usize`，直接派生会失败。
    /// 所以走 `table_data_serde`：落盘成 `[行索引, [[列索引, 值], …]]` 的
    /// 有序数组，读回时还原成同样的稀疏矩阵。
    #[serde(with = "table_data_serde")]
    data: HashMap<usize, BTreeMap<usize, CellValue>>,
}

/// `SpectraTable::data` 的 JSON 桥接。
///
/// 用「有序的 (行, [(列, 值)]) 数组」而不是对象：键是整数，JSON 只认字符串键，
/// 硬转字符串会让读写两端都得处理解析失败。数组形式既稳定又保序。
mod table_data_serde {
    use super::{BTreeMap, CellValue, HashMap};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    type Row = (usize, Vec<(usize, CellValue)>);

    pub fn serialize<S: Serializer>(
        m: &HashMap<usize, BTreeMap<usize, CellValue>>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let mut rows: Vec<Row> = m
            .iter()
            .map(|(r, row)| (*r, row.iter().map(|(c, v)| (*c, v.clone())).collect()))
            .collect();
        // 按行号排序：同一份工程两次存盘的字节应当一致，便于比对
        rows.sort_by_key(|x| x.0);
        rows.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<HashMap<usize, BTreeMap<usize, CellValue>>, D::Error> {
        let rows: Vec<Row> = Vec::deserialize(d)?;
        Ok(rows
            .into_iter()
            .map(|(r, row)| (r, row.into_iter().collect()))
            .collect())
    }
}

impl SpectraTable {
    /// 工厂：根据表类型生成精确初始模板。
    ///
    /// `num_subcols` 对无子列的类型无效；Nested 会被强制抬到 ≥2。
    pub fn new_by_type(table_type: TableType, num_subcols: usize) -> Self {
        // 延迟初始化：每个分支都会赋值，避免「先给空 Vec 再覆盖」的无用赋值
        let row_titles: Vec<String>;
        let mut data_sets = Vec::new();

        match table_type {
            TableType::XY => {
                // XY：第一列 X 无子列，后续 Y 数据集有并排子列
                data_sets.push(DataSetMeta {
                    name: "X".to_string(),
                    sub_columns: vec![SubColumnMeta { name: "".to_string() }],
                });
                for label in &["A", "B"] {
                    data_sets.push(Self::create_dataset_with_subcols(
                        label,
                        num_subcols.max(1),
                        SubLayout::SideBySide,
                    ));
                }
                row_titles = vec![String::new()];
            }
            TableType::Column => {
                // 列表：列即组别，无子列，重复值纵向堆叠
                for label in &["A", "B"] {
                    data_sets.push(DataSetMeta {
                        name: label.to_string(),
                        sub_columns: vec![],
                    });
                }
                row_titles = vec![];
            }
            TableType::Grouped => {
                // 分组表：行是分组变量 1，列是分组变量 2，列有并排子列
                for label in &["Control", "Treated"] {
                    data_sets.push(Self::create_dataset_with_subcols(
                        label,
                        num_subcols.max(1),
                        SubLayout::SideBySide,
                    ));
                }
                row_titles = vec!["Male".to_string(), "Female".to_string()];
            }
            TableType::Contingency => {
                for label in &["Outcome 1", "Outcome 2"] {
                    data_sets.push(DataSetMeta {
                        name: label.to_string(),
                        sub_columns: vec![],
                    });
                }
                row_titles = vec!["Exposure 1".to_string(), "Exposure 2".to_string()];
            }
            TableType::Survival => {
                // 生存表：X 是时间，后续列是各组事件状态（1=事件，0=删失）
                data_sets.push(DataSetMeta {
                    name: "X".to_string(),
                    sub_columns: vec![],
                });
                for label in &["Group A", "Group B"] {
                    data_sets.push(DataSetMeta {
                        name: label.to_string(),
                        sub_columns: vec![],
                    });
                }
                row_titles = vec![];
            }
            TableType::PartsOfWhole => {
                data_sets.push(DataSetMeta {
                    name: "Value".to_string(),
                    sub_columns: vec![],
                });
                row_titles = vec![
                    "Category 1".to_string(),
                    "Category 2".to_string(),
                    "Category 3".to_string(),
                ];
            }
            TableType::MultipleVariables => {
                for label in &["Age", "Weight", "Group"] {
                    data_sets.push(DataSetMeta {
                        name: label.to_string(),
                        sub_columns: vec![],
                    });
                }
                row_titles = vec![];
            }
            TableType::Nested => {
                // 嵌套表：堆叠子列，且强制至少 2 个
                for label in &["A", "B"] {
                    data_sets.push(Self::create_dataset_with_subcols(
                        label,
                        num_subcols.max(2),
                        SubLayout::Stacked,
                    ));
                }
                row_titles = vec!["Group 1".to_string(), "Group 2".to_string()];
            }
        }

        Self {
            table_type,
            row_titles,
            data_sets,
            data: HashMap::new(),
        }
    }

    /// 按排列方式生成子列。
    ///
    /// 与析光表头一致：并排重复值叫 `Y1 / Y2 / Y3`，
    /// 堆叠子列（嵌套的内层单元）叫 `1 / 2 / 3`。
    fn create_dataset_with_subcols(
        label: &str,
        count: usize,
        layout: SubLayout,
    ) -> DataSetMeta {
        let subs = (1..=count)
            .map(|i| SubColumnMeta {
                name: match layout {
                    // 并排重复值叫 Y1/Y2/Y3；堆叠子列（内层单元）叫 1/2/3
                    SubLayout::SideBySide => format!("Y{}", i),
                    SubLayout::Stacked => i.to_string(),
                },
            })
            .collect();
        DataSetMeta {
            name: label.to_string(),
            sub_columns: subs,
        }
    }

    // ── 物理坐标 ────────────────────────────────────────────────────────────

    /// 物理列总数（把所有数据集的子列展开）。
    pub fn total_physical_columns(&self) -> usize {
        self.data_sets.iter().map(|ds| ds.width()).sum()
    }

    /// 已写入数据的行号（升序）。
    ///
    /// `data` 是私有字段（稀疏层不该外泄），导出 / 遍历走这个方法。
    /// 空行不在 HashMap 里，所以第 100 万行也只返回真正写过的那些。
    pub fn filled_rows(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self.data.keys().copied().collect();
        v.sort_unstable();
        v
    }

    /// 某一行的全部已写入单元格 `(物理列, 值)`，按列升序。
    pub fn row_cells(&self, r: usize) -> Vec<(usize, CellValue)> {
        self.data
            .get(&r)
            .map(|row| row.iter().map(|(c, v)| (*c, v.clone())).collect())
            .unwrap_or_default()
    }

    /// 物理列 → (数据集索引, 子列索引)。
    /// UI 只需处理物理坐标，不必关心归属。
    pub fn phys_to_logic(&self, phys: usize) -> (usize, usize) {
        let mut acc = 0usize;
        for (di, ds) in self.data_sets.iter().enumerate() {
            if phys < acc + ds.width() {
                return (di, phys - acc);
            }
            acc += ds.width();
        }
        (
            self.data_sets.len().saturating_sub(1),
            self.data_sets.last().map(|d| d.width() - 1).unwrap_or(0),
        )
    }

    /// 数据集索引 → 该数据集第一个物理列
    pub fn dataset_start_col(&self, di: usize) -> usize {
        self.data_sets.iter().take(di).map(|d| d.width()).sum()
    }

    /// 新建数据集时的默认子列数：XY / Grouped / Nested 有子列，其余为 1。
    pub fn default_sub_width(&self) -> usize {
        if !self.table_type.has_subcolumns() {
            return 1;
        }
        match self.table_type {
            TableType::Nested => 2.max(2), // Nested 强制至少 2 个堆叠子列
            _ => 3,                        // XY / Grouped 默认 3 个并排重复
        }
    }

    /// 生成一个「下一个数据集」的模板（名字取下一个列字母，子列按表型）。
    pub fn new_dataset_template(&self) -> DataSetMeta {
        let n = self.data_sets.len();
        let subs: Vec<SubColumnMeta> = if self.default_sub_width() > 1 {
            (1..=self.default_sub_width())
                .map(|i| SubColumnMeta { name: i.to_string() })
                .collect()
        } else {
            Vec::new()
        };
        DataSetMeta { name: self.dataset_letter(n), sub_columns: subs }
    }

    /// **列的无限扩展**：保证物理列 `phys` 有归属，不够就补数据集。
    ///
    /// 行那边靠稀疏 HashMap 天然无限；列这边必须由结构承载（数据集 + 子列），
    /// 所以在写入前按需补齐 —— 用户往右走到 Z、AA 都能直接输入。
    ///
    /// 返回是否真的新增了数据集（供调用方决定是否记撤销点）。
    pub fn ensure_col(&mut self, phys: usize) -> bool {
        let mut added = false;
        while self.total_physical_columns() <= phys {
            self.data_sets.push(self.new_dataset_template());
            added = true;
        }
        added
    }

    /// 物理列的显示标签 `(组名, 子列名)`。
    /// 超出已有数据集的列也给出**它将来会叫什么**，
    /// 这样右侧空白区的表头不是空的，往右走时能看清到了哪一列。
    pub fn col_label(&self, phys: usize) -> (String, String) {
        let total = self.total_physical_columns();
        if phys < total {
            let (di, si) = self.phys_to_logic(phys);
            let ds = &self.data_sets[di];
            let sub = ds
                .sub_columns
                .get(si)
                .map(|s| s.name.clone())
                .unwrap_or_default();
            (ds.name.clone(), sub)
        } else {
            // 虚拟列：按「一个个补数据集」的规则推算它归属哪个数据集
            let mut n = self.data_sets.len();
            let mut acc = total;
            let w = self.default_sub_width().max(1);
            while acc + w <= phys {
                acc += w;
                n += 1;
            }
            let letter = self.dataset_letter(n);
            let si = phys - acc;
            let sub = if w > 1 {
                format!("{}:{}", letter, si + 1)
            } else {
                String::new()
            };
            (letter, sub)
        }
    }

    /// 数据集的列字母：XY / Survival 的第 0 列是 X，其后才是 A、B…
    pub fn dataset_letter(&self, di: usize) -> String {
        if self.table_type.first_is_x() {
            if di == 0 {
                return "X".to_string();
            }
            return ((b'A' + (di - 1) as u8) as char).to_string();
        }
        ((b'A' + di as u8) as char).to_string()
    }

    // ── 安全读写（无限扩展，零借用冲突）───────────────────────────────────

    /// 写入：行、列都会按需自动扩展，无需预分配。
    pub fn set_cell(&mut self, physical_row: usize, physical_col: usize, value: CellValue) {
        // 列也要无限：写到哪一列，就补齐到哪一列（XY/Grouped/Nested 会连子列一起建出来）
        self.ensure_col(physical_col);
        self.data.entry(physical_row).or_default().insert(physical_col, value);
    }

    /// 读取：越界或空单元格一律返回 `Empty`（不会 panic）。
    pub fn get_cell(&self, physical_row: usize, physical_col: usize) -> CellValue {
        self.data
            .get(&physical_row)
            .and_then(|row_map| row_map.get(&physical_col))
            .cloned()
            .unwrap_or(CellValue::Empty)
    }

    /// 在 `at` 处插入一行：`at` 及之后的数据整体下移一行。
    /// 稀疏层没有「行实体」，所以只做键的平移；行标题同步插入一个空位。
    pub fn insert_row(&mut self, at: usize) {
        let mut rows: Vec<usize> = self.data.keys().copied().collect();
        rows.sort_unstable();
        for r in rows.into_iter().rev() {
            if r >= at {
                if let Some(row) = self.data.remove(&r) {
                    self.data.insert(r + 1, row);
                }
            }
        }
        // 行标题：在 at 处插一个空位（标题本身随数据下移）
        let titles = std::mem::take(&mut self.row_titles);
        for (i, t) in titles.iter().enumerate() {
            if i == at {
                self.row_titles.push(String::new());
            }
            self.row_titles.push(t.clone());
        }
        if at >= titles.len() {
            self.row_titles.push(String::new());
        }
    }

    /// 删除第 `at` 行：之后的数据上移一行。
    pub fn delete_row(&mut self, at: usize) {
        let mut rows: Vec<usize> = self.data.keys().copied().collect();
        rows.sort_unstable();
        for r in rows {
            if r > at {
                if let Some(row) = self.data.remove(&r) {
                    self.data.insert(r - 1, row);
                }
            }
        }
        self.data.remove(&at);
        if at < self.row_titles.len() {
            self.row_titles.remove(at);
        }
    }

    /// 删除一个物理列：右侧所有列整体左移一格，行标题不受影响。
    ///
    /// 与 `delete_row` 的差别在于列是「行内的键」而不是一级键，所以只能
    /// 逐行重建 `BTreeMap`，不能像行那样整体搬移条目。
    pub fn delete_physical_col(&mut self, phys: usize) {
        for row in self.data.values_mut() {
            // 先整体取出再回写：BTreeMap 在迭代中改键会破坏遍历顺序
            let taken: Vec<(usize, CellValue)> =
                row.iter().map(|(&c, v)| (c, v.clone())).collect();
            row.clear();
            for (c, v) in taken {
                if c == phys {
                    continue;
                }
                row.insert(if c > phys { c - 1 } else { c }, v);
            }
        }
    }

    /// 删除一个数据集（连同它占的全部物理列）。
    ///
    /// 连续删 `width()` 次同一个起始列即可：每删一次，下一个子列就顶到
    /// `start` 这个位置上来。
    pub fn delete_data_set(&mut self, di: usize) {
        if di >= self.data_sets.len() {
            return;
        }
        let start = self.dataset_start_col(di);
        let w = self.data_sets[di].width().max(1);
        for _ in 0..w {
            self.delete_physical_col(start);
        }
        self.data_sets.remove(di);
    }

    /// 扩展行标题（无限扩展行时调用）。
    pub fn ensure_row_title(&mut self, physical_row: usize, title: String) {
        if self.row_titles.len() <= physical_row {
            self.row_titles.resize(physical_row + 1, String::new());
        }
        self.row_titles[physical_row] = title;
    }

    pub fn row_title(&self, r: usize) -> &str {
        self.row_titles.get(r).map(|s| s.as_str()).unwrap_or("")
    }

    /// 已有数据的最大行号 + 1（用于估算初始视口行数，未写入时为 0）。
    pub fn max_used_row(&self) -> usize {
        self.data.keys().max().map(|r| r + 1).unwrap_or(0)
    }

    // ── 统计 ────────────────────────────────────────────────────────────────

    /// 某个数据集下所有子列的非空数值，按子列顺序铺平。
    ///
    /// 并排（XY/Grouped）与堆叠（Column/Nested）都能用同一份铺平结果：
    /// 前者每个子列是一条重复序列，后者每个子列纵向堆着重复值。
    pub fn flat_values(&self, di: usize) -> Vec<f64> {
        let ds = match self.data_sets.get(di) {
            Some(d) => d,
            None => return Vec::new(),
        };
        let start = self.dataset_start_col(di);
        let w = ds.width();
        let mut v = Vec::new();
        for s in 0..w {
            for r in 0..self.max_used_row().max(self.row_titles.len()) {
                if let Some(x) = self.get_cell(r, start + s).as_f64() {
                    v.push(x);
                }
            }
        }
        v
    }

    /// 按子列分组取值：外层 = 数据集，内层 = 各子列的值向量。
    /// 嵌套 ANOVA 用它来区分「处理组」与「组内子单元」。
    pub fn sub_column_values(&self, di: usize) -> Vec<Vec<f64>> {
        let ds = match self.data_sets.get(di) {
            Some(d) => d,
            None => return Vec::new(),
        };
        let start = self.dataset_start_col(di);
        let w = ds.width();
        let rows = self.max_used_row().max(self.row_titles.len());
        (0..w)
            .map(|s| {
                (0..rows)
                    .filter_map(|r| self.get_cell(r, start + s).as_f64())
                    .collect()
            })
            .collect()
    }

    /// 整表数值矩阵视图（行主序），供卡方 / Fisher 用。
    /// 空单元格记 0；非数值单元格跳过。
    /// 清空所有单元格（保留行数、行标题、列与子列结构）。
    ///
    /// 用于「另存为模板」：版式留下，数据抹掉。
    pub fn clear_values(&mut self) {
        self.data.clear();
    }

    /// 导出成 CSV 文本：首行为列名，其后按物理列铺开。
    pub fn as_matrix_csv(&self) -> String {
        let ncol = self.total_physical_columns().max(1);
        let nrow = self.max_used_row().max(self.row_titles.len()).max(1);
        let mut head: Vec<String> = Vec::new();
        for c in 0..ncol {
            let (ds, sub) = self.col_label(c);
            head.push(if sub.is_empty() { ds } else { format!("{}.{}", ds, sub) });
        }
        let mut out = head.join(",");
        for r in 0..=nrow {
            let mut cells: Vec<String> = Vec::new();
            for c in 0..ncol {
                let v = match self.get_cell(r, c) {
                    CellValue::Empty => String::new(),
                    CellValue::Number(x) => x.to_string(),
                    CellValue::Text(s) => s,
                };
                cells.push(if v.contains(',') || v.contains('"') {
                    format!("\"{}\"", v.replace('"', "\"\""))
                } else {
                    v
                });
            }
            // 整行都空就跳过，避免尾部拖出一堆空行
            if cells.iter().all(|s| s.is_empty()) && r > nrow.saturating_sub(1) {
                continue;
            }
            out.push('\n');
            out.push_str(&cells.join(","));
        }
        out
    }

    pub fn as_matrix(&self) -> Vec<Vec<f64>> {
        let rows = self.max_used_row().max(self.row_titles.len()).max(1);
        let cols = self.total_physical_columns().max(1);
        (0..rows)
            .map(|r| {
                (0..cols)
                    .map(|c| self.get_cell(r, c).as_f64().unwrap_or(0.0))
                    .collect()
            })
            .collect()
    }

    /// 均值 / 样本标准差（n-1）/ n，用于误差棒。
    pub fn mean_sd(&self, di: usize) -> (f64, f64, usize) {
        let v = self.flat_values(di);
        let n = v.len();
        if n == 0 {
            return (0.0, 0.0, 0);
        }
        let mean = v.iter().sum::<f64>() / n as f64;
        if n < 2 {
            return (mean, 0.0, n);
        }
        let var = v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1) as f64;
        (mean, var.sqrt(), n)
    }
}

// ── 工作表包装 ──────────────────────────────────────────────────────────────

/// 数据表（Data Tables 文件夹中的一页）。
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct DataTable {
    pub name: String,
    pub table: SpectraTable,
    /// 视口行数（析光会随输入自动增长；这里是显示用的行窗口）
    pub view_rows: usize,
    /// 被排除的单元格（物理列, 行）。析光以蓝色斜体显示，分析中忽略。
    pub excluded: Vec<(usize, usize)>,
}

impl DataTable {
    /// 按类型新建一张表，视口给 12 行空行（与析光新建后一致）。
    pub fn new_by_type(name: &str, tt: TableType, num_subcols: usize) -> Self {
        let rows = match tt {
            TableType::XY => 6,
            TableType::Column => 12,
            TableType::Grouped => 8,
            TableType::Contingency => 2,
            TableType::Survival => 8,
            TableType::PartsOfWhole => 3,
            TableType::MultipleVariables => 8,
            TableType::Nested => 8,
        };
        Self {
            name: name.into(),
            table: SpectraTable::new_by_type(tt, num_subcols),
            view_rows: rows,
            excluded: Vec::new(),
        }
    }

    pub fn phys_cols(&self) -> usize {
        self.table.total_physical_columns()
    }

    pub fn phys_to_logic(&self, phys: usize) -> (usize, usize) {
        self.table.phys_to_logic(phys)
    }

    pub fn set(&mut self, phys: usize, row: usize, v: CellValue) {
        self.table.set_cell(row, phys, v);
        if row + 2 > self.view_rows {
            self.view_rows = row + 2;
        }
    }

    /// 数据集名（供图例 / 检查器色板使用）。
    pub fn dataset_names(&self) -> Vec<String> {
        self.table.data_sets.iter().map(|d| d.name.clone()).collect()
    }

    /// 按当前输入写入一个单元格，返回是否成功。
    ///
    /// 类型约束在这里收口：
    /// - 空串 → `Empty`
    /// - 数字 → `Number`（列联表取整）
    /// - 非数字：多变量表允许文本，其余类型拒绝（返回 false，保留原值）
    pub fn write_str(&mut self, phys: usize, row: usize, s: &str) -> bool {
        let s = s.trim();
        let v = if s.is_empty() {
            CellValue::Empty
        } else if let Ok(x) = s.parse::<f64>() {
            let x = if self.table.table_type.integer_only() {
                x.round()
            } else {
                x
            };
            CellValue::Number(x)
        } else if self.table.table_type.allows_text() {
            CellValue::Text(s.to_string())
        } else {
            return false;
        };
        self.set(phys, row, v);
        true
    }

    /// 单元格的显示文本（用于编辑框初值）。
    pub fn display_at(&self, phys: usize, row: usize) -> String {
        match self.table.get_cell(row, phys) {
            CellValue::Empty => String::new(),
            other => other.display(),
        }
    }

    /// 该物理列所属数据集的显示名（状态栏用）。
    pub fn column_label(&self, phys: usize) -> (String, String, String) {
        let (di, si) = self.phys_to_logic(phys);
        let letter = self.table.dataset_letter(di);
        let ds = self.table.data_sets.get(di);
        let title = ds.map(|d| d.name.clone()).unwrap_or_default();
        let sub = ds
            .and_then(|d| d.sub_columns.get(si).map(|s| s.name.clone()))
            .unwrap_or_default();
        (letter, title, sub)
    }
}

/// 结果表（Results 文件夹）。析光用分析选项卡替代旧的子页。
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ResultSheet {
    pub name: String,
    /// 分析选项卡名
    pub tabs: Vec<String>,
    pub active_tab: usize,
    /// rows[行][列]，第一列通常是行标题
    pub rows: Vec<Vec<String>>,
    /// 这份结果来自哪张数据表（工程 `tables` 的下标）。
    ///
    /// `.sprx` 的 `AnalysisSheet.inputDataSets` / `inputSheets` 要靠它定位，
    /// 没有它就只能写成空数组。旧快照没有这个字段，靠 `#[serde(default)]`
    /// 读成 `None`，不会打不开。
    #[serde(default)]
    pub source_table: Option<usize>,
}

/// 图形标注（Draw / Write / Text 三组工具的产物）。
///
/// 坐标用**绘图区归一化坐标** (0–1)，这样位图预览与 SVG 导出、
/// 以及不同缩放倍数下位置都一致。
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Annotation {
    pub kind: AnnKind,
    /// 起点（归一化）
    pub x: f32,
    pub y: f32,
    /// 终点（线 / 箭头用；文本标注不用）
    pub x2: f32,
    pub y2: f32,
    pub text: String,
    /// 字号（逻辑像素）
    pub size: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AnnKind {
    Line,
    Arrow,
    Text,
}

impl AnnKind {
    pub fn label(self) -> &'static str {
        match self {
            AnnKind::Line => "直线",
            AnnKind::Arrow => "箭头",
            AnnKind::Text => "文本",
        }
    }
}

/// 图表页（Graphs 文件夹）。
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphSheet {
    pub name: String,
    pub kind: ChartKind,
    pub title: String,
    // 轴标题：名称与单位分开存，排版时才决定斜体/正体与括号/斜杠
    pub x_name: String,
    pub x_unit: String,
    pub y_name: String,
    pub y_unit: String,
    pub unit_style: UnitStyle,
    /// 物理量符号斜体、单位正体（科研排版规范）
    pub use_italic: bool,
    /// 量符号（斜体显示），如 `c`、`t`；单位恒为正体
    pub x_sym: String,
    pub y_sym: String,
    /// 括号样式：0 半角 ()，1 全角 （）（GB 3100）
    pub bracket_style: usize,
    /// 柱宽（厘米）。0 表示用面板的分数值
    pub bar_width_cm: f32,
    /// Y 轴不从 0 开始时画断裂标记
    pub axis_break: bool,
    /// 启用右 Y 轴（双轴）
    pub right_axis: bool,
    /// 从哪个数据集起走右轴；`right_axis` 为假时忽略
    pub right_from: usize,
    /// 图注（画在 X 轴标题下方）
    pub caption: String,
    /// 网格线型：0 实线，1 虚线，2 点线
    pub grid_style: usize,
    /// 折线线型：0 实线，1 虚线，2 点线
    pub dash: usize,
    /// 参与绘图的数据表索引
    pub source_table: usize,
    pub show_error: bool,
    pub show_scatter: bool,
    pub show_legend: bool,
    pub show_bracket: bool,
    /// 括号高度模式
    pub bracket_mode: BracketMode,
    /// 绝对高度值 / 相对倍率
    pub bracket_value: f64,
    /// 配色方案在 `COLORMAPS` 中的索引（0 = Okabe-Ito，色盲友好）
    pub colormap: usize,
    /// 显著性对比（括号 + 星号），按层级绘制
    pub comparisons: Vec<Comparison>,
    /// 图形标注（Draw / Write 工具绘制，Text 组调整字号）
    pub annotations: Vec<Annotation>,
    /// 标注默认字号
    pub ann_size: f32,
    /// 边框样式：L 型（析光默认）/ 全框
    pub frame: crate::plot::FrameStyle,
    // ── 符号与散点 ────────────────────────────────────────────────────────
    pub symbol_follows_bar: bool,
    pub symbol_color_idx: usize,
    pub symbol_size: f32,
    pub jitter: f32,
    // ── ② 数据 ──────────────────────────────────────────────────────────────
    /// 误差棒量：0 = SD，1 = SEM，2 = 95% CI
    pub err_kind: usize,
    /// 误差棒方向：0 = 双向，1 = 只向上，2 = 只向下
    pub err_dir: usize,
    /// 误差棒线宽（px）
    pub err_width: f32,
    /// 各组均值连成折线
    pub connect: bool,
    // ── ③ 元素 ──────────────────────────────────────────────────────────────
    /// 柱宽 / 每组可用宽度
    pub bar_width: f32,
    /// 柱体图案：0 实心，1 斜线，2 交叉，3 横纹
    pub bar_pattern: usize,
    /// 基线强制为 0
    pub bar_zero: bool,
    /// 折线下方填充
    pub area_fill: bool,
    // ── ④ 坐标轴 ────────────────────────────────────────────────────────────
    /// 水平网格线
    pub grid_on: bool,
    /// Y 轴对数刻度
    pub log_y: bool,
    // ── ⑤ 配色 ──────────────────────────────────────────────────────────────
    /// 灰度模式
    pub grayscale: bool,
    // ── ⑦ 图例与线条 ────────────────────────────────────────────────────────
    /// 图例位置：0 右侧外，1 右上内，2 左上内
    pub legend_pos: usize,
    /// 全局线宽倍率
    pub line_width: f32,
    // ── 散点 ────────────────────────────────────────────────────────────────
    /// 符号形状：0 圆，1 方，2 三角，3 菱形
    pub symbol_shape: usize,
    /// 散点描边环宽（px）
    pub symbol_border: f32,
    /// 散点内部不透明度
    pub symbol_alpha: f32,
    // ── ① 图形 ──────────────────────────────────────────────────────────────
    /// 图标题加粗 / 倾斜
    pub title_bold: bool,
    pub title_italic: bool,
    // ── ⑧ 图类型专属 ────────────────────────────────────────────────────────
    /// 直方图 / 热图的分箱数
    pub bins: usize,
    /// 环形图内径比例（0 = 实心饼图）
    pub pie_inner: f32,
    /// 起始角（度）
    pub pie_start: f32,
    /// 热图色阶自动取值
    pub heat_auto: bool,
    pub heat_lo: f64,
    pub heat_hi: f64,
    /// 森林图参考线
    pub forest_ref: f64,
    /// 生存曲线画删失标记
    pub surv_censor: bool,
    /// 百分比点阵图（华夫图）的列数
    pub waffle_cols: usize,
    /// 100% 堆叠：各柱归一化到等高
    pub stacked_100: bool,
    /// 分面图共享同一套轴范围
    pub facet_shared: bool,
    /// 箱线样式：0 带须，1 只画盒
    pub box_style: usize,
}

/// 一个析光工程文件（.spectra）：5 个部分各自最多 500 页。
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Project {
    /// Data Tables
    pub tables: Vec<DataTable>,
    /// Info
    pub infos: Vec<String>,
    /// Results
    pub results: Vec<ResultSheet>,
    /// Graphs
    pub graphs: Vec<GraphSheet>,
    /// Layouts
    pub layouts: Vec<String>,
}

/// 导航器 5 个文件夹（顺序固定，不可增删）。存 i18n key，显示时按当前语言取文案。
pub const FOLDERS: [K; 4] = [
    K::FolderDataTables,
    K::FolderInfo,
    K::FolderResults,
    K::FolderGraphs,
];

/// 当前打开的工作表。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Active {
    Data(usize),
    Info(usize),
    Result(usize),
    Graph(usize),
    Layout(usize),
}

impl Active {
    /// 所属文件夹索引（0..5）
    pub fn folder(self) -> usize {
        match self {
            Active::Data(_) => 0,
            Active::Info(_) => 1,
            Active::Result(_) => 2,
            Active::Graph(_) => 3,
            Active::Layout(_) => 4,
        }
    }

    pub fn index(self) -> usize {
        match self {
            Active::Data(i) | Active::Info(i) | Active::Result(i) | Active::Graph(i)
            | Active::Layout(i) => i,
        }
    }
}

// ── 示例工程 ────────────────────────────────────────────────────────────────

/// 构造**启动工程**：Grouped 表，两组各 3 个并排重复子列，**不含任何数值**。
///
/// ★ 以前这里预填了 Control / Treated 两组共 48 个演示数值，还带一个
///   「排除值」标记。用户新建图表时看到的是别人的数据，而且图上凭空多出
///   标记，分不清哪些是自己录的。现在只留表结构，数据一律由用户录入。
pub fn sample_project() -> Project {
    let mut t = DataTable::new_by_type("Data 1", TableType::Grouped, 3);
    t.view_rows = 8;
    // 不写入任何数值、行标题与排除标记 —— 空表。

    let graph = GraphSheet {
        name: "Data 1".into(),
        kind: ChartKind::Bar,
        title: "Data 1".into(),
        x_name: "Group".into(),
        x_unit: String::new(),
        y_name: "Response".into(),
        y_unit: String::new(),
        unit_style: UnitStyle::Paren,
        use_italic: true,
        x_sym: String::new(),
        y_sym: String::new(),
        bracket_style: 0,
        bar_width_cm: 0.0,
        axis_break: true,
        right_axis: false,
        right_from: 0,
        caption: String::new(),
        grid_style: 0,
        dash: 0,

        source_table: 0,
        show_error: true,
        show_scatter: true,
        show_legend: true,
        // 默认**不**显示显著性标注，也不预置任何示例对比 —— 用户没添加过对比
        // 就不该凭空冒出一对 "**"。
        show_bracket: false,
        bracket_mode: BracketMode::Auto,
        bracket_value: 1.15,
        // 启动时的示例图也要听「设置 ▸ 默认配色」：写死 0 的话，用户改了
        // 默认配色后打开软件看到的仍是第一套，会以为设置没生效。
        colormap: crate::settings::get().def_colormap,
        comparisons: Vec::new(),
        annotations: Vec::new(),
        ann_size: 12.0,
        frame: crate::plot::FrameStyle::L,
        symbol_follows_bar: true,
        symbol_color_idx: 0,
        symbol_size: 2.0,
        jitter: 0.42,
        err_kind: 0,
        err_dir: 0,
        err_width: 2.0,
        connect: false,
        bar_width: 0.55,
        bar_pattern: 0,
        bar_zero: true,
        area_fill: false,
        grid_on: false,
        log_y: false,
        grayscale: false,
        legend_pos: 0,
        line_width: 1.0,
        symbol_shape: 0,
        symbol_border: 1.5,
        symbol_alpha: 1.0,
        title_bold: true,
        title_italic: false,
        bins: 8,
        pie_inner: 0.0,
        pie_start: 0.0,
        heat_auto: true,
        heat_lo: 0.0,
        heat_hi: 1.0,
        forest_ref: 0.0,
        surv_censor: true,
        waffle_cols: 10,
        stacked_100: false,
        facet_shared: true,
        box_style: 0,
    };

    Project {
        tables: vec![t],
        infos: vec!["Project info 1".into()],
        // 示例工程**不带任何结果**。
        // 之前这里硬编码了一张 "Unpaired t test of Data 1"，那是写死的数字。
        // 用户没输入数据时却在结果里看到一份分析，来源就是它。
        results: Vec::new(),
        graphs: vec![graph],
        layouts: vec!["Layout 1".into()],
    }
}

// ── 工厂与稀疏层的验收测试 ──────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouped_factory_and_sparse_write() {
        let mut table = SpectraTable::new_by_type(TableType::Grouped, 3);
        assert_eq!(table.data_sets.len(), 2);
        assert_eq!(table.data_sets[0].sub_columns.len(), 3);
        assert_eq!(table.row_titles, vec!["Male", "Female"]);

        table.set_cell(0, 0, CellValue::Number(10.5));
        table.set_cell(2, 5, CellValue::Number(22.1)); // 第 2 行自动扩展

        assert_eq!(table.get_cell(0, 0), CellValue::Number(10.5));
        assert_eq!(table.get_cell(2, 5), CellValue::Number(22.1));
        assert_eq!(table.get_cell(2, 4), CellValue::Empty);
        assert_eq!(table.get_cell(999_999, 0), CellValue::Empty);
    }

    #[test]
    fn xy_first_column_is_x_without_subcolumns() {
        let t = SpectraTable::new_by_type(TableType::XY, 2);
        assert_eq!(t.data_sets[0].name, "X");
        assert_eq!(t.data_sets[0].width(), 1);
        assert_eq!(t.data_sets[1].sub_columns.len(), 2);
        assert_eq!(t.total_physical_columns(), 5); // X + A×2 + B×2
        assert_eq!(t.dataset_letter(0), "X");
        assert_eq!(t.dataset_letter(1), "A");
    }

    #[test]
    fn nested_forces_min_two_subcolumns() {
        let t = SpectraTable::new_by_type(TableType::Nested, 1);
        assert!(t.data_sets[0].sub_columns.len() >= 2);
    }

    #[test]
    fn no_subcolumn_types_have_width_one() {
        for tt in [
            TableType::Column,
            TableType::Contingency,
            TableType::Survival,
            TableType::PartsOfWhole,
            TableType::MultipleVariables,
        ] {
            let t = SpectraTable::new_by_type(tt, 3);
            assert!(t.data_sets.iter().all(|d| d.sub_columns.is_empty()));
            assert_eq!(t.total_physical_columns(), t.data_sets.len());
        }
    }

    #[test]
    fn writing_at_row_one_million_costs_one_entry() {
        let mut t = SpectraTable::new_by_type(TableType::XY, 1);
        t.set_cell(1_000_000, 3, CellValue::Number(7.0));
        assert_eq!(t.get_cell(1_000_000, 3), CellValue::Number(7.0));
        // 稀疏：只存了 1 行 1 列
        assert_eq!(t.data.len(), 1);
        assert_eq!(t.data[&1_000_000].len(), 1);
        assert_eq!(t.max_used_row(), 1_000_001);
    }

    #[test]
    /// 列也是无限的：往任意一列写入都会把结构补齐到那一列。
    fn columns_are_unbounded() {
        let mut t = SpectraTable::new_by_type(TableType::Column, 0);
        assert_eq!(t.total_physical_columns(), 2); // A, B
        t.set_cell(0, 30, CellValue::Number(5.0));
        // 补到 31 列：A..Z + AA..AE
        assert!(t.total_physical_columns() > 30);
        assert_eq!(t.get_cell(0, 30), CellValue::Number(5.0));
        // 中间没写过的列安全返回 Empty
        assert_eq!(t.get_cell(0, 29), CellValue::Empty);
        // 稀疏性：只存了 1 行 1 列
        assert_eq!(t.data.len(), 1);
        assert_eq!(t.data[&0].len(), 1);
        // 虚拟列标签连续
        let (n0, _) = t.col_label(0);
        assert_eq!(n0, "A");
    }

    fn write_str_respects_type_rules() {
        let mut t = DataTable::new_by_type("t", TableType::Grouped, 2);
        assert!(t.write_str(0, 0, "12.5"));
        assert_eq!(t.table.get_cell(0, 0), CellValue::Number(12.5));
        assert!(t.write_str(0, 0, ""));
        assert_eq!(t.table.get_cell(0, 0), CellValue::Empty);
        // 分组表不接受文本
        assert!(!t.write_str(0, 0, "abc"));
        assert_eq!(t.table.get_cell(0, 0), CellValue::Empty);

        // 列联表取整
        let mut c = DataTable::new_by_type("c", TableType::Contingency, 0);
        assert!(c.write_str(0, 0, "7.6"));
        assert_eq!(c.table.get_cell(0, 0), CellValue::Number(8.0));

        // 多变量表允许文本
        let mut m = DataTable::new_by_type("m", TableType::MultipleVariables, 0);
        assert!(m.write_str(2, 0, "Control"));
        assert_eq!(
            m.table.get_cell(0, 2),
            CellValue::Text("Control".to_string())
        );
    }

    #[test]
    fn row_title_grows_without_preallocation() {
        let mut t = SpectraTable::new_by_type(TableType::Grouped, 1);
        t.ensure_row_title(5000, "Group 5001".into());
        assert_eq!(t.row_title(5000), "Group 5001");
        assert_eq!(t.row_title(4999), "");
    }

    #[test]
    fn flat_values_covers_all_subcolumns() {
        let mut t = SpectraTable::new_by_type(TableType::Grouped, 3);
        // Control 的三个并排子列各写 1 个值
        t.set_cell(0, 0, CellValue::Number(1.0));
        t.set_cell(0, 1, CellValue::Number(2.0));
        t.set_cell(0, 2, CellValue::Number(3.0));
        let v = t.flat_values(0);
        assert_eq!(v.len(), 3);
        assert!((t.mean_sd(0).0 - 2.0).abs() < 1e-9);
    }

    #[test]
    fn phys_to_logic_roundtrip() {
        let t = SpectraTable::new_by_type(TableType::Grouped, 3);
        assert_eq!(t.phys_to_logic(0), (0, 0));
        assert_eq!(t.phys_to_logic(2), (0, 2));
        assert_eq!(t.phys_to_logic(3), (1, 0));
        assert_eq!(t.phys_to_logic(5), (1, 2));
    }
}

// ── 工程文件读写（纯文本，无外部依赖）──────────────────────────────────────
//
// 格式（每行一条记录，字段用 \t 分隔）：
//   SPECTRA1
//   T\t<表名>\t<类型>\t<view_rows>
//   RT\t<行号>\t<行标题>
//   DS\t<数据集名>\t<子列名|子列名|…>
//   C\t<物理行>\t<物理列>\t<N|T|E>\t<值>
//   R\t<结果名>\t<选项卡|…>
//   RR\t<索引>\t<列|…>
//   G\t<图名>\t<kind>\t<title>\t<x_name>\t<x_unit>\t<y_name>\t<y_unit>\t<unit_style>\t<italic>\t<src>\t<err>\t<sc>\t<lg>\t<br>\t<cmap>\t<br_mode>\t<br_val>\t<follow>\t<sym>\t<symsize>\t<jit>
//   L\t<布局名>
//   I\t<信息名>
//
// 文本中的 \n 与 \t 会被转义，保证任意内容都能无损往返。

const MAGIC: &str = "SPECTRA1";

#[allow(dead_code)]
fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n")
}

fn unesc(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[allow(dead_code)]
fn type_idx(t: TableType) -> u8 {
    match t {
        TableType::XY => 0,
        TableType::Column => 1,
        TableType::Grouped => 2,
        TableType::Contingency => 3,
        TableType::Survival => 4,
        TableType::PartsOfWhole => 5,
        TableType::MultipleVariables => 6,
        TableType::Nested => 7,
    }
}

fn type_from_idx(i: u8) -> TableType {
    match i {
        1 => TableType::Column,
        2 => TableType::Grouped,
        3 => TableType::Contingency,
        4 => TableType::Survival,
        5 => TableType::PartsOfWhole,
        6 => TableType::MultipleVariables,
        7 => TableType::Nested,
        _ => TableType::XY,
    }
}

impl GraphSheet {
    /// 一张默认柱形图（L 型边框、显示误差棒与散点、Okabe-Ito 配色）。
    pub fn default_for(source_table: usize) -> Self {
        Self {
            name: "Data 1".to_string(),
            kind: ChartKind::Bar,
            title: String::new(),
            x_name: "Group".to_string(),
            x_unit: String::new(),
            y_name: "Response".to_string(),
            y_unit: String::new(),
            unit_style: UnitStyle::Paren,
            use_italic: true,
            x_sym: String::new(),
            y_sym: String::new(),
            bracket_style: 0,
            bar_width_cm: 0.0,
            axis_break: true,
            right_axis: false,
            right_from: 0,
            caption: String::new(),
            grid_style: 0,
            dash: 0,
            source_table,
            show_error: true,
            show_scatter: true,
            show_legend: true,
            show_bracket: false,
            bracket_mode: BracketMode::Auto,
            bracket_value: 1.15,
            colormap: 0,
            comparisons: Vec::new(),
            annotations: Vec::new(),
            ann_size: 12.0,
            frame: crate::plot::FrameStyle::L,
            symbol_follows_bar: true,
            symbol_color_idx: 0,
            symbol_size: 2.0,
            jitter: 0.42,
            err_kind: 0,
            err_dir: 0,
            err_width: 2.0,
            connect: false,
            bar_width: 0.55,
            bar_pattern: 0,
            bar_zero: true,
            area_fill: false,
            grid_on: false,
            log_y: false,
            grayscale: false,
            legend_pos: 0,
            line_width: 1.0,
            symbol_shape: 0,
            symbol_border: 1.5,
            symbol_alpha: 1.0,
            title_bold: true,
            title_italic: false,
            bins: 8,
            pie_inner: 0.0,
            pie_start: 0.0,
            heat_auto: true,
            heat_lo: 0.0,
            heat_hi: 1.0,
            forest_ref: 0.0,
            surv_censor: true,
        waffle_cols: 10,
        stacked_100: false,
        facet_shared: true,
        box_style: 0,
        }
    }
}

impl Project {
    /// 空白工程：一张 Column 表，其余为空。
    /// File ▸ 新建项目 / 全部关闭 用它重置。
    pub fn blank() -> Project {
        // 图形也要有一张：否则 File ▸ 新建项目 之后切到图表页，
        // `chart_view` 拿不到图形，画面空白（以前还会 panic）。
        let mut g = GraphSheet::default_for(0);
        g.name = "Data 1".to_string();
        Project {
            tables: vec![DataTable::new_by_type("Data 1", TableType::Column, 0)],
            infos: Vec::new(),
            results: Vec::new(),
            graphs: vec![g],
            layouts: Vec::new(),
        }
    }

    /// 序列化为文本。
    #[allow(dead_code)]
    pub fn save_text(&self) -> String {
        let mut o = String::from(MAGIC);
        o.push('\n');
        for t in &self.tables {
            o.push_str(&format!(
                "T\t{}\t{}\t{}\n",
                esc(&t.name),
                type_idx(t.table.table_type),
                t.view_rows
            ));
            for (i, rt) in t.table.row_titles.iter().enumerate() {
                if !rt.is_empty() {
                    o.push_str(&format!("RT\t{}\t{}\n", i, esc(rt)));
                }
            }
            for ds in &t.table.data_sets {
                let subs: Vec<String> = ds.sub_columns.iter().map(|s| esc(&s.name)).collect();
                o.push_str(&format!("DS\t{}\t{}\n", esc(&ds.name), subs.join("|")));
            }
            let mut rows: Vec<&usize> = t.table.data.keys().collect();
            rows.sort_unstable();
            for r in rows {
                if let Some(row) = t.table.data.get(r) {
                    for (c, v) in row.iter() {
                        let (k, s) = match v {
                            CellValue::Number(x) => ('N', format!("{}", x)),
                            CellValue::Text(s) => ('T', esc(s)),
                            CellValue::Empty => ('E', String::new()),
                        };
                        o.push_str(&format!("C\t{}\t{}\t{}\t{}\n", r, c, k, s));
                    }
                }
            }
        }
        for i in &self.infos {
            o.push_str(&format!("I\t{}\n", esc(i)));
        }
        for r in &self.results {
            o.push_str(&format!(
                "R\t{}\t{}\n",
                esc(&r.name),
                r.tabs.iter().map(|x| esc(x)).collect::<Vec<_>>().join("|")
            ));
            for row in &r.rows {
                o.push_str(&format!(
                    "RR\t{}\n",
                    row.iter().map(|x| esc(x)).collect::<Vec<_>>().join("|")
                ));
            }
        }
        for g in &self.graphs {
            // 字段打包成数组再 join —— 避免手写一长串占位符数错个数
                let gf: Vec<String> = vec![
                    esc(&g.name),
                    match g.kind {
                        // 62 种图型统一走判别值，新增图型不用再改这里
                        k => k.to_usize().to_string(),
                    },
                    esc(&g.title),
                    esc(&g.x_name),
                    esc(&g.x_unit),
                    esc(&g.y_name),
                    esc(&g.y_unit),
                    g.unit_style.to_i32().to_string(),
                    (g.use_italic as u8).to_string(),
                    g.source_table.to_string(),
                    (g.show_error as u8).to_string(),
                    (g.show_scatter as u8).to_string(),
                    (g.show_legend as u8).to_string(),
                    (g.show_bracket as u8).to_string(),
                    g.colormap.to_string(),
                    match g.bracket_mode {
                        crate::plot::BracketMode::Auto => "0".into(),
                        crate::plot::BracketMode::Absolute => "1".into(),
                        crate::plot::BracketMode::Relative => "2".into(),
                    },
                    format!("{}", g.bracket_value),
                    (g.symbol_follows_bar as u8).to_string(),
                    g.symbol_color_idx.to_string(),
                    format!("{}", g.symbol_size),
                    format!("{}", g.jitter),
                    // ── 22.. 绘图区面板新增项 ──
                    g.err_kind.to_string(),
                    g.err_dir.to_string(),
                    format!("{}", g.err_width),
                    (g.connect as u8).to_string(),
                    format!("{}", g.bar_width),
                    g.bar_pattern.to_string(),
                    (g.bar_zero as u8).to_string(),
                    (g.area_fill as u8).to_string(),
                    (g.grid_on as u8).to_string(),
                    (g.log_y as u8).to_string(),
                    (g.grayscale as u8).to_string(),
                    g.legend_pos.to_string(),
                    format!("{}", g.line_width),
                    g.symbol_shape.to_string(),
                    format!("{}", g.symbol_border),
                    format!("{}", g.symbol_alpha),
                    (g.title_bold as u8).to_string(),
                    (g.title_italic as u8).to_string(),
                    // ── 40.. 图类型专属 ──
                    g.bins.to_string(),
                    format!("{}", g.pie_inner),
                    format!("{}", g.pie_start),
                    (g.heat_auto as u8).to_string(),
                    format!("{}", g.heat_lo),
                    format!("{}", g.heat_hi),
                    format!("{}", g.forest_ref),
                    (g.surv_censor as u8).to_string(),
                    g.waffle_cols.to_string(),
                    (g.stacked_100 as u8).to_string(),
                    (g.facet_shared as u8).to_string(),
                    g.box_style.to_string(),
                    // ── 48.. 规范新增：符号 / 括号 / 柱宽 / 双轴 / 图注 / 线型 ──
                    esc(&g.x_sym),
                    esc(&g.y_sym),
                    g.bracket_style.to_string(),
                    format!("{}", g.bar_width_cm),
                    (g.axis_break as u8).to_string(),
                    (g.right_axis as u8).to_string(),
                    g.right_from.to_string(),
                    esc(&g.caption),
                    g.grid_style.to_string(),
                    g.dash.to_string(),
                ];
                o.push_str(&format!("G\t{}\n", gf.join("\t")));
            for cmp in &g.comparisons {
                let pcol = cmp
                    .p
                    .map(|v| format!("{:.10}", v))
                    .unwrap_or_default();
                o.push_str(&format!(
                    "GC\t{}\t{}\t{}\t{}\n",
                    cmp.a,
                    cmp.b,
                    esc(&cmp.label),
                    pcol
                ));
            }
            o.push_str(&format!("GA\t{}\n", g.ann_size));
            for a in &g.annotations {
                o.push_str(&format!(
                    "AN\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                    match a.kind {
                        AnnKind::Line => 0,
                        AnnKind::Arrow => 1,
                        AnnKind::Text => 2,
                    },
                    a.x,
                    a.y,
                    a.x2,
                    a.y2,
                    a.size,
                    esc(&a.text)
                ));
            }
        }
        for l in &self.layouts {
            o.push_str(&format!("L\t{}\n", esc(l)));
        }
        o
    }

    /// 从文本恢复。任何一行解析失败都会被跳过，不会整体失败。
    pub fn load_text(s: &str) -> Option<Project> {
        let mut lines = s.lines();
        if lines.next()?.trim() != MAGIC {
            return None;
        }
        let mut p = Project {
            tables: Vec::new(),
            infos: Vec::new(),
            results: Vec::new(),
            graphs: Vec::new(),
            layouts: Vec::new(),
        };
        let mut cur_t: Option<usize> = None;
        let mut cur_r: Option<usize> = None;
        let mut cur_g: Option<usize> = None;

        for line in lines {
            let f: Vec<&str> = line.split('\t').collect();
            if f.is_empty() {
                continue;
            }
            match f[0] {
                "T" if f.len() >= 4 => {
                    let tt = type_from_idx(f[2].parse().unwrap_or(0));
                    let mut t = SpectraTable::new_by_type(tt, 0);
                    // 结构与数据全部由文件里的 RT / DS / C 行重建，
                    // 所以先清掉工厂生成的默认结构，避免重复。
                    t.row_titles.clear();
                    t.data_sets.clear();
                    p.tables.push(DataTable {
                        name: unesc(f[1]),
                        table: t,
                        view_rows: f[3].parse().unwrap_or(20),
                        excluded: Vec::new(),
                    });
                    cur_t = Some(p.tables.len() - 1);
                    cur_r = None;
                    cur_g = None;
                }
                "RT" if f.len() >= 3 => {
                    if let (Some(ti), Ok(r)) = (cur_t, f[1].parse::<usize>()) {
                        p.tables[ti].table.ensure_row_title(r, unesc(f[2]));
                    }
                }
                "DS" if f.len() >= 3 => {
                    if let Some(ti) = cur_t {
                        let subs: Vec<SubColumnMeta> = if f[2].is_empty() {
                            Vec::new()
                        } else {
                            f[2].split('|').map(|x| SubColumnMeta { name: unesc(x) }).collect()
                        };
                        p.tables[ti].table.data_sets.push(DataSetMeta {
                            name: unesc(f[1]),
                            sub_columns: subs,
                        });
                    }
                }
                "C" if f.len() >= 4 => {
                    if let Some(ti) = cur_t {
                        let r = f[1].parse().unwrap_or(0);
                        let c = f[2].parse().unwrap_or(0);
                        let v = match f[3].chars().next().unwrap_or('E') {
                            'N' => CellValue::Number(f.get(4).and_then(|x| x.parse().ok()).unwrap_or(0.0)),
                            'T' => CellValue::Text(unesc(f.get(4).copied().unwrap_or(""))),
                            _ => CellValue::Empty,
                        };
                        p.tables[ti].table.set_cell(r, c, v);
                    }
                }
                "I" if f.len() >= 2 => p.infos.push(unesc(f[1])),
                "R" if f.len() >= 3 => {
                    p.results.push(ResultSheet {
                        name: unesc(f[1]),
                        tabs: f[2].split('|').map(unesc).collect(),
                        active_tab: 0,
                        rows: Vec::new(),
                        source_table: cur_t,
                    });
                    cur_r = Some(p.results.len() - 1);
                    cur_t = None;
                    cur_g = None;
                }
                "RR" if f.len() >= 2 => {
                    if let Some(ri) = cur_r {
                        p.results[ri].rows.push(f[1].split('|').map(unesc).collect());
                    }
                }
                "G" if f.len() >= 22 => {
                    p.graphs.push(GraphSheet {
                        name: unesc(f[1]),
                        kind: match f[2].parse().unwrap_or(0) {
                            v => crate::plot::ChartKind::from_usize(v),
                        },
                        title: unesc(f[3]),
                        x_name: unesc(f[4]),
                        x_unit: unesc(f[5]),
                        y_name: unesc(f[6]),
                        y_unit: unesc(f[7]),
                        unit_style: crate::plot::UnitStyle::from_i32(f[8].parse().unwrap_or(0)),
                        use_italic: f[9].parse::<u8>().unwrap_or(1) != 0,
                        x_sym: f.get(48).map(|x| unesc(x)).unwrap_or_default(),
                        y_sym: f.get(49).map(|x| unesc(x)).unwrap_or_default(),
                        bracket_style: f.get(50).and_then(|x| x.parse().ok()).unwrap_or(0),
                        bar_width_cm: f.get(51).and_then(|x| x.parse().ok()).unwrap_or(0.0),
                        axis_break: f.get(52).map(|x| *x != "0").unwrap_or(true),
                        right_axis: f.get(53).map(|x| *x == "1").unwrap_or(false),
                        right_from: f.get(54).and_then(|x| x.parse().ok()).unwrap_or(0),
                        caption: f.get(55).map(|x| unesc(x)).unwrap_or_default(),
                        grid_style: f.get(56).and_then(|x| x.parse().ok()).unwrap_or(0),
                        dash: f.get(57).and_then(|x| x.parse().ok()).unwrap_or(0),

                        source_table: f[10].parse().unwrap_or(0),
                        show_error: f[11].parse::<u8>().unwrap_or(1) != 0,
                        show_scatter: f[12].parse::<u8>().unwrap_or(1) != 0,
                        show_legend: f[13].parse::<u8>().unwrap_or(1) != 0,
                        show_bracket: f[14].parse::<u8>().unwrap_or(0) != 0,
                        colormap: f[15].parse().unwrap_or(0),
                        bracket_mode: match f[16].parse().unwrap_or(0) {
                            1 => crate::plot::BracketMode::Absolute,
                            2 => crate::plot::BracketMode::Relative,
                            _ => crate::plot::BracketMode::Auto,
                        },
                        bracket_value: f[17].parse().unwrap_or(1.15),
                        symbol_follows_bar: f[18].parse::<u8>().unwrap_or(1) != 0,
                        symbol_color_idx: f[19].parse().unwrap_or(0),
                        symbol_size: f[20].parse().unwrap_or(2.0),
                        jitter: f[21].parse().unwrap_or(0.42),
                        err_kind: f.get(22).and_then(|x| x.parse().ok()).unwrap_or(0),
                        err_dir: f.get(23).and_then(|x| x.parse().ok()).unwrap_or(0),
                        err_width: f.get(24).and_then(|x| x.parse().ok()).unwrap_or(2.0),
                        connect: f.get(25).map(|x| *x == "1").unwrap_or(false),
                        bar_width: f.get(26).and_then(|x| x.parse().ok()).unwrap_or(0.55),
                        bar_pattern: f.get(27).and_then(|x| x.parse().ok()).unwrap_or(0),
                        bar_zero: f.get(28).map(|x| *x != "0").unwrap_or(true),
                        area_fill: f.get(29).map(|x| *x == "1").unwrap_or(false),
                        grid_on: f.get(30).map(|x| *x == "1").unwrap_or(false),
                        log_y: f.get(31).map(|x| *x == "1").unwrap_or(false),
                        grayscale: f.get(32).map(|x| *x == "1").unwrap_or(false),
                        legend_pos: f.get(33).and_then(|x| x.parse().ok()).unwrap_or(0),
                        line_width: f.get(34).and_then(|x| x.parse().ok()).unwrap_or(1.0),
                        symbol_shape: f.get(35).and_then(|x| x.parse().ok()).unwrap_or(0),
                        symbol_border: f.get(36).and_then(|x| x.parse().ok()).unwrap_or(1.5),
                        symbol_alpha: f.get(37).and_then(|x| x.parse().ok()).unwrap_or(1.0),
                        title_bold: f.get(38).map(|x| *x != "0").unwrap_or(true),
                        title_italic: f.get(39).map(|x| *x == "1").unwrap_or(false),
                        bins: f.get(40).and_then(|x| x.parse().ok()).unwrap_or(8),
                        pie_inner: f.get(41).and_then(|x| x.parse().ok()).unwrap_or(0.0),
                        pie_start: f.get(42).and_then(|x| x.parse().ok()).unwrap_or(0.0),
                        heat_auto: f.get(43).map(|x| *x != "0").unwrap_or(true),
                        heat_lo: f.get(44).and_then(|x| x.parse().ok()).unwrap_or(0.0),
                        heat_hi: f.get(45).and_then(|x| x.parse().ok()).unwrap_or(1.0),
                        forest_ref: f.get(46).and_then(|x| x.parse().ok()).unwrap_or(0.0),
                        surv_censor: f.get(47).map(|x| *x != "0").unwrap_or(true),
                        waffle_cols: f.get(58).and_then(|x| x.parse().ok()).unwrap_or(10),
                        stacked_100: f.get(59).map(|x| *x != "0").unwrap_or(false),
                        facet_shared: f.get(60).map(|x| *x != "0").unwrap_or(true),
                        box_style: f.get(61).and_then(|x| x.parse().ok()).unwrap_or(0),
                        comparisons: Vec::new(),
                        annotations: Vec::new(),
                        ann_size: 12.0,
        frame: crate::plot::FrameStyle::L,
                    });
                    cur_g = Some(p.graphs.len() - 1);
                    cur_t = None;
                    cur_r = None;
                }
                "GC" if f.len() >= 4 => {
                    if let Some(gi) = cur_g {
                        p.graphs[gi].comparisons.push(Comparison {
                            a: f[1].parse().unwrap_or(0),
                            b: f[2].parse().unwrap_or(1),
                            label: unesc(f[3]),
                            // 旧文件没有存 P 值；重新定级在改组号时会补上
                            p: f.get(4).and_then(|x| x.parse().ok()),
                        });
                    }
                }
                "GA" if f.len() >= 2 => {
                    if let Some(gi) = cur_g {
                        p.graphs[gi].ann_size = f[1].parse().unwrap_or(12.0);
                    }
                }
                "AN" if f.len() >= 8 => {
                    if let Some(gi) = cur_g {
                        p.graphs[gi].annotations.push(Annotation {
                            kind: match f[1].parse().unwrap_or(0) {
                                1 => AnnKind::Arrow,
                                2 => AnnKind::Text,
                                _ => AnnKind::Line,
                            },
                            x: f[2].parse().unwrap_or(0.2),
                            y: f[3].parse().unwrap_or(0.7),
                            x2: f[4].parse().unwrap_or(0.6),
                            y2: f[5].parse().unwrap_or(0.6),
                            size: f[6].parse().unwrap_or(12.0),
                            text: unesc(f[7]),
                        });
                    }
                }
                "L" if f.len() >= 2 => p.layouts.push(unesc(f[1])),
                _ => {}
            }
        }
        if p.tables.is_empty() {
            p.tables.push(DataTable::new_by_type("Data 1", TableType::Grouped, 3));
        }
        Some(p)
    }
}
