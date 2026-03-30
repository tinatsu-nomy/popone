use crate::error::Result;
use glam::{Mat4, Vec3, Vec4};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::types::*;
use crate::convert::coord::{
    pmx_normal_to_gltf as pmx_normal_to_gltf_full, pmx_pos_to_gltf as pmx_pos_to_gltf_full,
};
use crate::intermediate::types::*;

/// PMD 座標 → glTF 座標（PMXと同じ座標系: 左手系Y-up、is_vrm0=false）
#[inline]
fn pmx_pos_to_gltf(v: Vec3) -> Vec3 {
    pmx_pos_to_gltf_full(v, false)
}

/// PMD 法線 → glTF 法線（is_vrm0=false）
#[inline]
fn pmx_normal_to_gltf(n: Vec3) -> Vec3 {
    pmx_normal_to_gltf_full(n, false)
}

/// PMDモデルから IrModel を構築
pub fn pmd_to_ir(pmd: &PmdModel, pmd_path: &Path) -> Result<IrModel> {
    pmd_to_ir_with_aux(pmd, pmd_path, None)
}

/// オンメモリ補助ファイル付きで PMD → IrModel 変換
pub fn pmd_to_ir_with_aux(
    pmd: &PmdModel,
    pmd_path: &Path,
    aux_files: Option<&HashMap<PathBuf, Arc<[u8]>>>,
) -> Result<IrModel> {
    let pmd_dir = pmd_path.parent().unwrap_or(Path::new("."));
    let bones = extract_bones(pmd);
    let textures = extract_textures(pmd, pmd_dir, aux_files);
    let mut materials = extract_materials(pmd, &textures);

    // 材質名テキストファイルの読み込み
    load_material_names(pmd_path, aux_files, &mut materials);

    let (meshes, _pmd_to_ir_vertex) = extract_meshes(pmd, &materials);
    let morphs = extract_morphs(pmd, &meshes);
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
        astance_result: AStanceResult::NotRequested,
    })
}

