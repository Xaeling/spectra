use serde::{Deserialize, Serialize};

use crate::core::curve::FitModel;
use crate::core::model::{DataTable, TableKind};
use crate::core::palette::PaletteId;

/// 图型：24 种。
///
/// 结构与 plotters 的「坐标映射 + 系列绘制」一致：每种图型都必须有明确的
/// **数据前提**（见 [`ChartKind::requires`]）和**坐标映射**（见 [`ChartKind::family`]），
/// 不满足前提时直接给出说明，而不是画一张看起来像图的错误图形。
///
/// 变体只增不改：`serde` 按名字序列化，旧工程文件仍然可读。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChartKind {
    // ── XY 坐标族（X 连续） ──
    Scatter,
    Line,
    SmoothLine,
    StepLine,
    AreaLine,
    Regression,
    // ── 分类族（X 为组名） ──
    Column,
    GroupedBar,
    StackedBar,
    StackedBar100,
    HorizontalBar,
    Lollipop,
    DotPlot,
    BeforeAfter,
    Box,
    Violin,
    Ridgeline,
    // ── 单变量分布 ──
    Histogram,
    Ecdf,
    Qq,
    // ── 其他 ──
    Survival,
    Heatmap,
    Pie,
    Donut,
    /// ROC 曲线（诊断：阳性 vs 阴性两组）
    Roc,
    /// 森林图（每行一个研究：效应量 + 95% CI）
    Forest,
}

/// 坐标映射族：决定走哪一套渲染核心。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// 连续 X / 连续 Y
    Xy,
    /// 分类 X / 连续 Y
    Category,
    /// 值 → 频数（单变量分布）
    Distribution,
    /// 自成一套坐标（饼、热图、生存）
    Special,
}

impl ChartKind {
    pub const ALL: [ChartKind; 26] = [
        Self::Scatter,
        Self::Line,
        Self::SmoothLine,
        Self::StepLine,
        Self::AreaLine,
        Self::Regression,
        Self::Column,
        Self::GroupedBar,
        Self::StackedBar,
        Self::StackedBar100,
        Self::HorizontalBar,
        Self::Lollipop,
        Self::DotPlot,
        Self::BeforeAfter,
        Self::Box,
        Self::Violin,
        Self::Ridgeline,
        Self::Histogram,
        Self::Ecdf,
        Self::Qq,
        Self::Survival,
        Self::Heatmap,
        Self::Pie,
        Self::Donut,
        Self::Roc,
        Self::Forest,
    ];

    pub fn family(self) -> Family {
        match self {
            Self::Scatter
            | Self::Line
            | Self::SmoothLine
            | Self::StepLine
            | Self::AreaLine
            | Self::Regression => Family::Xy,
            Self::Column
            | Self::GroupedBar
            | Self::StackedBar
            | Self::StackedBar100
            | Self::HorizontalBar
            | Self::Lollipop
            | Self::DotPlot
            | Self::BeforeAfter
            | Self::Box
            | Self::Violin
            | Self::Ridgeline => Family::Category,
            Self::Histogram | Self::Ecdf | Self::Qq => Family::Distribution,
            Self::Survival | Self::Heatmap | Self::Pie | Self::Donut | Self::Roc | Self::Forest => {
                Family::Special
            }
        }
    }

