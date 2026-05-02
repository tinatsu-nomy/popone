use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::color::{linear_f32_to_rgba8, rgba8_to_linear_f32};
use crate::intermediate::types::IrModel;
use anyhow::{Context, Result};
use eframe::wgpu;
use rust_i18n::t;

/// Whether the fallback color (used when texture decode fails or the referenced
/// texture is missing) is white.
/// - true (default): 1x1 white (255,255,255,255) — avoids tinting in multiply / additive slots.
/// - false: 1x1 magenta (255,0,255,255) — diagnostic mode that makes missing textures stand out.
///
/// The value is meaningful on its own (consulted before decoding). Already
/// uploaded GPU colors are kept in sync via `queue.write_texture` from the
/// `SharedFallback` side (see `set_white_texture_fallback_dynamic`).
static WHITE_FALLBACK: AtomicBool = AtomicBool::new(true);

pub fn set_white_texture_fallback(enabled: bool) {
    WHITE_FALLBACK.store(enabled, Ordering::Relaxed);
}

pub fn white_texture_fallback() -> bool {
    WHITE_FALLBACK.load(Ordering::Relaxed)
}

#[inline]
fn fallback_rgba() -> [u8; 4] {
    if WHITE_FALLBACK.load(Ordering::Relaxed) {
        [255, 255, 255, 255]
    } else {
        [255, 0, 255, 255]
    }
}

/// 1x1 fallback texture shared across every failure path.
///
/// Rather than creating a Texture per failure, the same `TextureView` is baked
/// into every material's BindGroup. When the color is toggled, a single
/// `queue.write_texture` (1 byte) instantly updates every model already being
/// rendered (no BindGroup rebuild needed).
struct SharedFallback {
    tex: wgpu::Texture,
    srgb_view: wgpu::TextureView,
    unorm_view: wgpu::TextureView,
}

impl SharedFallback {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fallback_shared_1x1"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
        });
        let srgb_view = tex.create_view(&Default::default());
        let unorm_view = tex.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Rgba8Unorm),
            ..Default::default()
        });
        let me = Self {
            tex,
            srgb_view,
            unorm_view,
        };
        me.write_current_color(queue);
        me
    }

    fn write_current_color(&self, queue: &wgpu::Queue) {
        let rgba = fallback_rgba();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
    }

    fn views(&self) -> (wgpu::TextureView, wgpu::TextureView) {
        (self.srgb_view.clone(), self.unorm_view.clone())
    }
}

static SHARED_FALLBACK: Mutex<Option<SharedFallback>> = Mutex::new(None);

