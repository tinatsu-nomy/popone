use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;
use eframe::egui_wgpu;
use eframe::wgpu;

use crate::intermediate::types::IrModel;
use crate::vrm;

use super::animation::AnimationState;
use super::camera::OrbitCamera;
use super::gpu::{self, DrawMode, GpuRenderer, LightMode, RenderParams};
use super::mesh::GpuModel;
use super::ui;

/// モデルの読み込み元を表す
#[derive(Clone)]
pub enum ReloadableSource {
    /// 通常のファイル（リロード時は再読み込み）
    File(PathBuf),
    /// 一時ファイルからのスナップショット（リロード時はメモリから）
    Snapshot {
        original_path: PathBuf,
        main_bytes: Arc<[u8]>,
        /// PMX/PMD 用: 相対パス → バイト列（テクスチャ・.txt等）
        aux_files: HashMap<PathBuf, Arc<[u8]>>,
    },
    /// アーカイブ（ZIP/7z）内のモデル
    Archive {
        original_path: PathBuf,
        /// D&D一時ファイル用スナップショット
        archive_bytes: Option<Arc<[u8]>>,
        /// 選択されたモデルの内部パス
        selected_entry_path: String,
        /// モデル種別
        inner_kind: crate::archive::ArchiveModelKind,
    },
}

impl ReloadableSource {
    /// 表示用パスを返す
    pub fn display_path(&self) -> &Path {
        match self {
            ReloadableSource::File(p) => p,
            ReloadableSource::Snapshot { original_path, .. } => original_path,
            ReloadableSource::Archive { original_path, .. } => original_path,
        }
    }

    /// 拡張子を小文字で返す
    pub fn extension_lower(&self) -> String {
        self.display_path()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase()
    }

    /// キャッシュ済みかどうか
    pub fn is_snapshot(&self) -> bool {
        matches!(self, ReloadableSource::Snapshot { .. })
    }
}

/// D&D temp ファイルの先読みデータ（ファイル消失前にバイト列をキャッシュ）
pub struct PreloadedData {
    path: PathBuf,
    main_bytes: Arc<[u8]>,
    aux_files: HashMap<PathBuf, Arc<[u8]>>,
}

/// テクスチャの読み込み元（ファイルまたはキャッシュ済みバイト列）
#[derive(Clone)]
pub enum TextureSource {
    File(PathBuf),
    Cached {
        original_name: String,
        data: Arc<[u8]>,
        is_psd: bool,
    },
}

impl TextureSource {
    /// 表示用名前を返す
    pub fn display_name(&self) -> String {
        match self {
            TextureSource::File(p) => p.to_string_lossy().into_owned(),
            TextureSource::Cached { original_name, .. } => original_name.clone(),
        }
    }
}

/// 一時ディレクトリ配下かどうかを検出する
fn is_temp_path(path: &Path) -> bool {
    // canonicalize ベース（ファイル存在時）
    if let (Ok(temp), Ok(target)) = (std::env::temp_dir().canonicalize(), path.canonicalize()) {
        if target.starts_with(&temp) {
            return true;
        }
    }
    // フォールバック: 文字列ベース（ファイル消失後でも機能）
    let path_str = path.to_string_lossy().to_lowercase();
    let mut temp_str = std::env::temp_dir().to_string_lossy().to_lowercase();
    // パス境界を保証: TempBackup 等の誤検出を防止
    if !temp_str.ends_with(std::path::MAIN_SEPARATOR) {
        temp_str.push(std::path::MAIN_SEPARATOR);
    }
    path_str.starts_with(&*temp_str)
}

/// FBX 外部テクスチャ用: 指定ディレクトリ以下の画像ファイルを再帰的に収集する
/// キーは base_dir からの相対パス（サブディレクトリ構造を保持）
fn collect_image_files_recursive(
    base_dir: &Path,
    current_dir: &Path,
    out: &mut HashMap<PathBuf, Arc<[u8]>>,
) {
    let Ok(entries) = std::fs::read_dir(current_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let ep = entry.path();
        if ep.is_dir() {
            collect_image_files_recursive(base_dir, &ep, out);
        } else if let Some(ext) = ep.extension().and_then(|e| e.to_str()) {
            let ext_low = ext.to_lowercase();
            if matches!(
                ext_low.as_str(),
                "png" | "jpg" | "jpeg" | "tga" | "bmp" | "tif" | "tiff" | "dds"
            ) {
                if let Ok(img_data) = std::fs::read(&ep) {
                    if let Ok(rel) = ep.strip_prefix(base_dir) {
                        out.insert(rel.to_path_buf(), Arc::from(img_data.into_boxed_slice()));
                    }
                }
            }
        }
    }
}

/// D&D 対応画像拡張子
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "tga", "bmp", "psd"];

/// D&D 対応モデル/アニメーション拡張子
const MODEL_EXTENSIONS: &[&str] = &[
    "vrm",
    "fbx",
    "pmx",
    "pmd",
    "unitypackage",
    "vrma",
    "glb",
    "gltf",
    "anim",
    "zip",
    "7z",
];

/// UI 表示用にキャッシュされた材質情報（借用制約回避 + 毎フレーム clone 回避）
pub struct CachedMaterialInfo {
    /// (draw_index, material_index)
    pub draw_indices: Vec<(usize, usize)>,
    /// 材質名
    pub names: Vec<String>,
    /// テクスチャインデックス
    pub tex_indices: Vec<Option<usize>>,
    /// FBX 元テクスチャファイル名
    pub source_tex_names: Vec<Option<String>>,
    /// テクスチャ設定済みカウント
    pub tex_set_count: usize,
    /// ステータスバー用テクスチャ状況文字列（毎フレーム format! 回避）
    pub tex_status_text: String,
}

/// ステータスバー用キャッシュ
pub struct CachedStats {
    pub total_vertices: usize,
    pub total_faces: usize,
    /// 事前フォーマット済みステータス文字列（毎フレーム format! 回避）
    pub status_text: String,
}

impl CachedStats {
    fn new(ir: &IrModel) -> Self {
        let total_vertices = ir.total_vertices();
        let total_faces = ir.total_faces();
        let status_text = format!(
            "頂点:{} 面:{} 材質:{} テクスチャ:{} ボーン:{} モーフ:{}",
            total_vertices,
            total_faces,
            ir.materials.len(),
            ir.textures.len(),
            ir.bones.len(),
            ir.morphs.len(),
        );
        Self {
            total_vertices,
            total_faces,
            status_text,
        }
    }
}

/// 追加読み込みされたモデルの情報（リロード時に再マージ用）
#[derive(Clone)]
pub struct AppendedModel {
    pub source: ReloadableSource,
    /// unitypackage内の選択モデル名（FBX/VRM直接の場合はNone）
    pub pkg_model_name: Option<String>,
}

/// モデル別の材質・DrawCall 区間情報
#[derive(Clone)]
pub struct MaterialGroup {
    pub name: String,
    pub material_range: std::ops::Range<usize>,
    pub draw_range: std::ops::Range<usize>,
}

pub struct LoadedModel {
    pub ir: IrModel,
    pub gpu_model: GpuModel,
    pub source: ReloadableSource,
    /// メインモデルの Aスタンス/Tスタンス変換結果（merge の影響を受けない）
    pub primary_astance_result: crate::intermediate::types::AStanceResult,
    /// 追加読み込みされたモデル一覧（リロード時に再マージ用）
    pub appended_models: Vec<AppendedModel>,
    /// モデル別の材質・DrawCall 区間
    pub material_groups: Vec<MaterialGroup>,
    /// 材質情報キャッシュ（テクスチャ割り当て時に更新）
    pub mat_cache: CachedMaterialInfo,
    /// 統計キャッシュ
    pub stats_cache: CachedStats,
}

/// 変換結果の種類
pub enum ConvertResult {
    Success(String),
    /// 成功したが警告あり（赤文字オーバーレイ）
    Warning(String),
    Failure(String),
}

/// 表示時刻付き変換結果メッセージ
pub struct ConvertMessage {
    pub result: ConvertResult,
    pub shown_at: std::time::Instant,
}

impl ConvertMessage {
    pub fn new(result: ConvertResult) -> Self {
        Self {
            result,
            shown_at: std::time::Instant::now(),
        }
    }

    pub fn success(msg: impl Into<String>) -> Self {
        Self::new(ConvertResult::Success(msg.into()))
    }

    pub fn warning(msg: impl Into<String>) -> Self {
        Self::new(ConvertResult::Warning(msg.into()))
    }

    pub fn failure(msg: impl Into<String>) -> Self {
        Self::new(ConvertResult::Failure(msg.into()))
    }

    /// 表示開始からの経過秒数
    pub fn elapsed_secs(&self) -> f32 {
        self.shown_at.elapsed().as_secs_f32()
    }
}

/// FBX 読み込みモード（モデル/アニメーション/両方）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FbxLoadMode {
    ModelOnly,
    AnimationOnly,
    Both,
}

/// unitypackage 内に複数FBXがある場合の選択待ち状態
/// unitypackage 内のモデルファイル種別
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PkgModelType {
    Fbx,
    Vrm,
}

/// unitypackage アセット群から VRM/FBX モデルリストを構築
fn build_pkg_model_list(
    assets: &[crate::unitypackage::ExtractedAsset],
) -> Vec<(usize, String, PkgModelType)> {
    let mut list = Vec::new();
    for (idx, name) in crate::unitypackage::find_vrm_list(assets) {
        list.push((idx, name, PkgModelType::Vrm));
    }
    for (idx, name) in crate::unitypackage::find_fbx_list(assets) {
        list.push((idx, name, PkgModelType::Fbx));
    }
    list
}

pub struct PendingUnityPackage {
    pub assets: Vec<crate::unitypackage::ExtractedAsset>,
    /// (アセットIndex, ファイル名, モデル種別)
    pub model_list: Vec<(usize, String, PkgModelType)>,
    pub source_path: PathBuf,
    /// アペンドモード（既存モデルに追加）
    pub append: bool,
    /// 一時ファイルからの読み込み時、アーカイブデータのスナップショット
    pub archive_snapshot: Option<Arc<[u8]>>,
    /// アーカイブ(ZIP/7z)内 .unitypackage の場合、リロード用のソース情報
    pub nested_archive_source: Option<ReloadableSource>,
}

/// unitypackage モデル遅延読み込み状態
pub struct PendingPkgModelLoad {
    pub assets: Vec<crate::unitypackage::ExtractedAsset>,
    pub fbx_index: usize,
    pub model_type: PkgModelType,
    pub source_path: PathBuf,
    /// オーバーレイ表示済みフラグ
    pub shown: bool,
    /// アペンドモード（既存モデルに追加）
    pub append: bool,
    /// テクスチャ選択ダイアログを抑制（リロード経由時）
    pub suppress_tex_match: bool,
    /// 一時ファイルからの読み込み時、アーカイブデータのスナップショット
    pub archive_snapshot: Option<Arc<[u8]>>,
    /// アーカイブ(ZIP/7z)内 .unitypackage の場合、リロード用のソース情報
    pub nested_archive_source: Option<ReloadableSource>,
}

/// アーカイブ内モデル選択待ち
pub struct PendingArchive {
    pub archive_data: Arc<[u8]>,
    pub format: crate::archive::ArchiveFormat,
    pub contents: crate::archive::ArchiveContents,
    pub source_path: PathBuf,
    pub append: bool,
    pub is_temp: bool,
}

/// アーカイブ内モデル遅延読み込み
pub struct PendingArchiveLoad {
    pub archive_data: Arc<[u8]>,
    pub format: crate::archive::ArchiveFormat,
    pub contents: crate::archive::ArchiveContents,
    pub model_index: usize,
    pub source_path: PathBuf,
    pub shown: bool,
    pub append: bool,
    pub is_temp: bool,
}

/// FBX読み込み方法選択ダイアログの状態（モデル+アニメーション両方含むFBX用）
pub struct PendingFbxChoice {
    pub path: PathBuf,
    pub load_model: bool,
    pub load_animation: bool,
    /// unitypackage 経由の場合のデータ
    pub pkg_context: Option<PendingFbxChoicePkg>,
    /// D&D一時ファイルの先読みデータ
    pub preloaded: Option<PreloadedData>,
}

/// unitypackage 経由 FBX 選択時の追加コンテキスト
pub struct PendingFbxChoicePkg {
    pub assets: Vec<crate::unitypackage::ExtractedAsset>,
    pub fbx_index: usize,
    pub source_path: PathBuf,
    /// 一時ファイルからの読み込み時、アーカイブデータのスナップショット
    pub archive_snapshot: Option<Arc<[u8]>>,
    /// アーカイブ(ZIP/7z)内 .unitypackage の場合、リロード用のソース情報
    pub nested_archive_source: Option<ReloadableSource>,
}

/// unitypackage テクスチャ手動割当ダイアログの状態
pub struct PendingTexMatch {
    /// 未割当の材質インデックス一覧（ir.materials 内のインデックス）
    pub mat_indices: Vec<usize>,
    /// 材質インデックス → 選択中のテクスチャインデックス (pkg_textures 内)
    pub selections: Vec<Option<usize>>,
    /// テクスチャ名フィルタ
    pub tex_filter: String,
}

/// 表示・描画関連の設定
pub struct DisplaySettings {
    /// ライト明るさ (0.0〜2.0)
    pub light_intensity: f32,
    /// 環境光 (0.0〜1.0)
    pub ambient_intensity: f32,
    /// 背景明るさ (0.0〜1.0)
    pub bg_brightness: f32,
    /// グリッド表示
    pub show_grid: bool,
    /// ボーン表示
    pub show_bones: bool,
    /// ボーン濃度
    pub bone_opacity: f32,
    /// SpringBone（物理）表示
    pub show_spring_bones: bool,
    /// SpringBone 濃度
    pub spring_bone_opacity: f32,
    /// ジョイント表示（PMX/PMDのみ）
    pub show_joints: bool,
    /// ジョイント濃度
    pub joint_opacity: f32,
    /// 描画モード
    pub draw_mode: DrawMode,
    /// ライトモード
    pub light_mode: LightMode,
    /// 剛体回転をボーン方向に揃える（PMX出力 + 物理表示）
    pub align_rigid_rotation: bool,
    /// MSAA アンチエイリアス
    pub msaa: bool,
    /// 法線平滑化（頂点統合 + 法線平均化）
    pub smooth_normals: bool,
    /// カスタム法線クリア（ジオメトリから法線を再計算）
    pub clear_custom_normals: bool,
    /// 法線表示
    pub show_normals: bool,
    /// 法線表示の長さ
    pub normal_length: f32,
    /// 法線マップ表示（法線ベクトル→RGB）
    pub show_normal_map: bool,
    /// MMD レンダリングモード
    pub mmd_mode: bool,
    /// MMD エッジ描画
    pub mmd_edge_enabled: bool,
    /// MMD エッジ太さ全体スケール (0.1〜3.0)
    pub mmd_edge_thickness: f32,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            light_intensity: 0.7,
            ambient_intensity: 0.5,
            bg_brightness: 0.19,
            show_grid: true,
            show_bones: false,
            bone_opacity: 0.85,
            show_spring_bones: false,
            spring_bone_opacity: 0.75,
            show_joints: false,
            joint_opacity: 0.75,
            draw_mode: DrawMode::Solid,
            light_mode: LightMode::CameraFollow,
            align_rigid_rotation: false,
            msaa: true,
            smooth_normals: false,
            clear_custom_normals: false,
            show_normals: false,
            normal_length: 0.1,
            show_normal_map: false,
            mmd_mode: false,
            mmd_edge_enabled: true,
            mmd_edge_thickness: 1.0,
        }
    }
}

/// テクスチャ割り当て・パッケージテクスチャ関連の状態
pub struct TextureState {
    /// 手動テクスチャ割り当て履歴（材質Index → テクスチャソース）
    pub assignments: HashMap<usize, TextureSource>,
    /// パッケージテクスチャ手動割り当て履歴（材質Index → テクスチャ名）
    pub pkg_assignments: HashMap<usize, String>,
    /// テクスチャD&Dプレビュー
    pub pending_preview: Option<PendingTexPreview>,
    /// unitypackageテクスチャ手動割当ダイアログ
    pub pending_match: Option<PendingTexMatch>,
    /// unitypackage内テクスチャ（モデル読み込み中保持）
    pub pkg_textures: Option<Vec<(String, Vec<u8>)>>,
    /// pkg_textures のサムネイル TextureId キャッシュ
    pub pkg_thumb_cache: Vec<Option<egui::TextureId>>,
    /// 同一材質名への同時テクスチャ割り当て
    pub link_same_name: bool,
    /// pkgテクスチャポップアップ用フィルタ
    pub pkg_popup_filter: String,
    /// 最後にテクスチャファイルを開いたディレクトリ
    pub last_dir: Option<PathBuf>,
}

impl Default for TextureState {
    fn default() -> Self {
        Self {
            assignments: HashMap::new(),
            pkg_assignments: HashMap::new(),
            pending_preview: None,
            pending_match: None,
            pkg_textures: None,
            pkg_thumb_cache: Vec::new(),
            link_same_name: true,
            pkg_popup_filter: String::new(),
            last_dir: None,
        }
    }
}

/// テクスチャD&Dプレビュー状態
pub struct PendingTexPreview {
    pub path: PathBuf,
    /// 読み込み済みバイトデータ（一時ファイル消失対策）
    pub cached_data: Vec<u8>,
    /// PSDファイルかどうか
    pub is_psd: bool,
    /// 一時パスから読み込まれたか（消失前に判定済み）
    pub was_temp: bool,
    /// 材質ごとの選択状態（チェックボックス）
    pub selection: Vec<bool>,
    /// 現在プレビュー適用中の材質
    previewed: Vec<bool>,
    /// プレビュー用テクスチャビュー（GPU）
    texture_view: wgpu::TextureView,
    /// draw_index → 退避した元の bind group
    saved_binds: HashMap<usize, Option<wgpu::BindGroup>>,
    /// サムネイル表示用 egui TextureId
    pub preview_tex_id: Option<egui::TextureId>,
}

/// 遅延処理のオーバーレイ表示状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingOverlay {
    /// オーバーレイ未表示（次フレームで表示）
    WaitingOverlay,
    /// オーバーレイ表示済み（次フレームで実行）
    Ready,
}

/// 右パネルのタブ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidePanelTab {
    /// 情報: モデル情報 + メタ情報
    Info,
    /// 操作: 表情モーフ + アニメーション制御
    Control,
    /// 表示: 表示設定 + 材質表示
    Display,
    /// 出力: PMX変換 + UVマップ
    Export,
}

/// アニメーションライブラリ・再生管理
pub struct AnimLibrary {
    /// VRMA アニメーション再生状態
    pub state: Option<AnimationState>,
    /// 読み込み済みVRMAライブラリ（名前, パス, アニメーションデータ）
    pub library: Vec<(
        String,
        PathBuf,
        Arc<crate::intermediate::animation::VrmaAnimation>,
    )>,
    /// 現在アクティブなVRMAのインデックス
    pub active_index: Option<usize>,
    /// Unity .anim Muscle 角度スケール
    pub muscle_scale: f32,
}

impl Default for AnimLibrary {
    fn default() -> Self {
        Self {
            state: None,
            library: Vec::new(),
            active_index: None,
            muscle_scale: 1.0,
        }
    }
}

/// 遅延処理（ペンディング）の集約状態
pub struct PendingState {
    /// FBX読み込み方法選択待ち（モデル+アニメ両方含む場合）
    pub fbx_choice: Option<PendingFbxChoice>,
    /// unitypackage FBX選択待ち
    pub unity_pkg: Option<PendingUnityPackage>,
    /// FBX遅延読み込み
    pub pkg_load: Option<PendingPkgModelLoad>,
    /// アーカイブ内モデル選択待ち
    pub archive: Option<PendingArchive>,
    /// アーカイブモデル遅延読み込み
    pub archive_load: Option<PendingArchiveLoad>,
    /// ファイル読み込み遅延実行 (path, overlay表示済みフラグ)
    pub load: Option<(PathBuf, bool)>,
    /// モデル追加読み込み遅延実行 (path, overlay表示済みフラグ)
    pub append: Option<(PathBuf, bool)>,
    /// PMX変換遅延実行
    pub convert: Option<PendingOverlay>,
    /// GPU再構築遅延実行
    pub rebuild: Option<PendingOverlay>,
    /// モデル再読み込み遅延実行
    pub reload: Option<PendingOverlay>,
    /// ビューポートサイズ確定後の refit（初回ロード時）
    pub refit: bool,
}

impl Default for PendingState {
    fn default() -> Self {
        Self {
            fbx_choice: None,
            unity_pkg: None,
            pkg_load: None,
            archive: None,
            archive_load: None,
            load: None,
            append: None,
            convert: None,
            rebuild: None,
            reload: None,
            refit: false,
        }
    }
}

/// PMXエクスポート関連の状態
pub struct ExportState {
    /// PMX変換時にログファイルを出力するか
    pub output_log: bool,
    /// PMX出力パス（テキストボックス編集用）
    pub pmx_output_path: String,
    /// 表示材質のみPMX出力（デフォルト: false）
    pub export_visible_only: bool,
    /// UVマップ出力解像度
    pub uv_map_size: u32,
    /// 物理（剛体・ジョイント）なしで出力
    pub no_physics: bool,
    /// 元のボーン構造のまま出力（標準ボーン挿入スキップ）
    pub raw_structure: bool,
}

impl Default for ExportState {
    fn default() -> Self {
        Self {
            output_log: false,
            pmx_output_path: String::new(),
            export_visible_only: false,
            uv_map_size: crate::convert::uvmap::DEFAULT_UV_SIZE,
            no_physics: false,
            raw_structure: false,
        }
    }
}

/// ビューアのメイン状態
pub struct ViewerApp {
    pub loaded: Option<LoadedModel>,
    pub camera: OrbitCamera,
    pub renderer: Option<GpuRenderer>,
    pub convert_message: Option<ConvertMessage>,
    /// 表情モーフのスライダ値
    pub morph_weights: Vec<f32>,
    /// モーフウェイト変更フラグ
    pub morph_dirty: bool,
    /// 表示・描画設定
    pub display: DisplaySettings,
    /// PMXエクスポート関連の状態
    pub export: ExportState,
    /// 材質ごとの表示ON/OFF
    pub material_visibility: Vec<bool>,
    /// 材質フィルター文字列
    pub material_filter: String,
    /// ドラッグオーバー中フラグ
    pub drag_hovering: bool,
    /// ビューポートテクスチャID
    pub viewport_texture_id: Option<egui::TextureId>,
    /// wgpu render state（CreationContext から取得）
    render_state: egui_wgpu::RenderState,
    /// Tポーズ→Aスタンス変換（トグル時に再読み込み）
    pub normalize_pose: bool,
    /// Aスタンス→Tスタンス変換（FBX用、トグル時に再読み込み）
    pub normalize_to_tstance: bool,
    /// ビューポートの幅（フィット計算用）
    pub last_viewport_width: f32,
    /// ビューポートの高さ（フィット計算用）
    pub last_viewport_height: f32,
    /// テクスチャ割り当て状態
    pub tex: TextureState,
    /// 遅延処理の集約状態
    pub pending: PendingState,
    /// FPS計測: フレームタイムスタンプのリングバッファ（直近1秒分）
    frame_times: VecDeque<Instant>,
    /// FPS計測: 前回フレームからの経過時間（ms）
    frame_dt_ms: f32,
    /// 表示用FPS（0.25秒間隔で更新）
    fps_display: f32,
    /// FPS表示の最終更新時刻
    fps_last_update: Instant,
    /// IPC受信チャネル（シングルインスタンス用）
    #[cfg(target_os = "windows")]
    ipc_receiver: std::sync::mpsc::Receiver<PathBuf>,
    /// ログディレクトリパス
    pub logs_dir: PathBuf,
    /// 現在のログファイルパス
    pub log_path: PathBuf,
    /// 最後にモデルファイルを開いたディレクトリ（ダイアログ経由のみ）
    pub last_model_dir: Option<PathBuf>,
    /// unitypackage 内で選択された FBX ファイル名（reload 時の照合用）
    pub selected_fbx_name: Option<String>,
    /// アニメーションライブラリ
    pub anim: AnimLibrary,
    /// 右パネルの現在のタブ
    pub side_panel_tab: SidePanelTab,
    /// ウィンドウタイトル更新要求
    pub window_title: Option<String>,
    /// テクスチャ手動割当ダイアログを抑制（リロード中に使用）
    pub suppress_tex_match: bool,
    /// D&D一時ファイルの先読みデータ（ロードチェーン中のみ使用）
    preloaded: Option<PreloadedData>,
}

