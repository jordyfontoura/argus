//! Protection presets: ready-made sets of rules for people who do not want to design their own.
//! Pure data and functions. The limits are derived from the machine (CPU Tjmax when the model is
//! known, else the generic limits in the `ThresholdRegistry`) and only sensors that exist and can
//! be read on this machine get rules.

use crate::{Actions, Mode, Operator, Rule, Severity};
use argus_core::ThresholdRegistry;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    /// Higher limits, longer waits: fewer alarms, less margin.
    Relaxed,
    Normal,
    /// Lower limits, shorter waits: more alarms, more margin.
    Strict,
}

impl Sensitivity {
    /// +1 relaxes the limits, -1 tightens them.
    fn step(self) -> f64 {
        match self {
            Self::Relaxed => 1.0,
            Self::Normal => 0.0,
            Self::Strict => -1.0,
        }
    }

    fn duration_factor(self) -> f64 {
        match self {
            Self::Relaxed => 2.0,
            Self::Normal => 1.0,
            Self::Strict => 0.5,
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "relaxed" => Some(Self::Relaxed),
            "normal" => Some(Self::Normal),
            "strict" => Some(Self::Strict),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Relaxed => "relaxed",
            Self::Normal => "normal",
            Self::Strict => "strict",
        }
    }
}

/// What the machine offers, as far as the generator is concerned.
#[derive(Debug, Clone, Default)]
pub struct MachineProfile {
    /// Sensors that exist and currently give a reading.
    pub readable: HashSet<String>,
    /// Why a known sensor cannot be read (shown next to the skipped rule).
    pub unreadable_reasons: HashMap<String, String>,
    /// CPU maximum junction temperature, when the model is known.
    pub tjmax: Option<f64>,
    pub thresholds: ThresholdRegistry,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PresetInfo {
    pub key: String,
    pub label: String,
    pub description: String,
    pub recommended: bool,
    pub default_actions: Actions,
    pub rule_count: u32,
}

/// One rule the preset would create, with whether this machine can run it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PlannedRule {
    /// Stable id of the entry inside the preset (used to leave single rules out).
    pub key: String,
    pub rule: Rule,
    /// Plain-language reason for the rule.
    pub why: String,
    pub available: bool,
    pub unavailable_reason: Option<String>,
    /// Where the limit came from, for example "Tjmax de 95 °C do modelo, com margem".
    pub limit_source: String,
}

enum Limit {
    /// CPU temperature: `Tjmax - margin` when Tjmax is known, else the registry's critical limit
    /// minus `fallback_delta`.
    TjmaxMinus {
        margin: f64,
        fallback_delta: f64,
    },
    /// The registry's critical limit minus `delta`.
    RegistryCrit {
        delta: f64,
    },
    /// The registry's warning limit.
    RegistryWarn,
    Fixed(f64),
}

struct Entry {
    key: &'static str,
    name: &'static str,
    why: &'static str,
    sensor: &'static str,
    mode: Mode,
    op: Operator,
    limit: Limit,
    /// Distance between the limit and the level where the rule resolves.
    resolve_margin: Option<f64>,
    for_sec: u32,
    severity: Severity,
    repeat_sec: u32,
    /// Full scale of the metric, used to size the sensitivity shift (5% of it).
    scale: f64,
}

struct Preset {
    key: &'static str,
    label: &'static str,
    description: &'static str,
    recommended: bool,
    actions: Actions,
    entries: &'static [Entry],
}

const FAN_STOP: Entry = Entry {
    key: "cpu_fan_stop",
    name: "Ventoinha da CPU parada",
    why: "Sem a ventoinha girando, a CPU esquenta em segundos. Confira cabos e poeira.",
    sensor: "cpu.fan_rpm",
    mode: Mode::Sustained,
    op: Operator::Below,
    limit: Limit::Fixed(300.0),
    resolve_margin: Some(200.0),
    for_sec: 10,
    severity: Severity::Crit,
    repeat_sec: 60,
    scale: 2500.0,
};

