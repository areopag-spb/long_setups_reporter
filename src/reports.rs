use crate::config::Settings;
use crate::models::PartData;
use crate::{
    db::Database,
    mailer::Mailer,
    utils::{next_send_time, parse_time, retry, MAX_RETRY_ATTEMPTS, RETRY_DELAY},
};
use chrono::Local;
use eyre::Result;
use std::collections::BTreeMap;
use std::fmt::Write as FmtWrite;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use tracing::info;

// Палитра «Work» (Ареопаг) из WpfTemplate, светлая тема
const BG: &str = "#F4F5F6";
const SURFACE: &str = "#FFFFFF";
const HOVER: &str = "#EAECEE";
const LINE: &str = "#D9DEE3";
const TEXT: &str = "#24303A";
const TEXT2: &str = "#5E6B75";
const RED: &str = "#E63C2F";
const FONT: &str = "font-family:'Golos Text','Segoe UI',Arial,sans-serif;";
const MONO: &str = "font-family:'JetBrains Mono',Consolas,monospace;";

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// "Иванов Иван Иванович" -> "Иванов И.И."
fn short_name(full: &str) -> String {
    let mut it = full.split_whitespace();
    let mut out = it.next().unwrap_or("").to_string();
    let initials: String = it
        .filter_map(|p| p.chars().next())
        .map(|c| format!("{}.", c))
        .collect();
    if !initials.is_empty() {
        out.push(' ');
        out.push_str(&initials);
    }
    out
}

fn fmt_count(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{:.1}", v)
    }
}

struct Checked<'a> {
    part: &'a PartData,
    setup: i64,
    limit: i32,
    setup_over: bool,
    dt_over: bool,
}

impl<'a> Checked<'a> {
    fn new(part: &'a PartData, settings: &Settings) -> Self {
        let limit = part
            .db_limit
            .unwrap_or_else(|| settings.get_setup_limit(&part.machine));
        let setup = part.setup_with_partial_minutes();
        Self {
            part,
            setup,
            limit,
            setup_over: setup > limit as i64,
            dt_over: part.is_downtime_over(),
        }
    }

    fn is_problem(&self) -> bool {
        self.setup_over || self.dt_over
    }
}

/// Ширины колонок (px) — одинаковые у всех станков. Колонка «Деталь» занимает остаток.
const W_TIME: u32 = 190;
const W_OPERATOR: u32 = 120;
const W_ORDER: u32 = 140;
const W_COUNT: u32 = 70;
const W_SETUP: u32 = 250;
const W_DOWNTIME: u32 = 130;

const TEXT3: &str = "#97A1AA";
const WARN: &str = "#9B770D";
const INFO: &str = "#1E7AEB";

fn td(content: &str, style: &str, width: Option<u32>) -> String {
    let w = width.map(|w| format!(" width=\"{w}\"")).unwrap_or_default();
    format!(
        "<td{w} style=\"{FONT}font-size:13px;padding:3px 6px;border-bottom:1px solid {LINE};{style}\">{content}</td>"
    )
}

fn th(content: &str, width: Option<u32>) -> String {
    let w = width.map(|w| format!(" width=\"{w}\"")).unwrap_or_default();
    format!(
        "<th align=\"left\"{w} style=\"{FONT}font-size:11px;font-weight:normal;color:{TEXT2};text-transform:uppercase;letter-spacing:0.04em;padding:5px 6px;background:{HOVER};border-bottom:1px solid {LINE};\">{content}</th>"
    )
}

fn red(content: &str) -> String {
    format!("<b style=\"color:{RED};\">{content}</b>")
}

/// Экранирование + сохранение переносов строк и отступов (Outlook игнорирует pre-wrap).
fn multiline(s: &str) -> String {
    s.replace("\r\n", "\n")
        .split('\n')
        .map(|line| {
            let indent = line.chars().take_while(|c| *c == ' ').count();
            format!("{}{}", "&nbsp;".repeat(indent), esc(line.trim_start()))
        })
        .collect::<Vec<_>>()
        .join("<br>")
}

fn is_night(shift: &str) -> bool {
    shift.to_lowercase().starts_with('н')
}

fn shift_icon(shift: &str) -> String {
    if is_night(shift) {
        format!("<span style=\"color:{INFO};font-size:16px;\" title=\"Ночь\">☾</span>")
    } else {
        format!("<span style=\"color:{WARN};font-size:16px;\" title=\"День\">☀</span>")
    }
}

