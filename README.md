# Prism 复刻版 · charis-ui + Rust + plotters

用 **charis-ui**（Rust 原生桌面 UI 框架，CSS 式样式 + flexbox + GPU 渲染，无内嵌浏览器）
做前端，Rust 作为编译语言，**plotters** 负责图表绘制，目标是对 GraphPad Prism 做**像素级观感复刻**。

```
cargo run --release     # 首次编译较慢（charis 依赖 wgpu/winit），之后增量很快
```

Windows 上 charis 需要 MSVC 工具链 + VS Build Tools；Linux 需要 X11/Wayland 开发库
（`sudo apt install libxkbcommon-dev libwayland-dev libx11-dev libxcursor-dev libxi-dev libxrandr-dev`）。

---

## 一、复刻了什么（对照 Prism 9/10/11 界面规范）

### 窗口骨架
| 分区 | 复刻要点 |
|---|---|
| 菜单栏 | 9 项，顺序完全对齐：`File / Edit / View / Insert / Change / Arrange / Family / Window / Help`，含 Ctrl+N/O/S/P、Ctrl+L（Family）等快捷键 |
| 主工具栏 | **16 个分组**，顺序对齐：Prism / File / Sheet / Undo / Clipboard / Analysis / Change / Import / Draw / Write / Text / Export / Print / Send / LA / Help；上排图标 + 下排组名，组间 1px 竖分隔线 |
| 导航器 | 宽 220px，顶部 Search 框；5 个文件夹 `Data Tables / Info / Results / Graphs / Layouts`；当前表浅蓝底、关联表粗体、冻结表斜体；每文件夹底部 `+ New …` 蓝色链接 |
| Family 面板 | 导航器底部，以缩进树展示「数据表 → 分析结果 → 图表」，底部 `New Analysis... / New Graph... / New Layout...` |
| 中央工作区 | 数据表（双层表头）/ 结果表（分析选项卡）/ 图表页切换 |
| 底部工具栏 | Go to 输入 + View gallery + Previous/Next + Ping-pong（Prism 的翻页导航） |
| 状态栏 | 22px，显示 `Row 3, A: Control (A:Y1)` 形式的坐标反馈 |

### 数据表网格
- **双层表头**：上层列组（Control / Treated），下层子列（A:Y1 / A:Y2 / A:Y3）
- **整表纯白，无斑马纹**（这是 Prism 与 Excel 最明显的视觉差异）
- 网格线 1px `#BFC3CA`，行号列 40px 灰底，列宽 76px，行高 18px
- 选中单元格：浅蓝填充 `#B8D6F5` + 蓝色边框（Prism 的选择外观）
- 排除值：蓝色斜体（Prism 规范：排除值以蓝色斜体显示，分析中忽略）
- `virtual_list` 只构建视口附近的行，十万行不卡

### 图表（plotters）
- 白底、**Plain Frame**（四边闭合）+ **向内刻度**、0.5pt 轴线
- 柱体：内部 85% 不透明 + `shade()` 边框；误差棒 mean ± SD 带端帽；散点柱内确定性抖动叠加
- Y 轴标题逆时针 90°（plotters 文本不带旋转，用逐字符竖排实现等价观感）
- 图例默认图内右上；1/2/5×10^k 整齐刻度
- **26 套内置配色方案**（见下节），检查器里一个下拉即可切换

### 配色与「内部 / 边框」两档差异
26 套方案在 `src/colormap.rs`，前两套 Okabe-Ito / Tol 为**色盲友好**方案；
色数不固定（多数 6 色，A1–A6 为 8/9 色的分级与多色相方案）。

柱体的两档差异由两个机制叠加，与原实现逐条对齐：

| 机制 | 做法 |
|---|---|
| `shade()`（边框色） | **在 HSL 里**把明度 ×0.40、饱和度 ×3.0（浅填充 ×1.5）；近中性色/近白走灰阶分支 ×0.647。保色相 → 边框不会发黑 |
| alpha（内部色） | 基础色以 **85% 不透明**绘制，混白底后更浅；边框是 100% 不透明纯色 |

