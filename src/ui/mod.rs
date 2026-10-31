//! 通用小部件：1px 分隔线、自绘工具栏图标、工具栏按钮、分区标题。
//!
//! 图标全部用 canvas 自绘，不依赖任何图标字体或资源文件——
//! 这样在任何平台（含无 Arial 的 Linux）观感都一致，也方便逐像素微调。

use charis_ui::prelude::*;
use charis_ui::style::Shadow;

use crate::app::Msg;
use crate::palette::*;

pub mod about;
pub mod alert;
pub mod anim;
pub mod context;
pub mod export;
pub mod grid;
pub mod inspector;
pub mod motion;
pub mod menubar;
pub mod navigator;
pub mod settings;
pub mod status;
pub mod toolbar;
pub mod welcome;

// ── 分隔线 ──────────────────────────────────────────────────────────────────
// 1px 冷灰实线。克制的原则：能靠留白分隔的地方就不用线。
pub fn vsep<M: Clone + 'static>() -> Element<M> {
    div().w(1.0).h_full().bg(hex(hair()))
}

pub fn hsep<M: Clone + 'static>() -> Element<M> {
    div().w_full().h(1.0).bg(hex(hair()))
}

/// 指定高度/宽度的 1px 线（工具栏组间、表头下沿）。
pub fn vsep_h<M: Clone + 'static>(h: f32) -> Element<M> {
    div().w(1.0).h(h).bg(hex(hair()))
}

// ── 投影 / 卡片 ─────────────────────────────────────────────────────────────
// 浅冷灰投影：在浅色界面上比黑色更干净，不会把界面「压脏」。
pub fn shadow_card() -> Shadow {
    Shadow::new(0.0, 1.0, 3.0, 0.0, hex(shadow_card_color()))
}

/// 面板左边缘的浅投影（检查器从右侧「压」在工作区之上）。
pub fn shadow_panel_left() -> Shadow {
    Shadow::new(-1.0, 0.0, 3.0, 0.0, hex(shadow_card_color()))
}

/// 浮层（对话框、下拉）用的较深投影。
pub fn shadow_pop() -> Shadow {
    Shadow::new(0.0, 10.0, 28.0, -6.0, hex(shadow_pop_color()))
}

/// 卡片容器：白底 + 极浅边框 + 圆角 + 浅投影。
/// 用它把「一组相关配置」圈起来，比画一堆分隔线更克制。
#[allow(dead_code)]
pub fn card() -> Element<Msg> {
    div()
        .w_full()
        .bg(hex(canvas_bg()))
        .border(1.0, hex(hair()))
        .rounded(r_md())
        .shadow(shadow_card())
}

/// 分区标题：小字号 + 半粗 + 下方一条极浅分隔线。
/// 不放大字号、不加色块 —— 靠层级而不是靠重量制造「高级感」。
pub fn section_text(title: &str) -> Element<Msg> {
    text(title.to_string())
        .font_size(ui_font_xs())
        .semibold()
        .color(hex(dim()))
}

// ── 图标 ────────────────────────────────────────────────────────────────────
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum Glyph {
    Plus,
    Folder,
    Floppy,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Analyze,
    Wand,
    Import,
    Pencil,
    Write,
    Text,
    Export,
    Printer,
    Send,
    Cloud,
    Help,
    Grid,
    Chevron,
    Search,
    Link,
    Bars,
    // ── 功能区（Ribbon）专用 ──────────────────────────────────────────────
    Highlight,
    Freeze,
    Insert,
    Delete,
    Format,
    Decimal,
    Sort,
    Series,
    Exclude,
    Dice,
}

