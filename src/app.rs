//! 应用状态、消息与视图组装。

use charis_ui::image::ImageSource;
use charis_ui::prelude::*;
use charis_ui::style::FontFamily;

use crate::i18n::{self, K, Lang};
use crate::model::{
    sample_project, Active, CellValue, DataTable, GraphSheet, Project, ResultSheet, TableType,
};
use crate::palette::*;
use crate::settings::{self, Edit, Settings};
use crate::export;
use crate::stats;
use crate::plot::{self, BracketMode, ChartKind, ChartView, Comparison, GroupStat, UnitStyle};
use crate::ui::{self, grid::{snapshot, TableSnap}};

/// 图表渲染尺寸（逻辑像素）。固定尺寸后由 charis 负责缩放；
/// 若要做「随窗口重排」，在 update 里接 Element::on_resize 后改这里并重绘。
/// 预览画布的**逻辑**尺寸。
const CHART_W: u32 = 480;
const CHART_H: u32 = 320;
/// 预览的超采样倍率。位图后端先按 dpr 倍渲染再交给 UI 缩放显示，
/// 所以线条与文字都是抗锯齿的，不会像 640×430 直接拉伸那样发虚。
const CHART_DPR: f32 = 2.0;
/// 滑块停多久算「松手 / 停下」（毫秒）—— 之后才真正重绘
const SLIDER_PAUSE_MS: u32 = 160;
/// 拖动过程中最多每隔多久更新一次画面（毫秒）—— 避免完全没反馈
const SLIDER_THROTTLE_MS: u32 = 220;

/// 功能区卡片上的下拉菜单。每个变种对应一个窗格，内容见 `ui::toolbar`。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RibbonMenu {
    New,
    Save,
    /// 表名高亮（Table 卡片第 1 项）
    HighlightTable,
    NewTable,
    Paste,
    Insert,
    Delete,
    /// 单元格高亮（Change 卡片第 3 项）
    HighlightCell,
    Sort,
}

#[derive(Clone)]
#[allow(dead_code)]
pub enum Msg {
    None,
    // 欢迎对话框
    PickTableKind(usize),
    PickExisting(usize),
    WelcomeCreate,
    WelcomeCancel,
    // 导航器
    ToggleFolder(usize),
    OpenSheet(usize, usize),
    Search(String),
    NewDataTable,
    NewAnalysis,
    NewGraph,
    // 工具栏 / 菜单
    Tool(&'static str),
    ToggleInspector,
    // 数据表
    // 图表与检查器
    SetChartKind(usize),
    // ── 八菜单（File / Edit / View / Insert / Analyze / Change / Settings / Help）──
    NewProject,
    SaveAs,
    CloseSheet,
    CloseAll,
    SaveTableCsv,
    SaveResultsCsv,
    SaveGraphSvg,
    ExportChart,
    ExportMatplotlib,
    PrintSetup,
    SendToPpt,
    SendToWord,
    CloudSpectra,
    CloudLuma,
    CheckCompat,
    RunScript,
    DoRedo,
    PasteTransposed,
    PasteSpecial,
    /// 粘贴为嵌入对象：本应用内等价于「粘贴为值」
    PasteEmbedded,
    /// 粘贴为链接：不支持 OLE 链接，降级为粘贴值并提示
    PasteLinked,
    PasteTransposedEmbedded,
    PasteTransposedLinked,
    SelectAll,
    SelectColumn,
    SelectRow,
    SortByX,
    SortByRowTitle,
    ReverseRowOrder,
    ExcludeValues,
    DeleteTable,
    TableOrder,
    ToggleFreeze,
    RenameTable,
    SetHighlight(usize),
    LinkTable,
    DataObject,
    UnlinkCalc,
    ZoomFitPage,
    ZoomFitChart,
    ZoomIn,
    ZoomOut,
    SetZoom(f32),
    GoToDataTable,
    GoToLibrary,
    ToggleMainToolbar,
    ToggleNavigator,
    ToggleBottomBar,
    ToggleRulers,
    ToggleGridView,
    ToggleRowTitles,
    ToggleColTitles,
    RestoreNotif,
    NewInfo,
    RenameStart(RenameTarget),
    RenameInput(String),
    RenameCommit,
    RenameCancel,
    /// 单击列标题 → 选中整列（物理列区间）
    PickCols(usize, usize),
    /// 单击行标题 → 选中整行
    PickRow(usize),
    InsertCol,
    CreateSeries,
    DuplicateTable,
    DuplicateSeries,
    /// 复制族：复制当前表 + 引用它的全部图表与结果
    DuplicateFamily,
    /// 合并文件：把另一个工程里的表并入当前工程
    MergeFile,
    CalcVariable,
    RiskTable,
    EmbedTable,
    InsertChar,
    InsertConst,
    InsertObject,
    RunAnalysisById(usize),
    FlipHorizontal,
    DataSetOrder,
    ReverseDataSetOrder,
    OpenColorSettings,
    HintInspector,
    ReverseLegend,
    AnalysisParams,
    Resimulate,
    ResizeGraph,
    About,
    CloseAbout,
    CloseAlert,
    // ── 标注 ─────────────────────────────────────────────────────────────

    ToggleError,
    ToggleScatter,
    ToggleLegend,
    SetColormap(usize),
    SetXName(String),
    SetXUnit(String),
    SetYName(String),
    SetYUnit(String),
    SetUnitStyle(usize),
    ToggleItalic,
    ToggleBracket,
    SetExportBackend(usize),
    /// 量符号（斜体显示）
    SetXSym(String),
    SetYSym(String),
    /// 括号样式：0 半角，1 全角
    SetBracketStyle(usize),
    /// 柱宽（厘米）
    SetBarWidthCm(f32),
    ToggleAxisBreak,
    ToggleRightAxis,
    SetRightFrom(usize),
    SetCaption(String),
    SetGridStyle(usize),
    SetDash(usize),
    /// 设置项编辑：所有设置共用一个变体，避免 Msg 里塞进几十个分支。
    /// 改的是 `settings_draft`，点「确认 / 应用」时才落到 `settings`。
    Edit(settings::Edit),
    /// 设置窗口里种子旁边那个骰子：随机换一个种子
    RandomizeSeed,
    ExportPng,
    /// 打开功能区下拉菜单（携带卡片左边缘的 x，用于对齐）
    OpenRibbonMenu(RibbonMenu, f32),
    CloseRibbonMenu,
    /// 打开「导出」弹窗（绘图检查器里那个唯一的大按钮）
    OpenExportDialog,
    /// 在导出弹窗里挑一个格式
    SetExportFmt(usize),
    /// 弹窗里点「导出」：先弹系统保存对话框，再按格式写盘
    ExportConfirm,
    CloseExport,
    SetChartTitle(String),
    // ── 数据表结构（真实生效）────────────────────────────────────────────
    AddDataSet,
    RemoveLastDataSet,
    AddSubColumn,
    RemoveLastSubColumn,
    InsertRow,
    DeleteRow,
    /// 删除光标所在的物理列（右侧列整体左移）
    DeleteCol,
    /// 删除光标所在的整个数据集（含其全部子列）
    DeleteDataSet,
    ClearCells,
    // ── 剪贴板 / 撤销 ────────────────────────────────────────────────────
    CopyCell,
    CutCell,
    PasteCell,
    DoUndo,
    // ── 分析（真实统计）──────────────────────────────────────────────────
    SetAnalysis(usize),
    SetWelch(bool),
    RunAnalysis,
    // ── 文件 ─────────────────────────────────────────────────────────────
    SaveProject,
    OpenProject,
    /// 另存为族：工程 + 每张表一份 CSV，一起落到选定目录
    SaveFamily,
    /// 另存为示例：落到工程目录下的 examples/ 子目录
    SaveExample,
    /// 另存为模板：清空所有单元格但保留表结构、图表与分析设置
    SaveTemplate,
    /// 另存为方法：把当前分析的参数集导出成 JSON
    SaveMethod,
    // ── 标注（Draw / Write / Text）───────────────────────────────────────
    AddLine,
    AddArrow,
    AddText,
    AnnFontUp,
    AnnFontDown,
    RemoveAnnotation(usize),
    // ── 其他工具 ─────────────────────────────────────────────────────────
    ImportCsv,
    PrintChart,
    SendChart,
    ExportBundle,
    CycleChartKind,
    MagicApply,
    // 符号与散点
    ToggleSymbolFollows,
    SetSymbolColor(usize),
    SetSymbolSize(f32),
    SetJitter(f32),
    // ── 绘图区面板（九组）─────────────────────────────────────────────────
    SetErrKind(usize),
    SetErrDir(usize),
    SetErrWidth(f32),
    ToggleConnect,
    SetBarWidth(f32),
    SetBarPattern(usize),
    ToggleBarZero,
    ToggleAreaFill,
    ToggleGrid,
    ToggleLogY,
    ToggleGrayscale,
    SetLegendPos(usize),
    SetLineWidth(f32),
    SetSymbolShape(usize),
    SetSymbolBorder(f32),
    SetSymbolAlpha(f32),
    ToggleTitleBold,
    ToggleTitleItalic,
    /// 滑块拖动结束：此时才真正重绘（拖动过程中只更新数值，不画图）
    SliderDone,
    /// 图表标题
    SetTitle(String),
    /// 重命名当前图形
    SetGraphName(String),
    /// 画布高度（① 图形组）
    SetCanvasH(f32),
    // ── ⑧ 图类型专属 ──────────────────────────────────────────────────────
    SetBinsF(f32),
    SetPieInner(f32),
    SetPieStart(f32),
    ToggleHeatAuto,
    SetForestRef(f64),
    ToggleSurvCensor,
    SetWaffleColsF(f32),
    ToggleStacked100,
    ToggleFacetShared,
    SetBoxStyle(usize),
    /// 边框样式：0 = L 型 / 1 = 全框
    SetFrame(usize),
    // 显著性标注
    SetBracketMode(usize),
    SetBracketValue(String),
    AddComparison,
    AutoComparisons,
    RemoveComparison(usize),
    EditComparison(usize, Option<usize>, Option<usize>, Option<String>),
    /// 切换显著性标注的写法：0 = 只星号，1 = 只 P 值，2 = 星号 + P 值
    SetBracketLabelFmt(usize),
    // 结果表
    SelectTab(usize),
    // 设置窗口
    OpenSettings,
    CloseSettings,
    SettingsTab(usize),
    SetLangDraft(Lang),
    SetTheme(usize),
    SettingsOk,
    SettingsApply,
    // ── 单元格编辑 ──
    /// 点击一个单元格：(物理列, 行)。usize::MAX 表示行标题列
    CellClick(usize, usize),
    /// 编辑框输入（实时写入稀疏层，不会丢）
    CellInput(String),
    /// 网格键盘事件（方向键 / Enter / Tab / Escape）
    GridKey(KeyEvent),
    /// 鼠标悬停的格子
    HoverCell(Option<(usize, usize)>),
    /// 切换鼠标框选模式（工具栏「框选」/ Ctrl+B，保留作键盘扩展的入口）
    ToggleSelDrag,
    // ── 真正的指针拖拽（charis `Element::on_drag`）──────────────────────
    /// 在某个格子上**按下**：以它为锚点开始框选
    SelDragStart(usize, usize),
    /// 拖拽移动中。参数是指针位移，实际落点由 `on_hover` 提供
    SelDragMove(charis_ui::geometry::Point),
    /// 松开：结束框选
    SelDragEnd,
    /// 在行标题上按下并拖动：连续选多行
    RowDragStart(usize),
    RowDragMove(usize),
    /// 在列标题上按下并拖动：连续选多列
    ColDragStart(usize),
    ColDragMove(usize),
    // ── 右键上下文菜单（charis `Element::on_context_menu`）──────────────
    /// 单元格右键：(列, 行, 指针位置)
    CellContextMenu(usize, usize, charis_ui::geometry::Point),
    /// 行标题右键：(行, 指针位置)
    RowContextMenu(usize, charis_ui::geometry::Point),
    /// 列标题右键：(列, 指针位置)
    ColContextMenu(usize, charis_ui::geometry::Point),
    /// 导航器条目右键：(文件夹, 索引, 指针位置)
    SheetContextMenu(usize, usize, charis_ui::geometry::Point),
    /// 点击上下文菜单项
    CtxMenu(CtxAction),
    /// 关闭上下文菜单
    CloseCtxMenu,
    // 底部
    GoTo(String),
    Quit,    RowDragEnd(usize),
    ColDragEnd(usize),

}

/// 右键菜单的作用范围：决定菜单里出现哪些项。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtxScope {
    /// 数据单元格
    Cell,
    /// 行标题 / 行号
    Row,
    /// 列标题
    Col,
    /// 导航器里的表单条目（表 / 信息 / 结果 / 图形 / 布局）
    Sheet,
}

/// 右键菜单的当前状态。
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub struct CtxMenuState {
    /// 指针位置（窗口坐标），菜单在这里弹出
    pub x: f32,
    pub y: f32,
    /// 目标列（列标题右键时为列索引；单元格右键时为所在列）
    pub col: usize,
    /// 目标行
    pub row: usize,
    pub scope: CtxScope,
    /// 目标表单是否已冻结（仅 Sheet 作用域有效）：决定菜单显示「冻结」还是「解冻」
    pub frozen: bool,
    /// 弹出时刻：菜单入场动画（整块下落 + 逐项 stagger）按它算进度
    pub opened_at: std::time::Instant,
}

/// 右键菜单可执行的操作。
///
/// 每一项都对应一个**已实现**的动作 —— 不放占位项。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtxAction {
    Copy,
    Cut,
    Paste,
    Clear,
    SelectAll,
    SelectCol,
    SelectRow,
    InsertRow,
    DeleteRow,
    AddDataSet,
    RemoveDataSet,
    RenameCol,
    RenameRow,
    // ── 导航器条目 ────────────────────────────────────────────────────────
    /// 打开该表单
    OpenSheet,
    /// 就地重命名（导航器该行变成输入框）
    RenameSheet,
    /// 创建副本
    DuplicateSheet,
    /// 冻结 / 解冻（冻结后不随源数据更新）
    ToggleFreezeSheet,
    /// 删除该表单
    DeleteSheet,
    /// 导出该图形
    ExportGraphSheet,
}


impl Msg {
    /// 工具栏按钮的身份标识，用于判断「按下」状态。
    #[allow(dead_code)]
    pub fn tool_id(&self) -> &'static str {
        match self {
            Msg::Tool(id) => id,
            _ => "",
        }
    }
}

/// 双击表头改名的目标：大块（数据集名）或小块（子列名）。
///
/// 之前只支持数据集名，子列名（A:Y1 里的 Y1）改不了 ——
/// 用户的原话是「双击改名字怎么只能改大块的？小块的也应该支持」。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RenameTarget {
    /// 数据集（上层大块）名
    DataSet(usize),
    /// 第 `0` 个数据集的第 `1` 个子列（下层小块）名
    Sub(usize, usize),
}

pub struct SpectraApp {
    pub project: Project,
    /// 当前打开的工作表
    pub active: Active,
    /// 5 个文件夹的展开状态
    pub open_folders: [bool; 5],
    pub search: String,
    pub goto: String,
    /// 设置（活动值）。UI 各处直接读 `settings::get()`，这里存一份便于编辑。
    pub settings: Settings,
    /// 设置窗口里的**草稿**：只在点「确认 / 应用」时才写回 `settings`。
    pub settings_draft: Settings,
    /// 欢迎对话框（析光启动时默认弹出）
    pub welcome_open: bool,
    /// 欢迎/建表对话框的用途：0 = 新建项目（替换首张表），1 = 新建数据表（追加）
    pub welcome_mode: usize,
    /// 最近一次弹窗打开的时刻（弹窗互斥，共用一个即可）—— 供展开动画算进度
    pub dlg_open_at: std::time::Instant,
    /// 上一次**真正执行**分析的 (方法 id, 时刻) —— 用于节流。
    ///
    /// 右边栏「运行分析」按钮位于滚动容器内，整棵子树每帧重建；一旦命中测试
    /// 让 `on_click` 在一帧内被重复派发，`run_analysis` 就会被连跑几十次，
    /// 而它每次都要 `push_undo()`（克隆整个 project）再跑一遍统计 ——
    /// 表现就是「点右边栏就卡死」。菜单栏是静态容器，不会重复派发，
    /// 所以「点菜单不卡、点右边栏卡」。这里用「同方法 300ms 内不重跑」挡住。
    pub last_analysis: Option<(usize, std::time::Instant)>,
    /// 上一次分析的耗时（毫秒）。显示在状态栏，用来区分
    /// 「算法真的慢」与「被重复触发」这两种完全不同的问题。
    pub last_analysis_ms: u128,
    /// 结果页的**已翻译内容缓存**（在 `set_active` 里预计算好）。
    ///
    /// `view()` 每帧重建整棵树，而结果页要对整张表的每个单元格查一次翻译表
    /// （20 行 × 十几列 ≈ 几百次 HashMap 查找）再 clone 出 String。
    /// 内容没变却每帧重算一遍，在 60fps 下是纯浪费。
    /// 这里不用 RefCell —— view 是 &self，缓存改不了；放在 update 侧预计算
    /// 就不需要内部可变性。键 = (结果表下标, 语言)。
    pub result_view_data: Option<(usize, Lang, Vec<Vec<String>>, Vec<String>, String)>,
    pub pick_table: usize,
    pub pick_existing: usize,
    /// 光标所在的单元格（物理列, 行）。usize::MAX 列 = 行标题列
    pub cursor: (usize, usize),
    /// 光标格是否处于输入态（点击即进入）
    pub editing: bool,
    /// 输入中的文本（实时写入稀疏层，draft 只用于回显）
    pub draft: String,
    /// 鼠标悬停的格子（跟随鼠标的高亮）
    pub hover: Option<(usize, usize)>,
    /// 虚拟列表渲染的**列数**。列与行一样无限：光标往右走会自动增长。
    pub cols_hint: usize,
    /// 虚拟列表渲染的行数。**只影响显示窗口，不限制数据**：
    /// 光标往下走会自动增长，所以行数可以是任意大。
    pub rows_hint: usize,
    pub show_inspector: bool,
    /// 当前界面语言
    pub lang: Lang,
    /// 设置窗口是否打开
    pub settings_open: bool,
    /// 导出格式弹窗（绘图 ▸ 导出 合并成单一入口后新增）
    pub export_open: bool,
    /// 功能区下拉菜单当前打开的是哪一个、它左边缘的 x、以及弹出时刻
    pub ribbon_menu: Option<(RibbonMenu, f32, std::time::Instant)>,
    /// 导出弹窗里当前选中的格式下标（`ExportFormat::ALL` 的下标）
    pub export_fmt: usize,
    /// 「关于」对话框是否打开（Help ▸ 关于）
    pub about_open: bool,
    pub settings_tab: usize,
    /// 设置里的草稿语言（确认/应用后才落到 `lang`）
    pub lang_draft: Lang,
    /// 深色主题
    pub dark: bool,
    /// 导出后端（设置里切换）
    pub export_backend: export::ExportBackend,
    /// 当前按下并保持的工具（析光的绘图工具会保持高亮）
    pub active_tool: &'static str,
    /// 底部状态栏：只放**不重要**的即时反馈（文件写到了哪、缩放了多少…）
    pub status: String,
    /// 窗口居中的提示弹窗：只放**重要**的信息（错误、需要用户确认的事）
    pub alert: Option<String>,
    /// 当前数据表的只读快照（虚拟列表用；直接查稀疏矩阵，不建行窗口）
    pub snap: TableSnap,
    /// plotters 渲染结果缓存
    pub chart: Option<charis_ui::image::Image>,
    /// 图表的**显示尺寸**（逻辑像素）。位图按 DPR 倍超采样渲染，
    /// 显示时缩回这个尺寸——否则 1920×1280 的位图按原始大小放进滚动区，
    /// 屏幕上只能看到一两根柱子。
    pub chart_w: f32,
    pub chart_h: f32,
    /// 图表基准画布尺寸（Change ▸ 调整图表尺寸 可调）
    pub base_w: u32,
    pub base_h: u32,
    /// 图表是否需要重绘。
    /// **关键性能开关**：以前每次移动光标/输入都会立刻重绘整张位图，
    /// 这是「一卡一卡」的主因。现在改成标脏 + 进入图表页时才真正重绘。
    pub chart_dirty: bool,
    /// 滑块拖动中：上一次滑块事件的时刻（用于判断用户是否停下来）
    pub slider_last: std::time::Instant,
    /// 上一次因滑块而重绘的时刻（节流用）
    pub slider_drawn: std::time::Instant,
    /// 撤销 / 重做栈（整份工程快照，深度有上限）
    pub undo: Vec<Project>,
    pub redo: Vec<Project>,
    /// 冻结表（Edit ▸ 冻结表）
    pub freeze_table: bool,
    /// 高亮表颜色索引（0..8，7 = 无）
    pub highlight: usize,
    /// View 菜单的四个面板开关
    pub main_toolbar: bool,
    pub navigator: bool,
    pub bottom_toolbar: bool,
    /// 标尺 / 网格 / 行标题 / 列标题
    pub rulers: bool,
    pub grid_view: bool,
    pub row_titles_on: bool,
    pub col_titles_on: bool,
    /// 预览缩放（View ▸ Zoom，也用于导出倍率参考）
    pub zoom: f32,
    /// 选区（全选 / 选行 / 选列时记录）
    /// 正在双击改名的列（表头组名）
    pub renaming: Option<RenameTarget>,
    pub rename_draft: String,
    /// 导航器里正在就地重命名的表单 (文件夹, 索引)。
    /// 单独一个字段是因为它与表头改名互不干扰：表头改名在表格里画输入框，
    /// 这个在导航器里画输入框，两者可能同时挂着（比如改名途中切了页）。
    pub sheet_renaming: Option<(usize, usize)>,
    /// 冻结的表单 (文件夹, 索引)：冻结后不随源数据更新。
    /// 之前是硬编码 `folder == 3 && index == 0`，右键菜单改不了。
    pub frozen: Vec<(usize, usize)>,
    pub sel_from: Option<(usize, usize)>,
    pub sel_to: Option<(usize, usize)>,
    /// **正在用鼠标拖拽框选**。
    ///
    /// charis 的 `Element::on_drag` 会依次给出 `DragPhase::Start / Move / End`，
    /// 所以「按下—移动—松开」是**真正可观测**的（此前误判为不可观测，
    /// 依据是 `on_release` 不存在 —— 那只是没有这个**名字**，能力在 `on_drag` 上）。
    ///
    /// 事件始终发往「按下时那个元素」，而 `DragEvent` 只给位移、不给落在哪一格，
    /// 所以落点由 `on_hover` 补充：拖动经过的格子会触发 hover，用它扩展选区。
    pub sel_drag: bool,
    /// 右键上下文菜单：打开时记录 (列, 行, 屏幕 x, 屏幕 y, 作用范围)。
    pub ctx_menu: Option<CtxMenuState>,
    /// 框选模式下鼠标进入的第一个格子，即选区起点。
    pub hover_from: Option<(usize, usize)>,
    /// 焦点请求标记：进入编辑时置位，下一帧开头才真正申请焦点。
    /// 见 `update()` 顶部「坑 2」的说明。
    pub pending_focus: bool,
    /// 焦点已就绪：为 true 时输入框才带 `autofocus`。
    /// 用它把「请求焦点」和「渲染输入框」解耦，避免每帧重复抢焦点。
    pub focus_ready: bool,
    /// 选区清理标记：点在网格外时置位，真正清理推迟到下一个非选区消费类消息。
    /// 见 `update()` 顶部「坑 1」的说明。
    pub pending_clear_sel: bool,
    /// 反转图例顺序（Change ▸ 反转图例顺序）
    pub reverse_legend: bool,
    /// 散点抖动的随机种子（Change ▸ 重新模拟 换一个）
    pub seed: u64,
    /// 主题模式：0 浅色 / 1 深色 / 2 跟随系统
    pub theme_mode: usize,
    /// 上一次处理 Ctrl 组合键的时刻（毫秒）。
    ///
    /// **去重用途**：`on_key`（窗口层）与 `on_key_capture`（元素层）可能
    /// 对同一次按键各产生一个 `Msg::GridKey`。若不去重，Ctrl+V 会粘贴两遍。
    /// 这里记录时间戳，80ms 内的第二次视为重复并丢弃。
    pub last_ctrl_ms: u64,
    /// 剪贴板（当前是单个单元格的文本）
    /// 剪贴板**两层结构**：
    /// - `text`：纯文本（TSV），与系统剪贴板 / 结果表展示一致
    /// - `block`：二维块，粘贴时按形状展开到光标处
    /// 复制时两者同时生成，粘贴优先用 block，缺则回落到按 TSV 解析 text。
    pub clipboard: Option<Clipboard>,
    /// 分析方法（0 描述统计 / 1 t 检验 / 2 方差分析 / 3 回归）
    pub analysis: usize,
    /// t 检验是否用 Welch
    ///
    /// 已收敛为 `params.welch`，保留此字段仅为兼容旧存盘格式；
    /// 读写请一律走 `params`。
    pub welch: bool,
    /// 分析配置的单一事实源（19 字段，见 analysis::AnalysisParams）
    pub params: crate::analysis::AnalysisParams,
    /// 当前工程文件路径（保存 / 打开用）
    pub file_path: Option<String>,
}

impl Default for SpectraApp {
    fn default() -> Self {
        Self::new()
    }
}

/// 剪贴板的两层表示。
#[derive(Clone, Default)]
pub struct Clipboard {
    /// 纯文本（TSV / 单值）
    pub text: String,
    /// 二维块：Some 表示是区域复制
    pub block: Option<Vec<Vec<String>>>,
}

/// 分组表 → 双因素 ANOVA 的 cells[行][列]。
///
/// 行 = 行标题（第一个分组变量），列 = 数据集（第二个分组变量），
/// 每个单元格是该格内的所有重复值。行标题为空的行会被跳过。
fn two_way_cells(table: &crate::model::SpectraTable) -> Vec<Vec<Vec<f64>>> {
    let mut rows: Vec<Vec<Vec<f64>>> = Vec::new();
    let n_row = table.row_titles.len().max(table.max_used_row() + 1);
    for r in 0..n_row {
        let title = table.row_title(r);
        if title.trim().is_empty() {
            continue;
        }
        let mut row: Vec<Vec<f64>> = Vec::new();
        for (di, _ds) in table.data_sets.iter().enumerate() {
            let vals = table.sub_column_values(di);
            let mut cell = Vec::new();
            for sub in vals.iter() {
                if let Some(v) = sub.get(r).copied() {
                    cell.push(v);
                }
            }
            if !cell.is_empty() {
                row.push(cell);
            }
        }
        if row.len() >= 2 {
            rows.push(row);
        }
    }
    rows
}

impl SpectraApp {
    pub fn new() -> Self {
        let project = sample_project();
        let snap = snapshot(&project.tables[0]);
        let mut app = Self {
            project,
            active: Active::Data(0),
            open_folders: [true, true, true, true, true],
            search: String::new(),
            goto: String::new(),
            welcome_open: true,
            welcome_mode: 0,
            dlg_open_at: std::time::Instant::now(),
            last_analysis: None,
            last_analysis_ms: 0,
            result_view_data: None,
            pick_table: 0,
            pick_existing: 0,
            cursor: (0, 0),
            editing: false,
            draft: String::new(),
            hover: None,
            rows_hint: crate::settings::get().grid_rows.max(8),
            cols_hint: crate::settings::get().grid_cols.max(2),
            show_inspector: true,
            lang: Lang::Zh,
            settings_open: false,
            export_open: false,
            ribbon_menu: None,
            export_fmt: 0,
            about_open: false,
            settings_tab: 0,
            lang_draft: Lang::Zh,
            dark: false,
            export_backend: export::ExportBackend::PlottersSvg,
            active_tool: "",
            status: String::new(),
            alert: None,
            chart_dirty: true,
            settings: settings::get(),
            settings_draft: settings::get(),
            slider_last: std::time::Instant::now(),
            slider_drawn: std::time::Instant::now(),
            undo: Vec::new(),
            redo: Vec::new(),
            clipboard: None,
            analysis: 0,
            welch: true,
            params: crate::analysis::AnalysisParams::default(),
            file_path: None,
            theme_mode: 0,
            last_ctrl_ms: 0,
            freeze_table: false,
            highlight: 7,
            main_toolbar: true,
            navigator: true,
            bottom_toolbar: true,
            rulers: false,
            grid_view: true,
            row_titles_on: true,
            col_titles_on: true,
            zoom: 1.0,
            renaming: None,
            rename_draft: String::new(),
            sheet_renaming: None,
            frozen: Vec::new(),
            sel_from: None,
            sel_to: None,
            sel_drag: false,
            hover_from: None,
            ctx_menu: None,
            pending_focus: false,
            focus_ready: false,
            pending_clear_sel: false,
            reverse_legend: false,
            seed: 0x9E3779B9,
            snap,
            chart: None,
            chart_w: CHART_W as f32,
            chart_h: CHART_H as f32,
            base_w: CHART_W,
            base_h: CHART_H,
        };
        // 主题必须在首帧之前应用：否则 `self.dark` 一直是 false，
        // 设置里存了「深色」也会以浅色启动（表现就是「深色模式没做」）。
        app.apply_theme();
        app.render_chart();
        app
    }

