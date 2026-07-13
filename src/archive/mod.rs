//! Direct archive (ZIP / 7z / RAR) loading.
//!
//! Provides a uniform interface for detecting model files (VRM/FBX/PMX/PMD)
//! inside an archive and extracting them together with their textures.
//! Encrypted archives are supported by passing a password down from the
//! caller; the password is held in memory only and never persisted.

pub mod rar;
pub mod sevenz;
pub mod zip_extract;

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::error::{PoponeError, Result};
use rust_i18n::t;

/// Archive-supported model extensions (`unitypackage` is supported via a second-stage extraction).
pub const MODEL_EXTENSIONS: &[&str] = &[
    "vrm",
    "glb",
    "fbx",
    "pmx",
    "pmd",
    "obj",
    "stl",
    "x",
    "unitypackage",
];

/// Texture extensions (`psd` excluded -- multi-hundred-MB files risk OOM).
pub const TEXTURE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "tga", "bmp", "tif", "tiff", "dds"];

/// Extraction size cap: 2 GB.
const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Archive entry metadata (used for listing; no payload).
pub struct ArchiveEntryMeta {
    pub path: PathBuf,
    pub size: u64,
}

/// Extracted archive entry (with payload).
pub struct ArchiveEntry {
    pub path: PathBuf,
    pub data: Vec<u8>,
}

/// Archive format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    SevenZ,
    Rar,
}

/// Model kind inside the archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveModelKind {
    Vrm,
    Glb,
    Fbx,
    Pmx,
    Pmd,
    Obj,
    Stl,
    DirectX,
    UnityPackage,
}

impl ArchiveModelKind {
    fn from_ext(ext: &str) -> Option<Self> {
        match ext {
            "vrm" => Some(Self::Vrm),
            "glb" => Some(Self::Glb),
            "fbx" => Some(Self::Fbx),
            "pmx" => Some(Self::Pmx),
            "pmd" => Some(Self::Pmd),
            "obj" => Some(Self::Obj),
            "stl" => Some(Self::Stl),
            "x" => Some(Self::DirectX),
            "unitypackage" => Some(Self::UnityPackage),
            _ => None,
        }
    }

    /// Display label for the UI.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Vrm => "VRM",
            Self::Glb => "GLB",
            Self::Fbx => "FBX",
            Self::Pmx => "PMX",
            Self::Pmd => "PMD",
            Self::Obj => "OBJ",
            Self::Stl => "STL",
            Self::DirectX => "DirectX",
            Self::UnityPackage => "UnityPackage",
        }
    }
}

/// Model-extraction result.
pub struct ModelBundle {
    pub model: ArchiveEntry,
    pub kind: ArchiveModelKind,
    /// For FBX/VRM: texture files as (filename, data).
    pub textures: Vec<(String, Vec<u8>)>,
    /// For PMX/PMD: auxiliary files keyed by relative path -> bytes.
    pub aux_files: HashMap<PathBuf, Arc<[u8]>>,
}

/// Result of an archive model listing.
pub struct ArchiveContents {
    /// (index, normalized internal path, display filename, kind).
    pub models: Vec<(usize, PathBuf, String, ArchiveModelKind)>,
    /// For 7z: holds every extracted entry.
    entries: Option<Vec<ArchiveEntry>>,
    /// For ZIP: only metadata.
    metas: Option<Vec<ArchiveEntryMeta>>,
}

/// Normalize a path (rejects `..` and absolute-path components).
pub fn normalize_archive_path(raw: &str) -> Result<PathBuf> {
    let cleaned = raw.replace('\\', "/");
    let mut out = PathBuf::new();
    for c in Path::new(&cleaned).components() {
        match c {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => {
                return Err(PoponeError::Archive(
                    t!("error.archive.unsafe_path", raw = raw.to_string()).to_string(),
                ))
            }
        }
    }
    Ok(out)
}

/// Determine the archive format from a file extension.
pub fn archive_format_from_ext(ext: &str) -> Option<ArchiveFormat> {
    match ext {
        "zip" => Some(ArchiveFormat::Zip),
        "7z" => Some(ArchiveFormat::SevenZ),
        "rar" => Some(ArchiveFormat::Rar),
        _ => None,
    }
}

