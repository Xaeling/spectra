//! 内置字体：全部打包进二进制，运行时不依赖系统字体。
//!
//! | 用途 | 字体 | 文件 |
//! |---|---|---|
//! | **界面中文** | MiSans | `assets/fonts/MiSans-Regular.ttf` |
//! | **导出中文** | 宋体 SimSun | `assets/fonts/SimSun.ttf` |
//! | **导出英文** | Times New Roman | `assets/fonts/TimesNewRoman.ttf` |
//!
//! # 为什么导出不用界面字体
//! 期刊投稿的通行规范是「中文宋体 + 英文 Times New Roman」。
//! 因此图表文字按**字符**判断：含 CJK 的连续段用宋体，其余用 Times New Roman，
//! 一段文字里中英混排也能各自用对的字体（例如 `浓度 (mol/L)`）。
//!
//! # plotters 的 ab_glyph
//! 开启 `ab_glyph` 后 plotters **不再加载系统字体**，必须先用
//! `register_font` 注册才能渲染文字。好处是：同一份代码在 Windows / macOS /
//! Linux 上导出的图形完全一致，不会因为缺字体而跑版。

#![allow(dead_code)]

use plotters::style::{register_font, FontStyle};
use std::sync::Once;

// ── 打包进二进制的字体数据 ──────────────────────────────────────────────────
pub const MI_SANS: &[u8] = include_bytes!("../assets/fonts/MiSans-Regular.ttf");
pub const SIMSUN: &[u8] = include_bytes!("../assets/fonts/SimSun.ttf");
pub const TIMES: &[u8] = include_bytes!("../assets/fonts/TimesNewRoman.ttf");

/// 界面字体族名（Mi Sans 在 name 表里登记的族名是 `MiSans`）。
pub const UI_FAMILY: &str = "MiSans";
/// 导出用的中文字体（宋体）。
pub const CJK_FAMILY: &str = "SimSun";
/// 导出用的英文字体。
pub const LATIN_FAMILY: &str = "Times New Roman";

/// 是否把界面字体强制设为 Mi Sans。
///
/// 若你的系统里 Mi Sans 不可见（charis 只认系统已安装的字体），
/// 界面会出现方块字；把这里改成 `false` 即回到系统默认 UI 字体。
/// 也可以直接双击 `assets/fonts/MiSans-Regular.ttf` 安装到系统。
pub const APPLY_UI_FONT: bool = true;

static INIT: Once = Once::new();

/// 注册字体。进程内只做一次；`main` 启动时调用。
pub fn register() {
    INIT.call_once(|| {
        // ab_glyph 模式下这三个是全部可用字体，缺一个都不会渲染
        let _ = register_font(CJK_FAMILY, FontStyle::Normal, SIMSUN);
        let _ = register_font(LATIN_FAMILY, FontStyle::Normal, TIMES);
        let _ = register_font(UI_FAMILY, FontStyle::Normal, MI_SANS);
        // 斜体：没有单独的斜体字库，注册同一份数据让斜体查找有结果。
        // plotters 会回退到 Normal 字形（正体），量符号斜体的视觉另由
        // plot.rs 的栅格化 + 倾斜贴回实现（见 blit 的 shear 参数）。
        let _ = register_font(CJK_FAMILY, FontStyle::Italic, SIMSUN);
        let _ = register_font(LATIN_FAMILY, FontStyle::Italic, TIMES);
        install_ui_font();
    });
}

// ── CJK 判定 ────────────────────────────────────────────────────────────────

/// 是否为 CJK / 全角字符（中日韩汉字、假名、谚文、中文标点、全角 ASCII）。
pub fn is_cjk(c: char) -> bool {
    let u = c as u32;
    (0x3400..=0x4DBF).contains(&u)  // CJK 扩展 A
        || (0x4E00..=0x9FFF).contains(&u)  // CJK 基本区
        || (0xF900..=0xFAFF).contains(&u)  // 兼容汉字
        || (0x3000..=0x303F).contains(&u)  // 中文标点
        || (0x3040..=0x30FF).contains(&u)  // 日文假名
        || (0xAC00..=0xD7AF).contains(&u)  // 韩文
        || (0xFF00..=0xFF60).contains(&u)  // 全角 ASCII / 标点
        || (0xFFE0..=0xFFE6).contains(&u)  // 全角符号
}

pub fn has_cjk(s: &str) -> bool {
    s.chars().any(is_cjk)
}

/// 按「含 CJK / 不含 CJK」选字体：中文宋体，其余 Times New Roman。
pub fn font_for(s: &str) -> &'static str {
    if has_cjk(s) {
        CJK_FAMILY
    } else {
        LATIN_FAMILY
    }
}

/// 把一段文字切成连续的同字体片段，每段带上斜体标记。
///
/// 例如 `浓度 (mol/L)` 切成 `["浓度" 宋体, " (" 英文, "mol/L" 英文, ")" 英文]`，
/// 这样中英混排时各自用对的字体，不会出现中文用 Times New Roman 变方框。
pub fn runs(s: &str, italic: bool) -> Vec<(String, bool)> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_cjk: Option<bool> = None;
    for c in s.chars() {
        let cjk = is_cjk(c);
        match cur_cjk {
            None => {
                cur_cjk = Some(cjk);
                cur.push(c);
            }
            Some(v) if v == cjk => cur.push(c),
            Some(_) => {
                out.push(std::mem::take(&mut cur));
                cur_cjk = Some(cjk);
                cur.push(c);
            }
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out.into_iter().map(|t| (t, italic)).collect()
}

// ── 界面字体：把 Mi Sans 挂进进程字体表 ────────────────────────────────────

/// Windows：`AddFontMemResourceEx` 把内存中的字体注册为进程私有字体。
/// 返回是否成功。非 Windows 平台目前不做处理（依赖系统已安装）。
fn install_ui_font() -> bool {
    #[cfg(windows)]
    {
        unsafe {
            let mut n: u32 = 0;
            let h = AddFontMemResourceEx(
                MI_SANS.as_ptr() as *const std::ffi::c_void,
                MI_SANS.len() as u32,
                std::ptr::null_mut(),
                &mut n as *mut u32,
            );
            !h.is_null()
        }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
extern "system" {
    fn AddFontMemResourceEx(
        pb_font: *const std::ffi::c_void,
        cb_font: u32,
        pdv: *mut std::ffi::c_void,
        pc_fonts: *mut u32,
    ) -> *mut std::ffi::c_void;
}
