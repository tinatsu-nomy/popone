use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::Vec3;

use crate::intermediate::types::{
    IrMaterial, IrMaterialColorBind, IrMesh, IrModel, IrMorph, IrMorphKind, IrPhysics, IrTexture,
    IrTextureTransformBind, IrVertex,
};

/// Build a fresh IrModel containing only the visible materials.
///
/// Keep only meshes / materials whose material_index is in `visible_mat_indices`,
/// and remap vertex-morph and group-morph indices accordingly.
pub fn build_filtered_ir(ir: &IrModel, visible_mat_indices: &HashSet<usize>) -> IrModel {
    // If every material is hidden, emit an empty PMX (with a warning).
    if visible_mat_indices.is_empty() {
        log::warn!("All materials are hidden. Exporting empty PMX.");
        return IrModel {
            name: ir.name.clone(),
            comment: ir.comment.clone(),
            bones: ir.bones.clone(),
            source_format: ir.source_format,
            node_to_bone: ir.node_to_bone.clone(),
            rig_type: ir.rig_type.clone(),
            humanoid_bone_count: ir.humanoid_bone_count,
            astance_result: ir.astance_result,
            ..Default::default()
        };
    }

    // -- Phase 1: material remap (old_mat_idx -> new_mat_idx) --
    let mut mat_remap: HashMap<usize, usize> = HashMap::new();
    let mut new_materials: Vec<IrMaterial> = Vec::new();
    for (old_idx, mat) in ir.materials.iter().enumerate() {
        if visible_mat_indices.contains(&old_idx) {
            mat_remap.insert(old_idx, new_materials.len());
            new_materials.push(mat.clone());
        }
    }

    // -- Phase 2: mesh filter + vertex remap table --
    // Record the global vertex offset of each source mesh.
    let mut old_mesh_vtx_start: Vec<usize> = Vec::with_capacity(ir.meshes.len());
    let mut offset = 0usize;
    for mesh in &ir.meshes {
        old_mesh_vtx_start.push(offset);
        offset += mesh.vertices.len();
    }
    let old_total_verts = offset;

    // old_global_vtx -> new_global_vtx remap (None = excluded).
    let mut vtx_remap: Vec<Option<usize>> = vec![None; old_total_verts];
    let mut new_meshes: Vec<IrMesh> = Vec::new();
    let mut new_vtx_offset = 0usize;

    for (mesh_i, mesh) in ir.meshes.iter().enumerate() {
        if !visible_mat_indices.contains(&mesh.material_index) {
            continue;
        }
        let new_mat_idx = mat_remap[&mesh.material_index];
        let old_start = old_mesh_vtx_start[mesh_i];

        // Register the vertex remap entries.
        for local_i in 0..mesh.vertices.len() {
            vtx_remap[old_start + local_i] = Some(new_vtx_offset + local_i);
        }

        new_meshes.push(IrMesh {
            name: mesh.name.clone(),
            vertices: Arc::new(
                mesh.vertices
                    .iter()
                    .map(|v| IrVertex {
                        position: v.position,
                        normal: v.normal,
                        uv: v.uv,
                        tangent: v.tangent,
                        weights: v.weights,
                        weight_count: v.weight_count,
                        edge_scale: v.edge_scale,
                    })
                    .collect(),
            ),
            indices: Arc::clone(&mesh.indices),
            material_index: new_mat_idx,
            morph_targets: Arc::clone(&mesh.morph_targets),
            node_index: mesh.node_index,
            uvs1: mesh.uvs1.clone(),
        });

        new_vtx_offset += mesh.vertices.len();
    }

    // -- Phase 3: morph liveness check --
    // Vertex morph: alive if at least one entry survives the remap.
    // Group morph: alive if at least one child morph is alive (resolved iteratively).
    let morph_count = ir.morphs.len();
    let mut morph_alive: Vec<bool> = vec![false; morph_count];

    // First, decide vertex-morph liveness (out-of-range entries are silently ignored via checked access).
    // A morph survives if positions / normals / tangents has at least one live vertex.
    for (i, morph) in ir.morphs.iter().enumerate() {
        if let IrMorphKind::Vertex {
            ref positions,
            ref normals,
            ref tangents,
        } = morph.kind
        {
            morph_alive[i] = positions
                .iter()
                .chain(normals.iter())
                .chain(tangents.iter())
                .any(|&(vi, _)| vtx_remap.get(vi).copied().flatten().is_some());
        }
        // v0.5.1 review 03 [P2] fix: a Material morph is alive only when at least one bind
        // references a visible material. If every bind drops out after remap, the morph is
        // a dead expression and is removed (a Material morph that survives only because a
        // Group references it stays dead after the convergence pass).
        if let IrMorphKind::Material {
            color_binds,
            uv_binds,
        } = &morph.kind
        {
            let any_color = color_binds
                .iter()
                .any(|b| mat_remap.contains_key(&b.material_index));
            let any_uv = uv_binds
                .iter()
                .any(|b| mat_remap.contains_key(&b.material_index));
            morph_alive[i] = any_color || any_uv;
        }
        // Phase 3 A-2: a UV morph is alive if any single vertex remains on the visible side.
        if let IrMorphKind::Uv {
            channel: _,
            offsets,
        } = &morph.kind
        {
            morph_alive[i] = offsets
                .iter()
                .any(|&(vi, _)| vtx_remap.get(vi).copied().flatten().is_some());
        }
    }

    // Iterate group-morph liveness until it converges (handles nesting).
    for iteration in 0..ir.morphs.len().max(1) {
        let mut changed = false;
        for (i, morph) in ir.morphs.iter().enumerate() {
            if morph_alive[i] {
                continue;
            }
            if let IrMorphKind::Group(goffs) = &morph.kind {
                if goffs
                    .iter()
                    .any(|&(child, _)| morph_alive.get(child).copied().unwrap_or(false))
                {
                    morph_alive[i] = true;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
        if iteration == ir.morphs.len().saturating_sub(1) {
            log::warn!("Group morph convergence loop reached limit");
        }
    }

    // Warning log for excluded morphs.
    for (i, morph) in ir.morphs.iter().enumerate() {
        if !morph_alive[i] {
            let kind_label = match &morph.kind {
                IrMorphKind::Vertex { .. } => "Vertex",
                IrMorphKind::Group(_) => "Group",
                IrMorphKind::Material { .. } => "Material",
                IrMorphKind::Uv { .. } => "Uv",
            };
            log::warn!(
                "{} morph \"{}\" references only excluded material vertices, removing.",
                kind_label,
                morph.name
            );
        }
    }

    // -- Phase 4: build the old_morph_idx -> new_morph_idx remap --
    let mut morph_remap: HashMap<usize, usize> = HashMap::new();
    let mut new_idx = 0usize;
    for (i, &alive) in morph_alive.iter().enumerate() {
        if alive {
            morph_remap.insert(i, new_idx);
            new_idx += 1;
        }
    }

    // Build morphs.
    let mut final_morphs: Vec<IrMorph> = Vec::new();
    for (old_idx, morph) in ir.morphs.iter().enumerate() {
        if !morph_alive[old_idx] {
            continue;
        }
        let new_kind = match &morph.kind {
            IrMorphKind::Vertex {
                ref positions,
                ref normals,
                ref tangents,
            } => {
                let remap_vec = |src: &[(usize, Vec3)]| -> Vec<(usize, Vec3)> {
                    src.iter()
                        .filter_map(|&(vi, off)| {
                            vtx_remap
                                .get(vi)
                                .copied()
                                .flatten()
                                .map(|new_vi| (new_vi, off))
                        })
                        .collect()
                };
                IrMorphKind::Vertex {
                    positions: remap_vec(positions),
                    normals: remap_vec(normals),
                    tangents: remap_vec(tangents),
                }
            }
            IrMorphKind::Group(goffs) => {
                let remapped: Vec<(usize, f32)> = goffs
                    .iter()
                    .filter_map(|&(child_idx, weight)| {
                        morph_remap
                            .get(&child_idx)
                            .map(|&new_child| (new_child, weight))
                    })
                    .collect();
                IrMorphKind::Group(remapped)
            }
            IrMorphKind::Material {
                color_binds,
                uv_binds,
            } => {
                // v0.5.1 review 02 [P2] fix: remap material_index via mat_remap.
                // The previous implementation only cloned the binds, leaving stale
                // old material_index values that did not match the new materials array
                // when only visible materials were exported. filter_map drops binds
                // that point to excluded materials.
                let new_color_binds: Vec<IrMaterialColorBind> = color_binds
                    .iter()
                    .filter_map(|b| {
                        mat_remap
                            .get(&b.material_index)
                            .map(|&new_mi| IrMaterialColorBind {
                                material_index: new_mi,
                                bind_type: b.bind_type,
                                target_value: b.target_value,
                            })
                    })
                    .collect();
                let new_uv_binds: Vec<IrTextureTransformBind> = uv_binds
                    .iter()
                    .filter_map(|b| {
                        mat_remap
                            .get(&b.material_index)
                            .map(|&new_mi| IrTextureTransformBind {
                                material_index: new_mi,
                                scale: b.scale,
                                offset: b.offset,
                            })
                    })
                    .collect();
                IrMorphKind::Material {
                    color_binds: new_color_binds,
                    uv_binds: new_uv_binds,
                }
            }
            // Phase 3 A-2: a UV morph remaps vertex indices through vtx_remap, just like a vertex morph.
            // Offsets that point to excluded vertices are dropped via filter_map.
            IrMorphKind::Uv { channel, offsets } => {
                let new_offsets: Vec<(usize, [f32; 4])> = offsets
                    .iter()
                    .filter_map(|&(vi, off)| {
                        vtx_remap
                            .get(vi)
                            .copied()
                            .flatten()
                            .map(|new_vi| (new_vi, off))
                    })
                    .collect();
                IrMorphKind::Uv {
                    channel: *channel,
                    offsets: new_offsets,
                }
            }
        };
        final_morphs.push(IrMorph {
            name: morph.name.clone(),
            name_en: morph.name_en.clone(),
            panel: morph.panel,
            kind: new_kind,
        });
    }

    let removed = morph_count - final_morphs.len();
    if removed > 0 {
        log::warn!("{} of {} morphs excluded.", morph_count, removed);
    }

    // -- Phase 5: texture pruning --
    // Keep only textures referenced by the post-filter materials.
    let used_tex_indices: HashSet<usize> = new_materials
        .iter()
        .filter_map(|m| m.texture_index)
        .chain(
            new_materials
                .iter()
                .filter_map(|m| m.base_color_tex_info.as_ref().map(|t| t.index)),
        )
        .chain(
            new_materials
                .iter()
                .filter_map(|m| m.mtoon().shade_texture.as_ref().map(|t| t.index)),
        )
        .chain(
            new_materials
                .iter()
                .filter_map(|m| m.mtoon().outline_width_texture.as_ref().map(|t| t.index)),
        )
        .chain(
            new_materials
                .iter()
                .filter_map(|m| m.mtoon().matcap_texture.as_ref().map(|t| t.index)),
        )
        .chain(
            new_materials
                .iter()
                .filter_map(|m| m.mtoon().shading_shift_texture.as_ref().map(|t| t.index)),
        )
        .chain(
            new_materials
                .iter()
                .filter_map(|m| m.mtoon().rim_multiply_texture.as_ref().map(|t| t.index)),
        )
        .chain(new_materials.iter().filter_map(|m| {
            m.mtoon()
                .uv_animation_mask_texture
                .as_ref()
                .map(|t| t.index)
        }))
        .chain(
            new_materials
                .iter()
                .filter_map(|m| m.emissive_texture.as_ref().map(|t| t.index)),
        )
        .chain(
            new_materials
                .iter()
                .filter_map(|m| m.normal_texture.as_ref().map(|t| t.index)),
        )
        .chain(new_materials.iter().filter_map(|m| m.sphere_texture_index))
        .chain(new_materials.iter().filter_map(|m| m.toon_texture_index))
        .collect();

    let mut tex_remap: HashMap<usize, usize> = HashMap::new();
    let mut new_textures: Vec<IrTexture> = Vec::new();
    for (old_idx, tex) in ir.textures.iter().enumerate() {
        if used_tex_indices.contains(&old_idx) {
            tex_remap.insert(old_idx, new_textures.len());
            new_textures.push(tex.clone());
        }
    }

    // Remap texture_index on the materials.
    for mat in &mut new_materials {
        mat.texture_index = mat.texture_index.and_then(|i| tex_remap.get(&i).copied());
        mat.base_color_tex_info = mat
            .base_color_tex_info
            .take()
            .and_then(|t| t.remap_index(&tex_remap));
        if let Some(ref mut mp) = mat.mtoon {
            mp.shade_texture = mp
                .shade_texture
                .take()
                .and_then(|t| t.remap_index(&tex_remap));
            mp.outline_width_texture = mp
                .outline_width_texture
                .take()
                .and_then(|t| t.remap_index(&tex_remap));
            mp.matcap_texture = mp
                .matcap_texture
                .take()
                .and_then(|t| t.remap_index(&tex_remap));
            mp.shading_shift_texture = mp
                .shading_shift_texture
                .take()
                .and_then(|t| t.remap_index(&tex_remap));
            mp.rim_multiply_texture = mp
                .rim_multiply_texture
                .take()
                .and_then(|t| t.remap_index(&tex_remap));
            mp.uv_animation_mask_texture = mp
                .uv_animation_mask_texture
                .take()
                .and_then(|t| t.remap_index(&tex_remap));
        }
        mat.emissive_texture = mat
            .emissive_texture
            .take()
            .and_then(|t| t.remap_index(&tex_remap));
        mat.normal_texture = mat
            .normal_texture
            .take()
            .and_then(|t| t.remap_index(&tex_remap));
        mat.sphere_texture_index = mat
            .sphere_texture_index
            .and_then(|i| tex_remap.get(&i).copied());
        mat.toon_texture_index = mat
            .toon_texture_index
            .and_then(|i| tex_remap.get(&i).copied());
    }

    // -- Phase 6: build the final IrModel --
    IrModel {
        name: ir.name.clone(),
        comment: ir.comment.clone(),
        bones: ir.bones.clone(),
        meshes: new_meshes,
        materials: new_materials,
        textures: new_textures,
        morphs: final_morphs,
        physics: IrPhysics {
            rigid_bodies: ir.physics.rigid_bodies.clone(),
            joints: ir.physics.joints.clone(),
        },
        node_to_bone: ir.node_to_bone.clone(),
        source_format: ir.source_format,
        rig_type: ir.rig_type.clone(),
        humanoid_bone_count: ir.humanoid_bone_count,
        astance_result: ir.astance_result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intermediate::types::{IrBone, TextureData};
    use glam::{Mat4, Vec2, Vec4};
    use std::sync::Arc;

    fn make_bone(name: &str) -> IrBone {
        IrBone {
            name: name.to_string(),
            name_en: String::new(),
            original_name: name.to_string(),
            vrm_bone_name: None,
            position: Vec3::ZERO,
            global_mat: Mat4::IDENTITY,
            parent: None,
            children: Vec::new(),
            node_index: 0,
            is_physics: false,
            tail_position: None,
            tail_bone_index: None,
            is_ik: false,
            is_ik_bone: false,
            is_translatable: false,
            is_axis_fixed: false,
            is_visible: true,
            grant: None,
        }
    }

    fn make_mesh(name: &str, mat_idx: usize, num_verts: usize) -> IrMesh {
        let vertices: Vec<IrVertex> = (0..num_verts)
            .map(|_| IrVertex {
                position: Vec3::ZERO,
                normal: Vec3::Y,
                uv: Vec2::ZERO,
                tangent: Vec4::new(1.0, 0.0, 0.0, 1.0),
                weights: [(0, 1.0), (0, 0.0), (0, 0.0), (0, 0.0)],
                weight_count: 1,
                edge_scale: 1.0,
            })
            .collect();
        let indices: Vec<u32> = if num_verts >= 3 {
            (0..num_verts as u32).collect()
        } else {
            Vec::new()
        };
        IrMesh {
            name: name.to_string(),
            vertices: vertices.into(),
            indices: indices.into(),
            material_index: mat_idx,
            morph_targets: Arc::new(Vec::new()),
            node_index: 0,
            uvs1: Vec::new(),
        }
    }

    /// Verify that a model with hidden materials does not panic.
    #[test]
    fn test_filter_with_hidden_material() {
        let ir = IrModel {
            name: "test".into(),
            bones: vec![make_bone("Root")],
            meshes: vec![
                make_mesh("mesh0", 0, 6), // material 0: vertices 0..6
                make_mesh("mesh1", 1, 3), // material 1: vertices 6..9
                make_mesh("mesh2", 2, 4), // material 2: vertices 9..13
            ],
            materials: vec![
                IrMaterial {
                    name: "mat0".into(),
                    ..Default::default()
                },
                IrMaterial {
                    name: "mat1".into(),
                    ..Default::default()
                },
                IrMaterial {
                    name: "mat2".into(),
                    ..Default::default()
                },
            ],
            morphs: vec![IrMorph {
                name: "blink".into(),
                name_en: String::new(),
                panel: 2,
                kind: IrMorphKind::Vertex {
                    positions: vec![
                        (1, Vec3::Y),  // mesh0 vertex 1
                        (7, Vec3::X),  // mesh1 vertex 1 (excluded)
                        (10, Vec3::Z), // mesh2 vertex 1
                    ],
                    normals: Vec::new(),
                    tangents: Vec::new(),
                },
            }],
            ..Default::default()
        };

        // Hide material 1.
        let visible: HashSet<usize> = [0, 2].iter().copied().collect();
        let filtered = build_filtered_ir(&ir, &visible);

        // Materials: 2
        assert_eq!(filtered.materials.len(), 2);
        assert_eq!(filtered.materials[0].name, "mat0");
        assert_eq!(filtered.materials[1].name, "mat2");

        // Meshes: 2 (mesh1 dropped)
        assert_eq!(filtered.meshes.len(), 2);
        assert_eq!(filtered.meshes[0].material_index, 0); // mat0 -> new 0
        assert_eq!(filtered.meshes[1].material_index, 1); // mat2 -> new 1

        // Morph: mesh1 vertex (7) is dropped, leaving 2 entries.
        assert_eq!(filtered.morphs.len(), 1);
        if let IrMorphKind::Vertex { ref positions, .. } = filtered.morphs[0].kind {
            let entries = positions;
            assert_eq!(entries.len(), 2);
            // mesh0 vertex 1 -> new index 1 (unchanged)
            assert_eq!(entries[0].0, 1);
            // mesh2 vertex 1 -> old 10, minus 3 (mesh1 had 3 verts) -> new 7
            assert_eq!(entries[1].0, 7);
        } else {
            panic!("should be a vertex morph");
        }
    }

    /// Verify that morphs that only reference vertices in excluded meshes are removed.
    #[test]
    fn test_morph_fully_removed() {
        let ir = IrModel {
            name: "test".into(),
            bones: vec![make_bone("Root")],
            meshes: vec![make_mesh("mesh0", 0, 3), make_mesh("mesh1", 1, 3)],
            materials: vec![
                IrMaterial {
                    name: "mat0".into(),
                    ..Default::default()
                },
                IrMaterial {
                    name: "mat1".into(),
                    ..Default::default()
                },
            ],
            morphs: vec![
                IrMorph {
                    name: "smile".into(),
                    name_en: String::new(),
                    panel: 3,
                    kind: IrMorphKind::Vertex {
                        positions: vec![
                            (3, Vec3::Y), // mesh1 vertex 0 only
                            (4, Vec3::X), // mesh1 vertex 1 only
                        ],
                        normals: Vec::new(),
                        tangents: Vec::new(),
                    },
                },
                IrMorph {
                    name: "blink".into(),
                    name_en: String::new(),
                    panel: 2,
                    kind: IrMorphKind::Vertex {
                        positions: vec![
                            (0, Vec3::Y), // mesh0 vertex 0 (kept)
                        ],
                        normals: Vec::new(),
                        tangents: Vec::new(),
                    },
                },
            ],
            ..Default::default()
        };

        // Hide material 1.
        let visible: HashSet<usize> = [0].iter().copied().collect();
        let filtered = build_filtered_ir(&ir, &visible);

        // smile only references mesh1 vertices -> removed.
        // blink references a mesh0 vertex -> kept.
        assert_eq!(filtered.morphs.len(), 1);
        assert_eq!(filtered.morphs[0].name, "blink");
    }

    /// Verify that a group morph's child references are remapped correctly.
    #[test]
    fn test_group_morph_remap() {
        let ir = IrModel {
            name: "test".into(),
            bones: vec![make_bone("Root")],
            meshes: vec![make_mesh("mesh0", 0, 3), make_mesh("mesh1", 1, 3)],
            materials: vec![
                IrMaterial {
                    name: "mat0".into(),
                    ..Default::default()
                },
                IrMaterial {
                    name: "mat1".into(),
                    ..Default::default()
                },
            ],
            morphs: vec![
                // [0] smile: mesh1 only -> excluded.
                IrMorph {
                    name: "smile".into(),
                    name_en: String::new(),
                    panel: 3,
                    kind: IrMorphKind::Vertex {
                        positions: vec![(3, Vec3::Y)],
                        normals: Vec::new(),
                        tangents: Vec::new(),
                    },
                },
                // [1] blink: mesh0 -> kept (new index 0).
                IrMorph {
                    name: "blink".into(),
                    name_en: String::new(),
                    panel: 2,
                    kind: IrMorphKind::Vertex {
                        positions: vec![(0, Vec3::Y)],
                        normals: Vec::new(),
                        tangents: Vec::new(),
                    },
                },
                // [2] group: smile(0) + blink(1) -> only blink(new 0) survives.
                IrMorph {
                    name: "group".into(),
                    name_en: String::new(),
                    panel: 4,
                    kind: IrMorphKind::Group(vec![(0, 0.5), (1, 1.0)]),
                },
            ],
            ..Default::default()
        };

        let visible: HashSet<usize> = [0].iter().copied().collect();
        let filtered = build_filtered_ir(&ir, &visible);

        // smile excluded -> blink(new 0), group(new 1).
        assert_eq!(filtered.morphs.len(), 2);
        assert_eq!(filtered.morphs[0].name, "blink");
        assert_eq!(filtered.morphs[1].name, "group");

        if let IrMorphKind::Group(ref entries) = filtered.morphs[1].kind {
            // smile is excluded so the group only has blink as a child.
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].0, 0); // blink new index = 0
            assert_eq!(entries[0].1, 1.0);
        } else {
            panic!("should be a group morph");
        }
    }

    /// Verify that with nested group morphs (outer -> inner -> vertex),
    /// dropping the leaf vertex correctly removes both inner and outer.
    #[test]
    fn test_nested_group_morph_cascade_removal() {
        let ir = IrModel {
            name: "test".into(),
            bones: vec![make_bone("Root")],
            meshes: vec![make_mesh("mesh0", 0, 3), make_mesh("mesh1", 1, 3)],
            materials: vec![
                IrMaterial {
                    name: "mat0".into(),
                    ..Default::default()
                },
                IrMaterial {
                    name: "mat1".into(),
                    ..Default::default()
                },
            ],
            morphs: vec![
                // [0] vtx_alive: mesh0 -> kept.
                IrMorph {
                    name: "vtx_alive".into(),
                    name_en: String::new(),
                    panel: 2,
                    kind: IrMorphKind::Vertex {
                        positions: vec![(0, Vec3::Y)],
                        normals: Vec::new(),
                        tangents: Vec::new(),
                    },
                },
                // [1] vtx_dead: mesh1 only -> excluded.
                IrMorph {
                    name: "vtx_dead".into(),
                    name_en: String::new(),
                    panel: 2,
                    kind: IrMorphKind::Vertex {
                        positions: vec![(3, Vec3::Y)],
                        normals: Vec::new(),
                        tangents: Vec::new(),
                    },
                },
                // [2] inner: only references vtx_dead(1) -> excluded once children die.
                IrMorph {
                    name: "inner".into(),
                    name_en: String::new(),
                    panel: 4,
                    kind: IrMorphKind::Group(vec![(1, 1.0)]),
                },
                // [3] outer: inner(2) + vtx_alive(0) -> only vtx_alive survives after inner is excluded.
                IrMorph {
                    name: "outer".into(),
                    name_en: String::new(),
                    panel: 4,
                    kind: IrMorphKind::Group(vec![(2, 0.5), (0, 1.0)]),
                },
            ],
            ..Default::default()
        };

        let visible: HashSet<usize> = [0].iter().copied().collect();
        let filtered = build_filtered_ir(&ir, &visible);

        // vtx_dead, inner are excluded -> vtx_alive(new 0), outer(new 1).
        assert_eq!(filtered.morphs.len(), 2);
        assert_eq!(filtered.morphs[0].name, "vtx_alive");
        assert_eq!(filtered.morphs[1].name, "outer");

        if let IrMorphKind::Group(ref entries) = filtered.morphs[1].kind {
            // inner excluded; only vtx_alive survives.
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].0, 0); // vtx_alive new index = 0
            assert_eq!(entries[0].1, 1.0);
        } else {
            panic!("should be a group morph");
        }
    }

    /// Verify that hiding every material returns an empty model.
    #[test]
    fn test_all_materials_hidden() {
        let ir = IrModel {
            name: "test".into(),
            bones: vec![make_bone("Root")],
            meshes: vec![make_mesh("mesh0", 0, 3)],
            materials: vec![IrMaterial {
                name: "mat0".into(),
                ..Default::default()
            }],
            morphs: vec![IrMorph {
                name: "blink".into(),
                name_en: String::new(),
                panel: 2,
                kind: IrMorphKind::Vertex {
                    positions: vec![(0, Vec3::Y)],
                    normals: Vec::new(),
                    tangents: Vec::new(),
                },
            }],
            ..Default::default()
        };

        let visible: HashSet<usize> = HashSet::new();
        let filtered = build_filtered_ir(&ir, &visible);

        assert_eq!(filtered.materials.len(), 0);
        assert_eq!(filtered.meshes.len(), 0);
        assert_eq!(filtered.morphs.len(), 0);
        assert_eq!(filtered.textures.len(), 0);
        // Bones are preserved.
        assert_eq!(filtered.bones.len(), 1);
    }

    /// Texture pruning: verify that textures of hidden materials are dropped.
    #[test]
    fn test_texture_pruning() {
        use crate::intermediate::types::IrTexture;

        let ir = IrModel {
            name: "test".into(),
            bones: vec![make_bone("Root")],
            meshes: vec![make_mesh("mesh0", 0, 3), make_mesh("mesh1", 1, 3)],
            materials: vec![
                IrMaterial {
                    name: "mat0".into(),
                    texture_index: Some(0),
                    ..Default::default()
                },
                IrMaterial {
                    name: "mat1".into(),
                    texture_index: Some(1),
                    ..Default::default()
                },
            ],
            textures: vec![
                IrTexture {
                    filename: "tex0.png".into(),
                    data: TextureData::Encoded(Arc::from(vec![0u8])),
                    mime_type: "image/png".into(),
                    source_path: String::new(),
                    mip_chain: None,
                },
                IrTexture {
                    filename: "tex1.png".into(),
                    data: TextureData::Encoded(Arc::from(vec![1u8])),
                    mime_type: "image/png".into(),
                    source_path: String::new(),
                    mip_chain: None,
                },
            ],
            ..Default::default()
        };

        // Hide material 1.
        let visible: HashSet<usize> = [0].iter().copied().collect();
        let filtered = build_filtered_ir(&ir, &visible);

        // Textures: only tex0 survives.
        assert_eq!(filtered.textures.len(), 1);
        assert_eq!(filtered.textures[0].filename, "tex0.png");

        // The material's texture_index has been remapped.
        assert_eq!(filtered.materials[0].texture_index, Some(0));
    }

    /// Verify that base_color_tex_info is remapped after filtering.
    #[test]
    fn test_base_color_tex_info_remap() {
        use crate::intermediate::types::{IrSamplerInfo, IrTexture, IrTextureInfo};

        let ir = IrModel {
            name: "test".into(),
            bones: vec![make_bone("Root")],
            meshes: vec![make_mesh("mesh0", 0, 3), make_mesh("mesh1", 1, 3)],
            materials: vec![
                IrMaterial {
                    name: "mat0".into(),
                    texture_index: Some(0),
                    base_color_tex_info: Some(IrTextureInfo {
                        index: 0,
                        tex_coord: 1,
                        offset: Vec2::new(0.1, 0.2),
                        scale: Vec2::new(2.0, 2.0),
                        rotation: 0.5,
                        sampler: IrSamplerInfo::default(),
                    }),
                    ..Default::default()
                },
                IrMaterial {
                    name: "mat1".into(),
                    texture_index: Some(1),
                    base_color_tex_info: Some(IrTextureInfo::from_index(1)),
                    ..Default::default()
                },
            ],
            textures: vec![
                IrTexture {
                    filename: "tex0.png".into(),
                    data: TextureData::Encoded(Arc::from(vec![0u8])),
                    mime_type: "image/png".into(),
                    source_path: String::new(),
                    mip_chain: None,
                },
                IrTexture {
                    filename: "tex1.png".into(),
                    data: TextureData::Encoded(Arc::from(vec![1u8])),
                    mime_type: "image/png".into(),
                    source_path: String::new(),
                    mip_chain: None,
                },
            ],
            ..Default::default()
        };

        // Show material 0 only (material 1's texture tex1 is dropped).
        let visible: HashSet<usize> = [0].iter().copied().collect();
        let filtered = build_filtered_ir(&ir, &visible);

        assert_eq!(filtered.textures.len(), 1);

        // texture_index and base_color_tex_info.index are remapped consistently.
        let mat = &filtered.materials[0];
        assert_eq!(mat.texture_index, Some(0));
        let ti = mat.base_color_tex_info.as_ref().unwrap();
        assert_eq!(ti.index, 0);
        // UV transform info is preserved.
        assert_eq!(ti.tex_coord, 1);
        assert!((ti.offset.x - 0.1).abs() < 1e-6);
        assert!((ti.scale.x - 2.0).abs() < 1e-6);
        assert!((ti.rotation - 0.5).abs() < 1e-6);
    }
}
