//! Module for extracting assets from .unitypackage (tar.gz)

use crate::error::{PoponeError, Result, ResultExt};
use crate::intermediate::types::{SourceMaterialRef, TextureData};
use flate2::read::GzDecoder;
use rayon::prelude::*;
use rust_i18n::t;
use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};
use std::sync::Arc;

// ── Error type for Prefab texture mapping ──

/// Dedicated error type for unitypackage processing
#[derive(Debug, thiserror::Error)]
pub enum PkgError {
    #[error("Asset not found (GUID: {guid}, expected: {expected_type})")]
    AssetNotFound {
        guid: String,
        expected_type: &'static str,
    },
    #[error("Model not found: '{hint}' (expected: {expected_type}). Candidates: {}", candidates.join(", "))]
    ModelNotFound {
        hint: String,
        expected_type: &'static str,
        candidates: Vec<String>,
    },
    #[error("Ambiguous model: '{hint}' (expected: {expected_type}). Matches: {}", candidates.join(", "))]
    ModelAmbiguous {
        hint: String,
        expected_type: &'static str,
        candidates: Vec<String>,
    },
    #[error("Prefab parse failed: {path} (format={format}, parsed_count={parsed_count})")]
    PrefabParseFailed {
        path: String,
        format: &'static str,
        parsed_count: usize,
    },
    #[error("Circular Prefab variant: GUID {guid}")]
    CircularPrefabVariant { guid: String },
    #[error("Prefab variant chain too deep (>32): GUID {guid}")]
    PrefabVariantTooDeep { guid: String },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type for unitypackage internal processing
pub type PkgResult<T> = std::result::Result<T, PkgError>;

// ── Type definitions for Prefab texture mapping ──

/// Model file kind inside a unitypackage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PkgModelType {
    Fbx,
    Vrm,
    Prefab,
}

/// Unique key for model selection (uniquely identified by GUID + pathname)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PkgModelLocator {
    pub guid: std::sync::Arc<str>,
    pub pathname: std::sync::Arc<str>,
    pub kind: PkgModelType,
}

/// Display item for the model list
pub struct PkgModelListItem {
    pub locator: PkgModelLocator,
    pub label: std::sync::Arc<str>,
}

/// In-scene instance ID used to distinguish multiple appends
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PkgInstanceId(pub u32);

/// Fixed instance ID for the base model
pub const BASE_INSTANCE_ID: PkgInstanceId = PkgInstanceId(0);

/// Stable key used for reload/append restoration
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PkgMaterialKey {
    pub instance_id: PkgInstanceId,
    pub model_guid: std::sync::Arc<str>,
    pub source_material: Option<crate::intermediate::types::SourceMaterialRef>,
    pub material_name: std::sync::Arc<str>,
}

// ── Existing code ──

/// Tuple type of FBX data, filename, and texture list
pub type FbxWithTextures = (Arc<[u8]>, String, Vec<(String, Arc<[u8]>)>);

/// Information about an extracted asset
pub struct ExtractedAsset {
    /// Path inside the Unity project (e.g. "Assets/Models/xxx.fbx")
    pub pathname: String,
    /// Asset payload data (shared with AssetEntry via Arc to avoid double copies)
    pub data: Arc<[u8]>,
}

impl ExtractedAsset {
    /// Extract the filename portion from the path
    pub fn filename(&self) -> String {
        std::path::Path::new(&self.pathname)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    }
}

/// Asset entry inside a unitypackage (holds GUID, path, data, and meta information)
pub struct AssetEntry {
    pub guid: String,
    pub pathname: String,
    pub data: Arc<[u8]>,
    pub meta: Option<String>,
}

/// Index of a unitypackage (all assets + reverse lookup maps for GUID/path)
pub struct UnityPackageIndex {
    pub entries: Vec<AssetEntry>,
    pub by_guid: HashMap<String, usize>,
    pub by_path: HashMap<String, usize>,
    /// FBX GUID → .prefab entry indices that reference this FBX
    pub prefab_by_fbx_guid: HashMap<String, Vec<usize>>,
    /// Prefab parsed result cache (entry index → parsed result)
    pub prefab_cache: HashMap<usize, ParsedPrefabCache>,
    /// Variant resolution cache: source GUID → resolved FBX GUIDs
    pub variant_cache: HashMap<String, Vec<String>>,
}

/// Parsed prefab cache entry (corresponds to an entry index)
pub struct ParsedPrefabCache {
    pub format: PrefabFormat,
    pub new_infos: Vec<NewPrefabInfo>,
    pub old_infos: Vec<OldPrefabInfo>,
}

/// Extraction size limit: 2GB (same as the archive module)
const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Extracts all assets from a .unitypackage
/// Internally calls `build_unity_package_index()` and converts the result into `Vec<ExtractedAsset>`
pub fn extract_all_assets(archive_data: &[u8]) -> Result<Vec<ExtractedAsset>> {
    let index = build_unity_package_index(archive_data)?;
    let result = index
        .entries
        .into_iter()
        .map(|entry| ExtractedAsset {
            pathname: entry.pathname,
            data: entry.data,
        })
        .collect();
    Ok(result)
}

/// Builds an index from a .unitypackage (all assets + reverse lookup maps for GUID/path)
pub fn build_unity_package_index(archive_data: &[u8]) -> Result<UnityPackageIndex> {
    build_unity_package_index_with_limit(archive_data, MAX_TOTAL_BYTES)
}

/// Builds an index from a .unitypackage (with a specified size limit)
fn build_unity_package_index_with_limit(
    archive_data: &[u8],
    max_bytes: u64,
) -> Result<UnityPackageIndex> {
    let decoder = GzDecoder::new(Cursor::new(archive_data));
    let mut archive = tar::Archive::new(decoder);

    // Collect GUID -> (pathname, asset_data, meta)
    let mut pathnames: HashMap<String, String> = HashMap::new();
    let mut assets: HashMap<String, Vec<u8>> = HashMap::new();
    let mut metas: HashMap<String, String> = HashMap::new();
    let mut total_bytes: u64 = 0;

    for entry in archive
        .entries()
        .with_context(|| t!("error.unitypackage.tar_entries_failed").to_string())?
    {
        let mut entry =
            entry.with_context(|| t!("error.unitypackage.tar_entry_parse_failed").to_string())?;
        let path = entry
            .path()
            .with_context(|| t!("error.unitypackage.path_failed").to_string())?
            .to_string_lossy()
            .to_string();
        // The path is in "GUID/filename" form
        let parts: Vec<&str> = path.splitn(3, ['/', '\\']).collect();
        if parts.len() < 2 {
            continue;
        }
        let guid = parts[0].to_string();
        let filename = parts[1];

        match filename {
            "pathname" => {
                let mut s = String::new();
                entry
                    .read_to_string(&mut s)
                    .with_context(|| t!("error.unitypackage.pathname_read_failed").to_string())?;
                // B-9: also count bytes read for pathname
                total_bytes += s.len() as u64;
                if total_bytes > max_bytes {
                    return Err(PoponeError::UnityPackage(
                        t!(
                            "error.unitypackage.size_limit_exceeded",
                            limit_mb = (max_bytes / (1024 * 1024)).to_string()
                        )
                        .to_string(),
                    ));
                }
                pathnames.insert(guid, s.trim().to_string());
            }
            "asset" => {
                let entry_size = entry.header().size().unwrap_or(0);
                if total_bytes.saturating_add(entry_size) > max_bytes {
                    return Err(PoponeError::UnityPackage(
                        t!(
                            "error.unitypackage.size_limit_exceeded",
                            limit_mb = (max_bytes / (1024 * 1024)).to_string()
                        )
                        .to_string(),
                    ));
                }
                let mut data = Vec::new();
                entry
                    .read_to_end(&mut data)
                    .with_context(|| t!("error.unitypackage.asset_read_failed").to_string())?;
                total_bytes += data.len() as u64;
                if total_bytes > max_bytes {
                    return Err(PoponeError::UnityPackage(
                        t!(
                            "error.unitypackage.size_limit_exceeded",
                            limit_mb = (max_bytes / (1024 * 1024)).to_string()
                        )
                        .to_string(),
                    ));
                }
                assets.insert(guid, data);
            }
            "asset.meta" => {
                let mut data = Vec::new();
                entry
                    .read_to_end(&mut data)
                    .with_context(|| t!("error.unitypackage.asset_meta_read_failed").to_string())?;
                // B-9: also count bytes read for asset.meta
                total_bytes += data.len() as u64;
                if total_bytes > max_bytes {
                    return Err(PoponeError::UnityPackage(
                        t!(
                            "error.unitypackage.size_limit_exceeded",
                            limit_mb = (max_bytes / (1024 * 1024)).to_string()
                        )
                        .to_string(),
                    ));
                }
                let cow = String::from_utf8_lossy(&data);
                if matches!(&cow, std::borrow::Cow::Owned(_)) {
                    log::warn!("asset.meta (GUID={}) contains invalid UTF-8", guid);
                }
                metas.insert(guid, cow.into_owned());
            }
            _ => {} // ignore preview.png etc.
        }
    }

    // Combine pathname and asset to build AssetEntry
    let mut entries = Vec::new();
    let mut by_guid = HashMap::new();
    let mut by_path = HashMap::new();

    for (guid, pathname) in pathnames {
        if let Some(data) = assets.remove(&guid) {
            let idx = entries.len();
            let meta = metas.remove(&guid);
            by_guid.insert(guid.clone(), idx);
            by_path.insert(pathname.clone(), idx);
            entries.push(AssetEntry {
                guid,
                pathname,
                data: Arc::from(data),
                meta,
            });
        }
    }

    log::debug!(
        "UnityPackageIndex built: {} entries, total {}KB",
        entries.len(),
        total_bytes / 1024
    );

    let mut index = UnityPackageIndex {
        entries,
        by_guid,
        by_path,
        prefab_by_fbx_guid: HashMap::new(),
        prefab_cache: HashMap::new(),
        variant_cache: HashMap::new(),
    };

    // Post-process: build prefab → FBX GUID map and cache parsed results
    build_prefab_fbx_map(&mut index);

    Ok(index)
}

/// Searches the asset index by full path (pathname).
/// Used for accurate GUID/path-based reselection via `selected_pkg_model.pathname`.
pub fn find_asset_by_pathname(assets: &[ExtractedAsset], pathname: &str) -> Option<usize> {
    assets.iter().position(|a| a.pathname == pathname)
}

/// Returns the list of FBX assets from already-extracted assets.
/// Returns: [(asset index, filename)]
pub fn find_fbx_list(assets: &[ExtractedAsset]) -> Vec<(usize, String)> {
    assets
        .iter()
        .enumerate()
        .filter(|(_, a)| a.pathname.to_lowercase().ends_with(".fbx"))
        .map(|(i, a)| (i, a.filename()))
        .collect()
}

/// Returns the specified FBX and its textures by reference from extracted assets
pub fn take_fbx_and_textures(
    assets: &[ExtractedAsset],
    fbx_index: usize,
) -> Result<FbxWithTextures> {
    if fbx_index >= assets.len() {
        return Err(PoponeError::UnityPackage(
            t!(
                "error.unitypackage.fbx_index_out_of_range",
                index = fbx_index.to_string()
            )
            .to_string(),
        ));
    }

    let fbx_asset = &assets[fbx_index];
    let fbx_name = fbx_asset.filename();
    let fbx_data = Arc::clone(&fbx_asset.data);

    // Collect textures (image files), excluding the FBX itself
    let texture_exts = ["png", "jpg", "jpeg", "tga", "bmp", "psd", "tif", "tiff"];
    let textures: Vec<(String, Arc<[u8]>)> = assets
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != fbx_index)
        .filter(|(_, a)| {
            let lower = a.pathname.to_lowercase();
            texture_exts.iter().any(|ext| lower.ends_with(ext))
        })
        .map(|(_, a)| {
            let name = a.filename();
            (name, Arc::clone(&a.data))
        })
        .collect();

    log::info!(
        ".unitypackage extract: FBX={} ({}KB), textures={}",
        fbx_name,
        fbx_data.len() / 1024,
        textures.len(),
    );

    Ok((fbx_data, fbx_name, textures))
}

/// Returns the list of VRM assets from already-extracted assets.
/// Returns: [(asset index, filename)]
pub fn find_vrm_list(assets: &[ExtractedAsset]) -> Vec<(usize, String)> {
    assets
        .iter()
        .enumerate()
        .filter(|(_, a)| a.pathname.to_lowercase().ends_with(".vrm"))
        .map(|(i, a)| (i, a.filename()))
        .collect()
}

/// Returns the specified VRM by reference from extracted assets
pub fn take_vrm(assets: &[ExtractedAsset], vrm_index: usize) -> Result<(Arc<[u8]>, String)> {
    if vrm_index >= assets.len() {
        return Err(PoponeError::UnityPackage(
            t!(
                "error.unitypackage.vrm_index_out_of_range",
                index = vrm_index.to_string()
            )
            .to_string(),
        ));
    }
    let vrm_asset = &assets[vrm_index];
    let vrm_name = vrm_asset.filename();
    let vrm_data = Arc::clone(&vrm_asset.data);
    log::info!(
        ".unitypackage extract: VRM={} ({}KB)",
        vrm_name,
        vrm_data.len() / 1024,
    );
    Ok((vrm_data, vrm_name))
}

