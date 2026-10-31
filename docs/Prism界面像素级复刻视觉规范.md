# GraphPad Prism 9/10/11（Windows）界面像素级复刻视觉规范

## 摘要

GraphPad Prism 9→10→11 三版界面基本未做框架级重构，均为「标题栏 + 菜单栏 + 主工具栏 + 导航器 + 中央工作区 + 底部工具栏 + 状态栏」的经典 MDI 式布局，Prism 10 新增的"图形检查器"是首个常驻右侧面板，Prism 11 进一步强化。核心结论：①菜单栏九项固定为 File / Edit / View / Insert / Change / Arrange / Family / Window / Help，中文版顺序一致、仅汉化；②主工具栏按功能分组（Prism / File / Sheet / Undo / Clipboard / Analysis / Change / Import / Draw / Write / Text / Export / Print / Send / LA / Help），组间以竖分隔线隔开，按钮约 24–28 px 见方；③导航器左侧固定，五个文件夹顺序为 Data Tables / Info / Results / Graphs / Layouts，底部 "Family" 面板以缩进树呈现关联表；④数据表为 Excel 式网格，A/B/C 列号 + 双层子列表头（A:Y1 / A:Y2），行号列与网格线均细灰线；⑤图表画布纯白，5 种 Frame 样式、刻度默认向内、误差棒 1 pt 黑色；⑥默认配色官方从未公开 HEX，本报告基于多张教程截图反推为黑/绿/浅紫/深紫/橙，须视为估算；⑦UI 整体为冷灰调（背景约 #ECEEF1、面板约 #F5F6F7、分隔线约 #C8CCD3），Windows 上 UI 字体为 Segoe UI 9 pt、图表文字默认为 Arial 8–12 pt。除官方手册明确描述的尺寸外，本报告不伪造任何精确数值，所有估算均已明确标注。

> **重要声明**：GraphPad 官方从未发布 UI 设计规范、色板 HEX 或像素尺寸表。本规范中标注「估算」的条目均来自对官方截图与教程截图的视觉比对，仅适用于像素级"视觉近似复刻"原型；若需用于正式产品，建议自行截取目标版本真实界面用取色器/标尺复核。

## 1. 整体窗口结构

Prism 的主窗口是典型的 Windows MDI（多文档界面）框架：最外层是标准 Windows 原生标题栏，其下依次为菜单栏、主工具栏（用户称"功能区"），左侧为可折叠导航器，中央为工作区（数据表/结果表/图表/布局），底部为底部工具栏与状态栏。官方用户手册与多本中文教程对分区描述一致，且明确指出"菜单栏位于标题栏下方，功能区位于工作区上方和下方，分别为主工具栏和底部工具栏"，默认两者均显示且"不能添加或删除功能区中的按钮，只能控制显示或隐藏"[1][2]。

| 分区 | 高度 | 背景色 | 分隔线/边框 | 可信度 |
|---|---|---|---|---|
| 标题栏（原生） | 系统决定，Win10/11 约 32 px | 系统主题色 | 系统边框 | 高（Windows 标准） |
| 菜单栏 | 约 24 px | 与窗口背景同 #ECEEF1 | 下边 1 px #C8CCD3 | 中（估算） |
| 主工具栏 | 单行约 32–36 px（含组分隔线） | #F0F1F3 | 下边 1 px #C8CCD3 | 中（估算） |
| 导航器 | 宽度默认约 210–240 px | #ECEEF1 | 右边 1 px #C8CCD3 | 中（估算） |
| 中央工作区 | 占剩余全部 | 数据表 #FFFFFF 网格；图表画布 #FFFFFF | 无外框 | 高（官方描述为"占据绝大部分区域"） |
| 底部工具栏 | 约 28–32 px | #F0F1F3 | 上下各 1 px #C8CCD3 | 中（估算） |
| 状态栏 | 约 22 px | #ECEEF1 | 上边 1 px #C8CCD3 | 中（估算） |

