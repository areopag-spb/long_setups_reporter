use crate::{config::Settings, models::PartData, utils::ToI64};
use eyre::Result;
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
        if let Some(client) = &mut self.client {
            let results = client
                .simple_query(
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
                    ORDER BY 
                        StartSetupTime DESC;",
                )
                .await?
                .into_results()
                .await?;

            let mut data = Vec::new();
            for rows in results {
                for row in rows {
                    if let Ok(part_data) = PartData::from_sql_row(&row) {
                        debug!(
                            "---\nСтанок: {}\nЛимит: {}\n",
                            part_data.machine,
                            settings.get_setup_limit(&part_data.machine)
                        );
                        debug!("{:#?}", settings.limits);
                        if (part_data.end_setup_time - part_data.start_setup_time).num_minutes()
                            - part_data
                                .downtimes
                                .to_i64(settings.report.default_setup_limit)
                            > settings.get_setup_limit(&part_data.machine).into()
                        {
                            data.push(part_data);
                        }
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
