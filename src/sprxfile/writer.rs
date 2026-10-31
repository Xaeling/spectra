//! `.sprx` 写端 —— 流程逐条对齐《`.sprx` 完整实现规范》第 13 章。
//!
//! 1. 每张表 → `table_grid()` 展开成网格 → 写 `data.csv` → **回读 CSV** 定
//!    `table.json` 的尺寸（不凭内存网格估算，两者必须严格一致）
//! 2. 每列进全局池 `columns/<cid>.json`（数值列 / 文本列两套默认值）
//! 3. 每张表 → `sheet.json`（`role: input`，`flags: ["HIDE_ROW_TITLES"]`）
//! 4. 每个分析 → `sheet.json` + `parameters.json` + `results.json` + `views/<vid>.json`
//!    **+ 一张 `role: result` 的数据表**（`ANALYSIS_VIEW` / 折叠 / `resultOf`）
//! 5. 图表 → `sheet.json` + `spec.json`（完整选项，无损失）
//! 6. `sheetAttributes` 补齐所有 id（缺的用 id 自身当标题，避免导航栏空白）
//! 7. `exportDir` 直接进 `project.json` 的 `extra`
//! 8. `manifest.sheets` 用 `set()` 写 —— 空清单是 `null` 不是 `[]`
//! 9. 追加 `spectra/snapshot.json`（完整 `Project` 的 pretty JSON）
//! 10. 组装：`mimetype`(Stored) → `manifest.json` → `project.json` → 其余
//! 11. `write_atomic`：`tmp → rename`，失败退回直写
//!
//! # 与 `.prism` 的分工
//!
//! [`table_csv`] 是给 `.prism` 桥接用的（**带表头行**），`.sprx` 自己的
//! [`table_grid`] + [`write_csv`] **没有表头行**。两条路径互不影响。

use super::model::*;
use super::{
    analysis_params_path, analysis_results_path, analysis_sheet_path, analysis_view_path,
    column_path, data_csv_path, data_sheet_path, data_table_path, graph_sheet_path,
    graph_spec_path, info_sheet_path, FORMAT_VERSION, MIMETYPE, SCHEMA_ANALYSIS_SHEET,
    SCHEMA_ANALYSIS_VIEW, SCHEMA_DATA_SHEET, SCHEMA_DATA_TABLE, SCHEMA_GRAPH_SHEET,
    SCHEMA_GRAPH_SPEC, SCHEMA_INFO_SHEET, SCHEMA_THEME, SNAPSHOT,
};
use crate::model::{CellValue, Project, SpectraTable, TableType};

/// 一个待写入的条目。
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub data: Vec<u8>,
}

impl Entry {
    pub fn new(name: impl Into<String>, data: Vec<u8>) -> Self {
        Self { name: name.into(), data }
    }
    /// JSON 条目。序列化失败就跳过——不能让一个坏对象毁掉整份工程。
    pub fn json<T: serde::Serialize>(name: impl Into<String>, v: &T) -> Option<Self> {
        serde_json::to_vec_pretty(v).ok().map(|d| Self::new(name, d))
    }
    pub fn text(name: impl Into<String>, s: &str) -> Self {
        Self::new(name, s.as_bytes().to_vec())
    }
}

// ══════════════════════════════════════════════════════════════════════════
// 命名工具
// ══════════════════════════════════════════════════════════════════════════

/// `analysisClass` → schema 里的 slug：非字母数字转 `_`、小写、去首尾 `_`。
pub fn slug(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            o.push(c.to_ascii_lowercase());
        } else {
            o.push('_');
        }
    }
    let t = o.trim_matches('_').to_string();
    if t.is_empty() {
        "analysis".to_string()
    } else {
        t
    }
}

/// 列下标 → 列字母：`0→A` … `25→Z` `26→AA`（26 进位，没有第 0 位）。
pub fn col_letter(mut i: usize) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (i % 26) as u8) as char);
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    s
}

// ══════════════════════════════════════════════════════════════════════════
// CSV
// ══════════════════════════════════════════════════════════════════════════

/// CSV 字段转义：含 `,` `"` `\n` `\r` 时加引号，内部引号翻倍。
pub fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// `.sprx` 的 `data.csv`：UTF-8 无 BOM、`,` 分隔、**LF** 换行、**无标题行**。
///
/// 每行先补齐成矩形再写 —— csv 写入器强制各行字段数一致，行宽不等会直接抛
/// `found record with N fields, but the previous record has M fields`。
/// 结果表各 section 列数不同（表头最窄、后续行更宽），最容易踩这个。
pub fn write_csv(grid: &[Vec<String>]) -> Option<Vec<u8>> {
    if grid.is_empty() {
        return None;
    }
    let width = grid.iter().map(|r| r.len()).max().unwrap_or(0);
    if width == 0 {
        return None;
    }
    let mut wtr = csv::WriterBuilder::new()
        .has_headers(false)
        .terminator(csv::Terminator::Any(b'\n'))
        .from_writer(Vec::new());
    for row in grid {
        let mut rec: Vec<&str> = row.iter().map(|s| s.as_str()).collect();
        while rec.len() < width {
            rec.push("");
        }
        if wtr.write_record(rec).is_err() {
            return None;
        }
    }
    wtr.into_inner().ok()
}