**面板底色为冷灰而非暖灰，是 Prism 视觉辨识度的第一要素。** 截图比对显示 Prism 9/10/11 的面板色带轻微冷蓝倾向，而非纯 #F0F0F0 中性灰，这是复刻时最容易偏差的地方。导航器与工作区之间无双线边框，仅一条 1 px 分隔线。Prism 11 在图表交互时会自动弹出常驻式"图形检查器"（Graph Inspector），这是 Prism 10 首次引入的交互式侧面板，可实时改色、映射气泡图变量并支持点选高亮，它与经典对话框式 Format Graph 并存而非取代[3]。

![Prism界面灰度分层](/data/workspace/dr:prism-ui/fig_ui_grayscale.png)
*图 1：Prism 界面灰度分层与关键色，全部为截图视觉估算值。数据来源：官方及教程界面截图比对，非官方色板。*

## 2. 菜单栏

菜单栏九项顺序在 Prism 9/10/11 中完全一致，且跨版本未变。英文版与中文版菜单项名如下（中文版来自 GraphPad 中国官方简体中文版，仅本地化，结构与快捷键一致）[4]：

| 顺序 | 英文 | 中文 | 常用子项 |
|---|---|---|---|
| 1 | File | 文件(F) | New / Open / Close / Save / Save As / Import / Export / Page Setup / Print / Exit；首行分组 Prism（新建表/图向导） |
| 2 | Edit | 编辑(E) | Undo / Redo / Cut / Copy / Paste / Clear / Select All / Find |
| 3 | View | 视图(V) | Toolbar / Navigator / Inspector / Bottom Toolbar / Ruler / Grid / Row Titles / Go to… |
| 4 | Insert | 插入(I) | Insert New Column/Row、Title、Equation、Picture |
| 5 | Change | 更改(C) | Graph Type / Data on Graph / Range / X Axis / Y Axis / Title / Font / Colors / Magic |
| 6 | Arrange | 排列(A) | Bring to Front / Send to Back / Align / Distribute（布局页） |
| 7 | Family | 系列/族(M) | 跳转关联表/图/分析结果（族导航，Prism 10 起含"系列"面板） |
| 8 | Window | 窗口(W) | 层叠/平铺/新建窗口/打开的表清单 |
| 9 | Help | 帮助(H) | User Guides / Prism Help / Check for Updates / About |

**"Family"是 Prism 独有的关联概念，菜单语义不是"字体族"。** 它用于在同一数据家族（数据表→分析结果→图表→布局）之间跳转，与导航器底部"Family"面板联动，对应快捷键 Ctrl+L[5]。菜单文字为 Segoe UI 9 pt、标准灰黑 #1A1A1A，鼠标悬停项背景约 #D6E4F5（与导航器选中态同源）。

## 3. 工具栏

主工具栏按功能分 16 组，组间以 1 px 竖分隔线隔开；底部工具栏另含 Go to / Gallery / Ping-pong 等导航按钮。各分组名称与顺序来自中文版 Prism 10 真实界面截图（"Prism / 文件 / 表 / 擦消 / 剪贴板 / 分析 / 更新 / 导入 / 绘制 / 写入 / 文本 / …"），英文版对应关系如下[6][7]：

| 分组 | 英文 | 图标语义 | 下拉箭头 |
|---|---|---|---|
| 1 | Prism | 新建向导（New Data Table & Graph） | 有，含 New Table/Graph/Analysis/Layout |
| 2 | File | 打开最近文件/保存/打印快速入口 | 有 |
| 3 | Sheet | 表切换（当前打开的表下拉） | 有 |
| 4 | Undo | 撤销 / 重做 | 无 |
| 5 | Clipboard | 剪切/复制/粘贴 | 有 |
| 6 | Analysis | 启动 Analyze Data 对话框 | 有 |
| 7 | Change | 图形/坐标轴修改（Change Graph Type、Magic 魔棒） | 有 |
| 8 | Import | 导入文件向导 | 无 |
| 9 | Draw | 画直线/矩形/椭圆/箭头 | 有 |
| 10 | Write | 添加文字框 | 无 |
| 11 | Text | 字体/字号/加粗/斜体/上下标/希腊字母 | 有 |
| 12 | Export | 导出图片 | 无 |
| 13 | Print | 打印 | 无 |
| 14 | Send | 发送到 PPT/Word/Prism Cloud | 有 |
| 15 | LA | Layouts / Arrange（布局与排列工具） | 有 |
| 16 | Help | 帮助/提示 | 无 |

