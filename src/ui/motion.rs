//! 物理感动效（Charis 路线）。
//!
//! # 为什么不用 Animator
//! Makepad 的 `Animator` / `live_design!` / `OutBack` 都属于 Makepad 自己那套
//! Live DSL + Widget 体系，Charis 里**没有**这些（它是 taffy + Element 树 + Msg，
//! 动画走 CSS 式三件套）。所以这里全部用 Charis 真实存在的 API：
//!
//! - `.transition(secs)` —— 交互态与样式变化过渡（`Style.transition`，默认 0 = 没动画）
//! - `.easing(Easing)` —— 时序函数（`Style.easing`，默认 `Easing::Standard`）
//! - `.animate_layout(secs)` —— FLIP 式位置/尺寸动画（`Style.layout_transition`）
//!
//! # 过冲怎么来的
//! Charis 的 `Easing` 只有 6 个变体，**没有 OutBack / OutElastic / OutBounce**。
//! 唯一能做出过冲的入口是 `Easing::CubicBezier(x1, y1, x2, y2)`：
//! **x 分量是时间轴，必须 ∈ [0,1]；y 分量是值轴，可以 > 1**，过冲就靠它。
//!
//! `cubic-bezier(0.34, 1.56, 0.64, 1.0)` 实测峰值 1.0978 @ t≈0.573，
//! 与 Penner easeOutBack（1.1000 @ t=0.5801）几乎重合，用它复刻 OutBack。
//!
//! # 消失类动作不做过冲
//! 出现/确认类用 OUT_BACK（过冲给「到位」的确认感）；
//! 消失/取消类用 OUT_CUBIC（用户期望迅速完成，弹性在这里是拖沓）。
//! 面板展开刻意**不用** OutElastic —— 多次震荡会让专业工具显得轻浮。

use charis_ui::prelude::*;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;
use std::time::{Duration, Instant};

// ── 缓动 ────────────────────────────────────────────────────────────────────
/// OutBack 复刻（Penner c1 = 1.70158）：峰值 1.0978 @ t≈0.573。
pub const OUT_BACK: Easing = Easing::CubicBezier(0.34, 1.56, 0.64, 1.0);
/// 弱过冲（更收敛）：峰值 1.0869 @ t≈0.634，大面板用它更稳。
#[allow(dead_code)]
pub const OUT_BACK_SOFT: Easing = Easing::CubicBezier(0.175, 0.885, 0.32, 1.275);
/// 收起 / 取消：快起慢收，无过冲。
pub const OUT_CUBIC: Easing = Easing::EaseOutCubic;
/// 可逆交互：进出对称。
#[allow(dead_code)]
pub const IN_OUT_CUBIC: Easing = Easing::EaseInOutCubic;

// ── 基准时长（秒）────────────────────────────────────────────────────────────
/// 勾选：0.4 s。峰值出现在 0.4 × 0.58 ≈ 232 ms。
pub const D_CHECK_ON: f32 = 0.40;
/// 取消勾选：0.15 s，干脆。
pub const D_CHECK_OFF: f32 = 0.15;
/// 浮层（菜单 / 面板）展开：0.35 s。
pub const D_PANEL_IN: f32 = 0.35;
/// 浮层收起：0.2 s。
#[allow(dead_code)]
pub const D_PANEL_OUT: f32 = 0.20;
/// 单个菜单项的入场时长。
pub const D_ITEM: f32 = 0.25;
/// 菜单项之间的 stagger 间隔。
pub const D_STAGGER: f32 = 0.06;
/// 普通控件（悬停 / 按下 / 颜色变化）。
pub const D_CONTROL: f32 = 0.15;

/// 主题里「标准交互过渡」的标定基准；我们的常量都按它折算。
const BASE_UNIT: f32 = 0.15;

/// 动效总开关（设置窗口的「动画」项驱动它，等价于系统的「减少动效」）。
pub fn enabled() -> bool {
    crate::ui::anim::enabled()
}

/// 主题 token 缓存：`(标准过渡时长的缩放系数, 标准缓动)`。
///
/// `theme()` 每次调用都要走一遍主题解析，而 `check()` / `prop_row()`
/// 这类控件一帧里要建几十上百个，逐个去问主题会把时间浪费在重复取值上
/// —— 这两个值在一个主题下是常量，取一次就够。
static THEME_TOKENS: Mutex<Option<(f32, Easing)>> = Mutex::new(None);

fn tokens() -> (f32, Easing) {
    if let Ok(g) = THEME_TOKENS.lock() {
        if let Some(v) = *g {
            return v;
        }
    }
    let th = charis_ui::theme();
    let u = th.transition;
    let k = if u > 0.0 {
        (u / BASE_UNIT).clamp(0.5, 2.0)
    } else {
        1.0
    };
    let v = (k, th.easing);
    if let Ok(mut g) = THEME_TOKENS.lock() {
        *g = Some(v);
    }
    v
}

/// 主题缩放后的实际时长。
///
/// 时长不写死：以 `theme().transition` 与 0.15 s 的比值整体缩放，
/// 这样换主题时动效跟着主题的节奏走。**返回 0 等于没有过渡**，
/// 关掉动效时正是靠这一点彻底静止。
pub fn secs(base: f32) -> f32 {
    if !enabled() {
        return 0.0;
    }
    base * tokens().0
}

