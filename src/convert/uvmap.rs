//! UVマップを材質ごとにレイヤー分けした PSD ファイルとして出力する。

use std::io::{self, Write};
use std::path::Path;

use crate::intermediate::types::IrModel;

/// UVマップ出力のデフォルト解像度
pub const DEFAULT_UV_SIZE: u32 = 4096;

/// UVマップを PSD として出力する。
/// 各材質を1レイヤーとして、UV三角形ワイヤーフレームを黒線で描画する。
pub fn export_uv_map(ir: &IrModel, path: &Path, size: u32) -> io::Result<()> {
    // 材質ごとにメッシュをグループ化
    let mat_count = ir.materials.len();
    let dim = size as usize;

    // 各材質のレイヤー画像を生成（RGBA、透明背景 + 黒線）
    let mut layers: Vec<Vec<u8>> = Vec::with_capacity(mat_count);
    let mut layer_names: Vec<String> = Vec::with_capacity(mat_count);

    for mat_idx in 0..mat_count {
        let mut buf = vec![0u8; dim * dim * 4]; // RGBA 全透明

        // この材質に属するメッシュを探して UV 三角形を描画
        for mesh in &ir.meshes {
            if mesh.material_index != mat_idx {
                continue;
            }
            let verts = &mesh.vertices;
            let indices = &mesh.indices;
            // 三角形ごとに3辺を描画
            for tri in indices.chunks(3) {
                if tri.len() < 3 {
                    continue;
                }
                let raw: [(f32, f32); 3] = [
                    { let uv = verts[tri[0] as usize].uv; (fract_uv(uv.x), fract_uv(uv.y)) },
                    { let uv = verts[tri[1] as usize].uv; (fract_uv(uv.x), fract_uv(uv.y)) },
                    { let uv = verts[tri[2] as usize].uv; (fract_uv(uv.x), fract_uv(uv.y)) },
                ];

                // 境界ラップ検出
                let u_wraps = uv_wraps(raw[0].0, raw[1].0, raw[2].0);
                let v_wraps = uv_wraps(raw[0].1, raw[1].1, raw[2].1);

                // ラップする軸ごとに +0 / -1 のオフセット組み合わせで描画
                let u_offsets: &[f32] = if u_wraps { &[0.0, -1.0] } else { &[0.0] };
                let v_offsets: &[f32] = if v_wraps { &[0.0, -1.0] } else { &[0.0] };

                for &uo in u_offsets {
                    for &vo in v_offsets {
                        // ラップ時: 小さい値の頂点を +1.0 シフトしてから全体にオフセット
                        let shifted: [(f32, f32); 3] = std::array::from_fn(|i| {
                            let u = raw[i].0 + if u_wraps && raw[i].0 < 0.5 { 1.0 } else { 0.0 } + uo;
                            let v = raw[i].1 + if v_wraps && raw[i].1 < 0.5 { 1.0 } else { 0.0 } + vo;
                            (u, v)
                        });

                        let px: [(i32, i32); 3] = std::array::from_fn(|i| {
                            ((shifted[i].0 * dim as f32) as i32, (shifted[i].1 * dim as f32) as i32)
                        });

                        draw_line(&mut buf, dim, px[0], px[1]);
                        draw_line(&mut buf, dim, px[1], px[2]);
                        draw_line(&mut buf, dim, px[2], px[0]);
                    }
                }
            }
        }

        layers.push(buf);
        layer_names.push(ir.materials[mat_idx].name.clone());
    }

    // レイヤー順を逆転（材質0が最上位レイヤーになるように）
    layers.reverse();
    layer_names.reverse();

    // PSD 書き出し
    let file = std::fs::File::create(path)?;
    let mut w = io::BufWriter::new(file);
    write_psd_file(&mut w, size, size, &layers, &layer_names)?;
    w.flush()?;

    log::info!("UVマップ出力: {} ({}x{}, {}レイヤー)", path.display(), size, size, mat_count);
    Ok(())
}

/// UV値を 0..1 に正規化（負値対応の fract）
#[inline]
fn fract_uv(v: f32) -> f32 {
    let f = v % 1.0;
    if f < 0.0 { f + 1.0 } else { f }
}

/// 三角形の3頂点の UV 座標（fract済み 0..1）が境界をまたぐか判定
#[inline]
fn uv_wraps(a: f32, b: f32, c: f32) -> bool {
    let min = a.min(b).min(c);
    let max = a.max(b).max(c);
    (max - min) > 0.5
}

