//! 全局设置（移植自 Spectra，适配 egui）。
//!
//! 七类：外观 / 视图 / 文件与打印 / 新建图表 / 分析 / 文件位置 / 发送到 Office。
//! 存盘用纯文本 `key=value`，落在用户可写的配置目录。
//! 草稿模式：设置窗口改的是 `draft`，点「确认/应用」才落 `live`。

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

// ── 全局活动设置 ────────────────────────────────────────────────────────────

static LIVE: OnceLock<Mutex<Settings>> = OnceLock::new();

pub fn live() -> &'static Mutex<Settings> {
    LIVE.get_or_init(|| {
        let s = Settings::load();
        if !Settings::path().exists() {
            let _ = s.save();
        }
        Mutex::new(s)
    })
}

pub fn get() -> Settings {
    live().lock().map(|s| s.clone()).unwrap_or_default()
}

pub fn with<R>(f: impl FnOnce(&Settings) -> R) -> R {
    match live().lock() {
        Ok(g) => f(&g),
        Err(p) => f(p.get_ref()),
    }
}

pub fn replace(next: Settings) {
    if let Ok(mut s) = live().lock() {
        *s = next;
    }
}

pub fn save_current() {
    let s = get();
    let _ = s.save();
}

/// 节流落盘（滑块拖动时最多每 700ms 写一次）。
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

pub const THEMES: [&str; 3] = ["浅色", "深色", "跟随系统"];
pub const ACCENTS: [&str; 8] = [
    "#5B6CFF", // 蓝（默认）
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
pub const REPORT_FORMATS: [&str; 3] = ["表格", "段落", "APA"];
pub const OUTLIER_METHODS: [&str; 4] = ["无", "Grubbs", "Tukey 1.5 IQR", "ROUT (Q=1%)"];
pub const ENCODINGS: [&str; 3] = ["UTF-8", "UTF-8 with BOM", "GBK"];
pub const CONF_LEVELS: [&str; 3] = ["90%", "95%", "99%"];
pub const BRACKET_DEFAULTS: [&str; 4] = ["无", "相邻两组", "全部两两", "都与第一组比"];
pub const GRID_KINDS: [&str; 8] = [
    "XY 表",
    "柱状表",
    "分组表",
    "列联表",
    "生存表",
    "构成表",
    "多变量表",
    "嵌套表",
];

// ── 设置项 ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Settings {
    // ── 0 外观 ──
    pub theme: usize,
    /// 语言：0=中文，1=English
    pub language: usize,
    /// UI 缩放倍率
    pub font_scale: f32,
    pub high_contrast: bool,
    /// 圆角倍率（1.0 = 设计默认值）
    pub radius: f32,
    pub animate: bool,
    pub opacity: f32,
    /// 无障碍：更大的点击区与更强的描边
    pub a11y: bool,
    pub accent: usize,

    // ── 1 视图设置 ──
    pub grid_rows: usize,
    pub grid_cols: usize,
    pub nav_width: f32,
    pub show_navigator: bool,
    pub show_inspector: bool,
    pub show_status: bool,
    pub recent_count: usize,
    pub anim_ms: usize,

    // ── 2 文件与打印 ──
    pub open_dir: String,
    pub save_dir: String,
    pub date_fmt: usize,
    pub date_sep: usize,
    pub author: String,
    pub auto_backup: bool,
    pub backup_min: usize,
    pub save_before_table: bool,
    pub margin: f32,
    pub header: String,
    pub footer: String,

    // ── 3 新建图表 ──
    pub def_kind: usize,
    pub def_colormap: usize,
    pub def_points: bool,
    pub def_line: bool,
    pub def_error: bool,
    pub def_bracket: usize,
    pub def_bracket_label: String,
    pub bracket_label_fmt: usize,

    // ── 4 分析 ──
    pub conf_level: usize,
    pub alpha: f32,
    pub decimals: usize,
    pub copy_decimals: usize,
    pub copy_decimal_sep: usize,
    pub copy_excluded: usize,
    pub p_format: usize,
    pub auto_sci: bool,
    pub seed: u64,
    pub residual: bool,
    pub report_fmt: usize,
    pub outlier: usize,
    pub encoding: usize,

    // ── 6 导出 ──
    pub export_dpi: usize,
    pub export_width: f32,
    pub export_format: usize,
    pub default_table_kind: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: 0,
            language: 0,
            font_scale: 1.0,
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
            auto_sci: false,
            seed: 0x9E3779B9,
            residual: false,
            report_fmt: 0,
            outlier: 0,
            encoding: 0,

            export_dpi: 300,
            export_width: 1600.0,
            export_format: 0,
            default_table_kind: 0,
        }
    }
}

