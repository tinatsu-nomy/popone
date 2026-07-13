//! RAR archive extraction (full extract, filtered by extension).
//!
//! Uses the official UnRAR library via the `unrar` crate. RAR archives are
//! often solid (like 7z), so we extract every matching entry in one pass and
//! keep the results in memory. The UnRAR API is path-based, so in-memory
//! archive data is staged through a temporary file.

use crate::error::{PoponeError, Result};
use rust_i18n::t;
use std::io::Write;

use super::{normalize_archive_path, ArchiveEntry};

/// Extract a RAR archive, keeping only model/texture extensions in memory.
/// `max_total_bytes`: total extraction size limit.
/// `password`: for encrypted archives (both header and content encryption).
/// `include_archives`: also keep nested archives (zip/7z/rar) for one-level
/// nested extraction; pass false when this call itself extracts a nested one.
pub fn extract_filtered(
    data: &[u8],
    max_total_bytes: u64,
    password: Option<&str>,
    include_archives: bool,
) -> Result<Vec<ArchiveEntry>> {
    // UnRAR only accepts file paths; stage the bytes in a temp file.
    let mut tmp = tempfile::Builder::new().suffix(".rar").tempfile()?;
    tmp.write_all(data)?;
    tmp.flush()?;
    let tmp_path = tmp.path().to_path_buf();

    let archive = match password {
        Some(pw) => unrar::Archive::with_password(&tmp_path, pw),
        None => unrar::Archive::new(&tmp_path),
    };

    let mut entries = Vec::new();
    let mut total = 0u64;

    // Header-encrypted archives fail here with MissingPassword / BadPassword,
    // which `From<UnrarError>` maps to the dedicated password variants.
    let mut open = archive.open_for_processing()?;
    while let Some(cursor) = open.read_header()? {
        let header = cursor.entry();

        if header.is_directory() {
            open = cursor.skip()?;
            continue;
        }

        let norm_path = match normalize_archive_path(&header.filename.to_string_lossy()) {
            Ok(p) => p,
            Err(_) => {
                // Skip unsafe paths
                open = cursor.skip()?;
                continue;
            }
        };

        if !super::should_extract(&norm_path, include_archives) {
            open = cursor.skip()?;
            continue;
        }

        // Content-encrypted entry without a password: report before UnRAR
        // fails the read, so the viewer can prompt for input.
        if header.is_encrypted() && password.is_none() {
            return Err(PoponeError::ArchivePasswordRequired);
        }

        let size = header.unpacked_size;
        if total.saturating_add(size) > max_total_bytes {
            return Err(PoponeError::Archive(
                t!(
                    "error.archive.size_limit_exceeded",
                    total = total.to_string(),
                    size = size.to_string(),
                    limit = max_total_bytes.to_string()
                )
                .to_string(),
            ));
        }

        // RAR4 has no password check value; a wrong password often surfaces as
        // BadData (CRC mismatch) here instead of BadPassword. RAR5 reports
        // BadPassword properly. Both fail the load either way.
        let (bytes, next) = cursor.read()?;

        // Defense against spoofed header sizes: re-check with the actual length.
        total = total.saturating_add(bytes.len() as u64);
        if total > max_total_bytes {
            return Err(PoponeError::Archive(
                t!(
                    "error.archive.size_limit_exceeded_actual",
                    total = (total - bytes.len() as u64).to_string(),
                    actual = bytes.len().to_string(),
                    limit = max_total_bytes.to_string()
                )
                .to_string(),
            ));
        }

        entries.push(ArchiveEntry {
            path: norm_path,
            data: bytes,
        });
        open = next;
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixtures from the unrar crate (muja/unrar.rs, MIT OR Apache-2.0).
    // Their single entry is `.gitignore`, which our extension filter skips --
    // the tests below exercise archive opening and header decryption, not payloads.
    const PLAIN_RAR: &[u8] = include_bytes!("../../tests/data/version.rar");
    const CONTENT_ENCRYPTED_RAR: &[u8] = include_bytes!("../../tests/data/crypted.rar");
    const HEADER_ENCRYPTED_RAR: &[u8] = include_bytes!("../../tests/data/comment-hpw-password.rar");

    const NO_LIMIT: u64 = u64::MAX;

    #[test]
    fn plain_rar_opens_without_password() {
        let entries = extract_filtered(PLAIN_RAR, NO_LIMIT, None, true).unwrap();
        // No model/texture extensions inside -> empty, but the archive iterates cleanly.
        assert!(entries.is_empty());
    }

    #[test]
    fn header_encrypted_without_password_reports_password_required() {
        let Err(err) = extract_filtered(HEADER_ENCRYPTED_RAR, NO_LIMIT, None, true) else {
            panic!("header-encrypted RAR without a password must fail");
        };
        assert!(
            matches!(err, PoponeError::ArchivePasswordRequired),
            "expected ArchivePasswordRequired, got: {err:?}"
        );
    }

    #[test]
    fn header_encrypted_with_password_opens() {
        let entries =
            extract_filtered(HEADER_ENCRYPTED_RAR, NO_LIMIT, Some("password"), true).unwrap();
        // Header decryption succeeded; the only entry (`.gitignore`) is filtered out.
        assert!(entries.is_empty());
    }

    #[test]
    fn header_encrypted_with_wrong_password_fails() {
        // RAR4 has no password-check value, so a wrong password may surface as
        // BadPassword or as generic corruption -- either way it must not succeed.
        let result = extract_filtered(HEADER_ENCRYPTED_RAR, NO_LIMIT, Some("wrong-password"), true);
        assert!(result.is_err());
    }

    #[test]
    fn content_encrypted_non_model_entries_are_skipped() {
        // Content-encrypted entries that our filter skips never trigger a prompt.
        let entries = extract_filtered(CONTENT_ENCRYPTED_RAR, NO_LIMIT, None, true).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn broken_rar_reports_error() {
        assert!(extract_filtered(b"this is not a rar file", NO_LIMIT, None, true).is_err());
    }
}
