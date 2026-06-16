//! Neutral, serializable geometry + vector export for the web front-end.
//!
//! A GL-free alternative to the desktop shell's `paper_draw_face` pipeline. It
//! reuses the engine primitives (`traverse_faces`, `face_plane().project()`,
//! `edge_status`, `flat_face_flap_dimensions`) to emit plain data the browser can
//! render with Three.js / SVG, and to produce printable vector PDF/SVG.
//!
//! Covers: 3D model; the 2D net per island (face triangles, cut outlines,
//! mountain/valley folds with fold-style in/out extensions, glue flaps and
//! edge-id labels); and multi-page vector PDF / page-grid SVG.

use std::f32::consts::{FRAC_PI_2, PI};
use std::fmt::Write as _;
use std::ops::ControlFlow;

use cgmath::{EuclideanSpace, InnerSpace, Point2, Transform};
use serde::Serialize;

use super::{EdgeIdPosition, EdgeStatus, FlapStyle, FoldStyle, Papercraft, VertexIndex};
use crate::util_3d::Vector2;

/// How an edge should be drawn.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EdgeKind {
    Cut,
    Mountain,
    Valley,
    Flat,
}

fn fold_kind(angle_negative: bool) -> EdgeKind {
    if angle_negative {
        EdgeKind::Valley
    } else {
        EdgeKind::Mountain
    }
}

fn edge_kind(status: EdgeStatus, angle_negative: bool) -> EdgeKind {
    match status {
        EdgeStatus::Cut(_) => EdgeKind::Cut,
        EdgeStatus::Joined => fold_kind(angle_negative),
        EdgeStatus::Hidden | EdgeStatus::SoftHidden => EdgeKind::Flat,
    }
}

// ---------------------------------------------------------------------------
// 3D model
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct Model3D {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub edges: Vec<Edge3D>,
}

#[derive(Serialize)]
pub struct Edge3D {
    pub edge: u32,
    pub p0: [f32; 3],
    pub p1: [f32; 3],
    pub kind: EdgeKind,
}

