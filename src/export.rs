//! 导出后端：位图预览之外的三种「出图」通道。
//!
//! # 设计
//! 数据结构与配色规范（26 套色板、`shade()` 压暗、85% 填充）全部复用，
//! 只有**代码生成器**是可替换的：
//!
//! | 后端 | 产物 | 适用 |
//! |---|---|---|
//! | `PlottersSvg` | `.svg` | 内置，与预览 100% 同源（`draw_chart` 泛型复用） |
//! | `TikZ` | `.tex` | 期刊级：真矢量 + 真旋转 + 斜体符号 / 正体单位 |
//! | `Matplotlib` | `.py` | 需要 Python 生态二次加工时 |
//!
//! 当 plotters 在文字旋转、单位混排上达不到要求时，切到 TikZ 即可，
//! 核心的 UI 交互与数据管理仍由 Rust/charis-ui 掌控。

use crate::colormap;
use crate::palette::*;
use crate::plot::{bracket_boxes, export_h, export_w, geom, x_of, y_of, ChartView};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ExportBackend {
    /// plotters 内置 SVG 后端（默认）
    PlottersSvg,
    /// LaTeX TikZ —— 出版级排版
    TikZ,
    /// Python Matplotlib 脚本
    Matplotlib,
}

impl ExportBackend {
    /// 全部后端（设置窗口与导出按钮遍历用）。
    pub const ALL: [ExportBackend; 3] = [
        ExportBackend::PlottersSvg,
        ExportBackend::TikZ,
        ExportBackend::Matplotlib,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ExportBackend::PlottersSvg => "Plotters SVG",
            ExportBackend::TikZ => "LaTeX TikZ",
            ExportBackend::Matplotlib => "Python Matplotlib",
        }
    }

    pub fn ext(self) -> &'static str {
        match self {
            ExportBackend::PlottersSvg => "svg",
            ExportBackend::TikZ => "tex",
            ExportBackend::Matplotlib => "py",
        }
    }

    /// 一行说明（设置界面里显示）。
    pub fn hint(self) -> &'static str {
        match self {
            ExportBackend::PlottersSvg => "内置后端，与预览完全同源；文字为文本对象，可再编辑",
            ExportBackend::TikZ => "出版级：真矢量、真 90° 旋转、斜体符号 / 正体单位",
            ExportBackend::Matplotlib => "生成 Python 脚本，便于在 Python 生态里二次加工",
        }
    }
}

/// 默认导出画布尺寸（pt 级别的常见投稿尺寸 800×600）。

/// 按后端导出。`stem` 是不带扩展名的路径，返回实际写入的文件路径。
pub fn export(view: &ChartView, backend: ExportBackend, stem: &str) -> Result<String, String> {
    let path = format!("{}.{}", stem, backend.ext());
    match backend {
        ExportBackend::PlottersSvg => {
            crate::plot::export_svg(view, &path, export_w(), export_h())?;
        }
        ExportBackend::TikZ => {
            // 脚本后端目前只实现了柱状图的几何。若对折线 / 箱线等类型也
            // 生成，出来的会是「只有坐标轴、没有数据」的残缺文件 ——
            // 宁可明确报错，也不要给用户一个看着像成功其实是空图的结果。
            if !view.kind.script_exportable() {
                return Err(
                    "脚本导出目前仅支持柱状图，其他图型请用 SVG 或 PNG 导出".into(),
                );
            }
            std::fs::write(&path, tikz_source(view)).map_err(|e| e.to_string())?;
        }
        ExportBackend::Matplotlib => {
            if !view.kind.script_exportable() {
                return Err(
                    "脚本导出目前仅支持柱状图，其他图型请用 SVG 或 PNG 导出".into(),
                );
            }
            std::fs::write(&path, matplotlib_source(view)).map_err(|e| e.to_string())?;
        }
    }
    Ok(path)
}

