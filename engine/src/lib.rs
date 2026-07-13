//! Papercraft unwrapping engine — the platform-neutral core extracted from the
//! desktop application. Contains the mesh model, the unwrap/island/flap logic,
//! the model importers/exporters, the `.craft` project (de)serialization and
//! the engine-local color types. It does **not** depend on the GUI, windowing
//! or OpenGL stacks (those live in the desktop shell), so it builds for
//! `wasm32-unknown-unknown`.

pub mod paper;
pub mod pdf_metrics;
pub mod util_3d;
pub mod version;
