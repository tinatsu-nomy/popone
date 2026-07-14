//! Viewer main state management (ViewerApp struct definition, eframe::App impl)

pub mod file_io;
pub mod helpers;
pub mod material_edit;
pub mod material_presets;
pub mod pending;
pub mod persistence;
pub mod texture_mgmt;
pub mod uv_edit;

use std::collections::VecDeque;
use std::path::PathBuf;

use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;
use eframe::egui_wgpu;
use rust_i18n::t;

use crate::intermediate::types::{IrMaterial, IrModel};
use crate::unitypackage::PkgModelLocator;

use super::animation::AnimationState;
use super::camera::OrbitCamera;
use super::gpu::{DrawMode, GpuRenderer, LightMode, RenderParams, ShaderOverride, ShaderSelection};
use super::mesh::GpuModel;
use super::ui;

/// Dark theme panel background color (#1D1D1D)
const DARK_PANEL_BG: egui::Color32 = egui::Color32::from_rgb(0x1D, 0x1D, 0x1D);
/// Dark theme border color (#333333)
const DARK_BORDER_COLOR: egui::Color32 = egui::Color32::from_rgb(0x33, 0x33, 0x33);

// Re-exports from submodules
pub use helpers::{FbxLoadMode, PkgModelType, PreloadedData, ReloadableSource, TextureSource};
pub use pending::{
    ExportState, PendingArchive, PendingArchiveLoad, PendingFbxChoice, PendingFbxChoicePkg,
    PendingMultiLoad, PendingOverlay, PendingPkgModelLoad, PendingState, PendingUnityPackage,
};
pub use texture_mgmt::{CachedMaterialInfo, PendingTexMatch, PendingTexPreview, TextureState};

/// Cache for the status bar.
pub struct CachedStats {
    pub total_vertices: usize,
    pub total_faces: usize,
    /// Pre-formatted status string (avoids per-frame format! cost).
    pub status_text: String,
}

impl CachedStats {
    pub(super) fn new(ir: &IrModel) -> Self {
        let total_vertices = ir.total_vertices();
        let total_faces = ir.total_faces();
        let status_text = t!(
            "viewer.status_bar.counts",
            vertices = total_vertices,
            faces = total_faces,
            materials = ir.materials.len(),
            textures = ir.textures.len(),
            bones = ir.bones.len(),
            morphs = ir.morphs.len(),
        )
        .into_owned();
        Self {
            total_vertices,
            total_faces,
            status_text,
        }
    }
}

/// Information about additionally loaded (appended) models (used to re-merge on reload).
#[derive(Clone)]
pub struct AppendedModel {
    pub source: ReloadableSource,
    /// Selected model name inside the unitypackage (None for direct FBX/VRM).
    pub pkg_model_name: Option<String>,
    /// Phase 3: stable key for unitypackage models (for staged migration).
    pub pkg_model: Option<PkgModelLocator>,
}

/// Per-model material / DrawCall range information.
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
    /// A-stance / T-stance conversion result of the main model (unaffected by merge).
    pub primary_astance_result: crate::intermediate::types::AStanceResult,
    /// List of appended models (used to re-merge on reload).
    pub appended_models: Vec<AppendedModel>,
    /// Per-model material / DrawCall ranges.
    pub material_groups: Vec<MaterialGroup>,
    /// Material info cache (updated on texture assignment).
    pub mat_cache: CachedMaterialInfo,
    /// Statistics cache.
    pub stats_cache: CachedStats,
    /// Stable per-material key (built when loaded via pkg_index).
    pub pkg_material_keys: Vec<Option<crate::unitypackage::PkgMaterialKey>>,
    /// Prefab file name when loaded via Prefab (used for the file-tree display).
    pub prefab_name: Option<String>,
    /// Path of the Prefab entry (kept so reload can re-resolve it).
    pub prefab_entry_path: Option<String>,
}

impl LoadedModel {
    /// Returns sibling material indices with the same name (limited to the same MaterialGroup).
    /// Used to constrain the scope of `link_same_name`.
    pub fn same_name_siblings(&self, mat_idx: usize) -> Vec<usize> {
        let Some(target_mat) = self.ir.materials.get(mat_idx) else {
            return Vec::new();
        };
        let target_name = &target_mat.name;
        // Determine the MaterialGroup range that mat_idx belongs to.
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

/// Per-material display / render state (indexed by mat_idx).
#[derive(Clone, Debug)]
pub struct MaterialDisplayState {
    /// Normal smoothing ON/OFF.
    pub smooth_normals: bool,
    /// Clear custom normals ON/OFF.
    pub clear_normals: bool,
    /// Apply normal map ON/OFF.
    pub normal_map: bool,
    /// Apply emissive ON/OFF.
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

/// Kinds of conversion results.
pub enum ConvertResult {
    Success(String),
    /// Succeeded but with a warning (rendered as a red text overlay).
    Warning(String),
    Failure(String),
}

/// Conversion result message with a display timestamp.
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

    /// Seconds elapsed since the message started being displayed.
    pub fn elapsed_secs(&self) -> f32 {
        self.shown_at.elapsed().as_secs_f32()
    }
}

/// Display / render related settings.
#[derive(Clone)]
pub struct DisplaySettings {
    /// Light intensity (0.0..=2.0).
    pub light_intensity: f32,
    /// Light color RGB (linear).
    pub light_color: [f32; 3],
    /// Ambient intensity (0.0..=1.0).
    pub ambient_intensity: f32,
    /// Ambient sky color RGB (linear).
    pub ambient_sky_color: [f32; 3],
    /// Ambient ground color RGB (linear).
    pub ambient_ground_color: [f32; 3],
    /// Background brightness (0.0..=1.0).
    pub bg_brightness: f32,
    /// Show grid.
    pub show_grid: bool,
    /// Show bones.
    pub show_bones: bool,
    /// Bone opacity.
    pub bone_opacity: f32,
    /// Show SpringBones (physics).
    pub show_spring_bones: bool,
    /// SpringBone opacity.
    pub spring_bone_opacity: f32,
    /// Show joints (PMX/PMD only).
    pub show_joints: bool,
    /// Joint opacity.
    pub joint_opacity: f32,
    /// Draw mode.
    pub draw_mode: DrawMode,
    /// Light mode.
    pub light_mode: LightMode,
    /// Align rigid-body rotation to bone direction (for PMX export + physics display).
    pub align_rigid_rotation: bool,
    /// MSAA antialiasing.
    pub msaa: bool,
    /// Smooth normals (vertex unification + normal averaging).
    pub smooth_normals: bool,
    /// Clear custom normals (recompute normals from geometry).
    pub clear_custom_normals: bool,
    /// Show normals.
    pub show_normals: bool,
    /// Normal display length.
    pub normal_length: f32,
    /// Shader override mode (for the GPU uniform).
    pub shader_override: ShaderOverride,
    /// Use MMD-dedicated render path (formerly `mmd_mode`).
    pub use_mmd_path: bool,
    /// Auto mode (chooses Standard/MMD based on the model format).
    pub auto_shader: bool,
    /// MToon outline rendering.
    pub outline_enabled: bool,
    /// MMD edge rendering.
    pub mmd_edge_enabled: bool,
    /// MMD edge-thickness global scale (0.1..=3.0).
    pub mmd_edge_thickness: f32,
    /// Bloom effect.
    pub bloom_enabled: bool,
    /// Bloom composite intensity (0.0..=4.0; 2.0 = VRM default).
    pub bloom_intensity: f32,
    /// Bloom luminance extraction threshold (0.0..=1.0).
    pub bloom_threshold: f32,
    /// Bloom diffusion stages (3..=6).
    pub bloom_radius: u32,
    /// Whether the fallback for texture decode failure is white (default `true`).
    /// When `false`, the legacy 1x1 magenta is used so missing textures stand out.
    pub white_texture_fallback: bool,
    /// Whether the right tool panel is resizable (default `false` = fixed 280 px width).
    /// When `true`, the user can drag to change the width within 280..=600 px.
    /// No automatic resize based on content volume is performed.
    pub panel_resizable: bool,
    /// Current width of the right tool panel (px). When `panel_resizable = true`,
    /// updated by user drag. When `false`, fixed at 280 px while the actual value is
    /// preserved. Range: [280, 600].
    pub panel_width: f32,
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
            white_texture_fallback: true,
            panel_resizable: false,
            panel_width: 280.0,
        }
    }
}

