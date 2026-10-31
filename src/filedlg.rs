//! 系统文件选择框（打开 / 保存 / 选目录）。
//!
//! 走 `rfd`（Rusty File Dialog）：Windows 上是原生 `GetOpenFileName` /
//! `IFileDialog`，macOS 是 `NSOpenPanel`，Linux 依次尝试 zenity / kdialog / GTK。
//!
//! # 为什么不再用固定文件名
//! 之前「保存」硬写 `project.sprx`、「导出」硬写 `spectra_chart.svg`，
//! 都落在**程序当前工作目录**。用户既不知道文件去了哪，
//! 也无法选择存到哪个文件夹 —— 现在统一弹系统对话框让用户自己选。
//!
//! 对话框被取消、或当前环境根本没有图形文件对话框时返回 `None`，
//! 调用方按「用户取消」处理，不去写一个没人知道在哪的文件。

/// 打开单个已存在的文件。
///
/// `filters` 是 `(显示名, 扩展名)` 列表，例如 `[("工程", "sprx")]`。
pub fn open_file(title: &str, filters: &[(&str, &str)]) -> Option<String> {
    let mut d = rfd::FileDialog::new().set_title(title);
    for (name, ext) in filters {
        d = d.add_filter(*name, &[*ext]);
    }
    d.pick_file().map(|p| p.to_string_lossy().to_string())
}

/// 保存文件：只负责让用户挑一个路径，真正写盘由调用方做。
///
/// `default_name` 是建议的文件名（含扩展名），决定保存对话框里的初始名字
/// 与默认扩展名。
pub fn save_file(title: &str, default_name: &str, filters: &[(&str, &str)]) -> Option<String> {
    let mut d = rfd::FileDialog::new().set_title(title).set_file_name(default_name);
    for (name, ext) in filters {
        d = d.add_filter(*name, &[*ext]);
    }
    d.save_file().map(|p| p.to_string_lossy().to_string())
}

/// 选择目录（设置 ▸ 文件位置 里改默认打开 / 保存目录时用）。
pub fn pick_folder(title: &str) -> Option<String> {
    rfd::FileDialog::new()
        .set_title(title)
        .pick_folder()
        .map(|p| p.to_string_lossy().to_string())
}

/// 工程文件的扩展名清单（保存时让用户二选一）。
pub const PROJECT_FILTERS: [(&str, &str); 2] = [("析光 工程 (.sprx)", "sprx"), ("Prism 兼容包 (.prism)", "prism")];

/// 打开工程时的扩展名清单：比保存多一个「所有文件」，
/// 因为按**内容**判定格式，改名过的工程也要能打开。
pub const OPEN_FILTERS: [(&str, &str); 3] = [
    ("析光 工程 (.sprx)", "sprx"),
    ("Prism 兼容包 (.prism)", "prism"),
    ("所有文件", "*"),
];
