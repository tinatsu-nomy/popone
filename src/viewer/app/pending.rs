//! 遅延タスク処理（PendingState, ExportState, process_pending_tasks 等）

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use eframe::egui;

use crate::unitypackage::UnityPackageIndex;

use crate::intermediate::types::IrModel;

use super::file_io::FileFormat;
use super::helpers::{PkgModelType, PreloadedData, ReloadableSource};
use super::{ConvertMessage, ViewerApp};

/// unitypackage 内に複数FBXがある場合の選択待ち状態
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
    /// Phase 3: パッケージインデックス（Prefab テクスチャ解決用）
    pub pkg_index: Option<Arc<UnityPackageIndex>>,
    /// 複数選択用チェック状態（model_list と同サイズ）
    pub checked: Vec<bool>,
}

/// unitypackage モデル遅延読み込み状態
pub struct PendingPkgModelLoad {
    pub assets: Arc<Vec<crate::unitypackage::ExtractedAsset>>,
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
    /// Phase 3: パッケージインデックス（Prefab テクスチャ解決用）
    pub pkg_index: Option<Arc<UnityPackageIndex>>,
    /// Batch progress: (current, total) — carried from PendingMultiLoad at queue pop time
    pub batch_progress: Option<(usize, usize)>,
    /// FBX アニメ選択ダイアログをスキップ（execute_fbx_choice 確定後の再投入時）
    pub skip_anim_check: bool,
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
    pub preloaded: Option<super::helpers::PreloadedData>,
}

/// unitypackage 経由 FBX 選択時の追加コンテキスト
pub struct PendingFbxChoicePkg {
    pub assets: Arc<Vec<crate::unitypackage::ExtractedAsset>>,
    pub fbx_index: usize,
    pub source_path: PathBuf,
    /// 一時ファイルからの読み込み時、アーカイブデータのスナップショット
    pub archive_snapshot: Option<Arc<[u8]>>,
    /// アーカイブ(ZIP/7z)内 .unitypackage の場合、リロード用のソース情報
    pub nested_archive_source: Option<ReloadableSource>,
    /// Phase 3: パッケージインデックス（Prefab テクスチャ解決用）
    pub pkg_index: Option<Arc<UnityPackageIndex>>,
}

/// 遅延処理のオーバーレイ表示状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingOverlay {
    /// オーバーレイ未表示（次フレームで表示）
    WaitingOverlay,
    /// オーバーレイ表示済み（次フレームで実行）
    Ready,
}

/// ロード投入（全入口統一: ダイアログ / D&D / IPC）
pub struct PendingLoadDispatch {
    pub path: PathBuf,
    pub append: bool,
    pub overlay: PendingOverlay,
    /// D&D temp ファイルの先読みデータ（self.preloaded から移動）
    pub preloaded: Option<PreloadedData>,
    /// reload_current 経由の dispatch かどうか。true の場合、route_load_dispatch は
    /// 新規ロード向けの状態リセット（normalize_pose 等）をスキップし、ユーザーが
    /// 現モデルに設定した値を保持したまま BG パースへ進む。
    pub is_reload: bool,
}

/// バックグラウンド CPU パースの結果
pub struct BgLoadResult {
    pub ir: IrModel,
    pub source: ReloadableSource,
    pub kind: BgLoadKind,
    pub path: PathBuf,
    /// 発行元 dispatch の世代番号。現世代の `BgLoadHandle.request_id` と一致しない結果は
    /// 「古いロードが完了したがユーザーは既に次のロードに進んでいる」状態として破棄する。
    pub request_id: u64,
}

/// バックグラウンド CPU パースのハンドル（受信チャネル + キャンセルトークン + 世代番号）
pub struct BgLoadHandle {
    pub rx: std::sync::mpsc::Receiver<anyhow::Result<BgLoadResult>>,
    /// 別スレッドのパース処理へのキャンセル通知。新規ロード投入時に `true` に設定する。
    pub cancel: Arc<AtomicBool>,
    pub request_id: u64,
}

/// ロード種別（後処理の分岐に使用）
pub enum BgLoadKind {
    /// 通常ロード（format + FBX自動アニメフラグ）
    Initial {
        format: FileFormat,
        auto_fbx_anim: bool,
    },
    /// 追加読み込み
    Append,
    /// アーカイブ内モデルの初回ロード（BGスレッドで解凍+パース済み）
    ArchiveInitial,
    /// アーカイブ内モデルの追加ロード（BGスレッドで解凍+パース済み）
    ArchiveAppend,
    /// アーカイブ内 .unitypackage — BGスレッドで pkg_index 構築済み
    ArchivePreparedUnityPackage {
        pkg_data: Vec<u8>,
        pkg_index: Arc<crate::unitypackage::UnityPackageIndex>,
        assets: Vec<crate::unitypackage::ExtractedAsset>,
        model_list: Vec<(usize, String, PkgModelType)>,
        source_path: PathBuf,
        archive_data: Arc<[u8]>,
        is_temp: bool,
        append: bool,
        entry_path: PathBuf,
    },
    /// UnityPackage 内モデル初回ロード（BGスレッドでパース済み）
    PkgInitial(Box<PkgInitialPayload>),
    /// UnityPackage 内モデル追加ロード（BGスレッドでパース済み）
    PkgAppend(Box<PkgAppendPayload>),
    /// FBX アニメーション選択待ち（パース済み IR を保留）
    NeedsFbxChoice(Box<PkgFbxChoicePayload>),
    /// UnityPackage index 構築完了（メインスレッドで PendingUnityPackage/PkgModelLoad をセット）
    UnityPackageIndexed {
        pkg_index: Arc<crate::unitypackage::UnityPackageIndex>,
        assets: Vec<crate::unitypackage::ExtractedAsset>,
        model_list: Vec<(usize, String, PkgModelType)>,
        source_path: PathBuf,
        is_temp: bool,
        archive_snapshot: Option<Arc<[u8]>>,
        append: bool,
    },
    /// アーカイブ一覧化完了（メインスレッドで PendingArchive/ArchiveLoad をセット）
    ArchiveIndexed {
        archive_data: Arc<[u8]>,
        format: crate::archive::ArchiveFormat,
        contents: crate::archive::ArchiveContents,
        source_path: PathBuf,
        is_temp: bool,
        append: bool,
    },
}

