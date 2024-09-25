#[cfg(test)]
mod tests {
    use crate::config::Settings;
    use crate::db::Database;
    use crate::mailer::Mailer;
    use eyre::Result;
    use std::sync::Arc;
    use tokio::sync::Mutex as TokioMutex;

    #[tokio::test]
    async fn test_send_report() -> Result<()> {
        // Подготовка конфигурации и инициализация объектов
        let settings = Settings::new()?;
        let db = Arc::new(TokioMutex::new(Database::new(&settings).await?));
        let mailer = Arc::new(TokioMutex::new(Mailer::new(&settings.smtp).await?));

        // Получение данных для отчета
        let mut db_lock = db.lock().await;
        let data = db_lock.fetch_report_data().await?;

        // Отправка отчета
        let mut mailer_lock = mailer.lock().await;
        let result = mailer_lock
            .send_report("Тестовый отчет", &data, "Уведомлятель")
            .await;

        assert!(result.is_ok(), "Отчет не был отправлен: {:?}", result);

        Ok(())
    }
}
