//! Collection runtime: runs collectors independently of any window. Never imports tauri.
//!
//! Each collector lives on its own worker thread. A cycle ([`Engine::tick`]) asks every
//! collector for readings in parallel and waits at most `timeout` for all of them. A collector
//! that errors, panics or hangs only marks its own sensors; the others keep reporting, and a
//! failed collector is asked again on the next cycle, so it recovers by itself.

mod alerts;
mod engine;
mod host;
mod session;

pub use alerts::{ActionDispatcher, AlertService, Notification, RecordingDispatcher};
pub use engine::{Engine, Health, Runtime};
pub use session::SessionStats;
