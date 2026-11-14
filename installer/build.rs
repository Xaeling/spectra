//! 把主程序嵌入安装器，并写 Windows 图标/manifest。
//!
//! 构建脚本由 build_setup.ps1 调用，STATAURA_APP_EXE 指向刚构建的
//! target/release/stataura.exe；普通 cargo check 时回退到该默认路径。

use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let app = std::env::var("STATAURA_APP_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("../target/release/stataura.exe"));
    let app = app.canonicalize().unwrap_or(app);
    println!("cargo:rerun-if-env-changed=STATAURA_APP_EXE");
    println!("cargo:rerun-if-env-changed=STATAURA_SETUP_AS_INVOKER");
    println!("cargo:rerun-if-changed={}", app.display());
    if !app.is_file() {
        panic!(
            "缺少主程序：{}。请先 cargo build --release，或设置 STATAURA_APP_EXE。",
            app.display()
        );
    }
    std::fs::copy(&app, out.join("stataura.exe")).expect("复制主程序失败");

    // 版本号直接从主程序 Cargo.toml 取，避免安装器和 app 版本漂移。
    let manifest = std::fs::read_to_string("../Cargo.toml").unwrap_or_default();
    let version = manifest
        .lines()
        .find_map(|l| l.strip_prefix("version = \""))
        .and_then(|v| v.split('"').next())
        .unwrap_or("0.0.0");
    println!("cargo:rustc-env=APP_VERSION={version}");
    println!("cargo:rerun-if-changed=../Cargo.toml");
    println!("cargo:rerun-if-changed=../assets/icon.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let level = if std::env::var("STATAURA_SETUP_AS_INVOKER").is_ok() {
            "asInvoker"
        } else {
            "requireAdministrator"
        };
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="{level}" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
      <longPathAware xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">true</longPathAware>
    </windowsSettings>
  </application>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}}"/>
    </application>
  </compatibility>
</assembly>"#
        );
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../assets/icon.ico");
        res.set("ProductName", "StatAura");
        res.set("FileDescription", "StatAura Setup");
        res.set("OriginalFilename", "StatAura-Setup.exe");
        res.set("LegalCopyright", "MIT OR Apache-2.0");
        res.set_manifest(&xml);
        res.compile().expect("编译安装器 Windows 资源失败");
    }
}
