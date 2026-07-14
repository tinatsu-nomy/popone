//! Deferred task processing (PendingState, ExportState, process_pending_tasks, etc.).

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use eframe::egui;
use rust_i18n::t;

use crate::unitypackage::UnityPackageIndex;

use crate::intermediate::types::IrModel;

use super::file_io::FileFormat;
use super::helpers::{PkgModelType, PreloadedData, ReloadableSource};
use super::{ConvertMessage, ViewerApp};

/// Wait state when a unitypackage contains multiple FBXs (model selection pending).
pub struct PendingUnityPackage {
    pub assets: Vec<crate::unitypackage::ExtractedAsset>,
    /// (asset index, file name, model kind)
    pub model_list: Vec<(usize, String, PkgModelType)>,
    pub source_path: PathBuf,
    /// Append mode (add to the existing model).
    pub append: bool,
    /// Snapshot of the archive data when loaded from a temp file.
    pub archive_snapshot: Option<Arc<[u8]>>,
    /// For .unitypackage inside an archive (ZIP / 7z): the source info used for reload.
    pub nested_archive_source: Option<ReloadableSource>,
    /// Phase 3: package index (used to resolve Prefab textures).
    pub pkg_index: Option<Arc<UnityPackageIndex>>,
    /// Per-entry checked state for multi-select (same length as model_list).
    pub checked: Vec<bool>,
}

/// Deferred load state for a unitypackage model.
pub struct PendingPkgModelLoad {
    pub assets: Arc<Vec<crate::unitypackage::ExtractedAsset>>,
    pub fbx_index: usize,
    pub model_type: PkgModelType,
    pub source_path: PathBuf,
    /// Whether the overlay has been shown.
    pub shown: bool,
    /// Append mode (add to the existing model).
    pub append: bool,
    /// Suppress the texture matching dialog (when reaching this via reload).
    pub suppress_tex_match: bool,
    /// Snapshot of the archive data when loaded from a temp file.
    pub archive_snapshot: Option<Arc<[u8]>>,
    /// For .unitypackage inside an archive (ZIP / 7z): the source info used for reload.
    pub nested_archive_source: Option<ReloadableSource>,
    /// Phase 3: package index (used to resolve Prefab textures).
    pub pkg_index: Option<Arc<UnityPackageIndex>>,
    /// Batch progress: (current, total) — carried from PendingMultiLoad at queue pop time.
    pub batch_progress: Option<(usize, usize)>,
    /// Skip the FBX animation choice dialog (used when re-submitting after execute_fbx_choice resolves).
    pub skip_anim_check: bool,
}

/// Pending model selection inside an archive.
pub struct PendingArchive {
    pub archive_data: Arc<[u8]>,
    pub format: crate::archive::ArchiveFormat,
    pub contents: crate::archive::ArchiveContents,
    pub source_path: PathBuf,
    pub append: bool,
    pub is_temp: bool,
    /// Password entered for this archive (in-memory only, never persisted).
    pub password: Option<String>,
}

/// Deferred load for a model inside an archive.
pub struct PendingArchiveLoad {
    pub archive_data: Arc<[u8]>,
    pub format: crate::archive::ArchiveFormat,
    pub contents: crate::archive::ArchiveContents,
    pub model_index: usize,
    pub source_path: PathBuf,
    pub shown: bool,
    pub append: bool,
    pub is_temp: bool,
    /// Password entered for this archive (in-memory only, never persisted).
    pub password: Option<String>,
}

/// State for the archive password-input dialog. Created when a background
/// load fails with `ArchivePasswordPrompt`; the dialog fills `input` and sets
/// `submitted`, then `poll_deferred_loads` restarts the load with the password.
/// The password lives only in this struct and the in-flight load request --
/// it is never written to `popone.toml` or kept after the load finishes.
pub struct PendingArchivePassword {
    pub path: PathBuf,
    pub append: bool,
    /// ZIP extract-stage retry: auto-select this internal model path after
    /// re-listing, so the user does not have to pick the model again.
    pub auto_select_model: Option<PathBuf>,
    /// Text-field buffer bound to the dialog.
    pub input: String,
    /// Error line shown in the dialog (set after a wrong password).
    pub error: Option<String>,
    /// True once the user pressed OK; consumed by `poll_deferred_loads`.
    pub submitted: bool,
}

/// Marker error raised by the BG parse thread when an archive needs a
/// password (or the supplied one was wrong). Carries enough context for the
/// main thread to reopen the password dialog and retry the load.
pub struct ArchivePasswordPrompt {
    pub path: PathBuf,
    pub append: bool,
    pub auto_select_model: Option<PathBuf>,
    /// True when a password was supplied but rejected.
    pub bad_password: bool,
    /// Listing-stage failures attach the outer archive's text documents
    /// (readme etc., extracted without nested expansion) so the text viewer
    /// works while the password dialog is up — MMD readmes typically hold the
    /// password hint. `None` at the extract stage (texts were already set at
    /// listing time and must be kept).
    pub texts: Option<Vec<(PathBuf, Vec<u8>)>>,
}

