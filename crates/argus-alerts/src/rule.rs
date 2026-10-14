use crate::Severity;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// The condition stays true for `for_sec` seconds in a row.
    Sustained,
    /// The time-weighted average over the last `for_sec` seconds meets the condition.
    Average,
    /// The condition starts `count` times within `for_sec` seconds.
    Spikes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    Above,
    Below,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Actions {
    pub toast: bool,
    pub sound: bool,
    pub notify: bool,
}

/// A user-defined alert rule. One metric per rule (a sensor id from the catalog).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Rule {
    pub id: u64,
    pub name: String,
    pub sensor: String,
    pub mode: Mode,
    pub op: Operator,
    pub threshold: f64,
    /// Value the reading must cross back over before the rule resolves (optional).
    pub hysteresis: Option<f64>,
    /// Duration (Sustained), window (Average) or window (Spikes), in seconds.
    pub for_sec: u32,
    /// Occurrences needed (Spikes only).
    pub count: u32,
    /// Seconds the value must stay normal before the rule resolves.
    pub recover_sec: u32,
    /// Repeat the warning every N seconds while active and unacknowledged; 0 = once.
    pub repeat_sec: u32,
    pub severity: Severity,
    pub actions: Actions,
    pub enabled: bool,
    /// Set when the rule came from a protection preset (its key); user-made rules have none.
    #[serde(default)]
    pub preset: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleError(pub String);

impl fmt::Display for RuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RuleError {}

const MAX_NAME_CHARS: usize = 80;
const MAX_WINDOW_SEC: u32 = 24 * 3600;

impl Rule {
    /// Rejects rules the engine cannot evaluate sensibly. Messages are shown to the user.
    pub fn validate(&self) -> Result<(), RuleError> {
        let err = |m: &str| Err(RuleError(m.to_string()));
        if self.name.trim().is_empty() {
            return err("dê um nome à regra");
        }
        if self.name.chars().count() > MAX_NAME_CHARS {
            return err("o nome da regra é longo demais (máximo 80 caracteres)");
        }
        if self.sensor.trim().is_empty() {
            return err("escolha a métrica da regra");
        }
        if !self.threshold.is_finite() {
            return err("o limite precisa ser um número");
        }
        if let Some(h) = self.hysteresis {
            if !h.is_finite() {
                return err("o valor de resolução precisa ser um número");
            }
            // Resolving must be on the "normal" side of the threshold, otherwise it would
            // resolve while the condition still holds.
            let wrong_side = match self.op {
                Operator::Above => h > self.threshold,
                Operator::Below => h < self.threshold,
            };
            if wrong_side {
                return err("o valor de resolução deve ficar do lado normal do limite");
            }
        }
        if self.for_sec > MAX_WINDOW_SEC
            || self.recover_sec > MAX_WINDOW_SEC
            || self.repeat_sec > MAX_WINDOW_SEC
        {
            return err("os tempos da regra podem ter no máximo 24 horas");
        }
        if self.mode != Mode::Sustained && self.for_sec == 0 {
            return err("informe o período da regra");
        }
        if self.mode == Mode::Spikes && self.count < 2 {
            return err("informe ao menos 2 ocorrências");
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn sample() -> Rule {
    Rule {
        id: 1,
        name: "CPU superaquecendo".into(),
        sensor: "cpu.temp".into(),
        mode: Mode::Sustained,
        op: Operator::Above,
        threshold: 85.0,
        hysteresis: Some(80.0),
        for_sec: 10,
        count: 3,
        recover_sec: 5,
        repeat_sec: 60,
        severity: Severity::Crit,
        actions: Actions {
            toast: true,
            sound: true,
            notify: true,
        },
        enabled: true,
        preset: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reasonable_rule_is_valid() {
        assert_eq!(sample().validate(), Ok(()));
    }

    #[test]
    fn rejects_bad_rules_with_a_message() {
        let mut r = sample();
        r.name = "  ".into();
        assert!(r.validate().is_err());
        let mut r = sample();
        r.name = "x".repeat(81);
        assert!(r.validate().is_err());
        let mut r = sample();
        r.threshold = f64::NAN;
        assert!(r.validate().is_err());
        let mut r = sample();
        r.hysteresis = Some(90.0); // above the threshold for an "above" rule
        assert!(r.validate().is_err());
        let mut r = sample();
        r.op = Operator::Below;
        r.hysteresis = Some(80.0); // below the threshold for a "below" rule
        assert!(r.validate().is_err());
        let mut r = sample();
        r.mode = Mode::Spikes;
        r.count = 1;
        assert!(r.validate().is_err());
        let mut r = sample();
        r.mode = Mode::Average;
        r.for_sec = 0;
        assert!(r.validate().is_err());
    }

    #[test]
    fn serializes_to_stable_json() {
        let json = serde_json::to_value(sample()).unwrap();
        assert_eq!(json["mode"], "sustained");
        assert_eq!(json["op"], "above");
        assert_eq!(json["severity"], "crit");
    }
}
