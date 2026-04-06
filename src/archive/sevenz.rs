//! 7z アーカイブの展開（フィルタ付き全展開）

use crate::error::Result;
use std::path::Path;

use super::{normalize_archive_path, ArchiveEntry, MODEL_EXTENSIONS, TEXTURE_EXTENSIONS};

/// 展開対象の拡張子かどうか
fn should_extract(path: &Path) -> bool {
    let ext = crate::path_ext_lower(path);
    MODEL_EXTENSIONS.contains(&ext.as_str())
        || TEXTURE_EXTENSIONS.contains(&ext.as_str())
        || ext == "txt"
        || ext == "spa"
        || ext == "sph"
        || ext == "mtl"
}

/// 7z を展開し、モデル/テクスチャ拡張子のみメモリ保持
/// max_total_bytes: 総展開サイズ上限
pub fn extract_filtered(data: &[u8], max_total_bytes: u64) -> Result<Vec<ArchiveEntry>> {
    let cursor = std::io::Cursor::new(data);
    let mut entries = Vec::new();
    let mut total = 0u64;

    // dest は使わない（コールバック内で自前処理するため）がAPIが要求する
    let dummy_dest = std::env::temp_dir();

    sevenz_rust2::decompress_with_extract_fn(cursor, &dummy_dest, |entry, reader, _dest_path| {
        let name = entry.name();
        let norm_path = match normalize_archive_path(name) {
            Ok(p) => p,
            Err(_) => return Ok(true), // 安全でないパスはスキップ
        };

        if entry.is_directory() {
            return Ok(true);
        }

        if !should_extract(&norm_path) {
            return Ok(true); // 不要なファイルはスキップ
        }

        let size = entry.size();
        // saturating_add でオーバーフロー安全な事前チェック
        if total.saturating_add(size) > max_total_bytes {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!(
                    "展開サイズ上限超過: {} + {} > {} bytes",
                    total, size, max_total_bytes
                ),
            )
            .into());
        }

        // 実読込もハード制限（ヘッダサイズ詐称対策）
        // dyn Read は Sized でないため take() が使えない → チャンク読みで制限
        let remaining = max_total_bytes - total;
        // ヘッダ size を信用せず、remaining を上限とした安全な容量で確保
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
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!(
                        "展開サイズ上限超過（実読込）: {} + {} > {} bytes",
                        total, read_total, max_total_bytes
                    ),
                )
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
