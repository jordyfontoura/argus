use crate::pdh::{Counter, Pdh};
use crate::slug;
use argus_core::{Availability, CollectError, Collector, Reading, SensorMeta, Unit};
use sysinfo::Disks;

pub struct DiskCollector {
    disks: Disks,
    pdh: Result<Pdh, String>,
    busy: Result<Counter, String>,
    read: Result<Counter, String>,
    write: Result<Counter, String>,
}

impl DiskCollector {
    pub fn new() -> Self {
        let mut pdh = Pdh::new();
        let unavailable = || Err("PDH indisponível".to_string());
        let (mut busy, mut read, mut write) = (unavailable(), unavailable(), unavailable());
        if let Ok(p) = pdh.as_mut() {
            busy = p.add(r"\PhysicalDisk(_Total)\% Disk Time");
            read = p.add(r"\PhysicalDisk(_Total)\Disk Read Bytes/sec");
            write = p.add(r"\PhysicalDisk(_Total)\Disk Write Bytes/sec");
            let _ = p.collect();
        }
        Self {
            disks: Disks::new_with_refreshed_list(),
            pdh,
            busy,
            read,
            write,
        }
    }

    fn volume_id(mount: &str) -> String {
        slug(mount)
    }

    fn counter(&self, counter: &Result<Counter, String>, clamp_percent: bool) -> Availability {
        let (Ok(pdh), Ok(counter)) = (&self.pdh, counter) else {
            let why = counter.as_ref().err().cloned().unwrap_or_default();
            return Availability::unavailable(format!("contador do Windows indisponível ({why})"));
        };
        match pdh.value(*counter) {
            Ok(v) if clamp_percent => Availability::ok(v.clamp(0.0, 100.0)),
            Ok(v) => Availability::ok(v.max(0.0)),
            Err(why) => Availability::unavailable(format!("contador do Windows sem dados ({why})")),
        }
    }
}

impl Default for DiskCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for DiskCollector {
    fn name(&self) -> &str {
        "disk"
    }

    fn sensors(&self) -> Vec<SensorMeta> {
        let meta = |id: String, name: String, unit| SensorMeta {
            id: id.as_str().into(),
            group: "Armazenamento".to_string(),
            name,
            unit,
        };
        let mut out = vec![
            meta("disk.busy".into(), "Atividade".into(), Unit::Percent),
            meta(
                "disk.read".into(),
                "Leitura total".into(),
                Unit::BytesPerSecond,
            ),
            meta(
                "disk.write".into(),
                "Gravação total".into(),
                Unit::BytesPerSecond,
            ),
        ];
        for disk in self.disks.list() {
            let mount = disk.mount_point().to_string_lossy().to_string();
            let id = Self::volume_id(&mount);
            out.push(meta(
                format!("disk.{id}.used_pct"),
                format!("{mount} uso"),
                Unit::Percent,
            ));
            out.push(meta(
                format!("disk.{id}.used"),
                format!("{mount} usado"),
                Unit::Bytes,
            ));
            out.push(meta(
                format!("disk.{id}.total"),
                format!("{mount} capacidade"),
                Unit::Bytes,
            ));
        }
        out
    }

    fn collect(&mut self, now_ms: i64) -> Result<Vec<Reading>, CollectError> {
        self.disks.refresh(true);
        if let Ok(pdh) = &self.pdh {
            let _ = pdh.collect();
        }
        let mut out = vec![
            Reading::new("disk.busy", now_ms, self.counter(&self.busy, true)),
            Reading::new("disk.read", now_ms, self.counter(&self.read, false)),
            Reading::new("disk.write", now_ms, self.counter(&self.write, false)),
        ];
        for disk in self.disks.list() {
            let id = Self::volume_id(&disk.mount_point().to_string_lossy());
            let total = disk.total_space();
            let used = total.saturating_sub(disk.available_space());
            let pct = if total == 0 {
                Availability::unavailable("o volume não informou a capacidade")
            } else {
                Availability::ok(used as f64 / total as f64 * 100.0)
            };
            out.push(Reading::new(format!("disk.{id}.used_pct"), now_ms, pct));
            out.push(Reading::new(
                format!("disk.{id}.used"),
                now_ms,
                Availability::ok(used as f64),
            ));
            out.push(Reading::new(
                format!("disk.{id}.total"),
                now_ms,
                Availability::ok(total as f64),
            ));
        }
        Ok(out)
    }
}
