//! ビューアのメイン状態管理（ViewerApp 構造体定義、eframe::App impl）

pub mod file_io;
pub mod helpers;
pub mod material_edit;
pub mod material_presets;
pub mod pending;
pub mod persistence;
pub mod texture_mgmt;

use std::collections::VecDeque;
use std::path::PathBuf;

use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;
use eframe::egui_wgpu;

use crate::intermediate::types::{IrMaterial, IrModel};
use crate::unitypackage::PkgModelLocator;

use super::animation::AnimationState;
use super::camera::OrbitCamera;
use super::gpu::{DrawMode, GpuRenderer, LightMode, RenderParams, ShaderOverride, ShaderSelection};
use super::mesh::GpuModel;
use super::ui;

/// ダークテーマのパネル背景色 (#1D1D1D)
const DARK_PANEL_BG: egui::Color32 = egui::Color32::from_rgb(0x1D, 0x1D, 0x1D);
/// ダークテーマのボーダー色 (#333333)
const DARK_BORDER_COLOR: egui::Color32 = egui::Color32::from_rgb(0x33, 0x33, 0x33);

// サブモジュールから再エクスポート
pub use helpers::{FbxLoadMode, PkgModelType, PreloadedData, ReloadableSource, TextureSource};
pub use pending::{
    ExportState, PendingArchive, PendingArchiveLoad, PendingFbxChoice, PendingFbxChoicePkg,
    PendingMultiLoad, PendingOverlay, PendingPkgModelLoad, PendingState, PendingUnityPackage,
};
pub use texture_mgmt::{CachedMaterialInfo, PendingTexMatch, PendingTexPreview, TextureState};

/// ステータスバー用キャッシュ
pub struct CachedStats {
    pub total_vertices: usize,
    pub total_faces: usize,
    /// 事前フォーマット済みステータス文字列（毎フレーム format! 回避）
    pub status_text: String,
}

impl CachedStats {
    pub(super) fn new(ir: &IrModel) -> Self {
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
    /// Phase 3: unitypackage モデルの安定キー（段階移行用）
    pub pkg_model: Option<PkgModelLocator>,
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
    /// 材質ごとの安定キー（pkg_index 経由ロード時に構築）
    pub pkg_material_keys: Vec<Option<crate::unitypackage::PkgMaterialKey>>,
    /// Prefab 経由ロード時の Prefab ファイル名（ファイル階層表示用）
    pub prefab_name: Option<String>,
    /// Prefab エントリのパス名（リロード時に再解決するため保持）
    pub prefab_entry_path: Option<String>,
}

impl LoadedModel {
    /// 同名の sibling 材質インデックスを返す（同一 MaterialGroup 内に限定）
    /// `link_same_name` のスコープ制限に使用
    pub fn same_name_siblings(&self, mat_idx: usize) -> Vec<usize> {
        let Some(target_mat) = self.ir.materials.get(mat_idx) else {
            return Vec::new();
        };
        let target_name = &target_mat.name;
        // mat_idx が属する MaterialGroup の範囲を特定
        let group_range = self
            .material_groups
            .iter()
            .find(|g| g.material_range.contains(&mat_idx))
            .map(|g| g.material_range.clone());
        let range = group_range.unwrap_or(0..self.ir.materials.len());
        self.ir.materials[range.clone()]
            .iter()
            .enumerate()
            .filter(|(i, m)| {
                let abs_idx = range.start + i;
                abs_idx != mat_idx && m.name == *target_name
            })
            .map(|(i, _)| range.start + i)
            .collect()
    }
}

/// 材質ごとの表示・描画状態（mat_idx でインデックス）
#[derive(Clone, Debug)]
pub struct MaterialDisplayState {
    /// 法線平滑化 ON/OFF
    pub smooth_normals: bool,
    /// カスタム法線クリア ON/OFF
    pub clear_normals: bool,
    /// ノーマルマップ適用 ON/OFF
    pub normal_map: bool,
    /// エミッシブ適用 ON/OFF
    pub emissive: bool,
}

impl Default for MaterialDisplayState {
    fn default() -> Self {
        Self {
            smooth_normals: false,
            clear_normals: false,
            normal_map: true,
            emissive: true,
        }
    }
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

/// 表示・描画関連の設定
#[derive(Clone)]
pub struct DisplaySettings {
    /// ライト明るさ (0.0〜2.0)
    pub light_intensity: f32,
    /// ライト色 RGB (linear)
    pub light_color: [f32; 3],
    /// 環境光 (0.0〜1.0)
    pub ambient_intensity: f32,
    /// 環境光 Sky 色 RGB (linear)
    pub ambient_sky_color: [f32; 3],
    /// 環境光 Ground 色 RGB (linear)
    pub ambient_ground_color: [f32; 3],
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
    /// シェーダーオーバーライドモード（GPU uniform 用）
    pub shader_override: ShaderOverride,
    /// MMD 専用レンダーパス使用（旧 mmd_mode）
    pub use_mmd_path: bool,
    /// Auto モード（モデル形式に応じて Standard/MMD を自動選択）
    pub auto_shader: bool,
    /// MToon アウトライン描画
    pub outline_enabled: bool,
    /// MMD エッジ描画
    pub mmd_edge_enabled: bool,
    /// MMD エッジ太さ全体スケール (0.1〜3.0)
    pub mmd_edge_thickness: f32,
    /// Bloom エフェクト
    pub bloom_enabled: bool,
    /// Bloom 合成強度 (0.0〜4.0、2.0 = VRM 標準)
    pub bloom_intensity: f32,
    /// Bloom 輝度抽出閾値 (0.0〜1.0)
    pub bloom_threshold: f32,
    /// Bloom 拡散段数 (3〜6)
    pub bloom_radius: u32,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            light_intensity: 0.7,
            light_color: [1.0, 1.0, 1.0],
            ambient_intensity: 0.5,
            ambient_sky_color: [1.0, 1.0, 1.0],
            ambient_ground_color: [0.6, 0.55, 0.5],
            bg_brightness: 0.19,
            show_grid: true,
            show_bones: false,
            bone_opacity: 0.85,
            show_spring_bones: false,
            spring_bone_opacity: 0.75,
            show_joints: false,
            joint_opacity: 0.75,
            draw_mode: DrawMode::Solid,
            light_mode: LightMode::Fixed,
            align_rigid_rotation: false,
            msaa: true,
            smooth_normals: false,
            clear_custom_normals: false,
            show_normals: false,
            normal_length: 0.1,
            shader_override: ShaderOverride::Default,
            use_mmd_path: false,
            auto_shader: true,
            outline_enabled: true,
            mmd_edge_enabled: true,
            mmd_edge_thickness: 1.0,
            bloom_enabled: false,
            bloom_intensity: 0.8,
            bloom_threshold: 0.0,
            bloom_radius: super::bloom::DEFAULT_BLOOM_RADIUS,
        }
    }
}

impl DisplaySettings {
    /// UI の ShaderSelection から内部状態を設定
    pub fn set_shader_selection(&mut self, sel: ShaderSelection) {
        match sel {
            ShaderSelection::Auto => {
                self.shader_override = ShaderOverride::Default;
                self.auto_shader = true;
                // use_mmd_path は normalize_shader_state で自動判定
            }
            ShaderSelection::Mtoon => {
                self.shader_override = ShaderOverride::Default;
                self.use_mmd_path = false;
                self.auto_shader = false;
            }
            ShaderSelection::Mmd => {
                self.shader_override = ShaderOverride::Default;
                self.use_mmd_path = true;
                self.auto_shader = false;
            }
            ShaderSelection::Unlit => {
                self.shader_override = ShaderOverride::Unlit;
                self.use_mmd_path = false;
                self.auto_shader = false;
            }
            ShaderSelection::GgxPreview => {
                self.shader_override = ShaderOverride::GgxPreview;
                self.use_mmd_path = false;
                self.auto_shader = false;
            }
            ShaderSelection::Normal => {
                self.shader_override = ShaderOverride::Normal;
                self.use_mmd_path = false;
                self.auto_shader = false;
            }
        }
    }

    /// 現在の内部状態から UI 表示用の ShaderSelection を取得
    pub fn shader_selection(&self) -> ShaderSelection {
        if self.auto_shader {
            ShaderSelection::Auto
        } else if self.use_mmd_path {
            ShaderSelection::Mmd
        } else {
            match self.shader_override {
                ShaderOverride::Default => ShaderSelection::Mtoon,
                ShaderOverride::Unlit => ShaderSelection::Unlit,
                ShaderOverride::GgxPreview => ShaderSelection::GgxPreview,
                ShaderOverride::Normal => ShaderSelection::Normal,
            }
        }
    }
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
    /// 材質ごとの表示ON/OFF（draw_idx でインデックス）
    pub material_visibility: Vec<bool>,
    /// 材質ごとの描画状態（mat_idx でインデックス）
    pub material_display: Vec<MaterialDisplayState>,
    /// 材質編集ドロワー（§C / §A）: 編集による bind group 再生成要求フラグ。
    /// mat_idx でインデックス、update() 終端で拾って rebuild_material_bind_groups を呼ぶ。
    pub material_dirty: Vec<bool>,
    /// 材質編集ドロワー（§A）: 現在編集中の材質インデックス（None なら Window 非表示）。
    pub editing_material_index: Option<usize>,
    /// 材質編集ドロワー（§H）: ロード直後の IR 材質スナップショット。
    /// 「初期値に戻す」ボタンから pristine_materials[mat_idx].clone() で復元する。
    /// `finish_load_with_gpu` で material_overrides の `apply_to` 実行前にキャプチャされ、
    /// reload 時も新 IR の値が pristine として設定される。
    pub pristine_materials: Vec<IrMaterial>,

    /// 材質編集ドロワー (Step 4-16b / review_016): 非 BaseColor テクスチャスロット割当の
    /// ファイルパス記録。reload 時に再読込 + assign_texture_core で復元する。
    /// key = (mat_idx, TextureSlot), value = ファイルパス。
    /// 新モデルロード時 (is_reload = false) にクリアされる。
    pub slot_texture_paths: std::collections::HashMap<
        (usize, crate::intermediate::types::TextureSlot),
        std::path::PathBuf,
    >,

