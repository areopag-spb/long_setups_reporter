use crate::{config::Settings, models::PartData};
use eyre::{Context, Result};
use tiberius::{Client, Config};
use tokio::net::TcpStream;
use tokio_util::compat::TokioAsyncWriteCompatExt;
use tracing::debug;

pub struct Database {
    pub client: Option<Client<tokio_util::compat::Compat<TcpStream>>>,
}

impl Database {
    pub async fn new(settings: &Settings) -> Result<Self> {
        let config = Self::create_config(settings)?;
        let tcp = TcpStream::connect(config.get_addr()).await?;
        tcp.set_nodelay(true)?;
        let client = Client::connect(config, tcp.compat_write()).await?;

        Ok(Self {
            client: Some(client),
        })
    }

    fn create_config(settings: &Settings) -> Result<Config> {
        let config_str = format!(
            "Data Source={};Initial Catalog={};TrustServerCertificate=True;User ID={};Password={};",
            settings.database.host,
            settings.database.database,
            settings.database.username,
            settings.database.password
        );
        let mut config = Config::from_ado_string(&config_str)?;
        config.trust_cert();
        Ok(config)
    }

    pub async fn reconnect(&mut self, settings: &Settings) -> Result<()> {
        let config = Self::create_config(settings)?;
        let tcp = TcpStream::connect(config.get_addr()).await?;
        tcp.set_nodelay(true)?;
        let client = Client::connect(config, tcp.compat_write()).await?;

        self.client = Some(client);
        Ok(())
    }

    pub async fn fetch_report_data(&mut self, settings: &Settings) -> Result<Vec<PartData>> {
        const QUERY: &str = r#"
            SELECT
                PartName,
                Setup,
                [Order],
                Machine,
                Operator,
                StartSetupTime,
                StartMachiningTime,
                SetupDowntimes,
                OperatorComment
            FROM
                parts
            WHERE
                ShiftDate = CONVERT(DATE, DATEADD(day, -1, GETDATE()))
            ORDER BY
                StartSetupTime DESC;
        "#;

        let client = self
            .client
            .as_mut()
            .ok_or_else(|| eyre::eyre!("Нет активного подключения к базе данных"))?;

        let results = client
            .simple_query(QUERY)
            .await
            .wrap_err("Ошибка выполнения запроса")?
            .into_results()
            .await
            .wrap_err("Ошибка получения результатов")?;

        let mut filtered_data: Vec<PartData> = Vec::new();

        for row in results.into_iter().flatten() {
            let part_data = match PartData::from_sql_row(&row) {
                Ok(data) => {
                    debug!("Обработка данных для станка: {}", data.machine);
                    debug!("{}", data);
                    data
                }
                Err(e) => {
                    debug!("Ошибка при разборе строки данных: {:?}", e);
                    continue;
                }
            };

            // let start_setup_time = if let Some(prev_part) = filtered_data.last() {
            //     if prev_part.part_name == part_data.part_name
            //         && prev_part.setup == part_data.setup
            //         && prev_part.order == part_data.order
            //     {
            //         prev_part.start_setup_time
            //     } else {
            //         part_data.start_setup_time
            //     }
            // } else {
            //     part_data.start_setup_time
            // };

            let setup_duration = part_data.end_setup_time - part_data.start_setup_time; // тут было start_setup_time
            let actual_minutes = (setup_duration - part_data.breaks_between(true)).num_minutes();
            let limit = settings.get_setup_limit(&part_data.machine);

            if actual_minutes > limit.into() {
                debug!(
                    "Превышение лимита наладки:\nСтанок: {}\n{}\nЛимит: {}\nФактическое время: {}",
                    part_data.machine, part_data.part_name, limit, actual_minutes
                );
                filtered_data.push(part_data);
            }
        }

        Ok(filtered_data)
    }
}
