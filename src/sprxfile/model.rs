//! `.sprx` 的对象模型 —— 字段名与 JSON 形状逐条对齐《`.sprx` 完整实现规范》。
//!
//! # 与运行期模型的关系
//!
//! 这里的对象是**落盘形态**，不等同于 [`crate::model::Project`]：
//! - 列单独成池（`columns/<id>.json`），CSV 里只存值，列标题不在 CSV 中
//! - 分析结果的 `content` 是 `serde_json::Value`，**不强类型化**
//!   （不同 `analysisClass` 的结构本来就不一样，硬定一个 struct 只会到处 `Option`）
//! - 图表的 `options` 同样是自由 JSON，这是 `.sprx` 相对 `.prism` 最大的保真优势
//!
//! # 三处最容易写错的地方
//!
//! 1. `mimetype` 必须是 ZIP 第 0 项、Stored、内容精确 —— 见 [`crate::sprxfile::MIMETYPE`]
//! 2. `manifest.sheets` 是**对象**不是数组，且空清单写 `null` 而不是 `[]`
//!    （见 [`SheetLists::set`]）
//! 3. 不写 `spectra/snapshot.json` 就等于放弃无损，见 `writer::write`
//!
//! # 前向兼容
//!
//! 除 [`crate::model::Project`] 外，每个结构体都带 `#[serde(flatten)] extra`：
//! 未知字段会被保留并原样回写，用新版本打开旧文件、再用旧版本存回去，
//! 不会把新字段抹掉。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ══════════════════════════════════════════════════════════════════════════
// 二进制条目
// ══════════════════════════════════════════════════════════════════════════

/// 只读不解析的二进制条目（`.bin` / `.dt`）。
///
/// 读端只登记存在性（[`crate::sprxfile::reader::binary_entries`]），绝不解析内容；
/// 写端在组装 ZIP 时**再跳一次**，双保险 —— 把未知二进制写回去没有意义。
pub fn is_forbidden_binary(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".bin") || n.ends_with(".dt")
}

// ══════════════════════════════════════════════════════════════════════════
// 富文本标题
// ══════════════════════════════════════════════════════════════════════════

