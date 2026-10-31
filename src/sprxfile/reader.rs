//! `.sprx` 读端。
//!
//! # 双保险
//!
//! ```text
//! is_sprx? → manifest.json → man.check()
//!    ↓
//! 优先: spectra/snapshot.json 反序列化为完整 Project  → 直接返回（无损）
//!    ↓ 缺失 / 失败
//! 结构化回退 rebuild()
//! ```
//!
//! 有快照就无损；没有也能靠 `sheet.json` / `table.json` / `data.csv` 重建出
//! 一份可用的工程。代价是回退路径拿不到分析参数，见 [`rebuild`]。

use super::model::*;
use super::{
    analysis_results_path, data_csv_path, data_sheet_path, data_table_path, graph_sheet_path,
    graph_spec_path, table_type_of, MIMETYPE, SNAPSHOT,
};
use crate::model::{CellValue, DataTable, Project, SpectraTable, TableType};

/// 一个读出来的条目。
#[derive(Clone, Debug)]
pub struct ReadEntry {
    pub name: String,
    pub data: Vec<u8>,
}

impl ReadEntry {
    /// 按 UTF-8 解成文本。
    pub fn text(&self) -> Option<&str> {
        std::str::from_utf8(&self.data).ok()
    }
    /// 按 JSON 反序列化。
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Option<T> {
        serde_json::from_slice(&self.data).ok()
    }
}

/// 是否是 `.sprx`：ZIP 魔数 + 第 0 项名为 `mimetype` + 内容匹配。
///
/// 只看开头几百字节，不解压整个文件——所以改名不影响打开。
pub fn is_sprx(bytes: &[u8]) -> bool {
    if !looks_like_zip(bytes) {
        return false;
    }
    match read_entries(bytes) {
        Ok(es) => es
            .first()
            .map(|e| {
                e.name == "mimetype"
                    && String::from_utf8_lossy(&e.data).trim() == MIMETYPE
            })
            .unwrap_or(false),
        Err(_) => false,
    }
}

fn looks_like_zip(b: &[u8]) -> bool {
    b.len() >= 4 && b[0] == 0x50 && b[1] == 0x4B && b[2] == 0x03 && b[3] == 0x04
}

/// 解出全部条目（含二进制，但**不解析**二进制内容）。
pub fn read_entries(bytes: &[u8]) -> Result<Vec<ReadEntry>, String> {
    if !looks_like_zip(bytes) {
        return Err("不是 ZIP 容器（未找到 PK 签名）".to_string());
    }
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("无法解析 ZIP：{}", e))?;
    let mut out: Vec<ReadEntry> = Vec::with_capacity(z.len());
    for i in 0..z.len() {
        // 借出的 ZipFile 必须在 z 析构前落地：
        // 直接把 match 当块尾表达式会让临时值活过 z 的生命周期。
        let item = {
            let mut f = match z.by_index(i) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let name = f.name().to_string();
            let mut data = Vec::with_capacity(f.size() as usize);
            match std::io::Read::read_to_end(&mut f, &mut data) {
                Ok(_) => Some(ReadEntry { name, data }),
                // CRC 校验失败会在这里暴露；跳过该条目而不是整体失败
                Err(_) => None,
            }
        };
        if let Some(e) = item {
            out.push(e);
        }
    }
    if out.is_empty() {
        return Err("ZIP 内没有任何可读条目".to_string());
    }
    Ok(out)
}

/// 只登记存在性的二进制条目（`.bin` / `.dt`）。
pub fn binary_entries(entries: &[ReadEntry]) -> Vec<String> {
    entries
        .iter()
        .filter(|e| is_forbidden_binary(&e.name))
        .map(|e| e.name.clone())
        .collect()
}

fn find<'a>(entries: &'a [ReadEntry], name: &str) -> Option<&'a ReadEntry> {
    entries.iter().find(|e| e.name == name)
}