// ══════════════════════════════════════════════════════════════════════════
// 导出格式清单（导出弹窗里那一整排选项）
// ══════════════════════════════════════════════════════════════════════════

/// 导出弹窗里可挑的每一种产物。
///
/// `Kind::Raster` 走「渲染到内存 RGBA → image crate 编码」；
/// `Kind::Vector` 走 plotters 自己的 SVGBackend；
/// `Kind::Script` 走已有的 TikZ / Matplotlib 生成器。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Png,
    Jpeg,
    Bmp,
    Tiff,
    Gif,
    WebP,
    Tga,
    Pnm,
    Qoi,
    /// JPEG XL：现代无损/有损格式，同画质体积比 PNG 小很多。
    /// image crate 没有它的编码器，走独立的 jxl-encoder（纯 Rust）。
    Jxl,
    Svg,
    Tikz,
    Matplotlib,
}

impl ExportFormat {
    /// 全部可选项，顺序即弹窗里的排列顺序（常用在前）。
    pub const ALL: [ExportFormat; 13] = [
        ExportFormat::Png,
        ExportFormat::Jpeg,
        ExportFormat::Bmp,
        ExportFormat::Tiff,
        ExportFormat::Gif,
        ExportFormat::WebP,
        ExportFormat::Tga,
        ExportFormat::Pnm,
        ExportFormat::Qoi,
        ExportFormat::Jxl,
        ExportFormat::Svg,
        ExportFormat::Tikz,
        ExportFormat::Matplotlib,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::Png => "PNG",
            ExportFormat::Jpeg => "JPEG",
            ExportFormat::Bmp => "BMP",
            ExportFormat::Tiff => "TIFF",
            ExportFormat::Gif => "GIF",
            ExportFormat::WebP => "WebP",
            ExportFormat::Tga => "TGA",
            ExportFormat::Pnm => "PNM",
            ExportFormat::Qoi => "QOI",
            ExportFormat::Jxl => "JPEG XL",
            ExportFormat::Svg => "SVG",
            ExportFormat::Tikz => "TikZ (.tex)",
            ExportFormat::Matplotlib => "Matplotlib (.py)",
        }
    }

    pub fn ext(self) -> &'static str {
        match self {
            ExportFormat::Png => "png",
            ExportFormat::Jpeg => "jpg",
            ExportFormat::Bmp => "bmp",
            ExportFormat::Tiff => "tiff",
            ExportFormat::Gif => "gif",
            ExportFormat::WebP => "webp",
            ExportFormat::Tga => "tga",
            ExportFormat::Pnm => "pnm",
            ExportFormat::Qoi => "qoi",
            ExportFormat::Jxl => "jxl",
            ExportFormat::Svg => "svg",
            ExportFormat::Tikz => "tex",
            ExportFormat::Matplotlib => "py",
        }
    }

    /// 一句话说明，显示在弹窗右侧。
    pub fn hint(self) -> &'static str {
        match self {
            ExportFormat::Png => "无损位图，通投首选；支持透明底",
            ExportFormat::Jpeg => "有损压缩，体积最小；不支持透明",
            ExportFormat::Bmp => "未压缩位图，体积大但兼容性最好",
            ExportFormat::Tiff => "无损位图，多数期刊投稿系统要求",
            ExportFormat::Gif => "256 色，仅适合极简示意图",
            ExportFormat::WebP => "现代格式，同等画质体积远小于 PNG",
            ExportFormat::Tga => "未压缩，部分老排版软件在用",
            ExportFormat::Pnm => "原始像素流，图像处理流水线中转用",
            ExportFormat::Qoi => "无损压缩，编解码极快",
            ExportFormat::Jxl => "现代格式，无损体积明显小于 PNG，支持有损/无损",
            ExportFormat::Svg => "矢量，文字为文本对象可再编辑",
            ExportFormat::Tikz => "LaTeX 源码，出版级排版",
            ExportFormat::Matplotlib => "Python 脚本，便于二次加工",
        }
    }

    /// 保存对话框里的过滤器名。
    pub fn filter(self) -> (&'static str, &'static str) {
        match self {
            ExportFormat::Png => ("PNG 图像", "png"),
            ExportFormat::Jpeg => ("JPEG 图像", "jpg"),
            ExportFormat::Bmp => ("BMP 图像", "bmp"),
            ExportFormat::Tiff => ("TIFF 图像", "tiff"),
            ExportFormat::Gif => ("GIF 图像", "gif"),
            ExportFormat::WebP => ("WebP 图像", "webp"),
            ExportFormat::Tga => ("TGA 图像", "tga"),
            ExportFormat::Pnm => ("PNM 图像", "pnm"),
            ExportFormat::Qoi => ("QOI 图像", "qoi"),
            ExportFormat::Jxl => ("JPEG XL 图像", "jxl"),
            ExportFormat::Svg => ("SVG 矢量图", "svg"),
            ExportFormat::Tikz => ("LaTeX 源码", "tex"),
            ExportFormat::Matplotlib => ("Python 脚本", "py"),
        }
    }

    fn is_raster(self) -> bool {
        !matches!(
            self,
            ExportFormat::Svg | ExportFormat::Tikz | ExportFormat::Matplotlib
        )
    }
}

