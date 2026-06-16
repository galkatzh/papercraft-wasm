//! WebAssembly bindings for `papercraft-engine`.
//!
//! Thin `#[wasm_bindgen]` glue over the platform-neutral engine: import a model,
//! query the 3D mesh and 2D net as plain JS objects, edit edges (join/split),
//! repack, and save the `.craft` project. The heavy lifting lives in the engine;
//! this crate intentionally contains no algorithm code.

use wasm_bindgen::prelude::*;

use papercraft_engine::paper::{
    EdgeIndex, Papercraft,
    export::{self},
    formats::import_model_bytes,
};

/// Install a panic hook that forwards Rust panics to `console.error`. Called
/// automatically when the module is instantiated.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// An opaque handle to a papercraft document (a model plus its current unfolding).
#[wasm_bindgen]
pub struct PaperDoc {
    pc: Papercraft,
}

fn js_err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

#[wasm_bindgen]
impl PaperDoc {
    /// Import a model from bytes and unfold it. `format` is the lowercase
    /// extension without the dot (`stl`, `obj`, `pdo`, `glb`/`gltf`, `craft`).
    #[wasm_bindgen(constructor)]
    pub fn import(bytes: &[u8], format: &str) -> Result<PaperDoc, JsError> {
        let pc = import_model_bytes(bytes, format).map_err(js_err)?;
        Ok(PaperDoc { pc })
    }

    /// The 3D mesh: `{ positions, normals, indices, edges }`.
    #[wasm_bindgen(js_name = model3d)]
    pub fn model_3d(&self) -> Result<JsValue, JsError> {
        serde_wasm_bindgen::to_value(&export::model_3d(&self.pc)).map_err(js_err)
    }

    /// The 2D net: `{ pieces: [{ id, triangles, cuts, folds }] }`.
    #[wasm_bindgen(js_name = pieces2d)]
    pub fn pieces_2d(&self) -> Result<JsValue, JsError> {
        serde_wasm_bindgen::to_value(&export::pieces_2d(&self.pc)).map_err(js_err)
    }

    /// Produce an initial net by auto-joining edges across the whole model.
    /// Use after importing raw geometry (STL/OBJ/glTF); a loaded `.craft` already
    /// has its own unfolding. Returns the resulting number of pieces.
    pub fn unwrap(&mut self) -> u32 {
        self.pc.unwrap();
        self.pc.num_islands() as u32
    }

    /// Join the cut edge `edge` (an `EdgeIndex` from `model3d`/`pieces2d`).
    /// Returns `true` if anything changed.
    pub fn join_edge(&mut self, edge: u32) -> bool {
        self.pc
            .edge_join(EdgeIndex::from(edge as usize), None)
            .is_some()
    }

    /// Split (cut) the joined edge `edge`. Returns `true` if anything changed.
    pub fn split_edge(&mut self, edge: u32) -> bool {
        self.pc
            .edge_cut(EdgeIndex::from(edge as usize), None)
            .is_some()
    }

    /// Re-pack the islands onto the page(s). Returns the number of pages used.
    pub fn pack_islands(&mut self) -> u32 {
        self.pc.pack_islands()
    }

    /// Number of pieces (islands) in the current unfolding.
    pub fn num_islands(&self) -> u32 {
        self.pc.num_islands() as u32
    }

    /// Serialize the document to the `.craft` project format.
    pub fn save_craft(&self) -> Result<Vec<u8>, JsError> {
        let mut buf = std::io::Cursor::new(Vec::new());
        self.pc.save(&mut buf, None).map_err(js_err)?;
        Ok(buf.into_inner())
    }
}
