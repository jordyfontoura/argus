//! Deterministic scenarios for end-to-end runs: `ARGUS_SENSORS=fake:<name>` makes the app read
//! these instead of the hardware, so an alert can be triggered on demand without heating a CPU.

use argus_core::{Availability, CollectError, Collector, Reading, SensorMeta, Unit};

/// Package temperature in °C of the "overheat" scenario, `elapsed_s` seconds after the start.
/// A 60 s loop: calm, a quick climb to 95 °C, 25 s of heat, a quick cool-down, calm.
pub fn overheat_temperature(elapsed_s: f64) -> f64 {
    let t = elapsed_s.rem_euclid(60.0);
    match t {
        t if t < 10.0 => 50.0,
        t if t < 15.0 => 50.0 + (t - 10.0) / 5.0 * 45.0,
        t if t < 40.0 => 95.0,
        t if t < 45.0 => 95.0 - (t - 40.0) / 5.0 * 45.0,
        _ => 50.0,
    }
}

fn meta(id: &str, group: &str, name: &str, unit: Unit) -> SensorMeta {
    SensorMeta {
        id: id.into(),
        group: group.to_string(),
        name: name.to_string(),
        unit,
    }
}

/// CPU, GPU and memory sensors (same ids as the real ones) following a named scenario.
struct Scenario {
    name: &'static str,
    started_ms: Option<i64>,
}

impl Collector for Scenario {
    fn name(&self) -> &str {
        self.name
    }

    fn sensors(&self) -> Vec<SensorMeta> {
        vec![
            meta("cpu.load", "CPU", "Carga total", Unit::Percent),
            meta("cpu.clock", "CPU", "Clock médio", Unit::Megahertz),
            meta("cpu.temp", "CPU", "Temperatura", Unit::Celsius),
            meta("gpu.0.load", "GPU", "Carga", Unit::Percent),
            meta("gpu.0.temp", "GPU", "Temperatura", Unit::Celsius),
            meta("mem.used", "Memória", "Em uso", Unit::Bytes),
            meta("mem.total", "Memória", "Total instalado", Unit::Bytes),
            meta("mem.used_pct", "Memória", "Uso", Unit::Percent),
        ]
    }

    fn collect(&mut self, now_ms: i64) -> Result<Vec<Reading>, CollectError> {
        let started = *self.started_ms.get_or_insert(now_ms);
        let elapsed = (now_ms - started) as f64 / 1000.0;
        let temp = overheat_temperature(elapsed);
        let load = 20.0 + (temp - 50.0) * 1.6;
        let ok = |id: &str, v: f64| Reading::new(id, now_ms, Availability::ok(v));
        Ok(vec![
            ok("cpu.load", load),
            ok("cpu.clock", 3600.0 + load * 6.0),
            ok("cpu.temp", temp),
            ok("gpu.0.load", 8.0),
            ok("gpu.0.temp", 45.0),
            ok("mem.used", 8.0e9),
            ok("mem.total", 16.0e9),
            ok("mem.used_pct", 50.0),
        ])
    }
}

/// The collectors of a named scenario, or `None` for an unknown name.
pub fn scenario(name: &str) -> Option<Vec<Box<dyn Collector>>> {
    match name {
        "overheat" => Some(vec![Box::new(Scenario {
            name: "fake-overheat",
            started_ms: None,
        })]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_overheat_curve_is_calm_hot_calm_and_repeats() {
        assert_eq!(overheat_temperature(0.0), 50.0);
        assert_eq!(overheat_temperature(9.9), 50.0);
        assert!((overheat_temperature(12.5) - 72.5).abs() < 1e-9);
        assert_eq!(overheat_temperature(15.0), 95.0);
        assert_eq!(overheat_temperature(39.9), 95.0);
        assert_eq!(overheat_temperature(50.0), 50.0);
        assert_eq!(overheat_temperature(75.0), 95.0, "second loop");
    }

    #[test]
    fn the_scenario_reports_every_sensor_it_declares() {
        let mut collectors = scenario("overheat").expect("known scenario");
        let c = &mut collectors[0];
        let declared: Vec<String> = c.sensors().iter().map(|m| m.id.to_string()).collect();
        let got: Vec<String> = c
            .collect(1_000)
            .unwrap()
            .iter()
            .map(|r| r.sensor.to_string())
            .collect();
        assert_eq!(declared, got);
        // The first cycle is time zero of the scenario.
        let temp = c
            .collect(1_000 + 20_000)
            .unwrap()
            .into_iter()
            .find(|r| r.sensor.as_str() == "cpu.temp")
            .unwrap();
        assert_eq!(temp.value, Availability::ok(95.0));
    }

    #[test]
    fn unknown_scenarios_are_rejected() {
        assert!(scenario("nope").is_none());
    }
}
