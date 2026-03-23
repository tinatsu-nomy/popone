//! UVマップを材質ごとにレイヤー分けした PSD ファイルとして出力する。
//! グループ化指定により、複数モデルマージ時にモデル別フォルダを生成できる。

use std::io::{self, Write};
use std::ops::Range;
use std::path::Path;

use crate::intermediate::types::IrModel;

/// UVマップ出力のデフォルト解像度
pub const DEFAULT_UV_SIZE: u32 = 4096;

// ── PSD レイヤーエントリ ──────────────────────────────────

/// PSD レイヤーセクションに書き出すエントリ（下→上順で格納）
enum PsdLayerEntry<'a> {
    /// 実画像レイヤー
    Content { name: &'a str, rgba: &'a [u8] },
    /// グループ開始マーカー (lsct type=1, blend mode=pass)
    GroupStart { name: &'a str },
    /// グループ終端マーカー (lsct type=3, "</Layer group>")
    GroupEnd,
}

// ── 公開 API ──────────────────────────────────────────────

/// UVマップを PSD として出力する（フラット版、下位互換ラッパー）。
pub fn export_uv_map(ir: &IrModel, path: &Path, size: u32) -> io::Result<()> {
    export_uv_map_grouped(ir, path, size, &[])
}

/// UVマップを PSD として出力する（グループ化対応版）。
/// `groups` は `(グループ名, 材質index範囲)` のスライス。空なら全材質フラット出力。
pub fn export_uv_map_grouped(
    ir: &IrModel,
    path: &Path,
    size: u32,
    groups: &[(String, Range<usize>)],
) -> io::Result<()> {
    let mat_count = ir.materials.len();
    let dim = size as usize;

    // 各材質のレイヤー画像を生成（RGBA、透明背景 + 黒線）
    let mut layers: Vec<Vec<u8>> = Vec::with_capacity(mat_count);
    let mut layer_names: Vec<String> = Vec::with_capacity(mat_count);

    for mat_idx in 0..mat_count {
        let mut buf = vec![0u8; dim * dim * 4]; // RGBA 全透明

        for mesh in &ir.meshes {
            if mesh.material_index != mat_idx {
                continue;
            }
            let verts = &mesh.vertices;
            let indices = &mesh.indices;
            for tri in indices.chunks(3) {
                if tri.len() < 3 {
                    continue;
                }
                let raw: [(f32, f32); 3] = [
                    {
                        let uv = verts[tri[0] as usize].uv;
                        (fract_uv(uv.x), fract_uv(uv.y))
                    },
                    {
                        let uv = verts[tri[1] as usize].uv;
                        (fract_uv(uv.x), fract_uv(uv.y))
                    },
                    {
                        let uv = verts[tri[2] as usize].uv;
                        (fract_uv(uv.x), fract_uv(uv.y))
                    },
                ];

                let u_wraps = uv_wraps(raw[0].0, raw[1].0, raw[2].0);
                let v_wraps = uv_wraps(raw[0].1, raw[1].1, raw[2].1);

                let u_offsets: &[f32] = if u_wraps { &[0.0, -1.0] } else { &[0.0] };
                let v_offsets: &[f32] = if v_wraps { &[0.0, -1.0] } else { &[0.0] };

                for &uo in u_offsets {
                    for &vo in v_offsets {
                        let shifted: [(f32, f32); 3] = std::array::from_fn(|i| {
                            let u =
                                raw[i].0 + if u_wraps && raw[i].0 < 0.5 { 1.0 } else { 0.0 } + uo;
                            let v =
                                raw[i].1 + if v_wraps && raw[i].1 < 0.5 { 1.0 } else { 0.0 } + vo;
                            (u, v)
                        });

                        let px: [(i32, i32); 3] = std::array::from_fn(|i| {
                            (
                                (shifted[i].0 * dim as f32) as i32,
                                (shifted[i].1 * dim as f32) as i32,
                            )
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

    // entries を PSD 書き込み順（下→上）で構築
    let processed = validate_groups(groups, mat_count)?;
    let entries = build_entries(&layers, &layer_names, groups, &processed, mat_count);

    // PSD 書き出し
    let file = std::fs::File::create(path)?;
    let mut w = io::BufWriter::new(file);
    write_psd_file(&mut w, size, size, &entries)?;
    w.flush()?;

    log::info!(
        "UVマップ出力: {} ({}x{}, {}レイヤー)",
        path.display(),
        size,
        size,
        mat_count
    );
    Ok(())
}

// ── entries 構築 ──────────────────────────────────────────

/// グループ情報を検証し、各材質の処理済みフラグを返す。
/// 範囲外・重複・逆順があればエラー。
fn validate_groups(groups: &[(String, Range<usize>)], mat_count: usize) -> io::Result<Vec<bool>> {
    let mut processed = vec![false; mat_count];
    for (name, range) in groups {
        if range.start > range.end {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "グループ '{}' の範囲が逆順です: {}..{}",
                    name, range.start, range.end
                ),
            ));
        }
        if range.end > mat_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "グループ '{}' の範囲が材質数 {} を超えています: {}..{}",
                    name, mat_count, range.start, range.end
                ),
            ));
        }
        for i in range.clone() {
            if processed[i] {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("材質 {} が複数のグループに含まれています", i),
                ));
            }
            processed[i] = true;
        }
    }
    Ok(processed)
}

