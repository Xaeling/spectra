//! `.sprx` —— 析光 Spectra 的**专有**工程格式。
//!
//! # 定位
//!
//! 这个格式不需要任何第三方软件能打开，所以结构可以自由设计。
//! 与 `.prism`（见 [`crate::prismfile`]）的分工是：
//!
//! | | `.sprx` | `.prism` |
//! |---|---|---|
//! | 定位 | 专有工程格式 | 兼容 Prism 的交换格式 |
//! | 图表选项 | `spec.json` **完整**保存，无损 | 桥接映射，有损 |
//! | 设置字段 | 直接进 `project.json` | 只能旁挂 |
//! | 往返 | 原生无损 | 靠旁挂快照补回损失 |
//!
//! # 容器
//!
//! ZIP。第一项必须是 `mimetype` 且 **Stored**（不压缩）—— 读端只读前几百字节
//! 就能判定格式，不必解压整个文件。其余条目一律 Deflated。
//!
//! 底层用 `zip` crate，不手写 Local Header / Central Directory / EOCD：
//! ZIP64、数据描述符、第三方产出的文件这些边界自己实现容易漏。

pub mod model;
pub mod reader;
pub mod writer;

#[allow(unused_imports)]
pub use model::Manifest;
#[allow(unused_imports)]
pub use reader::{is_sprx, read};
#[allow(unused_imports)]
pub use writer::{write, write_atomic};

/// 容器第一项的内容。读端靠它判定格式，所以**不能改**。
pub const MIMETYPE: &str = "application/vnd.spectra.project+zip";

/// 当前写出的格式版本。读端只比对 MAJOR。
pub const FORMAT_VERSION: &str = "1.0.0";

/// 完整工程快照的条目名。有它就无损还原，没有就走结构化回退。
pub const SNAPSHOT: &str = "spectra/snapshot.json";

// ── schema URL ──────────────────────────────────────────────────────────────
// 每个 JSON 对象都带一个 `$schema`，指向本格式的说明文档。
// 纯标注用途：读端不下载、不校验，只是让打开文件的人知道该去哪查结构。

pub const SCHEMA_BASE: &str = "https://spectra.dev/schema";

macro_rules! schemas {
    ($($name:ident => $path:literal),* $(,)?) => {
        $(
            #[doc = concat!("`", $path, "` 的 schema 地址。")]
            pub const $name: &str = concat!("https://spectra.dev/schema/", $path, "/v1.json");
        )*
        /// 全部 12 个 schema 地址，顺序与上面声明一致。
        #[allow(dead_code)]
        pub const ALL_SCHEMAS: &[&str] = &[$($name),*];
    };
}

schemas! {
    SCHEMA_MANIFEST       => "manifest",
    SCHEMA_PROJECT        => "project",
    SCHEMA_DATA_SHEET     => "data-sheet",
    SCHEMA_DATA_TABLE     => "data-table",
    SCHEMA_COLUMN         => "column",
    SCHEMA_ANALYSIS_SHEET => "analysis-sheet",
    SCHEMA_ANALYSIS_VIEW  => "analysis-view",
    SCHEMA_GRAPH_SHEET    => "graph-sheet",
    SCHEMA_GRAPH_SPEC     => "graph-spec",
    SCHEMA_INFO_SHEET     => "info-sheet",
    SCHEMA_THEME          => "theme",
}

/// 大写 UUID v4。目录名与对它的引用必须**逐字一致**，所以统一在这里生成。
pub fn uuid_upper() -> String {
    // 不引 uuid crate：只需要一个够用的随机标识，16 字节随机数足够，
    // 而为一个 4 行的函数拉依赖不值。
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut x = seed;
    let mut bytes = [0u8; 16];
    for b in bytes.iter_mut() {
        // xorshift：够随机，且跨平台结果确定
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *b = (x >> 24) as u8;
    }
    bytes[6] = (bytes[6] & 0x0F) | 0x40; // version 4
    bytes[8] = (bytes[8] & 0x3F) | 0x80; // variant 10
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut s = String::with_capacity(36);
    for (i, b) in bytes.iter().enumerate() {
        if matches!(i, 4 | 6 | 8 | 10) {
            s.push('-');
        }
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xF) as usize] as char);
    }
    s
}

/// ISO8601 时间戳。
pub fn now_iso() -> String {
    // 不引 chrono：只需要一个可读的落盘时间，格式固定，
    // 用 SystemTime 手算 UTC 偏移太麻烦，因此只写到秒并标 Z。
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 把秒数转成 Y-M-DTh:m:sZ
    let days = (secs / 86400) as i64;
    let rem = (secs % 86400) as u32;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, m, d,
        rem / 3600, (rem % 3600) / 60, rem % 60
    )
}

/// Howard Hinnant 的 civil_from_days：1970-01-01 = day 0，返回 (年, 月, 日)。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// ── 条目路径 ────────────────────────────────────────────────────────────────
// 集中在这里，写端与读端共用同一套字串，避免两边各拼一份而对不上。

