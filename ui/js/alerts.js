// Alerts screen: rule list with live state, the rule editor, simulation and on-screen toasts.
// Everything that comes from the user (rule names, details) is put in with textContent or
// attributes set through the DOM, never through innerHTML.
import { api } from "./bridge.js";
import { niceMax, chart } from "./charts.js";
import { MODES, SEVERITY, duration, extras, liveState, sentence } from "./alerts-text.js";
import { fmt, present } from "./format.js";

const $ = (s) => document.querySelector(s);

/** Tiny DOM builder: h("div", { class: "x" }, "text", child). */
function h(tag, attrs = {}, ...children) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v == null || (v === false && !k.startsWith("aria-"))) continue;
    if (k === "class") el.className = v;
    else if (k === "text") el.textContent = v;
    else el.setAttribute(k, v === true && !k.startsWith("aria-") ? "" : String(v));
  }
  for (const c of children.flat()) if (c != null) el.append(c);
  return el;
}

const state = {
  rules: [],
  alerts: { statuses: [], muted_until_ms: 0, active_count: 0, critical_bypasses_mute: true },
  sims: new Map(),
  presetLabels: {},
  store: null,
  editing: null, // rule id being edited, 0 for a new rule, null when closed
  opener: null,
  confirming: null,
};

// ---------- metrics (sensors a rule can watch) ----------

/** Sensors worth alerting on: numeric readings that are not per-thread or per-adapter detail. */
export function metrics(store) {
  return [...store.catalog.values()]
    .filter((s) => s.meta.unit !== "count" && !/\.thread\.|^net\.(?!total)/.test(s.meta.id))
    .map((s) => ({
      id: s.meta.id,
      name: `${s.meta.group} · ${s.meta.name}`,
      unit: present(s.meta.unit, 1, true).unit, // thresholds are stored in canonical units
      info: s,
    }));
}

const metricOf = (store, id) =>
  metrics(store).find((m) => m.id === id) ?? { id, name: id, unit: "", info: null };

// ---------- list ----------

let listSig = "";

function card(rule, store) {
  const metric = metricOf(store, rule.sensor);
  const on = rule.enabled;
  const tags = [
    rule.actions.toast && "🔔 tela",
    rule.actions.sound && "🔊 som",
    rule.actions.notify && "🖥 sistema",
  ].filter(Boolean);
  const btn = (action, label, extra = {}) =>
    h("button", { class: "btn sm " + (extra.cls ?? ""), "data-a": action, "data-i": rule.id, hidden: extra.hidden, text: label });
  return h(
    "div",
    { class: `rcard ${rule.severity}`, "data-card": rule.id },
    h(
      "div",
      { class: "rhead" },
      h("div", { class: "sw2" + (on ? " on" : ""), role: "switch", tabindex: "0", "aria-checked": on, "aria-label": `Ativar regra ${rule.name}`, "data-a": "tog", "data-i": rule.id }),
      h("span", { class: "n", text: rule.name }),
      rule.preset ? h("span", { class: "tag", text: "🛡 " + (state.presetLabels[rule.preset] ?? "Perfil") }) : null,
      h("span", { class: "tag", text: SEVERITY[rule.severity] }),
      h("span", { class: "st", "data-role": "st" }),
    ),
    h("div", { class: "sent" }, sentence(rule, metric), h("br"), h("span", { style: "font-size:12px", text: extras(rule, metric.unit) })),
    h("div", { class: "rlive", "data-role": "live" }),
    h(
      "div",
      { class: "chips" },
      tags.map((t) => h("span", { class: "tag", text: t })),
      h("span", { class: "sp" }),
      btn("ack", "Reconhecer", { hidden: true }),
      btn("sim", "Simular"),
      btn("edit", "Editar"),
      btn("dup", "Duplicar"),
      state.confirming === rule.id
        ? [btn("del-yes", "Confirmar exclusão", { cls: "dng" }), btn("del-no", "Cancelar")]
        : btn("del", "Excluir", { cls: "dng" }),
    ),
    h("div", { "data-role": "sim" }),
  );
}

