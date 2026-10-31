//! 全局设置。
//!
//! 设置按析光的 Preferences 分为七类（与 `ui::settings::TABS` 一一对应）：
//! 外观 / 视图设置 / 文件与打印 / 新建图表 / 分析 / 文件位置 / 发送到 Office。
//!
//! 存盘用纯文本 `key=value`，落在用户可写的配置目录（见 `Settings::dir()`）。
//! 不引外部依赖，也不写注册表。

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// 全局唯一的活动设置。启动时 `load()`，之后各处直接读。
static LIVE: OnceLock<Mutex<Settings>> = OnceLock::new();

pub fn live() -> &'static Mutex<Settings> {
    LIVE.get_or_init(|| {
        let s = Settings::load();
        // 首次运行（或旧位置迁移后）落一份默认文件：用户能在固定位置找到它，
        // 手改也生效，不至于「设置了但不知道存哪」。写失败不阻塞启动。
        if !Settings::path().exists() {
            let _ = s.save();
        }
        Mutex::new(s)
    })
}

/// 取一份设置快照（避免长期持锁）。不适合每帧调用 —— 会克隆整个结构。
pub fn get() -> Settings {
    live().lock().map(|s| s.clone()).unwrap_or_default()
}

/// 只借不改地读一个字段。
///
/// UI 每帧会调用几十上百次（字号、圆角、强调色…），用 `get()` 克隆整个
/// 结构就太贵了 —— 这里只加一次锁、读一个值就走。
pub fn with<R>(f: impl FnOnce(&Settings) -> R) -> R {
    match live().lock() {
        Ok(g) => f(&g),
        Err(p) => f(p.get_ref()),
    }
}

/// 写入活动设置（所有修改都走这里，保证 UI 读到的和存盘的是同一份）。
pub fn replace(next: Settings) {
    if let Ok(mut s) = live().lock() {
        *s = next;
    }
}

/// 落盘。
pub fn save_current() {
    let s = get();
    let _ = s.save();
}

/// 节流落盘。
///
/// 设置窗口里有一批「改了立刻生效」的项（主题、强调色、字号、圆角…），拖滑块时
/// 每帧都会改一次，逐帧写盘毫无意义。这里把落盘压到最多每 700ms 一次；
/// 最后一次改动由 `SettingsOk` / `SettingsApply` 强制写盘兜底，不会丢。
pub fn save_debounced() {
    const GAP: Duration = Duration::from_millis(700);
    static LAST: OnceLock<Mutex<Instant>> = OnceLock::new();
    let last = LAST.get_or_init(|| Mutex::new(Instant::now() - GAP));
    let mut g = match last.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    if g.elapsed() < GAP {
        return;
    }
    *g = Instant::now();
    drop(g);
    save_current();
}

// ── 选项表 ──────────────────────────────────────────────────────────────────
// 这些表同时供下拉列表的文案和存盘的索引使用，索引即顺序。

pub const THEMES: [&str; 3] = ["浅色", "深色", "跟随系统"];
pub const FONT_FAMILIES: [&str; 5] = ["Mi Sans", "系统默认", "Segoe UI", "微软雅黑", "自定义（导入）"];
pub const ACCENTS: [&str; 8] = [
    "#2B6FD4", // 蓝（默认）
    "#0F766E", // 青
    "#7C3AED", // 紫
    "#B91C1C", // 红
    "#C2410C", // 橙
    "#15803D", // 绿
    "#9333EA", // 品红
    "#334155", // 石墨
];
pub const DATE_FORMATS: [&str; 4] = ["年-月-日", "月/日/年", "日.月.年", "ISO 8601"];
pub const DATE_SEPS: [&str; 3] = ["-", "/", "."];
pub const P_FORMATS: [&str; 4] = ["0.0000", "0.000", "< 0.0001", "科学记数"];
pub const SCI_STYLES: [&str; 3] = ["1.23e-4", "1.23×10⁻⁴", "1.23E-4"];
pub const REPORT_FORMATS: [&str; 3] = ["表格", "段落", "APA"];
pub const OUTLIER_METHODS: [&str; 4] = ["无", "Grubbs", "Tukey 1.5 IQR", "ROUT (Q=1%)"];
pub const ENCODINGS: [&str; 3] = ["UTF-8", "UTF-8 with BOM", "GBK"];
pub const ZOOM_POLICIES: [&str; 3] = ["重绘（清晰）", "拉伸（快）", "先拉伸后重绘"];
pub const OFFICE_STYLES: [&str; 3] = ["简约", "学术", "演示"];
pub const CONF_LEVELS: [&str; 3] = ["90%", "95%", "99%"];
pub const BRACKET_DEFAULTS: [&str; 4] = ["无", "相邻两组", "全部两两", "都与第一组比"];

