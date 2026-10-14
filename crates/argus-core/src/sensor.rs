use serde::{Deserialize, Serialize};
use std::fmt;

/// Stable identifier of a sensor, for example `cpu.load` or `gpu.0.temp`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SensorId(String);

impl SensorId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SensorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for SensorId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for SensorId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// What a sensor reported. A failure is never a zero and never an omission.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Availability {
    /// A real value in the sensor's canonical unit.
    Ok { value: f64 },
    /// The sensor cannot be read on this machine, with the reason shown to the user.
    Unavailable { reason: String },
    /// The collector did not answer in time; `since_ms` is when the silence started.
    NoResponse { since_ms: i64 },
}

impl Availability {
    pub fn ok(value: f64) -> Self {
        Self::Ok { value }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
        }
    }

    pub fn value(&self) -> Option<f64> {
        match self {
            Self::Ok { value } => Some(*value),
            _ => None,
        }
    }
}

/// One timestamped reading of one sensor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Reading {
    pub sensor: SensorId,
    pub ts_ms: i64,
    pub value: Availability,
}

impl Reading {
    pub fn new(sensor: impl Into<SensorId>, ts_ms: i64, value: Availability) -> Self {
        Self {
            sensor: sensor.into(),
            ts_ms,
            value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Percent,
    Celsius,
    Megahertz,
    Watt,
    Volt,
    Rpm,
    Bytes,
    BytesPerSecond,
    Mbps,
    Milliseconds,
    /// A plain number or a 0/1 flag.
    Count,
}

impl Unit {
    /// Short label used in exports ("%", "°C", "MHz"...).
    pub fn label(self) -> &'static str {
        match self {
            Self::Percent => "%",
            Self::Celsius => "°C",
            Self::Megahertz => "MHz",
            Self::Watt => "W",
            Self::Volt => "V",
            Self::Rpm => "RPM",
            Self::Bytes => "B",
            Self::BytesPerSecond => "B/s",
            Self::Mbps => "Mbps",
            Self::Milliseconds => "ms",
            Self::Count => "",
        }
    }
}

/// Catalog entry describing a sensor (what `list_sensors` returns to the UI).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SensorMeta {
    pub id: SensorId,
    pub group: String,
    pub name: String,
    pub unit: Unit,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn availability_serializes_with_a_kind_tag() {
        let json = serde_json::to_string(&Availability::unavailable("sem driver")).unwrap();
        assert_eq!(json, r#"{"kind":"unavailable","reason":"sem driver"}"#);
        let json = serde_json::to_string(&Availability::NoResponse { since_ms: 5 }).unwrap();
        assert_eq!(json, r#"{"kind":"no_response","since_ms":5}"#);
    }

    #[test]
    fn only_ok_has_a_value() {
        assert_eq!(Availability::ok(3.0).value(), Some(3.0));
        assert_eq!(Availability::unavailable("x").value(), None);
        assert_eq!(Availability::NoResponse { since_ms: 0 }.value(), None);
    }

    #[test]
    fn sensor_id_is_a_plain_string_in_json() {
        let json = serde_json::to_string(&SensorId::new("cpu.load")).unwrap();
        assert_eq!(json, r#""cpu.load""#);
    }
}