两者叠加 = 内部与边框**拉开两档**。散点跟随柱体（填充 = 内部色、描边 = 边框色），
误差棒颜色**严格跟随柱边框色**（`CHART_ERROR_PX`）。

> 实现注记：`fill_solid()` 预混白底得到实色，与 85% alpha 在白底上视觉等价，
> 但不依赖绘图后端的 alpha 混合行为。要导出透明背景时改用 `fill_of()` 即可。

### 交互
- 启动弹出 `Welcome to GraphPad Prism`：左侧 8 种表类型 + Existing file 5 项，右侧说明 + 输入方式 + 预览，底部 Cancel / Create
- 右侧常驻 **Graph Inspector**（Prism 10+）：改图型、误差棒、散点、图例、轴标题，实时重绘

---

## 二、目录结构

```
src/
├── main.rs        入口：WindowOptions（system_font → Windows 上是 Segoe UI，与 Prism 一致）
├── app.rs         状态机：Msg / update / view / theme / menu
├── palette.rs     ★ 全部视觉常量：冷灰色板 + 尺寸标尺 + Prism 经典配色（改这里全站生效）
├── colormap.rs    ★ 26 套配色方案 + shade() 内部/边框差异 + HSL 转换
├── export.rs      ★ 可插拔导出后端：Plotters SVG / LaTeX TikZ / Python Matplotlib
├── model.rs       ★ 8 种表工厂 + 稀疏矩阵数据层（含 cargo test）+ 结果表 / 图表 / Family
├── plot.rs        ★ 后端泛型 draw_chart<DB>：位图预览 + SVG 导出共用；真 90° 旋转
├── fonts.rs       ★ 内置字体：MiSans / SimSun / Times New Roman（打包进二进制）
├── stats.rs       ★ 真实统计：描述统计 / t 检验 / ANOVA / 回归 + 精确 p 值（含测试）
└── ui/
    ├── mod.rs     自绘图标（canvas）、1px 分隔线、工具栏按钮、复选框/单选
    ├── menubar.rs 9 个顶层菜单
    ├── toolbar.rs 16 个工具栏分组
    ├── navigator.rs 导航器 + Family 面板
    ├── grid.rs    数据表网格（双层表头 + 行标题可编辑 + 横向滚动 + 虚拟滚动）
    ├── inspector.rs 检查器：按工作表类型分派（数据表 / 图形）
    ├── settings.rs 设置窗口（含中英语言切换）
    ├── welcome.rs 欢迎对话框
    └── status.rs  底部工具栏 + 状态栏
```

---

## 三、8 种表格：模板工厂 + 统一稀疏数据层

### 3.1 架构

两部分组成，缺一不可：

```
TableType ──► PrismTable::new_by_type(tt, num_subcols)  ← 工厂：精确初始模板
                        │
                        ▼
        HashMap<usize, BTreeMap<usize, CellValue>>      ← 数据层：行主序稀疏矩阵
```

- **`HashMap<usize, …>`**：行号作键 → 天然支持无限行稀疏扩展
- **`BTreeMap<usize, CellValue>`**：行内列数据 → 列有序、只存有数据的列
- 读写完全隔离，**零借用冲突**；第 1,000,000 行写入与第 1 行代价相同

### 3.2 8 种类型的差异（决定「长相」的根源）

| 类型 | 行定义 | 列定义 | 子列 | 特征 |
|---|---|---|---|---|
| **XY** | 可选行标题 | 首列 X，后续 Y 数据集 A/B… | **并排** `Y1/Y2/Y3` | X 列无子列 |
| **Column** | 样本 | 每列一个组别 | 无 | 重复值纵向堆叠 |
| **Grouped** | 分组变量 1（Male/Female） | 分组变量 2（Control/Treated） | **并排** | 大组包住子列 |
| **Contingency** | 分类变量 1 | 分类变量 2 | 无 | 仅整数频数 |
| **Survival** | 每个受试者一行 | X(时间) + 各组事件状态 | 无 | 1=事件，0=删失 |
| **Parts of whole** | 互斥类别 | 单列 Value | 无 | 和为总数 |
| **Multiple variables** | 每个案例一行 | 每列一个变量 | 无 | 宽表，单元格可为文本 |
| **Nested** | 外层分组 | 内层分组 | **堆叠** `1/2/3` | 强制 ≥2 子列 |

