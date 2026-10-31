//! 图表类型体系（62 种）。
//!
//! # 设计要点
//!
//! 62 种图型如果每种都写一套绘制代码，会产生大量重复。这里用
//! **「渲染族（Family）+ 参数化旗标」** 两层结构收敛：
//!
//! * [`ChartKind::family`] 决定走哪一套绘制核心（共 14 套）；
//! * [`ChartKind::stacked`] / [`horizontal`] / [`smooth`] / [`step`] / [`area`]
//!   等旗标在同一套核心内切换行为。
//!
//! 因此 `StackedBar` 不是独立绘制分支，而是 `Bar` 核心 + `stacked` 旗标；
//! `StepLine` 是 `Line` 核心 + `step` 旗标。新增图型只要能归入现有族，
//! 就不需要写新的绘制代码。
//!
//! # 向后兼容
//!
//! 前 10 个变体的判别值（0..=9）与旧版完全一致，已保存的 `.spectra`
//! 工程文件打开后图型不会错位。

/// 图表类型：62 种。
///
/// 判别值即存盘索引，见 [`ChartKind::to_usize`] / [`ChartKind::from_usize`]。
#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(usize)]
pub enum ChartKind {
    // ── 0..=9：旧版已有的十种（判别值不可变动） ──────────────────────────
    /// 柱状图：均值 + 误差棒 + 可选的柱形散点
    Bar = 0,
    /// 散点图：每个重复值一个点
    Scatter = 1,
    /// 折线图：组内数值按序连线
    Line = 2,
    /// 箱线图：中位数 / 四分位 / 须
    Box = 3,
    /// 小提琴图：核密度估计的对称轮廓
    Violin = 4,
    /// 直方图：按分箱统计频数
    Histogram = 5,
    /// 饼图（内径 > 0 即环形）
    Pie = 6,
    /// 热图：组 × 分箱的频数矩阵
    Heatmap = 7,
    /// 森林图：每组效应量 ± CI，带参考线
    Forest = 8,
    /// 生存曲线：Kaplan-Meier 阶梯
    Survival = 9,

    // ── 10..=19：折线与曲线 ──────────────────────────────────────────────
    /// 平滑曲线（Catmull-Rom 插值）
    Curve = 10,
    /// 只画线不画点
    LineOnly = 11,
    /// 拟合曲线（多项式，阶数取 `bins` 字段，默认 2）
    FitCurve = 12,
    /// LOWESS 式局部平滑
    SmoothCurve = 13,
    /// 面积填充：折线下方到基线
    AreaFill = 14,
    /// 两线之间填充（首末两组）
    AreaBetween = 15,
    /// 峰填充：只填充高于基线的部分
    PeakFill = 16,
    /// AUC 填充：填充曲线下方面积
    AucFill = 17,
    /// 阶梯线
    StepLine = 18,
    /// 多序列折线
    MultiLine = 19,

    // ── 20..=30：柱与柱的变体 ────────────────────────────────────────────
    /// 分组柱状图（并排）
    GroupedBar = 20,
    /// 堆叠柱状图
    StackedBar = 21,
    /// 100% 堆叠柱状图
    StackedBar100 = 22,
    /// 横向柱状图
    BarHorizontal = 23,
    /// 棒棒糖图：细杆 + 顶端圆点
    Lollipop = 24,
    /// Cleveland 点图：只画点不画杆
    ClevelandDot = 25,
    /// 点图（Wilkinson dot plot）：按值分箱堆叠
    DotPlot = 26,
    /// 列散点图：柱形轮廓 + 内部散点
    ColumnScatter = 27,
    /// 柱 + 散点叠加
    ScatterWithBar = 28,
    /// 多变量柱状图
    MultiBar = 29,
    /// 嵌套柱状图
    NestedBar = 30,

    // ── 31..=38：箱线 / 小提琴 ───────────────────────────────────────────
    /// 标准箱线图（含 1.5×IQR 须）
    BoxWhisker = 31,
    /// 浮动箱线图（箱体按数据密度浮动）
    BoxFloating = 32,
    /// 浮动小提琴图
    ViolinFloating = 33,
    /// 分组箱线图
    GroupedBoxPlot = 34,
    /// 分组小提琴图
    GroupedViolin = 35,
    /// 多变量箱线图
    MultiBoxPlot = 36,
    /// 多变量小提琴图
    MultiViolin = 37,
    /// 嵌套箱线图
    NestedBoxPlot = 38,

