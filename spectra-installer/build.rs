//! 构建脚本：把主程序与提权清单在**编译期**嵌入安装程序。
//!
//! 这样 `spectra-installer.exe` 单独一个文件就能展开出全部内容，
//! 不再需要旁边再放一份 spectra.exe。
//!
//! 查找顺序：
//!   1. 环境变量 SPECTRA_EXE（直接给 exe 路径；SPECTRA_EXE_LEGACY 仍兼容）
//!   2. 环境变量 SPECTRA_MAIN_DIR（给目录；SPECTRA_MAIN_DIR 仍兼容）
//!   3. 从 CARGO_MANIFEST_DIR 逐级向上找 `<dir>/prism-rs/target/release/spectra.exe`
//!      （默认是 ../prism-rs，所以同级放置时自动命中）
//!
//! 找不到时不会让编译失败，而是嵌入一个空占位并打出 cargo:warning ——
//! 这样 installer 仍能编译出来，但运行时会明确报错而不是装出空壳。

use std::path::{Path, PathBuf};

fn main() {
    // ---- 图标（仅 Windows，缺失不影响编译）----
    #[cfg(windows)]
    {
        if std::path::Path::new("assets/icon.ico").exists() {
            let mut res = winres::WindowsResource::new();
            res.set_icon("assets/icon.ico");
            res.set("ProductName", "Spectra Installer");
            res.set("FileDescription", "Spectra Setup / Uninstall");
            res.set("CompanyName", "Spectra");
            if let Err(e) = res.compile() {
                eprintln!("[build] icon embed failed (ignored): {}", e);
            }
        }
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=app.manifest");

    let out = PathBuf::from(
        std::env::var("OUT_DIR").expect("OUT_DIR not set"),
    );

    // ---- 主程序 ----
    let found = find_main_exe();
    let dst = out.join("spectra.exe");
    let ok = match found.as_deref() {
        Some(src) if src.is_file() => {
            println!("cargo:rerun-if-changed={}", src.display());
            std::fs::copy(src, &dst).is_ok()
        }
        _ => false,
    };
    if !ok {
        // include_bytes! 需要一个真实存在的文件，写空占位保证编译能过
        let _ = std::fs::write(&dst, b"");
        println!(
            "cargo:warning=spectra.exe NOT found - installer will be built WITHOUT the main program. \
             Run build.bat (it builds the main program first) or set SPECTRA_EXE=<path>."
        );
    } else {
        println!(
            "cargo:warning=embedded main program: {}",
            found.as_deref().unwrap().display()
        );
    }
    println!("cargo:rustc-env=SPECTRA_MAIN_EXE={}", slash(&dst));
    if ok {
        println!("cargo:rustc-env=SPECTRA_MAIN_OK=1");
    }

    // ---- 提权清单（卸载程序同样需要管理员，用于删 Program Files）----
    let mf_dst = out.join("app.manifest");
    let mf_ok = std::fs::copy("app.manifest", &mf_dst).is_ok();
    if !mf_ok {
        let _ = std::fs::write(&mf_dst, b"");
    }
    println!("cargo:rustc-env=SPECTRA_MANIFEST={}", slash(&mf_dst));
    if mf_ok {
        println!("cargo:rustc-env=SPECTRA_MANIFEST_OK=1");
    }
}

/// Windows 路径里的反斜杠在字符串字面量里是转义符，统一换成正斜杠
fn slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

fn find_main_exe() -> Option<PathBuf> {
    // 1) 直接指定 exe（SPECTRA_EXE_LEGACY 是旧名，保留兼容）
    for var in ["SPECTRA_EXE", "SPECTRA_EXE_LEGACY"] {
        if let Ok(p) = std::env::var(var) {
            let p = PathBuf::from(p);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    // 2) 指定目录
    for var in ["SPECTRA_MAIN_DIR", "PRISM_MAIN_DIR"] {
        if let Ok(d) = std::env::var(var) {
            for n in ["spectra.exe", "prism-rs.exe"] {
                let p = PathBuf::from(&d).join(n);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    // 3) 从 crate 目录逐级向上找 <dir>/prism-rs/target/release/{spectra,prism-rs}.exe
    //    源目录名仍是 prism-rs，所以这里两个名字都试
    let mut dir = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default(),
    );
    for _ in 0..4 {
        for d in ["prism-rs", "spectra"] {
            for n in ["spectra.exe", "prism-rs.exe"] {
                let c = dir.join(d).join("target").join("release").join(n);
                if c.is_file() {
                    return Some(c);
                }
            }
        }
        if !dir.pop() {
            break;
        }
    }
    None
}