/// PSD 書き込み順（下→上）の entries リストを構築する。
/// material index 降順で走査し、グループ境界で GroupEnd/GroupStart マーカーを挿入する。
fn build_entries<'a>(
    layers: &'a [Vec<u8>],
    layer_names: &'a [String],
    groups: &'a [(String, Range<usize>)],
    _processed: &[bool],
    mat_count: usize,
) -> Vec<PsdLayerEntry<'a>> {
    // groups を material_range.start 昇順にソートした index 配列
    let mut sorted_indices: Vec<usize> = (0..groups.len())
        .filter(|&i| !groups[i].1.is_empty())
        .collect();
    sorted_indices.sort_by_key(|&i| groups[i].1.start);

    // 各材質がどのソート済みグループに属するか逆引きマップ
    let mut group_map: Vec<Option<usize>> = vec![None; mat_count];
    for (si, &gi) in sorted_indices.iter().enumerate() {
        for mat_idx in groups[gi].1.clone() {
            group_map[mat_idx] = Some(si);
        }
    }

    let mut entries = Vec::new();
    let mut current_group: Option<usize> = None;

    // PSD 配列は下→上順。mat index 降順で走査（mat 0 が最前面 = 既存動作と一致）
    for mat_idx in (0..mat_count).rev() {
        let target_group = group_map[mat_idx];

        // グループが変わったら境界マーカーを挿入
        if current_group != target_group {
            // 前のグループを閉じる
            if let Some(prev_si) = current_group {
                entries.push(PsdLayerEntry::GroupStart {
                    name: &groups[sorted_indices[prev_si]].0,
                });
            }
            // 新しいグループを開く
            if target_group.is_some() {
                entries.push(PsdLayerEntry::GroupEnd);
            }
            current_group = target_group;
        }

        entries.push(PsdLayerEntry::Content {
            name: &layer_names[mat_idx],
            rgba: &layers[mat_idx],
        });
    }

    // 最後のグループを閉じる
    if let Some(last_si) = current_group {
        entries.push(PsdLayerEntry::GroupStart {
            name: &groups[sorted_indices[last_si]].0,
        });
    }

    entries
}

// ── UV 描画ヘルパー ──────────────────────────────────────

/// UV値を 0..1 に正規化（負値対応の fract）
#[inline]
fn fract_uv(v: f32) -> f32 {
    let f = v % 1.0;
    if f < 0.0 {
        f + 1.0
    } else {
        f
    }
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
            if x0 == x1 {
                break;
            }
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            if y0 == y1 {
                break;
            }
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
    buf[offset] = 0; // R
    buf[offset + 1] = 0; // G
    buf[offset + 2] = 0; // B
    buf[offset + 3] = 255; // A
}

// ── PSD ライター ──────────────────────────────────────────

