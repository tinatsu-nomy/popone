use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::Vec3;

use crate::intermediate::types::{
    IrMaterial, IrMaterialColorBind, IrMesh, IrModel, IrMorph, IrMorphKind, IrPhysics, IrTexture,
    IrTextureTransformBind, IrVertex,
};

/// 可視材質のみを含む IrModel を新規構築する。
///
/// `visible_mat_indices` に含まれる material_index のメッシュ・材質のみを残し、
/// 頂点モーフ・グループモーフの index を正しくリマップする。
pub fn build_filtered_ir(ir: &IrModel, visible_mat_indices: &HashSet<usize>) -> IrModel {
    // 全材質が非表示の場合は空 PMX を出力（ワーニング付き）
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

    // ── Phase 1: 材質リマップ（old_mat_idx → new_mat_idx）──
    let mut mat_remap: HashMap<usize, usize> = HashMap::new();
    let mut new_materials: Vec<IrMaterial> = Vec::new();
    for (old_idx, mat) in ir.materials.iter().enumerate() {
        if visible_mat_indices.contains(&old_idx) {
            mat_remap.insert(old_idx, new_materials.len());
            new_materials.push(mat.clone());
        }
    }

    // ── Phase 2: メッシュフィルタ + 頂点リマップテーブル構築 ──
    // 元メッシュのグローバル頂点オフセットを記録
    let mut old_mesh_vtx_start: Vec<usize> = Vec::with_capacity(ir.meshes.len());
    let mut offset = 0usize;
    for mesh in &ir.meshes {
        old_mesh_vtx_start.push(offset);
        offset += mesh.vertices.len();
    }
    let old_total_verts = offset;

    // old_global_vtx → new_global_vtx のリマップ（None = 除外）
    let mut vtx_remap: Vec<Option<usize>> = vec![None; old_total_verts];
    let mut new_meshes: Vec<IrMesh> = Vec::new();
    let mut new_vtx_offset = 0usize;

    for (mesh_i, mesh) in ir.meshes.iter().enumerate() {
        if !visible_mat_indices.contains(&mesh.material_index) {
            continue;
        }
        let new_mat_idx = mat_remap[&mesh.material_index];
        let old_start = old_mesh_vtx_start[mesh_i];

        // 頂点リマップ登録
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

    // ── Phase 3: モーフの有効性判定 ──
    // 頂点モーフ: リマップ後に1エントリ以上残れば有効
    // グループモーフ: 子モーフが1つ以上有効なら有効（再帰的に判定）
    let morph_count = ir.morphs.len();
    let mut morph_alive: Vec<bool> = vec![false; morph_count];

    // まず頂点モーフの有効性を判定（checked access で範囲外は無視）
    // positions / normals / tangents いずれかに有効な頂点があれば生存
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
        // v0.5.1 レビュー 03 [P2] 対応: Material morph は「少なくとも 1 つの bind が
        // 可視材質を参照しているとき」のみ生存。remap 後に bind が全て落ちる場合は
        // 機能しない「死んだ表情」として除外する（Group から参照されても収束判定で
        // 孤立した Material morph はそのまま死ぬ）。
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
        // Phase 3 A-2: UV モーフは 1 頂点でも可視側に残っていれば生存
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

    // グループモーフの有効性を収束するまで反復判定（ネスト対応）
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

    // 除外されるモーフのワーニングログ
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

    // ── Phase 4: old_morph_idx → new_morph_idx リマップ構築 ──
    let mut morph_remap: HashMap<usize, usize> = HashMap::new();
    let mut new_idx = 0usize;
    for (i, &alive) in morph_alive.iter().enumerate() {
        if alive {
            morph_remap.insert(i, new_idx);
            new_idx += 1;
        }
    }

    // モーフ構築
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
                // v0.5.1 レビュー 02 [P2] 対応: material_index を mat_remap で再マップ。
                // 旧実装は clone だけで old material_index が残留し、可視材質のみ export した
                // IR で Material morph が新しい materials 配列と整合しない状態になっていた。
                // 除外された材質を指す bind は filter_map で落とす。
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
            // Phase 3 A-2: UV モーフは 頂点モーフと同様に vtx_remap で頂点Indexを再マップ。
            // 除外された頂点を指すオフセットは filter_map で落とす。
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

    // ── Phase 5: テクスチャ pruning ──
    // フィルタ後の材質が参照するテクスチャのみ残す
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

    // 材質の texture_index をリマップ
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

    // ── Phase 6: IrModel 構築 ──
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

    /// 未使用材質を含むモデルで panic しないことを確認
    #[test]
    fn test_filter_with_hidden_material() {
        let ir = IrModel {
            name: "test".into(),
            bones: vec![make_bone("Root")],
            meshes: vec![
                make_mesh("mesh0", 0, 6), // 材質0: 頂点 0..6
                make_mesh("mesh1", 1, 3), // 材質1: 頂点 6..9
                make_mesh("mesh2", 2, 4), // 材質2: 頂点 9..13
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
                        (1, Vec3::Y),  // mesh0 の頂点1
                        (7, Vec3::X),  // mesh1 の頂点1（除外対象）
                        (10, Vec3::Z), // mesh2 の頂点1
                    ],
                    normals: Vec::new(),
                    tangents: Vec::new(),
                },
            }],
            ..Default::default()
        };

        // 材質1を非表示にする
        let visible: HashSet<usize> = [0, 2].iter().copied().collect();
        let filtered = build_filtered_ir(&ir, &visible);

        // 材質数: 2
        assert_eq!(filtered.materials.len(), 2);
        assert_eq!(filtered.materials[0].name, "mat0");
        assert_eq!(filtered.materials[1].name, "mat2");

        // メッシュ数: 2（mesh1 が除外）
        assert_eq!(filtered.meshes.len(), 2);
        assert_eq!(filtered.meshes[0].material_index, 0); // mat0 → new 0
        assert_eq!(filtered.meshes[1].material_index, 1); // mat2 → new 1

        // モーフ: mesh1 の頂点(7)が除外され、残り2エントリ
        assert_eq!(filtered.morphs.len(), 1);
        if let IrMorphKind::Vertex { ref positions, .. } = filtered.morphs[0].kind {
            let entries = positions;
            assert_eq!(entries.len(), 2);
            // mesh0 の頂点1 → new index 1（そのまま）
            assert_eq!(entries[0].0, 1);
            // mesh2 の頂点1 → old 10, mesh1(3頂点)除外で -3 → new 7
            assert_eq!(entries[1].0, 7);
        } else {
            panic!("should be a vertex morph");
        }
    }

    /// 除外メッシュの頂点のみを参照するモーフが削除されることを確認
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
                            (3, Vec3::Y), // mesh1 の頂点0 のみ
                            (4, Vec3::X), // mesh1 の頂点1 のみ
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
                            (0, Vec3::Y), // mesh0 の頂点0（残る）
                        ],
                        normals: Vec::new(),
                        tangents: Vec::new(),
                    },
                },
            ],
            ..Default::default()
        };

        // 材質1を非表示
        let visible: HashSet<usize> = [0].iter().copied().collect();
        let filtered = build_filtered_ir(&ir, &visible);

        // smile は mesh1 の頂点のみ → 削除
        // blink は mesh0 の頂点 → 残る
        assert_eq!(filtered.morphs.len(), 1);
        assert_eq!(filtered.morphs[0].name, "blink");
    }

    /// グループモーフの子参照が正しくリマップされることを確認
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
                // [0] smile: mesh1 のみ → 除外される
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
                // [1] blink: mesh0 → 残る（new index 0）
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
                // [2] group: smile(0) + blink(1) → smile除外後 blink(new 0) のみ
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

        // smile 除外 → blink(new 0), group(new 1)
        assert_eq!(filtered.morphs.len(), 2);
        assert_eq!(filtered.morphs[0].name, "blink");
        assert_eq!(filtered.morphs[1].name, "group");

        if let IrMorphKind::Group(ref entries) = filtered.morphs[1].kind {
            // smile は除外されているので group の子は blink のみ
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].0, 0); // blink の new index = 0
            assert_eq!(entries[0].1, 1.0);
        } else {
            panic!("should be a group morph");
        }
    }

    /// ネストしたグループモーフ（outer -> inner -> vertex）で
    /// vertex が除外されると inner, outer ともに正しく除去されることを確認
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
                // [0] vtx_alive: mesh0 → 残る
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
                // [1] vtx_dead: mesh1 のみ → 除外
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
                // [2] inner: vtx_dead(1) のみ → 子が全滅で除外
                IrMorph {
                    name: "inner".into(),
                    name_en: String::new(),
                    panel: 4,
                    kind: IrMorphKind::Group(vec![(1, 1.0)]),
                },
                // [3] outer: inner(2) + vtx_alive(0) → inner除外後 vtx_alive のみ残る
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

        // vtx_dead, inner は除外 → vtx_alive(new 0), outer(new 1)
        assert_eq!(filtered.morphs.len(), 2);
        assert_eq!(filtered.morphs[0].name, "vtx_alive");
        assert_eq!(filtered.morphs[1].name, "outer");

        if let IrMorphKind::Group(ref entries) = filtered.morphs[1].kind {
            // inner は除外、vtx_alive のみ残る
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].0, 0); // vtx_alive の new index = 0
            assert_eq!(entries[0].1, 1.0);
        } else {
            panic!("should be a group morph");
        }
    }

    /// 全材質非表示の場合、空モデルが返ることを確認
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
        // ボーンは保持
        assert_eq!(filtered.bones.len(), 1);
    }

    /// テクスチャ pruning: 非表示材質のテクスチャが除外されることを確認
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

        // 材質1を非表示
        let visible: HashSet<usize> = [0].iter().copied().collect();
        let filtered = build_filtered_ir(&ir, &visible);

        // テクスチャ: tex0 のみ残る
        assert_eq!(filtered.textures.len(), 1);
        assert_eq!(filtered.textures[0].filename, "tex0.png");

        // 材質の texture_index がリマップされている
        assert_eq!(filtered.materials[0].texture_index, Some(0));
    }

    /// base_color_tex_info がフィルタ後にリマップされることを確認
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

        // 材質0のみ表示（材質1のテクスチャ tex1 が除外される）
        let visible: HashSet<usize> = [0].iter().copied().collect();
        let filtered = build_filtered_ir(&ir, &visible);

        assert_eq!(filtered.textures.len(), 1);

        // texture_index と base_color_tex_info.index が一致してリマップされている
        let mat = &filtered.materials[0];
        assert_eq!(mat.texture_index, Some(0));
        let ti = mat.base_color_tex_info.as_ref().unwrap();
        assert_eq!(ti.index, 0);
        // UV transform 情報が維持されている
        assert_eq!(ti.tex_coord, 1);
        assert!((ti.offset.x - 0.1).abs() < 1e-6);
        assert!((ti.scale.x - 2.0).abs() < 1e-6);
        assert!((ti.rotation - 0.5).abs() < 1e-6);
    }
}
