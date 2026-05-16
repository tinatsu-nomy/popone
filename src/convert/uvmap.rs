//! Export the UV map as a PSD (or PSB / Large Document Format) file with one
//! layer per material. When groups are supplied, merging multiple models
//! places each one in its own folder.
//!
//! PSD vs PSB: PSD stores section lengths as `u32`, so once the layer section
//! exceeds ~2 GiB the writer silently truncates and produces a corrupt file
//! (PSB raises those length fields to `u64`). This module estimates the layer
//! section size before writing and auto-promotes to PSB (renaming `.psd`
//! → `.psb`) when the estimate crosses the safety threshold.

use rust_i18n::t;
use std::io::{self, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::intermediate::types::IrModel;

/// Default resolution for UV-map exports.
pub const DEFAULT_UV_SIZE: u32 = 4096;

/// Output container format. `Psd` is the legacy 4-byte length format and is
/// limited to ~2 GiB total layer section size. `Psb` (Large Document Format)
/// uses 8-byte length fields and removes that limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PsFormat {
    Psd,
    Psb,
}

/// Promote PSD → PSB when the estimated layer section size exceeds this many
/// bytes. The hard PSD limit is `u32::MAX`; we leave a generous safety margin
/// because the estimate ignores per-layer record overhead and luni/lsct
/// blocks. 1.9 GiB ≈ 2_040_109_465 bytes.
const PSD_TO_PSB_THRESHOLD_BYTES: u64 = 1_900 * 1024 * 1024;

// -- PSD layer entries --------------------------------------

/// Entries written into the PSD layer section (stored bottom-to-top).
enum PsdLayerEntry<'a> {
    /// Real image layer.
    Content { name: &'a str, rgba: &'a [u8] },
    /// Group-start marker (lsct type = 1, blend mode = pass).
    GroupStart { name: &'a str },
    /// Group-end marker (lsct type = 3, "</Layer group>").
    GroupEnd,
}

// -- Public API ---------------------------------------------

/// Export the UV map as a PSD (flat version; backwards-compatible wrapper).
/// Returns the path actually written, which may have been promoted from
/// `.psd` to `.psb` when the file would exceed the PSD 2 GiB length limit.
pub fn export_uv_map(ir: &IrModel, path: &Path, size: u32) -> io::Result<PathBuf> {
    export_uv_map_grouped(ir, path, size, &[])
}

