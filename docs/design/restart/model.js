// Preview-only port of tower3d.md: the tower as a shaded three.js model with accent edges, dimension lines and flat GA views. Not product code.
import * as THREE from "three";

const css = (n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim();
const col = (n) => new THREE.Color(css(n));
const L = { face: 1, line: 2, flow: 3, dim3: 4, plan: 5, elev: 6, sect: 7 };

function rng(seed) {
  let s = seed;
  return () => ((s = (s * 16807) % 2147483647) / 2147483647);
}

function build(g, o) {
  const cell = g.cells[0], light = document.documentElement.classList.contains("light");
  const ACC = col("--accent"), HI = col("--accent-hi"), FACE = col("--model-face"), FILL = col("--model-fill"), METAL = col("--model-metal");
  const HOT = col("--flow-hot"), MID = col("--flow-mid"), COLD = col("--flow-cold"), AIR = col("--flow-air"), DIM = col("--dim");
  const scene = new THREE.Scene();
  const plane = new THREE.Plane(new THREE.Vector3(0, 0, -1), 99);
  const clip = [plane];
  const faceMats = new Map(), lineMats = new Map();
  const faceMat = (c, name, opacity = 1) => {
    const picked = o.pick && name === o.pick, k = `${c.getHexString()}|${picked}|${opacity}`;
    if (!faceMats.has(k)) faceMats.set(k, new THREE.MeshStandardMaterial({
      color: c, metalness: 0.25, roughness: 0.6, side: THREE.DoubleSide, clippingPlanes: clip,
      transparent: opacity < 1, opacity, depthWrite: opacity === 1,
      emissive: picked ? ACC : new THREE.Color(0), emissiveIntensity: picked ? (light ? 0.18 : 0.32) : 0,
    }));
    return faceMats.get(k);
  };
  const lineMat = (a, name) => {
    const picked = o.pick && name === o.pick, dim = o.pick && !picked ? 0.6 : 1, k = `${a}|${picked}|${dim}`;
    if (!lineMats.has(k)) lineMats.set(k, new THREE.LineBasicMaterial({ color: picked ? HI : ACC, transparent: true, opacity: Math.min(1, a * dim), clippingPlanes: clip }));
    return lineMats.get(k);
  };
  const occ = new THREE.MeshBasicMaterial({ colorWrite: false, side: THREE.DoubleSide, polygonOffset: true, polygonOffsetFactor: 2, polygonOffsetUnits: 2, clippingPlanes: clip });
  const on = (obj, layer) => { obj.layers.set(layer); return obj; };
  const add = (obj, pos = [0, 0, 0], rot) => { obj.position.set(...pos); if (rot) obj.rotation.set(...rot); scene.add(obj); return obj; };

  const part = (geo, pos, rot, { name, face = FACE, opacity = 1, a = 1, thr = 20, occlude = true } = {}) => {
    const grp = new THREE.Group();
    grp.add(on(new THREE.Mesh(geo, faceMat(face, name, opacity)), L.face));
    if (occlude) grp.add(on(new THREE.Mesh(geo, occ), L.line));
    grp.add(on(new THREE.LineSegments(new THREE.EdgesGeometry(geo, thr), lineMat(a, name)), L.line));
    return add(grp, pos, rot);
  };
  const segs = (pts, mat, layer) => {
    const geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.Float32BufferAttribute(pts, 3));
    return add(on(new THREE.LineSegments(geo, mat), layer));
  };
  const lines = (pts, a, name) => segs(pts, lineMat(a, name), L.line);
  const ring = (r, y, n = 64, out = []) => {
    for (let i = 0; i < n; i++) {
      const a0 = (i / n) * Math.PI * 2, a1 = ((i + 1) / n) * Math.PI * 2;
      out.push(Math.cos(a0) * r, y, Math.sin(a0) * r, Math.cos(a1) * r, y, Math.sin(a1) * r);
    }
    return out;
  };

  const bs = cell.basin.box, wd = cell.basin.waterDepthM;
  part(new THREE.BoxGeometry(...bs.size), bs.centre, null, { name: "basin", face: METAL });
  const water = new THREE.Mesh(new THREE.PlaneGeometry(bs.size[0] - 0.3, bs.size[2] - 0.3), new THREE.MeshBasicMaterial({ color: COLD, transparent: true, opacity: light ? 0.28 : 0.22, side: THREE.DoubleSide, clippingPlanes: clip, depthWrite: false }));
  add(on(water, L.face), [0, bs.centre[1] + 0.31, 0], [-Math.PI / 2, 0, 0]);

  const sh = cell.shell.box, [sx, sy, sz] = sh.size, bottom = sh.centre[1] - sy / 2, top = sh.centre[1] + sy / 2;
  const wall = cell.shell.wallM, inletTop = cell.inlets[0].bottomM + cell.inlets[0].heightM;
  for (const s of [-1, 1]) {
    part(new THREE.BoxGeometry(wall, sy, sz), [s * sx / 2, sh.centre[1], 0], null, { name: "casing" });
    part(new THREE.BoxGeometry(sx, top - inletTop, wall), [0, (top + inletTop) / 2, s * sz / 2], null, { name: "casing" });
  }
  const seams = [];
  for (let i = 1; i < 4; i++) {
    const v = -sx / 2 + (i * sx) / 4, e = wall / 2 + 0.01;
    for (const s of [-1, 1]) {
      seams.push(v, inletTop, s * (sz / 2 + e), v, top, s * (sz / 2 + e));
      seams.push(s * (sx / 2 + e), bottom, v, s * (sx / 2 + e), top, v);
    }
  }
  lines(seams, 0.35, "casing");
  for (const x of [-1, 0, 1]) for (const z of [-1, 1]) part(new THREE.BoxGeometry(0.18, inletTop - bottom, 0.18), [x * (sx / 2 - 0.09), (inletTop + bottom) / 2, z * (sz / 2 - 0.09)], null, { name: "casing", face: METAL });

  const louver = [];
  for (const inl of cell.inlets) {
    const s = inl.face === "+z" ? 1 : -1;
    for (let i = 0; i < 7; i++) {
      const y = inl.bottomM + 0.15 + (i * (inl.heightM - 0.2)) / 6, z = s * (sz / 2 - 0.05);
      louver.push(-sx / 2, y, z, sx / 2, y, z, -sx / 2, y, z, -sx / 2, y - 0.16, z - s * 0.18);
    }
  }
  lines(louver, 0.5, "inlet");

  const sheetsPer = o.tier > 1 ? 32 : 16;
  for (const f of cell.fill) {
    const [fx, fy, fz] = f.box.size, cy = f.box.centre[1];
    part(new THREE.BoxGeometry(fx, fy, fz), f.box.centre, null, { name: "fill", face: FILL, opacity: o.style === "line" ? 1 : 0.38, occlude: false });
    const sheets = [];
    for (let i = 1; i < sheetsPer; i++) {
      const x = -fx / 2 + (i * fx) / sheetsPer;
      for (let k = 0; k < 6; k++) {
        const z0 = -fz / 2 + (k * fz) / 6, z1 = z0 + fz / 6;
        sheets.push(x, cy - fy / 2, z0, x, cy + fy / 2, (z0 + z1) / 2, x, cy + fy / 2, (z0 + z1) / 2, x, cy - fy / 2, z1);
      }
    }
    lines(sheets, 0.2, "fill");
  }

  const nz = cell.nozzles, nozzlePts = [];
  for (let r = 0; r < nz.rows; r++) {
    const z = -sz / 2 + ((r + 0.5) * sz) / nz.rows;
    part(new THREE.CylinderGeometry(nz.headerDiameterM / 2, nz.headerDiameterM / 2, sx - 0.2, 12), [0, nz.headerYM, z], [0, 0, Math.PI / 2], { name: "nozzles", face: METAL, a: 0.7, thr: 40 });
    for (let c = 0; c < nz.columns; c++) {
      const x = -((nz.columns - 1) * nz.pitchM) / 2 + c * nz.pitchM;
      part(new THREE.ConeGeometry(0.07, 0.14, 6), [x, nz.headerYM - 0.16, z], null, { name: "nozzles", face: METAL, a: 0.6, thr: 40, occlude: false });
      nozzlePts.push([x, nz.headerYM - 0.25, z]);
    }
  }
  part(new THREE.CylinderGeometry(0.2, 0.2, sz + 1.2, 16), [-sx / 2 - 0.45, nz.headerYM, 0], [Math.PI / 2, 0, 0], { name: "nozzles", face: METAL, thr: 40 });

  const dy = cell.drift.panels[0].centre[1], blades = [];
  for (let i = 0; i <= 40; i++) {
    const x = -sx / 2 + (i * sx) / 40;
    if (i < 40) for (const z of [-sz / 2 + 0.05, sz / 2 - 0.05]) blades.push(x, dy - 0.12, z, x + sx / 80, dy, z, x + sx / 80, dy, z, x, dy + 0.12, z);
  }
  lines(blades, 0.35, "drift");
  part(new THREE.BoxGeometry(sx, 0.24, sz), [0, dy, 0], null, { name: "drift", face: FILL, opacity: o.style === "line" ? 1 : 0.5, occlude: false, a: 0.8 });

  const fan = cell.fan, st = fan.stack, deckY = st.bottomM + 0.16;
  const deck = new THREE.Shape([[-sx / 2, -sz / 2], [sx / 2, -sz / 2], [sx / 2, sz / 2], [-sx / 2, sz / 2]].map(([a, b]) => new THREE.Vector2(a, b)));
  const hole = new THREE.Path();
  hole.absarc(0, 0, st.throatDiameterM / 2, 0, Math.PI * 2, false);
  deck.holes.push(hole);
  part(new THREE.ExtrudeGeometry(deck, { depth: 0.16, bevelEnabled: false, curveSegments: 64 }), [0, deckY, 0], [Math.PI / 2, 0, 0], { name: "casing" });
  const prof = [[st.throatDiameterM / 2 + 0.25, 0], [st.throatDiameterM / 2, 0.25], [fan.diameterM / 2 + 0.06, 0.55], [fan.diameterM / 2 + 0.08, 1.0], [st.exitDiameterM / 2 - 0.12, 1.75], [st.exitDiameterM / 2, st.topM - st.bottomM]];
  const lathe = new THREE.LatheGeometry(prof.map(([r, y]) => new THREE.Vector2(r, y)), 72);
  add(on(new THREE.Mesh(lathe, faceMat(FACE, "stack")), L.face), [0, deckY, 0]);
  add(on(new THREE.Mesh(lathe, occ), L.line), [0, deckY, 0]);
  const stack = [];
  for (const [r, y] of prof) ring(r, deckY + y, 72, stack);
  for (let i = 0; i < 24; i++) {
    const a = (i / 24) * Math.PI * 2;
    for (let j = 0; j < prof.length - 1; j++) stack.push(Math.cos(a) * prof[j][0], deckY + prof[j][1], Math.sin(a) * prof[j][0], Math.cos(a) * prof[j + 1][0], deckY + prof[j + 1][1], Math.sin(a) * prof[j + 1][0]);
  }
  lines(stack, 0.55, "stack");

  const rotor = new THREE.Group();
  rotor.position.set(...fan.centre);
  rotor.rotation.y = 0.2;
  const hubGeo = new THREE.CylinderGeometry(fan.hubDiameterM / 2, fan.hubDiameterM / 2, 0.35, 24);
  rotor.add(on(new THREE.Mesh(hubGeo, faceMat(METAL, "fan")), L.face), on(new THREE.Mesh(hubGeo, occ), L.line), on(new THREE.LineSegments(new THREE.EdgesGeometry(hubGeo, 20), lineMat(1, "fan")), L.line));
  const reach = fan.diameterM / 2 - fan.hubDiameterM / 2 - 0.05;
  for (let i = 0; i < fan.bladeCount; i++) {
    const arm = new THREE.Group(), blade = new THREE.Group(), bg = new THREE.BoxGeometry(reach, 0.04, 0.42);
    arm.rotation.y = (i / fan.bladeCount) * Math.PI * 2;
    blade.add(on(new THREE.Mesh(bg, faceMat(METAL, "fan")), L.face), on(new THREE.Mesh(bg, occ), L.line), on(new THREE.LineSegments(new THREE.EdgesGeometry(bg), lineMat(1, "fan")), L.line));
    blade.position.x = fan.hubDiameterM / 2 + reach / 2;
    blade.rotation.x = 0.32;
    arm.add(blade);
    rotor.add(arm);
  }
  scene.add(rotor);
  lines(ring(fan.diameterM / 2 + 0.07, fan.centre[1], 72), 0.5, "fan");
  const gb = cell.drive.gearbox;
  part(new THREE.BoxGeometry(...gb.size), gb.centre, null, { name: "drive", face: METAL });
  lines([0, gb.centre[1], 0, 0, fan.centre[1], 0, 0, gb.centre[1], 0, sx / 2 + 0.15, gb.centre[1], 0], 0.9, "drive");
  part(new THREE.CylinderGeometry(0.32, 0.32, 0.8, 20), [sx / 2 + 0.55, gb.centre[1], 0], [0, 0, Math.PI / 2], { name: "drive", face: METAL });

  const flowMat = (a) => new THREE.LineBasicMaterial({ vertexColors: true, transparent: true, opacity: a, depthWrite: false });
  const flow = (pts, cols, a) => {
    const geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.Float32BufferAttribute(pts, 3));
    geo.setAttribute("color", new THREE.Float32BufferAttribute(cols, 3));
    add(on(new THREE.LineSegments(geo, flowMat(a)), L.flow));
  };
  const rand = rng(7), n = o.tier > 1 ? 1 : 0.5, keepZ = o.flowZ ?? 99;
  if (o.water !== false) {
    const pts = [], cols = [], fillTop = Math.max(...cell.fill.map((f) => f.box.centre[1] + f.box.size[1] / 2)), surf = wd + 0.12;
    const tint = (y) => { const u = Math.max(0, Math.min(1, (y - surf) / (nz.headerYM - surf))); return u < 0.5 ? COLD.clone().lerp(MID, u * 2) : MID.clone().lerp(HOT, u * 2 - 1); };
    for (const p of nozzlePts) for (let i = 0; i < 12 * n; i++) {
      const y = p[1] - rand() * (p[1] - surf - 0.1), above = y > fillTop;
      const spread = above ? (p[1] - y) * 0.55 : 0.3, len = above ? 0.09 : 0.16;
      const x = p[0] + (rand() - 0.5) * spread * 2, z = p[2] + (rand() - 0.5) * spread * 2;
      if (z > keepZ) continue;
      pts.push(x, y, z, x, y - len, z);
      cols.push(...tint(y).toArray(), ...tint(y - len).toArray());
    }
    flow(pts, cols, light ? 0.9 : 0.9);
  }
  if (o.air !== false) {
    const pts = [], len = 0.34;
    for (let i = 0; i < 280 * n; i++) {
      const s = rand() < 0.5 ? -1 : 1, t = rand(), x = (rand() - 0.5) * (sx - 0.4), y0 = 0.8 + rand() * 1.2;
      let p, d;
      if (t < 0.25) { p = [x, y0, s * (sz / 2 + 1.1 - (t / 0.25) * 1.1)]; d = [0, 0, -s]; }
      else if (t < 0.45) { const u = (t - 0.25) / 0.2; p = [x, y0 + u * (inletTop - y0 + 0.3), s * (sz / 2) * (1 - u) * (0.4 + 0.6 * rand())]; d = [0, 0.7, -s * 0.7]; }
      else if (t < 0.8) { const u = (t - 0.45) / 0.35; p = [x * (1 - u * 0.35), inletTop + 0.3 + u * (st.bottomM - inletTop - 0.3), (rand() - 0.5) * (sz - 0.6) * (1 - u * 0.35)]; d = [-x * 0.05, 1, 0]; }
      else { const u = (t - 0.8) / 0.2, a = rand() * Math.PI * 2, r = Math.sqrt(rand()) * (fan.diameterM / 2 - 0.2) * (1 + u * 0.25); p = [Math.cos(a) * r, st.bottomM + u * (st.topM - st.bottomM + 2.0), Math.sin(a) * r]; d = [-Math.sin(a) * 0.35, 1, Math.cos(a) * 0.35]; }
      if (p[2] > keepZ && t >= 0.45) continue;
      const m = Math.hypot(...d);
      pts.push(...p, p[0] + (d[0] / m) * len, p[1] + (d[1] / m) * len, p[2] + (d[2] / m) * len);
    }
    flow(pts, Array.from({ length: pts.length / 3 }, () => AIR.toArray()).flat(), light ? 0.6 : 0.55);
  }

  const dimMat = new THREE.LineBasicMaterial({ color: DIM, transparent: true, opacity: 0.95, depthTest: false });
  const labels = [];
  const dim = (layer, p0, p1, off, text, { tag = "", chain = false, side = false } = {}) => {
    const v0 = new THREE.Vector3(...p0), v1 = new THREE.Vector3(...p1), ov = new THREE.Vector3(...off);
    const q0 = v0.clone().add(ov), q1 = v1.clone().add(ov), on1 = ov.clone().normalize(), along = q1.clone().sub(q0).normalize();
    const tick = along.clone().add(on1).normalize().multiplyScalar(0.16);
    const pts = [...q0.toArray(), ...q1.toArray()];
    if (!chain) for (const [v, q] of [[v0, q0], [v1, q1]]) pts.push(...v.clone().addScaledVector(on1, 0.15).toArray(), ...q.clone().addScaledVector(on1, 0.2).toArray());
    else for (const q of [q0, q1]) pts.push(...q.clone().addScaledVector(on1, -0.15).toArray(), ...q.clone().addScaledVector(on1, 0.15).toArray());
    for (const q of [q0, q1]) pts.push(...q.clone().sub(tick).toArray(), ...q.clone().add(tick).toArray());
    const m = segs(pts, dimMat, layer);
    m.renderOrder = 10;
    labels.push({ layer, at: q0.clone().add(q1).multiplyScalar(0.5), text, tag, side });
  };
  const ring3 = (layer, r, y, text) => {
    segs(ring(r, y, 96), dimMat, layer).renderOrder = 10;
    labels.push({ layer, at: new THREE.Vector3(0, y, 0), text, tag: "" });
  };
  const note = (layer, at, text) => labels.push({ layer, at: new THREE.Vector3(...at), text, tag: "", note: true });

  const bx = bs.size[0] / 2, bz = bs.size[2] / 2, H = st.topM;
  dim(L.dim3, [-bx, 0, bz], [-bx, H, bz], [-1.1, 0, 0], `${H.toFixed(2)} m`);
  dim(L.dim3, [-bx, 0, bz], [bx, 0, bz], [0, 0, 1.3], `${bs.size[0].toFixed(2)} m`);
  dim(L.dim3, [bx, 0, bz], [bx, 0, -bz], [1.3, 0, 0], `${bs.size[2].toFixed(2)} m`);
  dim(L.dim3, [-fan.diameterM / 2, H, 0], [fan.diameterM / 2, H, 0], [0, 0.9, 0], `Ø ${fan.diameterM.toFixed(2)} m`);
  dim(L.dim3, [sx / 2, cell.inlets[0].bottomM, sz / 2], [sx / 2, inletTop, sz / 2], [0.9, 0, 0], `${cell.inlets[0].heightM.toFixed(2)} m`, { tag: "inlet" });

  dim(L.plan, [-bx, H, bz], [bx, H, bz], [0, 0, 1.4], `${bs.size[0].toFixed(2)}`);
  dim(L.plan, [bx, H, bz], [bx, H, -bz], [1.4, 0, 0], `${bs.size[2].toFixed(2)}`);
  dim(L.plan, [-fan.diameterM / 2, H + 0.2, 0], [fan.diameterM / 2, H + 0.2, 0], [0, 0, 0], `Ø ${fan.diameterM.toFixed(2)}`, { chain: true });
  dim(L.plan, [-sx / 2 - 0.65, H, -sz / 2 - 0.6], [-sx / 2 - 0.65, H, sz / 2 + 0.6], [-0.9, 0, 0], `header ${(sz + 1.2).toFixed(2)}`);
  note(L.plan, [-2.2, H, bz + 1.4], "air in ↑");
  note(L.plan, [0, H, -bz - 1.0], "air in ↓");

  const ez = bz + 0.4;
  dim(L.elev, [-bx, 0, ez], [-bx, H, ez], [-1.3, 0, 0], `${H.toFixed(2)}`);
  dim(L.elev, [-bx, 0, ez], [bx, 0, ez], [0, -1.0, 0], `${bs.size[0].toFixed(2)}`);
  dim(L.elev, [-fan.diameterM / 2, H, ez], [fan.diameterM / 2, H, ez], [0, 0.9, 0], `Ø ${fan.diameterM.toFixed(2)}`);
  dim(L.elev, [bx, cell.inlets[0].bottomM, ez], [bx, inletTop, ez], [1.0, 0, 0], `${cell.inlets[0].heightM.toFixed(2)}`, { tag: "inlet" });
  dim(L.elev, [bx, st.bottomM, ez], [bx, H, ez], [1.0, 0, 0], `${(H - st.bottomM).toFixed(2)}`, { tag: "stack" });

  const fillBot = Math.min(...cell.fill.map((f) => f.box.centre[1] - f.box.size[1] / 2)), fillTop = Math.max(...cell.fill.map((f) => f.box.centre[1] + f.box.size[1] / 2));
  const deTop = dy + 0.12, chain = [[0, bs.size[1], "basin"], [bs.size[1], inletTop, "inlet"], [fillBot, fillTop, "fill"], [fillTop, deTop, "spray"], [deTop, st.bottomM, "plenum"], [st.bottomM, H, "stack"]];
  for (const [a, b, t] of chain) dim(L.sect, [bx, a, 0.2], [bx, b, 0.2], [1.1, 0, 0], `${(b - a).toFixed(2)} <small>${t}</small>`, { chain: true, side: true });
  note(L.sect, [-1.9, (fillBot + fillTop) / 2, 0.2], `${cell.fill[0].recordId} · ${cell.fill.length} × ${cell.fill[0].box.size[1].toFixed(2)}`);
  note(L.sect, [-1.6, nz.headerYM + 0.05, 0.2], `${nz.count} × ${nz.recordId}`);
  note(L.sect, [1.6, deTop + 0.05, 0.2], cell.drift.recordId);
  note(L.sect, [0, fan.centre[1] + 0.45, 0.2], `${fan.recordId} · ${fan.bladeCount} blades`);
  note(L.sect, [0, wd + 0.35, 0.2], `water ${wd.toFixed(2)}`);

  scene.add(new THREE.HemisphereLight(light ? 0xffffff : 0xcfe9dc, light ? 0xb9c9c0 : 0x0b100f, light ? 1.4 : 0.9));
  const key = new THREE.DirectionalLight(0xffffff, light ? 1.6 : 1.1);
  key.position.set(6, 12, 9);
  const rim = new THREE.DirectionalLight(ACC, light ? 0.4 : 1.4);
  rim.position.set(-9, 5, -7);
  scene.add(key, rim);
  for (const obj of [key, rim]) obj.layers.enableAll();
  scene.children.filter((c) => c.isHemisphereLight).forEach((h) => h.layers.enableAll());
  return { scene, plane, labels, cutZ: (1 - o.cut) * (bz + 0.1) };
}

