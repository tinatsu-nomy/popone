//! .unitypackage (tar.gz) からアセットを抽出するモジュール

use crate::error::{PoponeError, Result, ResultExt};
use crate::intermediate::types::{SourceMaterialRef, TextureData};
use flate2::read::GzDecoder;
use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};
use std::sync::Arc;

// ── Prefab テクスチャマッピング用エラー型 ──

/// unitypackage 処理の専用エラー型
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

/// unitypackage 内部処理用の Result 型
pub type PkgResult<T> = std::result::Result<T, PkgError>;

// ── Prefab テクスチャマッピング用型定義 ──

/// unitypackage 内のモデルファイル種別
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PkgModelType {
    Fbx,
    Vrm,
    Prefab,
}

/// モデル選択の唯一キー（GUID + pathname で一意識別）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PkgModelLocator {
    pub guid: std::sync::Arc<str>,
    pub pathname: std::sync::Arc<str>,
    pub kind: PkgModelType,
}

/// モデル一覧の表示用アイテム
pub struct PkgModelListItem {
    pub locator: PkgModelLocator,
    pub label: std::sync::Arc<str>,
}

/// append 複数回を区別する scene 内インスタンス ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PkgInstanceId(pub u32);

/// ベースモデル用の固定インスタンス ID
pub const BASE_INSTANCE_ID: PkgInstanceId = PkgInstanceId(0);

/// reload/append 復元の安定キー
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PkgMaterialKey {
    pub instance_id: PkgInstanceId,
    pub model_guid: std::sync::Arc<str>,
    pub source_material: Option<crate::intermediate::types::SourceMaterialRef>,
    pub material_name: std::sync::Arc<str>,
}

// ── 既存コード ──

/// FBXデータ、ファイル名、テクスチャ一覧のタプル型
pub type FbxWithTextures = (Vec<u8>, String, Vec<(String, Vec<u8>)>);

/// 展開されたアセット情報
pub struct ExtractedAsset {
    /// Unity プロジェクト内パス（例: "Assets/Models/xxx.fbx"）
    pub pathname: String,
    /// アセット本体データ（Arc で AssetEntry と共有し二重コピーを回避）
    pub data: Arc<[u8]>,
}

impl ExtractedAsset {
    /// パスからファイル名部分を取得
    pub fn filename(&self) -> String {
        std::path::Path::new(&self.pathname)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    }
}

/// unitypackage 内のアセットエントリ（GUID・パス・データ・メタ情報を保持）
pub struct AssetEntry {
    pub guid: String,
    pub pathname: String,
    pub data: Arc<[u8]>,
    pub meta: Option<String>,
}

/// unitypackage のインデックス（全アセット + GUID/パスの逆引きマップ）
pub struct UnityPackageIndex {
    pub entries: Vec<AssetEntry>,
    pub by_guid: HashMap<String, usize>,
    pub by_path: HashMap<String, usize>,
}

/// 展開サイズ上限: 2GB（archive モジュールと同じ）
const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// .unitypackage からすべてのアセットを展開
/// 内部で `build_unity_package_index()` を呼び、`Vec<ExtractedAsset>` に変換して返す
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

/// .unitypackage からインデックスを構築（全アセット + GUID/パスの逆引きマップ）
pub fn build_unity_package_index(archive_data: &[u8]) -> Result<UnityPackageIndex> {
    build_unity_package_index_with_limit(archive_data, MAX_TOTAL_BYTES)
}

/// .unitypackage からインデックスを構築（サイズ上限指定）
fn build_unity_package_index_with_limit(
    archive_data: &[u8],
    max_bytes: u64,
) -> Result<UnityPackageIndex> {
    let decoder = GzDecoder::new(Cursor::new(archive_data));
    let mut archive = tar::Archive::new(decoder);

    // GUID → (pathname, asset_data, meta) を収集
    let mut pathnames: HashMap<String, String> = HashMap::new();
    let mut assets: HashMap<String, Vec<u8>> = HashMap::new();
    let mut metas: HashMap<String, String> = HashMap::new();
    let mut total_bytes: u64 = 0;

    for entry in archive.entries().context("tarエントリ読み込み失敗")? {
        let mut entry = entry.context("tarエントリ解析失敗")?;
        let path = entry
            .path()
            .context("パス取得失敗")?
            .to_string_lossy()
            .to_string();
        // パスは "GUID/filename" 形式
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
                    .context("pathname読み込み失敗")?;
                // B-9: pathname の読み込みバイト数も加算
                total_bytes += s.len() as u64;
                if total_bytes > max_bytes {
                    return Err(PoponeError::UnityPackage(format!(
                        ".unitypackage 展開サイズが上限 ({}MB) を超えました",
                        max_bytes / (1024 * 1024)
                    )));
                }
                pathnames.insert(guid, s.trim().to_string());
            }
            "asset" => {
                let entry_size = entry.header().size().unwrap_or(0);
                if total_bytes.saturating_add(entry_size) > max_bytes {
                    return Err(PoponeError::UnityPackage(format!(
                        ".unitypackage 展開サイズが上限 ({}MB) を超えました",
                        max_bytes / (1024 * 1024)
                    )));
                }
                let mut data = Vec::new();
                entry.read_to_end(&mut data).context("asset読み込み失敗")?;
                total_bytes += data.len() as u64;
                if total_bytes > max_bytes {
                    return Err(PoponeError::UnityPackage(format!(
                        ".unitypackage 展開サイズが上限 ({}MB) を超えました",
                        max_bytes / (1024 * 1024)
                    )));
                }
                assets.insert(guid, data);
            }
            "asset.meta" => {
                let mut data = Vec::new();
                entry
                    .read_to_end(&mut data)
                    .context("asset.meta読み込み失敗")?;
                // B-9: asset.meta の読み込みバイト数も加算
                total_bytes += data.len() as u64;
                if total_bytes > max_bytes {
                    return Err(PoponeError::UnityPackage(format!(
                        ".unitypackage 展開サイズが上限 ({}MB) を超えました",
                        max_bytes / (1024 * 1024)
                    )));
                }
                let cow = String::from_utf8_lossy(&data);
                if matches!(&cow, std::borrow::Cow::Owned(_)) {
                    log::warn!("asset.meta (GUID={}) contains invalid UTF-8", guid);
                }
                metas.insert(guid, cow.into_owned());
            }
            _ => {} // preview.png 等は無視
        }
    }

    // pathname と asset を結合して AssetEntry を構築
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

    Ok(UnityPackageIndex {
        entries,
        by_guid,
        by_path,
    })
}

