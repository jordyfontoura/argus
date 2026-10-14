//! What happens when an alert fires: an on-screen toast, a sound played by Rust (so it works with
//! the window hidden) and a native Windows notification. The three outputs are small traits so
//! the routing can be tested without a screen or a speaker.

use crate::alerts_api::toast_for;
use argus_alerts::Severity;
use argus_api::ToastPayload;
use argus_runtime::{ActionDispatcher, Notification};
use std::sync::Arc;

pub trait Screen: Send + Sync {
    fn toast(&self, payload: ToastPayload);
}

pub trait Sound: Send + Sync {
    fn play(&self, severity: Severity);
}

pub trait Notifier: Send + Sync {
    fn show(&self, title: &str, body: &str);
}

/// Sends each requested action of a notification to its output.
pub struct ActionRouter {
    pub screen: Arc<dyn Screen>,
    pub sound: Arc<dyn Sound>,
    pub notifier: Arc<dyn Notifier>,
}

const MAX_TITLE: usize = 80;
const MAX_BODY: usize = 200;

/// Text for the Windows notification: control characters removed (a rule name can contain
/// anything) and length capped. The notification API builds its XML through the DOM, so the text
/// is not escaped here; escaping it twice would show `&lt;` to the user.
pub fn notification_text(n: &Notification) -> (String, String) {
    let clean = |s: &str, max: usize| -> String {
        let text: String = s
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.chars().count() > max {
            let cut: String = text.chars().take(max - 1).collect();
            format!("{cut}…")
        } else {
            text
        }
    };
    let prefix = match n.event.severity {
        Severity::Crit => "⚑ ",
        _ => "",
    };
    (
        clean(&format!("{prefix}{}", n.event.rule_name), MAX_TITLE),
        clean(&n.event.detail, MAX_BODY),
    )
}

impl ActionDispatcher for ActionRouter {
    fn dispatch(&self, n: &Notification) {
        if n.actions.toast {
            self.screen.toast(toast_for(n));
        }
        if n.actions.sound {
            self.sound.play(n.event.severity);
        }
        if n.actions.notify {
            let (title, body) = notification_text(n);
            self.notifier.show(&title, &body);
        }
    }
}

// ---------- real outputs ----------

use argus_api::events;
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter};

/// Emits `toast` to the window. If no window is listening the event is simply dropped: the alert
/// is already in the history and the other outputs still fire.
#[derive(Default)]
pub struct TauriScreen {
    pub handle: OnceLock<AppHandle>,
}

impl Screen for TauriScreen {
    fn toast(&self, payload: ToastPayload) {
        if let Some(handle) = self.handle.get() {
            let _ = handle.emit(events::TOAST, payload);
        }
    }
}

/// A short tone from Windows itself, played on its own thread so a cycle never waits for it.
pub struct WinSound;

impl Sound for WinSound {
    fn play(&self, severity: Severity) {
        let (hz, ms) = match severity {
            Severity::Crit => (880, 220),
            Severity::Warn => (660, 180),
            Severity::Info => (520, 140),
        };
        eprintln!("argus: som ({})", severity.as_str());
        std::thread::spawn(move || {
            // SAFETY: Beep takes plain integers and has no memory-safety requirements.
            let _ = unsafe { windows::Win32::System::Diagnostics::Debug::Beep(hz, ms) };
        });
    }
}

/// Native Windows notification through the Tauri notification plugin.
#[derive(Default)]
pub struct TauriNotifier {
    pub handle: OnceLock<AppHandle>,
}

