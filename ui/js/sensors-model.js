// Pure logic of the Sensors screen: grouping, search, favorites filter and number formatting.
// No DOM here, so it is unit-tested.
import { present } from "./format.js";

export const GROUP_ORDER = ["CPU", "CPU › Threads", "GPU", "Memória", "Armazenamento", "Rede"];

/** Per-thread sensors get their own group so the CPU group stays short. */
export function groupOf(meta) {
  return /^cpu\.thread\./.test(meta.id) ? "CPU › Threads" : meta.group;
}

const rank = (g) => {
  const i = GROUP_ORDER.indexOf(g);
  return i < 0 ? GROUP_ORDER.length : i;
};

/** Bar fill (0-100) for a value. Percentages are exact; others use a sensible full scale. */
export function barPercent(unit, value, range) {
  const full = {
    percent: 100,
    celsius: 110,
    megahertz: Math.max(range?.max ?? 0, 1000) * 1.1,
    watt: Math.max(range?.max ?? 0, 50) * 1.2,
    volt: 15,
    rpm: Math.max(range?.max ?? 0, 2000) * 1.2,
    bytes: Math.max(range?.max ?? 0, value, 1),
    bytes_per_second: Math.max(range?.max ?? 0, value, 1),
    mbps: Math.max(range?.max ?? 0, value, 1),
    milliseconds: Math.max(range?.max ?? 0, value, 1),
  }[unit];
  if (!full) return 0;
  return Math.max(0, Math.min(100, (value / full) * 100));
}

const text = (unit, v) => {
  const p = present(unit, v);
  return p.unit ? `${p.text} ${p.unit}` : p.text;
};

/**
 * @param {object} o
 * @param {Map<string, object>} o.catalog   id -> SensorInfo
 * @param {{value:Function, reason:Function, level:Function}} o.view   readings as of now
 * @param {Set<string>} o.favs
 * @param {string} o.query
 * @param {boolean} o.onlyFavs
 * @param {Set<string>} o.collapsed         group names
 * @param {Map<string, {min:number,max:number}>} o.ranges
 * @returns {{group:string, count:number, collapsed:boolean, rows:object[]}[]}
 */
export function buildGroups({ catalog, view, favs, query, onlyFavs, collapsed, ranges }) {
  const q = query.trim().toLowerCase();
  const byGroup = new Map();
  for (const info of catalog.values()) {
    const { id, name, unit } = info.meta;
    const group = groupOf(info.meta);
    if (onlyFavs && !favs.has(id)) continue;
    if (q && !`${group} ${name}`.toLowerCase().includes(q)) continue;
    const value = view.value(id);
    const range = ranges.get(id);
    const ok = value != null;
    const row = {
      id,
      name,
      unit,
      fav: favs.has(id),
      ok,
      reason: ok ? null : view.reason(id),
      level: ok ? view.level(id) : null,
      cur: ok ? text(unit, value) : null,
      min: ok && range ? text(unit, range.min) : null,
      max: ok && range ? text(unit, range.max) : null,
      pct: ok ? barPercent(unit, value, range) : 0,
    };
    if (!byGroup.has(group)) byGroup.set(group, []);
    byGroup.get(group).push(row);
  }
  return [...byGroup.entries()]
    .sort((a, b) => rank(a[0]) - rank(b[0]) || a[0].localeCompare(b[0]))
    .map(([group, rows]) => ({
      group,
      count: rows.length,
      // Searching always shows matches, even inside a collapsed group.
      collapsed: !q && collapsed.has(group),
      rows,
    }));
}

/** Pinned sensors in the order they were pinned; ids that no longer exist are skipped. */
export function pinnedRows(catalog, favList) {
  return favList.map((id) => catalog.get(id)).filter(Boolean);
}
