use argus_core::{Availability, Collector, Reading, SensorMeta};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

pub(crate) enum Outcome {
    Readings(Vec<Reading>),
    Failed(String),
}

/// State of a collector as of the last cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Health {
    Healthy,
    Failed(String),
    NoResponse { since_ms: i64 },
}

/// One collector with its worker thread. The collector is never touched from the engine thread,
/// so a hang or panic inside it cannot stall or crash the engine.
pub(crate) struct Host {
    pub(crate) name: String,
    pub(crate) sensors: Vec<SensorMeta>,
    requests: Sender<i64>,
    results: Receiver<Outcome>,
    pending_since: Option<i64>,
    pub(crate) health: Health,
}

impl Host {
    pub(crate) fn spawn(mut collector: Box<dyn Collector>) -> Self {
        let name = collector.name().to_string();
        let sensors = collector.sensors();
        let (requests, request_rx) = channel::<i64>();
        let (result_tx, results) = channel::<Outcome>();
        let thread_name = format!("argus-collector-{name}");
        thread::Builder::new()
            .name(thread_name)
            .spawn(move || {
                while let Ok(now_ms) = request_rx.recv() {
                    let outcome = match catch_unwind(AssertUnwindSafe(|| collector.collect(now_ms)))
                    {
                        Ok(Ok(readings)) => Outcome::Readings(readings),
                        Ok(Err(e)) => Outcome::Failed(e.to_string()),
                        Err(_) => Outcome::Failed("o coletor falhou inesperadamente".to_string()),
                    };
                    if result_tx.send(outcome).is_err() {
                        break;
                    }
                }
            })
            .expect("spawn collector thread");
        Self {
            name,
            sensors,
            requests,
            results,
            pending_since: None,
            health: Health::Healthy,
        }
    }

    /// Asks for a new reading unless the previous request is still running.
    pub(crate) fn request(&mut self, now_ms: i64) {
        if self.pending_since.is_none() && self.requests.send(now_ms).is_ok() {
            self.pending_since = Some(now_ms);
        }
    }

    /// Waits up to `wait` for the answer and turns whatever happened into readings for every
    /// sensor of this collector.
    pub(crate) fn finish(&mut self, now_ms: i64, wait: Duration) -> Vec<Reading> {
        let received = if wait.is_zero() {
            self.results.try_recv().map_err(|e| match e {
                std::sync::mpsc::TryRecvError::Empty => RecvTimeoutError::Timeout,
                std::sync::mpsc::TryRecvError::Disconnected => RecvTimeoutError::Disconnected,
            })
        } else {
            self.results.recv_timeout(wait)
        };
        match received {
            Ok(Outcome::Readings(readings)) => {
                self.pending_since = None;
                self.health = Health::Healthy;
                readings
            }
            Ok(Outcome::Failed(reason)) => {
                self.pending_since = None;
                self.health = Health::Failed(reason.clone());
                self.fill(now_ms, Availability::unavailable(reason))
            }
            Err(RecvTimeoutError::Timeout) => {
                let since_ms = self.pending_since.unwrap_or(now_ms);
                self.health = Health::NoResponse { since_ms };
                self.fill(now_ms, Availability::NoResponse { since_ms })
            }
            Err(RecvTimeoutError::Disconnected) => {
                let reason = "o coletor foi encerrado".to_string();
                self.health = Health::Failed(reason.clone());
                self.fill(now_ms, Availability::unavailable(reason))
            }
        }
    }

    pub(crate) fn is_pending(&self) -> bool {
        self.pending_since.is_some()
    }

    fn fill(&self, now_ms: i64, value: Availability) -> Vec<Reading> {
        self.sensors
            .iter()
            .map(|s| Reading::new(s.id.clone(), now_ms, value.clone()))
            .collect()
    }
}