/// Bresenham 線描画（黒色、アルファ255）
fn draw_line(buf: &mut [u8], dim: usize, p0: (i32, i32), p1: (i32, i32)) {
    let (mut x0, mut y0) = p0;
    let (x1, y1) = p1;
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        put_pixel(buf, dim, x0, y0);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            if x0 == x1 { break; }
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            if y0 == y1 { break; }
            err += dx;
            y0 += sy;
        }
    }
}

/// ピクセル書き込み（黒色 RGBA = 0,0,0,255）
#[inline]
fn put_pixel(buf: &mut [u8], dim: usize, x: i32, y: i32) {
    if x < 0 || y < 0 || x >= dim as i32 || y >= dim as i32 {
        return;
    }
    let offset = ((y as usize) * dim + x as usize) * 4;
    buf[offset] = 0;     // R
    buf[offset + 1] = 0; // G
    buf[offset + 2] = 0; // B
    buf[offset + 3] = 255; // A
}

// ── PSD 最小ライター ──────────────────────────────────────

/// PSD ファイル全体を一括書き出し（RGBA, 8bit/ch）
fn write_psd_file<W: Write>(
    w: &mut W,
    width: u32,
    height: u32,
    layers: &[Vec<u8>],
    names: &[String],
) -> io::Result<()> {
    let ch_count: u16 = 4;
    let pixel_count = (width as usize) * (height as usize);

    // ── File Header (26 bytes) ──
    w.write_all(b"8BPS")?;
    w.write_all(&1u16.to_be_bytes())?; // version = 1
    w.write_all(&[0u8; 6])?;
    w.write_all(&ch_count.to_be_bytes())?;
    w.write_all(&height.to_be_bytes())?;
    w.write_all(&width.to_be_bytes())?;
    w.write_all(&8u16.to_be_bytes())?;  // depth = 8 bit
    w.write_all(&3u16.to_be_bytes())?;  // color mode = RGB

    // ── Color Mode Data ──
    w.write_all(&0u32.to_be_bytes())?;

    // ── Image Resources ──
    w.write_all(&0u32.to_be_bytes())?;

    // ── Layer and Mask Information ──
    let layer_section = build_layer_section(width, height, layers, names)?;
    w.write_all(&(layer_section.len() as u32).to_be_bytes())?;
    w.write_all(&layer_section)?;

    // ── Image Data (composite) ──
    // compression = 0 (raw)
    w.write_all(&0u16.to_be_bytes())?;
    // 全レイヤーを合成した画像（単純に最初に描画のあるピクセルを採用）
    let composite = build_composite(width, height, layers);
    // チャンネル順: R, G, B, A（プレーン形式）
    for ch in 0..4 {
        for i in 0..pixel_count {
            w.write_all(&[composite[i * 4 + ch]])?;
        }
    }

    Ok(())
}

/// レイヤーセクションを構築
fn build_layer_section(
    width: u32,
    height: u32,
    layers: &[Vec<u8>],
    names: &[String],
) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();

    // Layer info
    let layer_info = build_layer_info(width, height, layers, names)?;
    w_u32(&mut buf, layer_info.len() as u32)?;
    buf.extend_from_slice(&layer_info);

    // Global layer mask info (empty)
    w_u32(&mut buf, 0)?;

    Ok(buf)
}

