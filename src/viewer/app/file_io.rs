//! File loading, drag-and-drop handling, reload, append, and animation loading.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use eframe::egui;
use rust_i18n::t;

use crate::intermediate::types::{IrModel, TextureData};
use crate::unitypackage::UnityPackageIndex;

use super::MaterialDisplayState;
use crate::vrm;

use super::pending::PendingOverlay;

use super::super::animation::AnimationState;
use super::helpers::{
    build_pkg_model_list, collect_image_files_recursive, is_temp_path, FbxLoadMode, PkgModelType,
    PreloadedData, ReloadableSource, IMAGE_EXTENSIONS, MODEL_EXTENSIONS,
};
use super::pending::{
    PendingArchiveLoad, PendingFbxChoice, PendingPkgModelLoad, PendingUnityPackage,
};
use super::texture_mgmt::PendingTexMatch;
use super::{
    AppendedModel, CachedStats, ConvertMessage, ConvertResult, DisplaySettings, MaterialGroup,
    ViewerApp,
};

/// Whether the FBX file contains mesh and/or animation data.
struct FbxContentInfo {
    has_mesh: bool,
    has_anim: bool,
}

use super::helpers::TextureSource;
use super::OrbitCamera;

/// File format inferred from the file extension.
/// Centralising extension dispatch in one place prevents drift across the three
/// independent branches that used to inspect extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    Vrm,
    Fbx,
    Pmx,
    Pmd,
    Obj,
    Stl,
    DirectX,
    UnityPackage,
    SevenZ,
    Zip,
    /// .vrma, .gltf, .glb (animation), .anim
    Animation,
    Unknown,
}

/// Resolve a `FileFormat` from a lowercased extension string.
pub(super) fn detect_format(ext: &str) -> FileFormat {
    match ext {
        "vrm" | "glb" | "gltf" => FileFormat::Vrm,
        "fbx" => FileFormat::Fbx,
        "pmx" => FileFormat::Pmx,
        "pmd" => FileFormat::Pmd,
        "obj" => FileFormat::Obj,
        "stl" => FileFormat::Stl,
        "x" => FileFormat::DirectX,
        "unitypackage" => FileFormat::UnityPackage,
        "7z" => FileFormat::SevenZ,
        "zip" => FileFormat::Zip,
        "vrma" | "anim" => FileFormat::Animation,
        _ => FileFormat::Unknown,
    }
}

/// Input source for a background load.
/// Future `ArchiveEntry` / `Reload` variants will unify in-archive parsing and
/// reload paths under the same background-load pipeline.
pub(super) enum CpuParseInput {
    /// Ordinary file load (with `preloaded` data when the source is a temp file).
    File {
        path: PathBuf,
        format: FileFormat,
        preloaded: Option<super::helpers::PreloadedData>,
    },
    /// Model nested inside an archive (decompressed and parsed on the BG thread).
    ArchiveModel {
        archive_data: Arc<[u8]>,
        format: crate::archive::ArchiveFormat,
        contents: crate::archive::ArchiveContents,
        model_index: usize,
        source_path: PathBuf,
        is_temp: bool,
        append: bool,
        normalize_pose: bool,
        normalize_to_tstance: bool,
    },
    /// Model inside a UnityPackage (FBX / VRM / Prefab).
    PkgModel {
        assets: Arc<Vec<crate::unitypackage::ExtractedAsset>>,
        model_index: usize,
        model_type: super::helpers::PkgModelType,
        source_path: PathBuf,
        pkg_index: Option<Arc<crate::unitypackage::UnityPackageIndex>>,
        source_override: Option<super::helpers::ReloadableSource>,
        normalize_pose: bool,
        normalize_to_tstance: bool,
        append: bool,
        suppress_tex_match: bool,
        batch_progress: Option<(usize, usize)>,
        /// Whether a model is already loaded (for FBX anim check)
        has_loaded_model: bool,
    },
    /// UnityPackage file entry point (reads the file and builds the index on the BG thread).
    UnityPackageIndex {
        path: PathBuf,
        preloaded: Option<super::helpers::PreloadedData>,
        append: bool,
    },
    /// Archive file entry point (reads the file and lists its contained models on the BG thread).
    ArchiveIndex {
        path: PathBuf,
        preloaded: Option<super::helpers::PreloadedData>,
        append: bool,
    },
}

/// Pre-decode textures on the BG thread (Encoded → RawRgba).
/// The main thread's `upload_textures_from_ir` then just uploads RawRgba straight
/// to the GPU, eliminating the UI freeze caused by image decoding on the UI thread.
/// On cancellation we bail out early; any leftover textures stay as Encoded and
/// fall back to main-thread decoding.
fn pre_decode_textures(ir: &mut IrModel, cancel: &std::sync::Arc<std::sync::atomic::AtomicBool>) {
    use crate::intermediate::types::TextureData;

    for tex in &mut ir.textures {
        // Check cancellation per texture.
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            log::info!(
                "Pre-decode cancelled, {} textures remaining as Encoded",
                ir.textures
                    .iter()
                    .filter(|t| matches!(t.data, TextureData::Encoded(_)))
                    .count()
            );
            break;
        }
        if let TextureData::Encoded(ref data) = tex.data {
            if data.is_empty() {
                continue;
            }
            let is_psd = crate::psd::is_psd_filename(&tex.filename);
            match super::super::texture::decode_image_to_rgba_with_hint(
                data,
                is_psd,
                Some(&tex.mime_type),
            ) {
                Ok((pixels, width, height)) => {
                    tex.data = TextureData::RawRgba {
                        pixels: pixels.into(),
                        width,
                        height,
                    };
                    // PSD must be exported as PNG once decoded (the image crate has no PSD encoder).
                    // Updating the filename and MIME to PNG here prevents a downstream bug where
                    // write_all_textures_from_ir_opt_cancel inspects only the extension and would
                    // mistakenly call psd_to_png on the now-raw-RGBA payload.
                    if is_psd {
                        let stem = std::path::Path::new(&tex.filename)
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned();
                        let old_name = std::mem::replace(&mut tex.filename, format!("{stem}.png"));
                        tex.mime_type = "image/png".to_string();
                        log::info!(
                            "Pre-decode PSD->PNG: '{}' -> '{}' ({}x{})",
                            old_name,
                            tex.filename,
                            width,
                            height
                        );
                    }
                }
                Err(e) => {
                    log::warn!(
                        "Pre-decode failed for '{}': {} ({} bytes)",
                        tex.filename,
                        e,
                        data.len()
                    );
                    // On decode failure leave the texture as Encoded (main thread will retry).
                }
            }
        }
    }
}

/// Build an IrModel from an archive bundle (free function used from the BG thread).
fn build_ir_from_archive_bundle_bg(
    bundle: &crate::archive::ModelBundle,
    source_path: &Path,
    normalize_pose: bool,
    normalize_to_tstance: bool,
) -> anyhow::Result<IrModel> {
    use crate::archive::ArchiveModelKind;
    match bundle.kind {
        ArchiveModelKind::Pmx => {
            let pmx_model = crate::pmx::reader::read_pmx_from_data(&bundle.model.data)?;
            let mut ir = crate::pmx::extract::pmx_to_ir_with_aux(
                &pmx_model,
                std::path::Path::new("."),
                Some(&bundle.aux_files),
            )?;
            if normalize_pose {
                ir.astance_result = crate::intermediate::pose::normalize_pose_to_tstance_full(
                    &mut ir.bones,
                    &mut ir.meshes,
                    &mut ir.morphs,
                    &mut ir.physics,
                    crate::convert::coord::gltf_pos_to_pmx,
                );
            }
            Ok(ir)
        }
        ArchiveModelKind::Pmd => {
            let pmd_model = crate::pmd::reader::read_pmd_from_data(&bundle.model.data)?;
            let mut ir = crate::pmd::extract::pmd_to_ir_with_aux(
                &pmd_model,
                &bundle.model.path,
                Some(&bundle.aux_files),
            )?;
            if normalize_pose {
                ir.astance_result = crate::intermediate::pose::normalize_pose_to_tstance_full(
                    &mut ir.bones,
                    &mut ir.meshes,
                    &mut ir.morphs,
                    &mut ir.physics,
                    crate::convert::coord::gltf_pos_to_pmx,
                );
            }
            Ok(ir)
        }
        ArchiveModelKind::Fbx => {
            let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                &bundle.model.data,
                Some(source_path),
                normalize_pose,
                normalize_to_tstance,
            )?;
            let label = format!(
                "archive({})",
                source_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            );
            crate::unitypackage::embed_textures_into_ir_with_label(
                &mut ir,
                &bundle.textures,
                &label,
            );
            Ok(ir)
        }
        ArchiveModelKind::Vrm | ArchiveModelKind::Glb => {
            let glb = vrm::loader::load_glb_from_data(&bundle.model.data)?;
            let version = vrm::detect::detect_version(&glb.document);
            let all_extensions = vrm::loader::get_raw_extensions(&glb.document);
            let ir = vrm::extract::extract_ir_model_with_options(
                &glb.document,
                &glb.buffers,
                &glb.images,
                &glb.vrm_extension,
                &version,
                &all_extensions,
                normalize_pose,
            )?;
            Ok(ir)
        }
        ArchiveModelKind::Obj => {
            let base_dir = bundle
                .model
                .path
                .parent()
                .unwrap_or(std::path::Path::new("."));
            let name = bundle
                .model
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Model");
            let ir = crate::obj::extract::load_obj_from_data(
                &bundle.model.data,
                name,
                base_dir,
                Some(&bundle.aux_files),
            )?;
            Ok(ir)
        }
        ArchiveModelKind::Stl => {
            let name = bundle
                .model
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Model");
            let ir = crate::stl::extract::load_stl_from_data(&bundle.model.data, name)?;
            Ok(ir)
        }
        ArchiveModelKind::DirectX => {
            let base_dir = bundle
                .model
                .path
                .parent()
                .unwrap_or(std::path::Path::new("."));
            let name = bundle
                .model
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Model");
            let ir = crate::directx::extract::load_x_from_data(
                &bundle.model.data,
                name,
                base_dir,
                Some(&bundle.aux_files),
            )?;
            Ok(ir)
        }
        ArchiveModelKind::UnityPackage => {
            // load_model_from_archive / cpu_parse_source already branched out earlier; unreachable here.
            anyhow::bail!("UnityPackage は build_ir_from_archive_bundle_bg では処理できません")
        }
    }
}

/// CPU-side parse executed on a background thread (free function — no `&self` required).
/// Reads the file and parses it, returning `(IrModel, ReloadableSource, Option<BgLoadKind>)`.
/// When the third element is `Some`, it overrides the spawning side's `kind` (used by archives).
/// Does no GPU resource construction. Textures are pre-decoded into RawRgba so the main
/// thread's GPU upload pays no decode cost.
pub(super) fn cpu_parse_source(
    input: CpuParseInput,
    normalize_pose: bool,
    normalize_to_tstance: bool,
    cancel: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> anyhow::Result<(
    IrModel,
    super::helpers::ReloadableSource,
    Option<super::pending::BgLoadKind>,
)> {
    let result = cpu_parse_source_inner(input, normalize_pose, normalize_to_tstance, cancel)?;
    let (mut ir, source, kind) = result;
    // BgLoadKind variants that return a dummy IrModel don't need texture pre-decoding.
    if !matches!(
        kind,
        Some(super::pending::BgLoadKind::ArchivePreparedUnityPackage { .. })
            | Some(super::pending::BgLoadKind::UnityPackageIndexed { .. })
            | Some(super::pending::BgLoadKind::ArchiveIndexed { .. })
    ) {
        pre_decode_textures(&mut ir, cancel);
    }
    Ok((ir, source, kind))
}

/// Body of `cpu_parse_source`.
fn cpu_parse_source_inner(
    input: CpuParseInput,
    normalize_pose: bool,
    normalize_to_tstance: bool,
    cancel: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> anyhow::Result<(
    IrModel,
    super::helpers::ReloadableSource,
    Option<super::pending::BgLoadKind>,
)> {
    use super::helpers::ReloadableSource;

    // Closure for cancellation checks.
    let check_cancel = |cancel: &Arc<std::sync::atomic::AtomicBool>| -> anyhow::Result<()> {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            anyhow::bail!("bg load cancelled");
        }
        Ok(())
    };

    match input {
        CpuParseInput::ArchiveModel {
            archive_data,
            format,
            contents,
            model_index,
            source_path,
            is_temp,
            append,
            normalize_pose: np,
            normalize_to_tstance: nt,
        } => {
            check_cancel(cancel)?;
            let model_path = contents.models[model_index].1.clone();
            let kind = contents.models[model_index].3;

            let bundle =
                crate::archive::extract_model_bundle(&archive_data, format, contents, model_index)?;
            check_cancel(cancel)?;

            // UnityPackage: build pkg_index on the BG thread and hand it to the main thread.
            if kind == crate::archive::ArchiveModelKind::UnityPackage {
                let pkg_index = Arc::new(crate::unitypackage::build_unity_package_index(
                    &bundle.model.data,
                )?);
                let assets: Vec<crate::unitypackage::ExtractedAsset> = pkg_index
                    .entries
                    .iter()
                    .map(|e| crate::unitypackage::ExtractedAsset {
                        pathname: e.pathname.clone(),
                        data: Arc::clone(&e.data),
                    })
                    .collect();
                let model_list = super::helpers::build_pkg_model_list(&assets);

                let bg_kind = super::pending::BgLoadKind::ArchivePreparedUnityPackage {
                    pkg_data: bundle.model.data,
                    pkg_index,
                    assets,
                    model_list,
                    source_path: source_path.clone(),
                    archive_data,
                    is_temp,
                    append,
                    entry_path: model_path,
                };
                // dummy ir/source — apply handler will use kind fields instead
                let ir = IrModel::default();
                let source = ReloadableSource::File(source_path);
                return Ok((ir, source, Some(bg_kind)));
            }

            let ir = build_ir_from_archive_bundle_bg(&bundle, &source_path, np, nt)?;
            check_cancel(cancel)?;

            let source = ReloadableSource::Archive {
                original_path: source_path.clone(),
                archive_bytes: if is_temp {
                    Some(Arc::clone(&archive_data))
                } else {
                    None
                },
                selected_entry_path: model_path.to_string_lossy().into_owned(),
                inner_kind: kind,
            };

            let bg_kind = if append {
                super::pending::BgLoadKind::ArchiveAppend
            } else {
                super::pending::BgLoadKind::ArchiveInitial
            };
            Ok((ir, source, Some(bg_kind)))
        }
        CpuParseInput::File {
            ref path,
            format,
            ref preloaded,
        } => {
            check_cancel(cancel)?;

            let is_temp =
                is_temp_path(path) || preloaded.as_ref().is_some_and(|pl| pl.path == *path);

            // Prefer bytes from `preloaded` when available; otherwise read from disk.
            let read_data = |p: &Path| -> anyhow::Result<Arc<[u8]>> {
                if let Some(ref pl) = preloaded {
                    if pl.path == *p {
                        return Ok(Arc::clone(&pl.main_bytes));
                    }
                    if let Some(data) = pl.aux_files.get(p) {
                        return Ok(Arc::clone(data));
                    }
                }
                Ok(std::fs::read(p)?.into())
            };
            let collect_aux = |p: &Path| -> HashMap<PathBuf, Arc<[u8]>> {
                if let Some(ref pl) = preloaded {
                    if pl.path == *p {
                        return pl.aux_files.clone();
                    }
                }
                let mut aux = HashMap::new();
                if let Some(dir) = p.parent() {
                    collect_image_files_recursive(dir, dir, &mut aux);
                }
                aux
            };

            let make_source =
                |data: Arc<[u8]>, aux: HashMap<PathBuf, Arc<[u8]>>| -> ReloadableSource {
                    if is_temp {
                        ReloadableSource::Snapshot {
                            original_path: path.to_path_buf(),
                            main_bytes: data,
                            aux_files: aux,
                        }
                    } else {
                        ReloadableSource::File(path.to_path_buf())
                    }
                };

            check_cancel(cancel)?;
            match format {
                FileFormat::Vrm => {
                    let glb = if is_temp {
                        let data = read_data(path)?;
                        crate::vrm::loader::load_glb_from_data(&data)?
                    } else {
                        crate::vrm::loader::load_glb(path)?
                    };
                    check_cancel(cancel)?;
                    let version = crate::vrm::detect::detect_version(&glb.document);
                    let all_extensions = crate::vrm::loader::get_raw_extensions(&glb.document);
                    let ir = crate::vrm::extract::extract_ir_model_with_options(
                        &glb.document,
                        &glb.buffers,
                        &glb.images,
                        &glb.vrm_extension,
                        &version,
                        &all_extensions,
                        normalize_pose,
                    )?;
                    check_cancel(cancel)?;
                    // Keep textures as raw RGBA inside IrTexture (already set by vrm::extract).
                    // PNG encoding is deferred until it is actually needed after upload (e.g. PMX export).
                    let source = if is_temp {
                        let data = read_data(path)?;
                        ReloadableSource::Snapshot {
                            original_path: path.to_path_buf(),
                            main_bytes: data,
                            aux_files: HashMap::new(),
                        }
                    } else {
                        ReloadableSource::File(path.to_path_buf())
                    };
                    Ok((ir, source, None))
                }
                FileFormat::Fbx => {
                    let data = read_data(path)?;
                    check_cancel(cancel)?;
                    let ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                        &data,
                        Some(path),
                        normalize_pose,
                        normalize_to_tstance,
                    )?;
                    check_cancel(cancel)?;
                    let aux = collect_aux(path);
                    let source = make_source(data, aux);
                    Ok((ir, source, None))
                }
                FileFormat::Pmx => {
                    // Non-temp: read from disk directly (pmx_to_ir handles every aux extension, sph/spa etc.).
                    // Temp: use preloaded.aux_files via pmx_to_ir_with_aux.
                    let pmx_dir = path.parent().unwrap_or(Path::new("."));
                    // Clone aux_files once and reuse it for both model load and source construction.
                    let aux = if is_temp {
                        preloaded
                            .as_ref()
                            .filter(|pl| pl.path == *path)
                            .map(|pl| pl.aux_files.clone())
                            .unwrap_or_default()
                    } else {
                        HashMap::new()
                    };
                    let mut ir = if is_temp {
                        let data = read_data(path)?;
                        check_cancel(cancel)?;
                        let pmx_model = crate::pmx::reader::read_pmx_from_data(&data)?;
                        check_cancel(cancel)?;
                        crate::pmx::extract::pmx_to_ir_with_aux(&pmx_model, pmx_dir, Some(&aux))?
                    } else {
                        let pmx_model = crate::pmx::reader::read_pmx(path)?;
                        check_cancel(cancel)?;
                        crate::pmx::extract::pmx_to_ir(&pmx_model, pmx_dir)?
                    };
                    check_cancel(cancel)?;
                    if normalize_pose {
                        ir.astance_result =
                            crate::intermediate::pose::normalize_pose_to_tstance_full(
                                &mut ir.bones,
                                &mut ir.meshes,
                                &mut ir.morphs,
                                &mut ir.physics,
                                crate::convert::coord::gltf_pos_to_pmx,
                            );
                    }
                    let source = if is_temp {
                        let data = read_data(path).unwrap_or_default();
                        ReloadableSource::Snapshot {
                            original_path: path.to_path_buf(),
                            main_bytes: data,
                            aux_files: aux,
                        }
                    } else {
                        ReloadableSource::File(path.to_path_buf())
                    };
                    Ok((ir, source, None))
                }
                FileFormat::Pmd => {
                    // Non-temp: read from disk directly (pmd_to_ir handles every aux extension, sph/spa etc.).
                    // Temp: use preloaded.aux_files via pmd_to_ir_with_aux.
                    // Clone aux_files once and reuse it for both model load and source construction.
                    let aux = if is_temp {
                        preloaded
                            .as_ref()
                            .filter(|pl| pl.path == *path)
                            .map(|pl| pl.aux_files.clone())
                            .unwrap_or_default()
                    } else {
                        HashMap::new()
                    };
                    let mut ir = if is_temp {
                        let data = read_data(path)?;
                        check_cancel(cancel)?;
                        let pmd_model = crate::pmd::reader::read_pmd_from_data(&data)?;
                        check_cancel(cancel)?;
                        crate::pmd::extract::pmd_to_ir_with_aux(&pmd_model, path, Some(&aux))?
                    } else {
                        let pmd_model = crate::pmd::reader::read_pmd(path)?;
                        check_cancel(cancel)?;
                        crate::pmd::extract::pmd_to_ir(&pmd_model, path)?
                    };
                    check_cancel(cancel)?;
                    if normalize_pose {
                        ir.astance_result =
                            crate::intermediate::pose::normalize_pose_to_tstance_full(
                                &mut ir.bones,
                                &mut ir.meshes,
                                &mut ir.morphs,
                                &mut ir.physics,
                                crate::convert::coord::gltf_pos_to_pmx,
                            );
                    }
                    let source = if is_temp {
                        let data = read_data(path).unwrap_or_default();
                        ReloadableSource::Snapshot {
                            original_path: path.to_path_buf(),
                            main_bytes: data,
                            aux_files: aux,
                        }
                    } else {
                        ReloadableSource::File(path.to_path_buf())
                    };
                    Ok((ir, source, None))
                }
                FileFormat::Obj => {
                    let ir = if is_temp {
                        let data = read_data(path)?;
                        check_cancel(cancel)?;
                        let obj_dir = path.parent().unwrap_or(Path::new("."));
                        let aux = collect_aux(path);
                        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Model");
                        crate::obj::extract::load_obj_from_data(&data, name, obj_dir, Some(&aux))?
                    } else {
                        crate::obj::extract::load_obj(path)?
                    };
                    check_cancel(cancel)?;
                    let data = read_data(path).unwrap_or_default();
                    let aux = collect_aux(path);
                    let source = make_source(data, aux);
                    Ok((ir, source, None))
                }
                FileFormat::Stl => {
                    let ir = if is_temp {
                        let data = read_data(path)?;
                        check_cancel(cancel)?;
                        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Model");
                        crate::stl::extract::load_stl_from_data(&data, name)?
                    } else {
                        crate::stl::extract::load_stl(path)?
                    };
                    check_cancel(cancel)?;
                    let data = read_data(path).unwrap_or_default();
                    let source = make_source(data, HashMap::new());
                    Ok((ir, source, None))
                }
                FileFormat::DirectX => {
                    let ir = if is_temp {
                        let data = read_data(path)?;
                        check_cancel(cancel)?;
                        let x_dir = path.parent().unwrap_or(Path::new("."));
                        let aux = collect_aux(path);
                        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Model");
                        crate::directx::extract::load_x_from_data(&data, name, x_dir, Some(&aux))?
                    } else {
                        crate::directx::extract::load_x(path)?
                    };
                    check_cancel(cancel)?;
                    let data = read_data(path).unwrap_or_default();
                    let aux = collect_aux(path);
                    let source = make_source(data, aux);
                    Ok((ir, source, None))
                }
                _ => anyhow::bail!("Unsupported format for background loading: {:?}", format),
            }
        } // CpuParseInput::File
        CpuParseInput::PkgModel {
            assets,
            model_index,
            model_type,
            source_path,
            pkg_index,
            source_override,
            normalize_pose: np,
            normalize_to_tstance: nt,
            append,
            suppress_tex_match,
            batch_progress,
            has_loaded_model,
        } => {
            use super::helpers::PkgModelType;
            use super::pending::{
                BgLoadKind, PkgAppendPayload, PkgFbxChoicePayload, PkgInitialPayload,
            };

            let source = source_override
                .unwrap_or_else(|| ReloadableSource::File(source_path.to_path_buf()));

            // For building PkgModelLocator: capture pathname before assets are consumed.
            let asset_pathname: Option<String> =
                assets.get(model_index).map(|a| a.pathname.clone());

            if append {
                // ── Append mode ──
                let mut pkg_unmatched: Vec<usize> = Vec::new();
                let mut pkg_model_name: Option<String> = None;
                let mut pkg_textures_to_add: Vec<(String, Arc<[u8]>)> = Vec::new();
                let pkg_material_keys: Vec<Option<crate::unitypackage::PkgMaterialKey>> =
                    Vec::new();

                let ir = match model_type {
                    PkgModelType::Fbx => {
                        check_cancel(cancel)?;
                        let (fbx_data, fbx_name, textures) =
                            crate::unitypackage::take_fbx_and_textures(&assets, model_index)?;
                        pkg_model_name = Some(fbx_name.clone());
                        check_cancel(cancel)?;
                        let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                            &fbx_data, None, np, nt,
                        )?;
                        check_cancel(cancel)?;
                        let unmatched =
                            crate::unitypackage::embed_textures_into_ir(&mut ir, &textures);
                        pkg_unmatched = unmatched;
                        pkg_textures_to_add = textures;
                        ir
                    }
                    PkgModelType::Vrm => {
                        check_cancel(cancel)?;
                        let (vrm_data, vrm_name) =
                            crate::unitypackage::take_vrm(&assets, model_index)?;
                        pkg_model_name = Some(vrm_name);
                        check_cancel(cancel)?;
                        let glb = crate::vrm::loader::load_glb_from_data(&vrm_data)?;
                        let version = crate::vrm::detect::detect_version(&glb.document);
                        let all_extensions = crate::vrm::loader::get_raw_extensions(&glb.document);
                        check_cancel(cancel)?;
                        let mut ir = crate::vrm::extract::extract_ir_model_with_options(
                            &glb.document,
                            &glb.buffers,
                            &glb.images,
                            &glb.vrm_extension,
                            &version,
                            &all_extensions,
                            np,
                        )?;
                        check_cancel(cancel)?;
                        super::ViewerApp::encode_ir_textures_as_png(&mut ir, &glb.images);
                        ir
                    }
                    PkgModelType::Prefab => {
                        check_cancel(cancel)?;
                        let pkg = pkg_index
                            .as_ref()
                            .context("Prefab append には pkg_index が必要です")?;
                        let resolve_result =
                            crate::unitypackage::resolve_single_prefab(pkg, model_index)?;

                        let prefab_filename =
                            std::path::Path::new(&pkg.entries[model_index].pathname)
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string();

                        let textures: Vec<crate::unitypackage::PackageTexture> = pkg
                            .entries
                            .iter()
                            .filter(|e| {
                                let lower = e.pathname.to_lowercase();
                                [
                                    ".png", ".jpg", ".jpeg", ".tga", ".bmp", ".psd", ".tif",
                                    ".tiff",
                                ]
                                .iter()
                                .any(|ext| lower.ends_with(ext))
                            })
                            .map(|e| {
                                let display_name = std::path::Path::new(&e.pathname)
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy();
                                crate::unitypackage::PackageTexture {
                                    guid: Arc::from(e.guid.as_str()),
                                    display_name: Arc::from(display_name.as_ref()),
                                    data: Arc::clone(&e.data),
                                    pathname: Arc::from(e.pathname.as_str()),
                                }
                            })
                            .collect();

                        pkg_textures_to_add = textures
                            .iter()
                            .map(|t| (t.display_name.to_string(), Arc::clone(&t.data)))
                            .collect();

                        let mut base_ir: Option<IrModel> = None;
                        check_cancel(cancel)?;

                        for (i, fbx_entry_info) in resolve_result.entries.iter().enumerate() {
                            let fbx_entry = &pkg.entries[fbx_entry_info.fbx_index];
                            let fbx_data = fbx_entry.data.to_vec();
                            let fbx_name = std::path::Path::new(&fbx_entry.pathname)
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string();
                            log::info!(
                                "  Append FBX[{}]: {} (GUID={})",
                                i,
                                fbx_name,
                                fbx_entry_info.fbx_guid
                            );
                            check_cancel(cancel)?;
                            let mut ir =
                                crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                                    &fbx_data, None, np, nt,
                                )?;
                            check_cancel(cancel)?;
                            let prefab_label = format!("prefab({})", prefab_filename);
                            let unmatched = crate::unitypackage::embed_textures_with_prefab(
                                &mut ir,
                                &textures,
                                &fbx_entry_info.materials,
                                &prefab_label,
                            );

                            if let Some(ref mut base) = base_ir {
                                let mat_offset = base.materials.len();
                                base.merge(ir);
                                pkg_unmatched.extend(unmatched.iter().map(|&idx| idx + mat_offset));
                            } else {
                                pkg_model_name = Some(prefab_filename.clone());
                                pkg_unmatched = unmatched;
                                base_ir = Some(ir);
                            }
                        }

                        base_ir.context("Prefab に有効な FBX が見つかりません")?
                    }
                };

                let pkg_locator = asset_pathname.map(|path| crate::unitypackage::PkgModelLocator {
                    guid: "".into(),
                    pathname: path.into(),
                    kind: model_type,
                });

                let payload = PkgAppendPayload {
                    pkg_model_name,
                    pkg_model_locator: pkg_locator,
                    pkg_textures_to_add,
                    pkg_unmatched,
                    batch_progress,
                    suppress_tex_match,
                    pkg_material_keys,
                };
                return Ok((ir, source, Some(BgLoadKind::PkgAppend(Box::new(payload)))));
            }

            // ── Normal load (append=false) ──
            match model_type {
                PkgModelType::Fbx => {
                    check_cancel(cancel)?;
                    let (fbx_data, fbx_name, textures_legacy, pkg_textures_new) =
                        if let Some(ref idx) = pkg_index {
                            let prepared = crate::unitypackage::prepare_pkg_fbx(idx, model_index)?;
                            let fbx_name = std::path::Path::new(prepared.model.pathname.as_ref())
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string();
                            let fbx_data = Arc::clone(&prepared.fbx_data);
                            let legacy_textures: Vec<(String, Arc<[u8]>)> = prepared
                                .textures
                                .iter()
                                .map(|t| (t.display_name.to_string(), Arc::clone(&t.data)))
                                .collect();
                            (fbx_data, fbx_name, legacy_textures, Some(prepared))
                        } else {
                            let (fbx_data, fbx_name, textures) =
                                crate::unitypackage::take_fbx_and_textures(&assets, model_index)?;
                            (fbx_data, fbx_name, textures, None)
                        };

                    check_cancel(cancel)?;

                    // has_loaded_model + has_anim => NeedsFbxChoice
                    if has_loaded_model {
                        let has_anim = if let Some(asset) = assets.get(model_index) {
                            crate::fbx::animation::load_fbx_animation_from_data(&asset.data)
                                .is_ok_and(|a| !a.is_empty())
                        } else {
                            false
                        };
                        if has_anim {
                            let payload = PkgFbxChoicePayload {
                                fbx_name,
                                assets,
                                fbx_index: model_index,
                                source_path,
                                archive_snapshot: None, // filled by spawn_bg_pkg_load
                                source_override: Some(source),
                                pkg_index,
                                batch_progress,
                            };
                            // NeedsFbxChoice: dummy IR (will not be applied)
                            let dummy_ir = IrModel::default();
                            let dummy_source = ReloadableSource::File(PathBuf::new());
                            return Ok((
                                dummy_ir,
                                dummy_source,
                                Some(BgLoadKind::NeedsFbxChoice(Box::new(payload))),
                            ));
                        }
                    }

                    let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                        &fbx_data, None, np, nt,
                    )?;
                    check_cancel(cancel)?;

                    // Embed textures into the IR.
                    let unmatched = if let Some(ref prepared) = pkg_textures_new {
                        let prefab_label = format!(
                            "prefab({})",
                            std::path::Path::new(&*prepared.model.pathname)
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                        );
                        crate::unitypackage::embed_textures_with_prefab(
                            &mut ir,
                            &prepared.textures,
                            &prepared.resolved,
                            &prefab_label,
                        )
                    } else {
                        crate::unitypackage::embed_textures_into_ir(&mut ir, &textures_legacy)
                    };

                    // Build pkg_material_keys.
                    let pkg_keys = if let Some(ref idx) = pkg_index {
                        let fbx_guid = idx
                            .entries
                            .get(model_index)
                            .map(|e| e.guid.as_str())
                            .unwrap_or("");
                        let instance_id = crate::unitypackage::BASE_INSTANCE_ID;
                        let model_guid: Arc<str> = fbx_guid.into();
                        ir.materials
                            .iter()
                            .map(|mat| {
                                Some(crate::unitypackage::PkgMaterialKey {
                                    instance_id,
                                    model_guid: model_guid.clone(),
                                    source_material: mat.source_material.clone(),
                                    material_name: mat.name.as_str().into(),
                                })
                            })
                            .collect::<Vec<_>>()
                    } else {
                        Vec::new()
                    };

                    let pkg_model_locator = pkg_textures_new.as_ref().map(|p| p.model.clone());

                    let payload = PkgInitialPayload {
                        fbx_name: Some(fbx_name),
                        pkg_model_locator,
                        pkg_textures_legacy: textures_legacy,
                        unmatched_indices: unmatched,
                        pkg_material_keys: pkg_keys,
                        fbx_ranges: Vec::new(),
                        batch_progress,
                        suppress_tex_match,
                        prefab_name: None,
                        prefab_entry_path: None,
                    };
                    Ok((ir, source, Some(BgLoadKind::PkgInitial(Box::new(payload)))))
                }
                PkgModelType::Vrm => {
                    check_cancel(cancel)?;
                    let vrm_pathname: Option<String> =
                        assets.get(model_index).map(|a| a.pathname.clone());
                    let (vrm_data, vrm_name) = crate::unitypackage::take_vrm(&assets, model_index)?;
                    check_cancel(cancel)?;
                    let glb = crate::vrm::loader::load_glb_from_data(&vrm_data)?;
                    let version = crate::vrm::detect::detect_version(&glb.document);
                    let all_extensions = crate::vrm::loader::get_raw_extensions(&glb.document);
                    check_cancel(cancel)?;
                    let mut ir = crate::vrm::extract::extract_ir_model_with_options(
                        &glb.document,
                        &glb.buffers,
                        &glb.images,
                        &glb.vrm_extension,
                        &version,
                        &all_extensions,
                        np,
                    )?;
                    check_cancel(cancel)?;
                    super::ViewerApp::encode_ir_textures_as_png(&mut ir, &glb.images);

                    let pkg_model_locator =
                        vrm_pathname.map(|path| crate::unitypackage::PkgModelLocator {
                            guid: "".into(),
                            pathname: path.into(),
                            kind: crate::unitypackage::PkgModelType::Vrm,
                        });

                    let payload = PkgInitialPayload {
                        fbx_name: Some(vrm_name),
                        pkg_model_locator,
                        pkg_textures_legacy: Vec::new(),
                        unmatched_indices: Vec::new(),
                        pkg_material_keys: Vec::new(),
                        fbx_ranges: Vec::new(),
                        batch_progress,
                        suppress_tex_match,
                        prefab_name: None,
                        prefab_entry_path: None,
                    };
                    Ok((ir, source, Some(BgLoadKind::PkgInitial(Box::new(payload)))))
                }
                PkgModelType::Prefab => {
                    check_cancel(cancel)?;
                    let pkg = pkg_index
                        .as_ref()
                        .context("Prefab ロードには pkg_index が必要です")?;
                    let resolve_result =
                        crate::unitypackage::resolve_single_prefab(pkg, model_index)?;

                    let prefab_filename = std::path::Path::new(&pkg.entries[model_index].pathname)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let prefab_entry_path = pkg.entries[model_index].pathname.clone();

                    let textures: Vec<crate::unitypackage::PackageTexture> = pkg
                        .entries
                        .iter()
                        .filter(|e| {
                            let lower = e.pathname.to_lowercase();
                            [
                                ".png", ".jpg", ".jpeg", ".tga", ".bmp", ".psd", ".tif", ".tiff",
                            ]
                            .iter()
                            .any(|ext| lower.ends_with(ext))
                        })
                        .map(|e| {
                            let display_name = std::path::Path::new(&e.pathname)
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy();
                            crate::unitypackage::PackageTexture {
                                guid: Arc::from(e.guid.as_str()),
                                display_name: Arc::from(display_name.as_ref()),
                                data: Arc::clone(&e.data),
                                pathname: Arc::from(e.pathname.as_str()),
                            }
                        })
                        .collect();

                    let legacy_textures: Vec<(String, Arc<[u8]>)> = textures
                        .iter()
                        .map(|t| (t.display_name.to_string(), Arc::clone(&t.data)))
                        .collect();

                    let mut base_ir: Option<IrModel> = None;
                    let mut all_pkg_keys: Vec<Option<crate::unitypackage::PkgMaterialKey>> =
                        Vec::new();
                    let mut all_unmatched: Vec<usize> = Vec::new();
                    let mut fbx_ranges: Vec<(String, usize, usize)> = Vec::new();
                    let mut first_fbx_name: Option<String> = None;
                    let mut first_locator: Option<crate::unitypackage::PkgModelLocator> = None;

                    check_cancel(cancel)?;

                    for (i, fbx_entry_info) in resolve_result.entries.iter().enumerate() {
                        let fbx_entry = &pkg.entries[fbx_entry_info.fbx_index];
                        let fbx_data = fbx_entry.data.to_vec();
                        let fbx_name = std::path::Path::new(&fbx_entry.pathname)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();

                        log::info!(
                            "  FBX[{}]: {} (GUID={})",
                            i,
                            fbx_name,
                            fbx_entry_info.fbx_guid
                        );
                        check_cancel(cancel)?;

                        let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                            &fbx_data, None, np, nt,
                        )?;
                        check_cancel(cancel)?;

                        let prefab_label = format!("prefab({})", prefab_filename);
                        let unmatched = crate::unitypackage::embed_textures_with_prefab(
                            &mut ir,
                            &textures,
                            &fbx_entry_info.materials,
                            &prefab_label,
                        );

                        let instance_id = crate::unitypackage::BASE_INSTANCE_ID;
                        let model_guid: Arc<str> = fbx_entry_info.fbx_guid.as_str().into();
                        let keys: Vec<_> = ir
                            .materials
                            .iter()
                            .map(|mat| {
                                Some(crate::unitypackage::PkgMaterialKey {
                                    instance_id,
                                    model_guid: model_guid.clone(),
                                    source_material: mat.source_material.clone(),
                                    material_name: mat.name.as_str().into(),
                                })
                            })
                            .collect();

                        if let Some(ref mut base) = base_ir {
                            let mat_offset = base.materials.len();
                            let mat_count = ir.materials.len();
                            base.merge(ir);
                            fbx_ranges.push((fbx_name, mat_offset, mat_count));
                            all_unmatched.extend(unmatched.iter().map(|&idx| idx + mat_offset));
                            all_pkg_keys.extend(keys);
                        } else {
                            let mat_count = ir.materials.len();
                            fbx_ranges.push((fbx_name.clone(), 0, mat_count));
                            first_fbx_name = Some(fbx_name);
                            first_locator = Some(crate::unitypackage::PkgModelLocator {
                                guid: fbx_entry_info.fbx_guid.as_str().into(),
                                pathname: fbx_entry.pathname.as_str().into(),
                                kind: crate::unitypackage::PkgModelType::Fbx,
                            });
                            all_unmatched = unmatched;
                            all_pkg_keys = keys;
                            base_ir = Some(ir);
                        }
                    }

                    let ir = base_ir.context("Prefab に有効な FBX が見つかりません")?;

                    let payload = PkgInitialPayload {
                        fbx_name: first_fbx_name,
                        pkg_model_locator: first_locator,
                        pkg_textures_legacy: legacy_textures,
                        unmatched_indices: all_unmatched,
                        pkg_material_keys: all_pkg_keys,
                        fbx_ranges,
                        batch_progress,
                        suppress_tex_match,
                        prefab_name: Some(prefab_filename),
                        prefab_entry_path: Some(prefab_entry_path),
                    };
                    Ok((ir, source, Some(BgLoadKind::PkgInitial(Box::new(payload)))))
                }
            }
        } // CpuParseInput::PkgModel
        CpuParseInput::UnityPackageIndex {
            path,
            preloaded,
            append,
        } => {
            check_cancel(cancel)?;

            let is_temp =
                is_temp_path(&path) || preloaded.as_ref().is_some_and(|pl| pl.path == path);

            // Read the file.
            let archive_data: Arc<[u8]> = if let Some(ref pl) = preloaded {
                if pl.path == path {
                    Arc::clone(&pl.main_bytes)
                } else {
                    std::fs::read(&path)?.into()
                }
            } else {
                std::fs::read(&path)?.into()
            };

            check_cancel(cancel)?;

            // Build the UnityPackageIndex.
            let pkg_index = Arc::new(crate::unitypackage::build_unity_package_index(
                &archive_data,
            )?);

            check_cancel(cancel)?;

            // Build the ExtractedAsset list.
            let assets: Vec<crate::unitypackage::ExtractedAsset> = pkg_index
                .entries
                .iter()
                .map(|e| crate::unitypackage::ExtractedAsset {
                    pathname: e.pathname.clone(),
                    data: Arc::clone(&e.data),
                })
                .collect();

            // Build the model list.
            let model_list = super::helpers::build_pkg_model_list(&assets);

            // For temp files, snapshot the archive data for reload.
            let archive_snapshot = if is_temp { Some(archive_data) } else { None };

            let bg_kind = super::pending::BgLoadKind::UnityPackageIndexed {
                pkg_index,
                assets,
                model_list,
                source_path: path.clone(),
                is_temp,
                archive_snapshot,
                append,
            };

            // Return dummy IrModel/source (apply_bg_load_result reads fields from `kind` instead).
            let ir = IrModel::default();
            let source = ReloadableSource::File(path);
            Ok((ir, source, Some(bg_kind)))
        }
        CpuParseInput::ArchiveIndex {
            path,
            preloaded,
            append,
        } => {
            check_cancel(cancel)?;

            let is_temp =
                is_temp_path(&path) || preloaded.as_ref().is_some_and(|pl| pl.path == path);

            // Read the file.
            let archive_data: Arc<[u8]> = if let Some(ref pl) = preloaded {
                if pl.path == path {
                    Arc::clone(&pl.main_bytes)
                } else {
                    std::fs::read(&path)?.into()
                }
            } else {
                std::fs::read(&path)?.into()
            };

            check_cancel(cancel)?;

            let ext = crate::path_ext_lower(&path);
            let format = crate::archive::archive_format_from_ext(&ext)
                .with_context(|| t!("error.unsupported_archive_format", ext = ext).into_owned())?;

            // List the models contained in the archive.
            let contents = crate::archive::list_models(&archive_data, format)?;

            check_cancel(cancel)?;

            // 7z: once entries are extracted, release the compressed payload to reduce memory peak.
            // Reload re-reads from disk (kept in memory only when is_temp).
            let archive_data = if format == crate::archive::ArchiveFormat::SevenZ && !is_temp {
                log::debug!(
                    "7z: releasing {} bytes of compressed data (entries already extracted)",
                    archive_data.len()
                );
                Arc::from([] as [u8; 0])
            } else {
                archive_data
            };

            let bg_kind = super::pending::BgLoadKind::ArchiveIndexed {
                archive_data,
                format,
                contents,
                source_path: path.clone(),
                is_temp,
                append,
            };

            let ir = IrModel::default();
            let source = ReloadableSource::File(path);
            Ok((ir, source, Some(bg_kind)))
        }
    } // match input
}