/// List models inside an archive.
///
/// **Note**: 7z and RAR are solid/streaming formats, forcing us to fully extract
/// every file matching the target extensions and keep them in memory (capped by
/// `MAX_TOTAL_BYTES`). ZIP only fetches metadata. The extracted entries are kept
/// inside `ArchiveContents` so that `extract_model_bundle` can reuse them without
/// re-extracting.
///
/// `password`: for encrypted archives. Encryption raises
/// `PoponeError::ArchivePasswordRequired` / `ArchiveBadPassword` so the viewer
/// can prompt the user and retry. ZIP never needs it at the listing stage
/// (only entry payloads are encrypted).
pub fn list_models(
    data: &[u8],
    format: ArchiveFormat,
    password: Option<&str>,
) -> Result<ArchiveContents> {
    match format {
        ArchiveFormat::Zip => {
            let metas = zip_extract::list_entries(data)?;
            let models = find_models_from_metas(&metas);
            Ok(ArchiveContents {
                models,
                entries: None,
                metas: Some(metas),
            })
        }
        ArchiveFormat::SevenZ => {
            let entries = sevenz::extract_filtered(data, MAX_TOTAL_BYTES, password)?;
            let models = find_models_from_entries(&entries);
            Ok(ArchiveContents {
                models,
                entries: Some(entries),
                metas: None,
            })
        }
        ArchiveFormat::Rar => {
            let entries = rar::extract_filtered(data, MAX_TOTAL_BYTES, password)?;
            let models = find_models_from_entries(&entries);
            Ok(ArchiveContents {
                models,
                entries: Some(entries),
                metas: None,
            })
        }
    }
}

/// Detect models from a metadata listing.
fn find_models_from_metas(
    metas: &[ArchiveEntryMeta],
) -> Vec<(usize, PathBuf, String, ArchiveModelKind)> {
    let mut models = Vec::new();
    for (i, meta) in metas.iter().enumerate() {
        if let Some(kind) = path_to_model_kind(&meta.path) {
            let display = meta
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            models.push((i, meta.path.clone(), display, kind));
        }
    }
    models
}

/// Detect models from an entry listing.
fn find_models_from_entries(
    entries: &[ArchiveEntry],
) -> Vec<(usize, PathBuf, String, ArchiveModelKind)> {
    let mut models = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        if let Some(kind) = path_to_model_kind(&entry.path) {
            let display = entry
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            models.push((i, entry.path.clone(), display, kind));
        }
    }
    models
}

fn path_to_model_kind(path: &Path) -> Option<ArchiveModelKind> {
    let ext = crate::path_ext_lower(path);
    if ext.is_empty() {
        return None;
    }
    ArchiveModelKind::from_ext(&ext)
}

/// Extract the selected model plus its associated files.
/// `password`: only consulted for ZIP (7z/RAR entries were already decrypted
/// at the listing stage and live in `contents.entries`).
pub fn extract_model_bundle(
    data: &[u8],
    format: ArchiveFormat,
    contents: ArchiveContents,
    model_index: usize,
    password: Option<&str>,
) -> Result<ModelBundle> {
    let (_, model_path, _, kind) = contents.models.get(model_index).ok_or_else(|| {
        PoponeError::Archive(
            t!(
                "error.archive.model_index_out_of_range",
                index = model_index.to_string()
            )
            .to_string(),
        )
    })?;
    let model_path = model_path.clone();
    let kind = *kind;

    match format {
        ArchiveFormat::Zip => {
            extract_bundle_from_zip(data, &model_path, kind, contents.metas.as_deref(), password)
        }
        ArchiveFormat::SevenZ | ArchiveFormat::Rar => {
            extract_bundle_from_entries(contents.entries.unwrap_or_default(), &model_path, kind)
        }
    }
}