/// 自绘图标。所有坐标是 rect 内的比例，缩放不失真。
pub fn glyph_at<M: Clone + 'static>(g: Glyph, size: f32) -> Element<M> {
    canvas(move |c, r| {
        let (x, y, w, h) = (r.origin().x, r.origin().y, r.w, r.h);
        let ink = hex(ink());
        let p = |fx: f32, fy: f32| Point::new(x + w * fx, y + h * fy);
        match g {
            Glyph::Plus => {
                c.line(p(0.5, 0.12), p(0.5, 0.88), 1.2, ink);
                c.line(p(0.12, 0.5), p(0.88, 0.5), 1.2, ink);
            }
            Glyph::Folder => {
                c.line(p(0.08, 0.28), p(0.08, 0.88), 1.0, ink);
                c.line(p(0.08, 0.28), p(0.42, 0.28), 1.0, ink);
                c.line(p(0.42, 0.28), p(0.5, 0.4), 1.0, ink);
                c.line(p(0.5, 0.4), p(0.92, 0.4), 1.0, ink);
                c.line(p(0.92, 0.4), p(0.92, 0.88), 1.0, ink);
                c.line(p(0.08, 0.88), p(0.92, 0.88), 1.0, ink);
            }
            Glyph::Floppy => {
                c.line(p(0.15, 0.15), p(0.85, 0.15), 1.0, ink);
                c.line(p(0.85, 0.15), p(0.85, 0.85), 1.0, ink);
                c.line(p(0.85, 0.85), p(0.15, 0.85), 1.0, ink);
                c.line(p(0.15, 0.85), p(0.15, 0.15), 1.0, ink);
                c.line(p(0.32, 0.15), p(0.32, 0.42), 1.0, ink);
                c.line(p(0.32, 0.42), p(0.68, 0.42), 1.0, ink);
                c.line(p(0.68, 0.42), p(0.68, 0.15), 1.0, ink);
            }
            Glyph::Undo => {
                c.line(p(0.2, 0.55), p(0.55, 0.55), 1.0, ink);
                c.line(p(0.55, 0.55), p(0.55, 0.3), 1.0, ink);
                c.line(p(0.55, 0.3), p(0.3, 0.3), 1.0, ink);
                c.line(p(0.3, 0.3), p(0.38, 0.22), 1.0, ink);
                c.line(p(0.3, 0.3), p(0.38, 0.38), 1.0, ink);
                c.line(p(0.2, 0.55), p(0.2, 0.75), 1.0, ink);
            }
            Glyph::Redo => {
                c.line(p(0.8, 0.55), p(0.45, 0.55), 1.0, ink);
                c.line(p(0.45, 0.55), p(0.45, 0.3), 1.0, ink);
                c.line(p(0.45, 0.3), p(0.7, 0.3), 1.0, ink);
                c.line(p(0.7, 0.3), p(0.62, 0.22), 1.0, ink);
                c.line(p(0.7, 0.3), p(0.62, 0.38), 1.0, ink);
                c.line(p(0.8, 0.55), p(0.8, 0.75), 1.0, ink);
            }
            Glyph::Cut => {
                c.line(p(0.2, 0.2), p(0.7, 0.7), 1.0, ink);
                c.line(p(0.7, 0.2), p(0.2, 0.7), 1.0, ink);
                c.line(p(0.5, 0.85), p(0.85, 0.85), 1.0, ink);
            }
            Glyph::Copy => {
                c.line(p(0.28, 0.2), p(0.78, 0.2), 1.0, ink);
                c.line(p(0.78, 0.2), p(0.78, 0.68), 1.0, ink);
                c.line(p(0.78, 0.68), p(0.28, 0.68), 1.0, ink);
                c.line(p(0.28, 0.68), p(0.28, 0.2), 1.0, ink);
                c.line(p(0.42, 0.32), p(0.42, 0.8), 1.0, ink);
                c.line(p(0.42, 0.8), p(0.92, 0.8), 1.0, ink);
                c.line(p(0.92, 0.8), p(0.92, 0.32), 1.0, ink);
            }
            Glyph::Paste => {
                c.line(p(0.25, 0.22), p(0.75, 0.22), 1.0, ink);
                c.line(p(0.5, 0.22), p(0.5, 0.1), 1.0, ink);
                c.line(p(0.25, 0.22), p(0.25, 0.88), 1.0, ink);
                c.line(p(0.75, 0.22), p(0.75, 0.88), 1.0, ink);
                c.line(p(0.25, 0.88), p(0.75, 0.88), 1.0, ink);
                c.line(p(0.38, 0.45), p(0.62, 0.45), 1.0, ink);
                c.line(p(0.38, 0.62), p(0.62, 0.62), 1.0, ink);
            }
            Glyph::Analyze => {
                // 三根高度不同的柱子：统计分析的通用语义
                c.line(p(0.22, 0.85), p(0.22, 0.55), 1.2, ink);
                c.line(p(0.5, 0.85), p(0.5, 0.28), 1.2, ink);
                c.line(p(0.78, 0.85), p(0.78, 0.42), 1.2, ink);
                c.line(p(0.12, 0.85), p(0.9, 0.85), 1.0, ink);
            }
            Glyph::Bars => {
                // 用粗线当柱体，避免依赖 Rect 构造函数
                c.line(p(0.3, 0.86), p(0.3, 0.4), 3.0, ink);
                c.line(p(0.68, 0.86), p(0.68, 0.18), 3.0, ink);
            }
            Glyph::Wand => {
                c.line(p(0.2, 0.85), p(0.8, 0.2), 1.2, ink);
                c.line(p(0.72, 0.12), p(0.88, 0.28), 1.0, ink);
                c.line(p(0.6, 0.1), p(0.7, 0.2), 1.0, ink);
                c.line(p(0.85, 0.35), p(0.95, 0.45), 1.0, ink);
            }
            Glyph::Import => {
                c.line(p(0.5, 0.15), p(0.5, 0.6), 1.0, ink);
                c.line(p(0.5, 0.6), p(0.36, 0.46), 1.0, ink);
                c.line(p(0.5, 0.6), p(0.64, 0.46), 1.0, ink);
                c.line(p(0.2, 0.85), p(0.8, 0.85), 1.0, ink);
            }
            Glyph::Pencil => {
                c.line(p(0.22, 0.82), p(0.72, 0.32), 1.0, ink);
                c.line(p(0.72, 0.32), p(0.86, 0.18), 1.0, ink);
                c.line(p(0.72, 0.32), p(0.58, 0.46), 1.0, ink);
                c.line(p(0.18, 0.86), p(0.28, 0.76), 1.0, ink);
            }
            Glyph::Write => {
                c.line(p(0.2, 0.22), p(0.8, 0.22), 1.2, ink);
                c.line(p(0.5, 0.22), p(0.5, 0.82), 1.2, ink);
            }
            Glyph::Text => {
                c.line(p(0.2, 0.82), p(0.5, 0.18), 1.0, ink);
                c.line(p(0.5, 0.18), p(0.8, 0.82), 1.0, ink);
                c.line(p(0.32, 0.62), p(0.68, 0.62), 1.0, ink);
            }
            Glyph::Export => {
                c.line(p(0.15, 0.55), p(0.62, 0.55), 1.0, ink);
                c.line(p(0.62, 0.55), p(0.5, 0.42), 1.0, ink);
                c.line(p(0.62, 0.55), p(0.5, 0.68), 1.0, ink);
                c.line(p(0.72, 0.25), p(0.72, 0.85), 1.0, ink);
            }
            Glyph::Printer => {
                c.line(p(0.25, 0.3), p(0.75, 0.3), 1.0, ink);
                c.line(p(0.75, 0.3), p(0.75, 0.55), 1.0, ink);
                c.line(p(0.75, 0.55), p(0.25, 0.55), 1.0, ink);
                c.line(p(0.25, 0.55), p(0.25, 0.3), 1.0, ink);
                c.line(p(0.38, 0.55), p(0.38, 0.72), 1.0, ink);
                c.line(p(0.38, 0.72), p(0.62, 0.72), 1.0, ink);
                c.line(p(0.62, 0.72), p(0.62, 0.55), 1.0, ink);
            }
            Glyph::Send => {
                c.line(p(0.15, 0.5), p(0.85, 0.18), 1.0, ink);
                c.line(p(0.85, 0.18), p(0.85, 0.82), 1.0, ink);
                c.line(p(0.85, 0.82), p(0.15, 0.5), 1.0, ink);
            }
            Glyph::Cloud => {
                c.fill_circle(p(0.35, 0.62), h * 0.18, ink);
                c.fill_circle(p(0.55, 0.5), h * 0.24, ink);
                c.fill_circle(p(0.72, 0.64), h * 0.17, ink);
            }
            Glyph::Help => {
                c.line(p(0.32, 0.32), p(0.68, 0.32), 1.0, ink);
                c.line(p(0.68, 0.32), p(0.68, 0.5), 1.0, ink);
                c.line(p(0.68, 0.5), p(0.5, 0.5), 1.0, ink);
                c.line(p(0.5, 0.5), p(0.5, 0.66), 1.0, ink);
                c.fill_circle(p(0.5, 0.8), 1.4, ink);
            }
            Glyph::Grid => {
                c.line(p(0.12, 0.12), p(0.88, 0.12), 1.0, ink);
                c.line(p(0.12, 0.5), p(0.88, 0.5), 1.0, ink);
                c.line(p(0.12, 0.88), p(0.88, 0.88), 1.0, ink);
                c.line(p(0.12, 0.12), p(0.12, 0.88), 1.0, ink);
                c.line(p(0.5, 0.12), p(0.5, 0.88), 1.0, ink);
                c.line(p(0.88, 0.12), p(0.88, 0.88), 1.0, ink);
            }
            Glyph::Chevron => {
                c.line(p(0.25, 0.35), p(0.5, 0.65), 1.2, ink);
                c.line(p(0.5, 0.65), p(0.75, 0.35), 1.2, ink);
            }
            Glyph::Search => {
                c.line(p(0.3, 0.3), p(0.62, 0.62), 1.2, ink);
                c.line(p(0.62, 0.62), p(0.82, 0.82), 1.6, ink);
                c.line(p(0.62, 0.3), p(0.3, 0.62), 1.0, ink);
            }
            Glyph::Link => {
                c.line(p(0.35, 0.65), p(0.65, 0.35), 1.0, ink);
                c.line(p(0.42, 0.5), p(0.28, 0.68), 1.0, ink);
                c.line(p(0.58, 0.5), p(0.72, 0.32), 1.0, ink);
            }
            Glyph::Highlight => {
                // 记号笔：斜杆 + 笔头下的两道，表示「给单元格上色」
                c.line(p(0.25, 0.75), p(0.75, 0.25), 1.2, ink);
                c.line(p(0.6, 0.4), p(0.85, 0.65), 1.6, ink);
                c.line(p(0.2, 0.8), p(0.35, 0.95), 1.6, ink);
            }
            Glyph::Freeze => {
                // 雪花：三条交叉线 + 端点小分叉
                c.line(p(0.5, 0.1), p(0.5, 0.9), 1.0, ink);
                c.line(p(0.17, 0.3), p(0.83, 0.7), 1.0, ink);
                c.line(p(0.17, 0.7), p(0.83, 0.3), 1.0, ink);
                c.line(p(0.5, 0.22), p(0.36, 0.32), 1.0, ink);
                c.line(p(0.5, 0.22), p(0.64, 0.32), 1.0, ink);
                c.line(p(0.5, 0.78), p(0.36, 0.68), 1.0, ink);
                c.line(p(0.5, 0.78), p(0.64, 0.68), 1.0, ink);
            }
            Glyph::Insert => {
                // 表格 + 行间插入箭头
                c.line(p(0.12, 0.2), p(0.88, 0.2), 1.0, ink);
                c.line(p(0.12, 0.2), p(0.12, 0.8), 1.0, ink);
                c.line(p(0.88, 0.2), p(0.88, 0.8), 1.0, ink);
                c.line(p(0.12, 0.5), p(0.88, 0.5), 1.0, ink);
                c.line(p(0.12, 0.8), p(0.88, 0.8), 1.0, ink);
                c.line(p(0.5, 0.32), p(0.5, 0.68), 1.2, ink);
                c.line(p(0.5, 0.68), p(0.38, 0.56), 1.2, ink);
                c.line(p(0.5, 0.68), p(0.62, 0.56), 1.2, ink);
            }
            Glyph::Delete => {
                // 垃圾桶：盖 + 桶身 + 两道竖纹
                c.line(p(0.2, 0.28), p(0.8, 0.28), 1.0, ink);
                c.line(p(0.42, 0.28), p(0.42, 0.16), 1.0, ink);
                c.line(p(0.58, 0.16), p(0.58, 0.28), 1.0, ink);
                c.line(p(0.42, 0.16), p(0.58, 0.16), 1.0, ink);
                c.line(p(0.26, 0.28), p(0.32, 0.88), 1.0, ink);
                c.line(p(0.74, 0.28), p(0.68, 0.88), 1.0, ink);
                c.line(p(0.32, 0.88), p(0.68, 0.88), 1.0, ink);
                c.line(p(0.44, 0.4), p(0.46, 0.76), 1.0, ink);
                c.line(p(0.56, 0.4), p(0.54, 0.76), 1.0, ink);
            }
            Glyph::Format => {
                // 表格边框 + 一条粗行（表格式样）
                c.line(p(0.1, 0.22), p(0.9, 0.22), 1.0, ink);
                c.line(p(0.1, 0.22), p(0.1, 0.8), 1.0, ink);
                c.line(p(0.9, 0.22), p(0.9, 0.8), 1.0, ink);
                c.line(p(0.1, 0.8), p(0.9, 0.8), 1.0, ink);
                c.line(p(0.1, 0.42), p(0.9, 0.42), 1.0, ink);
                c.line(p(0.5, 0.22), p(0.5, 0.8), 1.0, ink);
                c.line(p(0.18, 0.3), p(0.42, 0.3), 2.0, ink);
                c.line(p(0.18, 0.6), p(0.42, 0.6), 2.0, ink);
            }
            Glyph::Decimal => {
                // 「1.0」—— 小数点位数
                c.line(p(0.22, 0.25), p(0.34, 0.25), 1.2, ink);
                c.line(p(0.28, 0.25), p(0.28, 0.78), 1.2, ink);
                c.line(p(0.52, 0.78), p(0.62, 0.78), 1.6, ink);
                c.line(p(0.72, 0.32), p(0.9, 0.32), 1.2, ink);
                c.line(p(0.9, 0.32), p(0.9, 0.78), 1.2, ink);
                c.line(p(0.72, 0.78), p(0.9, 0.78), 1.2, ink);
                c.line(p(0.72, 0.32), p(0.72, 0.78), 1.2, ink);
            }
            Glyph::Sort => {
                // 上下双向箭头（改变行序）
                c.line(p(0.32, 0.2), p(0.32, 0.8), 1.2, ink);
                c.line(p(0.32, 0.2), p(0.2, 0.36), 1.2, ink);
                c.line(p(0.32, 0.2), p(0.44, 0.36), 1.2, ink);
                c.line(p(0.68, 0.8), p(0.68, 0.2), 1.2, ink);
                c.line(p(0.68, 0.8), p(0.56, 0.64), 1.2, ink);
                c.line(p(0.68, 0.8), p(0.8, 0.64), 1.2, ink);
            }
            Glyph::Series => {
                // 递增的三根柱（填充序列）
                c.line(p(0.16, 0.8), p(0.16, 0.56), 2.0, ink);
                c.line(p(0.42, 0.8), p(0.42, 0.36), 2.0, ink);
                c.line(p(0.68, 0.8), p(0.68, 0.16), 2.0, ink);
                c.line(p(0.08, 0.8), p(0.92, 0.8), 1.0, ink);
            }
            Glyph::Dice => {
                // 骰子：圆角方框 + 五点（1、3、5 的经典排布，一眼认得出）
                c.line(p(0.16, 0.16), p(0.84, 0.16), 1.2, ink);
                c.line(p(0.84, 0.16), p(0.84, 0.84), 1.2, ink);
                c.line(p(0.84, 0.84), p(0.16, 0.84), 1.2, ink);
                c.line(p(0.16, 0.84), p(0.16, 0.16), 1.2, ink);
                // 点用极短的粗线段画：只依赖已确认存在的 line，不赌 canvas 有没有 dot/circle
                c.line(p(0.28, 0.32), p(0.36, 0.32), 2.2, ink);
                c.line(p(0.64, 0.32), p(0.72, 0.32), 2.2, ink);
                c.line(p(0.46, 0.5), p(0.54, 0.5), 2.2, ink);
                c.line(p(0.28, 0.68), p(0.36, 0.68), 2.2, ink);
                c.line(p(0.64, 0.68), p(0.72, 0.68), 2.2, ink);
            }
            Glyph::Exclude => {
                // 圆圈加斜杠（排除）
                c.line(p(0.5, 0.14), p(0.86, 0.5), 1.0, ink);
                c.line(p(0.86, 0.5), p(0.5, 0.86), 1.0, ink);
                c.line(p(0.5, 0.86), p(0.14, 0.5), 1.0, ink);
                c.line(p(0.14, 0.5), p(0.5, 0.14), 1.0, ink);
                c.line(p(0.22, 0.78), p(0.78, 0.22), 1.2, ink);
            }
        }
    })
    .size(size, size)
}

