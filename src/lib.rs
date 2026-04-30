pub mod archive;
pub mod color;
pub mod convert;
pub mod directx;
pub mod error;
pub mod fbx;
pub mod i18n;
pub mod intermediate;
pub mod obj;
pub mod pmd;
pub mod pmx;
pub mod psd;
pub mod stl;
pub mod unity;
pub mod unitypackage;
pub mod vrm;

#[cfg(feature = "viewer")]
pub mod viewer;

// Embed translations (`locales/{ja,en,zh}.yml`) into the binary at compile time.
// English is used as the fallback when a key is missing in the active locale.
rust_i18n::i18n!("locales", fallback = "en");

use std::path::{Path, PathBuf};

/// Return the path extension lowercased (empty string when missing or not valid UTF-8).
pub fn path_ext_lower(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
}

/// Normalize a relative path and prevent directory traversal.
/// Converts backslashes to slashes and resolves `.` / `..`.
/// `..` segments that would escape the root are dropped (a warning is logged).
/// Windows drive letters (e.g. `C:`) are also stripped to keep absolute paths
/// from escaping the base directory.
pub fn sanitize_rel_path(raw: &str) -> PathBuf {
    let s = raw.replace('\\', "/");
    let mut out: Vec<&str> = Vec::new();
    let mut traversal_blocked = false;
    for component in s.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                if out.pop().is_none() {
                    traversal_blocked = true;
                }
            }
            // Strip drive letters like "C:" (prevents absolute-path escapes of the base dir)
            c if c.contains(':') => {
                traversal_blocked = true;
            }
            c => out.push(c),
        }
    }
    if traversal_blocked {
        log::warn!("Path traversal blocked: {}", raw);
    }
    PathBuf::from(out.join("/"))
}

/// In-memory log buffer with a cap and a cumulative-offset cursor.
/// Backed by `VecDeque` so head-trimming (drain) stays O(1).
pub struct LogBuffer {
    pub data: std::collections::VecDeque<u8>,
    /// Cumulative bytes written (never decreases on drain).
    pub total_written: usize,
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl LogBuffer {
    pub fn new() -> Self {
        Self {
            data: std::collections::VecDeque::new(),
            total_written: 0,
        }
    }

    /// Read data from cumulative offset `offset` onwards.
    /// Already-drained ranges are clipped; we return whatever remains in the buffer.
    pub fn read_from_offset(&self, offset: usize) -> Option<String> {
        let drained = self.total_written - self.data.len();
        let start = if offset <= drained {
            0 // Requested range is already drained -> start at the buffer head
        } else if offset >= self.total_written {
            return None; // Nothing written in this range yet
        } else {
            offset - drained
        };
        let (front, back) = self.data.as_slices();
        let total_len = self.data.len();
        if start >= total_len {
            return None;
        }
        // VecDeque is internally backed by two contiguous slices
        let bytes: std::borrow::Cow<'_, [u8]> = if start < front.len() {
            if back.is_empty() {
                std::borrow::Cow::Borrowed(&front[start..])
            } else {
                std::borrow::Cow::Owned([&front[start..], back].concat())
            }
        } else {
            std::borrow::Cow::Borrowed(&back[start - front.len()..])
        };
        if bytes.is_empty() {
            None
        } else {
            Some(String::from_utf8_lossy(&bytes).into_owned())
        }
    }
}

/// Shared handle to a log buffer.
pub type SharedLogBuffer = std::sync::Arc<std::sync::Mutex<LogBuffer>>;

use error::Result;
use pmx::build::PmxBuildOptions;
use serde::Serialize;

#[derive(Serialize, Debug)]
pub struct ConvertStats {
    pub output_path: String,
    pub tex_dir: String,
    pub bones: usize,
    pub vertices: usize,
    pub faces: usize,
    pub materials: usize,
    pub textures: usize,
    pub morphs: usize,
}

