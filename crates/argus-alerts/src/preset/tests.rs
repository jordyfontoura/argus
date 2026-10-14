use super::*;
use argus_core::ThresholdRegistry;

fn machine(sensors: &[&str], tjmax: Option<f64>) -> MachineProfile {
    MachineProfile {
        readable: sensors.iter().map(|s| s.to_string()).collect(),
        unreadable_reasons: HashMap::new(),
        tjmax,
        thresholds: ThresholdRegistry::with_defaults(),
    }
}

fn full_pc(tjmax: Option<f64>) -> MachineProfile {
    machine(
        &[
            "cpu.temp",
            "cpu.load",
            "gpu.0.temp",
            "gpu.0.vram_pct",
            "gpu.0.fan",
            "mem.used_pct",
            "cpu.fan_rpm",
        ],
        tjmax,
    )
}

fn planned(key: &str, sens: Sensitivity, profile: &MachineProfile) -> Vec<PlannedRule> {
    plan(key, sens, profile, None).expect("known preset")
}

fn by_key<'a>(rules: &'a [PlannedRule], key: &str) -> &'a PlannedRule {
    rules
        .iter()
        .find(|r| r.key == key)
        .unwrap_or_else(|| panic!("no entry {key}"))
}

#[test]
fn there_are_three_presets_with_a_recommended_one() {
    let list = presets();
    assert_eq!(
        list.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(),
        vec!["basic", "gaming", "always_on"]
    );
    assert_eq!(list.iter().filter(|p| p.recommended).count(), 1);
    assert!(list[0].recommended && list[0].rule_count == 4);
}

#[test]
fn every_generated_rule_is_valid_for_every_preset_and_sensitivity() {
    for preset in ["basic", "gaming", "always_on"] {
        for sens in [
            Sensitivity::Relaxed,
            Sensitivity::Normal,
            Sensitivity::Strict,
        ] {
            for tjmax in [None, Some(90.0), Some(95.0), Some(100.0)] {
                for p in planned(preset, sens, &full_pc(tjmax)) {
                    p.rule
                        .validate()
                        .unwrap_or_else(|e| panic!("{preset}/{sens:?}/{tjmax:?}/{}: {e}", p.key));
                    assert_eq!(p.rule.preset.as_deref(), Some(preset));
                    assert!(p.available);
                }
            }
        }
    }
}

#[test]
fn the_cpu_temperature_limit_comes_from_tjmax_with_a_margin() {
    let rules = planned("basic", Sensitivity::Normal, &full_pc(Some(100.0)));
    let cpu = by_key(&rules, "cpu_temp");
    assert_eq!(
        cpu.rule.threshold, 90.0,
        "Tjmax 100 - margin 8, capped by the registry's critical 90"
    );
    assert!(cpu.limit_source.contains("Tjmax de 100"));
    assert_eq!(cpu.rule.hysteresis, Some(82.0));

    // A CPU with a lower Tjmax gets a lower limit.
    let low = planned("basic", Sensitivity::Normal, &full_pc(Some(90.0)));
    assert_eq!(by_key(&low, "cpu_temp").rule.threshold, 82.0);
}

#[test]
fn an_unknown_tjmax_falls_back_to_the_generic_limit_and_says_so() {
    let rules = planned("basic", Sensitivity::Normal, &full_pc(None));
    let cpu = by_key(&rules, "cpu_temp");
    assert_eq!(cpu.rule.threshold, 90.0);
    assert!(cpu.limit_source.contains("não é conhecido"));
}

#[test]
fn limits_never_exceed_the_registry_critical_level_or_get_within_3_degrees_of_tjmax() {
    for sens in [
        Sensitivity::Relaxed,
        Sensitivity::Normal,
        Sensitivity::Strict,
    ] {
        let profile = full_pc(Some(95.0));
        for preset in ["basic", "gaming", "always_on"] {
            let rules = planned(preset, sens, &profile);
            let cpu = by_key(&rules, "cpu_temp");
            assert!(
                cpu.rule.threshold <= 92.0,
                "{preset}/{sens:?}: {}",
                cpu.rule.threshold
            );
            assert!(
                cpu.rule.threshold <= 90.0,
                "registry critical for cpu.temp is 90"
            );
        }
    }
}

#[test]
fn sensitivity_moves_limits_and_waiting_times_in_the_right_direction() {
    let profile = full_pc(Some(100.0));
    let relaxed = planned("gaming", Sensitivity::Relaxed, &profile);
    let normal = planned("gaming", Sensitivity::Normal, &profile);
    let strict = planned("gaming", Sensitivity::Strict, &profile);
    let gpu = |r: &[PlannedRule]| by_key(r, "gpu_temp").rule.clone();
    assert!(gpu(&strict).threshold < gpu(&normal).threshold);
    assert!(gpu(&normal).threshold <= gpu(&relaxed).threshold);
    assert!(
        gpu(&strict).for_sec < gpu(&normal).for_sec && gpu(&normal).for_sec < gpu(&relaxed).for_sec
    );
    // "Below" rules move the other way: stricter means a higher rotation counts as stopped.
    let fan = |r: &[PlannedRule]| by_key(r, "cpu_fan_stop").rule.threshold;
    assert!(fan(&strict) > fan(&normal) && fan(&normal) > fan(&relaxed));
    // Never shorter than 5 s.
    assert!(strict.iter().all(|p| p.rule.for_sec >= 5));
}

