//! A small stderr logger: `GITCRAFT_LOG=debug` for more.

struct Stderr(log::LevelFilter);

impl log::Log for Stderr {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= self.0 && (m.target().starts_with("gitcraft") || m.level() <= log::Level::Warn)
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            eprintln!("[{}] {}: {}", r.level(), r.target(), r.args());
        }
    }
    fn flush(&self) {}
}

pub fn init() {
    let level = match std::env::var("GITCRAFT_LOG").unwrap_or_default().to_ascii_lowercase().as_str() {
        "debug" | "trace" => log::LevelFilter::Debug,
        "info" => log::LevelFilter::Info,
        "error" => log::LevelFilter::Error,
        _ => log::LevelFilter::Warn,
    };
    let logger: &'static Stderr = Box::leak(Box::new(Stderr(level)));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(level);
    }
}