impl std::fmt::Debug for ArchivePasswordPrompt {
    // Manual impl: skip `texts` payloads (a derived Debug would dump the
    // readme bytes into logs whenever the error is debug-formatted).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArchivePasswordPrompt")
            .field("path", &self.path)
            .field("append", &self.append)
            .field("auto_select_model", &self.auto_select_model)
            .field("bad_password", &self.bad_password)
            .field("texts", &self.texts.as_ref().map(|t| t.len()))
            .finish()
    }
}

impl std::fmt::Display for ArchivePasswordPrompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.bad_password {
            write!(f, "{}", t!("error.archive_bad_password"))
        } else {
            write!(f, "{}", t!("error.archive_password_required"))
        }
    }
}

impl std::error::Error for ArchivePasswordPrompt {}

/// State for the FBX-load-method dialog (used when an FBX contains both model and animation).
pub struct PendingFbxChoice {
    pub path: PathBuf,
    pub load_model: bool,
    pub load_animation: bool,
    /// Extra data when arriving via unitypackage.
    pub pkg_context: Option<PendingFbxChoicePkg>,
    /// Pre-read data for the D&D temp file.
    pub preloaded: Option<super::helpers::PreloadedData>,
}

/// Extra context for the FBX-choice dialog when arriving via unitypackage.
pub struct PendingFbxChoicePkg {
    pub assets: Arc<Vec<crate::unitypackage::ExtractedAsset>>,
    pub fbx_index: usize,
    pub source_path: PathBuf,
    /// Snapshot of the archive data when loaded from a temp file.
    pub archive_snapshot: Option<Arc<[u8]>>,
    /// For .unitypackage inside an archive (ZIP / 7z): the source info used for reload.
    pub nested_archive_source: Option<ReloadableSource>,
    /// Phase 3: package index (used to resolve Prefab textures).
    pub pkg_index: Option<Arc<UnityPackageIndex>>,
}

/// Overlay-display state for a deferred process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingOverlay {
    /// Overlay not yet shown (will be shown next frame).
    WaitingOverlay,
    /// Overlay shown (will execute next frame).
    Ready,
}

/// Load dispatch (single entry point for dialog / D&D / IPC).
pub struct PendingLoadDispatch {
    pub path: PathBuf,
    pub append: bool,
    pub overlay: PendingOverlay,
    /// Pre-read data for D&D temp files (moved from self.preloaded).
    pub preloaded: Option<PreloadedData>,
    /// Whether this dispatch comes via reload_current. When true, route_load_dispatch
    /// skips the new-load-style state reset (normalize_pose, etc.) and proceeds to
    /// BG parse with the user's settings on the current model preserved.
    pub is_reload: bool,
}

/// Result of background CPU parsing.
pub struct BgLoadResult {
    pub ir: IrModel,
    pub source: ReloadableSource,
    pub kind: BgLoadKind,
    pub path: PathBuf,
    /// Generation number of the originating dispatch. A result whose request_id does
    /// not match the current `BgLoadHandle.request_id` represents "an old load
    /// finished, but the user has already moved on to the next load" and is discarded.
    pub request_id: u64,
}

/// Background CPU parse handle (receiver channel + cancel token + generation number).
pub struct BgLoadHandle {
    pub rx: std::sync::mpsc::Receiver<anyhow::Result<BgLoadResult>>,
    /// Cancel signal sent to the parse running on a separate thread. Set to `true` when a new load is submitted.
    pub cancel: Arc<AtomicBool>,
    pub request_id: u64,
}