/// 设置项。字段顺序与界面分组一致，便于对照。
#[derive(Clone, Debug)]
pub struct Settings {
    // ── 0 外观 ──
    pub theme: usize,
    pub font_family: usize,
    /// UI 缩放倍率：字号、间距、圆角一起放大
    pub font_scale: f32,
    /// 导入的 TTF 文件名（相对程序目录）
    pub custom_font: String,
    pub high_contrast: bool,
    /// 圆角倍率（1.0 = 设计默认值）
    pub radius: f32,
    pub animate: bool,
    /// 窗口不透明度（需要窗口层支持，当前仅存值）
    pub opacity: f32,
    /// 无障碍：更大的点击区与更强的描边
    pub a11y: bool,
    pub accent: usize,

    // ── 1 视图设置 ──
    /// 新建表格默认显示的行 / 列数
    pub grid_rows: usize,
    pub grid_cols: usize,
    pub nav_width: f32,
    pub show_navigator: bool,
    pub show_inspector: bool,
    pub show_status: bool,
    pub recent_count: usize,
    /// 动画速度（毫秒），同时也是 UI 反馈的节奏
    pub anim_ms: usize,
    pub zoom_policy: usize,

    // ── 2 文件与打印 ──
    pub open_dir: String,
    pub save_dir: String,
    pub date_fmt: usize,
    pub date_sep: usize,
    pub author: String,
    pub auto_backup: bool,
    pub backup_min: usize,
    /// 建表前先要求存盘（防止新建覆盖未保存内容）
    pub save_before_table: bool,
    pub margin: f32,
    pub header: String,
    pub footer: String,

    // ── 3 新建图表 ──
    pub def_kind: usize,
    pub def_colormap: usize,
    /// 默认起始色序（在配色方案内偏移）
    pub def_color_idx: usize,
    pub def_points: bool,
    pub def_line: bool,
    pub def_error: bool,
    pub def_bracket: usize,
    pub def_bracket_label: String,
    /// 显著性标注的默认写法：0 = 只星号，1 = 只 P 值，2 = 星号 + P 值
    pub bracket_label_fmt: usize,

    // ── 4 分析 ──
    pub conf_level: usize,
    pub alpha: f32,
    pub decimals: usize,
    /// 复制时的小数位：0 尽可能多 / 1 两位 / 2 四位 / 3 六位
    pub copy_decimals: usize,
    /// 复制时的小数分隔符：0 句点 / 1 逗号（此时千位分隔符改用句点）
    pub copy_decimal_sep: usize,
    /// 复制时排除值的处理：0 加 * 后缀 / 1 置空 / 2 原样
    pub copy_excluded: usize,
    pub p_format: usize,
    pub sci_style: usize,
    pub auto_sci: bool,
    pub seed: u64,
    pub residual: bool,
    pub report_fmt: usize,
    pub outlier: usize,
    pub encoding: usize,
    /// 事后多重比较方法（分析层枚举，运行时经 `mm_conv` 转到统计层）
    pub multi_method: crate::analysis::MultiMethod,
    /// 非线性回归默认模型
    pub nl_model: crate::analysis::ModelId,

    // ── 6 发送到 Office ──
    pub office_title: String,
    pub office_author: String,
    pub office_style: usize,
    pub office_colormap: usize,
    pub office_font: String,
    pub office_font_size: f32,
    /// 导出位图的宽度（像素）
    pub export_width: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: 0,
            font_family: 0,
            font_scale: 1.0,
            custom_font: String::new(),
            high_contrast: false,
            radius: 1.0,
            animate: true,
            opacity: 1.0,
            a11y: false,
            accent: 0,

