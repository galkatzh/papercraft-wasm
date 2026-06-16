//! Neutral, serializable geometry for the web front-end (and the vector exporter).
//!
//! This is a purpose-built, GL-free alternative to the desktop shell's
//! `paper_draw_face` pipeline. It reuses the engine primitives (`traverse_faces`,
//! `face_plane().project()`, `edge_status`, edge angles) to emit plain data the
//! browser can render with Three.js / SVG and feed back for editing.
//!
//! v1 scope: the 3D model (positions/normals/indices + classified edges) and the
//! 2D net per island (face triangles, cut outlines, mountain/valley folds).
//! Glue flaps and edge-id text labels are not emitted yet (follow-up).

use std::ops::ControlFlow;

use cgmath::{EuclideanSpace, Point2, Transform};
use serde::Serialize;

use super::{EdgeStatus, Papercraft, VertexIndex};

/// How an edge should be drawn.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EdgeKind {
    /// A cut edge (becomes part of a piece outline).
    Cut,
    /// A joined edge folded outwards (convex).
    Mountain,
    /// A joined edge folded inwards (concave).
    Valley,
    /// A joined edge that is (soft) hidden / coplanar — not normally drawn.
    Flat,
}

fn edge_kind(status: EdgeStatus, angle_negative: bool) -> EdgeKind {
    match status {
        EdgeStatus::Cut(_) => EdgeKind::Cut,
        EdgeStatus::Joined => {
            if angle_negative {
                EdgeKind::Valley
            } else {
                EdgeKind::Mountain
            }
        }
        EdgeStatus::Hidden | EdgeStatus::SoftHidden => EdgeKind::Flat,
    }
}

// ---------------------------------------------------------------------------
// 3D model
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct Model3D {
    /// Per-vertex positions (indexed by `VertexIndex`).
    pub positions: Vec<[f32; 3]>,
    /// Per-vertex normals, parallel to `positions`.
    pub normals: Vec<[f32; 3]>,
    /// Triangle list: indices into `positions`, 3 per face.
    pub indices: Vec<u32>,
    /// Every model edge, with its endpoints and current fold/cut status, for
    /// rendering edge overlays and for picking (the `edge` id drives join/split).
    pub edges: Vec<Edge3D>,
}

#[derive(Serialize)]
pub struct Edge3D {
    /// `EdgeIndex` as u32 — pass to `join_edge` / `split_edge`.
    pub edge: u32,
    pub p0: [f32; 3],
    pub p1: [f32; 3],
    pub kind: EdgeKind,
}

/// Build the 3D mesh (positions/normals/indices) plus the classified edge list.
pub fn model_3d(pc: &Papercraft) -> Model3D {
    let model = pc.model();

    let mut positions = Vec::with_capacity(model.num_faces() * 3);
    let mut normals = Vec::new();
    for (_i_v, v) in model.vertices() {
        let p = v.pos();
        positions.push([p.x, p.y, p.z]);
        let n = v.normal();
        normals.push([n.x, n.y, n.z]);
    }

    let mut indices = Vec::with_capacity(model.num_faces() * 3);
    for (_i_face, face) in model.faces() {
        for i_v in face.index_vertices() {
            indices.push(usize::from(i_v) as u32);
        }
    }

    let mut edges = Vec::with_capacity(model.num_edges());
    for (i_edge, edge) in model.edges() {
        let (p0, p1) = model.edge_pos(edge);
        let status = pc.edge_status(i_edge);
        let kind = edge_kind(status, edge.angle().0.is_sign_negative());
        edges.push(Edge3D {
            edge: usize::from(i_edge) as u32,
            p0: [p0.x, p0.y, p0.z],
            p1: [p1.x, p1.y, p1.z],
            kind,
        });
    }

    Model3D {
        positions,
        normals,
        indices,
        edges,
    }
}

// ---------------------------------------------------------------------------
// 2D net
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct Pieces2D {
    pub pieces: Vec<Piece2D>,
}

#[derive(Serialize)]
pub struct Piece2D {
    /// Stable per-document index of the island (enumeration order).
    pub id: u32,
    /// Face triangles laid out in 2D (paper millimetres, before page packing).
    pub triangles: Vec<[[f32; 2]; 3]>,
    /// Cut edges (the piece outline).
    pub cuts: Vec<Line2D>,
    /// Interior fold lines (mountain/valley).
    pub folds: Vec<Line2D>,
}

#[derive(Serialize)]
pub struct Line2D {
    /// `EdgeIndex` as u32 — pass to `join_edge` / `split_edge`.
    pub edge: u32,
    pub p0: [f32; 2],
    pub p1: [f32; 2],
    pub kind: EdgeKind,
}

/// Lay out every island in 2D and emit its triangles, cut outline and folds.
pub fn pieces_2d(pc: &Papercraft) -> Pieces2D {
    let model = pc.model();
    let scale = pc.options().scale;

    let mut pieces = Vec::with_capacity(pc.num_islands());
    for (id, (_key, island)) in pc.islands().enumerate() {
        let mut triangles = Vec::new();
        let mut cuts = Vec::new();
        let mut folds = Vec::new();

        let _ = pc.traverse_faces(island, |i_face, face, matrix| {
            let plane = model.face_plane(face);
            let project = |i_v: VertexIndex| {
                let v = &model[i_v];
                let p = plane.project(&v.pos(), scale);
                let p = matrix.transform_point(Point2::from_vec(p)).to_vec();
                [p.x, p.y]
            };

            let vs = face.index_vertices();
            triangles.push([project(vs[0]), project(vs[1]), project(vs[2])]);

            for (i_v0, i_v1, i_edge) in face.vertices_with_edges() {
                let status = pc.edge_status(i_edge);
                let edge = usize::from(i_edge) as u32;
                match status {
                    EdgeStatus::Cut(_) => {
                        cuts.push(Line2D {
                            edge,
                            p0: project(i_v0),
                            p1: project(i_v1),
                            kind: EdgeKind::Cut,
                        });
                    }
                    EdgeStatus::Joined => {
                        // Draw shared folds from a single side only to avoid duplicates.
                        if !model[i_edge].face_sign(i_face) {
                            continue;
                        }
                        let kind = edge_kind(status, model[i_edge].angle().0.is_sign_negative());
                        folds.push(Line2D {
                            edge,
                            p0: project(i_v0),
                            p1: project(i_v1),
                            kind,
                        });
                    }
                    EdgeStatus::Hidden | EdgeStatus::SoftHidden => {}
                }
            }
            ControlFlow::Continue(())
        });

        pieces.push(Piece2D {
            id: id as u32,
            triangles,
            cuts,
            folds,
        });
    }

    Pieces2D { pieces }
}
