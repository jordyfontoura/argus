// Sensors screen: every sensor in a collapsible tree, with search, pinning (star), session
// minimum/maximum and the reason for sensors that cannot be read.
import { api } from "./bridge.js";
import { buildGroups } from "./sensors-model.js";
import { favorites, isFavorite, setFavorite } from "./favorites.js";
import { showToast } from "./alerts.js";

const $ = (s) => document.querySelector(s);

function h(tag, attrs = {}, ...children) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v == null || (v === false && !k.startsWith("aria-"))) continue;
    if (k === "class") el.className = v;
    else if (k === "text") el.textContent = v;
    else el.setAttribute(k, v === true && !k.startsWith("aria-") ? "" : String(v));
  }
  for (const c of children.flat()) if (c != null) el.append(c);
  return el;
}

const state = { collapsed: new Set(), ranges: new Map(), sig: "", timer: null };

async function refreshRanges() {
  try {
    const list = await api.sessionRanges();
    state.ranges = new Map(list.map((r) => [r.sensor, { min: r.min, max: r.max }]));
    document.dispatchEvent(new CustomEvent("argus:rerender"));
  } catch { /* the table still works without min/max */ }
}

/** Starts or stops the min/max refresh depending on whether the screen is open. */
export function setSensorsVisible(visible) {
  clearInterval(state.timer);
  state.timer = null;
  if (visible) {
    refreshRanges();
    state.timer = setInterval(refreshRanges, 2000);
  }
}

export function renderSensors(view) {
  const query = $("#sq").value;
  const onlyFavs = $("#sfav").checked;
  const groups = buildGroups({
    catalog: view.catalog,
    view,
    favs: new Set(favorites()),
    query,
    onlyFavs,
    collapsed: state.collapsed,
    ranges: state.ranges,
  });
  const table = $("#senstable");
  const structure = JSON.stringify([query, onlyFavs, groups.map((g) => [g.group, g.collapsed, g.rows.map((r) => [r.id, r.fav, r.ok])])]);
  if (structure !== state.sig) {
    state.sig = structure;
    table.textContent = "";
    if (!groups.length) {
      table.append(h("tr", {}, h("td", { class: "meta", style: "padding:18px", text: onlyFavs && !query ? "Nenhum sensor fixado ainda. Use a ★ de qualquer sensor." : "Nenhum sensor encontrado." })));
    } else {
      table.append(
        h("tr", {}, h("th", { style: "width:34px" }), h("th", { text: "Sensor" }), h("th", { class: "num", text: "Atual" }), h("th", { class: "num", text: "Mín (sessão)" }), h("th", { class: "num", text: "Máx (sessão)" }), h("th", { style: "width:22%" })),
      );
    }
    for (const g of groups) {
      table.append(
        h(
          "tr",
          { class: "grp", "data-g": g.group, tabindex: "0", role: "button", "aria-expanded": !g.collapsed, style: "cursor:pointer" },
          h("td", { colspan: 6 }, `${g.collapsed ? "▸" : "▾"} ${g.group} `, h("span", { style: "text-transform:none;font-weight:400", text: `(${g.count})` })),
        ),
      );
      if (g.collapsed) continue;
      for (const r of g.rows) {
        const star = h("button", { class: "star" + (r.fav ? " on" : ""), "data-k": r.id, "aria-label": `Fixar ${r.name} no painel`, "aria-pressed": r.fav, text: "★" });
        if (r.ok) {
          table.append(
            h(
              "tr",
              { "data-k": r.id },
              h("td", {}, star),
              h("td", {}, h("span", { class: "lv" }), r.name),
              h("td", { class: "num mono c-cur" }),
              h("td", { class: "num mono c-min" }),
              h("td", { class: "num mono c-max" }),
              h("td", {}, h("div", { class: "bar" }, h("i"))),
            ),
          );
        } else {
          table.append(
            h(
              "tr",
              { "data-k": r.id, style: "opacity:.75" },
              h("td", {}, star),
              h("td", {}, h("span", { class: "lv na" }), r.name),
              h("td", { colspan: 4 }, h("span", { class: "tag warn", text: "indisponível" }), " ", h("span", { class: "meta", "data-role": "reason" })),
            ),
          );
        }
      }
    }
  }
  // Live values: only text and bar widths change, so focus and scroll are not disturbed.
  for (const g of groups) {
    if (g.collapsed) continue;
    for (const r of g.rows) {
      const tr = table.querySelector(`tr[data-k="${CSS.escape(r.id)}"]`);
      if (!tr) continue;
      if (!r.ok) {
        const reason = tr.querySelector('[data-role="reason"]');
        if (reason && reason.textContent !== r.reason) reason.textContent = r.reason ?? "";
        continue;
      }
      const dot = tr.querySelector(".lv");
      dot.className = "lv " + (r.level === "crit" ? "bad" : r.level === "warn" ? "warn" : "");
      dot.title = { crit: "Crítico", warn: "Elevado", ok: "Normal" }[r.level] ?? "";
      tr.querySelector(".c-cur").textContent = r.cur;
      tr.querySelector(".c-min").textContent = r.min ?? "—";
      tr.querySelector(".c-max").textContent = r.max ?? "—";
      const bar = tr.querySelector(".bar i");
      bar.style.width = r.pct + "%";
      bar.style.background = `var(--${r.level === "crit" ? "bad" : r.level === "warn" ? "warn" : "cpu"})`;
    }
  }
  $("#scollapse").textContent = state.collapsed.size ? "Expandir tudo" : "Recolher tudo";
}

$("#senstable").addEventListener("click", async (e) => {
  const star = e.target.closest(".star");
  if (star) {
    const id = star.dataset.k;
    try {
      await setFavorite(id, !isFavorite(id));
    } catch (err) {
      showToast({ title: "Não foi possível fixar", body: err.message, event: { severity: "warn" } });
    }
    return;
  }
  const grp = e.target.closest("tr.grp");
  if (grp) {
    const g = grp.dataset.g;
    state.collapsed.has(g) ? state.collapsed.delete(g) : state.collapsed.add(g);
    document.dispatchEvent(new CustomEvent("argus:rerender"));
  }
});
$("#senstable").addEventListener("keydown", (e) => {
  const grp = e.target.closest?.("tr.grp");
  if (grp && (e.key === "Enter" || e.key === " ")) {
    e.preventDefault();
    grp.click();
  }
});
$("#sq").addEventListener("input", () => document.dispatchEvent(new CustomEvent("argus:rerender")));
$("#sfav").addEventListener("change", () => document.dispatchEvent(new CustomEvent("argus:rerender")));
$("#scollapse").addEventListener("click", () => {
  const all = new Set([...document.querySelectorAll("#senstable tr.grp")].map((r) => r.dataset.g));
  if (state.collapsed.size >= all.size && all.size) state.collapsed.clear();
  else all.forEach((g) => state.collapsed.add(g));
  document.dispatchEvent(new CustomEvent("argus:rerender"));
});
$("#sessreset").addEventListener("click", async () => {
  try {
    await api.resetSessionRanges();
    state.ranges = new Map();
    await refreshRanges();
    showToast({ title: "Mín/máx zerados", body: "Os valores recomeçam a partir de agora.", event: { severity: "info", kind: "resolved" } });
  } catch (err) {
    showToast({ title: "Não foi possível zerar", body: err.message, event: { severity: "warn" } });
  }
});