    // ── 图表 ────────────────────────────────────────────────────────────────
    fn chart_view(&self) -> ChartView {
        // 空保护：新建 / 全部关闭后的工程没有图形，也没有表。
        // 以前这里直接 `graphs[0]`，新建项目就会 panic（exit code 101）。
        // 用**当前打开的那张图**，而不是永远第一张 ——
        // 否则新建多张图后预览始终显示 graphs[0]，看起来像「新建覆盖了原来的」。
        let gi = match self.active {
            Active::Graph(i) => i,
            _ => 0,
        };
        let Some(g) = self.project.graphs.get(gi).or_else(|| self.project.graphs.first()) else {
            return ChartView::default();
        };
        let Some(t) = self.project.tables.get(g.source_table) else {
            return ChartView::default();
        };
        // **只画有数据的列**：空数据集（一个值都没有）不参与作图，
        // 否则右边会多出几根高度为 0 的柱子，白占图例与横轴空间。
        let groups: Vec<GroupStat> = t
            .table
            .data_sets
            .iter()
            .enumerate()
            .filter_map(|(i, d)| {
                let values = t.table.flat_values(i);
                if values.is_empty() {
                    return None;
                }
                let (mean, sd, n) = t.table.mean_sd(i);
                Some(GroupStat {
                    name: d.name.clone(),
                    mean,
                    sd,
                    n,
                    values,
                })
            })
            .collect();
        // 显著性对比引用的组号可能因为过滤而越界，夹到有效范围内
        let _ = ();
        ChartView {
            kind: g.kind,
            groups,
            title: g.title.clone(),
            x_name: g.x_name.clone(),
            x_unit: g.x_unit.clone(),
            y_name: g.y_name.clone(),
            y_unit: g.y_unit.clone(),
            unit_style: g.unit_style,
            use_italic: g.use_italic,
            x_sym: g.x_sym.clone(),
            y_sym: g.y_sym.clone(),
            bracket_style: g.bracket_style,
            bar_width_cm: g.bar_width_cm,
            axis_break: g.axis_break,
            right_axis: g.right_axis,
            right_from: g.right_from,
            caption: g.caption.clone(),
            grid_style: g.grid_style,
            dash: g.dash,
            show_error: g.show_error,
            show_scatter: g.show_scatter,
            show_legend: g.show_legend,
            show_bracket: g.show_bracket,
            bracket_mode: g.bracket_mode,
            bracket_value: g.bracket_value,
            colormap: g.colormap,
            comparisons: g.comparisons.clone(),
            annotations: g.annotations.clone(),
            symbol_follows_bar: g.symbol_follows_bar,
            symbol_color_idx: g.symbol_color_idx,
            symbol_size: g.symbol_size,
            jitter: g.jitter,
            // 预览用「Zoom × 超采样倍率」，字号与线宽随画布一起放大，
            // 这样缩小到显示尺寸时边缘是锐的
            scale: self.zoom * CHART_DPR,
            reverse_legend: self.reverse_legend,
            frame: g.frame,
            seed: self.seed,
            err_kind: g.err_kind,
            err_dir: g.err_dir,
            err_width: g.err_width,
            connect: g.connect,
            bar_width: g.bar_width,
            bar_pattern: g.bar_pattern,
            bar_zero: g.bar_zero,
            area_fill: g.area_fill,
            grid_on: g.grid_on,
            log_y: g.log_y,
            grayscale: g.grayscale,
            legend_pos: g.legend_pos,
            line_width: g.line_width,
            symbol_shape: g.symbol_shape,
            symbol_border: g.symbol_border,
            symbol_alpha: g.symbol_alpha,
            title_bold: g.title_bold,
            title_italic: g.title_italic,
            bins: g.bins,
            pie_inner: g.pie_inner,
            pie_start: g.pie_start,
            heat_auto: g.heat_auto,
            heat_lo: g.heat_lo,
            heat_hi: g.heat_hi,
            forest_ref: g.forest_ref,
            surv_censor: g.surv_censor,
            waffle_cols: g.waffle_cols,
            stacked_100: g.stacked_100,
            facet_shared: g.facet_shared,
            box_style: g.box_style,
        }
    }

    fn render_chart(&mut self) {
        // 保证字体已注册（Once 保证只做一次；此时通常后台线程早已完成）
        crate::fonts::register();
        // 位图按 DPR 倍渲染（清晰），显示时缩回逻辑尺寸（看得全）
        let k = self.zoom * CHART_DPR;
        let view = self.chart_view();
        // 显著性括号把画布往上扩出的高度（画布像素）。
        // 显示尺寸必须同步加上（除 DPR 回到逻辑像素），否则画布变高、显示框不变，
        // 整张图会被垂直压扁 —— Y 轴看上去又变短了。
        let extra = plot::bracket_extra_h(&view).max(0) as f32;
        let w = (self.base_w as f32 * k).max(240.0) as u32;
        let h = (self.base_h as f32 * k).max(180.0) as u32;
        self.chart_w = self.base_w as f32 * self.zoom;
        self.chart_h = self.base_h as f32 * self.zoom + extra / CHART_DPR;
        self.chart = plot::render(&view, w, h);
        self.chart_dirty = false;
    }

    /// 图表分组名（= 数据表的列组标题），检查器色板用。
    fn group_names(&self, graph: usize) -> Vec<String> {
        let Some(g) = self.project.graphs.get(graph) else {
            return Vec::new();
        };
        let gi = g.source_table;
        match self.project.tables.get(gi) {
            Some(t) => t.dataset_names(),
            None => Vec::new(),
        }
    }

    // ── 导航器查询 ──────────────────────────────────────────────────────────
    pub fn folder_count(&self, folder: usize) -> usize {
        match folder {
            0 => self.project.tables.len(),
            1 => self.project.infos.len(),
            2 => self.project.results.len(),
            3 => self.project.graphs.len(),
            _ => self.project.layouts.len(),
        }
    }

    pub fn sheet_name(&self, folder: usize, index: usize) -> String {
        match folder {
            0 => self.project.tables[index].name.clone(),
            1 => self.project.infos[index].clone(),
            2 => self.project.results[index].name.clone(),
            3 => self.project.graphs[index].name.clone(),
            _ => self.project.layouts[index].clone(),
        }
    }

    pub fn matches_search(&self, folder: usize, index: usize) -> bool {
        let q = self.search.trim().to_lowercase();
        if q.is_empty() {
            return true;
        }
        self.sheet_name(folder, index).to_lowercase().contains(&q)
    }

    /// 与当前表同族的表单：析光用粗体标识。
    pub fn is_related(&self, folder: usize, index: usize) -> bool {
        match self.active {
            Active::Data(d) => (folder == 2 || folder == 3) && index == 0 && d == 0,
            _ => false,
        }
    }

    /// 冻结的表单：析光用斜体标识（数据变化时不再更新）。
    pub fn is_frozen(&self, folder: usize, index: usize) -> bool {
        self.frozen.contains(&(folder, index))
    }

    /// Family 面板内容：(根名, [(缩进, 名称, 文件夹, 索引)])
    #[allow(dead_code)]
    pub fn family_of_active(&self) -> (String, Vec<(f32, String, usize, usize)>) {
        match self.active {
            Active::Data(i) => {
                let root = self.project.tables[i].name.clone();
                let mut v = Vec::new();
                for (j, r) in self.project.results.iter().enumerate() {
                    v.push((1.0, r.name.clone(), 2, j));
                }
                for (j, g) in self.project.graphs.iter().enumerate() {
                    v.push((1.0, g.name.clone(), 3, j));
                }
                (root, v)
            }
            _ => (self.sheet_label(), Vec::new()),
        }
    }

    pub fn sheet_label(&self) -> String {
        match self.active {
            Active::Data(i) => self.project.tables[i].name.clone(),
            Active::Info(i) => self.project.infos[i].clone(),
            Active::Result(i) => self.project.results[i].name.clone(),
            Active::Graph(i) => self.project.graphs[i].name.clone(),
            Active::Layout(i) => self.project.layouts[i].clone(),
        }
    }

    #[allow(dead_code)]
    pub fn project_hint(&self) -> String {
        ui::status::project_hint(
            self.lang,
            self.project.tables.len(),
            self.project.results.len(),
            self.project.graphs.len(),
        )
    }

    /// 按当前 active 与语言重算结果页的翻译缓存。
    fn refresh_result_cache(&mut self) {
        let a = self.active;
        let precomputed: Option<(usize, Lang, Vec<Vec<String>>, Vec<String>, String)> =
            match a {
                Active::Result(i) => self.project.results.get(i).map(|r| {
                    (
                        i,
                        self.lang,
                        i18n::translate_rows(self.lang, &r.rows),
                        r.tabs.iter().map(|x| i18n::stat_term(self.lang, x)).collect(),
                        i18n::translate_title(self.lang, &r.name),
                    )
                }),
                _ => None,
            };
        self.result_view_data = precomputed;
    }

    fn set_active(&mut self, a: Active) {
        // 切换工作表 = 离开当前网格：选区清理推迟到帧末（同一套延迟机制）
        self.pending_clear_sel = true;
        self.active = a;
        // 预计算结果页的翻译内容：缓存键含下标与语言，换页自然失效。
        self.refresh_result_cache();
        // 切到图表页时才按需重绘（脏了才画）
        if matches!(a, Active::Graph(_)) && self.chart_dirty {
            self.render_chart();
        }
        if let Active::Data(i) = a {
            if i < self.project.tables.len() {
                self.refresh_snap();
                self.rows_hint = self.rows_hint.max(64);
                self.cursor = (0, 0);
                self.editing = false;
                self.draft.clear();
            }
        }
        self.status = self.sheet_label();
    }

    // ── 单元格编辑 ──────────────────────────────────────────────────────────

    /// 当前活动的数据表索引（非数据表时给 0）。
    fn table_index(&self) -> usize {
        match self.active {
            Active::Data(i) => i,
            _ => 0,
        }
    }

    /// 重建只读快照。只在数据变化后调用（稀疏表克隆代价 ≈ 已写入单元格数）。
    fn refresh_snap(&mut self) {
        if self.project.tables.is_empty() {
            return;
        }
        let i = self.table_index().min(self.project.tables.len() - 1);
        self.snap = snapshot(&self.project.tables[i]);
    }

    /// 记录一个撤销点（在**修改之前**调用）。栈深上限 50。
    fn push_undo(&mut self) {
        self.undo.push(self.project.clone());
        if self.undo.len() > 50 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn undo(&mut self) {
        if let Some(p) = self.undo.pop() {
            self.redo.push(self.project.clone());
            self.project = p;
            self.after_project_change();
            self.status = i18n::t(self.lang, K::Undo).to_string();
        } else {
            self.alert(i18n::t(self.lang, K::NoUndo).to_string());
        }
    }

    fn redo(&mut self) {
        if let Some(p) = self.redo.pop() {
            self.undo.push(self.project.clone());
            self.project = p;
            self.after_project_change();
            self.status = i18n::t(self.lang, K::Redo).to_string();
        }
    }

    /// 工程被整体替换后统一收尾：重建快照 + 标脏图表。
    /// 「显著性标注条默认结构」→ 比较对列表。
    ///
    /// 0 无 / 1 相邻两组 / 2 全部两两 / 3 都与第一组比。
    /// 组数取自当前图形；没有图形时按 2 组给一对，保证有个可用的起点。
    fn default_comparisons(&self) -> Vec<crate::plot::Comparison> {
        let st = &self.settings;
        let label = if st.def_bracket_label.is_empty() {
            "*".to_string()
        } else {
            st.def_bracket_label.clone()
        };
        let n = self
            .project
            .graphs
            .last()
            .map(|_| self.group_names(self.project.graphs.len() - 1).len())
            .unwrap_or(2)
            .max(2);
        let pairs: Vec<(usize, usize)> = match st.def_bracket {
            1 => (0..n.saturating_sub(1)).map(|i| (i, i + 1)).collect(),
            2 => {
                let mut v = Vec::new();
                for a in 0..n {
                    for b in (a + 1)..n {
                        v.push((a, b));
                    }
                }
                v
            }
            3 => (1..n).map(|i| (0, i)).collect(),
            _ => Vec::new(),
        };
        pairs
            .into_iter()
            .map(|(a, b)| crate::plot::Comparison {
                a,
                b,
                label: label.clone(),
                p: None,
            })
            .collect()
    }

    /// 弹系统「打开」对话框并载入工程。
    ///
    /// 菜单「打开…」与欢迎界面「打开一个文件」共用这一条路径，
    /// 保证两处行为完全一致（同一组过滤器、同一种格式判定）。
    pub fn open_project_flow(&mut self) {
        let title = i18n::t(self.lang, K::Open).to_string();
        let Some(path) = crate::filedlg::open_file(&title, &crate::filedlg::OPEN_FILTERS) else {
            return;
        };
        // 按**内容**判定格式：.sprx / .prism / 旧版纯文本都能开，
        // 改名不影响打开（但保存仍按后缀分派）。
        let read = std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| crate::sprxfile::read_any(&bytes));
        match read {
            Ok(p) => {
                self.project = p;
                self.file_path = Some(path.clone());
                self.rows_hint = 64;
                self.cursor = (0, 0);
                self.editing = false;
                self.draft.clear();
                self.after_project_change();
                self.set_active(Active::Data(0));
                self.status = format!("{} {}", i18n::t(self.lang, K::Loaded), path);
            }
            Err(e) => self.alert(e.to_string()),
        }
    }

    /// 把设置里的「外观」类真正应用到运行时。
    fn apply_theme(&mut self) {
        self.dark = self.settings.is_dark();
        // 同步给 palette：色板函数每帧被调用上百次，
        // 走原子布尔而不是每次读 settings（见 palette::dark() 的说明）
        crate::palette::set_dark(self.dark);
        crate::ui::anim::set_enabled(self.settings.animate);
        crate::ui::anim::set_ms(self.settings.anim_ms);
    }

    /// 把设置里的其余项应用到运行时状态（只改能立刻生效的）。
    fn apply_settings(&mut self) {
        let st = &self.settings;
        // 面板显隐：设置与 View 菜单共用同一批开关
        self.navigator = st.show_navigator;
        self.show_inspector = st.show_inspector;
        self.bottom_toolbar = st.show_status;
        // 随机数种子：影响散点抖动与「重新模拟」
        self.seed = st.seed;
        // ── 默认绘图项同步到当前图表 ──────────────────────────────────────
        // 设置里改了「默认配色 / 默认误差棒 / 默认散点 / 默认连线」，
        // 当前这张图必须立刻跟着变。以前这些默认值只在**新建**图表时读一次，
        // 于是「在设置里选了配色，绘图区却毫无反应」—— 默认值形同虚设。
        if let Active::Graph(i) = self.active {
            if let Some(g) = self.project.graphs.get_mut(i) {
                g.colormap = st.def_colormap;
                g.show_error = st.def_error;
                g.show_scatter = st.def_points;
                g.connect = st.def_line;
            }
        }
        // 导出宽度
        self.mark_chart_dirty();
        if matches!(self.active, Active::Graph(_)) {
            self.render_chart();
        }
    }

    fn after_project_change(&mut self) {
        self.refresh_snap();
        self.chart_dirty = true;
        if matches!(self.active, Active::Graph(_)) {
            self.render_chart();
        }
    }

    /// 关闭所有覆盖层（欢迎 / 设置 / 关于）。
    /// 打开任何一个新弹窗前调用它 —— 弹窗同时只能存在一个。
    fn close_all_overlays(&mut self) {
        self.welcome_open = false;
        self.settings_open = false;
        self.about_open = false;
    }

    /// 标脏：不立刻重绘。只有图表页可见时才真正渲染。
    /// 滑块拖动：先只记数值，不立刻重绘。
    ///
    /// charis 的 slider 没有「松手」事件，所以用一个**停顿检测**代替：
    /// 两次滑块事件间隔超过阈值说明用户停下来了（或松手了），此时才画。
    /// 另外加一个节流上限，保证拖动过程中也有低频反馈而不是完全黑屏。
    fn mark_slider(&mut self) {
        self.chart_dirty = true;
        let now = std::time::Instant::now();
        let paused = now.duration_since(self.slider_last).as_millis() >= SLIDER_PAUSE_MS as u128;
        let throttled = now.duration_since(self.slider_drawn).as_millis() >= SLIDER_THROTTLE_MS as u128;
        self.slider_last = now;
        if paused || throttled {
            self.slider_drawn = now;
            if matches!(self.active, Active::Graph(_)) {
                self.render_chart();
            }
        }
    }

    fn mark_chart_dirty(&mut self) {
        self.chart_dirty = true;
        // 图表正显示在屏幕上 → 立刻重绘，保证所见即所得
        if matches!(self.active, Active::Graph(_)) {
            self.render_chart();
        }
    }

    /// 当前结果表索引（不在结果页时取 0）。
    fn result_index(&self) -> usize {
        match self.active {
            Active::Result(i) => i,
            _ => 0,
        }
    }

    /// 菜单复选 / 单选的当前值。
    fn menu_state(&self) -> ui::menubar::MenuState {
        ui::menubar::MenuState {
            freeze_table: self.freeze_table,
            highlight: self.highlight,
            zoom: self.zoom,
            main_toolbar: self.main_toolbar,
            navigator: self.navigator,
            inspector: self.show_inspector,
            bottom_toolbar: self.bottom_toolbar,
            rulers: self.rulers,
            grid: self.grid_view,
            row_titles: self.row_titles_on,
            column_titles: self.col_titles_on,
        }
    }

    // ══ File 的实现 ══════════════════════════════════════════════════════

    /// 另存为。默认写 `.sprx`（本应用专有格式，ZIP 容器 + 快照，无损）。
    fn save_as(&mut self) {
        // 弹系统「另存为」对话框，让用户自己选存到哪、用什么格式。
        // 不再硬写当前目录下的 project.sprx —— 那种做法用户根本不知道文件去了哪。
        let title = i18n::t(self.lang, K::SaveAs).to_string();
        let default = self
            .file_path
            .clone()
            .unwrap_or_else(|| "project.sprx".to_string());
        let Some(path) = crate::filedlg::save_file(&title, &default, &crate::filedlg::PROJECT_FILTERS)
        else {
            // 用户取消：什么都不写
            return;
        };
        self.file_path = Some(path.clone());
        self.write_project(&path);
    }

    /// 写盘：按后缀分派 `.sprx` / `.prism`，两者都是 ZIP 容器。
    ///
    /// 统一走 `write_atomic`：先写临时文件再 rename，
    /// 中断（崩溃、断电）不会破坏原有工程。
    fn write_project(&mut self, path: &str) -> bool {
        let bytes = match crate::sprxfile::write_by_ext(&self.project, path) {
            Ok(b) => b,
            Err(e) => {
                // .prism 的写前自检失败：宁可报错也不产出会被 Prism 判定损坏的文件
                self.alert(e);
                return false;
            }
        };
        match crate::sprxfile::writer::write_atomic(path, &bytes) {
            Ok(()) => {
                self.status = format!("{} {}", i18n::t(self.lang, K::Saved), path);
                self.maybe_backup();
                true
            }
            Err(e) => {
                self.alert(e.to_string());
                false
            }
        }
    }

    /// 另存为族（F）：工程 + 每张数据表一份 CSV，一起落到选定目录。
    ///
    /// Prism 的「族」指同一工程下相互关联的一组表与图；这里落成
    /// 「一个工程文件 + N 张表的 CSV」，目录内自洽，可直接归档或发给别人。
    fn save_family(&mut self) {
        let title = i18n::t(self.lang, K::MiSaveFamily).to_string();
        let Some(dir) = crate::filedlg::pick_folder(&title) else {
            return;
        };
        let stem = self
            .file_path
            .as_deref()
            .and_then(|p| std::path::Path::new(p).file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "family".to_string());
        let proj = std::path::Path::new(&dir).join(format!("{}.sprx", stem));
        let proj_s = proj.to_string_lossy().to_string();
        if !self.write_project_to(&proj_s, false) {
            return;
        }
        let mut n = 0usize;
        for t in &self.project.tables {
            let csv = t.table.as_matrix_csv();
            let f = std::path::Path::new(&dir).join(format!("{}_{}.csv", stem, t.name));
            if std::fs::write(&f, csv).is_ok() {
                n += 1;
            }
        }
        self.status = format!("{} {} ({} CSV)", i18n::t(self.lang, K::Saved), dir, n);
    }

    /// 另存为示例（E）：写到工程所在目录的 `examples/` 子目录。
    fn save_example(&mut self) {
        let base = self
            .file_path
            .as_deref()
            .and_then(|p| std::path::Path::new(p).parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let dir = base.join("examples");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.alert(e.to_string());
            return;
        }
        let stem = self
            .file_path
            .as_deref()
            .and_then(|p| std::path::Path::new(p).file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "example".to_string());
        let p = dir.join(format!("{}.sprx", stem));
        let s = p.to_string_lossy().to_string();
        self.write_project_to(&s, false);
    }

    /// 另存为模板（T）：清空所有单元格，但保留表结构、列名、图表与分析设置。
    ///
    /// 模板的用途是「下次新建时直接套用版式」，所以数据必须清掉，
    /// 而列数、子列、图型、配色这些要原样留下。
    fn save_template(&mut self) {
        let mut p = self.project.clone();
        for t in p.tables.iter_mut() {
            t.table.clear_values();
            t.excluded.clear();
        }
        // 结果表随数据一起失效，模板里不该留着上一次的分析结果
        p.results.clear();
        let title = i18n::t(self.lang, K::MiSaveTemplate).to_string();
        let Some(path) =
            crate::filedlg::save_file(&title, "template.sprx", &crate::filedlg::PROJECT_FILTERS)
        else {
            return;
        };
        let bytes = match crate::sprxfile::write_by_ext(&p, &path) {
            Ok(b) => b,
            Err(e) => {
                self.alert(e);
                return;
            }
        };
        match crate::sprxfile::writer::write_atomic(&path, &bytes) {
            Ok(()) => self.status = format!("{} {}", i18n::t(self.lang, K::Saved), path),
            Err(e) => self.alert(e.to_string()),
        }
    }

    /// 另存为方法（M）：把当前分析配置导出成一份 JSON。
    fn save_method(&mut self) {
        let title = i18n::t(self.lang, K::MiSaveMethod).to_string();
        let Some(path) = crate::filedlg::save_file(&title, "method.json", &[("JSON", "json")])
        else {
            return;
        };
        let name = crate::analysis::AnalysisKind::all()
            .get(self.analysis)
            .map(|k| k.zh())
            .unwrap_or("");
        let doc = serde_json::json!({
            "app": "spectra",
            "kind": "analysis-method",
            "analysisId": self.analysis,
            "analysis": name,
            "params": {
                "conf": self.params.conf,
                "alpha": self.params.alpha,
                "welch": self.params.welch,
                "mu0": self.params.mu0,
                "yates": self.params.yates,
                "spearman": self.params.spearman,
                "throughOrigin": self.params.through_origin,
                "compareSlope": self.params.compare_slope,
                "demingRatio": self.params.deming_ratio,
                "clusters": self.params.clusters,
                "nPoints": self.params.n_points,
                "paired": self.params.paired,
            },
        });
        match serde_json::to_string_pretty(&doc) {
            Ok(s) => match std::fs::write(&path, s) {
                Ok(()) => self.status = format!("{} {}", i18n::t(self.lang, K::Saved), path),
                Err(e) => self.alert(e.to_string()),
            },
            Err(e) => self.alert(e.to_string()),
        }
    }

    /// 复制族：复制当前表，并复制引用它的全部图表与结果表。
    fn duplicate_family(&mut self) {
        let ti = self.table_index();
        if ti >= self.project.tables.len() {
            return;
        }
        self.push_undo();
        let src = self.project.tables[ti].clone();
        let mut dst = src.clone();
        dst.name = format!("{} copy", src.name);
        self.project.tables.push(dst);
        let new_i = self.project.tables.len() - 1;
        let graphs: Vec<_> = self
            .project
            .graphs
            .iter()
            .filter(|g| g.source_table == ti)
            .cloned()
            .collect();
        for mut g in graphs {
            g.source_table = new_i;
            g.name = format!("{} copy", g.name);
            self.project.graphs.push(g);
        }
        self.set_active(Active::Data(new_i));
        self.status = i18n::t(self.lang, K::TableDuplicated).to_string();
    }

