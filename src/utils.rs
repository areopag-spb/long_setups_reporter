use crate::config::Settings;
use chrono::Local;
use eyre::Result;
use std::path::PathBuf;
use std::{env, fs};
use tokio::time::sleep;
use tokio::time::Duration as TokioDuration;
use tracing::{info, warn};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling;
use tracing_subscriber::{
    fmt::{self, time::ChronoLocal},
    prelude::*,
    EnvFilter, Layer,
};

pub const MAX_RETRY_ATTEMPTS: usize = 3;
pub const RETRY_DELAY: u64 = 5;

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

pub async fn retry<F, Fut, T>(max_attempts: usize, retry_delay: u64, mut f: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    for attempt in 1..=max_attempts {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                warn!(
                    "Попытка {} из {} не удалась. Ошибка: {:?}",
                    attempt, max_attempts, e
                );
                if attempt < max_attempts {
                    info!("Повторная попытка через {} секунд...", retry_delay);
                    sleep(TokioDuration::from_secs(retry_delay)).await;
                } else {
                    return Err(e);
                }
            }
        }
    }
    Err(eyre::eyre!("Все попытки выполнения операции исчерпаны"))
}

pub fn parse_time(time_str: &str) -> Result<(u32, u32), eyre::Error> {
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() != 2 {
        eyre::bail!("Неверный формат времени");
    }
    let hour: u32 = parts[0].parse()?;
    let minute: u32 = parts[1].parse()?;
    Ok((hour, minute))
}

pub fn next_send_time(
    now: chrono::DateTime<Local>,
    send_time: (u32, u32),
) -> chrono::DateTime<Local> {
    let mut next = now
        .date_naive()
        .and_hms_opt(send_time.0, send_time.1, 0)
        .expect("Неверное время")
        .and_local_timezone(now.timezone())
        .unwrap();
    if next <= now {
        next += chrono::Duration::days(1);
    }
    next
}
