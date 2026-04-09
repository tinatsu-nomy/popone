use crate::error::{PoponeError, Result};
use crate::intermediate::types::{
    AStanceResult, IrBone, IrMaterial, IrMesh, IrModel, IrPhysics, IrTexture, IrVertex,
    SourceFormat, TextureData,
};
use glam::{Mat4, Vec2, Vec3, Vec4};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::parser;

/// DirectX .x ファイルを読み込んで IrModel に変換する
pub fn load_x(path: &Path) -> Result<IrModel> {
    let model = parser::read_x(path)?;
    let base_dir = path.parent().unwrap_or(Path::new("."));
    x_to_ir(&model, base_dir, None)
}

/// DirectX .x データをメモリから読み込んで IrModel に変換する
pub fn load_x_from_data(
    data: &[u8],
    name: &str,
    base_dir: &Path,
    aux: Option<&HashMap<PathBuf, Arc<[u8]>>>,
) -> Result<IrModel> {
    let model = parser::read_x_from_data(data, name)?;
    x_to_ir(&model, base_dir, aux)
}

/// 相対パスを正規化（バックスラッシュ→スラッシュ、"./" 除去、".." 解決）
fn normalize_rel_path(rel: &Path) -> PathBuf {
    let s = rel.to_string_lossy().replace('\\', "/");
    let mut out = Vec::new();
    for component in s.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    PathBuf::from(out.join("/"))
}

/// テクスチャファイルを解決して読み込む
fn resolve_texture(
    aux: Option<&HashMap<PathBuf, Arc<[u8]>>>,
    base_dir: &Path,
    tex_path: &str,
) -> Option<Vec<u8>> {
    // バックスラッシュをスラッシュに正規化
    let normalized = tex_path.replace('\\', "/");

    if let Some(aux_map) = aux {
        let rel_raw = PathBuf::from(&normalized); // ".." 保持
        let rel = normalize_rel_path(&rel_raw); // ".." 除去

        // 1. 元パスで完全一致（"../shared/body.png" → "../shared/body.png"）
        if let Some(bytes) = aux_map.get(&rel_raw) {
            return Some(bytes.to_vec());
        }
        // 2. 正規化パスで完全一致（"shared/body.png" → "shared/body.png"）
        if let Some(bytes) = aux_map.get(&rel) {
            return Some(bytes.to_vec());
        }
        // 3. case-insensitive（元パス・正規化パス両方で検索）
        let raw_lower = rel_raw.to_string_lossy().to_lowercase();
        let norm_lower = rel.to_string_lossy().to_lowercase();
        if let Some(bytes) = aux_map.iter().find_map(|(k, v)| {
            let k_lower = k.to_string_lossy().replace('\\', "/").to_lowercase();
            if k_lower == raw_lower || k_lower == norm_lower {
                Some(v.to_vec())
            } else {
                None
            }
        }) {
            return Some(bytes);
        }
        // archive/snapshot 由来: ディスクフォールバックしない（ローカルファイル漏洩防止）
        return None;
    }

    // ディスクから読む（".." を含む相対パスはそのまま OS が解決する）
    let rel_original = PathBuf::from(&normalized);
    let full_path = base_dir.join(&rel_original);
    std::fs::read(&full_path).ok()
}

/// Frame の親チェーンを辿ってワールド変換行列を計算する
fn compute_world_transform(frames: &[parser::XFrame], frame_index: usize) -> glam::Mat4 {
    let mut chain = Vec::new();
    let mut idx = Some(frame_index);
    while let Some(i) = idx {
        if i >= frames.len() {
            break;
        }
        chain.push(i);
        idx = frames[i].parent;
    }
    // ルートから順に積算
    let mut world = glam::Mat4::IDENTITY;
    for &i in chain.iter().rev() {
        world = world * frames[i].transform;
    }
    world
}

