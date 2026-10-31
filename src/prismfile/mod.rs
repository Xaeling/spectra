//! `.prism` —— 兼容 GraphPad Prism 的**交换**格式。
//!
//! # 三层模型
//!
//! ```text
//! 容器层 = ZIP
//! 索引层 = document.json
//! 组件层 = CSV + JSON
//! ```
//!
//! # 与 `.sprx` 的分工
//!
//! `.prism` 是**有损桥接**：Prism 只认识它自己的对象图，我们的绘图选项
//! 大部分映射不过去。为了做到「本应用 → .prism → 本应用」往返无损，
//! 写端额外旁挂一份完整快照（[`SIDECAR_PROJECT`]）。
//!
//! Prism 侧读的是标准对象图，会忽略这两条附属条目；本应用侧读回来时
//! 优先取快照，绕开桥接损失。
//!
//! # 硬边界
//!
//! **写端宁可报错也不产出坏文件** —— `.prism` 一旦被 Prism 判定损坏，
//! 用户会认为「这个软件坏了」。所以 [`crate::prismfile::writer::write`]
//! 第一步就是 `validate()`，有任何硬伤直接拒绝。
//!
//! # 版本三元组
//!
//! 不得硬编码，从黄金模板 `resources/prism_base.prism` 继承
//! （[`template_versions`]）；模板缺失才回落常量。

pub mod bridge;
pub mod model;
pub mod reader;
pub mod writer;

#[allow(unused_imports)]
pub use bridge::{project_from_bytes, project_to_bytes};
#[allow(unused_imports)]
pub use reader::read;
#[allow(unused_imports)]
pub use writer::write;

/// 旁挂快照：完整 `Project` 的 pretty JSON。
pub const SIDECAR_PROJECT: &str = "spectra/project.json";
/// 旁挂快照的元信息。
pub const SIDECAR_META: &str = "spectra/meta.json";
/// **旧路径**，仅读端兼容：早期版本把快照写在 `prism-rs/` 下。
pub const SIDECAR_PROJECT_LEGACY: &str = "prism-rs/project.json";

/// 黄金模板路径。存在就从它继承版本三元组。
pub const TEMPLATE: &str = "resources/prism_base.prism";

// ── 模板缺失时的回落常量 ────────────────────────────────────────────────────

pub const TPL_FORMAT_VERSION: &str = "1-6-0";
pub const TPL_MIN_FORMAT_VERSION: &str = "1-2-0";
pub const TPL_MIN_PRISM_VERSION: &str = "10.1.0";

/// 版本三元组。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Versions {
    pub format_version: String,
    pub min_format_version: String,
    pub min_prism_version: String,
}

/// 从黄金模板继承版本三元组。
///
/// 模板缺失时返回 `None`，由调用方决定回落策略（写端会优先沿用
/// `pf.document` 里已有的值，再没有才用常量）。
pub fn template_versions() -> Option<Versions> {
    let bytes = std::fs::read(TEMPLATE).ok()?;
    let entries = crate::sprxfile::reader::read_entries(&bytes).ok()?;
    let doc = entries
        .iter()
        .find(|e| e.name == "document.json")?
        .json::<crate::prismfile::model::Document>()?;
    Some(Versions {
        format_version: doc.format_version.unwrap_or_default(),
        min_format_version: doc.min_format_version.unwrap_or_default(),
        min_prism_version: doc.min_prism_version.unwrap_or_default(),
    })
}

/// 只读不解析的二进制。判定与 `.sprx` 不同：这里是 `.dt` / `.bin`。
pub fn is_forbidden_binary(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".dt") || n.ends_with(".bin")
}