/// 一段带格式信息的文本。
///
/// 规范 6.1 的形状是 `{ "range": "0", "attributes": { ... } }` —— `range` 是
/// **字符串**形式的偏移，属性是自由字典，所以这里不写死上标 / 粗体之类。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Formatting {
    /// 起始偏移（字符串形式，如 `"0"`）
    #[serde(default)]
    pub range: String,
    /// 属性字典，如 `{ "horizontalAlignment": "center" }`
    #[serde(default)]
    pub attributes: BTreeMap<String, serde_json::Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// 列标题。值与格式分开存——`string` 永远是纯文本，便于搜索与排序。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RichTitle {
    #[serde(default)]
    pub string: String,
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub formatting: Vec<Formatting>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl RichTitle {
    pub fn new(s: impl Into<String>) -> Self {
        let mut attributes = BTreeMap::new();
        attributes.insert(
            "horizontalAlignment".to_string(),
            serde_json::Value::String("center".to_string()),
        );
        Self {
            string: s.into(),
            version: 1,
            formatting: vec![Formatting { range: "0".to_string(), attributes, extra: BTreeMap::new() }],
            extra: BTreeMap::new(),
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════
// 列（columns/<id>.json）
// ══════════════════════════════════════════════════════════════════════════

/// 一个 replicate 的元数据。
///
/// 数值列与文本列的默认值**逐项不同**（规范 6.2），所以两个构造函数分开写，
/// 不靠调用方记参数。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Replicate {
    #[serde(default)]
    pub value_type: String,
    #[serde(default)]
    pub max_width: u32,
    #[serde(default)]
    pub common_attributes: Vec<String>,
    #[serde(default)]
    pub decimals_length: u32,
    #[serde(default)]
    pub significant_digits_count: u32,
    #[serde(default)]
    pub first_row: i32,
    /// 最后一行的下标。`-1` 表示这一列一个数据都没有。
    #[serde(default)]
    pub last_row: i32,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Replicate {
    /// 数值列：`real` / 55 / NUMERIC / 小数 6 / 有效位 7。
    pub fn numeric(last_row: i32) -> Self {
        Self {
            value_type: "real".to_string(),
            max_width: 55,
            common_attributes: vec!["NUMERIC".to_string()],
            decimals_length: 6,
            significant_digits_count: 7,
            first_row: 0,
            last_row,
            extra: BTreeMap::new(),
        }
    }
    /// 文本列：`text` / 200 / TEXT / 精度与有效位都是 0。
    pub fn text(last_row: i32) -> Self {
        Self {
            value_type: "text".to_string(),
            max_width: 200,
            common_attributes: vec!["TEXT".to_string()],
            decimals_length: 0,
            significant_digits_count: 0,
            first_row: 0,
            last_row,
            extra: BTreeMap::new(),
        }
    }
}

/// 全局列池里的一列。
///
/// `title` 对**文本列必须省略**（不是写成空对象），所以是 `Option` +
/// `skip_serializing_if`。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub schema: String,
    /// 与目录名 `columns/<id>.json` 中的 `id` 一致（大写 UUID v4）
    #[serde(default)]
    pub id: String,
    /// 数值列必需；文本列**必须省略**
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<RichTitle>,
    /// `"real"` | `"text"`
    #[serde(default)]
    pub value_type: String,
    /// 数值列 `["DS_ATTR_Y"]`；文本列 `["DS_ATTR_RT"]`
    #[serde(default)]
    pub attributes: Vec<String>,
    /// `"y_single"` | `"text"`
    #[serde(default)]
    pub data_format: String,
    #[serde(default)]
    pub replicates: Vec<Replicate>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Column {
    /// 数值列：`title` 必填、`DS_ATTR_Y`、`y_single`、精度 6/7。
    pub fn numeric(id: impl Into<String>, title: impl Into<String>, rows: usize) -> Self {
        Self {
            kind: "Column".to_string(),
            schema: super::SCHEMA_COLUMN.to_string(),
            id: id.into(),
            title: Some(RichTitle::new(title)),
            value_type: "real".to_string(),
            attributes: vec!["DS_ATTR_Y".to_string()],
            data_format: "y_single".to_string(),
            replicates: vec![Replicate::numeric(last_row_of(rows))],
            extra: BTreeMap::new(),
        }
    }

    /// 文本列：行标题 / 结果表内容。**无 `title`**、`DS_ATTR_RT`、`text`、精度 0/0。
    pub fn text(id: impl Into<String>, rows: usize) -> Self {
        Self {
            kind: "Column".to_string(),
            schema: super::SCHEMA_COLUMN.to_string(),
            id: id.into(),
            title: None,
            value_type: "text".to_string(),
            attributes: vec!["DS_ATTR_RT".to_string()],
            data_format: "text".to_string(),
            replicates: vec![Replicate::text(last_row_of(rows))],
            extra: BTreeMap::new(),
        }
    }

    /// 读端取值：没有 `title` 时退回 id，避免调用方到处 `unwrap`。
    pub fn title_string(&self) -> String {
        self.title.as_ref().map(|t| t.string.clone()).unwrap_or_else(|| self.id.clone())
    }
}

/// `lastRow`：空列是 `-1`，否则是 `行数 - 1`。
fn last_row_of(rows: usize) -> i32 {
    if rows == 0 {
        -1
    } else {
        rows as i32 - 1
    }
}

// ══════════════════════════════════════════════════════════════════════════
// 数据表（sheets/data/<id>/）
// ══════════════════════════════════════════════════════════════════════════

/// 字体。`family` 取用户设置的西文字体，不硬编码。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Font {
    #[serde(default)]
    pub family: String,
    #[serde(default = "ten")]
    pub size: f32,
}

fn ten() -> f32 {
    10.0
}

impl Default for Font {
    fn default() -> Self {
        Self { family: "Mi Sans".to_string(), size: 10.0 }
    }
}

/// 表格布局。
///
/// input 与 result 有三处不同（规范 7.1）：`xSubColWidth` 44→158、
/// `rowTitlesWidth` 1→454、`isCollapsed` false→true。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    #[serde(default = "f44")]
    pub title_width: f32,
    #[serde(default = "f44")]
    pub x_sub_col_width: f32,
    #[serde(default = "f1")]
    pub row_titles_width: f32,
    #[serde(default = "default_sub_col_title_type")]
    pub sub_column_title_type: String,
    /// 通常省略（各列等宽时不写）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_widths: Option<Vec<f32>>,
    #[serde(default = "dpi96")]
    pub horizontal_dpi: f32,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

fn f44() -> f32 {
    44.0
}
fn f1() -> f32 {
    1.0
}
fn dpi96() -> f32 {
    96.0
}
fn default_sub_col_title_type() -> String {
    "default".to_string()
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            title_width: 44.0,
            x_sub_col_width: 44.0,
            row_titles_width: 1.0,
            sub_column_title_type: "default".to_string(),
            column_widths: None,
            horizontal_dpi: 96.0,
            extra: BTreeMap::new(),
        }
    }
}

