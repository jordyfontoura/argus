use crate::{Availability, SensorId};
use std::collections::{HashMap, VecDeque};

/// One point of a series.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub ts_ms: i64,
    pub value: Availability,
}

/// Per-sensor history bounded by time (not by sample count), so a different collection
/// interval never changes how much history is kept.
#[derive(Debug, Clone)]
pub struct SeriesStore {
    retention_ms: i64,
    series: HashMap<SensorId, VecDeque<Sample>>,
}

impl SeriesStore {
    pub fn new(retention_ms: i64) -> Self {
        Self {
            retention_ms,
            series: HashMap::new(),
        }
    }

    /// Appends a sample and drops everything older than the retention relative to it.
    /// Samples older than the newest one already stored are ignored (out of order).
    pub fn push(&mut self, sensor: &SensorId, ts_ms: i64, value: Availability) {
        let buf = self.series.entry(sensor.clone()).or_default();
        if buf.back().is_some_and(|last| ts_ms < last.ts_ms) {
            return;
        }
        buf.push_back(Sample { ts_ms, value });
        let cutoff = ts_ms - self.retention_ms;
        while buf.front().is_some_and(|s| s.ts_ms < cutoff) {
            buf.pop_front();
        }
    }

    /// Samples with `now_ms - window_ms <= ts <= now_ms`, oldest first.
    pub fn window(&self, sensor: &SensorId, now_ms: i64, window_ms: i64) -> Vec<Sample> {
        let from = now_ms - window_ms;
        self.series
            .get(sensor)
            .map(|buf| {
                buf.iter()
                    .filter(|s| s.ts_ms >= from && s.ts_ms <= now_ms)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn latest(&self, sensor: &SensorId) -> Option<&Sample> {
        self.series.get(sensor).and_then(|b| b.back())
    }

    /// Min and max of the real values inside the window (unavailable samples are skipped).
    pub fn min_max(&self, sensor: &SensorId, now_ms: i64, window_ms: i64) -> Option<(f64, f64)> {
        self.window(sensor, now_ms, window_ms)
            .iter()
            .filter_map(|s| s.value.value())
            .fold(None, |acc, v| match acc {
                None => Some((v, v)),
                Some((lo, hi)) => Some((lo.min(v), hi.max(v))),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> SensorId {
        SensorId::new("cpu.load")
    }

    #[test]
    fn window_is_measured_in_time_not_in_samples() {
        let mut store = SeriesStore::new(60_000);
        for t in (0..=10_000).step_by(1_000) {
            store.push(&id(), t, Availability::ok(1.0));
        }
        assert_eq!(store.window(&id(), 10_000, 10_000).len(), 11);
        let mut slow = SeriesStore::new(60_000);
        for t in (0..=10_000).step_by(5_000) {
            slow.push(&id(), t, Availability::ok(1.0));
        }
        assert_eq!(slow.window(&id(), 10_000, 10_000).len(), 3);
        assert_eq!(slow.window(&id(), 10_000, 4_000).len(), 1);
    }

    #[test]
    fn retention_drops_old_samples_by_time() {
        let mut store = SeriesStore::new(5_000);
        for t in [0, 1_000, 2_000, 9_000] {
            store.push(&id(), t, Availability::ok(t as f64));
        }
        let kept = store.window(&id(), 9_000, 60_000);
        assert_eq!(
            kept.iter().map(|s| s.ts_ms).collect::<Vec<_>>(),
            vec![9_000]
        );
    }

    #[test]
    fn out_of_order_samples_are_ignored() {
        let mut store = SeriesStore::new(60_000);
        store.push(&id(), 2_000, Availability::ok(2.0));
        store.push(&id(), 1_000, Availability::ok(1.0));
        assert_eq!(store.window(&id(), 2_000, 60_000).len(), 1);
    }

    #[test]
    fn min_max_skips_unavailable_samples() {
        let mut store = SeriesStore::new(60_000);
        store.push(&id(), 0, Availability::ok(5.0));
        store.push(&id(), 1_000, Availability::unavailable("x"));
        store.push(&id(), 2_000, Availability::ok(9.0));
        assert_eq!(store.min_max(&id(), 2_000, 10_000), Some((5.0, 9.0)));
        assert_eq!(store.min_max(&SensorId::new("none"), 2_000, 10_000), None);
    }

    #[test]
    fn unavailable_samples_are_kept_so_gaps_are_visible() {
        let mut store = SeriesStore::new(60_000);
        store.push(&id(), 0, Availability::unavailable("sem driver"));
        assert_eq!(
            store.latest(&id()).unwrap().value,
            Availability::unavailable("sem driver")
        );
    }
}
