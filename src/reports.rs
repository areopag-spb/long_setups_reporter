use crate::models::PartData;
use eyre::Result;
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;

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