pub fn glyph<M: Clone + 'static>(g: Glyph) -> Element<M> {
    glyph_at(g, 16.0)
}

/// 实心三角（导航器折叠箭头 ▼/▶）。用 SVG path 填充，任意缩放下边角清晰。
pub fn triangle<M: Clone + 'static>(open: bool, size: f32) -> Element<M> {
    canvas(move |c, r| {
        let d = if open {
            "M 3 5 L 13 5 L 8 12 Z"
        } else {
            "M 5 3 L 12 8 L 5 13 Z"
        };
        c.svg_path(d, 16.0, r, hex(ink()), None);
    })
    .size(size, size)
}

/// 文件夹剪影（导航器文件夹图标）。
pub fn folder_icon<M: Clone + 'static>(size: f32) -> Element<M> {
    canvas(move |c, r| {
        c.svg_path(
            "M 2 4 L 6 4 L 7.5 5.5 L 14 5.5 L 14 13 L 2 13 Z",
            16.0,
            r,
            hex("#8A8F99"),
            None,
        );
    })
    .size(size, size)
}

// ── 工具栏按钮 ──────────────────────────────────────────────────────────────
/// 析光 工具栏按钮：等大方块、平铺、悬停浅蓝、按下更深的蓝。
pub fn tool_button(g: Glyph, tip: &'static str, msg: Msg, pressed: bool) -> Element<Msg> {
    div()
        .size(24.0, 24.0)
        .rounded(r_sm())
        .when(pressed, |e| e.bg(hex(tool_pressed())))
        .hover(|s| s.bg(hex(tool_hover())))
        .on_click(msg)
        .tooltip(tip)
        .center()
        .child(glyph(g))
        // 悬停/按下的底色过渡：走主题的标准曲线
        .transition(motion::secs(motion::D_CONTROL))
        .easing(motion::std_easing())
}

