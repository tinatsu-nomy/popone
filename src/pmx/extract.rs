use anyhow::Result;
use glam::{Mat4, Vec3, Vec4};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::convert::coord::{
    pmx_normal_to_gltf as pmx_normal_to_gltf_full, pmx_pos_to_gltf as pmx_pos_to_gltf_full,
};
use crate::intermediate::types::*;
use crate::pmx::types::*;

/// PMX 座標 → glTF 座標（VRM 1.0 変換の逆、PMXは常に is_vrm0=false）
#[inline]
fn pmx_pos_to_gltf(v: Vec3) -> Vec3 {
    pmx_pos_to_gltf_full(v, false)
}

/// PMX 法線 → glTF 法線（PMXは常に is_vrm0=false）
#[inline]
fn pmx_normal_to_gltf(n: Vec3) -> Vec3 {
    pmx_normal_to_gltf_full(n, false)
}

/// PMX モデルから IrModel を構築する
pub fn pmx_to_ir(pmx: &PmxModel, pmx_dir: &Path) -> Result<IrModel> {
    pmx_to_ir_with_aux(pmx, pmx_dir, None)
}

/// オンメモリ補助ファイル付きで PMX → IrModel 変換
pub fn pmx_to_ir_with_aux(
    pmx: &PmxModel,
    pmx_dir: &Path,
    aux_files: Option<&HashMap<PathBuf, Arc<[u8]>>>,
) -> Result<IrModel> {
    let bones = extract_bones(pmx);
    let textures = extract_textures(pmx, pmx_dir, aux_files);
    let materials = extract_materials(pmx);
    let (meshes, pmx_to_ir_vertex) = extract_meshes(pmx);
    let morphs = extract_morphs(pmx, &pmx_to_ir_vertex);
    let physics = extract_physics(pmx);

    Ok(IrModel {
        name: pmx.model_info.name.clone(),
        comment: pmx.model_info.comment.clone(),
        bones,
        meshes,
        materials,
        textures,
        morphs,
        physics,
        node_to_bone: HashMap::new(),
        source_format: SourceFormat::Pmx,
        rig_type: None,
        humanoid_bone_count: 0,
        astance_result: AStanceResult::NotRequested,
    })
}