/// 读 `.sprx` → [`Project`]。
pub fn read(bytes: &[u8]) -> Result<Project, String> {
    if !is_sprx(bytes) {
        return Err("不是有效的 .sprx（mimetype 不匹配）".to_string());
    }
    let entries = read_entries(bytes)?;

    // manifest 是必须的：不像 .prism 有扫描重建兜底
    let man: Manifest = find(&entries, "manifest.json")
        .and_then(|e| e.json())
        .ok_or_else(|| "manifest.json missing".to_string())?;
    man.check()?;

    // 二进制只登记，绝不解析
    let _bin = binary_entries(&entries);

    // 快照优先：有它就无损
    if let Some(e) = find(&entries, SNAPSHOT) {
        if let Some(p) = e.json::<Project>() {
            return Ok(p);
        }
    }

    rebuild(&entries, &man)
}

/// 结构化回退：从 `sheet.json` / `table.json` / `data.csv` 重建工程。
///
/// **落点一律是 Column 表**（最通用的表型）。原 XY / Grouped / Nested 的
/// 子列结构会丢失——只有走快照路径才保真。这是有意的取舍：宁可给一份
/// 结构简化但数据完整的项目，也不要打不开。
///
/// 另外结果表在这里**拿不到参数**（`params: None`），「重新计算」按钮不可用，
/// 需要手动重跑一次分析。
pub fn rebuild(entries: &[ReadEntry], man: &Manifest) -> Result<Project, String> {
    // 1) 扫 columns/*.json 建 id → title 映射
    let mut col_titles: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    for e in entries {
        if let Some(id) = e
            .name
            .strip_prefix("columns/")
            .and_then(|r| r.strip_suffix(".json"))
        {
            if let Some(c) = e.json::<Column>() {
                // 文本列没有 title（规范 6：必须省略），退回 id
                col_titles.insert(id.to_string(), c.title_string());
            }
        }
    }

    let mut p = Project {
        tables: Vec::new(),
        infos: Vec::new(),
        results: Vec::new(),
        graphs: Vec::new(),
        layouts: Vec::new(),
    };

    // 2) 按 manifest.sheets.data 逐表重建；role == "result" 跳过
    if let Some(ids) = &man.sheets.data {
        for sid in ids {
            let sheet: DataSheet = match find(entries, &data_sheet_path(sid)).and_then(|e| e.json()) {
                Some(s) => s,
                None => continue,
            };
            // 结果表归 analyses，这里建表会和结果表重复一遍
            if sheet.role == "result" {
                continue;
            }
            let spec: Option<DataTableSpec> =
                find(entries, &data_table_path(sid)).and_then(|e| e.json());

            // ★ 表类型从 `table.json` 的 extra 里读回。
            // 以前这里硬编码 TableType::Column，于是分组表 / XY 表存一次再打开
            // 就退化成列表 —— 行标题与子列结构全丢。
            let tt = spec
                .as_ref()
                .and_then(|s| s.extra.get("tableType"))
                .and_then(|v| v.as_str())
                .map(table_type_of)
                .unwrap_or(TableType::Column);
            let mut t = SpectraTable::new_by_type(tt, 0);
            t.data_sets.clear();

            let csv = match find(entries, &data_csv_path(sid)) {
                Some(e) => e.text().unwrap_or("").to_string(),
                None => String::new(),
            };
            let grid = parse_csv(&csv);

            // 列标题：优先 columns 池，其次 CSV 首行
            let heads: Vec<String> = if !sheet.columns.is_empty() {
                sheet
                    .columns
                    .iter()
                    .map(|cid| {
                        col_titles
                            .get(cid)
                            .cloned()
                            .unwrap_or_else(|| cid.clone())
                    })
                    .collect()
            } else {
                // CSV 无标题行，不能拿首行数据当标题（那会既丢一条数据、
                // 又把数字变成列名）。列池缺失时退回 A/B/C… 占位名。
                (0..grid.first().map(|r| r.len()).unwrap_or(0))
                    .map(crate::sprxfile::writer::col_letter)
                    .collect()
            };

            // 规范 7.1：`rowTitlesColumn` 通常省略，所以回退路径基本拿不到行标题
            // —— 这是有损回退的已知代价，无损路径走快照。
            let has_rt = sheet.row_titles_column.is_some();
            let start = if has_rt { 1 } else { 0 };
            // ★ 从列标题还原「数据集 + 子列」的父子结构。
            // 写入端 col_title_of 把物理列拼成 `"数据集:子列"`，而 CSV 是摊平的
            // 物理列。以前这里把每个物理列都建一个平级 data_set，于是
            // 4 个数据集 × 3 子列读回来变成 12 个数据集 —— 右边栏的统计表
            // 也就跟着变成十几列（「所有列都进来了」）。
            //
            // 只跟**上一个**数据集比较来决定是否并入，绝不跨数据集合并：
            // 物理列顺序必须和 data_sets 展开后的顺序严格一致，否则单元格错位。
            for (ci, h) in heads.iter().enumerate() {
                if has_rt && ci == 0 {
                    continue;
                }
                match h.split_once(':') {
                    Some((d, sub))
                        if !d.is_empty()
                            && t.data_sets.last().map(|x| x.name.as_str()) == Some(d) =>
                    {
                        let last = t.data_sets.len() - 1;
                        t.data_sets[last]
                            .sub_columns
                            .push(crate::model::SubColumnMeta { name: sub.to_string() });
                    }
                    _ => {
                        t.data_sets.push(crate::model::DataSetMeta {
                            name: h.clone(),
                            sub_columns: Vec::new(),
                        });
                    }
                }
            }

            // ★ 规范 7.3：`data.csv` **没有标题行**（写端 has_headers(false)），
            // 第一行就是第一条数据，列标题在 `columns/*.json` 池里。
            // 以前这里 `if ri == 0 { continue; } // 表头` 把首行跳掉了，
            // 于是保存 4 个数读回来只剩 3 个 —— 纯丢数据，不是判定问题。
            for (ri, row) in grid.iter().enumerate() {
                if has_rt {
                    if let Some(v) = row.first() {
                        if !v.is_empty() {
                            t.ensure_row_title(ri, v.clone());
                        }
                    }
                }
                for (k, cell) in row.iter().skip(start).enumerate() {
                    if cell.is_empty() {
                        continue;
                    }
                    let v = if let Ok(x) = cell.parse::<f64>() {
                        CellValue::Number(x)
                    } else {
                        CellValue::Text(cell.clone())
                    };
                    t.set_cell(ri, k, v);
                }
            }

            // 行标题：回退路径拿不到（rowTitlesColumn 通常省略），但分组表 / XY 表
            // 的行标题是结构的一部分，缺了会整列错位，所以按行号补齐占位。
            if t.table_type.has_row_titles() {
                for r in 0..grid.len().max(1) {
                    let empty = t
                        .row_titles
                        .get(r)
                        .map(|x| x.is_empty())
                        .unwrap_or(true);
                    if empty {
                        t.ensure_row_title(r, format!("{}", r + 1));
                    }
                }
            }

            p.tables.push(DataTable {
                name: sheet.title,
                table: t,
                view_rows: 20,
                excluded: Vec::new(),
            });
        }
    }

    // 3) 分析结果：results.json 的 content.sections
    if let Some(ids) = &man.sheets.analyses {
        for sid in ids {
            let sheet: AnalysisSheet =
                match find(entries, &super::analysis_sheet_path(sid)).and_then(|e| e.json()) {
                    Some(s) => s,
                    None => continue,
                };
            let payload: AnalysisPayload =
                match find(entries, &analysis_results_path(sid)).and_then(|e| e.json()) {
                    Some(x) => x,
                    None => continue,
                };
            let mut rows: Vec<Vec<String>> = Vec::new();
            for s in payload.sections() {
                rows.push(vec![format!("## {}", s.title)]);
                for r in s.rows {
                    rows.push(r);
                }
            }
            p.results.push(crate::model::ResultSheet {
                name: sheet.title,
                tabs: vec![sheet.analysis_class],
                active_tab: 0,
                rows,
                source_table: sheet.input_sheets.first().and_then(|s| {
                    // 回退路径只知道表名，按名字反查下标（查不到就 None）
                    (0..p.tables.len()).find(|i| {
                        man.sheet_attributes
                            .get(&s.uid)
                            .map(|a| a.title == p.tables[*i].name)
                            .unwrap_or(false)
                    })
                }),
            });
        }
    }

    // 4) 图表：spec.options → GraphSheet，graph_kind 解析失败回落 Bar
    if let Some(ids) = &man.sheets.graphs {
        for sid in ids {
            let sheet: GraphSheet =
                match find(entries, &graph_sheet_path(sid)).and_then(|e| e.json()) {
                    Some(s) => s,
                    None => continue,
                };
            let spec: Option<GraphSpec> =
                find(entries, &graph_spec_path(sid)).and_then(|e| e.json());
            let opts = spec.map(|s| s.options).unwrap_or(sheet.options);
            // options 里已经带了完整的 GraphSheet（含 kind），优先信它；
            // 只有 options 反序列化失败时才退回 graph_kind 字符串。
            let mut g: crate::model::GraphSheet = match serde_json::from_value(opts) {
                Ok(g) => g,
                Err(_) => {
                    let mut d = crate::model::GraphSheet::default_for(sheet.source_table);
                    d.kind = crate::chartkind::ChartKind::from_label(&sheet.graph_kind)
                        .unwrap_or(crate::chartkind::ChartKind::Bar);
                    d
                }
            };
            g.name = sheet.title.string;
            g.source_table = sheet.source_table;
            p.graphs.push(g);
        }
    }

    // 5) 信息表
    if let Some(ids) = &man.sheets.infos {
        for sid in ids {
            if let Some(s) = find(entries, &super::info_sheet_path(sid)).and_then(|e| e.json::<InfoSheet>()) {
                if let Some(v) = s.entries.values().find(|v| !v.is_empty()) {
                    p.infos.push(v.clone());
                }
            }
        }
    }

    if p.tables.is_empty() && p.results.is_empty() && p.graphs.is_empty() {
        return Err("工程文件里没有任何可重建的内容".to_string());
    }
    Ok(p)
}

