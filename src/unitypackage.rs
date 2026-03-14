//! .unitypackage (tar.gz) からアセットを抽出するモジュール

use std::collections::HashMap;
use std::io::{Cursor, Read};
use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;

/// 展開されたアセット情報
pub struct ExtractedAsset {
    /// Unity プロジェクト内パス（例: "Assets/Models/xxx.fbx"）
    pub pathname: String,
    /// アセット本体データ
    pub data: Vec<u8>,
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

/// .unitypackage からすべてのアセットを展開
pub fn extract_all_assets(archive_data: &[u8]) -> Result<Vec<ExtractedAsset>> {
    let decoder = GzDecoder::new(Cursor::new(archive_data));
    let mut archive = tar::Archive::new(decoder);

    // GUID → (pathname, asset_data) を収集
    let mut pathnames: HashMap<String, String> = HashMap::new();
    let mut assets: HashMap<String, Vec<u8>> = HashMap::new();

    for entry in archive.entries().context("tarエントリ読み込み失敗")? {
        let mut entry = entry.context("tarエントリ解析失敗")?;
        let path = entry.path().context("パス取得失敗")?.to_string_lossy().to_string();
        // パスは "GUID/filename" 形式
        let parts: Vec<&str> = path.splitn(3, |c| c == '/' || c == '\\').collect();
        if parts.len() < 2 {
            continue;
        }
        let guid = parts[0].to_string();
        let filename = parts[1];

        match filename {
            "pathname" => {
                let mut s = String::new();
                entry.read_to_string(&mut s).context("pathname読み込み失敗")?;
                pathnames.insert(guid, s.trim().to_string());
            }
            "asset" => {
                let mut data = Vec::new();
                entry.read_to_end(&mut data).context("asset読み込み失敗")?;
                assets.insert(guid, data);
            }
            _ => {} // asset.meta, preview.png は無視
        }
    }

    // pathname と asset を結合
    let mut result = Vec::new();
    for (guid, pathname) in pathnames {
        if let Some(data) = assets.remove(&guid) {
            result.push(ExtractedAsset { pathname, data });
        }
    }
    Ok(result)
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
) -> Result<(Vec<u8>, String, Vec<(String, Vec<u8>)>)> {
    if fbx_index >= assets.len() {
        bail!("FBXインデックスが範囲外: {}", fbx_index);
    }

    // FBX を取り出す
    let fbx_asset = assets.swap_remove(fbx_index);
    let fbx_name = fbx_asset.filename();
    let fbx_data = fbx_asset.data;

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
            (name, a.data)
        })
        .collect();

    log::info!(
        ".unitypackage 展開: FBX={} ({}KB), テクスチャ={}個",
        fbx_name,
        fbx_data.len() / 1024,
        textures.len(),
    );

    Ok((fbx_data, fbx_name, textures))
}

/// .unitypackage から FBX を探して抽出する（CLI用: 最初のFBXを使用）
/// 戻り値: (FBXデータ, FBXファイル名, テクスチャ一覧 [(パス名, データ)])
pub fn extract_fbx_from_unitypackage(
    archive_data: &[u8],
) -> Result<(Vec<u8>, String, Vec<(String, Vec<u8>)>)> {
    let assets = extract_all_assets(archive_data)?;
    let fbx_list = find_fbx_list(&assets);

    if fbx_list.is_empty() {
        bail!(".unitypackage 内に FBX ファイルが見つかりません");
    }

    // 複数ある場合はログ出力
    if fbx_list.len() > 1 {
        log::info!(
            ".unitypackage 内に {} 個の FBX が見つかりました。最初のものを使用: {}",
            fbx_list.len(),
            fbx_list[0].1,
        );
        for (_, name) in &fbx_list {
            log::info!("  FBX: {}", name);
        }
    }

    take_fbx_and_textures(assets, fbx_list[0].0)
}

/// unitypackage内テクスチャをIrModelの材質に自動割り当て
/// 材質の source_texture_name とテクスチャファイル名をマッチングする
/// 戻り値: 未割当材質のインデックス一覧
pub fn embed_textures_into_ir(
    ir: &mut crate::intermediate::types::IrModel,
    textures: &[(String, Vec<u8>)],
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
    let stem_map: HashMap<String, String> = tex_map.keys()
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
                ir.textures.push(crate::intermediate::types::IrTexture {
                    filename: key.clone(),
                    data: data.to_vec(),
                    mime_type: String::new(),
                });
                mat.texture_index = Some(tex_idx);
                matched += 1;
                log::info!("テクスチャ割当: {} → mat[{}]",
                    mat.source_texture_name.as_deref().unwrap_or("?"),
                    mat.name,
                );
            }
        }
    }

    // 未割当材質のインデックスを収集
    let unmatched: Vec<usize> = ir.materials.iter().enumerate()
        .filter(|(_, mat)| mat.texture_index.is_none())
        .map(|(i, _)| i)
        .collect();

    log::info!("unitypackageテクスチャ: {}/{}材質マッチ, 未割当: {}",
        matched, ir.materials.len(), unmatched.len());
    unmatched
}
