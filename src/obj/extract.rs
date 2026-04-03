use crate::error::{PoponeError, Result};
use crate::intermediate::types::{
    AStanceResult, IrBone, IrMaterial, IrMesh, IrModel, IrPhysics, IrTexture, IrVertex,
    SourceFormat,
};
use glam::{Mat4, Vec2, Vec3, Vec4};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// OBJ ファイルをパスから読み込んで IrModel に変換する
pub fn load_obj(path: &Path) -> Result<IrModel> {
    let (models, materials_result) = tobj::load_obj(path, &tobj::GPU_LOAD_OPTIONS)
        .map_err(|e| PoponeError::ObjParse(format!("{}", e)))?;

    let materials = match materials_result {
        Ok(mats) => mats,
        Err(e) => {
            log::warn!("MTL 読み込み失敗（デフォルト材質で続行）: {}", e);
            Vec::new()
        }
    };

    let base_dir = path.parent().unwrap_or(Path::new("."));
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("OBJ Model")
        .to_string();

    build_ir_model(&name, &models, &materials, base_dir, None)
}

/// OBJ データをメモリから読み込んで IrModel に変換する
pub fn load_obj_from_data(
    data: &[u8],
    name: &str,
    base_dir: &Path,
    aux: Option<&HashMap<PathBuf, Arc<[u8]>>>,
) -> Result<IrModel> {
    let mut reader = std::io::BufReader::new(std::io::Cursor::new(data));

    // MTL ローダー: aux_files から解決、なければ base_dir からディスク読み込み
    let mtl_loader = |mtl_path: &Path| -> tobj::MTLLoadResult {
        let mtl_data = resolve_sidecar(aux, base_dir, mtl_path);
        match mtl_data {
            Some(bytes) => {
                let mut mtl_reader = std::io::BufReader::new(std::io::Cursor::new(bytes));
                tobj::load_mtl_buf(&mut mtl_reader)
            }
            None => {
                log::warn!("MTL ファイルが見つかりません: {:?}", mtl_path);
                Ok((Vec::new(), Default::default()))
            }
        }
    };

    let (models, materials_result) =
        tobj::load_obj_buf(&mut reader, &tobj::GPU_LOAD_OPTIONS, mtl_loader)
            .map_err(|e| PoponeError::ObjParse(format!("{}", e)))?;

    let materials = match materials_result {
        Ok(mats) => mats,
        Err(e) => {
            log::warn!("MTL 読み込み失敗（デフォルト材質で続行）: {}", e);
            Vec::new()
        }
    };

    build_ir_model(name, &models, &materials, base_dir, aux)
}

