use chrono::{NaiveDateTime, Timelike, Duration};
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

    pub fn breaks_between(&self, calc_on_end: bool) -> Duration {
        let mut day_shift_first_break = NaiveDateTime::new(chrono::NaiveDate::from_ymd_opt(1, 1, 1).unwrap(), chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap());  // 09:00:00
        let mut day_shift_second_break = NaiveDateTime::new(chrono::NaiveDate::from_ymd_opt(1, 1, 1).unwrap(), chrono::NaiveTime::from_hms_opt(12, 30, 0).unwrap()); // 12:30:00
        let mut day_shift_third_break = NaiveDateTime::new(chrono::NaiveDate::from_ymd_opt(1, 1, 1).unwrap(), chrono::NaiveTime::from_hms_opt(15, 15, 0).unwrap());  // 15:15:00

        let mut night_shift_first_break = NaiveDateTime::new(chrono::NaiveDate::from_ymd_opt(1, 1, 1).unwrap(), chrono::NaiveTime::from_hms_opt(22, 30, 0).unwrap()); // 22:30:00
        let mut night_shift_second_break = NaiveDateTime::new(chrono::NaiveDate::from_ymd_opt(1, 1, 1).unwrap(), chrono::NaiveTime::from_hms_opt(1, 30, 0).unwrap());  // 01:30:00
        let mut night_shift_third_break = NaiveDateTime::new(chrono::NaiveDate::from_ymd_opt(1, 1, 1).unwrap(), chrono::NaiveTime::from_hms_opt(4, 30, 0).unwrap());   // 04:30:00

        if !calc_on_end {
            day_shift_first_break = day_shift_first_break - Duration::minutes(14);
            day_shift_second_break = day_shift_second_break - Duration::minutes(29);
            day_shift_third_break = day_shift_third_break - Duration::minutes(14);
            night_shift_first_break = night_shift_first_break - Duration::minutes(29);
            night_shift_second_break = night_shift_second_break - Duration::minutes(29);
            night_shift_third_break = night_shift_third_break - Duration::minutes(29);
        }

        let mut breaks = Duration::zero();
        let start_time = self.start_setup_time.num_seconds_from_midnight();
        let mut end_time = self.end_setup_time.num_seconds_from_midnight();

        if start_time > end_time {
            night_shift_second_break = night_shift_second_break + Duration::days(1);
            night_shift_third_break = night_shift_third_break + Duration::days(1);
            end_time += 24 * 60 * 60;
        }

        if day_shift_first_break.num_seconds_from_midnight() > start_time && day_shift_first_break.num_seconds_from_midnight() <= end_time {
            breaks = breaks + Duration::minutes(15);
            if !calc_on_end {
                end_time += 15 * 60;
            }
        }

        if day_shift_second_break.num_seconds_from_midnight() > start_time && day_shift_second_break.num_seconds_from_midnight() <= end_time {
            breaks = breaks + Duration::minutes(30);
            if !calc_on_end {
                end_time += 30 * 60;
            }
        }

        if day_shift_third_break.num_seconds_from_midnight() > start_time && day_shift_third_break.num_seconds_from_midnight() <= end_time {
            breaks = breaks + Duration::minutes(15);
            if !calc_on_end {
                end_time += 15 * 60;
            }
        }

        if night_shift_first_break.num_seconds_from_midnight() > start_time && night_shift_first_break.num_seconds_from_midnight() <= end_time {
            breaks = breaks + Duration::minutes(30);
            if !calc_on_end {
                end_time += 30 * 60;
            }
        }

        if night_shift_second_break.num_seconds_from_midnight() > start_time && night_shift_second_break.num_seconds_from_midnight() <= end_time {
            breaks = breaks + Duration::minutes(30);
            if !calc_on_end {
                end_time += 30 * 60;
            }
        }

        if night_shift_third_break.num_seconds_from_midnight() > start_time && night_shift_third_break.num_seconds_from_midnight() <= end_time {
            breaks = breaks + Duration::minutes(30);
        }

        breaks
    }
}

impl fmt::Display for PartData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let setup_duration = self
            .end_setup_time
            .signed_duration_since(self.start_setup_time);
        let breaks_minutes = self.breaks_between(true).num_minutes();
        let setup_minutes = setup_duration.num_minutes() - breaks_minutes;
        
        write!(
            f,
            "Деталь: {}\n Установка: {}\nМ/Л: {}\nОператор: {}\nНаладка: {} - {} ({} мин.)\nПростои: {} мин.\nПерерывы: {} мин.\nКомментарий оператора: {}",
            self.part_name,
            self.setup,
            self.order,
            self.operator,
            self.start_setup_time.format("%d.%m.%y %H:%M:%S"),
            self.end_setup_time.format("%d.%m.%y %H:%M:%S"),
            setup_minutes,
            self.downtimes,
            breaks_minutes,
            self.operators_comment
        )
    }
}