    /// 合并文件（M）：把另一个工程里的表追加进当前工程。
    ///
    /// 只并表与图，不并设置 —— 设置是「当前工程」的，被合并方带进来会打架。
    fn merge_file(&mut self) {
        let title = i18n::t(self.lang, K::MiMergeFile).to_string();
        let Some(path) = crate::filedlg::open_file(&title, &crate::filedlg::OPEN_FILTERS) else {
            return;
        };
        let other = std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|b| crate::sprxfile::read_any(&b));
        let Ok(other) = other else {
            self.alert(i18n::t(self.lang, K::OpenFailed).to_string());
            return;
        };
        self.push_undo();
        let base_g = self.project.graphs.len();
        let mut nt = 0usize;
        for t in other.tables {
            self.project.tables.push(t);
            nt += 1;
        }
        for mut g in other.graphs {
            g.source_table += self.project.tables.len() - nt;
            self.project.graphs.push(g);
        }
        let _ = base_g;
        self.after_project_change();
        self.status = format!("{} {} ({} tables)", i18n::t(self.lang, K::Loaded), path, nt);
    }

    /// 写盘到指定路径。`change_path` 决定是否把它记为当前工程路径。
    fn write_project_to(&mut self, path: &str, change_path: bool) -> bool {
        let bytes = match crate::sprxfile::write_by_ext(&self.project, path) {
            Ok(b) => b,
            Err(e) => {
                self.alert(e);
                return false;
            }
        };
        match crate::sprxfile::writer::write_atomic(path, &bytes) {
            Ok(()) => {
                self.status = format!("{} {}", i18n::t(self.lang, K::Saved), path);
                if change_path {
                    self.file_path = Some(path.to_string());
                }
                true
            }
            Err(e) => {
                self.alert(e.to_string());
                false
            }
        }
    }

    /// 删除光标所在的物理列。
    fn delete_col_at_cursor(&mut self) {
        let ti = self.table_index();
        if ti >= self.project.tables.len() {
            return;
        }
        let ncol = self.project.tables[ti].table.total_physical_columns();
        if ncol <= 1 {
            self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
            return;
        }
        let c = self.cursor.0.min(ncol - 1);
        self.push_undo();
        {
            let t = &mut self.project.tables[ti];
            t.table.delete_physical_col(c);
            // 排除标记也按物理列存，删列后要跟着平移
            let ex = std::mem::take(&mut t.excluded);
            t.excluded = ex
                .into_iter()
                .filter_map(|(pc, r)| {
                    if pc == c {
                        None
                    } else {
                        Some((if pc > c { pc - 1 } else { pc }, r))
                    }
                })
                .collect();
        }
        self.after_project_change();
        self.cursor.0 = c.min(self.phys_cols().saturating_sub(1));
        self.status = i18n::t(self.lang, K::DeleteColDone).to_string();
    }

    /// 删除光标所在的整个数据集（含其全部子列）。
    fn delete_dataset_at_cursor(&mut self) {
        let ti = self.table_index();
        if ti >= self.project.tables.len() {
            return;
        }
        if self.project.tables[ti].table.data_sets.len() <= 1 {
            self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
            return;
        }
        let (di, _) = self.project.tables[ti].table.phys_to_logic(self.cursor.0);
        self.push_undo();
        {
            let t = &mut self.project.tables[ti];
            let start = t.table.dataset_start_col(di);
            let w = t.table.data_sets[di].width().max(1);
            t.table.delete_data_set(di);
            let ex = std::mem::take(&mut t.excluded);
            t.excluded = ex
                .into_iter()
                .filter_map(|(pc, r)| {
                    if pc >= start && pc < start + w {
                        None
                    } else {
                        Some((if pc > start { pc - w } else { pc }, r))
                    }
                })
                .collect();
        }
        self.after_project_change();
        self.cursor.0 = 0;
        self.status = i18n::t(self.lang, K::DeleteDataSetDone).to_string();
    }

    /// 自动备份：在工程旁写一份 `.bak`。
    ///
    /// 只在设置开启时做，且失败不打断用户 —— 备份是保险，不该因为
    /// 备份失败就让保存失败。
    fn maybe_backup(&mut self) {
        if !self.settings.auto_backup {
            return;
        }
        let Some(p) = self.file_path.clone() else { return };
        let _ = std::fs::copy(&p, format!("{}.bak", p));
    }

    /// 「建表前先存盘」：开启时，新建数据表前先把当前工程写盘，
    /// 避免新建动作覆盖掉还没保存的内容。
    fn guard_save_before_table(&mut self) -> bool {
        if !self.settings.save_before_table {
            return true;
        }
        if self.file_path.is_none() {
            self.save_as();
        } else {
            let path = self.file_path.clone().unwrap();
            if !self.write_project(&path) {
                return false;
            }
        }
        true
    }

    /// 关闭当前工作表：数据表真的删掉，图表 / 结果切走即可。
    fn close_sheet(&mut self) {
        match self.active {
            Active::Data(i) if self.project.tables.len() > 1 => {
                self.project.tables.remove(i);
                let j = i.min(self.project.tables.len() - 1);
                self.set_active(Active::Data(j));
                self.status = i18n::t(self.lang, K::CloseSheet).to_string();
            }
            Active::Result(i) if !self.project.results.is_empty() => {
                self.project.results.remove(i.min(self.project.results.len() - 1));
                self.set_active(Active::Data(0));
                self.status = i18n::t(self.lang, K::CloseSheet).to_string();
            }
            _ => self.alert(i18n::t(self.lang, K::LastSheetKept).to_string()),
        }
    }

    /// 当前数据表 → CSV。
    fn save_table_csv(&mut self) {
        let Some(t) = self.project.tables.get(self.table_index()) else {
            self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
            return;
        };
        let mut csv = String::new();
        let ncol = t.table.data_sets.len().max(1);
        for r in 0..t.view_rows.min(100_000) {
            let cells: Vec<String> = (0..ncol).map(|c| t.display_at(c, r)).collect();
            csv.push_str(&cells.join(","));
            csv.push('\n');
        }
        match std::fs::write("table.csv", csv) {
            Ok(()) => self.status = format!("{} table.csv", i18n::t(self.lang, K::ExportedCsv)),
            Err(e) => self.alert(e.to_string()),
        }
    }

    /// 当前结果表 → CSV。
    fn save_results_csv(&mut self) {
        let Some(r) = self.project.results.get(self.result_index()) else {
            self.status = i18n::t(self.lang, K::NoResults).to_string();
            return;
        };
        let mut csv = String::new();
        for row in &r.rows {
            csv.push_str(&row.join(","));
            csv.push('\n');
        }
        match std::fs::write("results.csv", csv) {
            Ok(()) => self.status = format!("{} results.csv", i18n::t(self.lang, K::ExportedCsv)),
            Err(e) => self.alert(e.to_string()),
        }
    }

    /// 当前图形 → 按设置的后端导出。
    /// 导出弹窗里点「导出」：先让用户挑保存位置，再按选中格式写盘。
    ///
    /// 与 `export_current_graph` 的区别：那条走设置里的后端（svg/tex/py 三选一），
    /// 这条走弹窗里的 12 种格式，且**必须由用户选路径** —— 以前硬写
    /// `spectra_chart.png` 到程序当前目录，用户根本不知道文件去了哪。
    fn export_with_dialog(&mut self) {
        self.export_open = false;
        let fmt = crate::export::ExportFormat::ALL
            .get(self.export_fmt)
            .copied()
            .unwrap_or(crate::export::ExportFormat::Png);
        let (fname, fext) = fmt.filter();
        let default = format!("spectra_chart.{}", fmt.ext());
        let title = i18n::t(self.lang, K::Export).to_string();
        let Some(path) = crate::filedlg::save_file(&title, &default, &[(fname, fext)]) else {
            return;
        };
        // 用户在对话框里可能手改了后缀，也可能系统补了一个。
        // 这里统一按选中格式重写后缀，避免「选了 JPEG 却写出 PNG 内容」。
        let stem = match path.rfind('.') {
            Some(i) if path.rfind(std::path::MAIN_SEPARATOR).map_or(true, |s| i > s) => &path[..i],
            _ => path.as_str(),
        };
        let final_path = format!("{}.{}", stem, fmt.ext());
        match crate::export::export_as(&self.chart_view(), fmt, &final_path) {
            Ok(()) => {
                self.status = format!("{} {}", i18n::t(self.lang, K::Exported), final_path)
            }
            Err(e) => self.alert(e),
        }
    }

    fn export_current_graph(&mut self) {
        // 让用户挑导出到哪：默认文件名按当前后端给后缀。
        let (default, filters): (String, Vec<(String, String)>) = match self.export_backend {
            crate::export::ExportBackend::PlottersSvg => (
                "spectra_chart.svg".to_string(),
                vec![("SVG".to_string(), "svg".to_string())],
            ),
            crate::export::ExportBackend::TikZ => (
                "spectra_chart.tex".to_string(),
                vec![("LaTeX".to_string(), "tex".to_string())],
            ),
            crate::export::ExportBackend::Matplotlib => (
                "spectra_chart.py".to_string(),
                vec![("Python".to_string(), "py".to_string())],
            ),
        };
        let title = i18n::t(self.lang, K::Export).to_string();
        let refs: Vec<(&str, &str)> = filters.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let Some(path) = crate::filedlg::save_file(&title, &default, &refs) else {
            return;
        };
        // 去掉扩展名交给 export，它会按后端补回正确的后缀
        let stem = match path.rfind('.') {
            Some(i) if path.rfind(std::path::MAIN_SEPARATOR).map_or(true, |s| i > s) => &path[..i],
            _ => path.as_str(),
        };
        match crate::export::export(&self.chart_view(), self.export_backend, stem) {
            Ok(p) => self.status = format!("{} {}", i18n::t(self.lang, K::Exported), p),
            Err(e) => self.alert(e),
        }
    }

    /// 发送到 Office：导出 SVG/PNG 后打开；`ppt`/`doc` 只是提示路径，
    /// 因为 Rust 侧无法直接驱动 Office 的 COM 插入。
    fn send_to_office(&mut self, kind: &str) {
        let stem = "spectra_chart";
        match crate::export::export(
            &self.chart_view(),
            crate::export::ExportBackend::PlottersSvg,
            stem,
        ) {
            Ok(p) => {
                let _ = open_with_default(&p);
                self.status = format!("{} {} ({})", i18n::t(self.lang, K::Exported), p, kind);
            }
            Err(e) => self.alert(e),
        }
    }

    // ══ Edit 的实现 ══════════════════════════════════════════════════════

    /// 转置粘贴：把剪贴板的块**转置**后写入（行列互换）。
    fn paste_transposed(&mut self) {
        let cb = match self.clipboard.clone() {
            Some(c) => c,
            None => {
                self.alert(i18n::t(self.lang, K::NoClipboard).to_string());
                return;
            }
        };
        // 没有块（单格复制）时，按 TSV 文本拆成一行再转置成一列
        let block: Vec<Vec<String>> = match cb.block {
            Some(b) => b,
            None => vec![cb
                .text
                .split('\t')
                .map(|x| x.to_string())
                .collect()],
        };
        self.push_undo();
        let (c0, r0) = self.cursor;
        let ti = self.table_index();
        if let Some(t) = self.project.tables.get_mut(ti) {
            // 转置：block[行][列] → 写到 (列偏移, 行偏移) 的对称位置
            for (dr, row) in block.iter().enumerate() {
                for (dc, val) in row.iter().enumerate() {
                    let r = r0.saturating_add(dc);
                    let c = if c0 == usize::MAX {
                        usize::MAX
                    } else {
                        c0.saturating_add(dr)
                    };
                    if c == usize::MAX {
                        t.table.ensure_row_title(r, val.clone());
                    } else {
                        t.write_str(c, r, val);
                    }
                }
            }
        }
        self.after_project_change();
        self.status = i18n::t(self.lang, K::Transposed).to_string();
    }

    /// 全选：把光标范围记为整张表（用于后续的复制 / 清除）。
    fn select_all(&mut self) {
        let ti = self.table_index();
        if let Some(t) = self.project.tables.get(ti) {
            let ncol = t.table.total_physical_columns().max(1);
            let nrow = t.view_rows.max(1);
            self.sel_from = Some((0, 0));
            self.sel_to = Some((ncol - 1, nrow - 1));
            self.status = i18n::t(self.lang, K::SelectAllDone).to_string();
        }
    }

    /// 按 X 值或行标题排序（只重排「已使用的行」，稀疏行不受影响）。
    fn sort_rows_by(&mut self, by_x: bool) {
        let ti = self.table_index();
        if ti >= self.project.tables.len() {
            return;
        }
        self.push_undo();
        {
            let t = &mut self.project.tables[ti];
            let nrow = t.view_rows.max(t.table.row_titles.len()).max(1);
            let key: Vec<(usize, String)> = (0..nrow)
                .map(|r| {
                    let k = if by_x {
                        t.display_at(0, r)
                    } else {
                        t.table.row_titles.get(r).cloned().unwrap_or_default()
                    };
                    (r, k)
                })
                .collect();
            // 数值优先：能解析成 f64 就按数值排，否则按字典序
            let mut order: Vec<usize> = (0..nrow).collect();
            order.sort_by(|&a, &b| {
                let (ka, kb) = (&key[a].1, &key[b].1);
                match (ka.parse::<f64>(), kb.parse::<f64>()) {
                    (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap(),
                    _ => ka.cmp(kb),
                }
            });
            // 按 order 重排行：先把数据取出来再按新顺序写回
            let ncol = t.table.total_physical_columns().max(1);
            let old: Vec<Vec<crate::model::CellValue>> = (0..nrow)
                .map(|r| (0..ncol).map(|c| t.table.get_cell(r, c)).collect())
                .collect();
            let titles = t.table.row_titles.clone();
            for (new_r, &old_r) in order.iter().enumerate() {
                for c in 0..ncol {
                    t.table.set_cell(new_r, c, old[old_r][c].clone());
                }
                let ti2 = titles.get(old_r).cloned().unwrap_or_default();
                t.table.ensure_row_title(new_r, ti2);
            }
        }
        self.after_project_change();
        self.status = i18n::t(self.lang, K::SortDone).to_string();
    }

    /// 反转行序。
    fn reverse_row_order(&mut self) {
        let ti = self.table_index();
        if ti >= self.project.tables.len() {
            return;
        }
        self.push_undo();
        {
            let t = &mut self.project.tables[ti];
            let nrow = t.view_rows.max(t.table.row_titles.len()).max(1);
            let ncol = t.table.total_physical_columns().max(1);
            let old: Vec<Vec<crate::model::CellValue>> = (0..nrow)
                .map(|r| (0..ncol).map(|c| t.table.get_cell(r, c)).collect())
                .collect();
            let titles = t.table.row_titles.clone();
            for r in 0..nrow {
                let src = nrow - 1 - r;
                for c in 0..ncol {
                    t.table.set_cell(r, c, old[src][c].clone());
                }
                t.table
                    .ensure_row_title(r, titles.get(src).cloned().unwrap_or_default());
            }
        }
        self.after_project_change();
        self.status = i18n::t(self.lang, K::SortDone).to_string();
    }

    /// 删除当前数据表。至少保留一张。
    fn delete_table(&mut self) {
        if self.project.tables.len() <= 1 {
            self.alert(i18n::t(self.lang, K::LastSheetKept).to_string());
            return;
        }
        self.push_undo();
        let i = self.table_index();
        self.project.tables.remove(i);
        let j = i.min(self.project.tables.len() - 1);
        self.set_active(Active::Data(j));
        self.status = i18n::t(self.lang, K::TableDeleted).to_string();
    }

    // ══ 导航器条目操作（右键菜单）═══════════════════════════════════════

    /// 打开导航器里的某个表单。
    fn open_sheet_at(&mut self, folder: usize, index: usize) {
        if index >= self.folder_count(folder) {
            return;
        }
        let a = match folder {
            0 => Active::Data(index),
            1 => Active::Info(index),
            2 => Active::Result(index),
            3 => Active::Graph(index),
            _ => Active::Layout(index),
        };
        self.set_active(a);
    }

    /// 导航器就地改名：该行变成输入框，回车或点别处提交。
    fn start_sheet_rename(&mut self, folder: usize, index: usize) {
        self.sheet_renaming = Some((folder, index));
        self.rename_draft = self.sheet_name(folder, index);
    }

    fn commit_sheet_rename(&mut self) {
        let Some((folder, index)) = self.sheet_renaming.take() else {
            return;
        };
        let name = self.rename_draft.trim().to_string();
        self.rename_draft.clear();
        if name.is_empty() || index >= self.folder_count(folder) {
            return;
        }
        self.push_undo();
        match folder {
            0 => self.project.tables[index].name = name,
            1 => self.project.infos[index] = name,
            2 => self.project.results[index].name = name,
            3 => self.project.graphs[index].name = name,
            _ => self.project.layouts[index] = name,
        }
        self.status = i18n::t(self.lang, K::SheetRenamed).to_string();
        if folder == 2 {
            self.refresh_result_cache();
        }
    }

    /// 创建副本（表 / 信息 / 结果 / 图形 / 布局各按其类型复制）。
    fn duplicate_sheet_at(&mut self, folder: usize, index: usize) {
        if index >= self.folder_count(folder) {
            return;
        }
        self.push_undo();
        match folder {
            0 => {
                let Some(src) = self.project.tables.get(index).cloned() else {
                    return;
                };
                let mut dst = src;
                dst.name = format!("{} copy", dst.name);
                self.project.tables.push(dst);
                self.set_active(Active::Data(self.project.tables.len() - 1));
            }
            2 => {
                let Some(src) = self.project.results.get(index).cloned() else {
                    return;
                };
                let new_name = format!("{} copy", src.name);
                let mut dst = src;
                dst.name = new_name;
                self.project.results.push(dst);
                self.set_active(Active::Result(self.project.results.len() - 1));
            }
            3 => {
                let Some(src) = self.project.graphs.get(index).cloned() else {
                    return;
                };
                let new_name = format!("{} copy", src.name);
                let mut dst = src;
                dst.name = new_name;
                self.project.graphs.push(dst);
                self.set_active(Active::Graph(self.project.graphs.len() - 1));
            }
            1 => {
                let src = self.project.infos[index].clone();
                self.project.infos.push(format!("{} copy", src));
                self.set_active(Active::Info(self.project.infos.len() - 1));
            }
            _ => {
                let src = self.project.layouts[index].clone();
                self.project.layouts.push(format!("{} copy", src));
                self.set_active(Active::Layout(self.project.layouts.len() - 1));
            }
        }
        self.status = i18n::t(self.lang, K::SheetDuplicated).to_string();
    }

    /// 删除导航器里的某个表单。数据表至少保留一张（与 delete_table 同规则）。
    fn delete_sheet_at(&mut self, folder: usize, index: usize) {
        if index >= self.folder_count(folder) {
            return;
        }
        if folder == 0 && self.project.tables.len() <= 1 {
            self.alert(i18n::t(self.lang, K::LastSheetKept).to_string());
            return;
        }
        self.push_undo();
        match folder {
            0 => {
                self.project.tables.remove(index);
                // 图 / 结果引用的是表下标，删掉一张后必须重映射，
                // 否则会指向错表或直接越界。
                for g in &mut self.project.graphs {
                    if g.source_table > index {
                        g.source_table -= 1;
                    } else if g.source_table == index {
                        g.source_table = 0;
                    }
                }
                for r in &mut self.project.results {
                    if let Some(s) = r.source_table {
                        if s > index {
                            r.source_table = Some(s - 1);
                        } else if s == index {
                            r.source_table = Some(0);
                        }
                    }
                }
            }
            1 => {
                self.project.infos.remove(index);
            }
            2 => {
                self.project.results.remove(index);
            }
            3 => {
                self.project.graphs.remove(index);
            }
            _ => {
                self.project.layouts.remove(index);
            }
        }
        // 冻结标记跟着一起平移
        let mut next = Vec::new();
        for (f, i) in self.frozen.drain(..) {
            if f != folder {
                next.push((f, i));
            } else if i > index {
                next.push((f, i - 1));
            }
        }
        self.frozen = next;
        // 当前打开的是被删的那一族：落到同族的下一张，空了则回家
        let n = self.folder_count(folder);
        if self.active.folder() == folder {
            if n == 0 {
                self.set_active(Active::Data(0));
            } else {
                let j = index.min(n - 1);
                self.open_sheet_at(folder, j);
            }
        }
        self.status = i18n::t(self.lang, K::SheetDeleted).to_string();
    }

    /// 冻结 / 解冻。冻结的图形不参与「数据一变就重绘」。
    fn toggle_freeze_at(&mut self, folder: usize, index: usize) {
        if let Some(p) = self.frozen.iter().position(|&f| f == (folder, index)) {
            self.frozen.remove(p);
        } else {
            self.frozen.push((folder, index));
        }
        self.status = if self.is_frozen(folder, index) {
            i18n::t(self.lang, K::FreezeTable).to_string()
        } else {
            i18n::t(self.lang, K::CtxUnfreeze).to_string()
        };
    }

    // ══ Insert 的实现 ════════════════════════════════════════════════════

    /// 复制当前表（结构与数据一起复制）。
    fn duplicate_table(&mut self) {
        let Some(src) = self.project.tables.get(self.table_index()).cloned() else {
            return;
        };
        self.push_undo();
        let mut dst = src.clone();
        dst.name = format!("{} copy", src.name);
        self.project.tables.push(dst);
        let n = self.project.tables.len() - 1;
        self.set_active(Active::Data(n));
        self.status = i18n::t(self.lang, K::TableDuplicated).to_string();
    }

    /// 复制系列：把第 0 个数据集复制一份追加到末尾。
    fn duplicate_series(&mut self) {
        let ti = self.table_index();
        if ti >= self.project.tables.len() {
            return;
        }
        self.push_undo();
        {
            let t = &mut self.project.tables[ti];
            let n = t.table.data_sets.len();
            let src = t.table.data_sets.first().cloned();
            if let Some(mut ds) = src {
                ds.name = t.table.dataset_letter(n);
                t.table.data_sets.push(ds);
            }
        }
        self.after_project_change();
        self.status = i18n::t(self.lang, K::AddDataSet).to_string();
    }

    // ══ Change 的实现 ════════════════════════════════════════════════════

    /// 反转数据集顺序：数据集与物理列一起镜像。
    fn reverse_data_set_order(&mut self) {
        let ti = self.table_index();
        if ti >= self.project.tables.len() {
            return;
        }
        self.push_undo();
        {
            let t = &mut self.project.tables[ti];
            let nrow = t.view_rows.max(t.table.row_titles.len()).max(1);
            let ncol = t.table.total_physical_columns().max(1);
            let old: Vec<Vec<crate::model::CellValue>> = (0..nrow)
                .map(|r| (0..ncol).map(|c| t.table.get_cell(r, c)).collect())
                .collect();
            t.table.data_sets.reverse();
            // 物理列整体镜像
            for r in 0..nrow {
                for c in 0..ncol {
                    t.table.set_cell(r, c, old[r][ncol - 1 - c].clone());
                }
            }
        }
        self.after_project_change();
        self.mark_chart_dirty();
        self.status = i18n::t(self.lang, K::ReverseDsOrder).to_string();
    }

    /// 重新模拟：换随机种子重新生成散点的抖动相位。
    fn resimulate(&mut self) {
        self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.mark_chart_dirty();
        self.status = i18n::t(self.lang, K::Resimulate).to_string();
    }

    // ══ View 的实现 ══════════════════════════════════════════════════════

    /// 设置缩放并立即重绘。
    fn set_zoom(&mut self, z: f32) {
        self.zoom = z.clamp(0.10, 6.0);
        self.mark_chart_dirty();
        self.status = format!("{}%", (self.zoom * 100.0) as i32);
    }

    /// 新增一条标注。位置按已有标注数量错开，避免叠在一起。
    fn add_annotation(&mut self, kind: crate::model::AnnKind) {
        if self.project.graphs.is_empty() {
            return;
        }
        self.push_undo();
        for g in self.project.graphs.iter_mut() {
            let k = g.annotations.len() as f32;
            let y = 0.78 - (k % 5.0) * 0.12;
            g.annotations.push(crate::model::Annotation {
                kind,
                x: 0.18,
                y,
                x2: 0.62,
                y2: y - 0.10,
                text: if kind == crate::model::AnnKind::Text {
                    "标注".to_string()
                } else {
                    String::new()
                },
                size: g.ann_size,
            });
        }
        self.mark_chart_dirty();
        self.status = kind.label().to_string();
    }

    /// Import：从当前目录读 `import.csv` / `import.tsv`，写入当前数据表。
    fn import_csv(&mut self) {
        use crate::model::CellValue;
        // 弹系统「打开」对话框挑文件，不再依赖程序目录下恰好有个 import.csv
        let title = i18n::t(self.lang, K::Import).to_string();
        let filters = [
            ("CSV / TSV", "csv"),
            ("TSV", "tsv"),
            ("文本", "txt"),
            ("所有文件", "*"),
        ];
        let (path, txt) = match crate::filedlg::open_file(&title, &filters) {
            None => return, // 用户取消
            Some(p) => match std::fs::read_to_string(&p) {
                Ok(t) => (p, t),
                Err(e) => {
                    self.alert(e.to_string());
                    return;
                }
            },
        };
        self.push_undo();
        let ti = self.table_index();
        let (mut rows, mut cols) = (0usize, 0usize);
        if ti < self.project.tables.len() {
            let t = &mut self.project.tables[ti];
            for (r, line) in txt.lines().enumerate() {
                if line.trim().is_empty() {
                    continue;
                }
                let sep = if line.contains('\t') { '\t' } else { ',' };
                for (c, field) in line.split(sep).enumerate() {
                    let v = field.trim();
                    if v.is_empty() {
                        continue;
                    }
                    let cell = match v.parse::<f64>() {
                        Ok(n) => CellValue::Number(n),
                        Err(_) => CellValue::Text(v.to_string()),
                    };
                    t.table.set_cell(r, c, cell);
                    cols = cols.max(c + 1);
                }
                rows = rows.max(r + 1);
            }
            t.view_rows = t.view_rows.max(rows.max(8));
        }
        self.rows_hint = self.rows_hint.max(rows + 32);
        self.after_project_change();
        self.status = format!("{} {} ({} × {})", i18n::t(self.lang, K::Loaded), path, rows, cols);
    }

    /// Print：导出 600 dpi 级别的 PNG 并用系统默认程序打开。
    fn print_chart(&mut self) {
        match plot::export_png(&self.chart_view(), "spectra_print.png", 6.0) {
            Ok(()) => {
                let _ = open_with_default("spectra_print.png");
                self.status = format!("{} spectra_print.png", i18n::t(self.lang, K::Exported));
            }
            Err(e) => self.alert(e),
        }
    }

    /// Send：导出 SVG 后打开系统邮件客户端（正文带上文件路径）。
    fn send_chart(&mut self) {
        let stem = "spectra_send";
        match crate::export::export(&self.chart_view(), self.export_backend, stem) {
            Ok(p) => {
                let abs = std::fs::canonicalize(&p)
                    .map(|x| x.to_string_lossy().to_string())
                    .unwrap_or(p.clone());
                let subj = urlencode(&self.sheet_label());
                let _ = open_with_default(&format!(
                    "mailto:?subject={}&body={}",
                    subj,
                    urlencode(&abs)
                ));
                self.status = format!("{} {}", i18n::t(self.lang, K::Exported), p);
            }
            Err(e) => self.alert(e),
        }
    }

    /// LabArchives：导出「数据包」（CSV + SVG + PNG）到一个子目录。
    fn export_bundle(&mut self) {
        let dir = "labarchives_export";
        if let Err(e) = std::fs::create_dir_all(dir) {
            self.status = e.to_string();
            return;
        }
        let mut ok = Vec::new();
        // 数据表 → CSV
        if let Some(t) = self.project.tables.get(self.table_index()) {
            let mut csv = String::new();
            let ncol = t.table.data_sets.len().max(1);
            for r in 0..t.view_rows.min(100000) {
                let mut cells = Vec::new();
                for c in 0..ncol {
                    cells.push(t.display_at(c, r));
                }
                csv.push_str(&cells.join(","));
                csv.push('\n');
            }
            let p = format!("{}/data.csv", dir);
            if std::fs::write(&p, csv).is_ok() {
                ok.push(p);
            }
        }
        // 图形 → SVG 与 PNG
        if crate::export::export(&self.chart_view(), crate::export::ExportBackend::PlottersSvg, &format!("{}/graph", dir)).is_ok() {
            ok.push(format!("{}/graph.svg", dir));
        }
        if plot::export_png(&self.chart_view(), &format!("{}/graph.png", dir), 3.0).is_ok() {
            ok.push(format!("{}/graph.png", dir));
        }
        self.status = format!("{} {}", i18n::t(self.lang, K::Exported), ok.join(", "));
    }

    /// 复制当前单元格到剪贴板。
    /// 复制时的数值格式化（规范 §6）。
    /// `copy_decimals`：0 尽可能多 / 1 两位 / 2 四位 / 3 六位；
    /// `copy_decimal_sep = 1` 时小数分隔符用逗号、千位改用句点（欧洲写法）。
    /// 非数值（文本、空）原样返回，绝不改写用户数据。
    fn fmt_cell_for_copy(raw: &str) -> String {
        let t = raw.trim();
        if t.is_empty() {
            return raw.to_string();
        }
        let Ok(v) = t.parse::<f64>() else {
            return raw.to_string();
        };
        if !v.is_finite() {
            return raw.to_string();
        }
        let dp = match crate::settings::with(|s| s.copy_decimals) {
            1 => Some(2usize),
            2 => Some(4),
            3 => Some(6),
            _ => None,
        };
        let body = match dp {
            Some(d) => format!("{:.*}", d, v),
            None => {
                // 尽可能多：12 位输出后去掉尾随 0
                let s = format!("{:.*}", 12, v);
                if s.contains('.') {
                    s.trim_end_matches('0').trim_end_matches('.').to_string()
                } else {
                    s
                }
            }
        };
        if crate::settings::with(|s| s.copy_decimal_sep) != 1 {
            return body;
        }
        // 千位 '.' + 小数 ','
        let (neg, b) = match body.strip_prefix('-') {
            Some(x) => (true, x),
            None => (false, body.as_str()),
        };
        let (ip, fp) = match b.split_once('.') {
            Some((a, bb)) => (a, Some(bb)),
            None => (b, None),
        };
        let chars: Vec<char> = ip.chars().collect();
        let mut out = String::new();
        for (i, ch) in chars.iter().enumerate() {
            if i > 0 && (chars.len() - i) % 3 == 0 {
                out.push('.');
            }
            out.push(*ch);
        }
        if let Some(f) = fp {
            out.push(',');
            out.push_str(f);
        }
        format!("{}{}", if neg { "-" } else { "" }, out)
    }

    /// 写入系统剪贴板（可被其它程序读取）。失败静默，不因剪贴板不可用而崩。
    fn clipboard_set(text: &str) {
        if let Ok(mut c) = arboard::Clipboard::new() {
            let _ = c.set_text(text.to_string());
        }
    }

    /// 读取系统剪贴板（可从 Excel 直接粘贴）。读不到返回 None，回落到内部缓冲。
    fn clipboard_get() -> Option<String> {
        arboard::Clipboard::new().ok().and_then(|mut c| c.get_text().ok())
    }

    fn copy_cell(&mut self) {
        let i = self.table_index();
        if i >= self.project.tables.len() {
            return;
        }
        let t = &self.project.tables[i];
        let (txt, block) = match self.sel_range() {
            // 区域复制：text 用 TSV（可直接粘进 Excel），block 用二维结构
            Some((c0, r0, c1, r1)) => {
                // 列清单必须单独算：行标题列用 usize::MAX 表示，
                // 直接 `for c in c0..=c1` 在 c0 == usize::MAX 时会得到空区间
                // （行标题就复制不出来），而 c1 == usize::MAX 时区间会长到天上去。
                let cols: Vec<usize> = if c0 == usize::MAX {
                    vec![usize::MAX]
                } else {
                    (c0..=c1).collect()
                };
                let mut lines: Vec<String> = Vec::new();
                let mut blk: Vec<Vec<String>> = Vec::new();
                let excl = t.excluded.clone();
                for r in r0..=r1 {
                    let mut row: Vec<String> = Vec::new();
                    for &c in &cols {
                        let is_ex = c != usize::MAX && excl.iter().any(|&(ec, er)| ec == c && er == r);
                        if is_ex {
                            // 排除值：0 加 * 后缀 / 1 置空 / 2 原样（规范 §6）
                            row.push(match crate::settings::with(|s| s.copy_excluded) {
                                0 => format!("{}*", t.display_at(c, r)),
                                1 => String::new(),
                                _ => t.display_at(c, r),
                            });
                            continue;
                        }
                        let v = if c == usize::MAX {
                            t.table.row_title(r).to_string()
                        } else {
                            Self::fmt_cell_for_copy(&t.display_at(c, r))
                        };
                        row.push(v);
                    }
                    lines.push(row.join("\t"));
                    blk.push(row);
                }
                (lines.join("\n"), Some(blk))
            }
            None => (Self::fmt_cell_for_copy(&self.current_cell_text()), None),
        };
        Self::clipboard_set(&txt);
        self.clipboard = Some(Clipboard { text: txt, block });
        self.note(i18n::t(self.lang, K::CopyDone).to_string());
    }

    /// 当前选区（列0, 行0, 列1, 行1），已归一化。
    fn sel_range(&self) -> Option<(usize, usize, usize, usize)> {
        let (a, b) = (self.sel_from?, self.sel_to?);
        let (c0, c1) = (a.0.min(b.0), a.0.max(b.0));
        let (r0, r1) = (a.1.min(b.1), a.1.max(b.1));
        Some((c0, r0, c1, r1))
    }

    fn current_cell_text(&self) -> String {
        let i = self.table_index();
        if i >= self.project.tables.len() {
            return String::new();
        }
        let t = &self.project.tables[i];
        let (c, r) = self.cursor;
        if c == usize::MAX {
            t.table.row_title(r).to_string()
        } else {
            t.display_at(c, r)
        }
    }

    /// 剪切：复制 + 清空。
    fn cut_cell(&mut self) {
        self.copy_cell();
        self.push_undo();
        let i = self.table_index();
        // 有选区就清整块，否则只清当前格
        let cells: Vec<(usize, usize)> = match self.sel_range() {
            Some((c0, r0, c1, r1)) => {
                let mut v = Vec::new();
                for r in r0..=r1 {
                    for c in c0..=c1 {
                        v.push((c, r));
                    }
                }
                v
            }
            None => vec![self.cursor],
        };
        if let Some(t) = self.project.tables.get_mut(i) {
            for (c, r) in cells {
                if c == usize::MAX {
                    t.table.ensure_row_title(r, String::new());
                } else {
                    t.table.set_cell(r, c, CellValue::Empty);
                }
            }
        }
        self.draft.clear();
        self.after_project_change();
        self.status = i18n::t(self.lang, K::Cut).to_string();
    }

    /// 粘贴：按类型校验后写入（列联表取整、多变量表允许文本）。
    fn paste_cell(&mut self) {
        // 优先读**系统剪贴板**（这样能直接从 Excel 粘进来），
        // 读不到才回落到内部缓冲（程序内复制的那份）。
        let cb = match Self::clipboard_get() {
            Some(txt) if !txt.trim().is_empty() => {
                // Windows 剪贴板用 CRLF，Excel 复制出来的也是。
                // 不归一化的话每个字段末尾会带一个 \r，
                // 数值解析失败 → write_str 返回 false → 一格都写不进去，
                // 但状态栏照样显示「已粘贴」，表现成「粘了却没数据」。
                let txt = txt.replace("\r\n", "\n").replace('\r', "\n");
                let block: Vec<Vec<String>> = txt
                    .trim_end_matches('\n')
                    .split('\n')
                    .map(|line| {
                        // 兼容 CSV：没有 Tab 但有多个逗号时按逗号切
                        if !line.contains('\t') && line.matches(',').count() > 0 {
                            line.split(',').map(|x| x.trim().to_string()).collect()
                        } else {
                            line.split('\t').map(|x| x.to_string()).collect()
                        }
                    })
                    .collect();
                Clipboard { text: txt, block: Some(block) }
            }
            _ => match self.clipboard.clone() {
                Some(c) if !c.text.trim().is_empty() => c,
                _ => {
                    self.alert(i18n::t(self.lang, K::NoClipboard).to_string());
                    return;
                }
            },
        };
        // ── 编辑态：粘贴进输入框，不要去改单元格 ──────────────────────────
        // 单击单元格就进入输入态，此时 Ctrl+V 的预期是「文本进输入框」。
        // 以前这里照旧写单元格，紧接着 `self.draft = current_cell_text()`
        // 又把草稿覆盖回去；提交时草稿原样写回，粘贴的内容等于凭空消失 ——
        // 状态栏却已经报了「已粘贴」。这就是「显示了粘贴就是没有数据」。
        if self.editing {
            let one_line = cb.text.replace('\n', " ").trim().to_string();
            if one_line.is_empty() {
                self.alert(i18n::t(self.lang, K::NoClipboard).to_string());
                return;
            }
            self.draft.push_str(&one_line);
            self.status = i18n::t(self.lang, K::PasteDone).to_string();
            return;
        }
        self.push_undo();
        let i = self.table_index();
        // 落点是**选区的左上角**，不是光标（活动端点）。
        // 框选一大片后光标停在右下角，若按光标落点粘贴，数据会整体偏移到
        // 选区右下方 —— 规范要求的落点是 (c0, r0)。
        let (c0, r0) = match self.sel_range() {
            Some((a, b, _, _)) => (a, b),
            None => self.cursor,
        };
        // 优先按二维块展开；没有块就用文本按 TSV / 换行解析
        let block: Vec<Vec<String>> = match cb.block {
            Some(b) => b,
            None => cb
                .text
                .split('\n')
                .map(|line| line.split('\t').map(|x| x.to_string()).collect())
                .collect(),
        };
        // 统计真正写进去的格数。以前不管写没写进去都报「已粘贴」，
        // 于是「类型不接受 / 内容为空」这类失败全部伪装成成功。
        let mut written = 0usize;
        let mut rejected = 0usize;
        if let Some(t) = self.project.tables.get_mut(i) {
            for (dr, row) in block.iter().enumerate() {
                for (dc, val) in row.iter().enumerate() {
                    let r = r0.saturating_add(dr);
                    let c = if c0 == usize::MAX {
                        usize::MAX
                    } else {
                        c0.saturating_add(dc)
                    };
                    if c == usize::MAX {
                        t.table.ensure_row_title(r, val.clone());
                        written += 1;
                    } else if t.write_str(c, r, val) {
                        written += 1;
                    } else if !val.trim().is_empty() {
                        rejected += 1;
                    }
                }
            }
        }
        self.after_project_change();
        self.ensure_row(r0.saturating_add(block.len()));
        if c0 != usize::MAX {
            self.ensure_col(c0.saturating_add(block.first().map(|r| r.len()).unwrap_or(1)));
        }
        self.draft = self.current_cell_text();
        // 明确告知，不再「什么都没写进去也报已粘贴」
        self.status = if written == 0 {
            i18n::t(self.lang, K::PasteNothing).to_string()
        } else if rejected > 0 {
            i18n::t(self.lang, K::PastePartial).replace("{}", &rejected.to_string())
        } else {
            i18n::t(self.lang, K::PasteDone).to_string()
        };
    }

    /// 清空当前单元格。
    fn clear_cell(&mut self) {
        self.push_undo();
        let i = self.table_index();
        let (c, r) = self.cursor;
        if let Some(t) = self.project.tables.get_mut(i) {
            if c == usize::MAX {
                t.table.ensure_row_title(r, String::new());
            } else {
                t.table.set_cell(r, c, CellValue::Empty);
            }
        }
        self.draft.clear();
        self.after_project_change();
        self.status = i18n::t(self.lang, K::Cleared).to_string();
    }

    /// 运行真实分析：按所选方法算出结果，写进 Results 文件夹并打开。
    /// 按方法 id 跑分析。id 由 `TableType::analyses()` 给出，同 id 跨类型语义一致。
    ///
    /// 未实现的 id 会明确告知，而不是编造数字 —— 宁可少一个功能，也不放一个假结果。
    fn run_analysis(&mut self) {
        let ti = self.table_index();
        // 静默 return 是「点了没反应」最常见的来源：项目里一张表都没有时
        // `table_index()` 返回 0，而 0 >= 0 成立，于是什么都不发生、连提示都没有。
        // 用户看到的就是「点什么都出不来数据」。这里必须明确告知。
        if ti >= self.project.tables.len() {
            self.alert(i18n::t(self.lang, K::SelectTableFirst).to_string());
            return;
        }
        let method = self.analysis;
        // ── 节流：同一个方法 300ms 内不重复执行 ──────────────────────────
        // 右边栏按钮在滚动容器里，子树每帧重建；命中测试一旦让 on_click 在一帧内
        // 被重复派发，这里就会被连跑几十次 —— 每次都要 push_undo() 克隆整个
        // project 再跑一遍统计，于是「一点就卡死」。菜单栏是静态容器不会这样，
        // 这正是「点菜单不卡、点右边栏卡」的原因。300ms 内的重复必然是误触发，
        // 真人连续两次点击不可能快过这个间隔。
        let now = std::time::Instant::now();
        if let Some((m, at)) = self.last_analysis {
            if m == method && now.duration_since(at).as_millis() < 300 {
                return;
            }
        }
        self.last_analysis = Some((method, now));
        // 设置里的「多重比较方法」在运行时生效：分析参数里的 method 是默认档，
        // 用户在设置里改了就该用设置里的，否则改了存得住却看不出区别。
        self.params.method = self.settings.multi_method;
        self.params.model = self.settings.nl_model;
        let t0 = std::time::Instant::now();
        let t = &self.project.tables[ti];
        // **空列不参与分析**：一个值都没有的列当成 0 会得出错误的统计量。
        // 名字与数据同步过滤，保证结果表里的列与真实数据一一对应。
        let mut names: Vec<String> = Vec::new();
        let mut cols: Vec<Vec<f64>> = Vec::new();
        for (i, nm) in t.dataset_names().iter().enumerate() {
            let v = t.table.flat_values(i);
            if v.is_empty() {
                continue;
            }
            names.push(nm.clone());
            cols.push(v);
        }
        if cols.is_empty() {
            self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
            return;
        }

        // ── 智能统计制图顾问（决策层）────────────────────────────────────
        // 它是唯一「先看数据再决定方法」的分析：诊断组数 / 正态性 / 方差齐性，
        // 再按决策树给出图表类型与检验方法。放在其它分支之前处理，
        // 因为它不需要 method_id，也不受 params.method 影响。
        // 用专用 id，不能用 `StatsAdvisor as usize`（它是枚举首项，判别值=0，
        // 会撞上菜单 id 0「描述统计」）—— 见 analysis::ADVISOR_ID 的注释。
        if method == crate::analysis::ADVISOR_ID {
            // 先拷出标量，避免 &self.params 的借用挡住后面的 &mut self 调用
            let alpha = crate::analysis::advisor_alpha(&self.params);
            let design = if self.params.paired {
                crate::analysis::Design::Paired
            } else {
                crate::analysis::Design::Independent
            };
            let Some(d) = crate::analysis::Diagnosis::diagnose(&cols, design, alpha) else {
                self.alert(i18n::t(self.lang, K::NeedTwoGroups).to_string());
                return;
            };
            let rec = crate::analysis::Recommendation::recommend(&d);
            // 顾问走完整的多段式报告（数据概况 / 正态性 / 方差齐性 / 决策依据 /
            // 推荐方案 / 注意事项 / 描述统计 / 实际检验结果），不再是一份十几行的
            // 扁平表。此前用 wrap_fallback 会把首行当成表头吞掉，第一行数据丢失。
            let conf = self.params.conf;
            let blocks = crate::report::advisor_report(&d, &rec, &names, &cols, conf, alpha);
            let rows = crate::report::flatten(&blocks);
            // 先取出条数：`rows` 下一步就要被移进 ResultSheet，之后再读会
            // 报 E0382（borrow of moved value）。
            let nrows = rows.len();
            let title = format!("{} / {}", t.name, crate::analysis::AnalysisKind::StatsAdvisor.zh());
            self.push_undo();
            let existing = self.project.results.iter().position(|r| r.name == title);
            let sheet = ResultSheet {
                name: title.clone(),
                tabs: vec!["顾问".to_string()],
                active_tab: 0,
                rows,
                source_table: Some(ti),
            };
            let n = match existing {
                Some(i) => {
                    self.project.results[i] = sheet;
                    i + 1
                }
                None => {
                    self.project.results.push(sheet);
                    self.project.results.len()
                }
            };
            self.open_folders[2] = true;
            self.set_active(Active::Result(n - 1));
            self.last_analysis_ms = t0.elapsed().as_millis();
            self.status = format!(
                "{}（id {} · {} 组 · {} 行 · {} ms）",
                i18n::t(self.lang, K::AnalysisDone),
                method,
                cols.len(),
                nrows,
                self.last_analysis_ms
            );
            return;
        }

        let (title, tabs, rows) = match method {
            // ── 0 描述统计 ──
            0 => (
                format!("{} / 描述统计", t.name),
                vec!["描述统计".to_string()],
                stats::descriptive_rows(&names, &cols),
            ),
            // ── 1 独立样本 t 检验 ──
            1 => {
                if cols.len() < 2 {
                    self.alert(i18n::t(self.lang, K::NeedTwoGroups).to_string());
                    return;
                }
                // Welch 与否收敛在 params 里（见 analysis::AnalysisParams）
                let kind = if self.params.welch {
                    stats::TTestKind::Welch
                } else {
                    stats::TTestKind::Pooled
                };
                match stats::t_test(&cols[0], &cols[1], kind) {
                    Some(mut r) => {
                        r.a = names.get(0).cloned().unwrap_or_else(|| "A 组".into());
                        r.b = names.get(1).cloned().unwrap_or_else(|| "B 组".into());
                        (
                            format!("{} / 独立样本 t 检验", t.name),
                            vec![
                                "表格结果".to_string(),
                                "P 值摘要".to_string(),
                            ],
                            stats::ttest_rows(&r),
                        )
                    }
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                }
            }
            // ── 2 单因素 ANOVA ──
            2 => {
                let n: usize = cols.iter().map(|c| c.len()).sum();
                match stats::anova(&cols) {
                    Some(r) => (
                        format!("{} / 单因素方差分析", t.name),
                        vec!["方差分析结果".to_string()],
                        stats::anova_rows(&r, cols.len(), n),
                    ),
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                }
            }
            // ── 3 线性回归 ──
            3 => {
                // X 在第 0 个数据集；Y 取其后第一个（XY / Survival 布局）
                let yi = if t.table.table_type.first_is_x() && cols.len() > 1 { 1 } else { 0 };
                let xs = cols.first().cloned().unwrap_or_default();
                let ys = cols.get(yi).cloned().unwrap_or_default();
                let len = xs.len().min(ys.len());
                match stats::regression(&xs[..len], &ys[..len]) {
                    Some(r) => (
                        format!("{} / 线性回归", t.name),
                        vec!["回归结果".to_string()],
                        stats::regression_rows(&r),
                    ),
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                }
            }
            // ── 4 嵌套单因素 ANOVA（每个数据集 = 一个处理，子列 = 内层单元）──
            4 => {
                let groups: Vec<Vec<Vec<f64>>> = (0..t.table.data_sets.len())
                    .map(|i| t.table.sub_column_values(i))
                    .collect();
                match stats::nested_anova(&groups) {
                    Some(r) => (
                        format!("{} / 嵌套单因素方差分析", t.name),
                        vec!["嵌套方差分析".to_string()],
                        vec![
                            vec!["变异来源".to_string(), "F 值".to_string(), "P 值".to_string()],
                            vec![
                                "处理（组间）".to_string(),
                                format!("{:.4}", r.f_treat),
                                stats::fmt_p(r.p_treat),
                            ],
                            vec![
                                "嵌套（组内）".to_string(),
                                format!("{:.4}", r.f_nested),
                                stats::fmt_p(r.p_nested),
                            ],
                        ],
                    ),
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                }
            }
            // ── 5 相关矩阵 ──
            5 => {
                let ns: Vec<usize> = cols.iter().map(|c| c.len()).collect();
                match stats::corr_matrix(&cols) {
                    Some(m) => (
                        format!("{} / 相关矩阵", t.name),
                        vec!["相关矩阵".to_string()],
                        stats::corr_rows(&names, &m, &ns),
                    ),
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                }
            }
            // ── 6 正态性（Shapiro–Wilk）──
            6 => (
                format!("{} / 正态性检验", t.name),
                vec!["正态性".to_string()],
                stats::normality_rows(&names, &cols),
            ),
            // ── 7 离群值（Grubbs）──
            7 => (
                format!("{} / 离群值检验", t.name),
                vec!["离群值".to_string()],
                stats::grubbs_rows(&names, &cols),
            ),
            // ── 8 卡方（列联表）──
            8 => {
                let table = t.table.as_matrix();
                match stats::chi_square(&table) {
                    Some(r) => (
                        format!("{} / 卡方检验", t.name),
                        vec!["卡方".to_string()],
                        stats::chisq_rows(&r),
                    ),
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                }
            }
            // ── 12 Fisher 精确检验（2×2）──
            12 => {
                let m = t.table.as_matrix();
                if m.len() != 2 || m[0].len() != 2 {
                    self.status = i18n::t(self.lang, K::NeedTwoByTwo).to_string();
                    return;
                }
                let cell = |i: usize, j: usize| -> f64 {
                    m.get(i).and_then(|r| r.get(j)).copied().unwrap_or(0.0)
                };
                match stats::fisher_exact([
                    [cell(0, 0), cell(0, 1)],
                    [cell(1, 0), cell(1, 1)],
                ]) {
                    Some(r) => (
                        format!("{} / Fisher 精确检验", t.name),
                        vec!["Fisher".to_string()],
                        vec![
                            vec!["P 值（双尾）".to_string(), stats::fmt_p(r.p_two)],
                            vec![
                                "P 值摘要".to_string(),
                                stats::stars(r.p_two).to_string(),
                            ],
                        ],
                    ),
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                }
            }
            // ── 13 相对风险 / 比值比（2×2）──
            13 => {
                let m = t.table.as_matrix();
                if m.len() != 2 || m[0].len() != 2 {
                    self.status = i18n::t(self.lang, K::NeedTwoByTwo).to_string();
                    return;
                }
                let g = |i: usize, j: usize| -> f64 {
                    m.get(i).and_then(|r| r.get(j)).copied().unwrap_or(0.0)
                };
                let (a, b, c, d) = (g(0, 0), g(0, 1), g(1, 0), g(1, 1));
                let rr = if (a + b) > 0.0 && (c + d) > 0.0 {
                    (a / (a + b)) / (c / (c + d))
                } else {
                    f64::NAN
                };
                let or = if b > 0.0 && c > 0.0 { (a * d) / (b * c) } else { f64::NAN };
                (
                    format!("{} / 风险度量", t.name),
                    vec!["风险".to_string()],
                    vec![
                        vec!["相对危险度 RR".to_string(), Self::num_or_dash(rr)],
                        vec!["比值比 OR".to_string(), Self::num_or_dash(or)],
                    ],
                )
            }
            // ── 14 Kaplan–Meier（Survival：第 0 列时间，其余为组）──
            14 => {
                let time = cols.first().cloned().unwrap_or_default();
                let gi = if cols.len() > 1 { 1 } else { 0 };
                let ev = cols.get(gi).cloned().unwrap_or_default();
                let len = time.len().min(ev.len());
                let pts = stats::kaplan_meier(&time[..len], &ev[..len]);
                if pts.is_empty() {
                    self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                    return;
                }
                let mut rows = vec![vec![
                    "时间".to_string(),
                    "生存率".to_string(),
                    "在险人数".to_string(),
                    "事件数".to_string(),
                ]];
                for p in &pts {
                    rows.push(vec![
                        format!("{:.4}", p.time),
                        format!("{:.4}", p.surv),
                        p.at_risk.to_string(),
                        p.events.to_string(),
                    ]);
                }
                (
                    format!("{} / Kaplan-Meier 生存分析", t.name),
                    vec!["生存率".to_string()],
                    rows,
                )
            }
            // ── 16 总分统计（Parts of whole）──
            16 => {
                let total: f64 = cols.iter().flat_map(|c| c.iter()).sum();
                let mut rows = vec![vec![
                    "类别".to_string(),
                    "数值".to_string(),
                    "占比".to_string(),
                ]];
                for (i, c) in cols.iter().enumerate() {
                    let v = c.iter().sum::<f64>();
                    let pct = if total != 0.0 { v / total * 100.0 } else { 0.0 };
                    rows.push(vec![
                        names.get(i).cloned().unwrap_or_else(|| format!("{}", i + 1)),
                        format!("{:.4}", v),
                        format!("{:.2}%", pct),
                    ]);
                }
                rows.push(vec![
                    "总计".to_string(),
                    format!("{:.4}", total),
                    "100.00%".to_string(),
                ]);
                (
                    format!("{} / 总分统计", t.name),
                    vec!["各部分占比".to_string()],
                    rows,
                )
            }
            // ── 17 观察值 vs 期望值（卡方拟合优度）──
            17 => {
                let obs: Vec<f64> = cols.iter().map(|c| c.iter().sum::<f64>()).collect();
                let k = obs.len();
                if k < 2 {
                    self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                    return;
                }
                let total: f64 = obs.iter().sum();
                let exp = total / k as f64;
                let chi2: f64 = obs.iter().map(|o| (o - exp) * (o - exp) / exp).sum();
                let p = stats::gamma_q((k - 1) as f64 / 2.0, chi2 / 2.0);
                let mut rows = vec![vec![
                    "类别".to_string(),
                    "观察值".to_string(),
                    "期望值".to_string(),
                ]];
                for (i, o) in obs.iter().enumerate() {
                    rows.push(vec![
                        names.get(i).cloned().unwrap_or_else(|| format!("{}", i + 1)),
                        format!("{:.4}", o),
                        format!("{:.4}", exp),
                    ]);
                }
                rows.push(vec!["卡方".to_string(), format!("{:.4}", chi2), "".to_string()]);
                rows.push(vec![
                    "P 值".to_string(),
                    stats::fmt_p(p),
                    "".to_string(),
                ]);
                (
                    format!("{} / 观察值与期望值比较", t.name),
                    vec!["拟合优度".to_string()],
                    rows,
                )
            }
            // ── 21 事后多重比较（Tukey HSD）─────────────────────────────
            21 => {
                let pw = match stats::tukey_hsd(&cols, 0.95) {
                    Some(v) => v,
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                };
                (
                    format!("{} / 事后多重比较", t.name),
                    vec!["Tukey 多重比较".to_string()],
                    stats::pairwise_rows(&pw, stats::MultiMethod::Tukey),
                )
            }
            // ── 22 Games-Howell（方差不齐时的事后比较）───────────────────
            22 => {
                let pw = match stats::games_howell(&cols, 0.95) {
                    Some(v) => v,
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                };
                (
                    format!("{} / Games-Howell 多重比较", t.name),
                    vec!["Games-Howell".to_string()],
                    stats::pairwise_rows(&pw, stats::MultiMethod::GamesHowell),
                )
            }
            // ── 23 Dunnett（各组 vs 对照）───────────────────────────────
            23 => {
                let pw = match stats::dunnett(&cols, 0.95) {
                    Some(v) => v,
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                };
                (
                    format!("{} / Dunnett 多重比较", t.name),
                    vec!["Dunnett".to_string()],
                    stats::pairwise_rows(&pw, stats::MultiMethod::Dunnett),
                )
            }
            // ── 24 Dunn（Kruskal-Wallis 之后的两两比较）──────────────────
            24 => {
                let pw = match stats::dunn_test(&cols, 0.95) {
                    Some(v) => v,
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                };
                (
                    format!("{} / Dunn 多重比较", t.name),
                    vec!["Dunn".to_string()],
                    stats::pairwise_rows(&pw, stats::MultiMethod::Dunn),
                )
            }
            // ── 25 Levene / Brown-Forsythe 方差齐性 ──────────────────────
            25 => {
                let lev = match stats::levene(&cols, false) {
                    Some(v) => v,
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                };
                let bf = stats::levene(&cols, true);
                let mut rows = vec![
                    vec!["".to_string(), "Levene".to_string(), "Brown-Forsythe".to_string()],
                    vec![
                        "F 值".to_string(),
                        format!("{:.4}", lev.f),
                        bf.as_ref().map(|b| format!("{:.4}", b.f)).unwrap_or_else(|| "—".into()),
                    ],
                    vec![
                        "P 值".to_string(),
                        stats::fmt_p(lev.p),
                        bf.as_ref().map(|b| stats::fmt_p(b.p)).unwrap_or_else(|| "—".into()),
                    ],
                    vec![
                        "自由度".to_string(),
                        format!("{}, {}", lev.df1 as i64, lev.df2 as i64),
                        bf.as_ref().map(|b| format!("{}, {}", b.df1 as i64, b.df2 as i64))
                            .unwrap_or_else(|| "—".into()),
                    ],
                ];
                rows.push(vec![
                    "结论".to_string(),
                    if lev.p < 0.05 { "方差不齐" } else { "方差齐" }.to_string(),
                    "".to_string(),
                ]);
                (
                    format!("{} / 方差齐性检验", t.name),
                    vec!["Levene".to_string()],
                    rows,
                )
            }
            // ── 26 Mann-Whitney U ────────────────────────────────────────
            26 => {
                if cols.len() < 2 {
                    self.alert(i18n::t(self.lang, K::NeedTwoGroups).to_string());
                    return;
                }
                let r = match stats::mann_whitney(&cols[0], &cols[1]) {
                    Some(v) => v,
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                };
                (
                    format!("{} / Mann-Whitney U 检验", t.name),
                    vec!["Mann-Whitney".to_string()],
                    vec![
                        vec!["".to_string(), "数值".to_string()],
                        vec!["U 值".to_string(), format!("{:.1}", r.u)],
                        vec!["z 值".to_string(), format!("{:.4}", r.z)],
                        vec!["P 值".to_string(), stats::fmt_p(r.p)],
                        vec!["显著性".to_string(), stats::stars(r.p).to_string()],
                    ],
                )
            }
            // ── 27 Wilcoxon 符号秩 ───────────────────────────────────────
            27 => {
                if cols.len() < 2 {
                    self.alert(i18n::t(self.lang, K::NeedTwoGroups).to_string());
                    return;
                }
                let r = match stats::wilcoxon(&cols[0], &cols[1]) {
                    Some(v) => v,
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                };
                (
                    format!("{} / Wilcoxon 符号秩检验", t.name),
                    vec!["Wilcoxon".to_string()],
                    vec![
                        vec!["".to_string(), "数值".to_string()],
                        vec!["W 值".to_string(), format!("{:.1}", r.w)],
                        vec!["z 值".to_string(), format!("{:.4}", r.z)],
                        vec!["P 值".to_string(), stats::fmt_p(r.p)],
                        vec!["显著性".to_string(), stats::stars(r.p).to_string()],
                    ],
                )
            }
            // ── 28 Kruskal-Wallis ────────────────────────────────────────
            28 => {
                let r = match stats::kruskal_wallis(&cols) {
                    Some(v) => v,
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                };
                (
                    format!("{} / Kruskal-Wallis 检验", t.name),
                    vec!["Kruskal-Wallis".to_string()],
                    vec![
                        vec!["".to_string(), "数值".to_string()],
                        vec!["H 值".to_string(), format!("{:.4}", r.h)],
                        vec!["自由度".to_string(), format!("{}", r.df as i64)],
                        vec!["P 值".to_string(), stats::fmt_p(r.p)],
                        vec!["显著性".to_string(), stats::stars(r.p).to_string()],
                    ],
                )
            }
            // ── 29 双因素 ANOVA（重复数可不等）───────────────────────────
            29 => {
                let cells = two_way_cells(&t.table);
                let r = match stats::two_way_anova(&cells) {
                    Some(v) => v,
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                };
                let mut rows = vec![vec![
                    "变异来源".to_string(),
                    "平方和".to_string(),
                    "自由度".to_string(),
                    "F 值".to_string(),
                    "P 值".to_string(),
                ]];
                rows.push(vec![
                    "行因素".to_string(),
                    format!("{:.4}", r.ss_rows),
                    format!("{}", r.df_rows as i64),
                    format!("{:.4}", r.f_rows),
                    stats::fmt_p(r.p_rows),
                ]);
                rows.push(vec![
                    "列因素".to_string(),
                    format!("{:.4}", r.ss_cols),
                    format!("{}", r.df_cols as i64),
                    format!("{:.4}", r.f_cols),
                    stats::fmt_p(r.p_cols),
                ]);
                if r.unbalanced {
                    rows.push(vec![
                        "交互作用".to_string(),
                        "—".to_string(),
                        "—".to_string(),
                        "—".to_string(),
                        "重复数不等，交互项不可估".to_string(),
                    ]);
                } else {
                    rows.push(vec![
                        "交互作用".to_string(),
                        format!("{:.4}", r.ss_inter),
                        format!("{}", r.df_inter as i64),
                        format!("{:.4}", r.f_inter),
                        stats::fmt_p(r.p_inter),
                    ]);
                }
                (
                    format!("{} / 双因素方差分析", t.name),
                    vec!["双因素方差分析".to_string()],
                    rows,
                )
            }
            // ── 9 非线性回归（Levenberg-Marquardt）──────────────────────
            9 => {
                if cols.len() < 2 {
                    self.alert(i18n::t(self.lang, K::NeedTwoGroups).to_string());
                    return;
                }
                let xs = cols[0].clone();
                let ys = cols[1].clone();
                let model = self.params.model;
                match crate::analysis::fit_nonlinear(&xs, &ys, model) {
                    Some(r) => {
                        let mut rows = vec![vec![
                            "参数".to_string(),
                            "数值".to_string(),
                            "标准误".to_string(),
                            "95% CI".to_string(),
                            "t 值".to_string(),
                        ]];
                        let pn = model.param_names();
                        for (i, name) in pn.iter().enumerate() {
                            let v = r.params.get(i).copied().unwrap_or(f64::NAN);
                            let se = r.se.get(i).copied().unwrap_or(f64::NAN);
                            let lo = r.ci_lo.get(i).copied().unwrap_or(f64::NAN);
                            let hi = r.ci_hi.get(i).copied().unwrap_or(f64::NAN);
                            let tt = r.t.get(i).copied().unwrap_or(f64::NAN);
                            rows.push(vec![
                                (*name).to_string(),
                                Self::num_or_dash(v),
                                Self::num_or_dash(se),
                                if lo.is_finite() && hi.is_finite() {
                                    format!("{:.4} 至 {:.4}", lo, hi)
                                } else {
                                    "—".to_string()
                                },
                                Self::num_or_dash(tt),
                            ]);
                        }
                        rows.push(vec![
                            "回归方程".to_string(),
                            model.equation().to_string(),
                            String::new(),
                            String::new(),
                            String::new(),
                        ]);
                        rows.push(vec![
                            "残差平方和".to_string(),
                            Self::num_or_dash(r.sse),
                            String::new(),
                            String::new(),
                            String::new(),
                        ]);
                        rows.push(vec![
                            "R²".to_string(),
                            Self::num_or_dash(r.r2),
                            String::new(),
                            String::new(),
                            String::new(),
                        ]);
                        rows.push(vec![
                            "调整后 R²".to_string(),
                            Self::num_or_dash(r.adj_r2),
                            String::new(),
                            String::new(),
                            String::new(),
                        ]);
                        rows.push(vec![
                            "AICc".to_string(),
                            Self::num_or_dash(r.aicc),
                            String::new(),
                            String::new(),
                            String::new(),
                        ]);
                        rows.push(vec![
                            "是否收敛".to_string(),
                            if r.converged { "是".to_string() } else { "否".to_string() },
                            String::new(),
                            String::new(),
                            String::new(),
                        ]);
                        (
                            format!("{} / 非线性拟合", t.name),
                            vec!["非线性拟合".to_string()],
                            rows,
                        )
                    }
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                }
            }
            // ── 11 Bartlett 方差齐性 ────────────────────────────────────
            11 => {
                match stats::bartlett(&cols) {
                    Some(r) => (
                        format!("{} / Bartlett 方差齐性检验", t.name),
                        vec!["Bartlett".to_string()],
                        vec![
                            vec!["卡方".to_string(), Self::num_or_dash(r.chi2)],
                            vec!["自由度".to_string(), format!("{}", r.df as i64)],
                            vec!["P 值".to_string(), stats::fmt_p(r.p)],
                        ],
                    ),
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                }
            }
            // ── 30 单样本 t 检验（原假设锚定 params.mu0）─────────────────
            30 => {
                let v = cols.first().cloned().unwrap_or_default();
                let mu0 = self.params.mu0;
                match stats::one_sample(&v, mu0, self.params.conf) {
                    Some(r) => (
                        format!("{} / 单样本 t 检验", t.name),
                        vec!["单样本 t".to_string()],
                        vec![
                            vec!["样本量".to_string(), r.n.to_string()],
                            vec!["均值".to_string(), Self::num_or_dash(r.mean)],
                            vec!["标准差".to_string(), Self::num_or_dash(r.sd)],
                            vec!["标准误".to_string(), Self::num_or_dash(r.sem)],
                            vec!["假设均值 μ0".to_string(), Self::num_or_dash(mu0)],
                            vec!["t 值".to_string(), Self::num_or_dash(r.t)],
                            vec!["自由度".to_string(), Self::num_or_dash(r.df)],
                            vec!["P 值".to_string(), stats::fmt_p(r.p)],
                            vec![
                                "均值 95% CI".to_string(),
                                if r.ci_lo.is_finite() && r.ci_hi.is_finite() {
                                    format!("{:.4} 至 {:.4}", r.ci_lo, r.ci_hi)
                                } else {
                                    "—".to_string()
                                },
                            ],
                        ],
                    ),
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                }
            }
            // ── 31 配对 t 检验（本质是差值的单样本检验，μ0 固定 0）────────
            31 => {
                if cols.len() < 2 {
                    self.alert(i18n::t(self.lang, K::NeedTwoGroups).to_string());
                    return;
                }
                match stats::paired_t(&cols[0], &cols[1], self.params.conf) {
                    Some(r) => (
                        format!("{} / 配对 t 检验", t.name),
                        vec!["配对 t".to_string()],
                        vec![
                            vec!["配对数".to_string(), r.n.to_string()],
                            vec![
                                format!("{} 组均值", names.get(0).cloned().unwrap_or_else(|| "A 组".into())),
                                Self::num_or_dash(r.mean_a),
                            ],
                            vec![
                                format!("{} 组均值", names.get(1).cloned().unwrap_or_else(|| "B 组".into())),
                                Self::num_or_dash(r.mean_b),
                            ],
                            vec!["均值差".to_string(), Self::num_or_dash(r.mean_diff)],
                            vec!["差值标准差".to_string(), Self::num_or_dash(r.sd_diff)],
                            vec!["差值标准误".to_string(), Self::num_or_dash(r.sem_diff)],
                            vec!["t 值".to_string(), Self::num_or_dash(r.t)],
                            vec!["自由度".to_string(), Self::num_or_dash(r.df)],
                            vec!["P 值".to_string(), stats::fmt_p(r.p)],
                            vec![
                                "差值 95% CI".to_string(),
                                if r.ci_lo.is_finite() && r.ci_hi.is_finite() {
                                    format!("{:.4} 至 {:.4}", r.ci_lo, r.ci_hi)
                                } else {
                                    "—".to_string()
                                },
                            ],
                        ],
                    ),
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                }
            }
            // ── 32 Deming 回归（X、Y 均有误差）──────────────────────────
            32 => {
                if cols.len() < 2 {
                    self.alert(i18n::t(self.lang, K::NeedTwoGroups).to_string());
                    return;
                }
                let lam = self.params.deming_ratio;
                match stats::deming(&cols[0], &cols[1], lam) {
                    Some(r) => (
                        format!("{} / Deming 回归", t.name),
                        vec!["Deming".to_string()],
                        vec![
                            vec!["斜率".to_string(), Self::num_or_dash(r.slope)],
                            vec!["截距".to_string(), Self::num_or_dash(r.intercept)],
                            vec!["样本量".to_string(), r.n.to_string()],
                            vec!["λ（方差比）".to_string(), Self::num_or_dash(lam.max(1e-9))],
                        ],
                    ),
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                }
            }
            // ── 33 比较两条回归直线的斜率（ANCOVA 式合并误差）────────────
            33 => {
                // 需要两个数据集，各自内含 X / Y 两列：cols 为 [X1,Y1,X2,Y2]
                if cols.len() < 4 {
                    self.alert(i18n::t(self.lang, K::NeedTwoGroups).to_string());
                    return;
                }
                match stats::compare_slopes(&cols[0], &cols[1], &cols[2], &cols[3]) {
                    Some(r) => (
                        format!("{} / 回归斜率比较", t.name),
                        vec!["斜率比较".to_string()],
                        vec![
                            vec!["斜率 1".to_string(), Self::num_or_dash(r.slope1)],
                            vec!["斜率 2".to_string(), Self::num_or_dash(r.slope2)],
                            vec!["差值".to_string(), Self::num_or_dash(r.diff)],
                            vec!["差值标准误".to_string(), Self::num_or_dash(r.se)],
                            vec!["t 值".to_string(), Self::num_or_dash(r.t)],
                            vec!["自由度".to_string(), Self::num_or_dash(r.df)],
                            vec!["P 值".to_string(), stats::fmt_p(r.p)],
                            vec!["共同斜率".to_string(), Self::num_or_dash(r.common_slope)],
                            vec!["F 值（截距相等）".to_string(), Self::num_or_dash(r.f_intercept)],
                            vec!["P 值（截距相等）".to_string(), stats::fmt_p(r.p_equal_intercept)],
                        ],
                    ),
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                }
            }
            // ── 34 Log-rank（Mantel-Cox）────────────────────────────────
            34 => {
                // 第 0 列是时间，其余每列是一组的事件指示（非 0 视为事件）
                if cols.len() < 3 {
                    self.alert(i18n::t(self.lang, K::NeedTwoGroups).to_string());
                    return;
                }
                let time = cols[0].clone();
                let mut groups: Vec<(Vec<f64>, Vec<f64>)> = Vec::new();
                for gi in 1..cols.len() {
                    let ev = cols[gi].clone();
                    let len = time.len().min(ev.len());
                    groups.push((time[..len].to_vec(), ev[..len].to_vec()));
                }
                match stats::log_rank(&groups) {
                    Some(r) => {
                        let mut rows = vec![vec![
                            "卡方".to_string(),
                            Self::num_or_dash(r.chi2),
                        ]];
                        rows.push(vec!["自由度".to_string(), Self::num_or_dash(r.df)]);
                        rows.push(vec!["P 值".to_string(), stats::fmt_p(r.p)]);
                        for (i, h) in r.hr.iter().enumerate() {
                            rows.push(vec![
                                format!("风险比 HR（{}）", names.get(i + 1).cloned().unwrap_or_else(|| format!("G{}", i + 1))),
                                Self::num_or_dash(*h),
                            ]);
                        }
                        (
                            format!("{} / Log-rank 检验", t.name),
                            vec!["Log-rank".to_string()],
                            rows,
                        )
                    }
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                }
            }
            // ── 10 ROC 曲线（两组分数 → 诊断效能）────────────────────────
            //
            // 数据模型没有独立的「二分类真值标签列」，这里沿用诊断试验的
            // 标准简化口径：把两组分数分布当作阳性组与阴性组。
            // 统计上完备 —— AUC 恰等于 Mann-Whitney U / (n₁·n₂)。
            10 => {
                if cols.len() < 2 {
                    self.alert(i18n::t(self.lang, K::NeedTwoGroups).to_string());
                    return;
                }
                match stats::roc_curve(&cols[0], &cols[1]) {
                    Some(r) => {
                        let mut rows = vec![
                            vec!["阳性例数".to_string(), r.n_pos.to_string()],
                            vec!["阴性例数".to_string(), r.n_neg.to_string()],
                            vec!["AUC".to_string(), Self::num_or_dash(r.auc)],
                            vec!["标准误".to_string(), Self::num_or_dash(r.se)],
                            vec![
                                "95% CI".to_string(),
                                if r.ci_lo.is_finite() && r.ci_hi.is_finite() {
                                    format!("{:.4} 至 {:.4}", r.ci_lo, r.ci_hi)
                                } else {
                                    "—".to_string()
                                },
                            ],
                            vec![
                                "P 值（对比 AUC = 0.5）".to_string(),
                                if r.se > 0.0 && r.se.is_finite() {
                                    let z = (r.auc - 0.5) / r.se;
                                    stats::fmt_p(2.0 * (1.0 - stats::norm_cdf(z.abs())))
                                } else {
                                    "—".to_string()
                                },
                            ],
                            vec!["最佳阈值（Youden）".to_string(), Self::num_or_dash(r.best_threshold)],
                            vec!["切点灵敏度".to_string(), Self::num_or_dash(r.best_sens)],
                            vec!["切点特异度".to_string(), Self::num_or_dash(r.best_spec)],
                            vec!["ROC 曲线点".to_string(), r.points.len().to_string()],
                        ];
                        // 曲线本身铺进结果表，用户能直接看到坐标
                        for (i, p) in r.points.iter().enumerate() {
                            if i > 0 && i < r.points.len() - 1 && i % 4 != 0 && i < r.points.len() - 2 {
                                continue; // 点太多时抽样，避免结果表爆炸
                            }
                            rows.push(vec![
                                format!("FPR {:.3} / TPR {:.3}", p.fpr, p.tpr),
                                Self::num_or_dash(p.threshold),
                            ]);
                        }
                        (
                            format!("{} / ROC 曲线", t.name),
                            vec!["ROC".to_string()],
                            rows,
                        )
                    }
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                }
            }
            // ── 15 Cox 比例风险回归（Breslow 并列 + Newton-Raphson）──────
            15 => {
                // 第 0 列是随访时间；若列数 ≥3，最后一组列作为事件指示，
                // 其余为协变量。列数不足时按「全部发生事件」处理并如实标注。
                if cols.len() < 2 {
                    self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                    return;
                }
                let time = cols[0].clone();
                let (event, cov_start, censor_note) = if cols.len() >= 3 {
                    (cols[cols.len() - 1].clone(), 1, false)
                } else {
                    (vec![1.0; time.len()], 1, true)
                };
                let mut cov: Vec<(String, Vec<f64>)> = Vec::new();
                for ci in cov_start..cols.len().saturating_sub(if cols.len() >= 3 { 1 } else { 0 }) {
                    cov.push((names.get(ci).cloned().unwrap_or_else(|| format!("V{}", ci)), cols[ci].clone()));
                }
                if cov.is_empty() {
                    self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                    return;
                }
                match stats::cox_ph(&time, &event, &cov) {
                    Some(r) => {
                        let mut rows = vec![
                            vec!["样本量".to_string(), r.n.to_string()],
                            vec!["事件数".to_string(), r.n_events.to_string()],
                            vec!["对数偏似然".to_string(), Self::num_or_dash(r.loglik)],
                            vec![
                                "是否收敛".to_string(),
                                if r.converged { "是".to_string() } else { "否（达到最大迭代次数）".to_string() },
                            ],
                        ];
                        if censor_note {
                            rows.push(vec![
                                "说明".to_string(),
                                "缺少删失列：所有观测均按事件处理".to_string(),
                            ]);
                        }
                        for v in &r.vars {
                            rows.push(vec![format!("β（{}）", v.name), Self::num_or_dash(v.beta)]);
                            rows.push(vec![format!("风险比 HR（{}）", v.name), Self::num_or_dash(v.hr)]);
                            rows.push(vec![format!("标准误（{}）", v.name), Self::num_or_dash(v.se)]);
                            rows.push(vec![
                                format!("HR 95% CI（{}）", v.name),
                                if v.ci_lo.is_finite() && v.ci_hi.is_finite() {
                                    format!("{:.4} 至 {:.4}", v.ci_lo, v.ci_hi)
                                } else {
                                    "—".to_string()
                                },
                            ]);
                            rows.push(vec![format!("z 值（{}）", v.name), Self::num_or_dash(v.z)]);
                            rows.push(vec![format!("P 值（{}）", v.name), stats::fmt_p(v.p)]);
                        }
                        (
                            format!("{} / Cox 回归", t.name),
                            vec!["Cox".to_string()],
                            rows,
                        )
                    }
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                }
            }
            // ── 18 主成分分析（相关矩阵 + Jacobi 特征分解）───────────────
            18 => {
                if cols.len() < 2 {
                    self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                    return;
                }
                match stats::pca(&cols, true) {
                    Some(r) => {
                        let mut rows = vec![
                            vec!["观测数".to_string(), r.n.to_string()],
                            vec!["变量数".to_string(), r.n_vars.to_string()],
                            vec!["分解基".to_string(), "相关矩阵（z 分数标准化）".to_string()],
                        ];
                        for (i, e) in r.eigen.iter().enumerate() {
                            rows.push(vec![format!("PC{} 特征值", i + 1), Self::num_or_dash(*e)]);
                            rows.push(vec![
                                format!("PC{} 方差比例", i + 1),
                                format!("{:.2}%", r.proportion[i] * 100.0),
                            ]);
                            rows.push(vec![
                                format!("PC{} 累积比例", i + 1),
                                format!("{:.2}%", r.cumulative[i] * 100.0),
                            ]);
                        }
                        // 载荷矩阵：变量 × 主成分
                        for (v, ld) in r.loadings.iter().enumerate() {
                            let nm = names.get(v).cloned().unwrap_or_else(|| format!("V{}", v + 1));
                            let s: Vec<String> = ld
                                .iter()
                                .take(3)
                                .map(|x| format!("{:.4}", x))
                                .collect();
                            rows.push(vec![format!("载荷 {}", nm), s.join(", ")]);
                        }
                        rows.push(vec![
                            "PC1 + PC2 得分".to_string(),
                            format!("{} 个点（见图）", r.scores[0].len()),
                        ]);
                        (
                            format!("{} / 主成分分析", t.name),
                            vec!["主成分分析".to_string()],
                            rows,
                        )
                    }
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                }
            }
            // ── 19 层次聚类（平均连接，z-score 标准化）────────────────────
            19 => {
                if cols.is_empty() {
                    self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                    return;
                }
                let k = self.params.clusters.max(1);
                match stats::hierarchical(&cols, 0, k) {
                    Some(r) => {
                        let mut rows = vec![
                            vec!["观测数".to_string(), r.n.to_string()],
                            vec!["变量数".to_string(), cols.len().to_string()],
                            vec!["连接方法".to_string(), "平均连接".to_string()],
                            vec!["切分簇数".to_string(), k.to_string()],
                            vec!["合并次数".to_string(), r.merges.len().to_string()],
                        ];
                        let mut sizes = std::collections::BTreeMap::new();
                        for &c in &r.cluster {
                            *sizes.entry(c).or_insert(0usize) += 1;
                        }
                        for (c, s) in sizes {
                            rows.push(vec![format!("簇 {} 大小", c + 1), s.to_string()]);
                        }
                        rows.push(vec![
                            "归属".to_string(),
                            r.cluster.iter().map(|c| (c + 1).to_string()).collect::<Vec<_>>().join(" "),
                        ]);
                        (
                            format!("{} / 层次聚类", t.name),
                            vec!["层次聚类".to_string()],
                            rows,
                        )
                    }
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                }
            }
            // ── 20 K-means 聚类（k-means++ 初始化 + Lloyd 迭代）──────────
            20 => {
                if cols.is_empty() {
                    self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                    return;
                }
                let k = self.params.clusters.max(2);
                let seed = self.seed;
                match stats::kmeans(&cols, k, seed) {
                    Some(r) => {
                        let mut rows = vec![
                            vec!["簇数（k）".to_string(), k.to_string()],
                            vec!["迭代次数".to_string(), r.iterations.to_string()],
                            vec![
                                "是否收敛".to_string(),
                                if r.converged { "是".to_string() } else { "否（达到最大迭代次数）".to_string() },
                            ],
                            vec!["总平方和".to_string(), Self::num_or_dash(r.tss)],
                            vec!["簇内平方和".to_string(), Self::num_or_dash(r.wss)],
                            vec![
                                "方差解释率".to_string(),
                                if r.tss > 0.0 {
                                    format!("{:.2}%", (1.0 - r.wss / r.tss) * 100.0)
                                } else {
                                    "—".to_string()
                                },
                            ],
                        ];
                        for (i, s) in r.sizes.iter().enumerate() {
                            rows.push(vec![format!("簇 {} 大小", i + 1), s.to_string()]);
                        }
                        rows.push(vec![
                            "归属".to_string(),
                            r.cluster.iter().map(|c| (c + 1).to_string()).collect::<Vec<_>>().join(" "),
                        ]);
                        (
                            format!("{} / K-means 聚类", t.name),
                            vec!["K-means".to_string()],
                            rows,
                        )
                    }
                    None => {
                        self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                        return;
                    }
                }
            }
            // ── 其余：明确告知未实现，不给假数字 ──
            other => {
                self.alert(format!(
                    "{} (id {})",
                    i18n::t(self.lang, K::AnalysisNotImplemented),
                    other
                ));
                return;
            }
        };

        // 结果表**存原始英文**，渲染时再翻译。
        // 这样在设置里切换语言后，已经生成的结果也会立即跟着变，
        // 而不是永远停在生成时的语言。
        //
        // 升级成多段式报告：结果摘要 / 分析表 / 描述统计 / 前提检验 / 多重比较。
        // 只有真正写了构建器的分析会被替换，其余保持原样（不伪造内容）。
        let params = self.params.clone();
        let rows = crate::report::enrich(method, &names, &cols, rows, &params);
        self.push_undo();
        // **同一个表 / 同一个分析只保留最新一份结果**：同名结果整张替换。
        // 反复跑同一个分析不该堆出一串名字一模一样的兄弟表。
        let existing = self.project.results.iter().position(|r| r.name == title);
        let n = match existing {
            Some(i) => i + 1,
            None => self.project.results.len() + 1,
        };
        let sheet = ResultSheet {
            name: title,
            tabs,
            active_tab: 0,
            rows,
            source_table: Some(ti),
        };
        match existing {
            Some(i) => self.project.results[i] = sheet,
            None => self.project.results.push(sheet),
        }
        self.open_folders[2] = true;
        self.set_active(Active::Result(n - 1));
        // 状态栏带上方法 id、组数、结果行数：结果页万一为空，一眼就能判断
        // 是「没算出数据」还是「算出来了但没画出来」。
        self.last_analysis_ms = t0.elapsed().as_millis();
        self.status = format!(
            "{}（id {} · {} 组 · {} 行 · {} ms）",
            i18n::t(self.lang, K::AnalysisDone),
            method,
            cols.len(),
            self.project.results[n - 1].rows.len(),
            self.last_analysis_ms
        );
    }

    /// 底部状态栏：例行反馈（导出路径、缩放比例、当前行列…）。
    fn note(&mut self, msg: String) {
        self.status = msg;
    }

    /// 窗口居中弹窗：**重要**信息（出错、无法执行、需要用户知道的结果）。
    /// 与「设置 / 关于 / 欢迎」共用同一个弹窗位，所以互斥 —— 点新的会顶掉旧的。
    fn alert(&mut self, msg: String) {
        if self.alert.is_none() {
            self.dlg_open_at = std::time::Instant::now();
        }
        self.alert = Some(msg);
    }

    /// 提交表头改名。回车、以及**点到别处**都会走到这里 ——
    /// 用户要求「双击改完，鼠标点其他地方也要自动保存」。
    fn commit_rename(&mut self) {
        // 导航器改名优先：它与表头改名共用一套 draft，但目标是表单名。
        if self.sheet_renaming.is_some() {
            self.commit_sheet_rename();
            return;
        }
        let ti = self.table_index();
        if let Some(target) = self.renaming.take() {
            let name = self.rename_draft.trim().to_string();
            if !name.is_empty() {
                self.push_undo();
                if let Some(t) = self.project.tables.get_mut(ti) {
                    match target {
                        RenameTarget::DataSet(di) => {
                            if let Some(ds) = t.table.data_sets.get_mut(di) {
                                ds.name = name;
                            }
                        }
                        RenameTarget::Sub(di, si) => {
                            if let Some(ds) = t.table.data_sets.get_mut(di) {
                                if let Some(sc) = ds.sub_columns.get_mut(si) {
                                    sc.name = name;
                                }
                            }
                        }
                    }
                }
                self.after_project_change();
            }
        }
        self.rename_draft.clear();
    }

    /// 数值格式化：NaN 显示「—」，其余 4 位小数。
    fn num_or_dash(v: f64) -> String {
        if v.is_finite() {
            format!("{:.4}", v)
        } else {
            "—".to_string()
        }
    }

    /// 把当前 draft 写进稀疏层。**每次输入都实时写入**，所以内容不会丢。
    fn write_draft(&mut self) {
        if self.cursor.0 == usize::MAX {
            // 行标题列
            let i = self.table_index();
            self.project.tables[i]
                .table
                .ensure_row_title(self.cursor.1, self.draft.clone());
            return;
        }
        let i = self.table_index();
        let (c, r) = self.cursor;
        self.project.tables[i].write_str(c, r, &self.draft);
        self.ensure_row(r);
    }

    /// 提交：写回 + 重建快照 + 重绘图表。
    fn commit(&mut self) {
        if !self.editing {
            return;
        }
        self.write_draft();
        self.refresh_snap();
        self.mark_chart_dirty();
    }

    /// 把光标移到 (c, r) 并进入输入态，draft 预填该格当前值。
    fn move_cursor(&mut self, c: usize, r: usize) {
        self.commit();
        self.ensure_row(r);
        self.ensure_col(c);
        // 移动光标 = 单选：把框选**塌陷**到新光标，而不是置空。
        // 置空会连带丢掉「选区左上角」这个粘贴落点；
        // 塌陷成单格后 sel_range() 仍返回一个有效（1×1）区间，语义更稳。
        self.sel_from = Some((c, r));
        self.sel_to = Some((c, r));
        self.pending_clear_sel = false;
        self.cursor = (c, r);
        self.editing = true;
        self.draft = if c == usize::MAX {
            self.project.tables[self.table_index()]
                .table
                .row_title(r)
                .to_string()
        } else {
            self.project.tables[self.table_index()].display_at(c, r)
        };
        // 焦点延迟到下一帧开头（见 update() 顶部「坑 2」）：
        // 这里只置标记，真正申请焦点在下一帧。
        self.pending_focus = true;
        self.focus_ready = false;
        self.refresh_snap();
        self.update_status();
    }

    /// 行窗口按需增长：只增长一个整数，不预分配任何行。
    fn ensure_row(&mut self, r: usize) {
        // 已打开的工作表行数上限 100 万：稀疏层本身无上限，
        // 这里只是防止虚拟列表的行数被 GoTo 推到 usize 边界导致 r + 64 溢出。
        const MAX_VIEW_ROWS: usize = 1_000_000;
        let want = r.saturating_add(64).min(MAX_VIEW_ROWS);
        if want > self.rows_hint {
            self.rows_hint = want;
        }
    }

    /// 列窗口按需增长：与行同理，只增长一个整数。
    /// 往右越过已有列时自动多渲染一批，所以「走到 Z 之后还有 AA、AB」。
    fn ensure_col(&mut self, c: usize) {
        if c == usize::MAX {
            return; // 行标题列
        }
        const MAX_VIEW_COLS: usize = 1_000_000;
        let want = c.saturating_add(16).min(MAX_VIEW_COLS);
        if want > self.cols_hint {
            self.cols_hint = want;
        }
    }

    fn phys_cols(&self) -> usize {
        match self.project.tables.get(self.table_index()) {
            Some(t) => t.phys_cols(),
            None => 0,
        }
    }

    /// 打开右键菜单：先把光标移到目标格，再记录弹出位置。
    ///
    /// 光标先落过去是有意的 —— 菜单里的「插入行 / 删除行」都作用于光标位置，
    /// 若不同步，用户右键第 8 行却删了第 3 行。
    fn open_ctx_menu(&mut self, p: charis_ui::geometry::Point, c: usize, r: usize, scope: CtxScope) {
        self.cursor = (c, r);
        self.editing = false;
        self.ctx_menu = Some(CtxMenuState {
            x: p.x,
            y: p.y,
            col: c,
            row: r,
            scope,
            frozen: matches!(scope, CtxScope::Sheet) && self.is_frozen(c, r),
            opened_at: std::time::Instant::now(),
        });
    }

    /// 执行右键菜单项。每一项都对应一个**已实现**的动作。
    fn run_ctx_action(&mut self, a: CtxAction, tgt: (usize, usize)) {
        match a {
            CtxAction::Copy => self.copy_cell(),
            CtxAction::Cut => self.cut_cell(),
            CtxAction::Paste => self.paste_cell(),
            CtxAction::Clear => self.clear_cell(),
            CtxAction::SelectAll => self.select_all(),
            CtxAction::SelectCol => {
                let c = self.cursor.0;
                self.sel_from = Some((c, 0));
                self.sel_to = Some((c, self.rows_hint.saturating_sub(1)));
            }
            CtxAction::SelectRow => {
                let r = self.cursor.1;
                self.sel_from = Some((0, r));
                self.sel_to = Some((self.phys_cols().saturating_sub(1), r));
            }
            CtxAction::InsertRow => self.ctx_insert_row(),
            CtxAction::DeleteRow => self.ctx_delete_row(),
            CtxAction::AddDataSet => self.ctx_add_data_set(),
            CtxAction::RemoveDataSet => self.ctx_remove_data_set(),
            CtxAction::RenameCol => {
                let ti = self.table_index();
                if let Some(t) = self.project.tables.get(ti) {
                    let (d, _) = t.table.phys_to_logic(self.cursor.0);
                    if d < t.table.data_sets.len() {
                        self.renaming = Some(RenameTarget::DataSet(d));
                        self.rename_draft =
                            t.dataset_names().get(d).cloned().unwrap_or_default();
                    }
                }
            }
            // 行标题没有独立的 RenameTarget 变体：它的改名走的是
            // 「把光标移到行标题列并进入编辑」这条路（与双击行标题一致）。
            CtxAction::RenameRow => {
                let r = self.cursor.1;
                self.sel_drag = false;
                self.hover_from = None;
                self.move_cursor(usize::MAX, r);
            }
            // ── 导航器条目 ──────────────────────────────────────────────
            CtxAction::OpenSheet => {
                let (f, i) = tgt;
                self.open_sheet_at(f, i);
            }
            CtxAction::RenameSheet => {
                let (f, i) = tgt;
                self.start_sheet_rename(f, i);
            }
            CtxAction::DuplicateSheet => {
                let (f, i) = tgt;
                self.duplicate_sheet_at(f, i);
            }
            CtxAction::DeleteSheet => {
                let (f, i) = tgt;
                self.delete_sheet_at(f, i);
            }
            CtxAction::ToggleFreezeSheet => {
                let (f, i) = tgt;
                self.toggle_freeze_at(f, i);
            }
            CtxAction::ExportGraphSheet => {
                let (f, i) = tgt;
                if f != 3 {
                    self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                } else {
                    self.open_sheet_at(f, i);
                    self.export_open = true;
                    self.dlg_open_at = std::time::Instant::now();
                }
            }
        }
    }

    /// 右键 → 插入行（在当前光标行之前插入一行）
    fn ctx_insert_row(&mut self) {
        let ti = self.table_index();
        if ti >= self.project.tables.len() {
            return;
        }
        let at = self.cursor.1;
        self.push_undo();
        {
            let t = &mut self.project.tables[ti];
            t.table.insert_row(at);
            t.view_rows = t.view_rows.saturating_add(1);
        }
        self.after_project_change();
        self.status = i18n::t(self.lang, K::InsertRow).to_string();
    }

    /// 右键 → 删除当前行
    fn ctx_delete_row(&mut self) {
        let ti = self.table_index();
        if ti >= self.project.tables.len() {
            return;
        }
        let at = self.cursor.1;
        self.push_undo();
        {
            let t = &mut self.project.tables[ti];
            t.table.delete_row(at);
        }
        self.after_project_change();
        self.status = i18n::t(self.lang, K::DeleteRow).to_string();
    }

    /// 右键 → 新增一个数据集（列组）
    fn ctx_add_data_set(&mut self) {
        let ti = self.table_index();
        if ti >= self.project.tables.len() {
            return;
        }
        self.push_undo();
        {
            let t = &mut self.project.tables[ti];
            let n = t.table.data_sets.len();
            t.table.data_sets.push(crate::model::DataSetMeta {
                name: t.table.dataset_letter(n),
                sub_columns: vec![],
            });
        }
        self.after_project_change();
        self.status = i18n::t(self.lang, K::AddDataSet).to_string();
    }

    /// 右键 → 删除最后一个数据集
    fn ctx_remove_data_set(&mut self) {
        let ti = self.table_index();
        if ti >= self.project.tables.len() {
            return;
        }
        let n = self.project.tables[ti].table.data_sets.len();
        if n <= 1 {
            self.status = i18n::t(self.lang, K::NotApplicable).to_string();
            return;
        }
        self.push_undo();
        {
            let t = &mut self.project.tables[ti];
            t.table.data_sets.pop();
        }
        self.after_project_change();
        self.status = i18n::t(self.lang, K::RemoveLastDataSet).to_string();
    }

    /// 状态栏：与析光一致的「Row 3, A: Control (Y1)」
    fn update_status(&mut self) {
        let t = &self.project.tables[self.table_index()];
        let (c, r) = self.cursor;
        if c == usize::MAX {
            self.status = t.table.row_title(r).to_string();
        } else {
            let (letter, title, sub) = t.column_label(c);
            self.status = match self.lang {
                Lang::Zh => format!("第 {} 行, {}: {} ({})", r.saturating_add(1), letter, title, sub),
                Lang::En => format!("Row {}, {}: {} ({})", r.saturating_add(1), letter, title, sub),
            };
        }
    }

    // ── 视图 ────────────────────────────────────────────────────────────────
    fn workspace(&self) -> Element<Msg> {
        // 同样要能被压回窗口宽度，否则宽表格会把整行撑破（见 view() 里的说明）。
        let inner = match self.active {
            Active::Data(i) => ui::grid::data_grid(
                self.lang,
                &self.project.tables[i],
                &self.snap,
                self.rows_hint,
                self.cols_hint,
                self.sel_range(),
                self.cursor,
                self.editing,
                &self.draft,
                self.hover,
                self.renaming,
                &self.rename_draft,
                // 焦点延迟到帧初：进入编辑的第一帧不抢，下一帧才 autofocus
                self.focus_ready,
            )
            .flex1(),
            Active::Result(i) => {
                let r = &self.project.results[i];
                let hit = matches!(&self.result_view_data,
                    Some((ci, cl, _, _, _)) if *ci == i && *cl == self.lang);
                if hit {
                    // 命中缓存：直接渲染预翻译好的内容，本帧不再逐格查表。
                    let (_, _, rows, tabs, name) = self.result_view_data.as_ref().unwrap();
                    result_view(self.lang, rows.as_slice(), tabs.as_slice(), name, r.active_tab)
                        .flex1()
                } else {
                    // 未命中（切了语言但还没换页）：降级为实时翻译，保证内容正确。
                    let rows = i18n::translate_rows(self.lang, &r.rows);
                    let tabs: Vec<String> =
                        r.tabs.iter().map(|x| i18n::stat_term(self.lang, x)).collect();
                    let name = i18n::translate_title(self.lang, &r.name);
                    result_view(self.lang, &rows, &tabs, &name, r.active_tab).flex1()
                }
            }
            Active::Graph(_) => self.graph_view().flex1(),
            Active::Info(_) => placeholder(i18n::t(self.lang, K::InfoSheet)).flex1(),
            // 布局模块已移除：导航器里不再有该文件夹，此分支仅为穷尽性兜底
            Active::Layout(_) => placeholder(i18n::t(self.lang, K::InfoSheet)).flex1(),
        };
        // 套一层 min_w(0)：让 flex 子项可以被压回窗口宽度，
        // 否则宽表格会撑破整行，覆盖层居中就会偏到右边栏之外。
        div().flex1().min_w(0.0).child(inner)
    }

    fn graph_view(&self) -> Element<Msg> {
        // 图表画布做成「浮起卡片」：白底 + 极浅边框 + 圆角 + 浅投影。
        // 冷灰窗口底 + 白色卡片的层次，是现代科研软件最常见的高级感来源。
        let mut area = div()
            .flex1()
            .bg(hex(canvas_bg()))
            .border(1.0, hex(hair()))
            // 外层卡片 r_md()，内部若再嵌套圆角元素就用 r_sm() —— 同心
            .rounded(r_md())
            .shadow(ui::shadow_card())
            .center()
            .scroll_both();
        if let Some(img) = self.chart.clone() {
            // 关键：显式给出显示尺寸，让位图缩回逻辑大小。
            // 少了这两行，超采样后的大位图会按原始像素显示，只能看到一角。
            area = area
                .child(image(ImageSource::from(img)).w(self.chart_w).h(self.chart_h));
        }

        col()
            .size_full()
            .bg(hex(window_bg()))
            .child(
                // 图表页顶部的表选项卡
                row()
                    .bg(hex(window_bg()))
                    .border_b(1.0, hex(hair()))
                    .child(
                        div()
                            .px(10.0)
                            .h(row_h() + 6.0)
                            .bg(hex(canvas_bg()))
                            .border_b(2.0, hex(accent()))
                            .child(
                                row()
                                    .items_center()
                                    .child(
                                        text(self.sheet_label())
                                            .font_size(ui_font())
                                            .color(hex(ink())),
                                    ),
                            ),
                    ),
            )
            .child(div().flex1().p(14.0).child(area))
    }
}