/// 带下拉箭头的复合按钮（析光的 New / Analysis 等组）。
#[allow(dead_code)]
pub fn tool_button_dropdown(g: Glyph, tip: &'static str, msg: Msg) -> Element<Msg> {
    div()
        .rounded(r_sm())
        .hover(|s| s.bg(hex(tool_hover())))
        .on_click(msg)
        .tooltip(tip)
        .transition(motion::secs(motion::D_CONTROL))
        .easing(motion::std_easing())
        .child(
            row()
                .gap(0.0)
                .items_center()
                .child(div().size(22.0, 24.0).center().child(glyph(g)))
                .child(div().size(10.0, 24.0).center().child(glyph_at(Glyph::Chevron, 8.0))),
        )
}

/// 工具栏分组：上排图标按钮 + 下排组名（析光 ribbon 的经典排布）。
#[allow(dead_code)]
pub fn tool_group(name: &'static str, buttons: Vec<Element<Msg>>, dropdown: bool) -> Element<Msg> {
    col()
        .px(6.0)
        .gap(2.0)
        .child(
            buttons
                .into_iter()
                .fold(row().gap(3.0).items_center(), |r, b| r.child(b)),
        )
        .child(
            row()
                .gap(1.0)
                .items_center()
                .child(
                    text(name)
                        .font_size(ui_font_sm())
                        .color(hex(dim()))
                        .nowrap(),
                )
                .child_if(dropdown, || glyph_at(Glyph::Chevron, 7.0)),
        )
}

