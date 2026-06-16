# PROGRESS

Running log of decisions, the chosen module boundary, deviations from `PLAN.md`,
and open questions for the owner.

## Status

- **2026-06-16** — Phase 1 discovery complete. Owner decisions received:
  **(a) export = vector-only v1** (API designed so browser-rendered backgrounds can
  be layered on later); **(b) Cargo shape = workspace** (`papercraft-engine` lib +
  `papercraft` bin), overriding PLAN.md's Option-A default.
- **2026-06-16** — Stage 0 (engine→GUI decoupling) started.
  - ✅ **Color decoupling done** — engine now owns plain `paper::Color`/`paper::Rgba`
    (`src/paper/color.rs`) instead of `easy_imgui::Color` / `easy_imgui_opengl::Rgba`.
    Removed GL-specific `LineConfig::to_3dstatus` from the engine; reimplemented it
    plus an `Rgba` bridge as shell helpers in `util_gl.rs`
    (`line_config_to_3dstatus`, `rgba_from_paper`). `build_color` converts at the
    imgui boundary. `.craft` on-disk format unchanged (serde still uses r/g/b/a).
    Conversions are plain functions (not `From` impls) to stay orphan-rule-safe
    after the crate split. **`cargo build` green.**
  - ✅ **glTF `glow` constants removed** — defined a local `gl_const` module in
    `paper/model/formats/gltf.rs` (glTF reuses GL enum values: BYTE=5120 … etc.)
    and aliased `use super::gl_const as glow` in `data.rs`/`exporter.rs`.
    **`cargo build` green.**
  - ⏳ **Last engine→GL coupling:** the `AttribField` impl in the `index_type!`
    macro (`paper/model.rs`). Only `MaterialIndex` is actually used as a GL vertex
    attribute (in shell `MVertex3D`/`MVertex2DColor`). **Resolution (at split):**
    `easy-imgui-opengl` is a standalone, lightweight crate (deps: cgmath/glow/log/
    smallvec only — no winit). The engine crate will take it as an **optional dep
    behind a default-off `opengl` feature** and gate the `AttribField` impls on it
    (`#[cfg(feature = "opengl")]`). Impl lives with the type → no orphan-rule issue;
    desktop bin enables `papercraft-engine/opengl` (binary stays identical); WASM
    builds `--no-default-features` → GL-free. Can't do this yet (needs the engine's
    own Cargo.toml); deferred to the workspace-split step.
- **2026-06-16** — ✅ **Workspace split done (Phase 1 core complete).**
  - New crate `engine/` = `papercraft-engine` (lib). Moved into it: `paper/` (incl.
    `color.rs`), `util_3d.rs`, `pdf_metrics.rs`, `version.rs`, and `thirdparty/afm/`.
    Added `engine/build.rs` (helvetica AFM generation) and `engine/src/lib.rs`
    (`pub mod paper/pdf_metrics/util_3d/version`).
  - Engine features: `parallel` (Rayon, default-off — `compute_edge_map` now has a
    sequential fallback) and `opengl` (optional `easy-imgui-opengl`, gates the
    `AttribField` impls on the index types). Both default-off.
  - Root crate stays the `papercraft` **binary** (build.rs/locales/res/distro/CI
    untouched) and is now also the **workspace root** (`members = ["engine"]`).
    Depends on `papercraft-engine` with `features = ["opengl", "parallel"]` so the
    desktop build is behaviorally identical. `main.rs` re-exports the engine modules
    under their old paths (`pub use papercraft_engine::{paper, pdf_metrics, util_3d,
    version}`) so the rest of the shell needed no path churn.
  - Removed the helvetica step from the shell `build.rs` (now in the engine).
  - **Verification:** `cargo build` (workspace) ✅ · `cargo build -p papercraft-engine
    --no-default-features` ✅ pulling **no** easy-imgui/winit/glow/clap/reqwest/
    directories (confirmed via `cargo tree`) · `cargo test -p papercraft-engine` ✅
    8/8 · `cargo clippy --workspace` clean except one pre-existing warning in
    `printable.rs:91` (shell, untouched — left as-is to keep the binary identical).

### Phase 1 acceptance status
- [x] Engine builds with `--no-default-features`, no UI/CLI/OS deps.
- [x] Desktop binary still builds (default features); behavior unchanged (pure code
      move + re-export + feature gates; `.craft` format untouched).
- [ ] Minimal public export API (`export_pdf`/`export_svg` vector, `pieces_2d`,
      `model_3d`) — NOT yet; the existing PDF/SVG export is GL-entangled in the shell.
      Next step: build a **new vector-only exporter in the engine** (will add `lopdf`).

