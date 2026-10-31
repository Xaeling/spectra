//! `.prism` 读端。
//!
//! # 流程
//!
//! ```text
//! 收集全部条目名（索引缺失时要靠它重建）
//!   ↓
//! document.json
//!   ↓
//! 四类 sheet 全为空 → rebuild_index() 扫描重建
//!   ↓
//! 二进制只登记
//!   ↓
//! 数据表 / 结果表 / 图表 / 信息
//! ```
//!
//! # 版本门槛一律忽略
//!
//! `formatVersion` / `minPrismVersion` / `minFormatVersion` 只作记录，
//! 不据此拒绝打开 —— Prism 11.0.x 及其它版本的文件都按同一套结构解析。
//! 拿版本号挡人只会让「别人发来的文件打不开」变成常态。

use super::model::*;
use crate::sprxfile::reader::ReadEntry;

/// 读 `.prism` → [`crate::model::Project`]。
///
/// 快照优先由 [`super::bridge::project_from_bytes`] 处理；这里是**无快照时**
/// 的结构化回退。
#[allow(dead_code)]
pub fn read(bytes: &[u8]) -> Result<crate::model::Project, String> {
    let entries = crate::sprxfile::reader::read_entries(bytes)?;
    rebuild(&entries)
}

/// 从条目列表重建对象图，再桥接成 `Project`。
pub fn rebuild(entries: &[ReadEntry]) -> Result<crate::model::Project, String> {
    // 二进制只登记，绝不解析
    let _bin: Vec<String> = entries
        .iter()
        .filter(|e| super::is_forbidden_binary(&e.name))
        .map(|e| e.name.clone())
        .collect();

    let mut pf = PrismFile::default();

    // document.json
    if let Some(e) = entries.iter().find(|e| e.name == "document.json") {
        if let Some(d) = e.json::<Document>() {
            pf.document = d;
        }
    }
    // 四类全空 → 扫描重建
    if pf.document.sheets.is_empty() {
        pf.document.sheets = rebuild_index(entries);
        pf.document.sanitize();
    }

    // 数据表的 sheet.json
    for e in entries {
        if let Some(rest) = e.name.strip_prefix("data/sheets/") {
            if !rest.ends_with("/sheet.json") {
                continue;
            }
            let uid = rest.trim_end_matches("/sheet.json").to_string();
            if let Some(s) = e.json::<DataSheet>() {
                pf.data_sheets.push((uid, s));
            }
        }
    }
    // content.json
    for e in entries {
        if let Some(rest) = e.name.strip_prefix("data/tables/") {
            if !rest.ends_with("/content.json") {
                continue;
            }
            let uid = rest.trim_end_matches("/content.json").to_string();
            if let Some(c) = e.json::<TableContent>() {
                pf.table_contents.push((uid, c));
            }
        }
    }
    // data.csv
    for e in entries {
        if let Some(rest) = e.name.strip_prefix("data/tables/") {
            if !rest.ends_with("/data.csv") {
                continue;
            }
            let uid = rest.trim_end_matches("/data.csv").to_string();
            if let Some(t) = e.text() {
                pf.table_csv.push((uid, t.to_string()));
            }
        }
    }
    // DataSet：直接扫目录，即使某些没被 dataSets 数组引用也能取到
    for e in entries {
        if let Some(rest) = e.name.strip_prefix("data/sets/") {
            if !rest.ends_with(".json") {
                continue;
            }
            if let Some(d) = e.json::<DataSet>() {
                pf.data_sets.push(d);
            }
        }
    }
    // 分析页
    for e in entries {
        if let Some(rest) = e.name.strip_prefix("analyses/") {
            if !rest.ends_with("/sheet.json") {
                continue;
            }
            let uid = rest.trim_end_matches("/sheet.json").to_string();
            if uid.contains('/') {
                continue; // result_sheets 下的不是分析页
            }
            if let Some(a) = e.json::<AnalysisSheet>() {
                pf.analyses.push((uid, a));
            }
        }
    }
    // 结果视图：result_sheets/<vid>.json 是**扁平文件**
    for e in entries {
        if let Some(rest) = e.name.strip_suffix(".json") {
            if let Some(idx) = rest.find("/result_sheets/") {
                let vid = rest[idx + "/result_sheets/".len()..].to_string();
                if vid.contains('/') {
                    continue;
                }
                if let Some(v) = e.json::<AnalysisView>() {
                    pf.result_sheets.push((vid, v));
                }
            }
        }
    }
    // 图表 / 信息
    for e in entries {
        if let Some(rest) = e.name.strip_prefix("graphs/") {
            if !rest.ends_with("/sheet.json") {
                continue;
            }
            let uid = rest.trim_end_matches("/sheet.json").to_string();
            if let Some(g) = e.json::<FENGraphSheet>() {
                pf.graphs.push((uid, g));
            }
        }
    }
    for e in entries {
        if let Some(rest) = e.name.strip_prefix("info/") {
            if !rest.ends_with("/sheet.json") {
                continue;
            }
            let uid = rest.trim_end_matches("/sheet.json").to_string();
            if let Some(i) = e.json::<InfoSheet>() {
                pf.infos.push((uid, i));
            }
        }
    }

    // 索引登记了但实体缺失 → 保留占位，避免整表静默丢失。
    // 这里用一个空壳 DataSheet 顶上：至少导航栏里还有这一项。
    if let Some(ids) = pf.document.sheets.data.clone() {
        for id in ids {
            if !pf.data_sheets.iter().any(|(u, _)| u == &id) {
                pf.data_sheets.push((
                    id.clone(),
                    DataSheet {
                        id: None,
                        uid: id.clone(),
                        title: RichTitle::new(id.clone()),
                        table: DataTable {
                            id: None,
                            uid: format!("{}-table", id),
                            data_sets: Vec::new(),
                            table_type: None,
                            extra: Default::default(),
                        },
                        format: None,
                        extra: Default::default(),
                    },
                ));
            }
        }
    }

    if pf.data_sheets.is_empty()
        && pf.analyses.is_empty()
        && pf.graphs.is_empty()
        && pf.infos.is_empty()
    {
        return Err("文件里没有可识别的 .prism 内容".to_string());
    }

    Ok(super::bridge::from_prism(&pf))
}