/// aux_files またはディスクからサイドカーファイルを解決する
/// aux が Some の場合（archive/snapshot 由来）はディスクフォールバックを行わない
fn resolve_sidecar(
    aux: Option<&HashMap<PathBuf, Arc<[u8]>>>,
    base_dir: &Path,
    rel: &Path,
) -> Option<Vec<u8>> {
    if let Some(aux_map) = aux {
        // aux lookup 用: パス正規化（バックスラッシュ→スラッシュ、"./" 除去、".." 解決）
        let normalized = normalize_rel_path(rel);
        // 完全一致
        if let Some(bytes) = aux_map.get(&normalized) {
            return Some(bytes.to_vec());
        }
        // case-insensitive fallback（PMX/PMD の archive 解決と同じ方針）
        let norm_lower = normalized.to_string_lossy().to_lowercase();
        if let Some(bytes) = aux_map.iter().find_map(|(k, v)| {
            if k.to_string_lossy().to_lowercase() == norm_lower {
                Some(v)
            } else {
                None
            }
        }) {
            return Some(bytes.to_vec());
        }
        // ファイル名のみでも探す（case-insensitive）
        if let Some(filename) = normalized.file_name() {
            let fname_lower = filename.to_string_lossy().to_lowercase();
            if let Some(bytes) = aux_map.iter().find_map(|(k, v)| {
                let k_fname = k.file_name().map(|f| f.to_string_lossy().to_lowercase());
                if k_fname.as_deref() == Some(&*fname_lower) {
                    Some(v)
                } else {
                    None
                }
            }) {
                return Some(bytes.to_vec());
            }
        }
        // archive/snapshot 由来: ディスクフォールバックしない
        return None;
    }
    // ディスクから読む（通常ファイル読み込み時のみ）
    // ".." を含む相対パスをそのまま保持（OS が解決する）
    let rel_slash = PathBuf::from(rel.to_string_lossy().replace('\\', "/"));
    let full_path = base_dir.join(&rel_slash);
    std::fs::read(&full_path).ok()
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

fn build_ir_model(
    name: &str,
    models: &[tobj::Model],
    materials: &[tobj::Material],
    base_dir: &Path,
    aux: Option<&HashMap<PathBuf, Arc<[u8]>>>,
) -> Result<IrModel> {
    let root_bone = IrBone {
        name: "全ての親".to_string(),
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

    // テクスチャ収集（重複排除）
    let mut texture_map: HashMap<String, usize> = HashMap::new();
    let mut ir_textures: Vec<IrTexture> = Vec::new();

    // MTL 材質 → IrMaterial 変換
    let mut ir_materials: Vec<IrMaterial> = Vec::new();
    for mat in materials {
        let tex_index = mat.diffuse_texture.as_ref().and_then(|tex_name| {
            if let Some(&idx) = texture_map.get(tex_name) {
                Some(idx)
            } else {
                // テクスチャをバイト列として読み込み
                let tex_path = Path::new(tex_name);
                let data = resolve_sidecar(aux, base_dir, tex_path)?;
                let ext = tex_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("png")
                    .to_lowercase();
                let mime = crate::intermediate::types::mime_for_ext(&ext);
                let idx = ir_textures.len();
                ir_textures.push(IrTexture {
                    filename: tex_name.clone(),
                    data,
                    mime_type: mime.to_string(),
                });
                texture_map.insert(tex_name.clone(), idx);
                Some(idx)
            }
        });

        let diffuse = mat.diffuse.unwrap_or([0.8, 0.8, 0.8]);
        let specular = mat.specular.unwrap_or([0.0, 0.0, 0.0]);
        let shininess = mat.shininess.unwrap_or(0.0);
        let dissolve = mat.dissolve.unwrap_or(1.0);

        ir_materials.push(IrMaterial {
            name: mat.name.clone(),
            diffuse: Vec4::new(diffuse[0], diffuse[1], diffuse[2], dissolve),
            specular: Vec3::new(specular[0], specular[1], specular[2]),
            specular_power: shininess,
            ambient: Vec3::new(diffuse[0] * 0.5, diffuse[1] * 0.5, diffuse[2] * 0.5),
            texture_index: tex_index,
            source_format: SourceFormat::Obj,
            ..Default::default()
        });
    }

    // 材質がない場合のデフォルト
    if ir_materials.is_empty() {
        ir_materials.push(IrMaterial {
            name: "default".to_string(),
            source_format: SourceFormat::Obj,
            ..Default::default()
        });
    }

    // OBJ の座標を cm 単位と仮定し、glTF 空間（メートル）に正規化。
    // FBX と同じ変換: cm → m (÷100)
    // ビューア描画時に gltf_pos_to_pmx (×12.5) で PMX 単位に変換される。
    const CM_TO_M: f32 = 0.01;
    let pos_to_gltf = |v: Vec3| Vec3::new(v.x * CM_TO_M, v.y * CM_TO_M, v.z * CM_TO_M);

    // メッシュ変換
    let mut ir_meshes: Vec<IrMesh> = Vec::new();
    for model in models {
        let mesh = &model.mesh;
        let has_normals = !mesh.normals.is_empty();
        let has_texcoords = !mesh.texcoords.is_empty();
        let vert_count = mesh.positions.len() / 3;

        let mut vertices = Vec::with_capacity(vert_count);
        for i in 0..vert_count {
            let px = mesh.positions[i * 3];
            let py = mesh.positions[i * 3 + 1];
            let pz = mesh.positions[i * 3 + 2];

            let normal = if has_normals && i * 3 + 2 < mesh.normals.len() {
                Vec3::new(
                    mesh.normals[i * 3],
                    mesh.normals[i * 3 + 1],
                    mesh.normals[i * 3 + 2],
                )
            } else {
                Vec3::ZERO // 後で面法線から再計算
            };

            let uv = if has_texcoords && i * 2 + 1 < mesh.texcoords.len() {
                Vec2::new(mesh.texcoords[i * 2], 1.0 - mesh.texcoords[i * 2 + 1])
            } else {
                Vec2::ZERO
            };

            vertices.push(IrVertex {
                position: pos_to_gltf(Vec3::new(px, py, pz)),
                normal,
                uv,
                tangent: Vec4::new(1.0, 0.0, 0.0, 1.0),
                weights: [(0, 1.0), (0, 0.0), (0, 0.0), (0, 0.0)],
                weight_count: 1,
                edge_scale: 1.0,
            });
        }

        // 法線が欠落している場合、面法線から再計算（スムーズシェーディング）
        if !has_normals {
            compute_face_normals(&mut vertices, &mesh.indices);
        }

        // 材質インデックス（tobj ではメッシュごとに1つの材質）
        let mat_idx = mesh.material_id.unwrap_or(0);
        let mat_idx = if mat_idx < ir_materials.len() {
            mat_idx
        } else {
            0
        };

        ir_meshes.push(IrMesh {
            name: model.name.clone(),
            vertices,
            indices: mesh.indices.clone(),
            material_index: mat_idx,
            morph_targets: vec![],
            node_index: 0,
            uvs1: vec![],
        });
    }

    // メッシュが空の場合
    if ir_meshes.is_empty() {
        return Err(PoponeError::ObjParse(
            "メッシュが見つかりません（空の OBJ ファイル）".into(),
        ));
    }

    Ok(IrModel {
        name: name.to_string(),
        comment: String::new(),
        bones: vec![root_bone],
        meshes: ir_meshes,
        materials: ir_materials,
        textures: ir_textures,
        morphs: vec![],
        physics: IrPhysics::default(),
        node_to_bone: HashMap::new(),
        source_format: SourceFormat::Obj,
        rig_type: None,
        humanoid_bone_count: 0,
        astance_result: AStanceResult::NotRequested,
    })
}

/// 面法線を計算してスムーズシェーディング用に頂点法線を累積平均する
fn compute_face_normals(vertices: &mut [IrVertex], indices: &[u32]) {
    // 面法線を各頂点に累積加算
    for tri in indices.chunks_exact(3) {
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        if i0 >= vertices.len() || i1 >= vertices.len() || i2 >= vertices.len() {
            continue;
        }
        let p0 = vertices[i0].position;
        let p1 = vertices[i1].position;
        let p2 = vertices[i2].position;
        let face_normal = (p1 - p0).cross(p2 - p0);
        // 面積に比例した重み（正規化せずに加算）
        vertices[i0].normal += face_normal;
        vertices[i1].normal += face_normal;
        vertices[i2].normal += face_normal;
    }
    // 正規化
    for v in vertices.iter_mut() {
        let n = v.normal.normalize_or_zero();
        v.normal = if n == Vec3::ZERO { Vec3::Y } else { n };
    }
}