static BASIC: [Entry; 4] = [
    Entry {
        key: "cpu_temp",
        name: "CPU superaquecendo",
        why: "Calor demais encurta a vida do processador e pode desligar o PC.",
        sensor: "cpu.temp",
        mode: Mode::Sustained,
        op: Operator::Above,
        limit: Limit::TjmaxMinus {
            margin: 8.0,
            fallback_delta: 0.0,
        },
        resolve_margin: Some(8.0),
        for_sec: 15,
        severity: Severity::Crit,
        repeat_sec: 60,
        scale: 100.0,
    },
    Entry {
        key: "gpu_temp",
        name: "GPU superaquecendo",
        why: "A placa de vídeo reduz o desempenho e pode travar a imagem quando esquenta demais.",
        sensor: "gpu.0.temp",
        mode: Mode::Sustained,
        op: Operator::Above,
        limit: Limit::RegistryCrit { delta: 0.0 },
        resolve_margin: Some(8.0),
        for_sec: 15,
        severity: Severity::Crit,
        repeat_sec: 60,
        scale: 100.0,
    },
    Entry {
        key: "mem_high",
        name: "Memória quase cheia",
        why: "Com pouca memória livre, o PC fica lento e programas podem fechar sozinhos.",
        sensor: "mem.used_pct",
        mode: Mode::Average,
        op: Operator::Above,
        limit: Limit::Fixed(90.0),
        resolve_margin: None,
        for_sec: 60,
        severity: Severity::Warn,
        repeat_sec: 300,
        scale: 100.0,
    },
    FAN_STOP,
];

static GAMING: [Entry; 5] = [
    Entry {
        key: "cpu_temp",
        name: "CPU quente por muito tempo",
        why: "Calor sustentado reduz o desempenho. Vale limpar o cooler ou melhorar a ventilação.",
        sensor: "cpu.temp",
        mode: Mode::Sustained,
        op: Operator::Above,
        limit: Limit::TjmaxMinus {
            margin: 12.0,
            fallback_delta: 2.0,
        },
        resolve_margin: Some(8.0),
        for_sec: 120,
        severity: Severity::Warn,
        repeat_sec: 300,
        scale: 100.0,
    },
    Entry {
        key: "gpu_temp",
        name: "GPU quente por muito tempo",
        why:
            "Placas muito quentes perdem desempenho (thermal throttling) e envelhecem mais rápido.",
        sensor: "gpu.0.temp",
        mode: Mode::Sustained,
        op: Operator::Above,
        limit: Limit::RegistryCrit { delta: 1.0 },
        resolve_margin: Some(8.0),
        for_sec: 120,
        severity: Severity::Warn,
        repeat_sec: 300,
        scale: 100.0,
    },
    Entry {
        key: "vram_high",
        name: "VRAM quase cheia",
        why: "Quando a memória de vídeo acaba, os jogos engasgam. Reduza a qualidade das texturas.",
        sensor: "gpu.0.vram_pct",
        mode: Mode::Average,
        op: Operator::Above,
        limit: Limit::Fixed(95.0),
        resolve_margin: None,
        for_sec: 30,
        severity: Severity::Info,
        repeat_sec: 0,
        scale: 100.0,
    },
    Entry {
        key: "gpu_fan_max",
        name: "Ventoinha da GPU no máximo",
        why: "A ventoinha em rotação máxima por muito tempo indica pouca ventilação no gabinete.",
        sensor: "gpu.0.fan",
        mode: Mode::Sustained,
        op: Operator::Above,
        limit: Limit::Fixed(95.0),
        resolve_margin: Some(10.0),
        for_sec: 60,
        severity: Severity::Warn,
        repeat_sec: 0,
        scale: 100.0,
    },
    FAN_STOP,
];