impl Layout {
    /// 结果表版：首列要放得下「均值 95% CI」这类长标签。
    pub fn for_result() -> Self {
        Self {
            title_width: 44.0,
            x_sub_col_width: 158.0,
            row_titles_width: 454.0,
            sub_column_title_type: "default".to_string(),
            column_widths: None,
            horizontal_dpi: 96.0,
            extra: BTreeMap::new(),
        }
    }
}

/// 容量上限。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    #[serde(default = "max_cols")]
    pub max_columns: u32,
    #[serde(default = "max_rows")]
    pub max_rows: u32,
}

fn max_cols() -> u32 {
    2048
}
fn max_rows() -> u32 {
    500_000
}

impl Default for Limits {
    fn default() -> Self {
        Self { max_columns: 2048, max_rows: 500_000 }
    }
}

/// 一个单元格坐标。
///
/// `row` **允许负值**（规范 7.1）：`-1` 是列标题行、`-2` 是分组标题行。
/// 所以用 `i32`，读端不得截断成 0。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Cell {
    #[serde(default)]
    pub row: i32,
    #[serde(default)]
    pub column: i32,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// 选区：起点 + 终点两个单元格。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellRange {
    #[serde(default)]
    pub start_cell: Cell,
    #[serde(default)]
    pub end_cell: Cell,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// 当前选择。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    #[serde(default)]
    pub anchor_row: i32,
    #[serde(default)]
    pub anchor_column: i32,
    #[serde(default)]
    pub first_sub_column: i32,
    #[serde(default)]
    pub range: CellRange,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            anchor_row: 0,
            anchor_column: 0,
            first_sub_column: 0,
            range: CellRange { start_cell: Cell::default(), end_cell: Cell::default(), extra: BTreeMap::new() },
            extra: BTreeMap::new(),
        }
    }
}

/// 结果表归属于哪个分析的哪个视图。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultOf {
    #[serde(default)]
    pub analysis_id: String,
    #[serde(default)]
    pub view_id: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// 数据表页。
///
/// `role` 只有两个取值：`"input"` 与 `"result"`。
/// 结果表在结构化回退时被跳过 —— 结果归 analyses，再建一张表就重复了。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSheet {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    /// `"input"` | `"result"`
    #[serde(default = "default_role")]
    pub role: String,
    #[serde(default)]
    pub is_custom_title: bool,
    /// 结果表默认折叠
    #[serde(default)]
    pub is_collapsed: bool,
    /// 指向 `sheets/data/<tableId>/table.json`
    #[serde(default)]
    pub table_id: String,
    /// 该表用到的列 id，顺序即 CSV 的列顺序
    #[serde(default)]
    pub columns: Vec<String>,
    /// 行标题列的 UUID；通常省略
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_titles_column: Option<String>,
    /// `["HIDE_ROW_TITLES"]`（input） / `["ANALYSIS_VIEW"]`（result）
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default)]
    pub font: Font,
    #[serde(default)]
    pub layout: Layout,
    #[serde(default)]
    pub limits: Limits,
    #[serde(default)]
    pub selection: Selection,
    /// 若本表是某个分析的产物，这里记 `{analysisId, viewId}`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_of: Option<ResultOf>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