    // ── 39..=44：散点家族 ────────────────────────────────────────────────
    /// 分组散点图
    GroupedScatter = 39,
    /// 象限散点图（十字分割 + 象限计数）
    ScatterQuadrant = 40,
    /// 火山图：-log10(p) vs 效应量
    Volcano = 41,
    /// 嵌套点图
    NestedPoint = 42,
    /// 风险人数表（生存分析配套）
    NumberAtRisk = 43,
    /// 回归图：散点 + 拟合线 + 置信带
    RegressionPlot = 44,

    // ── 45..=52：前后对比与构成 ──────────────────────────────────────────
    /// 前后对比图（配对连线）
    BeforeAfter = 45,
    /// 分组前后对比图
    GroupedBeforeAfter = 46,
    /// 百分比点阵图（华夫图）
    PercentageDot = 47,
    /// 马赛克图（列联表）
    Mosaic = 48,
    /// 卡方关联图（偏差阴影）
    ChiSquareAssoc = 49,
    /// 堆叠条（列联表）
    StackedBarContingency = 50,
    /// 堆叠条（整体分解）
    StackedBarParts = 51,
    /// 环形图
    Donut = 52,

    // ── 53..=61：分面与矩阵 ──────────────────────────────────────────────
    /// 分面图（网格小倍数）
    Facet = 53,
    /// 三因素图
    ThreeWay = 54,
    /// 双 Y 轴图
    DualYAxis = 55,
    /// 深度图（反向纵轴）
    DepthChart = 56,
    /// 相关矩阵
    CorrMatrix = 57,
    /// 散点矩阵
    ScatterMatrix = 58,
    /// 多变量热图
    MultiHeatmap = 59,
    /// 嵌套散点图
    NestedScatter = 60,
    /// 子列柱状图
    SubcolumnBar = 61,
    // ── 62..：由 plotters-statistical 提供的高级统计图 ──────────────────
    /// 经验累积分布函数（ECDF）：可切生存函数、可加 DKW 置信带
    Ecdf = 62,
    /// 正态 QQ 图：分位数对分位数，带稳健参考线
    QqPlot = 63,
    Waffle = 64,
    Dumbbell = 65,
    Ridgeline = 66,
    Radar = 67,
    Contour = 68,
    Residual = 69,
}

/// 渲染族：决定走哪一套绘制核心。
///
/// 62 种图型收敛到 14 个族，族内差异由旗标（stacked / horizontal / …）区分。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// 柱（含堆叠 / 横向 / 棒棒糖等所有柱形变体）
    Bar,
    /// 散点
    Scatter,
    /// 折线与曲线（含平滑 / 阶梯 / 面积填充）
    Line,
    /// 箱线
    Box,
    /// 小提琴
    Violin,
    /// 直方图
    Histogram,
    /// 饼 / 环
    Pie,
    /// 热图与矩阵
    Heatmap,
    /// 森林图
    Forest,
    /// 生存曲线
    Survival,
    /// 前后对比（配对连线）
    BeforeAfter,
    /// 列联（马赛克 / 关联阴影 / 堆叠条）
    Contingency,
    /// 分面（小倍数网格）
    Facet,
    /// 风险人数表
    NumberAtRisk,
    /// 哑铃图：两点一线，表示区间/前后变化
    Dumbbell,
    /// 山脊图：多个密度曲线纵向叠放
    Ridgeline,
    /// 雷达图：多变量极坐标
    Radar,
    /// 华夫图：10×10 格子表示百分比
    Waffle,
    /// 等高线：二维密度/响应面
    Contour,
}

impl ChartKind {
    /// 存盘索引。前 10 个与旧版一致，保证老工程文件不失效。
    pub fn to_usize(self) -> usize {
        self as usize
    }

