// Preview-only port of tower3d.md: one Three.js scene built from the TowerGeometry fixture. Not product code.
import * as THREE from "three";

const C = {
  concrete: 0xcfcac2, water: 0x3c797d, casing: 0xe2e6e5, louver: 0x8f999a, post: 0xb6b1a8,
  fill: [0xd8c39b, 0xc9b184, 0xbca274], pipe: 0x6c777b, nozzle: 0x3d3d3d, drift: 0x87968b,
  deck: 0xd9d6cf, stack: 0xbfc6c6, blade: 0x3d3d3d, hub: 0x5e5e5e, drive: 0x6b6f74,
  hot: new THREE.Color(0xc8803f), cold: new THREE.Color(0x3c797d), air: new THREE.Color(0x6d8fa6),
};

function rng(seed) {
  let s = seed;
  return () => ((s = (s * 16807) % 2147483647) / 2147483647);
}

export function mountTower(el, g, o = {}) {
  const view = { cutaway: 0.55, air: true, water: true, labels: false, tier: 2, ...o };
  const cell = g.cells[0];
  const w = el.clientWidth, h = el.clientHeight;
  const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, preserveDrawingBuffer: true });
  renderer.setPixelRatio(Math.min(2, window.devicePixelRatio || 1));
  renderer.setSize(w, h);
  renderer.localClippingEnabled = true;
  renderer.shadowMap.enabled = view.tier > 0;
  renderer.shadowMap.type = THREE.PCFSoftShadowMap;
  el.prepend(renderer.domElement);

  const scene = new THREE.Scene();
  const half = g.size[2] / 2 + 0.1;
  const cut = new THREE.Plane(new THREE.Vector3(0, 0, -1), (1 - view.cutaway) * half);
  const clip = view.cutaway > 0 ? [cut] : [];
  const mat = (color, extra = {}) => new THREE.MeshStandardMaterial({ color, roughness: 0.8, metalness: 0.05, side: THREE.DoubleSide, ...extra });
  const clipped = (color, extra = {}) => mat(color, { clippingPlanes: clip, ...extra });
  const add = (geo, m, pos, rot) => {
    const mesh = new THREE.Mesh(geo, m);
    mesh.position.set(...pos);
    if (rot) mesh.rotation.set(...rot);
    mesh.castShadow = mesh.receiveShadow = true;
    scene.add(mesh);
    return mesh;
  };
  const box = (b, m) => add(new THREE.BoxGeometry(...b.size), m, b.centre);

  scene.add(new THREE.HemisphereLight(0xffffff, 0xd6d1c8, 2.2));
  const sun = new THREE.DirectionalLight(0xffffff, 2.4);
  sun.position.set(6, 20, 8);
  sun.castShadow = true;
  sun.shadow.mapSize.set(2048, 2048);
  Object.assign(sun.shadow.camera, { left: -10, right: 10, top: 12, bottom: -6, near: 1, far: 50 });
  sun.shadow.bias = -0.0005;
  scene.add(sun);
  const ground = new THREE.Mesh(new THREE.PlaneGeometry(60, 60), new THREE.ShadowMaterial({ opacity: 0.08 }));
  ground.rotation.x = -Math.PI / 2;
  ground.receiveShadow = true;
  scene.add(ground);

  const bs = cell.basin.box;
  box(bs, clipped(C.concrete));
  const wd = cell.basin.waterDepthM;
  add(new THREE.BoxGeometry(bs.size[0] - 0.3, 0.02, bs.size[2] - 0.3), clipped(C.water, { roughness: 0.15, metalness: 0.1, transparent: true, opacity: 0.85 }), [0, wd + 0.12, 0]);

  const sh = cell.shell.box, [sx, sy, sz] = sh.size, bottom = sh.centre[1] - sy / 2, top = sh.centre[1] + sy / 2;
  const wall = cell.shell.wallM, inletTop = cell.inlets[0].bottomM + cell.inlets[0].heightM;
  const casing = clipped(C.casing, { roughness: 0.55 });
  for (const s of [-1, 1]) {
    add(new THREE.BoxGeometry(wall, sy, sz), casing, [s * sx / 2, sh.centre[1], 0]);
    add(new THREE.BoxGeometry(sx, top - inletTop, wall), casing, [0, (top + inletTop) / 2, s * sz / 2]);
  }
  const post = clipped(C.post);
  for (const x of [-1, 0, 1]) for (const z of [-1, 1]) add(new THREE.BoxGeometry(0.18, inletTop - bottom, 0.18), post, [x * (sx / 2 - 0.09), (inletTop + bottom) / 2, z * (sz / 2 - 0.09)]);

  const slats = 7, louverGeo = new THREE.BoxGeometry(sx - 0.1, 0.03, 0.32);
  const louvers = new THREE.InstancedMesh(louverGeo, clipped(C.louver), slats * cell.inlets.length);
  const m4 = new THREE.Matrix4(), q = new THREE.Quaternion(), e = new THREE.Euler(), one = new THREE.Vector3(1, 1, 1);
  let k = 0;
  for (const inl of cell.inlets) {
    const s = inl.face === "+z" ? 1 : -1;
    for (let i = 0; i < slats; i++) {
      q.setFromEuler(e.set(s * 0.7, 0, 0));
      m4.compose(new THREE.Vector3(0, inl.bottomM + 0.15 + i * (inl.heightM - 0.2) / (slats - 1), s * (sz / 2 - 0.05)), q, one);
      louvers.setMatrixAt(k++, m4);
    }
  }
  louvers.castShadow = true;
  scene.add(louvers);

  const sheetsPer = view.tier > 1 ? 44 : 22;
  cell.fill.forEach((f, li) => {
    const [fx, fy, fz] = f.box.size;
    const sheets = new THREE.InstancedMesh(new THREE.BoxGeometry(0.018, fy - 0.03, fz - 0.06), clipped(C.fill[li % 3], { roughness: 0.9 }), sheetsPer);
    for (let i = 0; i < sheetsPer; i++) {
      m4.compose(new THREE.Vector3(-fx / 2 + (i + 0.5) * fx / sheetsPer, f.box.centre[1], 0), q.identity(), one);
      sheets.setMatrixAt(i, m4);
    }
    sheets.castShadow = sheets.receiveShadow = true;
    scene.add(sheets);
    const ribs = new THREE.InstancedMesh(new THREE.BoxGeometry(fx - 0.06, 0.018, 0.018), clipped(C.fill[(li + 1) % 3]), 2 * 12);
    let r = 0;
    for (const y of [-1, 1]) for (let i = 0; i < 12; i++) {
      m4.compose(new THREE.Vector3(0, f.box.centre[1] + y * (fy / 2 - 0.02), -fz / 2 + (i + 0.5) * fz / 12), q.identity(), one);
      ribs.setMatrixAt(r++, m4);
    }
    scene.add(ribs);
  });

  const nz = cell.nozzles, pipe = mat(C.pipe, { clippingPlanes: clip, metalness: 0.4, roughness: 0.5 });
  const nozzleGeo = new THREE.ConeGeometry(0.07, 0.14, 10);
  const cones = new THREE.InstancedMesh(nozzleGeo, clipped(C.nozzle), nz.rows * nz.columns);
  const nozzlePts = [];
  for (let rI = 0; rI < nz.rows; rI++) {
    const z = -sz / 2 + (rI + 0.5) * sz / nz.rows;
    add(new THREE.CylinderGeometry(nz.headerDiameterM / 2, nz.headerDiameterM / 2, sx - 0.2, 14), pipe, [0, nz.headerYM, z], [0, 0, Math.PI / 2]);
    for (let c = 0; c < nz.columns; c++) {
      const x = -(nz.columns - 1) * nz.pitchM / 2 + c * nz.pitchM;
      m4.compose(new THREE.Vector3(x, nz.headerYM - 0.16, z), q.identity(), one);
      cones.setMatrixAt(rI * nz.columns + c, m4);
      nozzlePts.push([x, nz.headerYM - 0.25, z]);
    }
  }
  scene.add(cones);
  add(new THREE.CylinderGeometry(0.22, 0.22, sz + 1.2, 16), pipe, [-sx / 2 - 0.45, nz.headerYM, 0.0], [Math.PI / 2, 0, 0]);

  const blades = 32, dGeo = new THREE.BoxGeometry(0.02, 0.24, 7.9);
  const drift = new THREE.InstancedMesh(dGeo, clipped(C.drift, { roughness: 0.6 }), blades * 2);
  const dy = cell.drift.panels[0].centre[1];
  for (let i = 0; i < blades; i++) for (const s of [-1, 1]) {
    q.setFromEuler(e.set(0, 0, s * 0.5));
    m4.compose(new THREE.Vector3(-sx / 2 + (i + 0.5) * sx / blades + s * 0.05, dy + s * 0.08, 0), q, one);
    drift.setMatrixAt(i * 2 + (s + 1) / 2, m4);
  }
  scene.add(drift);

  const fan = cell.fan, st = fan.stack;
  const deckShape = new THREE.Shape([[-sx / 2, -sz / 2], [sx / 2, -sz / 2], [sx / 2, sz / 2], [-sx / 2, sz / 2]].map(([a, b]) => new THREE.Vector2(a, b)));
  const hole = new THREE.Path();
  hole.absarc(0, 0, st.throatDiameterM / 2, 0, Math.PI * 2, false);
  deckShape.holes.push(hole);
  add(new THREE.ExtrudeGeometry(deckShape, { depth: 0.16, bevelEnabled: false, curveSegments: 48 }), clipped(C.deck), [0, st.bottomM + 0.16, 0], [Math.PI / 2, 0, 0]);
  const prof = [[st.throatDiameterM / 2 + 0.25, 0], [st.throatDiameterM / 2, 0.25], [fan.diameterM / 2 + 0.06, 0.55], [fan.diameterM / 2 + 0.08, 1.0], [st.exitDiameterM / 2 - 0.12, 1.75], [st.exitDiameterM / 2, st.topM - st.bottomM]];
  add(new THREE.LatheGeometry(prof.map(([r, y]) => new THREE.Vector2(r, y)), 72), clipped(C.stack, { roughness: 0.7, metalness: 0.15 }), [0, st.bottomM + 0.16, 0]);

  const rotor = new THREE.Group();
  rotor.position.set(...fan.centre);
  const hubM = mat(C.hub, { metalness: 0.5, roughness: 0.4 });
  const hubMesh = new THREE.Mesh(new THREE.CylinderGeometry(fan.hubDiameterM / 2, fan.hubDiameterM / 2, 0.35, 24), hubM);
  hubMesh.castShadow = true;
  rotor.add(hubMesh);
  const span = fan.diameterM / 2 - fan.hubDiameterM / 2 - 0.05;
  for (let i = 0; i < fan.bladeCount; i++) {
    const arm = new THREE.Group();
    arm.rotation.y = (i / fan.bladeCount) * Math.PI * 2;
    const blade = new THREE.Mesh(new THREE.BoxGeometry(span, 0.04, 0.42), mat(C.blade, { roughness: 0.5 }));
    blade.position.x = fan.hubDiameterM / 2 + span / 2;
    blade.rotation.x = 0.32;
    blade.castShadow = true;
    arm.add(blade);
    rotor.add(arm);
  }
  rotor.rotation.y = 0.2;
  scene.add(rotor);
  const gb = cell.drive.gearbox;
  box(gb, mat(C.drive, { metalness: 0.4, roughness: 0.5 }));
  add(new THREE.CylinderGeometry(0.06, 0.06, fan.centre[1] - gb.centre[1], 10), mat(C.drive), [0, (fan.centre[1] + gb.centre[1]) / 2, 0]);
  add(new THREE.CylinderGeometry(0.05, 0.05, sx / 2, 10), mat(C.drive), [sx / 4, gb.centre[1], 0], [0, 0, Math.PI / 2]);
  const motor = add(new THREE.CylinderGeometry(0.32, 0.32, 0.8, 20), mat(C.drive, { metalness: 0.4 }), [sx / 2 + 0.55, gb.centre[1], 0], [0, 0, Math.PI / 2]);
  motor.castShadow = false;

  const rand = rng(7);
  const n = view.tier > 1 ? 1 : 0.5;
  if (view.water) {
    const pts = [], cols = [], fillTop = Math.max(...cell.fill.map((f) => f.box.centre[1] + f.box.size[1] / 2));
    for (const p of nozzlePts) for (let i = 0; i < 26 * n; i++) {
      const y = p[1] - rand() * (p[1] - wd - 0.15);
      const spread = y > fillTop ? (p[1] - y) * 0.5 : 0.28;
      pts.push(p[0] + (rand() - 0.5) * spread * 2, y, p[2] + (rand() - 0.5) * spread * 2);
      cols.push(...C.cold.clone().lerp(C.hot, (y - wd) / (p[1] - wd)).toArray());
    }
    const geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.Float32BufferAttribute(pts, 3));
    geo.setAttribute("color", new THREE.Float32BufferAttribute(cols, 3));
    scene.add(new THREE.Points(geo, new THREE.PointsMaterial({ size: 0.055, vertexColors: true, transparent: true, opacity: 0.9 })));
  }
  if (view.air) {
    const pts = [];
    for (let i = 0; i < 650 * n; i++) {
      const s = rand() < 0.5 ? -1 : 1, t = rand(), x = (rand() - 0.5) * (sx - 0.4);
      const y0 = 0.75 + rand() * 1.3;
      if (t < 0.25) pts.push(x, y0, s * (sz / 2 + 0.5 - (t / 0.25) * 0.5));
      else if (t < 0.45) { const u = (t - 0.25) / 0.2; pts.push(x, y0 + u * (inletTop - y0 + 0.3), s * (sz / 2) * (1 - u) * (0.4 + 0.6 * rand())); }
      else if (t < 0.8) { const u = (t - 0.45) / 0.35; pts.push(x * (1 - u * 0.35), inletTop + 0.3 + u * (st.bottomM - inletTop - 0.3), (rand() - 0.5) * (sz - 0.6) * (1 - u * 0.35)); }
      else { const u = (t - 0.8) / 0.2, a = rand() * Math.PI * 2, r = Math.sqrt(rand()) * (fan.diameterM / 2 - 0.2) * (1 + u * 0.2); pts.push(Math.cos(a) * r, st.bottomM + u * (st.topM - st.bottomM + 1.6), Math.sin(a) * r); }
    }
    const geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.Float32BufferAttribute(pts, 3));
    scene.add(new THREE.Points(geo, new THREE.PointsMaterial({ size: 0.08, color: C.air, transparent: true, opacity: 0.45, depthWrite: false })));
  }

  const cam = new THREE.PerspectiveCamera(view.fov || 28, w / h, 0.1, 200);
  cam.position.set(...(view.camera || [15, 9.5, 19]));
  const target = new THREE.Vector3(...(view.target || [0, 3.9, 0]));
  cam.lookAt(target);
  renderer.render(scene, cam);

  if (view.labels) {
    const anchors = view.labelSet || [
      ["Fan · AX-500", [0, fan.centre[1] + 0.3, 1.6]],
      ["Drift eliminator · DE-4P-ULTRA", [2.6, dy, 0.8]],
      ["Spray nozzles · 78 × NZ-32", [-2.4, nz.headerYM, 1.0]],
      ["Fill · FILM-VF38 · 2 × 0.6 m", [1.0, 2.7, 1.2]],
      ["Air inlet · 1.5 m", [-3.0, 1.35, sz / 2]],
      ["Basin", [3.0, 0.4, sz / 2 + 0.2]],
    ];
    for (const [text, p, picked] of anchors) {
      const v = new THREE.Vector3(...p).project(cam);
      const d = document.createElement("div");
      d.className = "tl" + (picked ? " on" : "");
      d.style.left = ((v.x + 1) / 2 * w) + "px";
      d.style.top = ((1 - v.y) / 2 * h) + "px";
      d.innerHTML = `<i></i><span>${text}</span>`;
      el.append(d);
    }
  }
  return { renderer, scene, cam };
}
