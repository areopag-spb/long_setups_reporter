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
        let cfg = config::Config::builder()
            .add_source(config::File::with_name("config/config"))
            .build()?;
        cfg.try_deserialize()
    }
    pub fn update(&mut self) -> Result<Self, config::ConfigError> {
        Settings::new()
    }
}
