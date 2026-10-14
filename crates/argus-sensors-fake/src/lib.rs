//! Synthetic collectors for tests and end-to-end runs. They never touch the OS.

mod scenario;

pub use scenario::{overheat_temperature, scenario};

use argus_core::{Availability, CollectError, Collector, Reading, SensorId, SensorMeta, Unit};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What a [`FakeCollector`] does on its next `collect` call.
#[derive(Debug, Clone)]
pub enum Behavior {
    /// Report this value on every sensor.
    Value(f64),
    /// Return a collector-level error.
    Fail(String),
    /// Panic inside `collect`.
    Panic,
    /// Block for this long, then report this value.
    Hang(Duration, f64),
}

/// Changes the behavior of a [`FakeCollector`] that already moved to another thread.
#[derive(Debug, Clone)]
pub struct Control(Arc<Mutex<Behavior>>);

impl Control {
    pub fn set(&self, behavior: Behavior) {
        *self.0.lock().expect("behavior lock") = behavior;
    }
}

pub struct FakeCollector {
    name: String,
    sensors: Vec<SensorMeta>,
    behavior: Arc<Mutex<Behavior>>,
}

impl FakeCollector {
    /// A collector exposing one percent sensor per id, starting with `behavior`.
    pub fn new(name: &str, sensor_ids: &[&str], behavior: Behavior) -> (Self, Control) {
        let behavior = Arc::new(Mutex::new(behavior));
        let sensors = sensor_ids
            .iter()
            .map(|id| SensorMeta {
                id: SensorId::new(*id),
                group: name.to_string(),
                name: id.to_string(),
                unit: Unit::Percent,
            })
            .collect();
        (
            Self {
                name: name.to_string(),
                sensors,
                behavior: behavior.clone(),
            },
            Control(behavior),
        )
    }
}

impl Collector for FakeCollector {
    fn name(&self) -> &str {
        &self.name
    }

    fn sensors(&self) -> Vec<SensorMeta> {
        self.sensors.clone()
    }

    fn collect(&mut self, now_ms: i64) -> Result<Vec<Reading>, CollectError> {
        let behavior = self.behavior.lock().expect("behavior lock").clone();
        let value = match behavior {
            Behavior::Value(v) => v,
            Behavior::Fail(msg) => return Err(CollectError(msg)),
            Behavior::Panic => panic!("fake collector panic"),
            Behavior::Hang(d, v) => {
                std::thread::sleep(d);
                v
            }
        };
        Ok(self
            .sensors
            .iter()
            .map(|s| Reading::new(s.id.clone(), now_ms, Availability::ok(value)))
            .collect())
    }
}