// ── 结果表 ──────────────────────────────────────────────────────────────────
fn result_view(
    lang: Lang,
    rows: &[Vec<String>],
    tabs: &[String],
    name: &str,
    active_tab: usize,
) -> Element<Msg> {
    let mut tabs_bar = row().gap(0.0).bg(hex(window_bg())).border_b(1.0, hex(hair()));
    for (i, tname) in tabs.iter().enumerate() {
        tabs_bar = tabs_bar.child(
            div()
                .px(10.0)
                .h(row_h() + 6.0)
                .bg(if i == active_tab { hex(canvas_bg()) } else { hex(window_bg()) })
                .when(i == active_tab, |e| e.border_b(2.0, hex(accent())))
                .child(row().items_center().child(
                    text(tname.clone()).font_size(ui_font()).color(hex(ink())),
                )),
        );
    }

    // ── 按区块渲染 ────────────────────────────────────────────────────
    // `report::flatten` 用两个前缀标记结构：首格 `# ` 是区块标题、
    // `## ` 是表头。这里按标记把扁平行切成若干段，每段独立算列宽，
    // 因此不同列数的表（摘要两列、描述统计十几列）能各自铺满。
    let mut body = col().gap(0.0).w_full();
    let mut seg_title: Option<String> = None;
    let mut seg_head: Option<Vec<String>> = None;
    let mut seg_rows: Vec<Vec<String>> = Vec::new();
    let mut seg_notes: Vec<String> = Vec::new();
    // 统计实际渲染出来的行数。为 0 时说明「没有可画的内容」，
    // 这时给出明确提示，而不是留一片空白让人分不清是没算出来还是没画出来。
    let mut rendered = 0usize;

    for rline in rows.iter() {
        let first = rline.first().map(|x| x.as_str()).unwrap_or("");
        if first.starts_with("# ") {
            body = flush_seg(lang, &mut seg_title, &mut seg_head, &mut seg_rows, &mut seg_notes, body);
            seg_title = Some(first[2..].to_string());
            seg_head = None;
            rendered += 1;
        } else if first.starts_with("## ") {
            let mut h = rline.clone();
            h[0] = first[3..].to_string();
            seg_head = Some(h);
        } else if first.starts_with("#> ") {
            // 脚注：不占数据行，单独渲染成弱化小字
            seg_notes.push(first[3..].to_string());
        } else if rline.is_empty() {
            // 空行只作分隔，不渲染
        } else {
            seg_rows.push(rline.clone());
            rendered += 1;
        }
    }
    body = flush_seg(lang, &mut seg_title, &mut seg_head, &mut seg_rows, &mut seg_notes, body);

    if rendered == 0 {
        body = body.child(
            div()
                .w_full()
                .px(10.0)
                .pt(20.0)
                .child(
                    text(if rows.is_empty() {
                        "本次分析没有产出任何结果行。".to_string()
                    } else {
                        format!("结果行 {} 条，但都不可渲染。", rows.len())
                    })
                    .font_size(ui_font())
                    .color(hex(dim())),
                ),
        );
    }

    // 横向滚动范围：charis 只看滚动容器的**直接子节点**的
    // `location + size`，所以内容必须自己声明一个比视口更宽的最小宽度
    // （和 grid 用 `min_w(total)` 让表格能横向滚动是同一个做法）。
    // 取所有数据行里最宽的一行：列宽算法是 `disp_w + 20` 并夹在 64–260，
    // 这里照抄一遍，得到的就是内容真实需要的宽度。
    // 跳过 `# ` / `## ` 标记行——它们的首格是区块标题，会把宽度算爆。
    let mut content_w = 0.0f32;
    for rline in rows.iter() {
        let first = rline.first().map(|x| x.as_str()).unwrap_or("");
        if first.starts_with("# ")
            || first.starts_with("## ")
            || first.starts_with("#> ")
            || rline.is_empty()
        {
            continue;
        }
        let w: f32 = rline
            .iter()
            .map(|c| (disp_w(c) + 20.0).clamp(64.0, 260.0))
            .sum::<f32>()
            + 24.0;
        content_w = content_w.max(w);
    }

    col()
        .w_full()
        .flex1()
        .min_h(0.0)
        .min_w(0.0)
        .bg(hex(window_bg()))
        .child(tabs_bar)
        .child(
            col()
                .gap(2.0)
                .px(10.0)
                .pt(6.0)
                // 这一层必须有确定高度，否则下面的滚动区拿到 0：
                // flex1 的 flex-basis 是 0，它只能分配「剩余空间」；
                // 父容器高度若为 auto（由内容决定），剩余空间恒为 0，
                // 于是滚动区高度 = 0，表格全部被裁掉 ——
                // 表现正是「只看到表名，下面一片空白」。
                // flex1 + min_h(0) 让它在确定高度的父容器里吃满剩余空间。
                .flex1()
                .min_h(0.0)
                .min_w(0.0)
                .child(text(name).font_size(ui_font() + 2.0).semibold().color(hex(ink())))
                // 滚动容器同样必须是 **col()**（详见 settings.rs 里的说明）：
                // div() 默认是 row，唯一的子节点会被纵向拉伸到视口高度，
                // 于是 content_size.h == 视口高，纵向滚动范围恒为 0。
                // 用 scroll_both() 让横向也能滚（scroll_y 只开纵向）。
                .child(
                    col()
                        .flex1()
                        .min_h(0.0)
                        .min_w(0.0)
                        .scroll_both()
                        .child(body.w_full().min_w(content_w)),
                ),
        )
}