- **2026-06-16** — ✅ **Engine compiles to `wasm32-unknown-unknown` (pure Rust).**
  Early validation of Phase 2's foundation. Fix applied: zip's default features
  pulled `zstd-sys` (C, via `cc`) plus bzip2/lzma/ppmd/xz/aes and `time`. `.craft`
  only uses Deflate, so the engine now takes `zip` with
  `default-features = false, features = ["deflate"]` (pure-Rust zopfli +
  flate2-zlib-rs). Result: `cargo build -p papercraft-engine --no-default-features
  --target wasm32-unknown-unknown` ✅ with **no -sys/C crates** in the tree
  (confirmed via `cargo tree`). The desktop build keeps full zip features through
  the binary's own `zip` dep (Cargo feature unification), so it is unaffected.
  Also sidesteps the `time` local-offset wasm panic risk (PLAN §5.2).

### Phase 2 status
- [x] Engine compiles to `wasm32-unknown-unknown`, single-threaded, pure Rust.
- [x] **Engine web-geometry module** `paper::export` (new, GL-free): `model_3d(pc)`
      → `{positions, normals, indices, edges}` and `pieces_2d(pc)` → per-island
      `{triangles, cuts, folds}` with mountain/valley classification. Reuses
      `traverse_faces`/`face_plane().project()`/`edge_status`; the shell render
      path (`paper_draw_face`) is untouched. **v1 omits glue flaps + edge-id text
      labels** (documented follow-up).
- [x] **Bytes-based import** `formats::import_model_bytes(bytes, format)` (wasm has
      no filesystem) covering stl/obj/pdo/glb/gltf/craft.
- [x] **`papercraft-wasm` crate** (`wasm/`) with `#[wasm_bindgen]` `PaperDoc`:
      `import`, `model3d`, `pieces2d`, `join_edge`, `split_edge` (→ engine
      `edge_join`/`edge_cut`), `pack_islands`, `num_islands`, `save_craft`.
      Depends on the engine with default features (no opengl/parallel).
      **Builds for `wasm32-unknown-unknown`** ✅; native workspace still builds ✅.
- [x] **`unwrap()`** — engine `Papercraft::unwrap` greedily auto-joins edges across
      all islands (overlap-aware, terminates monotonically) to make an initial net
      from raw geometry; exposed on `PaperDoc`. (`.craft` already carries its net.)
- [x] **`wasm-pack` packaging** — `wasm-pack build wasm --target bundler --out-dir
      ../web/src/wasm` ✅ (output gitignored; ~5 MB dev `.wasm`). Clean TS `.d.ts`.
- [x] **Node smoke test** (`wasm/tests/smoke.mjs`), verified end-to-end:
      - `die.craft`: 1 island, 12 tris, **14 cuts / 5 folds** (correct cube net),
        `save_craft` → valid zip.
      - tetra `.stl`: import 4 islands → `unwrap()` → **1 island, 3 folds, 6 cuts**
        (correct tetrahedron net).
      - `bulbasaur.craft`: 30 islands; `split_edge` 30→31, `join_edge` 31→30,
        `pack_islands` → 7 pages.
- [x] **Vector exporter in the engine** (`paper::export`): `export_pdf` (multi-page,
      `lopdf`) and `export_svg` (single SVG over the page grid). Includes **glue
      flaps** (via `flat_face_flap_dimensions` + the desktop `draw_flap` side logic),
      cut outlines and mountain(solid)/valley(dashed) folds, page layout via
      `global_to_page`/`page_position`. `pieces2d` now also carries flaps. Wired
      into `PaperDoc` as `export_pdf()`/`export_svg()`.
      - `lopdf` added to the engine with `default-features=false, features=["wasm_js"]`
        (drops chrono/jiff/rayon/time; getrandom wasm backend) → still wasm-clean.
      - Edge-id text labels and fold-style in/out extensions deferred (follow-up).

### Phase 2 acceptance status — COMPLETE
- [x] `papercraft-wasm` builds via `wasm-pack` for `wasm32`, single-threaded, pure Rust.
- [x] Harness: import STL → `unwrap()` → non-empty `pieces2d()`.
- [x] `export_pdf()` returns `%PDF` bytes (validated: catalog/pages/xref/%%EOF).
      die.craft → correct cube net (rendered); bulbasaur → 7-page net (rendered).

### Phase 3 — web front-end (MVP working)
- Owner preference honoured: **no bundler / no build step** — vanilla JS + an
  import map + locally-vendored Three.js; `wasm-pack --target web` output committed
  under `web/wasm/` so the static site runs as-is.
- `web/index.html` + `web/app.js`: toolbar (open / unwrap / repack / split-join
  mode / export PDF·SVG / save .craft), a Three.js **3D view** (mesh + edge overlay
  coloured by cut/mountain/valley, orbit, click-to-pick edges) and a Canvas **2D net
  view** (triangles, cuts, mountain/valley folds, flaps; zoom/pan, click-to-pick).
  Linked editing: clicking an edge in either view splits/joins and both refresh.