/// Load kind (used by the post-processing branch).
pub enum BgLoadKind {
    /// Standard load (format + FBX auto-animation flag).
    Initial {
        format: FileFormat,
        auto_fbx_anim: bool,
    },
    /// Append load.
    Append,
    /// First load of a model inside an archive (already extracted + parsed on the BG thread).
    ArchiveInitial,
    /// Append load of a model inside an archive (already extracted + parsed on the BG thread).
    ArchiveAppend,
    /// .unitypackage inside an archive — pkg_index already built on the BG thread.
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
    /// First load of a model inside a UnityPackage (already parsed on the BG thread).
    PkgInitial(Box<PkgInitialPayload>),
    /// Append load of a model inside a UnityPackage (already parsed on the BG thread).
    PkgAppend(Box<PkgAppendPayload>),
    /// Waiting for FBX animation choice (parsed IR is held).
    NeedsFbxChoice(Box<PkgFbxChoicePayload>),
    /// UnityPackage index built (the main thread sets up PendingUnityPackage / PkgModelLoad).
    UnityPackageIndexed {
        pkg_index: Arc<crate::unitypackage::UnityPackageIndex>,
        assets: Vec<crate::unitypackage::ExtractedAsset>,
        model_list: Vec<(usize, String, PkgModelType)>,
        source_path: PathBuf,
        is_temp: bool,
        archive_snapshot: Option<Arc<[u8]>>,
        append: bool,
    },
    /// Archive listed (the main thread sets up PendingArchive / ArchiveLoad).
    ArchiveIndexed {
        archive_data: Arc<[u8]>,
        format: crate::archive::ArchiveFormat,
        contents: crate::archive::ArchiveContents,
        source_path: PathBuf,
        is_temp: bool,
        append: bool,
        /// Password used for listing (forwarded to the extract stage).
        password: Option<String>,
        /// Password-retry: skip the selection dialog and reload this model.
        auto_select_model: Option<PathBuf>,
    },
}

/// Payload for PkgInitial.
pub struct PkgInitialPayload {
    pub fbx_name: Option<String>,
    pub pkg_model_locator: Option<crate::unitypackage::PkgModelLocator>,
    pub pkg_textures_legacy: Vec<(String, Arc<[u8]>)>,
    pub unmatched_indices: Vec<usize>,
    pub pkg_material_keys: Vec<Option<crate::unitypackage::PkgMaterialKey>>,
    /// Prefab FBX ranges (name, mat_start, mat_count).
    pub fbx_ranges: Vec<(String, usize, usize)>,
    pub batch_progress: Option<(usize, usize)>,
    pub suppress_tex_match: bool,
    /// Prefab name (used for the file name display).
    pub prefab_name: Option<String>,
    /// Entry path of the Prefab.
    pub prefab_entry_path: Option<String>,
}

/// Payload for PkgAppend.
pub struct PkgAppendPayload {
    pub pkg_model_name: Option<String>,
    pub pkg_model_locator: Option<crate::unitypackage::PkgModelLocator>,
    pub pkg_textures_to_add: Vec<(String, Arc<[u8]>)>,
    pub pkg_unmatched: Vec<usize>,
    pub batch_progress: Option<(usize, usize)>,
    pub suppress_tex_match: bool,
    pub pkg_material_keys: Vec<Option<crate::unitypackage::PkgMaterialKey>>,
}

/// Payload while waiting for the FBX animation choice.
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

/// Kind of the asynchronous file dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileDialogKind {
    /// Open model / animation.
    Open,
    /// Append model load.
    Append,
}

/// State machine for background loading.
///
/// The previous representation used `load_dispatch: Option<PendingLoadDispatch>` and
/// `bg_load: Option<BgLoadHandle>` side by side, which left "both Some" or "one
/// stays behind when both should be None" as legal but invalid states. The
/// enum unifies them so those states are unrepresentable at the type level.
///
/// State transitions:
/// - `Idle` -> `PendingDispatch`: file-dialog result / D&D / IPC / command-line argument.
/// - `PendingDispatch` -> `Idle` or `Loading`: `route_load_dispatch` chooses immediate-execute or spawn_bg_load.
/// - `Loading` -> `Idle`: result received from the BG thread.
/// - `Loading` -> `PendingDispatch { prior_loading: Some(..) }`: when a new dispatch is
///   submitted during Loading, the prior handle is carried as `prior_loading`, and
///   `route_load_dispatch` decides whether to cancel (model request) or protect
///   (animation-only request) based on intent.
pub enum BackgroundLoadState {
    /// Nothing is running.
    Idle,
    /// A dispatch is queued. `route_load_dispatch` will be called next frame.
    /// `prior_loading` is the prior handle when a new dispatch was submitted during Loading.
    PendingDispatch {
        dispatch: PendingLoadDispatch,
        prior_loading: Option<BgLoadHandle>,
    },
    /// BG thread is parsing. Waiting for the result.
    Loading(BgLoadHandle),
}

impl BackgroundLoadState {
    pub fn is_idle(&self) -> bool {
        matches!(self, BackgroundLoadState::Idle)
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, BackgroundLoadState::Loading(_))
    }

    /// Some load processing is in progress (dispatch queued or BG parse in flight).
    pub fn is_active(&self) -> bool {
        !self.is_idle()
    }

    /// Submit a new dispatch.
    /// If a Loading is in flight, it is carried as `prior_loading` and the cancel
    /// decision is made by `route_load_dispatch` based on intent (model vs.
    /// animation-only request).
    /// If already in `PendingDispatch`, the old dispatch is dropped and only its `prior_loading` is carried over.
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