// ── 侧栏分区标题 ────────────────────────────────────────────────────────────
/// 检查器 / 导航器中的分区标题（析光 对话框里的分组标签风格）。
pub fn section(title: &str) -> Element<Msg> {
    col()
        .gap(6.0)
        .w_full()
        .pt(2.0)
        .child(section_text(title))
        .child(div().w_full().h(1.0).bg(hex(hair())))
}

/// 一个「标签 + 控件」行，用于检查器。
pub fn prop_row(label: &str, control: Element<Msg>) -> Element<Msg> {
    row()
        .gap(6.0)
        .w_full()
        .items_center()
        .child(text(label).font_size(ui_font_sm()).color(hex(dim())).w(84.0))
        // 控件自身的样式变化（下拉框底色、输入框描边…）走主题标准过渡
        .child(
            control
                .transition(motion::secs(motion::D_CONTROL))
                .easing(motion::std_easing()),
        )
}

/// 自绘复选框（析光 对话框里的方框 + 勾），带**弹性勾选**。
///
/// # 物理感怎么来的
/// 方框的填色/描边交给框架：`.bg()` / `.border()` 是样式属性，
/// 配 `.transition()` + `.easing()` 由 runtime 平滑过渡（勾选 0.4 s OutBack、
/// 取消 0.15 s OutCubic —— 出现有过冲、消失干脆）。
///
/// 对勾的**缩放过冲**（0.8 → 1.10 → 1.0）框架做不到：`StylePatch` 没有 `scale`。
/// 所以它由 canvas 按 `motion::spring()` 给出的数值自己画。
pub fn check(checked: bool, label: &str, msg: Msg) -> Element<Msg> {
    let v = motion::spring(motion::key(label), checked);
    let s = motion::tick_scale(v);

    // 方框：元素样式（可过渡）
    let bx = div()
        .size(13.0, 13.0)
        .center()
        .rounded(3.0)
        .border(1.0, hex(if checked { accent() } else { "#C4CAD3" }))
        .bg(hex(if checked { accent() } else { canvas_bg() }))
        .transition(motion::secs(if checked {
            motion::D_CHECK_ON
        } else {
            motion::D_CHECK_OFF
        }))
        .easing(if checked {
            motion::OUT_BACK
        } else {
            motion::OUT_CUBIC
        })
        .child(
            // 对勾：画布自绘，缩放由弹性时钟驱动（框架没有 scale）
            canvas(move |c, r| {
                let (x, y, w, h) = (r.origin().x, r.origin().y, r.w, r.h);
                let cx = x + w * 0.5;
                let cy = y + h * 0.5;
                // 以中心为原点按 s 缩放（s > 1 即过冲）
                let p = |dx: f32, dy: f32| Point::new(cx + dx * w * s, cy + dy * h * s);
                // 与原来一致：强调色底上用画布底色画勾（对比度最稳）
                let tick = hex(canvas_bg());
                c.line(p(-0.26, 0.02), p(-0.06, 0.24), 1.6, tick);
                c.line(p(-0.06, 0.24), p(0.28, -0.22), 1.6, tick);
            })
            .size(11.0, 11.0)
            // 淡入交给框架：样式过渡由 runtime 插值，即使某一帧没有重建
            // 视图也能推进；缩放（框架做不到）才由弹性时钟驱动。
            .opacity(if checked { 1.0 } else { 0.0 })
            .transition(motion::secs(if checked {
                motion::D_CHECK_ON
            } else {
                motion::D_CHECK_OFF
            }))
            .easing(if checked {
                motion::OUT_BACK
            } else {
                motion::OUT_CUBIC
            }),
        );

    row()
        .gap(7.0)
        .items_center()
        .py(1.0)
        .on_click(msg)
        .hover(|s| s.bg(hex(accent_soft())))
        .transition(motion::secs(motion::D_CONTROL))
        .easing(motion::std_easing())
        .child(bx)
        .child(text(label).font_size(ui_font()).color(hex(ink())))
}

