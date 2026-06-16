# Web Papercraft Unfolder — Implementation Plan

**Audience:** Claude Code (autonomous implementation), supervised by the project owner.
**Goal:** Build a browser-based papercraft unfolder (STL/OBJ → printable cut-fold-glue PDF/SVG) by reusing the unfolding engine from `rodrigorc/papercraft`, compiled to WebAssembly, behind a new TypeScript + Three.js front-end.

## How to use this document

Work phase by phase, in order. Each phase ends with a Verification block — do not proceed until it passes. Several steps are marked 🛑 HUMAN GATE: stop and get explicit owner approval before doing them (anything that publishes, pushes to a public remote, posts to the upstream project, or force-pushes). Section 11 lists facts you must discover from the actual source tree, because they could not be confirmed in advance — resolve those early and report back before committing to the module boundary.

Keep a running `PROGRESS.md` in the project root recording: decisions made, the chosen module boundary, anything that deviated from this plan, and open questions for the owner.

## 0. Context and rationale (self-contained summary)

There is no mature web-based tool that unfolds an arbitrary 3D mesh into a papercraft net. Capable desktop tools exist; the gap is purely "nobody built the web version," not a technical barrier. `rodrigorc/papercraft` is a mature, actively maintained (Rust, ~98%) desktop application that already does everything a "usable hobby tool" needs: edge join/split editing, glue-flap generation, edge-ID annotations, overlap highlighting, and PDF/PNG/SVG export with separate cut / mountain-fold / valley-fold layers. It imports OBJ, STL, and Pepakura PDO.

The decisive constraint: its GUI is Dear ImGui via `easy-imgui-window` (a desktop windowing + OpenGL stack), not egui. Therefore the whole app does not compile cleanly to `wasm32-unknown-unknown`. The viable strategy is split-the-core: separate the pure-Rust unfolding engine from the desktop shell, compile only the engine to WASM, and build a new web front-end around it.

This is favorable because nearly everything except the GUI is pure, WASM-friendly Rust — including the PDF export engine — so the algorithmically hard work and the print pipeline survive the port intact.

**License note (read before any commercial planning):** `rodrigorc/papercraft` is GPL-3.0-or-later. Because WASM is shipped to the browser (= distribution), any derivative front-end is bound by GPL-3.0 too. This is fine for an open-source hobby tool. It is a blocker for closed-source/commercial use. Do not assume a permissive path exists; the alternative C engine (`osresearch/papercraft`) is GPL-2.0 and lacks glue tabs, so it is reference material only, not a base.

## 1. Target repository facts

* Repo: a fork of `https://github.com/rodrigorc/papercraft`
* Language: Rust, edition 2024.
* License: GPL-3.0-or-later.
* Package shape: single binary crate (`name = "papercraft"`), engine and ImGui UI in one `src/` tree. No `[lib]` target currently.
* Engine-side dependencies (pure Rust, expected to compile to WASM): `cgmath`, `slotmap`, `serde` / `serde_json`, `zip`, `flate2`, `image` (png+jpeg), `lopdf` (PDF export), `bitflags`, `fxhash`, `base64`, `maybe-owned`, `anyhow`, `rayon` (needs gating — see Phase 2).
* Shell-side dependencies (desktop/OS/UI — must NOT be pulled by the engine lib): `easy-imgui-window`, `easy-imgui-filechooser`, `clap`, `opener`, `directories`, `signal-hook`, `sys-locale`, `num_cpus`, `env_logger`, `reqwest` (blocking), and `time` with `local-offset` (gate on WASM).
* Build script: `build.rs` uses `include-po` for i18n (`tr` crate). Keep it working for the binary; verify it does not block a lib-only build.
* Strong signal: `clap` is a dependency, so a headless command-line path very likely exists (load model → unfold → export PDF without a window). Find it — the functions it calls are essentially the library API you need to expose.

## 2. Architecture overview