/// Export the UV map as a PSD/PSB with grouping support.
/// `groups` is a slice of `(group name, material index range)`. An empty slice flattens every material.
/// Returns the path actually written. When the estimated layer section size
/// exceeds [`PSD_TO_PSB_THRESHOLD_BYTES`] the writer auto-promotes to PSB and
/// rewrites the extension to `.psb`.
pub fn export_uv_map_grouped(
    ir: &IrModel,
    path: &Path,
    size: u32,
    groups: &[(String, Range<usize>)],
) -> io::Result<PathBuf> {
    let mat_count = ir.materials.len();
    let dim = size as usize;

    // Generate the layer image for each material (RGBA, transparent background + black lines)
    let mut layers: Vec<Vec<u8>> = Vec::with_capacity(mat_count);
    let mut layer_names: Vec<String> = Vec::with_capacity(mat_count);

    for mat_idx in 0..mat_count {
        let mut buf = vec![0u8; dim * dim * 4]; // RGBA, fully transparent

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
                            // Clamp so u=1.0 or v=1.0 cannot land outside the pixel range
                            let x = (shifted[i].0 * dim as f32) as i32;
                            let y = (shifted[i].1 * dim as f32) as i32;
                            let max = dim as i32 - 1;
                            (x.min(max), y.min(max))
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

    // Build entries in PSD write order (bottom-to-top)
    let processed = validate_groups(groups, mat_count)?;
    let entries = build_entries(&layers, &layer_names, groups, &processed, mat_count);

    // Estimate the layer section size and pick PSD or PSB. A PSD whose layer
    // section exceeds ~2 GiB is silently corrupted by the 4-byte length
    // fields, so we promote oversized writes to PSB up front.
    let estimated = estimate_layer_section_bytes(size, size, &entries);
    let format = if estimated > PSD_TO_PSB_THRESHOLD_BYTES {
        PsFormat::Psb
    } else {
        PsFormat::Psd
    };
    let final_path = adjust_extension_for_format(path, format);

    if format == PsFormat::Psb {
        log::info!(
            "UV map export: estimated layer section {} bytes exceeds PSD threshold {} bytes; promoting to PSB ({})",
            estimated,
            PSD_TO_PSB_THRESHOLD_BYTES,
            final_path.display()
        );
    }

    // Write the file
    let file = std::fs::File::create(&final_path)?;
    let mut w = io::BufWriter::new(file);
    write_ps_file(&mut w, size, size, &entries, format)?;
    w.flush()?;

    log::info!(
        "UV map export: {} ({}x{}, {} layers, format={:?})",
        final_path.display(),
        size,
        size,
        mat_count,
        format
    );
    Ok(final_path)
}

/// Replace the file extension to match the chosen format. PSD → `.psd`,
/// PSB → `.psb`. If the input has no extension, append the appropriate one.
fn adjust_extension_for_format(path: &Path, format: PsFormat) -> PathBuf {
    let target_ext = match format {
        PsFormat::Psd => "psd",
        PsFormat::Psb => "psb",
    };
    let mut out = path.to_path_buf();
    out.set_extension(target_ext);
    out
}

// -- Entry construction -------------------------------------

/// Validate the group spec and return a processed flag per material.
/// Errors on out-of-range, overlapping, or reversed ranges.
fn validate_groups(groups: &[(String, Range<usize>)], mat_count: usize) -> io::Result<Vec<bool>> {
    let mut processed = vec![false; mat_count];
    for (name, range) in groups {
        if range.start > range.end {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                t!(
                    "error.uvmap.group_range_reversed",
                    name = name.clone(),
                    start = range.start.to_string(),
                    end = range.end.to_string()
                )
                .to_string(),
            ));
        }
        if range.end > mat_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                t!(
                    "error.uvmap.group_range_out_of_bounds",
                    name = name.clone(),
                    count = mat_count.to_string(),
                    start = range.start.to_string(),
                    end = range.end.to_string()
                )
                .to_string(),
            ));
        }
        for i in range.clone() {
            if processed[i] {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    t!("error.uvmap.group_overlap", index = i.to_string()).to_string(),
                ));
            }
            processed[i] = true;
        }
    }
    Ok(processed)
}