/// 从**写出的 CSV 字节**里回读尺寸。
///
/// 规范 7.2 要求 `table.json` 与 `data.csv` 严格一致，所以尺寸只能来自落盘后的
/// 内容，不能凭内存网格估算（两者在补齐 / 空行处理上很容易差一格）。
pub fn csv_dims(bytes: &[u8]) -> (u32, u32) {
    let mut rdr = csv::ReaderBuilder::new().has_headers(false).from_reader(bytes);
    let mut rows = 0u32;
    let mut cols = 0u32;
    for rec in rdr.records() {
        if let Ok(r) = rec {
            rows += 1;
            cols = cols.max(r.len() as u32);
        }
    }
    (cols, rows)
}

/// input 表的数值清洗：**只允许数值与空字符串**，其余一律清空。
fn clean_numeric(s: &str) -> String {
    let t = s.trim();
    if t.is_empty() || t.parse::<f64>().is_ok() {
        t.to_string()
    } else {
        String::new()
    }
}

/// 单元格 → 字符串。非有限值（NaN / Inf）写空串。
fn cell_str(v: CellValue) -> String {
    match v {
        CellValue::Empty => String::new(),
        CellValue::Number(x) => {
            if x.is_finite() {
                format!("{}", x)
            } else {
                String::new()
            }
        }
        CellValue::Text(s) => s,
    }
}

/// 取某行在某物理列区间内**第一个有限值**（Grouped / Nested 的合并规则）。
fn first_finite(t: &SpectraTable, r: usize, start: usize, width: usize) -> String {
    for c in start..start + width {
        match t.get_cell(r, c) {
            CellValue::Number(x) if x.is_finite() => return format!("{}", x),
            CellValue::Text(s) if !s.is_empty() => return s,
            _ => {}
        }
    }
    String::new()
}

/// 表类型 → CSV 网格（规范 7.4）。与 `.prism` 桥接共用同一套展开规则，
/// 保证两种格式导出的数据一致。
///
/// | 类型 | 展开 |
/// |---|---|
/// | XY | 第 0 列 = X；之后每个 dataset 的每个 sub 各占一列 |
/// | Column | 每个 group 的每个 sub 各占一列 |
/// | Grouped / Nested | 行 = 行标题，列 = dataset，单元格取区内第一个有限值 |
/// | Contingency | 行 = 频数，列 = dataset |
/// | Survival | 每个 group 两列：time、status |
/// | Parts | 单列 |
/// | Multiple | 列 = dataset；文本列优先（非空则写文本） |
pub fn table_grid(t: &SpectraTable) -> Vec<Vec<String>> {
    let nrow = t.row_titles.len().max(t.max_used_row() + 1);
    let mut grid: Vec<Vec<String>> = Vec::with_capacity(nrow);

    // 先按**列**组装，再转置成行 —— 各类型的差异全在「列怎么定」上。
    let mut cols: Vec<Vec<String>> = Vec::new();
    match t.table_type {
        TableType::XY => {
            // 第 0 列是 X；data_sets[0] 就是 X
            let mut x = Vec::with_capacity(nrow);
            for r in 0..nrow {
                x.push(cell_str(t.get_cell(r, 0)));
            }
            cols.push(x);
            for di in 1..t.data_sets.len() {
                for s in 0..t.data_sets[di].width() {
                    let start = t.dataset_start_col(di);
                    let mut c = Vec::with_capacity(nrow);
                    for r in 0..nrow {
                        c.push(cell_str(t.get_cell(r, start + s)));
                    }
                    cols.push(c);
                }
            }
        }
        TableType::Survival => {
            // 每个 group 两列：time（来自 X 列）+ status
            for di in 1..t.data_sets.len() {
                let start = t.dataset_start_col(di);
                let mut time = Vec::with_capacity(nrow);
                let mut status = Vec::with_capacity(nrow);
                for r in 0..nrow {
                    time.push(cell_str(t.get_cell(r, 0)));
                    status.push(cell_str(t.get_cell(r, start)));
                }
                cols.push(time);
                cols.push(status);
            }
        }
        TableType::Grouped | TableType::Nested | TableType::Contingency => {
            for (di, ds) in t.data_sets.iter().enumerate() {
                let start = t.dataset_start_col(di);
                let mut c = Vec::with_capacity(nrow);
                for r in 0..nrow {
                    c.push(first_finite(t, r, start, ds.width()));
                }
                cols.push(c);
            }
        }
        TableType::PartsOfWhole => {
            let mut c = Vec::with_capacity(nrow);
            for r in 0..nrow {
                c.push(cell_str(t.get_cell(r, 0)));
            }
            cols.push(c);
        }
        TableType::MultipleVariables => {
            for (di, ds) in t.data_sets.iter().enumerate() {
                let start = t.dataset_start_col(di);
                let mut c = Vec::with_capacity(nrow);
                for r in 0..nrow {
                    // 文本优先：非空文本直接写，否则写数值
                    let mut v = String::new();
                    for k in 0..ds.width() {
                        match t.get_cell(r, start + k) {
                            CellValue::Text(s) if !s.is_empty() => {
                                v = s;
                                break;
                            }
                            CellValue::Number(x) if x.is_finite() && v.is_empty() => {
                                v = format!("{}", x);
                            }
                            _ => {}
                        }
                    }
                    c.push(v);
                }
                cols.push(c);
            }
        }
        TableType::Column => {
            for (di, ds) in t.data_sets.iter().enumerate() {
                let start = t.dataset_start_col(di);
                for s in 0..ds.width() {
                    let mut c = Vec::with_capacity(nrow);
                    for r in 0..nrow {
                        c.push(cell_str(t.get_cell(r, start + s)));
                    }
                    cols.push(c);
                }
            }
        }
    }

    // 空 dataset 仍占一列（上面的 width() 已经 ≥1，这里只是兜底）
    if cols.is_empty() && !t.data_sets.is_empty() {
        cols.push(vec![String::new(); nrow]);
    }

    for r in 0..nrow {
        let mut row: Vec<String> = Vec::with_capacity(cols.len());
        for c in &cols {
            row.push(c.get(r).cloned().unwrap_or_default());
        }
        grid.push(row);
    }
    grid
}

