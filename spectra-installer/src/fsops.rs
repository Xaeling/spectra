//! 文件操作：复制、清理、保留文件备份与还原

use crate::model;
use std::io;
use std::path::{Path, PathBuf};

pub fn copy_dir_all(src: &Path, dst: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)?.flatten() {
        let t = dst.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir_all(&e.path(), &t)?;
        } else {
            std::fs::copy(e.path(), &t)?;
        }
    }
    Ok(())
}

/// 备份需要保留的条目（更新安装时保住用户设置）
pub fn backup_preserve(root: &Path, backup: &Path, extra: &[String]) -> io::Result<usize> {
    if !root.exists() {
        return Ok(0);
    }
    let _ = std::fs::remove_dir_all(backup);
    std::fs::create_dir_all(backup)?;

    let mut n = 0usize;
    for e in std::fs::read_dir(root)?.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !model::is_preserved(&name, extra) {
            continue;
        }
        let dst = backup.join(&name);
        if e.file_type()?.is_dir() {
            copy_dir_all(&e.path(), &dst)?;
        } else {
            std::fs::copy(&e.path(), &dst)?;
        }
        n += 1;
    }
    Ok(n)
}

pub fn restore_preserve(backup: &Path, root: &Path) -> io::Result<usize> {
    if !backup.exists() {
        return Ok(0);
    }
    std::fs::create_dir_all(root)?;
    let mut n = 0usize;
    for e in std::fs::read_dir(backup)?.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let dst = root.join(&name);
        if e.file_type()?.is_dir() {
            copy_dir_all(&e.path(), &dst)?;
        } else {
            std::fs::copy(&e.path(), &dst)?;
        }
        n += 1;
    }
    Ok(n)
}

/// 清空目录，跳过保留条目与自身 exe
pub fn clear_dir(root: &Path, self_exe: &Path, extra: &[String]) -> io::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    let self_name = self_exe
        .file_name()
        .map(|f| f.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    for e in std::fs::read_dir(root)?.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if model::is_preserved(&name, extra) {
            continue;
        }
        if name.to_ascii_lowercase() == self_name {
            continue;
        }
        let p = e.path();
        if e.file_type()?.is_dir() {
            let _ = std::fs::remove_dir_all(&p);
        } else {
            let _ = std::fs::remove_file(&p);
        }
    }
    Ok(())
}

/// 把 payload 目录内容复制到安装目录，跳过卸载程序与安装程序自身
pub fn copy_payload(src: &Path, dst: &Path, self_exe: &Path) -> io::Result<usize> {
    if !src.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("payload 目录不存在：{}", src.display()),
        ));
    }
    std::fs::create_dir_all(dst)?;

    let self_name = self_exe
        .file_name()
        .map(|f| f.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    let mut n = 0usize;
    for e in std::fs::read_dir(src)?.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let lower = name.to_ascii_lowercase();
        if lower == self_name {
            continue;
        }
        if model::PAYLOAD_SKIP
            .iter()
            .any(|s| s.to_ascii_lowercase() == lower)
        {
            continue;
        }
        let target = dst.join(&name);
        if e.file_type()?.is_dir() {
            copy_dir_all(&e.path(), &target)?;
        } else {
            std::fs::copy(&e.path(), &target)?;
            n += 1;
        }
    }
    Ok(n)
}

pub fn temp_backup_dir() -> PathBuf {
    std::env::temp_dir().join(format!("{}_preserve", model::APP_KEY))
}

/// 安装目录内的目录布局。
///
/// 主程序放在安装目录根下，其余是随程序分发的资源目录与程序要写入的目录。
/// 每个目录都写一份 README.txt —— 空目录会被打包工具和 git 丢掉，
/// 而主程序正是靠 `config/` 是否存在来判断「应用根」的。
pub const LAYOUT: &[(&str, &str)] = &[
    ("bin", "放置可执行文件及其依赖的 DLL。\n升级时只需替换本目录内容。\n"),
    (
        "resources",
        "放置字体、图标、示例数据与 i18n 词典等静态资源。\n程序启动时从这里加载，缺失时用内置默认值。\n",
    ),
    (
        "config",
        "放置随程序分发的默认配置 default.json。\n用户自己的设置通常写在 %APPDATA%\\Spectra 下。\n",
    ),
    (
        "scripts",
        "放置导出后处理脚本的描述文件（*.json）。\n格式：{\"id\",\"name\",\"desc\",\"command\"}，command 里可用 {out} 引用导出文件。\n",
    ),
    (
        "plugins",
        "放置插件：每个插件一个子目录，内含 manifest.json。\n格式：{\"id\",\"name\",\"version\",\"provides\":[],\"enabled\":true}\n",
    ),
    ("data", "放置示例项目与导入文件的默认落盘目录。\n"),
    ("logs", "运行日志目录。\n"),
];

/// 在安装目录里建出完整目录布局（已存在则跳过，不覆盖内容）。
/// 返回新建的目录数。
pub fn create_layout(dir: &std::path::Path) -> std::io::Result<usize> {
    let mut n = 0;
    for (name, note) in LAYOUT {
        let d = dir.join(name);
        if !d.is_dir() {
            std::fs::create_dir_all(&d)?;
            n += 1;
        }
        let f = d.join("README.txt");
        if !f.is_file() {
            let _ = std::fs::write(&f, note);
        }
    }
    Ok(n)
}