    /// 材質編集ドロワー: mat_idx ごとのパラメータ上書き値。
    ///
    /// Step 2 で `MaterialParamOverride` struct に集約され、§E の全セクション
    /// （基本 / 影 / アウトライン / リム / MatCap / UV アニメ / エミッシブ / 法線 / その他）の
    /// カラー・スカラー値を一括管理する。A スタンス変換・T スタンス変換などで IR が再
    /// ロードされても `MaterialParamOverride::apply_to()` で新 IR に再適用される。
    ///
    /// **Step 3 の移行計画**: 本フィールドは `MaterialEditRecord.param_override` に
    /// 吸収され、`declarative_macro` による diff/apply 自動生成に置き換えられる予定。
    pub material_overrides: std::collections::HashMap<usize, material_edit::MaterialParamOverride>,
    /// M6 Step 6.4: 材質パラメータのコピー/ペースト用クリップボード。
    /// テクスチャ割当は含まない（パス依存を避けるため）、カラー/スカラー値のみ。
    pub clipboard_material: Option<material_edit::MaterialParamOverride>,
    /// 材質フィルター文字列
    pub material_filter: String,
    /// 表情モーフフィルター文字列
    pub morph_filter: String,
    /// ドラッグオーバー中フラグ
    pub drag_hovering: bool,
    /// ビューポートテクスチャID
    pub viewport_texture_id: Option<egui::TextureId>,
    /// wgpu render state（CreationContext から取得）
    pub(crate) render_state: egui_wgpu::RenderState,
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
    /// ログメモリバッファ（ビューアモード用）
    pub log_buffer: crate::SharedLogBuffer,
    /// ログビュアーウインドウのモデル（別 OS ウインドウとして show_viewport_deferred で描画）
    pub log_viewer: super::log_viewer::SharedLogViewer,
    /// 最後にモデルファイルを開いたディレクトリ（ダイアログ経由のみ）
    pub last_model_dir: Option<PathBuf>,
    /// unitypackage 内で選択された FBX ファイル名（reload 時の照合用）
    pub selected_fbx_name: Option<String>,
    /// unitypackage 内で選択されたモデルの安定キー（Phase 3 移行用）
    pub selected_pkg_model: Option<PkgModelLocator>,
    /// アニメーションライブラリ
    pub anim: AnimLibrary,
    /// 右パネルの現在のタブ
    pub side_panel_tab: SidePanelTab,
    /// ウィンドウタイトル更新要求
    pub window_title: Option<String>,
    /// テクスチャ手動割当ダイアログを抑制（リロード中に使用）
    pub suppress_tex_match: bool,
    /// ホバー中の材質に対応する draw_index 群（3Dビューでハイライト表示）
    pub hovered_draw_indices: Vec<usize>,
    /// D&D一時ファイルの先読みデータ（ロードチェーン中のみ使用）
    pub(crate) preloaded: Option<PreloadedData>,
    /// 起動時刻（UVアニメーション用累積時間の基準）
    start_time: Instant,
    /// append 時のインスタンス ID カウンタ（ベースモデルは常に 0）
    pub next_instance_id: u32,
    /// スプラッシュ画像テクスチャ（モデル未ロード時に表示）
    splash_texture: Option<egui::TextureHandle>,
    /// アプリデータディレクトリ（設定・履歴ファイルの保存先）
    pub data_dir: PathBuf,
    /// セッション設定
    pub app_config: persistence::AppConfig,
    /// 設定変更フラグ（on_exit で保存するか判定）
    config_dirty: bool,
    /// ウィンドウ位置の遅延復元状態
    pending_window_restore: PendingWindowRestore,
    /// テクスチャ割り当て履歴（メモリキャッシュ）
    pub texture_history: persistence::TextureHistoryFile,
    /// ダークテーマ適用済みフラグ（update初回で再適用、eframeのスタイルリセット対策）
    dark_theme_applied: bool,
    /// テーマから解決済みのパネル背景色
    theme_panel_bg: egui::Color32,
    /// テーマから解決済みのボーダー色
    theme_border: egui::Color32,
    /// バックグラウンドロードの世代番号カウンタ。`fresh_request_id` 呼び出しごとに +1。
    /// 旧ロードの結果を識別して破棄するために使用する。
    pub(crate) next_request_id: u64,
    /// リロード用スナップショット（BG ロード完了後に復元する）
    pub(crate) reload_snapshot: Option<file_io::ReloadSnapshot>,
    /// ウォッチドッグ用ハートビート（毎フレーム tick して応答性を監視）
    heartbeat: super::watchdog::Heartbeat,
    /// GPU パイプラインのウォームアップ進行状態
    warmup_phase: WarmupPhase,
}

/// GpuRenderer の段階的ウォームアップ状態
#[derive(Default)]
enum WarmupPhase {
    /// GpuRenderer::new() をまだ呼んでいない
    #[default]
    NotStarted,
    /// GpuRenderer::new() 完了、sRGB+MSAA パイプライン未生成
    RendererCreated,
    /// sRGB+MSAA 完了
    SrgbMsaaDone,
    /// sRGB+noMSAA 完了
    SrgbNoMsaaDone,
    /// 全パイプライン事前生成完了
    Complete,
}

impl ViewerApp {
    /// 新しい `request_id` を発行する（世代番号カウンタをインクリメント）。
    pub(crate) fn fresh_request_id(&mut self) -> u64 {
        self.next_request_id = self.next_request_id.wrapping_add(1);
        self.next_request_id
    }

    /// `self.export.model_display_name` の変更に伴い、派生状態（ウィンドウタイトル、
    /// `pmx_output_path` のファイル名部分）を再生成する。
    ///
    /// - ウィンドウタイトル: `POPONE Model Viewer v{ver} - {model_display_name}` を次フレームで適用。
    /// - `pmx_output_path`: 親ディレクトリ（`converted_modelXX/`）は維持し、ファイル名のみ
    ///   `{model_display_name}.pmx` に差し替える。`pmx_output_path` が空、または
    ///   `model_display_name` が空の場合はパス更新をスキップする。
    ///
    /// 呼び出し元:
    /// - Prefab 名が後から確定した直後（file_io.rs の PkgInitial / 同期 Prefab ロード経路）
    /// - UI の TextEdit でユーザーが名前を編集したとき
    /// - リロード成功後の snapshot 復元時
    pub(crate) fn refresh_derived_from_display_name(&mut self) {
        self.window_title = Some(format!(
            "POPONE Model Viewer v{} - {}",
            env!("CARGO_PKG_VERSION"),
            self.export.model_display_name,
        ));
        if self.export.pmx_output_path.is_empty() || self.export.model_display_name.is_empty() {
            return;
        }
        let current = std::path::PathBuf::from(&self.export.pmx_output_path);
        let Some(parent) = current.parent() else {
            return;
        };
        let new_path = parent.join(format!("{}.pmx", self.export.model_display_name));
        self.export.pmx_output_path = new_path.to_string_lossy().into_owned();
    }

    /// GPU パイプラインの段階的ウォームアップ（スプラッシュ画面表示中に1フェーズずつ実行）。
    /// 各フェーズはシェーダーコンパイルを含むため数秒かかるが、フレーム間でスプラッシュ画像が表示される。
    fn tick_gpu_warmup(&mut self) {
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        match self.warmup_phase {
            WarmupPhase::NotStarted => {
                log::info!("[warmup] Creating GpuRenderer (shader compilation)");
                self.renderer = Some(super::gpu::GpuRenderer::new(device, queue, false));
                self.warmup_phase = WarmupPhase::RendererCreated;
            }
            WarmupPhase::RendererCreated => {
                if let Some(ref mut r) = self.renderer {
                    log::info!("[warmup] Pipeline set: sRGB + MSAA");
                    r.ensure_pipelines(device, false, true);
                }
                self.warmup_phase = WarmupPhase::SrgbMsaaDone;
            }
            WarmupPhase::SrgbMsaaDone => {
                if let Some(ref mut r) = self.renderer {
                    log::info!("[warmup] Pipeline set: sRGB + noMSAA");
                    r.ensure_pipelines(device, false, false);
                }
                self.warmup_phase = WarmupPhase::SrgbNoMsaaDone;
            }
            WarmupPhase::SrgbNoMsaaDone => {
                if let Some(ref mut r) = self.renderer {
                    log::info!("[warmup] Pipeline set: Unorm + MSAA");
                    r.ensure_pipelines(device, true, true);
                }
                self.warmup_phase = WarmupPhase::Complete;
            }
            WarmupPhase::Complete => {}
        }
    }

    /// ログビュアーウインドウ（OS レベルの別ウインドウ）を描画する。
    ///
    /// `show_viewport_deferred` を使う理由: メイン `update()` は 3D の
    /// `render_to_texture` を毎フレーム実行するため、`immediate` の「親子相互 repaint」
    /// がログ流入のたびに 3D 再描画を誘発してしまう。`deferred` なら子 viewport の
    /// 再描画は親を起こさない。
    ///
    /// クロージャは `Fn + Send + Sync + 'static` 制約があるので、`&mut self` を
    /// キャプチャできない。`Arc::clone` した `log_viewer` / `log_buffer`, `PathBuf::clone`
    /// した `logs_dir` を `move` で渡す。
    fn show_log_viewer(&self, ctx: &egui::Context) {
        // P1 修正: visible チェックを apply_geometry.take() より「先」に行う。
        // 隠れたまま起動した最初のフレームで apply_geometry が消費されると、後で
        // ユーザがボタンで開いたとき config 由来の保存位置が失われてしまうため。
        let apply_geometry = {
            let mut m = self.log_viewer.lock().unwrap_or_else(|p| p.into_inner());
            if !m.visible {
                return;
            }
            m.apply_geometry.take()
        };

        let mut builder = egui::ViewportBuilder::default()
            .with_title("popone - Log Viewer")
            .with_inner_size([720.0, 480.0]);
        if let Some((pos, size)) = apply_geometry {
            builder = builder
                .with_position(egui::pos2(pos[0], pos[1]))
                .with_inner_size([size[0], size[1]]);
        }

        let vp_id = egui::ViewportId::from_hash_of("popone_log_viewer");
        let model = Arc::clone(&self.log_viewer);
        let log_buffer = Arc::clone(&self.log_buffer);
        let logs_dir = self.logs_dir.clone();

        ctx.show_viewport_deferred(vp_id, builder, move |child_ctx, _class| {
            let mut m = model.lock().unwrap_or_else(|p| p.into_inner());

            // 1. SharedLogBuffer からの増分取り込み
            m.poll(&log_buffer);

            // 2. UI 描画
            m.draw(child_ctx, &log_buffer, &logs_dir);

            // 3. 最新 geometry を記録（同セッション reopen + on_exit 保存用）
            child_ctx.input(|i| {
                if let (Some(outer), Some(inner)) =
                    (i.viewport().outer_rect, i.viewport().inner_rect)
                {
                    m.last_geometry =
                        Some(([outer.min.x, outer.min.y], [inner.width(), inner.height()]));
                }
            });

            // 4. 閉じる検知（× ボタン）。hide() は last_geometry を apply_geometry に
            //    スナップショットして次回 show 時の位置維持を担保する。
            if child_ctx.input(|i| i.viewport().close_requested()) {
                m.hide();
            }

            // 5. visible 中は 150ms 間隔で再描画（新規ログの遅延表示防止）
            //    メイン update は 3 秒周期の自発 repaint しかしないので、ここで
            //    子 viewport だけを起こす。deferred なので親 3D は影響を受けない
            if m.visible {
                child_ctx.request_repaint_after(std::time::Duration::from_millis(150));
            }
        });
    }
}