function updateLive(el, rule, store) {
  const metric = metricOf(store, rule.sensor);
  const status = state.alerts.statuses.find((s) => s.rule_id === rule.id);
  const live = liveState(rule, status);
  el.className = `rcard ${rule.severity} ${live.card}`.trim();
  const st = el.querySelector('[data-role="st"]');
  st.className = "st " + live.css;
  st.textContent = live.label;

  const box = el.querySelector('[data-role="live"]');
  box.textContent = "";
  const v = store.value(rule.sensor);
  const unit = metric.unit;
  const cur = h("b", { class: "mono", style: "color:var(--text)", text: v != null ? `${fmt(v, 1)} ${unit}`.trim() : "—" });
  box.append("Atual ", cur);
  if (v == null) box.append(` · sem leitura: ${store.reason(rule.sensor) ?? ""}`);
  if (status?.phase === "pending" && rule.mode === "sustained" && status.needed_ms > 0) {
    const pct = Math.min(100, (status.elapsed_ms / status.needed_ms) * 100);
    box.append(h("div", { class: "bar" }, h("i", { style: `width:${pct}%;background:var(--warn)` })));
  }
  if (status && (status.phase === "firing" || status.phase === "acknowledged")) {
    box.append(" · há ", duration(Math.round(status.elapsed_ms / 1000)), " · pico ", h("b", { class: "mono", style: "color:var(--bad)", text: `${fmt(status.peak ?? 0, 1)} ${unit}`.trim() }));
  }
  if (rule.enabled && rule.mode === "spikes" && status) box.append(` · ${status.occurrences}/${rule.count} ocorrências`);
  el.querySelector('[data-a="ack"]').hidden = !live.canAck;
}

function renderSim(el, rule, store) {
  const box = el.querySelector('[data-role="sim"]');
  const sim = state.sims.get(rule.id);
  box.textContent = "";
  if (!sim) return;
  const metric = metricOf(store, rule.sensor);
  const W = 560, H = 90;
  const vals = sim.points.map((p) => p[1]);
  const lo = Math.min(...vals, rule.threshold), hi = Math.max(...vals, rule.threshold);
  const t0 = sim.points[0][0], t1 = sim.points.at(-1)[0];
  const x = (t) => 6 + ((W - 12) * (t - t0)) / Math.max(1, t1 - t0);
  const y = (v) => H - 8 - ((H - 16) * (v - lo)) / Math.max(1e-9, hi - lo);
  const NS = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(NS, "svg");
  svg.setAttribute("viewBox", `0 0 ${W} ${H}`);
  svg.setAttribute("class", "chart");
  svg.setAttribute("style", "height:90px");
  svg.setAttribute("role", "img");
  svg.setAttribute("aria-label", "Série sintética da simulação");
  const line = document.createElementNS(NS, "path");
  line.setAttribute("d", sim.points.map((p, i) => (i ? "L" : "M") + x(p[0]).toFixed(1) + "," + y(p[1]).toFixed(1)).join(""));
  line.setAttribute("fill", "none");
  line.setAttribute("stroke", "var(--cpu)");
  line.setAttribute("stroke-width", "1.6");
  const th = document.createElementNS(NS, "line");
  th.setAttribute("x1", 0); th.setAttribute("x2", W);
  th.setAttribute("y1", y(rule.threshold)); th.setAttribute("y2", y(rule.threshold));
  th.setAttribute("stroke", "var(--bad)"); th.setAttribute("stroke-dasharray", "5 4");
  svg.append(th, line);
  for (const ev of sim.events) {
    const m = document.createElementNS(NS, "line");
    m.setAttribute("x1", x(ev.ts_ms)); m.setAttribute("x2", x(ev.ts_ms));
    m.setAttribute("y1", 0); m.setAttribute("y2", H);
    m.setAttribute("stroke", ev.kind === "resolved" ? "var(--ok)" : "var(--bad)");
    svg.append(m);
  }
  box.append(
    h("div", { class: "meta", style: "margin-top:8px" }, `Simulação com dados sintéticos (${metric.name}). Os gráficos e sensores reais não foram alterados. `, h("button", { class: "btn sm", "data-a": "sim-close", "data-i": rule.id, text: "Fechar simulação" })),
    svg,
  );
}

