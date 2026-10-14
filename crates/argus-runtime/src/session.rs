//! Minimum and maximum of every sensor since the app started (or since the user reset them).
//! Kept by the runtime, not by a screen, so the numbers are right even if the Sensors screen was
//! never opened.

use argus_core::{Reading, SensorId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct SessionStats {
    ranges: Mutex<HashMap<SensorId, (f64, f64)>>,
}

impl SessionStats {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Folds one cycle into the ranges. Readings that are not real values (unavailable, no
    /// response) never count, so a failure cannot show up as a minimum of zero.
    pub fn observe(&self, batch: &[Reading]) {
        let mut ranges = self.ranges.lock().expect("session lock");
        for r in batch {
            let Some(v) = r.value.value() else { continue };
            if !v.is_finite() {
                continue;
            }
            ranges
                .entry(r.sensor.clone())
                .and_modify(|(lo, hi)| {
                    *lo = lo.min(v);
                    *hi = hi.max(v);
                })
                .or_insert((v, v));
        }
    }

    /// Registers the stats with the runtime engine.
    pub fn attach(self: &Arc<Self>, engine: &crate::Engine) {
        let me = Arc::clone(self);
        engine.add_observer(move |batch| me.observe(batch));
    }

    /// `(sensor, min, max)` for every sensor that has had a real reading, sorted by id.
    pub fn snapshot(&self) -> Vec<(SensorId, f64, f64)> {
        let mut out: Vec<_> = self
            .ranges
            .lock()
            .expect("session lock")
            .iter()
            .map(|(id, (lo, hi))| (id.clone(), *lo, *hi))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    pub fn reset(&self) {
        self.ranges.lock().expect("session lock").clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_core::Availability;

    fn r(id: &str, v: Availability) -> Reading {
        Reading::new(id, 0, v)
    }

    #[test]
    fn tracks_min_and_max_across_cycles() {
        let s = SessionStats::default();
        s.observe(&[r("a", Availability::ok(5.0)), r("b", Availability::ok(1.0))]);
        s.observe(&[
            r("a", Availability::ok(9.0)),
            r("b", Availability::ok(-2.0)),
        ]);
        s.observe(&[r("a", Availability::ok(7.0))]);
        let snap = s.snapshot();
        assert_eq!(snap[0], (SensorId::new("a"), 5.0, 9.0));
        assert_eq!(snap[1], (SensorId::new("b"), -2.0, 1.0));
    }

    #[test]
    fn failures_never_become_a_minimum() {
        let s = SessionStats::default();
        s.observe(&[
            r("a", Availability::unavailable("x")),
            r("b", Availability::NoResponse { since_ms: 0 }),
        ]);
        assert!(s.snapshot().is_empty(), "no real value yet, no range");
        s.observe(&[r("a", Availability::ok(10.0))]);
        s.observe(&[r("a", Availability::unavailable("x"))]);
        assert_eq!(s.snapshot(), vec![(SensorId::new("a"), 10.0, 10.0)]);
    }

    #[test]
    fn non_finite_values_are_ignored_and_reset_clears_everything() {
        let s = SessionStats::default();
        s.observe(&[
            r("a", Availability::ok(f64::NAN)),
            r("a", Availability::ok(f64::INFINITY)),
            r("a", Availability::ok(3.0)),
        ]);
        assert_eq!(s.snapshot(), vec![(SensorId::new("a"), 3.0, 3.0)]);
        s.reset();
        assert!(s.snapshot().is_empty());
        s.observe(&[r("a", Availability::ok(8.0))]);
        assert_eq!(
            s.snapshot(),
            vec![(SensorId::new("a"), 8.0, 8.0)],
            "starts again from the next reading"
        );
    }
}
