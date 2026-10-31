//! 全局常量

/// 显示名
pub const APP_NAME: &str = "析光 Spectra";

/// ARP 子键名（HKCU\...\Uninstall\{APP_KEY}）
pub const APP_KEY: &str = "Spectra";

pub const APP_VERSION: &str = "1.0.0";
pub const PUBLISHER: &str = "Spectra";

/// 文件关联 ProgId
pub const PROG_ID: &str = "Spectra.Document";

/// 需要关联的扩展名 -> 类型描述
pub const EXTENSIONS: &[(&str, &str)] = &[
    // 专有格式必须在这里登记，安装时才会写进注册表并出现在「打开方式」里
    (".sprx", "析光 Spectra 项目文件"),
    (".prism", "析光 Spectra 项目文件"),
    (".prism.json", "析光 Spectra JSON 项目"),
    // 注意：不要关联 .pzfx —— 那是真实 GraphPad Prism 的专有扩展名，
    // 抢过来会让装了正版 Prism 的用户双击 .pzfx 时被我们打开，属破坏性行为。
];

/// 更名前的旧标识，安装/卸载时一并清理，避免留下僵尸条目
pub const LEGACY_APP_KEYS: &[&str] = &["prism-rs", "PrismRS", "Prism"];
/// 更名前的旧 ProgId
pub const LEGACY_PROG_IDS: &[&str] = &["PrismRS.Document"];

/// 默认安装目录
pub const DEFAULT_INSTALL_DIR: &str = r"C:\Program Files\Spectra";

/// 主程序文件名
pub const MAIN_EXE: &str = "spectra.exe";

/// 卸载程序文件名
pub const UNINSTALL_EXE: &str = "uninstall.exe";

/// 更新安装时保留的条目（文件与目录名）
pub const PRESERVE: &[&str] = &[
    "settings.json",
    "config.json",
    "settings",
    "config",
    "user_data",
];

/// 从 payload 复制时一律跳过的文件名
pub const PAYLOAD_SKIP: &[&str] = &[
    "uninstall.exe",
    "spectra-installer.exe",
    "prism-installer.exe",
    "prism-installer.pdb",
    "spectra-installer.pdb",
    "prism-installer.exe.manifest",
    "spectra-installer.exe.manifest",
    "prism-rs.exe",
    "app.manifest",
    "build.bat",
    "cargo.toml",
    "cargo.lock",
    "readme.md",
];

pub fn arp_path() -> String {
    format!(
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{}",
        APP_KEY
    )
}

pub fn is_preserved(name: &str, extra: &[String]) -> bool {
    let lower = name.to_ascii_lowercase();
    if PRESERVE
        .iter()
        .any(|p| p.to_ascii_lowercase() == lower)
    {
        return true;
    }
    extra.iter().any(|p| p.to_ascii_lowercase() == lower)
}