/// Finds and extracts an FBX from a .unitypackage (CLI use).
/// If fbx_name is given, uses that FBX; otherwise uses the first FBX.
/// Returns: (FBX data, FBX filename, texture list [(pathname, data)])
pub fn extract_fbx_from_unitypackage(
    archive_data: &[u8],
    fbx_name: Option<&str>,
) -> Result<FbxWithTextures> {
    let assets = extract_all_assets(archive_data)?;
    let fbx_list = find_fbx_list(&assets);

    if fbx_list.is_empty() {
        return Err(PoponeError::UnityPackage(
            ".unitypackage 内に FBX ファイルが見つかりません".into(),
        ));
    }

    // Log if multiple FBX files exist
    if fbx_list.len() > 1 {
        log::info!("Found {} FBX files in .unitypackage:", fbx_list.len(),);
        for (_, name) in &fbx_list {
            log::info!("  FBX: {}", name);
        }
    }

    // If an FBX name was specified, use it
    let selected_idx = if let Some(target) = fbx_name {
        let target_lower = target.to_lowercase();
        fbx_list
            .iter()
            .find(|(_, name)| name.to_lowercase().contains(&target_lower))
            .map(|(idx, _)| *idx)
            .ok_or_else(|| {
                let candidates = fbx_list
                    .iter()
                    .map(|(_, n)| n.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                PoponeError::UnityPackage(
                    t!(
                        "error.unitypackage.fbx_not_found",
                        name = target.to_string(),
                        candidates = candidates
                    )
                    .to_string(),
                )
            })?
    } else {
        fbx_list[0].0
    };

    take_fbx_and_textures(&assets, selected_idx)
}

/// Automatically assigns textures inside the unitypackage to materials of an IrModel.
/// Matches each material's source_texture_name against texture filenames.
/// Returns: list of indices of unassigned materials
pub fn embed_textures_into_ir<T: AsRef<[u8]>>(
    ir: &mut crate::intermediate::types::IrModel,
    textures: &[(String, T)],
) -> Vec<usize> {
    embed_textures_into_ir_with_label(ir, textures, "package")
}

/// Texture embedding (variant with explicit source label)
pub fn embed_textures_into_ir_with_label<T: AsRef<[u8]>>(
    ir: &mut crate::intermediate::types::IrModel,
    textures: &[(String, T)],
    source_label: &str,
) -> Vec<usize> {
    if textures.is_empty() {
        return (0..ir.materials.len()).collect();
    }

    // Filename -> data map (lowercase keys)
    let tex_map: HashMap<String, &[u8]> = textures
        .iter()
        .map(|(name, data)| (name.to_lowercase(), data.as_ref()))
        .collect();

    // Stem (without extension) -> full-key reverse map (speeds up stem matching)
    let stem_map: HashMap<String, String> = tex_map
        .keys()
        .map(|k| {
            let stem = std::path::Path::new(k.as_str())
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            (stem, k.clone())
        })
        .collect();

    let mut matched = 0usize;
    for mat in &mut ir.materials {
        let src_name = match mat.source_texture_name.as_deref() {
            Some(name) if !name.is_empty() => name.to_lowercase(),
            _ => continue,
        };
        // Exact match -> fall back to stem match
        let found_key = if tex_map.contains_key(&src_name) {
            Some(src_name.clone())
        } else {
            let stem = std::path::Path::new(&src_name)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            stem_map.get(stem.as_str()).cloned()
        };

        if let Some(ref key) = found_key {
            if let Some(data) = tex_map.get(key) {
                let tex_idx = ir.textures.len();
                let ext = crate::path_ext_lower(std::path::Path::new(key.as_str()));
                let mime = crate::intermediate::types::mime_for_ext(&ext).to_string();
                ir.textures.push(crate::intermediate::types::IrTexture {
                    filename: key.clone(),
                    data: TextureData::Encoded(Arc::from(data.to_vec())),
                    mime_type: mime,
                    source_path: format!("{}: {}", source_label, key),
                    mip_chain: None,
                });
                mat.texture_index = Some(tex_idx);
                matched += 1;
                log::info!(
                    "Texture assigned: {} -> mat[{}]",
                    mat.source_texture_name.as_deref().unwrap_or("?"),
                    mat.name,
                );
            }
        }
    }

    // Collect indices of unassigned materials
    let unmatched: Vec<usize> = ir
        .materials
        .iter()
        .enumerate()
        .filter(|(_, mat)| mat.texture_index.is_none())
        .map(|(i, _)| i)
        .collect();

    log::info!(
        "Unitypackage textures: {}/{} materials matched, unassigned: {}",
        matched,
        ir.materials.len(),
        unmatched.len()
    );
    unmatched
}

// ── Step 6: Helper functions + parsers ──

/// Extracts the 32-character hex following "guid: " from a line
fn extract_guid_from_line(line: &str) -> Option<&str> {
    let idx = line.find("guid: ")?;
    let start = idx + 6; // "guid: ".len()
    let rest = line.get(start..)?;
    // Get the 32-character hex part (delimited by comma etc.)
    let end = rest
        .find(|c: char| !c.is_ascii_hexdigit())
        .unwrap_or(rest.len());
    if end >= 32 {
        Some(&rest[..32])
    } else {
        None
    }
}

/// Extracts N from "data[N]"
fn extract_array_index(line: &str) -> Option<usize> {
    let idx = line.find("data[")?;
    let start = idx + 5; // "data[".len()
    let rest = line.get(start..)?;
    let end = rest.find(']')?;
    rest[..end].parse().ok()
}

/// Decodes Unity YAML `\uXXXX` escape sequences
fn decode_unity_escape(s: &str) -> String {
    if !s.contains("\\u") {
        return s.to_string();
    }
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some('u') = chars.next() {
                let hex: String = chars.by_ref().take(4).collect();
                if hex.len() == 4 {
                    if let Ok(code) = u32::from_str_radix(&hex, 16) {
                        if let Some(decoded) = char::from_u32(code) {
                            result.push(decoded);
                            continue;
                        }
                    }
                }
                // On decode failure, keep as-is
                result.push('\\');
                result.push('u');
                result.push_str(&hex);
            } else {
                result.push('\\');
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Prefab format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefabFormat {
    New,
    Old,
}

/// Detects the Prefab format.
/// If a standalone `PrefabInstance:` appears at the start of a line, it is the new format.
/// Detection is performed line-by-line to avoid mismatching fields such as `m_PrefabInstance: {fileID: 0}`.
fn detect_prefab_format(content: &str) -> PrefabFormat {
    if content.lines().any(|line| line.trim() == "PrefabInstance:") {
        PrefabFormat::New
    } else {
        PrefabFormat::Old
    }
}

/// Parse result for the new-format Prefab
#[derive(Clone)]
pub struct NewPrefabInfo {
    pub source_fbx_guid: String,
    pub material_overrides: Vec<MaterialOverride>,
}

/// Material override (slot index + material GUID)
#[derive(Clone)]
pub struct MaterialOverride {
    pub slot_index: usize,
    pub material_guid: String,
}

/// Parses the new-format Prefab.
///
/// In the new format, `m_Modifications` (override list) appears first within a `PrefabInstance:`
/// block, and `m_SourcePrefab:` appears later. Therefore a two-pass approach is used:
/// 1. Accumulate overrides
/// 2. When `m_SourcePrefab:` is seen, associate the accumulated overrides and produce a result
fn parse_prefab_new(content: &str) -> PkgResult<Vec<NewPrefabInfo>> {
    let mut results: Vec<NewPrefabInfo> = Vec::new();
    let mut current_overrides: Vec<MaterialOverride> = Vec::new();
    let mut pending_slot_index: Option<usize> = None;
    let mut in_prefab_instance = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // PrefabInstance: block start
        if trimmed == "PrefabInstance:" {
            // Reset accumulators when a new PrefabInstance block begins
            current_overrides.clear();
            pending_slot_index = None;
            in_prefab_instance = true;
            continue;
        }

        if !in_prefab_instance {
            continue;
        }

        // propertyPath: m_Materials.Array.data[N]
        if trimmed.contains("propertyPath: m_Materials.Array.data[") {
            pending_slot_index = extract_array_index(trimmed);
            continue;
        }

        // objectReference: {fileID: ..., guid: XXXX, ...}
        if pending_slot_index.is_some() && trimmed.contains("objectReference:") {
            let slot_idx = pending_slot_index.take().expect("is_some() チェック済み");
            if let Some(guid) = extract_guid_from_line(trimmed) {
                current_overrides.push(MaterialOverride {
                    slot_index: slot_idx,
                    material_guid: guid.to_string(),
                });
            }
            continue;
        }

        // Pick up the GUID from m_SourcePrefab: and bind it to the accumulated overrides
        if trimmed.contains("m_SourcePrefab:") {
            if let Some(guid) = extract_guid_from_line(trimmed) {
                results.push(NewPrefabInfo {
                    source_fbx_guid: guid.to_string(),
                    material_overrides: std::mem::take(&mut current_overrides),
                });
            }
            in_prefab_instance = false;
            continue;
        }
    }

    Ok(results)
}

/// Parse result for the old-format Prefab
#[derive(Clone)]
pub struct OldPrefabInfo {
    pub fbx_guid: String,
    pub material_guids: Vec<String>,
}

/// Parses the old-format Prefab
fn parse_prefab_old(content: &str) -> PkgResult<Vec<OldPrefabInfo>> {
    let mut results: Vec<OldPrefabInfo> = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Detect SkinnedMeshRenderer section
        if trimmed.starts_with("--- !u!137") {
            let mut mat_guids: Vec<String> = Vec::new();
            let mut mesh_guid: Option<String> = None;
            let mut in_materials = false;
            i += 1;

            while i < lines.len() {
                let line = lines[i];
                let lt = line.trim();

                // Next object boundary
                if lt.starts_with("--- ") {
                    break;
                }

                // m_Materials: (multi-line list) or m_Materials: [] (empty inline)
                if lt.starts_with("m_Materials:") {
                    // Skip empty inline forms like "m_Materials: []"
                    if !lt.contains("[]") {
                        in_materials = true;
                    }
                    i += 1;
                    continue;
                }

                if in_materials {
                    if lt.starts_with("- {") && lt.contains("guid:") {
                        if let Some(guid) = extract_guid_from_line(lt) {
                            mat_guids.push(guid.to_string());
                        }
                        i += 1;
                        continue;
                    } else {
                        in_materials = false;
                    }
                }

                if lt.starts_with("m_Mesh:") && lt.contains("guid:") {
                    mesh_guid = extract_guid_from_line(lt).map(|s| s.to_string());
                }

                i += 1;
            }

            if let Some(fg) = mesh_guid {
                // If m_Mesh exists, this is a valid FBX reference even when m_Materials is empty
                results.push(OldPrefabInfo {
                    fbx_guid: fg,
                    material_guids: mat_guids,
                });
            }
            continue;
        }

        i += 1;
    }

    Ok(results)
}

/// Material information inside an FBX .meta
struct FbxMetaMaterial {
    material_name: String,
    material_guid: String,
}

/// Parses an FBX .meta and returns (material list, materialImportMode)
fn parse_fbx_meta(meta_content: &str) -> PkgResult<(Vec<FbxMetaMaterial>, Option<u32>)> {
    let mut materials: Vec<FbxMetaMaterial> = Vec::new();
    let mut import_mode: Option<u32> = None;
    let mut in_external_objects = false;
    let mut current_name: Option<String> = None;

    for line in meta_content.lines() {
        let trimmed = line.trim();

        if trimmed == "externalObjects:" {
            in_external_objects = true;
            continue;
        }

        // Detect end of externalObjects section (when indent level returns)
        if in_external_objects
            && !line.starts_with(' ')
            && !line.starts_with('\t')
            && !trimmed.is_empty()
        {
            in_external_objects = false;
        }

        if in_external_objects {
            // Material name starting with `name:` (decodes Unity YAML \uXXXX escapes, strips quotes)
            if trimmed.starts_with("name:") {
                let val = trimmed
                    .strip_prefix("name:")
                    .expect("starts_with チェック済み")
                    .trim()
                    .trim_matches('"');
                current_name = Some(decode_unity_escape(val));
                continue;
            }
            // GUID prefixed with `second:`
            if trimmed.starts_with("second:") && current_name.is_some() {
                if let Some(guid) = extract_guid_from_line(trimmed) {
                    let name = current_name
                        .take()
                        .expect("current_name.is_some() チェック済み");
                    log::debug!("  externalObjects: name='{}' -> guid={}", name, guid);
                    materials.push(FbxMetaMaterial {
                        material_name: name,
                        material_guid: guid.to_string(),
                    });
                } else {
                    current_name = None;
                }
                continue;
            }
        }

        // materialImportMode:
        if trimmed.starts_with("materialImportMode:") {
            let val = trimmed
                .strip_prefix("materialImportMode:")
                .expect("starts_with チェック済み")
                .trim();
            import_mode = val.parse().ok();
        }
    }

    // The materialImportMode value is logged only (we always look at externalObjects regardless)
    if let Some(mode) = import_mode {
        if mode != 2 {
            log::info!(
                "FBX .meta: materialImportMode={} (!=2) but using externalObjects ({} entries)",
                mode,
                materials.len()
            );
        }
    }

    Ok((materials, import_mode))
}

/// Texture-slot info inside a `.mat` file.
struct MatTextureSlot {
    slot_name: String,
    texture_guid: String,
}

/// Float-parameter info inside a `.mat` file.
struct MatFloatParam {
    param_name: String,
    value: f32,
}

/// Color-parameter info inside a `.mat` file.
#[expect(dead_code)]
struct MatColorParam {
    param_name: String,
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

/// Parse result for a `.mat` file.
struct ParsedMaterial {
    name: String,
    textures: Vec<MatTextureSlot>,
    floats: Vec<MatFloatParam>,
    colors: Vec<MatColorParam>,
    /// Keywords contained in `m_ShaderKeywords` / `m_ValidKeywords`.
    shader_keywords: Vec<String>,
}

/// Section kinds inside `m_SavedProperties`.
enum MatSection {
    None,
    TexEnvs,
    Floats,
    Colors,
    Keywords,
}

/// Extract the material name, texture slots, and float parameters from a `.mat` file.
fn parse_material_textures(mat_content: &str) -> PkgResult<ParsedMaterial> {
    let mut name = String::new();
    let mut textures: Vec<MatTextureSlot> = Vec::new();
    let mut floats: Vec<MatFloatParam> = Vec::new();
    let mut colors: Vec<MatColorParam> = Vec::new();
    let mut shader_keywords: Vec<String> = Vec::new();
    let mut section = MatSection::None;
    let mut current_slot: Option<String> = None;

    for line in mat_content.lines() {
        let trimmed = line.trim();

        // m_Name: (handled even outside sections)
        if trimmed.starts_with("m_Name:") {
            name = trimmed
                .strip_prefix("m_Name:")
                .expect("starts_with チェック済み")
                .trim()
                .to_string();
            continue;
        }

        // m_ShaderKeywords / m_ValidKeywords: either inline "KEY1 KEY2 KEY3" or a multi-line list
        if trimmed.starts_with("m_ShaderKeywords:") || trimmed.starts_with("m_ValidKeywords:") {
            let val = trimmed.split_once(':').map(|(_, v)| v.trim()).unwrap_or("");
            if val.is_empty() || val == "[]" {
                // No value or empty array -> may be the multi-line list form, so switch sections
                if val != "[]" {
                    section = MatSection::Keywords;
                }
            } else {
                // Inline form: "KEY1 KEY2 KEY3"
                let val = val.trim_matches('"').trim_matches('\'');
                for kw in val.split_whitespace() {
                    if !shader_keywords.contains(&kw.to_string()) {
                        shader_keywords.push(kw.to_string());
                    }
                }
            }
            continue;
        }

        // Section switching
        if trimmed == "m_TexEnvs:" {
            section = MatSection::TexEnvs;
            continue;
        }
        if trimmed == "m_Floats:" {
            section = MatSection::Floats;
            continue;
        }
        if trimmed == "m_Colors:" {
            section = MatSection::Colors;
            continue;
        }
        // Inline empty array
        if trimmed == "m_Floats: []" || trimmed == "m_TexEnvs: []" || trimmed == "m_Colors: []" {
            section = MatSection::None;
            continue;
        }
        // End the section when another `m_` header appears
        if trimmed.starts_with("m_")
            && !trimmed.starts_with("m_Texture:")
            && !trimmed.starts_with("m_Scale:")
            && !trimmed.starts_with("m_Offset:")
        {
            section = MatSection::None;
        }

        match section {
            MatSection::TexEnvs => {
                // Detect a slot name in the form "- _SlotName:"
                if trimmed.starts_with("- _") {
                    // "- _MainTex:" → "_MainTex"
                    let Some(stripped) = trimmed.strip_prefix("- ") else {
                        continue;
                    };
                    let slot = stripped.trim_end_matches(':').to_string();
                    current_slot = Some(slot);
                    continue;
                }

                // m_Texture: line
                if trimmed.starts_with("m_Texture:") {
                    if let Some(ref slot) = current_slot {
                        // fileID: 0 means no texture is set, so skip
                        if trimmed.contains("fileID: 0") {
                            current_slot = None;
                            continue;
                        }
                        // fileID: 2800000 + guid
                        if trimmed.contains("fileID: 2800000") {
                            if let Some(guid) = extract_guid_from_line(trimmed) {
                                textures.push(MatTextureSlot {
                                    slot_name: slot.clone(),
                                    texture_guid: guid.to_string(),
                                });
                            }
                        }
                        current_slot = None;
                    }
                    continue;
                }
            }
            MatSection::Floats => {
                // "- _BumpScale: 1" → param_name="_BumpScale", value=1.0
                if trimmed.starts_with("- _") {
                    if let Some((param_name, val_str)) =
                        trimmed.strip_prefix("- ").and_then(|s| s.split_once(':'))
                    {
                        if let Ok(v) = val_str.trim().parse::<f32>() {
                            floats.push(MatFloatParam {
                                param_name: param_name.trim().to_string(),
                                value: v,
                            });
                        }
                    }
                }
            }
            MatSection::Colors => {
                // "- _EmissionColor: {r: 1, g: 0.5, b: 0.2, a: 1}"
                if trimmed.starts_with("- _") {
                    if let Some((param_name, val_str)) =
                        trimmed.strip_prefix("- ").and_then(|s| s.split_once(':'))
                    {
                        let val_str = val_str.trim();
                        // Parse {r: R, g: G, b: B, a: A}
                        if let Some(color) = parse_unity_color(val_str) {
                            colors.push(MatColorParam {
                                param_name: param_name.trim().to_string(),
                                r: color.0,
                                g: color.1,
                                b: color.2,
                                a: color.3,
                            });
                        }
                    }
                }
            }
            MatSection::Keywords => {
                // Multi-line list form: "- _EMISSION"
                if let Some(kw) = trimmed.strip_prefix("- ") {
                    let kw = kw.trim();
                    if !kw.is_empty() && !shader_keywords.contains(&kw.to_string()) {
                        shader_keywords.push(kw.to_string());
                    }
                } else if !trimmed.starts_with('-') {
                    // End the section when a non-list-item line appears
                    section = MatSection::None;
                }
            }
            MatSection::None => {}
        }
    }

    Ok(ParsedMaterial {
        name,
        textures,
        floats,
        colors,
        shader_keywords,
    })
}

/// Parse a Unity color value of the form `{r: R, g: G, b: B, a: A}`.
fn parse_unity_color(s: &str) -> Option<(f32, f32, f32, f32)> {
    let s = s.trim().strip_prefix('{')?.strip_suffix('}')?;
    let mut r = 0.0f32;
    let mut g = 0.0f32;
    let mut b = 0.0f32;
    let mut a = 1.0f32;
    for part in s.split(',') {
        let part = part.trim();
        if let Some((key, val)) = part.split_once(':') {
            let val = val.trim().parse::<f32>().ok()?;
            match key.trim() {
                "r" => r = val,
                "g" => g = val,
                "b" => b = val,
                "a" => a = val,
                _ => {}
            }
        }
    }
    Some((r, g, b, a))
}

// ── Step 7: resolve_prefab_textures ──

/// Material-texture info resolved through Prefab references.
pub struct ResolvedMaterialTextures {
    pub source_material: Option<SourceMaterialRef>,
    pub material_name: Arc<str>,
    pub main_texture_guid: Option<Arc<str>>,
    /// Normal-map texture GUID (_BumpMap > _NormalMap, in priority order).
    pub normal_texture_guid: Option<Arc<str>>,
    /// Normal-map scale (`_BumpScale`, default 1.0).
    pub bump_scale: f32,
    /// FBX-internal material name listed in the FBX `.meta` `externalObjects` (matches the IrModel material name).
    pub fbx_material_name: Option<Arc<str>>,
    /// Emission-texture GUID (`_EmissionMap`).
    pub emission_texture_guid: Option<Arc<str>>,
    /// Emission color (r, g, b) (`_EmissionColor`; default black means disabled).
    pub emission_color: [f32; 3],
    /// Emission-enabled flag (`_Emission` float == 1.0).
    pub emission_enabled: bool,
    /// Emission blend mode (lilToon: 0 = Add, 1 = Screen; default 0).
    pub emission_blend: u8,
}

/// Prefab candidate (path + resolved material list).
struct PrefabCandidate {
    prefab_path: String,
    materials: Vec<ResolvedMaterialTextures>,
}

/// Similarity score between a Prefab path and an FBX path.
fn score_prefab_path(prefab_path: &str, fbx_path: &str) -> usize {
    // Score by the length of the shared prefix
    let prefab_parts: Vec<&str> = prefab_path.split('/').collect();
    let fbx_parts: Vec<&str> = fbx_path.split('/').collect();
    let mut score = 0;
    for (a, b) in prefab_parts.iter().zip(fbx_parts.iter()) {
        if a == b {
            score += a.len() + 1; // Account for the path separator too
        } else {
            break;
        }
    }
    score
}

/// Pick the best Prefab from multiple candidates.
fn choose_prefab<'a>(
    candidates: &'a [PrefabCandidate],
    fbx_path: &str,
) -> Option<&'a PrefabCandidate> {
    if candidates.len() == 1 {
        return Some(&candidates[0]);
    }
    candidates
        .iter()
        .max_by_key(|c| score_prefab_path(&c.prefab_path, fbx_path))
}

