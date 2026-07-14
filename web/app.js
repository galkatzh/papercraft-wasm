// Papercraft Unfolder — vanilla JS front-end (no build step).
import * as THREE from 'three';
import { OrbitControls } from './vendor/three/OrbitControls.js';
import init, { PaperDoc } from './wasm/papercraft_wasm.js';

const EDGE_COLORS = {
  cut:      [0.95, 0.35, 0.35],
  mountain: [0.36, 0.55, 1.0],
  valley:   [0.24, 0.86, 0.52],
  flat:     [0.30, 0.32, 0.38],
};

const els = {
  status: document.getElementById('status'),
  file: document.getElementById('file'),
  unwrap: document.getElementById('unwrap'),
  repack: document.getElementById('repack'),
  simplify: document.getElementById('simplify'),
  simpDialog: document.getElementById('simplify-dialog'),
  simpCurrent: document.getElementById('simp-current'),
  simpTarget: document.getElementById('simp-target'),
  simpSlider: document.getElementById('simp-slider'),
  pdf: document.getElementById('pdf'),
  svg: document.getElementById('svg'),
  craft: document.getElementById('craft'),
  modeSplit: document.getElementById('mode-split'),
  modeJoin: document.getElementById('mode-join'),
};

let doc = null;       // PaperDoc
let mode = 'split';   // 'split' | 'join'
let modelName = 'model';

// ---------------------------------------------------------------------------
// 3D view
// ---------------------------------------------------------------------------
const canvas3d = document.getElementById('c3d');
const renderer = new THREE.WebGLRenderer({ canvas: canvas3d, antialias: true });
const scene = new THREE.Scene();
scene.background = new THREE.Color(0x15161a);
const camera = new THREE.PerspectiveCamera(45, 1, 0.01, 5000);
camera.position.set(0, 0, 5);
const controls = new OrbitControls(camera, canvas3d);
controls.enableDamping = true;
scene.add(new THREE.AmbientLight(0xffffff, 0.6));
const key = new THREE.DirectionalLight(0xffffff, 0.9);
key.position.set(1, 1, 1);
scene.add(key);
const rim = new THREE.DirectionalLight(0xffffff, 0.4);
rim.position.set(-1, -0.5, -1);
scene.add(rim);

let meshObj = null;
let edgeLines = null;
let edgeSegIds = [];   // edge id per line segment
const raycaster = new THREE.Raycaster();

function resize3d() {
  const w = canvas3d.clientWidth, h = canvas3d.clientHeight;
  if (w === 0 || h === 0) return;
  renderer.setPixelRatio(window.devicePixelRatio);
  renderer.setSize(w, h, false);
  camera.aspect = w / h;
  camera.updateProjectionMatrix();
}

function build3d(model) {
  if (meshObj) { scene.remove(meshObj); meshObj.geometry.dispose(); meshObj.material.dispose(); }
  if (edgeLines) { scene.remove(edgeLines); edgeLines.geometry.dispose(); edgeLines.material.dispose(); }

  const pos = new Float32Array(model.positions.flat());
  const nrm = new Float32Array(model.normals.flat());
  const geo = new THREE.BufferGeometry();
  geo.setAttribute('position', new THREE.BufferAttribute(pos, 3));
  geo.setAttribute('normal', new THREE.BufferAttribute(nrm, 3));
  geo.setIndex(model.indices);
  const mat = new THREE.MeshStandardMaterial({
    color: 0xc9ccd4, roughness: 0.85, metalness: 0.0, side: THREE.DoubleSide,
  });
  meshObj = new THREE.Mesh(geo, mat);
  scene.add(meshObj);

  // Edge overlay coloured by fold/cut status, used for picking.
  const ep = [], ec = [];
  edgeSegIds = [];
  for (const e of model.edges) {
    ep.push(e.p0[0], e.p0[1], e.p0[2], e.p1[0], e.p1[1], e.p1[2]);
    const c = EDGE_COLORS[e.kind] || EDGE_COLORS.flat;
    ec.push(c[0], c[1], c[2], c[0], c[1], c[2]);
    edgeSegIds.push(e.edge);
  }
  const egeo = new THREE.BufferGeometry();
  egeo.setAttribute('position', new THREE.BufferAttribute(new Float32Array(ep), 3));
  egeo.setAttribute('color', new THREE.BufferAttribute(new Float32Array(ec), 3));
  edgeLines = new THREE.LineSegments(egeo, new THREE.LineBasicMaterial({ vertexColors: true }));
  scene.add(edgeLines);

  // Frame the model.
  geo.computeBoundingSphere();
  const bs = geo.boundingSphere;
  controls.target.copy(bs.center);
  const d = bs.radius * 2.6;
  camera.position.set(bs.center.x + d * 0.4, bs.center.y + d * 0.3, bs.center.z + d);
  camera.near = bs.radius / 100;
  camera.far = bs.radius * 100;
  camera.updateProjectionMatrix();
  controls.update();
  raycaster.params.Line.threshold = bs.radius * 0.02;
}

