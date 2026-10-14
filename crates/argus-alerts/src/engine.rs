//! The rule engine: readings in, alert events out. Pure and deterministic: it never reads a
//! clock, every time comes from the reading's own timestamp, so tests need no sleeping and the
//! collection interval does not change what a rule means.

use crate::{AlertEvent, EventKind, Mode, Operator, Rule, Severity};
use argus_core::Availability;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq)]
enum Phase {
    Ok,
    Pending {
        since: i64,
    },
    Firing {
        fired_at: i64,
        last_notice: i64,
        peak: f64,
        acked: bool,
    },
}

#[derive(Debug, Clone)]
struct State {
    phase: Phase,
    /// When the value first went back to normal while firing (the "wait X s" timer).
    clear_since: Option<i64>,
    /// Readings inside the averaging window (one extra just before it, to cover its start).
    samples: VecDeque<(i64, f64)>,
    /// Start times of each excursion (Spikes mode).
    edges: VecDeque<i64>,
    prev_condition: Option<bool>,
    last_value: Option<f64>,
    unavailable: bool,
    first_seen: Option<i64>,
}

impl State {
    fn new() -> Self {
        Self {
            phase: Phase::Ok,
            clear_since: None,
            samples: VecDeque::new(),
            edges: VecDeque::new(),
            prev_condition: None,
            last_value: None,
            unavailable: false,
            first_seen: None,
        }
    }

    /// Forgets what was being counted. An active firing is kept, see `Engine::on_reading`.
    fn restart_counting(&mut self) {
        if matches!(self.phase, Phase::Pending { .. }) {
            self.phase = Phase::Ok;
        }
        self.samples.clear();
        self.edges.clear();
        self.prev_condition = None;
        self.first_seen = None;
        self.clear_since = None;
    }
}

/// What the UI shows for a rule right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum RulePhase {
    Disabled,
    Ok,
    /// Counting towards firing.
    Pending,
    Firing,
    /// Firing and acknowledged: no more reminders.
    Acknowledged,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RuleStatus {
    pub rule_id: u64,
    pub phase: RulePhase,
    pub last_value: Option<f64>,
    /// False while the sensor reports unavailable or no response.
    pub sensor_available: bool,
    /// How long the condition has held (Pending) or the rule has been active (Firing).
    pub elapsed_ms: i64,
    /// The duration the condition must hold (Sustained), else 0.
    pub needed_ms: i64,
    pub peak: Option<f64>,
    /// Excursions counted in the window (Spikes).
    pub occurrences: u32,
}

struct Tracked {
    rule: Rule,
    state: State,
}

pub struct Engine {
    rules: Vec<Tracked>,
    is_test: bool,
}

fn fmt_value(v: f64) -> String {
    format!("{v:.1}").replace('.', ",")
}

fn fmt_threshold(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        fmt_value(v)
    }
}

fn fmt_duration(ms: i64) -> String {
    let s = (ms / 1000).max(0);
    if s >= 3600 && s % 3600 == 0 {
        format!("{} h", s / 3600)
    } else if s >= 60 && s % 60 == 0 {
        format!("{} min", s / 60)
    } else {
        format!("{s} s")
    }
}

fn above(op: Operator) -> bool {
    op == Operator::Above
}

/// Average of a piecewise-constant signal over `[from, to]`: each reading holds until the next
/// one, so a long stretch at one value weighs more than a brief one. A plain mean of the samples
/// would change meaning with the collection interval.
pub fn time_weighted_average(samples: &VecDeque<(i64, f64)>, from: i64, to: i64) -> Option<f64> {
    if to <= from || samples.is_empty() {
        return None;
    }
    let mut total = 0.0;
    let mut covered = 0i64;
    for (i, &(ts, value)) in samples.iter().enumerate() {
        let start = ts.max(from);
        let end = samples.get(i + 1).map_or(to, |next| next.0).min(to);
        if end > start {
            total += value * (end - start) as f64;
            covered += end - start;
        }
    }
    (covered > 0).then(|| total / covered as f64)
}