/// lsct (Section Divider Setting) ブロックを構築する。
/// section_type: 1=グループ開始(open folder), 3=グループ終端(bounding section divider)
fn build_lsct_block(section_type: u32) -> Vec<u8> {
    let mut block = Vec::with_capacity(24);
    block.extend_from_slice(b"8BIM");
    block.extend_from_slice(b"lsct");
    block.extend_from_slice(&12u32.to_be_bytes()); // data length = 12
    block.extend_from_slice(&section_type.to_be_bytes()); // type
    block.extend_from_slice(b"8BIM"); // signature
    block.extend_from_slice(b"pass"); // blend mode key
    block
}

/// PSD ファイル全体を一括書き出し（RGBA, 8bit/ch）
fn write_psd_file<W: Write>(
    w: &mut W,
    width: u32,
    height: u32,
    entries: &[PsdLayerEntry],
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
    w.write_all(&8u16.to_be_bytes())?; // depth = 8 bit
    w.write_all(&3u16.to_be_bytes())?; // color mode = RGB

    // ── Color Mode Data ──
    w.write_all(&0u32.to_be_bytes())?;

    // ── Image Resources ──
    w.write_all(&0u32.to_be_bytes())?;

    // ── Layer and Mask Information ──
    let layer_section = build_layer_section(width, height, entries)?;
    w.write_all(&(layer_section.len() as u32).to_be_bytes())?;
    w.write_all(&layer_section)?;

    // ── Image Data (composite) ──
    w.write_all(&0u16.to_be_bytes())?; // compression = raw
    let composite = build_composite(width, height, entries);
    let mut ch_buf = vec![0u8; pixel_count];
    for ch in 0..4 {
        for i in 0..pixel_count {
            ch_buf[i] = composite[i * 4 + ch];
        }
        w.write_all(&ch_buf)?;
    }

    Ok(())
}

/// レイヤーセクションを構築
fn build_layer_section(width: u32, height: u32, entries: &[PsdLayerEntry]) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();

    let layer_info = build_layer_info(width, height, entries)?;
    w_u32(&mut buf, layer_info.len() as u32)?;
    buf.extend_from_slice(&layer_info);

    // Global layer mask info (empty)
    w_u32(&mut buf, 0)?;

    Ok(buf)
}

