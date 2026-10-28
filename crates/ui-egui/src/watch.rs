//! Instant reloads: the open repository's folder is watched (the system's file notifications), and
//! a change anywhere but git's object store asks for a reload on the next frame. When watching
//! isn't possible (too many folders, no permission) GitCraft still checks every few seconds.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use notify::{RecommendedWatcher, RecursiveMode, Watcher as _};

pub struct Watcher {
    root: PathBuf,
    _inner: Option<RecommendedWatcher>,
    changed: Arc<AtomicBool>,
}

impl std::fmt::Debug for Watcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Watcher").field("root", &self.root).finish()
    }
}

/// Whether a changed path matters for what GitCraft shows: not objects being written, logs or
/// git's own lock files.
pub fn relevant(path: &Path) -> bool {
    let s = path.to_string_lossy();
    let s = s.replace('\\', "/");
    if let Some(i) = s.find("/.git/") {
        let inside = s.get(i + 6..).unwrap_or("");
        return !(inside.starts_with("objects/")
            || inside.starts_with("logs/")
            || inside.ends_with(".lock")
            || inside.starts_with("gitcraft-rebase"));
    }
    true
}

impl Watcher {
    /// Watch `root`; `ctx` repaints when something changes.
    pub fn new(root: &Path, ctx: egui::Context) -> Watcher {
        let changed = Arc::new(AtomicBool::new(false));
        let flag = changed.clone();
        let inner = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res
                && !matches!(ev.kind, notify::EventKind::Access(_))
                && ev.paths.iter().any(|p| relevant(p))
            {
                flag.store(true, Ordering::Relaxed);
                ctx.request_repaint_after(std::time::Duration::from_millis(150));
            }
        })
        .and_then(|mut w| w.watch(root, RecursiveMode::Recursive).map(|_| w));
        let inner = match inner {
            Ok(w) => Some(w),
            Err(e) => {
                log::info!("not watching {} ({e}); checking every few seconds instead", root.display());
                None
            }
        };
        Watcher { root: root.to_path_buf(), _inner: inner, changed }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether the watch works (otherwise only the periodic check runs).
    pub fn active(&self) -> bool {
        self._inner.is_some()
    }

    /// Something changed since the last call.
    pub fn take(&self) -> bool {
        self.changed.swap(false, Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_counts_as_a_change() {
        assert!(relevant(Path::new("/r/src/main.rs")));
        assert!(relevant(Path::new("/r/.git/HEAD")));
        assert!(relevant(Path::new("/r/.git/refs/heads/main")));
        assert!(!relevant(Path::new("/r/.git/objects/ab/cdef")));
        assert!(!relevant(Path::new("/r/.git/index.lock")));
        assert!(!relevant(Path::new("C:\\r\\.git\\logs\\HEAD")));
    }

    #[test]
    fn notices_a_change() {
        let d = gitcraft_git::testing::TempDir::new("watch").unwrap();
        let w = Watcher::new(d.path(), egui::Context::default());
        if !w.active() {
            return;
        }
        std::fs::write(d.path().join("a.txt"), "x").unwrap();
        let mut seen = false;
        for _ in 0..100 {
            if w.take() {
                seen = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(seen);
    }
}
