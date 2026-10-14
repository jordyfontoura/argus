use argus_alerts::{Actions, EventKind, Mode, Operator, Rule, RulePhase, Severity};
use argus_core::{Clock, FakeClock, SensorId};
use argus_runtime::{AlertService, Engine, RecordingDispatcher};
use argus_sensors_fake::{Behavior, FakeCollector};
use argus_store::{HistoryFilter, HistoryStore};
use std::sync::Arc;
use std::time::Duration;

fn rule() -> Rule {
    Rule {
        id: 1,
        name: "CPU superaquecendo".into(),
        sensor: "cpu.temp".into(),
        mode: Mode::Sustained,
        op: Operator::Above,
        threshold: 85.0,
        hysteresis: Some(80.0),
        for_sec: 5,
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
    alerts: Arc<AlertService>,
    dispatcher: Arc<RecordingDispatcher>,
}

fn rig() -> Rig {
    let (collector, _control) = FakeCollector::new("cpu", &["cpu.temp"], Behavior::Value(40.0));
    let clock = FakeClock::new(2_000_000);
    let engine = Engine::new(
        vec![Box::new(collector)],
        Arc::new(clock.clone()),
        Duration::from_millis(500),
        600_000,
    );
    let dispatcher = Arc::new(RecordingDispatcher::default());
    let alerts = AlertService::new(
        vec![rule()],
        HistoryStore::open_in_memory().unwrap(),
        dispatcher.clone(),
    );
    alerts.attach(&engine);
    for _ in 0..5 {
        engine.tick();
        clock.advance_ms(1000);
    }
    Rig {
        engine,
        clock,
        alerts,
        dispatcher,
    }
}

#[test]
fn simulating_never_touches_the_real_series_or_the_real_rule_state() {
    let rig = rig();
    let id = SensorId::new("cpu.temp");
    let before_series = rig.engine.window(&id, 600_000);
    let before_status = rig.alerts.status(1, rig.clock.now_ms()).unwrap();
    assert_eq!(before_status.phase, RulePhase::Ok);

    let result = rig.alerts.simulate(&rule(), rig.clock.now_ms());
    assert!(!result.points.is_empty());

    // The real data the charts read is exactly what it was.
    assert_eq!(rig.engine.window(&id, 600_000), before_series);
    // The real rule did not move: still OK, no progress, no peak.
    let after_status = rig.alerts.status(1, rig.clock.now_ms()).unwrap();
    assert_eq!(after_status, before_status);
}

#[test]
fn simulated_events_are_flagged_filterable_and_only_toast() {
    let rig = rig();
    rig.alerts.simulate(&rule(), rig.clock.now_ms());

    // Hidden by default, visible when asked for, all flagged.
    assert!(rig
        .alerts
        .history(&HistoryFilter::default())
        .unwrap()
        .is_empty());
    let all = rig
        .alerts
        .history(&HistoryFilter {
            include_tests: true,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(all.len(), 2);
    assert!(all.iter().all(|r| r.event.is_test));
    assert_eq!(
        rig.alerts.history_count(&HistoryFilter::default()).unwrap(),
        0
    );

    // On screen only: no sound, no Windows notification, even though the rule asks for both.
    let seen = rig.dispatcher.notifications();
    assert!(!seen.is_empty());
    assert!(seen.iter().all(|n| n.actions
        == Actions {
            toast: true,
            sound: false,
            notify: false
        }));
    assert!(seen.iter().all(|n| n.event.is_test));
}

#[test]
fn a_simulation_while_the_real_rule_is_firing_does_not_disturb_it() {
    let rig = rig();
    // Drive the real rule into firing with real readings.
    let hot = Engine::new(
        vec![Box::new(
            FakeCollector::new("hot", &["cpu.temp"], Behavior::Value(95.0)).0,
        )],
        Arc::new(rig.clock.clone()),
        Duration::from_millis(500),
        600_000,
    );
    rig.alerts.attach(&hot);
    for _ in 0..7 {
        hot.tick();
        rig.clock.advance_ms(1000);
    }
    assert_eq!(
        rig.alerts.status(1, rig.clock.now_ms()).unwrap().phase,
        RulePhase::Firing
    );
    let real_events = rig
        .alerts
        .history(&HistoryFilter::default())
        .unwrap()
        .iter()
        .filter(|e| e.event.kind == EventKind::Fired)
        .count();

    rig.alerts.simulate(&rule(), rig.clock.now_ms());

    assert_eq!(
        rig.alerts.status(1, rig.clock.now_ms()).unwrap().phase,
        RulePhase::Firing
    );
    // Real history (tests hidden) still holds only the real firing.
    let visible = rig.alerts.history(&HistoryFilter::default()).unwrap();
    assert!(visible.iter().all(|e| !e.event.is_test));
    assert_eq!(
        visible
            .iter()
            .filter(|e| e.event.kind == EventKind::Fired)
            .count(),
        real_events
    );
}
