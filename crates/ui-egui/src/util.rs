//! Small helpers: clocks and dates, shortcuts, base64, the file manager.

use std::path::Path;

/// Seconds since the Unix epoch (fractional).
pub fn now_s() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// `Mod+Shift+S`, `F5`, `Down` → modifiers and key. `Mod` is ⌘ on macOS, Ctrl elsewhere.
pub fn parse_shortcut(s: &str) -> Option<(egui::Modifiers, egui::Key)> {
    let mut m = egui::Modifiers::NONE;
    let mut key = None;
    for part in s.split('+').map(str::trim) {
        match part {
            "Mod" => m |= egui::Modifiers::COMMAND,
            "Shift" => m |= egui::Modifiers::SHIFT,
            "Alt" => m |= egui::Modifiers::ALT,
            "Ctrl" => m |= egui::Modifiers::CTRL,
            k => key = egui::Key::from_name(k),
        }
    }
    key.map(|k| (m, k))
}

/// A shortcut as the user reads it on this platform.
pub fn shortcut_label(s: &str) -> String {
    let mac = cfg!(target_os = "macos");
    s.split(" / ")
        .next()
        .unwrap_or("")
        .split('+')
        .map(|p| match p {
            "Mod" => {
                if mac {
                    "⌘"
                } else {
                    "Ctrl"
                }
            }
            "Shift" => {
                if mac {
                    "⇧"
                } else {
                    "Shift"
                }
            }
            "Alt" => {
                if mac {
                    "⌥"
                } else {
                    "Alt"
                }
            }
            "Enter" => "Enter",
            "Down" => "↓",
            "Up" => "↑",
            other => other,
        })
        .collect::<Vec<_>>()
        .join(if mac { "" } else { "+" })
}

/// `3 minutes ago`, `2 days ago`, or the date for anything older than a month.
pub fn relative_time(t: i64, now: f64) -> String {
    let d = now - t as f64;
    let plural = |n: i64, unit: &str| if n == 1 { format!("1 {unit} ago") } else { format!("{n} {unit}s ago") };
    if d < 60.0 {
        "just now".into()
    } else if d < 3600.0 {
        plural((d / 60.0) as i64, "minute")
    } else if d < 86_400.0 {
        plural((d / 3600.0) as i64, "hour")
    } else if d < 30.0 * 86_400.0 {
        plural((d / 86_400.0) as i64, "day")
    } else {
        date(t)
    }
}

/// `2026-10-17` (UTC) for seconds since the epoch.
pub fn date(t: i64) -> String {
    let (y, m, d) = civil(t.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// `2026-10-17 14:03` (UTC).
pub fn date_time(t: i64) -> String {
    let secs = t.rem_euclid(86_400);
    format!("{} {:02}:{:02} UTC", date(t), secs / 3600, (secs % 3600) / 60)
}

/// Days since 1970-01-01 → (year, month, day), proleptic Gregorian.
fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk.first().copied().unwrap_or(0), chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18u32, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(char::from(T[((n >> shift) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Show a folder in the system file manager.
pub fn reveal(path: &Path) -> Result<(), String> {
    let prog = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(prog).arg(path).spawn().map(|_| ()).map_err(|e| format!("{prog}: {e}"))
}

/// A stable colour for a person (their avatar circle).
pub fn person_color(name: &str) -> egui::Color32 {
    let h = name.bytes().fold(2_166_136_261u32, |h, b| (h ^ u32::from(b)).wrapping_mul(16_777_619));
    crate::theme::lane_color(h as usize)
}

/// Up to two initials.
pub fn initials(name: &str) -> String {
    let mut it = name.split_whitespace().filter_map(|w| w.chars().next());
    let a = it.next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into());
    let b = it.next_back().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
    format!("{a}{b}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(951_782_400), "2000-02-29");
        assert_eq!(date_time(1_792_243_380), "2026-10-17 13:23 UTC");
        assert_eq!(relative_time(1000, 1030.0), "just now");
        assert_eq!(relative_time(0, 7200.0), "2 hours ago");
        assert_eq!(relative_time(0, 86_400.0 * 1.5), "1 day ago");
    }

    #[test]
    fn shortcuts() {
        assert_eq!(parse_shortcut("Mod+Shift+S"), Some((egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::S)));
        assert_eq!(parse_shortcut("F5"), Some((egui::Modifiers::NONE, egui::Key::F5)));
        assert_eq!(parse_shortcut("Down").map(|x| x.1), Some(egui::Key::ArrowDown));
        assert_eq!(parse_shortcut("Mod+Nope"), None);
    }

    #[test]
    fn base64_matches_rfc4648() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn people() {
        assert_eq!(initials("Ada Lovelace"), "AL");
        assert_eq!(initials("ada"), "A");
        assert_eq!(initials(""), "?");
        assert_eq!(person_color("x"), person_color("x"));
    }
}