canvas3d.addEventListener('pointerdown', (ev) => {
  if (!doc || !edgeLines || ev.button !== 0) return;
  // Distinguish click from orbit-drag.
  const sx = ev.clientX, sy = ev.clientY;
  const up = (e2) => {
    canvas3d.removeEventListener('pointerup', up);
    if (Math.hypot(e2.clientX - sx, e2.clientY - sy) > 4) return;
    pick3d(e2);
  };
  canvas3d.addEventListener('pointerup', up);
});

function pick3d(ev) {
  const r = canvas3d.getBoundingClientRect();
  const ndc = new THREE.Vector2(
    ((ev.clientX - r.left) / r.width) * 2 - 1,
    -((ev.clientY - r.top) / r.height) * 2 + 1,
  );
  raycaster.setFromCamera(ndc, camera);
  const hits = raycaster.intersectObject(edgeLines, false);
  if (hits.length) {
    const seg = Math.floor(hits[0].index / 2);
    toggleEdge(edgeSegIds[seg]);
  }
}

function animate() {
  requestAnimationFrame(animate);
  controls.update();
  renderer.render(scene, camera);
}

// ---------------------------------------------------------------------------
// 2D net view
// ---------------------------------------------------------------------------
const canvas2d = document.getElementById('c2d');
const ctx = canvas2d.getContext('2d');
let net = null;                 // pieces2d result
let view2d = { scale: 1, x: 0, y: 0 };  // world(mm)->screen: s*world + offset
let fit2d = null;               // base fit transform

function resize2d() {
  const dpr = window.devicePixelRatio;
  const w = canvas2d.clientWidth, h = canvas2d.clientHeight;
  canvas2d.width = w * dpr; canvas2d.height = h * dpr;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  draw2d();
}

function bbox2d(net) {
  let minx = Infinity, miny = Infinity, maxx = -Infinity, maxy = -Infinity;
  const acc = (p) => { minx = Math.min(minx, p[0]); miny = Math.min(miny, p[1]);
                       maxx = Math.max(maxx, p[0]); maxy = Math.max(maxy, p[1]); };
  for (const pc of net.pieces) {
    for (const t of pc.triangles) t.forEach(acc);
    for (const f of pc.flaps) f.forEach(acc);
  }
  if (minx === Infinity) return { minx: 0, miny: 0, maxx: 1, maxy: 1 };
  return { minx, miny, maxx, maxy };
}

function fitNet() {
  if (!net) return;
  const b = bbox2d(net);
  const w = canvas2d.clientWidth, h = canvas2d.clientHeight;
  const m = 24;
  const s = Math.min((w - 2 * m) / (b.maxx - b.minx || 1), (h - 2 * m) / (b.maxy - b.miny || 1));
  fit2d = { scale: s,
    x: m - b.minx * s + (w - 2 * m - (b.maxx - b.minx) * s) / 2,
    y: m - b.miny * s + (h - 2 * m - (b.maxy - b.miny) * s) / 2 };
  view2d = { ...fit2d };
}

const W2S = (p) => [p[0] * view2d.scale + view2d.x, p[1] * view2d.scale + view2d.y];
const S2W = (sx, sy) => [(sx - view2d.x) / view2d.scale, (sy - view2d.y) / view2d.scale];

function strokePoly(pts, close) {
  ctx.beginPath();
  pts.forEach((p, i) => { const [x, y] = W2S(p); i ? ctx.lineTo(x, y) : ctx.moveTo(x, y); });
  if (close) ctx.closePath();
  ctx.stroke();
}
function line(a, b) { const [x0, y0] = W2S(a), [x1, y1] = W2S(b);
  ctx.beginPath(); ctx.moveTo(x0, y0); ctx.lineTo(x1, y1); ctx.stroke(); }

