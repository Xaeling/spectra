//! 构建脚本：把应用图标烧进 exe 资源段，并写入版本信息。
//!
//! 设计要点（这两条都是刻意的容错）：
//! - 条件是 `#[cfg(windows)]`，**不是** debug/release 判断。所以 `cargo run`
//!   （debug）在 Windows 上也会嵌入图标 —— 这正是用户能看到图标的场景。
//! - 图标文件缺失时静默跳过；`res.compile()` 失败只 `eprintln!` 警告，
//!   绝不 `panic!`。少一个图标不该让整个项目编不过。

fn main() {
    #[cfg(windows)]
    {
        // 仅在资源文件存在时嵌入，缺失不影响编译
        if std::path::Path::new("assets/icon.ico").exists() {
            let mut res = winres::WindowsResource::new();
            res.set_icon("assets/icon.ico");
            res.set("ProductName", "析光 Spectra");
            res.set("FileDescription", "析光 Spectra —— 数据统计与绘图");
            res.set("CompanyName", "Spectra");
            res.set("LegalCopyright", "Copyright © 2026 Spectra");
            if let Err(e) = res.compile() {
                eprintln!("[build] 图标嵌入失败（不影响运行）: {}", e);
            }
        }
    }
}