#[test]
fn only_sensors_that_exist_get_rules_and_the_rest_are_explained() {
    // A PC with no readable temperatures or fans (this very machine's CPU case).
    let profile = machine(&["cpu.load", "mem.used_pct", "gpu.0.temp"], None);
    let rules = planned("basic", Sensitivity::Normal, &profile);
    assert!(!by_key(&rules, "cpu_temp").available);
    assert!(by_key(&rules, "cpu_temp")
        .unavailable_reason
        .as_deref()
        .unwrap()
        .contains("não expõe"));
    assert!(!by_key(&rules, "cpu_fan_stop").available);
    assert!(by_key(&rules, "gpu_temp").available);
    assert!(by_key(&rules, "mem_high").available);

    let chosen: HashSet<String> = rules.iter().map(|r| r.key.clone()).collect();
    let applied = apply(&[], &rules, &chosen, true);
    assert_eq!(applied.len(), 2, "only the runnable rules are created");
    assert!(applied.iter().all(|r| profile.readable.contains(&r.sensor)));
}

#[test]
fn a_custom_reason_for_an_unreadable_sensor_is_kept() {
    let mut profile = machine(&[], None);
    profile.unreadable_reasons.insert(
        "cpu.temp".into(),
        "a placa não expõe zonas térmicas ao Windows".into(),
    );
    let rules = planned("basic", Sensitivity::Normal, &profile);
    assert_eq!(
        by_key(&rules, "cpu_temp").unavailable_reason.as_deref(),
        Some("a placa não expõe zonas térmicas ao Windows")
    );
}

#[test]
fn chosen_actions_override_the_preset_defaults() {
    let custom = Actions {
        toast: false,
        sound: true,
        notify: false,
    };
    let rules = plan("basic", Sensitivity::Normal, &full_pc(None), Some(custom)).unwrap();
    assert!(rules.iter().all(|r| r.rule.actions == custom));
    let defaults = planned("basic", Sensitivity::Normal, &full_pc(None));
    assert_eq!(defaults[0].rule.actions, presets()[0].default_actions);
}

#[test]
fn applying_replaces_old_preset_rules_and_keeps_or_drops_the_users_own() {
    let own = Rule {
        preset: None,
        name: "minha regra".into(),
        ..planned("basic", Sensitivity::Normal, &full_pc(None))[0]
            .rule
            .clone()
    };
    let old = Rule {
        preset: Some("gaming".into()),
        name: "antiga".into(),
        ..own.clone()
    };
    let existing = vec![own.clone(), old];
    let rules = planned("basic", Sensitivity::Normal, &full_pc(None));
    let chosen: HashSet<String> = rules.iter().map(|r| r.key.clone()).collect();

    let kept = apply(&existing, &rules, &chosen, true);
    assert!(kept.iter().any(|r| r.name == "minha regra"));
    assert!(
        !kept.iter().any(|r| r.name == "antiga"),
        "the previous preset is replaced"
    );
    assert_eq!(kept.len(), 1 + rules.len());

    let dropped = apply(&existing, &rules, &chosen, false);
    assert!(!dropped.iter().any(|r| r.name == "minha regra"));
    assert_eq!(dropped.len(), rules.len());
}

#[test]
fn leaving_entries_out_creates_fewer_rules() {
    let rules = planned("basic", Sensitivity::Normal, &full_pc(None));
    let chosen: HashSet<String> = ["cpu_temp".to_string()].into_iter().collect();
    let applied = apply(&[], &rules, &chosen, true);
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0].name, "CPU superaquecendo");
}

#[test]
fn an_unknown_preset_is_rejected() {
    assert!(plan("nope", Sensitivity::Normal, &full_pc(None), None).is_none());
}

#[test]
fn tjmax_is_only_claimed_for_models_where_it_is_well_known() {
    assert_eq!(tjmax_for("AMD Ryzen 5 3500X 6-Core Processor"), Some(95.0));
    assert_eq!(tjmax_for("AMD Ryzen 7 5800X 8-Core Processor"), Some(90.0));
    assert_eq!(tjmax_for("AMD Ryzen 9 7950X 16-Core Processor"), Some(95.0));
    assert_eq!(
        tjmax_for("Intel(R) Core(TM) i7-8700K CPU @ 3.70GHz"),
        Some(100.0)
    );
    assert_eq!(tjmax_for("Intel(R) Celeron(R) N4020"), None);
    assert_eq!(tjmax_for("Some Unknown CPU"), None);
    assert_eq!(tjmax_for(""), None);
}

#[test]
fn sensitivity_names_round_trip() {
    for s in [
        Sensitivity::Relaxed,
        Sensitivity::Normal,
        Sensitivity::Strict,
    ] {
        assert_eq!(Sensitivity::parse(s.as_str()), Some(s));
    }
    assert_eq!(Sensitivity::parse("x"), None);
}
