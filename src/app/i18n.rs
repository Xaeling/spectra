//! 国际化（i18n）：中英双语。
//!
//! 设计参照 Spectra i18n.rs：`Lang` 枚举 + `K` 文字键 + `t(lang, k)` 查表。
//! 与 Spectra 的差异：StatAura 的 UI 字符串较少，只覆盖菜单、对话框、
//! 状态栏等高频文案；绘图/统计输出不翻译（术语国际通用）。

/// 语言。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::Zh, Lang::En];
    pub fn label(self) -> &'static str {
        match self {
            Lang::Zh => "中文",
            Lang::En => "English",
        }
    }
}

/// 文字键。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum K {
    // ── 菜单 ──
    MenuFile,
    MenuEdit,
    MenuView,
    MenuInsert,
    MenuAnalyze,
    MenuChange,
    MenuSettings,
    MenuHelp,
    // ── 文件操作 ──
    New,
    Open,
    Save,
    SaveAs,
    Import,
    Export,
    Exit,
    // ── 编辑 ──
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Delete,
    SelectAll,
    // ── 视图 ──
    Sidebar,
    Inspector,
    StatusBar,
    DarkMode,
    ZoomIn,
    ZoomOut,
    ZoomFit,
    // ── 插入 ──
    NewTable,
    NewGraph,
    NewLayout,
    // ── 分析 ──
    AutoAnalyze,
    CommandPalette,
    // ── 设置 ──
    Settings,
    Preferences,
    Theme,
    Language,
    Appearance,
    View,
    Files,
    Chart,
    Analysis,
    Export2,
    About,
    Confirm,
    Apply,
    Cancel,
    ResetDefaults,
    // ── 主题 ──
    Light,
    Dark,
    FollowSystem,
    // ── 对话框 ──
    OpenProject,
    SaveProject,
    OpenFile,
    SaveFile,
    FileName,
    FileType,
    AllFormats,
    // ── 状态栏 ──
    Ready,
    Saved,
    Unsaved,
    Rows,
    Columns,
    // ── 表格 ──
    TableName,
    TableKind,
    ColumnName,
    GroupName,
    RowHeader,
    AddColumn,
    RemoveColumn,
    AddRow,
    RemoveRow,
    // ── 图表 ──
    ChartTitle,
    XAxis,
    YAxis,
    Legend,
    ShowLegend,
    LegendPos,
    DataPoints,
    ErrorBars,
    LineWidth,
    // ── 通知 ──
    SaveSuccess,
    OpenSuccess,
    ExportSuccess,
    Error,
    Warning,
    Info,
    // ── 关于 ──
    Version,
    Description,
    Credits,
    // ── 通用 ──
    Ok,
    Yes,
    No,
    Name,
    Value,
    Type,
    Description2,
    Method,
    Result,
    Note,
    // ── 智能分析 ──
    SmartAnalysis,
    Prerequisites,
    MainTest,
    PostHoc,
    Conclusion,
    Normality,
    Outliers,
    Variance,
    Transform,
}

/// 取翻译文本。
pub fn t(lang: Lang, k: K) -> &'static str {
    match lang {
        Lang::Zh => zh(k),
        Lang::En => en(k),
    }
}