impl Notifier for TauriNotifier {
    fn show(&self, title: &str, body: &str) {
        use tauri_plugin_notification::NotificationExt;
        eprintln!("argus: notificação do Windows: {title}");
        if let Some(handle) = self.handle.get() {
            if let Err(e) = handle
                .notification()
                .builder()
                .title(title)
                .body(body)
                .show()
            {
                eprintln!("argus: could not show the Windows notification: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_alerts::{Actions, AlertEvent, EventKind};
    use std::sync::Mutex;

    #[derive(Default)]
    struct Fake {
        toasts: Mutex<Vec<ToastPayload>>,
        sounds: Mutex<Vec<Severity>>,
        notes: Mutex<Vec<(String, String)>>,
    }
    impl Screen for Fake {
        fn toast(&self, p: ToastPayload) {
            self.toasts.lock().unwrap().push(p);
        }
    }
    impl Sound for Fake {
        fn play(&self, s: Severity) {
            self.sounds.lock().unwrap().push(s);
        }
    }
    impl Notifier for Fake {
        fn show(&self, t: &str, b: &str) {
            self.notes
                .lock()
                .unwrap()
                .push((t.to_string(), b.to_string()));
        }
    }

    fn router() -> (ActionRouter, Arc<Fake>) {
        let fake = Arc::new(Fake::default());
        (
            ActionRouter {
                screen: fake.clone(),
                sound: fake.clone(),
                notifier: fake.clone(),
            },
            fake,
        )
    }

    fn notification(
        name: &str,
        detail: &str,
        actions: Actions,
        sev: Severity,
        is_test: bool,
    ) -> Notification {
        Notification {
            event: AlertEvent {
                ts_ms: 1,
                rule_id: 1,
                rule_name: name.into(),
                sensor: "cpu.temp".into(),
                severity: sev,
                kind: EventKind::Fired,
                detail: detail.into(),
                value: Some(95.0),
                is_test,
            },
            actions,
        }
    }

    #[test]
    fn each_requested_action_goes_to_its_output_and_only_those() {
        let (r, f) = router();
        r.dispatch(&notification(
            "CPU",
            "quente",
            Actions {
                toast: true,
                sound: true,
                notify: true,
            },
            Severity::Crit,
            false,
        ));
        assert_eq!(f.toasts.lock().unwrap().len(), 1);
        assert_eq!(*f.sounds.lock().unwrap(), vec![Severity::Crit]);
        assert_eq!(f.notes.lock().unwrap().len(), 1);

        let (r, f) = router();
        r.dispatch(&notification(
            "CPU",
            "quente",
            Actions {
                toast: false,
                sound: true,
                notify: false,
            },
            Severity::Warn,
            false,
        ));
        assert!(f.toasts.lock().unwrap().is_empty());
        assert_eq!(*f.sounds.lock().unwrap(), vec![Severity::Warn]);
        assert!(f.notes.lock().unwrap().is_empty());
    }

    #[test]
    fn the_windows_notification_text_is_clean_and_bounded() {
        let n = notification(
            &format!("regra\u{0}\n<img src=x onerror=1> {}", "x".repeat(200)),
            &format!("linha1\r\nlinha2\t{}", "y".repeat(500)),
            Actions {
                toast: false,
                sound: false,
                notify: true,
            },
            Severity::Warn,
            false,
        );
        let (title, body) = notification_text(&n);
        assert!(title.chars().count() <= 80 && body.chars().count() <= 200);
        assert!(title.ends_with('…') && body.ends_with('…'));
        assert!(!title.chars().any(char::is_control) && !body.chars().any(char::is_control));
        // Not escaped here: the notification API builds its XML through the DOM.
        assert!(title.contains("<img src=x onerror=1>"));
        assert!(body.starts_with("linha1 linha2 "));
    }

    #[test]
    fn critical_notifications_are_marked_in_the_title() {
        let n = notification(
            "CPU",
            "d",
            Actions {
                toast: false,
                sound: false,
                notify: true,
            },
            Severity::Crit,
            false,
        );
        assert_eq!(notification_text(&n).0, "⚑ CPU");
        let n = notification(
            "CPU",
            "d",
            Actions {
                toast: false,
                sound: false,
                notify: true,
            },
            Severity::Warn,
            false,
        );
        assert_eq!(notification_text(&n).0, "CPU");
    }

    #[test]
    fn the_on_screen_toast_carries_the_test_prefix_for_simulations() {
        let (r, f) = router();
        r.dispatch(&notification(
            "CPU",
            "d",
            Actions {
                toast: true,
                sound: false,
                notify: false,
            },
            Severity::Crit,
            true,
        ));
        assert_eq!(f.toasts.lock().unwrap()[0].title, "[teste] CPU");
    }
}
