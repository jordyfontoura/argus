// Client-side copy of the recent readings. Windows are measured by timestamp, never by count.
export const RETENTION_MS = 20 * 60 * 1000;

export class Store {
  constructor() {
    /** @type {Map<string, {ts:number, a:object}[]>} */
    this.series = new Map();
    /** @type {Map<string, object>} sensor id -> SensorInfo */
    this.catalog = new Map();
  }

  setCatalog(list) {
    this.catalog = new Map(list.map((s) => [s.meta.id, s]));
  }

  info(id) {
    return this.catalog.get(id);
  }

  /** Adds readings from a tick; out-of-order points are slotted in by timestamp. */
  push(readings) {
    for (const r of readings) this.#insert(r.sensor, r.ts_ms, r.value);
  }

  /** Merges history fetched with `get_history`. */
  seed(histories) {
    for (const h of histories) for (const s of h.samples) this.#insert(h.sensor, s.ts_ms, s.value);
  }

  #insert(id, ts, a) {
    let arr = this.series.get(id);
    if (!arr) this.series.set(id, (arr = []));
    const last = arr[arr.length - 1];
    if (!last || ts > last.ts) arr.push({ ts, a });
    else {
      let i = arr.length;
      while (i > 0 && arr[i - 1].ts > ts) i--;
      if (arr[i - 1]?.ts === ts) arr[i - 1] = { ts, a };
      else arr.splice(i, 0, { ts, a });
    }
    const cutoff = arr[arr.length - 1].ts - RETENTION_MS;
    let drop = 0;
    while (drop < arr.length && arr[drop].ts < cutoff) drop++;
    if (drop) arr.splice(0, drop);
  }

  /** The same reads, as of `endMs` (a paused screen keeps looking at an old moment). */
  view(endMs) {
    const at = (id) => {
      const arr = this.series.get(id);
      if (!arr) return undefined;
      for (let i = arr.length - 1; i >= 0; i--) if (arr[i].ts <= endMs) return arr[i];
      return undefined;
    };
    const base = this;
    return {
      catalog: base.catalog,
      info: (id) => base.info(id),
      window: (id, windowMs, end = endMs) => base.window(id, windowMs, Math.min(end, endMs)),
      value: (id) => (at(id)?.a.kind === "ok" ? at(id).a.value : null),
      reason: (id) => reasonOf(at(id)?.a),
      level: (id) => levelOf(at(id)?.a, base.catalog.get(id)?.threshold),
    };
  }

  /** Newest point of any sensor: the moment the data is "as of". */
  latestTs() {
    let max = 0;
    for (const arr of this.series.values()) max = Math.max(max, arr[arr.length - 1]?.ts ?? 0);
    return max;
  }

  latest(id) {
    const arr = this.series.get(id);
    return arr?.[arr.length - 1];
  }

  /** Numeric value of the newest point, or null when the sensor is not readable now. */
  value(id) {
    const a = this.latest(id)?.a;
    return a?.kind === "ok" ? a.value : null;
  }

  /** Why the sensor has no value right now, or null when it has one. */
  reason(id) {
    return reasonOf(this.latest(id)?.a);
  }

  /** Points with `endMs - windowMs <= ts <= endMs`. */
  window(id, windowMs, endMs) {
    const arr = this.series.get(id) ?? [];
    const from = endMs - windowMs;
    return arr.filter((p) => p.ts >= from && p.ts <= endMs);
  }

  /** "ok" | "warn" | "crit" from the backend's limits, or null when the sensor has none. */
  level(id) {
    return levelOf(this.latest(id)?.a, this.catalog.get(id)?.threshold);
  }
}

function reasonOf(a) {
  if (!a) return "ainda sem leitura";
  if (a.kind === "unavailable") return a.reason;
  if (a.kind === "no_response") return "o coletor não respondeu a tempo";
  return null;
}

function levelOf(a, th) {
  if (a?.kind !== "ok" || !th) return null;
  return a.value >= th.crit ? "crit" : a.value >= th.warn ? "warn" : "ok";
}
