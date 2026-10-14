use argus_alerts::{Actions, AlertEvent, EventKind, Mode, Operator, Rule, RulePhase, Severity};
use argus_core::{Clock, FakeClock, SystemClock};
use argus_runtime::{AlertService, Engine, RecordingDispatcher, Runtime};
use argus_sensors_fake::{Behavior, Control, FakeCollector};
use argus_store::{HistoryFilter, HistoryStore};
use std::sync::Arc;
use std::time::Duration;

fn rule(id: u64) -> Rule {
    Rule {
        id,
        name: format!("regra {id}"),
        sensor: "cpu.temp".into(),
        mode: Mode::Sustained,
        op: Operator::Above,
        threshold: 85.0,
        hysteresis: None,
        for_sec: 3,
        count: 3,
        recover_sec: 2,
        repeat_sec: 0,
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

struct Rig {
    engine: Engine,
    clock: FakeClock,
    control: Control,
    alerts: Arc<AlertService>,
    dispatcher: Arc<RecordingDispatcher>,
}

fn rig(rules: Vec<Rule>, history: HistoryStore) -> Rig {
    let (collector, control) = FakeCollector::new("cpu", &["cpu.temp"], Behavior::Value(50.0));
    let clock = FakeClock::new(1_000_000);
    let engine = Engine::new(
        vec![Box::new(collector)],
        Arc::new(clock.clone()),
        Duration::from_millis(500),
        600_000,
    );
    let dispatcher = Arc::new(RecordingDispatcher::default());
    let alerts = AlertService::new(rules, history, dispatcher.clone());
    alerts.attach(&engine);
    Rig {
        engine,
        clock,
        control,
        alerts,
        dispatcher,
    }
}

impl Rig {
    /// Runs `n` one-second cycles with the sensor at `value`.
    fn run(&self, n: u32, value: f64) {
        self.control.set(Behavior::Value(value));
        self.cycles(n);
    }

    fn cycles(&self, n: u32) {
        for _ in 0..n {
            self.engine.tick();
            self.clock.advance_ms(1000);
        }
    }

    fn now(&self) -> i64 {
        self.clock.now_ms()
    }

    /// Oldest first.
    fn history(&self) -> Vec<(EventKind, bool)> {
        let mut rows = self
            .alerts
            .history(&HistoryFilter {
                include_tests: true,
                ..Default::default()
            })
            .unwrap();
        rows.reverse();
        rows.into_iter()
            .map(|r| (r.event.kind, r.event.is_test))
            .collect()
    }
}

#[test]
fn a_full_lifecycle_runs_and_is_recorded_with_no_window_at_all() {
    // No UI, no subscriber: only the runtime and the alert service exist.
    let rig = rig(vec![rule(1)], HistoryStore::open_in_memory().unwrap());
    rig.run(10, 50.0);
    assert!(rig.history().is_empty());
    rig.run(6, 95.0); // above for 6 s with a 3 s rule: fires
    rig.run(6, 50.0); // normal long enough: resolves
    assert_eq!(
        rig.history(),
        vec![(EventKind::Fired, false), (EventKind::Resolved, false)]
    );

    let seen = rig.dispatcher.notifications();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].event.kind, EventKind::Fired);
    assert_eq!(
        seen[0].actions,
        Actions {
            toast: true,
            sound: true,
            notify: true
        }
    );
    // Coming back to normal is only a quiet on-screen note.
    assert_eq!(
        seen[1].actions,
        Actions {
            toast: true,
            sound: false,
            notify: false
        }
    );
}

#[test]
fn live_state_is_available_to_the_ui_layer() {
    let rig = rig(vec![rule(1)], HistoryStore::open_in_memory().unwrap());
    rig.run(2, 95.0);
    assert_eq!(
        rig.alerts.status(1, rig.now()).unwrap().phase,
        RulePhase::Pending
    );
    rig.run(4, 95.0);
    assert_eq!(
        rig.alerts.status(1, rig.now()).unwrap().phase,
        RulePhase::Firing
    );
    assert!(rig.alerts.acknowledge(1));
    assert_eq!(
        rig.alerts.status(1, rig.now()).unwrap().phase,
        RulePhase::Acknowledged
    );
    assert_eq!(rig.alerts.statuses(rig.now()).len(), 1);
}

#[test]
fn subscribers_receive_events_as_they_happen() {
    let rig = rig(vec![rule(1)], HistoryStore::open_in_memory().unwrap());
    let rx = rig.alerts.subscribe();
    rig.run(8, 95.0);
    let first = rx.try_recv().expect("a fired event");
    assert_eq!(first.kind, EventKind::Fired);
    assert_eq!(first.rule_name, "regra 1");
}

