// Preview-only: the island's inline SVG poster, the same line drawing as the live tower: accent strokes, faces in the island ground (hidden lines), no shadow.
window.poster = function (g) {
  const c = g.cells[0], [sx, , sz] = c.shell.box.size, st = c.fan.stack, k = 26;
  const cos = Math.cos(Math.PI / 6), sin = Math.sin(Math.PI / 6);
  const P = (x, y, z) => [(x - z) * cos * k, (-(y) + (x + z) * sin * 0.5) * k];
  const pts = (a) => a.map((p) => P(...p).map((v) => v.toFixed(1)).join(",")).join(" ");
  const out = [];
  const poly = (a, fill, extra = "") => out.push(`<polygon points="${pts(a)}" fill="${fill}" stroke="var(--accent)" stroke-width="1.1" stroke-linejoin="round" ${extra}/>`);
  const line = (a, b, w = 0.8, col = "var(--accent)", op = 0.45) => out.push(`<line x1="${P(...a)[0].toFixed(1)}" y1="${P(...a)[1].toFixed(1)}" x2="${P(...b)[0].toFixed(1)}" y2="${P(...b)[1].toFixed(1)}" stroke="${col}" stroke-width="${w}" stroke-opacity="${op}"/>`);
  const prism = (x0, x1, y0, y1, z0, z1, top, side, front) => {
    poly([[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]], top);
    poly([[x1, y0, z0], [x1, y0, z1], [x1, y1, z1], [x1, y1, z0]], side);
    poly([[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]], front);
  };
  const b = c.basin.box, bx = b.size[0] / 2, bz = b.size[2] / 2;
  prism(-bx, bx, 0, 0.6, -bz, bz, "var(--canvas-1)", "var(--canvas-1)", "var(--canvas-1)");
  const hx = sx / 2, hz = sz / 2, inl = c.inlets[0], it = inl.bottomM + inl.heightM;
  prism(-hx, hx, 0.6, st.bottomM, -hz, hz, "var(--canvas-1)", "var(--canvas-1)", "var(--canvas-1)");
  for (let i = 0; i < 8; i++) { const y = inl.bottomM + 0.12 + i * (inl.heightM - 0.2) / 7; line([-hx, y, hz], [hx, y, hz], 0.9, "var(--accent)", 0.6); }
  line([-hx, it, hz], [hx, it, hz], 1.1, "var(--accent)", 1);
  for (let i = 1; i < 4; i++) { const x = -hx + i * sx / 4; line([x, it, hz], [x, st.bottomM, hz], 0.6); line([hx, 0.6, -hz + i * sz / 4], [hx, st.bottomM, -hz + i * sz / 4], 0.6); }
  const ring = (r, y, n = 48) => Array.from({ length: n }, (_, i) => [Math.cos(i / n * 2 * Math.PI) * r, y, Math.sin(i / n * 2 * Math.PI) * r]);
  const r0 = st.throatDiameterM / 2 + 0.25, r1 = st.exitDiameterM / 2, rl = ring(r0, st.bottomM), rt = ring(r1, st.topM);
  const left = rl.reduce((a, p, i) => (P(...p)[0] < P(...rl[a])[0] ? i : a), 0), right = rl.reduce((a, p, i) => (P(...p)[0] > P(...rl[a])[0] ? i : a), 0);
  const front = [];
  for (let i = right; ; i = (i + 1) % rl.length) { front.push(rl[i]); if (i === left) break; }
  const tl = rt.reduce((a, p, i) => (P(...p)[0] < P(...rt[a])[0] ? i : a), 0), tr = rt.reduce((a, p, i) => (P(...p)[0] > P(...rt[a])[0] ? i : a), 0);
  const back = [];
  for (let i = tl; ; i = (i + 1) % rt.length) { back.push(rt[i]); if (i === tr) break; }
  out.push(`<polygon points="${pts([...front, ...back])}" fill="var(--canvas-1)" stroke="var(--accent)" stroke-width="1.1" stroke-linejoin="round"/>`);
  out.push(`<polygon points="${pts(rt)}" fill="var(--canvas-1)" stroke="var(--accent)" stroke-width="1.1"/>`);
  out.push(`<polygon points="${pts(ring(c.fan.diameterM / 2, st.topM - 0.6))}" fill="none" stroke="var(--accent)" stroke-opacity=".5" stroke-width=".7" stroke-dasharray="3 3"/>`);
  for (let i = 0; i < c.fan.bladeCount; i++) { const a = i / c.fan.bladeCount * Math.PI * 2 + 0.2, r = c.fan.diameterM / 2 - 0.15; line([0, st.topM - 0.6, 0], [Math.cos(a) * r, st.topM - 0.6, Math.sin(a) * r], 1.2, "var(--accent)", 0.9); }
  const dim = (a, b, label, off) => {
    line(a, b, 0.8, "var(--fg-quiet)", 0.9);
    const [mx, my] = P((a[0] + b[0]) / 2, (a[1] + b[1]) / 2, (a[2] + b[2]) / 2);
    out.push(`<text x="${(mx + off[0]).toFixed(1)}" y="${(my + off[1]).toFixed(1)}" font-size="12" font-weight="600" fill="var(--fg-quiet)" font-family="Montserrat,sans-serif" text-anchor="middle">${label}</text>`);
  };
  dim([-hx - 0.6, 0, hz + 0.6], [-hx - 0.6, st.topM, hz + 0.6], `${st.topM.toFixed(2)} m`, [-22, 0]);
  dim([-bx, -0.5, bz + 1.0], [bx, -0.5, bz + 1.0], `${b.size[0].toFixed(2)} m`, [-6, 22]);
  dim([bx + 1.0, -0.5, bz], [bx + 1.0, -0.5, -bz], `${b.size[2].toFixed(2)} m`, [16, 20]);
  const all = [...rt, [-hx - 0.6, 0, hz + 0.6], [hx + 1, -0.5, -hz], [-hx, -0.5, hz + 1], [hx + 1, -0.5, hz]].map((p) => P(...p));
  const xs = all.map((p) => p[0]), ys = all.map((p) => p[1]);
  const vb = [Math.min(...xs) - 50, Math.min(...ys) - 20, Math.max(...xs) - Math.min(...xs) + 100, Math.max(...ys) - Math.min(...ys) + 60].map((v) => v.toFixed(0));
  return `<svg class="poster" viewBox="${vb.join(" ")}" role="img" aria-label="Counterflow cooling tower, ${sx} by ${sz} by ${st.topM} metres">${out.join("")}</svg>`;
};