/// 按格式导出到 `path`（含扩展名的完整路径）。
pub fn export_as(view: &ChartView, fmt: ExportFormat, path: &str) -> Result<(), String> {
    match fmt {
        ExportFormat::Svg => {
            return crate::plot::export_svg(view, path, export_w(), export_h());
        }
        ExportFormat::Tikz => {
            if !view.kind.script_exportable() {
                return Err("脚本导出目前仅支持柱状图，其他图型请选位图或 SVG".into());
            }
            std::fs::write(path, tikz_source(view)).map_err(|e| e.to_string())?;
            return Ok(());
        }
        ExportFormat::Matplotlib => {
            if !view.kind.script_exportable() {
                return Err("脚本导出目前仅支持柱状图，其他图型请选位图或 SVG".into());
            }
            std::fs::write(path, matplotlib_source(view)).map_err(|e| e.to_string())?;
            return Ok(());
        }
        _ => {}
    }
    debug_assert!(fmt.is_raster());
    // JPEG XL：image crate 不提供该编码器，交给 jxl-encoder（纯 Rust，无损）。
    if fmt == ExportFormat::Jxl {
        let (w, h) = (export_w(), export_h());
        let (rw, rh, buf) =
            crate::plot::render_rgba(view, w, h).ok_or_else(|| "图表渲染失败".to_string())?;
        // 压掉 alpha：科研图不需要透明底，且能显著减小体积。
        let mut rgb: Vec<u8> = Vec::with_capacity(rw as usize * rh as usize * 3);
        for px in buf.chunks_exact(4) {
            rgb.push(px[0]);
            rgb.push(px[1]);
            rgb.push(px[2]);
        }
        let bytes = jxl_encoder::LosslessConfig::new()
            .encode(&rgb, rw, rh, jxl_encoder::PixelLayout::Rgb8)
            .map_err(|e| format!("JPEG XL 编码失败：{}", e))?;
        return std::fs::write(path, bytes).map_err(|e| e.to_string());
    }
    let (w, h) = (export_w(), export_h());
    let (rw, rh, buf) =
        crate::plot::render_rgba(view, w, h).ok_or_else(|| "图表渲染失败".to_string())?;
    let img = image::RgbaImage::from_raw(rw, rh, buf).ok_or_else(|| "像素缓冲尺寸不符".to_string())?;
    let dyn_img = image::DynamicImage::ImageRgba8(img);
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut sink = std::io::BufWriter::new(file);
    // JPEG / BMP / GIF / PNM / TGA 的编码器不接受带 alpha 的输入，
    // 先压成 RGB 再写；PNG / TIFF / WebP / QOI 保留 alpha。
    match fmt {
        ExportFormat::Jpeg | ExportFormat::Bmp | ExportFormat::Gif | ExportFormat::Pnm => {
            let rgb = image::DynamicImage::ImageRgb8(dyn_img.to_rgb8());
            rgb.write_to(&mut sink, fmt.image_format())
                .map_err(|e| e.to_string())
        }
        ExportFormat::Tga => {
            let rgb = image::DynamicImage::ImageRgb8(dyn_img.to_rgb8());
            rgb.write_to(&mut sink, fmt.image_format())
                .map_err(|e| e.to_string())
        }
        _ => dyn_img
            .write_to(&mut sink, fmt.image_format())
            .map_err(|e| e.to_string()),
    }
}