```
┌─────────────────────────────────────────────────────────────┐
│  Browser (static SPA)                                        │
│  ┌───────────────┐   ┌──────────────────────────────────┐   │
│  │ TS + Three.js │   │ papercraft-wasm (wasm-bindgen)   │   │
│  │  3D model view│◄─►│  thin glue over engine API       │   │
│  │  2D net view  │   │  load/unfold/join/split/export   │   │
│  │  toolbar/io   │   └──────────────┬───────────────────┘   │
│  └───────────────┘                  │ depends on            │
└─────────────────────────────────────┼───────────────────────┘
                                       ▼
                         ┌──────────────────────────────┐
                         │ papercraft-engine (Rust lib) │  ← upstream split
                         │  mesh, unwrap/cut-tree,       │
                         │  flaps, importers, PDF/SVG    │
                         │  pure Rust, no GPU/window     │
                         └──────────────────────────────┘
```

Three crates / layers:

1. `papercraft-engine` — the upstream engine, exposed as a library. Ideally lives upstream via the PR in Section 8; otherwise in your fork (Section 9). Contains no `wasm-bindgen` code — it stays a clean, platform-neutral Rust library.
2. `papercraft-wasm` — your crate. Depends on `papercraft-engine`, adds the `#[wasm_bindgen]` glue, compiles to WASM with `wasm-pack`. Keeping the bindgen glue here (not upstream) is what keeps the upstream PR minimal and free of a `wasm-bindgen` dependency.
3. `web/` — the TypeScript + Three.js front-end (Vite). Consumes the `wasm-pack` output as an npm-style module.

Reused as-is in the engine: mesh model, unwrap/cut-tree algorithm, flap generation, `.craft` (de)serialization, OBJ/STL/PDO importers, PDF/PNG/SVG export. Replaced for web: the entire ImGui shell, winit event loop, GL rendering, file chooser, CLI parsing, OS integration.

## 3. Repository and branch strategy

You will pursue two tracks in parallel off the same fork:

* Track A (preferred long-term): upstream the `[lib]` split so the engine becomes a normal dependency and updates become `cargo update`. See Section 8.
* Track B (always-available fallback): keep the split in your fork as a thin, new-files-only patch that rebases cleanly. See Section 9.

Do all engine work on the fork; the front-end lives in a separate repo (or a top-level `web/` dir) that does not track upstream.

Never force-push a shared branch. Never push to the upstream remote.

## 4. Phase 1 — Engine/library split

Objective: Produce a `papercraft-engine` library target whose public API can drive a full unfold→export cycle headlessly, with the existing binary unchanged.

### 4.1 Find the seam

* Open `src/main.rs` (and any `mod.rs`) and enumerate the top-level `mod` declarations.
* Classify each module engine vs shell using this test: does it import `easy_imgui*`, perform drawing, parse CLI args, or touch the OS/filesystem-paths? If yes → shell. If it only manipulates geometry/data/serialization → engine.
* Trace the headless CLI path (driven by `clap`). The functions it calls from `main` down to "write PDF" delineate the engine's natural public surface. Record this call graph in `PROGRESS.md`.

### 4.2 🛑 HUMAN GATE — confirm the boundary

Before moving any code, write the proposed engine-vs-shell module split to `PROGRESS.md` and get owner sign-off. Boundary mistakes are expensive to unwind.

### 4.3 Choose the Cargo shape

Default to the smaller-diff option unless the owner prefers the workspace:

* Option A (single package, lib + bin): add `src/lib.rs` declaring the engine modules `pub mod …`; `src/main.rs` becomes the binary doing `use papercraft::…`. Gate all UI/CLI/OS deps behind a default feature so the lib can build without them:

```toml
[features]
default = ["gui"]
gui = ["dep:easy-imgui-window", "dep:easy-imgui-filechooser", "dep:clap",
       "dep:opener", "dep:directories", "dep:signal-hook", "dep:sys-locale",
       "dep:num_cpus", "dep:env_logger", "dep:reqwest"]
parallel = ["dep:rayon"]   # see Phase 2
```

Make `[[bin]]` build only under `gui` (e.g. guard `main.rs` and bin-only modules with `#[cfg(feature = "gui")]`). The engine then builds with `--no-default-features`.

* Option B (workspace): `papercraft-engine` (lib) + `papercraft` (bin depending on it). Cleaner dependency partitioning, larger diff, more upstream resistance.

### 4.4 Define a small public API

Do not `pub` everything — that turns every internal into a semver commitment and reintroduces upgrade churn. Expose only:

* `import_model(bytes, format) -> Document` (OBJ/STL/PDO)
* `Document` (opaque handle/type)
* `unwrap(&mut Document, options)` / re-pack
* `join_edge(&mut Document, edge_id)` / `split_edge(&mut Document, edge_id)`
* `pieces_2d(&Document) -> serializable geometry` (for the front-end to render)
* `model_3d(&Document) -> serializable geometry` (vertices/faces/edge ids for the 3D view)
* `export_pdf(&Document, settings) -> Vec<u8>`, `export_svg(&Document, settings) -> Vec<u8>` (reuse existing layered SVG)
* `save_craft` / `load_craft` (the `.craft` project format)

Everything else stays `pub(crate)`.

### 4.5 Keep the binary identical

The diff should be almost entirely code moves (relocating modules into the lib) plus `pub`/`use`-path adjustments and feature gates — no behavioral changes. CLI flags, i18n/`build.rs`, and output must be unchanged.

### Verification (Phase 1)

```bash
cargo build                          # binary still builds, default features
cargo build --no-default-features    # engine compiles WITHOUT ui/cli/os deps
cargo clippy --all-targets           # respect existing configured lints (warns)
# If a headless CLI exists, run it on a sample STL and diff output against pre-split build.
```

All must pass. If `--no-default-features` still pulls a shell dep, a module is misclassified — fix before continuing.

## 5. Phase 2 — WASM build of the engine

Objective: A `papercraft-wasm` crate that compiles the engine to `wasm32-unknown-unknown` and exposes the API to JS.

### 5.1 Crate setup

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
```

Create `papercraft-wasm/` depending on the engine with default features off:

```toml
[dependencies]
papercraft-engine = { path = "../papercraft-fork", default-features = false }
wasm-bindgen = "0.2"
serde-wasm-bindgen = "0.6"   # pass structured data to/from JS without manual JSON
console_error_panic_hook = "0.1"
[lib]
crate-type = ["cdylib", "rlib"]
```

### 5.2 Handle the WASM gotchas

* `rayon`: build with the engine's `parallel` feature off; run single-threaded for v1. (Threads require `wasm-bindgen-rayon` + cross-origin isolation — deferred; see Phase 4.) If `rayon` is hardwired in an engine module rather than feature-gated, part of this phase is making it optional upstream/in-fork.
* `time` with `local-offset`: can panic on WASM. If the engine uses it (likely only for PDF timestamps), feature-gate or stub it on `cfg(target_arch = "wasm32")`.
* `flate2` / `zip`: ensure the pure-Rust backend (miniz_oxide), not a C/zlib-ng backend. Check default features.
* `reqwest`, `clap`, `directories`, `opener`, `signal-hook`, `num_cpus`, `env_logger`: must already be excluded via `--no-default-features`. Swap `env_logger` for `console_log` in the wasm crate if logging is wanted.

### 5.3 Bindgen surface

Wrap the engine API in `#[wasm_bindgen]` exports in `papercraft-wasm` only. Example shape:

```rust
#[wasm_bindgen]
pub struct PaperDoc(papercraft_engine::Document);

#[wasm_bindgen]
impl PaperDoc {
    #[wasm_bindgen(constructor)]
    pub fn import(bytes: &[u8], format: &str) -> Result<PaperDoc, JsError> { /* ... */ }
    pub fn unwrap(&mut self) { /* ... */ }
    pub fn split_edge(&mut self, edge_id: u32) { /* ... */ }
    pub fn join_edge(&mut self, edge_id: u32) { /* ... */ }
    pub fn pieces_2d(&self) -> JsValue { /* serde_wasm_bindgen::to_value(...) */ }
    pub fn model_3d(&self) -> JsValue { /* ... */ }
    pub fn export_pdf(&self) -> Vec<u8> { /* ... */ }
    pub fn export_svg(&self) -> Vec<u8> { /* ... */ }
}
```

### 5.4 Build

```bash
wasm-pack build papercraft-wasm --target bundler --out-dir ../web/src/wasm
# Use --target web instead of bundler if not using a bundler that supports it.
```

### Verification (Phase 2)

