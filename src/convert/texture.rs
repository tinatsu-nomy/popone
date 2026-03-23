use anyhow::Result;
use image::{ImageBuffer, RgbaImage};
use std::path::Path;

use crate::intermediate::types::IrTexture;

/// テクスチャをPNGとして書き出す
pub fn write_texture(
    tex: &IrTexture,
    output_dir: &Path,
    width: u32,
    height: u32,
) -> Result<String> {
    let out_path = output_dir.join(&tex.filename);

    // 生ピクセルデータ（gltf::image::Data.pixels）はRGB8またはRGBA8
    // widthとheightが必要
    let img: RgbaImage = if tex.data.len() == (width * height * 4) as usize {
        // RGBA8
        ImageBuffer::from_raw(width, height, tex.data.clone())
            .ok_or_else(|| anyhow::anyhow!("RGBA画像バッファ生成失敗: {}", tex.filename))?
    } else if tex.data.len() == (width * height * 3) as usize {
        // RGB8 → RGBA8変換
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for chunk in tex.data.chunks(3) {
            rgba.push(chunk[0]);
            rgba.push(chunk[1]);
            rgba.push(chunk[2]);
            rgba.push(255);
        }
        ImageBuffer::from_raw(width, height, rgba)
            .ok_or_else(|| anyhow::anyhow!("RGB→RGBA変換失敗: {}", tex.filename))?
    } else {
        // サイズ不一致の場合は空白画像
        log::warn!(
            "テクスチャ '{}' のサイズが不一致 (data={}, expected={}x{})",
            tex.filename,
            tex.data.len(),
            width,
            height
        );
        RgbaImage::new(1, 1)
    };

    img.save(&out_path)?;
    log::info!("テクスチャ書き出し: {}", out_path.display());

    Ok(tex.filename.clone())
}

/// 全テクスチャを書き出す
/// gltf::image::DataのwidthとheightをImagesから取得
pub fn write_all_textures(
    textures: &[IrTexture],
    images: &[gltf::image::Data],
    output_dir: &Path,
) -> Result<Vec<String>> {
    std::fs::create_dir_all(output_dir)?;
    let mut filenames = Vec::new();

    for (i, tex) in textures.iter().enumerate() {
        if let Some(img_data) = images.get(i) {
            let filename = write_texture(tex, output_dir, img_data.width, img_data.height)?;
            filenames.push(filename);
        }
    }

    Ok(filenames)
}

/// IrTexture のデータ（PNG/JPEG バイナリ）をそのまま書き出す（FBX 用）
pub fn write_all_textures_from_ir(
    textures: &[IrTexture],
    output_dir: &Path,
) -> Result<Vec<String>> {
    if textures.is_empty() {
        return Ok(Vec::new());
    }
    std::fs::create_dir_all(output_dir)?;
    let mut filenames = Vec::new();
    for tex in textures {
        let out_path = output_dir.join(&tex.filename);
        std::fs::write(&out_path, &tex.data)?;
        log::info!("テクスチャ書き出し: {}", out_path.display());
        filenames.push(tex.filename.clone());
    }
    Ok(filenames)
}