- `wasm-opt` disabled (binaryen download blocked here) → dev `.wasm` ~1.5 MB;
  size-optimize at deploy.
- **Verified in a real browser (Playwright/Chromium):** engine init, `die.craft`
  loads → 3D die + cube net render (screenshot), `tetra.stl` → fresh `unwrap()` →
  1 piece, **Export PDF downloads a `%PDF` file**, zero console/page errors.

### Phase 3 acceptance status
- [x] Loads STL/OBJ/PDO/glTF/.craft; linked 3D + 2D views.
- [x] Edge split/join + repack; both views update.
- [x] Exports printable PDF and layered-ish SVG (cut/fold/flap).
- [x] Deploys as a static site, no special headers (single-threaded).

### Remaining / follow-ups
- Edge-id text labels (PDF/SVG + 2D) and fold-style in/out extensions.
- `wasm-opt` size pass for deploy; lazy-load the module.
- Large/non-manifold mesh guards + graceful engine-error surfacing.
- Track A (upstream lib-split PR) / Track B (fork CI building the wasm target).

### Follow-ups / minor deviations
- Engine crate does not inherit the root `[lints.clippy]` table; consider
  `[workspace.lints]` later (polish, deferred to avoid churn now).
- `thirdparty/afm/` moved under `engine/` (only `build.rs` referenced it).

- Note: remote push is currently blocked (git proxy returns 403 — no write permission).
  Work is committed locally only. Owner is aware.

---

## Phase 1 — Discovery findings (Section 11 of PLAN.md)

The repo at the project root **is** the upstream `rodrigorc/papercraft` source
(single binary crate `papercraft` v2.12.0, Rust edition 2024, GPL-3.0-or-later).
Not a separate fork directory — the engine and ImGui shell share one `src/` tree.

### 1. Module inventory and engine-vs-shell classification

Top-level modules declared in `src/main.rs`:
`config`, `paper`, `pdf_metrics`, `printable`, `semaphore`, `util_3d`, `util_gl`, `ui`, `version`.

