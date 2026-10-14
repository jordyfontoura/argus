//! "Simular": runs one rule against a made-up series on a private engine and a virtual clock.
//! It never sees real readings and its engine is separate, so a simulation cannot change a real
//! rule's state, a real series or the real charts. Every event it makes is flagged `is_test`.

use crate::{AlertEvent, Engine, Mode, Operator, Rule};
use argus_core::Availability;
use serde::{Deserialize, Serialize};

const STEP_MS: i64 = 1000;

/// What the simulation produced: the synthetic series (for the rule's own mini chart) and the
/// events the rule generated on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SimResult {
    /// `(timestamp ms, value)` once per second, ending at `end_ms`.
    pub points: Vec<(i64, f64)>,
    pub events: Vec<AlertEvent>,
}

/// A value clearly on the "bad" side of the threshold, and one clearly on the "normal" side
/// (beyond the resolve level when the rule has one).
fn levels(rule: &Rule) -> (f64, f64) {
    let margin = (rule.threshold.abs() * 0.1).max(5.0);
    match rule.op {
        Operator::Above => {
            let normal = rule
                .hysteresis
                .map_or(rule.threshold, |h| h.min(rule.threshold))
                - margin * 2.0;
            (rule.threshold + margin, normal)
        }
        Operator::Below => {
            let normal = rule
                .hysteresis
                .map_or(rule.threshold, |h| h.max(rule.threshold))
                + margin * 2.0;
            (rule.threshold - margin, normal)
        }
    }
}

/// The synthetic signal as `(offset seconds, value)`: calm, a violation shaped for the rule's
/// mode, then calm again long enough for the rule to resolve.
fn scenario(rule: &Rule) -> Vec<f64> {
    let (bad, good) = levels(rule);
    let window = rule.for_sec.max(1) as usize;
    let mut series = Vec::new();
    // Calm lead-in: averages need the whole window of history first.
    let lead = if rule.mode == Mode::Average {
        window + 3
    } else {
        3
    };
    series.extend(std::iter::repeat_n(good, lead));
    match rule.mode {
        Mode::Sustained => series.extend(std::iter::repeat_n(bad, rule.for_sec as usize + 3)),
        Mode::Average => series.extend(std::iter::repeat_n(bad, window + 3)),
        Mode::Spikes => {
            let count = rule.count.max(2) as usize;
            // Short excursions spaced so that `count` of them fit inside the window.
            let period = (window.saturating_sub(1) / count).max(2);
            for _ in 0..count {
                series.push(bad);
                series.extend(std::iter::repeat_n(good, period - 1));
            }
            series.push(bad);
        }
    }
    // Averages and counts look back over a whole window, so calm must last that long to resolve.
    let settle = if rule.mode == Mode::Sustained {
        0
    } else {
        window
    };
    series.extend(std::iter::repeat_n(
        good,
        settle + rule.recover_sec as usize + 5,
    ));
    series
}

/// Runs the simulation so that its last point is at `end_ms` (events therefore land just before
/// "now" in the history). Pure: nothing outside the returned value changes.
pub fn simulate(rule: &Rule, end_ms: i64) -> SimResult {
    let mut probe = rule.clone();
    probe.enabled = true;
    // Reminders would only add noise to a demonstration.
    probe.repeat_sec = 0;
    let values = scenario(&probe);
    let start_ms = end_ms - (values.len() as i64 - 1) * STEP_MS;
    let mut engine = Engine::new_test(vec![probe.clone()]);
    let mut points = Vec::with_capacity(values.len());
    let mut events = Vec::new();
    for (i, v) in values.iter().enumerate() {
        let ts = start_ms + i as i64 * STEP_MS;
        points.push((ts, *v));
        events.extend(engine.on_reading(ts, &probe.sensor, &Availability::ok(*v)));
    }
    SimResult { points, events }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Actions, EventKind, Severity};

    fn rule(mode: Mode) -> Rule {
        Rule {
            id: 7,
            name: "teste".into(),
            sensor: "cpu.temp".into(),
            mode,
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

    #[test]
    fn every_mode_fires_then_resolves_and_every_event_is_a_test() {
        for mode in [Mode::Sustained, Mode::Average, Mode::Spikes] {
            let r = simulate(&rule(mode), 1_000_000);
            let kinds: Vec<EventKind> = r.events.iter().map(|e| e.kind).collect();
            assert_eq!(
                kinds,
                vec![EventKind::Fired, EventKind::Resolved],
                "{mode:?}"
            );
            assert!(r.events.iter().all(|e| e.is_test), "{mode:?}");
            assert!(r.events.iter().all(|e| e.rule_id == 7));
        }
    }

    #[test]
    fn the_timeline_ends_at_the_requested_moment() {
        let r = simulate(&rule(Mode::Sustained), 1_000_000);
        assert_eq!(r.points.last().unwrap().0, 1_000_000);
        assert!(r.events.iter().all(|e| e.ts_ms <= 1_000_000));
        let steps: Vec<i64> = r.points.windows(2).map(|w| w[1].0 - w[0].0).collect();
        assert!(steps.iter().all(|s| *s == 1000));
    }

    #[test]
    fn below_rules_and_rules_without_hysteresis_simulate_too() {
        let mut r = rule(Mode::Sustained);
        r.op = Operator::Below;
        r.threshold = 300.0;
        r.hysteresis = Some(500.0);
        let sim = simulate(&r, 5_000_000);
        assert_eq!(sim.events.len(), 2);
        let mut r = rule(Mode::Sustained);
        r.hysteresis = None;
        assert_eq!(simulate(&r, 5_000_000).events.len(), 2);
    }

    #[test]
    fn a_disabled_rule_can_still_be_simulated_and_the_input_is_untouched() {
        let mut r = rule(Mode::Sustained);
        r.enabled = false;
        let before = r.clone();
        assert_eq!(simulate(&r, 1_000_000).events.len(), 2);
        assert_eq!(r, before, "the caller's rule is not modified");
    }
}