impl DisplaySettings {
    /// Set the internal state from the UI `ShaderSelection`.
    pub fn set_shader_selection(&mut self, sel: ShaderSelection) {
        match sel {
            ShaderSelection::Auto => {
                self.shader_override = ShaderOverride::Default;
                self.auto_shader = true;
                // `use_mmd_path` is decided automatically in `normalize_shader_state`.
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

    /// Get the UI-display `ShaderSelection` derived from the current internal state.
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

/// Right side panel tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidePanelTab {
    /// Info: model info + meta info.
    Info,
    /// Control: expression morphs + animation playback.
    Control,
    /// Display: display settings + material display.
    Display,
    /// Export: PMX conversion + UV map.
    Export,
}

/// Animation library / playback management.
pub struct AnimLibrary {
    /// VRMA animation playback state.
    pub state: Option<AnimationState>,
    /// Loaded VRMA library (name, path, animation data).
    pub library: Vec<(
        String,
        PathBuf,
        Arc<crate::intermediate::animation::VrmaAnimation>,
    )>,
    /// Index of the currently active VRMA.
    pub active_index: Option<usize>,
    /// Unity .anim muscle angle scale.
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

/// Main viewer state.
pub struct ViewerApp {
    pub loaded: Option<LoadedModel>,
    pub camera: OrbitCamera,
    pub renderer: Option<GpuRenderer>,
    pub convert_message: Option<ConvertMessage>,
    /// Slider values for expression morphs.
    pub morph_weights: Vec<f32>,
    /// Morph-weight changed flag.
    pub morph_dirty: bool,
    /// Display / render settings.
    pub display: DisplaySettings,
    /// State related to PMX export.
    pub export: ExportState,
    /// Per-material visibility ON/OFF (indexed by `draw_idx`).
    pub material_visibility: Vec<bool>,
    /// Per-material render state (indexed by `mat_idx`).
    pub material_display: Vec<MaterialDisplayState>,
    /// Material edit drawer (§C / §A): bind-group rebuild request flag from edits.
    /// Indexed by `mat_idx`; consumed at the end of `update()` to call
    /// `rebuild_material_bind_groups`.
    pub material_dirty: Vec<bool>,
    /// Material edit drawer (§A): index of the material currently being edited
    /// (Window is hidden when `None`).
    pub editing_material_index: Option<usize>,
    /// Material edit drawer (§H): IR material snapshot taken right after load.
    /// Restored from `pristine_materials[mat_idx].clone()` via the "Reset to default" button.
    /// Captured in `finish_load_with_gpu` before `material_overrides`'s `apply_to`,
    /// and on reload the new IR's values are also installed as pristine.
    pub pristine_materials: Vec<IrMaterial>,

    /// Material edit drawer (Step 4-16b / review_016): file paths of non-BaseColor
    /// texture-slot assignments. Restored on reload by re-reading + `assign_texture_core`.
    /// key = (mat_idx, TextureSlot), value = file path.
    /// Cleared on a new model load (`is_reload = false`).
    pub slot_texture_paths: std::collections::HashMap<
        (usize, crate::intermediate::types::TextureSlot),
        std::path::PathBuf,
    >,

    /// Material edit drawer: per-`mat_idx` parameter overrides.
    ///
    /// Aggregated into the `MaterialParamOverride` struct in Step 2; uniformly
    /// manages color/scalar values for all §E sections (basic / shade / outline /
    /// rim / MatCap / UV anim / emissive / normal / other). Even when the IR is
    /// reloaded by an A-stance / T-stance conversion, the values are reapplied to
    /// the new IR via `MaterialParamOverride::apply_to()`.
    ///
    /// **Step 3 migration plan**: this field will be absorbed into
    /// `MaterialEditRecord.param_override` and replaced by an auto-generated
    /// diff/apply produced by a `declarative_macro`.
    pub material_overrides: std::collections::HashMap<usize, material_edit::MaterialParamOverride>,
    /// M6 Step 6.4: clipboard for copy/paste of material parameters.
    /// Excludes texture assignments (to avoid path dependence); only color/scalar values.
    pub clipboard_material: Option<material_edit::MaterialParamOverride>,
    /// Material filter string.
    pub material_filter: String,
    /// Expression morph filter string.
    pub morph_filter: String,
    /// Drag-over flag.
    pub drag_hovering: bool,
    /// Viewport texture ID.
    pub viewport_texture_id: Option<egui::TextureId>,
    /// wgpu render state (obtained from `CreationContext`).
    pub(crate) render_state: egui_wgpu::RenderState,
    /// T-pose -> A-stance conversion (re-loads when toggled).
    pub normalize_pose: bool,
    /// A-stance -> T-stance conversion (FBX; re-loads when toggled).
    pub normalize_to_tstance: bool,
    /// Viewport width (used for fit computation).
    pub last_viewport_width: f32,
    /// Viewport height (used for fit computation).
    pub last_viewport_height: f32,
    /// Pixel height of the material edit panel (bottom `TopBottomPanel`) overlapping
    /// the central viewport. 0.0 when not open / not visible. Used by the renderer
    /// to apply FOV compensation so the on-screen pixel size of the model is
    /// preserved across panel open/close.
    pub material_panel_height_px: f32,
    /// Texture-assignment state.
    pub tex: TextureState,
    /// Aggregated state for deferred processing.
    pub pending: PendingState,
    /// FPS measurement: ring buffer of frame timestamps (most recent 1 second).
    frame_times: VecDeque<Instant>,
    /// FPS measurement: time elapsed since the previous frame (ms).
    frame_dt_ms: f32,
    /// FPS for display (updated every 0.25 s).
    fps_display: f32,
    /// Last update time of the FPS display.
    fps_last_update: Instant,
    /// IPC receive channel (single-instance support).
    #[cfg(target_os = "windows")]
    ipc_receiver: std::sync::mpsc::Receiver<PathBuf>,
    /// Log directory path.
    pub logs_dir: PathBuf,
    /// Current log file path.
    pub log_path: PathBuf,
    /// In-memory log buffer (for the viewer mode).
    pub log_buffer: crate::SharedLogBuffer,
    /// Model for the log viewer window (drawn as a separate OS window via
    /// `show_viewport_deferred`).
    pub log_viewer: super::log_viewer::SharedLogViewer,
    /// Model for the archive text viewer (file list + per-document windows,
    /// each drawn as a separate OS window via `show_viewport_deferred`).
    pub text_viewer: super::text_viewer::SharedTextViewer,
    /// Directory of the most recently opened model file (dialog only).
    pub last_model_dir: Option<PathBuf>,
    /// Selected FBX file name inside the unitypackage (used for matching on reload).
    pub selected_fbx_name: Option<String>,
    /// Stable key of the model selected inside the unitypackage (Phase 3 migration).
    pub selected_pkg_model: Option<PkgModelLocator>,
    /// Animation library.
    pub anim: AnimLibrary,
    /// Currently selected tab on the right panel.
    pub side_panel_tab: SidePanelTab,
    /// Window-title update request.
    pub window_title: Option<String>,
    /// Suppress the manual texture-assignment dialog (used during reload).
    pub suppress_tex_match: bool,
    /// `draw_index` set corresponding to the hovered material (highlighted in the 3D view).
    pub hovered_draw_indices: Vec<usize>,
    /// Pre-read data of the D&D temporary file (used only during the load chain).
    pub(crate) preloaded: Option<PreloadedData>,
    /// Startup time (reference for UV-animation cumulative time).
    start_time: Instant,
    /// Instance-ID counter used at append (the base model is always 0).
    pub next_instance_id: u32,
    /// Splash image texture (shown when no model is loaded).
    splash_texture: Option<egui::TextureHandle>,
    /// App data directory (where settings / history files are saved).
    pub data_dir: PathBuf,
    /// Session settings.
    pub app_config: persistence::AppConfig,
    /// Config-changed flag (decides whether to save in `on_exit`).
    config_dirty: bool,
    /// Deferred-restore state for the window position.
    pending_window_restore: PendingWindowRestore,
    /// Texture-assignment history (in-memory cache).
    pub texture_history: persistence::TextureHistoryFile,
    /// State for per-vertex UV editing (v0.5.5).
    pub uv_edit: uv_edit::UvEditState,
    /// Open/close state of the UV edit window (v0.5.5). Opened from the material
    /// edit panel.
    pub uv_edit_window_open: bool,
    /// Background texture cache for the UV edit canvas (v0.5.5 Phase 2-1).
    /// A simple single-entry cache holding `(ir_texture_index, egui::TextureId)`.
    /// When the BaseColor resolution result for the active material changes,
    /// release the old `TextureId` via `renderer.free_texture` and re-register
    /// with `register_native_texture`.
    pub uv_edit_bg_tex: Option<(usize, egui::TextureId)>,
    /// Flag indicating dark-theme applied (re-applied on the first `update` to
    /// counter eframe's style reset).
    dark_theme_applied: bool,
    /// Panel background color resolved from the theme.
    theme_panel_bg: egui::Color32,
    /// Border color resolved from the theme.
    theme_border: egui::Color32,
    /// Generation-number counter for background loads. Incremented by every
    /// `fresh_request_id` call. Used to identify and discard the result of an
    /// older load.
    pub(crate) next_request_id: u64,
    /// Snapshot used for reload (restored after a BG load completes).
    pub(crate) reload_snapshot: Option<file_io::ReloadSnapshot>,
    /// Watchdog heartbeat (ticks every frame to monitor responsiveness).
    heartbeat: super::watchdog::Heartbeat,
    /// Progress state of the GPU pipeline warmup.
    warmup_phase: WarmupPhase,
}

/// Staged warmup state of the `GpuRenderer`.
#[derive(Default)]
enum WarmupPhase {
    /// `GpuRenderer::new()` has not been called yet.
    #[default]
    NotStarted,
    /// `GpuRenderer::new()` done; sRGB+MSAA pipeline not created yet.
    RendererCreated,
    /// sRGB+MSAA done.
    SrgbMsaaDone,
    /// sRGB+noMSAA done.
    SrgbNoMsaaDone,
    /// All pipelines pre-created.
    Complete,
}

impl ViewerApp {
    /// Issue a fresh `request_id` (increments the generation counter).
    pub(crate) fn fresh_request_id(&mut self) -> u64 {
        self.next_request_id = self.next_request_id.wrapping_add(1);
        self.next_request_id
    }

    /// Regenerates derived state (window title and the file-name part of
    /// `pmx_output_path`) when `self.export.model_display_name` changes.
    ///
    /// - Window title: `POPONE Model Viewer v{ver} - {model_display_name}` applied
    ///   on the next frame.
    /// - `pmx_output_path`: keeps the parent directory (`converted_modelXX/`) and
    ///   replaces only the file name with `{model_display_name}.pmx`. Skips the
    ///   path update when `pmx_output_path` is empty or `model_display_name` is
    ///   empty.
    ///
    /// Call sites:
    /// - Right after the Prefab name is determined (PkgInitial / synchronous
    ///   Prefab load path in `file_io.rs`).
    /// - When the user edits the name in the UI `TextEdit`.
    /// - At snapshot restore after a successful reload.
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

    /// Staged warmup of the GPU pipeline (one phase per frame while the splash
    /// screen is shown). Each phase includes shader compilation and takes a few
    /// seconds, but the splash image keeps displaying between frames.
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

    /// Draws the log viewer window (a separate OS-level window).
    ///
    /// Why `show_viewport_deferred`: the main `update()` runs 3D
    /// `render_to_texture` every frame, so `immediate`'s parent-child mutual
    /// repaint would trigger a 3D redraw on every log inflow. With `deferred`,
    /// the child viewport's repaints do not wake the parent.
    ///
    /// The closure has `Fn + Send + Sync + 'static` bounds, so it cannot capture
    /// `&mut self`. We pass `Arc::clone`d `log_viewer` / `log_buffer` and a
    /// `PathBuf::clone`d `logs_dir` via `move`.
    fn show_log_viewer(&self, ctx: &egui::Context) {
        // P1 fix: perform the visible check *before* `apply_geometry.take()`.
        // If `apply_geometry` is consumed on the very first frame while the
        // window is hidden, then when the user later opens it via the button
        // the saved position from the config would be lost.
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

            // 1. Incremental ingest from `SharedLogBuffer`.
            m.poll(&log_buffer);

            // 2. Draw UI.
            m.draw(child_ctx, &log_buffer, &logs_dir);

            // 3. Record the latest geometry (for same-session reopen + on_exit save).
            child_ctx.input(|i| {
                if let (Some(outer), Some(inner)) =
                    (i.viewport().outer_rect, i.viewport().inner_rect)
                {
                    m.last_geometry =
                        Some(([outer.min.x, outer.min.y], [inner.width(), inner.height()]));
                }
            });

            // 4. Close detection (x button). `hide()` snapshots `last_geometry`
            //    into `apply_geometry` to preserve the position on the next show.
            if child_ctx.input(|i| i.viewport().close_requested()) {
                m.hide();
            }

            // 5. While visible, repaint every 150 ms (prevents delayed display
            //    of new logs). The main `update` self-repaints only every 3 s,
            //    so we wake only the child viewport here. Because it is deferred,
            //    the parent 3D view is not affected.
            if m.visible {
                child_ctx.request_repaint_after(std::time::Duration::from_millis(150));
            }
        });
    }

    /// Draws the archive-text-viewer list window plus one deferred viewport
    /// (separate OS window) per opened document.
    ///
    /// Deferred for the same reason as the log viewer: scrolling a readme must
    /// not wake the parent's 3D rendering. The static document content also
    /// needs no periodic repaint.
    fn show_text_viewer_windows(&self, ctx: &egui::Context) {
        // --- File-list window (inside the main viewport). ---
        let mut clicked: Option<usize> = None;
        {
            let mut m = self.text_viewer.lock().unwrap_or_else(|p| p.into_inner());
            if m.list_visible {
                let mut open = true;
                egui::Window::new(t!("viewer.text_viewer.list_title"))
                    .open(&mut open)
                    .resizable(true)
                    .default_width(360.0)
                    .show(ctx, |ui| {
                        ui.label(
                            egui::RichText::new(t!("viewer.text_viewer.list_hint"))
                                .color(egui::Color32::from_gray(0xA0))
                                .size(11.0),
                        );
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .max_height(320.0)
                            .show(ui, |ui| {
                                clicked = m.list_ui(ui);
                            });
                    });
                if !open {
                    m.list_visible = false;
                }
            }
            if let Some(i) = clicked {
                if let Some(vp_id) = m.open_doc(i) {
                    ctx.send_viewport_cmd_to(vp_id, egui::ViewportCommand::Focus);
                }
            }
        }

        // --- One deferred viewport per open document. ---
        let open_docs: Vec<(std::path::PathBuf, String)> = {
            let m = self.text_viewer.lock().unwrap_or_else(|p| p.into_inner());
            m.docs
                .iter()
                .filter(|d| d.open)
                .map(|d| (d.path.clone(), d.title.clone()))
                .collect()
        };
        for (path, title) in open_docs {
            let vp_id = super::text_viewer::doc_viewport_id(&path);
            let builder = egui::ViewportBuilder::default()
                .with_title(format!("POPONE - {title}"))
                .with_inner_size([560.0, 520.0]);
            let model = Arc::clone(&self.text_viewer);
            ctx.show_viewport_deferred(vp_id, builder, move |child_ctx, _class| {
                let mut m = model.lock().unwrap_or_else(|p| p.into_inner());
                if let Some(doc) = m.docs.iter_mut().find(|d| d.path == path) {
                    super::text_viewer::draw_doc_window(child_ctx, doc);
                }
            });
        }
    }
}

/// State for first-frame validation/application of the window position.
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

        // Load Japanese fonts.
        Self::setup_cjk_fonts(&cc.egui_ctx);

        // Dark theme (Blender / Substance Painter style); colors are configurable via popone.toml [theme].
        let theme = app_config
            .as_ref()
            .map(|c| &c.theme)
            .cloned()
            .unwrap_or_default();
        Self::setup_dark_theme(&cc.egui_ctx, &theme);

        // Load splash image.
        let splash_texture = Self::load_splash_texture(&cc.egui_ctx);

        // Single-instance: start the IPC pipe listener.
        #[cfg(target_os = "windows")]
        let ipc_receiver = {
            let (tx, rx) = std::sync::mpsc::channel();
            super::single_instance::start_pipe_listener(tx, cc.egui_ctx.clone());
            rx
        };

        // Restore directory paths from the config.
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

        // For deferred restore of the window position (only if the config file has a [window] section).
        let saved_window = app_config.as_ref().and_then(|c| c.window.clone());
        let pending_window_restore = PendingWindowRestore {
            validated: saved_window.is_none(),
            saved_config: saved_window,
        };

        let app_config = app_config.unwrap_or_default();

        // Reflect the initial texture fallback color (white / magenta) globally.
        // All subsequent texture uploads (including BG loads) reference this value.
        super::texture::set_white_texture_fallback(app_config.display.white_texture_fallback);

        let display = DisplaySettings {
            white_texture_fallback: app_config.display.white_texture_fallback,
            panel_resizable: app_config.display.panel_resizable,
            panel_width: app_config.display.panel_width.clamp(280.0, 600.0),
            ..DisplaySettings::default()
        };

        // Load texture history.
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
            display,
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
            material_panel_height_px: 0.0,
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
            text_viewer: Arc::new(std::sync::Mutex::new(
                super::text_viewer::TextViewerModel::default(),
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
            uv_edit: uv_edit::UvEditState::default(),
            uv_edit_window_open: false,
            uv_edit_bg_tex: None,
            dark_theme_applied: false,
            theme_panel_bg: Self::theme_color(&theme.panel_bg, DARK_PANEL_BG),
            theme_border: Self::theme_color(&theme.border, DARK_BORDER_COLOR),
            next_request_id: 0,
            reload_snapshot: None,
            heartbeat: super::watchdog::start(Duration::from_secs(5), Duration::from_secs(2)),
            warmup_phase: WarmupPhase::NotStarted,
        }
    }

    /// Look up the stable key from `mat_idx`.
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
        // Noto Sans JP (OFL license) - primary Japanese face.
        const NOTO_SANS_JP: &[u8] = include_bytes!("../../../assets/NotoSansJP-Regular.ttf");
        // Noto Sans SC (OFL license) - simplified-Chinese fallback.
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
        // Fallback order JP -> SC (SC supplies glyphs missing from JP).
        let proportional = fonts
            .families
            .get_mut(&egui::FontFamily::Proportional)
            .expect("Proportional font family always exists");
        proportional.insert(0, "noto_sc".to_owned());
        proportional.insert(0, "noto_jp".to_owned());
        let monospace = fonts
            .families
            .get_mut(&egui::FontFamily::Monospace)
            .expect("Monospace font family always exists");
        monospace.push("noto_jp".to_owned());
        monospace.push("noto_sc".to_owned());
        ctx.set_fonts(fonts);
    }

