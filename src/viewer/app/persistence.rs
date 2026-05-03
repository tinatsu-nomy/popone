//! Persistence of session settings and texture history.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Application data directory
// ---------------------------------------------------------------------------

/// Return the application data directory.
/// Windows: `%LOCALAPPDATA%\popone` (writable per-user area).
/// Other platforms: next to the executable.
pub fn data_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let dir = PathBuf::from(local).join("popone");
            if std::fs::create_dir_all(&dir).is_ok() {
                return dir;
            }
        }
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Migrate config files next to the exe over to data_dir (one-time).
/// Skip when source and destination resolve to the same directory.
pub fn migrate_from_exe_dir(data_dir: &Path) {
    let exe_dir = match std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    {
        Some(d) => d,
        None => return,
    };
    // No migration needed when both directories are identical.
    if exe_dir == data_dir {
        return;
    }
    for name in &["popone.toml", "popone_history.json"] {
        let old = exe_dir.join(name);
        let new_path = data_dir.join(name);
        if old.exists() && !new_path.exists() {
            if std::fs::rename(&old, &new_path).is_ok() {
                log::info!("Migrated {} -> {}", old.display(), new_path.display());
            } else if std::fs::copy(&old, &new_path).is_ok() {
                let _ = std::fs::remove_file(&old);
                log::info!(
                    "Migrated {} -> {} (copy)",
                    old.display(),
                    new_path.display()
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// AppConfig (popone.toml)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    pub window: Option<WindowConfig>,
    #[serde(default)]
    pub directory: DirectoryConfig,
    #[serde(default)]
    pub log: LogConfig,
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default)]
    pub log_viewer: LogViewerConfig,
    #[serde(default)]
    pub display: DisplayConfig,
    /// ray-mmd root folder (§K.2 / Step 6). Used when resolving `#include` relative
    /// paths during MME export. When unset (None) the current directory `.\` is used as fallback.
    #[serde(default)]
    pub ray_mmd_root: Option<String>,
}

/// Display options that survive across sessions.
///
/// `DisplaySettings` as a whole is not serde-aware and most fields are session-scoped,
/// so the persisted ones are split out into this struct (same approach as `LogViewerConfig`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayConfig {
    /// Whether the texture-decode-failure fallback color is white.
    /// true = white (avoid color tinting; default). false = magenta (highlights missing textures).
    #[serde(default = "DisplayConfig::default_true")]
    pub white_texture_fallback: bool,
    /// Whether the right tool panel is resizable. When true the user can drag the edge.
    /// false (default) locks the panel at 280 px and disables content-driven auto-resize.
    #[serde(default)]
    pub panel_resizable: bool,
    /// Width of the right tool panel (px). Persists the user's drag-resized width.
    /// Range is [280, 600]. Unused while `panel_resizable = false`.
    #[serde(default = "DisplayConfig::default_panel_width")]
    pub panel_width: f32,
}

impl DisplayConfig {
    fn default_true() -> bool {
        true
    }

    fn default_panel_width() -> f32 {
        280.0
    }
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            white_texture_fallback: true,
            panel_resizable: false,
            panel_width: 280.0,
        }
    }
}

/// Log viewer window settings (visibility / position / size / level filters).
///
/// Every field is `#[serde(default)]` for backward compatibility. An existing
/// `popone.toml` without a `[log_viewer]` section starts up with default values.
///
/// Position / size fields are stored as the same `x/y/width/height` scalar form
/// used by the `[window]` section. The toml serializer renders `[f32; 2]` as
/// vertical arrays, which broke formatting consistency — this format keeps it
/// aligned with the rest of the file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogViewerConfig {
    /// Whether the log viewer is shown automatically at startup. Defaults to false.
    #[serde(default)]
    pub visible: bool,
    /// Window top-left X coordinate. None = let the OS pick a default position.
    #[serde(default)]
    pub x: Option<f32>,
    /// Window top-left Y coordinate. None = let the OS pick a default position.
    #[serde(default)]
    pub y: Option<f32>,
    /// Window inner width. None = default 720.
    #[serde(default)]
    pub width: Option<f32>,
    /// Window inner height. None = default 480.
    #[serde(default)]
    pub height: Option<f32>,
    #[serde(default = "LogViewerConfig::default_true")]
    pub show_error: bool,
    #[serde(default = "LogViewerConfig::default_true")]
    pub show_warn: bool,
    #[serde(default = "LogViewerConfig::default_true")]
    pub show_info: bool,
    #[serde(default)]
    pub show_debug: bool,
    #[serde(default = "LogViewerConfig::default_true")]
    pub follow_tail: bool,
}

