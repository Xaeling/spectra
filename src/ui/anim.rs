//! 弹窗的「形态渐变」（metamorphosis）。
//!
//! 视觉上一个圆角矩形从**按钮里凝结出的一滴水珠**成长为弹窗长条：
//! 位置、尺寸、圆角半径三者同时插值，圆角在过程中保持圆滑。
//!
//! # 三个被同时驱动的量
//! - **尺寸**：从 12% 长到 100%（起始足够小，读起来就是一个点）
//! - **圆角**：起始 = min(宽,高)/2 → 视觉上是**正圆**；结束时收敛到 `r_lg()`
//!   （「方形一开始不存在」靠的就是这个：圆角半径 = 边长一半即圆点）
//! - **位置**：随进度轻微上浮，配合缩放产生「铺开」的方向感
//!
//! # 缓动
//! 用 ease_out_cubic（末端减速）+ 极小的过冲，模拟表面张力被克服后稳定下来。
//! 过冲幅度刻意压得很小，避免看起来在弹跳。

use charis_ui::prelude::*;
use std::time::Instant;

/// 动画时长（毫秒）。150–200ms 是这类形变最舒服的区间。
#[allow(dead_code)]
pub const MORPH_MS: f32 = 190.0;
/// 总开关：某些环境下每帧重绘不可靠（动画会卡在起始的极小状态），
/// 关掉它就是直接显示最终形态。设置窗口的「动画」项改的是它。
static ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
static MS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(190);

pub fn set_enabled(on: bool) {
    ENABLED.store(on, std::sync::atomic::Ordering::Relaxed);
}
pub fn set_ms(ms: usize) {
    MS.store(ms.min(800) as u32, std::sync::atomic::Ordering::Relaxed);
}
pub fn enabled() -> bool {
    ENABLED.load(std::sync::atomic::Ordering::Relaxed)
}
fn dur_ms() -> f32 {
    let v = MS.load(std::sync::atomic::Ordering::Relaxed);
    if v == 0 { 0.001 } else { v as f32 }
}
/// 起始尺寸比例（已不再用于缩放，保留供将来做真正的 scale 变换时用）
#[allow(dead_code)]
const FROM: f32 = 0.12;

/// ease_out_cubic：末端明显减速。
pub fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t)
}

/// 从打开时刻算进度（0 → 1）。
pub fn progress(open_at: Instant) -> f32 {
    if !enabled() {
        return 1.0;
    }
    let ms = open_at.elapsed().as_secs_f32() * 1000.0;
    (ms / dur_ms()).clamp(0.0, 1.0)
}

/// 一次算好弹窗的动画量：`(宽, 高, 圆角, 上移量, 是否已完成)`。
///
/// # 尺寸为什么现在敢真的插值了
/// 最早的版本让容器从 12% 长到 100%，但内容（固定宽输入区、按钮行、
/// 预览缩略图）按自然尺寸排布，容器一被压小内容就**溢出到容器外**——
/// 表现正是「弹窗凸出来一截」「按键跑到弹窗范围之外」。
///
/// 解决办法是**让载体和内容分离**：
/// - 动画**未完成**时，调用方只渲染一个**空的圆角矩形**（载体），
///   尺寸/圆角/位置三个量同时插值。因为此时根本没渲染内容，
///   容器压到多小都不会挤爆任何东西。
/// - 动画**完成**后（`done == true`），才渲染完整内容。
///
/// 观感上就是：水珠铺开 → 内容浮现。动画只有 `MORPH_MS` 毫秒，
/// 切换发生在最后一帧，人眼看不出接缝。
pub fn morph(open_at: Instant, w: f32, h: f32) -> (f32, f32, f32, f32, bool) {
    // 原始进度（判断「是否结束」要用它，不能用缓动后的值，
    // 否则 ease 会把 0.999 也抬到接近 1，导致尺寸还没满就提前切内容）
    let raw = progress(open_at);
    let done = raw >= 1.0;
    let p = ease(raw);

    // ── 量 1 / 2：尺寸 —— **不再缩放** ──
    //
    // 之前让容器从 12% 长到 100%，代价是内容必须等动画结束才能渲染
    // （按自然尺寸排的内容在小容器里会溢出）。用户看到的就是
    // 「先弹一个白框，再弹出真正的弹窗」——那块白框正是空的载体。
    //
    // 与其留一个必须先空一段的形状，不如让内容从头就在：尺寸恒定，
    // 形变改由圆角和位移承担。
    let cw = w;
    let ch = h;

    // ── 量 3：圆角。起始比常规值更圆（接近胶囊），收敛到常规圆角 ──
    // 起始半径取常规值的 2.5 倍而不是「短边一半」：后者在小尺寸下才成立，
    // 尺寸既然不缩了，取一半会超过短边、把四角削掉一大块。
    let r_from = crate::palette::r_lg() * 2.5;
    let r_to = crate::palette::r_lg();
    let r = r_from + (r_to - r_from) * p;

    // ── 量 4：位置。从下方 +18px 上浮回 0，配合缩放产生铺开的方向感 ──
    let dy = 18.0 * (1.0 - p);

    (cw, ch, r, dy, done)
}

/// 动画未完成时用的「形态载体」：一个空的圆角矩形。
///
/// 只画形状和底色，不放任何内容 —— 这样尺寸可以任意缩放而不挤压内容。
#[allow(dead_code)]
pub fn carrier(w: f32, h: f32, r: f32, dy: f32) -> Element<crate::app::Msg> {
    col()
        .w(w)
        .h(h)
        .bg(hex(crate::palette::canvas_bg()))
        .rounded(r)
        .shadow(crate::ui::shadow_pop())
        .mt(dy)
}

/// 只取圆角（不想改动容器尺寸时用这个）。
#[allow(dead_code)]
pub fn morph_radius(open_at: Instant, w: f32, h: f32) -> f32 {
    morph(open_at, w, h).2
}

/// 只取上浮位移。
#[allow(dead_code)]
pub fn morph_dy(open_at: Instant, w: f32, h: f32) -> f32 {
    morph(open_at, w, h).3
}

/// 只取「是否已完成」。
#[allow(dead_code)]
pub fn morph_done(open_at: Instant, w: f32, h: f32) -> bool {
    morph(open_at, w, h).4
}
