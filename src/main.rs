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
use std::io::Write;
use std::sync::Arc;
use tokio::signal;
use tokio::time::{sleep, Duration as TokioDuration};
use tracing::{debug, error, info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    print!("\x1B]0;{}\x07", "Long Setup Reporter");
    std::io::stdout().flush()?;
    let mut settings = Settings::new()?;

    let _guard = init_logger(&settings, LoggerLayers::Both);
    info!("Приложение запущено");
    let db = init_db(&settings).await?;
    let mailer = init_mailer(&settings).await?;
    debug!("Приложение инициализировано с параметрами:\n{settings}");
    let ctrl_c_handler = async {
        signal::ctrl_c()
            .await
            .expect("Не удалось настроить обработку сигнала Ctrl+C");
        info!("Получен сигнал Ctrl+C, завершение работы...");
    };

    let main_task = async {
        loop {
            let secs = match calc_delay(&settings) {
                Ok(s) => s,
                Err(e) => {
                    error!("Не удалость вычислить время ожидания.\n{}", e);
                    break;
                }
            };
            sleep(TokioDuration::from_secs(secs)).await;

            if let Err(e) = settings.update() {
                warn!("Не удалось обновить параметры приложения: {}\nИспользуются предыдущие настройки.", e);
                debug!("Текущие параметры приложения:\n{}", settings);
            } else {
                debug!("Параметры приложения успешно обновлены:\n{}", settings);
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
        _ = ctrl_c_handler => {
            info!("Приложение завершено.");
        }
        _ = main_task => {}
    }

    Ok(())
}