/// ボーン抽出: PmxBone → IrBone
fn extract_bones(pmx: &PmxModel) -> Vec<IrBone> {
    let mut bones: Vec<IrBone> = pmx
        .bones
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let parent = if b.parent_index >= 0 {
                Some(b.parent_index as usize)
            } else {
                None
            };

            let vrm_bone_name =
                crate::convert::bone_map::pmx_name_to_vrm_bone(&b.name).map(|s| s.to_string());

            // テイル位置とボーンIndexを計算（glTF座標系）
            let (tail_position, tail_bone_index) = match &b.tail {
                BoneTail::BoneIndex(idx) => {
                    let ci = *idx as usize;
                    if ci < pmx.bones.len() {
                        (Some(pmx_pos_to_gltf(pmx.bones[ci].position)), Some(ci))
                    } else {
                        (None, None)
                    }
                }
                BoneTail::Offset(off) => {
                    if off.length_squared() > 0.0001 {
                        (Some(pmx_pos_to_gltf(b.position + *off)), None)
                    } else {
                        (None, None)
                    }
                }
            };

            IrBone {
                name: b.name.clone(),
                name_en: b.name_en.clone(),
                original_name: b.name.clone(),
                vrm_bone_name,
                position: pmx_pos_to_gltf(b.position),
                global_mat: Mat4::IDENTITY,
                parent,
                children: Vec::new(),
                node_index: i,
                is_physics: b.flags & BONE_FLAG_PHYS_AFTER != 0,
                tail_position,
                tail_bone_index,
                is_ik: false, // 後で IK Target/Link を設定
                is_ik_bone: b.flags & BONE_FLAG_IK != 0,
                is_translatable: b.flags & BONE_FLAG_TRANSLATABLE != 0,
                is_axis_fixed: b.flags & BONE_FLAG_AXIS_FIXED != 0,
                is_visible: b.flags & BONE_FLAG_VISIBLE != 0,
                grant: b.grant.as_ref().and_then(|g| {
                    if g.parent_index >= 0 && (g.parent_index as usize) < pmx.bones.len() {
                        Some(IrGrant {
                            parent_index: g.parent_index as usize,
                            ratio: g.ratio,
                            is_rotation: b.flags & BONE_FLAG_ROTATION_GRANT != 0,
                            is_move: b.flags & BONE_FLAG_MOVE_GRANT != 0,
                            is_local: b.flags & BONE_FLAG_LOCAL_GRANT != 0,
                        })
                    } else {
                        None
                    }
                }),
            }
        })
        .collect();

    // children を構築
    let parents: Vec<Option<usize>> = bones.iter().map(|b| b.parent).collect();
    for (i, parent) in parents.iter().enumerate() {
        if let Some(p) = parent {
            if *p < bones.len() {
                bones[*p].children.push(i);
            }
        }
    }

    // グローバル行列の計算（ルートから順に）
    for i in 0..bones.len() {
        let pos = bones[i].position;
        let local = Mat4::from_translation(pos);
        if let Some(parent_idx) = bones[i].parent {
            if parent_idx < i {
                // 親のグローバル行列を元にオフセット計算
                let parent_pos = bones[parent_idx].position;
                let offset = pos - parent_pos;
                let parent_mat = bones[parent_idx].global_mat;
                bones[i].global_mat = parent_mat * Mat4::from_translation(offset);
            } else {
                bones[i].global_mat = local;
            }
        } else {
            bones[i].global_mat = local;
        }
    }

    // IK影響下ボーン（Linkのみ）をマーク — Targetはブルー表示
    for b in &pmx.bones {
        if let Some(ref ik) = b.ik {
            for link in &ik.links {
                let li = link.bone_index as usize;
                if li < bones.len() {
                    bones[li].is_ik = true;
                }
            }
        }
    }

    bones
}

/// テクスチャ抽出: 外部ファイルまたは aux_files から読み込み
fn extract_textures(
    pmx: &PmxModel,
    pmx_dir: &Path,
    aux_files: Option<&HashMap<PathBuf, Arc<[u8]>>>,
) -> Vec<IrTexture> {
    pmx.textures
        .iter()
        .map(|tex_path| {
            // パス区切りを正規化
            let normalized = tex_path.replace('\\', "/");
            let full_path = pmx_dir.join(&normalized);
            let filename = Path::new(&normalized)
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| normalized.clone());

            let ext = Path::new(&normalized)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();
            let mime = match ext.as_str() {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "bmp" => "image/bmp",
                "tga" => "image/tga",
                _ => "application/octet-stream",
            };

            // aux_files があればそこから、なければファイルシステムから読む
            let data = if let Some(aux) = aux_files {
                let key = PathBuf::from(&normalized);
                if let Some(cached) = aux.get(&key) {
                    cached.to_vec()
                } else {
                    log::warn!("aux_files にテクスチャが見つかりません: {:?}", key);
                    Vec::new()
                }
            } else if full_path.exists() {
                std::fs::read(&full_path).unwrap_or_default()
            } else {
                log::warn!("テクスチャファイルが見つかりません: {:?}", full_path);
                Vec::new()
            };

            IrTexture {
                filename,
                data,
                mime_type: mime.to_string(),
            }
        })
        .collect()
}

