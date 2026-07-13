//! Mesh simplification by iterative edge collapse (Garland–Heckbert quadric
//! error metrics).
//!
//! The entry point is [`simplify_papercraft`]: it welds the model's vertices by
//! position (papercraft models often duplicate vertices along UV seams),
//! collapses the cheapest edges until the requested triangle budget is met, and
//! rebuilds a fresh [`Papercraft`] through the regular importer pipeline, so the
//! result behaves exactly like a newly imported model.
//!
//! Simplification is geometry-only: textures, UVs and per-face materials are
//! dropped (the same as importing an STL). Boundary edges of open meshes are
//! held in place with constraint quadrics so holes don't grow.

use std::collections::BinaryHeap;

use anyhow::{Result, bail};
use cgmath::InnerSpace;

use super::formats::Importer;
use super::{MaterialIndex, Model, Texture, Vertex, VertexIndex};
use crate::paper::Papercraft;
use crate::util_3d::{Vector2, Vector3};

type V3 = cgmath::Vector3<f64>;

/// A simplified triangle soup: welded positions plus triangle indices.
pub struct SimplifiedMesh {
    pub positions: Vec<Vector3>,
    pub triangles: Vec<[u32; 3]>,
}

/// Simplify `pc`'s model down to (about) `target_faces` triangles and build a
/// new document from the result. The new document is fully cut, like a freshly
/// imported model: call [`Papercraft::unwrap`] afterwards.
pub fn simplify_papercraft(pc: &Papercraft, target_faces: usize) -> Result<Papercraft> {
    // Odd meshes can panic deep in the rebuild; contain it so the caller gets a
    // clean error (same contract as `import_model_bytes`).
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        simplify_papercraft_priv(pc, target_faces)
    })) {
        Ok(res) => res,
        Err(err) => {
            let msg = err
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| err.downcast_ref::<String>().cloned());
            match msg {
                Some(msg) => bail!("Panic simplifying the model: {msg}"),
                None => bail!("Panic simplifying the model"),
            }
        }
    }
}

fn simplify_papercraft_priv(pc: &Papercraft, target_faces: usize) -> Result<Papercraft> {
    let model = pc.model();
    if model.num_faces() == 0 {
        bail!("The model has no faces");
    }
    if target_faces >= model.num_faces() {
        bail!(
            "Target face count ({}) is not below the current face count ({})",
            target_faces,
            model.num_faces()
        );
    }
    let mesh = simplify_model(model, target_faces);
    if mesh.triangles.len() < 4 {
        bail!("Simplification left too little of the mesh; try a larger target");
    }
    Ok(Papercraft::import(SimplifiedImporter::new(mesh)))
}

/// Weld `model`'s vertices by exact position and run [`simplify`].
pub fn simplify_model(model: &Model, target_faces: usize) -> SimplifiedMesh {
    // Weld by bit pattern: seam-duplicated vertices come from the same source
    // coordinates, so they are bitwise identical.
    let mut weld: fxhash::FxHashMap<[u32; 3], u32> = Default::default();
    let mut positions: Vec<Vector3> = Vec::new();
    let mut remap: Vec<u32> = Vec::with_capacity(model.num_vertices());
    for (_, v) in model.vertices() {
        let p = v.pos();
        let key = [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()];
        let idx = *weld.entry(key).or_insert_with(|| {
            positions.push(p);
            positions.len() as u32 - 1
        });
        remap.push(idx);
    }
    let triangles: Vec<[u32; 3]> = model
        .faces()
        .map(|(_, f)| f.index_vertices().map(|iv| remap[usize::from(iv)]))
        .collect();
    simplify(&positions, &triangles, target_faces)
}

// ---------------------------------------------------------------------------
// Quadrics
// ---------------------------------------------------------------------------

/// A symmetric 4x4 quadric matrix, stored as the upper triangle:
/// `[a², ab, ac, ad, b², bc, bd, c², cd, d²]` for the plane `ax+by+cz+d=0`.
#[derive(Clone, Copy, Default)]
struct Quadric([f64; 10]);

