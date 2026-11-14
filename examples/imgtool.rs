//! 开发期图片小工具（替代原 tools/ 下的脚本）——只用 Rust。
//!
//! ```text
//! cargo run --release --example imgtool -- crop in.png out.png x y w h [zoom]
//! cargo run --release --example imgtool -- sheet out.png cols rows scale in1.png in2.png ...
//! cargo run --release --example imgtool -- diff a.png b.png [out.png]
//! ```
//!
//! - `crop`：裁一块并整数倍最近邻放大，逐像素核对图表对齐用；
//! - `sheet`：多张 PNG 拼成联系表（白底、8px 间隙），总览一批样张用；
//! - `diff`：两张图逐像素比对，差异像素标洋红、其余转灰；不一致退出码 1。

use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(|s| s.as_str()) {
        Some("crop") => crop(&args[1..]),
        Some("sheet") => sheet(&args[1..]),
        Some("diff") => diff(&args[1..]),
        _ => {
            eprintln!("用法:");
            eprintln!("  imgtool crop  in.png out.png x y w h [zoom]");
            eprintln!("  imgtool sheet out.png cols rows scale in1.png ...");
            eprintln!("  imgtool diff  a.png b.png [out.png]");
            exit(2);
        }
    }
}

fn die(msg: &str) -> ! {
    eprintln!("错误: {msg}");
    exit(1);
}

fn crop(args: &[String]) {
    if args.len() < 6 {
        die("crop 需要: in.png out.png x y w h [zoom]");
    }
    let (input, output) = (&args[0], &args[1]);
    let (x, y, w, h): (u32, u32, u32, u32) = (
        args[2].parse().unwrap_or_else(|_| die("x 不是整数")),
        args[3].parse().unwrap_or_else(|_| die("y 不是整数")),
        args[4].parse().unwrap_or_else(|_| die("w 不是整数")),
        args[5].parse().unwrap_or_else(|_| die("h 不是整数")),
    );
    let zoom: u32 = args.get(6).and_then(|s| s.parse().ok()).unwrap_or(1).max(1);

    let img = image::open(input).unwrap_or_else(|e| die(&format!("读 {input} 失败: {e}")));
    let (iw, ih) = (img.width(), img.height());
    if x >= iw || y >= ih || w == 0 || h == 0 {
        die("裁剪区超出图片范围");
    }
    let w = w.min(iw - x);
    let h = h.min(ih - y);
    let cropped = img.crop_imm(x, y, w, h);
    // 最近邻放大：保留像素边界，方便看对齐
    let out = if zoom > 1 {
        cropped.resize_exact(w * zoom, h * zoom, image::imageops::FilterType::Nearest)
    } else {
        cropped
    };
    out.save(output)
        .unwrap_or_else(|e| die(&format!("写 {output} 失败: {e}")));
    println!("cropped {}x{} zoom {} -> {output}", w, h, zoom);
}

fn sheet(args: &[String]) {
    if args.len() < 5 {
        die("sheet 需要: out.png cols rows scale in1.png ...");
    }
    let output = &args[0];
    let cols: u32 = args[1].parse().unwrap_or_else(|_| die("cols 不是整数"));
    let rows: u32 = args[2].parse().unwrap_or_else(|_| die("rows 不是整数"));
    let scale: u32 = args[3].parse().unwrap_or_else(|_| die("scale 不是整数"));
    let scale = scale.max(1);
    let files = &args[4..];
    if cols == 0 || rows == 0 {
        die("cols/rows 必须 > 0");
    }

    // 先算统一的格子尺寸（所有图按 scale 下取整，居中放入）
    let mut tiles: Vec<image::RgbaImage> = Vec::new();
    let (mut tw, mut th) = (0u32, 0u32);
    for f in files {
        let img = image::open(f)
            .unwrap_or_else(|e| die(&format!("读 {f} 失败: {e}")))
            .to_rgba8();
        let (w, h) = (img.width() / scale, img.height() / scale);
        tw = tw.max(w);
        th = th.max(h);
        tiles.push(image::imageops::resize(
            &img,
            w,
            h,
            image::imageops::FilterType::Nearest,
        ));
    }

    let gap = 8u32;
    let sheet_w = cols * tw + (cols + 1) * gap;
    let sheet_h = rows * th + (rows + 1) * gap;
    let mut canvas =
        image::RgbaImage::from_pixel(sheet_w, sheet_h, image::Rgba([255, 255, 255, 255]));
    for (i, tile) in tiles.iter().enumerate() {
        if i as u32 >= cols * rows {
            break;
        }
        let (c, r) = (i as u32 % cols, i as u32 / cols);
        let x = gap + c * (tw + gap) + (tw - tile.width()) / 2;
        let y = gap + r * (th + gap) + (th - tile.height()) / 2;
        image::imageops::replace(&mut canvas, tile, x as i64, y as i64);
    }
    canvas
        .save(output)
        .unwrap_or_else(|e| die(&format!("写 {output} 失败: {e}")));
    println!("sheet {sheet_w}x{sheet_h} -> {output}");
}

fn diff(args: &[String]) {
    if args.len() < 2 {
        die("diff 需要: a.png b.png [out.png]");
    }
    let (a_path, b_path) = (&args[0], &args[1]);
    let out = args.get(2).cloned().unwrap_or_else(|| "diff.png".into());
    let a = image::open(a_path)
        .unwrap_or_else(|e| die(&format!("读 {a_path} 失败: {e}")))
        .to_rgba8();
    let b = image::open(b_path)
        .unwrap_or_else(|e| die(&format!("读 {b_path} 失败: {e}")))
        .to_rgba8();
    if a.width() != b.width() || a.height() != b.height() {
        println!(
            "尺寸不同 {}x{} vs {}x{} → 无法逐像素比对",
            a.width(),
            a.height(),
            b.width(),
            b.height()
        );
        exit(1);
    }
    let (w, h) = (a.width(), a.height());
    let mut diff_img = image::RgbaImage::new(w, h);
    let mut ndiff = 0u64;
    for y in 0..h {
        for x in 0..w {
            let pa = a.get_pixel(x, y).0;
            let pb = b.get_pixel(x, y).0;
            if pa == pb {
                // 一致像素转灰，让洋红的差异跳出来
                let g = (pa[0] as u32 * 30 + pa[1] as u32 * 59 + pa[2] as u32 * 11) / 100;
                let g = (g / 2 + 110) as u8;
                diff_img.put_pixel(x, y, image::Rgba([g, g, g, 255]));
            } else {
                ndiff += 1;
                diff_img.put_pixel(x, y, image::Rgba([255, 0, 255, 255]));
            }
        }
    }
    diff_img
        .save(&out)
        .unwrap_or_else(|e| die(&format!("写 {out} 失败: {e}")));
    let total = w as u64 * h as u64;
    if ndiff == 0 {
        println!("两张图逐像素一致（{}x{}）", w, h);
    } else {
        println!(
            "{ndiff} / {total} 像素不一致（{:.3}%）→ {out}",
            ndiff as f64 * 100.0 / total as f64
        );
        exit(1);
    }
}
