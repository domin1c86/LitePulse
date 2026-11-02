use windows::Win32::Foundation::COLORREF;

use crate::config::ThemeSetting;
use crate::win32::theme as system_theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolvedTheme {
    Dark,
    Light,
}

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub bg: COLORREF,
    pub border: COLORREF,
    pub text: COLORREF,
    pub dim: COLORREF,
    pub track: COLORREF,
    pub hover: COLORREF,
    pub cpu: COLORREF,
    pub gpu: COLORREF,
    pub mem: COLORREF,
    pub disk: COLORREF,
    pub net: COLORREF,
    pub accent: COLORREF,
}

pub const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | ((g as u32) << 8) | ((b as u32) << 16))
}

pub const fn download_color() -> COLORREF {
    rgb(48, 170, 100)
}

pub fn resolve(setting: ThemeSetting) -> ResolvedTheme {
    match setting {
        ThemeSetting::Dark => ResolvedTheme::Dark,
        ThemeSetting::Light => ResolvedTheme::Light,
        ThemeSetting::System => {
            if system_theme::system_uses_light() {
                ResolvedTheme::Light
            } else {
                ResolvedTheme::Dark
            }
        }
    }
}

pub fn palette(theme: ResolvedTheme) -> Palette {
    match theme {
        ResolvedTheme::Dark => Palette {
            bg: rgb(26, 28, 33),
            border: rgb(45, 50, 59),
            text: rgb(237, 240, 245),
            dim: rgb(148, 157, 171),
            track: rgb(43, 47, 55),
            hover: rgb(38, 42, 50),
            cpu: rgb(91, 157, 255),
            gpu: rgb(167, 139, 250),
            mem: rgb(96, 165, 250),
            disk: rgb(163, 205, 82),
            net: rgb(244, 114, 161),
            accent: rgb(91, 157, 255),
        },
        ResolvedTheme::Light => Palette {
            bg: rgb(247, 248, 250),
            border: rgb(224, 227, 232),
            text: rgb(23, 26, 31),
            dim: rgb(95, 104, 117),
            track: rgb(229, 232, 237),
            hover: rgb(236, 239, 244),
            cpu: rgb(37, 99, 235),
            gpu: rgb(124, 58, 237),
            mem: rgb(48, 116, 244),
            disk: rgb(103, 143, 37),
            net: rgb(205, 35, 99),
            accent: rgb(37, 99, 235),
        },
    }
}