/// 输出一个已收集完的区块（标题 + 表头 + 数据行），并清空缓冲。
fn flush_seg(
    lang: Lang,
    title: &mut Option<String>,
    head: &mut Option<Vec<String>>,
    srows: &mut Vec<Vec<String>>,
    notes: &mut Vec<String>,
    body: Element<Msg>,
) -> Element<Msg> {
    if title.is_none() && srows.is_empty() && notes.is_empty() {
        return body;
    }
    let mut body = body;
    if let Some(t) = title.take() {
        body = body.child(
            div()
                .w_full()
                .px(10.0)
                .pt(10.0)
                .pb(4.0)
                .child(
                    row()
                        .items_center()
                        .gap(8.0)
                        .child(div().w(3.0).h(14.0).bg(hex(accent())))
                        .child(
                            text(i18n::stat_term(lang, &t))
                                .font_size(ui_font() + 1.0)
                                .semibold()
                                .color(hex(ink())),
                        ),
                ),
        );
    }
    if srows.is_empty() {
        return body;
    }
    let ncols = srows
        .iter()
        .map(|r| r.len())
        .max()
        .unwrap_or(1)
        .max(head.as_ref().map(|h| h.len()).unwrap_or(1));
    let widths: Vec<f32> = (0..ncols)
        .map(|c| {
            let mut mx = 40.0f32;
            if let Some(h) = head.as_ref() {
                if let Some(v) = h.get(c) {
                    mx = mx.max(disp_w(v));
                }
            }
            for r in srows.iter() {
                if let Some(v) = r.get(c) {
                    mx = mx.max(disp_w(v));
                }
            }
            (mx + 20.0).clamp(64.0, 260.0)
        })
        .collect();

    let tbl_is_empty = srows.is_empty() && head.is_none();
    let mut tbl = col().gap(0.0);
    if let Some(h) = head.take() {
        let mut line = row().gap(0.0).h(row_h());
        for c in 0..ncols {
            let v = h.get(c).map(|x| x.as_str()).unwrap_or("");
            line = line.child(cell(v, widths[c], true, false));
        }
        tbl = tbl.child(line);
    }
    for (ri, rline) in srows.iter().enumerate() {
        let mut line = row().gap(0.0).h(row_h());
        for c in 0..ncols {
            let v = rline.get(c).map(|x| x.as_str()).unwrap_or("");
            line = line.child(cell(v, widths[c], false, ri % 2 == 1));
        }
        tbl = tbl.child(line);
    }
    srows.clear();
    if !tbl_is_empty {
        body = body.child(div().px(10.0).pb(6.0).child(tbl.border(1.0, hex(grid_line()))));
    }
    // 脚注：弱化小字，紧跟本段表格。放在表格之后而不是之前，
    // 因为它是对上面那张表的说明（样本量口径、校正方法等）。
    for n in notes.drain(..) {
        body = body.child(
            div()
                .px(10.0)
                .pb(8.0)
                .child(text(n).font_size(ui_font() - 1.0).color(hex(dim()))),
        );
    }
    body
}

