//! セッション設定・テクスチャ履歴の永続化

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Application data directory
// ---------------------------------------------------------------------------

/// アプリケーションデータディレクトリを返す。
/// Windows: `%LOCALAPPDATA%\popone`（書き込み可能なユーザー領域）
/// それ以外: 実行ファイルの隣
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

/// exe 隣接の設定ファイルを data_dir に移行する（初回のみ）。
/// 移行元と移行先が同じディレクトリの場合は何もしない。
pub fn migrate_from_exe_dir(data_dir: &Path) {
    let exe_dir = match std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    {
        Some(d) => d,
        None => return,
    };
    // 同一ディレクトリなら移行不要
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// ログレベル (error, warn, info, debug)
    #[serde(default = "LogConfig::default_level")]
    pub level: String,
    /// ログファイル保持数
    #[serde(default = "LogConfig::default_keep")]
    pub keep: usize,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: Self::default_level(),
            keep: Self::default_keep(),
        }
    }
}

impl LogConfig {
    fn default_level() -> String {
        "debug".to_string()
    }

    fn default_keep() -> usize {
        5
    }

    /// ログレベル文字列を `log::LevelFilter` に変換
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
    /// 1px 以上の差がある場合のみ「変更あり」とみなす
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

/// 設定ファイルを読み込む。ファイルが存在しない・解析失敗の場合は None。
/// 本体が存在せず `.bak` がある場合はバックアップから復旧する。
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
}

fn default_version() -> u32 {
    1
}

impl Default for TextureHistoryFile {
    fn default() -> Self {
        Self {
            version: 1,
            history: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextureHistoryEntry {
    pub material_index: usize,
    pub material_name: String,
    pub texture_path: String,
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
// 材質照合
// ---------------------------------------------------------------------------

/// 履歴エントリの material_index + material_name で現在のモデルの材質を照合する。
/// 1. index + name が一致 → そのまま
/// 2. name が一意に一致 → フォールバック
/// 3. いずれも不一致 → None
pub fn resolve_material(
    materials: &[crate::intermediate::types::IrMaterial],
    entry: &TextureHistoryEntry,
) -> Option<usize> {
    // 1. index + name 完全一致
    if let Some(mat) = materials.get(entry.material_index) {
        if mat.name == entry.material_name {
            return Some(entry.material_index);
        }
    }
    // 2. name 一意一致フォールバック
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
// 履歴キー・パス正規化
// ---------------------------------------------------------------------------

/// Windows パスを正規化して小文字にする（履歴キー用）
pub fn normalize_path(path: &Path) -> String {
    dunce::simplified(path)
        .to_string_lossy()
        .to_lowercase()
        .replace('/', "\\")
}

// ---------------------------------------------------------------------------
// atomic write（Windows 対応）
// ---------------------------------------------------------------------------

/// 本体ファイルが存在せず `.bak` がある場合、bak を本体にリネームして復旧する。
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
    // Windows: rename は既存ファイル上書きに失敗するため、
    // バックアップ方式で安全に置換する（rename 失敗時も元ファイルを残す）
    if path.exists() {
        let bak = path.with_extension("bak");
        let _ = std::fs::remove_file(&bak); // 古い bak は無視
        if let Err(e) = std::fs::rename(path, &bak) {
            // バックアップ作成失敗 → tmp を直接上書き（元ファイルは残る）
            log::warn!("Backup creation failed (direct overwrite): {e}");
            return std::fs::copy(&tmp, path).map(|_| ()).map(|_| {
                let _ = std::fs::remove_file(&tmp);
            });
        }
        if let Err(e) = std::fs::rename(&tmp, path) {
            // tmp→path の rename 失敗 → bak を復元
            log::warn!("File replacement failed (restored from backup): {e}");
            let _ = std::fs::rename(&bak, path);
            return Err(e);
        }
        let _ = std::fs::remove_file(&bak); // 成功したら bak を削除
    } else {
        std::fs::rename(&tmp, path)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// テスト
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
        // 微小な差 (< 1px) → 変更なし
        assert!(!cfg.is_significantly_different(100.5, 50.3, 1280.2, 720.1));
        // 1px 以上の差 → 変更あり
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
        // index=5 は存在しないが name="body" は一意に一致
        let entry = TextureHistoryEntry {
            material_index: 5,
            material_name: "body".into(),
            texture_path: "tex.png".into(),
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
        };
        // 同名が複数 → None
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

        // window セクションなしの部分設定 → window は None
        let partial = "[directory]\nlast_model = 'C:\\\\Test'\n";
        let parsed2: AppConfig = toml::from_str(partial).unwrap();
        assert!(parsed2.window.is_none());
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
            }],
        );
        let json = serde_json::to_string_pretty(&h).unwrap();
        let parsed: TextureHistoryFile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.history.len(), 1);
    }
}
