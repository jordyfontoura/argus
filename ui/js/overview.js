import { chart, spark } from "./charts.js";
import { bytesText, esc, fmt, memUnit, present, tempUnit, toMemUnit, toTemp, withUnit } from "./format.js";
import { favorites, setFavorite } from "./favorites.js";
import { pinnedRows } from "./sensors-model.js";

const $ = (s) => document.querySelector(s);
const LEVEL_TEXT = { ok: "Normal", warn: "Elevado", crit: "Crítico" };
let tab = "load";
// What the highlights table shows, in order; sensors that cannot be read are simply left out.
const FEATURED = ["cpu.load", "cpu.clock", "cpu.temp", "gpu.0.load", "gpu.0.temp", "gpu.0.power", "mem.used_pct", "disk.busy", "net.total.rx", "net.total.tx"];

$("#maintabs").addEventListener("click", (e) => {
  const b = e.target.closest("button");
  if (!b) return;
  tab = b.dataset.t;
  document.querySelectorAll("#maintabs button").forEach((x) => {
    x.classList.toggle("on", x === b);
    x.setAttribute("aria-pressed", String(x === b));
  });
  document.dispatchEvent(new CustomEvent("argus:rerender"));
});

/** A value, or the reason it is missing. Never a zero standing in for a failure. */
function valueOrReason(store, id, unitHint) {
  const v = store.value(id);
  if (v != null) return { v, ...present(unitHint ?? store.info(id)?.meta.unit, v) };
  return { v: null, reason: store.reason(id) };
}

function tile(store, { title, color, id, sub, windowMs, endMs }) {
  const cur = valueOrReason(store, id);
  const level = store.level(id);
  const body = cur.v != null
    ? `<div class="big mono">${cur.text}<small>${cur.unit}</small></div><div class="meta">${sub}${level ? ` · ${LEVEL_TEXT[level]}` : ""}</div>${spark(store, id, color, { windowMs, endMs, max: store.info(id)?.meta.unit === "percent" ? 100 : undefined })}`
    : `<div class="big mono na">—<small></small></div><div class="meta"><span class="tag warn">indisponível</span></div><span class="na-reason">${esc(cur.reason)}</span>`;
  return `<div class="card"><h3><span class="sw" style="background:var(--${color})"></span>${title}</h3>${body}</div>`;
}

const part = (store, id, fn) => {
  const v = store.value(id);
  return v == null ? null : fn(v);
};

