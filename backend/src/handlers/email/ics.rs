//! Invitaciones de calendario (text/calendar, RFC 5545) adjuntas a un correo.
//! Se leen sin IA: es exacto y no cuesta nada.

use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};

#[derive(Debug, Clone, PartialEq)]
pub struct IcsEvent {
    pub summary: String,
    pub date: String,             // YYYY-MM-DD (hora local)
    pub time: String,             // HH:MM
    pub end_time: Option<String>, // HH:MM, mismo dia
    pub location: Option<String>,
    pub description: String,
    /// METHOD:CANCEL o STATUS:CANCELLED
    pub cancelled: bool,
}

/// Busca partes text/calendar (o adjuntos .ics) y extrae sus VEVENT
pub fn events_from_mail(parsed: &mailparse::ParsedMail) -> Vec<IcsEvent> {
    let mut out = Vec::new();
    collect(parsed, &mut out);
    out.truncate(5);
    out
}

fn collect(part: &mailparse::ParsedMail, out: &mut Vec<IcsEvent>) {
    let mime = part.ctype.mimetype.to_ascii_lowercase();
    let is_ics_name = part
        .get_content_disposition()
        .params
        .get("filename")
        .is_some_and(|n| n.to_ascii_lowercase().ends_with(".ics"));
    if mime == "text/calendar" || mime == "application/ics" || is_ics_name {
        if let Ok(body) = part.get_body() {
            out.extend(parse_ics(&body));
        }
    }
    for sub in &part.subparts {
        collect(sub, out);
    }
}

/// Parser minimo de VEVENT (SUMMARY, DTSTART, DTEND, LOCATION, DESCRIPTION, STATUS)
pub fn parse_ics(text: &str) -> Vec<IcsEvent> {
    // "Unfolding": una linea que empieza con espacio o tab continua la anterior
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let raw = raw.trim_end_matches('\r');
        if (raw.starts_with(' ') || raw.starts_with('\t')) && !lines.is_empty() {
            lines.last_mut().unwrap().push_str(&raw[1..]);
        } else {
            lines.push(raw.to_string());
        }
    }

    let method_cancel = lines.iter().any(|l| l.eq_ignore_ascii_case("METHOD:CANCEL"));
    let mut events = Vec::new();
    let mut cur: Option<Vec<(String, String, String)>> = None; // (nombre, params, valor)

    for line in lines {
        if line.eq_ignore_ascii_case("BEGIN:VEVENT") {
            cur = Some(Vec::new());
            continue;
        }
        if line.eq_ignore_ascii_case("END:VEVENT") {
            if let Some(props) = cur.take() {
                if let Some(ev) = build_event(&props, method_cancel) {
                    events.push(ev);
                }
            }
            continue;
        }
        if let Some(props) = cur.as_mut() {
            let Some((head, value)) = line.split_once(':') else { continue };
            let (name, params) = head.split_once(';').unwrap_or((head, ""));
            props.push((name.to_ascii_uppercase(), params.to_string(), value.to_string()));
        }
    }
    events
}

fn build_event(props: &[(String, String, String)], method_cancel: bool) -> Option<IcsEvent> {
    let get = |n: &str| props.iter().find(|(name, _, _)| name == n);
    let summary = get("SUMMARY").map(|(_, _, v)| unescape(v)).unwrap_or_else(|| "Evento".to_string());
    let (_, sp, sv) = get("DTSTART")?;
    let start = parse_dt(sp, sv)?;
    let end = get("DTEND").and_then(|(_, p, v)| parse_dt(p, v));

    let (date, time) = (start.date(), start.time());
    let end_time = end
        .filter(|e| e.date() == date && *e > start)
        .map(|e| e.time().format("%H:%M").to_string());
    let cancelled = method_cancel
        || get("STATUS").is_some_and(|(_, _, v)| v.eq_ignore_ascii_case("CANCELLED"));

    Some(IcsEvent {
        summary: crate::textutil::ellipsize(summary.trim(), 200),
        date: date.format("%Y-%m-%d").to_string(),
        time: time.format("%H:%M").to_string(),
        end_time,
        location: get("LOCATION").map(|(_, _, v)| unescape(v)).filter(|s| !s.trim().is_empty()),
        description: get("DESCRIPTION").map(|(_, _, v)| crate::textutil::ellipsize(&unescape(v), 1000)).unwrap_or_default(),
        cancelled,
    })
}