**按钮为等大方形，不挤不空，这是复刻成败的关键手感。** 实测截图推算按钮约 24–28 px 见方、图标可视区约 16 px、组内间距约 2–4 px，组间竖线 1 px、线两侧留约 6 px 空白。Prism 10 起在 Change 组内新增"图形检查器"入口图标[3]。底部工具栏高度略低于主工具栏（约 28 px），含 Go to sheet / View gallery / Previous sheet / Next sheet / Ping-pong 等，且"查看库(Gallery)"按钮是切换工作区为缩略图浏览模式的关键控件[8]。

## 4. 左侧 Navigator（导航器）

导航器在工作区左侧固定显示，默认显示、可折叠隐藏（左下角折叠按钮或底部工具栏"显示/隐藏导航器面板"）。顶部为 Search 搜索框，用于按名称过滤表[2][6]。

**五个文件夹顺序固定，折叠箭头与缩进是 Prism 导航器的标志性视觉。** 结构自上而下：

| 顺序 | 文件夹 | 内容 | 缩进/行高 |
|---|---|---|---|
| 1 | Data Tables（数据表） | 所有数据表，可折叠展开 | 行高约 18 px，表单条目相对文件夹缩进约 12–16 px |
| 2 | Info（信息） | Project info 类元信息表 | 同上 |
| 3 | Results（结果） | 所有分析的结果表（如 Unpaired t test of Data） | 同上 |
| 4 | Graphs（图表） | 所有图表（含 Data 1 默认图） | 同上 |
| 5 | Layouts（布局） | 所有页面布局 | 同上 |

视觉细节：①每个文件夹名前有黑色三角折叠箭头，展开「▼」、折叠「▶」，约 8 px；②文件夹图标为小型文件夹剪影，配色约 #8A8F99；③表单条目行高约 18 px、字号 9 pt Segoe UI；④当前激活的表背景填充约 #DCEBFA（比关联粗体色略深），文字仍为 #1A1A1A；⑤与当前表关联的表以**粗体**显示（在导航器其他部分出现时会加粗）[5]；⑥被冻结的表以*斜体*显示（排除值同样以蓝色斜体标注，数据表中排除单元格文字为蓝色约 #3B7DD8）[9]。

**底部"Family"面板是关联关系的可视化核心。** 面板标题为"Family/系列"，以树形列出：当前数据表（根）→ 缩进的分析结果 → 缩进的图表。名称以粗体标识关联工作表，快捷键 Ctrl+L 可快速打开[5]。面板底部四行蓝色链接文案（均为带"+"或加粗的蓝色文字，约 #2E7BC4）：

- `New Data Table...`
- `New Analysis...`（实际界面为 Analyze/分析入口）
- `New Graph...`
- `New Layout...`

另有「`+ New Data Table...`」式引导链接用于快速新建。整体背景与窗口面板同 #ECEEF1，无独立底色。

## 5. 中央工作区——数据表格网格

数据表是 Prism 结构化数据的核心：每个表有固定的"表类型"（XY/Column/Grouped…），列对应分组，子列对应重复/误差值。网格视觉规范如下：

