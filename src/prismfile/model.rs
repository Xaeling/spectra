//! `.prism` 的对象模型（按 Prism 11.0.1.90 空白模板反推）。
//!
//! # 8 条模板修正（每一条都是踩过的坑）
//!
//! 1. `DataSheet.table` 是**内嵌 DataTable 对象**，不是 UUID 字符串；且 `sheet.uid ≠ table.uid`
//! 2. 五个 `$id` 各不相同，写端原样保留
//! 3. 空白表 `content.json` 为 0 列 0 行，此时 **`data.csv` 不写**
//! 4. `InfoSheet` / `FENGraphSheet` 是独立结构；信息常量名保留中文
//! 5. `document.json.sheets` 中为空的类型**省略键**，不写空数组
//! 6. 列标题在 `data/sets/<uuid>.json` 的 `title.string`
//! 7. `DataTable.dataSets` 是 DataSet uuid 的反向索引，**数组顺序 = CSV 列顺序**
//! 8. `FENGraphSheet.inputDataSets` 引用 **DataSet uuid**
//!
//! 另有写端修正：`AnalysisView` 是 `result_sheets/` 下的**扁平文件**
//! （旧实现放在 `analyses/<uid>/result_sheets/<uuid>/sheet.json`，Prism 找不到）。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ══════════════════════════════════════════════════════════════════════════
// document.json
// ══════════════════════════════════════════════════════════════════════════

/// 索引层。四类 sheet 各自登记 uuid。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Document {
    #[serde(rename = "$id", default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, rename = "formatVersion")]
    pub format_version: Option<String>,
    #[serde(default, rename = "minFormatVersion")]
    pub min_format_version: Option<String>,
    #[serde(default, rename = "minPrismVersion")]
    pub min_prism_version: Option<String>,
    #[serde(default)]
    pub sheets: Sheets,
    #[serde(default, rename = "sheetAttributesMap")]
    pub sheet_attributes_map: BTreeMap<String, SheetAttribute>,
    #[serde(default, rename = "currentSheets")]
    pub current_sheets: BTreeMap<String, String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// 四类 sheet 的 uuid 清单。为空的类型**省略键**。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Sheets {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analyses: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graphs: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub info: Option<Vec<String>>,
}

