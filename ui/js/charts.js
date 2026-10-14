// SVG charts drawn from the store. The x axis is time: a point's position depends on its
// timestamp, so a pause or a different collection interval never distorts the line.
import { fmt } from "./format.js";

const css = (n) => getComputedStyle(document.documentElement).getPropertyValue("--" + n).trim();
const $ = (s) => document.querySelector(s);

export function niceMax(m) {
  return [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10000].find((x) => x >= m * 1.1) ?? m;
}

const label = (windowMs) => (windowMs >= 60000 ? `-${windowMs / 60000} min` : `-${windowMs / 1000} s`);

/**
 * @param {string|SVGElement} el
 * @param {object[]} series  [{ id, color, name, unit, fill }]
 * @param {{ store, windowMs:number, endMs:number, max?:number|"auto", unit?:string }} opts
 */
export function chart(el, series, { store, windowMs, endMs, max: maxIn = 100, unit = "", line = null }) {
  const svg = typeof el === "string" ? $(el) : el;
  const W = svg.clientWidth || 600;
  const H = svg.clientHeight || 220;
  const pl = 34, pb = 18, pt = 6;
  const iw = W - pl - 4;
  const ih = H - pb - pt;
  const from = endMs - windowMs;
  const sc = (s, v) => (s.convert ? s.convert(v) : v * (s.scale ?? 1));
  const data = series.map((s) => ({ s, pts: store.window(s.id, windowMs, endMs) }));

  let max = maxIn;
  if (max === "auto") {
    let m = 0;
    for (const { s, pts } of data) for (const p of pts) if (p.a.kind === "ok") m = Math.max(m, sc(s, p.a.value));
    max = niceMax(m);
  }
  const x = (ts) => pl + (iw * (ts - from)) / windowMs;
  const y = (v) => pt + ih * (1 - Math.min(Math.max(v, 0), max) / max);

  let g = "";
  for (let i = 0; i <= 4; i++) {
    const yy = pt + (ih * i) / 4;
    g += `<line x1="${pl}" x2="${W}" y1="${yy}" y2="${yy}" stroke="${css("line")}" stroke-width="1"/>`;
    g += `<text x="0" y="${yy + 4}" fill="${css("mute")}" font-size="10">${+(max * (1 - i / 4)).toFixed(max < 10 ? 1 : 0)}</text>`;
  }
  g += `<text x="${pl}" y="${H - 3}" fill="${css("mute")}" font-size="10">${label(windowMs)}</text>`;
  g += `<text x="${W - 22}" y="${H - 3}" fill="${css("mute")}" font-size="10">agora</text>`;

  if (line != null && Number.isFinite(line) && line <= max) {
    const ly = y(line);
    g += `<line x1="${pl}" x2="${W}" y1="${ly}" y2="${ly}" stroke="${css("bad")}" stroke-dasharray="5 4"/><text x="${W - 70}" y="${ly - 4}" fill="${css("bad")}" font-size="10">limite ${fmt(line, 0)}</text>`;
  }
  for (const { s, pts } of data) {
    const col = css(s.color);
    // Split into runs of readable points: an unavailable stretch leaves a gap, never a line to zero.
    const runs = [];
    let run = [];
    for (const p of pts) {
      if (p.a.kind === "ok") run.push([x(p.ts), y(sc(s, p.a.value))]);
      else if (run.length) { runs.push(run); run = []; }
    }
    if (run.length) runs.push(run);
    for (const r of runs) {
      if (r.length < 2) continue;
      const d = r.map((p, i) => (i ? "L" : "M") + p[0].toFixed(1) + "," + p[1].toFixed(1)).join("");
      if (s.fill !== false) g += `<path d="${d}L${r.at(-1)[0]},${pt + ih}L${r[0][0]},${pt + ih}Z" fill="${col}" opacity=".12"/>`;
      g += `<path d="${d}" fill="none" stroke="${col}" stroke-width="1.8" stroke-linejoin="round"/>`;
    }
    const last = runs.at(-1)?.at(-1);
    if (last) g += `<circle cx="${last[0]}" cy="${last[1]}" r="3" fill="${col}"/>`;
  }
  svg.setAttribute("viewBox", `0 0 ${W} ${H}`);
  svg.innerHTML = g;

  svg.onmousemove = (e) => {
    const r = svg.getBoundingClientRect();
    const px = e.clientX - r.left;
    const ts = from + (Math.max(0, Math.min(1, (px - pl) / iw)) * windowMs);
    let tip = "";
    for (const { s, pts } of data) {
      let best = null;
      for (const p of pts) if (best == null || Math.abs(p.ts - ts) < Math.abs(best.ts - ts)) best = p;
      if (best?.a.kind === "ok" && Math.abs(best.ts - ts) < Math.max(3000, windowMs / 60)) {
        tip += `<tspan fill="${css(s.color)}"> ${s.name} ${fmt(sc(s, best.a.value), 1)}${s.unitLabel ?? unit}</tspan>`;
      }
    }
    svg.querySelectorAll(".hv").forEach((n) => n.remove());
    svg.insertAdjacentHTML(
      "beforeend",
      `<g class="hv"><line x1="${px}" x2="${px}" y1="${pt}" y2="${pt + ih}" stroke="${css("mute")}" stroke-dasharray="3"/><text x="${pl + 6}" y="${pt + 12}" font-size="11" class="mono">${tip}</text></g>`,
    );
  };
  svg.onmouseleave = () => svg.querySelectorAll(".hv").forEach((n) => n.remove());
}

/** Small area sparkline of the last `windowMs`. Returns "" when there is nothing to draw. */
export function spark(store, id, color, { windowMs, endMs, max = 100 }) {
  const pts = store.window(id, windowMs, endMs).filter((p) => p.a.kind === "ok");
  if (pts.length < 2) return "";
  const W = 200, H = 46, from = endMs - windowMs;
  const d = pts
    .map((p, i) => (i ? "L" : "M") + ((W * (p.ts - from)) / windowMs).toFixed(1) + "," + (H - 2 - ((H - 6) * Math.min(p.a.value, max)) / max).toFixed(1))
    .join("");
  const first = ((W * (pts[0].ts - from)) / windowMs).toFixed(1);
  const lastX = ((W * (pts.at(-1).ts - from)) / windowMs).toFixed(1);
  return `<svg class="spark" viewBox="0 0 ${W} ${H}" preserveAspectRatio="none" aria-hidden="true"><path d="${d}L${lastX},${H}L${first},${H}Z" fill="${css(color)}" opacity=".13"/><path d="${d}" fill="none" stroke="${css(color)}" stroke-width="1.6" vector-effect="non-scaling-stroke"/></svg>`;
}
