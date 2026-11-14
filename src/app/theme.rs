//! UI 设计系统：颜色、字号、间距与 egui 样式。
//!
//! 所有视觉常量集中在这里；改一处，全站跟着变。

use std::sync::Arc;

use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke, Style,
    TextStyle, Vec2, Visuals,
};

use crate::plot::text::SANS_BYTES;

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub panel: Color32,
    pub panel_alt: Color32,
    pub sidebar: Color32,
    pub border: Color32,
    pub border_soft: Color32,
    pub ink: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub accent_ink: Color32,
    pub hover: Color32,
    pub grid_line: Color32,
    pub danger: Color32,
    /// 警告：非阻断性问题（如「建议改用 Fisher」）。
    pub warn: Color32,
    pub success: Color32,
    pub shadow: Color32,
}

pub fn light() -> Palette {
    Palette {
        dark: false,
        bg: Color32::from_rgb(0xF4, 0xF6, 0xF8),
        panel: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        panel_alt: Color32::from_rgb(0xFA, 0xFB, 0xFC),
        sidebar: Color32::from_rgb(0xFA, 0xFB, 0xFC),
        border: Color32::from_rgb(0xE2, 0xE6, 0xEC),
        border_soft: Color32::from_rgb(0xED, 0xF0, 0xF4),
        ink: Color32::from_rgb(0x1B, 0x24, 0x30),
        muted: Color32::from_rgb(0x5D, 0x69, 0x79),
        faint: Color32::from_rgb(0x79, 0x84, 0x92),
        accent: Color32::from_rgb(0x5B, 0x6C, 0xFF),
        accent_soft: Color32::from_rgb(0xEE, 0xF0, 0xFF),
        accent_ink: Color32::from_rgb(0x3A, 0x49, 0xE0),
        hover: Color32::from_rgb(0xF1, 0xF3, 0xF7),
        grid_line: Color32::from_rgb(0xEC, 0xEF, 0xF3),
        danger: Color32::from_rgb(0xE5, 0x48, 0x4D),
        warn: Color32::from_rgb(0xB4, 0x53, 0x09),
        success: Color32::from_rgb(0x18, 0xA4, 0x7A),
        shadow: Color32::from_rgba_unmultiplied(0x1B, 0x24, 0x30, 18),
    }
}

pub fn dark() -> Palette {
    Palette {
        dark: true,
        bg: Color32::from_rgb(0x0E, 0x11, 0x15),
        panel: Color32::from_rgb(0x16, 0x1A, 0x21),
        panel_alt: Color32::from_rgb(0x1A, 0x1F, 0x27),
        sidebar: Color32::from_rgb(0x12, 0x16, 0x1B),
        border: Color32::from_rgb(0x28, 0x2E, 0x38),
        border_soft: Color32::from_rgb(0x21, 0x26, 0x2E),
        ink: Color32::from_rgb(0xE9, 0xED, 0xF2),
        muted: Color32::from_rgb(0x93, 0x9E, 0xAC),
        faint: Color32::from_rgb(0x8C, 0x97, 0xA6),
        accent: Color32::from_rgb(0x7C, 0x8B, 0xFF),
        accent_soft: Color32::from_rgb(0x24, 0x2A, 0x44),
        accent_ink: Color32::from_rgb(0xA9, 0xB4, 0xFF),
        hover: Color32::from_rgb(0x1D, 0x23, 0x2B),
        grid_line: Color32::from_rgb(0x22, 0x28, 0x31),
        danger: Color32::from_rgb(0xF2, 0x6D, 0x72),
        warn: Color32::from_rgb(0xF0, 0xB4, 0x29),
        success: Color32::from_rgb(0x3E, 0xC9, 0x9B),
        shadow: Color32::from_rgba_unmultiplied(0, 0, 0, 90),
    }
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "misans".to_owned(),
        Arc::new(FontData::from_owned(SANS_BYTES.to_vec())),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "misans".to_owned());
    // 等宽族保留 Hack（数字/代码），再挂 MiSans 兜底中文。
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .push("misans".to_owned());
    ctx.set_fonts(fonts);
}