impl Engine {
    pub fn new(rules: Vec<Rule>) -> Self {
        Self {
            rules: rules
                .into_iter()
                .map(|rule| Tracked {
                    rule,
                    state: State::new(),
                })
                .collect(),
            is_test: false,
        }
    }

    /// An engine whose events are all flagged as test events ("Simular").
    pub fn new_test(rules: Vec<Rule>) -> Self {
        let mut e = Self::new(rules);
        e.is_test = true;
        e
    }

    pub fn rules(&self) -> impl Iterator<Item = &Rule> {
        self.rules.iter().map(|t| &t.rule)
    }

    /// Replaces the rule set. A rule that was active and is now edited, disabled or deleted ends
    /// with an `Ended` event; rules that did not change keep their state.
    pub fn set_rules(&mut self, ts_ms: i64, new_rules: Vec<Rule>) -> Vec<AlertEvent> {
        let mut events = Vec::new();
        let mut old: Vec<Tracked> = std::mem::take(&mut self.rules);
        let mut next = Vec::with_capacity(new_rules.len());
        for rule in new_rules {
            match old.iter().position(|t| t.rule.id == rule.id) {
                Some(i) => {
                    let mut tracked = old.remove(i);
                    if tracked.rule != rule {
                        if matches!(tracked.state.phase, Phase::Firing { .. }) {
                            events.push(self.ended(&tracked, ts_ms, "regra editada"));
                        }
                        tracked.state = State::new();
                        tracked.rule = rule;
                    }
                    if !tracked.rule.enabled {
                        if matches!(tracked.state.phase, Phase::Firing { .. }) {
                            events.push(self.ended(&tracked, ts_ms, "regra desativada"));
                        }
                        tracked.state = State::new();
                    }
                    next.push(tracked);
                }
                None => next.push(Tracked {
                    rule,
                    state: State::new(),
                }),
            }
        }
        for gone in old {
            if matches!(gone.state.phase, Phase::Firing { .. }) {
                events.push(self.ended(&gone, ts_ms, "regra excluída"));
            }
        }
        self.rules = next;
        events
    }

    fn ended(&self, t: &Tracked, ts_ms: i64, why: &str) -> AlertEvent {
        let (fired_at, peak) = match t.state.phase {
            Phase::Firing { fired_at, peak, .. } => (fired_at, peak),
            _ => (ts_ms, 0.0),
        };
        AlertEvent {
            ts_ms,
            rule_id: t.rule.id,
            rule_name: t.rule.name.clone(),
            sensor: t.rule.sensor.clone(),
            severity: t.rule.severity,
            kind: EventKind::Ended,
            detail: format!(
                "encerrada ({why}) · ativa por {} · pico {}",
                fmt_duration(ts_ms - fired_at),
                fmt_value(peak)
            ),
            value: Some(peak),
            is_test: self.is_test,
        }
    }

    /// Stops reminders for an active rule. Returns false if it was not firing.
    pub fn acknowledge(&mut self, rule_id: u64) -> bool {
        for t in &mut self.rules {
            if t.rule.id == rule_id {
                if let Phase::Firing { acked, .. } = &mut t.state.phase {
                    *acked = true;
                    return true;
                }
            }
        }
        false
    }

    pub fn status(&self, rule_id: u64, now_ms: i64) -> Option<RuleStatus> {
        let t = self.rules.iter().find(|t| t.rule.id == rule_id)?;
        let s = &t.state;
        let (phase, elapsed_ms, peak) = if !t.rule.enabled {
            (RulePhase::Disabled, 0, None)
        } else {
            match s.phase {
                Phase::Ok => (RulePhase::Ok, 0, None),
                Phase::Pending { since } => (RulePhase::Pending, now_ms - since, None),
                Phase::Firing {
                    fired_at,
                    peak,
                    acked,
                    ..
                } => (
                    if acked {
                        RulePhase::Acknowledged
                    } else {
                        RulePhase::Firing
                    },
                    now_ms - fired_at,
                    Some(peak),
                ),
            }
        };
        Some(RuleStatus {
            rule_id,
            phase,
            last_value: s.last_value,
            sensor_available: !s.unavailable,
            elapsed_ms,
            needed_ms: if t.rule.mode == Mode::Sustained {
                i64::from(t.rule.for_sec) * 1000
            } else {
                0
            },
            peak,
            occurrences: s.edges.len() as u32,
        })
    }