/// Build the entries list in PSD write order (bottom-to-top).
/// Walks materials in descending index order and inserts GroupEnd/GroupStart markers at boundaries.
fn build_entries<'a>(
    layers: &'a [Vec<u8>],
    layer_names: &'a [String],
    groups: &'a [(String, Range<usize>)],
    _processed: &[bool],
    mat_count: usize,
) -> Vec<PsdLayerEntry<'a>> {
    // Indexes into groups, sorted by material_range.start ascending
    let mut sorted_indices: Vec<usize> = (0..groups.len())
        .filter(|&i| !groups[i].1.is_empty())
        .collect();
    sorted_indices.sort_by_key(|&i| groups[i].1.start);

    // Reverse map: material index -> sorted group index
    let mut group_map: Vec<Option<usize>> = vec![None; mat_count];
    for (si, &gi) in sorted_indices.iter().enumerate() {
        for mat_idx in groups[gi].1.clone() {
            group_map[mat_idx] = Some(si);
        }
    }

    let mut entries = Vec::new();
    let mut current_group: Option<usize> = None;

    // PSD layers are stored bottom-to-top. Walk material indices in descending order so that
    // material 0 ends up on top (matches the existing behavior).
    for mat_idx in (0..mat_count).rev() {
        let target_group = group_map[mat_idx];

        // Insert boundary markers when the active group changes
        if current_group != target_group {
            // Close the previous group
            if let Some(prev_si) = current_group {
                entries.push(PsdLayerEntry::GroupStart {
                    name: &groups[sorted_indices[prev_si]].0,
                });
            }
            // Open the new group
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

    // Close the final group
    if let Some(last_si) = current_group {
        entries.push(PsdLayerEntry::GroupStart {
            name: &groups[sorted_indices[last_si]].0,
        });
    }

    entries
}

// -- UV-drawing helpers -------------------------------------

/// Normalize a UV to 0..1 (`fract` with negative-value support).
/// Values inside [0, 1] are kept as-is (prevents 1.0 % 1.0 = 0.0 rounding).
#[inline]
fn fract_uv(v: f32) -> f32 {
    if (0.0..=1.0).contains(&v) {
        return v;
    }
    let f = v % 1.0;
    if f < 0.0 {
        f + 1.0
    } else {
        f
    }
}

/// Whether the three triangle UVs (already fract-ed to 0..1) cross the wrap boundary.
#[inline]
fn uv_wraps(a: f32, b: f32, c: f32) -> bool {
    let min = a.min(b).min(c);
    let max = a.max(b).max(c);
    (max - min) > 0.5
}

/// Bresenham line drawing (black, alpha = 255).
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

/// Plot a pixel (black RGBA = 0, 0, 0, 255).
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

// -- PSD writer ---------------------------------------------

/// Build an lsct (Section Divider Setting) block.
/// section_type: 1 = group start (open folder), 3 = group end (bounding section divider).
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

/// Estimate the in-memory size of the layer section (in bytes) before writing.
/// Used to decide between PSD and PSB. The estimate is intentionally a slight
/// over-approximation: per-layer record overhead is rounded up so the
/// threshold check stays conservative.
fn estimate_layer_section_bytes(width: u32, height: u32, entries: &[PsdLayerEntry]) -> u64 {
    let pixel_count = (width as u64) * (height as u64);
    // Layer count i16 + global layer mask info u32 ≈ 8 bytes of fixed overhead.
    let mut total: u64 = 8;
    // Per-Content channel data: 4 channels × (compression u16 + pixel_count).
    // Per-marker channel data: 4 channels × compression u16 = 8 bytes.
    // Layer record overhead (rect, channels, blend mode, lengths, names, luni,
    // optional lsct): bounded by ~256 bytes per entry; we round up to 512 to
    // cover unicode names and future additions.
    for entry in entries {
        match entry {
            PsdLayerEntry::Content { .. } => {
                total += 512 + 4 * (2 + pixel_count);
            }
            PsdLayerEntry::GroupStart { .. } | PsdLayerEntry::GroupEnd => {
                total += 512 + 4 * 2;
            }
        }
    }
    total
}

/// Write the entire PSD or PSB file at once (RGBA, 8 bit/channel).
fn write_ps_file<W: Write>(
    w: &mut W,
    width: u32,
    height: u32,
    entries: &[PsdLayerEntry],
    format: PsFormat,
) -> io::Result<()> {
    let ch_count: u16 = 4;
    let pixel_count = (width as usize) * (height as usize);

    // -- File header (26 bytes) --
    let (sig, version): (&[u8; 4], u16) = match format {
        PsFormat::Psd => (b"8BPS", 1),
        PsFormat::Psb => (b"8BPB", 2),
    };
    w.write_all(sig)?;
    w.write_all(&version.to_be_bytes())?;
    w.write_all(&[0u8; 6])?;
    w.write_all(&ch_count.to_be_bytes())?;
    w.write_all(&height.to_be_bytes())?;
    w.write_all(&width.to_be_bytes())?;
    w.write_all(&8u16.to_be_bytes())?; // depth = 8 bit
    w.write_all(&3u16.to_be_bytes())?; // color mode = RGB

    // -- Color Mode Data --
    w.write_all(&0u32.to_be_bytes())?;

    // -- Image Resources --
    w.write_all(&0u32.to_be_bytes())?;

    // -- Layer and Mask Information --
    // PSD: section length is u32. PSB: section length is u64.
    let layer_section = build_layer_section(width, height, entries, format)?;
    write_section_length(w, layer_section.len() as u64, format)?;
    w.write_all(&layer_section)?;

    // -- Image Data (composite) --
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

/// Write a section length using u32 (PSD) or u64 (PSB).
fn write_section_length<W: Write>(w: &mut W, len: u64, format: PsFormat) -> io::Result<()> {
    match format {
        PsFormat::Psd => w.write_all(&(len as u32).to_be_bytes()),
        PsFormat::Psb => w.write_all(&len.to_be_bytes()),
    }
}

/// Append a section length to a `Vec<u8>` using u32 (PSD) or u64 (PSB).
fn push_section_length(buf: &mut Vec<u8>, len: u64, format: PsFormat) {
    match format {
        PsFormat::Psd => buf.extend_from_slice(&(len as u32).to_be_bytes()),
        PsFormat::Psb => buf.extend_from_slice(&len.to_be_bytes()),
    }
}

/// Build the layer section.
fn build_layer_section(
    width: u32,
    height: u32,
    entries: &[PsdLayerEntry],
    format: PsFormat,
) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();

    let layer_info = build_layer_info(width, height, entries, format)?;
    push_section_length(&mut buf, layer_info.len() as u64, format);
    buf.extend_from_slice(&layer_info);

    // Global layer mask info (empty) — same u32 length on both PSD and PSB.
    w_u32(&mut buf, 0)?;

    Ok(buf)
}

/// Build the layer info (driven by `entries`).
fn build_layer_info(
    width: u32,
    height: u32,
    entries: &[PsdLayerEntry],
    format: PsFormat,
) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    let layer_count = entries.len() as i16;
    let pixel_count = (width as usize) * (height as usize);

    // layer count (positive = composite has no alpha)
    w_i16(&mut buf, layer_count)?;

    // Channel data length for Content layers (raw = 2 + pixel_count per channel).
    // PSD: u32, PSB: u64. We carry a u64 and write through `push_section_length`.
    let content_ch_data_len: u64 = 2 + pixel_count as u64;
    // Channel data length for group markers (just the compression u16)
    let marker_ch_data_len: u64 = 2;

    // -- Layer records per entry --
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
                    push_section_length(&mut buf, content_ch_data_len, format);
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
                // Group start: top = left = bottom = right = 0, 4 empty channels
                w_u32(&mut buf, 0)?; // top
                w_u32(&mut buf, 0)?; // left
                w_u32(&mut buf, 0)?; // bottom
                w_u32(&mut buf, 0)?; // right
                w_u16(&mut buf, 4)?; // number of channels

                for ch_id in &[0i16, 1, 2, -1] {
                    w_i16(&mut buf, *ch_id)?;
                    push_section_length(&mut buf, marker_ch_data_len, format);
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
                    push_section_length(&mut buf, marker_ch_data_len, format);
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

    // -- Channel data per entry --
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
                // 4 channels x compression(u16) only
                for _ in 0..4 {
                    w_u16(&mut buf, 0)?; // compression = raw, no pixel data
                }
            }
        }
    }

    Ok(buf)
}