/// Options for the VRM -> PMX conversion.
#[derive(Debug, Clone)]
pub struct VrmConvertOptions {
    /// Skip writing physics data (rigid bodies and joints).
    pub no_physics: bool,
    /// Align rigid-body rotations with the bone direction.
    pub align_rigid_rotation: bool,
    /// Normalize the pose to A-stance.
    pub normalize_pose: bool,
    /// Skip standard-bone insertion (preserve the original bone structure).
    pub raw_structure: bool,
    /// PMX output scale multiplier (default: 1.0).
    pub scale: f32,
}

impl Default for VrmConvertOptions {
    fn default() -> Self {
        Self {
            no_physics: false,
            align_rigid_rotation: false,
            normalize_pose: false,
            raw_structure: false,
            scale: 1.0,
        }
    }
}

pub fn convert_vrm_to_pmx(
    input_path: &Path,
    output_path: &Path,
    options: &VrmConvertOptions,
) -> Result<ConvertStats> {
    let glb = vrm::loader::load_glb(input_path)?;
    let version = vrm::detect::detect_version(&glb.document);
    let all_extensions = vrm::loader::get_raw_extensions(&glb.document);

    let ir = vrm::extract::extract_ir_model_with_options(
        &glb.document,
        &glb.buffers,
        &glb.images,
        &glb.vrm_extension,
        &version,
        &all_extensions,
        options.normalize_pose,
    )?;

    let output_dir = output_path.parent().unwrap_or(Path::new("."));
    let tex_dir = output_dir.join("textures");
    convert::texture::write_all_textures(&ir.textures, &glb.images, &tex_dir)?;

    let build_options = PmxBuildOptions {
        align_rigid_rotation: options.align_rigid_rotation,
        no_physics: options.no_physics,
        raw_structure: options.raw_structure,
        scale: options.scale,
    };
    let (mut pmx_model, toon_textures) =
        pmx::build::build_pmx_model_with_options(&ir, &build_options)?;
    let toon_written = convert::texture::write_all_textures_from_ir(&toon_textures, &tex_dir)?;
    // Patch PMX texture paths with the actual generated toon-texture filenames
    let base_tex_count = ir.textures.len();
    for (i, name) in toon_written.iter().enumerate() {
        let pmx_idx = base_tex_count + i;
        if pmx_idx < pmx_model.textures.len() {
            pmx_model.textures[pmx_idx] = format!("textures\\{}", name);
        }
    }
    write_pmx_and_stats(&pmx_model, output_path, &tex_dir, None)
}

/// FBX -> PMX conversion.
pub fn convert_fbx_to_pmx(
    input_path: &Path,
    output_path: &Path,
    options: &VrmConvertOptions,
) -> Result<ConvertStats> {
    let data = std::fs::read(input_path)?;
    let ir = fbx::extract::extract_ir_model_from_fbx_with_options(
        &data,
        Some(input_path),
        options.normalize_pose,
        false,
    )?;
    let build_options = PmxBuildOptions {
        align_rigid_rotation: options.align_rigid_rotation,
        no_physics: options.no_physics,
        raw_structure: options.raw_structure,
        scale: options.scale,
    };
    convert_ir_to_pmx(&ir, output_path, &build_options)
}

/// OBJ -> PMX conversion.
pub fn convert_obj_to_pmx(
    input_path: &Path,
    output_path: &Path,
    options: &VrmConvertOptions,
) -> Result<ConvertStats> {
    let ir = obj::extract::load_obj(input_path)?;
    let build_options = PmxBuildOptions {
        align_rigid_rotation: options.align_rigid_rotation,
        no_physics: options.no_physics,
        raw_structure: options.raw_structure,
        scale: options.scale,
    };
    convert_ir_to_pmx(&ir, output_path, &build_options)
}

/// STL -> PMX conversion.
pub fn convert_stl_to_pmx(
    input_path: &Path,
    output_path: &Path,
    options: &VrmConvertOptions,
) -> Result<ConvertStats> {
    let ir = stl::extract::load_stl(input_path)?;
    let build_options = PmxBuildOptions {
        align_rigid_rotation: options.align_rigid_rotation,
        no_physics: options.no_physics,
        raw_structure: options.raw_structure,
        scale: options.scale,
    };
    convert_ir_to_pmx(&ir, output_path, &build_options)
}

