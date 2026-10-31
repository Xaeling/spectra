//! `.sprx` / `.prism` 双格式工程文件。
//!
//! # 双格式策略
//!
//! | | `.sprx` | `.prism` |
//! |---|---|---|
//! | 定位 | 本应用**专有**工程格式 | 对齐 Prism「ZIP + CSV + JSON」组织方式的**交换**格式 |
//! | 索引 | `manifest.json` | `document.json` |
//! | 往返 | 无损 | 无损（靠旁挂快照） |
//! | 判定 | mimetype 内容 | ZIP + document.json |
//!
//! ## 关于 `.prism` 的兼容性边界（必须说清楚）
//!
//! GraphPad 没有公开 `.prism` 的完整 schema，我们也没有 Prism 可用来验证。
//! 所以这里的做法是：
//! - **组织结构对齐** Prism 的已知形态（ZIP 容器 + CSV 数据表 + JSON 索引）
//! - 数据表导出为**标准 CSV**，Excel / R / Python 都能直接读
//! - 往返无损靠**旁挂快照**（`spectra/snapshot.txt`）
//!
//! 但**不保证 Prism 本身能打开**——那是需要真机验证的事，做不到就不该声称做到。
//!
//! # 两种格式都用的「快照优先」机制
//!
//! 两种容器内都放一份 `save_text()` 的完整快照。读端优先取它，
//! 因此无论容器结构怎么变，往返都不会丢字段。

use crate::model::{CellValue, Project};
use crate::zipstore::{self, Entry};

// ══════════════════════════════════════════════════════════════════════
// 常量
// ══════════════════════════════════════════════════════════════════════

pub const MIMETYPE: &str = "application/vnd.spectra.project+zip";
pub const FORMAT_VERSION: &str = "1.0.0";
/// 旁挂快照条目名（两个格式共用）。
pub const SNAPSHOT: &str = "spectra/snapshot.txt";

/// `.prism` 侧的旁挂目录前缀。
const SIDECAR: &str = "spectra/";

// ══════════════════════════════════════════════════════════════════════
// 工具
// ══════════════════════════════════════════════════════════════════════

/// CSV 字段转义：含 `,` `"` `\n` `\r` 时加引号并转义内部引号。
fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// 极简 JSON 字符串转义（manifest / document 都是我们自己拼的短 JSON）。
fn json_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o
}

