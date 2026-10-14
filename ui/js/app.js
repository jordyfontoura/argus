import { api, EVENTS, isDemo, onEvent, onTick } from "./bridge.js";
import { Store } from "./store.js";
import { renderOverview } from "./overview.js";
import { renderCpu } from "./cpu.js";
import { renderGpu } from "./gpu.js";
import { renderMemory } from "./memory.js";
import { renderNetwork } from "./network.js";
import { openNewRule, refreshRules, renderAlerts, ruleCount, setAlertsState, showToast } from "./alerts.js";
import { initWizard, maybeAutoOpen } from "./wizard.js";
import { refreshHistory, setHistoryVisible, setRules } from "./history.js";
import { loadSettings, renderSettings } from "./settings.js";
import { renderSensors, setSensorsVisible } from "./sensors.js";
import { loadFavorites } from "./favorites.js";
import { renderProcesses, setProcessesVisible } from "./processes.js";
import { renderStorage, setStorageVisible } from "./storage.js";
import { loadSystem, renderSystem } from "./system.js";
import { initHeader } from "./header.js";
import { loadPrefs } from "./prefs.js";

const $ = (s) => document.querySelector(s);
const store = new Store();
// `frozenEnd` is the moment a paused screen keeps showing; 0 means "follow live data".
const state = { windowMs: 60_000, view: "overview", paused: false, frozenEnd: 0 };
const details = { cpu: null, gpus: null, memory: null, adapters: null };

// Screens that exist so far. A screen is added to the menu only when it works with real data.
const VIEWS = [
  ["overview", "Visão geral", "◧", (s) => renderOverview(store, s)],
  ["cpu", "CPU", "▦", (s) => renderCpu(store, s, details.cpu)],
  ["gpu", "GPU", "▣", (s) => renderGpu(store, s, details.gpus)],
  ["mem", "Memória", "▤", (s) => renderMemory(store, s, details.memory)],
  ["disk", "Armazenamento", "◉", (s) => renderStorage(store, s)],
  ["net", "Rede", "⇅", (s) => renderNetwork(store, s, details.adapters)],
  ["sys", "Sistema", "ⓘ", () => renderSystem(details.memory)],
  ["proc", "Processos", "☰", () => renderProcesses()],
  ["sens", "Sensores", "♨", (s) => renderSensors(store.view(s.endMs))],
  ["alerts", "Alertas", "⚑", () => renderAlerts(store)],
  ["hist", "Histórico", "☷", () => {}],
  ["cfg", "Configurações", "⚙", () => renderSettings()],
];

$("#nav").innerHTML = VIEWS.map(
  ([id, name, icon]) => `<button data-v="${id}" aria-label="${name}" title="${name}"><span aria-hidden="true">${icon}</span><em>${name}</em>${id === "alerts" ? '<span class="badge" id="abadge" hidden>0</span>' : ""}${id === "hist" ? '<span class="badge" id="hbadge" style="background:var(--panel2);color:var(--mute)" hidden>0</span>' : ""}</button>`,
).join("");

let scheduled = false;
function render() {
  scheduled = false;
  const endMs = state.paused ? state.frozenEnd : store.latestTs();
  VIEWS.find((v) => v[0] === state.view)[3]({ windowMs: state.windowMs, endMs });
}
const schedule = () => {
  if (!scheduled) {
    scheduled = true;
    requestAnimationFrame(render);
  }
};

function go(id) {
  const view = VIEWS.find((v) => v[0] === id);
  if (!view) return;
  state.view = id;
  document.querySelectorAll("nav button").forEach((b) => {
    b.classList.toggle("on", b.dataset.v === id);
    if (b.dataset.v === id) b.setAttribute("aria-current", "page");
    else b.removeAttribute("aria-current");
  });
  document.querySelectorAll("section.view").forEach((s) => s.classList.toggle("on", s.id === "v-" + id));
  $("#title").textContent = view[1];
  if (id === "net") refreshAdapters();
  setHistoryVisible(id === "hist");
  setSensorsVisible(id === "sens");
  setProcessesVisible(id === "proc");
  setStorageVisible(id === "disk");
  if (id === "sys") loadSystem();
  if (id === "cfg") loadSettings();
  schedule();
}

