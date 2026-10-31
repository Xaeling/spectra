//! 底部工具栏 + 状态栏。
//!
//! 底部工具栏：Go to sheet / View gallery / Previous / Next / Ping-pong。
//! 状态栏：显示当前光标位置，格式为「第 N 行, A: 列名」。

use charis_ui::prelude::*;

use crate::app::{Msg, SpectraApp};
use crate::i18n::{self, K, Lang};
use crate::palette::*;
use crate::ui::{tool_button, Glyph};

pub fn bottom_bar(app: &SpectraApp) -> Element<Msg> {
    let t = |k: K| i18n::t(app.lang, k);
    row()
        .h(BOTTOM_BAR_H)
        .bg(hex(toolbar_bg()))
        .border_t(1.0, hex(hair()))
        .px(6.0)
        .gap(6.0)
        .items_center()
        .child(text(t(K::GoTo)).font_size(ui_font()).color(hex(ink())))
        .child(text_input(app.goto.clone(), Msg::GoTo).w(72.0))
        .child(crate::ui::vsep_h(18.0))
        .child(tool_button(
            Glyph::Grid,
            t(K::ViewGallery),
            Msg::Tool("gallery"),
            false,
        ))
        .child(tool_button(
            Glyph::Chevron,
            t(K::PrevSheet),
            Msg::Tool("prev"),
            false,
        ))
        .child(tool_button(
            Glyph::Link,
            t(K::NextSheet),
            Msg::Tool("next"),
            false,
        ))
        .child(tool_button(
            Glyph::Undo,
            t(K::PingPong),
            Msg::Tool("pingpong"),
            false,
        ))
        .child(spacer())
        .child(text(app.sheet_label()).font_size(ui_font()).color(hex(dim())))
}

pub fn status_bar(app: &SpectraApp) -> Element<Msg> {
    row()
        .h(STATUS_H)
        .bg(hex(window_bg()))
        .border_t(1.0, hex(hair()))
        .px(6.0)
        .items_center()
        .gap(10.0)
        .child(text(app.status.clone()).font_size(ui_font()).color(hex(ink())))
        .child(spacer())
        .child(
            text(app.project_hint())
                .font_size(ui_font())
                .color(hex(dim())),
        )
}

/// 状态栏右侧的工程摘要（析光 显示工程名与表数量）。
pub fn project_hint(lang: Lang, tables: usize, results: usize, graphs: usize) -> String {
    let t = |k: K| i18n::t(lang, k);
    match lang {
        Lang::Zh => format!(
            "{} · {} 个数据表 · {} 个结果 · {} 个图形",
            t(K::ProjectHint),
            tables,
            results,
            graphs
        ),
        Lang::En => format!(
            "{} · {} data table(s) · {} result(s) · {} graph(s)",
            t(K::ProjectHint),
            tables,
            results,
            graphs
        ),
    }
}
