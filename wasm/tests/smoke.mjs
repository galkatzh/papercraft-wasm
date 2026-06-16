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

console.log('pages            :', doc.pack_islands());

const craft = doc.save_craft();
const isZip = craft[0] === 0x50 && craft[1] === 0x4b; // "PK"
console.log(`save_craft       : ${craft.length} bytes, zip=${isZip}`);

if (net.pieces.length === 0 || craft.length === 0 || !isZip) {
  console.error('SMOKE TEST FAILED');
  process.exit(1);
}
console.log('OK');