    /// Feeds one reading of one sensor to every rule that watches it.
    pub fn on_reading(
        &mut self,
        ts_ms: i64,
        sensor: &str,
        value: &Availability,
    ) -> Vec<AlertEvent> {
        let mut out = Vec::new();
        let is_test = self.is_test;
        for t in &mut self.rules {
            if t.rule.sensor != sensor || !t.rule.enabled {
                continue;
            }
            step(&t.rule, &mut t.state, ts_ms, value, is_test, &mut out);
        }
        out
    }
}

fn event(
    rule: &Rule,
    ts_ms: i64,
    kind: EventKind,
    severity: Severity,
    detail: String,
    value: Option<f64>,
    is_test: bool,
) -> AlertEvent {
    AlertEvent {
        ts_ms,
        rule_id: rule.id,
        rule_name: rule.name.clone(),
        sensor: rule.sensor.clone(),
        severity,
        kind,
        detail,
        value,
        is_test,
    }
}

fn describe(rule: &Rule, value: f64, occurrences: usize) -> String {
    let dir = if above(rule.op) { "acima" } else { "abaixo" };
    let window = fmt_duration(i64::from(rule.for_sec) * 1000);
    match rule.mode {
        Mode::Sustained if rule.for_sec == 0 => format!(
            "valor {} {dir} de {}",
            fmt_value(value),
            fmt_threshold(rule.threshold)
        ),
        Mode::Sustained => format!(
            "valor {} {dir} de {} por {window} seguidos",
            fmt_value(value),
            fmt_threshold(rule.threshold)
        ),
        Mode::Average => format!(
            "média {} {dir} de {} nos últimos {window}",
            fmt_value(value),
            fmt_threshold(rule.threshold)
        ),
        Mode::Spikes => format!(
            "{occurrences} ocorrências {dir} de {} em {window}",
            fmt_threshold(rule.threshold)
        ),
    }
}

