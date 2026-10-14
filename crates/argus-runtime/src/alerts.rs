//! Alerts inside the runtime: readings go through the rule engine on the runtime's own thread,
//! every event is written to the history, and actions go out through an [`ActionDispatcher`].
//! Nothing here knows about windows, so alerts keep working with the UI hidden or closed.

use argus_alerts::{should_notify, Actions, AlertEvent, Engine, EventKind, Rule, RuleStatus};
use argus_core::Reading;
use argus_store::{HistoryError, HistoryFilter, HistoryStore, StoredEvent};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// What the dispatcher is asked to show: the event plus the actions that survived muting.
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub event: AlertEvent,
    pub actions: Actions,
}

/// Turns notifications into something the user sees or hears (on-screen toast, sound, Windows
/// notification). Implemented by the app; tests use [`RecordingDispatcher`].
pub trait ActionDispatcher: Send + Sync {
    fn dispatch(&self, notification: &Notification);
}

/// Test double that remembers every notification.
#[derive(Default)]
pub struct RecordingDispatcher {
    seen: Mutex<Vec<Notification>>,
}

impl RecordingDispatcher {
    pub fn notifications(&self) -> Vec<Notification> {
        self.seen.lock().expect("recording lock").clone()
    }
}

impl ActionDispatcher for RecordingDispatcher {
    fn dispatch(&self, notification: &Notification) {
        self.seen
            .lock()
            .expect("recording lock")
            .push(notification.clone());
    }
}

struct Inner {
    engine: Engine,
    history: HistoryStore,
    muted_until_ms: i64,
    critical_bypasses_mute: bool,
    subscribers: Vec<Sender<AlertEvent>>,
}

/// Shared handle: the runtime feeds it readings, the UI layer reads state and sends commands.
pub struct AlertService {
    inner: Mutex<Inner>,
    dispatcher: Arc<dyn ActionDispatcher>,
}