#[test]
fn muting_blocks_actions_but_not_the_history_and_critical_can_bypass() {
    let mut warn = rule(2);
    warn.severity = Severity::Warn;
    let rig = rig(vec![rule(1), warn], HistoryStore::open_in_memory().unwrap());
    rig.alerts.mute_until(i64::MAX);
    rig.run(6, 95.0);
    // Both fired and both are in the history...
    assert_eq!(
        rig.history()
            .iter()
            .filter(|(k, _)| *k == EventKind::Fired)
            .count(),
        2
    );
    // ...but only the critical one notified.
    let seen = rig.dispatcher.notifications();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].event.severity, Severity::Crit);

    // With the bypass off, nothing notifies while muted.
    let rig2 = self::rig(vec![rule(1)], HistoryStore::open_in_memory().unwrap());
    rig2.alerts.set_critical_bypasses_mute(false);
    rig2.alerts.mute_until(i64::MAX);
    rig2.run(6, 95.0);
    assert!(rig2.dispatcher.notifications().is_empty());
    assert_eq!(rig2.history().len(), 1);
}

#[test]
fn editing_an_active_rule_ends_it_and_records_that() {
    let rig = rig(vec![rule(1)], HistoryStore::open_in_memory().unwrap());
    rig.run(6, 95.0);
    let mut edited = rule(1);
    edited.threshold = 99.0;
    rig.alerts.set_rules(rig.now(), vec![edited]);
    assert_eq!(
        rig.history(),
        vec![(EventKind::Fired, false), (EventKind::Ended, false)]
    );
    // Ended is history only: no new notification.
    assert_eq!(rig.dispatcher.notifications().len(), 1);
}

#[test]
fn a_sensor_that_stops_answering_leaves_a_note_and_the_alert_stays_active() {
    let rig = rig(vec![rule(1)], HistoryStore::open_in_memory().unwrap());
    rig.run(6, 95.0);
    rig.control.set(Behavior::Fail("sem acesso".into()));
    rig.cycles(5);
    assert_eq!(
        rig.history(),
        vec![(EventKind::Fired, false), (EventKind::Note, false)]
    );
    assert_eq!(
        rig.alerts.status(1, rig.now()).unwrap().phase,
        RulePhase::Firing
    );
}

#[test]
fn the_history_survives_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite");
    {
        let rig = rig(vec![rule(1)], HistoryStore::open(&path).unwrap());
        rig.run(6, 95.0);
        assert_eq!(rig.history().len(), 1);
    }
    let store = HistoryStore::open(&path).unwrap();
    let rows = store.query(&HistoryFilter::default()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].event.kind, EventKind::Fired);
}

#[test]
fn test_events_only_ever_show_on_screen() {
    let rig = rig(vec![rule(1)], HistoryStore::open_in_memory().unwrap());
    rig.alerts.record(AlertEvent {
        ts_ms: 5,
        rule_id: 1,
        rule_name: "regra 1".into(),
        sensor: "cpu.temp".into(),
        severity: Severity::Crit,
        kind: EventKind::Fired,
        detail: "simulado".into(),
        value: Some(99.0),
        is_test: true,
    });
    let seen = rig.dispatcher.notifications();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        seen[0].actions,
        Actions {
            toast: true,
            sound: false,
            notify: false
        }
    );
    assert_eq!(rig.history(), vec![(EventKind::Fired, true)]);
}

#[test]
fn it_keeps_working_on_the_runtime_thread() {
    // The same wiring, driven by the real timer thread instead of manual ticks.
    let (collector, _control) = FakeCollector::new("cpu", &["cpu.temp"], Behavior::Value(95.0));
    let mut r = rule(1);
    r.for_sec = 0;
    let engine = Engine::new(
        vec![Box::new(collector)],
        Arc::new(SystemClock),
        Duration::from_millis(500),
        60_000,
    );
    let alerts = AlertService::new(
        vec![r],
        HistoryStore::open_in_memory().unwrap(),
        Arc::new(RecordingDispatcher::default()),
    );
    alerts.attach(&engine);
    let rx = alerts.subscribe();
    let runtime = Runtime::start(engine, Duration::from_millis(20));
    let ev = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("an alert from the background runtime");
    assert_eq!(ev.kind, EventKind::Fired);
    runtime.stop();
}