/// v0.5.6: snapshot of one UV morph's offsets, retained across reload (per-morph entry).
///
/// Codex review 0.5.6/04 P1 fix: the previous `HashMap<name, ...>` lost edits on key
/// collisions via last-write-wins (duplicate UV-morph names exist in VRM/glTF).
/// Now we keep all UV morphs in a Vec in order, and restoration matches uniquely by
/// `(name, name_en, channel)` plus an unused flag. Even with duplicate names, the
/// N-th morph correctly receives the N-th offsets back.
#[derive(Clone)]
struct UvMorphOffsetEntry {
    name: String,
    name_en: String,
    channel: u8,
    offsets: Vec<(usize, [f32; 4])>,
}

type UvMorphOffsetsSnapshot = Vec<UvMorphOffsetEntry>;

/// Bag of fields that `reload_current` saves and restores around a reload.
/// Centralising them here prevents drift when new state is added.
pub(crate) struct ReloadSnapshot {
    appended_models: Vec<AppendedModel>,
    camera: OrbitCamera,
    morph_weights: Vec<f32>,
    material_visibility: Vec<bool>,
    material_display: Vec<MaterialDisplayState>,
    material_filter: String,
    pmx_output_path: String,
    model_display_name: String,
    export_visible_only: bool,
    /// Side-panel tab the user had open before reload.
    /// `finish_load_with_gpu` resets it to Info; we restore it here so the user
    /// stays on, e.g., the Export tab after triggering a reload from there.
    side_panel_tab: super::SidePanelTab,
    tex_assignments: HashMap<usize, TextureSource>,
    pkg_tex_assignments: HashMap<usize, String>,
    pkg_textures: Option<Vec<(String, Arc<[u8]>)>>,
    vrma_library: Vec<(
        String,
        PathBuf,
        Arc<crate::intermediate::animation::VrmaAnimation>,
    )>,
    vrma_active_index: Option<usize>,
    display: DisplaySettings,
    /// v0.5.5: per-vertex UV edit overrides, kept across reload.
    /// `finish_load_with_gpu` calls `uv_edit.reset()` during reload, so we stash
    /// them here and reapply to both IR and GPU in `restore_snapshot_on_success`.
    /// Phase 3 A-1: VertexKey was widened to the triple `(mi, vi, uv_set)`.
    uv_edit_overrides: HashMap<(u32, u32, u8), [f32; 2]>,
    uv_edit_active_material: usize,
    uv_edit_window_open: bool,
    /// v0.5.6 (Codex review 0.5.6/03 P1 fix): preserves the old IR's UV-morph offsets so
    /// unsaved UV-morph edits — those `write_displayed_uv` wrote directly into the old IR —
    /// can be written back into the matching morph of the new IR after a successful reload.
    /// Key: morph `name_en` (falls back to `name`); value: `(channel, Vec<(global_vi, [f32;4])>)`.
    uv_morph_offsets: UvMorphOffsetsSnapshot,
}

impl ViewerApp {
    /// Move `aux_files` out of `preloaded` when available (avoiding a clone),
    /// otherwise gather them recursively from disk.
    pub(super) fn take_or_collect_aux(&mut self, path: &Path) -> HashMap<PathBuf, Arc<[u8]>> {
        if let Some(ref pl) = self.preloaded {
            if pl.path == path {
                // Move aux_files out of preloaded (avoids re-allocating the HashMap).
                let pl = self.preloaded.take().expect("preloaded confirmed Some");
                self.preloaded = Some(PreloadedData {
                    path: pl.path,
                    main_bytes: pl.main_bytes,
                    aux_files: HashMap::new(),
                });
                return pl.aux_files;
            }
        }
        let mut aux = HashMap::new();
        if let Some(dir) = path.parent() {
            collect_image_files_recursive(dir, dir, &mut aux);
        }
        aux
    }

    /// Return temp-preloaded bytes when available, otherwise read from disk.
    pub(super) fn read_or_preloaded(&self, path: &Path) -> anyhow::Result<Arc<[u8]>> {
        if let Some(ref pl) = self.preloaded {
            if pl.path == path {
                return Ok(Arc::clone(&pl.main_bytes));
            }
            // Also check aux_files for sub-file references.
            if let Some(data) = pl.aux_files.get(path) {
                return Ok(Arc::clone(data));
            }
        }
        Ok(std::fs::read(path)?.into())
    }

    /// Inspect an FBX file for the presence of mesh and animation data.
    fn inspect_fbx(&self, path: &Path) -> FbxContentInfo {
        let data = match self.read_or_preloaded(path) {
            Ok(d) => d,
            Err(_) => {
                return FbxContentInfo {
                    has_mesh: false,
                    has_anim: false,
                }
            }
        };
        FbxContentInfo {
            has_mesh: crate::fbx::extract::fbx_has_mesh(&data),
            has_anim: crate::fbx::animation::load_fbx_animation_from_data(&data)
                .is_ok_and(|a| !a.is_empty()),
        }
    }

    /// Main-thread routing for load dispatches.
    /// Animation detection, FBX choice, and archive/pkg flows take the existing
    /// synchronous paths; only raw model parsing is offloaded to the BG thread.
    pub(super) fn route_load_dispatch(
        &mut self,
        dispatch: super::pending::PendingLoadDispatch,
        prior_loading: Option<super::pending::BgLoadHandle>,
    ) {
        use super::pending::{BackgroundLoadState, BgLoadKind};

        let path = dispatch.path;
        let append = dispatch.append;
        let is_reload = dispatch.is_reload;
        let ext = crate::path_ext_lower(&path);
        let format = detect_format(&ext);

        // Decide the dispatch kind first.
        // Animation-only requests (vrma / .anim / gltf-anim / anim-only FBX applied to
        // the currently loaded model) depend on an in-flight model load, so we must
        // not cancel it here. We reject the request instead when a bg_load is running
        // (the animation has no defined target model yet).
        let is_anim_only_request = !append
            && match ext.as_str() {
                "vrma" => true,
                "anim" => self.loaded.is_some(),
                "glb" | "gltf" if self.loaded.is_some() => {
                    vrm::animation::load_gltf_animation(&path)
                        .map(|a| !a.is_empty())
                        .unwrap_or(false)
                }
                _ => {
                    format == FileFormat::Fbx && self.loaded.is_some() && {
                        let info = self.inspect_fbx(&path);
                        !info.has_mesh && info.has_anim
                    }
                }
            };

        if is_anim_only_request {
            if let Some(prior) = prior_loading {
                // If an animation request arrives while a model load is in flight,
                // cancelling would erase the target model and fail both. Reject the
                // request and keep the current load alive.
                log::warn!(
                    "Cannot load animation while model load is in progress: {}",
                    path.display()
                );
                self.convert_message = Some(ConvertMessage::failure(
                    t!("viewer.toast.precondition.busy_loading").into_owned(),
                ));
                // Put the prior Loading back into bg_state to protect the current load.
                self.pending.bg_state = BackgroundLoadState::Loading(prior);
                return;
            }
            // No bg_load in flight: fall through to the normal flow that applies the animation
            // to the existing model (no cancellation needed).
        } else {
            // Model load request: cancel any in-flight bg_load and accept the new one.
            if let Some(old) = prior_loading {
                old.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                log::info!(
                    "Cancelling previous bg load (req={}) for new dispatch: {}",
                    old.request_id,
                    path.display()
                );
                // `old` is dropped here, which also closes its receiver channel.
            }
        }

        // Stash dispatch.preloaded into self.preloaded for compatibility with existing methods.
        self.preloaded = dispatch.preloaded;

        // Append mode.
        if append {
            // unitypackage / archive build their index on the BG thread.
            if format == FileFormat::UnityPackage
                || format == FileFormat::Zip
                || format == FileFormat::SevenZ
            {
                self.spawn_bg_index_load(path, format, true);
                return;
            }
            // Other formats: parse on the BG thread.
            self.spawn_bg_load(path, BgLoadKind::Append, format);
            return;
        }

        // --- Below: normal (non-append) load ---

        // On load, restore any preview bind group then clear it.
        self.cancel_tex_match_preview();
        // Clear package textures unless we're loading another unitypackage.
        if format != FileFormat::UnityPackage {
            self.tex.pkg_textures = None;
            self.clear_pkg_thumb_cache();
        }

        // Detect animation-only files (handled inline; no BG needed).
        if ext == "vrma" {
            self.try_load_vrma(&path);
            return;
        }
        if (ext == "glb" || ext == "gltf") && self.loaded.is_some() {
            if let Ok(anims) = vrm::animation::load_gltf_animation(&path) {
                if !anims.is_empty() {
                    self.try_load_gltf_animation(&path);
                    return;
                }
            }
        }
        if ext == "anim" && self.loaded.is_some() {
            self.try_load_unity_animation(&path);
            return;
        }

        // FBX with both mesh and animation: open the choice dialog.
        if format == FileFormat::Fbx {
            let info = self.inspect_fbx(&path);
            if info.has_mesh && info.has_anim {
                self.pending.fbx_choice = Some(PendingFbxChoice {
                    path: path.clone(),
                    load_model: true,
                    load_animation: true,
                    pkg_context: None,
                    preloaded: self.preloaded.take(),
                });
                return;
            } else if !info.has_mesh && info.has_anim {
                if self.loaded.is_some() {
                    self.try_load_fbx_animation(&path);
                } else {
                    self.convert_message = Some(ConvertMessage::failure(
                        t!("viewer.toast.precondition.load_model_first").into_owned(),
                    ));
                }
                return;
            }
        }

        // archive / unitypackage build their index on the BG thread.
        if format == FileFormat::UnityPackage {
            self.tex.pkg_textures = None;
            self.clear_pkg_thumb_cache();
            self.normalize_pose = false;
            self.normalize_to_tstance = false;
            self.spawn_bg_index_load(path, format, false);
            return;
        }
        if matches!(format, FileFormat::Zip | FileFormat::SevenZ) {
            self.normalize_pose = false;
            self.normalize_to_tstance = false;
            self.spawn_bg_index_load(path, format, false);
            return;
        }

        // FBX auto-animation: whether to auto-apply animation after BG load completes.
        let auto_fbx_anim = format == FileFormat::Fbx && self.inspect_fbx(&path).has_anim;

        // Pre-reset stance flags (shader state is reset by finish_load_with_gpu on success).
        // For reload-driven dispatches, skip this so the user's A/T-stance conversion choices
        // (e.g. set on the Export tab) survive the reload.
        if !is_reload {
            self.normalize_pose = false;
            self.normalize_to_tstance = false;
        }

        self.spawn_bg_load(
            path,
            BgLoadKind::Initial {
                format,
                auto_fbx_anim,
            },
            format,
        );
    }

    /// Shared helper that runs `cpu_parse_source` on a BG thread.
    /// Centralises cancellation of any prior Loading, request_id / cancel / channel
    /// creation, BgLoadHandle wiring, and thread spawn.
    ///
    /// `post_map` is a closure run on the BG thread to make final tweaks to the
    /// `BgLoadResult` — for example, attaching `archive_snapshot` to a NeedsFbxChoice.
    fn spawn_bg_task(
        &mut self,
        input: CpuParseInput,
        fallback_kind: super::pending::BgLoadKind,
        result_path: PathBuf,
        post_map: impl FnOnce(&mut super::pending::BgLoadResult) + Send + 'static,
    ) {
        use super::pending::BackgroundLoadState;
        use std::sync::atomic::AtomicBool;
        use std::sync::Arc;

        // Cancel any leftover prior Loading (safety net for paths that bypass route_load_dispatch).
        if let BackgroundLoadState::Loading(old) =
            std::mem::replace(&mut self.pending.bg_state, BackgroundLoadState::Idle)
        {
            old.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }

        let request_id = self.fresh_request_id();
        let cancel = Arc::new(AtomicBool::new(false));
        let normalize_pose = self.normalize_pose;
        let normalize_to_tstance = self.normalize_to_tstance;

        let (tx, rx) = std::sync::mpsc::channel();
        self.pending.bg_state = BackgroundLoadState::Loading(super::pending::BgLoadHandle {
            rx,
            cancel: Arc::clone(&cancel),
            request_id,
        });

        std::thread::spawn(move || {
            let result = cpu_parse_source(input, normalize_pose, normalize_to_tstance, &cancel);
            let result = result.map(|(ir, source, kind_override)| {
                let mut bg_result = super::pending::BgLoadResult {
                    ir,
                    source,
                    kind: kind_override.unwrap_or(fallback_kind),
                    path: result_path,
                    request_id,
                };
                post_map(&mut bg_result);
                bg_result
            });
            let _ = tx.send(result);
        });
    }

    /// Run CPU-side parsing on a BG thread.
    fn spawn_bg_load(
        &mut self,
        path: PathBuf,
        kind: super::pending::BgLoadKind,
        format: FileFormat,
    ) {
        let preloaded = self.preloaded.take();
        let input = CpuParseInput::File {
            path: path.clone(),
            format,
            preloaded,
        };
        self.spawn_bg_task(input, kind, path, |_| {});
    }