static ALWAYS: [Entry; 5] = [
    Entry {
        key: "cpu_temp",
        name: "CPU quente por 5 minutos",
        why: "Máquinas ligadas o tempo todo sofrem mais com calor contínuo.",
        sensor: "cpu.temp",
        mode: Mode::Sustained,
        op: Operator::Above,
        limit: Limit::TjmaxMinus {
            margin: 18.0,
            fallback_delta: 10.0,
        },
        resolve_margin: Some(8.0),
        for_sec: 300,
        severity: Severity::Warn,
        repeat_sec: 900,
        scale: 100.0,
    },
    Entry {
        key: "gpu_temp",
        name: "GPU quente por 5 minutos",
        why: "Calor contínuo na GPU encurta a vida útil da placa.",
        sensor: "gpu.0.temp",
        mode: Mode::Sustained,
        op: Operator::Above,
        limit: Limit::RegistryWarn,
        resolve_margin: Some(8.0),
        for_sec: 300,
        severity: Severity::Warn,
        repeat_sec: 900,
        scale: 100.0,
    },
    Entry {
        key: "mem_high",
        name: "Memória alta por 5 minutos",
        why: "Uso de memória que só cresce costuma indicar um programa com vazamento.",
        sensor: "mem.used_pct",
        mode: Mode::Average,
        op: Operator::Above,
        limit: Limit::Fixed(85.0),
        resolve_margin: None,
        for_sec: 300,
        severity: Severity::Warn,
        repeat_sec: 900,
        scale: 100.0,
    },
    Entry {
        key: "cpu_load",
        name: "CPU no limite há 10 minutos",
        why: "Carga máxima por tanto tempo costuma significar um programa travado.",
        sensor: "cpu.load",
        mode: Mode::Average,
        op: Operator::Above,
        limit: Limit::Fixed(90.0),
        resolve_margin: None,
        for_sec: 600,
        severity: Severity::Info,
        repeat_sec: 0,
        scale: 100.0,
    },
    FAN_STOP,
];

fn all() -> [Preset; 3] {
    [
        Preset {
            key: "basic",
            label: "Proteção básica",
            description: "O essencial para o PC não superaquecer nem travar. Boa escolha para a maioria das pessoas.",
            recommended: true,
            actions: Actions { toast: true, sound: false, notify: true },
            entries: &BASIC,
        },
        Preset {
            key: "gaming",
            label: "Jogos e uso pesado",
            description: "Tolera picos de calor de jogos e renderizações, mas avisa quando o calor é prolongado.",
            recommended: false,
            actions: Actions { toast: true, sound: true, notify: false },
            entries: &GAMING,
        },
        Preset {
            key: "always_on",
            label: "PC ligado 24 h",
            description: "Para servidores e máquinas que ficam ligadas sem ninguém olhando. Foca em estabilidade e funciona com a janela fechada.",
            recommended: false,
            actions: Actions { toast: true, sound: true, notify: true },
            entries: &ALWAYS,
        },
    ]
}

pub fn presets() -> Vec<PresetInfo> {
    all()
        .iter()
        .map(|p| PresetInfo {
            key: p.key.to_string(),
            label: p.label.to_string(),
            description: p.description.to_string(),
            recommended: p.recommended,
            default_actions: p.actions,
            rule_count: p.entries.len() as u32,
        })
        .collect()
}

/// Maximum junction temperature by CPU model. Only families where the figure is well known are
/// listed, and where models of a family differ the lower value is used, so the derived limit errs
/// on the safe side. `None` means "unknown": the generator falls back to generic limits.
pub fn tjmax_for(model: &str) -> Option<f64> {
    let m = model.to_ascii_lowercase();
    if m.contains("ryzen") {
        // Ryzen 3000 and 7000/9000 series are specified at 95 °C; 5000 series parts at 90 °C or 95 °C.
        let digits: String = m
            .split_whitespace()
            .find(|w| w.len() >= 4 && w.chars().take(4).all(|c| c.is_ascii_digit()))
            .unwrap_or("")
            .chars()
            .take(4)
            .collect();
        return match digits.chars().next() {
            Some('3') | Some('7') | Some('9') => Some(95.0),
            Some('5') => Some(90.0),
            _ => None,
        };
    }
    if m.contains("core(tm)")
        && (m.contains(" i3") || m.contains(" i5") || m.contains(" i7") || m.contains(" i9"))
    {
        return Some(100.0);
    }
    None
}

fn round(v: f64) -> f64 {
    v.round()
}

