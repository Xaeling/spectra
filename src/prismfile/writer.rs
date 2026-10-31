//! `.prism` 写端。
//!
//! # 流程
//!
//! ```text
//! pf.validate()          ← 有任何硬伤直接拒绝写盘
//!   ↓
//! 版本三元组（模板优先 → 沿用 document → 常量）
//!   ↓
//! doc.sanitize()         ← 空类型省略键 / attributesMap 全覆盖 / currentSheets 清悬空
//!   ↓
//! put("document.json")   ← 第一项
//!   ↓
//! 收集 referenced 集合  ← 只有被引用的表落盘，避免孤儿目录
//!   ↓
//! data/sheets/ → data/sets/ → analyses/ → result_sheets/ → graphs/ → info/
//!   ↓
//! 旁挂快照（spectra/project.json + spectra/meta.json）
//! ```
//!
//! **写端原则**：任何一条硬伤都会让 Prism 判定文件损坏，宁可报错也不产出坏文件。

use super::model::*;
use std::collections::HashSet;
use std::io::Write;

/// 一个待写入的条目。
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub data: Vec<u8>,
}

impl Entry {
    pub fn json<T: serde::Serialize>(name: impl Into<String>, v: &T) -> Option<Self> {
        serde_json::to_vec_pretty(v).ok().map(|d| Self { name: name.into(), data: d })
    }
    pub fn text(name: impl Into<String>, s: &str) -> Self {
        Self { name: name.into(), data: s.as_bytes().to_vec() }
    }
}

/// 序列化为 `.prism` 字节流（不含旁挂快照）。
#[allow(dead_code)]
pub fn write(pf: &PrismFile) -> Result<Vec<u8>, String> {
    Ok(assemble(&write_impl(pf)?))
}

/// 组装标准条目。
fn write_impl(pf: &PrismFile) -> Result<Vec<Entry>, String> {
    let mut pf = pf.clone();

    // 1) 硬伤自检：宁可报错也不产出坏文件
    pf.validate()?;

    // 2) 版本三元组：模板优先，其次沿用已有值，最后才是常量
    let tpl = super::template_versions();
    if let Some(v) = &tpl {
        pf.document.format_version = Some(v.format_version.clone());
        pf.document.min_format_version = Some(v.min_format_version.clone());
        pf.document.min_prism_version = Some(v.min_prism_version.clone());
    } else {
        if pf.document.format_version.is_none() {
            pf.document.format_version = Some(super::TPL_FORMAT_VERSION.to_string());
        }
        if pf.document.min_format_version.is_none() {
            pf.document.min_format_version = Some(super::TPL_MIN_FORMAT_VERSION.to_string());
        }
        if pf.document.min_prism_version.is_none() {
            pf.document.min_prism_version = Some(super::TPL_MIN_PRISM_VERSION.to_string());
        }
    }

    // 3) 清理后再校验一次：sanitize 会补齐 attributesMap
    pf.document.sanitize();
    pf.validate()?;

    // 4) 只落盘被引用的表，避免孤儿目录
    let mut referenced: HashSet<String> = HashSet::new();
    for id in pf.document.sheets.all_ids() {
        referenced.insert(id);
    }

    let mut out: Vec<Entry> = Vec::new();
    // document.json 必须是第一项
    if let Some(e) = Entry::json("document.json", &pf.document) {
        out.push(e);
    }

    // data/sheets/<uuid>/sheet.json
    for (uid, s) in &pf.data_sheets {
        if !referenced.contains(uid) {
            continue;
        }
        if let Some(e) = Entry::json(format!("data/sheets/{}/sheet.json", uid), s) {
            out.push(e);
        }
    }
    // data/tables/<uuid>/content.json（空白表不写 csv）
    for (uid, c) in &pf.table_contents {
        if let Some(e) = Entry::json(format!("data/tables/{}/content.json", uid), c) {
            out.push(e);
        }
    }
    for (uid, csv) in &pf.table_csv {
        if csv.trim().is_empty() {
            continue;
        }
        out.push(Entry::text(format!("data/tables/{}/data.csv", uid), csv));
    }
    // data/sets/<uuid>.json —— 列标题在这里
    for d in &pf.data_sets {
        if let Some(e) = Entry::json(format!("data/sets/{}.json", d.uid), d) {
            out.push(e);
        }
    }
    // analyses/<uuid>/sheet.json
    for (uid, a) in &pf.analyses {
        if !referenced.contains(uid) {
            continue;
        }
        if let Some(e) = Entry::json(format!("analyses/{}/sheet.json", uid), a) {
            out.push(e);
        }
    }
    // analyses/<uuid>/result_sheets/<vid>.json —— 扁平文件，不是子目录
    for (vid, v) in &pf.result_sheets {
        let parent = pf
            .analyses
            .first()
            .map(|(u, _)| u.clone())
            .unwrap_or_else(|| "analysis".to_string());
        if !referenced.contains(&parent) {
            continue;
        }
        if let Some(e) =
            Entry::json(format!("analyses/{}/result_sheets/{}.json", parent, vid), v)
        {
            out.push(e);
        }
    }
    for (uid, g) in &pf.graphs {
        if !referenced.contains(uid) {
            continue;
        }
        if let Some(e) = Entry::json(format!("graphs/{}/sheet.json", uid), g) {
            out.push(e);
        }
    }
    for (uid, i) in &pf.infos {
        if !referenced.contains(uid) {
            continue;
        }
        if let Some(e) = Entry::json(format!("info/{}/sheet.json", uid), i) {
            out.push(e);
        }
    }

    Ok(out)
}

