//! Pure domain types for Argus. No OS or UI dependencies live here.
//!
//! All values are canonical: °C, bytes, Mbps, milliseconds since the Unix epoch (UTC).
//! The UI converts units; windows and durations are always computed from timestamps.

mod clock;
mod collector;
mod info;
mod report;
mod sensor;
mod series;
mod threshold;

pub use clock::{Clock, FakeClock, SystemClock};
pub use collector::{CollectError, Collector};
pub use info::{
    CacheInfo, CpuDetails, DiskInfo, Field, GpuDetails, MemoryDetails, MemoryModule,
    NetworkAdapter, ProcessRow, SmartAttribute, StaticInfo, SystemDetails, VolumeInfo,
};
pub use report::{build_report, ReportInput};
pub use sensor::{Availability, Reading, SensorId, SensorMeta, Unit};
pub use series::{Sample, SeriesStore};
pub use threshold::{Level, Threshold, ThresholdRegistry};
