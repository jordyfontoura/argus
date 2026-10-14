use argus_core::{Availability, CollectError, Collector, Reading, SensorMeta, Unit};
use sysinfo::{MemoryRefreshKind, RefreshKind, System};

pub struct MemoryCollector {
    sys: System,
}

impl MemoryCollector {
    pub fn new() -> Self {
        Self {
            sys: System::new_with_specifics(
                RefreshKind::nothing().with_memory(MemoryRefreshKind::everything()),
            ),
        }
    }
}

impl Default for MemoryCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for MemoryCollector {
    fn name(&self) -> &str {
        "memory"
    }

    fn sensors(&self) -> Vec<SensorMeta> {
        let meta = |id: &str, name: &str, unit| SensorMeta {
            id: id.into(),
            group: "Memória".to_string(),
            name: name.to_string(),
            unit,
        };
        vec![
            meta("mem.used", "Em uso", Unit::Bytes),
            meta("mem.total", "Total instalado", Unit::Bytes),
            meta("mem.available", "Disponível", Unit::Bytes),
            meta("mem.used_pct", "Uso", Unit::Percent),
            meta("mem.swap_used", "Arquivo de paginação em uso", Unit::Bytes),
            meta("mem.swap_total", "Arquivo de paginação total", Unit::Bytes),
        ]
    }

    fn collect(&mut self, now_ms: i64) -> Result<Vec<Reading>, CollectError> {
        self.sys.refresh_memory();
        let total = self.sys.total_memory();
        if total == 0 {
            return Err(CollectError("o Windows não informou a memória".to_string()));
        }
        let used = self.sys.used_memory();
        let ok = |id: &str, v: f64| Reading::new(id, now_ms, Availability::ok(v));
        Ok(vec![
            ok("mem.used", used as f64),
            ok("mem.total", total as f64),
            ok("mem.available", self.sys.available_memory() as f64),
            ok("mem.used_pct", used as f64 / total as f64 * 100.0),
            ok("mem.swap_used", self.sys.used_swap() as f64),
            ok("mem.swap_total", self.sys.total_swap() as f64),
        ])
    }
}
