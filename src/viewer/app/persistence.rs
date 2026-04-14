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
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default)]
    pub log_viewer: LogViewerConfig,
    /// ray-mmd ルートフォルダ（§K.2 / Step 6）。MME 出力時の `#include` 相対パス解決に使用。
    /// 未設定（None）時はカレントディレクトリ `.\` をフォールバックとして使用する。
    #[serde(default)]
    pub ray_mmd_root: Option<String>,
}

/// ログビュアーウインドウの設定（表示状態・位置・サイズ・レベルフィルタ）。
///
/// 全フィールド `#[serde(default)]` で後方互換性を担保している。既存の `popone.toml`
/// に `[log_viewer]` セクションが無くてもデフォルト値で起動する。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogViewerConfig {
    /// 起動時にログビュアーを自動表示するか。初期 false。
    #[serde(default)]
    pub visible: bool,
    /// ウインドウ左上座標 (x, y)。None = OS 任せのデフォルト位置。
    #[serde(default)]
    pub position: Option<[f32; 2]>,
    /// ウインドウ内寸 (width, height)。None = デフォルト 720x480。
    #[serde(default)]
    pub size: Option<[f32; 2]>,
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
            position: None,
            size: None,
            show_error: true,
            show_warn: true,
            show_info: true,
            show_debug: false,
            follow_tail: true,
        }
    }
}

/// GUI テーマカラー設定。値は 6 桁の 16 進数 (例: "4A90D9", "#4A90D9")。
/// 未指定の項目はデフォルトのダークテーマ色にフォールバックする。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThemeConfig {
    /// パネル・ウィンドウ背景色 (デフォルト: "1D1D1D")
    pub panel_bg: Option<String>,
    /// ボーダー色 (デフォルト: "333333")
    pub border: Option<String>,
    /// アクセントカラー — ホバー・選択 (デフォルト: "4A90D9")
    pub accent: Option<String>,
    /// テキスト色 (デフォルト: "D0D0D0")
    pub text: Option<String>,
    /// ウィジェット背景色 (デフォルト: "252525")
    pub widget_bg: Option<String>,
    /// アクティブ（クリック中）色 (デフォルト: "2A5A8A")
    pub active: Option<String>,
}

impl ThemeConfig {
    /// 16 進数カラー文字列 ("RRGGBB" or "#RRGGBB") を (r, g, b) に変換
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

/// ログ設定（現在は出力レベルのみ）。
///
/// 旧バージョンには `keep`（古いログファイル保持数）フィールドがあったが、v0.4.0 で
/// ログ出力が「ユーザ明示の手動エクスポート + パニック時のクラッシュダンプ」に集約され
/// たため、自動ローテーションを廃止して `keep` フィールドも削除した。
/// 既存 popone.toml に `keep = N` が残っていても serde が未知フィールドを無視するため
/// 後方互換性は維持される。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// ログレベル (error, warn, info, debug)
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
    /// v0.5.0 追加 (§I 最小永続化): 材質パラメータ編集差分。
    /// `#[serde(default)]` により v1 の既存ファイル（このフィールドがない）も問題なく読み込める。
    /// key は正規化モデルパス（`history` と同じキー）。
    #[serde(default)]
    pub param_overrides: HashMap<String, Vec<MaterialParamOverrideEntry>>,
    /// v0.5.5 追加 (Phase 1 頂点 UV 編集): 頂点単位 UV 編集差分。
    /// `#[serde(default)]` により v0.5.4 以前のファイル（このフィールドがない）も読み込める。
    /// key は正規化モデルパス（`history` と同じキー）。
    #[serde(default)]
    pub vertex_uv_overrides: HashMap<String, Vec<VertexUvOverrideEntry>>,
}

/// 材質パラメータ編集差分の永続化エントリ (v0.5.0 / §I 最小永続化)。
///
/// `TextureHistoryEntry` のテクスチャ保存と並行して、材質の色・スカラー値の編集差分を
/// `MaterialParamOverride` として保存する。材質同定は `material_index + material_name` で
/// `resolve_material()` を共用する。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialParamOverrideEntry {
    pub material_index: usize,
    pub material_name: String,
    #[serde(flatten)]
    pub overrides: super::material_edit::MaterialParamOverride,
}

/// 頂点単位 UV 編集の永続化エントリ (v0.5.5 / Phase 1 / Phase 3 A-1)。
///
/// モデル全体に対して 1 エントリ。`uv_set` は 0 = UV0, 1 = UV1。
/// `#[serde(default)]` で JSON 下位互換性を確保する: v0.5.5 Phase 1 で書き出された
/// UV0 のみのエントリ (uv_set フィールドなし) は `uv_set = 0` として読み込まれる。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VertexUvOverrideEntry {
    pub mesh_index: u32,
    pub vertex_index: u32,
    /// UV セット (0 = UV0, 1 = UV1)。Phase 3 A-1 で追加。既存ファイルは `0` になる。
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
    /// v0.5.1 追加: テクスチャスロット種別。
    /// `#[serde(default)]` により v0.5.0 以前の JSON（このフィールドがない）は
    /// `TextureSlot::BaseColor` として解釈される（後方互換）。
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
    // 相対パスと絶対パスで同じモデルが別キーになる問題の修正:
    // `dunce::canonicalize` で絶対パスに解決してから正規化する。
    // ファイルが存在しない場合（テスト等）は元のパスにフォールバック。
    let abs = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    abs.to_string_lossy().to_lowercase().replace('/', "\\")
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
        // index=5 は存在しないが name="body" は一意に一致
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
    fn test_texture_history_slot_default_backward_compat() {
        // v0.5.0 以前のフィールドなし JSON を v0.5.1 で読み込む後方互換テスト
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
        // v0.5.1 の slot フィールド付き JSON
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
            "明示的な slot 指定が反映される"
        );
    }

    #[test]
    fn test_texture_history_slot_roundtrip() {
        // serde ラウンドトリップ: Emissive スロット
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