impl AlertService {
    pub fn new(
        rules: Vec<Rule>,
        history: HistoryStore,
        dispatcher: Arc<dyn ActionDispatcher>,
    ) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner {
                engine: Engine::new(rules),
                history,
                muted_until_ms: 0,
                critical_bypasses_mute: true,
                subscribers: Vec::new(),
            }),
            dispatcher,
        })
    }

    /// Feeds one collection cycle to the rules. Called by the runtime after every cycle.
    pub fn observe(&self, batch: &[Reading]) {
        let mut inner = self.inner.lock().expect("alerts lock");
        let mut events = Vec::new();
        for r in batch {
            events.extend(
                inner
                    .engine
                    .on_reading(r.ts_ms, r.sensor.as_str(), &r.value),
            );
        }
        let now = batch.iter().map(|r| r.ts_ms).max().unwrap_or(0);
        self.publish(&mut inner, now, events);
    }

    /// Registers the service with the runtime engine.
    pub fn attach(self: &Arc<Self>, engine: &crate::Engine) {
        let me = Arc::clone(self);
        engine.add_observer(move |batch| me.observe(batch));
    }

    fn publish(&self, inner: &mut Inner, now_ms: i64, events: Vec<AlertEvent>) {
        if events.is_empty() {
            return;
        }
        // One transaction per cycle: a storm of events costs one disk sync, not one each.
        if let Err(e) = inner.history.append_batch(&events) {
            eprintln!("argus: could not record alert events: {e}");
        }
        let muted = now_ms < inner.muted_until_ms;
        for event in &events {
            if let Some(n) = self.notification_for(inner, event, muted) {
                self.dispatcher.dispatch(&n);
            }
        }
        inner
            .subscribers
            .retain(|tx| events.iter().all(|e| tx.send(e.clone()).is_ok()));
    }

    /// Which actions an event triggers. Real events use the rule's actions; "Simular" events only
    /// ever show on screen. Muting removes everything except critical rules when they bypass it.
    fn notification_for(
        &self,
        inner: &Inner,
        event: &AlertEvent,
        muted: bool,
    ) -> Option<Notification> {
        let rule = inner.engine.rules().find(|r| r.id == event.rule_id)?;
        let mut actions = match event.kind {
            EventKind::Fired | EventKind::Repeat => rule.actions,
            // Coming back to normal is only worth a quiet on-screen note.
            EventKind::Resolved => Actions {
                toast: rule.actions.toast,
                sound: false,
                notify: false,
            },
            EventKind::Ended | EventKind::Note => return None,
        };
        if event.is_test {
            actions = Actions {
                toast: true,
                sound: false,
                notify: false,
            };
        }
        if !should_notify(event.severity, muted, inner.critical_bypasses_mute) {
            return None;
        }
        (actions.toast || actions.sound || actions.notify).then(|| Notification {
            event: event.clone(),
            actions,
        })
    }

    /// Events as they happen (for the UI's live badge and toasts).
    pub fn subscribe(&self) -> Receiver<AlertEvent> {
        let (tx, rx) = channel();
        self.inner.lock().expect("alerts lock").subscribers.push(tx);
        rx
    }

    pub fn set_rules(&self, now_ms: i64, rules: Vec<Rule>) {
        let mut inner = self.inner.lock().expect("alerts lock");
        let events = inner.engine.set_rules(now_ms, rules);
        self.publish(&mut inner, now_ms, events);
    }

    pub fn rules(&self) -> Vec<Rule> {
        self.inner
            .lock()
            .expect("alerts lock")
            .engine
            .rules()
            .cloned()
            .collect()
    }

    pub fn acknowledge(&self, rule_id: u64) -> bool {
        self.inner
            .lock()
            .expect("alerts lock")
            .engine
            .acknowledge(rule_id)
    }

    pub fn status(&self, rule_id: u64, now_ms: i64) -> Option<RuleStatus> {
        self.inner
            .lock()
            .expect("alerts lock")
            .engine
            .status(rule_id, now_ms)
    }

    pub fn statuses(&self, now_ms: i64) -> Vec<RuleStatus> {
        let inner = self.inner.lock().expect("alerts lock");
        inner
            .engine
            .rules()
            .filter_map(|r| inner.engine.status(r.id, now_ms))
            .collect()
    }

    /// Silences notifications until `until_ms` (0 clears the mute).
    pub fn mute_until(&self, until_ms: i64) {
        self.inner.lock().expect("alerts lock").muted_until_ms = until_ms;
    }

    pub fn muted_until(&self) -> i64 {
        self.inner.lock().expect("alerts lock").muted_until_ms
    }

    pub fn set_critical_bypasses_mute(&self, on: bool) {
        self.inner
            .lock()
            .expect("alerts lock")
            .critical_bypasses_mute = on;
    }

    pub fn history(&self, filter: &HistoryFilter) -> Result<Vec<StoredEvent>, HistoryError> {
        self.inner
            .lock()
            .expect("alerts lock")
            .history
            .query(filter)
    }

    pub fn history_count(&self, filter: &HistoryFilter) -> Result<u64, HistoryError> {
        self.inner
            .lock()
            .expect("alerts lock")
            .history
            .count(filter)
    }

    pub fn clear_history(&self) -> Result<usize, HistoryError> {
        self.inner.lock().expect("alerts lock").history.clear()
    }

    pub fn apply_retention(&self, now_ms: i64, days: u32) -> Result<usize, HistoryError> {
        self.inner
            .lock()
            .expect("alerts lock")
            .history
            .apply_retention(now_ms, days)
    }

    /// "Simular": runs the rule on a synthetic series ending at `now_ms`. Real rule state and real
    /// series are untouched; the events are recorded flagged as tests and only ever toast.
    pub fn simulate(&self, rule: &Rule, now_ms: i64) -> argus_alerts::SimResult {
        let result = argus_alerts::simulate(rule, now_ms);
        let mut inner = self.inner.lock().expect("alerts lock");
        self.publish_sim(&mut inner, now_ms, result.events.clone(), rule);
        result
    }

    fn publish_sim(&self, inner: &mut Inner, now_ms: i64, events: Vec<AlertEvent>, rule: &Rule) {
        if let Err(e) = inner.history.append_batch(&events) {
            eprintln!("argus: could not record simulated events: {e}");
        }
        let muted = now_ms < inner.muted_until_ms;
        for event in &events {
            // The rule may not be saved yet (editor preview), so build the notification from it.
            if !matches!(
                event.kind,
                EventKind::Fired | EventKind::Repeat | EventKind::Resolved
            ) {
                continue;
            }
            if should_notify(event.severity, muted, inner.critical_bypasses_mute)
                && rule.actions.toast
            {
                self.dispatcher.dispatch(&Notification {
                    event: event.clone(),
                    actions: Actions {
                        toast: true,
                        sound: false,
                        notify: false,
                    },
                });
            }
        }
        inner
            .subscribers
            .retain(|tx| events.iter().all(|e| tx.send(e.clone()).is_ok()));
    }

    /// Records an event that did not come from the rule engine (used by "Simular" and tests).
    pub fn record(&self, event: AlertEvent) {
        let mut inner = self.inner.lock().expect("alerts lock");
        let now = event.ts_ms;
        self.publish(&mut inner, now, vec![event]);
    }
}