    /// 由存盘索引还原。越界回落 `Bar`。
    pub fn from_usize(v: usize) -> Self {
        match v {
            0 => ChartKind::Bar,
            1 => ChartKind::Scatter,
            2 => ChartKind::Line,
            3 => ChartKind::Box,
            4 => ChartKind::Violin,
            5 => ChartKind::Histogram,
            6 => ChartKind::Pie,
            7 => ChartKind::Heatmap,
            8 => ChartKind::Forest,
            9 => ChartKind::Survival,
            10 => ChartKind::Curve,
            11 => ChartKind::LineOnly,
            12 => ChartKind::FitCurve,
            13 => ChartKind::SmoothCurve,
            14 => ChartKind::AreaFill,
            15 => ChartKind::AreaBetween,
            16 => ChartKind::PeakFill,
            17 => ChartKind::AucFill,
            18 => ChartKind::StepLine,
            19 => ChartKind::MultiLine,
            20 => ChartKind::GroupedBar,
            21 => ChartKind::StackedBar,
            22 => ChartKind::StackedBar100,
            23 => ChartKind::BarHorizontal,
            24 => ChartKind::Lollipop,
            25 => ChartKind::ClevelandDot,
            26 => ChartKind::DotPlot,
            27 => ChartKind::ColumnScatter,
            28 => ChartKind::ScatterWithBar,
            29 => ChartKind::MultiBar,
            30 => ChartKind::NestedBar,
            31 => ChartKind::BoxWhisker,
            32 => ChartKind::BoxFloating,
            33 => ChartKind::ViolinFloating,
            34 => ChartKind::GroupedBoxPlot,
            35 => ChartKind::GroupedViolin,
            36 => ChartKind::MultiBoxPlot,
            37 => ChartKind::MultiViolin,
            38 => ChartKind::NestedBoxPlot,
            39 => ChartKind::GroupedScatter,
            40 => ChartKind::ScatterQuadrant,
            41 => ChartKind::Volcano,
            42 => ChartKind::NestedPoint,
            43 => ChartKind::NumberAtRisk,
            62 => ChartKind::Ecdf,
            63 => ChartKind::QqPlot,
            64 => ChartKind::Waffle,
            65 => ChartKind::Dumbbell,
            66 => ChartKind::Ridgeline,
            67 => ChartKind::Radar,
            68 => ChartKind::Contour,
            69 => ChartKind::Residual,
            44 => ChartKind::RegressionPlot,
            45 => ChartKind::BeforeAfter,
            46 => ChartKind::GroupedBeforeAfter,
            47 => ChartKind::PercentageDot,
            48 => ChartKind::Mosaic,
            49 => ChartKind::ChiSquareAssoc,
            50 => ChartKind::StackedBarContingency,
            51 => ChartKind::StackedBarParts,
            52 => ChartKind::Donut,
            53 => ChartKind::Facet,
            54 => ChartKind::ThreeWay,
            55 => ChartKind::DualYAxis,
            56 => ChartKind::DepthChart,
            57 => ChartKind::CorrMatrix,
            58 => ChartKind::ScatterMatrix,
            59 => ChartKind::MultiHeatmap,
            60 => ChartKind::NestedScatter,
            61 => ChartKind::SubcolumnBar,
            _ => ChartKind::Bar,
        }
    }

    /// 由英文图型名反查。用于读 `.sprx` / `.prism` 时把 `graph_kind`
    /// 字符串还原成枚举；认不出返回 `None`，调用方自行回落。
    ///
    /// 先扫 `all()`，再按判别值兜一遍 —— `all()` 只覆盖下拉框里列出的那些，
    /// 而新增的图型（华夫、等高线…）可能还没进清单。
    pub fn from_label(s: &str) -> Option<Self> {
        for k in Self::all() {
            if k.label().eq_ignore_ascii_case(s) {
                return Some(k);
            }
        }
        for v in 0..128usize {
            let k = Self::from_usize(v);
            if k.label().eq_ignore_ascii_case(s) {
                return Some(k);
            }
        }
        None
    }

    /// 全量清单（供下拉框遍历）。
    pub fn all() -> [ChartKind; 64] {
        [
            ChartKind::Bar,
            ChartKind::Scatter,
            ChartKind::Line,
            ChartKind::Box,
            ChartKind::Violin,
            ChartKind::Histogram,
            ChartKind::Pie,
            ChartKind::Heatmap,
            ChartKind::Forest,
            ChartKind::Survival,
            ChartKind::Curve,
            ChartKind::LineOnly,
            ChartKind::FitCurve,
            ChartKind::SmoothCurve,
            ChartKind::AreaFill,
            ChartKind::AreaBetween,
            ChartKind::PeakFill,
            ChartKind::AucFill,
            ChartKind::StepLine,
            ChartKind::MultiLine,
            ChartKind::GroupedBar,
            ChartKind::StackedBar,
            ChartKind::StackedBar100,
            ChartKind::BarHorizontal,
            ChartKind::Lollipop,
            ChartKind::ClevelandDot,
            ChartKind::DotPlot,
            ChartKind::ColumnScatter,
            ChartKind::ScatterWithBar,
            ChartKind::MultiBar,
            ChartKind::NestedBar,
            ChartKind::BoxWhisker,
            ChartKind::BoxFloating,
            ChartKind::ViolinFloating,
            ChartKind::GroupedBoxPlot,
            ChartKind::GroupedViolin,
            ChartKind::MultiBoxPlot,
            ChartKind::MultiViolin,
            ChartKind::NestedBoxPlot,
            ChartKind::GroupedScatter,
            ChartKind::ScatterQuadrant,
            ChartKind::Volcano,
            ChartKind::NestedPoint,
            ChartKind::NumberAtRisk,
            ChartKind::RegressionPlot,
            ChartKind::BeforeAfter,
            ChartKind::GroupedBeforeAfter,
            ChartKind::PercentageDot,
            ChartKind::Mosaic,
            ChartKind::ChiSquareAssoc,
            ChartKind::StackedBarContingency,
            ChartKind::StackedBarParts,
            ChartKind::Donut,
            ChartKind::Facet,
            ChartKind::ThreeWay,
            ChartKind::DualYAxis,
            ChartKind::DepthChart,
            ChartKind::CorrMatrix,
            ChartKind::ScatterMatrix,
            ChartKind::MultiHeatmap,
            ChartKind::NestedScatter,
            ChartKind::SubcolumnBar,
            ChartKind::Ecdf,
            ChartKind::QqPlot,
        ]
    }

