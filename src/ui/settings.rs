use super::layout::Layout;
use crate::{
    config::{Settings, ThemeSetting, TrayMetric},
    tray::TrayCommand,
    win32::font,
};
use windows::Win32::Foundation::RECT;

pub struct Choice {
    pub rect: RECT,
    pub label: &'static str,
    pub selected: bool,
    pub enabled: bool,
    pub command: TrayCommand,
}
pub const ROW_LABELS: [&str; 6] = [
    "窗口置顶",
    "开机启动",
    "采样间隔",
    "主题",
    "字体",
    "托盘指标",
];

pub fn choices(l: &Layout, s: &Settings, autostart: bool, misans: bool) -> Vec<Choice> {
    let rows = vec![
        vec![(
            "总在最前",
            s.always_on_top,
            true,
            TrayCommand::ToggleTopmost,
        )],
        vec![(
            "开机自动启动",
            autostart,
            true,
            TrayCommand::ToggleAutostart,
        )],
        [1000, 2000, 5000]
            .into_iter()
            .zip(["1 秒", "2 秒", "5 秒"])
            .map(|(ms, label)| {
                (
                    label,
                    s.interval_ms == ms,
                    true,
                    TrayCommand::SetInterval(ms),
                )
            })
            .collect(),
        [
            ThemeSetting::System,
            ThemeSetting::Light,
            ThemeSetting::Dark,
        ]
        .into_iter()
        .zip(["跟随系统", "浅色", "深色"])
        .map(|(theme, label)| (label, s.theme == theme, true, TrayCommand::SetTheme(theme)))
        .collect(),
        vec![
            (
                "MiSans",
                s.font_family == font::DEFAULT_FAMILY && misans,
                misans,
                TrayCommand::SetFontFamily(font::DEFAULT_FAMILY.into()),
            ),
            (
                "微软雅黑 UI",
                s.font_family == font::FALLBACK_FAMILY || !misans,
                true,
                TrayCommand::SetFontFamily(font::FALLBACK_FAMILY.into()),
            ),
        ],
        [
            TrayMetric::Cpu,
            TrayMetric::Memory,
            TrayMetric::Gpu,
            TrayMetric::Disk,
        ]
        .into_iter()
        .zip(["CPU", "内存", "显存", "磁盘"])
        .map(|(metric, label)| {
            (
                label,
                s.tray_metric == metric,
                true,
                TrayCommand::SetTrayMetric(metric),
            )
        })
        .collect(),
        vec![
            ("隐藏面板", false, true, TrayCommand::TogglePanel),
            ("退出 LitePulse", false, true, TrayCommand::Exit),
        ],
    ];
    rows.into_iter()
        .enumerate()
        .flat_map(|(row, items)| {
            let count = items.len() as i32;
            let start = l.s(if row == 6 { 24.0 } else { 164.0 });
            let available = l.width - start - l.s(24.0);
            items
                .into_iter()
                .enumerate()
                .map(move |(col, (label, selected, enabled, command))| Choice {
                    rect: RECT {
                        left: start + available * col as i32 / count,
                        right: start + available * (col + 1) as i32 / count - l.s(6.0),
                        top: l.s(110.0 + row as f32 * 47.0),
                        bottom: l.s(145.0 + row as f32 * 47.0),
                    },
                    label,
                    selected,
                    enabled,
                    command,
                })
        })
        .collect()
}

pub fn command_at(
    l: &Layout,
    s: &Settings,
    autostart: bool,
    misans: bool,
    x: i32,
    y: i32,
) -> Option<TrayCommand> {
    choices(l, s, autostart, misans)
        .into_iter()
        .find(|c| c.enabled && Layout::contains(&c.rect, x, y))
        .map(|c| c.command)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_menu_commands_have_clickable_settings_and_disabled_fonts_do_not_fire() {
        for dpi in [96, 168, 192] {
            let (w, h) = super::super::layout::panel_size(dpi);
            let l = Layout::new(w, h, dpi);
            let s = Settings::default();
            let buttons = choices(&l, &s, false, false);
            assert_eq!(buttons.len(), 16);
            for c in buttons {
                assert!(c.rect.bottom < h && c.rect.right < w);
                let command = command_at(&l, &s, false, false, c.rect.left, c.rect.top);
                assert_eq!(command, c.enabled.then_some(c.command));
            }
        }
    }
}
