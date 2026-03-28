use crate::intermediate::types::IrModel;
use anyhow::Result;
use eframe::wgpu;

// PSD 関連関数は crate::psd に移動済み — 後方互換のため re-export
pub use crate::psd::{decode_psd, is_psd_filename};

/// RGBA データを GPU テクスチャにアップロード（共通処理）
/// GPU の最大テクスチャサイズを超える場合は自動的に縮小する
/// 戻り値: (sRGB ビュー, Unorm ビュー) — sRGB は標準描画用、Unorm は MMD 描画用
pub fn upload_rgba_to_gpu(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    rgba: &[u8],
    width: u32,
    height: u32,
    label: Option<&str>,
) -> (wgpu::TextureView, wgpu::TextureView) {
    let max_dim = device.limits().max_texture_dimension_2d;
    let (upload_owned, upload_w, upload_h) = if width > max_dim || height > max_dim {
        log::warn!(
            "テクスチャ {:?} ({}x{}) が GPU 制限 {} を超えています — 縮小します",
            label,
            width,
            height,
            max_dim
        );
        let scale = (max_dim as f64 / width as f64).min(max_dim as f64 / height as f64);
        let new_w = ((width as f64 * scale) as u32).max(1);
        let new_h = ((height as f64 * scale) as u32).max(1);
        let src =
            image::RgbaImage::from_raw(width, height, rgba.to_vec()).expect("RgbaImage 構築失敗");
        let resized =
            image::imageops::resize(&src, new_w, new_h, image::imageops::FilterType::Triangle);
        (Some(resized.into_raw()), new_w, new_h)
    } else {
        (None, width, height)
    };
    let upload_rgba: &[u8] = upload_owned.as_deref().unwrap_or(rgba);

    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label,
        size: wgpu::Extent3d {
            width: upload_w,
            height: upload_h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
    });

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        upload_rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * upload_w),
            rows_per_image: None,
        },
        wgpu::Extent3d {
            width: upload_w,
            height: upload_h,
            depth_or_array_layers: 1,
        },
    );

    let srgb_view = tex.create_view(&Default::default());
    let unorm_view = tex.create_view(&wgpu::TextureViewDescriptor {
        format: Some(wgpu::TextureFormat::Rgba8Unorm),
        ..Default::default()
    });
    (srgb_view, unorm_view)
}