    /// Run the entry-point work for UnityPackage / archive (file read + index build) on a BG thread.
    fn spawn_bg_index_load(&mut self, path: PathBuf, format: FileFormat, append: bool) {
        use super::pending::BgLoadKind;

        let preloaded = self.preloaded.take();
        let input = if format == FileFormat::UnityPackage {
            CpuParseInput::UnityPackageIndex {
                path: path.clone(),
                preloaded,
                append,
            }
        } else {
            CpuParseInput::ArchiveIndex {
                path: path.clone(),
                preloaded,
                append,
            }
        };
        // Index loads always return a kind_override, so the fallback is never used.
        self.spawn_bg_task(input, BgLoadKind::ArchiveInitial, path, |_| {});
    }

    /// Decompress and parse an in-archive model on a BG thread.
    pub(super) fn spawn_bg_archive_load(&mut self, p: PendingArchiveLoad) {
        use super::pending::BgLoadKind;

        let result_path = p.source_path.clone();
        let append = p.append;
        let input = CpuParseInput::ArchiveModel {
            archive_data: p.archive_data,
            format: p.format,
            contents: p.contents,
            model_index: p.model_index,
            source_path: p.source_path,
            is_temp: p.is_temp,
            append,
            normalize_pose: self.normalize_pose,
            normalize_to_tstance: self.normalize_to_tstance,
        };
        let fallback = if append {
            BgLoadKind::ArchiveAppend
        } else {
            BgLoadKind::ArchiveInitial
        };
        self.spawn_bg_task(input, fallback, result_path, |_| {});
    }

    /// Parse a model contained in a UnityPackage on a BG thread.
    pub(super) fn spawn_bg_pkg_load(&mut self, p: PendingPkgModelLoad) {
        use super::pending::BgLoadKind;

        // When skip_anim_check is true (after execute_fbx_choice has resolved), force
        // has_loaded_model to false to prevent looping back into NeedsFbxChoice.
        let has_loaded_model = if p.skip_anim_check {
            false
        } else {
            self.loaded.is_some()
        };

        // Build source_override with priority: nested_archive_source > archive_snapshot > None.
        let source_override = if let Some(nested) = p.nested_archive_source {
            Some(nested)
        } else if let Some(ref snap) = p.archive_snapshot {
            Some(ReloadableSource::Snapshot {
                original_path: p.source_path.clone(),
                main_bytes: Arc::clone(snap),
                aux_files: HashMap::new(),
            })
        } else {
            None
        };

        let result_path = p.source_path.clone();
        let archive_snapshot = p.archive_snapshot.clone();
        let input = CpuParseInput::PkgModel {
            assets: p.assets,
            model_index: p.fbx_index,
            model_type: p.model_type,
            source_path: p.source_path,
            pkg_index: p.pkg_index,
            source_override,
            normalize_pose: self.normalize_pose,
            normalize_to_tstance: self.normalize_to_tstance,
            append: p.append,
            suppress_tex_match: p.suppress_tex_match,
            batch_progress: p.batch_progress,
            has_loaded_model,
        };
        let fallback = if p.append {
            BgLoadKind::ArchiveAppend
        } else {
            BgLoadKind::ArchiveInitial
        };
        self.spawn_bg_task(input, fallback, result_path, move |bg_result| {
            // Attach archive_snapshot to the NeedsFbxChoice payload.
            if let BgLoadKind::NeedsFbxChoice(ref mut payload) = bg_result.kind {
                payload.archive_snapshot = archive_snapshot;
            }
        });
    }

    /// Post-processing for BG parse results (basic path: direct load / append).
    pub(super) fn apply_bg_load_result(
        &mut self,
        result: super::pending::BgLoadResult,
    ) -> anyhow::Result<()> {
        use super::pending::BgLoadKind;
        match result.kind {
            BgLoadKind::Initial {
                format,
                auto_fbx_anim,
            } => {
                self.start_deferred_gpu_build(
                    result.ir,
                    result.source,
                    Some(BgLoadKind::Initial {
                        format,
                        auto_fbx_anim,
                    }),
                    result.path,
                );
            }
            BgLoadKind::Append => {
                // Coordinate-system compatibility check.
                if let Some(ref loaded) = self.loaded {
                    let host_fmt = loaded.ir.source_format;
                    let other_fmt = result.ir.source_format;
                    if host_fmt.is_vrm0() != other_fmt.is_vrm0() {
                        log::warn!(
                            "Appending model with different coordinate system: {} + {}",
                            host_fmt.label(),
                            other_fmt.label()
                        );
                        anyhow::bail!(t!(
                            "viewer.toast.append.coord_mismatch",
                            host = host_fmt.label(),
                            other = other_fmt.label(),
                        )
                        .into_owned());
                    }
                }
                self.start_deferred_append_gpu_build(
                    result.ir,
                    result.source,
                    false,
                    None,
                    None,
                    result.path,
                );
            }
            BgLoadKind::ArchiveInitial => {
                self.start_deferred_gpu_build(
                    result.ir,
                    result.source,
                    Some(BgLoadKind::ArchiveInitial),
                    result.path,
                );
            }
            BgLoadKind::ArchiveAppend => {
                // Coordinate-system compatibility check.
                if let Some(ref loaded) = self.loaded {
                    let host_fmt = loaded.ir.source_format;
                    let other_fmt = result.ir.source_format;
                    if host_fmt.is_vrm0() != other_fmt.is_vrm0() {
                        log::warn!(
                            "Appending archive model with different coordinate system: {} + {}",
                            host_fmt.label(),
                            other_fmt.label()
                        );
                        anyhow::bail!(t!(
                            "viewer.toast.append.coord_mismatch",
                            host = host_fmt.label(),
                            other = other_fmt.label(),
                        )
                        .into_owned());
                    }
                }
                self.start_deferred_append_gpu_build(
                    result.ir,
                    result.source,
                    false,
                    None,
                    None,
                    result.path,
                );
            }
            BgLoadKind::ArchivePreparedUnityPackage {
                pkg_data: _,
                pkg_index,
                assets,
                model_list,
                source_path,
                archive_data,
                is_temp,
                append,
                entry_path,
            } => {
                if model_list.is_empty() {
                    anyhow::bail!(".unitypackage 内に VRM / FBX ファイルが見つかりません");
                }

                let archive_snapshot = if is_temp {
                    Some(Arc::clone(&archive_data))
                } else {
                    None
                };
                let nested_archive_source = Some(ReloadableSource::Archive {
                    original_path: source_path.clone(),
                    archive_bytes: if is_temp { Some(archive_data) } else { None },
                    selected_entry_path: entry_path.to_string_lossy().into_owned(),
                    inner_kind: crate::archive::ArchiveModelKind::UnityPackage,
                });

                if model_list.len() == 1 {
                    let (model_index, ref _name, model_type) = model_list[0];
                    log::info!("Archive .unitypackage (bg): 1 model detected");
                    self.pending.pkg_load = Some(PendingPkgModelLoad {
                        assets: Arc::new(assets),
                        fbx_index: model_index,
                        model_type,
                        source_path: source_path.clone(),
                        shown: false,
                        append,
                        suppress_tex_match: false,
                        archive_snapshot,
                        nested_archive_source,
                        pkg_index: Some(pkg_index),
                        batch_progress: None,
                        skip_anim_check: false,
                    });
                } else {
                    log::info!(
                        "Archive .unitypackage (bg): found {} models:",
                        model_list.len()
                    );
                    for (_, name, mt) in &model_list {
                        let label = match mt {
                            PkgModelType::Prefab => "Prefab",
                            PkgModelType::Vrm => "VRM",
                            PkgModelType::Fbx => "FBX",
                        };
                        log::info!("  [{}] {}", label, name);
                    }
                    let checked = vec![false; model_list.len()];
                    self.pending.unity_pkg = Some(PendingUnityPackage {
                        assets,
                        model_list,
                        source_path: source_path.clone(),
                        append,
                        archive_snapshot,
                        nested_archive_source,
                        pkg_index: Some(pkg_index),
                        checked,
                    });
                }
            }
            BgLoadKind::PkgInitial(mut payload) => {
                // Pre-GPU-build setup: selected_fbx_name, pkg_textures, etc.
                self.selected_fbx_name = payload.fbx_name.clone();
                self.selected_pkg_model = payload.pkg_model_locator.clone();

                // Move pkg_textures out of the payload and apply immediately.
                let pkg_tex = std::mem::take(&mut payload.pkg_textures_legacy);
                if !pkg_tex.is_empty() {
                    self.tex.pkg_textures = Some(pkg_tex);
                    self.rebuild_pkg_thumb_cache();
                }

                // The GPU build itself is deferred; finalisation happens in apply_gpu_build_post.
                self.start_deferred_gpu_build(
                    result.ir,
                    result.source,
                    Some(BgLoadKind::PkgInitial(payload)),
                    result.path,
                );
            }
            BgLoadKind::PkgAppend(payload) => {
                let pkg_model_name = payload.pkg_model_name.clone();
                let pkg_model_locator = payload.pkg_model_locator.clone();
                self.start_deferred_append_gpu_build_ext(
                    result.ir,
                    result.source,
                    false,
                    pkg_model_name,
                    pkg_model_locator,
                    result.path,
                    Some(payload),
                );
            }
            BgLoadKind::NeedsFbxChoice(payload) => {
                // Show the FBX choice dialog so the user can pick model / animation / both.
                self.pending.fbx_choice = Some(PendingFbxChoice {
                    path: PathBuf::from(&payload.fbx_name),
                    load_model: true,
                    load_animation: true,
                    pkg_context: Some(super::pending::PendingFbxChoicePkg {
                        assets: payload.assets,
                        fbx_index: payload.fbx_index,
                        source_path: payload.source_path,
                        archive_snapshot: payload.archive_snapshot,
                        nested_archive_source: payload.source_override,
                        pkg_index: payload.pkg_index,
                    }),
                    preloaded: None,
                });
            }
            BgLoadKind::UnityPackageIndexed {
                pkg_index,
                assets,
                model_list,
                source_path,
                is_temp,
                archive_snapshot,
                append,
            } => {
                if model_list.is_empty() {
                    anyhow::bail!(".unitypackage 内に VRM / FBX ファイルが見つかりません");
                }

                let snapshot = if is_temp { archive_snapshot } else { None };

                if model_list.len() == 1 {
                    let (idx, _, model_type) = model_list[0];
                    self.pending.pkg_load = Some(PendingPkgModelLoad {
                        assets: Arc::new(assets),
                        fbx_index: idx,
                        model_type,
                        source_path,
                        shown: false,
                        append,
                        suppress_tex_match: if append {
                            self.suppress_tex_match
                        } else {
                            false
                        },
                        archive_snapshot: snapshot,
                        nested_archive_source: None,
                        pkg_index: Some(pkg_index),
                        batch_progress: None,
                        skip_anim_check: false,
                    });
                } else {
                    log::info!("Found {} models in .unitypackage:", model_list.len());
                    for (_, name, mtype) in &model_list {
                        log::info!("  {:?}: {}", mtype, name);
                    }
                    let checked = vec![false; model_list.len()];
                    self.pending.unity_pkg = Some(PendingUnityPackage {
                        assets,
                        model_list,
                        source_path,
                        append,
                        archive_snapshot: snapshot,
                        nested_archive_source: None,
                        pkg_index: Some(pkg_index),
                        checked,
                    });
                }
            }
            BgLoadKind::ArchiveIndexed {
                archive_data,
                format,
                contents,
                source_path,
                is_temp,
                append,
            } => {
                if contents.models.is_empty() {
                    anyhow::bail!(t!("error.archive_no_models_found").into_owned());
                }

                if contents.models.len() == 1 {
                    self.pending.archive_load = Some(PendingArchiveLoad {
                        archive_data,
                        format,
                        contents,
                        model_index: 0,
                        source_path,
                        shown: false,
                        append,
                        is_temp,
                    });
                } else {
                    log::info!("Found {} models in archive:", contents.models.len());
                    for (_, p, _, kind) in &contents.models {
                        log::info!("  [{}] {}", kind.label(), p.display());
                    }
                    self.pending.archive = Some(super::pending::PendingArchive {
                        archive_data,
                        format,
                        contents,
                        source_path,
                        append,
                        is_temp,
                    });
                }
            }
        }
        Ok(())
    }