/// 参与除法前用的安全时长（避免关动效时为 0 导致除零）。
fn dur(base: f32) -> f32 {
    let v = secs(base);
    if v > 0.0 {
        v
    } else {
        0.0001
    }
}

/// 标准交互过渡的缓动：取主题 token，不硬编码。
pub fn std_easing() -> Easing {
    tokens().1
}

// ── 弹性时钟 ────────────────────────────────────────────────────────────────
/// 一次勾选的状态：`on` 是目标态，`at` 是**最后一次状态变化**的时刻。
///
/// 记录「变化时刻」而不是「起始值」是关键：连续快速点击时，
/// 动画从当前时间重新起算、方向平滑反转，不会跳帧，
/// 最终状态也一定与点击次数的奇偶一致。
#[derive(Clone, Copy)]
struct Spring {
    on: bool,
    at: Instant,
}

/// key = 勾选框标签的哈希。
///
/// 同一标签在两处同时出现时会共用一个时钟（极其罕见，
/// 且后果只是两个框一起动一下），换来的是不需要给 `check()` / `radio()`
/// 增加 id 参数、也就不用改动几十处调用点。
static SPRINGS: Mutex<Option<HashMap<u64, Spring>>> = Mutex::new(None);

fn springs() -> std::sync::MutexGuard<'static, Option<HashMap<u64, Spring>>> {
    SPRINGS.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn key(label: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    label.hash(&mut h);
    h.finish()
}

/// 勾选进度 `v`：
/// - `0` 完全未选中
/// - `1` 完全选中
/// - **`> 1` 过冲中**（峰值 ≈ 1.098，出现在 58% 处）
///
/// # 为什么必须自己算
/// `StylePatch` **没有 `scale`**（只有 bg / border_color / color / opacity /
/// rounded / shadow / translate / cursor），所以「0.8 → 冲到 1.10 → 回落 1.0」
/// 这种缩放过冲没法交给框架做，只能由 canvas 按这里的数值自己画。
pub fn spring(k: u64, on: bool) -> f32 {
    if !enabled() {
        return if on { 1.0 } else { 0.0 };
    }
    let now = Instant::now();
    // 首次出现：直接给一个早就结束的时刻，免得一进界面就全体弹一遍。
    let settled = Spring {
        on,
        at: now - Duration::from_secs(5),
    };
    let mut g = springs();
    let map = g.get_or_insert_with(HashMap::new);
    let s = *map.get(&k).unwrap_or(&settled);
    if s.on != on {
        // 状态刚变：从本帧起算（on 从 0 开始，off 从 1 开始）
        map.insert(k, Spring { on, at: now });
        return if on { 0.0 } else { 1.0 };
    }
    if !map.contains_key(&k) {
        map.insert(k, settled);
    }
    let el = now.duration_since(s.at).as_secs_f32();
    if on {
        OUT_BACK.apply((el / dur(D_CHECK_ON)).clamp(0.0, 1.0))
    } else {
        1.0 - OUT_CUBIC.apply((el / dur(D_CHECK_OFF)).clamp(0.0, 1.0))
    }
}

/// 勾选标记的缩放：0.8 → 1.10 → 1.0（相对目标值过冲 110%）。
pub fn tick_scale(v: f32) -> f32 {
    0.8 + 0.2 * v
}

/// 透明度（过冲部分截断，1 就是完全不透明）。
#[allow(dead_code)]
pub fn fade(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

// ── 浮层入场 ────────────────────────────────────────────────────────────────
/// 浮层（右键菜单 / 功能区菜单）入场的 `(不透明度, 纵向位移)`。
///
/// 位移在**绘制时**做（translate 不参与布局），所以展开过程不会触发逐帧重排、
/// 周围元素也不会被一帧帧地挤动 —— 这正是文档要求用 `animate_layout` 而非
/// 每帧改 height 的原因。过冲让面板「落到位」再回稳，即重量感的来源。
pub fn panel_enter(opened_at: Instant) -> (f32, f32) {
    if !enabled() {
        return (1.0, 0.0);
    }
    let el = opened_at.elapsed().as_secs_f32();
    let v = OUT_BACK.apply((el / dur(D_PANEL_IN)).clamp(0.0, 1.0));
    (v.clamp(0.0, 1.0), (1.0 - v) * -8.0)
}

/// 菜单项的第 `i` 项入场 `(不透明度, 横向位移)`：从左侧 8px 处滑入。
///
/// 逐项延迟 = `i × 0.06 s`，单项 0.25 s —— Charis 没有 transition-delay，
/// 这里按文档给的近似做法：用「已过时间 − 延迟」自己算进度。
pub fn item_enter(opened_at: Instant, i: usize) -> (f32, f32) {
    if !enabled() {
        return (1.0, 0.0);
    }
    let el = opened_at.elapsed().as_secs_f32() - i as f32 * D_STAGGER;
    if el <= 0.0 {
        return (0.0, -8.0);
    }
    let v = OUT_CUBIC.apply((el / dur(D_ITEM)).clamp(0.0, 1.0));
    (v, -8.0 * (1.0 - v))
}

/// FLIP 布局动画的时长（面板尺寸/位置变化时用，子元素跟随）。
pub fn layout_secs() -> f32 {
    secs(D_PANEL_IN)
}
