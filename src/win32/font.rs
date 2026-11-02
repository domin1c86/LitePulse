use windows::Win32::Graphics::Gdi::{
    CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH,
    DeleteObject, FF_DONTCARE, GetDC, GetTextFaceW, HGDIOBJ, OUT_DEFAULT_PRECIS, ReleaseDC,
    SelectObject,
};
use windows::core::PCWSTR;

use super::to_wide;

/// Preferred UI family. Users can override it in settings.json, but the tray
/// menu offers this one plus the Windows fallback.
pub const DEFAULT_FAMILY: &str = "MiSans";
pub const FALLBACK_FAMILY: &str = "Microsoft YaHei UI";

/// GDI's font mapper silently substitutes a missing family. Select the font
/// once and ask GDI which face it actually resolved to, so missing MiSans
/// falls back explicitly instead of ending up with an arbitrary UI font.
pub fn family_available(name: &str) -> bool {
    let name = name.trim();
    if name.is_empty() {
        return false;
    }

    unsafe {
        let dc = GetDC(None);
        if dc.0.is_null() {
            return false;
        }

        let wide = to_wide(name);
        let font = CreateFontW(
            -12,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            DEFAULT_PITCH.0 as u32 | FF_DONTCARE.0 as u32,
            PCWSTR(wide.as_ptr()),
        );

        let mut available = false;
        if !font.0.is_null() {
            let old = SelectObject(dc, HGDIOBJ(font.0));
            let mut face = [0u16; 64];
            let copied = GetTextFaceW(dc, Some(&mut face));
            if copied > 0 {
                let end = face
                    .iter()
                    .position(|&unit| unit == 0)
                    .unwrap_or(face.len());
                let actual = String::from_utf16_lossy(&face[..end]);
                available = same_family(&actual, name);
            }
            SelectObject(dc, old);
            let _ = DeleteObject(HGDIOBJ(font.0));
        }

        ReleaseDC(None, dc);
        available
    }
}

/// Resolve the requested family to a family this machine can actually use.
pub fn resolve(requested: &str) -> String {
    let requested = requested.trim();
    if family_available(requested) {
        requested.to_string()
    } else {
        FALLBACK_FAMILY.to_string()
    }
}

fn same_family(actual: &str, requested: &str) -> bool {
    let actual = actual.trim();
    if actual.eq_ignore_ascii_case(requested) {
        return true;
    }
    let Some(prefix) = actual.get(..requested.len()) else {
        return false;
    };
    prefix.eq_ignore_ascii_case(requested) && actual.as_bytes().get(requested.len()) == Some(&b' ')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_suffix_is_same_family() {
        assert!(same_family("MiSans", "MiSans"));
        assert!(same_family("MiSans Semibold", "MiSans"));
        assert!(same_family("microsoft yahei ui", "Microsoft YaHei UI"));
        assert!(!same_family("MiSansDisplay", "MiSans"));
        assert!(!same_family("Arial", "MiSans"));
    }
}