/// Recursive resolution of Variant Prefabs (source_prefab GUID -> original FBX GUID).
pub fn resolve_variant(pkg: &UnityPackageIndex, guid: &str) -> PkgResult<Option<String>> {
    let guids = resolve_variant_multi(pkg, guid)?;
    Ok(guids.into_iter().next())
}

/// Resolve Variant Prefabs recursively and return every referenced FBX GUID.
/// Also supports mixed-format Prefabs (PrefabInstance + SkinnedMeshRenderer co-existing).
pub fn resolve_variant_multi(pkg: &UnityPackageIndex, guid: &str) -> PkgResult<Vec<String>> {
    let mut visited = HashSet::new();
    resolve_variant_multi_inner(pkg, guid, 0, &mut visited)
}

/// Cached version of resolve_variant_multi: checks variant_cache first.
fn resolve_variant_multi_cached(pkg: &UnityPackageIndex, guid: &str) -> PkgResult<Vec<String>> {
    if let Some(cached) = pkg.variant_cache.get(guid) {
        return Ok(cached.clone());
    }
    resolve_variant_multi(pkg, guid)
}

fn resolve_variant_multi_inner(
    pkg: &UnityPackageIndex,
    guid: &str,
    depth: usize,
    visited: &mut HashSet<String>,
) -> PkgResult<Vec<String>> {
    if !visited.insert(guid.to_string()) {
        return Err(PkgError::CircularPrefabVariant { guid: guid.into() });
    }
    if depth > 32 {
        return Err(PkgError::PrefabVariantTooDeep { guid: guid.into() });
    }

    // Resolve the pathname from the GUID
    let entry_idx = match pkg.by_guid.get(guid) {
        Some(&idx) => idx,
        None => {
            log::debug!("resolve_variant: guid={} -> no entry", guid);
            return Ok(Vec::new());
        }
    };
    let pathname = &pkg.entries[entry_idx].pathname;
    let lower = pathname.to_lowercase();

    log::debug!(
        "resolve_variant: guid={} -> pathname={} (depth={})",
        guid,
        pathname,
        depth
    );

    if lower.ends_with(".fbx") {
        return Ok(vec![guid.to_string()]);
    }

    if lower.ends_with(".prefab") {
        let mut results: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        // Use prefab_cache if available to avoid re-parsing
        if let Some(cached) = pkg.prefab_cache.get(&entry_idx) {
            log::debug!(
                "resolve_variant: Prefab {} format={:?} (cached)",
                pathname,
                cached.format
            );

            // New format: recurse into source_fbx_guid references
            if cached.format == PrefabFormat::New {
                // Collect source GUIDs first to avoid borrow conflict with pkg
                let source_guids: Vec<String> = cached
                    .new_infos
                    .iter()
                    .map(|info| info.source_fbx_guid.clone())
                    .collect();
                log::debug!("resolve_variant: New Prefab infos={}", source_guids.len());
                for (i, sg) in source_guids.iter().enumerate() {
                    log::debug!("resolve_variant:   [{}] source_guid={}", i, sg);
                }
                for sg in &source_guids {
                    let sub = resolve_variant_multi_inner(pkg, sg, depth + 1, visited)?;
                    for g in sub {
                        if seen.insert(g.clone()) {
                            results.push(g);
                        }
                    }
                }
            }

            // Old format: direct FBX GUID references
            {
                let fbx_guids: Vec<String> = cached
                    .old_infos
                    .iter()
                    .map(|info| info.fbx_guid.clone())
                    .collect();
                log::debug!("resolve_variant: Old Prefab infos={}", fbx_guids.len());
                for (i, fg) in fbx_guids.iter().enumerate() {
                    log::debug!("resolve_variant:   [{}] fbx_guid={}", i, fg);
                }
                for fg in &fbx_guids {
                    if seen.insert(fg.clone()) {
                        results.push(fg.clone());
                    }
                }
            }
        } else {
            // Fallback: parse from raw data (shouldn't happen after index build)
            let data = &pkg.entries[entry_idx].data;
            let content = String::from_utf8_lossy(data);
            let format = detect_prefab_format(&content);
            log::debug!(
                "resolve_variant: Prefab {} format={:?} (uncached fallback)",
                pathname,
                format
            );

            if format == PrefabFormat::New {
                if let Ok(infos) = parse_prefab_new(&content) {
                    log::debug!("resolve_variant: New Prefab infos={}", infos.len());
                    for (i, info) in infos.iter().enumerate() {
                        log::debug!(
                            "resolve_variant:   [{}] source_guid={}",
                            i,
                            info.source_fbx_guid
                        );
                    }
                    for info in &infos {
                        let sub = resolve_variant_multi_inner(
                            pkg,
                            &info.source_fbx_guid,
                            depth + 1,
                            visited,
                        )?;
                        for g in sub {
                            if seen.insert(g.clone()) {
                                results.push(g);
                            }
                        }
                    }
                }
            }

            {
                if let Ok(infos) = parse_prefab_old(&content) {
                    log::debug!("resolve_variant: Old Prefab infos={}", infos.len());
                    for (i, info) in infos.iter().enumerate() {
                        log::debug!("resolve_variant:   [{}] fbx_guid={}", i, info.fbx_guid);
                    }
                    for info in &infos {
                        if seen.insert(info.fbx_guid.clone()) {
                            results.push(info.fbx_guid.clone());
                        }
                    }
                }
            }
        }

        if !results.is_empty() {
            return Ok(results);
        }
    }

    log::debug!(
        "resolve_variant: guid={} is not .fbx/.prefab, returning empty",
        guid
    );
    Ok(Vec::new())
}

