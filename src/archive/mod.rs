//! アーカイブ（ZIP / 7z）直接ロード機能
//!
//! アーカイブ内のモデルファイル（VRM/FBX/PMX/PMD）を検出し、
//! 関連テクスチャと共に展開する統一インターフェース。

pub mod sevenz;
pub mod zip_extract;

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Result};

/// アーカイブ対応モデル拡張子（unitypackage も二重展開で対応）
pub const MODEL_EXTENSIONS: &[&str] = &["vrm", "glb", "fbx", "pmx", "pmd", "unitypackage"];

/// テクスチャ拡張子（psd は数百MB級で OOM リスクのため除外）
pub const TEXTURE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "tga", "bmp", "tif", "tiff"];

/// 展開サイズ上限: 2GB
const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// アーカイブ内のエントリメタデータ（一覧取得用、データなし）
pub struct ArchiveEntryMeta {
    pub path: PathBuf,
    pub size: u64,
}

/// アーカイブ内の展開済みエントリ（データあり）
pub struct ArchiveEntry {
    pub path: PathBuf,
    pub data: Vec<u8>,
}

/// アーカイブ形式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    SevenZ,
}

/// アーカイブ内モデルの種別
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveModelKind {
    Vrm,
    Glb,
    Fbx,
    Pmx,
    Pmd,
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
            "unitypackage" => Some(Self::UnityPackage),
            _ => None,
        }
    }

    /// UI 表示用ラベル
    pub fn label(&self) -> &'static str {
        match self {
            Self::Vrm => "VRM",
            Self::Glb => "GLB",
            Self::Fbx => "FBX",
            Self::Pmx => "PMX",
            Self::Pmd => "PMD",
            Self::UnityPackage => "UnityPackage",
        }
    }
}

/// モデル展開結果
pub struct ModelBundle {
    pub model: ArchiveEntry,
    pub kind: ArchiveModelKind,
    /// FBX/VRM 用: テクスチャファイル群 (ファイル名, データ)
    pub textures: Vec<(String, Vec<u8>)>,
    /// PMX/PMD 用: 相対パス→バイト列の補助ファイル群
    pub aux_files: HashMap<PathBuf, Arc<[u8]>>,
}

/// アーカイブ内モデル一覧の結果
pub struct ArchiveContents {
    /// (index, 正規化済み内部パス, 表示用ファイル名, kind)
    pub models: Vec<(usize, PathBuf, String, ArchiveModelKind)>,
    /// 7z の場合は全展開済みエントリを保持
    entries: Option<Vec<ArchiveEntry>>,
    /// ZIP の場合はメタデータのみ
    metas: Option<Vec<ArchiveEntryMeta>>,
}

/// パス正規化（.. や絶対パス要素を拒否）
pub fn normalize_archive_path(raw: &str) -> Result<PathBuf> {
    let cleaned = raw.replace('\\', "/");
    let mut out = PathBuf::new();
    for c in Path::new(&cleaned).components() {
        match c {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => bail!("安全でないアーカイブパス: {raw}"),
        }
    }
    Ok(out)
}

/// 拡張子からアーカイブ形式を判定
pub fn archive_format_from_ext(ext: &str) -> Option<ArchiveFormat> {
    match ext {
        "zip" => Some(ArchiveFormat::Zip),
        "7z" => Some(ArchiveFormat::SevenZ),
        _ => None,
    }
}

/// アーカイブ内モデル一覧を取得
///
/// **注意**: 7z 形式はストリーミング展開の制約上、対象拡張子のファイルを全展開して
/// メモリに保持します（`MAX_TOTAL_BYTES` 上限あり）。ZIP はメタデータのみ取得。
/// 7z の展開済みエントリは `ArchiveContents` 内に保持され、後続の
/// `extract_model_bundle` で再展開なく利用されます。
pub fn list_models(data: &[u8], format: ArchiveFormat) -> Result<ArchiveContents> {
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
            let entries = sevenz::extract_filtered(data, MAX_TOTAL_BYTES)?;
            let models = find_models_from_entries(&entries);
            Ok(ArchiveContents {
                models,
                entries: Some(entries),
                metas: None,
            })
        }
    }
}

/// メタデータ一覧からモデルを検出
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

/// エントリ一覧からモデルを検出
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
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())?;
    ArchiveModelKind::from_ext(&ext)
}

/// 選択モデル + 関連ファイルを展開
pub fn extract_model_bundle(
    data: &[u8],
    format: ArchiveFormat,
    contents: ArchiveContents,
    model_index: usize,
) -> Result<ModelBundle> {
    let (_, model_path, _, kind) = contents
        .models
        .get(model_index)
        .ok_or_else(|| anyhow::anyhow!("モデルインデックスが範囲外: {model_index}"))?;
    let model_path = model_path.clone();
    let kind = *kind;

    match format {
        ArchiveFormat::Zip => {
            extract_bundle_from_zip(data, &model_path, kind, contents.metas.as_deref())
        }
        ArchiveFormat::SevenZ => {
            extract_bundle_from_entries(contents.entries.unwrap_or_default(), &model_path, kind)
        }
    }
}

