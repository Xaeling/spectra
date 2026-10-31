//! 居中提示弹窗：只承载**重要**信息。
//!
//! 设计原则：底部状态栏放例行反馈（文件写到哪、缩放多少），
//! 这里只放会打断用户的事 —— 出错、无法执行、需要确认。
//! 与「欢迎 / 设置 / 关于」共用同一套覆盖层，因此互斥：新的顶掉旧的。

use charis_ui::prelude::*;

use crate::app::Msg;
use crate::i18n::{self, K, Lang};
use crate::palette::*;
use std::time::Instant;

pub fn alert_dialog(lang: Lang, msg: String, open_at: Instant) -> Element<Msg> {
    let t = |k: K| i18n::t(lang, k);
    // 高度按文本长度自适应：短句不该撑成一个大方块，长句也不该挤成一条。
    // 先按每行约 34 个汉字估行数，再夹到 [150, 340]。
    let lines = (msg.chars().count() as f32 / 34.0).ceil().max(1.0);
    let h = (96.0 + lines * 22.0).clamp(150.0, 340.0);
    let (mw, mh, mr, mdy, _done) = crate::ui::anim::morph(open_at, 400.0, h);

    let body = col()
        .gap(10.0)
        .w_full()
        .child(text(msg).font_size(ui_font()).color(hex(ink())).nowrap());

    // dim = false：不再铺那层灰（会把底层界面整个盖成一块灰方块）。
    // 遮罩只负责「点外部 = 关闭」，不画任何可见矩形。
    backdrop(Msg::CloseAlert, false)
        .size_full()
        .center()
        .child(crate::ui::dialog_panel(
            t(K::NoticeTitle).to_string(),
            body,
            vec![primary_button(t(K::Ok)).on_click(Msg::CloseAlert)],
            mw,
            mh,
            mr,
            mdy,
            Msg::CloseAlert,
        ))
}
