use alloc::format;
use alloc::{string::String, string::ToString, vec::Vec};

/// Convert a UEFI CHAR16* (NUL-terminated) to a Rust UTF-8 String.
/// Returns None if the pointer is NULL or decoding fails.
pub unsafe fn utf16_cstr_to_string(p: *const uefi_raw::Char16) -> Option<String> {
    if p.is_null() {
        return None;
    }
    // Count length until NUL terminator
    let mut len = 0usize;
    loop {
        let ch = unsafe { *p.add(len) };
        if ch == 0 {
            break;
        }
        len += 1;
    }
    let slice = unsafe { core::slice::from_raw_parts(p as *const u16, len) };
    String::from_utf16(slice).ok()
}

/// Normalize a UEFI-style path:
/// - Uses backslash '\' as separator
/// - If name starts with '\' → absolute path from volume root
/// - Otherwise path is relative to `base`
/// - Handles empty string, ".", ".."
/// - Root directory is represented as "\" (a single backslash)
pub fn normalize_uefi_path(base: &str, name: &str) -> String {
    if name.is_empty() || name == "." {
        return base.to_string();
    }

    let combined = if name.starts_with('\\') {
        name.to_string()
    } else if base == "\\" {
        format!("\\{name}")
    } else if base.ends_with('\\') {
        format!("{base}{name}")
    } else {
        format!("{base}\\{name}")
    };

    // Split into components, remove ".", handle ".."
    let mut parts: Vec<&str> = Vec::new();
    for seg in combined.split('\\') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." {
            if !parts.is_empty() {
                parts.pop();
            }
            continue;
        }
        parts.push(seg);
    }

    if parts.is_empty() {
        "\\".to_string()
    } else {
        format!("\\{}", parts.join("\\"))
    }
}
