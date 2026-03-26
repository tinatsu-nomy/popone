//! ユーティリティ型・関数（ReloadableSource, TextureSource, is_temp_path 等）

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// モデルの読み込み元を表す
#[derive(Clone)]
pub enum ReloadableSource {
    /// 通常のファイル（リロード時は再読み込み）
    File(PathBuf),
    /// 一時ファイルからのスナップショット（リロード時はメモリから）
    Snapshot {
        original_path: PathBuf,
        main_bytes: Arc<[u8]>,
        /// PMX/PMD 用: 相対パス → バイト列（テクスチャ・.txt等）
        aux_files: HashMap<PathBuf, Arc<[u8]>>,
    },
    /// アーカイブ（ZIP/7z）内のモデル
    Archive {
        original_path: PathBuf,
        /// D&D一時ファイル用スナップショット
        archive_bytes: Option<Arc<[u8]>>,
        /// 選択されたモデルの内部パス
        selected_entry_path: String,
        /// モデル種別
        inner_kind: crate::archive::ArchiveModelKind,
    },
}

impl ReloadableSource {
    /// 表示用パスを返す
    pub fn display_path(&self) -> &Path {
        match self {
            ReloadableSource::File(p) => p,
            ReloadableSource::Snapshot { original_path, .. } => original_path,
            ReloadableSource::Archive { original_path, .. } => original_path,
        }
    }

    /// 拡張子を小文字で返す
    pub fn extension_lower(&self) -> String {
        self.display_path()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase()
    }

    /// キャッシュ済みかどうか
    pub fn is_snapshot(&self) -> bool {
        matches!(self, ReloadableSource::Snapshot { .. })
    }
}

/// D&D temp ファイルの先読みデータ（ファイル消失前にバイト列をキャッシュ）
pub struct PreloadedData {
    pub path: PathBuf,
    pub main_bytes: Arc<[u8]>,
    pub aux_files: HashMap<PathBuf, Arc<[u8]>>,
}

/// テクスチャの読み込み元（ファイルまたはキャッシュ済みバイト列）
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
    /// 表示用名前を返す
    pub fn display_name(&self) -> String {
        match self {
            TextureSource::File(p) => p.to_string_lossy().into_owned(),
            TextureSource::Cached { original_name, .. } => original_name.clone(),
        }
    }
}

/// 一時ディレクトリ配下かどうかを検出する
pub fn is_temp_path(path: &Path) -> bool {
    // canonicalize ベース（ファイル存在時）
    if let (Ok(temp), Ok(target)) = (std::env::temp_dir().canonicalize(), path.canonicalize()) {
        if target.starts_with(&temp) {
            return true;
        }
    }
    // フォールバック: 文字列ベース（ファイル消失後でも機能）
    let path_str = path.to_string_lossy().to_lowercase();
    let mut temp_str = std::env::temp_dir().to_string_lossy().to_lowercase();
    // パス境界を保証: TempBackup 等の誤検出を防止
    if !temp_str.ends_with(std::path::MAIN_SEPARATOR) {
        temp_str.push(std::path::MAIN_SEPARATOR);
    }
    path_str.starts_with(&*temp_str)
}

/// FBX 外部テクスチャ用: 指定ディレクトリ以下の画像ファイルを再帰的に収集する
/// キーは base_dir からの相対パス（サブディレクトリ構造を保持）
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
        } else if let Some(ext) = ep.extension().and_then(|e| e.to_str()) {
            let ext_low = ext.to_lowercase();
            if matches!(
                ext_low.as_str(),
                "png" | "jpg" | "jpeg" | "tga" | "bmp" | "tif" | "tiff" | "dds"
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

/// D&D 対応画像拡張子
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "tga", "bmp", "psd"];

/// D&D 対応モデル/アニメーション拡張子
pub const MODEL_EXTENSIONS: &[&str] = &[
    "vrm",
    "fbx",
    "pmx",
    "pmd",
    "unitypackage",
    "vrma",
    "glb",
    "gltf",
    "anim",
    "zip",
    "7z",
];

/// unitypackage 内のモデルファイル種別
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PkgModelType {
    Fbx,
    Vrm,
}

/// unitypackage アセット群から VRM/FBX モデルリストを構築
pub fn build_pkg_model_list(
    assets: &[crate::unitypackage::ExtractedAsset],
) -> Vec<(usize, String, PkgModelType)> {
    let mut list = Vec::new();
    for (idx, name) in crate::unitypackage::find_vrm_list(assets) {
        list.push((idx, name, PkgModelType::Vrm));
    }
    for (idx, name) in crate::unitypackage::find_fbx_list(assets) {
        list.push((idx, name, PkgModelType::Fbx));
    }
    list
}

/// FBX 読み込みモード（モデル/アニメーション/両方）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FbxLoadMode {
    ModelOnly,
    AnimationOnly,
    Both,
}

/// ディレクトリをOSのファイルマネージャで開く
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
