//! Tangent vector generation via the MikkTSpace algorithm.
//!
//! VRM 1.0 spec: "TANGENT is not exported; recompute with MikkTSpace on import."
//! UniVRM reference: MToon_GetTangentToWorld() in vrmc_materials_mtoon_utility.hlsl.

use glam::Vec4;

use super::types::IrMesh;

/// Threshold used to decide whether the xyz components of a tangent are valid (non-degenerate).
const TANGENT_VALID_THRESHOLD: f32 = 1e-8;

/// Wrapper that implements the MikkTSpace `Geometry` trait.
struct MikkGeometry<'a> {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: &'a [u32],
    /// Tangent vectors per corner (corner = face * 3 + vert).
    corner_tangents: Vec<[f32; 4]>,
}

impl<'a> mikktspace::Geometry for MikkGeometry<'a> {
    fn num_faces(&self) -> usize {
        self.indices.len() / 3
    }

    fn num_vertices_of_face(&self, _face: usize) -> usize {
        3
    }

    fn position(&self, face: usize, vert: usize) -> [f32; 3] {
        self.positions[self.indices[face * 3 + vert] as usize]
    }

    fn normal(&self, face: usize, vert: usize) -> [f32; 3] {
        self.normals[self.indices[face * 3 + vert] as usize]
    }

    fn tex_coord(&self, face: usize, vert: usize) -> [f32; 2] {
        self.uvs[self.indices[face * 3 + vert] as usize]
    }

    fn set_tangent_encoded(&mut self, tangent: [f32; 4], face: usize, vert: usize) {
        self.corner_tangents[face * 3 + vert] = tangent;
    }
}

/// Whether the xyz components of a tangent are valid.
#[inline]
fn has_valid_tangent(tangent: Vec4) -> bool {
    tangent.truncate().length_squared() > TANGENT_VALID_THRESHOLD
}

/// Whether the referenced positions span a usable tangent basis.
///
/// mikktspace-0.2.0 panics (index out of bounds in `GenerateSharedVerticesIndexList`)
/// when every referenced position is identical or non-finite: the vertex-merge
/// grid divides by the extent of the widest axis, `(fVal - fMin) / (fMax - fMin)`
/// becomes NaN, every entry hashes into cell 0, and the next cell's offset lands
/// one past the hash-table end *before* the empty-cell check. Zero-extent meshes
/// are real data — MMD models often collapse hidden parts to a single point and
/// expand them via vertex morphs — so detect them here and skip generation
/// (tangents are meaningless without positional extent anyway).
fn has_tangent_basis(positions: &[[f32; 3]], indices: &[u32]) -> bool {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for &vi in indices {
        let Some(p) = positions.get(vi as usize) else {
            return false; // out-of-range index: do not trust the geometry
        };
        for c in 0..3 {
            if !p[c].is_finite() {
                return false; // NaN/Inf positions also break the merge grid
            }
            min[c] = min[c].min(p[c]);
            max[c] = max[c].max(p[c]);
        }
    }
    (0..3).any(|c| max[c] > min[c])
}