/// Unit choice for OBJ / STL import.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImportUnit {
    Mm,
    Cm,
    M,
    Inch,
}

impl ImportUnit {
    /// Scale factor into glTF space (meters).
    pub fn scale(self) -> f32 {
        match self {
            ImportUnit::Mm => 0.001,
            ImportUnit::Cm => 0.01,
            ImportUnit::M => 1.0,
            ImportUnit::Inch => 0.0254,
        }
    }
    pub fn label(self) -> std::borrow::Cow<'static, str> {
        match self {
            ImportUnit::Mm => t!("viewer.import_unit.mm"),
            ImportUnit::Cm => t!("viewer.import_unit.cm"),
            ImportUnit::M => t!("viewer.import_unit.m"),
            ImportUnit::Inch => t!("viewer.import_unit.inch"),
        }
    }
}

/// Wait state for OBJ / STL import options.
pub struct PendingImportOptions {
    pub path: PathBuf,
    pub format: FileFormat,
    pub append: bool,
    pub preloaded: Option<PreloadedData>,
    pub unit: ImportUnit,
    pub z_up: bool,
}

/// Aggregated state of all pending (deferred) processes.
pub struct PendingState {
    /// Waiting for the FBX-load-method choice (when both model and animation are in the FBX).
    pub fbx_choice: Option<PendingFbxChoice>,
    /// Waiting for unitypackage FBX selection.
    pub unity_pkg: Option<PendingUnityPackage>,
    /// Deferred FBX load.
    pub pkg_load: Option<PendingPkgModelLoad>,
    /// Waiting for in-archive model selection.
    pub archive: Option<PendingArchive>,
    /// Deferred archive model load.
    pub archive_load: Option<PendingArchiveLoad>,
    /// Waiting for archive password input.
    pub archive_password: Option<PendingArchivePassword>,
    /// State machine for background loading
    /// (unifies the dispatch reservation and the BG parse handle at the type level).
    pub bg_state: BackgroundLoadState,
    /// Deferred PMX conversion.
    pub convert: Option<PendingOverlay>,
    /// Deferred GPU rebuild.
    pub rebuild: Option<PendingOverlay>,
    /// Deferred model reload.
    pub reload: Option<PendingOverlay>,
    /// Refit after the viewport size is finalized (on first load).
    pub refit: bool,
    /// Flag to show the "overwrite save texture history" confirmation dialog.
    pub confirm_save_tex_history: bool,
    /// Asynchronous file dialog (kind, result receiver channel).
    pub file_dialog: Option<(FileDialogKind, std::sync::mpsc::Receiver<Option<PathBuf>>)>,
    /// Wait state for OBJ / STL import options.
    pub import_options: Option<PendingImportOptions>,
    /// Multi-model batch load (holds assets only once and clones at dequeue time).
    pub multi_load: Option<PendingMultiLoad>,
    /// GPU texture upload spread across frames (after BG parse completes).
    pub gpu_build: Option<PendingGpuBuild>,
    /// State of the background PMX conversion.
    pub convert_bg: Option<PendingConvertBg>,
}

/// State for splitting GPU texture upload + CPU prep + GPU finalize across frames.
pub struct PendingGpuBuild {
    pub ir: IrModel,
    pub source: super::helpers::ReloadableSource,
    /// Uploaded texture views (accumulated in order).
    pub gpu_textures: Vec<(eframe::wgpu::TextureView, eframe::wgpu::TextureView)>,
    /// Index of the next texture to upload.
    pub next_tex: usize,
    /// Flags extracted from material_display.
    pub mat_flags: super::super::mesh::MaterialBuildFlags,
    /// Post-processing kind for apply_bg_load_result (PkgInitial etc.).
    pub post_kind: Option<BgLoadKind>,
    /// Result path (for logging).
    pub path: PathBuf,
    /// Extra info on Append (None = first load).
    pub append_info: Option<Box<AppendGpuBuildInfo>>,
    /// Receiver channel for the CPU prep running on the BG thread.
    pub(crate) cpu_prep_rx: Option<
        std::sync::mpsc::Receiver<anyhow::Result<(super::super::mesh::CpuPrepResult, IrModel)>>,
    >,
    /// Whether the dispatch came via `reload_current` (Step 2-10 fix).
    /// Captures `self.reload_snapshot.is_some()` at `start_deferred_gpu_build` time
    /// and carries it reliably through to `finish_load_with_gpu`. review_004 [P2] fix.
    pub is_reload: bool,
}

/// IR-size snapshot before merge (used for rollback).
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
    /// Capture a snapshot of the current IrModel state.
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

    /// Rewind a merged IR back to the snapshot state.
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

