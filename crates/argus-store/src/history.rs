use argus_alerts::{AlertEvent, EventKind, Severity};
use rusqlite::{params_from_iter, Connection, ToSql};
use std::fmt;
use std::path::Path;

#[derive(Debug)]
pub struct HistoryError(pub String);

impl fmt::Display for HistoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for HistoryError {}

impl From<rusqlite::Error> for HistoryError {
    fn from(e: rusqlite::Error) -> Self {
        Self(e.to_string())
    }
}

/// An event with the id the database gave it.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredEvent {
    pub id: i64,
    pub event: AlertEvent,
}

/// What to list. `include_tests = false` hides events produced by "Simular".
#[derive(Debug, Clone, Default)]
pub struct HistoryFilter {
    pub severity: Option<Severity>,
    pub rule_id: Option<u64>,
    /// Only these kinds; empty means every kind.
    pub kinds: Vec<EventKind>,
    pub include_tests: bool,
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub limit: Option<u32>,
}

pub struct HistoryStore {
    conn: Connection,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS events (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    ts_ms     INTEGER NOT NULL,
    rule_id   INTEGER NOT NULL,
    rule_name TEXT    NOT NULL,
    sensor    TEXT    NOT NULL,
    severity  TEXT    NOT NULL,
    kind      TEXT    NOT NULL,
    detail    TEXT    NOT NULL,
    value     REAL,
    is_test   INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_events_ts ON events (ts_ms);
CREATE INDEX IF NOT EXISTS idx_events_rule ON events (rule_id, ts_ms);
";

impl HistoryStore {
    pub fn open(path: &Path) -> Result<Self, HistoryError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| HistoryError(e.to_string()))?;
        }
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self, HistoryError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, HistoryError> {
        // WAL keeps writes cheap and lets the UI read while the runtime writes.
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "synchronous", "NORMAL");
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn append(&self, e: &AlertEvent) -> Result<i64, HistoryError> {
        self.conn.execute(
            "INSERT INTO events (ts_ms, rule_id, rule_name, sensor, severity, kind, detail, value, is_test)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                e.ts_ms,
                e.rule_id as i64,
                e.rule_name,
                e.sensor,
                e.severity.as_str(),
                e.kind.as_str(),
                e.detail,
                e.value,
                e.is_test,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Appends many events in one transaction (a storm of events costs one disk sync).
    pub fn append_batch(&mut self, events: &[AlertEvent]) -> Result<(), HistoryError> {
        let tx = self.conn.transaction()?;
        for e in events {
            tx.execute(
                "INSERT INTO events (ts_ms, rule_id, rule_name, sensor, severity, kind, detail, value, is_test)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    e.ts_ms,
                    e.rule_id as i64,
                    e.rule_name,
                    e.sensor,
                    e.severity.as_str(),
                    e.kind.as_str(),
                    e.detail,
                    e.value,
                    e.is_test,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    fn where_clause(f: &HistoryFilter) -> (String, Vec<Box<dyn ToSql>>) {
        let mut parts: Vec<&str> = Vec::new();
        let mut args: Vec<Box<dyn ToSql>> = Vec::new();
        if let Some(s) = f.severity {
            parts.push("severity = ?");
            args.push(Box::new(s.as_str()));
        }
        if let Some(id) = f.rule_id {
            parts.push("rule_id = ?");
            args.push(Box::new(id as i64));
        }
        let kind_marks;
        if !f.kinds.is_empty() {
            kind_marks = format!("kind IN ({})", vec!["?"; f.kinds.len()].join(","));
            parts.push(&kind_marks);
            for k in &f.kinds {
                args.push(Box::new(k.as_str()));
            }
        }
        if !f.include_tests {
            parts.push("is_test = 0");
        }
        if let Some(t) = f.from_ms {
            parts.push("ts_ms >= ?");
            args.push(Box::new(t));
        }
        if let Some(t) = f.to_ms {
            parts.push("ts_ms <= ?");
            args.push(Box::new(t));
        }
        let clause = if parts.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", parts.join(" AND "))
        };
        (clause, args)
    }

    /// Newest first.
    pub fn query(&self, filter: &HistoryFilter) -> Result<Vec<StoredEvent>, HistoryError> {
        let (clause, args) = Self::where_clause(filter);
        let limit = filter
            .limit
            .map(|l| format!(" LIMIT {l}"))
            .unwrap_or_default();
        let sql = format!(
            "SELECT id, ts_ms, rule_id, rule_name, sensor, severity, kind, detail, value, is_test
             FROM events{clause} ORDER BY ts_ms DESC, id DESC{limit}"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args.iter().map(|a| a.as_ref())), |r| {
            let severity: String = r.get(5)?;
            let kind: String = r.get(6)?;
            Ok(StoredEvent {
                id: r.get(0)?,
                event: AlertEvent {
                    ts_ms: r.get(1)?,
                    rule_id: r.get::<_, i64>(2)? as u64,
                    rule_name: r.get(3)?,
                    sensor: r.get(4)?,
                    severity: Severity::parse(&severity).unwrap_or(Severity::Info),
                    kind: EventKind::parse(&kind).unwrap_or(EventKind::Note),
                    detail: r.get(7)?,
                    value: r.get(8)?,
                    is_test: r.get(9)?,
                },
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn count(&self, filter: &HistoryFilter) -> Result<u64, HistoryError> {
        let (clause, args) = Self::where_clause(filter);
        let sql = format!("SELECT COUNT(*) FROM events{clause}");
        let n: i64 = self.conn.query_row(
            &sql,
            params_from_iter(args.iter().map(|a| a.as_ref())),
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }

    /// Deletes events older than `retention_days` before `now_ms`. Returns how many went away.
    pub fn apply_retention(&self, now_ms: i64, retention_days: u32) -> Result<usize, HistoryError> {
        let cutoff = now_ms - i64::from(retention_days) * 86_400_000;
        Ok(self
            .conn
            .execute("DELETE FROM events WHERE ts_ms < ?1", [cutoff])?)
    }

    pub fn clear(&self) -> Result<usize, HistoryError> {
        Ok(self.conn.execute("DELETE FROM events", [])?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(
        ts_ms: i64,
        rule_id: u64,
        severity: Severity,
        kind: EventKind,
        is_test: bool,
    ) -> AlertEvent {
        AlertEvent {
            ts_ms,
            rule_id,
            rule_name: format!("regra {rule_id}"),
            sensor: "cpu.temp".into(),
            severity,
            kind,
            detail: "detalhe".into(),
            value: Some(90.0),
            is_test,
        }
    }

    fn seeded() -> HistoryStore {
        let mut s = HistoryStore::open_in_memory().unwrap();
        s.append_batch(&[
            event(1_000, 1, Severity::Crit, EventKind::Fired, false),
            event(2_000, 1, Severity::Crit, EventKind::Resolved, false),
            event(3_000, 2, Severity::Warn, EventKind::Fired, false),
            event(4_000, 2, Severity::Warn, EventKind::Fired, true),
            event(5_000, 3, Severity::Info, EventKind::Note, false),
        ])
        .unwrap();
        s
    }

    #[test]
    fn lists_newest_first_and_hides_tests_by_default() {
        let s = seeded();
        let all = s.query(&HistoryFilter::default()).unwrap();
        assert_eq!(
            all.iter().map(|e| e.event.ts_ms).collect::<Vec<_>>(),
            vec![5_000, 3_000, 2_000, 1_000]
        );
        let with_tests = s
            .query(&HistoryFilter {
                include_tests: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(with_tests.len(), 5);
        assert!(with_tests.iter().any(|e| e.event.is_test));
    }

    #[test]
    fn filters_by_severity_rule_kind_and_time() {
        let s = seeded();
        let f = |f: HistoryFilter| s.query(&f).unwrap().len();
        assert_eq!(
            f(HistoryFilter {
                severity: Some(Severity::Crit),
                ..Default::default()
            }),
            2
        );
        assert_eq!(
            f(HistoryFilter {
                rule_id: Some(2),
                ..Default::default()
            }),
            1
        );
        assert_eq!(
            f(HistoryFilter {
                rule_id: Some(2),
                include_tests: true,
                ..Default::default()
            }),
            2
        );
        assert_eq!(
            f(HistoryFilter {
                kinds: vec![EventKind::Resolved],
                ..Default::default()
            }),
            1
        );
        assert_eq!(
            f(HistoryFilter {
                from_ms: Some(2_000),
                to_ms: Some(3_000),
                ..Default::default()
            }),
            2
        );
        assert_eq!(
            f(HistoryFilter {
                limit: Some(1),
                ..Default::default()
            }),
            1
        );
        assert_eq!(
            f(HistoryFilter {
                kinds: vec![EventKind::Fired, EventKind::Resolved],
                ..Default::default()
            }),
            3
        );
    }

    #[test]
    fn counts_match_the_filter() {
        let s = seeded();
        assert_eq!(s.count(&HistoryFilter::default()).unwrap(), 4);
        assert_eq!(
            s.count(&HistoryFilter {
                include_tests: true,
                ..Default::default()
            })
            .unwrap(),
            5
        );
    }

    #[test]
    fn retention_removes_only_old_events() {
        let s = seeded();
        // "now" is day 10 in ms; keep 7 days: cutoff = day 3 => events at 1..5 s are all older.
        let day = 86_400_000;
        s.append(&event(10 * day, 9, Severity::Info, EventKind::Note, false))
            .unwrap();
        let removed = s.apply_retention(10 * day, 7).unwrap();
        assert_eq!(removed, 5);
        let left = s
            .query(&HistoryFilter {
                include_tests: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].event.rule_id, 9);
    }

    #[test]
    fn clear_empties_the_history() {
        let s = seeded();
        assert_eq!(s.clear().unwrap(), 5);
        assert_eq!(
            s.count(&HistoryFilter {
                include_tests: true,
                ..Default::default()
            })
            .unwrap(),
            0
        );
    }

    #[test]
    fn events_survive_reopening_the_file_and_keep_unicode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.sqlite");
        {
            let s = HistoryStore::open(&path).unwrap();
            let mut e = event(1_000, 1, Severity::Warn, EventKind::Fired, false);
            e.rule_name = "Memória quase cheia — 90% 🔥".into();
            e.detail = "x'; DROP TABLE events; --".into();
            s.append(&e).unwrap();
        }
        let s = HistoryStore::open(&path).unwrap();
        let got = s.query(&HistoryFilter::default()).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].event.rule_name, "Memória quase cheia — 90% 🔥");
        // The injection attempt is stored as plain text and the table is intact.
        assert_eq!(got[0].event.detail, "x'; DROP TABLE events; --");
    }

    #[test]
    fn a_value_of_none_round_trips() {
        let s = HistoryStore::open_in_memory().unwrap();
        let mut e = event(1_000, 1, Severity::Info, EventKind::Note, false);
        e.value = None;
        s.append(&e).unwrap();
        assert_eq!(
            s.query(&HistoryFilter::default()).unwrap()[0].event.value,
            None
        );
    }
}