const renderer = (w, h, el, cls) => {
  const r = new THREE.WebGLRenderer({ antialias: true, alpha: true, preserveDrawingBuffer: true });
  r.setPixelRatio(1);
  r.setSize(w, h);
  r.localClippingEnabled = true;
  r.domElement.className = cls;
  el.append(r.domElement);
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
  const cam = new THREE.PerspectiveCamera(26, w / h, 0.1, 300);
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
  if (!light) {
    const ctx = glow.getContext("2d");
    ctx.filter = "blur(5px)";
    ctx.drawImage(edges.domElement, 0, 0);
    ctx.filter = "blur(1.5px)";
    ctx.drawImage(edges.domElement, 0, 0);
  }
  const at = (v) => { const p = v.clone().project(cam); return [((p.x + 1) / 2) * w, ((1 - p.y) / 2) * h]; };
  if (o.dims !== false) for (const l of m.labels.filter((l) => l.layer === L.dim3)) label(el, l, ...at(l.at));
  return { at: (p) => at(new THREE.Vector3(...p)) };
}

export function mountGA(el, g, o = {}) {
  o = { tier: 2, pick: null, views: [], scale: 30, ...o };
  const w = el.clientWidth, h = el.clientHeight;
  const m = build(g, { ...o, cut: 0, style: "line", flowZ: 0.1 });
  const r = renderer(w, h, el, "cv edges");
  r.setScissorTest(true);
  const kinds = {
    plan: { layer: L.plan, eye: [0, 50, 0], up: [0, 0, -1], cut: 99, centre: [0, 0, 0.5] },
    elev: { layer: L.elev, eye: [0, 0, 50], up: [0, 1, 0], cut: 99, centre: [-0.1, 4.0, 0] },
    sect: { layer: L.sect, eye: [0, 0, 50], up: [0, 1, 0], cut: 0, centre: [1.7, 4.0, 0], flows: true },
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