/// 材質抽出: PmxMaterial → IrMaterial
fn extract_materials(pmx: &PmxModel) -> Vec<IrMaterial> {
    pmx.materials
        .iter()
        .map(|m| {
            let texture_index = m.texture_index.map(|i| i as usize);
            let cull_mode = if m.draw_flags & 0x01 != 0 {
                CullMode::None
            } else {
                CullMode::Back
            };
            let has_edge = m.draw_flags & 0x10 != 0;

            // スフィアモード: 3（サブテクスチャ）は非対応
            let sphere_mode = if m.sphere_mode == 3 {
                log::warn!(
                    "材質 '{}': sphere_mode=3（サブテクスチャ）は非対応、無効化します",
                    m.name
                );
                0
            } else {
                m.sphere_mode
            };
            let sphere_texture_index = if sphere_mode > 0 {
                m.sphere_texture_index.map(|i| i as usize)
            } else {
                None
            };

            // トゥーン参照 (-1 は未設定 → トゥーンなし)
            let (toon_texture_index, toon_shared_index) = match &m.toon_ref {
                PmxToonRef::Texture(i) if *i >= 0 => (Some(*i as usize), None),
                PmxToonRef::Texture(_) => (None, None),
                PmxToonRef::Shared(i) => (None, Some(*i)),
            };

            IrMaterial {
                name: m.name.clone(),
                diffuse: m.diffuse,
                specular: m.specular,
                specular_power: m.specular_power,
                ambient: m.ambient,
                texture_index,
                base_color_tex_info: None,
                cull_mode,
                is_mtoon: false,
                edge_color: if has_edge { m.edge_color } else { Vec4::ZERO },
                edge_size: if has_edge { m.edge_size } else { 0.0 },
                shade_color: None,
                shade_texture: None,
                shading_toony_factor: 0.9,
                shading_shift_factor: 0.0,
                outline_width_texture: None,
                outline_width_tex_channel: ColorChannel::G,
                outline_width_mode: OutlineWidthMode::None,
                outline_width_factor: 0.0,
                outline_lighting_mix: 1.0,
                source_texture_name: None,
                source_format: SourceFormat::Pmx,
                sphere_texture_index,
                sphere_mode,
                toon_texture_index,
                toon_shared_index,
                parametric_rim_color: Vec3::ZERO,
                parametric_rim_fresnel_power: 5.0,
                parametric_rim_lift: 0.0,
                rim_lighting_mix: 1.0,
                gi_equalization_factor: 0.9,
                matcap_factor: Vec3::ONE,
                matcap_texture: None,
                shading_shift_texture: None,
                shading_shift_texture_scale: 1.0,
                rim_multiply_texture: None,
                uv_animation_scroll_x_speed: 0.0,
                uv_animation_scroll_y_speed: 0.0,
                uv_animation_rotation_speed: 0.0,
                uv_animation_mask_texture: None,
                uv_anim_mask_tex_channel: ColorChannel::B,
                alpha_mode: AlphaMode::Opaque,
                alpha_cutoff: 0.5,
                render_queue_offset: 0,
                emissive_factor: Vec3::ZERO,
                emissive_texture: None,
                normal_texture: None,
                normal_texture_scale: 1.0,
            }
        })
        .collect()
}

