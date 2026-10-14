// Header controls: pause (freezes only the screen), time window, theme, keyboard shortcuts.
import { prefs, updatePrefs } from "./prefs.js";

const $ = (s) => document.querySelector(s);

export const WINDOWS = [
  { ms: 60_000, label: "60 s" },
  { ms: 5 * 60_000, label: "5 min" },
  { ms: 15 * 60_000, label: "15 min" },
];

/**
 * @param {{ paused:boolean, frozenEnd:number, windowMs:number }} state shared view state
 * @param {{ latestTs:()=>number, onChange:()=>void, onView:(index:number)=>void }} hooks
 */
export function initHeader(state, { latestTs, onChange, onView, onStep, onError }) {
  const setPaused = (paused) => {
    state.paused = paused;
    // Freeze the screen at the newest data it has; collection keeps filling the store.
    state.frozenEnd = paused ? latestTs() : 0;
    document.body.classList.toggle("paused", paused);
    $("#pause").setAttribute("aria-pressed", String(paused));
    $("#pt").textContent = paused ? "Tela pausada · a coleta continua" : "Ao vivo";
    onChange();
  };

  const setWindow = (index) => {
    state.windowMs = WINDOWS[index].ms;
    $("#rate").textContent = "Janela: " + WINDOWS[index].label;
    onChange();
  };

  $("#pause").addEventListener("click", () => setPaused(!state.paused));
  $("#rate").addEventListener("click", () => {
    const i = WINDOWS.findIndex((w) => w.ms === state.windowMs);
    setWindow((i + 1) % WINDOWS.length);
  });
  $("#theme").addEventListener("click", () => {
    updatePrefs({ light_theme: !prefs().light_theme }).catch((e) => onError?.("Não foi possível salvar o tema: " + e.message));
  });

  document.addEventListener("keydown", (e) => {
    const t = e.target;
    if (/INPUT|SELECT|TEXTAREA|BUTTON/.test(t.tagName) || t.getAttribute?.("role") === "switch") return;
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.key === " ") {
      e.preventDefault();
      setPaused(!state.paused);
    } else if (/^[0-9]$/.test(e.key)) {
      onView(e.key === "0" ? 9 : Number(e.key) - 1); // 1-9 are the first nine screens, 0 the tenth
    } else if (e.key === "[" || e.key === "]") {
      onStep(e.key === "]" ? 1 : -1); // screens beyond the tenth are reached with [ and ]
    }
  });

  setWindow(0);
  setPaused(false);
}