/// 结果表网格（规范 7.6）：第 0 行是表头且首列空；每个 section 的标题独占一行；
/// 最后按最宽行**全局补齐**。
pub fn result_grid(sections: &[ResultSection]) -> Vec<Vec<String>> {
    let mut body: Vec<Vec<String>> = Vec::new();
    for s in sections {
        if !s.title.is_empty() {
            body.push(vec![s.title.clone()]);
        }
        for r in &s.rows {
            body.push(r.clone());
        }
    }
    if body.is_empty() {
        return Vec::new();
    }
    // 第 0 行 = ["", 首个数据行的其余列]
    let mut head: Vec<String> = vec![String::new()];
    if let Some(first) = body.first() {
        for c in first.iter().skip(1) {
            head.push(c.clone());
        }
    }
    let mut grid: Vec<Vec<String>> = vec![head];
    grid.extend(body);

    let width = grid.iter().map(|r| r.len()).max().unwrap_or(0);
    for r in grid.iter_mut() {
        while r.len() < width {
            r.push(String::new());
        }
    }
    grid
}

// ══════════════════════════════════════════════════════════════════════════
// 组装
// ══════════════════════════════════════════════════════════════════════════

struct Built {
    manifest: Manifest,
    project: ProjectMeta,
    entries: Vec<Entry>,
}

