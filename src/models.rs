use chrono::NaiveDateTime;
use eyre::Result;
use std::fmt;
use tiberius::Row;

#[derive(Debug)]
pub struct PartData {
    pub part_name: String,
    pub setup: i32,
    pub order: String,
    pub machine: String,
    pub operator: String,
    pub start_setup_time: NaiveDateTime,
    pub end_setup_time: NaiveDateTime,
    pub operators_comment: String,
    pub downtimes: f64,
}

impl PartData {
    pub fn from_sql_row(row: &Row) -> Result<Self> {
        let part_name: &str = row
            .get("PartName")
            .ok_or_else(|| eyre::eyre!("Missing PartName"))?;
        let setup: i32 = row
            .get("Setup")
            .ok_or_else(|| eyre::eyre!("Missing Setup"))?;
        let order: &str = row
            .get("Order")
            .ok_or_else(|| eyre::eyre!("Missing Order"))?;
        let machine: &str = row
            .get("Machine")
            .ok_or_else(|| eyre::eyre!("Missing Machine"))?;
        let operator: &str = row
            .get("Operator")
            .ok_or_else(|| eyre::eyre!("Missing Operator"))?;
        let start_setup_time: NaiveDateTime = row
            .get("StartSetupTime")
            .ok_or_else(|| eyre::eyre!("Missing StartSetupTime"))?;
        let end_setup_time: NaiveDateTime = row
            .get("StartMachiningTime")
            .ok_or_else(|| eyre::eyre!("Missing EndSetupTime"))?;
        let operator_comment: &str = row
            .get("OperatorComment")
            .ok_or_else(|| eyre::eyre!("Missing Order"))?;
        let downtimes: f64 = row
            .get("SetupDowntimes")
            .ok_or_else(|| eyre::eyre!("Missing Downtimes"))?;
        Ok(Self {
            part_name: part_name.to_string(),
            setup,
            order: order.to_string(),
            machine: machine.to_string(),
            operator: operator.to_string(),
            start_setup_time,
            end_setup_time,
            operators_comment: operator_comment.to_string(),
            downtimes,
        })
    }
}

impl fmt::Display for PartData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let setup_duration = self.end_setup_time.signed_duration_since(self.start_setup_time);
        let setup_minutes = setup_duration.num_minutes();
        write!(
            f,
            "Деталь: {}, Установка: {}, М/Л: {}, Оператор: {}, Наладка: {} - {} ({} мин.), Простои: {} мин.,\nКомментарий оператора: {}",
            self.part_name,
            self.setup,
            self.order,
            self.operator,
            self.start_setup_time.format("%d.%m.%y %H:%M:%S"),
            self.end_setup_time.format("%d.%m.%y %H:%M:%S"),
            self.downtimes,
            setup_minutes,
            self.operators_comment
        )
    }
}
