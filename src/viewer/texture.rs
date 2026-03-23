use crate::intermediate::types::IrModel;
use anyhow::Result;
use eframe::wgpu;

/// ファイル名が PSD かどうか判定
#[inline]
pub fn is_psd_filename(name: &str) -> bool {
    name.len() >= 4 && name.as_bytes()[name.len() - 4..].eq_ignore_ascii_case(b".psd")
}

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

/// PSD ファイルを RGBA にデコード（統合画像のみ取得、レイヤーセクションをスキップ）
///
/// psd crate はレイヤーセクションのパースでパニックする場合があるため、
/// ファイルヘッダと画像データセクションのみを自前でパースする。
pub fn decode_psd(data: &[u8]) -> Result<(Vec<u8>, u32, u32)> {
    // --- ファイルヘッダ (26 bytes) ---
    if data.len() < 26 {
        anyhow::bail!("PSD ファイルが短すぎます ({} bytes)", data.len());
    }
    if &data[0..4] != b"8BPS" {
        anyhow::bail!("PSD シグネチャが不正です");
    }
    let version = u16::from_be_bytes([data[4], data[5]]);
    if version != 1 {
        anyhow::bail!("PSD バージョン {} は未対応です (v1 のみ対応)", version);
    }
    let channel_count = u16::from_be_bytes([data[12], data[13]]) as usize;
    let height = u32::from_be_bytes([data[14], data[15], data[16], data[17]]);
    let width = u32::from_be_bytes([data[18], data[19], data[20], data[21]]);
    let depth = u16::from_be_bytes([data[22], data[23]]);
    // color_mode: data[24..26] (未使用)

    if depth != 8 && depth != 16 {
        anyhow::bail!("PSD ビット深度 {} は未対応です (8/16 のみ対応)", depth);
    }

    // --- 可変長セクションをスキップしてイメージデータセクションへ ---
    let mut pos: usize = 26;

    // Color Mode Data セクション (4 bytes length + data)
    if pos + 4 > data.len() {
        anyhow::bail!("PSD: Color Mode Data セクションが不正");
    }
    let section_len =
        u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
    pos += 4 + section_len;

    // Image Resources セクション (4 bytes length + data)
    if pos + 4 > data.len() {
        anyhow::bail!("PSD: Image Resources セクションが不正");
    }
    let section_len =
        u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
    pos += 4 + section_len;

    // Layer and Mask Information セクション (4 bytes length + data) — スキップ!
    if pos + 4 > data.len() {
        anyhow::bail!("PSD: Layer and Mask セクションが不正");
    }
    let section_len =
        u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
    pos += 4 + section_len;

    // --- Image Data セクション ---
    if pos + 2 > data.len() {
        anyhow::bail!("PSD: Image Data セクションが不正");
    }
    let compression = u16::from_be_bytes([data[pos], data[pos + 1]]);
    pos += 2;

    let image_bytes = &data[pos..];
    let pixel_count = (width * height) as usize;

    // チャンネル別バイト列をデコード
    let channels = decode_psd_image_channels(
        image_bytes,
        compression,
        channel_count,
        height as usize,
        pixel_count,
        depth,
    )?;

    // RGBA に組み立て
    let mut rgba = vec![0u8; pixel_count * 4];
    let ch_count = channels.len().min(4);
    for (ch, ch_data) in channels.iter().enumerate().take(ch_count) {
        let offset = if ch < 3 { ch } else { 3 }; // R=0, G=1, B=2, A=3
        for i in 0..pixel_count {
            if i < ch_data.len() {
                rgba[i * 4 + offset] = ch_data[i];
            }
        }
    }
    // グレースケール: R のみの場合 G,B にコピー
    if ch_count == 1 {
        for i in 0..pixel_count {
            rgba[i * 4 + 1] = rgba[i * 4];
            rgba[i * 4 + 2] = rgba[i * 4];
        }
    }
    // アルファチャンネルがない場合は不透明
    if ch_count <= 3 {
        for i in 0..pixel_count {
            rgba[i * 4 + 3] = 255;
        }
    }

    Ok((rgba, width, height))
}

/// PSD 画像データセクションのチャンネルをデコード
fn decode_psd_image_channels(
    data: &[u8],
    compression: u16,
    channel_count: usize,
    height: usize,
    pixel_count: usize,
    depth: u16,
) -> Result<Vec<Vec<u8>>> {
    match compression {
        0 => {
            // Raw データ
            let bytes_per_pixel = if depth == 16 { 2 } else { 1 };
            let channel_byte_count = pixel_count * bytes_per_pixel;
            let mut channels = Vec::with_capacity(channel_count);
            for ch in 0..channel_count {
                let start = ch * channel_byte_count;
                let end = start + channel_byte_count;
                if end > data.len() {
                    anyhow::bail!("PSD Raw: チャンネル {} のデータが不足", ch);
                }
                let ch_data = if depth == 16 {
                    // 16bit → 8bit に変換
                    data[start..end]
                        .chunks(2)
                        .map(|pair| (u16::from_be_bytes([pair[0], pair[1]]) / 256) as u8)
                        .collect()
                } else {
                    data[start..end].to_vec()
                };
                channels.push(ch_data);
            }
            Ok(channels)
        }
        1 => {
            // RLE 圧縮: 各スキャンライン長が先頭に格納
            let scanline_counts = channel_count * height;
            let header_bytes = scanline_counts * 2;
            if data.len() < header_bytes {
                anyhow::bail!("PSD RLE: スキャンラインヘッダが不足");
            }

            // 各チャンネルのバイト数を集計
            let mut ch_byte_counts = vec![0usize; channel_count];
            for (ch, count) in ch_byte_counts.iter_mut().enumerate() {
                for row in 0..height {
                    let idx = (ch * height + row) * 2;
                    *count += u16::from_be_bytes([data[idx], data[idx + 1]]) as usize;
                }
            }

            // チャンネルデータをデコード
            let mut offset = header_bytes;
            let mut channels = Vec::with_capacity(channel_count);
            for (ch, &ch_bytes) in ch_byte_counts.iter().enumerate() {
                let end = offset + ch_bytes;
                if end > data.len() {
                    anyhow::bail!("PSD RLE: チャンネル {} のデータが不足", ch);
                }
                let decompressed = packbits_decompress(&data[offset..end]);
                let ch_data = if depth == 16 {
                    decompressed
                        .chunks(2)
                        .map(|pair| {
                            if pair.len() == 2 {
                                (u16::from_be_bytes([pair[0], pair[1]]) / 256) as u8
                            } else {
                                0
                            }
                        })
                        .collect()
                } else {
                    decompressed
                };
                channels.push(ch_data);
                offset = end;
            }
            Ok(channels)
        }
        _ => anyhow::bail!(
            "PSD 圧縮方式 {} は未対応です (Raw/RLE のみ対応)",
            compression
        ),
    }
}

/// PackBits (RLE) デコード
fn packbits_decompress(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let header = data[i] as i8;
        i += 1;
        if header == -128 {
            // nop
        } else if header >= 0 {
            let count = header as usize + 1;
            let end = (i + count).min(data.len());
            result.extend_from_slice(&data[i..end]);
            i = end;
        } else {
            let count = (1 - header as i16) as usize;
            if i < data.len() {
                let byte = data[i];
                i += 1;
                result.extend(std::iter::repeat_n(byte, count));
            }
        }
    }
    result
}