    /// 渲染族。绘制代码按这个分派，而不是按 62 个变体分派。
    pub fn family(self) -> Family {
        match self {
            // 柱与柱的变体
            ChartKind::Bar
            | ChartKind::GroupedBar
            | ChartKind::StackedBar
            | ChartKind::StackedBar100
            | ChartKind::BarHorizontal
            | ChartKind::Lollipop
            | ChartKind::ClevelandDot
            | ChartKind::DotPlot
            | ChartKind::ColumnScatter
            | ChartKind::ScatterWithBar
            | ChartKind::MultiBar
            | ChartKind::NestedBar
            | ChartKind::SubcolumnBar
            | ChartKind::StackedBarContingency
            | ChartKind::StackedBarParts
            | ChartKind::PercentageDot => Family::Bar,

            // 散点
            ChartKind::Scatter
            | ChartKind::GroupedScatter
            | ChartKind::ScatterQuadrant
            | ChartKind::Volcano
            | ChartKind::NestedPoint
            | ChartKind::NestedScatter
            | ChartKind::RegressionPlot => Family::Scatter,

            // 折线与曲线
            ChartKind::Line
            | ChartKind::Curve
            | ChartKind::LineOnly
            | ChartKind::FitCurve
            | ChartKind::SmoothCurve
            | ChartKind::AreaFill
            | ChartKind::AreaBetween
            | ChartKind::PeakFill
            | ChartKind::AucFill
            | ChartKind::StepLine
            | ChartKind::MultiLine
            | ChartKind::DualYAxis
            | ChartKind::DepthChart => Family::Line,

            ChartKind::Box
            | ChartKind::BoxWhisker
            | ChartKind::BoxFloating
            | ChartKind::GroupedBoxPlot
            | ChartKind::MultiBoxPlot
            | ChartKind::NestedBoxPlot => Family::Box,

            ChartKind::Violin
            | ChartKind::ViolinFloating
            | ChartKind::GroupedViolin
            | ChartKind::MultiViolin => Family::Violin,

            ChartKind::Histogram => Family::Histogram,
            ChartKind::Pie | ChartKind::Donut => Family::Pie,
            ChartKind::Heatmap | ChartKind::CorrMatrix | ChartKind::ScatterMatrix
            | ChartKind::MultiHeatmap => Family::Heatmap,
            ChartKind::Forest => Family::Forest,
            ChartKind::Survival => Family::Survival,
            ChartKind::BeforeAfter | ChartKind::GroupedBeforeAfter => Family::BeforeAfter,
            ChartKind::Mosaic | ChartKind::ChiSquareAssoc => Family::Contingency,
            ChartKind::Facet | ChartKind::ThreeWay => Family::Facet,
            ChartKind::NumberAtRisk => Family::NumberAtRisk,
            ChartKind::Waffle => Family::Waffle,
            ChartKind::Dumbbell => Family::Dumbbell,
            ChartKind::Ridgeline => Family::Ridgeline,
            ChartKind::Radar => Family::Radar,
            ChartKind::Contour => Family::Contour,
            // 残差图：横轴拟合值（组内均值）、纵轴残差，语义上就是散点
            ChartKind::Residual => Family::Scatter,
            // 高级统计图不参与族分派：draw_chart 开头就提前返回给 advplot，
            // 这里归到 Line 只是为了满足穷尽性检查。
            ChartKind::Ecdf | ChartKind::QqPlot => Family::Line,
        }
    }

