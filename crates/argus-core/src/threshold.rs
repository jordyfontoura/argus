use crate::SensorId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Warning and critical limits of one sensor, in its canonical unit.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Threshold {
    pub warn: f64,
    pub crit: f64,
}

/// Where a reading sits relative to its limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Ok,
    Warn,
    Crit,
}

/// Single source of limits: cards, the sensor tree and presets all read from here.
#[derive(Debug, Clone, Default)]
pub struct ThresholdRegistry {
    limits: HashMap<SensorId, Threshold>,
}

impl ThresholdRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Generic limits for the sensors Argus knows. Hardware-specific values (such as a CPU's
    /// Tjmax) are applied on top of these when they are known.
    pub fn with_defaults() -> Self {
        let mut reg = Self::new();
        for (sensor, warn, crit) in [
            ("cpu.load", 90.0, 98.0),
            ("cpu.temp", 80.0, 90.0),
            ("cpu.power", 110.0, 130.0),
            ("mem.used_pct", 80.0, 92.0),
            ("gpu.0.load", 95.0, 99.0),
            ("gpu.0.temp", 75.0, 85.0),
            ("gpu.0.hotspot", 90.0, 105.0),
            ("gpu.0.fan", 85.0, 95.0),
            ("gpu.0.vram_pct", 85.0, 95.0),
        ] {
            reg.set(sensor, warn, crit);
        }
        reg
    }

    /// Sets the limits of a sensor. Returns `false` and changes nothing if `warn > crit`.
    pub fn set(&mut self, sensor: impl Into<SensorId>, warn: f64, crit: f64) -> bool {
        if warn.is_nan() || crit.is_nan() || warn > crit {
            return false;
        }
        self.limits.insert(sensor.into(), Threshold { warn, crit });
        true
    }

    pub fn get(&self, sensor: &SensorId) -> Option<Threshold> {
        self.limits.get(sensor).copied()
    }

    /// `None` when the sensor has no registered limits.
    pub fn level(&self, sensor: &SensorId, value: f64) -> Option<Level> {
        let limits = self.get(sensor)?;
        Some(if value >= limits.crit {
            Level::Crit
        } else if value >= limits.warn {
            Level::Warn
        } else {
            Level::Ok
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_follow_the_limits() {
        let mut reg = ThresholdRegistry::new();
        assert!(reg.set("cpu.temp", 80.0, 95.0));
        let id = SensorId::new("cpu.temp");
        assert_eq!(reg.level(&id, 79.9), Some(Level::Ok));
        assert_eq!(reg.level(&id, 80.0), Some(Level::Warn));
        assert_eq!(reg.level(&id, 95.0), Some(Level::Crit));
    }

    #[test]
    fn defaults_are_ordered_and_cover_the_main_sensors() {
        let reg = ThresholdRegistry::with_defaults();
        for id in ["cpu.temp", "cpu.load", "gpu.0.temp", "mem.used_pct"] {
            let t = reg.get(&SensorId::new(id)).expect(id);
            assert!(t.warn < t.crit, "{id}");
        }
    }

    #[test]
    fn unknown_sensor_has_no_level() {
        let reg = ThresholdRegistry::new();
        assert_eq!(reg.level(&SensorId::new("nope"), 1.0), None);
    }

    #[test]
    fn rejects_inverted_or_nan_limits() {
        let mut reg = ThresholdRegistry::new();
        assert!(!reg.set("a", 90.0, 80.0));
        assert!(!reg.set("a", f64::NAN, 80.0));
        assert_eq!(reg.get(&SensorId::new("a")), None);
    }
}