fn zh(k: K) -> &'static str {
    use K::*;
    match k {
        MenuFile => "文件",
        MenuEdit => "编辑",
        MenuView => "视图",
        MenuInsert => "插入",
        MenuAnalyze => "分析",
        MenuChange => "变换",
        MenuSettings => "设置",
        MenuHelp => "帮助",
        New => "新建",
        Open => "打开",
        Save => "保存",
        SaveAs => "另存为",
        Import => "导入",
        Export => "导出",
        Exit => "退出",
        Undo => "撤销",
        Redo => "重做",
        Cut => "剪切",
        Copy => "复制",
        Paste => "粘贴",
        Delete => "删除",
        SelectAll => "全选",
        Sidebar => "导航栏",
        Inspector => "检查器",
        StatusBar => "状态栏",
        DarkMode => "深色模式",
        ZoomIn => "放大",
        ZoomOut => "缩小",
        ZoomFit => "适应",
        NewTable => "新建表格",
        NewGraph => "新建图表",
        NewLayout => "新建拼版",
        AutoAnalyze => "智能分析",
        CommandPalette => "命令面板",
        Settings => "设置",
        Preferences => "偏好设置",
        Theme => "主题",
        Language => "语言",
        Appearance => "外观",
        View => "视图",
        Files => "文件",
        Chart => "图表",
        Analysis => "分析",
        Export2 => "导出",
        About => "关于",
        Confirm => "确认",
        Apply => "应用",
        Cancel => "取消",
        ResetDefaults => "恢复默认",
        Light => "浅色",
        Dark => "深色",
        FollowSystem => "跟随系统",
        OpenProject => "打开工程",
        SaveProject => "保存工程",
        OpenFile => "打开文件",
        SaveFile => "保存文件",
        FileName => "文件名",
        FileType => "文件类型",
        AllFormats => "所有格式",
        Ready => "就绪",
        Saved => "已保存",
        Unsaved => "未保存",
        Rows => "行",
        Columns => "列",
        TableName => "表名",
        TableKind => "表类型",
        ColumnName => "列名",
        GroupName => "组名",
        RowHeader => "行标题",
        AddColumn => "添加列",
        RemoveColumn => "删除列",
        AddRow => "添加行",
        RemoveRow => "删除行",
        ChartTitle => "图表标题",
        XAxis => "X 轴",
        YAxis => "Y 轴",
        Legend => "图例",
        ShowLegend => "显示图例",
        LegendPos => "图例位置",
        DataPoints => "数据点",
        ErrorBars => "误差棒",
        LineWidth => "线宽",
        SaveSuccess => "已保存",
        OpenSuccess => "已打开",
        ExportSuccess => "已导出",
        Error => "错误",
        Warning => "警告",
        Info => "信息",
        Version => "版本",
        Description => "科研数据统计与出版级绘图",
        Credits => "基于 Rust + egui + tiny-skia",
        Ok => "确定",
        Yes => "是",
        No => "否",
        Name => "名称",
        Value => "数值",
        Type => "类型",
        Description2 => "说明",
        Method => "方法",
        Result => "结果",
        Note => "备注",
        SmartAnalysis => "智能分析",
        Prerequisites => "前提检验",
        MainTest => "主检验",
        PostHoc => "事后比较",
        Conclusion => "结论",
        Normality => "正态性",
        Outliers => "离群值",
        Variance => "方差",
        Transform => "变换",
    }
}