fn style_for(palette: &Palette) -> Style {
    let mut style = Style::default();
    style.animation_time = 0.16;
    style.spacing.item_spacing = Vec2::new(8.0, metric::ROW_GAP);
    style.spacing.button_padding = Vec2::new(11.0, 5.0);
    style.spacing.window_margin = Margin::same(metric::WINDOW_PAD as i8);
    style.spacing.menu_margin = Margin::same(6);
    style.spacing.indent = 16.0;
    style.spacing.interact_size.y = metric::CONTROL_H;
    style.spacing.slider_width = 120.0;
    style.spacing.scroll.bar_width = 6.0;
    style.spacing.scroll.floating = true;
    style.spacing.scroll.bar_inner_margin = 3.0;
    style.spacing.scroll.bar_outer_margin = 1.0;

    style.text_styles = [
        (TextStyle::Heading, FontId::proportional(text::DISPLAY)),
        (TextStyle::Body, FontId::proportional(text::BODY)),
        (TextStyle::Button, FontId::proportional(text::BODY)),
        (TextStyle::Small, FontId::proportional(text::META)),
        (TextStyle::Monospace, FontId::monospace(12.0)),
    ]
    .into();

    let mut v = if palette.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.dark_mode = palette.dark;
    v.override_text_color = Some(palette.ink);
    v.panel_fill = palette.bg;
    v.window_fill = palette.panel;
    v.extreme_bg_color = palette.panel_alt;
    v.faint_bg_color = palette.panel_alt;
    v.window_stroke = Stroke::new(1.0, palette.border);
    v.window_corner_radius = CornerRadius::same(radius::WINDOW as u8);
    v.menu_corner_radius = CornerRadius::same(radius::CARD as u8);
    v.selection.bg_fill = palette.accent_soft;
    v.selection.stroke = Stroke::new(1.0, palette.accent);
    v.hyperlink_color = palette.accent;
    // 滑块：圆手柄 + 已填充轨道，比默认方手柄更轻、更容易看清当前值。
    v.handle_shape = egui::style::HandleShape::Circle;
    v.slider_trailing_fill = true;
    // 弹窗投影：偏移 + 大模糊 + 低透明度，避免「贴纸」感。
    v.window_shadow = egui::epaint::Shadow {
        offset: [0, 10],
        blur: 28,
        spread: 0,
        color: if palette.dark {
            Color32::from_black_alpha(160)
        } else {
            Color32::from_rgba_unmultiplied(0x1B, 0x24, 0x30, 34)
        },
    };
    v.popup_shadow = egui::epaint::Shadow {
        offset: [0, 6],
        blur: 16,
        spread: 0,
        color: if palette.dark {
            Color32::from_black_alpha(140)
        } else {
            Color32::from_rgba_unmultiplied(0x1B, 0x24, 0x30, 26)
        },
    };
    v.warn_fg_color = palette.danger;
    v.error_fg_color = palette.danger;

    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(radius::CONTROL as u8);
        w.expansion = 0.0;
    }
    v.widgets.noninteractive.bg_fill = palette.panel;
    v.widgets.noninteractive.weak_bg_fill = palette.panel;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.border_soft);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.ink);

    v.widgets.inactive.bg_fill = palette.border_soft;
    v.widgets.inactive.weak_bg_fill = palette.panel_alt;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, palette.border);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, palette.ink);

    v.widgets.hovered.bg_fill = palette.border_soft;
    v.widgets.hovered.weak_bg_fill = palette.hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, palette.border);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, palette.ink);

    v.widgets.active.bg_fill = palette.accent_soft;
    v.widgets.active.weak_bg_fill = palette.accent_soft;
    v.widgets.active.bg_stroke = Stroke::new(1.0, palette.accent);
    v.widgets.active.fg_stroke = Stroke::new(1.0, palette.accent_ink);

    v.widgets.open.bg_fill = palette.panel;
    v.widgets.open.weak_bg_fill = palette.panel;
    v.widgets.open.bg_stroke = Stroke::new(1.0, palette.border);

    style.visuals = v;
    style
}