/// PMDファイルと同名の .txt から材質名を読み込む
/// ファイルが存在し、行数が材質数と一致する場合のみ適用
fn load_material_names(
    pmd_path: &Path,
    aux_files: Option<&HashMap<PathBuf, Arc<[u8]>>>,
    materials: &mut [IrMaterial],
) {
    let txt_path = pmd_path.with_extension("txt");
    let txt_filename = txt_path
        .file_name()
        .map(|f| PathBuf::from(f))
        .unwrap_or_default();

    // aux_files があればそこから、なければファイルシステムから読む
    let data = if let Some(aux) = aux_files {
        if let Some(cached) = aux.get(&txt_filename) {
            cached.to_vec()
        } else {
            return; // aux_files にもなければスキップ
        }
    } else {
        if !txt_path.exists() {
            return;
        }
        match std::fs::read(&txt_path) {
            Ok(d) => d,
            Err(_) => return,
        }
    };
    // Shift_JIS デコード
    let (text, _, _) = encoding_rs::SHIFT_JIS.decode(&data);
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() != materials.len() {
        log::info!(
            "材質名テキスト '{}': 行数({})と材質数({})が不一致、スキップ",
            txt_path.display(),
            lines.len(),
            materials.len()
        );
        return;
    }
    for (mat, line) in materials.iter_mut().zip(lines.iter()) {
        mat.name = line.trim().to_string();
    }
    log::info!(
        "材質名テキスト '{}' から{}材質名を適用",
        txt_path.display(),
        materials.len()
    );
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

            let name_en = pmd
                .english_header
                .as_ref()
                .and_then(|eh| eh.bone_names.get(i))
                .cloned()
                .unwrap_or_default();

            let vrm_bone_name =
                crate::convert::bone_map::pmx_name_to_vrm_bone(&b.name).map(|s| s.to_string());

            // テイル位置: PMDの child ボーンIndex（0xFFFF/0 = なし）
            let (tail_position, tail_bone_index) =
                if b.child != 0xFFFF && b.child != 0 && (b.child as usize) < pmd.bones.len() {
                    let ci = b.child as usize;
                    (Some(pmx_pos_to_gltf(pmd.bones[ci].position)), Some(ci))
                } else {
                    (None, None)
                };

            IrBone {
                name: b.name.clone(),
                name_en,
                original_name: b.name.clone(),
                vrm_bone_name,
                position: pmx_pos_to_gltf(b.position),
                global_mat: Mat4::IDENTITY,
                parent,
                children: Vec::new(),
                node_index: i,
                is_physics: false,
                tail_position,
                tail_bone_index,
                is_ik: false, // 後で IK Target/Link を設定
                is_ik_bone: b.bone_type == 2,
                is_translatable: b.bone_type == 1,
                is_axis_fixed: false, // PMDには軸制限フラグなし
                is_visible: b.bone_type != 7,
                grant: None, // PMDの付与は未対応
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

    // IK影響下ボーン（Chainのみ）をマーク — Targetはブルー表示
    for ik in &pmd.ik_list {
        for &chain_bone in &ik.chain {
            let ci = chain_bone as usize;
            if ci < bones.len() {
                bones[ci].is_ik = true;
            }
        }
    }

    bones
}

/// テクスチャファイル名を分類: .sph→乗算(1), .spa→加算(2), それ以外→メインテクスチャ
fn classify_tex(s: &str) -> (Option<&str>, Option<(&str, u8)>) {
    let lower = s.to_ascii_lowercase();
    if lower.ends_with(".sph") {
        (None, Some((s, 1u8)))
    } else if lower.ends_with(".spa") {
        (None, Some((s, 2u8)))
    } else {
        (Some(s), None)
    }
}

/// テクスチャ名を main/sphere に分解し、.sph→乗算, .spa→加算 を判定
fn parse_pmd_texture_slots(name: &str) -> (Option<&str>, Option<(&str, u8)>) {
    let mut parts = name.split('*').filter(|s| !s.is_empty());
    match (parts.next(), parts.next()) {
        (Some(a), Some(b)) => {
            let (ma, sa) = classify_tex(a);
            let (mb, sb) = classify_tex(b);
            (ma.or(mb), sa.or(sb))
        }
        (Some(a), None) => classify_tex(a),
        _ => (None, None),
    }
}

fn extract_textures(
    pmd: &PmdModel,
    pmd_dir: &Path,
    aux_files: Option<&HashMap<PathBuf, Arc<[u8]>>>,
) -> Vec<IrTexture> {
    // PMD材質からユニークなテクスチャパスを収集（メイン + スフィア + トゥーン）
    let mut tex_paths: Vec<String> = Vec::new();
    for mat in &pmd.materials {
        if mat.texture_name.is_empty() {
            continue;
        }
        let (main_tex, sphere) = parse_pmd_texture_slots(&mat.texture_name);
        if let Some(path) = main_tex {
            if !path.is_empty() && !tex_paths.contains(&path.to_string()) {
                tex_paths.push(path.to_string());
            }
        }
        if let Some((path, _)) = sphere {
            if !path.is_empty() && !tex_paths.contains(&path.to_string()) {
                tex_paths.push(path.to_string());
            }
        }
    }
    // トゥーンテクスチャもテクスチャ表に登録（モデル同梱分のみ）
    for toon_name in &pmd.toon_textures {
        if !toon_name.is_empty() && !tex_paths.contains(toon_name) {
            let normalized = toon_name.replace('\\', "/");
            let full_path = pmd_dir.join(&normalized);
            // aux_files にあるか、ファイルシステムに存在する場合のみ登録
            let exists = if let Some(aux) = aux_files {
                aux.contains_key(&PathBuf::from(&normalized))
            } else {
                full_path.exists()
            };
            if exists {
                tex_paths.push(toon_name.clone());
            }
        }
    }

    tex_paths
        .iter()
        .map(|tex_path| {
            let normalized = tex_path.replace('\\', "/");
            let full_path = pmd_dir.join(&normalized);
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
                "sph" | "spa" => "image/bmp", // スフィアマップは通常 BMP
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

/// テクスチャ名 → IrTexture インデックスのマッピング
/// extract_textures の結果から構築し、インデックスの一貫性を保証する
fn build_tex_map(pmd: &PmdModel, textures: &[IrTexture]) -> HashMap<String, usize> {
    // IrTexture の filename からインデックスマップを構築
    let filename_to_idx: HashMap<&str, usize> = textures
        .iter()
        .enumerate()
        .map(|(i, t)| (t.filename.as_str(), i))
        .collect();

    let mut map = HashMap::new();
    // 材質テクスチャ（メイン + スフィア）
    for mat in &pmd.materials {
        if mat.texture_name.is_empty() {
            continue;
        }
        let (main_tex, sphere) = parse_pmd_texture_slots(&mat.texture_name);
        if let Some(path) = main_tex {
            if !path.is_empty() && !map.contains_key(path) {
                if let Some(&idx) = filename_to_idx.get(path) {
                    map.insert(path.to_string(), idx);
                }
            }
        }
        if let Some((path, _)) = sphere {
            if !path.is_empty() && !map.contains_key(path) {
                if let Some(&idx) = filename_to_idx.get(path) {
                    map.insert(path.to_string(), idx);
                }
            }
        }
    }
    // トゥーンテクスチャ
    for toon_name in &pmd.toon_textures {
        if !toon_name.is_empty() && !map.contains_key(toon_name) {
            if let Some(&idx) = filename_to_idx.get(toon_name.as_str()) {
                map.insert(toon_name.clone(), idx);
            }
        }
    }
    map
}

fn extract_materials(pmd: &PmdModel, textures: &[IrTexture]) -> Vec<IrMaterial> {
    let tex_map = build_tex_map(pmd, textures);

    pmd.materials
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let (main_tex, sphere) = parse_pmd_texture_slots(&m.texture_name);
            let texture_index = main_tex.and_then(|path| {
                if path.is_empty() {
                    None
                } else {
                    tex_map.get(path).copied()
                }
            });

            // スフィアテクスチャ
            let (sphere_texture_index, sphere_mode) = match sphere {
                Some((path, mode)) => (tex_map.get(path).copied(), mode),
                None => (None, 0),
            };

            // トゥーン参照: toon_index が 0..=9 → 共有トゥーン、
            // pmd.toon_textures にファイル名があればそれを個別トゥーンとして使用
            let (toon_texture_index, toon_shared_index) = if m.toon_index <= 9 {
                let toon_name = &pmd.toon_textures[m.toon_index as usize];
                if !toon_name.is_empty() {
                    // 個別トゥーンテクスチャがテクスチャリストに存在するか確認
                    if let Some(&idx) = tex_map.get(toon_name) {
                        (Some(idx), None)
                    } else {
                        // ファイルが見つからなければ共有トゥーンにフォールバック
                        (None, Some(m.toon_index))
                    }
                } else {
                    (None, Some(m.toon_index))
                }
            } else {
                log::warn!(
                    "材質{}: toon_index={} が範囲外（0-9）、トゥーンなしとして扱います",
                    i + 1,
                    m.toon_index
                );
                (None, None)
            };

            let has_edge = m.edge_flag == 1; // PMD: 1=エッジあり

            IrMaterial {
                name: format!("材質{}", i + 1),
                diffuse: m.diffuse,
                specular: m.specular,
                specular_power: m.specular_power,
                ambient: m.ambient,
                texture_index,
                base_color_tex_info: None,
                cull_mode: CullMode::Back,
                edge_color: if has_edge {
                    Vec4::new(0.0, 0.0, 0.0, 1.0)
                } else {
                    Vec4::ZERO
                },
                edge_size: if has_edge { 1.0 } else { 0.0 },
                mtoon: None,
                shader_family: ShaderFamily::Other,
                source_texture_name: None,
                source_format: SourceFormat::Pmd,
                sphere_texture_index,
                sphere_mode,
                toon_texture_index,
                toon_shared_index,
                alpha_mode: AlphaMode::Opaque,
                alpha_cutoff: 0.5,
                emissive_factor: Vec3::ZERO,
                emissive_texture: None,
                normal_texture: None,
                normal_texture_scale: 1.0,
                source_material: None,
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
                    let (w_arr, w_cnt) = if w1 < 1.0 {
                        (
                            [
                                (v.bone1 as usize, w1),
                                (v.bone2 as usize, 1.0 - w1),
                                (0, 0.0),
                                (0, 0.0),
                            ],
                            2u8,
                        )
                    } else {
                        ([(v.bone1 as usize, w1), (0, 0.0), (0, 0.0), (0, 0.0)], 1u8)
                    };

                    local_vertices.push(IrVertex {
                        position: pmx_pos_to_gltf(v.position),
                        normal: pmx_normal_to_gltf(v.normal),
                        uv: v.uv,
                        tangent: Vec4::ZERO, // MikkTSpace で後から生成
                        weights: w_arr,
                        weight_count: w_cnt,
                        // PMD 頂点: edge_flag=0 はエッジ有効、1 はエッジ無効（材質の edge_flag とは逆）
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
            uvs1: Vec::new(),
        });

        face_offset += face_count;
    }

    // PMD 頂点モーフをメッシュの morph_targets に分配（generate_tangents の前に実行）
    distribute_pmd_vertex_morphs(pmd, &mut meshes);

    // 接線生成（tangent w 不一致時に頂点を分割 + morph_targets も複製）
    for mesh in &mut meshes {
        crate::intermediate::tangent::generate_tangents(mesh, 0);
    }

    (meshes, pmd_to_ir_vertex)
}

/// PMD 頂点モーフをメッシュの morph_targets に分配（generate_tangents の前に実行）
fn distribute_pmd_vertex_morphs(pmd: &PmdModel, meshes: &mut [IrMesh]) {
    let base = match pmd.morphs.iter().find(|m| m.morph_type == 0) {
        Some(b) => b,
        None => return,
    };

    // PMDグローバル頂点Index → (mesh_idx, local_vertex_idx) のマッピング
    let mut global_to_local: HashMap<u16, Vec<(usize, u32)>> = HashMap::new();
    let mut face_offset = 0usize;
    for (mesh_idx, pmd_mat) in pmd.materials.iter().enumerate() {
        let face_count = (pmd_mat.face_count / 3) as usize;
        let mut vertex_map: HashMap<u16, u32> = HashMap::new();
        let mut next_local = 0u32;
        for fi in face_offset..face_offset + face_count {
            let face = &pmd.faces[fi];
            // extract_meshes と同じ巻き順（b↔c swap）で走査して
            // ローカル頂点インデックスの割り当て順を一致させる
            let reordered = [face[0], face[2], face[1]];
            for &global_idx in &reordered {
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

    // 各表情モーフをメッシュの morph_targets に分配
    for m in pmd.morphs.iter().filter(|m| m.morph_type != 0) {
        let mesh_count = meshes.len();
        let mut mesh_offsets: Vec<Vec<(u32, Vec3)>> = (0..mesh_count).map(|_| Vec::new()).collect();

        for mv in &m.vertices {
            let base_v = match base.vertices.get(mv.index as usize) {
                Some(v) => v,
                None => continue,
            };
            let gltf_offset = pmx_pos_to_gltf(mv.offset);
            if let Some(targets) = global_to_local.get(&(base_v.index as u16)) {
                for &(mesh_idx, local_idx) in targets {
                    mesh_offsets[mesh_idx].push((local_idx, gltf_offset));
                }
            }
        }

        for (mesh_idx, mut offsets) in mesh_offsets.into_iter().enumerate() {
            if !offsets.is_empty() {
                offsets.sort_by_key(|&(vi, _)| vi);
                meshes[mesh_idx].morph_targets.push(IrMorphTarget {
                    name: m.name.clone(),
                    position_offsets: offsets,
                    normal_offsets: Vec::new(),
                    tangent_offsets: Vec::new(),
                });
            }
        }
    }
}

/// モーフ抽出: mesh.morph_targets から構築（generate_tangents の頂点分割に対応）
fn extract_morphs(pmd: &PmdModel, meshes: &[IrMesh]) -> Vec<IrMorph> {
    // 各メッシュのグローバル頂点オフセット
    let mesh_global_offsets: Vec<usize> = {
        let mut offsets = Vec::with_capacity(meshes.len());
        let mut cum = 0usize;
        for m in meshes {
            offsets.push(cum);
            cum += m.vertices.len();
        }
        offsets
    };

    pmd.morphs
        .iter()
        .filter(|m| m.morph_type != 0) // base 以外
        .enumerate()
        .map(|(i, m)| {
            // mesh.morph_targets から構築（generate_tangents の分割頂点を含む）
            let mut entries: Vec<(usize, Vec3)> = Vec::new();
            for (mi, mesh) in meshes.iter().enumerate() {
                let global_offset = mesh_global_offsets[mi];
                if let Some(mt) = mesh.morph_targets.iter().find(|t| t.name == m.name) {
                    for &(local_vi, offset) in &mt.position_offsets {
                        entries.push((global_offset + local_vi as usize, offset));
                    }
                }
            }

            let panel = match m.morph_type {
                1 => 1, // 眉
                2 => 2, // 目
                3 => 3, // 口
                _ => 4, // その他
            };

            let name_en = pmd
                .english_header
                .as_ref()
                .and_then(|eh| eh.morph_names.get(i))
                .cloned()
                .unwrap_or_default();

            IrMorph {
                name: m.name.clone(),
                name_en,
                panel,
                kind: IrMorphKind::Vertex {
                    positions: entries,
                    normals: Vec::new(),
                    tangents: Vec::new(),
                },
            }
        })
        .collect()
}

fn extract_physics(pmd: &PmdModel) -> IrPhysics {
    let rigid_bodies = pmd
        .rigid_bodies
        .iter()
        .map(|r| {
            // bone_index=0xFFFF（関連ボーンなし）の場合はボーン0（センター）に追従
            let bone_index = if (r.bone_index as usize) < pmd.bones.len() {
                Some(r.bone_index as usize)
            } else if !pmd.bones.is_empty() {
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
