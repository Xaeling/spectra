//! `.prism` ↔ [`Project`] 的双向桥接。
//!
//! # 为什么需要旁挂快照
//!
//! `.prism` 是**有损桥接**：Prism 只认识自己的对象图，我们的绘图选项（误差棒
//! 方向、散点抖动、显著性括号、配色…）大部分映射不过去。若只写标准对象图，
//! 本应用读回来就会丢一堆设置。
//!
//! 所以写端额外附两条条目：
//! - `spectra/project.json` —— 完整 `Project` 的 pretty JSON
//! - `spectra/meta.json`   —— 快照的元信息
//!
//! Prism 侧读标准对象图，忽略这两条；本应用侧读回时优先取快照，绕开损失。
//!
//! # 读端的三条兼容
//!
//! - 版本门槛**一律忽略**：Prism 11.0.x 及其它版本都按同一套结构解析
//! - `prism-rs/project.json`（旧路径）仍然认
//! - 索引缺失时按目录前缀**扫描重建**

use super::model::*;
use crate::model::{CellValue, DataTable as RTDataTable, Project, SpectraTable, TableType};

/// `Project` → `.prism` 字节流。
#[allow(dead_code)]
pub fn project_to_bytes(p: &Project) -> Result<Vec<u8>, String> {
    let pf = to_prism(p);
    super::writer::write(&pf)
}

/// `.prism` 字节流 → `Project`。
pub fn project_from_bytes(bytes: &[u8]) -> Result<Project, String> {
    // 旁挂快照优先：有它就无损
    if let Some(p) = read_sidecar(bytes) {
        return Ok(p);
    }
    let entries = crate::sprxfile::reader::read_entries(bytes)?;
    super::reader::rebuild(&entries)
}

/// 读旁挂快照（新路径优先，旧路径兜底）。
fn read_sidecar(bytes: &[u8]) -> Option<Project> {
    let entries = crate::sprxfile::reader::read_entries(bytes).ok()?;
    for name in [super::SIDECAR_PROJECT, super::SIDECAR_PROJECT_LEGACY] {
        if let Some(e) = entries.iter().find(|e| e.name == name) {
            if let Some(p) = e.json::<Project>() {
                return Some(p);
            }
        }
    }
    None
}

// ══════════════════════════════════════════════════════════════════════════
// 写：Project → PrismFile
// ══════════════════════════════════════════════════════════════════════════