/// ZIP から選択モデル + 関連ファイルを展開
fn extract_bundle_from_zip(
    data: &[u8],
    model_path: &Path,
    kind: ArchiveModelKind,
    metas: Option<&[ArchiveEntryMeta]>,
) -> Result<ModelBundle> {
    match kind {
        ArchiveModelKind::Pmx | ArchiveModelKind::Pmd => {
            // PMX/PMD: まずモデルを展開してテクスチャ参照パスを取得し、必要なファイルのみ追加展開
            let model_entries = zip_extract::extract_files(data, &[model_path], MAX_TOTAL_BYTES)?;
            let model_entry = model_entries
                .into_iter()
                .find(|e| e.path == model_path)
                .ok_or_else(|| {
                    anyhow::anyhow!("モデルファイルが展開できません: {}", model_path.display())
                })?;

            // テクスチャ参照パスを取得
            let tex_refs = get_texture_refs_from_model(&model_entry.data, kind)?;
            let model_dir = model_path.parent().unwrap_or(Path::new(""));

            // 必要なファイルを特定
            let needed: Vec<PathBuf> = if let Some(metas) = metas {
                collect_needed_paths(metas.iter().map(|m| &m.path), &tex_refs, model_dir)
            } else {
                Vec::new()
            };

            let aux_files = if !needed.is_empty() {
                let needed_refs: Vec<&Path> = needed.iter().map(|p| p.as_path()).collect();
                let remaining = MAX_TOTAL_BYTES.saturating_sub(model_entry.data.len() as u64);
                let aux_entries = zip_extract::extract_files(data, &needed_refs, remaining)?;
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
        ArchiveModelKind::UnityPackage => {
            // UnityPackage: 本体のみ展開（テクスチャはパッケージ内に含まれるため不要）
            let model_entries = zip_extract::extract_files(data, &[model_path], MAX_TOTAL_BYTES)?;
            let model_entry = model_entries
                .into_iter()
                .find(|e| e.path == model_path)
                .ok_or_else(|| {
                    anyhow::anyhow!("モデルファイルが展開できません: {}", model_path.display())
                })?;
            Ok(ModelBundle {
                model: model_entry,
                kind,
                textures: Vec::new(),
                aux_files: HashMap::new(),
            })
        }
        _ => {
            // VRM/GLB/FBX: モデル + 同ディレクトリ以下のテクスチャを展開
            let model_dir = model_path.parent().unwrap_or(Path::new(""));
            let mut paths_to_extract = vec![model_path.to_path_buf()];

            if let Some(metas) = metas {
                for meta in metas {
                    if is_texture_in_scope(&meta.path, model_dir) {
                        paths_to_extract.push(meta.path.clone());
                    }
                }
            }

            let path_refs: Vec<&Path> = paths_to_extract.iter().map(|p| p.as_path()).collect();
            let entries = zip_extract::extract_files(data, &path_refs, MAX_TOTAL_BYTES)?;

            let mut model_entry = None;
            let mut textures = Vec::new();
            for entry in entries {
                if entry.path == model_path {
                    model_entry = Some(entry);
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
                model: model_entry
                    .ok_or_else(|| anyhow::anyhow!("モデルファイルが展開できません"))?,
                kind,
                textures,
                aux_files: HashMap::new(),
            })
        }
    }
}

/// 7z 展開済みエントリからバンドルを構築
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
        anyhow::anyhow!("モデルファイルが見つかりません: {}", model_path.display())
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
        ArchiveModelKind::UnityPackage => {
            // UnityPackage: 本体のみ（テクスチャはパッケージ内に含まれるため不要）
            Ok(ModelBundle {
                model: model_entry,
                kind,
                textures: Vec::new(),
                aux_files: HashMap::new(),
            })
        }
        _ => {
            let textures: Vec<(String, Vec<u8>)> = other_entries
                .into_iter()
                .filter(|e| is_texture_in_scope(&e.path, model_dir))
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

/// PMX/PMD モデルデータからテクスチャ参照パス一覧を取得
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
                    // PMD テクスチャ名は "*" でスフィア区切り
                    for part in mat.texture_name.split('*') {
                        let trimmed = part.trim();
                        if !trimmed.is_empty() {
                            refs.push(trimmed.to_string());
                        }
                    }
                }
            }
            // toon テクスチャ
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

/// テクスチャ参照パスにマッチするアーカイブ内パスを収集
fn collect_needed_paths<'a>(
    archive_paths: impl Iterator<Item = &'a PathBuf>,
    tex_refs: &[String],
    model_dir: &Path,
) -> Vec<PathBuf> {
    let mut needed = Vec::new();
    let archive_paths: Vec<&PathBuf> = archive_paths.collect();

    for tex_ref in tex_refs {
        let ref_path = PathBuf::from(tex_ref.replace('\\', "/"));
        // モデル親ディレクトリ基準で解決
        let resolved = normalize_relative_path(&model_dir.join(&ref_path));

        for &ap in &archive_paths {
            // 完全一致
            if *ap == resolved {
                needed.push(ap.clone());
                continue;
            }
            // Case-insensitive フォールバック
            if path_eq_ignore_case(ap, &resolved) {
                needed.push(ap.clone());
                continue;
            }
            // PMD: basename のみで照合
            if ref_path.components().count() == 1 {
                if let (Some(a_name), Some(r_name)) = (ap.file_name(), ref_path.file_name()) {
                    if a_name.to_string_lossy().to_lowercase()
                        == r_name.to_string_lossy().to_lowercase()
                    {
                        // 同ディレクトリまたはサブディレクトリ内
                        if ap.starts_with(model_dir) || model_dir == Path::new("") {
                            needed.push(ap.clone());
                        }
                    }
                }
            }
        }
    }
    // .txt ファイルも収集（PMD/PMX の readme 等）
    for &ap in &archive_paths {
        let ext = ap
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if ext == "txt" && (ap.starts_with(model_dir) || model_dir == Path::new("")) {
            needed.push(ap.clone());
        }
    }
    needed.sort();
    needed.dedup();
    needed
}