// Adapters come and go (Wi-Fi off, VPN up), so the list is re-read while its screen is open.
async function refreshAdapters() {
  try {
    details.adapters = await api.networkAdapters();
    schedule();
  } catch (e) {
    showError("Não foi possível listar os adaptadores de rede: " + e.message);
  }
}
setInterval(() => state.view === "net" && !state.paused && refreshAdapters(), 5000);
$("#nav").addEventListener("click", (e) => {
  const b = e.target.closest("button");
  if (b) go(b.dataset.v);
});
document.addEventListener("argus:rerender", schedule);
window.addEventListener("resize", schedule);

function showError(message) {
  $("#toasts").textContent = "";
  const toast = document.createElement("div");
  toast.className = "toast crit";
  toast.textContent = message;
  $("#toasts").append(toast);
}

initHeader(state, {
  latestTs: () => store.latestTs(),
  onChange: schedule,
  onView: (i) => VIEWS[i] && go(VIEWS[i][0]),
  onStep: (d) => {
    const i = VIEWS.findIndex((v) => v[0] === state.view);
    go(VIEWS[(i + d + VIEWS.length) % VIEWS.length][0]);
  },
  onError: (message) => showError(message),
});

$("#newrule").addEventListener("click", (e) => openNewRule(e.currentTarget));
initWizard({
  store: () => store,
  onCustom: () => { go("alerts"); openNewRule(null); },
  onDone: async (label, count) => {
    await refreshRules();
    setAlertsState(await api.alertsState());
    go("alerts");
    showToast({ title: `🛡 ${label} ativada`, body: `${count} alertas criados. Você pode editar cada um na lista.`, event: { severity: "info", kind: "resolved" } });
  },
});

async function start() {
  $("#demo").hidden = !isDemo;
  await onEvent(EVENTS.alertsState, (value) => {
    setAlertsState(value);
    if (state.view === "alerts") schedule();
  });
  await onEvent(EVENTS.toast, showToast);
  await onEvent(EVENTS.alertsEvent, () => state.view === "hist" && refreshHistory());
  document.addEventListener("argus:rules-changed", (e) => setRules(e.detail));
  document.addEventListener("argus:history-changed", async () => setAlertsState(await api.alertsState()));
  // Listen first so no tick is lost while history is being fetched.
  await onTick((readings) => {
    store.push(readings);
    schedule();
  });
  try {
    await loadPrefs();
  } catch (e) {
    showError("Não foi possível ler as configurações; usando os padrões: " + e.message);
  }
  try {
    const [info, catalog, cpu, gpus, memory] = await Promise.all([
      api.staticInfo(),
      api.listSensors(),
      api.cpuDetails(),
      api.gpuDetails(),
      api.memoryDetails(),
    ]);
    store.setCatalog(catalog);
    details.cpu = cpu;
    details.gpus = gpus;
    details.memory = memory;
    const field = (f, fallback) => f.value ?? fallback;
    $("#host").textContent = `${field(info.host, "computador")} · ${field(info.os, "Windows")}`;
    $("#foot").textContent = isDemo ? "demonstração" : "Argus 0.2";
    store.seed(await api.history(catalog.map((s) => s.meta.id), 15 * 60_000));
    await refreshRules();
    await loadFavorites();
    setRules(await api.listRules());
    const alertsState = await api.alertsState();
    setAlertsState(alertsState);
    maybeAutoOpen({ length: ruleCount() }, alertsState.preset_label);
  } catch (e) {
    showError("Não foi possível ler o hardware: " + e.message);
  }
  go(state.view);
}

start();