    /// Post-processing after the deferred per-frame GPU build completes
    /// (called from `process_pending_tasks`).
    pub(super) fn apply_gpu_build_post(
        &mut self,
        post_kind: Option<super::pending::BgLoadKind>,
        path: &Path,
    ) {
        use super::pending::BgLoadKind;
        match post_kind {
            None => {
                log::info!("Model loaded (deferred gpu): {}", path.display());
                self.convert_message = None;
            }
            Some(BgLoadKind::Initial { auto_fbx_anim, .. }) => {
                log::info!("Model loaded (deferred gpu): {}", path.display());
                self.convert_message = None;
                self.anim.state = None;
                self.anim.library.clear();
                self.anim.active_index = None;
                if auto_fbx_anim {
                    self.try_load_fbx_animation(path);
                }
            }
            Some(BgLoadKind::ArchiveInitial) => {
                log::info!(
                    "Model loaded from archive (deferred gpu): {}",
                    path.display()
                );
                self.convert_message = None;
                self.anim.state = None;
                self.anim.library.clear();
                self.anim.active_index = None;
            }
            Some(BgLoadKind::PkgInitial(payload)) => {
                // Set pkg_material_keys after finish_load_with_gpu has populated `loaded`.
                if !payload.pkg_material_keys.is_empty() {
                    if let Some(ref mut loaded) = self.loaded {
                        loaded.pkg_material_keys = payload.pkg_material_keys;
                    }
                }

                // Prefab: set prefab_name / prefab_entry_path, then overwrite
                // model_display_name with the prefab name (extension stripped and sanitized).
                let new_display_name: Option<String> = if let Some(ref mut loaded) = self.loaded {
                    if let Some(ref prefab_filename) = payload.prefab_name {
                        let stem = std::path::Path::new(prefab_filename)
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy();
                        let sanitized = crate::sanitize_filename(&stem);
                        loaded.prefab_name = payload.prefab_name.clone();
                        loaded.prefab_entry_path = payload.prefab_entry_path.clone();
                        sanitized
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(name) = new_display_name {
                    self.export.model_display_name = name;
                    self.refresh_derived_from_display_name();
                }

                // Prefab: build MaterialGroup from fbx_ranges.
                if payload.fbx_ranges.len() > 1 {
                    if let Some(ref mut loaded) = self.loaded {
                        let mut new_groups = Vec::with_capacity(payload.fbx_ranges.len());
                        for (name, mat_start, mat_count) in &payload.fbx_ranges {
                            let mat_range = *mat_start..*mat_start + *mat_count;
                            let mut draw_start = usize::MAX;
                            let mut draw_end = 0usize;
                            for (di, draw) in loaded.gpu_model.draws.iter().enumerate() {
                                if mat_range.contains(&draw.material_index) {
                                    draw_start = draw_start.min(di);
                                    draw_end = draw_end.max(di + 1);
                                }
                            }
                            if draw_start == usize::MAX {
                                draw_start = draw_end;
                            }
                            new_groups.push(MaterialGroup {
                                name: name.clone(),
                                material_range: mat_range,
                                draw_range: draw_start..draw_end,
                            });
                        }
                        loaded.material_groups = new_groups;
                    }
                }

                // Texture-picker dialog for materials still without an assigned texture.
                if !payload.unmatched_indices.is_empty()
                    && self.tex.pkg_textures.is_some()
                    && !payload.suppress_tex_match
                {
                    self.cancel_tex_match_preview();
                    let count = payload.unmatched_indices.len();
                    self.tex.pending_match = Some(PendingTexMatch {
                        mat_indices: payload.unmatched_indices,
                        selections: vec![None; count],
                        tex_filter: String::new(),
                        previewed: vec![None; count],
                        saved_binds: std::collections::HashMap::new(),
                        texture_views: Vec::new(),
                        failed_uploads: std::collections::HashSet::new(),
                    });
                }

                // Clear any previously bound animation.
                self.anim.state = None;
                self.anim.library.clear();
                self.anim.active_index = None;

                // Batch-progress toast.
                if let Some((current, total)) = payload.batch_progress {
                    let name = payload.fbx_name.as_deref().unwrap_or("?");
                    self.convert_message = Some(ConvertMessage::success(
                        t!(
                            "viewer.toast.progress.loaded",
                            current = current,
                            total = total,
                            name = name
                        )
                        .into_owned(),
                    ));
                } else {
                    self.convert_message = None;
                }
                log::info!("Model loaded from pkg (deferred gpu): {}", path.display());
            }
            _ => {
                // Other kinds do not use the deferred GPU path.
                log::warn!(
                    "Unexpected post_kind in apply_gpu_build_post: {}",
                    path.display()
                );
            }
        }
    }

    /// Legacy synchronous load path, kept as a fallback for archive/reload flows.
    /// New direct loads should go through route_load_dispatch → spawn_bg_load.
    #[allow(dead_code)]
    pub(super) fn load_file(&mut self, path: PathBuf) {
        log::info!("Open file: {}", path.display());
        let ext = crate::path_ext_lower(&path);
        let format = detect_format(&ext);

        // On load, restore any preview bind group then clear it.
        self.cancel_tex_match_preview();
        // Clear package textures unless we're loading another unitypackage.
        if format != FileFormat::UnityPackage {
            self.tex.pkg_textures = None;
            self.clear_pkg_thumb_cache();
        }

        // Animation-only file detection.
        if ext == "vrma" {
            self.try_load_vrma(&path);
            return;
        }
        // GLB / glTF: when a model is already loaded, ask whether to open as animation.
        if (ext == "glb" || ext == "gltf") && self.loaded.is_some() {
            // Pre-check whether the file actually contains animation data.
            if let Ok(anims) = vrm::animation::load_gltf_animation(&path) {
                if !anims.is_empty() {
                    self.try_load_gltf_animation(&path);
                    return;
                }
            }
        }
        // Unity .anim: always load as animation.
        if ext == "anim" && self.loaded.is_some() {
            self.try_load_unity_animation(&path);
            return;
        }

        // FBX with both mesh and animation: show the choice dialog (also on the very first load).
        if format == FileFormat::Fbx {
            let info = self.inspect_fbx(&path);
            if info.has_mesh && info.has_anim {
                // Both present → show the choice dialog.
                self.pending.fbx_choice = Some(PendingFbxChoice {
                    path: path.clone(),
                    load_model: true,
                    load_animation: true,
                    pkg_context: None,
                    preloaded: self.preloaded.take(),
                });
                return;
            } else if !info.has_mesh && info.has_anim {
                // Animation only.
                if self.loaded.is_some() {
                    self.try_load_fbx_animation(&path);
                } else {
                    self.convert_message = Some(ConvertMessage::failure(
                        t!("viewer.toast.precondition.load_model_first").into_owned(),
                    ));
                }
                return;
            }
            // Mesh only, or neither → fall through and load as a model.
        }

        // OBJ / STL: show the import-options dialog before loading.
        if format == FileFormat::Obj || format == FileFormat::Stl {
            use super::pending::{ImportUnit, PendingImportOptions};
            let (default_unit, default_z_up) = match format {
                FileFormat::Obj => (ImportUnit::Cm, false),
                FileFormat::Stl => (ImportUnit::Mm, true),
                _ => unreachable!(),
            };
            self.pending.import_options = Some(PendingImportOptions {
                path,
                format,
                append: false,
                preloaded: self.preloaded.take(),
                unit: default_unit,
                z_up: default_z_up,
            });
            return;
        }

        self.load_file_as_model(path);
    }

    /// Load a file as a model (path used when the FBX choice dialog is not needed).
    fn load_file_as_model(&mut self, path: PathBuf) {
        // Pre-reset stance flags only (shader state is reset by finish_load_with_gpu on success).
        self.normalize_pose = false;
        self.normalize_to_tstance = false;

        let ext = crate::path_ext_lower(&path);
        let format = detect_format(&ext);

        let result = match format {
            FileFormat::Fbx => self.try_load_fbx(&path),
            FileFormat::UnityPackage => self.try_load_unitypackage(&path),
            FileFormat::Pmx => self.try_load_pmx(&path),
            FileFormat::Pmd => self.try_load_pmd(&path),
            FileFormat::Obj => self.try_load_obj(&path),
            FileFormat::Stl => self.try_load_stl(&path),
            FileFormat::DirectX => self.try_load_x(&path),
            FileFormat::Zip | FileFormat::SevenZ => self.try_load_archive(&path),
            _ => self.try_load_vrm(&path),
        };

        match result {
            Ok(()) => {
                // Archives have only finished listing (model selection still pending); other formats are fully loaded.
                match format {
                    FileFormat::UnityPackage => {
                        log::info!("Unitypackage indexed: {}", path.display());
                    }
                    FileFormat::Zip | FileFormat::SevenZ => {
                        log::info!("Archive indexed: {}", path.display());
                    }
                    _ => {
                        log::info!("Model loaded: {}", path.display());
                    }
                }
                self.convert_message = None;
                self.anim.state = None;
                self.anim.library.clear();
                self.anim.active_index = None;

                // After loading the FBX as a model, auto-apply any animation found in the same file.
                if format == FileFormat::Fbx && self.inspect_fbx(&path).has_anim {
                    self.try_load_fbx_animation(&path);
                }
            }
            Err(e) => {
                log::error!("Load failed: {e}");
                let user_msg = t!(
                    "viewer.toast.reload.file_not_loaded",
                    detail = format!("{e}"),
                )
                .into_owned();
                self.convert_message = Some(ConvertMessage::failure(user_msg));
            }
        }
    }

    /// Run the OBJ / STL import-options dialog result.
    pub fn execute_import_with_options(&mut self, opts: super::pending::PendingImportOptions) {
        let scale = opts.unit.scale();
        let z_up = opts.z_up;
        let path = opts.path;
        self.preloaded = opts.preloaded;

        // Pre-reset stance flags only.
        self.normalize_pose = false;
        self.normalize_to_tstance = false;

        let result = match opts.format {
            FileFormat::Obj => self.try_load_obj_with_params(&path, scale, z_up),
            FileFormat::Stl => self.try_load_stl_with_params(&path, scale, z_up),
            _ => Err(anyhow::anyhow!(
                "Unexpected format for import options: {:?}",
                opts.format
            )),
        };
        if let Err(e) = result {
            self.convert_message = Some(ConvertMessage::failure(
                t!("viewer.toast.reload.load_failed", error = e.to_string()).into_owned(),
            ));
        }
    }

    /// Run the FBX load-mode choice dialog result.
    pub fn execute_fbx_choice(&mut self, choice: PendingFbxChoice) {
        let PendingFbxChoice {
            path,
            load_model,
            load_animation,
            pkg_context,
            preloaded,
        } = choice;

        let mode = match (load_model, load_animation) {
            (true, true) => FbxLoadMode::Both,
            (true, false) => FbxLoadMode::ModelOnly,
            (false, true) => FbxLoadMode::AnimationOnly,
            (false, false) => return,
        };

        // AnimationOnly is cheap; run it synchronously.
        if mode == FbxLoadMode::AnimationOnly {
            if let Some(pkg) = pkg_context {
                if let Some(asset) = pkg.assets.get(pkg.fbx_index) {
                    if let Ok(anims) =
                        crate::fbx::animation::load_fbx_animation_from_data(&asset.data)
                    {
                        let fbx_name = asset.filename();
                        for anim in anims {
                            let display_name = if anim.name == "animation" {
                                fbx_name.clone()
                            } else {
                                format!("{} ({})", fbx_name, anim.name)
                            };
                            let anim = Arc::new(anim);
                            if let Some(ref loaded) = self.loaded {
                                let state = AnimationState::new(
                                    Arc::clone(&anim),
                                    &loaded.ir,
                                    &loaded.gpu_model,
                                );
                                self.anim.library.push((
                                    display_name,
                                    pkg.source_path.clone(),
                                    anim,
                                ));
                                self.anim.active_index = Some(self.anim.library.len() - 1);
                                self.anim.state = Some(state);
                            }
                        }
                    }
                }
            } else {
                self.try_load_fbx_animation(&path);
            }
            return;
        }

        // ModelOnly / Both → parse on the BG thread.
        if let Some(pkg) = pkg_context {
            // Via unitypackage: build source_override and dispatch as PendingPkgModelLoad → BG.
            let source_override = if let Some(nested) = pkg.nested_archive_source {
                Some(nested)
            } else if let Some(ref snap) = pkg.archive_snapshot {
                Some(ReloadableSource::Snapshot {
                    original_path: pkg.source_path.clone(),
                    main_bytes: Arc::clone(snap),
                    aux_files: HashMap::new(),
                })
            } else {
                None
            };
            self.spawn_bg_pkg_load(PendingPkgModelLoad {
                assets: pkg.assets,
                fbx_index: pkg.fbx_index,
                model_type: super::helpers::PkgModelType::Fbx,
                source_path: pkg.source_path,
                shown: true, // spawn immediately
                append: false,
                suppress_tex_match: false,
                archive_snapshot: pkg.archive_snapshot,
                nested_archive_source: source_override,
                pkg_index: pkg.pkg_index,
                batch_progress: None,
                skip_anim_check: true,
            });
        } else {
            // Direct file → spawn_bg_load.
            self.preloaded = preloaded;
            let auto_fbx_anim = mode == FbxLoadMode::Both;
            self.cancel_tex_match_preview();
            self.normalize_pose = false;
            self.normalize_to_tstance = false;
            self.spawn_bg_load(
                path,
                super::pending::BgLoadKind::Initial {
                    format: FileFormat::Fbx,
                    auto_fbx_anim,
                },
                FileFormat::Fbx,
            );
        }
    }

    fn try_load_fbx(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let data = self.read_or_preloaded(path)?;
        let ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
            &data,
            Some(path),
            self.normalize_pose,
            self.normalize_to_tstance,
        )?;
        let source =
            if is_temp_path(path) || self.preloaded.as_ref().is_some_and(|pl| pl.path == path) {
                let aux = self.take_or_collect_aux(path);
                ReloadableSource::Snapshot {
                    original_path: path.to_path_buf(),
                    main_bytes: data,
                    aux_files: aux,
                }
            } else {
                ReloadableSource::File(path.to_path_buf())
            };
        self.finish_load(ir, source)
    }

    fn try_load_unitypackage(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        // Decide is_temp before the file may disappear (canonicalize requires the file to exist).
        let is_temp =
            is_temp_path(path) || self.preloaded.as_ref().is_some_and(|pl| pl.path == path);
        let archive_data: Arc<[u8]> = self.read_or_preloaded(path)?;

        // Phase 3: build UnityPackageIndex (used for Prefab texture resolution).
        let pkg_index = Arc::new(crate::unitypackage::build_unity_package_index(
            &archive_data,
        )?);
        // Also build ExtractedAsset for compatibility with the existing API (shares Arc; no copy).
        let assets: Vec<crate::unitypackage::ExtractedAsset> = pkg_index
            .entries
            .iter()
            .map(|e| crate::unitypackage::ExtractedAsset {
                pathname: e.pathname.clone(),
                data: Arc::clone(&e.data),
            })
            .collect();

        // For temp files, snapshot the archive data so reload can find it.
        let snapshot = if is_temp {
            Some(Arc::clone(&archive_data))
        } else {
            None
        };

        // Build a unified model list (FBX + VRM).
        let model_list = build_pkg_model_list(&assets);

        if model_list.is_empty() {
            anyhow::bail!(".unitypackage 内に VRM / FBX ファイルが見つかりません");
        }

        if model_list.len() == 1 {
            // Exactly one model → defer loading until after the progress UI shows.
            let (idx, _, model_type) = model_list[0];
            self.pending.pkg_load = Some(PendingPkgModelLoad {
                assets: Arc::new(assets),
                fbx_index: idx,
                model_type,
                source_path: path.to_path_buf(),
                shown: false,
                append: false,
                suppress_tex_match: false,
                archive_snapshot: snapshot,
                nested_archive_source: None,
                pkg_index: Some(pkg_index),
                batch_progress: None,
                skip_anim_check: false,
            });
        } else {
            // Multiple models → show the selection dialog.
            log::info!("Found {} models in .unitypackage:", model_list.len());
            for (_, name, mtype) in &model_list {
                log::info!("  {:?}: {}", mtype, name);
            }
            let checked = vec![false; model_list.len()];
            self.pending.unity_pkg = Some(PendingUnityPackage {
                assets,
                model_list,
                source_path: path.to_path_buf(),
                append: false,
                archive_snapshot: snapshot,
                nested_archive_source: None,
                pkg_index: Some(pkg_index),
                checked,
            });
        }
        Ok(())
    }

    /// Load a unitypackage in append mode.
    #[allow(dead_code)]
    fn try_load_unitypackage_for_append(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        // Decide is_temp before the file may disappear (canonicalize requires the file to exist).
        let is_temp =
            is_temp_path(path) || self.preloaded.as_ref().is_some_and(|pl| pl.path == path);
        let archive_data: Arc<[u8]> = self.read_or_preloaded(path)?;

        // Phase 3: build UnityPackageIndex.
        let pkg_index = Arc::new(crate::unitypackage::build_unity_package_index(
            &archive_data,
        )?);
        let assets: Vec<crate::unitypackage::ExtractedAsset> = pkg_index
            .entries
            .iter()
            .map(|e| crate::unitypackage::ExtractedAsset {
                pathname: e.pathname.clone(),
                data: Arc::clone(&e.data),
            })
            .collect();

        // For temp files, snapshot the archive data so reload can find it.
        let snapshot = if is_temp {
            Some(Arc::clone(&archive_data))
        } else {
            None
        };

        let model_list = build_pkg_model_list(&assets);

        if model_list.is_empty() {
            anyhow::bail!(".unitypackage 内に VRM / FBX ファイルが見つかりません");
        }

        if model_list.len() == 1 {
            let (idx, _, model_type) = model_list[0];
            self.pending.pkg_load = Some(PendingPkgModelLoad {
                assets: Arc::new(assets),
                fbx_index: idx,
                model_type,
                source_path: path.to_path_buf(),
                shown: false,
                append: true,
                suppress_tex_match: self.suppress_tex_match,
                archive_snapshot: snapshot,
                nested_archive_source: None,
                pkg_index: Some(pkg_index),
                batch_progress: None,
                skip_anim_check: false,
            });
        } else {
            let checked = vec![false; model_list.len()];
            self.pending.unity_pkg = Some(PendingUnityPackage {
                assets,
                model_list,
                source_path: path.to_path_buf(),
                append: true,
                archive_snapshot: snapshot,
                nested_archive_source: None,
                pkg_index: Some(pkg_index),
                checked,
            });
        }
        Ok(())
    }

    /// Load an archive (ZIP / 7z).
    fn try_load_archive(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        self.try_load_archive_impl(path, false)
    }

    /// Load an archive in append mode.
    #[allow(dead_code)]
    fn try_load_archive_for_append(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        self.try_load_archive_impl(path, true)
    }

    fn try_load_archive_impl(
        &mut self,
        path: &std::path::Path,
        append: bool,
    ) -> anyhow::Result<()> {
        let is_temp =
            is_temp_path(path) || self.preloaded.as_ref().is_some_and(|pl| pl.path == path);
        let archive_data: Arc<[u8]> = self.read_or_preloaded(path)?;

        let ext = crate::path_ext_lower(path);
        let format = crate::archive::archive_format_from_ext(&ext)
            .with_context(|| t!("error.unsupported_archive_format", ext = ext).into_owned())?;

        let contents = crate::archive::list_models(&archive_data, format)?;

        if contents.models.is_empty() {
            anyhow::bail!(t!("error.archive_no_models_found").into_owned());
        }

        if contents.models.len() == 1 {
            // Exactly one model → defer loading.
            self.pending.archive_load = Some(PendingArchiveLoad {
                archive_data,
                format,
                contents,
                model_index: 0,
                source_path: path.to_path_buf(),
                shown: false,
                append,
                is_temp,
            });
        } else {
            // Multiple models → show the selection dialog.
            log::info!("Found {} models in archive:", contents.models.len());
            for (_, p, _, kind) in &contents.models {
                log::info!("  [{}] {}", kind.label(), p.display());
            }
            self.pending.archive = Some(super::pending::PendingArchive {
                archive_data,
                format,
                contents,
                source_path: path.to_path_buf(),
                append,
                is_temp,
            });
        }
        Ok(())
    }

    /// Synchronous fallback for loading a model from an archive
    /// (the normal path is `spawn_bg_archive_load`).
    #[allow(dead_code)]
    pub(super) fn load_model_from_archive(
        &mut self,
        pending: PendingArchiveLoad,
    ) -> anyhow::Result<()> {
        let model_path = pending.contents.models[pending.model_index].1.clone();
        let kind = pending.contents.models[pending.model_index].3;
        log::info!(
            "Load from archive: {:?} [{}] from {}",
            kind,
            model_path.display(),
            pending.source_path.display()
        );

        let bundle = crate::archive::extract_model_bundle(
            &pending.archive_data,
            pending.format,
            pending.contents,
            pending.model_index,
        )?;

        // UnityPackage: double-extract and feed back into the existing unitypackage flow.
        if kind == crate::archive::ArchiveModelKind::UnityPackage {
            return self.load_unitypackage_from_archive(
                bundle.model.data,
                pending.source_path,
                pending.is_temp,
                pending.archive_data,
                pending.append,
                model_path,
            );
        }

        let ir = self.build_ir_from_archive_bundle(&bundle, &pending.source_path)?;

        let source = ReloadableSource::Archive {
            original_path: pending.source_path,
            archive_bytes: if pending.is_temp {
                Some(pending.archive_data)
            } else {
                None
            },
            selected_entry_path: model_path.to_string_lossy().into_owned(),
            inner_kind: kind,
        };

        if pending.append {
            self.finish_append_with_source(ir, source, None);
            Ok(())
        } else {
            self.finish_load(ir, source)
        }
    }

    /// Extract a .unitypackage nested inside an archive and feed it into the existing
    /// unitypackage loading flow (synchronous fallback).
    #[allow(dead_code)]
    fn load_unitypackage_from_archive(
        &mut self,
        pkg_data: Vec<u8>,
        source_path: PathBuf,
        is_temp: bool,
        archive_data: Arc<[u8]>,
        append: bool,
        entry_path: PathBuf,
    ) -> anyhow::Result<()> {
        // Build the UnityPackageIndex (required for Prefab resolution).
        let pkg_index = Arc::new(crate::unitypackage::build_unity_package_index(&pkg_data)?);
        // Also build ExtractedAsset for compatibility with the existing API (shares Arc; no copy).
        let assets: Vec<crate::unitypackage::ExtractedAsset> = pkg_index
            .entries
            .iter()
            .map(|e| crate::unitypackage::ExtractedAsset {
                pathname: e.pathname.clone(),
                data: Arc::clone(&e.data),
            })
            .collect();

        // Detect VRM / FBX entries.
        let model_list = build_pkg_model_list(&assets);
        if model_list.is_empty() {
            anyhow::bail!(".unitypackage 内に VRM / FBX ファイルが見つかりません");
        }

        // Archive snapshot (kept only for temp files).
        let archive_snapshot = if is_temp {
            Some(Arc::clone(&archive_data))
        } else {
            None
        };

        // Information needed to re-extract through Archive on reload.
        let nested_archive_source = Some(ReloadableSource::Archive {
            original_path: source_path.clone(),
            archive_bytes: if is_temp { Some(archive_data) } else { None },
            selected_entry_path: entry_path.to_string_lossy().into_owned(),
            inner_kind: crate::archive::ArchiveModelKind::UnityPackage,
        });

        if model_list.len() == 1 {
            let (model_index, ref _name, model_type) = model_list[0];
            log::info!("Archive .unitypackage: 1 model detected");
            self.pending.pkg_load = Some(PendingPkgModelLoad {
                assets: Arc::new(assets),
                fbx_index: model_index,
                model_type,
                source_path: source_path.clone(),
                shown: false,
                append,
                suppress_tex_match: false,
                archive_snapshot,
                nested_archive_source,
                pkg_index: Some(pkg_index),
                batch_progress: None,
                skip_anim_check: false,
            });
        } else {
            log::info!("Archive .unitypackage: found {} models:", model_list.len());
            for (_, name, mt) in &model_list {
                let label = match mt {
                    PkgModelType::Prefab => "Prefab",
                    PkgModelType::Vrm => "VRM",
                    PkgModelType::Fbx => "FBX",
                };
                log::info!("  [{}] {}", label, name);
            }
            let checked = vec![false; model_list.len()];
            self.pending.unity_pkg = Some(PendingUnityPackage {
                assets,
                model_list,
                source_path: source_path.clone(),
                append,
                archive_snapshot,
                nested_archive_source,
                pkg_index: Some(pkg_index),
                checked,
            });
        }
        Ok(())
    }

    /// Build an IrModel from an archive bundle (delegates to the free function).
    fn build_ir_from_archive_bundle(
        &self,
        bundle: &crate::archive::ModelBundle,
        source_path: &Path,
    ) -> anyhow::Result<IrModel> {
        build_ir_from_archive_bundle_bg(
            bundle,
            source_path,
            self.normalize_pose,
            self.normalize_to_tstance,
        )
    }

    /// Build an IrModel from a `ReloadableSource::Archive` (shared by reload and append).
    fn load_ir_from_archive_source(
        &self,
        original_path: &Path,
        archive_bytes: Option<&Arc<[u8]>>,
        selected_entry_path: &str,
        inner_kind: crate::archive::ArchiveModelKind,
    ) -> anyhow::Result<IrModel> {
        let owned;
        let data: &[u8] = if let Some(snap) = archive_bytes {
            snap
        } else {
            owned = std::fs::read(original_path)?;
            &owned
        };

        let ext = crate::path_ext_lower(original_path);
        let format = crate::archive::archive_format_from_ext(&ext)
            .with_context(|| t!("error.unsupported_archive_format", ext = ext).into_owned())?;

        let contents = crate::archive::list_models(data, format)?;

        // Re-locate the same model by `selected_entry_path`.
        let model_index = contents
            .models
            .iter()
            .position(|(_, p, _, _)| p.to_string_lossy() == selected_entry_path)
            .ok_or_else(|| {
                anyhow::anyhow!(t!(
                    "error.archive_old_model_not_found",
                    path = selected_entry_path
                )
                .into_owned())
            })?;

        let bundle = crate::archive::extract_model_bundle(data, format, contents, model_index)?;
        let _ = inner_kind; // use bundle.kind instead
        self.build_ir_from_archive_bundle(&bundle, original_path)
    }

    /// Extract the .unitypackage payload from inside an archive (ZIP / 7z).
    fn extract_pkg_from_archive(
        original_path: &Path,
        archive_bytes: Option<&Arc<[u8]>>,
        selected_entry_path: &str,
    ) -> anyhow::Result<Vec<u8>> {
        let owned;
        let data: &[u8] = if let Some(snap) = archive_bytes {
            snap
        } else {
            owned = std::fs::read(original_path)?;
            &owned
        };

        let ext = crate::path_ext_lower(original_path);
        let format = crate::archive::archive_format_from_ext(&ext)
            .with_context(|| t!("error.unsupported_archive_format", ext = ext).into_owned())?;

        let contents = crate::archive::list_models(data, format)?;

        let model_index = contents
            .models
            .iter()
            .position(|(_, p, _, _)| p.to_string_lossy() == selected_entry_path)
            .ok_or_else(|| {
                anyhow::anyhow!(t!(
                    "error.archive_old_model_not_found",
                    path = selected_entry_path
                )
                .into_owned())
            })?;

        let bundle = crate::archive::extract_model_bundle(data, format, contents, model_index)?;
        Ok(bundle.model.data)
    }

    /// Synchronous unitypackage append for the reload path
    /// (avoids deferred work and also restores texture assignments).
    fn reload_append_unitypackage(
        &mut self,
        source: &ReloadableSource,
        pkg_model_name: Option<&str>,
        pkg_model: Option<&crate::unitypackage::PkgModelLocator>,
        saved_pkg_tex_assignments: &HashMap<usize, String>,
    ) {
        // Avoid copying when an Arc reference suffices; only allocate Vec on owned paths.
        use std::borrow::Cow;
        let archive_data: Cow<'_, [u8]> = match source {
            ReloadableSource::Snapshot { main_bytes, .. } => Cow::Borrowed(main_bytes),
            ReloadableSource::File(path) => match std::fs::read(path) {
                Ok(d) => Cow::Owned(d),
                Err(e) => {
                    log::error!("Unitypackage reload failed: {e}");
                    return;
                }
            },
            ReloadableSource::Archive {
                original_path,
                archive_bytes,
                selected_entry_path,
                inner_kind,
            } => {
                if *inner_kind == crate::archive::ArchiveModelKind::UnityPackage {
                    // .unitypackage nested in an archive: double-extract.
                    match Self::extract_pkg_from_archive(
                        original_path,
                        archive_bytes.as_ref(),
                        selected_entry_path,
                    ) {
                        Ok(data) => Cow::Owned(data),
                        Err(e) => {
                            log::error!("Archive unitypackage extraction failed: {e}");
                            return;
                        }
                    }
                } else if let Some(snap) = archive_bytes {
                    Cow::Borrowed(snap.as_ref())
                } else {
                    match std::fs::read(original_path) {
                        Ok(d) => Cow::Owned(d),
                        Err(e) => {
                            log::error!("Unitypackage reload failed: {e}");
                            return;
                        }
                    }
                }
            }
        };
        let path = source.display_path();
        // Build pkg_index first, then derive assets from it.
        // (extract_all_assets and build_unity_package_index iterate a HashMap in a
        //  non-deterministic order, so building them independently would mis-align indices.)
        let pkg_index_for_reload =
            match crate::unitypackage::build_unity_package_index(&archive_data) {
                Ok(idx) => Arc::new(idx),
                Err(e) => {
                    log::error!("Unitypackage extraction failed: {e}");
                    return;
                }
            };
        let assets: Vec<crate::unitypackage::ExtractedAsset> = pkg_index_for_reload
            .entries
            .iter()
            .map(|e| crate::unitypackage::ExtractedAsset {
                pathname: e.pathname.clone(),
                data: Arc::clone(&e.data),
            })
            .collect();

        // Match in priority order: pkg_model (GUID/path) → pkg_model_name (basename) → selected_fbx_name (basename).
        let fbx_list = crate::unitypackage::find_fbx_list(&assets);
        let vrm_list = crate::unitypackage::find_vrm_list(&assets);

        // 1. Exact match by GUID / path.
        let found_by_locator = pkg_model.and_then(|loc| {
            crate::unitypackage::find_asset_by_pathname(&assets, &loc.pathname)
                .map(|idx| (idx, loc.kind))
        });

        let (model_index, model_type) = if let Some(found) = found_by_locator {
            found
        } else {
            // 2. Basename fallback.
            let search_name = pkg_model_name.or(self.selected_fbx_name.as_deref());
            if let Some(prev_name) = search_name {
                if let Some((idx, _)) = fbx_list.iter().find(|(_, name)| name == prev_name) {
                    (*idx, PkgModelType::Fbx)
                } else if let Some((idx, _)) = vrm_list.iter().find(|(_, name)| name == prev_name) {
                    (*idx, PkgModelType::Vrm)
                } else if !fbx_list.is_empty() {
                    (fbx_list[0].0, PkgModelType::Fbx)
                } else if !vrm_list.is_empty() {
                    (vrm_list[0].0, PkgModelType::Vrm)
                } else {
                    log::error!("No models found in unitypackage");
                    return;
                }
            } else if !fbx_list.is_empty() {
                (fbx_list[0].0, PkgModelType::Fbx)
            } else if !vrm_list.is_empty() {
                (vrm_list[0].0, PkgModelType::Vrm)
            } else {
                log::error!("No models found in unitypackage");
                return;
            }
        };

        // Record the material offset before merging.
        let mat_offset = self
            .loaded
            .as_ref()
            .map(|l| l.ir.materials.len())
            .unwrap_or(0);

        // Append synchronously.
        // When the source is Archive, pass it through as source_override.
        let source_override = match source {
            ReloadableSource::Archive { .. } => Some(source.clone()),
            ReloadableSource::Snapshot { .. } => Some(source.clone()),
            _ => None,
        };
        // Pass pkg_index for Prefab append (reuses the one built at the top of reload).
        let pkg_index = if model_type == PkgModelType::Prefab {
            Some(pkg_index_for_reload)
        } else {
            None
        };
        let _ok = self.append_from_pkg(
            &assets,
            model_index,
            model_type,
            path,
            source_override,
            pkg_index,
        );

        // Restore pkg-texture assignments for the newly appended materials.
        if !saved_pkg_tex_assignments.is_empty() {
            // Collect the assignments to restore first (so the borrow is released before the loop).
            let assignments_to_restore: Vec<(usize, String, Vec<u8>)> = {
                let pkg_src = self.tex.pkg_textures.as_deref().unwrap_or(&[]);
                let name_to_data: HashMap<&str, &[u8]> = pkg_src
                    .iter()
                    .map(|(name, data)| (name.as_str(), data.as_ref()))
                    .collect();
                let mat_count = self
                    .loaded
                    .as_ref()
                    .map(|l| l.ir.materials.len())
                    .unwrap_or(0);
                saved_pkg_tex_assignments
                    .iter()
                    .filter(|(idx, _)| **idx >= mat_offset && **idx < mat_count)
                    .filter_map(|(idx, tex_name)| {
                        name_to_data
                            .get(tex_name.as_str())
                            .map(|data| (*idx, tex_name.clone(), data.to_vec()))
                    })
                    .collect()
            };
            for (mat_idx, tex_name, data) in &assignments_to_restore {
                if self.assign_texture_data_to_material(*mat_idx, tex_name, data) {
                    self.tex.pkg_assignments.insert(*mat_idx, tex_name.clone());
                } else {
                    // Restoration failed → drop the now-invalid history entry.
                    self.tex.pkg_assignments.remove(mat_idx);
                }
            }
        }
    }

    /// Load the specified FBX from already-extracted assets.
    pub fn load_fbx_from_assets(
        &mut self,
        assets: &[crate::unitypackage::ExtractedAsset],
        fbx_index: usize,
        source_path: &std::path::Path,
        mode: FbxLoadMode,
        source_override: Option<ReloadableSource>,
        pkg_index: Option<&UnityPackageIndex>,
    ) -> anyhow::Result<()> {
        // When pkg_index is provided, use prepare_pkg_fbx + embed_textures_with_prefab.
        let (fbx_data, fbx_name, textures_legacy, pkg_textures_new, _unmatched_precomputed) =
            if let Some(idx) = pkg_index {
                let prepared = crate::unitypackage::prepare_pkg_fbx(idx, fbx_index)?;
                let fbx_name = std::path::Path::new(prepared.model.pathname.as_ref())
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let fbx_data = Arc::clone(&prepared.fbx_data);
                // Convert PackageTexture → (String, Arc<[u8]>) (the legacy pkg_textures shape).
                let legacy_textures: Vec<(String, Arc<[u8]>)> = prepared
                    .textures
                    .iter()
                    .map(|t| (t.display_name.to_string(), Arc::clone(&t.data)))
                    .collect();
                (
                    fbx_data,
                    fbx_name,
                    legacy_textures,
                    Some(prepared),
                    None::<Vec<usize>>,
                )
            } else {
                let (fbx_data, fbx_name, textures) =
                    crate::unitypackage::take_fbx_and_textures(assets, fbx_index)?;
                (fbx_data, fbx_name, textures, None, None)
            };

        log::info!(
            "FBX in unitypackage: {} textures: {}",
            fbx_name,
            textures_legacy.len()
        );
        self.selected_fbx_name = Some(fbx_name.clone());
        self.selected_pkg_model = pkg_textures_new.as_ref().map(|p| p.model.clone());

        let load_model = matches!(mode, FbxLoadMode::ModelOnly | FbxLoadMode::Both);
        let load_animation = matches!(mode, FbxLoadMode::AnimationOnly | FbxLoadMode::Both);

        if load_model {
            // Via unitypackage: pass fbx_path=None to disable nearby-texture search.
            // Textures are embedded via embed_textures_with_prefab / embed_textures_into_ir.
            let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                &fbx_data,
                None,
                self.normalize_pose,
                self.normalize_to_tstance,
            )?;

            // Embed textures: use embed_textures_with_prefab when pkg_index is available.
            let unmatched = if let Some(ref prepared) = pkg_textures_new {
                let prefab_label = format!(
                    "prefab({})",
                    std::path::Path::new(&*prepared.model.pathname)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                );
                crate::unitypackage::embed_textures_with_prefab(
                    &mut ir,
                    &prepared.textures,
                    &prepared.resolved,
                    &prefab_label,
                )
            } else {
                crate::unitypackage::embed_textures_into_ir(&mut ir, &textures_legacy)
            };

            // Keep textures in the app state.
            if !textures_legacy.is_empty() {
                self.tex.pkg_textures = Some(textures_legacy);
                self.rebuild_pkg_thumb_cache();
            }

            // Build pkg_material_keys (only when pkg_index is available).
            let pkg_keys = if let Some(idx) = pkg_index {
                let fbx_guid = idx
                    .entries
                    .get(fbx_index)
                    .map(|e| e.guid.as_str())
                    .unwrap_or("");
                let instance_id = crate::unitypackage::BASE_INSTANCE_ID;
                let model_guid: std::sync::Arc<str> = fbx_guid.into();
                ir.materials
                    .iter()
                    .map(|mat| {
                        Some(crate::unitypackage::PkgMaterialKey {
                            instance_id,
                            model_guid: model_guid.clone(),
                            source_material: mat.source_material.clone(),
                            material_name: mat.name.as_str().into(),
                        })
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };

            let source = source_override
                .unwrap_or_else(|| ReloadableSource::File(source_path.to_path_buf()));
            self.finish_load(ir, source)?;

            // Set pkg_material_keys after finish_load has populated `loaded`.
            if !pkg_keys.is_empty() {
                if let Some(ref mut loaded) = self.loaded {
                    loaded.pkg_material_keys = pkg_keys;
                }
            }

            // Clear any previously bound animation when loading a model.
            self.anim.state = None;
            self.anim.library.clear();
            self.anim.active_index = None;

            // If any materials remain unassigned, open the manual-assign dialog
            // (suppressed during reload).
            if !unmatched.is_empty() && self.tex.pkg_textures.is_some() && !self.suppress_tex_match
            {
                // Restore any existing preview first.
                self.cancel_tex_match_preview();
                let count = unmatched.len();
                self.tex.pending_match = Some(PendingTexMatch {
                    mat_indices: unmatched,
                    selections: vec![None; count],
                    tex_filter: String::new(),
                    previewed: vec![None; count],
                    saved_binds: std::collections::HashMap::new(),
                    texture_views: Vec::new(),
                    failed_uploads: std::collections::HashSet::new(),
                });
            }
        }

        if load_animation {
            if let Ok(anims) = crate::fbx::animation::load_fbx_animation_from_data(&fbx_data) {
                for anim in anims {
                    let display_name = if anim.name == "animation" {
                        fbx_name.clone()
                    } else {
                        format!("{} ({})", fbx_name, anim.name)
                    };
                    let anim = std::sync::Arc::new(anim);
                    if let Some(ref loaded) = self.loaded {
                        let state = AnimationState::new(
                            std::sync::Arc::clone(&anim),
                            &loaded.ir,
                            &loaded.gpu_model,
                        );
                        self.anim
                            .library
                            .push((display_name, source_path.to_path_buf(), anim));
                        self.anim.active_index = Some(self.anim.library.len() - 1);
                        self.anim.state = Some(state);
                    }
                }
            }
        }

        Ok(())
    }

    /// Load the specified VRM from already-extracted assets.
    pub fn load_vrm_from_assets(
        &mut self,
        assets: &[crate::unitypackage::ExtractedAsset],
        vrm_index: usize,
        source_path: &std::path::Path,
        source_override: Option<ReloadableSource>,
    ) -> anyhow::Result<()> {
        // Capture pathname before assets are consumed (needed for accurate reload re-selection).
        let vrm_pathname: Option<String> = assets.get(vrm_index).map(|a| a.pathname.clone());
        let (vrm_data, vrm_name) = crate::unitypackage::take_vrm(assets, vrm_index)?;
        log::info!(
            "VRM in unitypackage: {} ({}KB)",
            vrm_name,
            vrm_data.len() / 1024
        );
        self.selected_fbx_name = Some(vrm_name.clone());
        // VRM doesn't need Prefab texture mapping, but reload still needs pathname to re-pick the model.
        self.selected_pkg_model = vrm_pathname.map(|path| crate::unitypackage::PkgModelLocator {
            guid: "".into(),
            pathname: path.into(),
            kind: crate::unitypackage::PkgModelType::Vrm,
        });

        let glb = vrm::loader::load_glb_from_data(&vrm_data)?;
        let version = vrm::detect::detect_version(&glb.document);
        let all_extensions = vrm::loader::get_raw_extensions(&glb.document);

        let mut ir = vrm::extract::extract_ir_model_with_options(
            &glb.document,
            &glb.buffers,
            &glb.images,
            &glb.vrm_extension,
            &version,
            &all_extensions,
            self.normalize_pose,
        )?;

        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        let mat_count = ir.materials.len();
        let mat_flags = Self::per_mat_or_default_display(&self.material_display, mat_count);
        let gpu_model =
            super::super::mesh::build_gpu_model(&ir, &glb.images, device, queue, &mat_flags)?;

        Self::encode_ir_textures_as_png(&mut ir, &glb.images);
        let source =
            source_override.unwrap_or_else(|| ReloadableSource::File(source_path.to_path_buf()));
        self.finish_load_with_gpu(ir, gpu_model, source, false)
    }

    /// Resolve the referenced FBX(s) from a Prefab entry and load them
    /// (supports merging multiple FBX files into a single model).
    pub fn load_prefab_from_assets(
        &mut self,
        _assets: &[crate::unitypackage::ExtractedAsset],
        prefab_index: usize,
        source_path: &std::path::Path,
        source_override: Option<ReloadableSource>,
        pkg_index: Option<Arc<UnityPackageIndex>>,
    ) -> anyhow::Result<()> {
        let pkg = pkg_index
            .as_ref()
            .context("Prefab ロードには pkg_index が必要です")?;

        // Pull every FBX GUID plus its material-resolution result from the prefab.
        let resolve_result = crate::unitypackage::resolve_single_prefab(pkg, prefab_index)?;

        log::info!(
            "Prefab resolved: {} FBX detected",
            resolve_result.entries.len()
        );

        // Save the prefab filename (used by the file-hierarchy display).
        let prefab_filename = std::path::Path::new(&pkg.entries[prefab_index].pathname)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // Collect textures.
        let textures: Vec<crate::unitypackage::PackageTexture> = pkg
            .entries
            .iter()
            .filter(|e| {
                let lower = e.pathname.to_lowercase();
                [
                    ".png", ".jpg", ".jpeg", ".tga", ".bmp", ".psd", ".tif", ".tiff",
                ]
                .iter()
                .any(|ext| lower.ends_with(ext))
            })
            .map(|e| {
                let display_name = std::path::Path::new(&e.pathname)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                crate::unitypackage::PackageTexture {
                    guid: Arc::from(e.guid.as_str()),
                    display_name: Arc::from(display_name.as_ref()),
                    data: Arc::clone(&e.data),
                    pathname: Arc::from(e.pathname.as_str()),
                }
            })
            .collect();

        // Also build the legacy-shaped texture list (used by pkg_textures).
        let legacy_textures: Vec<(String, Arc<[u8]>)> = textures
            .iter()
            .map(|t| (t.display_name.to_string(), Arc::clone(&t.data)))
            .collect();

        let mut base_ir: Option<crate::intermediate::types::IrModel> = None;
        let mut all_pkg_keys: Vec<Option<crate::unitypackage::PkgMaterialKey>> = Vec::new();
        let mut all_unmatched: Vec<usize> = Vec::new();
        // Track each FBX's material range (used to split MaterialGroup).
        let mut fbx_ranges: Vec<(String, usize, usize)> = Vec::new(); // (name, mat_start, mat_count)

        for (i, fbx_entry_info) in resolve_result.entries.iter().enumerate() {
            let fbx_entry = &pkg.entries[fbx_entry_info.fbx_index];
            let fbx_data = fbx_entry.data.to_vec();
            let fbx_name = std::path::Path::new(&fbx_entry.pathname)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            log::info!(
                "  FBX[{}]: {} (GUID={})",
                i,
                fbx_name,
                fbx_entry_info.fbx_guid
            );

            // Via unitypackage: pass fbx_path=None to disable nearby-texture search.
            let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                &fbx_data,
                None,
                self.normalize_pose,
                self.normalize_to_tstance,
            )?;

            let prefab_label = format!("prefab({})", prefab_filename);
            let unmatched = crate::unitypackage::embed_textures_with_prefab(
                &mut ir,
                &textures,
                &fbx_entry_info.materials,
                &prefab_label,
            );

            // Build pkg_material_keys.
            let instance_id = crate::unitypackage::BASE_INSTANCE_ID;
            let model_guid: Arc<str> = fbx_entry_info.fbx_guid.as_str().into();
            let keys: Vec<_> = ir
                .materials
                .iter()
                .map(|mat| {
                    Some(crate::unitypackage::PkgMaterialKey {
                        instance_id,
                        model_guid: model_guid.clone(),
                        source_material: mat.source_material.clone(),
                        material_name: mat.name.as_str().into(),
                    })
                })
                .collect();

            if let Some(ref mut base) = base_ir {
                // Second and subsequent FBX: merge into the base.
                let mat_offset = base.materials.len();
                let mat_count = ir.materials.len();
                base.merge(ir);
                fbx_ranges.push((fbx_name, mat_offset, mat_count));
                // Offset unmatched indices into the base material space.
                all_unmatched.extend(unmatched.iter().map(|&idx| idx + mat_offset));
                all_pkg_keys.extend(keys);
            } else {
                // First FBX becomes the base model.
                let mat_count = ir.materials.len();
                fbx_ranges.push((fbx_name.clone(), 0, mat_count));
                self.selected_fbx_name = Some(fbx_name);
                self.selected_pkg_model = Some(crate::unitypackage::PkgModelLocator {
                    guid: fbx_entry_info.fbx_guid.as_str().into(),
                    pathname: fbx_entry.pathname.as_str().into(),
                    kind: crate::unitypackage::PkgModelType::Fbx,
                });
                all_unmatched = unmatched;
                all_pkg_keys = keys;
                base_ir = Some(ir);
            }
        }

        let ir = base_ir.context("Prefab に有効な FBX が見つかりません")?;

        // Keep textures in the app state.
        if !legacy_textures.is_empty() {
            self.tex.pkg_textures = Some(legacy_textures);
            self.rebuild_pkg_thumb_cache();
        }

        let source =
            source_override.unwrap_or_else(|| ReloadableSource::File(source_path.to_path_buf()));
        self.finish_load(ir, source)?;

        // After finish_load, apply Prefab metadata and per-FBX MaterialGroup splits.
        if let Some(ref mut loaded) = self.loaded {
            loaded.prefab_name = Some(prefab_filename);
            loaded.prefab_entry_path = Some(pkg.entries[prefab_index].pathname.clone());

            if !all_pkg_keys.is_empty() {
                loaded.pkg_material_keys = all_pkg_keys;
            }

            // When there are multiple FBX, split the single MaterialGroup per FBX.
            if fbx_ranges.len() > 1 {
                let mut new_groups = Vec::with_capacity(fbx_ranges.len());
                for (name, mat_start, mat_count) in &fbx_ranges {
                    let mat_range = *mat_start..*mat_start + *mat_count;
                    // draw_range: locate draws whose material_index falls in this range.
                    let mut draw_start = usize::MAX;
                    let mut draw_end = 0usize;
                    for (di, draw) in loaded.gpu_model.draws.iter().enumerate() {
                        if mat_range.contains(&draw.material_index) {
                            draw_start = draw_start.min(di);
                            draw_end = draw_end.max(di + 1);
                        }
                    }
                    if draw_start == usize::MAX {
                        draw_start = draw_end;
                    }
                    new_groups.push(MaterialGroup {
                        name: name.clone(),
                        material_range: mat_range,
                        draw_range: draw_start..draw_end,
                    });
                }
                loaded.material_groups = new_groups;
            }
        }

        // Now that the Prefab name is fixed, overwrite model_display_name with the prefab name
        // so both the title bar and the PMX output filename pick it up.
        if let Some(ref loaded) = self.loaded {
            if let Some(ref prefab_filename) = loaded.prefab_name {
                let stem = std::path::Path::new(prefab_filename)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy();
                if let Some(sanitized) = crate::sanitize_filename(&stem) {
                    self.export.model_display_name = sanitized;
                }
            }
        }
        self.refresh_derived_from_display_name();

        // Clear any previously bound animation when loading a model.
        self.anim.state = None;
        self.anim.library.clear();
        self.anim.active_index = None;

        // If any materials remain unassigned, open the manual-assign dialog.
        if !all_unmatched.is_empty() && self.tex.pkg_textures.is_some() && !self.suppress_tex_match
        {
            self.cancel_tex_match_preview();
            let count = all_unmatched.len();
            self.tex.pending_match = Some(PendingTexMatch {
                mat_indices: all_unmatched,
                selections: vec![None; count],
                tex_filter: String::new(),
                previewed: vec![None; count],
                saved_binds: std::collections::HashMap::new(),
                texture_views: Vec::new(),
                failed_uploads: std::collections::HashSet::new(),
            });
        }

        Ok(())
    }