/// IrModel のテクスチャを GPU にアップロード
/// 戻り値: テクスチャインデックス → (sRGB TextureView, Unorm TextureView) のマッピング
pub fn upload_textures(
    _ir: &IrModel,
    images: &[gltf::image::Data],
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<Vec<(wgpu::TextureView, wgpu::TextureView)>> {
    let mut views = Vec::with_capacity(images.len());

    for (i, img) in images.iter().enumerate() {
        let (width, height) = (img.width, img.height);

        // RGBA8 に変換
        let rgba_data = match img.format {
            gltf::image::Format::R8G8B8A8 => {
                // RGBA8 はそのまま — clone せず参照で直接アップロード
                let label = format!("texture_{i}");
                views.push(upload_rgba_to_gpu(
                    device,
                    queue,
                    &img.pixels,
                    width,
                    height,
                    Some(&label),
                ));
                continue;
            }
            gltf::image::Format::R8G8B8 => {
                let mut rgba = Vec::with_capacity(img.pixels.len() / 3 * 4);
                for chunk in img.pixels.chunks(3) {
                    rgba.push(chunk[0]);
                    rgba.push(chunk[1]);
                    rgba.push(chunk[2]);
                    rgba.push(255);
                }
                rgba
            }
            gltf::image::Format::R8 => {
                let mut rgba = Vec::with_capacity(img.pixels.len() * 4);
                for &p in &img.pixels {
                    rgba.push(p);
                    rgba.push(p);
                    rgba.push(p);
                    rgba.push(255);
                }
                rgba
            }
            gltf::image::Format::R8G8 => {
                let mut rgba = Vec::with_capacity(img.pixels.len() / 2 * 4);
                for chunk in img.pixels.chunks(2) {
                    rgba.push(chunk[0]);
                    rgba.push(chunk[1]);
                    rgba.push(0);
                    rgba.push(255);
                }
                rgba
            }
            _ => {
                log::warn!(
                    "未対応テクスチャフォーマット: {:?} (index {})",
                    img.format,
                    i
                );
                // 1x1 マゼンタ
                vec![255, 0, 255, 255]
            }
        };

        let (actual_w, actual_h) = if rgba_data.len() == 4 && (width != 1 || height != 1) {
            (1u32, 1u32)
        } else {
            (width, height)
        };

        let label = format!("texture_{i}");
        views.push(upload_rgba_to_gpu(
            device,
            queue,
            &rgba_data,
            actual_w,
            actual_h,
            Some(&label),
        ));
    }

    Ok(views)
}

/// バイト列から RGBA にデコード（PSD 対応）
pub fn decode_image_to_rgba(data: &[u8], is_psd: bool) -> Result<(Vec<u8>, u32, u32)> {
    decode_image_to_rgba_with_hint(data, is_psd, None)
}

pub fn decode_image_to_rgba_with_hint(
    data: &[u8],
    is_psd: bool,
    mime_hint: Option<&str>,
) -> Result<(Vec<u8>, u32, u32)> {
    if is_psd {
        return decode_psd(data);
    }

    // MIME ヒントからフォーマットを明示指定（TGA 等はマジックナンバーがなく自動判定が失敗しうる）
    let format = match mime_hint {
        Some("image/tga") | Some("image/x-tga") => Some(image::ImageFormat::Tga),
        Some("image/bmp") => Some(image::ImageFormat::Bmp),
        Some("image/png") => Some(image::ImageFormat::Png),
        Some("image/jpeg") => Some(image::ImageFormat::Jpeg),
        _ => None,
    };

    let img = if let Some(fmt) = format {
        image::load_from_memory_with_format(data, fmt)
            .or_else(|_| image::load_from_memory(data))
            .map_err(|e| anyhow::anyhow!("画像デコード失敗: {}", e))?
    } else {
        image::load_from_memory(data).map_err(|e| anyhow::anyhow!("画像デコード失敗: {}", e))?
    };

    let img = img.to_rgba8();
    let (w, h) = (img.width(), img.height());
    Ok((img.into_raw(), w, h))
}

/// バイト列からサムネイル RGBA を生成（デコード→縮小）
pub fn create_thumbnail_rgba(data: &[u8], is_psd: bool, thumb_size: u32) -> Result<Vec<u8>> {
    let (rgba, w, h) = decode_image_to_rgba(data, is_psd)?;
    let img = image::RgbaImage::from_raw(w, h, rgba)
        .ok_or_else(|| anyhow::anyhow!("RgbaImage構築失敗"))?;
    let resized = image::imageops::resize(
        &img,
        thumb_size,
        thumb_size,
        image::imageops::FilterType::Triangle,
    );
    Ok(resized.into_raw())
}

/// バイト列から RGBA にデコードして GPU テクスチャをアップロード（PSD 対応）
/// 戻り値: (sRGB ビュー, Unorm ビュー)
pub fn upload_texture_from_bytes(
    data: &[u8],
    is_psd: bool,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<(wgpu::TextureView, wgpu::TextureView)> {
    let (rgba_data, width, height) = decode_image_to_rgba(data, is_psd)?;
    Ok(upload_rgba_to_gpu(
        device,
        queue,
        &rgba_data,
        width,
        height,
        Some("assigned_texture"),
    ))
}

/// IrTexture（PNG/JPEG データ）から GPU テクスチャをアップロード
/// 戻り値: テクスチャインデックス → (sRGB TextureView, Unorm TextureView) のマッピング
pub fn upload_textures_from_ir(
    ir: &IrModel,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<Vec<(wgpu::TextureView, wgpu::TextureView)>> {
    let mut views = Vec::with_capacity(ir.textures.len());

    for (i, tex) in ir.textures.iter().enumerate() {
        let is_psd = is_psd_filename(&tex.filename);
        if tex.data.is_empty() {
            log::warn!("テクスチャ '{}' のデータが空 (index {})", tex.filename, i);
            views.push(upload_rgba_to_gpu(
                device,
                queue,
                &[255, 0, 255, 255],
                1,
                1,
                Some(&format!("texture_{i}")),
            ));
            continue;
        }
        let decoded = match decode_image_to_rgba_with_hint(&tex.data, is_psd, Some(&tex.mime_type))
        {
            Ok(d) => d,
            Err(e) => {
                log::warn!(
                    "テクスチャ '{}' のデコード失敗: {} (index {}, {} bytes)",
                    tex.filename,
                    e,
                    i,
                    tex.data.len()
                );
                (vec![255, 0, 255, 255], 1, 1)
            }
        };

        let (rgba_data, width, height) = decoded;
        let label = format!("texture_{i}");
        views.push(upload_rgba_to_gpu(
            device,
            queue,
            &rgba_data,
            width,
            height,
            Some(&label),
        ));
    }

    Ok(views)
}

// decode_psd, is_psd_filename は上部で crate::psd から re-export 済み