/// Extract the selected model + its associated files from a ZIP.
fn extract_bundle_from_zip(
    data: &[u8],
    model_path: &Path,
    kind: ArchiveModelKind,
    metas: Option<&[ArchiveEntryMeta]>,
    password: Option<&str>,
) -> Result<ModelBundle> {
    match kind {
        ArchiveModelKind::Pmx | ArchiveModelKind::Pmd => {
            // PMX/PMD: extract the model first to discover texture references, then extract just the needed files
            let model_entries =
                zip_extract::extract_files(data, &[model_path], MAX_TOTAL_BYTES, password)?;
            let model_entry = model_entries
                .into_iter()
                .find(|e| e.path == model_path)
                .ok_or_else(|| {
                    PoponeError::Archive(
                        t!(
                            "error.archive.model_file_extract_failed",
                            path = model_path.display().to_string()
                        )
                        .to_string(),
                    )
                })?;

            // Read out the referenced texture paths
            let tex_refs = get_texture_refs_from_model(&model_entry.data, kind)?;
            let model_dir = model_path.parent().unwrap_or(Path::new(""));

            // Pick the files that are actually needed
            let needed: Vec<PathBuf> = if let Some(metas) = metas {
                collect_needed_paths(metas.iter().map(|m| &m.path), &tex_refs, model_dir)
            } else {
                Vec::new()
            };

            let aux_files = if !needed.is_empty() {
                let needed_refs: Vec<&Path> = needed.iter().map(|p| p.as_path()).collect();
                let remaining = MAX_TOTAL_BYTES.saturating_sub(model_entry.data.len() as u64);
                let aux_entries =
                    zip_extract::extract_files(data, &needed_refs, remaining, password)?;
                build_aux_files(aux_entries, model_dir)
            } else {
                HashMap::new()
            };

            Ok(ModelBundle {
                model: model_entry,
                kind,
                textures: Vec::new(),
                aux_files,
            })
        }
        ArchiveModelKind::Stl | ArchiveModelKind::UnityPackage => {
            // STL / UnityPackage: extract only the model itself (no textures needed)
            let model_entries =
                zip_extract::extract_files(data, &[model_path], MAX_TOTAL_BYTES, password)?;
            let model_entry = model_entries
                .into_iter()
                .find(|e| e.path == model_path)
                .ok_or_else(|| {
                    PoponeError::Archive(
                        t!(
                            "error.archive.model_file_extract_failed",
                            path = model_path.display().to_string()
                        )
                        .to_string(),
                    )
                })?;
            Ok(ModelBundle {
                model: model_entry,
                kind,
                textures: Vec::new(),
                aux_files: HashMap::new(),
            })
        }
        _ => {
            // VRM/GLB/FBX/OBJ/STL: extract the model plus textures under the same directory
            let model_dir = model_path.parent().unwrap_or(Path::new(""));
            let mut paths_to_extract = vec![model_path.to_path_buf()];

            if let Some(metas) = metas {
                for meta in metas {
                    if is_texture_in_scope(&meta.path, model_dir) {
                        paths_to_extract.push(meta.path.clone());
                    } else if (kind == ArchiveModelKind::DirectX || kind == ArchiveModelKind::Obj)
                        && is_texture_near_model(&meta.path, model_dir)
                    {
                        // Collect textures outside the model directory too (e.g. "../textures/foo.png")
                        paths_to_extract.push(meta.path.clone());
                    }
                    // OBJ: also collect the .mtl sidecar
                    if kind == ArchiveModelKind::Obj
                        && is_sidecar_in_scope(&meta.path, model_dir, "mtl")
                    {
                        paths_to_extract.push(meta.path.clone());
                    }
                }
            }

            let path_refs: Vec<&Path> = paths_to_extract.iter().map(|p| p.as_path()).collect();
            let entries = zip_extract::extract_files(data, &path_refs, MAX_TOTAL_BYTES, password)?;

            let mut model_entry = None;
            let mut textures = Vec::new();
            let mut aux_files: HashMap<PathBuf, Arc<[u8]>> = HashMap::new();
            for entry in entries {
                if entry.path == model_path {
                    model_entry = Some(entry);
                } else if kind == ArchiveModelKind::Obj || kind == ArchiveModelKind::DirectX {
                    // OBJ/DirectX: store the path normalized relative to model_dir.
                    // Apply the same normalization as `normalize_rel_path` in resolve_texture so the keys line up.
                    let rel = if let Ok(r) = entry.path.strip_prefix(model_dir) {
                        r.to_path_buf()
                    } else {
                        // File outside model_dir: compute and normalize the relative path
                        relative_from_model_dir(model_dir, &entry.path)
                    };
                    aux_files.insert(rel, Arc::from(entry.data.into_boxed_slice()));
                } else {
                    let filename = entry
                        .path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    textures.push((filename, entry.data));
                }
            }

            Ok(ModelBundle {
                model: model_entry.ok_or_else(|| {
                    PoponeError::Archive(
                        t!("error.archive.model_file_extract_failed_simple").to_string(),
                    )
                })?,
                kind,
                textures,
                aux_files,
            })
        }
    }
}