/// PkgInitial 用ペイロード
pub struct PkgInitialPayload {
    pub fbx_name: Option<String>,
    pub pkg_model_locator: Option<crate::unitypackage::PkgModelLocator>,
    pub pkg_textures_legacy: Vec<(String, Arc<[u8]>)>,
    pub unmatched_indices: Vec<usize>,
    pub pkg_material_keys: Vec<Option<crate::unitypackage::PkgMaterialKey>>,
    /// Prefab FBX ranges (name, mat_start, mat_count)
    pub fbx_ranges: Vec<(String, usize, usize)>,
    pub batch_progress: Option<(usize, usize)>,
    pub suppress_tex_match: bool,
    /// Prefab 名（ファイル名表示用）
    pub prefab_name: Option<String>,
    /// Prefab のエントリパス
    pub prefab_entry_path: Option<String>,
}

/// PkgAppend 用ペイロード
pub struct PkgAppendPayload {
    pub pkg_model_name: Option<String>,
    pub pkg_model_locator: Option<crate::unitypackage::PkgModelLocator>,
    pub pkg_textures_to_add: Vec<(String, Arc<[u8]>)>,
    pub pkg_unmatched: Vec<usize>,
    pub batch_progress: Option<(usize, usize)>,
    pub suppress_tex_match: bool,
    pub pkg_material_keys: Vec<Option<crate::unitypackage::PkgMaterialKey>>,
}

/// FBX アニメーション選択保留用ペイロード
pub struct PkgFbxChoicePayload {
    pub fbx_name: String,
    pub assets: Arc<Vec<crate::unitypackage::ExtractedAsset>>,
    pub fbx_index: usize,
    pub source_path: PathBuf,
    pub archive_snapshot: Option<Arc<[u8]>>,
    pub source_override: Option<super::helpers::ReloadableSource>,
    pub pkg_index: Option<Arc<crate::unitypackage::UnityPackageIndex>>,
    pub batch_progress: Option<(usize, usize)>,
}

/// 非同期ファイルダイアログの種別
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileDialogKind {
    /// モデル/アニメーションを開く
    Open,
    /// モデル追加読み込み
    Append,
}

/// バックグラウンドロードの状態マシン。
///
/// 従来は `load_dispatch: Option<PendingLoadDispatch>` と `bg_load: Option<BgLoadHandle>` の
/// 2 フィールド併存で表現していたが、「両方 Some」「両方 None のはずが片方だけ取り残される」などの
/// 不正状態を型レベルで排除するため enum に統合した。
///
/// 状態遷移:
/// - `Idle` → `PendingDispatch`: ファイルダイアログ結果・D&D・IPC・コマンドライン引数
/// - `PendingDispatch` → `Idle` or `Loading`: `route_load_dispatch` が即時実行 / spawn_bg_load を選ぶ
/// - `Loading` → `Idle`: BG スレッドからの結果受信
/// - `Loading` → `PendingDispatch { prior_loading: Some(..) }`: Loading 中に次の dispatch が投入された場合、
///   先行 handle を `prior_loading` として引き継ぎ、`route_load_dispatch` が intent に応じて
///   キャンセル（モデル要求）か保護（アニメ単体要求）を判断する
pub enum BackgroundLoadState {
    /// 何も走っていない
    Idle,
    /// dispatch 予約あり。次フレームで `route_load_dispatch` が呼ばれる。
    /// `prior_loading` は Loading 中に新 dispatch が投入された場合の先行 handle。
    PendingDispatch {
        dispatch: PendingLoadDispatch,
        prior_loading: Option<BgLoadHandle>,
    },
    /// BG スレッドがパース中。結果受信待ち。
    Loading(BgLoadHandle),
}

impl BackgroundLoadState {
    pub fn is_idle(&self) -> bool {
        matches!(self, BackgroundLoadState::Idle)
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, BackgroundLoadState::Loading(_))
    }

    /// 何らかのロード処理が進行中（dispatch 予約 or BG パース中）
    pub fn is_active(&self) -> bool {
        !self.is_idle()
    }

    /// 新しい dispatch を投入する。
    /// 先行 Loading がある場合は `prior_loading` として引き継ぎ、`route_load_dispatch` の
    /// intent 判定（モデル要求 vs アニメ単体要求）でキャンセル可否を決定する。
    /// 既に `PendingDispatch` 状態なら古い dispatch は破棄し、その `prior_loading` のみ引き継ぐ。
    pub fn submit_dispatch(&mut self, dispatch: PendingLoadDispatch) {
        let prior_loading = match std::mem::replace(self, BackgroundLoadState::Idle) {
            BackgroundLoadState::Idle => None,
            BackgroundLoadState::Loading(h) => Some(h),
            BackgroundLoadState::PendingDispatch { prior_loading, .. } => prior_loading,
        };
        *self = BackgroundLoadState::PendingDispatch {
            dispatch,
            prior_loading,
        };
    }
}

/// OBJ/STL インポート時の単位選択
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImportUnit {
    Mm,
    Cm,
    M,
    Inch,
}

impl ImportUnit {
    /// glTF 空間（メートル）へのスケール係数
    pub fn scale(self) -> f32 {
        match self {
            ImportUnit::Mm => 0.001,
            ImportUnit::Cm => 0.01,
            ImportUnit::M => 1.0,
            ImportUnit::Inch => 0.0254,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            ImportUnit::Mm => "ミリメートル (mm)",
            ImportUnit::Cm => "センチメートル (cm)",
            ImportUnit::M => "メートル (m)",
            ImportUnit::Inch => "インチ (inch)",
        }
    }
}