    /// Load an animation file based on its extension.
    pub fn load_animation_file(&mut self, path: &std::path::Path) {
        let ext = crate::path_ext_lower(path);
        match ext.as_str() {
            "glb" | "gltf" => self.try_load_gltf_animation(path),
            "fbx" => self.try_load_fbx_animation(path),
            "anim" => self.try_load_unity_animation(path),
            _ => self.try_load_vrma(path),
        }
    }

    pub fn try_load_vrma(&mut self, path: &std::path::Path) {
        if self.loaded.is_none() {
            self.convert_message = Some(ConvertMessage::failure(
                t!("viewer.toast.precondition.vrma_needs_vrm").into_owned(),
            ));
            return;
        }

        match vrm::animation::load_vrma(path) {
            Ok(anim) => {
                let anim = Arc::new(anim);
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let loaded = self.loaded.as_ref().expect("loaded inside is_some branch");
                let state = AnimationState::new(Arc::clone(&anim), &loaded.ir, &loaded.gpu_model);
                log::info!("VRMALoad success: {}", path.display());

                // Add to library (entries with the same path overwrite the existing one).
                let path_buf = path.to_path_buf();
                if let Some(idx) = self
                    .anim
                    .library
                    .iter()
                    .position(|(_, p, _)| p == &path_buf)
                {
                    self.anim.library[idx] = (name.clone(), path_buf, anim);
                    self.anim.active_index = Some(idx);
                } else {
                    self.anim.library.push((name.clone(), path_buf, anim));
                    self.anim.active_index = Some(self.anim.library.len() - 1);
                }

                self.anim.state = Some(state);
                self.convert_message = Some(ConvertMessage::success(
                    t!("viewer.toast.anim.vrma_loaded", name = name).into_owned(),
                ));
            }
            Err(e) => {
                log::error!("VRMALoad failed: {e}");
                self.convert_message = Some(ConvertMessage::failure(
                    t!("viewer.toast.anim.vrma_failed", error = e.to_string()).into_owned(),
                ));
            }
        }
    }

    /// Load animation data from an FBX file.
    pub fn try_load_fbx_animation(&mut self, path: &std::path::Path) {
        if self.loaded.is_none() {
            self.convert_message = Some(ConvertMessage::failure(
                t!("viewer.toast.precondition.anim_needs_model").into_owned(),
            ));
            return;
        }

        let anim_result = match self.read_or_preloaded(path) {
            Ok(data) => crate::fbx::animation::load_fbx_animation_from_data(&data),
            Err(_) => crate::fbx::animation::load_fbx_animation(path),
        };
        match anim_result {
            Ok(anims) if anims.is_empty() => {
                // Empty array → no-op (do not show a success toast either).
                log::debug!("FBX animation: empty (skipped)");
            }
            Ok(anims) => {
                let loaded = self.loaded.as_ref().expect("loaded inside is_some branch");
                let path_buf = path.to_path_buf();
                let file_name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

                for anim in anims {
                    let display_name = if anim.name == "animation" {
                        file_name.clone()
                    } else {
                        format!("{} ({})", file_name, anim.name)
                    };
                    let anim = Arc::new(anim);
                    let state =
                        AnimationState::new(Arc::clone(&anim), &loaded.ir, &loaded.gpu_model);

                    // Add to library.
                    self.anim
                        .library
                        .push((display_name.clone(), path_buf.clone(), anim));
                    self.anim.active_index = Some(self.anim.library.len() - 1);
                    self.anim.state = Some(state);
                }

                log::info!("FBX animation loaded: {}", path.display());
                self.convert_message = Some(ConvertMessage::success(
                    t!("viewer.toast.anim.fbx_loaded", name = file_name).into_owned(),
                ));
            }
            Err(e) => {
                log::warn!("FBX animation load failed: {e}");
                self.convert_message = Some(ConvertMessage::failure(
                    t!("viewer.toast.anim.fbx_failed", error = e.to_string()).into_owned(),
                ));
            }
        }
    }

    /// Load animation data from a Unity .anim file.
    pub fn try_load_unity_animation(&mut self, path: &std::path::Path) {
        if self.loaded.is_none() {
            self.convert_message = Some(ConvertMessage::failure(
                t!("viewer.toast.precondition.anim_needs_model").into_owned(),
            ));
            return;
        }

        match crate::unity::animation::load_unity_anim(path, self.anim.muscle_scale) {
            Ok(anim) => {
                let loaded = self.loaded.as_ref().expect("loaded inside is_some branch");
                let path_buf = path.to_path_buf();
                let file_name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let display_name = format!("{} ({})", file_name, anim.name);
                let anim = Arc::new(anim);
                let state = AnimationState::new(Arc::clone(&anim), &loaded.ir, &loaded.gpu_model);

                self.anim.library.push((display_name, path_buf, anim));
                self.anim.active_index = Some(self.anim.library.len() - 1);
                self.anim.state = Some(state);

                log::info!("Unity .animLoad success: {}", path.display());
                self.convert_message = Some(ConvertMessage::success(
                    t!("viewer.toast.anim.unity_loaded", name = file_name).into_owned(),
                ));
            }
            Err(e) => {
                log::error!("Unity .animLoad failed: {e}");
                self.convert_message = Some(ConvertMessage::failure(
                    t!("viewer.toast.anim.unity_failed", error = e.to_string()).into_owned(),
                ));
            }
        }
    }

    /// Load animation data from a GLB / glTF file.
    pub fn try_load_gltf_animation(&mut self, path: &std::path::Path) {
        if self.loaded.is_none() {
            self.convert_message = Some(ConvertMessage::failure(
                t!("viewer.toast.precondition.anim_needs_model").into_owned(),
            ));
            return;
        }

        match vrm::animation::load_gltf_animation(path) {
            Ok(anims) => {
                let loaded = self.loaded.as_ref().expect("loaded inside is_some branch");
                let path_buf = path.to_path_buf();
                let file_name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

                for anim in anims {
                    let display_name = if anim.name == "animation" {
                        file_name.clone()
                    } else {
                        format!("{} ({})", file_name, anim.name)
                    };
                    let anim = Arc::new(anim);
                    let state =
                        AnimationState::new(Arc::clone(&anim), &loaded.ir, &loaded.gpu_model);

                    // Add to library.
                    self.anim
                        .library
                        .push((display_name.clone(), path_buf.clone(), anim));
                    self.anim.active_index = Some(self.anim.library.len() - 1);
                    self.anim.state = Some(state);
                }

                log::info!("glTF animation loaded: {}", path.display());
                self.convert_message = Some(ConvertMessage::success(
                    t!("viewer.toast.anim.generic_loaded", name = file_name).into_owned(),
                ));
            }
            Err(e) => {
                log::error!("glTF animation load failed: {e}");
                self.convert_message = Some(ConvertMessage::failure(
                    t!("viewer.toast.anim.generic_failed", error = e.to_string()).into_owned(),
                ));
            }
        }
    }

    /// Switch the active VRMA in the library by index.
    pub fn switch_vrma(&mut self, index: usize) {
        if let Some((_, _, ref anim)) = self.anim.library.get(index) {
            if let Some(ref loaded) = self.loaded {
                let state = AnimationState::new(Arc::clone(anim), &loaded.ir, &loaded.gpu_model);
                self.anim.state = Some(state);
                self.anim.active_index = Some(index);
            }
        }
    }

    fn try_load_pmx(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let source = if is_temp_path(path)
            || self.preloaded.as_ref().is_some_and(|pl| pl.path == path)
        {
            let main_data: Arc<[u8]> = self.read_or_preloaded(path)?;
            let pmx_model = crate::pmx::reader::read_pmx_from_data(&main_data)?;
            let pmx_dir = path.parent().unwrap_or(Path::new("."));

            // Gather aux files (textures); prefer preloaded.aux_files.
            let mut aux = HashMap::new();
            let preloaded_aux = self
                .preloaded
                .as_ref()
                .filter(|pl| pl.path == path)
                .map(|pl| &pl.aux_files);
            for tex_path in &pmx_model.textures {
                let normalized = tex_path.replace('\\', "/");
                let key = PathBuf::from(&normalized);
                // Prefer preloaded aux_files when available.
                if let Some(data) = preloaded_aux.and_then(|a| a.get(&key)) {
                    aux.insert(key, Arc::clone(data));
                } else {
                    let full_path = pmx_dir.join(&normalized);
                    if let Ok(data) = std::fs::read(&full_path) {
                        aux.insert(key, Arc::from(data.into_boxed_slice()));
                    }
                }
            }

            let mut ir = crate::pmx::extract::pmx_to_ir_with_aux(&pmx_model, pmx_dir, Some(&aux))?;
            if self.normalize_pose {
                ir.astance_result = crate::intermediate::pose::normalize_pose_to_tstance_full(
                    &mut ir.bones,
                    &mut ir.meshes,
                    &mut ir.morphs,
                    &mut ir.physics,
                    crate::convert::coord::gltf_pos_to_pmx,
                );
            }

            let source = ReloadableSource::Snapshot {
                original_path: path.to_path_buf(),
                main_bytes: main_data,
                aux_files: aux,
            };
            return self.finish_load(ir, source);
        } else {
            ReloadableSource::File(path.to_path_buf())
        };

        let pmx_model = crate::pmx::reader::read_pmx(path)?;
        let pmx_dir = path.parent().unwrap_or(Path::new("."));
        let mut ir = crate::pmx::extract::pmx_to_ir(&pmx_model, pmx_dir)?;

        if self.normalize_pose {
            ir.astance_result = crate::intermediate::pose::normalize_pose_to_tstance_full(
                &mut ir.bones,
                &mut ir.meshes,
                &mut ir.morphs,
                &mut ir.physics,
                crate::convert::coord::gltf_pos_to_pmx,
            );
        }

        self.finish_load(ir, source)
    }

    fn try_load_pmd(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let source =
            if is_temp_path(path) || self.preloaded.as_ref().is_some_and(|pl| pl.path == path) {
                let main_data: Arc<[u8]> = self.read_or_preloaded(path)?;
                let pmd_model = crate::pmd::reader::read_pmd_from_data(&main_data)?;
                let pmd_dir = path.parent().unwrap_or(Path::new("."));

                // Gather aux files (textures + .txt); prefer preloaded.aux_files.
                let mut aux = HashMap::new();
                let preloaded_aux = self
                    .preloaded
                    .as_ref()
                    .filter(|pl| pl.path == path)
                    .map(|pl| &pl.aux_files);
                // Textures.
                for mat in &pmd_model.materials {
                    if mat.texture_name.is_empty() {
                        continue;
                    }
                    let main_tex = mat.texture_name.split('*').next().unwrap_or("");
                    if main_tex.is_empty() {
                        continue;
                    }
                    let normalized = main_tex.replace('\\', "/");
                    let key = PathBuf::from(&normalized);
                    if aux.contains_key(&key) {
                        continue;
                    }
                    // Prefer preloaded aux_files when available.
                    if let Some(data) = preloaded_aux.and_then(|a| a.get(&key)) {
                        aux.insert(key, Arc::clone(data));
                    } else {
                        let full_path = pmd_dir.join(&normalized);
                        if let Ok(data) = std::fs::read(&full_path) {
                            aux.insert(key, Arc::from(data.into_boxed_slice()));
                        }
                    }
                }
                // .txt sidecar file.
                let txt_path = path.with_extension("txt");
                let txt_name = txt_path.file_name().map(PathBuf::from).unwrap_or_default();
                if let Some(data) = preloaded_aux.and_then(|a| a.get(&txt_name)) {
                    aux.insert(txt_name, Arc::clone(data));
                } else if let Ok(data) = std::fs::read(&txt_path) {
                    aux.insert(txt_name, Arc::from(data.into_boxed_slice()));
                }

                let mut ir = crate::pmd::extract::pmd_to_ir_with_aux(&pmd_model, path, Some(&aux))?;
                if self.normalize_pose {
                    ir.astance_result = crate::intermediate::pose::normalize_pose_to_tstance_full(
                        &mut ir.bones,
                        &mut ir.meshes,
                        &mut ir.morphs,
                        &mut ir.physics,
                        crate::convert::coord::gltf_pos_to_pmx,
                    );
                }

                let source = ReloadableSource::Snapshot {
                    original_path: path.to_path_buf(),
                    main_bytes: main_data,
                    aux_files: aux,
                };
                return self.finish_load(ir, source);
            } else {
                ReloadableSource::File(path.to_path_buf())
            };

        let pmd_model = crate::pmd::reader::read_pmd(path)?;
        let mut ir = crate::pmd::extract::pmd_to_ir(&pmd_model, path)?;

        if self.normalize_pose {
            ir.astance_result = crate::intermediate::pose::normalize_pose_to_tstance_full(
                &mut ir.bones,
                &mut ir.meshes,
                &mut ir.morphs,
                &mut ir.physics,
                crate::convert::coord::gltf_pos_to_pmx,
            );
        }

        self.finish_load(ir, source)
    }

    fn try_load_obj(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        self.try_load_obj_with_params(path, 0.01, false)
    }

    fn try_load_obj_with_params(
        &mut self,
        path: &std::path::Path,
        scale: f32,
        z_up: bool,
    ) -> anyhow::Result<()> {
        let source =
            if is_temp_path(path) || self.preloaded.as_ref().is_some_and(|pl| pl.path == path) {
                let main_data: Arc<[u8]> = self.read_or_preloaded(path)?;
                let obj_dir = path.parent().unwrap_or(Path::new("."));
                let aux = self.take_or_collect_aux(path);
                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Model");
                let ir = crate::obj::extract::load_obj_from_data_with_params(
                    &main_data,
                    name,
                    obj_dir,
                    Some(&aux),
                    scale,
                    z_up,
                )?;

                let source = ReloadableSource::Snapshot {
                    original_path: path.to_path_buf(),
                    main_bytes: main_data,
                    aux_files: aux,
                };
                return self.finish_load(ir, source);
            } else {
                ReloadableSource::File(path.to_path_buf())
            };

        let ir = crate::obj::extract::load_obj_with_params(path, scale, z_up)?;
        self.finish_load(ir, source)
    }

    fn try_load_stl(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        self.try_load_stl_with_params(path, 0.001, true)
    }

    fn try_load_stl_with_params(
        &mut self,
        path: &std::path::Path,
        scale: f32,
        z_up: bool,
    ) -> anyhow::Result<()> {
        let source = if is_temp_path(path)
            || self.preloaded.as_ref().is_some_and(|pl| pl.path == path)
        {
            let main_data: Arc<[u8]> = self.read_or_preloaded(path)?;
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Model");
            let ir =
                crate::stl::extract::load_stl_from_data_with_params(&main_data, name, scale, z_up)?;

            let source = ReloadableSource::Snapshot {
                original_path: path.to_path_buf(),
                main_bytes: main_data,
                aux_files: HashMap::new(),
            };
            return self.finish_load(ir, source);
        } else {
            ReloadableSource::File(path.to_path_buf())
        };

        let ir = crate::stl::extract::load_stl_with_params(path, scale, z_up)?;
        self.finish_load(ir, source)
    }

    fn try_load_x(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let source =
            if is_temp_path(path) || self.preloaded.as_ref().is_some_and(|pl| pl.path == path) {
                let main_data: Arc<[u8]> = self.read_or_preloaded(path)?;
                let x_dir = path.parent().unwrap_or(Path::new("."));
                let aux = self.take_or_collect_aux(path);
                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Model");
                let ir =
                    crate::directx::extract::load_x_from_data(&main_data, name, x_dir, Some(&aux))?;

                let source = ReloadableSource::Snapshot {
                    original_path: path.to_path_buf(),
                    main_bytes: main_data,
                    aux_files: aux,
                };
                return self.finish_load(ir, source);
            } else {
                ReloadableSource::File(path.to_path_buf())
            };

        let ir = crate::directx::extract::load_x(path)?;
        self.finish_load(ir, source)
    }

    fn try_load_vrm(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        // .gltf can reference external buffers, so don't snapshot it (limit snapshotting to .glb / .vrm).
        let ext_lower = crate::path_ext_lower(path);
        let source = if (is_temp_path(path)
            || self.preloaded.as_ref().is_some_and(|pl| pl.path == path))
            && ext_lower != "gltf"
        {
            let data: Arc<[u8]> = self.read_or_preloaded(path)?;
            let glb = vrm::loader::load_glb_from_data(&data)?;
            let version = vrm::detect::detect_version(&glb.document);
            let all_extensions = vrm::loader::get_raw_extensions(&glb.document);

            let mut ir = vrm::extract::extract_ir_model_with_options(
                &glb.document,
                &glb.buffers,
                &glb.images,
                &glb.vrm_extension,
                &version,
                &all_extensions,
                self.normalize_pose,
            )?;

            let device = &self.render_state.device;
            let queue = &self.render_state.queue;
            let mc = ir.materials.len();
            let mat_flags = Self::per_mat_or_default_display(&self.material_display, mc);
            let gpu_model =
                super::super::mesh::build_gpu_model(&ir, &glb.images, device, queue, &mat_flags)?;
            Self::encode_ir_textures_as_png(&mut ir, &glb.images);

            let source = ReloadableSource::Snapshot {
                original_path: path.to_path_buf(),
                main_bytes: data,
                aux_files: HashMap::new(),
            };
            return self.finish_load_with_gpu(ir, gpu_model, source, false);
        } else {
            ReloadableSource::File(path.to_path_buf())
        };

        let glb = vrm::loader::load_glb(path)?;
        let version = vrm::detect::detect_version(&glb.document);
        let all_extensions = vrm::loader::get_raw_extensions(&glb.document);

        let mut ir = vrm::extract::extract_ir_model_with_options(
            &glb.document,
            &glb.buffers,
            &glb.images,
            &glb.vrm_extension,
            &version,
            &all_extensions,
            self.normalize_pose,
        )?;

        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        let mc = ir.materials.len();
        let mat_flags = Self::per_mat_or_default_display(&self.material_display, mc);
        let gpu_model =
            super::super::mesh::build_gpu_model(&ir, &glb.images, device, queue, &mat_flags)?;

        // Convert IrTexture to PNG-encoded form so convert_ir_to_pmx can consume it uniformly.
        Self::encode_ir_textures_as_png(&mut ir, &glb.images);

        self.finish_load_with_gpu(ir, gpu_model, source, false)
    }

