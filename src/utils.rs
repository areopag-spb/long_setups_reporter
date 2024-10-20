use chrono::Local;
use eyre::Result;
use tokio::time::sleep;
use tokio::time::Duration as TokioDuration;
use tracing::{info, warn};

pub const MAX_RETRY_ATTEMPTS: usize = 3;
pub const RETRY_DELAY: u64 = 5;

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
