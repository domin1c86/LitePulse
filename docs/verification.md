# LitePulse 验证记录

## 2026-11-03：托盘概览与标签页改版

本次改动将主窗口改为 620×470 DIP、五个标签页，托盘概览为 280×266 DIP。图标来自 `images/`，其中置顶素材的原文件名是 `.svg`。圆环默认 CPU，可在右键菜单选择内存、显存或磁盘活动时间，写入 `tray_metric`。图表保留 60 个采样位置；缺失数据产生空档，改变采样间隔后重新积累历史。

| 检查 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | 通过；最后仅有测试文件的格式整理，未改变渲染 |
| `cargo test`（完整测试） | **28 passed / 0 failed / 2 ignored**；两个 ignored 为显式手动视觉入口 |
| `cargo test ui::`、`cargo test history::tests` | 相关模块检查通过；补充控制悬停渲染用例后再次运行 UI 范围检查通过 |
| `cargo clippy --all-targets --features console -- -D warnings` | 通过；构建环境另有增量缓存硬链接回退提示，无代码 lint 错误 |
| `cargo build --release` | 通过；593,408 字节；已复制至 `dist/litepulse.exe`，SHA-256 与构建产物一致 |
| `cargo test render_visual_matrix -- --ignored` | 原生 GDI 渲染成功，逐一检查组合图；不能仅凭此命令成功视为视觉通过 |
| Windows 实际窗口 | 在 175% 桌面依次点击五个标签，核对实时数据、面积图、读写双曲线；检查置顶开关与深浅色即时切换 |
| 动态指标菜单 | 实际展开四项指标菜单并切换内存；配置往返与所选数据来源由自动测试覆盖 |
| 悬浮层 | 测试构建通过 F8 调用与 Shell `NIN_POPUPOPEN` / `NIN_POPUPCLOSE` 相同的回调，核对实际窗口位置、实时更新、深浅色、开启后仍可通过主窗口按键关闭（不抢焦点） |

桌面工具不枚举 `WS_EX_TOOLWINDOW`，因此交互检查使用 `cfg(test)` 入口使主窗口出现在任务栏，且不保存测试设置；生产构建仍为仅托盘工具窗口。鼠标从 Shell 托盘真实进入/离开的触发路径未通过桌面自动化直接验证；这不等同于完成该路径的端到端测试。跨显示器移动、休眠/锁屏以及长期资源测试沿用此前基线，未在本次复测。

实际窗口截图：[浅色主窗口](panel-light.png)、[深色主窗口](panel-dark.png)、[浅色概览](hover-light.png)、[深色概览](hover-dark.png)。改版前截图保留为 [浅色](panel-before-light.png) 与 [深色](panel-before-dark.png)。

以下为同一原生 Renderer 使用固定测试数据生成并人工检查的排版矩阵，不是机器性能测量：

- [MiSans，浅色 100%：五页与概览](ui-light-100.png)
- [MiSans，深色 175%：五页与概览](ui-dark-175.png)
- [微软雅黑 UI，浅色 200%：五页与概览](ui-light-200-yahei.png)
- [加载状态](ui-loading.png)、[数据不可用](ui-unavailable.png)、[断网状态](ui-disconnected.png)
- [0% / 100% 与大单位速率](ui-extremes-preview.png)、[图钉/关闭/未选中标签悬停](ui-controls.png)

原始 BMP 位于忽略的 `target/visual/`。视觉检查修正了 GDI 默认缩小 SVG 蒙版时的黑色边缘，改为 HALFTONE；最终图标缩放后的所有相关矩阵已重查。仅格式整理复用紧接此前同一渲染状态的证据。

复现：`cargo test render_visual_matrix -- --ignored` 生成 BMP；`cargo test interactive_visual_check -- --ignored --nocapture` 启动可交互测试窗口（F8 打开/关闭概览，通过右键“退出 LitePulse”结束）。测试入口不需要修改生产配置或安装依赖。

## 改版前验证基线

日期：2026-11-02