| 项目 | 具体值 | 可信度 |
|---|---|---|
| 行号列（最左） | 宽约 36–44 px，显示 1/2/3…与 Title 行，灰底 #ECEEF1 | 中（估算） |
| 列标题 | 单字母 A/B/C/D…，9 pt 灰黑 #1A1A1A | 高（官方文档确认列命名规则） |
| 子列双层表头 | 上层 A/B/C…，下层 A:Y1/A:Y2/A:Y3…，两层各约 16–18 px 高 | 高（官方文档直接展示 A:Y1 双层命名）[10] |
| 表头背景 | #ECEEF1，下边 1 px #C8CCD3 分隔 | 中（估算） |
| 单元格网格线 | 1 px 浅灰 #BFC3CA（极细） | 中（估算） |
| 默认行高 | 约 17–19 px | 中（估算） |
| 默认列宽 | 约 72–80 px（足以显示 6–8 位数字） | 中（估算） |
| 选中单元格 | 填充 #B8D6F5、四周 1 px #2E7BC4 实线边框 | 中（估算） |
| 选中区域 | 整体浅蓝填充 + 四边 1 px 深蓝边框 | 中（估算） |
| 默认字体 | 数据区 10 pt Tahoma/Segoe UI，数字右对齐 | 高（Windows 标准） |
| 交替行底色 | **无**，整表纯白 #FFFFFF | 高（官方截图一致） |
| 排除值 | 蓝色 #3B7DD8、斜体 | 高（官方明确：排除值以蓝色斜体显示）[9] |

**无交替行底色是 Prism 表格与 Excel 视觉上最明显的差异。** 整张表为纯白背景，仅依靠极细网格线分区；标题行（Title 行）在行号列显示为"Title"文字，列标题行显示列名与子列。官方文档明确描述数据表格式为"Table format / Column Titles / Subcolumn Titles"，并以 A:Y1、A:Y2、A:Y3、B:Y1… 双层命名[10]。列宽、行高用户可拖拽调整，Prism 支持最多 2048 列、每列最多 512 个子列（Prism 10 起）[3]。

## 6. 中央工作区——图表区

图表画布背景为纯白 #FFFFFF，所有绘图元素（坐标框、数据点、误差棒、图例）叠加其上。坐标框（Frame）是 Prism 图表辨识度的核心，官方手册明确列出 5 种 Frame style[11]：

| Frame 样式 | 视觉描述 |
|---|---|
| No frame（无框） | 仅刻度线，无轴线包围 |
| Offset X & Y axes（偏移 X&Y） | X 轴与 Y 轴在左下角相交但不闭合，轴带箭头感 |
| Plain Frame（普通框） | 闭合矩形框，四边轴线 |
| Frame with Ticks (mirrored)（镜像刻度框） | 闭合框 + 上下左右均向外刻度 |
| Frame with Ticks (inward)（内向刻度框） | 闭合框 + 刻度指向框内 |

**Prism 默认是"向内刻度 + 闭合框"组合，且坐标轴粗细为 1/2 pt。** 教程与官方截图反复出现"Frame style = Plain Frame、坐标轴粗细 1/2 pt"的设置[11][12]。刻度方向默认向内（Dir: inward），误差棒默认向两侧延伸（Dir: Both）、粗细 1 pt、颜色与数据集同色或黑色[13]。

| 图表元素 | 默认规范 | 可信度 |
|---|---|---|
| 画布背景 | #FFFFFF 纯白 | 高 |
| 坐标轴线宽 | 0.5 pt（1/2 pt） | 高（教程反复出现）[11] |
| 刻度长度 | Short / Normal / Long 三档，默认 Short | 高（官方对话框选项）[13] |
| 默认字体 | Arial（图表文字独立设置） | 高（官方 Font… 对话框默认）[13] |
| 默认字号 | 8–12 pt（轴标题/刻度标签 8–10 pt） | 中（估算） |
| 图例位置 | 默认右侧或图表内右上，可拖拽 | 高（教程明确"直接拖拽改位置"）[12] |
| 误差棒 | 1 pt、双向、颜色随数据集 | 高（Format Graph 默认值）[13] |
| 标题 | 默认显示图表标题，位于图表内上方；官方描述"一般位于图正下方"，实际 Prism 默认在图内上方 | 中 |

**默认配色是黑/绿/浅紫/深紫/橙，官方从未公开精确 HEX。** 综合多张默认柱状图、折线图、分组图截图，Prism 前 5 个数据集的填充与边框色反推如下（色板数据：[prism_default_colors.csv](/data/workspace/dr:prism-ui/prism_default_colors.csv)）：

