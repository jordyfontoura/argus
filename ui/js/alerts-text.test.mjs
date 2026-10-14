import test from "node:test";
import assert from "node:assert/strict";
import { duration, extras, liveState, sentence } from "./alerts-text.js";

const rule = (over = {}) => ({
  id: 1, name: "r", sensor: "cpu.temp", mode: "sustained", op: "above", threshold: 85, hysteresis: null,
  for_sec: 10, count: 3, recover_sec: 5, repeat_sec: 0, severity: "crit",
  actions: { toast: true, sound: false, notify: false }, enabled: true, ...over,
});
const metric = { name: "CPU · Temperatura", unit: "°C" };

test("durations read naturally", () => {
  assert.equal(duration(30), "30 s");
  assert.equal(duration(60), "1 min");
  assert.equal(duration(90), "90 s");
  assert.equal(duration(7200), "2 h");
});

test("sentences describe each rule type", () => {
  assert.equal(sentence(rule(), metric), "CPU · Temperatura > 85 °C por 10 s seguidos");
  assert.equal(sentence(rule({ for_sec: 0 }), metric), "CPU · Temperatura > 85 °C (imediato)");
  assert.equal(sentence(rule({ mode: "average", for_sec: 60 }), metric), "Média de CPU · Temperatura nos últimos 1 min > 85 °C");
  assert.equal(sentence(rule({ mode: "spikes", for_sec: 300, count: 3 }), metric), "CPU · Temperatura > 85 °C, 3 vezes em 5 min");
  assert.equal(sentence(rule({ op: "below", threshold: 300 }), { name: "Ventoinha", unit: "RPM" }), "Ventoinha < 300 RPM por 10 s seguidos");
});

test("extras mention the resolve level, the wait and the repetition", () => {
  assert.equal(extras(rule({ hysteresis: 80, repeat_sec: 60 }), "°C"), "resolve abaixo de 80 °C · após 5 s normal · repete a cada 1 min");
  assert.equal(extras(rule({ op: "below", hysteresis: 500, recover_sec: 0 }), "RPM"), "resolve acima de 500 RPM · avisa uma vez");
  assert.equal(extras(rule({ mode: "spikes", hysteresis: 80, recover_sec: 0 }), "°C"), "avisa uma vez");
});

test("the live state follows the engine's phase", () => {
  assert.equal(liveState(rule({ enabled: false }), null).label, "Desativada");
  assert.equal(liveState(rule(), null).label, "OK");
  const pending = { phase: "pending", elapsed_ms: 4200, needed_ms: 10000, sensor_available: true };
  assert.equal(liveState(rule(), pending).label, "Contando 4/10 s");
  assert.equal(liveState(rule({ mode: "average" }), { ...pending, needed_ms: 0 }).label, "Avaliando");
  const firing = liveState(rule(), { phase: "firing", sensor_available: true });
  assert.deepEqual([firing.label, firing.canAck, firing.card], ["Disparado", true, "firing"]);
  const acked = liveState(rule(), { phase: "acknowledged", sensor_available: true });
  assert.deepEqual([acked.label, acked.canAck], ["Reconhecido", undefined]);
  assert.equal(liveState(rule(), { phase: "ok", sensor_available: false }).label, "Sem leitura");
});
