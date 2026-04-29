//! PSD (Photoshop) ファイルのデコード処理
//!
//! feature gate なしで常にコンパイル可能。
//! convert/texture.rs (CLI) と viewer/texture.rs (ビューア) の両方から利用される。

use anyhow::Result;
use rust_i18n::t;

/// ファイル名が PSD かどうか判定
#[inline]
pub fn is_psd_filename(name: &str) -> bool {
    name.len() >= 4 && name.as_bytes()[name.len() - 4..].eq_ignore_ascii_case(b".psd")
}

/// PSD ファイルを RGBA バイト列にデコード
///
/// 戻り値: (rgba, width, height)
pub fn decode_psd(data: &[u8]) -> Result<(Vec<u8>, u32, u32)> {
    // --- ファイルヘッダ (26 bytes) ---
    if data.len() < 26 {
        anyhow::bail!(
            "{}",
            t!("error.psd.too_short", size = data.len().to_string())
        );
    }
    if &data[0..4] != b"8BPS" {
        anyhow::bail!("{}", t!("error.psd.invalid_signature"));
    }
    let version = u16::from_be_bytes([data[4], data[5]]);
    if version != 1 {
        anyhow::bail!(
            "{}",
            t!(
                "error.psd.unsupported_version",
                version = version.to_string()
            )
        );
    }
    let channel_count = u16::from_be_bytes([data[12], data[13]]) as usize;
    let height = u32::from_be_bytes([data[14], data[15], data[16], data[17]]);
    let width = u32::from_be_bytes([data[18], data[19], data[20], data[21]]);
    let depth = u16::from_be_bytes([data[22], data[23]]);
    // color_mode: data[24..26] (未使用)

    if depth != 8 && depth != 16 {
        anyhow::bail!(
            "{}",
            t!("error.psd.unsupported_depth", depth = depth.to_string())
        );
    }

    // --- 可変長セクションをスキップしてイメージデータセクションへ ---
    let mut pos: usize = 26;

    // Color Mode Data section (4 bytes length + data)
    if pos + 4 > data.len() {
        anyhow::bail!("{}", t!("error.psd.invalid_color_mode_section"));
    }
    let section_len =
        u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
    pos += 4 + section_len;

    // Image Resources section (4 bytes length + data)
    if pos + 4 > data.len() {
        anyhow::bail!("{}", t!("error.psd.invalid_image_resources_section"));
    }
    let section_len =
        u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
    pos += 4 + section_len;

    // Layer and Mask Information section (4 bytes length + data) -- skipped
    if pos + 4 > data.len() {
        anyhow::bail!("{}", t!("error.psd.invalid_layer_mask_section"));
    }
    let section_len =
        u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
    pos += 4 + section_len;

    // --- Image Data section ---
    if pos + 2 > data.len() {
        anyhow::bail!("{}", t!("error.psd.invalid_image_data_section"));
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

/// PSD データを PNG バイト列に変換
pub fn psd_to_png(psd_data: &[u8]) -> Result<Vec<u8>> {
    let (rgba, width, height) = decode_psd(psd_data)?;

    let mut png_data = Vec::new();
    {
        let encoder = image::codecs::png::PngEncoder::new(&mut png_data);
        use image::ImageEncoder;
        encoder
            .write_image(&rgba, width, height, image::ExtendedColorType::Rgba8)
            .map_err(|e| {
                anyhow::anyhow!(
                    "{}",
                    t!("error.psd.png_encode_failed", detail = e.to_string())
                )
            })?;
    }
    Ok(png_data)
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
                    anyhow::bail!(
                        "{}",
                        t!("error.psd.raw_channel_short", channel = ch.to_string())
                    );
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
                anyhow::bail!("{}", t!("error.psd.rle_scanline_header_short"));
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
                    anyhow::bail!(
                        "{}",
                        t!("error.psd.rle_channel_short", channel = ch.to_string())
                    );
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
            "{}",
            t!(
                "error.psd.unsupported_compression",
                mode = compression.to_string()
            )
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
