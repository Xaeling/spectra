//! 工程文件与 CSV 导入导出。

use std::path::Path;

use anyhow::{Context, Result};

use crate::core::model::{Cell, ColRole, Column, DataTable, Project, TableKind};

pub const EXT: &str = "aura";

pub fn save_project(project: &Project, path: &Path) -> Result<()> {
    let json = serde_json::to_vec_pretty(project).context("序列化工程失败")?;
    let tmp = path.with_extension("aura.tmp");

    // .aura = ZIP 包（与 .prism 同格式），内含 project.json + format.txt
    let file =
        std::fs::File::create(&tmp).with_context(|| format!("创建 {} 失败", tmp.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("format.txt", opts)?;
    use std::io::Write;
    zip.write_all(b"statrau-aura-1")?;
    zip.start_file("project.json", opts)?;
    zip.write_all(&json)?;
    zip.finish().context("写入 ZIP 失败")?;

    std::fs::rename(&tmp, path).with_context(|| format!("替换 {} 失败", path.display()))?;
    Ok(())
}

pub fn load_project(path: &Path) -> Result<Project> {
    let bytes = std::fs::read(path).with_context(|| format!("读取 {} 失败", path.display()))?;

    // 兼容：旧版 .aura 是纯 JSON；新版是 ZIP（PK 头）
    let json_bytes = if bytes.starts_with(b"PK") {
        // ZIP 格式
        let cursor = std::io::Cursor::new(&bytes);
        let mut zip = zip::ZipArchive::new(cursor).context("解析 .aura (ZIP) 失败")?;
        let mut entry = zip
            .by_name("project.json")
            .context("ZIP 中缺少 project.json")?;
        let mut buf = Vec::new();
        use std::io::Read;
        entry.read_to_end(&mut buf)?;
        buf
    } else {
        // 旧版纯 JSON（向后兼容）
        bytes
    };

    let mut project: Project =
        serde_json::from_slice(&json_bytes).with_context(|| "工程文件不是合法的 StatAura 格式")?;
    if project.format > crate::core::model::FORMAT_VERSION {
        anyhow::bail!(
            "这个工程由更新版本的 StatAura 创建（format {}），当前版本只支持 {}。",
            project.format,
            crate::core::model::FORMAT_VERSION
        );
    }
    if project.next_id == 0 {
        project.next_id = project
            .tables
            .iter()
            .map(|t| t.id)
            .chain(project.graphs.iter().map(|g| g.id))
            .chain(project.analyses.iter().map(|a| a.id))
            .max()
            .unwrap_or(0)
            + 1;
    }
    Ok(project)
}

// ---------------------------------------------------------------------------
// CSV
// ---------------------------------------------------------------------------

pub fn export_csv(table: &DataTable, path: &Path) -> Result<()> {
    let mut w = csv::WriterBuilder::new().from_path(path)?;
    let mut header: Vec<String> = Vec::new();
    for (i, c) in table.columns.iter().enumerate() {
        header.push(match (c.role, i) {
            (ColRole::X, _) => c.name.clone(),
            (ColRole::Y, _) => {
                if table
                    .group_names
                    .get(c.group)
                    .map(|s| !s.is_empty())
                    .unwrap_or(false)
                    && table.kind != TableKind::Column
                {
                    format!("{}:{}", table.group_name(c.group), c.name)
                } else {
                    c.name.clone()
                }
            }
        });
    }
    w.write_record(&header)?;
    for r in 0..table.nrows() {
        let row: Vec<String> = table
            .columns
            .iter()
            .map(|c| c.cells.get(r).map(|v| v.display()).unwrap_or_default())
            .collect();
        w.write_record(&row)?;
    }
    w.flush()?;
    Ok(())
}

/// 导入 CSV。规则：
/// - 首列是递增数字且至少两列数字 → XY 表（首列为 X）
/// - 否则：首列文本作为行标题，其余数字列作为分组（Column 表）
pub fn import_csv(path: &Path, name: &str) -> Result<DataTable> {
    let mut rdr = csv::ReaderBuilder::new().flexible(true).from_path(path)?;
    let headers: Vec<String> = rdr
        .headers()?
        .iter()
        .map(|s| s.trim().to_string())
        .collect();
    let mut rows: Vec<Vec<String>> = Vec::new();
    for rec in rdr.records() {
        let rec = rec?;
        if rec.iter().all(|v| v.trim().is_empty()) {
            continue;
        }
        rows.push(rec.iter().map(|s| s.trim().to_string()).collect());
    }
    if rows.is_empty() {
        anyhow::bail!("CSV 里没有数据行");
    }
    let ncols = rows.iter().map(|r| r.len()).max().unwrap_or(headers.len());
    let numeric: Vec<bool> = (0..ncols)
        .map(|c| {
            let total = rows
                .iter()
                .filter(|r| !r.get(c).map(|s| s.trim().is_empty()).unwrap_or(true))
                .count();
            if total == 0 {
                return false;
            }
            let ok = rows
                .iter()
                .filter_map(|r| r.get(c))
                .filter(|s| !s.trim().is_empty())
                .filter(|s| s.trim().replace(',', "").parse::<f64>().is_ok())
                .count();
            ok as f64 / total as f64 >= 0.75
        })
        .collect();

    let first_col_is_x = ncols >= 2 && numeric[0] && {
        let xs: Vec<f64> = rows
            .iter()
            .filter_map(|r| r.get(0).and_then(|s| s.trim().parse::<f64>().ok()))
            .collect();
        xs.len() >= 2 && xs.windows(2).all(|w| w[1] > w[0])
    };

    let id = 0;
    let mut table = DataTable {
        id,
        name: name.to_string(),
        kind: if first_col_is_x {
            TableKind::Xy
        } else {
            TableKind::Column
        },
        columns: Vec::new(),
        group_names: Vec::new(),
        row_headers: Vec::new(),
        row_header_width: None,
        notes: format!("从 CSV 导入：{}", path.display()),
    };

    if first_col_is_x {
        let mut x = Column::new(header_name(&headers, 0, "X"), ColRole::X);
        for r in &rows {
            let v = r.get(0).and_then(|s| s.parse::<f64>().ok());
            x.cells.push(v.map(Cell::number).unwrap_or_default());
        }
        table.columns.push(x);
        for c in 1..ncols {
            if !numeric[c] {
                continue;
            }
            let g = table.group_names.len();
            table.group_names.push(header_name(&headers, c, "Y"));
            let mut col = Column::new("Y1", ColRole::Y);
            col.group = g;
            for r in &rows {
                let s = r.get(c).cloned().unwrap_or_default();
                col.cells.push(parse_cell(&s));
            }
            table.columns.push(col);
        }
    } else {
        let label_col = if !numeric[0] { Some(0) } else { None };
        for r in &rows {
            table.row_headers.push(
                label_col
                    .and_then(|c| r.get(c).cloned())
                    .unwrap_or_default(),
            );
        }
        for c in 0..ncols {
            if !numeric[c] {
                continue;
            }
            let name = header_name(&headers, c, &format!("Column {}", c + 1));
            let mut col = Column::new(name.clone(), ColRole::Y);
            col.group = table.columns.len();
            for r in &rows {
                let s = r.get(c).cloned().unwrap_or_default();
                col.cells.push(parse_cell(&s));
            }
            table.columns.push(col);
            table.group_names.push(name);
        }
    }
    if table.columns.is_empty() {
        anyhow::bail!("CSV 里没有识别到数值列");
    }
    table.ensure_rows(rows.len());
    Ok(table)
}

fn header_name(headers: &[String], i: usize, fallback: &str) -> String {
    headers
        .get(i)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn parse_cell(s: &str) -> Cell {
    let t = s.trim();
    if t.is_empty() {
        Cell::empty()
    } else if let Ok(v) = t.replace(',', "").parse::<f64>() {
        Cell::number(v)
    } else {
        Cell::label(t)
    }
}

/// 导出分析结果为 CSV。
pub fn export_result_csv(result: &crate::core::model::ResultTable, path: &Path) -> Result<()> {
    let mut w = csv::WriterBuilder::new().from_path(path)?;
    w.write_record(&result.columns)?;
    for row in &result.rows {
        w.write_record(row)?;
    }
    w.flush()?;
    Ok(())
}
