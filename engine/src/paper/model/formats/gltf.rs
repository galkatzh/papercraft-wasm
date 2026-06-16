mod data;
mod exporter;
mod importer;

// glTF reuses OpenGL enum values for accessor component types, buffer targets and
// texture filters. The engine must not depend on the GL stack (`glow`), so the
// handful of constants the importer/exporter need are defined here directly.
mod gl_const {
    pub const BYTE: u32 = 5120;
    pub const UNSIGNED_BYTE: u32 = 5121;
    pub const SHORT: u32 = 5122;
    pub const UNSIGNED_SHORT: u32 = 5123;
    pub const UNSIGNED_INT: u32 = 5125;
    pub const FLOAT: u32 = 5126;
    pub const NEAREST: u32 = 9728;
    pub const LINEAR: u32 = 9729;
    pub const LINEAR_MIPMAP_LINEAR: u32 = 9987;
    pub const ARRAY_BUFFER: u32 = 34962;
    pub const ELEMENT_ARRAY_BUFFER: u32 = 34963;
}

// glTF seems to store positions in meters, but we use millimeters.
const GLTF_SCALE: f32 = 1000.0;

pub use exporter::{GltfFormat, export};
pub use importer::GltfImporter;
