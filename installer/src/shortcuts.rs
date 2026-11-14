//! 快捷方式（IShellLinkW）与已知文件夹。无 PowerShell / WScript 依赖。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use windows::core::{Interface, HSTRING};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{
    IShellLinkW, SHChangeNotify, SHGetFolderPathW, ShellLink, CSIDL_COMMON_DESKTOPDIRECTORY,
    CSIDL_COMMON_PROGRAMS, CSIDL_DESKTOPDIRECTORY, CSIDL_PROGRAMS, SHCNE_ASSOCCHANGED,
    SHCNF_IDLIST,
};

pub fn create(app: &Path) -> Result<()> {
    // 沙箱/CI 里桌面可能不可写；真实安装器由构建脚本不设置该变量。
    if std::env::var_os("STATAURA_SKIP_SHORTCUTS").is_some() {
        return Ok(());
    }
    let desktop = folder(if crate::install::is_admin() {
        CSIDL_COMMON_DESKTOPDIRECTORY
    } else {
        CSIDL_DESKTOPDIRECTORY
    })?;
    make_link(&desktop.join("StatAura.lnk"), app)?;

    let programs = folder(if crate::install::is_admin() {
        CSIDL_COMMON_PROGRAMS
    } else {
        CSIDL_PROGRAMS
    })?;
    let stat = programs.join("StatAura");
    std::fs::create_dir_all(&stat)?;
    make_link(&stat.join("StatAura.lnk"), app)?;
    notify_shell();
    Ok(())
}

pub fn remove() -> Result<()> {
    if std::env::var_os("STATAURA_SKIP_SHORTCUTS").is_some() {
        return Ok(());
    }
    for desktop in [
        folder(CSIDL_COMMON_DESKTOPDIRECTORY),
        folder(CSIDL_DESKTOPDIRECTORY),
    ]
    .into_iter()
    .flatten()
    {
        let _ = std::fs::remove_file(desktop.join("StatAura.lnk"));
    }
    for programs in [folder(CSIDL_COMMON_PROGRAMS), folder(CSIDL_PROGRAMS)]
        .into_iter()
        .flatten()
    {
        let dir = programs.join("StatAura");
        let _ = std::fs::remove_dir_all(dir);
    }
    notify_shell();
    Ok(())
}

pub fn notify_shell() {
    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
}

fn folder(csidl: u32) -> Result<PathBuf> {
    let mut buf = [0u16; 260];
    unsafe {
        SHGetFolderPathW(None, csidl as i32, None, 0, &mut buf).context("读取系统文件夹失败")?;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Ok(PathBuf::from(String::from_utf16_lossy(&buf[..len])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_a_shortcut_in_temp() {
        let dir = std::env::temp_dir().join(format!("stataura_lnk_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let lnk = dir.join("StatAura.lnk");
        make_link(&lnk, Path::new(r"C:\Windows\notepad.exe")).expect("创建快捷方式");
        assert!(lnk.is_file(), "快捷方式应存在");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

fn make_link(link_path: &Path, app: &Path) -> Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
            .context("创建 ShellLink 失败")?;
        let app_h = HSTRING::from(app.to_string_lossy().to_string());
        let dir = HSTRING::from(
            app.parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
        );
        link.SetPath(&app_h)?;
        link.SetWorkingDirectory(&dir)?;
        link.SetDescription(&HSTRING::from("StatAura"))?;
        link.SetIconLocation(&app_h, 0)?;
        let file: IPersistFile = link.cast()?;
        file.Save(
            &HSTRING::from(link_path.to_string_lossy().to_string()),
            true,
        )?;
    }
    Ok(())
}
