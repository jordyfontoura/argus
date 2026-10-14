// Pure logic of the Processes screen: sorting, filtering, the frozen order while the mouse is
// over the table, and memory formatting.
import { bytesText, fmt } from "./format.js";

export const COLUMNS = [
  ["name", "Nome"],
  ["pid", "PID"],
  ["cpu", "CPU %"],
  ["mem", "Memória"],
];

const KEY = { name: (r) => r.name.toLowerCase(), pid: (r) => r.pid, cpu: (r) => r.cpu_percent, mem: (r) => r.memory_bytes };

export function sortRows(rows, key, dir) {
  const get = KEY[key] ?? KEY.cpu;
  return rows.slice().sort((a, b) => {
    const x = get(a), y = get(b);
    const c = typeof x === "string" ? x.localeCompare(y, "pt-BR") : x - y;
    // Ties fall back to the PID so the order is stable between refreshes.
    return (c || a.pid - b.pid) * dir;
  });
}

export function filterRows(rows, query) {
  const q = query.trim().toLowerCase();
  return q ? rows.filter((r) => r.name.toLowerCase().includes(q) || String(r.pid) === q) : rows;
}

/**
 * Rows in display order. While the mouse is over the table the order is frozen (`frozen` is the
 * list of PIDs shown at that moment), so a row cannot jump away just before it is clicked; the
 * values still update. Processes that appeared meanwhile go to the end.
 */
export function orderRows(rows, { key, dir, query }, frozen) {
  const visible = filterRows(rows, query);
  if (!frozen) return sortRows(visible, key, dir);
  const place = new Map(frozen.map((pid, i) => [pid, i]));
  const kept = visible.filter((r) => place.has(r.pid)).sort((a, b) => place.get(a.pid) - place.get(b.pid));
  const fresh = sortRows(visible.filter((r) => !place.has(r.pid)), key, dir);
  return [...kept, ...fresh];
}

export function formatMemory(bytes) {
  const mb = bytes / 1e6;
  return mb >= 1000 ? bytesText(bytes, 1) : `${Math.round(mb)} MB`;
}

export const MAX_ROWS = 300;