/// 从运行期 [`Project`] 组装出全部条目。
fn build(p: &Project) -> Built {
    let mut data_ids: Vec<String> = Vec::new();
    let mut analysis_ids: Vec<String> = Vec::new();
    let mut graph_ids: Vec<String> = Vec::new();
    let mut info_ids: Vec<String> = Vec::new();
    let mut attrs: std::collections::BTreeMap<String, SheetAttribute> =
        std::collections::BTreeMap::new();
    let mut entries: Vec<Entry> = Vec::new();

    let now = super::now_iso();
    // 规范 7.1：`font.family` 取用户设置的西文字体，不是硬编码。
    // 设置里的 `font_family` 是下标：0 = 界面字体（Mi Sans），其余走 Times。
    let latin = crate::settings::with(|s| {
        if s.font_family == 0 {
            crate::fonts::UI_FAMILY.to_string()
        } else {
            crate::fonts::LATIN_FAMILY.to_string()
        }
    });

    // 每张表的数据 sheet id 与列 id（分析结果要引用它们）
    let mut table_sheet_ids: Vec<String> = Vec::new();
    let mut table_col_ids: Vec<Vec<String>> = Vec::new();
    let mut table_col_titles: Vec<Vec<String>> = Vec::new();

    // ── 数据表 ────────────────────────────────────────────────────────────
    for t in &p.tables {
        let sid = super::uuid_upper();
        // 规范 7.1：`tableId` = 本 sheet 的 id，三者同目录
        let tid = sid.clone();
        data_ids.push(sid.clone());
        table_sheet_ids.push(sid.clone());
        attrs.insert(
            sid.clone(),
            SheetAttribute { title: t.name.clone(), color: None, collapsed: Some(false) },
        );

        // 1) 网格 → CSV → 回读尺寸
        let mut grid = table_grid(&t.table);
        for row in grid.iter_mut() {
            for c in row.iter_mut() {
                *c = clean_numeric(c);
            }
        }
        let csv_bytes = write_csv(&grid);
        let (ncols, nrows) = match &csv_bytes {
            Some(b) => csv_dims(b),
            None => (0, 0),
        };

        // 2) 每一列进全局池。列 id 的顺序即 CSV 的列顺序。
        let mut col_ids: Vec<String> = Vec::new();
        let mut col_titles: Vec<String> = Vec::new();
        for ci in 0..ncols as usize {
            let cid = super::uuid_upper();
            let title = col_title_of(&t.table, ci);
            col_titles.push(title.clone());
            if let Some(e) = Entry::json(column_path(&cid), &Column::numeric(cid.clone(), title, nrows as usize)) {
                entries.push(e);
            }
            col_ids.push(cid);
        }
        table_col_ids.push(col_ids.clone());
        table_col_titles.push(col_titles);

        // 3) sheet.json（role = input，flags = HIDE_ROW_TITLES）
        let sheet = DataSheet {
            kind: "DataSheet".to_string(),
            schema: SCHEMA_DATA_SHEET.to_string(),
            id: sid.clone(),
            title: t.name.clone(),
            role: "input".to_string(),
            is_custom_title: false,
            is_collapsed: false,
            table_id: tid.clone(),
            columns: col_ids,
            row_titles_column: None,
            flags: vec!["HIDE_ROW_TITLES".to_string()],
            font: Font { family: latin.clone(), size: 10.0 },
            layout: Layout::default(),
            limits: Limits::default(),
            selection: Selection::default(),
            result_of: None,
            extra: Default::default(),
        };
        if let Some(e) = Entry::json(data_sheet_path(&sid), &sheet) {
            entries.push(e);
        }

        // 4) table.json —— 尺寸来自落盘后的 CSV
        // extra 里挂一份表类型：规范没给 DataTableSpec 这个字段，但它有
        // flatten extra，塞进去既不破坏规范，又能让结构化回退恢复真实表类型。
        let mut spec_extra: std::collections::BTreeMap<String, serde_json::Value> =
            std::collections::BTreeMap::new();
        spec_extra.insert(
            "tableType".to_string(),
            serde_json::Value::String(super::table_type_key(t.table.table_type).to_string()),
        );
        let spec = DataTableSpec {
            kind: "DataTable".to_string(),
            schema: SCHEMA_DATA_TABLE.to_string(),
            number_of_columns: ncols,
            number_of_rows: nrows,
            version: 1,
            extra: spec_extra,
        };
        if let Some(e) = Entry::json(data_table_path(&tid), &spec) {
            entries.push(e);
        }

        // 5) data.csv（全空表不写）
        if let Some(b) = csv_bytes {
            entries.push(Entry::new(data_csv_path(&tid), b));
        }
    }

    // ── 分析结果 ──────────────────────────────────────────────────────────
    for r in &p.results {
        let aid = super::uuid_upper();
        let vid = super::uuid_upper();
        let rsid = super::uuid_upper();
        analysis_ids.push(aid.clone());
        attrs.insert(
            aid.clone(),
            SheetAttribute { title: r.name.clone(), color: None, collapsed: Some(false) },
        );

        let mut sections = Vec::new();
        // 结果行以 `# ` / `## ` 开头的标记行切段（见 report.rs 的 flatten）
        let mut cur = ResultSection { title: String::new(), rows: Vec::new() };
        for row in &r.rows {
            let head = row.first().map(|s| s.as_str()).unwrap_or("");
            if let Some(rest) = head.strip_prefix("## ").or_else(|| head.strip_prefix("# ")) {
                if !cur.rows.is_empty() || !cur.title.is_empty() {
                    sections.push(std::mem::take(&mut cur));
                }
                cur.title = rest.to_string();
            } else {
                cur.rows.push(row.clone());
            }
        }
        if !cur.rows.is_empty() || !cur.title.is_empty() {
            sections.push(cur);
        }

        let class = r.tabs.first().cloned().unwrap_or_else(|| "Analysis".to_string());
        let sl = slug(&class);

        // 输入引用：source_table 指到哪张表就引用哪张的列；空列用 null 占位
        let mut input_sets: Vec<Option<DataSetRef>> = Vec::new();
        let mut input_sheets: Vec<SheetRef> = Vec::new();
        if let Some(ti) = r.source_table {
            if let Some(t) = p.tables.get(ti) {
                input_sheets.push(SheetRef {
                    uid: table_sheet_ids[ti].clone(),
                    title: t.name.clone(),
                    extra: Default::default(),
                });
                let ids = &table_col_ids[ti];
                let titles = &table_col_titles[ti];
                let mut grid = table_grid(&t.table);
                for row in grid.iter_mut() {
                    for c in row.iter_mut() {
                        *c = clean_numeric(c);
                    }
                }
                let nonempty: Vec<bool> = (0..ids.len())
                    .map(|ci| grid.iter().any(|row| row.get(ci).map(|s| !s.is_empty()).unwrap_or(false)))
                    .collect();
                for (ci, uuid) in ids.iter().enumerate() {
                    if nonempty.get(ci).copied().unwrap_or(false) {
                        input_sets.push(Some(DataSetRef {
                            uid: uuid.clone(),
                            title: titles.get(ci).cloned().unwrap_or_default(),
                            idx: col_letter(ci),
                            extra: Default::default(),
                        }));
                    } else {
                        // ★ 空列必须落 `null` 占位，跳过会让后面的列整体错位
                        input_sets.push(None);
                    }
                }
            }
        }

        let sheet = AnalysisSheet {
            kind: "AnalysisSheet".to_string(),
            schema: SCHEMA_ANALYSIS_SHEET.to_string(),
            id: aid.clone(),
            title: r.name.clone(),
            analysis_class: class.clone(),
            is_all_data_analyzed: true,
            is_calculated: true,
            current_result_sheet: vid.clone(),
            preferred_result_sheet: vid.clone(),
            min_required_version: MinRequiredVersion::default(),
            result_sheets: vec![SheetRef {
                uid: vid.clone(),
                title: "ANOVA results".to_string(),
                extra: Default::default(),
            }],
            input_data_sets: input_sets,
            input_sheets: input_sheets,
            extra: Default::default(),
        };
        if let Some(e) = Entry::json(analysis_sheet_path(&aid), &sheet) {
            entries.push(e);
        }

        let table_name = r
            .source_table
            .and_then(|ti| p.tables.get(ti))
            .map(|t| t.name.clone())
            .unwrap_or_default();
        let params = AnalysisPayload {
            kind: "AnalysisParameters".to_string(),
            schema: format!("{}/analysis/{}/parameters/v1.json", super::SCHEMA_BASE, sl),
            version: "0.0.1".to_string(),
            analysis_class: class.clone(),
            content: serde_json::json!({ "table": table_name }),
            extra: Default::default(),
        };
        if let Some(e) = Entry::json(analysis_params_path(&aid), &params) {
            entries.push(e);
        }

        let results = AnalysisPayload {
            kind: "AnalysisResults".to_string(),
            schema: format!("{}/analysis/{}/results/v1.json", super::SCHEMA_BASE, sl),
            version: "0.0.0".to_string(),
            analysis_class: class.clone(),
            content: sections_content(&sections),
            extra: Default::default(),
        };
        if let Some(e) = Entry::json(analysis_results_path(&aid), &results) {
            entries.push(e);
        }

        let view = AnalysisView {
            kind: "AnalysisView".to_string(),
            schema: SCHEMA_ANALYSIS_VIEW.to_string(),
            id: vid.clone(),
            title: r.name.clone(),
            sections: sections.clone(),
            extra: Default::default(),
        };
        if let Some(e) = Entry::json(analysis_view_path(&aid, &vid), &view) {
            entries.push(e);
        }

        // 结果数据表：role = result / ANALYSIS_VIEW / 折叠 / resultOf
        let rgrid = result_grid(&sections);
        let rcsv = write_csv(&rgrid);
        let (rcols, rrows) = match &rcsv {
            Some(b) => csv_dims(b),
            None => (0, 0),
        };
        let mut rcol_ids: Vec<String> = Vec::new();
        for _ in 0..rcols {
            let cid = super::uuid_upper();
            if let Some(e) =
                Entry::json(column_path(&cid), &Column::text(cid.clone(), rrows as usize))
            {
                entries.push(e);
            }
            rcol_ids.push(cid);
        }
        let rsheet = DataSheet {
            kind: "DataSheet".to_string(),
            schema: SCHEMA_DATA_SHEET.to_string(),
            id: rsid.clone(),
            title: r.name.clone(),
            role: "result".to_string(),
            is_custom_title: false,
            is_collapsed: true,
            table_id: rsid.clone(),
            columns: rcol_ids,
            row_titles_column: None,
            flags: vec!["ANALYSIS_VIEW".to_string()],
            font: Font { family: latin.clone(), size: 10.0 },
            layout: Layout::for_result(),
            limits: Limits::default(),
            selection: Selection::default(),
            result_of: Some(ResultOf {
                analysis_id: aid.clone(),
                view_id: vid.clone(),
                extra: Default::default(),
            }),
            extra: Default::default(),
        };
        if let Some(e) = Entry::json(data_sheet_path(&rsid), &rsheet) {
            entries.push(e);
        }
        let rspec = DataTableSpec {
            kind: "DataTable".to_string(),
            schema: SCHEMA_DATA_TABLE.to_string(),
            number_of_columns: rcols,
            number_of_rows: rrows,
            version: 1,
            extra: Default::default(),
        };
        if let Some(e) = Entry::json(data_table_path(&rsid), &rspec) {
            entries.push(e);
        }
        if let Some(b) = rcsv {
            entries.push(Entry::new(data_csv_path(&rsid), b));
        }
        // 结果表也在 sheets.data 里注册（读端靠 role 区分，不会重复建表）
        data_ids.push(rsid.clone());
        attrs.insert(
            rsid,
            SheetAttribute { title: r.name.clone(), color: None, collapsed: Some(true) },
        );
    }

    // ── 图表 ──────────────────────────────────────────────────────────────
    for g in &p.graphs {
        let sid = super::uuid_upper();
        graph_ids.push(sid.clone());
        attrs.insert(
            sid.clone(),
            SheetAttribute { title: g.name.clone(), color: None, collapsed: Some(false) },
        );
        // 完整选项：不做任何有损映射
        let options = serde_json::to_value(g).unwrap_or_else(|_| serde_json::json!({}));
        let sheet = GraphSheet {
            kind: "GraphSheet".to_string(),
            schema: SCHEMA_GRAPH_SHEET.to_string(),
            id: sid.clone(),
            title: RichTitle::new(g.name.clone()),
            graph_kind: g.kind.label().to_string(),
            source_table: g.source_table,
            options: options.clone(),
            extra: Default::default(),
        };
        if let Some(e) = Entry::json(graph_sheet_path(&sid), &sheet) {
            entries.push(e);
        }
        let spec = GraphSpec {
            kind: "GraphSpec".to_string(),
            schema: SCHEMA_GRAPH_SPEC.to_string(),
            graph_kind: sheet.graph_kind.clone(),
            source_table: g.source_table,
            options,
            extra: Default::default(),
        };
        if let Some(e) = Entry::json(graph_spec_path(&sid), &spec) {
            entries.push(e);
        }
    }

    // ── 信息表 ────────────────────────────────────────────────────────────
    for (i, text) in p.infos.iter().enumerate() {
        let sid = super::uuid_upper();
        info_ids.push(sid.clone());
        attrs.insert(
            sid.clone(),
            SheetAttribute { title: format!("Info {}", i + 1), color: None, collapsed: Some(false) },
        );
        let mut map = std::collections::BTreeMap::new();
        for (j, k) in super::INFO_DEFAULTS.iter().enumerate() {
            map.insert(k.to_string(), if j == 0 { text.clone() } else { String::new() });
        }
        let sheet = InfoSheet {
            kind: "InfoSheet".to_string(),
            schema: SCHEMA_INFO_SHEET.to_string(),
            id: sid.clone(),
            title: RichTitle::new(format!("Info {}", i + 1)),
            entries: map,
            extra: Default::default(),
        };
        if let Some(e) = Entry::json(info_sheet_path(&sid), &sheet) {
            entries.push(e);
        }
    }

    // ── sheetAttributes 必须覆盖所有 id ───────────────────────────────────
    let mut lists = SheetLists::default();
    lists.set("data", data_ids);
    lists.set("analyses", analysis_ids);
    lists.set("graphs", graph_ids);
    lists.set("infos", info_ids);
    for id in lists.all_ids() {
        attrs.entry(id.clone()).or_insert_with(|| SheetAttribute {
            title: id.clone(),
            color: None,
            collapsed: Some(false),
        });
    }

    let mut extra = std::collections::BTreeMap::new();
    extra.insert(
        "spectraSnapshot".to_string(),
        serde_json::Value::String(SNAPSHOT.to_string()),
    );

    let mut ui_state = UiState::default();
    if let Some(v) = lists.data.as_ref().and_then(|v| v.first()) {
        ui_state.current_sheets.insert("data".to_string(), v.clone());
    }
    if let Some(v) = lists.analyses.as_ref().and_then(|v| v.first()) {
        ui_state.current_sheets.insert("analyses".to_string(), v.clone());
    }

    let manifest = Manifest {
        kind: "SpectraProject".to_string(),
        schema: super::SCHEMA_MANIFEST.to_string(),
        format_version: FORMAT_VERSION.to_string(),
        min_reader_version: min_reader_version(FORMAT_VERSION),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: now.clone(),
        modified_at: now.clone(),
        sheets: lists,
        sheet_attributes: attrs,
        ui_state,
        theme: Some("styles/theme.json".to_string()),
        extra,
    };

    // 专有格式：扩展设置直接进主结构（.prism 做不到，只能旁挂）
    let mut pextra = std::collections::BTreeMap::new();
    pextra.insert(
        "exportDir".to_string(),
        serde_json::Value::String(crate::settings::with(|s| s.save_dir.clone())),
    );
    let project = ProjectMeta {
        kind: "ProjectMetadata".to_string(),
        schema: super::SCHEMA_PROJECT.to_string(),
        title: p
            .tables
            .first()
            .map(|t| t.name.clone())
            .unwrap_or_else(|| "Untitled".to_string()),
        description: String::new(),
        authors: match crate::settings::with(|s| s.author.clone()) {
            a if a.is_empty() => Vec::new(),
            a => vec![Author { name: a, email: None, extra: Default::default() }],
        },
        tags: Vec::new(),
        notes: String::new(),
        assets: Vec::new(),
        attachments: Vec::new(),
        extra: pextra,
    };

    Built { manifest, project, entries }
}

