use super::*;
use crate::{Actions, Rule};
use std::collections::VecDeque;

const S: i64 = 1000;

fn rule(mode: Mode) -> Rule {
    Rule {
        id: 1,
        name: "regra".into(),
        sensor: "t".into(),
        mode,
        op: Operator::Above,
        threshold: 85.0,
        hysteresis: None,
        for_sec: 10,
        count: 3,
        recover_sec: 5,
        repeat_sec: 0,
        severity: Severity::Crit,
        actions: Actions {
            toast: true,
            sound: false,
            notify: false,
        },
        enabled: true,
        preset: None,
    }
}

fn ok(v: f64) -> Availability {
    Availability::ok(v)
}

fn gone() -> Availability {
    Availability::unavailable("sem leitura")
}

/// Feeds `signal(second)` once per `step` seconds for `secs` seconds and returns all events.
fn run(
    engine: &mut Engine,
    start: i64,
    secs: i64,
    step: i64,
    signal: impl Fn(i64) -> f64,
) -> Vec<AlertEvent> {
    let mut events = Vec::new();
    let mut t = 0;
    while t <= secs {
        events.extend(engine.on_reading(start + t * S, "t", &ok(signal(t))));
        t += step;
    }
    events
}

fn kinds(events: &[AlertEvent]) -> Vec<EventKind> {
    events.iter().map(|e| e.kind).collect()
}

// ---- sustained ----

#[test]
fn sustained_fires_only_after_the_full_duration() {
    let mut e = Engine::new(vec![rule(Mode::Sustained)]);
    let events = run(&mut e, 0, 30, 1, |t| if t >= 5 { 90.0 } else { 50.0 });
    assert_eq!(kinds(&events), vec![EventKind::Fired]);
    assert_eq!(events[0].ts_ms, 15 * S, "above since 5 s, fires 10 s later");
    assert_eq!(events[0].severity, Severity::Crit);
    assert_eq!(events[0].value, Some(90.0));
}

#[test]
fn an_interruption_restarts_the_count() {
    let mut e = Engine::new(vec![rule(Mode::Sustained)]);
    // Above for 8 s, one normal reading at 8 s, above again from 9 s: the count restarts at 9 s,
    // so it fires at 19 s, not at 10 s.
    let events = run(&mut e, 0, 25, 1, |t| if t == 8 { 50.0 } else { 90.0 });
    assert_eq!(kinds(&events), vec![EventKind::Fired]);
    assert_eq!(events[0].ts_ms, 19 * S);
}

#[test]
fn a_zero_duration_rule_fires_immediately() {
    let mut r = rule(Mode::Sustained);
    r.for_sec = 0;
    let mut e = Engine::new(vec![r]);
    let ev = e.on_reading(5 * S, "t", &ok(90.0));
    assert_eq!(kinds(&ev), vec![EventKind::Fired]);
}

#[test]
fn below_operator_works() {
    let mut r = rule(Mode::Sustained);
    r.op = Operator::Below;
    r.threshold = 300.0;
    r.for_sec = 3;
    let mut e = Engine::new(vec![r]);
    let events = run(&mut e, 0, 10, 1, |t| if t >= 2 { 100.0 } else { 900.0 });
    assert_eq!(kinds(&events), vec![EventKind::Fired]);
    assert_eq!(events[0].ts_ms, 5 * S);
}

#[test]
fn nothing_fires_while_below_the_limit() {
    let mut e = Engine::new(vec![rule(Mode::Sustained)]);
    assert!(
        run(&mut e, 0, 60, 1, |_| 85.0).is_empty(),
        "equal to the limit is not above it"
    );
}

// ---- hysteresis and recovery ----

#[test]
fn hysteresis_keeps_the_rule_active_until_the_resolve_level() {
    let mut r = rule(Mode::Sustained);
    r.hysteresis = Some(80.0);
    r.recover_sec = 0;
    let mut e = Engine::new(vec![r]);
    // 90 for 12 s fires; dips to 83 (below 85 but above 80) must not resolve; 79 resolves.
    let mut events = run(&mut e, 0, 12, 1, |_| 90.0);
    events.extend(e.on_reading(13 * S, "t", &ok(83.0)));
    events.extend(e.on_reading(14 * S, "t", &ok(83.0)));
    assert_eq!(kinds(&events), vec![EventKind::Fired]);
    events.extend(e.on_reading(15 * S, "t", &ok(79.0)));
    assert_eq!(kinds(&events), vec![EventKind::Fired, EventKind::Resolved]);
}