function draw2d() {
  const w = canvas2d.clientWidth, h = canvas2d.clientHeight;
  ctx.clearRect(0, 0, w, h);
  if (!net) return;
  // faint face fill
  ctx.fillStyle = 'rgba(255,255,255,0.05)';
  for (const pc of net.pieces) for (const t of pc.triangles) strokeTriFill(t);
  // flaps
  ctx.lineWidth = 1; ctx.setLineDash([]);
  ctx.strokeStyle = 'rgba(210,180,120,0.9)';
  for (const pc of net.pieces) for (const f of pc.flaps) strokePoly(f, false);
  // folds
  for (const pc of net.pieces) for (const l of pc.folds) {
    if (l.kind === 'valley') { ctx.setLineDash([4, 3]); ctx.strokeStyle = 'rgba(60,220,130,0.95)'; }
    else { ctx.setLineDash([]); ctx.strokeStyle = 'rgba(92,140,255,0.95)'; }
    line(l.p0, l.p1);
  }
  // cuts
  ctx.setLineDash([]); ctx.lineWidth = 1.4; ctx.strokeStyle = 'rgba(240,240,245,0.95)';
  for (const pc of net.pieces) for (const l of pc.cuts) line(l.p0, l.p1);
  // edge-id labels
  ctx.fillStyle = 'rgba(235,190,110,0.95)';
  ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
  for (const pc of net.pieces) for (const t of pc.texts) {
    const px = t.size * view2d.scale;          // font height in screen px
    if (px < 5) continue;                       // hide when too small to read
    const [x, y] = W2S(t.pos);
    ctx.save();
    ctx.translate(x, y); ctx.rotate(t.angle);
    ctx.font = `${px}px Helvetica, sans-serif`;
    ctx.fillText(t.text, 0, 0);
    ctx.restore();
  }
}
function strokeTriFill(t) {
  ctx.beginPath();
  t.forEach((p, i) => { const [x, y] = W2S(p); i ? ctx.lineTo(x, y) : ctx.moveTo(x, y); });
  ctx.closePath(); ctx.fill();
}

// pan / zoom
canvas2d.addEventListener('wheel', (ev) => {
  ev.preventDefault();
  const r = canvas2d.getBoundingClientRect();
  const mx = ev.clientX - r.left, my = ev.clientY - r.top;
  const [wx, wy] = S2W(mx, my);
  const k = Math.exp(-ev.deltaY * 0.0015);
  view2d.scale *= k;
  view2d.x = mx - wx * view2d.scale;
  view2d.y = my - wy * view2d.scale;
  draw2d();
}, { passive: false });

let drag2d = null;
canvas2d.addEventListener('pointerdown', (ev) => {
  drag2d = { x: ev.clientX, y: ev.clientY, ox: view2d.x, oy: view2d.y, moved: false };
  canvas2d.setPointerCapture(ev.pointerId);
});
canvas2d.addEventListener('pointermove', (ev) => {
  if (!drag2d) return;
  const dx = ev.clientX - drag2d.x, dy = ev.clientY - drag2d.y;
  if (Math.hypot(dx, dy) > 3) drag2d.moved = true;
  view2d.x = drag2d.ox + dx; view2d.y = drag2d.oy + dy;
  draw2d();
});
canvas2d.addEventListener('pointerup', (ev) => {
  const d = drag2d; drag2d = null;
  if (!d || d.moved || !net) return;
  const r = canvas2d.getBoundingClientRect();
  pick2d(ev.clientX - r.left, ev.clientY - r.top);
});

function distToSeg(px, py, a, b) {
  const ax = a[0], ay = a[1], bx = b[0], by = b[1];
  const dx = bx - ax, dy = by - ay;
  const len2 = dx * dx + dy * dy || 1e-9;
  let t = ((px - ax) * dx + (py - ay) * dy) / len2;
  t = Math.max(0, Math.min(1, t));
  return Math.hypot(px - (ax + t * dx), py - (ay + t * dy));
}

function pick2d(sx, sy) {
  const [wx, wy] = S2W(sx, sy);
  const tol = 6 / view2d.scale;   // 6px in world units
  let best = null, bestD = tol;
  for (const pc of net.pieces) {
    for (const l of [...pc.cuts, ...pc.folds]) {
      const d = distToSeg(wx, wy, l.p0, l.p1);
      if (d < bestD) { bestD = d; best = l.edge; }
    }
  }
  if (best != null) toggleEdge(best);
}

// ---------------------------------------------------------------------------
// Document operations
// ---------------------------------------------------------------------------
function toggleEdge(id) {
  if (!doc) return;
  const changed = mode === 'split' ? doc.split_edge(id) : doc.join_edge(id);
  if (changed) refresh(false);
}

const LARGE_FACES = 50000;   // beyond this, single-threaded unfolding gets slow

function refresh(refit) {
  if (!doc) return;
  build3d(doc.model3d());
  net = doc.pieces2d();
  if (refit || !fit2d) fitNet();
  draw2d();
  const s = doc.stats();
  let msg = `${modelName} · ${s.faces.toLocaleString()} faces · ${s.pieces} piece(s)`;
  const warns = [];
  if (s.boundary_edges > 0) warns.push(`open mesh (${s.boundary_edges} boundary edge${s.boundary_edges > 1 ? 's' : ''})`);
  if (s.faces > LARGE_FACES) warns.push('large mesh — use Simplify… to decimate');
  setStatus(warns.length ? `${msg}  ⚠ ${warns.join(' · ')}` : msg);
  for (const b of [els.unwrap, els.repack, els.pdf, els.svg, els.craft, els.modeSplit, els.modeJoin])
    b.disabled = false;
  els.simplify.disabled = s.faces < 8;   // nothing sensible to simplify below that
}