/// `results.json` 的 content：sections + summary。
///
/// 用显式 `to_value` 而不是 `json!` 宏 —— 后者对非字面量表达式走的是借用，
/// 语义不直观，这里要的就是「拷一份进 Value」。
fn sections_content(sections: &[ResultSection]) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    m.insert(
        "sections".to_string(),
        serde_json::to_value(sections).unwrap_or(serde_json::Value::Null),
    );
    m.insert("summary".to_string(), serde_json::Value::Array(Vec::new()));
    serde_json::Value::Object(m)
}

/// CSV 第 `ci` 列的列标题。
fn col_title_of(t: &SpectraTable, ci: usize) -> String {
    let (ds, sub) = t.col_label(ci);
    if sub.is_empty() {
        ds
    } else {
        format!("{}:{}", ds, sub)
    }
}

/// `minReaderVersion` = formatVersion 的 `MAJOR.MINOR.0`。
fn min_reader_version(v: &str) -> String {
    let mut it = v.split('.');
    let major = it.next().unwrap_or("1");
    let minor = it.next().unwrap_or("0");
    format!("{}.{}.0", major, minor)
}

/// 序列化为 `.sprx` 字节流。
pub fn write(p: &Project) -> Vec<u8> {
    let b = build(p);

    let mut out: Vec<Entry> = Vec::new();
    // 1) mimetype 必须第一项且 Stored
    out.push(Entry::text("mimetype", MIMETYPE));
    // 2) manifest / 3) project
    if let Some(e) = Entry::json("manifest.json", &b.manifest) {
        out.push(e);
    }
    if let Some(e) = Entry::json("project.json", &b.project) {
        out.push(e);
    }
    // 主题
    if let Some(e) = Entry::json(
        "styles/theme.json",
        &serde_json::json!({
            "schema": SCHEMA_THEME,
            "dark": crate::settings::with(|s| s.is_dark()),
            "accent": crate::settings::with(|s| crate::settings::ACCENTS[s.accent.min(crate::settings::ACCENTS.len()-1)]),
        }),
    ) {
        out.push(e);
    }
    // 4) 其余条目（跳过只登记不解析的二进制）
    for e in b.entries {
        if is_forbidden_binary(&e.name) {
            continue;
        }
        out.push(e);
    }
    // 5) 快照 —— 有它才无损
    if let Ok(s) = serde_json::to_string_pretty(p) {
        out.push(Entry::text(SNAPSHOT, &s));
    }

    assemble(&out)
}

