import { chart } from "./charts.js";
import { field, row } from "./fields.js";
import { esc, withUnit } from "./format.js";

const $ = (s) => document.querySelector(s);

const live = (store, id, unit) => {
  const v = store.value(id);
  return v != null
    ? withUnit(unit, v)
    : `<span class="na">indisponível</span><span class="na-reason">${esc(store.reason(id))}</span>`;
};

const LIMITERS = [
  ["limit.thermal", "Temperatura", "bad"],
  ["limit.power", "Potência", "warn"],
  ["limit.hw_slowdown", "Redução por hardware", "bad"],
  ["limit.idle", "Placa ociosa", "info"],
];

function limiters(store, i) {
  const known = LIMITERS.filter(([key]) => store.value(`gpu.${i}.${key}`) != null);
  if (!known.length) {
    return { chips: '<span class="lim">sem informação</span>', text: `A placa não informa limitadores (${esc(store.reason(`gpu.${i}.limit.power`))}).` };
  }
  const active = known.filter(([key]) => store.value(`gpu.${i}.${key}`) > 0);
  const chips = known
    .map(([key, name, kind]) => (store.value(`gpu.${i}.${key}`) > 0 ? `<span class="lim ${kind}">${name}</span>` : `<span class="lim">${name}: não</span>`))
    .join("");
  const limiting = active.filter(([key]) => key !== "limit.idle");
  const text = limiting.length
    ? `A GPU está reduzindo o desempenho por: ${limiting.map((l) => l[1].toLowerCase()).join(", ")}.`
    : active.length
      ? "A GPU está ociosa, com o clock reduzido de propósito."
      : "Nenhum limitador ativo agora.";
  return { chips, text };
}

export function renderGpu(store, { windowMs, endMs }, details) {
  const list = details?.length ? details : [null];
  $("#gpus").innerHTML = list
    .map(
      (d, i) => `<div class="grid g3"${i ? ' style="margin-top:14px"' : ""}>
      <div class="card"><h3><span class="sw" style="background:var(--gpu)"></span>Uso da GPU e VRAM</h3><svg class="chart" id="gpuchart${i}" role="img" aria-label="Uso da GPU e da VRAM"></svg></div>
      <div class="card"><h3>Placa de vídeo</h3><dl class="kv">
        ${row("Modelo", d ? field(d.name) : field(null))}
        ${d ? row("Arquitetura", field(d.architecture)) : ""}
        ${d ? row("Shaders", field(d.shader_units), true) : ""}
        ${d ? row("VRAM", `${field(d.vram_bytes, (v) => withUnit("bytes", v))}${d.memory_bus_bits.value ? ` · ${d.memory_bus_bits.value} bits` : ""}`) : ""}
        ${d ? row("BIOS", field(d.bios), true) : ""}
        ${d ? row("Driver", field(d.driver), true) : ""}
        ${d ? row("Link PCIe", field(d.pcie_link), true) : ""}
        ${row("Clock núcleo", live(store, `gpu.${i}.clock_core`, "megahertz"), true)}
        ${row("Clock memória", live(store, `gpu.${i}.clock_mem`, "megahertz"), true)}
        ${row("Temperatura", live(store, `gpu.${i}.temp`, "celsius"), true)}
        ${row("Hot spot", live(store, `gpu.${i}.hotspot`, "celsius"), true)}
        ${row("Ventoinha", live(store, `gpu.${i}.fan`, "percent"), true)}
        ${row("Potência", `${live(store, `gpu.${i}.power`, "watt")}${store.value(`gpu.${i}.power_limit`) != null ? ` de ${withUnit("watt", store.value(`gpu.${i}.power_limit`))}` : ""}`, true)}
      </dl></div>
    </div>
    <div class="card" style="margin-top:14px"><h3>Limitador de desempenho</h3><div class="lims">${limiters(store, i).chips}</div><p class="meta">${limiters(store, i).text}</p></div>`,
    )
    .join("");

  list.forEach((_, i) => {
    const total = store.value(`gpu.${i}.vram_total`);
    const used = store.value(`gpu.${i}.vram_used`);
    // VRAM is charted as a percentage of the board's memory, so both lines share the 0-100 scale.
    chart(`#gpuchart${i}`, [
      { id: `gpu.${i}.load`, color: "gpu", name: "GPU", unitLabel: "%" },
      ...(total && used != null ? [{ id: `gpu.${i}.vram_pct`, color: "net", name: "VRAM", unitLabel: "%", fill: false }] : []),
    ], { store, windowMs, endMs, max: 100, unit: "%" });
  });
}
