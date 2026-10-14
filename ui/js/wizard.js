// Protection wizard: pick a preset, choose a sensitivity, review what it would create on THIS
// machine (rules for sensors that cannot be read are listed as skipped, with the reason), apply.
import { api } from "./bridge.js";
import { sentence } from "./alerts-text.js";
import { metrics } from "./alerts.js";

const $ = (s) => document.querySelector(s);
const SEEN_KEY = "argus-wizard-seen";

function h(tag, attrs = {}, ...children) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v == null || (v === false && !k.startsWith("aria-"))) continue;
    if (k === "class") el.className = v;
    else if (k === "text") el.textContent = v;
    else el.setAttribute(k, v === true && !k.startsWith("aria-") ? "" : String(v));
  }
  for (const c of children.flat()) if (c != null) el.append(c);
  return el;
}

const SENS = [
  ["relaxed", "Relaxado", "Avisa mais tarde e com limites mais altos. Menos alarmes, menos margem."],
  ["normal", "Normal", "Equilíbrio recomendado entre segurança e alarmes falsos."],
  ["strict", "Rigoroso", "Avisa mais cedo, com limites mais baixos. Mais alarmes, mais margem de segurança."],
];
const ICONS = { basic: "🛡", gaming: "🎮", always_on: "🖥" };

const wiz = { step: 1, presets: [], preset: "basic", sens: "normal", actions: null, off: new Set(), keepOwn: true, plan: [], store: null, opener: null, error: "" };
let hooks = { store: () => null, onDone: () => {}, onCustom: () => {} };

export function initWizard(options) {
  hooks = options;
  $("#openwiz").addEventListener("click", (e) => open(e.currentTarget));
}

/** Opens the wizard by itself once, on a machine that has no rules and no preset yet. */
export function maybeAutoOpen(rules, presetLabel) {
  let seen = false;
  try { seen = localStorage.getItem(SEEN_KEY) === "1"; } catch { /* storage may be off */ }
  if (!seen && !rules.length && !presetLabel) open(null);
}

async function open(opener) {
  wiz.step = 1;
  wiz.off = new Set();
  wiz.actions = null;
  wiz.error = "";
  wiz.opener = opener ?? document.activeElement;
  try {
    wiz.presets = await api.listPresets();
  } catch (e) {
    wiz.presets = [];
    wiz.error = e.message;
  }
  draw();
  $("#wiz").classList.add("show");
  $("#wiz-box").querySelector("button")?.focus();
}

function close(markSeen = true) {
  $("#wiz").classList.remove("show");
  if (markSeen) {
    try { localStorage.setItem(SEEN_KEY, "1"); } catch { /* storage may be off */ }
  }
  wiz.opener?.focus?.();
}

async function refreshPlan() {
  const preset = wiz.presets.find((p) => p.key === wiz.preset);
  try {
    wiz.plan = await api.previewPreset(wiz.preset, wiz.sens, wiz.actions ?? preset.default_actions);
    wiz.error = "";
  } catch (e) {
    wiz.plan = [];
    wiz.error = e.message;
  }
}

const chosenCount = () => wiz.plan.filter((p) => p.available && !wiz.off.has(p.key)).length;

function metricOf(sensor) {
  const store = hooks.store();
  return metrics(store).find((m) => m.id === sensor) ?? { name: sensor, unit: "" };
}

