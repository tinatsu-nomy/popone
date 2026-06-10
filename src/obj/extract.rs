use crate::error::{PoponeError, Result};
use crate::intermediate::types::{
    AStanceResult, IrBone, IrMaterial, IrMesh, IrModel, IrPhysics, IrTexture, IrVertex,
    SourceFormat, TextureData,
};
use glam::{Mat4, Vec2, Vec3, Vec4};
use rust_i18n::t;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Load an OBJ file from a path and convert it into an `IrModel` (default: cm, Y-Up).
pub fn load_obj(path: &Path) -> Result<IrModel> {
    load_obj_with_params(path, 0.01, false)
}

/// Load an OBJ file from a path and convert it into an `IrModel` (custom parameters).
/// - `scale`: scale factor into glTF space (meters).
/// - `z_up`: when true, apply a Z-Up -> Y-Up conversion.
pub fn load_obj_with_params(path: &Path, scale: f32, z_up: bool) -> Result<IrModel> {
    // Route disk loads through the in-memory path so the shared `mtl_loader`
    // captures each `.mtl`'s directory (needed to resolve textures that live
    // next to a `.mtl` in a subdirectory rather than next to the `.obj`).
    let data = std::fs::read(path)?;
    let base_dir = path.parent().unwrap_or(Path::new("."));
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("OBJ Model")
        .to_string();

    load_obj_from_data_with_params(&data, &name, base_dir, None, scale, z_up)
}

/// Load OBJ data from memory and convert it into an `IrModel`.
/// (Default: cm, Y-Up.)
pub fn load_obj_from_data(
    data: &[u8],
    name: &str,
    base_dir: &Path,
    aux: Option<&HashMap<PathBuf, Arc<[u8]>>>,
) -> Result<IrModel> {
    load_obj_from_data_with_params(data, name, base_dir, aux, 0.01, false)
}