/// OBJ/STL インポートオプション選択待ち状態
pub struct PendingImportOptions {
    pub path: PathBuf,
    pub format: FileFormat,
    pub append: bool,
    pub preloaded: Option<PreloadedData>,
    pub unit: ImportUnit,
    pub z_up: bool,
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
    /// アーカイブモデル遅延読み���み
    pub archive_load: Option<PendingArchiveLoad>,
    /// バックグラウンドロードの状態マシン
    /// （dispatch 予約 + BG パース中のハンドルを型レベルで統合）
    pub bg_state: BackgroundLoadState,
    /// PMX変換遅延実行
    pub convert: Option<PendingOverlay>,
    /// GPU再構築遅延実行
    pub rebuild: Option<PendingOverlay>,
    /// モデル再読み込み遅延実行
    pub reload: Option<PendingOverlay>,
    /// ビューポートサイズ確定後の refit（初回ロード時）
    pub refit: bool,
    /// テクスチャ履歴の上書き保存確認ダイアログ表示フラグ
    pub confirm_save_tex_history: bool,
    /// 非同期ファイルダイアログ（種別, 結果受信チャネル）
    pub file_dialog: Option<(FileDialogKind, std::sync::mpsc::Receiver<Option<PathBuf>>)>,
    /// OBJ/STL インポートオプション選択待ち
    pub import_options: Option<PendingImportOptions>,
    /// 複数モデル一括ロード（assets を1つだけ保持し、デキュー時に clone）
    pub multi_load: Option<PendingMultiLoad>,
    /// GPU テクスチャアップロードのフレーム分割（BG パース完了後）
    pub gpu_build: Option<PendingGpuBuild>,
    /// バックグラウンド PMX 変換の状態
    pub convert_bg: Option<PendingConvertBg>,
}

/// GPU テクスチャアップロード + CPU prep + GPU finalize のフレーム分割状態
pub struct PendingGpuBuild {
    pub ir: IrModel,
    pub source: super::helpers::ReloadableSource,
    /// アップロード済みテクスチャビュー（順番に蓄積）
    pub gpu_textures: Vec<(eframe::wgpu::TextureView, eframe::wgpu::TextureView)>,
    /// 次にアップロードするテクスチャインデックス
    pub next_tex: usize,
    /// material_display から抽出したフラグ群
    pub mat_flags: super::super::mesh::MaterialBuildFlags,
    /// apply_bg_load_result 側の後処理用 kind（PkgInitial 等）
    pub post_kind: Option<BgLoadKind>,
    /// 結果パス（ログ用）
    pub path: PathBuf,
    /// Append 時の追加情報（None = 初回ロード）
    pub append_info: Option<Box<AppendGpuBuildInfo>>,
    /// BG スレッドで CPU prep 実行中の結果受信チャネル
    pub(crate) cpu_prep_rx: Option<
        std::sync::mpsc::Receiver<anyhow::Result<(super::super::mesh::CpuPrepResult, IrModel)>>,
    >,
}

/// IR merge 前のサイズ情報スナップショット（ロールバック用）
pub struct IrRollbackSnapshot {
    pub bone_count: usize,
    pub mesh_count: usize,
    pub material_count: usize,
    pub texture_count: usize,
    pub morph_count: usize,
    pub rigid_count: usize,
    pub joint_count: usize,
    pub name: String,
    pub node_to_bone: std::collections::HashMap<usize, usize>,
    pub humanoid_bone_count: usize,
    pub bone_meta: Vec<(Vec<usize>, Option<String>)>,
}

impl IrRollbackSnapshot {
    /// IrModel の現在の状態からスナップショットを取得
    pub fn capture(ir: &crate::intermediate::types::IrModel) -> Self {
        Self {
            bone_count: ir.bones.len(),
            mesh_count: ir.meshes.len(),
            material_count: ir.materials.len(),
            texture_count: ir.textures.len(),
            morph_count: ir.morphs.len(),
            rigid_count: ir.physics.rigid_bodies.len(),
            joint_count: ir.physics.joints.len(),
            name: ir.name.clone(),
            node_to_bone: ir.node_to_bone.clone(),
            humanoid_bone_count: ir.humanoid_bone_count,
            bone_meta: ir
                .bones
                .iter()
                .map(|b| (b.children.clone(), b.vrm_bone_name.clone()))
                .collect(),
        }
    }

    /// マージ済み IR をスナップショット時点に巻き戻す
    pub fn rollback(self, ir: &mut crate::intermediate::types::IrModel) {
        ir.bones.truncate(self.bone_count);
        ir.meshes.truncate(self.mesh_count);
        ir.materials.truncate(self.material_count);
        ir.textures.truncate(self.texture_count);
        ir.morphs.truncate(self.morph_count);
        ir.physics.rigid_bodies.truncate(self.rigid_count);
        ir.physics.joints.truncate(self.joint_count);
        ir.name = self.name;
        ir.node_to_bone = self.node_to_bone;
        ir.humanoid_bone_count = self.humanoid_bone_count;
        for (i, bone) in ir.bones.iter_mut().enumerate() {
            if i < self.bone_meta.len() {
                bone.children = self.bone_meta[i].0.clone();
                bone.vrm_bone_name = self.bone_meta[i].1.clone();
            }
        }
    }
}

/// LoadedModel の所有権フィールドスナップショット（IR/GPU以外）
pub struct LoadedModelOwnership {
    pub source: super::helpers::ReloadableSource,
    pub primary_astance_result: crate::intermediate::types::AStanceResult,
    pub appended_models: Vec<super::AppendedModel>,
    pub material_groups: Vec<super::MaterialGroup>,
    pub pkg_material_keys: Vec<Option<crate::unitypackage::PkgMaterialKey>>,
    pub prefab_name: Option<String>,
    pub prefab_entry_path: Option<String>,
}

/// Append 中のアニメーション再生状態スナップショット
pub struct AnimationSnapshot {
    pub animation_arc: Option<std::sync::Arc<crate::intermediate::animation::VrmaAnimation>>,
    pub playing: bool,
    pub loop_mode: super::super::animation::LoopMode,
    pub speed: f32,
    pub current_time: f32,
    pub ab_start: Option<f32>,
    pub ab_end: Option<f32>,
    pub ping_pong_direction: f32,
}

impl AnimationSnapshot {
    /// ViewerApp のアニメーション状態からスナップショットを取得
    pub fn capture(anim_state: Option<&super::super::animation::AnimationState>) -> Self {
        if let Some(s) = anim_state {
            Self {
                animation_arc: Some(std::sync::Arc::clone(&s.animation)),
                playing: s.playing,
                loop_mode: s.loop_mode,
                speed: s.speed,
                current_time: s.current_time,
                ab_start: s.ab_start,
                ab_end: s.ab_end,
                ping_pong_direction: s.ping_pong_direction,
            }
        } else {
            Self {
                animation_arc: None,
                playing: false,
                loop_mode: super::super::animation::LoopMode::Normal,
                speed: 1.0,
                current_time: 0.0,
                ab_start: None,
                ab_end: None,
                ping_pong_direction: 1.0,
            }
        }
    }

