//! Utility types and functions (ReloadableSource, TextureSource, is_temp_path, etc.).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Source a model is loaded from.
#[derive(Clone)]
pub enum ReloadableSource {
    /// Regular file (re-read from disk on reload).
    File(PathBuf),
    /// Snapshot from a temporary file (reload from memory).
    Snapshot {
        original_path: PathBuf,
        main_bytes: Arc<[u8]>,
        /// PMX/PMD: relative path -> bytes (textures, .txt, etc.).
        aux_files: HashMap<PathBuf, Arc<[u8]>>,
    },
    /// Model inside an archive (ZIP/7z).
    Archive {
        original_path: PathBuf,
        /// Snapshot for D&D temp files.
        archive_bytes: Option<Arc<[u8]>>,
        /// Internal path of the selected model.
        selected_entry_path: String,
        /// Model kind.
        inner_kind: crate::archive::ArchiveModelKind,
    },
}

impl ReloadableSource {
    /// Return the path used for display.
    pub fn display_path(&self) -> &Path {
        match self {
            ReloadableSource::File(p) => p,
            ReloadableSource::Snapshot { original_path, .. } => original_path,
            ReloadableSource::Archive { original_path, .. } => original_path,
        }
    }

    /// Return the extension in lowercase.
    pub fn extension_lower(&self) -> String {
        crate::path_ext_lower(self.display_path())
    }

    /// Whether the source is cached.
    pub fn is_snapshot(&self) -> bool {
        matches!(self, ReloadableSource::Snapshot { .. })
    }
}

/// Pre-read data for D&D temp files (cache the bytes before the file disappears).
pub struct PreloadedData {
    pub path: PathBuf,
    pub main_bytes: Arc<[u8]>,
    pub aux_files: HashMap<PathBuf, Arc<[u8]>>,
}

/// Source of a texture (a file path, or cached bytes).
#[derive(Clone)]
pub enum TextureSource {
    File(PathBuf),
    Cached {
        original_name: String,
        data: Arc<[u8]>,
        is_psd: bool,
    },
}

impl TextureSource {
    /// Return the name used for display.
    pub fn display_name(&self) -> String {
        match self {
            TextureSource::File(p) => p.to_string_lossy().into_owned(),
            TextureSource::Cached { original_name, .. } => original_name.clone(),
        }
    }
}

/// Cached temp directory info (canonicalized path + lowercased string).
fn cached_temp_dir() -> &'static (Option<PathBuf>, String) {
    use std::sync::OnceLock;
    static CACHE: OnceLock<(Option<PathBuf>, String)> = OnceLock::new();
    CACHE.get_or_init(|| {
        let raw = std::env::temp_dir();
        let canonical = raw.canonicalize().ok();
        let mut lower = raw.to_string_lossy().to_lowercase();
        if !lower.ends_with(std::path::MAIN_SEPARATOR) {
            lower.push(std::path::MAIN_SEPARATOR);
        }
        (canonical, lower)
    })
}

/// Detect whether the given path is under the temp directory.
pub fn is_temp_path(path: &Path) -> bool {
    let (canonical_temp, lower_temp) = cached_temp_dir();
    // Canonicalize-based check (when the file still exists).
    if let Some(ref temp) = canonical_temp {
        if let Ok(target) = path.canonicalize() {
            if target.starts_with(temp) {
                return true;
            }
        }
    }
    // Fallback: string-based check (still works after the file disappears).
    let path_str = path.to_string_lossy().to_lowercase();
    path_str.starts_with(lower_temp.as_str())
}

/// For FBX external textures: recursively collect image files under the given directory.
/// Keys are paths relative to base_dir (subdirectory structure is preserved).
pub fn collect_image_files_recursive(
    base_dir: &Path,
    current_dir: &Path,
    out: &mut HashMap<PathBuf, Arc<[u8]>>,
) {
    let Ok(entries) = std::fs::read_dir(current_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let ep = entry.path();
        if ep.is_dir() {
            collect_image_files_recursive(base_dir, &ep, out);
        } else {
            let ext_low = crate::path_ext_lower(&ep);
            if matches!(
                ext_low.as_str(),
                "png" | "jpg" | "jpeg" | "tga" | "bmp" | "tif" | "tiff" | "dds" | "mtl"
            ) {
                if let Ok(img_data) = std::fs::read(&ep) {
                    if let Ok(rel) = ep.strip_prefix(base_dir) {
                        out.insert(rel.to_path_buf(), Arc::from(img_data.into_boxed_slice()));
                    }
                }
            }
        }
    }
}

/// Image extensions accepted by D&D.
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "tga", "bmp", "psd", "dds"];

/// Model / animation extensions accepted by D&D.
pub const MODEL_EXTENSIONS: &[&str] = &[
    "vrm",
    "fbx",
    "pmx",
    "pmd",
    "obj",
    "stl",
    "x",
    "unitypackage",
    "vrma",
    "glb",
    "gltf",
    "anim",
    "zip",
    "7z",
    "rar",
];

// PkgModelType moved to unitypackage.rs (it is used by the CLI as well).
pub use crate::unitypackage::PkgModelType;

/// Build the Prefab/VRM/FBX model list from a unitypackage asset set.
pub fn build_pkg_model_list(
    assets: &[crate::unitypackage::ExtractedAsset],
) -> Vec<(usize, String, PkgModelType)> {
    let mut list = Vec::new();
    // Add Prefabs first (loading via Prefab is preferred).
    for (idx, asset) in assets.iter().enumerate() {
        if asset.pathname.to_lowercase().ends_with(".prefab") {
            list.push((idx, asset.filename(), PkgModelType::Prefab));
        }
    }
    // VRM
    for (idx, name) in crate::unitypackage::find_vrm_list(assets) {
        list.push((idx, name, PkgModelType::Vrm));
    }
    // FBX
    for (idx, name) in crate::unitypackage::find_fbx_list(assets) {
        list.push((idx, name, PkgModelType::Fbx));
    }
    list
}

/// FBX load mode (model only / animation only / both).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FbxLoadMode {
    ModelOnly,
    AnimationOnly,
    Both,
}

/// Open a directory in the OS file manager.
pub fn open_directory(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer").arg(path).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(path).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}