/// Composite every layer (entries order = PSD bottom-to-top; later entries win).
fn build_composite(width: u32, height: u32, entries: &[PsdLayerEntry]) -> Vec<u8> {
    let pixel_count = (width as usize) * (height as usize);
    let mut composite = vec![255u8; pixel_count * 4]; // White background (RGBA all 255)

    // entries is in PSD bottom-to-top order. Composite only Content layers; later ones land on top.
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

// -- String encoding ----------------------------------------

/// Pascal-string encoding (padded to a 4-byte boundary).
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

/// Build the Unicode layer-name resource (luni).
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

// -- Binary helpers -----------------------------------------

fn w_u16<W: Write>(w: &mut W, v: u16) -> io::Result<()> {
    w.write_all(&v.to_be_bytes())
}

fn w_i16<W: Write>(w: &mut W, v: i16) -> io::Result<()> {
    w.write_all(&v.to_be_bytes())
}

fn w_u32<W: Write>(w: &mut W, v: u32) -> io::Result<()> {
    w.write_all(&v.to_be_bytes())
}

// -- Tests --------------------------------------------------

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

    // The three tests below intentionally avoid asserting on a specific
    // localized phrase from the error message. After the i18n migration
    // the wording depends on the active locale (which on CI runners is
    // typically `en`), so we anchor the assertions on locale-independent
    // signals instead: `io::ErrorKind` and the data values that appear
    // verbatim regardless of language (group name, index numbers).

    #[test]
    fn test_validate_groups_rejects_out_of_range() {
        let groups = vec![("OutOfRangeGroup".to_string(), 0..5)];
        let err = validate_groups(&groups, 3).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        let msg = err.to_string();
        assert!(
            msg.contains("OutOfRangeGroup"),
            "expected group name in error: {msg}"
        );
        assert!(
            msg.contains('3') && msg.contains('5'),
            "expected mat count and range bound in error: {msg}"
        );
    }

    #[test]
    fn test_validate_groups_rejects_overlap() {
        let groups = vec![
            ("OverlapA".to_string(), 0..3),
            ("OverlapB".to_string(), 2..5),
        ];
        let err = validate_groups(&groups, 5).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        // Material at index 2 is the first overlap and should appear in the message.
        let msg = err.to_string();
        assert!(msg.contains('2'), "expected overlapping index in: {msg}");
    }

    #[test]
    #[allow(clippy::reversed_empty_ranges)]
    fn test_validate_groups_rejects_reversed_range() {
        let groups = vec![("ReversedGroup".to_string(), 3..1)];
        let err = validate_groups(&groups, 5).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        let msg = err.to_string();
        assert!(
            msg.contains("ReversedGroup"),
            "expected group name in error: {msg}"
        );
        assert!(
            msg.contains('3') && msg.contains('1'),
            "expected reversed range bounds in error: {msg}"
        );
    }

    #[test]
    fn test_empty_groups_flat_output() {
        // Empty groups -> flat output
        let dim = 2usize;
        let layers: Vec<Vec<u8>> = (0..3).map(|_| vec![0u8; dim * dim * 4]).collect();
        let names: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
        let processed = validate_groups(&[], 3).unwrap();
        let entries = build_entries(&layers, &names, &[], &processed, 3);

        // All entries are Content; no group markers
        assert_eq!(entries.len(), 3);
        for entry in &entries {
            assert!(matches!(entry, PsdLayerEntry::Content { .. }));
        }
        // mat-index descending: c, b, a
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

        // GroupEnd -> m2 -> m1 -> m0 -> GroupStart
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
        // mat0/mat1 belong to a group; mat2 is orphaned
        let dim = 2usize;
        let layers: Vec<Vec<u8>> = (0..3).map(|_| vec![0u8; dim * dim * 4]).collect();
        let names: Vec<String> = vec!["m0".into(), "m1".into(), "orphan".into()];
        let groups = vec![("G".to_string(), 0..2)];
        let processed = validate_groups(&groups, 3).unwrap();
        let entries = build_entries(&layers, &names, &groups, &processed, 3);

        // orphan(2) -> GroupEnd -> m1 -> m0 -> GroupStart
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
        // mat0-1: GroupA, mat2: orphan, mat3-4: GroupB
        let dim = 2usize;
        let layers: Vec<Vec<u8>> = (0..5).map(|_| vec![0u8; dim * dim * 4]).collect();
        let names: Vec<String> = (0..5).map(|i| format!("m{i}")).collect();
        let groups = vec![("GA".to_string(), 0..2), ("GB".to_string(), 3..5)];
        let processed = validate_groups(&groups, 5).unwrap();
        let entries = build_entries(&layers, &names, &groups, &processed, 5);

        // PSD bottom-to-top:
        // GroupEnd(GB) -> m4 -> m3 -> GroupStart(GB)
        // -> orphan(m2)
        // -> GroupEnd(GA) -> m1 -> m0 -> GroupStart(GA)
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
        // Even when groups are passed in reverse, they should be sorted by material_range.start ascending
        let dim = 2usize;
        let layers: Vec<Vec<u8>> = (0..4).map(|_| vec![0u8; dim * dim * 4]).collect();
        let names: Vec<String> = (0..4).map(|i| format!("m{i}")).collect();
        let groups = vec![("Second".to_string(), 2..4), ("First".to_string(), 0..2)];
        let processed = validate_groups(&groups, 4).unwrap();
        let entries = build_entries(&layers, &names, &groups, &processed, 4);

        // Second(2..4) is at the bottom, First(0..2) is on top
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
        // Verify the PSD byte stream for a single material in a single group
        let layers = vec![vec![0u8, 0, 0, 255]]; // 1x1 black pixel
        let names = vec!["mat".to_string()];
        let groups = vec![("G".to_string(), 0..1)];
        let processed = validate_groups(&groups, 1).unwrap();
        let entries = build_entries(&layers, &names, &groups, &processed, 1);

        let info = build_layer_info(1, 1, &entries, PsFormat::Psd).unwrap();

        // layer_count = 3 (GroupEnd + Content + GroupStart)
        let count = i16::from_be_bytes([info[0], info[1]]);
        assert_eq!(count, 3);

        // Find every lsct block and capture its type
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
        // GroupEnd (type=3) comes first, GroupStart (type=1) comes after
        assert_eq!(lsct_types, vec![3, 1]);
    }

    #[test]
    fn test_group_start_blend_mode() {
        // Verify that the GroupStart layer record uses the "pass" blend mode
        let layers = vec![vec![0u8; 4]];
        let names = vec!["m".to_string()];
        let groups = vec![("G".to_string(), 0..1)];
        let processed = validate_groups(&groups, 1).unwrap();
        let entries = build_entries(&layers, &names, &groups, &processed, 1);

        let info = build_layer_info(1, 1, &entries, PsFormat::Psd).unwrap();

        // "pass" should appear twice (in the GroupEnd and GroupStart layer records)
        let mut pass_count = 0;
        for i in 0..info.len().saturating_sub(4) {
            if &info[i..i + 4] == b"pass" {
                pass_count += 1;
            }
        }
        // GroupEnd(pass blend) + GroupStart(pass blend) + 2x pass inside lsct = 4 occurrences
        assert!(pass_count >= 4, "pass count: {pass_count}");
    }

    #[test]
    fn test_group_end_name() {
        // Verify that the GroupEnd layer name is "</Layer group>" (inside the luni block)
        let luni = build_luni_block("</Layer group>");
        let expected_utf16: Vec<u16> = "</Layer group>".encode_utf16().collect();
        // luni layout: "8BIM" + "luni" + len(4) + char_count(4) + UTF-16BE chars
        let char_count = u32::from_be_bytes([luni[12], luni[13], luni[14], luni[15]]);
        assert_eq!(char_count, expected_utf16.len() as u32);
    }

    // -- PSB / PSD format auto-promotion tests ----------------

    #[test]
    fn test_adjust_extension_for_format() {
        // .psd → .psd when staying as PSD
        let p = adjust_extension_for_format(Path::new("/tmp/foo.psd"), PsFormat::Psd);
        assert_eq!(p.extension().unwrap(), "psd");
        // .psd → .psb when promoted
        let p = adjust_extension_for_format(Path::new("/tmp/foo.psd"), PsFormat::Psb);
        assert_eq!(p.extension().unwrap(), "psb");
        // .psb → .psb (stable)
        let p = adjust_extension_for_format(Path::new("/tmp/foo.psb"), PsFormat::Psb);
        assert_eq!(p.extension().unwrap(), "psb");
        // No extension → appended
        let p = adjust_extension_for_format(Path::new("/tmp/foo"), PsFormat::Psd);
        assert_eq!(p.extension().unwrap(), "psd");
        // Unrelated extension → overwritten
        let p = adjust_extension_for_format(Path::new("/tmp/foo.txt"), PsFormat::Psb);
        assert_eq!(p.extension().unwrap(), "psb");
    }

    #[test]
    fn test_psd_header_signature_and_version() {
        // PSD: signature "8BPS", version 1
        let mut buf = Vec::new();
        let entries: Vec<PsdLayerEntry> = vec![PsdLayerEntry::Content {
            name: "m",
            rgba: &[0, 0, 0, 255],
        }];
        write_ps_file(&mut buf, 1, 1, &entries, PsFormat::Psd).unwrap();
        assert_eq!(&buf[0..4], b"8BPS");
        assert_eq!(u16::from_be_bytes([buf[4], buf[5]]), 1);
    }

    #[test]
    fn test_psb_header_signature_and_version() {
        // PSB: signature "8BPB", version 2
        let mut buf = Vec::new();
        let entries: Vec<PsdLayerEntry> = vec![PsdLayerEntry::Content {
            name: "m",
            rgba: &[0, 0, 0, 255],
        }];
        write_ps_file(&mut buf, 1, 1, &entries, PsFormat::Psb).unwrap();
        assert_eq!(&buf[0..4], b"8BPB");
        assert_eq!(u16::from_be_bytes([buf[4], buf[5]]), 2);
    }

    #[test]
    fn test_psb_layer_section_uses_8byte_length() {
        // PSD writes the layer section length as u32 (4 bytes after the 26-byte
        // header + 4-byte color mode length + 4-byte image resources length =
        // offset 34). PSB writes it as u64 (8 bytes at the same offset). When
        // we compare the same payload across both formats, the PSB output must
        // be at least 4 bytes longer (the additional length-field bytes).
        let entries: Vec<PsdLayerEntry> = vec![PsdLayerEntry::Content {
            name: "m",
            rgba: &[0, 0, 0, 255],
        }];
        let mut psd_buf = Vec::new();
        write_ps_file(&mut psd_buf, 1, 1, &entries, PsFormat::Psd).unwrap();
        let mut psb_buf = Vec::new();
        write_ps_file(&mut psb_buf, 1, 1, &entries, PsFormat::Psb).unwrap();
        // PSB has +4 bytes for the outer layer-section length, +4 for the
        // inner layer-info length, and +4 per channel record (4 channels × 1
        // layer = 16). Total expected delta = 4 + 4 + 16 = 24 bytes.
        assert_eq!(
            psb_buf.len(),
            psd_buf.len() + 24,
            "psd={} psb={}",
            psd_buf.len(),
            psb_buf.len()
        );
    }

    #[test]
    fn test_estimate_layer_section_bytes_grows_with_layers() {
        // The estimator must scale linearly with layer count and grow with
        // resolution. Used to drive the PSD → PSB promotion threshold.
        let one = vec![PsdLayerEntry::Content {
            name: "a",
            rgba: &[],
        }];
        let two = vec![
            PsdLayerEntry::Content {
                name: "a",
                rgba: &[],
            },
            PsdLayerEntry::Content {
                name: "b",
                rgba: &[],
            },
        ];
        let e1 = estimate_layer_section_bytes(4096, 4096, &one);
        let e2 = estimate_layer_section_bytes(4096, 4096, &two);
        assert!(e2 > e1, "two layers must estimate larger than one");
        // Doubling the resolution roughly quadruples the estimate.
        let e1_hi = estimate_layer_section_bytes(8192, 8192, &one);
        assert!(e1_hi > e1 * 3, "8k must be ~4x larger than 4k");
    }

    #[test]
    fn test_estimate_crosses_psb_threshold_at_realistic_payload() {
        // Sanity: 4096 × 4096 × ~30 layers should cross the PSB promotion
        // threshold (the silent-corruption boundary reported by users).
        let layers: Vec<PsdLayerEntry> = (0..30)
            .map(|_| PsdLayerEntry::Content {
                name: "m",
                rgba: &[],
            })
            .collect();
        let est = estimate_layer_section_bytes(4096, 4096, &layers);
        assert!(
            est > PSD_TO_PSB_THRESHOLD_BYTES,
            "expected estimate {} to exceed threshold {}",
            est,
            PSD_TO_PSB_THRESHOLD_BYTES
        );
        // Conversely, a single 4k layer must stay below the threshold.
        let one = vec![PsdLayerEntry::Content {
            name: "m",
            rgba: &[],
        }];
        let est_small = estimate_layer_section_bytes(4096, 4096, &one);
        assert!(
            est_small < PSD_TO_PSB_THRESHOLD_BYTES,
            "expected single-layer estimate {} to stay below threshold {}",
            est_small,
            PSD_TO_PSB_THRESHOLD_BYTES
        );
    }

    /// Read a big-endian section length that is `u32` for PSD and `u64` for
    /// PSB, returning the value and the number of bytes the field occupied.
    fn read_section_len(buf: &[u8], at: usize, format: PsFormat) -> (u64, usize) {
        match format {
            PsFormat::Psd => {
                let b: [u8; 4] = buf[at..at + 4].try_into().unwrap();
                (u32::from_be_bytes(b) as u64, 4)
            }
            PsFormat::Psb => {
                let b: [u8; 8] = buf[at..at + 8].try_into().unwrap();
                (u64::from_be_bytes(b), 8)
            }
        }
    }

    /// Round-trip the writer for both PSD and PSB on a small canvas and
    /// re-parse the produced bytes. v0.5.10 only had estimator unit tests; it
    /// never read the written bytes back, so a truncated PSB length field
    /// (the exact silent-corruption failure mode) would have gone unnoticed.
    /// A real 1.9 GiB write is infeasible here, but the structural invariant
    /// `section_start + declared_len + image_data_len == file_len` is
    /// format-independent: it fails the moment a length field overflows or is
    /// written at the wrong width, which is precisely the bug class.
    #[test]
    fn test_ps_file_roundtrip_psd_and_psb_parse_back() {
        const W: u32 = 8;
        const H: u32 = 8;
        let pixel_count = (W as usize) * (H as usize);
        let rgba = vec![0u8; pixel_count * 4];

        let entries = vec![
            PsdLayerEntry::GroupStart { name: "grp" },
            PsdLayerEntry::Content {
                name: "mat0",
                rgba: &rgba,
            },
            PsdLayerEntry::Content {
                name: "mat1",
                rgba: &rgba,
            },
            PsdLayerEntry::GroupEnd,
        ];

        let mut sizes = [0usize; 2];
        for (slot, format) in [PsFormat::Psd, PsFormat::Psb].into_iter().enumerate() {
            let mut buf: Vec<u8> = Vec::new();
            write_ps_file(&mut buf, W, H, &entries, format).expect("write_ps_file failed");

            // -- File header (26 bytes) --
            let (sig, ver): (&[u8; 4], u16) = match format {
                PsFormat::Psd => (b"8BPS", 1),
                PsFormat::Psb => (b"8BPB", 2),
            };
            assert_eq!(&buf[0..4], sig, "signature mismatch for {format:?}");
            assert_eq!(u16::from_be_bytes([buf[4], buf[5]]), ver, "version");
            assert_eq!(u16::from_be_bytes([buf[12], buf[13]]), 4, "channel count");
            assert_eq!(
                u32::from_be_bytes(buf[14..18].try_into().unwrap()),
                H,
                "height"
            );
            assert_eq!(
                u32::from_be_bytes(buf[18..22].try_into().unwrap()),
                W,
                "width"
            );
            assert_eq!(u16::from_be_bytes([buf[22], buf[23]]), 8, "depth");
            assert_eq!(u16::from_be_bytes([buf[24], buf[25]]), 3, "color mode RGB");

            // -- Color Mode Data (empty) + Image Resources (empty), u32 each --
            assert_eq!(
                u32::from_be_bytes(buf[26..30].try_into().unwrap()),
                0,
                "color mode data must be empty"
            );
            assert_eq!(
                u32::from_be_bytes(buf[30..34].try_into().unwrap()),
                0,
                "image resources must be empty"
            );

            // -- Layer and Mask Information --
            // Outer section length: u32 (PSD) / u64 (PSB).
            let (outer_len, outer_w) = read_section_len(&buf, 34, format);
            let section_start = 34 + outer_w;

            // Inner "Layer Info" length is also u32/u64, followed by the layer
            // info bytes and a 4-byte (empty) global layer mask info block.
            let (inner_len, inner_w) = read_section_len(&buf, section_start, format);
            assert_eq!(
                inner_w + inner_len as usize + 4,
                outer_len as usize,
                "inner layer-info length + global mask must fill the outer section ({format:?})"
            );

            // Image data has no length prefix; it runs to EOF as a 2-byte
            // compression marker plus 4 raw channels of `pixel_count` bytes.
            let image_data_len = 2 + 4 * pixel_count;

            // The corruption detector: a wrong-width or overflowed length
            // breaks this equality.
            assert_eq!(
                section_start + outer_len as usize + image_data_len,
                buf.len(),
                "declared layer-section length is inconsistent with file size ({format:?})"
            );
            assert_eq!(
                u16::from_be_bytes([
                    buf[section_start + outer_len as usize],
                    buf[section_start + outer_len as usize + 1]
                ]),
                0,
                "image data compression marker must be raw"
            );

            sizes[slot] = buf.len();
        }

        // PSB widens three classes of length field, so it must be strictly
        // larger than the PSD encoding of the identical layer set.
        assert!(
            sizes[1] > sizes[0],
            "PSB output ({}) must be larger than PSD ({}) for the same layers",
            sizes[1],
            sizes[0]
        );
    }
}