    /// AnimationState に再生状態を復元する
    pub fn apply_to(&self, state: &mut super::super::animation::AnimationState) {
        state.playing = self.playing;
        state.loop_mode = self.loop_mode;
        state.speed = self.speed;
        state.current_time = self.current_time;
        state.ab_start = self.ab_start;
        state.ab_end = self.ab_end;
        state.ping_pong_direction = self.ping_pong_direction;
    }
}

/// Append 遅延 GPU ビルド時の追加情報
pub struct AppendGpuBuildInfo {
    /// GPU ビルド失敗時のロールバック用: マージ前の GPU モデル
    pub rollback_gpu_model: super::super::mesh::GpuModel,
    /// マージ前の IR スナップショット（ロールバック用）
    pub ir_snapshot: IrRollbackSnapshot,
    /// マージ前の LoadedModel 所有権フィールド
    pub ownership: LoadedModelOwnership,
    /// 追加モデルの ReloadableSource
    pub append_source: super::helpers::ReloadableSource,
    /// 追加モデル名
    pub added_name: String,
    /// 追加モデルのボーン数
    pub added_bones: usize,
    /// 追加モデルのメッシュ数
    pub added_meshes: usize,
    /// 追加モデルの材質数
    pub added_materials: usize,
    /// マージ前の材質数（MaterialGroup 構築用）
    pub saved_material_count: usize,
    /// merge() の戻り値: 統合ボーン数
    pub merged_bones: usize,
    /// merge() の戻り値: 新規ボーン数
    pub new_bones: usize,
    /// unitypackage 内のモデル名
    pub pkg_model_name: Option<String>,
    /// unitypackage モデルロケータ
    pub pkg_locator: Option<crate::unitypackage::PkgModelLocator>,
    /// サイレントモード（トーストを表示しない）
    pub silent: bool,
    /// PkgAppend 後処理用ペイロード（None = 非 Pkg アペンド）
    pub pkg_append_payload: Option<Box<PkgAppendPayload>>,
    /// PkgAppend 用: マージ前の材質オフセット
    pub mat_offset: usize,
    /// PkgAppend 用: マージ前のテクスチャ数
    pub tex_count_before: usize,
    /// PkgAppend 用: ソースパス
    pub source_path: PathBuf,
    /// アニメーション状態スナップショット
    pub anim_snapshot: AnimationSnapshot,
}

/// バックグラウンド PMX 変換の状態
pub struct PendingConvertBg {
    /// BG スレッドからの結果受信チャネル
    pub rx: std::sync::mpsc::Receiver<ConvertBgResult>,
    /// キャンセルフラグ
    pub cancel: Arc<AtomicBool>,
}

/// バックグラウンド PMX 変換の結果
pub struct ConvertBgResult {
    /// 変換結果: Ok(stats_message) or Err(error_message)
    pub result: Result<String, String>,
    /// ログバッファ書き込み（output_log=true の場合）
    pub log_written: bool,
    /// 警告付きメッセージか
    pub has_warning: bool,
    /// 出力ディレクトリ（成功時に開く用）
    pub output_dir: Option<PathBuf>,
}

/// 1 フレームにアップロードするテクスチャ枚数
pub const GPU_UPLOAD_BATCH: usize = 4;

/// 複数モデル一括ロード用キュー（assets を Arc 共有して clone コストを排除）
pub struct PendingMultiLoad {
    pub assets: Arc<Vec<crate::unitypackage::ExtractedAsset>>,
    /// 残りのモデル (fbx_index, model_type)
    pub remaining: Vec<(usize, PkgModelType)>,
    pub source_path: PathBuf,
    pub archive_snapshot: Option<Arc<[u8]>>,
    pub nested_archive_source: Option<ReloadableSource>,
    pub pkg_index: Option<Arc<UnityPackageIndex>>,
    /// Total number of models in batch (for progress display)
    pub total_count: usize,
}

impl Default for PendingState {
    fn default() -> Self {
        Self {
            fbx_choice: None,
            unity_pkg: None,
            pkg_load: None,
            archive: None,
            archive_load: None,
            bg_state: BackgroundLoadState::Idle,
            convert: None,
            rebuild: None,
            reload: None,
            refit: false,
            confirm_save_tex_history: false,
            file_dialog: None,
            import_options: None,
            multi_load: None,
            gpu_build: None,
            convert_bg: None,
        }
    }
}

/// UVマップ保存ダイアログの非同期結果待ち状態
pub struct PendingUvExport {
    /// ダイアログ結果の受信チャネル
    pub rx: std::sync::mpsc::Receiver<Option<std::path::PathBuf>>,
    /// UVマップ出力解像度
    pub uv_map_size: u32,
    /// 材質グループ情報（名前, 材質インデックス範囲）
    pub uv_groups: Vec<(String, std::ops::Range<usize>)>,
}

/// UVマップ BG エクスポートの結果待ち状態
pub struct PendingUvBgExport {
    /// BG スレッドからの結果受信チャネル（Ok=出力パス, Err=エラーメッセージ）
    pub rx: std::sync::mpsc::Receiver<Result<std::path::PathBuf, String>>,
}

/// PMX 変換前のディレクトリ作成 BG 処理の結果待ち状態
pub struct PendingMkdir {
    /// BG スレッドからの結果受信チャネル（Ok=成功, Err=エラーメッセージ）
    pub rx: std::sync::mpsc::Receiver<Result<(), String>>,
}

