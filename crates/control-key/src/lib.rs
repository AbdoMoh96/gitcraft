//! The control channel's key file, one rule for the app (which writes it) and the MCP bridge
//! (which reads it): `<settings>/control-key.local-<port>`. See `docs/control-protocol.md`, Keys.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::path::{Path, PathBuf};

/// The app's settings folder (preferences, recent repositories, key files): macOS
/// `~/Library/Application Support/GitCraft`, Windows `%APPDATA%\GitCraft`, otherwise
/// `$XDG_CONFIG_HOME/gitcraft` or `~/.config/gitcraft`.
/// `GITCRAFT_PROFILE_DIR` overrides the entire folder for isolated runs.
pub fn settings_dir() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("GITCRAFT_PROFILE_DIR").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(path));
    }
    if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support/GitCraft"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("GitCraft"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|c| c.join("gitcraft"))
    }
}

/// `<settings>/control-key.local-<port>`.
pub fn key_path(settings: &Path, port: u16) -> PathBuf {
    settings.join(format!("control-key.local-{port}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_file_per_port() {
        let dir = Path::new("/settings/gitcraft");
        assert_eq!(key_path(dir, 7990), dir.join("control-key.local-7990"));
        assert_eq!(key_path(dir, 7991), dir.join("control-key.local-7991"));
    }

    #[test]
    fn settings_dir_is_the_apps_folder() {
        if std::env::var_os("GITCRAFT_PROFILE_DIR").is_some() {
            return;
        }
        if let Some(d) = settings_dir() {
            let name = d.file_name().and_then(|n| n.to_str()).unwrap_or("");
            assert!(name == "gitcraft" || name == "GitCraft", "{}", d.display());
        }
    }
}