/// aux_files を構築（モデル親ディレクトリ基準の相対パスをキーに）
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

/// 7z 展開済みエントリから PMX/PMD 用 aux_files を構築
fn build_aux_from_entries_pmx(
    entries: Vec<ArchiveEntry>,
    tex_refs: &[String],
    model_dir: &Path,
) -> HashMap<PathBuf, Arc<[u8]>> {
    let mut aux = HashMap::new();

    for entry in entries {
        let ext = entry
            .path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let is_needed = if ext == "txt" {
            entry.path.starts_with(model_dir) || model_dir == Path::new("")
        } else {
            // テクスチャ参照パスにマッチするか確認
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

/// テクスチャがモデルと同ディレクトリ + サブディレクトリ内かどうか
fn is_texture_in_scope(path: &Path, model_dir: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    if !TEXTURE_EXTENSIONS.contains(&ext.as_str()) {
        return false;
    }
    model_dir == Path::new("") || path.starts_with(model_dir)
}

/// 相対パスを正規化（`foo/../bar` → `bar`）
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

/// Case-insensitive パス比較
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
        // ZIP内に .unitypackage と無関係な画像がある場合、
        // UnityPackage 抽出時にテクスチャを巻き込まないことを確認
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

        let contents = list_models(&buf, ArchiveFormat::Zip).unwrap();
        assert_eq!(contents.models.len(), 1);
        assert_eq!(contents.models[0].3, ArchiveModelKind::UnityPackage);

        let bundle = extract_model_bundle(&buf, ArchiveFormat::Zip, contents, 0).unwrap();
        // モデル本体のみ展開、テクスチャは空
        assert_eq!(bundle.model.data, b"fake unitypackage data");
        assert!(
            bundle.textures.is_empty(),
            "UnityPackage 抽出時にテクスチャを巻き込んではならない"
        );
        assert!(bundle.aux_files.is_empty());
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
        // プログラム的にZIPを作成してテスト
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            writer.start_file("test/model.pmx", options).unwrap();
            // PMX マジック + 最小ヘッダ（パースは失敗するが list_entries は動く）
            std::io::Write::write_all(&mut writer, b"PMX test data").unwrap();
            writer.start_file("test/texture.png", options).unwrap();
            std::io::Write::write_all(&mut writer, b"PNG fake data").unwrap();
            writer.start_file("other/readme.txt", options).unwrap();
            std::io::Write::write_all(&mut writer, b"readme").unwrap();
            writer.finish().unwrap();
        }

        let contents = list_models(&buf, ArchiveFormat::Zip).unwrap();
        assert_eq!(contents.models.len(), 1);
        assert_eq!(contents.models[0].2, "model.pmx");
        assert_eq!(contents.models[0].3, ArchiveModelKind::Pmx);
    }

    #[test]
    fn test_broken_archive_error() {
        // 壊れたデータではエラーが返る
        let result = list_models(b"this is not a zip file", ArchiveFormat::Zip);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_archive() {
        let mut buf = Vec::new();
        {
            let writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            writer.finish().unwrap();
        }
        let contents = list_models(&buf, ArchiveFormat::Zip).unwrap();
        assert!(contents.models.is_empty());
    }

    #[test]
    fn test_zip_bomb_protection() {
        // declared size が上限を超える場合のテスト
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            writer.start_file("huge.pmx", options).unwrap();
            std::io::Write::write_all(&mut writer, b"small data").unwrap();
            writer.finish().unwrap();
        }
        // 極小上限でテスト
        let result = zip_extract::extract_files(&buf, &[Path::new("huge.pmx")], 1);
        // 10バイトのデータに対して上限1バイト → サイズ超過エラー
        assert!(result.is_err());
    }

    #[test]
    fn test_subdirectory_pmx_aux_keys() {
        // サブディレクトリ内PMXの aux_files キーが正しい相対パスか確認
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

        // tex/body.png はモデルディレクトリ基準の相対パスでキーに
        assert!(aux.contains_key(Path::new("tex/body.png")));
        // readme.txt も収集される
        assert!(aux.contains_key(Path::new("readme.txt")));
        // 他ディレクトリのファイルは除外
        assert!(!aux.contains_key(Path::new("unrelated.png")));
    }
}
