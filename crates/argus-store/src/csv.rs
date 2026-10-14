use crate::StoredEvent;

/// One CSV cell. Quotes cells that need it, and defuses spreadsheet formulas: a cell that starts
/// with `=`, `+`, `-`, `@`, tab or carriage return would be executed by Excel, so it gets a
/// leading apostrophe. Rule names and details come from the user, so this is not optional.
fn cell(text: &str) -> String {
    let defused = if text.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{text}")
    } else {
        text.to_string()
    };
    // The separator is a semicolon (pt-BR Excel), so a decimal or text comma needs no quoting.
    if defused.contains(['"', '\n', '\r', ';']) {
        format!("\"{}\"", defused.replace('"', "\"\""))
    } else {
        defused
    }
}

/// UTC time as `YYYY-MM-DD HH:MM:SS`, without a time library.
pub fn format_utc(ts_ms: i64) -> String {
    timestamp(ts_ms)
}

fn timestamp(ts_ms: i64) -> String {
    let secs = ts_ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// The history as CSV for the Brazilian Excel: semicolon separated (the list separator of pt-BR
/// Windows), decimal comma, UTF-8 with a BOM so accents survive.
pub fn to_csv(events: &[StoredEvent]) -> String {
    let mut out =
        String::from("\u{feff}quando_utc;gravidade;regra;sensor;evento;valor;detalhe;teste\r\n");
    for e in events {
        let ev = &e.event;
        let row = [
            cell(&timestamp(ev.ts_ms)),
            cell(ev.severity.as_str()),
            cell(&ev.rule_name),
            cell(&ev.sensor),
            cell(ev.kind.as_str()),
            cell(
                &ev.value
                    .map(|v| format!("{v:.2}").replace('.', ","))
                    .unwrap_or_default(),
            ),
            cell(&ev.detail),
            cell(if ev.is_test { "sim" } else { "não" }),
        ];
        out.push_str(&row.join(";"));
        out.push_str("\r\n");
    }
    out
}

/// One exported sensor sample.
pub struct SensorRow {
    pub ts_ms: i64,
    pub sensor: String,
    pub name: String,
    pub unit: String,
    /// The numeric value, or `None` when the sensor could not be read at that moment.
    pub value: Option<f64>,
    /// Why there is no value ("indisponível: ..." or "sem resposta").
    pub note: String,
}

/// The session's sensor history as CSV, same dialect as the alert history. Unreadable moments keep
/// their row with an empty value and the reason, so gaps are visible instead of being zeros.
pub fn sensors_csv(rows: &[SensorRow]) -> String {
    let mut out = String::from("\u{feff}quando_utc;sensor;nome;unidade;valor;observacao\r\n");
    for r in rows {
        let value = r
            .value
            .map(|v| format!("{v:.3}").replace('.', ","))
            .unwrap_or_default();
        let row = [
            cell(&timestamp(r.ts_ms)),
            cell(&r.sensor),
            cell(&r.name),
            cell(&r.unit),
            cell(&value),
            cell(&r.note),
        ];
        out.push_str(&row.join(";"));
        out.push_str("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_alerts::{AlertEvent, EventKind, Severity};

    fn stored(name: &str, detail: &str) -> StoredEvent {
        StoredEvent {
            id: 1,
            event: AlertEvent {
                ts_ms: 1_791_840_000_000,
                rule_id: 1,
                rule_name: name.to_string(),
                sensor: "cpu.temp".into(),
                severity: Severity::Crit,
                kind: EventKind::Fired,
                detail: detail.to_string(),
                value: Some(91.5),
                is_test: false,
            },
        }
    }

    #[test]
    fn formulas_are_defused() {
        for evil in [
            "=HYPERLINK(\"http://x\")",
            "+1+1",
            "-2+3",
            "@SUM(A1)",
            "\tcmd",
            "\rcmd",
        ] {
            assert!(cell(evil).trim_matches('"').starts_with('\''), "{evil:?}");
        }
        assert_eq!(cell("normal"), "normal");
        // A minus sign inside the text is fine.
        assert_eq!(cell("a-b"), "a-b");
    }

    #[test]
    fn quotes_separators_quotes_and_newlines_but_not_plain_commas() {
        // The separator is a semicolon, so a comma is ordinary text (and a decimal comma).
        assert_eq!(cell("a,b"), "a,b");
        assert_eq!(cell("a;b"), "\"a;b\"");
        assert_eq!(cell("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(cell("two\nlines"), "\"two\nlines\"");
    }

    #[test]
    fn csv_has_a_bom_header_and_one_row_per_event() {
        let csv = to_csv(&[stored("CPU, quente", "passou de 85 °C")]);
        assert!(csv.starts_with('\u{feff}'));
        let lines: Vec<&str> = csv.trim_end().split("\r\n").collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[1].contains(";CPU, quente;"), "{}", lines[1]);
        assert!(lines[1].contains("91,50"));
        assert!(lines[1].ends_with(";não"));
    }

    #[test]
    fn malicious_rule_names_cannot_inject_formulas() {
        let csv = to_csv(&[stored("=cmd|' /C calc'!A0", "ok")]);
        assert!(csv.contains("'=cmd"));
        assert!(!csv.contains(";=cmd"));
    }

    #[test]
    fn sensor_history_csv_keeps_gaps_visible_and_defuses_formulas() {
        let rows = [
            SensorRow {
                ts_ms: 0,
                sensor: "cpu.load".into(),
                name: "Carga".into(),
                unit: "%".into(),
                value: Some(12.5),
                note: String::new(),
            },
            SensorRow {
                ts_ms: 1000,
                sensor: "cpu.temp".into(),
                name: "=Temperatura".into(),
                unit: "°C".into(),
                value: None,
                note: "indisponível: sem sensor".into(),
            },
        ];
        let csv = sensors_csv(&rows);
        assert!(csv.starts_with('\u{feff}'));
        let lines: Vec<&str> = csv.trim_end().split("\r\n").collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[1].contains(";12,500;"), "{}", lines[1]);
        assert!(lines[2].contains(";'=Temperatura;"), "{}", lines[2]);
        assert!(
            lines[2].ends_with(";;indisponível: sem sensor"),
            "empty value, reason kept: {}",
            lines[2]
        );
    }

    #[test]
    fn timestamps_are_utc_dates() {
        assert_eq!(timestamp(0), "1970-01-01 00:00:00");
        assert_eq!(timestamp(951_782_400_000), "2000-02-29 00:00:00");
        assert_eq!(timestamp(1_791_840_000_000), "2026-10-12 21:20:00");
    }
}