/// Return the shared fallback texture, initializing it on first use.
///
/// Called from any path where texture decoding fails. Every caller shares the
/// same `TextureView` pair.
fn fallback_views(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> (wgpu::TextureView, wgpu::TextureView) {
    let mut guard = SHARED_FALLBACK.lock().unwrap_or_else(|p| p.into_inner());
    if guard.is_none() {
        *guard = Some(SharedFallback::new(device, queue));
    }
    guard
        .as_ref()
        .expect("SharedFallback must be Some after init")
        .views()
}

/// Toggle the color and, if already initialized, write the new color to the GPU
/// 1x1 via `queue.write_texture` (the View is unchanged so BindGroups need not
/// be rebuilt).
///
/// If not yet initialized this is a no-op — the next `fallback_views` call will
/// initialize with the current color.
pub fn set_white_texture_fallback_dynamic(enabled: bool, queue: &wgpu::Queue) {
    WHITE_FALLBACK.store(enabled, Ordering::Relaxed);
    let guard = SHARED_FALLBACK.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(ft) = guard.as_ref() {
        ft.write_current_color(queue);
    }
}

/// Resize an sRGB RGBA byte buffer in linear space and return it back in sRGB.
fn resize_srgb(
    src: &image::RgbaImage,
    new_w: u32,
    new_h: u32,
    filter: image::imageops::FilterType,
) -> image::RgbaImage {
    let linear = rgba8_to_linear_f32(src);
    let resized = image::imageops::resize(&linear, new_w, new_h, filter);
    linear_f32_to_rgba8(&resized)
}

// PSD-related functions moved to crate::psd — re-exported for backward compatibility.
pub use crate::psd::{decode_psd, is_psd_filename};

/// Upload an RGBA buffer to a GPU texture (shared helper).
/// Automatically downscales when the GPU max texture size is exceeded.
/// Returns: (sRGB view, Unorm view) — sRGB for standard rendering, Unorm for MMD rendering.
pub fn upload_rgba_to_gpu(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    rgba: &[u8],
    width: u32,
    height: u32,
    label: Option<&str>,
) -> (wgpu::TextureView, wgpu::TextureView) {
    upload_rgba_to_gpu_with_mips(device, queue, rgba, width, height, label, None)
}

/// Variant that accepts a pre-built mip chain.
/// When `mip_chain` is Some, CPU mip generation is skipped and the chain is uploaded directly.
#[allow(clippy::type_complexity)]
pub fn upload_rgba_to_gpu_with_mips(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    rgba: &[u8],
    width: u32,
    height: u32,
    label: Option<&str>,
    mip_chain: Option<&[(u32, u32, Arc<[u8]>)]>,
) -> (wgpu::TextureView, wgpu::TextureView) {
    let max_dim = device.limits().max_texture_dimension_2d;
    let (upload_owned, upload_w, upload_h) = if width > max_dim || height > max_dim {
        log::warn!(
            "Texture {:?} ({}x{}) exceeds GPU limit {} - downscaling",
            label,
            width,
            height,
            max_dim
        );
        let scale = (max_dim as f64 / width as f64).min(max_dim as f64 / height as f64);
        let new_w = ((width as f64 * scale) as u32).max(1);
        let new_h = ((height as f64 * scale) as u32).max(1);
        let src = image::RgbaImage::from_raw(width, height, rgba.to_vec())
            .expect("RgbaImage construction failed");
        let resized = resize_srgb(&src, new_w, new_h, image::imageops::FilterType::Triangle);
        (Some(resized.into_raw()), new_w, new_h)
    } else {
        (None, width, height)
    };
    let upload_rgba: &[u8] = upload_owned.as_deref().unwrap_or(rgba);

    // Mip level count: floor(log2(max(w, h))) + 1
    let mip_level_count = {
        let max_side = upload_w.max(upload_h);
        if max_side <= 1 {
            1
        } else {
            32 - max_side.leading_zeros()
        }
    };

    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label,
        size: wgpu::Extent3d {
            width: upload_w,
            height: upload_h,
            depth_or_array_layers: 1,
        },
        mip_level_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
    });

    // Upload level 0.
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

    // Upload the mip chain.
    // Use the pre-built chain (built on the BG thread) when available, otherwise generate on CPU.
    if mip_level_count > 1 {
        // After resizing (because of GPU limit) we ignore any pre-built mip chain and regenerate.
        let use_prebuilt = mip_chain.is_some() && upload_owned.is_none();
        if use_prebuilt {
            let chain = mip_chain.expect("verified by use_prebuilt");
            for (level_idx, (mip_w, mip_h, mip_data)) in chain.iter().enumerate() {
                let level = (level_idx + 1) as u32;
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &tex,
                        mip_level: level,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    mip_data,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * mip_w),
                        rows_per_image: None,
                    },
                    wgpu::Extent3d {
                        width: *mip_w,
                        height: *mip_h,
                        depth_or_array_layers: 1,
                    },
                );
            }
        } else {
            // sRGB -> linear -> downscale -> sRGB for color-correct downsampling
            // (powf calls are eliminated by a LUT).
            let base = image::RgbaImage::from_raw(upload_w, upload_h, upload_rgba.to_vec())
                .expect("RgbaImage construction for mip generation failed");
            let mut current_linear = rgba8_to_linear_f32(&base);
            for level in 1..mip_level_count {
                let mip_w = (upload_w >> level).max(1);
                let mip_h = (upload_h >> level).max(1);
                current_linear = image::imageops::resize(
                    &current_linear,
                    mip_w,
                    mip_h,
                    image::imageops::FilterType::Triangle,
                );
                let mip_srgb = linear_f32_to_rgba8(&current_linear);
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &tex,
                        mip_level: level,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    mip_srgb.as_raw(),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * mip_w),
                        rows_per_image: None,
                    },
                    wgpu::Extent3d {
                        width: mip_w,
                        height: mip_h,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
    }

    let srgb_view = tex.create_view(&Default::default());
    let unorm_view = tex.create_view(&wgpu::TextureViewDescriptor {
        format: Some(wgpu::TextureFormat::Rgba8Unorm),
        ..Default::default()
    });
    (srgb_view, unorm_view)
}

