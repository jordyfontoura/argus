//! Commands around alert rules: validation, persistence and the live state. Free of Tauri types.

use argus_alerts::{
    Actions, MachineProfile, PlannedRule, PresetInfo, Rule, Sensitivity, SimResult,
};
use argus_api::{AlertsState, ApiError, AppSettings, HistoryPage, HistoryQuery, ToastPayload};
use argus_core::{Clock, SystemClock};
use argus_runtime::{AlertService, Notification};
use argus_store::{Config, ConfigStore, HistoryFilter, PresetChoice};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// Longest mute the UI may ask for (seconds).
const MAX_MUTE_SEC: u32 = 24 * 3600;

pub struct AlertsApi {
    service: Arc<AlertService>,
    store: ConfigStore,
    config: Mutex<Config>,
    clock: Arc<dyn Clock>,
}

impl AlertsApi {
    pub fn new(service: Arc<AlertService>, store: ConfigStore, config: Config) -> Self {
        Self::with_clock(service, store, config, Arc::new(SystemClock))
    }

    pub fn with_clock(
        service: Arc<AlertService>,
        store: ConfigStore,
        config: Config,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            service,
            store,
            config: Mutex::new(config),
            clock,
        }
    }

    pub fn service(&self) -> &Arc<AlertService> {
        &self.service
    }

    pub fn list_rules(&self) -> Vec<Rule> {
        self.config.lock().expect("config lock").rules.clone()
    }

    fn persist(&self, config: &Config) -> Result<(), ApiError> {
        self.store.save(config).map_err(|e| {
            ApiError::new(
                "save_failed",
                format!("não foi possível salvar as regras: {e}"),
            )
        })
    }

    /// Creates (id 0) or replaces a rule. The saved rule is returned with its final id.
    pub fn save_rule(&self, mut rule: Rule) -> Result<Rule, ApiError> {
        rule.validate()
            .map_err(|e| ApiError::new("invalid_rule", e.to_string()))?;
        rule.name = rule.name.trim().to_string();
        let mut config = self.config.lock().expect("config lock");
        let mut next = config.clone();
        if rule.id == 0 {
            rule.id = next.next_rule_id;
            next.next_rule_id += 1;
            next.rules.push(rule.clone());
        } else {
            match next.rules.iter_mut().find(|r| r.id == rule.id) {
                Some(slot) => *slot = rule.clone(),
                None => {
                    return Err(ApiError::new(
                        "unknown_rule",
                        "a regra não existe mais; recarregue a lista",
                    ))
                }
            }
        }
        // Persist first: if the disk refuses, nothing changes in memory or in the engine.
        self.persist(&next)?;
        *config = next;
        self.service
            .set_rules(self.clock.now_ms(), config.rules.clone());
        Ok(rule)
    }

    pub fn delete_rule(&self, id: u64) -> Result<(), ApiError> {
        let mut config = self.config.lock().expect("config lock");
        if !config.rules.iter().any(|r| r.id == id) {
            return Err(ApiError::new("unknown_rule", "a regra não existe mais"));
        }
        let mut next = config.clone();
        next.rules.retain(|r| r.id != id);
        self.persist(&next)?;
        *config = next;
        self.service
            .set_rules(self.clock.now_ms(), config.rules.clone());
        Ok(())
    }

    pub fn acknowledge_rule(&self, id: u64) -> bool {
        self.service.acknowledge(id)
    }

    /// Runs the rule against a synthetic series. Needs a valid rule but not a saved one, so the
    /// editor can test before saving.
    pub fn simulate_rule(&self, rule: Rule) -> Result<SimResult, ApiError> {
        rule.validate()
            .map_err(|e| ApiError::new("invalid_rule", e.to_string()))?;
        Ok(self.service.simulate(&rule, self.clock.now_ms()))
    }

    pub fn set_mute(&self, seconds: u32) -> Result<AlertsState, ApiError> {
        if seconds > MAX_MUTE_SEC {
            return Err(ApiError::new(
                "invalid_mute",
                "o silêncio pode durar no máximo 24 horas",
            ));
        }
        let until = if seconds == 0 {
            0
        } else {
            self.clock.now_ms() + i64::from(seconds) * 1000
        };
        self.service.mute_until(until);
        Ok(self.state())
    }

    pub fn favorites(&self) -> Vec<String> {
        self.config.lock().expect("config lock").favorites.clone()
    }

    /// Pins or unpins a sensor. Pinned sensors keep their order of pinning and survive restarts.
    pub fn set_favorite(&self, sensor: &str, on: bool) -> Result<Vec<String>, ApiError> {
        if sensor.trim().is_empty() || sensor.len() > 120 {
            return Err(ApiError::new("invalid_sensor", "sensor inválido"));
        }
        let mut config = self.config.lock().expect("config lock");
        let mut next = config.clone();
        next.favorites.retain(|s| s != sensor);
        if on {
            next.favorites.push(sensor.to_string());
        }
        self.persist(&next)?;
        *config = next;
        Ok(config.favorites.clone())
    }

    pub fn settings(&self) -> AppSettings {
        let s = &self.config.lock().expect("config lock").settings;
        AppSettings {
            sample_interval_ms: s.sample_interval_ms,
            temperature_unit: s.temperature_unit.clone(),
            memory_unit: s.memory_unit.clone(),
            light_theme: s.light_theme,
            history_retention_days: s.history_retention_days,
        }
    }

    /// Validates and saves the display and collection settings.
    pub fn update_settings(&self, new: AppSettings) -> Result<AppSettings, ApiError> {
        if ![500, 1000, 2000, 5000].contains(&new.sample_interval_ms) {
            return Err(ApiError::new(
                "invalid_settings",
                "o intervalo de coleta deve ser 500 ms, 1 s, 2 s ou 5 s",
            ));
        }
        if !["C", "F"].contains(&new.temperature_unit.as_str()) {
            return Err(ApiError::new(
                "invalid_settings",
                "a unidade de temperatura deve ser °C ou °F",
            ));
        }
        if !["GB", "GiB"].contains(&new.memory_unit.as_str()) {
            return Err(ApiError::new(
                "invalid_settings",
                "a unidade de memória deve ser GB ou GiB",
            ));
        }
        if !(1..=3650).contains(&new.history_retention_days) {
            return Err(ApiError::new(
                "invalid_settings",
                "a retenção deve ficar entre 1 e 3650 dias",
            ));
        }
        {
            let mut config = self.config.lock().expect("config lock");
            let mut next = config.clone();
            next.settings.sample_interval_ms = new.sample_interval_ms;
            next.settings.temperature_unit = new.temperature_unit.clone();
            next.settings.memory_unit = new.memory_unit.clone();
            next.settings.light_theme = new.light_theme;
            next.settings.history_retention_days = new.history_retention_days;
            self.persist(&next)?;
            *config = next;
        }
        self.service
            .apply_retention(self.clock.now_ms(), new.history_retention_days)
            .map_err(Self::history_error)?;
        Ok(self.settings())
    }

    pub fn minimize_to_tray(&self) -> bool {
        self.config
            .lock()
            .expect("config lock")
            .settings
            .minimize_to_tray
    }

    /// Saves the background options (the autostart flag is also the registry's job, see main).
    pub fn save_background(
        &self,
        autostart: Option<bool>,
        minimize_to_tray: Option<bool>,
    ) -> Result<(), ApiError> {
        let mut config = self.config.lock().expect("config lock");
        let mut next = config.clone();
        if let Some(on) = autostart {
            next.settings.autostart = on;
        }
        if let Some(on) = minimize_to_tray {
            next.settings.minimize_to_tray = on;
        }
        self.persist(&next)?;
        *config = next;
        Ok(())
    }

    /// Whether critical rules still notify while muted. Saved with the settings.
    pub fn set_critical_bypass(&self, on: bool) -> Result<AlertsState, ApiError> {
        {
            let mut config = self.config.lock().expect("config lock");
            let mut next = config.clone();
            next.settings.critical_bypasses_mute = on;
            self.persist(&next)?;
            *config = next;
        }
        self.service.set_critical_bypasses_mute(on);
        Ok(self.state())
    }

    pub fn list_presets(&self) -> Vec<PresetInfo> {
        argus_alerts::presets()
    }

    fn check_preset(preset: &str) -> Result<(), ApiError> {
        if argus_alerts::presets().iter().any(|p| p.key == preset) {
            Ok(())
        } else {
            Err(ApiError::new(
                "unknown_preset",
                "perfil de proteção desconhecido",
            ))
        }
    }

    /// What a preset would create on this machine (including what it must skip, and why).
    pub fn preview_preset(
        &self,
        profile: &MachineProfile,
        preset: &str,
        sensitivity: Sensitivity,
        actions: Option<Actions>,
    ) -> Result<Vec<PlannedRule>, ApiError> {
        Self::check_preset(preset)?;
        Ok(argus_alerts::plan(preset, sensitivity, profile, actions).unwrap_or_default())
    }

    /// Replaces the earlier preset's rules with the chosen ones of this preset.
    pub fn apply_preset(
        &self,
        profile: &MachineProfile,
        preset: &str,
        sensitivity: Sensitivity,
        actions: Option<Actions>,
        chosen: &[String],
        keep_own: bool,
    ) -> Result<Vec<Rule>, ApiError> {
        let planned = self.preview_preset(profile, preset, sensitivity, actions)?;
        let chosen: HashSet<String> = chosen.iter().cloned().collect();
        let mut config = self.config.lock().expect("config lock");
        let mut next = config.clone();
        let mut rules = argus_alerts::apply(&next.rules, &planned, &chosen, keep_own);
        for rule in &mut rules {
            if rule.id == 0 {
                rule.id = next.next_rule_id;
                next.next_rule_id += 1;
            }
            rule.validate()
                .map_err(|e| ApiError::new("invalid_rule", format!("{}: {e}", rule.name)))?;
        }
        next.rules = rules;
        next.preset = Some(PresetChoice {
            preset: preset.to_string(),
            sensitivity: sensitivity.as_str().to_string(),
        });
        self.persist(&next)?;
        *config = next;
        self.service
            .set_rules(self.clock.now_ms(), config.rules.clone());
        Ok(config.rules.clone())
    }

    fn filter_of(q: &HistoryQuery) -> HistoryFilter {
        HistoryFilter {
            severity: q.severity,
            rule_id: q.rule_id,
            kinds: q.kinds.clone(),
            include_tests: q.include_tests,
            limit: q.limit,
            ..Default::default()
        }
    }

    fn history_error(e: argus_store::HistoryError) -> ApiError {
        ApiError::new(
            "history_failed",
            format!("não foi possível ler o histórico: {e}"),
        )
    }

    pub fn query_history(&self, q: &HistoryQuery) -> Result<HistoryPage, ApiError> {
        let filter = Self::filter_of(q);
        let events = self.service.history(&filter).map_err(Self::history_error)?;
        let unlimited = HistoryFilter {
            limit: None,
            ..filter.clone()
        };
        let matching = self
            .service
            .history_count(&unlimited)
            .map_err(Self::history_error)?;
        let total = self
            .service
            .history_count(&HistoryFilter {
                include_tests: q.include_tests,
                ..Default::default()
            })
            .map_err(Self::history_error)?;
        let critical_fired = self
            .service
            .history_count(&HistoryFilter {
                severity: Some(argus_alerts::Severity::Crit),
                kinds: vec![argus_alerts::EventKind::Fired],
                limit: None,
                ..unlimited
            })
            .map_err(Self::history_error)?;
        Ok(HistoryPage {
            events: events.into_iter().map(|e| e.event).collect(),
            matching,
            total,
            critical_fired,
            retention_days: self
                .config
                .lock()
                .expect("config lock")
                .settings
                .history_retention_days,
        })
    }

    /// The CSV text of every event matching the query (no limit).
    pub fn history_csv(&self, q: &HistoryQuery) -> Result<String, ApiError> {
        let filter = HistoryFilter {
            limit: None,
            ..Self::filter_of(q)
        };
        let events = self.service.history(&filter).map_err(Self::history_error)?;
        Ok(argus_store::to_csv(&events))
    }

    pub fn clear_history(&self) -> Result<u64, ApiError> {
        self.service
            .clear_history()
            .map(|n| n as u64)
            .map_err(Self::history_error)
    }

    /// How long events are kept (days); older ones are removed now and on every start.
    pub fn set_history_retention(&self, days: u32) -> Result<HistoryPage, ApiError> {
        if !(1..=3650).contains(&days) {
            return Err(ApiError::new(
                "invalid_retention",
                "a retenção deve ficar entre 1 e 3650 dias",
            ));
        }
        {
            let mut config = self.config.lock().expect("config lock");
            let mut next = config.clone();
            next.settings.history_retention_days = days;
            self.persist(&next)?;
            *config = next;
        }
        self.service
            .apply_retention(self.clock.now_ms(), days)
            .map_err(Self::history_error)?;
        self.query_history(&HistoryQuery {
            limit: Some(200),
            ..Default::default()
        })
    }

    pub fn state(&self) -> AlertsState {
        let now = self.clock.now_ms();
        let statuses = self.service.statuses(now);
        let active_count = statuses
            .iter()
            .filter(|s| {
                matches!(
                    s.phase,
                    argus_alerts::RulePhase::Firing | argus_alerts::RulePhase::Acknowledged
                )
            })
            .count() as u32;
        let (preset_label, preset_sensitivity) =
            match &self.config.lock().expect("config lock").preset {
                Some(c) => (
                    argus_alerts::presets()
                        .into_iter()
                        .find(|p| p.key == c.preset)
                        .map(|p| p.label),
                    Some(c.sensitivity.clone()),
                ),
                None => (None, None),
            };
        AlertsState {
            statuses,
            muted_until_ms: self.service.muted_until(),
            critical_bypasses_mute: self
                .config
                .lock()
                .expect("config lock")
                .settings
                .critical_bypasses_mute,
            active_count,
            preset_label,
            preset_sensitivity,
            history_count: self
                .service
                .history_count(&HistoryFilter::default())
                .unwrap_or(0),
        }
    }
}