    /// 该图型对数据表的最低要求。不满足时渲染层给提示而不是硬画。
    pub fn requires(self) -> &'static str {
        match self {
            Self::Scatter | Self::Line | Self::SmoothLine | Self::StepLine => {
                "需要 XY 表：一列 X + 至少一列 Y"
            }
            Self::AreaLine => "需要 XY 表；误差列会画成 ± 误差带",
            Self::Regression => "需要 XY 表：至少 3 个点才能拟合",
            Self::Column | Self::Lollipop | Self::DotPlot | Self::Box | Self::Violin => {
                "每个 Y 列一个组（柱状表 / XY 表的 Y 列）"
            }
            Self::HorizontalBar => "每个 Y 列一个组，数值画在横轴",
            Self::GroupedBar | Self::StackedBar | Self::StackedBar100 => {
                "分组表：每行一个类别、每列一个系列"
            }
            Self::BeforeAfter => "两列配对数据（同一行是同一个体）",
            Self::Ridgeline => "每列一组，按组叠放核密度",
            Self::Histogram | Self::Ecdf | Self::Qq => "每列一组的数值列",
            Self::Roc => "两组连续观测：第 1 组阳性（病例）、第 2 组阴性（对照）",
            Self::Forest => "每行一个研究：列1=效应量、列2=CI下限、列3=CI上限（可选列4=权重）",
            Self::Survival => "时间 + 事件状态（生存表）",
            Self::Heatmap => "组 × 分箱的二维密度",
            Self::Pie | Self::Donut => "每列一个扇区（构成表的合计，或柱状表的均值）",
        }
    }

    /// 分类轴（X 为组名）还是连续轴（数值）。
    pub fn categorical(self) -> bool {
        matches!(self.family(), Family::Category)
    }

    /// 是否把「组均值 + 误差棒」当主体。
    pub fn mean_based(self) -> bool {
        matches!(
            self,
            Self::Column | Self::GroupedBar | Self::HorizontalBar | Self::Lollipop
        )
    }

    pub fn is_stacked(self) -> bool {
        matches!(self, Self::StackedBar | Self::StackedBar100)
    }

    pub fn horizontal(self) -> bool {
        matches!(self, Self::HorizontalBar)
    }

    pub fn smooth(self) -> bool {
        matches!(self, Self::SmoothLine)
    }

    pub fn step(self) -> bool {
        matches!(self, Self::StepLine)
    }

    pub fn area(self) -> bool {
        matches!(self, Self::AreaLine)
    }

    pub fn fits(self) -> bool {
        matches!(self, Self::Regression)
    }

    /// 是否显示「误差棒」控件（均值型图 + XY 带误差列的图）。
    pub fn shows_error(self) -> bool {
        match self.family() {
            Family::Xy => !self.fits(),
            Family::Category => matches!(
                self,
                Self::Column | Self::GroupedBar | Self::HorizontalBar | Self::Lollipop
            ),
            _ => false,
        }
    }

    /// 「叠加散点」开关是否**可调**。
    ///
    /// 散点图本身、回归图的原始观测是硬性规定（规范 G3「必须展示原始点」、
    /// C1「点不得被拟合线盖住」）——拧了也不生效的开关就不该出现在界面上。
    pub fn points_toggleable(self) -> bool {
        self.shows_points() && !matches!(self, Self::Scatter | Self::Regression)
    }

    /// 「散点抖动」开关是否可调。箱线/小提琴叠点必须抖动（硬性），柱类可选。
    pub fn jitter_toggleable(self) -> bool {
        matches!(self, Self::Column | Self::GroupedBar | Self::HorizontalBar)
    }

    /// 「拟合模型」控件：只有回归图有拟合。
    pub fn fit_toggleable(self) -> bool {
        self.fits()
    }

    /// 「双 Y 轴」控件：XY 家族才有多序列共用 X 的场景。
    /// 回归图不给（双 Y 的回归会误导斜率解释）。
    pub fn dual_y_toggleable(self) -> bool {
        matches!(self.family(), Family::Xy) && !self.fits()
    }

    /// 「分面」控件：多组小倍数。XY 全系；分类图只给「单标记类」
    /// （柱/箱/小提琴/棒棒糖/点阵）——堆叠、前后对比等跨组同框才有意义的不拆，
    /// 不给拧了也不生效的开关。
    pub fn facet_toggleable(self) -> bool {
        matches!(self.family(), Family::Xy)
            || matches!(
                self,
                Self::Column | Self::Box | Self::Violin | Self::Lollipop | Self::DotPlot
            )
    }

    /// 是否显示「叠加散点」控件。
    pub fn shows_points(self) -> bool {
        matches!(self, Self::Column | Self::GroupedBar | Self::HorizontalBar)
            || matches!(self.family(), Family::Xy)
    }

    /// 是否显示「柱宽」控件（含小提琴的半宽，它同属「横向占宽」）。
    pub fn shows_width(self) -> bool {
        matches!(
            self,
            Self::Column
                | Self::GroupedBar
                | Self::StackedBar
                | Self::StackedBar100
                | Self::HorizontalBar
                | Self::Violin
        )
    }

    /// 填充类图型：色块会顶到右上角，图例放进去必然压住数据。
    ///
    /// 这类图型的图例默认改到右侧外部（§7.3「图例要待在空白里」）。
    pub fn filled_marks(self) -> bool {
        matches!(
            self,
            Self::Column
                | Self::GroupedBar
                | Self::StackedBar
                | Self::StackedBar100
                | Self::HorizontalBar
                | Self::Lollipop
                | Self::AreaLine
                // 线族同样会穿过右上角：饱和曲线（剂量–反应、生长曲线）几乎
                // 必然经过那里，图例放进去就是压在数据上。
                | Self::Line
                | Self::SmoothLine
                | Self::StepLine
                | Self::Regression
        )
    }

    /// 是否可能同时出现多个数据系列（决定配色 / 色盲检查是否有意义）。
    ///
    /// 实际上每种图型都能画多组（多列 Y / 多个扇区），所以恒为 true；
    /// 保留这个方法是为了让调用点表达意图，将来若有真正单系列图型
    /// （如单条 ECDF）可以直接收窄，而不必改所有调用点。
    pub fn uses_multiple_series(self) -> bool {
        true
    }

    /// 是否显示「符号 / 线宽」控件。
    pub fn shows_line(self) -> bool {
        matches!(self.family(), Family::Xy)
    }

    /// 由键名解析图型（供 `STATAURA_KIND` 与命令面板使用）。
    pub fn from_key(key: &str) -> Option<Self> {
        let k = key.trim();
        // 先用中文名 / 短名精确匹配
        for c in Self::ALL {
            if c.label() == k
                || c.short() == k
                || c.label().eq_ignore_ascii_case(k)
                || format!("{c:?}").eq_ignore_ascii_case(k)
            {
                return Some(c);
            }
        }
        None
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Scatter => "散点图",
            Self::Line => "折线图",
            Self::SmoothLine => "平滑曲线",
            Self::StepLine => "阶梯线",
            Self::AreaLine => "面积图",
            Self::Regression => "回归图",
            Self::Column => "柱状图",
            Self::GroupedBar => "分组柱状图",
            Self::StackedBar => "堆叠柱状图",
            Self::StackedBar100 => "百分比堆叠柱",
            Self::HorizontalBar => "横向柱状图",
            Self::Lollipop => "棒棒糖图",
            Self::DotPlot => "点阵图",
            Self::BeforeAfter => "前后对比图",
            Self::Box => "箱线图",
            Self::Violin => "小提琴图",
            Self::Ridgeline => "山脊图",
            Self::Histogram => "直方图",
            Self::Ecdf => "累积分布 (ECDF)",
            Self::Qq => "Q–Q 图",
            Self::Roc => "ROC 曲线",
            Self::Forest => "森林图",
            Self::Survival => "生存曲线",
            Self::Heatmap => "热图",
            Self::Pie => "饼图",
            Self::Donut => "环形图",
        }
    }

    /// 检查器 / 侧栏用短名。
    pub fn short(self) -> &'static str {
        match self {
            Self::Scatter => "散点",
            Self::Line => "折线",
            Self::SmoothLine => "平滑",
            Self::StepLine => "阶梯",
            Self::AreaLine => "面积",
            Self::Regression => "回归",
            Self::Column => "柱状",
            Self::GroupedBar => "分组",
            Self::StackedBar => "堆叠",
            Self::StackedBar100 => "百分比",
            Self::HorizontalBar => "横向",
            Self::Lollipop => "棒棒糖",
            Self::DotPlot => "点阵",
            Self::BeforeAfter => "前后",
            Self::Box => "箱线",
            Self::Violin => "小提琴",
            Self::Ridgeline => "山脊",
            Self::Histogram => "直方",
            Self::Ecdf => "ECDF",
            Self::Qq => "Q–Q",
            Self::Roc => "ROC",
            Self::Forest => "森林",
            Self::Survival => "生存",
            Self::Heatmap => "热图",
            Self::Pie => "饼图",
            Self::Donut => "环形",
        }
    }

    /// 图型族的中文名（检查器分组标题）。
    pub fn family_label(self) -> &'static str {
        match self.family() {
            Family::Xy => "XY 坐标",
            Family::Category => "分类",
            Family::Distribution => "分布",
            Family::Special => "其他",
        }
    }

    /// 该图型能否在给定数据表上被**诚实**地表达。
    ///
    /// 这是「图型 ↔ 表格类型」的唯一事实来源：检查器磁贴、画布菜单、顶栏菜单、
    /// 命令面板用它过滤可选项，渲染入口用它兜底（不匹配时给说明，不画假图）。
    ///
    /// 判定依据是 Prism 的表格格式语义（表格格式决定能画什么图），而不是
    /// 「渲染器碰巧能不能跑起来」——后者正是会产生误导图形的地方。
    pub fn supports(self, t: TableKind) -> bool {
        use TableKind as T;
        match self {
            // XY 族：需要一列 X + 若干 Y 子列，只有 XY 表有 X 列。
            Self::Scatter
            | Self::Line
            | Self::SmoothLine
            | Self::StepLine
            | Self::AreaLine
            | Self::Regression => t == T::Xy,

            // 生存曲线：只有生存表有「时间 + 事件/删失」语义。
            Self::Survival => t == T::Survival,
            // ROC：两组连续观测（阳性/阴性）
            Self::Roc => matches!(t, T::Column | T::Grouped | T::Xy | T::MultipleVariables),
            Self::Forest => t == T::MultipleVariables,

            // 构成图：值之和为总数，或列联表的整数频数合计。
            Self::Pie | Self::Donut => matches!(t, T::PartsOfWhole | T::Contingency),

            // 二维矩阵：行是类别、列是系列，且每格是一个数值。
            Self::Heatmap => matches!(t, T::Grouped | T::Contingency | T::MultipleVariables),

            // 分组 / 堆叠：需要「行类别 × 列系列」两个分组变量。
            Self::GroupedBar | Self::StackedBar | Self::StackedBar100 => {
                matches!(t, T::Grouped | T::Contingency)
            }

            // 前后对比：同一行的两列是同一个体；XY 表第一列是 X，不构成配对。
            Self::BeforeAfter => matches!(
                t,
                T::Column | T::Grouped | T::MultipleVariables | T::PartsOfWhole
            ),

            // 其余分类族 + 分布族：每列（或每个 Y 子列组）是一组数值。
            // 构成表 / 生存表 / 列联表的值不是「同一变量的重复观测」，排除。
            _ => matches!(
                t,
                T::Xy | T::Column | T::Grouped | T::MultipleVariables | T::Nested
            ),
        }
    }

    /// 按族给出「指定表格类型允许的全部图型」，供 UI 直接渲染。
    /// 空族会被省略，避免留下无意义的分组标题。
    pub fn groups_for(t: TableKind) -> Vec<(&'static str, Vec<ChartKind>)> {
        Self::group_all()
            .iter()
            .map(|(name, kinds)| {
                (
                    *name,
                    kinds
                        .iter()
                        .copied()
                        .filter(|k| k.supports(t))
                        .collect::<Vec<ChartKind>>(),
                )
            })
            .filter(|(_, kinds)| !kinds.is_empty())
            .collect()
    }

    /// 该表格类型允许的图型数量（面板头部的计数）。
    pub fn count_for(t: TableKind) -> usize {
        Self::ALL.iter().filter(|k| k.supports(t)).count()
    }

    /// 按族给出「本族内允许的图型」，检查器按族分组展示。
    pub fn group_all() -> &'static [(&'static str, &'static [ChartKind])] {
        const XY: [ChartKind; 6] = [
            ChartKind::Scatter,
            ChartKind::Line,
            ChartKind::SmoothLine,
            ChartKind::StepLine,
            ChartKind::AreaLine,
            ChartKind::Regression,
        ];
        const CAT: [ChartKind; 11] = [
            ChartKind::Column,
            ChartKind::GroupedBar,
            ChartKind::StackedBar,
            ChartKind::StackedBar100,
            ChartKind::HorizontalBar,
            ChartKind::Lollipop,
            ChartKind::DotPlot,
            ChartKind::BeforeAfter,
            ChartKind::Box,
            ChartKind::Violin,
            ChartKind::Ridgeline,
        ];
        const DIST: [ChartKind; 3] = [ChartKind::Histogram, ChartKind::Ecdf, ChartKind::Qq];
        const SPECIAL: [ChartKind; 6] = [
            ChartKind::Roc,
            ChartKind::Forest,
            ChartKind::Survival,
            ChartKind::Heatmap,
            ChartKind::Pie,
            ChartKind::Donut,
        ];
        &[
            ("XY 坐标", &XY),
            ("分类", &CAT),
            ("分布", &DIST),
            ("其他", &SPECIAL),
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorKind {
    None,
    Sd,
    Sem,
    Ci95,
}

impl ErrorKind {
    pub const ALL: [ErrorKind; 4] = [Self::None, Self::Sd, Self::Sem, Self::Ci95];
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "不显示",
            Self::Sd => "均值 ± SD",
            Self::Sem => "均值 ± SEM",
            Self::Ci95 => "均值 ± 95% CI",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LegendPos {
    InsideTopRight,
    InsideTopLeft,
    OutsideRight,
    Bottom,
}

impl LegendPos {
    pub fn label(self) -> &'static str {
        match self {
            Self::InsideTopRight => "图内右上",
            Self::InsideTopLeft => "图内左上",
            Self::OutsideRight => "图外右侧",
            Self::Bottom => "底部横排",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlotThemeId {
    Aura,
    Prism,
    Nature,
    Mono,
}

impl PlotThemeId {
    pub const ALL: [PlotThemeId; 4] = [Self::Aura, Self::Prism, Self::Nature, Self::Mono];
    pub fn label(self) -> &'static str {
        match self {
            Self::Aura => "Aura",
            Self::Prism => "Prism 经典",
            Self::Nature => "Nature 投稿",
            Self::Mono => "单色印刷",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Marker {
    Circle,
    Square,
    Triangle,
    Diamond,
    Cross,
}

impl Marker {
    pub const ALL: [Marker; 5] = [
        Self::Circle,
        Self::Square,
        Self::Triangle,
        Self::Diamond,
        Self::Cross,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Circle => "●",
            Self::Square => "■",
            Self::Triangle => "▲",
            Self::Diamond => "◆",
            Self::Cross => "✚",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SigAnnotation {
    pub a: usize,
    pub b: usize,
    pub label: String,
    /// None = 自动放在两组数据最大值上方
    #[serde(default)]
    pub y: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphSpec {
    pub kind: ChartKind,
    pub title: String,
    pub show_title: bool,
    pub x_title: String,
    pub y_title: String,
    pub theme: PlotThemeId,
    pub palette: PaletteId,
    pub x_log: bool,
    pub y_log: bool,
    pub x_range: Option<[f64; 2]>,
    pub y_range: Option<[f64; 2]>,
    pub show_legend: bool,
    pub legend_pos: LegendPos,
    pub error: ErrorKind,
    pub show_points: bool,
    pub jitter: bool,
    pub point_size: f64,
    pub line_width: f64,
    pub bar_width: f64,
    pub show_grid: bool,
    pub marker: Marker,
    pub histogram_bins: usize,
    pub annotations: Vec<SigAnnotation>,
    /// 拟合模型（回归图）。默认直线；四/五参数 Logistic、Hill、指数等见 `FitModel`。
    #[serde(default)]
    pub fit_model: FitModel,
    /// 双 Y 轴：第二组起的序列映射到右侧轴（XY 家族）。
    #[serde(default)]
    pub dual_y: bool,
    /// 右侧 Y 轴标题
    #[serde(default)]
    pub y2_title: String,
    /// 分面：多组小倍数（共享 X），替代「多组颜色堆在一起」。
    /// **纯开关**：规范 G8 建议多组时开启，但开不开由你定。
    #[serde(default)]
    pub facet: bool,
    /// 分面时每个面板用自己的 Y 值域（默认关 = 共享值域，可直接比高低）。
    /// 只有各组量级差一个数量级以上、又只想看形状时才打开，并在图注写明。
    #[serde(default)]
    pub facet_free_y: bool,
    /// 图内标注（统计文字 / 显著性括号 / 图注行）总开关。
    /// 关掉时**数据与坐标轴不动**，只是不画这些文字——投稿时常常要一张干净图。
    #[serde(default = "yes")]
    pub annotations_visible: bool,
}

/// serde 默认值：老工程文件里没有这个字段时按「显示」处理。
fn yes() -> bool {
    true
}

impl Default for GraphSpec {
    fn default() -> Self {
        Self {
            kind: ChartKind::Column,
            title: String::new(),
            show_title: true,
            x_title: String::new(),
            y_title: String::new(),
            theme: PlotThemeId::Aura,
            palette: PaletteId::Aura,
            x_log: false,
            y_log: false,
            x_range: None,
            y_range: None,
            show_legend: true,
            legend_pos: LegendPos::InsideTopRight,
            error: ErrorKind::Sem,
            show_points: true,
            jitter: true,
            point_size: 5.0,
            line_width: 1.8,
            bar_width: 0.62,
            show_grid: false,
            marker: Marker::Circle,
            histogram_bins: 0,
            annotations: Vec::new(),
            fit_model: FitModel::default(),
            dual_y: false,
            y2_title: String::new(),
            facet: false,
            facet_free_y: false,
            annotations_visible: true,
        }
    }
}

impl GraphSpec {
    pub fn default_for(t: &DataTable) -> Self {
        let mut s = Self::default();
        s.kind = match t.kind {
            TableKind::Xy => ChartKind::Line,
            TableKind::Column => ChartKind::Column,
            TableKind::Grouped => ChartKind::GroupedBar,
            TableKind::Contingency => ChartKind::StackedBar100,
            TableKind::Survival => ChartKind::Survival,
            TableKind::PartsOfWhole => ChartKind::Pie,
            // 多变量表每列是一个变量、每行一个案例，没有 X 列；
            // 默认用柱状图看各变量的均值，而不是需要 X 列的散点图。
            TableKind::MultipleVariables | TableKind::Nested => ChartKind::Column,
        };
        s.title = t.name.clone();
        s.x_title = match t.kind {
            TableKind::Xy | TableKind::Survival => t
                .columns
                .first()
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "X".into()),
            _ => String::new(),
        };
        s.y_title = match t.kind {
            TableKind::PartsOfWhole => "Value".into(),
            // 列/分组表的列名是组名，不是量纲；默认留空，由用户填写。
            _ => String::new(),
        };
        if t.kind == TableKind::Xy {
            s.show_legend = t.group_names.len() > 1;
            s.error = ErrorKind::Sem;
        }
        if t.kind == TableKind::Column {
            s.show_legend = false;
        }
        s
    }

    pub fn tick_size(&self) -> f32 {
        11.0
    }
}

#[cfg(test)]
mod compatibility_tests {
    use super::*;

    #[test]
    fn 每种表格的默认图型都在允许集合内() {
        for t in TableKind::ALL {
            let spec = GraphSpec::default_for(&DataTable::template(t, "t".to_string(), 1));
            assert!(
                spec.kind.supports(t),
                "{} 的默认图型 {} 不被允许",
                t.label(),
                spec.kind.label()
            );
        }
    }

    #[test]
    fn 每种表格至少有一个可用图型() {
        for t in TableKind::ALL {
            let n = ChartKind::count_for(t);
            assert!(n > 0, "{} 没有任何可用图型", t.label());
            assert_eq!(
                n,
                ChartKind::groups_for(t)
                    .iter()
                    .map(|(_, v)| v.len())
                    .sum::<usize>(),
                "{} 的分组列表与计数不一致",
                t.label()
            );
        }
    }

    #[test]
    fn 生存表只给生存曲线() {
        assert_eq!(ChartKind::count_for(TableKind::Survival), 1);
        assert!(ChartKind::Survival.supports(TableKind::Survival));
        assert!(!ChartKind::Scatter.supports(TableKind::Survival));
    }

    #[test]
    fn XY图型不会出现在非XY表上() {
        for k in [ChartKind::Scatter, ChartKind::Line, ChartKind::Regression] {
            assert!(k.supports(TableKind::Xy));
            assert!(!k.supports(TableKind::Column));
            assert!(!k.supports(TableKind::Survival));
            assert!(!k.supports(TableKind::PartsOfWhole));
        }
    }

    #[test]
    fn 构成图不会出现在需要重复观测的表上() {
        for k in [ChartKind::Pie, ChartKind::Donut] {
            assert!(k.supports(TableKind::PartsOfWhole));
            assert!(k.supports(TableKind::Contingency));
            assert!(!k.supports(TableKind::Xy));
            assert!(!k.supports(TableKind::Column));
            assert!(!k.supports(TableKind::Survival));
        }
    }

    #[test]
    fn 分组列表里没有重复图型也没有非法项() {
        for t in TableKind::ALL {
            let all: Vec<ChartKind> = ChartKind::groups_for(t)
                .into_iter()
                .flat_map(|(_, v)| v)
                .collect();
            let mut sorted = all.clone();
            sorted.sort_by_key(|k| format!("{k:?}"));
            sorted.dedup_by_key(|k| format!("{k:?}"));
            assert_eq!(all.len(), sorted.len(), "{} 出现重复图型", t.label());
            assert!(all.iter().all(|k| k.supports(t)));
        }
    }
}
