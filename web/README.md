# Papercraft Unfolder — web front-end

A browser papercraft unfolder: load a 3D mesh (STL / OBJ / PDO / glTF / `.craft`),
see the 3D model and the 2D net side by side, edit edges (split / join), repack,
and export a printable cut–fold–glue **PDF** or **SVG**.

Built on [`rodrigorc/papercraft`](https://github.com/rodrigorc/papercraft)'s
unwrapping engine compiled to WebAssembly (see `../engine` and `../wasm`).

## Running — no build step

This is plain HTML + JavaScript with no bundler. Just serve this `web/` directory
over HTTP (the WASM module must be fetched over `http(s)://`, not `file://`):

```sh
cd web
python3 -m http.server 8099
# open http://127.0.0.1:8099/
```

Any static host works (GitHub Pages, Netlify, Cloudflare Pages) — no special
headers are needed (the engine runs single-threaded).

## Layout

- `index.html` — page + import map (maps the bare `three` specifier to the vendored copy).
- `app.js` — the application (Three.js 3D view + Canvas 2D net + the WASM glue).
- `wasm/` — `wasm-pack --target web` output, committed so the site runs as-is.
- `vendor/three/` — locally vendored Three.js (`three.module.js`, `OrbitControls.js`).

## Rebuilding the WASM

After changing the Rust engine or bindings:

```sh
wasm-pack build wasm --target web --release --out-dir ../web/wasm
rm -f ../web/wasm/.gitignore   # keep the artifacts tracked
```

For size-optimized deploys, also run `wasm-opt` on `papercraft_wasm_bg.wasm`
(disabled here because the binaryen download is blocked in this environment).

## License

GPL-3.0-or-later — the engine is GPL-3.0, so this front-end and the shipped WASM
are too. See the repository root `LICENSE`.