fn default_role() -> String {
    "input".to_string()
}

/// 数据表的尺寸与版本。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataTableSpec {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub number_of_columns: u32,
    #[serde(default)]
    pub number_of_rows: u32,
    #[serde(default = "one_u32")]
    pub version: u32,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

fn one_u32() -> u32 {
    1
}

// ══════════════════════════════════════════════════════════════════════════
// 分析（sheets/analyses/<id>/）
// ══════════════════════════════════════════════════════════════════════════

/// 分析的一个结果分区。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ResultSection {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub rows: Vec<Vec<String>>,
}

/// 分析结果载荷（`parameters.json` / `results.json`）。
///
/// `content` 是自由 JSON：不同 `analysisClass` 的结构差异很大，
/// 强类型化只会让每个字段都变成 `Option`。读端按需取
/// `content.sections` / `content.summary`。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisPayload {
    /// `"AnalysisParameters"` | `"AnalysisResults"`
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub analysis_class: String,
    #[serde(default)]
    pub content: serde_json::Value,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl AnalysisPayload {
    /// 取 `content.sections`。
    pub fn sections(&self) -> Vec<ResultSection> {
        serde_json::from_value(self.content.get("sections").cloned().unwrap_or_default())
            .unwrap_or_default()
    }
    /// 取 `content.summary`（`[名称, 数值, 说明]` 三元组）。
    #[allow(dead_code)]
    pub fn summary(&self) -> Vec<(String, f64, String)> {
        let v = match self.content.get("summary") {
            Some(v) => v,
            None => return Vec::new(),
        };
        let raw: Vec<Vec<serde_json::Value>> =
            serde_json::from_value(v.clone()).unwrap_or_default();
        raw.into_iter()
            .filter_map(|r| {
                if r.len() < 3 {
                    return None;
                }
                let name = r[0].as_str().unwrap_or_default().to_string();
                let val = r[1].as_f64().unwrap_or(f64::NAN);
                let note = r[2].as_str().unwrap_or_default().to_string();
                Some((name, val, note))
            })
            .collect()
    }
}

/// 分析的一个选项卡视图（`views/<vid>.json`）。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisView {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub sections: Vec<ResultSection>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// `{ major, minor, subminor, build }`。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MinRequiredVersion {
    #[serde(default)]
    pub major: u32,
    #[serde(default)]
    pub minor: u32,
    #[serde(default)]
    pub subminor: u32,
    #[serde(default)]
    pub build: u32,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// 对另一张 sheet 的引用。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SheetRef {
    #[serde(default)]
    pub uid: String,
    #[serde(default)]
    pub title: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// 对输入数据列（dataset）的引用。`idx` 是列字母 `A`/`B`/…/`AA`。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DataSetRef {
    #[serde(default)]
    pub uid: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub idx: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// 分析页。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisSheet {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub analysis_class: String,
    #[serde(default = "yes")]
    pub is_all_data_analyzed: bool,
    #[serde(default = "yes")]
    pub is_calculated: bool,
    #[serde(default)]
    pub current_result_sheet: String,
    #[serde(default)]
    pub preferred_result_sheet: String,
    #[serde(default)]
    pub min_required_version: MinRequiredVersion,
    #[serde(default)]
    pub result_sheets: Vec<SheetRef>,
    /// 数组下标对应数据表列下标，**空列用 `None`（落盘成 `null`）占位，不能跳过**
    #[serde(default)]
    pub input_data_sets: Vec<Option<DataSetRef>>,
    #[serde(default)]
    pub input_sheets: Vec<SheetRef>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

