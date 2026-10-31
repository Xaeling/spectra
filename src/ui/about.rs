//! Help ▸ 关于：一个只读的信息对话框。
//!
//! 与设置窗口一样属于覆盖层，但更轻量：只有说明文字 + 关闭按钮。
//! 打开时由 `SpectraApp::close_all_overlays()` 关掉其他弹窗，
//! 所以「先点设置、再点帮助」不会出现两层叠在一起的情况。

use charis_ui::prelude::*;

use crate::app::Msg;
use crate::i18n::{self, K, Lang};
use crate::palette::*;
use std::time::Instant;

pub fn about_dialog(lang: Lang, open_at: Instant) -> Element<Msg> {
    let t = |k: K| i18n::t(lang, k);
    let (mw, mh, mr, mdy, _done) = crate::ui::anim::morph(open_at, 440.0, 260.0);

    let body = col()
        .gap(10.0)
        .w_full()
        .child(
            text("Spectra")
                .font_size(ui_font_lg())
                .bold()
                .color(hex(ink())),
        )
        .child(div().w_full().h(1.0).bg(hex(hair())))
        .child(text(t(K::AboutText)).font_size(ui_font()).color(hex(ink())))
        .child(
            text(t(K::AboutStack))
                .font_size(ui_font_sm())
                .color(hex(dim())),
        );

    // dim = false：不铺灰底，遮罩只负责「点外部关闭」。
    backdrop(Msg::CloseAbout, false)
        .size_full()
        .center()
        .child(crate::ui::dialog_panel(
            t(K::AboutSpectra).to_string(),
            body,
            vec![primary_button(t(K::Ok)).on_click(Msg::CloseAbout)],
            mw,
            mh,
            mr,
            mdy,
            Msg::CloseAbout,
        ))
}
