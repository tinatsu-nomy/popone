use crate::error::{Result, ResultExt};
use serde_json::Value;
use std::path::Path;

pub struct GlbData {
    pub document: gltf::Document,
    pub buffers: Vec<gltf::buffer::Data>,
    pub images: Vec<gltf::image::Data>,
    pub vrm_extension: Value,
}

pub fn load_glb(path: &Path) -> Result<GlbData> {
    let (document, buffers, images) = gltf::import(path)
        .with_context(|| format!("GLBファイルの読み込みに失敗: {}", path.display()))?;

    // VRM拡張をJSONから取得
    let vrm_extension = extract_vrm_extension(&document)?;

    Ok(GlbData {
        document,
        buffers,
        images,
        vrm_extension,
    })
}

/// バイト列から GLB/VRM を読み込む（unitypackage 内 VRM 用）
pub fn load_glb_from_data(data: &[u8]) -> Result<GlbData> {
    let (document, buffers, images) =
        gltf::import_slice(data).context("GLBデータの読み込みに失敗")?;

    let vrm_extension = extract_vrm_extension(&document)?;

    Ok(GlbData {
        document,
        buffers,
        images,
        vrm_extension,
    })
}

fn extract_vrm_extension(document: &gltf::Document) -> Result<Value> {
    let json = document.as_json();

    if let Some(exts) = &json.extensions {
        // VRM 1.0: VRMC_vrm
        if let Some(val) = exts.others.get("VRMC_vrm") {
            return Ok(val.clone());
        }
        // VRM 0.0: VRM
        if let Some(val) = exts.others.get("VRM") {
            return Ok(val.clone());
        }
    }

    Ok(Value::Object(serde_json::Map::new()))
}

/// GLBドキュメントの全extensions（フラット）をValueとして返す
pub fn get_raw_extensions(document: &gltf::Document) -> Value {
    let json = document.as_json();
    if let Some(exts) = &json.extensions {
        let mut map = serde_json::Map::new();
        for (k, v) in &exts.others {
            map.insert(k.clone(), v.clone());
        }
        Value::Object(map)
    } else {
        Value::Object(serde_json::Map::new())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_load_seed_san_vrm() {
        let Some(sample) = crate::test_util::try_test_file(crate::test_util::seed_san_vrm()) else {
            return;
        };

        // GLBとして読み込み
        let glb = super::load_glb(&sample).expect("VRM読み込み失敗");
        let version = crate::vrm::detect::detect_version(&glb.document);
        let all_extensions = super::get_raw_extensions(&glb.document);

        // VRM 1.0 であることを確認
        assert_eq!(
            version,
            crate::vrm::detect::VrmVersion::V1,
            "Seed-san.vrm は VRM 1.0 であるべき"
        );

        // IrModel に変換
        let ir = crate::vrm::extract::extract_ir_model(
            &glb.document,
            &glb.buffers,
            &glb.images,
            &glb.vrm_extension,
            &version,
            &all_extensions,
        )
        .expect("IrModel抽出失敗");

        assert_eq!(
            ir.source_format,
            crate::intermediate::types::SourceFormat::Vrm1,
            "ソース形式が Vrm1 であるべき"
        );
        assert!(
            ir.bones.len() > 100,
            "ボーン数が少なすぎる: {}",
            ir.bones.len()
        );
        assert!(!ir.meshes.is_empty(), "メッシュが空");
        assert!(!ir.materials.is_empty(), "材質が空");
        assert!(!ir.textures.is_empty(), "テクスチャが空");
        assert!(!ir.morphs.is_empty(), "モーフが空");
    }
}
