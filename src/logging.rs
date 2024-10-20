use std::{env, fs, path::PathBuf};

use crate::config::Settings;
use tracing_appender::{non_blocking::WorkerGuard, rolling};
use tracing_subscriber::{
    fmt::{self, time::ChronoLocal},
    prelude::*,
    EnvFilter, Layer,
};

#[allow(dead_code)]
pub enum LoggerLayers {
    File,
    StdErr,
    Both,
}

pub fn init_logger(settings: &Settings, layer: LoggerLayers) -> Option<WorkerGuard> {
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let log_dir = exe_dir.join("logs");
    fs::create_dir_all(&log_dir).expect("Не удалось создать папку для логов");
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
    let file_name = env::current_exe()
        .ok()
        .and_then(|pb| pb.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "long_setups_reporter".to_string());
    let file_appender = rolling::never(log_dir, format!("{file_name}.log"));
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = fmt::layer()
        .with_writer(file_writer)
        .event_format(format.clone())
        .with_ansi(false)
        .with_filter(file_filter)
        .boxed();

    let console_layer = fmt::layer()
        .pretty()
        .event_format(format.clone())
        .with_filter(console_filter)
        .boxed();

    let mut layers = Vec::new();
    let mut guard_option = None;
    match layer {
        LoggerLayers::File => {
            layers.push(file_layer);
            guard_option = Some(guard);
        }
        LoggerLayers::StdErr => {
            layers.push(console_layer);
        }
        LoggerLayers::Both => {
            layers.push(file_layer);
            layers.push(console_layer);
            guard_option = Some(guard);
        }
    }
    tracing_subscriber::registry().with(layers).init();
    guard_option
}