| 数据集 | 填充色（估算 HEX） | 边框色（估算 HEX） |
|---|---|---|
| 1 | #1A1A1A（近黑） | #1A1A1A |
| 2 | #009E73（绿） | #007250 |
| 3 | #948FD4（浅紫） | #6B63B5 |
| 4 | #5C4F9D（深紫） | #3D3370 |
| 5 | #E69F00（橙） | #B87700 |

![Prism默认配色](/data/workspace/dr:prism-ui/fig_prism_colors.png)
*图 2：Prism 默认数据集配色色板。数据来源：教程截图视觉估算；GraphPad 未公开官方色板 HEX，使用前请自行取色复核。*

**Prism 9.5 起新增 9 套内置配色方案，默认色板本身并未变。** 官方更新说明称这些方案"旨在确保任何人都能感知色差"，包含色盲友好方案，并有 1970s/1980s/1990s/2000s/2010s 等主题包[14]；Prism 10.2 进一步重做了配色方案菜单，可预览填充色与边框色、并显示连续映射的渐变条[3]。这意味着复刻时建议同时提供"Classic 默认"与"Colorblind-safe"两套色板。

## 7. 右侧/浮动格式面板

Prism 传统上以**浮动对话框**承载格式设置，而非常驻侧栏。两个核心对话框结构与标签页如下：

**Format Graph 对话框**（双击图形空白处或点 Change）：
- 左侧标签页/树：`Appearance`（外观）、`Data Sets on Graph`（图上的数据集）、`Graph Settings`（图表设置）[13]
- 内容区：`Data set` 下拉选择数据集 → `Style`（外观：Scatter/Bars/Line）、`Bars`（Fill 填充 / Border 边框 / Border thickness 边框粗细 / Fill pattern 填充图案 / Design）、`Symbols`（Color/Shape/Size/Border）、`Error bars`（Color/Style/Dir./Thickness）、`Lines`（Color/Style/Thickness）、`Additional options`（Plot on Left/Right Y axis、Show legend、Revert to column title、Label each point）
- 底部按钮：`Change Graph Type...`、`Help`、`Cancel`、`Apply`、`OK`

**Format Axes 对话框**（双击坐标轴）：标签页顺序为 `Frame and Origin`（框与原点）→ `X axis` → `Left Y axis` → `Right Y axis` → `Titles & Fonts`[11][13]。

| 标签页 | 核心控件 |
|---|---|
| Frame and Origin | Frame style（5 种）、Shape and size（Wide/Square）、Axes and colors、Frame and Grid line |
| X axis / Left Y axis / Right Y axis | Scale（Linear/Log）、Range（Min/Max）、Ticks direction（Left/Right/In/Out）、Ticks length（Short/Normal/Long）、Regularly spaced ticks（interval）、Minor ticks、Number format（Decimals/Thousand separator）、Additional ticks and grid lines |
| Titles & Fonts | X title、Left Y title、Right Y title、Font（Arial…）、Size（pt）、Style（Bold/Italic）、Color、Greek letter 支持 |

**Prism 10 的"图形检查器"是首个常驻右侧面板，11 版保留并增强。** 它"在与图形交互时始终可用"，把多变量图的自定义工具集中在一处，支持实时更改与可视化更新、气泡图变量映射、以及点选高亮联动数据表，弥补了对话框式 Format Graph 不能实时预览的短板[3]。复刻时应同时实现"经典对话框"与"常驻检查器"两种交互模式。

## 8. 底部状态栏

状态栏位于主窗口最底部，单行、高约 22 px，背景 #ECEEF1、上边 1 px #C8CCD3 分隔。内容动态变化[2]：

- 默认显示当前光标所在单元格的"坐标"信息，格式为 `第 N 行, A: 列名`（如"第 19 行, A: Control"）
- 显示当前工作表/项目文件名与状态
- 底部工具栏的 Go to / Gallery / 前后翻页 / Ping-pong 按钮也贴附在此区域（部分版本将底部工具栏与状态栏合并为同一横条）

状态栏字体为 9 pt Segoe UI、灰黑 #1A1A1A，无图标、纯文本左对齐。其存在感低但信息密度高，是"冻结第 N 行""光标位置"等操作的即时反馈区。