            grid_rows: 40,
            grid_cols: 26,
            nav_width: 240.0,
            show_navigator: true,
            show_inspector: true,
            show_status: true,
            recent_count: 8,
            anim_ms: 180,
            zoom_policy: 0,

            open_dir: String::new(),
            save_dir: String::new(),
            date_fmt: 0,
            date_sep: 0,
            author: String::new(),
            auto_backup: false,
            backup_min: 10,
            save_before_table: false,
            margin: 18.0,
            header: String::new(),
            footer: String::new(),

            def_kind: 0,
            def_colormap: 0,
            def_color_idx: 0,
            def_points: true,
            def_line: false,
            def_error: true,
            def_bracket: 0,
            def_bracket_label: "*".into(),
            bracket_label_fmt: 2,

            conf_level: 1,
            alpha: 0.05,
            decimals: 6,
            copy_decimals: 0,
            copy_decimal_sep: 0,
            copy_excluded: 0,
            p_format: 0,
            sci_style: 0,
            auto_sci: false,
            seed: 0x9E3779B9,
            residual: false,
            report_fmt: 0,
            outlier: 0,
            encoding: 0,
            multi_method: crate::analysis::MultiMethod::Tukey,
            nl_model: crate::analysis::ModelId::ExpDecay1,

            office_title: String::new(),
            office_author: String::new(),
            office_style: 1,
            office_colormap: 0,
            office_font: "Times New Roman".into(),
            office_font_size: 12.0,
            export_width: 1600.0,
        }
    }
}

const FILE: &str = "spectra_settings.txt";