/// The on-screen text of a notification. Simulations are always marked, so nobody mistakes
/// one for a real alert.
pub fn toast_for(n: &Notification) -> ToastPayload {
    let prefix = if n.event.is_test { "[teste] " } else { "" };
    ToastPayload {
        event: n.event.clone(),
        title: format!("{prefix}{}", n.event.rule_name),
        body: n.event.detail.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_alerts::{Actions, Mode, Operator, RulePhase, Severity};
    use argus_core::FakeClock;
    use argus_runtime::RecordingDispatcher;
    use argus_store::HistoryStore;

    fn rule(name: &str) -> Rule {
        Rule {
            id: 0,
            name: name.into(),
            sensor: "cpu.temp".into(),
            mode: Mode::Sustained,
            op: Operator::Above,
            threshold: 85.0,
            hysteresis: None,
            for_sec: 5,
            count: 3,
            recover_sec: 2,
            repeat_sec: 0,
            severity: Severity::Warn,
            actions: Actions {
                toast: true,
                sound: false,
                notify: false,
            },
            enabled: true,
            preset: None,
        }
    }

    fn api(dir: &std::path::Path) -> (AlertsApi, FakeClock) {
        let store = ConfigStore::new(dir);
        let (config, _) = store.load();
        let service = AlertService::new(
            config.rules.clone(),
            HistoryStore::open_in_memory().unwrap(),
            Arc::new(RecordingDispatcher::default()),
        );
        let clock = FakeClock::new(5_000_000);
        (
            AlertsApi::with_clock(service, store, config, Arc::new(clock.clone())),
            clock,
        )
    }

    #[test]
    fn creating_assigns_ids_and_persists_across_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let (api1, _) = api(dir.path());
        let a = api1.save_rule(rule("primeira")).unwrap();
        let b = api1.save_rule(rule("segunda")).unwrap();
        assert_eq!((a.id, b.id), (1, 2));

        let (api2, _) = api(dir.path());
        let names: Vec<String> = api2.list_rules().into_iter().map(|r| r.name).collect();
        assert_eq!(names, vec!["primeira", "segunda"]);
        // The engine of the restarted app has them too.
        assert_eq!(api2.service().rules().len(), 2);
    }

    #[test]
    fn editing_replaces_in_place_and_unknown_ids_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let (api, _) = api(dir.path());
        let mut r = api.save_rule(rule("a")).unwrap();
        r.threshold = 90.0;
        api.save_rule(r.clone()).unwrap();
        assert_eq!(api.list_rules().len(), 1);
        assert_eq!(api.list_rules()[0].threshold, 90.0);
        r.id = 99;
        assert_eq!(api.save_rule(r).unwrap_err().code, "unknown_rule");
    }

    #[test]
    fn invalid_rules_are_refused_with_a_message_and_change_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (api, _) = api(dir.path());
        let err = api.save_rule(rule("  ")).unwrap_err();
        assert_eq!(err.code, "invalid_rule");
        assert!(err.message.contains("nome"));
        assert!(api.list_rules().is_empty());
        assert_eq!(
            api.simulate_rule(rule("")).unwrap_err().code,
            "invalid_rule"
        );
    }

    #[test]
    fn names_are_trimmed_and_html_is_kept_as_plain_text() {
        let dir = tempfile::tempdir().unwrap();
        let (api, _) = api(dir.path());
        let saved = api
            .save_rule(rule("  <img src=x onerror=alert(1)>  "))
            .unwrap();
        // The backend never interprets the name; the UI must show it as text (tested there).
        assert_eq!(saved.name, "<img src=x onerror=alert(1)>");
    }

    #[test]
    fn deleting_removes_the_rule_everywhere() {
        let dir = tempfile::tempdir().unwrap();
        let (api, _) = api(dir.path());
        let r = api.save_rule(rule("a")).unwrap();
        api.delete_rule(r.id).unwrap();
        assert!(api.list_rules().is_empty());
        assert!(api.service().rules().is_empty());
        assert_eq!(api.delete_rule(r.id).unwrap_err().code, "unknown_rule");
        let (again, _) = self::api(dir.path());
        assert!(again.list_rules().is_empty());
    }

    #[test]
    fn a_failing_disk_leaves_memory_and_engine_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let (api, _) = api(dir.path());
        api.save_rule(rule("a")).unwrap();
        // Make the config path unwritable by replacing it with a directory.
        let path = dir.path().join("config.json");
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        let err = api.save_rule(rule("b")).unwrap_err();
        assert_eq!(err.code, "save_failed");
        assert_eq!(api.list_rules().len(), 1);
        assert_eq!(api.service().rules().len(), 1);
    }

    #[test]
    fn state_reports_phases_mute_and_the_badge_count() {
        let dir = tempfile::tempdir().unwrap();
        let (api, clock) = api(dir.path());
        let r = api.save_rule(rule("a")).unwrap();
        let s = api.state();
        assert_eq!(s.statuses.len(), 1);
        assert_eq!(s.statuses[0].phase, RulePhase::Ok);
        assert_eq!(s.active_count, 0);
        assert_eq!(s.muted_until_ms, 0);

        let muted = api.set_mute(300).unwrap();
        assert_eq!(muted.muted_until_ms, clock.now_ms() + 300_000);
        assert_eq!(api.set_mute(0).unwrap().muted_until_ms, 0);
        assert_eq!(api.set_mute(999_999).unwrap_err().code, "invalid_mute");
        assert!(
            !api.acknowledge_rule(r.id),
            "not firing, nothing to acknowledge"
        );
    }

    #[test]
    fn toasts_are_marked_when_they_come_from_a_simulation() {
        let mut event = argus_alerts::simulate(&rule("x"), 1_000_000)
            .events
            .remove(0);
        let n = Notification {
            event: event.clone(),
            actions: Actions {
                toast: true,
                sound: false,
                notify: false,
            },
        };
        assert!(toast_for(&n).title.starts_with("[teste] "));
        event.is_test = false;
        let n = Notification {
            event,
            actions: n.actions,
        };
        assert_eq!(toast_for(&n).title, "x");
    }

    #[test]
    fn the_critical_bypass_is_saved_and_applied() {
        let dir = tempfile::tempdir().unwrap();
        let (api1, _) = api(dir.path());
        assert!(api1.state().critical_bypasses_mute);
        assert!(
            !api1
                .set_critical_bypass(false)
                .unwrap()
                .critical_bypasses_mute
        );
        let (api2, _) = api(dir.path());
        assert!(!api2.state().critical_bypasses_mute, "survives a restart");
    }

    fn profile(sensors: &[&str]) -> MachineProfile {
        MachineProfile {
            readable: sensors.iter().map(|s| s.to_string()).collect(),
            unreadable_reasons: Default::default(),
            tjmax: Some(95.0),
            thresholds: argus_core::ThresholdRegistry::with_defaults(),
        }
    }

    #[test]
    fn applying_a_preset_creates_numbered_tagged_rules_and_persists_the_choice() {
        let dir = tempfile::tempdir().unwrap();
        let (api1, _) = api(dir.path());
        api1.save_rule(rule("minha")).unwrap();
        let p = profile(&["cpu.temp", "gpu.0.temp", "mem.used_pct"]);
        let keys: Vec<String> = ["cpu_temp", "gpu_temp", "mem_high"]
            .map(String::from)
            .to_vec();
        let rules = api1
            .apply_preset(&p, "basic", Sensitivity::Normal, None, &keys, true)
            .unwrap();
        assert_eq!(rules.len(), 4, "the user's own rule plus 3 from the preset");
        let ids: std::collections::HashSet<u64> = rules.iter().map(|r| r.id).collect();
        assert_eq!(ids.len(), 4, "unique ids");
        assert_eq!(
            rules
                .iter()
                .filter(|r| r.preset.as_deref() == Some("basic"))
                .count(),
            3
        );
        let state = api1.state();
        assert_eq!(state.preset_label.as_deref(), Some("Proteção básica"));
        assert_eq!(state.preset_sensitivity.as_deref(), Some("normal"));

        // Survives a restart, and applying another preset replaces the first one's rules.
        let (api2, _) = api(dir.path());
        assert_eq!(api2.list_rules().len(), 4);
        let again = api2
            .apply_preset(
                &p,
                "gaming",
                Sensitivity::Strict,
                None,
                &["cpu_temp".to_string()],
                true,
            )
            .unwrap();
        assert_eq!(
            again
                .iter()
                .filter(|r| r.preset.as_deref() == Some("basic"))
                .count(),
            0
        );
        assert_eq!(
            again
                .iter()
                .filter(|r| r.preset.as_deref() == Some("gaming"))
                .count(),
            1
        );
        assert!(again.iter().any(|r| r.name == "minha"));
        assert_eq!(api2.service().rules().len(), 2);
    }

    #[test]
    fn previewing_lists_skipped_rules_with_reasons_and_unknown_presets_fail() {
        let dir = tempfile::tempdir().unwrap();
        let (api, _) = api(dir.path());
        let p = profile(&["mem.used_pct"]);
        let plan = api
            .preview_preset(&p, "basic", Sensitivity::Normal, None)
            .unwrap();
        assert_eq!(plan.len(), 4);
        assert_eq!(plan.iter().filter(|r| r.available).count(), 1);
        assert!(plan
            .iter()
            .filter(|r| !r.available)
            .all(|r| r.unavailable_reason.is_some()));
        assert_eq!(
            api.preview_preset(&p, "nope", Sensitivity::Normal, None)
                .unwrap_err()
                .code,
            "unknown_preset"
        );
        assert_eq!(api.list_presets().len(), 3);
        // Previewing changes nothing.
        assert!(api.list_rules().is_empty());
    }

    #[test]
    fn dropping_own_rules_when_asked() {
        let dir = tempfile::tempdir().unwrap();
        let (api, _) = api(dir.path());
        api.save_rule(rule("minha")).unwrap();
        let p = profile(&["mem.used_pct"]);
        let rules = api
            .apply_preset(
                &p,
                "basic",
                Sensitivity::Normal,
                None,
                &["mem_high".to_string()],
                false,
            )
            .unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "Memória quase cheia");
    }

    fn event(
        ts_ms: i64,
        rule_id: u64,
        name: &str,
        severity: argus_alerts::Severity,
        kind: argus_alerts::EventKind,
        is_test: bool,
    ) -> argus_alerts::AlertEvent {
        argus_alerts::AlertEvent {
            ts_ms,
            rule_id,
            rule_name: name.to_string(),
            sensor: "cpu.temp".into(),
            severity,
            kind,
            detail: "detalhe, com vírgula".into(),
            value: Some(91.0),
            is_test,
        }
    }

    fn seeded(dir: &std::path::Path) -> (AlertsApi, FakeClock) {
        use argus_alerts::{EventKind::*, Severity::*};
        let (api, clock) = api(dir);
        let s = api.service();
        s.record(event(1_000, 1, "CPU", Crit, Fired, false));
        s.record(event(2_000, 1, "CPU", Crit, Resolved, false));
        s.record(event(3_000, 2, "=GPU()", Warn, Fired, false));
        s.record(event(4_000, 2, "=GPU()", Warn, Fired, true));
        s.record(event(5_000, 3, "RAM", Info, Note, false));
        (api, clock)
    }

    #[test]
    fn the_history_page_reports_filters_counts_and_hides_tests_by_default() {
        let dir = tempfile::tempdir().unwrap();
        let (api, _) = seeded(dir.path());
        let page = api.query_history(&HistoryQuery::default()).unwrap();
        assert_eq!(page.events.len(), 4);
        assert_eq!((page.matching, page.total, page.critical_fired), (4, 4, 1));
        assert_eq!(page.events[0].ts_ms, 5_000, "newest first");

        let with_tests = api
            .query_history(&HistoryQuery {
                include_tests: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!((with_tests.matching, with_tests.total), (5, 5));

        let only_fired = api
            .query_history(&HistoryQuery {
                kinds: vec![argus_alerts::EventKind::Fired],
                ..Default::default()
            })
            .unwrap();
        assert_eq!((only_fired.matching, only_fired.total), (2, 4));

        let rule2 = api
            .query_history(&HistoryQuery {
                rule_id: Some(2),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(rule2.matching, 1);
        let limited = api
            .query_history(&HistoryQuery {
                limit: Some(1),
                ..Default::default()
            })
            .unwrap();
        assert_eq!((limited.events.len(), limited.matching), (1, 4));
        assert_eq!(
            api.state().history_count,
            4,
            "the menu counter ignores test events"
        );
    }

    #[test]
    fn the_csv_follows_the_filter_and_defuses_formulas() {
        let dir = tempfile::tempdir().unwrap();
        let (api, _) = seeded(dir.path());
        let csv = api.history_csv(&HistoryQuery::default()).unwrap();
        assert!(csv.starts_with('\u{feff}'));
        assert_eq!(csv.trim_end().split("\r\n").count(), 1 + 4);
        assert!(
            csv.contains("'=GPU()"),
            "the formula-looking rule name is neutralised"
        );
        assert!(!csv.contains(";=GPU()"));
        assert!(
            csv.contains(";detalhe, com vírgula;"),
            "commas need no quoting with a semicolon separator"
        );
        let all = api
            .history_csv(&HistoryQuery {
                include_tests: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(all.trim_end().split("\r\n").count(), 1 + 5);
        assert!(all.contains(";sim"));
    }

    #[test]
    fn clearing_and_retention_work_and_are_validated() {
        let dir = tempfile::tempdir().unwrap();
        let (api, clock) = seeded(dir.path());
        // A month passes: a 7-day retention then removes the seeded (old) events.
        clock.advance_ms(30 * 86_400_000);
        assert_eq!(
            api.set_history_retention(0).unwrap_err().code,
            "invalid_retention"
        );
        let page = api.set_history_retention(7).unwrap();
        assert_eq!(page.retention_days, 7);
        assert_eq!(page.total, 0);
        // The setting is saved.
        let (again, _) = self::api(dir.path());
        assert_eq!(
            again
                .query_history(&HistoryQuery::default())
                .unwrap()
                .retention_days,
            7
        );

        // Recent events survive the retention and clear() empties everything.
        let now = clock.now_ms();
        api.service().record(event(
            now,
            9,
            "agora",
            argus_alerts::Severity::Warn,
            argus_alerts::EventKind::Fired,
            false,
        ));
        assert_eq!(api.set_history_retention(7).unwrap().total, 1);
        assert_eq!(api.clear_history().unwrap(), 1);
        assert_eq!(
            api.query_history(&HistoryQuery::default()).unwrap().total,
            0
        );
    }

    #[test]
    fn favorites_are_pinned_in_order_deduplicated_and_persisted() {
        let dir = tempfile::tempdir().unwrap();
        let (api1, _) = api(dir.path());
        api1.set_favorite("cpu.temp", true).unwrap();
        api1.set_favorite("gpu.0.temp", true).unwrap();
        api1.set_favorite("cpu.temp", true).unwrap();
        assert_eq!(
            api1.favorites(),
            vec!["gpu.0.temp", "cpu.temp"],
            "re-pinning moves it to the end"
        );
        api1.set_favorite("gpu.0.temp", false).unwrap();
        assert_eq!(api1.set_favorite("nada", false).unwrap(), vec!["cpu.temp"]);
        assert_eq!(
            api1.set_favorite("  ", true).unwrap_err().code,
            "invalid_sensor"
        );
        let (api2, _) = api(dir.path());
        assert_eq!(api2.favorites(), vec!["cpu.temp"], "survives a restart");
    }

    #[test]
    fn settings_are_validated_saved_and_come_back_after_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let (api1, _) = api(dir.path());
        let defaults = api1.settings();
        assert_eq!(
            (
                defaults.sample_interval_ms,
                defaults.temperature_unit.as_str(),
                defaults.memory_unit.as_str()
            ),
            (1000, "C", "GB")
        );

        let changed = AppSettings {
            sample_interval_ms: 2000,
            temperature_unit: "F".into(),
            memory_unit: "GiB".into(),
            light_theme: true,
            history_retention_days: 90,
        };
        assert_eq!(api1.update_settings(changed.clone()).unwrap(), changed);
        let (api2, _) = api(dir.path());
        assert_eq!(api2.settings(), changed, "persisted");

        for bad in [
            AppSettings {
                sample_interval_ms: 10,
                ..changed.clone()
            },
            AppSettings {
                temperature_unit: "K".into(),
                ..changed.clone()
            },
            AppSettings {
                memory_unit: "MB".into(),
                ..changed.clone()
            },
            AppSettings {
                history_retention_days: 0,
                ..changed.clone()
            },
        ] {
            assert_eq!(
                api2.update_settings(bad).unwrap_err().code,
                "invalid_settings"
            );
        }
        assert_eq!(
            api2.settings(),
            changed,
            "a rejected change leaves everything as it was"
        );
    }

    #[test]
    fn the_background_options_are_saved() {
        let dir = tempfile::tempdir().unwrap();
        let (api1, _) = api(dir.path());
        assert!(api1.minimize_to_tray(), "on by default");
        api1.save_background(Some(true), Some(false)).unwrap();
        let (api2, _) = api(dir.path());
        assert!(!api2.minimize_to_tray());
        let (config, _) = ConfigStore::new(dir.path()).load();
        assert!(config.settings.autostart);
    }

    #[test]
    fn simulating_returns_a_synthetic_series_and_test_events() {
        let dir = tempfile::tempdir().unwrap();
        let (api, _) = api(dir.path());
        let sim = api.simulate_rule(rule("a")).unwrap();
        assert!(!sim.points.is_empty());
        assert!(!sim.events.is_empty() && sim.events.iter().all(|e| e.is_test));
        assert!(
            api.list_rules().is_empty(),
            "simulating does not save the rule"
        );
    }
}
