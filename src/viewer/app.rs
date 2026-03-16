use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

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

/// D&D 対応画像拡張子
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "tga", "bmp", "psd"];

/// D&D 対応モデル/アニメーション拡張子
const MODEL_EXTENSIONS: &[&str] = &["vrm", "fbx", "pmx", "pmd", "unitypackage", "vrma", "glb", "gltf", "anim"];

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
}

/// ステータスバー用キャッシュ
pub struct CachedStats {
    pub total_vertices: usize,
    pub total_faces: usize,
}

/// VRM読み込み結果
/// 追加読み込みされたモデルの情報（リロード時に再マージ用）
#[derive(Clone)]
pub struct AppendedModel {
    pub path: PathBuf,
    /// unitypackage内の選択モデル名（FBX/VRM直接の場合はNone）
    pub pkg_model_name: Option<String>,
}

pub struct LoadedModel {
    pub ir: IrModel,
    pub gpu_model: GpuModel,
    pub file_path: PathBuf,
    /// 追加読み込みされたモデル一覧（リロード時に再マージ用）
    pub appended_paths: Vec<AppendedModel>,
    /// モデル別の材質区間: (モデル名, DrawCall開始Index, DrawCall数)
    pub material_groups: Vec<(String, usize, usize)>,
    /// 材質情報キャッシュ（テクスチャ割り当て時に更新）
    pub mat_cache: CachedMaterialInfo,
    /// 統計キャッシュ
    pub stats_cache: CachedStats,
}

/// 変換結果の種類
pub enum ConvertResult {
    Success(String),
    Failure(String),
}

/// 表示時刻付き変換結果メッセージ
pub struct ConvertMessage {
    pub result: ConvertResult,
    pub shown_at: std::time::Instant,
}

impl ConvertMessage {
    pub fn new(result: ConvertResult) -> Self {
        Self { result, shown_at: std::time::Instant::now() }
    }

    pub fn success(msg: impl Into<String>) -> Self {
        Self::new(ConvertResult::Success(msg.into()))
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

pub struct PendingUnityPackage {
    pub assets: Vec<crate::unitypackage::ExtractedAsset>,
    /// (アセットIndex, ファイル名, モデル種別)
    pub model_list: Vec<(usize, String, PkgModelType)>,
    pub source_path: PathBuf,
    /// アペンドモード（既存モデルに追加）
    pub append: bool,
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
}

/// FBX読み込み方法選択ダイアログの状態（モデル+アニメーション両方含むFBX用）
pub struct PendingFbxChoice {
    pub path: PathBuf,
    pub load_model: bool,
    pub load_animation: bool,
    /// unitypackage 経由の場合のデータ
    pub pkg_context: Option<PendingFbxChoicePkg>,
}

/// unitypackage 経由 FBX 選択時の追加コンテキスト
pub struct PendingFbxChoicePkg {
    pub assets: Vec<crate::unitypackage::ExtractedAsset>,
    pub fbx_index: usize,
    pub source_path: PathBuf,
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
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            light_intensity: 0.7,
            ambient_intensity: 0.45,
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
        }
    }
}

/// テクスチャ割り当て・パッケージテクスチャ関連の状態
pub struct TextureState {
    /// 手動テクスチャ割り当て履歴（材質Index → ファイルパス）
    pub assignments: HashMap<usize, PathBuf>,
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
    pub library: Vec<(String, PathBuf, Arc<crate::intermediate::animation::VrmaAnimation>)>,
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
    /// PMX変換時にログファイルを出力するか
    pub output_log: bool,
    /// PMX出力パス（テキストボックス編集用）
    pub pmx_output_path: String,
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
    /// ビューポートの高さ（フィット計算用）
    pub last_viewport_height: f32,
    /// テクスチャ割り当て状態
    pub tex: TextureState,
    /// FBX読み込み方法選択待ち（モデル+アニメ両方含む場合）
    pub pending_fbx_choice: Option<PendingFbxChoice>,
    /// unitypackage FBX選択待ち
    pub pending_unity_pkg: Option<PendingUnityPackage>,
    /// FBX遅延読み込み
    pub pending_pkg_load: Option<PendingPkgModelLoad>,
    /// ファイル読み込み遅延実行 (path, overlay表示済みフラグ)
    pub pending_load: Option<(PathBuf, bool)>,
    /// モデル追加読み込み遅延実行 (path, overlay表示済みフラグ)
    pub pending_append: Option<(PathBuf, bool)>,
    /// PMX変換遅延実行
    pub pending_convert: Option<PendingOverlay>,
    /// GPU再構築遅延実行
    pub pending_rebuild: Option<PendingOverlay>,
    /// モデル再読み込み遅延実行
    pub pending_reload: Option<PendingOverlay>,
    /// FPS計測用
    last_frame_time: Instant,
    fps_smoothed: f32,
    /// ログディレクトリパス
    pub logs_dir: PathBuf,
    /// 現在のログファイルパス
    pub log_path: PathBuf,
    /// UVマップ出力解像度
    pub uv_map_size: u32,
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
}

impl ViewerApp {
    pub fn new(cc: &eframe::CreationContext, logs_dir: PathBuf, log_path: PathBuf) -> Self {
        let render_state = cc
            .wgpu_render_state
            .clone()
            .expect("wgpu render state required");

        // 日本語フォント読み込み
        Self::setup_japanese_font(&cc.egui_ctx);

        Self {
            loaded: None,
            camera: OrbitCamera::default(),
            renderer: None,
            convert_message: None,
            morph_weights: Vec::new(),
            morph_dirty: false,
            display: DisplaySettings::default(),
            material_visibility: Vec::new(),
            material_filter: String::new(),
            output_log: false,
            pmx_output_path: String::new(),
            drag_hovering: false,
            viewport_texture_id: None,
            render_state,
            normalize_pose: false,
            last_viewport_height: 720.0,
            tex: TextureState::default(),
            pending_fbx_choice: None,
            pending_unity_pkg: None,
            pending_pkg_load: None,
            pending_load: None,
            pending_append: None,
            pending_convert: None,
            pending_rebuild: None,
            pending_reload: None,
            last_frame_time: Instant::now(),
            fps_smoothed: 0.0,
            logs_dir,
            log_path,
            uv_map_size: crate::convert::uvmap::DEFAULT_UV_SIZE,
            last_model_dir: None,
            selected_fbx_name: None,
            anim: AnimLibrary::default(),
            side_panel_tab: SidePanelTab::Info,
            window_title: None,
            suppress_tex_match: false,
        }
    }

    fn setup_japanese_font(ctx: &egui::Context) {
        // Noto Sans JP（OFL ライセンス）をバイナリに組み込み
        const NOTO_SANS_JP: &[u8] =
            include_bytes!("../../assets/NotoSansJP-Regular.ttf");

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
            let data = match std::fs::read(&path) {
                Ok(d) => d,
                Err(_) => { self.load_file_as_model(path); return; }
            };
            let has_mesh = crate::fbx::extract::fbx_has_mesh(&data);
            let has_anim = crate::fbx::animation::load_fbx_animation_from_data(&data)
                .is_ok_and(|a| !a.is_empty());

            if has_mesh && has_anim {
                // 両方含む → 選択ダイアログを表示
                self.pending_fbx_choice = Some(PendingFbxChoice {
                    path: path.clone(),
                    load_model: true,
                    load_animation: true,
                    pkg_context: None,
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
                    if let Ok(anims) = crate::fbx::animation::load_fbx_animation(&path) {
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
                     ファイルが破損していないか、対応形式（VRM/FBX/PMX/PMD）であるか確認してください。\n\
                     詳細: {e}"
                );
                self.convert_message = Some(ConvertMessage::failure(user_msg));
            }
        }
    }

    /// FBX読み込み方法選択ダイアログの結果を実行
    pub fn execute_fbx_choice(&mut self, choice: PendingFbxChoice) {
        let PendingFbxChoice { path, load_model, load_animation, pkg_context } = choice;

        let mode = match (load_model, load_animation) {
            (true, true) => FbxLoadMode::Both,
            (true, false) => FbxLoadMode::ModelOnly,
            (false, true) => FbxLoadMode::AnimationOnly,
            (false, false) => return,
        };

        if let Some(pkg) = pkg_context {
            // unitypackage 経由
            match self.load_fbx_from_assets(pkg.assets, pkg.fbx_index, &pkg.source_path, mode) {
                Ok(()) => {
                    log::info!("読み込み成功: {}", pkg.source_path.display());
                    self.convert_message = None;
                }
                Err(e) => {
                    log::error!("読み込み失敗: {e}");
                    self.convert_message = Some(ConvertMessage::failure(format!("ファイルを読み込めませんでした。\n詳細: {e}")));
                }
            }
        } else {
            // ファイル直接
            match mode {
                FbxLoadMode::Both | FbxLoadMode::ModelOnly => {
                    match self.try_load_fbx(&path) {
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
                            self.convert_message = Some(ConvertMessage::failure(format!("ファイルを読み込めませんでした。\n詳細: {e}")));
                        }
                    }
                }
                FbxLoadMode::AnimationOnly => {
                    self.try_load_fbx_animation(&path);
                }
            }
        }
    }