export function renderAlerts(store, liveAlerts = state.alerts) {
  state.store = store;
  state.alerts = liveAlerts;
  const list = $("#rlist");
  const sig = JSON.stringify([state.rules, state.confirming, [...state.sims.keys()]]);
  if (sig !== listSig) {
    listSig = sig;
    list.textContent = "";
    if (!state.rules.length) {
      list.append(h("div", { class: "card meta", text: "Nenhuma regra. Crie a primeira em “+ Nova regra”." }));
    }
    for (const rule of state.rules) {
      const el = card(rule, store);
      renderSim(el, rule, store);
      list.append(el);
    }
  }
  for (const rule of state.rules) {
    const el = list.querySelector(`[data-card="${rule.id}"]`);
    if (el) updateLive(el, rule, store);
  }
  const active = state.rules.filter((r) => r.enabled).length;
  const firing = state.alerts.active_count;
  $("#asum").textContent = "";
  $("#asum").append(h("b", { text: active }), ` ativas de ${state.rules.length} · `, h("b", { style: `color:${firing ? "var(--bad)" : "var(--ok)"}`, text: firing }), " disparadas");
  if (state.alerts.muted_until_ms > Date.now()) $("#asum").append(" · ", h("span", { style: "color:var(--warn)", text: "silenciado" }));
}

export function setAlertsState(value) {
  state.alerts = value;
  $("#presetinfo").textContent = value.preset_label
    ? `🛡 Perfil: ${value.preset_label} · ${{ relaxed: "Relaxado", normal: "Normal", strict: "Rigoroso" }[value.preset_sensitivity] ?? ""}`
    : "Nenhum perfil aplicado";
  // Keep the controls in step with the backend (the mute expires on its own).
  const muted = value.muted_until_ms > Date.now();
  if (!muted && $("#mute").value !== "0") $("#mute").value = "0";
  $("#crit-bypass").checked = value.critical_bypasses_mute;
  const hb = $("#hbadge");
  if (hb) {
    hb.hidden = !value.history_count;
    hb.textContent = String(value.history_count);
  }
  const badge = $("#abadge");
  if (badge) {
    badge.hidden = !value.active_count;
    badge.textContent = String(value.active_count);
  }
}

export async function refreshRules() {
  state.rules = await api.listRules();
  document.dispatchEvent(new CustomEvent("argus:rules-changed", { detail: state.rules }));
  if (!Object.keys(state.presetLabels).length) {
    try {
      state.presetLabels = Object.fromEntries((await api.listPresets()).map((p) => [p.key, p.label]));
    } catch { /* labels are cosmetic */ }
  }
}

export const ruleCount = () => state.rules.length;

// ---------- actions on cards ----------

async function run(action, rule) {
  try {
    if (action === "tog") await api.saveRule({ ...rule, enabled: !rule.enabled });
    else if (action === "ack") await api.acknowledgeRule(rule.id);
    else if (action === "dup") await api.saveRule({ ...rule, id: 0, name: `${rule.name} (cópia)` });
    else if (action === "del-yes") {
      await api.deleteRule(rule.id);
      state.confirming = null;
      state.sims.delete(rule.id);
    } else if (action === "sim") state.sims.set(rule.id, await api.simulateRule(rule));
    await refreshRules();
  } catch (e) {
    showToast({ title: "Não foi possível concluir", body: e.message, event: { severity: "warn" } });
  }
  document.dispatchEvent(new CustomEvent("argus:rerender"));
}

document.addEventListener("click", (e) => {
  const b = e.target.closest("#rlist [data-a]");
  if (!b) return;
  const rule = state.rules.find((r) => r.id === Number(b.dataset.i));
  const action = b.dataset.a;
  if (!rule) return;
  if (action === "edit") return openEditor(rule, b);
  if (action === "del") { state.confirming = rule.id; return document.dispatchEvent(new CustomEvent("argus:rerender")); }
  if (action === "del-no") { state.confirming = null; return document.dispatchEvent(new CustomEvent("argus:rerender")); }
  if (action === "sim-close") { state.sims.delete(rule.id); return document.dispatchEvent(new CustomEvent("argus:rerender")); }
  run(action, rule);
});
document.addEventListener("keydown", (e) => {
  const sw = e.target.closest?.('#rlist [role="switch"]');
  if (sw && (e.key === " " || e.key === "Enter")) {
    e.preventDefault();
    sw.click();
  }
});

