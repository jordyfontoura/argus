use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warn,
    Crit,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Crit => "crit",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "info" => Some(Self::Info),
            "warn" => Some(Self::Warn),
            "crit" => Some(Self::Crit),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// The rule's condition held for long enough.
    Fired,
    /// A reminder while the rule stays active and unacknowledged.
    Repeat,
    /// The value came back to normal.
    Resolved,
    /// The rule was edited, disabled or deleted while active.
    Ended,
    /// A note, for example that the sensor stopped answering.
    Note,
}

impl EventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fired => "fired",
            Self::Repeat => "repeat",
            Self::Resolved => "resolved",
            Self::Ended => "ended",
            Self::Note => "note",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "fired" => Some(Self::Fired),
            "repeat" => Some(Self::Repeat),
            "resolved" => Some(Self::Resolved),
            "ended" => Some(Self::Ended),
            "note" => Some(Self::Note),
            _ => None,
        }
    }
}

/// One entry of the alert history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AlertEvent {
    pub ts_ms: i64,
    pub rule_id: u64,
    /// The rule's name when the event happened (it may be renamed later).
    pub rule_name: String,
    pub sensor: String,
    pub severity: Severity,
    pub kind: EventKind,
    pub detail: String,
    pub value: Option<f64>,
    /// True for events produced by "Simular"; they never mix with real ones unless asked.
    pub is_test: bool,
}