/// 展開済みアセットからFBX一覧を取得
/// 戻り値: [(アセットインデックス, ファイル名)]
pub fn find_fbx_list(assets: &[ExtractedAsset]) -> Vec<(usize, String)> {
    assets
        .iter()
        .enumerate()
        .filter(|(_, a)| a.pathname.to_lowercase().ends_with(".fbx"))
        .map(|(i, a)| (i, a.filename()))
        .collect()
}

/// 展開済みアセットから指定FBXとテクスチャを取り出す
/// assets は消費される（所有権移動）
pub fn take_fbx_and_textures(
    mut assets: Vec<ExtractedAsset>,
    fbx_index: usize,
) -> Result<FbxWithTextures> {
    if fbx_index >= assets.len() {
        return Err(PoponeError::UnityPackage(format!(
            "FBXインデックスが範囲外: {fbx_index}"
        )));
    }

    // FBX を取り出す
    let fbx_asset = assets.swap_remove(fbx_index);
    let fbx_name = fbx_asset.filename();
    let fbx_data = fbx_asset.data.to_vec();

    // テクスチャ（画像ファイル）を収集
    let texture_exts = ["png", "jpg", "jpeg", "tga", "bmp", "psd", "tif", "tiff"];
    let textures: Vec<(String, Vec<u8>)> = assets
        .into_iter()
        .filter(|a| {
            let lower = a.pathname.to_lowercase();
            texture_exts.iter().any(|ext| lower.ends_with(ext))
        })
        .map(|a| {
            let name = a.filename();
            (name, a.data.to_vec())
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

/// 展開済みアセットからVRM一覧を取得
/// 戻り値: [(アセットインデックス, ファイル名)]
pub fn find_vrm_list(assets: &[ExtractedAsset]) -> Vec<(usize, String)> {
    assets
        .iter()
        .enumerate()
        .filter(|(_, a)| a.pathname.to_lowercase().ends_with(".vrm"))
        .map(|(i, a)| (i, a.filename()))
        .collect()
}

/// 展開済みアセットから指定VRMを取り出す
/// assets は消費される（所有権移動）
pub fn take_vrm(mut assets: Vec<ExtractedAsset>, vrm_index: usize) -> Result<(Vec<u8>, String)> {
    if vrm_index >= assets.len() {
        return Err(PoponeError::UnityPackage(format!(
            "VRMインデックスが範囲外: {vrm_index}"
        )));
    }
    let vrm_asset = assets.swap_remove(vrm_index);
    let vrm_name = vrm_asset.filename();
    let vrm_data = vrm_asset.data.to_vec();
    log::info!(
        ".unitypackage extract: VRM={} ({}KB)",
        vrm_name,
        vrm_data.len() / 1024,
    );
    Ok((vrm_data, vrm_name))
}

/// .unitypackage から FBX を探して抽出する（CLI用）
/// fbx_name が指定されていればそのFBXを使用、なければ最初のFBXを使用
/// 戻り値: (FBXデータ, FBXファイル名, テクスチャ一覧 [(パス名, データ)])
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

    // 複数ある場合はログ出力
    if fbx_list.len() > 1 {
        log::info!("Found {} FBX files in .unitypackage:", fbx_list.len(),);
        for (_, name) in &fbx_list {
            log::info!("  FBX: {}", name);
        }
    }

    // FBX 名が指定されていればそれを使用
    let selected_idx = if let Some(target) = fbx_name {
        let target_lower = target.to_lowercase();
        fbx_list
            .iter()
            .find(|(_, name)| name.to_lowercase().contains(&target_lower))
            .map(|(idx, _)| *idx)
            .ok_or_else(|| {
                PoponeError::UnityPackage(format!(
                    "指定された FBX '{}' が見つかりません。利用可能: {}",
                    target,
                    fbx_list
                        .iter()
                        .map(|(_, n)| n.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })?
    } else {
        fbx_list[0].0
    };

    take_fbx_and_textures(assets, selected_idx)
}

/// unitypackage内テクスチャをIrModelの材質に自動割り当て
/// 材質の source_texture_name とテクスチャファイル名をマッチングする
/// 戻り値: 未割当材質のインデックス一覧
pub fn embed_textures_into_ir(
    ir: &mut crate::intermediate::types::IrModel,
    textures: &[(String, Vec<u8>)],
) -> Vec<usize> {
    embed_textures_into_ir_with_label(ir, textures, "package")
}

/// テクスチャ埋め込み（ソースラベル指定版）
pub fn embed_textures_into_ir_with_label(
    ir: &mut crate::intermediate::types::IrModel,
    textures: &[(String, Vec<u8>)],
    source_label: &str,
) -> Vec<usize> {
    if textures.is_empty() {
        return (0..ir.materials.len()).collect();
    }

    // ファイル名 → データのマップ（小文字キー）
    let tex_map: HashMap<String, &[u8]> = textures
        .iter()
        .map(|(name, data)| (name.to_lowercase(), data.as_slice()))
        .collect();

    // ステム（拡張子なし） → フルキーの逆引きマップ（ステム一致高速化）
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
        // 完全一致 → ステム一致のフォールバック
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
                let ext = std::path::Path::new(key.as_str())
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                let mime = crate::intermediate::types::mime_for_ext(&ext).to_string();
                ir.textures.push(crate::intermediate::types::IrTexture {
                    filename: key.clone(),
                    data: TextureData::Encoded(data.to_vec()),
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

    // 未割当材質のインデックスを収集
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

// ── Step 6: ヘルパー関数 + パーサー群 ──

/// 行から "guid: " の後の32文字hexを抽出
fn extract_guid_from_line(line: &str) -> Option<&str> {
    let idx = line.find("guid: ")?;
    let start = idx + 6; // "guid: ".len()
    let rest = line.get(start..)?;
    // 32文字hex部分を取得（カンマ等で区切られている）
    let end = rest
        .find(|c: char| !c.is_ascii_hexdigit())
        .unwrap_or(rest.len());
    if end >= 32 {
        Some(&rest[..32])
    } else {
        None
    }
}

/// "data[N]" から N を抽出
fn extract_array_index(line: &str) -> Option<usize> {
    let idx = line.find("data[")?;
    let start = idx + 5; // "data[".len()
    let rest = line.get(start..)?;
    let end = rest.find(']')?;
    rest[..end].parse().ok()
}

/// Unity YAML の `\uXXXX` エスケープをデコードする
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
                // デコード失敗時はそのまま
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

/// Prefab 形式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrefabFormat {
    New,
    Old,
}

/// Prefab 形式を判別
/// 行頭に独立した `PrefabInstance:` があれば新形式。
/// `m_PrefabInstance: {fileID: 0}` 等のフィールドとの誤マッチを防ぐため行単位で判定。
fn detect_prefab_format(content: &str) -> PrefabFormat {
    if content.lines().any(|line| line.trim() == "PrefabInstance:") {
        PrefabFormat::New
    } else {
        PrefabFormat::Old
    }
}

/// 新形式 Prefab の解析結果
struct NewPrefabInfo {
    source_fbx_guid: String,
    material_overrides: Vec<MaterialOverride>,
}

/// マテリアルオーバーライド（スロット番号 + マテリアル GUID）
struct MaterialOverride {
    slot_index: usize,
    material_guid: String,
}

/// 新形式 Prefab をパース
///
/// 新形式では `PrefabInstance:` ブロック内に `m_Modifications` (オーバーライド一覧) が先に来て、
/// `m_SourcePrefab:` が後に出現する。そのため2パス方式で処理する:
/// 1. オーバーライドを蓄積
/// 2. `m_SourcePrefab:` 出現時に蓄積分と紐付けて結果を生成
fn parse_prefab_new(content: &str) -> PkgResult<Vec<NewPrefabInfo>> {
    let mut results: Vec<NewPrefabInfo> = Vec::new();
    let mut current_overrides: Vec<MaterialOverride> = Vec::new();
    let mut pending_slot_index: Option<usize> = None;
    let mut in_prefab_instance = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // PrefabInstance: ブロック開始
        if trimmed == "PrefabInstance:" {
            // 新しい PrefabInstance ブロック開始時に蓄積をリセット
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

        // m_SourcePrefab: で GUID を取得し、蓄積したオーバーライドと紐付け
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

/// 旧形式 Prefab の解析結果
struct OldPrefabInfo {
    fbx_guid: String,
    material_guids: Vec<String>,
}

/// 旧形式 Prefab をパース
fn parse_prefab_old(content: &str) -> PkgResult<Vec<OldPrefabInfo>> {
    let mut results: Vec<OldPrefabInfo> = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // SkinnedMeshRenderer セクション検出
        if trimmed.starts_with("--- !u!137") {
            let mut mat_guids: Vec<String> = Vec::new();
            let mut mesh_guid: Option<String> = None;
            let mut in_materials = false;
            i += 1;

            while i < lines.len() {
                let line = lines[i];
                let lt = line.trim();

                // 次のオブジェクト境界
                if lt.starts_with("--- ") {
                    break;
                }

                // m_Materials: (複数行リスト) または m_Materials: [] (空インライン)
                if lt.starts_with("m_Materials:") {
                    // "m_Materials: []" のような空インラインはスキップ
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
                // m_Mesh があれば m_Materials が空でも FBX 参照として有効
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

/// FBX .meta 内のマテリアル情報
struct FbxMetaMaterial {
    material_name: String,
    material_guid: String,
}

/// FBX .meta をパースして (マテリアル一覧, materialImportMode) を返す
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

        // externalObjects セクション終了検出（インデントレベルが戻った場合）
        if in_external_objects
            && !line.starts_with(' ')
            && !line.starts_with('\t')
            && !trimmed.is_empty()
        {
            in_external_objects = false;
        }

        if in_external_objects {
            // name: で始まるマテリアル名（Unity YAML の \uXXXX エスケープをデコード、クォート除去）
            if trimmed.starts_with("name:") {
                let val = trimmed
                    .strip_prefix("name:")
                    .expect("starts_with チェック済み")
                    .trim()
                    .trim_matches('"');
                current_name = Some(decode_unity_escape(val));
                continue;
            }
            // second: で始まる GUID
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

    // materialImportMode の値はログ出力のみ（値に関わらず externalObjects を参照する）
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

/// .mat ファイル内のテクスチャスロット情報
struct MatTextureSlot {
    slot_name: String,
    texture_guid: String,
}

/// .mat ファイル内の Float パラメータ情報
struct MatFloatParam {
    param_name: String,
    value: f32,
}

/// .mat ファイル内の Color パラメータ情報
#[expect(dead_code)]
struct MatColorParam {
    param_name: String,
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

/// .mat ファイルの解析結果
struct ParsedMaterial {
    name: String,
    textures: Vec<MatTextureSlot>,
    floats: Vec<MatFloatParam>,
    colors: Vec<MatColorParam>,
    /// m_ShaderKeywords / m_ValidKeywords に含まれるキーワード
    shader_keywords: Vec<String>,
}

/// m_SavedProperties 内のセクション種別
enum MatSection {
    None,
    TexEnvs,
    Floats,
    Colors,
    Keywords,
}

/// .mat ファイルからマテリアル名・テクスチャスロット・Float パラメータを抽出
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

        // m_Name:（セクション外でも処理）
        if trimmed.starts_with("m_Name:") {
            name = trimmed
                .strip_prefix("m_Name:")
                .expect("starts_with チェック済み")
                .trim()
                .to_string();
            continue;
        }

        // m_ShaderKeywords / m_ValidKeywords: インライン "KEY1 KEY2 KEY3" または複数行リスト
        if trimmed.starts_with("m_ShaderKeywords:") || trimmed.starts_with("m_ValidKeywords:") {
            let val = trimmed.split_once(':').map(|(_, v)| v.trim()).unwrap_or("");
            if val.is_empty() || val == "[]" {
                // 値なし or 空配列 → 複数行リスト形式の可能性があるのでセクション切替
                if val != "[]" {
                    section = MatSection::Keywords;
                }
            } else {
                // インライン形式: "KEY1 KEY2 KEY3"
                let val = val.trim_matches('"').trim_matches('\'');
                for kw in val.split_whitespace() {
                    if !shader_keywords.contains(&kw.to_string()) {
                        shader_keywords.push(kw.to_string());
                    }
                }
            }
            continue;
        }

        // セクション切り替え
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
        // 空配列インライン
        if trimmed == "m_Floats: []" || trimmed == "m_TexEnvs: []" || trimmed == "m_Colors: []" {
            section = MatSection::None;
            continue;
        }
        // 他の m_ セクションヘッダーで終了
        if trimmed.starts_with("m_")
            && !trimmed.starts_with("m_Texture:")
            && !trimmed.starts_with("m_Scale:")
            && !trimmed.starts_with("m_Offset:")
        {
            section = MatSection::None;
        }

        match section {
            MatSection::TexEnvs => {
                // スロット名検出: "- _SlotName:" の形式
                if trimmed.starts_with("- _") {
                    // "- _MainTex:" → "_MainTex"
                    let Some(stripped) = trimmed.strip_prefix("- ") else {
                        continue;
                    };
                    let slot = stripped.trim_end_matches(':').to_string();
                    current_slot = Some(slot);
                    continue;
                }

                // m_Texture: 行
                if trimmed.starts_with("m_Texture:") {
                    if let Some(ref slot) = current_slot {
                        // fileID: 0 はスキップ（テクスチャ未設定）
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
                        // {r: R, g: G, b: B, a: A} をパース
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
                // 複数行リスト: "- _EMISSION" の形式
                if let Some(kw) = trimmed.strip_prefix("- ") {
                    let kw = kw.trim();
                    if !kw.is_empty() && !shader_keywords.contains(&kw.to_string()) {
                        shader_keywords.push(kw.to_string());
                    }
                } else if !trimmed.starts_with('-') {
                    // リスト項目以外が来たらセクション終了
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

/// Unity の色値 `{r: R, g: G, b: B, a: A}` をパース
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

/// Prefab 解決済みマテリアルテクスチャ情報
pub struct ResolvedMaterialTextures {
    pub source_material: Option<SourceMaterialRef>,
    pub material_name: Arc<str>,
    pub main_texture_guid: Option<Arc<str>>,
    /// ノーマルマップテクスチャ GUID（_BumpMap > _NormalMap の優先順）
    pub normal_texture_guid: Option<Arc<str>>,
    /// ノーマルマップスケール（_BumpScale、デフォルト 1.0）
    pub bump_scale: f32,
    /// FBX .meta の externalObjects に記載された FBX 内マテリアル名（IrModel の材質名と一致する）
    pub fbx_material_name: Option<Arc<str>>,
    /// Emission テクスチャ GUID（_EmissionMap）
    pub emission_texture_guid: Option<Arc<str>>,
    /// Emission 色 (r, g, b)（_EmissionColor、デフォルト黒 = 無効）
    pub emission_color: [f32; 3],
    /// Emission 有効フラグ（_Emission float == 1.0）
    pub emission_enabled: bool,
}

/// Prefab 候補（パスと解決済みマテリアル一覧）
struct PrefabCandidate {
    prefab_path: String,
    materials: Vec<ResolvedMaterialTextures>,
}

/// Prefab パスと FBX パスの類似度スコア
fn score_prefab_path(prefab_path: &str, fbx_path: &str) -> usize {
    // 共通プレフィックスの長さでスコアリング
    let prefab_parts: Vec<&str> = prefab_path.split('/').collect();
    let fbx_parts: Vec<&str> = fbx_path.split('/').collect();
    let mut score = 0;
    for (a, b) in prefab_parts.iter().zip(fbx_parts.iter()) {
        if a == b {
            score += a.len() + 1; // パス区切り分も加算
        } else {
            break;
        }
    }
    score
}

/// 複数候補から最適な Prefab を選択
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

/// Variant Prefab の再帰解決（source_prefab GUID → 元 FBX の GUID）
pub fn resolve_variant(pkg: &UnityPackageIndex, guid: &str) -> PkgResult<Option<String>> {
    let guids = resolve_variant_multi(pkg, guid)?;
    Ok(guids.into_iter().next())
}

/// Variant Prefab を再帰的に解決し、参照先の FBX GUID を全て返す。
/// 混合形式 Prefab（PrefabInstance + SkinnedMeshRenderer が共存）にも対応。
pub fn resolve_variant_multi(pkg: &UnityPackageIndex, guid: &str) -> PkgResult<Vec<String>> {
    let mut visited = HashSet::new();
    resolve_variant_multi_inner(pkg, guid, 0, &mut visited)
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

    // GUID から pathname を解決
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
        let data = &pkg.entries[entry_idx].data;
        let content = String::from_utf8_lossy(data);
        let format = detect_prefab_format(&content);
        log::debug!("resolve_variant: Prefab {} format={:?}", pathname, format);

        let mut results: Vec<String> = Vec::new();

        // New 形式パース（PrefabInstance ブロック）
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
                        if !results.contains(&g) {
                            results.push(g);
                        }
                    }
                }
            }
        }

        // Old 形式パース（SkinnedMeshRenderer セクション）
        // 混合形式（PrefabInstance + SkinnedMeshRenderer 共存）は New + Old 両方をパースする
        {
            if let Ok(infos) = parse_prefab_old(&content) {
                log::debug!("resolve_variant: Old Prefab infos={}", infos.len());
                for (i, info) in infos.iter().enumerate() {
                    log::debug!("resolve_variant:   [{}] fbx_guid={}", i, info.fbx_guid);
                }
                for info in &infos {
                    if !results.contains(&info.fbx_guid) {
                        results.push(info.fbx_guid.clone());
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

/// Prefab テクスチャ解決: FBX GUID に対応する Prefab を探してマテリアル→テクスチャマッピングを返す
pub fn resolve_prefab_textures(
    pkg: &UnityPackageIndex,
    fbx_guid: &str,
    fbx_path: &str,
) -> Vec<ResolvedMaterialTextures> {
    let mut candidates: Vec<PrefabCandidate> = Vec::new();

    // .prefab エントリを全件検索
    for entry in &pkg.entries {
        if !entry.pathname.to_lowercase().ends_with(".prefab") {
            continue;
        }

        let content = String::from_utf8_lossy(&entry.data);
        let format = detect_prefab_format(&content);

        log::debug!("Prefab inspection: {} format={:?}", entry.pathname, format);

        // New 形式（PrefabInstance: あり）のパース
        let mut new_matched = false;
        if format == PrefabFormat::New {
            let infos = match parse_prefab_new(&content) {
                Ok(v) => v,
                Err(e) => {
                    log::warn!("Prefab parse failed ({}): {}", entry.pathname, e);
                    Vec::new()
                }
            };

            log::debug!("  New Prefab infos: {}", infos.len());
            for (i, info) in infos.iter().enumerate() {
                log::debug!(
                    "    [{}] source_guid={}, overrides={}",
                    i,
                    info.source_fbx_guid,
                    info.material_overrides.len()
                );
            }

            for info in infos {
                // Variant 解決: source_fbx_guid が .prefab を指す場合は再帰（複数 FBX 対応）
                let resolved_guids = match resolve_variant_multi(pkg, &info.source_fbx_guid) {
                    Ok(gs) => {
                        if gs.is_empty() {
                            log::debug!(
                                "  Variant resolve: empty (using guid={} as-is)",
                                info.source_fbx_guid
                            );
                            vec![info.source_fbx_guid.clone()]
                        } else {
                            log::debug!("  Variant resolved: {} -> {:?}", info.source_fbx_guid, gs);
                            gs
                        }
                    }
                    Err(e) => {
                        log::warn!("Variant resolve failed: {}", e);
                        continue;
                    }
                };

                if !resolved_guids.contains(&fbx_guid.to_string()) {
                    log::debug!(
                        "  resolved_guids={:?}, fbx_guid={}, match=false",
                        resolved_guids,
                        fbx_guid
                    );
                    continue;
                }
                log::debug!(
                    "  resolved_guids={:?}, fbx_guid={}, match=true",
                    resolved_guids,
                    fbx_guid
                );
                new_matched = true;

                // FBX の .meta から externalObjects を取得
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

                // マテリアル GUID セット構築: FBX .meta + Prefab オーバーライド
                // ユニークなマテリアル GUID を順序付きで収集
                // (Variant Prefab では異なる SMR の同じスロットに異なるマテリアルが割り当てられるため、
                //  slot_index をキーにすると上書きが発生する → ユニーク GUID を連番で管理)
                let mut all_mat_guids: Vec<String> = Vec::new();

                // FBX .meta の externalObjects を基本として登録
                for fbx_mat in &meta_materials {
                    if !all_mat_guids.contains(&fbx_mat.material_guid) {
                        all_mat_guids.push(fbx_mat.material_guid.clone());
                    }
                }

                // Prefab のオーバーライドを追加（ユニーク）
                for ov in &info.material_overrides {
                    if !all_mat_guids.contains(&ov.material_guid) {
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

        // Old 形式のパース（New 形式でマッチしなかった場合のフォールバック含む）
        // 混合形式の Prefab（SkinnedMeshRenderer + PrefabInstance が共存）に対応
        if format == PrefabFormat::Old || (format == PrefabFormat::New && !new_matched) {
            if format == PrefabFormat::New {
                log::debug!(
                    "  New format no match -> Old format fallback ({})",
                    entry.pathname
                );
            }
            let infos = match parse_prefab_old(&content) {
                Ok(v) => v,
                Err(e) => {
                    log::warn!("Old Prefab parse failed ({}): {}", entry.pathname, e);
                    continue;
                }
            };

            // 同じ FBX GUID の複数 SkinnedMeshRenderer のマテリアル GUID を統合
            let mut all_mat_guids: Vec<String> = Vec::new();
            let mut has_match = false;
            for info in &infos {
                if info.fbx_guid != fbx_guid {
                    continue;
                }
                has_match = true;
                for guid in &info.material_guids {
                    if !all_mat_guids.contains(guid) {
                        all_mat_guids.push(guid.clone());
                    }
                }
            }
            if !has_match {
                continue;
            }

            // FBX .meta からマテリアル GUID → FBX マテリアル名を取得
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

            // .meta から取得できたマテリアルも追加
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
            // 所有権を移動するため候補リストから取り出す
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

/// 単一 FBX の解決結果
pub struct FbxResolveEntry {
    pub fbx_guid: String,
    pub fbx_index: usize,
    pub materials: Vec<ResolvedMaterialTextures>,
}

/// Prefab 全体の解決結果（複数 FBX を含む可能性あり）
pub struct PrefabResolveResult {
    pub entries: Vec<FbxResolveEntry>,
}

/// 特定の Prefab エントリから全 FBX を解決し、テクスチャマッピングを返す
pub fn resolve_single_prefab(
    pkg: &UnityPackageIndex,
    prefab_index: usize,
) -> PkgResult<PrefabResolveResult> {
    resolve_single_prefab_inner(pkg, prefab_index, 0)
}

/// 再帰対応の内部実装（depth で無限ループ防止）
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

    // New 形式パース（PrefabInstance ブロック）
    if format == PrefabFormat::New {
        let infos = parse_prefab_new(&content)?;
        parsed_count += infos.len();
        for info in infos {
            // 参照先が Prefab かどうかチェック（Nested Prefab / Variant 対応）
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
                // Nested Prefab: 再帰的に解決して全 FBX エントリを取得
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
                // 参照先が FBX（または Variant 経由で FBX に解決）
                let resolved_guid =
                    resolve_variant(pkg, &info.source_fbx_guid)?.unwrap_or(info.source_fbx_guid);
                if !seen_guids.insert(resolved_guid.clone()) {
                    continue; // 重複 FBX GUID スキップ
                }
                let Some(&fbx_idx) = pkg.by_guid.get(&resolved_guid) else {
                    log::warn!("Prefab referenced FBX not found: GUID={}", resolved_guid);
                    continue;
                };
                // 新形式: FBX .meta + Prefab オーバーライド
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

    // Old 形式パース（SkinnedMeshRenderer セクション）
    // 混合形式対応: New の後にも Old を常に実行し、追加の FBX 参照を取得
    {
        if let Ok(infos) = parse_prefab_old(&content) {
            if !infos.is_empty() {
                parsed_count += infos.len();
                // 同じ FBX GUID を参照する複数の SkinnedMeshRenderer のマテリアルを統合
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
                        continue; // New 経路で既に追加済み
                    }
                    let Some(&fbx_idx) = pkg.by_guid.get(&fbx_guid) else {
                        log::warn!("Prefab referenced FBX not found: GUID={}", fbx_guid);
                        continue;
                    };
                    // FBX .meta からマテリアル GUID → FBX マテリアル名のマッピングを構築
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
                    // .meta から取得できていないマテリアルも Prefab の m_Materials から追加
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

/// マテリアル GUID マップからテクスチャ解決結果を生成（.meta の FBX マテリアル名付き）
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

        // _MainTex を最優先、次に _BaseMap、最後に _BaseColorMap（lilToon 等では
        // _BaseColorMap が _MainTex と異なるテクスチャを参照する場合があるため）
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

        // ノーマルマップ: _BumpMap (Standard/lilToon/Poiyomi/AXCS/WF) > _NormalMap (UTS2)
        let bump_map = parsed.textures.iter().find(|t| t.slot_name == "_BumpMap");
        let normal_map = parsed.textures.iter().find(|t| t.slot_name == "_NormalMap");
        // 両方存在して GUID が異なる場合は warn
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

        // Emission テクスチャ
        let emission_tex_guid = parsed
            .textures
            .iter()
            .find(|t| t.slot_name == "_EmissionMap")
            .map(|t| Arc::from(t.texture_guid.as_str()));

        // Emission 色（_EmissionColor）
        let emission_color = parsed
            .colors
            .iter()
            .find(|c| c.param_name == "_EmissionColor")
            .map(|c| [c.r, c.g, c.b])
            .unwrap_or([0.0; 3]);

        // Emission 有効判定（優先順）:
        // 1. _Emission float が明示的にある場合はその値で判定
        // 2. m_ShaderKeywords / m_ValidKeywords に _EMISSION が含まれる場合は有効
        // 3. _EmissionMap テクスチャがある場合は有効
        // 4. _EmissionColor が非黒かつ非白の場合は有効
        //    白 (1,1,1) 除外理由: 多くのシェーダーが emission 無効時でも白で初期化（実例: Masscat v1.02）
        let has_emission_keyword = parsed.shader_keywords.iter().any(|kw| kw == "_EMISSION");
        let emission_color_meaningful =
            emission_color != [0.0; 3] && emission_color != [1.0, 1.0, 1.0];
        let emission_enabled = parsed
            .floats
            .iter()
            .find(|f| f.param_name == "_Emission")
            .map(|f| f.value >= 0.5)
            .unwrap_or(
                has_emission_keyword || emission_tex_guid.is_some() || emission_color_meaningful,
            );

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
        });
    }

    resolved_mats
}

// ── Step 8: PackageTexture + PreparedPkgFbx ──

/// unitypackage 内のテクスチャ（GUID 付き）
pub struct PackageTexture {
    pub guid: Arc<str>,
    pub display_name: Arc<str>,
    pub data: Arc<[u8]>,
    /// アーカイブ内フルパス（例: "Assets/texture/body.png"）
    pub pathname: Arc<str>,
}

/// Prefab 解決済み FBX パッケージ
pub struct PreparedPkgFbx {
    pub model: PkgModelLocator,
    pub fbx_data: Arc<[u8]>,
    pub textures: Vec<PackageTexture>,
    pub resolved: Vec<ResolvedMaterialTextures>,
}

/// 画像ファイル拡張子かどうか
fn is_image_extension(path: &str) -> bool {
    let lower = path.to_lowercase();
    const IMAGE_EXTS: &[&str] = &[
        ".png", ".jpg", ".jpeg", ".tga", ".bmp", ".psd", ".tif", ".tiff",
    ];
    IMAGE_EXTS.iter().any(|ext| lower.ends_with(ext))
}

/// UnityPackageIndex から FBX を準備（Prefab 解決 + テクスチャ収集）
pub fn prepare_pkg_fbx(pkg: &UnityPackageIndex, fbx_index: usize) -> Result<PreparedPkgFbx> {
    let entry = &pkg.entries[fbx_index];
    let fbx_guid = &entry.guid;
    let fbx_path = &entry.pathname;

    // PkgModelLocator 構築
    let model = PkgModelLocator {
        guid: Arc::from(fbx_guid.as_str()),
        pathname: Arc::from(fbx_path.as_str()),
        kind: PkgModelType::Fbx,
    };

    // Prefab テクスチャ解決
    let resolved = resolve_prefab_textures(pkg, fbx_guid, fbx_path);

    // テクスチャ収集（画像拡張子フィルタ）
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

/// FBX インデックス一覧からベストな FBX を自動選択
///
/// 選択基準（優先順）:
/// 1. ファイルサイズが最大の FBX を優先（本体モデルはアニメーション・小道具より大きい傾向）
pub fn select_best_fbx_index(pkg: &UnityPackageIndex, fbx_indices: &[(usize, String)]) -> usize {
    if fbx_indices.len() == 1 {
        return fbx_indices[0].0;
    }
    // データサイズが最大の FBX を選択
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

/// Prefab 解決テクスチャを IrModel に埋め込み（三段階フォールバック）
/// 戻り値: 未割当材質のインデックス一覧
pub fn embed_textures_with_prefab(
    ir: &mut crate::intermediate::types::IrModel,
    textures: &[PackageTexture],
    resolved: &[ResolvedMaterialTextures],
    prefab_label: &str,
) -> Vec<usize> {
    if textures.is_empty() {
        return (0..ir.materials.len()).collect();
    }

    // GUID → PackageTexture の逆引き
    let tex_by_guid: HashMap<&str, &PackageTexture> =
        textures.iter().map(|t| (t.guid.as_ref(), t)).collect();

    // 既に追加済みのテクスチャ GUID → ir.textures index
    let mut added_guids: HashMap<Arc<str>, usize> = HashMap::new();

    let mut matched_base = 0usize;
    let mut matched_normal = 0usize;

    // ── 戦略1: source_material で照合（Phase 3 で有効化）──
    // 現在は IrMaterial.source_material が None なので実質スキップ

    // ── 戦略2: material_name / fbx_material_name で照合 ──
    if !resolved.is_empty() {
        // resolved の material_name → index マップ（完全一致用）
        let resolved_by_name: HashMap<&str, &ResolvedMaterialTextures> = resolved
            .iter()
            .map(|r| (r.material_name.as_ref(), r))
            .collect();

        // 小文字化マップ（大文字小文字無視マッチ用）
        let resolved_by_lower: HashMap<String, &ResolvedMaterialTextures> = resolved
            .iter()
            .map(|r| (r.material_name.to_lowercase(), r))
            .collect();

        // fbx_material_name による照合マップ（FBX .meta の externalObjects から取得した名前）
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
            // 完全一致 → 大文字小文字無視 → FBX名完全一致 → FBX名大小無視 → サフィックス一致
            let mat_lower = mat.name.to_lowercase();
            let res_opt = resolved_by_name
                .get(mat.name.as_str())
                .copied()
                .or_else(|| resolved_by_lower.get(mat_lower.as_str()).copied())
                .or_else(|| resolved_by_fbx_name.get(mat.name.as_str()).copied())
                .or_else(|| resolved_by_fbx_lower.get(mat_lower.as_str()).copied())
                .or_else(|| {
                    // サフィックス一致: resolved 名（小文字）が mat.name（小文字）で終わるか、
                    // mat.name（小文字）が resolved 名（小文字）で終わる
                    // 例: "fc_milltina_body" ends_with "milltina_body"
                    resolved.iter().find(|r| {
                        let r_lower = r.material_name.to_lowercase();
                        if r_lower.ends_with(&mat_lower) || mat_lower.ends_with(&r_lower) {
                            return true;
                        }
                        // fbx_material_name でもサフィックス一致を試行
                        if let Some(ref fbx_name) = r.fbx_material_name {
                            let f_lower = fbx_name.to_lowercase();
                            f_lower.ends_with(&mat_lower) || mat_lower.ends_with(&f_lower)
                        } else {
                            false
                        }
                    })
                });

            if let Some(res) = res_opt {
                // ── メインテクスチャ ──
                if mat.texture_index.is_none() {
                    if let Some(ref tex_guid) = res.main_texture_guid {
                        // 既に追加済みならインデックスを再利用
                        if let Some(&existing_idx) = added_guids.get(tex_guid) {
                            mat.texture_index = Some(existing_idx);
                            matched_base += 1;
                            log::info!(
                                "Prefab texture assign (name, reuse): {} -> mat[{}]",
                                tex_guid,
                                mat.name
                            );
                        } else if let Some(pkg_tex) = tex_by_guid.get(tex_guid.as_ref()) {
                            let tex_idx = ir.textures.len();
                            let ext = std::path::Path::new(pkg_tex.display_name.as_ref())
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("")
                                .to_lowercase();
                            let mime = crate::intermediate::types::mime_for_ext(&ext).to_string();
                            ir.textures.push(crate::intermediate::types::IrTexture {
                                filename: pkg_tex.display_name.to_string(),
                                data: TextureData::Encoded(pkg_tex.data.to_vec()),
                                mime_type: mime,
                                source_path: format!("{}: {}", prefab_label, pkg_tex.pathname),
                                mip_chain: None,
                            });
                            mat.texture_index = Some(tex_idx);
                            added_guids.insert(Arc::clone(tex_guid), tex_idx);
                            matched_base += 1;
                            log::info!(
                                "Prefab texture assign (name): {} -> mat[{}]",
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

                // ── ノーマルマップ ──
                if mat.normal_texture.is_none() {
                    if let Some(ref normal_guid) = res.normal_texture_guid {
                        if let Some(&existing_idx) = added_guids.get(normal_guid) {
                            mat.normal_texture = Some(
                                crate::intermediate::types::IrTextureInfo::from_index(existing_idx),
                            );
                            mat.normal_texture_scale = res.bump_scale;
                            matched_normal += 1;
                            log::info!(
                                "Prefab normal map assign (reuse): {} -> mat[{}]",
                                normal_guid,
                                mat.name
                            );
                        } else if let Some(pkg_tex) = tex_by_guid.get(normal_guid.as_ref()) {
                            let tex_idx = ir.textures.len();
                            let ext = std::path::Path::new(pkg_tex.display_name.as_ref())
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("")
                                .to_lowercase();
                            let mime = crate::intermediate::types::mime_for_ext(&ext).to_string();
                            ir.textures.push(crate::intermediate::types::IrTexture {
                                filename: pkg_tex.display_name.to_string(),
                                data: TextureData::Encoded(pkg_tex.data.to_vec()),
                                mime_type: mime,
                                source_path: format!("{}: {}", prefab_label, pkg_tex.pathname),
                                mip_chain: None,
                            });
                            mat.normal_texture = Some(
                                crate::intermediate::types::IrTextureInfo::from_index(tex_idx),
                            );
                            mat.normal_texture_scale = res.bump_scale;
                            added_guids.insert(Arc::clone(normal_guid), tex_idx);
                            matched_normal += 1;
                            log::info!(
                                "Prefab normal map assign: {} -> mat[{}]",
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
                    // emissive_factor に Emission 色をセット
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

                    // Emission テクスチャ
                    if mat.emissive_texture.is_none() {
                        if let Some(ref em_guid) = res.emission_texture_guid {
                            if let Some(&existing_idx) = added_guids.get(em_guid) {
                                mat.emissive_texture =
                                    Some(crate::intermediate::types::IrTextureInfo::from_index(
                                        existing_idx,
                                    ));
                                log::info!(
                                    "Prefab emission texture assign (reuse): {} -> mat[{}]",
                                    em_guid,
                                    mat.name
                                );
                            } else if let Some(pkg_tex) = tex_by_guid.get(em_guid.as_ref()) {
                                let tex_idx = ir.textures.len();
                                let ext = std::path::Path::new(pkg_tex.display_name.as_ref())
                                    .extension()
                                    .and_then(|e| e.to_str())
                                    .unwrap_or("")
                                    .to_lowercase();
                                let mime =
                                    crate::intermediate::types::mime_for_ext(&ext).to_string();
                                ir.textures.push(crate::intermediate::types::IrTexture {
                                    filename: pkg_tex.display_name.to_string(),
                                    data: TextureData::Encoded(pkg_tex.data.to_vec()),
                                    mime_type: mime,
                                    source_path: format!("{}: {}", prefab_label, pkg_tex.pathname),
                                    mip_chain: None,
                                });
                                mat.emissive_texture = Some(
                                    crate::intermediate::types::IrTextureInfo::from_index(tex_idx),
                                );
                                added_guids.insert(Arc::clone(em_guid), tex_idx);
                                log::info!(
                                    "Prefab Emission Texture assigned: {} -> mat[{}]",
                                    pkg_tex.display_name,
                                    mat.name
                                );
                            }
                        }
                    }

                    // emissive_texture があるのに emissive_factor がゼロだと
                    // シェーダーで 0 * texture = 0 になり発光しない。白に補正。
                    if mat.emissive_texture.is_some() && mat.emissive_factor == glam::Vec3::ZERO {
                        mat.emissive_factor = glam::Vec3::ONE;
                        log::info!(
                            "Prefab emission color correction: (0,0,0) -> (1,1,1) (has texture) mat[{}]",
                            mat.name
                        );
                    }
                }
            }
        }
    }

    // ── 戦略3: source_texture_name ファイル名マッチング（既存ロジック流用）──
    // ファイル名 → PackageTexture の逆引き
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
                // GUID 重複チェック
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
                let ext = std::path::Path::new(key.as_str())
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                let mime = crate::intermediate::types::mime_for_ext(&ext).to_string();
                ir.textures.push(crate::intermediate::types::IrTexture {
                    filename: pkg_tex.display_name.to_string(),
                    data: TextureData::Encoded(pkg_tex.data.to_vec()),
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

// ── Step 6: ユニットテスト ──

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
        // Unpacked 形式: m_PrefabInstance フィールドはあるが、独立した PrefabInstance: ブロックはない
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
        // materialImportMode が 0 でも externalObjects を返す（チェック緩和済み）
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
        // materialImportMode: 1 でも externalObjects を返す
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
        // externalObjects: {} （空インライン形式）は空 Vec を返す
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
        // _MainTex と _BaseMap の2つ（_BumpMap は fileID: 0 でスキップ）
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
        // m_Floats なし → floats 空
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
        // m_Floats: [] （空配列インライン形式）は floats 空
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
        let mut mat = crate::intermediate::types::IrMaterial::default();
        mat.name = "Body".into();
        mat.texture_index = Some(0); // ベースカラーは既にある
        ir.materials.push(mat);
        ir.textures.push(crate::intermediate::types::IrTexture {
            filename: "base.png".into(),
            data: TextureData::Encoded(vec![0u8; 4]),
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
        }];

        let unmatched = embed_textures_with_prefab(&mut ir, &textures, &resolved, "test");

        // ベースカラーは設定済みなので unmatched は空
        assert!(unmatched.is_empty());
        // ノーマルマップが割り当てられている
        assert_eq!(
            ir.materials[0].normal_texture.as_ref().map(|t| t.index),
            Some(1)
        );
        assert!((ir.materials[0].normal_texture_scale - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn test_embed_unmatched_based_on_base_texture_only() {
        // ベースカラーなし + ノーマルマップありでも unmatched に入る
        let mut ir = crate::intermediate::types::IrModel::default();
        let mut mat = crate::intermediate::types::IrMaterial::default();
        mat.name = "Body".into();
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
        }];

        let unmatched = embed_textures_with_prefab(&mut ir, &textures, &resolved, "test");

        // ベースカラーが None のまま → unmatched に含まれる
        assert_eq!(unmatched, vec![0]);
        // ノーマルマップは割り当てられている
        assert!(ir.materials[0].normal_texture.is_some());
    }

    #[test]
    fn test_embed_normal_reuses_added_guid() {
        // 同じノーマルマップ GUID を2つの材質で共有
        let mut ir = crate::intermediate::types::IrModel::default();
        for name in &["Body", "Face"] {
            let mut mat = crate::intermediate::types::IrMaterial::default();
            mat.name = (*name).into();
            mat.texture_index = Some(0);
            ir.materials.push(mat);
        }
        ir.textures.push(crate::intermediate::types::IrTexture {
            filename: "base.png".into(),
            data: TextureData::Encoded(vec![0u8; 4]),
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
            },
        ];

        let _unmatched = embed_textures_with_prefab(&mut ir, &textures, &resolved, "test");

        // 両方の材質が同じテクスチャインデックスを参照
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
        // テクスチャは1回だけ追加されている（base + normal = 2）
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
}
