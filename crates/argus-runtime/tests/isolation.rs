use argus_core::{Availability, FakeClock, SensorId};
use argus_runtime::{Engine, Health, Runtime};
use argus_sensors_fake::{Behavior, FakeCollector};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn id(s: &str) -> SensorId {
    SensorId::new(s)
}

fn value_of(batch: &[argus_core::Reading], sensor: &str) -> Availability {
    batch
        .iter()
        .find(|r| r.sensor == id(sensor))
        .unwrap_or_else(|| panic!("no reading for {sensor}"))
        .value
        .clone()
}

fn engine_with(
    collectors: Vec<Box<dyn argus_core::Collector>>,
    clock: &FakeClock,
    timeout_ms: u64,
) -> Engine {
    Engine::new(
        collectors,
        Arc::new(clock.clone()),
        Duration::from_millis(timeout_ms),
        600_000,
    )
}

#[test]
fn failing_collector_only_marks_its_own_sensors() {
    let (good, _) = FakeCollector::new("cpu", &["cpu.load"], Behavior::Value(10.0));
    let (bad, _) = FakeCollector::new("mem", &["mem.used"], Behavior::Fail("sem acesso".into()));
    let clock = FakeClock::new(1_000);
    let engine = engine_with(vec![Box::new(good), Box::new(bad)], &clock, 500);

    let batch = engine.tick();

    assert_eq!(value_of(&batch, "cpu.load"), Availability::ok(10.0));
    assert_eq!(
        value_of(&batch, "mem.used"),
        Availability::unavailable("sem acesso")
    );
}

#[test]
fn panicking_collector_does_not_stop_the_others_and_recovers() {
    let (good, _) = FakeCollector::new("cpu", &["cpu.load"], Behavior::Value(10.0));
    let (flaky, control) = FakeCollector::new("gpu", &["gpu.load"], Behavior::Panic);
    let clock = FakeClock::new(1_000);
    let engine = engine_with(vec![Box::new(good), Box::new(flaky)], &clock, 500);

    let batch = engine.tick();
    assert_eq!(value_of(&batch, "cpu.load"), Availability::ok(10.0));
    assert!(matches!(
        value_of(&batch, "gpu.load"),
        Availability::Unavailable { .. }
    ));

    control.set(Behavior::Value(42.0));
    clock.advance_ms(1_000);
    let batch = engine.tick();
    assert_eq!(value_of(&batch, "gpu.load"), Availability::ok(42.0));
    assert!(engine.health().iter().all(|(_, h)| *h == Health::Healthy));
}

#[test]
fn hanging_collector_is_marked_no_response_without_blocking_the_cycle() {
    let (good, _) = FakeCollector::new("cpu", &["cpu.load"], Behavior::Value(10.0));
    let (slow, control) = FakeCollector::new(
        "disk",
        &["disk.busy"],
        Behavior::Hang(Duration::from_millis(600), 7.0),
    );
    let clock = FakeClock::new(1_000);
    // 200 ms: the healthy collector must answer inside it even when the machine is busy.
    let engine = engine_with(vec![Box::new(good), Box::new(slow)], &clock, 200);

    let started = Instant::now();
    let batch = engine.tick();
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "cycle blocked"
    );
    assert_eq!(value_of(&batch, "cpu.load"), Availability::ok(10.0));
    assert_eq!(
        value_of(&batch, "disk.busy"),
        Availability::NoResponse { since_ms: 1_000 }
    );

    // The hang ends; the next cycles pick the collector up again by themselves.
    control.set(Behavior::Value(7.0));
    std::thread::sleep(Duration::from_millis(700));
    clock.advance_ms(1_000);
    let _ = engine.tick();
    clock.advance_ms(1_000);
    let batch = engine.tick();
    assert_eq!(value_of(&batch, "disk.busy"), Availability::ok(7.0));
}

#[test]
fn readings_are_stored_by_time_and_published_to_subscribers() {
    let (good, _) = FakeCollector::new("cpu", &["cpu.load"], Behavior::Value(5.0));
    let clock = FakeClock::new(10_000);
    let engine = engine_with(vec![Box::new(good)], &clock, 500);
    let rx = engine.subscribe();

    engine.tick();
    clock.advance_ms(1_000);
    engine.tick();

    assert_eq!(rx.try_iter().count(), 2);
    assert_eq!(engine.window(&id("cpu.load"), 5_000).len(), 2);
    assert_eq!(engine.window(&id("cpu.load"), 500).len(), 1);
    assert_eq!(engine.sensors().len(), 1);
}

#[test]
fn runtime_keeps_collecting_on_its_own_thread() {
    let (good, _) = FakeCollector::new("cpu", &["cpu.load"], Behavior::Value(5.0));
    let engine = Engine::new(
        vec![Box::new(good)],
        Arc::new(argus_core::SystemClock),
        Duration::from_millis(200),
        60_000,
    );
    let rx = engine.subscribe();
    let runtime = Runtime::start(engine, Duration::from_millis(20));

    let first = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("first batch");
    let second = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("second batch");
    assert!(second[0].ts_ms >= first[0].ts_ms);
    runtime.stop();
}

#[test]
fn changing_the_interval_applies_without_waiting_for_the_old_one() {
    let (good, _) = FakeCollector::new("cpu", &["cpu.load"], Behavior::Value(5.0));
    let engine = Engine::new(
        vec![Box::new(good)],
        Arc::new(argus_core::SystemClock),
        Duration::from_millis(200),
        60_000,
    );
    let rx = engine.subscribe();
    let runtime = Runtime::start(engine, Duration::from_secs(30));
    rx.recv_timeout(Duration::from_secs(2))
        .expect("first batch");

    runtime.set_interval(Duration::from_millis(100));

    rx.recv_timeout(Duration::from_secs(2))
        .expect("next batch should follow the new interval");
    runtime.stop();
}
