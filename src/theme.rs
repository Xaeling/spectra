//! 主题：浅色 / 深色 / 跟随系统。
//!
//! 「跟随系统」在 Windows 上读注册表 `AppsUseLightTheme`（DWORD：1 = 浅色，0 = 深色），
//! 这是 Windows 10/11 官方的深浅色开关，比猜环境变量可靠。
//! 其他平台读不到就回落浅色，并在设置里说明原因。

/// 系统当前的深浅色。**true = 应该用深色**。
///
/// 结果会被**缓存**：探测要走一次外部进程（`reg query`），
/// 而设置窗口每帧都会调用 `system_hint()` —— 不缓存就等于每帧启一个进程，
/// 界面会被拖到完全点不动（设置窗口点开后关不掉就是这个原因）。
pub fn system_prefers_dark() -> Option<bool> {
    // 1 秒轮询：走后台线程按秒刷新缓存值。
    // 放在这里而不是「每帧探测」，是因为每帧启一次 reg 进程会把界面拖死；
    // 放在这里而不是「只在启动时探测」，是因为用户可能在系统设置里随时切换。
    static POLL: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    POLL.get_or_init(|| {
        std::thread::spawn(|| loop {
            if let Some(v) = probe() {
                set_cached(v);
            }
            std::thread::sleep(std::time::Duration::from_millis(1000));
        });
    });
    cached().or_else(|| {
        let v = probe();
        if let Some(x) = v {
            set_cached(x);
        }
        v
    })
}

/// 缓存值。用 AtomicU8：0 = 未探测、1 = 浅色、2 = 深色。
static CACHED: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

fn cached() -> Option<bool> {
    match CACHED.load(std::sync::atomic::Ordering::Relaxed) {
        1 => Some(false),
        2 => Some(true),
        _ => None,
    }
}

fn set_cached(dark: bool) {
    CACHED.store(if dark { 2 } else { 1 }, std::sync::atomic::Ordering::Relaxed);
}

/// 真正去问系统。只会被 `system_prefers_dark()` 调用一次。
fn probe() -> Option<bool> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        // CREATE_NO_WINDOW：reg.exe 是控制台程序，默认启动会**闪出一个黑窗口**。
        // 「跟随系统」每秒轮询一次，就变成每秒闪一次 —— 必须带上这个标志。
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut cmd = Command::new("reg");
        cmd.args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "/v",
            "AppsUseLightTheme",
        ]);
        cmd.creation_flags(CREATE_NO_WINDOW);
        let out = cmd.output().ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&out.stdout);
        // 形如：    AppsUseLightTheme    REG_DWORD    0x0
        let v = s
            .lines()
            .find(|l| l.contains("AppsUseLightTheme"))?
            .split_whitespace()
            .last()?;
        let n = if let Some(hex) = v.strip_prefix("0x") {
            u32::from_str_radix(hex, 16).ok()?
        } else {
            v.parse::<u32>().ok()?
        };
        // AppsUseLightTheme = 0 → 深色
        Some(n == 0)
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// 按主题模式解析出最终的深色开关。
/// 0 = 浅色、1 = 深色、2 = 跟随系统（读不到时回落浅色）。
pub fn resolve_dark(mode: usize) -> bool {
    match mode {
        1 => true,
        2 => system_prefers_dark().unwrap_or(false),
        _ => false,
    }
}

/// 设置窗口里「跟随系统」一行后面的说明文字。
pub fn system_hint() -> &'static str {
    // **只读缓存，绝不在这里触发探测**。
    // 探测要走一次 reg 进程，而这个函数在设置窗口的 view() 里被调用 ——
    // 首次打开设置窗口时同步跑一次外部进程，界面会卡住几百毫秒
    // （表现就是「设置窗口弹出来特别慢」）。
    // 后台轮询线程启动后自然会把结果填进来，这里读不到就先说「检测中」。
    match cached() {
        Some(true) => "跟随系统：检测到系统为深色",
        Some(false) => "跟随系统：检测到系统为浅色",
        None => "跟随系统：正在检测…",
    }
}

/// 启动后台轮询（1 秒一次）。只在真正需要时调用，避免白白起线程。
///
/// 调用点：设置窗口打开时、以及主题模式切到「跟随系统」时。
#[allow(dead_code)]
pub fn ensure_polling() {
    system_prefers_dark();
}
