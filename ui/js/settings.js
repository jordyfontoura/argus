// Settings screen. Only controls that really do something are shown: collection and display
// (prefs.js, kept by the backend) and the background behaviour (autostart, tray).
import { api, isDemo } from "./bridge.js";
import { INTERVALS, loadPrefs, prefs, updatePrefs } from "./prefs.js";
import { showToast } from "./alerts.js";

const $ = (s) => document.querySelector(s);

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

let current = null;
let activating = false;
let activationMessage = "Ativa ou atualiza temperatura, tensão e potência disponíveis. O Windows pede permissão de administrador uma vez; o Argus continua sem administrador.";

function renderCpuActivation() {
  const box = $("#cpusettings");
  box.textContent = "";
  const button = h("button", { class: "btn", type: "button", text: activating ? "Ativando…" : "Ativar / atualizar leitura", disabled: activating || isDemo });
  button.addEventListener("click", async () => {
    if (activating) return;
    activating = true;
    renderCpuActivation();
    try { activationMessage = await api.activateCpuMeasurements(); }
    catch (e) { activationMessage = e.message; }
    finally { activating = false; renderCpuActivation(); }
  });
  box.append(h("div", { class: "rule" }, h("div", { class: "n" }, "Leitura de baixo nível", h("div", { class: "meta", role: "status", text: activationMessage })), button));
  box.append(h("div", { class: "meta", text: "Sem ativação, os outros sensores continuam funcionando. PawnIO é compartilhado com outros programas e permanece instalado ao remover o Argus." }));
}

function row(key, title, description, on) {
  return h(
    "div",
    { class: "rule" },
    h("div", { class: "n" }, title, h("div", { class: "meta", text: description })),
    h("div", { class: "sw2" + (on ? " on" : ""), role: "switch", tabindex: "0", "aria-checked": on, "aria-label": title, "data-key": key }),
  );
}

const RETENTION = [7, 30, 90, 365];

function selectRow(key, title, description, options, value) {
  const select = h("select", { "data-pref": key, "aria-label": title });
  for (const [v, label] of options) select.append(h("option", { value: v, text: label, selected: String(v) === String(value) }));
  select.value = String(value);
  return h("div", { class: "rule" }, h("div", { class: "n" }, title, h("div", { class: "meta", text: description })), select);
}

function renderDisplay() {
  const box = $("#dispsettings");
  const p = prefs();
  box.textContent = "";
  const days = [...new Set([...RETENTION, p.history_retention_days])].sort((a, b) => a - b);
  box.append(
    selectRow("sample_interval_ms", "Intervalo de coleta", "De quanto em quanto tempo os sensores são lidos. Valores menores gastam um pouco mais de CPU.", INTERVALS, p.sample_interval_ms),
    selectRow("temperature_unit", "Unidade de temperatura", "Só muda como os valores aparecem; alertas e histórico continuam em °C.", [["C", "°C"], ["F", "°F"]], p.temperature_unit),
    selectRow("memory_unit", "Unidade de memória", "GB usa 1000 (como os fabricantes), GiB usa 1024 (como o Windows mostra).", [["GB", "GB"], ["GiB", "GiB"]], p.memory_unit),
    row("light_theme", "Tema claro", "Alterna entre o tema escuro e o claro.", p.light_theme),
    selectRow("history_retention_days", "Guardar histórico de alertas", "Eventos mais antigos são apagados automaticamente.", days.map((d) => [d, d === 365 ? "1 ano" : d + " dias"]), p.history_retention_days),
  );
}

export function renderSettings() {
  renderCpuActivation();
  renderDisplay();
  const box = $("#bgsettings");
  box.textContent = "";
  if (!current) {
    box.append(h("div", { class: "meta", text: "Carregando…" }));
    return;
  }
  box.append(
    row("autostart", "Iniciar com o Windows", "O Argus abre em segundo plano (na bandeja) quando você entra no Windows.", current.autostart),
    row("minimize_to_tray", "Minimizar para a bandeja ao fechar", "Os alertas continuam rodando com a janela fechada. Para encerrar de vez, use Sair no menu da bandeja.", current.minimize_to_tray),
  );
}

export async function loadSettings() {
  try {
    await loadPrefs();
    current = await api.backgroundState();
  } catch (e) {
    showToast({ title: "Configurações indisponíveis", body: e.message, event: { severity: "warn" } });
  }
  renderSettings();
}

async function toggle(sw) {
  const key = sw.dataset.key;
  const on = !(sw.getAttribute("aria-checked") === "true");
  try {
    if (key === "light_theme") await updatePrefs({ light_theme: on });
    else current = await api.setBackground({ [key]: on });
  } catch (e) {
    showToast({ title: "Não foi possível salvar", body: e.message, event: { severity: "warn" } });
  }
  renderSettings();
  document.querySelector("#v-cfg [data-key=" + key + "]")?.focus();
}

for (const id of ["#bgsettings", "#dispsettings"]) {
  $(id).addEventListener("click", (e) => {
    const sw = e.target.closest("[data-key]");
    if (sw) toggle(sw);
  });
  $(id).addEventListener("keydown", (e) => {
    const sw = e.target.closest?.("[data-key]");
    if (sw && (e.key === " " || e.key === "Enter")) {
      e.preventDefault();
      e.stopPropagation();
      toggle(sw);
    }
  });
}

$("#dispsettings").addEventListener("change", async (e) => {
  const key = e.target.dataset?.pref;
  if (!key) return;
  const value = key === "temperature_unit" || key === "memory_unit" ? e.target.value : Number(e.target.value);
  try {
    await updatePrefs({ [key]: value });
  } catch (err) {
    showToast({ title: "Não foi possível salvar", body: err.message, event: { severity: "warn" } });
  }
  renderDisplay();
  document.querySelector("#dispsettings [data-pref=" + key + "]")?.focus();
});
