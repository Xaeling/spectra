//! 导出格式弹窗：绘图检查器里那个唯一的「导出」大按钮按下去出现的东西。
//!
//! # 为什么合并成一个入口
//!
//! 之前检查器里并排放着「导出图形」「导出 PNG」两个按钮，分别走
//! 设置里的后端（svg / tex / py）和一条写死 `spectra_chart.png` 的路径 ——
//! 用户既不知道点了哪个会出什么格式，也不知道文件最后落在哪个目录。
//!
//! 现在只有一个按钮：按下 → 在这里挑格式 → 弹系统保存对话框自己选位置。
//!
//! # 关于 JPEG XL
//!
//! `image` crate（以及它依赖的编码器生态）没有可用的 JPEG XL **编码器**，
//! 所以清单里没有它。硬塞一个点下去报错的选项，比不给更糟。

use charis_ui::prelude::*;

use crate::app::Msg;
use crate::export::ExportFormat;
use crate::i18n::{self, K, Lang};
use crate::palette::*;
use std::time::Instant;

pub fn export_dialog(lang: Lang, sel: usize, open_at: Instant) -> Element<Msg> {
    let t = |k: K| i18n::t(lang, k);
    let (mw, mh, mr, mdy, _done) = crate::ui::anim::morph(open_at, 430.0, 470.0);

    let mut list = col().gap(3.0).w_full();
    for (i, f) in ExportFormat::ALL.iter().enumerate() {
        let on = i == sel;
        list = list.child(fmt_row(*f, on, i));
    }

    let body = col()
        .gap(10.0)
        .w_full()
        .child(
            text("选择导出格式；点「导出」后在系统对话框里挑选保存位置".to_string())
                .font_size(ui_font_sm())
                .color(hex(dim())),
        )
        .child(list);

    backdrop(Msg::CloseExport, false)
        .size_full()
        .center()
        .child(crate::ui::dialog_panel(
            t(K::Export).to_string(),
            body,
            vec![
                primary_button(t(K::Cancel)).on_click(Msg::CloseExport),
                primary_button(t(K::Export).to_string()).on_click(Msg::ExportConfirm),
            ],
            mw,
            mh,
            mr,
            mdy,
            Msg::CloseExport,
        ))
}

/// 一行格式：左边格式名，右边一句说明。选中态用强调底色 + 强调色描边。
fn fmt_row(f: ExportFormat, on: bool, i: usize) -> Element<Msg> {
    row()
        .w_full()
        .px(10.0)
        .py(6.0)
        .gap(8.0)
        .items_center()
        .rounded(r_sm())
        .border(1.0, hex(if on { accent() } else { hair() }))
        .bg(hex(if on { accent_soft() } else { canvas_bg() }))
        .hover(|s| s.bg(hex(if on { accent_soft() } else { tool_hover() })))
        .on_click(Msg::SetExportFmt(i))
        .child(
            text(f.label().to_string())
                .font_size(ui_font())
                .semibold()
                .color(hex(if on { accent() } else { ink() }))
                .nowrap(),
        )
        .child(spacer())
        .child(
            text(f.hint().to_string())
                .font_size(ui_font_sm())
                .color(hex(dim())),
        )
}
