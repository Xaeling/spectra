#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod install;
mod registry;
mod shortcuts;

use std::path::PathBuf;

#[derive(Clone)]
pub struct Args {
    pub uninstall: bool,
    pub silent: bool,
    pub dir: Option<PathBuf>,
    pub ui_scale: f32,
}

impl Args {
    fn parse() -> Self {
        let mut a = Self {
            uninstall: false,
            silent: false,
            dir: None,
            ui_scale: 1.0,
        };
        let exe_uninstall = std::env::current_exe()
            .ok()
            .and_then(|p| {
                p.file_stem()
                    .map(|s| s.to_string_lossy().to_ascii_lowercase())
            })
            .map(|s| s == "uninstall")
            .unwrap_or(false);
        a.uninstall = exe_uninstall;

        let args: Vec<String> = std::env::args().skip(1).collect();
        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            match arg.as_str() {
                "--uninstall" => a.uninstall = true,
                "--silent" | "/S" => a.silent = true,
                "--dir" => {
                    if let Some(v) = args.get(i + 1) {
                        a.dir = Some(PathBuf::from(v));
                        i += 1;
                    }
                }
                "--ui-scale" => {
                    if let Some(v) = args.get(i + 1) {
                        a.ui_scale = v.parse::<f32>().unwrap_or(1.0).clamp(1.0, 2.0);
                        i += 1;
                    }
                }
                _ => {
                    if let Some(v) = arg.strip_prefix("--dir=") {
                        a.dir = Some(PathBuf::from(v));
                    } else if let Some(v) = arg.strip_prefix("--ui-scale=") {
                        a.ui_scale = v.parse::<f32>().unwrap_or(1.0).clamp(1.0, 2.0);
                    }
                }
            }
            i += 1;
        }
        a
    }

    fn target_dir(&self) -> PathBuf {
        self.dir.clone().unwrap_or_else(|| {
            if self.uninstall {
                install::current_dir_or_default()
            } else {
                install::default_dir()
            }
        })
    }
}

fn main() {
    let args = Args::parse();
    if args.silent {
        let dir = args.target_dir();
        let result = if args.uninstall {
            install::uninstall(&dir, None)
        } else {
            install::install(&dir, None)
        };
        match result {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                install::log(&format!("silent failed: {e:#}"));
                std::process::exit(1);
            }
        }
    }

    let scale = if args.ui_scale > 1.001 {
        args.ui_scale
    } else {
        1.0
    };
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(if args.uninstall {
            "StatAura 卸载程序"
        } else {
            "StatAura 安装程序"
        })
        .with_inner_size([700.0 * scale, 480.0 * scale])
        .with_min_inner_size([700.0 * scale, 480.0 * scale])
        .with_resizable(false)
        .with_app_id("stataura-setup");
    if let Ok(img) = image::load_from_memory(include_bytes!("../../assets/icon.png")) {
        let rgba = img.to_rgba8();
        viewport = viewport.with_icon(std::sync::Arc::new(egui::IconData {
            rgba: rgba.as_raw().clone(),
            width: rgba.width(),
            height: rgba.height(),
        }));
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    let _ = eframe::run_native(
        "StatAura Setup",
        options,
        Box::new(move |cc| Ok(Box::new(app::SetupApp::new(cc, args)))),
    );
}