impl Settings {
    /// 设置文件所在的目录。
    ///
    /// 以前这里用的是 `current_dir()` —— 那是**当前工作目录**，不是程序目录：
    /// 双击快捷方式启动时它可能是 System32 或用户目录，装进 Program Files 之后
    /// 又不可写，于是设置要么散落各处、要么静默写失败（表现就是「改了没保存」）。
    /// 现在固定到用户可写的位置，按优先级：
    ///
    /// 1. `SPECTRA_CONFIG_DIR` 环境变量 —— 便携版 / 多实例 / 测试用，最高优先级
    /// 2. Windows：`%APPDATA%\Spectra`
    /// 3. 其他平台：`~/.config/spectra`
    /// 4. 以上都拿不到（无 HOME/APPDATA）：exe 同级的 `userdata/`
    pub fn dir() -> std::path::PathBuf {
        if let Ok(d) = std::env::var("SPECTRA_CONFIG_DIR") {
            let d = d.trim();
            if !d.is_empty() {
                return std::path::PathBuf::from(d);
            }
        }
        #[cfg(windows)]
        {
            if let Ok(a) = std::env::var("APPDATA") {
                if !a.trim().is_empty() {
                    return std::path::PathBuf::from(a).join("Spectra");
                }
            }
        }
        #[cfg(not(windows))]
        {
            if let Ok(h) = std::env::var("HOME") {
                if !h.trim().is_empty() {
                    return std::path::PathBuf::from(h).join(".config").join("spectra");
                }
            }
        }
        std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|p| p.to_path_buf()))
            .unwrap_or_default()
            .join("userdata")
    }

    pub fn path() -> std::path::PathBuf {
        Self::dir().join(FILE)
    }

    /// 旧位置（工作目录）。只用于把老设置读回来，绝不是新的写入目标。
    fn legacy_path() -> std::path::PathBuf {
        std::env::current_dir().unwrap_or_default().join(FILE)
    }

    pub fn load() -> Self {
        let mut s = Self::default();
        // 新位置优先；没有就回退旧位置 —— 老用户的设置不会因这次搬家而丢失
        // （读到后会在 `live()` 里写回新位置，完成迁移）。
        let txt = std::fs::read_to_string(Self::path())
            .or_else(|_| std::fs::read_to_string(Self::legacy_path()));
        let Ok(txt) = txt else {
            return s;
        };
        for line in txt.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let b = |dst: &mut bool| *dst = v == "1" || v == "true";
            let u = |dst: &mut usize| {
                if let Ok(x) = v.parse() {
                    *dst = x;
                }
            };
            let f = |dst: &mut f32| {
                if let Ok(x) = v.parse() {
                    *dst = x;
                }
            };
            let st = |dst: &mut String| *dst = v.to_string();
            match k {
                "theme" => u(&mut s.theme),
                "font_family" => u(&mut s.font_family),
                "font_scale" => f(&mut s.font_scale),
                "custom_font" => st(&mut s.custom_font),
                "high_contrast" => b(&mut s.high_contrast),
                "radius" => f(&mut s.radius),
                "animate" => b(&mut s.animate),
                "opacity" => f(&mut s.opacity),
                "a11y" => b(&mut s.a11y),
                "accent" => u(&mut s.accent),

                "grid_rows" => u(&mut s.grid_rows),
                "grid_cols" => u(&mut s.grid_cols),
                "nav_width" => f(&mut s.nav_width),
                "show_navigator" => b(&mut s.show_navigator),
                "show_inspector" => b(&mut s.show_inspector),
                "show_status" => b(&mut s.show_status),
                "recent_count" => u(&mut s.recent_count),
                "anim_ms" => u(&mut s.anim_ms),
                "zoom_policy" => u(&mut s.zoom_policy),

                "open_dir" => st(&mut s.open_dir),
                "save_dir" => st(&mut s.save_dir),
                "date_fmt" => u(&mut s.date_fmt),
                "date_sep" => u(&mut s.date_sep),
                "author" => st(&mut s.author),
                "auto_backup" => b(&mut s.auto_backup),
                "backup_min" => u(&mut s.backup_min),
                "save_before_table" => b(&mut s.save_before_table),
                "margin" => f(&mut s.margin),
                "header" => st(&mut s.header),
                "footer" => st(&mut s.footer),

                "def_kind" => u(&mut s.def_kind),
                "def_colormap" => u(&mut s.def_colormap),
                "def_color_idx" => u(&mut s.def_color_idx),
                "def_points" => b(&mut s.def_points),
                "def_line" => b(&mut s.def_line),
                "def_error" => b(&mut s.def_error),
                "def_bracket" => u(&mut s.def_bracket),
                "def_bracket_label" => st(&mut s.def_bracket_label),
                "bracket_label_fmt" => u(&mut s.bracket_label_fmt),

                "conf_level" => u(&mut s.conf_level),
                "alpha" => f(&mut s.alpha),
                "decimals" => u(&mut s.decimals),
                "copy_decimals" => u(&mut s.copy_decimals),
                "copy_decimal_sep" => u(&mut s.copy_decimal_sep),
                "copy_excluded" => u(&mut s.copy_excluded),
                "p_format" => u(&mut s.p_format),
                "sci_style" => u(&mut s.sci_style),
                "auto_sci" => b(&mut s.auto_sci),
                "seed" => {
                    if let Ok(x) = v.parse() {
                        s.seed = x;
                    }
                }
                "residual" => b(&mut s.residual),
                "report_fmt" => u(&mut s.report_fmt),
                "outlier" => u(&mut s.outlier),
                "encoding" => u(&mut s.encoding),
                "multi_method" => {
                    let all = crate::analysis::MultiMethod::all();
                    s.multi_method = all.get(v.parse().unwrap_or(0)).copied().unwrap_or(all[0]);
                }
                "nl_model" => {
                    let all = crate::analysis::ModelId::all();
                    s.nl_model = all.get(v.parse().unwrap_or(0)).copied().unwrap_or(all[0]);
                }

                "office_title" => st(&mut s.office_title),
                "office_author" => st(&mut s.office_author),
                "office_style" => u(&mut s.office_style),
                "office_colormap" => u(&mut s.office_colormap),
                "office_font" => st(&mut s.office_font),
                "office_font_size" => f(&mut s.office_font_size),
                "export_width" => f(&mut s.export_width),
                _ => {}
            }
        }
        s
    }

    pub fn save(&self) -> Result<(), String> {
        let mut out = String::new();
        let mut p = |k: &str, v: String| {
            out.push_str(k);
            out.push('=');
            out.push_str(&v);
            out.push('\n');
        };
        let b = |x: bool| (x as u8).to_string();
        p("theme", self.theme.to_string());
        p("font_family", self.font_family.to_string());
        p("font_scale", format!("{}", self.font_scale));
        p("custom_font", self.custom_font.clone());
        p("high_contrast", b(self.high_contrast));
        p("radius", format!("{}", self.radius));
        p("animate", b(self.animate));
        p("opacity", format!("{}", self.opacity));
        p("a11y", b(self.a11y));
        p("accent", self.accent.to_string());

        p("grid_rows", self.grid_rows.to_string());
        p("grid_cols", self.grid_cols.to_string());
        p("nav_width", format!("{}", self.nav_width));
        p("show_navigator", b(self.show_navigator));
        p("show_inspector", b(self.show_inspector));
        p("show_status", b(self.show_status));
        p("recent_count", self.recent_count.to_string());
        p("anim_ms", self.anim_ms.to_string());
        p("zoom_policy", self.zoom_policy.to_string());

        p("open_dir", self.open_dir.clone());
        p("save_dir", self.save_dir.clone());
        p("date_fmt", self.date_fmt.to_string());
        p("date_sep", self.date_sep.to_string());
        p("author", self.author.clone());
        p("auto_backup", b(self.auto_backup));
        p("backup_min", self.backup_min.to_string());
        p("save_before_table", b(self.save_before_table));
        p("margin", format!("{}", self.margin));
        p("header", self.header.clone());
        p("footer", self.footer.clone());

        p("def_kind", self.def_kind.to_string());
        p("def_colormap", self.def_colormap.to_string());
        p("def_color_idx", self.def_color_idx.to_string());
        p("def_points", b(self.def_points));
        p("def_line", b(self.def_line));
        p("def_error", b(self.def_error));
        p("def_bracket", self.def_bracket.to_string());
        p("def_bracket_label", self.def_bracket_label.clone());
        p("bracket_label_fmt", self.bracket_label_fmt.to_string());

        p("conf_level", self.conf_level.to_string());
        p("alpha", format!("{}", self.alpha));
        p("decimals", self.decimals.to_string());
        p("copy_decimals", self.copy_decimals.to_string());
        p("copy_decimal_sep", self.copy_decimal_sep.to_string());
        p("copy_excluded", self.copy_excluded.to_string());
        p("p_format", self.p_format.to_string());
        p("sci_style", self.sci_style.to_string());
        p("auto_sci", b(self.auto_sci));
        p("seed", self.seed.to_string());
        p("residual", b(self.residual));
        p("report_fmt", self.report_fmt.to_string());
        p("outlier", self.outlier.to_string());
        p("encoding", self.encoding.to_string());

        // 多重比较 / 非线性模型是枚举，落盘转索引（与各自 all() 的顺序绑定）。
        // 用 discriminant 比较，不必给枚举派生 PartialEq。
        let mi = crate::analysis::MultiMethod::all()
            .iter()
            .position(|m| std::mem::discriminant(m) == std::mem::discriminant(&self.multi_method))
            .unwrap_or(0);
        let ni = crate::analysis::ModelId::all()
            .iter()
            .position(|m| std::mem::discriminant(m) == std::mem::discriminant(&self.nl_model))
            .unwrap_or(0);
        p("multi_method", mi.to_string());
        p("nl_model", ni.to_string());

        p("office_title", self.office_title.clone());
        p("office_author", self.office_author.clone());
        p("office_style", self.office_style.to_string());
        p("office_colormap", self.office_colormap.to_string());
        p("office_font", self.office_font.clone());
        p("office_font_size", format!("{}", self.office_font_size));
        p("export_width", format!("{}", self.export_width));

        // 目录可能不存在（首次运行 / 用户手删了）—— 不建目录的话写盘必然失败，
        // 而调用方大多 `let _ =`，失败就静默吞掉了。
        let dir = Self::dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(FILE), out).map_err(|e| e.to_string())
    }

    /// 深色的判定：跟随系统时读系统偏好。
    pub fn is_dark(&self) -> bool {
        crate::theme::resolve_dark(self.theme)
    }

    /// 置信水平对应的 z 值（双侧）。
    pub fn z(&self) -> f64 {
        match self.conf_level {
            0 => 1.645,
            2 => 2.576,
            _ => 1.960,
        }
    }

    /// 按设置的日期格式与分隔符生成今天的日期串。
    #[allow(dead_code)]
    pub fn today(&self) -> String {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        // 把 Unix 秒转成年月日（不引 chrono，用标准算法）
        let days = (t / 86400) as i64 + 719468;
        let era = if days >= 0 { days } else { days - 146096 } / 146097;
        let doe = (days - era * 146097) as i64;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let yy = if m <= 2 { y + 1 } else { y } as u32;
        let sep = DATE_SEPS
            .get(self.date_sep)
            .copied()
            .unwrap_or("-");
        match self.date_fmt {
            1 => format!("{:02}{}{:02}{}{:04}", m, sep, d, sep, yy),
            2 => format!("{:02}{}{:02}{}{:04}", d, sep, m, sep, yy),
            3 => format!("{:04}{}{:02}{}{:02}", yy, sep, m, sep, d),
            _ => format!("{:04}{}{:02}{}{:02}", yy, sep, m, sep, d),
        }
    }
}