/// 把运行期 [`Project`] 映射成 Prism 的对象图。
///
/// 这一步是**有损**的：只保留 Prism 能理解的字段。无损部分交给旁挂快照。
pub fn to_prism(p: &Project) -> PrismFile {
    let mut pf = PrismFile::default();
    let mut data_ids: Vec<String> = Vec::new();
    let mut analysis_ids: Vec<String> = Vec::new();
    let mut graph_ids: Vec<String> = Vec::new();
    let mut info_ids: Vec<String> = Vec::new();

    for (ti, t) in p.tables.iter().enumerate() {
        let suid = crate::sprxfile::uuid_upper();
        let tuid = crate::sprxfile::uuid_upper();
        data_ids.push(suid.clone());

        // 每列一个 DataSet：列标题存在这里（模板修正 6）
        let mut set_uids: Vec<String> = Vec::new();
        let mut cols: Vec<(String, bool)> = Vec::new(); // (标题, 是否行标题列)
        if t.table.table_type.has_row_titles() {
            cols.push(("Row Title".to_string(), true));
        }
        for ds in &t.table.data_sets {
            if ds.sub_columns.is_empty() {
                cols.push((ds.name.clone(), false));
            } else {
                for sc in &ds.sub_columns {
                    let nm = if sc.name.is_empty() {
                        ds.name.clone()
                    } else {
                        format!("{}:{}", ds.name, sc.name)
                    };
                    cols.push((nm, false));
                }
            }
        }
        for (nm, is_rt) in cols {
            let duid = crate::sprxfile::uuid_upper();
            set_uids.push(duid.clone());
            pf.data_sets.push(DataSet {
                id: Some("ID_DATA_SET".to_string()),
                uid: duid,
                title: RichTitle::new(nm),
                value_type: if is_rt { "Text".to_string() } else { "Number".to_string() },
                extra: Default::default(),
            });
        }

        // 空白表不写 data.csv（模板修正 3）
        let csv = crate::sprxfile::writer::table_csv(p, ti).unwrap_or_default();
        let ncol = t.table.total_physical_columns() as u32;
        let nrow = t.table.filled_rows().len() as u32;
        if ncol > 0 && nrow > 0 {
            pf.table_csv.push((tuid.clone(), csv));
        }
        pf.table_contents.push((
            tuid.clone(),
            TableContent {
                id: Some("ID_TABLE_STORAGE".to_string()),
                number_of_columns: ncol,
                number_of_rows: nrow,
                extra: Default::default(),
            },
        ));

        pf.data_sheets.push((
            suid.clone(),
            DataSheet {
                id: Some("ID_DATA_SHEET".to_string()),
                uid: suid.clone(),
                title: RichTitle::new(t.name.clone()),
                table: DataTable {
                    id: Some("ID_DATA_TABLE".to_string()),
                    uid: tuid.clone(),
                    data_sets: set_uids.clone(),
                    table_type: Some(table_type_name(t.table.table_type).to_string()),
                    extra: Default::default(),
                },
                format: None,
                extra: Default::default(),
            },
        ));

        pf.document.sheet_attributes_map.insert(
            suid.clone(),
            SheetAttribute { title: t.name.clone(), color: None, collapsed: false },
        );
    }

    // ── 分析 ──────────────────────────────────────────────────────────────
    for r in &p.results {
        let suid = crate::sprxfile::uuid_upper();
        let vuid = crate::sprxfile::uuid_upper();
        analysis_ids.push(suid.clone());
        let class = r.tabs.first().cloned().unwrap_or_else(|| "Analysis".to_string());

        let mut sections = Vec::new();
        let mut cur = ResultSection::default();
        for row in &r.rows {
            let head = row.first().cloned().unwrap_or_default();
            let is_sec = head.starts_with("## ") || head.starts_with("# ");
            if is_sec {
                if !cur.rows.is_empty() || !cur.title.is_empty() {
                    sections.push(std::mem::take(&mut cur));
                }
                cur.title = head.trim_start_matches('#').trim().to_string();
            } else {
                cur.rows.push(row.clone());
            }
        }
        if !cur.rows.is_empty() || !cur.title.is_empty() {
            sections.push(cur);
        }

        pf.analyses.push((
            suid.clone(),
            AnalysisSheet {
                id: Some("ID_ANALYSIS_SHEET".to_string()),
                uid: suid.clone(),
                title: RichTitle::new(r.name.clone()),
                analysis_class: Some(class),
                extra: Default::default(),
            },
        ));
        pf.result_sheets.push((
            vuid.clone(),
            AnalysisView {
                id: Some("ID_ANALYSIS_VIEW".to_string()),
                uid: vuid,
                title: RichTitle::new(r.name.clone()),
                format: Some("view".to_string()),
                sections,
                extra: Default::default(),
            },
        ));
        pf.document.sheet_attributes_map.insert(
            suid.clone(),
            SheetAttribute { title: r.name.clone(), color: None, collapsed: false },
        );
    }

    // ── 图表：inputDataSets 引用 DataSet uuid（模板修正 8）─────────────────
    for g in &p.graphs {
        let suid = crate::sprxfile::uuid_upper();
        graph_ids.push(suid.clone());
        let inputs: Vec<String> = pf
            .data_sets
            .iter()
            .filter(|d| d.value_type == "Number")
            .take(6)
            .map(|d| d.uid.clone())
            .collect();
        pf.graphs.push((
            suid.clone(),
            FENGraphSheet {
                id: Some("ID_GRAPH_SHEET".to_string()),
                uid: suid.clone(),
                title: RichTitle::new(g.name.clone()),
                graph_type: Some(g.kind.label().to_string()),
                input_data_sets: inputs,
                extra: Default::default(),
            },
        ));
        pf.document.sheet_attributes_map.insert(
            suid.clone(),
            SheetAttribute { title: g.name.clone(), color: None, collapsed: false },
        );
    }

    // ── 信息 ──────────────────────────────────────────────────────────────
    for (i, text) in p.infos.iter().enumerate() {
        let suid = crate::sprxfile::uuid_upper();
        info_ids.push(suid.clone());
        let mut map = std::collections::BTreeMap::new();
        for (j, k) in super::model::INFO_DEFAULTS.iter().enumerate() {
            map.insert(k.to_string(), if j == 0 { text.clone() } else { String::new() });
        }
        pf.infos.push((
            suid.clone(),
            InfoSheet {
                id: Some("ID_INFO_SHEET".to_string()),
                uid: suid.clone(),
                title: RichTitle::new(format!("Info {}", i + 1)),
                entries: map,
                extra: Default::default(),
            },
        ));
        pf.document.sheet_attributes_map.insert(
            suid.clone(),
            SheetAttribute { title: format!("Info {}", i + 1), color: None, collapsed: false },
        );
    }

    pf.document.id = Some("ID_DOCUMENT".to_string());
    pf.document.sheets = Sheets {
        data: if data_ids.is_empty() { None } else { Some(data_ids) },
        analyses: if analysis_ids.is_empty() { None } else { Some(analysis_ids) },
        graphs: if graph_ids.is_empty() { None } else { Some(graph_ids) },
        info: if info_ids.is_empty() { None } else { Some(info_ids) },
    };
    pf
}

fn table_type_name(t: TableType) -> &'static str {
    match t {
        TableType::Column => "Column",
        TableType::XY => "XY",
        TableType::Grouped => "Grouped",
        TableType::Nested => "Nested",
        TableType::PartsOfWhole => "PartsOfWhole",
        TableType::MultipleVariables => "MultipleVariables",
        TableType::Contingency => "Contingency",
        TableType::Survival => "Survival",
    }
}

