# LitePulse

LitePulse 是一个 Windows 桌面系统监控小工具：实时显示 **CPU、内存、显卡、显存、磁盘、网络** 占用。悬停托盘图标显示紧凑概览，点击后打开五个标签页的性能面板。

- 原生 Rust + Win32 + GDI，无 WebView、无 .NET 运行时、无 Electron。
- 单文件 exe；本次 release 构建约 **602 KB**。
- 默认不加载任何显卡厂商 SDK；GPU 数据来自 Windows 性能计数器和 DXGI。
- 不联网、不采集遥测、不需要管理员权限、不安装驱动。

![LitePulse 浅色面板](docs/panel-light.png)

![LitePulse 深色面板](docs/panel-dark.png)

## 功能

- **CPU**：总利用率 + 最近 60 个采样点走势。
- **内存**：占用历史、已用/可用容量、内存组成条。
- **显卡/显存**：GPU 利用率与专用显存占用两张历史图；自动选择独显（按专用显存最大）。
- **磁盘**：所有物理盘的活动时间图、读取/写入双曲线，以及系统盘（通常是 C:）已用空间。
- **网络**：默认路由网卡的下载/上传双曲线和实时速率，统一使用字节速率单位。
- **图表**：任务管理器式网格与填充曲线；最新数据靠右，保留 60 个采样位置；缺失数据留空档，切换采样间隔清空旧时间轴。
- **托盘小图标**：默认显示 CPU 利用率；右键“托盘图标指标”可改为内存占用、显存占用或磁盘活动时间，选择会保存。圆环随采样更新，颜色按 70%/90% 变色。
- **托盘概览**：CPU、内存、显存、磁盘四行进度条，底部同一行显示上传/下载速率，箭头紧挨数字；宽度 140 DIP，圆角边框，Windows 11 支持原生 Acrylic 磨砂背景，浅色采用接近白纸的浅白衬底以减少灰蒙感；上传箭头固定红色、下载箭头固定绿色；采用 `images/` 提供的 SVG 图形，不抢焦点，跟随主题与字体。
- **托盘菜单**：显示/隐藏面板、总在最前、开机自动启动、采样间隔（1/2/5/30 秒、1 分钟）、主题（跟随系统/深色/浅色）、字体（MiSans/微软雅黑 UI）、托盘图标指标、退出。主面板右键也能打开菜单。
- **交互采样**：新配置默认后台 30 秒、自动加速开启；悬停托盘、打开右键菜单或主界面时使用 1 秒采样，全部结束后恢复后台间隔。后台间隔可选 1/2/5/30 秒、1 分钟；基础间隔为 1 秒时禁用加速开关。设置页和右键菜单均可控制；既有配置保留所选间隔。实际采样间隔变化时重新积累图表历史。
- **设置页**：点击右上角齿轮进入，包含置顶、开机启动、采样间隔、主题、字体、托盘指标，以及隐藏/退出；修改立即生效，复用托盘菜单的设置处理。再次点击齿轮或按 Esc 返回监控。
- **低干扰**：关闭按钮和 Esc 只是隐藏到托盘；面板不占任务栏、不进入 Alt+Tab；单实例运行。
- **自适应**：Per-Monitor V2 DPI、多显示器位置钳制、浅/深色主题、休眠/锁屏恢复后重置基线。
- **字体可选**：面板默认使用本机 **MiSans**（字体文件不随 exe 分发）；托盘菜单可切换回微软雅黑 UI。未安装 MiSans 时自动回退，不会出现替代字体导致的排版异常。
- **GPU 后端可选**：默认 Windows PDH + DXGI（低内存、AMD/Intel/NVIDIA 通用）；可切换到 NVIDIA NVML（更精确的 GPU 利用率、显存和温度，但内存多约 15 MB）。

## 改版前资源基线

以下是此前紧凑面板的资源测量，本次界面改版未重复长时间性能测试。当前可执行文件大小为 616,448 字节。

测试机：Windows 11 Build 26200、32 GB 内存、RTX 5060 Laptop 8 GB、3200×2000 @175%、1 秒采样、release 构建。

| 场景 | 工作集 | 私有内存 | CPU（单核占比） | GDI / USER 对象 |
|---|---:|---:|---:|---:|
| 仅托盘（隐藏 60 秒） | 21.2 MB | 7.5 MB | 0.29% | 13 / 17 |
| 面板可见（30 秒） | 21.2 MB | 7.5 MB | 0.47% | 21 / 17 |

- exe：`537,088` 字节。
- 启动到托盘：约 20–30 ms（`starting` → `attached` 日志）。
- MiSans 比此前的微软雅黑 UI 基线多约 1.9 MB 工作集（字体缓存）；隐藏时会释放后备位图和 HFONT，GDI 对象回落到 13。
- 隐藏时整体约占 0.29% 单核，折合 16 核总量约 0.02%；此前的 10 分钟连续运行测试（微软雅黑 UI）平均 0.20% 单核、工作集约 19.5 MB，句柄数稳定。
- 如果强制使用 NVML 后端，隐藏内存约 34 MB，但能读到 GPU 温度；默认 PDH 后端更符合低占用目标。

更完整的数据与验证过程见 [docs/verification.md](docs/verification.md)。

## 构建

环境要求：

