use crate::error::Result;
use image::RgbaImage;
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
    if tex.data.len() == (width * height * 4) as usize {
        // RGBA8 — スライス参照から直接保存（clone 回避）
        image::save_buffer(&out_path, &tex.data, width, height, image::ColorType::Rgba8)?;
    } else if tex.data.len() == (width * height * 3) as usize {
        // RGB8 → RGBA8変換
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for chunk in tex.data.chunks(3) {
            rgba.push(chunk[0]);
            rgba.push(chunk[1]);
            rgba.push(chunk[2]);
            rgba.push(255);
        }
        image::save_buffer(&out_path, &rgba, width, height, image::ColorType::Rgba8)?;
    } else {
        // サイズ不一致の場合は空白画像
        log::warn!(
            "Texture '{}' size mismatch (data={}, expected={}x{})",
            tex.filename,
            tex.data.len(),
            width,
            height
        );
        RgbaImage::new(1, 1).save(&out_path)?;
    };
    log::info!("Texture export: {}", out_path.display());

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
/// PSD データの場合は PNG に変換して書き出す
pub fn write_all_textures_from_ir(
    textures: &[IrTexture],
    output_dir: &Path,
) -> Result<Vec<String>> {
    if textures.is_empty() {
        return Ok(Vec::new());
    }
    std::fs::create_dir_all(output_dir)?;

    // 書き出し済みファイル名を逐次追跡（全テクスチャの衝突検出・回避用）
    let mut used_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    let mut filenames = Vec::new();
    for tex in textures {
        if crate::psd::is_psd_filename(&tex.filename) {
            // PSD → PNG 変換
            match crate::psd::psd_to_png(&tex.data) {
                Ok(png_data) => {
                    let stem = std::path::Path::new(&tex.filename)
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    let mut candidate = format!("{}.png", stem);
                    if used_names.contains(&candidate.to_lowercase()) {
                        candidate = format!("{}_from_psd.png", stem);
                        let mut suffix = 2u32;
                        while used_names.contains(&candidate.to_lowercase()) {
                            candidate = format!("{}_from_psd{}.png", stem, suffix);
                            suffix += 1;
                        }
                        log::info!("PSD->PNG: collision avoidance -> '{}'", candidate);
                    }
                    used_names.insert(candidate.to_lowercase());
                    let out_path = output_dir.join(&candidate);
                    std::fs::write(&out_path, &png_data)?;
                    log::info!("Texture export (PSD->PNG): {}", out_path.display());
                    filenames.push(candidate);
                }
                Err(e) => {
                    log::warn!("PSD->PNG conversion failed, exporting as PSD: {e}");
                    // 衝突回避（非PSD分岐と同じロジック）
                    let mut out_name = tex.filename.clone();
                    if used_names.contains(&out_name.to_lowercase()) {
                        let p = std::path::Path::new(&tex.filename);
                        let stem = p.file_stem().unwrap_or_default().to_string_lossy();
                        let ext = p.extension().unwrap_or_default().to_string_lossy();
                        let mut suffix = 2u32;
                        loop {
                            out_name = if ext.is_empty() {
                                format!("{}_{}", stem, suffix)
                            } else {
                                format!("{}_{}.{}", stem, suffix, ext)
                            };
                            if !used_names.contains(&out_name.to_lowercase()) {
                                break;
                            }
                            suffix += 1;
                        }
                        log::info!(
                            "PSD collision avoidance: '{}' -> '{}'",
                            tex.filename,
                            out_name
                        );
                    }
                    used_names.insert(out_name.to_lowercase());
                    let out_path = output_dir.join(&out_name);
                    std::fs::write(&out_path, &tex.data)?;
                    filenames.push(out_name);
                }
            }
        } else {
            // 非PSD: 同名ファイルの衝突回避（手動割当で PSD→PNG 済みの同名エントリ等）
            let mut out_name = tex.filename.clone();
            if used_names.contains(&out_name.to_lowercase()) {
                let p = std::path::Path::new(&tex.filename);
                let stem = p.file_stem().unwrap_or_default().to_string_lossy();
                let ext = p.extension().unwrap_or_default().to_string_lossy();
                let mut suffix = 2u32;
                loop {
                    out_name = if ext.is_empty() {
                        format!("{}_{}", stem, suffix)
                    } else {
                        format!("{}_{}.{}", stem, suffix, ext)
                    };
                    if !used_names.contains(&out_name.to_lowercase()) {
                        break;
                    }
                    suffix += 1;
                }
                log::info!(
                    "Texture name collision avoidance: '{}' -> '{}'",
                    tex.filename,
                    out_name
                );
            }
            used_names.insert(out_name.to_lowercase());
            let out_path = output_dir.join(&out_name);
            if tex.is_raw_rgba() {
                // 生 RGBA は PNG エンコードして書き出す
                let (w, h) = tex.raw_dims.expect("is_raw_rgba で確認済み");
                if let Some(img) = image::RgbaImage::from_raw(w, h, tex.data.clone()) {
                    img.save(&out_path)?;
                } else {
                    log::warn!(
                        "Raw RGBA texture size mismatch: {} ({}x{}, data={})",
                        tex.filename,
                        w,
                        h,
                        tex.data.len()
                    );
                    RgbaImage::new(1, 1).save(&out_path)?;
                }
            } else {
                std::fs::write(&out_path, &tex.data)?;
            }
            log::info!("Texture export: {}", out_path.display());
            filenames.push(out_name);
        }
    }
    Ok(filenames)
}
