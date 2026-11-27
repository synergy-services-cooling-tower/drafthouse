// Preview-only port of tower3d.md: the tower as a graphite three.js model with white edges, grey dimension lines and flat GA views. Not product code.
import * as THREE from "three";

const css = (n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim();
const col = (n) => new THREE.Color(css(n));
const L = { face: 1, line: 2, flow: 3, dim3: 4, plan: 5, elev: 6, sect: 7, sectx: 8, halo: 9 };
const hits = (sel, name) => !!sel && !!name && (name === sel || name.startsWith(`${sel}:`));

function rng(seed) {
  let s = seed;
  return () => ((s = (s * 16807) % 2147483647) / 2147483647);
}

function build(g, o) {
  const light = document.documentElement.classList.contains("light"), S = g.structure || "frp";
  const EDGE = col("--edge"), ACC = col("--accent"), BAD = col("--bad"), FACE = col("--model-face"), METAL = col("--model-metal"), CONC = col("--model-concrete");
  const FILLC = { film: col("--model-fill"), splash: col("--model-fill-2"), trickle: col("--model-fill-3") };
  const HOT = col("--flow-hot"), MID = col("--flow-mid"), COLD = col("--flow-cold"), AIR = col("--flow-air"), DIM = col("--dim");
  const scene = new THREE.Scene();
  const plane = new THREE.Plane(new THREE.Vector3(0, 0, -1), 99);
  const clip = [plane];
  const mark = (name) => (hits(o.refuse, name) ? "bad" : hits(o.pick, name) ? "pick" : "");
  const faceMats = new Map(), lineMats = new Map();
  const faceMat = (c, name, opacity = 1) => {
    const m = mark(name), k = `${c.getHexString()}|${m}|${opacity}`;
    if (!faceMats.has(k)) faceMats.set(k, new THREE.MeshStandardMaterial({
      color: c, metalness: 0.15, roughness: 0.75, side: THREE.DoubleSide, clippingPlanes: clip,
      transparent: opacity < 1, opacity, depthWrite: opacity === 1,
      emissive: m === "pick" ? ACC : m === "bad" ? BAD : new THREE.Color(0), emissiveIntensity: m ? (light ? 0.06 : 0.1) : 0,
    }));
    return faceMats.get(k);
  };
  const lineMat = (a, name) => {
    const m = mark(name), quiet = (o.pick || o.refuse) && !m ? 0.7 : 1, k = `${a}|${m}|${quiet}`;
    if (!lineMats.has(k)) lineMats.set(k, new THREE.LineBasicMaterial({
      color: m === "pick" ? ACC : m === "bad" ? BAD : EDGE, transparent: true, clippingPlanes: clip,
      opacity: m ? Math.max(0.75, a) : Math.min(1, a * quiet * (light ? 0.9 : 0.8)),
    }));
    return lineMats.get(k);
  };
  const occ = new THREE.MeshBasicMaterial({ colorWrite: false, side: THREE.DoubleSide, polygonOffset: true, polygonOffsetFactor: 2, polygonOffsetUnits: 2, clippingPlanes: clip });
  const on = (obj, layer) => { obj.layers.set(layer); return obj; };
  let parent = scene;
  const add = (obj, pos = [0, 0, 0], rot) => { obj.position.set(...pos); if (rot) obj.rotation.set(...rot); parent.add(obj); return obj; };
  const halo = (geoOrLines, name) => { if (mark(name)) parent.add(on(geoOrLines, L.halo)); };

  const part = (geo, pos, rot, { name, face = FACE, opacity = 1, a = 1, thr = 20, occlude = true } = {}) => {
    const grp = new THREE.Group(), edges = new THREE.EdgesGeometry(geo, thr);
    grp.add(on(new THREE.Mesh(geo, faceMat(face, name, opacity)), L.face));
    if (occlude) grp.add(on(new THREE.Mesh(geo, occ), L.line));
    grp.add(on(new THREE.LineSegments(edges, lineMat(a, name)), L.line));
    if (mark(name)) grp.add(on(new THREE.LineSegments(edges, lineMat(1, name)), L.halo));
    return add(grp, pos, rot);
  };
  const segs = (pts, mat, layer) => {
    const geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.Float32BufferAttribute(pts, 3));
    return add(on(new THREE.LineSegments(geo, mat), layer));
  };
  const lines = (pts, a, name) => {
    const m = segs(pts, lineMat(a, name), L.line);
    halo(new THREE.LineSegments(m.geometry, lineMat(1, name)), name);
    return m;
  };
  const ring = (r, y, n = 64, out = []) => {
    for (let i = 0; i < n; i++) {
      const a0 = (i / n) * Math.PI * 2, a1 = ((i + 1) / n) * Math.PI * 2;
      out.push(Math.cos(a0) * r, y, Math.sin(a0) * r, Math.cos(a1) * r, y, Math.sin(a1) * r);
    }
    return out;
  };
  const rand = rng(7), keepZ = o.flowZ ?? 99, flowMat = (a) => new THREE.LineBasicMaterial({ vertexColors: true, transparent: true, opacity: a, depthWrite: false, clippingPlanes: o.style === "line" ? clip : [] });
  const flow = (pts, cols, a) => {
    const geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.Float32BufferAttribute(pts, 3));
    geo.setAttribute("color", new THREE.Float32BufferAttribute(cols, 3));
    add(on(new THREE.LineSegments(geo, flowMat(a)), L.flow));
  };
  const anchors = {};

  for (const [ci, cell] of g.cells.entries()) {
    parent = new THREE.Group();
    parent.position.set(...cell.origin);
    scene.add(parent);
    const ox = cell.origin[0], last = ci === g.cells.length - 1;
    const bs = cell.basin.box, wd = cell.basin.waterDepthM;
    part(new THREE.BoxGeometry(...bs.size), bs.centre, null, { name: "basin", face: METAL, a: 0.8 });
    const water = new THREE.Mesh(new THREE.PlaneGeometry(bs.size[0] - 0.3, bs.size[2] - 0.3), new THREE.MeshBasicMaterial({ color: COLD, transparent: true, opacity: light ? 0.25 : 0.2, side: THREE.DoubleSide, clippingPlanes: clip, depthWrite: false }));
    add(on(water, L.face), [0, bs.centre[1] + 0.31, 0], [-Math.PI / 2, 0, 0]);

    const sh = cell.shell.box, [sx, sy, sz] = sh.size, bottom = sh.centre[1] - sy / 2, top = sh.centre[1] + sy / 2;
    const wall = S === "concrete" ? 0.22 : cell.shell.wallM, inletTop = cell.inlets[0].bottomM + cell.inlets[0].heightM;
    const skin = S === "concrete" ? CONC : FACE;
    for (const s of [-1, 1]) {
      if (s === -1 || last) part(new THREE.BoxGeometry(wall, sy, sz), [s * sx / 2, sh.centre[1], 0], null, { name: "casing", face: skin });
      part(new THREE.BoxGeometry(sx, top - inletTop, wall), [0, (top + inletTop) / 2, s * sz / 2], null, { name: "casing", face: skin });
    }
    const skinLines = [], e = wall / 2 + 0.01;
    const pitch = S === "steel" ? 0.4 : S === "frp" ? sx / 4 : 0;
    if (pitch) for (let v = -sx / 2 + pitch; v < sx / 2 - 0.05; v += pitch) for (const s of [-1, 1]) {
      skinLines.push(v, inletTop, s * (sz / 2 + e), v, top, s * (sz / 2 + e));
      if (last) skinLines.push(sx / 2 + e, bottom, v, sx / 2 + e, top, v);
    }
    if (S === "concrete") for (const yy of [inletTop + (top - inletTop) / 2]) for (const s of [-1, 1]) skinLines.push(-sx / 2, yy, s * (sz / 2 + e), sx / 2, yy, s * (sz / 2 + e));
    lines(skinLines, S === "steel" ? 0.16 : 0.3, "casing");

    const colW = S === "concrete" ? 0.4 : S === "steel" ? 0.14 : 0.18, posts = S === "steel" ? [-1, -0.5, 0, 0.5, 1] : [-1, 0, 1];
    for (const x of posts) for (const z of [-1, 1]) part(new THREE.BoxGeometry(colW, inletTop - bottom, colW), [x * (sx / 2 - colW / 2), (inletTop + bottom) / 2, z * (sz / 2 - colW / 2)], null, { name: "frame", face: S === "concrete" ? CONC : METAL });
    if (S === "concrete") for (const z of [-1, 1]) part(new THREE.BoxGeometry(sx, 0.35, 0.3), [0, inletTop - 0.175, z * (sz / 2 - 0.15)], null, { name: "frame", face: CONC });
    if (S === "steel") {
      const br = [];
      for (let i = 0; i < posts.length - 1; i++) for (const z of [-1, 1]) {
        const x0 = posts[i] * (sx / 2 - 0.07), x1 = posts[i + 1] * (sx / 2 - 0.07), zz = z * (sz / 2 - 0.07);
        br.push(x0, bottom, zz, x1, inletTop, zz, x1, bottom, zz, x0, inletTop, zz);
      }
      lines(br, 0.9, "frame");
    }

    const louver = [];
    for (const inl of cell.inlets) {
      const s = inl.face === "+z" ? 1 : -1;
      for (let i = 0; i < 7; i++) {
        const y = inl.bottomM + 0.15 + (i * (inl.heightM - 0.2)) / 6, z = s * (sz / 2 - 0.05);
        louver.push(-sx / 2, y, z, sx / 2, y, z, -sx / 2, y, z, -sx / 2, y - 0.16, z - s * 0.18);
      }
    }
    lines(louver, 0.4, "inlet");

    for (const f of cell.fill) {
      const [fx, fy, fz] = f.box.size, cy = f.box.centre[1], kind = (FILLS[f.recordId] || { kind: "film" }).kind, name = `fill:${f.layer}`;
      part(new THREE.BoxGeometry(fx, fy, fz), f.box.centre, null, { name, face: FILLC[kind], opacity: o.style === "line" ? 1 : 0.55, occlude: false, a: 0.9 });
      const pat = [], y0 = cy - fy / 2, y1 = cy + fy / 2, n = Math.round((fx / 0.25) * (o.tier > 1 ? 1 : 0.5));
      if (kind === "film") for (let i = 1; i < n; i++) {
        const x = -fx / 2 + (i * fx) / n;
        for (let k = 0; k < 6; k++) { const z0 = -fz / 2 + (k * fz) / 6, z1 = z0 + fz / 6; pat.push(x, y0, z0, x, y1, (z0 + z1) / 2, x, y1, (z0 + z1) / 2, x, y0, z1); }
      }
      if (kind === "splash") for (let yy = y0 + 0.075; yy < y1; yy += 0.15) for (let i = 1; i < n; i += 2) {
        const x = -fx / 2 + ((i + ((yy * 20) % 2 < 1 ? 0 : 1)) * fx) / n;
        pat.push(x, yy, -fz / 2, x, yy, fz / 2);
      }
      if (kind === "trickle") for (const yy of [y0 + fy * 0.25, y0 + fy * 0.75]) for (let i = 1; i < n; i += 2) {
        const v = -fx / 2 + (i * fx) / n;
        pat.push(v, yy, -fz / 2, v, yy, fz / 2, -fx / 2, yy, v, fx / 2, yy, v);
      }
      lines(pat, 0.16, name);
      if (ci === 0) anchors[name] = [ox + fx / 2, cy, fz / 2];
      if (ci === 0 && f.layer === 0) anchors.fillFront = [ox, cy, fz / 2];
    }

    const nz = cell.nozzles, nozzlePts = [];
    for (let r = 0; r < nz.rows; r++) {
      const z = -sz / 2 + ((r + 0.5) * sz) / nz.rows;
      part(new THREE.CylinderGeometry(nz.headerDiameterM / 2, nz.headerDiameterM / 2, sx - 0.2, 12), [0, nz.headerYM, z], [0, 0, Math.PI / 2], { name: "nozzles", face: METAL, a: 0.6, thr: 40 });
      for (let c = 0; c < nz.columns; c++) {
        const x = -((nz.columns - 1) * nz.pitchM) / 2 + c * nz.pitchM;
        part(new THREE.ConeGeometry(0.07, 0.14, 6), [x, nz.headerYM - 0.16, z], null, { name: "nozzles", face: METAL, a: 0.5, thr: 40, occlude: false });
        nozzlePts.push([x, nz.headerYM - 0.25, z]);
      }
    }
    if (ci === 0) part(new THREE.CylinderGeometry(0.2, 0.2, sz + 1.2, 16), [-sx / 2 - 0.45, nz.headerYM, 0], [Math.PI / 2, 0, 0], { name: "nozzles", face: METAL, thr: 40 });

    const dy = cell.drift.panels[0].centre[1], blades = [];
    for (let i = 0; i < 40; i++) {
      const x = -sx / 2 + (i * sx) / 40;
      for (const z of [-sz / 2 + 0.05, sz / 2 - 0.05]) blades.push(x, dy - 0.12, z, x + sx / 80, dy, z, x + sx / 80, dy, z, x, dy + 0.12, z);
    }
    lines(blades, 0.3, "drift");
    part(new THREE.BoxGeometry(sx, 0.24, sz), [0, dy, 0], null, { name: "drift", face: FILLC.film, opacity: o.style === "line" ? 1 : 0.6, occlude: false, a: 0.7 });
    if (ci === 0) anchors.drift = [ox + sx / 2, dy, sz / 2];

    const fan = cell.fan, st = fan.stack, deckY = st.bottomM + 0.16;
    const deck = new THREE.Shape([[-sx / 2, -sz / 2], [sx / 2, -sz / 2], [sx / 2, sz / 2], [-sx / 2, sz / 2]].map(([a, b]) => new THREE.Vector2(a, b)));
    const hole = new THREE.Path();
    hole.absarc(0, 0, st.throatDiameterM / 2, 0, Math.PI * 2, false);
    deck.holes.push(hole);
    part(new THREE.ExtrudeGeometry(deck, { depth: 0.16, bevelEnabled: false, curveSegments: 64 }), [0, deckY, 0], [Math.PI / 2, 0, 0], { name: "casing", face: skin });
    const k = st.topM - st.bottomM;
    const prof = [[st.throatDiameterM / 2 + 0.25, 0], [st.throatDiameterM / 2, k * 0.11], [fan.diameterM / 2 + 0.06, k * 0.25], [fan.diameterM / 2 + 0.08, k * 0.45], [st.exitDiameterM / 2 - 0.12, k * 0.8], [st.exitDiameterM / 2, k]];
    const lathe = new THREE.LatheGeometry(prof.map(([r, y]) => new THREE.Vector2(r, y)), 72);
    add(on(new THREE.Mesh(lathe, faceMat(skin, "stack")), L.face), [0, deckY, 0]);
    add(on(new THREE.Mesh(lathe, occ), L.line), [0, deckY, 0]);
    const stack = [];
    for (const [r, y] of prof) ring(r, deckY + y, 72, stack);
    for (let i = 0; i < 24; i++) {
      const a = (i / 24) * Math.PI * 2;
      for (let j = 0; j < prof.length - 1; j++) stack.push(Math.cos(a) * prof[j][0], deckY + prof[j][1], Math.sin(a) * prof[j][0], Math.cos(a) * prof[j + 1][0], deckY + prof[j + 1][1], Math.sin(a) * prof[j + 1][0]);
    }
    lines(stack, 0.45, "stack");

    const rotor = new THREE.Group();
    rotor.position.set(...fan.centre);
    rotor.rotation.y = 0.2 + ci * 0.5;
    const hubGeo = new THREE.CylinderGeometry(fan.hubDiameterM / 2, fan.hubDiameterM / 2, 0.35, 24);
    const piece = (geo, grp) => {
      const ed = new THREE.EdgesGeometry(geo, 20);
      grp.add(on(new THREE.Mesh(geo, faceMat(METAL, "fan")), L.face), on(new THREE.Mesh(geo, occ), L.line), on(new THREE.LineSegments(ed, lineMat(1, "fan")), L.line));
      if (mark("fan")) grp.add(on(new THREE.LineSegments(ed, lineMat(1, "fan")), L.halo));
    };
    piece(hubGeo, rotor);
    const reach = fan.diameterM / 2 - fan.hubDiameterM / 2 - 0.05;
    for (let i = 0; i < fan.bladeCount; i++) {
      const arm = new THREE.Group(), blade = new THREE.Group();
      arm.rotation.y = (i / fan.bladeCount) * Math.PI * 2;
      piece(new THREE.BoxGeometry(reach, 0.04, 0.42 * fan.diameterM / 5), blade);
      blade.position.x = fan.hubDiameterM / 2 + reach / 2;
      blade.rotation.x = 0.32;
      arm.add(blade);
      rotor.add(arm);
    }
    parent.add(rotor);
    lines(ring(fan.diameterM / 2 + 0.07, fan.centre[1], 72), 0.45, "fan");
    if (ci === 0) anchors.fan = [ox, st.topM + 0.1, st.exitDiameterM / 2];
    const gb = cell.drive.gearbox;
    part(new THREE.BoxGeometry(...gb.size), gb.centre, null, { name: "drive", face: METAL });
    lines([0, gb.centre[1], 0, 0, fan.centre[1], 0, 0, gb.centre[1], 0, sx / 2 + 0.15, gb.centre[1], 0], 0.8, "drive");
    part(new THREE.CylinderGeometry(0.32, 0.32, 0.8, 20), [sx / 2 + 0.55, gb.centre[1], 0], [0, 0, Math.PI / 2], { name: "drive", face: METAL });
    if (ci === 0) anchors.casing = [ox - sx / 2, (inletTop + top) / 2, sz / 2];
    if (ci === 0) anchors.frame = [ox - sx / 2 + colW / 2, (bottom + inletTop) / 2, sz / 2];

    const share = (sx / 8) ** 2, n = (o.tier > 1 ? 1 : 0.5) * share;
    if (o.water !== false) {
      const pts = [], cols = [], fillTop = Math.max(...cell.fill.map((f) => f.box.centre[1] + f.box.size[1] / 2)), surf = wd + 0.12;
      const tint = (y) => { const u = Math.max(0, Math.min(1, (y - surf) / (nz.headerYM - surf))); return u < 0.5 ? COLD.clone().lerp(MID, u * 2) : MID.clone().lerp(HOT, u * 2 - 1); };
      for (const p of nozzlePts) for (let i = 0; i < 12 * (o.tier > 1 ? 1 : 0.5); i++) {
        const y = p[1] - rand() * (p[1] - surf - 0.1), above = y > fillTop;
        const spread = above ? (p[1] - y) * 0.55 : 0.3, len = above ? 0.09 : 0.16;
        const x = p[0] + (rand() - 0.5) * spread * 2, z = p[2] + (rand() - 0.5) * spread * 2;
        if (z > keepZ) continue;
        pts.push(x, y, z, x, y - len, z);
        cols.push(...tint(y).toArray(), ...tint(y - len).toArray());
      }
      flow(pts, cols, 0.9);
    }
    if (o.air !== false) {
      const pts = [], len = 0.34;
      for (let i = 0; i < 280 * n; i++) {
        const s = rand() < 0.5 ? -1 : 1, t = rand(), x = (rand() - 0.5) * (sx - 0.4), y0 = 0.8 + rand() * (inletTop - 0.9);
        let p, d;
        if (t < 0.25) { p = [x, y0, s * (sz / 2 + 1.1 - (t / 0.25) * 1.1)]; d = [0, 0, -s]; }
        else if (t < 0.45) { const u = (t - 0.25) / 0.2; p = [x, y0 + u * (inletTop - y0 + 0.3), s * (sz / 2) * (1 - u) * (0.4 + 0.6 * rand())]; d = [0, 0.7, -s * 0.7]; }
        else if (t < 0.8) { const u = (t - 0.45) / 0.35; p = [x * (1 - u * 0.35), inletTop + 0.3 + u * (st.bottomM - inletTop - 0.3), (rand() - 0.5) * (sz - 0.6) * (1 - u * 0.35)]; d = [-x * 0.05, 1, 0]; }
        else { const u = (t - 0.8) / 0.2, a = rand() * Math.PI * 2, r = Math.sqrt(rand()) * (fan.diameterM / 2 - 0.2) * (1 + u * 0.25); p = [Math.cos(a) * r, st.bottomM + u * (k + 2.0), Math.sin(a) * r]; d = [-Math.sin(a) * 0.35, 1, Math.cos(a) * 0.35]; }
        if (p[2] > keepZ && t >= 0.45) continue;
        const m = Math.hypot(...d);
        pts.push(...p, p[0] + (d[0] / m) * len, p[1] + (d[1] / m) * len, p[2] + (d[2] / m) * len);
      }
      flow(pts, Array.from({ length: pts.length / 3 }, () => AIR.toArray()).flat(), light ? 0.6 : 0.5);
    }
  }
  parent = scene;

  const dimMat = new THREE.LineBasicMaterial({ color: DIM, transparent: true, opacity: 0.95, depthTest: false });
  const labels = [];
  const dim = (layer, p0, p1, off, text, { tag = "", chain = false, side = false } = {}) => {
    const v0 = new THREE.Vector3(...p0), v1 = new THREE.Vector3(...p1), ov = new THREE.Vector3(...off);
    const q0 = v0.clone().add(ov), q1 = v1.clone().add(ov), on1 = ov.lengthSq() ? ov.clone().normalize() : new THREE.Vector3(0, 0, 1), along = q1.clone().sub(q0).normalize();
    const tick = along.clone().add(on1).normalize().multiplyScalar(0.16);
    const pts = [...q0.toArray(), ...q1.toArray()];
    if (!chain) for (const [v, q] of [[v0, q0], [v1, q1]]) pts.push(...v.clone().addScaledVector(on1, 0.15).toArray(), ...q.clone().addScaledVector(on1, 0.2).toArray());
    else for (const q of [q0, q1]) pts.push(...q.clone().addScaledVector(on1, -0.15).toArray(), ...q.clone().addScaledVector(on1, 0.15).toArray());
    for (const q of [q0, q1]) pts.push(...q.clone().sub(tick).toArray(), ...q.clone().add(tick).toArray());
    segs(pts, dimMat, layer).renderOrder = 10;
    labels.push({ layer, at: q0.clone().add(q1).multiplyScalar(0.5), text, tag, side });
  };
  const note = (layer, at, text) => labels.push({ layer, at: new THREE.Vector3(...at), text, tag: "", note: true });

  const c0 = g.cells[0], cn = g.cells.at(-1), mid = g.cells[(g.cells.length - 1) >> 1], many = g.cells.length > 1;
  const bs = c0.basin.box, B = bs.size[0], bz = bs.size[2] / 2, x0 = c0.origin[0] - B / 2, x1 = cn.origin[0] + B / 2, len = x1 - x0;
  const fan = c0.fan, st = fan.stack, H = st.topM, inl = c0.inlets[0], inletTop = inl.bottomM + inl.heightM, fx = cn.origin[0];
  const m2 = (v) => v.toFixed(2);
  dim(L.dim3, [x0, 0, bz], [x0, H, bz], [-1.1, 0, 0], `${m2(H)} m`);
  dim(L.dim3, [x0, 0, bz], [x1, 0, bz], [0, 0, 1.3], `${m2(len)} m`, { tag: many ? `${g.cells.length} cells` : "" });
  dim(L.dim3, [x1, 0, bz], [x1, 0, -bz], [1.3, 0, 0], `${m2(B)} m`);
  dim(L.dim3, [fx - fan.diameterM / 2, H, 0], [fx + fan.diameterM / 2, H, 0], [0, 0.9, 0], `Ø ${m2(fan.diameterM)} m`);
  dim(L.dim3, [x1 - 0.3, inl.bottomM, bz], [x1 - 0.3, inletTop, bz], [0.9, 0, 0], `${m2(inl.heightM)} m`, { tag: "inlet" });
  if (o.fillDim) {
    const fb = Math.min(...c0.fill.map((f) => f.box.centre[1] - f.box.size[1] / 2)), ft = Math.max(...c0.fill.map((f) => f.box.centre[1] + f.box.size[1] / 2));
    dim(L.dim3, [x1 - 0.3, fb, bz], [x1 - 0.3, ft, bz], [0.9, 0, 0], `${m2(ft - fb)} m`, { tag: "fill" });
  }

  dim(L.plan, [x0, H, bz], [x1, H, bz], [0, 0, 1.4], m2(len));
  dim(L.plan, [x1, H, bz], [x1, H, -bz], [1.4, 0, 0], m2(B));
  if (many) for (const c of g.cells) dim(L.plan, [c.origin[0] - B / 2, H, -bz], [c.origin[0] + B / 2, H, -bz], [0, 0, -0.9], m2(B), { chain: true });
  dim(L.plan, [c0.origin[0] - fan.diameterM / 2, H + 0.2, 0], [c0.origin[0] + fan.diameterM / 2, H + 0.2, 0], [0, 0, 0], `Ø ${m2(fan.diameterM)}`, { chain: true });
  if (!many) dim(L.plan, [x0 - 0.65, H, -bz - 0.6], [x0 - 0.65, H, bz + 0.6], [-0.9, 0, 0], `header ${m2(c0.shell.box.size[2] + 1.2)}`);
  note(L.plan, [x0 + 2.2, H, bz + 1.4], "air in ↑");
  note(L.plan, [x0 + (many ? 2.2 : len / 2), H, -bz - (many ? 1.9 : 1.0)], "air in ↓");

  const ez = bz + 0.4;
  dim(L.elev, [x0, 0, ez], [x0, H, ez], [-1.3, 0, 0], m2(H));
  if (many) {
    for (const c of g.cells) dim(L.elev, [c.origin[0] - B / 2, 0, ez], [c.origin[0] + B / 2, 0, ez], [0, -0.9, 0], m2(B), { chain: true });
    dim(L.elev, [x0, 0, ez], [x1, 0, ez], [0, -2.0, 0], `${m2(len)} <small>row</small>`);
  } else dim(L.elev, [x0, 0, ez], [x1, 0, ez], [0, -1.0, 0], m2(len));
  dim(L.elev, [c0.origin[0] - fan.diameterM / 2, H, ez], [c0.origin[0] + fan.diameterM / 2, H, ez], [0, 0.9, 0], `Ø ${m2(fan.diameterM)}`);
  dim(L.elev, [x1, inl.bottomM, ez], [x1, inletTop, ez], [1.0, 0, 0], m2(inl.heightM), { tag: "inlet" });
  dim(L.elev, [x1, st.bottomM, ez], [x1, H, ez], [1.0, 0, 0], m2(H - st.bottomM), { tag: "stack" });

  const fillBot = Math.min(...c0.fill.map((f) => f.box.centre[1] - f.box.size[1] / 2)), fillTop = Math.max(...c0.fill.map((f) => f.box.centre[1] + f.box.size[1] / 2));
  const deTop = c0.drift.panels[0].centre[1] + 0.12, wd = c0.basin.waterDepthM, nz = c0.nozzles;
  const chain = [[0, bs.size[1], "basin"], [bs.size[1], inletTop, "inlet"], [fillBot, fillTop, "fill"], [fillTop, deTop, "spray"], [deTop, st.bottomM, "plenum"], [st.bottomM, H, "stack"]];
  const fillNote = c0.fill.every((f) => f.recordId === c0.fill[0].recordId) ? `${c0.fill[0].recordId} · ${c0.fill.length} × ${m2(c0.fill[0].box.size[1])}` : c0.fill.map((f) => `${f.recordId} ${m2(f.box.size[1])}`).join(" / ");
  const mx = mid.origin[0];
  for (const [a, b, t] of chain) dim(L.sect, [x1, a, 0.2], [x1, b, 0.2], [1.1, 0, 0], `${m2(b - a)} <small>${t}</small>`, { chain: true, side: true });
  note(L.sect, [mx - 1.9 * B / 8.6, (fillBot + fillTop) / 2, 0.2], fillNote);
  note(L.sect, [mx - 1.6 * B / 8.6, nz.headerYM + 0.05, 0.2], `${nz.count} × ${nz.recordId}`);
  note(L.sect, [mx + 1.6 * B / 8.6, deTop + 0.05, 0.2], c0.drift.recordId);
  note(L.sect, [mx, fan.centre[1] + 0.45, 0.2], `${fan.recordId} · ${fan.bladeCount} blades`);
  note(L.sect, [mx, wd + 0.35, 0.2], `water ${m2(wd)}`);

  const cx = c0.origin[0] - 0.2;
  for (const [a, b, t] of chain) dim(L.sectx, [cx, a, -bz], [cx, b, -bz], [0, 0, -1.1], `${m2(b - a)} <small>${t}</small>`, { chain: true, side: true });
  dim(L.sectx, [cx, 0, bz], [cx, 0, -bz], [0, -0.9, 0], m2(B));
  note(L.sectx, [cx, (fillBot + fillTop) / 2, 0], fillNote);
  note(L.sectx, [cx, fan.centre[1] + 0.45, 0], `${fan.recordId} · Ø ${m2(fan.diameterM)}`);
  note(L.sectx, [cx, inl.bottomM + 0.3, bz + 1.5], "air in →");

  scene.add(new THREE.HemisphereLight(0xffffff, light ? 0xc4cbc8 : 0x101413, light ? 1.5 : 1.0));
  const key = new THREE.DirectionalLight(0xffffff, light ? 1.4 : 1.3);
  key.position.set(6, 12, 9);
  const rim = new THREE.DirectionalLight(0xdfe7ff, light ? 0.3 : 0.5);
  rim.position.set(-9, 5, -7);
  scene.add(key, rim);
  for (const obj of scene.children.filter((c) => c.isLight)) obj.layers.enableAll();
  return { scene, plane, labels, anchors, cutZ: (1 - o.cut) * (bz + 0.1), span: [x0, x1, H, bz] };
}