export function renderOverview(liveStore, { windowMs, endMs }) {
  // While the screen is paused `endMs` is frozen: every value, sparkline and chart is read as of
  // that moment, while the store keeps receiving the data collected in the meantime.
  const store = liveStore.view(endMs);

  const cpuSub = [
    part(store, "cpu.clock", (v) => withUnit("megahertz", v)),
    part(store, "cpu.temp", (v) => withUnit("celsius", v)),
  ].filter(Boolean).join(" · ") || "sem clock nem temperatura";
  const gpuSub = [
    part(store, "gpu.0.temp", (v) => withUnit("celsius", v)),
    part(store, "gpu.0.power", (v) => withUnit("watt", v)),
  ].filter(Boolean).join(" · ") || "sem temperatura nem potência";

  const used = store.value("mem.used");
  const total = store.value("mem.total");
  const memTile = used != null && total
    ? `<div class="card"><h3><span class="sw" style="background:var(--mem)"></span>Memória</h3><div class="big mono">${fmt(toMemUnit(used), 1)}<small>${memUnit()}</small></div><div class="meta">de ${bytesText(total)} · ${fmt((used / total) * 100, 0)}%</div>${spark(store, "mem.used_pct", "mem", { windowMs, endMs })}</div>`
    : tile(store, { title: "Memória", color: "mem", id: "mem.used", sub: "", windowMs, endMs });

  $("#tiles").innerHTML = [
    tile(store, { title: "CPU", color: "cpu", id: "cpu.load", sub: cpuSub, windowMs, endMs }),
    tile(store, { title: "GPU", color: "gpu", id: "gpu.0.load", sub: gpuSub, windowMs, endMs }),
    memTile,
    tile(store, { title: "Temp. CPU", color: "temp", id: "cpu.temp", sub: "", windowMs, endMs }),
  ].join("");

  const opts = { store, windowMs, endMs };
  const tempTab = document.querySelector("[data-t=temp]");
  if (tempTab) tempTab.textContent = `Temperatura ${tempUnit()}`;
  if (tab === "temp") {
    chart("#mainchart", [
      { id: "cpu.temp", color: "temp", name: "CPU", unitLabel: ` ${tempUnit()}`, convert: toTemp },
      { id: "gpu.0.temp", color: "gpu", name: "GPU", unitLabel: ` ${tempUnit()}`, fill: false, convert: toTemp },
    ], { ...opts, max: toTemp(100), unit: ` ${tempUnit()}` });
    const missing = ["cpu.temp", "gpu.0.temp"].filter((id) => store.value(id) == null);
    $("#mainnote").textContent = missing.length
      ? "Sem leitura: " + missing.map((id) => `${store.info(id)?.meta.group ?? id} (${store.reason(id)})`).join("; ")
      : "";
  } else {
    chart("#mainchart", [
      { id: "cpu.load", color: "cpu", name: "CPU", unitLabel: "%" },
      { id: "gpu.0.load", color: "gpu", name: "GPU", unitLabel: "%" },
    ], { ...opts, max: 100, unit: "%" });
    $("#mainnote").textContent = "";
  }

  const rows = FEATURED.map((id) => store.info(id))
    .filter((info) => info && store.value(info.meta.id) != null)
    .map((info) => `<tr><td>${esc(info.meta.group)} · ${esc(info.meta.name)}</td><td class="num mono">${withUnit(info.meta.unit, store.value(info.meta.id))}</td></tr>`)
    .join("");
  renderPins(store, windowMs, endMs);
  $("#topsens").innerHTML = rows || '<tr><td class="empty">Nenhum sensor legível ainda.</td></tr>';
}

/** "Meu painel": the pinned sensors as small cards with a sparkline. */
function renderPins(store, windowMs, endMs) {
  const box = $("#pins");
  const rows = pinnedRows(store.catalog, favorites());
  box.textContent = "";
  if (!rows.length) {
    const none = document.createElement("div");
    none.className = "meta";
    none.textContent = "Nenhum sensor fixado. Use a ★ na tela Sensores.";
    box.append(none);
    return;
  }
  for (const info of rows) {
    const { id, name, group, unit } = info.meta;
    const v = store.value(id);
    const level = store.level(id);
    const pin = document.createElement("div");
    pin.className = "pin";
    const remove = document.createElement("button");
    remove.className = "star x on";
    remove.dataset.k = id;
    remove.setAttribute("aria-label", `Remover ${name} do painel`);
    remove.textContent = "✕";
    const label = document.createElement("div");
    label.className = "meta";
    label.textContent = `${group} › ${name}`;
    const value = document.createElement("div");
    value.className = "big mono";
    value.style.fontSize = "22px";
    if (v != null) {
      value.textContent = withUnit(unit, v);
      if (level === "warn" || level === "crit") value.style.color = level === "crit" ? "var(--bad)" : "var(--warn)";
    } else {
      value.textContent = "indisponível";
      value.className += " na";
    }
    pin.append(remove, label, value);
    if (v != null) pin.insertAdjacentHTML("beforeend", spark(store, id, level === "crit" ? "bad" : level === "warn" ? "warn" : "cpu", { windowMs: Math.min(windowMs, 60000), endMs, max: unit === "percent" ? 100 : undefined }));
    else {
      const why = document.createElement("span");
      why.className = "na-reason";
      why.textContent = store.reason(id) ?? "";
      pin.append(why);
    }
    box.append(pin);
  }
}

$("#pins").addEventListener("click", (e) => {
  const b = e.target.closest(".x");
  if (b) setFavorite(b.dataset.k, false);
});
