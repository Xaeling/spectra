//! 桌面与开始菜单快捷方式

use crate::model;
use known_folders::{get_known_folder_path, KnownFolder};
use std::path::{Path, PathBuf};

fn desktop_dir() -> Option<PathBuf> {
    get_known_folder_path(KnownFolder::Desktop).or_else(|| {
        std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("Desktop"))
    })
}

fn programs_dir() -> Option<PathBuf> {
    get_known_folder_path(KnownFolder::Programs).or_else(|| {
        std::env::var_os("APPDATA").map(|p| {
            PathBuf::from(p).join(r"Microsoft\Windows\Start Menu\Programs")
        })
    })
}

/// 让子进程不显示控制台窗口
#[cfg(windows)]
fn hide_window(c: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    c.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_window(_c: &mut std::process::Command) {}

/// lnks 失败时改用 PowerShell + WScript.Shell 兜底
fn ps_create(target: &Path, work_dir: &Path, lnk: &Path) -> bool {
    let esc = |s: &str| s.replace('\'', "''");
    let cmd = format!(
        "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}');$s.TargetPath='{}';$s.WorkingDirectory='{}';$s.Save()",
        esc(&lnk.to_string_lossy()),
        esc(&target.to_string_lossy()),
        esc(&work_dir.to_string_lossy()),
    );
    let mut c = std::process::Command::new("powershell");
    c.args([
        "-NoProfile",
        "-NonInteractive",
        "-WindowStyle",
        "Hidden",
        "-Command",
        &cmd,
    ]);
    hide_window(&mut c);
    c.output().map(|o| o.status.success()).unwrap_or(false)
}

fn create_one(target: &Path, work_dir: &Path, lnk: &Path) -> bool {
    // 编译器提示这里不需要 mut —— 也就确认了 Shortcut::new 返回的不是 Result，
    // 且 save 走的是 &self，之前担心的签名问题不存在
    let sc = lnks::Shortcut::new(target);
    match sc.save(lnk) {
        Ok(_) => true,
        Err(_) => ps_create(target, work_dir, lnk),
    }
}

pub fn create(main_exe: &Path) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let work_dir = main_exe.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let work = work_dir.as_path();

    if let Some(d) = desktop_dir() {
        let lnk = d.join(format!("{}.lnk", model::APP_NAME));
        let ok = create_one(main_exe, work, &lnk);
        out.push((format!("桌面：{}", lnk.display()), ok));
    }

    if let Some(p) = programs_dir() {
        let folder = p.join(model::APP_NAME);
        let _ = std::fs::create_dir_all(&folder);
        let lnk = folder.join(format!("{}.lnk", model::APP_NAME));
        let ok = create_one(main_exe, work, &lnk);
        out.push((format!("开始菜单：{}", lnk.display()), ok));
    }

    out
}

pub fn remove() -> Vec<(String, bool)> {
    let mut out = Vec::new();

    if let Some(d) = desktop_dir() {
        let lnk = d.join(format!("{}.lnk", model::APP_NAME));
        let ok = std::fs::remove_file(&lnk).is_ok();
        out.push((format!("桌面：{}", lnk.display()), ok));
    }

    if let Some(p) = programs_dir() {
        let folder = p.join(model::APP_NAME);
        let lnk = folder.join(format!("{}.lnk", model::APP_NAME));
        let ok1 = std::fs::remove_file(&lnk).is_ok();
        let ok2 = std::fs::remove_dir(&folder).is_ok();
        out.push((format!("开始菜单：{}", lnk.display()), ok1 || ok2));
    }

    out
}