/// Build prefab_by_fbx_guid map and prefab_cache from all .prefab entries.
/// Called once after the index is initially constructed.
fn build_prefab_fbx_map(index: &mut UnityPackageIndex) {
    // Collect prefab data slices for parallel parsing (borrows index.entries immutably)
    let prefab_data: Vec<(usize, &[u8], &str)> = index
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.pathname.to_lowercase().ends_with(".prefab"))
        .map(|(i, e)| (i, e.data.as_ref() as &[u8], e.pathname.as_str()))
        .collect();

    // Phase 1 (parallel): Parse all prefabs using rayon
    let parsed_results: Vec<(usize, ParsedPrefabCache)> = prefab_data
        .par_iter()
        .map(|&(idx, data, pathname)| {
            let content = String::from_utf8_lossy(data).into_owned();
            let format = detect_prefab_format(&content);

            let new_infos = if format == PrefabFormat::New {
                parse_prefab_new(&content).unwrap_or_else(|e| {
                    log::warn!("Prefab parse failed ({}): {}", pathname, e);
                    Vec::new()
                })
            } else {
                Vec::new()
            };

            // Always parse Old format too: mixed-format Prefabs have both
            // PrefabInstance and SkinnedMeshRenderer sections
            let old_infos = parse_prefab_old(&content).unwrap_or_else(|e| {
                if format == PrefabFormat::Old {
                    log::warn!("Old Prefab parse failed ({}): {}", pathname, e);
                }
                Vec::new()
            });

            (
                idx,
                ParsedPrefabCache {
                    format,
                    new_infos,
                    old_infos,
                },
            )
        })
        .collect();

    // Phase 2a: Insert all parsed caches first so resolve_variant_multi_inner can find them
    for (prefab_idx, cache) in &parsed_results {
        index.prefab_cache.insert(
            *prefab_idx,
            ParsedPrefabCache {
                format: cache.format,
                new_infos: cache.new_infos.clone(),
                old_infos: cache.old_infos.clone(),
            },
        );
    }

    // Phase 2b (sequential): Variant resolution + map building (needs &UnityPackageIndex)
    for (prefab_idx, cache) in &parsed_results {
        // New format: resolve variant chains to find actual FBX GUIDs
        for info in &cache.new_infos {
            let fbx_guids = match resolve_variant_multi(index, &info.source_fbx_guid) {
                Ok(gs) => {
                    if gs.is_empty() {
                        vec![info.source_fbx_guid.clone()]
                    } else {
                        gs
                    }
                }
                Err(e) => {
                    log::warn!("Variant resolve failed during index build: {}", e);
                    continue;
                }
            };
            // Cache variant resolution result for later reuse
            index
                .variant_cache
                .insert(info.source_fbx_guid.clone(), fbx_guids.clone());
            for fbx_guid in fbx_guids {
                index
                    .prefab_by_fbx_guid
                    .entry(fbx_guid)
                    .or_default()
                    .push(*prefab_idx);
            }
        }

        // Old format: direct FBX GUID references
        for info in &cache.old_infos {
            index
                .prefab_by_fbx_guid
                .entry(info.fbx_guid.clone())
                .or_default()
                .push(*prefab_idx);
        }
    }

    // Deduplicate prefab indices per FBX GUID
    for indices in index.prefab_by_fbx_guid.values_mut() {
        indices.sort_unstable();
        indices.dedup();
    }

    log::debug!(
        "Prefab FBX map built: {} FBX GUIDs, {} cached prefabs",
        index.prefab_by_fbx_guid.len(),
        index.prefab_cache.len()
    );
}