/// PMXエクスポート関連の状態
pub struct ExportState {
    /// PMX変換時にログファイルを出力するか
    pub output_log: bool,
    /// PMX出力パス（テキストボックス編集用）
    pub pmx_output_path: String,
    /// ユーザーに表示するモデル名（拡張子なし）。
    /// タイトルバー表示と PMX 出力ファイル名の両方に使われる。
    /// 初期値はロード元に応じて自動設定され、UI から編集可能。
    pub model_display_name: String,
    /// 表示材質のみPMX出力（デフォルト: false）
    pub export_visible_only: bool,
    /// UVマップ出力解像度
    pub uv_map_size: u32,
    /// 物理（剛体・ジョイント）なしで出力
    pub no_physics: bool,
    /// 元のボーン構造のまま出力（標準ボーン挿入スキップ）
    pub raw_structure: bool,
    /// PMX出力倍率（デフォルト: 1.0）
    pub scale: f32,
    /// converted_modelXX の作成先ベースディレクトリ（None = ソースファイルと同じ場所）
    pub output_base_dir: Option<std::path::PathBuf>,
    /// 非同期フォルダ選択ダイアログ（PMX出力先）
    pub pending_folder_dialog: Option<std::sync::mpsc::Receiver<Option<std::path::PathBuf>>>,
    /// 非同期UVマップ保存ダイアログ
    pub pending_uv_dialog: Option<PendingUvExport>,
    /// UVマップ BG エクスポートの結果待ち
    pub pending_uv_bg: Option<PendingUvBgExport>,
    /// PMX 変換前のディレクトリ作成 BG 処理の結果待ち
    pub pending_mkdir: Option<PendingMkdir>,
}

impl Default for ExportState {
    fn default() -> Self {
        Self {
            output_log: false,
            pmx_output_path: String::new(),
            model_display_name: String::new(),
            export_visible_only: false,
            uv_map_size: crate::convert::uvmap::DEFAULT_UV_SIZE,
            no_physics: false,
            raw_structure: false,
            scale: 1.0,
            output_base_dir: None,
            pending_folder_dialog: None,
            pending_uv_dialog: None,
            pending_uv_bg: None,
            pending_mkdir: None,
        }
    }
}

impl ViewerApp {
    /// プログレスオーバーレイ描画（処理中はキャンセルボタン付き）
    pub(super) fn paint_progress_overlay(
        &mut self,
        viewport: &egui::Ui,
        rect: egui::Rect,
        ctx: &egui::Context,
    ) {
        let is_bg_loading = self.pending.bg_state.is_active()
            || self.pending.pkg_load.is_some()
            || self.pending.archive_load.is_some();
        let is_gpu_building = self.pending.gpu_build.is_some();
        let is_converting_bg = self.pending.convert_bg.is_some();
        let msg = if is_bg_loading {
            Some("読み込み中...")
        } else if is_gpu_building {
            Some("GPU構築中...")
        } else if self.pending.rebuild.is_some() || self.pending.reload.is_some() {
            Some("処理中...")
        } else if self.export.pending_mkdir.is_some() {
            Some("ディレクトリ作成中...")
        } else if is_converting_bg || self.pending.convert.is_some() {
            Some("PMX変換中...")
        } else if self.export.pending_uv_bg.is_some() {
            Some("UVマップ出力中...")
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

        // キャンセル可能な処理中: 最前面 Area でキャンセルボタンを配置
        let is_cancellable = is_bg_loading || is_gpu_building || is_converting_bg;
        if is_cancellable {
            let btn_w = 56.0_f32;
            let btn_h = bar_h - 8.0;
            let btn_pos = egui::pos2(bar_rect.right() - btn_w - 12.0, bar_rect.top() + 4.0);

            egui::Area::new(egui::Id::new("bg_cancel_btn"))
                .fixed_pos(btn_pos)
                .order(egui::Order::Foreground)
                .interactable(true)
                .show(ctx, |ui| {
                    let btn = ui.add_sized(
                        [btn_w, btn_h],
                        egui::Button::new(
                            egui::RichText::new("中止")
                                .color(egui::Color32::WHITE)
                                .size(14.0),
                        )
                        .fill(egui::Color32::from_rgb(0xA0, 0x40, 0x40)),
                    );
                    if btn.clicked() {
                        if is_bg_loading {
                            self.cancel_bg_load();
                        } else if is_gpu_building {
                            self.cancel_gpu_build();
                        } else if is_converting_bg {
                            self.cancel_convert_bg();
                        }
                    }
                });

            // Esc キーでもキャンセル
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                if is_bg_loading {
                    self.cancel_bg_load();
                } else if is_gpu_building {
                    self.cancel_gpu_build();
                } else if is_converting_bg {
                    self.cancel_convert_bg();
                }
            }
        }
        ctx.request_repaint();
    }