    /// Snapshot the pre-reload state so we can restore it later.
    fn save_reload_snapshot(&mut self) -> ReloadSnapshot {
        // v0.5.6 (Codex review 0.5.6/01 P1 fix): while UV-morph editing is active,
        // the target morph's weight is pinned at 1.0. If we don't restore the original
        // weight before snapshotting, exit-from-edit gets lost across reload and the
        // morph stays permanently at 1.0. `switch_active_morph(None, ...)` both
        // restores the weight and clears active_morph in one call.
        let was_morph_editing = self.uv_edit.active_morph.is_some();
        self.uv_edit
            .switch_active_morph(None, &mut self.morph_weights);
        // v0.5.6 (Codex review 0.5.6/02 P1 fix): during UV-morph editing, `overrides`
        // holds the displayed value `base + morph offset`. Writing it back to the base
        // UV via `apply_to_ir` after reload would bake the morph offset into the base
        // (and re-enabling the morph would then double-apply the offset). Morph edits
        // are already pushed into the IR by `write_displayed_uv`, and a freshly built
        // IR would lose the overrides anyway. Even on failure-restore where the old IR
        // survives, the IR already carries the edit, so we don't need to restore
        // overrides as base. So during morph-edit reload, just clear all overrides state.
        if was_morph_editing {
            self.uv_edit.overrides.clear();
            self.uv_edit.pristine_uvs.clear();
            self.uv_edit.undo_stack.clear();
            self.uv_edit.redo_stack.clear();
            self.uv_edit.selected.clear();
            log::info!(
                "Reload during UV morph editing: dropped overrides to prevent baking into base UV"
            );
        }
        let appended_models = self
            .loaded
            .as_ref()
            .map(|l| l.appended_models.clone())
            .unwrap_or_default();
        // v0.5.6 (Codex review 0.5.6/03 P1 fix): snapshot every UV-morph offsets from
        // the old IR and re-apply them to the matching morph of the new IR on a successful
        // reload. This prevents unsaved edits made directly to the old IR by
        // `write_displayed_uv` from disappearing across reload. Morphs that weren't
        // edited get an identical-value overwrite (effectively a no-op).
        // v0.5.6 (Codex review 0.5.6/04 P1 fix): switched from HashMap to Vec to handle
        // duplicate UV-morph names (saving every entry in encounter order). The
        // restoration side matches uniquely by `(name, name_en, channel)` plus an
        // unused flag.
        let uv_morph_offsets: UvMorphOffsetsSnapshot = self
            .loaded
            .as_ref()
            .map(|l| {
                l.ir.morphs
                    .iter()
                    .filter_map(|m| {
                        if let crate::intermediate::types::IrMorphKind::Uv { channel, offsets } =
                            &m.kind
                        {
                            Some(UvMorphOffsetEntry {
                                name: m.name.clone(),
                                name_en: m.name_en.clone(),
                                channel: *channel,
                                offsets: offsets.clone(),
                            })
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        ReloadSnapshot {
            appended_models,
            camera: self.camera.clone(),
            morph_weights: std::mem::take(&mut self.morph_weights),
            material_visibility: std::mem::take(&mut self.material_visibility),
            material_display: std::mem::take(&mut self.material_display),
            material_filter: std::mem::take(&mut self.material_filter),
            pmx_output_path: std::mem::take(&mut self.export.pmx_output_path),
            model_display_name: std::mem::take(&mut self.export.model_display_name),
            export_visible_only: self.export.export_visible_only,
            side_panel_tab: self.side_panel_tab,
            tex_assignments: std::mem::take(&mut self.tex.assignments),
            pkg_tex_assignments: std::mem::take(&mut self.tex.pkg_assignments),
            pkg_textures: self.tex.pkg_textures.take(),
            vrma_library: std::mem::take(&mut self.anim.library),
            vrma_active_index: self.anim.active_index.take(),
            display: self.display.clone(),
            // v0.5.5: stash the current overrides so they can be re-applied after a new IR is built on reload.
            uv_edit_overrides: std::mem::take(&mut self.uv_edit.overrides),
            uv_edit_active_material: self.uv_edit.active_material,
            uv_edit_window_open: self.uv_edit_window_open,
            uv_morph_offsets,
        }
    }

    /// Restore state from the snapshot when a reload fails.
    /// The old model stays loaded, so write back every field that `save_reload_snapshot` captured.
    pub(super) fn restore_snapshot_on_failure(&mut self, snap: ReloadSnapshot) {
        self.camera = snap.camera;
        self.morph_weights = snap.morph_weights;
        self.morph_dirty = true;
        self.material_visibility = snap.material_visibility;
        self.material_display = snap.material_display;
        self.material_filter = snap.material_filter;
        self.export.pmx_output_path = snap.pmx_output_path;
        self.export.model_display_name = snap.model_display_name;
        self.export.export_visible_only = snap.export_visible_only;
        // When a reload triggered from e.g. the Export tab fails before finish_load_with_gpu,
        // side_panel_tab hasn't been touched, but restore it anyway to cover partially-progressed runs.
        self.side_panel_tab = snap.side_panel_tab;
        if let Some(pkg) = snap.pkg_textures {
            self.tex.pkg_textures = Some(pkg);
        }
        self.tex.assignments = snap.tex_assignments;
        self.tex.pkg_assignments = snap.pkg_tex_assignments;
        self.anim.library = snap.vrma_library;
        self.anim.active_index = snap.vrma_active_index;
        self.display = snap.display;
        // v0.5.5: on reload failure the old model is still loaded, so write overrides back.
        // The old IR's per-vertex UV is still inside the old model, so re-call apply_to_ir to keep them in sync.
        self.uv_edit.overrides = snap.uv_edit_overrides;
        self.uv_edit.active_material = snap.uv_edit_active_material;
        self.uv_edit_window_open = snap.uv_edit_window_open;
        if let Some(loaded) = self.loaded.as_mut() {
            self.uv_edit.apply_to_ir(&mut loaded.ir);
        }
        self.suppress_tex_match = false;
    }

    /// Restore state from the snapshot after a successful reload.
    fn restore_snapshot_on_success(&mut self, snap: ReloadSnapshot) {
        // Restore pkg_textures.
        if self.tex.pkg_textures.is_none() {
            self.tex.pkg_textures = snap.pkg_textures;
        }

        // Restore the camera (reload should never reset the camera).
        self.camera = snap.camera;
        self.pending.refit = false;

        // Only restore morph weights when the morph count matches.
        if snap.morph_weights.len() == self.morph_weights.len() {
            self.morph_weights = snap.morph_weights;
            self.morph_dirty = true;
        }
        // Only restore material flags when the material count matches.
        if snap.material_visibility.len() == self.material_visibility.len() {
            self.material_visibility = snap.material_visibility;
        }
        if snap.material_display.len() == self.material_display.len() {
            self.material_display = snap.material_display;
        }
        // If any per-material flags were restored, rebuild the GPU model to apply them.
        if self.material_display.iter().any(|d| d.smooth_normals)
            || self.material_display.iter().any(|d| d.clear_normals)
            || self.material_display.iter().any(|d| !d.normal_map)
            || self.material_display.iter().any(|d| !d.emissive)
        {
            self.pending.rebuild = Some(PendingOverlay::WaitingOverlay);
        }
        self.material_filter = snap.material_filter;
        self.export.pmx_output_path = snap.pmx_output_path;
        self.export.model_display_name = snap.model_display_name;
        self.export.export_visible_only = snap.export_visible_only;
        // Reflect the restored model_display_name into the title bar.
        self.refresh_derived_from_display_name();
        // Restore the side-panel tab the user had open before reload.
        // (finish_load_with_gpu treats reload like a fresh load and resets to Info, so we overwrite that.)
        self.side_panel_tab = snap.side_panel_tab;

        // Restore texture assignments (file-path entries only; pkg entries are handled inside reload_unitypackage).
        let saved_link = self.tex.link_same_name;
        self.tex.link_same_name = false;
        self.tex.assignments = HashMap::new();
        let current_mat_count = self
            .loaded
            .as_ref()
            .map(|l| l.ir.materials.len())
            .unwrap_or(0);
        for (mat_idx, tex_src) in &snap.tex_assignments {
            if *mat_idx < current_mat_count {
                self.assign_texture_source_to_material(*mat_idx, tex_src);
            }
        }
        self.tex.link_same_name = saved_link;

        // Restore the VRMA library and rebuild the active animation.
        if !snap.vrma_library.is_empty() {
            self.anim.library = snap.vrma_library;
            if let Some(idx) = snap.vrma_active_index {
                self.switch_vrma(idx);
            }
        }
        // Restore display settings (shader overrides, lights, bloom, etc.).
        self.display = snap.display;
        // v0.5.5: restore per-vertex UV edit overrides — write back into IR and re-sync the GPU vertex buffer.
        // We overwrite right after `finish_load_with_gpu` calls `reset()`, so this is the real restoration point.
        // When the new IR's mesh/vertex counts have changed, `apply_to_ir` silently skips out-of-range entries.
        self.uv_edit.overrides = snap.uv_edit_overrides;
        self.uv_edit.active_material = snap.uv_edit_active_material;
        self.uv_edit_window_open = snap.uv_edit_window_open;
        let queue = self.render_state.queue.clone();
        if let Some(loaded) = self.loaded.as_mut() {
            self.uv_edit.apply_to_ir(&mut loaded.ir);
            // v0.5.6 (Codex review 0.5.6/03 P1 fix): write the UV-morph offsets edited in
            // the old IR back into the matching morph of the new IR. Match preferentially
            // on `name_en`, falling back to `name`. This preserves unsaved UV-morph edits
            // (those that were written directly into the old IR by write_displayed_uv).
            // Unedited morphs get an identical-value overwrite (no-op).
            // v0.5.6 (Codex review 0.5.6/04 P1 fix): when multiple UV morphs share a name,
            // restore the N-th correctly by tracking an unused flag plus exact match.
            // Match condition: both `name` and `name_en` match, `channel` matches, entry unused.
            // The old HashMap approach lost one side's edits whenever names collided.
            if !snap.uv_morph_offsets.is_empty() {
                let mut used = vec![false; snap.uv_morph_offsets.len()];
                let mut restored = 0usize;
                for morph in loaded.ir.morphs.iter_mut() {
                    if let crate::intermediate::types::IrMorphKind::Uv { channel, offsets } =
                        &mut morph.kind
                    {
                        for (idx, entry) in snap.uv_morph_offsets.iter().enumerate() {
                            if !used[idx]
                                && entry.name == morph.name
                                && entry.name_en == morph.name_en
                                && entry.channel == *channel
                            {
                                *offsets = entry.offsets.clone();
                                used[idx] = true;
                                restored += 1;
                                break;
                            }
                        }
                    }
                }
                if restored > 0 {
                    log::info!(
                        "UV morph offsets restored: {} entries (preserved across reload)",
                        restored
                    );
                }
                let unmatched = used.iter().filter(|&&u| !u).count();
                if unmatched > 0 {
                    log::warn!(
                        "UV morph offsets snapshot: {} entries dropped (no exact match for name/name_en/channel in new IR)",
                        unmatched
                    );
                }
            }
            loaded.gpu_model.sync_uvs_from_ir(&loaded.ir, &queue);
        }
        // Reload complete: lift the texture-picker suppression.
        self.suppress_tex_match = false;
    }

    /// Reload the currently-loaded VRM (e.g., after option changes).
    /// State such as camera, morphs, and material visibility is preserved.
    pub fn reload_current(&mut self) {
        if self.loaded.is_none() {
            return;
        }
        // Restore the preview before reload (while the old model's GPU resources are still valid).
        self.cancel_tex_match_preview();
        // Suppress the texture-picker dialog during reload.
        self.suppress_tex_match = true;
        let Some(loaded) = self.loaded.as_ref() else {
            return;
        };
        let source = loaded.source.clone();

        // Snapshot the current state for restoration.
        let snap = self.save_reload_snapshot();

        let ext = source.extension_lower();

        // Archive / UnityPackage: keep the synchronous path (state management is too complex otherwise).
        match &source {
            ReloadableSource::Archive {
                inner_kind,
                original_path,
                archive_bytes,
                selected_entry_path,
            } if *inner_kind == crate::archive::ArchiveModelKind::UnityPackage => {
                let result = self.reload_archive_unitypackage(
                    original_path,
                    archive_bytes.as_ref(),
                    selected_entry_path,
                    &source,
                    &snap.pkg_textures,
                    &snap.pkg_tex_assignments,
                );
                self.finish_reload_sync(result, snap);
                return;
            }
            _ if ext == "unitypackage" => {
                let result = self.reload_unitypackage(
                    &source,
                    &snap.pkg_textures,
                    &snap.pkg_tex_assignments,
                );
                self.finish_reload_sync(result, snap);
                return;
            }
            _ => {}
        }

        // File / Snapshot: non-blocking reload via the BG pipeline.
        let (path, preloaded) = match &source {
            ReloadableSource::File(path) => (path.clone(), None),
            ReloadableSource::Snapshot {
                original_path,
                main_bytes,
                aux_files,
            } => {
                let preloaded = Some(PreloadedData {
                    path: original_path.clone(),
                    main_bytes: Arc::clone(main_bytes),
                    aux_files: aux_files.clone(),
                });
                (original_path.clone(), preloaded)
            }
            ReloadableSource::Archive { .. } => {
                // Archive (non-UnityPackage) takes the synchronous fallback.
                let result = self.reload_from_source(&source);
                self.finish_reload_sync(result, snap);
                return;
            }
        };

        // Stash the snapshot for restoration after the BG load completes.
        self.reload_snapshot = Some(snap);

        // Hand off to the existing BG pipeline.
        // is_reload: true keeps user-set flags (normalize_pose, etc.) intact.
        self.pending
            .bg_state
            .submit_dispatch(super::pending::PendingLoadDispatch {
                path,
                append: false,
                overlay: super::pending::PendingOverlay::WaitingOverlay,
                preloaded,
                is_reload: true,
            });
    }

    /// Finalisation for the synchronous reload path (Archive / UnityPackage).
    fn finish_reload_sync(&mut self, result: anyhow::Result<()>, snap: ReloadSnapshot) {
        if let Err(e) = result {
            log::error!("Reload failed: {e}");
            self.convert_message = Some(ConvertMessage::failure(
                t!("viewer.toast.reload.failed", error = e.to_string()).into_owned(),
            ));
            self.restore_snapshot_on_failure(snap);
            return;
        }
        self.remerge_appended_models(&snap);
        self.restore_snapshot_on_success(snap);
    }

    /// Snapshot restoration after the BG reload (called once the GPU build completes).
    pub(super) fn finish_reload_from_snapshot(&mut self) {
        let Some(snap) = self.reload_snapshot.take() else {
            return;
        };
        self.remerge_appended_models(&snap);
        self.restore_snapshot_on_success(snap);
    }

    /// Re-merge the previously appended models (run after reload completes).
    fn remerge_appended_models(&mut self, snap: &ReloadSnapshot) {
        let has_appended = self
            .loaded
            .as_ref()
            .is_some_and(|l| l.appended_models.is_empty())
            && !snap.appended_models.is_empty();
        if !has_appended {
            return;
        }
        self.suppress_tex_match = true;
        for appended in &snap.appended_models {
            match &appended.source {
                ReloadableSource::Archive { inner_kind, .. }
                    if *inner_kind == crate::archive::ArchiveModelKind::UnityPackage =>
                {
                    self.reload_append_unitypackage(
                        &appended.source,
                        appended.pkg_model_name.as_deref(),
                        appended.pkg_model.as_ref(),
                        &snap.pkg_tex_assignments,
                    );
                }
                _ if appended.source.extension_lower() == "unitypackage" => {
                    self.reload_append_unitypackage(
                        &appended.source,
                        appended.pkg_model_name.as_deref(),
                        appended.pkg_model.as_ref(),
                        &snap.pkg_tex_assignments,
                    );
                }
                _ => {
                    self.append_model_from_source(
                        &appended.source,
                        appended.pkg_model_name.as_deref(),
                        appended.pkg_model.as_ref(),
                    );
                }
            }
        }
        self.suppress_tex_match = false;
        self.cancel_tex_match_preview();
        if let Some(ref msg) = self.convert_message {
            if matches!(
                msg.result,
                ConvertResult::Success(_) | ConvertResult::Warning(_)
            ) {
                self.convert_message = None;
            }
        }
    }

    /// Reload a model from a `ReloadableSource` (bypasses the UI branching in `load_file`).
    fn reload_from_source(&mut self, source: &ReloadableSource) -> anyhow::Result<()> {
        // Match on `source` by reference and clone exactly once just before finish_load.
        // (Eliminates the previous double-clone from `source_clone + source_clone.clone()`.)
        let result: anyhow::Result<()> = (|| {
            match source {
                ReloadableSource::File(path) => {
                    let ext = crate::path_ext_lower(path);
                    match detect_format(&ext) {
                        FileFormat::Fbx => self.try_load_fbx(path),
                        FileFormat::Pmx => self.try_load_pmx(path),
                        FileFormat::Pmd => self.try_load_pmd(path),
                        FileFormat::Obj => self.try_load_obj(path),
                        FileFormat::Stl => self.try_load_stl(path),
                        FileFormat::DirectX => self.try_load_x(path),
                        _ => self.try_load_vrm(path),
                    }
                }
                ReloadableSource::Snapshot {
                    original_path,
                    main_bytes,
                    aux_files,
                } => {
                    let ext = crate::path_ext_lower(original_path);
                    match detect_format(&ext) {
                        FileFormat::Fbx => {
                            // When external textures exist, materialise them under a unique temp directory
                            // (auto-deleted by TempDir's Drop). A fixed name would collide between concurrent
                            // BG loads, so we use tempfile to generate a fresh unique name each time.
                            let temp_dir = if !aux_files.is_empty() {
                                let td = tempfile::Builder::new()
                                    .prefix("popone_fbx_reload_")
                                    .tempdir()?;
                                for (rel_path, data) in aux_files {
                                    let target = td.path().join(rel_path);
                                    if let Some(parent) = target.parent() {
                                        std::fs::create_dir_all(parent)?;
                                    }
                                    std::fs::write(&target, data.as_ref())?;
                                }
                                Some(td)
                            } else {
                                None
                            };
                            let fbx_path = temp_dir
                                .as_ref()
                                .map(|d| {
                                    d.path().join(original_path.file_name().unwrap_or_default())
                                })
                                .unwrap_or_else(|| original_path.clone());
                            let ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                                main_bytes,
                                Some(&fbx_path),
                                self.normalize_pose,
                                self.normalize_to_tstance,
                            )?;
                            // temp_dir goes out of scope here → TempDir::drop auto-deletes the directory.
                            drop(temp_dir);
                            self.finish_load(ir, source.clone())
                        }
                        FileFormat::Pmx => {
                            let pmx_model = crate::pmx::reader::read_pmx_from_data(main_bytes)?;
                            let pmx_dir = original_path.parent().unwrap_or(Path::new("."));
                            let mut ir = crate::pmx::extract::pmx_to_ir_with_aux(
                                &pmx_model,
                                pmx_dir,
                                Some(aux_files),
                            )?;
                            if self.normalize_pose {
                                ir.astance_result =
                                    crate::intermediate::pose::normalize_pose_to_tstance_full(
                                        &mut ir.bones,
                                        &mut ir.meshes,
                                        &mut ir.morphs,
                                        &mut ir.physics,
                                        crate::convert::coord::gltf_pos_to_pmx,
                                    );
                            }
                            self.finish_load(ir, source.clone())
                        }
                        FileFormat::Pmd => {
                            let pmd_model = crate::pmd::reader::read_pmd_from_data(main_bytes)?;
                            let mut ir = crate::pmd::extract::pmd_to_ir_with_aux(
                                &pmd_model,
                                original_path,
                                Some(aux_files),
                            )?;
                            if self.normalize_pose {
                                ir.astance_result =
                                    crate::intermediate::pose::normalize_pose_to_tstance_full(
                                        &mut ir.bones,
                                        &mut ir.meshes,
                                        &mut ir.morphs,
                                        &mut ir.physics,
                                        crate::convert::coord::gltf_pos_to_pmx,
                                    );
                            }
                            self.finish_load(ir, source.clone())
                        }
                        FileFormat::Obj => {
                            let obj_dir = original_path.parent().unwrap_or(Path::new("."));
                            let name = original_path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("Model");
                            let ir = crate::obj::extract::load_obj_from_data(
                                main_bytes,
                                name,
                                obj_dir,
                                Some(aux_files),
                            )?;
                            self.finish_load(ir, source.clone())
                        }
                        FileFormat::Stl => {
                            let name = original_path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("Model");
                            let ir = crate::stl::extract::load_stl_from_data(main_bytes, name)?;
                            self.finish_load(ir, source.clone())
                        }
                        FileFormat::DirectX => {
                            let x_dir = original_path.parent().unwrap_or(Path::new("."));
                            let name = original_path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("Model");
                            let ir = crate::directx::extract::load_x_from_data(
                                main_bytes,
                                name,
                                x_dir,
                                Some(aux_files),
                            )?;
                            self.finish_load(ir, source.clone())
                        }
                        _ => {
                            // VRM
                            let glb = vrm::loader::load_glb_from_data(main_bytes)?;
                            let version = vrm::detect::detect_version(&glb.document);
                            let all_extensions = vrm::loader::get_raw_extensions(&glb.document);
                            let mut ir = vrm::extract::extract_ir_model_with_options(
                                &glb.document,
                                &glb.buffers,
                                &glb.images,
                                &glb.vrm_extension,
                                &version,
                                &all_extensions,
                                self.normalize_pose,
                            )?;
                            let device = &self.render_state.device;
                            let queue = &self.render_state.queue;
                            let mc = ir.materials.len();
                            let mat_flags =
                                Self::per_mat_or_default_display(&self.material_display, mc);
                            let gpu_model = super::super::mesh::build_gpu_model(
                                &ir,
                                &glb.images,
                                device,
                                queue,
                                &mat_flags,
                            )?;
                            Self::encode_ir_textures_as_png(&mut ir, &glb.images);
                            self.finish_load_with_gpu(ir, gpu_model, source.clone(), false)
                        }
                    }
                }
                ReloadableSource::Archive {
                    original_path,
                    archive_bytes,
                    selected_entry_path,
                    inner_kind,
                } => {
                    if *inner_kind == crate::archive::ArchiveModelKind::UnityPackage {
                        // .unitypackage nested in an archive: double-extract for reload.
                        // (reload_current passes its own saved_pkg_textures / assignments,
                        //  so here we just use empty defaults.)
                        return self.reload_archive_unitypackage(
                            original_path,
                            archive_bytes.as_ref(),
                            selected_entry_path,
                            source,
                            &None,
                            &HashMap::new(),
                        );
                    }
                    let ir = self.load_ir_from_archive_source(
                        original_path,
                        archive_bytes.as_ref(),
                        selected_entry_path,
                        *inner_kind,
                    )?;
                    self.finish_load(ir, source.clone())
                }
            }
        })();
        if let Err(ref e) = result {
            log::error!("reload_from_source failed: {e}");
            self.convert_message = Some(ConvertMessage::failure(
                t!("viewer.toast.reload.failed", error = e.to_string()).into_owned(),
            ));
        }
        result
    }

    /// Load an appended model from a `ReloadableSource` (used during reload).
    fn append_model_from_source(
        &mut self,
        source: &ReloadableSource,
        pkg_model_name: Option<&str>,
        pkg_model: Option<&crate::unitypackage::PkgModelLocator>,
    ) {
        // .unitypackage nested inside an archive takes its own dedicated path.
        if let ReloadableSource::Archive { inner_kind, .. } = source {
            if *inner_kind == crate::archive::ArchiveModelKind::UnityPackage {
                self.reload_append_unitypackage(source, pkg_model_name, pkg_model, &HashMap::new());
                return;
            }
        }

        let source_clone = source.clone();
        let ir_result: anyhow::Result<IrModel> = (|| -> anyhow::Result<IrModel> {
            match &source_clone {
                ReloadableSource::File(path) => {
                    let ext = crate::path_ext_lower(path);
                    match detect_format(&ext) {
                        FileFormat::Fbx => {
                            let ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                                &std::fs::read(path)?,
                                Some(path),
                                self.normalize_pose,
                                self.normalize_to_tstance,
                            )?;
                            Ok(ir)
                        }
                        FileFormat::Pmx => {
                            let pmx_model = crate::pmx::reader::read_pmx(path)?;
                            let pmx_dir = path.parent().unwrap_or(Path::new("."));
                            let mut ir = crate::pmx::extract::pmx_to_ir(&pmx_model, pmx_dir)?;
                            if self.normalize_pose {
                                ir.astance_result =
                                    crate::intermediate::pose::normalize_pose_to_tstance_full(
                                        &mut ir.bones,
                                        &mut ir.meshes,
                                        &mut ir.morphs,
                                        &mut ir.physics,
                                        crate::convert::coord::gltf_pos_to_pmx,
                                    );
                            }
                            Ok(ir)
                        }
                        FileFormat::Pmd => {
                            let pmd_model = crate::pmd::reader::read_pmd(path)?;
                            let mut ir = crate::pmd::extract::pmd_to_ir(&pmd_model, path)?;
                            if self.normalize_pose {
                                ir.astance_result =
                                    crate::intermediate::pose::normalize_pose_to_tstance_full(
                                        &mut ir.bones,
                                        &mut ir.meshes,
                                        &mut ir.morphs,
                                        &mut ir.physics,
                                        crate::convert::coord::gltf_pos_to_pmx,
                                    );
                            }
                            Ok(ir)
                        }
                        FileFormat::Obj => Ok(crate::obj::extract::load_obj(path)?),
                        FileFormat::Stl => Ok(crate::stl::extract::load_stl(path)?),
                        FileFormat::DirectX => Ok(crate::directx::extract::load_x(path)?),
                        _ => self.load_vrm_as_ir(path),
                    }
                }
                ReloadableSource::Snapshot {
                    original_path,
                    main_bytes,
                    aux_files,
                } => {
                    let ext = crate::path_ext_lower(original_path);
                    match detect_format(&ext) {
                        FileFormat::Fbx => {
                            // A fixed name would collide between concurrent BG loads, so use tempfile to generate a fresh unique name each time.
                            let temp_dir = if !aux_files.is_empty() {
                                let td = tempfile::Builder::new()
                                    .prefix("popone_fbx_reload_")
                                    .tempdir()?;
                                for (rel_path, data) in aux_files {
                                    let target = td.path().join(rel_path);
                                    if let Some(parent) = target.parent() {
                                        std::fs::create_dir_all(parent)?;
                                    }
                                    std::fs::write(&target, data.as_ref())?;
                                }
                                Some(td)
                            } else {
                                None
                            };
                            let fbx_path = temp_dir
                                .as_ref()
                                .map(|d| {
                                    d.path().join(original_path.file_name().unwrap_or_default())
                                })
                                .unwrap_or_else(|| original_path.clone());
                            let ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                                main_bytes,
                                Some(&fbx_path),
                                self.normalize_pose,
                                self.normalize_to_tstance,
                            )?;
                            drop(temp_dir); // TempDir::drop auto-deletes the directory.
                            Ok(ir)
                        }
                        FileFormat::Pmx => {
                            let pmx_model = crate::pmx::reader::read_pmx_from_data(main_bytes)?;
                            let pmx_dir = original_path.parent().unwrap_or(Path::new("."));
                            let mut ir = crate::pmx::extract::pmx_to_ir_with_aux(
                                &pmx_model,
                                pmx_dir,
                                Some(aux_files),
                            )?;
                            if self.normalize_pose {
                                ir.astance_result =
                                    crate::intermediate::pose::normalize_pose_to_tstance_full(
                                        &mut ir.bones,
                                        &mut ir.meshes,
                                        &mut ir.morphs,
                                        &mut ir.physics,
                                        crate::convert::coord::gltf_pos_to_pmx,
                                    );
                            }
                            Ok(ir)
                        }
                        FileFormat::Pmd => {
                            let pmd_model = crate::pmd::reader::read_pmd_from_data(main_bytes)?;
                            let mut ir = crate::pmd::extract::pmd_to_ir_with_aux(
                                &pmd_model,
                                original_path,
                                Some(aux_files),
                            )?;
                            if self.normalize_pose {
                                ir.astance_result =
                                    crate::intermediate::pose::normalize_pose_to_tstance_full(
                                        &mut ir.bones,
                                        &mut ir.meshes,
                                        &mut ir.morphs,
                                        &mut ir.physics,
                                        crate::convert::coord::gltf_pos_to_pmx,
                                    );
                            }
                            Ok(ir)
                        }
                        FileFormat::Obj => {
                            let obj_dir = original_path.parent().unwrap_or(Path::new("."));
                            let name = original_path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("Model");
                            Ok(crate::obj::extract::load_obj_from_data(
                                main_bytes,
                                name,
                                obj_dir,
                                Some(aux_files),
                            )?)
                        }
                        FileFormat::Stl => {
                            let name = original_path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("Model");
                            Ok(crate::stl::extract::load_stl_from_data(main_bytes, name)?)
                        }
                        FileFormat::DirectX => {
                            let x_dir = original_path.parent().unwrap_or(Path::new("."));
                            let name = original_path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("Model");
                            Ok(crate::directx::extract::load_x_from_data(
                                main_bytes,
                                name,
                                x_dir,
                                Some(aux_files),
                            )?)
                        }
                        _ => {
                            let glb = vrm::loader::load_glb_from_data(main_bytes)?;
                            let version = vrm::detect::detect_version(&glb.document);
                            let all_extensions = vrm::loader::get_raw_extensions(&glb.document);
                            let mut ir = vrm::extract::extract_ir_model_with_options(
                                &glb.document,
                                &glb.buffers,
                                &glb.images,
                                &glb.vrm_extension,
                                &version,
                                &all_extensions,
                                self.normalize_pose,
                            )?;
                            Self::encode_ir_textures_as_png(&mut ir, &glb.images);
                            Ok(ir)
                        }
                    }
                }
                ReloadableSource::Archive {
                    original_path,
                    archive_bytes,
                    selected_entry_path,
                    inner_kind,
                } => {
                    // UnityPackage was handled earlier; we never reach here for it.
                    self.load_ir_from_archive_source(
                        original_path,
                        archive_bytes.as_ref(),
                        selected_entry_path,
                        *inner_kind,
                    )
                }
            }
        })();

        match ir_result {
            Ok(other_ir) => {
                if let Some(ref loaded) = self.loaded {
                    let host_fmt = loaded.ir.source_format;
                    let other_fmt = other_ir.source_format;
                    if host_fmt.is_vrm0() != other_fmt.is_vrm0() {
                        log::warn!(
                            "Appending model with different coordinate system: {} + {}",
                            host_fmt.label(),
                            other_fmt.label()
                        );
                        return;
                    }
                }
                self.finish_append_with_source(
                    other_ir,
                    source.clone(),
                    pkg_model_name.map(|s| s.to_string()),
                );
            }
            Err(e) => {
                log::error!("Additional model reload failed: {e}");
            }
        }
    }

    /// Reload a unitypackage (re-extract the FBX / VRM and restore texture assignments).
    #[allow(clippy::type_complexity)]
    fn reload_unitypackage(
        &mut self,
        source: &ReloadableSource,
        saved_pkg_textures: &Option<Vec<(String, Arc<[u8]>)>>,
        saved_pkg_tex_assignments: &HashMap<usize, String>,
    ) -> anyhow::Result<()> {
        use std::borrow::Cow;
        let (archive_data, snapshot): (Cow<'_, [u8]>, Option<Arc<[u8]>>) = match source {
            ReloadableSource::Snapshot { main_bytes, .. } => {
                (Cow::Borrowed(main_bytes), Some(Arc::clone(main_bytes)))
            }
            ReloadableSource::File(path) => {
                let data = std::fs::read(path)?;
                (Cow::Owned(data), None)
            }
            ReloadableSource::Archive {
                original_path,
                archive_bytes,
                ..
            } => {
                if let Some(snap) = archive_bytes {
                    (Cow::Borrowed(snap), Some(Arc::clone(snap)))
                } else {
                    let data = std::fs::read(original_path)?;
                    (Cow::Owned(data), None)
                }
            }
        };
        let path = source.display_path();

        // For Prefab models, reload via the Prefab path.
        if let Some(prefab_path) = self
            .loaded
            .as_ref()
            .and_then(|l| l.prefab_entry_path.clone())
        {
            return self.reload_as_prefab(
                &archive_data,
                snapshot,
                path,
                &prefab_path,
                source,
                saved_pkg_textures,
                saved_pkg_tex_assignments,
            );
        }

        let assets = crate::unitypackage::extract_all_assets(&archive_data)?;

        // If the current model is a VRM, reload it as VRM.
        let is_vrm = self.loaded.as_ref().is_some_and(|l| {
            !matches!(
                l.ir.source_format,
                crate::intermediate::types::SourceFormat::Fbx
            )
        });

        if is_vrm {
            let vrm_list = crate::unitypackage::find_vrm_list(&assets);
            if vrm_list.is_empty() {
                anyhow::bail!(".unitypackage 内に VRM ファイルが見つかりません");
            }
            // GUID / path match → basename fallback.
            let vrm_idx = self
                .selected_pkg_model
                .as_ref()
                .and_then(|loc| crate::unitypackage::find_asset_by_pathname(&assets, &loc.pathname))
                .or_else(|| {
                    self.selected_fbx_name.as_ref().and_then(|prev_name| {
                        vrm_list
                            .iter()
                            .find(|(_, name)| name == prev_name)
                            .map(|(idx, _)| *idx)
                    })
                })
                .unwrap_or(vrm_list[0].0);
            let source_override = snapshot.map(|snap| ReloadableSource::Snapshot {
                original_path: path.to_path_buf(),
                main_bytes: snap,
                aux_files: HashMap::new(),
            });
            return self.load_vrm_from_assets(&assets, vrm_idx, path, source_override);
        }

        // Did the initial load use Prefab-aware texture mapping?
        let use_prefab_mapping = self
            .loaded
            .as_ref()
            .is_some_and(|l| !l.pkg_material_keys.is_empty());

        let fbx_list = crate::unitypackage::find_fbx_list(&assets);
        if fbx_list.is_empty() {
            anyhow::bail!(".unitypackage 内に FBX ファイルが見つかりません");
        }

        // GUID / path match → basename fallback.
        let fbx_idx = self
            .selected_pkg_model
            .as_ref()
            .and_then(|loc| crate::unitypackage::find_asset_by_pathname(&assets, &loc.pathname))
            .or_else(|| {
                self.selected_fbx_name.as_ref().and_then(|prev_name| {
                    fbx_list
                        .iter()
                        .find(|(_, name)| name == prev_name)
                        .map(|(idx, _)| *idx)
                })
            })
            .unwrap_or(fbx_list[0].0);

        if use_prefab_mapping {
            // Prefab-aware path: build UnityPackageIndex and let prepare_pkg_fbx resolve textures.
            let pkg_index = std::sync::Arc::new(crate::unitypackage::build_unity_package_index(
                &archive_data,
            )?);
            // selected_pkg_model GUID → pkg_index lookup → pathname match → fallback.
            let pkg_fbx_idx = self
                .selected_pkg_model
                .as_ref()
                .and_then(|loc| pkg_index.by_guid.get(loc.guid.as_ref()).copied())
                .or_else(|| {
                    let fbx_pathname = &assets[fbx_idx].pathname;
                    pkg_index.by_path.get(fbx_pathname.as_str()).copied()
                })
                .unwrap_or(fbx_idx);

            let prepared = crate::unitypackage::prepare_pkg_fbx(&pkg_index, pkg_fbx_idx)?;
            let fbx_name = std::path::Path::new(prepared.model.pathname.as_ref())
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            log::info!(
                "Unitypackage reload (Prefab): {} textures: {}",
                fbx_name,
                prepared.textures.len()
            );

            // Via unitypackage: pass fbx_path=None to disable nearby-texture search.
            let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                &prepared.fbx_data,
                None,
                self.normalize_pose,
                self.normalize_to_tstance,
            )?;

            // Prefab-aware texture embedding.
            let prefab_label = format!(
                "prefab({})",
                std::path::Path::new(&*prepared.model.pathname)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            );
            crate::unitypackage::embed_textures_with_prefab(
                &mut ir,
                &prepared.textures,
                &prepared.resolved,
                &prefab_label,
            );

            // Keep pkg_textures in the legacy shape.
            let legacy_textures: Vec<(String, Arc<[u8]>)> = prepared
                .textures
                .iter()
                .map(|t| (t.display_name.to_string(), Arc::clone(&t.data)))
                .collect();
            if !legacy_textures.is_empty() {
                self.tex.pkg_textures = Some(legacy_textures);
                self.rebuild_pkg_thumb_cache();
            }

            // Restore manual assignments (apply to IrModel before the GPU build).
            if !saved_pkg_tex_assignments.is_empty() {
                let pkg_src = self.tex.pkg_textures.as_deref().unwrap_or(&[]);
                let name_to_data: HashMap<&str, &[u8]> = pkg_src
                    .iter()
                    .map(|(name, data)| (name.as_str(), data.as_ref()))
                    .collect();
                let mut name_to_ir: HashMap<String, usize> = HashMap::new();
                for (mat_idx, tex_name) in saved_pkg_tex_assignments {
                    if *mat_idx >= ir.materials.len() {
                        continue;
                    }
                    let ir_idx = if let Some(&cached) = name_to_ir.get(tex_name) {
                        cached
                    } else if let Some(data) = name_to_data.get(tex_name.as_str()) {
                        let is_psd = super::super::texture::is_psd_filename(tex_name);
                        let basename = std::path::Path::new(tex_name)
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        let (ir_data, ir_filename, ir_mime) = if is_psd {
                            match Self::psd_to_png(data) {
                                Ok(png_data) => (
                                    png_data,
                                    format!("{}.png", basename),
                                    "image/png".to_string(),
                                ),
                                Err(e) => {
                                    log::warn!("PSD->PNG conversion failed (pkg restore): {e}");
                                    continue;
                                }
                            }
                        } else {
                            let ext = crate::path_ext_lower(std::path::Path::new(tex_name));
                            let mime = crate::intermediate::types::mime_for_ext(&ext).to_string();
                            (data.to_vec(), tex_name.clone(), mime)
                        };
                        let idx = ir.textures.len();
                        ir.textures.push(crate::intermediate::types::IrTexture {
                            filename: ir_filename,
                            data: TextureData::Encoded(Arc::from(ir_data)),
                            mime_type: ir_mime,
                            source_path: format!("unitypackage: {}", tex_name),
                            mip_chain: None,
                        });
                        name_to_ir.insert(tex_name.clone(), idx);
                        idx
                    } else {
                        continue;
                    };
                    ir.materials[*mat_idx].texture_index = Some(ir_idx);
                    ir.materials[*mat_idx].apply_textured_defaults();
                    log::info!(
                        "Texture restored: mat[{}] '{}' <- '{}'",
                        mat_idx,
                        ir.materials[*mat_idx].name,
                        tex_name
                    );
                }
            }

            // Rebuild pkg_material_keys.
            let pkg_keys: Vec<Option<crate::unitypackage::PkgMaterialKey>> = {
                let fbx_guid: std::sync::Arc<str> = prepared.model.guid.as_ref().into();
                let instance_id = crate::unitypackage::BASE_INSTANCE_ID;
                ir.materials
                    .iter()
                    .map(|mat| {
                        Some(crate::unitypackage::PkgMaterialKey {
                            instance_id,
                            model_guid: fbx_guid.clone(),
                            source_material: mat.source_material.clone(),
                            material_name: mat.name.as_str().into(),
                        })
                    })
                    .collect()
            };

            let reload_source = match snapshot {
                Some(snap) => ReloadableSource::Snapshot {
                    original_path: path.to_path_buf(),
                    main_bytes: snap,
                    aux_files: HashMap::new(),
                },
                None => ReloadableSource::File(path.to_path_buf()),
            };
            let result = self.finish_load(ir, reload_source);
            // finish_load clears these, so restore them afterwards.
            self.tex.pkg_assignments = saved_pkg_tex_assignments.clone();
            if let Some(ref mut loaded) = self.loaded {
                loaded.pkg_material_keys = pkg_keys;
            }
            return result;
        }

        // Normal path: simple name-based matching.
        let (fbx_data, fbx_name, textures) =
            crate::unitypackage::take_fbx_and_textures(&assets, fbx_idx)?;
        log::info!(
            "Unitypackage reload: {} textures: {}",
            fbx_name,
            textures.len()
        );

        // Via unitypackage: pass fbx_path=None to disable nearby-texture search.
        let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
            &fbx_data,
            None,
            self.normalize_pose,
            self.normalize_to_tstance,
        )?;

        // Embed textures into the IR.
        let tex_source = if !textures.is_empty() {
            &textures
        } else if let Some(ref pkg) = saved_pkg_textures {
            pkg.as_slice()
        } else {
            &[]
        };
        crate::unitypackage::embed_textures_into_ir(&mut ir, tex_source);

        // Restore manual assignments (apply to IrModel before the GPU build).
        let pkg_src = if !textures.is_empty() {
            &textures
        } else {
            saved_pkg_textures.as_deref().unwrap_or(&[])
        };
        if !saved_pkg_tex_assignments.is_empty() && !pkg_src.is_empty() {
            let name_to_data: HashMap<&str, &[u8]> = pkg_src
                .iter()
                .map(|(name, data)| (name.as_str(), data.as_ref()))
                .collect();
            let mut name_to_ir: HashMap<String, usize> = HashMap::new();
            for (mat_idx, tex_name) in saved_pkg_tex_assignments {
                if *mat_idx >= ir.materials.len() {
                    continue;
                }
                let ir_idx = if let Some(&cached) = name_to_ir.get(tex_name) {
                    cached
                } else if let Some(data) = name_to_data.get(tex_name.as_str()) {
                    let is_psd = super::super::texture::is_psd_filename(tex_name);
                    let basename = std::path::Path::new(tex_name)
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let (ir_data, ir_filename, ir_mime) = if is_psd {
                        match Self::psd_to_png(data) {
                            Ok(png_data) => (
                                png_data,
                                format!("{}.png", basename),
                                "image/png".to_string(),
                            ),
                            Err(e) => {
                                log::warn!("PSD->PNG conversion failed (pkg restore): {e}");
                                continue;
                            }
                        }
                    } else {
                        let ext = crate::path_ext_lower(std::path::Path::new(tex_name));
                        let mime = crate::intermediate::types::mime_for_ext(&ext).to_string();
                        (data.to_vec(), tex_name.clone(), mime)
                    };
                    let idx = ir.textures.len();
                    ir.textures.push(crate::intermediate::types::IrTexture {
                        filename: ir_filename,
                        data: TextureData::Encoded(Arc::from(ir_data)),
                        mime_type: ir_mime,
                        source_path: format!("unitypackage: {}", tex_name),
                        mip_chain: None,
                    });
                    name_to_ir.insert(tex_name.clone(), idx);
                    idx
                } else {
                    continue;
                };
                ir.materials[*mat_idx].texture_index = Some(ir_idx);
                ir.materials[*mat_idx].apply_textured_defaults();
                log::info!(
                    "Texture restored: mat[{}] '{}' <- '{}'",
                    mat_idx,
                    ir.materials[*mat_idx].name,
                    tex_name
                );
            }
        }

        if !textures.is_empty() {
            self.tex.pkg_textures = Some(textures);
            self.rebuild_pkg_thumb_cache();
        }

        let reload_source = match snapshot {
            Some(snap) => ReloadableSource::Snapshot {
                original_path: path.to_path_buf(),
                main_bytes: snap,
                aux_files: HashMap::new(),
            },
            None => ReloadableSource::File(path.to_path_buf()),
        };
        let result = self.finish_load(ir, reload_source);
        // finish_load clears these, so restore them afterwards.
        self.tex.pkg_assignments = saved_pkg_tex_assignments.clone();
        result
    }

