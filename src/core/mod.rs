//! StatAura 内核：数据模型、统计、配色、工程文件。
//!
//! 这一层完全不依赖 egui / 窗口，可单独测试，也方便将来做 CLI / 批量出图。

pub mod analysis;
pub mod color;
pub mod contingency;
pub mod curve;
pub mod expr;
pub mod graph;
pub mod infer;
pub mod model;
pub mod nested;
pub mod normality;
pub mod palette;
pub mod posthoc;
pub mod prism;
pub mod sample;
pub mod smart;
pub mod stats;
pub mod transform;

pub use color::Rgba;
pub use contingency::{chi_square, fisher_exact, ChiSquare, Fisher};
pub use curve::{best_curve, fit_curve, CurveFit, FitModel};
pub use graph::{ChartKind, ErrorKind, GraphSpec, LegendPos, PlotThemeId, SigAnnotation};
pub use model::{
    Analysis, AnalysisKind, AnalysisOptions, Cell, ColRole, Column, DataTable, Graph, Project,
    ResultTable, TableKind,
};
pub use nested::{nested_anova, nested_t, NestedAnova, NestedGroups, NestedT};
pub use palette::PaletteId;