/// 把一张表导出为标准 CSV：首行列标题，其后逐行单元格。
fn table_csv(p: &Project, ti: usize) -> Option<String> {
    let t = p.tables.get(ti)?;
    let mut o = String::new();

    // 表头：行标题列 + 各物理列
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

    // 数据行：`data` 是私有字段，走公开的访问器遍历
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

// ══════════════════════════════════════════════════════════════════════
// 写端
// ══════════════════════════════════════════════════════════════════════

/// 格式判定：靠**后缀**（保存时后缀是用户显式选择，不该被内容覆盖）。
pub fn is_sprx_container(path: &str) -> bool {
    path.to_lowercase().ends_with(".sprx")
}

/// 打开时的格式判定：靠**内容**（改名不影响打开）。
pub fn detect(bytes: &[u8]) -> Format {
    if is_sprx_bytes(bytes) {
        Format::Sprx
    } else if looks_like_zip(bytes) {
        Format::Prism
    } else {
        Format::Plain
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Format {
    Sprx,
    Prism,
    /// 旧版纯文本 .spectra
    Plain,
}

/// ZIP 魔数 `PK\x03\x04`。
fn looks_like_zip(b: &[u8]) -> bool {
    b.len() >= 4 && b[0] == 0x50 && b[1] == 0x4B && b[2] == 0x03 && b[3] == 0x04
}

/// `.sprx` 靠 mimetype 判定：解开第一个条目看内容。
pub fn is_sprx_bytes(bytes: &[u8]) -> bool {
    if !looks_like_zip(bytes) {
        return false;
    }
    match zipstore::read(bytes) {
        Ok(es) => es
            .first()
            .map(|e| e.name == "mimetype" && String::from_utf8_lossy(&e.data).trim() == MIMETYPE)
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// 写 `.sprx`：mimetype 必须是**第一项**（读端才能只读开头判定格式）。
pub fn write_sprx(p: &Project) -> Vec<u8> {
    let mut es = Vec::new();

    // 1) mimetype —— 必须第一项
    es.push(Entry::text("mimetype", MIMETYPE));

    // 2) manifest.json —— 索引与版本
    let sheets: Vec<String> = (0..p.tables.len()).map(|i| format!("\"t{}\"", i)).collect();
    let manifest = format!(
        concat!(
            "{{\n",
            "  \"kind\": \"SpectraProject\",\n",
            "  \"schema\": \"https://spectra.dev/schema/manifest/v1.json\",\n",
            "  \"formatVersion\": \"{}\",\n",
            "  \"appVersion\": \"0.1.0\",\n",
            "  \"sheets\": {{\"data\": [{}], \"analyses\": [], \"graphs\": [], \"infos\": []}},\n",
            "  \"tableCount\": {},\n",
            "  \"graphCount\": {},\n",
            "  \"resultCount\": {}\n",
            "}}\n"
        ),
        FORMAT_VERSION,
        sheets.join(", "),
        p.tables.len(),
        p.graphs.len(),
        p.results.len()
    );
    es.push(Entry::text("manifest.json", &manifest));

    // 3) 每张表的标准 CSV（便于 Excel / R / Python 直接读）
    for i in 0..p.tables.len() {
        if let Some(csv) = table_csv(p, i) {
            es.push(Entry::text(format!("sheets/data/t{}/data.csv", i), &csv));
        }
    }

    // 4) 快照 —— 无损往返的保证
    es.push(Entry::text(SNAPSHOT, &p.save_text()));

    zipstore::write(&es)
}

/// 写 `.prism`：ZIP + document.json 索引 + CSV 数据表 + 旁挂快照。
pub fn write_prism(p: &Project) -> Vec<u8> {
    let mut es = Vec::new();

    let mut sheet_ids: Vec<String> = Vec::new();
    for i in 0..p.tables.len() {
        sheet_ids.push(format!("\"sheet-{}\"", i));
    }

    let doc = format!(
        concat!(
            "{{\n",
            "  \"format\": \"prism\",\n",
            "  \"formatVersion\": \"1-6-0\",\n",
            "  \"sheets\": {{\"data\": [{}]}},\n",
            "  \"tableCount\": {},\n",
            "  \"graphCount\": {}\n",
            "}}\n"
        ),
        sheet_ids.join(", "),
        p.tables.len(),
        p.graphs.len()
    );
    es.push(Entry::text("document.json", &doc));

    for i in 0..p.tables.len() {
        if let Some(csv) = table_csv(p, i) {
            es.push(Entry::text(format!("data/sheets/sheet-{}/data.csv", i), &csv));
        }
    }

    // 旁挂快照：保证「本应用 → .prism → 本应用」无损
    es.push(Entry::text(SNAPSHOT, &p.save_text()));
    es.push(Entry::text(
        format!("{}meta.json", SIDECAR),
        &format!(
            "{{\"app\":\"spectra\",\"schema\":\"spectra/project\",\"version\":1,\"kind\":\"{}\"}}",
            "prism"
        ),
    ));

    zipstore::write(&es)
}

/// 按后缀分派。
pub fn write_by_ext(p: &Project, path: &str) -> Vec<u8> {
    if is_sprx_container(path) {
        write_sprx(p)
    } else {
        write_prism(p)
    }
}

// ══════════════════════════════════════════════════════════════════════
// 读端
// ══════════════════════════════════════════════════════════════════════

/// 解析一个工程文件（三种格式自动判定）。
pub fn read_any(bytes: &[u8]) -> Result<Project, String> {
    match detect(bytes) {
        // 旧版纯文本
        Format::Plain => {
            let s = String::from_utf8_lossy(bytes);
            Project::load_text(&s).ok_or_else(|| "无法解析旧版 .spectra 文本".to_string())
        }
        Format::Sprx => read_sprx(bytes),
        Format::Prism => read_prism(bytes),
    }
}

/// 读 `.sprx`：先校验 mimetype 与版本，再优先取快照。
pub fn read_sprx(bytes: &[u8]) -> Result<Project, String> {
    if !is_sprx_bytes(bytes) {
        return Err("不是有效的 .sprx（mimetype 不匹配）".to_string());
    }
    let es = zipstore::read(bytes)?;

    // 版本门槛：只比 MAJOR，MINOR/PATCH 差异放行
    if let Some(m) = zipstore::get_text(&es, "manifest.json") {
        if let Some(v) = json_str(m, "formatVersion") {
            let cur_major = FORMAT_VERSION.split('.').next().unwrap_or("1");
            let got_major = v.split('.').next().unwrap_or("1");
            if got_major.parse::<u32>().unwrap_or(0) > cur_major.parse::<u32>().unwrap_or(1) {
                return Err(format!(
                    "工程文件版本 {} 高于当前支持的 {}，请升级软件",
                    v, FORMAT_VERSION
                ));
            }
        }
    }

    // 快照优先：有它就无损
    if let Some(s) = zipstore::get_text(&es, SNAPSHOT) {
        if let Some(p) = Project::load_text(s) {
            return Ok(p);
        }
    }
    Err(".sprx 缺少可用的快照，无法还原工程".to_string())
}

/// 读 `.prism`：优先取旁挂快照（无损），否则退回纯文本失败。
pub fn read_prism(bytes: &[u8]) -> Result<Project, String> {
    let es = zipstore::read(bytes).map_err(|e| format!("无法解析 .prism：{}", e))?;

    if let Some(s) = zipstore::get_text(&es, SNAPSHOT) {
        if let Some(p) = Project::load_text(s) {
            return Ok(p);
        }
    }
    Err(".prism 缺少旁挂快照，无法完整还原（Prism 原生文件的完整解析未实现）".to_string())
}

/// 从一段手拼的 JSON 里取字符串字段（只处理我们自己写出的格式）。
fn json_str(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\"", key);
    let i = json.find(&needle)?;
    let rest = &json[i + needle.len()..];
    let rest = rest.trim_start();
    if !rest.starts_with(':') {
        return None;
    }
    let rest = rest[1..].trim_start();
    if !rest.starts_with('"') {
        return None;
    }
    let rest = &rest[1..];
    let mut out = String::new();
    let mut esc = false;
    for c in rest.chars() {
        if esc {
            out.push(c);
            esc = false;
        } else if c == '\\' {
            esc = true;
        } else if c == '"' {
            return Some(out);
        } else {
            out.push(c);
        }
    }
    None
}

/// 原子写盘：先写临时文件再 rename，中断也不会破坏原文件。
pub fn write_atomic(path: &str, data: &[u8]) -> std::io::Result<()> {
    let tmp = format!("{}.tmp", path);
    match std::fs::write(&tmp, data).and_then(|_| std::fs::rename(&tmp, path)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            // rename 失败（跨分区等）时退回直写
            std::fs::write(path, data).map_err(|_| e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Project;

    fn sample() -> Project {
        let mut p = Project::blank();
        p.tables[0].table.set_cell(0, 0, CellValue::Number(1.5));
        p.tables[0].table.set_cell(1, 0, CellValue::Number(2.5));
        p.tables[0].table.set_cell(0, 1, CellValue::Number(9.0));
        p
    }

    #[test]
    fn sprx_roundtrip_is_lossless() {
        let p = sample();
        let bytes = write_sprx(&p);
        let back = read_sprx(&bytes).unwrap();
        assert_eq!(back.tables.len(), p.tables.len());
        assert_eq!(
            back.tables[0].table.get_cell(0, 0),
            CellValue::Number(1.5)
        );
        assert_eq!(
            back.tables[0].table.get_cell(1, 0),
            CellValue::Number(2.5)
        );
    }

    #[test]
    fn sprx_mimetype_is_first_entry() {
        let bytes = write_sprx(&sample());
        let es = zipstore::read(&bytes).unwrap();
        assert_eq!(es[0].name, "mimetype");
        assert!(is_sprx_bytes(&bytes));
    }

    #[test]
    fn prism_roundtrip_via_sidecar() {
        let p = sample();
        let bytes = write_prism(&p);
        let back = read_prism(&bytes).unwrap();
        assert_eq!(
            back.tables[0].table.get_cell(0, 1),
            CellValue::Number(9.0)
        );
    }

    #[test]
    fn detect_by_content_not_extension() {
        let bytes = write_sprx(&sample());
        // 内容判定：即便后缀改成 .prism，detect 仍认出是 sprx
        assert_eq!(detect(&bytes), Format::Sprx);
        // 但保存分派走后缀
        assert!(is_sprx_container("a.SPRX"));
        assert!(!is_sprx_container("a.prism"));
    }

    #[test]
    fn csv_export_has_header_and_rows() {
        let p = sample();
        let csv = table_csv(&p, 0).unwrap();
        let lines: Vec<&str> = csv.lines().collect();
        assert!(!lines.is_empty());
        assert!(lines[0].contains(','), "首行应为列标题: {}", lines[0]);
        assert!(lines.len() >= 2, "应有数据行");
    }

    #[test]
    fn csv_escapes_commas_and_quotes() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn json_str_parses_our_own_output() {
        let j = "{\n  \"kind\": \"SpectraProject\",\n  \"formatVersion\": \"1.0.0\"\n}";
        assert_eq!(json_str(j, "kind").unwrap(), "SpectraProject");
        assert_eq!(json_str(j, "formatVersion").unwrap(), "1.0.0");
        assert!(json_str(j, "missing").is_none());
    }

    #[test]
    fn future_major_version_is_rejected() {
        let mut es = Vec::new();
        es.push(Entry::text("mimetype", MIMETYPE));
        es.push(Entry::text(
            "manifest.json",
            "{\"formatVersion\": \"99.0.0\", \"kind\": \"SpectraProject\"}",
        ));
        es.push(Entry::text(SNAPSHOT, &sample().save_text()));
        let bytes = zipstore::write(&es);
        assert!(read_sprx(&bytes).is_err());
    }

    #[test]
    fn minor_version_differences_are_accepted() {
        let mut es = Vec::new();
        es.push(Entry::text("mimetype", MIMETYPE));
        es.push(Entry::text(
            "manifest.json",
            "{\"formatVersion\": \"1.5.3\", \"kind\": \"SpectraProject\"}",
        ));
        es.push(Entry::text(SNAPSHOT, &sample().save_text()));
        let bytes = zipstore::write(&es);
        assert!(read_sprx(&bytes).is_ok());
    }

    #[test]
    fn atomic_write_creates_file() {
        let dir = std::env::temp_dir().join("spectra_atomic_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("out.sprx");
        let p = path.to_string_lossy().to_string();
        write_atomic(&p, &write_sprx(&sample())).unwrap();
        assert!(path.exists());
        let read_back = std::fs::read(&path).unwrap();
        assert!(is_sprx_bytes(&read_back));
        let _ = std::fs::remove_file(&path);
    }
}