impl Quadric {
    fn from_plane(n: V3, d: f64, w: f64) -> Quadric {
        Quadric([
            w * n.x * n.x,
            w * n.x * n.y,
            w * n.x * n.z,
            w * n.x * d,
            w * n.y * n.y,
            w * n.y * n.z,
            w * n.y * d,
            w * n.z * n.z,
            w * n.z * d,
            w * d * d,
        ])
    }
    fn add(&mut self, o: &Quadric) {
        for (a, b) in self.0.iter_mut().zip(&o.0) {
            *a += b;
        }
    }
    /// The error `vᵀQv` at position `v`.
    fn error(&self, v: V3) -> f64 {
        let q = &self.0;
        q[0] * v.x * v.x
            + 2.0 * q[1] * v.x * v.y
            + 2.0 * q[2] * v.x * v.z
            + 2.0 * q[3] * v.x
            + q[4] * v.y * v.y
            + 2.0 * q[5] * v.y * v.z
            + 2.0 * q[6] * v.y
            + q[7] * v.z * v.z
            + 2.0 * q[8] * v.z
            + q[9]
    }
    /// Position minimizing the error, if the 3x3 block is invertible.
    fn optimum(&self) -> Option<V3> {
        let q = &self.0;
        let (a, b, c, e, f, g) = (q[0], q[1], q[2], q[4], q[5], q[7]);
        let det = a * (e * g - f * f) - b * (b * g - c * f) + c * (b * f - c * e);
        if det.abs() < 1e-12 {
            return None;
        }
        let (rx, ry, rz) = (-q[3], -q[6], -q[8]);
        let x = (rx * (e * g - f * f) - b * (ry * g - rz * f) + c * (ry * f - rz * e)) / det;
        let y = (a * (ry * g - rz * f) - rx * (b * g - c * f) + c * (b * rz - c * ry)) / det;
        let z = (a * (e * rz - f * ry) - b * (b * rz - c * ry) + rx * (b * f - c * e)) / det;
        let v = V3::new(x, y, z);
        if v.x.is_finite() && v.y.is_finite() && v.z.is_finite() {
            Some(v)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Edge-collapse simplifier
// ---------------------------------------------------------------------------

/// A candidate collapse in the priority queue. Entries are lazily invalidated:
/// each vertex has a version that bumps on every change, and stale entries are
/// discarded when popped.
struct Candidate {
    cost: f64,
    v0: u32,
    v1: u32,
    ver0: u32,
    ver1: u32,
    target: V3,
}
impl PartialEq for Candidate {
    fn eq(&self, o: &Self) -> bool {
        self.cost == o.cost
    }
}
impl Eq for Candidate {}
impl PartialOrd for Candidate {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Candidate {
    // Reversed: BinaryHeap is a max-heap, we want the cheapest collapse first.
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        o.cost.total_cmp(&self.cost)
    }
}

struct Simplifier {
    pos: Vec<V3>,
    quadric: Vec<Quadric>,
    version: Vec<u32>,
    alive_v: Vec<bool>,
    boundary_v: Vec<bool>,
    tris: Vec<[u32; 3]>,
    alive_t: Vec<bool>,
    /// Incident (possibly dead, lazily cleaned) triangles per vertex.
    vtx_tris: Vec<Vec<u32>>,
    heap: BinaryHeap<Candidate>,
    n_tris: usize,
}

/// Simplify an indexed triangle mesh down to (about) `target_faces` triangles.
/// Stops early if no more edges can be collapsed without breaking the mesh
/// (normal flips, non-manifold pinches).
pub fn simplify(positions: &[Vector3], triangles: &[[u32; 3]], target_faces: usize) -> SimplifiedMesh {
    let mut s = Simplifier::new(positions, triangles);
    s.run(target_faces.max(4));
    s.into_mesh()
}

impl Simplifier {
    fn new(positions: &[Vector3], triangles: &[[u32; 3]]) -> Simplifier {
        let nv = positions.len();
        let pos: Vec<V3> = positions
            .iter()
            .map(|p| V3::new(p.x as f64, p.y as f64, p.z as f64))
            .collect();
        // Keep only non-degenerate triangles.
        let tris: Vec<[u32; 3]> = triangles
            .iter()
            .copied()
            .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
            .collect();

        let mut vtx_tris: Vec<Vec<u32>> = vec![Vec::new(); nv];
        for (i, t) in tris.iter().enumerate() {
            for &v in t {
                vtx_tris[v as usize].push(i as u32);
            }
        }

        // Face quadrics, area-weighted.
        let mut quadric = vec![Quadric::default(); nv];
        for t in &tris {
            let (p0, p1, p2) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
            let cross = (p1 - p0).cross(p2 - p0);
            let area2 = cross.magnitude();
            if area2 < 1e-30 {
                continue;
            }
            let n = cross / area2;
            let q = Quadric::from_plane(n, -n.dot(p0), area2 / 2.0);
            for &v in t {
                quadric[v as usize].add(&q);
            }
        }

        // Count faces per undirected edge to find the boundary.
        let mut edge_faces: fxhash::FxHashMap<(u32, u32), u32> = Default::default();
        for t in &tris {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *edge_faces.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        let mut boundary_v = vec![false; nv];
        for (&(a, b), &n) in &edge_faces {
            if n == 1 {
                boundary_v[a as usize] = true;
                boundary_v[b as usize] = true;
                // Constrain boundary edges with a heavily weighted plane
                // through the edge, perpendicular to an adjacent face.
                let (pa, pb) = (pos[a as usize], pos[b as usize]);
                let edge = pb - pa;
                let i_t = vtx_tris[a as usize]
                    .iter()
                    .find(|&&t| tris[t as usize].contains(&b));
                if let Some(&i_t) = i_t {
                    let t = tris[i_t as usize];
                    let fp0 = pos[t[0] as usize];
                    let fn_ = (pos[t[1] as usize] - fp0).cross(pos[t[2] as usize] - fp0);
                    let n = edge.cross(fn_);
                    let len = n.magnitude();
                    if len > 1e-30 {
                        let n = n / len;
                        let q =
                            Quadric::from_plane(n, -n.dot(pa), edge.magnitude2() * 1000.0);
                        quadric[a as usize].add(&q);
                        quadric[b as usize].add(&q);
                    }
                }
            }
        }

        let n_tris = tris.len();
        let mut s = Simplifier {
            version: vec![0; nv],
            alive_v: vec![true; nv],
            boundary_v,
            alive_t: vec![true; n_tris],
            heap: BinaryHeap::with_capacity(edge_faces.len()),
            pos,
            quadric,
            tris,
            vtx_tris,
            n_tris,
        };
        for &(a, b) in edge_faces.keys() {
            s.push_candidate(a, b);
        }
        s
    }

    fn push_candidate(&mut self, v0: u32, v1: u32) {
        let mut q = self.quadric[v0 as usize];
        q.add(&self.quadric[v1 as usize]);
        let (p0, p1) = (self.pos[v0 as usize], self.pos[v1 as usize]);
        // Try the quadric optimum, fall back to the best of both ends and the
        // midpoint. Boundary vertices don't move onto interior positions:
        // collapsing an interior vertex into a boundary vertex keeps the rim.
        let mut best = (q.error(p0), p0);
        for cand in [q.error(p1), q.error((p0 + p1) / 2.0)]
            .into_iter()
            .zip([p1, (p0 + p1) / 2.0])
        {
            if cand.0 < best.0 {
                best = cand;
            }
        }
        match (self.boundary_v[v0 as usize], self.boundary_v[v1 as usize]) {
            (true, false) => best = (q.error(p0), p0),
            (false, true) => best = (q.error(p1), p1),
            (true, true) => {} // keep endpoint/midpoint choice
            (false, false) => {
                if let Some(opt) = q.optimum() {
                    let e = q.error(opt);
                    if e < best.0 {
                        best = (e, opt);
                    }
                }
            }
        }
        self.heap.push(Candidate {
            cost: best.0.max(0.0),
            v0,
            v1,
            ver0: self.version[v0 as usize],
            ver1: self.version[v1 as usize],
            target: best.1,
        });
    }

    /// Live neighbor vertices of `v`.
    fn neighbors(&self, v: u32) -> fxhash::FxHashSet<u32> {
        let mut out: fxhash::FxHashSet<u32> = Default::default();
        for &t in &self.vtx_tris[v as usize] {
            if self.alive_t[t as usize] {
                for &o in &self.tris[t as usize] {
                    if o != v {
                        out.insert(o);
                    }
                }
            }
        }
        out
    }

    /// Would moving `v` (and merging `v_gone` into it) to `target` flip or
    /// squash any surviving incident triangle?
    fn collapse_flips(&self, v: u32, v_gone: u32, target: V3) -> bool {
        for &it in self.vtx_tris[v as usize]
            .iter()
            .chain(&self.vtx_tris[v_gone as usize])
        {
            if !self.alive_t[it as usize] {
                continue;
            }
            let t = self.tris[it as usize];
            // Triangles containing both endpoints disappear in the collapse.
            if t.contains(&v) && t.contains(&v_gone) {
                continue;
            }
            let old: [V3; 3] = t.map(|i| self.pos[i as usize]);
            let new: [V3; 3] = t.map(|i| {
                if i == v || i == v_gone {
                    target
                } else {
                    self.pos[i as usize]
                }
            });
            let n_old = (old[1] - old[0]).cross(old[2] - old[0]);
            let n_new = (new[1] - new[0]).cross(new[2] - new[0]);
            // Reject flips and near-degenerate results.
            if n_new.dot(n_old) <= 0.0 || n_new.magnitude2() < 1e-24 * n_old.magnitude2().max(1.0)
            {
                return true;
            }
        }
        false
    }

    fn run(&mut self, target_faces: usize) {
        while self.n_tris > target_faces {
            let Some(c) = self.heap.pop() else { break };
            let (v0, v1) = (c.v0, c.v1);
            // Stale?
            if !self.alive_v[v0 as usize]
                || !self.alive_v[v1 as usize]
                || self.version[v0 as usize] != c.ver0
                || self.version[v1 as usize] != c.ver1
            {
                continue;
            }
            // Link condition: collapsing an edge whose endpoints share more
            // than two neighbors (one for boundary edges) pinches the surface.
            let shared: Vec<u32> = self
                .neighbors(v0)
                .intersection(&self.neighbors(v1))
                .copied()
                .collect();
            let n_shared_tris = self.vtx_tris[v0 as usize]
                .iter()
                .filter(|&&t| self.alive_t[t as usize] && self.tris[t as usize].contains(&v1))
                .count();
            if shared.len() > n_shared_tris {
                continue;
            }
            if self.collapse_flips(v0, v1, c.target) || self.collapse_flips(v1, v0, c.target) {
                continue;
            }

            // Merge v1 into v0 at the target position.
            self.pos[v0 as usize] = c.target;
            let q1 = self.quadric[v1 as usize];
            self.quadric[v0 as usize].add(&q1);
            self.boundary_v[v0 as usize] |= self.boundary_v[v1 as usize];
            self.alive_v[v1 as usize] = false;
            self.version[v0 as usize] += 1;
            self.version[v1 as usize] += 1;

            let v1_tris = std::mem::take(&mut self.vtx_tris[v1 as usize]);
            for it in v1_tris {
                if !self.alive_t[it as usize] {
                    continue;
                }
                let t = &mut self.tris[it as usize];
                if t.contains(&v0) {
                    // Shared triangle collapses away.
                    self.alive_t[it as usize] = false;
                    self.n_tris -= 1;
                } else {
                    for i in t.iter_mut() {
                        if *i == v1 {
                            *i = v0;
                        }
                    }
                    self.vtx_tris[v0 as usize].push(it);
                }
            }
            // Compact the adjacency of the surviving vertex.
            let alive_t = &self.alive_t;
            let list = &mut self.vtx_tris[v0 as usize];
            list.sort_unstable();
            list.dedup();
            list.retain(|&t| alive_t[t as usize]);

            // Refresh candidates around the merged vertex.
            for n in self.neighbors(v0) {
                self.push_candidate(v0, n);
            }
        }
    }

    fn into_mesh(self) -> SimplifiedMesh {
        let mut remap = vec![u32::MAX; self.pos.len()];
        let mut positions = Vec::new();
        let mut triangles = Vec::with_capacity(self.n_tris);
        for (it, t) in self.tris.iter().enumerate() {
            if !self.alive_t[it] {
                continue;
            }
            triangles.push(t.map(|v| {
                let r = &mut remap[v as usize];
                if *r == u32::MAX {
                    let p = self.pos[v as usize];
                    positions.push(Vector3::new(p.x as f32, p.y as f32, p.z as f32));
                    *r = positions.len() as u32 - 1;
                }
                *r
            }));
        }
        SimplifiedMesh {
            positions,
            triangles,
        }
    }
}

// ---------------------------------------------------------------------------
// Importer over the simplified soup
// ---------------------------------------------------------------------------

/// Feeds a [`SimplifiedMesh`] back through the regular import pipeline.
/// Geometry-only, like STL: one default texture, no UVs.
pub struct SimplifiedImporter {
    vertices: Vec<Vertex>,
    triangles: Vec<[u32; 3]>,
}

impl SimplifiedImporter {
    pub fn new(mesh: SimplifiedMesh) -> SimplifiedImporter {
        // Area-weighted vertex normals.
        let mut normals = vec![V3::new(0.0, 0.0, 0.0); mesh.positions.len()];
        for t in &mesh.triangles {
            let [p0, p1, p2] = t.map(|i| {
                let p = mesh.positions[i as usize];
                V3::new(p.x as f64, p.y as f64, p.z as f64)
            });
            let n = (p1 - p0).cross(p2 - p0);
            for &v in t {
                normals[v as usize] += n;
            }
        }
        let vertices = mesh
            .positions
            .iter()
            .zip(&normals)
            .map(|(p, n)| {
                let len = n.magnitude();
                let n = if len > 1e-30 { n / len } else { V3::new(0.0, 0.0, 1.0) };
                Vertex {
                    pos: *p,
                    normal: Vector3::new(n.x as f32, n.y as f32, n.z as f32),
                    uv: Vector2::new(0.0, 0.0),
                }
            })
            .collect();
        SimplifiedImporter {
            vertices,
            triangles: mesh.triangles,
        }
    }
}

impl Importer for SimplifiedImporter {
    // Vertices are already welded: the index is the identity.
    type VertexId = u32;

    fn vertex_map(&self, i_v: VertexIndex) -> u32 {
        usize::from(i_v) as u32
    }
    fn build_vertices(&self) -> (bool, Vec<Vertex>) {
        (true, self.vertices.clone())
    }
    fn face_count(&self) -> usize {
        self.triangles.len()
    }
    fn faces(&self) -> impl Iterator<Item = (impl AsRef<[VertexIndex]>, MaterialIndex)> + '_ {
        self.triangles
            .iter()
            .map(|t| (t.map(|v| VertexIndex::from(v as usize)), MaterialIndex::from(0usize)))
    }
    fn build_textures(&self) -> Vec<Texture> {
        vec![Texture::default()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A regular (2n)x(2n) grid of triangles on a slightly bumpy surface.
    fn grid(n: u32) -> (Vec<Vector3>, Vec<[u32; 3]>) {
        let mut pos = Vec::new();
        for y in 0..=n {
            for x in 0..=n {
                let (fx, fy) = (x as f32, y as f32);
                let z = 0.05 * ((fx * 0.7).sin() + (fy * 0.9).cos());
                pos.push(Vector3::new(fx, fy, z));
            }
        }
        let at = |x: u32, y: u32| y * (n + 1) + x;
        let mut tris = Vec::new();
        for y in 0..n {
            for x in 0..n {
                tris.push([at(x, y), at(x + 1, y), at(x + 1, y + 1)]);
                tris.push([at(x, y), at(x + 1, y + 1), at(x, y + 1)]);
            }
        }
        (pos, tris)
    }

    // An octahedron subdivided by projecting midpoints onto the unit sphere.
    fn sphere(subdiv: u32) -> (Vec<Vector3>, Vec<[u32; 3]>) {
        let mut pos = vec![
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(-1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            Vector3::new(0.0, -1.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(0.0, 0.0, -1.0),
        ];
        let mut tris: Vec<[u32; 3]> = vec![
            [0, 2, 4], [2, 1, 4], [1, 3, 4], [3, 0, 4],
            [2, 0, 5], [1, 2, 5], [3, 1, 5], [0, 3, 5],
        ];
        for _ in 0..subdiv {
            let mut midpoints: fxhash::FxHashMap<(u32, u32), u32> = Default::default();
            let mut next = Vec::with_capacity(tris.len() * 4);
            for t in &tris {
                let mut mid = [0u32; 3];
                for k in 0..3 {
                    let (a, b) = (t[k], t[(k + 1) % 3]);
                    mid[k] = *midpoints.entry((a.min(b), a.max(b))).or_insert_with(|| {
                        let m = (pos[a as usize] + pos[b as usize]).normalize();
                        pos.push(m);
                        pos.len() as u32 - 1
                    });
                }
                next.push([t[0], mid[0], mid[2]]);
                next.push([t[1], mid[1], mid[0]]);
                next.push([t[2], mid[2], mid[1]]);
                next.push(mid);
            }
            tris = next;
        }
        (pos, tris)
    }

    fn check_valid(m: &SimplifiedMesh) {
        for t in &m.triangles {
            assert!(t.iter().all(|&v| (v as usize) < m.positions.len()));
            assert!(t[0] != t[1] && t[1] != t[2] && t[0] != t[2]);
        }
    }

    #[test]
    fn simplifies_sphere_to_target() {
        let (pos, tris) = sphere(4); // 2048 triangles
        assert_eq!(tris.len(), 2048);
        let m = simplify(&pos, &tris, 200);
        check_valid(&m);
        assert!(m.triangles.len() <= 200, "got {}", m.triangles.len());
        assert!(m.triangles.len() > 50, "over-collapsed: {}", m.triangles.len());
        // All the vertices should stay near the unit sphere.
        for p in &m.positions {
            let r = p.magnitude();
            assert!((0.8..=1.2).contains(&r), "vertex drifted to radius {r}");
        }
    }

    #[test]
    fn preserves_open_mesh_boundary() {
        let (pos, tris) = grid(16); // 512 triangles, open borders
        let m = simplify(&pos, &tris, 100);
        check_valid(&m);
        assert!(m.triangles.len() <= 100, "got {}", m.triangles.len());
        // The four grid corners are boundary-constrained; they must survive.
        for corner in [
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(16.0, 0.0, 0.0),
            Vector3::new(0.0, 16.0, 0.0),
            Vector3::new(16.0, 16.0, 0.0),
        ] {
            let closest = m
                .positions
                .iter()
                .map(|p| {
                    let d = Vector2::new(p.x - corner.x, p.y - corner.y);
                    d.magnitude()
                })
                .fold(f32::INFINITY, f32::min);
            assert!(closest < 0.5, "corner {corner:?} moved by {closest}");
        }
    }

    #[test]
    fn target_above_current_is_a_noop_shape() {
        let (pos, tris) = sphere(2); // 128 triangles
        let m = simplify(&pos, &tris, 100000);
        check_valid(&m);
        assert_eq!(m.triangles.len(), 128);
    }
}
