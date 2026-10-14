use crate::slug;
use argus_core::{Availability, CollectError, Collector, Reading, SensorMeta, Unit};
use std::time::Instant;
use sysinfo::Networks;

pub struct NetworkCollector {
    networks: Networks,
    last: Instant,
}

impl NetworkCollector {
    pub fn new() -> Self {
        Self {
            networks: Networks::new_with_refreshed_list(),
            last: Instant::now(),
        }
    }
}

impl Default for NetworkCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Bytes moved in `secs` seconds as megabits per second.
fn mbps(bytes: u64, secs: f64) -> f64 {
    bytes as f64 * 8.0 / 1_000_000.0 / secs
}

impl Collector for NetworkCollector {
    fn name(&self) -> &str {
        "network"
    }

    fn sensors(&self) -> Vec<SensorMeta> {
        let meta = |id: String, name: String| SensorMeta {
            id: id.as_str().into(),
            group: "Rede".to_string(),
            name,
            unit: Unit::Mbps,
        };
        let mut out = vec![
            meta("net.total.rx".into(), "Recebido (total)".into()),
            meta("net.total.tx".into(), "Enviado (total)".into()),
        ];
        for name in self.networks.keys() {
            let id = slug(name);
            out.push(meta(format!("net.{id}.rx"), format!("{name} recebido")));
            out.push(meta(format!("net.{id}.tx"), format!("{name} enviado")));
        }
        out
    }

    fn collect(&mut self, now_ms: i64) -> Result<Vec<Reading>, CollectError> {
        let secs = self.last.elapsed().as_secs_f64().max(0.001);
        self.networks.refresh(false);
        self.last = Instant::now();

        let (mut rx_total, mut tx_total) = (0.0, 0.0);
        let mut out = Vec::new();
        for (name, data) in self.networks.iter() {
            let id = slug(name);
            let (rx, tx) = (mbps(data.received(), secs), mbps(data.transmitted(), secs));
            rx_total += rx;
            tx_total += tx;
            out.push(Reading::new(
                format!("net.{id}.rx"),
                now_ms,
                Availability::ok(rx),
            ));
            out.push(Reading::new(
                format!("net.{id}.tx"),
                now_ms,
                Availability::ok(tx),
            ));
        }
        out.push(Reading::new(
            "net.total.rx",
            now_ms,
            Availability::ok(rx_total),
        ));
        out.push(Reading::new(
            "net.total.tx",
            now_ms,
            Availability::ok(tx_total),
        ));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::mbps;

    #[test]
    fn converts_bytes_to_megabits_per_second() {
        assert!((mbps(125_000, 1.0) - 1.0).abs() < 1e-9);
        assert!((mbps(125_000, 0.5) - 2.0).abs() < 1e-9);
    }
}
