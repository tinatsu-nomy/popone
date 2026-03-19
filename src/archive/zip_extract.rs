//! ZIP アーカイブの展開（2パス: 一覧→選択展開）

use anyhow::{bail, Result};
use std::io::Read;

use super::{ArchiveEntry, ArchiveEntryMeta, normalize_archive_path};

/// ZIP エントリのファイル名を取得（UTF-8 → Shift_JIS フォールバック）
fn decode_filename(file: &zip::read::ZipFile) -> Result<String> {
    let raw = file.name_raw();
    // UTF-8 として試行
    if let Ok(s) = std::str::from_utf8(raw) {
        return Ok(s.to_string());
    }
    // Shift_JIS フォールバック
    let (decoded, _, had_errors) = encoding_rs::SHIFT_JIS.decode(raw);
    if had_errors {
        bail!("ファイル名のデコードに失敗: {:?}", raw);
    }
    Ok(decoded.into_owned())
}

/// Pass 1: メタデータのみ取得（データ展開なし）
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

/// Pass 2: 指定パスのファイル群のみ展開
/// max_total_bytes: 総展開サイズ上限（zip bomb 対策）
pub fn extract_files(data: &[u8], paths: &[&std::path::Path], max_total_bytes: u64) -> Result<Vec<ArchiveEntry>> {
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

        // サイズチェック（declared size）
        let declared = file.size();
        if total + declared > max_total_bytes {
            bail!("展開サイズ上限超過: {} + {} > {} bytes", total, declared, max_total_bytes);
        }

        // 実際の読み込み（take でハード制限）
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
