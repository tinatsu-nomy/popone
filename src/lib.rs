pub mod error;
pub mod vrm;
pub mod intermediate;
pub mod pmx;
pub mod convert;
pub mod fbx;

#[cfg(feature = "viewer")]
pub mod viewer;

use std::path::Path;
use anyhow::Result;
use serde::Serialize;

#[derive(Serialize, Debug)]
pub struct ConvertStats {
    pub output_path: String,
    pub tex_dir: String,
    pub bones: usize,
    pub vertices: usize,
    pub faces: usize,
    pub materials: usize,
    pub textures: usize,
    pub morphs: usize,
}

pub fn convert_vrm_to_pmx(
    input_path: &Path,
    output_path: &Path,
    no_physics: bool,
) -> Result<ConvertStats> {
    convert_vrm_to_pmx_with_options(input_path, output_path, no_physics, false)
}

pub fn convert_vrm_to_pmx_with_options(
    input_path: &Path,
    output_path: &Path,
    no_physics: bool,
    align_rigid_rotation: bool,
) -> Result<ConvertStats> {
    convert_vrm_to_pmx_full(input_path, output_path, no_physics, align_rigid_rotation, false)
}

pub fn convert_vrm_to_pmx_full(
    input_path: &Path,
    output_path: &Path,
    no_physics: bool,
    align_rigid_rotation: bool,
    normalize_pose: bool,
) -> Result<ConvertStats> {
    let glb = vrm::loader::load_glb(input_path)?;
    let version = vrm::detect::detect_version(&glb.document);
    let all_extensions = vrm::loader::get_raw_extensions(&glb.document);

    let mut ir = vrm::extract::extract_ir_model_with_options(
        &glb.document,
        &glb.buffers,
        &glb.images,
        &glb.vrm_extension,
        &version,
        &all_extensions,
        normalize_pose,
    )?;

    if no_physics {
        ir.physics = intermediate::types::IrPhysics::default();
    }

    let output_dir = output_path.parent().unwrap_or(Path::new("."));
    let tex_dir = output_dir.join("textures");
    convert::texture::write_all_textures(&ir.textures, &glb.images, &tex_dir)?;

    let pmx_model = pmx::build::build_pmx_model_with_options(&ir, align_rigid_rotation)?;

    let stats = ConvertStats {
        output_path: output_path.to_string_lossy().to_string(),
        tex_dir: tex_dir.to_string_lossy().to_string(),
        bones: pmx_model.bones.len(),
        vertices: pmx_model.vertices.len(),
        faces: pmx_model.faces.len(),
        materials: pmx_model.materials.len(),
        textures: pmx_model.textures.len(),
        morphs: pmx_model.morphs.len(),
    };

    let file = std::fs::File::create(output_path)?;
    let writer = std::io::BufWriter::new(file);
    let header = pmx_model.header.clone();
    let mut pmx_writer = pmx::writer::PmxWriter::new(writer, header);
    pmx_writer.write_model(&pmx_model)?;

    Ok(stats)
}

/// FBX → PMX 変換
pub fn convert_fbx_to_pmx(
    input_path: &Path,
    output_path: &Path,
) -> Result<ConvertStats> {
    let data = std::fs::read(input_path)?;
    let ir = fbx::extract::extract_ir_model_from_fbx(&data, Some(input_path))?;

    let output_dir = output_path.parent().unwrap_or(Path::new("."));
    let tex_dir = output_dir.join("textures");
    convert::texture::write_all_textures_from_ir(&ir.textures, &tex_dir)?;

    let pmx_model = pmx::build::build_pmx_model(&ir)?;

    let stats = ConvertStats {
        output_path: output_path.to_string_lossy().to_string(),
        tex_dir: tex_dir.to_string_lossy().to_string(),
        bones: pmx_model.bones.len(),
        vertices: pmx_model.vertices.len(),
        faces: pmx_model.faces.len(),
        materials: pmx_model.materials.len(),
        textures: pmx_model.textures.len(),
        morphs: pmx_model.morphs.len(),
    };

    let file = std::fs::File::create(output_path)?;
    let writer = std::io::BufWriter::new(file);
    let header = pmx_model.header.clone();
    let mut pmx_writer = pmx::writer::PmxWriter::new(writer, header);
    pmx_writer.write_model(&pmx_model)?;

    Ok(stats)
}

/// 拡張子で VRM/FBX を自動判定して PMX 変換
pub fn convert_to_pmx(
    input_path: &Path,
    output_path: &Path,
    no_physics: bool,
) -> Result<ConvertStats> {
    let ext = input_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());
    match ext.as_deref() {
        Some("fbx") => convert_fbx_to_pmx(input_path, output_path),
        _ => convert_vrm_to_pmx(input_path, output_path, no_physics),
    }
}