/// レイヤー情報を構築（entries ベース）
fn build_layer_info(width: u32, height: u32, entries: &[PsdLayerEntry]) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    let layer_count = entries.len() as i16;
    let pixel_count = (width as usize) * (height as usize);

    // layer count (正の値 = 合成画像にアルファなし)
    w_i16(&mut buf, layer_count)?;

    // Content レイヤーのチャンネルデータ長（raw = 2 + pixel_count per channel）
    let content_ch_data_len = (2 + pixel_count) as u32;
    // グループマーカーのチャンネルデータ長（compression u16 のみ）
    let marker_ch_data_len: u32 = 2;

    // ── 各エントリのレイヤーレコード ──
    for entry in entries {
        match entry {
            PsdLayerEntry::Content { name, .. } => {
                w_u32(&mut buf, 0)?; // top
                w_u32(&mut buf, 0)?; // left
                w_u32(&mut buf, height)?; // bottom
                w_u32(&mut buf, width)?; // right
                w_u16(&mut buf, 4)?; // number of channels

                for ch_id in &[0i16, 1, 2, -1] {
                    w_i16(&mut buf, *ch_id)?;
                    w_u32(&mut buf, content_ch_data_len)?;
                }

                buf.extend_from_slice(b"8BIM"); // blend mode signature
                buf.extend_from_slice(b"norm"); // blend mode = normal
                buf.push(255); // opacity
                buf.push(0); // clipping = base
                buf.push(0); // flags (visible)
                buf.push(0); // filler

                let pascal_name = encode_pascal_string(name);
                let luni_block = build_luni_block(name);
                let extra_len = 4 + 4 + pascal_name.len() + luni_block.len();
                w_u32(&mut buf, extra_len as u32)?;

                w_u32(&mut buf, 0)?; // Layer mask data (empty)
                w_u32(&mut buf, 0)?; // Layer blending ranges (empty)
                buf.extend_from_slice(&pascal_name);
                buf.extend_from_slice(&luni_block);
            }
            PsdLayerEntry::GroupStart { name } => {
                // グループ開始: top=left=bottom=right=0, 4ch empty
                w_u32(&mut buf, 0)?; // top
                w_u32(&mut buf, 0)?; // left
                w_u32(&mut buf, 0)?; // bottom
                w_u32(&mut buf, 0)?; // right
                w_u16(&mut buf, 4)?; // number of channels

                for ch_id in &[0i16, 1, 2, -1] {
                    w_i16(&mut buf, *ch_id)?;
                    w_u32(&mut buf, marker_ch_data_len)?;
                }

                buf.extend_from_slice(b"8BIM");
                buf.extend_from_slice(b"pass"); // pass-through blend mode
                buf.push(255); // opacity
                buf.push(0); // clipping
                buf.push(0); // flags
                buf.push(0); // filler

                let pascal_name = encode_pascal_string(name);
                let luni_block = build_luni_block(name);
                let lsct_block = build_lsct_block(1); // type=1: open folder
                let extra_len = 4 + 4 + pascal_name.len() + luni_block.len() + lsct_block.len();
                w_u32(&mut buf, extra_len as u32)?;

                w_u32(&mut buf, 0)?; // Layer mask data (empty)
                w_u32(&mut buf, 0)?; // Layer blending ranges (empty)
                buf.extend_from_slice(&pascal_name);
                buf.extend_from_slice(&luni_block);
                buf.extend_from_slice(&lsct_block);
            }
            PsdLayerEntry::GroupEnd => {
                let end_name = "</Layer group>";
                w_u32(&mut buf, 0)?; // top
                w_u32(&mut buf, 0)?; // left
                w_u32(&mut buf, 0)?; // bottom
                w_u32(&mut buf, 0)?; // right
                w_u16(&mut buf, 4)?; // number of channels

                for ch_id in &[0i16, 1, 2, -1] {
                    w_i16(&mut buf, *ch_id)?;
                    w_u32(&mut buf, marker_ch_data_len)?;
                }

                buf.extend_from_slice(b"8BIM");
                buf.extend_from_slice(b"pass"); // pass-through blend mode
                buf.push(255); // opacity
                buf.push(0); // clipping
                buf.push(0); // flags
                buf.push(0); // filler

                let pascal_name = encode_pascal_string(end_name);
                let luni_block = build_luni_block(end_name);
                let lsct_block = build_lsct_block(3); // type=3: bounding section divider
                let extra_len = 4 + 4 + pascal_name.len() + luni_block.len() + lsct_block.len();
                w_u32(&mut buf, extra_len as u32)?;

                w_u32(&mut buf, 0)?; // Layer mask data (empty)
                w_u32(&mut buf, 0)?; // Layer blending ranges (empty)
                buf.extend_from_slice(&pascal_name);
                buf.extend_from_slice(&luni_block);
                buf.extend_from_slice(&lsct_block);
            }
        }
    }

    // ── 各エントリのチャンネルデータ ──
    for entry in entries {
        match entry {
            PsdLayerEntry::Content { rgba, .. } => {
                for ch in [0usize, 1, 2, 3] {
                    w_u16(&mut buf, 0)?; // compression = raw
                    let src_ch = if ch == 3 { 3 } else { ch };
                    buf.reserve(pixel_count);
                    for i in 0..pixel_count {
                        buf.push(rgba[i * 4 + src_ch]);
                    }
                }
            }
            PsdLayerEntry::GroupStart { .. } | PsdLayerEntry::GroupEnd => {
                // 4チャンネル × compression(u16) のみ
                for _ in 0..4 {
                    w_u16(&mut buf, 0)?; // compression = raw, ピクセルデータなし
                }
            }
        }
    }

    Ok(buf)
}