impl ViewerApp {
    pub fn new(cc: &eframe::CreationContext, logs_dir: PathBuf, log_path: PathBuf) -> Self {
        let render_state = cc
            .wgpu_render_state
            .clone()
            .expect("wgpu render state required");

        // 日本語フォント読み込み
        Self::setup_japanese_font(&cc.egui_ctx);

        // シングルインスタンス: IPC パイプリスナー起動
        #[cfg(target_os = "windows")]
        let ipc_receiver = {
            let (tx, rx) = std::sync::mpsc::channel();
            super::single_instance::start_pipe_listener(tx, cc.egui_ctx.clone());
            rx
        };

        Self {
            loaded: None,
            camera: OrbitCamera::default(),
            renderer: None,
            convert_message: None,
            morph_weights: Vec::new(),
            morph_dirty: false,
            display: DisplaySettings::default(),
            material_visibility: Vec::new(),
            export: ExportState::default(),
            material_filter: String::new(),
            drag_hovering: false,
            viewport_texture_id: None,
            render_state,
            normalize_pose: false,
            normalize_to_tstance: false,
            last_viewport_width: 1280.0,
            last_viewport_height: 720.0,
            tex: TextureState::default(),
            pending: PendingState::default(),
            frame_times: VecDeque::with_capacity(120),
            frame_dt_ms: 0.0,
            fps_display: 0.0,
            fps_last_update: Instant::now(),
            #[cfg(target_os = "windows")]
            ipc_receiver,
            logs_dir,
            log_path,
            last_model_dir: None,
            selected_fbx_name: None,
            anim: AnimLibrary::default(),
            side_panel_tab: SidePanelTab::Info,
            window_title: None,
            suppress_tex_match: false,
            preloaded: None,
        }
    }