/// 结果表的一个单元格。
fn cell(v: &str, w: f32, is_head: bool, zebra: bool) -> Element<Msg> {
    div()
        .w(w)
        .h(row_h())
        .bg(if is_head {
            hex(header_bg())
        } else if zebra {
            hex(zebra_bg())
        } else {
            hex(canvas_bg())
        })
        .border_r(1.0, hex(grid_line()))
        .border_b(1.0, hex(grid_line()))
        .child(
            row()
                .px(6.0)
                .items_center()
                .child(
                    text(v.to_string())
                        .font_size(ui_font())
                        .color(if is_head { hex(ink()) } else { hex(dim()) }),
                ),
        )
}

/// 文本显示宽度估算（CJK 按双宽计），用于自适应列宽。
fn disp_w(s: &str) -> f32 {
    let mut w = 0.0f32;
    for c in s.chars() {
        w += if (c as u32) > 0x2000 { 13.0 } else { 7.0 };
    }
    w
}

/// 信息页 / 布局页的占位视图。
fn placeholder(title: &str) -> Element<Msg> {
    div()
        .size_full()
        .bg(hex(canvas_bg()))
        .center()
        .child(text(title).font_size(ui_font()).color(hex(dim())))
}

impl SpectraApp {

    /// 当前表里最后一个**有数据**的行号（没有数据就是 0）。
    /// Ctrl+Shift+↓ 用它把选区一次扩到数据边缘。
    fn last_data_row(&self) -> usize {
        let i = self.table_index();
        self.project
            .tables
            .get(i)
            .map(|t| t.table.filled_rows().into_iter().max().unwrap_or(0))
            .unwrap_or(0)
    }

    /// 当前表里最后一个**有数据**的物理列号。
    fn last_data_col(&self) -> usize {
        let i = self.table_index();
        let Some(t) = self.project.tables.get(i) else {
            return 0;
        };
        t.table
            .filled_rows()
            .into_iter()
            .flat_map(|r| t.table.row_cells(r).into_iter().map(|(c, _)| c))
            .max()
            .unwrap_or(0)
    }

    /// 第 gi 张图实际参与作图的各组**原始值**（空数据集已剔除，
    /// 与 chart_view 里的过滤口径一致，所以组号能对上）。
    fn graph_group_values(&self, gi: usize) -> Vec<Vec<f64>> {
        let Some(g) = self.project.graphs.get(gi) else {
            return Vec::new();
        };
        let Some(t) = self.project.tables.get(g.source_table) else {
            return Vec::new();
        };
        t.table
            .data_sets
            .iter()
            .enumerate()
            .map(|(i, _)| t.table.flat_values(i))
            .filter(|v| !v.is_empty())
            .collect()
    }

    /// 显著性标注的写法，由设置 `bracket_label_fmt` 决定。
    ///
    /// 只给星号的话，所有「极显著」的对都是 `****`、看不出谁更强；
    /// 带上 P 值才能一眼比较显著程度，所以默认是「星号 + P 值」。
    fn sig_label(stars: &str, p: Option<f64>) -> String {
        let fmt = crate::settings::get().bracket_label_fmt;
        match (fmt, p.map(stats::fmt_p)) {
            (0, _) => stars.to_string(),
            (_, None) => stars.to_string(),
            (1, Some(x)) => format!("P = {}", x),
            (_, Some(x)) => format!("{} P = {}", stars, x),
        }
    }

    /// 推荐某一组对的显著性标注，返回 (标注文本, 校正后 P 值)。
    ///
    /// 不是只看这一对，而是**枚举所有组之间**的比较并统一做多重比较校正，
    /// 再按校正后的 p 推荐 `*` / `**` / `***` / `ns`。
    /// 组数越多，单看某一对的 p 越容易假阳性，所以必须校正后才分级。
    fn recommend_pair(&self, gi: usize, a: usize, b: usize) -> (String, Option<f64>) {
        let vals = self.graph_group_values(gi);
        if a >= vals.len() || b >= vals.len() || a == b {
            return ("*".to_string(), None);
        }
        // 先试 Tukey（方差齐）；它不可用时换 Games-Howell（方差不齐），
        // 两者都不可用再退回 Welch 的裸 p —— 逐级降级，不硬套一个方法。
        let find = |m: stats::MultiMethod| {
            stats::pairwise_matrix(&vals, m, 0.95)
                .into_iter()
                .find(|(i, j, _, _, _)| (*i == a && *j == b) || (*i == b && *j == a))
                .map(|(_, _, _, adj, st)| (adj, st))
        };
        let got = find(stats::MultiMethod::Tukey).or_else(|| find(stats::MultiMethod::GamesHowell));
        match got {
            Some((adj, st)) => (Self::sig_label(st, Some(adj)), Some(adj)),
            None => {
                // Tukey 不可用时（样本量不足等）退回 Welch t 检验的裸 p
                match stats::t_test(&vals[a], &vals[b], stats::TTestKind::Welch) {
                    Some(r) => (Self::sig_label(stats::stars(r.p), Some(r.p)), Some(r.p)),
                    None => ("*".to_string(), None),
                }
            }
        }
    }

    /// 只要文本的场合（改组号后重新定级等）。
    #[allow(dead_code)]
    fn recommend_label(&self, gi: usize, a: usize, b: usize) -> String {
        self.recommend_pair(gi, a, b).0
    }

    /// 枚举所有组对，一次性生成全部显著性对比（含自动 P 值分级）。
    fn auto_comparisons(&self, gi: usize) -> Vec<Comparison> {
        let vals = self.graph_group_values(gi);
        stats::pairwise_matrix(&vals, stats::MultiMethod::Tukey, 0.95)
            .into_iter()
            .map(|(i, j, _, adj, st)| Comparison {
                a: i,
                b: j,
                label: Self::sig_label(st, Some(adj)),
                p: Some(adj),
            })
            .collect()
    }

    /// 该消息是否需要读取当前选区？需要的话就先别清理选区。
    ///
    /// 对应「清选区延迟到帧末」那个坑：复制 / 剪切 / 清除 / 粘贴 / 全选
    /// 都在网格代码之后执行，若选区已被清掉就只能作用到一个单元格上。
    fn consumes_selection(msg: &Msg) -> bool {
        matches!(
            msg,
            Msg::CopyCell
                | Msg::CutCell
                | Msg::PasteCell
                | Msg::PasteTransposed
                | Msg::PasteSpecial
                | Msg::ClearCells
                | Msg::SelectAll
        )
    }
}

impl App for SpectraApp {
    type Msg = Msg;