- Windows 10/11 x64。
- Rust stable（已验证 1.97.1，MSVC 工具链）。
- Visual Studio Build Tools + Windows SDK（用于链接和 exe 图标资源）。
- 首次构建需要访问 crates.io 拉取依赖。

命令：

```powershell
cargo build --release
# 产物：target\release\litepulse.exe
```

或者运行构建脚本（会先执行 fmt/test/clippy，再构建 release 并复制到 `dist`）：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build.ps1
```

开发/诊断模式：

```powershell
cargo run --features console -- --probe 5
```

`--probe 5` 会逐秒打印六项指标、GPU 后端、网卡与每项采集耗时，便于与任务管理器 / `nvidia-smi` 对照。

## 使用

1. 双击 `litepulse.exe`：托盘出现圆环图标，同时显示面板。
2. 悬停托盘图标查看占用条与上传/下载；左键点击显示/隐藏主面板。右键打开菜单，选择“托盘图标指标”设置动态圆环的数据来源；“字体”同时影响主面板和概览，菜单仍由系统绘制。
3. 点击 CPU、内存、显卡、磁盘、网络标签切换图表，左右方向键也能切换。右上角齿轮打开设置页，图钉控制总在最前，关闭图标隐藏到托盘。Esc 在设置页返回监控，在监控页隐藏面板。
4. 右键菜单中的“退出 LitePulse”才会真正结束进程。
5. 勾选“开机自动启动”后写入 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`，启动参数为 `--startup`（只留托盘，不弹面板）。建议先把 exe 放到固定目录再开启；之后移动 exe 并在新位置手动启动一次，会自动修正 Run 路径。

> Windows 11 默认会把新托盘图标收进“隐藏的图标”溢出区。打开任务栏的 `^`，把 LitePulse 圆环拖到任务栏即可常显；这是 Windows 的托盘策略，不是程序遗漏。

命令行参数：

- `--startup`：启动后只驻留托盘。
- `--probe [秒数]`：无窗口采样诊断（需配合 `--features console` 构建）。
- `--version`：输出版本号。

## GPU 后端与低占用取舍

默认：

```text
Windows PDH:  \GPU Engine(*)\Utilization Percentage
              \GPU Adapter Memory(*)\Dedicated Usage
DXGI:         adapter name / dedicated VRAM / LUID
```

优点：不加载 NVIDIA DLL，隐藏内存约 19 MB，支持 AMD/Intel/NVIDIA；缺点：没有温度，GPU 利用率是“当前最忙 GPU 进程/引擎”的近似值，和 `nvidia-smi` 可能相差几个百分点。

强制 NVML（仅 NVIDIA，含温度）：

```powershell
$env:LITEPULSE_GPU_BACKEND = "nvml"
.\litepulse.exe
```

完全关闭 GPU 采集（例如排查 NVML/PDH 兼容性问题）：

```powershell
$env:LITEPULSE_DISABLE_GPU = "1"
.\litepulse.exe
```

## 配置文件与日志

- 设置：`%LOCALAPPDATA%\LitePulse\settings.json`
- 日志：`%LOCALAPPDATA%\LitePulse\litepulse.log`（只记录启动、后端选择、错误与退出）

设置包含主题、后台采样间隔、`boost_on_interaction`（交互时自动切到 1 秒）、是否总在最前、字体、窗口位置，以及 `tray_metric`（`cpu`、`memory`、`gpu`、`disk`，其中 `gpu` 表示显存占用）。旧设置文件会自动采用 CPU 默认值。删除设置文件即可恢复默认（跟随系统主题、后台 30 秒、交互加速开启、总在最前、MiSans、CPU 圆环）。也可以直接把 `font_family` 改成其他已安装的字体族名；程序会先确认字体可用，不可用时回退到微软雅黑 UI。

## 已知限制

- 不显示 CPU 温度/风扇/功耗；这些需要额外驱动或特权，和“低占用 + 无管理员权限”冲突。
- 不做每进程排行、历史数据库、阈值告警或自动更新。
- 网络取默认路由网卡，不聚合多块同时上网的网卡；磁盘取所有物理盘合计与系统盘容量。
- GPU 显存总量来自 DXGI，可能比 NVIDIA 驱动报告的值少约 3%（本机 7.7 GB vs 8.0 GB）；显存已用量基本一致。
- 无安装包/代码签名；如需开机自启请使用托盘菜单。
- 主要验证环境为 Windows 11；Windows 10 1809+ 为尽力兼容。

## 开发与测试

```powershell
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets --features console -- -D warnings
cargo build --release
```

当前 37 个自动测试覆盖：CPU 利用率公式、速率计算与重置、历史环形缓冲与缺失空档、托盘指标选择和设置序列化、字体族匹配、单位格式化、托盘提示截断、GPU 计数器聚合、标签命中与图表坐标、磨砂前景保色、长间隔配置与网络速率、采样唤醒、交互状态组合与禁用开关等。另有两个默认跳过的手动视觉测试入口，执行方式与截图见 [验证记录](docs/verification.md)。

`assets/ui/*.alpha` 是由用户 SVG 生成的 64×64 透明度蒙版，编译时嵌入 exe。修改 SVG 后，可在 Node.js 能找到 `sharp` 的环境中运行 `node tools/gen_ui_icons.cjs` 重新生成；普通 Rust 构建不需要 Node.js 或 SVG 运行时。