/// ウィンドウ位置の初回フレーム検証・適用用
struct PendingWindowRestore {
    saved_config: Option<persistence::WindowConfig>,
    validated: bool,
}

impl ViewerApp {
    pub fn new(
        cc: &eframe::CreationContext,
        logs_dir: PathBuf,
        log_path: PathBuf,
        log_buffer: crate::SharedLogBuffer,
        data_dir: PathBuf,
        app_config: Option<persistence::AppConfig>,
    ) -> Self {
        let render_state = cc
            .wgpu_render_state
            .clone()
            .expect("wgpu render state required");

        // 日本語フォント読み込み
        Self::setup_cjk_fonts(&cc.egui_ctx);

        // ダークテーマ（Blender/Substance Painter 風）— popone.toml [theme] で色変更可能
        let theme = app_config
            .as_ref()
            .map(|c| &c.theme)
            .cloned()
            .unwrap_or_default();
        Self::setup_dark_theme(&cc.egui_ctx, &theme);

        // スプラッシュ画像読み込み
        let splash_texture = Self::load_splash_texture(&cc.egui_ctx);

        // シングルインスタンス: IPC パイプリスナー起動
        #[cfg(target_os = "windows")]
        let ipc_receiver = {
            let (tx, rx) = std::sync::mpsc::channel();
            super::single_instance::start_pipe_listener(tx, cc.egui_ctx.clone());
            rx
        };

        // config からディレクトリパスを復元
        let last_model_dir = app_config
            .as_ref()
            .and_then(|c| c.directory.last_model.as_ref())
            .map(PathBuf::from)
            .filter(|p| p.is_dir());
        let last_texture_dir = app_config
            .as_ref()
            .and_then(|c| c.directory.last_texture.as_ref())
            .map(PathBuf::from)
            .filter(|p| p.is_dir());

        // ウィンドウ位置の遅延復元用（設定ファイルに [window] セクションがある場合のみ）
        let saved_window = app_config.as_ref().and_then(|c| c.window.clone());
        let pending_window_restore = PendingWindowRestore {
            validated: saved_window.is_none(),
            saved_config: saved_window,
        };

        let app_config = app_config.unwrap_or_default();

        // テクスチャ履歴を読み込み
        let texture_history = persistence::load_texture_history(&data_dir);

        let tex = TextureState {
            last_dir: last_texture_dir,
            ..Default::default()
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
            material_display: Vec::new(),
            material_dirty: Vec::new(),
            editing_material_index: None,
            pristine_materials: Vec::new(),
            slot_texture_paths: std::collections::HashMap::new(),
            material_overrides: std::collections::HashMap::new(),
            clipboard_material: None,
            export: ExportState::default(),
            material_filter: String::new(),
            morph_filter: String::new(),
            drag_hovering: false,
            viewport_texture_id: None,
            render_state,
            normalize_pose: false,
            normalize_to_tstance: false,
            last_viewport_width: 1280.0,
            last_viewport_height: 720.0,
            tex,
            pending: PendingState::default(),
            frame_times: VecDeque::with_capacity(120),
            frame_dt_ms: 0.0,
            fps_display: 0.0,
            fps_last_update: Instant::now(),
            #[cfg(target_os = "windows")]
            ipc_receiver,
            logs_dir,
            log_path,
            log_buffer,
            log_viewer: Arc::new(std::sync::Mutex::new(
                super::log_viewer::LogViewerModel::from_config(&app_config.log_viewer),
            )),
            last_model_dir,
            selected_fbx_name: None,
            selected_pkg_model: None,
            anim: AnimLibrary::default(),
            side_panel_tab: SidePanelTab::Info,
            window_title: None,
            suppress_tex_match: false,
            hovered_draw_indices: Vec::new(),
            preloaded: None,
            start_time: Instant::now(),
            next_instance_id: 1,
            splash_texture,
            data_dir,
            app_config,
            config_dirty: false,
            pending_window_restore,
            texture_history,
            dark_theme_applied: false,
            theme_panel_bg: Self::theme_color(&theme.panel_bg, DARK_PANEL_BG),
            theme_border: Self::theme_color(&theme.border, DARK_BORDER_COLOR),
            next_request_id: 0,
            reload_snapshot: None,
            heartbeat: super::watchdog::start(Duration::from_secs(5), Duration::from_secs(2)),
            warmup_phase: WarmupPhase::NotStarted,
        }
    }

    /// mat_idx から stable key を引く
    pub fn pkg_key_for_material(
        &self,
        mat_idx: usize,
    ) -> Option<crate::unitypackage::PkgMaterialKey> {
        self.loaded
            .as_ref()?
            .pkg_material_keys
            .get(mat_idx)?
            .clone()
    }