/// Load OBJ data from memory and convert it into an `IrModel` (custom parameters).
pub fn load_obj_from_data_with_params(
    data: &[u8],
    name: &str,
    base_dir: &Path,
    aux: Option<&HashMap<PathBuf, Arc<[u8]>>>,
    scale: f32,
    z_up: bool,
) -> Result<IrModel> {
    let mut reader = std::io::BufReader::new(std::io::Cursor::new(data));

    // Directories of every `.mtl` referenced via `mtllib`. Textures named inside
    // a `.mtl` are relative to that `.mtl`, so these become preferred bases for
    // texture lookup in `build_ir_model`. `RefCell` because tobj requires `Fn`.
    let mtl_dirs: RefCell<Vec<PathBuf>> = RefCell::new(Vec::new());

    // MTL loader: try aux_files first, fall back to disk reads from base_dir
    let mtl_loader = |mtl_path: &Path| -> tobj::MTLLoadResult {
        if let Some(parent) = mtl_path.parent() {
            let norm = normalize_rel_path(parent);
            if !norm.as_os_str().is_empty() {
                let mut dirs = mtl_dirs.borrow_mut();
                if !dirs.contains(&norm) {
                    dirs.push(norm);
                }
            }
        }
        let mtl_data = resolve_sidecar(aux, base_dir, mtl_path);
        match mtl_data {
            Some(bytes) => {
                let mut mtl_reader = std::io::BufReader::new(std::io::Cursor::new(bytes));
                tobj::load_mtl_buf(&mut mtl_reader)
            }
            None => {
                log::warn!("MTL file not found: {:?}", mtl_path);
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
            log::warn!("MTL load failed (continuing with default material): {}", e);
            Vec::new()
        }
    };

    let mtl_dirs = mtl_dirs.into_inner();
    build_ir_model(
        name, &models, &materials, base_dir, &mtl_dirs, aux, scale, z_up,
    )
}

/// Resolve a sidecar file from `aux_files` or from disk.
/// When `aux` is Some (archive/snapshot origin), we do not fall back to disk.
fn resolve_sidecar(
    aux: Option<&HashMap<PathBuf, Arc<[u8]>>>,
    base_dir: &Path,
    rel: &Path,
) -> Option<Vec<u8>> {
    if let Some(aux_map) = aux {
        let rel_raw = PathBuf::from(rel.to_string_lossy().replace('\\', "/")); // Keep ".."
        let normalized = normalize_rel_path(rel); // Strip ".."

        // 1. Exact match against the raw path ("../shared/body.png" -> "../shared/body.png")
        if let Some(bytes) = aux_map.get(&rel_raw) {
            return Some(bytes.to_vec());
        }
        // 2. Exact match against the normalized path ("shared/body.png" -> "shared/body.png")
        if let Some(bytes) = aux_map.get(&normalized) {
            return Some(bytes.to_vec());
        }
        // 3. Case-insensitive match (search both raw and normalized paths)
        let raw_lower = rel_raw.to_string_lossy().to_lowercase();
        let norm_lower = normalized.to_string_lossy().to_lowercase();
        if let Some(bytes) = aux_map.iter().find_map(|(k, v)| {
            let k_lower = k.to_string_lossy().replace('\\', "/").to_lowercase();
            if k_lower == raw_lower || k_lower == norm_lower {
                Some(v)
            } else {
                None
            }
        }) {
            return Some(bytes.to_vec());
        }
        // 4. As a last resort, match by filename only (case-insensitive)
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
        // Archive/snapshot origin: do not fall back to disk
        return None;
    }
    // Read from disk -- normalize first to prevent path traversal
    let sanitized = crate::sanitize_rel_path(&rel.to_string_lossy());
    let full_path = base_dir.join(&sanitized);
    std::fs::read(&full_path).ok()
}

/// Resolve a texture referenced from a `.mtl`.
///
/// Texture names written in a `.mtl` are relative to that `.mtl`'s location,
/// not the `.obj`'s. So try each `.mtl` directory as a prefix first, then fall
/// back to the bare name (resolved against `base_dir` = the `.obj` directory)
/// for the common case where the `.mtl` sits next to the `.obj`.
fn resolve_texture(
    aux: Option<&HashMap<PathBuf, Arc<[u8]>>>,
    base_dir: &Path,
    mtl_dirs: &[PathBuf],
    tex_rel: &Path,
) -> Option<Vec<u8>> {
    for dir in mtl_dirs {
        if let Some(bytes) = resolve_sidecar(aux, base_dir, &dir.join(tex_rel)) {
            return Some(bytes);
        }
    }
    resolve_sidecar(aux, base_dir, tex_rel)
}

/// Normalize a relative path (backslash -> slash, drop "./", resolve "..").
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

#[allow(clippy::too_many_arguments)]
fn build_ir_model(
    name: &str,
    models: &[tobj::Model],
    materials: &[tobj::Material],
    base_dir: &Path,
    mtl_dirs: &[PathBuf],
    aux: Option<&HashMap<PathBuf, Arc<[u8]>>>,
    scale: f32,
    z_up: bool,
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

    // Collect textures (deduplicated)
    let mut texture_map: HashMap<String, usize> = HashMap::new();
    let mut ir_textures: Vec<IrTexture> = Vec::new();

    // Convert MTL materials -> IrMaterial
    let mut ir_materials: Vec<IrMaterial> = Vec::new();
    for mat in materials {
        let tex_index = mat.diffuse_texture.as_ref().and_then(|tex_name| {
            if let Some(&idx) = texture_map.get(tex_name) {
                Some(idx)
            } else {
                // Load the texture as a byte buffer. Names inside a `.mtl` are
                // relative to that `.mtl`, so try the `.mtl` directories first
                // before falling back to the `.obj` directory.
                let tex_path = Path::new(tex_name);
                let data = resolve_texture(aux, base_dir, mtl_dirs, tex_path)?;
                let ext_raw = crate::path_ext_lower(tex_path);
                let ext = if ext_raw.is_empty() {
                    "png".to_string()
                } else {
                    ext_raw
                };
                let mime = crate::intermediate::types::mime_for_ext(&ext);
                let idx = ir_textures.len();
                ir_textures.push(IrTexture {
                    filename: tex_name.clone(),
                    data: TextureData::Encoded(Arc::from(data)),
                    mime_type: mime.to_string(),
                    source_path: tex_name.clone(),
                    mip_chain: None,
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

    // Default material when none are defined
    if ir_materials.is_empty() {
        ir_materials.push(IrMaterial {
            name: "default".to_string(),
            source_format: SourceFormat::Obj,
            ..Default::default()
        });
    }

    // Coord conversion: apply the user-specified unit scale and Z-Up flag.
    // The viewer later scales to PMX units via gltf_pos_to_pmx (x12.5).
    let pos_to_gltf = if z_up {
        // Z-Up -> Y-Up: (x, y, z) -> (x * scale, z * scale, y * scale)
        |v: Vec3, s: f32| Vec3::new(v.x * s, v.z * s, v.y * s)
    } else {
        |v: Vec3, s: f32| Vec3::new(v.x * s, v.y * s, v.z * s)
    };

    // Mesh conversion
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
                Vec3::ZERO // Recomputed from face normals later
            };

            let uv = if has_texcoords && i * 2 + 1 < mesh.texcoords.len() {
                Vec2::new(mesh.texcoords[i * 2], 1.0 - mesh.texcoords[i * 2 + 1])
            } else {
                Vec2::ZERO
            };

            vertices.push(IrVertex {
                position: pos_to_gltf(Vec3::new(px, py, pz), scale),
                normal,
                uv,
                tangent: Vec4::new(1.0, 0.0, 0.0, 1.0),
                weights: [(0, 1.0), (0, 0.0), (0, 0.0), (0, 0.0)],
                weight_count: 1,
                edge_scale: 1.0,
            });
        }

        // Recompute normals from face normals when missing (smooth shading)
        if !has_normals {
            compute_face_normals(&mut vertices, &mesh.indices);
        }

        // Material index (tobj uses one material per mesh)
        let mat_idx = mesh.material_id.unwrap_or(0);
        let mat_idx = if mat_idx < ir_materials.len() {
            mat_idx
        } else {
            0
        };

        ir_meshes.push(IrMesh {
            name: model.name.clone(),
            vertices: vertices.into(),
            indices: Arc::new(mesh.indices.clone()),
            material_index: mat_idx,
            morph_targets: Arc::new(Vec::new()),
            node_index: 0,
            uvs1: vec![],
        });
    }

    // No meshes -> empty file
    if ir_meshes.is_empty() {
        return Err(PoponeError::ObjParse(
            t!("error.obj.empty_mesh").to_string(),
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

/// Compute face normals and accumulate them into per-vertex normals for smooth shading.
fn compute_face_normals(vertices: &mut [IrVertex], indices: &[u32]) {
    // Accumulate face normals onto each incident vertex
    for tri in indices.chunks_exact(3) {
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        if i0 >= vertices.len() || i1 >= vertices.len() || i2 >= vertices.len() {
            continue;
        }
        let p0 = vertices[i0].position;
        let p1 = vertices[i1].position;
        let p2 = vertices[i2].position;
        let face_normal = (p1 - p0).cross(p2 - p0);
        // Weight by face area (add without normalizing)
        vertices[i0].normal += face_normal;
        vertices[i1].normal += face_normal;
        vertices[i2].normal += face_normal;
    }
    // Normalize
    for v in vertices.iter_mut() {
        let n = v.normal.normalize_or_zero();
        v.normal = if n == Vec3::ZERO { Vec3::Y } else { n };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Best-effort unique temp directory; removed by `Drop` after the test.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(tag: &str) -> Self {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "popone_obj_test_{}_{}_{}",
                tag,
                std::process::id(),
                n
            ));
            std::fs::create_dir_all(&path).unwrap();
            TempDir { path }
        }

        fn write(&self, rel: &str, bytes: &[u8]) {
            let full = self.path.join(rel);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(full, bytes).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    const TRI_OBJ_BODY: &str =
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 0 1\nusemtl m\nf 1/1 2/2 3/3\n";

    /// Texture named inside a `.mtl` that lives in a subdirectory must be found
    /// relative to the `.mtl`, not relative to the `.obj`. Regression for the
    /// "MTL subdirectory resolution" bug.
    #[test]
    fn texture_resolves_relative_to_mtl_subdirectory() {
        let dir = TempDir::new("mtl_subdir");
        dir.write(
            "model.obj",
            format!("mtllib mtl/model.mtl\n{}", TRI_OBJ_BODY).as_bytes(),
        );
        dir.write("mtl/model.mtl", b"newmtl m\nmap_Kd tex.png\n");
        // The texture sits next to the `.mtl`, NOT next to the `.obj`.
        dir.write("mtl/tex.png", b"FAKE-PNG-BYTES");

        let model = load_obj(&dir.path.join("model.obj")).expect("load_obj should succeed");

        assert_eq!(model.textures.len(), 1, "texture should have been resolved");
        match &model.textures[0].data {
            TextureData::Encoded(bytes) => {
                assert_eq!(&bytes[..], b"FAKE-PNG-BYTES");
            }
            other => panic!("expected encoded texture, got {:?}", other),
        }
    }

    /// The flat layout (`.mtl` next to the `.obj`) must keep working: the
    /// fallback to the `.obj` directory should still resolve the texture.
    #[test]
    fn texture_resolves_in_flat_layout() {
        let dir = TempDir::new("flat");
        dir.write(
            "model.obj",
            format!("mtllib model.mtl\n{}", TRI_OBJ_BODY).as_bytes(),
        );
        dir.write("model.mtl", b"newmtl m\nmap_Kd tex.png\n");
        dir.write("tex.png", b"FLAT-PNG");

        let model = load_obj(&dir.path.join("model.obj")).expect("load_obj should succeed");

        assert_eq!(model.textures.len(), 1);
        match &model.textures[0].data {
            TextureData::Encoded(bytes) => assert_eq!(&bytes[..], b"FLAT-PNG"),
            other => panic!("expected encoded texture, got {:?}", other),
        }
    }
}