/// Snapshot of the LoadedModel ownership fields (everything except IR / GPU).
pub struct LoadedModelOwnership {
    pub source: super::helpers::ReloadableSource,
    pub primary_astance_result: crate::intermediate::types::AStanceResult,
    pub appended_models: Vec<super::AppendedModel>,
    pub material_groups: Vec<super::MaterialGroup>,
    pub pkg_material_keys: Vec<Option<crate::unitypackage::PkgMaterialKey>>,
    pub prefab_name: Option<String>,
    pub prefab_entry_path: Option<String>,
}

/// Animation playback state snapshot during Append.
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
    /// Capture a snapshot from ViewerApp's animation state.
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

    /// Restore the playback state into AnimationState.
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

/// Extra info for a deferred Append GPU build.
pub struct AppendGpuBuildInfo {
    /// GPU model before merge (used for GPU build failure rollback).
    pub rollback_gpu_model: super::super::mesh::GpuModel,
    /// IR snapshot before merge (used for rollback).
    pub ir_snapshot: IrRollbackSnapshot,
    /// LoadedModel ownership fields before merge.
    pub ownership: LoadedModelOwnership,
    /// ReloadableSource for the appended model.
    pub append_source: super::helpers::ReloadableSource,
    /// Name of the appended model.
    pub added_name: String,
    /// Bone count of the appended model.
    pub added_bones: usize,
    /// Mesh count of the appended model.
    pub added_meshes: usize,
    /// Material count of the appended model.
    pub added_materials: usize,
    /// Material count before merge (used to build MaterialGroup).
    pub saved_material_count: usize,
    /// merge() return value: total integrated bone count.
    pub merged_bones: usize,
    /// merge() return value: count of newly added bones.
    pub new_bones: usize,
    /// Model name inside the unitypackage.
    pub pkg_model_name: Option<String>,
    /// unitypackage model locator.
    pub pkg_locator: Option<crate::unitypackage::PkgModelLocator>,
    /// Silent mode (do not show toasts).
    pub silent: bool,
    /// Post-processing payload for PkgAppend (None = non-Pkg append).
    pub pkg_append_payload: Option<Box<PkgAppendPayload>>,
    /// PkgAppend: material offset before merge.
    pub mat_offset: usize,
    /// PkgAppend: texture count before merge.
    pub tex_count_before: usize,
    /// PkgAppend: source path.
    pub source_path: PathBuf,
    /// Animation state snapshot.
    pub anim_snapshot: AnimationSnapshot,
}

/// State of a background PMX conversion.
pub struct PendingConvertBg {
    /// Receiver channel for the BG thread result.
    pub rx: std::sync::mpsc::Receiver<ConvertBgResult>,
    /// Cancel flag.
    pub cancel: Arc<AtomicBool>,
}

/// Result of a background PMX conversion.
pub struct ConvertBgResult {
    /// Conversion result: Ok(stats_message) or Err(error_message).
    pub result: Result<String, String>,
    /// Whether the log buffer was written (true when output_log = true).
    pub log_written: bool,
    /// Whether the message includes a warning.
    pub has_warning: bool,
    /// Output directory (for opening on success).
    pub output_dir: Option<PathBuf>,
}

/// Number of textures uploaded per frame.
pub const GPU_UPLOAD_BATCH: usize = 4;