/// Prefab texture resolution: locate the Prefab that matches the FBX GUID and return a material -> texture mapping.
pub fn resolve_prefab_textures(
    pkg: &UnityPackageIndex,
    fbx_guid: &str,
    fbx_path: &str,
) -> Vec<ResolvedMaterialTextures> {
    let mut candidates: Vec<PrefabCandidate> = Vec::new();

    // Use prebuilt FBX GUID → prefab index map instead of iterating all entries
    let prefab_indices = match pkg.prefab_by_fbx_guid.get(fbx_guid) {
        Some(indices) => indices.clone(),
        None => {
            log::info!(
                "Prefab texture resolve: no Prefab found for FBX {}",
                fbx_path
            );
            return Vec::new();
        }
    };

    for &prefab_idx in &prefab_indices {
        let entry = &pkg.entries[prefab_idx];
        let cache = match pkg.prefab_cache.get(&prefab_idx) {
            Some(c) => c,
            None => continue,
        };

        log::debug!(
            "Prefab inspection: {} format={:?}",
            entry.pathname,
            cache.format
        );

        // New format: use cached NewPrefabInfo (variant resolution already done at index build)
        let mut new_matched = false;
        if cache.format == PrefabFormat::New {
            log::debug!("  New Prefab infos: {}", cache.new_infos.len());
            for (i, info) in cache.new_infos.iter().enumerate() {
                log::debug!(
                    "    [{}] source_guid={}, overrides={}",
                    i,
                    info.source_fbx_guid,
                    info.material_overrides.len()
                );
            }

            for info in &cache.new_infos {
                // Variant resolution was already performed during index build;
                // the fact that this prefab_idx appears under fbx_guid means it matched.
                // However, a single prefab may reference multiple FBX GUIDs, so we still
                // need to verify that this particular info's source resolves to our fbx_guid.
                // We re-check via resolve_variant_multi (cheap since it's a simple lookup).
                let resolved_guids = match resolve_variant_multi_cached(pkg, &info.source_fbx_guid)
                {
                    Ok(gs) => {
                        if gs.is_empty() {
                            vec![info.source_fbx_guid.clone()]
                        } else {
                            gs
                        }
                    }
                    Err(_) => continue,
                };

                if !resolved_guids.contains(&fbx_guid.to_string()) {
                    continue;
                }
                new_matched = true;

                // FBX .meta -> externalObjects
                let fbx_entry_idx = match pkg.by_guid.get(fbx_guid) {
                    Some(&idx) => idx,
                    None => continue,
                };
                let meta_materials = if let Some(ref meta) = pkg.entries[fbx_entry_idx].meta {
                    match parse_fbx_meta(meta) {
                        Ok((mats, _)) => mats,
                        Err(e) => {
                            log::warn!("FBX .meta parse failed: {}", e);
                            Vec::new()
                        }
                    }
                } else {
                    Vec::new()
                };
                let meta_guid_to_fbx_name: HashMap<String, String> = meta_materials
                    .iter()
                    .map(|m| (m.material_guid.clone(), m.material_name.clone()))
                    .collect();

                // Collect unique material GUIDs: FBX .meta + Prefab overrides
                let mut all_mat_guids: Vec<String> = Vec::new();
                let mut seen_guids: HashSet<String> = HashSet::new();

                for fbx_mat in &meta_materials {
                    if seen_guids.insert(fbx_mat.material_guid.clone()) {
                        all_mat_guids.push(fbx_mat.material_guid.clone());
                    }
                }

                for ov in &info.material_overrides {
                    if seen_guids.insert(ov.material_guid.clone()) {
                        all_mat_guids.push(ov.material_guid.clone());
                    }
                }

                let mat_guid_map: HashMap<usize, String> =
                    all_mat_guids.into_iter().enumerate().collect();

                let resolved_mats = resolve_material_guids_to_textures_with_meta(
                    pkg,
                    &mat_guid_map,
                    &meta_guid_to_fbx_name,
                );

                if !resolved_mats.is_empty() {
                    candidates.push(PrefabCandidate {
                        prefab_path: entry.pathname.clone(),
                        materials: resolved_mats,
                    });
                }
            }
        }

        // Old format (or New format fallback when no new_infos matched)
        if cache.format == PrefabFormat::Old || (cache.format == PrefabFormat::New && !new_matched)
        {
            if cache.format == PrefabFormat::New {
                log::debug!(
                    "  New format no match -> Old format fallback ({})",
                    entry.pathname
                );
            }

            // Merge material GUIDs from all OldPrefabInfo matching this FBX GUID
            let mut all_mat_guids: Vec<String> = Vec::new();
            let mut seen_guids: HashSet<String> = HashSet::new();
            let mut has_match = false;
            for info in &cache.old_infos {
                if info.fbx_guid != fbx_guid {
                    continue;
                }
                has_match = true;
                for guid in &info.material_guids {
                    if seen_guids.insert(guid.clone()) {
                        all_mat_guids.push(guid.clone());
                    }
                }
            }
            if !has_match {
                continue;
            }

            // FBX .meta -> material GUID → FBX material name
            let fbx_entry_idx = match pkg.by_guid.get(fbx_guid) {
                Some(&idx) => idx,
                None => continue,
            };
            let meta_guid_to_fbx_name: HashMap<String, String> =
                if let Some(ref meta) = pkg.entries[fbx_entry_idx].meta {
                    match parse_fbx_meta(meta) {
                        Ok((mats, _)) => mats
                            .into_iter()
                            .map(|m| (m.material_guid, m.material_name))
                            .collect(),
                        Err(e) => {
                            log::warn!("FBX .meta parse failed: {}", e);
                            HashMap::new()
                        }
                    }
                } else {
                    HashMap::new()
                };

            // Add meta-sourced materials too
            for meta_guid in meta_guid_to_fbx_name.keys() {
                if seen_guids.insert(meta_guid.clone()) {
                    all_mat_guids.push(meta_guid.clone());
                }
            }

            let mat_guid_map: HashMap<usize, String> =
                all_mat_guids.into_iter().enumerate().collect();
            let resolved_mats = resolve_material_guids_to_textures_with_meta(
                pkg,
                &mat_guid_map,
                &meta_guid_to_fbx_name,
            );

            if !resolved_mats.is_empty() {
                candidates.push(PrefabCandidate {
                    prefab_path: entry.pathname.clone(),
                    materials: resolved_mats,
                });
            }
        }
    }

    if candidates.is_empty() {
        log::info!(
            "Prefab texture resolve: no Prefab found for FBX {}",
            fbx_path
        );
        return Vec::new();
    }

    log::info!(
        "Prefab texture resolve: {} candidates ({})",
        candidates.len(),
        candidates
            .iter()
            .map(|c| c.prefab_path.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );

    match choose_prefab(&candidates, fbx_path) {
        Some(c) => {
            log::info!("Prefab selected: {}", c.prefab_path);
            // Pull the chosen candidate out of the list to move ownership
            let idx = candidates
                .iter()
                .position(|x| std::ptr::eq(x, c))
                .expect("choose_prefab は candidates 内の参照を返す");
            candidates
                .into_iter()
                .nth(idx)
                .expect("position で見つかったインデックスは nth で必ず取得可能")
                .materials
        }
        None => {
            log::warn!("Prefab texture resolve: ambiguous, returning empty");
            Vec::new()
        }
    }
}

/// Resolution result for a single FBX.
pub struct FbxResolveEntry {
    pub fbx_guid: String,
    pub fbx_index: usize,
    pub materials: Vec<ResolvedMaterialTextures>,
}

/// Full Prefab resolution result (may contain multiple FBX entries).
pub struct PrefabResolveResult {
    pub entries: Vec<FbxResolveEntry>,
}

/// Resolve every FBX referenced from a specific Prefab entry and return their texture mappings.
pub fn resolve_single_prefab(
    pkg: &UnityPackageIndex,
    prefab_index: usize,
) -> PkgResult<PrefabResolveResult> {
    resolve_single_prefab_inner(pkg, prefab_index, 0)
}

/// Recursive internal implementation (uses `depth` to guard against infinite loops).
fn resolve_single_prefab_inner(
    pkg: &UnityPackageIndex,
    prefab_index: usize,
    depth: usize,
) -> PkgResult<PrefabResolveResult> {
    if depth > 32 {
        let guid = &pkg.entries[prefab_index].guid;
        return Err(PkgError::PrefabVariantTooDeep { guid: guid.clone() });
    }

    let entry = &pkg.entries[prefab_index];
    let content = String::from_utf8_lossy(&entry.data);
    let format = detect_prefab_format(&content);

    let mut entries = Vec::new();
    let mut seen_guids = HashSet::new();
    let mut parsed_count = 0;

    // Parse the New format (PrefabInstance block)
    if format == PrefabFormat::New {
        let infos = parse_prefab_new(&content)?;
        parsed_count += infos.len();
        for info in infos {
            // Check whether the reference points to another Prefab (handles Nested Prefab / Variant)
            let is_nested_prefab = pkg
                .by_guid
                .get(&info.source_fbx_guid)
                .map(|&idx| {
                    pkg.entries[idx]
                        .pathname
                        .to_lowercase()
                        .ends_with(".prefab")
                })
                .unwrap_or(false);

            if is_nested_prefab {
                // Nested Prefab: recurse and collect every FBX entry
                let nested_idx = *pkg
                    .by_guid
                    .get(&info.source_fbx_guid)
                    .expect("is_nested_prefab チェックで存在確認済み");
                log::debug!(
                    "resolve_single_prefab: nested Prefab resolve (depth={}) -> {}",
                    depth,
                    pkg.entries[nested_idx].pathname
                );
                match resolve_single_prefab_inner(pkg, nested_idx, depth + 1) {
                    Ok(nested_result) => {
                        for nested_entry in nested_result.entries {
                            if seen_guids.insert(nested_entry.fbx_guid.clone()) {
                                entries.push(nested_entry);
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Nested Prefab resolve failed: {}", e);
                    }
                }
            } else {
                // The reference is an FBX (or resolves to an FBX via a Variant)
                let resolved_guid =
                    resolve_variant(pkg, &info.source_fbx_guid)?.unwrap_or(info.source_fbx_guid);
                if !seen_guids.insert(resolved_guid.clone()) {
                    continue; // Skip duplicate FBX GUID
                }
                let Some(&fbx_idx) = pkg.by_guid.get(&resolved_guid) else {
                    log::warn!("Prefab referenced FBX not found: GUID={}", resolved_guid);
                    continue;
                };
                // New format: FBX .meta + Prefab overrides
                let meta_materials = if let Some(ref meta) = pkg.entries[fbx_idx].meta {
                    match parse_fbx_meta(meta) {
                        Ok((mats, _)) => mats,
                        Err(e) => {
                            log::warn!("FBX .meta parse failed: {}", e);
                            Vec::new()
                        }
                    }
                } else {
                    Vec::new()
                };
                let meta_guid_to_fbx_name: HashMap<String, String> = meta_materials
                    .iter()
                    .map(|m| (m.material_guid.clone(), m.material_name.clone()))
                    .collect();
                let mut mat_guid_map: HashMap<usize, String> = HashMap::new();
                for (idx, fbx_mat) in meta_materials.iter().enumerate() {
                    mat_guid_map.insert(idx, fbx_mat.material_guid.clone());
                }
                for ov in &info.material_overrides {
                    mat_guid_map.insert(ov.slot_index, ov.material_guid.clone());
                }
                let resolved_mats = resolve_material_guids_to_textures_with_meta(
                    pkg,
                    &mat_guid_map,
                    &meta_guid_to_fbx_name,
                );
                entries.push(FbxResolveEntry {
                    fbx_guid: resolved_guid,
                    fbx_index: fbx_idx,
                    materials: resolved_mats,
                });
            }
        }
    }

    // Parse the Old format (SkinnedMeshRenderer section).
    // Mixed-format support: always run Old after New so we still pick up any extra FBX references.
    {
        if let Ok(infos) = parse_prefab_old(&content) {
            if !infos.is_empty() {
                parsed_count += infos.len();
                // Merge materials across multiple SkinnedMeshRenderers that point at the same FBX GUID
                let mut fbx_mat_guids: HashMap<String, Vec<String>> = HashMap::new();
                let mut fbx_guid_order: Vec<String> = Vec::new();
                for info in infos {
                    let mat_entry =
                        fbx_mat_guids
                            .entry(info.fbx_guid.clone())
                            .or_insert_with(|| {
                                fbx_guid_order.push(info.fbx_guid.clone());
                                Vec::new()
                            });
                    for guid in info.material_guids {
                        if !mat_entry.contains(&guid) {
                            mat_entry.push(guid);
                        }
                    }
                }
                for fbx_guid in fbx_guid_order {
                    if !seen_guids.insert(fbx_guid.clone()) {
                        continue; // Already added through the New path
                    }
                    let Some(&fbx_idx) = pkg.by_guid.get(&fbx_guid) else {
                        log::warn!("Prefab referenced FBX not found: GUID={}", fbx_guid);
                        continue;
                    };
                    // Build material GUID -> FBX material name mapping from the FBX .meta
                    let meta_guid_to_fbx_name: HashMap<String, String> =
                        if let Some(ref meta) = pkg.entries[fbx_idx].meta {
                            match parse_fbx_meta(meta) {
                                Ok((mats, _)) => mats
                                    .into_iter()
                                    .map(|m| (m.material_guid, m.material_name))
                                    .collect(),
                                Err(e) => {
                                    log::warn!("FBX .meta parse failed: {}", e);
                                    HashMap::new()
                                }
                            }
                        } else {
                            HashMap::new()
                        };

                    let mat_guids = fbx_mat_guids.remove(&fbx_guid).unwrap_or_default();
                    // Also include materials missing from .meta by reading the Prefab's m_Materials
                    let mut all_mat_guids = mat_guids;
                    for meta_guid in meta_guid_to_fbx_name.keys() {
                        if !all_mat_guids.contains(meta_guid) {
                            all_mat_guids.push(meta_guid.clone());
                        }
                    }
                    let mat_guid_map: HashMap<usize, String> =
                        all_mat_guids.into_iter().enumerate().collect();
                    let resolved_mats = resolve_material_guids_to_textures_with_meta(
                        pkg,
                        &mat_guid_map,
                        &meta_guid_to_fbx_name,
                    );
                    entries.push(FbxResolveEntry {
                        fbx_guid,
                        fbx_index: fbx_idx,
                        materials: resolved_mats,
                    });
                }
            }
        }
    }

    if entries.is_empty() {
        let format_name = match format {
            PrefabFormat::New => "New",
            PrefabFormat::Old => "Old",
        };
        return Err(PkgError::PrefabParseFailed {
            path: entry.pathname.clone(),
            format: format_name,
            parsed_count,
        });
    }

    Ok(PrefabResolveResult { entries })
}

/// Build texture-resolution results from a material-GUID map (with FBX material names from .meta).
fn resolve_material_guids_to_textures_with_meta(
    pkg: &UnityPackageIndex,
    mat_guid_map: &HashMap<usize, String>,
    meta_guid_to_fbx_name: &HashMap<String, String>,
) -> Vec<ResolvedMaterialTextures> {
    let mut resolved_mats: Vec<ResolvedMaterialTextures> = Vec::new();
    let mut slot_indices: Vec<usize> = mat_guid_map.keys().copied().collect();
    slot_indices.sort();

    for &slot_idx in &slot_indices {
        let mat_guid = &mat_guid_map[&slot_idx];
        let mat_entry_idx = match pkg.by_guid.get(mat_guid.as_str()) {
            Some(&idx) => idx,
            None => {
                log::debug!("Material GUID {} not found", mat_guid);
                continue;
            }
        };

        let mat_data = &pkg.entries[mat_entry_idx].data;
        let mat_content = String::from_utf8_lossy(mat_data);
        let parsed = match parse_material_textures(&mat_content) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Material parse failed ({}): {}", mat_guid, e);
                continue;
            }
        };

        // Priority: _MainTex first, then _BaseMap, then _BaseColorMap. lilToon (and similar shaders)
        // sometimes assign a different texture to _BaseColorMap from _MainTex.
        let main_tex_guid = parsed
            .textures
            .iter()
            .find(|t| t.slot_name == "_MainTex")
            .or_else(|| parsed.textures.iter().find(|t| t.slot_name == "_BaseMap"))
            .or_else(|| {
                parsed
                    .textures
                    .iter()
                    .find(|t| t.slot_name == "_BaseColorMap")
            })
            .map(|t| Arc::from(t.texture_guid.as_str()));

        // Normal map: _BumpMap (Standard/lilToon/Poiyomi/AXCS/WF) > _NormalMap (UTS2)
        let bump_map = parsed.textures.iter().find(|t| t.slot_name == "_BumpMap");
        let normal_map = parsed.textures.iter().find(|t| t.slot_name == "_NormalMap");
        // Warn when both exist with different GUIDs
        if let (Some(b), Some(n)) = (bump_map, normal_map) {
            if b.texture_guid != n.texture_guid {
                log::warn!(
                    "Material '{}': _BumpMap and _NormalMap GUIDs differ (_BumpMap preferred)",
                    parsed.name
                );
            }
        }
        let normal_tex_guid = bump_map
            .or(normal_map)
            .map(|t| Arc::from(t.texture_guid.as_str()));

        let bump_scale = parsed
            .floats
            .iter()
            .find(|f| f.param_name == "_BumpScale")
            .map(|f| f.value)
            .unwrap_or(1.0);

        // Emission texture
        let emission_tex_guid = parsed
            .textures
            .iter()
            .find(|t| t.slot_name == "_EmissionMap")
            .map(|t| Arc::from(t.texture_guid.as_str()));

        // Emission color (`_EmissionColor`)
        let emission_color = parsed
            .colors
            .iter()
            .find(|c| c.param_name == "_EmissionColor")
            .map(|c| [c.r, c.g, c.b])
            .unwrap_or([0.0; 3]);

        // Emission enabled-detection (priority order):
        // 1. If `_UseEmission` (lilToon) is set explicitly, follow its value.
        // 2. If `_Emission` float (Standard) is set explicitly, follow its value.
        // 3. Enabled if `_EMISSION` appears in `m_ShaderKeywords` / `m_ValidKeywords`.
        // 4. Enabled if an `_EmissionMap` texture exists.
        // 5. Enabled if `_EmissionColor` is neither black nor white.
        //    White (1, 1, 1) is excluded because many shaders initialize emission to white
        //    even when disabled (real-world example: Masscat v1.02).
        let has_emission_keyword = parsed.shader_keywords.iter().any(|kw| kw == "_EMISSION");
        let emission_color_meaningful =
            emission_color != [0.0; 3] && emission_color != [1.0, 1.0, 1.0];
        let use_emission_liltoon = parsed
            .floats
            .iter()
            .find(|f| f.param_name == "_UseEmission")
            .map(|f| f.value >= 0.5);
        let emission_enabled = use_emission_liltoon.unwrap_or_else(|| {
            parsed
                .floats
                .iter()
                .find(|f| f.param_name == "_Emission")
                .map(|f| f.value >= 0.5)
                .unwrap_or(
                    has_emission_keyword
                        || emission_tex_guid.is_some()
                        || emission_color_meaningful,
                )
        });

        // Emission blend mode (lilToon: 0 = Add, 1 = Screen)
        let emission_blend = parsed
            .floats
            .iter()
            .find(|f| f.param_name == "_EmissionBlend")
            .map(|f| f.value as u8)
            .unwrap_or(0);

        let fbx_name = meta_guid_to_fbx_name
            .get(mat_guid.as_str())
            .map(|n| Arc::from(n.as_str()));

        log::debug!(
            "  Material resolve: slot={} mat_guid={} -> .mat name='{}' fbx_name={:?} main_tex={:?} normal_tex={:?} bump_scale={} slots=[{}]",
            slot_idx,
            mat_guid,
            parsed.name,
            fbx_name,
            main_tex_guid,
            normal_tex_guid,
            bump_scale,
            parsed.textures.iter().map(|t| format!("{}:{}", t.slot_name, &t.texture_guid[..8.min(t.texture_guid.len())])).collect::<Vec<_>>().join(", ")
        );

        resolved_mats.push(ResolvedMaterialTextures {
            source_material: None,
            material_name: Arc::from(parsed.name.as_str()),
            main_texture_guid: main_tex_guid,
            normal_texture_guid: normal_tex_guid,
            bump_scale,
            fbx_material_name: fbx_name,
            emission_texture_guid: emission_tex_guid,
            emission_color,
            emission_enabled,
            emission_blend,
        });
    }

    resolved_mats
}

// ── Step 8: PackageTexture + PreparedPkgFbx ──

/// Texture inside a unitypackage (with its GUID).
pub struct PackageTexture {
    pub guid: Arc<str>,
    pub display_name: Arc<str>,
    pub data: Arc<[u8]>,
    /// Full path inside the archive (e.g. "Assets/texture/body.png").
    pub pathname: Arc<str>,
}

/// Prefab-resolved FBX package.
pub struct PreparedPkgFbx {
    pub model: PkgModelLocator,
    pub fbx_data: Arc<[u8]>,
    pub textures: Vec<PackageTexture>,
    pub resolved: Vec<ResolvedMaterialTextures>,
}

/// Whether the path has an image-file extension.
fn is_image_extension(path: &str) -> bool {
    let lower = path.to_lowercase();
    const IMAGE_EXTS: &[&str] = &[
        ".png", ".jpg", ".jpeg", ".tga", ".bmp", ".psd", ".tif", ".tiff",
    ];
    IMAGE_EXTS.iter().any(|ext| lower.ends_with(ext))
}

/// Prepare an FBX from a `UnityPackageIndex` (Prefab resolution + texture collection).
pub fn prepare_pkg_fbx(pkg: &UnityPackageIndex, fbx_index: usize) -> Result<PreparedPkgFbx> {
    let entry = &pkg.entries[fbx_index];
    let fbx_guid = &entry.guid;
    let fbx_path = &entry.pathname;

    // Build the PkgModelLocator
    let model = PkgModelLocator {
        guid: Arc::from(fbx_guid.as_str()),
        pathname: Arc::from(fbx_path.as_str()),
        kind: PkgModelType::Fbx,
    };

    // Prefab texture resolution
    let resolved = resolve_prefab_textures(pkg, fbx_guid, fbx_path);

    // Texture collection (filtered by image extension)
    let textures: Vec<PackageTexture> = pkg
        .entries
        .iter()
        .filter(|e| is_image_extension(&e.pathname))
        .map(|e| {
            let display_name = std::path::Path::new(&e.pathname)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            PackageTexture {
                guid: Arc::from(e.guid.as_str()),
                display_name: Arc::from(display_name.as_ref()),
                data: Arc::clone(&e.data),
                pathname: Arc::from(e.pathname.as_str()),
            }
        })
        .collect();

    log::info!(
        "prepare_pkg_fbx: FBX={}, textures={}, Prefab resolved materials={}",
        fbx_path,
        textures.len(),
        resolved.len()
    );

    Ok(PreparedPkgFbx {
        model,
        fbx_data: Arc::clone(&entry.data),
        textures,
        resolved,
    })
}

/// Automatically pick the best FBX from a list of FBX indices.
///
/// Selection criteria (priority order):
/// 1. Prefer the largest FBX by file size (the main model tends to be bigger than animations or props).
pub fn select_best_fbx_index(pkg: &UnityPackageIndex, fbx_indices: &[(usize, String)]) -> usize {
    if fbx_indices.len() == 1 {
        return fbx_indices[0].0;
    }
    // Pick the FBX with the largest data size
    let best = fbx_indices
        .iter()
        .max_by_key(|(idx, _)| pkg.entries[*idx].data.len())
        .expect("fbx_indices は len>=2（len==1 は早期リターン済み）");
    log::info!(
        "FBX auto-selected: {} ({}KB, {}/{} candidates)",
        best.1,
        pkg.entries[best.0].data.len() / 1024,
        fbx_indices
            .iter()
            .position(|(i, _)| *i == best.0)
            .expect("best は fbx_indices から取得したため必ず存在")
            + 1,
        fbx_indices.len()
    );
    best.0
}

/// Model resolution for the CLI: pick one entry from a list of FBX candidates using the `--fbx-name` hint.
/// FBX only (the CLI supports only FBX conversion).
pub fn resolve_pkg_model_for_cli<'a>(
    items: &'a [PkgModelListItem],
    hint: Option<&str>,
) -> PkgResult<&'a PkgModelLocator> {
    let fbx_items: Vec<_> = items
        .iter()
        .filter(|it| it.locator.kind == PkgModelType::Fbx)
        .collect();
    let candidates: Vec<String> = fbx_items
        .iter()
        .map(|it| it.locator.pathname.to_string())
        .collect();
    let Some(hint) = hint else {
        return match fbx_items.as_slice() {
            [one] => Ok(&one.locator),
            [] => Err(PkgError::ModelNotFound {
                hint: "(none)".into(),
                expected_type: "FBX",
                candidates,
            }),
            _ => Err(PkgError::ModelAmbiguous {
                hint: "(auto)".into(),
                expected_type: "FBX",
                candidates,
            }),
        };
    };
    let hint_lower = hint.to_lowercase();
    let matches: Vec<_> = fbx_items
        .iter()
        .filter(|it| it.locator.pathname.to_lowercase().contains(&hint_lower))
        .collect();
    match matches.as_slice() {
        [one] => Ok(&one.locator),
        [] => Err(PkgError::ModelNotFound {
            hint: hint.into(),
            expected_type: "FBX",
            candidates,
        }),
        _ => Err(PkgError::ModelAmbiguous {
            hint: hint.into(),
            expected_type: "FBX",
            candidates: matches
                .iter()
                .map(|it| it.locator.pathname.to_string())
                .collect(),
        }),
    }
}