/// 按当前主题偏好生成一套调色板（强调色跟随设置）。
pub fn palette_for(is_dark: bool, accent: usize) -> Palette {
    let mut p = if is_dark { dark() } else { light() };
    if let Some(hex) = crate::app::settings::ACCENTS.get(accent) {
        p.accent = accent_color(hex);
    }
    p
}

fn accent_color(hex: &str) -> Color32 {
    let h = hex.trim_start_matches('#');
    if h.len() >= 6 {
        Color32::from_rgb(
            u8::from_str_radix(&h[0..2], 16).unwrap_or(0),
            u8::from_str_radix(&h[2..4], 16).unwrap_or(0),
            u8::from_str_radix(&h[4..6], 16).unwrap_or(0),
        )
    } else {
        Color32::from_rgb(0x5B, 0x6C, 0xFF)
    }
}

/// 按设置应用主题（浅色/深色/跟随系统）。
///
/// `mode`: 0=浅色, 1=深色, 2=跟随系统。
/// 始终同时安装 Light/Dark 两套自定义样式，再交给 ThemePreference 选择：
/// 跟随系统时 egui 才能按 OS 偏好切换，而不是丢掉 StatAura 的调色板。
pub fn apply_theme_mode(ctx: &egui::Context, mode: usize) {
    let s = crate::app::settings::get();
    ctx.set_style_of(egui::Theme::Light, style_for(&palette_for(false, s.accent)));
    ctx.set_style_of(egui::Theme::Dark, style_for(&palette_for(true, s.accent)));
    ctx.set_theme(match mode {
        0 => egui::ThemePreference::Light,
        1 => egui::ThemePreference::Dark,
        _ => egui::ThemePreference::System,
    });

    a11y_enhance(ctx, s.high_contrast, s.a11y);
    let scale = s.font_scale.clamp(0.85, 1.60);
    if (scale - 1.0).abs() > 0.01 {
        ctx.all_styles_mut(|style| {
            for (_, id) in style.text_styles.iter_mut() {
                id.size *= scale;
            }
            style.spacing.item_spacing *= scale;
            style.spacing.button_padding *= scale;
        });
    }
    let r = s.radius.clamp(0.0, 2.0);
    if (r - 1.0).abs() > 0.01 {
        ctx.all_styles_mut(|style| {
            let cr = |base: u8| -> u8 { (base as f32 * r).round().clamp(0.0, 32.0) as u8 };
            style.visuals.window_corner_radius = egui::CornerRadius::same(cr(radius::WINDOW as u8));
            style.visuals.menu_corner_radius = egui::CornerRadius::same(cr(radius::CARD as u8));
            for w in [
                &mut style.visuals.widgets.noninteractive,
                &mut style.visuals.widgets.inactive,
                &mut style.visuals.widgets.hovered,
                &mut style.visuals.widgets.active,
                &mut style.visuals.widgets.open,
            ] {
                w.corner_radius = egui::CornerRadius::same(cr(radius::CONTROL as u8));
            }
        });
    }
}

/// 常用间距
pub const GAP: f32 = 8.0;
pub const PAD: f32 = 14.0;
pub const TOPBAR_H: f32 = 52.0;
pub const STATUS_H: f32 = 26.0;
pub const SIDEBAR_W: f32 = 236.0;
pub const INSPECTOR_W: f32 = 312.0;

