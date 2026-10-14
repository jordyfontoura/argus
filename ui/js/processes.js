// Processes screen: live table from the OS. Names are put in with textContent (a process can be
// called anything).
import { api } from "./bridge.js";
import { fmt } from "./format.js";
import { COLUMNS, MAX_ROWS, formatMemory, orderRows } from "./processes-model.js";
import { showToast } from "./alerts.js";

const $ = (s) => document.querySelector(s);

const state = { rows: [], key: "cpu", dir: -1, frozen: null, timer: null, busy: false, hover: false };

function cell(tag, text, cls = "") {
  const el = document.createElement(tag);
  if (cls) el.className = cls;
  el.textContent = text;
  return el;
}

function head() {
  const tr = document.createElement("tr");
  for (const [key, label] of COLUMNS) {
    const th = cell("th", label + (state.key === key ? (state.dir < 0 ? " ▼" : " ▲") : ""), "sortable" + (key !== "name" ? " num" : ""));
    th.dataset.k = key;
    th.tabIndex = 0;
    th.setAttribute("role", "button");
    th.setAttribute("aria-sort", state.key === key ? (state.dir < 0 ? "descending" : "ascending") : "none");
    tr.append(th);
  }
  $("#phead").textContent = "";
  $("#phead").append(tr);
}

export function renderProcesses() {
  head();
  const query = $("#pf").value;
  const hovering = state.hover;
  const ordered = orderRows(state.rows, { key: state.key, dir: state.dir, query }, hovering ? state.frozen : null);
  const shown = ordered.slice(0, MAX_ROWS);
  // Remember what is on screen: if the mouse goes over the table, this is the order to keep.
  state.frozen = shown.map((r) => r.pid);
  const body = $("#pbody");
  body.textContent = "";
  if (!shown.length) {
    const tr = document.createElement("tr");
    const td = cell("td", state.rows.length ? `Nenhum processo encontrado para “${query}”.` : "Lendo os processos…", "meta");
    td.colSpan = 4;
    tr.append(td);
    body.append(tr);
    return;
  }
  for (const r of shown) {
    const tr = document.createElement("tr");
    tr.append(cell("td", r.name), cell("td", String(r.pid), "num mono"), cell("td", fmt(r.cpu_percent, 1), "num mono"), cell("td", formatMemory(r.memory_bytes), "num mono"));
    body.append(tr);
  }
  $("#pcount").textContent = `${ordered.length} de ${state.rows.length} processos` + (ordered.length > MAX_ROWS ? ` (mostrando ${MAX_ROWS})` : "");
}

async function refresh() {
  if (state.busy) return;
  state.busy = true;
  try {
    state.rows = await api.listProcesses();
    document.dispatchEvent(new CustomEvent("argus:rerender"));
  } catch (e) {
    showToast({ title: "Não foi possível ler os processos", body: e.message, event: { severity: "warn" } });
  } finally {
    state.busy = false;
  }
}

/** Sampling the processes costs CPU, so it only runs while this screen is open. */
export function setProcessesVisible(visible) {
  clearInterval(state.timer);
  state.timer = null;
  if (visible) {
    refresh();
    state.timer = setInterval(refresh, 2000);
  }
}

function sortBy(key) {
  state.dir = state.key === key ? -state.dir : key === "name" ? 1 : -1;
  state.key = key;
  state.frozen = null;
  document.dispatchEvent(new CustomEvent("argus:rerender"));
}
$("#phead").addEventListener("click", (e) => {
  const th = e.target.closest("th[data-k]");
  if (th) sortBy(th.dataset.k);
});
$("#phead").addEventListener("keydown", (e) => {
  const th = e.target.closest?.("th[data-k]");
  if (th && (e.key === "Enter" || e.key === " ")) {
    e.preventDefault();
    sortBy(th.dataset.k);
  }
});
$("#pf").addEventListener("input", () => {
  state.frozen = null;
  document.dispatchEvent(new CustomEvent("argus:rerender"));
});

// Hover is tracked from pointer events instead of :hover, which is not re-evaluated when the rows
// under a motionless cursor are replaced.
$("#pbody").addEventListener("pointerenter", () => { state.hover = true; });
$("#pbody").addEventListener("pointerleave", () => { state.hover = false; state.frozen = null; });