function setStatus(s) { els.status.textContent = s; }

function download(bytes, name, mime) {
  const blob = new Blob([bytes], { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url; a.download = name; document.body.appendChild(a); a.click(); a.remove();
  URL.revokeObjectURL(url);
}

async function loadFile(file) {
  const ext = (file.name.split('.').pop() || '').toLowerCase();
  const bytes = new Uint8Array(await file.arrayBuffer());
  modelName = file.name;
  try {
    setStatus(`Importing ${file.name}…`);
    doc = new PaperDoc(bytes, ext);
    if (ext !== 'craft') {
      const s = doc.stats();
      if (s.faces > LARGE_FACES) setStatus(`Unfolding ${s.faces.toLocaleString()} faces — this may take a moment…`);
      // Yield so the status paints before the (single-threaded) unwrap.
      await new Promise((r) => setTimeout(r, 16));
      doc.unwrap();
    }
    refresh(true);
  } catch (e) {
    doc = null;
    setStatus(`Could not open ${file.name}: ${e.message || e} — reload the page and try another file.`);
    console.error(e);
  }
}

// ---------------------------------------------------------------------------
// Mesh simplification
// ---------------------------------------------------------------------------
function openSimplifyDialog() {
  const faces = doc.stats().faces;
  els.simpCurrent.textContent = faces.toLocaleString();
  els.simpTarget.max = faces - 1;
  // Default to half the faces, or the "large mesh" threshold if way over it.
  const def = Math.max(4, Math.min(Math.round(faces / 2), LARGE_FACES));
  els.simpTarget.value = def;
  els.simpSlider.value = Math.max(1, Math.min(99, Math.round((def / faces) * 100)));
  els.simpDialog.showModal();
}

els.simpSlider.addEventListener('input', () => {
  const faces = doc.stats().faces;
  els.simpTarget.value = Math.max(4, Math.round((faces * els.simpSlider.value) / 100));
});
els.simpTarget.addEventListener('input', () => {
  const faces = doc.stats().faces;
  const t = Number(els.simpTarget.value) || 0;
  els.simpSlider.value = Math.max(1, Math.min(99, Math.round((t / faces) * 100)));
});

async function runSimplify(target) {
  const before = doc.stats().faces;
  setStatus(`Simplifying ${before.toLocaleString()} → ~${target.toLocaleString()} faces…`);
  await new Promise((r) => setTimeout(r, 16));   // let the status paint
  try {
    const after = doc.simplify(target);
    doc.unwrap();
    refresh(true);
    setStatus(`${els.status.textContent}  ·  simplified ${before.toLocaleString()} → ${after.toLocaleString()} faces`);
  } catch (e) {
    // The document is left untouched on failure.
    refresh(false);
    setStatus(`Could not simplify: ${e.message || e}`);
    console.error(e);
  }
}

els.simplify.addEventListener('click', () => { if (doc) openSimplifyDialog(); });
els.simpDialog.addEventListener('close', () => {
  if (els.simpDialog.returnValue !== 'ok' || !doc) return;
  const faces = doc.stats().faces;
  const target = Math.round(Number(els.simpTarget.value));
  if (!Number.isFinite(target) || target < 4 || target >= faces) {
    setStatus(`Simplify: target must be between 4 and ${faces - 1} faces.`);
    return;
  }
  runSimplify(target);
});

// ---------------------------------------------------------------------------
// Wire up UI
// ---------------------------------------------------------------------------
els.file.addEventListener('change', (e) => { if (e.target.files[0]) loadFile(e.target.files[0]); });
els.unwrap.addEventListener('click', () => { doc.unwrap(); refresh(true); });
els.repack.addEventListener('click', () => { doc.pack_islands(); refresh(true); });
els.pdf.addEventListener('click', () => download(doc.export_pdf(), baseName() + '.pdf', 'application/pdf'));
els.svg.addEventListener('click', () => download(doc.export_svg(), baseName() + '.svg', 'image/svg+xml'));
els.craft.addEventListener('click', () => download(doc.save_craft(), baseName() + '.craft', 'application/zip'));
els.modeSplit.addEventListener('click', () => setMode('split'));
els.modeJoin.addEventListener('click', () => setMode('join'));

function baseName() { return modelName.replace(/\.[^.]+$/, '') || 'papercraft'; }
function setMode(m) {
  mode = m;
  els.modeSplit.classList.toggle('active', m === 'split');
  els.modeJoin.classList.toggle('active', m === 'join');
}

window.addEventListener('resize', () => { resize3d(); resize2d(); });

(async function main() {
  await init();
  resize3d();
  resize2d();
  animate();
  setStatus('Ready — open an STL, OBJ, PDO, glTF or .craft file.');
})();