/// 同上，并附旁挂快照。
///
/// 快照必须是**原始** [`Project`]，不能是「对象图 → Project」反推出来的那份
/// —— 反推经过一次有损桥接，拿它当快照等于把损失固化下来，往返照样丢东西。
pub fn write_with_snapshot(pf: &PrismFile, project: &crate::model::Project) -> Result<Vec<u8>, String> {
    let mut out = write_entries(pf)?;
    out.extend(sidecar_entries(project));
    Ok(assemble(&out))
}

/// 旁挂快照的两条条目。
fn sidecar_entries(project: &crate::model::Project) -> Vec<Entry> {
    let mut out = Vec::new();
    if let Ok(s) = serde_json::to_string_pretty(project) {
        out.push(Entry::text(super::SIDECAR_PROJECT, &s));
    }
    let meta = serde_json::json!({
        "app": "spectra",
        "schema": "spectra/project",
        "version": 1,
        "savedAt": crate::sprxfile::now_iso(),
    });
    out.push(Entry::text(super::SIDECAR_META, &meta.to_string()));
    out
}

/// 组装全部标准条目（不含快照）。
fn write_entries(pf: &PrismFile) -> Result<Vec<Entry>, String> {
    write_impl(pf)
}

/// 组装 ZIP：全部 Deflated（`document.json` 第一项即可）。
fn assemble(entries: &[Entry]) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for e in entries {
            if super::is_forbidden_binary(&e.name) {
                continue;
            }
            if w.start_file(e.name.clone(), opts).is_err() {
                continue;
            }
            if w.write_all(&e.data).is_err() {
                continue;
            }
        }
        let _ = w.finish();
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal() -> PrismFile {
        let mut pf = PrismFile::default();
        let mut doc = Document::default();
        doc.sheets.data = Some(vec!["S1".to_string()]);
        doc.sheet_attributes_map.insert(
            "S1".to_string(),
            SheetAttribute { title: "Data 1".into(), color: None, collapsed: false },
        );
        pf.document = doc;
        pf.data_sets.push(DataSet {
            id: None,
            uid: "D1".to_string(),
            title: RichTitle::new("A"),
            value_type: "Number".to_string(),
            extra: Default::default(),
        });
        pf.data_sheets.push((
            "S1".to_string(),
            DataSheet {
                id: None,
                uid: "S1".to_string(),
                title: RichTitle::new("Data 1"),
                table: DataTable {
                    id: None,
                    uid: "T1".to_string(),
                    data_sets: vec!["D1".to_string()],
                    table_type: Some("Column".to_string()),
                    extra: Default::default(),
                },
                format: None,
                extra: Default::default(),
            },
        ));
        pf.table_contents.push((
            "T1".to_string(),
            TableContent { id: None, number_of_columns: 1, number_of_rows: 1, extra: Default::default() },
        ));
        pf.table_csv.push(("T1".to_string(), "A\n1.0\n".to_string()));
        pf
    }

    #[test]
    fn document_json_is_first_entry() {
        let bytes = write(&minimal()).unwrap();
        let name = String::from_utf8_lossy(&bytes[30..45]).to_string();
        assert!(name.starts_with("document"));
    }

    #[test]
    fn invalid_file_is_refused() {
        let mut pf = minimal();
        pf.data_sheets.clear(); // 登记了 S1 但没有实体
        assert!(write(&pf).is_err());
    }

    #[test]
    fn sidecar_is_appended() {
        let bytes = write(&minimal()).unwrap();
        let es = crate::sprxfile::reader::read_entries(&bytes).unwrap();
        assert!(es.iter().any(|e| e.name == super::super::SIDECAR_PROJECT));
        assert!(es.iter().any(|e| e.name == super::super::SIDECAR_META));
    }
}
