// 不弹出控制台窗口；日志写文件由 io 层负责（v0.2）。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use stataura::app::App;

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("StatAura")
        .with_inner_size([1120.0, 700.0])
        .with_min_inner_size([960.0, 640.0])
        .with_app_id("stataura")
        .with_drag_and_drop(true);

    // 视觉回归用：允许从环境变量指定窗口尺寸（`STATAURA_SIZE=1280x860`）。
    if let Ok(s) = std::env::var("STATAURA_SIZE") {
        if let Some((w, h)) = s.split_once('x') {
            if let (Ok(w), Ok(h)) = (w.trim().parse::<f32>(), h.trim().parse::<f32>()) {
                viewport = viewport.with_inner_size([w, h]);
            }
        }
    }

    if let Ok(img) = image::load_from_memory(include_bytes!("../assets/icon.png")) {
        let rgba = img.to_rgba8();
        viewport = viewport.with_icon(Arc::new(egui::IconData {
            rgba: rgba.as_raw().clone(),
            width: rgba.width(),
            height: rgba.height(),
        }));
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "StatAura",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
