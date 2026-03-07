use anyhow::Result;
use eframe::wgpu;
use crate::intermediate::types::IrModel;

/// RGBA データを GPU テクスチャにアップロード（共通処理）
pub fn upload_rgba_to_gpu(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    rgba: &[u8],
    width: u32,
    height: u32,
    label: Option<&str>,
) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label,
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: None,
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    tex.create_view(&Default::default())
}

/// IrModel のテクスチャを GPU にアップロード
/// 戻り値: テクスチャインデックス → TextureView のマッピング
pub fn upload_textures(
    _ir: &IrModel,
    images: &[gltf::image::Data],
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<Vec<wgpu::TextureView>> {
    let mut views = Vec::with_capacity(images.len());

    for (i, img) in images.iter().enumerate() {
        let (width, height) = (img.width, img.height);

        // RGBA8 に変換
        let rgba_data = match img.format {
            gltf::image::Format::R8G8B8A8 => img.pixels.clone(),
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
                log::warn!("未対応テクスチャフォーマット: {:?} (index {})", img.format, i);
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
        views.push(upload_rgba_to_gpu(device, queue, &rgba_data, actual_w, actual_h, Some(&label)));
    }

    Ok(views)
}

/// バイト列から RGBA にデコードして GPU テクスチャをアップロード（PSD 対応）
pub fn upload_texture_from_bytes(
    data: &[u8],
    is_psd: bool,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<wgpu::TextureView> {
    let (rgba_data, width, height) = if is_psd {
        decode_psd(data)?
    } else {
        let img = image::load_from_memory(data)
            .map_err(|e| anyhow::anyhow!("画像デコード失敗: {}", e))?
            .to_rgba8();
        let (w, h) = (img.width(), img.height());
        (img.into_raw(), w, h)
    };

    Ok(upload_rgba_to_gpu(device, queue, &rgba_data, width, height, Some("assigned_texture")))
}

/// IrTexture（PNG/JPEG データ）から GPU テクスチャをアップロード
pub fn upload_textures_from_ir(
    ir: &IrModel,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<Vec<wgpu::TextureView>> {
    let mut views = Vec::with_capacity(ir.textures.len());

    for (i, tex) in ir.textures.iter().enumerate() {
        let img = match image::load_from_memory(&tex.data) {
            Ok(img) => img.to_rgba8(),
            Err(e) => {
                log::warn!("テクスチャ '{}' のデコード失敗: {} (index {})", tex.filename, e, i);
                image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 255, 255]))
            }
        };

        let (width, height) = (img.width(), img.height());
        let rgba_data = img.into_raw();

        let label = format!("texture_{i}");
        views.push(upload_rgba_to_gpu(device, queue, &rgba_data, width, height, Some(&label)));
    }

    Ok(views)
}

/// PSD ファイルを RGBA にデコード（結合済み画像を取得）
pub fn decode_psd(data: &[u8]) -> Result<(Vec<u8>, u32, u32)> {
    let psd = psd::Psd::from_bytes(data)
        .map_err(|e| anyhow::anyhow!("PSD デコード失敗: {:?}", e))?;
    let width = psd.width();
    let height = psd.height();
    let rgba = psd.rgba();
    Ok((rgba, width, height))
}