/// メッシュ抽出: 材質の face_count で分割
/// 戻り値: (meshes, pmx_global_vertex → ir_global_vertex のマッピング)
fn extract_meshes(pmx: &PmxModel) -> (Vec<IrMesh>, HashMap<u32, usize>) {
    let mut meshes = Vec::new();
    let mut face_offset = 0usize;
    // PMX グローバル頂点Index → IrModel 通し番号（メッシュ0の頂点0=0, 頂点1=1, ..., メッシュ1の頂点0=N, ...）
    let mut pmx_to_ir_vertex: HashMap<u32, usize> = HashMap::new();
    let mut ir_vertex_offset = 0usize;

    for (mat_idx, mat) in pmx.materials.iter().enumerate() {
        let face_count = (mat.face_count / 3) as usize;
        if face_count == 0 {
            meshes.push(IrMesh {
                name: mat.name.clone(),
                vertices: Vec::new(),
                indices: Vec::new(),
                material_index: mat_idx,
                morph_targets: Vec::new(),
                node_index: 0,
                uvs1: Vec::new(),
            });
            face_offset += face_count;
            continue;
        }

        // この材質が参照する面から頂点インデックスを収集
        let mut vertex_map: HashMap<u32, u32> = HashMap::new();
        let mut local_vertices = Vec::new();
        let mut local_indices = Vec::new();

        for fi in face_offset..face_offset + face_count {
            let face = &pmx.faces[fi];
            // 面巻き順を反転（PMX → glTF: b↔c swap）
            let reordered = [face[0], face[2], face[1]];
            for &global_idx in &reordered {
                let local_idx = if let Some(&existing) = vertex_map.get(&global_idx) {
                    existing
                } else {
                    let new_idx = local_vertices.len() as u32;
                    vertex_map.insert(global_idx, new_idx);

                    // PMXグローバル → IrModel通し番号を記録
                    pmx_to_ir_vertex.insert(global_idx, ir_vertex_offset + new_idx as usize);

                    let v = &pmx.vertices[global_idx as usize];
                    let (w_arr, w_cnt) = match &v.weight {
                        PmxWeightType::Bdef1 { bone } => {
                            ([(*bone as usize, 1.0), (0, 0.0), (0, 0.0), (0, 0.0)], 1u8)
                        }
                        PmxWeightType::Bdef2 {
                            bone1,
                            bone2,
                            weight1,
                        } => {
                            if *weight1 < 1.0 {
                                (
                                    [
                                        (*bone1 as usize, *weight1),
                                        (*bone2 as usize, 1.0 - weight1),
                                        (0, 0.0),
                                        (0, 0.0),
                                    ],
                                    2,
                                )
                            } else {
                                (
                                    [(*bone1 as usize, *weight1), (0, 0.0), (0, 0.0), (0, 0.0)],
                                    1,
                                )
                            }
                        }
                        PmxWeightType::Bdef4 { bones, weights } => {
                            let mut arr = [(0usize, 0.0f32); 4];
                            let mut cnt = 0u8;
                            for (&b, &w) in bones.iter().zip(weights.iter()) {
                                if w > 0.0 && cnt < 4 {
                                    arr[cnt as usize] = (b as usize, w);
                                    cnt += 1;
                                }
                            }
                            (arr, cnt)
                        }
                    };

                    local_vertices.push(IrVertex {
                        position: pmx_pos_to_gltf(v.position),
                        normal: pmx_normal_to_gltf(v.normal),
                        uv: v.uv,
                        tangent: Vec4::ZERO, // MikkTSpace で後から生成
                        weights: w_arr,
                        weight_count: w_cnt,
                        edge_scale: v.edge_scale,
                    });
                    new_idx
                };
                local_indices.push(local_idx);
            }
        }

        ir_vertex_offset += local_vertices.len();

        let mut ir_mesh = IrMesh {
            name: mat.name.clone(),
            vertices: local_vertices,
            indices: local_indices,
            material_index: mat_idx,
            morph_targets: Vec::new(),
            node_index: 0,
            uvs1: Vec::new(),
        };
        crate::intermediate::tangent::generate_tangents(&mut ir_mesh, 0);
        meshes.push(ir_mesh);

        face_offset += face_count;
    }

    // 頂点モーフをメッシュに分配
    distribute_vertex_morphs(pmx, &mut meshes);

    (meshes, pmx_to_ir_vertex)
}

