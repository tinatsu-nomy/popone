//! 7z archive extraction (full extract, filtered by extension).

use crate::error::Result;
use std::path::Path;

use super::{normalize_archive_path, ArchiveEntry, MODEL_EXTENSIONS, TEXTURE_EXTENSIONS};

/// Whether the extension is one we want to extract.
fn should_extract(path: &Path) -> bool {
    let ext = crate::path_ext_lower(path);
    MODEL_EXTENSIONS.contains(&ext.as_str())
        || TEXTURE_EXTENSIONS.contains(&ext.as_str())
        || ext == "txt"
        || ext == "spa"
        || ext == "sph"
        || ext == "mtl"
}

/// Extract a 7z archive, keeping only model/texture extensions in memory.
/// `max_total_bytes`: total extraction size limit.
pub fn extract_filtered(data: &[u8], max_total_bytes: u64) -> Result<Vec<ArchiveEntry>> {
    let cursor = std::io::Cursor::new(data);
    let mut entries = Vec::new();
    let mut total = 0u64;

    // `dest` is unused (we handle bytes inside the callback) but required by the API.
    let dummy_dest = std::env::temp_dir();

    sevenz_rust2::decompress_with_extract_fn(cursor, &dummy_dest, |entry, reader, _dest_path| {
        let name = entry.name();
        let norm_path = match normalize_archive_path(name) {
            Ok(p) => p,
            Err(_) => return Ok(true), // skip unsafe paths
        };

        if entry.is_directory() {
            return Ok(true);
        }

        if !should_extract(&norm_path) {
            return Ok(true); // skip unwanted files
        }

        let size = entry.size();
        // Overflow-safe pre-check with saturating_add
        if total.saturating_add(size) > max_total_bytes {
            return Err(std::io::Error::other(format!(
                "展開サイズ上限超過: {} + {} > {} bytes",
                total, size, max_total_bytes
            ))
            .into());
        }

        // Hard-limit the actual read too (defense against spoofed header sizes).
        // `dyn Read` is not Sized so `take()` is unavailable; cap via chunked reads instead.
        let remaining = max_total_bytes - total;
        // Do not trust the header size; cap allocation at `remaining` bytes.
        let safe_capacity = std::cmp::min(size, remaining) as usize;
        let mut buf = Vec::with_capacity(safe_capacity);
        let mut read_total = 0u64;
        let mut chunk = [0u8; 65536];
        loop {
            let n = reader.read(&mut chunk)?;
            if n == 0 {
                break;
            }
            read_total += n as u64;
            if read_total > remaining {
                return Err(std::io::Error::other(format!(
                    "展開サイズ上限超過（実読込）: {} + {} > {} bytes",
                    total, read_total, max_total_bytes
                ))
                .into());
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        total += read_total;

        entries.push(ArchiveEntry {
            path: norm_path,
            data: buf,
        });
        Ok(true)
    })?;

    Ok(entries)
}