/// 版式度量：全站共用，用来保证「同一片区内只有一条左边缘、一条右边缘」。
pub mod metric {
    /// 面板内边距（侧栏 / 检查器 / 中心区域共用同一套水平留白）。
    pub const PAD_X: f32 = 16.0;
    pub const PAD_Y: f32 = 14.0;
    /// 中央工作区（数据表 / 图表 / 结果）的水平留白，比侧栏略宽。
    pub const CONTENT_PAD: f32 = 20.0;
    /// 顶栏 / 状态栏的水平留白，比面板略大一点，视觉上才与面板文字对齐。
    pub const BAR_PAD_X: f32 = 18.0;
    /// 表单标签列宽：标签右对齐到 `PAD_X + LABEL_W`。
    pub const LABEL_W: f32 = 58.0;
    /// 标签与控件之间的呼吸间距。
    pub const GAP_X: f32 = 10.0;
    /// 同一表单内相邻行的垂直间距。
    pub const ROW_GAP: f32 = 6.0;
    /// 单个控件行的高度。
    pub const ROW_H: f32 = 28.0;
    /// 输入类控件的统一高度。
    pub const CONTROL_H: f32 = 26.0;
    /// 下拉弹层的最大高度。注意 `ComboBox::height()` 是**弹层**高度不是按钮高度，
    /// 以前误设成 CONTROL_H，弹层被压成一行、只能看到当前这一个选项。
    pub const COMBO_POPUP_H: f32 = 152.0;
    /// 章节标题与上一节内容之间。
    pub const SECTION_GAP: f32 = 16.0;
    /// 章节标题与其后内容的间距。
    pub const SECTION_LEAD: f32 = 8.0;
    /// 窗口 / 弹窗内边距。
    pub const WINDOW_PAD: f32 = 18.0;
}

/// 圆角阶梯：尺寸越大圆角越大；子元素用 `nested()` 从父级圆角里减去内边距，
/// 保证嵌套圆角同心、过渡连续。
pub mod radius {
    /// 徽标、色块、chip
    pub const CHIP: f32 = 5.0;
    /// 输入框、下拉框、小按钮
    pub const CONTROL: f32 = 8.0;
    /// 卡片、面板、按钮组里的选中块
    pub const CARD: f32 = 12.0;
    /// 大卡片、列表容器
    pub const PANEL: f32 = 16.0;
    /// 窗口、模态框
    pub const WINDOW: f32 = 20.0;

    /// 同心圆角：子元素圆角 = 父级圆角 − 内边距，最小不低于 `CHIP`。
    pub fn nested(outer: f32, inset: f32) -> f32 {
        (outer - inset).max(CHIP)
    }
}

/// 字号阶梯：最大 17（页面标题）到最小 9（微标签），相邻级差约 2px。
pub mod text {
    pub const DISPLAY: f32 = 17.0;
    pub const TITLE: f32 = 14.0;
    pub const BODY: f32 = 12.0;
    pub const LABEL: f32 = 10.5;
    pub const META: f32 = 9.8;
    pub const MICRO: f32 = 9.0;
}

pub fn text_style(ctx: &egui::Context, style: TextStyle) -> FontId {
    ctx.style_of(egui::Theme::Light)
        .text_styles
        .get(&style)
        .cloned()
        .unwrap_or(FontId::proportional(13.0))
}

/// 无障碍增强：更粗的焦点环、更强的对比度、更大的点击区。
///
/// 在 `apply()` 之后调用，叠加修改当前 Visuals。
/// 依据 WCAG 2.1 AA：正文对比 ≥4.5:1、焦点可见（3:1 非文本对比）。
pub fn a11y_enhance(ctx: &egui::Context, high_contrast: bool, a11y: bool) {
    if !high_contrast && !a11y {
        return;
    }
    ctx.all_styles_mut(|style| {
        let v = &mut style.visuals;

        if high_contrast {
            v.selection.bg_fill = v.selection.bg_fill.gamma_multiply(0.8);
            v.selection.stroke = egui::Stroke::new(2.5, v.selection.stroke.color);
            v.window_stroke = egui::Stroke::new(2.0, v.window_stroke.color);
            v.widgets.inactive.bg_stroke =
                egui::Stroke::new(1.5, v.widgets.inactive.bg_stroke.color);
            v.widgets.hovered.bg_stroke = egui::Stroke::new(2.0, v.widgets.hovered.bg_stroke.color);
            v.widgets.active.bg_stroke = egui::Stroke::new(2.5, v.widgets.active.bg_stroke.color);
        }

        if a11y {
            v.selection.stroke = egui::Stroke::new(3.0, v.selection.stroke.color);
            style.spacing.interact_size.y = style.spacing.interact_size.y.max(32.0);
            style.spacing.button_padding = egui::vec2(14.0, 8.0);
            v.widgets.hovered.bg_stroke = egui::Stroke::new(2.0, v.widgets.hovered.bg_stroke.color);
        }
    });
}