/// Build the 3D mesh (positions/normals/indices) plus the classified edge list.
pub fn model_3d(pc: &Papercraft) -> Model3D {
    let model = pc.model();

    let mut positions = Vec::new();
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
        edges.push(Edge3D {
            edge: usize::from(i_edge) as u32,
            p0: [p0.x, p0.y, p0.z],
            p1: [p1.x, p1.y, p1.z],
            kind: edge_kind(status, edge.angle().0.is_sign_negative()),
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
// 2D net geometry (placed on the infinite paper, in millimetres)
// ---------------------------------------------------------------------------

#[derive(Serialize, Default)]
pub struct IslandGeom {
    pub triangles: Vec<[[f32; 2]; 3]>,
    pub cuts: Vec<Line2D>,
    pub folds: Vec<Line2D>,
    /// Each flap is an open polyline (its base is the shared fold edge).
    pub flaps: Vec<Vec<[f32; 2]>>,
    /// Edge-id labels (`island:id`).
    pub texts: Vec<Text2D>,
}

#[derive(Serialize, Clone, Copy)]
pub struct Line2D {
    pub edge: u32,
    pub p0: [f32; 2],
    pub p1: [f32; 2],
    pub kind: EdgeKind,
}

#[derive(Serialize, Clone)]
pub struct Text2D {
    pub pos: [f32; 2],
    /// Baseline rotation in radians (kept within ±90° for readability).
    pub angle: f32,
    /// Font height in millimetres.
    pub size: f32,
    pub text: String,
}

/// Push the fold-line segment(s) for an edge, honouring the fold style (the
/// optional little in/out extension marks). Mirrors the desktop's logic.
fn push_fold(
    folds: &mut Vec<Line2D>,
    edge: u32,
    p0: Vector2,
    p1: Vector2,
    kind: EdgeKind,
    style: FoldStyle,
    fold_line_len: f32,
) {
    let edge_vec = p1 - p0;
    let v_len = edge_vec.magnitude();
    if v_len < 1e-9 {
        return;
    }
    let vn = edge_vec * (fold_line_len / v_len);
    let mut seg = |a: Vector2, b: Vector2| {
        folds.push(Line2D {
            edge,
            p0: [a.x, a.y],
            p1: [b.x, b.y],
            kind,
        });
    };
    match style {
        FoldStyle::None => {}
        FoldStyle::Full => seg(p0, p1),
        FoldStyle::FullAndOut => seg(p0 - vn, p1 + vn),
        FoldStyle::Out => {
            seg(p0 - vn, p0);
            seg(p1, p1 + vn);
        }
        FoldStyle::In => {
            seg(p0, p0 + vn);
            seg(p1 - vn, p1);
        }
        FoldStyle::InAndOut => {
            seg(p0 - vn, p0 + vn);
            seg(p1 - vn, p1 + vn);
        }
    }
}

/// Position an edge-id label next to a cut edge (mirrors `CutDescription`).
fn edge_id_label(
    p0: Vector2,
    p1: Vector2,
    n_flap: Option<Vector2>,
    pos_mode: EdgeIdPosition,
    size_mm: f32,
    text: String,
) -> Option<Text2D> {
    let mut center = (p0 + p1) * 0.5;
    let voffs = match (pos_mode, n_flap) {
        (EdgeIdPosition::Inside, None) => -0.2,
        (EdgeIdPosition::Inside, Some(_)) => 1.0,
        (EdgeIdPosition::Outside, None) => 1.0,
        (EdgeIdPosition::Outside, Some(n)) => {
            center += n;
            1.0
        }
        (EdgeIdPosition::None, _) => return None,
    };
    let dir = (p1 - p0).normalize();
    let normal = Vector2::new(-dir.y, dir.x);
    let pos = center + size_mm * voffs * normal;
    // Keep text upright (within ±90°).
    let mut angle = dir.y.atan2(dir.x);
    if angle > FRAC_PI_2 {
        angle -= PI;
    } else if angle < -FRAC_PI_2 {
        angle += PI;
    }
    Some(Text2D {
        pos: [pos.x, pos.y],
        angle,
        size: size_mm,
        text,
    })
}

/// Compute the placed 2D geometry of one island.
fn island_geom(pc: &Papercraft, island: &super::Island) -> IslandGeom {
    let model = pc.model();
    let options = pc.options();
    let scale = options.scale;
    let flap_style = options.flap_style;
    let flap_double = options.flap_double;
    let fold_style = options.fold_style;
    let fold_line_len = options.fold_line_len;
    let id_pos = options.edge_id_position;
    let id_size = options.edge_id_font_size * 25.4 / 72.0; // pt -> mm

    let mut g = IslandGeom::default();

    let _ = pc.traverse_faces(island, |i_face, face, matrix| {
        let plane = model.face_plane(face);
        let project = |i_v: VertexIndex| -> Vector2 {
            let p = plane.project(&model[i_v].pos(), scale);
            matrix.transform_point(Point2::from_vec(p)).to_vec()
        };
        let arr = |v: Vector2| [v.x, v.y];

        let vs = face.index_vertices();
        g.triangles
            .push([arr(project(vs[0])), arr(project(vs[1])), arr(project(vs[2]))]);

        for (i_v0, i_v1, i_edge) in face.vertices_with_edges() {
            let status = pc.edge_status(i_edge);
            let edge = &model[i_edge];
            let edge_u32 = usize::from(i_edge) as u32;
            let pos0 = project(i_v0);
            let pos1 = project(i_v1);

            let other_face = match edge.faces() {
                (fa, Some(fb)) if i_face == fb => Some(fa),
                (fa, Some(fb)) if i_face == fa => Some(fb),
                (_, None) => None,
                _ => continue,
            };

            // Edge-id label text (shared helper closure).
            let label = |g: &mut IslandGeom, n_flap: Option<Vector2>| {
                if id_pos == EdgeIdPosition::None {
                    return;
                }
                let (Some(i_face_b), Some(id)) = (other_face, pc.edge_id(i_edge)) else {
                    return;
                };
                let name = pc
                    .island_by_key(pc.island_by_face(i_face_b))
                    .map_or("?", |i| i.name());
                if let Some(t) =
                    edge_id_label(pos0, pos1, n_flap, id_pos, id_size, format!("{name}:{id}"))
                {
                    g.texts.push(t);
                }
            };

            match status {
                EdgeStatus::Hidden | EdgeStatus::SoftHidden => {}
                EdgeStatus::Joined => {
                    if !edge.face_sign(i_face) {
                        continue;
                    }
                    push_fold(
                        &mut g.folds,
                        edge_u32,
                        pos0,
                        pos1,
                        fold_kind(edge.angle().0.is_sign_negative()),
                        fold_style,
                        fold_line_len,
                    );
                }
                EdgeStatus::Cut(c) => {
                    let flap_here = match flap_style {
                        FlapStyle::None => false,
                        _ => flap_double && other_face.is_some() || c.flap_visible(edge.face_sign(i_face)),
                    };

                    if flap_here {
                        // The flap folds along this edge; the tab outline is the cut.
                        push_fold(
                            &mut g.folds,
                            edge_u32,
                            pos0,
                            pos1,
                            fold_kind(edge.angle().0.is_sign_negative()),
                            fold_style,
                            fold_line_len,
                        );

                        let fg = pc.flat_face_flap_dimensions(i_face, other_face, i_edge);
                        let edge_vec = pos1 - pos0;
                        let v_len = edge_vec.magnitude();
                        if v_len > 1e-6 {
                            let vn = edge_vec * (fg.width / v_len);
                            let v0 = vn * fg.tan_0;
                            let v1 = vn * fg.tan_1;
                            let normal = Vector2::new(-vn.y, vn.x);
                            let flap = if fg.triangular {
                                vec![arr(pos0), arr(pos0 + normal + v1), arr(pos1)]
                            } else {
                                vec![
                                    arr(pos0),
                                    arr(pos0 + normal + v1),
                                    arr(pos1 + normal - v0),
                                    arr(pos1),
                                ]
                            };
                            g.flaps.push(flap);
                            label(&mut g, Some(normal));
                        }
                    } else {
                        g.cuts.push(Line2D {
                            edge: edge_u32,
                            p0: arr(pos0),
                            p1: arr(pos1),
                            kind: EdgeKind::Cut,
                        });
                        label(&mut g, None);
                    }
                }
            }
        }
        ControlFlow::Continue(())
    });

    g
}

#[derive(Serialize)]
pub struct Pieces2D {
    pub pieces: Vec<Piece2D>,
}

#[derive(Serialize)]
pub struct Piece2D {
    pub id: u32,
    pub triangles: Vec<[[f32; 2]; 3]>,
    pub cuts: Vec<Line2D>,
    pub folds: Vec<Line2D>,
    pub flaps: Vec<Vec<[f32; 2]>>,
    pub texts: Vec<Text2D>,
}

/// The full 2D net: every island placed on the infinite paper.
pub fn pieces_2d(pc: &Papercraft) -> Pieces2D {
    let pieces = pc
        .islands()
        .enumerate()
        .map(|(id, (_key, island))| {
            let g = island_geom(pc, island);
            Piece2D {
                id: id as u32,
                triangles: g.triangles,
                cuts: g.cuts,
                folds: g.folds,
                flaps: g.flaps,
                texts: g.texts,
            }
        })
        .collect();
    Pieces2D { pieces }
}

// ---------------------------------------------------------------------------
// Vector export (SVG + PDF)
// ---------------------------------------------------------------------------

const MM_TO_PT: f32 = 72.0 / 25.4;

fn color_rgb(c: &super::MyColor) -> (f32, f32, f32) {
    (c.0.r, c.0.g, c.0.b)
}

/// One island's geometry assigned to a page, with page-local coordinates.
struct Placed {
    page: u32,
    geom: IslandGeom,
}

fn placed_islands(pc: &Papercraft) -> Vec<Placed> {
    let options = pc.options();
    let page_cols = options.page_cols;
    pc.islands()
        .map(|(_key, island)| {
            let po = options.global_to_page(island.location());
            let page = (po.row.max(0) as u32) * page_cols + (po.col.max(0) as u32);
            let origin = options.page_position(page);
            let mut geom = island_geom(pc, island);
            let shift = |p: &mut [f32; 2]| {
                p[0] -= origin.x;
                p[1] -= origin.y;
            };
            for t in &mut geom.triangles {
                t.iter_mut().for_each(shift);
            }
            for l in &mut geom.cuts {
                shift(&mut l.p0);
                shift(&mut l.p1);
            }
            for l in &mut geom.folds {
                shift(&mut l.p0);
                shift(&mut l.p1);
            }
            for f in &mut geom.flaps {
                f.iter_mut().for_each(shift);
            }
            for t in &mut geom.texts {
                shift(&mut t.pos);
            }
            Placed { page, geom }
        })
        .collect()
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Render the whole document to a single SVG laying every page out in the page
/// grid (handy for a print preview that matches the desktop layout).
pub fn export_svg(pc: &Papercraft) -> Vec<u8> {
    let options = pc.options();
    let (pw, ph) = options.page_size;
    let n_pages = options.pages.max(1);
    let page_cols = options.page_cols.max(1);
    let page_rows = n_pages.div_ceil(page_cols);

    let placed = placed_islands(pc);
    let cut = color_rgb(&options.cut_line_color);
    let fold = color_rgb(&options.fold_line_color);
    let tab = color_rgb(&options.tab_line_color);
    let cut_w = options.cut_line_width;
    let fold_w = options.fold_line_width;
    let tab_w = options.tab_line_width;

    let total_w = page_cols as f32 * pw + (page_cols as f32 - 1.0) * 5.0;
    let total_h = page_rows as f32 * ph + (page_rows as f32 - 1.0) * 5.0;

    let mut s = String::new();
    let _ = write!(
        s,
        r#"<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}mm" height="{total_h}mm" viewBox="0 0 {total_w} {total_h}" version="1.1">
"#
    );

    for page in 0..n_pages {
        let o = options.page_position(page);
        let _ = writeln!(
            s,
            r##"<rect x="{}" y="{}" width="{pw}" height="{ph}" fill="white" stroke="#cccccc" stroke-width="0.2"/>"##,
            o.x, o.y
        );
    }

    let rgb = |c: (f32, f32, f32)| {
        format!("rgb({},{},{})", (c.0 * 255.0) as u8, (c.1 * 255.0) as u8, (c.2 * 255.0) as u8)
    };
    let line = |s: &mut String, a: [f32; 2], b: [f32; 2], o: Vector2, col: String, w: f32, dash: bool| {
        let d = if dash { r#" stroke-dasharray="2,2""# } else { "" };
        let _ = writeln!(
            s,
            r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{col}" stroke-width="{w}"{d}/>"#,
            a[0] + o.x, a[1] + o.y, b[0] + o.x, b[1] + o.y,
        );
    };

    for placed in &placed {
        let o = options.page_position(placed.page);
        for f in &placed.geom.flaps {
            for w in f.windows(2) {
                line(&mut s, w[0], w[1], o, rgb(tab), tab_w, false);
            }
        }
        for l in &placed.geom.folds {
            line(&mut s, l.p0, l.p1, o, rgb(fold), fold_w, l.kind == EdgeKind::Valley);
        }
        for l in &placed.geom.cuts {
            line(&mut s, l.p0, l.p1, o, rgb(cut), cut_w, false);
        }
        for t in &placed.geom.texts {
            let (x, y) = (t.pos[0] + o.x, t.pos[1] + o.y);
            let deg = t.angle.to_degrees();
            let _ = writeln!(
                s,
                r#"<text x="{x}" y="{y}" font-family="Helvetica, sans-serif" font-size="{}" text-anchor="middle" dominant-baseline="central" transform="rotate({deg} {x} {y})">{}</text>"#,
                t.size, xml_escape(&t.text),
            );
        }
    }

    s.push_str("</svg>\n");
    s.into_bytes()
}

/// Render the whole document to a multi-page vector PDF (no raster background).
pub fn export_pdf(pc: &Papercraft) -> Vec<u8> {
    use lopdf::{Document, Object, Stream, dictionary};

    let options = pc.options();
    let (pw_mm, ph_mm) = options.page_size;
    let n_pages = options.pages.max(1);
    let pw_pt = pw_mm * MM_TO_PT;
    let ph_pt = ph_mm * MM_TO_PT;

    let cut = color_rgb(&options.cut_line_color);
    let fold = color_rgb(&options.fold_line_color);
    let tab = color_rgb(&options.tab_line_color);
    let cut_w = options.cut_line_width * MM_TO_PT;
    let fold_w = options.fold_line_width * MM_TO_PT;
    let tab_w = options.tab_line_width * MM_TO_PT;

    let placed = placed_islands(pc);

    let px = |x: f32| x * MM_TO_PT;
    let py = |y: f32| ph_pt - y * MM_TO_PT;

    let mut doc = Document::with_version("1.4");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let mut kids: Vec<Object> = Vec::new();

    for page in 0..n_pages {
        let mut c = String::new();
        let _ = writeln!(c, "1 J 1 j");

        let stroke = |c: &mut String, rgb: (f32, f32, f32), w: f32, dash: bool| {
            let _ = writeln!(c, "{} {} {} RG {w} w", rgb.0, rgb.1, rgb.2);
            if dash {
                let _ = writeln!(c, "[{} {}] 0 d", 2.0 * MM_TO_PT, 2.0 * MM_TO_PT);
            } else {
                c.push_str("[] 0 d\n");
            }
        };
        let seg = |c: &mut String, a: [f32; 2], b: [f32; 2]| {
            let _ = writeln!(c, "{} {} m {} {} l S", px(a[0]), py(a[1]), px(b[0]), py(b[1]));
        };
        let poly = |c: &mut String, pts: &[[f32; 2]]| {
            if let Some((first, rest)) = pts.split_first() {
                let _ = write!(c, "{} {} m ", px(first[0]), py(first[1]));
                for p in rest {
                    let _ = write!(c, "{} {} l ", px(p[0]), py(p[1]));
                }
                c.push_str("S\n");
            }
        };

        let page_islands: Vec<&Placed> = placed.iter().filter(|p| p.page == page).collect();

        for placed in &page_islands {
            stroke(&mut c, tab, tab_w, false);
            for f in &placed.geom.flaps {
                poly(&mut c, f);
            }
            for l in &placed.geom.folds {
                stroke(&mut c, fold, fold_w, l.kind == EdgeKind::Valley);
                seg(&mut c, l.p0, l.p1);
            }
            stroke(&mut c, cut, cut_w, false);
            for l in &placed.geom.cuts {
                seg(&mut c, l.p0, l.p1);
            }
        }

        // Edge-id labels (black text).
        let mut has_text = false;
        for placed in &page_islands {
            for t in &placed.geom.texts {
                if !has_text {
                    c.push_str("0 0 0 rg\n");
                    has_text = true;
                }
                let size_pt = t.size * MM_TO_PT;
                // PDF y is up, so the paper rotation is negated.
                let (sn, cs) = (-t.angle).sin_cos();
                // Approximate centering: shift left by half the text width.
                let half_w = 0.5 * t.text.chars().count() as f32 * size_pt * 0.5;
                let ox = px(t.pos[0]) - half_w * cs;
                let oy = py(t.pos[1]) - half_w * sn;
                let _ = writeln!(
                    c,
                    "BT /F1 {size_pt} Tf {cs} {sn} {} {cs} {ox} {oy} Tm ({}) Tj ET",
                    -sn,
                    pdf_escape(&t.text),
                );
            }
        }

        let content_id = doc.add_object(Stream::new(dictionary! {}, c.into_bytes()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "MediaBox" => vec![0.0.into(), 0.0.into(), pw_pt.into(), ph_pt.into()],
            "Resources" => dictionary! {
                "Font" => dictionary! { "F1" => font_id },
            },
        });
        kids.push(page_id.into());
    }

    let count = kids.len() as i64;
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => count,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    let mut buf = Vec::new();
    let _ = doc.save_to(&mut buf);
    buf
}

fn pdf_escape(s: &str) -> String {
    s.replace('\\', r"\\").replace('(', r"\(").replace(')', r"\)")
}