impl LogViewerConfig {
    fn default_true() -> bool {
        true
    }
}

impl Default for LogViewerConfig {
    fn default() -> Self {
        Self {
            visible: false,
            x: None,
            y: None,
            width: None,
            height: None,
            show_error: true,
            show_warn: true,
            show_info: true,
            show_debug: false,
            follow_tail: true,
        }
    }
}

/// GUI theme color settings. Values are 6-digit hex (e.g. "4A90D9", "#4A90D9").
/// Unspecified entries fall back to the default dark-theme colors.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThemeConfig {
    /// Panel / window background color (default: "1D1D1D")
    pub panel_bg: Option<String>,
    /// Border color (default: "333333")
    pub border: Option<String>,
    /// Accent color — hover / selected (default: "4A90D9")
    pub accent: Option<String>,
    /// Text color (default: "D0D0D0")
    pub text: Option<String>,
    /// Widget background color (default: "252525")
    pub widget_bg: Option<String>,
    /// Active (mouse-down) color (default: "2A5A8A")
    pub active: Option<String>,
}

impl ThemeConfig {
    /// Convert a hex color string ("RRGGBB" or "#RRGGBB") into (r, g, b).
    pub fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
        let s = s.trim().trim_start_matches('#');
        if s.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some((r, g, b))
    }
}

/// Log settings (currently only the output level).
///
/// Older versions had a `keep` field (number of old log files to retain). v0.4.0
/// consolidated log output into "user-triggered manual export + crash dump on panic"
/// and dropped automatic rotation, so the `keep` field was removed too.
/// `keep = N` left in an existing popone.toml is silently ignored by serde, so
/// backward compatibility is preserved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// Log level (error, warn, info, debug)
    #[serde(default = "LogConfig::default_level")]
    pub level: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: Self::default_level(),
        }
    }
}

impl LogConfig {
    fn default_level() -> String {
        "debug".to_string()
    }

    /// Convert the log level string into `log::LevelFilter`.
    pub fn level_filter(&self) -> log::LevelFilter {
        self.level
            .parse::<log::LevelFilter>()
            .unwrap_or(log::LevelFilter::Debug)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowConfig {
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 1280.0,
            height: 720.0,
        }
    }
}