    fn try_load_fbx(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let data = std::fs::read(path)?;
        let ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
            &data, Some(path), self.normalize_pose,
        )?;
        self.finish_load(ir, path)
    }

    fn try_load_unitypackage(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let archive_data = std::fs::read(path)?;
        let assets = crate::unitypackage::extract_all_assets(&archive_data)?;

        // FBX と VRM を統合したモデルリストを構築
        let mut model_list: Vec<(usize, String, PkgModelType)> = Vec::new();
        for (idx, name) in crate::unitypackage::find_vrm_list(&assets) {
            model_list.push((idx, name, PkgModelType::Vrm));
        }
        for (idx, name) in crate::unitypackage::find_fbx_list(&assets) {
            model_list.push((idx, name, PkgModelType::Fbx));
        }

        if model_list.is_empty() {
            anyhow::bail!(".unitypackage 内に VRM / FBX ファイルが見つかりません");
        }

        if model_list.len() == 1 {
            // モデルが1つだけ → プログレス表示後に遅延ロード
            let (idx, _, model_type) = model_list[0];
            self.pending_pkg_load = Some(PendingPkgModelLoad {
                assets, fbx_index: idx, model_type, source_path: path.to_path_buf(), shown: false, append: false, suppress_tex_match: false,
            });
        } else {
            // 複数 → 選択ダイアログを表示
            log::info!(".unitypackage 内に {} 個のモデルが見つかりました:", model_list.len());
            for (_, name, mtype) in &model_list {
                log::info!("  {:?}: {}", mtype, name);
            }
            self.pending_unity_pkg = Some(PendingUnityPackage {
                assets,
                model_list,
                source_path: path.to_path_buf(),
                append: false,
            });
        }
        Ok(())
    }

    /// unitypackage をアペンドモードで読み込み
    fn try_load_unitypackage_for_append(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let archive_data = std::fs::read(path)?;
        let assets = crate::unitypackage::extract_all_assets(&archive_data)?;

        let mut model_list: Vec<(usize, String, PkgModelType)> = Vec::new();
        for (idx, name) in crate::unitypackage::find_vrm_list(&assets) {
            model_list.push((idx, name, PkgModelType::Vrm));
        }
        for (idx, name) in crate::unitypackage::find_fbx_list(&assets) {
            model_list.push((idx, name, PkgModelType::Fbx));
        }

        if model_list.is_empty() {
            anyhow::bail!(".unitypackage 内に VRM / FBX ファイルが見つかりません");
        }

        if model_list.len() == 1 {
            let (idx, _, model_type) = model_list[0];
            self.pending_pkg_load = Some(PendingPkgModelLoad {
                assets, fbx_index: idx, model_type, source_path: path.to_path_buf(), shown: false, append: true, suppress_tex_match: self.suppress_tex_match,
            });
        } else {
            self.pending_unity_pkg = Some(PendingUnityPackage {
                assets,
                model_list,
                source_path: path.to_path_buf(),
                append: true,
            });
        }
        Ok(())
    }

    /// リロード時の unitypackage 同期アペンド（遅延処理を避け、テクスチャ復元も行う）
    /// リロード時の unitypackage 同期アペンド（遅延処理を避け、テクスチャ復元も行う）
    fn reload_append_unitypackage(
        &mut self,
        path: &std::path::Path,
        pkg_model_name: Option<&str>,
        saved_pkg_tex_assignments: &HashMap<usize, String>,
    ) {
        let archive_data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                log::error!("unitypackage 再読み込み失敗: {e}");
                return;
            }
        };
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
        let mat_offset = self.loaded.as_ref()
            .map(|l| l.ir.materials.len())
            .unwrap_or(0);

        // 同期的にアペンド
        self.append_from_pkg(assets, model_index, model_type, path);

        // 追加材質に対するpkgテクスチャ割り当てを復元
        if !saved_pkg_tex_assignments.is_empty() {
            // 割り当て対象を先に収集（借用解放のため）
            let assignments_to_restore: Vec<(usize, String, Vec<u8>)> = {
                let pkg_src = self.tex.pkg_textures.as_deref().unwrap_or(&[]);
                let name_to_data: HashMap<&str, &[u8]> = pkg_src.iter()
                    .map(|(name, data)| (name.as_str(), data.as_slice()))
                    .collect();
                let mat_count = self.loaded.as_ref().map(|l| l.ir.materials.len()).unwrap_or(0);
                saved_pkg_tex_assignments.iter()
                    .filter(|(idx, _)| **idx >= mat_offset && **idx < mat_count)
                    .filter_map(|(idx, tex_name)| {
                        name_to_data.get(tex_name.as_str())
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
    ) -> anyhow::Result<()> {
        let (fbx_data, fbx_name, textures) =
            crate::unitypackage::take_fbx_and_textures(assets, fbx_index)?;
        log::info!("unitypackage内FBX: {} テクスチャ: {}個", fbx_name, textures.len());
        self.selected_fbx_name = Some(fbx_name.clone());

        let load_model = matches!(mode, FbxLoadMode::ModelOnly | FbxLoadMode::Both);
        let load_animation = matches!(mode, FbxLoadMode::AnimationOnly | FbxLoadMode::Both);

        if load_model {
            let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                &fbx_data, Some(source_path), self.normalize_pose,
            )?;

            let unmatched = crate::unitypackage::embed_textures_into_ir(&mut ir, &textures);

            // テクスチャをアプリ状態に保持
            if !textures.is_empty() {
                self.tex.pkg_textures = Some(textures);
                self.rebuild_pkg_thumb_cache();
            }

            self.finish_load(ir, source_path)?;

            // モデル読み込み時はアニメーションをクリア
            self.anim.state = None;
            self.anim.library.clear();
            self.anim.active_index = None;

            // 未割当材質がある場合、手動割当ダイアログを開く（リロード中は抑制）
            if !unmatched.is_empty() && self.tex.pkg_textures.is_some() && !self.suppress_tex_match {
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
                            std::sync::Arc::clone(&anim), &loaded.ir, &loaded.gpu_model,
                        );
                        self.anim.library.push((display_name, source_path.to_path_buf(), anim));
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
    ) -> anyhow::Result<()> {
        let (vrm_data, vrm_name) = crate::unitypackage::take_vrm(assets, vrm_index)?;
        log::info!("unitypackage内VRM: {} ({}KB)", vrm_name, vrm_data.len() / 1024);
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
            &ir, &glb.images, device, queue,
            self.display.smooth_normals, self.display.clear_custom_normals,
        )?;

        Self::encode_ir_textures_as_png(&mut ir, &glb.images);
        self.finish_load_with_gpu(ir, gpu_model, source_path)
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
                let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                let loaded = self.loaded.as_ref().expect("loaded は is_some 分岐内");
                let state = AnimationState::new(Arc::clone(&anim), &loaded.ir, &loaded.gpu_model);
                log::info!("VRMA読み込み成功: {}", path.display());

                // ライブラリに追加（重複パスは上書き）
                let path_buf = path.to_path_buf();
                if let Some(idx) = self.anim.library.iter().position(|(_, p, _)| p == &path_buf) {
                    self.anim.library[idx] = (name.clone(), path_buf, anim);
                    self.anim.active_index = Some(idx);
                } else {
                    self.anim.library.push((name.clone(), path_buf, anim));
                    self.anim.active_index = Some(self.anim.library.len() - 1);
                }

                self.anim.state = Some(state);
                self.convert_message = Some(ConvertMessage::success(
                    format!("VRMA読み込み成功: {}", name),
                ));
            }
            Err(e) => {
                log::error!("VRMA読み込み失敗: {e}");
                self.convert_message = Some(ConvertMessage::failure(
                    format!("VRMA読み込み失敗: {e}"),
                ));
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

        match crate::fbx::animation::load_fbx_animation(path) {
            Ok(anims) => {
                let loaded = self.loaded.as_ref().expect("loaded は is_some 分岐内");
                let path_buf = path.to_path_buf();
                let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

                for anim in anims {
                    let display_name = if anim.name == "animation" {
                        file_name.clone()
                    } else {
                        format!("{} ({})", file_name, anim.name)
                    };
                    let anim = Arc::new(anim);
                    let state = AnimationState::new(Arc::clone(&anim), &loaded.ir, &loaded.gpu_model);

                    // ライブラリに追加
                    self.anim.library.push((display_name.clone(), path_buf.clone(), anim));
                    self.anim.active_index = Some(self.anim.library.len() - 1);
                    self.anim.state = Some(state);
                }

                log::info!("FBXアニメーション読み込み成功: {}", path.display());
                self.convert_message = Some(ConvertMessage::success(
                    format!("FBXアニメーション読み込み成功: {}", file_name),
                ));
            }
            Err(e) => {
                log::error!("FBXアニメーション読み込み失敗: {e}");
                self.convert_message = Some(ConvertMessage::failure(
                    format!("FBXアニメーション読み込み失敗: {e}"),
                ));
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
                let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                let display_name = format!("{} ({})", file_name, anim.name);
                let anim = Arc::new(anim);
                let state = AnimationState::new(Arc::clone(&anim), &loaded.ir, &loaded.gpu_model);

                self.anim.library.push((display_name, path_buf, anim));
                self.anim.active_index = Some(self.anim.library.len() - 1);
                self.anim.state = Some(state);

                log::info!("Unity .anim読み込み成功: {}", path.display());
                self.convert_message = Some(ConvertMessage::success(
                    format!("Unity .anim読み込み成功: {}", file_name),
                ));
            }
            Err(e) => {
                log::error!("Unity .anim読み込み失敗: {e}");
                self.convert_message = Some(ConvertMessage::failure(
                    format!("Unity .anim読み込み失敗: {e}"),
                ));
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
                let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

                for anim in anims {
                    let display_name = if anim.name == "animation" {
                        file_name.clone()
                    } else {
                        format!("{} ({})", file_name, anim.name)
                    };
                    let anim = Arc::new(anim);
                    let state = AnimationState::new(Arc::clone(&anim), &loaded.ir, &loaded.gpu_model);

                    // ライブラリに追加
                    self.anim.library.push((display_name.clone(), path_buf.clone(), anim));
                    self.anim.active_index = Some(self.anim.library.len() - 1);
                    self.anim.state = Some(state);
                }

                log::info!("glTFアニメーション読み込み成功: {}", path.display());
                self.convert_message = Some(ConvertMessage::success(
                    format!("アニメーション読み込み成功: {}", file_name),
                ));
            }
            Err(e) => {
                log::error!("glTFアニメーション読み込み失敗: {e}");
                self.convert_message = Some(ConvertMessage::failure(
                    format!("アニメーション読み込み失敗: {e}"),
                ));
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
        let pmx_model = crate::pmx::reader::read_pmx(path)?;
        let pmx_dir = path.parent().unwrap_or(std::path::Path::new("."));
        let mut ir = crate::pmx::extract::pmx_to_ir(&pmx_model, pmx_dir)?;

        if self.normalize_pose {
            crate::intermediate::pose::normalize_pose_to_tstance_full(
                &mut ir.bones, &mut ir.meshes, &mut ir.morphs, &mut ir.physics,
                crate::convert::coord::gltf_pos_to_pmx,
            );
        }

        self.finish_load(ir, path)
    }

    fn try_load_pmd(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let pmd_model = crate::pmd::reader::read_pmd(path)?;
        let mut ir = crate::pmd::extract::pmd_to_ir(&pmd_model, path)?;

        if self.normalize_pose {
            crate::intermediate::pose::normalize_pose_to_tstance_full(
                &mut ir.bones, &mut ir.meshes, &mut ir.morphs, &mut ir.physics,
                crate::convert::coord::gltf_pos_to_pmx,
            );
        }

        self.finish_load(ir, path)
    }

    fn try_load_vrm(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
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
        let gpu_model = super::mesh::build_gpu_model(&ir, &glb.images, device, queue, self.display.smooth_normals, self.display.clear_custom_normals)?;

        // IrTexture を PNG エンコード済みに変換（convert_ir_to_pmx で統一的に使えるように）
        Self::encode_ir_textures_as_png(&mut ir, &glb.images);

        self.finish_load_with_gpu(ir, gpu_model, path)
    }

    fn finish_load(&mut self, ir: IrModel, path: &std::path::Path) -> anyhow::Result<()> {
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;

        // GPU リソース構築（IrTexture から直接アップロード）
        let gpu_model = super::mesh::build_gpu_model_from_ir(&ir, device, queue, self.display.smooth_normals, self.display.clear_custom_normals)?;
        self.finish_load_with_gpu(ir, gpu_model, path)
    }

    fn finish_load_with_gpu(&mut self, ir: IrModel, gpu_model: super::mesh::GpuModel, path: &std::path::Path) -> anyhow::Result<()> {
        // レンダラー初期化（まだなければ）または可視化キャッシュ無効化
        if self.renderer.is_none() {
            let device = &self.render_state.device;
            let queue = &self.render_state.queue;
            self.renderer = Some(GpuRenderer::new(device, queue, gpu_model.has_alpha));
        } else if let Some(ref mut renderer) = self.renderer {
            renderer.invalidate_visualization_cache();
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
        self.material_filter.clear();
        // カメラをモデルのバウンディングボックスにフィット
        let (bbox_min, bbox_max) = gpu_model.bbox();
        self.camera.reset_to_bbox_with_margin(bbox_min, bbox_max, self.last_viewport_height);

        // デフォルト出力パス: 入力VRMと同じ場所に .pmx
        self.pmx_output_path = path.with_extension("pmx").to_string_lossy().to_string();

        // キャッシュ構築
        let mat_cache = Self::build_mat_cache(&ir, &gpu_model);
        let stats_cache = CachedStats {
            total_vertices: ir.total_vertices(),
            total_faces: ir.total_faces(),
        };

        let format_name = ir.source_format.label().to_string();
        let model_name = ir.name.clone();
        let draw_count = gpu_model.draws.len();
        self.loaded = Some(LoadedModel {
            ir,
            gpu_model,
            file_path: path.to_path_buf(),
            appended_paths: Vec::new(),
            material_groups: vec![(model_name, 0, draw_count)],
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
    pub fn rebuild_gpu_model(&mut self) {
        let Some(loaded) = &self.loaded else { return };
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        let smooth = self.display.smooth_normals;
        let clear_normals = self.display.clear_custom_normals;

        match super::mesh::build_gpu_model_from_ir(&loaded.ir, device, queue, smooth, clear_normals) {
            Ok(new_model) => {
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
        let draw_indices: Vec<(usize, usize)> = gpu_model.draws.iter()
            .enumerate()
            .map(|(i, d)| (i, d.material_index))
            .collect();
        let names: Vec<String> = ir.materials.iter().map(|m| m.name.clone()).collect();
        let tex_indices: Vec<Option<usize>> = ir.materials.iter().map(|m| m.texture_index).collect();
        let source_tex_names: Vec<Option<String>> = ir.materials.iter()
            .map(|m| m.source_texture_name.clone()).collect();
        let tex_set_count = ir.materials.iter().filter(|m| m.texture_index.is_some()).count();
        CachedMaterialInfo { draw_indices, names, tex_indices, source_tex_names, tex_set_count }
    }

    /// 材質キャッシュを更新（テクスチャ割り当て後）
    fn update_mat_cache(&mut self) {
        if let Some(ref loaded) = self.loaded {
            let cache = Self::build_mat_cache(&loaded.ir, &loaded.gpu_model);
            // loaded を再借用して書き込み
            if let Some(ref mut loaded) = self.loaded {
                loaded.mat_cache = cache;
            }
        }
    }

    /// pkg_textures のサムネイルを GPU にアップロードしてキャッシュ
    pub fn rebuild_pkg_thumb_cache(&mut self) {
        self.clear_pkg_thumb_cache();
        let Some(ref pkg) = self.tex.pkg_textures else { return };
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        let mut renderer = self.render_state.renderer.write();
        const THUMB_SIZE: u32 = 64;

        for (name, data) in pkg.iter() {
            let is_psd = super::texture::is_psd_filename(name);
            match super::texture::create_thumbnail_rgba(data, is_psd, THUMB_SIZE) {
                Ok(rgba) => {
                    let view = super::texture::upload_rgba_to_gpu(
                        device, queue, &rgba, THUMB_SIZE, THUMB_SIZE, Some("pkg_thumb"),
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

    /// 指定材質に外部テクスチャファイルを割り当て
    pub fn assign_texture_to_material(&mut self, material_index: usize, path: &std::path::Path) {
        // ファイルを1回だけ読み込み
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

        let ext_lower = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let is_psd = ext_lower == "psd";

        let device = &self.render_state.device;
        let queue = &self.render_state.queue;

        // GPU テクスチャをアップロード（読み込み済みバイト列を使用）
        let texture_view = match super::texture::upload_texture_from_bytes(&tex_data, is_psd, device, queue) {
            Ok(view) => view,
            Err(e) => {
                log::error!("テクスチャ読み込み失敗: {e}");
                self.convert_message = Some(ConvertMessage::failure(format!(
                    "テクスチャ読み込み失敗: {e}"
                )));
                return;
            }
        };

        // IrModel にテクスチャを追加・材質を更新
        let Some(ref mut loaded) = self.loaded else { return };

        let basename = path.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // PSD の場合は PNG に変換して保存
        let (ir_data, ir_filename, ir_mime) = if is_psd {
            match Self::psd_to_png(&tex_data) {
                Ok(png_data) => (png_data, format!("{}.png", basename), "image/png".to_string()),
                Err(e) => {
                    log::error!("PSD→PNG変換失敗: {e}");
                    self.convert_message = Some(ConvertMessage::failure(format!(
                        "PSD→PNG変換失敗: {e}"
                    )));
                    return;
                }
            }
        } else {
            let filename = path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let mime = match ext_lower.as_str() {
                "png" => "image/png",
                "tga" => "image/x-tga",
                "bmp" => "image/bmp",
                _ => "image/jpeg",
            };
            (tex_data, filename, mime.to_string())
        };

        let tex_idx = loaded.ir.textures.len();
        loaded.ir.textures.push(crate::intermediate::types::IrTexture {
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
        loaded.gpu_model.assign_texture_to_material(material_index, &texture_view, device, texture_bgl, sampler);

        log::info!(
            "テクスチャ割り当て: 材質[{}] '{}' ← {}",
            material_index,
            loaded.ir.materials[material_index].name,
            path.display()
        );

        // 割り当て履歴を記録（reload_current 時の復元用）
        self.tex.assignments.insert(material_index, path.to_path_buf());

        // 同一材質名への連動割り当て
        if self.tex.link_same_name {
            let target_name = loaded.ir.materials[material_index].name.clone();
            let siblings: Vec<usize> = loaded.ir.materials.iter().enumerate()
                .filter(|(i, m)| *i != material_index && m.name == target_name)
                .map(|(i, _)| i)
                .collect();
            for sib_idx in siblings {
                loaded.ir.materials[sib_idx].texture_index = Some(tex_idx);
                loaded.ir.materials[sib_idx].apply_textured_defaults();
                loaded.gpu_model.assign_texture_to_material(sib_idx, &texture_view, device, texture_bgl, sampler);
                self.tex.assignments.insert(sib_idx, path.to_path_buf());
                log::info!("  連動割り当て: 材質[{}] '{}'", sib_idx, target_name);
            }
        }

        // 材質キャッシュ更新
        self.update_mat_cache();
    }

    /// パッケージ内テクスチャデータを材質に割り当て（バイト列から直接）
    pub fn assign_texture_data_to_material(&mut self, material_index: usize, tex_name: &str, tex_data: &[u8]) {
        let is_psd = super::texture::is_psd_filename(tex_name);

        let device = &self.render_state.device;
        let queue = &self.render_state.queue;

        let texture_view = match super::texture::upload_texture_from_bytes(tex_data, is_psd, device, queue) {
            Ok(view) => view,
            Err(e) => {
                log::error!("テクスチャデコード失敗: {e}");
                self.convert_message = Some(ConvertMessage::failure(format!(
                    "テクスチャデコード失敗: {e}"
                )));
                return;
            }
        };

        let Some(ref mut loaded) = self.loaded else { return };

        // IrModel にテクスチャを追加
        let basename = std::path::Path::new(tex_name)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let (ir_data, ir_filename, ir_mime) = if is_psd {
            match Self::psd_to_png(tex_data) {
                Ok(png_data) => (png_data, format!("{}.png", basename), "image/png".to_string()),
                Err(e) => {
                    log::warn!("PSD→PNG変換失敗 (IrTexture用): {e}");
                    (tex_data.to_vec(), tex_name.to_string(), String::new())
                }
            }
        } else {
            (tex_data.to_vec(), tex_name.to_string(), String::new())
        };

        let tex_idx = loaded.ir.textures.len();
        loaded.ir.textures.push(crate::intermediate::types::IrTexture {
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
        loaded.gpu_model.assign_texture_to_material(material_index, &texture_view, device, texture_bgl, sampler);

        log::info!("パッケージテクスチャ割り当て: 材質[{}] '{}' ← {}",
            material_index,
            loaded.ir.materials[material_index].name,
            tex_name,
        );

        // 同一材質名への連動割り当て
        if self.tex.link_same_name {
            let target_name = loaded.ir.materials[material_index].name.clone();
            let siblings: Vec<usize> = loaded.ir.materials.iter().enumerate()
                .filter(|(i, m)| *i != material_index && m.name == target_name)
                .map(|(i, _)| i)
                .collect();
            for sib_idx in siblings {
                loaded.ir.materials[sib_idx].texture_index = Some(tex_idx);
                loaded.ir.materials[sib_idx].apply_textured_defaults();
                loaded.gpu_model.assign_texture_to_material(sib_idx, &texture_view, device, texture_bgl, sampler);
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
                            tex.filename = tex.filename.replace(".jpg", ".png")
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
            encoder.write_image(&rgba, width, height, image::ExtendedColorType::Rgba8)
                .map_err(|e| anyhow::anyhow!("PNG エンコード失敗: {}", e))?;
        }
        Ok(png_data)
    }

    /// 現在読み込み中のVRMを再読み込みする（オプション変更時）
    /// カメラ・モーフ・材質表示などの状態は保持する
    pub fn reload_current(&mut self) {
        let Some(ref loaded) = self.loaded else { return };
        // リロード中はテクスチャ選択ダイアログを抑制
        self.suppress_tex_match = true;
        let path = loaded.file_path.clone();
        let saved_appended = loaded.appended_paths.clone();
        let saved_camera = self.camera.clone();
        let saved_morphs = self.morph_weights.clone();
        let saved_visibility = self.material_visibility.clone();
        let saved_filter = self.material_filter.clone();
        let saved_pmx_path = self.pmx_output_path.clone();
        let saved_tex_assignments = self.tex.assignments.clone();
        let saved_pkg_tex_assignments = self.tex.pkg_assignments.clone();
        let saved_pkg_textures = self.tex.pkg_textures.take();
        let saved_vrma_library = std::mem::take(&mut self.anim.library);
        let saved_vrma_index = self.anim.active_index.take();

        // unitypackage の場合は再展開せず FBX として再読み込み
        // （source_format が Fbx なら try_load_fbx を使う）
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let result = match ext.as_str() {
            "unitypackage" => self.reload_unitypackage(&path, &saved_pkg_textures, &saved_pkg_tex_assignments),
            _ => {
                self.load_file(path.clone());
                Ok(())
            }
        };

        // リロード時はテクスチャ選択ダイアログを抑制（後で割り当てを復元するため不要）
        self.tex.pending_match = None;

        // 追加モデルを再マージ（ベースモデルが正しく再ロードされた場合のみ）
        // 再ロード成功 = loaded の appended_paths が空（新規 LoadedModel が作られた）
        if let Some(ref loaded) = self.loaded {
            if loaded.appended_paths.is_empty() && !saved_appended.is_empty() {
                // リロード中フラグON（テクスチャ選択ダイアログ抑制）
                self.suppress_tex_match = true;
                for appended in &saved_appended {
                    let ext = appended.path.extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    if ext == "unitypackage" {
                        // unitypackage は同期的にアペンド（遅延処理を避ける）
                        self.reload_append_unitypackage(&appended.path, appended.pkg_model_name.as_deref(), &saved_pkg_tex_assignments);
                    } else {
                        self.append_model(appended.path.clone());
                    }
                }
                self.suppress_tex_match = false;
                // リロード経由の再アペンドではテクスチャ選択ダイアログを抑制
                self.tex.pending_match = None;
                // アペンド失敗のエラーメッセージは保持、成功メッセージのみクリア
                if let Some(ref msg) = self.convert_message {
                    if matches!(msg.result, ConvertResult::Success(_)) {
                        self.convert_message = None;
                    }
                }
            }
        }

        if let Err(e) = result {
            log::error!("再読み込み失敗: {e}");
            self.convert_message = Some(ConvertMessage::failure(
                format!("再読み込み失敗: {e}")
            ));
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
        self.pmx_output_path = saved_pmx_path;

        // テクスチャ割り当てを復元（ファイルパス分のみ。pkg分はreload_unitypackage内で処理済み）
        let saved_link = self.tex.link_same_name;
        self.tex.link_same_name = false;
        self.tex.assignments = HashMap::new();
        let current_mat_count = self.loaded.as_ref().map(|l| l.ir.materials.len()).unwrap_or(0);
        for (mat_idx, tex_path) in &saved_tex_assignments {
            if *mat_idx < current_mat_count {
                self.assign_texture_to_material(*mat_idx, tex_path);
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

    /// unitypackage 再読み込み（FBX/VRM再展開 + テクスチャ復元）
    fn reload_unitypackage(
        &mut self,
        path: &std::path::Path,
        saved_pkg_textures: &Option<Vec<(String, Vec<u8>)>>,
        saved_pkg_tex_assignments: &HashMap<usize, String>,
    ) -> anyhow::Result<()> {
        let archive_data = std::fs::read(path)?;
        let assets = crate::unitypackage::extract_all_assets(&archive_data)?;

        // 現在のモデルが VRM の場合は VRM として再読み込み
        let is_vrm = self.loaded.as_ref().is_some_and(|l| {
            !matches!(l.ir.source_format, crate::intermediate::types::SourceFormat::Fbx)
        });

        if is_vrm {
            let vrm_list = crate::unitypackage::find_vrm_list(&assets);
            if vrm_list.is_empty() {
                anyhow::bail!(".unitypackage 内に VRM ファイルが見つかりません");
            }
            let vrm_idx = if let Some(ref prev_name) = self.selected_fbx_name {
                vrm_list.iter()
                    .find(|(_, name)| name == prev_name)
                    .map(|(idx, _)| *idx)
                    .unwrap_or(vrm_list[0].0)
            } else {
                vrm_list[0].0
            };
            return self.load_vrm_from_assets(assets, vrm_idx, path);
        }

        let fbx_list = crate::unitypackage::find_fbx_list(&assets);
        if fbx_list.is_empty() {
            anyhow::bail!(".unitypackage 内に FBX ファイルが見つかりません");
        }

        // 前回と同じ FBX を選択（ファイル名で照合、見つからなければ最初のもの）
        let fbx_idx = if let Some(ref prev_name) = self.selected_fbx_name {
            fbx_list.iter()
                .find(|(_, name)| name == prev_name)
                .map(|(idx, _)| *idx)
                .unwrap_or(fbx_list[0].0)
        } else {
            fbx_list[0].0
        };

        let (fbx_data, fbx_name, textures) =
            crate::unitypackage::take_fbx_and_textures(assets, fbx_idx)?;
        log::info!("unitypackage再読み込み: {} テクスチャ: {}個", fbx_name, textures.len());

        let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
            &fbx_data, Some(path), self.normalize_pose,
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
        let pkg_src = if !textures.is_empty() { &textures } else {
            saved_pkg_textures.as_deref().unwrap_or(&[])
        };
        if !saved_pkg_tex_assignments.is_empty() && !pkg_src.is_empty() {
            // テクスチャ名 → pkgデータの逆引きマップ
            let name_to_data: HashMap<&str, &[u8]> = pkg_src.iter()
                .map(|(name, data)| (name.as_str(), data.as_slice()))
                .collect();
            // 同一テクスチャ名は1回だけIrTextureに追加
            let mut name_to_ir: HashMap<String, usize> = HashMap::new();
            for (mat_idx, tex_name) in saved_pkg_tex_assignments {
                if *mat_idx >= ir.materials.len() { continue; }
                let ir_idx = if let Some(&cached) = name_to_ir.get(tex_name) {
                    cached
                } else if let Some(data) = name_to_data.get(tex_name.as_str()) {
                    let idx = ir.textures.len();
                    ir.textures.push(crate::intermediate::types::IrTexture {
                        filename: tex_name.clone(),
                        data: data.to_vec(),
                        mime_type: String::new(),
                    });
                    name_to_ir.insert(tex_name.clone(), idx);
                    idx
                } else {
                    continue;
                };
                ir.materials[*mat_idx].texture_index = Some(ir_idx);
                ir.materials[*mat_idx].apply_textured_defaults();
                log::info!("テクスチャ復元: 材質[{}] '{}' ← '{}'",
                    mat_idx, ir.materials[*mat_idx].name, tex_name);
            }
        }

        if !textures.is_empty() {
            self.tex.pkg_textures = Some(textures);
            self.rebuild_pkg_thumb_cache();
        }

        let result = self.finish_load(ir, path);
        // finish_load がクリアするので、その後に復元
        self.tex.pkg_assignments = saved_pkg_tex_assignments.clone();
        result
    }

    /// 1枚のテクスチャをプレビューダイアログで開く
    fn open_texture_preview(&mut self, path: PathBuf) {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let is_psd = ext == "psd";
        match std::fs::read(&path).and_then(|data|
            super::texture::upload_texture_from_bytes(
                &data, is_psd,
                &self.render_state.device, &self.render_state.queue,
            ).map_err(std::io::Error::other)
        ) {
            Ok(texture_view) => {
                let num_mats = self.loaded.as_ref()
                    .map_or(0, |l| l.ir.materials.len());
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
                    selection: vec![false; num_mats],
                    previewed: vec![false; num_mats],
                    texture_view,
                    saved_binds: HashMap::new(),
                    preview_tex_id,
                });
            }
            Err(e) => {
                self.convert_message = Some(ConvertMessage::failure(
                    format!("テクスチャ読み込み失敗: {e}")
                ));
            }
        }
    }

    /// 複数テクスチャの自動割り当て（ファイル名と材質名のマッチング）
    fn auto_assign_textures(&mut self, image_files: Vec<PathBuf>) {
        let Some(ref loaded) = self.loaded else { return };
        let mat_names: Vec<String> = loaded.ir.materials.iter()
            .map(|m| m.name.to_lowercase())
            .collect();

        let mut assigned = 0usize;
        let mut unmatched: Vec<String> = Vec::new();

        // ファイル名 → マッチする材質インデックスを収集
        let mut assignments: Vec<(PathBuf, Vec<usize>)> = Vec::new();
        for path in &image_files {
            let stem = path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            if stem.is_empty() {
                continue;
            }
            // 材質名にファイル名（拡張子なし）を含む材質を検索
            let matched: Vec<usize> = mat_names.iter()
                .enumerate()
                .filter(|(_, name)| name.contains(&stem) || stem.contains(name.as_str()))
                .map(|(i, _)| i)
                .collect();
            if matched.is_empty() {
                unmatched.push(path.file_name().unwrap_or_default().to_string_lossy().to_string());
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
            self.convert_message = Some(ConvertMessage::failure(
                format!("マッチする材質が見つかりませんでした\nファイル: {}", unmatched.join(", "))
            ));
        }
    }

    fn open_file_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .set_title("3Dモデル / VRMAアニメーションを開く")
            .add_filter("対応形式", &["vrm", "fbx", "pmx", "pmd", "unitypackage", "vrma"])
            .add_filter("VRM (.vrm)", &["vrm"])
            .add_filter("FBX (.fbx)", &["fbx"])
            .add_filter("PMX (.pmx)", &["pmx"])
            .add_filter("PMD (.pmd)", &["pmd"])
            .add_filter("UnityPackage (.unitypackage)", &["unitypackage"])
            .add_filter("VRMA (.vrma)", &["vrma"]);
        if let Some(ref dir) = self.last_model_dir {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.pick_file() {
            if let Some(dir) = path.parent() {
                self.last_model_dir = Some(dir.to_path_buf());
            }
            self.pending_load = Some((path, false));
        }
    }

    /// モデル追加読み込みダイアログ
    fn open_append_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .set_title("モデルを追加読み込み")
            .add_filter("3Dモデル", &["vrm", "fbx", "pmx", "pmd", "unitypackage"])
            .add_filter("VRM (.vrm)", &["vrm"])
            .add_filter("FBX (.fbx)", &["fbx"])
            .add_filter("PMX (.pmx)", &["pmx"])
            .add_filter("PMD (.pmd)", &["pmd"])
            .add_filter("UnityPackage (.unitypackage)", &["unitypackage"]);
        if let Some(ref dir) = self.last_model_dir {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.pick_file() {
            if let Some(dir) = path.parent() {
                self.last_model_dir = Some(dir.to_path_buf());
            }
            self.pending_append = Some((path, false));
        }
    }

    /// モデルを既存モデルに追加（マージ）
    fn append_model(&mut self, path: PathBuf) {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        // unitypackage は専用フローで処理（FBX/VRM選択が必要なため）
        if ext == "unitypackage" {
            match self.try_load_unitypackage_for_append(&path) {
                Ok(()) => {}
                Err(e) => {
                    log::error!("追加読み込み失敗(pkg): {e}");
                    self.convert_message = Some(ConvertMessage::failure(
                        format!("追加読み込みに失敗しました。\n詳細: {e}")
                    ));
                }
            }
            return;
        }

        // 追加モデルの IrModel を構築
        let ir_result: anyhow::Result<IrModel> = (|| -> anyhow::Result<IrModel> {
            match ext.as_str() {
                "fbx" => {
                    crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                        &std::fs::read(&path)?, Some(&path), self.normalize_pose,
                    )
                }
                "pmx" => {
                    let pmx_model = crate::pmx::reader::read_pmx(&path)?;
                    let pmx_dir = path.parent().unwrap_or(std::path::Path::new("."));
                    let mut ir = crate::pmx::extract::pmx_to_ir(&pmx_model, pmx_dir)?;
                    if self.normalize_pose {
                        crate::intermediate::pose::normalize_pose_to_tstance_full(
                            &mut ir.bones, &mut ir.meshes, &mut ir.morphs, &mut ir.physics,
                            crate::convert::coord::gltf_pos_to_pmx,
                        );
                    }
                    Ok(ir)
                }
                "pmd" => {
                    let pmd_model = crate::pmd::reader::read_pmd(&path)?;
                    let mut ir = crate::pmd::extract::pmd_to_ir(&pmd_model, &path)?;
                    if self.normalize_pose {
                        crate::intermediate::pose::normalize_pose_to_tstance_full(
                            &mut ir.bones, &mut ir.meshes, &mut ir.morphs, &mut ir.physics,
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
                            host_fmt.label(), other_fmt.label()
                        );
                        self.convert_message = Some(ConvertMessage::failure(format!(
                            "座標系が異なるモデルは追加できません。\nホスト: {}, 追加: {}",
                            host_fmt.label(), other_fmt.label()
                        )));
                        return;
                    }
                }
                self.finish_append(other_ir, &path);
            }
            Err(e) => {
                log::error!("追加読み込み失敗: {e}");
                self.convert_message = Some(ConvertMessage::failure(
                    format!("追加読み込みに失敗しました。\n詳細: {e}")
                ));
            }
        }
    }

    /// VRMファイルを IrModel として読み込む（追加用・GPU構築なし）
    fn load_vrm_as_ir(&mut self, path: &std::path::Path) -> anyhow::Result<IrModel> {
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
    ) {
        let normalize = self.normalize_pose;
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
                    log::info!("追加(pkg内FBX): {} テクスチャ: {}個", fbx_name, textures.len());
                    pkg_model_name = Some(fbx_name.clone());
                    let mut ir = crate::fbx::extract::extract_ir_model_from_fbx_with_options(
                        &fbx_data, Some(source_path), normalize,
                    )?;
                    // pkg内テクスチャを IrModel に埋め込み
                    let unmatched = crate::unitypackage::embed_textures_into_ir(&mut ir, &textures);
                    log::info!("追加(pkg): {}材質マッチ, 未割当: {}",
                        ir.materials.len() - unmatched.len(), unmatched.len());
                    pkg_unmatched = unmatched;
                    // テクスチャは成功後に蓄積するため、ここでは保持のみ
                    pkg_textures_to_add = textures;
                    Ok(ir)
                }
                PkgModelType::Vrm => {
                    let (vrm_data, vrm_name) =
                        crate::unitypackage::take_vrm(assets, model_index)?;
                    log::info!("追加(pkg内VRM): {}", vrm_name);
                    pkg_model_name = Some(vrm_name.clone());
                    let glb = vrm::loader::load_glb_from_data(&vrm_data)?;
                    let version = vrm::detect::detect_version(&glb.document);
                    let all_extensions = vrm::loader::get_raw_extensions(&glb.document);
                    let mut ir = vrm::extract::extract_ir_model_with_options(
                        &glb.document, &glb.buffers, &glb.images,
                        &glb.vrm_extension, &version, &all_extensions,
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
                let mat_offset = self.loaded.as_ref()
                    .map(|l| l.ir.materials.len())
                    .unwrap_or(0);
                let appended_before = self.loaded.as_ref().map(|l| l.appended_paths.len()).unwrap_or(0);
                self.finish_append_with_pkg_name(other_ir, source_path, pkg_model_name);
                let appended_after = self.loaded.as_ref().map(|l| l.appended_paths.len()).unwrap_or(0);
                // アペンド成功後のみpkgテクスチャを蓄積（ロールバック時はスキップ）
                if appended_after > appended_before && !pkg_textures_to_add.is_empty() {
                    if let Some(ref mut existing) = self.tex.pkg_textures {
                        existing.extend(pkg_textures_to_add);
                    } else {
                        self.tex.pkg_textures = Some(pkg_textures_to_add);
                    }
                    self.rebuild_pkg_thumb_cache();
                }
                // 未割当材質がある場合、手動割当ダイアログを開く（リロード中は抑制）
                if !pkg_unmatched.is_empty() && self.tex.pkg_textures.is_some() && !self.suppress_tex_match {
                    // ローカルIndexにマージ後の材質オフセットを加算
                    let global_unmatched: Vec<usize> = pkg_unmatched.iter()
                        .map(|&i| i + mat_offset)
                        .collect();
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
                self.convert_message = Some(ConvertMessage::failure(
                    format!("追加読み込みに失敗しました。\n詳細: {e}")
                ));
            }
        }
    }

    /// 追加モデルの IrModel を既存モデルにマージしてGPU再構築
    fn finish_append(&mut self, other_ir: crate::intermediate::types::IrModel, path: &std::path::Path) {
        self.finish_append_ext(other_ir, path, false, None);
    }

    fn finish_append_with_pkg_name(&mut self, other_ir: crate::intermediate::types::IrModel, path: &std::path::Path, pkg_model_name: Option<String>) {
        self.finish_append_ext(other_ir, path, false, pkg_model_name);
    }

    fn finish_append_ext(&mut self, other_ir: crate::intermediate::types::IrModel, path: &std::path::Path, silent: bool, pkg_model_name: Option<String>) {
        let Some(ref mut loaded) = self.loaded else { return };

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
        let saved_bone_meta: Vec<(Vec<usize>, Option<String>)> = loaded.ir.bones.iter()
            .map(|b| (b.children.clone(), b.vrm_bone_name.clone()))
            .collect();

        // IrModel をマージ（同名ボーン統合）
        let (merged_bones, new_bones) = loaded.ir.merge(other_ir);

        // GPU モデル再構築
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        match super::mesh::build_gpu_model_from_ir(
            &loaded.ir, device, queue,
            self.display.smooth_normals, self.display.clear_custom_normals,
        ) {
            Ok(gpu_model) => {
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
                loaded.stats_cache = CachedStats {
                    total_vertices: loaded.ir.total_vertices(),
                    total_faces: loaded.ir.total_faces(),
                };
                // 材質グループを更新（追加モデル分を記録）
                let prev_draw_end: usize = loaded.material_groups.iter()
                    .map(|(_, start, count)| start + count)
                    .max()
                    .unwrap_or(0);
                let new_draws = gpu_model.draws.len().saturating_sub(prev_draw_end);
                if new_draws > 0 {
                    loaded.material_groups.push((added_name.clone(), prev_draw_end, new_draws));
                }
                loaded.gpu_model = gpu_model;
                // 追加パスを記録（リロード時に再マージ用）
                loaded.appended_paths.push(AppendedModel {
                    path: path.to_path_buf(),
                    pkg_model_name: pkg_model_name.clone(),
                });
                // テクスチャダイアログの初期ディレクトリを追加モデルのディレクトリに設定
                if let Some(dir) = path.parent() {
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
                    added_name, added_bones, merged_bones, new_bones, added_meshes, added_materials,
                );
                if !silent {
                    self.convert_message = Some(ConvertMessage::success(
                        format!(
                            "追加読み込み完了: {}\nボーン:{} (統合:{} + 新規:{}), メッシュ:{}, 材質:{}",
                            added_name, added_bones, merged_bones, new_bones, added_meshes, added_materials,
                        )
                    ));
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
                self.convert_message = Some(ConvertMessage::failure(
                    format!("追加読み込み後のGPU再構築に失敗しました。\n詳細: {e}")
                ));
            }
        }
    }

    /// プログレスオーバーレイ描画（ビューポート上、結果メッセージと同じスタイル）
    fn paint_progress_overlay(&self, viewport: &egui::Ui, rect: egui::Rect, ctx: &egui::Context) {
        let msg = if self.pending_load.is_some() || self.pending_append.is_some() || self.pending_pkg_load.is_some() {
            Some("読み込み中...")
        } else if self.pending_rebuild.is_some() || self.pending_reload.is_some() {
            Some("処理中...")
        } else if self.pending_convert.is_some() {
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
        if let Some((_, ref mut shown)) = self.pending_load {
            if !*shown {
                *shown = true;
                ctx.request_repaint();
            }
        }
        if let Some((_, ref mut shown)) = self.pending_append {
            if !*shown {
                *shown = true;
                ctx.request_repaint();
            }
        }
        if let Some(ref mut p) = self.pending_pkg_load {
            if !p.shown {
                p.shown = true;
                ctx.request_repaint();
            }
        }
        if self.pending_rebuild == Some(PendingOverlay::WaitingOverlay) {
            self.pending_rebuild = Some(PendingOverlay::Ready);
            ctx.request_repaint();
        }
        if self.pending_reload == Some(PendingOverlay::WaitingOverlay) {
            self.pending_reload = Some(PendingOverlay::Ready);
            ctx.request_repaint();
        }
        if self.pending_convert == Some(PendingOverlay::WaitingOverlay) {
            self.pending_convert = Some(PendingOverlay::Ready);
            ctx.request_repaint();
        }
    }

    /// テクスチャプレビューの同期（selection と previewed の差分を GPU に反映）
    pub fn sync_tex_preview(&mut self) {
        let Some(ref mut preview) = self.tex.pending_preview else { return };
        let Some(ref mut loaded) = self.loaded else { return };
        let Some(ref renderer) = self.renderer else { return };
        let device = &self.render_state.device;
        let texture_bgl = renderer.texture_bgl();
        let sampler = renderer.sampler();

        for mat_idx in 0..preview.selection.len() {
            if preview.selection[mat_idx] && !preview.previewed[mat_idx] {
                // プレビュー適用: 元の bind group を退避し、プレビュー用に差し替え
                for (draw_idx, draw) in loaded.gpu_model.draws.iter_mut().enumerate() {
                    if draw.material_index == mat_idx {
                        if let std::collections::hash_map::Entry::Vacant(e) = preview.saved_binds.entry(draw_idx) {
                            e.insert(draw.texture_bind_group.take());
                        }
                        draw.texture_bind_group = Some(
                            gpu::create_texture_bind_group(device, texture_bgl, &preview.texture_view, sampler),
                        );
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
        let Some(preview) = self.tex.pending_preview.take() else { return };
        let path = &preview.path;

        // 選択された材質のインデックスを収集
        let selected: Vec<usize> = preview.selection.iter()
            .enumerate()
            .filter_map(|(i, &v)| if v { Some(i) } else { None })
            .collect();

        if selected.is_empty() {
            // 何も選択されていなければ元に戻す
            self.cancel_tex_preview_inner(preview);
            return;
        }

        // IrModel にテクスチャを追加（1回だけ）
        let Some(ref mut loaded) = self.loaded else { return };

        let ext_lower = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let is_psd = ext_lower == "psd";

        let tex_data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                log::error!("ファイル読み込み失敗: {e}");
                return;
            }
        };

        let basename = path.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let (ir_data, ir_filename, ir_mime) = if is_psd {
            match Self::psd_to_png(&tex_data) {
                Ok(png_data) => (png_data, format!("{}.png", basename), "image/png".to_string()),
                Err(e) => {
                    log::error!("PSD→PNG変換失敗: {e}");
                    return;
                }
            }
        } else {
            let filename = path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let mime = match ext_lower.as_str() {
                "png" => "image/png",
                "tga" => "image/x-tga",
                "bmp" => "image/bmp",
                _ => "image/jpeg",
            };
            (tex_data, filename, mime.to_string())
        };

        let tex_idx = loaded.ir.textures.len();
        loaded.ir.textures.push(crate::intermediate::types::IrTexture {
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
                mat_idx, mat.name, path_buf.display()
            );
        }

        // 割り当て履歴を記録（reload_current 時の復元用）
        for &mat_idx in &selected {
            self.tex.assignments.insert(mat_idx, path_buf.clone());
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
        let Some(preview) = self.tex.pending_preview.take() else { return };
        self.cancel_tex_preview_inner(preview);
    }

    fn cancel_tex_preview_inner(&mut self, preview: PendingTexPreview) {
        // サムネイル用 egui テクスチャを解放
        if let Some(tex_id) = preview.preview_tex_id {
            let mut renderer = self.render_state.renderer.write();
            renderer.free_texture(&tex_id);
        }
        let Some(ref mut loaded) = self.loaded else { return };
        // 退避した全 bind group を復元
        for (draw_idx, orig) in preview.saved_binds.into_iter() {
            if draw_idx < loaded.gpu_model.draws.len() {
                loaded.gpu_model.draws[draw_idx].texture_bind_group = orig;
            }
        }
    }

    /// 遅延処理（ファイル読み込み、GPU再構築、PMX変換など）を実行
    fn process_pending_tasks(&mut self) {
        if let Some((_, true)) = self.pending_load {
            let (path, _) = self.pending_load.take().expect("pending_load は Some(true) 確認済み");
            self.load_file(path);
        }
        // モデル追加読み込み（アペンド）
        if let Some((_, true)) = self.pending_append {
            let (path, _) = self.pending_append.take().expect("pending_append は Some(true) 確認済み");
            self.append_model(path);
        }
        // unitypackage モデル遅延読み込み
        if self.pending_pkg_load.as_ref().is_some_and(|p| p.shown) {
            let p = self.pending_pkg_load.take().expect("pending_pkg_load は shown 確認済み");
            let source_path = p.source_path.clone();

            // アペンドモード: unitypackage内モデルを既存モデルに追加
            if p.append {
                // リロード経由の場合はテクスチャ選択ダイアログを抑制
                if p.suppress_tex_match {
                    self.suppress_tex_match = true;
                }
                self.append_from_pkg(p.assets, p.fbx_index, p.model_type, &source_path);
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
                            let fbx_name = p.assets.get(p.fbx_index)
                                .map(|a| a.filename())
                                .unwrap_or_default();
                            self.pending_fbx_choice = Some(PendingFbxChoice {
                                path: std::path::PathBuf::from(&fbx_name),
                                load_model: true,
                                load_animation: true,
                                pkg_context: Some(PendingFbxChoicePkg {
                                    assets: p.assets,
                                    fbx_index: p.fbx_index,
                                    source_path,
                                }),
                            });
                        } else {
                            match self.load_fbx_from_assets(p.assets, p.fbx_index, &source_path, FbxLoadMode::ModelOnly) {
                                Ok(()) => { self.convert_message = None; }
                                Err(e) => {
                                    log::error!("読み込み失敗: {e}");
                                    self.convert_message = Some(ConvertMessage::failure(format!("ファイルを読み込めませんでした。\n詳細: {e}")));
                                }
                            }
                        }
                    } else {
                        match self.load_fbx_from_assets(p.assets, p.fbx_index, &source_path, FbxLoadMode::Both) {
                            Ok(()) => {
                                log::info!("読み込み成功: {}", source_path.display());
                                self.convert_message = None;
                            }
                            Err(e) => {
                                log::error!("読み込み失敗: {e}");
                                self.convert_message = Some(ConvertMessage::failure(format!("ファイルを読み込めませんでした。\n詳細: {e}")));
                            }
                        }
                    }
                }
                PkgModelType::Vrm => {
                    match self.load_vrm_from_assets(p.assets, p.fbx_index, &source_path) {
                        Ok(()) => {
                            log::info!("読み込み成功: {}", source_path.display());
                            self.convert_message = None;
                        }
                        Err(e) => {
                            log::error!("読み込み失敗: {e}");
                            self.convert_message = Some(ConvertMessage::failure(format!("ファイルを読み込めませんでした。\n詳細: {e}")));
                        }
                    }
                }
            }
            } // else (通常ロード)
        }
        if self.pending_rebuild == Some(PendingOverlay::Ready) {
            self.pending_rebuild = None;
            self.rebuild_gpu_model();
        }
        if self.pending_reload == Some(PendingOverlay::Ready) {
            self.pending_reload = None;
            self.reload_current();
        }
        if self.pending_convert == Some(PendingOverlay::Ready) {
            self.pending_convert = None;
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
                anim.apply_bone_animation(
                    &mut loaded.gpu_model,
                    queue,
                    &self.morph_weights,
                );
                self.morph_dirty = false;
            }
        }
    }

    /// ドラッグ＆ドロップ処理。(画像ホバー中, モデルホバー中) を返す
    fn process_drag_and_drop(&mut self, ctx: &egui::Context) -> (bool, bool) {
        let (dropped_files, hover_ext, shift_held) = ctx.input(|i| {
            let hover_ext = i.raw.hovered_files.first()
                .and_then(|f| f.path.as_ref())
                .and_then(|p| p.extension())
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();
            self.drag_hovering = !i.raw.hovered_files.is_empty();
            let paths: Vec<PathBuf> = i.raw.dropped_files.iter()
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
                let ext = path.extension()
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
                let append_ext = model_path.extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                let is_appendable = matches!(append_ext.as_str(), "vrm" | "fbx" | "pmx" | "pmd" | "unitypackage");
                if shift_held && has_loaded_model && is_appendable {
                    // Shift+D&D: 既存モデルに追加
                    self.pending_append = Some((model_path, false));
                } else {
                    self.pending_load = Some((model_path, false));
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
                        self.camera.reset_to_bbox_with_margin(bbox_min, bbox_max, self.last_viewport_height);
                    }
                }
                if !i.modifiers.ctrl && i.key_pressed(egui::Key::F) {
                    if let Some(ref loaded) = self.loaded {
                        let (bbox_min, bbox_max) = loaded.gpu_model.bbox();
                        self.camera.fit_to_bbox_with_margin(bbox_min, bbox_max, self.last_viewport_height);
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
                            self.camera.fit_to_bbox_with_margin(bbox_min, bbox_max, self.last_viewport_height);
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
        // ウィンドウタイトル更新
        if let Some(title) = self.window_title.take() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }

        // FPS計測（指数移動平均）
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame_time).as_secs_f32();
        self.last_frame_time = now;
        if dt > 0.0 {
            let fps = 1.0 / dt;
            self.fps_smoothed = if self.fps_smoothed == 0.0 {
                fps
            } else {
                self.fps_smoothed * 0.9 + fps * 0.1
            };
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
                if self.loaded.is_some() {
                    if bar.button("追加").on_hover_text("モデルを追加読み込み（Shift+D&Dでも可）").clicked() {
                        self.open_append_dialog();
                    }
                }

                if bar.button("ログ").clicked() {
                    open_directory(&self.logs_dir);
                }

                if let Some(ref loaded) = self.loaded {
                    bar.separator();
                    bar.label(
                        egui::RichText::new(&loaded.ir.name)
                            .color(egui::Color32::from_gray(0x20)),
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
                        ui.label(egui::RichText::new(
                            loaded.file_path.to_string_lossy().as_ref()
                        ).font(font.clone()).color(color));

                        ui.separator();

                        // モデル統計（キャッシュ利用）
                        let stats = format!(
                            "頂点:{} 面:{} 材質:{} テクスチャ:{} ボーン:{} モーフ:{}",
                            loaded.stats_cache.total_vertices,
                            loaded.stats_cache.total_faces,
                            ir.materials.len(),
                            ir.textures.len(),
                            ir.bones.len(),
                            ir.morphs.len(),
                        );
                        ui.label(egui::RichText::new(stats).font(font.clone()).color(color));

                        // FBXの場合、テクスチャ設定状況（キャッシュ利用）
                        if ir.source_format == crate::intermediate::types::SourceFormat::Fbx {
                            let tex_set = loaded.mat_cache.tex_set_count;
                            let tex_total = ir.materials.len();
                            ui.separator();
                            let tex_status = format!("Tex:{}/{}", tex_set, tex_total);
                            let tex_color = if tex_set == tex_total {
                                egui::Color32::from_rgb(0x40, 0xC0, 0x40)
                            } else {
                                egui::Color32::from_rgb(0xD0, 0xA0, 0x40)
                            };
                            ui.label(egui::RichText::new(tex_status).font(font).color(tex_color));
                        }
                    } else {
                        ui.label(
                            egui::RichText::new("VRM/FBX ファイルを読み込んでください")
                                .font(egui::FontId::proportional(11.0))
                                .color(egui::Color32::from_gray(0x60))
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

                // ビューポートの高さを記録（フィット計算用）
                self.last_viewport_height = response.rect.height();

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
                                    self.camera.fit_to_bbox_with_margin(bbox_min, bbox_max, rect.height());
                                }
                            }
                            if ui.small_button("リセット(R)").on_hover_text("カメラをリセット").clicked() {
                                if let Some(ref loaded) = self.loaded {
                                    let (bbox_min, bbox_max) = loaded.gpu_model.bbox();
                                    self.camera.reset_to_bbox_with_margin(bbox_min, bbox_max, rect.height());
                                }
                            }
                        });
                    });
                }

                // FPS表示（右上オーバーレイ）
                {
                    let rect = response.rect;
                    let fps_text = format!("{:.0} fps", self.fps_smoothed);
                    viewport.painter().text(
                        egui::pos2(rect.right() - 10.0, rect.top() + 10.0),
                        egui::Align2::RIGHT_TOP,
                        &fps_text,
                        egui::FontId::monospace(11.0),
                        egui::Color32::BLACK,
                    );
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
                            "左ドラッグ:回転  右/中ドラッグ:パン  ホイール:ズーム  Ctrl+O:開く",
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
