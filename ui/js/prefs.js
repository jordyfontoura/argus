// App preferences: kept by the backend (config file), applied here. Units are display-only; the
// backend and the history stay in canonical units.
import { api } from "./bridge.js";
import { setDisplayPrefs } from "./format.js";

export const INTERVALS = [
  [500, "500 ms"],
  [1000, "1 s"],
  [2000, "2 s"],
  [5000, "5 s"],
];

let current = { sample_interval_ms: 1000, temperature_unit: "C", memory_unit: "GB", light_theme: false, history_retention_days: 30 };

export const prefs = () => current;

function apply() {
  setDisplayPrefs(current);
  document.documentElement.classList.toggle("light", current.light_theme);
  const theme = document.querySelector("#theme");
  if (theme) {
    theme.setAttribute("aria-pressed", String(current.light_theme));
    theme.textContent = current.light_theme ? "Tema claro" : "Tema escuro";
  }
  document.dispatchEvent(new CustomEvent("argus:rerender"));
}

/** Reads the saved preferences. A failure keeps the defaults; the caller decides what to tell the user. */
export async function loadPrefs() {
  current = await api.settings();
  apply();
}

/** Saves a change (the backend validates and applies the interval). Returns the saved preferences. */
export async function updatePrefs(patch) {
  current = await api.setSettings({ ...current, ...patch });
  apply();
  return current;
}
