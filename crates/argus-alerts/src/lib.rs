//! Alert rules, events and the engine that turns readings into events.
//! Pure: no OS, no UI, no clock of its own.

mod engine;
mod event;
mod preset;
mod rule;
mod sim;

pub use engine::{should_notify, time_weighted_average, Engine, RulePhase, RuleStatus};
pub use event::{AlertEvent, EventKind, Severity};
pub use preset::{
    apply, plan, presets, tjmax_for, MachineProfile, PlannedRule, PresetInfo, Sensitivity,
};
pub use rule::{Actions, Mode, Operator, Rule, RuleError};
pub use sim::{simulate, SimResult};
