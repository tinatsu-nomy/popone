use anyhow::{Context, Result};
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
