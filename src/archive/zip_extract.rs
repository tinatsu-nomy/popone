//! ZIP archive extraction (two-pass: list, then selective extract).

use crate::error::{PoponeError, Result};
use std::io::Read;

use super::{normalize_archive_path, ArchiveEntry, ArchiveEntryMeta};

/// Decode a ZIP entry filename (UTF-8 first, Shift_JIS as fallback).
fn decode_filename(file: &zip::read::ZipFile) -> Result<String> {
    let raw = file.name_raw();
    // Try UTF-8 first
    if let Ok(s) = std::str::from_utf8(raw) {
        return Ok(s.to_string());
    }
    // Shift_JIS fallback
    let (decoded, _, had_errors) = encoding_rs::SHIFT_JIS.decode(raw);
    if had_errors {
        return Err(PoponeError::Archive(format!(
            "ファイル名のデコードに失敗: {raw:?}"
        )));
    }
    Ok(decoded.into_owned())
}

/// Pass 1: collect entry metadata only (no data extraction).
pub fn list_entries(data: &[u8]) -> Result<Vec<ArchiveEntryMeta>> {
    let reader = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(reader)?;
    let mut entries = Vec::new();
    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        if file.is_dir() {
            continue;
        }
        let name = decode_filename(&file)?;
        let path = normalize_archive_path(&name)?;
        entries.push(ArchiveEntryMeta {
            path,
            size: file.size(),
        });
    }
    Ok(entries)
}

/// Pass 2: extract only the files matching the given paths.
/// `max_total_bytes`: total extraction size limit (zip-bomb defense).
pub fn extract_files(
    data: &[u8],
    paths: &[&std::path::Path],
    max_total_bytes: u64,
) -> Result<Vec<ArchiveEntry>> {
    let reader = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(reader)?;
    let mut results = Vec::new();
    let mut total = 0u64;

    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        if file.is_dir() {
            continue;
        }
        let name = decode_filename(&file)?;
        let norm_path = normalize_archive_path(&name)?;

        if !paths.iter().any(|p| *p == norm_path) {
            continue;
        }

        // Size check (declared size)
        let declared = file.size();
        if total + declared > max_total_bytes {
            return Err(PoponeError::Archive(format!(
                "展開サイズ上限超過: {total} + {declared} > {max_total_bytes} bytes"
            )));
        }

        // Actual read; cap with `take` as a hard limit
        let limit = max_total_bytes - total;
        let mut buf = Vec::with_capacity(declared as usize);
        file.take(limit).read_to_end(&mut buf)?;
        total += buf.len() as u64;

        results.push(ArchiveEntry {
            path: norm_path,
            data: buf,
        });
    }
    Ok(results)
}
