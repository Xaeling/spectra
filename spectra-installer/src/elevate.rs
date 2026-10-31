//! 管理员权限检测与自我提升

use std::path::Path;
use std::process::Command;

#[cfg(windows)]
pub fn is_admin() -> bool {
    unsafe { windows::Win32::UI::Shell::IsUserAnAdmin().as_bool() }
}

#[cfg(not(windows))]
pub fn is_admin() -> bool {
    true
}

/// 以管理员权限重新启动自身。
/// 返回 true 表示已成功发起提升请求，调用方应直接退出当前进程。
#[cfg(windows)]
pub fn run_as_admin(args: &[String]) -> bool {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return false,
    };

    let params: String = args
        .iter()
        .map(|a| format!("\"{}\" ", a))
        .collect::<Vec<_>>()
        .concat();

    let op = windows::core::HSTRING::from("runas");
    let file = windows::core::HSTRING::from(exe.to_string_lossy().as_ref());
    let par = windows::core::HSTRING::from(params.as_str());
    let dir = windows::core::HSTRING::new();

    let r = unsafe {
        windows::Win32::UI::Shell::ShellExecuteW(
            None,
            &op,
            &file,
            &par,
            &dir,
            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        )
    };

    // 返回值 > 32 表示成功（HINSTANCE 是指针，先转成整数再比）
    if r.0 as usize > 32 {
        true
    } else {
        ps_runas(&exe, args)
    }
}

#[cfg(not(windows))]
pub fn run_as_admin(_args: &[String]) -> bool {
    false
}

/// 兜底：ShellExecuteW 失败时改用 PowerShell 的 -Verb RunAs
fn ps_runas(exe: &Path, args: &[String]) -> bool {
    let list = args
        .iter()
        .map(|a| format!("'{}'", a.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(",");
    let cmd = format!(
        "Start-Process -FilePath '{}' -ArgumentList @({}) -Verb RunAs",
        exe.to_string_lossy().replace('\'', "''"),
        list
    );

    let mut c = Command::new("powershell");
    c.args([
        "-NoProfile",
        "-NonInteractive",
        "-WindowStyle",
        "Hidden",
        "-Command",
        &cmd,
    ]);
    // 不显示控制台窗口（PowerShell 自身启动时会闪一下）
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c.spawn().is_ok()
}
