//! 安装/卸载文件系统操作。UI 线程只负责显示，重活在线程里跑。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use anyhow::{bail, Context, Result};

use crate::{registry, shortcuts};

pub static APP_EXE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/stataura.exe"));
pub const APP_VERSION: &str = env!("APP_VERSION");

#[derive(Debug)]
pub enum Msg {
    Progress(f32, String),
    Done(Result<(), String>),
}

type Tx<'a> = Option<&'a Sender<Msg>>;

pub fn report(tx: Tx<'_>, pct: f32, text: &str) {
    if let Some(tx) = tx {
        let _ = tx.send(Msg::Progress(pct, text.to_string()));
    }
}

pub fn default_dir() -> PathBuf {
    let pf = std::env::var_os("ProgramFiles")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
    pf.join("StatAura")
}

pub fn log(line: &str) {
    let path = std::env::temp_dir().join("StatAura-setup.log");
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("new");
    fs::write(&tmp, bytes).with_context(|| format!("写入 {}", tmp.display()))?;
    if path.exists() {
        fs::remove_file(path)
            .with_context(|| format!("无法覆盖 {}，请先关闭正在运行的 StatAura", path.display()))?;
    }
    fs::rename(&tmp, path).with_context(|| format!("替换 {}", path.display()))?;
    Ok(())
}

pub fn install(dir: &Path, tx: Tx<'_>) -> Result<()> {
    let dir = normalize_dir(dir)?;
    report(tx, 0.04, "创建安装目录");
    fs::create_dir_all(&dir).with_context(|| format!("创建 {}", dir.display()))?;

    report(tx, 0.18, "写入主程序");
    write_atomic(&dir.join("stataura.exe"), APP_EXE)?;

    report(tx, 0.42, "写入卸载程序");
    let me = std::env::current_exe().context("定位安装器自身")?;
    let uninstaller = dir.join("uninstall.exe");
    if me != uninstaller {
        let bytes = fs::read(&me).context("读取安装器自身")?;
        write_atomic(&uninstaller, &bytes)?;
    }

    let app = dir.join("stataura.exe");
    report(tx, 0.62, "创建快捷方式");
    shortcuts::create(&app)?;

    report(tx, 0.84, "注册文件关联");
    registry::install(&app, &dir)?;

    report(tx, 1.0, "安装完成");
    log(&format!("installed to {}", dir.display()));
    Ok(())
}

pub fn uninstall(dir: &Path, tx: Tx<'_>) -> Result<()> {
    let dir = normalize_dir(dir)?;
    let app = dir.join("stataura.exe");

    report(tx, 0.1, "检查运行中的 StatAura");
    if app.exists() {
        fs::remove_file(&app)
            .with_context(|| "请先关闭 StatAura，再点“卸载”重试（程序文件正在使用）")?;
    }

    report(tx, 0.35, "删除快捷方式");
    shortcuts::remove().context("删除快捷方式失败")?;

    report(tx, 0.6, "清理文件关联");
    registry::uninstall(&dir).context("清理文件关联失败")?;

    report(tx, 0.78, "删除安装文件");
    let me = std::env::current_exe().context("定位卸载程序自身")?;
    if me.parent() == Some(dir.as_path()) {
        // 当前卸载器就在安装目录里：先把它移出该目录，再整目录删除。
        if let Err(e) = self_replace::self_delete_outside_path(&dir) {
            log(&format!("self delete schedule: {e}"));
        }
    }
    if dir.exists() {
        let _ = fs::remove_file(dir.join("stataura.new"));
        let _ = fs::remove_file(dir.join("uninstall.new"));
        // 只删空目录：自定义安装目录里如果有用户文件，不能被卸载器一起清掉。
        if let Err(e) = fs::remove_dir(&dir) {
            if e.kind() != std::io::ErrorKind::NotFound {
                log(&format!("安装目录非空，已保留：{e}"));
            }
        }
    }

    report(tx, 1.0, "卸载完成");
    log(&format!("uninstalled {}", dir.display()));
    Ok(())
}

fn normalize_dir(dir: &Path) -> Result<PathBuf> {
    if dir.as_os_str().is_empty() {
        bail!("安装目录不能为空");
    }
    let dir = if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        std::env::current_dir()?.join(dir)
    };
    if dir.parent().is_none() {
        bail!("不能安装到磁盘根目录");
    }
    Ok(dir)
}

pub fn is_admin() -> bool {
    unsafe { windows::Win32::UI::Shell::IsUserAnAdmin().as_bool() }
}

pub fn current_dir_or_default() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(default_dir)
}