/// 全レイヤーを合成（entries 順 = PSD 下→上順、上のレイヤーが優先）
fn build_composite(width: u32, height: u32, entries: &[PsdLayerEntry]) -> Vec<u8> {
    let pixel_count = (width as usize) * (height as usize);
    let mut composite = vec![255u8; pixel_count * 4]; // 白背景（RGBA全255）

    // entries は PSD 下→上順。Content のみを順に合成（後のレイヤーが上に重なる）
    for entry in entries {
        if let PsdLayerEntry::Content { rgba, .. } = entry {
            for i in 0..pixel_count {
                let a = rgba[i * 4 + 3];
                if a > 0 {
                    composite[i * 4] = rgba[i * 4];
                    composite[i * 4 + 1] = rgba[i * 4 + 1];
                    composite[i * 4 + 2] = rgba[i * 4 + 2];
                    composite[i * 4 + 3] = 255;
                }
            }
        }
    }

    composite
}

// ── 文字列エンコード ──────────────────────────────────────

/// Pascal文字列エンコード（4バイト境界パディング）
fn encode_pascal_string(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let len = bytes.len().min(255) as u8;
    let mut out = vec![len];
    out.extend_from_slice(&bytes[..len as usize]);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
    out
}

/// Unicode レイヤー名リソース (luni) を構築
fn build_luni_block(name: &str) -> Vec<u8> {
    let utf16: Vec<u16> = name.encode_utf16().collect();
    let str_bytes = utf16.len() * 2;
    let data_len = 4 + str_bytes;
    let mut block = Vec::with_capacity(8 + 4 + data_len + 1);
    block.extend_from_slice(b"8BIM");
    block.extend_from_slice(b"luni");
    block.extend_from_slice(&(data_len as u32).to_be_bytes());
    block.extend_from_slice(&(utf16.len() as u32).to_be_bytes());
    for ch in &utf16 {
        block.extend_from_slice(&ch.to_be_bytes());
    }
    if block.len() % 2 != 0 {
        block.push(0);
    }
    block
}

// ── バイナリヘルパー ──────────────────────────────────────

fn w_u16<W: Write>(w: &mut W, v: u16) -> io::Result<()> {
    w.write_all(&v.to_be_bytes())
}

fn w_i16<W: Write>(w: &mut W, v: i16) -> io::Result<()> {
    w.write_all(&v.to_be_bytes())
}

fn w_u32<W: Write>(w: &mut W, v: u32) -> io::Result<()> {
    w.write_all(&v.to_be_bytes())
}

