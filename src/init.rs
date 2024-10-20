use crate::utils::{retry, MAX_RETRY_ATTEMPTS, RETRY_DELAY};
use crate::{config::Settings, db::Database, mailer::Mailer};
use eyre::Result;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use tracing::info;

pub async fn init_db(settings: &Settings) -> Result<Arc<TokioMutex<Database>>> {
    let db = Arc::new(TokioMutex::new(
        retry(MAX_RETRY_ATTEMPTS, RETRY_DELAY, || Database::new(settings)).await?,
    ));
    info!("Подключение к базе данных установлено");
    Ok(db)
}

pub async fn init_mailer(settings: &Settings) -> Result<Arc<TokioMutex<Mailer>>> {
    let mailer = Arc::new(TokioMutex::new(
        retry(MAX_RETRY_ATTEMPTS, RETRY_DELAY, || {
            Mailer::new(&settings.smtp)
        })
        .await?,
    ));
    info!("Почтовый клиент инициализирован");
    Ok(mailer)
}