fn step(
    rule: &Rule,
    st: &mut State,
    ts: i64,
    reading: &Availability,
    is_test: bool,
    out: &mut Vec<AlertEvent>,
) {
    let value = match reading {
        Availability::Ok { value } => *value,
        _ => {
            // An unreadable sensor is neither "normal" nor "a violation": counting restarts, an
            // active firing stays active, and the history gets one note about it.
            if !st.unavailable {
                st.unavailable = true;
                let active = !matches!(st.phase, Phase::Ok)
                    || !st.edges.is_empty()
                    || !st.samples.is_empty();
                if active {
                    let detail = if matches!(st.phase, Phase::Firing { .. }) {
                        "o sensor parou de responder; o alerta continua ativo até voltar a ler"
                    } else {
                        "o sensor parou de responder; a contagem recomeça quando voltar"
                    };
                    out.push(event(
                        rule,
                        ts,
                        EventKind::Note,
                        Severity::Info,
                        detail.to_string(),
                        None,
                        is_test,
                    ));
                }
            }
            st.restart_counting();
            return;
        }
    };
    st.unavailable = false;
    st.last_value = Some(value);
    st.first_seen.get_or_insert(ts);

    let above_op = above(rule.op);
    let cmp = |x: f64| {
        if above_op {
            x > rule.threshold
        } else {
            x < rule.threshold
        }
    };
    // While active the rule only lets go once the value crosses the resolve level, if one is set.
    let beyond_resolve = |x: f64| match rule.hysteresis {
        None => cmp(x),
        Some(h) => {
            if above_op {
                x > h
            } else {
                x < h
            }
        }
    };
    let firing = matches!(st.phase, Phase::Firing { .. });
    let window_ms = i64::from(rule.for_sec) * 1000;

    let mut shown = value;
    let mut occurrences = 0usize;
    let hit = match rule.mode {
        Mode::Sustained => {
            if firing {
                beyond_resolve(value)
            } else {
                cmp(value)
            }
        }
        Mode::Average => {
            st.samples.push_back((ts, value));
            // Keep one reading at or before the window start so the window's beginning is covered.
            while st.samples.len() >= 2 && st.samples[1].0 <= ts - window_ms {
                st.samples.pop_front();
            }
            let covered = st.first_seen.is_some_and(|f| f <= ts - window_ms);
            match (
                covered,
                time_weighted_average(&st.samples, ts - window_ms, ts),
            ) {
                (true, Some(avg)) => {
                    shown = avg;
                    if firing {
                        beyond_resolve(avg)
                    } else {
                        cmp(avg)
                    }
                }
                _ => false,
            }
        }
        Mode::Spikes => {
            let now = cmp(value);
            if now && !st.prev_condition.unwrap_or(false) {
                st.edges.push_back(ts);
            }
            st.prev_condition = Some(now);
            while st.edges.front().is_some_and(|&e| ts - e >= window_ms) {
                st.edges.pop_front();
            }
            occurrences = st.edges.len();
            occurrences >= rule.count as usize
        }
    };
    let need_ms = if rule.mode == Mode::Sustained {
        window_ms
    } else {
        0
    };

    if hit {
        st.clear_since = None;
        if st.phase == Phase::Ok {
            st.phase = Phase::Pending { since: ts };
        }
        if let Phase::Pending { since } = st.phase {
            if ts - since >= need_ms {
                st.phase = Phase::Firing {
                    fired_at: ts,
                    last_notice: ts,
                    peak: shown,
                    acked: false,
                };
                out.push(event(
                    rule,
                    ts,
                    EventKind::Fired,
                    rule.severity,
                    describe(rule, shown, occurrences),
                    Some(shown),
                    is_test,
                ));
            }
        } else if let Phase::Firing {
            last_notice,
            peak,
            acked,
            ..
        } = &mut st.phase
        {
            *peak = if above_op {
                peak.max(shown)
            } else {
                peak.min(shown)
            };
            if rule.repeat_sec > 0
                && !*acked
                && ts - *last_notice >= i64::from(rule.repeat_sec) * 1000
            {
                *last_notice = ts;
                out.push(event(
                    rule,
                    ts,
                    EventKind::Repeat,
                    rule.severity,
                    describe(rule, shown, occurrences),
                    Some(shown),
                    is_test,
                ));
            }
        }
    } else if matches!(st.phase, Phase::Pending { .. }) {
        st.phase = Phase::Ok;
    } else if let Phase::Firing { fired_at, peak, .. } = st.phase {
        let since = *st.clear_since.get_or_insert(ts);
        if ts - since >= i64::from(rule.recover_sec) * 1000 {
            out.push(event(
                rule,
                ts,
                EventKind::Resolved,
                rule.severity,
                format!(
                    "voltou ao normal · ativa por {} · pico {}",
                    fmt_duration(ts - fired_at),
                    fmt_value(peak)
                ),
                Some(shown),
                is_test,
            ));
            st.phase = Phase::Ok;
            st.clear_since = None;
        }
    }
}

/// Whether an action (sound, notification) should go out right now. Muting silences everything
/// except critical rules when the user keeps "criticals ignore mute" on.
pub fn should_notify(severity: Severity, muted: bool, critical_bypasses_mute: bool) -> bool {
    !muted || (severity == Severity::Crit && critical_bypasses_mute)
}

#[cfg(test)]
mod tests;
