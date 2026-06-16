// Engine-local color types.
//
// The engine must not depend on the GUI/GL stack (easy-imgui / glow), so it
// owns these plain POD color types instead of borrowing `easy_imgui::Color`
// and `easy_imgui_opengl::Rgba`. The desktop shell converts to/from its own
// imgui/GL color types at the boundary. The `.craft` serialization only relies
// on the `r/g/b/a` fields and `new`, so the on-disk format is unchanged.

/// An RGBA color with non-premultiplied alpha, components in `0.0..=1.0`.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Color {
        Color { r, g, b, a }
    }
    pub const BLACK: Color = Color::new(0.0, 0.0, 0.0, 1.0);
    pub const WHITE: Color = Color::new(1.0, 1.0, 1.0, 1.0);
    pub const YELLOW: Color = Color::new(1.0, 1.0, 0.0, 1.0);
}

/// Same data as [`Color`]; kept as a distinct type to mirror the shell's
/// separate GL `Rgba` uniform/attribute type.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Rgba {
        Rgba { r, g, b, a }
    }
}