/// 对话框外壳：**整个对话框只有一个背景矩形**。
///
/// 之前是 `modal(...)` 外面再套 `.bg().rounded().shadow()`，于是出现两个矩形：
/// `modal` 内部自带的那块是方的，也不吃外层的圆角与位移，
/// 表现成「弹窗背后多了一个莫名其妙的方形，而且和弹窗不在同一层」。
///
/// 这里用基础容器自己搭（标题条 / 内容 / 按钮条），
/// 圆角、投影、位移动画都作用在同一个矩形上，不会再有第二块。
#[allow(clippy::too_many_arguments)]
pub fn dialog_panel(
    title: String,
    content: Element<Msg>,
    buttons: Vec<Element<Msg>>,
    w: f32,
    max_h: f32,
    radius: f32,
    dy: f32,
    on_close: Msg,
) -> Element<Msg> {
    // 标题条
    let head = row()
        .w_full()
        .items_center()
        .gap(8.0)
        .px(14.0)
        .py(10.0)
        .child(text(title).font_size(ui_font()).semibold().color(hex(ink())))
        .child(spacer())
        .child(
            div()
                .size(22.0, 22.0)
                .center()
                .rounded(r_sm())
                .hover(|s| s.bg(hex(tool_hover())))
                .on_click(on_close)
                .child(text("×".to_string()).font_size(ui_font()).color(hex(dim()))),
        );

    // 按钮条：整体右对齐（与 Prism 对话框一致）
    let mut foot = row().w_full().items_center().gap(8.0).px(14.0).py(10.0);
    foot = foot.child(spacer());
    for b in buttons {
        foot = foot.child(b);
    }

    col()
        .gap(0.0)
        .w(w)
        .max_h(max_h)
        .bg(hex(canvas_bg()))
        .rounded(radius)
        .shadow(shadow_pop())
        .mt(dy)
        .scroll_y()
        .child(head)
        .child(hsep())
        .child(div().w_full().px(14.0).py(10.0).child(content))
        .child(hsep())
        .child(foot)
}