测试机：Windows 11 Build 26200（x64）、32 GB 内存、NVIDIA RTX 5060 Laptop 8 GB（驱动 596.21）、3200×2000 @175%、系统浅色主题。

构建：Rust 1.97.1 stable MSVC、VS Build Tools 18.8.2、Windows SDK 10.0.26100。

## 1. 构建与静态检查

| 检查 | 命令 | 结果 |
|---|---|---|
| 格式 | `cargo fmt --all -- --check` | 通过 |
| 单元测试 | `cargo test` | 24 passed / 0 failed |
| Clippy | `cargo clippy --all-targets --features console -- -D warnings` | 通过，无 warning |
| Release | `cargo build --release` | 成功 |
| 产物大小 | `target\release\litepulse.exe` | 537,088 字节 |
| 版本信息 | `Get-Item ... .VersionInfo` | FileDescription/ProductName/0.1.0.0 正确 |
| exe 图标 | 提取并查看 | 深色圆角底 + 蓝色脉冲圆环，正常 |

## 2. 资源占用

测量方法：release 构建、默认 PDH GPU 后端、1 秒采样；`Start-Process` 启动后静置；用 `Get-Process` 的 `WorkingSet64`、`PrivateMemorySize64`、`HandleCount`、`TotalProcessorTime` 和 `GetGuiResources` 采样。

| 场景 | 时长 | 工作集 | 私有内存 | CPU（单核） | 句柄 | 线程 | GDI/USER |
|---|---:|---:|---:|---:|---:|---:|---:|
| 仅托盘（隐藏） | 60 s | 21.2 MB | 7.5 MB | 0.286% | 273 | 5 | 13 / 17 |
| 面板可见 | 30 s | 21.2 MB | 7.5 MB | 0.469% | 273 | 5 | 21 / 17 |

- 启动到托盘：日志 `starting` → `attached` 约 20–30 ms；NVML 初始化不计入托盘出现时间。
- 默认 PDH 后端比 NVML 后端少约 15 MB 私有内存（NVML 实测隐藏约 34 MB 工作集 / 21 MB 私有）。
- 默认 MiSans 比微软雅黑 UI 基线多约 1.9 MB 工作集（字体缓存）；隐藏时会释放后备位图和 HFONT，GDI 对象从 21 回落到 13。
- 60 秒与 30 秒测量期间 GDI 对象数量稳定，无持续增长。

## 3. GPU 数据源对照

默认 PDH 后端，与 `nvidia-smi` 同一时间窗口对照：

| 指标 | LitePulse (PDH/DXGI) | nvidia-smi | 说明 |
|---|---:|---:|---|
| GPU 利用率 | 7.8% / 3.8% / 4.2% | 5% / 6% / 6% | PDH 为最忙引擎/进程的近似值，差几个百分点 |
| 显存已用 | 2,479,538,176 B（约 2365 MiB） | 2349 MiB | 基本一致 |
| 显存总量 | 7899 MiB | 8151 MiB | DXGI 少约 3% |
| GPU 名称 | NVIDIA GeForce RTX 5060 Laptop GPU | 同 | 来自 DXGI |

每源采样耗时（`--probe`，微秒）：CPU ≈ 60–180、内存 ≈ 4–10、GPU ≈ 1500–2200、磁盘 ≈ 300–500、网络 ≈ 130–180。GPU 的 PDH 通配符计数（约 532 个实例）是主要开销，总采样约 3 ms/秒。

强制 NVML 后端时：GPU 利用率、显存、温度均与 `nvidia-smi` 基本一致，但隐藏内存升到约 34 MB。

## 4. 功能验证