* `wasm-pack build` succeeds with no missing-symbol or non-wasm-dep errors.
* A throwaway Node/Vitest harness loads the module, imports a sample STL, calls `unwrap()`, and gets non-empty `pieces_2d()`.
* `export_pdf()` returns bytes beginning with `%PDF`.

## 6. Phase 3 — Web front-end

Objective: A usable single-page app: load a model, see the 3D shape and the 2D net side by side, click edges to join/split pieces, export a printable PDF/SVG.

### 6.1 Stack

* Vite + TypeScript. React optional; plain TS is fine for v1. Keep it a static SPA (no backend).
* Three.js for the 3D model view. For the 2D net view, either Three.js (orthographic camera) or an SVG/Canvas 2D renderer — SVG makes export-preview parity easy.

### 6.2 Core flow

1. Load: `<input type="file">` → `ArrayBuffer` → `Uint8Array` → `new PaperDoc(bytes, format)`. Detect format by extension (.stl/.obj/.pdo).
2. Initial unwrap: call `unwrap()`, then `model_3d()` and `pieces_2d()`.
3. Render 3D: build a Three.js mesh from `model_3d()`; color/edge data carries edge IDs for picking.
4. Render 2D: lay out pieces from `pieces_2d()` (polygons, fold lines as mountain/valley, flap shapes, edge-id labels).
5. Interact: raycast (3D) or hit-test (2D) to identify a clicked edge → call `join_edge(id)` or `split_edge(id)` → re-query and re-render both views. Provide a "repack pieces" button.
6. Export: `export_pdf()` / `export_svg()` → `new Blob([bytes])` → trigger download.

### 6.3 UI essentials (v1)

* File open; format auto-detect.
* 3D view (orbit) + 2D net view, linked selection.
* Edge mode toggle (join/split); repack; reset view.
* Document options panel mapping to engine settings (scale, flap width/angle, fold-line style, paper size, DPI, edge-id placement).
* Export PDF / SVG buttons.

### Verification (Phase 3)

* Load the repo's example low-poly model; confirm the rendered net matches the desktop app's output qualitatively.
* Join/split an edge and confirm both views update and piece count changes.
* Export a PDF and open it; pages, fold lines, and flaps are present and printable.

## 7. Phase 4 — Packaging, performance, deploy