/// 极简 CSV 解析：支持引号包裹与内部 `""` 转义。
///
/// 不引 csv crate：这里的输入永远是我们自己写出的文件，字段规则已知，
/// 为一个二十行的解析器拉依赖不值。
pub fn parse_csv(s: &str) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' if in_q && it.peek() == Some(&'"') => {
                cur.push('"');
                it.next();
            }
            '"' => in_q = !in_q,
            ',' if !in_q => {
                row.push(std::mem::take(&mut cur));
            }
            '\n' if !in_q => {
                if cur.ends_with('\r') {
                    cur.pop();
                }
                row.push(std::mem::take(&mut cur));
                rows.push(std::mem::take(&mut row));
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() || !row.is_empty() {
        row.push(cur);
        rows.push(row);
    }
    rows.retain(|r| !r.is_empty() && !(r.len() == 1 && r[0].is_empty()));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_roundtrip_keeps_commas_and_quotes() {
        let g = parse_csv("a,b\n\"x,y\",\"say \"\"hi\"\"\"\n");
        assert_eq!(g[0], vec!["a", "b"]);
        assert_eq!(g[1], vec!["x,y", "say \"hi\""]);
    }

    #[test]
    fn csv_keeps_empty_cells() {
        let g = parse_csv("h1,h2,h3\n1,,3\n");
        assert_eq!(g[1], vec!["1", String::new(), "3"]);
    }

    #[test]
    fn is_sprx_needs_matching_mimetype() {
        assert!(!is_sprx(b"not a zip"));
        let p = Project::blank();
        assert!(is_sprx(&crate::sprxfile::writer::write(&p)));
    }

    #[test]
    fn roundtrip_is_lossless_via_snapshot() {
        let mut p = Project::blank();
        p.tables[0].table.set_cell(0, 0, CellValue::Number(1.5));
        p.tables[0].table.set_cell(1, 2, CellValue::Number(7.25));
        let bytes = crate::sprxfile::writer::write(&p);
        let back = read(&bytes).unwrap();
        assert_eq!(back.tables[0].table.get_cell(0, 0), CellValue::Number(1.5));
        assert_eq!(back.tables[0].table.get_cell(1, 2), CellValue::Number(7.25));
    }
}
