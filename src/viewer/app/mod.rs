//! ビューアのメイン状態管理（ViewerApp 構造体定義、eframe::App impl）

pub mod file_io;
pub mod helpers;
pub mod pending;
pub mod texture_mgmt;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;
use eframe::egui_wgpu;

use crate::intermediate::types::IrModel;
use crate::unitypackage::PkgModelLocator;

use super::animation::AnimationState;
use super::camera::OrbitCamera;
use super::gpu::{DrawMode, GpuRenderer, LightMode, RenderParams, ShaderOverride, ShaderSelection};
use super::mesh::GpuModel;
use super::ui;

// サブモジュールから再エクスポート
pub use helpers::{FbxLoadMode, PkgModelType, PreloadedData, ReloadableSource, TextureSource};
pub use pending::{
    ExportState, PendingArchive, PendingArchiveLoad, PendingFbxChoice, PendingFbxChoicePkg,
    PendingOverlay, PendingPkgModelLoad, PendingState, PendingUnityPackage,
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
    /// 材質ごとの表示ON/OFF
    pub material_visibility: Vec<bool>,
    /// 材質ごとの法線平滑化 ON/OFF（mat_idx でインデックス）
    pub smooth_normals_per_mat: Vec<bool>,
    /// 材質ごとのカスタム法線クリア ON/OFF（mat_idx でインデックス）
    pub clear_normals_per_mat: Vec<bool>,
    /// 材質フィルター文字列
    pub material_filter: String,
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
}

