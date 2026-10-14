use argus_alerts::Rule;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Version written to new files. Older files are migrated on load; newer ones are never
/// overwritten (they are set aside so a downgrade cannot destroy data).
pub const CURRENT_VERSION: u32 = 1;

/// Which protection preset the user applied and how sensitive it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresetChoice {
    pub preset: String,
    pub sensitivity: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub sample_interval_ms: u64,
    pub temperature_unit: String,
    pub memory_unit: String,
    pub light_theme: bool,
    pub autostart: bool,
    pub minimize_to_tray: bool,
    pub history_retention_days: u32,
    /// Critical rules still notify while notifications are muted.
    pub critical_bypasses_mute: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sample_interval_ms: 1000,
            temperature_unit: "C".to_string(),
            memory_unit: "GB".to_string(),
            light_theme: false,
            autostart: false,
            minimize_to_tray: true,
            history_retention_days: 30,
            critical_bypasses_mute: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub schema_version: u32,
    pub rules: Vec<Rule>,
    pub next_rule_id: u64,
    pub preset: Option<PresetChoice>,
    pub favorites: Vec<String>,
    pub settings: Settings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_VERSION,
            rules: Vec::new(),
            next_rule_id: 1,
            preset: None,
            favorites: Vec::new(),
            settings: Settings::default(),
        }
    }
}

/// What happened while loading, so the app can tell the user instead of failing silently.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoadReport {
    /// The file that was set aside (corrupt or from a newer version), if any.
    pub set_aside: Option<PathBuf>,
    pub reason: Option<String>,
    /// The file existed in an older format and was upgraded in memory.
    pub migrated_from: Option<u32>,
}

pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new(dir: &Path) -> Self {
        Self {
            path: dir.join("config.json"),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads the config. A missing file gives the defaults. A corrupt file or one written by a
    /// newer Argus is renamed to `config.json.bak-<time>` and the defaults are returned, so the
    /// app always opens and nothing the user had is lost.
    pub fn load(&self) -> (Config, LoadReport) {
        let text = match fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return (Config::default(), LoadReport::default());
            }
            Err(e) => return self.set_aside(format!("não foi possível ler o arquivo: {e}")),
        };
        let value: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => return self.set_aside(format!("arquivo corrompido: {e}")),
        };
        let version = value
            .get("schema_version")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32;
        if version > CURRENT_VERSION {
            return self.set_aside(format!(
                "arquivo criado por uma versão mais nova do Argus (esquema {version})"
            ));
        }
        let migrated = migrate(value, version);
        match serde_json::from_value::<Config>(migrated) {
            Ok(mut config) => {
                config.schema_version = CURRENT_VERSION;
                let report = LoadReport {
                    migrated_from: (version < CURRENT_VERSION).then_some(version),
                    ..LoadReport::default()
                };
                (config, report)
            }
            Err(e) => self.set_aside(format!("configuração inválida: {e}")),
        }
    }

    fn set_aside(&self, reason: String) -> (Config, LoadReport) {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let mut backup = self.path.clone().into_os_string();
        backup.push(format!(".bak-{stamp}"));
        let backup = PathBuf::from(backup);
        let moved = fs::rename(&self.path, &backup).is_ok();
        (
            Config::default(),
            LoadReport {
                set_aside: moved.then_some(backup),
                reason: Some(reason),
                migrated_from: None,
            },
        )
    }

    /// Writes the config so a crash can never leave a half-written file: the new content goes
    /// to a temporary file in the same folder, is flushed to disk, then replaces the old file.
    pub fn save(&self, config: &Config) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut tmp = self.path.clone().into_os_string();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        let json = serde_json::to_vec_pretty(config).map_err(std::io::Error::other)?;
        {
            let mut file = File::create(&tmp)?;
            file.write_all(&json)?;
            file.sync_all()?;
        }
        fs::rename(&tmp, &self.path)
    }
}

