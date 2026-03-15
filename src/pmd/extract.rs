use anyhow::Result;
use glam::{Mat4, Vec3, Vec4};
use std::collections::HashMap;
use std::path::Path;

use crate::convert::coord::PMX_SCALE;
use crate::intermediate::types::*;
use super::types::*;

/// PMD 座標 → glTF 座標（PMXと同じ座標系: 左手系Y-up）
#[inline]
fn pmx_pos_to_gltf(v: Vec3) -> Vec3 {
    Vec3::new(v.x / PMX_SCALE, v.y / PMX_SCALE, -v.z / PMX_SCALE)
}

#[inline]
fn pmx_normal_to_gltf(n: Vec3) -> Vec3 {
    Vec3::new(n.x, n.y, -n.z)
}

/// PMDモデルから IrModel を構築
pub fn pmd_to_ir(pmd: &PmdModel, pmd_path: &Path) -> Result<IrModel> {
    let pmd_dir = pmd_path.parent().unwrap_or(Path::new("."));
    let bones = extract_bones(pmd);
    let textures = extract_textures(pmd, pmd_dir);
    let mut materials = extract_materials(pmd, &textures);

    // 材質名テキストファイルの読み込み
    load_material_names(pmd_path, &mut materials);

    let (meshes, pmd_to_ir_vertex) = extract_meshes(pmd, &materials);
    let morphs = extract_morphs(pmd, &pmd_to_ir_vertex);
    let physics = extract_physics(pmd);

    Ok(IrModel {
        name: pmd.header.name.clone(),
        comment: pmd.header.comment.clone(),
        bones,
        meshes,
        materials,
        textures,
        morphs,
        physics,
        node_to_bone: HashMap::new(),
        source_format: SourceFormat::Pmd,
        rig_type: None,
        humanoid_bone_count: 0,
    })
}

/// PMDファイルと同名の .txt から材質名を読み込む
/// ファイルが存在し、行数が材質数と一致する場合のみ適用
fn load_material_names(pmd_path: &Path, materials: &mut [IrMaterial]) {
    let txt_path = pmd_path.with_extension("txt");
    if !txt_path.exists() {
        return;
    }
    let data = match std::fs::read(&txt_path) {
        Ok(d) => d,
        Err(_) => return,
    };
    // Shift_JIS デコード
    let (text, _, _) = encoding_rs::SHIFT_JIS.decode(&data);
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() != materials.len() {
        log::info!(
            "材質名テキスト '{}': 行数({})と材質数({})が不一致、スキップ",
            txt_path.display(), lines.len(), materials.len()
        );
        return;
    }
    for (mat, line) in materials.iter_mut().zip(lines.iter()) {
        mat.name = line.trim().to_string();
    }
    log::info!("材質名テキスト '{}' から{}材質名を適用", txt_path.display(), materials.len());
}