    /// 分类（下拉框里的分组标题索引）。
    ///
    /// 0 散点 / 1 折线与曲线 / 2 面积与填充 / 3 柱 / 4 箱线 /
    /// 5 小提琴 / 6 前后对比 / 7 点图 / 8 热图与矩阵 /
    /// 9 分面与三因素 / 10 关联与马赛克 / 11 生存 / 12 饼与环 / 13 森林
    pub fn category(self) -> usize {
        match self {
            ChartKind::Scatter | ChartKind::GroupedScatter | ChartKind::ScatterQuadrant
            | ChartKind::Volcano | ChartKind::NestedScatter | ChartKind::RegressionPlot => 0,

            ChartKind::Line
            | ChartKind::Curve
            | ChartKind::LineOnly
            | ChartKind::FitCurve
            | ChartKind::SmoothCurve
            | ChartKind::StepLine
            | ChartKind::MultiLine
            | ChartKind::DualYAxis
            | ChartKind::DepthChart
            | ChartKind::NestedPoint
            | ChartKind::Ecdf
            | ChartKind::QqPlot => 1,

            ChartKind::AreaFill | ChartKind::AreaBetween | ChartKind::PeakFill
            | ChartKind::AucFill => 2,

            ChartKind::Bar
            | ChartKind::GroupedBar
            | ChartKind::StackedBar
            | ChartKind::StackedBar100
            | ChartKind::BarHorizontal
            | ChartKind::MultiBar
            | ChartKind::NestedBar
            | ChartKind::SubcolumnBar
            | ChartKind::StackedBarContingency
            | ChartKind::StackedBarParts => 3,

            ChartKind::Box
            | ChartKind::BoxWhisker
            | ChartKind::BoxFloating
            | ChartKind::GroupedBoxPlot
            | ChartKind::MultiBoxPlot
            | ChartKind::NestedBoxPlot => 4,

            ChartKind::Violin | ChartKind::ViolinFloating | ChartKind::GroupedViolin
            | ChartKind::MultiViolin => 5,

            ChartKind::BeforeAfter | ChartKind::GroupedBeforeAfter => 6,

            ChartKind::Lollipop
            | ChartKind::ClevelandDot
            | ChartKind::DotPlot
            | ChartKind::ColumnScatter
            | ChartKind::ScatterWithBar
            | ChartKind::PercentageDot
            | ChartKind::Histogram => 7,

            ChartKind::Heatmap
            | ChartKind::CorrMatrix
            | ChartKind::ScatterMatrix
            | ChartKind::MultiHeatmap => 8,

            ChartKind::Facet | ChartKind::ThreeWay => 9,

            ChartKind::Mosaic | ChartKind::ChiSquareAssoc => 10,

            ChartKind::Survival | ChartKind::NumberAtRisk => 11,

            ChartKind::Pie | ChartKind::Donut => 12,

            ChartKind::Forest => 13,

            ChartKind::Waffle => 14,

            ChartKind::Dumbbell => 15,

            ChartKind::Ridgeline => 16,

            ChartKind::Radar => 17,

            ChartKind::Contour => 18,

            ChartKind::Residual => 19,
        }
    }

