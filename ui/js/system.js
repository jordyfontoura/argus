// System screen: static hardware and OS facts, "Copiar relatório" and "Exportar CSV".
import { api } from "./bridge.js";
import { field, row } from "./fields.js";
import { bytesText, esc, fmt, memUnit } from "./format.js";
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

const state = { system: null, disks: [] };

const gb = (bytes) => {
  const g = bytes / 1e9;
  return g >= 1000 && memUnit() === "GB" ? `${fmt(g / 1000, 2)} TB` : bytesText(bytes, g < 10 ? 1 : 0);
};

function uptime(secs) {
  const d = Math.floor(secs / 86400), hh = Math.floor((secs % 86400) / 3600), m = Math.floor((secs % 3600) / 60);
  const hm = `${String(hh).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
  return d ? `${d} d ${hm}` : hm;
}

export async function loadSystem() {
  try {
    [state.system, state.disks] = await Promise.all([api.systemDetails(), api.disks()]);
  } catch (e) {
    showToast({ title: "Não foi possível ler o sistema", body: e.message, event: { severity: "warn" } });
  }
  document.dispatchEvent(new CustomEvent("argus:rerender"));
}

function card(title, ...body) {
  return h("div", { class: "card" }, h("h3", { text: title }), ...body);
}

function kv(rows) {
  return h("dl", { class: "kv", html: rows.join("") });
}

export function renderSystem(memory) {
  const box = $("#sysgrid");
  box.textContent = "";
  const s = state.system;
  if (!s) {
    box.append(h("div", { class: "card meta", text: "Lendo o sistema…" }));
    return;
  }
  box.append(
    card(
      "Placa-mãe e BIOS",
      kv([
        row("Fabricante", field(s.board_vendor)),
        row("Modelo", field(s.board_model)),
        row("Chipset", field(s.chipset)),
        row("BIOS", `${field(s.bios_vendor)} ${field(s.bios_version)} (${field(s.bios_date)})`),
      ]),
    ),
    card(
      "Sistema operacional",
      kv([
        row("Sistema", field(s.os_name)),
        row("Versão", `${field(s.os_version)} (build ${field(s.os_build)})`),
        row("Arquitetura", field(s.os_arch)),
        row("Tempo ligado", field(s.uptime_secs, (v) => esc(uptime(v))), true),
        row("Argus", "0.1"),
      ]),
    ),
  );

  const mem = h("table", {}, h("tr", {}, h("th", { text: "Slot" }), h("th", { text: "Módulo" }), h("th", { text: "Velocidade" }), h("th", { text: "Tensão" })));
  for (const m of memory?.modules ?? []) {
    const volts = m.voltage_mv.value != null ? `${fmt(m.voltage_mv.value / 1000, 2)} V` : "indisponível";
    const speed = m.configured_mts.value != null ? `${m.kind.value ?? ""} ${m.configured_mts.value} MT/s`.trim() : "indisponível";
    mem.append(
      h("tr", {}, h("td", { text: `${m.slot} · ${m.bank}` }), h("td", { text: `${gb(m.capacity_bytes)} ${m.manufacturer.value ?? ""} ${m.part_number.value ?? ""}`.trim() }), h("td", { class: "mono", text: speed }), h("td", { class: "mono", text: volts })),
    );
  }
  if (!memory?.modules.length) mem.append(h("tr", {}, h("td", { colspan: 4, class: "meta", text: "O firmware não listou módulos de memória." })));
  box.append(card("Memória (por módulo)", mem));

  const disks = h("table", {}, h("tr", {}, h("th", { text: "Unidade" }), h("th", { text: "Interface" }), h("th", { text: "Firmware" })));
  for (const d of state.disks) {
    disks.append(
      h("tr", {}, h("td", { text: `${d.name} · ${gb(d.size_bytes)}` }), h("td", { text: [d.media.value, d.bus.value].filter(Boolean).join(" · ") || "indisponível" }), h("td", { class: "mono", text: d.firmware.value ?? "indisponível" })),
    );
  }
  if (!state.disks.length) disks.append(h("tr", {}, h("td", { colspan: 3, class: "meta", text: "O Windows não listou discos físicos." })));
  box.append(card("Armazenamento", disks));
}

/** Copies text to the clipboard, with a fallback for webviews that block the async API. */
async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch { /* fall through to the legacy path */ }
  const ta = document.createElement("textarea");
  ta.value = text;
  ta.setAttribute("readonly", "");
  ta.style.position = "fixed";
  ta.style.opacity = "0";
  document.body.append(ta);
  ta.select();
  const ok = document.execCommand("copy");
  ta.remove();
  if (!ok) throw new Error("o sistema não permitiu copiar");
}

$("#copyrep").addEventListener("click", async () => {
  try {
    await copyText(await api.report());
    showToast({ title: "Relatório copiado", body: "Cole onde precisar (Ctrl+V).", event: { severity: "info", kind: "resolved" } });
  } catch (e) {
    showToast({ title: "Não foi possível copiar", body: e.message, event: { severity: "warn" } });
  }
});

$("#exportcsv").addEventListener("click", async () => {
  try {
    const path = await api.exportSensorCsv();
    if (path) showToast({ title: "Sensores exportados", body: path, event: { severity: "info", kind: "resolved" } });
  } catch (e) {
    showToast({ title: "Não foi possível exportar", body: e.message, event: { severity: "warn" } });
  }
});