fn yes() -> bool {
    true
}

// ══════════════════════════════════════════════════════════════════════════
// 图表
// ══════════════════════════════════════════════════════════════════════════

/// 图表页 + **完整**绘图选项。
///
/// `options` 不做任何有损映射（对比 [`crate::prismfile`] 的桥接，那里只能
/// 映射到 Prism 认识的少数几个字段）。这是 `.sprx` 保真的关键。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphSheet {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: RichTitle,
    #[serde(default)]
    pub graph_kind: String,
    /// 源数据表在运行期模型中的索引
    #[serde(default)]
    pub source_table: usize,
    #[serde(default)]
    pub options: serde_json::Value,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// `GraphSheet.options` 的强类型视图（写端用它保证字段名稳定）。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphSpec {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub graph_kind: String,
    #[serde(default)]
    pub source_table: usize,
    /// 完整绘图选项，不做有损映射
    #[serde(default)]
    pub options: serde_json::Value,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

// ══════════════════════════════════════════════════════════════════════════
// 信息表
// ══════════════════════════════════════════════════════════════════════════

/// 信息页。条目名保留中文。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoSheet {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: RichTitle,
    #[serde(default)]
    pub entries: BTreeMap<String, String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

// ══════════════════════════════════════════════════════════════════════════
// manifest
// ══════════════════════════════════════════════════════════════════════════

/// 一个 sheet 在导航栏里的属性。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SheetAttribute {
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collapsed: Option<bool>,
}

/// 四类 sheet 的 id 清单。
///
/// **没有** `rename_all`：字段名就是小写原样 `data` / `analyses` / `graphs` /
/// `infos`，而 `Manifest` 的其它字段都是 camelCase —— 这个不一致最容易写错。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SheetLists {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analyses: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graphs: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub infos: Option<Vec<String>>,
}

impl SheetLists {
    /// 写入一类清单。**空清单一律记 `None`（落盘时被 skip），绝不写 `[]`** ——
    /// 写成空数组能过 `check()`，但 `rebuild` 取不到 UUID，
    /// 表现为「打开了但什么都没有」。
    pub fn set(&mut self, kind: &str, v: Vec<String>) {
        let slot = match kind {
            "data" => &mut self.data,
            "analyses" => &mut self.analyses,
            "graphs" => &mut self.graphs,
            "infos" => &mut self.infos,
            _ => return,
        };
        if v.is_empty() {
            *slot = None;
        } else {
            *slot = Some(v);
        }
    }

    /// 全部 id（去重，按 数据 → 分析 → 图形 → 信息 的顺序）。
    pub fn all_ids(&self) -> Vec<String> {
        let mut out = Vec::new();
        for l in [&self.data, &self.analyses, &self.graphs, &self.infos] {
            if let Some(v) = l {
                for s in v {
                    if !out.contains(s) {
                        out.push(s.clone());
                    }
                }
            }
        }
        out
    }
}

/// 界面状态。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiState {
    #[serde(default)]
    pub current_sheets: BTreeMap<String, String>,
    #[serde(default = "default_tab")]
    pub active_tab: String,
    #[serde(default = "one_f32")]
    pub zoom: f32,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

fn default_tab() -> String {
    "data".to_string()
}
fn one_f32() -> f32 {
    1.0
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            current_sheets: BTreeMap::new(),
            active_tab: "data".to_string(),
            zoom: 1.0,
            extra: BTreeMap::new(),
        }
    }
}