    /// Reload a Prefab model (rebuild pkg_index and re-invoke load_prefab_from_assets).
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    fn reload_as_prefab(
        &mut self,
        archive_data: &[u8],
        snapshot: Option<Arc<[u8]>>,
        path: &Path,
        prefab_entry_path: &str,
        archive_source: &ReloadableSource,
        saved_pkg_textures: &Option<Vec<(String, Arc<[u8]>)>>,
        saved_pkg_tex_assignments: &HashMap<usize, String>,
    ) -> anyhow::Result<()> {
        let pkg_index = Arc::new(crate::unitypackage::build_unity_package_index(
            archive_data,
        )?);
        let prefab_index = pkg_index
            .by_path
            .get(prefab_entry_path)
            .copied()
            .ok_or_else(|| {
                anyhow::anyhow!("Prefab エントリが見つかりません: {}", prefab_entry_path)
            })?;

        // Keep the Archive source after reload (use Snapshot when available, otherwise carry over the original Archive).
        let source_override: Option<ReloadableSource> = if let Some(snap) = snapshot {
            Some(ReloadableSource::Snapshot {
                original_path: path.to_path_buf(),
                main_bytes: snap,
                aux_files: HashMap::new(),
            })
        } else {
            Some(archive_source.clone())
        };

        self.load_prefab_from_assets(&[], prefab_index, path, source_override, Some(pkg_index))?;

        // Restore pkg textures if load_prefab_from_assets didn't set them.
        if self.tex.pkg_textures.is_none() {
            if let Some(ref saved) = saved_pkg_textures {
                self.tex.pkg_textures = Some(saved.clone());
                self.rebuild_pkg_thumb_cache();
            }
        }

        // Restore manual texture assignments after the GPU model is built.
        // (Collect the data first so the borrow checker stays happy when applying.)
        if !saved_pkg_tex_assignments.is_empty() {
            let assignments_to_restore: Vec<(usize, String, Vec<u8>)> = {
                let pkg_src = self.tex.pkg_textures.as_deref().unwrap_or(&[]);
                let name_to_data: HashMap<&str, &[u8]> = pkg_src
                    .iter()
                    .map(|(name, data)| (name.as_str(), data.as_ref()))
                    .collect();
                saved_pkg_tex_assignments
                    .iter()
                    .filter_map(|(idx, tex_name)| {
                        name_to_data
                            .get(tex_name.as_str())
                            .map(|data| (*idx, tex_name.clone(), data.to_vec()))
                    })
                    .collect()
            };
            for (mat_idx, tex_name, data) in &assignments_to_restore {
                if self.assign_texture_data_to_material(*mat_idx, tex_name, data) {
                    self.tex.pkg_assignments.insert(*mat_idx, tex_name.clone());
                }
            }
        }

        Ok(())
    }

    /// Reload a .unitypackage that lives inside an archive (ZIP / 7z).
    #[allow(clippy::type_complexity)]
    fn reload_archive_unitypackage(
        &mut self,
        original_path: &Path,
        archive_bytes: Option<&Arc<[u8]>>,
        selected_entry_path: &str,
        archive_source: &ReloadableSource,
        saved_pkg_textures: &Option<Vec<(String, Arc<[u8]>)>>,
        saved_pkg_tex_assignments: &HashMap<usize, String>,
    ) -> anyhow::Result<()> {
        let owned;
        let data: &[u8] = if let Some(snap) = archive_bytes {
            snap
        } else {
            owned = std::fs::read(original_path)?;
            &owned
        };

        let ext = crate::path_ext_lower(original_path);
        let format = crate::archive::archive_format_from_ext(&ext)
            .with_context(|| t!("error.unsupported_archive_format", ext = ext).into_owned())?;

        let contents = crate::archive::list_models(data, format)?;

        let model_index = contents
            .models
            .iter()
            .position(|(_, p, _, _)| p.to_string_lossy() == selected_entry_path)
            .ok_or_else(|| {
                anyhow::anyhow!(t!(
                    "error.archive_old_model_not_found",
                    path = selected_entry_path
                )
                .into_owned())
            })?;

        let bundle = crate::archive::extract_model_bundle(data, format, contents, model_index)?;

        let pkg_data = bundle.model.data;

        // For Prefab models, reload via the Prefab path.
        if let Some(prefab_path) = self
            .loaded
            .as_ref()
            .and_then(|l| l.prefab_entry_path.clone())
        {
            let snapshot_arc: Option<Arc<[u8]>> = archive_bytes.cloned();
            return self.reload_as_prefab(
                &pkg_data,
                snapshot_arc,
                original_path,
                &prefab_path,
                archive_source,
                saved_pkg_textures,
                saved_pkg_tex_assignments,
            );
        }

        let assets = crate::unitypackage::extract_all_assets(&pkg_data)?;

        let is_vrm = self.loaded.as_ref().is_some_and(|l| {
            !matches!(
                l.ir.source_format,
                crate::intermediate::types::SourceFormat::Fbx
            )
        });

        if is_vrm {
            let vrm_list = crate::unitypackage::find_vrm_list(&assets);
            if vrm_list.is_empty() {
                anyhow::bail!(".unitypackage 内に VRM ファイルが見つかりません");
            }
            // GUID / path match → basename fallback.
            let vrm_idx = self
                .selected_pkg_model
                .as_ref()
                .and_then(|loc| crate::unitypackage::find_asset_by_pathname(&assets, &loc.pathname))
                .or_else(|| {
                    self.selected_fbx_name.as_ref().and_then(|prev_name| {
                        vrm_list
                            .iter()
                            .find(|(_, name)| name == prev_name)
                            .map(|(idx, _)| *idx)
                    })
                })
                .unwrap_or(vrm_list[0].0);
            return self.load_vrm_from_assets(
                &assets,
                vrm_idx,
                original_path,
                Some(archive_source.clone()),
            );
        }

        let fbx_list = crate::unitypackage::find_fbx_list(&assets);
        if fbx_list.is_empty() {
            anyhow::bail!(".unitypackage 内に FBX ファイルが見つかりません");
        }

        // GUID / path match → basename fallback.
        let fbx_idx = self
            .selected_pkg_model
            .as_ref()
            .and_then(|loc| crate::unitypackage::find_asset_by_pathname(&assets, &loc.pathname))
            .or_else(|| {
                self.selected_fbx_name.as_ref().and_then(|prev_name| {
                    fbx_list
                        .iter()
                        .find(|(_, name)| name == prev_name)
                        .map(|(idx, _)| *idx)
                })
            })
            .unwrap_or(fbx_list[0].0);

        let (fbx_data, fbx_name, textures) =
            crate::unitypackage::take_fbx_and_textures(&assets, fbx_idx)?;
        log::info!(
            "Archive unitypackage reload: {} textures: {}",
            fbx_name,
            textures.len()
        );

        // Via unitypackage: pass fbx_path=None to disable nearby-texture search.
        let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
            &fbx_data,
            None,
            self.normalize_pose,
            self.normalize_to_tstance,
        )?;

        let tex_source = if !textures.is_empty() {
            &textures
        } else if let Some(ref pkg) = saved_pkg_textures {
            pkg.as_slice()
        } else {
            &[]
        };
        crate::unitypackage::embed_textures_into_ir(&mut ir, tex_source);

        let pkg_src = if !textures.is_empty() {
            &textures
        } else {
            saved_pkg_textures.as_deref().unwrap_or(&[])
        };
        if !saved_pkg_tex_assignments.is_empty() && !pkg_src.is_empty() {
            let name_to_data: HashMap<&str, &[u8]> = pkg_src
                .iter()
                .map(|(name, data)| (name.as_str(), data.as_ref()))
                .collect();
            let mut name_to_ir: HashMap<String, usize> = HashMap::new();
            for (mat_idx, tex_name) in saved_pkg_tex_assignments {
                if *mat_idx >= ir.materials.len() {
                    continue;
                }
                let ir_idx = if let Some(&cached) = name_to_ir.get(tex_name) {
                    cached
                } else if let Some(data) = name_to_data.get(tex_name.as_str()) {
                    let is_psd = super::super::texture::is_psd_filename(tex_name);
                    let basename = std::path::Path::new(tex_name)
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let (ir_data, ir_filename, ir_mime) = if is_psd {
                        match Self::psd_to_png(data) {
                            Ok(png_data) => (
                                png_data,
                                format!("{}.png", basename),
                                "image/png".to_string(),
                            ),
                            Err(e) => {
                                log::warn!("PSD->PNG conversion failed (pkg restore): {e}");
                                continue;
                            }
                        }
                    } else {
                        let ext = crate::path_ext_lower(std::path::Path::new(tex_name));
                        let mime = crate::intermediate::types::mime_for_ext(&ext).to_string();
                        (data.to_vec(), tex_name.clone(), mime)
                    };
                    let idx = ir.textures.len();
                    ir.textures.push(crate::intermediate::types::IrTexture {
                        filename: ir_filename,
                        data: TextureData::Encoded(Arc::from(ir_data)),
                        mime_type: ir_mime,
                        source_path: format!("unitypackage: {}", tex_name),
                        mip_chain: None,
                    });
                    name_to_ir.insert(tex_name.clone(), idx);
                    idx
                } else {
                    continue;
                };
                ir.materials[*mat_idx].texture_index = Some(ir_idx);
                ir.materials[*mat_idx].apply_textured_defaults();
                log::info!(
                    "Texture restored: mat[{}] '{}' <- '{}'",
                    mat_idx,
                    ir.materials[*mat_idx].name,
                    tex_name
                );
            }
        }

        if !textures.is_empty() {
            self.tex.pkg_textures = Some(textures);
            self.rebuild_pkg_thumb_cache();
        }