/// Upgrades a document from `from` to [`CURRENT_VERSION`], one step at a time.
fn migrate(mut doc: Value, from: u32) -> Value {
    let mut version = from;
    while version < CURRENT_VERSION {
        doc = match version {
            // Files without a version predate versioning: every field already has a default,
            // so stamping the version is the whole upgrade.
            0 => {
                if let Some(obj) = doc.as_object_mut() {
                    obj.insert("schema_version".to_string(), Value::from(1));
                }
                doc
            }
            _ => doc,
        };
        version += 1;
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_alerts::{Actions, Mode, Operator, Severity};
    use tempfile::tempdir;

    fn rule(id: u64) -> Rule {
        Rule {
            id,
            name: format!("regra {id}"),
            sensor: "cpu.temp".into(),
            mode: Mode::Sustained,
            op: Operator::Above,
            threshold: 85.0,
            hysteresis: None,
            for_sec: 10,
            count: 3,
            recover_sec: 5,
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

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempdir().unwrap();
        let (config, report) = ConfigStore::new(dir.path()).load();
        assert_eq!(config, Config::default());
        assert_eq!(report, LoadReport::default());
    }

    #[test]
    fn saved_config_round_trips() {
        let dir = tempdir().unwrap();
        let store = ConfigStore::new(dir.path());
        let mut config = Config::default();
        config.rules.push(rule(1));
        config.next_rule_id = 2;
        config.favorites.push("cpu.temp".into());
        config.preset = Some(PresetChoice {
            preset: "basic".into(),
            sensitivity: "normal".into(),
        });
        config.settings.light_theme = true;
        store.save(&config).unwrap();
        let (loaded, report) = store.load();
        assert_eq!(loaded, config);
        assert_eq!(report, LoadReport::default());
    }

    #[test]
    fn save_leaves_no_temporary_file_and_replaces_the_old_one() {
        let dir = tempdir().unwrap();
        let store = ConfigStore::new(dir.path());
        store.save(&Config::default()).unwrap();
        let mut second = Config::default();
        second.favorites.push("a".into());
        store.save(&second).unwrap();
        assert_eq!(store.load().0, second);
        let names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["config.json".to_string()]);
    }

    #[test]
    fn a_leftover_temp_file_from_a_crash_does_not_affect_the_real_config() {
        let dir = tempdir().unwrap();
        let store = ConfigStore::new(dir.path());
        let mut good = Config::default();
        good.favorites.push("keep".into());
        store.save(&good).unwrap();
        // A crash in the middle of a later save leaves a truncated temp file behind.
        fs::write(dir.path().join("config.json.tmp"), b"{\"schema_ver").unwrap();
        let (loaded, report) = store.load();
        assert_eq!(loaded, good);
        assert!(report.set_aside.is_none());
        // And the next save still works.
        store.save(&good).unwrap();
        assert_eq!(store.load().0, good);
    }

    #[test]
    fn a_corrupt_file_is_set_aside_and_the_app_gets_defaults() {
        let dir = tempdir().unwrap();
        let store = ConfigStore::new(dir.path());
        fs::write(store.path(), b"{ not json").unwrap();
        let (config, report) = store.load();
        assert_eq!(config, Config::default());
        let backup = report.set_aside.expect("backup path");
        assert!(backup.exists());
        assert_eq!(fs::read(&backup).unwrap(), b"{ not json");
        assert!(!store.path().exists());
        assert!(report.reason.unwrap().contains("corrompido"));
    }

    #[test]
    fn a_file_from_a_newer_version_is_preserved_not_overwritten() {
        let dir = tempdir().unwrap();
        let store = ConfigStore::new(dir.path());
        fs::write(store.path(), br#"{"schema_version": 99, "future": true}"#).unwrap();
        let (config, report) = store.load();
        assert_eq!(config, Config::default());
        let backup = report.set_aside.expect("backup path");
        assert!(fs::read_to_string(backup).unwrap().contains("future"));
    }

    #[test]
    fn a_file_without_a_version_is_migrated_and_keeps_its_data() {
        let dir = tempdir().unwrap();
        let store = ConfigStore::new(dir.path());
        fs::write(
            store.path(),
            br#"{"favorites": ["cpu.load"], "settings": {"light_theme": true}}"#,
        )
        .unwrap();
        let (config, report) = store.load();
        assert_eq!(report.migrated_from, Some(0));
        assert_eq!(config.schema_version, CURRENT_VERSION);
        assert_eq!(config.favorites, vec!["cpu.load".to_string()]);
        assert!(config.settings.light_theme);
        // Fields missing from an older file take their defaults.
        assert_eq!(config.settings.history_retention_days, 30);
    }

    #[test]
    fn a_structurally_wrong_file_is_set_aside() {
        let dir = tempdir().unwrap();
        let store = ConfigStore::new(dir.path());
        fs::write(store.path(), br#"{"schema_version": 1, "rules": "nope"}"#).unwrap();
        let (config, report) = store.load();
        assert_eq!(config, Config::default());
        assert!(report.set_aside.is_some());
    }
}