/// Build a bundle from previously extracted 7z entries.
fn extract_bundle_from_entries(
    entries: Vec<ArchiveEntry>,
    model_path: &Path,
    kind: ArchiveModelKind,
) -> Result<ModelBundle> {
    let model_dir = model_path.parent().unwrap_or(Path::new(""));
    let mut model_entry = None;
    let mut other_entries = Vec::new();

    for entry in entries {
        if entry.path == model_path {
            model_entry = Some(entry);
        } else {
            other_entries.push(entry);
        }
    }

    let model_entry = model_entry.ok_or_else(|| {
        PoponeError::Archive(
            t!(
                "error.archive.model_file_not_found",
                path = model_path.display().to_string()
            )
            .to_string(),
        )
    })?;

    match kind {
        ArchiveModelKind::Pmx | ArchiveModelKind::Pmd => {
            let tex_refs = get_texture_refs_from_model(&model_entry.data, kind)?;
            let aux_files = build_aux_from_entries_pmx(other_entries, &tex_refs, model_dir);
            Ok(ModelBundle {
                model: model_entry,
                kind,
                textures: Vec::new(),
                aux_files,
            })
        }
        ArchiveModelKind::Stl | ArchiveModelKind::UnityPackage => {
            // STL / UnityPackage: model only (no textures)
            Ok(ModelBundle {
                model: model_entry,
                kind,
                textures: Vec::new(),
                aux_files: HashMap::new(),
            })
        }
        _ => {
            let relevant: Vec<ArchiveEntry> = other_entries
                .into_iter()
                .filter(|e| {
                    is_texture_in_scope(&e.path, model_dir)
                        || (kind == ArchiveModelKind::Obj
                            && is_sidecar_in_scope(&e.path, model_dir, "mtl"))
                        || ((kind == ArchiveModelKind::DirectX || kind == ArchiveModelKind::Obj)
                            && is_texture_near_model(&e.path, model_dir))
                })
                .collect();

            if kind == ArchiveModelKind::Obj || kind == ArchiveModelKind::DirectX {
                // OBJ/DirectX: store paths normalized relative to model_dir
                let mut aux_files: HashMap<PathBuf, Arc<[u8]>> = HashMap::new();
                for e in relevant {
                    let rel = if let Ok(r) = e.path.strip_prefix(model_dir) {
                        r.to_path_buf()
                    } else {
                        // File outside model_dir: compute and normalize the relative path
                        relative_from_model_dir(model_dir, &e.path)
                    };
                    aux_files.insert(rel, Arc::from(e.data.into_boxed_slice()));
                }
                Ok(ModelBundle {
                    model: model_entry,
                    kind,
                    textures: Vec::new(),
                    aux_files,
                })
            } else {
                let textures: Vec<(String, Vec<u8>)> = relevant
                    .into_iter()
                    .map(|e| {
                        let filename = e
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        (filename, e.data)
                    })
                    .collect();
                Ok(ModelBundle {
                    model: model_entry,
                    kind,
                    textures,
                    aux_files: HashMap::new(),
                })
            }
        }
    }
}

/// Extract the list of texture references from a PMX/PMD model.
fn get_texture_refs_from_model(data: &[u8], kind: ArchiveModelKind) -> Result<Vec<String>> {
    match kind {
        ArchiveModelKind::Pmx => {
            let pmx = crate::pmx::reader::read_pmx_from_data(data)?;
            Ok(pmx.textures)
        }
        ArchiveModelKind::Pmd => {
            let pmd = crate::pmd::reader::read_pmd_from_data(data)?;
            let mut refs = Vec::new();
            for mat in &pmd.materials {
                if !mat.texture_name.is_empty() {
                    // PMD texture names use "*" to separate the sphere texture
                    for part in mat.texture_name.split('*') {
                        let trimmed = part.trim();
                        if !trimmed.is_empty() {
                            refs.push(trimmed.to_string());
                        }
                    }
                }
            }
            // Toon textures
            for toon in &pmd.toon_textures {
                if !toon.is_empty() {
                    refs.push(toon.clone());
                }
            }
            refs.sort();
            refs.dedup();
            Ok(refs)
        }
        _ => Ok(Vec::new()),
    }
}

