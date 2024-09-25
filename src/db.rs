use crate::{config::Settings, models::PartData};
use eyre::Result;
use tiberius::{Client, Config};
use tokio::net::TcpStream;
use tokio_util::compat::TokioAsyncWriteCompatExt;
use tracing::debug;

pub struct Database {
    pub client: Option<Client<tokio_util::compat::Compat<TcpStream>>>,
    setup_limit: i32,
}

impl Database {
    pub async fn new(settings: &Settings) -> Result<Self> {
        let config = Self::create_config(settings)?;
        let tcp = TcpStream::connect(config.get_addr()).await?;
        tcp.set_nodelay(true)?;
        let client = Client::connect(config, tcp.compat_write()).await?;

        Ok(Self {
            client: Some(client),
            setup_limit: settings.report.setup_limit,
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

    pub async fn fetch_report_data(&mut self) -> Result<Vec<PartData>> {
        if let Some(client) = &mut self.client {
            let results = client
                .query(
                    "SELECT 
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
                        AND DATEDIFF(minute, StartSetupTime, StartMachiningTime) - SetupDowntimes > @P1 
                    ORDER BY 
                        StartSetupTime DESC;",
                    &[&self.setup_limit],
                )
                .await?
                .into_results()
                .await?;

            let mut data = Vec::new();
            for rows in results {
                for row in rows {
                    if let Ok(part_data) = PartData::from_sql_row(&row) {
                        data.push(part_data);
                    } else {
                        debug!("Ошибка при разборе строки данных.");
                    }
                }
            }
            Ok(data)
        } else {
            Err(eyre::eyre!("Нет активного подключения к базе данных"))
        }
    }
}