    /// BGロードをキャンセルし、モデルと関連する全ての状態をクリアする
    fn cancel_bg_load(&mut self) {
        if let BackgroundLoadState::Loading(handle) =
            std::mem::replace(&mut self.pending.bg_state, BackgroundLoadState::Idle)
        {
            handle
                .cancel
                .store(true, std::sync::atomic::Ordering::Relaxed);
            log::info!("User cancelled bg load (req={})", handle.request_id);
        } else if let BackgroundLoadState::PendingDispatch {
            prior_loading: Some(handle),
            ..
        } = std::mem::replace(&mut self.pending.bg_state, BackgroundLoadState::Idle)
        {
            handle
                .cancel
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        // 待機中の全ての遅延処理をクリア
        self.pending.pkg_load = None;
        self.pending.archive_load = None;
        self.pending.multi_load = None;
        self.pending.gpu_build = None;
        self.pending.unity_pkg = None;
        self.pending.archive = None;

        // リロード中のキャンセル: スナップショットから旧状態を復元して旧モデルを残す
        if let Some(snap) = self.reload_snapshot.take() {
            self.restore_snapshot_on_failure(snap);
            self.convert_message = Some(ConvertMessage::success(
                "再読み込みを中止しました".to_string(),
            ));
            return;
        }

        // 通常ロードのキャンセル: モデルとアニメーション状態を全クリア
        self.loaded = None;
        self.anim.state = None;
        self.anim.library.clear();
        self.anim.active_index = None;
        self.tex.pkg_textures = None;
        self.clear_pkg_thumb_cache();
        self.cancel_tex_match_preview();
        self.tex.pending_match = None;
        self.selected_fbx_name = None;
        self.selected_pkg_model = None;
        self.material_display.clear();
        self.morph_weights.clear();
        self.material_visibility.clear();

        // レンダラキャッシュ無効化
        if let Some(ref mut renderer) = self.renderer {
            renderer.invalidate_visualization_cache();
            renderer.invalidate_normal_cache();
        }

        self.convert_message = Some(ConvertMessage::success(
            "読み込みを中止しました".to_string(),
        ));
    }

    /// GPU 構築（テクスチャアップロード）をキャンセルし、状態をクリアする
    fn cancel_gpu_build(&mut self) {
        if self.pending.gpu_build.is_some() {
            log::info!("User cancelled GPU build");
            self.pending.gpu_build = None;
        }

        // リロード中のキャンセル: スナップショットから旧状態を復元して旧モデルを残す
        if let Some(snap) = self.reload_snapshot.take() {
            self.restore_snapshot_on_failure(snap);
            self.convert_message = Some(ConvertMessage::success(
                "再読み込みを中止しました".to_string(),
            ));
            return;
        }

        // 通常ロードのキャンセル: モデルとアニメーション状態を全クリア
        self.loaded = None;
        self.anim.state = None;
        self.anim.library.clear();
        self.anim.active_index = None;
        self.tex.pkg_textures = None;
        self.clear_pkg_thumb_cache();
        self.cancel_tex_match_preview();
        self.tex.pending_match = None;
        self.selected_fbx_name = None;
        self.selected_pkg_model = None;
        self.material_display.clear();
        self.morph_weights.clear();
        self.material_visibility.clear();

        if let Some(ref mut renderer) = self.renderer {
            renderer.invalidate_visualization_cache();
            renderer.invalidate_normal_cache();
        }

        self.convert_message = Some(ConvertMessage::success("GPU構築を中止しました".to_string()));
    }

    /// バックグラウンド PMX 変換をキャンセルする
    fn cancel_convert_bg(&mut self) {
        if let Some(ref handle) = self.pending.convert_bg {
            handle
                .cancel
                .store(true, std::sync::atomic::Ordering::Relaxed);
            log::info!("User cancelled background PMX conversion");
        }
        self.pending.convert_bg = None;
        self.convert_message = Some(ConvertMessage::success("PMX変換を中止しました".to_string()));
    }

    /// プログレスフラグ更新（次フレームで処理を実行するためのトリガー）
    pub(super) fn update_progress_flags(&mut self, ctx: &egui::Context) {
        if let BackgroundLoadState::PendingDispatch {
            ref mut dispatch, ..
        } = self.pending.bg_state
        {
            if dispatch.overlay == PendingOverlay::WaitingOverlay {
                dispatch.overlay = PendingOverlay::Ready;
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

    /// Set progress toast for batch model loading
    #[allow(dead_code)]
    fn set_batch_progress_message(
        &mut self,
        batch_progress: &Option<(usize, usize)>,
        model_name: &str,
    ) {
        if let Some((current, total)) = *batch_progress {
            let msg = format!("読み込み完了 ({}/{}): {}", current, total, model_name);
            self.convert_message = Some(ConvertMessage::success(msg));
        } else {
            self.convert_message = None;
        }
    }

    /// 遅延処理（ファイル読み込み、GPU再構築、PMX変換など）を実行
    pub(super) fn process_pending_tasks(&mut self, ctx: &egui::Context) {
        self.poll_pending_psd_conversions();
        self.poll_file_dialog();
        self.poll_dispatch_and_bg_load();
        self.poll_deferred_loads();
        self.poll_gpu_build(ctx);
        self.poll_export_tasks(ctx);
        self.poll_overlay_tasks(ctx);
        self.poll_convert_bg();
    }

    /// Non-blocking poll of an `mpsc::Receiver`.
    /// Returns `Some(value)` when a message is ready, `None` otherwise.
    /// Sets `*alive` to `false` on disconnect so the caller can clear the slot.
    fn poll_receiver<T>(rx: &std::sync::mpsc::Receiver<T>, alive: &mut bool) -> Option<T> {
        match rx.try_recv() {
            Ok(v) => Some(v),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                *alive = false;
                None
            }
        }
    }

    fn poll_file_dialog(&mut self) {
        if let Some((kind, ref rx)) = self.pending.file_dialog {
            let mut alive = true;
            if let Some(opt_path) = Self::poll_receiver(rx, &mut alive) {
                if let Some(path) = opt_path {
                    if let Some(dir) = path.parent() {
                        self.last_model_dir = Some(dir.to_path_buf());
                    }
                    let append = kind == FileDialogKind::Append;
                    self.pending.bg_state.submit_dispatch(PendingLoadDispatch {
                        path,
                        append,
                        overlay: PendingOverlay::WaitingOverlay,
                        preloaded: None,
                        is_reload: false,
                    });
                }
                self.pending.file_dialog = None;
            } else if !alive {
                self.pending.file_dialog = None;
            }
        }
    }

    fn poll_dispatch_and_bg_load(&mut self) {
        // PendingDispatch が Ready → メインスレッドルーティング
        let dispatch_ready = matches!(
            &self.pending.bg_state,
            BackgroundLoadState::PendingDispatch { dispatch, .. }
                if dispatch.overlay == PendingOverlay::Ready
        );
        if dispatch_ready {
            let (dispatch, prior_loading) =
                match std::mem::replace(&mut self.pending.bg_state, BackgroundLoadState::Idle) {
                    BackgroundLoadState::PendingDispatch {
                        dispatch,
                        prior_loading,
                    } => (dispatch, prior_loading),
                    _ => unreachable!("dispatch_ready で PendingDispatch 確認済み"),
                };
            self.route_load_dispatch(dispatch, prior_loading);
        }

        // バックグラウンド CPU パース結果をポーリング
        if let BackgroundLoadState::Loading(ref handle) = self.pending.bg_state {
            let current_id = handle.request_id;
            match handle.rx.try_recv() {
                Ok(Ok(result)) => {
                    if result.request_id != current_id {
                        log::info!(
                            "Discarding stale bg load result (req={}, current={})",
                            result.request_id,
                            current_id
                        );
                    } else {
                        self.pending.bg_state = BackgroundLoadState::Idle;
                        if let Err(e) = self.apply_bg_load_result(result) {
                            self.convert_message =
                                Some(ConvertMessage::failure(format!("{:#}", e)));
                        }
                    }
                }
                Ok(Err(e)) => {
                    self.pending.bg_state = BackgroundLoadState::Idle;
                    let msg = format!("{:#}", e);
                    if msg.contains("bg load cancelled") {
                        log::info!("Bg load cancelled (req={})", current_id);
                    } else {
                        self.convert_message = Some(ConvertMessage::failure(msg));
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.pending.bg_state = BackgroundLoadState::Idle;
                    self.convert_message = Some(ConvertMessage::failure(
                        "Background load thread panicked".to_string(),
                    ));
                }
            }
        }
    }

    fn poll_deferred_loads(&mut self) {
        // unitypackage モデル遅延読み込み → BGスレッドへ投入
        if self.pending.pkg_load.as_ref().is_some_and(|p| p.shown) {
            let p = self
                .pending
                .pkg_load
                .take()
                .expect("pending_pkg_load は shown 確認済み");
            self.spawn_bg_pkg_load(p);
        }

        // 複数モデル一括ロードキューの処理
        // pkg_load と fbx_choice の両方が空、かつ bg_state が idle のときのみ次を投入
        if self.pending.pkg_load.is_none()
            && self.pending.fbx_choice.is_none()
            && self.pending.bg_state.is_idle()
        {
            if let Some(ref mut ml) = self.pending.multi_load {
                if let Some((fbx_index, model_type)) = ml.remaining.pop() {
                    let current = ml.total_count - ml.remaining.len();
                    self.pending.pkg_load = Some(PendingPkgModelLoad {
                        assets: ml.assets.clone(),
                        fbx_index,
                        model_type,
                        source_path: ml.source_path.clone(),
                        shown: false,
                        append: true,
                        suppress_tex_match: false,
                        archive_snapshot: ml.archive_snapshot.clone(),
                        nested_archive_source: ml.nested_archive_source.clone(),
                        pkg_index: ml.pkg_index.clone(),
                        batch_progress: Some((current, ml.total_count)),
                        skip_anim_check: false,
                    });
                }
            }
            if self
                .pending
                .multi_load
                .as_ref()
                .is_some_and(|ml| ml.remaining.is_empty())
            {
                self.pending.multi_load = None;
            }
        }

        // アーカイブモデル遅延読み込み → BGスレッドへ投入
        if self.pending.archive_load.as_ref().is_some_and(|p| p.shown) {
            let p = self
                .pending
                .archive_load
                .take()
                .expect("pending_archive_load は shown 確認済み");
            self.spawn_bg_archive_load(p);
        }
    }

    fn poll_gpu_build(&mut self, ctx: &egui::Context) {
        if self.pending.gpu_build.is_none() {
            return;
        }
        let render_state = &self.render_state;
        let pending = &mut self.pending;
        let gb = pending.gpu_build.as_mut().unwrap();
        let device = &render_state.device;
        let queue = &render_state.queue;
        let total = gb.ir.textures.len();

        // Phase 1: テクスチャアップロード（フレーム分割）
        if gb.next_tex < total && gb.cpu_prep_rx.is_none() {
            let end = (gb.next_tex + GPU_UPLOAD_BATCH).min(total);
            for i in gb.next_tex..end {
                let view = super::super::texture::upload_single_texture(
                    &gb.ir.textures[i],
                    i,
                    device,
                    queue,
                );
                gb.gpu_textures.push(view);
            }
            gb.next_tex = end;

            // テクスチャ完了 → Phase 2: BG スレッドで CPU prep 開始
            if gb.next_tex >= total {
                log::info!(
                    "[gpu_build] Phase 1 done: {total} textures uploaded, spawning BG cpu_prep"
                );
                let ir = std::mem::take(&mut gb.ir);
                let flags = gb.mat_flags.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                gb.cpu_prep_rx = Some(rx);
                let repaint = ctx.clone();
                std::thread::Builder::new()
                    .name("gpu_cpu_prep".into())
                    .spawn(move || {
                        let result = super::super::mesh::cpu_prep_model(&ir, &flags);
                        let _ = tx.send(result.map(|prep| (prep, ir)));
                        repaint.request_repaint();
                    })
                    .expect("gpu_cpu_prep thread spawn");
            }
        }

        // Phase 3: CPU prep 結果ポーリング → GPU finalize
        if let Some(ref rx) = gb.cpu_prep_rx {
            match rx.try_recv() {
                Ok(Ok((prep, ir))) => {
                    log::info!(
                        "[gpu_build] Phase 2 done: cpu_prep complete (verts={}, mats={}), starting GPU finalize",
                        prep.all_vertices.len(),
                        prep.draw_plans.len()
                    );
                    let mut gb = self.pending.gpu_build.take().unwrap();
                    gb.ir = ir;
                    let device = &self.render_state.device;
                    let queue = &self.render_state.queue;
                    let t0 = std::time::Instant::now();
                    match super::super::mesh::gpu_finalize_model(
                        prep,
                        gb.gpu_textures,
                        device,
                        queue,
                    ) {
                        Ok(gpu_model) => {
                            log::info!(
                                "[gpu_build] GPU finalize done in {}ms",
                                t0.elapsed().as_millis()
                            );
                            if let Some(ai) = gb.append_info {
                                let t1 = std::time::Instant::now();
                                self.finish_deferred_append(gb.ir, gpu_model, *ai);
                                log::info!(
                                    "[gpu_build] finish_deferred_append done in {}ms",
                                    t1.elapsed().as_millis()
                                );
                            } else {
                                let t1 = std::time::Instant::now();
                                match self.finish_load_with_gpu(gb.ir, gpu_model, gb.source) {
                                    Ok(()) => {
                                        log::info!(
                                            "[gpu_build] finish_load_with_gpu done in {}ms",
                                            t1.elapsed().as_millis()
                                        );
                                        self.apply_gpu_build_post(gb.post_kind, &gb.path);
                                        if self.reload_snapshot.is_some() {
                                            self.finish_reload_from_snapshot();
                                        }
                                    }
                                    Err(e) => {
                                        if let Some(snap) = self.reload_snapshot.take() {
                                            self.restore_snapshot_on_failure(snap);
                                        }
                                        self.convert_message =
                                            Some(ConvertMessage::failure(format!("{:#}", e)));
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            if let Some(ai) = gb.append_info {
                                log::error!("Append GPU rebuild failed, rolling back: {:#}", e);
                                self.rollback_append(gb.ir, *ai);
                                self.convert_message = Some(ConvertMessage::failure(format!(
                                    "追加読み込みの GPU 構築に失敗しました。元のモデルに戻しました。\n詳細: {:#}",
                                    e
                                )));
                            } else {
                                self.convert_message = Some(ConvertMessage::failure(format!(
                                    "GPU build failed: {:#}",
                                    e
                                )));
                            }
                        }
                    }
                }
                Ok(Err(e)) => {
                    let gb = self.pending.gpu_build.take().unwrap();
                    if let Some(ai) = gb.append_info {
                        log::error!("CPU prep failed, rolling back: {:#}", e);
                        self.rollback_append(gb.ir, *ai);
                        self.convert_message = Some(ConvertMessage::failure(format!(
                            "追加読み込みの CPU 処理に失敗しました。元のモデルに戻しました。\n詳細: {:#}",
                            e
                        )));
                    } else {
                        self.convert_message =
                            Some(ConvertMessage::failure(format!("CPU prep failed: {:#}", e)));
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    let gb = self.pending.gpu_build.take().unwrap();
                    if let Some(ai) = gb.append_info {
                        self.rollback_append(gb.ir, *ai);
                    }
                    self.convert_message = Some(ConvertMessage::failure(
                        "GPU build CPU prep thread panicked".to_string(),
                    ));
                }
            }
        }
    }

    fn poll_export_tasks(&mut self, ctx: &egui::Context) {
        self.poll_folder_dialog();
        self.poll_uv_dialog(ctx);
        self.poll_uv_bg_export();
        self.poll_mkdir();
    }

    fn poll_folder_dialog(&mut self) {
        if let Some(ref rx) = self.export.pending_folder_dialog {
            let mut alive = true;
            if let Some(opt_dir) = Self::poll_receiver(rx, &mut alive) {
                if let Some(dir) = opt_dir {
                    self.export.output_base_dir = Some(dir);
                }
                self.export.pending_folder_dialog = None;
            } else if !alive {
                self.export.pending_folder_dialog = None;
            }
        }
    }

    fn poll_uv_dialog(&mut self, ctx: &egui::Context) {
        if let Some(ref pending_uv) = self.export.pending_uv_dialog {
            let mut alive = true;
            if let Some(opt_path) = Self::poll_receiver(&pending_uv.rx, &mut alive) {
                if let Some(path) = opt_path {
                    let uv_map_size = pending_uv.uv_map_size;
                    let uv_groups = pending_uv.uv_groups.clone();
                    self.export.pending_uv_dialog = None;
                    if let Some(ref loaded) = self.loaded {
                        let minimal_ir = build_minimal_ir_for_uv(&loaded.ir);
                        let (tx, rx) = std::sync::mpsc::channel();
                        let repaint = ctx.clone();
                        std::thread::spawn(move || {
                            log::info!("Starting UV map export in background thread");
                            let result = crate::convert::uvmap::export_uv_map_grouped(
                                &minimal_ir,
                                &path,
                                uv_map_size,
                                &uv_groups,
                            );
                            let _ = tx.send(match result {
                                Ok(()) => Ok(path),
                                Err(e) => Err(format!("{e}")),
                            });
                            repaint.request_repaint();
                        });
                        self.export.pending_uv_bg = Some(PendingUvBgExport { rx });
                    } else {
                        self.convert_message = Some(ConvertMessage::failure(
                            "モデルが読み込まれていません".to_string(),
                        ));
                    }
                } else {
                    self.export.pending_uv_dialog = None;
                }
            } else if !alive {
                self.export.pending_uv_dialog = None;
            }
        }
    }

    fn poll_uv_bg_export(&mut self) {
        if let Some(ref pending_uv_bg) = self.export.pending_uv_bg {
            let mut alive = true;
            if let Some(result) = Self::poll_receiver(&pending_uv_bg.rx, &mut alive) {
                self.export.pending_uv_bg = None;
                match result {
                    Ok(path) => {
                        self.convert_message = Some(ConvertMessage::success(format!(
                            "UVマップ出力完了: {}",
                            path.display()
                        )));
                    }
                    Err(e) => {
                        self.convert_message =
                            Some(ConvertMessage::failure(format!("UVマップ出力失敗: {e}")));
                    }
                }
            } else if !alive {
                self.export.pending_uv_bg = None;
            }
        }
    }

    fn poll_mkdir(&mut self) {
        if let Some(ref pending_mkdir) = self.export.pending_mkdir {
            let mut alive = true;
            if let Some(result) = Self::poll_receiver(&pending_mkdir.rx, &mut alive) {
                self.export.pending_mkdir = None;
                if let Err(e) = result {
                    log::warn!("Failed to create output directory in background: {e}");
                }
                // Both success and failure proceed to conversion
                self.pending.convert = Some(PendingOverlay::WaitingOverlay);
            } else if !alive {
                self.export.pending_mkdir = None;
            }
        }
    }

    fn poll_overlay_tasks(&mut self, ctx: &egui::Context) {
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
            super::super::ui::execute_conversion(self, ctx);
        }
    }

    fn poll_convert_bg(&mut self) {
        if let Some(ref pending_convert) = self.pending.convert_bg {
            let mut alive = true;
            if let Some(bg_result) = Self::poll_receiver(&pending_convert.rx, &mut alive) {
                self.pending.convert_bg = None;
                match bg_result.result {
                    Ok(msg) => {
                        if bg_result.has_warning {
                            self.convert_message = Some(ConvertMessage::warning(msg));
                        } else {
                            self.convert_message = Some(ConvertMessage::success(msg));
                        }
                        if let Some(ref dir) = bg_result.output_dir {
                            super::helpers::open_directory(dir);
                        }
                    }
                    Err(msg) => {
                        self.convert_message = Some(ConvertMessage::failure(msg));
                    }
                }
            } else if !alive {
                self.pending.convert_bg = None;
                self.convert_message = Some(ConvertMessage::failure(
                    "PMX変換スレッドが異常終了しました".to_string(),
                ));
            }
        }
    }
}

/// UVマップエクスポートに必要な最小データだけを IrModel として抽出する。
/// テクスチャ・ボーン等の重いデータはコピーしない。
fn build_minimal_ir_for_uv(ir: &IrModel) -> IrModel {
    use crate::intermediate::types::IrMesh;
    let meshes = ir
        .meshes
        .iter()
        .map(|m| IrMesh {
            name: String::new(),
            vertices: Arc::clone(&m.vertices),
            indices: Arc::clone(&m.indices),
            material_index: m.material_index,
            morph_targets: Arc::new(Vec::new()),
            node_index: 0,
            uvs1: vec![],
        })
        .collect();
    IrModel {
        materials: ir.materials.clone(),
        meshes,
        ..Default::default()
    }
}
