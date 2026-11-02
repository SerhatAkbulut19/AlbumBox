use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::Config;

/// Daily rolling log files under `<data_dir>\logs`, plus the console when
/// `console` is set. Keep the returned guard alive until exit.
pub fn init(config: &Config, console: bool) -> WorkerGuard {
    let _ = std::fs::create_dir_all(config.log_dir());
    let (file_writer, guard) = tracing_appender::non_blocking(tracing_appender::rolling::daily(
        config.log_dir(),
        "albumbox-api.log",
    ));
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level));
    let file_layer = fmt::layer().with_ansi(false).with_writer(file_writer);
    let console_layer = console.then(|| fmt::layer().with_target(false));
    tracing_subscriber::registry().with(filter).with(file_layer).with(console_layer).init();
    guard
}