/// 自绘单选点（析光 对话框里的圆点选项）。
pub fn radio(active: bool, label: &str, msg: Msg) -> Element<Msg> {
    // 与复选框同一套弹性时钟：选中时圆心从 0 弹到 0.22（过冲到 0.24）再回落。
    // 选中态给一个下限，避免「某一帧没重绘」时看起来选了却没点。
    let v = motion::spring(motion::key(label), active);
    let v = if active { v.max(0.3) } else { v };
    row()
        .gap(5.0)
        .items_center()
        .py(1.0)
        .on_click(msg)
        .hover(|s| s.bg(hex(accent_soft())))
        .transition(motion::secs(motion::D_CONTROL))
        .easing(motion::std_easing())
        .child(
            canvas(move |c, r| {
                // 未选中：冷灰细环；选中：强调蓝环 + 蓝心
                let (ring, core) = if active {
                    (hex(accent()), hex(accent()))
                } else {
                    (hex("#C4CAD3"), hex(canvas_bg()))
                };
                c.fill_circle(r.center(), r.w * 0.5, ring);
                c.fill_circle(r.center(), r.w * 0.36, hex(canvas_bg()));
                if v > 0.001 {
                    // 半径按弹性进度缩放 —— 这就是「点下去弹出来」的那一下
                    c.fill_circle(r.center(), r.w * 0.22 * v, core);
                }
            })
            .size(12.0, 12.0),
        )
        .child(text(label).font_size(ui_font()).color(hex(ink())))
}
