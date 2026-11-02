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

    pub async fn fetch_report_data(&mut self, _settings: &Settings) -> Result<Vec<PartData>> {
        const QUERY: &str = r#"
            SELECT
                p.PartName,
                p.Setup,
                p.[Order],
                p.Machine,
                p.Shift,
                p.Operator,
                p.FinishedCount,
                p.TotalCount,
                p.StartSetupTime,
                p.StartMachiningTime,
                p.EndMachiningTime,
                p.SetupDowntimes,
                p.MachiningDowntimes,
                p.PartialSetupTime,
                p.OperatorComment,
                p.MasterSetupComment,
                p.MasterMachiningComment,
                p.MasterComment,
                p.SpecifiedDowntimesComment,
                m.SetupLimit
            FROM
                parts p
                LEFT JOIN cnc_machines m ON m.Name = p.Machine
            WHERE
                p.ShiftDate = CONVERT(DATE, DATEADD(day, -1, GETDATE()))
            ORDER BY
                p.Machine, p.StartSetupTime;
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

        let mut data: Vec<PartData> = Vec::new();
        for row in results.into_iter().flatten() {
            match PartData::from_sql_row(&row) {
                Ok(part) => data.push(part),
                Err(e) => debug!("Ошибка при разборе строки данных: {:?}", e),
            }
        }
        Ok(data)
    }
}
