// Preview-only: the shared header and footer, and the theme (dark by default, `?light` for paper). Not product code.
(() => {
  const qs = new URLSearchParams(location.search);
  if (qs.has("light")) document.documentElement.classList.add("light");
  const light = qs.has("light");
  const mark = `<svg viewBox="0 0 32 32" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" aria-hidden="true"><path d="M16 5 27 11.3v12.4L16 30 5 23.7V11.3z"/><path d="M5 11.3 16 17.6 27 11.3M16 17.6V30"/><path d="M8.5 9.3 16 13.6l7.5-4.3"/><ellipse cx="16" cy="7.4" rx="4.6" ry="2.2"/><path d="M7.5 20.5l8.5 4.9 8.5-4.9M7.5 16.9l8.5 4.9 8.5-4.9"/></svg>`;
  const sun = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/></svg>`;
  const moon = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z"/></svg>`;
  window.MARK = mark;
  window.chrome = (on, cta = true) => {
    const nav = [["Overview", "landing"], ["Size", "size"], ["Method", "method"], ["Limitations", "lim"]]
      .map(([t, k]) => `<a href="#" class="${k === on ? "on" : ""}">${t}</a>`).join("");
    const h = document.querySelector("header[data-chrome]");
    if (h) h.outerHTML = `<header class="wrap top"><a class="brand" href="#">${mark}Drafthouse<small>Synergy</small></a>
      <nav>${nav}<button class="mode keep" aria-label="${light ? "Dark mode" : "Light mode"}">${light ? moon : sun}</button>${cta ? `<a class="btn sm pri keep" href="#">Start sizing</a>` : ""}</nav></header>`;
    const f = document.querySelector("footer[data-chrome]");
    if (f) f.outerHTML = `<footer class="foot"><div class="wrap">${mark}<span>Drafthouse · Synergy Services</span><a href="#">Source</a><a href="#">Method</a><a href="#">Limitations</a><span>PolyForm Noncommercial 1.0.0</span><span class="end">Synthetic catalog. Not CTI certified. · v1.0.0</span></div></footer>`;
  };
  window.flowColor = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  window.settle = () => { document.body.dataset.h = Math.ceil(document.documentElement.scrollHeight); };
})();