// ---------- toasts ----------

export function showToast({ title, body, event }) {
  const box = $("#toasts");
  const sev = event?.severity === "crit" ? "crit" : event?.kind === "resolved" ? "res" : event?.severity === "info" ? "info" : "";
  const el = h("div", { class: "toast " + sev }, h("b", { text: title }), h("small", { text: body }));
  box.append(el);
  while (box.children.length > 4) box.firstChild.remove();
  setTimeout(() => el.remove(), 6000);
}

// ---------- editor ----------

function setDuration(sec) {
  if (sec >= 60 && sec % 60 === 0) {
    $("#f-dur").value = sec / 60;
    $("#f-durU").value = "60";
  } else {
    $("#f-dur").value = sec;
    $("#f-durU").value = "1";
  }
}

function defaultRule(store) {
  const first = metrics(store).find((m) => m.info?.threshold) ?? metrics(store)[0];
  return {
    id: 0, name: "", sensor: first?.id ?? "", mode: "sustained", op: "above",
    threshold: first?.info?.threshold?.warn ?? 80, hysteresis: null, for_sec: 60, count: 3, recover_sec: 5,
    repeat_sec: 0, severity: "warn", actions: { toast: true, sound: false, notify: false }, enabled: true,
  };
}

function openEditor(rule, opener) {
  const store = state.store;
  state.editing = rule ? rule.id : 0;
  state.opener = opener ?? document.activeElement;
  const d = rule ?? defaultRule(store);
  $("#mt").textContent = rule ? "Editar regra" : "Nova regra";
  const sel = $("#f-metric");
  sel.textContent = "";
  for (const m of metrics(store)) sel.append(h("option", { value: m.id, text: `${m.name}${m.unit ? ` (${m.unit})` : ""}` }));
  if (![...sel.options].some((o) => o.value === d.sensor)) sel.append(h("option", { value: d.sensor, text: d.sensor }));
  $("#f-name").value = d.name;
  sel.value = d.sensor;
  $("#f-mode").value = d.mode;
  $("#f-op").value = d.op;
  $("#f-val").value = d.threshold;
  $("#f-count").value = d.count;
  setDuration(d.for_sec);
  $("#f-rec").value = d.recover_sec;
  $("#f-hyst").value = d.hysteresis ?? "";
  $("#f-rep").value = String(d.repeat_sec);
  $("#f-sev").value = d.severity;
  $("#a-toast").checked = d.actions.toast;
  $("#a-sound").checked = d.actions.sound;
  $("#a-notify").checked = d.actions.notify;
  $("#f-err").textContent = "";
  $("#ovl").classList.add("show");
  syncForm();
  $("#f-name").focus();
}

export function openNewRule(opener) {
  openEditor(null, opener);
}

function closeEditor() {
  $("#ovl").classList.remove("show");
  state.editing = null;
  state.opener?.focus?.();
}

function readForm() {
  const current = state.rules.find((r) => r.id === state.editing);
  const unit = Number($("#f-durU").value);
  const mode = $("#f-mode").value;
  const dur = Math.round((Number($("#f-dur").value) || 0) * unit);
  return {
    id: state.editing || 0,
    name: $("#f-name").value.trim(),
    sensor: $("#f-metric").value,
    mode,
    op: $("#f-op").value,
    threshold: Number($("#f-val").value),
    hysteresis: $("#f-hyst").value === "" ? null : Number($("#f-hyst").value),
    for_sec: Math.max(mode === "sustained" ? 0 : 1, dur),
    count: Math.max(2, Number($("#f-count").value) || 2),
    recover_sec: Math.max(0, Number($("#f-rec").value) || 0),
    repeat_sec: Number($("#f-rep").value),
    severity: $("#f-sev").value,
    actions: { toast: $("#a-toast").checked, sound: $("#a-sound").checked, notify: $("#a-notify").checked },
    enabled: current ? current.enabled : true,
  };
}