/// Helper that applies resolved texture info to an `IrMaterial`.
/// Shared logic between strategy 1 (source_material match) and strategy 2 (material_name match).
#[allow(clippy::too_many_arguments)]
fn apply_resolved_textures(
    mat: &mut crate::intermediate::types::IrMaterial,
    res: &ResolvedMaterialTextures,
    tex_by_guid: &HashMap<&str, &PackageTexture>,
    added_guids: &mut HashMap<Arc<str>, usize>,
    ir_textures: &mut Vec<crate::intermediate::types::IrTexture>,
    matched_base: &mut usize,
    matched_normal: &mut usize,
    prefab_label: &str,
    strategy: &str,
) {
    // -- Main texture --
    if mat.texture_index.is_none() {
        if let Some(ref tex_guid) = res.main_texture_guid {
            if let Some(&existing_idx) = added_guids.get(tex_guid) {
                mat.texture_index = Some(existing_idx);
                *matched_base += 1;
                log::info!(
                    "Prefab texture assign ({}, reuse): {} -> mat[{}]",
                    strategy,
                    tex_guid,
                    mat.name
                );
            } else if let Some(pkg_tex) = tex_by_guid.get(tex_guid.as_ref()) {
                let tex_idx = ir_textures.len();
                let ext =
                    crate::path_ext_lower(std::path::Path::new(pkg_tex.display_name.as_ref()));
                let mime = crate::intermediate::types::mime_for_ext(&ext).to_string();
                ir_textures.push(crate::intermediate::types::IrTexture {
                    filename: pkg_tex.display_name.to_string(),
                    data: TextureData::Encoded(Arc::clone(&pkg_tex.data)),
                    mime_type: mime,
                    source_path: format!("{}: {}", prefab_label, pkg_tex.pathname),
                    mip_chain: None,
                });
                mat.texture_index = Some(tex_idx);
                added_guids.insert(Arc::clone(tex_guid), tex_idx);
                *matched_base += 1;
                log::info!(
                    "Prefab texture assign ({}): {} -> mat[{}]",
                    strategy,
                    pkg_tex.display_name,
                    mat.name
                );
            } else {
                log::debug!(
                    "Prefab texture GUID {} not found in pkg (mat: {})",
                    tex_guid,
                    mat.name
                );
            }
        }
    }

    // -- Normal map --
    if mat.normal_texture.is_none() {
        if let Some(ref normal_guid) = res.normal_texture_guid {
            if let Some(&existing_idx) = added_guids.get(normal_guid) {
                mat.normal_texture = Some(crate::intermediate::types::IrTextureInfo::from_index(
                    existing_idx,
                ));
                mat.normal_texture_scale = res.bump_scale;
                *matched_normal += 1;
                log::info!(
                    "Prefab normal map assign ({}, reuse): {} -> mat[{}]",
                    strategy,
                    normal_guid,
                    mat.name
                );
            } else if let Some(pkg_tex) = tex_by_guid.get(normal_guid.as_ref()) {
                let tex_idx = ir_textures.len();
                let ext =
                    crate::path_ext_lower(std::path::Path::new(pkg_tex.display_name.as_ref()));
                let mime = crate::intermediate::types::mime_for_ext(&ext).to_string();
                ir_textures.push(crate::intermediate::types::IrTexture {
                    filename: pkg_tex.display_name.to_string(),
                    data: TextureData::Encoded(Arc::clone(&pkg_tex.data)),
                    mime_type: mime,
                    source_path: format!("{}: {}", prefab_label, pkg_tex.pathname),
                    mip_chain: None,
                });
                mat.normal_texture = Some(crate::intermediate::types::IrTextureInfo::from_index(
                    tex_idx,
                ));
                mat.normal_texture_scale = res.bump_scale;
                added_guids.insert(Arc::clone(normal_guid), tex_idx);
                *matched_normal += 1;
                log::info!(
                    "Prefab normal map assign ({}): {} -> mat[{}]",
                    strategy,
                    pkg_tex.display_name,
                    mat.name
                );
            } else {
                log::debug!(
                    "Prefab normal map GUID {} not found in pkg (mat: {})",
                    normal_guid,
                    mat.name
                );
            }
        }
    }

    // ── Emission ──
    if res.emission_enabled {
        let ec = res.emission_color;
        if ec != [0.0; 3] && mat.emissive_factor == glam::Vec3::ZERO {
            mat.emissive_factor = glam::Vec3::new(ec[0], ec[1], ec[2]);
            log::info!(
                "Prefab emission color assign: [{:.2},{:.2},{:.2}] -> mat[{}]",
                ec[0],
                ec[1],
                ec[2],
                mat.name
            );
        }

        if mat.emissive_texture.is_none() {
            if let Some(ref em_guid) = res.emission_texture_guid {
                if let Some(&existing_idx) = added_guids.get(em_guid) {
                    mat.emissive_texture = Some(
                        crate::intermediate::types::IrTextureInfo::from_index(existing_idx),
                    );
                    log::info!(
                        "Prefab emission texture assign ({}, reuse): {} -> mat[{}]",
                        strategy,
                        em_guid,
                        mat.name
                    );
                } else if let Some(pkg_tex) = tex_by_guid.get(em_guid.as_ref()) {
                    let tex_idx = ir_textures.len();
                    let ext =
                        crate::path_ext_lower(std::path::Path::new(pkg_tex.display_name.as_ref()));
                    let mime = crate::intermediate::types::mime_for_ext(&ext).to_string();
                    ir_textures.push(crate::intermediate::types::IrTexture {
                        filename: pkg_tex.display_name.to_string(),
                        data: TextureData::Encoded(Arc::clone(&pkg_tex.data)),
                        mime_type: mime,
                        source_path: format!("{}: {}", prefab_label, pkg_tex.pathname),
                        mip_chain: None,
                    });
                    mat.emissive_texture = Some(
                        crate::intermediate::types::IrTextureInfo::from_index(tex_idx),
                    );
                    added_guids.insert(Arc::clone(em_guid), tex_idx);
                    log::info!(
                        "Prefab emission texture assign ({}): {} -> mat[{}]",
                        strategy,
                        pkg_tex.display_name,
                        mat.name
                    );
                }
            }
        }

        if mat.emissive_texture.is_some() && mat.emissive_factor == glam::Vec3::ZERO {
            mat.emissive_factor = glam::Vec3::ONE;
            log::info!(
                "Prefab emission color correction: (0,0,0) -> (1,1,1) (has texture) mat[{}]",
                mat.name
            );
        }

        // Approximation for lilToon's Screen blend (mode 1):
        // Screen composite = base + emission - base * emission is dimmer than additive blending.
        // PBR only provides additive emission, so we attenuate the factor to approximate it.
        if res.emission_blend == 1 && mat.emissive_factor != glam::Vec3::ZERO {
            const SCREEN_ATTENUATION: f32 = 0.5;
            let before = mat.emissive_factor;
            mat.emissive_factor *= SCREEN_ATTENUATION;
            log::info!(
                "Prefab emission screen-blend attenuation: [{:.2},{:.2},{:.2}] -> [{:.2},{:.2},{:.2}] mat[{}]",
                before.x, before.y, before.z,
                mat.emissive_factor.x, mat.emissive_factor.y, mat.emissive_factor.z,
                mat.name
            );
        }
    }
}