    fn setup_japanese_font(ctx: &egui::Context) {
        // Noto Sans JP（OFL ライセンス）をバイナリに組み込み
        const NOTO_SANS_JP: &[u8] = include_bytes!("../../assets/NotoSansJP-Regular.ttf");

        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "noto_jp".to_owned(),
            egui::FontData::from_static(NOTO_SANS_JP).into(),
        );
        fonts
            .families
            .get_mut(&egui::FontFamily::Proportional)
            .expect("Proportional フォントファミリーは常に存在")
            .insert(0, "noto_jp".to_owned());
        fonts
            .families
            .get_mut(&egui::FontFamily::Monospace)
            .expect("Monospace フォントファミリーは常に存在")
            .push("noto_jp".to_owned());
        ctx.set_fonts(fonts);
    }

    /// preloaded の aux_files があればそれを移動（clone回避）、なければディスクから再帰収集する
    fn take_or_collect_aux(&mut self, path: &Path) -> HashMap<PathBuf, Arc<[u8]>> {
        if let Some(ref pl) = self.preloaded {
            if pl.path == path {
                // preloaded から aux_files を移動（HashMap の再割り当て回避）
                let pl = self.preloaded.take().expect("preloaded は Some 確認済み");
                self.preloaded = Some(PreloadedData {
                    path: pl.path,
                    main_bytes: pl.main_bytes,
                    aux_files: HashMap::new(),
                });
                return pl.aux_files;
            }
        }
        let mut aux = HashMap::new();
        if let Some(dir) = path.parent() {
            collect_image_files_recursive(dir, dir, &mut aux);
        }
        aux
    }

    /// temp先読みデータがあればそれを、なければファイルから読む
    fn read_or_preloaded(&self, path: &Path) -> anyhow::Result<Arc<[u8]>> {
        if let Some(ref pl) = self.preloaded {
            if pl.path == path {
                return Ok(Arc::clone(&pl.main_bytes));
            }
            // aux_files も確認（サブファイル参照用）
            if let Some(data) = pl.aux_files.get(path) {
                return Ok(Arc::clone(data));
            }
        }
        Ok(std::fs::read(path)?.into())
    }

    fn load_file(&mut self, path: PathBuf) {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        // unitypackage以外の読み込み時はパッケージテクスチャをクリア
        if ext != "unitypackage" {
            self.tex.pkg_textures = None;
            self.clear_pkg_thumb_cache();
            self.tex.pending_match = None;
        }

        // アニメーションファイルの判定
        if ext == "vrma" {
            self.try_load_vrma(&path);
            return;
        }
        // GLB/glTF: モデルが読み込み済みの場合、アニメーションとして開くか確認
        if (ext == "glb" || ext == "gltf") && self.loaded.is_some() {
            // アニメーションが含まれるか先に確認
            if let Ok(anims) = vrm::animation::load_gltf_animation(&path) {
                if !anims.is_empty() {
                    self.try_load_gltf_animation(&path);
                    return;
                }
            }
        }
        // Unity .anim: アニメーションとして読み込む
        if ext == "anim" && self.loaded.is_some() {
            self.try_load_unity_animation(&path);
            return;
        }

        // FBX: モデル読み込み済みの場合、メッシュ+アニメーション両方含むなら選択ダイアログ
        if ext == "fbx" && self.loaded.is_some() {
            // ファイルを1回だけ読み込んで、メッシュとアニメーションの有無を判定
            let data = match self.read_or_preloaded(&path) {
                Ok(d) => d,
                Err(_) => {
                    self.load_file_as_model(path);
                    return;
                }
            };
            let has_mesh = crate::fbx::extract::fbx_has_mesh(&data);
            let has_anim = crate::fbx::animation::load_fbx_animation_from_data(&data)
                .is_ok_and(|a| !a.is_empty());

            if has_mesh && has_anim {
                // 両方含む → 選択ダイアログを表示
                self.pending.fbx_choice = Some(PendingFbxChoice {
                    path: path.clone(),
                    load_model: true,
                    load_animation: true,
                    pkg_context: None,
                    preloaded: self.preloaded.take(),
                });
                return;
            } else if !has_mesh && has_anim {
                // アニメーションのみ
                self.try_load_fbx_animation(&path);
                return;
            }
            // メッシュのみ or どちらもなし → モデルとして読み込み（下へ続行）
        }

        self.load_file_as_model(path);
    }

    /// モデルとしてファイルを読み込む（FBX選択ダイアログ不要時のパス）
    fn load_file_as_model(&mut self, path: PathBuf) {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let result = match ext.as_str() {
            "fbx" => self.try_load_fbx(&path),
            "unitypackage" => self.try_load_unitypackage(&path),
            "pmx" => self.try_load_pmx(&path),
            "pmd" => self.try_load_pmd(&path),
            "zip" | "7z" => self.try_load_archive(&path),
            _ => self.try_load_vrm(&path),
        };

        match result {
            Ok(()) => {
                log::info!("読み込み成功: {}", path.display());
                self.convert_message = None;
                self.anim.state = None;
                self.anim.library.clear();
                self.anim.active_index = None;

                // FBXモデル読み込み後、同じファイルにアニメーションがあれば自動適用
                if ext == "fbx" {
                    let anim_result = match self.read_or_preloaded(&path) {
                        Ok(data) => crate::fbx::animation::load_fbx_animation_from_data(&data),
                        Err(_) => crate::fbx::animation::load_fbx_animation(&path),
                    };
                    if let Ok(anims) = anim_result {
                        if !anims.is_empty() {
                            self.try_load_fbx_animation(&path);
                        }
                    }
                }
            }
            Err(e) => {
                log::error!("読み込み失敗: {e}");
                let user_msg = format!(
                    "ファイルを読み込めませんでした。\n\
                     ファイルが破損していないか、対応形式（VRM/FBX/PMX/PMD/ZIP/7z）であるか確認してください。\n\
                     詳細: {e}"
                );
                self.convert_message = Some(ConvertMessage::failure(user_msg));
            }
        }
    }

    /// FBX読み込み方法選択ダイアログの結果を実行
    pub fn execute_fbx_choice(&mut self, choice: PendingFbxChoice) {
        let PendingFbxChoice {
            path,
            load_model,
            load_animation,
            pkg_context,
            preloaded,
        } = choice;
        self.preloaded = preloaded;

        let mode = match (load_model, load_animation) {
            (true, true) => FbxLoadMode::Both,
            (true, false) => FbxLoadMode::ModelOnly,
            (false, true) => FbxLoadMode::AnimationOnly,
            (false, false) => return,
        };

        if let Some(pkg) = pkg_context {
            // unitypackage 経由: source_override を構築
            let source_override = if let Some(nested) = pkg.nested_archive_source {
                Some(nested)
            } else if let Some(ref snap) = pkg.archive_snapshot {
                Some(ReloadableSource::Snapshot {
                    original_path: pkg.source_path.clone(),
                    main_bytes: Arc::clone(snap),
                    aux_files: HashMap::new(),
                })
            } else {
                None
            };
            match self.load_fbx_from_assets(
                pkg.assets,
                pkg.fbx_index,
                &pkg.source_path,
                mode,
                source_override,
            ) {
                Ok(()) => {
                    log::info!("読み込み成功: {}", pkg.source_path.display());
                    self.convert_message = None;
                }
                Err(e) => {
                    log::error!("読み込み失敗: {e}");
                    self.convert_message = Some(ConvertMessage::failure(format!(
                        "ファイルを読み込めませんでした。\n詳細: {e}"
                    )));
                }
            }
        } else {
            // ファイル直接
            match mode {
                FbxLoadMode::Both | FbxLoadMode::ModelOnly => match self.try_load_fbx(&path) {
                    Ok(()) => {
                        log::info!("FBXモデル読み込み成功: {}", path.display());
                        self.convert_message = None;
                        self.anim.state = None;
                        self.anim.library.clear();
                        self.anim.active_index = None;

                        if mode == FbxLoadMode::Both {
                            self.try_load_fbx_animation(&path);
                        }
                    }
                    Err(e) => {
                        log::error!("読み込み失敗: {e}");
                        self.convert_message = Some(ConvertMessage::failure(format!(
                            "ファイルを読み込めませんでした。\n詳細: {e}"
                        )));
                    }
                },
                FbxLoadMode::AnimationOnly => {
                    self.try_load_fbx_animation(&path);
                }
            }
        }
        self.preloaded = None;
    }

    fn try_load_fbx(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let data = self.read_or_preloaded(path)?;
        let ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
            &data,
            Some(path),
            self.normalize_pose,
            self.normalize_to_tstance,
        )?;
        let source =
            if is_temp_path(path) || self.preloaded.as_ref().is_some_and(|pl| pl.path == path) {
                let aux = self.take_or_collect_aux(path);
                ReloadableSource::Snapshot {
                    original_path: path.to_path_buf(),
                    main_bytes: data,
                    aux_files: aux,
                }
            } else {
                ReloadableSource::File(path.to_path_buf())
            };
        self.finish_load(ir, source)
    }

    fn try_load_unitypackage(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        // ファイル消失前に一時パス判定を確定（canonicalize がファイル存在を前提とするため）
        let is_temp =
            is_temp_path(path) || self.preloaded.as_ref().is_some_and(|pl| pl.path == path);
        let archive_data: Arc<[u8]> = self.read_or_preloaded(path)?;
        let assets = crate::unitypackage::extract_all_assets(&archive_data)?;

        // 一時ファイルの場合はアーカイブデータをスナップショット
        let snapshot = if is_temp {
            Some(Arc::clone(&archive_data))
        } else {
            None
        };

        // FBX と VRM を統合したモデルリストを構築
        let model_list = build_pkg_model_list(&assets);

        if model_list.is_empty() {
            anyhow::bail!(".unitypackage 内に VRM / FBX ファイルが見つかりません");
        }

        if model_list.len() == 1 {
            // モデルが1つだけ → プログレス表示後に遅延ロード
            let (idx, _, model_type) = model_list[0];
            self.pending.pkg_load = Some(PendingPkgModelLoad {
                assets,
                fbx_index: idx,
                model_type,
                source_path: path.to_path_buf(),
                shown: false,
                append: false,
                suppress_tex_match: false,
                archive_snapshot: snapshot,
                nested_archive_source: None,
            });
        } else {
            // 複数 → 選択ダイアログを表示
            log::info!(
                ".unitypackage 内に {} 個のモデルが見つかりました:",
                model_list.len()
            );
            for (_, name, mtype) in &model_list {
                log::info!("  {:?}: {}", mtype, name);
            }
            self.pending.unity_pkg = Some(PendingUnityPackage {
                assets,
                model_list,
                source_path: path.to_path_buf(),
                append: false,
                archive_snapshot: snapshot,
                nested_archive_source: None,
            });
        }
        Ok(())
    }

    /// unitypackage をアペンドモードで読み込み
    fn try_load_unitypackage_for_append(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        // ファイル消失前に一時パス判定を確定（canonicalize がファイル存在を前提とするため）
        let is_temp =
            is_temp_path(path) || self.preloaded.as_ref().is_some_and(|pl| pl.path == path);
        let archive_data: Arc<[u8]> = self.read_or_preloaded(path)?;
        let assets = crate::unitypackage::extract_all_assets(&archive_data)?;

        // 一時ファイルの場合はアーカイブデータをスナップショット
        let snapshot = if is_temp {
            Some(Arc::clone(&archive_data))
        } else {
            None
        };

        let model_list = build_pkg_model_list(&assets);

        if model_list.is_empty() {
            anyhow::bail!(".unitypackage 内に VRM / FBX ファイルが見つかりません");
        }

        if model_list.len() == 1 {
            let (idx, _, model_type) = model_list[0];
            self.pending.pkg_load = Some(PendingPkgModelLoad {
                assets,
                fbx_index: idx,
                model_type,
                source_path: path.to_path_buf(),
                shown: false,
                append: true,
                suppress_tex_match: self.suppress_tex_match,
                archive_snapshot: snapshot,
                nested_archive_source: None,
            });
        } else {
            self.pending.unity_pkg = Some(PendingUnityPackage {
                assets,
                model_list,
                source_path: path.to_path_buf(),
                append: true,
                archive_snapshot: snapshot,
                nested_archive_source: None,
            });
        }
        Ok(())
    }

    /// アーカイブ（ZIP/7z）を読み込み
    fn try_load_archive(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        self.try_load_archive_impl(path, false)
    }

    /// アーカイブをアペンドモードで読み込み
    fn try_load_archive_for_append(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        self.try_load_archive_impl(path, true)
    }

    fn try_load_archive_impl(
        &mut self,
        path: &std::path::Path,
        append: bool,
    ) -> anyhow::Result<()> {
        let is_temp =
            is_temp_path(path) || self.preloaded.as_ref().is_some_and(|pl| pl.path == path);
        let archive_data: Arc<[u8]> = self.read_or_preloaded(path)?;

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let format = crate::archive::archive_format_from_ext(&ext)
            .ok_or_else(|| anyhow::anyhow!("未対応のアーカイブ形式: {ext}"))?;

        let contents = crate::archive::list_models(&archive_data, format)?;

        if contents.models.is_empty() {
            anyhow::bail!("アーカイブ内にモデルファイルが見つかりません");
        }

        if contents.models.len() == 1 {
            // モデルが1つだけ → 遅延ロード
            self.pending.archive_load = Some(PendingArchiveLoad {
                archive_data,
                format,
                contents,
                model_index: 0,
                source_path: path.to_path_buf(),
                shown: false,
                append,
                is_temp,
            });
        } else {
            // 複数 → 選択ダイアログ
            log::info!(
                "アーカイブ内に {} 個のモデルが見つかりました:",
                contents.models.len()
            );
            for (_, p, _, kind) in &contents.models {
                log::info!("  [{}] {}", kind.label(), p.display());
            }
            self.pending.archive = Some(PendingArchive {
                archive_data,
                format,
                contents,
                source_path: path.to_path_buf(),
                append,
                is_temp,
            });
        }
        Ok(())
    }

    /// アーカイブからモデルを読み込み
    fn load_model_from_archive(&mut self, pending: PendingArchiveLoad) -> anyhow::Result<()> {
        let model_path = pending.contents.models[pending.model_index].1.clone();
        let kind = pending.contents.models[pending.model_index].3;

        let bundle = crate::archive::extract_model_bundle(
            &pending.archive_data,
            pending.format,
            pending.contents,
            pending.model_index,
        )?;

        // UnityPackage: 二重展開 → 既存の unitypackage フローへ接続
        if kind == crate::archive::ArchiveModelKind::UnityPackage {
            return self.load_unitypackage_from_archive(
                bundle.model.data,
                pending.source_path,
                pending.is_temp,
                pending.archive_data,
                pending.append,
                model_path,
            );
        }

        let ir = self.build_ir_from_archive_bundle(&bundle, &pending.source_path)?;

        let source = ReloadableSource::Archive {
            original_path: pending.source_path,
            archive_bytes: if pending.is_temp {
                Some(pending.archive_data)
            } else {
                None
            },
            selected_entry_path: model_path.to_string_lossy().into_owned(),
            inner_kind: kind,
        };

        if pending.append {
            self.finish_append_with_source(ir, source, None);
            Ok(())
        } else {
            self.finish_load(ir, source)
        }
    }

    /// アーカイブ内 .unitypackage を展開し、既存の unitypackage 読み込みフローへ接続
    fn load_unitypackage_from_archive(
        &mut self,
        pkg_data: Vec<u8>,
        source_path: PathBuf,
        is_temp: bool,
        archive_data: Arc<[u8]>,
        append: bool,
        entry_path: PathBuf,
    ) -> anyhow::Result<()> {
        let assets = crate::unitypackage::extract_all_assets(&pkg_data)?;

        // VRM / FBX を検出
        let model_list = build_pkg_model_list(&assets);
        if model_list.is_empty() {
            anyhow::bail!(".unitypackage 内に VRM / FBX ファイルが見つかりません");
        }

        // アーカイブスナップショット（一時ファイルの場合のみ保持）
        let archive_snapshot = if is_temp {
            Some(Arc::clone(&archive_data))
        } else {
            None
        };

        // リロード時に Archive 経由で二重展開するための情報
        let nested_archive_source = Some(ReloadableSource::Archive {
            original_path: source_path.clone(),
            archive_bytes: if is_temp { Some(archive_data) } else { None },
            selected_entry_path: entry_path.to_string_lossy().into_owned(),
            inner_kind: crate::archive::ArchiveModelKind::UnityPackage,
        });

        if model_list.len() == 1 {
            let (model_index, ref _name, model_type) = model_list[0];
            log::info!("アーカイブ内 .unitypackage: モデル1個検出");
            self.pending.pkg_load = Some(PendingPkgModelLoad {
                assets,
                fbx_index: model_index,
                model_type,
                source_path: source_path.clone(),
                shown: false,
                append,
                suppress_tex_match: false,
                archive_snapshot,
                nested_archive_source,
            });
        } else {
            log::info!(
                "アーカイブ内 .unitypackage: {} 個のモデルが見つかりました:",
                model_list.len()
            );
            for (_, name, mt) in &model_list {
                let label = match mt {
                    PkgModelType::Vrm => "VRM",
                    PkgModelType::Fbx => "FBX",
                };
                log::info!("  [{}] {}", label, name);
            }
            self.pending.unity_pkg = Some(PendingUnityPackage {
                assets,
                model_list,
                source_path: source_path.clone(),
                append,
                archive_snapshot,
                nested_archive_source,
            });
        }
        Ok(())
    }

    /// アーカイブバンドルから IrModel を構築
    fn build_ir_from_archive_bundle(
        &self,
        bundle: &crate::archive::ModelBundle,
        source_path: &Path,
    ) -> anyhow::Result<IrModel> {
        use crate::archive::ArchiveModelKind;
        match bundle.kind {
            ArchiveModelKind::Pmx => {
                let pmx_model = crate::pmx::reader::read_pmx_from_data(&bundle.model.data)?;
                let mut ir = crate::pmx::extract::pmx_to_ir_with_aux(
                    &pmx_model,
                    std::path::Path::new("."),
                    Some(&bundle.aux_files),
                )?;
                if self.normalize_pose {
                    ir.astance_result = crate::intermediate::pose::normalize_pose_to_tstance_full(
                        &mut ir.bones,
                        &mut ir.meshes,
                        &mut ir.morphs,
                        &mut ir.physics,
                        crate::convert::coord::gltf_pos_to_pmx,
                    );
                }
                Ok(ir)
            }
            ArchiveModelKind::Pmd => {
                let pmd_model = crate::pmd::reader::read_pmd_from_data(&bundle.model.data)?;
                let mut ir = crate::pmd::extract::pmd_to_ir_with_aux(
                    &pmd_model,
                    &bundle.model.path,
                    Some(&bundle.aux_files),
                )?;
                if self.normalize_pose {
                    ir.astance_result = crate::intermediate::pose::normalize_pose_to_tstance_full(
                        &mut ir.bones,
                        &mut ir.meshes,
                        &mut ir.morphs,
                        &mut ir.physics,
                        crate::convert::coord::gltf_pos_to_pmx,
                    );
                }
                Ok(ir)
            }
            ArchiveModelKind::Fbx => {
                let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                    &bundle.model.data,
                    Some(source_path),
                    self.normalize_pose,
                    self.normalize_to_tstance,
                )?;
                crate::unitypackage::embed_textures_into_ir(&mut ir, &bundle.textures);
                Ok(ir)
            }
            ArchiveModelKind::Vrm | ArchiveModelKind::Glb => {
                let glb = vrm::loader::load_glb_from_data(&bundle.model.data)?;
                let version = vrm::detect::detect_version(&glb.document);
                let all_extensions = vrm::loader::get_raw_extensions(&glb.document);
                let ir = vrm::extract::extract_ir_model_with_options(
                    &glb.document,
                    &glb.buffers,
                    &glb.images,
                    &glb.vrm_extension,
                    &version,
                    &all_extensions,
                    self.normalize_pose,
                )?;
                Ok(ir)
            }
            ArchiveModelKind::UnityPackage => {
                // load_model_from_archive で先に分岐済み。ここに到達することはない
                anyhow::bail!("UnityPackage は build_ir_from_archive_bundle では処理できません")
            }
        }
    }

    /// ReloadableSource::Archive からIrModelを構築（reload/append共通）
    fn load_ir_from_archive_source(
        &self,
        original_path: &Path,
        archive_bytes: Option<&Arc<[u8]>>,
        selected_entry_path: &str,
        inner_kind: crate::archive::ArchiveModelKind,
    ) -> anyhow::Result<IrModel> {
        let owned;
        let data: &[u8] = if let Some(snap) = archive_bytes {
            snap
        } else {
            owned = std::fs::read(original_path)?;
            &owned
        };

        let ext = original_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let format = crate::archive::archive_format_from_ext(&ext)
            .ok_or_else(|| anyhow::anyhow!("未対応のアーカイブ形式: {ext}"))?;

        let contents = crate::archive::list_models(data, format)?;

        // selected_entry_path で同じモデルを再選択
        let model_index = contents
            .models
            .iter()
            .position(|(_, p, _, _)| p.to_string_lossy() == selected_entry_path)
            .ok_or_else(|| {
                anyhow::anyhow!("アーカイブ内に以前のモデルが見つかりません: {selected_entry_path}")
            })?;

        let bundle = crate::archive::extract_model_bundle(data, format, contents, model_index)?;
        let _ = inner_kind; // bundle.kind を使用
        self.build_ir_from_archive_bundle(&bundle, original_path)
    }

    /// アーカイブ(ZIP/7z)内の .unitypackage データを取り出す
    fn extract_pkg_from_archive(
        original_path: &Path,
        archive_bytes: Option<&Arc<[u8]>>,
        selected_entry_path: &str,
    ) -> anyhow::Result<Vec<u8>> {
        let owned;
        let data: &[u8] = if let Some(snap) = archive_bytes {
            snap
        } else {
            owned = std::fs::read(original_path)?;
            &owned
        };

        let ext = original_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let format = crate::archive::archive_format_from_ext(&ext)
            .ok_or_else(|| anyhow::anyhow!("未対応のアーカイブ形式: {ext}"))?;

        let contents = crate::archive::list_models(data, format)?;

        let model_index = contents
            .models
            .iter()
            .position(|(_, p, _, _)| p.to_string_lossy() == selected_entry_path)
            .ok_or_else(|| {
                anyhow::anyhow!("アーカイブ内に以前のモデルが見つかりません: {selected_entry_path}")
            })?;

        let bundle = crate::archive::extract_model_bundle(data, format, contents, model_index)?;
        Ok(bundle.model.data)
    }

    /// リロード時の unitypackage 同期アペンド（遅延処理を避け、テクスチャ復元も行う）
    fn reload_append_unitypackage(
        &mut self,
        source: &ReloadableSource,
        pkg_model_name: Option<&str>,
        saved_pkg_tex_assignments: &HashMap<usize, String>,
    ) {
        // Arc 参照で済むケースではコピーを避け、所有権が必要なパスのみ Vec を確保
        use std::borrow::Cow;
        let archive_data: Cow<'_, [u8]> = match source {
            ReloadableSource::Snapshot { main_bytes, .. } => Cow::Borrowed(main_bytes),
            ReloadableSource::File(path) => match std::fs::read(path) {
                Ok(d) => Cow::Owned(d),
                Err(e) => {
                    log::error!("unitypackage 再読み込み失敗: {e}");
                    return;
                }
            },
            ReloadableSource::Archive {
                original_path,
                archive_bytes,
                selected_entry_path,
                inner_kind,
            } => {
                if *inner_kind == crate::archive::ArchiveModelKind::UnityPackage {
                    // アーカイブ内 .unitypackage: 二重展開
                    match Self::extract_pkg_from_archive(
                        original_path,
                        archive_bytes.as_ref(),
                        selected_entry_path,
                    ) {
                        Ok(data) => Cow::Owned(data),
                        Err(e) => {
                            log::error!("アーカイブ内unitypackage展開失敗: {e}");
                            return;
                        }
                    }
                } else if let Some(snap) = archive_bytes {
                    Cow::Borrowed(snap.as_ref())
                } else {
                    match std::fs::read(original_path) {
                        Ok(d) => Cow::Owned(d),
                        Err(e) => {
                            log::error!("unitypackage 再読み込み失敗: {e}");
                            return;
                        }
                    }
                }
            }
        };
        let path = source.display_path();
        let assets = match crate::unitypackage::extract_all_assets(&archive_data) {
            Ok(a) => a,
            Err(e) => {
                log::error!("unitypackage 展開失敗: {e}");
                return;
            }
        };

        // 保存されたモデル名で照合（なければ selected_fbx_name にフォールバック）
        let fbx_list = crate::unitypackage::find_fbx_list(&assets);
        let vrm_list = crate::unitypackage::find_vrm_list(&assets);

        let search_name = pkg_model_name.or(self.selected_fbx_name.as_deref());
        let (model_index, model_type) = if let Some(prev_name) = search_name {
            if let Some((idx, _)) = fbx_list.iter().find(|(_, name)| name == prev_name) {
                (*idx, PkgModelType::Fbx)
            } else if let Some((idx, _)) = vrm_list.iter().find(|(_, name)| name == prev_name) {
                (*idx, PkgModelType::Vrm)
            } else if !fbx_list.is_empty() {
                (fbx_list[0].0, PkgModelType::Fbx)
            } else if !vrm_list.is_empty() {
                (vrm_list[0].0, PkgModelType::Vrm)
            } else {
                log::error!("unitypackage 内にモデルが見つかりません");
                return;
            }
        } else if !fbx_list.is_empty() {
            (fbx_list[0].0, PkgModelType::Fbx)
        } else if !vrm_list.is_empty() {
            (vrm_list[0].0, PkgModelType::Vrm)
        } else {
            log::error!("unitypackage 内にモデルが見つかりません");
            return;
        };

        // マージ前の材質オフセットを記録
        let mat_offset = self
            .loaded
            .as_ref()
            .map(|l| l.ir.materials.len())
            .unwrap_or(0);

        // 同期的にアペンド
        // sourceがArchiveの場合はsource_overrideとして渡す
        let source_override = match source {
            ReloadableSource::Archive { .. } => Some(source.clone()),
            ReloadableSource::Snapshot { .. } => Some(source.clone()),
            _ => None,
        };
        self.append_from_pkg(assets, model_index, model_type, path, source_override);

        // 追加材質に対するpkgテクスチャ割り当てを復元
        if !saved_pkg_tex_assignments.is_empty() {
            // 割り当て対象を先に収集（借用解放のため）
            let assignments_to_restore: Vec<(usize, String, Vec<u8>)> = {
                let pkg_src = self.tex.pkg_textures.as_deref().unwrap_or(&[]);
                let name_to_data: HashMap<&str, &[u8]> = pkg_src
                    .iter()
                    .map(|(name, data)| (name.as_str(), data.as_slice()))
                    .collect();
                let mat_count = self
                    .loaded
                    .as_ref()
                    .map(|l| l.ir.materials.len())
                    .unwrap_or(0);
                saved_pkg_tex_assignments
                    .iter()
                    .filter(|(idx, _)| **idx >= mat_offset && **idx < mat_count)
                    .filter_map(|(idx, tex_name)| {
                        name_to_data
                            .get(tex_name.as_str())
                            .map(|data| (*idx, tex_name.clone(), data.to_vec()))
                    })
                    .collect()
            };
            for (mat_idx, tex_name, data) in &assignments_to_restore {
                self.assign_texture_data_to_material(*mat_idx, tex_name, data);
                self.tex.pkg_assignments.insert(*mat_idx, tex_name.clone());
            }
        }
    }

    /// 展開済みアセットから指定FBXをロード
    pub fn load_fbx_from_assets(
        &mut self,
        assets: Vec<crate::unitypackage::ExtractedAsset>,
        fbx_index: usize,
        source_path: &std::path::Path,
        mode: FbxLoadMode,
        source_override: Option<ReloadableSource>,
    ) -> anyhow::Result<()> {
        let (fbx_data, fbx_name, textures) =
            crate::unitypackage::take_fbx_and_textures(assets, fbx_index)?;
        log::info!(
            "unitypackage内FBX: {} テクスチャ: {}個",
            fbx_name,
            textures.len()
        );
        self.selected_fbx_name = Some(fbx_name.clone());

        let load_model = matches!(mode, FbxLoadMode::ModelOnly | FbxLoadMode::Both);
        let load_animation = matches!(mode, FbxLoadMode::AnimationOnly | FbxLoadMode::Both);

        if load_model {
            let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                &fbx_data,
                Some(source_path),
                self.normalize_pose,
                self.normalize_to_tstance,
            )?;

            let unmatched = crate::unitypackage::embed_textures_into_ir(&mut ir, &textures);

            // テクスチャをアプリ状態に保持
            if !textures.is_empty() {
                self.tex.pkg_textures = Some(textures);
                self.rebuild_pkg_thumb_cache();
            }

            let source = source_override
                .unwrap_or_else(|| ReloadableSource::File(source_path.to_path_buf()));
            self.finish_load(ir, source)?;

            // モデル読み込み時はアニメーションをクリア
            self.anim.state = None;
            self.anim.library.clear();
            self.anim.active_index = None;

            // 未割当材質がある場合、手動割当ダイアログを開く（リロード中は抑制）
            if !unmatched.is_empty() && self.tex.pkg_textures.is_some() && !self.suppress_tex_match
            {
                let count = unmatched.len();
                self.tex.pending_match = Some(PendingTexMatch {
                    mat_indices: unmatched,
                    selections: vec![None; count],
                    tex_filter: String::new(),
                });
            }
        }

        if load_animation {
            if let Ok(anims) = crate::fbx::animation::load_fbx_animation_from_data(&fbx_data) {
                for anim in anims {
                    let display_name = if anim.name == "animation" {
                        fbx_name.clone()
                    } else {
                        format!("{} ({})", fbx_name, anim.name)
                    };
                    let anim = std::sync::Arc::new(anim);
                    if let Some(ref loaded) = self.loaded {
                        let state = super::animation::AnimationState::new(
                            std::sync::Arc::clone(&anim),
                            &loaded.ir,
                            &loaded.gpu_model,
                        );
                        self.anim
                            .library
                            .push((display_name, source_path.to_path_buf(), anim));
                        self.anim.active_index = Some(self.anim.library.len() - 1);
                        self.anim.state = Some(state);
                    }
                }
            }
        }

        Ok(())
    }

    /// 展開済みアセットから指定VRMをロード
    pub fn load_vrm_from_assets(
        &mut self,
        assets: Vec<crate::unitypackage::ExtractedAsset>,
        vrm_index: usize,
        source_path: &std::path::Path,
        source_override: Option<ReloadableSource>,
    ) -> anyhow::Result<()> {
        let (vrm_data, vrm_name) = crate::unitypackage::take_vrm(assets, vrm_index)?;
        log::info!(
            "unitypackage内VRM: {} ({}KB)",
            vrm_name,
            vrm_data.len() / 1024
        );
        self.selected_fbx_name = Some(vrm_name.clone());

        let glb = vrm::loader::load_glb_from_data(&vrm_data)?;
        let version = vrm::detect::detect_version(&glb.document);
        let all_extensions = vrm::loader::get_raw_extensions(&glb.document);

        let mut ir = vrm::extract::extract_ir_model_with_options(
            &glb.document,
            &glb.buffers,
            &glb.images,
            &glb.vrm_extension,
            &version,
            &all_extensions,
            self.normalize_pose,
        )?;

        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        let gpu_model = super::mesh::build_gpu_model(
            &ir,
            &glb.images,
            device,
            queue,
            self.display.smooth_normals,
            self.display.clear_custom_normals,
        )?;

        Self::encode_ir_textures_as_png(&mut ir, &glb.images);
        let source =
            source_override.unwrap_or_else(|| ReloadableSource::File(source_path.to_path_buf()));
        self.finish_load_with_gpu(ir, gpu_model, source)
    }

    /// 拡張子に基づいてアニメーションファイルを読み込む
    pub fn load_animation_file(&mut self, path: &std::path::Path) {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        match ext.as_str() {
            "glb" | "gltf" => self.try_load_gltf_animation(path),
            "fbx" => self.try_load_fbx_animation(path),
            "anim" => self.try_load_unity_animation(path),
            _ => self.try_load_vrma(path),
        }
    }

    pub fn try_load_vrma(&mut self, path: &std::path::Path) {
        if self.loaded.is_none() {
            self.convert_message = Some(ConvertMessage::failure(
                "VRMAを読み込むには先にVRMモデルを読み込んでください".to_string(),
            ));
            return;
        }

        match vrm::animation::load_vrma(path) {
            Ok(anim) => {
                let anim = Arc::new(anim);
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let loaded = self.loaded.as_ref().expect("loaded は is_some 分岐内");
                let state = AnimationState::new(Arc::clone(&anim), &loaded.ir, &loaded.gpu_model);
                log::info!("VRMA読み込み成功: {}", path.display());

                // ライブラリに追加（重複パスは上書き）
                let path_buf = path.to_path_buf();
                if let Some(idx) = self
                    .anim
                    .library
                    .iter()
                    .position(|(_, p, _)| p == &path_buf)
                {
                    self.anim.library[idx] = (name.clone(), path_buf, anim);
                    self.anim.active_index = Some(idx);
                } else {
                    self.anim.library.push((name.clone(), path_buf, anim));
                    self.anim.active_index = Some(self.anim.library.len() - 1);
                }

                self.anim.state = Some(state);
                self.convert_message = Some(ConvertMessage::success(format!(
                    "VRMA読み込み成功: {}",
                    name
                )));
            }
            Err(e) => {
                log::error!("VRMA読み込み失敗: {e}");
                self.convert_message =
                    Some(ConvertMessage::failure(format!("VRMA読み込み失敗: {e}")));
            }
        }
    }

    /// FBXファイルからアニメーションを読み込む
    pub fn try_load_fbx_animation(&mut self, path: &std::path::Path) {
        if self.loaded.is_none() {
            self.convert_message = Some(ConvertMessage::failure(
                "アニメーションを読み込むには先にモデルを読み込んでください".to_string(),
            ));
            return;
        }

        let anim_result = match self.read_or_preloaded(path) {
            Ok(data) => crate::fbx::animation::load_fbx_animation_from_data(&data),
            Err(_) => crate::fbx::animation::load_fbx_animation(path),
        };
        match anim_result {
            Ok(anims) => {
                let loaded = self.loaded.as_ref().expect("loaded は is_some 分岐内");
                let path_buf = path.to_path_buf();
                let file_name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

                for anim in anims {
                    let display_name = if anim.name == "animation" {
                        file_name.clone()
                    } else {
                        format!("{} ({})", file_name, anim.name)
                    };
                    let anim = Arc::new(anim);
                    let state =
                        AnimationState::new(Arc::clone(&anim), &loaded.ir, &loaded.gpu_model);

                    // ライブラリに追加
                    self.anim
                        .library
                        .push((display_name.clone(), path_buf.clone(), anim));
                    self.anim.active_index = Some(self.anim.library.len() - 1);
                    self.anim.state = Some(state);
                }

                log::info!("FBXアニメーション読み込み成功: {}", path.display());
                self.convert_message = Some(ConvertMessage::success(format!(
                    "FBXアニメーション読み込み成功: {}",
                    file_name
                )));
            }
            Err(e) => {
                log::error!("FBXアニメーション読み込み失敗: {e}");
                self.convert_message = Some(ConvertMessage::failure(format!(
                    "FBXアニメーション読み込み失敗: {e}"
                )));
            }
        }
    }

    /// Unity .animファイルからアニメーションを読み込む
    pub fn try_load_unity_animation(&mut self, path: &std::path::Path) {
        if self.loaded.is_none() {
            self.convert_message = Some(ConvertMessage::failure(
                "アニメーションを読み込むには先にモデルを読み込んでください".to_string(),
            ));
            return;
        }

        match crate::unity::animation::load_unity_anim(path, self.anim.muscle_scale) {
            Ok(anim) => {
                let loaded = self.loaded.as_ref().expect("loaded は is_some 分岐内");
                let path_buf = path.to_path_buf();
                let file_name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let display_name = format!("{} ({})", file_name, anim.name);
                let anim = Arc::new(anim);
                let state = AnimationState::new(Arc::clone(&anim), &loaded.ir, &loaded.gpu_model);

                self.anim.library.push((display_name, path_buf, anim));
                self.anim.active_index = Some(self.anim.library.len() - 1);
                self.anim.state = Some(state);

                log::info!("Unity .anim読み込み成功: {}", path.display());
                self.convert_message = Some(ConvertMessage::success(format!(
                    "Unity .anim読み込み成功: {}",
                    file_name
                )));
            }
            Err(e) => {
                log::error!("Unity .anim読み込み失敗: {e}");
                self.convert_message = Some(ConvertMessage::failure(format!(
                    "Unity .anim読み込み失敗: {e}"
                )));
            }
        }
    }

    /// GLB/glTFファイルからアニメーションを読み込む
    pub fn try_load_gltf_animation(&mut self, path: &std::path::Path) {
        if self.loaded.is_none() {
            self.convert_message = Some(ConvertMessage::failure(
                "アニメーションを読み込むには先にモデルを読み込んでください".to_string(),
            ));
            return;
        }

        match vrm::animation::load_gltf_animation(path) {
            Ok(anims) => {
                let loaded = self.loaded.as_ref().expect("loaded は is_some 分岐内");
                let path_buf = path.to_path_buf();
                let file_name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

                for anim in anims {
                    let display_name = if anim.name == "animation" {
                        file_name.clone()
                    } else {
                        format!("{} ({})", file_name, anim.name)
                    };
                    let anim = Arc::new(anim);
                    let state =
                        AnimationState::new(Arc::clone(&anim), &loaded.ir, &loaded.gpu_model);

                    // ライブラリに追加
                    self.anim
                        .library
                        .push((display_name.clone(), path_buf.clone(), anim));
                    self.anim.active_index = Some(self.anim.library.len() - 1);
                    self.anim.state = Some(state);
                }

                log::info!("glTFアニメーション読み込み成功: {}", path.display());
                self.convert_message = Some(ConvertMessage::success(format!(
                    "アニメーション読み込み成功: {}",
                    file_name
                )));
            }
            Err(e) => {
                log::error!("glTFアニメーション読み込み失敗: {e}");
                self.convert_message = Some(ConvertMessage::failure(format!(
                    "アニメーション読み込み失敗: {e}"
                )));
            }
        }
    }

    /// VRMAライブラリからインデックス指定で切り替え
    pub fn switch_vrma(&mut self, index: usize) {
        if let Some((_, _, ref anim)) = self.anim.library.get(index) {
            if let Some(ref loaded) = self.loaded {
                let state = AnimationState::new(Arc::clone(anim), &loaded.ir, &loaded.gpu_model);
                self.anim.state = Some(state);
                self.anim.active_index = Some(index);
            }
        }
    }

    fn try_load_pmx(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let source = if is_temp_path(path)
            || self.preloaded.as_ref().is_some_and(|pl| pl.path == path)
        {
            let main_data: Arc<[u8]> = self.read_or_preloaded(path)?;
            let pmx_model = crate::pmx::reader::read_pmx_from_data(&main_data)?;
            let pmx_dir = path.parent().unwrap_or(Path::new("."));

            // 補助ファイル（テクスチャ）を収集: preloaded.aux_files を優先
            let mut aux = HashMap::new();
            let preloaded_aux = self
                .preloaded
                .as_ref()
                .filter(|pl| pl.path == path)
                .map(|pl| &pl.aux_files);
            for tex_path in &pmx_model.textures {
                let normalized = tex_path.replace('\\', "/");
                let key = PathBuf::from(&normalized);
                // preloaded aux_files からの取得を優先
                if let Some(data) = preloaded_aux.and_then(|a| a.get(&key)) {
                    aux.insert(key, Arc::clone(data));
                } else {
                    let full_path = pmx_dir.join(&normalized);
                    if let Ok(data) = std::fs::read(&full_path) {
                        aux.insert(key, Arc::from(data.into_boxed_slice()));
                    }
                }
            }

            let mut ir = crate::pmx::extract::pmx_to_ir_with_aux(&pmx_model, pmx_dir, Some(&aux))?;
            if self.normalize_pose {
                ir.astance_result = crate::intermediate::pose::normalize_pose_to_tstance_full(
                    &mut ir.bones,
                    &mut ir.meshes,
                    &mut ir.morphs,
                    &mut ir.physics,
                    crate::convert::coord::gltf_pos_to_pmx,
                );
            }

            let source = ReloadableSource::Snapshot {
                original_path: path.to_path_buf(),
                main_bytes: main_data,
                aux_files: aux,
            };
            return self.finish_load(ir, source);
        } else {
            ReloadableSource::File(path.to_path_buf())
        };

        let pmx_model = crate::pmx::reader::read_pmx(path)?;
        let pmx_dir = path.parent().unwrap_or(Path::new("."));
        let mut ir = crate::pmx::extract::pmx_to_ir(&pmx_model, pmx_dir)?;

        if self.normalize_pose {
            ir.astance_result = crate::intermediate::pose::normalize_pose_to_tstance_full(
                &mut ir.bones,
                &mut ir.meshes,
                &mut ir.morphs,
                &mut ir.physics,
                crate::convert::coord::gltf_pos_to_pmx,
            );
        }

        self.finish_load(ir, source)
    }

    fn try_load_pmd(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let source =
            if is_temp_path(path) || self.preloaded.as_ref().is_some_and(|pl| pl.path == path) {
                let main_data: Arc<[u8]> = self.read_or_preloaded(path)?;
                let pmd_model = crate::pmd::reader::read_pmd_from_data(&main_data)?;
                let pmd_dir = path.parent().unwrap_or(Path::new("."));

                // 補助ファイル（テクスチャ + .txt）を収集: preloaded.aux_files を優先
                let mut aux = HashMap::new();
                let preloaded_aux = self
                    .preloaded
                    .as_ref()
                    .filter(|pl| pl.path == path)
                    .map(|pl| &pl.aux_files);
                // テクスチャ
                for mat in &pmd_model.materials {
                    if mat.texture_name.is_empty() {
                        continue;
                    }
                    let main_tex = mat.texture_name.split('*').next().unwrap_or("");
                    if main_tex.is_empty() {
                        continue;
                    }
                    let normalized = main_tex.replace('\\', "/");
                    let key = PathBuf::from(&normalized);
                    if aux.contains_key(&key) {
                        continue;
                    }
                    // preloaded aux_files からの取得を優先
                    if let Some(data) = preloaded_aux.and_then(|a| a.get(&key)) {
                        aux.insert(key, Arc::clone(data));
                    } else {
                        let full_path = pmd_dir.join(&normalized);
                        if let Ok(data) = std::fs::read(&full_path) {
                            aux.insert(key, Arc::from(data.into_boxed_slice()));
                        }
                    }
                }
                // .txt ファイル
                let txt_path = path.with_extension("txt");
                let txt_name = txt_path
                    .file_name()
                    .map(|f| PathBuf::from(f))
                    .unwrap_or_default();
                if let Some(data) = preloaded_aux.and_then(|a| a.get(&txt_name)) {
                    aux.insert(txt_name, Arc::clone(data));
                } else if let Ok(data) = std::fs::read(&txt_path) {
                    aux.insert(txt_name, Arc::from(data.into_boxed_slice()));
                }

                let mut ir = crate::pmd::extract::pmd_to_ir_with_aux(&pmd_model, path, Some(&aux))?;
                if self.normalize_pose {
                    ir.astance_result = crate::intermediate::pose::normalize_pose_to_tstance_full(
                        &mut ir.bones,
                        &mut ir.meshes,
                        &mut ir.morphs,
                        &mut ir.physics,
                        crate::convert::coord::gltf_pos_to_pmx,
                    );
                }

                let source = ReloadableSource::Snapshot {
                    original_path: path.to_path_buf(),
                    main_bytes: main_data,
                    aux_files: aux,
                };
                return self.finish_load(ir, source);
            } else {
                ReloadableSource::File(path.to_path_buf())
            };

        let pmd_model = crate::pmd::reader::read_pmd(path)?;
        let mut ir = crate::pmd::extract::pmd_to_ir(&pmd_model, path)?;

        if self.normalize_pose {
            ir.astance_result = crate::intermediate::pose::normalize_pose_to_tstance_full(
                &mut ir.bones,
                &mut ir.meshes,
                &mut ir.morphs,
                &mut ir.physics,
                crate::convert::coord::gltf_pos_to_pmx,
            );
        }

        self.finish_load(ir, source)
    }

    fn try_load_vrm(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        // .gltf は外部バッファ参照を持つためスナップショット化しない（.glb/.vrm のみ対象）
        let ext_lower = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let source = if (is_temp_path(path)
            || self.preloaded.as_ref().is_some_and(|pl| pl.path == path))
            && ext_lower != "gltf"
        {
            let data: Arc<[u8]> = self.read_or_preloaded(path)?;
            let glb = vrm::loader::load_glb_from_data(&data)?;
            let version = vrm::detect::detect_version(&glb.document);
            let all_extensions = vrm::loader::get_raw_extensions(&glb.document);

            let mut ir = vrm::extract::extract_ir_model_with_options(
                &glb.document,
                &glb.buffers,
                &glb.images,
                &glb.vrm_extension,
                &version,
                &all_extensions,
                self.normalize_pose,
            )?;

            let device = &self.render_state.device;
            let queue = &self.render_state.queue;
            let gpu_model = super::mesh::build_gpu_model(
                &ir,
                &glb.images,
                device,
                queue,
                self.display.smooth_normals,
                self.display.clear_custom_normals,
            )?;
            Self::encode_ir_textures_as_png(&mut ir, &glb.images);

            let source = ReloadableSource::Snapshot {
                original_path: path.to_path_buf(),
                main_bytes: data,
                aux_files: HashMap::new(),
            };
            return self.finish_load_with_gpu(ir, gpu_model, source);
        } else {
            ReloadableSource::File(path.to_path_buf())
        };

        let glb = vrm::loader::load_glb(path)?;
        let version = vrm::detect::detect_version(&glb.document);
        let all_extensions = vrm::loader::get_raw_extensions(&glb.document);

        let mut ir = vrm::extract::extract_ir_model_with_options(
            &glb.document,
            &glb.buffers,
            &glb.images,
            &glb.vrm_extension,
            &version,
            &all_extensions,
            self.normalize_pose,
        )?;

        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        let gpu_model = super::mesh::build_gpu_model(
            &ir,
            &glb.images,
            device,
            queue,
            self.display.smooth_normals,
            self.display.clear_custom_normals,
        )?;

        // IrTexture を PNG エンコード済みに変換（convert_ir_to_pmx で統一的に使えるように）
        Self::encode_ir_textures_as_png(&mut ir, &glb.images);

        self.finish_load_with_gpu(ir, gpu_model, source)
    }

    fn finish_load(&mut self, ir: IrModel, source: ReloadableSource) -> anyhow::Result<()> {
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;

        // GPU リソース構築（IrTexture から直接アップロード）
        let gpu_model = super::mesh::build_gpu_model_from_ir(
            &ir,
            device,
            queue,
            self.display.smooth_normals,
            self.display.clear_custom_normals,
        )?;
        self.finish_load_with_gpu(ir, gpu_model, source)
    }

    fn finish_load_with_gpu(
        &mut self,
        ir: IrModel,
        mut gpu_model: super::mesh::GpuModel,
        source: ReloadableSource,
    ) -> anyhow::Result<()> {
        // レンダラー初期化（まだなければ）または可視化キャッシュ無効化
        if self.renderer.is_none() {
            let device = &self.render_state.device;
            let queue = &self.render_state.queue;
            self.renderer = Some(GpuRenderer::new(device, queue, gpu_model.has_alpha));
        } else if let Some(ref mut renderer) = self.renderer {
            renderer.invalidate_visualization_cache();
        }

        // MMD リソース構築
        self.prepare_mmd_for_model(&mut gpu_model, &ir);

        // PMX/PMD 単体ロード時は mmd_mode を自動 ON
        let has_mmd_draw = gpu_model
            .draws
            .iter()
            .any(|d| d.render_style == super::mesh::RenderStyle::Mmd);
        if has_mmd_draw {
            self.display.mmd_mode = true;
        } else {
            self.display.mmd_mode = false;
        }

        // テクスチャ割り当て履歴クリア（別モデル読み込み時）
        self.tex.assignments.clear();
        self.tex.pkg_assignments.clear();
        // L3: pending_tex_preview の egui TextureId を正しく解放してから破棄
        if let Some(preview) = self.tex.pending_preview.take() {
            self.cancel_tex_preview_inner(preview);
        }
        // L1: 前モデルの viewport テクスチャIDを解放
        if let Some(tex_id) = self.viewport_texture_id.take() {
            let mut renderer = self.render_state.renderer.write();
            renderer.free_texture(&tex_id);
        }

        // モーフスライダ初期化
        self.morph_weights = vec![0.0; ir.morphs.len()];
        self.morph_dirty = false;
        // 材質表示フラグ初期化（DrawCall数 = 材質数ではない場合があるのでdraws数に合わせる）
        self.material_visibility = vec![true; gpu_model.draws.len()];
        self.export.export_visible_only = false;
        self.material_filter.clear();
        // カメラをモデルのバウンディングボックスにフィット
        let (bbox_min, bbox_max) = gpu_model.bbox();
        self.camera.reset_to_bbox_with_margin(
            bbox_min,
            bbox_max,
            self.last_viewport_width,
            self.last_viewport_height,
        );
        // ビューポートサイズ確定後に refit（初回ロード時はサイズが未確定の場合がある）
        self.pending.refit = true;

        // デフォルト出力パス: 入力VRMと同じ場所に .pmx
        let path = source.display_path();
        self.export.pmx_output_path = path.with_extension("pmx").to_string_lossy().into_owned();

        // キャッシュ構築
        let mat_cache = Self::build_mat_cache(&ir, &gpu_model);
        let stats_cache = CachedStats::new(&ir);

        let format_name = ir.source_format.label().to_string();
        let model_name = ir.name.clone();
        let mat_count = ir.materials.len();
        let draw_count = gpu_model.draws.len();
        let primary_astance_result = ir.astance_result;
        self.loaded = Some(LoadedModel {
            ir,
            gpu_model,
            source,
            primary_astance_result,
            appended_models: Vec::new(),
            material_groups: vec![MaterialGroup {
                name: model_name,
                material_range: 0..mat_count,
                draw_range: 0..draw_count,
            }],
            mat_cache,
            stats_cache,
        });

        // ウィンドウタイトル更新
        self.window_title = Some(format!(
            "Model Viewer v{} - {}",
            env!("CARGO_PKG_VERSION"),
            format_name,
        ));

        if let Some(ref mut renderer) = self.renderer {
            renderer.invalidate_normal_cache();
        }

        Ok(())
    }

    /// smooth_normals 切り替え時に GPU モデルを再構築
    /// MMD リソースを GpuModel に構築
    fn prepare_mmd_for_model(
        &self,
        gpu_model: &mut super::mesh::GpuModel,
        ir: &crate::intermediate::types::IrModel,
    ) {
        if let Some(ref renderer) = self.renderer {
            let device = &self.render_state.device;
            renderer.prepare_mmd_resources(device, gpu_model, ir);
        }
    }

    pub fn rebuild_gpu_model(&mut self) {
        let Some(loaded) = &self.loaded else { return };
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        let smooth = self.display.smooth_normals;
        let clear_normals = self.display.clear_custom_normals;

        match super::mesh::build_gpu_model_from_ir(&loaded.ir, device, queue, smooth, clear_normals)
        {
            Ok(mut new_model) => {
                // MMD リソース構築
                if let Some(ref renderer) = self.renderer {
                    renderer.prepare_mmd_resources(device, &mut new_model, &loaded.ir);
                }
                let mat_cache = Self::build_mat_cache(&loaded.ir, &new_model);
                self.material_visibility = vec![true; new_model.draws.len()];
                if let Some(loaded) = &mut self.loaded {
                    loaded.gpu_model = new_model;
                    loaded.mat_cache = mat_cache;
                }
                if let Some(ref mut renderer) = self.renderer {
                    renderer.invalidate_normal_cache();
                }
                // アニメーション状態を新しい gpu_model で再構築
                if let (Some(ref loaded), Some(ref old_anim)) = (&self.loaded, &self.anim.state) {
                    let mut new_state = AnimationState::new(
                        Arc::clone(&old_anim.animation),
                        &loaded.ir,
                        &loaded.gpu_model,
                    );
                    new_state.playing = old_anim.playing;
                    new_state.loop_mode = old_anim.loop_mode;
                    new_state.speed = old_anim.speed;
                    new_state.current_time = old_anim.current_time;
                    new_state.ab_start = old_anim.ab_start;
                    new_state.ab_end = old_anim.ab_end;
                    new_state.ping_pong_direction = old_anim.ping_pong_direction;
                    self.anim.state = Some(new_state);
                }
                log::info!("GPU モデル再構築完了 (smooth_normals={})", smooth);
            }
            Err(e) => log::error!("GPU モデル再構築失敗: {}", e),
        }
    }

    /// 材質情報キャッシュを構築
    fn build_mat_cache(ir: &IrModel, gpu_model: &GpuModel) -> CachedMaterialInfo {
        let draw_indices: Vec<(usize, usize)> = gpu_model
            .draws
            .iter()
            .enumerate()
            .map(|(i, d)| (i, d.material_index))
            .collect();
        let names: Vec<String> = ir.materials.iter().map(|m| m.name.clone()).collect();
        let tex_indices: Vec<Option<usize>> =
            ir.materials.iter().map(|m| m.texture_index).collect();
        let source_tex_names: Vec<Option<String>> = ir
            .materials
            .iter()
            .map(|m| m.source_texture_name.clone())
            .collect();
        let tex_set_count = ir
            .materials
            .iter()
            .filter(|m| m.texture_index.is_some())
            .count();
        let tex_total = ir.materials.len();
        let tex_status_text = format!("Tex:{}/{}", tex_set_count, tex_total);
        CachedMaterialInfo {
            draw_indices,
            names,
            tex_indices,
            source_tex_names,
            tex_set_count,
            tex_status_text,
        }
    }

    /// 材質キャッシュを更新（テクスチャ割り当て後）
    fn update_mat_cache(&mut self) {
        if let Some(ref mut loaded) = self.loaded {
            loaded.mat_cache = Self::build_mat_cache(&loaded.ir, &loaded.gpu_model);
        }
    }

    /// pkg_textures のサムネイルを GPU にアップロードしてキャッシュ
    pub fn rebuild_pkg_thumb_cache(&mut self) {
        self.clear_pkg_thumb_cache();
        let Some(ref pkg) = self.tex.pkg_textures else {
            return;
        };
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        let mut renderer = self.render_state.renderer.write();
        const THUMB_SIZE: u32 = 64;

        for (name, data) in pkg.iter() {
            let is_psd = super::texture::is_psd_filename(name);
            match super::texture::create_thumbnail_rgba(data, is_psd, THUMB_SIZE) {
                Ok(rgba) => {
                    let (view, _) = super::texture::upload_rgba_to_gpu(
                        device,
                        queue,
                        &rgba,
                        THUMB_SIZE,
                        THUMB_SIZE,
                        Some("pkg_thumb"),
                    );
                    let tex_id = renderer.register_native_texture(
                        device,
                        &view,
                        eframe::wgpu::FilterMode::Linear,
                    );
                    self.tex.pkg_thumb_cache.push(Some(tex_id));
                }
                Err(e) => {
                    log::warn!("サムネイル生成失敗: {} - {}", name, e);
                    self.tex.pkg_thumb_cache.push(None);
                }
            }
        }
    }

    /// サムネイルキャッシュをクリア
    fn clear_pkg_thumb_cache(&mut self) {
        let mut renderer = self.render_state.renderer.write();
        for tex_id in self.tex.pkg_thumb_cache.drain(..).flatten() {
            renderer.free_texture(&tex_id);
        }
    }

    /// 指定材質に外部テクスチャを割り当て（ファイルパスから）
    pub fn assign_texture_to_material(&mut self, material_index: usize, path: &Path) {
        // 一時パスの場合はキャッシュ
        let tex_source = if is_temp_path(path) {
            let tex_data = match std::fs::read(path) {
                Ok(d) => d,
                Err(e) => {
                    log::error!("ファイル読み込み失敗: {e}");
                    self.convert_message = Some(ConvertMessage::failure(format!(
                        "テクスチャ読み込み失敗: {e}"
                    )));
                    return;
                }
            };
            let ext_lower = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            TextureSource::Cached {
                original_name: path.to_string_lossy().into_owned(),
                data: Arc::from(tex_data.into_boxed_slice()),
                is_psd: ext_lower == "psd",
            }
        } else {
            TextureSource::File(path.to_path_buf())
        };
        self.assign_texture_source_to_material(material_index, &tex_source);
    }

    /// 指定材質に TextureSource を割り当て
    pub fn assign_texture_source_to_material(
        &mut self,
        material_index: usize,
        tex_source: &TextureSource,
    ) {
        // テクスチャデータを取得
        let (tex_data, is_psd, display_name) = match tex_source {
            TextureSource::File(path) => {
                let data = match std::fs::read(path) {
                    Ok(d) => d,
                    Err(e) => {
                        log::error!("ファイル読み込み失敗: {e}");
                        self.convert_message = Some(ConvertMessage::failure(format!(
                            "テクスチャ読み込み失敗: {e}"
                        )));
                        return;
                    }
                };
                let ext_lower = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                (
                    data,
                    ext_lower == "psd",
                    path.to_string_lossy().into_owned(),
                )
            }
            TextureSource::Cached {
                original_name,
                data,
                is_psd,
            } => (data.to_vec(), *is_psd, original_name.clone()),
        };

        let ext_lower = if is_psd {
            "psd".to_string()
        } else {
            // display_name から拡張子を取得
            Path::new(&display_name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase()
        };

        let device = &self.render_state.device;
        let queue = &self.render_state.queue;

        // GPU テクスチャをアップロード（読み込み済みバイト列を使用）
        let (texture_view, _texture_view_unorm) =
            match super::texture::upload_texture_from_bytes(&tex_data, is_psd, device, queue) {
                Ok(views) => views,
                Err(e) => {
                    log::error!("テクスチャ読み込み失敗: {e}");
                    self.convert_message = Some(ConvertMessage::failure(format!(
                        "テクスチャ読み込み失敗: {e}"
                    )));
                    return;
                }
            };

        // IrModel にテクスチャを追加・材質を更新
        let Some(ref mut loaded) = self.loaded else {
            return;
        };

        let basename = Path::new(&display_name)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // PSD の場合は PNG に変換して保存
        let (ir_data, ir_filename, ir_mime) = if is_psd {
            match Self::psd_to_png(&tex_data) {
                Ok(png_data) => (
                    png_data,
                    format!("{}.png", basename),
                    "image/png".to_string(),
                ),
                Err(e) => {
                    log::error!("PSD→PNG変換失敗: {e}");
                    self.convert_message =
                        Some(ConvertMessage::failure(format!("PSD→PNG変換失敗: {e}")));
                    return;
                }
            }
        } else {
            let filename = Path::new(&display_name)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let mime = crate::intermediate::types::mime_for_ext(&ext_lower);
            (tex_data, filename, mime.to_string())
        };

        let tex_idx = loaded
            .ir
            .textures
            .iter()
            .position(|t| {
                t.filename == ir_filename && t.data.len() == ir_data.len() && t.data == ir_data
            })
            .unwrap_or_else(|| {
                let idx = loaded.ir.textures.len();
                loaded
                    .ir
                    .textures
                    .push(crate::intermediate::types::IrTexture {
                        filename: ir_filename,
                        data: ir_data,
                        mime_type: ir_mime,
                    });
                idx
            });
        let mat = &mut loaded.ir.materials[material_index];
        mat.texture_index = Some(tex_idx);
        mat.apply_textured_defaults();

        // GPU DrawCall 更新
        let (texture_bgl, sampler) = match self.renderer {
            Some(ref r) => (r.texture_bgl(), r.sampler()),
            None => return,
        };
        loaded.gpu_model.assign_texture_to_material(
            material_index,
            &texture_view,
            device,
            texture_bgl,
            sampler,
        );

        log::info!(
            "テクスチャ割り当て: 材質[{}] '{}' ← {}",
            material_index,
            loaded.ir.materials[material_index].name,
            display_name
        );

        // 割り当て履歴を記録（reload_current 時の復元用）
        self.tex
            .assignments
            .insert(material_index, tex_source.clone());

        // 同一材質名への連動割り当て
        if self.tex.link_same_name {
            let target_name = loaded.ir.materials[material_index].name.clone();
            let siblings: Vec<usize> = loaded
                .ir
                .materials
                .iter()
                .enumerate()
                .filter(|(i, m)| *i != material_index && m.name == target_name)
                .map(|(i, _)| i)
                .collect();
            for sib_idx in siblings {
                loaded.ir.materials[sib_idx].texture_index = Some(tex_idx);
                loaded.ir.materials[sib_idx].apply_textured_defaults();
                loaded.gpu_model.assign_texture_to_material(
                    sib_idx,
                    &texture_view,
                    device,
                    texture_bgl,
                    sampler,
                );
                self.tex.assignments.insert(sib_idx, tex_source.clone());
                log::info!("  連動割り当て: 材質[{}] '{}'", sib_idx, target_name);
            }
        }

        // 材質キャッシュ更新
        self.update_mat_cache();
    }

    /// パッケージ内テクスチャデータを材質に割り当て（バイト列から直接）
    pub fn assign_texture_data_to_material(
        &mut self,
        material_index: usize,
        tex_name: &str,
        tex_data: &[u8],
    ) {
        let is_psd = super::texture::is_psd_filename(tex_name);

        let device = &self.render_state.device;
        let queue = &self.render_state.queue;

        let (texture_view, _texture_view_unorm) =
            match super::texture::upload_texture_from_bytes(tex_data, is_psd, device, queue) {
                Ok(views) => views,
                Err(e) => {
                    log::error!("テクスチャデコード失敗: {e}");
                    self.convert_message = Some(ConvertMessage::failure(format!(
                        "テクスチャデコード失敗: {e}"
                    )));
                    return;
                }
            };

        let Some(ref mut loaded) = self.loaded else {
            return;
        };

        // IrModel にテクスチャを追加
        let basename = std::path::Path::new(tex_name)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let (ir_data, ir_filename, ir_mime) = if is_psd {
            match Self::psd_to_png(tex_data) {
                Ok(png_data) => (
                    png_data,
                    format!("{}.png", basename),
                    "image/png".to_string(),
                ),
                Err(e) => {
                    log::warn!("PSD→PNG変換失敗 (IrTexture用): {e}");
                    (tex_data.to_vec(), tex_name.to_string(), String::new())
                }
            }
        } else {
            (tex_data.to_vec(), tex_name.to_string(), String::new())
        };

        let tex_idx = loaded.ir.textures.len();
        loaded
            .ir
            .textures
            .push(crate::intermediate::types::IrTexture {
                filename: ir_filename,
                data: ir_data,
                mime_type: ir_mime,
            });
        let mat = &mut loaded.ir.materials[material_index];
        mat.texture_index = Some(tex_idx);
        mat.apply_textured_defaults();

        // GPU DrawCall 更新
        let (texture_bgl, sampler) = match self.renderer {
            Some(ref r) => (r.texture_bgl(), r.sampler()),
            None => return,
        };
        loaded.gpu_model.assign_texture_to_material(
            material_index,
            &texture_view,
            device,
            texture_bgl,
            sampler,
        );

        log::info!(
            "パッケージテクスチャ割り当て: 材質[{}] '{}' ← {}",
            material_index,
            loaded.ir.materials[material_index].name,
            tex_name,
        );

        // 同一材質名への連動割り当て
        if self.tex.link_same_name {
            let target_name = loaded.ir.materials[material_index].name.clone();
            let siblings: Vec<usize> = loaded
                .ir
                .materials
                .iter()
                .enumerate()
                .filter(|(i, m)| *i != material_index && m.name == target_name)
                .map(|(i, _)| i)
                .collect();
            for sib_idx in siblings {
                loaded.ir.materials[sib_idx].texture_index = Some(tex_idx);
                loaded.ir.materials[sib_idx].apply_textured_defaults();
                loaded.gpu_model.assign_texture_to_material(
                    sib_idx,
                    &texture_view,
                    device,
                    texture_bgl,
                    sampler,
                );
                log::info!("  連動割り当て: 材質[{}] '{}'", sib_idx, target_name);
            }
        }

        self.update_mat_cache();
    }

    /// VRM の IrTexture（raw ピクセル）を PNG エンコード済みに変換
    fn encode_ir_textures_as_png(ir: &mut IrModel, images: &[gltf::image::Data]) {
        use image::ImageEncoder;
        for (i, tex) in ir.textures.iter_mut().enumerate() {
            if let Some(img_data) = images.get(i) {
                let (w, h) = (img_data.width, img_data.height);
                // RGBA 画像を構築
                let rgba_img: Option<image::RgbaImage> = if tex.data.len() == (w * h * 4) as usize {
                    image::ImageBuffer::from_raw(w, h, tex.data.clone())
                } else if tex.data.len() == (w * h * 3) as usize {
                    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
                    for chunk in tex.data.chunks(3) {
                        rgba.extend_from_slice(chunk);
                        rgba.push(255);
                    }
                    image::ImageBuffer::from_raw(w, h, rgba)
                } else {
                    None
                };
                if let Some(img) = rgba_img {
                    let mut png_data = Vec::new();
                    if image::codecs::png::PngEncoder::new(&mut png_data)
                        .write_image(img.as_raw(), w, h, image::ExtendedColorType::Rgba8)
                        .is_ok()
                    {
                        tex.data = png_data;
                        if !tex.filename.ends_with(".png") {
                            tex.filename = tex
                                .filename
                                .replace(".jpg", ".png")
                                .replace(".jpeg", ".png");
                            if !tex.filename.ends_with(".png") {
                                tex.filename.push_str(".png");
                            }
                        }
                        tex.mime_type = "image/png".to_string();
                    }
                }
            }
        }
    }

    /// PSD データを PNG に変換（decode_psd を共有）
    fn psd_to_png(psd_data: &[u8]) -> anyhow::Result<Vec<u8>> {
        let (rgba, width, height) = super::texture::decode_psd(psd_data)?;

        let mut png_data = Vec::new();
        {
            let encoder = image::codecs::png::PngEncoder::new(&mut png_data);
            use image::ImageEncoder;
            encoder
                .write_image(&rgba, width, height, image::ExtendedColorType::Rgba8)
                .map_err(|e| anyhow::anyhow!("PNG エンコード失敗: {}", e))?;
        }
        Ok(png_data)
    }

    /// 現在読み込み中のVRMを再読み込みする（オプション変更時）
    /// カメラ・モーフ・材質表示などの状態は保持する
    pub fn reload_current(&mut self) {
        let Some(ref loaded) = self.loaded else {
            return;
        };
        // リロード中はテクスチャ選択ダイアログを抑制
        self.suppress_tex_match = true;
        let source = loaded.source.clone();
        let saved_appended = loaded.appended_models.clone();
        let saved_camera = self.camera.clone();
        let saved_morphs = std::mem::take(&mut self.morph_weights);
        let saved_visibility = std::mem::take(&mut self.material_visibility);
        let saved_filter = std::mem::take(&mut self.material_filter);
        let saved_pmx_path = std::mem::take(&mut self.export.pmx_output_path);
        let saved_tex_assignments = std::mem::take(&mut self.tex.assignments);
        let saved_pkg_tex_assignments = std::mem::take(&mut self.tex.pkg_assignments);
        let saved_pkg_textures = self.tex.pkg_textures.take();
        let saved_vrma_library = std::mem::take(&mut self.anim.library);
        let saved_vrma_index = self.anim.active_index.take();

        // unitypackage の場合は再展開せず FBX として再読み込み
        let ext = source.extension_lower();
        let result = match &source {
            ReloadableSource::Archive {
                inner_kind,
                original_path,
                archive_bytes,
                selected_entry_path,
            } if *inner_kind == crate::archive::ArchiveModelKind::UnityPackage => self
                .reload_archive_unitypackage(
                    original_path,
                    archive_bytes.as_ref(),
                    selected_entry_path,
                    &source,
                    &saved_pkg_textures,
                    &saved_pkg_tex_assignments,
                ),
            _ if ext == "unitypackage" => {
                self.reload_unitypackage(&source, &saved_pkg_textures, &saved_pkg_tex_assignments)
            }
            _ => self.reload_from_source(&source),
        };

        // リロード時はテクスチャ選択ダイアログを抑制（後で割り当てを復元するため不要）
        self.tex.pending_match = None;

        // リロード失敗時は状態変更をスキップして早期リターン
        if let Err(e) = result {
            log::error!("再読み込み失敗: {e}");
            self.convert_message = Some(ConvertMessage::failure(format!("再読み込み失敗: {e}")));
            // 退避した状態を復元
            self.camera = saved_camera;
            if let Some(pkg) = saved_pkg_textures {
                self.tex.pkg_textures = Some(pkg);
            }
            self.tex.assignments = saved_tex_assignments;
            self.tex.pkg_assignments = saved_pkg_tex_assignments;
            self.anim.library = saved_vrma_library;
            self.anim.active_index = saved_vrma_index;
            self.suppress_tex_match = false;
            return;
        }

        // 追加モデルを再マージ（ベースモデルが正しく再ロードされた場合のみ）
        // 再ロード成功 = loaded の appended_models が空（新規 LoadedModel が作られた）
        if let Some(ref loaded) = self.loaded {
            if loaded.appended_models.is_empty() && !saved_appended.is_empty() {
                // リロード中フラグON（テクスチャ選択ダイアログ抑制）
                self.suppress_tex_match = true;
                for appended in &saved_appended {
                    match &appended.source {
                        ReloadableSource::Archive { inner_kind, .. }
                            if *inner_kind == crate::archive::ArchiveModelKind::UnityPackage =>
                        {
                            // アーカイブ内 unitypackage は同期的にアペンド
                            self.reload_append_unitypackage(
                                &appended.source,
                                appended.pkg_model_name.as_deref(),
                                &saved_pkg_tex_assignments,
                            );
                        }
                        _ if appended.source.extension_lower() == "unitypackage" => {
                            // 通常の unitypackage は同期的にアペンド（遅延処理を避ける）
                            self.reload_append_unitypackage(
                                &appended.source,
                                appended.pkg_model_name.as_deref(),
                                &saved_pkg_tex_assignments,
                            );
                        }
                        _ => {
                            self.append_model_from_source(
                                &appended.source,
                                appended.pkg_model_name.as_deref(),
                            );
                        }
                    }
                }
                self.suppress_tex_match = false;
                // リロード経由の再アペンドではテクスチャ選択ダイアログを抑制
                self.tex.pending_match = None;
                // アペンド失敗のエラーメッセージは保持、成功メッセージのみクリア
                if let Some(ref msg) = self.convert_message {
                    if matches!(
                        msg.result,
                        ConvertResult::Success(_) | ConvertResult::Warning(_)
                    ) {
                        self.convert_message = None;
                    }
                }
            }
        }

        // pkg_textures を復元
        if self.tex.pkg_textures.is_none() {
            self.tex.pkg_textures = saved_pkg_textures;
        }

        // 状態を復元（モーフ数・材質数が変わらなければそのまま使う）
        self.camera = saved_camera;
        if saved_morphs.len() == self.morph_weights.len() {
            self.morph_weights = saved_morphs;
            self.morph_dirty = true; // 強制更新
        }
        if saved_visibility.len() == self.material_visibility.len() {
            self.material_visibility = saved_visibility;
        }
        self.material_filter = saved_filter;
        self.export.pmx_output_path = saved_pmx_path;

        // テクスチャ割り当てを復元（ファイルパス分のみ。pkg分はreload_unitypackage内で処理済み）
        let saved_link = self.tex.link_same_name;
        self.tex.link_same_name = false;
        self.tex.assignments = HashMap::new();
        let current_mat_count = self
            .loaded
            .as_ref()
            .map(|l| l.ir.materials.len())
            .unwrap_or(0);
        for (mat_idx, tex_src) in &saved_tex_assignments {
            if *mat_idx < current_mat_count {
                self.assign_texture_source_to_material(*mat_idx, tex_src);
            }
        }
        self.tex.link_same_name = saved_link;

        // VRMAライブラリを復元し、アクティブなアニメーションを再構築
        if !saved_vrma_library.is_empty() {
            self.anim.library = saved_vrma_library;
            if let Some(idx) = saved_vrma_index {
                self.switch_vrma(idx);
            }
        }
        // リロード完了: テクスチャ選択ダイアログ抑制を解除
        self.suppress_tex_match = false;
    }

    /// ReloadableSource からモデルを再読み込み（load_file の UI 分岐を回避）
    fn reload_from_source(&mut self, source: &ReloadableSource) -> anyhow::Result<()> {
        let source_clone = source.clone();
        let result: anyhow::Result<()> = (|| {
            match &source_clone {
                ReloadableSource::File(path) => {
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    match ext.as_str() {
                        "fbx" => self.try_load_fbx(path),
                        "pmx" => self.try_load_pmx(path),
                        "pmd" => self.try_load_pmd(path),
                        _ => self.try_load_vrm(path),
                    }
                }
                ReloadableSource::Snapshot {
                    original_path,
                    main_bytes,
                    aux_files,
                } => {
                    let ext = original_path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    match ext.as_str() {
                        "fbx" => {
                            // 外部テクスチャがある場合、一時ディレクトリに復元（サブディレクトリ構造を保持）
                            let temp_dir = if !aux_files.is_empty() {
                                let dir = std::env::temp_dir().join("popone_fbx_reload");
                                let _ = std::fs::create_dir_all(&dir);
                                for (rel_path, data) in aux_files {
                                    let target = dir.join(rel_path);
                                    if let Some(parent) = target.parent() {
                                        let _ = std::fs::create_dir_all(parent);
                                    }
                                    let _ = std::fs::write(&target, data.as_ref());
                                }
                                Some(dir)
                            } else {
                                None
                            };
                            let fbx_path = temp_dir
                                .as_ref()
                                .map(|d| d.join(original_path.file_name().unwrap_or_default()))
                                .unwrap_or_else(|| original_path.clone());
                            let result =
                                crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                                    main_bytes,
                                    Some(&fbx_path),
                                    self.normalize_pose,
                                    self.normalize_to_tstance,
                                );
                            // 一時ファイルをクリーンアップ（成功・失敗問わず）
                            if let Some(dir) = &temp_dir {
                                let _ = std::fs::remove_dir_all(dir);
                            }
                            let ir = result?;
                            self.finish_load(ir, source_clone.clone())
                        }
                        "pmx" => {
                            let pmx_model = crate::pmx::reader::read_pmx_from_data(main_bytes)?;
                            let pmx_dir = original_path.parent().unwrap_or(Path::new("."));
                            let mut ir = crate::pmx::extract::pmx_to_ir_with_aux(
                                &pmx_model,
                                pmx_dir,
                                Some(aux_files),
                            )?;
                            if self.normalize_pose {
                                ir.astance_result =
                                    crate::intermediate::pose::normalize_pose_to_tstance_full(
                                        &mut ir.bones,
                                        &mut ir.meshes,
                                        &mut ir.morphs,
                                        &mut ir.physics,
                                        crate::convert::coord::gltf_pos_to_pmx,
                                    );
                            }
                            self.finish_load(ir, source_clone.clone())
                        }
                        "pmd" => {
                            let pmd_model = crate::pmd::reader::read_pmd_from_data(main_bytes)?;
                            let mut ir = crate::pmd::extract::pmd_to_ir_with_aux(
                                &pmd_model,
                                original_path,
                                Some(aux_files),
                            )?;
                            if self.normalize_pose {
                                ir.astance_result =
                                    crate::intermediate::pose::normalize_pose_to_tstance_full(
                                        &mut ir.bones,
                                        &mut ir.meshes,
                                        &mut ir.morphs,
                                        &mut ir.physics,
                                        crate::convert::coord::gltf_pos_to_pmx,
                                    );
                            }
                            self.finish_load(ir, source_clone.clone())
                        }
                        _ => {
                            // VRM
                            let glb = vrm::loader::load_glb_from_data(main_bytes)?;
                            let version = vrm::detect::detect_version(&glb.document);
                            let all_extensions = vrm::loader::get_raw_extensions(&glb.document);
                            let mut ir = vrm::extract::extract_ir_model_with_options(
                                &glb.document,
                                &glb.buffers,
                                &glb.images,
                                &glb.vrm_extension,
                                &version,
                                &all_extensions,
                                self.normalize_pose,
                            )?;
                            let device = &self.render_state.device;
                            let queue = &self.render_state.queue;
                            let gpu_model = super::mesh::build_gpu_model(
                                &ir,
                                &glb.images,
                                device,
                                queue,
                                self.display.smooth_normals,
                                self.display.clear_custom_normals,
                            )?;
                            Self::encode_ir_textures_as_png(&mut ir, &glb.images);
                            self.finish_load_with_gpu(ir, gpu_model, source_clone.clone())
                        }
                    }
                }
                ReloadableSource::Archive {
                    original_path,
                    archive_bytes,
                    selected_entry_path,
                    inner_kind,
                } => {
                    if *inner_kind == crate::archive::ArchiveModelKind::UnityPackage {
                        // アーカイブ内 .unitypackage: 二重展開してリロード
                        // （reload_current 経由では saved_pkg_textures/assignments が渡されるため、
                        //   ここでは空デフォルトを使用）
                        return self.reload_archive_unitypackage(
                            original_path,
                            archive_bytes.as_ref(),
                            selected_entry_path,
                            &source_clone,
                            &None,
                            &HashMap::new(),
                        );
                    }
                    let ir = self.load_ir_from_archive_source(
                        original_path,
                        archive_bytes.as_ref(),
                        selected_entry_path,
                        *inner_kind,
                    )?;
                    self.finish_load(ir, source_clone.clone())
                }
            }
        })();
        if let Err(ref e) = result {
            log::error!("reload_from_source 失敗: {e}");
            self.convert_message = Some(ConvertMessage::failure(format!("再読み込み失敗: {e}")));
        }
        result
    }

    /// ReloadableSource から追加モデルを読み込み（リロード時用）
    fn append_model_from_source(
        &mut self,
        source: &ReloadableSource,
        pkg_model_name: Option<&str>,
    ) {
        // アーカイブ内 .unitypackage は専用パスで処理
        if let ReloadableSource::Archive { inner_kind, .. } = source {
            if *inner_kind == crate::archive::ArchiveModelKind::UnityPackage {
                self.reload_append_unitypackage(source, pkg_model_name, &HashMap::new());
                return;
            }
        }

        let source_clone = source.clone();
        let ir_result: anyhow::Result<IrModel> = (|| -> anyhow::Result<IrModel> {
            match &source_clone {
                ReloadableSource::File(path) => {
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    match ext.as_str() {
                        "fbx" => crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                            &std::fs::read(path)?,
                            Some(path),
                            self.normalize_pose,
                            self.normalize_to_tstance,
                        ),
                        "pmx" => {
                            let pmx_model = crate::pmx::reader::read_pmx(path)?;
                            let pmx_dir = path.parent().unwrap_or(Path::new("."));
                            let mut ir = crate::pmx::extract::pmx_to_ir(&pmx_model, pmx_dir)?;
                            if self.normalize_pose {
                                ir.astance_result =
                                    crate::intermediate::pose::normalize_pose_to_tstance_full(
                                        &mut ir.bones,
                                        &mut ir.meshes,
                                        &mut ir.morphs,
                                        &mut ir.physics,
                                        crate::convert::coord::gltf_pos_to_pmx,
                                    );
                            }
                            Ok(ir)
                        }
                        "pmd" => {
                            let pmd_model = crate::pmd::reader::read_pmd(path)?;
                            let mut ir = crate::pmd::extract::pmd_to_ir(&pmd_model, path)?;
                            if self.normalize_pose {
                                ir.astance_result =
                                    crate::intermediate::pose::normalize_pose_to_tstance_full(
                                        &mut ir.bones,
                                        &mut ir.meshes,
                                        &mut ir.morphs,
                                        &mut ir.physics,
                                        crate::convert::coord::gltf_pos_to_pmx,
                                    );
                            }
                            Ok(ir)
                        }
                        _ => self.load_vrm_as_ir(path),
                    }
                }
                ReloadableSource::Snapshot {
                    original_path,
                    main_bytes,
                    aux_files,
                } => {
                    let ext = original_path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    match ext.as_str() {
                        "fbx" => {
                            // 外部テクスチャがある場合、一時ディレクトリに復元（サブディレクトリ構造を保持）
                            let temp_dir = if !aux_files.is_empty() {
                                let dir = std::env::temp_dir().join("popone_fbx_reload");
                                let _ = std::fs::create_dir_all(&dir);
                                for (rel_path, data) in aux_files {
                                    let target = dir.join(rel_path);
                                    if let Some(parent) = target.parent() {
                                        let _ = std::fs::create_dir_all(parent);
                                    }
                                    let _ = std::fs::write(&target, data.as_ref());
                                }
                                Some(dir)
                            } else {
                                None
                            };
                            let fbx_path = temp_dir
                                .as_ref()
                                .map(|d| d.join(original_path.file_name().unwrap_or_default()))
                                .unwrap_or_else(|| original_path.clone());
                            let result =
                                crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                                    main_bytes,
                                    Some(&fbx_path),
                                    self.normalize_pose,
                                    self.normalize_to_tstance,
                                );
                            if let Some(dir) = &temp_dir {
                                let _ = std::fs::remove_dir_all(dir);
                            }
                            result
                        }
                        "pmx" => {
                            let pmx_model = crate::pmx::reader::read_pmx_from_data(main_bytes)?;
                            let pmx_dir = original_path.parent().unwrap_or(Path::new("."));
                            let mut ir = crate::pmx::extract::pmx_to_ir_with_aux(
                                &pmx_model,
                                pmx_dir,
                                Some(aux_files),
                            )?;
                            if self.normalize_pose {
                                ir.astance_result =
                                    crate::intermediate::pose::normalize_pose_to_tstance_full(
                                        &mut ir.bones,
                                        &mut ir.meshes,
                                        &mut ir.morphs,
                                        &mut ir.physics,
                                        crate::convert::coord::gltf_pos_to_pmx,
                                    );
                            }
                            Ok(ir)
                        }
                        "pmd" => {
                            let pmd_model = crate::pmd::reader::read_pmd_from_data(main_bytes)?;
                            let mut ir = crate::pmd::extract::pmd_to_ir_with_aux(
                                &pmd_model,
                                original_path,
                                Some(aux_files),
                            )?;
                            if self.normalize_pose {
                                ir.astance_result =
                                    crate::intermediate::pose::normalize_pose_to_tstance_full(
                                        &mut ir.bones,
                                        &mut ir.meshes,
                                        &mut ir.morphs,
                                        &mut ir.physics,
                                        crate::convert::coord::gltf_pos_to_pmx,
                                    );
                            }
                            Ok(ir)
                        }
                        _ => {
                            let glb = vrm::loader::load_glb_from_data(main_bytes)?;
                            let version = vrm::detect::detect_version(&glb.document);
                            let all_extensions = vrm::loader::get_raw_extensions(&glb.document);
                            let mut ir = vrm::extract::extract_ir_model_with_options(
                                &glb.document,
                                &glb.buffers,
                                &glb.images,
                                &glb.vrm_extension,
                                &version,
                                &all_extensions,
                                self.normalize_pose,
                            )?;
                            Self::encode_ir_textures_as_png(&mut ir, &glb.images);
                            Ok(ir)
                        }
                    }
                }
                ReloadableSource::Archive {
                    original_path,
                    archive_bytes,
                    selected_entry_path,
                    inner_kind,
                } => {
                    // UnityPackage は早期に処理済み（ここに到達しない）
                    self.load_ir_from_archive_source(
                        original_path,
                        archive_bytes.as_ref(),
                        selected_entry_path,
                        *inner_kind,
                    )
                }
            }
        })();

        match ir_result {
            Ok(other_ir) => {
                if let Some(ref loaded) = self.loaded {
                    let host_fmt = loaded.ir.source_format;
                    let other_fmt = other_ir.source_format;
                    if host_fmt.is_vrm0() != other_fmt.is_vrm0() {
                        log::warn!(
                            "座標系の異なるモデルの追加: {} + {}",
                            host_fmt.label(),
                            other_fmt.label()
                        );
                        return;
                    }
                }
                self.finish_append_with_source(
                    other_ir,
                    source.clone(),
                    pkg_model_name.map(|s| s.to_string()),
                );
            }
            Err(e) => {
                log::error!("追加モデル再読み込み失敗: {e}");
            }
        }
    }

    /// unitypackage 再読み込み（FBX/VRM再展開 + テクスチャ復元）
    fn reload_unitypackage(
        &mut self,
        source: &ReloadableSource,
        saved_pkg_textures: &Option<Vec<(String, Vec<u8>)>>,
        saved_pkg_tex_assignments: &HashMap<usize, String>,
    ) -> anyhow::Result<()> {
        // Arc 参照で済むケースではコピーを避け、所有権が必要なパスのみ Vec を確保
        use std::borrow::Cow;
        let (archive_data, snapshot): (Cow<'_, [u8]>, Option<Arc<[u8]>>) = match source {
            ReloadableSource::Snapshot { main_bytes, .. } => {
                (Cow::Borrowed(main_bytes), Some(Arc::clone(main_bytes)))
            }
            ReloadableSource::File(path) => {
                let data = std::fs::read(path)?;
                (Cow::Owned(data), None)
            }
            ReloadableSource::Archive {
                original_path,
                archive_bytes,
                ..
            } => {
                if let Some(snap) = archive_bytes {
                    (Cow::Borrowed(snap), Some(Arc::clone(snap)))
                } else {
                    let data = std::fs::read(original_path)?;
                    (Cow::Owned(data), None)
                }
            }
        };
        let path = source.display_path();
        let assets = crate::unitypackage::extract_all_assets(&archive_data)?;

        // 現在のモデルが VRM の場合は VRM として再読み込み
        let is_vrm = self.loaded.as_ref().is_some_and(|l| {
            !matches!(
                l.ir.source_format,
                crate::intermediate::types::SourceFormat::Fbx
            )
        });

        if is_vrm {
            let vrm_list = crate::unitypackage::find_vrm_list(&assets);
            if vrm_list.is_empty() {
                anyhow::bail!(".unitypackage 内に VRM ファイルが見つかりません");
            }
            let vrm_idx = if let Some(ref prev_name) = self.selected_fbx_name {
                vrm_list
                    .iter()
                    .find(|(_, name)| name == prev_name)
                    .map(|(idx, _)| *idx)
                    .unwrap_or(vrm_list[0].0)
            } else {
                vrm_list[0].0
            };
            let source_override = snapshot.map(|snap| ReloadableSource::Snapshot {
                original_path: path.to_path_buf(),
                main_bytes: snap,
                aux_files: HashMap::new(),
            });
            return self.load_vrm_from_assets(assets, vrm_idx, path, source_override);
        }

        let fbx_list = crate::unitypackage::find_fbx_list(&assets);
        if fbx_list.is_empty() {
            anyhow::bail!(".unitypackage 内に FBX ファイルが見つかりません");
        }

        // 前回と同じ FBX を選択（ファイル名で照合、見つからなければ最初のもの）
        let fbx_idx = if let Some(ref prev_name) = self.selected_fbx_name {
            fbx_list
                .iter()
                .find(|(_, name)| name == prev_name)
                .map(|(idx, _)| *idx)
                .unwrap_or(fbx_list[0].0)
        } else {
            fbx_list[0].0
        };

        let (fbx_data, fbx_name, textures) =
            crate::unitypackage::take_fbx_and_textures(assets, fbx_idx)?;
        log::info!(
            "unitypackage再読み込み: {} テクスチャ: {}個",
            fbx_name,
            textures.len()
        );

        let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
            &fbx_data,
            Some(path),
            self.normalize_pose,
            self.normalize_to_tstance,
        )?;

        // テクスチャ埋め込み
        let tex_source = if !textures.is_empty() {
            &textures
        } else if let Some(ref pkg) = saved_pkg_textures {
            pkg.as_slice()
        } else {
            &[]
        };
        crate::unitypackage::embed_textures_into_ir(&mut ir, tex_source);

        // 手動割当の復元（GPU構築前にIrModelに適用）
        let pkg_src = if !textures.is_empty() {
            &textures
        } else {
            saved_pkg_textures.as_deref().unwrap_or(&[])
        };
        if !saved_pkg_tex_assignments.is_empty() && !pkg_src.is_empty() {
            // テクスチャ名 → pkgデータの逆引きマップ
            let name_to_data: HashMap<&str, &[u8]> = pkg_src
                .iter()
                .map(|(name, data)| (name.as_str(), data.as_slice()))
                .collect();
            // 同一テクスチャ名は1回だけIrTextureに追加
            let mut name_to_ir: HashMap<String, usize> = HashMap::new();
            for (mat_idx, tex_name) in saved_pkg_tex_assignments {
                if *mat_idx >= ir.materials.len() {
                    continue;
                }
                let ir_idx = if let Some(&cached) = name_to_ir.get(tex_name) {
                    cached
                } else if let Some(data) = name_to_data.get(tex_name.as_str()) {
                    let is_psd = super::texture::is_psd_filename(tex_name);
                    let basename = std::path::Path::new(tex_name)
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let (ir_data, ir_filename, ir_mime) = if is_psd {
                        match Self::psd_to_png(data) {
                            Ok(png_data) => (
                                png_data,
                                format!("{}.png", basename),
                                "image/png".to_string(),
                            ),
                            Err(e) => {
                                log::warn!("PSD→PNG変換失敗 (pkg復元): {e}");
                                continue;
                            }
                        }
                    } else {
                        let ext = std::path::Path::new(tex_name)
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_lowercase();
                        let mime = crate::intermediate::types::mime_for_ext(&ext).to_string();
                        (data.to_vec(), tex_name.clone(), mime)
                    };
                    let idx = ir.textures.len();
                    ir.textures.push(crate::intermediate::types::IrTexture {
                        filename: ir_filename,
                        data: ir_data,
                        mime_type: ir_mime,
                    });
                    name_to_ir.insert(tex_name.clone(), idx);
                    idx
                } else {
                    continue;
                };
                ir.materials[*mat_idx].texture_index = Some(ir_idx);
                ir.materials[*mat_idx].apply_textured_defaults();
                log::info!(
                    "テクスチャ復元: 材質[{}] '{}' ← '{}'",
                    mat_idx,
                    ir.materials[*mat_idx].name,
                    tex_name
                );
            }
        }

        if !textures.is_empty() {
            self.tex.pkg_textures = Some(textures);
            self.rebuild_pkg_thumb_cache();
        }

        let reload_source = match snapshot {
            Some(snap) => ReloadableSource::Snapshot {
                original_path: path.to_path_buf(),
                main_bytes: snap,
                aux_files: HashMap::new(),
            },
            None => ReloadableSource::File(path.to_path_buf()),
        };
        let result = self.finish_load(ir, reload_source);
        // finish_load がクリアするので、その後に復元
        self.tex.pkg_assignments = saved_pkg_tex_assignments.clone();
        result
    }

    /// アーカイブ(ZIP/7z)内 .unitypackage のリロード
    ///
    /// アーカイブから .unitypackage を展開し、その中のモデルを reload_unitypackage と同様に再読み込みする。
    fn reload_archive_unitypackage(
        &mut self,
        original_path: &Path,
        archive_bytes: Option<&Arc<[u8]>>,
        selected_entry_path: &str,
        archive_source: &ReloadableSource,
        saved_pkg_textures: &Option<Vec<(String, Vec<u8>)>>,
        saved_pkg_tex_assignments: &HashMap<usize, String>,
    ) -> anyhow::Result<()> {
        // アーカイブデータを取得（Arc 参照で済むケースではコピーを避ける）
        let owned;
        let data: &[u8] = if let Some(snap) = archive_bytes {
            snap
        } else {
            owned = std::fs::read(original_path)?;
            &owned
        };

        let ext = original_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let format = crate::archive::archive_format_from_ext(&ext)
            .ok_or_else(|| anyhow::anyhow!("未対応のアーカイブ形式: {ext}"))?;

        let contents = crate::archive::list_models(data, format)?;

        // selected_entry_path で unitypackage エントリを特定
        let model_index = contents
            .models
            .iter()
            .position(|(_, p, _, _)| p.to_string_lossy() == selected_entry_path)
            .ok_or_else(|| {
                anyhow::anyhow!("アーカイブ内に以前のモデルが見つかりません: {selected_entry_path}")
            })?;

        let bundle = crate::archive::extract_model_bundle(data, format, contents, model_index)?;

        // .unitypackage を二重展開
        let pkg_data = bundle.model.data;
        let assets = crate::unitypackage::extract_all_assets(&pkg_data)?;

        // 現在のモデルが VRM の場合は VRM として再読み込み
        let is_vrm = self.loaded.as_ref().is_some_and(|l| {
            !matches!(
                l.ir.source_format,
                crate::intermediate::types::SourceFormat::Fbx
            )
        });

        if is_vrm {
            let vrm_list = crate::unitypackage::find_vrm_list(&assets);
            if vrm_list.is_empty() {
                anyhow::bail!(".unitypackage 内に VRM ファイルが見つかりません");
            }
            let vrm_idx = if let Some(ref prev_name) = self.selected_fbx_name {
                vrm_list
                    .iter()
                    .find(|(_, name)| name == prev_name)
                    .map(|(idx, _)| *idx)
                    .unwrap_or(vrm_list[0].0)
            } else {
                vrm_list[0].0
            };
            return self.load_vrm_from_assets(
                assets,
                vrm_idx,
                original_path,
                Some(archive_source.clone()),
            );
        }

        let fbx_list = crate::unitypackage::find_fbx_list(&assets);
        if fbx_list.is_empty() {
            anyhow::bail!(".unitypackage 内に FBX ファイルが見つかりません");
        }

        let fbx_idx = if let Some(ref prev_name) = self.selected_fbx_name {
            fbx_list
                .iter()
                .find(|(_, name)| name == prev_name)
                .map(|(idx, _)| *idx)
                .unwrap_or(fbx_list[0].0)
        } else {
            fbx_list[0].0
        };

        let (fbx_data, fbx_name, textures) =
            crate::unitypackage::take_fbx_and_textures(assets, fbx_idx)?;
        log::info!(
            "アーカイブ内unitypackage再読み込み: {} テクスチャ: {}個",
            fbx_name,
            textures.len()
        );

        let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
            &fbx_data,
            Some(original_path),
            self.normalize_pose,
            self.normalize_to_tstance,
        )?;

        // テクスチャ埋め込み（reload_unitypackage と同等のフォールバック付き）
        let tex_source = if !textures.is_empty() {
            &textures
        } else if let Some(ref pkg) = saved_pkg_textures {
            pkg.as_slice()
        } else {
            &[]
        };
        crate::unitypackage::embed_textures_into_ir(&mut ir, tex_source);

        // 手動割当の復元（GPU構築前にIrModelに適用）
        let pkg_src = if !textures.is_empty() {
            &textures
        } else {
            saved_pkg_textures.as_deref().unwrap_or(&[])
        };
        if !saved_pkg_tex_assignments.is_empty() && !pkg_src.is_empty() {
            let name_to_data: HashMap<&str, &[u8]> = pkg_src
                .iter()
                .map(|(name, data)| (name.as_str(), data.as_slice()))
                .collect();
            let mut name_to_ir: HashMap<String, usize> = HashMap::new();
            for (mat_idx, tex_name) in saved_pkg_tex_assignments {
                if *mat_idx >= ir.materials.len() {
                    continue;
                }
                let ir_idx = if let Some(&cached) = name_to_ir.get(tex_name) {
                    cached
                } else if let Some(data) = name_to_data.get(tex_name.as_str()) {
                    let is_psd = super::texture::is_psd_filename(tex_name);
                    let basename = std::path::Path::new(tex_name)
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let (ir_data, ir_filename, ir_mime) = if is_psd {
                        match Self::psd_to_png(data) {
                            Ok(png_data) => (
                                png_data,
                                format!("{}.png", basename),
                                "image/png".to_string(),
                            ),
                            Err(e) => {
                                log::warn!("PSD→PNG変換失敗 (pkg復元): {e}");
                                continue;
                            }
                        }
                    } else {
                        let ext = std::path::Path::new(tex_name)
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_lowercase();
                        let mime = crate::intermediate::types::mime_for_ext(&ext).to_string();
                        (data.to_vec(), tex_name.clone(), mime)
                    };
                    let idx = ir.textures.len();
                    ir.textures.push(crate::intermediate::types::IrTexture {
                        filename: ir_filename,
                        data: ir_data,
                        mime_type: ir_mime,
                    });
                    name_to_ir.insert(tex_name.clone(), idx);
                    idx
                } else {
                    continue;
                };
                ir.materials[*mat_idx].texture_index = Some(ir_idx);
                ir.materials[*mat_idx].apply_textured_defaults();
                log::info!(
                    "テクスチャ復元: 材質[{}] '{}' ← '{}'",
                    mat_idx,
                    ir.materials[*mat_idx].name,
                    tex_name
                );
            }
        }

        if !textures.is_empty() {
            self.tex.pkg_textures = Some(textures);
            self.rebuild_pkg_thumb_cache();
        }

        let result = self.finish_load(ir, archive_source.clone());
        // finish_load がクリアするので、その後に復元
        self.tex.pkg_assignments = saved_pkg_tex_assignments.clone();
        result
    }

    /// 1枚のテクスチャをプレビューダイアログで開く
    fn open_texture_preview(&mut self, path: PathBuf) {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let is_psd = ext == "psd";
        // ファイル消失前に一時パス判定を確定（canonicalize がファイル存在を前提とするため）
        let was_temp = is_temp_path(&path);
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                self.convert_message = Some(ConvertMessage::failure(format!(
                    "テクスチャ読み込み失敗: {e}"
                )));
                return;
            }
        };
        match super::texture::upload_texture_from_bytes(
            &data,
            is_psd,
            &self.render_state.device,
            &self.render_state.queue,
        ) {
            Ok((texture_view, _texture_view_unorm)) => {
                let num_mats = self.loaded.as_ref().map_or(0, |l| l.ir.materials.len());
                let preview_tex_id = {
                    let mut renderer = self.render_state.renderer.write();
                    Some(renderer.register_native_texture(
                        &self.render_state.device,
                        &texture_view,
                        wgpu::FilterMode::Linear,
                    ))
                };
                self.tex.pending_preview = Some(PendingTexPreview {
                    path,
                    cached_data: data,
                    is_psd,
                    was_temp,
                    selection: vec![false; num_mats],
                    previewed: vec![false; num_mats],
                    texture_view,
                    saved_binds: HashMap::new(),
                    preview_tex_id,
                });
            }
            Err(e) => {
                self.convert_message = Some(ConvertMessage::failure(format!(
                    "テクスチャ読み込み失敗: {e}"
                )));
            }
        }
    }

    /// 複数テクスチャの自動割り当て（ファイル名と材質名のマッチング）
    fn auto_assign_textures(&mut self, image_files: Vec<PathBuf>) {
        let Some(ref loaded) = self.loaded else {
            return;
        };
        let mat_names: Vec<String> = loaded
            .ir
            .materials
            .iter()
            .map(|m| m.name.to_lowercase())
            .collect();

        let mut assigned = 0usize;
        let mut unmatched: Vec<String> = Vec::new();

        // ファイル名 → マッチする材質インデックスを収集
        let mut assignments: Vec<(PathBuf, Vec<usize>)> = Vec::new();
        for path in &image_files {
            let stem = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            if stem.is_empty() {
                continue;
            }
            // 材質名にファイル名（拡張子なし）を含む材質を検索
            let matched: Vec<usize> = mat_names
                .iter()
                .enumerate()
                .filter(|(_, name)| name.contains(&stem) || stem.contains(name.as_str()))
                .map(|(i, _)| i)
                .collect();
            if matched.is_empty() {
                unmatched.push(
                    path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                );
            } else {
                assignments.push((path.clone(), matched));
            }
        }

        // 割り当て実行
        for (path, mat_indices) in assignments {
            for &mat_idx in &mat_indices {
                self.assign_texture_to_material(mat_idx, &path);
                assigned += 1;
            }
        }

        // 結果メッセージ
        let mut msg = format!("テクスチャ自動割り当て: {}材質に適用", assigned);
        if !unmatched.is_empty() {
            msg += &format!("\nマッチなし: {}", unmatched.join(", "));
        }
        if assigned > 0 {
            self.convert_message = Some(ConvertMessage::success(msg));
        } else {
            self.convert_message = Some(ConvertMessage::failure(format!(
                "マッチする材質が見つかりませんでした\nファイル: {}",
                unmatched.join(", ")
            )));
        }
    }

    fn open_file_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .set_title("3Dモデル / VRMAアニメーションを開く")
            .add_filter(
                "対応形式",
                &[
                    "vrm",
                    "fbx",
                    "pmx",
                    "pmd",
                    "unitypackage",
                    "vrma",
                    "zip",
                    "7z",
                ],
            )
            .add_filter("VRM (.vrm)", &["vrm"])
            .add_filter("FBX (.fbx)", &["fbx"])
            .add_filter("PMX (.pmx)", &["pmx"])
            .add_filter("PMD (.pmd)", &["pmd"])
            .add_filter("UnityPackage (.unitypackage)", &["unitypackage"])
            .add_filter("アーカイブ (.zip, .7z)", &["zip", "7z"])
            .add_filter("VRMA (.vrma)", &["vrma"]);
        if let Some(ref dir) = self.last_model_dir {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.pick_file() {
            if let Some(dir) = path.parent() {
                self.last_model_dir = Some(dir.to_path_buf());
            }
            self.pending.load = Some((path, false));
        }
    }

    /// モデル追加読み込みダイアログ
    fn open_append_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .set_title("モデルを追加読み込み")
            .add_filter(
                "3Dモデル",
                &["vrm", "fbx", "pmx", "pmd", "unitypackage", "zip", "7z"],
            )
            .add_filter("VRM (.vrm)", &["vrm"])
            .add_filter("FBX (.fbx)", &["fbx"])
            .add_filter("PMX (.pmx)", &["pmx"])
            .add_filter("PMD (.pmd)", &["pmd"])
            .add_filter("UnityPackage (.unitypackage)", &["unitypackage"])
            .add_filter("アーカイブ (.zip, .7z)", &["zip", "7z"]);
        if let Some(ref dir) = self.last_model_dir {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.pick_file() {
            if let Some(dir) = path.parent() {
                self.last_model_dir = Some(dir.to_path_buf());
            }
            self.pending.append = Some((path, false));
        }
    }

    /// モデルを既存モデルに追加（マージ）
    fn append_model(&mut self, path: PathBuf) {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        // unitypackage / アーカイブは専用フローで処理（モデル選択が必要なため）
        if ext == "unitypackage" {
            match self.try_load_unitypackage_for_append(&path) {
                Ok(()) => {}
                Err(e) => {
                    log::error!("追加読み込み失敗(pkg): {e}");
                    self.convert_message = Some(ConvertMessage::failure(format!(
                        "追加読み込みに失敗しました。\n詳細: {e}"
                    )));
                }
            }
            return;
        }
        if matches!(ext.as_str(), "zip" | "7z") {
            match self.try_load_archive_for_append(&path) {
                Ok(()) => {}
                Err(e) => {
                    log::error!("追加読み込み失敗(archive): {e}");
                    self.convert_message = Some(ConvertMessage::failure(format!(
                        "追加読み込みに失敗しました。\n詳細: {e}"
                    )));
                }
            }
            return;
        }

        // 追加モデルの IrModel を構築
        let ir_result: anyhow::Result<IrModel> = (|| -> anyhow::Result<IrModel> {
            match ext.as_str() {
                "fbx" => {
                    let data = self.read_or_preloaded(&path)?;
                    crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                        &data,
                        Some(&path),
                        self.normalize_pose,
                        self.normalize_to_tstance,
                    )
                }
                "pmx" => {
                    let pmx_model = if self.preloaded.as_ref().is_some_and(|pl| pl.path == path) {
                        let data = self.read_or_preloaded(&path)?;
                        crate::pmx::reader::read_pmx_from_data(&data)?
                    } else {
                        crate::pmx::reader::read_pmx(&path)?
                    };
                    let pmx_dir = path.parent().unwrap_or(std::path::Path::new("."));
                    let mut ir = crate::pmx::extract::pmx_to_ir(&pmx_model, pmx_dir)?;
                    if self.normalize_pose {
                        ir.astance_result =
                            crate::intermediate::pose::normalize_pose_to_tstance_full(
                                &mut ir.bones,
                                &mut ir.meshes,
                                &mut ir.morphs,
                                &mut ir.physics,
                                crate::convert::coord::gltf_pos_to_pmx,
                            );
                    }
                    Ok(ir)
                }
                "pmd" => {
                    let pmd_model = if self.preloaded.as_ref().is_some_and(|pl| pl.path == path) {
                        let data = self.read_or_preloaded(&path)?;
                        crate::pmd::reader::read_pmd_from_data(&data)?
                    } else {
                        crate::pmd::reader::read_pmd(&path)?
                    };
                    let mut ir = crate::pmd::extract::pmd_to_ir(&pmd_model, &path)?;
                    if self.normalize_pose {
                        ir.astance_result =
                            crate::intermediate::pose::normalize_pose_to_tstance_full(
                                &mut ir.bones,
                                &mut ir.meshes,
                                &mut ir.morphs,
                                &mut ir.physics,
                                crate::convert::coord::gltf_pos_to_pmx,
                            );
                    }
                    Ok(ir)
                }
                _ => {
                    // VRM
                    self.load_vrm_as_ir(&path)
                }
            }
        })();

        match ir_result {
            Ok(other_ir) => {
                // 座標系の互換性チェック
                if let Some(ref loaded) = self.loaded {
                    let host_fmt = loaded.ir.source_format;
                    let other_fmt = other_ir.source_format;
                    // VRM 0.0 は座標変換が異なるため、異種混在を警告
                    if host_fmt.is_vrm0() != other_fmt.is_vrm0() {
                        log::warn!(
                            "座標系の異なるモデルの追加: {} + {}",
                            host_fmt.label(),
                            other_fmt.label()
                        );
                        self.convert_message = Some(ConvertMessage::failure(format!(
                            "座標系が異なるモデルは追加できません。\nホスト: {}, 追加: {}",
                            host_fmt.label(),
                            other_fmt.label()
                        )));
                        return;
                    }
                }
                // 一時パスならスナップショット構築（読み込み失敗時は File にフォールバック）
                let source = if is_temp_path(&path) {
                    let main_data = match std::fs::read(&path) {
                        Ok(d) => d,
                        Err(_) => {
                            // ファイルが既に消えている場合、Fileで記録（リロード時は失敗する）
                            log::warn!("一時ファイル再読み込み失敗: {}", path.display());
                            self.finish_append_with_source(
                                other_ir,
                                ReloadableSource::File(path.clone()),
                                None,
                            );
                            return;
                        }
                    };
                    let mut aux = HashMap::new();
                    if ext == "fbx" {
                        // FBX 外部テクスチャ: サブディレクトリ含め再帰収集
                        if let Some(dir) = path.parent() {
                            collect_image_files_recursive(dir, dir, &mut aux);
                        }
                    } else if ext == "pmx" {
                        if let Ok(pmx_model) = crate::pmx::reader::read_pmx_from_data(&main_data) {
                            let pmx_dir = path.parent().unwrap_or(Path::new("."));
                            for tex_path in &pmx_model.textures {
                                let normalized = tex_path.replace('\\', "/");
                                let full = pmx_dir.join(&normalized);
                                if let Ok(data) = std::fs::read(&full) {
                                    aux.insert(
                                        PathBuf::from(&normalized),
                                        Arc::from(data.into_boxed_slice()),
                                    );
                                }
                            }
                        }
                    } else if ext == "pmd" {
                        let pmd_dir = path.parent().unwrap_or(Path::new("."));
                        // テクスチャ
                        if let Ok(pmd_model) = crate::pmd::reader::read_pmd_from_data(&main_data) {
                            for mat in &pmd_model.materials {
                                if mat.texture_name.is_empty() {
                                    continue;
                                }
                                let main_tex = mat.texture_name.split('*').next().unwrap_or("");
                                if main_tex.is_empty() {
                                    continue;
                                }
                                let normalized = main_tex.replace('\\', "/");
                                let key = PathBuf::from(&normalized);
                                if !aux.contains_key(&key) {
                                    if let Ok(data) = std::fs::read(pmd_dir.join(&normalized)) {
                                        aux.insert(key, Arc::from(data.into_boxed_slice()));
                                    }
                                }
                            }
                        }
                        // .txt
                        let txt_path = path.with_extension("txt");
                        if let Ok(data) = std::fs::read(&txt_path) {
                            let txt_name = txt_path
                                .file_name()
                                .map(|f| PathBuf::from(f))
                                .unwrap_or_default();
                            aux.insert(txt_name, Arc::from(data.into_boxed_slice()));
                        }
                    }
                    ReloadableSource::Snapshot {
                        original_path: path.clone(),
                        main_bytes: main_data.into(),
                        aux_files: aux,
                    }
                } else {
                    ReloadableSource::File(path.clone())
                };
                self.finish_append_with_source(other_ir, source, None);
            }
            Err(e) => {
                log::error!("追加読み込み失敗: {e}");
                self.convert_message = Some(ConvertMessage::failure(format!(
                    "追加読み込みに失敗しました。\n詳細: {e}"
                )));
            }
        }
    }

    /// VRMファイルを IrModel として読み込む（追加用・GPU構築なし）
    fn load_vrm_as_ir(&mut self, path: &std::path::Path) -> anyhow::Result<IrModel> {
        let glb = if self.preloaded.as_ref().is_some_and(|pl| pl.path == path) {
            let data = self.read_or_preloaded(path)?;
            vrm::loader::load_glb_from_data(&data)?
        } else {
            vrm::loader::load_glb(path)?
        };
        let version = vrm::detect::detect_version(&glb.document);
        let all_extensions = vrm::loader::get_raw_extensions(&glb.document);

        let mut ir = vrm::extract::extract_ir_model_with_options(
            &glb.document,
            &glb.buffers,
            &glb.images,
            &glb.vrm_extension,
            &version,
            &all_extensions,
            self.normalize_pose,
        )?;

        // IrTexture を PNG エンコード済みに変換
        Self::encode_ir_textures_as_png(&mut ir, &glb.images);
        Ok(ir)
    }

    /// unitypackage 内のモデルを既存モデルに追加（アペンド）
    fn append_from_pkg(
        &mut self,
        assets: Vec<crate::unitypackage::ExtractedAsset>,
        model_index: usize,
        model_type: PkgModelType,
        source_path: &std::path::Path,
        source_override: Option<ReloadableSource>,
    ) {
        let normalize = self.normalize_pose;
        let normalize_tstance = self.normalize_to_tstance;
        // 未マッチ材質（マージ前のローカルIndex）
        let mut pkg_unmatched: Vec<usize> = Vec::new();
        // pkg内モデル名（リロード時の照合用）
        let mut pkg_model_name: Option<String> = None;
        // GPU構築成功後に蓄積するpkgテクスチャ
        let mut pkg_textures_to_add: Vec<(String, Vec<u8>)> = Vec::new();
        let ir_result: anyhow::Result<IrModel> = (|| -> anyhow::Result<IrModel> {
            match model_type {
                PkgModelType::Fbx => {
                    let (fbx_data, fbx_name, textures) =
                        crate::unitypackage::take_fbx_and_textures(assets, model_index)?;
                    log::info!(
                        "追加(pkg内FBX): {} テクスチャ: {}個",
                        fbx_name,
                        textures.len()
                    );
                    pkg_model_name = Some(fbx_name.clone());
                    let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                        &fbx_data,
                        Some(source_path),
                        normalize,
                        normalize_tstance,
                    )?;
                    // pkg内テクスチャを IrModel に埋め込み
                    let unmatched = crate::unitypackage::embed_textures_into_ir(&mut ir, &textures);
                    log::info!(
                        "追加(pkg): {}材質マッチ, 未割当: {}",
                        ir.materials.len() - unmatched.len(),
                        unmatched.len()
                    );
                    pkg_unmatched = unmatched;
                    // テクスチャは成功後に蓄積するため、ここでは保持のみ
                    pkg_textures_to_add = textures;
                    Ok(ir)
                }
                PkgModelType::Vrm => {
                    let (vrm_data, vrm_name) = crate::unitypackage::take_vrm(assets, model_index)?;
                    log::info!("追加(pkg内VRM): {}", vrm_name);
                    pkg_model_name = Some(vrm_name.clone());
                    let glb = vrm::loader::load_glb_from_data(&vrm_data)?;
                    let version = vrm::detect::detect_version(&glb.document);
                    let all_extensions = vrm::loader::get_raw_extensions(&glb.document);
                    let mut ir = vrm::extract::extract_ir_model_with_options(
                        &glb.document,
                        &glb.buffers,
                        &glb.images,
                        &glb.vrm_extension,
                        &version,
                        &all_extensions,
                        normalize,
                    )?;
                    Self::encode_ir_textures_as_png(&mut ir, &glb.images);
                    Ok(ir)
                }
            }
        })();

        match ir_result {
            Ok(other_ir) => {
                // マージ前の材質数を記録（未マッチIndexのオフセット用）
                let mat_offset = self
                    .loaded
                    .as_ref()
                    .map(|l| l.ir.materials.len())
                    .unwrap_or(0);
                let appended_before = self
                    .loaded
                    .as_ref()
                    .map(|l| l.appended_models.len())
                    .unwrap_or(0);
                let tex_count_before = self
                    .loaded
                    .as_ref()
                    .map(|l| l.ir.textures.len())
                    .unwrap_or(0);
                match source_override {
                    Some(source) => {
                        self.finish_append_with_source(other_ir, source, pkg_model_name)
                    }
                    None => self.finish_append_with_pkg_name(other_ir, source_path, pkg_model_name),
                }
                let appended_after = self
                    .loaded
                    .as_ref()
                    .map(|l| l.appended_models.len())
                    .unwrap_or(0);
                // アペンド成功後のみpkgテクスチャを蓄積（ロールバック時はスキップ）
                if appended_after > appended_before {
                    // 複数パッケージ間のテクスチャ名衝突を防止するためプレフィックス付与
                    // ファイル名 + append連番で一意化（同名packageの別ディレクトリ追加にも対応）
                    // パスセパレータを含まないよう `_` で結合（PMX export で IrTexture.filename に使われるため）
                    let pkg_stem = source_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("pkg");
                    let pkg_prefix = format!("{}_pkg{}", pkg_stem, appended_after);

                    // auto-matched テクスチャ（embed_textures_into_ir 経由で IrModel に入ったもの）にもプレフィックス付与
                    if let Some(ref mut loaded) = self.loaded {
                        for tex in loaded.ir.textures[tex_count_before..].iter_mut() {
                            tex.filename = format!("{}_{}", pkg_prefix, tex.filename);
                        }
                    }

                    // 手動割当用の pkg_textures にもプレフィックス付与
                    if !pkg_textures_to_add.is_empty() {
                        for (name, _) in &mut pkg_textures_to_add {
                            *name = format!("{}_{}", pkg_prefix, name);
                        }
                        if let Some(ref mut existing) = self.tex.pkg_textures {
                            existing.extend(pkg_textures_to_add);
                        } else {
                            self.tex.pkg_textures = Some(pkg_textures_to_add);
                        }
                        self.rebuild_pkg_thumb_cache();
                    }
                }
                // 未割当材質がある場合、手動割当ダイアログを開く（リロード中は抑制）
                if !pkg_unmatched.is_empty()
                    && self.tex.pkg_textures.is_some()
                    && !self.suppress_tex_match
                {
                    // ローカルIndexにマージ後の材質オフセットを加算
                    let global_unmatched: Vec<usize> =
                        pkg_unmatched.iter().map(|&i| i + mat_offset).collect();
                    let count = global_unmatched.len();
                    self.tex.pending_match = Some(PendingTexMatch {
                        mat_indices: global_unmatched,
                        selections: vec![None; count],
                        tex_filter: String::new(),
                    });
                }
            }
            Err(e) => {
                log::error!("追加読み込み失敗(pkg): {e}");
                self.convert_message = Some(ConvertMessage::failure(format!(
                    "追加読み込みに失敗しました。\n詳細: {e}"
                )));
            }
        }
    }

    /// 追加モデルの IrModel を既存モデルにマージしてGPU再構築
    #[allow(dead_code)]
    fn finish_append(&mut self, other_ir: crate::intermediate::types::IrModel, path: &Path) {
        self.finish_append_ext(
            other_ir,
            ReloadableSource::File(path.to_path_buf()),
            false,
            None,
        );
    }

    fn finish_append_with_pkg_name(
        &mut self,
        other_ir: crate::intermediate::types::IrModel,
        path: &Path,
        pkg_model_name: Option<String>,
    ) {
        self.finish_append_ext(
            other_ir,
            ReloadableSource::File(path.to_path_buf()),
            false,
            pkg_model_name,
        );
    }

    fn finish_append_with_source(
        &mut self,
        other_ir: crate::intermediate::types::IrModel,
        source: ReloadableSource,
        pkg_model_name: Option<String>,
    ) {
        self.finish_append_ext(other_ir, source, false, pkg_model_name);
    }

    fn finish_append_ext(
        &mut self,
        other_ir: crate::intermediate::types::IrModel,
        source: ReloadableSource,
        silent: bool,
        pkg_model_name: Option<String>,
    ) {
        let Some(ref mut loaded) = self.loaded else {
            return;
        };

        let added_name = other_ir.name.clone();
        let added_bones = other_ir.bones.len();
        let added_meshes = other_ir.meshes.len();
        let added_materials = other_ir.materials.len();

        // ロールバック用: マージ前の状態を退避
        let saved_bone_count = loaded.ir.bones.len();
        let saved_mesh_count = loaded.ir.meshes.len();
        let saved_material_count = loaded.ir.materials.len();
        let saved_texture_count = loaded.ir.textures.len();
        let saved_morph_count = loaded.ir.morphs.len();
        let saved_rigid_count = loaded.ir.physics.rigid_bodies.len();
        let saved_joint_count = loaded.ir.physics.joints.len();
        let saved_name = loaded.ir.name.clone();
        let saved_node_to_bone = loaded.ir.node_to_bone.clone();
        let saved_humanoid_count = loaded.ir.humanoid_bone_count;
        // 既存ボーンの children と vrm_bone_name を退避（merge で変更されるため）
        let saved_bone_meta: Vec<(Vec<usize>, Option<String>)> = loaded
            .ir
            .bones
            .iter()
            .map(|b| (b.children.clone(), b.vrm_bone_name.clone()))
            .collect();

        // IrModel をマージ（同名ボーン統合）
        let (merged_bones, new_bones) = loaded.ir.merge(other_ir);

        // GPU モデル再構築
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        match super::mesh::build_gpu_model_from_ir(
            &loaded.ir,
            device,
            queue,
            self.display.smooth_normals,
            self.display.clear_custom_normals,
        ) {
            Ok(mut gpu_model) => {
                // MMD リソース構築
                if let Some(ref renderer) = self.renderer {
                    renderer.prepare_mmd_resources(device, &mut gpu_model, &loaded.ir);
                }
                // ビューポートテクスチャ解放
                if let Some(tex_id) = self.viewport_texture_id.take() {
                    let mut renderer = self.render_state.renderer.write();
                    renderer.free_texture(&tex_id);
                }
                // 材質表示フラグ更新（既存分を保持、追加分はtrue）
                let new_draw_count = gpu_model.draws.len();
                self.material_visibility.resize(new_draw_count, true);
                // モーフスライダ更新（既存分を保持、追加分は0.0）
                let new_morph_count = loaded.ir.morphs.len();
                self.morph_weights.resize(new_morph_count, 0.0);
                self.morph_dirty = self.morph_weights.iter().any(|&w| w != 0.0);
                // キャッシュ再構築
                loaded.mat_cache = Self::build_mat_cache(&loaded.ir, &gpu_model);
                loaded.stats_cache = CachedStats::new(&loaded.ir);
                // 材質グループを更新（追加モデル分を記録）
                let prev_draw_end: usize = loaded
                    .material_groups
                    .iter()
                    .map(|g| g.draw_range.end)
                    .max()
                    .unwrap_or(0);
                loaded.material_groups.push(MaterialGroup {
                    name: added_name.clone(),
                    material_range: saved_material_count..saved_material_count + added_materials,
                    draw_range: prev_draw_end..gpu_model.draws.len(),
                });
                loaded.gpu_model = gpu_model;
                // 追加ソースを記録（リロード時に再マージ用）
                let display_path = source.display_path().to_path_buf();
                loaded.appended_models.push(AppendedModel {
                    source,
                    pkg_model_name: pkg_model_name.clone(),
                });
                // テクスチャダイアログの初期ディレクトリを追加モデルのディレクトリに設定
                if let Some(dir) = display_path.parent() {
                    self.tex.last_dir = Some(dir.to_path_buf());
                }
                // 可視化キャッシュ無効化
                if let Some(ref mut renderer) = self.renderer {
                    renderer.invalidate_visualization_cache();
                    renderer.invalidate_normal_cache();
                }
                // アニメーション状態を再構築（ボーン/メッシュ構成が変わったため）
                if let (Some(ref loaded), Some(ref old_anim)) = (&self.loaded, &self.anim.state) {
                    let mut new_state = AnimationState::new(
                        Arc::clone(&old_anim.animation),
                        &loaded.ir,
                        &loaded.gpu_model,
                    );
                    new_state.playing = old_anim.playing;
                    new_state.loop_mode = old_anim.loop_mode;
                    new_state.speed = old_anim.speed;
                    new_state.current_time = old_anim.current_time;
                    new_state.ab_start = old_anim.ab_start;
                    new_state.ab_end = old_anim.ab_end;
                    new_state.ping_pong_direction = old_anim.ping_pong_direction;
                    self.anim.state = Some(new_state);
                }
                log::info!(
                    "追加読み込み成功: {} (ボーン:{} → 統合:{}/新規:{}, メッシュ:{}, 材質:{})",
                    added_name,
                    added_bones,
                    merged_bones,
                    new_bones,
                    added_meshes,
                    added_materials,
                );
                if !silent {
                    self.convert_message = Some(ConvertMessage::success(format!(
                        "追加読み込み完了: {}\nボーン:{} (統合:{} + 新規:{}), メッシュ:{}, 材質:{}",
                        added_name,
                        added_bones,
                        merged_bones,
                        new_bones,
                        added_meshes,
                        added_materials,
                    )));
                }
            }
            Err(e) => {
                log::error!("GPU再構築失敗（マージをロールバック）: {e}");
                // IR をマージ前の状態にロールバック
                loaded.ir.bones.truncate(saved_bone_count);
                loaded.ir.meshes.truncate(saved_mesh_count);
                loaded.ir.materials.truncate(saved_material_count);
                loaded.ir.textures.truncate(saved_texture_count);
                loaded.ir.morphs.truncate(saved_morph_count);
                loaded.ir.physics.rigid_bodies.truncate(saved_rigid_count);
                loaded.ir.physics.joints.truncate(saved_joint_count);
                loaded.ir.name = saved_name;
                loaded.ir.node_to_bone = saved_node_to_bone;
                loaded.ir.humanoid_bone_count = saved_humanoid_count;
                // 既存ボーンの children と vrm_bone_name を退避した状態に完全復元
                for (i, bone) in loaded.ir.bones.iter_mut().enumerate() {
                    if i < saved_bone_meta.len() {
                        bone.children = saved_bone_meta[i].0.clone();
                        bone.vrm_bone_name = saved_bone_meta[i].1.clone();
                    }
                }
                self.convert_message = Some(ConvertMessage::failure(format!(
                    "追加読み込み後のGPU再構築に失敗しました。\n詳細: {e}"
                )));
            }
        }
    }

    /// プログレスオーバーレイ描画（ビューポート上、結果メッセージと同じスタイル）
    fn paint_progress_overlay(&self, viewport: &egui::Ui, rect: egui::Rect, ctx: &egui::Context) {
        let msg = if self.pending.load.is_some()
            || self.pending.append.is_some()
            || self.pending.pkg_load.is_some()
            || self.pending.archive_load.is_some()
        {
            Some("読み込み中...")
        } else if self.pending.rebuild.is_some() || self.pending.reload.is_some() {
            Some("処理中...")
        } else if self.pending.convert.is_some() {
            Some("PMX変換中...")
        } else {
            None
        };
        let Some(msg) = msg else { return };

        let color = egui::Color32::from_rgb(0x60, 0xA0, 0xFF);
        let bar_h = 36.0_f32;
        let center = rect.center();
        let bar_rect = egui::Rect::from_center_size(
            egui::pos2(center.x, center.y),
            egui::vec2(rect.width(), bar_h),
        );
        // 背景帯
        viewport.painter().rect_filled(
            bar_rect,
            0.0,
            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 0xC0),
        );
        // テキスト（中央揃え）
        viewport.painter().text(
            center,
            egui::Align2::CENTER_CENTER,
            msg,
            egui::FontId::proportional(16.0),
            color,
        );
        ctx.request_repaint();
    }

    /// プログレスフラグ更新（次フレームで処理を実行するためのトリガー）
    fn update_progress_flags(&mut self, ctx: &egui::Context) {
        if let Some((_, ref mut shown)) = self.pending.load {
            if !*shown {
                *shown = true;
                ctx.request_repaint();
            }
        }
        if let Some((_, ref mut shown)) = self.pending.append {
            if !*shown {
                *shown = true;
                ctx.request_repaint();
            }
        }
        if let Some(ref mut p) = self.pending.pkg_load {
            if !p.shown {
                p.shown = true;
                ctx.request_repaint();
            }
        }
        if let Some(ref mut p) = self.pending.archive_load {
            if !p.shown {
                p.shown = true;
                ctx.request_repaint();
            }
        }
        if self.pending.rebuild == Some(PendingOverlay::WaitingOverlay) {
            self.pending.rebuild = Some(PendingOverlay::Ready);
            ctx.request_repaint();
        }
        if self.pending.reload == Some(PendingOverlay::WaitingOverlay) {
            self.pending.reload = Some(PendingOverlay::Ready);
            ctx.request_repaint();
        }
        if self.pending.convert == Some(PendingOverlay::WaitingOverlay) {
            self.pending.convert = Some(PendingOverlay::Ready);
            ctx.request_repaint();
        }
    }

    /// テクスチャプレビューの同期（selection と previewed の差分を GPU に反映）
    pub fn sync_tex_preview(&mut self) {
        let Some(ref mut preview) = self.tex.pending_preview else {
            return;
        };
        let Some(ref mut loaded) = self.loaded else {
            return;
        };
        let Some(ref renderer) = self.renderer else {
            return;
        };
        let device = &self.render_state.device;
        let texture_bgl = renderer.texture_bgl();
        let sampler = renderer.sampler();

        for mat_idx in 0..preview.selection.len() {
            if preview.selection[mat_idx] && !preview.previewed[mat_idx] {
                // プレビュー適用: 元の bind group を退避し、プレビュー用に差し替え
                for (draw_idx, draw) in loaded.gpu_model.draws.iter_mut().enumerate() {
                    if draw.material_index == mat_idx {
                        if let std::collections::hash_map::Entry::Vacant(e) =
                            preview.saved_binds.entry(draw_idx)
                        {
                            e.insert(draw.texture_bind_group.take());
                        }
                        draw.texture_bind_group = Some(gpu::create_texture_bind_group(
                            device,
                            texture_bgl,
                            &preview.texture_view,
                            sampler,
                        ));
                    }
                }
                preview.previewed[mat_idx] = true;
            } else if !preview.selection[mat_idx] && preview.previewed[mat_idx] {
                // プレビュー解除: 退避した元の bind group を復元
                for (draw_idx, draw) in loaded.gpu_model.draws.iter_mut().enumerate() {
                    if draw.material_index == mat_idx {
                        if let Some(orig) = preview.saved_binds.remove(&draw_idx) {
                            draw.texture_bind_group = orig;
                        }
                    }
                }
                preview.previewed[mat_idx] = false;
            }
        }
    }

    /// テクスチャプレビューを確定適用
    pub fn apply_tex_preview(&mut self) {
        let Some(preview) = self.tex.pending_preview.take() else {
            return;
        };
        let path = &preview.path;

        // 選択された材質のインデックスを収集
        let selected: Vec<usize> = preview
            .selection
            .iter()
            .enumerate()
            .filter_map(|(i, &v)| if v { Some(i) } else { None })
            .collect();

        if selected.is_empty() {
            // 何も選択されていなければ元に戻す
            self.cancel_tex_preview_inner(preview);
            return;
        }

        // IrModel にテクスチャを追加（1回だけ）
        let Some(ref mut loaded) = self.loaded else {
            return;
        };

        let is_psd = preview.is_psd;
        let tex_data = preview.cached_data.clone();

        // 一時パスの場合はキャッシュ用にバイト列を保持（消失前に判定済みのフラグを使用）
        let cached_data = if preview.was_temp {
            Some(tex_data.clone())
        } else {
            None
        };

        let basename = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let (ir_data, ir_filename, ir_mime) = if is_psd {
            match Self::psd_to_png(&tex_data) {
                Ok(png_data) => (
                    png_data,
                    format!("{}.png", basename),
                    "image/png".to_string(),
                ),
                Err(e) => {
                    log::error!("PSD→PNG変換失敗: {e}");
                    return;
                }
            }
        } else {
            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let ext_l = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            let mime = crate::intermediate::types::mime_for_ext(&ext_l);
            (tex_data, filename, mime.to_string())
        };

        let tex_idx = loaded.ir.textures.len();
        loaded
            .ir
            .textures
            .push(crate::intermediate::types::IrTexture {
                filename: ir_filename,
                data: ir_data,
                mime_type: ir_mime,
            });

        // 選択した材質の texture_index を更新
        let path_buf = path.clone();
        for &mat_idx in &selected {
            let mat = &mut loaded.ir.materials[mat_idx];
            mat.texture_index = Some(tex_idx);
            mat.apply_textured_defaults();
            log::info!(
                "テクスチャ割り当て: 材質[{}] '{}' ← {}",
                mat_idx,
                mat.name,
                path_buf.display()
            );
        }

        // 割り当て履歴を記録（reload_current 時の復元用）
        let tex_src = if let Some(data) = cached_data {
            TextureSource::Cached {
                original_name: path_buf.to_string_lossy().into_owned(),
                data: Arc::from(data.into_boxed_slice()),
                is_psd,
            }
        } else {
            TextureSource::File(path_buf.clone())
        };
        for &mat_idx in &selected {
            self.tex.assignments.insert(mat_idx, tex_src.clone());
        }

        // サムネイル用 egui テクスチャを解放
        if let Some(tex_id) = preview.preview_tex_id {
            let mut renderer = self.render_state.renderer.write();
            renderer.free_texture(&tex_id);
        }

        // GPU は既にプレビュー状態 → saved_binds を捨てて確定
        // saved_binds 内の未プレビュー分は復元
        for (draw_idx, orig) in preview.saved_binds.into_iter() {
            let draw = &mut loaded.gpu_model.draws[draw_idx];
            if !selected.contains(&draw.material_index) {
                draw.texture_bind_group = orig;
            }
        }

        // 材質キャッシュ更新
        self.update_mat_cache();
    }

    /// テクスチャプレビューをキャンセル（元に戻す）
    pub fn cancel_tex_preview(&mut self) {
        let Some(preview) = self.tex.pending_preview.take() else {
            return;
        };
        self.cancel_tex_preview_inner(preview);
    }

    fn cancel_tex_preview_inner(&mut self, preview: PendingTexPreview) {
        // サムネイル用 egui テクスチャを解放
        if let Some(tex_id) = preview.preview_tex_id {
            let mut renderer = self.render_state.renderer.write();
            renderer.free_texture(&tex_id);
        }
        let Some(ref mut loaded) = self.loaded else {
            return;
        };
        // 退避した全 bind group を復元
        for (draw_idx, orig) in preview.saved_binds.into_iter() {
            if draw_idx < loaded.gpu_model.draws.len() {
                loaded.gpu_model.draws[draw_idx].texture_bind_group = orig;
            }
        }
    }

    /// 遅延処理（ファイル読み込み、GPU再構築、PMX変換など）を実行
    fn process_pending_tasks(&mut self) {
        if let Some((_, true)) = self.pending.load {
            let (path, _) = self
                .pending
                .load
                .take()
                .expect("pending_load は Some(true) 確認済み");
            self.load_file(path);
        }
        // モデル追加読み込み（アペンド）
        if let Some((_, true)) = self.pending.append {
            let (path, _) = self
                .pending
                .append
                .take()
                .expect("pending_append は Some(true) 確認済み");
            self.append_model(path);
        }
        // unitypackage モデル遅延読み込み
        if self.pending.pkg_load.as_ref().is_some_and(|p| p.shown) {
            let p = self
                .pending
                .pkg_load
                .take()
                .expect("pending_pkg_load は shown 確認済み");
            let source_path = p.source_path.clone();

            // source_override を構築（nested_archive_source > archive_snapshot > None）
            let source_override = if let Some(nested) = p.nested_archive_source {
                Some(nested)
            } else if let Some(ref snap) = p.archive_snapshot {
                Some(ReloadableSource::Snapshot {
                    original_path: source_path.clone(),
                    main_bytes: Arc::clone(snap),
                    aux_files: HashMap::new(),
                })
            } else {
                None
            };

            // アペンドモード: unitypackage内モデルを既存モデルに追加
            if p.append {
                // リロード経由の場合はテクスチャ選択ダイアログを抑制
                if p.suppress_tex_match {
                    self.suppress_tex_match = true;
                }
                self.append_from_pkg(
                    p.assets,
                    p.fbx_index,
                    p.model_type,
                    &source_path,
                    source_override.clone(),
                );
                self.suppress_tex_match = false;
                // 以下の通常ロードをスキップ
            } else {
                match p.model_type {
                    PkgModelType::Fbx => {
                        if self.loaded.is_some() {
                            let has_anim = if let Some(asset) = p.assets.get(p.fbx_index) {
                                crate::fbx::animation::load_fbx_animation_from_data(&asset.data)
                                    .is_ok_and(|a| !a.is_empty())
                            } else {
                                false
                            };
                            if has_anim {
                                let fbx_name = p
                                    .assets
                                    .get(p.fbx_index)
                                    .map(|a| a.filename())
                                    .unwrap_or_default();
                                self.pending.fbx_choice = Some(PendingFbxChoice {
                                    path: std::path::PathBuf::from(&fbx_name),
                                    load_model: true,
                                    load_animation: true,
                                    pkg_context: Some(PendingFbxChoicePkg {
                                        assets: p.assets,
                                        fbx_index: p.fbx_index,
                                        source_path,
                                        archive_snapshot: p.archive_snapshot,
                                        nested_archive_source: source_override,
                                    }),
                                    preloaded: None,
                                });
                            } else {
                                match self.load_fbx_from_assets(
                                    p.assets,
                                    p.fbx_index,
                                    &source_path,
                                    FbxLoadMode::ModelOnly,
                                    source_override,
                                ) {
                                    Ok(()) => {
                                        self.convert_message = None;
                                    }
                                    Err(e) => {
                                        log::error!("読み込み失敗: {e}");
                                        self.convert_message = Some(ConvertMessage::failure(
                                            format!("ファイルを読み込めませんでした。\n詳細: {e}"),
                                        ));
                                    }
                                }
                            }
                        } else {
                            match self.load_fbx_from_assets(
                                p.assets,
                                p.fbx_index,
                                &source_path,
                                FbxLoadMode::Both,
                                source_override,
                            ) {
                                Ok(()) => {
                                    log::info!("読み込み成功: {}", source_path.display());
                                    self.convert_message = None;
                                }
                                Err(e) => {
                                    log::error!("読み込み失敗: {e}");
                                    self.convert_message = Some(ConvertMessage::failure(format!(
                                        "ファイルを読み込めませんでした。\n詳細: {e}"
                                    )));
                                }
                            }
                        }
                    }
                    PkgModelType::Vrm => {
                        match self.load_vrm_from_assets(
                            p.assets,
                            p.fbx_index,
                            &source_path,
                            source_override,
                        ) {
                            Ok(()) => {
                                log::info!("読み込み成功: {}", source_path.display());
                                self.convert_message = None;
                            }
                            Err(e) => {
                                log::error!("読み込み失敗: {e}");
                                self.convert_message = Some(ConvertMessage::failure(format!(
                                    "ファイルを読み込めませんでした。\n詳細: {e}"
                                )));
                            }
                        }
                    }
                }
            } // else (通常ロード)
        }
        // アーカイブモデル遅延読み込み
        if self.pending.archive_load.as_ref().is_some_and(|p| p.shown) {
            let p = self
                .pending
                .archive_load
                .take()
                .expect("pending_archive_load は shown 確認済み");
            let source_path = p.source_path.clone();
            match self.load_model_from_archive(p) {
                Ok(()) => {
                    log::info!("アーカイブ読み込み成功: {}", source_path.display());
                    self.convert_message = None;
                    self.anim.state = None;
                    self.anim.library.clear();
                    self.anim.active_index = None;
                }
                Err(e) => {
                    log::error!("アーカイブ読み込み失敗: {e}");
                    self.convert_message = Some(ConvertMessage::failure(format!(
                        "アーカイブからの読み込みに失敗しました。\n詳細: {e}"
                    )));
                }
            }
        }
        if self.pending.rebuild == Some(PendingOverlay::Ready) {
            self.pending.rebuild = None;
            self.rebuild_gpu_model();
        }
        if self.pending.reload == Some(PendingOverlay::Ready) {
            self.pending.reload = None;
            self.reload_current();
        }
        if self.pending.convert == Some(PendingOverlay::Ready) {
            self.pending.convert = None;
            ui::execute_conversion(self);
        }
    }

    /// アニメーション状態の更新（ボーン適用 + モーフ適用）
    fn update_animation(&mut self, dt: f32, ctx: &egui::Context) {
        if let Some(ref mut anim) = self.anim.state {
            if anim.playing {
                anim.advance(dt);
                ctx.request_repaint();
            }

            let expr_changed = anim.apply_expressions(&mut self.morph_weights);
            if expr_changed {
                self.morph_dirty = true;
            }

            if let Some(ref mut loaded) = self.loaded {
                let queue = &self.render_state.queue;
                anim.apply_bone_animation(&mut loaded.gpu_model, queue, &self.morph_weights);
                self.morph_dirty = false;
            }
        }
    }

    /// ドラッグ＆ドロップ処理。(画像ホバー中, モデルホバー中) を返す
    fn process_drag_and_drop(&mut self, ctx: &egui::Context) -> (bool, bool) {
        let (dropped_files, hover_ext, shift_held) = ctx.input(|i| {
            let hover_ext = i
                .raw
                .hovered_files
                .first()
                .and_then(|f| f.path.as_ref())
                .and_then(|p| p.extension())
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();
            self.drag_hovering = !i.raw.hovered_files.is_empty();
            let paths: Vec<PathBuf> = i
                .raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect();
            (paths, hover_ext, i.modifiers.shift)
        });
        let is_hover_image = IMAGE_EXTENSIONS.contains(&hover_ext.as_str());
        let is_hover_model = MODEL_EXTENSIONS.contains(&hover_ext.as_str());

        if !dropped_files.is_empty() {
            let mut image_files: Vec<PathBuf> = Vec::new();
            let mut model_file: Option<PathBuf> = None;
            for path in dropped_files {
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
                    image_files.push(path);
                } else {
                    model_file = Some(path);
                }
            }

            let has_loaded_model = self.loaded.is_some();

            if let Some(model_path) = model_file {
                // アペンド対応形式: VRM/FBX/PMX/PMD のみ
                let append_ext = model_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                let is_appendable = matches!(
                    append_ext.as_str(),
                    "vrm" | "fbx" | "pmx" | "pmd" | "unitypackage" | "zip" | "7z"
                );

                if is_temp_path(&model_path) {
                    // 一時パス（zipアーカイブ等）: ファイル消失前にバイト列を先読み
                    match std::fs::read(&model_path) {
                        Ok(bytes) => {
                            let mut aux = HashMap::new();
                            if let Some(dir) = model_path.parent() {
                                collect_image_files_recursive(dir, dir, &mut aux);
                            }
                            self.preloaded = Some(PreloadedData {
                                path: model_path.clone(),
                                main_bytes: bytes.into(),
                                aux_files: aux,
                            });
                        }
                        Err(e) => {
                            log::error!("一時ファイル先読み失敗: {e}");
                        }
                    }
                    if shift_held && has_loaded_model && is_appendable {
                        self.append_model(model_path);
                    } else {
                        self.load_file(model_path);
                    }
                    // PendingFbxChoice にデータが移されていなければクリア
                    if self.pending.fbx_choice.is_none() {
                        self.preloaded = None;
                    }
                } else {
                    // 通常パス: プログレスオーバーレイ付き遅延ロード
                    if shift_held && has_loaded_model && is_appendable {
                        self.pending.append = Some((model_path, false));
                    } else {
                        self.pending.load = Some((model_path, false));
                    }
                }
            }

            if !image_files.is_empty() && has_loaded_model {
                if image_files.len() == 1 {
                    let path = image_files.into_iter().next().expect("image_files は非空");
                    self.open_texture_preview(path);
                } else {
                    self.auto_assign_textures(image_files);
                }
            }
        }

        (is_hover_image, is_hover_model)
    }

    /// キーボードショートカット処理
    fn process_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        let wants_kb = ctx.wants_keyboard_input();
        ctx.input(|i| {
            if i.modifiers.ctrl && i.key_pressed(egui::Key::O) {
                self.open_file_dialog();
            }
            if !wants_kb {
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::R) {
                    if let Some(ref loaded) = self.loaded {
                        let (bbox_min, bbox_max) = loaded.gpu_model.bbox();
                        self.camera.reset_to_bbox_with_margin(
                            bbox_min,
                            bbox_max,
                            self.last_viewport_width,
                            self.last_viewport_height,
                        );
                    }
                }
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::F) {
                    if let Some(ref loaded) = self.loaded {
                        let (bbox_min, bbox_max) = loaded.gpu_model.bbox();
                        self.camera.fit_to_bbox_with_margin(
                            bbox_min,
                            bbox_max,
                            self.last_viewport_width,
                            self.last_viewport_height,
                        );
                    }
                }
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::G) {
                    self.display.show_grid = !self.display.show_grid;
                }
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::B) {
                    self.display.show_bones = !self.display.show_bones;
                }
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::P) {
                    self.display.show_spring_bones = !self.display.show_spring_bones;
                }
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::W) {
                    self.display.draw_mode = match self.display.draw_mode {
                        DrawMode::Solid => DrawMode::Wireframe,
                        DrawMode::Wireframe => DrawMode::SolidWireframe,
                        DrawMode::SolidWireframe => DrawMode::Solid,
                    };
                }
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::N) {
                    self.display.show_normals = !self.display.show_normals;
                }
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::L) {
                    self.display.light_mode = match self.display.light_mode {
                        LightMode::CameraFollow => LightMode::Fixed,
                        LightMode::Fixed => LightMode::CameraFollow,
                    };
                }
                {
                    let deg15 = 15.0_f32.to_radians();
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num0) {
                        self.camera.yaw = 0.0;
                        self.camera.pitch = 0.0;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num1) {
                        self.camera.yaw = std::f32::consts::FRAC_PI_2;
                        self.camera.pitch = 0.0;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num2) {
                        self.camera.pitch -= deg15;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num3) {
                        self.camera.yaw = -std::f32::consts::FRAC_PI_2;
                        self.camera.pitch = 0.0;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num4) {
                        self.camera.yaw += deg15;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num5) {
                        self.camera.perspective = !self.camera.perspective;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num6) {
                        self.camera.yaw -= deg15;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num7) {
                        self.camera.yaw = 0.0;
                        self.camera.pitch = std::f32::consts::FRAC_PI_2 - 0.01;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num8) {
                        self.camera.pitch += deg15;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Num9) {
                        self.camera.yaw = std::f32::consts::PI;
                        self.camera.pitch = 0.0;
                    }
                    if !i.modifiers.ctrl && i.key_pressed(egui::Key::Period) {
                        if let Some(ref loaded) = self.loaded {
                            let (bbox_min, bbox_max) = loaded.gpu_model.bbox();
                            self.camera.fit_to_bbox_with_margin(
                                bbox_min,
                                bbox_max,
                                self.last_viewport_width,
                                self.last_viewport_height,
                            );
                        }
                    }
                }
                if i.key_pressed(egui::Key::Space) {
                    if let Some(ref mut anim) = self.anim.state {
                        anim.playing = !anim.playing;
                    }
                }
                if i.key_pressed(egui::Key::ArrowLeft) {
                    if let Some(ref mut anim) = self.anim.state {
                        if !anim.playing {
                            anim.step_frame(false);
                        }
                    }
                }
                if i.key_pressed(egui::Key::ArrowRight) {
                    if let Some(ref mut anim) = self.anim.state {
                        if !anim.playing {
                            anim.step_frame(true);
                        }
                    }
                }
            }
        });
    }
}