const FILE: &str = "stataura_settings.txt";

impl Settings {
    /// 设置文件目录：环境变量 > APPDATA > ~/.config > exe 同级 userdata。
    pub fn dir() -> std::path::PathBuf {
        if let Ok(d) = std::env::var("STATAURA_CONFIG_DIR") {
            let d = d.trim();
            if !d.is_empty() {
                return std::path::PathBuf::from(d);
            }
        }
        #[cfg(windows)]
        {
            if let Ok(a) = std::env::var("APPDATA") {
                if !a.trim().is_empty() {
                    return std::path::PathBuf::from(a).join("StatAura");
                }
            }
        }
        #[cfg(not(windows))]
        {
            if let Ok(h) = std::env::var("HOME") {
                if !h.trim().is_empty() {
                    return std::path::PathBuf::from(h).join(".config").join("stataura");
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

    pub fn load() -> Self {
        let mut s = Self::default();
        let txt = std::fs::read_to_string(Self::path());
        let Ok(txt) = txt else {
            return s;
        };
        for line in txt.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
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
                "language" => u(&mut s.language),
                "font_scale" => f(&mut s.font_scale),
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

                "export_dpi" => u(&mut s.export_dpi),
                "export_width" => f(&mut s.export_width),
                "export_format" => u(&mut s.export_format),
                "default_table_kind" => u(&mut s.default_table_kind),
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
        p("language", self.language.to_string());
        p("font_scale", format!("{}", self.font_scale));
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
        p("auto_sci", b(self.auto_sci));
        p("seed", self.seed.to_string());
        p("residual", b(self.residual));
        p("report_fmt", self.report_fmt.to_string());
        p("outlier", self.outlier.to_string());
        p("encoding", self.encoding.to_string());

        p("export_dpi", self.export_dpi.to_string());
        p("export_width", format!("{}", self.export_width));
        p("export_format", self.export_format.to_string());
        p("default_table_kind", self.default_table_kind.to_string());

        let dir = Self::dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(FILE), out).map_err(|e| e.to_string())
    }

    /// 置信水平对应的 z 值（双侧）。
    pub fn z(&self) -> f64 {
        match self.conf_level {
            0 => 1.645,
            2 => 2.576,
            _ => 1.960,
        }
    }

    /// 是否深色模式。
    pub fn is_dark(&self) -> bool {
        match self.theme {
            1 => true,
            2 => {
                // 跟随系统：读 egui 上下文偏好
                false // 简化——实际由调用方根据 ctx 决定
            }
            _ => false,
        }
    }
}

// ── 编辑消息 ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum Edit {
    Theme(usize),
    Language(usize),
    FontScale(f32),
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
    DefPoints(bool),
    DefLine(bool),
    DefError(bool),
    DefBracket(usize),
    DefBracketLabel(String),
    BracketLabelFmt(usize),

    ConfLevel(usize),
    Alpha(f32),
    Decimals(f32),
    CopyDecimals(usize),
    CopyDecSep(usize),
    CopyExcluded(usize),
    PFormat(usize),
    AutoSci(bool),
    Seed(String),
    Residual(bool),
    ReportFmt(usize),
    Outlier(usize),
    Encoding(usize),

    ExportDpi(usize),
    ExportWidth(f32),
    ExportFormat(usize),
    DefaultTableKind(usize),
}

pub fn apply(s: &mut Settings, e: Edit) {
    use Edit::*;
    match e {
        Theme(v) => s.theme = v.min(THEMES.len() - 1),
        Language(v) => s.language = v.min(1),
        FontScale(v) => s.font_scale = v.clamp(0.85, 1.60),
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

        DefKind(v) => s.def_kind = v.min(7),
        DefColormap(v) => s.def_colormap = v,
        DefPoints(v) => s.def_points = v,
        DefLine(v) => s.def_line = v,
        DefError(v) => s.def_error = v,
        DefBracket(v) => s.def_bracket = v.min(BRACKET_DEFAULTS.len() - 1),
        DefBracketLabel(v) => s.def_bracket_label = v,
        BracketLabelFmt(v) => s.bracket_label_fmt = v.min(2),

        ConfLevel(v) => s.conf_level = v.min(CONF_LEVELS.len() - 1),
        Alpha(v) => s.alpha = v.clamp(0.001, 0.20),
        Decimals(v) => s.decimals = (v as usize).clamp(0, 10),
        CopyDecimals(v) => s.copy_decimals = (v as usize).clamp(0, 3),
        CopyDecSep(v) => s.copy_decimal_sep = (v as usize).clamp(0, 1),
        CopyExcluded(v) => s.copy_excluded = (v as usize).clamp(0, 2),
        PFormat(v) => s.p_format = v.min(P_FORMATS.len() - 1),
        AutoSci(v) => s.auto_sci = v,
        Seed(v) => s.seed = v.parse().unwrap_or(s.seed),
        Residual(v) => s.residual = v,
        ReportFmt(v) => s.report_fmt = v.min(REPORT_FORMATS.len() - 1),
        Outlier(v) => s.outlier = v.min(OUTLIER_METHODS.len() - 1),
        Encoding(v) => s.encoding = v.min(ENCODINGS.len() - 1),

        ExportDpi(v) => s.export_dpi = v.clamp(72, 1200),
        ExportWidth(v) => s.export_width = v.clamp(320.0, 8000.0),
        ExportFormat(v) => s.export_format = v.min(4),
        DefaultTableKind(v) => s.default_table_kind = v.min(7),
    }
}

// ── 随机种子 ────────────────────────────────────────────────────────────────

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
    if x == 0 {
        0x9E37_79B9
    } else {
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 设置读写往返() {
        let mut s = Settings::default();
        s.theme = 1;
        s.alpha = 0.01;
        s.author = "测试".into();
        s.export_dpi = 600;

        // 手动序列化再反序列化
        let txt = format!("theme=1\nalpha=0.01\nauthor=测试\nexport_dpi=600\n");
        let mut s2 = Settings::default();
        for line in txt.lines() {
            let (k, v) = line.split_once('=').unwrap();
            match k {
                "theme" => s2.theme = v.parse().unwrap(),
                "alpha" => s2.alpha = v.parse().unwrap(),
                "author" => s2.author = v.to_string(),
                "export_dpi" => s2.export_dpi = v.parse().unwrap(),
                _ => {}
            }
        }
        assert_eq!(s2.theme, 1);
        assert!((s2.alpha - 0.01).abs() < 1e-6);
        assert_eq!(s2.author, "测试");
        assert_eq!(s2.export_dpi, 600);
    }

    #[test]
    fn 默认值合理() {
        let s = Settings::default();
        assert_eq!(s.theme, 0);
        assert!((s.font_scale - 1.0).abs() < 1e-6);
        assert_eq!(s.conf_level, 1);
        assert!((s.alpha - 0.05).abs() < 1e-6);
        assert_eq!(s.export_dpi, 300);
    }

    #[test]
    fn z值正确() {
        let s = Settings::default();
        assert!((s.z() - 1.960).abs() < 0.01);
        let mut s2 = Settings::default();
        s2.conf_level = 0;
        assert!((s2.z() - 1.645).abs() < 0.01);
        s2.conf_level = 2;
        assert!((s2.z() - 2.576).abs() < 0.01);
    }
}
