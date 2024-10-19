mod config;
mod utils;

use config::Settings;
use eyre::Result;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;
use tracing::{error, info};
use utils::{init_logger, LoggerLayers::File};
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
    let _settings = Settings::new()?;

    let _guard = init_logger(&_settings, File);

    let running = Arc::new(AtomicBool::new(true));
    let running_clone = Arc::clone(&running);

    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Stop => {
                running_clone.store(false, Ordering::SeqCst); // Останавливаем службу
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
    status_handle.set_service_status(next_status)?;
    while running.load(Ordering::SeqCst) {
        let file_path = Path::new("C:/1.txt");
        if file_path.exists() {
            info!("Файл существует");
        } else {
            error!("Файл не найден");
        }

        thread::sleep(Duration::from_secs(30));
    }

    let stop_status = ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    };
    status_handle.set_service_status(stop_status)?;
    Ok(())
}

fn main() -> windows_service::Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
}
