use crate::config::Settings;
use std::fs;
use std::path::Path;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling;
use tracing_subscriber::{
    fmt::{self, time::ChronoLocal},
    prelude::*,
    EnvFilter, Layer,
};

pub fn init_logger(settings: &Settings) -> WorkerGuard {
    if let Some(parent) = Path::new("logs").parent() {
        fs::create_dir_all(parent).expect("Не удалось создать папку для логов");
    }
    let timer = ChronoLocal::new("%d.%m.%Y %H:%M:%S%.3f".to_string());
    let is_debug = settings.general.log_level.to_uppercase() == "DEBUG";
    let format = fmt::format()
        .pretty()
        .with_level(true)
        .with_target(is_debug)
        .with_source_location(is_debug)
        .with_thread_ids(is_debug)
        .with_thread_names(is_debug)
        .with_timer(timer.clone());
    let file_filter = EnvFilter::new(settings.general.log_level.clone());
    let console_filter = EnvFilter::new(settings.general.log_level.clone());

    let file_appender = rolling::never("logs", "app.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = fmt::layer()
        .with_writer(file_writer)
        .event_format(format.clone())
        .with_ansi(false)
        .with_filter(file_filter);

    let console_layer = fmt::layer()
        .pretty()
        .event_format(format.clone())
        .with_filter(console_filter);

    tracing_subscriber::registry()
        .with(file_layer)
        .with(console_layer)
        .init();

    guard
}