/// «начало наладки › начало изготовления › конец»; отсутствующий этап — «—».
/// Конец наладки совпадает с началом изготовления, поэтому не дублируется.
fn times_cell(p: &PartData) -> String {
    let f = |t: chrono::NaiveDateTime| t.format("%H:%M").to_string();
    // Прочерк шириной в «ЧЧ:ММ» (5 моно-символов), чтобы времена не плыли.
    let dash = "&nbsp;&nbsp;—&nbsp;&nbsp;".to_string();
    let has_setup = p.start_setup_time < p.end_setup_time;
    let has_machining = p.end_setup_time < p.end_machining_time;
    let slots: [String; 3] = match (has_setup, has_machining) {
        (true, true) => [
            f(p.start_setup_time),
            f(p.end_setup_time),
            f(p.end_machining_time),
        ],
        (true, false) => [f(p.start_setup_time), f(p.end_setup_time), dash.clone()],
        (false, true) => [dash.clone(), f(p.end_setup_time), f(p.end_machining_time)],
        (false, false) => [f(p.start_setup_time), dash.clone(), dash.clone()],
    };
    slots.join(&format!(" <span style=\"color:{TEXT3};\">›</span> "))
}

pub fn generate_html_report(
    data: &[PartData],
    settings: &Settings,
    date: &str,
) -> Result<String> {
    let mut by_machine: BTreeMap<&str, Vec<Checked>> = BTreeMap::new();
    for part in data {
        by_machine
            .entry(part.machine.as_str())
            .or_default()
            .push(Checked::new(part, settings));
    }

    let mut html = String::new();
    writeln!(
        html,
        "<html><body style=\"{FONT}color:{TEXT};background:{SURFACE};margin:0;\"><table width=\"100%\" cellspacing=\"0\" cellpadding=\"0\" bgcolor=\"{SURFACE}\" style=\"background:{SURFACE};\"><tr><td style=\"padding:2px 4px;\">\
         <h2 style=\"{FONT}font-size:18px;font-weight:700;margin:0 0 4px 0;\">Работа станков за {date}</h2>"
    )?;

    let problems_total: usize = by_machine
        .values()
        .map(|v| v.iter().filter(|c| c.is_problem()).count())
        .sum();
    let problems_text = if problems_total > 0 {
        red(&format!("требующих внимания: {problems_total}"))
    } else {
        "отклонений нет".to_string()
    };
    writeln!(
        html,
        "<p style=\"{FONT}font-size:13px;margin:0 0 10px 0;\">Станков: {}, записей: {}, {}</p>",
        by_machine.len(),
        data.len(),
        problems_text,
    )?;

    for (machine, rows) in &by_machine {
        let bad = rows.iter().filter(|c| c.is_problem()).count();
        let mark = if bad > 0 {
            format!("{} ", red("■"))
        } else {
            String::new()
        };
        writeln!(
            html,
            "<table cellspacing=\"0\" cellpadding=\"0\" width=\"100%\" style=\"width:100%;border-collapse:collapse;background:{SURFACE};border:1px solid {LINE};margin:0 0 10px 0;\">\
             <tr><td colspan=\"7\" style=\"{FONT}color:{TEXT};font-size:14px;padding:8px 6px;border-bottom:1px solid {LINE};\">{mark}<b>{}</b></td></tr>",
            esc(machine),
        )?;
        writeln!(
            html,
            "<tr>{}{}{}{}{}{}{}</tr>",
            th("Наладка › изгот. › конец", Some(W_TIME)),
            th("Оператор", Some(W_OPERATOR)),
            th("Деталь", None),
            th("М/Л", Some(W_ORDER)),
            th("Шт.", Some(W_COUNT)),
            th("Наладка", Some(W_SETUP)),
            th("Простои", Some(W_DOWNTIME)),
        )?;

        for night in [false, true] {
        let group: Vec<&Checked> = rows
            .iter()
            .filter(|c| is_night(&c.part.shift) == night)
            .collect();
        if group.is_empty() {
            continue;
        }
        let dt: f64 = group.iter().map(|c| c.part.downtime_minutes()).sum();
        writeln!(
            html,
            "<tr><td colspan=\"5\" style=\"{FONT}font-size:13px;background:{BG};padding:5px 6px;border-bottom:1px solid {LINE};\"><table cellspacing=\"0\" cellpadding=\"0\"><tr><td width=\"22\" align=\"center\" style=\"width:22px;text-align:center;\">{}</td><td style=\"{FONT}font-size:13px;\"><b>{}</b></td></tr></table></td>\
             <td colspan=\"2\" align=\"right\" style=\"{FONT}font-size:12px;color:{TEXT2};background:{BG};padding:5px 6px;border-bottom:1px solid {LINE};\">простои за смену: <b style=\"color:{TEXT};\">{} мин</b></td></tr>",
            shift_icon(if night { "Ночь" } else { "День" }),
            if night { "Ночь" } else { "День" },
            fmt_count(dt)
        )?;
        for c in group {
            let p = c.part;
            let net = p.setup_minutes();
            let setup_text = if p.partial_setup > 0.0 && net <= 0 {
                format!("частичная {} мин", c.setup)
            } else if p.partial_setup > 0.0 {
                format!("{} мин (в т.ч. частичная {})", c.setup, fmt_count(p.partial_setup))
            } else if c.setup <= 0 {
                "—".to_string()
            } else {
                format!("{} мин", c.setup)
            };
            let setup_cell = if c.setup_over {
                red(&format!("{setup_text}, лимит {}", c.limit))
            } else {
                setup_text
            };
            let dt_cell = if p.downtime_minutes() <= 0.0 {
                "—".to_string()
            } else {
                match p.downtime_ratio() {
                    Some(r) => {
                        let t = format!(
                            "{} мин ({:.0}%)",
                            fmt_count(p.downtime_minutes()),
                            r * 100.0
                        );
                        if c.dt_over {
                            red(&t)
                        } else {
                            t
                        }
                    }
                    None => format!("{} мин", fmt_count(p.downtime_minutes())),
                }
            };
            let nowrap_mono = format!("white-space:nowrap;{MONO}");
            writeln!(
                html,
                "<tr>{}{}{}{}{}{}{}</tr>",
                td(&times_cell(p), &nowrap_mono, Some(W_TIME)),
                td(
                    &esc(&short_name(&p.operator)),
                    "white-space:nowrap;",
                    Some(W_OPERATOR)
                ),
                td(&format!("{} (уст. {})", esc(&p.part_name), p.setup), "", None),
                td(&esc(&p.order), &nowrap_mono, Some(W_ORDER)),
                td(
                    &format!("{}/{}", fmt_count(p.finished), p.total),
                    &nowrap_mono,
                    Some(W_COUNT)
                ),
                td(&setup_cell, &nowrap_mono, Some(W_SETUP)),
                td(&dt_cell, &nowrap_mono, Some(W_DOWNTIME)),
            )?;
        }
        }
        writeln!(html, "</table>")?;
    }

    let problems: Vec<(&str, &Checked)> = by_machine
        .iter()
        .flat_map(|(m, rows)| rows.iter().filter(|c| c.is_problem()).map(move |c| (*m, c)))
        .collect();

    if !problems.is_empty() {
        writeln!(
            html,
            "<h3 style=\"{FONT}font-size:15px;margin:16px 0 8px 0;color:{TEXT};\">Требуют внимания</h3>"
        )?;
        for (machine, c) in problems {
            let p = c.part;
            let mut reasons: Vec<String> = Vec::new();
            if c.setup_over {
                let kind = if p.partial_setup > 0.0 { "наладка (частичная)" } else { "наладка" };
                reasons.push(format!("{kind} {} мин при лимите {} мин", c.setup, c.limit));
            }
            if c.dt_over {
                reasons.push(format!(
                    "простои {:.0}% (норма до {:.0}%)",
                    p.downtime_ratio().unwrap_or(0.0) * 100.0,
                    p.downtime_ratio_limit() * 100.0
                ));
            }
            let mut notes = String::new();
            let mut add = |label: &str, v: &str| {
                if !v.is_empty() {
                    notes.push_str(&format!(
                        "<div style=\"margin:6px 0 0 0;\"><span style=\"color:{TEXT2};font-size:12px;\">{label}</span><br>{}</div>",
                        multiline(v)
                    ));
                }
            };
            add("Оператор", &p.operators_comment);
            add("Мастер, наладка", &p.master_setup_comment);
            add("Мастер, изготовление", &p.master_machining_comment);
            add("Мастер", &p.master_comment);
            add("Простои", &p.downtimes_comment);
            writeln!(
                html,
                "<table width=\"100%\" cellspacing=\"0\" cellpadding=\"0\" style=\"width:100%;border-collapse:collapse;margin:0 0 8px 0;\"><tr>\
                 <td width=\"3\" bgcolor=\"{RED}\" style=\"background:{RED};width:3px;\"></td>\
                 <td width=\"30\" align=\"center\" valign=\"top\" bgcolor=\"{BG}\" style=\"background:{BG};width:30px;text-align:center;padding:8px 0 0 6px;\">{}</td>\
                 <td bgcolor=\"{BG}\" style=\"{FONT}font-size:13px;background:{BG};padding:8px 12px 8px 4px;\">\
                 <b>{}</b> · {} · <span style=\"{MONO}\">{}</span><br>\
                 {}, уст. {} · М/Л <span style=\"{MONO}\">{}</span><br>{}{}</td></tr></table>",
                shift_icon(&p.shift),
                esc(machine),
                esc(&short_name(&p.operator)),
                times_cell(p),
                esc(&p.part_name),
                p.setup,
                esc(&p.order),
                red(&reasons.join("; ")),
                notes
            )?;
        }
    }

    writeln!(html, "</td></tr></table></body></html>")?;
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
            let data = db.fetch_report_data(&settings).await?;
            let mut mailer = mailer.lock().await;
            mailer.reconnect(&settings.smtp).await?;
            mailer
                .send_report(
                    "Ежедневный отчёт по работе станков",
                    &data,
                    "Уведомлятель",
                    &settings,
                )
                .await
        }
    })
    .await
}