#[test]
fn resolve_waits_the_configured_time_in_the_normal_range() {
    let mut e = Engine::new(vec![rule(Mode::Sustained)]);
    let mut events = run(&mut e, 0, 10, 1, |_| 90.0);
    assert_eq!(kinds(&events), vec![EventKind::Fired]);
    // Normal for 3 s, then high again: no resolve.
    for t in 11..=13 {
        events.extend(e.on_reading(t * S, "t", &ok(50.0)));
    }
    events.extend(e.on_reading(14 * S, "t", &ok(90.0)));
    assert_eq!(kinds(&events), vec![EventKind::Fired]);
    // Normal from 15 s on: resolves once it has stayed normal for 5 s, at 20 s.
    for t in 15..=20 {
        events.extend(e.on_reading(t * S, "t", &ok(50.0)));
    }
    assert_eq!(kinds(&events), vec![EventKind::Fired, EventKind::Resolved]);
    let resolved = events.last().unwrap();
    assert_eq!(resolved.ts_ms, 20 * S);
    assert!(resolved.detail.contains("pico 90,0"), "{}", resolved.detail);
}

#[test]
fn after_resolving_the_rule_can_fire_again() {
    let mut r = rule(Mode::Sustained);
    r.recover_sec = 0;
    let mut e = Engine::new(vec![r]);
    let mut events = run(&mut e, 0, 10, 1, |_| 90.0);
    events.extend(run(&mut e, 11 * S, 1, 1, |_| 50.0));
    events.extend(run(&mut e, 100 * S, 10, 1, |_| 90.0));
    assert_eq!(
        kinds(&events),
        vec![EventKind::Fired, EventKind::Resolved, EventKind::Fired]
    );
}

// ---- repeat and acknowledge ----

#[test]
fn reminders_repeat_until_acknowledged() {
    let mut r = rule(Mode::Sustained);
    r.for_sec = 0;
    r.repeat_sec = 60;
    let mut e = Engine::new(vec![r]);
    let events = run(&mut e, 0, 130, 1, |_| 90.0);
    assert_eq!(
        kinds(&events),
        vec![EventKind::Fired, EventKind::Repeat, EventKind::Repeat]
    );
    assert_eq!(events[1].ts_ms, 60 * S);
    assert_eq!(events[2].ts_ms, 120 * S);

    let mut e = Engine::new(vec![{
        let mut r = rule(Mode::Sustained);
        r.for_sec = 0;
        r.repeat_sec = 60;
        r
    }]);
    let mut events = run(&mut e, 0, 10, 1, |_| 90.0);
    assert!(e.acknowledge(1));
    events.extend(run(&mut e, 11 * S, 200, 1, |_| 90.0));
    assert_eq!(
        kinds(&events),
        vec![EventKind::Fired],
        "no reminders after acknowledging"
    );
    assert_eq!(e.status(1, 300 * S).unwrap().phase, RulePhase::Acknowledged);
}

#[test]
fn a_repeat_value_of_zero_warns_once() {
    let mut r = rule(Mode::Sustained);
    r.for_sec = 0;
    r.repeat_sec = 0;
    let mut e = Engine::new(vec![r]);
    assert_eq!(
        kinds(&run(&mut e, 0, 500, 1, |_| 90.0)),
        vec![EventKind::Fired]
    );
}

#[test]
fn acknowledging_a_rule_that_is_not_firing_does_nothing() {
    let mut e = Engine::new(vec![rule(Mode::Sustained)]);
    assert!(!e.acknowledge(1));
    assert!(!e.acknowledge(99));
}

// ---- average over a window ----

#[test]
fn time_weighted_average_weighs_by_duration_not_by_count() {
    // 100 for 1 s, then 0 for 9 s: the mean over 10 s is 10, whatever the sample count.
    let samples: VecDeque<(i64, f64)> = [(0, 100.0), (S, 0.0)].into_iter().collect();
    let avg = time_weighted_average(&samples, 0, 10 * S).unwrap();
    assert!((avg - 10.0).abs() < 1e-9, "{avg}");
    // A plain mean of the two samples would say 50.
    let dense: VecDeque<(i64, f64)> = (0..10)
        .map(|i| (i * S, if i == 0 { 100.0 } else { 0.0 }))
        .collect();
    assert!((time_weighted_average(&dense, 0, 10 * S).unwrap() - 10.0).abs() < 1e-9);
}

