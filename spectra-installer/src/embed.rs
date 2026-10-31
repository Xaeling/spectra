//! 单文件安装包：编译期嵌入的主程序与提权清单。
//!
//! `build.rs` 会把 `spectra.exe` 与 `app.manifest` 复制到 OUT_DIR，
//! 并通过 `cargo:rustc-env` 把路径传进来，这里用 `include_bytes!` 嵌进本 exe。
//!
//! 查找失败时 build.rs 会嵌入一个**空文件**并置 `SPECTRA_MAIN_OK` 为空，
//! 因此这里的两个函数都返回 `Option`：拿到 `None` 就说明这个 installer
//! 是用「没有主程序」的方式编出来的，不能拿去发给别人。

/// 嵌入的主程序字节；未嵌入（或嵌入了空占位）时为 None
pub fn main_exe() -> Option<&'static [u8]> {
    if option_env!("SPECTRA_MAIN_OK").is_some() {
        let b: &'static [u8] = include_bytes!(env!("SPECTRA_MAIN_EXE"));
        if b.is_empty() {
            None
        } else {
            Some(b)
        }
    } else {
        None
    }
}

/// 嵌入的提权清单（给 uninstall.exe 用）；缺失时为 None
pub fn manifest() -> Option<&'static [u8]> {
    if option_env!("SPECTRA_MANIFEST_OK").is_some() {
        let b: &'static [u8] = include_bytes!(env!("SPECTRA_MANIFEST"));
        if b.is_empty() {
            None
        } else {
            Some(b)
        }
    } else {
        None
    }
}

/// 是否带有内嵌主程序（供 UI / 自检提示使用）
pub fn has_main() -> bool {
    main_exe().is_some()
}
