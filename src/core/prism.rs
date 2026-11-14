//! GraphPad Prism `.prism` 文件读写。
//!
//! `.prism` 是一个 ZIP 压缩包（PK 魔数），内部结构：
//! ```text
//! document.json               文档元数据 + 各 sheet 的 UID 列表
//! data/
//!   sheets/{UUID}/sheet.json  数据表定义（标题、表格式、数据集 UID 列表）
//!   sets/{UUID}.json          数据集元数据（标题、格式 y_single/text/x…）
//!   tables/{UUID}/
//!     content.json            表维度（行×列）
//!     data.csv                数值（空串=缺失值）
//! analyses/{UUID}/            分析定义（导入时跳过）
//! graphs/{UUID}/              图形定义（导入时跳过）
//! info/{UUID}/                项目信息
//! misc/                       字体等
//! ```
//!
//! 读取：解 ZIP → document.json 取 data sheet UID → 逐 sheet 解析 → DataTable。
//! 写入：反向组装 → ZIP 打包。

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;

use crate::core::model::{Cell, ColRole, Column, DataTable, TableKind};

// ────────────────────────────────────────────────────────────────────────────
// Prism JSON 结构（只定义导入/导出需要的字段，其余忽略）
// ────────────────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize, serde::Serialize)]
struct PrismDocument {
    #[serde(rename = "formatVersion")]
    format_version: String,
    sheets: PrismSheets,
    #[serde(rename = "sheetAttributesMap", default)]
    sheet_attributes_map: HashMap<String, SheetAttributes>,
    #[serde(rename = "createdBy", default)]
    created_by: Option<PrismUser>,
    #[serde(rename = "modifiedBy", default)]
    modified_by: Option<PrismUser>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct PrismUser {
    name: String,
    user: String,
    version: String,
    platform: String,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct PrismSheets {
    #[serde(default)]
    data: Vec<String>,
    #[serde(default)]
    graphs: Vec<String>,
    #[serde(default)]
    analyses: Vec<String>,
    #[serde(default)]
    info: Vec<String>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct SheetAttributes {
    #[serde(default)]
    title: String,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct PrismDataSheet {
    #[serde(rename = "type", default = "default_sheet_class")]
    class: String,
    uid: String,
    title: String,
    table: PrismTable,
    #[serde(default)]
    flags: Vec<String>,
}

fn default_sheet_class() -> String {
    "DataSheet".into()
}

#[derive(serde::Serialize)]
struct PrismTable {
    uid: String,
    format: String,
    data_format: String,
    data_sets: Vec<String>,
    row_titles_data_set: Option<String>,
}

// serde 需要把 @class 映射到 type 字段
impl<'de> serde::Deserialize<'de> for PrismTable {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Raw {
            uid: String,
            format: String,
            #[serde(rename = "dataFormat")]
            data_format: String,
            #[serde(rename = "dataSets", default)]
            data_sets: Vec<String>,
            #[serde(rename = "rowTitlesDataSet", default)]
            row_titles_data_set: Option<String>,
        }
        let r = Raw::deserialize(deserializer)?;
        Ok(PrismTable {
            uid: r.uid,
            format: r.format,
            data_format: r.data_format,
            data_sets: r.data_sets,
            row_titles_data_set: r.row_titles_data_set,
        })
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
struct PrismDataSet {
    uid: String,
    format: String,
    title: PrismTitle,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct PrismTitle {
    string: String,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct PrismTableContent {
    #[serde(rename = "numberOfColumns")]
    number_of_columns: usize,
    #[serde(rename = "numberOfRows")]
    number_of_rows: usize,
}

// ────────────────────────────────────────────────────────────────────────────
// 读取
// ────────────────────────────────────────────────────────────────────────────

/// 从 .prism 文件读取所有数据表。
///
/// 返回 `(表名, DataTable)` 列表——Prism 的每个 data sheet 对应一张表。
/// 分析表（format="view"）跳过，只导入数据表。
pub fn read_prism(path: &Path) -> Result<Vec<(String, DataTable)>> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("打开 .prism 文件失败: {}", path.display()))?;
    let mut zip = zip::ZipArchive::new(file).context("解析 .prism (ZIP) 失败")?;

    // 1. document.json
    let doc: PrismDocument = read_json_from_zip(&mut zip, "document.json")?;

    // 2. 逐 data sheet 解析
    let mut out = Vec::new();
    for sheet_uid in &doc.sheets.data {
        let sheet_path = format!("data/sheets/{sheet_uid}/sheet.json");
        let sheet: PrismDataSheet = match read_json_from_zip(&mut zip, &sheet_path) {
            Ok(s) => s,
            Err(e) => {
                if std::env::var("PRISM_DEBUG").is_ok() {
                    eprintln!("READ sheet FAIL {sheet_path}: {e}");
                }
                continue;
            }
        };
        if std::env::var("PRISM_DEBUG").is_ok() {
            eprintln!(
                "READ sheet: uid={} title={:?} fmt={} data_sets={:?}",
                sheet.uid, sheet.title, sheet.table.format, sheet.table.data_sets
            );
        }

        // 分析结果表（view）跳过——不是用户数据
        if sheet.flags.iter().any(|f| f == "ANALYSIS_VIEW") {
            continue;
        }

        let table = parse_data_sheet(&mut zip, &sheet)?;
        out.push((sheet.title.clone(), table));
    }

    Ok(out)
}

/// 解析一个 data sheet → DataTable。
fn parse_data_sheet(
    zip: &mut zip::ZipArchive<std::fs::File>,
    sheet: &PrismDataSheet,
) -> Result<DataTable> {
    let table_ref = &sheet.table;

    // 表格式 → TableKind
    let kind = match table_ref.format.as_str() {
        "xy" | "xy_triplet" => TableKind::Xy,
        "column" => TableKind::Column,
        "grouped" => TableKind::Grouped,
        "contingency" => TableKind::Contingency,
        "survival" => TableKind::Survival,
        "partsOfWhole" | "parts_of_whole" => TableKind::PartsOfWhole,
        "multipleVariables" | "multiple_variables" => TableKind::MultipleVariables,
        "nested" => TableKind::Nested,
        _ => TableKind::Column,
    };

    // 读 data.csv
    let csv_path = format!("data/tables/{}/data.csv", table_ref.uid);
    let csv_text = read_string_from_zip(zip, &csv_path)?;
    let rows = parse_csv(&csv_text);

    // 读每个数据集的元数据
    let mut datasets = Vec::new();
    for ds_uid in &table_ref.data_sets {
        let ds_path = format!("data/sets/{ds_uid}.json");
        if let Ok(ds) = read_json_from_zip::<PrismDataSet>(zip, &ds_path) {
            if std::env::var("PRISM_DEBUG").is_ok() {
                eprintln!(
                    "READ ds: uid={} name={:?} fmt={:?}",
                    ds.uid, ds.title.string, ds.format
                );
            }
            datasets.push(ds);
        }
    }

    // 建 DataTable
    let mut dt = DataTable::template(kind, sheet.title.clone(), 0);
    // template 会预建默认列——必须清掉，否则和数据集列叠加变双倍
    dt.columns.clear();
    dt.group_names.clear();
    dt.row_headers.clear();
    let nrows = rows.len();

    // 行标题数据集（如果有）
    if let Some(rt_uid) = &table_ref.row_titles_data_set {
        let ds_path = format!("data/sets/{rt_uid}.json");
        if let Ok(_rt) = read_json_from_zip::<PrismDataSet>(zip, &ds_path) {
            // 行标题在 data.csv 的哪一列？Prism 把行标题放在单独的 data set 里，
            // 对应 data.csv 的一列。但 dataSets 列表里不含它，所以行标题数据
            // 在 csv 中的列号 = dataSets 中的位置（如果有行标题列在前面）。
            // 简化处理：如果 rowTitlesDataSet 存在，取 csv 第一列为行标题。
            // 标准 Prism 格式中行标题通常不在 dataSets 中，而是单独存。
            // 这里先把行标题列作为 row_headers，跳过它在 dataSets 中的处理。
        }
    }

    // 数据集 → 列。Prism 的 data.csv 列顺序与 dataSets 顺序一致。
    for (ci, ds) in datasets.iter().enumerate() {
        // 跳过 text 格式（行标题/分析文本，不是数值列）
        if ds.format == "text" {
            continue;
        }

        let is_x = ds.format == "x";
        let role = if is_x { ColRole::X } else { ColRole::Y };
        // 无标题的数据集给默认名（Prism 里有些数据集 title 为空）
        let col_name = if ds.title.string.is_empty() {
            format!("Y{}", ci + 1)
        } else {
            ds.title.string.clone()
        };

        let mut col = Column::new(col_name, role);
        col.group = if is_x {
            0
        } else {
            // 每个 Y 数据集独立一组（Prism column 格式）
            ci
        };

        // 从 data.csv 取第 ci 列的值
        for r in 0..nrows {
            let val = rows
                .get(r)
                .and_then(|row| row.get(ci))
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            let cell = if val.is_empty() {
                Cell::empty()
            } else if let Ok(n) = val.parse::<f64>() {
                Cell::number(n)
            } else {
                Cell::label(val)
            };
            col.cells.push(cell);
        }

        dt.columns.push(col);
    }

    // group_names = 各 Y 列的名字（Column 表的惯例）
    dt.group_names = dt
        .columns
        .iter()
        .filter(|c| c.role == ColRole::Y)
        .map(|c| c.name.clone())
        .collect();

    // 行标题：如果 rowTitlesDataSet 指向的数据集在 dataSets 之前（第 0 列），
    // 则 csv 第 0 列是行标题。Prism 标准格式中 rowTitlesDataSet 的数据
    // 在 csv 中占一列，位置在所有数据集之前。
    if table_ref.row_titles_data_set.is_some() && !rows.is_empty() {
        // 行标题列在 csv 中的位置：Prism 把行标题数据集放在 csv 的最后一列
        // （或最前，取决于版本）。这里用启发式：如果 dataSets 有 N 个数值列，
        // csv 有 N+1 列，则多出的那一列是行标题。
        let numeric_sets = datasets.iter().filter(|d| d.format != "text").count();
        if rows[0].len() > numeric_sets {
            // 行标题在最后
            let rt_col = rows[0].len() - 1;
            dt.row_headers = rows
                .iter()
                .map(|r| r.get(rt_col).cloned().unwrap_or_default())
                .collect();
        }
    }

    Ok(dt)
}

// ────────────────────────────────────────────────────────────────────────────
// 写入
// ────────────────────────────────────────────────────────────────────────────

/// 把 DataTable 写成 .prism 文件。
///
/// 每张表生成一个 data sheet + 对应的 data sets + table 存储。
pub fn write_prism(path: &Path, tables: &[(String, DataTable)]) -> Result<()> {
    let file = std::fs::File::create(path)
        .with_context(|| format!("创建 .prism 文件失败: {}", path.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // 收集 document.json 的 sheet UID
    let mut data_sheet_uids = Vec::new();
    let mut sheet_attrs = HashMap::new();

    // 临时存储要写入的文件内容
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();

    for (ti, (name, table)) in tables.iter().enumerate() {
        let sheet_uid = make_uuid(ti as u64 + 1);
        let table_uid = make_uuid(100 + ti as u64);
        data_sheet_uids.push(sheet_uid.clone());
        sheet_attrs.insert(
            sheet_uid.clone(),
            SheetAttributes {
                title: name.clone(),
            },
        );

        // data/sets/*.json
        let mut ds_uids = Vec::new();
        for (ci, col) in table.columns.iter().enumerate() {
            let ds_uid = make_uuid(1000 + ti as u64 * 100 + ci as u64);
            ds_uids.push(ds_uid.clone());
            let ds = PrismDataSet {
                uid: ds_uid.clone(),
                format: if col.role == ColRole::X {
                    "x".into()
                } else {
                    "y_single".into()
                },
                title: PrismTitle {
                    string: col.name.clone(),
                },
            };
            if std::env::var("PRISM_DEBUG").is_ok() {
                eprintln!("WRITE ds: uid={} name={:?}", ds_uid, col.name);
            }
            files.push((
                format!("data/sets/{ds_uid}.json"),
                serde_json::to_vec_pretty(&ds)?,
            ));
        }

        // data/tables/{uid}/content.json + data.csv
        let nrows = table
            .columns
            .iter()
            .map(|c| c.cells.len())
            .max()
            .unwrap_or(0);
        let ncols = table.columns.len();
        let content = PrismTableContent {
            number_of_columns: ncols,
            number_of_rows: nrows,
        };
        files.push((
            format!("data/tables/{table_uid}/content.json"),
            serde_json::to_vec_pretty(&content)?,
        ));

        // data.csv
        let mut csv = String::new();
        for r in 0..nrows {
            let row: Vec<String> = table
                .columns
                .iter()
                .map(|c| {
                    c.cells
                        .get(r)
                        .map(|cell| {
                            if cell.is_empty() {
                                String::new()
                            } else if let Some(n) = cell.num {
                                if n.fract() == 0.0 && n.abs() < 1e15 {
                                    format!("{}", n as i64)
                                } else {
                                    format!("{n}")
                                }
                            } else {
                                cell.text.clone().unwrap_or_default()
                            }
                        })
                        .unwrap_or_default()
                })
                .collect();
            csv.push_str(&row.join(","));
            csv.push('\n');
        }
        files.push((
            format!("data/tables/{table_uid}/data.csv"),
            csv.into_bytes(),
        ));

        // data/sheets/{uid}/sheet.json
        let fmt = match table.kind {
            TableKind::Xy => "xy",
            TableKind::Column => "column",
            TableKind::Grouped => "grouped",
            TableKind::Contingency => "contingency",
            TableKind::Survival => "survival",
            TableKind::PartsOfWhole => "partsOfWhole",
            TableKind::MultipleVariables => "multipleVariables",
            TableKind::Nested => "nested",
        };
        let sheet = serde_json::json!({
            "@class": "DataSheet",
            "$id": "sheets/data_sheet/data_sheet.schema.json",
            "uid": sheet_uid,
            "fenID": ti,
            "title": name,
            "isCustomTitle": true,
            "table": {
                "@class": "DataTable",
                "$id": "sheets/data_sheet/data_table/data_table.schema.json",
                "uid": table_uid,
                "format": fmt,
                "dataFormat": "y_single",
                "dataSets": ds_uids,
            },
            "font": { "family": "Times New Roman", "size": 10.0 },
            "flags": ["HIDE_ROW_TITLES"],
            "titleWidth": 44,
            "xSubColWidth": 44,
            "maxDataSets": 2048,
            "maxRow": 500000,
        });
        files.push((
            format!("data/sheets/{sheet_uid}/sheet.json"),
            serde_json::to_vec_pretty(&sheet)?,
        ));
    }

    // document.json
    let doc = PrismDocument {
        format_version: "1-6-0".into(),
        sheets: PrismSheets {
            data: data_sheet_uids,
            graphs: vec![],
            analyses: vec![],
            info: vec![],
        },
        sheet_attributes_map: sheet_attrs,
        created_by: Some(PrismUser {
            name: "StatAura".into(),
            user: "StatAura".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            platform: "Win".into(),
        }),
        modified_by: Some(PrismUser {
            name: "StatAura".into(),
            user: "StatAura".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            platform: "Win".into(),
        }),
    };
    files.push(("document.json".into(), serde_json::to_vec_pretty(&doc)?));

    // misc/used_fonts.bin（占位）
    files.push(("misc/used_fonts.bin".into(), vec![0u8; 8]));

    // 写入 ZIP
    for (name, content) in &files {
        zip.start_file(name.as_str(), opts)?;
        zip.write_all(content)?;
    }
    zip.finish()?;

    Ok(())
}

// ────────────────────────────────────────────────────────────────────────────
// 工具
// ────────────────────────────────────────────────────────────────────────────

/// 简易 CSV 解析（Prism 的 data.csv 是纯逗号分隔，无引号）。
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    text.lines()
        .map(|line| {
            line.split(',')
                .map(|s| s.trim().to_string())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// 从 ZIP 读 JSON。
fn read_json_from_zip<T: serde::de::DeserializeOwned>(
    zip: &mut zip::ZipArchive<std::fs::File>,
    path: &str,
) -> Result<T> {
    let text = read_string_from_zip(zip, path)?;
    serde_json::from_str(&text).with_context(|| format!("解析 {path} 失败"))
}

/// 从 ZIP 读字符串。
fn read_string_from_zip(zip: &mut zip::ZipArchive<std::fs::File>, path: &str) -> Result<String> {
    let mut entry = zip
        .by_name(path)
        .with_context(|| format!("ZIP 中找不到 {path}"))?;
    let mut buf = String::new();
    entry.read_to_string(&mut buf)?;
    Ok(buf)
}

/// 生成确定性 UUID（用于写入时的 UID）。
fn make_uuid(n: u64) -> String {
    format!(
        "SA{n:08X}-0000-4000-8000-{:012X}",
        n.wrapping_mul(0x9E37_79B9_7F4A_7C15)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_table() -> (String, DataTable) {
        let mut t = DataTable::template(TableKind::Column, "造模", 0);
        // template 会预建列和 group_names，必须清干净再放数据
        t.columns.clear();
        t.group_names.clear();
        t.row_headers.clear();
        for (i, name) in ["空白", "低", "中", "高"].iter().enumerate() {
            let mut c = Column::new(*name, ColRole::Y);
            c.group = i;
            c.cells = vec![
                Cell::number(22.5),
                Cell::number(21.0),
                Cell::empty(),
                Cell::number(17.5),
            ];
            t.columns.push(c);
        }
        t.group_names = t.columns.iter().map(|c| c.name.clone()).collect();
        ("造模".into(), t)
    }

    #[test]
    fn roundtrip_单表() {
        let (_name, table) = sample_table();
        eprintln!(
            "DEBUG: table has {} columns: {:?}",
            table.columns.len(),
            table
                .columns
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
        );
        let tables = vec![sample_table()];
        let dir = std::path::PathBuf::from(".");

        let path = dir.join("test.prism");

        write_prism(&path, &tables).expect("写入应成功");
        assert!(path.exists());

        let loaded = read_prism(&path).expect("读取应成功");
        assert_eq!(loaded.len(), 1);
        let (name, t) = &loaded[0];
        assert_eq!(name, "造模");
        assert_eq!(t.columns.len(), 4);
        assert_eq!(t.columns[0].name, "空白");
        assert_eq!(t.columns[3].name, "高");
        assert_eq!(t.columns[0].cells.len(), 4);
        assert_eq!(t.columns[0].cells[0].display(), "22.5");
        assert!(t.columns[0].cells[2].is_empty());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn roundtrip_多表() {
        let mut t2 = DataTable::template(TableKind::Column, "给药", 0);
        t2.columns.clear();
        let mut c = Column::new("阳性", ColRole::Y);
        c.cells = vec![Cell::number(10.0), Cell::number(20.0)];
        t2.columns.push(c);
        t2.group_names = vec!["阳性".into()];

        let tables = vec![sample_table(), ("给药".into(), t2)];
        let dir = std::path::PathBuf::from(".");

        let path = dir.join("multi.prism");

        write_prism(&path, &tables).expect("写入应成功");
        let loaded = read_prism(&path).expect("读取应成功");
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].0, "造模");
        assert_eq!(loaded[1].0, "给药");
        assert_eq!(loaded[1].1.columns.len(), 1);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn csv_解析() {
        let rows = parse_csv("1.5,2.5\n,3.5\n4,\n");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], vec!["1.5", "2.5"]);
        assert_eq!(rows[1], vec!["", "3.5"]);
        assert_eq!(rows[2], vec!["4", ""]);
    }
}

#[cfg(test)]
mod real_file_tests {
    use super::*;

    /// 用真实 .prism 文件验证导入（如果文件存在）。
    #[test]
    fn 读取真实_prism() {
        let path = std::path::Path::new(
            r"C:/Users/Qing/Desktop/英才计划/实验计划/预实验/预实验数据.prism",
        );
        if !path.exists() {
            eprintln!("真实 .prism 文件不存在，跳过");
            return;
        }
        let tables = read_prism(path).expect("读取真实 .prism 应成功");
        assert!(!tables.is_empty(), "至少有一张数据表");
        for (name, t) in &tables {
            eprintln!(
                "表「{name}」: {} 列, 组: {:?}",
                t.columns.len(),
                t.group_names
            );
            for c in &t.columns {
                eprintln!(
                    "  [{}] {} ({} 行)",
                    if c.role == ColRole::X { "X" } else { "Y" },
                    c.name,
                    c.cells.len()
                );
            }
            assert!(!t.columns.is_empty(), "表「{name}」应有列");
        }
    }
}
