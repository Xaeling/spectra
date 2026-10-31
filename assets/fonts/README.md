# 内置字体

三个字体文件都通过 `include_bytes!` 打进可执行程序，**运行时不依赖系统字体**。

| 文件 | 字体 | 用途 |
|---|---|---|
| `MiSans-Regular.ttf` | Mi Sans | 软件界面（中文） |
| `SimSun.ttf` | 宋体 | 导出的图形：中文 |
| `TimesNewRoman.ttf` | Times New Roman | 导出的图形：英文 |

## 导出规则

图表文字按**字符**自动切换字体：连续的 CJK 段用宋体，其余用 Times New Roman。
所以 `浓度 (mol/L)` 会渲染成「浓度」宋体 + 「(mol/L)」Times New Roman，
符合中文期刊「中文宋体 + 英文 Times New Roman」的通行规范。

TikZ 后端生成的 `.tex` 用 `xeCJK` + `fontspec`，
Matplotlib 后端生成的 `.py` 通过 `rcParams['font.serif']` 设置同样的字体栈。

## 界面字体没生效？

charis-ui 只能使用**系统可见**的字体。程序启动时会尝试用 Win32
`AddFontMemResourceEx` 把 Mi Sans 注册为进程私有字体，但该 API 只作用于 GDI，
部分字体栈（如 DirectWrite）读不到。

若界面出现方块字，二选一：

1. 双击 `MiSans-Regular.ttf` → 「安装」，装到系统字体目录；
2. 把 `src/fonts.rs` 里的 `APPLY_UI_FONT` 改成 `false`，回到系统默认 UI 字体。

## 替换字体

直接替换同名文件即可，其余代码不用动。若想换成 `仿宋_GB2312` 之类，
把文件改名成 `SimSun.ttf` 覆盖即可（代码里只认文件名）。
