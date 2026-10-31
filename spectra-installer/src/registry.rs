//! 注册表：ARP 条目 + 文件关联

use crate::model;
use winreg::enums::*;
use winreg::RegKey;

/// 写入 HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\{AppKey}
pub fn register_arp(install_dir: &str, main_exe: &str) -> std::io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _disp) = hkcu.create_subkey(model::arp_path())?;

    let uninstaller = format!(
        r"{}\{}",
        install_dir.trim_end_matches('\\'),
        model::UNINSTALL_EXE
    );

    key.set_value("DisplayName", &model::APP_NAME)?;
    key.set_value("DisplayVersion", &model::APP_VERSION)?;
    key.set_value("Publisher", &model::PUBLISHER)?;
    key.set_value("InstallLocation", &install_dir)?;
    key.set_value("UninstallString", &format!("\"{}\"", uninstaller))?;
    key.set_value(
        "QuietUninstallString",
        &format!("\"{}\" /S", uninstaller),
    )?;
    key.set_value("DisplayIcon", &main_exe)?;
    key.set_value("NoModify", &1u32)?;
    key.set_value("NoRepair", &1u32)?;
    key.set_value("EstimatedSize", &64u32)?; // KB，仅用于展示
    Ok(())
}

pub fn unregister_arp() -> std::io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    // 当前标识必须先删，删不掉就直接报错
    hkcu.delete_subkey_all(model::arp_path())?;
    // 更名前的旧标识残留会让「已安装的应用」里出现僵尸条目，一并清掉
    for legacy in model::LEGACY_APP_KEYS {
        let _ = hkcu.delete_subkey_all(format!(
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{}",
            legacy
        ));
    }
    Ok(())
}

/// 扩展名 -> ProgId，ProgId -> 打开命令
/// 文件关联写入点：优先 HKLM\Software\Classes（真正的全局关联，需要管理员），
/// 失败再回落 HKCU\Software\Classes（仅当前用户）。
/// 直接写 HKEY_CLASSES_ROOT 时，非提升进程会被 UAC 虚拟化静默丢弃——
/// 写入看着成功、回读也对，但别的进程读到的仍是旧值。
fn class_roots() -> Vec<RegKey> {
    let mut v = Vec::new();
    if let Ok(k) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"Software\Classes") {
        v.push(k);
    }
    if let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Classes") {
        v.push(k);
    }
    if v.is_empty() {
        v.push(RegKey::predef(HKEY_CLASSES_ROOT));
    }
    v
}

pub fn register_file_association(main_exe: &str) -> std::io::Result<()> {
    let mut last: Option<std::io::Error> = None;
    for root in class_roots() {
        match register_file_association_at(&root, main_exe) {
            Ok(()) => return Ok(()),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "no registry root available")))
}

fn register_file_association_at(hkcr: &RegKey, main_exe: &str) -> std::io::Result<()> {

    // 更名前的旧 ProgId 子树先清掉，避免 .prism 仍指向已删除的旧 exe
    for legacy in model::LEGACY_PROG_IDS {
        let _ = hkcr.delete_subkey_all(*legacy);
    }

    for (ext, _desc) in model::EXTENSIONS {
        let (k, _) = hkcr.create_subkey(*ext)?;
        k.set_value("", &model::PROG_ID)?;
    }

    let (prog, _) = hkcr.create_subkey(model::PROG_ID)?;
    prog.set_value("", &"析光 Spectra 项目文件")?;

    let (cmd, _) = prog.create_subkey(r"shell\open\command")?;
    cmd.set_value("", &format!("\"{}\" \"%1\"", main_exe))?;

    let (icon, _) = prog.create_subkey("DefaultIcon")?;
    icon.set_value("", &format!("{},0", main_exe))?;

    Ok(())
}

/// 卸载时清理。
/// 注意：只有当扩展名的默认值确实指向我们自己的 ProgId 时才整键删除，
/// 否则只移除我们写入的 OpenWithProgids 条目，避免破坏其他程序的关联。
pub fn unregister_file_association() -> std::io::Result<()> {
    for root in class_roots() {
        let _ = unregister_file_association_at(&root);
    }
    Ok(())
}

fn unregister_file_association_at(hkcr: &RegKey) -> std::io::Result<()> {
    for (ext, _desc) in model::EXTENSIONS {
        let mut owned = false;
        if let Ok(k) = hkcr.open_subkey(*ext) {
            if let Ok(v) = k.get_value::<String, _>("") {
                owned = v == model::PROG_ID;
            }
        }
        if owned {
            let _ = hkcr.delete_subkey_all(*ext);
        } else if let Ok(k) = hkcr.open_subkey(format!(r"{}\OpenWithProgids", ext)) {
            let _ = k.delete_value(model::PROG_ID);
        }
    }

    let _ = hkcr.delete_subkey_all(model::PROG_ID);
    for legacy in model::LEGACY_PROG_IDS {
        let _ = hkcr.delete_subkey_all(*legacy);
    }
    Ok(())
}