impl eframe::App for ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // IPC: 別プロセスからのファイルパス受信
        #[cfg(target_os = "windows")]
        while let Ok(path) = self.ipc_receiver.try_recv() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            if !path.as_os_str().is_empty() {
                self.pending.load = Some((path, false));
            }
        }

        // ウィンドウタイトル更新
        if let Some(title) = self.window_title.take() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }

        // FPS計測（フレームカウント方式: 直近1秒のフレーム数から算出）
        let now = Instant::now();
        let dt = if let Some(&last) = self.frame_times.back() {
            now.duration_since(last).as_secs_f32()
        } else {
            0.0
        };
        self.frame_times.push_back(now);
        // 1秒より古いエントリを除去
        let window = Duration::from_secs(1);
        while self
            .frame_times
            .front()
            .is_some_and(|&t| now.duration_since(t) > window)
        {
            self.frame_times.pop_front();
        }
        // 表示FPS・ms更新（0.5秒ごと、ちらつき防止）
        if now.duration_since(self.fps_last_update).as_secs_f32() >= 0.5 {
            if self.frame_times.len() >= 2 {
                let span = now
                    .duration_since(*self.frame_times.front().unwrap())
                    .as_secs_f32();
                if span > 0.0 {
                    self.fps_display = (self.frame_times.len() - 1) as f32 / span;
                    self.frame_dt_ms = span / (self.frame_times.len() - 1) as f32 * 1000.0;
                }
            }
            self.fps_last_update = now;
        }

        self.process_pending_tasks();
        self.update_animation(dt, ctx);
        let (is_hover_image, is_hover_model) = self.process_drag_and_drop(ctx);
        self.process_keyboard_shortcuts(ctx);

        // トップバー
        egui::TopBottomPanel::top("top_bar").show(ctx, |bar| {
            egui::menu::bar(bar, |bar| {
                if bar.button("開く").clicked() {
                    self.open_file_dialog();
                }

                // モデル読み込み済みの場合のみ「追加」ボタンを表示
                if self.loaded.is_some()
                    && bar
                        .button("追加")
                        .on_hover_text("モデルを追加読み込み（Shift+D&Dでも可）")
                        .clicked()
                {
                    self.open_append_dialog();
                }

                if bar.button("ログ").clicked() {
                    open_directory(&self.logs_dir);
                }

                if let Some(ref loaded) = self.loaded {
                    bar.separator();
                    bar.label(
                        egui::RichText::new(&loaded.ir.name).color(egui::Color32::from_gray(0x20)),
                    );
                }
            });
        });

        // 右側パネル
        ui::show_side_panel(ctx, self);

        // テクスチャD&Dダイアログ + プレビュー同期
        ui::show_texture_drop_dialog(ctx, self);
        self.sync_tex_preview();

        // ステータスバー
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(22.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    if let Some(ref loaded) = self.loaded {
                        let ir = &loaded.ir;
                        let font = egui::FontId::proportional(11.0);
                        let color = egui::Color32::from_gray(0x30);

                        // ファイルパス
                        let path_label = if loaded.source.is_snapshot() {
                            format!(
                                "{} (キャッシュ済み)",
                                loaded.source.display_path().to_string_lossy()
                            )
                        } else {
                            loaded.source.display_path().to_string_lossy().into_owned()
                        };
                        ui.label(
                            egui::RichText::new(path_label)
                                .font(font.clone())
                                .color(color),
                        );

                        ui.separator();

                        // モデル統計（キャッシュ済み文字列）
                        ui.label(
                            egui::RichText::new(&loaded.stats_cache.status_text)
                                .font(font.clone())
                                .color(color),
                        );

                        // FBXの場合、テクスチャ設定状況（キャッシュ済み文字列）
                        if ir.source_format == crate::intermediate::types::SourceFormat::Fbx {
                            let tex_set = loaded.mat_cache.tex_set_count;
                            let tex_total = ir.materials.len();
                            ui.separator();
                            let tex_color = if tex_set == tex_total {
                                egui::Color32::from_rgb(0x40, 0xC0, 0x40)
                            } else {
                                egui::Color32::from_rgb(0xD0, 0xA0, 0x40)
                            };
                            ui.label(
                                egui::RichText::new(&loaded.mat_cache.tex_status_text)
                                    .font(font)
                                    .color(tex_color),
                            );
                        }
                    } else {
                        ui.label(
                            egui::RichText::new("VRM/FBX ファイルを読み込んでください")
                                .font(egui::FontId::proportional(11.0))
                                .color(egui::Color32::from_gray(0x60)),
                        );
                    }

                    // (FPS表示はビューポートオーバーレイに移動)
                });
            });

        // 中央ビューポート
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill({
                let b = (self.display.bg_brightness * 255.0).clamp(0.0, 255.0) as u8;
                egui::Color32::from_rgb(b, b, b)
            }))
            .show(ctx, |viewport| {
                let available = viewport.available_size();
                if available.x < 1.0 || available.y < 1.0 {
                    return;
                }

                // カメラ操作
                let response = viewport.allocate_rect(
                    egui::Rect::from_min_size(viewport.cursor().min, available),
                    egui::Sense::click_and_drag(),
                );
                self.camera.handle_input(ctx, &response);

                // ダブルクリック: モデルにフィット
                if response.double_clicked() {
                    if let Some(ref loaded) = self.loaded {
                        let (bbox_min, bbox_max) = loaded.gpu_model.bbox();
                        self.camera.fit_to_bbox_with_margin(bbox_min, bbox_max, self.last_viewport_width, self.last_viewport_height);
                    }
                }

                // モーフウェイト変更検知 → 頂点バッファ更新
                if self.morph_dirty {
                    if let Some(ref mut loaded) = self.loaded {
                        let queue = &self.render_state.queue;
                        loaded.gpu_model.apply_morphs(
                            &self.morph_weights,
                            queue,
                        );
                        self.morph_dirty = false;
                    }
                }

                // 3D描画（renderer を take して &mut で使い、戻す）
                if let Some(ref loaded) = self.loaded {
                    let width = (available.x * ctx.pixels_per_point()) as u32;
                    let height = (available.y * ctx.pixels_per_point()) as u32;
                    if width > 0 && height > 0 {
                        if let Some(mut renderer) = self.renderer.take() {
                            let device = &self.render_state.device;
                            let queue = &self.render_state.queue;

                            let animated_globals = self.anim.state.as_ref()
                                .map(|anim| anim.animated_globals());
                            let is_vrm0 = loaded.ir.source_format.is_vrm0();

                            let render_params = RenderParams {
                                camera: &self.camera,
                                width,
                                height,
                                material_visibility: &self.material_visibility,
                                display: &self.display,
                                animated_bone_globals: animated_globals,
                                is_vrm0,
                            };

                            let (texture_id, _) = renderer.render_to_texture(
                                device,
                                queue,
                                &mut self.render_state.renderer.write(),
                                &loaded.gpu_model,
                                &loaded.ir,
                                &render_params,
                                &mut self.viewport_texture_id,
                            );

                            self.renderer = Some(renderer);

                            // egui に表示
                            let uv = egui::Rect::from_min_max(
                                egui::pos2(0.0, 0.0),
                                egui::pos2(1.0, 1.0),
                            );
                            viewport.painter().image(
                                texture_id,
                                response.rect,
                                uv,
                                egui::Color32::WHITE,
                            );
                        }
                    }
                }

                // ドロップオーバーレイ
                if self.drag_hovering {
                    let rect = response.rect;
                    let has_model = self.loaded.is_some();
                    let (overlay_color, overlay_text) = if is_hover_image && has_model {
                        (
                            egui::Color32::from_rgba_unmultiplied(0x40, 0xC0, 0x40, 0x60),
                            "テクスチャを割り当て",
                        )
                    } else if is_hover_image && !has_model {
                        (
                            egui::Color32::from_rgba_unmultiplied(0xD0, 0xA0, 0x40, 0x60),
                            "先にモデルを読み込んでください",
                        )
                    } else if is_hover_model {
                        let shift = ctx.input(|i| i.modifiers.shift);
                        if shift && has_model {
                            (
                                egui::Color32::from_rgba_unmultiplied(0x40, 0xC0, 0xFF, 0x60),
                                "モデルを追加読み込み（Shift）",
                            )
                        } else {
                            (
                                egui::Color32::from_rgba_unmultiplied(0x40, 0x80, 0xFF, 0x60),
                                "モデルファイルを読み込み",
                            )
                        }
                    } else {
                        (
                            egui::Color32::from_rgba_unmultiplied(0x80, 0x80, 0x80, 0x60),
                            "非対応の形式です（VRM/FBX/PMX/PMD/画像に対応）",
                        )
                    };
                    viewport.painter().rect_filled(
                        rect,
                        0.0,
                        overlay_color,
                    );
                    viewport.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        overlay_text,
                        egui::FontId::proportional(28.0),
                        egui::Color32::WHITE,
                    );
                }

                // ビューポートのサイズを記録（フィット計算用）
                self.last_viewport_width = response.rect.width();
                self.last_viewport_height = response.rect.height();

                // 初回ロード時の refit（ビューポートサイズ確定後）
                if self.pending.refit {
                    self.pending.refit = false;
                    if let Some(ref loaded) = self.loaded {
                        let (bbox_min, bbox_max) = loaded.gpu_model.bbox();
                        self.camera.reset_to_bbox_with_margin(bbox_min, bbox_max, self.last_viewport_width, self.last_viewport_height);
                    }
                }

                // カメラ情報（左上、テキスト直接描画）
                if self.loaded.is_some() {
                    let rect = response.rect;
                    let cam_info = format!(
                        "({:.1},{:.1},{:.1}) D:{:.1} Y:{:.0}° P:{:.0}°",
                        self.camera.target.x,
                        self.camera.target.y,
                        self.camera.target.z,
                        self.camera.distance,
                        self.camera.yaw.to_degrees(),
                        self.camera.pitch.to_degrees(),
                    );
                    viewport.painter().text(
                        egui::pos2(rect.left() + 10.0, rect.top() + 10.0),
                        egui::Align2::LEFT_TOP,
                        &cam_info,
                        egui::FontId::monospace(11.0),
                        egui::Color32::BLACK,
                    );

                    // フィット・リセットボタン（右上）
                    let margin = 8.0;
                    let btn_pos = egui::pos2(rect.right() - margin, rect.top() + margin);
                    let btn_area = egui::Area::new(egui::Id::new("camera_btn_overlay"))
                        .fixed_pos(btn_pos)
                        .constrain(false)
                        .interactable(true)
                        .anchor(egui::Align2::RIGHT_TOP, egui::Vec2::ZERO);
                    btn_area.show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            if ui.small_button("フィット(F)").on_hover_text("モデルにフィット").clicked() {
                                if let Some(ref loaded) = self.loaded {
                                    let (bbox_min, bbox_max) = loaded.gpu_model.bbox();
                                    self.camera.fit_to_bbox_with_margin(bbox_min, bbox_max, rect.width(), rect.height());
                                }
                            }
                            if ui.small_button("リセット(R)").on_hover_text("カメラをリセット").clicked() {
                                if let Some(ref loaded) = self.loaded {
                                    let (bbox_min, bbox_max) = loaded.gpu_model.bbox();
                                    self.camera.reset_to_bbox_with_margin(bbox_min, bbox_max, rect.width(), rect.height());
                                }
                            }
                        });
                    });
                }

                // FPS表示（右上オーバーレイ）
                {
                    let rect = response.rect;
                    let fps_text = format!("{:.0} fps  {:.1} ms", self.fps_display, self.frame_dt_ms);
                    viewport.painter().text(
                        egui::pos2(rect.right() - 10.0, rect.top() + 10.0),
                        egui::Align2::RIGHT_TOP,
                        &fps_text,
                        egui::FontId::monospace(11.0),
                        egui::Color32::BLACK,
                    );
                }

                // Aスタンス/Tスタンス変換失敗時の常時警告（操作ヒントの上）
                if self.normalize_pose || self.normalize_to_tstance {
                    if let Some(ref loaded) = self.loaded {
                        use crate::intermediate::types::AStanceResult;
                        let label = if loaded.ir.source_format.is_pmx_pmd()
                            || self.normalize_to_tstance
                        {
                            "Tスタンス"
                        } else {
                            "Aスタンス"
                        };
                        let warn = match loaded.primary_astance_result {
                            AStanceResult::NotFound => Some((
                                format!("⚠ {}変換失敗: 腕ボーンが見つかりません", label),
                                egui::Color32::from_rgb(0xE0, 0x40, 0x40),
                            )),
                            AStanceResult::AlreadyAStance => Some((
                                format!("※ 既に{}に近いためスキップしました", label),
                                egui::Color32::from_rgb(0xCC, 0x99, 0x00),
                            )),
                            _ => None,
                        };
                        if let Some((text, color)) = warn {
                            let rect = response.rect;
                            let font = egui::FontId::proportional(12.0);
                            viewport.painter().text(
                                egui::pos2(rect.left() + 8.0, rect.bottom() - 36.0),
                                egui::Align2::LEFT_BOTTOM,
                                &text,
                                font,
                                color,
                            );
                        }
                    }
                }

                // 操作ヒント（左下、2行で常時表示）
                {
                    let rect = response.rect;
                    let hint_color = if self.loaded.is_some() {
                        egui::Color32::BLACK
                    } else {
                        egui::Color32::from_gray(0xC0)
                    };
                    let font = egui::FontId::proportional(12.0);
                    if self.loaded.is_some() {
                        viewport.painter().text(
                            egui::pos2(rect.left() + 8.0, rect.bottom() - 22.0),
                            egui::Align2::LEFT_BOTTOM,
                            "左ドラッグ:回転  右/中ドラッグ:パン  ホイール:ズーム  Shift:精密  ダブルクリック:フィット",
                            font.clone(),
                            hint_color,
                        );
                        viewport.painter().text(
                            egui::pos2(rect.left() + 8.0, rect.bottom() - 8.0),
                            egui::Align2::LEFT_BOTTOM,
                            "R:リセット  F:フィット  G:グリッド  B:ボーン  P:物理  W:ワイヤー  N:法線  L:ライト",
                            font,
                            hint_color,
                        );
                    } else {
                        viewport.painter().text(
                            egui::pos2(rect.left() + 8.0, rect.bottom() - 8.0),
                            egui::Align2::LEFT_BOTTOM,
                            "Ctrl+O:開く  ドラッグ&ドロップ:VRM/FBXファイル読込",
                            font,
                            hint_color,
                        );
                    }
                }

                // プログレスオーバーレイ（読み込み中 / 変換中）
                self.paint_progress_overlay(viewport, response.rect, ctx);
                self.update_progress_flags(ctx);

                // 結果メッセージオーバーレイ（5秒フェードアウト）
                if let Some(ref cm) = self.convert_message {
                    let elapsed = cm.elapsed_secs();
                    let display_secs = 5.0_f32;
                    let fade_start = 3.5_f32;
                    if elapsed < display_secs {
                        let alpha = if elapsed > fade_start {
                            1.0 - (elapsed - fade_start) / (display_secs - fade_start)
                        } else {
                            1.0
                        };
                        let a = (alpha * 180.0) as u8;
                        let (msg, color) = match &cm.result {
                            ConvertResult::Success(m) => (m.as_str(), egui::Color32::from_rgba_unmultiplied(0x30, 0xC0, 0x30, (alpha * 255.0) as u8)),
                            ConvertResult::Warning(m) => (m.as_str(), egui::Color32::from_rgba_unmultiplied(0xE0, 0x40, 0x40, (alpha * 255.0) as u8)),
                            ConvertResult::Failure(m) => (m.as_str(), egui::Color32::from_rgba_unmultiplied(0xE0, 0x40, 0x40, (alpha * 255.0) as u8)),
                        };
                        let rect = response.rect;
                        // 背景帯
                        let text_galley = viewport.painter().layout_no_wrap(
                            msg.to_string(),
                            egui::FontId::proportional(14.0),
                            color,
                        );
                        let text_h = text_galley.size().y * (msg.lines().count().max(1) as f32) + 16.0;
                        let bar_rect = egui::Rect::from_min_size(
                            egui::pos2(rect.left(), rect.bottom() - text_h),
                            egui::vec2(rect.width(), text_h),
                        );
                        viewport.painter().rect_filled(
                            bar_rect,
                            0.0,
                            egui::Color32::from_rgba_unmultiplied(0, 0, 0, a),
                        );
                        // テキスト
                        viewport.painter().text(
                            egui::pos2(rect.left() + 12.0, bar_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            msg,
                            egui::FontId::proportional(14.0),
                            color,
                        );
                        ctx.request_repaint();
                    }
                }
                // タイムアウトでメッセージクリア
                if self.convert_message.as_ref().is_some_and(|cm| cm.elapsed_secs() >= 5.0) {
                    self.convert_message = None;
                }
            });
    }
}

/// ディレクトリをOSのファイルマネージャで開く
fn open_directory(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer").arg(path).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(path).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}
