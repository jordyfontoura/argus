// The only place that knows whether the UI runs inside the Tauri app or in a plain browser.
// Command and event names come from the generated contract (ui/generated/names.js).
import { COMMANDS, EVENTS } from "../generated/names.js";
import { createDemo } from "./demo.js";

const tauri = globalThis.__TAURI__;

/** True when there is no backend (opened in a browser, used for captures and tests). */
export const isDemo = !tauri?.core;

const demo = isDemo ? createDemo() : null;

async function invoke(name, args) {
  if (demo) return demo.invoke(name, args);
  try {
    return await tauri.core.invoke(name, args);
  } catch (e) {
    // Commands fail with { code, message }; surface the message and let the screen carry on.
    const err = new Error(e?.message ?? String(e));
    err.code = e?.code;
    throw err;
  }
}

export const api = {
  /** Explicit activation only; Windows asks for administrator consent. @returns {Promise<string>} */
  activateCpuMeasurements: () => invoke(COMMANDS.activateCpuMeasurements),
  /** @returns {Promise<import("../generated/StaticInfo").StaticInfo>} */
  staticInfo: () => invoke(COMMANDS.getStaticInfo),
  /** @returns {Promise<import("../generated/CpuDetails").CpuDetails>} */
  cpuDetails: () => invoke(COMMANDS.getCpuDetails),
  /** @returns {Promise<import("../generated/GpuDetails").GpuDetails[]>} */
  gpuDetails: () => invoke(COMMANDS.getGpuDetails),
  /** @returns {Promise<import("../generated/MemoryDetails").MemoryDetails>} */
  memoryDetails: () => invoke(COMMANDS.getMemoryDetails),
  /** @returns {Promise<import("../generated/NetworkAdapter").NetworkAdapter[]>} */
  networkAdapters: () => invoke(COMMANDS.getNetworkAdapters),
  /** @returns {Promise<import("../generated/Rule").Rule[]>} */
  listRules: () => invoke(COMMANDS.listRules),
  /** @returns {Promise<import("../generated/Rule").Rule>} */
  saveRule: (rule) => invoke(COMMANDS.saveRule, { rule }),
  deleteRule: (id) => invoke(COMMANDS.deleteRule, { id }),
  acknowledgeRule: (id) => invoke(COMMANDS.acknowledgeRule, { id }),
  /** @returns {Promise<import("../generated/SimResult").SimResult>} */
  simulateRule: (rule) => invoke(COMMANDS.simulateRule, { rule }),
  /** @returns {Promise<import("../generated/AlertsState").AlertsState>} */
  setMute: (seconds) => invoke(COMMANDS.setMute, { seconds }),
  /** @returns {Promise<import("../generated/AlertsState").AlertsState>} */
  setCriticalBypass: (on) => invoke(COMMANDS.setCriticalBypass, { on }),
  /** @returns {Promise<import("../generated/PresetInfo").PresetInfo[]>} */
  listPresets: () => invoke(COMMANDS.listPresets),
  /** @returns {Promise<import("../generated/PlannedRule").PlannedRule[]>} */
  previewPreset: (preset, sensitivity, actions) => invoke(COMMANDS.previewPreset, { preset, sensitivity, actions }),
  /** @returns {Promise<import("../generated/Rule").Rule[]>} */
  applyPreset: (preset, sensitivity, actions, chosen, keepOwn) => invoke(COMMANDS.applyPreset, { preset, sensitivity, actions, chosen, keepOwn }),
  /** @returns {Promise<import("../generated/HistoryPage").HistoryPage>} */
  queryHistory: (query) => invoke(COMMANDS.queryHistory, { query }),
  /** @returns {Promise<string|null>} the saved path, or null when the dialog was cancelled */
  exportHistory: (query) => invoke(COMMANDS.exportHistory, { query }),
  clearHistory: () => invoke(COMMANDS.clearHistory),
  /** @returns {Promise<import("../generated/HistoryPage").HistoryPage>} */
  setHistoryRetention: (days) => invoke(COMMANDS.setHistoryRetention, { days }),
  /** @returns {Promise<import("../generated/BackgroundState").BackgroundState>} */
  backgroundState: () => invoke(COMMANDS.getBackgroundState),
  /** @param {{autostart?: boolean, minimizeToTray?: boolean}} change */
  setBackground: (change) => invoke(COMMANDS.setBackground, { autostart: change.autostart ?? null, minimizeToTray: change.minimize_to_tray ?? null }),
  /** @returns {Promise<import("../generated/SessionRange").SessionRange[]>} */
  sessionRanges: () => invoke(COMMANDS.getSessionRanges),
  resetSessionRanges: () => invoke(COMMANDS.resetSessionRanges),
  /** @returns {Promise<string[]>} sensor ids in the order they were pinned */
  favorites: () => invoke(COMMANDS.getFavorites),
  /** @returns {Promise<string[]>} */
  setFavorite: (sensor, on) => invoke(COMMANDS.setFavorite, { sensor, on }),
  /** @returns {Promise<import("../generated/ProcessRow").ProcessRow[]>} */
  listProcesses: () => invoke(COMMANDS.listProcesses),
  /** @returns {Promise<import("../generated/DiskInfo").DiskInfo[]>} */
  disks: () => invoke(COMMANDS.getDisks),
  /** @returns {Promise<import("../generated/VolumeInfo").VolumeInfo[]>} */
  volumes: () => invoke(COMMANDS.getVolumes),
  /** @returns {Promise<import("../generated/AppSettings").AppSettings>} */
  settings: () => invoke(COMMANDS.getSettings),
  /** @returns {Promise<import("../generated/AppSettings").AppSettings>} */
  setSettings: (settings) => invoke(COMMANDS.setSettings, { settings }),
  /** @returns {Promise<import("../generated/SystemDetails").SystemDetails>} */
  systemDetails: () => invoke(COMMANDS.getSystemDetails),
  /** @returns {Promise<string>} the plain-text report */
  report: () => invoke(COMMANDS.getReport),
  /** @returns {Promise<string|null>} the saved path, or null when the dialog was cancelled */
  exportSensorCsv: () => invoke(COMMANDS.exportSensorCsv),
  /** @returns {Promise<import("../generated/AlertsState").AlertsState>} */
  alertsState: () => invoke(COMMANDS.getAlertsState),
  /** @returns {Promise<import("../generated/SensorInfo").SensorInfo[]>} */
  listSensors: () => invoke(COMMANDS.listSensors),
  /** @returns {Promise<import("../generated/SensorHistory").SensorHistory[]>} */
  history: (sensorIds, windowMs) => invoke(COMMANDS.getHistory, { sensorIds, windowMs }),
  setSampleInterval: (intervalMs) => invoke(COMMANDS.setSampleInterval, { intervalMs }),
};

/** Calls `handler(readings)` for every collection cycle. Returns an unsubscribe function. */
export async function onTick(handler) {
  if (demo) return demo.onTick(handler);
  return tauri.event.listen(EVENTS.sensorsTick, (e) => handler(e.payload.readings));
}

/** Subscribes to a backend event by its generated name (EVENTS.alertsState, ...). */
export async function onEvent(name, handler) {
  if (demo) return demo.onEvent(name, handler);
  return tauri.event.listen(name, (e) => handler(e.payload));
}

export { EVENTS };