    fn setup_cjk_fonts(ctx: &egui::Context) {
        // Noto Sans JP（OFL ライセンス）— 日本語プライマリ
        const NOTO_SANS_JP: &[u8] = include_bytes!("../../../assets/NotoSansJP-Regular.ttf");
        // Noto Sans SC（OFL ライセンス）— 簡体字フォールバック
        const NOTO_SANS_SC: &[u8] = include_bytes!("../../../assets/NotoSansSC-Regular.otf");

        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "noto_jp".to_owned(),
            egui::FontData::from_static(NOTO_SANS_JP).into(),
        );
        fonts.font_data.insert(
            "noto_sc".to_owned(),
            egui::FontData::from_static(NOTO_SANS_SC).into(),
        );
        // JP → SC の順にフォールバック（JP にないグリフを SC で補完）
        let proportional = fonts
            .families
            .get_mut(&egui::FontFamily::Proportional)
            .expect("Proportional フォントファミリーは常に存在");
        proportional.insert(0, "noto_sc".to_owned());
        proportional.insert(0, "noto_jp".to_owned());
        let monospace = fonts
            .families
            .get_mut(&egui::FontFamily::Monospace)
            .expect("Monospace フォントファミリーは常に存在");
        monospace.push("noto_jp".to_owned());
        monospace.push("noto_sc".to_owned());
        ctx.set_fonts(fonts);
    }

    /// スプラッシュ画像を埋め込み PNG からデコードし egui テクスチャとして登録
    fn load_splash_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
        static SPLASH_PNG: &[u8] = include_bytes!("../../../assets/popone_image.png");
        let image = image::load_from_memory(SPLASH_PNG).ok()?.into_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        let pixels = image.into_raw();
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
        Some(ctx.load_texture("splash", color_image, egui::TextureOptions::LINEAR))
    }

    /// v0 デザイン準拠のダークテーマを適用
    /// hex 文字列を Color32 に変換（デフォルト値つき）
    fn theme_color(opt: &Option<String>, default: egui::Color32) -> egui::Color32 {
        opt.as_ref()
            .and_then(|s| persistence::ThemeConfig::parse_hex(s))
            .map(|(r, g, b)| egui::Color32::from_rgb(r, g, b))
            .unwrap_or(default)
    }

    fn setup_dark_theme(ctx: &egui::Context, theme: &persistence::ThemeConfig) {
        let mut visuals = egui::Visuals::dark();

        let panel_bg = Self::theme_color(&theme.panel_bg, DARK_PANEL_BG);
        let border = Self::theme_color(&theme.border, DARK_BORDER_COLOR);
        let accent = Self::theme_color(&theme.accent, egui::Color32::from_rgb(0x4A, 0x90, 0xD9));
        let text_color = Self::theme_color(&theme.text, egui::Color32::from_gray(0xD0));
        let widget_bg =
            Self::theme_color(&theme.widget_bg, egui::Color32::from_rgb(0x25, 0x25, 0x25));
        let active_color =
            Self::theme_color(&theme.active, egui::Color32::from_rgb(0x2A, 0x5A, 0x8A));

        // パネル・ウィンドウ背景
        visuals.panel_fill = panel_bg;
        visuals.window_fill = panel_bg;

        // ボーダー
        let border_stroke = egui::Stroke::new(1.0, border);
        visuals.window_stroke = border_stroke;

        // ウィジェット共通テキスト色
        let fg = egui::Stroke::new(1.0, text_color);

        // noninteractive（ラベル・セパレータ等）
        visuals.widgets.noninteractive.bg_stroke = border_stroke;
        visuals.widgets.noninteractive.fg_stroke = fg;

        // inactive（ボタン通常時）
        visuals.widgets.inactive.bg_fill = widget_bg;
        visuals.widgets.inactive.weak_bg_fill = widget_bg;
        visuals.widgets.inactive.bg_stroke = border_stroke;
        visuals.widgets.inactive.fg_stroke = fg;

        // hovered（ホバー時）: アクセントカラー
        visuals.widgets.hovered.bg_fill = accent;
        visuals.widgets.hovered.weak_bg_fill = accent;
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, accent);
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);

        // active（クリック中）
        visuals.widgets.active.bg_fill = active_color;
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, active_color);
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);

        // open（展開中のComboBox等）
        visuals.widgets.open.bg_fill = egui::Color32::from_rgb(0x2A, 0x2A, 0x2A);
        visuals.widgets.open.bg_stroke = border_stroke;
        visuals.widgets.open.fg_stroke = fg;

        // セレクション/アクセント
        visuals.selection.bg_fill = accent;
        visuals.selection.stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);

        // 極端な背景色（TextEdit内部等）
        visuals.extreme_bg_color = egui::Color32::from_rgb(0x15, 0x15, 0x15);

        // スクロールバーを細く
        let mut spacing = ctx.style().spacing.clone();
        spacing.scroll.bar_width = 6.0;

        let mut style = (*ctx.style()).clone();
        style.visuals = visuals;
        style.spacing = spacing;
        ctx.set_style(style);
    }

    pub(crate) fn finish_load(
        &mut self,
        ir: IrModel,
        source: ReloadableSource,
    ) -> anyhow::Result<()> {
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;

        // GPU リソース構築（IrTexture から直接アップロード）
        // 初回ロード時は material_display 未初期化のためデフォルト値使用
        let display = if self.material_display.len() == ir.materials.len() {
            self.material_display.clone()
        } else {
            Self::default_material_display(&ir)
        };
        let mat_flags = Self::extract_per_mat_vecs(&display);
        let gpu_model = super::mesh::build_gpu_model_from_ir(&ir, device, queue, &mat_flags)?;
        self.finish_load_with_gpu(ir, gpu_model, source, false)
    }

    /// GPU テクスチャアップロードを分割して実行する版（BG パース完了後に使用）
    pub(crate) fn start_deferred_gpu_build(
        &mut self,
        ir: IrModel,
        source: ReloadableSource,
        post_kind: Option<pending::BgLoadKind>,
        path: std::path::PathBuf,
    ) {
        let display = if self.material_display.len() == ir.materials.len() {
            self.material_display.clone()
        } else {
            Self::default_material_display(&ir)
        };
        let mat_flags = Self::extract_per_mat_vecs(&display);

        // reload_snapshot が Some なら「reload_current 経由」であることをキャプチャし、
        // GPU ビルド分割を経て finish_load_with_gpu まで確実に運ぶ (review_004 [P2] 対応)。
        let is_reload = self.reload_snapshot.is_some();
        self.pending.gpu_build = Some(pending::PendingGpuBuild {
            gpu_textures: Vec::with_capacity(ir.textures.len()),
            next_tex: 0,
            mat_flags,
            post_kind,
            path,
            ir,
            source,
            append_info: None,
            cpu_prep_rx: None,
            is_reload,
        });
    }

    /// Append 操作の GPU ビルドを遅延実行する版。
    /// IR マージは即時実行し、マージ済み IR をフレーム分割テクスチャアップロードに回す。
    /// ビルド完了までは `self.loaded = None`（一時的にモデル非表示）。
    pub(crate) fn start_deferred_append_gpu_build(
        &mut self,
        other_ir: IrModel,
        append_source: helpers::ReloadableSource,
        silent: bool,
        pkg_model_name: Option<String>,
        pkg_locator: Option<crate::unitypackage::PkgModelLocator>,
        path: std::path::PathBuf,
    ) {
        self.start_deferred_append_gpu_build_ext(
            other_ir,
            append_source,
            silent,
            pkg_model_name,
            pkg_locator,
            path,
            None,
        );
    }

    /// Append 操作の GPU ビルドを遅延実行（PkgAppend ペイロード付き版）
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start_deferred_append_gpu_build_ext(
        &mut self,
        mut other_ir: IrModel,
        append_source: helpers::ReloadableSource,
        silent: bool,
        pkg_model_name: Option<String>,
        pkg_locator: Option<crate::unitypackage::PkgModelLocator>,
        path: std::path::PathBuf,
        pkg_append_payload: Option<Box<pending::PkgAppendPayload>>,
    ) {
        let Some(mut loaded) = self.loaded.take() else {
            return;
        };

        let added_name = other_ir.name.clone();
        let added_bones = other_ir.bones.len();
        let added_meshes = other_ir.meshes.len();
        let added_materials = other_ir.materials.len();
        let saved_material_count = loaded.ir.materials.len();
        let mat_offset = saved_material_count;
        let tex_count_before = loaded.ir.textures.len();

        let ir_snapshot = pending::IrRollbackSnapshot::capture(&loaded.ir);
        // ヒューマノイド補完（finish_append_ext と同じ処理）
        let other_has_humanoid = other_ir.bones.iter().any(|b| b.vrm_bone_name.is_some());
        if !other_has_humanoid {
            let names: Vec<(usize, &str)> = other_ir
                .bones
                .iter()
                .enumerate()
                .map(|(i, b)| (i, b.original_name.as_str()))
                .collect();
            let mapping = crate::fbx::humanoid::detect_humanoid(&names);
            for (&idx, hb) in &mapping.mapping {
                other_ir.bones[idx].vrm_bone_name = Some(hb.as_vrm_name().to_string());
            }
            if !mapping.mapping.is_empty() {
                log::info!(
                    "Pre-merge humanoid completion: {} bones detected",
                    mapping.mapping.len()
                );
            }
        }

        let t_merge = std::time::Instant::now();
        let (merged_bones, new_bones) = loaded.ir.merge(other_ir);
        log::info!("[gpu_build] IR merge done in {}ms (merged_bones={merged_bones}, new_bones={new_bones})", t_merge.elapsed().as_millis());

        // material_display を resize
        let mc = loaded.ir.materials.len();
        self.material_display
            .resize_with(mc, MaterialDisplayState::default);
        let mat_flags = Self::extract_per_mat_vecs(&self.material_display);

        let anim_snapshot = pending::AnimationSnapshot::capture(self.anim.state.as_ref());

        // loaded を分解: IR (マージ済み) + GPU モデル + 所有権フィールド
        let rollback_gpu_model = loaded.gpu_model;
        let ownership = pending::LoadedModelOwnership {
            source: loaded.source,
            primary_astance_result: loaded.primary_astance_result,
            appended_models: loaded.appended_models,
            material_groups: loaded.material_groups,
            pkg_material_keys: loaded.pkg_material_keys,
            prefab_name: loaded.prefab_name,
            prefab_entry_path: loaded.prefab_entry_path,
        };
        let merged_ir = loaded.ir;

        let append_info = pending::AppendGpuBuildInfo {
            rollback_gpu_model,
            ir_snapshot,
            ownership,
            append_source,
            added_name,
            added_bones,
            added_meshes,
            added_materials,
            saved_material_count,
            merged_bones,
            new_bones,
            pkg_model_name,
            pkg_locator,
            silent,
            pkg_append_payload,
            mat_offset,
            tex_count_before,
            source_path: path.clone(),
            anim_snapshot,
        };

        let tex_count = merged_ir.textures.len();
        self.pending.gpu_build = Some(pending::PendingGpuBuild {
            gpu_textures: Vec::with_capacity(tex_count),
            next_tex: 0,
            mat_flags,
            post_kind: None,
            path,
            ir: merged_ir,
            source: helpers::ReloadableSource::File(std::path::PathBuf::new()), // ダミー（append 完了時に primary_source を使用）
            append_info: Some(Box::new(append_info)),
            cpu_prep_rx: None,
            is_reload: false, // append は reload ではない
        });
    }

    /// Append GPU ビルド失敗時に元のモデルにロールバックする。
    /// マージ済み IR を truncate で元に戻し、旧 GPU モデルを復元する。
    pub(crate) fn rollback_append(&mut self, mut ir: IrModel, ai: pending::AppendGpuBuildInfo) {
        log::info!("Rolling back append: restoring original model via truncate");
        let mat_count = ai.ir_snapshot.material_count;
        ai.ir_snapshot.rollback(&mut ir);

        // LoadedModel を旧 GPU モデル + truncate 済み IR で再構築
        let mat_cache = Self::build_mat_cache(&ir, &ai.rollback_gpu_model);
        let stats_cache = CachedStats::new(&ir);
        let own = ai.ownership;
        self.loaded = Some(LoadedModel {
            ir,
            gpu_model: ai.rollback_gpu_model,
            source: own.source,
            primary_astance_result: own.primary_astance_result,
            appended_models: own.appended_models,
            material_groups: own.material_groups,
            pkg_material_keys: own.pkg_material_keys,
            prefab_name: own.prefab_name,
            prefab_entry_path: own.prefab_entry_path,
            mat_cache,
            stats_cache,
        });
        // material_display を元のサイズに戻す
        self.material_display.truncate(mat_count);
        // レンダラキャッシュ無効化
        if let Some(ref mut renderer) = self.renderer {
            renderer.invalidate_visualization_cache();
            renderer.invalidate_normal_cache();
        }
    }

    /// 遅延 GPU ビルド完了後の Append 後処理。
    /// `PendingGpuBuild` から取り出したマージ済み IR + 構築済み GPU モデルと、
    /// `AppendGpuBuildInfo` に保存しておいた旧 LoadedModel 情報から LoadedModel を再構築する。
    pub(crate) fn finish_deferred_append(
        &mut self,
        ir: IrModel,
        mut gpu_model: super::mesh::GpuModel,
        mut ai: pending::AppendGpuBuildInfo,
    ) {
        let _t = std::time::Instant::now();
        // レンダラー初期化（まだなければ）
        if self.renderer.is_none() {
            let device = &self.render_state.device;
            let queue = &self.render_state.queue;
            self.renderer = Some(super::gpu::GpuRenderer::new(
                device,
                queue,
                gpu_model.has_alpha,
            ));
        }
        log::info!(
            "[append_detail] renderer init: {}ms",
            _t.elapsed().as_millis()
        );

        // MMD リソース構築
        let t1 = std::time::Instant::now();
        let emissive_vec: Vec<bool> = self.material_display.iter().map(|d| d.emissive).collect();
        if let Some(ref renderer) = self.renderer {
            let device = &self.render_state.device;
            renderer.prepare_mmd_resources(device, &mut gpu_model, &ir, &emissive_vec);
        }
        log::info!(
            "[append_detail] prepare_mmd: {}ms",
            t1.elapsed().as_millis()
        );

        // viewport テクスチャ解放
        if let Some(tex_id) = self.viewport_texture_id.take() {
            let mut renderer = self.render_state.renderer.write();
            renderer.free_texture(&tex_id);
        }

        let new_draw_count = gpu_model.draws.len();
        self.material_visibility.resize(new_draw_count, true);
        let new_morph_count = ir.morphs.len();
        self.morph_weights.resize(new_morph_count, 0.0);
        self.morph_dirty = self.morph_weights.iter().any(|&w| w != 0.0);

        let t2 = std::time::Instant::now();
        let mat_cache = Self::build_mat_cache(&ir, &gpu_model);
        let stats_cache = CachedStats::new(&ir);
        log::info!(
            "[append_detail] mat_cache+stats: {}ms",
            t2.elapsed().as_millis()
        );

        // MaterialGroup: 旧グループ + 新規グループ
        let mut own = ai.ownership;
        let prev_draw_end: usize = own
            .material_groups
            .iter()
            .map(|g| g.draw_range.end)
            .max()
            .unwrap_or(0);
        own.material_groups.push(MaterialGroup {
            name: ai.added_name.clone(),
            material_range: ai.saved_material_count..ai.saved_material_count + ai.added_materials,
            draw_range: prev_draw_end..gpu_model.draws.len(),
        });

        // appended_models に追加
        let display_path = ai.append_source.display_path().to_path_buf();
        own.appended_models.push(AppendedModel {
            source: ai.append_source,
            pkg_model_name: ai.pkg_model_name,
            pkg_model: ai.pkg_locator,
        });

        // LoadedModel を再構築
        self.loaded = Some(LoadedModel {
            ir,
            gpu_model,
            source: own.source,
            primary_astance_result: own.primary_astance_result,
            appended_models: own.appended_models,
            material_groups: own.material_groups,
            mat_cache,
            stats_cache,
            pkg_material_keys: own.pkg_material_keys,
            prefab_name: own.prefab_name,
            prefab_entry_path: own.prefab_entry_path,
        });

        // last_dir 更新
        if let Some(dir) = display_path.parent() {
            self.tex.last_dir = Some(dir.to_path_buf());
        }

        // レンダラーのキャッシュ無効化
        let t3 = std::time::Instant::now();
        if let Some(ref mut renderer) = self.renderer {
            renderer.invalidate_visualization_cache();
            renderer.invalidate_normal_cache();
            renderer.mark_sort_dirty();
            // グリッドを更新
            if let Some(ref loaded) = self.loaded {
                let (bbox_min, bbox_max) = loaded.gpu_model.bbox();
                renderer.rebuild_grid(&self.render_state.device, bbox_min, bbox_max);
            }
        }
        log::info!(
            "[append_detail] renderer invalidate+grid: {}ms",
            t3.elapsed().as_millis()
        );

        // アニメーション状態を再構築
        let t4 = std::time::Instant::now();
        if let Some(anim_arc) = ai.anim_snapshot.animation_arc.take() {
            if let Some(ref loaded) = self.loaded {
                let mut new_state =
                    super::animation::AnimationState::new(anim_arc, &loaded.ir, &loaded.gpu_model);
                ai.anim_snapshot.apply_to(&mut new_state);
                self.anim.state = Some(new_state);
            }
        }
        log::info!(
            "[append_detail] animation rebuild: {}ms",
            t4.elapsed().as_millis()
        );

        // シェーダー状態を正規化
        let t5 = std::time::Instant::now();
        self.normalize_shader_state();
        log::info!(
            "[append_detail] normalize_shader: {}ms",
            t5.elapsed().as_millis()
        );

        log::info!(
            "Append loaded (deferred gpu): {} (bones:{} -> merged:{}/new:{}, meshes:{}, materials:{})",
            ai.added_name,
            ai.added_bones,
            ai.merged_bones,
            ai.new_bones,
            ai.added_meshes,
            ai.added_materials,
        );
        if !ai.silent {
            self.convert_message = Some(ConvertMessage::success(format!(
                "追加読み込み完了: {}\nボーン:{} (統合:{} + 新規:{}), メッシュ:{}, 材質:{}",
                ai.added_name,
                ai.added_bones,
                ai.merged_bones,
                ai.new_bones,
                ai.added_meshes,
                ai.added_materials,
            )));
        }

        // PkgAppend 後処理（テクスチャリネーム・テクスチャマッチング等）
        let t6 = std::time::Instant::now();
        if let Some(payload) = ai.pkg_append_payload {
            self.apply_pkg_append_post(
                *payload,
                ai.mat_offset,
                ai.tex_count_before,
                &ai.source_path,
            );
        }
        log::info!(
            "[append_detail] pkg_append_post: {}ms",
            t6.elapsed().as_millis()
        );
        log::info!("[append_detail] TOTAL: {}ms", _t.elapsed().as_millis());
    }

    /// PkgAppend 遅延 GPU ビルド完了後の後処理
    fn apply_pkg_append_post(
        &mut self,
        payload: pending::PkgAppendPayload,
        mat_offset: usize,
        tex_count_before: usize,
        source_path: &std::path::Path,
    ) {
        if payload.suppress_tex_match {
            self.suppress_tex_match = true;
        }

        // appended_models の最後のインデックスで pkg_prefix を構築
        let appended_count = self
            .loaded
            .as_ref()
            .map(|l| l.appended_models.len())
            .unwrap_or(0);

        if appended_count > 0 {
            let pkg_stem = source_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("pkg");
            let pkg_prefix = format!("{}_pkg{}", pkg_stem, appended_count);

            if let Some(ref mut loaded) = self.loaded {
                for tex in loaded.ir.textures[tex_count_before..].iter_mut() {
                    tex.filename = format!("{}_{}", pkg_prefix, tex.filename);
                }
            }

            let mut pkg_textures_to_add = payload.pkg_textures_to_add;
            if !pkg_textures_to_add.is_empty() {
                for (name, _) in &mut pkg_textures_to_add {
                    *name = format!("{}_{}", pkg_prefix, name);
                }
                let thumb_start = self.tex.pkg_thumb_cache.len();
                if let Some(ref mut existing) = self.tex.pkg_textures {
                    existing.extend(pkg_textures_to_add);
                } else {
                    self.tex.pkg_textures = Some(pkg_textures_to_add);
                }
                // 新規追加分のみサムネイル生成（全再構築を回避）
                self.append_pkg_thumb_cache(thumb_start);
            }
        }

        if !payload.pkg_unmatched.is_empty()
            && self.tex.pkg_textures.is_some()
            && !payload.suppress_tex_match
        {
            self.cancel_tex_match_preview();
            let global_unmatched: Vec<usize> = payload
                .pkg_unmatched
                .iter()
                .map(|&i| i + mat_offset)
                .collect();
            let count = global_unmatched.len();
            self.tex.pending_match = Some(texture_mgmt::PendingTexMatch {
                mat_indices: global_unmatched,
                selections: vec![None; count],
                tex_filter: String::new(),
                previewed: vec![None; count],
                saved_binds: std::collections::HashMap::new(),
                texture_views: Vec::new(),
                failed_uploads: std::collections::HashSet::new(),
            });
        }

        self.suppress_tex_match = false;

        // バッチ進捗トースト（成功メッセージを上書き）
        if let Some((current, total)) = payload.batch_progress {
            let name = payload.pkg_model_name.as_deref().unwrap_or("?");
            self.convert_message = Some(ConvertMessage::success(format!(
                "読み込み完了 ({}/{}): {}",
                current, total, name
            )));
        }
    }

    /// シェーダー状態を現在のモデルに合わせて正規化
    ///
    /// - Auto: モデル形式に応じて Standard/MMD を自動判定
    /// - Mtoon/Unlit/GGX/Normal/MMD: ユーザー選択を維持（整合性チェックのみ）
    pub(crate) fn normalize_shader_state(&mut self) {
        let has_mmd = self.loaded.as_ref().is_some_and(|l| {
            l.gpu_model
                .draws
                .iter()
                .any(|d| d.mmd_material_bind_group.is_some())
        });

        if self.display.auto_shader {
            // Auto モード: モデル形式に合わせて MMD パスを自動判定
            self.display.shader_override = ShaderOverride::Default;
            if !has_mmd {
                self.display.use_mmd_path = false;
            } else {
                let is_pmx_pmd = self.loaded.as_ref().is_some_and(|l| {
                    matches!(
                        l.ir.source_format,
                        crate::intermediate::types::SourceFormat::Pmx
                            | crate::intermediate::types::SourceFormat::Pmd
                    )
                });
                self.display.use_mmd_path = is_pmx_pmd;
            }
        } else {
            // ユーザー明示選択: MMD リソースがなければ use_mmd_path だけ落とす
            if !has_mmd && self.display.use_mmd_path {
                self.display.use_mmd_path = false;
            }
        }
    }

    pub(crate) fn finish_load_with_gpu(
        &mut self,
        ir: IrModel,
        mut gpu_model: super::mesh::GpuModel,
        source: ReloadableSource,
        is_reload: bool,
    ) -> anyhow::Result<()> {
        // 材質編集ドロワー（§A / P2 対応 / review_004 [P2] 完全対応）: 明示的 reload
        // （A スタンス変換・T スタンス変換・reload_current 経由）のときだけ前回の
        // 材質編集状態を保持し、それ以外（新規ロード・同じファイルを再オープン等）では
        // 破棄する。
        //
        // `is_reload` は BG パイプライン経由では `PendingGpuBuild.is_reload` から渡され、
        // 同期パス（直接 finish_load_with_gpu を呼ぶ経路）では `false` が渡される。
        // `PendingGpuBuild.is_reload` は `start_deferred_gpu_build` 時点の
        // `self.reload_snapshot.is_some()` をキャプチャしているため、GPU ビルド分割の
        // 数フレームにわたるタイミング問題を回避できる。
        log::debug!(
            "finish_load_with_gpu: is_reload={}, material_overrides={}",
            is_reload,
            self.material_overrides.len(),
        );
        if !is_reload {
            self.editing_material_index = None;
            self.material_dirty.clear();
            self.material_overrides.clear();
            self.slot_texture_paths.clear();
        }

        // レンダラー初期化（まだなければ）または可視化キャッシュ無効化
        if self.renderer.is_none() {
            let device = &self.render_state.device;
            let queue = &self.render_state.queue;
            self.renderer = Some(GpuRenderer::new(device, queue, gpu_model.has_alpha));
        } else if let Some(ref mut renderer) = self.renderer {
            renderer.invalidate_visualization_cache();
            renderer.mark_sort_dirty();
        }

        // 材質ごとのフラグを先に初期化（MMD リソース構築で使用するため）
        self.material_display = Self::default_material_display(&ir);

        // MMD リソース構築
        let emissive_vec: Vec<bool> = self.material_display.iter().map(|d| d.emissive).collect();
        self.prepare_mmd_for_model(&mut gpu_model, &ir, &emissive_vec);

        // テクスチャ割り当て履歴クリア（別モデル読み込み時）
        self.tex.assignments.clear();
        self.tex.pkg_assignments.clear();
        // 非同期テクスチャダイアログが開いていれば結果を破棄
        // (前モデルの material index が stale になるのを防ぐ)
        if self.tex.pending_file_dialog.is_some() {
            self.tex.pending_file_dialog = None;
        }
        // PSD→PNG バックグラウンド変換を破棄（前モデルの tex_idx が stale になるのを防ぐ）
        self.tex.pending_psd_conversions.clear();
        // v0.5.2 [review_01 P1] 対応: 前モデルのサムネイル TextureId を解放する。
        // これを呼ばないと、新旧モデルのテクスチャ数が一致した場合に
        // `sync_ir_thumb_cache()` が長さ比較で early return し、
        // 材質編集ウィンドウで前モデルのサムネイルが別モデルの画像として表示される。
        self.clear_ir_thumb_cache();
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
        // グリッドをモデルサイズに合わせて再構築
        if let Some(ref mut renderer) = self.renderer {
            renderer.rebuild_grid(&self.render_state.device, bbox_min, bbox_max);
            renderer.mark_sort_dirty();
        }
        // ビューポートサイズ確定後に refit（初回ロード時はサイズが未確定の場合がある）
        self.pending.refit = true;

        // デフォルト出力パス: converted_modelXX/ ディレクトリ内に .pmx
        // output_base_dir が設定されていればそちらを優先
        let path = source.display_path();
        let base_dir = self
            .export
            .output_base_dir
            .as_deref()
            .unwrap_or_else(|| path.parent().unwrap_or(std::path::Path::new(".")));
        let converted_dir = crate::next_converted_dir(base_dir);
        // モデル表示名の初期値（拡張子なし）:
        //   - 通常のファイル (FBX/VRM/PMX/...) → そのファイル名
        //   - アーカイブ (zip/7z/unitypackage) → アーカイブファイル名
        //     (ReloadableSource::Archive の display_path はアーカイブ本体を指す)
        //   - Prefab は後段（file_io.rs の PkgInitial/同期 Prefab ロード経路）で Prefab 名に上書き
        //   - 追加 (append) はこの関数を経由しないため自動的に維持される
        let initial_display_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(crate::sanitize_filename)
            .or_else(|| crate::sanitize_filename(&ir.name))
            .unwrap_or_else(|| "model".to_string());
        self.export.model_display_name = initial_display_name.clone();
        let pmx_name = format!("{initial_display_name}.pmx");
        self.export.pmx_output_path = converted_dir.join(&pmx_name).to_string_lossy().into_owned();

        // キャッシュ構築
        let mat_cache = Self::build_mat_cache(&ir, &gpu_model);
        let stats_cache = CachedStats::new(&ir);

        // テクスチャ割当ログ出力
        ir.log_texture_assignments();

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
            pkg_material_keys: Vec::new(),
            prefab_name: None,
            prefab_entry_path: None,
        });

        // 新規ロード時: サイドパネルを情報タブに戻す
        self.side_panel_tab = SidePanelTab::Info;
        // 新規ロード時: シェーダーを初期値にリセットしてからモデル形式に応じて正規化
        self.display.shader_override = ShaderOverride::Default;
        self.display.use_mmd_path = false;
        self.display.auto_shader = true;
        self.normalize_shader_state();

        // ウィンドウタイトル更新（model_display_name ベース）
        self.window_title = Some(format!(
            "POPONE Model Viewer v{} - {}",
            env!("CARGO_PKG_VERSION"),
            self.export.model_display_name,
        ));

        if let Some(ref mut renderer) = self.renderer {
            renderer.invalidate_normal_cache();
        }

        // 材質編集ドロワー（§H）: ロード直後の IR 材質値を pristine としてスナップショット。
        // 「初期値に戻す」で pristine に復元する経路が使う。
        // **重要**: override の apply_to 実行前にキャプチャしないと、apply 後の値が
        // pristine になってしまい「初期値」の意味をなさなくなる。
        if let Some(loaded) = self.loaded.as_ref() {
            self.pristine_materials = loaded.ir.materials.clone();
        }

        // 材質編集ドロワー（§A / A スタンス対応）: reload なら `material_overrides` を
        // 新 IR に一括再適用し、該当材質を dirty に立てて次フレームで bind group を再生成する。
        // Step 2 では §E の全セクション（影 / アウトライン / リム / MatCap / UV アニメ /
        // エミッシブ / 法線 / その他）のパラメータが `MaterialParamOverride` に集約されている
        // ので、この 1 経路で A スタンス / T スタンス変換を挟んだ編集値保持がすべて機能する。
        if is_reload && !self.material_overrides.is_empty() {
            // `self.material_overrides` と `self.loaded` の同時借用を避けるため、先に適用対象の
            // (mat_idx, override_clone) リストを作ってから `loaded` を mut で借用する。
            let override_list: Vec<(usize, material_edit::MaterialParamOverride)> = self
                .material_overrides
                .iter()
                .map(|(&i, o)| (i, o.clone()))
                .collect();
            if let Some(loaded) = self.loaded.as_mut() {
                for (mat_idx, override_val) in override_list {
                    if let Some(mat) = loaded.ir.materials.get_mut(mat_idx) {
                        override_val.apply_to(mat);
                        if self.material_dirty.len() <= mat_idx {
                            self.material_dirty.resize(mat_idx + 1, false);
                        }
                        self.material_dirty[mat_idx] = true;
                    }
                }
            }
        }

        // Step 4-16b / review_016 対応: 非 BaseColor テクスチャスロット割当の reload 復元。
        // slot_texture_paths に記録されたファイルパスを再読込し、assign_texture_core で
        // GPU にアップロード + IrMaterial のスロットフィールドに設定する。
        if is_reload && !self.slot_texture_paths.is_empty() {
            let paths: Vec<(
                (usize, crate::intermediate::types::TextureSlot),
                std::path::PathBuf,
            )> = self
                .slot_texture_paths
                .iter()
                .map(|(k, v)| (*k, v.clone()))
                .collect();
            for ((mat_idx, slot), path) in paths {
                if let Ok(data) = std::fs::read(&path) {
                    let ext = crate::path_ext_lower(&path);
                    let is_psd = ext == "psd";
                    let name = path.to_string_lossy().to_string();
                    self.assign_texture_core(mat_idx, slot, &data, is_psd, &name);
                } else {
                    log::warn!(
                        "Slot texture reload failed: mat[{}] {:?} <- {}",
                        mat_idx,
                        slot,
                        path.display()
                    );
                }
            }
        }

        Ok(())
    }

    /// MMD リソースを GpuModel に構築
    fn prepare_mmd_for_model(
        &self,
        gpu_model: &mut super::mesh::GpuModel,
        ir: &crate::intermediate::types::IrModel,
        emissive_per_mat: &[bool],
    ) {
        if let Some(ref renderer) = self.renderer {
            let device = &self.render_state.device;
            renderer.prepare_mmd_resources(device, gpu_model, ir, emissive_per_mat);
        }
    }

    /// smooth_normals 切り替え時に GPU モデルを再構築
    pub fn rebuild_gpu_model(&mut self) {
        let Some(loaded) = &self.loaded else { return };
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;

        let mat_flags = Self::extract_per_mat_vecs(&self.material_display);
        match super::mesh::build_gpu_model_from_ir(&loaded.ir, device, queue, &mat_flags) {
            Ok(mut new_model) => {
                // MMD リソース構築
                if let Some(ref renderer) = self.renderer {
                    renderer.prepare_mmd_resources(
                        device,
                        &mut new_model,
                        &loaded.ir,
                        &mat_flags.emissive,
                    );
                }
                let mat_cache = Self::build_mat_cache(&loaded.ir, &new_model);
                // draw数が同じなら材質表示状態を保持
                if self.material_visibility.len() != new_model.draws.len() {
                    self.material_visibility = vec![true; new_model.draws.len()];
                }
                if let Some(loaded) = &mut self.loaded {
                    loaded.gpu_model = new_model;
                    loaded.mat_cache = mat_cache;
                }
                self.normalize_shader_state();
                if let Some(ref mut renderer) = self.renderer {
                    renderer.invalidate_normal_cache();
                    renderer.mark_sort_dirty();
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
                log::info!("GPU model rebuilt (per-material normals)");
            }
            Err(e) => log::error!("GPU model rebuild failed: {}", e),
        }
    }

    /// ロード済みモデルの bbox でカメラをリセットする
    fn camera_reset_to_model(&mut self) {
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

    /// ロード済みモデルの bbox にカメラをフィットさせる
    fn camera_fit_to_model(&mut self) {
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

    /// 材質ごとのデフォルト表示状態を生成（HDR emissive はエミッシブ OFF）
    fn default_material_display(ir: &IrModel) -> Vec<MaterialDisplayState> {
        // HDR エミッシブ材質（emissive_factor のいずれか成分が 1.0 超）は
        // デフォルトで emission OFF にする。
        // シェーダーは `lit = lighting + rim + emissive` と加算するため、
        // 係数 > 1.0 だと表面全体に flat な明るさが加わり、例えば lilToon
        // Screen ブレンドを 0.5 減衰した後でも辛うじて > 1.0 になる材質
        // （Shinano_face の attenuated [0.89, 0.96, 1.06] など）がテクスチャを
        // 覆い隠して白飛びする。ユーザーが手動で ON にすれば従来通り有効化できる。
        // v0.2.40 で一度削除したが、HDR 材質のビューア表示不具合を招いたため復活。
        ir.materials
            .iter()
            .map(|m| {
                let ef = m.emissive_factor;
                let emissive = !(ef.x > 1.0 || ef.y > 1.0 || ef.z > 1.0);
                MaterialDisplayState {
                    emissive,
                    ..Default::default()
                }
            })
            .collect()
    }

    /// `material_display` から `MaterialBuildFlags` を展開する
    fn extract_per_mat_vecs(display: &[MaterialDisplayState]) -> super::mesh::MaterialBuildFlags {
        super::mesh::MaterialBuildFlags {
            smooth: display.iter().map(|d| d.smooth_normals).collect(),
            clear: display.iter().map(|d| d.clear_normals).collect(),
            normal_map: display.iter().map(|d| d.normal_map).collect(),
            emissive: display.iter().map(|d| d.emissive).collect(),
        }
    }

    /// `material_display` が材質数と一致すればそのまま展開し、不一致ならデフォルト値で生成
    fn per_mat_or_default_display(
        display: &[MaterialDisplayState],
        mat_count: usize,
    ) -> super::mesh::MaterialBuildFlags {
        if display.len() == mat_count {
            Self::extract_per_mat_vecs(display)
        } else {
            super::mesh::MaterialBuildFlags::default_for(mat_count)
        }
    }

    /// 材質編集ドロワー（§C / §E-1）: 指定材質を「次フレームで bind group 再生成」対象に追加する。
    ///
    /// ロード経路（material_display の再構築箇所）で `material_dirty` を逐一リサイズする代わりに、
    /// このヘルパが必要に応じて `Vec<bool>` を拡張する。古いモデルで立ったフラグが残る懸念は
    /// `apply_pending_material_rebuilds` 側で `ir.materials.len()` にクランプして処理するため、
    /// 安全側に倒している。
    pub fn mark_material_dirty(&mut self, mat_idx: usize) {
        let needed = mat_idx + 1;
        if self.material_dirty.len() < needed {
            self.material_dirty.resize(needed, false);
        }
        self.material_dirty[mat_idx] = true;
    }

    /// 材質編集ドロワー（§C）: 立った `material_dirty` を拾って rebuild_material_bind_groups を
    /// 呼び出し、標準パスと MMD 互換パスの両方で bind group を再生成する。
    ///
    /// `update()` 内で UI 描画後・wgpu 描画前に呼び出され、1 フレームで dirty が全消化される。
    fn apply_pending_material_rebuilds(&mut self) {
        if !self.material_dirty.iter().any(|&d| d) {
            return;
        }
        let Some(renderer) = self.renderer.as_ref() else {
            // renderer 未初期化（スプラッシュ中等）では dirty を握りつぶす
            self.material_dirty.fill(false);
            return;
        };
        let Some(loaded) = self.loaded.as_mut() else {
            self.material_dirty.fill(false);
            return;
        };

        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        let mat_count = loaded.ir.materials.len();
        let flags = Self::per_mat_or_default_display(&self.material_display, mat_count);
        // 古いモデルの dirty が残っている可能性があるので ir.materials.len() にクランプ
        let dirty_len = self.material_dirty.len().min(mat_count);

        for mat_idx in 0..dirty_len {
            if self.material_dirty[mat_idx] {
                // テクスチャ変更を伴うかどうかで uniform_only を判定
                // material_dirty はテクスチャ変更時にも立つため、常に full rebuild を行う
                renderer.rebuild_material_bind_groups(
                    device,
                    queue,
                    &mut loaded.gpu_model,
                    &loaded.ir,
                    mat_idx,
                    &flags,
                    false, // uniform_only: テクスチャ変更の可能性があるため full rebuild
                );

                // v0.5.1 レビュー [P2] 対応: 材質エディタ編集値が Expression の新しいベース値になる仕様を実装。
                //
                // 旧実装は material_base_values をモデルロード時に一度だけキャプチャしており、
                // 材質エディタで diffuse / emissive / shade / rim / matcap / UV を編集後に
                // Expression を再生すると、合成基準が「編集後」ではなく「ロード時」のままだった。
                // dirty 時に `MaterialBaseValues::from_ir()` で再キャプチャして最新値を反映する。
                if let Some(mat) = loaded.ir.materials.get(mat_idx) {
                    if mat_idx < loaded.gpu_model.material_base_values.len() {
                        loaded.gpu_model.material_base_values[mat_idx] =
                            crate::viewer::mesh::MaterialBaseValues::from_ir(mat);
                    }
                }
            }
        }
        // すべての dirty を消化（リサイズが古くてクランプ外でも全消去）
        self.material_dirty.fill(false);

        // v0.5.1 レビュー 02 [P1] 対応: 材質編集の rebuild 後に Expression 材質バインドを
        // 再適用する。旧実装では dirty 対象材質の uniform は base 値のみ書き込まれており、
        // 手動モーフで非ゼロの Expression が保持されているケースでは「編集した瞬間に
        // Expression の材質反映が消える」不具合があった（次フレームの update_animation で
        // 上書きされるが、再生停止中・手動モーフ単独時は復帰しない）。
        //
        // accumulate_expression_materials は Material morph が参照する全材質を dirty 扱いに
        // するため、編集対象外の材質でも Expression 影響下なら正しく再適用される。
        if self.morph_weights.iter().any(|w| w.abs() > 1e-6) {
            let mat_count = loaded.ir.materials.len();
            let dirty_params = crate::viewer::mesh::accumulate_expression_materials(
                &loaded.gpu_model.gpu_morphs,
                &self.morph_weights,
                &loaded.gpu_model.material_base_values,
                &loaded.ir.materials,
                mat_count,
                &flags,
            );
            for (mat_idx, params) in dirty_params.iter().enumerate() {
                if let Some(p) = params {
                    for draw in &loaded.gpu_model.draws {
                        if draw.material_index == mat_idx {
                            crate::viewer::gpu::write_material_buffer(queue, &draw.material_buf, p);
                        }
                    }
                }
            }
        }
    }

    /// VRM の IrTexture（raw ピクセル）を PNG エンコード済みに変換
    pub(crate) fn encode_ir_textures_as_png(ir: &mut IrModel, images: &[gltf::image::Data]) {
        use crate::intermediate::types::TextureData;
        use image::ImageEncoder;
        for (i, tex) in ir.textures.iter_mut().enumerate() {
            if let Some(img_data) = images.get(i) {
                let (w, h) = (img_data.width, img_data.height);
                let bytes = tex.data.as_bytes();
                // RGBA 画像を構築
                let rgba_img: Option<image::RgbaImage> = if bytes.len() == (w * h * 4) as usize {
                    image::ImageBuffer::from_raw(w, h, bytes.to_vec())
                } else if bytes.len() == (w * h * 3) as usize {
                    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
                    for chunk in bytes.chunks(3) {
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
                        tex.data = TextureData::Encoded(Arc::from(png_data));
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

    /// PSD データを PNG に変換（crate::psd::psd_to_png に委譲）
    pub(crate) fn psd_to_png(psd_data: &[u8]) -> anyhow::Result<Vec<u8>> {
        crate::psd::psd_to_png(psd_data)
    }

    /// ViewportInfo からウィンドウ状態をキャッシュし、初回フレームで位置を検証・適用
    fn update_viewport_config(&mut self, ctx: &egui::Context) {
        let mut restore_pos: Option<egui::Pos2> = None;

        ctx.input(|i| {
            let vp = i.viewport();
            let maximized = vp.maximized.unwrap_or(false);
            let minimized = vp.minimized.unwrap_or(false);

            // 初回フレーム: 保���された位置を無条件で復元
            // egui の monitor_size は「今ウィンドウがいるモニター」のサイズしか返さないため、
            // サブディスプレイへの復元判���には使えない。
            // サイズが正の値であれば常に復元を試みる。
            if !self.pending_window_restore.validated {
                if let Some(ref saved) = self.pending_window_restore.saved_config {
                    if saved.width >= 10.0 && saved.height >= 10.0 {
                        restore_pos = Some(egui::pos2(saved.x, saved.y));
                        log::info!("Window position restored: ({}, {})", saved.x, saved.y);
                    }
                }
                self.pending_window_restore.validated = true;
            }

            // 最大化・最小化中は位置・サイズを更新しない
            if maximized || minimized {
                return;
            }

            // 位置: outer_rect（OuterPosition との座標系一致）
            // サイズ: inner_rect（with_inner_size との座標系一致、ドリフト防止）
            if let (Some(outer), Some(inner)) = (vp.outer_rect, vp.inner_rect) {
                let win = self
                    .app_config
                    .window
                    .get_or_insert_with(persistence::WindowConfig::default);
                if win.is_significantly_different(
                    outer.min.x,
                    outer.min.y,
                    inner.width(),
                    inner.height(),
                ) {
                    win.x = outer.min.x;
                    win.y = outer.min.y;
                    win.width = inner.width();
                    win.height = inner.height();
                    self.config_dirty = true;
                }
            }
        });

        // ctx.input() の外で viewport コマンドを送信
        if let Some(pos) = restore_pos {
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
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
                anim.apply_bone_animation(
                    &mut loaded.gpu_model,
                    queue,
                    &self.morph_weights,
                    &loaded.ir,
                );

                // Expression 材質バインドの GPU 反映
                let mat_count = loaded.ir.materials.len();
                let flags = Self::per_mat_or_default_display(&self.material_display, mat_count);
                let dirty_params = crate::viewer::mesh::accumulate_expression_materials(
                    &loaded.gpu_model.gpu_morphs,
                    &self.morph_weights,
                    &loaded.gpu_model.material_base_values,
                    &loaded.ir.materials,
                    mat_count,
                    &flags,
                );
                for (mat_idx, params) in dirty_params.iter().enumerate() {
                    if let Some(p) = params {
                        for draw in &loaded.gpu_model.draws {
                            if draw.material_index == mat_idx {
                                crate::viewer::gpu::write_material_buffer(
                                    queue,
                                    &draw.material_buf,
                                    p,
                                );
                            }
                        }
                    }
                }

                self.morph_dirty = false;
            }
        }
    }
}

impl eframe::App for ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ウォッチドッグ: 最小化中は update() の呼び出しが保証されないため pause し、
        // 通常時は tick で応答性を記録する。idle 時も 3 秒間隔で repaint を予約して
        // ハートビートを維持する（フリーズ時はスレッド自体がブロックされ実行されない）。
        if ctx.input(|i| i.viewport().minimized == Some(true)) {
            self.heartbeat.pause();
        } else {
            self.heartbeat.tick();
        }
        ctx.request_repaint_after(Duration::from_secs(3));

        // ダークテーマ: new() での設定が eframe の初期化で上書きされる場合があるため
        // update() 初回で再適用する（以降はフラグで1回のみ）
        if !self.dark_theme_applied {
            Self::setup_dark_theme(ctx, &self.app_config.theme);
            self.dark_theme_applied = true;
        }

        // IPC: 別プロセスからのファイルパス受信
        #[cfg(target_os = "windows")]
        while let Ok(path) = self.ipc_receiver.try_recv() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            if !path.as_os_str().is_empty() {
                self.pending
                    .bg_state
                    .submit_dispatch(pending::PendingLoadDispatch {
                        path,
                        append: false,
                        overlay: pending::PendingOverlay::WaitingOverlay,
                        preloaded: None,
                        is_reload: false,
                    });
            }
        }

        // セッション設定: ViewportInfo キャッシュ + 初回位置検証
        self.update_viewport_config(ctx);

        // ホバー状態リセット（UIフレーム中に再設定される）
        self.hovered_draw_indices.clear();

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
                    .duration_since(*self.frame_times.front().expect("len >= 2 確認済み"))
                    .as_secs_f32();
                if span > 0.0 {
                    self.fps_display = (self.frame_times.len() - 1) as f32 / span;
                    self.frame_dt_ms = span / (self.frame_times.len() - 1) as f32 * 1000.0;
                }
            }
            self.fps_last_update = now;
        }

        // GPU ウォームアップ: スプラッシュ表示中にパイプラインを段階的に事前生成
        if self.loaded.is_none()
            && self.pending.gpu_build.is_none()
            && !matches!(self.warmup_phase, WarmupPhase::Complete)
        {
            self.tick_gpu_warmup();
            ctx.request_repaint();
        }

        self.process_pending_tasks(ctx);
        self.update_animation(dt, ctx);
        let (is_hover_image, is_hover_model) = self.process_drag_and_drop(ctx);
        self.process_keyboard_shortcuts(ctx);

        // ダークテーマ: パネル背景を明示的に設定（テーマ自体は new() で1回だけ設定済み）
        let dark_panel = self.theme_panel_bg;
        let dark_border = egui::Stroke::new(1.0, self.theme_border);
        let panel_frame = egui::Frame::new()
            .fill(dark_panel)
            .stroke(dark_border)
            .inner_margin(egui::Margin::same(4));

        // トップバー
        egui::TopBottomPanel::top("top_bar")
            .frame(panel_frame)
            .show(ctx, |bar| {
                bar.horizontal(|ui| {
                    // トップバーボタン: 通常時は透明背景、ホバー時はグローバルテーマのブルーが効く
                    let border33 = self.theme_border;
                    ui.visuals_mut().widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
                    ui.visuals_mut().widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
                    ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::new(1.0, border33);

                    // ダークテーマ用メニューボタンヘルパー（.fill() を使わずビジュアルに任せる）
                    let menu_btn = |ui: &mut egui::Ui, label: &str| -> egui::Response {
                        let btn = egui::Button::new(
                            egui::RichText::new(label)
                                .color(egui::Color32::WHITE)
                                .size(12.0),
                        );
                        ui.add(btn)
                    };

                    if menu_btn(ui, "開く").clicked() {
                        self.open_file_dialog(ctx);
                    }

                    if self.loaded.is_some()
                        && menu_btn(ui, "追加")
                            .on_hover_text("モデルを追加読み込み（Shift+D&Dでも可）")
                            .clicked()
                    {
                        self.open_append_dialog(ctx);
                    }

                    if menu_btn(ui, "ログ")
                        .on_hover_text("ログビュアーを別ウインドウで開く / 閉じる")
                        .clicked()
                    {
                        // toggle_visible は閉じる際に last_geometry を apply_geometry に
                        // スナップショットするので、再度開いたとき同じ位置で開く
                        let mut m = self.log_viewer.lock().unwrap_or_else(|p| p.into_inner());
                        m.toggle_visible();
                    }

                    // モデル名（編集可能）。タイトルバー表示 + PMX 出力ファイル名の両方に反映。
                    // 右側パネルの「モデル名:」TextEdit と同じ値を共有する。
                    if self.loaded.is_some() {
                        ui.separator();
                        ui.label(
                            egui::RichText::new("モデル名:")
                                .color(egui::Color32::from_gray(0xB0))
                                .size(11.0),
                        );
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.export.model_display_name)
                                .desired_width(240.0)
                                .text_color(egui::Color32::WHITE)
                                .font(egui::FontId::proportional(12.0))
                                .hint_text("(拡張子なし)"),
                        );
                        if response.changed() {
                            self.refresh_derived_from_display_name();
                        }
                    }

                    // 右端にフィット/リセットボタン
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if menu_btn(ui, "リセット(R)")
                            .on_hover_text("カメラをリセット")
                            .clicked()
                        {
                            self.camera_reset_to_model();
                        }
                        if menu_btn(ui, "フィット(F)")
                            .on_hover_text("モデルにフィット")
                            .clicked()
                        {
                            self.camera_fit_to_model();
                        }
                    });
                });
            });

        // 右側パネル
        ui::show_side_panel(ctx, self);

        // 材質編集による dirty フラグを消化（§C）:
        // UI 操作で立った material_dirty を見て rebuild_material_bind_groups を呼び、
        // 同フレーム内に標準パスと MMD 互換パスの bind group を両方更新する。
        // ※ 材質編集パネル本体は status_bar/shortcut_hints の追加後に呼ぶことで
        //    積み上げ順「最下=status_bar / 中=shortcut_hints / 上=編集パネル」を確保する。
        self.apply_pending_material_rebuilds();

        // テクスチャD&Dダイアログ + プレビュー同期
        ui::show_texture_drop_dialog(ctx, self);
        self.sync_tex_preview();

        // ステータスバー: ファイルパス + 統計
        egui::TopBottomPanel::bottom("status_bar")
            .frame(panel_frame)
            .show(ctx, |ui| {
                ui.visuals_mut().override_text_color = Some(egui::Color32::WHITE);
                ui.horizontal_centered(|ui| {
                    if let Some(ref loaded) = self.loaded {
                        let ir = &loaded.ir;
                        let font = egui::FontId::proportional(10.0);

                        let path_label = if loaded.source.is_snapshot() {
                            format!(
                                "{} (キャッシュ済み)",
                                loaded.source.display_path().to_string_lossy()
                            )
                        } else {
                            loaded.source.display_path().to_string_lossy().into_owned()
                        };
                        ui.label(egui::RichText::new(path_label).font(font.clone()));

                        ui.separator();

                        ui.label(
                            egui::RichText::new(&loaded.stats_cache.status_text).font(font.clone()),
                        );

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
                                .font(egui::FontId::proportional(11.0)),
                        );
                    }
                });
            });

        // ショートカットヒントバー（ステータスバーの上）
        egui::TopBottomPanel::bottom("shortcut_hints")
            .frame(panel_frame)
            .show(ctx, |ui| {
                let hint_color = egui::Color32::WHITE;
                let hint_font = egui::FontId::proportional(10.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(
                            "左ドラッグ:回転  右ドラッグ:パン  ホイール:ズーム  ダブルクリック:フィット",
                        )
                        .font(hint_font.clone())
                        .color(hint_color),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("G:グリッド B:ボーン P:物理 W:ワイヤー N:法線 L:ライト")
                                .font(hint_font)
                                .color(hint_color),
                        );
                    });
                });
            });

        // 材質編集パネル（v0.5.3）: ショートカットヒントバーの直上に固定表示。
        // editing_material_index が Some のときのみ TopBottomPanel が出現し、
        // [編] アイコンOFFまたは [×] でパネル自体が消える（中央ビューポートが拡張される）。
        ui::show_material_editor_window(ctx, self);

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
                    self.camera_fit_to_model();
                }

                // モーフウェイト変更検知 → 頂点バッファ更新
                if self.morph_dirty {
                    if let Some(ref mut loaded) = self.loaded {
                        let queue = &self.render_state.queue;
                        loaded.gpu_model.apply_morphs(&self.morph_weights, queue);

                        // Expression 材質バインド: スライダー操作時も反映
                        let mat_count = loaded.ir.materials.len();
                        let flags =
                            Self::per_mat_or_default_display(&self.material_display, mat_count);
                        let dirty_params = crate::viewer::mesh::accumulate_expression_materials(
                            &loaded.gpu_model.gpu_morphs,
                            &self.morph_weights,
                            &loaded.gpu_model.material_base_values,
                            &loaded.ir.materials,
                            mat_count,
                            &flags,
                        );
                        for (mat_idx, params) in dirty_params.iter().enumerate() {
                            if let Some(p) = params {
                                for draw in &loaded.gpu_model.draws {
                                    if draw.material_index == mat_idx {
                                        crate::viewer::gpu::write_material_buffer(
                                            queue,
                                            &draw.material_buf,
                                            p,
                                        );
                                    }
                                }
                            }
                        }

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

                            let animated_globals =
                                self.anim.state.as_ref().map(|anim| anim.animated_globals());
                            let is_vrm0 = loaded.ir.source_format.is_vrm0();

                            let render_params = RenderParams {
                                camera: &self.camera,
                                width,
                                height,
                                material_visibility: &self.material_visibility,
                                display: &self.display,
                                animated_bone_globals: animated_globals,
                                is_vrm0,
                                time: self.start_time.elapsed().as_secs_f32(),
                                hovered_draw_indices: &self.hovered_draw_indices,
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

                // スプラッシュ画像（モデル未ロード時にビューポート中央に角丸で表示）
                if self.loaded.is_none() {
                    if let Some(ref tex) = self.splash_texture {
                        let tex_size = tex.size_vec2();
                        let rect = response.rect;
                        // ビューポートに収まるようスケール
                        let scale = (rect.width() / tex_size.x)
                            .min(rect.height() / tex_size.y)
                            .min(1.0);
                        let img_size = egui::vec2(tex_size.x * scale, tex_size.y * scale);
                        let img_rect = egui::Rect::from_center_size(rect.center(), img_size);
                        let image =
                            egui::Image::new(egui::load::SizedTexture::new(tex.id(), tex_size))
                                .corner_radius(egui::CornerRadius::same(16));
                        viewport.put(img_rect, image);
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
                    viewport.painter().rect_filled(rect, 0.0, overlay_color);
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
                    self.camera_reset_to_model();
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
                }

                // FPS表示（右上オーバーレイ）
                {
                    let rect = response.rect;
                    let fps_text =
                        format!("{:.0} fps  {:.1} ms", self.fps_display, self.frame_dt_ms);
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
                        let label =
                            if loaded.ir.source_format.is_pmx_pmd() || self.normalize_to_tstance {
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
                // 未読込時のみビューポートにヒント表示（読込後はステータスバーに集約）
                if self.loaded.is_none() {
                    let rect = response.rect;
                    viewport.painter().text(
                        egui::pos2(rect.left() + 8.0, rect.bottom() - 8.0),
                        egui::Align2::LEFT_BOTTOM,
                        "Ctrl+O:開く  ドラッグ&ドロップ:VRM/FBXファイル読込",
                        egui::FontId::proportional(12.0),
                        egui::Color32::from_gray(0xC0),
                    );
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
                            ConvertResult::Success(m) => (
                                m.as_str(),
                                egui::Color32::from_rgba_unmultiplied(
                                    0x30,
                                    0xC0,
                                    0x30,
                                    (alpha * 255.0) as u8,
                                ),
                            ),
                            ConvertResult::Warning(m) => (
                                m.as_str(),
                                egui::Color32::from_rgba_unmultiplied(
                                    0xE0,
                                    0x40,
                                    0x40,
                                    (alpha * 255.0) as u8,
                                ),
                            ),
                            ConvertResult::Failure(m) => (
                                m.as_str(),
                                egui::Color32::from_rgba_unmultiplied(
                                    0xE0,
                                    0x40,
                                    0x40,
                                    (alpha * 255.0) as u8,
                                ),
                            ),
                        };
                        let rect = response.rect;
                        // 背景帯
                        let text_galley = viewport.painter().layout_no_wrap(
                            msg.to_string(),
                            egui::FontId::proportional(14.0),
                            color,
                        );
                        let text_h =
                            text_galley.size().y * (msg.lines().count().max(1) as f32) + 16.0;
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
                if self
                    .convert_message
                    .as_ref()
                    .is_some_and(|cm| cm.elapsed_secs() >= 5.0)
                {
                    self.convert_message = None;
                }
            });

        // ログビュアーウインドウ（OS 別ウインドウ）を描画
        // visible == false なら何もしない
        self.show_log_viewer(ctx);
    }

    fn on_exit(&mut self) {
        // 通常終了時はログバッファをファイルに書き出さない。
        // 「パニックログ以外は保存しない」方針のため。panic 時は main.rs の
        // panic フックが `flush_log_buffer` 経由で `panic_*.log` を独立に生成する。
        // ユーザが明示的に保存したい場合はログビュアー内の「ログ保存」ボタンを使う。

        // ログビュアーの表示状態・位置・サイズ・フィルタを config に反映
        {
            let m = self.log_viewer.lock().unwrap_or_else(|p| p.into_inner());
            self.app_config.log_viewer = m.export_config();
        }

        // ディレクトリパスを config に反映
        self.app_config.directory.last_model = self
            .last_model_dir
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned());
        self.app_config.directory.last_texture = self
            .tex
            .last_dir
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned());
        persistence::save_config(&self.data_dir, &self.app_config);
    }
}