/// Embed Prefab-resolved textures into an `IrModel` (with a three-stage fallback).
/// Returns the indices of materials that remained unassigned.
pub fn embed_textures_with_prefab(
    ir: &mut crate::intermediate::types::IrModel,
    textures: &[PackageTexture],
    resolved: &[ResolvedMaterialTextures],
    prefab_label: &str,
) -> Vec<usize> {
    if textures.is_empty() {
        return (0..ir.materials.len()).collect();
    }

    // Reverse map GUID -> PackageTexture
    let tex_by_guid: HashMap<&str, &PackageTexture> =
        textures.iter().map(|t| (t.guid.as_ref(), t)).collect();

    // Already-added texture GUID -> ir.textures index
    let mut added_guids: HashMap<Arc<str>, usize> = HashMap::new();

    let mut matched_base = 0usize;
    let mut matched_normal = 0usize;

    // -- Strategy 1: match via source_material (renderer_path + slot_index) --
    // When FBX extract sets source_material, this matches the Prefab resolution result exactly.
    {
        let resolved_by_source: HashMap<&SourceMaterialRef, &ResolvedMaterialTextures> = resolved
            .iter()
            .filter_map(|r| r.source_material.as_ref().map(|sm| (sm, r)))
            .collect();

        if !resolved_by_source.is_empty() {
            for mat in &mut ir.materials {
                if let Some(ref sm) = mat.source_material {
                    if let Some(res) = resolved_by_source.get(sm) {
                        apply_resolved_textures(
                            mat,
                            res,
                            &tex_by_guid,
                            &mut added_guids,
                            &mut ir.textures,
                            &mut matched_base,
                            &mut matched_normal,
                            prefab_label,
                            "source_material",
                        );
                    }
                }
            }
        }
    }

    // -- Strategy 2: match via material_name / fbx_material_name --
    if !resolved.is_empty() {
        // Map resolved.material_name -> index (for exact matches)
        let resolved_by_name: HashMap<&str, &ResolvedMaterialTextures> = resolved
            .iter()
            .map(|r| (r.material_name.as_ref(), r))
            .collect();

        // Lowercased map (for case-insensitive matches)
        let resolved_by_lower: HashMap<String, &ResolvedMaterialTextures> = resolved
            .iter()
            .map(|r| (r.material_name.to_lowercase(), r))
            .collect();

        // fbx_material_name lookup map (names taken from the FBX .meta externalObjects)
        let resolved_by_fbx_name: HashMap<&str, &ResolvedMaterialTextures> = resolved
            .iter()
            .filter_map(|r| r.fbx_material_name.as_ref().map(|n| (n.as_ref(), r)))
            .collect();
        let resolved_by_fbx_lower: HashMap<String, &ResolvedMaterialTextures> = resolved
            .iter()
            .filter_map(|r| r.fbx_material_name.as_ref().map(|n| (n.to_lowercase(), r)))
            .collect();

        log::debug!(
            "Prefab resolved material names: [{}]",
            resolved
                .iter()
                .map(|r| {
                    if let Some(ref fbx_name) = r.fbx_material_name {
                        format!("{}(fbx:{})", r.material_name, fbx_name)
                    } else {
                        r.material_name.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        );
        log::debug!(
            "IrModel material names: [{}]",
            ir.materials
                .iter()
                .map(|m| m.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );

        for mat in &mut ir.materials {
            // Exact -> case-insensitive -> exact FBX-name -> case-insensitive FBX-name -> suffix match
            let mat_lower = mat.name.to_lowercase();
            let res_opt = resolved_by_name
                .get(mat.name.as_str())
                .copied()
                .or_else(|| resolved_by_lower.get(mat_lower.as_str()).copied())
                .or_else(|| resolved_by_fbx_name.get(mat.name.as_str()).copied())
                .or_else(|| resolved_by_fbx_lower.get(mat_lower.as_str()).copied())
                .or_else(|| {
                    // Suffix match: lowercase(resolved) ends with lowercase(mat.name), or
                    // lowercase(mat.name) ends with lowercase(resolved).
                    // Example: "fc_milltina_body" ends_with "milltina_body".
                    resolved.iter().find(|r| {
                        let r_lower = r.material_name.to_lowercase();
                        if r_lower.ends_with(&mat_lower) || mat_lower.ends_with(&r_lower) {
                            return true;
                        }
                        // Try a suffix match against fbx_material_name as well
                        if let Some(ref fbx_name) = r.fbx_material_name {
                            let f_lower = fbx_name.to_lowercase();
                            f_lower.ends_with(&mat_lower) || mat_lower.ends_with(&f_lower)
                        } else {
                            false
                        }
                    })
                });

            if let Some(res) = res_opt {
                apply_resolved_textures(
                    mat,
                    res,
                    &tex_by_guid,
                    &mut added_guids,
                    &mut ir.textures,
                    &mut matched_base,
                    &mut matched_normal,
                    prefab_label,
                    "name",
                );
            }
        }
    }

    // -- Strategy 3: filename matching against source_texture_name (reuses the legacy path) --
    // Reverse map filename -> PackageTexture
    let tex_by_name: HashMap<String, &PackageTexture> = textures
        .iter()
        .map(|t| (t.display_name.to_lowercase(), t))
        .collect();
    let stem_map: HashMap<String, String> = tex_by_name
        .keys()
        .map(|k| {
            let stem = std::path::Path::new(k.as_str())
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            (stem, k.clone())
        })
        .collect();

    for mat in &mut ir.materials {
        if mat.texture_index.is_some() {
            continue;
        }

        let src_name = match mat.source_texture_name.as_deref() {
            Some(name) if !name.is_empty() => name.to_lowercase(),
            _ => continue,
        };

        let found_key = if tex_by_name.contains_key(&src_name) {
            Some(src_name.clone())
        } else {
            let stem = std::path::Path::new(&src_name)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            stem_map.get(stem.as_str()).cloned()
        };

        if let Some(ref key) = found_key {
            if let Some(pkg_tex) = tex_by_name.get(key) {
                // Check for GUID duplicates
                if let Some(&existing_idx) = added_guids.get(&pkg_tex.guid) {
                    mat.texture_index = Some(existing_idx);
                    matched_base += 1;
                    log::info!(
                        "Texture assign (filename, reuse): {} -> mat[{}]",
                        pkg_tex.display_name,
                        mat.name
                    );
                    continue;
                }

                let tex_idx = ir.textures.len();
                let ext = crate::path_ext_lower(std::path::Path::new(key.as_str()));
                let mime = crate::intermediate::types::mime_for_ext(&ext).to_string();
                ir.textures.push(crate::intermediate::types::IrTexture {
                    filename: pkg_tex.display_name.to_string(),
                    data: TextureData::Encoded(Arc::clone(&pkg_tex.data)),
                    mime_type: mime,
                    source_path: format!("{}: {}", prefab_label, pkg_tex.pathname),
                    mip_chain: None,
                });
                mat.texture_index = Some(tex_idx);
                added_guids.insert(Arc::clone(&pkg_tex.guid), tex_idx);
                matched_base += 1;
                log::info!(
                    "Texture assign (filename): {} -> mat[{}]",
                    pkg_tex.display_name,
                    mat.name
                );
            }
        }
    }

    let unmatched: Vec<usize> = ir
        .materials
        .iter()
        .enumerate()
        .filter(|(_, mat)| mat.texture_index.is_none())
        .map(|(i, _)| i)
        .collect();

    log::info!(
        "Prefab textures: base={}/{}, normal={}/{}, unassigned: {}",
        matched_base,
        ir.materials.len(),
        matched_normal,
        ir.materials.len(),
        unmatched.len()
    );
    unmatched
}

// -- Step 6: unit tests --

#[cfg(test)]
mod tests {
    use super::*;

    const NEW_PREFAB_SAMPLE: &str = r#"
%YAML 1.1
--- !u!1001 &1234567890
PrefabInstance:
  m_ObjectHideFlags: 0
  serializedVersion: 2
  m_Modification:
    m_TransformParent: {fileID: 0}
    m_Modifications:
    - target: {fileID: 1889091244555525569, guid: 21faec3b318252e4d8bf08c0fd7cb57a, type: 3}
      propertyPath: m_Materials.Array.data[0]
      value:
      objectReference: {fileID: 2100000, guid: eae6570aefbe01d4dbfaa646b8860412, type: 2}
    - target: {fileID: 1889091244555525569, guid: 21faec3b318252e4d8bf08c0fd7cb57a, type: 3}
      propertyPath: m_Materials.Array.data[1]
      value:
      objectReference: {fileID: 2100000, guid: abcdef1234567890abcdef1234567890, type: 2}
    m_RemovedComponents: []
  m_SourcePrefab: {fileID: 100100000, guid: 21faec3b318252e4d8bf08c0fd7cb57a, type: 3}
"#;

    const OLD_PREFAB_SAMPLE: &str = r#"
%YAML 1.1
--- !u!137 &137310319924682442
SkinnedMeshRenderer:
  m_ObjectHideFlags: 1
  m_Materials:
  - {fileID: 2100000, guid: 84aeac104d594b849beb3901ec49708d, type: 2}
  - {fileID: 2100000, guid: 423ae37d686c81e4f938b805dc38414f, type: 2}
  m_Mesh: {fileID: 4300116, guid: a34dbac3b9681584eaccc448e727ce44, type: 3}
"#;

    const FBX_META_SAMPLE: &str = r#"
fileFormatVersion: 2
guid: a34dbac3b9681584eaccc448e727ce44
ModelImporter:
  externalObjects:
  - first:
      type: UnityEngine:Material
      assembly: UnityEngine.CoreModule
      name: Shinano_body
    second: {fileID: 2100000, guid: 698e1c0df0ac6d54e9121cfcb99c50a6, type: 2}
  - first:
      type: UnityEngine:Material
      assembly: UnityEngine.CoreModule
      name: Shinano_face
    second: {fileID: 2100000, guid: abcdef1234567890abcdef1234567890, type: 2}
  materials:
    materialImportMode: 2
"#;

    const MAT_SAMPLE: &str = r#"
%YAML 1.1
--- !u!21 &2100000
Material:
  m_Name: Shinano_body
  m_SavedProperties:
    m_TexEnvs:
    - _MainTex:
        m_Texture: {fileID: 2800000, guid: 8cb30821603cd844dbee97db4c216501, type: 3}
        m_Scale: {x: 1, y: 1}
        m_Offset: {x: 0, y: 0}
    - _BumpMap:
        m_Texture: {fileID: 0}
        m_Scale: {x: 1, y: 1}
        m_Offset: {x: 0, y: 0}
    - _BaseMap:
        m_Texture: {fileID: 2800000, guid: aabbccdd11223344aabbccdd11223344, type: 3}
        m_Scale: {x: 1, y: 1}
        m_Offset: {x: 0, y: 0}
    m_Ints: []
"#;

    #[test]
    fn test_extract_guid_from_line() {
        let line =
            "  m_SourcePrefab: {fileID: 100100000, guid: 21faec3b318252e4d8bf08c0fd7cb57a, type: 3}";
        assert_eq!(
            extract_guid_from_line(line),
            Some("21faec3b318252e4d8bf08c0fd7cb57a")
        );

        assert_eq!(extract_guid_from_line("no guid here"), None);
        assert_eq!(extract_guid_from_line("guid: abc"), None); // 短すぎ
    }

    #[test]
    fn test_extract_array_index() {
        let line = "      propertyPath: m_Materials.Array.data[0]";
        assert_eq!(extract_array_index(line), Some(0));

        let line2 = "      propertyPath: m_Materials.Array.data[12]";
        assert_eq!(extract_array_index(line2), Some(12));

        assert_eq!(extract_array_index("no array here"), None);
    }

    #[test]
    fn test_detect_prefab_format() {
        assert_eq!(detect_prefab_format(NEW_PREFAB_SAMPLE), PrefabFormat::New);
        assert_eq!(detect_prefab_format(OLD_PREFAB_SAMPLE), PrefabFormat::Old);
    }

    #[test]
    fn test_detect_prefab_format_unpacked() {
        // Unpacked form: has the m_PrefabInstance field but no standalone PrefabInstance: block
        let content = r#"--- !u!1 &1234
GameObject:
  m_PrefabInstance: {fileID: 0}
  m_PrefabAsset: {fileID: 0}
--- !u!137 &5678
SkinnedMeshRenderer:
  m_Materials:
  - {fileID: 2100000, guid: aabbccdd11223344aabbccdd11223344, type: 2}
  m_Mesh: {fileID: 4300116, guid: 11223344aabbccdd11223344aabbccdd, type: 3}
"#;
        assert_eq!(detect_prefab_format(content), PrefabFormat::Old);
    }

    #[test]
    fn test_parse_prefab_new() {
        let infos = parse_prefab_new(NEW_PREFAB_SAMPLE).unwrap();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].source_fbx_guid, "21faec3b318252e4d8bf08c0fd7cb57a");
        assert_eq!(infos[0].material_overrides.len(), 2);
        assert_eq!(infos[0].material_overrides[0].slot_index, 0);
        assert_eq!(
            infos[0].material_overrides[0].material_guid,
            "eae6570aefbe01d4dbfaa646b8860412"
        );
        assert_eq!(infos[0].material_overrides[1].slot_index, 1);
        assert_eq!(
            infos[0].material_overrides[1].material_guid,
            "abcdef1234567890abcdef1234567890"
        );
    }

    #[test]
    fn test_parse_prefab_old() {
        let infos = parse_prefab_old(OLD_PREFAB_SAMPLE).unwrap();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].fbx_guid, "a34dbac3b9681584eaccc448e727ce44");
        assert_eq!(infos[0].material_guids.len(), 2);
        assert_eq!(
            infos[0].material_guids[0],
            "84aeac104d594b849beb3901ec49708d"
        );
        assert_eq!(
            infos[0].material_guids[1],
            "423ae37d686c81e4f938b805dc38414f"
        );
    }

    #[test]
    fn test_parse_fbx_meta() {
        let (mats, mode) = parse_fbx_meta(FBX_META_SAMPLE).unwrap();
        assert_eq!(mode, Some(2));
        assert_eq!(mats.len(), 2);
        assert_eq!(mats[0].material_name, "Shinano_body");
        assert_eq!(mats[0].material_guid, "698e1c0df0ac6d54e9121cfcb99c50a6");
        assert_eq!(mats[1].material_name, "Shinano_face");
        assert_eq!(mats[1].material_guid, "abcdef1234567890abcdef1234567890");
    }

    #[test]
    fn test_parse_fbx_meta_import_mode_not_2() {
        // Returns externalObjects even when materialImportMode is 0 (the check has been relaxed)
        let meta = r#"
externalObjects:
  - first:
      type: UnityEngine:Material
      name: SomeMat
    second: {fileID: 2100000, guid: 11111111222222223333333344444444, type: 2}
  materials:
    materialImportMode: 0
"#;
        let (mats, mode) = parse_fbx_meta(meta).unwrap();
        assert_eq!(mode, Some(0));
        assert_eq!(mats.len(), 1);
        assert_eq!(mats[0].material_name, "SomeMat");
        assert_eq!(mats[0].material_guid, "11111111222222223333333344444444");
    }

    #[test]
    fn test_parse_fbx_meta_mode_1() {
        // Still returns externalObjects with materialImportMode: 1
        let meta = r#"
externalObjects:
  - first:
      type: UnityEngine:Material
      assembly: UnityEngine.CoreModule
      name: Body
    second: {fileID: 2100000, guid: aabbccdd11223344aabbccdd11223344, type: 2}
  materials:
    materialImportMode: 1
"#;
        let (mats, mode) = parse_fbx_meta(meta).unwrap();
        assert_eq!(mats.len(), 1);
        assert_eq!(mats[0].material_name, "Body");
        assert_eq!(mats[0].material_guid, "aabbccdd11223344aabbccdd11223344");
        assert_eq!(mode, Some(1));
    }

    #[test]
    fn test_parse_fbx_meta_empty_external_objects() {
        // externalObjects: {} (empty inline form) yields an empty Vec
        let meta = r#"
externalObjects: {}
materials:
  materialImportMode: 2
"#;
        let (mats, mode) = parse_fbx_meta(meta).unwrap();
        assert!(mats.is_empty());
        assert_eq!(mode, Some(2));
    }

    #[test]
    fn test_parse_material_textures() {
        let parsed = parse_material_textures(MAT_SAMPLE).unwrap();
        assert_eq!(parsed.name, "Shinano_body");
        // Two slots (_MainTex and _BaseMap); _BumpMap is skipped because fileID is 0
        assert_eq!(parsed.textures.len(), 2);
        assert_eq!(parsed.textures[0].slot_name, "_MainTex");
        assert_eq!(
            parsed.textures[0].texture_guid,
            "8cb30821603cd844dbee97db4c216501"
        );
        assert_eq!(parsed.textures[1].slot_name, "_BaseMap");
        assert_eq!(
            parsed.textures[1].texture_guid,
            "aabbccdd11223344aabbccdd11223344"
        );
        // No m_Floats -> floats is empty
        assert!(parsed.floats.is_empty());
    }

    #[test]
    fn test_parse_material_textures_with_normal() {
        let mat = r#"
%YAML 1.1
--- !u!21 &2100000
Material:
  m_Name: Body_Mat
  m_SavedProperties:
    m_TexEnvs:
    - _MainTex:
        m_Texture: {fileID: 2800000, guid: 8cb30821603cd844dbee97db4c216501, type: 3}
        m_Scale: {x: 1, y: 1}
        m_Offset: {x: 0, y: 0}
    - _BumpMap:
        m_Texture: {fileID: 2800000, guid: aaaa1111bbbb2222cccc3333dddd4444, type: 3}
        m_Scale: {x: 1, y: 1}
        m_Offset: {x: 0, y: 0}
    m_Floats:
    - _BumpScale: 0.75
    - _Cutoff: 0.5
    m_Ints: []
"#;
        let parsed = parse_material_textures(mat).unwrap();
        assert_eq!(parsed.name, "Body_Mat");
        assert_eq!(parsed.textures.len(), 2);
        assert_eq!(parsed.textures[0].slot_name, "_MainTex");
        assert_eq!(parsed.textures[1].slot_name, "_BumpMap");
        assert_eq!(
            parsed.textures[1].texture_guid,
            "aaaa1111bbbb2222cccc3333dddd4444"
        );
        // floats
        assert_eq!(parsed.floats.len(), 2);
        let bump_scale = parsed.floats.iter().find(|f| f.param_name == "_BumpScale");
        assert!(bump_scale.is_some());
        assert!((bump_scale.unwrap().value - 0.75).abs() < f32::EPSILON);
        let cutoff = parsed.floats.iter().find(|f| f.param_name == "_Cutoff");
        assert!((cutoff.unwrap().value - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_parse_material_floats_empty_inline() {
        // m_Floats: [] (empty inline array) leaves floats empty
        let mat = r#"
Material:
  m_Name: EmptyFloats
  m_SavedProperties:
    m_TexEnvs: []
    m_Floats: []
    m_Ints: []
"#;
        let parsed = parse_material_textures(mat).unwrap();
        assert_eq!(parsed.name, "EmptyFloats");
        assert!(parsed.textures.is_empty());
        assert!(parsed.floats.is_empty());
    }

    #[test]
    fn test_embed_assigns_normal_when_base_already_exists() {
        let mut ir = crate::intermediate::types::IrModel::default();
        let mat = crate::intermediate::types::IrMaterial {
            name: "Body".into(),
            texture_index: Some(0), // Base color is already populated
            ..Default::default()
        };
        ir.materials.push(mat);
        ir.textures.push(crate::intermediate::types::IrTexture {
            filename: "base.png".into(),
            data: TextureData::Encoded(Arc::from(vec![0u8; 4])),
            mime_type: "image/png".into(),
            source_path: String::new(),
            mip_chain: None,
        });

        let textures = vec![PackageTexture {
            guid: Arc::from("normal-guid"),
            display_name: Arc::from("body_n.png"),
            data: Arc::from(vec![1u8; 4].as_slice()),
            pathname: Arc::from(""),
        }];
        let resolved = vec![ResolvedMaterialTextures {
            source_material: None,
            material_name: Arc::from("Body"),
            main_texture_guid: None,
            normal_texture_guid: Some(Arc::from("normal-guid")),
            bump_scale: 0.7,
            fbx_material_name: None,
            emission_texture_guid: None,
            emission_color: [0.0; 3],
            emission_enabled: false,
            emission_blend: 0,
        }];

        let unmatched = embed_textures_with_prefab(&mut ir, &textures, &resolved, "test");

        // Base color is already set, so unmatched is empty
        assert!(unmatched.is_empty());
        // The normal map is assigned
        assert_eq!(
            ir.materials[0].normal_texture.as_ref().map(|t| t.index),
            Some(1)
        );
        assert!((ir.materials[0].normal_texture_scale - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn test_embed_unmatched_based_on_base_texture_only() {
        // Even with no base color and a normal map, the material lands in unmatched
        let mut ir = crate::intermediate::types::IrModel::default();
        let mat = crate::intermediate::types::IrMaterial {
            name: "Body".into(),
            ..Default::default()
        };
        ir.materials.push(mat);

        let textures = vec![PackageTexture {
            guid: Arc::from("normal-guid"),
            display_name: Arc::from("body_n.png"),
            data: Arc::from(vec![1u8; 4].as_slice()),
            pathname: Arc::from(""),
        }];
        let resolved = vec![ResolvedMaterialTextures {
            source_material: None,
            material_name: Arc::from("Body"),
            main_texture_guid: None,
            normal_texture_guid: Some(Arc::from("normal-guid")),
            bump_scale: 1.0,
            fbx_material_name: None,
            emission_texture_guid: None,
            emission_color: [0.0; 3],
            emission_enabled: false,
            emission_blend: 0,
        }];

        let unmatched = embed_textures_with_prefab(&mut ir, &textures, &resolved, "test");

        // Base color stays None -> ends up in unmatched
        assert_eq!(unmatched, vec![0]);
        // The normal map is assigned
        assert!(ir.materials[0].normal_texture.is_some());
    }

    #[test]
    fn test_embed_normal_reuses_added_guid() {
        // The same normal-map GUID is shared by two materials
        let mut ir = crate::intermediate::types::IrModel::default();
        for name in &["Body", "Face"] {
            let mat = crate::intermediate::types::IrMaterial {
                name: (*name).into(),
                texture_index: Some(0),
                ..Default::default()
            };
            ir.materials.push(mat);
        }
        ir.textures.push(crate::intermediate::types::IrTexture {
            filename: "base.png".into(),
            data: TextureData::Encoded(Arc::from(vec![0u8; 4])),
            mime_type: "image/png".into(),
            source_path: String::new(),
            mip_chain: None,
        });

        let textures = vec![PackageTexture {
            guid: Arc::from("shared-normal"),
            display_name: Arc::from("shared_n.png"),
            data: Arc::from(vec![2u8; 4].as_slice()),
            pathname: Arc::from(""),
        }];
        let resolved = vec![
            ResolvedMaterialTextures {
                source_material: None,
                material_name: Arc::from("Body"),
                main_texture_guid: None,
                normal_texture_guid: Some(Arc::from("shared-normal")),
                bump_scale: 1.0,
                fbx_material_name: None,
                emission_texture_guid: None,
                emission_color: [0.0; 3],
                emission_enabled: false,
                emission_blend: 0,
            },
            ResolvedMaterialTextures {
                source_material: None,
                material_name: Arc::from("Face"),
                main_texture_guid: None,
                normal_texture_guid: Some(Arc::from("shared-normal")),
                bump_scale: 1.0,
                fbx_material_name: None,
                emission_texture_guid: None,
                emission_color: [0.0; 3],
                emission_enabled: false,
                emission_blend: 0,
            },
        ];

        let _unmatched = embed_textures_with_prefab(&mut ir, &textures, &resolved, "test");

        // Both materials reference the same texture index
        let idx0 = ir.materials[0]
            .normal_texture
            .as_ref()
            .map(|t| t.index)
            .unwrap();
        let idx1 = ir.materials[1]
            .normal_texture
            .as_ref()
            .map(|t| t.index)
            .unwrap();
        assert_eq!(idx0, idx1);
        // The texture is only added once (base + normal = 2)
        assert_eq!(ir.textures.len(), 2);
    }

    #[test]
    fn test_score_prefab_path() {
        let fbx = "Assets/Models/Character/body.fbx";
        assert!(
            score_prefab_path("Assets/Models/Character/scene.prefab", fbx)
                > score_prefab_path("Assets/Scenes/main.prefab", fbx)
        );
    }

    #[test]
    fn test_is_image_extension() {
        assert!(is_image_extension("textures/body.png"));
        assert!(is_image_extension("TEXTURES/FACE.TGA"));
        assert!(is_image_extension("model.psd"));
        assert!(!is_image_extension("model.fbx"));
        assert!(!is_image_extension("scene.prefab"));
    }

    /// Mixed-format Prefab: PrefabInstance (New) + SkinnedMeshRenderer (Old) coexist.
    const MIXED_PREFAB_SAMPLE: &str = r#"
%YAML 1.1
--- !u!1001 &1234567890
PrefabInstance:
  m_ObjectHideFlags: 0
  serializedVersion: 2
  m_Modification:
    m_TransformParent: {fileID: 0}
    m_Modifications:
    - target: {fileID: 1889091244555525569, guid: aaaa1111bbbb2222cccc3333dddd4444, type: 3}
      propertyPath: m_Materials.Array.data[0]
      value:
      objectReference: {fileID: 2100000, guid: aabbccdd00112233aabbccdd00112233, type: 2}
    m_RemovedComponents: []
  m_SourcePrefab: {fileID: 100100000, guid: aaaa1111bbbb2222cccc3333dddd4444, type: 3}
--- !u!137 &137310319924682442
SkinnedMeshRenderer:
  m_ObjectHideFlags: 1
  m_Materials:
  - {fileID: 2100000, guid: 00000000111111112222222233333333, type: 2}
  m_Mesh: {fileID: 4300116, guid: 44444444555555556666666677777777, type: 3}
"#;

    #[test]
    fn test_mixed_prefab_parses_both_formats() {
        // Mixed-format should be detected as New (PrefabInstance: present)
        assert_eq!(detect_prefab_format(MIXED_PREFAB_SAMPLE), PrefabFormat::New);

        // New-format parse should find the PrefabInstance block
        let new_infos = parse_prefab_new(MIXED_PREFAB_SAMPLE).unwrap();
        assert_eq!(new_infos.len(), 1);
        assert_eq!(
            new_infos[0].source_fbx_guid,
            "aaaa1111bbbb2222cccc3333dddd4444"
        );
        assert_eq!(new_infos[0].material_overrides.len(), 1);

        // Old-format parse should find the SkinnedMeshRenderer block
        let old_infos = parse_prefab_old(MIXED_PREFAB_SAMPLE).unwrap();
        assert_eq!(old_infos.len(), 1);
        assert_eq!(old_infos[0].fbx_guid, "44444444555555556666666677777777");
        assert_eq!(old_infos[0].material_guids.len(), 1);
    }

    #[test]
    fn test_mixed_prefab_cache_has_both_infos() {
        // Build a minimal UnityPackageIndex with a mixed-format Prefab and two FBX entries
        let prefab_data: Arc<[u8]> = Arc::from(MIXED_PREFAB_SAMPLE.as_bytes().to_vec());
        let new_fbx_guid = "aaaa1111bbbb2222cccc3333dddd4444";
        let old_fbx_guid = "44444444555555556666666677777777";

        let entries = vec![
            AssetEntry {
                guid: "prefab_guid_0000000000000000000000".into(),
                pathname: "Assets/scene.prefab".into(),
                data: prefab_data,
                meta: None,
            },
            AssetEntry {
                guid: new_fbx_guid.into(),
                pathname: "Assets/new_model.fbx".into(),
                data: Arc::from(vec![0u8; 4]),
                meta: None,
            },
            AssetEntry {
                guid: old_fbx_guid.into(),
                pathname: "Assets/old_model.fbx".into(),
                data: Arc::from(vec![0u8; 4]),
                meta: None,
            },
        ];

        let mut index = UnityPackageIndex {
            by_guid: entries
                .iter()
                .enumerate()
                .map(|(i, e)| (e.guid.clone(), i))
                .collect(),
            by_path: entries
                .iter()
                .enumerate()
                .map(|(i, e)| (e.pathname.clone(), i))
                .collect(),
            entries,
            prefab_by_fbx_guid: HashMap::new(),
            prefab_cache: HashMap::new(),
            variant_cache: HashMap::new(),
        };

        build_prefab_fbx_map(&mut index);

        // Cache should have both new_infos and old_infos for the mixed Prefab
        let cache = index.prefab_cache.get(&0).expect("prefab should be cached");
        assert_eq!(cache.format, PrefabFormat::New);
        assert!(!cache.new_infos.is_empty(), "new_infos should not be empty");
        assert!(
            !cache.old_infos.is_empty(),
            "old_infos should not be empty for mixed-format Prefab"
        );

        // prefab_by_fbx_guid should map BOTH FBX GUIDs to this Prefab
        assert!(
            index.prefab_by_fbx_guid.contains_key(new_fbx_guid),
            "New-format FBX GUID should be in prefab_by_fbx_guid"
        );
        assert!(
            index.prefab_by_fbx_guid.contains_key(old_fbx_guid),
            "Old-format FBX GUID should be in prefab_by_fbx_guid"
        );
    }
}