/// Collect archive paths that match the texture references.
fn collect_needed_paths<'a>(
    archive_paths: impl Iterator<Item = &'a PathBuf>,
    tex_refs: &[String],
    model_dir: &Path,
) -> Vec<PathBuf> {
    let mut needed = Vec::new();
    let archive_paths: Vec<&PathBuf> = archive_paths.collect();

    for tex_ref in tex_refs {
        let ref_path = PathBuf::from(tex_ref.replace('\\', "/"));
        // Resolve relative to the model's parent directory
        let resolved = normalize_relative_path(&model_dir.join(&ref_path));

        for &ap in &archive_paths {
            // Exact match
            if *ap == resolved {
                needed.push(ap.clone());
                continue;
            }
            // Case-insensitive fallback
            if path_eq_ignore_case(ap, &resolved) {
                needed.push(ap.clone());
                continue;
            }
            // PMD: match by basename only
            if ref_path.components().count() == 1 {
                if let (Some(a_name), Some(r_name)) = (ap.file_name(), ref_path.file_name()) {
                    if a_name.to_string_lossy().to_lowercase()
                        == r_name.to_string_lossy().to_lowercase()
                    {
                        // Same directory or any subdirectory
                        if ap.starts_with(model_dir) || model_dir == Path::new("") {
                            needed.push(ap.clone());
                        }
                    }
                }
            }
        }
    }
    // Also collect .txt files (e.g. PMD/PMX readmes)
    for &ap in &archive_paths {
        let ext = crate::path_ext_lower(ap);
        if ext == "txt" && (ap.starts_with(model_dir) || model_dir == Path::new("")) {
            needed.push(ap.clone());
        }
    }
    needed.sort();
    needed.dedup();
    needed
}

/// Build `aux_files` keyed by relative paths from the model's parent directory.
fn build_aux_files(entries: Vec<ArchiveEntry>, model_dir: &Path) -> HashMap<PathBuf, Arc<[u8]>> {
    let mut aux = HashMap::new();
    for entry in entries {
        let rel = if model_dir == Path::new("") {
            entry.path.clone()
        } else {
            entry
                .path
                .strip_prefix(model_dir)
                .unwrap_or(&entry.path)
                .to_path_buf()
        };
        aux.insert(rel, Arc::from(entry.data.into_boxed_slice()));
    }
    aux
}

/// Build PMX/PMD `aux_files` from previously extracted 7z entries.
fn build_aux_from_entries_pmx(
    entries: Vec<ArchiveEntry>,
    tex_refs: &[String],
    model_dir: &Path,
) -> HashMap<PathBuf, Arc<[u8]>> {
    let mut aux = HashMap::new();

    for entry in entries {
        let ext = crate::path_ext_lower(&entry.path);

        let is_needed = if ext == "txt" {
            entry.path.starts_with(model_dir) || model_dir == Path::new("")
        } else {
            // Match against the texture reference paths
            tex_refs.iter().any(|tex_ref| {
                let ref_path = PathBuf::from(tex_ref.replace('\\', "/"));
                let resolved = normalize_relative_path(&model_dir.join(&ref_path));
                entry.path == resolved
                    || path_eq_ignore_case(&entry.path, &resolved)
                    || (ref_path.components().count() == 1
                        && {
                            entry
                                .path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .map(|n| n.to_lowercase())
                                == ref_path
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .map(|n| n.to_lowercase())
                        }
                        && (entry.path.starts_with(model_dir) || model_dir == Path::new("")))
            })
        };

        if is_needed {
            let rel = if model_dir == Path::new("") {
                entry.path.clone()
            } else {
                entry
                    .path
                    .strip_prefix(model_dir)
                    .unwrap_or(&entry.path)
                    .to_path_buf()
            };
            aux.insert(rel, Arc::from(entry.data.into_boxed_slice()));
        }
    }
    aux
}

/// Whether a sidecar file with the given extension is in scope.
/// Allows up to model_dir's parent (so references like `../shared/materials.mtl` work).
fn is_sidecar_in_scope(path: &Path, model_dir: &Path, ext: &str) -> bool {
    let file_ext = crate::path_ext_lower(path);
    if file_ext != ext {
        return false;
    }
    let parent = model_dir.parent().unwrap_or(Path::new(""));
    parent == Path::new("") || path.starts_with(parent)
}

/// Whether the path has a texture extension and lives under the model's parent directory.
/// Allows one level above model_dir while excluding unrelated textures elsewhere in the archive.
fn is_texture_near_model(path: &Path, model_dir: &Path) -> bool {
    let ext = crate::path_ext_lower(path);
    if !TEXTURE_EXTENSIONS.contains(&ext.as_str()) {
        return false;
    }
    // Collect anything under model_dir's parent directory
    let parent = model_dir.parent().unwrap_or(Path::new(""));
    parent == Path::new("") || path.starts_with(parent)
}

fn is_texture_in_scope(path: &Path, model_dir: &Path) -> bool {
    let ext = crate::path_ext_lower(path);
    if !TEXTURE_EXTENSIONS.contains(&ext.as_str()) {
        return false;
    }
    model_dir == Path::new("") || path.starts_with(model_dir)
}