    /// 分类名（英文）。
    pub fn category_name(c: usize) -> &'static str {
        match c {
            0 => "Scatter",
            1 => "Lines and curves",
            2 => "Area and fill",
            3 => "Bars",
            4 => "Box and whiskers",
            5 => "Violin",
            6 => "Before-after",
            7 => "Dot plots",
            8 => "Heat map and matrices",
            9 => "Facet and three-way",
            10 => "Association and mosaic",
            11 => "Survival",
            12 => "Pie and donut",
            13 => "Forest",
            _ => "Other",
        }
    }

    /// 分类名（中文）。
    pub fn category_name_zh(c: usize) -> &'static str {
        match c {
            0 => "散点图",
            1 => "折线与曲线",
            2 => "面积与填充",
            3 => "柱状图",
            4 => "箱线图",
            5 => "小提琴图",
            6 => "前后对比",
            7 => "点图",
            8 => "热图与矩阵",
            9 => "分面与三因素",
            10 => "关联与马赛克",
            11 => "生存分析",
            12 => "饼图与环形图",
            13 => "森林图",
            _ => "其他",
        }
    }

    /// 图型名（英文，供 TikZ / matplotlib 导出注释）。
    pub fn label(self) -> &'static str {
        match self {
            ChartKind::Bar => "Bar",
            ChartKind::Scatter => "Scatter",
            ChartKind::Line => "Line",
            ChartKind::Box => "Box",
            ChartKind::Violin => "Violin",
            ChartKind::Histogram => "Histogram",
            ChartKind::Pie => "Pie",
            ChartKind::Heatmap => "Heat map",
            ChartKind::Forest => "Forest plot",
            ChartKind::Survival => "Survival (KM)",
            ChartKind::Curve => "Curve",
            ChartKind::LineOnly => "Line only",
            ChartKind::FitCurve => "Fitted curve",
            ChartKind::SmoothCurve => "Smoothed curve",
            ChartKind::AreaFill => "Area fill",
            ChartKind::AreaBetween => "Area between",
            ChartKind::PeakFill => "Peak fill",
            ChartKind::AucFill => "AUC fill",
            ChartKind::StepLine => "Step line",
            ChartKind::MultiLine => "Multiple lines",
            ChartKind::GroupedBar => "Grouped bar",
            ChartKind::StackedBar => "Stacked bar",
            ChartKind::StackedBar100 => "100% stacked bar",
            ChartKind::BarHorizontal => "Horizontal bar",
            ChartKind::Lollipop => "Lollipop",
            ChartKind::ClevelandDot => "Cleveland dot",
            ChartKind::DotPlot => "Dot plot",
            ChartKind::ColumnScatter => "Column scatter",
            ChartKind::ScatterWithBar => "Scatter with bar",
            ChartKind::MultiBar => "Multiple bar",
            ChartKind::NestedBar => "Nested bar",
            ChartKind::BoxWhisker => "Box & whisker",
            ChartKind::BoxFloating => "Floating box",
            ChartKind::ViolinFloating => "Floating violin",
            ChartKind::GroupedBoxPlot => "Grouped box",
            ChartKind::GroupedViolin => "Grouped violin",
            ChartKind::MultiBoxPlot => "Multiple box",
            ChartKind::MultiViolin => "Multiple violin",
            ChartKind::NestedBoxPlot => "Nested box",
            ChartKind::GroupedScatter => "Grouped scatter",
            ChartKind::ScatterQuadrant => "Quadrant scatter",
            ChartKind::Volcano => "Volcano",
            ChartKind::NestedPoint => "Nested point",
            ChartKind::NumberAtRisk => "Number at risk",
            ChartKind::Ecdf => "ECDF",
            ChartKind::QqPlot => "Q-Q plot",
            ChartKind::RegressionPlot => "Regression",
            ChartKind::BeforeAfter => "Before-after",
            ChartKind::GroupedBeforeAfter => "Grouped before-after",
            ChartKind::PercentageDot => "Percentage dot",
            ChartKind::Mosaic => "Mosaic",
            ChartKind::ChiSquareAssoc => "Chi-square association",
            ChartKind::StackedBarContingency => "Stacked bar (contingency)",
            ChartKind::StackedBarParts => "Stacked bar (parts)",
            ChartKind::Donut => "Donut",
            ChartKind::Facet => "Facet",
            ChartKind::ThreeWay => "Three-way",
            ChartKind::DualYAxis => "Dual Y axis",
            ChartKind::DepthChart => "Depth chart",
            ChartKind::CorrMatrix => "Correlation matrix",
            ChartKind::ScatterMatrix => "Scatter matrix",
            ChartKind::MultiHeatmap => "Multiple heat map",
            ChartKind::NestedScatter => "Nested scatter",
            ChartKind::SubcolumnBar => "Subcolumn bar",
            ChartKind::Waffle => "Waffle chart",
            ChartKind::Dumbbell => "Dumbbell plot",
            ChartKind::Ridgeline => "Ridgeline plot",
            ChartKind::Radar => "Radar chart",
            ChartKind::Contour => "Contour plot",
            ChartKind::Residual => "Residual plot",
        }
    }

    /// 图型名（中文）。
    pub fn label_zh(self) -> &'static str {
        match self {
            ChartKind::Bar => "柱状图",
            ChartKind::Scatter => "散点图",
            ChartKind::Line => "折线图",
            ChartKind::Box => "箱线图",
            ChartKind::Violin => "小提琴图",
            ChartKind::Histogram => "直方图",
            ChartKind::Pie => "饼图",
            ChartKind::Heatmap => "热图",
            ChartKind::Forest => "森林图",
            ChartKind::Survival => "生存曲线",
            ChartKind::Curve => "平滑曲线",
            ChartKind::LineOnly => "纯折线",
            ChartKind::FitCurve => "拟合曲线",
            ChartKind::SmoothCurve => "局部平滑曲线",
            ChartKind::AreaFill => "面积填充",
            ChartKind::AreaBetween => "区间填充",
            ChartKind::PeakFill => "峰填充",
            ChartKind::AucFill => "曲线下面积填充",
            ChartKind::StepLine => "阶梯线",
            ChartKind::MultiLine => "多序列折线",
            ChartKind::GroupedBar => "分组柱状图",
            ChartKind::StackedBar => "堆叠柱状图",
            ChartKind::StackedBar100 => "百分比堆叠柱状图",
            ChartKind::BarHorizontal => "横向柱状图",
            ChartKind::Lollipop => "棒棒糖图",
            ChartKind::ClevelandDot => "克利夫兰点图",
            ChartKind::DotPlot => "点图",
            ChartKind::ColumnScatter => "列散点图",
            ChartKind::ScatterWithBar => "柱形散点图",
            ChartKind::MultiBar => "多变量柱状图",
            ChartKind::NestedBar => "嵌套柱状图",
            ChartKind::BoxWhisker => "箱须图",
            ChartKind::BoxFloating => "浮动箱线图",
            ChartKind::ViolinFloating => "浮动小提琴图",
            ChartKind::GroupedBoxPlot => "分组箱线图",
            ChartKind::GroupedViolin => "分组小提琴图",
            ChartKind::MultiBoxPlot => "多变量箱线图",
            ChartKind::MultiViolin => "多变量小提琴图",
            ChartKind::NestedBoxPlot => "嵌套箱线图",
            ChartKind::GroupedScatter => "分组散点图",
            ChartKind::ScatterQuadrant => "象限散点图",
            ChartKind::Volcano => "火山图",
            ChartKind::NestedPoint => "嵌套点图",
            ChartKind::NumberAtRisk => "风险人数表",
            ChartKind::Ecdf => "经验累积分布",
            ChartKind::QqPlot => "正态 QQ 图",
            ChartKind::RegressionPlot => "回归图",
            ChartKind::BeforeAfter => "前后对比图",
            ChartKind::GroupedBeforeAfter => "分组前后对比图",
            ChartKind::PercentageDot => "百分比点阵图",
            ChartKind::Mosaic => "马赛克图",
            ChartKind::ChiSquareAssoc => "卡方关联图",
            ChartKind::StackedBarContingency => "堆叠条（列联）",
            ChartKind::StackedBarParts => "堆叠条（构成）",
            ChartKind::Donut => "环形图",
            ChartKind::Facet => "分面图",
            ChartKind::ThreeWay => "三因素图",
            ChartKind::DualYAxis => "双 Y 轴图",
            ChartKind::DepthChart => "深度图",
            ChartKind::CorrMatrix => "相关矩阵",
            ChartKind::ScatterMatrix => "散点矩阵",
            ChartKind::MultiHeatmap => "多变量热图",
            ChartKind::NestedScatter => "嵌套散点图",
            ChartKind::SubcolumnBar => "子列柱状图",
            ChartKind::Waffle => "华夫图",
            ChartKind::Dumbbell => "哑铃图",
            ChartKind::Ridgeline => "山脊图",
            ChartKind::Radar => "雷达图",
            ChartKind::Contour => "等高线图",
            ChartKind::Residual => "残差图",
        }
    }

    // ── 参数化旗标 ────────────────────────────────────────────────────────

    /// 堆叠：各组数值累加成一柱，而不是并排。
    pub fn stacked(self) -> bool {
        matches!(
            self,
            ChartKind::StackedBar
                | ChartKind::StackedBar100
                | ChartKind::StackedBarContingency
                | ChartKind::StackedBarParts
        )
    }

    /// 归一化到 100%：堆叠后按占比缩放，各柱等高。
    pub fn stacked_100(self) -> bool {
        matches!(self, ChartKind::StackedBar100)
    }

    /// 横向：交换两轴。
    pub fn horizontal(self) -> bool {
        matches!(self, ChartKind::BarHorizontal)
    }

    /// 平滑：折线走 Catmull-Rom 插值而不是直线段。
    pub fn smooth(self) -> bool {
        matches!(
            self,
            ChartKind::Curve | ChartKind::SmoothCurve | ChartKind::FitCurve
        )
    }

    /// 阶梯：折线走「先横后竖」的阶梯段。
    pub fn step(self) -> bool {
        matches!(self, ChartKind::StepLine)
    }

    /// 面积填充：折线下方铺色。
    ///
    /// 四种填充的差别只在基线：`AreaFill` 到轴底、`AreaBetween` 到另一条线、
    /// `PeakFill` 只填高于基线的段、`AucFill` 到轴底且画满全程。
    pub fn area(self) -> bool {
        matches!(
            self,
            ChartKind::AreaFill
                | ChartKind::AreaBetween
                | ChartKind::PeakFill
                | ChartKind::AucFill
        )
    }

    /// 只画线不画点。
    pub fn line_only(self) -> bool {
        matches!(self, ChartKind::LineOnly)
    }

    /// 棒棒糖：细杆 + 顶端圆点（细杆用柱体的窄版实现）。
    pub fn lollipop(self) -> bool {
        matches!(self, ChartKind::Lollipop)
    }

    /// 只画点不画杆（Cleveland 点图）。
    pub fn dot_only(self) -> bool {
        matches!(self, ChartKind::ClevelandDot)
    }

    /// 点图：按值分箱后逐点堆叠（Wilkinson dot plot）。
    pub fn dot_stack(self) -> bool {
        matches!(self, ChartKind::DotPlot | ChartKind::PercentageDot)
    }

    /// 环形：饼图内径 > 0。
    pub fn donut(self) -> bool {
        matches!(self, ChartKind::Donut)
    }

    /// 强制启用双 Y 轴（不依赖 `right_axis` 开关）。
    pub fn force_dual_y(self) -> bool {
        matches!(self, ChartKind::DualYAxis)
    }

    /// 反向纵轴（深度图：越往下越大）。
    pub fn reversed_y(self) -> bool {
        matches!(self, ChartKind::DepthChart)
    }

    /// 是否需要「分箱数」控件。
    pub fn needs_bins(self) -> bool {
        matches!(
            self,
            ChartKind::Histogram | ChartKind::Heatmap | ChartKind::FitCurve
        )
    }

    /// 是否需要「内径」控件。
    pub fn needs_inner(self) -> bool {
        matches!(self, ChartKind::Pie | ChartKind::Donut)
    }

    /// 是否需要「点阵列数」控件。
    pub fn needs_waffle(self) -> bool {
        matches!(self, ChartKind::PercentageDot)
    }

    /// 是否需要「参考线」控件。
    pub fn needs_ref(self) -> bool {
        matches!(self, ChartKind::Forest | ChartKind::Volcano)
    }

    /// 是否需要「生存」相关控件。
    pub fn needs_survival(self) -> bool {
        matches!(self, ChartKind::Survival | ChartKind::NumberAtRisk)
    }

    /// 是否需要「分面」相关控件。
    pub fn needs_facet(self) -> bool {
        matches!(self, ChartKind::Facet | ChartKind::ThreeWay)
    }

    /// 是否需要「箱线样式」控件。
    pub fn needs_box_style(self) -> bool {
        self.family() == Family::Box
    }

    /// 是否需要「小提琴平滑度」控件。
    pub fn needs_violin(self) -> bool {
        self.family() == Family::Violin
    }

    /// 是否需要「热图」控件。
    pub fn needs_heat(self) -> bool {
        self.family() == Family::Heatmap
    }

    /// 是否需要「饼图」控件（显示总数 / 扇区分离 / 隐藏扇区）。
    pub fn needs_pie(self) -> bool {
        self.family() == Family::Pie
    }

    /// 脚本导出（TikZ / matplotlib）是否支持。
    ///
    /// 脚本后端目前只实现了**基础纵向柱**的几何；堆叠、横向、点图这类
    /// 变体的坐标算法不同，硬生成会得到错位的图。这里宁可明确报错。
    pub fn script_exportable(self) -> bool {
        matches!(
            self,
            ChartKind::Bar
                | ChartKind::GroupedBar
                | ChartKind::ColumnScatter
                | ChartKind::ScatterWithBar
                | ChartKind::SubcolumnBar
        )
    }

    /// 该图型是否已经实现渲染。
    ///
    /// 目前 14 个渲染族全部可用，62 种图型也就全部可用 ——
    /// 差别只在于某些变体的**专属语义**还没做（见 [`ChartKind::caveat`]）。
    #[allow(dead_code)]
    pub fn implemented(self) -> bool {
        true
    }

    /// 语义提示：该图型对数据结构有什么要求，不满足时给出说明。
    ///
    /// 返回 `None` 表示无附加约束。
    pub fn caveat(self) -> Option<&'static str> {
        match self {
            ChartKind::RegressionPlot | ChartKind::ScatterQuadrant | ChartKind::Volcano => {
                Some("此图型按「每组一个数值序列」绘制；XY 表的 X 列不参与")
            }
            ChartKind::Survival | ChartKind::NumberAtRisk => {
                Some("数据层没有事件指示器，所有观测按「事件」处理（等价于无删失 KM）")
            }
            ChartKind::CorrMatrix | ChartKind::ScatterMatrix => {
                Some("矩阵图按「组 × 分箱」导出，不是真正的变量两两矩阵")
            }
            ChartKind::Mosaic | ChartKind::ChiSquareAssoc => {
                Some("列联图按各组均值作权重，需要原始计数时请改用列联表分析")
            }
            ChartKind::Facet | ChartKind::ThreeWay => {
                Some("分面按组数自动排布网格，共享同一套轴范围")
            }
            _ => None,
        }
    }
}
