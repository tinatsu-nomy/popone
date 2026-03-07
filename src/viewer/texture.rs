use anyhow::Result;
use eframe::wgpu;
use crate::intermediate::types::IrModel;

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

        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("texture_{i}")),
            size: wgpu::Extent3d {
                width: actual_w,
                height: actual_h,
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
            &rgba_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * actual_w),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: actual_w,
                height: actual_h,
                depth_or_array_layers: 1,
            },
        );

        views.push(tex.create_view(&Default::default()));
    }

    Ok(views)
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

        let gpu_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("texture_{i}")),
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
                texture: &gpu_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba_data,
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

        views.push(gpu_tex.create_view(&Default::default()));
    }

    Ok(views)
}