#[test]
fn average_mode_needs_a_full_window_before_firing() {
    let mut r = rule(Mode::Average);
    r.for_sec = 10;
    r.threshold = 70.0;
    let mut e = Engine::new(vec![r]);
    let events = run(&mut e, 0, 30, 1, |_| 90.0);
    assert_eq!(kinds(&events), vec![EventKind::Fired]);
    assert_eq!(
        events[0].ts_ms,
        10 * S,
        "first moment the whole 10 s window has been seen"
    );
    assert!(events[0].detail.contains("média"));
}

#[test]
fn average_ignores_a_short_spike() {
    let mut r = rule(Mode::Average);
    r.for_sec = 10;
    r.threshold = 70.0;
    let mut e = Engine::new(vec![r]);
    // One-second spike to 100 in an otherwise 20 signal: the 10 s average stays near 28.
    assert!(run(&mut e, 0, 60, 1, |t| if t == 30 { 100.0 } else { 20.0 }).is_empty());
}

#[test]
fn collection_interval_does_not_change_an_average_rule() {
    let signal = |t: i64| if (20..40).contains(&t) { 90.0 } else { 20.0 };
    let fire_time = |step: i64| {
        let mut r = rule(Mode::Average);
        r.for_sec = 10;
        r.threshold = 60.0;
        let mut e = Engine::new(vec![r]);
        let events = run(&mut e, 0, 80, step, signal);
        events
            .iter()
            .find(|ev| ev.kind == EventKind::Fired)
            .map(|ev| ev.ts_ms)
    };
    let (a, b) = (fire_time(1).unwrap(), fire_time(5).unwrap());
    assert!((a - b).abs() <= 5 * S, "1 s fired at {a}, 5 s fired at {b}");
}

#[test]
fn collection_interval_does_not_change_a_sustained_rule() {
    let signal = |t: i64| if t >= 20 { 90.0 } else { 20.0 };
    let fire_time = |step: i64| {
        let mut e = Engine::new(vec![rule(Mode::Sustained)]);
        run(&mut e, 0, 80, step, signal)
            .iter()
            .find(|ev| ev.kind == EventKind::Fired)
            .map(|ev| ev.ts_ms)
    };
    let (a, b) = (fire_time(1).unwrap(), fire_time(5).unwrap());
    assert!((a - b).abs() <= 5 * S, "1 s fired at {a}, 5 s fired at {b}");
}

// ---- N occurrences in a window ----

#[test]
fn spikes_fire_on_the_nth_excursion_inside_the_window() {
    let mut r = rule(Mode::Spikes);
    r.for_sec = 60;
    r.count = 3;
    let mut e = Engine::new(vec![r]);
    // Excursions starting at 10, 20 and 30 s (each lasts 2 s).
    let signal = |t: i64| {
        if [10, 11, 20, 21, 30, 31].contains(&t) {
            95.0
        } else {
            40.0
        }
    };
    let events = run(&mut e, 0, 50, 1, signal);
    assert_eq!(kinds(&events), vec![EventKind::Fired]);
    assert_eq!(events[0].ts_ms, 30 * S);
    assert!(
        events[0].detail.contains("3 ocorrências"),
        "{}",
        events[0].detail
    );
}

#[test]
fn spikes_outside_the_window_do_not_add_up() {
    let mut r = rule(Mode::Spikes);
    r.for_sec = 30;
    r.count = 3;
    let mut e = Engine::new(vec![r]);
    // Excursions at 0, 20 and 45 s: never 3 within 30 s.
    let signal = |t: i64| if [0, 20, 45].contains(&t) { 95.0 } else { 40.0 };
    assert!(run(&mut e, 0, 100, 1, signal).is_empty());
}

#[test]
fn a_long_excursion_counts_once() {
    let mut r = rule(Mode::Spikes);
    r.for_sec = 60;
    r.count = 2;
    let mut e = Engine::new(vec![r]);
    assert!(
        run(&mut e, 0, 50, 1, |t| if t >= 10 { 95.0 } else { 40.0 }).is_empty(),
        "one long excursion is one occurrence"
    );
}

