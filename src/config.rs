use std::{env, path::PathBuf};

use config::Config;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub database: DatabaseSettings,
    pub smtp: SmtpSettings,
    pub report: ReportSettings,
    pub general: GeneralSettings,
}

#[derive(Debug, Deserialize)]
pub struct DatabaseSettings {
    pub host: String,
    pub username: String,
    pub password: String,
    pub database: String,
}

#[derive(Debug, Deserialize)]
pub struct SmtpSettings {
    pub server: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from: String,
    pub to: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReportSettings {
    pub send_time: String, // Format "HH:MM"
    pub setup_limit: i32,
}

#[derive(Debug, Deserialize)]
pub struct GeneralSettings {
    pub log_level: String,
    pub send_delay: i32,
}

impl Settings {
    pub fn new() -> Result<Self, config::ConfigError> {
        let exe_dir = match env::current_exe() {
            Ok(path) => path.parent().map(PathBuf::from),
            Err(e) => {
                return Err(config::ConfigError::Message(format!(
                    "Ошибка получения пути к исполняемому файлу: {:?}",
                    e
                )))
            }
        };

        let mut config_path = exe_dir.ok_or_else(|| {
            config::ConfigError::Message(
                "Не удалось определить директорию с исполняемым файлом".to_string(),
            )
        })?;
        config_path.push("config/config");

        let cfg = Config::builder()
            .add_source(config::File::with_name(config_path.to_str().unwrap())) // Преобразуем путь в строку
            .build()?;
        cfg.try_deserialize()
    }
    pub fn update(&mut self) -> Result<Self, config::ConfigError> {
        Settings::new()
    }
}