fn en(k: K) -> &'static str {
    use K::*;
    match k {
        MenuFile => "File",
        MenuEdit => "Edit",
        MenuView => "View",
        MenuInsert => "Insert",
        MenuAnalyze => "Analyze",
        MenuChange => "Change",
        MenuSettings => "Settings",
        MenuHelp => "Help",
        New => "New",
        Open => "Open",
        Save => "Save",
        SaveAs => "Save As",
        Import => "Import",
        Export => "Export",
        Exit => "Exit",
        Undo => "Undo",
        Redo => "Redo",
        Cut => "Cut",
        Copy => "Copy",
        Paste => "Paste",
        Delete => "Delete",
        SelectAll => "Select All",
        Sidebar => "Navigator",
        Inspector => "Inspector",
        StatusBar => "Status Bar",
        DarkMode => "Dark Mode",
        ZoomIn => "Zoom In",
        ZoomOut => "Zoom Out",
        ZoomFit => "Zoom Fit",
        NewTable => "New Table",
        NewGraph => "New Graph",
        NewLayout => "New Layout",
        AutoAnalyze => "Auto Analysis",
        CommandPalette => "Command Palette",
        Settings => "Settings",
        Preferences => "Preferences",
        Theme => "Theme",
        Language => "Language",
        Appearance => "Appearance",
        View => "View",
        Files => "Files",
        Chart => "Chart",
        Analysis => "Analysis",
        Export2 => "Export",
        About => "About",
        Confirm => "OK",
        Apply => "Apply",
        Cancel => "Cancel",
        ResetDefaults => "Reset Defaults",
        Light => "Light",
        Dark => "Dark",
        FollowSystem => "Follow System",
        OpenProject => "Open Project",
        SaveProject => "Save Project",
        OpenFile => "Open File",
        SaveFile => "Save File",
        FileName => "File Name",
        FileType => "File Type",
        AllFormats => "All Formats",
        Ready => "Ready",
        Saved => "Saved",
        Unsaved => "Unsaved",
        Rows => "Rows",
        Columns => "Columns",
        TableName => "Table Name",
        TableKind => "Table Kind",
        ColumnName => "Column Name",
        GroupName => "Group Name",
        RowHeader => "Row Header",
        AddColumn => "Add Column",
        RemoveColumn => "Remove Column",
        AddRow => "Add Row",
        RemoveRow => "Remove Row",
        ChartTitle => "Chart Title",
        XAxis => "X Axis",
        YAxis => "Y Axis",
        Legend => "Legend",
        ShowLegend => "Show Legend",
        LegendPos => "Legend Position",
        DataPoints => "Data Points",
        ErrorBars => "Error Bars",
        LineWidth => "Line Width",
        SaveSuccess => "Saved successfully",
        OpenSuccess => "Opened successfully",
        ExportSuccess => "Exported successfully",
        Error => "Error",
        Warning => "Warning",
        Info => "Info",
        Version => "Version",
        Description => "Scientific statistics & publication-grade plotting",
        Credits => "Built with Rust + egui + tiny-skia",
        Ok => "OK",
        Yes => "Yes",
        No => "No",
        Name => "Name",
        Value => "Value",
        Type => "Type",
        Description2 => "Description",
        Method => "Method",
        Result => "Result",
        Note => "Note",
        SmartAnalysis => "Smart Analysis",
        Prerequisites => "Prerequisites",
        MainTest => "Main Test",
        PostHoc => "Post-hoc",
        Conclusion => "Conclusion",
        Normality => "Normality",
        Outliers => "Outliers",
        Variance => "Variance",
        Transform => "Transform",
    }
}

/// 当前语言（全局）。存储在 Settings 里，这里提供快捷读取。
pub fn current() -> Lang {
    let idx = crate::app::settings::get().language;
    if idx == 1 {
        Lang::En
    } else {
        Lang::Zh
    }
}

/// 取当前语言的翻译。
pub fn tt(k: K) -> &'static str {
    t(current(), k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 中英文不缺键() {
        // 所有 K 变体必须有中英文翻译
        for k in [
            K::MenuFile,
            K::New,
            K::Save,
            K::Settings,
            K::Confirm,
            K::Cancel,
            K::Theme,
            K::Language,
            K::Ready,
        ] {
            let zh_t = t(Lang::Zh, k);
            let en_t = t(Lang::En, k);
            assert!(!zh_t.is_empty(), "中文缺失: {k:?}");
            assert!(!en_t.is_empty(), "英文缺失: {k:?}");
        }
    }

    #[test]
    fn 语言切换() {
        assert_eq!(t(Lang::Zh, K::MenuFile), "文件");
        assert_eq!(t(Lang::En, K::MenuFile), "File");
        assert_eq!(t(Lang::Zh, K::Save), "保存");
        assert_eq!(t(Lang::En, K::Save), "Save");
    }

    #[test]
    fn lang标签() {
        assert_eq!(Lang::Zh.label(), "中文");
        assert_eq!(Lang::En.label(), "English");
    }
}
