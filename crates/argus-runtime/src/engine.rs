pub use crate::host::Health;
use crate::host::Host;
use argus_core::{Clock, Collector, Reading, Sample, SensorId, SensorMeta, SeriesStore};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

type Subscribers = Mutex<Vec<Sender<Arc<Vec<Reading>>>>>;
type Observer = Box<dyn Fn(&[Reading]) + Send + Sync>;

/// Runs one collection cycle at a time. [`Runtime`] drives it on a timer; tests call
/// [`Engine::tick`] directly with a fake clock.
pub struct Engine {
    hosts: Mutex<Vec<Host>>,
    clock: Arc<dyn Clock>,
    store: Arc<Mutex<SeriesStore>>,
    subscribers: Subscribers,
    observers: Mutex<Vec<Observer>>,
    timeout: Duration,
}

impl Engine {
    /// `timeout` is how long a cycle waits for the slowest collector before marking it as not
    /// responding. `retention_ms` bounds the in-memory history.
    pub fn new(
        collectors: Vec<Box<dyn Collector>>,
        clock: Arc<dyn Clock>,
        timeout: Duration,
        retention_ms: i64,
    ) -> Self {
        Self {
            hosts: Mutex::new(collectors.into_iter().map(Host::spawn).collect()),
            clock,
            store: Arc::new(Mutex::new(SeriesStore::new(retention_ms))),
            subscribers: Mutex::new(Vec::new()),
            observers: Mutex::new(Vec::new()),
            timeout,
        }
    }

    /// Catalog of every sensor the collectors can report.
    pub fn sensors(&self) -> Vec<SensorMeta> {
        let hosts = self.hosts.lock().expect("hosts lock");
        hosts.iter().flat_map(|h| h.sensors.clone()).collect()
    }

    /// Receives every batch of readings produced by a cycle.
    pub fn subscribe(&self) -> Receiver<Arc<Vec<Reading>>> {
        let (tx, rx) = channel();
        self.subscribers.lock().expect("subscribers lock").push(tx);
        rx
    }

    /// Runs `observer` synchronously at the end of every cycle, on the runtime's own thread, so
    /// whatever it does (alerts, recording) never depends on a window being open.
    pub fn add_observer(&self, observer: impl Fn(&[Reading]) + Send + Sync + 'static) {
        self.observers
            .lock()
            .expect("observers lock")
            .push(Box::new(observer));
    }

    pub fn health(&self) -> Vec<(String, Health)> {
        let hosts = self.hosts.lock().expect("hosts lock");
        hosts
            .iter()
            .map(|h| (h.name.clone(), h.health.clone()))
            .collect()
    }

    pub fn window(&self, sensor: &SensorId, window_ms: i64) -> Vec<Sample> {
        let now = self.clock.now_ms();
        self.store
            .lock()
            .expect("store lock")
            .window(sensor, now, window_ms)
    }

    /// Every stored sample of every sensor, for exporting the session. Sensors without samples are
    /// left out.
    pub fn export_series(&self) -> Vec<(SensorMeta, Vec<Sample>)> {
        let now = self.clock.now_ms();
        let store = self.store.lock().expect("store lock");
        self.sensors()
            .into_iter()
            .filter_map(|meta| {
                let samples = store.window(&meta.id, now.saturating_add(60_000), i64::MAX / 4);
                (!samples.is_empty()).then_some((meta, samples))
            })
            .collect()
    }

    pub fn latest(&self, sensor: &SensorId) -> Option<Sample> {
        self.store
            .lock()
            .expect("store lock")
            .latest(sensor)
            .cloned()
    }

    /// One cycle: ask every collector in parallel, wait at most `timeout` for all of them,
    /// store and publish the result. Failures are readings, never errors.
    pub fn tick(&self) -> Vec<Reading> {
        let now_ms = self.clock.now_ms();
        let mut hosts = self.hosts.lock().expect("hosts lock");
        let fresh: Vec<bool> = hosts
            .iter_mut()
            .map(|h| {
                let was_pending = h.is_pending();
                h.request(now_ms);
                !was_pending
            })
            .collect();
        let deadline = Instant::now() + self.timeout;
        let mut batch = Vec::new();
        for (host, fresh) in hosts.iter_mut().zip(fresh) {
            let wait = if fresh {
                deadline.saturating_duration_since(Instant::now())
            } else {
                Duration::ZERO
            };
            batch.extend(host.finish(now_ms, wait));
        }
        drop(hosts);

        {
            let mut store = self.store.lock().expect("store lock");
            for r in &batch {
                store.push(&r.sensor, r.ts_ms, r.value.clone());
            }
        }
        for observer in self.observers.lock().expect("observers lock").iter() {
            observer(&batch);
        }
        let shared = Arc::new(batch.clone());
        self.subscribers
            .lock()
            .expect("subscribers lock")
            .retain(|tx| tx.send(shared.clone()).is_ok());
        batch
    }
}

enum Msg {
    Wake,
    Stop,
}

/// An [`Engine`] ticking on its own thread until stopped or dropped.
pub struct Runtime {
    engine: Arc<Engine>,
    interval_ms: Arc<AtomicU64>,
    stop: Sender<Msg>,
    thread: Option<JoinHandle<()>>,
}

impl Runtime {
    pub fn start(engine: Engine, interval: Duration) -> Self {
        let engine = Arc::new(engine);
        let (stop, stop_rx) = channel::<Msg>();
        let worker = engine.clone();
        let interval_ms = Arc::new(AtomicU64::new(interval.as_millis() as u64));
        let shared_interval = interval_ms.clone();
        let thread = thread::Builder::new()
            .name("argus-runtime".to_string())
            .spawn(move || loop {
                let started = Instant::now();
                worker.tick();
                let interval = Duration::from_millis(shared_interval.load(Ordering::Relaxed));
                let rest = interval.saturating_sub(started.elapsed());
                match stop_rx.recv_timeout(rest) {
                    Err(RecvTimeoutError::Timeout) | Ok(Msg::Wake) => continue,
                    _ => break,
                }
            })
            .expect("spawn runtime thread");
        Self {
            engine,
            interval_ms,
            stop,
            thread: Some(thread),
        }
    }

    /// Changes the time between cycles; takes effect after the cycle in progress.
    pub fn set_interval(&self, interval: Duration) {
        let ms = (interval.as_millis() as u64).max(100);
        self.interval_ms.store(ms, Ordering::Relaxed);
        let _ = self.stop.send(Msg::Wake);
    }

    pub fn engine(&self) -> &Arc<Engine> {
        &self.engine
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        let _ = self.stop.send(Msg::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.shutdown();
    }
}
