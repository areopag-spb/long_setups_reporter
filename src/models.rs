use chrono::{NaiveDateTime, Timelike, Duration};
use eyre::Result;
use tiberius::Row;

pub const MIN_RECORD_MINUTES: i64 = 60;

#[derive(Debug)]
pub struct PartData {
    pub part_name: String,
    pub setup: i32,
    pub order: String,
    pub machine: String,
    pub shift: String,
    pub operator: String,
    pub finished: f64,
    pub total: i32,
    pub start_setup_time: NaiveDateTime,
    pub end_setup_time: NaiveDateTime,
    pub end_machining_time: NaiveDateTime,
    pub setup_downtimes: f64,
    pub machining_downtimes: f64,
    pub partial_setup: f64,
    pub operators_comment: String,
    pub master_setup_comment: String,
    pub master_machining_comment: String,
    pub master_comment: String,
    pub downtimes_comment: String,
    /// Лимит наладки из cnc_machines.SetupLimit
    pub db_limit: Option<i32>,
}

fn text(row: &Row, col: &str) -> String {
    row.get::<&str, _>(col).unwrap_or("").trim().to_string()
}

impl PartData {
    pub fn from_sql_row(row: &Row) -> Result<Self> {
        let miss = |c: &str| eyre::eyre!("Missing {c}");
        Ok(Self {
            part_name: text(row, "PartName"),
            setup: row.get("Setup").ok_or_else(|| miss("Setup"))?,
            order: text(row, "Order"),
            machine: text(row, "Machine"),
            shift: text(row, "Shift"),
            operator: text(row, "Operator"),
            finished: row.get("FinishedCount").unwrap_or(0.0),
            total: row.get("TotalCount").unwrap_or(0),
            start_setup_time: row
                .get("StartSetupTime")
                .ok_or_else(|| miss("StartSetupTime"))?,
            end_setup_time: row
                .get("StartMachiningTime")
                .ok_or_else(|| miss("StartMachiningTime"))?,
            end_machining_time: row
                .get("EndMachiningTime")
                .ok_or_else(|| miss("EndMachiningTime"))?,
            setup_downtimes: row.get("SetupDowntimes").unwrap_or(0.0),
            machining_downtimes: row.get("MachiningDowntimes").unwrap_or(0.0),
            partial_setup: row.get("PartialSetupTime").unwrap_or(0.0),
            operators_comment: text(row, "OperatorComment"),
            master_setup_comment: text(row, "MasterSetupComment"),
            master_machining_comment: text(row, "MasterMachiningComment"),
            master_comment: text(row, "MasterComment"),
            downtimes_comment: text(row, "SpecifiedDowntimesComment"),
            db_limit: row.get("SetupLimit"),
        })
    }

    /// Наладка вместе с частичной: за вычетом перерывов и простоев. Сравнивается с лимитом.
    pub fn setup_with_partial_minutes(&self) -> i64 {
        let gross = (self.end_setup_time - self.start_setup_time).num_minutes()
            - self.breaks_between(true).num_minutes();
        (gross - self.setup_downtimes.round() as i64).max(0)
    }

    /// Чистая наладка без частичной (частичная - особый простой, факт при ней 0).
    pub fn setup_minutes(&self) -> i64 {
        (self.setup_with_partial_minutes() - self.partial_setup.round() as i64).max(0)
    }

    /// Всё время записи (начало наладки - конец изготовления) за вычетом перерывов.
    pub fn total_minutes(&self) -> i64 {
        (self.end_machining_time - self.start_setup_time).num_minutes()
            - breaks_in(self.start_setup_time, self.end_machining_time, true).num_minutes()
    }

    pub fn downtime_minutes(&self) -> f64 {
        self.setup_downtimes + self.machining_downtimes
    }

    /// Доля простоев от общего времени записи (K2).
    pub fn downtime_ratio(&self) -> Option<f64> {
        let total = self.total_minutes();
        (total > 0).then(|| self.downtime_minutes() / total as f64)
    }

    /// Граница K2: 15% для Hyundai SKT21, 10% для остальных.
    pub fn downtime_ratio_limit(&self) -> f64 {
        if self.machine.to_lowercase().contains("skt21") {
            0.15
        } else {
            0.10
        }
    }

    /// Простои оцениваются только для записей не короче часа.
    pub fn is_downtime_over(&self) -> bool {
        self.total_minutes() >= MIN_RECORD_MINUTES
            && self
                .downtime_ratio()
                .is_some_and(|r| r > self.downtime_ratio_limit())
    }

    pub fn breaks_between(&self, calc_on_end: bool) -> Duration {
        breaks_in(self.start_setup_time, self.end_setup_time, calc_on_end)
    }
}

pub fn breaks_in(start: NaiveDateTime, end: NaiveDateTime, calc_on_end: bool) -> Duration {
    {
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
        let start_time = start.num_seconds_from_midnight();
        let mut end_time = end.num_seconds_from_midnight();

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