/// DirectX `.x` -> PMX conversion.
pub fn convert_x_to_pmx(
    input_path: &Path,
    output_path: &Path,
    options: &VrmConvertOptions,
) -> Result<ConvertStats> {
    let ir = directx::extract::load_x(input_path)?;
    let build_options = PmxBuildOptions {
        align_rigid_rotation: options.align_rigid_rotation,
        no_physics: options.no_physics,
        raw_structure: options.raw_structure,
        scale: options.scale,
    };
    convert_ir_to_pmx(&ir, output_path, &build_options)
}

/// Convert directly from an `IrModel` to PMX (used for IrModels already edited in the viewer).
pub fn convert_ir_to_pmx(
    ir: &intermediate::types::IrModel,
    output_path: &Path,
    options: &PmxBuildOptions,
) -> Result<ConvertStats> {
    let output_dir = output_path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(output_dir)?;
    let tex_dir = output_dir.join("textures");
    let written_filenames = convert::texture::write_all_textures_from_ir(&ir.textures, &tex_dir)?;

    let (mut pmx_model, toon_textures) = pmx::build::build_pmx_model_with_options(ir, options)?;
    // Patch PMX texture paths when PSD->PNG conversion changed the filename
    for (i, name) in written_filenames.iter().enumerate() {
        if i < pmx_model.textures.len() {
            pmx_model.textures[i] = format!("textures\\{}", name);
        }
    }
    // Write generated toon textures to disk and patch the PMX paths
    let base_tex_count = ir.textures.len();
    let toon_written = convert::texture::write_all_textures_from_ir(&toon_textures, &tex_dir)?;
    for (i, name) in toon_written.iter().enumerate() {
        let pmx_idx = base_tex_count + i;
        if pmx_idx < pmx_model.textures.len() {
            pmx_model.textures[pmx_idx] = format!("textures\\{}", name);
        }
    }
    write_pmx_and_stats(&pmx_model, output_path, &tex_dir, None)
}