// ── テスト ────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_lsct_block_format() {
        let block = build_lsct_block(1);
        assert_eq!(block.len(), 24);
        assert_eq!(&block[0..4], b"8BIM");
        assert_eq!(&block[4..8], b"lsct");
        assert_eq!(&block[8..12], &12u32.to_be_bytes()); // data length
        assert_eq!(&block[12..16], &1u32.to_be_bytes()); // section type
        assert_eq!(&block[16..20], b"8BIM");
        assert_eq!(&block[20..24], b"pass");

        let block3 = build_lsct_block(3);
        assert_eq!(&block3[12..16], &3u32.to_be_bytes());
    }

    #[test]
    fn test_validate_groups_rejects_out_of_range() {
        let groups = vec![("A".to_string(), 0..5)];
        let result = validate_groups(&groups, 3);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("超えています"));
    }

    #[test]
    fn test_validate_groups_rejects_overlap() {
        let groups = vec![("A".to_string(), 0..3), ("B".to_string(), 2..5)];
        let result = validate_groups(&groups, 5);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("複数のグループ"));
    }

    #[test]
    fn test_validate_groups_rejects_reversed_range() {
        let groups = vec![("A".to_string(), 3..1)];
        let result = validate_groups(&groups, 5);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("逆順"));
    }

    #[test]
    fn test_empty_groups_flat_output() {
        // 空グループでフラット出力
        let dim = 2usize;
        let layers: Vec<Vec<u8>> = (0..3).map(|_| vec![0u8; dim * dim * 4]).collect();
        let names: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
        let processed = validate_groups(&[], 3).unwrap();
        let entries = build_entries(&layers, &names, &[], &processed, 3);

        // 全て Content、グループマーカーなし
        assert_eq!(entries.len(), 3);
        for entry in &entries {
            assert!(matches!(entry, PsdLayerEntry::Content { .. }));
        }
        // mat index 降順: c, b, a
        match &entries[0] {
            PsdLayerEntry::Content { name, .. } => assert_eq!(*name, "c"),
            _ => panic!(),
        }
        match &entries[1] {
            PsdLayerEntry::Content { name, .. } => assert_eq!(*name, "b"),
            _ => panic!(),
        }
        match &entries[2] {
            PsdLayerEntry::Content { name, .. } => assert_eq!(*name, "a"),
            _ => panic!(),
        }
    }

    #[test]
    fn test_one_group_structure() {
        let dim = 2usize;
        let layers: Vec<Vec<u8>> = (0..3).map(|_| vec![0u8; dim * dim * 4]).collect();
        let names: Vec<String> = vec!["m0".into(), "m1".into(), "m2".into()];
        let groups = vec![("Model".to_string(), 0..3)];
        let processed = validate_groups(&groups, 3).unwrap();
        let entries = build_entries(&layers, &names, &groups, &processed, 3);

        // GroupEnd → m2 → m1 → m0 → GroupStart
        assert_eq!(entries.len(), 5);
        assert!(matches!(&entries[0], PsdLayerEntry::GroupEnd));
        assert!(matches!(
            &entries[1],
            PsdLayerEntry::Content { name: "m2", .. }
        ));
        assert!(matches!(
            &entries[2],
            PsdLayerEntry::Content { name: "m1", .. }
        ));
        assert!(matches!(
            &entries[3],
            PsdLayerEntry::Content { name: "m0", .. }
        ));
        match &entries[4] {
            PsdLayerEntry::GroupStart { name } => assert_eq!(*name, "Model"),
            _ => panic!(),
        }
    }

    #[test]
    fn test_orphan_materials_output() {
        // mat0,1 はグループ、mat2 は孤立
        let dim = 2usize;
        let layers: Vec<Vec<u8>> = (0..3).map(|_| vec![0u8; dim * dim * 4]).collect();
        let names: Vec<String> = vec!["m0".into(), "m1".into(), "orphan".into()];
        let groups = vec![("G".to_string(), 0..2)];
        let processed = validate_groups(&groups, 3).unwrap();
        let entries = build_entries(&layers, &names, &groups, &processed, 3);

        // orphan(2) → GroupEnd → m1 → m0 → GroupStart
        assert_eq!(entries.len(), 5);
        assert!(matches!(
            &entries[0],
            PsdLayerEntry::Content { name: "orphan", .. }
        ));
        assert!(matches!(&entries[1], PsdLayerEntry::GroupEnd));
        assert!(matches!(
            &entries[2],
            PsdLayerEntry::Content { name: "m1", .. }
        ));
        assert!(matches!(
            &entries[3],
            PsdLayerEntry::Content { name: "m0", .. }
        ));
        match &entries[4] {
            PsdLayerEntry::GroupStart { name } => assert_eq!(*name, "G"),
            _ => panic!(),
        }
    }

    #[test]
    fn test_two_groups_layer_order() {
        // mat0-1: GroupA, mat2: 孤立, mat3-4: GroupB
        let dim = 2usize;
        let layers: Vec<Vec<u8>> = (0..5).map(|_| vec![0u8; dim * dim * 4]).collect();
        let names: Vec<String> = (0..5).map(|i| format!("m{i}")).collect();
        let groups = vec![("GA".to_string(), 0..2), ("GB".to_string(), 3..5)];
        let processed = validate_groups(&groups, 5).unwrap();
        let entries = build_entries(&layers, &names, &groups, &processed, 5);

        // PSD 下→上:
        // GroupEnd(GB) → m4 → m3 → GroupStart(GB)
        // → orphan(m2)
        // → GroupEnd(GA) → m1 → m0 → GroupStart(GA)
        let entry_desc: Vec<&str> = entries
            .iter()
            .map(|e| match e {
                PsdLayerEntry::Content { name, .. } => *name,
                PsdLayerEntry::GroupStart { name } => *name,
                PsdLayerEntry::GroupEnd => "</end>",
            })
            .collect();

        assert_eq!(
            entry_desc,
            vec!["</end>", "m4", "m3", "GB", "m2", "</end>", "m1", "m0", "GA",]
        );
    }

    #[test]
    fn test_entries_sorted_by_material_range() {
        // groups を逆順で渡しても material_range.start 昇順にソートされる
        let dim = 2usize;
        let layers: Vec<Vec<u8>> = (0..4).map(|_| vec![0u8; dim * dim * 4]).collect();
        let names: Vec<String> = (0..4).map(|i| format!("m{i}")).collect();
        let groups = vec![("Second".to_string(), 2..4), ("First".to_string(), 0..2)];
        let processed = validate_groups(&groups, 4).unwrap();
        let entries = build_entries(&layers, &names, &groups, &processed, 4);

        // Second(2..4) が下、First(0..2) が上
        let entry_desc: Vec<&str> = entries
            .iter()
            .map(|e| match e {
                PsdLayerEntry::Content { name, .. } => *name,
                PsdLayerEntry::GroupStart { name } => *name,
                PsdLayerEntry::GroupEnd => "</end>",
            })
            .collect();

        assert_eq!(
            entry_desc,
            vec!["</end>", "m3", "m2", "Second", "</end>", "m1", "m0", "First",]
        );
    }

    #[test]
    fn test_layer_info_lsct_bytes() {
        // 1グループ1材質で PSD バイト列を検証
        let layers = vec![vec![0u8, 0, 0, 255]]; // 1x1 黒ピクセル
        let names = vec!["mat".to_string()];
        let groups = vec![("G".to_string(), 0..1)];
        let processed = validate_groups(&groups, 1).unwrap();
        let entries = build_entries(&layers, &names, &groups, &processed, 1);

        let info = build_layer_info(1, 1, &entries).unwrap();

        // layer_count = 3 (GroupEnd + Content + GroupStart)
        let count = i16::from_be_bytes([info[0], info[1]]);
        assert_eq!(count, 3);

        // lsct ブロックを検索して type 値を確認
        let info_bytes = &info;
        let mut lsct_types = Vec::new();
        for i in 0..info_bytes.len().saturating_sub(8) {
            if &info_bytes[i..i + 4] == b"8BIM" && &info_bytes[i + 4..i + 8] == b"lsct" {
                let t = u32::from_be_bytes([
                    info_bytes[i + 12],
                    info_bytes[i + 13],
                    info_bytes[i + 14],
                    info_bytes[i + 15],
                ]);
                lsct_types.push(t);
            }
        }
        // GroupEnd(type=3) が先、GroupStart(type=1) が後
        assert_eq!(lsct_types, vec![3, 1]);
    }

    #[test]
    fn test_group_start_blend_mode() {
        // GroupStart のレイヤーレコードが "pass" blend mode を持つことを検証
        let layers = vec![vec![0u8; 4]];
        let names = vec!["m".to_string()];
        let groups = vec![("G".to_string(), 0..1)];
        let processed = validate_groups(&groups, 1).unwrap();
        let entries = build_entries(&layers, &names, &groups, &processed, 1);

        let info = build_layer_info(1, 1, &entries).unwrap();

        // "pass" が2回出現するはず（GroupEnd と GroupStart のレイヤーレコード内）
        let mut pass_count = 0;
        for i in 0..info.len().saturating_sub(4) {
            if &info[i..i + 4] == b"pass" {
                pass_count += 1;
            }
        }
        // GroupEnd(pass blend) + GroupStart(pass blend) + lsct内pass × 2 = 4回
        assert!(pass_count >= 4, "pass count: {pass_count}");
    }

    #[test]
    fn test_group_end_name() {
        // GroupEnd のレイヤー名が "</Layer group>" であることを検証（luni ブロック内）
        let luni = build_luni_block("</Layer group>");
        let expected_utf16: Vec<u16> = "</Layer group>".encode_utf16().collect();
        // luni 構造: "8BIM" + "luni" + len(4) + char_count(4) + UTF-16BE chars
        let char_count = u32::from_be_bytes([luni[12], luni[13], luni[14], luni[15]]);
        assert_eq!(char_count, expected_utf16.len() as u32);
    }
}