/// レイヤー情報を構築
fn build_layer_info(
    width: u32,
    height: u32,
    layers: &[Vec<u8>],
    names: &[String],
) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    let layer_count = layers.len() as i16;
    let pixel_count = (width as usize) * (height as usize);

    // layer count (正の値 = 合成画像にアルファなし)
    w_i16(&mut buf, layer_count)?;

    // 各レイヤーのレコード
    // チャンネルデータ長を事前計算（raw = 2 + pixel_count per channel）
    let ch_data_len = 2 + pixel_count; // compression(2) + raw data

    for (idx, name) in names.iter().enumerate() {
        // Layer record
        w_u32(&mut buf, 0)?;           // top
        w_u32(&mut buf, 0)?;           // left
        w_u32(&mut buf, height)?;      // bottom
        w_u32(&mut buf, width)?;       // right
        w_u16(&mut buf, 4)?;           // number of channels

        // Channel info (id, data_length) × 4
        // 0=R, 1=G, 2=B, -1=A
        for ch_id in &[0i16, 1, 2, -1] {
            w_i16(&mut buf, *ch_id)?;
            w_u32(&mut buf, ch_data_len as u32)?;
        }

        buf.extend_from_slice(b"8BIM");  // blend mode signature
        buf.extend_from_slice(b"norm");  // blend mode = normal
        buf.push(255);                   // opacity
        buf.push(0);                     // clipping = base
        buf.push(if idx == 0 { 0 } else { 0 }); // flags (visible)
        buf.push(0);                     // filler

        // Extra data
        let pascal_name = encode_pascal_string(name);
        let luni_block = build_luni_block(name);
        let extra_len = 4 + 4 + pascal_name.len() + luni_block.len(); // mask(4) + blending_ranges(4) + name + luni
        w_u32(&mut buf, extra_len as u32)?;

        // Layer mask data (empty)
        w_u32(&mut buf, 0)?;
        // Layer blending ranges (empty)
        w_u32(&mut buf, 0)?;
        // Layer name (Pascal string, padded to 4 bytes)
        buf.extend_from_slice(&pascal_name);
        // Unicode layer name (luni) — Photoshop で日本語等を正しく表示するため
        buf.extend_from_slice(&luni_block);
    }

    // Channel image data for each layer
    for layer_data in layers {
        // R, G, B, A channels (planar, raw compression)
        for ch in [0usize, 1, 2, 3] {
            w_u16(&mut buf, 0)?; // compression = raw
            // チャンネルIDの順: 0=R, 1=G, 2=B, 3→-1=A
            let src_ch = if ch == 3 { 3 } else { ch }; // RGBA順そのまま
            for i in 0..pixel_count {
                buf.push(layer_data[i * 4 + src_ch]);
            }
        }
    }

    Ok(buf)
}

/// 全レイヤーを合成（上のレイヤーが優先、透明ピクセルは下に透過）
fn build_composite(width: u32, height: u32, layers: &[Vec<u8>]) -> Vec<u8> {
    let pixel_count = (width as usize) * (height as usize);
    let mut composite = vec![255u8; pixel_count * 4]; // 白背景
    // アルファチャンネルを255に初期化
    for i in 0..pixel_count {
        composite[i * 4 + 3] = 255;
    }

    // 下から上へ合成（レイヤー0が最下）
    for layer in layers {
        for i in 0..pixel_count {
            let a = layer[i * 4 + 3];
            if a > 0 {
                composite[i * 4] = layer[i * 4];
                composite[i * 4 + 1] = layer[i * 4 + 1];
                composite[i * 4 + 2] = layer[i * 4 + 2];
                composite[i * 4 + 3] = 255;
            }
        }
    }

    composite
}

/// Pascal文字列エンコード（4バイト境界パディング）
fn encode_pascal_string(s: &str) -> Vec<u8> {
    // PSD の Pascal string: 長さバイト + 文字列 + 4バイト境界パディング
    let bytes = s.as_bytes();
    let len = bytes.len().min(255) as u8;
    let mut out = vec![len];
    out.extend_from_slice(&bytes[..len as usize]);
    // 全体を4の倍数にパディング
    while out.len() % 4 != 0 {
        out.push(0);
    }
    out
}

/// Unicode レイヤー名リソース (luni) を構築
/// 形式: "8BIM" + "luni" + length(4) + char_count(4) + UTF-16BE文字列
/// ブロック全体を偶数バイトにパディング
fn build_luni_block(name: &str) -> Vec<u8> {
    let utf16: Vec<u16> = name.encode_utf16().collect();
    let str_bytes = utf16.len() * 2;
    let data_len = 4 + str_bytes; // char_count(4) + UTF-16BE data
    let mut block = Vec::with_capacity(8 + 4 + data_len + 1);
    block.extend_from_slice(b"8BIM");
    block.extend_from_slice(b"luni");
    block.extend_from_slice(&(data_len as u32).to_be_bytes());
    block.extend_from_slice(&(utf16.len() as u32).to_be_bytes());
    for ch in &utf16 {
        block.extend_from_slice(&ch.to_be_bytes());
    }
    // パディング（偶数バイト境界）
    if block.len() % 2 != 0 {
        block.push(0);
    }
    block
}

fn w_u16<W: Write>(w: &mut W, v: u16) -> io::Result<()> {
    w.write_all(&v.to_be_bytes())
}

fn w_i16<W: Write>(w: &mut W, v: i16) -> io::Result<()> {
    w.write_all(&v.to_be_bytes())
}

fn w_u32<W: Write>(w: &mut W, v: u32) -> io::Result<()> {
    w.write_all(&v.to_be_bytes())
}
