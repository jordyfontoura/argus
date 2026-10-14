// Synthetic backend used only when the UI is opened outside the app (captures, UI tests).
// It speaks the same contract as the Rust side and says so on screen ("Modo de demonstração").
import { COMMANDS } from "../generated/names.js";

const SENSORS = [
  ["cpu.load", "CPU", "Carga total", "percent", [90, 98]],
  ["cpu.clock", "CPU", "Clock médio", "megahertz", null],
  ["cpu.temp", "CPU", "Temperatura", "celsius", [80, 90]],
  ["gpu.0.load", "GPU", "Carga", "percent", [95, 99]],
  ["gpu.0.temp", "GPU", "Temperatura", "celsius", [75, 85]],
  ["gpu.0.power", "GPU", "Potência", "watt", null],
  ["mem.used", "Memória", "Em uso", "bytes", null],
  ["mem.total", "Memória", "Total instalado", "bytes", null],
  ["mem.used_pct", "Memória", "Uso", "percent", [80, 92]],
  ["cpu.power", "CPU", "Potência do pacote", "watt", [110, 130]],
  ["cpu.vcore", "CPU", "Tensão do núcleo", "volt", null],
  ["cpu.freq_pct", "CPU", "Frequência em relação ao máximo", "percent", null],
  ["cpu.limit_flags", "CPU", "Flags de limite de desempenho", "count", null],
  ...Array.from({ length: 8 }, (_, i) => [`cpu.thread.${i}.load`, "CPU", `Thread ${i}`, "percent", null]),
  ["gpu.0.vram_pct", "GPU", "VRAM em uso (%)", "percent", [85, 95]],
  ["gpu.0.clock_core", "GPU", "Clock do núcleo", "megahertz", null],
  ["gpu.0.clock_mem", "GPU", "Clock da memória", "megahertz", null],
  ["gpu.0.hotspot", "GPU", "Temperatura do hot spot", "celsius", [90, 105]],
  ["gpu.0.fan", "GPU", "Ventoinha", "percent", [85, 95]],
  ["gpu.0.power_limit", "GPU", "Limite de potência", "watt", null],
  ["gpu.0.limit.power", "GPU", "Limitador: potência", "count", null],
  ["gpu.0.limit.thermal", "GPU", "Limitador: temperatura", "count", null],
  ["gpu.0.limit.hw_slowdown", "GPU", "Limitador: redução por hardware", "count", null],
  ["gpu.0.limit.idle", "GPU", "Placa ociosa", "count", null],
  ["disk.busy", "Armazenamento", "Atividade", "percent", null],
  ["disk.read", "Armazenamento", "Leitura total", "bytes_per_second", null],
  ["disk.write", "Armazenamento", "Gravação total", "bytes_per_second", null],
  ["mem.swap_used", "Memória", "Arquivo de paginação em uso", "bytes", null],
  ["mem.swap_total", "Memória", "Arquivo de paginação total", "bytes", null],
  ["net.total.rx", "Rede", "Recebido (total)", "mbps", null],
  ["net.total.tx", "Rede", "Enviado (total)", "mbps", null],
  ["net.ethernet.rx", "Rede", "Ethernet recebido", "mbps", null],
  ["net.ethernet.tx", "Rede", "Ethernet enviado", "mbps", null],
];
const GB = 1e9;