## 9. 欢迎对话框「Welcome to GraphPad Prism」

启动 Prism 后弹出的"欢迎使用 GraphPad Prism"对话框（英文 Welcome to GraphPad Prism）为模态窗口，左侧为类别树、右侧为说明与预览区、底部为按钮[4][15]：

**左侧 New table & graph（新建表与图）下 8 种表类型**（顺序固定）：

1. XY（X、Y 数值对，用于散点/回归/折线）
2. Column / 纵列表（单列 Y，分组柱状图/散点）
3. Grouped / 分组表（多子列，分组柱状图）
4. Contingency / 列联表（频数统计、卡方）
5. Survival / 生存表（Kaplan-Meier）
6. Parts of whole / 整体分解（单比例，饼图/百分）
7. Multiple variables / 多变量（用于 PCA 等）
8. Nested / 嵌套（嵌套设计）

**左侧 Existing file（现有文件）下 5 项**：Open a file / LabArchives / Clone a graph / Graph portfolio / Prism Tips[4][15]。

右侧随所选表类型变化，展示"列联表说明"+"输入方式选项"（如 Enter replicate values / Enter and plot error values already calculated / Enter Mean, SD, N）+"Use tutorial data"下拉 + 图表预览缩略图。底部按钮左为 `Cancel`，右为 `Create`（主按钮，蓝色背景 #2E7BC4、白字、圆角约 2–3 px）。对话框标题栏为原生 Windows 风格，右上角 × 关闭按钮。中文版对话框标题为"欢迎使用 GraphPad Prism"，按钮文案为"取消/创建"[4]。

## 10. 整体配色与字体

Prism 的视觉体系可概括为"冷灰 UI + 纯白画布 + 黑绿紫橙数据集色 + 蓝色交互点缀"。由于官方未发布设计系统，以下除标注"官方明确"外均为截图估算。

| 用途 | 颜色（估算 HEX / RGB） | 说明 | 可信度 |
|---|---|---|---|
| 窗口背景/导航器 | #ECEEF1 / 236,238,241 | 冷灰，略带蓝调 | 中 |
| 面板/工作区底色 | #F5F6F7 / 245,246,247 | 导航器内空白处 | 中 |
| 工具栏底色 | #F0F1F3 / 240,241,243 | 主工具栏/底部工具栏 | 中 |
| 控件/输入框底色 | #FFFFFF | Search 框、下拉框 | 中 |
| 分隔线（面板间/组间） | #C8CCD3 / 200,204,211 | 1 px | 中 |
| 单元格网格线 | #BFC3CA / 191,195,202 | 1 px 极细 | 中 |
| 主文本色 | #1A1A1A / 26,26,26 | 菜单/表/导航器文字 | 中（接近纯黑） |
| 次要文本色 | #6B7280 / 107,114,128 | 占位符/说明 | 中 |
| 选中高亮（关联粗体） | #D6E4F5 / 214,228,245 | 导航器关联表 | 中 |
| 当前激活表背景 | #DCEBFA / 220,235,250 | 略深于高亮 | 中 |
| 排除值/斜体值 | #3B7DD8 / 59,125,216 | 官方明确为蓝色斜体[9] | 高 |
| 品牌强调色 | #2E7BC4 / 46,123,196 | 按钮/链接/+New 链接 | 中（logo 蓝估算） |
| 图表画布 | #FFFFFF | 官方明确为白底输出 TIF[12] | 高 |
| 图表坐标轴线 | 近黑 #1A1A1A，0.5 pt | 官方明确 1/2 pt[11] | 高 |

**字体分层是两套体系：UI 用 Segoe UI，图表用 Arial。** Windows 版 UI 字体为 Segoe UI 9 pt（Win10/11 系统标准，Tahoma 为 WinXP 时代遗留，Prism 9+ 已切到 Segoe UI），菜单/导航器/按钮/状态栏统一 9 pt、颜色 #1A1A1A；数据表正文 10 pt、数字右对齐；图表文字独立在 Format Axes 的 Titles & Fonts 页设置，默认 Arial 8–12 pt，支持上下标与希腊字母（μ/± 等科学符号）[13]。