/// 容器索引。`kind` 必须是 `SpectraProject`。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub format_version: String,
    #[serde(default)]
    pub min_reader_version: String,
    #[serde(default)]
    pub app_version: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub modified_at: String,
    /// **对象**，不是数组
    #[serde(default)]
    pub sheets: SheetLists,
    /// 必须覆盖 `sheets` 中的**每一个** id，否则导航栏标题会空白
    #[serde(default)]
    pub sheet_attributes: BTreeMap<String, SheetAttribute>,
    #[serde(default)]
    pub ui_state: UiState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            kind: "SpectraProject".to_string(),
            schema: super::SCHEMA_MANIFEST.to_string(),
            format_version: super::FORMAT_VERSION.to_string(),
            min_reader_version: super::FORMAT_VERSION.to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            created_at: String::new(),
            modified_at: String::new(),
            sheets: SheetLists::default(),
            sheet_attributes: BTreeMap::new(),
            ui_state: UiState::default(),
            theme: None,
            extra: BTreeMap::new(),
        }
    }
}

impl Manifest {
    /// 硬性校验。两条规则：
    /// 1. `kind` 必须是 `SpectraProject`
    /// 2. `formatVersion` 的 **MAJOR** 大于当前 → 拒绝
    ///    （只比 MAJOR，MINOR / PATCH 差异放行，否则小版本迭代会互相打不开）
    pub fn check(&self) -> Result<(), String> {
        if self.kind != "SpectraProject" {
            return Err(format!("不是 Spectra 工程文件（kind = {}）", self.kind));
        }
        let cur = crate::sprxfile::FORMAT_VERSION
            .split('.')
            .next()
            .and_then(|x| x.parse::<u32>().ok())
            .unwrap_or(1);
        let got = self.format_version.split('.').next().and_then(|x| x.parse::<u32>().ok());
        if let Some(g) = got {
            if g > cur {
                return Err(format!(
                    "unsupported format version {}（当前 {}）",
                    self.format_version,
                    crate::sprxfile::FORMAT_VERSION
                ));
            }
        }
        Ok(())
    }
}

// ══════════════════════════════════════════════════════════════════════════
// project.json
// ══════════════════════════════════════════════════════════════════════════