function draw() {
  const box = $("#wiz-box");
  box.textContent = "";
  if (wiz.step === 1) {
    box.append(
      h("div", { class: "wiz-steps", text: "Passo 1 de 2" }),
      h("h2", { id: "wt", text: "Proteger meu PC" }),
      h("p", { class: "meta", text: "Escolha um perfil e o Argus cria os alertas certos para você. Dá para ajustar tudo depois, regra por regra." }),
      h(
        "div",
        { class: "pcards" },
        wiz.presets.map((p) =>
          h(
            "button",
            { class: "pcard" + (wiz.preset === p.key ? " on" : ""), "data-act": "pick", "data-p": p.key },
            h("span", { class: "ic", text: ICONS[p.key] ?? "🛡" }),
            h("b", { text: p.label }),
            p.recommended ? h("span", { class: "rec", text: "Recomendado" }) : null,
            h("small", { text: p.description }),
            h("small", { text: `${p.rule_count} alertas` }),
          ),
        ),
        h("button", { class: "pcard", "data-act": "custom" }, h("span", { class: "ic", text: "🛠" }), h("b", { text: "Montar do zero" }), h("small", { text: "Para quem já sabe o que quer monitorar. Abre o editor de regras." })),
      ),
      ...(wiz.error ? [h("p", { class: "meta", style: "color:var(--bad)", text: wiz.error })] : []),
      h("div", { class: "mfoot" }, h("button", { class: "btn", "data-act": "skip", text: "Pular por agora" }), h("button", { class: "btn pri", "data-act": "next", text: "Continuar" })),
    );
    return;
  }
  const preset = wiz.presets.find((p) => p.key === wiz.preset);
  const acts = wiz.actions ?? preset.default_actions;
  const sensText = SENS.find((s) => s[0] === wiz.sens)[2];
  box.append(
    h("div", { class: "wiz-steps", text: "Passo 2 de 2" }),
    h("h2", { id: "wt", text: `${ICONS[preset.key] ?? "🛡"} ${preset.label}` }),
    h(
      "div",
      { class: "wrow" },
      h("span", { class: "meta", text: "Sensibilidade" }),
      h("span", { class: "seg" }, SENS.map(([key, label]) => h("button", { class: wiz.sens === key ? "on" : "", "data-act": "sens", "data-s": key, text: label }))),
    ),
    h("p", { class: "meta", style: "margin:6px 0 10px", text: sensText }),
    h("div", { class: "meta", style: "margin-bottom:2px", text: "O que será monitorado nesta máquina" }),
    h(
      "div",
      { id: "wiz-rules" },
      wiz.plan.map((p) => {
        const m = metricOf(p.rule.sensor);
        return h(
          "label",
          { class: "wrule", style: p.available ? null : "opacity:.6;cursor:default" },
          h("input", { type: "checkbox", "data-act": "rule", "data-k": p.key, checked: p.available && !wiz.off.has(p.key), disabled: !p.available }),
          h(
            "span",
            {},
            h("b", { text: p.rule.name }),
            h("small", { text: p.available ? sentence(p.rule, m) : `Não disponível nesta máquina: ${p.unavailable_reason}` }),
            h("small", { text: p.why }),
            p.available ? h("small", { text: `Limite: ${p.limit_source}` }) : null,
          ),
        );
      }),
    ),
    h("div", { class: "meta", style: "margin:12px 0 4px", text: "Como avisar" }),
    h(
      "div",
      { class: "acts" },
      [["toast", "Aviso na tela"], ["sound", "Som"], ["notify", "Notificação do sistema"]].map(([k, label]) =>
        h("label", {}, h("input", { type: "checkbox", "data-act": "acts", "data-k": k, checked: acts[k] }), ` ${label}`),
      ),
    ),
    h(
      "label",
      { class: "wrule", style: "border:0;margin-top:6px" },
      h("input", { type: "checkbox", "data-act": "keep", checked: wiz.keepOwn }),
      h("span", {}, "Manter as regras que eu mesmo criei", h("small", { text: "Regras de perfis anteriores são substituídas por estas." })),
    ),
    ...(wiz.error ? [h("p", { class: "meta", role: "alert", style: "color:var(--bad)", text: wiz.error })] : []),
    h(
      "div",
      { class: "mfoot" },
      h("button", { class: "btn", "data-act": "back", text: "Voltar" }),
      h("button", { class: "btn pri", id: "wiz-apply", "data-act": "apply", disabled: chosenCount() === 0, text: `Ativar ${chosenCount()} alertas` }),
    ),
  );
}

async function redraw(focusSelector) {
  await refreshPlan();
  draw();
  if (focusSelector) $("#wiz-box").querySelector(focusSelector)?.focus();
}

$("#wiz-box").addEventListener("click", async (e) => {
  const b = e.target.closest("[data-act]");
  if (!b || b.tagName === "INPUT") return;
  const act = b.dataset.act;
  if (act === "pick") { wiz.preset = b.dataset.p; wiz.actions = null; wiz.off = new Set(); draw(); $(`[data-p="${wiz.preset}"]`)?.focus(); }
  else if (act === "next") { wiz.step = 2; await redraw(); $("#wiz-box").querySelector('[data-act="sens"].on')?.focus(); }
  else if (act === "back") { wiz.step = 1; draw(); }
  else if (act === "skip") close();
  else if (act === "custom") { close(); hooks.onCustom(); }
  else if (act === "sens") { wiz.sens = b.dataset.s; await redraw(`[data-s="${wiz.sens}"]`); }
  else if (act === "apply" && chosenCount()) {
    const preset = wiz.presets.find((p) => p.key === wiz.preset);
    try {
      const chosen = wiz.plan.filter((p) => p.available && !wiz.off.has(p.key)).map((p) => p.key);
      await api.applyPreset(wiz.preset, wiz.sens, wiz.actions ?? preset.default_actions, chosen, wiz.keepOwn);
      close();
      hooks.onDone(preset.label, chosen.length);
    } catch (err) {
      wiz.error = err.message;
      draw();
    }
  }
});

$("#wiz-box").addEventListener("change", async (e) => {
  const el = e.target;
  const act = el.dataset.act;
  if (act === "rule") {
    el.checked ? wiz.off.delete(el.dataset.k) : wiz.off.add(el.dataset.k);
    const apply = $("#wiz-apply");
    apply.textContent = `Ativar ${chosenCount()} alertas`;
    apply.disabled = chosenCount() === 0;
  } else if (act === "acts") {
    const preset = wiz.presets.find((p) => p.key === wiz.preset);
    wiz.actions = { ...(wiz.actions ?? preset.default_actions), [el.dataset.k]: el.checked };
    await redraw(`[data-act="acts"][data-k="${el.dataset.k}"]`);
  } else if (act === "keep") {
    wiz.keepOwn = el.checked;
  }
});

document.addEventListener("keydown", (e) => {
  const modal = $("#wiz");
  if (!modal.classList.contains("show")) return;
  if (e.key === "Escape") { e.preventDefault(); close(); return; }
  if (e.key !== "Tab") return;
  const focusable = [...modal.querySelectorAll("input,select,button")].filter((el) => !el.disabled && el.offsetParent !== null);
  if (!focusable.length) return;
  const first = focusable[0], last = focusable.at(-1);
  if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
  else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
  else if (!modal.contains(document.activeElement)) { e.preventDefault(); first.focus(); }
});
$("#wiz").addEventListener("click", (e) => { if (e.target.id === "wiz") close(); });
