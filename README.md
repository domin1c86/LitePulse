# LitePulse

**English** | [简体中文](README_ZH.md)

A lightweight Windows tray monitor for **CPU, memory, GPU, VRAM, disk, and network** activity. Hover for a quick overview, or open the panel for live performance charts.

- **Native and portable** — Rust + Win32 + GDI, with a single executable and no installation required.
- **Small footprint** — the current v0.1.0 release build is **602 KiB** (616,448 bytes).
- **Local only** — no telemetry, network requests, administrator privileges, or extra drivers required.
- **Adaptive sampling** — refreshes every 30 seconds in the background by default, switching to 1 second during interaction.

The app currently uses a Chinese interface. The language links above switch the documentation language.

## Screenshots

### Performance panel

![LitePulse CPU panel with current controls and 1-second interaction sampling](docs/sampling-active.jpg)

### Settings and tray overview

![Settings with background sampling and interaction acceleration](docs/settings-interaction-boost.jpg)

![Compact tray overview with upload and download rates](docs/hover-glass-paper.jpg)

## Monitoring

| Tab | What it shows |
|---|---|
| CPU | Total utilization and recent usage history |
| Memory | Usage history, used/available capacity, and a composition bar |
| GPU | GPU utilization and dedicated VRAM history; temperature when available through NVML |
| Disk | Combined physical-disk activity, read/write rates, and system-drive space usage |
| Network | Upload/download history and rates for the default-route adapter |

Charts keep **60 sample positions**, with the newest data on the right. Missing readings leave gaps. A change to the actual sampling interval clears the previous timeline and starts a new history.

The tray ring can show **CPU, memory, VRAM, or disk activity**. It changes color at 70% and 90%. Hovering opens a compact overview with four usage bars and network rates; upload is red and download is green. On Windows 11, the overview supports native Acrylic with a near-white backing in the light theme.

## Quick start

1. Put `litepulse.exe` in a permanent folder and run it. The tray icon and monitoring panel appear.
2. **Hover over the tray icon** for the overview; **left-click** it to show or hide the panel.
3. Switch tabs by clicking them or using the **left/right arrow keys**. Open settings with the **gear**, or toggle always on top with the **pin**.
4. Use the settings page or right-click the tray icon/panel to change preferences. Changes take effect immediately and are saved.
5. The **close button** hides the panel. **Esc** returns from settings to monitoring, or hides the monitoring panel. Choose **退出 LitePulse** (Exit LitePulse) to stop the app completely.

> If the tray icon is hidden on Windows 11, open the taskbar's hidden-icons area (`^`) and drag the LitePulse ring onto the taskbar.

The panel stays out of the taskbar and Alt+Tab. Launching the executable again opens the existing instance instead of keeping a second instance running.

## Settings and sampling

| Setting | Options / default for a new configuration |
|---|---|
| Always on top | On by default |
| Start with Windows | Off by default; starts in the tray with the panel hidden |
| Background sampling | **30 seconds** by default; choose 1, 2, 5, or 30 seconds, or 1 minute |
| Interaction acceleration | **On** by default; switches to 1-second sampling while the panel, tray overview, or context menu is open |
| Theme | System (default), light, or dark |
| Font | MiSans (default), with Microsoft YaHei UI fallback |
| Tray metric | CPU (default), memory, VRAM, or disk activity |

When all interactions end, sampling returns to the selected background interval. Disabling acceleration keeps that interval during interaction as well. At a 1-second background interval, the acceleration toggle is disabled because sampling is already at its fastest setting. Existing configurations retain their saved background interval.

MiSans is used only if installed locally; no font files are bundled. The app supports Per-Monitor V2 DPI scaling, multiple monitors, and sampling baseline resets after sleep or session lock/unlock.

Startup is registered for the current user under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, using `--startup`. If you move the executable after enabling startup, launch it once from the new location to update the registered path.

## Build from source

Use **Windows 10/11 x64**, Rust stable with the **MSVC toolchain**, Visual Studio Build Tools, and the Windows SDK. The first build needs access to crates.io for dependencies.

```powershell
cargo build --release
# Output: target\release\litepulse.exe
```

The build script checks formatting, runs the full test suite and Clippy, builds the release, and copies the executable to `dist/litepulse.exe`:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build.ps1
```

## GPU backends

By default, LitePulse uses **Windows PDH + DXGI**, supporting AMD, Intel, and NVIDIA without loading a vendor SDK when those counters are available. It selects the adapter with the most dedicated VRAM. If the PDH GPU counters are unavailable, it attempts to fall back to **NVIDIA NVML**.

PDH utilization approximates the busiest GPU process/engine, so it can differ from Task Manager or `nvidia-smi`. DXGI's total VRAM may also differ slightly from the driver's figure. GPU temperature is available only through NVML, which requires a compatible NVIDIA driver and uses more memory.

To request NVML explicitly, exit the running instance first, then launch from PowerShell:

```powershell
$env:LITEPULSE_GPU_BACKEND = "nvml"
.\litepulse.exe
```

To disable GPU collection, exit the app and launch with:

```powershell
$env:LITEPULSE_DISABLE_GPU = "1"
.\litepulse.exe
```

## Configuration and logs

- Settings: `%LOCALAPPDATA%\LitePulse\settings.json`
- Local diagnostic log: `%LOCALAPPDATA%\LitePulse\litepulse.log`

Settings store the theme, background interval (`interval_ms`), interaction acceleration (`boost_on_interaction`), always-on-top preference, font, window position, and tray metric. In `tray_metric`, `gpu` means **VRAM usage**, not GPU utilization. The startup option is stored separately in the Windows Run registry entry.

To restore defaults, exit LitePulse, delete `settings.json`, and restart it. This does not remove the startup registry entry. You can also set `font_family` to another installed font family while the app is closed; unavailable fonts fall back to Microsoft YaHei UI.

## Limitations and performance notes

- No CPU temperature, fan speed, power readings, per-process rankings, historical database, alerts, or automatic updates.
- Network readings use the default-route adapter, without combining multiple active adapters. Disk activity combines physical disks; capacity is shown for the system drive.
- Primarily verified on Windows 11; Windows 10 1809+ compatibility is best effort. Acrylic depends on Windows support, with a solid-background fallback.
- The executable is unsigned and has no installer.

The current executable size was checked on **2026-11-03**. Earlier CPU and memory measurements belong to specific builds and settings; see the [verification record](docs/verification.md) and [sampling performance investigation](docs/performance-investigation.md) for their scope and results (both in Chinese).

## Development and diagnostics

```powershell
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets --features console -- -D warnings
cargo build --release
```

For a five-second console report of metrics, selected sources, and collection timings:

```powershell
cargo run --features console -- --probe 5
```

Command-line options: `--startup` (tray only), `--probe [seconds]` (diagnostic sampling; use a console build for visible output), and `--version` (version output). Manual visual and performance diagnostics are documented in the verification records above.

UI icons are embedded as 64×64 alpha masks in `assets/ui/`. After editing the SVG sources in `images/`, regenerate them with `node tools/gen_ui_icons.cjs` in an environment where Node.js can find `sharp`. Normal Rust builds require neither Node.js nor an SVG runtime.

## License

[MIT](LICENSE)