const renderer = (w, h, el, cls) => {
  const r = new THREE.WebGLRenderer({ antialias: true, alpha: true, preserveDrawingBuffer: true });
  r.setPixelRatio(1);
  r.setSize(w, h);
  r.localClippingEnabled = true;
  r.domElement.className = cls;
  if (el) el.append(r.domElement);
  return r;
};

const label = (el, l, x, y) => {
  const d = document.createElement("div");
  d.className = l.note ? "dl note" : l.side ? "dl side" : "dl";
  d.style.left = `${x}px`;
  d.style.top = `${y}px`;
  d.innerHTML = l.tag ? `<small>${l.tag}</small>${l.text}` : l.text;
  if (l.text || l.tag) el.append(d);
};

export function mountModel(el, g, o = {}) {
  o = { cut: 0.55, tier: 2, pick: null, free: null, dir: [1, 0.62, 1.3], span: 15, target: [0, 4.0, 0], ...o };
  const w = el.clientWidth, h = el.clientHeight, light = document.documentElement.classList.contains("light");
  const m = build(g, { ...o, style: "solid", flowZ: (1 - o.cut) * 4.4 + 0.3 });
  m.plane.constant = m.cutZ;
  const cam = new THREE.PerspectiveCamera(26, w / h, 0.1, 400);
  const aim = new THREE.Vector3(...o.target);
  cam.position.copy(aim).add(new THREE.Vector3(...o.dir).normalize().multiplyScalar(o.span / (2 * Math.tan((13 * Math.PI) / 180))));
  cam.lookAt(aim);
  const f = o.free || [0, 0, w, h];
  cam.setViewOffset(w, h, w / 2 - (f[0] + f[2] / 2), h / 2 - (f[1] + f[3] / 2), w, h);
  const faces = renderer(w, h, el, "cv faces");
  cam.layers.set(L.face);
  faces.render(m.scene, cam);
  const glow = document.createElement("canvas");
  glow.width = w; glow.height = h; glow.className = "cv glow";
  el.append(glow);
  const edges = renderer(w, h, el, "cv edges");
  cam.layers.set(L.line);
  cam.layers.enable(L.flow);
  if (o.dims !== false) cam.layers.enable(L.dim3);
  edges.render(m.scene, cam);
  if (o.pick || o.refuse) {
    const off = renderer(w, h, null, "");
    cam.layers.set(L.halo);
    off.render(m.scene, cam);
    const ctx = glow.getContext("2d");
    ctx.filter = "blur(7px)";
    ctx.drawImage(off.domElement, 0, 0);
    ctx.filter = "blur(3px)";
    ctx.drawImage(off.domElement, 0, 0);
    glow.style.opacity = light ? 0.35 : 0.5;
  }
  const at = (v) => { const p = v.clone().project(cam); return [((p.x + 1) / 2) * w, ((1 - p.y) / 2) * h]; };
  if (o.dims !== false) for (const l of m.labels.filter((l) => l.layer === L.dim3)) label(el, l, ...at(l.at));
  return { at: (p) => at(new THREE.Vector3(...p)), anchor: (k) => at(new THREE.Vector3(...m.anchors[k])) };
}

