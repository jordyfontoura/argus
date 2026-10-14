// Plain-language text for alert rules. Pure functions (no DOM), so they are unit-tested.
import { fmt } from "./format.js";

export const SEVERITY = { info: "Informação", warn: "Aviso", crit: "Crítico" };
export const MODES = {
  sustained: "Contínua (fica acima/abaixo por um tempo)",
  average: "Média no período",
  spikes: "N ocorrências no período",
};

/** "30 s", "5 min", "2 h". */
export function duration(sec) {
  if (sec >= 3600 && sec % 3600 === 0) return `${sec / 3600} h`;
  if (sec >= 60 && sec % 60 === 0) return `${sec / 60} min`;
  return `${sec} s`;
}

const num = (v) => (Number.isInteger(v) ? String(v) : fmt(v, 1));

/**
 * One sentence describing the rule. `metric` is `{ name, unit }` (unit is the display label).
 */
export function sentence(rule, metric) {
  const op = rule.op === "above" ? ">" : "<";
  const cond = `${metric.name} ${op} ${num(rule.threshold)} ${metric.unit}`.trim();
  if (rule.mode === "average") {
    return `Média de ${metric.name} nos últimos ${duration(rule.for_sec)} ${op} ${num(rule.threshold)} ${metric.unit}`.trim();
  }
  if (rule.mode === "spikes") return `${cond}, ${rule.count} vezes em ${duration(rule.for_sec)}`;
  return rule.for_sec ? `${cond} por ${duration(rule.for_sec)} seguidos` : `${cond} (imediato)`;
}

/** Secondary line: resolve level, recovery wait and repetition. */
export function extras(rule, unit) {
  const parts = [];
  if (rule.hysteresis != null && rule.mode !== "spikes") {
    parts.push(`resolve ${rule.op === "above" ? "abaixo" : "acima"} de ${num(rule.hysteresis)} ${unit}`.trim());
  }
  if (rule.recover_sec) parts.push(`após ${duration(rule.recover_sec)} normal`);
  parts.push(rule.repeat_sec ? `repete a cada ${duration(rule.repeat_sec)}` : "avisa uma vez");
  return parts.join(" · ");
}

/** Label, CSS class and extra info of a rule's live status. */
export function liveState(rule, status) {
  if (!rule.enabled) return { label: "Desativada", css: "off", card: "off" };
  if (!status) return { label: "OK", css: "ok", card: "" };
  const unreadable = status.sensor_available === false;
  switch (status.phase) {
    case "pending": {
      const label = rule.mode === "sustained" && status.needed_ms > 0
        ? `Contando ${Math.min(Math.floor(status.elapsed_ms / 1000), Math.ceil(status.needed_ms / 1000))}/${Math.ceil(status.needed_ms / 1000)} s`
        : "Avaliando";
      return { label, css: "pending", card: "" };
    }
    case "firing":
      return { label: "Disparado", css: "firing", card: "firing", canAck: true };
    case "acknowledged":
      return { label: "Reconhecido", css: "firing", card: "firing" };
    default:
      return unreadable ? { label: "Sem leitura", css: "pending", card: "" } : { label: "OK", css: "ok", card: "" };
  }
}