/// `columns/<id>.json`
pub fn column_path(id: &str) -> String {
    format!("columns/{}.json", id)
}
/// `sheets/data/<id>/sheet.json`
pub fn data_sheet_path(id: &str) -> String {
    format!("sheets/data/{}/sheet.json", id)
}
/// `sheets/data/<id>/table.json`
pub fn data_table_path(id: &str) -> String {
    format!("sheets/data/{}/table.json", id)
}
/// [`TableType`] 与 `table.json` 里 `extra.tableType` 字符串的互转。
///
/// 规范里的 `DataTableSpec` 没有表类型字段（只有行/列数），而它有
/// `#[serde(flatten)] extra`，所以把这个信息挂在 extra 上既不破坏规范、
/// 又能让结构化回退路径恢复真实表类型 —— 否则分组表存一次再打开就退化成列表。
pub fn table_type_key(t: crate::model::TableType) -> &'static str {
    use crate::model::TableType;
    match t {
        TableType::XY => "XY",
        TableType::Column => "Column",
        TableType::Grouped => "Grouped",
        TableType::Contingency => "Contingency",
        TableType::Survival => "Survival",
        TableType::PartsOfWhole => "PartsOfWhole",
        TableType::MultipleVariables => "MultipleVariables",
        TableType::Nested => "Nested",
    }
}

pub fn table_type_of(s: &str) -> crate::model::TableType {
    use crate::model::TableType;
    match s {
        "XY" => TableType::XY,
        "Grouped" => TableType::Grouped,
        "Contingency" => TableType::Contingency,
        "Survival" => TableType::Survival,
        "PartsOfWhole" => TableType::PartsOfWhole,
        "MultipleVariables" => TableType::MultipleVariables,
        "Nested" => TableType::Nested,
        _ => TableType::Column,
    }
}

/// `sheets/data/<id>/data.csv`
pub fn data_csv_path(id: &str) -> String {
    format!("sheets/data/{}/data.csv", id)
}
/// `sheets/analyses/<id>/sheet.json`
pub fn analysis_sheet_path(id: &str) -> String {
    format!("sheets/analyses/{}/sheet.json", id)
}
/// `sheets/analyses/<id>/parameters.json`
pub fn analysis_params_path(id: &str) -> String {
    format!("sheets/analyses/{}/parameters.json", id)
}
/// `sheets/analyses/<id>/results.json`
pub fn analysis_results_path(id: &str) -> String {
    format!("sheets/analyses/{}/results.json", id)
}
/// `sheets/analyses/<sid>/views/<vid>.json`
pub fn analysis_view_path(sid: &str, vid: &str) -> String {
    format!("sheets/analyses/{}/views/{}.json", sid, vid)
}
/// `sheets/graphs/<id>/sheet.json`
pub fn graph_sheet_path(id: &str) -> String {
    format!("sheets/graphs/{}/sheet.json", id)
}
/// `sheets/graphs/<id>/spec.json`
pub fn graph_spec_path(id: &str) -> String {
    format!("sheets/graphs/{}/spec.json", id)
}
/// `sheets/infos/<id>/sheet.json`
pub fn info_sheet_path(id: &str) -> String {
    format!("sheets/infos/{}/sheet.json", id)
}

// ══════════════════════════════════════════════════════════════════════════
// 打开 / 保存分派
// ══════════════════════════════════════════════════════════════════════════

/// 打开时的格式判定：靠**内容**（改名不影响打开）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Format {
    Sprx,
    Prism,
    /// 旧版纯文本 .spectra
    Plain,
}

/// 三级判定链：mimetype → ZIP 魔数 → 旧版明文。
pub fn detect(bytes: &[u8]) -> Format {
    if is_sprx(bytes) {
        Format::Sprx
    } else if bytes.len() >= 4
        && bytes[0] == 0x50
        && bytes[1] == 0x4B
        && bytes[2] == 0x03
        && bytes[3] == 0x04
    {
        Format::Prism
    } else {
        Format::Plain
    }
}

/// 解析任意一种能认出的格式。
pub fn read_any(bytes: &[u8]) -> Result<crate::model::Project, String> {
    match detect(bytes) {
        Format::Plain => {
            let s = String::from_utf8_lossy(bytes);
            crate::model::Project::load_text(&s)
                .ok_or_else(|| "无法解析旧版 .spectra 文本".to_string())
        }
        Format::Sprx => reader::read(bytes),
        Format::Prism => crate::prismfile::project_from_bytes(bytes),
    }
}

/// 保存时的格式判定：靠**后缀**（后缀是用户显式选择，不该被内容覆盖）。
///
/// 与打开的不对称是**有意的**：打开靠内容保证改名也能开，
/// 保存靠后缀保证用户选了什么就写什么。
pub fn is_sprx_container(path: &str) -> bool {
    path.to_lowercase().ends_with(".sprx")
}

/// 按后缀分派写端。`.prism` 可能因校验失败而报错（宁可报错也不写坏文件）。
pub fn write_by_ext(p: &crate::model::Project, path: &str) -> Result<Vec<u8>, String> {
    if is_sprx_container(path) {
        Ok(writer::write(p))
    } else {
        let pf = crate::prismfile::bridge::to_prism(p);
        crate::prismfile::writer::write_with_snapshot(&pf, p)
    }
}

/// 信息表的默认常量名。**中文原样保留，不翻译** —— 与 `.prism` 侧一致。
pub const INFO_DEFAULTS: &[&str] = &[
    "实验日期",
    "实验 ID",
    "笔记本 ID",
    "项目",
    "实验者",
    "协议",
];