// ── 随机种子 ───────────────────────────────────────────────────────────────

/// 随机一个新种子（设置窗口里骰子按钮用）。
///
/// 不引 rand：只需要一个「换个值」的动作，SystemTime + 单调计数 + splitmix64 足够。
/// 纯时间戳的高位几乎不变，不搅拌的话连点几次看不出区别。
pub fn random_seed() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static CTR: AtomicU64 = AtomicU64::new(0);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E3779B9);
    let c = CTR.fetch_add(1, Ordering::Relaxed);
    let mut x = t ^ c.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    if x == 0 { 0x9E37_79B9 } else { x }
}

// ── 本地化的选项表 ─────────────────────────────────────────────────────────
// 存盘存的是索引，界面显示的是文案，所以中英文各给一份。

impl Settings {
    pub fn themes(lang: crate::i18n::Lang) -> Vec<String> {
        let (zh, en) = (
            ["浅色", "深色", "跟随系统"],
            ["Light", "Dark", "Follow system"],
        );
        pick(zh.as_ref(), en.as_ref(), lang)
    }
    pub fn font_families(lang: crate::i18n::Lang) -> Vec<String> {
        let zh = ["Mi Sans", "系统默认", "Segoe UI", "微软雅黑", "自定义（导入）"];
        let en = [
            "Mi Sans",
            "System default",
            "Segoe UI",
            "Microsoft YaHei",
            "Custom (imported)",
        ];
        pick(zh.as_ref(), en.as_ref(), lang)
    }
    pub fn accents() -> [&'static str; 8] {
        ACCENTS
    }
    pub fn accent_names(lang: crate::i18n::Lang) -> Vec<String> {
        let zh = ["蓝", "青", "紫", "红", "橙", "绿", "品红", "石墨"];
        let en = [
            "Blue",
            "Teal",
            "Violet",
            "Red",
            "Orange",
            "Green",
            "Magenta",
            "Graphite",
        ];
        pick(zh.as_ref(), en.as_ref(), lang)
    }
    pub fn zoom_policies(lang: crate::i18n::Lang) -> Vec<String> {
        pick(
            &["重绘（清晰）", "拉伸（快）", "先拉伸后重绘"],
            &[
                "Redraw (sharp)",
                "Stretch (fast)",
                "Stretch then redraw",
            ],
            lang,
        )
    }
    pub fn date_formats(lang: crate::i18n::Lang) -> Vec<String> {
        pick(
            &["年-月-日", "月/日/年", "日.月.年", "ISO 8601"],
            &[
                "Year-Month-Day",
                "Month/Day/Year",
                "Day.Month.Year",
                "ISO 8601",
            ],
            lang,
        )
    }
    pub fn date_seps() -> Vec<String> {
        vec!["-".into(), "/".into(), ".".into()]
    }
    pub fn p_formats(lang: crate::i18n::Lang) -> Vec<String> {
        pick(
            &["0.0000", "0.000", "小于 0.0001", "科学记数"],
            &["0.0000", "0.000", "< 0.0001", "Scientific"],
            lang,
        )
    }
    pub fn sci_styles() -> Vec<String> {
        vec!["1.23e-4".into(), "1.23×10⁻⁴".into(), "1.23E-4".into()]
    }
    pub fn report_formats(lang: crate::i18n::Lang) -> Vec<String> {
        pick(&["表格", "段落", "APA"], &["Table", "Paragraph", "APA"], lang)
    }
    pub fn outlier_methods(lang: crate::i18n::Lang) -> Vec<String> {
        pick(
            &["无", "Grubbs", "Tukey 1.5 IQR", "ROUT (Q=1%)"],
            &["None", "Grubbs", "Tukey 1.5 IQR", "ROUT (Q=1%)"],
            lang,
        )
    }
    pub fn encodings() -> Vec<String> {
        vec![
            "UTF-8".into(),
            "UTF-8 with BOM".into(),
            "GBK".into(),
        ]
    }
    pub fn conf_levels() -> Vec<String> {
        vec!["90%".into(), "95%".into(), "99%".into()]
    }
    pub fn bracket_defaults(lang: crate::i18n::Lang) -> Vec<String> {
        pick(
            &["无", "相邻两组", "全部两两", "都与第一组比"],
            &[
                "None",
                "Adjacent pairs",
                "All pairs",
                "Each vs. first",
            ],
            lang,
        )
    }
    pub fn office_styles(lang: crate::i18n::Lang) -> Vec<String> {
        pick(&["简约", "学术", "演示"], &["Minimal", "Academic", "Presentation"], lang)
    }
    /// 图表类型名（与 `Msg::SetChartKind` 的索引一致）。
    pub fn chart_kinds(lang: crate::i18n::Lang) -> Vec<String> {
        use crate::i18n::K::*;
        let keys = [
            ChartBar,
            ChartScatter,
            ChartLine,
            ChartBox,
            ChartViolin,
            ChartHistogram,
            ChartPie,
            ChartHeatmap,
            ChartForest,
            ChartSurvival,
        ];
        keys.iter().map(|k| crate::i18n::t(lang, *k).to_string()).collect()
    }
}

