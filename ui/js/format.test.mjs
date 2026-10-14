import test from "node:test";
import assert from "node:assert/strict";
import { bytesText, present, setDisplayPrefs, withUnit } from "./format.js";

test.afterEach(() => setDisplayPrefs({ temperature_unit: "C", memory_unit: "GB" }));

test("temperatures follow the chosen unit but stay canonical for alerts", () => {
  assert.equal(withUnit("celsius", 100), "100,0 °C");
  setDisplayPrefs({ temperature_unit: "F", memory_unit: "GB" });
  assert.equal(withUnit("celsius", 100), "212,0 °F");
  assert.equal(withUnit("celsius", 0), "32,0 °F");
  assert.deepEqual(present("celsius", 100, true), { text: "100,0", unit: "°C" });
});

test("memory switches between decimal GB and binary GiB", () => {
  const bytes = 16 * 1024 ** 3; // what Windows calls 16 GB of RAM
  assert.equal(bytesText(bytes), "17,2 GB");
  setDisplayPrefs({ temperature_unit: "C", memory_unit: "GiB" });
  assert.equal(bytesText(bytes), "16,0 GiB");
  assert.equal(withUnit("bytes", bytes), "16,0 GiB");
  assert.deepEqual(present("bytes", bytes, true), { text: "17,2", unit: "GB" });
});

test("unknown preference values fall back to the defaults", () => {
  setDisplayPrefs({ temperature_unit: "K", memory_unit: "MB" });
  assert.equal(withUnit("celsius", 50), "50,0 °C");
  assert.equal(bytesText(1e9), "1,0 GB");
});