/// Queue for batch loading of multiple models (assets shared via Arc to eliminate clone cost).
pub struct PendingMultiLoad {
    pub assets: Arc<Vec<crate::unitypackage::ExtractedAsset>>,
    /// Remaining models (fbx_index, model_type).
    pub remaining: Vec<(usize, PkgModelType)>,
    pub source_path: PathBuf,
    pub archive_snapshot: Option<Arc<[u8]>>,
    pub nested_archive_source: Option<ReloadableSource>,
    pub pkg_index: Option<Arc<UnityPackageIndex>>,
    /// Total number of models in batch (for progress display).
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
            archive_password: None,
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

/// Wait state for the asynchronous result of the UV-map save dialog.
pub struct PendingUvExport {
    /// Receiver channel for the dialog result.
    pub rx: std::sync::mpsc::Receiver<Option<std::path::PathBuf>>,
    /// UV-map output resolution.
    pub uv_map_size: u32,
    /// Material group info (name, material index range).
    pub uv_groups: Vec<(String, std::ops::Range<usize>)>,
}

/// Wait state for a UV-map BG export result.
pub struct PendingUvBgExport {
    /// Receiver channel for the BG thread result (Ok = output path, Err = error message).
    pub rx: std::sync::mpsc::Receiver<Result<std::path::PathBuf, String>>,
}

/// Wait state for the BG directory-creation step before PMX conversion.
pub struct PendingMkdir {
    /// Receiver channel for the BG thread result (Ok = success, Err = error message).
    pub rx: std::sync::mpsc::Receiver<Result<(), String>>,
}

/// State related to PMX export.
pub struct ExportState {
    /// Whether to write a log file at PMX conversion time.
    pub output_log: bool,
    /// PMX output path (for the text-box edit).
    pub pmx_output_path: String,
    /// Model name shown to the user (without extension).
    /// Used both in the title bar and in the PMX output file name.
    /// The default depends on the load source and is editable via the UI.
    pub model_display_name: String,
    /// Export only visible materials to PMX (default: false).
    pub export_visible_only: bool,
    /// UV-map output resolution.
    pub uv_map_size: u32,
    /// Output without physics (rigid bodies / joints).
    pub no_physics: bool,
    /// Output with the original bone structure (skip standard-bone insertion).
    pub raw_structure: bool,
    /// Also output MME materials (.fx) (§K.5 / Step 6).
    pub output_mme: bool,
    /// PMX output scale factor (default: 1.0).
    pub scale: f32,
    /// Base directory for creating converted_modelXX (None = same place as the source file).
    pub output_base_dir: Option<std::path::PathBuf>,
    /// Asynchronous folder selection dialog (PMX output destination).
    pub pending_folder_dialog: Option<std::sync::mpsc::Receiver<Option<std::path::PathBuf>>>,
    /// Asynchronous folder selection dialog (ray-mmd root).
    pub pending_ray_mmd_dialog: Option<std::sync::mpsc::Receiver<Option<std::path::PathBuf>>>,
    /// Asynchronous UV-map save dialog.
    pub pending_uv_dialog: Option<PendingUvExport>,
    /// Wait state for UV-map BG export.
    pub pending_uv_bg: Option<PendingUvBgExport>,
    /// Wait state for the BG directory-creation step before PMX conversion.
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
            output_mme: false,
            scale: 1.0,
            output_base_dir: None,
            pending_folder_dialog: None,
            pending_ray_mmd_dialog: None,
            pending_uv_dialog: None,
            pending_uv_bg: None,
            pending_mkdir: None,
        }
    }
}

