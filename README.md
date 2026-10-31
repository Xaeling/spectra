# 本项目是一个独立实现的功能演示/学习项目，核心代码由 AI（Hunyuan 4.0 preview）生成，并经人工整理与测试。项目未使用原软件的任何源代码、二进制文件、素材、文档或商标，也不包含破解、绕过授权或分发原软件的内容。项目与原作者及官方无关联，未获其授权或背书。功能思路仅来自公开资料、通用需求或用户描述，代码与实现均为独立完成。本项目仅用于学习交流和技术演示，不用于商业用途；如相关权利人认为存在异议，请联系我们删除或调整。
# This project is an independent implementation for learning and technical demonstration. Its core code was generated with the assistance of an AI model (Hunyuan 4.0 preview) and then organized and tested manually. It does not use any source code, binaries, assets, documentation, or trademarks from the original software, and it does not include any cracking, license-bypassing, or redistribution of the original software. This project is not affiliated with, endorsed by, or authorized by the original developer or official team. Any functional inspiration comes only from public information, general requirements, or user descriptions, while the code and implementation are independently produced. This project is intended solely for learning, communication, and technical demonstration, not for commercial use. If any rights holder has concerns, please contact us for removal or adjustment.

# 析光 Spectra

## 中文

析光 Spectra 是一款面向科研工作者的桌面统计分析与科学绘图软件，以原生性能与本地化处理为核心：无需联网，数据不上传。

软件以数据表为组织单元，支持 XY、分组、列、嵌套、列联等多种表类型，录入数据后即可直接分析。

分析能力覆盖 60 种统计方法，包括描述统计、t 检验、单因素与嵌套方差分析、非参数检验、回归与 11 种非线性模型拟合、生存分析与多元统计。内置的智能统计制图推荐会先完成正态性与方差齐性诊断，再据此推荐合适的检验方法、图形表达与多重比较方案。

绘图方面提供 60 种图表类型，支持误差棒、散点叠加与显著性标注；标注采用期刊通行的分级标准并可显示精确 P 值。图形可导出 PNG、JPEG、TIFF、SVG、TikZ、Matplotlib 脚本等 13 种格式，矢量输出可直接用于论文排版。

工程以 .sprx 单文件保存，基于 ZIP 容器并内嵌无损快照，旨在再次打开时完整还原数据与图表状态。

## English

Spectra is a desktop application for statistical analysis and scientific plotting, built for researchers who work with experimental data. It runs entirely offline — no account, no upload, no telemetry.

Data is organized into typed tables (XY, grouped, column, nested, contingency). Once values are entered, analysis can be run immediately.

The analysis engine covers 60 statistical procedures: descriptive statistics, t-tests, one-way and nested ANOVA, nonparametric tests, regression with 11 built-in nonlinear models, survival analysis, and multivariate methods. A built-in advisor first runs normality and homogeneity diagnostics, then recommends an appropriate test, chart form, and multiple-comparison procedure based on the results.

Plotting offers 60 chart types with error bars, overlaid scatter, and significance annotation. Annotations follow conventional journal tiers and can display exact P values. Figures export to 13 formats — PNG, JPEG, TIFF, SVG, TikZ, and Matplotlib scripts among them — with vector output ready for publication.

Projects are stored as a single .sprx file: a ZIP container with an embedded lossless snapshot, designed so reopening restores data and figures in full.