/// 作者。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Author {
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// 资源引用（图片等外部文件）。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetRef {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub media_type: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// 附件引用。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRef {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub media_type: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub size: u64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// 项目元数据。`.sprx` 是专有格式，所以扩展设置可以**直接进主结构**
/// （`.prism` 做不到，只能旁挂）。
///
/// `exportDir` 走 `extra` —— 它只有在工程设置了导出目录时才写。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMeta {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub authors: Vec<Author>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub assets: Vec<AssetRef>,
    #[serde(default)]
    pub attachments: Vec<AttachmentRef>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Default for ProjectMeta {
    fn default() -> Self {
        Self {
            kind: "ProjectMetadata".to_string(),
            schema: super::SCHEMA_PROJECT.to_string(),
            title: String::new(),
            description: String::new(),
            authors: Vec::new(),
            tags: Vec::new(),
            notes: String::new(),
            assets: Vec::new(),
            attachments: Vec::new(),
            extra: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forbidden_binary_matches_extensions() {
        assert!(is_forbidden_binary("a/data.bin"));
        assert!(is_forbidden_binary("x/y.dt"));
        assert!(!is_forbidden_binary("a/data.csv"));
        assert!(!is_forbidden_binary("manifest.json"));
    }

    /// 规范 6.2：空列的 lastRow 是 -1，有数据的是 行数-1。
    #[test]
    fn replicate_last_row_matches_spec() {
        assert_eq!(last_row_of(0), -1);
        assert_eq!(last_row_of(1), 0);
        assert_eq!(last_row_of(5), 4);
        assert_eq!(Replicate::numeric(last_row_of(0)).last_row, -1);
        assert_eq!(Replicate::text(last_row_of(3)).last_row, 2);
    }

    /// 规范 6：数值列带 title、DS_ATTR_Y、y_single；文本列**省略 title**。
    #[test]
    fn column_numeric_and_text_match_spec() {
        let n = Column::numeric("CID-1", "Control", 4);
        assert_eq!(n.kind, "Column");
        assert!(n.title.is_some());
        assert_eq!(n.value_type, "real");
        assert_eq!(n.attributes, vec!["DS_ATTR_Y".to_string()]);
        assert_eq!(n.data_format, "y_single");
        assert_eq!(n.replicates[0].common_attributes, vec!["NUMERIC".to_string()]);
        assert_eq!(n.replicates[0].max_width, 55);
        assert_eq!(n.replicates[0].decimals_length, 6);
        assert_eq!(n.replicates[0].significant_digits_count, 7);

        let t = Column::text("CID-2", 2);
        assert!(t.title.is_none());
        assert_eq!(t.value_type, "text");
        assert_eq!(t.attributes, vec!["DS_ATTR_RT".to_string()]);
        assert_eq!(t.data_format, "text");
        assert_eq!(t.replicates[0].max_width, 200);

        // 文本列的 title 必须被 skip，而不是序列化成 null
        let v = serde_json::to_value(&t).unwrap();
        assert!(v.get("title").is_none());
    }

    /// 规范 4.2：空清单不能落成 `[]`，必须被 skip。
    #[test]
    fn empty_sheet_list_is_skipped_not_empty_array() {
        let mut l = SheetLists::default();
        l.set("graphs", Vec::new());
        let v = serde_json::to_value(&l).unwrap();
        assert!(v.get("graphs").is_none());

        l.set("data", vec!["A".to_string()]);
        let v = serde_json::to_value(&l).unwrap();
        assert_eq!(v["data"][0], "A");
    }

    /// 规范 4.5：只拒绝更高的 MAJOR，MINOR 更高放行。
    #[test]
    fn manifest_accepts_newer_minor_but_rejects_newer_major() {
        let mut m = Manifest::default();
        m.format_version = "1.9.9".to_string();
        assert!(m.check().is_ok());
        m.format_version = "99.0.0".to_string();
        assert!(m.check().is_err());
        m.format_version = "1.0.0".to_string();
        m.kind = "SomethingElse".to_string();
        assert!(m.check().is_err());
    }

    /// 规范 7.1：Cell.row 允许负值，不得被截断成 0。
    #[test]
    fn cell_row_keeps_negative_values() {
        let c: Cell = serde_json::from_str(r#"{"row":-2,"column":3}"#).unwrap();
        assert_eq!(c.row, -2);
        assert_eq!(c.column, 3);
    }

    #[test]
    fn payload_sections_roundtrip() {
        let p: AnalysisPayload = serde_json::from_str(
            r#"{"analysisClass":"OneWayAnova","content":{"sections":[{"title":"ANOVA","rows":[["F","0.747"]]}]}}"#,
        )
        .unwrap();
        assert_eq!(p.sections().len(), 1);
        assert_eq!(p.sections()[0].rows[0][1], "0.747");
    }

    /// 规范 8.1：inputDataSets 里的空列落盘成 `null`，不能跳过。
    #[test]
    fn input_data_sets_keeps_null_placeholders() {
        let s = AnalysisSheet {
            kind: "AnalysisSheet".to_string(),
            schema: "s".to_string(),
            id: "A".to_string(),
            title: "t".to_string(),
            analysis_class: "OneWayAnova".to_string(),
            is_all_data_analyzed: true,
            is_calculated: true,
            current_result_sheet: "V".to_string(),
            preferred_result_sheet: "V".to_string(),
            min_required_version: MinRequiredVersion::default(),
            result_sheets: vec![SheetRef { uid: "V".to_string(), title: "ANOVA results".to_string(), extra: BTreeMap::new() }],
            input_data_sets: vec![
                Some(DataSetRef { uid: "a".to_string(), title: "Control".to_string(), idx: "A".to_string(), extra: BTreeMap::new() }),
                None,
                Some(DataSetRef { uid: "c".to_string(), title: "Treated".to_string(), idx: "C".to_string(), extra: BTreeMap::new() }),
            ],
            input_sheets: Vec::new(),
            extra: BTreeMap::new(),
        };
        let v = serde_json::to_value(&s).unwrap();
        let arr = v["inputDataSets"].as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert!(arr[1].is_null());
        assert_eq!(arr[2]["idx"], "C");
    }
}
