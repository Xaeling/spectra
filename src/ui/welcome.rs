//! 启动时的「Welcome to 析光 Spectra」模态对话框。
//!
//! 结构：左侧是 New table & graph（8 种表）+ Existing file（只留「打开现有文件」），
//! 右侧是所选表类型的说明，底部 Cancel / Create。
//!
//! 内容区带 `scroll_y`：窗口不高时对话框不会超出屏幕。
//!
//! 注意：这里**不画预览缩略图**。之前右侧有一张 240×130 的自绘柱图，
//! 它在动画期间会被容器挤压溢出（表现为弹窗凸出一截），
//! 而且对「选哪种表」这件事没有帮助 —— 已经去掉。

use charis_ui::prelude::*;

use crate::app::{Msg, SpectraApp};
use crate::i18n::{self, K};
use crate::model::{EXISTING_ITEMS, TABLE_KINDS};
use crate::palette::*;
use crate::ui::{radio, Glyph};

pub fn welcome(app: &SpectraApp) -> Element<Msg> {
    let lang = app.lang;
    let t = |k: K| i18n::t(lang, k);
    let (mw, mh, mr, mdy, _done) = crate::ui::anim::morph(app.dlg_open_at, 720.0, 480.0);

    // pick_table == usize::MAX 表示用户选的是 Existing file 分组
    let (name, desc) = if app.pick_table < TABLE_KINDS.len() {
        let (nk, dk) = TABLE_KINDS[app.pick_table];
        (t(nk), t(dk))
    } else {
        (t(K::ExistingFile), t(K::ExistingFileDesc))
    };

    // 左侧：类别列表
    let mut left = col().gap(1.0).w(210.0).bg(hex(canvas_bg()));
    left = left.child(
        text(t(K::NewTableGraph))
            .font_size(ui_font())
            .semibold()
            .color(hex(ink())),
    );
    for (i, (nk, _)) in TABLE_KINDS.iter().enumerate() {
        left = left.child(indent_row(radio(
            app.pick_table == i,
            t(*nk),
            Msg::PickTableKind(i),
        )));
    }
    left = left.child(div().h(8.0));
    left = left.child(
        text(t(K::ExistingFile))
            .font_size(ui_font())
            .semibold()
            .color(hex(ink())),
    );
    for (i, nk) in EXISTING_ITEMS.iter().enumerate() {
        left = left.child(indent_row(radio(
            app.pick_existing == i && app.pick_table == usize::MAX,
            t(*nk),
            Msg::PickExisting(i),
        )));
    }

    // 右侧：说明。不再放预览图。
    let right = col()
        .gap(8.0)
        .flex1()
        .bg(hex(canvas_bg()))
        .child(text(name).font_size(ui_font_lg()).bold().color(hex(ink())))
        .child(text(desc).font_size(ui_font()).color(hex(ink())))
        .child(div().w_full().h(1.0).bg(hex(hair())))
        .child(text(t(K::EnterOrImport)).font_size(ui_font()).color(hex(ink())))
        .child(spacer());

    // 高度上限保守一点：窗口纵向不足时优先压缩对话框自身，而不是被裁掉。
    let body = col()
        .gap(12.0)
        .w_full()
        .bg(hex(canvas_bg()))
        .child(
            row()
                .gap(12.0)
                .bg(hex(canvas_bg()))
                .child(left.scroll_y())
                .child(right.scroll_y()),
        )
        .child(
            row()
                .gap(6.0)
                .items_center()
                .bg(hex(canvas_bg()))
                .child(text(t(K::UseTutorialData)).font_size(ui_font_sm()).color(hex(dim())))
                .child(glyph_small(Glyph::Chevron)),
        );

    // dim = false：不铺灰底。遮罩只负责「点外部 = 取消」。
    backdrop(Msg::WelcomeCancel, false)
        .size_full()
        .center()
        .child(crate::ui::dialog_panel(
            t(K::WelcomeTitle).to_string(),
            body,
            vec![
                button(t(K::Cancel)).on_click(Msg::WelcomeCancel),
                primary_button(t(K::Create)).on_click(Msg::WelcomeCreate),
            ],
            mw,
            mh,
            mr,
            mdy,
            Msg::WelcomeCancel,
        ))
}

fn indent_row(e: Element<Msg>) -> Element<Msg> {
    row().pl(10.0).w_full().child(e)
}

fn glyph_small(g: Glyph) -> Element<Msg> {
    crate::ui::glyph_at(g, 9.0)
}
