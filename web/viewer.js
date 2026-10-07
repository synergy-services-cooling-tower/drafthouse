/**
 * The stage: one three.js scene built from the part spec that web/geometry.js derived.
 *
 * The viewer knows nothing about towers. It receives `parts` (each with solids, an explode
 * vector, an anchor) and draws them; that is why the same code renders a counterflow cell and
 * a crossflow cell. Picking is done exclusively against the meshes this file generated for the
 * pickable assemblies — decorations (louver slats, fill sheets, blades, flow ribbons, grid) are
 * never added to the pick list, and neither is any part whose geometry spec says pickable
 * === false. In this build every assembly maps to a record the selection produced, so all
 * seven are pickable and nothing is decoration-only.
 *
 * three.js is vendored (web/vendor/, MIT — see THIRD_PARTY_NOTICES.md): no CDN, no build step.
 *
 * Rendering is ON DEMAND (no animation loop) so a captured frame is a settled frame. Motion
 * (explode, focus) is a short tween that honours prefers-reduced-motion.
 */
import * as THREE from "./vendor/three.module.js";

const INK = "#17232d";
const LINE = "#5c6f79";
const GRID = "#dce4e8";
const GRID_CENTER = "#eef2f4";
const AIR = "#0b6073";
const WATER = "#c77a1b";

export class ViewerUnavailable extends Error {}

export function webglReason() {
  if (typeof window === "undefined") return "no window";
  if (!window.WebGLRenderingContext) return "WebGLRenderingContext is not defined in this browser";
  const probe = document.createElement("canvas");
  const attributes = { failIfMajorPerformanceCaveat: false };
  let context = null;
  try {
    context = probe.getContext("webgl2", attributes) || probe.getContext("webgl", attributes) || probe.getContext("experimental-webgl", attributes);
  } catch (error) {
    return `context creation threw: ${error.message}`;
  }
  if (!context) return "the browser refused a WebGL context (hardware acceleration disabled or blocklisted)";
  return null;
}

function disposeTree(object) {
  object.traverse((child) => {
    if (child.geometry) child.geometry.dispose();
    if (child.material) {
      const materials = Array.isArray(child.material) ? child.material : [child.material];
      for (const material of materials) material.dispose();
    }
  });
}