/// DTSTART;VALUE=DATE:20261120 | DTSTART:20261120T150000Z | DTSTART;TZID=America/Santiago:20261120T120000
/// Devuelve fecha-hora local. Con TZID se asume que es la zona del NAS (no hay base de zonas horarias).
fn parse_dt(params: &str, value: &str) -> Option<NaiveDateTime> {
    let value = value.trim();
    if params.to_ascii_uppercase().contains("VALUE=DATE") || value.len() == 8 {
        let d = NaiveDate::parse_from_str(value, "%Y%m%d").ok()?;
        return Some(d.and_time(NaiveTime::from_hms_opt(9, 0, 0)?));
    }
    if let Some(utc) = value.strip_suffix('Z') {
        let naive = NaiveDateTime::parse_from_str(utc, "%Y%m%dT%H%M%S").ok()?;
        return Some(Utc.from_utc_datetime(&naive).with_timezone(&Local).naive_local());
    }
    NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S").ok()
}

fn unescape(v: &str) -> String {
    v.replace("\\n", "\n").replace("\\N", "\n").replace("\\,", ",").replace("\\;", ";").replace("\\\\", "\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    const INVITE: &str = "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nSUMMARY:Reunion de lab\r\n oratorio\r\nDTSTART;TZID=America/Santiago:20261120T150000\r\nDTEND;TZID=America/Santiago:20261120T163000\r\nLOCATION:Sala 3\\, edificio B\r\nDESCRIPTION:Traer avances\\nGracias\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    #[test]
    fn lee_invitacion() {
        let ev = &parse_ics(INVITE)[0];
        assert_eq!(ev.summary, "Reunion de laboratorio");
        assert_eq!(ev.date, "2026-11-20");
        assert_eq!(ev.time, "15:00");
        assert_eq!(ev.end_time.as_deref(), Some("16:30"));
        assert_eq!(ev.location.as_deref(), Some("Sala 3, edificio B"));
        assert_eq!(ev.description, "Traer avances\nGracias");
        assert!(!ev.cancelled);
    }

    #[test]
    fn dia_completo_y_cancelacion() {
        let ics = "BEGIN:VCALENDAR\nMETHOD:CANCEL\nBEGIN:VEVENT\nSUMMARY:Feriado\nDTSTART;VALUE=DATE:20261208\nEND:VEVENT\nEND:VCALENDAR";
        let ev = &parse_ics(ics)[0];
        assert_eq!(ev.date, "2026-12-08");
        assert_eq!(ev.time, "09:00");
        assert!(ev.cancelled);
    }

    #[test]
    fn utc_se_pasa_a_local() {
        let ics = "BEGIN:VEVENT\nDTSTART:20260601T120000Z\nEND:VEVENT";
        let ev = &parse_ics(ics)[0];
        let esperado = Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap().with_timezone(&Local);
        assert_eq!(ev.time, esperado.format("%H:%M").to_string());
    }

    #[test]
    fn dentro_de_un_correo() {
        let raw = format!(
            "From: a@b.c\r\nSubject: Invitacion\r\nContent-Type: multipart/mixed; boundary=XX\r\n\r\n--XX\r\nContent-Type: text/plain\r\n\r\nHola\r\n--XX\r\nContent-Type: text/calendar; method=REQUEST\r\n\r\n{}--XX--\r\n",
            INVITE
        );
        let parsed = mailparse::parse_mail(raw.as_bytes()).unwrap();
        assert_eq!(events_from_mail(&parsed).len(), 1);
    }
}
