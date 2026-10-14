import { chart } from "./charts.js";
import { field, kb, row } from "./fields.js";
import { esc, fmt, withUnit } from "./format.js";

const $ = (s) => document.querySelector(s);

/** Live value or its reason, as HTML. */
function live(store, id, unit) {
  const v = store.value(id);
  return v != null
    ? withUnit(unit, v)
    : `<span class="na">indisponível</span><span class="na-reason">${esc(store.reason(id))}</span>`;
}

function limiters(store) {
  const flags = store.value("cpu.limit_flags");
  const pct = store.value("cpu.freq_pct");
  const chips = [];
  if (flags == null && pct == null) {
    $("#cpulim").innerHTML = '<span class="lim">sem informação</span>';
    $("#cpulimtxt").textContent = `O Windows não informa limites de desempenho nesta máquina (${store.reason("cpu.limit_flags")}).`;
    return;
  }
  if (flags != null) {
    chips.push(flags > 0
      ? `<span class="lim bad">Limite de desempenho ativo (0x${(flags >>> 0).toString(16).toUpperCase()})</span>`
      : '<span class="lim">Sem limite de desempenho</span>');
  }
  if (pct != null) {
    chips.push(pct < 95
      ? `<span class="lim warn">Clock em ${fmt(pct, 0)}% do máximo</span>`
      : `<span class="lim">Clock em ${fmt(pct, 0)}% do máximo</span>`);
  }
  $("#cpulim").innerHTML = chips.join("");
  $("#cpulimtxt").textContent = flags > 0 || (pct != null && pct < 95)
    ? "O Windows está limitando o clock, mas não diz o motivo (temperatura, potência ou política de energia)."
    : "Nenhum limitador ativo agora.";
}

export function renderCpu(store, { windowMs, endMs }, details) {
  chart("#cpuchart", [{ id: "cpu.load", color: "cpu", name: "Uso", unitLabel: "%" }], { store, windowMs, endMs, max: 100, unit: "%" });

  const d = details;
  const clock = store.value("cpu.clock");
  $("#cpukv").innerHTML = d
    ? [
        row("Modelo", field(d.model)),
        row("Fabricante", field(d.vendor)),
        row("Codinome", field(d.codename)),
        row("Soquete", field(d.socket)),
        row("Processo", field(d.process_node)),
        row("Núcleos / Threads", `${field(d.physical_cores)} / ${field(d.threads)}`),
        row("Família / Modelo / Stepping", field(d.signature), true),
        row("Clock base", field(d.base_clock_mhz, (v) => withUnit("megahertz", v)), true),
        row("Clock atual", clock != null ? withUnit("megahertz", clock) : live(store, "cpu.clock", "megahertz"), true),
        row("Temperatura", live(store, "cpu.temp", "celsius"), true),
        row("Multiplicador", field(d.multiplier), true),
        row("Tensão (Vcore)", live(store, "cpu.vcore", "volt"), true),
        row("TDP", field(d.tdp_watts, (v) => withUnit("watt", v))),
        row("Potência", live(store, "cpu.power", "watt"), true),
        row("Tjmax", field(d.tjmax_celsius, (v) => withUnit("celsius", v))),
      ].join("")
    : "";

  const threads = [...store.catalog.keys()].filter((id) => /^cpu\.thread\.\d+\.load$/.test(id));
  threads.sort((a, b) => Number(a.split(".")[2]) - Number(b.split(".")[2]));
  $("#cores").innerHTML = threads
    .map((id, i) => {
      const v = store.value(id);
      return v == null
        ? `<div class="core"><span>T${i}</span><b class="mono">—</b></div>`
        : `<div class="core" style="--h:${Math.min(100, v)}%"><span>T${i}</span><b class="mono">${Math.round(v)}%</b></div>`;
    })
    .join("");

  $("#cpucache").innerHTML = d?.caches.length
    ? `<tr><th>Nível</th><th>Tamanho</th><th>Associatividade</th></tr>` +
      d.caches
        .map((c) => `<tr><td>${esc(c.name)}</td><td>${c.instances} × ${kb(c.size_bytes_each)}</td><td>${c.ways ? c.ways + " vias" : "—"}</td></tr>`)
        .join("")
    : '<tr><td class="empty">O Windows não informou a topologia de cache.</td></tr>';

  limiters(store);
  $("#cpuisa").innerHTML = d?.instruction_sets.length
    ? d.instruction_sets.map((s) => `<span>${esc(s)}</span>`).join("")
    : '<span>sem informação</span>';
}