/// PMXの頂点モーフを各メッシュに分配
fn distribute_vertex_morphs(pmx: &PmxModel, meshes: &mut [IrMesh]) {
    // グローバル頂点Index → (mesh_idx, local_vertex_idx) のマッピングを構築
    let mut global_to_local: HashMap<u32, Vec<(usize, u32)>> = HashMap::new();
    let mut face_offset = 0usize;
    for (mesh_idx, mat) in pmx.materials.iter().enumerate() {
        let face_count = (mat.face_count / 3) as usize;
        let mut vertex_map: HashMap<u32, u32> = HashMap::new();
        let mut next_local = 0u32;

        for fi in face_offset..face_offset + face_count {
            let face = &pmx.faces[fi];
            for &global_idx in face {
                if let std::collections::hash_map::Entry::Vacant(e) = vertex_map.entry(global_idx) {
                    e.insert(next_local);
                    next_local += 1;
                }
            }
        }

        for (&global_idx, &local_idx) in &vertex_map {
            global_to_local
                .entry(global_idx)
                .or_default()
                .push((mesh_idx, local_idx));
        }

        face_offset += face_count;
    }

    // 各頂点モーフをメッシュの morph_targets に分配
    for morph in &pmx.morphs {
        if let PmxMorphOffsets::Vertex(offsets) = &morph.offsets {
            // 各メッシュに対するモーフターゲットを構築（疎表現）
            let mesh_count = meshes.len();
            let mut mesh_offsets: Vec<Vec<(u32, Vec3)>> =
                (0..mesh_count).map(|_| Vec::new()).collect();

            for off in offsets {
                let gltf_offset = pmx_pos_to_gltf(off.offset); // 変位ベクトル: Z反転 + スケール÷12.5
                if let Some(targets) = global_to_local.get(&off.vertex_index) {
                    for &(mesh_idx, local_idx) in targets {
                        mesh_offsets[mesh_idx].push((local_idx, gltf_offset));
                    }
                }
            }

            for (mesh_idx, mut offsets) in mesh_offsets.into_iter().enumerate() {
                // このメッシュに影響がある場合のみ追加
                if !offsets.is_empty() {
                    offsets.sort_by_key(|&(vi, _)| vi);
                    meshes[mesh_idx].morph_targets.push(IrMorphTarget {
                        name: morph.name.clone(),
                        position_offsets: offsets,
                        normal_offsets: Vec::new(),
                        tangent_offsets: Vec::new(),
                    });
                }
            }
        }
    }
}

/// モーフ抽出: 頂点モーフ・グループモーフ → IrMorph
/// pmx_to_ir_vertex: PMXグローバル頂点Index → IrModel通し番号
///
/// ボーン/材質/UV モーフはスキップされるため、グループモーフ内の
/// サブモーフ参照インデックスを IrModel 上のインデックスにリマッピングする。
fn extract_morphs(pmx: &PmxModel, pmx_to_ir_vertex: &HashMap<u32, usize>) -> Vec<IrMorph> {
    // Pass 1: PMX インデックス → IrModel インデックスのマッピングを構築
    // スキップされるモーフは None になる
    let mut pmx_to_ir_morph: Vec<Option<usize>> = Vec::with_capacity(pmx.morphs.len());
    let mut ir_idx = 0usize;
    for m in &pmx.morphs {
        match &m.offsets {
            PmxMorphOffsets::Vertex(_) | PmxMorphOffsets::Group(_) => {
                pmx_to_ir_morph.push(Some(ir_idx));
                ir_idx += 1;
            }
            _ => {
                pmx_to_ir_morph.push(None);
            }
        }
    }

    // Pass 2: モーフを変換（リマッピング済みインデックスを使用）
    pmx.morphs
        .iter()
        .filter_map(|m| {
            let kind = match &m.offsets {
                PmxMorphOffsets::Vertex(offsets) => {
                    let entries: Vec<(usize, Vec3)> = offsets
                        .iter()
                        .filter_map(|off| {
                            let ir_vi = pmx_to_ir_vertex.get(&off.vertex_index)?;
                            Some((
                                *ir_vi,
                                pmx_pos_to_gltf(off.offset), // 変位ベクトル: Z反転 + スケール÷12.5
                            ))
                        })
                        .collect();
                    IrMorphKind::Vertex {
                        positions: entries,
                        normals: Vec::new(),
                        tangents: Vec::new(),
                    }
                }
                PmxMorphOffsets::Group(offsets) => {
                    let entries: Vec<(usize, f32)> = offsets
                        .iter()
                        .filter_map(|off| {
                            let pmx_idx = off.morph_index as usize;
                            match pmx_to_ir_morph.get(pmx_idx).copied().flatten() {
                                Some(ir_idx) => Some((ir_idx, off.weight)),
                                None => {
                                    log::warn!(
                                        "グループモーフ: サブモーフ[{}]は非対応モーフ種別のためスキップ",
                                        pmx_idx
                                    );
                                    None
                                }
                            }
                        })
                        .collect();
                    IrMorphKind::Group(entries)
                }
                _ => return None, // ボーン/材質/UV モーフはスキップ
            };

            Some(IrMorph {
                name: m.name.clone(),
                name_en: m.name_en.clone(),
                panel: m.panel,
                kind,
            })
        })
        .collect()
}