/// 按语言挑一份文案。
fn pick(zh: &[&str], en: &[&str], lang: crate::i18n::Lang) -> Vec<String> {
    let src = if matches!(lang, crate::i18n::Lang::Zh) { zh } else { en };
    src.iter().map(|s| (*s).to_string()).collect()
}

/// 设置编辑消息。
///
/// 所有设置项共用 `Msg::Edit(...)` 一个变体，避免在 `Msg` 里塞进几十个
/// 只用于设置的分支 —— `update` 里一个 match 就能分发完。
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum Edit {
    Theme(usize),
    FontFamily(usize),
    FontScale(f32),
    CustomFont(String),
    HighContrast(bool),
    Radius(f32),
    Animate(bool),
    Opacity(f32),
    A11y(bool),
    Accent(usize),

    GridRows(f32),
    GridCols(f32),
    NavWidth(f32),
    ShowNavigator(bool),
    ShowInspector(bool),
    ShowStatus(bool),
    RecentCount(f32),
    AnimMs(f32),
    ZoomPolicy(usize),

    OpenDir(String),
    SaveDir(String),
    DateFmt(usize),
    DateSep(usize),
    Author(String),
    AutoBackup(bool),
    BackupMin(f32),
    SaveBeforeTable(bool),
    Margin(f32),
    Header(String),
    Footer(String),

    DefKind(usize),
    DefColormap(usize),
    DefColorIdx(usize),
    DefPoints(bool),
    DefLine(bool),
    DefError(bool),
    DefBracket(usize),
    DefBracketLabel(String),

    ConfLevel(usize),
    Alpha(f32),
    Decimals(f32),
    CopyDecimals(usize),
    CopyDecSep(usize),
    CopyExcluded(usize),
    PFormat(usize),
    SciStyle(usize),
    AutoSci(bool),
    Seed(String),
    Residual(bool),
    ReportFmt(usize),
    Outlier(usize),
    Encoding(usize),
    MultiMethod(usize),
    NonlinearModel(usize),

    OfficeTitle(String),
    OfficeAuthor(String),
    OfficeStyle(usize),
    OfficeColormap(usize),
    OfficeFont(String),
    OfficeFontSize(f32),
    ExportWidth(f32),
}

