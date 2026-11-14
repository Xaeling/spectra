//! 文件关联、Open With、应用和功能卸载项。提升时写 HKLM；测试 asInvoker 时写 HKCU。

use std::path::Path;

use anyhow::{Context, Result};
use winreg::enums::*;
use winreg::RegKey;

use crate::install::{is_admin, APP_VERSION};

const PROGID: &str = "StatAura.aura";
const APP_KEY: &str = "Software\\StatAura";
const CLASSES: &str = "Software\\Classes";
const UNINSTALL: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\StatAura";

fn hive() -> RegKey {
    if is_admin() {
        RegKey::predef(HKEY_LOCAL_MACHINE)
    } else {
        RegKey::predef(HKEY_CURRENT_USER)
    }
}

fn create(path: &str) -> Result<RegKey> {
    let (key, _) = hive()
        .create_subkey_with_flags(path, KEY_WRITE | KEY_WOW64_64KEY)
        .with_context(|| format!("创建注册表项 {path}"))?;
    Ok(key)
}

fn set_sz(key: &RegKey, name: &str, value: &str) -> Result<()> {
    key.set_value(name, &value)
        .with_context(|| format!("写注册表值 {name}"))?;
    Ok(())
}

pub fn install(app: &Path, dir: &Path) -> Result<()> {
    if std::env::var_os("STATAURA_SKIP_REGISTRY").is_some() {
        return Ok(());
    }
    let app = app.to_string_lossy();
    let dir_s = dir.to_string_lossy();
    let command = format!("\"{app}\" \"%1\"");
    let icon = format!("\"{app}\",0");

    let ext = create(&format!("{CLASSES}\\.aura"))?;
    set_sz(&ext, "", PROGID)?;
    set_sz(&ext, "Content Type", "application/x-stataura")?;
    set_sz(&ext, "PerceivedType", "document")?;

    let prog = create(&format!("{CLASSES}\\{PROGID}"))?;
    set_sz(&prog, "", "StatAura 工程")?;
    set_sz(&prog, "FriendlyTypeName", "StatAura 工程")?;
    let icon_key = create(&format!("{CLASSES}\\{PROGID}\\DefaultIcon"))?;
    set_sz(&icon_key, "", &icon)?;
    let cmd = create(&format!("{CLASSES}\\{PROGID}\\shell\\open\\command"))?;
    set_sz(&cmd, "", &command)?;

    let app_key = create(&format!("{CLASSES}\\Applications\\stataura.exe"))?;
    set_sz(&app_key, "FriendlyAppName", "StatAura")?;
    set_sz(&app_key, "ApplicationCompany", "StatAura")?;
    let app_cmd = create(&format!(
        "{CLASSES}\\Applications\\stataura.exe\\shell\\open\\command"
    ))?;
    set_sz(&app_cmd, "", &command)?;
    let supported = create(&format!(
        "{CLASSES}\\Applications\\stataura.exe\\SupportedTypes"
    ))?;
    set_sz(&supported, ".aura", "")?;

    let caps = create(&format!("{APP_KEY}\\Capabilities"))?;
    set_sz(&caps, "ApplicationName", "StatAura")?;
    set_sz(&caps, "ApplicationDescription", "科研数据统计与出版级绘图")?;
    let files = create(&format!("{APP_KEY}\\Capabilities\\FileAssociations"))?;
    set_sz(&files, ".aura", PROGID)?;

    let registered = create("Software\\RegisteredApplications")?;
    set_sz(&registered, "StatAura", &format!("{APP_KEY}\\Capabilities"))?;

    let uninstall = create(UNINSTALL)?;
    set_sz(&uninstall, "DisplayName", "StatAura")?;
    set_sz(&uninstall, "DisplayVersion", APP_VERSION)?;
    set_sz(&uninstall, "Publisher", "StatAura")?;
    set_sz(&uninstall, "DisplayIcon", &icon)?;
    set_sz(&uninstall, "InstallLocation", &dir_s)?;
    set_sz(
        &uninstall,
        "UninstallString",
        &format!("\"{dir_s}\\uninstall.exe\" --uninstall"),
    )?;
    set_sz(
        &uninstall,
        "QuietUninstallString",
        &format!("\"{dir_s}\\uninstall.exe\" --uninstall --silent"),
    )?;
    uninstall
        .set_value("EstimatedSize", &(APP_EXE_ESTIMATE_KB as u32))
        .context("写 EstimatedSize")?;
    uninstall
        .set_value("NoModify", &1u32)
        .context("写 NoModify")?;
    uninstall
        .set_value("NoRepair", &1u32)
        .context("写 NoRepair")?;

    crate::shortcuts::notify_shell();
    Ok(())
}

const APP_EXE_ESTIMATE_KB: u64 = 22 * 1024;

pub fn uninstall(dir: &Path) -> Result<()> {
    let _ = dir;
    if std::env::var_os("STATAURA_SKIP_REGISTRY").is_some() {
        return Ok(());
    }
    let classes = hive();
    // 只有仍指向我们时才删除扩展名，避免抹掉用户后来改的默认程序。
    let aura = classes
        .open_subkey_with_flags(format!("{CLASSES}\\.aura"), KEY_READ)
        .and_then(|k| k.get_value::<String, _>(""))
        .ok();
    if aura.as_deref() == Some(PROGID) {
        let _ = classes.delete_subkey_all(format!("{CLASSES}\\.aura"));
    }
    let _ = classes.delete_subkey_all(format!("{CLASSES}\\{PROGID}"));
    let _ = classes.delete_subkey_all(format!("{CLASSES}\\Applications\\stataura.exe"));
    let _ = classes.delete_subkey_all(APP_KEY);
    let _ = classes.delete_subkey_all(UNINSTALL);
    if let Ok(registered) = classes.open_subkey_with_flags(
        "Software\\RegisteredApplications",
        KEY_WRITE | KEY_WOW64_64KEY,
    ) {
        let _ = registered.delete_value("StatAura");
    }
    crate::shortcuts::notify_shell();
    Ok(())
}
