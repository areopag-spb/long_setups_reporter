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
use std::io::Write;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::Mutex as TokioMutex;
use tokio::time::{sleep, Duration as TokioDuration};
use tracing::{debug, error, info, warn};
use utils::{init_logger, LoggerLayers::Both};

const MAX_RETRY_ATTEMPTS: usize = 3;
const RETRY_DELAY: u64 = 5;

#[tokio::main]
async fn main() -> Result<()> {
    print!("\x1B]0;{}\x07", "Long Setup Reporter");
    std::io::stdout().flush()?;
    let mut settings = Settings::new()?;

    let _guard = init_logger(&settings, Both);
    info!("Приложение запущено");

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

    let ctrl_c_handler = async {
        signal::ctrl_c()
            .await
            .expect("Не удалось настроить обработку сигнала Ctrl+C");
        info!("Получен сигнал Ctrl+C, завершение работы...");
    };

    let main_task = async {
        loop {
            let now = Local::now();
            let send_time = parse_time(&settings.report.send_time).unwrap();
            let next_run = next_send_time(now, send_time);

            let duration_until_next = next_run - now;
            // let duration_until_next = chrono::TimeDelta::seconds(3); // для тестов
            info!(
                "Следующий отчёт: {}, через {:02}:{:02}:{:02}",
                next_run.format("%d.%m.%y %H:%M:%S"),
                duration_until_next.num_hours(),
                duration_until_next.num_minutes() % 60,
                duration_until_next.num_seconds() % 60,
            );

            sleep(TokioDuration::from_secs(
                (duration_until_next.num_seconds() + settings.general.send_delay as i64) as u64,
            ))
            .await;

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

async fn retry<F, Fut, T>(max_attempts: usize, retry_delay: u64, mut f: F) -> Result<T>
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

fn parse_time(time_str: &str) -> Result<(u32, u32), eyre::Error> {
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() != 2 {
        eyre::bail!("Неверный формат времени");
    }
    let hour: u32 = parts[0].parse()?;
    let minute: u32 = parts[1].parse()?;
    Ok((hour, minute))
}

fn next_send_time(now: chrono::DateTime<Local>, send_time: (u32, u32)) -> chrono::DateTime<Local> {
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