/// Convert `IrModel` -> PMX with cooperative cancellation.
/// Writes everything into a temporary directory and only moves to the final path on success.
/// On cancel the entire temp directory is removed, so no partial files are left behind.
pub fn convert_ir_to_pmx_with_cancel(
    ir: &intermediate::types::IrModel,
    output_path: &Path,
    options: &PmxBuildOptions,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<ConvertStats> {
    use std::sync::atomic::Ordering;

    let check = || -> Result<()> {
        if cancel.load(Ordering::Relaxed) {
            return Err(error::PoponeError::Other("PMX conversion cancelled".into()));
        }
        Ok(())
    };

    let output_dir = output_path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(output_dir)?;

    // Drop guard: removes the temp directory when the scope exits (disarmed on success)
    let tmp_dir = output_dir.join(".popone_convert_tmp");
    if tmp_dir.exists() {
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
    std::fs::create_dir_all(&tmp_dir)?;
    let mut guard = TmpDirGuard::new(tmp_dir.clone());

    let tmp_tex_dir = tmp_dir.join("textures");
    let tmp_pmx_path = tmp_dir.join(
        output_path
            .file_name()
            .unwrap_or(std::ffi::OsStr::new("output.pmx")),
    );

    check()?;
    let written_filenames = convert::texture::write_all_textures_from_ir_opt_cancel(
        &ir.textures,
        &tmp_tex_dir,
        Some(cancel),
    )?;

    check()?;
    let (mut pmx_model, toon_textures) = pmx::build::build_pmx_model_with_options(ir, options)?;
    for (i, name) in written_filenames.iter().enumerate() {
        if i < pmx_model.textures.len() {
            pmx_model.textures[i] = format!("textures\\{}", name);
        }
    }

    check()?;
    let base_tex_count = ir.textures.len();
    let toon_written = convert::texture::write_all_textures_from_ir_opt_cancel(
        &toon_textures,
        &tmp_tex_dir,
        Some(cancel),
    )?;
    for (i, name) in toon_written.iter().enumerate() {
        let pmx_idx = base_tex_count + i;
        if pmx_idx < pmx_model.textures.len() {
            pmx_model.textures[pmx_idx] = format!("textures\\{}", name);
        }
    }

    check()?;
    let mut stats = write_pmx_and_stats(&pmx_model, &tmp_pmx_path, &tmp_tex_dir, Some(cancel))?;

    // Final cancel check after PMX write, before committing to output path
    check()?;

    // Success path: disarm the guard so the temp directory survives
    guard.disarm();

    // Move from the temp directory to the final output location
    let final_tex_dir = output_dir.join("textures");
    if tmp_tex_dir.exists() {
        std::fs::create_dir_all(&final_tex_dir)?;
        if let Ok(entries) = std::fs::read_dir(&tmp_tex_dir) {
            for entry in entries.flatten() {
                let dest = final_tex_dir.join(entry.file_name());
                if std::fs::rename(entry.path(), &dest).is_err() {
                    let _ = std::fs::copy(entry.path(), &dest);
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
    if std::fs::rename(&tmp_pmx_path, output_path).is_err() {
        let _ = std::fs::copy(&tmp_pmx_path, output_path);
        let _ = std::fs::remove_file(&tmp_pmx_path);
    }
    let _ = std::fs::remove_dir_all(&tmp_dir);

    stats.output_path = output_path.to_string_lossy().into_owned();
    stats.tex_dir = final_tex_dir.to_string_lossy().into_owned();
    Ok(stats)
}

/// RAII guard that removes a temporary directory on drop unless disarmed.
struct TmpDirGuard {
    path: Option<std::path::PathBuf>,
}

impl TmpDirGuard {
    fn new(path: std::path::PathBuf) -> Self {
        Self { path: Some(path) }
    }

    fn disarm(&mut self) {
        self.path = None;
    }
}

impl Drop for TmpDirGuard {
    fn drop(&mut self) {
        if let Some(ref p) = self.path {
            let _ = std::fs::remove_dir_all(p);
        }
    }
}

/// Write a PMX model to a file and return `ConvertStats` (shared helper).
fn write_pmx_and_stats(
    pmx_model: &pmx::types::PmxModel,
    output_path: &Path,
    tex_dir: &Path,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Result<ConvertStats> {
    let stats = ConvertStats {
        output_path: output_path.to_string_lossy().into_owned(),
        tex_dir: tex_dir.to_string_lossy().into_owned(),
        bones: pmx_model.bones.len(),
        vertices: pmx_model.vertices.len(),
        faces: pmx_model.faces.len(),
        materials: pmx_model.materials.len(),
        textures: pmx_model.textures.len(),
        morphs: pmx_model.morphs.len(),
    };

    let file = std::fs::File::create(output_path)?;
    let writer = std::io::BufWriter::new(file);
    let header = pmx_model.header.clone();
    let mut pmx_writer = pmx::writer::PmxWriter::new(writer, header);
    pmx_writer.write_model_opt_cancel(pmx_model, cancel)?;

    Ok(stats)
}

/// When `base_dir` is already inside `converted_modelXX`, return its parent (prevents nesting).
pub fn resolve_converted_base(dir: &Path) -> &Path {
    if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
        if name.starts_with("converted_model")
            && name["converted_model".len()..]
                .chars()
                .all(|c| c.is_ascii_digit())
        {
            return dir.parent().unwrap_or(dir);
        }
    }
    dir
}

/// Find the next free `converted_modelXX` directory number (no upper bound).
pub fn next_converted_dir(base_dir: &Path) -> std::path::PathBuf {
    let base_dir = resolve_converted_base(base_dir);
    for i in 1.. {
        let dir = base_dir.join(format!("converted_model{:02}", i));
        if !dir.exists() {
            return dir;
        }
    }
    unreachable!()
}

/// Sanitize a model name into a filesystem-safe filename.
/// Handles Windows-illegal characters and reserved names; returns None if the result is empty.
pub fn sanitize_filename(name: &str) -> Option<String> {
    const INVALID_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if name.is_empty() {
        return None;
    }
    // Replace illegal characters with `_`
    let sanitized: String = name
        .chars()
        .map(|c| {
            if INVALID_CHARS.contains(&c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    // Strip trailing spaces and periods
    let trimmed = sanitized.trim_end_matches([' ', '.']);
    if trimmed.is_empty() {
        return None;
    }
    // Truncate names that are too long (cutting at a safe char boundary)
    const MAX_FILENAME_CHARS: usize = 80;
    let trimmed = if trimmed.chars().count() > MAX_FILENAME_CHARS {
        let end = trimmed
            .char_indices()
            .nth(MAX_FILENAME_CHARS)
            .map_or(trimmed.len(), |(i, _)| i);
        // After truncation, strip trailing spaces / periods again
        trimmed[..end].trim_end_matches([' ', '.'])
    } else {
        trimmed
    };
    // Windows reserved-name check (compares the basename, i.e. the part before the first '.')
    let base = match trimmed.find('.') {
        Some(pos) => &trimmed[..pos],
        None => trimmed,
    };
    let base_upper = base.to_uppercase();
    if RESERVED.contains(&base_upper.as_str()) {
        return Some(format!("_{}", trimmed));
    }
    Some(trimmed.to_string())
}

/// Detect the input format from the extension (VRM/FBX/...) and convert to PMX.
pub fn convert_to_pmx(
    input_path: &Path,
    output_path: &Path,
    options: &VrmConvertOptions,
) -> Result<ConvertStats> {
    let ext = path_ext_lower(input_path);
    match ext.as_str() {
        "fbx" => convert_fbx_to_pmx(input_path, output_path, options),
        "obj" => convert_obj_to_pmx(input_path, output_path, options),
        "stl" => convert_stl_to_pmx(input_path, output_path, options),
        "x" => convert_x_to_pmx(input_path, output_path, options),
        _ => convert_vrm_to_pmx(input_path, output_path, options),
    }
}

/// Test utilities (resolves paths to test fixtures).
///
/// Per-file resolution priority:
///   1. File-specific environment variable (e.g. `POPONE_TEST_VRM_SEED_SAN`).
///   2. Root environment variable `POPONE_TEST_DATA` + relative path.
///   3. `CARGO_MANIFEST_DIR/..` + relative path (the default for local development).
///
/// Example CI configuration:
/// ```sh
/// # Root override (shared base for every fixture)
/// export POPONE_TEST_DATA=/path/to/test-fixtures
///
/// # Or per-file overrides (when a single fixture lives somewhere else)
/// export POPONE_TEST_VRM_SEED_SAN=/data/models/Seed-san.vrm
/// export POPONE_TEST_PMX_SEED_SAN=/data/converted/Seed-san.pmx
/// export POPONE_TEST_PMD_MIKU_V2=/data/mmd/初音ミクVer2.pmd
/// ```
#[cfg(test)]
pub mod test_util {
    use std::path::PathBuf;

    /// Root directory for test fixtures.
    fn test_data_root() -> PathBuf {
        if let Ok(dir) = std::env::var("POPONE_TEST_DATA") {
            return PathBuf::from(dir);
        }
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
    }

    /// Resolve via the file-specific env var first, then via the root + relative path.
    fn resolve(env_key: &str, relative: &str) -> PathBuf {
        if let Ok(path) = std::env::var(env_key) {
            return PathBuf::from(path);
        }
        test_data_root().join(relative)
    }

    /// VRM sample (in the vrm-c/vrm-specification repository).
    /// Environment variable: `POPONE_TEST_VRM_SEED_SAN`.
    pub fn seed_san_vrm() -> PathBuf {
        resolve(
            "POPONE_TEST_VRM_SEED_SAN",
            "vrm-spec/vrm-specification/samples/Seed-san/vrm/Seed-san.vrm",
        )
    }

    /// PMX test fixture (Seed-san.vrm already converted with popone).
    /// Environment variable: `POPONE_TEST_PMX_SEED_SAN`.
    pub fn seed_san_pmx() -> PathBuf {
        resolve("POPONE_TEST_PMX_SEED_SAN", "tmp/Seed-san.pmx")
    }

    /// PMD test fixture (bundled with MikuMikuDance_v932x64.zip).
    /// Environment variable: `POPONE_TEST_PMD_MIKU_V2`.
    pub fn miku_v2_pmd() -> PathBuf {
        resolve("POPONE_TEST_PMD_MIKU_V2", "tmp/pmd/初音ミクVer2.pmd")
    }

    /// Return Some(path) when the test file exists, None otherwise.
    pub fn try_test_file(path: PathBuf) -> Option<PathBuf> {
        if path.exists() {
            Some(path)
        } else {
            eprintln!(
                "テストファイルが存在しません（スキップ）: {}",
                path.display()
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_rel_path_normal() {
        assert_eq!(
            sanitize_rel_path("textures/body.png"),
            PathBuf::from("textures/body.png")
        );
    }

    #[test]
    fn test_sanitize_rel_path_backslash() {
        assert_eq!(
            sanitize_rel_path("textures\\body.png"),
            PathBuf::from("textures/body.png")
        );
    }

    #[test]
    fn test_sanitize_rel_path_dot() {
        assert_eq!(
            sanitize_rel_path("./textures/body.png"),
            PathBuf::from("textures/body.png")
        );
    }

    #[test]
    fn test_sanitize_rel_path_dotdot_resolved() {
        // "a/../b" -> "b" (legitimate ".." resolution)
        assert_eq!(sanitize_rel_path("a/../b.png"), PathBuf::from("b.png"));
    }

    #[test]
    fn test_sanitize_rel_path_traversal_blocked() {
        // ".." segments that escape the root are dropped
        assert_eq!(
            sanitize_rel_path("../../../etc/passwd"),
            PathBuf::from("etc/passwd")
        );
    }

    #[test]
    fn test_sanitize_rel_path_traversal_mixed() {
        assert_eq!(
            sanitize_rel_path("a/../../secret.txt"),
            PathBuf::from("secret.txt")
        );
    }

    #[test]
    fn test_sanitize_rel_path_empty() {
        assert_eq!(sanitize_rel_path(""), PathBuf::from(""));
    }

    #[test]
    fn test_sanitize_rel_path_only_dotdot() {
        assert_eq!(sanitize_rel_path(".."), PathBuf::from(""));
    }

    #[test]
    fn test_sanitize_rel_path_absolute_drive_letter() {
        // Absolute paths with a Windows drive letter must drop the drive part
        assert_eq!(
            sanitize_rel_path("C:/secret.png"),
            PathBuf::from("secret.png")
        );
    }

    #[test]
    fn test_sanitize_rel_path_absolute_drive_backslash() {
        assert_eq!(
            sanitize_rel_path("D:\\Windows\\System32\\secret.dll"),
            PathBuf::from("Windows/System32/secret.dll")
        );
    }

    #[test]
    fn test_vrm_to_pmx_end_to_end() {
        let Some(input) = crate::test_util::try_test_file(crate::test_util::seed_san_vrm()) else {
            return;
        };
        let output = std::env::temp_dir().join("popone_test_e2e.pmx");

        // VRM -> PMX conversion
        let stats =
            crate::convert_vrm_to_pmx(&input, &output, &crate::VrmConvertOptions::default())
                .expect("VRM→PMX変換失敗");

        // Sanity-check the stats
        assert!(stats.bones > 100, "ボーン数が少なすぎる: {}", stats.bones);
        assert!(
            stats.vertices > 1000,
            "頂点数が少なすぎる: {}",
            stats.vertices
        );
        assert!(stats.materials > 0, "材質数がゼロ");

        // Check that the output file exists and has a reasonable size
        let metadata = std::fs::metadata(&output).expect("出力ファイルなし");
        assert!(
            metadata.len() > 1000,
            "PMXファイルが小さすぎる: {} bytes",
            metadata.len()
        );

        // Read back and verify
        let pmx = crate::pmx::reader::read_pmx(&output).expect("PMX再読み込み失敗");
        assert!(
            pmx.bones.len() > 100,
            "ボーン数が少なすぎる: {}",
            pmx.bones.len()
        );
        assert!(
            pmx.vertices.len() > 1000,
            "頂点数が少なすぎる: {}",
            pmx.vertices.len()
        );

        // Clean up the temporary files
        let _ = std::fs::remove_file(&output);
        let tex_dir = output.parent().unwrap().join("textures");
        let _ = std::fs::remove_dir_all(&tex_dir);
    }
}
