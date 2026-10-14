// Pinned sensors ("Meu painel"). The list lives in the backend config (so it survives restarts and
// reinstalls); this module keeps a local copy and talks to the backend.
import { api } from "./bridge.js";

let list = [];

export const favorites = () => list;
export const isFavorite = (id) => list.includes(id);

export async function loadFavorites() {
  list = await api.favorites();
  return list;
}

/** Pins or unpins a sensor. The local copy changes first so the star answers at once. */
export async function setFavorite(id, on) {
  const before = list;
  list = on ? [...list.filter((s) => s !== id), id] : list.filter((s) => s !== id);
  try {
    list = await api.setFavorite(id, on);
  } catch (e) {
    list = before;
    throw e;
  }
  document.dispatchEvent(new CustomEvent("argus:rerender"));
  return list;
}