    /// Decode the splash image from the embedded PNG and register it as an `egui` texture.
    fn load_splash_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
        static SPLASH_PNG: &[u8] = include_bytes!("../../../assets/popone_image.png");
        let image = image::load_from_memory(SPLASH_PNG).ok()?.into_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        let pixels = image.into_raw();
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
        Some(ctx.load_texture("splash", color_image, egui::TextureOptions::LINEAR))
    }

    /// Applies the v0-design-conformant dark theme.
    /// Converts a hex string to `Color32` (with a default).
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

        // Panel / window background.
        visuals.panel_fill = panel_bg;
        visuals.window_fill = panel_bg;

        // Border.
        let border_stroke = egui::Stroke::new(1.0, border);
        visuals.window_stroke = border_stroke;

        // Common widget text color.
        let fg = egui::Stroke::new(1.0, text_color);

        // noninteractive (labels, separators, etc.).
        visuals.widgets.noninteractive.bg_stroke = border_stroke;
        visuals.widgets.noninteractive.fg_stroke = fg;

        // inactive (button at rest).
        visuals.widgets.inactive.bg_fill = widget_bg;
        visuals.widgets.inactive.weak_bg_fill = widget_bg;
        visuals.widgets.inactive.bg_stroke = border_stroke;
        visuals.widgets.inactive.fg_stroke = fg;

        // hovered: accent color.
        visuals.widgets.hovered.bg_fill = accent;
        visuals.widgets.hovered.weak_bg_fill = accent;
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, accent);
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);

        // active (while pressed).
        visuals.widgets.active.bg_fill = active_color;
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, active_color);
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);

        // open (expanded ComboBox, etc.).
        visuals.widgets.open.bg_fill = egui::Color32::from_rgb(0x2A, 0x2A, 0x2A);
        visuals.widgets.open.bg_stroke = border_stroke;
        visuals.widgets.open.fg_stroke = fg;

        // Selection / accent.
        visuals.selection.bg_fill = accent;
        visuals.selection.stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);

        // Extreme background (e.g., interior of `TextEdit`).
        visuals.extreme_bg_color = egui::Color32::from_rgb(0x15, 0x15, 0x15);

        // Make the scrollbar thinner.
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

        // GPU resource build (uploaded directly from `IrTexture`).
        // On first load, `material_display` is not yet initialized, so use defaults.
        let display = if self.material_display.len() == ir.materials.len() {
            self.material_display.clone()
        } else {
            Self::default_material_display(&ir)
        };
        let mat_flags = Self::extract_per_mat_vecs(&display);
        let gpu_model = super::mesh::build_gpu_model_from_ir(&ir, device, queue, &mat_flags)?;
        self.finish_load_with_gpu(ir, gpu_model, source, false)
    }

    /// Variant that splits GPU texture uploads across frames (used after BG parse completes).
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

        // If `reload_snapshot` is `Some`, capture "via `reload_current`" status and
        // carry it through the split GPU build all the way to `finish_load_with_gpu`
        // (review_004 [P2]).
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

    /// Deferred-execution version of an append operation's GPU build.
    /// IR merge runs immediately; the merged IR is fed into the frame-split
    /// texture-upload pipeline. Until the build completes `self.loaded = None`
    /// (model is temporarily hidden).
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

    /// Deferred-execution append GPU build (variant carrying a `PkgAppend` payload).
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
        // Humanoid completion (same logic as `finish_append_ext`).
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

        // Resize `material_display`.
        let mc = loaded.ir.materials.len();
        self.material_display
            .resize_with(mc, MaterialDisplayState::default);
        let mat_flags = Self::extract_per_mat_vecs(&self.material_display);

        let anim_snapshot = pending::AnimationSnapshot::capture(self.anim.state.as_ref());

        // Decompose `loaded`: IR (merged) + GPU model + ownership fields.
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
            source: helpers::ReloadableSource::File(std::path::PathBuf::new()), // dummy (the primary source is used on append completion)
            append_info: Some(Box::new(append_info)),
            cpu_prep_rx: None,
            is_reload: false, // append is not a reload
        });
    }

    /// Roll back to the original model when an append GPU build fails.
    /// Truncates the merged IR back to the prior state and restores the old GPU model.
    pub(crate) fn rollback_append(&mut self, mut ir: IrModel, ai: pending::AppendGpuBuildInfo) {
        log::info!("Rolling back append: restoring original model via truncate");
        let mat_count = ai.ir_snapshot.material_count;
        ai.ir_snapshot.rollback(&mut ir);

        // Rebuild `LoadedModel` with the old GPU model + truncated IR.
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
        // Restore `material_display` to its original size.
        self.material_display.truncate(mat_count);
        // Invalidate renderer caches.
        if let Some(ref mut renderer) = self.renderer {
            renderer.invalidate_visualization_cache();
            renderer.invalidate_normal_cache();
        }
    }

    /// Append post-processing run after the deferred GPU build completes.
    /// Rebuilds `LoadedModel` from the merged IR + built GPU model taken from
    /// `PendingGpuBuild`, plus the old-`LoadedModel` info stashed in
    /// `AppendGpuBuildInfo`.
    pub(crate) fn finish_deferred_append(
        &mut self,
        ir: IrModel,
        mut gpu_model: super::mesh::GpuModel,
        mut ai: pending::AppendGpuBuildInfo,
    ) {
        let _t = std::time::Instant::now();
        // Initialize the renderer if not yet present.
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

        // Build MMD resources.
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

        // Release the viewport texture.
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

        // MaterialGroup: existing groups + the newly added group.
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

        // Append to `appended_models`.
        let display_path = ai.append_source.display_path().to_path_buf();
        own.appended_models.push(AppendedModel {
            source: ai.append_source,
            pkg_model_name: ai.pkg_model_name,
            pkg_model: ai.pkg_locator,
        });

        // Rebuild `LoadedModel`.
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

        // Update `last_dir`.
        if let Some(dir) = display_path.parent() {
            self.tex.last_dir = Some(dir.to_path_buf());
        }

        // Invalidate renderer caches.
        let t3 = std::time::Instant::now();
        if let Some(ref mut renderer) = self.renderer {
            renderer.invalidate_visualization_cache();
            renderer.invalidate_normal_cache();
            renderer.mark_sort_dirty();
            // Update the grid.
            if let Some(ref loaded) = self.loaded {
                let (bbox_min, bbox_max) = loaded.gpu_model.bbox();
                renderer.rebuild_grid(&self.render_state.device, bbox_min, bbox_max);
            }
        }
        log::info!(
            "[append_detail] renderer invalidate+grid: {}ms",
            t3.elapsed().as_millis()
        );

        // Rebuild the animation state.
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

        // Normalize the shader state.
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
            self.convert_message = Some(ConvertMessage::success(
                t!(
                    "viewer.toast.append.loaded",
                    name = ai.added_name,
                    bones = ai.added_bones,
                    merged = ai.merged_bones,
                    new = ai.new_bones,
                    meshes = ai.added_meshes,
                    materials = ai.added_materials
                )
                .into_owned(),
            ));
        }

        // PkgAppend post-processing (texture rename, texture matching, etc.).
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

    /// Post-processing after the PkgAppend deferred GPU build completes.
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

        // Build `pkg_prefix` using the last index of `appended_models`.
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
                // Generate thumbnails only for newly added entries (avoid full rebuild).
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

        // Batch progress toast (overwrites the success message).
        if let Some((current, total)) = payload.batch_progress {
            let name = payload.pkg_model_name.as_deref().unwrap_or("?");
            self.convert_message = Some(ConvertMessage::success(
                t!(
                    "viewer.toast.progress.loaded",
                    current = current,
                    total = total,
                    name = name
                )
                .into_owned(),
            ));
        }
    }

    /// Normalize the shader state for the currently loaded model.
    ///
    /// - Auto: pick Standard/MMD automatically based on the model format.
    /// - Mtoon/Unlit/GGX/Normal/MMD: keep the user's selection (consistency check only).
    pub(crate) fn normalize_shader_state(&mut self) {
        let has_mmd = self.loaded.as_ref().is_some_and(|l| {
            l.gpu_model
                .draws
                .iter()
                .any(|d| d.mmd_material_bind_group.is_some())
        });

        if self.display.auto_shader {
            // Auto mode: pick the MMD path automatically based on the model format.
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
            // Explicit user selection: drop only `use_mmd_path` when there are no MMD resources.
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
        // Material edit drawer (§A / P2 / review_004 [P2] full): keep the previous
        // material-edit state only on an explicit reload (A-stance / T-stance
        // conversion or via `reload_current`). Discard it otherwise (new load,
        // re-opening the same file, etc.).
        //
        // `is_reload` is passed from `PendingGpuBuild.is_reload` via the BG
        // pipeline; through synchronous paths (where `finish_load_with_gpu` is
        // called directly) `false` is passed. `PendingGpuBuild.is_reload`
        // captures `self.reload_snapshot.is_some()` at the time of
        // `start_deferred_gpu_build`, which avoids the multi-frame timing
        // problem of split GPU builds.
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

        // Initialize the renderer if absent, or invalidate the visualization cache.
        if self.renderer.is_none() {
            let device = &self.render_state.device;
            let queue = &self.render_state.queue;
            self.renderer = Some(GpuRenderer::new(device, queue, gpu_model.has_alpha));
        } else if let Some(ref mut renderer) = self.renderer {
            renderer.invalidate_visualization_cache();
            renderer.mark_sort_dirty();
        }

        // Initialize per-material flags first (used during MMD resource build).
        self.material_display = Self::default_material_display(&ir);

        // Build MMD resources.
        let emissive_vec: Vec<bool> = self.material_display.iter().map(|d| d.emissive).collect();
        self.prepare_mmd_for_model(&mut gpu_model, &ir, &emissive_vec);

        // Clear texture-assignment history (when loading a different model).
        self.tex.assignments.clear();
        self.tex.pkg_assignments.clear();
        // If an async texture dialog is open, discard its result
        // (prevents the previous model's material index from going stale).
        if self.tex.pending_file_dialog.is_some() {
            self.tex.pending_file_dialog = None;
        }
        // Discard PSD->PNG background conversions (prevents the previous model's tex_idx from going stale).
        self.tex.pending_psd_conversions.clear();
        // v0.5.2 [review_01 P1]: release the previous model's thumbnail `TextureId`s.
        // Without this, when the old and new models happen to have the same
        // texture count, `sync_ir_thumb_cache()` early-returns on the length
        // check, and the material edit window ends up displaying the previous
        // model's thumbnails for a different model.
        self.clear_ir_thumb_cache();
        // L3: properly release the egui `TextureId` held by `pending_tex_preview` before discarding.
        if let Some(preview) = self.tex.pending_preview.take() {
            self.cancel_tex_preview_inner(preview);
        }
        // L1: release the previous model's viewport texture ID.
        if let Some(tex_id) = self.viewport_texture_id.take() {
            let mut renderer = self.render_state.renderer.write();
            renderer.free_texture(&tex_id);
        }

        // Initialize morph sliders.
        self.morph_weights = vec![0.0; ir.morphs.len()];
        self.morph_dirty = false;
        // Initialize material visibility flags (DrawCall count may differ from material count, so size to draws.len).
        self.material_visibility = vec![true; gpu_model.draws.len()];
        self.export.export_visible_only = false;
        self.material_filter.clear();
        // Fit the camera to the model's bounding box.
        let (bbox_min, bbox_max) = gpu_model.bbox();
        self.camera.reset_to_bbox_with_margin(
            bbox_min,
            bbox_max,
            self.last_viewport_width,
            self.last_viewport_height,
        );
        // Rebuild the grid to match the model size.
        if let Some(ref mut renderer) = self.renderer {
            renderer.rebuild_grid(&self.render_state.device, bbox_min, bbox_max);
            renderer.mark_sort_dirty();
        }
        // Refit after the viewport size is settled (size may be undetermined on first load).
        self.pending.refit = true;

        // Default output path: a `.pmx` inside `converted_modelXX/`.
        // Prefer `output_base_dir` if set.
        let path = source.display_path();
        let base_dir = self
            .export
            .output_base_dir
            .as_deref()
            .unwrap_or_else(|| path.parent().unwrap_or(std::path::Path::new(".")));
        let converted_dir = crate::next_converted_dir(base_dir);
        // Initial display name for the model (no extension):
        //   - Normal file (FBX/VRM/PMX/...) -> that file name.
        //   - Archive (zip/7z/unitypackage) -> the archive file name
        //     (`ReloadableSource::Archive::display_path` points to the archive itself).
        //   - Prefab is overwritten later with the Prefab name (PkgInitial /
        //     synchronous Prefab load path in `file_io.rs`).
        //   - Append does not pass through this function and is preserved automatically.
        let initial_display_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(crate::sanitize_filename)
            .or_else(|| crate::sanitize_filename(&ir.name))
            .unwrap_or_else(|| "model".to_string());
        self.export.model_display_name = initial_display_name.clone();
        let pmx_name = format!("{initial_display_name}.pmx");
        self.export.pmx_output_path = converted_dir.join(&pmx_name).to_string_lossy().into_owned();

        // Build caches.
        let mat_cache = Self::build_mat_cache(&ir, &gpu_model);
        let stats_cache = CachedStats::new(&ir);

        // Emit texture-assignment logs.
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

        // On a new load: return the side panel to the Info tab.
        self.side_panel_tab = SidePanelTab::Info;
        // On a new load: also reset the UV edit state (cannot be reused because mesh/vertex indices change).
        self.uv_edit.reset();
        self.uv_edit_window_open = false;
        // Free the egui `TextureId` bound to the previous model's `TextureView` (prevents GPU leaks).
        if let Some((_, tex_id)) = self.uv_edit_bg_tex.take() {
            let mut renderer = self.render_state.renderer.write();
            renderer.free_texture(&tex_id);
        }
        // On a new load: reset the shader to its initial value, then normalize for the model format.
        self.display.shader_override = ShaderOverride::Default;
        self.display.use_mmd_path = false;
        self.display.auto_shader = true;
        self.normalize_shader_state();

        // Update the window title (based on `model_display_name`).
        self.window_title = Some(format!(
            "POPONE Model Viewer v{} - {}",
            env!("CARGO_PKG_VERSION"),
            self.export.model_display_name,
        ));

        if let Some(ref mut renderer) = self.renderer {
            renderer.invalidate_normal_cache();
        }

        // Material edit drawer (§H): snapshot the IR material values right after
        // load as pristine. Used by the "Reset to default" path that restores
        // from pristine.
        // **Important**: capture before the override's `apply_to`, otherwise
        // the post-apply values would become pristine, defeating the "initial
        // values" meaning.
        if let Some(loaded) = self.loaded.as_ref() {
            self.pristine_materials = loaded.ir.materials.clone();
        }

        // Material edit drawer (§A / A-stance support): on reload, reapply
        // `material_overrides` to the new IR in a batch, mark the matching
        // materials dirty, and rebuild bind groups on the next frame.
        // In Step 2, every §E section (shade / outline / rim / MatCap / UV
        // anim / emissive / normal / other) is consolidated into
        // `MaterialParamOverride`, so this single path handles edit-value
        // retention across A-stance / T-stance conversions for all sections.
        if is_reload && !self.material_overrides.is_empty() {
            // To avoid simultaneous borrows of `self.material_overrides` and
            // `self.loaded`, build the `(mat_idx, override_clone)` list first,
            // then borrow `loaded` mutably.
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

        // Step 4-16b / review_016: reload restoration of non-BaseColor texture
        // slot assignments. Re-read file paths recorded in
        // `slot_texture_paths`, then `assign_texture_core` uploads to the GPU
        // and sets the slot fields on `IrMaterial`.
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

    /// Build MMD resources on the `GpuModel`.
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

    /// Rebuild the GPU model when `smooth_normals` is toggled.
    pub fn rebuild_gpu_model(&mut self) {
        let Some(loaded) = &self.loaded else { return };
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;

        let mat_flags = Self::extract_per_mat_vecs(&self.material_display);
        match super::mesh::build_gpu_model_from_ir(&loaded.ir, device, queue, &mat_flags) {
            Ok(mut new_model) => {
                // Build MMD resources.
                if let Some(ref renderer) = self.renderer {
                    renderer.prepare_mmd_resources(
                        device,
                        &mut new_model,
                        &loaded.ir,
                        &mat_flags.emissive,
                    );
                }
                let mat_cache = Self::build_mat_cache(&loaded.ir, &new_model);
                // Preserve the material visibility state if the draw count is unchanged.
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
                // Rebuild the animation state with the new `gpu_model`.
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

    /// Reset the camera using the loaded model's bbox.
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

    /// Fit the camera to the loaded model's bbox.
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

    /// Generate the default per-material display state (HDR emissive defaults to OFF).
    fn default_material_display(ir: &IrModel) -> Vec<MaterialDisplayState> {
        // HDR emissive materials (any `emissive_factor` component > 1.0)
        // default to emission OFF.
        // The shader composes as `lit = lighting + rim + emissive`, so when
        // the factor > 1.0 a flat brightness is added across the whole
        // surface. For example, lilToon Screen-blend materials that are
        // barely > 1.0 even after a 0.5 attenuation (e.g. Shinano_face's
        // attenuated [0.89, 0.96, 1.06]) end up covering the texture and
        // blowing out to white. Users can still enable it manually for the
        // original behavior.
        // Removed in v0.2.40 once but restored because it caused viewer
        // display bugs for HDR materials.
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

    /// Expand `MaterialBuildFlags` from `material_display`.
    fn extract_per_mat_vecs(display: &[MaterialDisplayState]) -> super::mesh::MaterialBuildFlags {
        super::mesh::MaterialBuildFlags {
            smooth: display.iter().map(|d| d.smooth_normals).collect(),
            clear: display.iter().map(|d| d.clear_normals).collect(),
            normal_map: display.iter().map(|d| d.normal_map).collect(),
            emissive: display.iter().map(|d| d.emissive).collect(),
        }
    }

    /// Expand `material_display` when it matches the material count; otherwise generate defaults.
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

    /// Material edit drawer (§C / §E-1): mark the specified material for
    /// "rebuild bind group on the next frame".
    ///
    /// Instead of resizing `material_dirty` at every load path
    /// (`material_display` rebuild sites), this helper extends the
    /// `Vec<bool>` on demand. Concerns about leftover flags from older
    /// models are mitigated on the `apply_pending_material_rebuilds` side
    /// by clamping to `ir.materials.len()`, biased toward safety.
    pub fn mark_material_dirty(&mut self, mat_idx: usize) {
        let needed = mat_idx + 1;
        if self.material_dirty.len() < needed {
            self.material_dirty.resize(needed, false);
        }
        self.material_dirty[mat_idx] = true;
    }

    /// Material edit drawer (§C): pick up set `material_dirty` flags and call
    /// `rebuild_material_bind_groups`, regenerating bind groups on both the
    /// standard path and the MMD-compatible path.
    ///
    /// Called inside `update()` after UI drawing and before wgpu rendering;
    /// every dirty entry is fully consumed within a single frame.
    fn apply_pending_material_rebuilds(&mut self) {
        if !self.material_dirty.iter().any(|&d| d) {
            return;
        }
        let Some(renderer) = self.renderer.as_ref() else {
            // When the renderer is not yet initialized (e.g. during the splash), squash dirty flags.
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
        // Clamp to `ir.materials.len()` because dirty entries from an older model may remain.
        let dirty_len = self.material_dirty.len().min(mat_count);

        for mat_idx in 0..dirty_len {
            if self.material_dirty[mat_idx] {
                // Decide `uniform_only` based on whether textures may change.
                // `material_dirty` is also set on texture changes, so always do a full rebuild.
                renderer.rebuild_material_bind_groups(
                    device,
                    queue,
                    &mut loaded.gpu_model,
                    &loaded.ir,
                    mat_idx,
                    &flags,
                    false, // uniform_only: textures may change, so do a full rebuild.
                );

                // v0.5.1 review [P2]: material editor edits become the new
                // Expression base value.
                //
                // The old implementation captured `material_base_values`
                // only once at model load. After editing diffuse / emissive
                // / shade / rim / matcap / UV in the material editor,
                // playing an Expression composed from "load time" rather
                // than "post-edit". Re-capture via
                // `MaterialBaseValues::from_ir()` on dirty to reflect the
                // latest values.
                if let Some(mat) = loaded.ir.materials.get(mat_idx) {
                    if mat_idx < loaded.gpu_model.material_base_values.len() {
                        loaded.gpu_model.material_base_values[mat_idx] =
                            crate::viewer::mesh::MaterialBaseValues::from_ir(mat);
                    }
                }
            }
        }
        // Consume all dirty entries (clears even out-of-clamp older entries).
        self.material_dirty.fill(false);

        // v0.5.1 review 02 [P1]: reapply the Expression material bind after a
        // material-edit rebuild. The old implementation only wrote base
        // values to the uniform of dirty materials, so when a non-zero
        // Expression was held by manual morphs there was a bug where "the
        // Expression material effect disappears the moment you edit"
        // (overwritten on the next frame's `update_animation`, but does not
        // recover while playback is stopped or only manual morphs are
        // active).
        //
        // `accumulate_expression_materials` treats every material referenced
        // by Material morphs as dirty, so even materials not currently being
        // edited are reapplied correctly when they are under Expression
        // influence.
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

    /// Convert VRM `IrTexture`s (raw pixels) into PNG-encoded form.
    pub(crate) fn encode_ir_textures_as_png(ir: &mut IrModel, images: &[gltf::image::Data]) {
        use crate::intermediate::types::TextureData;
        use image::ImageEncoder;
        for (i, tex) in ir.textures.iter_mut().enumerate() {
            if let Some(img_data) = images.get(i) {
                let (w, h) = (img_data.width, img_data.height);
                let bytes = tex.data.as_bytes();
                // Build the RGBA image.
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

    /// Convert PSD data to PNG (delegates to `crate::psd::psd_to_png`).
    pub(crate) fn psd_to_png(psd_data: &[u8]) -> anyhow::Result<Vec<u8>> {
        crate::psd::psd_to_png(psd_data)
    }

    /// Cache window state from `ViewportInfo`; on the first frame validate and apply the position.
    fn update_viewport_config(&mut self, ctx: &egui::Context) {
        let mut restore_pos: Option<egui::Pos2> = None;

        ctx.input(|i| {
            let vp = i.viewport();
            let maximized = vp.maximized.unwrap_or(false);
            let minimized = vp.minimized.unwrap_or(false);

            // First frame: unconditionally restore the saved position.
            // `egui`'s `monitor_size` only returns the size of "the monitor
            // the window is currently on", so it cannot be used to decide
            // restoration to a sub-display. Always attempt restoration if
            // the size is positive.
            if !self.pending_window_restore.validated {
                if let Some(ref saved) = self.pending_window_restore.saved_config {
                    if saved.width >= 10.0 && saved.height >= 10.0 {
                        restore_pos = Some(egui::pos2(saved.x, saved.y));
                        log::info!("Window position restored: ({}, {})", saved.x, saved.y);
                    }
                }
                self.pending_window_restore.validated = true;
            }

            // Do not update position / size while maximized or minimized.
            if maximized || minimized {
                return;
            }

            // Position: `outer_rect` (matches the `OuterPosition` coordinate system).
            // Size: `inner_rect` (matches the `with_inner_size` coordinate system; prevents drift).
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

        // Send the viewport command outside `ctx.input()`.
        if let Some(pos) = restore_pos {
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
        }
    }

    /// Update the animation state (apply bones + apply morphs).
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

                // GPU reflection of the Expression material bind.
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
        // Watchdog: while minimized, `update()` is not guaranteed to be called,
        // so pause; in normal cases, tick to record responsiveness. Even when
        // idle, schedule a repaint every 3 s to keep the heartbeat alive (on a
        // freeze the thread itself blocks and is never executed).
        if ctx.input(|i| i.viewport().minimized == Some(true)) {
            self.heartbeat.pause();
        } else {
            self.heartbeat.tick();
        }
        ctx.request_repaint_after(Duration::from_secs(3));

        // Hidden option ([behavior] exit_on_escape in popone.toml, no GUI toggle):
        // Escape closes the main window immediately, same as the close button.
        if self.app_config.behavior.exit_on_escape
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // Dark theme: `new()`'s settings can be overwritten by eframe's
        // initialization, so reapply on the first `update()` (subsequent
        // frames skip via the flag).
        if !self.dark_theme_applied {
            Self::setup_dark_theme(ctx, &self.app_config.theme);
            self.dark_theme_applied = true;
        }

        // IPC: receive file paths from other processes.
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

        // Session settings: ViewportInfo cache + first-frame position validation.
        self.update_viewport_config(ctx);

        // Reset hover state (re-set during the UI frame).
        self.hovered_draw_indices.clear();

        // Apply pending window title update.
        if let Some(title) = self.window_title.take() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }

        // FPS measurement (frame-count method: derived from the frame count over the last 1 s).
        let now = Instant::now();
        let dt = if let Some(&last) = self.frame_times.back() {
            now.duration_since(last).as_secs_f32()
        } else {
            0.0
        };
        self.frame_times.push_back(now);
        // Drop entries older than 1 s.
        let window = Duration::from_secs(1);
        while self
            .frame_times
            .front()
            .is_some_and(|&t| now.duration_since(t) > window)
        {
            self.frame_times.pop_front();
        }
        // Update displayed FPS / ms (every 0.5 s; prevents flicker).
        if now.duration_since(self.fps_last_update).as_secs_f32() >= 0.5 {
            if self.frame_times.len() >= 2 {
                let span = now
                    .duration_since(*self.frame_times.front().expect("len >= 2 verified"))
                    .as_secs_f32();
                if span > 0.0 {
                    self.fps_display = (self.frame_times.len() - 1) as f32 / span;
                    self.frame_dt_ms = span / (self.frame_times.len() - 1) as f32 * 1000.0;
                }
            }
            self.fps_last_update = now;
        }

        // GPU warmup: pre-create pipelines in stages while the splash is shown.
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

        // Dark theme: explicitly set the panel background here (the theme itself
        // was set once in `new()`).
        let dark_panel = self.theme_panel_bg;
        let dark_border = egui::Stroke::new(1.0, self.theme_border);
        let panel_frame = egui::Frame::new()
            .fill(dark_panel)
            .stroke(dark_border)
            .inner_margin(egui::Margin::same(4));

        // Top bar.
        egui::TopBottomPanel::top("top_bar")
            .frame(panel_frame)
            .show(ctx, |bar| {
                bar.horizontal(|ui| {
                    // Top-bar button: transparent background normally; the
                    // global theme's blue takes effect on hover.
                    let border33 = self.theme_border;
                    ui.visuals_mut().widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
                    ui.visuals_mut().widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
                    ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::new(1.0, border33);

                    // Menu-button helper for the dark theme (lets the visuals win, no `.fill()`).
                    let menu_btn = |ui: &mut egui::Ui, label: &str| -> egui::Response {
                        let btn = egui::Button::new(
                            egui::RichText::new(label)
                                .color(egui::Color32::WHITE)
                                .size(12.0),
                        );
                        ui.add(btn)
                    };

                    if menu_btn(ui, &t!("viewer.topbar.open")).clicked() {
                        self.open_file_dialog(ctx);
                    }

                    if self.loaded.is_some()
                        && menu_btn(ui, &t!("viewer.topbar.append"))
                            .on_hover_text(t!("viewer.topbar.append_tooltip"))
                            .clicked()
                    {
                        self.open_append_dialog(ctx);
                    }

                    if menu_btn(ui, &t!("viewer.topbar.log"))
                        .on_hover_text(t!("viewer.topbar.log_tooltip"))
                        .clicked()
                    {
                        // `toggle_visible` snapshots `last_geometry` into
                        // `apply_geometry` on close, so the next open uses the
                        // same position.
                        let mut m = self.log_viewer.lock().unwrap_or_else(|p| p.into_inner());
                        m.toggle_visible();
                    }

                    // Archive text files (readme etc.): shown only when the
                    // current archive actually contains text documents.
                    {
                        let text_count = {
                            let m = self.text_viewer.lock().unwrap_or_else(|p| p.into_inner());
                            m.files.len()
                        };
                        if text_count > 0
                            && menu_btn(
                                ui,
                                &format!("{} ({})", t!("viewer.topbar.text"), text_count),
                            )
                            .on_hover_text(t!("viewer.topbar.text_tooltip"))
                            .clicked()
                        {
                            let mut m = self.text_viewer.lock().unwrap_or_else(|p| p.into_inner());
                            m.list_visible = !m.list_visible;
                        }
                    }

                    // Model name (editable). Reflected in both the title bar
                    // and the PMX output file name. Shares its value with the
                    // "Model name:" TextEdit on the right panel.
                    if self.loaded.is_some() {
                        ui.separator();
                        ui.label(
                            egui::RichText::new(t!("viewer.topbar.model_name_label"))
                                .color(egui::Color32::from_gray(0xB0))
                                .size(11.0),
                        );
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.export.model_display_name)
                                .desired_width(240.0)
                                .text_color(egui::Color32::WHITE)
                                .font(egui::FontId::proportional(12.0))
                                .hint_text(t!("viewer.topbar.model_name_hint")),
                        );
                        if response.changed() {
                            self.refresh_derived_from_display_name();
                        }
                    }

                    // Fit / Reset buttons + right-panel resizable toggle on the right edge.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if menu_btn(ui, &t!("viewer.topbar.reset"))
                            .on_hover_text(t!("viewer.topbar.reset_tooltip"))
                            .clicked()
                        {
                            self.camera_reset_to_model();
                        }
                        if menu_btn(ui, &t!("viewer.topbar.fit"))
                            .on_hover_text(t!("viewer.topbar.fit_tooltip"))
                            .clicked()
                        {
                            self.camera_fit_to_model();
                        }
                        // Toggle that makes the right tool panel's width
                        // user-resizable via drag.
                        // ON: variable in 280..=600 px via drag. OFF: fixed at 280 px.
                        // The current state is highlighted via `selected()`.
                        let resizable_label =
                            egui::RichText::new(t!("viewer.topbar.panel_resizable_label"))
                                .color(egui::Color32::WHITE)
                                .size(12.0);
                        let resizable_btn = egui::Button::new(resizable_label)
                            .selected(self.display.panel_resizable);
                        if ui
                            .add(resizable_btn)
                            .on_hover_text(t!("viewer.topbar.panel_resizable_hover"))
                            .clicked()
                        {
                            self.display.panel_resizable = !self.display.panel_resizable;
                        }
                    });
                });
            });

        // Right side panel.
        ui::show_side_panel(ctx, self);

        // Consume material-edit dirty flags (§C):
        // Look at `material_dirty` set by UI actions and call
        // `rebuild_material_bind_groups` to update bind groups on both the
        // standard path and the MMD-compatible path within the same frame.
        // Note: the material-edit panel itself is added after
        // `status_bar` / `shortcut_hints` so the stacking order ends up
        // "bottom = status_bar / middle = shortcut_hints / top = edit panel".
        self.apply_pending_material_rebuilds();

        // Texture D&D dialog + preview sync.
        ui::show_texture_drop_dialog(ctx, self);
        self.sync_tex_preview();

        // Status bar: file path + statistics.
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
                                "{}{}",
                                loaded.source.display_path().to_string_lossy(),
                                t!("viewer.statusbar.cached_suffix")
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
                            egui::RichText::new(t!("viewer.statusbar.no_model"))
                                .font(egui::FontId::proportional(11.0)),
                        );
                    }
                });
            });

        // Shortcut hint bar (above the status bar).
        egui::TopBottomPanel::bottom("shortcut_hints")
            .frame(panel_frame)
            .show(ctx, |ui| {
                let hint_color = egui::Color32::WHITE;
                let hint_font = egui::FontId::proportional(10.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(t!("viewer.shortcut.camera"))
                            .font(hint_font.clone())
                            .color(hint_color),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(t!("viewer.shortcut.toggles"))
                                .font(hint_font)
                                .color(hint_color),
                        );
                    });
                });
            });

        // Material edit panel (v0.5.3): pinned right above the shortcut hint bar.
        // The `TopBottomPanel` appears only when `editing_material_index` is
        // `Some`. The panel itself disappears when the [Edit] icon is OFF or
        // when [x] is pressed (the central viewport then expands).
        ui::show_material_editor_window(ctx, self);
        // UV edit window (v0.5.5 Phase 1): opened from a button on the material
        // edit panel. Floating `egui::Window` with a fixed Id to prevent
        // multiple instances.
        ui::show_uv_edit_window(ctx, self);

        // Central viewport.
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

                // Camera input.
                let response = viewport.allocate_rect(
                    egui::Rect::from_min_size(viewport.cursor().min, available),
                    egui::Sense::click_and_drag(),
                );
                self.camera.handle_input(ctx, &response);

                // Double-click: fit to the model.
                if response.double_clicked() {
                    self.camera_fit_to_model();
                }

                // Morph-weight change detection -> vertex buffer update.
                if self.morph_dirty {
                    if let Some(ref mut loaded) = self.loaded {
                        let queue = &self.render_state.queue;
                        loaded.gpu_model.apply_morphs(&self.morph_weights, queue);

                        // Expression material bind: also applied when the slider moves.
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

                // 3D rendering (take the renderer, use it as &mut, then return it).
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
                                overlay_h_pixels: self.material_panel_height_px,
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

                            // Show on egui.
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

                // Splash image (rounded-corner display at the viewport center while no model is loaded).
                if self.loaded.is_none() {
                    if let Some(ref tex) = self.splash_texture {
                        let tex_size = tex.size_vec2();
                        let rect = response.rect;
                        // Scale to fit the viewport.
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

                // Drop overlay.
                if self.drag_hovering {
                    let rect = response.rect;
                    let has_model = self.loaded.is_some();
                    let (overlay_color, overlay_text): (
                        egui::Color32,
                        std::borrow::Cow<'static, str>,
                    ) = if is_hover_image && has_model {
                        (
                            egui::Color32::from_rgba_unmultiplied(0x40, 0xC0, 0x40, 0x60),
                            t!("viewer.drop_overlay.assign_texture"),
                        )
                    } else if is_hover_image && !has_model {
                        (
                            egui::Color32::from_rgba_unmultiplied(0xD0, 0xA0, 0x40, 0x60),
                            t!("viewer.drop_overlay.load_model_first"),
                        )
                    } else if is_hover_model {
                        let shift = ctx.input(|i| i.modifiers.shift);
                        if shift && has_model {
                            (
                                egui::Color32::from_rgba_unmultiplied(0x40, 0xC0, 0xFF, 0x60),
                                t!("viewer.drop_overlay.add_model_shift"),
                            )
                        } else {
                            (
                                egui::Color32::from_rgba_unmultiplied(0x40, 0x80, 0xFF, 0x60),
                                t!("viewer.drop_overlay.load_model_file"),
                            )
                        }
                    } else {
                        (
                            egui::Color32::from_rgba_unmultiplied(0x80, 0x80, 0x80, 0x60),
                            t!("viewer.drop_overlay.unsupported_format"),
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

                // Record the viewport size (for fit computation).
                self.last_viewport_width = response.rect.width();
                self.last_viewport_height = response.rect.height();

                // Refit on first load (after the viewport size is settled).
                if self.pending.refit {
                    self.pending.refit = false;
                    self.camera_reset_to_model();
                }

                // Camera info (top-left, drawn as raw text).
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

                // FPS display (top-right overlay).
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

                // Persistent warning when A-stance / T-stance conversion fails (above the operation hints).
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

                // Operation hints (bottom-left, two lines, always visible).
                // Only show hints in the viewport while no model is loaded
                // (after loading, hints move to the status bar).
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

                // Progress overlay (loading / converting).
                self.paint_progress_overlay(viewport, response.rect, ctx);
                self.update_progress_flags(ctx);

                // Result-message overlay (5 s fade-out).
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
                        // Background band.
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
                        // Text.
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
                // Clear the message on timeout.
                if self
                    .convert_message
                    .as_ref()
                    .is_some_and(|cm| cm.elapsed_secs() >= 5.0)
                {
                    self.convert_message = None;
                }
            });

        // Render the log viewer window (a separate OS window).
        // No-op when `visible == false`.
        self.show_log_viewer(ctx);

        // Render the archive-text-viewer list window and document windows.
        // No-op when no text files are listed / opened.
        self.show_text_viewer_windows(ctx);
    }

    fn on_exit(&mut self) {
        // On normal exit do not flush the log buffer to a file.
        // Policy: "save nothing other than panic logs". On panic, the
        // panic hook in `main.rs` independently generates `panic_*.log`
        // via `flush_log_buffer`. Users who explicitly want to save can
        // use the "Save log" button inside the log viewer.

        // Reflect the log viewer's visibility / position / size / filter into the config.
        {
            let m = self.log_viewer.lock().unwrap_or_else(|p| p.into_inner());
            self.app_config.log_viewer = m.export_config();
        }

        // Reflect directory paths into the config.
        self.app_config.directory.last_model = self
            .last_model_dir
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned());
        self.app_config.directory.last_texture = self
            .tex
            .last_dir
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned());

        // Reflect display options (persisted ones only) into the config.
        self.app_config.display.white_texture_fallback = self.display.white_texture_fallback;
        self.app_config.display.panel_resizable = self.display.panel_resizable;
        self.app_config.display.panel_width = self.display.panel_width;

        persistence::save_config(&self.data_dir, &self.app_config);
    }
}
