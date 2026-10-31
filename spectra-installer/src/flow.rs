//! 安装 / 卸载流程。在后台线程执行，通过 mpsc 推送进度事件给 UI。

use crate::{embed, fsops, model, registry, shortcuts};
use std::path::PathBuf;
use std::sync::mpsc::Sender;

#[derive(Debug, Clone)]
pub enum Msg {
    Progress(f32, String),
    Log(String),
    Finished(bool, String),
}

fn send(tx: &Sender<Msg>, m: Msg) {
    let _ = tx.send(m);
}

fn step(tx: &Sender<Msg>, p: f32, s: impl Into<String>) {
    send(tx, Msg::Progress(p.clamp(0.0, 1.0), s.into()));
}

fn log(tx: &Sender<Msg>, s: impl Into<String>) {
    send(tx, Msg::Log(s.into()));
}

/// 安装文件的来源
enum Src {
    /// 编译期嵌在本 exe 里的主程序（单文件安装包的正常路径）
    Embedded,
    /// 外部目录（--payload，或本 exe 旁边正好放着主程序时）
    Dir(PathBuf),
}

pub fn run_install(
    tx: Sender<Msg>,
    dir: PathBuf,
    payload: Option<PathBuf>,
    extra_preserve: Vec<String>,
) {
    step(&tx, 0.02, "正在准备安装…");

    if let Err(e) = std::fs::create_dir_all(&dir) {
        send(
            &tx,
            Msg::Finished(false, format!("无法创建安装目录：{}", e)),
        );
        return;
    }
    step(&tx, 0.10, format!("安装目录：{}", dir.display()));

    // 0) 决定文件来源：给了 --payload（或旁边正好有主程序）就用目录，
    //    否则用编译期嵌进来的那份 —— 这才是「一个 exe 展开所有文件」。
    let src = match payload {
        Some(d) => Src::Dir(d),
        None => Src::Embedded,
    };

    //    校验必须有主程序，且必须放在清理旧文件之前：
    //    否则 payload 为空时会先把已装好的旧版本删光，再装出一个空壳
    //    （快捷方式与 ARP 全都指向不存在的 exe）。
    let has_main = match &src {
        Src::Dir(d) => d.join(model::MAIN_EXE).is_file(),
        Src::Embedded => embed::has_main(),
    };
    if !has_main {
        let detail = match &src {
            Src::Dir(d) => format!(
                "当前 payload 目录：{}\n请把 {} 放进该目录，或换一个目录。",
                d.display(),
                model::MAIN_EXE
            ),
            Src::Embedded => "本安装程序在编译时没有嵌入主程序。\n\n\
                 请用 build.bat 重新生成：它会先编译主程序，\n\
                 再把 spectra.exe 嵌进本安装程序。"
                .to_string(),
        };
        send(
            &tx,
            Msg::Finished(
                false,
                format!("payload 中缺少主程序 {}\n\n{}", model::MAIN_EXE, detail),
            ),
        );
        return;
    }
    log(
        &tx,
        match &src {
            Src::Dir(d) => format!("主程序来源：{}", d.join(model::MAIN_EXE).display()),
            Src::Embedded => "主程序来源：安装包内嵌".to_string(),
        },
    );

    // 1) 备份保留文件（更新安装时不丢设置）
    let backup = fsops::temp_backup_dir();
    match fsops::backup_preserve(&dir, &backup, &extra_preserve) {
        Ok(0) => log(&tx, "没有检测到旧的用户设置"),
        Ok(n) => log(&tx, format!("已备份 {} 项用户设置", n)),
        Err(e) => log(&tx, format!("备份用户设置失败：{}", e)),
    }
    step(&tx, 0.20, "已备份用户设置");

    // 2) 清理旧版本文件（保留设置与自身）
    let self_exe = std::env::current_exe().unwrap_or_default();
    if let Err(e) = fsops::clear_dir(&dir, &self_exe, &extra_preserve) {
        log(&tx, format!("清理旧文件失败：{}", e));
    }
    step(&tx, 0.32, "已清理旧版本文件");

    // 3) 写主程序
    match &src {
        Src::Dir(d) => match fsops::copy_payload(d, &dir, &self_exe) {
            Ok(n) => step(&tx, 0.62, format!("已复制 {} 个文件", n)),
            Err(e) => {
                send(&tx, Msg::Finished(false, format!("复制文件失败：{}", e)));
                return;
            }
        },
        Src::Embedded => {
            let bytes = embed::main_exe().unwrap_or_default();
            match std::fs::write(dir.join(model::MAIN_EXE), bytes) {
                Ok(_) => step(&tx, 0.62, format!("已释放 {}", model::MAIN_EXE)),
                Err(e) => {
                    send(&tx, Msg::Finished(false, format!("写入主程序失败：{}", e)));
                    return;
                }
            }
        }
    }

    // 3.5) 建出目录布局。少了这一步安装目录里就只有一个 exe：
    //      主程序找不到「应用根」，字体/图标/示例数据等资源目录也不存在。
    match fsops::create_layout(&dir) {
        Ok(n) => log(&tx, format!("已建立目录布局（新建 {} 个目录）", n)),
        Err(e) => log(&tx, format!("建立目录布局失败：{}", e)),
    }
    step(&tx, 0.68, "已建立目录布局");

    // 4) 生成 uninstall.exe（复制自身）+ 它的提权清单
    let dst_uninst = dir.join(model::UNINSTALL_EXE);
    let already_uninstall = self_exe
        .file_name()
        .map(|f| f.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
        == "uninstall.exe";
    if !already_uninstall {
        match std::fs::copy(&self_exe, &dst_uninst) {
            Ok(_) => log(&tx, "已写入卸载程序"),
            Err(e) => log(&tx, format!("写入卸载程序失败：{}", e)),
        }
    }
    // 清单优先取 payload 目录里的，其次用编译期嵌进来的那份。
    // 卸载同样要写 Program Files，所以 uninstall.exe 也得能提权。
    let mf_bytes: Option<Vec<u8>> = match &src {
        Src::Dir(d) => [
            "app.manifest",
            "uninstall.exe.manifest",
            "spectra-installer.exe.manifest",
            "spectra-installer.exe.manifest",
        ]
        .iter()
        .filter_map(|n| std::fs::read(d.join(n)).ok())
        .next(),
        Src::Embedded => embed::manifest().map(|b| b.to_vec()),
    };
    if let Some(b) = mf_bytes {
        let mf_name = format!("{}.manifest", model::UNINSTALL_EXE);
        match std::fs::write(dir.join(&mf_name), b) {
            Ok(_) => log(&tx, format!("已写入 {}", mf_name)),
            Err(e) => log(&tx, format!("写入 {} 失败：{}", mf_name, e)),
        }
    }
    step(&tx, 0.74, "已写入卸载程序");

    // 5) 快捷方式
    let main_exe = dir.join(model::MAIN_EXE);
    for (name, ok) in shortcuts::create(&main_exe) {
        log(
            &tx,
            format!(
                "{}：{}",
                name,
                if ok { "已创建" } else { "失败（已尝试兜底）" }
            ),
        );
    }
    step(&tx, 0.85, "已创建快捷方式");

    // 6) ARP
    match registry::register_arp(
        &dir.to_string_lossy(),
        &main_exe.to_string_lossy(),
    ) {
        Ok(_) => log(&tx, "已注册到“已安装的应用”"),
        Err(e) => log(&tx, format!("注册 ARP 失败：{}", e)),
    }
    step(&tx, 0.92, "已写入系统卸载入口");

    // 7) 文件关联
    match registry::register_file_association(&main_exe.to_string_lossy()) {
        Ok(_) => log(&tx, "已注册文件关联"),
        Err(e) => log(&tx, format!("注册文件关联失败：{}", e)),
    }
    step(&tx, 0.96, "已注册文件关联");

    // 8) 还原用户设置
    match fsops::restore_preserve(&backup, &dir) {
        Ok(n) if n > 0 => log(&tx, format!("已还原 {} 项用户设置", n)),
        Err(e) => log(&tx, format!("还原用户设置失败：{}", e)),
        _ => {}
    }
    let _ = std::fs::remove_dir_all(&backup);

    step(&tx, 1.0, "安装完成");
    send(
        &tx,
        Msg::Finished(
            true,
            format!("{} 已安装到 {}", model::APP_NAME, dir.display()),
        ),
    );
}

pub fn run_uninstall(tx: Sender<Msg>, dir: PathBuf) {
    step(&tx, 0.05, "正在准备卸载…");

    // 1) 快捷方式
    for (name, ok) in shortcuts::remove() {
        log(
            &tx,
            format!("{}：{}", name, if ok { "已删除" } else { "不存在或删除失败" }),
        );
    }
    step(&tx, 0.30, "已删除快捷方式");

    // 2) 文件关联
    match registry::unregister_file_association() {
        Ok(_) => log(&tx, "已清理文件关联"),
        Err(e) => log(&tx, format!("清理文件关联失败：{}", e)),
    }
    step(&tx, 0.50, "已清理文件关联");

    // 3) ARP
    match registry::unregister_arp() {
        Ok(_) => log(&tx, "已移除系统卸载入口"),
        Err(e) => log(&tx, format!("移除 ARP 失败：{}", e)),
    }
    step(&tx, 0.65, "已移除 ARP 条目");

    // 4) 删除安装目录内容（跳过自身）
    let self_exe = std::env::current_exe().unwrap_or_default();
    if let Err(e) = fsops::clear_dir(&dir, &self_exe, &[]) {
        log(&tx, format!("删除程序文件失败：{}", e));
    }
    let _ = std::fs::remove_dir(&dir);
    step(&tx, 0.85, "已删除程序文件");

    step(&tx, 0.95, "正在清理卸载程序自身…");
    send(&tx, Msg::Finished(true, "卸载完成".to_string()));

    // 注意：真正的收尾（安排 cmd 延迟删除整个安装目录，含 uninstall.exe 自身）
    // 不在这里做。worker 是派生线程，主线程一旦退出它就会被杀，
    // 放在这里有可能根本执行不到。改由主线程在 UI 关闭后执行，见 main.rs。
}
