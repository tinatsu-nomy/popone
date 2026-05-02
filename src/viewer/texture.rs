use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::color::{linear_f32_to_rgba8, rgba8_to_linear_f32};
use crate::intermediate::types::IrModel;
use anyhow::{Context, Result};
use eframe::wgpu;
use rust_i18n::t;

/// テクスチャデコード失敗・参照先不在時のフォールバック色を白にするか。
/// - true（既定）: 1×1 白 (255,255,255,255) — 乗算/加算系スロットで色被りしない
/// - false: 1×1 マゼンタ (255,0,255,255) — 欠落を目立たせたい診断用
///
/// 値は単独で意味を持つ（デコード前に参照される）。既にアップロード済みの
/// GPU 上の色は `SharedFallback` 側の `queue.write_texture` によって同期する
/// （`set_white_texture_fallback_dynamic` を参照）。
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

/// 失敗経路すべてで共有する 1×1 フォールバックテクスチャ。
///
/// 個別に Texture を作らず、同一 `TextureView` を全材質の BindGroup に焼き込む
/// ことで、色切替時に `queue.write_texture` で中身 1 バイトを書き換えるだけで
/// 既に描画中のモデルにも即時反映できる（BindGroup 再構築不要）。
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

/// 共有フォールバックテクスチャを（必要なら初期化したうえで）取得する。
///
/// テクスチャデコードに失敗した経路から呼ばれ、全呼び出し元で同一の
/// `TextureView` ペアを共有する。
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

/// 色を切り替え、既に初期化済みなら GPU 上の 1×1 に `queue.write_texture` で
/// 新色を書き込む（View は不変なので BindGroup 再構築不要）。
///
/// 未初期化の場合は何もしない — 次回 `fallback_views` 呼び出し時に現行色で
/// 初期化される。
pub fn set_white_texture_fallback_dynamic(enabled: bool, queue: &wgpu::Queue) {
    WHITE_FALLBACK.store(enabled, Ordering::Relaxed);
    let guard = SHARED_FALLBACK.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(ft) = guard.as_ref() {
        ft.write_current_color(queue);
    }
}

/// sRGB RGBA バイト列を linear 空間で縮小し、sRGB に戻して返す
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
    upload_rgba_to_gpu_with_mips(device, queue, rgba, width, height, label, None)
}

/// 事前生成されたミップチェーンを受け取る版。
/// `mip_chain` が Some なら CPU ミップ生成をスキップし、直接 GPU アップロードする。
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
        let src =
            image::RgbaImage::from_raw(width, height, rgba.to_vec()).expect("RgbaImage 構築失敗");
        let resized = resize_srgb(&src, new_w, new_h, image::imageops::FilterType::Triangle);
        (Some(resized.into_raw()), new_w, new_h)
    } else {
        (None, width, height)
    };
    let upload_rgba: &[u8] = upload_owned.as_deref().unwrap_or(rgba);

    // ミップレベル数を計算: floor(log2(max(w,h))) + 1
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

    // レベル 0 をアップロード
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

    // ミップチェーンをアップロード
    // 事前生成済み（BGスレッドで生成）があればそれを使用、なければCPUで生成
    if mip_level_count > 1 {
        // リサイズ後の場合（GPU 上限超過）は事前生成ミップを無視して新規生成
        let use_prebuilt = mip_chain.is_some() && upload_owned.is_none();
        if use_prebuilt {
            let chain = mip_chain.expect("use_prebuilt で確認済み");
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
            // sRGB→linear→縮小→sRGB で色空間的に正確なダウンサンプリング
            // (LUT で powf 呼び出しを排除済み)
            let base = image::RgbaImage::from_raw(upload_w, upload_h, upload_rgba.to_vec())
                .expect("ミップ生成用 RgbaImage 構築失敗");
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
                log::warn!("Unsupported texture format: {:?} (index {})", img.format, i);
                // 共有フォールバックに切替 — 動的に白/マゼンタへ書き換えられる
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

/// バイト列からサムネイル RGBA を生成（デコード→縮小）
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
    for i in 0..ir.textures.len() {
        views.push(upload_single_texture(&ir.textures[i], i, device, queue));
    }
    Ok(views)
}

/// 1 枚のテクスチャを GPU にアップロードする（フレーム分割用に公開）
pub fn upload_single_texture(
    tex: &crate::intermediate::types::IrTexture,
    index: usize,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> (wgpu::TextureView, wgpu::TextureView) {
    let is_psd = is_psd_filename(&tex.filename);
    if tex.data.is_empty() {
        log::warn!("Texture '{}' data is empty (index {})", tex.filename, index);
        // 共有フォールバックに切替 — 動的に白/マゼンタへ書き換えられる
        return fallback_views(device, queue);
    }
    // 生 RGBA バイパス（BG ロードパスで事前デコード済み）
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

// decode_psd, is_psd_filename は上部で crate::psd から re-export 済み
