import { chart } from "./charts.js";
import { field, row } from "./fields.js";
import { bytesText, esc, fmt, withUnit } from "./format.js";

const $ = (s) => document.querySelector(s);
let selected = 0;

$("#spdtabs").addEventListener("click", (e) => {
  const b = e.target.closest("button[data-m]");
  if (!b || b.disabled) return;
  selected = Number(b.dataset.m);
  document.dispatchEvent(new CustomEvent("argus:rerender"));
});

const gb = (bytes) => bytesText(bytes, bytes % 1024 ** 3 ? 1 : 0);
const label = (m) => `${m.slot}${m.bank ? " · " + m.bank : ""}`;

/** "DDR4-2400 (1200 MHz)" from a transfer rate in MT/s. */
const speed = (kind, mts) => `${kind ? kind + "-" : ""}${mts} (${mts / 2} MHz)`;

function configuration(d) {
  const mods = d.modules;
  const total = mods.reduce((sum, m) => sum + m.capacity_bytes, 0);
  const kinds = [...new Set(mods.map((m) => m.kind.value).filter(Boolean))];
  const rates = [...new Set(mods.map((m) => m.configured_mts.value).filter(Boolean))];
  const volts = [...new Set(mods.map((m) => m.voltage_mv.value).filter(Boolean))];
  const kind = kinds.length === 1 ? kinds[0] : null;
  return [
    row("Tipo", kind ? esc(kind) : field({ value: null, unavailable: mods.length ? "os módulos informam tipos diferentes" : "o firmware não informou os módulos" })),
    row("Tamanho", mods.length ? `${gb(total)} (${mods.map((m) => gb(m.capacity_bytes)).join(" + ")})` : field({ value: null, unavailable: "o firmware não listou módulos" })),
    row("Canais", field(d.channels)),
    row("Frequência", rates.length === 1 ? speed(kind, rates[0]) : rates.length ? esc(rates.map((r) => speed(kind, r)).join(" / ")) : field({ value: null, unavailable: "o firmware não informou a velocidade" }), true),
    row("Timings", field(d.timings), true),
    row("Command rate", field(d.command_rate), true),
    row("Tensão", volts.length ? esc(volts.map((v) => fmt(v / 1000, 2) + " V").join(" / ")) : field({ value: null, unavailable: "o firmware não informou a tensão" }), true),
    row("Perfil (XMP/EXPO)", field(d.profile)),
  ].join("");
}

function moduleDetail(m) {
  const rated = m.rated_mts.value;
  const cfg = m.configured_mts.value;
  return `<dl class="kv">${[
    row("Slot", esc(label(m))),
    row("Fabricante", field(m.manufacturer)),
    row("Modelo", field(m.part_number), true),
    row("Número de série", field(m.serial), true),
    row("Capacidade", gb(m.capacity_bytes)),
    row("Tipo / formato", `${field(m.kind)} / ${field(m.form_factor)}`),
    row("Velocidade nominal", field(m.rated_mts, (v) => speed(m.kind.value, v)), true),
    row("Velocidade em uso", field(m.configured_mts, (v) => speed(m.kind.value, v)), true),
    rated && cfg && cfg < rated ? row("Observação", `<span class="tag warn">em uso abaixo do nominal</span>`) : "",
    row("Tensão", field(m.voltage_mv, (v) => fmt(v / 1000, 2) + " V"), true),
  ].join("")}</dl>`;
}

export function renderMemory(store, { windowMs, endMs }, details) {
  chart("#memchart", [{ id: "mem.used_pct", color: "mem", name: "Uso", unitLabel: "%" }], { store, windowMs, endMs, max: 100, unit: "%" });

  const d = details;
  const used = store.value("mem.used");
  const total = store.value("mem.total");
  const live = used != null && total
    ? `${row("Em uso agora", `${withUnit("bytes", used)} de ${withUnit("bytes", total)} (${fmt((used / total) * 100, 0)}%)`, true)}${row("Arquivo de paginação", `${store.value("mem.swap_used") != null ? withUnit("bytes", store.value("mem.swap_used")) : "—"} de ${store.value("mem.swap_total") != null ? withUnit("bytes", store.value("mem.swap_total")) : "—"}`, true)}`
    : row("Em uso agora", `<span class="na">indisponível</span><span class="na-reason">${esc(store.reason("mem.used"))}</span>`);
  $("#memkv").innerHTML = (d ? configuration(d) : "") + live;

  const mods = d?.modules ?? [];
  const empty = Math.max(0, (d?.total_slots.value ?? mods.length) - mods.length);
  if (selected >= mods.length) selected = 0;
  $("#spdtabs").innerHTML =
    mods.map((m, i) => `<button data-m="${i}" class="${i === selected ? "on" : ""}">${esc(label(m))}</button>`).join("") +
    Array.from({ length: empty }, () => '<button disabled style="opacity:.5">slot vazio</button>').join("");
  $("#spd").innerHTML = mods.length
    ? moduleDetail(mods[selected])
    : `<p class="empty">${esc(d ? "O firmware não listou módulos de memória." : "Sem informação de módulos.")}</p>`;
}