// ---- unreadable sensors ----

#[test]
fn a_pending_count_restarts_when_the_sensor_goes_away_and_a_note_is_logged() {
    let mut e = Engine::new(vec![rule(Mode::Sustained)]);
    let mut events = run(&mut e, 0, 8, 1, |_| 90.0);
    events.extend(e.on_reading(9 * S, "t", &gone()));
    events.extend(e.on_reading(10 * S, "t", &gone()));
    assert_eq!(
        kinds(&events),
        vec![EventKind::Note],
        "one note, not one per reading"
    );
    assert_eq!(events[0].severity, Severity::Info);
    // Back at 11 s: counting starts from scratch, so it fires 10 s later (not at 11 s).
    events.extend(run(&mut e, 11 * S, 12, 1, |_| 90.0));
    let fired = events
        .iter()
        .find(|ev| ev.kind == EventKind::Fired)
        .unwrap();
    assert_eq!(fired.ts_ms, 21 * S);
}

#[test]
fn an_active_rule_stays_active_when_the_sensor_goes_away() {
    let mut e = Engine::new(vec![rule(Mode::Sustained)]);
    let mut events = run(&mut e, 0, 10, 1, |_| 90.0);
    // 60 s without a reading must NOT count as "normal" and resolve the rule.
    events.extend(e.on_reading(20 * S, "t", &gone()));
    events.extend(e.on_reading(80 * S, "t", &gone()));
    assert_eq!(kinds(&events), vec![EventKind::Fired, EventKind::Note]);
    assert_eq!(e.status(1, 80 * S).unwrap().phase, RulePhase::Firing);
    assert!(!e.status(1, 80 * S).unwrap().sensor_available);
    // Reading again, normal for the recovery time: resolves.
    for t in 81..=86 {
        events.extend(e.on_reading(t * S, "t", &ok(50.0)));
    }
    assert_eq!(events.last().unwrap().kind, EventKind::Resolved);
}

#[test]
fn a_no_response_reading_is_handled_like_unavailable() {
    let mut e = Engine::new(vec![rule(Mode::Sustained)]);
    run(&mut e, 0, 5, 1, |_| 90.0);
    let ev = e.on_reading(6 * S, "t", &Availability::NoResponse { since_ms: 5 * S });
    assert_eq!(kinds(&ev), vec![EventKind::Note]);
}

#[test]
fn an_idle_rule_makes_no_note_when_its_sensor_is_unavailable() {
    let mut e = Engine::new(vec![rule(Mode::Sustained)]);
    assert!(e.on_reading(0, "t", &gone()).is_empty());
}

// ---- editing the rule set ----

#[test]
fn editing_disabling_or_deleting_an_active_rule_ends_it() {
    let mk = || {
        let mut r = rule(Mode::Sustained);
        r.for_sec = 0;
        let mut e = Engine::new(vec![r]);
        e.on_reading(0, "t", &ok(90.0));
        e
    };

    let mut e = mk();
    let mut edited = rule(Mode::Sustained);
    edited.for_sec = 0;
    edited.threshold = 95.0;
    let ev = e.set_rules(10 * S, vec![edited]);
    assert_eq!(kinds(&ev), vec![EventKind::Ended]);
    assert!(ev[0].detail.contains("editada"));
    assert_eq!(e.status(1, 10 * S).unwrap().phase, RulePhase::Ok);

    let mut e = mk();
    let mut off = rule(Mode::Sustained);
    off.for_sec = 0;
    off.enabled = false;
    let ev = e.set_rules(10 * S, vec![off]);
    assert_eq!(kinds(&ev), vec![EventKind::Ended]);
    assert_eq!(e.status(1, 10 * S).unwrap().phase, RulePhase::Disabled);

    let mut e = mk();
    let ev = e.set_rules(10 * S, vec![]);
    assert_eq!(kinds(&ev), vec![EventKind::Ended]);
    assert!(ev[0].detail.contains("excluída"));
}

#[test]
fn unchanged_rules_keep_their_state_when_others_are_edited() {
    let mut a = rule(Mode::Sustained);
    a.for_sec = 0;
    let mut b = rule(Mode::Sustained);
    b.id = 2;
    b.sensor = "u".into();
    b.for_sec = 0;
    let mut e = Engine::new(vec![a.clone(), b.clone()]);
    e.on_reading(0, "t", &ok(90.0));
    b.threshold = 10.0;
    let ev = e.set_rules(S, vec![a, b]);
    assert!(ev.is_empty());
    assert_eq!(e.status(1, S).unwrap().phase, RulePhase::Firing);
}