/// 组装 ZIP：`mimetype` 用 Stored（读端只读开头即可判定），其余 Deflated。
fn assemble(entries: &[Entry]) -> Vec<u8> {
    use std::io::Write;
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        let deflated = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for e in entries {
            let opts = if e.name == "mimetype" { stored } else { deflated };
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

/// 原子写盘：`tmp → rename`，失败删 tmp 并退回直写。
pub fn write_atomic(path: &str, data: &[u8]) -> std::io::Result<()> {
    let tmp = format!("{}.tmp", path);
    match std::fs::write(&tmp, data).and_then(|_| std::fs::rename(&tmp, path)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            std::fs::write(path, data).map_err(|_| e)
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════
// .prism 桥接用的旧接口（**带表头行**，与 .sprx 的 data.csv 不是同一个东西）
// ══════════════════════════════════════════════════════════════════════════

/// 把一张表导出为标准 CSV：首行列标题，其后逐行单元格。
///
/// 空单元格写空串而不是跳过——跳过会让后面的列整体左移，读回来就错位了。
pub fn table_csv(p: &Project, ti: usize) -> Option<String> {
    let t = p.tables.get(ti)?;
    let mut o = String::new();

    let mut heads: Vec<String> = Vec::new();
    if t.table.table_type.has_row_titles() {
        heads.push("Row Title".to_string());
    }
    for ds in &t.table.data_sets {
        if ds.sub_columns.is_empty() {
            heads.push(ds.name.clone());
        } else {
            for s in &ds.sub_columns {
                if s.name.is_empty() {
                    heads.push(ds.name.clone());
                } else {
                    heads.push(format!("{}:{}", ds.name, s.name));
                }
            }
        }
    }
    o.push_str(&heads.iter().map(|h| csv_field(h)).collect::<Vec<_>>().join(","));
    o.push('\n');

    let ncol = t.table.total_physical_columns().max(1);
    for r in t.table.filled_rows() {
        let row: std::collections::BTreeMap<usize, CellValue> =
            t.table.row_cells(r).into_iter().collect();
        let mut cells: Vec<String> = Vec::new();
        if t.table.table_type.has_row_titles() {
            cells.push(t.table.row_titles.get(r).cloned().unwrap_or_default());
        }
        for c in 0..ncol {
            let s = match row.get(&c) {
                Some(CellValue::Number(x)) => format!("{}", x),
                Some(CellValue::Text(s)) => s.clone(),
                Some(CellValue::Empty) | None => String::new(),
            };
            cells.push(s);
        }
        o.push_str(&cells.iter().map(|x| csv_field(x)).collect::<Vec<_>>().join(","));
        o.push('\n');
    }
    Some(o)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Project {
        let mut p = Project::blank();
        p.tables[0].table.set_cell(0, 0, CellValue::Number(1.5));
        p.tables[0].table.set_cell(1, 0, CellValue::Number(2.5));
        p.tables[0].table.set_cell(0, 1, CellValue::Number(9.0));
        p
    }

    #[test]
    fn mimetype_is_first_and_stored() {
        let bytes = write(&sample());
        assert!(bytes.starts_with(b"PK\x03\x04"));
        // 本地文件头里能直接读到 "mimetype" 这个名字
        let name = String::from_utf8_lossy(&bytes[30..38]).to_string();
        assert_eq!(name, "mimetype");
    }

    #[test]
    fn min_reader_version_keeps_major_minor_only() {
        assert_eq!(min_reader_version("1.0.0"), "1.0.0");
        assert_eq!(min_reader_version("2.3.7"), "2.3.0");
    }

    #[test]
    fn csv_escapes_and_keeps_empty_cells() {
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("plain"), "plain");
    }

    #[test]
    fn forbidden_binary_is_dropped_on_write() {
        let mut es = vec![
            Entry::text("mimetype", MIMETYPE),
            Entry::text("junk/data.bin", "xxxx"),
        ];
        es.retain(|e| !is_forbidden_binary(&e.name));
        assert_eq!(es.len(), 1);
    }

    /// 规范 8.2：非字母数字转 `_`、小写、去首尾 `_`、空串回落 analysis。
    #[test]
    fn slug_matches_spec() {
        assert_eq!(slug("OneWayAnova"), "onewayanova");
        assert_eq!(slug("One-way ANOVA!"), "one_way_anova");
        assert_eq!(slug("__"), "analysis");
    }

    /// 规范 8.1：列字母支持 26 进位。
    #[test]
    fn col_letter_is_base26_without_zero_digit() {
        assert_eq!(col_letter(0), "A");
        assert_eq!(col_letter(25), "Z");
        assert_eq!(col_letter(26), "AA");
        assert_eq!(col_letter(27), "AB");
        assert_eq!(col_letter(51), "AZ");
        assert_eq!(col_letter(52), "BA");
    }

    /// 规范 7.3：LF 换行、无标题行、矩形、空行计入行数。
    #[test]
    fn csv_has_no_header_uses_lf_and_is_rectangular() {
        let g = vec![
            vec!["1".to_string(), "2".to_string()],
            vec!["3".to_string()],
            vec![String::new(), String::new()],
        ];
        let b = write_csv(&g).unwrap();
        let s = String::from_utf8(b.clone()).unwrap();
        assert!(!s.contains('\r'));
        assert_eq!(s, "1,2\n3,\n,\n");
        assert_eq!(csv_dims(&b), (2, 3));
    }

    /// 规范 7.5：input 表的非数值一律清空。
    #[test]
    fn clean_numeric_drops_non_numbers() {
        assert_eq!(clean_numeric(" 1.5 "), "1.5");
        assert_eq!(clean_numeric("abc"), "");
        assert_eq!(clean_numeric(""), "");
    }

    /// 规范 7.6：第 0 行首列空、section 标题独占一行、尾部补齐。
    #[test]
    fn result_grid_matches_spec() {
        let secs = vec![ResultSection {
            title: "方差分析表".to_string(),
            rows: vec![vec!["F".to_string(), "0.747".to_string()]],
        }];
        let g = result_grid(&secs);
        assert_eq!(g[0][0], "");
        assert_eq!(g[1], vec!["方差分析表".to_string(), String::new()]);
        assert_eq!(g[2], vec!["F".to_string(), "0.747".to_string()]);
        assert!(g.iter().all(|r| r.len() == g[0].len()));
    }
}