* v1 deploy (single-threaded): plain static hosting — GitHub Pages, Netlify, Cloudflare Pages. No special headers needed.
* Optional threads (later): to re-enable `rayon` via `wasm-bindgen-rayon`, the site must be cross-origin isolated (`Cross-Origin-Opener-Policy: same-origin` + `Cross-Origin-Embedder-Policy: require-corp`) to allow `SharedArrayBuffer`. This rules out plain GitHub Pages without a workaround; use a host where you control headers. Keep v1 single-threaded to avoid this entirely.
* Bundle size: run `wasm-opt` (via `wasm-pack`'s release profile) and lazy-load the WASM module.
* Large/dirty meshes: add a guard that warns on very high triangle counts and suggests decimation; non-manifold/holey STLs are the known failure class — surface engine errors gracefully rather than hanging.

## 8. Track A — Upstream the `[lib]` split (preferred)

The aim is to make the engine a normal dependency so future updates are a version bump.

### 8.1 🛑 HUMAN GATE — issue first, code second

Do not open a PR cold. Have the owner (or draft for them to post) an issue that: explains the intent (a web/WASM front-end on the unwrap engine), proposes splitting into lib + unchanged binary, names the preferred Cargo shape (Option A), and asks whether the maintainer would accept it and where they'd put the boundary. Wait for a response. A pre-agreed refactor is far more likely to merge, and the maintainer may point to the natural seam directly.

### 8.2 Prepare the PR branch

* Base the branch on a released tag.
* The diff must be only the split: code moves + `pub`/`use` adjustments + feature gates. No reformatting, no feature additions, no unrelated clippy churn, no new dependencies (esp. no `wasm-bindgen` — that lives in your separate crate).
* Structure commits for review: (1) introduce `lib.rs` and move engine modules; (2) add feature gates for UI/CLI/OS deps. Each commit should build.
* Confirm the binary is byte-for-behavior identical and the release workflow in `.github/workflows` still passes.

### 8.3 🛑 HUMAN GATE — submit

Get owner approval before opening the PR. PR description should state: motivation, "no behavior change for existing users," the minimal public API (so it isn't a maintenance burden), and reference the issue.

### 8.4 If accepted

Switch `papercraft-wasm` to depend on the published/tagged engine (git dependency on upstream tag, or crates.io if published). Updates become:

```bash
# bump the dependency tag/version, then:
cargo update -p papercraft-engine
wasm-pack build papercraft-wasm --target bundler --out-dir ../web/src/wasm
```

## 9. Track B — Fork-only fallback

If the split is declined or stalls, keep it in your fork with a near-zero upstream footprint so it rebases cleanly.

### 9.1 Footprint discipline

* Put your additions in new files (`src/lib.rs`, a `features` block addition) and avoid editing upstream's existing source. Merge pain scales with overlap, not with upstream commit count.
* Never refactor engine internals to "fit" web; never scatter `cfg(target_arch="wasm32")` through upstream files. That converts every sync into a merge conflict.

### 9.2 Update cadence

```bash
git fetch upstream --tags
git rebase <new-upstream-tag>      # or merge; resolve only in files you added
cargo build --no-default-features  # re-confirm engine still builds
wasm-pack build papercraft-wasm --target bundler --out-dir ../web/src/wasm
```

### 9.3 CI guard (catch the real recurring tax)

A future upstream update may add a dependency that doesn't build for WASM. Add a CI job that, on every sync, builds the engine for `wasm32-unknown-unknown` and runs `wasm-pack build`. It fails loudly the moment a non-wasm dep is introduced; the fix is usually feature-gating the new dep. Pin to release tags, bump deliberately.

## 10. Licensing — keep this correct

* The engine is GPL-3.0-or-later. `papercraft-wasm` and the web front-end are derivative works and must also be GPL-3.0-or-later, including the JS/TS that links the WASM, since the WASM is distributed to browsers. Ship the license, keep sources available.
* Do not mix in `osresearch/papercraft` (GPL-2.0) code — if it is GPL-2.0-only it is incompatible with GPL-3.0. Use it only as conceptual reference, never copied.
* If a closed-source/commercial product is ever required, this base cannot be used; that path means writing an independent engine under a permissive license. Flag to the owner rather than working around it.

## 11. Discover from the source tree (resolve early, report in PROGRESS.md)

These could not be confirmed in advance; determine them in Phase 1 and surface anything surprising to the owner:

1. Exact module names and which are engine vs shell (Section 4.1).
2. Whether a headless CLI path exists end-to-end (load → unwrap → export) — your API template.
3. Whether `rayon` is feature-gated or hardwired in engine code (affects Phase 2 effort).
4. Whether `time`/`local-offset`, `reqwest`, or any OS dep leaks into otherwise-engine modules (must be gated for WASM).
5. Where `.craft` (de)serialization and the importers live, and whether they pull shell deps.
6. The current latest release tag to base branches on.
7. `build.rs`/i18n behavior under a lib-only build.

## 12. Overall acceptance criteria

* [ ] Engine builds with `--no-default-features`, pulling no UI/CLI/OS deps.
* [ ] Desktop binary still builds and behaves identically (default features).
* [ ] `papercraft-wasm` builds via `wasm-pack` for `wasm32-unknown-unknown`, single-threaded.
* [ ] Web SPA loads an STL/OBJ, shows linked 3D + 2D views, supports edge join/split + repack, and exports a valid printable PDF and layered SVG.
* [ ] v1 deploys as a static site with no special headers.
* [ ] Track A: issue posted and (if agreed) a minimal, behavior-preserving split PR opened — both behind human gates.
* [ ] Track B: fork carries a clean, rebaseable patch with a CI job that builds the WASM target on every upstream sync.
* [ ] GPL-3.0 obligations satisfied across all shipped artifacts.

## Suggested phase order for execution

1. Phase 1 (engine split) → verify.
2. Phase 2 (WASM build) → verify.
3. Phase 3 (front-end MVP: load + render + export, no editing) → verify.
4. Phase 3 editing (join/split/repack) → verify.
5. Phase 4 (deploy v1).
6. Track A issue/PR (parallel, human-gated) and Track B fork hardening + CI.

Stop at each 🛑 HUMAN GATE. Record every deviation in `PROGRESS.md`.
