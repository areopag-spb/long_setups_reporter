mod config;
mod db;
mod mailer;
mod models;
mod reports;
mod tests;
mod utils;

use chrono::Local;
use config::Settings;
use db::Database;
use eyre::Result;
use mailer::Mailer;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::Mutex as TokioMutex;
use tokio::time::{sleep, Duration as TokioDuration};
use tracing::{debug, error, info, warn};
use utils::{init_logger, LoggerLayers::File};
use utils::{next_send_time, parse_time, retry, MAX_RETRY_ATTEMPTS, RETRY_DELAY};
use windows_service::define_windows_service;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_dispatcher;

const SERVICE_NAME: &str = "LSRService";

define_windows_service!(ffi_service_main, service_main);

fn log_to_file(message: &str) {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("C:/lsrs.txt")
        .unwrap();
    let _ = file.write(format!("{message}\n").as_bytes());
}

fn service_main(_arguments: Vec<std::ffi::OsString>) {
    if let Err(e) = run_service() {
        error!("Ошибка в службе: {:?}", e);

        // Устанавливаем статус "Stopped" при ошибке
        if let Ok(status_handle) = service_control_handler::register(SERVICE_NAME, |_| {
            ServiceControlHandlerResult::NoError
        }) {
            let stop_status = ServiceStatus {
                service_type: ServiceType::OWN_PROCESS,
                current_state: ServiceState::Stopped,
                controls_accepted: windows_service::service::ServiceControlAccept::empty(),
                exit_code: ServiceExitCode::Win32(1), // Возвращаем код ошибки
                checkpoint: 0,
                wait_hint: Duration::default(),
                process_id: None,
            };
            log_to_file("Устанавливаем статус \"Stopped\"");
            status_handle
                .set_service_status(stop_status)
                .unwrap_or_else(|e| {
                    error!("Не удалось установить статус остановки службы: {:?}", e);
                });
        }
    }
}

fn run_service() -> Result<()> {
    let runtime = Runtime::new()?;
    let _ = runtime.block_on(async {
        let mut settings: Settings = Settings::new().unwrap();
        let _guard = init_logger(&settings, File);
        debug!("Конфигурация: {:#?}", settings);
        debug!("Guard: {:#?}", _guard);

        let db = Arc::new(TokioMutex::new(
            retry(MAX_RETRY_ATTEMPTS, RETRY_DELAY, || Database::new(&settings)).await?,
        ));
        info!("Подключение к базе данных установлено");

        let mailer = Arc::new(TokioMutex::new(
            retry(MAX_RETRY_ATTEMPTS, RETRY_DELAY, || {
                Mailer::new(&settings.smtp)
            })
            .await?,
        ));
        info!("Почтовый клиент инициализирован");
        info!("Служба инициализирована");
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);

        let event_handler = move |control_event| -> ServiceControlHandlerResult {
            match control_event {
                ServiceControl::Stop => {
                    running_clone.store(false, Ordering::SeqCst);
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        };

        let status_handle = service_control_handler::register(SERVICE_NAME, event_handler).unwrap();

        let next_status = ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        };
        info!("Установка статуса Running");
        status_handle.set_service_status(next_status).unwrap();

        while running.load(Ordering::SeqCst) {
            let file_path = Path::new("C:/1.txt");
            if file_path.exists() {
                info!("Файл существует");
            } else {
                error!("Файл не найден");
            }

            let now = Local::now();
            let send_time = parse_time(&settings.report.send_time).unwrap();
            let next_run = next_send_time(now, send_time);

            let duration_until_next = next_run - now;
            // let duration_until_next = chrono::TimeDelta::seconds(30); // для тестов
            info!(
                "Следующий отчёт: {}, через {:02}:{:02}:{:02}",
                next_run.format("%d.%m.%y %H:%M:%S"),
                duration_until_next.num_hours(),
                duration_until_next.num_minutes() % 60,
                duration_until_next.num_seconds() % 60,
            );

            let sleep_duration =
                (duration_until_next.num_seconds() + settings.general.send_delay as i64) as u64;

            if let Err(e) = settings.update() {
                warn!("Не удалось обновить параметры приложения.\n{}", e);
            } else {
                debug!("Параметры приложения обновлены:\n{:#?}", settings);
            }

            let result = retry(MAX_RETRY_ATTEMPTS, RETRY_DELAY, {
                let db = Arc::clone(&db);
                let mailer = Arc::clone(&mailer);
                let settings = &settings;
                move || {
                    let db = db.clone();
                    let mailer = mailer.clone();
                    async move {
                        let mut db = db.lock().await;
                        if let Err(e) = db.reconnect(settings).await {
                            warn!("Не удалось обновить подключение к БД\n{}", e);
                        } else {
                            debug!("Подключение к БД обновлено");
                        }
                        let data = db.fetch_report_data().await?;
                        let mut mailer = mailer.lock().await;
                        if let Err(e) = mailer.reconnect(&settings.smtp).await {
                            warn!("Не удалось обновить подключение к почтовому серверу\n{}", e);
                        } else {
                            debug!("Подключение к почтовому серверу обновлено");
                        }
                        mailer
                            .send_report(
                                "Ежедневный отчёт по длительным наладкам",
                                &data,
                                "Уведомлятель",
                            )
                            .await
                    }
                }
            })
            .await;

            if let Err(e) = result {
                error!("Все попытки отправки отчета исчерпаны. Ошибка: {:?}", e);
            } else {
                info!("Отчёт отправлен")
            }

            tokio::select! {
                _ = sleep(TokioDuration::from_secs(sleep_duration)) => { }
                _ = async {
                    while running.load(Ordering::SeqCst) {
                        tokio::time::sleep(TokioDuration::from_millis(500)).await;
                    }
                } => {
                    break;
                }
            }
        }

        info!("Остановка службы");
        let stop_status = ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        };
        status_handle.set_service_status(stop_status).unwrap();
        eyre::Ok(())
    });
    Ok(())
}

fn main() -> windows_service::Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
}
