//! StatAura —— 科研统计与出版级绘图。
//!
//! 分层：`core`（数据/统计，无 UI）→ `plot`（矢量绘图，无 UI）→ `app`（egui 桌面壳）。

pub mod app;
pub mod core;
pub mod io;
pub mod plot;
