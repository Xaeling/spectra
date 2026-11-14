//! 图表导出：位图（PNG/JPEG/TIFF）与矢量（SVG/PDF）。

use std::path::Path;

use anyhow::{Context, Result};

use crate::plot::chart::Render;
use crate::plot::export::{to_pdf, to_svg};
use crate::plot::raster::rasterize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Png,
    Jpeg,
    Tiff,
    Svg,
    Pdf,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 5] = [Self::Png, Self::Jpeg, Self::Tiff, Self::Svg, Self::Pdf];

    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Tiff => "TIFF",
            Self::Svg => "SVG",
            Self::Pdf => "PDF",
        }
    }

    pub fn ext(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Tiff => "tif",
            Self::Svg => "svg",
            Self::Pdf => "pdf",
        }
    }

    pub fn is_vector(self) -> bool {
        matches!(self, Self::Svg | Self::Pdf)
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Png => "无损位图，适合投稿系统与幻灯片",
            Self::Jpeg => "有损压缩，文件更小",
            Self::Tiff => "出版用无损位图，支持 300–600 dpi",
            Self::Svg => "矢量，文字已转曲，可直接进 Illustrator",
            Self::Pdf => "矢量，版式固定，适合期刊投稿",
        }
    }
}

pub fn export_chart(
    render: &Render,
    format: ExportFormat,
    width_pt: f32,
    height_pt: f32,
    path: &Path,
) -> Result<()> {
    match format {
        ExportFormat::Svg => {
            let svg = to_svg(&render.scene, width_pt, height_pt);
            std::fs::write(path, svg).with_context(|| format!("写入 {} 失败", path.display()))?;
        }
        ExportFormat::Pdf => {
            let pdf = to_pdf(&render.scene, width_pt, height_pt);
            std::fs::write(path, pdf).with_context(|| format!("写入 {} 失败", path.display()))?;
        }
        _ => {
            let (w, h, rgba) = rasterize(&render.scene);
            let img = image::RgbaImage::from_raw(w, h, rgba)
                .ok_or_else(|| anyhow::anyhow!("图像缓冲尺寸不匹配"))?;
            match format {
                ExportFormat::Png => img
                    .save(path)
                    .with_context(|| format!("写入 {} 失败", path.display()))?,
                ExportFormat::Tiff => img
                    .save(path)
                    .with_context(|| format!("写入 {} 失败", path.display()))?,
                ExportFormat::Jpeg => {
                    let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
                    let file = std::fs::File::create(path)
                        .with_context(|| format!("创建 {} 失败", path.display()))?;
                    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(file, 94);
                    encoder
                        .encode(&rgb, w, h, image::ExtendedColorType::Rgb8)
                        .context("JPEG 编码失败")?;
                }
                _ => unreachable!(),
            }
        }
    }
    Ok(())
}

pub fn png_bytes(render: &Render) -> (u32, u32, Vec<u8>) {
    rasterize(&render.scene)
}