fn extract_bones(pmd: &PmdModel) -> Vec<IrBone> {
    let mut bones: Vec<IrBone> = pmd
        .bones
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let parent = if b.parent == 0xFFFF {
                None
            } else {
                Some(b.parent as usize)
            };

            let name_en = pmd.english_header.as_ref()
                .and_then(|eh| eh.bone_names.get(i))
                .cloned()
                .unwrap_or_default();

            let vrm_bone_name = crate::convert::bone_map::pmx_name_to_vrm_bone(&b.name)
                .map(|s| s.to_string());

            IrBone {
                name: b.name.clone(),
                name_en,
                vrm_bone_name,
                position: pmx_pos_to_gltf(b.position),
                global_mat: Mat4::IDENTITY,
                parent,
                children: Vec::new(),
                node_index: i,
                is_physics: false,
            }
        })
        .collect();

    // children 構築
    let parents: Vec<Option<usize>> = bones.iter().map(|b| b.parent).collect();
    for (i, parent) in parents.iter().enumerate() {
        if let Some(p) = parent {
            if *p < bones.len() {
                bones[*p].children.push(i);
            }
        }
    }

    // グローバル行列計算
    for i in 0..bones.len() {
        let pos = bones[i].position;
        let local = Mat4::from_translation(pos);
        if let Some(parent_idx) = bones[i].parent {
            if parent_idx < i {
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

    bones
}

fn extract_textures(pmd: &PmdModel, pmd_dir: &Path) -> Vec<IrTexture> {
    // PMD材質からユニークなテクスチャパスを収集
    let mut tex_paths: Vec<String> = Vec::new();
    for mat in &pmd.materials {
        if mat.texture_name.is_empty() {
            continue;
        }
        // "*" でスフィアテクスチャと分離
        let main_tex = mat.texture_name.split('*').next().unwrap_or("");
        if !main_tex.is_empty() && !tex_paths.contains(&main_tex.to_string()) {
            tex_paths.push(main_tex.to_string());
        }
    }

    tex_paths
        .iter()
        .map(|tex_path| {
            let normalized = tex_path.replace('\\', "/");
            let full_path = pmd_dir.join(&normalized);
            let filename = Path::new(&normalized)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| normalized.clone());

            let (data, mime_type) = if full_path.exists() {
                let data = std::fs::read(&full_path).unwrap_or_default();
                let ext = full_path
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
                (data, mime.to_string())
            } else {
                log::warn!("テクスチャファイルが見つかりません: {:?}", full_path);
                (Vec::new(), "application/octet-stream".to_string())
            };

            IrTexture {
                filename,
                data,
                mime_type,
            }
        })
        .collect()
}

/// テクスチャ名 → IrTexture インデックスのマッピング
fn build_tex_map(pmd: &PmdModel) -> HashMap<String, usize> {
    let mut map = HashMap::new();
    let mut idx = 0;
    for mat in &pmd.materials {
        if mat.texture_name.is_empty() {
            continue;
        }
        let main_tex = mat.texture_name.split('*').next().unwrap_or("");
        if !main_tex.is_empty() && !map.contains_key(main_tex) {
            map.insert(main_tex.to_string(), idx);
            idx += 1;
        }
    }
    map
}

fn extract_materials(pmd: &PmdModel, _textures: &[IrTexture]) -> Vec<IrMaterial> {
    let tex_map = build_tex_map(pmd);

    pmd.materials
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let main_tex = m.texture_name.split('*').next().unwrap_or("");
            let texture_index = if main_tex.is_empty() {
                None
            } else {
                tex_map.get(main_tex).copied()
            };

            let has_edge = m.edge_flag == 0; // PMD: 0=エッジあり

            IrMaterial {
                name: format!("材質{}", i + 1),
                diffuse: m.diffuse,
                specular: m.specular,
                specular_power: m.specular_power,
                ambient: m.ambient,
                texture_index,
                is_double_sided: false,
                is_mtoon: false,
                edge_color: if has_edge { Vec4::new(0.0, 0.0, 0.0, 1.0) } else { Vec4::ZERO },
                edge_size: if has_edge { 1.0 } else { 0.0 },
                shade_color: None,
                shade_texture_index: None,
                outline_width_texture_index: None,
                source_texture_name: None,
            }
        })
        .collect()
}

/// メッシュ抽出: 材質の face_count で分割
/// 戻り値: (meshes, PMDグローバル頂点Index → IrModel通し番号)
fn extract_meshes(pmd: &PmdModel, _materials: &[IrMaterial]) -> (Vec<IrMesh>, HashMap<u32, usize>) {
    let mut meshes = Vec::new();
    let mut face_offset = 0usize;
    let mut pmd_to_ir_vertex: HashMap<u32, usize> = HashMap::new();
    let mut ir_vertex_offset = 0usize;

    for (mat_idx, pmd_mat) in pmd.materials.iter().enumerate() {
        let face_count = (pmd_mat.face_count / 3) as usize;

        let mut vertex_map: HashMap<u16, u32> = HashMap::new();
        let mut local_vertices = Vec::new();
        let mut local_indices = Vec::new();

        for fi in face_offset..face_offset + face_count {
            let face = &pmd.faces[fi];
            // 面巻き順反転（PMD → glTF: b↔c swap）
            let reordered = [face[0], face[2], face[1]];
            for &global_idx in &reordered {
                let local_idx = if let Some(&existing) = vertex_map.get(&global_idx) {
                    existing
                } else {
                    let new_idx = local_vertices.len() as u32;
                    vertex_map.insert(global_idx, new_idx);

                    // PMDグローバル → IrModel通し番号を記録
                    pmd_to_ir_vertex.insert(global_idx as u32, ir_vertex_offset + new_idx as usize);

                    let v = &pmd.vertices[global_idx as usize];
                    let w1 = v.weight as f32 / 100.0;
                    let mut weights = vec![(v.bone1 as usize, w1)];
                    if w1 < 1.0 {
                        weights.push((v.bone2 as usize, 1.0 - w1));
                    }

                    local_vertices.push(IrVertex {
                        position: pmx_pos_to_gltf(v.position),
                        normal: pmx_normal_to_gltf(v.normal),
                        uv: v.uv,
                        weights,
                        edge_scale: if v.edge_flag == 0 { 1.0 } else { 0.0 },
                    });
                    new_idx
                };
                local_indices.push(local_idx);
            }
        }

        ir_vertex_offset += local_vertices.len();

        let name = format!("材質{}", mat_idx + 1);

        meshes.push(IrMesh {
            name,
            vertices: local_vertices,
            indices: local_indices,
            material_index: mat_idx,
            morph_targets: Vec::new(),
            node_index: 0,
        });

        face_offset += face_count;
    }

    (meshes, pmd_to_ir_vertex)
}