| Module | Class | Notes |
|---|---|---|
| `paper/` (model, craft, formats/*) | **ENGINE** | Mesh, unwrap/islands, flaps, importers, `.craft` serde. Pure except shallow coupling — see §4. |
| `util_3d.rs` | **ENGINE** | Pure math. 0 shell imports. |
| `pdf_metrics.rs` | **ENGINE** | Helvetica font metrics for PDF text. Pure. |
| `semaphore.rs` | **ENGINE-ok** | std threads only; unused on wasm (single-threaded v1). |
| `printable.rs` | **SPLIT** | Vector PDF/SVG assembly logic is engine-portable; page **rasterization is OpenGL** — see §3. Must be split, not assigned wholesale. |
| `util_gl.rs` | **SHELL** | OpenGL wrappers. Exception: `MLine3DStatus` (a bitflags type) is referenced by engine `craft.rs` — needs relocation/replacement (§4). |
| `ui.rs` | **SHELL** | Dear ImGui UI. |
| `config.rs` | **SHELL** | App config (1 shell import). |
| `version.rs` | SHELL-lean | App version / update check (pairs with `reqwest` in main). |
| `main.rs` | **SHELL** | Window/event loop, CLI (`clap`), OS integration. |

`glr` is not a local module — it is `easy_imgui_window::easy_imgui_renderer::easy_imgui_opengl`
aliased as `glr` in `main.rs` and `util_gl.rs`. Pure shell.

### 2. Headless CLI path — PARTIAL

`clap` is used only in `main.rs`. A CLI exists, but the export path it drives is
**not** GL-free (see §3), so it is **not** a clean headless load→unwrap→export
template the way PLAN.md hoped. The unwrap/edit API in `paper/craft.rs` is clean;
the export half is entangled with OpenGL.

### 3. ⚠️ KEY FINDING — PDF/SVG export is GL-backed, not pure Rust

This contradicts PLAN.md §0/§2 ("the PDF export engine survives the port intact").

- `printable.rs::generate_pages()` renders every page into an `image::RgbaImage`
  using an **OpenGL framebuffer** (`glr::Framebuffer`, `glow::*`), lines ~736–998.
- `generate_pdf()` and `generate_png()` both consume that GL-rendered bitmap.
  The PDF **embeds the raster page image** ("two images per page... compress the
  images, the most expensive part of the PDF").
- `generate_svg()` is **mostly vector** (cut/fold lines, edge-id text, flap
  outlines are computed from geometry) **but** its "Background" layer embeds the
  same GL-rendered bitmap as a base64 PNG `<image>` (`svg_write_layer_background`).

**Consequence:** the vector geometry (cut/fold/flap/text layers) and the PDF/SVG
*assembly* code (lopdf, svg writers) are portable; the **textured/colored face
background raster is GL-bound**. The web port needs a rasterization strategy.
Options (see Open Questions):
- **(A) Vector-only v1** — export cut/fold/flap lines + edge-id text, no textured
  background. Trivial port; perfect for solid-color models; loses face textures in print.
- **(B) Browser-rendered background** — render page bitmaps with Three.js/WebGL in
  the front-end, hand them to a WASM export assembler that embeds them (faithful parity).
- **(C) CPU rasterizer in WASM** — replace GL page rendering with e.g. `tiny-skia`. Most work.

### 4. Engine→GL/imgui coupling that must be decoupled (shallow, tractable)

All in otherwise-pure engine code; all replaceable with engine-local types:

- `paper/craft.rs` uses `easy_imgui::Color` and `easy_imgui_opengl::Rgba` for color
  **config fields** (line/paper/bg colors). `Color`/`Rgba` are simple `{r,g,b,a:f32}`
  structs — replace with an engine-local `MyColor`/`Rgba` newtype + `BLACK/WHITE/new`.
- `paper/craft.rs` uses `util_gl::MLine3DStatus` (a `bitflags` type) in `to_3dstatus()`
  — a 3D-line render hint. Relocate the type into the engine or feature-gate the method.
- `paper/model.rs` line ~144: `unsafe impl crate::glr::AttribField for …` macro —
  GL vertex-attribute trait impl inside the model module. Feature-gate (`gui`) or remove from lib.
- `paper/model/formats/gltf/*` use `glow::{BYTE,FLOAT,…}` as plain integer component-type
  constants — replace with local `const` values (trivial).

### 5. `.craft` (de)serialization & importers

- `.craft` serde lives in `paper/craft.rs` + `paper/craft/file.rs` + `paper/craft/update.rs`.
- Importers/exporters in `paper/model/formats/` (stl, waveobj/OBJ, pepakura/PDO, gltf).
- These pull `serde`, `zip`, `flate2`, `image`, `base64` — all pure Rust. Only the
  `glow`-constant coupling in `gltf` needs the §4 fix. No shell deps otherwise.

### 6. Latest release tag

No tags present locally (shallow/tagless clone). To resolve via GitHub before any
Track-A/B rebase work. Current `Cargo.toml` version = **2.12.0**; HEAD merges PR #25.

### 7. build.rs / i18n under lib-only build

`build.rs` uses `include-po` (i18n) and emits generated code; `tr`/`tr!` macro is
used in `main.rs`/`ui.rs` (shell). Engine modules do not appear to call `tr!`
(to be re-confirmed during the move). Expectation: `build.rs` stays for the binary;
the engine lib does not depend on i18n. Verify with `cargo build --no-default-features`.

---

## Proposed Cargo shape & boundary (for HUMAN GATE 4.2)

**Cargo shape: Option A** (single package, `lib` + `bin`) per PLAN.md default —
smaller diff, friendlier to a future upstream PR.

- Add `src/lib.rs` exposing engine modules: `pub mod paper; pub mod util_3d; pub mod pdf_metrics;`
  plus a new engine-local color module, and the extracted vector-export logic.
- Feature-gate all UI/CLI/OS deps behind `default = ["gui"]`; engine builds with
  `--no-default-features`.
- `printable.rs`: extract the vector PDF/SVG assembly into the lib; leave
  `generate_pages` (GL) + PNG + the GL-bitmap background behind `gui`.
- Decouple the §4 items first (color/glow-const/MLine3DStatus/AttribField), since
  the lib cannot compile until those leave the engine modules.

**Public API (minimal):** `import_model`, `Document`, `unwrap`/repack,
`join_edge`/`split_edge`, `pieces_2d`, `model_3d`, vector `export_pdf`/`export_svg`,
`save_craft`/`load_craft`. Everything else `pub(crate)`.

---

## Open questions for the owner

1. **Export rasterization strategy** (architecturally significant — shapes the engine
   API and the front-end). Vector-only v1 (A), browser-rendered background (B), or
   CPU rasterizer (C)? Recommendation: **A for v1**, design the export API so **B**
   can be layered on later (front-end supplies optional page bitmaps).
2. **Confirm the boundary above** before code is moved (HUMAN GATE 4.2).

## Deviations from PLAN.md

- PLAN.md assumed PDF/SVG export is pure Rust. **It is not** — page rasterization is
  OpenGL, and both PDF and the SVG background layer embed GL-rendered bitmaps (§3).
  This adds an export-strategy decision not anticipated in the plan.
- The "headless CLI = ready-made engine API" assumption (PLAN.md §1/§11.2) holds for
  the unwrap/edit half but not the export half (§2).