function syncForm() {
  const store = state.store;
  const f = readForm();
  const m = metricOf(store, f.sensor);
  $("#f-unit").textContent = m.unit ? `(${m.unit})` : "";
  $("#l-hyst").textContent = f.op === "above" ? "abaixo" : "acima";
  $("#l-hystw").hidden = f.mode === "spikes";
  $("#l-count").hidden = f.mode !== "spikes";
  $("#l-dur").textContent = f.mode === "average" ? "Janela da média" : f.mode === "spikes" ? "Dentro de um período de" : "Por quanto tempo (seguido)";
  $("#f-sent").textContent = "Avisar quando: " + sentence(f, m) + ".";
  // Preview of the limit over the metric's real recent history.
  const endMs = store.latestTs();
  const pts = store.window(f.sensor, 5 * 60_000, endMs).filter((p) => p.a.kind === "ok");
  const peak = Math.max(0, ...pts.map((p) => p.a.value), Number.isFinite(f.threshold) ? f.threshold : 0);
  chart("#mchart", [{ id: f.sensor, color: "cpu", name: m.name, unitLabel: ` ${m.unit}` }], {
    store, windowMs: 5 * 60_000, endMs, max: niceMax(peak), line: Number.isFinite(f.threshold) ? f.threshold : null,
  });
  $("#f-prevnote").textContent = pts.length ? "" : "Ainda sem histórico desta métrica para mostrar.";
}

$("#f-metric").addEventListener("change", () => {
  const info = metricOf(state.store, $("#f-metric").value).info;
  if (info?.threshold) $("#f-val").value = info.threshold.warn;
  syncForm();
});
$("#ovl").addEventListener("input", syncForm);
$("#ovl").addEventListener("change", syncForm);
$("#mcancel").addEventListener("click", closeEditor);
$("#ovl").addEventListener("click", (e) => { if (e.target.id === "ovl") closeEditor(); });

$("#msave").addEventListener("click", async () => {
  if ($("#f-val").value === "") {
    $("#f-err").textContent = "Informe o limite da regra.";
    $("#f-val").focus();
    return;
  }
  const rule = readForm();
  if (!rule.name) rule.name = sentence(rule, metricOf(state.store, rule.sensor));
  try {
    await api.saveRule(rule);
    await refreshRules();
    closeEditor();
    document.dispatchEvent(new CustomEvent("argus:rerender"));
  } catch (e) {
    $("#f-err").textContent = e.message;
  }
});

$("#msim").addEventListener("click", async () => {
  const rule = readForm();
  if (!rule.name) rule.name = sentence(rule, metricOf(state.store, rule.sensor));
  try {
    const sim = await api.simulateRule(rule);
    if (rule.id) state.sims.set(rule.id, sim);
    $("#f-err").textContent = "";
    showToast({ title: "Simulação executada", body: rule.id ? "Veja o resultado no cartão da regra." : "Salve a regra para ver a simulação no cartão.", event: { severity: "info" } });
  } catch (e) {
    $("#f-err").textContent = e.message;
  }
});

// Keyboard: Escape closes, Tab stays inside the dialog.
document.addEventListener("keydown", (e) => {
  const modal = $("#ovl");
  if (!modal.classList.contains("show")) return;
  if (e.key === "Escape") {
    e.preventDefault();
    closeEditor();
  } else if (e.key === "Tab") {
    const focusable = [...modal.querySelectorAll("input,select,button,textarea,[tabindex]:not([tabindex='-1'])")].filter(
      (el) => !el.disabled && !el.closest("[hidden]") && el.offsetParent !== null,
    );
    if (!focusable.length) return;
    const first = focusable[0], last = focusable.at(-1);
    if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
    else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
    else if (!modal.contains(document.activeElement)) { e.preventDefault(); first.focus(); }
  }
});

// Fill the mode select once.
for (const [value, text] of Object.entries(MODES)) $("#f-mode").append(h("option", { value, text }));

export function editorOpen() {
  return state.editing !== null;
}

$("#mute").addEventListener("change", async (e) => {
  try {
    setAlertsState(await api.setMute(Number(e.target.value)));
  } catch (err) {
    showToast({ title: "Não foi possível silenciar", body: err.message, event: { severity: "warn" } });
  }
});
$("#crit-bypass").addEventListener("change", async (e) => {
  try {
    setAlertsState(await api.setCriticalBypass(e.target.checked));
  } catch (err) {
    showToast({ title: "Não foi possível salvar", body: err.message, event: { severity: "warn" } });
  }
});