| 项目 | 验证方式 | 结果 |
|---|---|---|
| 托盘图标注册 | UI Automation 搜索 `LitePulse` 元素 | 找到，提示文本含六项实时值 |
| 托盘图标绘制 | 打开“隐藏的图标”浮层截图 | 蓝色圆环 + 灰色轨道，透明背景，清晰 |
| 托盘左键 | UIA 调用图标 Invoke | 日志出现 `panel shown`，面板显示 |
| 托盘右键菜单 | 发送托盘回调 + 截图/UIA | 显示/隐藏、总在最前（勾选）、开机自启动（未勾选）、采样间隔、主题、字体、退出 |
| 字体切换 | 菜单“字体” → 微软雅黑 UI → MiSans | `settings.json` 的 `font_family` 随之变化，日志记录切换，面板立即重绘；默认启动日志为 `font family: MiSans` |
| 主题切换 | 菜单选择“深色” | `settings.json` 变为 `"theme": "dark"`，面板即时变暗 |
| 采样间隔 | 菜单选择“2 秒” | 设置保存为 `"interval_ms": 2000` |
| 开机自启 | 菜单开关 + 查注册表 | 启用后 Run 值为 `"<exe>" --startup`；关闭后删除 |
| 置顶 | 点击面板“置顶”/读取扩展样式 | `WS_EX_TOPMOST` 正确切换并持久化 |
| 拖动 | 按住标题区拖拽 | 窗口从 2612,1415 移到 2504,1463，位置正常保存 |
| 关闭/隐藏 | 点击 × / 发 `WM_CLOSE` | 窗口隐藏，进程保留，托盘继续更新 |
| Esc | 向面板发送 `WM_KEYDOWN` VK_ESCAPE | 窗口隐藏 |
| 启动隐藏 | `--startup` | 窗口不可见，托盘图标存在 |
| 单实例 | 隐藏状态下第二次启动 exe | 第二进程退出；第一实例收到唤醒并显示面板，进程数保持 1 |
| 退出 | 菜单“退出 LitePulse” | 进程结束，日志出现 `shutdown complete`，托盘清理 |
| 浅/深色视觉 | 面板截图 | 见 `docs/panel-light.png`、`docs/panel-dark.png`（均为 MiSans） |

字体相关冒烟测试顺序：启动（MiSans）→ 菜单切换微软雅黑 UI → 切回 MiSans → 隐藏 → 第二次启动唤醒；全部通过。此前拖动、Esc、退出清理测试仍有效；测试后已停止进程并删除设置/日志。

## 5. 稳定性

10 分钟隐藏运行（微软雅黑 UI 基线；每 60 秒采样一次工作集、私有内存、句柄、线程）：

| 时间 | 工作集 | 私有内存 | 句柄 | 线程 |
|---|---:|---:|---:|---:|
| 0 min | 19.3 MB | 7.5 MB | 260 | 6 |
| 1–4 min | 19.3 MB | 7.4–7.5 MB | 257–261 | 3–5 |
| 5–9 min | 19.5 MB | 7.4–7.5 MB | 257 | 3–6 |
| 10 min | 19.5 MB | 7.5 MB | 257 | 5 |

- 10 分钟总 CPU 时间 1,218.8 ms，折合 **0.203% 单核**。
- 工作集增长 +0.2 MB，私有内存波动 ±0.1 MB，句柄数 257–261，无持续增长或句柄泄漏。
- 期间进行过托盘浮层/截图操作，未影响隐藏状态的稳定性。
- 字体切换后，MiSans 的 60 秒隐藏复测为 21.2 MB / 0.286% 单核；10 分钟长测未重复，但字体只影响面板绘制缓存，不改变采样和托盘更新路径。

## 6. 已知差异与限制

- PDH 的 GPU 利用率不是硬件全局值，而是最忙 GPU 引擎/进程的近似；与 NVML/任务管理器可能差几个百分点。需要精确值/温度时可用 `LITEPULSE_GPU_BACKEND=nvml`。
- DXGI 报告的显存总量可能比 NVIDIA 驱动少约 3%，显存已用量基本准确。
- 默认不采集 GPU 温度；NVML 后端才提供温度。
- 不包含 CPU 温度、风扇、功耗、每进程明细、历史数据库、告警、安装包和自动更新。
- 未签名的单文件 exe 首次运行可能触发 SmartScreen；本地构建不受影响。
- 主要验证于 Windows 11；Windows 10 的 DWM 圆角会回退到 `SetWindowRgn`。
