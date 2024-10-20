mod config;
mod db;
mod init;
mod logging;
mod mailer;
mod models;
mod reports;
mod tests;
mod utils;

use config::Settings;
use eyre::Result;
use init::{init_db, init_mailer};
use logging::{init_logger, LoggerLayers};
use reports::{calc_delay, send_report_with_retry};
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::time::{sleep, Duration as TokioDuration};
use tracing::{debug, error, info, warn};
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
        let mut settings = Settings::new()?;

        let _guard = init_logger(&settings, LoggerLayers::Both);
        info!("Приложение запущено");
        let db = init_db(&settings).await?;
        let mailer = init_mailer(&settings).await?;
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

        let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

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
        status_handle.set_service_status(next_status)?;

        let main_task = async {
            loop {
                let secs = match calc_delay(&settings) {
                    Ok(s) => s,
                    Err(e) => {
                        error!("Не удалость вычислить время ожидания.\n{}", e);
                        break;
                    }
                };
                debug!("{secs}");
                sleep(TokioDuration::from_secs(secs)).await;

                if let Err(e) = settings.update() {
                    warn!("Не удалось обновить параметры приложения.\n{}", e);
                } else {
                    debug!("Параметры приложения обновлены:\n{:#?}", settings);
                }

                if let Err(e) =
                    send_report_with_retry(Arc::clone(&db), Arc::clone(&mailer), &settings).await
                {
                    error!("Все попытки отправки отчета исчерпаны: {:?}", e);
                } else {
                    info!("Отчёт успешно отправлен");
                }
            }
        };

        tokio::select! {
            _ = async {
                while running.load(Ordering::SeqCst) {
                    tokio::time::sleep(TokioDuration::from_millis(500)).await;
                }
                info!("Сигнал завершения службы получен.");
            } => { }
            _ = main_task => {}
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
