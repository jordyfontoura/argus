use crate::{Reading, SensorMeta};
use std::fmt;

/// Why a whole collector failed this cycle. Individual sensors that merely cannot be read
/// are reported through `Availability` instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectError(pub String);

impl fmt::Display for CollectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CollectError {}

/// A source of readings (CPU, memory, GPU...). Real implementations live in
/// `argus-sensors-win`; synthetic ones in `argus-sensors-fake`.
pub trait Collector: Send {
    /// Stable name used in logs and failure reports.
    fn name(&self) -> &str;

    /// The sensors this collector may report, with their metadata.
    fn sensors(&self) -> Vec<SensorMeta>;

    /// Reads every sensor once. `now_ms` is the cycle timestamp to stamp the readings with.
    fn collect(&mut self, now_ms: i64) -> Result<Vec<Reading>, CollectError>;
}