/// 物理情報抽出
fn extract_physics(pmx: &PmxModel) -> IrPhysics {
    let rigid_bodies = pmx
        .rigid_bodies
        .iter()
        .map(|r| {
            // bone_index=-1（関連ボーンなし）の場合はボーン0（センター）に追従
            let bone_index = if r.bone_index >= 0 {
                Some(r.bone_index as usize)
            } else if !pmx.bones.is_empty() {
                Some(0)
            } else {
                None
            };

            let shape = match r.shape {
                0 => RigidShape::Sphere { radius: r.size.x },
                1 => RigidShape::Box { size: r.size },
                2 => RigidShape::Capsule {
                    radius: r.size.x,
                    height: r.size.y,
                },
                _ => RigidShape::Sphere { radius: r.size.x },
            };

            IrRigidBody {
                name: r.name.clone(),
                bone_index,
                group: r.group,
                no_collision_mask: r.no_collision_mask,
                shape,
                position: r.position, // PMX座標のまま保持（ビューアがPMX座標で描画するため）
                rotation: r.rotation,
                mass: r.mass,
                linear_damping: r.linear_damping,
                angular_damping: r.angular_damping,
                restitution: r.restitution,
                friction: r.friction,
                physics_mode: r.physics_mode,
            }
        })
        .collect();

    let joints = pmx
        .joints
        .iter()
        .map(|j| IrJoint {
            name: j.name.clone(),
            rigid_a: if j.rigid_a >= 0 {
                j.rigid_a as usize
            } else {
                0
            },
            rigid_b: if j.rigid_b >= 0 {
                j.rigid_b as usize
            } else {
                0
            },
            position: j.position, // PMX座標のまま保持
            rotation: j.rotation,
            move_limit_lo: j.move_limit_lo,
            move_limit_hi: j.move_limit_hi,
            rot_limit_lo: j.rot_limit_lo,
            rot_limit_hi: j.rot_limit_hi,
            spring_move: j.spring_move,
            spring_rot: j.spring_rot,
        })
        .collect();

    IrPhysics {
        rigid_bodies,
        joints,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pmx_to_ir_seed_san() {
        let Some(pmx_path) = crate::test_util::try_test_file(crate::test_util::seed_san_pmx())
        else {
            return;
        };

        let pmx = crate::pmx::reader::read_pmx(&pmx_path).expect("PMX読み込み失敗");
        let pmx_dir = pmx_path.parent().unwrap();
        let ir = pmx_to_ir(&pmx, pmx_dir).expect("PMX→IrModel変換失敗");

        assert_eq!(ir.source_format, SourceFormat::Pmx);
        assert_eq!(ir.bones.len(), 179);
        assert_eq!(ir.materials.len(), 17);
        assert_eq!(ir.meshes.len(), 17); // 材質数と一致
        assert!(!ir.name.is_empty());

        // 頂点数の合計確認
        let total_verts: usize = ir.meshes.iter().map(|m| m.vertices.len()).sum();
        assert!(total_verts > 0, "頂点数が0");

        // 面数の合計確認
        let total_faces: usize = ir.meshes.iter().map(|m| m.indices.len() / 3).sum();
        assert_eq!(total_faces, 45058);

        // 物理情報
        assert_eq!(ir.physics.rigid_bodies.len(), 36);
        assert_eq!(ir.physics.joints.len(), 19);

        // ボーン親子関係の整合性
        for (i, bone) in ir.bones.iter().enumerate() {
            if let Some(parent) = bone.parent {
                assert!(parent < ir.bones.len(), "ボーン{}の親{}が範囲外", i, parent);
                assert!(
                    ir.bones[parent].children.contains(&i),
                    "ボーン{}が親{}のchildrenに含まれていない",
                    i,
                    parent
                );
            }
        }

        // テクスチャデータ確認
        for tex in &ir.textures {
            assert!(
                !tex.data.is_empty(),
                "テクスチャ '{}' のデータが空",
                tex.filename
            );
        }
    }
}