impl WindowConfig {
    /// Treat as "changed" only when the difference is at least 1 px.
    pub fn is_significantly_different(&self, x: f32, y: f32, width: f32, height: f32) -> bool {
        (self.x - x).abs() >= 1.0
            || (self.y - y).abs() >= 1.0
            || (self.width - width).abs() >= 1.0
            || (self.height - height).abs() >= 1.0
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DirectoryConfig {
    pub last_model: Option<String>,
    pub last_texture: Option<String>,
}

// ---------------------------------------------------------------------------
// AppConfig I/O
// ---------------------------------------------------------------------------

pub fn config_path(dir: &Path) -> PathBuf {
    dir.join("popone.toml")
}

/// Load the config file. Returns None if the file is missing or fails to parse.
/// If the main file is missing but a `.bak` exists, recover from the backup.
pub fn load_config(dir: &Path) -> Option<AppConfig> {
    let path = config_path(dir);
    recover_from_bak(&path);
    let text = std::fs::read_to_string(&path).ok()?;
    match toml::from_str::<AppConfig>(&text) {
        Ok(cfg) => {
            log::info!("Settings loaded: {}", path.display());
            Some(cfg)
        }
        Err(e) => {
            log::warn!("Settings file parse failed: {e}");
            None
        }
    }
}

pub fn save_config(dir: &Path, config: &AppConfig) {
    let path = config_path(dir);
    match toml::to_string_pretty(config) {
        Ok(text) => {
            if let Err(e) = atomic_write(&path, text.as_bytes()) {
                log::warn!("Settings file save failed: {e}");
            } else {
                log::info!("Settings saved: {}", path.display());
            }
        }
        Err(e) => log::warn!("Settings serialization failed: {e}"),
    }
}

// ---------------------------------------------------------------------------
// TextureHistory (popone_history.json)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextureHistoryFile {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub history: HashMap<String, Vec<TextureHistoryEntry>>,
    /// Added in v0.5.0 (§I minimal persistence): material parameter edit deltas.
    /// `#[serde(default)]` lets v1 files (lacking this field) load without issues.
    /// Keys are normalized model paths (same key as `history`).
    #[serde(default)]
    pub param_overrides: HashMap<String, Vec<MaterialParamOverrideEntry>>,
    /// Added in v0.5.5 (Phase 1 vertex UV editing): per-vertex UV edit deltas.
    /// `#[serde(default)]` lets pre-v0.5.5 files (lacking this field) load.
    /// Keys are normalized model paths (same key as `history`).
    #[serde(default)]
    pub vertex_uv_overrides: HashMap<String, Vec<VertexUvOverrideEntry>>,
}

/// Persistent entry for material parameter edit deltas (v0.5.0 / §I minimal persistence).
///
/// Stored alongside `TextureHistoryEntry` texture saves: edit deltas for material colors
/// and scalar values are serialized as `MaterialParamOverride`. Material identity uses
/// `material_index + material_name`, sharing `resolve_material()` with texture lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialParamOverrideEntry {
    pub material_index: usize,
    pub material_name: String,
    #[serde(flatten)]
    pub overrides: super::material_edit::MaterialParamOverride,
}

/// Persistent entry for per-vertex UV edits (v0.5.5 / Phase 1 / Phase 3 A-1).
///
/// One entry per (mesh, vertex). `uv_set` = 0 = UV0, 1 = UV1.
/// `#[serde(default)]` provides JSON backward compatibility: entries written by
/// v0.5.5 Phase 1 (UV0 only, no uv_set field) load with `uv_set = 0`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VertexUvOverrideEntry {
    pub mesh_index: u32,
    pub vertex_index: u32,
    /// UV set (0 = UV0, 1 = UV1). Added in Phase 3 A-1; existing files default to `0`.
    #[serde(default)]
    pub uv_set: u8,
    pub uv: [f32; 2],
}

fn default_version() -> u32 {
    1
}

