//! 卸载收尾：安排本进程退出后删除安装目录（含 uninstall.exe 自身）
//!
//! ## 为什么不用 `self_replace::self_delete()`
//!
//! 最初按常规做法用了 `self-replace` 的 `self_delete()`。它在 Windows 上的实现是：
//!
//!   1. 把当前 exe **复制**到临时目录
//!   2. 从临时目录**派生一个隐藏的 helper 进程**
//!   3. helper 等父进程退出后，删除原 exe
//!
//! 问题在于——「把自己复制到 temp + 派生隐藏子进程 + 进程退出后自删除」
//! 这个组合，正是启发式杀软重点盯的行为特征（很多木马的持久化/反取证就是
//! 这套动作）。所以卸载程序很容易被报毒。
//!
//! ## 这里的做法
//!
//! 改为派生一个**系统自带的 cmd.exe**，让它延迟几秒后 `rmdir /s /q` 整个
//! 安装目录：
//!
//!   - 不生成任何额外的可执行文件（没有 temp 副本，没有 helper 进程）
//!   - cmd.exe 是系统可信进程，动作只是一条普通的删除命令
//!   - 连 uninstall.exe 自身一起删掉，比只删自己更彻底、更简单
//!
//! 调用后应尽快让本进程退出，cmd 才会真正删得掉（运行中的 exe 无法删除）。

use std::io;
use std::path::Path;

/// 创建子进程时不显示控制台窗口
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// cmd 等待本进程退出的秒数
#[cfg(windows)]
const WAIT_SECS: u32 = 3;

/// 派生 cmd，延迟后递归删除 dir。调用方随后应立即退出进程。
#[cfg(windows)]
pub fn schedule_dir_removal(dir: &Path) -> io::Result<()> {
    use std::os::windows::process::CommandExt;

    // 路径里若有双引号会破坏命令行，先剥掉
    let target = dir.to_string_lossy().trim().trim_matches('"').to_string();
    if target.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "安装目录路径为空",
        ));
    }

    // 主用 timeout（语义清晰）；极老系统上没有就退化成 ping 做延迟
    let by_timeout = format!(
        r#"timeout /t {s} /nobreak >nul & rmdir /s /q "{t}""#,
        s = WAIT_SECS,
        t = target
    );
    let by_ping = format!(
        r#"ping -n {n} 127.0.0.1 >nul & rmdir /s /q "{t}""#,
        n = WAIT_SECS + 1,
        t = target
    );

    spawn_hidden(&by_timeout).or_else(|_| spawn_hidden(&by_ping))
}

#[cfg(windows)]
fn spawn_hidden(script: &str) -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("cmd")
        .args(["/C", script])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
}

#[cfg(not(windows))]
pub fn schedule_dir_removal(_dir: &Path) -> io::Result<()> {
    Ok(())
}