/// 把一条编辑消息施加到设置对象上。
pub fn apply(s: &mut Settings, e: Edit) {
    use Edit::*;
    match e {
        Theme(v) => s.theme = v.min(THEMES.len() - 1),
        FontFamily(v) => s.font_family = v.min(FONT_FAMILIES.len() - 1),
        FontScale(v) => s.font_scale = v.clamp(0.85, 1.60),
        CustomFont(v) => s.custom_font = v,
        HighContrast(v) => s.high_contrast = v,
        Radius(v) => s.radius = v.clamp(0.0, 2.0),
        Animate(v) => s.animate = v,
        Opacity(v) => s.opacity = v.clamp(0.5, 1.0),
        A11y(v) => s.a11y = v,
        Accent(v) => s.accent = v.min(ACCENTS.len() - 1),

        GridRows(v) => s.grid_rows = (v as usize).clamp(5, 100_000),
        GridCols(v) => s.grid_cols = (v as usize).clamp(2, 100_000),
        NavWidth(v) => s.nav_width = v.clamp(150.0, 420.0),
        ShowNavigator(v) => s.show_navigator = v,
        ShowInspector(v) => s.show_inspector = v,
        ShowStatus(v) => s.show_status = v,
        RecentCount(v) => s.recent_count = (v as usize).clamp(0, 20),
        AnimMs(v) => s.anim_ms = (v as usize).clamp(0, 800),
        ZoomPolicy(v) => s.zoom_policy = v.min(ZOOM_POLICIES.len() - 1),

        OpenDir(v) => s.open_dir = v,
        SaveDir(v) => s.save_dir = v,
        DateFmt(v) => s.date_fmt = v.min(DATE_FORMATS.len() - 1),
        DateSep(v) => s.date_sep = v.min(DATE_SEPS.len() - 1),
        Author(v) => s.author = v,
        AutoBackup(v) => s.auto_backup = v,
        BackupMin(v) => s.backup_min = (v as usize).clamp(1, 240),
        SaveBeforeTable(v) => s.save_before_table = v,
        Margin(v) => s.margin = v.clamp(0.0, 60.0),
        Header(v) => s.header = v,
        Footer(v) => s.footer = v,

        DefKind(v) => s.def_kind = v.min(9),
        DefColormap(v) => s.def_colormap = v,
        DefColorIdx(v) => s.def_color_idx = v.min(11),
        DefPoints(v) => s.def_points = v,
        DefLine(v) => s.def_line = v,
        DefError(v) => s.def_error = v,
        DefBracket(v) => s.def_bracket = v.min(BRACKET_DEFAULTS.len() - 1),
        DefBracketLabel(v) => s.def_bracket_label = v,

        ConfLevel(v) => s.conf_level = v.min(CONF_LEVELS.len() - 1),
        Alpha(v) => s.alpha = v.clamp(0.001, 0.20),
        Decimals(v) => s.decimals = (v as usize).clamp(0, 10),
        CopyDecimals(v) => s.copy_decimals = (v as usize).clamp(0, 3),
        CopyDecSep(v) => s.copy_decimal_sep = (v as usize).clamp(0, 1),
        CopyExcluded(v) => s.copy_excluded = (v as usize).clamp(0, 2),
        PFormat(v) => s.p_format = v.min(P_FORMATS.len() - 1),
        SciStyle(v) => s.sci_style = v.min(SCI_STYLES.len() - 1),
        AutoSci(v) => s.auto_sci = v,
        Seed(v) => s.seed = v.parse().unwrap_or(s.seed),
        Residual(v) => s.residual = v,
        ReportFmt(v) => s.report_fmt = v.min(REPORT_FORMATS.len() - 1),
        Outlier(v) => s.outlier = v.min(OUTLIER_METHODS.len() - 1),
        Encoding(v) => s.encoding = v.min(ENCODINGS.len() - 1),
        MultiMethod(v) => {
            let all = crate::analysis::MultiMethod::all();
            s.multi_method = all.get(v).copied().unwrap_or(all[0]);
        }
        NonlinearModel(v) => {
            let all = crate::analysis::ModelId::all();
            s.nl_model = all.get(v).copied().unwrap_or(all[0]);
        }

        OfficeTitle(v) => s.office_title = v,
        OfficeAuthor(v) => s.office_author = v,
        OfficeStyle(v) => s.office_style = v.min(OFFICE_STYLES.len() - 1),
        OfficeColormap(v) => s.office_colormap = v,
        OfficeFont(v) => s.office_font = v,
        OfficeFontSize(v) => s.office_font_size = v.clamp(6.0, 48.0),
        ExportWidth(v) => s.export_width = v.clamp(320.0, 8000.0),
    }
}