impl Default for TextureHistoryFile {
    fn default() -> Self {
        Self {
            version: 1,
            history: HashMap::new(),
            param_overrides: HashMap::new(),
            vertex_uv_overrides: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextureHistoryEntry {
    pub material_index: usize,
    pub material_name: String,
    pub texture_path: String,
    /// Added in v0.5.1: texture slot kind.
    /// `#[serde(default)]` lets pre-v0.5.1 JSON (lacking this field) be read as
    /// `TextureSlot::BaseColor` (backward compatible).
    #[serde(default = "default_base_color_slot")]
    pub slot: crate::intermediate::types::TextureSlot,
}

fn default_base_color_slot() -> crate::intermediate::types::TextureSlot {
    crate::intermediate::types::TextureSlot::BaseColor
}

// ---------------------------------------------------------------------------
// TextureHistory I/O
// ---------------------------------------------------------------------------

pub fn history_path(dir: &Path) -> PathBuf {
    dir.join("popone_history.json")
}

pub fn load_texture_history(dir: &Path) -> TextureHistoryFile {
    let path = history_path(dir);
    recover_from_bak(&path);
    match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<TextureHistoryFile>(&text) {
            Ok(h) => {
                log::info!(
                    "Texture history loaded: {} ({} entries)",
                    path.display(),
                    h.history.len()
                );
                h
            }
            Err(e) => {
                log::warn!("Texture history parse failed (continuing empty): {e}");
                TextureHistoryFile::default()
            }
        },
        Err(_) => TextureHistoryFile::default(),
    }
}

pub fn save_texture_history(dir: &Path, history: &TextureHistoryFile) {
    let path = history_path(dir);
    match serde_json::to_string_pretty(history) {
        Ok(json) => {
            if let Err(e) = atomic_write(&path, json.as_bytes()) {
                log::warn!("Texture history save failed: {e}");
            } else {
                log::info!("Texture history saved: {}", path.display());
            }
        }
        Err(e) => log::warn!("Texture history serialization failed: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Material matching
// ---------------------------------------------------------------------------

/// Match a history entry's material_index + material_name to a material in the current model.
/// 1. index + name both match -> use as-is
/// 2. name matches uniquely -> fallback
/// 3. neither -> None
pub fn resolve_material(
    materials: &[crate::intermediate::types::IrMaterial],
    entry: &TextureHistoryEntry,
) -> Option<usize> {
    // 1. exact index + name match
    if let Some(mat) = materials.get(entry.material_index) {
        if mat.name == entry.material_name {
            return Some(entry.material_index);
        }
    }
    // 2. name-uniqueness fallback
    let mut matched = materials
        .iter()
        .enumerate()
        .filter(|(_, m)| m.name == entry.material_name)
        .map(|(i, _)| i);
    let first = matched.next()?;
    if matched.next().is_none() {
        Some(first)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// History key / path normalization
// ---------------------------------------------------------------------------

/// Normalize a Windows path and lowercase it (used as a history key).
pub fn normalize_path(path: &Path) -> String {
    // Fix for the issue where the same model under a relative path and an absolute path
    // produced different history keys: resolve to an absolute path with `dunce::canonicalize`,
    // then normalize. If the file does not exist (tests etc.), fall back to the original path.
    let abs = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    abs.to_string_lossy().to_lowercase().replace('/', "\\")
}

// ---------------------------------------------------------------------------
// atomic write (Windows-aware)
// ---------------------------------------------------------------------------

/// If the main file is missing but a `.bak` exists, rename the bak back into place to recover.
fn recover_from_bak(path: &Path) {
    if !path.exists() {
        let bak = path.with_extension("bak");
        if bak.exists() {
            if let Err(e) = std::fs::rename(&bak, path) {
                log::warn!(
                    "Backup restore failed: {} -> {}: {e}",
                    bak.display(),
                    path.display()
                );
            } else {
                log::info!("Restored from backup: {}", path.display());
            }
        }
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    // Windows: rename fails when overwriting an existing file, so use a backup-based
    // swap to replace safely (the original file is preserved even if rename fails).
    if path.exists() {
        let bak = path.with_extension("bak");
        let _ = std::fs::remove_file(&bak); // ignore errors removing an old bak
        if let Err(e) = std::fs::rename(path, &bak) {
            // Backup creation failed -> overwrite tmp directly (the original file is kept).
            log::warn!("Backup creation failed (direct overwrite): {e}");
            return std::fs::copy(&tmp, path).map(|_| ()).map(|_| {
                let _ = std::fs::remove_file(&tmp);
            });
        }
        if let Err(e) = std::fs::rename(&tmp, path) {
            // tmp -> path rename failed -> restore from bak.
            log::warn!("File replacement failed (restored from backup): {e}");
            let _ = std::fs::rename(&bak, path);
            return Err(e);
        }
        let _ = std::fs::remove_file(&bak); // remove the bak once the swap succeeded
    } else {
        std::fs::rename(&tmp, path)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_config_significantly_different() {
        let cfg = WindowConfig {
            x: 100.0,
            y: 50.0,
            width: 1280.0,
            height: 720.0,
        };
        // Sub-pixel difference (< 1 px) -> not changed.
        assert!(!cfg.is_significantly_different(100.5, 50.3, 1280.2, 720.1));
        // >= 1 px difference -> changed.
        assert!(cfg.is_significantly_different(102.0, 50.0, 1280.0, 720.0));
    }

    #[test]
    fn test_resolve_material_exact_match() {
        let materials = vec![
            crate::intermediate::types::IrMaterial {
                name: "body".into(),
                ..Default::default()
            },
            crate::intermediate::types::IrMaterial {
                name: "face".into(),
                ..Default::default()
            },
        ];
        let entry = TextureHistoryEntry {
            material_index: 1,
            material_name: "face".into(),
            texture_path: "tex.png".into(),
            slot: crate::intermediate::types::TextureSlot::BaseColor,
        };
        assert_eq!(resolve_material(&materials, &entry), Some(1));
    }

    #[test]
    fn test_resolve_material_name_fallback() {
        let materials = vec![
            crate::intermediate::types::IrMaterial {
                name: "face".into(),
                ..Default::default()
            },
            crate::intermediate::types::IrMaterial {
                name: "body".into(),
                ..Default::default()
            },
        ];
        // index=5 does not exist, but name="body" matches uniquely.
        let entry = TextureHistoryEntry {
            material_index: 5,
            material_name: "body".into(),
            texture_path: "tex.png".into(),
            slot: crate::intermediate::types::TextureSlot::BaseColor,
        };
        assert_eq!(resolve_material(&materials, &entry), Some(1));
    }

    #[test]
    fn test_resolve_material_ambiguous() {
        let materials = vec![
            crate::intermediate::types::IrMaterial {
                name: "mat".into(),
                ..Default::default()
            },
            crate::intermediate::types::IrMaterial {
                name: "mat".into(),
                ..Default::default()
            },
        ];
        let entry = TextureHistoryEntry {
            material_index: 5,
            material_name: "mat".into(),
            texture_path: "tex.png".into(),
            slot: crate::intermediate::types::TextureSlot::BaseColor,
        };
        // Multiple materials share the name -> None.
        assert_eq!(resolve_material(&materials, &entry), None);
    }

    #[test]
    fn test_normalize_path() {
        let p = Path::new("C:/Users/Test/Models/char.fbx");
        let normalized = normalize_path(p);
        assert!(normalized.contains("\\"));
        assert_eq!(normalized, normalized.to_lowercase());
    }

    #[test]
    fn test_config_toml_roundtrip() {
        let cfg = AppConfig {
            window: Some(WindowConfig {
                x: 100.0,
                y: 50.0,
                width: 1280.0,
                height: 720.0,
            }),
            directory: DirectoryConfig {
                last_model: Some("C:\\Test".into()),
                last_texture: None,
            },
            ..Default::default()
        };
        let text = toml::to_string_pretty(&cfg).unwrap();
        let parsed: AppConfig = toml::from_str(&text).unwrap();
        let win = parsed.window.expect("window should be Some");
        assert!((win.x - 100.0).abs() < 0.01);
        assert_eq!(parsed.directory.last_model, Some("C:\\Test".into()));

        // Partial config without a window section -> window is None.
        let partial = "[directory]\nlast_model = 'C:\\\\Test'\n";
        let parsed2: AppConfig = toml::from_str(partial).unwrap();
        assert!(parsed2.window.is_none());
    }

    #[test]
    fn test_log_viewer_geometry_uses_scalar_fields() {
        // Verify the log_viewer section is emitted with the same x/y/width/height scalar
        // form as [window], not the old vertical `position = [...]` / `size = [...]` arrays.
        // This is the v0.5.9 fix that dropped the array form.
        let cfg = AppConfig {
            log_viewer: LogViewerConfig {
                visible: true,
                x: Some(978.0),
                y: Some(664.0),
                width: Some(720.0),
                height: Some(480.0),
                ..LogViewerConfig::default()
            },
            ..Default::default()
        };
        let text = toml::to_string_pretty(&cfg).unwrap();
        // Scalar form (same style as [window]).
        assert!(
            text.contains("x = 978"),
            "x should be inline scalar:\n{text}"
        );
        assert!(
            text.contains("y = 664"),
            "y should be inline scalar:\n{text}"
        );
        assert!(
            text.contains("width = 720"),
            "width should be inline scalar:\n{text}"
        );
        assert!(
            text.contains("height = 480"),
            "height should be inline scalar:\n{text}"
        );
        // The legacy vertical-array keys must be gone.
        assert!(
            !text.contains("position ="),
            "position field should be removed:\n{text}"
        );
        assert!(
            !text.contains("size ="),
            "size field should be removed:\n{text}"
        );
    }

    #[test]
    fn test_log_viewer_legacy_position_size_ignored() {
        // Even when the popone.toml uses the legacy form (position = [...], size = [...]),
        // parsing must not error and x/y/width/height must fall back to the default None.
        let legacy = r#"
            [log_viewer]
            visible = true
            position = [978.0, 664.0]
            size = [720.0, 480.0]
            show_error = true
            show_warn = true
            show_info = true
            show_debug = false
            follow_tail = true
        "#;
        let parsed: AppConfig = toml::from_str(legacy).expect("legacy should parse");
        assert!(parsed.log_viewer.visible);
        assert!(parsed.log_viewer.x.is_none());
        assert!(parsed.log_viewer.y.is_none());
        assert!(parsed.log_viewer.width.is_none());
        assert!(parsed.log_viewer.height.is_none());
    }

    #[test]
    fn test_texture_history_slot_default_backward_compat() {
        // Backward-compatibility test: pre-v0.5.0 JSON (no slot field) loaded by v0.5.1.
        let legacy_json = r#"{
            "material_index": 0,
            "material_name": "body",
            "texture_path": "C:\\Tex\\body.png"
        }"#;
        let entry: TextureHistoryEntry = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(entry.material_index, 0);
        assert_eq!(entry.material_name, "body");
        assert_eq!(
            entry.slot,
            crate::intermediate::types::TextureSlot::BaseColor,
            "slot がない旧エントリは BaseColor として読み込まれる"
        );
    }

    #[test]
    fn test_texture_history_slot_explicit() {
        // v0.5.1 JSON with the slot field set.
        let json = r#"{
            "material_index": 1,
            "material_name": "face",
            "texture_path": "C:\\Tex\\face_normal.png",
            "slot": "normal"
        }"#;
        let entry: TextureHistoryEntry = serde_json::from_str(json).unwrap();
        assert_eq!(
            entry.slot,
            crate::intermediate::types::TextureSlot::Normal,
            "explicit slot specification should be reflected"
        );
    }

    #[test]
    fn test_texture_history_slot_roundtrip() {
        // Serde round-trip: Emissive slot.
        let original = TextureHistoryEntry {
            material_index: 2,
            material_name: "face".into(),
            texture_path: "emissive.png".into(),
            slot: crate::intermediate::types::TextureSlot::Emissive,
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: TextureHistoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed.slot,
            crate::intermediate::types::TextureSlot::Emissive
        );
    }

    #[test]
    fn test_history_json_roundtrip() {
        let mut h = TextureHistoryFile::default();
        h.history.insert(
            "c:\\test.fbx".into(),
            vec![TextureHistoryEntry {
                material_index: 0,
                material_name: "body".into(),
                texture_path: "C:\\Tex\\body.png".into(),
                slot: crate::intermediate::types::TextureSlot::BaseColor,
            }],
        );
        let json = serde_json::to_string_pretty(&h).unwrap();
        let parsed: TextureHistoryFile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.history.len(), 1);
    }
}