// ══════════════════════════════════════════════════════════════════════════
// 读：PrismFile → Project（无快照时的结构化回退）
// ══════════════════════════════════════════════════════════════════════════

/// 从 Prism 的对象图重建一份可用工程。
///
/// **落点一律是 Column 表**——Prism 的表型语义与我们的不完全对应，
/// 保守还原成最通用的表型，数据不丢。
pub fn from_prism(pf: &PrismFile) -> Project {
    let mut p = Project {
        tables: Vec::new(),
        infos: Vec::new(),
        results: Vec::new(),
        graphs: Vec::new(),
        layouts: Vec::new(),
    };

    // uuid → 列标题。直接扫 data_sets，即使某些 DataSet 没被 dataSets 数组
    // 引用也能取到。
    let mut titles: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    for d in &pf.data_sets {
        titles.insert(d.uid.clone(), d.title.string.clone());
    }

    for (suid, s) in &pf.data_sheets {
        // 结果表靠 format == "view" 区分，不在这里建表
        if s.format.as_deref() == Some("view") {
            continue;
        }
        let csv = pf
            .table_csv
            .iter()
            .find(|(u, _)| u == &s.table.uid)
            .map(|(_, c)| c.clone())
            .unwrap_or_default();
        let grid = crate::sprxfile::reader::parse_csv(&csv);

        let heads: Vec<String> = s
            .table
            .data_sets
            .iter()
            .map(|u| titles.get(u).cloned().unwrap_or_else(|| u.clone()))
            .collect();

        let mut t = SpectraTable::new_by_type(TableType::Column, 0);
        t.row_titles.clear();
        t.data_sets.clear();

        let has_rt = heads.first().map(|h| h == "Row Title").unwrap_or(false);
        for (i, h) in heads.iter().enumerate() {
            if has_rt && i == 0 {
                continue;
            }
            t.data_sets.push(crate::model::DataSetMeta {
                name: h.clone(),
                sub_columns: Vec::new(),
            });
        }
        for (ri, row) in grid.iter().enumerate() {
            if ri == 0 {
                continue;
            }
            if has_rt {
                if let Some(v) = row.first() {
                    if !v.is_empty() {
                        t.ensure_row_title(ri - 1, v.clone());
                    }
                }
            }
            for (k, cell) in row.iter().skip(if has_rt { 1 } else { 0 }).enumerate() {
                if cell.is_empty() {
                    continue;
                }
                let v = if let Ok(x) = cell.parse::<f64>() {
                    CellValue::Number(x)
                } else {
                    CellValue::Text(cell.clone())
                };
                t.set_cell(ri - 1, k, v);
            }
        }

        let title = pf
            .document
            .sheet_attributes_map
            .get(suid)
            .map(|a| a.title.clone())
            .unwrap_or_else(|| s.title.string.clone());
        p.tables.push(RTDataTable {
            name: title,
            table: t,
            view_rows: 20,
            excluded: Vec::new(),
        });
    }

    // 结果：result_sheets 是扁平文件，直接读
    for (_, v) in &pf.result_sheets {
        let mut rows: Vec<Vec<String>> = Vec::new();
        for s in &v.sections {
            rows.push(vec![format!("## {}", s.title)]);
            for r in &s.rows {
                rows.push(r.clone());
            }
        }
        p.results.push(crate::model::ResultSheet {
            name: v.title.string.clone(),
            tabs: vec!["Results".to_string()],
            active_tab: 0,
            rows,
            source_table: None,
        });
    }

    // 图表：桥接只带 graphType，其余回落默认
    for (_, g) in &pf.graphs {
        let mut sheet = crate::model::GraphSheet::default_for(0);
        sheet.name = g.title.string.clone();
        if let Some(gt) = &g.graph_type {
            if let Some(k) = crate::chartkind::ChartKind::from_label(gt) {
                sheet.kind = k;
            }
        }
        p.graphs.push(sheet);
    }

    for (_, i) in &pf.infos {
        if let Some(v) = i.entries.values().find(|v| !v.is_empty()) {
            p.infos.push(v.clone());
        }
    }

    if p.tables.is_empty() && p.results.is_empty() && p.graphs.is_empty() {
        // 什么都不该丢：给一张空表，避免打开后界面空白
        p.tables.push(RTDataTable::new_by_type("Data 1", TableType::Column, 0));
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_type_names_are_stable() {
        assert_eq!(table_type_name(TableType::XY), "XY");
        assert_eq!(table_type_name(TableType::Column), "Column");
    }

    #[test]
    fn prism_roundtrip_via_sidecar_keeps_cells() {
        let mut p = Project::blank();
        p.tables[0].table.set_cell(0, 0, CellValue::Number(1.5));
        p.tables[0].table.set_cell(1, 2, CellValue::Number(7.25));
        let bytes = project_to_bytes(&p).unwrap();
        let back = project_from_bytes(&bytes).unwrap();
        assert_eq!(back.tables[0].table.get_cell(0, 0), CellValue::Number(1.5));
        assert_eq!(back.tables[0].table.get_cell(1, 2), CellValue::Number(7.25));
    }
}
