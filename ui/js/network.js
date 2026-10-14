import { chart } from "./charts.js";
import { field } from "./fields.js";
import { esc, fmt } from "./format.js";

const $ = (s) => document.querySelector(s);

const rate = (store, key, dir) => {
  const v = store.value(`net.${key}.${dir}`);
  return v == null ? '<span class="na">—</span>' : `${fmt(v, 1)} Mbps`;
};

const link = (a) => {
  const v = a.link_mbps.value;
  if (v == null) return field(a.link_mbps);
  return v >= 1000 ? `${fmt(v / 1000, v % 1000 ? 1 : 0)} Gbps` : `${fmt(v, 0)} Mbps`;
};

export function renderNetwork(store, { windowMs, endMs }, adapters) {
  chart("#netchart", [
    { id: "net.total.rx", color: "net", name: "↓", unitLabel: " Mbps" },
    { id: "net.total.tx", color: "mem", name: "↑", unitLabel: " Mbps", fill: false },
  ], { store, windowMs, endMs, max: "auto", unit: "" });

  const list = adapters ?? [];
  $("#adapters").innerHTML = list.length
    ? `<tr><th>Nome</th><th>IP</th><th>Link</th><th class="num">↓</th><th class="num">↑</th></tr>` +
      list
        .map((a) => {
          const up = a.connected.value;
          const state = up === false ? '<span class="tag">desconectado</span>' : "";
          const ip = a.addresses.length ? a.addresses.map(esc).join(", ") : up === false ? "desconectado" : '<span class="na">indisponível</span>';
          return `<tr${up === false ? ' style="opacity:.65"' : ""}><td>${esc(a.name)} ${state}<span class="na-reason">${esc(a.description.value ?? "")}</span></td><td class="mono">${ip}</td><td>${up === false ? "—" : link(a)}</td><td class="num mono">${rate(store, a.sensor_key, "rx")}</td><td class="num mono">${rate(store, a.sensor_key, "tx")}</td></tr>`;
        })
        .join("")
    : '<tr><td class="empty">O Windows não listou adaptadores de rede.</td></tr>';
}
