// Storage screen: volumes (usage), total disk throughput, and each physical disk with its health.
// What Windows will not tell a standard user is shown as "indisponível" with the reason.
import { api } from "./bridge.js";
import { chart } from "./charts.js";
import { bytesText, fmt, memUnit, withUnit } from "./format.js";
import { field } from "./fields.js";
import { showToast } from "./alerts.js";

const $ = (s) => document.querySelector(s);

function h(tag, attrs = {}, ...children) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v == null || (v === false && !k.startsWith("aria-"))) continue;
    if (k === "class") el.className = v;
    else if (k === "text") el.textContent = v;
    else if (k === "html") el.innerHTML = v;
    else el.setAttribute(k, v === true && !k.startsWith("aria-") ? "" : String(v));
  }
  for (const c of children.flat()) if (c != null) el.append(c);
  return el;
}

const state = { volumes: [], disks: [], selected: 0, timer: null };

const gb = (bytes) => {
  const g = bytes / 1e9;
  return g >= 1000 && memUnit() === "GB" ? `${fmt(g / 1000, 2)} TB` : bytesText(bytes, g < 10 ? 1 : 0);
};

async function refresh() {
  try {
    [state.volumes, state.disks] = await Promise.all([api.volumes(), api.disks()]);
    document.dispatchEvent(new CustomEvent("argus:rerender"));
  } catch (e) {
    showToast({ title: "Não foi possível ler o armazenamento", body: e.message, event: { severity: "warn" } });
  }
}

/** Disk queries are not free, so they run only while this screen is open. */
export function setStorageVisible(visible) {
  clearInterval(state.timer);
  state.timer = null;
  if (visible) {
    refresh();
    state.timer = setInterval(refresh, 10_000);
  }
}

function volumeCard(v, busy) {
  const used = v.total_bytes - v.free_bytes;
  const pct = v.total_bytes ? (used / v.total_bytes) * 100 : 0;
  const color = pct >= 95 ? "var(--bad)" : pct >= 85 ? "var(--warn)" : "var(--disk)";
  const name = `${v.mount.replace(/\\$/, "")}${v.label ? " " + v.label : ""}`;
  return h(
    "div",
    { class: "card" },
    h("h3", {}, name, " ", h("span", { class: "tag", text: [v.kind, v.file_system].filter(Boolean).join(" · ") })),
    h("div", { class: "big mono" }, String(Math.round(pct)), h("small", { text: "% usado" })),
    h("div", { class: "meta", text: `${gb(used)} de ${gb(v.total_bytes)} · livre ${gb(v.free_bytes)}${busy != null ? ` · atividade do disco ${fmt(busy, 0)}%` : ""}` }),
    h("div", { class: "bar", style: "margin-top:10px" }, h("i", { style: `width:${Math.min(100, pct)}%;background:${color}` })),
  );
}

function kv(rows) {
  const dl = h("dl", { class: "kv" });
  for (const [label, html, mono] of rows) {
    dl.append(h("dt", { text: label }), h("dd", { class: mono ? "mono" : "", html }));
  }
  return dl;
}

function smartPanel(d) {
  const box = h("div", {});
  const attrs = d.smart.value;
  if (!attrs) {
    box.append(
      h("div", { class: "health" }, h("span", { class: "lv na" }), h("b", { text: "SMART indisponível" })),
      h("p", { class: "meta", text: d.smart.unavailable ?? "sem informação" }),
    );
  } else {
    const worst = attrs.some((a) => a.level === "crit") ? "crit" : attrs.some((a) => a.level === "warn") ? "warn" : "ok";
    const verdict = { ok: "Sem sinais de problema nos itens avaliados", warn: "Atenção: há itens fora do ideal", crit: "Problema: há itens em estado crítico" }[worst];
    box.append(h("div", { class: "health" }, h("span", { class: "lv " + (worst === "crit" ? "bad" : worst === "warn" ? "warn" : "") }), h("b", { text: verdict })));
    const table = h("table", {}, h("tr", {}, h("th", { text: "Item" }), h("th", { class: "num", text: "Valor" }), h("th", { text: "" })));
    for (const a of attrs) {
      table.append(
        h(
          "tr",
          {},
          h("td", { text: a.name }),
          h("td", { class: "num mono", text: a.value }),
          h("td", {}, a.level ? h("span", { class: "tag " + (a.level === "crit" ? "bad" : a.level === "warn" ? "warn" : "ok"), text: { ok: "ok", warn: "atenção", crit: "crítico" }[a.level] }) : null),
        ),
      );
    }
    box.append(table);
  }
  box.append(h("p", { class: "meta", style: "margin-top:8px", text: `Contadores de confiabilidade do Windows: indisponível (${d.reliability.unavailable ?? "sem informação"}).` }));
  return box;
}

export function renderStorage(store, { windowMs, endMs }) {
  const busy = store.value("disk.busy");
  const vols = $("#volumes");
  vols.textContent = "";
  if (!state.volumes.length) vols.append(h("div", { class: "card meta", text: "Lendo os volumes…" }));
  for (const v of state.volumes) vols.append(volumeCard(v, busy));

  chart("#diskchart", [
    { id: "disk.read", color: "disk", name: "Leitura", unitLabel: " MB/s", scale: 1e-6 },
    { id: "disk.write", color: "temp", name: "Escrita", unitLabel: " MB/s", scale: 1e-6, fill: false },
  ], { store, windowMs, endMs, max: "auto", unit: "" });

  const tabs = $("#disktabs");
  tabs.textContent = "";
  if (state.selected >= state.disks.length) state.selected = 0;
  state.disks.forEach((d, i) => tabs.append(h("button", { class: i === state.selected ? "on" : "", "data-d": i, text: `Disco ${d.index}: ${d.name}` })));
  const panel = $("#diskdetail");
  panel.textContent = "";
  const d = state.disks[state.selected];
  if (!d) {
    panel.append(h("p", { class: "empty", text: state.volumes.length ? "O Windows não listou discos físicos." : "Lendo os discos…" }));
    return;
  }
  const temp = d.temperature_c.value != null ? withUnit("celsius", d.temperature_c.value) : null;
  panel.append(
    h(
      "div",
      { class: "grid g2" },
      kv([
        ["Modelo", field({ value: d.name })],
        ["Tipo", `${field(d.media)} · ${field(d.bus)}`],
        ["Capacidade", gb(d.size_bytes)],
        ["Firmware", field(d.firmware), true],
        ["Número de série", field(d.serial), true],
        ["Saúde geral (Windows)", field(d.health)],
        ["Temperatura", temp ?? field(d.temperature_c), true],
      ]),
      smartPanel(d),
    ),
  );
}

$("#disktabs").addEventListener("click", (e) => {
  const b = e.target.closest("button[data-d]");
  if (!b) return;
  state.selected = Number(b.dataset.d);
  document.dispatchEvent(new CustomEvent("argus:rerender"));
});
