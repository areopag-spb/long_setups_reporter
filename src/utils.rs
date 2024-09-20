use crate::config::Settings;
use std::fs;
use std::path::Path;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling;
use tracing_subscriber::{fmt, prelude::*, EnvFilter, Layer};

pub fn init_logger(settings: &Settings) -> WorkerGuard {
    if let Some(parent) = Path::new("logs").parent() {
        fs::create_dir_all(parent).expect("Не удалось создать папку для логов");
    }

    let file_filter = EnvFilter::new(settings.general.log_level.clone());
    let console_filter = EnvFilter::new(settings.general.log_level.clone());

    let file_appender = rolling::daily("logs", "app.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = fmt::layer()
        .with_writer(file_writer)
        .with_filter(file_filter);

    let console_layer = fmt::layer().pretty().with_filter(console_filter);

    tracing_subscriber::registry()
        .with(file_layer)
        .with(console_layer)
        .init();

    guard
}
