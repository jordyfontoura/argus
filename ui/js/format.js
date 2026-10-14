// Number and unit formatting. The backend always sends canonical units; conversion lives here.
const nf = (d) => ({ minimumFractionDigits: d, maximumFractionDigits: d });
export const fmt = (v, d = 1) => v.toLocaleString("pt-BR", nf(d));

// Display preferences. The backend is canonical (°C, bytes); only this layer converts.
const prefs = { temperature: "C", memory: "GB" };
export function setDisplayPrefs({ temperature_unit, memory_unit }) {
  prefs.temperature = temperature_unit === "F" ? "F" : "C";
  prefs.memory = memory_unit === "GiB" ? "GiB" : "GB";
}
export const tempUnit = () => (prefs.temperature === "F" ? "°F" : "°C");
export const toTemp = (c) => (prefs.temperature === "F" ? (c * 9) / 5 + 32 : c);
/** Bytes as a number in the chosen memory unit (decimal GB or binary GiB). */
export const toMemUnit = (bytes) => bytes / (prefs.memory === "GiB" ? 1024 ** 3 : 1e9);
export const memUnit = () => prefs.memory;
/** "16,0 GB" / "14,9 GiB". */
export const bytesText = (bytes, d = 1) => `${fmt(toMemUnit(bytes), d)} ${prefs.memory}`;

/** `{ text, unit }` for a value in the canonical unit of a sensor. */
export function present(unit, v, canonical = false) {
  if (canonical && unit === "celsius") return { text: fmt(v, 1), unit: "°C" };
  if (canonical && unit === "bytes") return { text: fmt(v / 1e9, 1), unit: "GB" };
  switch (unit) {
    case "percent": return { text: fmt(v, 1), unit: "%" };
    case "celsius": return { text: fmt(toTemp(v), 1), unit: tempUnit() };
    case "megahertz": return v >= 1000 ? { text: fmt(v / 1000, 2), unit: "GHz" } : { text: fmt(v, 0), unit: "MHz" };
    case "watt": return { text: fmt(v, 1), unit: "W" };
    case "volt": return { text: fmt(v, 3), unit: "V" };
    case "rpm": return { text: fmt(v, 0), unit: "RPM" };
    case "bytes": return { text: fmt(toMemUnit(v), 1), unit: prefs.memory };
    case "bytes_per_second": return { text: fmt(v / 1e6, 1), unit: "MB/s" };
    case "mbps": return { text: fmt(v, 1), unit: "Mbps" };
    case "milliseconds": return { text: fmt(v, 0), unit: "ms" };
    default: return { text: fmt(v, 0), unit: "" };
  }
}

export const withUnit = (unit, v) => {
  const p = present(unit, v);
  return p.unit ? `${p.text} ${p.unit}` : p.text;
};

export const esc = (s) =>
  String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