#[test]
fn a_disabled_rule_never_fires() {
    let mut r = rule(Mode::Sustained);
    r.enabled = false;
    r.for_sec = 0;
    let mut e = Engine::new(vec![r]);
    assert!(run(&mut e, 0, 30, 1, |_| 99.0).is_empty());
}

#[test]
fn readings_of_other_sensors_are_ignored() {
    let mut r = rule(Mode::Sustained);
    r.for_sec = 0;
    let mut e = Engine::new(vec![r]);
    assert!(e.on_reading(0, "other", &ok(99.0)).is_empty());
}

#[test]
fn two_rules_on_one_sensor_are_independent() {
    let mut warn = rule(Mode::Sustained);
    warn.for_sec = 0;
    warn.threshold = 70.0;
    warn.severity = Severity::Warn;
    let mut crit = rule(Mode::Sustained);
    crit.id = 2;
    crit.for_sec = 0;
    let mut e = Engine::new(vec![warn, crit]);
    let ev = e.on_reading(0, "t", &ok(75.0));
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0].severity, Severity::Warn);
    let ev = e.on_reading(S, "t", &ok(90.0));
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0].severity, Severity::Crit);
}

// ---- test events, status, mute ----

#[test]
fn a_test_engine_flags_every_event() {
    let mut r = rule(Mode::Sustained);
    r.for_sec = 0;
    let mut e = Engine::new_test(vec![r]);
    let ev = e.on_reading(0, "t", &ok(90.0));
    assert!(ev.iter().all(|x| x.is_test));
    let mut real = Engine::new(vec![{
        let mut r = rule(Mode::Sustained);
        r.for_sec = 0;
        r
    }]);
    assert!(real
        .on_reading(0, "t", &ok(90.0))
        .iter()
        .all(|x| !x.is_test));
}

#[test]
fn status_reports_progress_while_counting() {
    let mut e = Engine::new(vec![rule(Mode::Sustained)]);
    e.on_reading(0, "t", &ok(90.0));
    e.on_reading(4 * S, "t", &ok(90.0));
    let s = e.status(1, 4 * S).unwrap();
    assert_eq!(s.phase, RulePhase::Pending);
    assert_eq!(s.elapsed_ms, 4 * S);
    assert_eq!(s.needed_ms, 10 * S);
    assert_eq!(s.last_value, Some(90.0));
    assert!(e.status(42, 0).is_none());
}

#[test]
fn muting_silences_everything_but_critical_rules_when_they_bypass() {
    assert!(should_notify(Severity::Warn, false, true));
    assert!(!should_notify(Severity::Warn, true, true));
    assert!(should_notify(Severity::Crit, true, true));
    assert!(!should_notify(Severity::Crit, true, false));
    assert!(!should_notify(Severity::Info, true, true));
}

// ---- robustness ----

#[test]
fn random_input_never_panics_and_events_stay_consistent() {
    // A tiny deterministic generator keeps the test reproducible without a dependency.
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for mode in [Mode::Sustained, Mode::Average, Mode::Spikes] {
        let mut r = rule(mode);
        r.for_sec = 7;
        r.hysteresis = Some(70.0);
        r.repeat_sec = 5;
        let mut e = Engine::new(vec![r]);
        let mut active = false;
        let mut ts = 0i64;
        for _ in 0..5000 {
            ts += (next() % 4000) as i64; // irregular, sometimes repeated timestamps
            let reading = match next() % 10 {
                0 => gone(),
                1 => Availability::NoResponse { since_ms: ts },
                _ => ok((next() % 120) as f64),
            };
            for ev in e.on_reading(ts, "t", &reading) {
                match ev.kind {
                    EventKind::Fired => {
                        assert!(!active, "fired twice without resolving");
                        active = true;
                    }
                    EventKind::Repeat => assert!(active, "repeat while inactive"),
                    EventKind::Resolved | EventKind::Ended => {
                        assert!(active, "resolved while inactive");
                        active = false;
                    }
                    EventKind::Note => {}
                }
            }
        }
    }
}