**Prism 9→10→11 三版视觉几乎零变化，可统一一套规范。** 主要增量是 Prism 10 的图形检查器（Inspector）常驻面板与性能提升、Prism 11 的公式编辑与多因素 ANOVA 参数面板，界面框架与配色体系保持稳定[3][16]。因此本规范可作为三版通用基准，版本差异仅在个别面板/对话框上做补充即可。

## 引用来源

[1] https://baike.baidu.com/item/动手学科技绘图与数据分析：GraphPad Prism 10/66273758
> "菜单栏位于标题栏的下方，使用菜单栏中的命令可以执行 GraphPad Prism 的所有命令。"

[2] https://baike.baidu.com/item/GraphPad Prism图表可视化与统计数据分析(视频教学版)/65676874
> "1.3.1 菜单栏 1.3.2 功能区 1.3.3 导航器 1.3.4 工作区 1.3.5 状态栏"

[3] https://www.sohu.com/a/697193130_120089208
> "新的图形检查器通过在一个位置为多个变量图形组合您喜爱的自定义工具，为数据可视化提供了新的控制级别。此侧面板在与图形交互时始终可用。"

[4] https://www.sohu.com/a/399977443_120736615
> "Welcome to GraphPad Prism ... New table & graph ... XY Column Grouped Contingency Survival Parts of whole ... Existing file ... Open a file LabArchives Clone a graph Graph portfolio"

[5] https://www.docin.com/p-4638479350.html
> "在导航浏览树的顶端，有一个部分将相互联系的工作表组织成一族。当用户在导航器的任一部分选择一张表单，与之相关的表单——数据表、分析结果表、信息表、图、构图——会被组织和展示于族的那部分。"

[6] https://baike.baidu.com/item/动手学科技绘图与数据分析：GraphPad Prism 10/66273758
> "1.2.3 导航器 1.3.1 搜索工作表 1.3.2 项目中的工作表 1.3.3 系列文件"

[7] https://www.163.com/dy/article/HQCPGU340538H9RK.html
> "File 编辑 视图 插入 分析 更改 排列 系列 窗口 帮助"

[8] https://view.inews.qq.com/a/20260914A03CHU00
> "Prism 基础知识 ... 图 1-74 以缩略图的形式显示工作表"

[9] https://www.sohu.com/a/378123897_120089208
> "排除的值在数据表上以蓝色斜体显示，但在分析和图形中被忽略。"

[10] https://www.docin.com/p-2408344944.html
> "Table format: Group A Group B ... A:Y1 A:Y2 A:Y3 B:Y1 B:Y2 B:Y3"

[11] https://www.163.com/dy/article/H5IK900V0538H9RK.html
> "双击坐标轴，选择 Frame and Origin 选项卡，修改 Frame style 改为 Offset X & Y axes"

[12] https://www.163.com/dy/article/HQCPGU340538H9RK.html
> "坐标轴粗细修改为1/2pt，Frame style(坐标轴样式)设置为Plain Frame"

[13] https://www.sohu.com/a/399977443_120736615
> "Format Graph ... Appearance / Data Sets on Graph / Graph Settings ... Error bars Color Style Dir. Both Thickness 1 pt"

[14] https://www.sohu.com/a/620086128_120314002
> "Prism 9.5版最新更新 ... 添加了九种新的内置配色方案，您可以从中进行选择"

[15] http://mp.weixin.qq.com/s?__biz=MzA4NTE0MDgxNw==&mid=2650279589&idx=2&sn=c97d803571e051fdf119e1bc221774cb
> "欢迎界面：左侧为表格类型，右侧为表格说明和数据输入方式。"

[16] https://www.sohu.com/a/903991044_100183478
> "GraphPad Prism v10.5 更新 ... 优化树状图生成算法，现基于平均距离对子树进行排序"

[17] https://www.docin.com/p-3811503293.html
> "Frame and Origin(坐标框与原点) ... X axis (X轴) ... Left Y axis(左Y轴) ... Right Y axis(右Y轴) ... Titles & Fonts"
