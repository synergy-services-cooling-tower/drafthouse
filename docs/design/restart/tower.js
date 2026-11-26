// Preview-only port of tower3d.md as line art: edges in the accent, depth-only occluders, colour only in water and air. Not product code.
import * as THREE from "three";

function rng(seed) {
  let s = seed;
  return () => ((s = (s * 16807) % 2147483647) / 2147483647);
}

export function mountTower(el, g, o = {}) {
  const view = { cutaway: 0.55, air: true, water: true, labels: false, tier: 2, pick: null, ...o };
  const css = (n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim();
  const ACC = new THREE.Color(css("--accent")), HI = new THREE.Color(css("--accent-hi"));
  const MID = new THREE.Color(css("--flow-mid")), HOT = new THREE.Color(css("--flow-hot")), COLD = new THREE.Color(css("--flow-cold")), AIR = new THREE.Color(css("--flow-air"));
  const light = document.documentElement.classList.contains("light");
  const cell = g.cells[0];
  const w = el.clientWidth, h = el.clientHeight;
  const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, preserveDrawingBuffer: true });
  renderer.setPixelRatio(Math.min(2, window.devicePixelRatio || 1));
  renderer.setSize(w, h);
  renderer.localClippingEnabled = true;
  el.prepend(renderer.domElement);

  const scene = new THREE.Scene();
  const half = g.size[2] / 2 + 0.1;
  const cutZ = (1 - view.cutaway) * half;
  const clip = view.cutaway > 0 ? [new THREE.Plane(new THREE.Vector3(0, 0, -1), cutZ)] : [];
  const mats = new Map();
  const ink = (a = 1, part = null) => {
    const picked = view.pick && part === view.pick, dim = view.pick && !picked ? 0.55 : 1;
    const k = `${a}|${picked}|${dim}`;
    if (!mats.has(k)) mats.set(k, new THREE.LineBasicMaterial({ color: picked ? HI : ACC, transparent: true, opacity: Math.min(1, a * dim * (light ? 1 : 0.92)), clippingPlanes: clip }));
    return mats.get(k);
  };
  const occ = new THREE.MeshBasicMaterial({ colorWrite: false, side: THREE.DoubleSide, polygonOffset: true, polygonOffsetFactor: 2, polygonOffsetUnits: 2, clippingPlanes: clip });
  const place = (obj, pos, rot) => { obj.position.set(...pos); if (rot) obj.rotation.set(...rot); scene.add(obj); return obj; };
  const ghostMat = new THREE.LineBasicMaterial({ color: ACC, transparent: true, opacity: light ? 0.22 : 0.16, depthWrite: false });
  const part = (geo, pos, rot, { a = 1, solid = true, thr = 20, name = null, ghost = false } = {}) => {
    const grp = new THREE.Group(), edges = new THREE.EdgesGeometry(geo, thr);
    grp.add(new THREE.LineSegments(edges, ink(a, name)));
    if (ghost && clip.length) grp.add(new THREE.LineSegments(edges, ghostMat));
    if (solid) grp.add(new THREE.Mesh(geo, occ));
    return place(grp, pos, rot);
  };
  const lines = (pts, a = 1, name = null, cols = null, ghost = false) => {
    const geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.Float32BufferAttribute(pts, 3));
    if (cols) geo.setAttribute("color", new THREE.Float32BufferAttribute(cols, 3));
    const m = cols ? new THREE.LineBasicMaterial({ vertexColors: true, transparent: true, opacity: a, clippingPlanes: [] , depthWrite: false }) : ink(a, name);
    if (ghost && clip.length) place(new THREE.LineSegments(geo, ghostMat), [0, 0, 0]);
    return place(new THREE.LineSegments(geo, m), [0, 0, 0]);
  };
  const ring = (r, y, n = 64, out = [], cx = 0, cz = 0) => {
    for (let i = 0; i < n; i++) {
      const a0 = (i / n) * Math.PI * 2, a1 = ((i + 1) / n) * Math.PI * 2;
      out.push(cx + Math.cos(a0) * r, y, cz + Math.sin(a0) * r, cx + Math.cos(a1) * r, y, cz + Math.sin(a1) * r);
    }
    return out;
  };

  const bs = cell.basin.box, wd = cell.basin.waterDepthM;
  part(new THREE.BoxGeometry(...bs.size), bs.centre, null, { name: "basin", ghost: true });
  const sh = cell.shell.box, [sx, sy, sz] = sh.size, bottom = sh.centre[1] - sy / 2, top = sh.centre[1] + sy / 2;
  const wall = cell.shell.wallM, inletTop = cell.inlets[0].bottomM + cell.inlets[0].heightM;
  for (const s of [-1, 1]) {
    part(new THREE.BoxGeometry(wall, sy, sz), [s * sx / 2, sh.centre[1], 0], null, { name: "casing", ghost: true });
    part(new THREE.BoxGeometry(sx, top - inletTop, wall), [0, (top + inletTop) / 2, s * sz / 2], null, { name: "casing", ghost: true });
  }
  const seams = [];
  for (let i = 1; i < 4; i++) {
    const v = -sx / 2 + (i * sx) / 4;
    for (const s of [-1, 1]) {
      seams.push(v, inletTop, s * (sz / 2 + wall / 2 + 0.01), v, top, s * (sz / 2 + wall / 2 + 0.01));
      seams.push(s * (sx / 2 + wall / 2 + 0.01), bottom, v, s * (sx / 2 + wall / 2 + 0.01), top, v);
    }
  }
  lines(seams, 0.35, "casing");
  for (const x of [-1, 0, 1]) for (const z of [-1, 1]) part(new THREE.BoxGeometry(0.18, inletTop - bottom, 0.18), [x * (sx / 2 - 0.09), (inletTop + bottom) / 2, z * (sz / 2 - 0.09)], null, { name: "casing" });

  const louver = [];
  for (const inl of cell.inlets) {
    const s = inl.face === "+z" ? 1 : -1;
    for (let i = 0; i < 7; i++) {
      const y = inl.bottomM + 0.15 + (i * (inl.heightM - 0.2)) / 6, z = s * (sz / 2 - 0.05);
      louver.push(-sx / 2, y, z, sx / 2, y, z, -sx / 2, y, z, -sx / 2, y - 0.16, z - s * 0.18);
    }
  }
  lines(louver, 0.45, "inlet");

  const sheetsPer = view.tier > 1 ? 32 : 16;
  cell.fill.forEach((f) => {
    const [fx, fy, fz] = f.box.size, cy = f.box.centre[1];
    part(new THREE.BoxGeometry(fx, fy, fz), f.box.centre, null, { solid: false, name: "fill" });
    const sheets = [];
    for (let i = 1; i < sheetsPer; i++) {
      const x = -fx / 2 + (i * fx) / sheetsPer;
      sheets.push(x, cy - fy / 2, -fz / 2, x, cy - fy / 2, fz / 2, x, cy + fy / 2, -fz / 2, x, cy + fy / 2, fz / 2);
      for (let k = 0; k < 6; k++) {
        const z0 = -fz / 2 + (k * fz) / 6, z1 = z0 + fz / 6;
        sheets.push(x, cy - fy / 2, z0, x, cy + fy / 2, (z0 + z1) / 2, x, cy + fy / 2, (z0 + z1) / 2, x, cy - fy / 2, z1);
      }
    }
    lines(sheets, 0.16, "fill");
  });

  const nz = cell.nozzles, nozzlePts = [];
  for (let rI = 0; rI < nz.rows; rI++) {
    const z = -sz / 2 + ((rI + 0.5) * sz) / nz.rows;
    part(new THREE.BoxGeometry(sx - 0.2, nz.headerDiameterM, nz.headerDiameterM), [0, nz.headerYM, z], null, { a: 0.7, name: "nozzles" });
    for (let c = 0; c < nz.columns; c++) {
      const x = -((nz.columns - 1) * nz.pitchM) / 2 + c * nz.pitchM;
      part(new THREE.ConeGeometry(0.07, 0.14, 6), [x, nz.headerYM - 0.16, z], null, { a: 0.6, solid: false, thr: 40, name: "nozzles" });
      nozzlePts.push([x, nz.headerYM - 0.25, z]);
    }
  }
  part(new THREE.BoxGeometry(0.4, 0.4, sz + 1.2), [-sx / 2 - 0.45, nz.headerYM, 0], null, { name: "nozzles" });

  const dy = cell.drift.panels[0].centre[1], drift = [];
  for (let i = 0; i <= 40; i++) {
    const x = -sx / 2 + (i * sx) / 40;
    drift.push(x, dy - 0.12, -sz / 2, x, dy - 0.12, sz / 2, x, dy + 0.12, -sz / 2, x, dy + 0.12, sz / 2);
    if (i < 40) for (const z of [-sz / 2, sz / 2]) drift.push(x, dy - 0.12, z, x + sx / 80, dy, z, x + sx / 80, dy, z, x, dy + 0.12, z);
  }
  lines(drift, 0.3, "drift");
  part(new THREE.BoxGeometry(sx, 0.24, sz), [0, dy, 0], null, { solid: false, a: 0.8, name: "drift" });

  const fan = cell.fan, st = fan.stack, deckY = st.bottomM + 0.16;
  const deckShape = new THREE.Shape([[-sx / 2, -sz / 2], [sx / 2, -sz / 2], [sx / 2, sz / 2], [-sx / 2, sz / 2]].map(([a, b]) => new THREE.Vector2(a, b)));
  const hole = new THREE.Path();
  hole.absarc(0, 0, st.throatDiameterM / 2, 0, Math.PI * 2, false);
  deckShape.holes.push(hole);
  part(new THREE.ExtrudeGeometry(deckShape, { depth: 0.16, bevelEnabled: false, curveSegments: 64 }), [0, deckY, 0], [Math.PI / 2, 0, 0], { name: "casing", ghost: true });
  const prof = [[st.throatDiameterM / 2 + 0.25, 0], [st.throatDiameterM / 2, 0.25], [fan.diameterM / 2 + 0.06, 0.55], [fan.diameterM / 2 + 0.08, 1.0], [st.exitDiameterM / 2 - 0.12, 1.75], [st.exitDiameterM / 2, st.topM - st.bottomM]];
  const lathe = new THREE.LatheGeometry(prof.map(([r, y]) => new THREE.Vector2(r, y)), 72);
  place(new THREE.Mesh(lathe, occ), [0, deckY, 0]);
  const stack = [];
  for (const [r, y] of prof) ring(r, deckY + y, 72, stack);
  for (let i = 0; i < 24; i++) {
    const a = (i / 24) * Math.PI * 2;
    for (let j = 0; j < prof.length - 1; j++) stack.push(Math.cos(a) * prof[j][0], deckY + prof[j][1], Math.sin(a) * prof[j][0], Math.cos(a) * prof[j + 1][0], deckY + prof[j + 1][1], Math.sin(a) * prof[j + 1][0]);
  }
  lines(stack, 0.8, "stack", null, true);

  const rotor = new THREE.Group();
  rotor.position.set(...fan.centre);
  const hubGeo = new THREE.CylinderGeometry(fan.hubDiameterM / 2, fan.hubDiameterM / 2, 0.35, 24);
  rotor.add(new THREE.LineSegments(new THREE.EdgesGeometry(hubGeo, 20), ink(1, "fan")), new THREE.Mesh(hubGeo, occ));
  const reach = fan.diameterM / 2 - fan.hubDiameterM / 2 - 0.05;
  for (let i = 0; i < fan.bladeCount; i++) {
    const arm = new THREE.Group();
    arm.rotation.y = (i / fan.bladeCount) * Math.PI * 2;
    const bg = new THREE.BoxGeometry(reach, 0.03, 0.42);
    const blade = new THREE.Group();
    blade.add(new THREE.LineSegments(new THREE.EdgesGeometry(bg), ink(1, "fan")), new THREE.Mesh(bg, occ));
    blade.position.x = fan.hubDiameterM / 2 + reach / 2;
    blade.rotation.x = 0.32;
    arm.add(blade);
    rotor.add(arm);
  }
  rotor.rotation.y = 0.2;
  scene.add(rotor);
  lines(ring(fan.diameterM / 2 + 0.07, fan.centre[1], 72), 0.5, "fan");
  const gb = cell.drive.gearbox;
  part(new THREE.BoxGeometry(...gb.size), gb.centre, null, { name: "drive" });
  lines([0, gb.centre[1], 0, 0, fan.centre[1], 0, 0, gb.centre[1], 0, sx / 2 + 0.15, gb.centre[1], 0], 0.9, "drive");
  part(new THREE.CylinderGeometry(0.32, 0.32, 0.8, 20), [sx / 2 + 0.55, gb.centre[1], 0], [0, 0, Math.PI / 2], { name: "drive" });

  const rand = rng(7), n = view.tier > 1 ? 1 : 0.5;
  if (view.water) {
    const pts = [], cols = [], fillTop = Math.max(...cell.fill.map((f) => f.box.centre[1] + f.box.size[1] / 2)), surf = wd + 0.12;
    const tint = (y) => { const u = Math.max(0, Math.min(1, (y - surf) / (nz.headerYM - surf))); return u < 0.5 ? COLD.clone().lerp(MID, u * 2) : MID.clone().lerp(HOT, u * 2 - 1); };
    for (const p of nozzlePts) for (let i = 0; i < 12 * n; i++) {
      const y = p[1] - rand() * (p[1] - surf - 0.1);
      const sprayK = y > fillTop ? (p[1] - y) * 0.55 : 0.3, len = y > fillTop ? 0.09 : 0.16;
      const dx = (rand() - 0.5) * sprayK * 2, dz = (rand() - 0.5) * sprayK * 2;
      const x = p[0] + dx, z = p[2] + dz;
      if (z > cutZ + 0.3) continue;
      const lean = y > fillTop ? 0.25 : 0;
      pts.push(x, y, z, x + dx * lean * 0.3, y - len, z + dz * lean * 0.3);
      cols.push(...tint(y).toArray(), ...tint(y - len).toArray());
    }
    const ripple = [];
    for (let i = 0; i < 9; i++) { const z = -sz / 2 + 0.4 + i * ((Math.min(cutZ, sz / 2) + sz / 2 - 0.8) / 8); ripple.push(-bs.size[0] / 2 + 0.25, surf, z, bs.size[0] / 2 - 0.25, surf, z); }
    lines(pts, light ? 0.9 : 0.85, null, cols);
    lines(ripple, 0.7, null, Array.from({ length: ripple.length / 3 }, () => COLD.toArray()).flat());
  }
  if (view.air) {
    const pts = [], L = 0.34;
    for (let i = 0; i < 420 * n; i++) {
      const s = rand() < 0.5 ? -1 : 1, t = rand(), x = (rand() - 0.5) * (sx - 0.4), y0 = 0.8 + rand() * 1.2;
      let p, d;
      if (t < 0.25) { p = [x, y0, s * (sz / 2 + 1.1 - (t / 0.25) * 1.1)]; d = [0, 0, -s]; }
      else if (t < 0.45) { const u = (t - 0.25) / 0.2; p = [x, y0 + u * (inletTop - y0 + 0.3), s * (sz / 2) * (1 - u) * (0.4 + 0.6 * rand())]; d = [0, 0.7, -s * 0.7]; }
      else if (t < 0.8) { const u = (t - 0.45) / 0.35; p = [x * (1 - u * 0.35), inletTop + 0.3 + u * (st.bottomM - inletTop - 0.3), (rand() - 0.5) * (sz - 0.6) * (1 - u * 0.35)]; d = [-x * 0.05, 1, 0]; }
      else { const u = (t - 0.8) / 0.2, a = rand() * Math.PI * 2, r = Math.sqrt(rand()) * (fan.diameterM / 2 - 0.2) * (1 + u * 0.25); p = [Math.cos(a) * r, st.bottomM + u * (st.topM - st.bottomM + 2.0), Math.sin(a) * r]; d = [-Math.sin(a) * 0.35, 1, Math.cos(a) * 0.35]; }
      if (p[2] > cutZ + 0.3 && t >= 0.45) continue;
      const m = Math.hypot(...d);
      pts.push(...p, p[0] + (d[0] / m) * L, p[1] + (d[1] / m) * L, p[2] + (d[2] / m) * L);
    }
    lines(pts, light ? 0.6 : 0.5, null, Array.from({ length: pts.length / 3 }, () => AIR.toArray()).flat());
  }

  const span = view.span || 13, aim = new THREE.Vector3(...(view.target || [0, 4, 0]));
  const cam = new THREE.OrthographicCamera((-span * w) / h / 2, (span * w) / h / 2, span / 2, -span / 2, 0.1, 200);
  cam.position.copy(aim).add(new THREE.Vector3(...(view.dir || [1, 0.82, 1])).normalize().multiplyScalar(40));
  cam.lookAt(aim);
  if (view.shift) { cam.left -= view.shift[0]; cam.right -= view.shift[0]; cam.top -= view.shift[1]; cam.bottom -= view.shift[1]; cam.updateProjectionMatrix(); }
  renderer.render(scene, cam);

  if (view.labels) {
    for (const [text, p, picked] of view.labelSet) {
      const v = new THREE.Vector3(...p).project(cam);
      const d = document.createElement("div");
      d.className = "tl" + (picked ? " on" : "");
      d.style.left = ((v.x + 1) / 2) * w + "px";
      d.style.top = ((1 - v.y) / 2) * h + "px";
      d.innerHTML = `<i></i><span>${text}</span>`;
      el.append(d);
    }
  }
  return { renderer, scene, cam };
}
