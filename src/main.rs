// 隐藏 Windows 控制台黑框：GUI 程序不需要那个附带的 cmd 窗口。
// 注意：加了这行后 println! 不再可见，调试时临时注释掉即可。
#![cfg_attr(windows, windows_subsystem = "windows")]

//! 析光 Spectra —— 析光版入口。
//!
//! 技术栈：charis-ui（前端 UI）+ Rust（编译型）+ plotters（图表绘制）。
//! 字体全部打包进二进制，不依赖系统：
//! - 界面：Mi Sans（`assets/fonts/MiSans-Regular.ttf`）
//! - 图表导出：中文宋体 SimSun、英文 Times New Roman（按字符自动切换）

mod analysis;
mod app;
mod chartkind;
mod colormap;
mod export;
mod filedlg;
mod fonts;
mod theme;
mod i18n;
mod model;
mod palette;
mod plot;
mod advplot;
mod report;
mod settings;
mod prismfile;
mod sprxfile;
mod stats;
mod ui;

use charis_ui::prelude::*;

use crate::app::SpectraApp;

fn main() {
    // 字体注册挪到后台线程。
    //
    // 三份 TTF 加起来 20MB，其中宋体 10MB —— 解析它要一两秒。
    // 之前在开窗口之前同步注册，这段时间屏幕上就是一片白底，
    // 表现成「软件打开很卡、白屏很久」。
    //
    // 现在：窗口立刻画出来，字体在后台注册。
    // `register()` 内部有 Once，真正绘图时 render_chart() 再调一次，
    // 已注册就是空操作，没注册完则会等它 —— 总之不会丢字。
    std::thread::spawn(|| fonts::register());

    // 析光主窗口通常接近最大化。这里给的初始尺寸要能完整装下
    // 「菜单栏 + 工具栏 + 工作区 + 底部工具栏 + 状态栏」，不留悬念。
    let opts = WindowOptions::new("析光 Spectra")
        .size(1360.0, 920.0)
        .min_size(980.0, 720.0)
        // 窗口图标（标题栏 / 任务栏 / Alt-Tab）。
        // build.rs 只负责 exe **文件**图标；运行时的窗口图标必须在这里给，
        // 否则 exe 有图标、窗口却是个默认方块。
        .icon_png(include_bytes!("../assets/icon.png"))
        // 用系统字体（Segoe UI / SF Pro / 桌面字体），而不是内置的 Inter
        .system_font(true);

    charis_ui::run(SpectraApp::new(), opts).unwrap();
}