    fn update(&mut self, msg: Msg, _cx: &mut Cx<Msg>) {
        // 见函数末尾：菜单只在「打开菜单」这一条消息之后保持打开。
        let keep_menu = matches!(msg, Msg::OpenRibbonMenu(..));
        // 「跟随系统」：后台线程每秒刷新注册表缓存，但缓存变了不会主动通知 UI。
        // 这里每帧廉价地重解析一次（只读 AtomicU8），系统深浅色一变就立刻跟上，
        // 不必等用户再打开一次设置窗口。
        if self.settings.theme == 2 {
            let want = crate::theme::resolve_dark(2);
            if want != self.dark {
                self.dark = want;
                crate::palette::set_dark(want);
            }
        }
        // ── 坑 1：清选区延迟到「帧末」───────────────────────────────────
        // 菜单（复制 / 剪切 / 清除）的执行发生在网格之后；
        // 点菜单项本身也算一次「点在网格外」。若那一下就立刻清掉选区，
        // 等菜单动作跑起来时选区已经塌成单格 —— 框选后复制只能复制到一个单元格。
        //
        // 所以：点在网格外时**只打标记**（pending_clear_sel），
        // 真正的清理推迟到下一个「非选区消费类」消息进来时执行。
        // 复制 / 剪切 / 粘贴 / 清除这类动作会先读到完整选区，再在之后被清掉。
        if self.pending_clear_sel && !Self::consumes_selection(&msg) {
            self.pending_clear_sel = false;
            self.sel_from = None;
            self.sel_to = None;
        }

        // ── 坑 2：焦点延迟到「帧初」─────────────────────────────────────
        // 渲染途中改焦点状态会被当帧的布局结果覆盖，
        // 所以进入编辑时只记一个标记，等到下一帧开头再真正申请焦点。
        // 代价是首次进入编辑晚一帧拿到光标 —— 这是可接受的。
        if self.pending_focus {
            self.pending_focus = false;
            self.focus_ready = true;
        }

        // 点到别处就自动保存改名：除了「正在改名」自己的三个消息，
        // 其余任何交互都先把未提交的改名落盘。
        if (self.renaming.is_some() || self.sheet_renaming.is_some())
            && !matches!(
                msg,
                Msg::RenameStart(_) | Msg::RenameInput(_) | Msg::RenameCommit | Msg::RenameCancel
            )
        {
            self.commit_rename();
        }

        match msg {
            Msg::None => {}
            Msg::PickTableKind(i) => self.pick_table = i,
            Msg::PickExisting(i) => {
                self.pick_existing = i;
                self.pick_table = usize::MAX;
            }
            Msg::WelcomeCreate => {
                // 左侧选中的是「现有文件」那一组 → 走和菜单「打开…」完全一样的
                // 系统文件选择框流程。之前这里仍然去建新表，属于选了却没生效。
                if self.pick_table == usize::MAX {
                    self.welcome_open = false;
                    self.open_project_flow();
                    return;
                }
                self.welcome_open = false;
                // 按对话框所选类型建表（工厂生成模板）。
                // 新建项目 → 替换首张表；新建数据表 → 追加一张。
                let tt = table_type_of_pick(self.pick_table);
                if self.welcome_mode == 0 {
                    if self.project.tables.is_empty() {
                        self.project.tables.push(new_table("Data 1", tt));
                    } else {
                        self.project.tables[0] = new_table("Data 1", tt);
                    }
                    self.rows_hint = 64;
                    self.after_project_change();
                    self.set_active(Active::Data(0));
                } else {
                    let n = self.project.tables.len() + 1;
                    let t = new_table(&format!("Data {}", n), tt);
                    self.project.tables.push(t);
                    self.open_folders[0] = true;
                    self.rows_hint = 64;
                    self.after_project_change();
                    self.set_active(Active::Data(n - 1));
                }
            }
            Msg::WelcomeCancel => self.welcome_open = false,

            Msg::ToggleFolder(f) => self.open_folders[f] = !self.open_folders[f],
            Msg::OpenSheet(f, i) => {
                let a = match f {
                    0 => Active::Data(i),
                    1 => Active::Info(i),
                    2 => Active::Result(i),
                    3 => Active::Graph(i),
                    _ => Active::Layout(i),
                };
                self.set_active(a);
            }
            Msg::Search(s) => self.search = s,
            // 新建数据表：先弹「选择表类型」对话框，确定类型后再建。
            // 直接按上一次的选择建表会让用户困惑于「这张表到底是什么类型」。
            Msg::NewDataTable => {
                if !self.guard_save_before_table() {
                    return;
                }
                self.close_all_overlays();
                self.welcome_mode = 1;
                self.welcome_open = true;
                self.dlg_open_at = std::time::Instant::now();
            }
            // 新建分析：跑真实统计，而不是塞一份假结果
            Msg::NewAnalysis => {
                self.run_analysis();
            }
            Msg::NewGraph => {
                // **同一个表只保留最新的一张图**：已存在同源图形就整张替换，
                // 而不是再 push 一张。反复「新建图表」不该堆出一串同名兄弟。
                let src = self.table_index();
                let existing = self
                    .project
                    .graphs
                    .iter()
                    .position(|g| g.source_table == src);
                let n = match existing {
                    Some(i) => i + 1,
                    None => self.project.graphs.len() + 1,
                };
                // 取「新建图表」设置里的默认参数
                let st = self.settings.clone();
                // 62 种图型统一走判别值
                let kind = ChartKind::from_usize(st.def_kind);
                // 规范命名：表名 / 图类型（跟随界面语言）
                let src_name = self
                    .project
                    .tables
                    .get(src)
                    .map(|t| t.name.clone())
                    .unwrap_or_else(|| format!("Data {}", src + 1));
                let kind_name = if matches!(self.lang, crate::i18n::Lang::Zh) {
                    kind.label_zh()
                } else {
                    kind.label()
                };
                // 显著性标注条默认结构：无 / 相邻 / 全部两两 / 都与第一组比
                let comps = self.default_comparisons();
                let mut sheet = GraphSheet {
                    frame: plot::FrameStyle::L,
                    name: format!("{} / {}", src_name, kind_name),
                    kind,
                    title: String::new(),
                    x_name: "Group".into(),
                    x_unit: String::new(),
                    y_name: "Response".into(),
                    y_unit: String::new(),
                    unit_style: UnitStyle::Paren,
                    use_italic: true,
                    x_sym: String::new(),
                    y_sym: String::new(),
                    bracket_style: 0,
                    bar_width_cm: 0.0,
                    axis_break: true,
                    right_axis: false,
                    right_from: 0,
                    caption: String::new(),
                    grid_style: 0,
                    dash: 0,

                    source_table: self.table_index(),
                    show_error: st.def_error,
                    show_scatter: st.def_points,
                    show_legend: true,
                    show_bracket: !comps.is_empty(),
                    bracket_mode: BracketMode::Auto,
                    bracket_value: 1.15,
                    colormap: st.def_colormap,
                    comparisons: comps,
                    annotations: Vec::new(),
                    ann_size: 12.0,
                    symbol_follows_bar: true,
                    symbol_color_idx: 0,
                    symbol_size: 2.0,
                    jitter: 0.42,
                    err_kind: 0,
                    err_dir: 0,
                    err_width: 2.0,
                    connect: false,
                    bar_width: 0.55,
                    bar_pattern: 0,
                    bar_zero: true,
                    area_fill: false,
                    grid_on: false,
                    log_y: false,
                    grayscale: false,
                    legend_pos: 0,
                    line_width: 1.0,
                    symbol_shape: 0,
                    symbol_border: 1.5,
                    symbol_alpha: 1.0,
                    title_bold: true,
                    title_italic: false,
                    bins: 8,
                    pie_inner: 0.0,
                    pie_start: 0.0,
                    heat_auto: true,
                    heat_lo: 0.0,
                    heat_hi: 1.0,
                    forest_ref: 0.0,
                    surv_censor: true,
                    waffle_cols: 10,
                    stacked_100: false,
                    facet_shared: true,
                    box_style: 0,
                };
                match existing {
                    Some(i) => {
                        // 保留用户已经改过的名字，避免替换后名字跳号
                        sheet.name = self.project.graphs[i].name.clone();
                        self.project.graphs[i] = sheet;
                    }
                    None => self.project.graphs.push(sheet),
                }
                self.open_folders[3] = true;
                self.set_active(Active::Graph(n - 1));
            }

            Msg::Tool(id) => {
                self.active_tool = id;
                self.status = format!("{} — {}", self.sheet_label(), id);
            }
            Msg::ToggleInspector => self.show_inspector = !self.show_inspector,

            // 点击即进入输入态：先提交上一格（内容早已实时写入，不会丢）
            Msg::CellClick(c, r) => {
                // ── 单击必须能直接输入 ──────────────────────────────
                // 只有「确实拖出了一片」的那次点击才算框选收尾、被吃掉；
                // 判据是锚点与落点不同。单纯单击时 charis 同样会走完
                // Start→End，若一律 return，就变成「点两下才能输入」。
                // 不再用 hover_from 判断"是否拖过"：on_drag 与 on_click 挂在
                // 同一元素上时，两者的触发顺序在框架里没有保证，任何依赖顺序
                // 的判据都会在某些机型/某些操作下失效 —— 表现为「点两下才能
                // 输入」。这里改成：单击一律进编辑；真正的框选收尾由
                // SelDragEnd 单独处理（它只在锚点≠落点时才退出编辑）。
                self.sel_drag = false;
                self.hover_from = None;
                if !self.editing || self.cursor != (c, r) {
                    self.move_cursor(c, r);
                } else {
                    // 已在同一格且处于编辑态：再点一次不退出编辑，
                    // 只把光标放回，避免「点一下丢失输入焦点」。
                    self.editing = true;
                }
            }
            Msg::ToggleSelDrag => {
                self.sel_drag = !self.sel_drag;
                self.hover_from = None;
                if self.sel_drag {
                    self.editing = false;
                }
                self.update_status();
            }

            // ── 真正的鼠标拖拽框选 ──────────────────────────────────────
            // 按下 → 锚点；移动 → hover 扩展；松开 → 结束。
            Msg::SelDragStart(c, r) => {
                self.sel_drag = true;
                self.hover_from = Some((c, r));
                self.sel_from = Some((c, r));
                self.sel_to = Some((c, r));
                self.cursor = (c, r);
                self.editing = false;
                self.ensure_row(r);
                if c != usize::MAX {
                    self.ensure_col(c);
                }
                self.update_status();
            }
            // 移动阶段只更新光标/滚动跟随；真正的落点由 HoverCell 扩展
            Msg::SelDragMove(_d) => {
                if !self.sel_drag {
                    self.sel_drag = true;
                }
            }
            Msg::SelDragEnd => {
                // 没拖动（锚点 == 落点）就是一次普通单击：必须进入输入态。
                // 否则 charis 把单击也走完 Start→End，点击被 End 吃掉，
                // 表现成「点两下才能输入」。
                let same = match (self.sel_from, self.sel_to) {
                    (Some(a), Some(b)) => a == b,
                    _ => true,
                };
                self.sel_drag = false;
                self.hover_from = None;
                // 锚点 == 落点 就是一次普通单击：交给 CellClick 进编辑。
                // 这里若再 move_cursor 一次，会把 CellClick 刚设好的 editing
                // 状态重置（取决于两者谁先到），单击输入就时灵时不灵。
                if !same {
                    // 真的拖出了一片：收尾，不进编辑
                    self.editing = false;
                    self.update_status();
                }
            }
            // 行标题上按下并拖动：连续选多行
            Msg::RowDragStart(r) => {
                self.sel_drag = true;
                self.hover_from = Some((0, r));
                self.sel_from = Some((0, r));
                self.sel_to = Some((self.phys_cols().saturating_sub(1), r));
                self.cursor = (0, r);
                self.editing = false;
                self.update_status();
            }
            Msg::RowDragMove(r) => {
                if !self.sel_drag {
                    return;
                }
                let from = self.hover_from.unwrap_or((0, r)).1;
                self.sel_from = Some((0, from.min(r)));
                self.sel_to = Some((self.phys_cols().saturating_sub(1), from.max(r)));
                self.cursor = (0, r);
                self.ensure_row(r);
                self.update_status();
            }
            // 列标题上按下并拖动：连续选多列
            Msg::RowDragEnd(r) => {
                // 锚点与落点相同 = 只是单击行标题：补一次「选中整行」。
                // 因为 on_drag 挂上后框架未必再发 on_click，不补的话
                // 单击行标题会毫无反应。
                let same = match (self.sel_from, self.sel_to) {
                    (Some(a), Some(b)) => a.1 == b.1,
                    _ => true,
                };
                self.sel_drag = false;
                self.hover_from = None;
                if same {
                    let ncol = self
                        .project
                        .tables
                        .get(self.table_index())
                        .map(|t| t.phys_cols().max(self.cols_hint))
                        .unwrap_or(self.cols_hint)
                        .saturating_sub(1);
                    self.sel_from = Some((0, r));
                    self.sel_to = Some((ncol, r));
                    self.editing = false;
                    self.update_status();
                }
            }
            Msg::ColDragEnd(c) => {
                // 同上：单击列标题补一次「选中整列」
                let same = match (self.sel_from, self.sel_to) {
                    (Some(a), Some(b)) => a.0 == b.0,
                    _ => true,
                };
                self.sel_drag = false;
                self.hover_from = None;
                if same {
                    let ncol = self
                        .project
                        .tables
                        .get(self.table_index())
                        .map(|t| t.phys_cols().max(self.cols_hint))
                        .unwrap_or(self.cols_hint)
                        .saturating_sub(1);
                    let _ = ncol;
                    self.sel_from = Some((c, 0));
                    self.sel_to = Some((c, self.rows_hint.saturating_sub(1)));
                    self.editing = false;
                    self.update_status();
                }
            }
            Msg::ColDragStart(c) => {
                self.sel_drag = true;
                self.hover_from = Some((c, 0));
                self.sel_from = Some((c, 0));
                self.sel_to = Some((c, self.rows_hint.saturating_sub(1)));
                self.cursor = (c, 0);
                self.editing = false;
                self.update_status();
            }
            Msg::ColDragMove(c) => {
                if !self.sel_drag {
                    return;
                }
                let from = self.hover_from.unwrap_or((c, 0)).0;
                let last = self.rows_hint.saturating_sub(1);
                self.sel_from = Some((from.min(c), 0));
                self.sel_to = Some((from.max(c), last));
                self.cursor = (c, 0);
                self.ensure_col(c);
                self.update_status();
            }

            // ── 右键上下文菜单 ──────────────────────────────────────────
            Msg::CellContextMenu(c, r, p) => {
                self.open_ctx_menu(p, c, r, CtxScope::Cell);
            }
            Msg::RowContextMenu(r, p) => {
                self.open_ctx_menu(p, 0, r, CtxScope::Row);
            }
            Msg::ColContextMenu(c, p) => {
                self.open_ctx_menu(p, c, 0, CtxScope::Col);
            }
            // 导航器条目右键：col/row 两个字段在此复用为 (文件夹, 索引)，
            // 所以这里把 folder 放进 col、index 放进 row。
            Msg::SheetContextMenu(f, i, p) => {
                self.open_ctx_menu(p, f, i, CtxScope::Sheet);
            }
            Msg::CloseCtxMenu => self.ctx_menu = None,
            Msg::CtxMenu(a) => {
                // 目标必须先取出来再关菜单：Sheet 作用域的 (文件夹, 索引) 只存在
                // 于 ctx_menu 里，先置 None 会让动作落到当前打开的表单上。
                let tgt = self
                    .ctx_menu
                    .map(|m| (m.col, m.row))
                    .unwrap_or_else(|| (self.active.folder(), self.active.index()));
                self.ctx_menu = None;
                self.run_ctx_action(a, tgt);
            }
            // 每次按键都实时写回稀疏层
            // 每次按键都实时写回稀疏层（内容不会丢）。
            // **不刷新快照**：光标格由 draft 直接渲染，刷新只会带来
            // 每敲一个键就整表克隆一次的开销 —— 这是卡顿的另一半原因。
            Msg::CellInput(s) => {
                self.draft = s;
                if !self.editing {
                    self.editing = true;
                }
                self.write_draft();
            }
            Msg::HoverCell(h) => {
                // 判等后再写，避免 hover 消息把每帧都变成重绘
                if self.hover != h {
                    self.hover = h;
                }
                // 框选模式：鼠标进入的第一个格子是起点，之后实时扩展。
                // 这样「移到起点 → 按住拖到终点」时选区会跟着鼠标一起长出来。
                if self.sel_drag {
                    if let Some((c, r)) = h {
                        let from = self.hover_from.unwrap_or((c, r));
                        self.hover_from = Some(from);
                        self.sel_from = Some(from);
                        self.sel_to = Some((c, r));
                        self.cursor = (c, r);
                        self.ensure_row(r);
                        if c != usize::MAX {
                            self.ensure_col(c);
                        }
                        self.update_status();
                    }
                }
            }
            Msg::GridKey(e) => {
                if !matches!(self.active, Active::Data(_)) {
                    return;
                }
                let (c, r) = self.cursor;
                let ncols = self.phys_cols();

                // Ctrl 组合键：撤销 / 重做 / 复制 / 剪切 / 粘贴
                if e.mods.ctrl {
                    // 去重：on_key（窗口层）与 on_key_capture（元素层）可能
                    // 对同一次按键各发一个 GridKey，不挡住就会粘贴两遍。
                    {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0);
                        if now > self.last_ctrl_ms && now - self.last_ctrl_ms < 80 {
                            return;
                        }
                        self.last_ctrl_ms = now;
                    }
                    // 关键：某些平台把 Ctrl+字母解析成**控制字符**而不是字母本身
                    // （Ctrl+C→U+0003、Ctrl+V→U+0016、Ctrl+Z→U+001A…）。
                    // 只匹配 'c'/'v' 会在这些平台上完全收不到，所以两种都认。
                    let ctrl_char = match e.key {
                        Key::Char(c) => Some(c.to_ascii_lowercase()),
                        _ => None,
                    };
                    let hit = |want: char| -> bool {
                        match ctrl_char {
                            Some(c) => {
                                if c == want {
                                    return true;
                                }
                                // 控制字符：Ctrl+A=1, B=2, C=3, X=18, V=22, Y=25, Z=26
                                let code = want as u32 - 'a' as u32 + 1;
                                c == char::from_u32(code).unwrap_or('\0')
                            }
                            None => false,
                        }
                    };
                    if hit('z') {
                        self.undo();
                        return;
                    }
                    if hit('y') {
                        self.redo();
                        return;
                    }
                    if hit('c') {
                        self.copy_cell();
                        return;
                    }
                    if hit('x') {
                        self.cut_cell();
                        return;
                    }
                    if hit('v') {
                        self.paste_cell();
                        return;
                    }
                    if hit('a') {
                        self.select_all();
                        return;
                    }
                    match e.key {
                        Key::Char('b') | Key::Char('B') => {
                            self.sel_drag = !self.sel_drag;
                            self.hover_from = None;
                            if self.sel_drag {
                                self.editing = false;
                            }
                            self.update_status();
                            return;
                        }
                        Key::Char('a') | Key::Char('A') => {
                            self.select_all();
                            return;
                        }
                        _ => {}
                    }
                }

                // 非编辑态下 Delete / Backspace 清空单元格
                // （编辑态下这两个键要留给文本框，不能抢）
                if !self.editing && matches!(e.key, Key::Delete | Key::Backspace) {
                    self.clear_cell();
                    return;
                }

                // Shift + 方向键：扩展选区（实现「大范围框选」，无需拖拽）
                if e.mods.shift {
                    let anchor = self.sel_from.unwrap_or((c, r));
                    // Ctrl+Shift：一次性扩到数据边缘（Excel 的 Ctrl+Shift+方向键）。
                    // 大范围框选靠这个才现实 —— 逐行按 Shift+↓ 走到第 5 万行不现实。
                    let edge_r = self.last_data_row();
                    let edge_c = self.last_data_col();
                    let (nc, nr) = match e.key {
                        Key::Up if e.mods.ctrl => (c, 0),
                        Key::Down if e.mods.ctrl => (c, edge_r),
                        Key::Left if e.mods.ctrl => (0, r),
                        Key::Right if e.mods.ctrl => (edge_c, r),
                        Key::Home if e.mods.ctrl => (0, 0),
                        Key::End if e.mods.ctrl => (edge_c, edge_r),
                        Key::Up => (c, r.saturating_sub(1)),
                        Key::Down => (c, r.saturating_add(1)),
                        Key::Left => (
                            if c == usize::MAX { usize::MAX } else { c.saturating_sub(1) },
                            r,
                        ),
                        Key::Right => (c.saturating_add(1), r),
                        Key::PageDown => (c, r.saturating_add(20)),
                        Key::PageUp => (c, r.saturating_sub(20)),
                        Key::Home => (0, r),
                        Key::End => (ncols.saturating_sub(1), r),
                        _ => (c, r),
                    };
                    self.sel_from = Some(anchor);
                    self.sel_to = Some((nc, nr));
                    self.cursor = (nc, nr);
                    self.editing = false;
                    self.ensure_row(nr);
                    self.ensure_col(nc);
                    self.update_status();
                    return;
                }

                match e.key {
                    // 注意：c 可能是 usize::MAX（行标题列），所有 c + 1 都要 saturating
                    Key::Up => self.move_cursor(c, r.saturating_sub(1)),
                    Key::Down => self.move_cursor(c, r.saturating_add(1)),
                    // 行标题列再左移停住；首个数据列左移回到行标题列
                    Key::Left => {
                        let prev = match c {
                            usize::MAX => usize::MAX,
                            0 => {
                                if self.project.tables[self.table_index()]
                                    .table
                                    .table_type
                                    .has_row_titles()
                                {
                                    usize::MAX
                                } else {
                                    0
                                }
                            }
                            _ => c - 1,
                        };
                        self.move_cursor(prev, r);
                    }
                    Key::Right => {
                        let next = c.saturating_add(1);
                        if next >= ncols {
                            // 行末：停住（不跨行，避免误操作）
                        } else {
                            self.move_cursor(next, r);
                        }
                    }
                    Key::Enter => self.move_cursor(c, r.saturating_add(1)),
                    Key::Tab => {
                        let next = c.saturating_add(1);
                        if next >= ncols {
                            self.move_cursor(0, r.saturating_add(1));
                        } else {
                            self.move_cursor(next, r);
                        }
                    }
                    Key::Escape => {
                        self.editing = false;
                        self.draft.clear();
                        // 正在框选 → 取消这次框选，选区一并放弃
                        if self.sel_drag {
                            self.sel_drag = false;
                            self.hover_from = None;
                            self.sel_from = None;
                            self.sel_to = None;
                            self.update_status();
                            return;
                        }
                        // 只打标记：真正的清理推迟到下一个非选区消费类消息，
                        // 这样「Esc 之后紧接着按 Ctrl+C」仍能作用在当前选区上。
                        self.pending_clear_sel = true;
                    }
                    Key::PageDown => self.move_cursor(c, r.saturating_add(20)),
                    Key::PageUp => self.move_cursor(c, r.saturating_sub(20)),
                    Key::Home => self.move_cursor(0, r),
                    Key::End => self.move_cursor(ncols.saturating_sub(1), r),
                    // 退格 / Delete：没在编辑就清空该格。
                    // 编辑态下 key_capture 返回 None，事件回到输入框由它处理，
                    // 所以不会冲突。
                    Key::Delete | Key::Backspace => self.clear_cell(),
                    // 其余键（含字符）交回输入框处理
                    _ => {}
                }
            }

            Msg::SetChartKind(i) => {
                let kind = ChartKind::from_usize(i);
                for g in &mut self.project.graphs {
                    g.kind = kind;
                }
                self.mark_chart_dirty();
            }
            Msg::ToggleError => {
                for g in &mut self.project.graphs {
                    g.show_error = !g.show_error;
                }
                self.mark_chart_dirty();
            }
            Msg::ToggleScatter => {
                for g in &mut self.project.graphs {
                    g.show_scatter = !g.show_scatter;
                }
                self.mark_chart_dirty();
            }
            Msg::ToggleLegend => {
                for g in &mut self.project.graphs {
                    g.show_legend = !g.show_legend;
                }
                self.mark_chart_dirty();
            }
            Msg::SetColormap(i) => {
                for g in &mut self.project.graphs {
                    g.colormap = i;
                }
                self.mark_chart_dirty();
            }
            Msg::SetXName(s) => {
                for g in &mut self.project.graphs {
                    g.x_name = s.clone();
                }
                self.mark_chart_dirty();
            }
            Msg::SetXSym(s) => {
                for g in &mut self.project.graphs {
                    g.x_sym = s.clone();
                }
                self.mark_chart_dirty();
            }
            Msg::SetYSym(s) => {
                for g in &mut self.project.graphs {
                    g.y_sym = s.clone();
                }
                self.mark_chart_dirty();
            }
            Msg::SetBracketStyle(i) => {
                for g in &mut self.project.graphs {
                    g.bracket_style = i.min(1);
                }
                self.mark_chart_dirty();
            }
            Msg::SetBarWidthCm(v) => {
                for g in &mut self.project.graphs {
                    g.bar_width_cm = v.clamp(0.0, 6.0);
                }
                self.mark_chart_dirty();
            }
            Msg::ToggleAxisBreak => {
                for g in &mut self.project.graphs {
                    g.axis_break = !g.axis_break;
                }
                self.mark_chart_dirty();
            }
            Msg::ToggleRightAxis => {
                for g in &mut self.project.graphs {
                    g.right_axis = !g.right_axis;
                }
                self.mark_chart_dirty();
            }
            Msg::SetRightFrom(i) => {
                for g in &mut self.project.graphs {
                    g.right_from = i;
                }
                self.mark_chart_dirty();
            }
            Msg::SetCaption(v) => {
                for g in &mut self.project.graphs {
                    g.caption = v.clone();
                }
                self.mark_chart_dirty();
            }
            Msg::SetGridStyle(i) => {
                for g in &mut self.project.graphs {
                    g.grid_style = i.min(2);
                }
                self.mark_chart_dirty();
            }
            Msg::SetDash(i) => {
                for g in &mut self.project.graphs {
                    g.dash = i.min(2);
                }
                self.mark_chart_dirty();
            }
            Msg::SetXUnit(s) => {
                for g in &mut self.project.graphs {
                    g.x_unit = s.clone();
                }
                self.mark_chart_dirty();
            }
            Msg::SetYName(s) => {
                for g in &mut self.project.graphs {
                    g.y_name = s.clone();
                }
                self.mark_chart_dirty();
            }
            Msg::SetYUnit(s) => {
                for g in &mut self.project.graphs {
                    g.y_unit = s.clone();
                }
                self.mark_chart_dirty();
            }
            Msg::SetUnitStyle(i) => {
                let u = UnitStyle::from_i32(i as i32);
                for g in &mut self.project.graphs {
                    g.unit_style = u;
                }
                self.mark_chart_dirty();
            }
            Msg::ToggleItalic => {
                for g in &mut self.project.graphs {
                    g.use_italic = !g.use_italic;
                }
                self.mark_chart_dirty();
            }
            Msg::ToggleBracket => {
                for g in &mut self.project.graphs {
                    g.show_bracket = !g.show_bracket;
                }
                self.mark_chart_dirty();
            }
            Msg::SetChartTitle(s) => {
                for g in &mut self.project.graphs {
                    g.title = s.clone();
                }
                self.mark_chart_dirty();
            }
            // ── 数据表结构：真实增删，不是摆设 ──────────────────────────
            // 借用规则：先算索引、先压撤销栈，再取可变引用；
            // 并把可变借用限制在块作用域内，结束之后才能调 self 的其他方法。
            Msg::AddDataSet => {
                let ti = self.table_index();
                if ti < self.project.tables.len() {
                    self.push_undo();
                    {
                        let t = &mut self.project.tables[ti];
                        let n = t.table.data_sets.len();
                        t.table.data_sets.push(crate::model::DataSetMeta {
                            name: t.table.dataset_letter(n),
                            sub_columns: vec![],
                        });
                    }
                    self.after_project_change();
                    self.status = i18n::t(self.lang, K::AddDataSet).to_string();
                }
            }
            Msg::RemoveLastDataSet => {
                let ti = self.table_index();
                let can = self.project.tables.get(ti).map(|t| t.table.data_sets.len() > 1).unwrap_or(false);
                if can {
                    self.push_undo();
                    {
                        self.project.tables[ti].table.data_sets.pop();
                    }
                    self.after_project_change();
                    self.status = i18n::t(self.lang, K::RemoveLastDataSet).to_string();
                } else {
                    self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                }
            }
            Msg::AddSubColumn => {
                let ti = self.table_index();
                let ok = self.project.tables.get(ti).map(|t| t.table.table_type.has_subcolumns()).unwrap_or(false);
                if !ok {
                    self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                } else {
                    self.push_undo();
                    {
                        let t = &mut self.project.tables[ti];
                        let side = t.table.table_type.sub_layout()
                            == crate::model::SubLayout::SideBySide;
                        for ds in t.table.data_sets.iter_mut() {
                            let k = ds.sub_columns.len() + 1;
                            ds.sub_columns.push(crate::model::SubColumnMeta {
                                name: if side { format!("Y{}", k) } else { k.to_string() },
                            });
                        }
                    }
                    self.after_project_change();
                    self.status = i18n::t(self.lang, K::AddSubColumn).to_string();
                }
            }
            Msg::RemoveLastSubColumn => {
                let ti = self.table_index();
                let can = self.project.tables.get(ti).map(|t| {
                    let min = if t.table.table_type == TableType::Nested { 2 } else { 1 };
                    t.table.data_sets.first().map(|d| d.sub_columns.len()).unwrap_or(0) > min
                }).unwrap_or(false);
                if !can {
                    self.alert(i18n::t(self.lang, K::NotApplicable).to_string());
                } else {
                    self.push_undo();
                    {
                        for ds in self.project.tables[ti].table.data_sets.iter_mut() {
                            ds.sub_columns.pop();
                        }
                    }
                    self.after_project_change();
                    self.status = i18n::t(self.lang, K::RemoveLastSubColumn).to_string();
                }
            }
            Msg::InsertRow => {
                let ti = self.table_index();
                if ti < self.project.tables.len() {
                    let at = self.cursor.1;
                    self.push_undo();
                    {
                        let t = &mut self.project.tables[ti];
                        // 行是稀疏的：只做键平移，不预分配
                        t.table.insert_row(at);
                        t.view_rows = t.view_rows.saturating_add(1);
                    }
                    self.after_project_change();
                    self.status = i18n::t(self.lang, K::InsertRow).to_string();
                }
            }
            Msg::DeleteRow => {
                let ti = self.table_index();
                if ti < self.project.tables.len() {
                    let at = self.cursor.1;
                    self.push_undo();
                    {
                        let t = &mut self.project.tables[ti];
                        t.table.delete_row(at);
                        t.view_rows = t.view_rows.saturating_sub(1).max(1);
                    }
                    self.after_project_change();
                    self.status = i18n::t(self.lang, K::DeleteRow).to_string();
                }
            }
            Msg::DeleteCol => self.delete_col_at_cursor(),
            Msg::DeleteDataSet => self.delete_dataset_at_cursor(),
            Msg::ClearCells => self.clear_cell(),
            // ── 剪贴板 / 撤销 ────────────────────────────────────────────
            Msg::CopyCell => self.copy_cell(),
            Msg::CutCell => self.cut_cell(),
            // Ctrl+V 与功能区「从剪贴板粘贴」走**同一个入口**，行为必须完全一致。
            // 差异只在编辑态：点功能区按钮时焦点已经离开单元格、editing 为 false，
            // 而按 Ctrl+V 时往往还在输入态。所以这里先提交编辑再粘贴，
            // 保证两条路都把内容写进**单元格**（而不是悄悄塞进输入框草稿）。
            Msg::PasteCell => {
                if self.editing {
                    self.commit();
                }
                self.paste_cell();
            }
            Msg::DoUndo => self.undo(),
            Msg::DoRedo => self.redo(),
            // ── 分析：真实统计 ───────────────────────────────────────────
            // 不能夹到 3：右侧栏现在列出的是「按表型动态生成的完整分析清单」，
            // 其中智能推荐用的是 ADVISOR_ID(=100)，夹一下会被当成线性回归。
            Msg::SetAnalysis(i) => self.analysis = i,
            Msg::SetWelch(b) => {
                self.params.welch = b;
                self.welch = b;
            }
            Msg::RunAnalysis => self.run_analysis(),
            // ── 文件 ─────────────────────────────────────────────────────
            Msg::SaveProject => {
                let path = self
                    .file_path
                    .clone()
                    .unwrap_or_else(|| "project.sprx".to_string());
                if self.write_project(&path) {
                    self.file_path = Some(path);
                }
            }
            Msg::OpenProject => self.open_project_flow(),
            Msg::SaveFamily => self.save_family(),
            Msg::SaveExample => self.save_example(),
            Msg::SaveTemplate => self.save_template(),
            Msg::SaveMethod => self.save_method(),
            Msg::DuplicateFamily => self.duplicate_family(),
            Msg::MergeFile => self.merge_file(),
            // 粘贴为嵌入：本应用内没有 OLE 容器，语义就是「粘贴为值」
            Msg::PasteEmbedded => self.paste_cell(),
            // 粘贴为链接：不支持跨工程链接，降级为粘贴值并如实提示
            Msg::PasteLinked => {
                self.paste_cell();
                self.status = i18n::t(self.lang, K::PasteLinkedHint).to_string();
            }
            Msg::PasteTransposedEmbedded => self.paste_transposed(),
            Msg::PasteTransposedLinked => {
                self.paste_transposed();
                self.status = i18n::t(self.lang, K::PasteLinkedHint).to_string();
            }
            Msg::ToggleSymbolFollows => {
                for g in &mut self.project.graphs {
                    g.symbol_follows_bar = !g.symbol_follows_bar;
                }
                self.mark_chart_dirty();
            }
            Msg::SetSymbolColor(i) => {
                for g in &mut self.project.graphs {
                    g.symbol_color_idx = i;
                }
                self.mark_chart_dirty();
            }
            Msg::SetFrame(i) => {
                for g in &mut self.project.graphs {
                    g.frame = if i == 1 {
                        plot::FrameStyle::Box
                    } else {
                        plot::FrameStyle::L
                    };
                }
                self.mark_chart_dirty();
            }
            Msg::SetSymbolSize(v) => {
                for g in &mut self.project.graphs {
                    g.symbol_size = v;
                }
                self.mark_chart_dirty();
            }
            Msg::SetJitter(v) => {
                for g in &mut self.project.graphs {
                    g.jitter = v;
                }
                self.mark_slider();
            }
            // ══ 绘图区面板（九组）══════════════════════════════════════════
            Msg::SetErrKind(v) => {
                for g in &mut self.project.graphs {
                    g.err_kind = v;
                }
                self.mark_chart_dirty();
            }
            Msg::SetErrDir(v) => {
                for g in &mut self.project.graphs {
                    g.err_dir = v;
                }
                self.mark_chart_dirty();
            }
            Msg::SetErrWidth(v) => {
                for g in &mut self.project.graphs {
                    g.err_width = v;
                }
                self.mark_slider();
            }
            Msg::ToggleConnect => {
                for g in &mut self.project.graphs {
                    g.connect = !g.connect;
                }
                self.mark_chart_dirty();
            }
            Msg::SetBarWidth(v) => {
                for g in &mut self.project.graphs {
                    g.bar_width = v;
                }
                self.mark_slider();
            }
            Msg::SetBarPattern(v) => {
                for g in &mut self.project.graphs {
                    g.bar_pattern = v;
                }
                self.mark_chart_dirty();
            }
            Msg::ToggleBarZero => {
                for g in &mut self.project.graphs {
                    g.bar_zero = !g.bar_zero;
                }
                self.mark_chart_dirty();
            }
            Msg::ToggleAreaFill => {
                for g in &mut self.project.graphs {
                    g.area_fill = !g.area_fill;
                }
                self.mark_chart_dirty();
            }
            Msg::ToggleGrid => {
                for g in &mut self.project.graphs {
                    g.grid_on = !g.grid_on;
                }
                self.mark_chart_dirty();
            }
            Msg::ToggleLogY => {
                for g in &mut self.project.graphs {
                    g.log_y = !g.log_y;
                }
                self.mark_chart_dirty();
            }
            Msg::ToggleGrayscale => {
                for g in &mut self.project.graphs {
                    g.grayscale = !g.grayscale;
                }
                self.mark_chart_dirty();
            }
            Msg::SetLegendPos(v) => {
                for g in &mut self.project.graphs {
                    g.legend_pos = v;
                }
                self.mark_chart_dirty();
            }
            Msg::SetLineWidth(v) => {
                for g in &mut self.project.graphs {
                    g.line_width = v;
                }
                self.mark_slider();
            }
            Msg::SetSymbolShape(v) => {
                for g in &mut self.project.graphs {
                    g.symbol_shape = v;
                }
                self.mark_chart_dirty();
            }
            Msg::SetSymbolBorder(v) => {
                for g in &mut self.project.graphs {
                    g.symbol_border = v;
                }
                self.mark_slider();
            }
            Msg::SetSymbolAlpha(v) => {
                for g in &mut self.project.graphs {
                    g.symbol_alpha = v;
                }
                self.mark_slider();
            }
            Msg::ToggleTitleBold => {
                for g in &mut self.project.graphs {
                    g.title_bold = !g.title_bold;
                }
                self.mark_chart_dirty();
            }
            Msg::ToggleTitleItalic => {
                for g in &mut self.project.graphs {
                    g.title_italic = !g.title_italic;
                }
                self.mark_chart_dirty();
            }
            // 滑块松手：这时才真正重绘。
            // 拖动过程中每一帧都重画位图会明显卡顿，而用户此时只需要看数值变化。
            Msg::SetGraphName(v) => {
                let gi = match self.active {
                    Active::Graph(i) => i,
                    _ => 0,
                };
                if let Some(g) = self.project.graphs.get_mut(gi) {
                    g.name = v;
                }
                self.after_project_change();
            }
            Msg::SetBinsF(v) => {
                for g in &mut self.project.graphs {
                    g.bins = (v as usize).clamp(1, 64);
                }
                self.mark_slider();
            }
            Msg::SetWaffleColsF(v) => {
                for g in &mut self.project.graphs {
                    g.waffle_cols = (v as usize).clamp(1, 64);
                }
                self.mark_slider();
            }
            Msg::ToggleStacked100 => {
                for g in &mut self.project.graphs {
                    g.stacked_100 = !g.stacked_100;
                }
                self.mark_chart_dirty();
            }
            Msg::ToggleFacetShared => {
                for g in &mut self.project.graphs {
                    g.facet_shared = !g.facet_shared;
                }
                self.mark_chart_dirty();
            }
            Msg::SetBoxStyle(v) => {
                for g in &mut self.project.graphs {
                    g.box_style = v.min(1);
                }
                self.mark_chart_dirty();
            }
            Msg::SetPieInner(v) => {
                for g in &mut self.project.graphs {
                    g.pie_inner = v;
                }
                self.mark_slider();
            }
            Msg::SetPieStart(v) => {
                for g in &mut self.project.graphs {
                    g.pie_start = v;
                }
                self.mark_slider();
            }
            Msg::ToggleHeatAuto => {
                for g in &mut self.project.graphs {
                    g.heat_auto = !g.heat_auto;
                }
                self.mark_chart_dirty();
            }
            Msg::SetForestRef(v) => {
                for g in &mut self.project.graphs {
                    g.forest_ref = v;
                }
                self.mark_chart_dirty();
            }
            Msg::ToggleSurvCensor => {
                for g in &mut self.project.graphs {
                    g.surv_censor = !g.surv_censor;
                }
                self.mark_chart_dirty();
            }
            Msg::SetTitle(v) => {
                let gi = match self.active {
                    Active::Graph(i) => i,
                    _ => 0,
                };
                if let Some(g) = self.project.graphs.get_mut(gi) {
                    g.title = v;
                }
                self.mark_chart_dirty();
            }
            Msg::SetCanvasH(v) => {
                self.base_h = v.clamp(120.0, 600.0) as u32;
                self.mark_slider();
            }
            Msg::SliderDone => {
                self.mark_chart_dirty();
                if matches!(self.active, Active::Graph(_)) {
                    self.render_chart();
                }
            }
            Msg::SetBracketMode(i) => {
                let m = match i {
                    1 => BracketMode::Absolute,
                    2 => BracketMode::Relative,
                    _ => BracketMode::Auto,
                };
                for g in &mut self.project.graphs {
                    g.bracket_mode = m;
                }
                self.mark_chart_dirty();
            }
            Msg::SetBracketValue(s) => {
                if let Ok(v) = s.trim().parse::<f64>() {
                    for g in &mut self.project.graphs {
                        g.bracket_value = v;
                    }
                    self.mark_chart_dirty();
                }
            }
            Msg::SetBracketLabelFmt(v) => {
                let mut st = crate::settings::get();
                st.bracket_label_fmt = v.min(2);
                crate::settings::replace(st);
                crate::settings::save_current();
                // 已有的自动对比按新写法重排文本；P 值本身不变，
                // 手写且没有 P 值的（p = None）保持原样不动。
                for g in &mut self.project.graphs {
                    for c in &mut g.comparisons {
                        if let Some(p) = c.p {
                            c.label = Self::sig_label(stats::stars(p), Some(p));
                        }
                    }
                }
                self.mark_chart_dirty();
            }
            Msg::AddComparison => {
                // 先算好，再写进去：借用检查不允许在遍历里再调 &self 的方法
                let gi = match self.active {
                    Active::Graph(i) => i,
                    _ => self.project.graphs.len().saturating_sub(1),
                };
                // 组对按 (0,1) (0,2) (1,2) (0,3) … 的顺序枚举，跳过已添加的。
                // 以前是按「已添加条数」推算，删过一条后就会重复或越界。
                let n_grp = self.group_names(gi).len().max(2);
                let used: Vec<(usize, usize)> = self
                    .project
                    .graphs
                    .get(gi)
                    .map(|g| {
                        g.comparisons
                            .iter()
                            .map(|c| (c.a.min(c.b), c.a.max(c.b)))
                            .collect()
                    })
                    .unwrap_or_default();
                let mut pair: Option<(usize, usize)> = None;
                'outer: for x in 0..n_grp {
                    for y in (x + 1)..n_grp {
                        if !used.contains(&(x, y)) {
                            pair = Some((x, y));
                            break 'outer;
                        }
                    }
                }
                let (a, b) = pair.unwrap_or((0, 1 % n_grp.max(1)));
                let (label, pv) = self.recommend_pair(gi, a, b);
                if let Some(g) = self.project.graphs.get_mut(gi) {
                    g.comparisons.push(Comparison { a, b, label, p: pv });
                    g.show_bracket = true;
                }
                self.mark_chart_dirty();
            }
            // 一键枚举所有组对：全部两两比较，P 值自动分级
            Msg::AutoComparisons => {
                let gi = match self.active {
                    Active::Graph(i) => i,
                    _ => self.project.graphs.len().saturating_sub(1),
                };
                let comps = self.auto_comparisons(gi);
                if let Some(g) = self.project.graphs.get_mut(gi) {
                    g.comparisons = comps;
                    g.show_bracket = !g.comparisons.is_empty();
                }
                self.mark_chart_dirty();
            }
            Msg::RemoveComparison(i) => {
                for g in &mut self.project.graphs {
                    if i < g.comparisons.len() {
                        g.comparisons.remove(i);
                    }
                }
                self.mark_chart_dirty();
            }
            Msg::EditComparison(i, a, b, label) => {
                let gi = match self.active {
                    Active::Graph(x) => x,
                    _ => self.project.graphs.len().saturating_sub(1),
                };
                // 只改了组号、没手写标注 → 重新按 P 值推荐分级。
                // 先算再写：借用检查不允许在 &mut 遍历里调 &self 的方法。
                let recomputed: Option<(String, Option<f64>)> = match (label.clone(), a, b) {
                    (None, Some(x), Some(y)) => Some(self.recommend_pair(gi, x.min(y), x.max(y))),
                    (None, Some(x), None) => {
                        let cur_b = self
                            .project
                            .graphs
                            .get(gi)
                            .and_then(|g| g.comparisons.get(i))
                            .map(|c| c.b)
                            .unwrap_or(x + 1);
                        Some(self.recommend_pair(gi, x.min(cur_b), x.max(cur_b)))
                    }
                    (None, None, Some(y)) => {
                        let cur_a = self
                            .project
                            .graphs
                            .get(gi)
                            .and_then(|g| g.comparisons.get(i))
                            .map(|c| c.a)
                            .unwrap_or(0);
                        Some(self.recommend_pair(gi, cur_a.min(y), cur_a.max(y)))
                    }
                    // 用户手写了标注 → 尊重手写值，不覆盖
                    (Some(_), _, _) => None,
                    // 三组都没变（只改了别处）：保持原样
                    (None, None, None) => None,
                };
                if let Some(g) = self.project.graphs.get_mut(gi) {
                    if let Some(c) = g.comparisons.get_mut(i) {
                        if let Some(v) = a {
                            c.a = v;
                        }
                        if let Some(v) = b {
                            c.b = v;
                        }
                        if let Some(v) = &label {
                            c.label = v.clone();
                        } else if let Some((v, pv)) = recomputed {
                            c.label = v;
                            c.p = pv;
                        }
                    }
                }
                self.mark_chart_dirty();
            }
            // ══ File ═══════════════════════════════════════════════════
            // 新建项目：先清空，再弹「选择表类型」对话框。
            // 项目里第一张表的类型决定了后续所有分析与作图，必须让用户选。
            Msg::NewProject => {
                self.project = Project::blank();
                self.cursor = (0, 0);
                self.draft.clear();
                self.editing = false;
                self.renaming = None;
                self.rename_draft.clear();
                self.sel_from = None;
                self.sel_to = None;
                self.rows_hint = 64;
                self.open_folders = [true, true, true, true, true];
                self.after_project_change();
                self.set_active(Active::Data(0));
                self.close_all_overlays();
                self.welcome_mode = 0;
                self.welcome_open = true;
                self.dlg_open_at = std::time::Instant::now();
            }
            Msg::SaveAs => self.save_as(),
            Msg::CloseSheet => self.close_sheet(),
            Msg::CloseAll => {
                self.project = Project::blank();
                self.cursor = (0, 0);
                self.draft.clear();
                self.editing = false;
                self.after_project_change();
                self.set_active(Active::Data(0));
                self.status = i18n::t(self.lang, K::CloseAll).to_string();
            }
            Msg::SaveTableCsv => self.save_table_csv(),
            Msg::SaveResultsCsv => self.save_results_csv(),
            Msg::SaveGraphSvg => self.export_current_graph(),
            Msg::ExportChart => self.export_current_graph(),
            Msg::ExportMatplotlib => {
                let stem = "spectra_chart";
                match crate::export::export(
                    &self.chart_view(),
                    crate::export::ExportBackend::Matplotlib,
                    stem,
                ) {
                    Ok(p) => self.status = format!("{} {}", i18n::t(self.lang, K::ExportedMpl), p),
                    Err(e) => self.alert(e),
                }
            }
            Msg::PrintSetup => self.status = i18n::t(self.lang, K::PrintHint).to_string(),
            Msg::SendToPpt => self.send_to_office("ppt"),
            Msg::SendToWord => self.send_to_office("doc"),
            Msg::CloudSpectra | Msg::CloudLuma => {
                self.status = i18n::t(self.lang, K::NotImplementedCloud).to_string()
            }
            Msg::CheckCompat | Msg::RunScript => {
                self.status = i18n::t(self.lang, K::NotImplementedCloud).to_string()
            }