/// モーフ抽出
/// pmd_to_ir_vertex: PMDグローバル頂点Index → IrModel通し番号
fn extract_morphs(pmd: &PmdModel, pmd_to_ir_vertex: &HashMap<u32, usize>) -> Vec<IrMorph> {
    // base モーフを探す（morph_type == 0）
    let base = pmd.morphs.iter().find(|m| m.morph_type == 0);
    let base_verts = match base {
        Some(b) => &b.vertices,
        None => return Vec::new(),
    };

    pmd.morphs
        .iter()
        .filter(|m| m.morph_type != 0) // base 以外
        .enumerate()
        .map(|(i, m)| {
            let entries: Vec<(usize, Vec3)> = m
                .vertices
                .iter()
                .filter_map(|mv| {
                    // mv.index は base モーフ内のインデックス
                    let base_v = base_verts.get(mv.index as usize)?;
                    // base_v.index がPMDグローバル頂点インデックス → IrModel通し番号に変換
                    let ir_vi = pmd_to_ir_vertex.get(&base_v.index)?;
                    Some((
                        *ir_vi,
                        pmx_pos_to_gltf(mv.offset), // 変位ベクトル: Z反転 + スケール÷12.5
                    ))
                })
                .collect();

            let panel = match m.morph_type {
                1 => 1, // 眉
                2 => 2, // 目
                3 => 3, // 口
                _ => 4, // その他
            };

            let name_en = pmd.english_header.as_ref()
                .and_then(|eh| eh.morph_names.get(i))
                .cloned()
                .unwrap_or_default();

            IrMorph {
                name: m.name.clone(),
                name_en,
                panel,
                kind: IrMorphKind::Vertex(entries),
            }
        })
        .collect()
}

/// PMD剛体回転の調整: ボーン方向が下向き（Y<0）の場合、X回転を反転
/// PMXEditor の GetPoseMatrix_Bone と同様の Y軸反転規約に対応
fn adjust_pmd_rigid_rotation(pmd: &PmdModel, bone_index: Option<usize>, rot: Vec3) -> Vec3 {
    let Some(bi) = bone_index else { return rot; };
    let bone = &pmd.bones[bi];
    let child_idx = bone.child as usize;

    let dir = if child_idx < pmd.bones.len() && child_idx != bi {
        pmd.bones[child_idx].position - bone.position
    } else if bone.parent != 0xFFFF {
        bone.position - pmd.bones[bone.parent as usize].position
    } else {
        return rot;
    };

    if dir.y < 0.0 {
        Vec3::new(-rot.x, rot.y, rot.z)
    } else {
        rot
    }
}

fn extract_physics(pmd: &PmdModel) -> IrPhysics {
    let rigid_bodies = pmd
        .rigid_bodies
        .iter()
        .map(|r| {
            let bone_index = if (r.bone_index as usize) < pmd.bones.len() {
                Some(r.bone_index as usize)
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

            // PMDの剛体位置はボーンからの相対オフセット → 絶対座標に変換
            // 回転は絶対値（ワールド座標）
            let abs_position = if let Some(bi) = bone_index {
                pmd.bones[bi].position + r.position
            } else {
                r.position
            };

            IrRigidBody {
                name: r.name.clone(),
                bone_index,
                group: r.group,
                no_collision_mask: r.no_collision_mask,
                shape,
                position: abs_position,
                rotation: adjust_pmd_rigid_rotation(pmd, bone_index, r.rotation),
                mass: r.mass,
                linear_damping: r.linear_damping,
                angular_damping: r.angular_damping,
                restitution: r.restitution,
                friction: r.friction,
                physics_mode: r.physics_mode,
            }
        })
        .collect();

    let joints = pmd
        .joints
        .iter()
        .map(|j| IrJoint {
            name: j.name.clone(),
            rigid_a: j.rigid_a as usize,
            rigid_b: j.rigid_b as usize,
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