/// Upload IrModel textures to the GPU.
/// Returns: mapping from texture index to (sRGB TextureView, Unorm TextureView).
pub fn upload_textures(
    _ir: &IrModel,
    images: &[gltf::image::Data],
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<Vec<(wgpu::TextureView, wgpu::TextureView)>> {
    let mut views = Vec::with_capacity(images.len());

    for (i, img) in images.iter().enumerate() {
        let (width, height) = (img.width, img.height);

        // Convert to RGBA8.
        let rgba_data = match img.format {
            gltf::image::Format::R8G8B8A8 => {
                // RGBA8 needs no conversion — upload by reference without cloning.
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
                log::warn!("Unsupported texture format: {:?} (index {})", img.format, i);
                // Switch to the shared fallback — can be flipped between white and magenta dynamically.
                views.push(fallback_views(device, queue));
                continue;
            }
        };

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

/// Decode a byte slice into RGBA (PSD supported).
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

    // Pin the format from the MIME hint when available (TGA etc. lack a magic number, so auto-detect can fail).
    let format = match mime_hint {
        Some("image/tga") | Some("image/x-tga") => Some(image::ImageFormat::Tga),
        Some("image/bmp") => Some(image::ImageFormat::Bmp),
        Some("image/png") => Some(image::ImageFormat::Png),
        Some("image/jpeg") => Some(image::ImageFormat::Jpeg),
        Some("image/vnd.ms-dds") | Some("image/x-dds") | Some("image/dds") => {
            Some(image::ImageFormat::Dds)
        }
        _ => None,
    };

    let img = if let Some(fmt) = format {
        image::load_from_memory_with_format(data, fmt)
            .or_else(|_| image::load_from_memory(data))
            .map_err(|e| {
                anyhow::anyhow!(
                    t!("error.image_decode_failed", detail = format!("{e}")).into_owned()
                )
            })?
    } else {
        image::load_from_memory(data).map_err(|e| {
            anyhow::anyhow!(t!("error.image_decode_failed", detail = format!("{e}")).into_owned())
        })?
    };

    let img = img.to_rgba8();
    let (w, h) = (img.width(), img.height());
    Ok((img.into_raw(), w, h))
}

/// Generate a thumbnail RGBA from a byte slice (decode -> downscale).
pub fn create_thumbnail_rgba(data: &[u8], is_psd: bool, thumb_size: u32) -> Result<Vec<u8>> {
    let (rgba, w, h) = decode_image_to_rgba(data, is_psd)?;
    let img = image::RgbaImage::from_raw(w, h, rgba).context("RgbaImage構築失敗")?;
    let resized = image::imageops::resize(
        &img,
        thumb_size,
        thumb_size,
        image::imageops::FilterType::Triangle,
    );
    Ok(resized.into_raw())
}

/// Decode a byte slice into RGBA and upload it to a GPU texture (PSD supported).
/// Returns: (sRGB view, Unorm view).
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

/// Upload IrTexture (PNG/JPEG bytes) to GPU textures.
/// Returns: mapping from texture index to (sRGB TextureView, Unorm TextureView).
pub fn upload_textures_from_ir(
    ir: &IrModel,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<Vec<(wgpu::TextureView, wgpu::TextureView)>> {
    let mut views = Vec::with_capacity(ir.textures.len());
    for i in 0..ir.textures.len() {
        views.push(upload_single_texture(&ir.textures[i], i, device, queue));
    }
    Ok(views)
}

/// Upload a single texture to the GPU (exposed for frame splitting).
pub fn upload_single_texture(
    tex: &crate::intermediate::types::IrTexture,
    index: usize,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> (wgpu::TextureView, wgpu::TextureView) {
    let is_psd = is_psd_filename(&tex.filename);
    if tex.data.is_empty() {
        log::warn!("Texture '{}' data is empty (index {})", tex.filename, index);
        // Switch to the shared fallback — can be flipped between white and magenta dynamically.
        return fallback_views(device, queue);
    }
    // Raw RGBA bypass (already decoded on the BG load path).
    if let crate::intermediate::types::TextureData::RawRgba {
        ref pixels,
        width,
        height,
    } = tex.data
    {
        let label = format!("texture_{index}");
        return upload_rgba_to_gpu_with_mips(
            device,
            queue,
            pixels,
            width,
            height,
            Some(&label),
            tex.mip_chain.as_deref(),
        );
    }
    match decode_image_to_rgba_with_hint(tex.data.as_bytes(), is_psd, Some(&tex.mime_type)) {
        Ok((rgba_data, width, height)) => {
            let label = format!("texture_{index}");
            upload_rgba_to_gpu(device, queue, &rgba_data, width, height, Some(&label))
        }
        Err(e) => {
            log::warn!(
                "Texture '{}' decode failed: {} (index {}, {} bytes)",
                tex.filename,
                e,
                index,
                tex.data.len()
            );
            fallback_views(device, queue)
        }
    }
}

// decode_psd, is_psd_filename are re-exported from crate::psd above.