fn x_to_ir(
    model: &parser::XModel,
    base_dir: &Path,
    aux: Option<&HashMap<PathBuf, Arc<[u8]>>>,
) -> Result<IrModel> {
    // ダミールートボーン（merge 時のボーンインデックス参照パニック防止）
    let root_bone = IrBone {
        name: "ルート".to_string(),
        name_en: "Root".to_string(),
        original_name: "Root".to_string(),
        vrm_bone_name: None,
        position: Vec3::ZERO,
        global_mat: Mat4::IDENTITY,
        parent: None,
        children: vec![],
        node_index: 0,
        is_physics: false,
        tail_position: None,
        tail_bone_index: None,
        is_ik: false,
        is_ik_bone: false,
        is_translatable: true,
        is_axis_fixed: false,
        is_visible: true,
        grant: None,
    };

    // スキニング情報を含む .x ファイルはエラーとする
    if model.meshes.iter().any(|m| m.has_skin_weights) {
        return Err(PoponeError::DirectXParse(
            "SkinWeights（スキニング情報）を含む .x ファイルは未対応です。\
             静的メッシュ（アクセサリ・ステージ等）のみ対応しています"
                .into(),
        ));
    }

    // DirectX 左手系 Y-Up → glTF 右手系 Y-Up: Z 反転
    // PMX 出力で元座標の 10 倍になるようスケーリング（10 / PMX_SCALE(12.5) = 0.8）
    const DX_SCALE: f32 = 0.8;
    let pos_to_gltf = |v: Vec3| Vec3::new(v.x * DX_SCALE, v.y * DX_SCALE, -v.z * DX_SCALE);
    let norm_to_gltf = |v: Vec3| Vec3::new(v.x, v.y, -v.z);

    // テクスチャ収集（重複排除）
    let mut texture_map: HashMap<String, usize> = HashMap::new();
    let mut ir_textures: Vec<IrTexture> = Vec::new();

    // 全メッシュの材質を先に収集（グローバル材質リスト構築）
    let mut ir_materials: Vec<IrMaterial> = Vec::new();
    // メッシュごとの材質オフセット
    let mut mesh_mat_offsets: Vec<usize> = Vec::new();

    for mesh in &model.meshes {
        let offset = ir_materials.len();
        mesh_mat_offsets.push(offset);

        if let Some(mat_list) = &mesh.materials {
            for mat in &mat_list.materials {
                let tex_index = mat.texture_filename.as_ref().and_then(|tex_name| {
                    if let Some(&idx) = texture_map.get(tex_name) {
                        Some(idx)
                    } else {
                        let data = resolve_texture(aux, base_dir, tex_name)?;
                        let ext_raw = crate::path_ext_lower(Path::new(tex_name));
                        let ext = if ext_raw.is_empty() {
                            "png".to_string()
                        } else {
                            ext_raw
                        };
                        let mime = crate::intermediate::types::mime_for_ext(&ext);
                        let idx = ir_textures.len();
                        // ファイル名のみ保持（"../shared/body.png" → "body.png"）
                        // PMX テクスチャ書き出しでパス逸脱を防止
                        let safe_filename = Path::new(tex_name)
                            .file_name()
                            .and_then(|f| f.to_str())
                            .unwrap_or(tex_name)
                            .to_string();
                        ir_textures.push(IrTexture {
                            filename: safe_filename,
                            data: TextureData::Encoded(Arc::from(data)),
                            mime_type: mime.to_string(),
                            source_path: tex_name.clone(),
                            mip_chain: None,
                        });
                        texture_map.insert(tex_name.clone(), idx);
                        if ext == "dds" {
                            log::info!("DDS texture '{}' detected", tex_name);
                        }
                        Some(idx)
                    }
                });

                ir_materials.push(IrMaterial {
                    name: if mat.name.is_empty() {
                        format!("material_{}", ir_materials.len())
                    } else {
                        mat.name.clone()
                    },
                    diffuse: Vec4::new(
                        mat.diffuse[0],
                        mat.diffuse[1],
                        mat.diffuse[2],
                        mat.diffuse[3],
                    ),
                    specular: Vec3::new(mat.specular[0], mat.specular[1], mat.specular[2]),
                    specular_power: mat.specular_power,
                    ambient: Vec3::new(
                        mat.diffuse[0] * 0.5,
                        mat.diffuse[1] * 0.5,
                        mat.diffuse[2] * 0.5,
                    ),
                    texture_index: tex_index,
                    source_format: SourceFormat::DirectX,
                    ..Default::default()
                });
            }
        }
    }

    // 材質なしメッシュ用のデフォルト材質インデックス（遅延初期化）
    let mut default_mat_idx: Option<usize> = None;

    // メッシュ変換
    let mut ir_meshes: Vec<IrMesh> = Vec::new();

    for (mi, mesh) in model.meshes.iter().enumerate() {
        // Frame 階層のワールド変換を計算
        let world_transform = mesh
            .frame_index
            .map(|fi| compute_world_transform(&model.frames, fi))
            .unwrap_or(glam::Mat4::IDENTITY);
        let has_frame_transform = world_transform != glam::Mat4::IDENTITY;
        // 法線用の変換行列（逆転置、スケール除去）
        let normal_transform = if has_frame_transform {
            world_transform.inverse().transpose()
        } else {
            glam::Mat4::IDENTITY
        };

        let mat_offset = mesh_mat_offsets.get(mi).copied().unwrap_or(0);
        let has_normals = mesh.normals.is_some();
        log::debug!(
            "DirectX mesh[{}]: positions={}, indices={}, normals={}, texcoords={:?}",
            mi,
            mesh.positions.len(),
            mesh.indices.len(),
            has_normals,
            mesh.texcoords.as_ref().map(|tc| tc.len()),
        );
        let has_texcoords = mesh.texcoords.is_some();

        // 材質ごとにメッシュを分割
        let mat_count = mesh
            .materials
            .as_ref()
            .map(|m| m.materials.len())
            .unwrap_or(0);

        if mat_count <= 1 {
            // 単一材質: 面頂点単位で頂点を生成（ハードエッジ対応）
            let mat_idx = if mat_count == 1 {
                mat_offset
            } else {
                // 材質なしメッシュ用のデフォルト材質（初回のみ追加、以降は共用）
                *default_mat_idx.get_or_insert_with(|| {
                    let idx = ir_materials.len();
                    ir_materials.push(IrMaterial {
                        name: "default".to_string(),
                        source_format: SourceFormat::DirectX,
                        ..Default::default()
                    });
                    idx
                })
            };

            // (position_index, normal_index) → 新頂点インデックス の重複排除マップ
            let mut vert_map: HashMap<(u32, u32), u32> = HashMap::new();
            let mut vertices: Vec<IrVertex> = Vec::new();
            let mut new_indices: Vec<u32> = Vec::new();

            let normals_data = mesh.normals.as_ref();

            for (tri_idx, tri) in mesh.indices.chunks_exact(3).enumerate() {
                for k in 0..3 {
                    let pos_idx = tri[k];
                    // 法線インデックスの取得
                    let norm_idx = if let Some(nd) = normals_data {
                        let fn_idx = tri_idx * 3 + k;
                        if fn_idx < nd.face_normals.len() {
                            nd.face_normals[fn_idx]
                        } else {
                            0
                        }
                    } else {
                        // 法線なしの場合は position_index をキーに使う
                        pos_idx
                    };

                    let key = (pos_idx, norm_idx);
                    let new_vi = if let Some(&existing) = vert_map.get(&key) {
                        existing
                    } else {
                        let vi = pos_idx as usize;
                        let mut pos = if vi < mesh.positions.len() {
                            mesh.positions[vi]
                        } else {
                            Vec3::ZERO
                        };
                        // Frame 変換を適用（pos_to_gltf の前）
                        if has_frame_transform {
                            pos = world_transform.transform_point3(pos);
                        }

                        let normal = if let Some(nd) = normals_data {
                            let ni = norm_idx as usize;
                            if ni < nd.normals.len() {
                                let mut n = nd.normals[ni];
                                if has_frame_transform {
                                    n = normal_transform.transform_vector3(n).normalize_or_zero();
                                }
                                norm_to_gltf(n)
                            } else {
                                Vec3::ZERO
                            }
                        } else {
                            Vec3::ZERO
                        };

                        let uv = if has_texcoords {
                            let tc = mesh.texcoords.as_ref().expect("has_texcoords チェック済み");
                            if vi < tc.len() {
                                // DirectX .x の UV は D3D 慣習で左上原点（PMX/FBX と同じ）。
                                // Y 反転は不要。
                                Vec2::new(tc[vi].x, tc[vi].y)
                            } else {
                                Vec2::ZERO
                            }
                        } else {
                            Vec2::ZERO
                        };

                        let idx = vertices.len() as u32;
                        vertices.push(IrVertex {
                            position: pos_to_gltf(pos),
                            normal,
                            uv,
                            tangent: Vec4::new(1.0, 0.0, 0.0, 1.0),
                            weights: [(0, 1.0), (0, 0.0), (0, 0.0), (0, 0.0)],
                            weight_count: 1,
                            edge_scale: 1.0,
                        });
                        vert_map.insert(key, idx);
                        idx
                    };
                    new_indices.push(new_vi);
                }
            }

            // Z反転(det=-1) × ワールド変換の行列式 で最終的な巻き順を決定
            let need_swap = world_transform.determinant() >= 0.0;
            let final_indices = if need_swap {
                // Z 反転で行列式 -1 → 面の巻き順を反転 (b↔c swap)
                let mut swapped = Vec::with_capacity(new_indices.len());
                for tri in new_indices.chunks_exact(3) {
                    swapped.push(tri[0]);
                    swapped.push(tri[2]);
                    swapped.push(tri[1]);
                }
                swapped
            } else {
                // ワールド変換の負スケールで行列式が反転 → swap 不要
                new_indices
            };

            // 法線が欠落している場合、最終インデックスで面法線を再計算
            if !has_normals {
                compute_face_normals(&mut vertices, &final_indices);
            }

            ir_meshes.push(IrMesh {
                name: if mesh.name.is_empty() {
                    format!("mesh_{}", mi)
                } else {
                    mesh.name.clone()
                },
                vertices: vertices.into(),
                indices: final_indices.into(),
                material_index: mat_idx,
                morph_targets: Arc::new(Vec::new()),
                node_index: 0,
                uvs1: vec![],
            });
        } else {
            // 複数材質: 三角形を材質ごとにグループ分け
            let mat_list = mesh
                .materials
                .as_ref()
                .expect("mat_count > 1 のため materials は Some");

            for local_mat_idx in 0..mat_count {
                let global_mat_idx = mat_offset + local_mat_idx;

                // この材質に属する三角形を収集
                let mut tri_indices: Vec<usize> = Vec::new();
                for (ti, &face_mat) in mat_list.face_material_indices.iter().enumerate() {
                    if face_mat == local_mat_idx {
                        tri_indices.push(ti);
                    }
                }
                if tri_indices.is_empty() {
                    continue;
                }

                // 頂点の再インデックス（(position_index, normal_index) をキーに重複排除）
                let mut vert_map: HashMap<(u32, u32), u32> = HashMap::new();
                let mut vertices: Vec<IrVertex> = Vec::new();
                let mut indices: Vec<u32> = Vec::new();

                for &ti in &tri_indices {
                    let base = ti * 3;
                    if base + 2 >= mesh.indices.len() {
                        continue;
                    }
                    for k in 0..3 {
                        let orig_vi = mesh.indices[base + k];
                        // 法線インデックスの取得
                        let norm_idx = if let Some(nd) = &mesh.normals {
                            let fn_idx = base + k;
                            if fn_idx < nd.face_normals.len() {
                                nd.face_normals[fn_idx]
                            } else {
                                0
                            }
                        } else {
                            orig_vi
                        };

                        let key = (orig_vi, norm_idx);
                        let new_vi = if let Some(&existing) = vert_map.get(&key) {
                            existing
                        } else {
                            let vi = orig_vi as usize;
                            let mut pos = if vi < mesh.positions.len() {
                                mesh.positions[vi]
                            } else {
                                Vec3::ZERO
                            };
                            // Frame 変換を適用（pos_to_gltf の前）
                            if has_frame_transform {
                                pos = world_transform.transform_point3(pos);
                            }

                            let uv = if has_texcoords {
                                let tc =
                                    mesh.texcoords.as_ref().expect("has_texcoords チェック済み");
                                if vi < tc.len() {
                                    // DirectX .x の UV は D3D 慣習で左上原点（PMX/FBX と同じ）。
                                    // Y 反転は不要。
                                    Vec2::new(tc[vi].x, tc[vi].y)
                                } else {
                                    Vec2::ZERO
                                }
                            } else {
                                Vec2::ZERO
                            };

                            let mut normal = Vec3::ZERO;
                            if let Some(normals_data) = &mesh.normals {
                                let ni = norm_idx as usize;
                                if ni < normals_data.normals.len() {
                                    normal = normals_data.normals[ni];
                                    if has_frame_transform {
                                        normal = normal_transform
                                            .transform_vector3(normal)
                                            .normalize_or_zero();
                                    }
                                    normal = norm_to_gltf(normal);
                                }
                            }

                            let new_idx = vertices.len() as u32;
                            vertices.push(IrVertex {
                                position: pos_to_gltf(pos),
                                normal,
                                uv,
                                tangent: Vec4::new(1.0, 0.0, 0.0, 1.0),
                                weights: [(0, 1.0), (0, 0.0), (0, 0.0), (0, 0.0)],
                                weight_count: 1,
                                edge_scale: 1.0,
                            });
                            vert_map.insert(key, new_idx);
                            new_idx
                        };
                        indices.push(new_vi);
                    }
                }

                // Z反転(det=-1) × ワールド変換の行列式 で最終的な巻き順を決定
                let need_swap = world_transform.determinant() >= 0.0;
                let final_indices = if need_swap {
                    // Z 反転で行列式 -1 → 面の巻き順を反転 (b↔c swap)
                    let mut swapped = Vec::with_capacity(indices.len());
                    for tri in indices.chunks_exact(3) {
                        swapped.push(tri[0]);
                        swapped.push(tri[2]);
                        swapped.push(tri[1]);
                    }
                    swapped
                } else {
                    // ワールド変換の負スケールで行列式が反転 → swap 不要
                    indices
                };

                // 最終インデックスで面法線を再計算
                if !has_normals {
                    compute_face_normals(&mut vertices, &final_indices);
                }

                ir_meshes.push(IrMesh {
                    name: format!(
                        "{}_mat{}",
                        if mesh.name.is_empty() {
                            format!("mesh_{}", mi)
                        } else {
                            mesh.name.clone()
                        },
                        local_mat_idx
                    ),
                    vertices: vertices.into(),
                    indices: final_indices.into(),
                    material_index: global_mat_idx,
                    morph_targets: Arc::new(Vec::new()),
                    node_index: 0,
                    uvs1: vec![],
                });
            }
        }
    }

    if ir_meshes.is_empty() {
        return Err(PoponeError::DirectXParse(
            "変換可能なメッシュがありません".into(),
        ));
    }

    Ok(IrModel {
        name: model.name.clone(),
        comment: String::new(),
        bones: vec![root_bone],
        meshes: ir_meshes,
        materials: ir_materials,
        textures: ir_textures,
        morphs: vec![],
        physics: IrPhysics::default(),
        node_to_bone: HashMap::new(),
        source_format: SourceFormat::DirectX,
        rig_type: None,
        humanoid_bone_count: 0,
        astance_result: AStanceResult::NotRequested,
    })
}

/// 面法線を計算してスムーズシェーディング用に頂点法線を累積平均する
fn compute_face_normals(vertices: &mut [IrVertex], indices: &[u32]) {
    for tri in indices.chunks_exact(3) {
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        if i0 >= vertices.len() || i1 >= vertices.len() || i2 >= vertices.len() {
            continue;
        }
        let p0 = vertices[i0].position;
        let p1 = vertices[i1].position;
        let p2 = vertices[i2].position;
        let face_normal = (p1 - p0).cross(p2 - p0);
        vertices[i0].normal += face_normal;
        vertices[i1].normal += face_normal;
        vertices[i2].normal += face_normal;
    }
    for v in vertices.iter_mut() {
        let n = v.normal.normalize_or_zero();
        v.normal = if n == Vec3::ZERO { Vec3::Y } else { n };
    }
}
