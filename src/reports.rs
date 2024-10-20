use crate::config::Settings;
use crate::models::PartData;
use crate::{
    db::Database,
    mailer::Mailer,
    utils::{next_send_time, parse_time, retry, MAX_RETRY_ATTEMPTS, RETRY_DELAY},
};
use chrono::Local;
use eyre::Result;
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use tracing::info;

pub fn generate_html_report(data: &[PartData]) -> Result<String> {
    let mut grouped_by_machine: HashMap<String, Vec<&PartData>> = HashMap::new();

    for part in data {
        grouped_by_machine
            .entry(part.machine.clone())
            .or_default()
            .push(part);
    }

    let mut html = String::new();
    writeln!(
        html,
        "<html><head><style>
        body {{ font-family: Calibri, sans-serif; margin: 5px; }}
        h3 {{ color: #003366; padding-bottom: 0px; }}
        .part-block {{ border: 1px solid #ddd; padding: 6px; margin: 0px 2px 0px 2px; background-color: #f9f9f9; }}
        .part-block p, .part-block pre {{ margin: 3px 0; font-family: Calibri, sans-serif; }}
        pre {{ white-space: pre-wrap; word-wrap: break-word; }}
        </style></head><body>"
    )?;

    for (machine, parts) in grouped_by_machine {
        writeln!(html, "<h3>{}</h3>", machine)?;

        for part in parts {
            let setup_duration = part
                .end_setup_time
                .signed_duration_since(part.start_setup_time);
            let setup_minutes = setup_duration.num_minutes();
            writeln!(
                html,
                "<div class='part-block'>
                    <p><strong>Деталь:</strong> {}</p>
                    <p><strong>Установка:</strong> {}</p>
                    <p><strong>М/Л:</strong> {}</p>
                    <p><strong>Оператор:</strong> {}</p>
                    <p><strong>Наладка:</strong> {} - {} ({} мин. без учета перерывов)</p>
                    <p><strong>Простои:</strong> {} мин.</p>
                    <p><strong>Комментарий:</strong></p>
                    <pre>{}</pre>
                </div>",
                part.part_name,
                part.setup,
                part.order,
                part.operator,
                part.start_setup_time.format("%H:%M:%S"),
                part.end_setup_time.format("%H:%M:%S"),
                setup_minutes,
                part.downtimes,
                part.operators_comment
            )?;
        }
    }

    writeln!(html, "</body></html>")?;
    Ok(html)
}

pub fn calc_delay(settings: &Settings) -> Result<u64> {
    let now = Local::now();
    let send_time = parse_time(&settings.report.send_time)?;
    let next_run = next_send_time(now, send_time);

    let duration_until_next = next_run - now;
    info!(
        "Следующий отчёт: {}, через {:02}:{:02}:{:02}",
        next_run.format("%d.%m.%y %H:%M:%S"),
        duration_until_next.num_hours(),
        duration_until_next.num_minutes() % 60,
        duration_until_next.num_seconds() % 60,
    );

    let total_delay =
        (duration_until_next.num_seconds() + settings.general.send_delay as i64) as u64;

    Ok(total_delay)
}

pub async fn send_report_with_retry(
    db: Arc<TokioMutex<Database>>,
    mailer: Arc<TokioMutex<Mailer>>,
    settings: &Settings,
) -> Result<()> {
    retry(MAX_RETRY_ATTEMPTS, RETRY_DELAY, || {
        let db = Arc::clone(&db);
        let mailer = Arc::clone(&mailer);
        let settings = settings.clone();
        async move {
            let mut db = db.lock().await;
            db.reconnect(&settings).await?;
            let data = db.fetch_report_data().await?;
            let mut mailer = mailer.lock().await;
            mailer.reconnect(&settings.smtp).await?;
            mailer
                .send_report(
                    "Ежедневный отчёт по длительным наладкам",
                    &data,
                    "Уведомлятель",
                )
                .await
        }
    })
    .await
}
