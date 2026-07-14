// Smoke test for the papercraft-wasm bindings.
//
// Build the Node target first, then run against an example model:
//
//   wasm-pack build wasm --target nodejs --out-dir /tmp/pcwasm-node --dev
//   node wasm/tests/smoke.mjs /tmp/pcwasm-node examples/die.craft craft
//   node wasm/tests/smoke.mjs /tmp/pcwasm-node /tmp/tet.stl stl
//
// Args: <pkgDir> <modelPath> <format>
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';

const [pkgDir, modelPath, format] = process.argv.slice(2);
if (!pkgDir || !modelPath || !format) {
  console.error('usage: node smoke.mjs <pkgDir> <modelPath> <format>');
  process.exit(2);
}

const pc = await import(
  pathToFileURL(resolve(pkgDir, 'papercraft_wasm.js')).href
);

const bytes = new Uint8Array(readFileSync(modelPath));
const doc = new pc.PaperDoc(bytes, format);

const sum = (pieces, key) => pieces.reduce((a, p) => a + p[key].length, 0);

console.log('imported islands :', doc.num_islands());

// Raw geometry comes in cut; .craft already carries its unfolding.
if (format !== 'craft') {
  console.log('unwrapped islands:', doc.unwrap());
}

const m = doc.model3d();
console.log(
  `model3d          : positions=${m.positions.length} indices=${m.indices.length} edges=${m.edges.length}`,
);

const net = doc.pieces2d();
console.log(
  `pieces2d         : pieces=${net.pieces.length} triangles=${sum(net.pieces, 'triangles')} cuts=${sum(net.pieces, 'cuts')} folds=${sum(net.pieces, 'folds')}`,
);

console.log(
  `flaps            : ${sum(net.pieces, 'flaps')} (over ${net.pieces.length} pieces)`,
);
console.log('pages            :', doc.pack_islands());

const pdf = doc.export_pdf();
const pdfOk = String.fromCharCode(...pdf.slice(0, 5)) === '%PDF-';
console.log(`export_pdf       : ${pdf.length} bytes, %PDF=${pdfOk}`);

const svg = doc.export_svg();
const svgOk = String.fromCharCode(...svg.slice(0, 5)) === '<?xml';
console.log(`export_svg       : ${svg.length} bytes, xml=${svgOk}`);

const craft = doc.save_craft();
const isZip = craft[0] === 0x50 && craft[1] === 0x4b; // "PK"
console.log(`save_craft       : ${craft.length} bytes, zip=${isZip}`);

// --- Mesh simplification, on a synthetic dense sphere (model-independent) ---
// Binary STL of an octahedron subdivided `n` times, projected on the unit sphere.
function sphereStl(subdiv) {
  let pos = [
    [1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1],
  ];
  let tris = [
    [0, 2, 4], [2, 1, 4], [1, 3, 4], [3, 0, 4],
    [2, 0, 5], [1, 2, 5], [3, 1, 5], [0, 3, 5],
  ];
  for (let s = 0; s < subdiv; s++) {
    const mid = new Map();
    const midpoint = (a, b) => {
      const k = a < b ? `${a},${b}` : `${b},${a}`;
      if (!mid.has(k)) {
        const m = pos[a].map((v, i) => v + pos[b][i]);
        const len = Math.hypot(...m);
        pos.push(m.map((v) => v / len));
        mid.set(k, pos.length - 1);
      }
      return mid.get(k);
    };
    tris = tris.flatMap(([a, b, c]) => {
      const [ab, bc, ca] = [midpoint(a, b), midpoint(b, c), midpoint(c, a)];
      return [[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]];
    });
  }
  const buf = new DataView(new ArrayBuffer(84 + 50 * tris.length));
  buf.setUint32(80, tris.length, true);
  tris.forEach((t, i) => {
    let o = 84 + 50 * i + 12; // skip the (zero) normal
    for (const v of t) {
      for (const x of pos[v]) { buf.setFloat32(o, x, true); o += 4; }
    }
  });
  return new Uint8Array(buf.buffer);
}

const sdoc = new pc.PaperDoc(sphereStl(4), 'stl');
sdoc.unwrap();
const before = sdoc.stats().faces;
const after = sdoc.simplify(300);
sdoc.unwrap();
const snet = sdoc.pieces2d();
console.log(
  `simplify         : ${before} -> ${after} faces, pieces=${snet.pieces.length}`,
);
const simplifyOk =
  before === 2048 && after <= 300 && after > 50 && snet.pieces.length > 0;
let simplifyRejects = false;
try {
  sdoc.simplify(1e9); // target above current must be rejected
} catch {
  simplifyRejects = true;
}
console.log(`simplify guards  : rejects-bad-target=${simplifyRejects}`);

if (net.pieces.length === 0 || !pdfOk || !svgOk || !isZip || !simplifyOk || !simplifyRejects) {
  console.error('SMOKE TEST FAILED');
  process.exit(1);
}
console.log('OK');
