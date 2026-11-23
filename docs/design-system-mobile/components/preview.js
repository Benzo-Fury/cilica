// Preview-only helper: renders one component for iOS and Android, in light and dark.
// Usage: cpreview(p => `<markup for platform p ("ios" | "and")>`, { note: "caption" })
window.cpreview = function (fn, opts = {}) {
  const I = window.cicon;
  const cols = [["ios", "light", "iOS"], ["and", "light", "Android"], ["ios", "dark", "iOS dark"], ["and", "dark", "Android dark"]];
  const root = document.createElement("div");
  root.className = "pv";
  root.innerHTML = cols.map(([p, t, label]) => `
    <div class="pv-col ${opts.sunken ? "sunken" : ""} ${p}" data-theme="${t}" style="${opts.style || ""}">
      <div class="pv-tag">${label}</div>
      ${fn(p, I)}
    </div>`).join("") + (opts.note ? `<p class="pv-note" style="grid-column:1/-1;margin:0">${opts.note}</p>` : "");
  document.body.appendChild(root);
};
