use crate::error::Result;
use image::RgbaImage;
use std::path::Path;

use crate::intermediate::types::{IrTexture, TextureData};

/// Write a texture as PNG.
pub fn write_texture(
    tex: &IrTexture,
    output_dir: &Path,
    width: u32,
    height: u32,
) -> Result<String> {
    let out_path = output_dir.join(&tex.filename);

    // Raw pixel data (`gltf::image::Data.pixels`) is RGB8 or RGBA8.
    // Width and height are required.
    let bytes = tex.data.as_bytes();
    if bytes.len() == (width * height * 4) as usize {
        // RGBA8 — save directly from a slice ref (avoid clone)
        image::save_buffer(&out_path, bytes, width, height, image::ColorType::Rgba8)?;
    } else if bytes.len() == (width * height * 3) as usize {
        // RGB8 -> RGBA8 conversion
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for chunk in bytes.chunks(3) {
            rgba.push(chunk[0]);
            rgba.push(chunk[1]);
            rgba.push(chunk[2]);
            rgba.push(255);
        }
        image::save_buffer(&out_path, &rgba, width, height, image::ColorType::Rgba8)?;
    } else {
        // Blank image on size mismatch
        log::warn!(
            "Texture '{}' size mismatch (data={}, expected={}x{})",
            tex.filename,
            bytes.len(),
            width,
            height
        );
        RgbaImage::new(1, 1).save(&out_path)?;
    };
    log::info!("Texture export: {}", out_path.display());

    Ok(tex.filename.clone())
}

/// Write all textures.
/// Width/height for each `gltf::image::Data` are obtained from `Images`.
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

/// Write `IrTexture` data (PNG/JPEG binary) as-is (used for FBX).
/// PSD data is converted to PNG before writing.
pub fn write_all_textures_from_ir(
    textures: &[IrTexture],
    output_dir: &Path,
) -> Result<Vec<String>> {
    write_all_textures_from_ir_opt_cancel(textures, output_dir, None)
}

/// Write textures (cooperative cancellation variant).
/// When `cancel` is `Some`, the cancellation flag is checked once per texture.
pub fn write_all_textures_from_ir_opt_cancel(
    textures: &[IrTexture],
    output_dir: &Path,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Result<Vec<String>> {
    if textures.is_empty() {
        return Ok(Vec::new());
    }
    std::fs::create_dir_all(output_dir)?;

    // Track written filenames incrementally (for collision detection across all textures)
    let mut used_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    let mut filenames = Vec::new();
    for tex in textures {
        if let Some(c) = cancel {
            if c.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(crate::error::PoponeError::Other(
                    "texture export cancelled".into(),
                ));
            }
        }
        if crate::psd::is_psd_filename(&tex.filename) {
            // PSD -> PNG conversion
            match crate::psd::psd_to_png(tex.data.as_bytes()) {
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
                    // Collision avoidance (same logic as the non-PSD branch)
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
                    std::fs::write(&out_path, tex.data.as_bytes())?;
                    filenames.push(out_name);
                }
            }
        } else {
            // Non-PSD: avoid filename collision (e.g. manually-assigned PSD->PNG entries with the same name)
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
            match &tex.data {
                TextureData::RawRgba {
                    pixels,
                    width,
                    height,
                } => {
                    // Encode raw RGBA as PNG and write
                    if let Some(img) = image::RgbaImage::from_raw(*width, *height, pixels.to_vec())
                    {
                        img.save(&out_path)?;
                    } else {
                        log::warn!(
                            "Raw RGBA texture size mismatch: {} ({}x{}, data={})",
                            tex.filename,
                            width,
                            height,
                            pixels.len()
                        );
                        RgbaImage::new(1, 1).save(&out_path)?;
                    }
                }
                TextureData::Encoded(bytes) => {
                    std::fs::write(&out_path, bytes)?;
                }
            }
            log::info!("Texture export: {}", out_path.display());
            filenames.push(out_name);
        }
    }
    Ok(filenames)
}