export function createDemo() {
  let t = 0;
  const walk = (p, lo, hi, s) => Math.max(lo, Math.min(hi, p + (Math.random() - 0.5) * s));
  const st = { cpu: 22, gpu: 18, temp: 46, gtemp: 50, mem: 48 };
  const handlers = new Set();
  const listeners = new Map(); // event name -> Set<handler>
  const emit = (name, payload) => listeners.get(name)?.forEach((h) => h(payload));
  // Alert rules live in memory only; the demo does a naive evaluation (no timers).
  let rules = [];
  let nextRuleId = 1;
  let mutedUntil = 0;
  let presetChoice = null;
  let retention = 30;
  const demoSettings = { sample_interval_ms: 1000, temperature_unit: "C", memory_unit: "GB", light_theme: false, history_retention_days: 30 };
  const background = { autostart: false, minimize_to_tray: true };
  const now0 = Date.now();
  const ev = (min, rule_id, rule_name, severity, kind, detail, is_test = false) => ({ ts_ms: now0 - min * 60000, rule_id, rule_name, sensor: "cpu.temp", severity, kind, detail, value: 90, is_test });
  const historyLog = [
    ev(5, 1, "CPU superaquecendo", "crit", "resolved", "voltou ao normal · ativa por 4 min · pico 91,2"),
    ev(9, 1, "CPU superaquecendo", "crit", "fired", "valor 91,2 acima de 85 por 10 s seguidos"),
    ev(60, 2, "<b>Memória</b>", "warn", "fired", "média 91,0 acima de 90 nos últimos 1 min"),
    ev(61, 2, "<b>Memória</b>", "warn", "note", "o sensor parou de responder"),
    ev(120, 3, "Regra antiga =cmd()", "info", "ended", "encerrada (regra excluída)"),
    ev(130, 1, "CPU superaquecendo", "crit", "fired", "teste", true),
  ];
  const lastValue = {};
  const sessionRanges = {};
  let favs = ["cpu.load", "gpu.0.temp"];

  function readings(ts) {
    t++;
    st.cpu = walk(st.cpu, 3, 100, t % 40 < 8 ? 60 : 14);
    st.gpu = walk(st.gpu, 0, 100, 16);
    st.temp = walk(st.temp + (st.cpu - 45) * 0.01, 32, 92, 2.5);
    st.gtemp = walk(st.gtemp + (st.gpu - 50) * 0.01, 35, 88, 2);
    st.mem = walk(st.mem, 35, 80, 1.6);
    const ok = (value) => ({ kind: "ok", value });
    const r = (sensor, value) => ({ sensor, ts_ms: ts, value });
    return [
      r("cpu.load", ok(st.cpu)),
      r("cpu.clock", ok(3600 + st.cpu * 12)),
      // The demo also exercises the "unavailable" path, as on many real machines.
      r("cpu.temp", { kind: "unavailable", reason: "demonstração: sensor simulado como indisponível" }),
      r("gpu.0.load", ok(st.gpu)),
      r("gpu.0.temp", ok(st.gtemp)),
      r("gpu.0.power", ok(40 + st.gpu * 1.3)),
      r("mem.used", ok((st.mem / 100) * 32 * GB)),
      r("mem.total", ok(32 * GB)),
      r("mem.used_pct", ok(st.mem)),
      r("cpu.power", ok(20 + st.cpu * 0.9)),
      r("cpu.vcore", { kind: "unavailable", reason: "demonstração: exige driver" }),
      r("cpu.freq_pct", ok(st.cpu > 90 ? 88 : 100)),
      r("cpu.limit_flags", ok(st.cpu > 90 ? 1 : 0)),
      ...Array.from({ length: 8 }, (_, i) => r(`cpu.thread.${i}.load`, ok(Math.max(0, Math.min(100, st.cpu + Math.sin(t / 5 + i) * 25))))),
      r("gpu.0.vram_pct", ok(30 + st.gpu * 0.3)),
      r("gpu.0.clock_core", ok(300 + st.gpu * 18)),
      r("gpu.0.clock_mem", ok(405 + st.gpu * 70)),
      r("gpu.0.hotspot", { kind: "unavailable", reason: "demonstração: o NVML não expõe o hot spot" }),
      r("gpu.0.fan", ok(st.gpu > 5 ? 30 + st.gpu * 0.4 : 0)),
      r("gpu.0.power_limit", ok(170)),
      r("gpu.0.limit.power", ok(st.gpu > 80 ? 1 : 0)),
      r("gpu.0.limit.thermal", ok(0)),
      r("gpu.0.limit.hw_slowdown", ok(0)),
      r("gpu.0.limit.idle", ok(st.gpu < 5 ? 1 : 0)),
      r("disk.busy", ok(st.gpu * 0.5)),
      r("disk.read", ok(st.cpu * 2e6)),
      r("disk.write", ok(st.gpu * 1e6)),
      r("mem.swap_used", ok(4 * GB)),
      r("mem.swap_total", ok(24 * GB)),
      r("net.total.rx", ok(st.cpu * 1.5)),
      r("net.total.tx", ok(st.cpu * 0.2)),
      r("net.ethernet.rx", ok(st.cpu * 1.5)),
      r("net.ethernet.tx", ok(st.cpu * 0.2)),
    ];
  }

  const log = [];
  const now = Date.now();
  for (let i = 900; i > 0; i--) log.push(...readings(now - i * 1000));

  const alertsState = () => ({
    statuses: rules.map((r) => {
      const v = lastValue[r.sensor];
      const hot = r.enabled && v != null && (r.op === "above" ? v > r.threshold : v < r.threshold);
      return { rule_id: r.id, phase: !r.enabled ? "disabled" : hot ? "firing" : "ok", last_value: v ?? null, sensor_available: v != null, elapsed_ms: 0, needed_ms: r.for_sec * 1000, peak: hot ? v : null, occurrences: 0 };
    }),
    muted_until_ms: mutedUntil,
    critical_bypasses_mute: true,
    preset_label: presetChoice?.label ?? null,
    preset_sensitivity: presetChoice?.sensitivity ?? null,
    history_count: historyLog.filter((e) => !e.is_test).length,
    active_count: rules.filter((r) => { const v = lastValue[r.sensor]; return r.enabled && v != null && (r.op === "above" ? v > r.threshold : v < r.threshold); }).length,
  });

  setInterval(() => {
    const batch = readings(Date.now());
    for (const r of batch) {
      if (r.value.kind !== "ok") continue;
      lastValue[r.sensor] = r.value.value;
      const s = sessionRanges[r.sensor];
      sessionRanges[r.sensor] = s ? [Math.min(s[0], r.value.value), Math.max(s[1], r.value.value)] : [r.value.value, r.value.value];
    }
    emit("alerts:state", alertsState());
    log.push(...batch);
    if (log.length > 40 * 1200) log.splice(0, 40 * 60);
    handlers.forEach((h) => h(batch));
  }, 1000);

  return {
    async invoke(name, args) {
      switch (name) {
        case COMMANDS.getStaticInfo:
          return Promise.resolve({
            host: { value: "DEMO-PC", unavailable: null },
            os: { value: "Windows 11 (demonstração)", unavailable: null },
            cpu_model: { value: "CPU de demonstração", unavailable: null },
            cpu_physical_cores: { value: 8, unavailable: null },
            cpu_threads: { value: 16, unavailable: null },
            memory_total_bytes: { value: 32 * GB, unavailable: null },
            gpus: [{ value: "GPU de demonstração", unavailable: null }],
          });
        case COMMANDS.getCpuDetails: {
          const na = (m) => ({ value: null, unavailable: m });
          const k = (v) => ({ value: v, unavailable: null });
          return Promise.resolve({
            model: k("CPU de demonstração"), vendor: k("DemoVendor"), codename: na("demonstração: exigiria tabela por modelo"),
            socket: k("AM4"), process_node: na("demonstração: exigiria tabela por modelo"),
            physical_cores: k(8), threads: k(16), signature: k("19h / 21h / 0"), base_clock_mhz: k(3800),
            tdp_watts: na("demonstração: exigiria tabela por modelo"), tjmax_celsius: na("demonstração: exigiria tabela por modelo"),
            multiplier: na("demonstração: exige driver"),
            caches: [
              { name: "L1 dados", instances: 8, size_bytes_each: 32768, ways: 8 },
              { name: "L1 instruções", instances: 8, size_bytes_each: 32768, ways: 8 },
              { name: "L2", instances: 8, size_bytes_each: 524288, ways: 8 },
              { name: "L3", instances: 1, size_bytes_each: 33554432, ways: 16 },
            ],
            instruction_sets: ["MMX", "SSE", "SSE2", "SSE3", "SSSE3", "SSE4.1", "SSE4.2", "AES", "AVX", "AVX2", "FMA3"],
          });
        }
        case COMMANDS.getGpuDetails: {
          const k = (v) => ({ value: v, unavailable: null });
          return Promise.resolve([{
            name: k("GPU de demonstração"), architecture: k("Ada Lovelace"), shader_units: k(5888), vram_bytes: k(12 * GB),
            memory_bus_bits: k(192), bios: k("00.00.00"), driver: k("0.0"), pcie_link: k("PCIe 4.0 x16 (máx. PCIe 4.0 x16)"),
          }]);
        }
        case COMMANDS.getMemoryDetails: {
          const k = (v) => ({ value: v, unavailable: null });
          const mod = (slot, bank) => ({
            slot, bank, manufacturer: k("DemoRAM"), part_number: k("DEMO3200C16/16G"), serial: k("00000000"),
            kind: k("DDR4"), form_factor: k("DIMM"), capacity_bytes: 16 * 1024 ** 3,
            rated_mts: k(3600), configured_mts: k(3200), voltage_mv: k(1350),
          });
          const spd = { value: null, unavailable: "demonstração: fica no chip SPD do módulo" };
          return Promise.resolve({
            total_slots: k(4), modules: [mod("DIMM 0", "CHANNEL A"), mod("DIMM 0", "CHANNEL B")],
            timings: spd, command_rate: spd, profile: spd, channels: { value: null, unavailable: "demonstração" },
          });
        }
        case COMMANDS.getNetworkAdapters: {
          const k = (v) => ({ value: v, unavailable: null });
          return Promise.resolve([
            { name: "Ethernet", sensor_key: "ethernet", description: k("Adaptador de demonstração"), addresses: ["192.168.0.12"], connected: k(true), link_mbps: k(1000) },
            { name: "Wi-Fi", sensor_key: "wi_fi", description: k("Adaptador sem fio de demonstração"), addresses: [], connected: k(false), link_mbps: { value: null, unavailable: "desconectado" } },
          ]);
        }
        case COMMANDS.listSensors:
          return Promise.resolve(
            SENSORS.map(([id, group, name, unit, th]) => ({
              meta: { id, group, name, unit },
              threshold: th ? { warn: th[0], crit: th[1] } : null,
            })),
          );
        case COMMANDS.listRules:
          return Promise.resolve(structuredClone(rules));
        case COMMANDS.saveRule: {
          const rule = structuredClone(args.rule);
          if (!rule.name?.trim()) return Promise.reject(Object.assign(new Error("dê um nome à regra"), { code: "invalid_rule" }));
          rule.name = rule.name.trim();
          if (!rule.id) { rule.id = nextRuleId++; rules.push(rule); }
          else { const i = rules.findIndex((r) => r.id === rule.id); if (i < 0) return Promise.reject(new Error("a regra não existe mais")); rules[i] = rule; }
          return Promise.resolve(structuredClone(rule));
        }
        case COMMANDS.deleteRule:
          rules = rules.filter((r) => r.id !== args.id);
          return Promise.resolve(null);
        case COMMANDS.acknowledgeRule:
          return Promise.resolve(true);
        case COMMANDS.simulateRule: {
          const r = args.rule, hi = r.threshold + 10, lo = r.threshold - 25, now = Date.now();
          const vals = [...Array(3).fill(lo), ...Array(r.for_sec + 3).fill(hi), ...Array(r.recover_sec + 5).fill(lo)];
          const points = vals.map((v, i) => [now - (vals.length - 1 - i) * 1000, v]);
          const mk = (kind, i, detail) => ({ ts_ms: points[i][0], rule_id: r.id, rule_name: r.name, sensor: r.sensor, severity: r.severity, kind, detail, value: hi, is_test: true });
          const events = [mk("fired", 3 + r.for_sec, "valor simulado acima do limite"), mk("resolved", vals.length - 1, "voltou ao normal (simulado)")];
          emit("toast", { event: events[0], title: "[teste] " + r.name, body: events[0].detail });
          return Promise.resolve({ points, events });
        }
        case COMMANDS.setMute:
          mutedUntil = args.seconds ? Date.now() + args.seconds * 1000 : 0;
          return Promise.resolve(alertsState());
        case COMMANDS.setCriticalBypass:
          return Promise.resolve({ ...alertsState(), critical_bypasses_mute: args.on });
        case COMMANDS.listPresets:
          return Promise.resolve([
            { key: "basic", label: "Proteção básica", description: "O essencial para o PC não superaquecer nem travar.", recommended: true, default_actions: { toast: true, sound: false, notify: true }, rule_count: 2 },
            { key: "gaming", label: "Jogos e uso pesado", description: "Tolera picos de calor, avisa quando é prolongado.", recommended: false, default_actions: { toast: true, sound: true, notify: false }, rule_count: 2 },
          ]);
        case COMMANDS.previewPreset: {
          const k = args.sensitivity === "strict" ? -5 : args.sensitivity === "relaxed" ? 5 : 0;
          const mk = (key, name, sensor, thr, extra = {}) => ({
            key, why: "Motivo de demonstração.", available: !extra.off, unavailable_reason: extra.off ? "demonstração: sem leitura" : null, limit_source: "limite padrão",
            rule: { id: 0, name, sensor, mode: "sustained", op: "above", threshold: thr + k, hysteresis: thr + k - 8, for_sec: 15, count: 3, recover_sec: 5, repeat_sec: 60, severity: "crit", actions: args.actions ?? { toast: true, sound: false, notify: true }, enabled: true, preset: args.preset },
          });
          return Promise.resolve([mk("gpu_temp", "GPU superaquecendo", "gpu.0.temp", 85), mk("mem_high", "Memória quase cheia", "mem.used_pct", 90), mk("cpu_temp", "CPU superaquecendo", "cpu.temp", 90, { off: true })]);
        }
        case COMMANDS.applyPreset: {
          rules = rules.filter((r) => args.keepOwn && !r.preset);
          const plan = await this.invoke(COMMANDS.previewPreset, args);
          for (const p of plan) if (p.available && args.chosen.includes(p.key)) rules.push({ ...p.rule, id: nextRuleId++ });
          presetChoice = { label: args.preset === "basic" ? "Proteção básica" : "Jogos e uso pesado", sensitivity: args.sensitivity };
          return rules;
        }
        case COMMANDS.queryHistory: {
          const q = args.query;
          const all = historyLog.filter((e) => q.include_tests || !e.is_test);
          const list = all.filter((e) => (!q.severity || e.severity === q.severity) && (q.rule_id == null || e.rule_id === q.rule_id) && (!q.kinds.length || q.kinds.includes(e.kind)));
          return Promise.resolve({ events: list.slice(0, q.limit ?? 200), matching: list.length, total: all.length, critical_fired: list.filter((e) => e.severity === "crit" && e.kind === "fired").length, retention_days: retention });
        }
        case COMMANDS.exportHistory:
          return Promise.resolve("C:\\demo\\argus-historico.csv");
        case COMMANDS.clearHistory: {
          const n = historyLog.length;
          historyLog.length = 0;
          return Promise.resolve(n);
        }
        case COMMANDS.setHistoryRetention:
          retention = args.days;
          return this.invoke(COMMANDS.queryHistory, { query: { kinds: [], include_tests: false, limit: 200 } });
        case COMMANDS.getBackgroundState:
          return Promise.resolve({ ...background });
        case COMMANDS.setBackground:
          if (args.autostart != null) background.autostart = args.autostart;
          if (args.minimizeToTray != null) background.minimize_to_tray = args.minimizeToTray;
          return Promise.resolve({ ...background });
        case COMMANDS.getSessionRanges: {
          const out = [];
          for (const [sensor, r] of Object.entries(sessionRanges)) out.push({ sensor, min: r[0], max: r[1] });
          return Promise.resolve(out);
        }
        case COMMANDS.resetSessionRanges:
          for (const k of Object.keys(sessionRanges)) delete sessionRanges[k];
          return Promise.resolve(null);
        case COMMANDS.getFavorites:
          return Promise.resolve([...favs]);
        case COMMANDS.setFavorite:
          favs = favs.filter((s) => s !== args.sensor);
          if (args.on) favs.push(args.sensor);
          return Promise.resolve([...favs]);
        case COMMANDS.listProcesses: {
          const names = ["chrome.exe", "Code.exe", "argus-app.exe", "<img src=x onerror=alert(1)>.exe", "Discord.exe", "svchost.exe", "dwm.exe", "System", "explorer.exe", "node.exe"];
          return Promise.resolve(names.map((name, i) => ({ pid: 1000 + i * 137, name, cpu_percent: Math.max(0, 8 - i * 0.8 + (Math.random() - 0.5) * 3), memory_bytes: (2100 - i * 180) * 1e6 })));
        }
        case COMMANDS.getVolumes:
          return Promise.resolve([
            { mount: "C:\\", label: "Sistema", file_system: "NTFS", kind: "SSD", removable: false, total_bytes: 1e12, free_bytes: 5.8e11 },
            { mount: "D:\\", label: "", file_system: "NTFS", kind: "HDD", removable: false, total_bytes: 2e12, free_bytes: 1.9e12 },
          ]);
        case COMMANDS.getDisks: {
          const k = (v) => ({ value: v, unavailable: null });
          const na = (m) => ({ value: null, unavailable: m });
          return Promise.resolve([
            { index: 0, name: "Disco NVMe de demonstração", media: k("SSD"), bus: k("NVMe"), size_bytes: 1e12, firmware: k("1.0"), serial: k("DEMO0001"), health: k("Saudável"), temperature_c: k(44),
              smart: k([{ name: "Temperatura", value: "44 °C", level: "ok" }, { name: "Vida útil consumida", value: "12 %", level: "ok" }, { name: "Horas ligado", value: "3210", level: null }]),
              reliability: na("demonstração: exige administrador") },
            { index: 1, name: "<b>Disco SATA</b>", media: k("HDD"), bus: k("SATA"), size_bytes: 2e12, firmware: k("2.0"), serial: na("demonstração"), health: k("Saudável"), temperature_c: na("o Windows só entrega o SMART de discos SATA/ATA a um administrador"),
              smart: na("o Windows só entrega o SMART de discos SATA/ATA a um administrador"), reliability: na("demonstração: exige administrador") },
          ]);
        }
        case COMMANDS.getSettings:
          return Promise.resolve({ ...demoSettings });
        case COMMANDS.setSettings:
          Object.assign(demoSettings, args.settings);
          return Promise.resolve({ ...demoSettings });
        case COMMANDS.getSystemDetails: {
          const k = (v) => ({ value: v, unavailable: null });
          return Promise.resolve({
            board_vendor: k("Fabricante de demonstração"), board_model: k("<b>Placa</b> DEMO"), chipset: { value: null, unavailable: "o Windows não informa o chipset" },
            bios_vendor: k("DemoBIOS"), bios_version: k("F1"), bios_date: k("01/01/2024"),
            os_name: k("Windows 11 (demonstração)"), os_version: k("10.0.0"), os_build: k("0"), os_arch: k("64 bits"), uptime_secs: k(93_720),
          });
        }
        case COMMANDS.getReport:
          return Promise.resolve("Argus demonstração\nRelatório de exemplo\n  Modelo: <b>Placa</b> DEMO\n");
        case COMMANDS.exportSensorCsv:
          return Promise.resolve("C:\\demo\\argus-sensores.csv");
        case COMMANDS.getAlertsState:
          return Promise.resolve(alertsState());
        case COMMANDS.getHistory: {
          const from = Date.now() - args.windowMs;
          return Promise.resolve(
            args.sensorIds.map((sensor) => ({
              sensor,
              samples: log.filter((r) => r.sensor === sensor && r.ts_ms >= from).map(({ ts_ms, value }) => ({ ts_ms, value })),
            })),
          );
        }
        case COMMANDS.setSampleInterval:
          return Promise.resolve(null);
        default:
          return Promise.reject(new Error("comando desconhecido: " + name));
      }
    },
    onEvent(name, handler) {
      if (!listeners.has(name)) listeners.set(name, new Set());
      listeners.get(name).add(handler);
      return () => listeners.get(name).delete(handler);
    },
    onTick(handler) {
      handlers.add(handler);
      return () => handlers.delete(handler);
    },
  };
}
