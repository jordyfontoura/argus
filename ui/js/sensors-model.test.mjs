import test from "node:test";
import assert from "node:assert/strict";
import { barPercent, buildGroups, groupOf, pinnedRows } from "./sensors-model.js";

const info = (id, group, name, unit, threshold = null) => ({ meta: { id, group, name, unit }, threshold });
const catalog = new Map(
  [
    info("mem.used_pct", "Memória", "Uso", "percent", { warn: 80, crit: 92 }),
    info("cpu.temp", "CPU", "Temperatura", "celsius", { warn: 80, crit: 90 }),
    info("cpu.load", "CPU", "Carga total", "percent"),
    info("cpu.thread.0.load", "CPU", "Thread 0", "percent"),
    info("gpu.0.temp", "GPU", "Temperatura", "celsius"),
    info("net.total.rx", "Rede", "Recebido (total)", "mbps"),
    info("odd.x", "Zeta", "Estranho", "count"),
  ].map((i) => [i.meta.id, i]),
);
const values = { "mem.used_pct": 85, "cpu.load": 12.5, "cpu.thread.0.load": 7, "gpu.0.temp": 55, "net.total.rx": 3 };
const reasons = { "cpu.temp": "a placa não expõe zonas térmicas" };
const view = {
  value: (id) => values[id] ?? null,
  reason: (id) => reasons[id] ?? "sem leitura",
  level: (id) => (id === "mem.used_pct" ? "warn" : "ok"),
};
const base = { catalog, view, favs: new Set(), query: "", onlyFavs: false, collapsed: new Set(), ranges: new Map() };

test("threads get their own group and groups follow the fixed order, unknown ones last", () => {
  assert.equal(groupOf(catalog.get("cpu.thread.0.load").meta), "CPU › Threads");
  assert.deepEqual(buildGroups(base).map((g) => g.group), ["CPU", "CPU › Threads", "GPU", "Memória", "Rede", "Zeta"]);
});

test("unavailable sensors stay listed with the reason instead of a value", () => {
  const cpu = buildGroups(base).find((g) => g.group === "CPU");
  const temp = cpu.rows.find((r) => r.id === "cpu.temp");
  assert.equal(temp.ok, false);
  assert.equal(temp.reason, "a placa não expõe zonas térmicas");
  assert.equal(temp.cur, null);
  const load = cpu.rows.find((r) => r.id === "cpu.load");
  assert.equal(load.cur, "12,5 %");
});

test("search matches group and name, case-insensitively", () => {
  const found = buildGroups({ ...base, query: "TEMPERATURA" }).flatMap((g) => g.rows.map((r) => r.id));
  assert.deepEqual(found.sort(), ["cpu.temp", "gpu.0.temp"]);
  assert.deepEqual(buildGroups({ ...base, query: "memória uso" }).flatMap((g) => g.rows.map((r) => r.id)), ["mem.used_pct"]);
  assert.deepEqual(buildGroups({ ...base, query: "nada disso" }), []);
});

test("collapsed groups hide rows but a search still shows matches", () => {
  const collapsed = new Set(["CPU"]);
  assert.equal(buildGroups({ ...base, collapsed }).find((g) => g.group === "CPU").collapsed, true);
  assert.equal(buildGroups({ ...base, collapsed, query: "carga" }).find((g) => g.group === "CPU").collapsed, false);
});

test("only-favorites filters and marks the stars", () => {
  const favs = new Set(["cpu.temp", "gpu.0.temp"]);
  const groups = buildGroups({ ...base, favs, onlyFavs: true });
  assert.deepEqual(groups.flatMap((g) => g.rows.map((r) => r.id)).sort(), ["cpu.temp", "gpu.0.temp"]);
  assert.ok(groups.every((g) => g.rows.every((r) => r.fav)));
});

test("min and max come from the session ranges and are formatted in the sensor's unit", () => {
  const ranges = new Map([["cpu.load", { min: 3, max: 99.5 }]]);
  const row = buildGroups({ ...base, ranges }).find((g) => g.group === "CPU").rows.find((r) => r.id === "cpu.load");
  assert.deepEqual([row.min, row.max], ["3,0 %", "99,5 %"]);
  const none = buildGroups(base).find((g) => g.group === "CPU").rows.find((r) => r.id === "cpu.load");
  assert.equal(none.min, null);
});

test("bar fill is exact for percentages and bounded for everything else", () => {
  assert.equal(barPercent("percent", 42, null), 42);
  assert.equal(barPercent("percent", 140, null), 100);
  assert.equal(barPercent("celsius", 55, null), 50);
  assert.equal(barPercent("mbps", 0, null), 0);
  assert.equal(barPercent("count", 5, null), 0);
  assert.ok(barPercent("watt", 30, { max: 60 }) < 100);
});

test("pinned sensors keep their pin order and drop ids that no longer exist", () => {
  const rows = pinnedRows(catalog, ["gpu.0.temp", "gone.sensor", "cpu.load"]);
  assert.deepEqual(rows.map((r) => r.meta.id), ["gpu.0.temp", "cpu.load"]);
});

test("a level comes from the backend limits only", () => {
  const row = buildGroups(base).find((g) => g.group === "Memória").rows[0];
  assert.equal(row.level, "warn");
});