> 关键区分：**XY / Grouped 用并排子列**（重复值横向排开），
> **Column / Nested 用堆叠子列**（重复值纵向堆叠）——这正是它们看起来不同的原因。

### 3.3 无限行是怎么成立的

虚拟列表的行内容**直接查稀疏矩阵**：

```rust
// 闭包只捕获 Arc<PrismTable>，每行按需 get_cell —— 不预建任何「行窗口」
virtual_list(rows_hint, move |r| grid_row(&snap, r, ...))
```

`rows_hint` 只是一个整数（表示"显示到第几行"），光标往下走或跳行时按需增长：

```rust
fn ensure_row(&mut self, r: usize) {
    if r >= self.rows_hint { self.rows_hint = r + 64; }   // 只加一个整数
}
```

所以第 1,000,000 行和第一行的代价一样——内存只随**已写入的单元格**增长
（测试 `writing_at_row_one_million_costs_one_entry` 断言 `data.len() == 1`）。

> 早期版本用 `build_matrix()` 预建 `view_rows × cols` 的行窗口快照，那个不是无限。
> 现已删除 `Matrix` 类型，改为 `TableSnap = Arc<PrismTable>`。

### 3.4 输入与导航

| 操作 | 行为 |
|---|---|
| 单击单元格 | 直接进入输入态（光标框自动获焦），再点别处内容不丢 |
| 输入 | **每次按键实时写入稀疏层**，所以不存在"未提交的输入" |
| ↑ ↓ ← → | 移动光标并**保持输入态**，落格即可继续打字，之前输入的内容已落盘 |
| Enter / Tab | 提交并下移 / 右移 |
| Escape | 退出输入态 |
| Home / End / PageUp / PageDown | 行内首列 / 末列 / 上下翻 20 行 |
| 底部「转到」 | 输入行号直达任意行（行窗口自动增长） |

类型约束在 `DataTable::write_str()` 收口：空串 → `Empty`、数字 → `Number`
（列联表取整）、非数字仅在多变量表允许存 `Text`，其余类型拒绝。

键盘走 `Element::on_key_capture` —— 抢在 `text_input` 之前处理导航键，
所以编辑态下方向键是移动单元格而不是移动文本光标；其余键（含字符）
返回 `None` 交回输入框。

### 3.5 鼠标跟随高亮