/// 索引缺失时按目录前缀扫描重建。
///
/// 扫 `data/sheets/` · `analyses/` · `graphs/` · `info/` 下的 `sheet.json`，
/// 并跳过 `format == "view"` 的结果表（否则会多出一堆空表）。
pub fn rebuild_index(entries: &[ReadEntry]) -> Sheets {
    let mut s = Sheets::default();
    let mut data = Vec::new();
    let mut analyses = Vec::new();
    let mut graphs = Vec::new();
    let mut info = Vec::new();

    for e in &entries[..] {
        if let Some(rest) = e.name.strip_prefix("data/sheets/") {
            if let Some(uid) = rest.strip_suffix("/sheet.json") {
                if !uid.contains('/') {
                    data.push(uid.to_string());
                }
            }
        } else if let Some(rest) = e.name.strip_prefix("analyses/") {
            if let Some(uid) = rest.strip_suffix("/sheet.json") {
                if !uid.contains('/') {
                    analyses.push(uid.to_string());
                }
            }
        } else if let Some(rest) = e.name.strip_prefix("graphs/") {
            if let Some(uid) = rest.strip_suffix("/sheet.json") {
                if !uid.contains('/') {
                    graphs.push(uid.to_string());
                }
            }
        } else if let Some(rest) = e.name.strip_prefix("info/") {
            if let Some(uid) = rest.strip_suffix("/sheet.json") {
                if !uid.contains('/') {
                    info.push(uid.to_string());
                }
            }
        }
    }
    s.data = if data.is_empty() { None } else { Some(data) };
    s.analyses = if analyses.is_empty() { None } else { Some(analyses) };
    s.graphs = if graphs.is_empty() { None } else { Some(graphs) };
    s.info = if info.is_empty() { None } else { Some(info) };
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ent(name: &str, body: &str) -> ReadEntry {
        ReadEntry { name: name.to_string(), data: body.as_bytes().to_vec() }
    }

    #[test]
    fn index_is_rebuilt_by_scanning_prefixes() {
        let es = vec![
            ent("data/sheets/S1/sheet.json", "{}"),
            ent("data/sheets/S2/sheet.json", "{}"),
            ent("graphs/G1/sheet.json", "{}"),
            ent("info/I1/sheet.json", "{}"),
            ent("analyses/A1/result_sheets/V1.json", "{}"),
        ];
        let s = rebuild_index(&es);
        assert_eq!(s.data.unwrap(), vec!["S1", "S2"]);
        assert_eq!(s.graphs.unwrap(), vec!["G1"]);
        assert_eq!(s.info.unwrap(), vec!["I1"]);
        assert!(s.analyses.is_none(), "result_sheets 不是分析页");
    }

    #[test]
    fn result_views_are_read_as_flat_files() {
        let es = vec![ent(
            "analyses/A1/result_sheets/V1.json",
            r#"{"uid":"V1","title":{"string":"R","version":1},"sections":[{"title":"S","rows":[["a","b"]]}]}"#,
        )];
        // 直接验证扁平路径能被解析为 AnalysisView
        let v: AnalysisView = es[0].json().unwrap();
        assert_eq!(v.sections[0].rows[0], vec!["a", "b"]);
    }
}
