# Repository Guidelines

## Project Structure & Module Organization

LitePulse is a lightweight Windows tray monitor built with Rust 2024, Win32, and GDI. `src/main.rs` handles startup; `src/sampler/` collects system metrics; `src/metrics.rs` and `src/history.rs` model samples and history. `src/ui/` implements panel layout, painting, and themes; `src/tray.rs` manages the tray; `src/win32/` wraps platform integration. Settings live in `src/config.rs`.

Unit tests reside beside implementation code. `assets/litepulse.ico` is embedded by `build.rs`; `tools/gen_icon.py` generates icons. `docs/` contains screenshots and verification records. Generated `target/` and `dist/` directories are ignored.

## Build, Test, and Development Commands

Use Windows 10/11 x64, Rust stable with the MSVC toolchain, Visual Studio Build Tools, and Windows SDK.

- `cargo run`: launch the tray app and panel locally.
- `cargo run --features console -- --probe 5`: print five seconds of diagnostic samples.
- `cargo fmt --all -- --check`: check Rust formatting.
- `cargo test history::tests`: run a focused module's tests.
- `cargo test`: run all unit tests.
- `cargo clippy --all-targets --features console -- -D warnings`: lint with warnings treated as errors.
- `cargo build --release`: produce `target/release/litepulse.exe`.
- `powershell -ExecutionPolicy Bypass -File .\scripts\build.ps1`: run format, full tests, lint, and release build; copy the executable to `dist/`.

## Coding Style & Naming Conventions

Use rustfmt defaults, four-space indentation, `snake_case` functions/modules, `PascalCase` types, and `SCREAMING_SNAKE_CASE` constants. Keep platform calls in the relevant Win32 or sampler module. Preserve low resource usage and release Windows/GDI resources explicitly.

## Testing Guidelines

Use Rust's built-in `#[test]` framework inside `#[cfg(test)] mod tests`, with descriptive names such as `missing_values_do_not_create_points`. Cover new behavior and edge cases; no numeric coverage threshold is configured. Run changed-module tests and directly affected callers first. Reserve full-suite runs for broad/high-risk changes or releases, and report results explicitly.

Verify visual changes in the rendered Windows app, covering affected states, themes, fonts, and DPI settings. Capture relevant screenshots and update `docs/verification.md` when appropriate.

## Commit & Pull Request Guidelines

Git history is unavailable in this checkout. Follow the supplied convention: commit each completed, verified change; use only applicable `completed:` and `fixed:` sections with tab-indented bullets, followed by `time: MM-DD HH:mm` in SGT (UTC+8). Omit empty sections and attribution trailers. Ask before pushing.

PRs should describe behavior changes, link related issues, report verification commands/results, and include screenshots for visual changes.
