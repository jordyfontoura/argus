// Rendering of static facts that may be unreadable: a value, or "indisponível" plus the reason.
import { esc } from "./format.js";

/** HTML for a `Field<T>`. `show` formats the known value (default: plain text). */
export function field(f, show = (v) => esc(v)) {
  if (f?.value != null) return show(f.value);
  return `<span class="na">indisponível</span><span class="na-reason">${esc(f?.unavailable ?? "sem informação")}</span>`;
}

/** `<dt>/<dd>` pair. `value` is already HTML (use `field()` or escape it yourself). */
export const row = (label, value, mono = false) =>
  `<dt>${esc(label)}</dt><dd${mono ? ' class="mono"' : ""}>${value}</dd>`;

export const kb = (bytes) => {
  const k = bytes / 1024;
  return k >= 1024 ? `${+(k / 1024).toFixed(1)} MB` : `${+k.toFixed(0)} KB`;
};
