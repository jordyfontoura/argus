import test from "node:test";
import assert from "node:assert/strict";
import { filterRows, formatMemory, orderRows, sortRows } from "./processes-model.js";

const row = (pid, name, cpu, mem) => ({ pid, name, cpu_percent: cpu, memory_bytes: mem * 1e6 });
const rows = [row(10, "chrome.exe", 9, 2100), row(20, "Code.exe", 6, 1400), row(30, "svchost.exe", 0.6, 120), row(40, "Discord.exe", 3, 650)];
const pids = (list) => list.map((r) => r.pid);

test("sorts by each column in both directions, case-insensitively for names", () => {
  assert.deepEqual(pids(sortRows(rows, "cpu", -1)), [10, 20, 40, 30]);
  assert.deepEqual(pids(sortRows(rows, "cpu", 1)), [30, 40, 20, 10]);
  assert.deepEqual(pids(sortRows(rows, "mem", -1)), [10, 20, 40, 30]);
  assert.deepEqual(pids(sortRows(rows, "pid", 1)), [10, 20, 30, 40]);
  assert.deepEqual(pids(sortRows(rows, "name", 1)), [10, 20, 40, 30], "chrome, Code, Discord, svchost");
});

test("ties are broken by PID so the order does not flicker", () => {
  const same = [row(5, "a", 1, 1), row(3, "b", 1, 1), row(4, "c", 1, 1)];
  assert.deepEqual(pids(sortRows(same, "cpu", -1)), [5, 4, 3], "equal values: descending order uses descending PIDs");
  assert.deepEqual(pids(sortRows(same, "cpu", 1)), [3, 4, 5]);
});

test("filters by name fragment or exact PID", () => {
  assert.deepEqual(pids(filterRows(rows, "EXE")), [10, 20, 30, 40]);
  assert.deepEqual(pids(filterRows(rows, "disc")), [40]);
  assert.deepEqual(pids(filterRows(rows, "20")), [20]);
  assert.deepEqual(filterRows(rows, "nada"), []);
  assert.equal(filterRows(rows, "  ").length, 4);
});

test("the order is frozen while hovering: values change, positions do not", () => {
  const frozen = pids(sortRows(rows, "cpu", -1)); // [10,20,40,30]
  // CPU values swap, which would normally reorder the table.
  const later = [row(10, "chrome.exe", 1, 2100), row(20, "Code.exe", 2, 1400), row(30, "svchost.exe", 50, 120), row(40, "Discord.exe", 3, 650)];
  assert.deepEqual(pids(orderRows(later, { key: "cpu", dir: -1, query: "" }, null)), [30, 40, 20, 10]);
  assert.deepEqual(pids(orderRows(later, { key: "cpu", dir: -1, query: "" }, frozen)), frozen);
});

test("frozen order drops vanished processes and appends new ones sorted at the end", () => {
  const frozen = [10, 20, 40, 30];
  const later = [row(10, "chrome.exe", 1, 1), row(40, "Discord.exe", 3, 1), row(99, "new.exe", 9, 1), row(98, "newer.exe", 20, 1)];
  assert.deepEqual(pids(orderRows(later, { key: "cpu", dir: -1, query: "" }, frozen)), [10, 40, 98, 99]);
});

test("the filter still applies while frozen", () => {
  assert.deepEqual(pids(orderRows(rows, { key: "cpu", dir: -1, query: "code" }, [10, 20, 30, 40])), [20]);
});

test("memory shows MB below a gigabyte and GB above", () => {
  assert.equal(formatMemory(120e6), "120 MB");
  assert.equal(formatMemory(2100e6), "2,1 GB");
  assert.equal(formatMemory(999e6), "999 MB");
});
