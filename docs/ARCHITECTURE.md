# StatAura 架构

> 目标：一条可验证的数据 → 统计 → 矢量图 → 导出链路；每层都能独立测试。

---

## 1. 分层

```
            ┌──────────────────────────── app（egui 桌面壳）────────────┐
            │ topbar · sidebar · grid · canvas · inspector · results   │
            │ dialogs · modals · theme · icons · widgets              │
            └───────────────┬───────────────────────────┬─────────────┘
                            │                           │
                     core（纯数据）               plot（纯图形）
              model / table / stats / analysis     scene / text / axis
              palette / sample / color             chart / extras / export
                            │                           │
                            └────────────── io ─────────┘
                         .aura 工程 · CSV · PNG/SVG/PDF
```

依赖方向严格单向：`app → plot → core`，`app → io → core/plot`。
`core` 与 `plot` 都不依赖 egui，因此可以：

- 在 CLI / 批量出图里复用；
- 用 `examples/smoke.rs` 在没有窗口的环境里跑完整链路；
- 单元测试不需要构造 UI。

---

## 2. 数据模型

```rust
DataTable {
    id, name, kind,
    columns: Vec<Column>,   // 第 0 列可能是 X（role = X）
    group_names: Vec<String>,
    row_headers: Vec<String>,
}
Column { name, group, role, cells: Vec<Cell> }
Cell   { num: Option<f64>, text: Option<String>, excluded: bool }
```

设计取舍：

- **统一列数组**：XY / Survival 的 X 也是 `columns[0]`，网格、粘贴、CSV 导出只有一套索引逻辑。
- **group 字段**：XY 的重复子列（Y1/Y2/Y3）共享同一个 `group`，分析时按组聚合；Grouped 的每个列是一个组。
- **row_headers**：分组表的组 1、构成表的类别、生存表的受试者。不塞进 columns，避免网格出现一个语义不明的「行名列」。
- **排除值**：`excluded` 保留数值用于显示，但 `Cell::value()` 返回 `None`，统计与绘图自动忽略。
- **稀疏 vs 定长**：v0.1 用定长 `Vec<Cell>`，10 万行以内足够；换稀疏索引是后话。

七种表的模板、网格列头、分析分组都由 `TableKind` 分支集中处理：

| 能力 | 位置 |
|---|---|
| 初始模板 | `DataTable::template` |
| 网格列头 | `app/grid.rs::column_views` |
| 统计分组 | `DataTable::analysis_groups` |
| 绘图数据 | `plot/chart.rs::build_groups` / `build_xy` |
| CSV 表头 | `io/project.rs::export_csv` |

---

## 3. 统计内核

全部为纯函数，输入 `&[f64]`，输出结构体；无隐藏状态。

### 3.1 数值基础

- `ln_gamma`：Lanczos 近似；
- `betai`：正则不完全 beta（连分式，300 次迭代上限，1e-12 收敛）；
- `norm_cdf/erfc`：Numerical Recipes 近似，相对误差 < 1.2e-7；
- `norm_ppf`：Acklam 有理逼近；
- `t_cdf/t_p/t_quantile`：由 `betai` 与二分法得到；
- `f_sf`：`I_{df2/(df2+d1F)}(df2/2, df1/2)`。

### 3.2 检验

| 检验 | 关键公式 |
|---|---|
| Welch t | `SE=√(s1²/n1+s2²/n2)`，Welch–Satterthwaite df |
| Student t | 合并方差，`df=n1+n2−2` |
| 配对 t | 差值均值 / (sd/√n)，df=n−1 |
| ANOVA | SSB/SSW，F=MSB/MSW，η²=SSB/(SSB+SSW) |
| 事后 | 每对 Welch t，Holm 逐步校正（单调化） |
| Mann–Whitney | 秩和 DP 精确分布（无并列、n≤25），否则正态近似 + 并列校正 |
| Wilcoxon | W+ 子集和 DP 精确分布（无并列、n≤30），否则正态近似 |
| 相关 | Pearson r，`t=r√((n−2)/(1−r²))`；Spearman 先取秩 |
| 回归 | OLS，`SE(slope)=√(MSE/Sxx)`，t 分布 CI |
| 正态性 | D'Agostino–Pearson K²（偏度 Z1、峰度 Z2），p=exp(−K²/2) |

### 3.3 呈现

`core/analysis.rs` 负责编排与格式化，产出 `ResultTable { title, method, columns, rows, note }`。
方法说明永远和数值一起展示，避免「只有 p 值没有口径」。

---

## 4. 绘图内核（SPE）

### 4.1 场景 IR

```rust
Scene { width, height, bg, ops: Vec<Op> }
Op    { path: Path, fill: Option<Rgba>, stroke: Option<Stroke> }
Path  { segs: Vec<Seg> }  // Move/Line/Quad/Cubic/Close
```

- 所有坐标最终是**像素**，由 `px_per_pt` 从物理尺寸换算；
- 文字在构建场景时已经由 `ab_glyph` 转成轮廓并 `Transform`（平移/旋转）；
- 后端只有三个：`raster`（tiny-skia）、`svg`、`pdf`；
- 因为 IR 里没有「文字」，SVG/PDF 不需要字体嵌入，也不会出现排版漂移。

### 4.2 版式

`chart.rs::frame_layout` 先从刻度标签宽度、轴标题高度、图例尺寸反推边距：

```
left   = pad + max(y tick label width) + gap + y-title height
bottom = pad + x-title height + tick length + tick label height
top    = pad + title height
right  = pad + (outside legend width) + 末位刻度标签半宽
```

分类轴如果标签宽度 > 槽宽，自动转 -34° 并增加底部/右侧留白。
绘图区的数据坐标映射：

