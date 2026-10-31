# spectra-installer

**析光 Spectra** 的纯 Rust 安装 / 卸载程序。安装与卸载**共用同一个二进制**、同一套动画引擎，
靠 exe 文件名区分模式：文件名为 `uninstall.exe` 时进入卸载流程，否则进入安装流程。

## 单文件自包含

主程序在**编译期**通过 `include_bytes!` 嵌进安装程序本体，所以 `dist\` 里只有一个 exe，
拷到任何一台机器双击即可安装，不需要附带任何文件。

## 编译

双击 `build.bat`。它会按顺序做三件事：

1. 编译主程序（`..\prism-rs`，产物 `spectra.exe`）—— 必须先编出来，否则 embed 进去的是旧版
2. 编译安装程序（build.rs 把 `spectra.exe` 嵌进来）
3. 产出 `dist\spectra-installer.exe`，并做体积自检（小于 3MB 说明没嵌入，直接报错）

源码目录若不在 `..\prism-rs`，显式指定：

```bat
build.bat "D:\project\prism-rs\target\release"
```

主程序刚编过想跳过第 1 步：`set SKIP_MAIN=1`。

## 用法

```bat
spectra-installer.exe                          :: 安装到 C:\Program Files\Spectra
spectra-installer.exe --dir "D:\Spectra"       :: 指定安装目录
spectra-installer.exe --payload D:\build       :: 改用外部目录里的主程序
spectra-installer.exe --preserve mydata        :: 额外保留的条目（可重复）
spectra-installer.exe /S                       :: 静默安装（无界面）
uninstall.exe /S                               :: 静默卸载
```

## 做了什么

**安装**：建目录结构（含 `README.txt` 占位，空目录会被打包工具丢掉）
→ 写入 `spectra.exe` 与 `uninstall.exe`
→ 桌面 + 开始菜单快捷方式
→ ARP 注册（HKCU `...\Uninstall\Spectra`）
→ 文件关联 `.prism` / `.prism.json`（ProgId `Spectra.Document`）

**卸载**：删快捷方式 → 清文件关联 → 删 ARP 子键 → 删目录内容 → 派生 `cmd.exe` 延迟删除自身与整个目录。

刻意**不关联 `.pzfx`**：那是真实 GraphPad Prism 的专有扩展名，抢过来会让装了正版的用户
双击时被我们打开。

刻意**不使用 `self-replace`**：它的 `self_delete()` 在 Windows 上是「复制自身到 temp →
派生隐藏 helper → 等父进程退出后删原 exe」，正是启发式杀软的重点特征。改用 `src/selfdel.rs`
的 cmd 延迟删除，不生成任何额外可执行文件。

**更新安装**：备份 `PRESERVE` 清单里的条目 → 清空其余 → 写入新文件 → 还原保留项。
所以新文件完全覆盖，用户设置不丢。

## 改名清理

更名前的旧标识（`prism-rs`、`PrismRS`、`PrismRS.Document`）在安装与卸载时都会清理，
避免留下僵尸 ARP 条目和失效的文件关联。

## 已知风险

- **未签名**，杀软仍可能报毒。根本解法是 Authenticode 签名，不是改代码。
- 目标机器若没有 VC++ 运行库会打不开（症状：`vcruntime140.dll 找不到`）。
  装 [VC++ Redist](https://aka.ms/vs/17/release/vc_redist.x64.exe)，
  或换 `x86_64-pc-windows-gnu` 目标完全静态链接。