/// Generate MikkTSpace tangents for every vertex of an `IrMesh`.
///
/// `normal_tex_coord` is the TEXCOORD set index referenced by `normalTexture`.
/// When texCoord=1 and UV1 exists, UV1 is used for tangent generation.
/// Vertices that already have a valid tangent (|tangent.xyz| above the threshold) are skipped.
/// If MikkTSpace generation fails, a default tangent (1, 0, 0, 1) is assigned.
///
/// Vertices that share corners with conflicting handedness (w) are automatically split.
pub fn generate_tangents(mesh: &mut IrMesh, normal_tex_coord: u32) {
    // Skip if every vertex already has a valid tangent
    let needs_generation = mesh.vertices.iter().any(|v| !has_valid_tangent(v.tangent));
    if !needs_generation {
        return;
    }

    let vertex_count = mesh.vertices.len();
    let positions: Vec<[f32; 3]> = mesh
        .vertices
        .iter()
        .map(|v| v.position.to_array())
        .collect();
    let normals: Vec<[f32; 3]> = mesh.vertices.iter().map(|v| v.normal.to_array()).collect();
    // Generate tangents using the UV set referenced by normalTexture.texCoord.
    // When UV1 is absent, fall back to zero UV (matches the renderer in mesh.rs and UniVRM MeshData.cs).
    // texCoord values on materials are shared across meshes, so we never rewrite the material side.
    let uvs: Vec<[f32; 2]> = if normal_tex_coord == 1 {
        if mesh.uvs1.len() == vertex_count {
            mesh.uvs1.clone()
        } else {
            log::debug!("Tangent generation: texCoord=1 but UV1 absent, using zero UV");
            vec![[0.0, 0.0]; vertex_count]
        }
    } else {
        mesh.vertices.iter().map(|v| v.uv.to_array()).collect()
    };

    let corner_count = mesh.indices.len();

    // Degenerate geometry (all points identical / NaN) panics inside
    // mikktspace-0.2.0 -- fall back to default tangents instead.
    if !has_tangent_basis(&positions, &mesh.indices) {
        log::info!(
            "MikkTSpace skipped (degenerate geometry, e.g. morph-collapsed part): mesh='{}' - using default tangents",
            mesh.name
        );
        for v in mesh.vertices_mut() {
            if !has_valid_tangent(v.tangent) {
                v.tangent = Vec4::new(1.0, 0.0, 0.0, 1.0);
            }
        }
        return;
    }

    // MikkTSpace generation (block scope keeps the indices borrow tight)
    let corner_tangents_opt = {
        let mut geom = MikkGeometry {
            positions,
            normals,
            uvs,
            indices: &mesh.indices,
            corner_tangents: vec![[0.0; 4]; corner_count],
        };
        // Safety net for other panics in the mikktspace C-to-Rust port: treat a
        // panic like a normal generation failure (default tangents). On panic,
        // `geom` is discarded untouched, so AssertUnwindSafe is sound here.
        let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            mikktspace::generate_tangents(&mut geom)
        }))
        .unwrap_or_else(|_| {
            log::warn!(
                "MikkTSpace panicked during tangent generation: mesh='{}' - using default tangents",
                mesh.name
            );
            false
        });
        if ok {
            Some(geom.corner_tangents)
        } else {
            None
        }
    };
    // geom is dropped here, releasing the borrow on mesh.indices

    if let Some(corner_tangents) = corner_tangents_opt {
        // --- Vertex splitting due to w mismatches ---
        // Group each vertex's corners into positive-w and negative-w buckets
        let mut split_count = 0usize;
        let mut vert_neg_corners: Vec<Vec<usize>> = vec![Vec::new(); vertex_count];
        let mut vert_pos_corners: Vec<Vec<usize>> = vec![Vec::new(); vertex_count];

        for (corner, &vi) in mesh.indices.iter().enumerate() {
            let vi = vi as usize;
            // Vertices that already carry a valid glTF-supplied tangent never need splitting
            if has_valid_tangent(mesh.vertices[vi].tangent) {
                continue;
            }
            if corner_tangents[corner][3] >= 0.0 {
                vert_pos_corners[vi].push(corner);
            } else {
                vert_neg_corners[vi].push(corner);
            }
        }

        for vi in 0..vertex_count {
            if vert_pos_corners[vi].is_empty() || vert_neg_corners[vi].is_empty() {
                continue; // All corners agree on w -> no split needed
            }
            // Remap the minority corners to a freshly cloned vertex
            let (minority_corners, minority_w) =
                if vert_pos_corners[vi].len() <= vert_neg_corners[vi].len() {
                    (&vert_pos_corners[vi], 1.0f32)
                } else {
                    (&vert_neg_corners[vi], -1.0f32)
                };

            let vert_copy = mesh.vertices[vi];
            let new_vi = mesh.vertices.len() as u32;
            mesh.vertices_mut().push(vert_copy);
            if !mesh.uvs1.is_empty() {
                let uv1 = if vi < mesh.uvs1.len() {
                    mesh.uvs1[vi]
                } else {
                    [0.0, 0.0]
                };
                mesh.uvs1.push(uv1);
            }
            // Duplicate morph-target offsets for the new vertex index
            for mt in mesh.morph_targets_mut() {
                if let Some(&(_, offset)) = mt
                    .position_offsets
                    .iter()
                    .find(|(idx, _)| *idx == vi as u32)
                {
                    mt.position_offsets.push((new_vi, offset));
                }
                if let Some(&(_, offset)) =
                    mt.normal_offsets.iter().find(|(idx, _)| *idx == vi as u32)
                {
                    mt.normal_offsets.push((new_vi, offset));
                }
                if let Some(&(_, offset)) =
                    mt.tangent_offsets.iter().find(|(idx, _)| *idx == vi as u32)
                {
                    mt.tangent_offsets.push((new_vi, offset));
                }
            }
            // Rewrite the minority corner indices to point at the new vertex
            for &corner in minority_corners {
                mesh.indices_mut()[corner] = new_vi;
            }
            split_count += 1;
            log::trace!(
                "tangent w split: mesh='{}' vertex={} -> new={} (minority_w={:.0})",
                mesh.name,
                vi,
                new_vi,
                minority_w
            );
        }

        if split_count > 0 {
            log::info!(
                "MikkTSpace w mismatch vertex split: mesh='{}' splits={} ({}->{}vertices)",
                mesh.name,
                split_count,
                vertex_count,
                mesh.vertices.len()
            );
        }

        // --- Aggregate corner tangents into per-vertex tangents ---
        let new_vertex_count = mesh.vertices.len();
        let mut tangent_acc: Vec<([f32; 3], f32, u32)> = vec![([0.0; 3], 1.0, 0); new_vertex_count];

        for (corner, &vi) in mesh.indices.iter().enumerate() {
            let vi = vi as usize;
            if has_valid_tangent(mesh.vertices[vi].tangent) {
                continue; // Preserve glTF-supplied tangent
            }
            let ct = &corner_tangents[corner];
            let acc = &mut tangent_acc[vi];
            acc.0[0] += ct[0];
            acc.0[1] += ct[1];
            acc.0[2] += ct[2];
            acc.1 = ct[3]; // w (consistent per vertex after splitting)
            acc.2 += 1;
        }

        for (i, v) in mesh.vertices_mut().iter_mut().enumerate() {
            if has_valid_tangent(v.tangent) {
                continue; // Already populated from glTF
            }
            let (xyz, w, count) = &tangent_acc[i];
            if *count > 0 {
                let inv = 1.0 / (*count as f32);
                let tx = xyz[0] * inv;
                let ty = xyz[1] * inv;
                let tz = xyz[2] * inv;
                let len = (tx * tx + ty * ty + tz * tz).sqrt();
                if len > TANGENT_VALID_THRESHOLD {
                    v.tangent = Vec4::new(tx / len, ty / len, tz / len, *w);
                } else {
                    v.tangent = Vec4::new(1.0, 0.0, 0.0, 1.0);
                }
            } else {
                // Vertex MikkTSpace did not produce (e.g. isolated vertex)
                v.tangent = Vec4::new(1.0, 0.0, 0.0, 1.0);
            }
        }
        log::debug!(
            "MikkTSpace tangent generation complete: mesh='{}' vertices={}",
            mesh.name,
            new_vertex_count
        );
    } else {
        log::warn!(
            "MikkTSpace tangent generation failed: mesh='{}' - using default tangents",
            mesh.name
        );
        for v in mesh.vertices_mut() {
            if !has_valid_tangent(v.tangent) {
                v.tangent = Vec4::new(1.0, 0.0, 0.0, 1.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intermediate::types::{IrMesh, IrVertex};
    use glam::{Vec2, Vec3};
    use std::sync::Arc;

    fn make_vertex(position: Vec3, uv: Vec2) -> IrVertex {
        IrVertex {
            position,
            normal: Vec3::Y,
            uv,
            tangent: Vec4::ZERO,
            weights: [(0, 0.0); 4],
            weight_count: 0,
            edge_scale: 1.0,
        }
    }

    fn make_mesh(vertices: Vec<IrVertex>, indices: Vec<u32>) -> IrMesh {
        IrMesh {
            name: "test".to_string(),
            vertices: Arc::new(vertices),
            indices: Arc::new(indices),
            material_index: 0,
            morph_targets: Arc::new(Vec::new()),
            node_index: 0,
            uvs1: Vec::new(),
        }
    }

    /// Regression test: a mesh whose every vertex sits at the same point
    /// (MMD parts collapsed to a point, expanded later by vertex morphs)
    /// used to panic inside mikktspace-0.2.0 with "index out of bounds".
    #[test]
    fn collapsed_mesh_gets_default_tangents_without_panic() {
        let p = Vec3::new(1.25, -3.5, 0.75);
        let vertices: Vec<IrVertex> = (0..9)
            .map(|i| make_vertex(p, Vec2::new(i as f32 * 0.1, 0.0)))
            .collect();
        let indices: Vec<u32> = (0..9).collect();
        let mut mesh = make_mesh(vertices, indices);

        generate_tangents(&mut mesh, 0);

        for v in mesh.vertices.iter() {
            assert_eq!(v.tangent, Vec4::new(1.0, 0.0, 0.0, 1.0));
        }
    }

    /// NaN positions also break the mikktspace merge grid -> default tangents.
    #[test]
    fn nan_position_gets_default_tangents_without_panic() {
        let mut vertices = vec![
            make_vertex(Vec3::ZERO, Vec2::new(0.0, 0.0)),
            make_vertex(Vec3::X, Vec2::new(1.0, 0.0)),
            make_vertex(Vec3::Z, Vec2::new(0.0, 1.0)),
        ];
        vertices[1].position = Vec3::new(f32::NAN, 0.0, 0.0);
        let mut mesh = make_mesh(vertices, vec![0, 1, 2]);

        generate_tangents(&mut mesh, 0);

        for v in mesh.vertices.iter() {
            assert_eq!(v.tangent, Vec4::new(1.0, 0.0, 0.0, 1.0));
        }
    }

    /// A well-formed triangle still gets real MikkTSpace tangents.
    #[test]
    fn regular_mesh_generates_valid_tangents() {
        let vertices = vec![
            make_vertex(Vec3::ZERO, Vec2::new(0.0, 0.0)),
            make_vertex(Vec3::X, Vec2::new(1.0, 0.0)),
            make_vertex(Vec3::Z, Vec2::new(0.0, 1.0)),
        ];
        let mut mesh = make_mesh(vertices, vec![0, 1, 2]);

        generate_tangents(&mut mesh, 0);

        for v in mesh.vertices.iter() {
            assert!(has_valid_tangent(v.tangent), "tangent = {:?}", v.tangent);
        }
    }

    #[test]
    fn has_tangent_basis_detects_extent() {
        // Zero extent
        let flat = vec![[1.0f32, 2.0, 3.0]; 3];
        assert!(!has_tangent_basis(&flat, &[0, 1, 2]));
        // Positive extent
        let spread = vec![[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        assert!(has_tangent_basis(&spread, &[0, 1, 2]));
        // Non-finite
        let nan = vec![[f32::NAN, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        assert!(!has_tangent_basis(&nan, &[0, 1, 2]));
        // Out-of-range index
        assert!(!has_tangent_basis(&spread, &[0, 1, 5]));
    }
}
