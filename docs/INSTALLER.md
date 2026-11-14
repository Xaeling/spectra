# StatAura Windows 安装包

## 产物

```text
dist/StatAura-0.19.2.exe            # 单文件主程序（无 VC++ 运行库 / 无旁置资源）
dist/StatAura-Setup-0.19.2.exe      # 单文件离线安装器（内嵌主程序）
dist/SHA256SUMS.txt
```

两个 exe 均为 x64、静态 CRT；安装器只读自身，不联网、不下载。

## 构建

```bat
build_setup.cmd
```

等价于 `powershell -ExecutionPolicy Bypass -File .\build_setup.ps1`：

1. 构建主程序（`build.rs` 写入 `assets/icon.ico`、DPI/manifest）；
2. 设置 `STATAURA_APP_EXE`，构建 `installer/`；
3. 输出到 `dist/` 并生成 SHA256。

离线构建使用工作区缓存的 `..\.cargo-home`，不写入全局 `~/.cargo`。

## 安装行为

- 安装器 manifest 为 `requireAdministrator`，双击即弹 UAC。
- 默认目录：`C:\Program Files\StatAura`，可修改。
- 写入 `stataura.exe`、`uninstall.exe`（安装器自身副本）。
- 创建公共桌面 / 公共开始菜单快捷方式，图标取自主程序资源。
- 注册 `.aura`：
  - `HKCU/HKLM\Software\Classes\.aura` → `StatAura.aura`
  - `StatAura.aura\DefaultIcon` → `"<install>\stataura.exe",0`
  - `shell\open\command` → `"<install>\stataura.exe" "%1"`
  - `Applications\stataura.exe`、`RegisteredApplications`、Capabilities
  - `Uninstall\StatAura`（应用和功能中的卸载入口）

`.prism` 不抢默认关联，但支持拖进窗口或通过命令行打开；避免覆盖 GraphPad Prism。

## 卸载

“应用和功能”里卸载，或运行 `<install>\uninstall.exe`：

- 删除主程序、卸载器、安装目录；
- 删除桌面/开始菜单快捷方式；
- 删除 `.aura` 关联、`.aura` ProgID、Open With、Capabilities、卸载项；
- 仅当 `.aura` 仍指向 `StatAura.aura` 时删除，用户改过的默认程序不会被覆盖；
- 保留 `%APPDATA%\StatAura` 中的个人设置与工程文件。

## 静默参数

```text
StatAura-Setup.exe --silent
StatAura-Setup.exe --silent --dir "D:\Apps\StatAura"
StatAura-Setup.exe --uninstall --silent
StatAura-Setup.exe --uninstall --silent --dir "..."
```

测试用隐藏开关：`STATAURA_SETUP_AS_INVOKER=1`（构建时）生成 asInvoker manifest；
`STATAURA_SKIP_SHORTCUTS=1`、`STATAURA_SKIP_REGISTRY=1`（运行时）跳过桌面/注册表，
只用于受限沙箱验收。正式产物不需要这些变量。

## 文件打开

- 双击 `.aura`（关联）→ `stataura.exe "%1"`。
- 把 `.aura` / `.prism` 拖进窗口 → 直接加载；有未保存修改时先弹保存确认。
- 不支持的文件会提示，不改变当前工程。

## 已知限制

- 未做代码签名，SmartScreen 可能提示“未知发布者”。
- Windows 10/11 的默认应用保护可能让首次 `.aura` 打开弹出“选择打开方式”，
  StatAura 已注册为可用程序；卸载不会删除用户后来选择的默认程序。
- 安装器使用 OpenGL/egui；目标机器需可用 OpenGL（与主程序一致）。