/// Normalize a relative path (`foo/../bar` -> `bar`).
fn normalize_relative_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => out.push(c.as_os_str()),
        }
    }
    out
}

/// Compute the relative path from `source_dir` to `target` (no normalization; keeps `..`).
/// Example: source_dir = "assets/models", target = "assets/shared/body.png" -> "../shared/body.png".
fn relative_from_model_dir(source_dir: &Path, target: &Path) -> PathBuf {
    let source_components: Vec<_> = source_dir.components().collect();
    let target_components: Vec<_> = target.components().collect();
    let common_len = source_components
        .iter()
        .zip(target_components.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let mut rel = PathBuf::new();
    for _ in common_len..source_components.len() {
        rel.push("..");
    }
    for comp in &target_components[common_len..] {
        rel.push(comp);
    }
    // Normalize backslashes to forward slashes
    PathBuf::from(rel.to_string_lossy().replace('\\', "/"))
}

/// Case-insensitive path comparison.
fn path_eq_ignore_case(a: &Path, b: &Path) -> bool {
    let a_str = a.to_string_lossy().to_lowercase();
    let b_str = b.to_string_lossy().to_lowercase();
    a_str == b_str
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_archive_path_normal() {
        let p = normalize_archive_path("model/texture/body.png").unwrap();
        assert_eq!(p, PathBuf::from("model/texture/body.png"));
    }

    #[test]
    fn test_normalize_archive_path_backslash() {
        let p = normalize_archive_path("model\\texture\\body.png").unwrap();
        assert_eq!(p, PathBuf::from("model/texture/body.png"));
    }

    #[test]
    fn test_normalize_archive_path_dotdot_rejected() {
        assert!(normalize_archive_path("../etc/passwd").is_err());
        assert!(normalize_archive_path("model/../../etc/passwd").is_err());
    }

    #[test]
    fn test_normalize_archive_path_current_dir() {
        let p = normalize_archive_path("./model/./body.pmx").unwrap();
        assert_eq!(p, PathBuf::from("model/body.pmx"));
    }

    #[test]
    fn test_find_model_list() {
        let metas = vec![
            ArchiveEntryMeta {
                path: PathBuf::from("readme.txt"),
                size: 100,
            },
            ArchiveEntryMeta {
                path: PathBuf::from("model/test.pmx"),
                size: 50000,
            },
            ArchiveEntryMeta {
                path: PathBuf::from("model/textures/body.png"),
                size: 10000,
            },
            ArchiveEntryMeta {
                path: PathBuf::from("other.fbx"),
                size: 20000,
            },
        ];
        let models = find_models_from_metas(&metas);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].3, ArchiveModelKind::Pmx);
        assert_eq!(models[1].3, ArchiveModelKind::Fbx);
    }

    #[test]
    fn test_find_model_list_with_unitypackage() {
        let metas = vec![
            ArchiveEntryMeta {
                path: PathBuf::from("readme.txt"),
                size: 100,
            },
            ArchiveEntryMeta {
                path: PathBuf::from("avatar.unitypackage"),
                size: 500000,
            },
            ArchiveEntryMeta {
                path: PathBuf::from("model.vrm"),
                size: 200000,
            },
        ];
        let models = find_models_from_metas(&metas);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].3, ArchiveModelKind::UnityPackage);
        assert_eq!(models[0].2, "avatar.unitypackage");
        assert_eq!(models[1].3, ArchiveModelKind::Vrm);
    }

    #[test]
    fn test_unitypackage_extracts_only_model() {
        // Verify that when a ZIP contains a `.unitypackage` plus unrelated images,
        // the UnityPackage extraction does not pull those textures in.
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            writer.start_file("avatar.unitypackage", options).unwrap();
            std::io::Write::write_all(&mut writer, b"fake unitypackage data").unwrap();
            writer.start_file("unrelated_large.png", options).unwrap();
            std::io::Write::write_all(&mut writer, &vec![0u8; 10000]).unwrap();
            writer.start_file("textures/body.png", options).unwrap();
            std::io::Write::write_all(&mut writer, &vec![0u8; 5000]).unwrap();
            writer.finish().unwrap();
        }

        let contents = list_models(&buf, ArchiveFormat::Zip, None).unwrap();
        assert_eq!(contents.models.len(), 1);
        assert_eq!(contents.models[0].3, ArchiveModelKind::UnityPackage);

        let bundle = extract_model_bundle(&buf, ArchiveFormat::Zip, contents, 0, None).unwrap();
        // Only the model itself is extracted; textures are empty
        assert_eq!(bundle.model.data, b"fake unitypackage data");
        assert!(
            bundle.textures.is_empty(),
            "UnityPackage 抽出時にテクスチャを巻き込んではならない"
        );
        assert!(bundle.aux_files.is_empty());
    }

    #[test]
    fn test_relative_from_model_dir() {
        // File outside model_dir: ".." segments are preserved
        let rel = relative_from_model_dir(
            Path::new("assets/models"),
            Path::new("assets/shared/body.png"),
        );
        assert_eq!(rel, PathBuf::from("../shared/body.png"));

        // File inside model_dir: no ".."
        let rel2 = relative_from_model_dir(
            Path::new("assets/models"),
            Path::new("assets/models/tex/body.png"),
        );
        assert_eq!(rel2, PathBuf::from("tex/body.png"));

        // Two levels above the root: two ".." segments are preserved
        let rel3 = relative_from_model_dir(Path::new("a/b/c"), Path::new("a/other/body.png"));
        assert_eq!(rel3, PathBuf::from("../../other/body.png"));
    }

    #[test]
    fn test_path_eq_ignore_case() {
        assert!(path_eq_ignore_case(
            Path::new("model/Textures/Body.PNG"),
            Path::new("model/textures/body.png"),
        ));
        assert!(!path_eq_ignore_case(
            Path::new("model/other.png"),
            Path::new("model/body.png"),
        ));
    }

    #[test]
    fn test_normalize_relative_path() {
        let p = normalize_relative_path(Path::new("model/../textures/body.png"));
        assert_eq!(p, PathBuf::from("textures/body.png"));
    }

    #[test]
    fn test_zip_roundtrip() {
        // Build a ZIP programmatically for the test
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            writer.start_file("test/model.pmx", options).unwrap();
            // PMX magic + minimal header (parsing will fail, but list_entries works)
            std::io::Write::write_all(&mut writer, b"PMX test data").unwrap();
            writer.start_file("test/texture.png", options).unwrap();
            std::io::Write::write_all(&mut writer, b"PNG fake data").unwrap();
            writer.start_file("other/readme.txt", options).unwrap();
            std::io::Write::write_all(&mut writer, b"readme").unwrap();
            writer.finish().unwrap();
        }

        let contents = list_models(&buf, ArchiveFormat::Zip, None).unwrap();
        assert_eq!(contents.models.len(), 1);
        assert_eq!(contents.models[0].2, "model.pmx");
        assert_eq!(contents.models[0].3, ArchiveModelKind::Pmx);
    }

    #[test]
    fn test_broken_archive_error() {
        // Corrupt data must return an error
        let result = list_models(b"this is not a zip file", ArchiveFormat::Zip, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_archive() {
        let mut buf = Vec::new();
        {
            let writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            writer.finish().unwrap();
        }
        let contents = list_models(&buf, ArchiveFormat::Zip, None).unwrap();
        assert!(contents.models.is_empty());
    }

    /// Build an AES-256 encrypted ZIP containing one STL entry
    /// (STL is extracted without parsing, keeping the test focused on decryption).
    fn build_encrypted_zip(password: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored)
                .with_aes_encryption(zip::AesMode::Aes256, password);
            writer.start_file("model/test.stl", options).unwrap();
            std::io::Write::write_all(&mut writer, b"STL secret data").unwrap();
            writer.finish().unwrap();
        }
        buf
    }

    #[test]
    fn test_zip_encrypted_password_flow() {
        let buf = build_encrypted_zip("secret");

        // Listing works without a password (only entry payloads are encrypted).
        let contents = list_models(&buf, ArchiveFormat::Zip, None).unwrap();
        assert_eq!(contents.models.len(), 1);

        // Extracting without a password reports that one is required.
        let Err(err) = extract_model_bundle(&buf, ArchiveFormat::Zip, contents, 0, None) else {
            panic!("extraction without a password must fail");
        };
        assert!(
            matches!(err, PoponeError::ArchivePasswordRequired),
            "expected ArchivePasswordRequired, got: {err:?}"
        );

        // A wrong password is rejected (AES stores a password verifier).
        let contents = list_models(&buf, ArchiveFormat::Zip, None).unwrap();
        let Err(err) = extract_model_bundle(&buf, ArchiveFormat::Zip, contents, 0, Some("wrong"))
        else {
            panic!("extraction with a wrong password must fail");
        };
        assert!(
            matches!(err, PoponeError::ArchiveBadPassword),
            "expected ArchiveBadPassword, got: {err:?}"
        );

        // The correct password decrypts the payload.
        let contents = list_models(&buf, ArchiveFormat::Zip, None).unwrap();
        let bundle =
            extract_model_bundle(&buf, ArchiveFormat::Zip, contents, 0, Some("secret")).unwrap();
        assert_eq!(bundle.model.data, b"STL secret data");
    }

    /// Build an AES-256 encrypted 7z (with encrypted headers) containing one STL entry.
    fn build_encrypted_7z(password: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut writer =
                sevenz_rust2::ArchiveWriter::new(std::io::Cursor::new(&mut buf)).unwrap();
            writer.set_content_methods(vec![
                sevenz_rust2::encoder_options::AesEncoderOptions::new(sevenz_rust2::Password::new(
                    password,
                ))
                .into(),
                sevenz_rust2::encoder_options::Lzma2Options::default().into(),
            ]);
            let entry = sevenz_rust2::ArchiveEntry::new_file("model/test.stl");
            writer
                .push_archive_entry(entry, Some(&b"STL secret data"[..]))
                .unwrap();
            writer.finish().unwrap();
        }
        buf
    }

    #[test]
    fn test_sevenz_encrypted_password_flow() {
        let buf = build_encrypted_7z("secret");

        // 7z extracts at listing time, so the password failure surfaces here.
        let Err(err) = list_models(&buf, ArchiveFormat::SevenZ, None) else {
            panic!("listing an encrypted 7z without a password must fail");
        };
        assert!(
            matches!(err, PoponeError::ArchivePasswordRequired),
            "expected ArchivePasswordRequired, got: {err:?}"
        );

        // A wrong password must not succeed (BadPassword or corruption).
        assert!(list_models(&buf, ArchiveFormat::SevenZ, Some("wrong")).is_err());

        // The correct password decrypts everything.
        let contents = list_models(&buf, ArchiveFormat::SevenZ, Some("secret")).unwrap();
        assert_eq!(contents.models.len(), 1);
        assert_eq!(contents.models[0].3, ArchiveModelKind::Stl);
        let bundle = extract_model_bundle(&buf, ArchiveFormat::SevenZ, contents, 0, None).unwrap();
        assert_eq!(bundle.model.data, b"STL secret data");
    }

    #[test]
    fn test_rar_format_from_ext() {
        assert_eq!(archive_format_from_ext("rar"), Some(ArchiveFormat::Rar));
        assert_eq!(archive_format_from_ext("zip"), Some(ArchiveFormat::Zip));
        assert_eq!(archive_format_from_ext("7z"), Some(ArchiveFormat::SevenZ));
        assert_eq!(archive_format_from_ext("tar"), None);
    }

    #[test]
    fn test_zip_bomb_protection() {
        // Test the case where the declared size exceeds the cap
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            writer.start_file("huge.pmx", options).unwrap();
            std::io::Write::write_all(&mut writer, b"small data").unwrap();
            writer.finish().unwrap();
        }
        // Test with a tiny cap
        let result = zip_extract::extract_files(&buf, &[Path::new("huge.pmx")], 1, None);
        // 10 bytes of data against a 1-byte cap -> size-overflow error
        assert!(result.is_err());
    }

    #[test]
    fn test_subdirectory_pmx_aux_keys() {
        // Verify that the aux_files keys for a PMX in a subdirectory use the correct relative path
        let entries = vec![
            ArchiveEntry {
                path: PathBuf::from("model/sub/test.pmx"),
                data: Vec::new(),
            },
            ArchiveEntry {
                path: PathBuf::from("model/sub/tex/body.png"),
                data: vec![1, 2, 3],
            },
            ArchiveEntry {
                path: PathBuf::from("model/sub/readme.txt"),
                data: vec![4, 5],
            },
            ArchiveEntry {
                path: PathBuf::from("other/unrelated.png"),
                data: vec![6, 7],
            },
        ];
        let model_dir = Path::new("model/sub");
        let tex_refs = vec!["tex/body.png".to_string()];
        let aux = build_aux_from_entries_pmx(entries, &tex_refs, model_dir);

        // tex/body.png is keyed relative to the model directory
        assert!(aux.contains_key(Path::new("tex/body.png")));
        // readme.txt is collected too
        assert!(aux.contains_key(Path::new("readme.txt")));
        // Files in other directories are excluded
        assert!(!aux.contains_key(Path::new("unrelated.png")));
    }
}
