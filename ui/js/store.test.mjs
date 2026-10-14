import test from "node:test";
import assert from "node:assert/strict";
import { Store, RETENTION_MS } from "./store.js";

const ok = (value) => ({ kind: "ok", value });
const r = (sensor, ts_ms, value) => ({ sensor, ts_ms, value });

test("a window is measured by time, not by the number of samples", () => {
  const s = new Store();
  for (let t = 0; t <= 10_000; t += 1000) s.push([r("a", t, ok(t))]);
  assert.equal(s.window("a", 4000, 10_000).length, 5);
  for (let t = 0; t <= 10_000; t += 5000) s.push([r("b", t, ok(t))]);
  assert.equal(s.window("b", 4000, 10_000).length, 1);
});

test("history seeded after ticks is merged in order without duplicates", () => {
  const s = new Store();
  s.push([r("a", 3000, ok(3)), r("a", 4000, ok(4))]);
  s.seed([{ sensor: "a", samples: [{ ts_ms: 1000, value: ok(1) }, { ts_ms: 2000, value: ok(2) }, { ts_ms: 3000, value: ok(3) }] }]);
  assert.deepEqual(s.window("a", 10_000, 4000).map((p) => p.ts), [1000, 2000, 3000, 4000]);
});

test("old points are dropped by retention time", () => {
  const s = new Store();
  s.push([r("a", 0, ok(1))]);
  s.push([r("a", RETENTION_MS + 1, ok(2))]);
  assert.equal(s.window("a", RETENTION_MS * 2, RETENTION_MS + 1).length, 1);
});

test("an unavailable sensor has no value and says why", () => {
  const s = new Store();
  s.push([r("t", 1, { kind: "unavailable", reason: "sem driver" })]);
  assert.equal(s.value("t"), null);
  assert.equal(s.reason("t"), "sem driver");
  s.push([r("n", 1, { kind: "no_response", since_ms: 1 })]);
  assert.match(s.reason("n"), /não respondeu/);
  assert.equal(s.reason("missing"), "ainda sem leitura");
});

test("levels come from the backend limits", () => {
  const s = new Store();
  s.setCatalog([{ meta: { id: "t", group: "g", name: "n", unit: "celsius" }, threshold: { warn: 80, crit: 90 } }]);
  s.push([r("t", 1, ok(70))]);
  assert.equal(s.level("t"), "ok");
  s.push([r("t", 2, ok(85))]);
  assert.equal(s.level("t"), "warn");
  s.push([r("t", 3, ok(95))]);
  assert.equal(s.level("t"), "crit");
});

test("a view as of an earlier moment keeps showing what was true then", () => {
  const s = new Store();
  s.push([r("a", 1000, ok(1)), r("a", 2000, ok(2)), r("a", 3000, { kind: "unavailable", reason: "x" })]);
  const paused = s.view(2000);
  s.push([r("a", 4000, ok(4))]);
  assert.equal(paused.value("a"), 2);
  assert.equal(s.value("a"), 4);
  assert.equal(s.view(3000).value("a"), null);
  assert.equal(s.view(3000).reason("a"), "x");
  assert.equal(paused.window("a", 10_000, 99_999).length, 2);
});