impl Sheets {
    /// 全部 uuid，按 数据 → 分析 → 图形 → 信息。
    pub fn all_ids(&self) -> Vec<String> {
        let mut out = Vec::new();
        for l in [&self.data, &self.analyses, &self.graphs, &self.info] {
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
    /// 四类是否全为空（读端据此决定要不要扫描重建）。
    pub fn is_empty(&self) -> bool {
        self.all_ids().is_empty()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SheetAttribute {
    pub title: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub collapsed: bool,
}

impl Document {
    /// 写前清理。三件事：
    /// 1. 空类型**省略键**（Prism 见到空数组会认为结构异常）
    /// 2. `sheetAttributesMap` 补齐所有 uuid（漏一个导航栏就空白）
    /// 3. `currentSheets` 清掉悬空引用
    pub fn sanitize(&mut self) {
        if self.sheets.data.as_ref().map(|v| v.is_empty()).unwrap_or(false) {
            self.sheets.data = None;
        }
        if self.sheets.analyses.as_ref().map(|v| v.is_empty()).unwrap_or(false) {
            self.sheets.analyses = None;
        }
        if self.sheets.graphs.as_ref().map(|v| v.is_empty()).unwrap_or(false) {
            self.sheets.graphs = None;
        }
        if self.sheets.info.as_ref().map(|v| v.is_empty()).unwrap_or(false) {
            self.sheets.info = None;
        }
        for id in self.sheets.all_ids() {
            self.sheet_attributes_map.entry(id.clone()).or_insert_with(|| SheetAttribute {
                title: id.clone(),
                color: None,
                collapsed: false,
            });
        }
        let known = self.sheets.all_ids();
        self.current_sheets.retain(|_, v| known.contains(v));
    }
}

// ══════════════════════════════════════════════════════════════════════════
// 数据表
// ══════════════════════════════════════════════════════════════════════════

/// 富文本标题。与 `.sprx` 侧同构，但这里必须保留 `version` 字段。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RichTitle {
    pub string: String,
    #[serde(default = "one")]
    pub version: u32,
    #[serde(default)]
    pub formatting: Vec<serde_json::Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

fn one() -> u32 {
    1
}

impl RichTitle {
    pub fn new(s: impl Into<String>) -> Self {
        Self { string: s.into(), version: 1, formatting: Vec::new(), extra: BTreeMap::new() }
    }
}

/// 列标题所在的对象（**不是** data.csv，也不是 sheet.json）。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DataSet {
    #[serde(rename = "$id", default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub uid: String,
    pub title: RichTitle,
    #[serde(default = "num_ty")]
    #[serde(rename = "valueType")]
    pub value_type: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

fn num_ty() -> String {
    "Number".to_string()
}

/// `data/tables/<uuid>/content.json`。空白表为 0 列 0 行。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TableContent {
    #[serde(rename = "$id", default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, rename = "numberOfColumns")]
    pub number_of_columns: u32,
    #[serde(default, rename = "numberOfRows")]
    pub number_of_rows: u32,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// 内嵌在 `DataSheet` 里的表对象（**不是 uuid 字符串** —— 见模板修正 1）。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DataTable {
    #[serde(rename = "$id", default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub uid: String,
    /// 该表所有 DataSet 的 uuid 反向索引，**数组顺序 = CSV 列顺序**
    #[serde(default, rename = "dataSets")]
    pub data_sets: Vec<String>,
    #[serde(default, rename = "tableType")]
    pub table_type: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// 数据表页。`uid` 必须与其内嵌 `table.uid` **不同**（模板修正 1）。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DataSheet {
    #[serde(rename = "$id", default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub uid: String,
    pub title: RichTitle,
    /// 内嵌对象
    pub table: DataTable,
    /// 结果表与原始数据表**同目录**，靠 `format == "view"` 区分
    #[serde(default)]
    pub format: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

// ══════════════════════════════════════════════════════════════════════════
// 分析
// ══════════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnalysisSheet {
    #[serde(rename = "$id", default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub uid: String,
    pub title: RichTitle,
    #[serde(default, rename = "analysisClass")]
    pub analysis_class: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// 分析结果视图。`result_sheets/` 下的**扁平文件**（模板修正：不能是子目录）。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnalysisView {
    #[serde(rename = "$id", default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub uid: String,
    pub title: RichTitle,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub sections: Vec<ResultSection>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ResultSection {
    pub title: String,
    #[serde(default)]
    pub rows: Vec<Vec<String>>,
}

// ══════════════════════════════════════════════════════════════════════════
// 图表 / 信息
// ══════════════════════════════════════════════════════════════════════════

/// 图表页。`inputDataSets` 引用 **DataSet uuid**（模板修正 8）。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FENGraphSheet {
    #[serde(rename = "$id", default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub uid: String,
    pub title: RichTitle,
    #[serde(default, rename = "graphType")]
    pub graph_type: Option<String>,
    #[serde(default, rename = "inputDataSets")]
    pub input_data_sets: Vec<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// 信息页。常量名保留中文，不翻译。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InfoSheet {
    #[serde(rename = "$id", default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub uid: String,
    pub title: RichTitle,
    #[serde(default)]
    pub entries: BTreeMap<String, String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// 信息表默认常量名。**中文原样保留**。
pub const INFO_DEFAULTS: &[&str] = &[
    "实验日期",
    "实验 ID",
    "笔记本 ID",
    "项目",
    "实验者",
    "协议",
];

// ══════════════════════════════════════════════════════════════════════════
// 文件级容器
// ══════════════════════════════════════════════════════════════════════════

/// 一个待写的 `.prism` 文件的完整内容。
#[derive(Clone, Debug, Default)]
pub struct PrismFile {
    pub document: Document,
    pub data_sheets: Vec<(String, DataSheet)>,
    pub table_contents: Vec<(String, TableContent)>,
    pub table_csv: Vec<(String, String)>,
    pub data_sets: Vec<DataSet>,
    pub analyses: Vec<(String, AnalysisSheet)>,
    pub result_sheets: Vec<(String, AnalysisView)>,
    pub graphs: Vec<(String, FENGraphSheet)>,
    pub infos: Vec<(String, InfoSheet)>,
}

impl PrismFile {
    /// 写前自检。**任何一条硬伤都会让 Prism 判定文件损坏**，
    /// 所以宁可报错也不产出坏文件。
    ///
    /// 四类硬伤：
    /// 1. 索引登记了 uuid 但实体不存在
    /// 2. `sheetAttributesMap` 漏 uuid
    /// 3. `table.uid` 为空，或 `table.uid == sheet.uid`
    /// 4. `dataSets` 引用了不存在的 DataSet
    pub fn validate(&self) -> Result<(), String> {
        fn have<T>(v: &[(String, T)], id: &str) -> bool {
            v.iter().any(|(u, _)| u == id)
        }

        if let Some(ids) = &self.document.sheets.data {
            for id in ids {
                if !have(&self.data_sheets, id) {
                    return Err(format!("数据表 {} 已登记但实体缺失", id));
                }
            }
        }
        if let Some(ids) = &self.document.sheets.analyses {
            for id in ids {
                if !have(&self.analyses, id) {
                    return Err(format!("分析 {} 已登记但实体缺失", id));
                }
            }
        }
        if let Some(ids) = &self.document.sheets.graphs {
            for id in ids {
                if !have(&self.graphs, id) {
                    return Err(format!("图表 {} 已登记但实体缺失", id));
                }
            }
        }
        if let Some(ids) = &self.document.sheets.info {
            for id in ids {
                if !have(&self.infos, id) {
                    return Err(format!("信息表 {} 已登记但实体缺失", id));
                }
            }
        }

        for id in self.document.sheets.all_ids() {
            if !self.document.sheet_attributes_map.contains_key(&id) {
                return Err(format!("sheetAttributesMap 缺少 {}", id));
            }
        }

        let set_ids: Vec<&str> = self.data_sets.iter().map(|d| d.uid.as_str()).collect();
        for (_, s) in &self.data_sheets {
            if s.table.uid.is_empty() {
                return Err("DataTable.uid 为空".to_string());
            }
            if s.table.uid == s.uid {
                return Err(format!("DataTable.uid 与 DataSheet.uid 相同（{}）", s.uid));
            }
            for d in &s.table.data_sets {
                if !set_ids.contains(&d.as_str()) {
                    return Err(format!("dataSets 引用了不存在的 DataSet {}", d));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(uid: &str, tuid: &str, sets: Vec<String>) -> DataSheet {
        DataSheet {
            id: None,
            uid: uid.to_string(),
            title: RichTitle::new(uid),
            table: DataTable {
                id: None,
                uid: tuid.to_string(),
                data_sets: sets,
                table_type: None,
                extra: Default::default(),
            },
            format: None,
            extra: Default::default(),
        }
    }

    #[test]
    fn validate_rejects_uid_collision() {
        let mut pf = PrismFile::default();
        let mut doc = Document::default();
        doc.sheets.data = Some(vec!["S1".to_string()]);
        doc.sheet_attributes_map.insert(
            "S1".to_string(),
            SheetAttribute { title: "S1".into(), color: None, collapsed: false },
        );
        pf.document = doc;
        pf.data_sheets.push(("S1".to_string(), sheet("S1", "S1", vec![])));
        assert!(pf.validate().is_err());
    }

    #[test]
    fn validate_rejects_missing_sheet_attribute() {
        let mut pf = PrismFile::default();
        let mut doc = Document::default();
        doc.sheets.data = Some(vec!["S1".to_string()]);
        pf.document = doc;
        pf.data_sheets.push(("S1".to_string(), sheet("S1", "T1", vec![])));
        assert!(pf.validate().is_err());
    }

    #[test]
    fn validate_accepts_a_consistent_file() {
        let mut pf = PrismFile::default();
        let mut doc = Document::default();
        doc.sheets.data = Some(vec!["S1".to_string()]);
        doc.sheet_attributes_map.insert(
            "S1".to_string(),
            SheetAttribute { title: "S1".into(), color: None, collapsed: false },
        );
        pf.document = doc;
        pf.data_sets.push(DataSet {
            id: None,
            uid: "D1".to_string(),
            title: RichTitle::new("A"),
            value_type: "Number".to_string(),
            extra: Default::default(),
        });
        pf.data_sheets.push(("S1".to_string(), sheet("S1", "T1", vec!["D1".to_string()])));
        pf.table_contents.push((
            "T1".to_string(),
            TableContent { id: None, number_of_columns: 1, number_of_rows: 0, extra: Default::default() },
        ));
        assert!(pf.validate().is_ok());
    }

    #[test]
    fn sanitize_drops_empty_lists() {
        let mut doc = Document::default();
        doc.sheets.graphs = Some(Vec::new());
        doc.sanitize();
        assert!(doc.sheets.graphs.is_none());
    }
}