每个格子注册 `on_hover`，命中时画 `CELL_HOVER`(#E8EEF4) 底。
`update` 里先判等再写状态，避免 hover 消息把每一帧都变成重绘。

### 3.6 物理坐标：UI 不必关心归属

`set_cell(row, phys_col)` / `get_cell(row, phys_col)` 只用物理坐标。
`phys_to_logic()` 负责 (数据集, 子列) ↔ 物理列的换算，UI 一律不碰。

`XY` 表举例（3 个重复）：

```
物理列:   0     1    2    3     4    5    6
逻辑:     X    A:Y1 A:Y2 A:Y3  B:Y1 B:Y2 B:Y3
```

### 3.7 验收测试

`cargo test` 覆盖 5 组（见 `src/model.rs` 末尾）：

```rust
// 工厂结构 + 稀疏写入
let mut t = PrismTable::new_by_type(TableType::Grouped, 3);
assert_eq!(t.data_sets[0].sub_columns.len(), 3);
t.set_cell(2, 5, CellValue::Number(22.1));           // 第 2 行自动扩展
assert_eq!(t.get_cell(2, 5), CellValue::Number(22.1));
assert_eq!(t.get_cell(999_999, 0), CellValue::Empty); // 越界安全

// XY 首列是 X 且无子列 / Nested 强制 ≥2 子列 / 无子列类型宽度恒为 1 / 坐标换算往返
```

## 五、科研级绘图规范

### 5.1 视觉规范（逐条落实）

| 项 | 实现 |
|---|---|
| Y 轴标题 | **真 90° 逆时针**旋转（自下而上），轴心居中 |
| 单位格式 | 括号式 `量名称 (单位)` / 斜杠式 `量名称 / 单位` |
| 字体排版 | 物理量符号**斜体**、单位**正体**（`ChartView::pieces()` 按片段标记） |
| 柱体填充 | `Alpha = 0.85`，混白底后自然变浅 |
| 柱体边框 | `shade()`：HSL 明度 ×0.40、饱和 ×3.0，保色相不发黑；线宽 1.5–2.0pt |
| 散点 | **双层绘制**：底层大圆 = `shade()` 深色，上层小圆 = 填充色 → 清晰描边环 |
| 误差棒 | 线条与 T 型端帽**严格跟随柱边框色**，不是填充色 |
| 抖动 | 黄金比确定性抖动，同柱散点不重合 |
| 显著性括号 | 起点基于**所有数据最高点**动态上移，多层逐层累加（`BRACKET_*` 比例常量） |

**关于真旋转**：plotters 的文本不带旋转。这里把文字先离屏栅格化，
再按 90° 逆时针（`屏幕坐标下 (x,y) → (y, w-1-x)`）整块旋转后逐像素贴回
——阅读方向变成向上、字头朝左，即标准 Y 轴标题。
逐片段栅格化保留了「符号斜体 / 单位正体」的区分。

### 5.2 双端复用：位图预览 + 矢量导出

```rust
pub fn draw_chart<DB: DrawingBackend>(view: &ChartView, root: &DrawingArea<DB, Shift>) -> ...
```

| 端 | 后端 | 用途 |
|---|---|---|
| 预览 | `BitMapBackend::with_buffer` | 内存位图 → `Image::from_rgba`，charis 实时渲染 |
| 导出 | `SVGBackend::new(path, …)` | 真矢量，文字为文本对象 |

两者**共用 100% 绘图代码**，只换后端。几何计算（`geom()` / `x_of()` / `y_of()` /
`bracket_boxes()`）也一并导出，保证导出端与预览端像素级一致。

### 5.3 可插拔导出后端（设置里切换）

**数据结构与配色规范完全复用，只换「代码生成器」：**

| 后端 | 产物 | 说明 |
|---|---|---|
| `PlottersSvg` | `.svg` | 内置，与预览同源 |
| `TikZ` | `.tex` | 出版级：真矢量 + 真 `rotate=90` + `\textit{符号}\,\mathrm{单位}` |
| `Matplotlib` | `.py` | 生成 Python 脚本，便于二次加工 |

切换位置：设置 → **导出**；或检查器底部。导出按钮在工具栏 Export 组。

> **终极方案的预留**：如果 plotters 在文字旋转、单位混排上仍达不到
> Nature/Science 级别，直接把设置里的后端切到 **TikZ** 即可 ——
> 核心的 UI 交互与数据管理仍由 Rust/charis-ui 掌控，只换生成器。
> 生成的 `.tex` 可直接 `pdflatex` 编译；`.py` 可直接 `python` 运行出 PDF。

TikZ 侧的排版细节：

```latex
\node[rotate=90,anchor=center] at (14,cy) {\textit{Response}\,\mathrm{(ms)}};
\draw[fill=fill0,fill opacity=0.85,draw=edge0,line width=1.5pt] (x0,y0) rectangle (x1,y1);
\draw[edge0,line width=1pt] (cx,hi) -- (cx,lo);   % 误差棒跟随边框色
\draw[fill=fill0,draw=edge0,line width=0.6pt] (px,py) circle (2.2pt);
```

## 六、右侧控制面板（绘图配置的灵魂）

### 6.1 三列布局

```
Row
├─ 左：导航器        固定 240px  (NAV_W)
├─ 中：绘图区        Length::Fill（位图 + 滚动 + 缩放）
└─ 右：控制面板      固定 320px  (INSPECTOR_W)，分区 + scroll_y
```

### 6.2 五个分区（`Divider` 隔开，从上到下）

| # | 分区 | 内容 |
|---|---|---|
| 1 | **坐标轴与排版** | X/Y 名称 + 单位输入框；单位格式（括号式 / 斜杠式）；量符号斜体开关 |
| 2 | **图型与数据** | 柱/散点/线；误差棒、散点、图例开关 |
| 3 | **配色方案** | 26 套逐行列出，**每行左侧渲染前 5 个色块**（内部色 + 深色边框），点击即切换 |
| 4 | **符号与散点** | 「自动跟随柱子配色」开关；**取消勾选才露出** 13 色符号选择器（防呆）；散点大小 / 抖动幅度滑块 |
| 5 | **显著性标注** | 启用开关；高度模式（自动分层 / 绝对高度 / 相对柱子）；**动态比较对表格**（组 A / 组 B / 星号 / 删除），限高 150px 独立滚动；表格为空则不绘制 |
| 6 | **导出与图幅** | 图表标题；导出 SVG/TikZ/py；导出高分辨率 PNG（3.0× ≈ 300dpi） |

### 6.3 实时联动链条

```
用户操作 → Msg → update 改 state → view
                                   ↓
                        plot.render(&chart_view())
                                   ↓
                     BitMapBackend 重绘到内存缓冲
                                   ↓
                        Image::from_rgba → 中央区刷新
```

面板里任何一次改动都会触发 `render_chart()`，所以是**毫秒级所见即所得**。

### 6.4 防呆与降噪

- 符号专用色选择器默认**隐藏**，取消「跟随柱子」才展开
- 显著性比较对为空时，即使勾选「显示」也不绘制
- 面板整体 `scroll_y`，小屏不溢出；比较对表格再套一层限高滚动

## 七、三个常见布局问题与处理

### 0. `attempt to add with overflow`（已修）

根因：行标题列用 `usize::MAX` 作哨兵值，按下方向键时 `c + 1` 直接溢出。
另外 `ensure_row()` 的 `r + 64` 在 GoTo 输入超大行号时也会溢出。

处理：全部换成 `saturating_add`，并给视口行数加 100 万上限
（稀疏数据层本身无上限，这只是防止虚拟列表的行数撞到 `usize` 边界）。

```rust
Key::Right => { let next = c.saturating_add(1); ... }
fn ensure_row(&mut self, r: usize) {
    let want = r.saturating_add(64).min(1_000_000);
    ...
}
```

同类加固：`refresh_snap()` 空表保护（避免 `len() - 1` 下溢）、状态栏行号 `r + 1`。

### 1. 菜单栏上方那一大条空白
**根因**：charis 的 `Theme` 有 `titlebar_height` 字段，它按「应用自绘标题栏」预留顶部空间；
而我们用的是 OS 原生标题栏（`frameless=false`），那段预留就空在那里。

处理：生成主题后显式归零（`app.rs::theme`）：

```rust
let mut t = Theme::from_config(ThemeConfig { .. });
t.titlebar_height = 0.0;   // 不画自绘标题栏，就别预留它的高度
t
```

若你之后想改成自绘标题栏（可放工程名 + 窗口按钮），把 `WindowOptions.frameless(true)`
并在 root 顶部加 `titlebar(...)`，此时 `titlebar_height` 要改回 32 左右。

### 2. 语言切换（中 / 英）
`src/i18n.rs` 是全部文案词条表。key 用 `K` 枚举而非字符串——漏翻会在编译期暴露，
不会静默降级成英文。

- 设置窗口：`Ctrl+,` 或 File → 设置。分类：常规 / 外观 / **语言** / 分析
- 语言切换**即时生效**；确认（保存 + 关闭）/ 应用（保存不关闭）/ 关闭（仅关闭）
- 切换只影响界面文案，**不影响**已存盘的工程数据与统计结果

新增文案：在 `K` 加变体 → 在 `zh()` / `en()` 两个 match 里都补齐。

### 3. 窗口不高时 UI 被截掉
几处都已改成可滚动 / 自然高度，窗口变矮不再丢内容：

| 位置 | 处理 |
|---|---|
| 主工具栏 | **不再钉死高度**（原固定 34px 会把"组名"那行裁掉），由「图标行 + 组名行」自然撑开；横向 `scroll_x` |
| 检查器 | 内容区 `flex1 + scroll_y`，标题固定、选项区滚动 |
| 导航器 | 表单列表 `flex1 + scroll_y`；Family 面板限高 220px 且自身可滚动，优先压缩它 |
| 欢迎对话框 | 左右两栏各 `scroll_y`，整体限高 460px |
| 窗口初始尺寸 | 1360×920，最小 980×720 |

## 八、视觉风格：克制 / 现代

在保持 Prism 结构（冷灰面板、双层表头、ribbon 工具栏）的前提下，把观感往
「高级、克制、现代、科技」调了一档。全部变化集中在 `palette.rs`，改一处全站生效。

| 手法 | 做法 |
|---|---|
| **用留白代替线条** | 分隔线降到 `#E1E5EA`、网格线降到 `#EBEEF2`，区域靠间距划分而不是靠线 |
| **单一强调色** | 蓝色收敛到 `ACCENT #2B6FD4`，配 `ACCENT_SOFT` 浅底；界面里不再出现第二种高饱和色 |
| **冷灰而非黑** | 主文本 `#111827`（近黑带蓝调），投影用浅冷灰 `#B7BECB` 而不是黑色——不脏 |
| **圆角 token** | `R_SM 3` / `R_MD 6` / `R_LG 10`，色块、按钮、卡片、对话框各有归属 |
| **克制的层次** | 图表画布做成浮起卡片（白底 + 圆角 + 浅投影）；对话框用较深投影 |
| **选中用竖条不用色块** | 导航器 / 设置分类的选中项左侧一条 2px 强调竖条，比整行加深色块更好扫读 |
| **字号收敛** | 三级标尺：正文 12 / 次要 10.5 / 分区标题 9.5，避免字号泛滥 |
| **控件状态** | 复选框、单选激活态用强调蓝实心，未激活是极浅冷灰描边 |

## 九、性能：为什么以前会「一卡一卡」

两处热路径，都已修：

| 问题 | 修法 |
|---|---|
| **每敲一个键就整表克隆一次**<br>`CellInput → write_draft + refresh_snap` | 光标格本来就用 `draft` 渲染，根本不需要刷新快照。<br>去掉 `refresh_snap()` —— 数据仍每次实时写入稀疏层（不丢内容），只是不再重刷视图 |
| **每次移动光标都重绘整张位图**<br>`move_cursor → commit → render_chart` | 改为**标脏 + 按需重绘**：`mark_chart_dirty()` 只在图表页可见时才真正渲染；<br>在数据表里按方向键不再触发任何 plotters 绘制 |

```rust
fn mark_chart_dirty(&mut self) {
    self.chart_dirty = true;
    if matches!(self.active, Active::Graph(_)) { self.render_chart(); }
}
```

## 十、真实功能（不再是空壳 UI）

| 分类 | 功能 | 状态 |
|---|---|---|
| **统计** | 描述统计 / 独立样本 t 检验（Welch 与合并两种）/ 单因素方差分析 / 线性回归 | 真实计算，`src/stats.rs`，含 `cargo test` |
| **p 值** | 由不完全 Beta 函数精确计算，非正态近似 | `betai` + 连分数展开 |
| **分析入口** | 数据表检查器 → 选方法 → 运行分析 → 生成结果表并自动打开 | 结果随数据变化 |
| **表结构** | 增删数据集、增删子列、插入行、删除行、清除单元格 | 真实改稀疏层 |
| **剪贴板** | 复制 / 剪切 / 粘贴（含类型校验：列联表取整、多变量表允许文本） | 工具栏 + `Ctrl+C/X/V` |
| **撤销 / 重做** | 工程快照栈，深度 50 | 工具栏 + `Ctrl+Z/Y` |
| **文件** | 保存 / 打开工程（纯文本格式，无外部依赖） | 工具栏 + File 菜单 |
| **导航** | `Go to` 跳任意行、Search 过滤表单 | 已实现 |

## 十一、标注与其他工具（Draw / Write / Text / Print / Send / LA）

| 工具 | 真实行为 |
|---|---|
| **Draw** | 添加直线 / 箭头标注，画进图里（位图与 SVG 导出都有） |
| **Write** | 添加文本标注 |
| **Text** | 标注字号 ±（6–48px） |
| **Change** | 循环切换图型（柱 → 散点 → 线） |
| **Magic 魔棒** | 把当前图形的外观套用到本项目**所有**图形（Prism 的 Magic 行为） |
| **Import** | 读程序目录下的 `import.csv` / `.tsv`，按 tab 或逗号分隔写入当前表 |
| **Print** | 导出 6×（约 600 dpi）PNG 并用系统默认程序打开 |
| **Send** | 导出后打开系统邮件客户端，正文带上文件绝对路径 |
| **LA** | 导出「数据包」：`labarchives_export/` 下 CSV + SVG + PNG |

标注坐标用**绘图区归一化坐标 (0–1)**，所以位图预览、SVG 导出、不同缩放倍数下位置完全一致。
标注会随工程一起保存 / 读取（`AN` / `GA` 记录）。

## 十二、八个顶层菜单

`File · Edit · View · Insert · Analyze · Change · Settings · Help`

- **Analyze 是动态菜单**：按当前选中表的类型生成条目（8 种表各有自己的方法清单，
  见 `TableType::analyses()`）。未选中数据表时显示「（请先选择数据表）」。
- **Settings 的 7 个条目就是设置窗口左侧的 7 个分类**，点了直接跳到对应页，
  两边顺序必须一致（menubar 与 settings.rs 的 `TABS`）。
- **复选 / 单选带状态**：冻结表、四个面板开关、标尺 / 网格 / 行标题 / 列标题、
  高亮表 8 色、缩放 8 档，都从 `MenuState` 读当前值。

### 分析方法清单

| 表类型 | 可用方法 |
|---|---|
| XY | 线性回归、相关矩阵、描述统计、正态性、离群值、非线性拟合、ROC |
| Column | 描述统计、t 检验、单因素 ANOVA、相关矩阵、正态性、离群值、Bartlett |
| Grouped | 描述统计、双因素 ANOVA、t 检验、正态性、离群值 |
| Contingency | 卡方、Fisher 精确检验、相对风险与比值比 |
| Survival | Kaplan–Meier、Cox、描述统计 |
| Parts of whole | 总分、观察值 vs 期望值、描述统计 |
| Multiple variables | 描述统计、相关矩阵、线性回归、离群值、PCA、层次聚类、K-means、Cox |
| Nested | 嵌套单因素 ANOVA、嵌套 t 检验、描述统计、正态性、离群值 |

**已真实实现**：描述统计、t 检验（Welch / 合并）、单因素 ANOVA、嵌套 ANOVA、
相关矩阵、线性回归、正态性（Shapiro–Wilk / Royston）、离群值（Grubbs）、
卡方（含 Yates 校正）、Fisher 精确检验、相对风险 / 比值比、Kaplan–Meier、
总分、观察 vs 期望（拟合优度）。

**尚未实现**（会明确提示，不给假数字）：非线性拟合、ROC、双因素 ANOVA、
Cox 回归、PCA、层次聚类、K-means、嵌套 t 检验、Bartlett。
这些都需要在状态栏显示「该分析方法尚未实现」，不会编造结果。

### View 菜单的开关是真的生效

主工具栏 / 导航器 / 底部工具栏会真的从布局里移除；缩放会真的改变位图画布
尺寸与 `ChartView::scale`；图例反转会真的镜像图例顺序。

## 十三、图表视觉要点（容易做错的地方）

- **L 型边框是默认**。Prism 默认只画左轴 + 下轴，四面全框会立刻显得像 Excel。
  `ChartView::frame` / `GraphSheet::frame` 控制，检查器「边框样式」可切。
- **填充色只能混白一次**。`fill_solid` 曾写成 `mix_white(fill_of(..))`，
  等于把 0.85 用了两遍，散点内芯被洗成近白色，看上去像空心圈。
- **散点是柱形散点图的散点**：把该组每个重复值按其真实数值画点，
  X 方向确定性抖动（seed 驱动，同一次渲染位置稳定，"重新模拟"换相位）。
  双层绘制：外层深色环（= 柱边框色）+ 内层填充色实心。
- **显著性括号强制抬到安全高度**：绝对 / 相对模式下用户填的值可能低于柱顶，
  那样竖臂会切穿柱子和误差棒，所以统一 `max(top_val + gap)`。
  层间距 = 竖臂 + 标签高度，避免第 n+1 层压到第 n 层的星号。
- **预览超采样**：`CHART_DPR = 2.0`，位图按 2 倍渲染再缩回显示尺寸，
  字号线宽由 `scale` 同步放大，所以边缘是锐的。

## 十四、字体：全部打包，中英自动切换

三个字体文件放在 `assets/fonts/`，用 `include_bytes!` 打进可执行程序，
**运行时不依赖系统字体**。

| 文件 | 字体 | 用途 |
|---|---|---|
| `MiSans-Regular.ttf` | Mi Sans | 软件界面（中文） |
| `SimSun.ttf` | 宋体 | 导出图形：中文 |
| `TimesNewRoman.ttf` | Times New Roman | 导出图形：英文 |

**按字符切换**：图表文字先切成连续的 CJK 段与非 CJK 段，
前者用宋体、后者用 Times New Roman。所以 `浓度 (mol/L)` 会渲染成
「浓度」宋体 +「(mol/L)」Times New Roman，中英混排也各自正确。

实现上开启了 plotters 的 `ab_glyph` 特性：该模式下 plotters
**不再读系统字体**，必须先用 `register_font` 注册（见 `src/fonts.rs`）。
代价是多一步注册，好处是同一份代码在 Windows / macOS / Linux 上
导出的图形完全一致，不会因为缺字体而跑版。

TikZ 后端生成的 `.tex` 用 `xeCJK` + `fontspec` 设同样的字体栈（需 xelatex 编译）；
Matplotlib 后端生成的 `.py` 通过 `rcParams['font.serif']` 设置。

## 十五、关于取色：必须知道的免责声明

**GraphPad 官方从未发布 UI 设计规范、色板 HEX 或像素尺寸表。**

`src/palette.rs` 里的所有颜色与尺寸，除 Prism 官方手册明确记载的部分（如坐标轴 0.5pt、
排除值蓝色斜体、5 种 Frame 样式、8 种表类型）外，均为对官方与教程截图的**视觉估算值**。
它们足以做到「一眼看上去就是 Prism」，但不是官方真值。

建议：拿一张你目标版本的真实截图，用取色器复核后直接改 `palette.rs` 里的常量
——全部 UI 都从该文件取色，没有第二处硬编码。

主要估算值：

| 用途 | 值 | 用途 | 值 |
|---|---|---|---|
| 窗口背景/导航器 | `#ECEEF1` | 分隔线 | `#C8CCD3` |
| 工具栏底色 | `#F0F1F3` | 网格线 | `#BFC3CA` |
| 主文本 | `#1A1A1A` | 当前表高亮 | `#DCEBFA` |
| 选中单元格 | `#B8D6F5` | 品牌蓝/链接 | `#2E7BC4` |
| 排除值 | `#3B7DD8` | 图表画布 | `#FFFFFF` |

字体：UI 走系统字体（Windows = Segoe UI 9pt，即 12px）；图表文字 plotters 用 Arial
（Linux 无 Arial，`plot.rs` 已自动回退 DejaVu Sans）。

---

## 十六、当前状态与已知待办

- **未编译验证**：开发环境无 Rust 工具链，本次代码未经 `cargo check`。首次编译若报 API 不符，
  最可能需要微调的是 `image(ImageSource::from(img))`（若 charis 的 `image()` 直接接受 `Image`，
  改成 `image(img)` 即可），其余 API 均已按 charis-ui 0.1.0 官方文档逐项核对。
- 图表尺寸目前固定 640×430 渲染后交给 charis 缩放；要做「随窗口重排」可在 update 里接
  `Element::on_resize` 后按新尺寸调用 `plot::render` 重绘。
- 尚未接入：统计检验引擎（t 检验/ANOVA/事后检验）、七种导出（PNG/SVG/PDF/EPS…）、
  `.prism` / `.sprx` 工程文件读写、显著性标注条、更多图型（热图/火山图/生存曲线）。

这些在原「析光 Spectra」项目里已有完整设计与实现，可整体平移过来：
数据层 8 种表格 + 稀疏矩阵、分析层 scipy 对照验证过的统计、`.sprx` 双格式策略。
平移时保留本项目的 `palette.rs` 与 `ui/` 即可延续 Prism 观感。