            // ══ Edit ═══════════════════════════════════════════════════
            Msg::PasteTransposed => self.paste_transposed(),
            Msg::PasteSpecial => self.paste_cell(),
            Msg::SelectAll => self.select_all(),
            Msg::SelectColumn => self.status = i18n::t(self.lang, K::SelectColumnDone).to_string(),
            Msg::SelectRow => self.status = i18n::t(self.lang, K::SelectRowDone).to_string(),
            Msg::SortByX => self.sort_rows_by(true),
            Msg::SortByRowTitle => self.sort_rows_by(false),
            Msg::ReverseRowOrder => self.reverse_row_order(),
            Msg::ExcludeValues => {
                self.status = i18n::t(self.lang, K::ExcludeHint).to_string()
            }
            Msg::DeleteTable => self.delete_table(),
            Msg::TableOrder => self.status = i18n::t(self.lang, K::TableOrderHint).to_string(),
            Msg::ToggleFreeze => {
                self.freeze_table = !self.freeze_table;
                self.status = i18n::t(self.lang, K::FreezeTable).to_string();
            }
            Msg::RenameTable => self.status = i18n::t(self.lang, K::RenameHint).to_string(),
            Msg::SetHighlight(i) => {
                self.highlight = i;
                self.status = i18n::t(self.lang, K::HighlightSet).to_string();
            }
            Msg::LinkTable | Msg::DataObject | Msg::UnlinkCalc => {
                self.status = i18n::t(self.lang, K::LinkHint).to_string()
            }

            // ══ View ═══════════════════════════════════════════════════
            Msg::ZoomFitPage => {
                self.zoom = 1.0;
                self.status = i18n::t(self.lang, K::ZoomFitPage).to_string();
            }
            Msg::ZoomFitChart => {
                self.zoom = 1.0;
                self.mark_chart_dirty();
                self.status = i18n::t(self.lang, K::ZoomFitChart).to_string();
            }
            Msg::ZoomIn => self.set_zoom((self.zoom * 1.25).min(6.0)),
            Msg::ZoomOut => self.set_zoom((self.zoom / 1.25).max(0.10)),
            Msg::SetZoom(z) => self.set_zoom(z),
            Msg::GoToDataTable => {
                self.set_active(Active::Data(0));
                self.status = i18n::t(self.lang, K::DataTablesView).to_string();
            }
            Msg::GoToLibrary => {
                self.status = i18n::t(self.lang, K::LibraryHint).to_string()
            }
            Msg::ToggleMainToolbar => self.main_toolbar = !self.main_toolbar,
            Msg::ToggleNavigator => self.navigator = !self.navigator,
            Msg::ToggleBottomBar => self.bottom_toolbar = !self.bottom_toolbar,
            Msg::ToggleRulers => self.rulers = !self.rulers,
            Msg::ToggleGridView => self.grid_view = !self.grid_view,
            Msg::ToggleRowTitles => self.row_titles_on = !self.row_titles_on,
            Msg::ToggleColTitles => self.col_titles_on = !self.col_titles_on,
            Msg::RestoreNotif => self.status = i18n::t(self.lang, K::NotifRestored).to_string(),

            // 表头组名：双击进入编辑 → Enter 提交 / Escape 取消
            Msg::RenameStart(target) => {
                let ti = self.table_index();
                self.rename_draft = self
                    .project
                    .tables
                    .get(ti)
                    .and_then(|t| match target {
                        RenameTarget::DataSet(di) => {
                            t.table.data_sets.get(di).map(|d| d.name.clone())
                        }
                        RenameTarget::Sub(di, si) => t
                            .table
                            .data_sets
                            .get(di)
                            .and_then(|d| d.sub_columns.get(si))
                            .map(|sc| sc.name.clone()),
                    })
                    .unwrap_or_default();
                self.renaming = Some(target);
            }
            Msg::RenameInput(v) => self.rename_draft = v,
            Msg::RenameCommit => self.commit_rename(),
            Msg::RenameCancel => {
                self.renaming = None;
                self.rename_draft.clear();
            }
            // 单击列标题：选中整列（区间，覆盖该数据集的全部子列）
            Msg::PickCols(c0, c1) => {
                let last = self.rows_hint.saturating_sub(1);
                self.sel_from = Some((c0.min(c1), 0));
                self.sel_to = Some((c0.max(c1), last));
                self.cursor = (c0.min(c1), 0);
                self.editing = false;
                self.update_status();
            }
            // 单击行标题：选中整行，同时把光标留在行标题列以便直接改标题
            Msg::PickRow(r) => {
                let ncol = self
                    .project
                    .tables
                    .get(self.table_index())
                    .map(|t| t.phys_cols().max(self.cols_hint))
                    .unwrap_or(self.cols_hint)
                    .saturating_sub(1);
                self.sel_from = Some((0, r));
                self.sel_to = Some((ncol, r));
                self.cursor = (usize::MAX, r);
                self.editing = false;
                self.ensure_row(r);
                self.update_status();
            }

            // ══ Insert ═════════════════════════════════════════════════
            // Insert ▸ 新建信息：建完直接打开这一页，
            // 否则用户看不到「新建了」——反馈必须发生在用户本来就期待的地方。
            Msg::NewInfo => {
                let n = self.project.infos.len() + 1;
                self.push_undo();
                self.project.infos.push(format!("New info {}", n));
                self.open_folders[1] = true;
                self.set_active(Active::Info(n - 1));
                self.status = i18n::t(self.lang, K::NewInfo).to_string();
            }
            Msg::DuplicateTable => self.duplicate_table(),
            Msg::DuplicateSeries => self.duplicate_series(),
            // Insert ▸ 插入列：在光标所在列之后插入一个数据集
            // （行/列… 菜单的两半：行由 InsertRow 负责，列由这里负责）
            Msg::InsertCol => {
                let ti = self.table_index();
                let at = self.cursor.0;
                if ti < self.project.tables.len() {
                    self.push_undo();
                    {
                        let t = &mut self.project.tables[ti];
                        let d = t.table.new_dataset_template();
                        let pos = (at + 1).min(t.table.data_sets.len());
                        t.table.data_sets.insert(pos, d);
                    }
                    self.after_project_change();
                    self.ensure_col(at + 1);
                    self.status = i18n::t(self.lang, K::InsertRow).to_string();
                }
            }
            Msg::CalcVariable => {
                // 计算变量：新增一个数据集，列名为 Var
                let ti = self.table_index();
                if ti < self.project.tables.len() {
                    self.push_undo();
                    {
                        let t = &mut self.project.tables[ti];
                        let mut d = t.table.new_dataset_template();
                        d.name = "Var".to_string();
                        t.table.data_sets.push(d);
                    }
                    self.after_project_change();
                    self.status = i18n::t(self.lang, K::AddDataSet).to_string();
                }
            }
            // Insert ▸ 创建系列：等价于新增一个数据集
            Msg::CreateSeries => {
                let ti = self.table_index();
                if ti < self.project.tables.len() {
                    self.push_undo();
                    {
                        let t = &mut self.project.tables[ti];
                        t.table.data_sets.push(t.table.new_dataset_template());
                    }
                    self.after_project_change();
                    self.status = i18n::t(self.lang, K::AddDataSet).to_string();
                }
            }
            Msg::RiskTable | Msg::EmbedTable | Msg::InsertChar | Msg::InsertConst
            | Msg::InsertObject => {
                self.status = i18n::t(self.lang, K::InsertHint).to_string()
            }

            // ══ Analyze（动态：按方法 id 直接跑）════════════════════════
            Msg::RunAnalysisById(id) => {
                self.analysis = id;
                self.run_analysis();
            }

            // ══ Change ═════════════════════════════════════════════════
            Msg::FlipHorizontal => {
                for g in &mut self.project.graphs {
                    for c in g.comparisons.iter_mut() {
                        let n = self
                            .project
                            .tables
                            .first()
                            .map(|t| t.table.data_sets.len().saturating_sub(1))
                            .unwrap_or(0);
                        c.a = n - c.a.min(n);
                        c.b = n - c.b.min(n);
                    }
                }
                self.mark_chart_dirty();
                self.status = i18n::t(self.lang, K::FlipHorizontal).to_string();
            }
            // Change ▸ 数据集顺序：把第二个数据集轮到最前（循环左移一位）。
            // 析光这里是拖拽对话框，本实现用「循环轮转」达到同样的重排效果。
            Msg::DataSetOrder => {
                let ti = self.table_index();
                let n = self
                    .project
                    .tables
                    .get(ti)
                    .map(|t| t.table.data_sets.len())
                    .unwrap_or(0);
                if n > 1 {
                    self.push_undo();
                    {
                        let t = &mut self.project.tables[ti];
                        let first = t.table.data_sets.remove(0);
                        t.table.data_sets.push(first);
                    }
                    self.after_project_change();
                    self.status = i18n::t(self.lang, K::DsOrderHint).to_string();
                }
            }
            Msg::ReverseDataSetOrder => self.reverse_data_set_order(),
            Msg::OpenColorSettings => {
                self.settings_open = true;
                self.settings_tab = 0;
                self.lang_draft = self.lang;
                self.status = i18n::t(self.lang, K::DefineColorScheme).to_string();
            }
            Msg::HintInspector => {
                self.status = i18n::t(self.lang, K::UseInspectorHint).to_string()
            }
            Msg::ReverseLegend => {
                self.reverse_legend = !self.reverse_legend;
                self.mark_chart_dirty();
                self.status = i18n::t(self.lang, K::ReverseLegend).to_string();
            }
            Msg::AnalysisParams => {
                self.status = i18n::t(self.lang, K::AnalysisParamsHint).to_string()
            }
            Msg::Resimulate => self.resimulate(),
            // Change ▸ 调整图表尺寸：在三档预设之间循环，改的是导出/预览画布尺寸
            Msg::ResizeGraph => {
                const SIZES: [(u32, u32); 3] = [(400, 300), (480, 320), (600, 450)];
                let cur = (self.base_w, self.base_h);
                let next = SIZES
                    .iter()
                    .position(|&s| s == cur)
                    .map(|i| (i + 1) % SIZES.len())
                    .unwrap_or(0);
                self.base_w = SIZES[next].0;
                self.base_h = SIZES[next].1;
                self.mark_chart_dirty();
                self.status = format!(
                    "{} {}×{}",
                    i18n::t(self.lang, K::ResizeHint),
                    SIZES[next].0,
                    SIZES[next].1
                );
            }
            // Help ▸ 关于：弹窗显示。打开前先关掉其他弹窗（弹窗互斥），
            // 否则「先点设置、再点帮助」会两层叠在一起。
            Msg::About => {
                self.close_all_overlays();
                self.about_open = true;
            }
            Msg::CloseAbout => self.about_open = false,
            // 关掉居中提示弹窗
            Msg::CloseAlert => self.alert = None,
            // ── 标注（Draw / Write / Text）：真实产物，会画进图里 ────────
            Msg::AddLine => self.add_annotation(crate::model::AnnKind::Line),
            Msg::AddArrow => self.add_annotation(crate::model::AnnKind::Arrow),
            Msg::AddText => self.add_annotation(crate::model::AnnKind::Text),
            Msg::AnnFontUp => {
                for g in &mut self.project.graphs {
                    g.ann_size = (g.ann_size + 1.0).min(48.0);
                    for a in g.annotations.iter_mut() {
                        a.size = g.ann_size;
                    }
                }
                self.mark_chart_dirty();
            }
            Msg::AnnFontDown => {
                for g in &mut self.project.graphs {
                    g.ann_size = (g.ann_size - 1.0).max(6.0);
                    for a in g.annotations.iter_mut() {
                        a.size = g.ann_size;
                    }
                }
                self.mark_chart_dirty();
            }
            Msg::RemoveAnnotation(i) => {
                for g in &mut self.project.graphs {
                    if i < g.annotations.len() {
                        g.annotations.remove(i);
                    }
                }
                self.mark_chart_dirty();
            }
            // ── Change 组：改图型 / 魔棒 ─────────────────────────────────
            Msg::CycleChartKind => {
                for g in &mut self.project.graphs {
                    // 在 62 种图型里循环推进
                    g.kind = ChartKind::from_usize((g.kind.to_usize() + 1) % 62);
                }
                self.mark_chart_dirty();
                self.status = i18n::t(self.lang, K::TipChangeType).to_string();
            }
            // 魔棒：把当前图形的外观套用到本项目所有图形（析光的 Magic 行为）
            Msg::MagicApply => {
                let n = self.project.graphs.len();
                if n > 1 {
                    let src = self.project.graphs[0].clone();
                    for g in self.project.graphs.iter_mut().skip(1) {
                        g.kind = src.kind;
                        g.colormap = src.colormap;
                        g.show_error = src.show_error;
                        g.show_scatter = src.show_scatter;
                        g.show_legend = src.show_legend;
                        g.symbol_follows_bar = src.symbol_follows_bar;
                        g.symbol_color_idx = src.symbol_color_idx;
                        g.symbol_size = src.symbol_size;
                        g.jitter = src.jitter;
                        g.unit_style = src.unit_style;
                        g.use_italic = src.use_italic;
                    }
                    self.mark_chart_dirty();
                    self.status = format!("{} {}", i18n::t(self.lang, K::TipMagic), n);
                }
            }
            // ── Import / Print / Send / LabArchives ──────────────────────
            Msg::ImportCsv => self.import_csv(),
            Msg::PrintChart => self.print_chart(),
            Msg::SendChart => self.send_chart(),
            Msg::ExportBundle => self.export_bundle(),
            Msg::Edit(e) => {
                settings::apply(&mut self.settings_draft, e.clone());
                // 主题 / 强调色 / 字号 / 圆角 / 高对比 这几项立刻生效，
                // 用户不必等到点确认就能看到效果（这是设置窗口的常识）。
                match e {
                    Edit::Theme(_) | Edit::Accent(_) | Edit::FontScale(_)
                    | Edit::Radius(_) | Edit::HighContrast(_) | Edit::A11y(_)
                    | Edit::Opacity(_) | Edit::Animate(_) | Edit::AnimMs(_)
                    | Edit::FontFamily(_) | Edit::CustomFont(_) => {
                        self.settings = self.settings_draft.clone();
                        settings::replace(self.settings.clone());
                        crate::settings::save_debounced();
                        self.apply_theme();
                    }
                    Edit::ShowNavigator(_) | Edit::ShowInspector(_) | Edit::ShowStatus(_)
                    | Edit::NavWidth(_) => {
                        self.settings = self.settings_draft.clone();
                        settings::replace(self.settings.clone());
                        crate::settings::save_debounced();
                    }
                    _ => {}
                }
            }
            Msg::RandomizeSeed => {
                let v = crate::settings::random_seed();
                self.seed = v;
                self.settings_draft.seed = v;
                self.settings.seed = v;
                crate::settings::replace(self.settings.clone());
                crate::settings::save_debounced();
                self.status = format!(
                    "{} {}",
                    i18n::t(self.lang, K::SetSeedRandom),
                    v
                );
            }
            Msg::SetExportBackend(i) => {
                self.export_backend = export::ExportBackend::ALL
                    .get(i)
                    .copied()
                    .unwrap_or(export::ExportBackend::PlottersSvg);
            }
            Msg::ExportPng => {
                // 3.0 倍 → 2400×1800，约等于 300dpi 的投稿尺寸
                match plot::export_png(&self.chart_view(), "spectra_chart.png", 3.0) {
                    Ok(()) => {
                        self.status = format!("{} spectra_chart.png · 3.0×", i18n::t(self.lang, K::Exported));
                    }
                    Err(e) => self.alert(e),
                }
            }
            Msg::OpenExportDialog => {
                // 没有打开的图表就没东西可导。静默返回是「点了没反应」的来源，
                // 必须明确告知。
                if !matches!(self.active, Active::Graph(_)) {
                    self.alert("请先打开一个图表再导出".to_string());
                } else {
                    self.export_open = true;
                    self.dlg_open_at = std::time::Instant::now();
                }
            }
            Msg::SetExportFmt(i) => {
                if i < crate::export::ExportFormat::ALL.len() {
                    self.export_fmt = i;
                }
            }
            Msg::CloseExport => self.export_open = false,
            Msg::OpenRibbonMenu(m, x) => {
                self.ribbon_menu = Some((m, x, std::time::Instant::now()))
            }
            Msg::CloseRibbonMenu => self.ribbon_menu = None,
            Msg::ExportConfirm => self.export_with_dialog(),

            Msg::SelectTab(i) => {
                if let Active::Result(ri) = self.active {
                    if let Some(r) = self.project.results.get_mut(ri) {
                        r.active_tab = i;
                    }
                }
            }
            Msg::OpenSettings => {
                self.alert = None;
                self.dlg_open_at = std::time::Instant::now();
                self.close_all_overlays();
                // 每次打开都把草稿同步为当前值：
                // 否则上次「关了但没确认」的修改会残留到这一次。
                self.settings_draft = self.settings.clone();
                self.settings_open = true;
                self.lang_draft = self.lang;
            }
            Msg::CloseSettings => self.settings_open = false,
            // Settings 菜单的 7 项 = 设置窗口左侧 7 个分类：
            // 点了直接打开窗口并跳到对应页（不是只切 tab）。
            Msg::SettingsTab(i) => {
                self.close_all_overlays();
                self.settings_draft = self.settings.clone();
                self.settings_tab = i;
                self.settings_open = true;
            }
            Msg::SetLangDraft(l) => {
                // 语言即时生效（切完立刻能看懂界面），确认/应用只是关闭窗口
                self.lang_draft = l;
                self.lang = l;
                // 结果页缓存的键含语言，这里重算一次；否则切完语言会一直走
                // 「每帧实时翻译」的降级路径，白白多出几百次查表。
                self.refresh_result_cache();
            }
            Msg::SetTheme(i) => {
                self.settings_draft.theme = i;
                self.theme_mode = i;
                self.dark = crate::theme::resolve_dark(i);
                crate::palette::set_dark(self.dark);
                self.settings.theme = i;
                settings::replace(self.settings.clone());
            }
            Msg::SettingsOk => {
                self.settings = self.settings_draft.clone();
                settings::replace(self.settings.clone());
                let _ = self.settings.save();
                self.apply_theme();
                self.apply_settings();
                self.settings_open = false;
            }
            Msg::SettingsApply => {
                self.settings = self.settings_draft.clone();
                settings::replace(self.settings.clone());
                let _ = self.settings.save();
                self.apply_theme();
                self.apply_settings();
                self.lang = self.lang_draft;
                self.refresh_result_cache();
            }
            Msg::GoTo(s) => {
                self.goto = s.clone();
                // 「跳到行」输入框：直达任意行，行窗口按需增长（真正的无限）
                if let Ok(n) = s.trim().parse::<usize>() {
                    if n > 0 {
                        self.move_cursor(self.cursor.0, n - 1);
                    }
                }
            }
            Msg::Quit => std::process::exit(0),
        }
        // 功能区下拉菜单：除「打开菜单」本身，任何动作之后都收起来 ——
        // 选了菜单项、点了别处、按了快捷键，菜单都不该继续挂着。
        if !keep_menu {
            self.ribbon_menu = None;
        }
    }

    fn view(&self) -> Element<Msg> {
        let root = col()
            .size_full()
            .bg(hex(window_bg()))
            // 全局键盘兜底：Ctrl+C/V/X/Z/Y 等组合键**无论焦点在哪**都要生效。
            // 只转发组合键，不抢普通打字。见 ui::grid::global_key_hook。
            .focusable()
            .on_key_capture(crate::ui::grid::global_key_hook)
            // 界面字体：Mi Sans（打包在 assets/fonts 里）。
            // 若系统里看不到这个字体导致方块字，把 fonts::APPLY_UI_FONT 改成 false。
            .when(crate::fonts::APPLY_UI_FONT, |e| {
                e.font(FontFamily::Named(crate::fonts::UI_FAMILY.to_string()))
            })
            // 菜单栏
            .child(menubar(self.menu()).h(MENU_H).px(0.0))
            // 主工具栏（16 组）—— View 菜单可关
            .child_if(self.main_toolbar, || {
                ui::toolbar::toolbar(self.lang, self.active_tool)
            })
            ;
        // 工作区：导航器 | 中央 | 检查器
        // `w_full` + `min_w(0)` 必须成对出现。
        //
        // flex 子项默认 min-width:auto —— 内容多宽容器就多宽。表格列一多
        // （粘贴一大片、或往右扩到第 40 列），这一整行会被撑得比窗口还宽，
        // 于是覆盖层 `size_full().center()` 居中到了「右边栏再往右」的位置，
        // 表现就是弹窗跑到屏幕外，得点一下才滚回来。
        // min_w(0) 让它可以收缩回窗口宽度，滚动交给内部 scroll_both。
        let body = row()
            .flex1()
            .w_full()
            .min_w(0.0)
            // 导航器 —— View 菜单可关
            .child_if(self.navigator, || ui::navigator::navigator(self))
            .child(self.workspace())
            // 检查器按当前工作表类型分派：
            // 数据表 → 表结构 / 统计 / 分析；图表 → 绘图配置。
            // 表格界面不再出现任何绘图选项。
            .child_if(self.show_inspector, || {
                let is_graph = matches!(self.active, Active::Graph(_));
                let names = self.group_names(match self.active {
                    Active::Graph(i) => i,
                    _ => 0,
                });
                ui::inspector::inspector(
                    self.lang,
                    is_graph,
                    self.project.tables.get(self.table_index()),
                    self.project.graphs.get(match self.active {
                        Active::Graph(i) => i,
                        _ => 0,
                    }),
                    &names,
                    self.export_backend,
                    self.analysis,
                    self.welch,
                    self.base_h as f32,
                    self.reverse_legend,
                )
            })
            ;
        // 覆盖层只挂在**工作区**上，不遮菜单栏 —— 否则弹窗一开，
        // 菜单就点不动了（「先点设置、再点帮助」会失效）。
        let body = if self.about_open {
            div()
                .size_full()
                .child(body)
                .child(ui::about::about_dialog(self.lang, self.dlg_open_at))
        } else if self.settings_open {
            div()
                .size_full()
                .child(body)
                .child(ui::settings::settings_overlay(
                    self.lang,
                    self.settings_tab,
                    &self.settings_draft,
                    self.export_backend,
                    self.dlg_open_at,
                ))
        } else if self.welcome_open {
            div().size_full().child(body).child(ui::welcome::welcome(self))
        } else if self.export_open {
            div()
                .size_full()
                .child(body)
                .child(ui::export::export_dialog(self.lang, self.export_fmt, self.dlg_open_at))
        } else {
            body
        };
        // 居中提示弹窗：与欢迎/设置/关于互斥 —— 后来者顶掉前者。
        // 只有重要信息走这里，例行反馈放底部状态栏。
        let body = if let Some(msg) = self.alert.clone() {
            div().size_full().child(body).child(ui::alert::alert_dialog(self.lang, msg, self.dlg_open_at))
        } else {
            body
        };
        // 底部两行：工具栏（Go to / 翻页）+ 状态栏（光标位置、最近一次操作反馈）。
        // ★ 状态栏必须挂在这里 —— 之前它整个没被接进视图树，于是「已粘贴」
        //   「没有可写入的内容」这类反馈全都没有出口，用户只能看到「没反应」。
        let root = root
            .child(body)
            .child_if(self.bottom_toolbar, || ui::status::bottom_bar(self))
            .child_if(self.bottom_toolbar, || ui::status::status_bar(self));
        // 功能区下拉菜单挂**窗口根**：要盖住下方内容必须绝对定位，
        // 挂在工具栏内部会被卡片裁掉，挂在 body 上又会挤窄工作区。
        let root = if let Some((m, x, at)) = self.ribbon_menu {
            root.child(ui::toolbar::ribbon_menu_popup(self.lang, m, x, at))
        } else {
            root
        };

        // 右键上下文菜单必须挂在**窗口根**上，不能挂在 body 上。
        //
        // `on_context_menu` 给出的指针位置是窗口坐标，而菜单用
        // `absolute().left(x).top(y)` 定位 —— 这个坐标是相对**最近的容器**算的。
        // 之前菜单是 body 的子层，body 的顶边在菜单栏（+主工具栏）之下，
        // 于是同一个 y 被整体下移了菜单栏+工具栏的高度，表现就是
        // 「菜单离鼠标很远」。挂到 root 之后原点与窗口重合，位置才对得上。
        let root = if let Some(m) = self.ctx_menu {
            div()
                .size_full()
                .child(root)
                .child(ui::context::context_menu(self.lang, m))
        } else {
            root
        };

        root
    }

    /// 主题：冷蓝调灰（Slate）+ 析光蓝强调色 + 近方角 + 紧凑密度。
    ///
    /// 关键：`titlebar_height` 归零。charis 按「应用自绘标题栏」预留了一段顶部空间，
    /// 而我们用的是 OS 原生标题栏（frameless=false），那段预留就会变成一条空白。
    fn theme(&self) -> Theme {
        let mut t = Theme::from_config(ThemeConfig {
            accent: hex(accent()),
            gray: GrayTint::Slate,
            radius: 2.0,
            ..if self.dark {
                ThemeConfig::dark()
            } else {
                ThemeConfig::light()
            }
        });
        // 顶部空白的根因：不画自绘标题栏，就别预留它的高度。
        t.titlebar_height = 0.0;
        t
    }

    /// 全局快捷键（框架级）。
    ///
    /// 为什么必须走 `commands()`：我们自己挂的 `on_key_capture` 只在
    /// 「该元素或其后代持有焦点」时才运行，焦点一旦跑到别处（点过检查器、
    /// 刚切回表格页、弹窗刚关闭），Ctrl+V 就静默失效 —— 表现成
    /// 「右键能粘贴，Ctrl+V 不行」。
    ///
    /// `Command::key` 由框架在窗口层派发，与焦点无关，因此始终生效。
    /// Mod = Windows/Linux 上的 Ctrl、macOS 上的 Cmd。
    /// 窗口级按键钩子（App trait 的 provided method）。
    ///
    /// **为什么还需要它**：`on_key_capture` 挂在元素上，只在「该元素或其
    /// 后代持有焦点」时才运行。焦点一旦跑到检查器输入框、刚切回表格页、
    /// 或弹窗刚关闭，Ctrl+V 就静默失效 —— 表现成「右键能粘贴，Ctrl+V 不行」。
    /// `on_key` 由框架在窗口层调用，与焦点无关。
    ///
    /// 只转发组合键，普通打字一律放过，绝不干扰任何输入框。
    fn on_key(&self, e: &KeyEvent) -> Option<Msg> {
        // 正在编辑单元格：把按键让给输入框（Ctrl+V 应粘贴进文本本身）
        if self.editing {
            return None;
        }
        if !matches!(self.active, Active::Data(_)) {
            return None;
        }
        if e.mods.ctrl || e.mods.alt {
            Some(Msg::GridKey(e.clone()))
        } else {
            None
        }
    }

    fn commands(&self) -> charis_ui::commands::Commands<Msg> {
        use charis_ui::commands::{Command, Commands};
        // 只在数据表页注册编辑类快捷键：在图形页 / 结果页按 Ctrl+V 不该去改单元格。
        //
        // 编辑态（双击或单击进入输入）时**仍然注册**：charis 的文本输入框没有
        // 自带的系统剪贴板粘贴，注销掉就变成「Ctrl+V 完全没反应」。
        // 具体往哪儿写由 paste_cell 自己判断（编辑态写进输入框，否则写进单元格）。
        if !matches!(self.active, Active::Data(_)) {
            return Commands::new(Vec::new());
        }
        Commands::new(vec![
            Command::new("edit.copy", "", Msg::CopyCell).key("Mod+C"),
            Command::new("edit.cut", "", Msg::CutCell).key("Mod+X"),
            Command::new("edit.paste", "", Msg::PasteCell).key("Mod+V"),
            Command::new("edit.undo", "", Msg::DoUndo).key("Mod+Z"),
            Command::new("edit.redo", "", Msg::DoRedo).key("Mod+Shift+Z"),
            Command::new("edit.select_all", "", Msg::SelectAll).key("Mod+A"),
            Command::new("edit.clear", "", Msg::ClearCells).key("Delete"),
        ])
    }

    fn menu(&self) -> Vec<Menu<Msg>> {
        // Analyze 菜单按当前选中表的类型动态生成
        let methods: Vec<(usize, &'static str, &'static str)> = match self.active {
            Active::Data(i) => self
                .project
                .tables
                .get(i)
                .map(|t| t.table.table_type.analyses().to_vec())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        ui::menubar::menus(self.lang, &self.menu_state(), &methods)
    }

}

/// 欢迎对话框选中的表类型 → TableType。
/// `pick_table` 是 TABLE_KINDS 的下标；选中 Existing file 时回落成 XY。
fn table_type_of_pick(pick: usize) -> TableType {
    match pick {
        0 => TableType::XY,
        1 => TableType::Column,
        2 => TableType::Grouped,
        3 => TableType::Contingency,
        4 => TableType::Survival,
        5 => TableType::PartsOfWhole,
        6 => TableType::MultipleVariables,
        7 => TableType::Nested,
        _ => TableType::XY,
    }
}

/// 新建一张表：完全由工厂按类型生成模板（与析光新建流程一致）。
fn new_table(name: &str, tt: TableType) -> DataTable {
    DataTable::new_by_type(name, tt, default_subcols(tt))
}

/// 各类型的默认子列数（并排 = 重复值个数，堆叠 = 内层单元个数）。
fn default_subcols(tt: TableType) -> usize {
    match tt {
        TableType::XY | TableType::Grouped => 3,
        TableType::Nested => 3,
        _ => 0,
    }
}

/// 用系统默认程序打开一个文件或 URL。
fn open_with_default(target: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", target])
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(target).spawn()?;
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(target).spawn()?;
    }
    Ok(())
}

/// mailto 用的极简百分号编码。
fn urlencode(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => o.push(b as char),
            _ => o.push_str(&format!("%{:02X}", b)),
        }
    }
    o
}