export function mountGA(el, g, o = {}) {
  o = { tier: 2, pick: null, views: [], scale: 30, ...o };
  const w = el.clientWidth, h = el.clientHeight;
  const m = build(g, { ...o, cut: 0, style: "line", flowZ: o.views.some((v) => v.kind === "sectx") ? 99 : 0.1 });
  const r = renderer(w, h, el, "cv edges");
  r.setScissorTest(true);
  const [x0, x1, H] = m.span, cx = (x0 + x1) / 2, c0 = g.cells[0].origin[0];
  const kinds = {
    plan: { layer: L.plan, eye: [0, 50, 0], up: [0, 0, -1], n: [0, 0, -1], cut: 99, centre: [cx, 0, 0.5] },
    elev: { layer: L.elev, eye: [0, 0, 50], up: [0, 1, 0], n: [0, 0, -1], cut: 99, centre: [cx - 0.1, H / 2 - (g.cells.length > 1 ? 0.6 : 0.1), 0] },
    sect: { layer: L.sect, eye: [0, 0, 50], up: [0, 1, 0], n: [0, 0, -1], cut: 0, centre: [cx + 1.7, H / 2 - 0.1, 0], flows: true },
    sectx: { layer: L.sectx, eye: [50, 0, 0], up: [0, 1, 0], n: [-1, 0, 0], cut: c0, centre: [c0, H / 2 - 0.4, -0.9], flows: true },
  };
  for (const v of o.views) {
    const k = kinds[v.kind], [vx, vy, vw, vh] = v.rect, s = v.scale || o.scale;
    const cam = new THREE.OrthographicCamera(-vw / 2 / s, vw / 2 / s, vh / 2 / s, -vh / 2 / s, 0.1, 200);
    const c = new THREE.Vector3(...k.centre);
    cam.up.set(...k.up);
    cam.position.copy(c).add(new THREE.Vector3(...k.eye));
    cam.lookAt(c);
    cam.layers.set(L.line);
    cam.layers.enable(k.layer);
    if (k.flows) cam.layers.enable(L.flow);
    m.plane.normal.set(...k.n);
    m.plane.constant = k.cut;
    r.setViewport(vx, h - vy - vh, vw, vh);
    r.setScissor(vx, h - vy - vh, vw, vh);
    r.render(m.scene, cam);
    for (const l of m.labels.filter((l) => l.layer === k.layer)) {
      const p = l.at.clone().project(cam);
      label(el, l, vx + ((p.x + 1) / 2) * vw, vy + ((1 - p.y) / 2) * vh);
    }
  }
}