impl ExportFormat {
    fn image_format(self) -> image::ImageFormat {
        match self {
            ExportFormat::Png => image::ImageFormat::Png,
            ExportFormat::Jpeg => image::ImageFormat::Jpeg,
            ExportFormat::Bmp => image::ImageFormat::Bmp,
            ExportFormat::Tiff => image::ImageFormat::Tiff,
            ExportFormat::Gif => image::ImageFormat::Gif,
            ExportFormat::WebP => image::ImageFormat::WebP,
            ExportFormat::Tga => image::ImageFormat::Tga,
            ExportFormat::Pnm => image::ImageFormat::Pnm,
            ExportFormat::Qoi => image::ImageFormat::Qoi,
            // Jxl 不走 image crate（它没有 JPEG XL 编码器），这里只给占位值。
            ExportFormat::Jxl => image::ImageFormat::Png,
            // 下面三个不走 image crate，这里给个占位值。
            _ => image::ImageFormat::Png,
        }
    }
}

// ── 共享小工具 ──────────────────────────────────────────────────────────────

fn f(v: f64) -> String {
    format!("{:.2}", v)
}

/// 数据集的两种颜色（与绘图端完全一致）：内部 85%、边框 shade()。
fn pair(cm: usize, i: usize) -> (colormap::Rgba, colormap::Rgba) {
    (
        colormap::fill_solid(cm, i),
        colormap::edge_of(cm, i),
    )
}

/// 轴标题片段：(文本, 是否斜体)。给 TikZ / Matplotlib 的排版用。
fn axis_parts(view: &ChartView, axis: char) -> Vec<(String, bool)> {
    view.pieces(axis)
}

/// 把 (名称, 单位) 拼成 LaTeX：`\textit{符号}\,(单位)` —— 符号斜体、单位正体。
fn latex_label(parts: &[(String, bool)]) -> String {
    parts
        .iter()
        .map(|(s, italic)| {
            let esc = s.replace('%', "\\%").replace('&', "\\&").replace('_', "\\_");
            if *italic {
                format!("\\textit{{{}}}", esc)
            } else {
                format!("\\mathrm{{{}}}", esc)
            }
        })
        .collect::<Vec<_>>()
        .join("\\,")
}