impl ViewerApp {
    pub fn new(cc: &eframe::CreationContext, logs_dir: PathBuf, log_path: PathBuf) -> Self {
        let render_state = cc
            .wgpu_render_state
            .clone()
            .expect("wgpu render state required");

        // 日本語フォント読み込み
        Self::setup_japanese_font(&cc.egui_ctx);

        // ダークテーマ（Blender/Substance Painter 風）
        Self::setup_dark_theme(&cc.egui_ctx);

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
            smooth_normals_per_mat: Vec::new(),
            clear_normals_per_mat: Vec::new(),
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
            selected_pkg_model: None,
            anim: AnimLibrary::default(),
            side_panel_tab: SidePanelTab::Info,
            window_title: None,
            suppress_tex_match: false,
            hovered_draw_indices: Vec::new(),
            preloaded: None,
            start_time: Instant::now(),
            next_instance_id: 1,
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

    fn setup_japanese_font(ctx: &egui::Context) {
        // Noto Sans JP（OFL ライセンス）をバイナリに組み込み
        const NOTO_SANS_JP: &[u8] = include_bytes!("../../../assets/NotoSansJP-Regular.ttf");

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

    /// v0 デザイン準拠のダークテーマを適用
    fn setup_dark_theme(ctx: &egui::Context) {
        let mut visuals = egui::Visuals::dark();

        // パネル・ウィンドウ背景: #1D1D1D
        let panel_bg = egui::Color32::from_rgb(0x1D, 0x1D, 0x1D);
        visuals.panel_fill = panel_bg;
        visuals.window_fill = panel_bg;

        // ボーダー: #333333
        let border = egui::Color32::from_rgb(0x33, 0x33, 0x33);
        let border_stroke = egui::Stroke::new(1.0, border);
        visuals.window_stroke = border_stroke;

        // アクセントカラー
        let accent = egui::Color32::from_rgb(0x4A, 0x90, 0xD9);

        // ウィジェット共通テキスト色: #D0D0D0
        let fg = egui::Stroke::new(1.0, egui::Color32::from_gray(0xD0));

        // noninteractive（ラベル・セパレータ等）
        visuals.widgets.noninteractive.bg_stroke = border_stroke;
        visuals.widgets.noninteractive.fg_stroke = fg;

        // inactive（ボタン通常時）
        let widget_bg = egui::Color32::from_rgb(0x25, 0x25, 0x25);
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
        visuals.widgets.active.bg_fill = egui::Color32::from_rgb(0x2A, 0x5A, 0x8A);
        visuals.widgets.active.bg_stroke =
            egui::Stroke::new(1.0, egui::Color32::from_rgb(0x2A, 0x5A, 0x8A));
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
        // 初回ロード時は per_mat 未初期化のため空スライス（全OFF扱い）
        let smooth_per_mat: Vec<bool>;
        let clear_per_mat: Vec<bool>;
        if self.smooth_normals_per_mat.len() == ir.materials.len() {
            smooth_per_mat = self.smooth_normals_per_mat.clone();
            clear_per_mat = self.clear_normals_per_mat.clone();
        } else {
            smooth_per_mat = vec![false; ir.materials.len()];
            clear_per_mat = vec![false; ir.materials.len()];
        }
        let gpu_model = super::mesh::build_gpu_model_from_ir(
            &ir,
            device,
            queue,
            &smooth_per_mat,
            &clear_per_mat,
        )?;
        self.finish_load_with_gpu(ir, gpu_model, source)
    }

    /// シェーダー状態を現在のモデルに合わせて正規化
    ///
    /// - Auto: モデル形式に応じて Standard/MMD を自動判定
    /// - Mtoon/Unlit/GGX/Normal/MMD: ユーザー選択を維持（整合性チェックのみ）
    fn normalize_shader_state(&mut self) {
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
        self.smooth_normals_per_mat = vec![false; ir.materials.len()];
        self.clear_normals_per_mat = vec![false; ir.materials.len()];
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

        // デフォルト出力パス: converted_modelXX/ ディレクトリ内に .pmx
        // output_base_dir が設定されていればそちらを優先
        let path = source.display_path();
        let base_dir = self
            .export
            .output_base_dir
            .as_deref()
            .unwrap_or_else(|| path.parent().unwrap_or(std::path::Path::new(".")));
        let converted_dir = crate::next_converted_dir(base_dir);
        let pmx_stem = crate::sanitize_filename(&ir.name).unwrap_or_else(|| {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
        let pmx_name = format!("{}.pmx", pmx_stem);
        self.export.pmx_output_path = converted_dir.join(&pmx_name).to_string_lossy().into_owned();

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
            pkg_material_keys: Vec::new(),
            prefab_name: None,
        });

        // シェーダー状態を正規化（PMX/PMD → 自動 MMD、VRM → 標準パスに戻す）
        self.normalize_shader_state();

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

    /// smooth_normals 切り替え時に GPU モデルを再構築
    pub fn rebuild_gpu_model(&mut self) {
        let Some(loaded) = &self.loaded else { return };
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;

        match super::mesh::build_gpu_model_from_ir(
            &loaded.ir,
            device,
            queue,
            &self.smooth_normals_per_mat,
            &self.clear_normals_per_mat,
        ) {
            Ok(mut new_model) => {
                // MMD リソース構築
                if let Some(ref renderer) = self.renderer {
                    renderer.prepare_mmd_resources(device, &mut new_model, &loaded.ir);
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
                log::info!("GPU モデル再構築完了 (per-material normals)");
            }
            Err(e) => log::error!("GPU モデル再構築失敗: {}", e),
        }
    }

    /// per_mat フラグが材質数と一致すればそのまま返し、不一致なら全 false で生成
    fn per_mat_or_default(flags: &[bool], mat_count: usize) -> Vec<bool> {
        if flags.len() == mat_count {
            flags.to_vec()
        } else {
            vec![false; mat_count]
        }
    }

    /// VRM の IrTexture（raw ピクセル）を PNG エンコード済みに変換
    pub(crate) fn encode_ir_textures_as_png(ir: &mut IrModel, images: &[gltf::image::Data]) {
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

    /// PSD データを PNG に変換（crate::psd::psd_to_png に委譲）
    pub(crate) fn psd_to_png(psd_data: &[u8]) -> anyhow::Result<Vec<u8>> {
        crate::psd::psd_to_png(psd_data)
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

        // ダークテーマ: 毎フレームビジュアルを適用（ツールチップ・ポップアップ含む全UIに反映）
        Self::setup_dark_theme(ctx);

        // ダークテーマ: パネル背景を明示的に設定
        let dark_panel = egui::Color32::from_rgb(0x1D, 0x1D, 0x1D);
        let dark_border = egui::Stroke::new(1.0, egui::Color32::from_rgb(0x33, 0x33, 0x33));
        let panel_frame = egui::Frame::new()
            .fill(dark_panel)
            .stroke(dark_border)
            .inner_margin(egui::Margin::same(4));

        // トップバー
        egui::TopBottomPanel::top("top_bar")
            .frame(panel_frame.clone())
            .show(ctx, |bar| {
                bar.horizontal(|ui| {
                    // トップバーボタン: 通常時は透明背景、ホバー時はグローバルテーマのブルーが効く
                    let border33 = egui::Color32::from_rgb(0x33, 0x33, 0x33);
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
                        self.open_file_dialog();
                    }

                    if self.loaded.is_some() {
                        if menu_btn(ui, "追加")
                            .on_hover_text("モデルを追加読み込み（Shift+D&Dでも可）")
                            .clicked()
                        {
                            self.open_append_dialog();
                        }
                    }

                    if menu_btn(ui, "ログ").clicked() {
                        helpers::open_directory(&self.logs_dir);
                    }

                    if let Some(ref loaded) = self.loaded {
                        ui.separator();
                        ui.label(
                            egui::RichText::new(&loaded.ir.name)
                                .color(egui::Color32::WHITE)
                                .size(11.0),
                        );
                    }

                    // 右端にフィット/リセットボタン
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if menu_btn(ui, "リセット(R)")
                            .on_hover_text("カメラをリセット")
                            .clicked()
                        {
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
                        if menu_btn(ui, "フィット(F)")
                            .on_hover_text("モデルにフィット")
                            .clicked()
                        {
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
                    });
                });
            });

        // 右側パネル
        ui::show_side_panel(ctx, self);

        // テクスチャD&Dダイアログ + プレビュー同期
        ui::show_texture_drop_dialog(ctx, self);
        self.sync_tex_preview();

        // ステータスバー: ファイルパス + 統計
        egui::TopBottomPanel::bottom("status_bar")
            .frame(panel_frame.clone())
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
            .frame(panel_frame.clone())
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
                        self.camera.fit_to_bbox_with_margin(
                            bbox_min,
                            bbox_max,
                            self.last_viewport_width,
                            self.last_viewport_height,
                        );
                    }
                }

                // モーフウェイト変更検知 → 頂点バッファ更新
                if self.morph_dirty {
                    if let Some(ref mut loaded) = self.loaded {
                        let queue = &self.render_state.queue;
                        loaded.gpu_model.apply_morphs(&self.morph_weights, queue);
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
    }
}
