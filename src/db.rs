use crate::{config::DatabaseSettings, models::PartData};
use eyre::Result;
use tiberius::{Client, Config};
use tokio::net::TcpStream;
use tokio_util::compat::TokioAsyncWriteCompatExt;
use tracing::debug;

pub struct Database {
    pub client: Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
}

impl Database {
    pub async fn new(settings: &DatabaseSettings) -> Result<Self> {
        let mut config = Config::from_ado_string(
            format!(
            "Data Source={};Initial Catalog={};TrustServerCertificate=True;User ID={};Password={};",
            settings.host, settings.database, settings.username, settings.password
        )
            .as_str(),
        )?;
        config.trust_cert();
        let tcp = TcpStream::connect(config.get_addr()).await?;
        tcp.set_nodelay(true)?;
        let client = Client::connect(config, tcp.compat_write()).await?;

        Ok(Self { client })
    }

    pub async fn fetch_report_data(&mut self) -> Result<Vec<PartData>> {
        // Выполнение SQL-запроса
        let results = self
            .client
            .simple_query("SELECT 
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
    AND DATEDIFF(minute, StartSetupTime, StartMachiningTime) - SetupDowntimes > 180 
ORDER BY 
    StartSetupTime DESC;")
            .await?.into_results().await?;

        let mut data = Vec::new();
        for rows in results {
            for row in rows {
                match PartData::from_sql_row(&row) {
                    Ok(part_data) => data.push(part_data),
                    Err(e) => {
                        debug!("Error parsing row: {:?}", e);
                    }
                }
            }
        }
        Ok(data)
    }
}
