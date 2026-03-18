use std::collections::{HashMap, HashSet};

use glam::Vec3;

use crate::intermediate::types::{
    IrMaterial, IrMesh, IrModel, IrMorph, IrMorphKind, IrPhysics, IrTexture, IrVertex,
};

/// 可視材質のみを含む IrModel を新規構築する。
///
/// `visible_mat_indices` に含まれる material_index のメッシュ・材質のみを残し、
/// 頂点モーフ・グループモーフの index を正しくリマップする。
pub fn build_filtered_ir(
    ir: &IrModel,
    visible_mat_indices: &HashSet<usize>,
) -> IrModel {
    // 全材質が非表示の場合は空 PMX を出力（ワーニング付き）
    if visible_mat_indices.is_empty() {
        log::warn!("全材質が非表示です。空の PMX を出力します。");
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
            vertices: mesh.vertices.iter().map(|v| IrVertex {
                position: v.position,
                normal: v.normal,
                uv: v.uv,
                weights: v.weights.clone(),
                edge_scale: v.edge_scale,
            }).collect(),
            indices: mesh.indices.clone(),
            material_index: new_mat_idx,
            morph_targets: mesh.morph_targets.clone(),
            node_index: mesh.node_index,
        });

        new_vtx_offset += mesh.vertices.len();
    }

    // ── Phase 3: モーフの有効性判定 ──
    // 頂点モーフ: リマップ後に1エントリ以上残れば有効
    // グループモーフ: 子モーフが1つ以上有効なら有効（再帰的に判定）
    let morph_count = ir.morphs.len();
    let mut morph_alive: Vec<bool> = vec![false; morph_count];

    // まず頂点モーフの有効性を判定（checked access で範囲外は無視）
    for (i, morph) in ir.morphs.iter().enumerate() {
        if let IrMorphKind::Vertex(voffs) = &morph.kind {
            morph_alive[i] = voffs.iter().any(|&(vi, _)| {
                vtx_remap.get(vi).copied().flatten().is_some()
            });
        }
    }

    // グループモーフの有効性を収束するまで反復判定（ネスト対応）
    loop {
        let mut changed = false;
        for (i, morph) in ir.morphs.iter().enumerate() {
            if morph_alive[i] { continue; }
            if let IrMorphKind::Group(goffs) = &morph.kind {
                if goffs.iter().any(|&(child, _)| {
                    morph_alive.get(child).copied().unwrap_or(false)
                }) {
                    morph_alive[i] = true;
                    changed = true;
                }
            }
        }
        if !changed { break; }
    }

    // 除外されるモーフのワーニングログ
    for (i, morph) in ir.morphs.iter().enumerate() {
        if !morph_alive[i] {
            let kind_label = match &morph.kind {
                IrMorphKind::Vertex(_) => "頂点",
                IrMorphKind::Group(_) => "グループ",
            };
            log::warn!(
                "{}モーフ \"{}\" は除外材質の頂点のみを参照しているため削除されます。",
                kind_label, morph.name
            );
        }
    }

    // ── Phase 4: old_morph_idx → new_morph_idx リマップ構築 ──
    let mut morph_remap: HashMap<usize, usize> = HashMap::new();
    let mut new_idx = 0usize;
    for i in 0..morph_count {
        if morph_alive[i] {
            morph_remap.insert(i, new_idx);
            new_idx += 1;
        }
    }

    // モーフ構築
    let mut final_morphs: Vec<IrMorph> = Vec::new();
    for (old_idx, morph) in ir.morphs.iter().enumerate() {
        if !morph_alive[old_idx] { continue; }
        let new_kind = match &morph.kind {
            IrMorphKind::Vertex(voffs) => {
                let remapped: Vec<(usize, Vec3)> = voffs.iter()
                    .filter_map(|&(vi, off)| {
                        vtx_remap.get(vi).copied().flatten().map(|new_vi| (new_vi, off))
                    })
                    .collect();
                IrMorphKind::Vertex(remapped)
            }
            IrMorphKind::Group(goffs) => {
                let remapped: Vec<(usize, f32)> = goffs.iter()
                    .filter_map(|&(child_idx, weight)| {
                        morph_remap.get(&child_idx).map(|&new_child| (new_child, weight))
                    })
                    .collect();
                IrMorphKind::Group(remapped)
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
        log::warn!("モーフ {} 個中 {} 個が除外されました。", morph_count, removed);
    }

    // ── Phase 5: テクスチャ pruning ──
    // フィルタ後の材質が参照するテクスチャのみ残す
    let used_tex_indices: HashSet<usize> = new_materials.iter()
        .filter_map(|m| m.texture_index)
        .chain(new_materials.iter().filter_map(|m| m.shade_texture_index))
        .chain(new_materials.iter().filter_map(|m| m.outline_width_texture_index))
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
        mat.shade_texture_index = mat.shade_texture_index.and_then(|i| tex_remap.get(&i).copied());
        mat.outline_width_texture_index = mat.outline_width_texture_index.and_then(|i| tex_remap.get(&i).copied());
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
    use crate::intermediate::types::IrBone;
    use glam::{Mat4, Vec2};

    fn make_bone(name: &str) -> IrBone {
        IrBone {
            name: name.to_string(),
            name_en: String::new(),
            vrm_bone_name: None,
            position: Vec3::ZERO,
            global_mat: Mat4::IDENTITY,
            parent: None,
            children: Vec::new(),
            node_index: 0,
            is_physics: false,
        }
    }

    fn make_mesh(name: &str, mat_idx: usize, num_verts: usize) -> IrMesh {
        let vertices: Vec<IrVertex> = (0..num_verts)
            .map(|_| IrVertex {
                position: Vec3::ZERO,
                normal: Vec3::Y,
                uv: Vec2::ZERO,
                weights: vec![(0, 1.0)],
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
            vertices,
            indices,
            material_index: mat_idx,
            morph_targets: Vec::new(),
            node_index: 0,
        }
    }

    /// 未使用材質を含むモデルで panic しないことを確認
    #[test]
    fn test_filter_with_hidden_material() {
        let ir = IrModel {
            name: "test".into(),
            bones: vec![make_bone("Root")],
            meshes: vec![
                make_mesh("mesh0", 0, 6),  // 材質0: 頂点 0..6
                make_mesh("mesh1", 1, 3),  // 材質1: 頂点 6..9
                make_mesh("mesh2", 2, 4),  // 材質2: 頂点 9..13
            ],
            materials: vec![
                IrMaterial { name: "mat0".into(), ..Default::default() },
                IrMaterial { name: "mat1".into(), ..Default::default() },
                IrMaterial { name: "mat2".into(), ..Default::default() },
            ],
            morphs: vec![
                IrMorph {
                    name: "blink".into(),
                    name_en: String::new(),
                    panel: 2,
                    kind: IrMorphKind::Vertex(vec![
                        (1, Vec3::Y),   // mesh0 の頂点1
                        (7, Vec3::X),   // mesh1 の頂点1（除外対象）
                        (10, Vec3::Z),  // mesh2 の頂点1
                    ]),
                },
            ],
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
        if let IrMorphKind::Vertex(ref entries) = filtered.morphs[0].kind {
            assert_eq!(entries.len(), 2);
            // mesh0 の頂点1 → new index 1（そのまま）
            assert_eq!(entries[0].0, 1);
            // mesh2 の頂点1 → old 10, mesh1(3頂点)除外で -3 → new 7
            assert_eq!(entries[1].0, 7);
        } else {
            panic!("頂点モーフであるべき");
        }
    }

    /// 除外メッシュの頂点のみを参照するモーフが削除されることを確認
    #[test]
    fn test_morph_fully_removed() {
        let ir = IrModel {
            name: "test".into(),
            bones: vec![make_bone("Root")],
            meshes: vec![
                make_mesh("mesh0", 0, 3),
                make_mesh("mesh1", 1, 3),
            ],
            materials: vec![
                IrMaterial { name: "mat0".into(), ..Default::default() },
                IrMaterial { name: "mat1".into(), ..Default::default() },
            ],
            morphs: vec![
                IrMorph {
                    name: "smile".into(),
                    name_en: String::new(),
                    panel: 3,
                    kind: IrMorphKind::Vertex(vec![
                        (3, Vec3::Y),  // mesh1 の頂点0 のみ
                        (4, Vec3::X),  // mesh1 の頂点1 のみ
                    ]),
                },
                IrMorph {
                    name: "blink".into(),
                    name_en: String::new(),
                    panel: 2,
                    kind: IrMorphKind::Vertex(vec![
                        (0, Vec3::Y),  // mesh0 の頂点0（残る）
                    ]),
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
            meshes: vec![
                make_mesh("mesh0", 0, 3),
                make_mesh("mesh1", 1, 3),
            ],
            materials: vec![
                IrMaterial { name: "mat0".into(), ..Default::default() },
                IrMaterial { name: "mat1".into(), ..Default::default() },
            ],
            morphs: vec![
                // [0] smile: mesh1 のみ → 除外される
                IrMorph {
                    name: "smile".into(),
                    name_en: String::new(),
                    panel: 3,
                    kind: IrMorphKind::Vertex(vec![(3, Vec3::Y)]),
                },
                // [1] blink: mesh0 → 残る（new index 0）
                IrMorph {
                    name: "blink".into(),
                    name_en: String::new(),
                    panel: 2,
                    kind: IrMorphKind::Vertex(vec![(0, Vec3::Y)]),
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
            panic!("グループモーフであるべき");
        }
    }

    /// ネストしたグループモーフ（outer -> inner -> vertex）で
    /// vertex が除外されると inner, outer ともに正しく除去されることを確認
    #[test]
    fn test_nested_group_morph_cascade_removal() {
        let ir = IrModel {
            name: "test".into(),
            bones: vec![make_bone("Root")],
            meshes: vec![
                make_mesh("mesh0", 0, 3),
                make_mesh("mesh1", 1, 3),
            ],
            materials: vec![
                IrMaterial { name: "mat0".into(), ..Default::default() },
                IrMaterial { name: "mat1".into(), ..Default::default() },
            ],
            morphs: vec![
                // [0] vtx_alive: mesh0 → 残る
                IrMorph {
                    name: "vtx_alive".into(),
                    name_en: String::new(),
                    panel: 2,
                    kind: IrMorphKind::Vertex(vec![(0, Vec3::Y)]),
                },
                // [1] vtx_dead: mesh1 のみ → 除外
                IrMorph {
                    name: "vtx_dead".into(),
                    name_en: String::new(),
                    panel: 2,
                    kind: IrMorphKind::Vertex(vec![(3, Vec3::Y)]),
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
            panic!("グループモーフであるべき");
        }
    }

    /// 全材質非表示の場合、空モデルが返ることを確認
    #[test]
    fn test_all_materials_hidden() {
        let ir = IrModel {
            name: "test".into(),
            bones: vec![make_bone("Root")],
            meshes: vec![make_mesh("mesh0", 0, 3)],
            materials: vec![IrMaterial { name: "mat0".into(), ..Default::default() }],
            morphs: vec![
                IrMorph {
                    name: "blink".into(),
                    name_en: String::new(),
                    panel: 2,
                    kind: IrMorphKind::Vertex(vec![(0, Vec3::Y)]),
                },
            ],
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
            meshes: vec![
                make_mesh("mesh0", 0, 3),
                make_mesh("mesh1", 1, 3),
            ],
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
                IrTexture { filename: "tex0.png".into(), data: vec![0], mime_type: "image/png".into() },
                IrTexture { filename: "tex1.png".into(), data: vec![1], mime_type: "image/png".into() },
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
}
