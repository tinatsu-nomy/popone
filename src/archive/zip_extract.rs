//! ZIP archive extraction (two-pass: list, then selective extract).

use crate::error::{PoponeError, Result};
use rust_i18n::t;
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
        return Err(PoponeError::Archive(
            t!(
                "error.archive.filename_decode_failed",
                raw = format!("{raw:?}")
            )
            .to_string(),
        ));
    }
    Ok(decoded.into_owned())
}

/// Pass 1: collect entry metadata only (no data extraction).
/// Uses `by_index_raw` so listing also works on encrypted ZIPs
/// (ZIP encrypts entry payloads only; names and sizes stay readable).
pub fn list_entries(data: &[u8]) -> Result<Vec<ArchiveEntryMeta>> {
    let reader = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(reader)?;
    let mut entries = Vec::new();
    for i in 0..archive.len() {
        let file = archive.by_index_raw(i)?;
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
/// `password`: for encrypted entries (ZipCrypto / AES). Without it, hitting an
/// encrypted entry yields `ArchivePasswordRequired`; a wrong password yields
/// `ArchiveBadPassword` (ZipCrypto's 1-byte check may let a wrong password
/// through, in which case the CRC check fails as a generic error instead).
pub fn extract_files(
    data: &[u8],
    paths: &[&std::path::Path],
    max_total_bytes: u64,
    password: Option<&str>,
) -> Result<Vec<ArchiveEntry>> {
    let reader = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(reader)?;
    let mut results = Vec::new();
    let mut total = 0u64;

    for i in 0..archive.len() {
        // Resolve name/size/encryption via the raw handle first: `by_index` fails
        // on encrypted entries, and we must not fail on entries we never extract.
        let (norm_path, declared, encrypted) = {
            let file = archive.by_index_raw(i)?;
            if file.is_dir() {
                continue;
            }
            let name = decode_filename(&file)?;
            (
                normalize_archive_path(&name)?,
                file.size(),
                file.encrypted(),
            )
        };

        if !paths.iter().any(|p| *p == norm_path) {
            continue;
        }

        // Detect the need for a password from the entry flag (robust against the
        // various errors `by_index` produces for encrypted entries).
        if encrypted && password.is_none() {
            return Err(PoponeError::ArchivePasswordRequired);
        }

        // Size check (declared size)
        if total + declared > max_total_bytes {
            return Err(PoponeError::Archive(
                t!(
                    "error.archive.size_limit_exceeded",
                    total = total.to_string(),
                    size = declared.to_string(),
                    limit = max_total_bytes.to_string()
                )
                .to_string(),
            ));
        }

        // Only pass the password to entries that are actually encrypted:
        // `by_index_decrypt` on a plaintext entry runs the ZipCrypto validator
        // against plain data and fails (relevant for mixed ZIPs and for outer
        // plaintext ZIPs re-read with the password meant for a nested one).
        let file = match password {
            Some(pw) if encrypted => archive.by_index_decrypt(i, pw.as_bytes()),
            _ => archive.by_index(i),
        }
        .map_err(|e| match e {
            zip::result::ZipError::InvalidPassword => PoponeError::ArchiveBadPassword,
            other => PoponeError::from(other),
        })?;

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

/// Extract every entry matching the shared extension filter (nested archives
/// excluded -- this is the one-level expansion of an archive found *inside*
/// another archive, so going deeper is deliberately not supported).
/// `max_total_bytes`: total extraction size limit.
pub fn extract_filtered(
    data: &[u8],
    max_total_bytes: u64,
    password: Option<&str>,
) -> Result<Vec<ArchiveEntry>> {
    let metas = list_entries(data)?;
    let wanted: Vec<std::path::PathBuf> = metas
        .into_iter()
        .filter(|m| super::should_extract(&m.path, false))
        .map(|m| m.path)
        .collect();
    let refs: Vec<&std::path::Path> = wanted.iter().map(|p| p.as_path()).collect();
    extract_files(data, &refs, max_total_bytes, password)
}
