// History screen: the alert history from the database, with filters, CSV export, clearing and
// the retention setting. All text from events comes in through textContent.
import { api } from "./bridge.js";
import { SEVERITY } from "./alerts-text.js";
import { showToast } from "./alerts.js";

const $ = (s) => document.querySelector(s);

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

const KIND_GROUPS = {
  all: [],
  fired: ["fired", "repeat"],
  resolved: ["resolved"],
  other: ["ended", "note"],
};
const KIND_LABEL = {
  fired: ["⚑ Disparou", "fired"],
  repeat: ["⟳ Ainda ativo", "repeat"],
  resolved: ["✔ Resolvido", "resolved"],
  ended: ["■ Encerrado", ""],
  note: ["ⓘ Nota", ""],
};
const RETENTION_CHOICES = [7, 30, 90];

const state = { page: null, rules: [], confirming: false, timer: null };

export function query() {
  const sev = $("#hf-sev").value;
  const rule = $("#hf-rule").value;
  return {
    severity: sev === "all" ? null : sev,
    rule_id: rule === "all" ? null : Number(rule),
    kinds: KIND_GROUPS[$("#hf-kind").value] ?? [],
    include_tests: $("#hf-test").checked,
    limit: 200,
  };
}

export function setRules(rules) {
  state.rules = rules;
}

function fillRules(events) {
  const sel = $("#hf-rule");
  const current = sel.value;
  const known = new Map(state.rules.map((r) => [String(r.id), r.name]));
  // Events of deleted rules keep their name in the history, so they stay filterable.
  for (const e of events) if (!known.has(String(e.rule_id))) known.set(String(e.rule_id), `${e.rule_name} (excluída)`);
  const sig = [...known].join("|");
  if (sel.dataset.sig === sig) return;
  sel.dataset.sig = sig;
  sel.textContent = "";
  sel.append(h("option", { value: "all", text: "Todas as regras" }));
  for (const [id, name] of known) sel.append(h("option", { value: id, text: name }));
  sel.value = [...sel.options].some((o) => o.value === current) ? current : "all";
}

function when(ts) {
  const d = new Date(ts);
  const today = new Date().toDateString() === d.toDateString();
  const day = today ? "hoje" : d.toLocaleDateString("pt-BR", { day: "2-digit", month: "2-digit" });
  return `${day} ${d.toLocaleTimeString("pt-BR")}`;
}

function render() {
  const page = state.page;
  if (!page) return;
  fillRules(page.events);
  $("#hsum").textContent = "";
  $("#hsum").append(
    h("b", { text: page.matching }),
    ` de ${page.total} eventos · `,
    h("b", { style: "color:var(--bad)", text: page.critical_fired }),
    ` ${page.critical_fired === 1 ? "disparo crítico" : "disparos críticos"}`,
  );
  const body = $("#hbody");
  body.textContent = "";
  if (!page.events.length) {
    body.append(
      h("tr", {}, h("td", { colspan: 5, class: "meta", style: "padding:18px", text: page.total ? "Nenhum evento com esses filtros." : "Nenhum evento ainda. Use “Simular” numa regra para testar." })),
    );
  }
  for (const e of page.events) {
    const [label, css] = KIND_LABEL[e.kind] ?? [e.kind, ""];
    body.append(
      h(
        "tr",
        {},
        h("td", { class: "mono", style: "white-space:nowrap", text: when(e.ts_ms) }),
        h("td", {}, h("span", { class: "tag " + (e.severity === "crit" ? "bad" : e.severity === "warn" ? "warn" : ""), text: SEVERITY[e.severity] })),
        h("td", {}, e.rule_name, e.is_test ? [" ", h("span", { class: "tag", text: "teste" })] : null),
        h("td", {}, h("span", { class: "ev " + css, text: label })),
        h("td", { class: "meta", text: e.detail }),
      ),
    );
  }
  const keep = $("#hret");
  if (!keep.dataset.ready || Number(keep.value) !== page.retention_days) {
    keep.textContent = "";
    const days = [...new Set([...RETENTION_CHOICES, page.retention_days])].sort((a, b) => a - b);
    for (const d of days) keep.append(h("option", { value: d, text: `${d} dias` }));
    keep.value = String(page.retention_days);
    keep.dataset.ready = "1";
  }
  const clear = $("#clrlog");
  clear.textContent = state.confirming ? "Confirmar: apagar tudo" : "Limpar";
  $("#clrcancel").hidden = !state.confirming;
}

export async function refreshHistory() {
  try {
    state.page = await api.queryHistory(query());
    render();
  } catch (e) {
    showToast({ title: "Histórico indisponível", body: e.message, event: { severity: "warn" } });
  }
}

/** Starts or stops the periodic refresh depending on whether the screen is open. */
export function setHistoryVisible(visible) {
  clearInterval(state.timer);
  state.timer = null;
  if (visible) {
    refreshHistory();
    state.timer = setInterval(refreshHistory, 5000);
  }
}

for (const id of ["hf-sev", "hf-rule", "hf-kind", "hf-test"]) $("#" + id).addEventListener("change", refreshHistory);

$("#hexport").addEventListener("click", async () => {
  try {
    const path = await api.exportHistory(query());
    if (path) showToast({ title: "Histórico exportado", body: path, event: { severity: "info", kind: "resolved" } });
  } catch (e) {
    showToast({ title: "Não foi possível exportar", body: e.message, event: { severity: "warn" } });
  }
});

$("#clrlog").addEventListener("click", async () => {
  if (!state.confirming) {
    state.confirming = true;
    render();
    return;
  }
  state.confirming = false;
  try {
    await api.clearHistory();
  } catch (e) {
    showToast({ title: "Não foi possível limpar", body: e.message, event: { severity: "warn" } });
  }
  await refreshHistory();
  document.dispatchEvent(new CustomEvent("argus:history-changed"));
});
$("#clrcancel").addEventListener("click", () => {
  state.confirming = false;
  render();
});

$("#hret").addEventListener("change", async (e) => {
  try {
    state.page = await api.setHistoryRetention(Number(e.target.value));
    render();
    showToast({ title: "Retenção atualizada", body: `O histórico é mantido por ${e.target.value} dias.`, event: { severity: "info", kind: "resolved" } });
  } catch (err) {
    showToast({ title: "Não foi possível salvar", body: err.message, event: { severity: "warn" } });
  }
});