fn limit_value(entry: &Entry, profile: &MachineProfile) -> (f64, String) {
    let registry = profile.thresholds.get(&entry.sensor.into());
    match &entry.limit {
        Limit::TjmaxMinus {
            margin,
            fallback_delta,
        } => match profile.tjmax {
            Some(t) => (
                t - margin,
                format!("Tjmax de {t:.0} °C do modelo, com margem"),
            ),
            None => (
                registry.map_or(90.0, |r| r.crit) - fallback_delta,
                "limite padrão (o Tjmax deste processador não é conhecido)".to_string(),
            ),
        },
        Limit::RegistryCrit { delta } => (
            registry.map_or(85.0, |r| r.crit) - delta,
            "limite padrão para este sensor".to_string(),
        ),
        Limit::RegistryWarn => (
            registry.map_or(75.0, |r| r.warn),
            "limite padrão para este sensor".to_string(),
        ),
        Limit::Fixed(v) => (*v, "limite padrão".to_string()),
    }
}

fn plan_entry(
    preset: &Preset,
    entry: &Entry,
    sens: Sensitivity,
    profile: &MachineProfile,
    actions: Actions,
) -> PlannedRule {
    let (base, source) = limit_value(entry, profile);
    let shift = sens.step() * entry.scale * 0.05;
    let mut value = match entry.op {
        Operator::Above => base + shift,
        Operator::Below => base - shift,
    };
    // Never above what the machine says is the critical level, nor within 3 °C of Tjmax.
    if entry.op == Operator::Above {
        if let Some(r) = profile.thresholds.get(&entry.sensor.into()) {
            value = value.min(r.crit);
        }
        if entry.sensor == "cpu.temp" {
            if let Some(t) = profile.tjmax {
                value = value.min(t - 3.0);
            }
        }
        if entry.sensor.ends_with("temp") {
            value = value.max(40.0);
        }
    }
    let value = round(value);
    let hysteresis = entry.resolve_margin.map(|m| match entry.op {
        Operator::Above => value - m,
        Operator::Below => value + m,
    });
    let for_sec = ((f64::from(entry.for_sec) * sens.duration_factor()).round() as u32).max(5);
    let readable = profile.readable.contains(entry.sensor);
    PlannedRule {
        key: entry.key.to_string(),
        why: entry.why.to_string(),
        available: readable,
        unavailable_reason: (!readable).then(|| {
            profile
                .unreadable_reasons
                .get(entry.sensor)
                .cloned()
                .unwrap_or_else(|| "esta máquina não expõe este sensor".to_string())
        }),
        limit_source: source,
        rule: Rule {
            id: 0,
            name: entry.name.to_string(),
            sensor: entry.sensor.to_string(),
            mode: entry.mode,
            op: entry.op,
            threshold: value,
            hysteresis,
            for_sec,
            count: 3,
            recover_sec: 5,
            repeat_sec: entry.repeat_sec,
            severity: entry.severity,
            actions,
            enabled: true,
            preset: Some(preset.key.to_string()),
        },
    }
}

/// The rules a preset would create on this machine, including the ones it has to skip (with the
/// reason), so the user sees what is and is not covered. `None` for an unknown preset key.
pub fn plan(
    preset_key: &str,
    sensitivity: Sensitivity,
    profile: &MachineProfile,
    actions: Option<Actions>,
) -> Option<Vec<PlannedRule>> {
    let presets = all();
    let preset = presets.iter().find(|p| p.key == preset_key)?;
    let actions = actions.unwrap_or(preset.actions);
    Some(
        preset
            .entries
            .iter()
            .map(|e| plan_entry(preset, e, sensitivity, profile, actions))
            .collect(),
    )
}

/// The rule list after applying a preset: rules from an earlier preset are replaced, the user's
/// own rules are kept (or dropped when `keep_own` is false), and the chosen, runnable planned
/// rules are added with id 0 for the caller to number.
pub fn apply(
    existing: &[Rule],
    planned: &[PlannedRule],
    chosen: &HashSet<String>,
    keep_own: bool,
) -> Vec<Rule> {
    let mut rules: Vec<Rule> = existing
        .iter()
        .filter(|r| keep_own && r.preset.is_none())
        .cloned()
        .collect();
    rules.extend(
        planned
            .iter()
            .filter(|p| p.available && chosen.contains(&p.key))
            .map(|p| p.rule.clone()),
    );
    rules
}

#[cfg(test)]
mod tests;
