// 不创建控制台窗口。
// 安装/卸载全过程都是 GUI，加这一行后运行时不会再闪黑框。
// 代价是 println!/eprintln! 不再可见 —— 静默模式改为写日志文件（见下）。
#![cfg_attr(windows, windows_subsystem = "windows")]

mod elevate;
mod embed;
mod flow;
mod fsops;
mod model;
mod registry;
mod selfdel;
mod shortcuts;
mod ui;

use std::path::PathBuf;

struct Options {
    uninstall: bool,
    silent: bool,
    dir: Option<PathBuf>,
    payload: Option<PathBuf>,
    preserve: Vec<String>,
}

fn parse_args() -> Options {
    let mut o = Options {
        uninstall: false,
        silent: false,
        dir: None,
        payload: None,
        preserve: Vec::new(),
    };

    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "/U" | "/u" | "--uninstall" => o.uninstall = true,
            "/S" | "/s" | "--silent" => o.silent = true,
            "--dir" => o.dir = it.next().map(PathBuf::from),
            "--payload" => o.payload = it.next().map(PathBuf::from),
            "--preserve" => {
                if let Some(v) = it.next() {
                    o.preserve.push(v);
                }
            }
            _ => {}
        }
    }
    o
}

/// 静默模式下没有控制台，进度与错误写到这里，便于排查
fn silent_log_path() -> PathBuf {
    std::env::temp_dir().join(format!("{}_setup.log", model::APP_KEY))
}

fn main() {
    let mut opt = parse_args();

    // 模式判定：可执行文件名为 uninstall.exe 时进入卸载模式
    if let Ok(exe) = std::env::current_exe() {
        let stem = exe
            .file_stem()
            .map(|s| s.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if stem == "uninstall" {
            opt.uninstall = true;
        }
    }
    let is_uninst = opt.uninstall;

    // 提权：写 Program Files 和 HKCR 都需要管理员
    if !elevate::is_admin() {
        let args: Vec<String> = std::env::args().skip(1).collect();
        if elevate::run_as_admin(&args) {
            // 已发起提升请求，当前进程退出
            return;
        }
        // 提权失败也继续（可能装到用户可写目录），只是大概率会中途报错
    }

    let dir = opt
        .dir
        .unwrap_or_else(|| PathBuf::from(model::DEFAULT_INSTALL_DIR));

    // payload 来源：
    //   --payload <dir>  → 用该目录（调试 / 临时换主程序）
    //   旁边正好放着 spectra.exe → 也用目录模式
    //   否则             → 用编译期嵌进本 exe 的主程序（单文件安装包的正常路径）
    let payload: Option<PathBuf> = opt.payload.clone().or_else(|| {
        let d = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|x| x.to_path_buf()))?;
        if d.join(model::MAIN_EXE).is_file() {
            Some(d)
        } else {
            None
        }
    });
    if payload.is_none() && !embed::has_main() {
        // 走到这里说明两者都没有；flow 里会给出明确报错并中止，
        // 这里只额外记一条日志，方便静默模式排查。
        let _ = std::fs::write(
            silent_log_path(),
            "本安装程序没有嵌入主程序，且旁边也没有 spectra.exe。请用 build.bat 重新生成。",
        );
    }

    let (tx, rx) = std::sync::mpsc::channel();
    let (d2, p2, pre2) = (dir.clone(), payload.clone(), opt.preserve.clone());
    let worker = std::thread::spawn(move || {
        if is_uninst {
            flow::run_uninstall(tx, d2)
        } else {
            flow::run_install(tx, d2, p2, pre2)
        }
    });

    if opt.silent {
        let mut lines: Vec<String> = Vec::new();
        for m in rx {
            match m {
                flow::Msg::Log(s) => lines.push(s),
                flow::Msg::Progress(_, s) => lines.push(s),
                flow::Msg::Finished(_, s) => {
                    lines.push(s);
                    break;
                }
            }
        }
        let _ = worker.join();
        let _ = std::fs::write(silent_log_path(), lines.join("\n"));

        // 收尾同样放在主线程：派生 cmd 延迟删除安装目录
        if is_uninst {
            let _ = selfdel::schedule_dir_removal(&dir);
            // 立刻退出，cmd 才能删掉 uninstall.exe
            std::process::exit(0);
        }
        return;
    }

    let title = if is_uninst {
        format!("正在卸载 {}", model::APP_NAME)
    } else {
        format!("正在安装 {}", model::APP_NAME)
    };

    let _ = eframe::run_native(
        &title,
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([520.0, 340.0])
                .with_resizable(false)
                .with_minimize_button(false)
                .with_maximize_button(false),
            ..Default::default()
        },
        Box::new(move |cc| {
            let app = ui::InstallerApp::new(cc, is_uninst, rx);
            Ok(Box::new(app) as Box<dyn eframe::App>)
        }),
    );

    // UI 已关闭。等 worker 收尾，再由主线程执行自我清理。
    let _ = worker.join();

    if is_uninst {
        // 派生 cmd，等本进程退出后递归删除整个安装目录（含 uninstall.exe 自身）
        if let Err(e) = selfdel::schedule_dir_removal(&dir) {
            let _ = std::fs::write(
                silent_log_path(),
                format!("清理安装目录失败，请手动删除：{}\n{}", dir.display(), e),
            );
        }
        // 立即退出，不要再触碰安装目录里的任何文件
        std::process::exit(0);
    }
}