/// 把 (名称, 单位) 拼成 matplotlib mathtext：`$\it{符号}$ (单位)`。
fn mpl_label(parts: &[(String, bool)]) -> String {
    parts
        .iter()
        .map(|(s, italic)| {
            if *italic {
                format!("$\\it{{{}}}$", s.replace('$', ""))
            } else {
                s.replace('$', "")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// 复刻 plot.rs 的确定性抖动（黄金比），保证导出端与预览端点位一致。
fn jitter_x(cx: f64, bar_w: f64, k: usize) -> f64 {
    let t = (k as f64 * 0.6180339887498949) % 1.0 - 0.5;
    cx + t * bar_w * 0.42
}

// ── TikZ ────────────────────────────────────────────────────────────────────

/// 生成一份可直接 `pdflatex` 编译的 standalone 文档。
pub fn tikz_source(view: &ChartView) -> String {
    let g = geom(view, export_w(), export_h());
    let h = g.h as f64;
    // 像素坐标 → TikZ 坐标（y 轴翻转）
    let ty = |y: f64| h - y;
    let bar_w = g.slot * CHART_BAR_WIDTH_RATIO as f64;

    let mut s = String::new();
    s.push_str("% 由 析光 Spectra 导出 — 必须用 xelatex 编译（xeCJK 需要）\n");
    s.push_str("\\documentclass[border=2mm]{standalone}\n");
    s.push_str("\\usepackage{tikz}\n");
    s.push_str("\\usepackage{amsmath}\n");
    // 期刊规范：英文 Times，中文宋体
    s.push_str("\\usepackage{mathptmx}\n");
    s.push_str("\\usepackage{fontspec}\n");
    s.push_str("\\usepackage{xeCJK}\n");
    s.push_str("\\setmainfont{Times New Roman}\n");
    s.push_str("\\setCJKmainfont{SimSun}\n");
    s.push_str("\\begin{document}\n");
    s.push_str("\\begin{tikzpicture}[x=1pt,y=1pt]\n");

    // 颜色定义：内部色 / 边框色（shade 压暗）
    for (i, _) in view.groups.iter().enumerate() {
        let (fill, edge) = pair(view.colormap, i);
        s.push_str(&format!(
            "  \\definecolor{{fill{}}}{{RGB}}{{{},{},{}}}\n",
            i, fill[0], fill[1], fill[2]
        ));
        s.push_str(&format!(
            "  \\definecolor{{edge{}}}{{RGB}}{{{},{},{}}}\n",
            i, edge[0], edge[1], edge[2]
        ));
    }

    // 坐标框（Plain Frame，四边闭合）
    s.push_str(&format!(
        "  \\draw[line width=0.5pt,black] ({},{}) rectangle ({},{});\n",
        f(g.left as f64),
        f(ty(g.top as f64)),
        f(g.right as f64),
        f(ty(g.bottom as f64))
    ));

    // 向内刻度 + 刻度标签
    for v in &g.ticks {
        let y = ty(y_of(&g, *v));
        s.push_str(&format!(
            "  \\draw[line width=0.5pt] ({},{}) -- ({},{});\n",
            f(g.left as f64),
            f(y),
            f((g.left + CHART_TICK_LEN as i32) as f64),
            f(y)
        ));
        s.push_str(&format!(
            "  \\node[anchor=east] at ({},{}) {{\\small {}}};\n",
            f((g.left - 5) as f64),
            f(y),
            fmt_num(*v)
        ));
    }
    // 组名
    for (i, grp) in view.groups.iter().enumerate() {
        s.push_str(&format!(
            "  \\node[anchor=north] at ({},{}) {{\\small {}}};\n",
            f(x_of(&g, i)),
            f(ty(g.bottom as f64) - 4.0),
            tex_escape(&grp.name)
        ));
    }

    // 柱体：内部 85% 不透明 + shade() 边框（1.5pt）
    if view.kind.script_exportable() {
        for (i, grp) in view.groups.iter().enumerate() {
            let cx = x_of(&g, i);
            let x0 = cx - bar_w / 2.0;
            let x1 = cx + bar_w / 2.0;
            s.push_str(&format!(
                "  \\draw[fill=fill{},fill opacity=0.85,draw=edge{},line width=1.5pt] ({},{}) rectangle ({},{});\n",
                i,
                i,
                f(x0),
                f(ty(y_of(&g, 0.0))),
                f(x1),
                f(ty(y_of(&g, grp.mean)))
            ));
        }
    }

    // 误差棒：颜色严格跟随柱边框色
    if view.show_error {
        for (i, grp) in view.groups.iter().enumerate() {
            if grp.n < 2 {
                continue;
            }
            let cx = x_of(&g, i);
            let cap = bar_w * CHART_ERR_CAP_RATIO as f64 / 2.0;
            let hi = ty(y_of(&g, grp.mean + grp.sd));
            let lo = ty(y_of(&g, grp.mean - grp.sd));
            s.push_str(&format!(
                "  \\draw[edge{},line width=1pt] ({},{}) -- ({},{});\n",
                i,
                f(cx),
                f(hi),
                f(cx),
                f(lo)
            ));
            s.push_str(&format!(
                "  \\draw[edge{},line width=1pt] ({},{}) -- ({},{});\n",
                i,
                f(cx - cap),
                f(hi),
                f(cx + cap),
                f(hi)
            ));
            s.push_str(&format!(
                "  \\draw[edge{},line width=1pt] ({},{}) -- ({},{});\n",
                i,
                f(cx - cap),
                f(lo),
                f(cx + cap),
                f(lo)
            ));
        }
    }

    // 散点：双层（深色描边 + 填充色内芯）+ 抖动
    if view.show_scatter {
        for (i, grp) in view.groups.iter().enumerate() {
            let cx = x_of(&g, i);
            for (k, v) in grp.values.iter().enumerate() {
                s.push_str(&format!(
                    "  \\draw[fill=fill{},draw=edge{},line width=0.6pt] ({},{}) circle (2.2pt);\n",
                    i,
                    i,
                    f(jitter_x(cx, bar_w, k)),
                    f(ty(y_of(&g, *v)))
                ));
            }
        }
    }

    // 显著性括号：起点基于最高点，多层逐层累加
    for (xa, xb, y0, y1, label) in bracket_boxes(view, &g) {
        s.push_str(&format!(
            "  \\draw[line width=1pt,black] ({},{}) -- ({},{}) -- ({},{}) -- ({},{});\n",
            f(xa),
            f(ty(y0)),
            f(xa),
            f(ty(y1)),
            f(xb),
            f(ty(y1)),
            f(xb),
            f(ty(y0))
        ));
        if !label.is_empty() {
            s.push_str(&format!(
                "  \\node[anchor=south,inner sep=1pt] at ({},{}) {{\\small {}}};\n",
                f((xa + xb) / 2.0),
                f(ty(y1) + 2.0),
                tex_escape(&label)
            ));
        }
    }

    // 轴标题：X 水平；Y 逆时针 90°（TikZ rotate=90 即 CCW，自下而上阅读）
    let xp = axis_parts(view, 'x');
    if !xp.is_empty() {
        s.push_str(&format!(
            "  \\node[anchor=north] at ({},{}) {{{}}};\n",
            f((g.left + g.right) as f64 / 2.0),
            f(ty(g.bottom as f64) - 26.0),
            latex_label(&xp)
        ));
    }
    let yp = axis_parts(view, 'y');
    if !yp.is_empty() {
        s.push_str(&format!(
            "  \\node[rotate=90,anchor=center] at ({},{}) {{{}}};\n",
            f(14.0),
            f(ty((g.top + g.bottom) as f64 / 2.0)),
            latex_label(&yp)
        ));
    }
    if !view.title.is_empty() {
        s.push_str(&format!(
            "  \\node[anchor=north] at ({},{}) {{\\textbf{{{}}}}};\n",
            f((g.left + g.right) as f64 / 2.0),
            f(ty(4.0)),
            tex_escape(&view.title)
        ));
    }

    s.push_str("\\end{tikzpicture}\n");
    s.push_str("\\end{document}\n");
    s
}

fn tex_escape(s: &str) -> String {
    s.replace('\\', "\\textbackslash{}")
        .replace('%', "\\%")
        .replace('&', "\\&")
        .replace('#', "\\#")
        .replace('_', "\\_")
        .replace('$', "\\$")
}

fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{:.0}", v)
    } else {
        format!("{}", v)
    }
}

// ── Matplotlib ──────────────────────────────────────────────────────────────

/// 生成一份可直接 `python xxx.py` 运行的脚本。
pub fn matplotlib_source(view: &ChartView) -> String {
    let g = geom(view, export_w(), export_h());
    let bar_w = g.slot * CHART_BAR_WIDTH_RATIO as f64;

    let mut s = String::new();
    s.push_str("# 由 析光 Spectra 导出 — 运行：python this_file.py\n");
    s.push_str("import matplotlib.pyplot as plt\n\n");

    // 数据
    s.push_str(&format!("names = {}\n", py_list_str(&view.groups.iter().map(|x| x.name.clone()).collect::<Vec<_>>())));
    s.push_str(&format!("means = {}\n", py_list_num(&view.groups.iter().map(|x| x.mean).collect::<Vec<_>>())));
    s.push_str(&format!("sds = {}\n", py_list_num(&view.groups.iter().map(|x| x.sd).collect::<Vec<_>>())));
    s.push('\n');

    // 配色：内部色（0–1 浮点）+ 边框色（shade 压暗）
    s.push_str("FILL, EDGE = [], []\n");
    for (i, _) in view.groups.iter().enumerate() {
        let (fill, edge) = pair(view.colormap, i);
        // 显式绑定成 f64 再格式化：避免宏内联表达式的类型推断歧义
        let (fr, fg, fb) = (
            fill[0] as f64 / 255.0,
            fill[1] as f64 / 255.0,
            fill[2] as f64 / 255.0,
        );
        let (er, eg, eb) = (
            edge[0] as f64 / 255.0,
            edge[1] as f64 / 255.0,
            edge[2] as f64 / 255.0,
        );
        s.push_str(&format!("FILL.append(({:.4}, {:.4}, {:.4}))\n", fr, fg, fb));
        s.push_str(&format!("EDGE.append(({:.4}, {:.4}, {:.4}))\n", er, eg, eb));
    }
    s.push('\n');

    s.push_str("fig, ax = plt.subplots(figsize=(8, 6), dpi=150)\n");
    s.push_str("xs = list(range(len(means)))\n\n");

    // 柱体：alpha=0.85 填充 + shade 边框 1.5pt；误差棒颜色 = 边框色
    s.push_str("# 柱体：85% 填充 + shade() 压暗边框；误差棒与 T 型帽严格跟随边框色\n");
    s.push_str("bars = ax.bar(xs, means, width=0.6, linewidth=1.5)\n");
    s.push_str("for i, b in enumerate(bars):\n");
    s.push_str("    b.set_facecolor(FILL[i % len(FILL)])\n");
    s.push_str("    b.set_alpha(0.85)\n");
    s.push_str("    b.set_edgecolor(EDGE[i % len(EDGE)])\n");
    if view.show_error {
        s.push_str("    ax.errorbar(i, means[i], yerr=sds[i], fmt='none',\n");
        s.push_str("                ecolor=EDGE[i % len(EDGE)], elinewidth=1.0, capsize=4, capthick=1.0)\n");
    }
    s.push('\n');

    // 散点 + 抖动
    if view.show_scatter {
        s.push_str("# 散点：确定性抖动 + 深色描边（与柱体同规则）\n");
        for (i, grp) in view.groups.iter().enumerate() {
            let cx = x_of(&g, i);
            let px: Vec<f64> = (0..grp.values.len())
                .map(|k| jitter_x(cx, bar_w, k))
                .collect();
            // 把像素 X 换算回 matplotlib 的类目坐标（柱心为 i）
            let rel: Vec<f64> = px.iter().map(|x| (*x - cx) / g.slot * 0.8).collect();
            s.push_str(&format!(
                "ax.plot([{}], [{}], 'o',\n",
                py_list_num_rel(&rel, i as f64),
                py_list_num(&grp.values)
            ));
            s.push_str(&format!(
                "        markerfacecolor=FILL[{}], markeredgecolor=EDGE[{}],\n",
                i % view.groups.len().max(1),
                i % view.groups.len().max(1)
            ));
            s.push_str("        markeredgewidth=0.8, markersize=5, linestyle='none', zorder=3)\n");
        }
        s.push('\n');
    }

    // 显著性括号
    let bb = bracket_boxes(view, &g);
    if !bb.is_empty() {
        // 括号是画在坐标轴上方的覆盖层，不影响 y 轴定标。
        // 这里把像素 y 反算成数值写进脚本，保证导出图与预览位置一致。
        for (lv, (_xa, _xb, y0, y1, label)) in bb.iter().enumerate() {
            let v0 = value_of(&g, *y0);
            let v1 = value_of(&g, *y1);
            let a = view.comparisons.get(lv).map(|c| c.a).unwrap_or(0);
            let b = view.comparisons.get(lv).map(|c| c.b).unwrap_or(1);
            s.push_str(&format!(
                "ax.plot([{}, {}, {}, {}], [{}, {}, {}, {}], color='black', lw=1.0)\n",
                a, a, b, b, f(v0), f(v1), f(v1), f(v0)
            ));
            if !label.is_empty() {
                s.push_str(&format!(
                    "ax.text({}, {}, '{}', ha='center', va='bottom')\n",
                    (a as f64 + b as f64) / 2.0,
                    f(v1),
                    label.replace('\'', "")
                ));
            }
        }
        s.push('\n');
    }

    // 字体：中文宋体 + 英文 Times New Roman（期刊通行规范）
    s.push_str("# 字体：中文宋体，英文 Times New Roman\n");
    s.push_str("plt.rcParams['font.family'] = 'serif'\n");
    s.push_str("plt.rcParams['font.serif'] = ['Times New Roman', 'SimSun', 'DejaVu Serif']\n");
    s.push_str("plt.rcParams['mathtext.fontset'] = 'stix'\n");
    s.push_str("plt.rcParams['axes.unicode_minus'] = False\n\n");
    s.push_str("# Plain Frame + 向内刻度（析光默认）\n");
    s.push_str("for sp in ax.spines.values():\n    sp.set_visible(True)\n    sp.set_linewidth(0.8)\n");
    s.push_str("ax.tick_params(direction='in', length=4, width=0.8)\n");
    s.push_str("ax.set_xticks(xs)\nax.set_xticklabels(names)\n");
    s.push_str(&format!("ax.set_ylim(0, {})\n", f(g.y_max)));
    let xp = axis_parts(view, 'x');
    if !xp.is_empty() {
        s.push_str(&format!("ax.set_xlabel(r'{}')\n", mpl_label(&xp)));
    }
    let yp = axis_parts(view, 'y');
    if !yp.is_empty() {
        // rotation=90：matplotlib 逆时针 90°，自下而上阅读
        s.push_str(&format!("ax.set_ylabel(r'{}', rotation=90)\n", mpl_label(&yp)));
    }
    if !view.title.is_empty() {
        s.push_str(&format!("ax.set_title('{}')\n", view.title.replace('\'', "")));
    }
    if view.show_legend {
        s.push_str("ax.legend([b for b in bars], names, frameon=False)\n");
    }
    s.push_str("\nplt.tight_layout()\nplt.savefig('chart.pdf', dpi=300)\nplt.show()\n");
    s
}

/// 像素 y → 数值（供 matplotlib 用数据坐标画括号）。
fn value_of(g: &crate::plot::Geom, y: f64) -> f64 {
    g.y_min + (g.bottom as f64 - y) / (g.bottom - g.top) as f64 * (g.y_max - g.y_min)
}

fn py_list_str(v: &[String]) -> String {
    let items: Vec<String> = v.iter().map(|s| format!("'{}'", s.replace('\'', ""))).collect();
    format!("[{}]", items.join(", "))
}

fn py_list_num(v: &[f64]) -> String {
    format!("[{}]", v.iter().map(|x| f(*x)).collect::<Vec<_>>().join(", "))
}

fn py_list_num_rel(v: &[f64], base: f64) -> String {
    format!(
        "[{}]",
        v.iter().map(|x| f(base + *x)).collect::<Vec<_>>().join(", ")
    )
}