```
x_px = plot.x + axis.norm(x) * plot.w
y_px = plot.y + (1 - axis.norm(y)) * plot.h
```

`Axis::norm` 内部处理线性/对数/反向；对数轴只接受正值。

### 4.3 图元与细节

- 1-2-5 取整刻度；线性轴按 `step` 决定小数位；对数轴按 10^k 仅标注十进位；
- 柱状图从 0 基线出发，负值自动翻转；误差棒端帽宽 4.5 pt；
- 抖动是**确定性哈希**（`stats::jitter`），重绘不会跳；
- 箱线：type-7 分位数、Tukey 1.5×IQR 须、离群点；
- 小提琴：Silverman 带宽 KDE，两端外扩 8% 让密度自然收敛；
- KM：事件时间处先水平后垂直，曲线延伸到最后一个观测（含删失）；
- 热图：行标题相同的行取均值，颜色从浅到深，右侧色标；
- 显著性括号：按轴顶部余量均分堆叠，星号用基线对齐（避免星号悬空）；
- 命中测试：渲染时同时输出 `Hit { at, title, lines }`，UI hover 时取最近点。

### 4.4 导出

| 后端 | 要点 |
|---|---|
| raster | tiny-skia `Pixmap`，白底，RGBA 缓冲 → image 编码 |
| SVG | 纯 `<path>`，`fill-opacity` / `stroke-dasharray` / `linecap` |
| PDF | 1.4 最小写入器：CTM 做 y 翻转、`m/l/c/h`、`rg/RG`、`w/J/j/d`、`f/S/B`、ExtGState 透明度、xref |

PDF 的页面尺寸是物理 pt（6.5 in = 468 pt），内容坐标用 `k = width_pt/width_px` 缩放。
`examples/smoke.rs` 会检查 PDF 头、xref 偏移、每个 stream 的 `/Length` 与 `endstream` 对齐。

---

## 5. 渲染调度

```
spec + table ──rev++──►  Job{spec, table, scale}
                              │  后台线程（latest-wins）
                              ▼
                      SPE → RGBA → Done
                              │
        poll：只接受 in_flight 的 id，上传为 egui texture
```

- `chart_rev` 只在数据/规格/选中图变化时递增；鼠标移动、滚动、面板重排不触发；
- 渲染线程从 channel 里取任务时先 `try_recv` 清空，只画最新一份；
- 纹理上传后旧纹理被替换；菜单/拖动期间仍显示上一帧，不闪烁；
- 预览尺寸取自画布宽高，导出重新按目标 dpi 渲染，不复用预览纹理。

典型耗时（本机，release）：800×600、4 组柱 + 40 点 ≈ 1–2 ms；300 dpi 1950×1325 ≈ 13 ms。

---

## 6. 应用层

### 6.1 布局

根 `Ui` 手动切五块：顶栏 54、状态栏 26、侧栏 232、检查器 304、中间自适应。
每块用 `scope_builder(max_rect)` 建立独立子 `Ui`，并给 rect 派生的 `id_salt` 防止控件 ID 冲突。

### 6.2 数据网格

- 自绘，不用 `egui_extras::Table`，因为需要整列选择、冻结行头、单元格编辑、排除值斜体；
- 只画视口内的行/列；`scroll_x/scroll_y` 手动维护；
- 单元格文本用 `layout_no_wrap` + 截断，绘画器 clip 到 body，避免溢出到滚动条；
- 编辑态用 `TextEdit` 覆盖在单元格 rect 上，Enter 提交并下移，Esc 取消，失焦提交；
- 普通输入直接起编辑并带入首字符；
- 右键先把光标定位到点击格，再应用「排除/恢复/插入/删除/复制/粘贴」。

### 6.3 撤销

快照式：`undo: Vec<Project>`，每次结构性修改前 `clone`，上限 80；`redo` 对称。
对 10 万行表 clone 偏重，但编辑是低频操作；后续可换成「操作日志 + 区间补丁」。

### 6.4 自动保存

45 秒空闲触发一次 `.aura` 原路径写入；原子写（临时文件 + rename）。
标题栏 `●` 与状态栏红点表示未保存。

---

## 7. 工程文件

```json
{
  "format": 1,
  "name": "示例 · 药物对体重与生存的影响",
  "tables": [ ... ],
  "analyses": [ ... ],
  "graphs": [ ... ],
  "next_id": 42
}
```

- `format` 用于版本判断；未来字段用 `#[serde(default)]` 兼容；
- 图形规格、分析结果全部内嵌，打开即可复现；
- 写入原子化；读取失败给出中文错误。

---

## 8. 测试

| 层 | 方式 |
|---|---|
| 统计 | 29 个单元测试：已知 t/F/ANOVA/U/W/r、type-7 分位数、CI |
| 刻度/排版 | 1-2-5、对数刻度、字体测量、字形轮廓非空 |
| 数据层 | 模板矩形性、排除值、TSV 粘贴、数字格式 |
| 工程/导出 | `examples/smoke.rs`：保存→读取→全部分析→每图 PNG/SVG/PDF→CSV 往返 |
| 视觉 | `examples/gallery.rs` 生成 13 张样张，人工/后续像素基线 |

---

## 9. 已知取舍

- **不引入 plotters**：旋转文字、CJK、抗锯齿、样式天花板都不满足；详见 README 的对照实验。
- **文字转曲**：文件更大（单图 PDF 60–90 KB），换来零字体依赖；长文本会显著变大。
- **PDF 不压缩**：v0.1 直接写未压缩流，便于排查；后续加 FlateDecode。
- **定长单元格**：实现简单，10 万行以上内存偏高。
- **快照撤销**：简单可靠，超大表需要换增量。
- **单线程渲染**：一帧只跑一个任务；图很多时可再加 worker 池。