impl ViewerApp {
    /// Draw the progress overlay (with a cancel button while processing is in flight).
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
        let msg: Option<std::borrow::Cow<'static, str>> = if is_bg_loading {
            Some(t!("viewer.overlay.loading"))
        } else if is_gpu_building {
            Some(t!("viewer.overlay.gpu_building"))
        } else if self.pending.rebuild.is_some() || self.pending.reload.is_some() {
            Some(t!("viewer.overlay.processing"))
        } else if self.export.pending_mkdir.is_some() {
            Some(t!("viewer.overlay.creating_directory"))
        } else if is_converting_bg || self.pending.convert.is_some() {
            Some(t!("viewer.overlay.pmx_converting"))
        } else if self.export.pending_uv_bg.is_some() {
            Some(t!("viewer.overlay.uvmap_exporting"))
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
        // Background band.
        viewport.painter().rect_filled(
            bar_rect,
            0.0,
            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 0xC0),
        );

        // Text (centered).
        viewport.painter().text(
            center,
            egui::Align2::CENTER_CENTER,
            msg,
            egui::FontId::proportional(16.0),
            color,
        );

        // Cancellable while processing: place the cancel button on a top-most Area.
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
                            egui::RichText::new(t!("viewer.overlay.cancel"))
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

            // Esc also cancels.
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

    /// Cancel the BG load and clear the model and every related state.
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
        // Clear every queued deferred process.
        self.pending.pkg_load = None;
        self.pending.archive_load = None;
        self.pending.multi_load = None;
        self.pending.gpu_build = None;
        self.pending.unity_pkg = None;
        self.pending.archive = None;

        // Cancellation during reload: restore the prior state from the snapshot to keep the old model.
        if let Some(snap) = self.reload_snapshot.take() {
            self.restore_snapshot_on_failure(snap);
            self.convert_message = Some(ConvertMessage::success(
                t!("viewer.toast.cancel.reload").into_owned(),
            ));
            return;
        }

        // Cancellation during a normal load: clear the model and animation state entirely.
        self.loaded = None;
        self.anim.state = None;
        self.anim.library.clear();
        self.anim.active_index = None;
        self.tex.pkg_textures = None;
        self.clear_pkg_thumb_cache();
        // v0.5.2 [review_01 P1]: discard on load cancel as well, so the previous
        // model's TextureId is not reused even when swapped in for a different
        // model with the same number of ir.textures.
        self.clear_ir_thumb_cache();
        self.cancel_tex_match_preview();
        self.tex.pending_match = None;
        self.selected_fbx_name = None;
        self.selected_pkg_model = None;
        self.material_display.clear();
        self.morph_weights.clear();
        self.material_visibility.clear();

        // Invalidate renderer caches.
        if let Some(ref mut renderer) = self.renderer {
            renderer.invalidate_visualization_cache();
            renderer.invalidate_normal_cache();
        }

        self.convert_message = Some(ConvertMessage::success(
            t!("viewer.toast.cancel.load").into_owned(),
        ));
    }

    /// Cancel the GPU build (texture upload) and clear the state.
    fn cancel_gpu_build(&mut self) {
        if self.pending.gpu_build.is_some() {
            log::info!("User cancelled GPU build");
            self.pending.gpu_build = None;
        }

        // Cancellation during reload: restore the prior state from the snapshot to keep the old model.
        if let Some(snap) = self.reload_snapshot.take() {
            self.restore_snapshot_on_failure(snap);
            self.convert_message = Some(ConvertMessage::success(
                t!("viewer.toast.cancel.reload").into_owned(),
            ));
            return;
        }

        // Cancellation during a normal load: clear the model and animation state entirely.
        self.loaded = None;
        self.anim.state = None;
        self.anim.library.clear();
        self.anim.active_index = None;
        self.tex.pkg_textures = None;
        self.clear_pkg_thumb_cache();
        // v0.5.2 [review_01 P1]: discard on load cancel as well, so the previous
        // model's TextureId is not reused even when swapped in for a different
        // model with the same number of ir.textures.
        self.clear_ir_thumb_cache();
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

        self.convert_message = Some(ConvertMessage::success(
            t!("viewer.toast.cancel.gpu_build").into_owned(),
        ));
    }

    /// Cancel the background PMX conversion.
    fn cancel_convert_bg(&mut self) {
        if let Some(ref handle) = self.pending.convert_bg {
            handle
                .cancel
                .store(true, std::sync::atomic::Ordering::Relaxed);
            log::info!("User cancelled background PMX conversion");
        }
        self.pending.convert_bg = None;
        self.convert_message = Some(ConvertMessage::success(
            t!("viewer.toast.cancel.pmx_export").into_owned(),
        ));
    }

    /// Update progress flags (the trigger to run the action on the next frame).
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

    /// Set progress toast for batch model loading.
    #[allow(dead_code)]
    fn set_batch_progress_message(
        &mut self,
        batch_progress: &Option<(usize, usize)>,
        model_name: &str,
    ) {
        if let Some((current, total)) = *batch_progress {
            let msg = t!(
                "viewer.toast.progress.loaded",
                current = current,
                total = total,
                name = model_name,
            )
            .into_owned();
            self.convert_message = Some(ConvertMessage::success(msg));
        } else {
            self.convert_message = None;
        }
    }

    /// Run the deferred processes (file load, GPU rebuild, PMX conversion, etc.).
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
        // PendingDispatch is Ready -> route on the main thread.
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
                    _ => unreachable!("PendingDispatch verified by dispatch_ready"),
                };
            self.route_load_dispatch(dispatch, prior_loading);
        }

        // Poll the background CPU parse result.
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
                    if let Some(prompt) = e.downcast_ref::<ArchivePasswordPrompt>() {
                        // Encrypted archive: open the password dialog instead of a
                        // failure toast, then retry the load with the input.
                        log::info!(
                            "Archive requires a password (bad_password={}): {}",
                            prompt.bad_password,
                            prompt.path.display()
                        );
                        // Listing-stage failures carry the outer texts: make
                        // the readme readable while the dialog is up (it often
                        // holds the password hint).
                        if let Some(texts) = prompt.texts.clone() {
                            self.set_archive_text_files(texts, prompt.append);
                        }
                        self.pending.archive_password = Some(PendingArchivePassword {
                            path: prompt.path.clone(),
                            append: prompt.append,
                            auto_select_model: prompt.auto_select_model.clone(),
                            input: String::new(),
                            error: prompt
                                .bad_password
                                .then(|| t!("error.archive_bad_password").into_owned()),
                            submitted: false,
                        });
                    } else {
                        let msg = format!("{:#}", e);
                        if msg.contains("bg load cancelled") {
                            log::info!("Bg load cancelled (req={})", current_id);
                        } else {
                            // Also log the failure: the toast is transient, and
                            // without this the log file holds no trace at all.
                            log::error!("Bg load failed (req={}): {}", current_id, msg);
                            self.convert_message = Some(ConvertMessage::failure(msg));
                        }
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
        // Deferred unitypackage model load -> hand off to the BG thread.
        if self.pending.pkg_load.as_ref().is_some_and(|p| p.shown) {
            let p = self
                .pending
                .pkg_load
                .take()
                .expect("pending_pkg_load verified by shown");
            self.spawn_bg_pkg_load(p);
        }

        // Process the multi-model batch-load queue.
        // Submit the next entry only when both pkg_load and fbx_choice are empty and bg_state is idle.
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

        // Deferred archive model load -> hand off to the BG thread.
        if self.pending.archive_load.as_ref().is_some_and(|p| p.shown) {
            let p = self
                .pending
                .archive_load
                .take()
                .expect("pending_archive_load verified by shown");
            self.spawn_bg_archive_load(p);
        }

        // Archive password entered -> restart the load with the password.
        if self
            .pending
            .archive_password
            .as_ref()
            .is_some_and(|p| p.submitted)
        {
            let p = self
                .pending
                .archive_password
                .take()
                .expect("pending archive_password verified by submitted");
            self.spawn_bg_archive_index_retry(p);
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

        // Phase 1: texture upload (split across frames) + start Phase 2 once done.
        // Run only while cpu_prep_rx is None (do nothing after Phase 2 is started).
        // For total == 0 (e.g. an OBJ where MTL resolution failed and only the
        // default material remains), there is nothing to upload, so we must
        // proceed straight to starting Phase 2.
        // The previous implementation included `gb.next_tex < total` on the
        // outer if, so the entire Phase 1 block was skipped when total == 0,
        // Phase 2 was never started, and the GPU build hung indefinitely.
        if gb.cpu_prep_rx.is_none() {
            // Upload one batch of remaining textures (this block is skipped when total == 0).
            if gb.next_tex < total {
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
            }

            // Phase 1 done (including 0 textures) -> Phase 2: start CPU prep on the BG thread.
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

        // Phase 3: poll the CPU prep result -> GPU finalize.
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
                                match self.finish_load_with_gpu(
                                    gb.ir,
                                    gpu_model,
                                    gb.source,
                                    gb.is_reload,
                                ) {
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
                                self.convert_message = Some(ConvertMessage::failure(
                                    t!(
                                        "viewer.toast.append.gpu_build_failed_reverted",
                                        error = format!("{:#}", e)
                                    )
                                    .into_owned(),
                                ));
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
                        self.convert_message = Some(ConvertMessage::failure(
                            t!(
                                "viewer.toast.append.cpu_prep_failed_reverted",
                                error = format!("{:#}", e)
                            )
                            .into_owned(),
                        ));
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
        self.poll_ray_mmd_dialog();
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

    fn poll_ray_mmd_dialog(&mut self) {
        if let Some(ref rx) = self.export.pending_ray_mmd_dialog {
            let mut alive = true;
            if let Some(opt_dir) = Self::poll_receiver(rx, &mut alive) {
                if let Some(dir) = opt_dir {
                    self.app_config.ray_mmd_root = Some(dir.to_string_lossy().into_owned());
                }
                self.export.pending_ray_mmd_dialog = None;
            } else if !alive {
                self.export.pending_ray_mmd_dialog = None;
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
                            // PSB auto-promotion may rewrite `.psd` → `.psb`,
                            // so report the path that was actually written.
                            let _ = tx.send(match result {
                                Ok(actual_path) => Ok(actual_path),
                                Err(e) => Err(format!("{e}")),
                            });
                            repaint.request_repaint();
                        });
                        self.export.pending_uv_bg = Some(PendingUvBgExport { rx });
                    } else {
                        self.convert_message = Some(ConvertMessage::failure(
                            t!("viewer.toast.precondition.no_model_loaded").into_owned(),
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
                        // The UV export flow never offers `.psb` as a user
                        // choice; the writer only produces `.psb` via the size
                        // based PSD→PSB auto-promotion. So a `.psb` result
                        // unambiguously means promotion happened, and the user
                        // (who asked for `.psd`) gets an explicit explanation
                        // instead of a silently different extension.
                        let promoted = path
                            .extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("psb"));
                        let key = if promoted {
                            "viewer.toast.uvmap.exported_psb"
                        } else {
                            "viewer.toast.uvmap.exported"
                        };
                        self.convert_message = Some(ConvertMessage::success(
                            t!(key, path = path.display().to_string()).into_owned(),
                        ));
                    }
                    Err(e) => {
                        self.convert_message = Some(ConvertMessage::failure(
                            t!("viewer.toast.uvmap.failed", error = e.to_string()).into_owned(),
                        ));
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
                    t!("viewer.toast.bg_failure.pmx_export_thread_panic").into_owned(),
                ));
            }
        }
    }
}

/// Extract only the minimum data needed for UV-map export as an IrModel.
/// Heavy data such as textures and bones are not copied.
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