export function createViewer(options) {
  const {
    canvas,
    labelHost,
    geometry,
    colourFor,
    onPick = () => {},
    onHover = () => {},
    reducedMotion = false,
    frameRadiusFactor = 1.95,
    /**
     * Emphasis colour for a selected / highlighted part. It must not carry a hue the variant is
     * using for data: variant A codes contribution in teal, so an amber selection reads as
     * "selected"; variant B codes material family, so it emphasises with a neutral lift instead of
     * tinting a PVC-recorded part like a PP one.
     */
    emphasisColour = "#c77a1b"
  } = options;

  const reason = webglReason();
  if (reason) throw new ViewerUnavailable(reason);

  const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: true, preserveDrawingBuffer: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
  renderer.outputColorSpace = THREE.SRGBColorSpace;

  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(38, 1, 0.1, 900);

  const hemi = new THREE.HemisphereLight(0xffffff, 0xdfe7ea, 2.05);
  scene.add(hemi);
  const key = new THREE.DirectionalLight(0xffffff, 1.15);
  key.position.set(-18, 26, 16);
  scene.add(key);
  const fill = new THREE.DirectionalLight(0xffffff, 0.5);
  fill.position.set(20, 12, -18);
  scene.add(fill);

  const stage = new THREE.Group();
  scene.add(stage);

  const gridExtent = Math.ceil(Math.max(geometry.extents.halfX, geometry.extents.halfZ) * 2.6);
  const grid = new THREE.GridHelper(gridExtent * 2, gridExtent * 2, GRID_CENTER, GRID);
  grid.position.y = 0;
  scene.add(grid);

  /* ---- build meshes ------------------------------------------------- */

  const pickable = [];          // flat list of the seven assemblies' body meshes
  const partGroups = new Map(); // partId -> Group
  const bodyEntries = [];       // { part, solid, mesh, base, explode }
  const pickables = [];
  const detailMeshes = [];

  function solidExplode(part, solid) {
    // Shell walls open outward instead of travelling with the shell as one lump: an exploded
    // view that hides the interior explains nothing.
    if (part.id === "shell" && solid.id.startsWith("shell-wall")) {
      const sx = Math.abs(solid.position[0]) > 0.01 ? Math.sign(solid.position[0]) : 0;
      const sz = Math.abs(solid.position[2]) > 0.01 ? Math.sign(solid.position[2]) : 0;
      return [sx * 1.35, 0, sz * 1.35];
    }
    if (part.id === "shell" && solid.id === "shell-deck") return [0, 0.55, 0];
    return part.explode;
  }

  for (const part of geometry.parts) {
    const group = new THREE.Group();
    group.userData.partId = part.id;
    stage.add(group);
    partGroups.set(part.id, group);

    for (const solid of part.solids) {
      const material = new THREE.MeshLambertMaterial({
        color: new THREE.Color(colourFor(part)),
        flatShading: true,
        transparent: solid.role === "detail" || part.id === "shell",
        opacity: solid.role === "detail" ? 0.6 : (part.id === "shell" ? 0.46 : 1),
        depthWrite: !(part.id === "shell") && solid.role !== "detail"
      });
      let mesh;
      if (solid.shape === "cylinder") {
        mesh = new THREE.Mesh(new THREE.CylinderGeometry(solid.radiusTop, solid.radiusBottom, solid.height, 28, 1, false), material);
      } else {
        mesh = new THREE.Mesh(new THREE.BoxGeometry(solid.size[0], solid.size[1], solid.size[2]), material);
      }
      mesh.position.set(solid.position[0], solid.position[1], solid.position[2]);
      mesh.rotation.set(solid.rotation[0], solid.rotation[1], solid.rotation[2]);
      mesh.userData.partId = part.id;
      mesh.userData.solidId = solid.id;
      group.add(mesh);

      const edges = new THREE.LineSegments(
        new THREE.EdgesGeometry(mesh.geometry, 28),
        new THREE.LineBasicMaterial({ color: LINE, transparent: true, opacity: solid.role === "detail" ? 0.35 : 0.75 })
      );
      mesh.add(edges);

      const record = {
        part, solid, mesh, material, edges,
        base: new THREE.Vector3(solid.position[0], solid.position[1], solid.position[2]),
        explode: new THREE.Vector3(...solidExplode(part, solid))
      };
      if (solid.role === "body" && part.pickable !== false) {
        pickable.push(mesh);
        bodyEntries.push(record);
      } else {
        /* Non-pickable bodies (round 2: context-only assemblies) are decoration for the pick
           list, exactly like a louver slat: drawn, never selectable. */
        detailMeshes.push(record);
      }
    }
  }

  /* ---- flow ribbons and marker pins --------------------------------- */

  const flowGroups = { air: new THREE.Group(), water: new THREE.Group() };
  for (const key of Object.keys(flowGroups)) scene.add(flowGroups[key]);

  function ribbon(points, colour, group, { head = true, radius = 0.16 } = {}) {
    const material = new THREE.MeshBasicMaterial({ color: colour, transparent: true, opacity: 0.9 });
    const up = new THREE.Vector3(0, 1, 0);
    for (let i = 0; i < points.length - 1; i += 1) {
      const a = new THREE.Vector3(...points[i]);
      const b = new THREE.Vector3(...points[i + 1]);
      const dir = new THREE.Vector3().subVectors(b, a);
      const length = dir.length();
      if (length < 1e-6) continue;
      const bar = new THREE.Mesh(new THREE.CylinderGeometry(radius, radius, length, 8, 1, false), material);
      bar.position.copy(a).addScaledVector(dir, 0.5);
      bar.quaternion.setFromUnitVectors(up, dir.clone().normalize());
      group.add(bar);
      if (head) {
        const cone = new THREE.Mesh(new THREE.ConeGeometry(radius * 2.6, radius * 5.4, 10), material);
        cone.position.copy(a).addScaledVector(dir, 0.82);
        cone.quaternion.setFromUnitVectors(up, dir.clone().normalize());
        group.add(cone);
      }
    }
  }

  for (const path of geometry.flows.air) ribbon(path, AIR, flowGroups.air, { radius: 0.14 });
  for (const path of geometry.flows.water) ribbon(path, WATER, flowGroups.water, { radius: 0.12 });

  /** Marker pins: the loss terms, and anything else the variant wants to point at. */
  const markerGroup = new THREE.Group();
  scene.add(markerGroup);

  function addMarker({ position, colour, size = 0.3 }) {
    const pin = new THREE.Mesh(
      new THREE.OctahedronGeometry(size, 0),
      new THREE.MeshBasicMaterial({ color: colour })
    );
    pin.position.set(position[0], position[1], position[2]);
    markerGroup.add(pin);
    return pin;
  }

  /* ---- the "differs" outline (comparison stance) --------------------- */

  /*
   * Punch-list item 3: two compared candidates can carry assemblies of the SAME material family
   * (both fills here are PP), so a tint cannot separate them. The differentiator is therefore
   * non-material: a dashed outline box drawn around each differing assembly — every differing
   * part gets one, at the same dash geometry every run, so a captured frame stays a settled
   * frame. The variants pair it with a Δ chip in the label layer and a Δ badge in the rail.
   */
  const DIFFERS_COLOUR = "#a94d0c";
  const differsGroup = new THREE.Group();
  scene.add(differsGroup);

  function setDiffers(ids = []) {
    for (const child of [...differsGroup.children]) {
      differsGroup.remove(child);
      child.geometry?.dispose();
      child.material?.dispose();
    }
    for (const id of ids) {
      const group = partGroups.get(id);
      if (!group) continue;
      const box = new THREE.Box3().setFromObject(group);
      const size = box.getSize(new THREE.Vector3());
      const centre = box.getCenter(new THREE.Vector3());
      const pad = geometry.extents.spanM * 0.014;
      const edges = new THREE.EdgesGeometry(new THREE.BoxGeometry(size.x + pad, size.y + pad, size.z + pad));
      const dash = Math.max(0.22, geometry.extents.spanM * 0.026);
      const material = new THREE.LineDashedMaterial({ color: DIFFERS_COLOUR, dashSize: dash, gapSize: dash * 0.6 });
      const outline = new THREE.LineSegments(edges, material);
      outline.position.copy(centre);
      outline.computeLineDistances();
      outline.userData.partId = id;
      differsGroup.add(outline);
    }
    requestRender();
  }

  /* ---- labels ------------------------------------------------------- */

  const labelEntries = [];
  const SVG_NS = "http://www.w3.org/2000/svg";
  const leaderLayer = document.createElementNS(SVG_NS, "svg");
  leaderLayer.setAttribute("class", "pp-leaders");
  leaderLayer.setAttribute("aria-hidden", "true");
  labelHost.appendChild(leaderLayer);

  function addLabel(entry) {
    const node = document.createElement("span");
    node.className = "pp-label";
    node.dataset.kind = entry.kind ?? "part";
    if (entry.state) node.dataset.state = entry.state;
    if (entry.differs) node.dataset.differs = "true";
    node.innerHTML = entry.html;
    labelHost.appendChild(node);
    const record = { ...entry, node, anchor: new THREE.Vector3(...entry.anchor) };
    labelEntries.push(record);
    return record;
  }

  /* ---- camera ------------------------------------------------------- */

  const home = {
    target: new THREE.Vector3(),
    /* Framing leaves a margin around the silhouette on purpose: the callout chips live in that
       margin, so the model is framed for its annotations, not to fill the canvas. The framing is
       measured on the EXPLODED assembly, so pulling the parts apart never pushes one out of shot. */
    radius: 20,
    theta: -0.72,
    phi: 1.06
  };

  function frameHome() {
    const min = new THREE.Vector3(Infinity, Infinity, Infinity);
    const max = new THREE.Vector3(-Infinity, -Infinity, -Infinity);
    const spread = [];
    for (const part of geometry.parts) {
      for (const solid of part.solids) {
        const half = solid.shape === "cylinder"
          ? [solid.radiusTop, solid.height / 2, solid.radiusTop]
          : [solid.size[0] / 2, solid.size[1] / 2, solid.size[2] / 2];
        const offset = part.explode;
        for (const sx of [-1, 1]) {
          for (const sy of [-1, 1]) {
            for (const sz of [-1, 1]) {
              spread.push(new THREE.Vector3(
                solid.position[0] + offset[0] + sx * half[0],
                solid.position[1] + offset[1] + sy * half[1],
                solid.position[2] + offset[2] + sz * half[2]
              ));
            }
          }
        }
      }
    }
    for (const point of spread) {
      min.min(point);
      max.max(point);
    }
    home.target.set(0, (min.y + max.y) / 2, 0);
    home.radius = Math.max(Math.max(max.y - min.y, max.x - min.x, max.z - min.z) * frameRadiusFactor, 13);
  }

  frameHome();
  const view = { ...home };
  const tween = { active: false, from: null, to: null, startedAt: 0, duration: 320 };

  function applyCamera() {
    view.phi = Math.max(0.18, Math.min(Math.PI / 2 - 0.02, view.phi));
    view.radius = Math.max(4, Math.min(320, view.radius));
    const sinPhi = Math.sin(view.phi);
    camera.position.set(
      view.target.x + view.radius * sinPhi * Math.sin(view.theta),
      view.target.y + view.radius * Math.cos(view.phi),
      view.target.z + view.radius * sinPhi * Math.cos(view.theta)
    );
    camera.lookAt(view.target);
  }

  function setView(next, { animate = true } = {}) {
    const to = {
      target: next.target ? next.target.clone() : view.target.clone(),
      radius: next.radius ?? view.radius,
      theta: next.theta ?? view.theta,
      phi: next.phi ?? view.phi
    };
    const changed = ["radius", "theta", "phi"].some((key) => Math.abs(to[key] - view[key]) > 1e-4)
      || !to.target.equals(view.target);
    if (!animate || reducedMotion || !changed) {
      view.target.copy(to.target);
      view.radius = to.radius;
      view.theta = to.theta;
      view.phi = to.phi;
      applyCamera();
      requestRender();
      return;
    }
    tween.active = true;
    tween.from = { target: view.target.clone(), radius: view.radius, theta: view.theta, phi: view.phi };
    tween.to = to;
    tween.startedAt = performance.now();
    requestRender();
  }

  /* ---- render loop (on demand) -------------------------------------- */

  let frameHandle = null;
  let pending = false;
  let settled = true;

  function renderFrame() {
    frameHandle = null;
    const now = performance.now();
    let animating = false;

    if (tween.active) {
      const t = Math.min(1, (now - tween.startedAt) / tween.duration);
      const eased = 1 - (1 - t) ** 3;
      view.target.lerpVectors(tween.from.target, tween.to.target, eased);
      view.radius = tween.from.radius + (tween.to.radius - tween.from.radius) * eased;
      view.theta = tween.from.theta + (tween.to.theta - tween.from.theta) * eased;
      view.phi = tween.from.phi + (tween.to.phi - tween.from.phi) * eased;
      applyCamera();
      if (t >= 1) tween.active = false;
      animating = true;
    }

    if (explodeTween.active) {
      const t = Math.min(1, (now - explodeTween.startedAt) / explodeTween.duration);
      const eased = 1 - (1 - t) ** 3;
      explodeFactor = explodeTween.from + (explodeTween.to - explodeTween.from) * eased;
      applyExplode();
      refreshMaterials();
      if (t >= 1) explodeTween.active = false;
      animating = true;
    }

    renderer.render(scene, camera);
    syncLabels();

    if (animating || pending) {
      pending = false;
      frameHandle = requestAnimationFrame(renderFrame);
      settled = false;
    } else {
      settled = true;
      stageReady = true;
      document.body.dataset.stage = "ready";
    }
  }

  function requestRender() {
    if (frameHandle === null) {
      settled = false;
      delete document.body.dataset.stage;
      frameHandle = requestAnimationFrame(renderFrame);
    }
  }

  const LABEL_MARGIN = 5;
  const labelPriority = (entry) => (entry.kind === "part" ? 0 : entry.kind === "loss" ? 1 : 2);
  /** Callout distance from the anchor, by kind: part names sit outside the silhouette, values
   *  closer in, so the two never fight for the same band. */
  const labelRadiusFactor = (entry) => (entry.kind === "part" ? 1 : entry.kind === "loss" ? 0.8 : 0.62);
  const labelCentre = new THREE.Vector3(0, geometry.extents.maxY * 0.5, 0);

  function boxesOverlap(a, b, slack = 4) {
    return Math.abs(a.x - b.x) * 2 < (a.w + b.w - slack) && Math.abs(a.y - b.y) * 2 < (a.h + b.h - slack);
  }

  /**
   * Label placement pass.
   *
   * Each chip wants to sit at its anchor plus a small offset, but a chip that runs off the stage
   * or lands on another chip is a defect — not a style choice. So every chip is clamped into the
   * frame and then nudged (up/down/sideways, nearest-first) until it is clear, in priority order:
   * part names stay put, pressure terms move, zone chips move furthest. Whether a chip had to be
   * moved is recorded per chip and asserted by tools/capture.mjs, so clutter is measured rather
   * than eyeballed.
   */
  function syncLabels() {
    const width = renderer.domElement.clientWidth;
    const height = renderer.domElement.clientHeight;
    const visible = [];

    for (const entry of labelEntries) {
      if (!entry.visible) {
        entry.node.style.display = "none";
        entry.clipped = false;
        entry.screen = null;
        continue;
      }
      const projected = entry.anchor.clone().project(camera);
      if (projected.z > 1) {
        entry.node.style.display = "none";
        entry.clipped = false;
        entry.screen = null;
        continue;
      }
      entry.node.style.display = "";
      const w = entry.node.offsetWidth || 0;
      const h = entry.node.offsetHeight || 0;
      const anchorX = (projected.x * 0.5 + 0.5) * width;
      const anchorY = (-projected.y * 0.5 + 0.5) * height;
      /* Callouts are placed radially outward from the model centre: a chip then always reads as
         belonging to the pin it sits next to, instead of floating on a hand-tuned offset that
         only works at one window size. */
      const centreProjected = labelCentre.clone().project(camera);
      const centreX = (centreProjected.x * 0.5 + 0.5) * width;
      const centreY = (-centreProjected.y * 0.5 + 0.5) * height;
      let ux = anchorX - centreX;
      let uy = anchorY - centreY;
      const length = Math.hypot(ux, uy) || 1;
      ux /= length;
      uy /= length;
      /* Chips anchored near the model's vertical axis would all stack in one column, because
         "outward" is the same direction for every one of them. Bias those sideways, alternating
         left and right, so the callouts read as two annotated columns instead of a pile. */
      if (Math.abs(ux) < 0.34) {
        const side = visible.length % 2 === 0 ? -1 : 1;
        ux = side * 0.92;
        uy = uy * 0.4;
      }
      const radius = Math.max(16, Math.min(36, width * 0.05)) * labelRadiusFactor(entry);
      entry.desired = {
        x: anchorX + ux * radius + (entry.dx ?? 0),
        y: anchorY + uy * radius + (entry.dy ?? 0),
        w, h
      };
      entry.anchorScreen = { x: anchorX, y: anchorY };
      entry.offFrame = entry.desired.x - w / 2 < 0 || entry.desired.x + w / 2 > width
        || entry.desired.y - h / 2 < 0 || entry.desired.y + h / 2 > height;
      visible.push(entry);
    }

    visible.sort((a, b) => labelPriority(a) - labelPriority(b));
    const placed = [];
    const leaderNodes = [];
    for (const entry of visible) {
      const { w, h } = entry.desired;
      const clampX = (x) => Math.min(Math.max(x, w / 2 + LABEL_MARGIN), Math.max(w / 2 + LABEL_MARGIN, width - w / 2 - LABEL_MARGIN));
      const clampY = (y) => Math.min(Math.max(y, h / 2 + LABEL_MARGIN), Math.max(h / 2 + LABEL_MARGIN, height - h / 2 - LABEL_MARGIN));
      let x = clampX(entry.desired.x);
      let y = clampY(entry.desired.y);
      let nudged = Math.abs(x - entry.desired.x) > 0.5 || Math.abs(y - entry.desired.y) > 0.5;

      const step = Math.max(8, h + 4);
      const candidates = [
        [0, -step], [0, step], [-step, 0], [step, 0],
        [-step, -step], [step, -step], [-step, step], [step, step],
        [0, -2 * step], [0, 2 * step], [-2 * step, 0], [2 * step, 0],
        [-2 * step, -step], [2 * step, -step], [-2 * step, step], [2 * step, step]
      ];
      for (let attempt = 0; attempt < 8 && placed.some((other) => boxesOverlap({ x, y, w, h }, other)); attempt += 1) {
        const away = y < height / 2 ? -1 : 1;
        const shuffled = candidates.slice().sort((a, b) => {
          const bias = (item) => (item[1] === away * step ? -1 : 0) + Math.hypot(item[0], item[1]) / (step * 4);
          return bias(a) - bias(b);
        });
        let settled = false;
        for (const [dx, dy] of shuffled) {
          const nx = clampX(x + dx);
          const ny = clampY(y + dy);
          if (!placed.some((other) => boxesOverlap({ x: nx, y: ny, w, h }, other))) {
            x = nx;
            y = ny;
            nudged = true;
            settled = true;
            break;
          }
        }
        if (!settled) {
          /* Nothing clear nearby: fall back to sliding out along the axis with room to spare. */
          const room = y > height / 2 ? -1 : 1;
          y = clampY(y + room * step * (attempt + 1));
          nudged = true;
        }
      }

      placed.push({ x, y, w, h, id: entry.id });
      entry.nudged = nudged;
      entry.screen = { x, y, w, h };
      entry.node.style.transform = `translate(${x}px, ${y}px) translate(-50%, -50%)`;

      /* A leader line from the chip to the pin it belongs to. Without it a callout placed clear
         of the geometry is only guesswork for the reader. */
      const anchor = entry.anchorScreen;
      const distance = Math.hypot(anchor.x - x, anchor.y - y);
      if (distance > 26) {
        const dx = anchor.x - x;
        const dy = anchor.y - y;
        const scale = Math.min((w / 2) / (Math.abs(dx) || 1e-6), (h / 2) / (Math.abs(dy) || 1e-6));
        const line = document.createElementNS(SVG_NS, "line");
        line.setAttribute("x1", (x + dx * scale).toFixed(1));
        line.setAttribute("y1", (y + dy * scale).toFixed(1));
        line.setAttribute("x2", anchor.x.toFixed(1));
        line.setAttribute("y2", anchor.y.toFixed(1));
        line.setAttribute("data-kind", entry.kind ?? "part");
        leaderNodes.push(line);
        const dot = document.createElementNS(SVG_NS, "circle");
        dot.setAttribute("cx", anchor.x.toFixed(1));
        dot.setAttribute("cy", anchor.y.toFixed(1));
        dot.setAttribute("r", "2.4");
        dot.setAttribute("data-kind", entry.kind ?? "part");
        leaderNodes.push(dot);
      }
    }
    leaderLayer.setAttribute("viewBox", `0 0 ${Math.max(1, width)} ${Math.max(1, height)}`);
    leaderLayer.replaceChildren(...leaderNodes);
  }

  /* ---- explode ------------------------------------------------------ */

  let explodeFactor = 1;
  const explodeTween = { active: false, from: 1, to: 1, startedAt: 0, duration: reducedMotion ? 0 : 360 };

  function applyExplode() {
    for (const entry of bodyEntries) {
      entry.mesh.position.copy(entry.base).addScaledVector(entry.explode, explodeFactor);
    }
    for (const entry of detailMeshes) {
      entry.mesh.position.copy(entry.base).addScaledVector(entry.explode, explodeFactor);
    }
    syncLabels();
  }

  function setExplode(exploded, { animate = true } = {}) {
    const to = exploded ? 1 : 0;
    if (!animate || reducedMotion || to === explodeFactor) {
      explodeFactor = to;
      explodeTween.active = false;
      applyExplode();
      refreshMaterials();
      requestRender();
      return;
    }
    explodeTween.active = true;
    explodeTween.from = explodeFactor;
    explodeTween.to = to;
    explodeTween.startedAt = performance.now();
    requestRender();
  }

  /* ---- selection state ---------------------------------------------- */

  let selectedId = null;
  let hoverId = null;
  let highlightIds = new Set();

  function refreshMaterials() {
    for (const entry of bodyEntries) {
      const part = entry.part;
      const isSelected = part.id === selectedId;
      const isHover = part.id === hoverId;
      const hasHighlight = highlightIds.size > 0;
      const isHighlighted = highlightIds.has(part.id);
      const base = new THREE.Color(colourFor(part));
      /* Exploding the assembly pulls the shell casing back so the separated parts read. */
      const shellOpacity = 0.46 - 0.18 * explodeFactor;
      if ((selectedId && !isSelected) || (hasHighlight && !isHighlighted)) {
        base.lerp(new THREE.Color("#c3ced3"), 0.55);
        entry.material.opacity = part.id === "shell" ? shellOpacity * 0.4 : 0.5;
        entry.material.transparent = true;
        entry.material.depthWrite = false;
      } else {
        entry.material.opacity = part.id === "shell" ? shellOpacity : (entry.solid.role === "detail" ? 0.6 : 1);
        entry.material.transparent = entry.solid.role === "detail" || part.id === "shell";
        entry.material.depthWrite = !(part.id === "shell") && entry.solid.role !== "detail";
      }
      entry.material.color.copy(base);
      const emphasised = isSelected || (hasHighlight && isHighlighted);
      entry.material.emissive = new THREE.Color(emphasised ? emphasisColour : "#000000");
      entry.material.emissiveIntensity = emphasised ? (isSelected ? 0.24 : 0.16) : (isHover ? 0.12 : 0);
      entry.edges.material.color = new THREE.Color(emphasised ? (emphasisColour === "#c77a1b" ? "#a45f0d" : INK) : LINE);
      entry.edges.material.opacity = ((selectedId && !isSelected) || (hasHighlight && !isHighlighted))
        ? 0.25
        : (entry.solid.role === "detail" ? 0.35 : (emphasised ? 1 : 0.75));
    }
    for (const entry of detailMeshes) {
      entry.material.color.copy(new THREE.Color(colourFor(entry.part)));
      const dimmed = (selectedId && entry.part.id !== selectedId) || (highlightIds.size > 0 && !highlightIds.has(entry.part.id));
      if (dimmed) {
        entry.material.opacity = 0.24;
        entry.material.transparent = true;
      } else {
        entry.material.opacity = 0.6;
      }
    }
    requestRender();
  }

  /** Emphasise a set of part ids and push everything else back (used by the comparison view). */
  function setHighlight(ids) {
    highlightIds = new Set(ids ?? []);
    refreshMaterials();
  }

  function select(partId) {
    selectedId = partId;
    refreshMaterials();
    requestRender();
  }

  function framePart(partId, { animate = true } = {}) {
    const group = partGroups.get(partId);
    if (!group) return;
    const box = new THREE.Box3().setFromObject(group);
    const centre = box.getCenter(new THREE.Vector3());
    const size = box.getSize(new THREE.Vector3()).length();
    setView({
      target: centre,
      radius: Math.max(size * 1.5, geometry.extents.spanM * 0.7)
    }, { animate });
  }

  function resetView() {
    setView(home);
  }

  /* ---- pointer + keyboard ------------------------------------------ */

  let dragging = null;
  let moved = 0;

  canvas.addEventListener("pointerdown", (event) => {
    canvas.setPointerCapture(event.pointerId);
    dragging = { x: event.clientX, y: event.clientY, pan: event.shiftKey || event.button === 2 };
    moved = 0;
  });

  canvas.addEventListener("pointermove", (event) => {
    if (dragging) {
      const dx = event.clientX - dragging.x;
      const dy = event.clientY - dragging.y;
      moved += Math.abs(dx) + Math.abs(dy);
      dragging.x = event.clientX;
      dragging.y = event.clientY;
      if (dragging.pan) {
        const scale = view.radius * 0.0016;
        const right = new THREE.Vector3().setFromMatrixColumn(camera.matrix, 0);
        const up = new THREE.Vector3().setFromMatrixColumn(camera.matrix, 1);
        view.target.addScaledVector(right, -dx * scale).addScaledVector(up, dy * scale);
      } else {
        view.theta -= dx * 0.0075;
        view.phi -= dy * 0.0065;
      }
      applyCamera();
      requestRender();
      return;
    }
    const hit = pickAt(event);
    const id = hit ? hit.userData.partId : null;
    if (id !== hoverId) {
      hoverId = id;
      canvas.style.cursor = id ? "pointer" : "grab";
      refreshMaterials();
      onHover(id);
    }
  });

  function endDrag(event) {
    if (!dragging) return;
    const wasClick = moved < 5;
    dragging = null;
    if (wasClick) {
      const hit = pickAt(event);
      onPick(hit ? hit.userData.partId : null);
    }
  }
  canvas.addEventListener("pointerup", endDrag);
  canvas.addEventListener("pointercancel", () => { dragging = null; });
  canvas.addEventListener("contextmenu", (event) => event.preventDefault());

  canvas.addEventListener("wheel", (event) => {
    if (!event.ctrlKey) return;
    event.preventDefault();
    view.radius *= 1 + Math.sign(event.deltaY) * 0.08;
    applyCamera();
    requestRender();
  }, { passive: false });

  canvas.addEventListener("keydown", (event) => {
    const step = event.shiftKey ? 0.12 : 0.05;
    switch (event.key) {
      case "ArrowLeft": view.theta -= step; break;
      case "ArrowRight": view.theta += step; break;
      case "ArrowUp": view.phi -= step * 0.7; break;
      case "ArrowDown": view.phi += step * 0.7; break;
      case "+":
      case "=": view.radius *= 0.9; break;
      case "-":
      case "_": view.radius *= 1.1; break;
      case "r":
      case "R": resetView(); return;
      default: return;
    }
    event.preventDefault();
    applyCamera();
    requestRender();
  });

  const raycaster = new THREE.Raycaster();
  const pointer = new THREE.Vector2();

  /** Raycast at a point given in CSS pixels relative to the canvas — the pick list, and only it. */
  function pickAtPoint(x, y) {
    const rect = canvas.getBoundingClientRect();
    pointer.x = (x / rect.width) * 2 - 1;
    pointer.y = -(y / rect.height) * 2 + 1;
    raycaster.setFromCamera(pointer, camera);
    const hits = raycaster.intersectObjects(pickable, false);
    return hits.length ? hits[0].object : null;
  }

  function pickAt(event) {
    const rect = canvas.getBoundingClientRect();
    return pickAtPoint(event.clientX - rect.left, event.clientY - rect.top);
  }

  /* ---- sizing ------------------------------------------------------- */

  function resize() {
    const width = canvas.clientWidth || 640;
    const height = canvas.clientHeight || 420;
    if (canvas.width === Math.floor(width * renderer.getPixelRatio()) && canvas.height === Math.floor(height * renderer.getPixelRatio())) return;
    renderer.setSize(width, height, false);
    camera.aspect = width / height;
    camera.updateProjectionMatrix();
    requestRender();
  }

  const observer = new ResizeObserver(resize);
  observer.observe(canvas);

  let stageReady = false;
  canvas.addEventListener("webglcontextlost", (event) => {
    event.preventDefault();
    options.onContextLost?.("the WebGL context was lost");
  });

  applyCamera();
  resize();
  applyExplode();
  requestRender();

  return {
    renderer,
    scene,
    camera,
    setExplode,
    setView,
    resetView,
    select,
    setHighlight,
    setDiffers,
    differsCount: () => differsGroup.children.length,
    framePart,
    pickAtPoint,
    applyExplode,
    refreshMaterials,
    requestRender,
    addLabel,
    addMarker,
    labelEntries,
    markerGroup,
    flowGroups,
    pickable,
    /** The pickable assemblies by identity, as opposed to the meshes that draw them. */
    pickableParts: () => [...new Set(pickable.map((mesh) => mesh.userData.partId))],
    meshCounts: () => ({ bodies: pickable.length, details: detailMeshes.length }),
    dispose() {
      observer.disconnect();
      disposeTree(scene);
      renderer.dispose();
      for (const entry of labelEntries) entry.node.remove();
    },
    /** Settled = no tween or render in flight; the capture harness waits on this. */
    isSettled: () => settled,
    isReady: () => stageReady,
    stageWidth: () => canvas.clientWidth,
    stageBounds: () => ({ w: canvas.clientWidth, h: canvas.clientHeight, host: canvas.parentElement?.id ?? "" }),
    /** Label geometry for the clutter probe: every chip's box inside the stage. */
    labelBoxes: () => labelEntries.filter((entry) => entry.visible && entry.screen).map((entry) => ({
      id: entry.id, nudged: Boolean(entry.nudged), offFrame: Boolean(entry.offFrame),
      x: Math.round(entry.screen.x), y: Math.round(entry.screen.y),
      w: entry.screen.w, h: entry.screen.h,
      anchor: entry.anchor.toArray().map((value) => (Number.isFinite(value) ? Number(value.toFixed(3)) : null))
    })),
    state: () => ({ selectedId, hoverId, highlighted: [...highlightIds], explode: explodeFactor, view: { ...view, target: view.target.toArray() } })
  };
}