        let result = self.finish_load(ir, archive_source.clone());
        self.tex.pkg_assignments = saved_pkg_tex_assignments.clone();
        result
    }

    pub(super) fn open_file_dialog(&mut self, ctx: &egui::Context) {
        // Skip if a dialog is already open.
        if self.pending.file_dialog.is_some() {
            return;
        }
        let initial_dir = self.last_model_dir.clone();
        let dialog_title = t!("viewer.dialog.open_model.title").into_owned();
        let filter_supported = t!("viewer.dialog.open_model.filter_supported").into_owned();
        let filter_archive = t!("viewer.dialog.common.filter_archive").into_owned();
        let (tx, rx) = std::sync::mpsc::channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let mut dialog = rfd::FileDialog::new()
                .set_title(dialog_title)
                .add_filter(
                    filter_supported,
                    &[
                        "vrm",
                        "fbx",
                        "pmx",
                        "pmd",
                        "obj",
                        "stl",
                        "x",
                        "unitypackage",
                        "vrma",
                        "zip",
                        "7z",
                    ],
                )
                .add_filter("VRM (.vrm)", &["vrm"])
                .add_filter("FBX (.fbx)", &["fbx"])
                .add_filter("PMX (.pmx)", &["pmx"])
                .add_filter("PMD (.pmd)", &["pmd"])
                .add_filter("OBJ (.obj)", &["obj"])
                .add_filter("STL (.stl)", &["stl"])
                .add_filter("DirectX text (.x)", &["x"])
                .add_filter("UnityPackage (.unitypackage)", &["unitypackage"])
                .add_filter(filter_archive, &["zip", "7z"])
                .add_filter("VRMA (.vrma)", &["vrma"]);
            if let Some(ref dir) = initial_dir {
                dialog = dialog.set_directory(dir);
            }
            let _ = tx.send(dialog.pick_file());
            repaint.request_repaint();
        });
        self.pending.file_dialog = Some((super::pending::FileDialogKind::Open, rx));
    }

    /// Append-model file dialog.
    pub(super) fn open_append_dialog(&mut self, ctx: &egui::Context) {
        // Skip if a dialog is already open.
        if self.pending.file_dialog.is_some() {
            return;
        }
        let initial_dir = self.last_model_dir.clone();
        let dialog_title = t!("viewer.dialog.append_model.title").into_owned();
        let filter_3d_model = t!("viewer.dialog.append_model.filter_3d_model").into_owned();
        let filter_archive = t!("viewer.dialog.common.filter_archive").into_owned();
        let (tx, rx) = std::sync::mpsc::channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let mut dialog = rfd::FileDialog::new()
                .set_title(dialog_title)
                .add_filter(
                    filter_3d_model,
                    &[
                        "vrm",
                        "fbx",
                        "pmx",
                        "pmd",
                        "obj",
                        "stl",
                        "x",
                        "unitypackage",
                        "zip",
                        "7z",
                    ],
                )
                .add_filter("VRM (.vrm)", &["vrm"])
                .add_filter("FBX (.fbx)", &["fbx"])
                .add_filter("PMX (.pmx)", &["pmx"])
                .add_filter("PMD (.pmd)", &["pmd"])
                .add_filter("UnityPackage (.unitypackage)", &["unitypackage"])
                .add_filter(filter_archive, &["zip", "7z"]);
            if let Some(ref dir) = initial_dir {
                dialog = dialog.set_directory(dir);
            }
            let _ = tx.send(dialog.pick_file());
            repaint.request_repaint();
        });
        self.pending.file_dialog = Some((super::pending::FileDialogKind::Append, rx));
    }

    /// Append (merge) another model into the currently loaded one.
    #[allow(dead_code)]
    pub(super) fn append_model(&mut self, path: PathBuf) {
        log::info!("Append file: {}", path.display());
        let ext = crate::path_ext_lower(&path);

        if ext == "unitypackage" {
            match self.try_load_unitypackage_for_append(&path) {
                Ok(()) => {}
                Err(e) => {
                    log::error!("Append load failed (pkg): {e}");
                    self.convert_message = Some(ConvertMessage::failure(
                        t!("viewer.toast.append.failed", error = e.to_string()).into_owned(),
                    ));
                }
            }
            return;
        }
        if matches!(ext.as_str(), "zip" | "7z") {
            match self.try_load_archive_for_append(&path) {
                Ok(()) => {}
                Err(e) => {
                    log::error!("Append load failed (archive): {e}");
                    self.convert_message = Some(ConvertMessage::failure(
                        t!("viewer.toast.append.failed", error = e.to_string()).into_owned(),
                    ));
                }
            }
            return;
        }

        let ir_result: anyhow::Result<IrModel> = (|| -> anyhow::Result<IrModel> {
            match ext.as_str() {
                "fbx" => {
                    let data = self.read_or_preloaded(&path)?;
                    Ok(crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                        &data,
                        Some(&path),
                        self.normalize_pose,
                        self.normalize_to_tstance,
                    )?)
                }
                "pmx" => {
                    let pmx_model = if self.preloaded.as_ref().is_some_and(|pl| pl.path == path) {
                        let data = self.read_or_preloaded(&path)?;
                        crate::pmx::reader::read_pmx_from_data(&data)?
                    } else {
                        crate::pmx::reader::read_pmx(&path)?
                    };
                    let pmx_dir = path.parent().unwrap_or(std::path::Path::new("."));
                    let mut ir = crate::pmx::extract::pmx_to_ir(&pmx_model, pmx_dir)?;
                    if self.normalize_pose {
                        ir.astance_result =
                            crate::intermediate::pose::normalize_pose_to_tstance_full(
                                &mut ir.bones,
                                &mut ir.meshes,
                                &mut ir.morphs,
                                &mut ir.physics,
                                crate::convert::coord::gltf_pos_to_pmx,
                            );
                    }
                    Ok(ir)
                }
                "pmd" => {
                    let pmd_model = if self.preloaded.as_ref().is_some_and(|pl| pl.path == path) {
                        let data = self.read_or_preloaded(&path)?;
                        crate::pmd::reader::read_pmd_from_data(&data)?
                    } else {
                        crate::pmd::reader::read_pmd(&path)?
                    };
                    let mut ir = crate::pmd::extract::pmd_to_ir(&pmd_model, &path)?;
                    if self.normalize_pose {
                        ir.astance_result =
                            crate::intermediate::pose::normalize_pose_to_tstance_full(
                                &mut ir.bones,
                                &mut ir.meshes,
                                &mut ir.morphs,
                                &mut ir.physics,
                                crate::convert::coord::gltf_pos_to_pmx,
                            );
                    }
                    Ok(ir)
                }
                "obj" => {
                    if is_temp_path(&path)
                        || self.preloaded.as_ref().is_some_and(|pl| pl.path == path)
                    {
                        let data = self.read_or_preloaded(&path)?;
                        let obj_dir = path.parent().unwrap_or(Path::new("."));
                        let aux = self.take_or_collect_aux(&path);
                        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Model");
                        Ok(crate::obj::extract::load_obj_from_data(
                            &data,
                            name,
                            obj_dir,
                            Some(&aux),
                        )?)
                    } else {
                        Ok(crate::obj::extract::load_obj(&path)?)
                    }
                }
                "stl" => {
                    if is_temp_path(&path)
                        || self.preloaded.as_ref().is_some_and(|pl| pl.path == path)
                    {
                        let data = self.read_or_preloaded(&path)?;
                        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Model");
                        Ok(crate::stl::extract::load_stl_from_data(&data, name)?)
                    } else {
                        Ok(crate::stl::extract::load_stl(&path)?)
                    }
                }
                "x" => {
                    if is_temp_path(&path)
                        || self.preloaded.as_ref().is_some_and(|pl| pl.path == path)
                    {
                        let data = self.read_or_preloaded(&path)?;
                        let x_dir = path.parent().unwrap_or(Path::new("."));
                        let aux = self.take_or_collect_aux(&path);
                        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Model");
                        Ok(crate::directx::extract::load_x_from_data(
                            &data,
                            name,
                            x_dir,
                            Some(&aux),
                        )?)
                    } else {
                        Ok(crate::directx::extract::load_x(&path)?)
                    }
                }
                _ => self.load_vrm_as_ir(&path),
            }
        })();

        match ir_result {
            Ok(other_ir) => {
                if let Some(ref loaded) = self.loaded {
                    let host_fmt = loaded.ir.source_format;
                    let other_fmt = other_ir.source_format;
                    if host_fmt.is_vrm0() != other_fmt.is_vrm0() {
                        log::warn!(
                            "Appending model with different coordinate system: {} + {}",
                            host_fmt.label(),
                            other_fmt.label()
                        );
                        self.convert_message = Some(ConvertMessage::failure(
                            t!(
                                "viewer.toast.append.coord_mismatch",
                                host = host_fmt.label(),
                                other = other_fmt.label()
                            )
                            .into_owned(),
                        ));
                        return;
                    }
                }
                let source = if is_temp_path(&path) {
                    let main_data = match std::fs::read(&path) {
                        Ok(d) => d,
                        Err(_) => {
                            log::warn!("Temp file reload failed: {}", path.display());
                            self.finish_append_with_source(
                                other_ir,
                                ReloadableSource::File(path.clone()),
                                None,
                            );
                            return;
                        }
                    };
                    let mut aux = HashMap::new();
                    if ext == "fbx" {
                        if let Some(dir) = path.parent() {
                            collect_image_files_recursive(dir, dir, &mut aux);
                        }
                    } else if ext == "pmx" {
                        if let Ok(pmx_model) = crate::pmx::reader::read_pmx_from_data(&main_data) {
                            let pmx_dir = path.parent().unwrap_or(Path::new("."));
                            for tex_path in &pmx_model.textures {
                                let normalized = tex_path.replace('\\', "/");
                                let full = pmx_dir.join(&normalized);
                                if let Ok(data) = std::fs::read(&full) {
                                    aux.insert(
                                        PathBuf::from(&normalized),
                                        Arc::from(data.into_boxed_slice()),
                                    );
                                }
                            }
                        }
                    } else if ext == "pmd" {
                        let pmd_dir = path.parent().unwrap_or(Path::new("."));
                        if let Ok(pmd_model) = crate::pmd::reader::read_pmd_from_data(&main_data) {
                            for mat in &pmd_model.materials {
                                if mat.texture_name.is_empty() {
                                    continue;
                                }
                                let main_tex = mat.texture_name.split('*').next().unwrap_or("");
                                if main_tex.is_empty() {
                                    continue;
                                }
                                let normalized = main_tex.replace('\\', "/");
                                let key = PathBuf::from(&normalized);
                                if let std::collections::hash_map::Entry::Vacant(e) = aux.entry(key)
                                {
                                    if let Ok(data) = std::fs::read(pmd_dir.join(&normalized)) {
                                        e.insert(Arc::from(data.into_boxed_slice()));
                                    }
                                }
                            }
                        }
                        let txt_path = path.with_extension("txt");
                        if let Ok(data) = std::fs::read(&txt_path) {
                            let txt_name =
                                txt_path.file_name().map(PathBuf::from).unwrap_or_default();
                            aux.insert(txt_name, Arc::from(data.into_boxed_slice()));
                        }
                    } else if ext == "obj" || ext == "x" {
                        // OBJ / DirectX: collect images + MTL from the same directory.
                        if let Some(dir) = path.parent() {
                            collect_image_files_recursive(dir, dir, &mut aux);
                        }
                    }
                    // STL needs no aux files (no textures, no MTL).
                    ReloadableSource::Snapshot {
                        original_path: path.clone(),
                        main_bytes: main_data.into(),
                        aux_files: aux,
                    }
                } else {
                    ReloadableSource::File(path.clone())
                };
                self.finish_append_with_source(other_ir, source, None);
            }
            Err(e) => {
                log::error!("Append load failed: {e}");
                self.convert_message = Some(ConvertMessage::failure(
                    t!("viewer.toast.append.failed", error = e.to_string()).into_owned(),
                ));
            }
        }
    }

    /// Load a VRM file as an IrModel (for append flows; no GPU build).
    fn load_vrm_as_ir(&mut self, path: &std::path::Path) -> anyhow::Result<IrModel> {
        let glb = if self.preloaded.as_ref().is_some_and(|pl| pl.path == path) {
            let data = self.read_or_preloaded(path)?;
            vrm::loader::load_glb_from_data(&data)?
        } else {
            vrm::loader::load_glb(path)?
        };
        let version = vrm::detect::detect_version(&glb.document);
        let all_extensions = vrm::loader::get_raw_extensions(&glb.document);

        let mut ir = vrm::extract::extract_ir_model_with_options(
            &glb.document,
            &glb.buffers,
            &glb.images,
            &glb.vrm_extension,
            &version,
            &all_extensions,
            self.normalize_pose,
        )?;

        Self::encode_ir_textures_as_png(&mut ir, &glb.images);
        Ok(ir)
    }

    /// Append a model contained in a unitypackage onto the currently loaded model.
    /// Returns true on success.
    pub(super) fn append_from_pkg(
        &mut self,
        assets: &[crate::unitypackage::ExtractedAsset],
        model_index: usize,
        model_type: PkgModelType,
        source_path: &std::path::Path,
        source_override: Option<ReloadableSource>,
        pkg_index: Option<Arc<UnityPackageIndex>>,
    ) -> bool {
        let normalize = self.normalize_pose;
        let normalize_tstance = self.normalize_to_tstance;
        let mut pkg_unmatched: Vec<usize> = Vec::new();
        let mut pkg_model_name: Option<String> = None;
        let mut pkg_textures_to_add: Vec<(String, Arc<[u8]>)> = Vec::new();
        // For building PkgModelLocator: capture pathname before assets are consumed.
        let asset_pathname: Option<String> = assets.get(model_index).map(|a| a.pathname.clone());
        let ir_result: anyhow::Result<IrModel> = (|| -> anyhow::Result<IrModel> {
            match model_type {
                PkgModelType::Fbx => {
                    let (fbx_data, fbx_name, textures) =
                        crate::unitypackage::take_fbx_and_textures(assets, model_index)?;
                    log::info!(
                        "Append (FBX in pkg): {} textures: {}",
                        fbx_name,
                        textures.len()
                    );
                    pkg_model_name = Some(fbx_name.clone());
                    // Via unitypackage: pass fbx_path=None to disable nearby-texture search.
                    let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                        &fbx_data,
                        None,
                        normalize,
                        normalize_tstance,
                    )?;
                    let unmatched = crate::unitypackage::embed_textures_into_ir(&mut ir, &textures);
                    log::info!(
                        "Append (pkg): {} materials matched, unassigned: {}",
                        ir.materials.len() - unmatched.len(),
                        unmatched.len()
                    );
                    pkg_unmatched = unmatched;
                    pkg_textures_to_add = textures;
                    Ok(ir)
                }
                PkgModelType::Vrm => {
                    let (vrm_data, vrm_name) = crate::unitypackage::take_vrm(assets, model_index)?;
                    log::info!("Append (VRM in pkg): {}", vrm_name);
                    pkg_model_name = Some(vrm_name.clone());
                    let glb = vrm::loader::load_glb_from_data(&vrm_data)?;
                    let version = vrm::detect::detect_version(&glb.document);
                    let all_extensions = vrm::loader::get_raw_extensions(&glb.document);
                    let mut ir = vrm::extract::extract_ir_model_with_options(
                        &glb.document,
                        &glb.buffers,
                        &glb.images,
                        &glb.vrm_extension,
                        &version,
                        &all_extensions,
                        normalize,
                    )?;
                    Self::encode_ir_textures_as_png(&mut ir, &glb.images);
                    Ok(ir)
                }
                PkgModelType::Prefab => {
                    let pkg = pkg_index
                        .as_ref()
                        .context("Prefab append には pkg_index が必要です")?;

                    let resolve_result =
                        crate::unitypackage::resolve_single_prefab(pkg, model_index)?;
                    log::info!(
                        "Append Prefab resolved: {} FBX detected",
                        resolve_result.entries.len()
                    );

                    let prefab_filename = std::path::Path::new(&pkg.entries[model_index].pathname)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();

                    // Collect textures.
                    let textures: Vec<crate::unitypackage::PackageTexture> = pkg
                        .entries
                        .iter()
                        .filter(|e| {
                            let lower = e.pathname.to_lowercase();
                            [
                                ".png", ".jpg", ".jpeg", ".tga", ".bmp", ".psd", ".tif", ".tiff",
                            ]
                            .iter()
                            .any(|ext| lower.ends_with(ext))
                        })
                        .map(|e| {
                            let display_name = std::path::Path::new(&e.pathname)
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy();
                            crate::unitypackage::PackageTexture {
                                guid: Arc::from(e.guid.as_str()),
                                display_name: Arc::from(display_name.as_ref()),
                                data: Arc::clone(&e.data),
                                pathname: Arc::from(e.pathname.as_str()),
                            }
                        })
                        .collect();

                    // Legacy-shaped texture list (used to extend pkg_textures).
                    pkg_textures_to_add = textures
                        .iter()
                        .map(|t| (t.display_name.to_string(), Arc::clone(&t.data)))
                        .collect();

                    let mut base_ir: Option<IrModel> = None;

                    for (i, fbx_entry_info) in resolve_result.entries.iter().enumerate() {
                        let fbx_entry = &pkg.entries[fbx_entry_info.fbx_index];
                        let fbx_data = fbx_entry.data.to_vec();
                        let fbx_name = std::path::Path::new(&fbx_entry.pathname)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();

                        log::info!(
                            "  Append FBX[{}]: {} (GUID={})",
                            i,
                            fbx_name,
                            fbx_entry_info.fbx_guid
                        );

                        let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                            &fbx_data,
                            None,
                            normalize,
                            normalize_tstance,
                        )?;

                        let prefab_label = format!("prefab({})", prefab_filename);
                        let unmatched = crate::unitypackage::embed_textures_with_prefab(
                            &mut ir,
                            &textures,
                            &fbx_entry_info.materials,
                            &prefab_label,
                        );

                        if let Some(ref mut base) = base_ir {
                            let mat_offset = base.materials.len();
                            base.merge(ir);
                            pkg_unmatched.extend(unmatched.iter().map(|&idx| idx + mat_offset));
                        } else {
                            pkg_model_name = Some(prefab_filename.clone());
                            pkg_unmatched = unmatched;
                            base_ir = Some(ir);
                        }
                    }

                    base_ir.context("Prefab に有効な FBX が見つかりません")
                }
            }
        })();

        match ir_result {
            Ok(other_ir) => {
                let mat_offset = self
                    .loaded
                    .as_ref()
                    .map(|l| l.ir.materials.len())
                    .unwrap_or(0);
                let appended_before = self
                    .loaded
                    .as_ref()
                    .map(|l| l.appended_models.len())
                    .unwrap_or(0);
                let tex_count_before = self
                    .loaded
                    .as_ref()
                    .map(|l| l.ir.textures.len())
                    .unwrap_or(0);
                // Build a stable key (pathname-based — GUID is unavailable through the ExtractedAsset path).
                let pkg_locator = asset_pathname.map(|path| {
                    crate::unitypackage::PkgModelLocator {
                        guid: "".into(), // No GUID via ExtractedAsset.
                        pathname: path.into(),
                        kind: model_type,
                    }
                });
                match source_override {
                    Some(source) => {
                        self.finish_append_ext(other_ir, source, false, pkg_model_name, pkg_locator)
                    }
                    None => self.finish_append_ext(
                        other_ir,
                        ReloadableSource::File(source_path.to_path_buf()),
                        false,
                        pkg_model_name,
                        pkg_locator,
                    ),
                }
                let appended_after = self
                    .loaded
                    .as_ref()
                    .map(|l| l.appended_models.len())
                    .unwrap_or(0);
                if appended_after > appended_before {
                    let pkg_stem = source_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("pkg");
                    let pkg_prefix = format!("{}_pkg{}", pkg_stem, appended_after);

                    if let Some(ref mut loaded) = self.loaded {
                        for tex in loaded.ir.textures[tex_count_before..].iter_mut() {
                            tex.filename = format!("{}_{}", pkg_prefix, tex.filename);
                        }
                    }

                    if !pkg_textures_to_add.is_empty() {
                        for (name, _) in &mut pkg_textures_to_add {
                            *name = format!("{}_{}", pkg_prefix, name);
                        }
                        if let Some(ref mut existing) = self.tex.pkg_textures {
                            existing.extend(pkg_textures_to_add);
                        } else {
                            self.tex.pkg_textures = Some(pkg_textures_to_add);
                        }
                        self.rebuild_pkg_thumb_cache();
                    }
                }
                if !pkg_unmatched.is_empty()
                    && self.tex.pkg_textures.is_some()
                    && !self.suppress_tex_match
                {
                    // Restore any existing preview first.
                    self.cancel_tex_match_preview();
                    let global_unmatched: Vec<usize> =
                        pkg_unmatched.iter().map(|&i| i + mat_offset).collect();
                    let count = global_unmatched.len();
                    self.tex.pending_match = Some(PendingTexMatch {
                        mat_indices: global_unmatched,
                        selections: vec![None; count],
                        tex_filter: String::new(),
                        previewed: vec![None; count],
                        saved_binds: std::collections::HashMap::new(),
                        texture_views: Vec::new(),
                        failed_uploads: std::collections::HashSet::new(),
                    });
                }
            }
            Err(e) => {
                log::error!("Append load failed (pkg): {e}");
                self.convert_message = Some(ConvertMessage::failure(
                    t!("viewer.toast.append.failed", error = e.to_string()).into_owned(),
                ));
                return false;
            }
        }
        true
    }

    #[expect(dead_code)]
    fn finish_append(&mut self, other_ir: IrModel, path: &Path) {
        self.finish_append_ext(
            other_ir,
            ReloadableSource::File(path.to_path_buf()),
            false,
            None,
            None,
        );
    }

    pub(super) fn finish_append_with_source(
        &mut self,
        other_ir: IrModel,
        source: ReloadableSource,
        pkg_model_name: Option<String>,
    ) {
        self.finish_append_ext(other_ir, source, false, pkg_model_name, None);
    }

    fn finish_append_ext(
        &mut self,
        mut other_ir: IrModel,
        source: ReloadableSource,
        silent: bool,
        pkg_model_name: Option<String>,
        #[allow(unused_variables)] pkg_locator: Option<crate::unitypackage::PkgModelLocator>,
    ) {
        let Some(ref mut loaded) = self.loaded else {
            return;
        };

        let added_name = other_ir.name.clone();
        let added_bones = other_ir.bones.len();
        let added_meshes = other_ir.meshes.len();
        let added_materials = other_ir.materials.len();

        let saved_bone_count = loaded.ir.bones.len();
        let saved_mesh_count = loaded.ir.meshes.len();
        let saved_material_count = loaded.ir.materials.len();
        let saved_texture_count = loaded.ir.textures.len();
        let saved_morph_count = loaded.ir.morphs.len();
        let saved_rigid_count = loaded.ir.physics.rigid_bodies.len();
        let saved_joint_count = loaded.ir.physics.joints.len();
        let saved_name = loaded.ir.name.clone();
        let saved_node_to_bone = loaded.ir.node_to_bone.clone();
        let saved_humanoid_count = loaded.ir.humanoid_bone_count;
        let saved_bone_meta: Vec<(Vec<usize>, Option<String>)> = loaded
            .ir
            .bones
            .iter()
            .map(|b| (b.children.clone(), b.vrm_bone_name.clone()))
            .collect();

        // If `other` lacks humanoid info, re-detect from original_name and fill it in.
        let other_has_humanoid = other_ir.bones.iter().any(|b| b.vrm_bone_name.is_some());
        if !other_has_humanoid {
            let names: Vec<(usize, &str)> = other_ir
                .bones
                .iter()
                .enumerate()
                .map(|(i, b)| (i, b.original_name.as_str()))
                .collect();
            let mapping = crate::fbx::humanoid::detect_humanoid(&names);
            for (&idx, hb) in &mapping.mapping {
                other_ir.bones[idx].vrm_bone_name = Some(hb.as_vrm_name().to_string());
            }
            if !mapping.mapping.is_empty() {
                log::info!(
                    "Pre-merge humanoid completion: {} bones detected",
                    mapping.mapping.len()
                );
            }
        }

        let (merged_bones, new_bones) = loaded.ir.merge(other_ir);

        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        // Material count changes after merge, so resize material_display.
        let mc = loaded.ir.materials.len();
        self.material_display
            .resize_with(mc, MaterialDisplayState::default);
        let mat_flags = Self::extract_per_mat_vecs(&self.material_display);
        match super::super::mesh::build_gpu_model_from_ir(&loaded.ir, device, queue, &mat_flags) {
            Ok(mut gpu_model) => {
                if let Some(ref renderer) = self.renderer {
                    renderer.prepare_mmd_resources(
                        device,
                        &mut gpu_model,
                        &loaded.ir,
                        &mat_flags.emissive,
                    );
                }
                if let Some(tex_id) = self.viewport_texture_id.take() {
                    let mut renderer = self.render_state.renderer.write();
                    renderer.free_texture(&tex_id);
                }
                let new_draw_count = gpu_model.draws.len();
                self.material_visibility.resize(new_draw_count, true);
                let new_morph_count = loaded.ir.morphs.len();
                self.morph_weights.resize(new_morph_count, 0.0);
                self.morph_dirty = self.morph_weights.iter().any(|&w| w != 0.0);
                loaded.mat_cache = Self::build_mat_cache(&loaded.ir, &gpu_model);
                loaded.stats_cache = CachedStats::new(&loaded.ir);
                let prev_draw_end: usize = loaded
                    .material_groups
                    .iter()
                    .map(|g| g.draw_range.end)
                    .max()
                    .unwrap_or(0);
                loaded.material_groups.push(MaterialGroup {
                    name: added_name.clone(),
                    material_range: saved_material_count..saved_material_count + added_materials,
                    draw_range: prev_draw_end..gpu_model.draws.len(),
                });
                loaded.gpu_model = gpu_model;
                let display_path = source.display_path().to_path_buf();
                loaded.appended_models.push(AppendedModel {
                    source,
                    pkg_model_name: pkg_model_name.clone(),
                    pkg_model: pkg_locator,
                });
                if let Some(dir) = display_path.parent() {
                    self.tex.last_dir = Some(dir.to_path_buf());
                }
                if let Some(ref mut renderer) = self.renderer {
                    renderer.invalidate_visualization_cache();
                    renderer.invalidate_normal_cache();
                    renderer.mark_sort_dirty();
                    // Refresh the grid after append (it grows when a large model is added).
                    let (bbox_min, bbox_max) = loaded.gpu_model.bbox();
                    renderer.rebuild_grid(&self.render_state.device, bbox_min, bbox_max);
                }
                if let (Some(ref loaded), Some(ref old_anim)) = (&self.loaded, &self.anim.state) {
                    let mut new_state = AnimationState::new(
                        Arc::clone(&old_anim.animation),
                        &loaded.ir,
                        &loaded.gpu_model,
                    );
                    new_state.playing = old_anim.playing;
                    new_state.loop_mode = old_anim.loop_mode;
                    new_state.speed = old_anim.speed;
                    new_state.current_time = old_anim.current_time;
                    new_state.ab_start = old_anim.ab_start;
                    new_state.ab_end = old_anim.ab_end;
                    new_state.ping_pong_direction = old_anim.ping_pong_direction;
                    self.anim.state = Some(new_state);
                }
                log::info!(
                    "Append loaded: {} (bones:{} -> merged:{}/new:{}, meshes:{}, materials:{})",
                    added_name,
                    added_bones,
                    merged_bones,
                    new_bones,
                    added_meshes,
                    added_materials,
                );
                if !silent {
                    self.convert_message = Some(ConvertMessage::success(
                        t!(
                            "viewer.toast.append.loaded",
                            name = added_name,
                            bones = added_bones,
                            merged = merged_bones,
                            new = new_bones,
                            meshes = added_meshes,
                            materials = added_materials
                        )
                        .into_owned(),
                    ));
                }
            }
            Err(e) => {
                log::error!("GPU rebuild failed (merge rollback): {e}");
                loaded.ir.bones.truncate(saved_bone_count);
                loaded.ir.meshes.truncate(saved_mesh_count);
                loaded.ir.materials.truncate(saved_material_count);
                loaded.ir.textures.truncate(saved_texture_count);
                loaded.ir.morphs.truncate(saved_morph_count);
                loaded.ir.physics.rigid_bodies.truncate(saved_rigid_count);
                loaded.ir.physics.joints.truncate(saved_joint_count);
                loaded.ir.name = saved_name;
                loaded.ir.node_to_bone = saved_node_to_bone;
                loaded.ir.humanoid_bone_count = saved_humanoid_count;
                for (i, bone) in loaded.ir.bones.iter_mut().enumerate() {
                    if i < saved_bone_meta.len() {
                        bone.children = saved_bone_meta[i].0.clone();
                        bone.vrm_bone_name = saved_bone_meta[i].1.clone();
                    }
                }
                self.convert_message = Some(ConvertMessage::failure(
                    t!(
                        "viewer.toast.append.gpu_rebuild_failed",
                        error = e.to_string()
                    )
                    .into_owned(),
                ));
            }
        }
        // Normalize shader state outside the `loaded` borrow scope (preserving the user's selections).
        self.normalize_shader_state();
    }

    /// Drag-and-drop handling. Returns `(image_hovering, model_hovering)`.
    pub(super) fn process_drag_and_drop(&mut self, ctx: &egui::Context) -> (bool, bool) {
        let (dropped_files, hover_ext, shift_held) = ctx.input(|i| {
            let hover_ext = i
                .raw
                .hovered_files
                .first()
                .and_then(|f| f.path.as_ref())
                .and_then(|p| p.extension())
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();
            self.drag_hovering = !i.raw.hovered_files.is_empty();
            let paths: Vec<PathBuf> = i
                .raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect();
            (paths, hover_ext, i.modifiers.shift)
        });

        let is_hover_image = IMAGE_EXTENSIONS.contains(&hover_ext.as_str());
        let is_hover_model = MODEL_EXTENSIONS.contains(&hover_ext.as_str());

        if !dropped_files.is_empty() {
            let mut image_files: Vec<PathBuf> = Vec::new();
            let mut model_file: Option<PathBuf> = None;
            for path in dropped_files {
                let ext = crate::path_ext_lower(&path);
                if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
                    image_files.push(path);
                } else {
                    model_file = Some(path);
                }
            }

            let has_loaded_model = self.loaded.is_some();

            if let Some(model_path) = model_file {
                let append_ext = crate::path_ext_lower(&model_path);
                let is_appendable = matches!(
                    append_ext.as_str(),
                    "vrm"
                        | "fbx"
                        | "pmx"
                        | "pmd"
                        | "obj"
                        | "stl"
                        | "x"
                        | "unitypackage"
                        | "zip"
                        | "7z"
                );

                // Unify temp / non-temp paths through a single PendingLoadDispatch.
                // Pre-reading temp files is offloaded to the BG thread so the UI thread never blocks.
                let append = shift_held && has_loaded_model && is_appendable;
                self.pending
                    .bg_state
                    .submit_dispatch(super::pending::PendingLoadDispatch {
                        path: model_path,
                        append,
                        overlay: super::pending::PendingOverlay::WaitingOverlay,
                        preloaded: None,
                        is_reload: false,
                    });
            }

            if !image_files.is_empty() && has_loaded_model {
                if image_files.len() == 1 {
                    let path = image_files
                        .into_iter()
                        .next()
                        .expect("image_files is non-empty");
                    self.open_texture_preview(path);
                } else {
                    self.auto_assign_textures(image_files);
                }
            }
        }

        (is_hover_image, is_hover_model)
    }

    /// Keyboard shortcut handling.
    pub(super) fn process_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        use super::super::gpu::{DrawMode, LightMode};
        let wants_kb = ctx.wants_keyboard_input();
        ctx.input(|i| {
            if i.modifiers.ctrl && i.key_pressed(egui::Key::O) {
                self.open_file_dialog(ctx);
            }
            if !wants_kb {
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::R) {
                    if let Some(ref loaded) = self.loaded {
                        let (bbox_min, bbox_max) = loaded.gpu_model.bbox();
                        self.camera.reset_to_bbox_with_margin(
                            bbox_min,
                            bbox_max,
                            self.last_viewport_width,
                            self.last_viewport_height,
                        );
                    }
                }
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::F) {
                    if let Some(ref loaded) = self.loaded {
                        let (bbox_min, bbox_max) = loaded.gpu_model.bbox();
                        self.camera.fit_to_bbox_with_margin(
                            bbox_min,
                            bbox_max,
                            self.last_viewport_width,
                            self.last_viewport_height,
                        );
                    }
                }
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::G) {
                    self.display.show_grid = !self.display.show_grid;
                }
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::B) {
                    self.display.show_bones = !self.display.show_bones;
                }
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::P) {
                    self.display.show_spring_bones = !self.display.show_spring_bones;
                }
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::W) {
                    self.display.draw_mode = match self.display.draw_mode {
                        DrawMode::Solid => DrawMode::Wireframe,
                        DrawMode::Wireframe => DrawMode::SolidWireframe,
                        DrawMode::SolidWireframe => DrawMode::Solid,
                    };
                }
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::N) {
                    self.display.show_normals = !self.display.show_normals;
                }
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::L) {
                    self.display.light_mode = match self.display.light_mode {
                        LightMode::CameraFollow => LightMode::Fixed,
                        LightMode::Fixed => LightMode::CameraFollow,
                    };
                }
                {
                    let deg15 = 15.0_f32.to_radians();
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num0) {
                        self.camera.yaw = 0.0;
                        self.camera.pitch = 0.0;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num1) {
                        self.camera.yaw = std::f32::consts::FRAC_PI_2;
                        self.camera.pitch = 0.0;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num2) {
                        self.camera.pitch -= deg15;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num3) {
                        self.camera.yaw = -std::f32::consts::FRAC_PI_2;
                        self.camera.pitch = 0.0;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num4) {
                        self.camera.yaw += deg15;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num5) {
                        self.camera.perspective = !self.camera.perspective;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num6) {
                        self.camera.yaw -= deg15;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num7) {
                        self.camera.yaw = 0.0;
                        self.camera.pitch = std::f32::consts::FRAC_PI_2 - 0.01;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num8) {
                        self.camera.pitch += deg15;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num9) {
                        self.camera.yaw = std::f32::consts::PI;
                        self.camera.pitch = 0.0;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Period) {
                        if let Some(ref loaded) = self.loaded {
                            let (bbox_min, bbox_max) = loaded.gpu_model.bbox();
                            self.camera.fit_to_bbox_with_margin(
                                bbox_min,
                                bbox_max,
                                self.last_viewport_width,
                                self.last_viewport_height,
                            );
                        }
                    }
                }
                if i.key_pressed(egui::Key::Space) {
                    if let Some(ref mut anim) = self.anim.state {
                        anim.playing = !anim.playing;
                    }
                }
                if i.key_pressed(egui::Key::ArrowLeft) {
                    if let Some(ref mut anim) = self.anim.state {
                        if !anim.playing {
                            anim.step_frame(false);
                        }
                    }
                }
                if i.key_pressed(egui::Key::ArrowRight) {
                    if let Some(ref mut anim) = self.anim.state {
                        if !anim.playing {
                            anim.step_frame(true);
                        }
                    }
                }
            }
        });
    }
}
