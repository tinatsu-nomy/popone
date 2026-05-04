//! Texture assignment, preview, and pkg-texture handling.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui;
use eframe::wgpu;
use rust_i18n::t;

/// Result type for the PSD -> PNG background conversion.
type PsdConversionResult = anyhow::Result<Vec<u8>>;

/// Pending state for the PSD -> PNG background conversion.
pub struct PendingPsdConversion {
    /// Receiver channel for the conversion result.
    pub rx: std::sync::mpsc::Receiver<PsdConversionResult>,
    /// Index of the IrTexture to swap in once the conversion completes.
    pub tex_idx: usize,
    /// PNG file name to set after the conversion (e.g. "foo.png").
    pub png_filename: String,
    /// Original display name (used for logging).
    pub display_name: String,
}

use super::helpers::{is_temp_path, TextureSource};
use super::{ConvertMessage, GpuModel, ViewerApp};
use crate::intermediate::types::{TextureData, TextureSlot};

/// State for texture assignment and package textures.
pub struct TextureState {
    /// Manual texture assignment history (material index -> texture source).
    pub assignments: HashMap<usize, TextureSource>,
    /// Manual package-texture assignment history (material index -> texture name).
    pub pkg_assignments: HashMap<usize, String>,
    /// Texture D&D preview.
    pub pending_preview: Option<PendingTexPreview>,
    /// Manual unitypackage texture assignment dialog.
    pub pending_match: Option<PendingTexMatch>,
    /// Textures inside a unitypackage (held while the model is loaded).
    pub pkg_textures: Option<Vec<(String, Arc<[u8]>)>>,
    /// TextureId thumbnail cache for pkg_textures.
    pub pkg_thumb_cache: Vec<Option<egui::TextureId>>,
    /// TextureId thumbnail cache for loaded.ir.textures (used to display already-assigned textures).
    ///
    /// Added in v0.5.2. Used to show thumbnails of textures assigned to a
    /// material in the UI: the 64 px scaled-down version corresponding to
    /// `ir.textures[i]` is registered as an egui `TextureId` and held here.
    /// Kept in sync with `ir.textures.len()` via `sync_ir_thumb_cache()`.
    pub ir_thumb_cache: Vec<Option<egui::TextureId>>,
    /// Simultaneous texture assignment to materials with the same name.
    pub link_same_name: bool,
    /// Filter for the pkg-texture popup.
    pub pkg_popup_filter: String,
    /// Most recent directory used for opening a texture file.
    pub last_dir: Option<PathBuf>,
    /// Asynchronous texture file dialog (material index, TextureSlot, result receiver channel).
    /// Step 4-16b: added the slot info so it can be launched from the texture-select button of each section.
    pub pending_file_dialog: Option<(
        usize,
        TextureSlot,
        std::sync::mpsc::Receiver<Option<PathBuf>>,
    )>,
    /// Pending list of PSD -> PNG background conversions.
    pub pending_psd_conversions: Vec<PendingPsdConversion>,
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
            ir_thumb_cache: Vec::new(),
            link_same_name: true,
            pkg_popup_filter: String::new(),
            last_dir: None,
            pending_file_dialog: None,
            pending_psd_conversions: Vec::new(),
        }
    }
}

/// Texture D&D preview state.
pub struct PendingTexPreview {
    pub path: PathBuf,
    /// Cached bytes that have already been read (defends against the temp file disappearing).
    pub cached_data: Vec<u8>,
    /// Whether this is a PSD file.
    pub is_psd: bool,
    /// Whether the data was read from a temp path (decided before the file disappeared).
    pub was_temp: bool,
    /// Per-material selection state (checkboxes).
    pub selection: Vec<bool>,
    /// Materials currently in preview.
    pub previewed: Vec<bool>,
    /// Preview texture view (GPU).
    pub(super) texture_view: wgpu::TextureView,
    /// draw_index -> the original bind group that was saved aside.
    pub(super) saved_binds: HashMap<usize, Option<wgpu::BindGroup>>,
    /// egui TextureId for showing the thumbnail.
    pub preview_tex_id: Option<egui::TextureId>,
}

/// State of the manual unitypackage texture assignment dialog.
pub struct PendingTexMatch {
    /// List of indices of unassigned materials (indices into ir.materials).
    pub mat_indices: Vec<usize>,
    /// material index -> currently selected texture index (within pkg_textures).
    pub selections: Vec<Option<usize>>,
    /// Texture-name filter.
    pub tex_filter: String,
    /// Selection currently being previewed.
    pub previewed: Vec<Option<usize>>,
    /// draw_index -> the original (texture_bind_group, mmd_texture_bind_group) saved aside.
    pub saved_binds: HashMap<usize, (Option<wgpu::BindGroup>, Option<wgpu::BindGroup>)>,
    /// GPU TextureView for the pkg textures (index-aligned).
    pub texture_views: Vec<Option<wgpu::TextureView>>,
    /// Indices of textures whose upload has failed (prevents retry).
    pub failed_uploads: std::collections::HashSet<usize>,
}

/// Material info cached for UI display (avoids borrow constraints + per-frame clone).
pub struct CachedMaterialInfo {
    /// (draw_index, material_index)
    pub draw_indices: Vec<(usize, usize)>,
    /// Material name.
    pub names: Vec<String>,
    /// Texture index.
    pub tex_indices: Vec<Option<usize>>,
    /// Source texture file name (FBX origin).
    pub source_tex_names: Vec<Option<String>>,
    /// Count of materials with a texture set.
    pub tex_set_count: usize,
    /// Texture-status string for the status bar (avoids per-frame format!).
    pub tex_status_text: String,
}

impl ViewerApp {
    /// Build the material info cache.
    pub(super) fn build_mat_cache(
        ir: &crate::intermediate::types::IrModel,
        gpu_model: &GpuModel,
    ) -> CachedMaterialInfo {
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

    /// Refresh the material cache (after a texture assignment / material rename).
    pub(in crate::viewer) fn update_mat_cache(&mut self) {
        if let Some(ref mut loaded) = self.loaded {
            loaded.mat_cache = Self::build_mat_cache(&loaded.ir, &loaded.gpu_model);
        }
    }

    /// Upload pkg_textures thumbnails to the GPU and cache them.
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
            let is_psd = super::super::texture::is_psd_filename(name);
            match super::super::texture::create_thumbnail_rgba(data, is_psd, THUMB_SIZE) {
                Ok(rgba) => {
                    let (view, _) = super::super::texture::upload_rgba_to_gpu(
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
                    log::warn!("Thumbnail generation failed: {} - {}", name, e);
                    self.tex.pkg_thumb_cache.push(None);
                }
            }
        }
    }

    /// Generate thumbnails only for newly added pkg_textures and append them (incremental update).
    /// Entries from `start_index` onward are the newly added ones.
    pub fn append_pkg_thumb_cache(&mut self, start_index: usize) {
        let Some(ref pkg) = self.tex.pkg_textures else {
            return;
        };
        if start_index >= pkg.len() {
            return;
        }
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        let mut renderer = self.render_state.renderer.write();
        const THUMB_SIZE: u32 = 64;

        for (name, data) in pkg[start_index..].iter() {
            let is_psd = super::super::texture::is_psd_filename(name);
            match super::super::texture::create_thumbnail_rgba(data, is_psd, THUMB_SIZE) {
                Ok(rgba) => {
                    let (view, _) = super::super::texture::upload_rgba_to_gpu(
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
                    log::warn!("Thumbnail generation failed: {} - {}", name, e);
                    self.tex.pkg_thumb_cache.push(None);
                }
            }
        }
    }

    /// Clear the thumbnail cache.
    pub(super) fn clear_pkg_thumb_cache(&mut self) {
        let mut renderer = self.render_state.renderer.write();
        for tex_id in self.tex.pkg_thumb_cache.drain(..).flatten() {
            renderer.free_texture(&tex_id);
        }
    }

    /// Upload thumbnails of `loaded.ir.textures` to the GPU and register them in `ir_thumb_cache`.
    ///
    /// Added in v0.5.2. Used to show thumbnails of textures assigned to a slot in
    /// the material edit window. Like `pkg_thumb_cache`, the 64 px scaled-down
    /// version is registered with `register_native_texture` and the `egui::TextureId` is held.
    pub fn rebuild_ir_thumb_cache(&mut self) {
        self.clear_ir_thumb_cache();
        let Some(ref loaded) = self.loaded else {
            return;
        };
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        let mut renderer = self.render_state.renderer.write();
        const THUMB_SIZE: u32 = 64;

        for tex in loaded.ir.textures.iter() {
            let thumb_id =
                Self::build_ir_thumb_entry(tex, THUMB_SIZE, device, queue, &mut renderer);
            self.tex.ir_thumb_cache.push(thumb_id);
        }
    }

    /// Sync `ir_thumb_cache` with the length of `loaded.ir.textures` (incremental update).
    ///
    /// Call when `ir.textures` length changes, e.g. on model swap or texture addition.
    /// - When `loaded` is missing: clear all.
    /// - When the length shrank: discard existing and rebuild.
    /// - When the length grew: only upload the new tail.
    pub fn sync_ir_thumb_cache(&mut self) {
        let Some(ref loaded) = self.loaded else {
            if !self.tex.ir_thumb_cache.is_empty() {
                self.clear_ir_thumb_cache();
            }
            return;
        };
        let target_len = loaded.ir.textures.len();
        let cache_len = self.tex.ir_thumb_cache.len();
        if cache_len == target_len {
            return;
        }
        if cache_len > target_len {
            self.rebuild_ir_thumb_cache();
            return;
        }
        // The append-only case.
        self.append_ir_thumb_cache(cache_len);
    }

    /// Append thumbnails for new textures from `start_index` onward into `ir_thumb_cache` (incremental update).
    pub fn append_ir_thumb_cache(&mut self, start_index: usize) {
        let Some(ref loaded) = self.loaded else {
            return;
        };
        if start_index >= loaded.ir.textures.len() {
            return;
        }
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        let mut renderer = self.render_state.renderer.write();
        const THUMB_SIZE: u32 = 64;

        for tex in loaded.ir.textures[start_index..].iter() {
            let thumb_id =
                Self::build_ir_thumb_entry(tex, THUMB_SIZE, device, queue, &mut renderer);
            self.tex.ir_thumb_cache.push(thumb_id);
        }
    }

    /// Discard `ir_thumb_cache` and free the GPU resources.
    pub(super) fn clear_ir_thumb_cache(&mut self) {
        let mut renderer = self.render_state.renderer.write();
        for tex_id in self.tex.ir_thumb_cache.drain(..).flatten() {
            renderer.free_texture(&tex_id);
        }
    }

    /// Build a thumbnail TextureId from a single `IrTexture`.
    ///
    /// `TextureData::RawRgba` is resized directly; `Encoded` is decoded then resized.
    /// On failure returns `None`, and the UI side falls back to "no thumbnail".
    fn build_ir_thumb_entry(
        tex: &crate::intermediate::types::IrTexture,
        thumb_size: u32,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut eframe::egui_wgpu::Renderer,
    ) -> Option<egui::TextureId> {
        use crate::intermediate::types::TextureData;
        let rgba = match &tex.data {
            TextureData::RawRgba {
                pixels,
                width,
                height,
            } => {
                let img = image::RgbaImage::from_raw(*width, *height, pixels.to_vec())?;
                let resized = image::imageops::resize(
                    &img,
                    thumb_size,
                    thumb_size,
                    image::imageops::FilterType::Triangle,
                );
                resized.into_raw()
            }
            TextureData::Encoded(data) => {
                let is_psd = super::super::texture::is_psd_filename(&tex.filename);
                match super::super::texture::create_thumbnail_rgba(data, is_psd, thumb_size) {
                    Ok(rgba) => rgba,
                    Err(e) => {
                        log::warn!("ir thumb decode failed: {} - {}", tex.filename, e);
                        return None;
                    }
                }
            }
        };
        let (view, _) = super::super::texture::upload_rgba_to_gpu(
            device,
            queue,
            &rgba,
            thumb_size,
            thumb_size,
            Some("ir_thumb"),
        );
        Some(renderer.register_native_texture(device, &view, eframe::wgpu::FilterMode::Linear))
    }

    /// Assign an external texture (from a file path) to the given material.
    pub fn assign_texture_to_material(&mut self, material_index: usize, path: &Path) {
        // Read the file once (avoids double read).
        let tex_data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                log::error!("File read failed: {e}");
                self.convert_message = Some(ConvertMessage::failure(
                    t!("viewer.toast.tex.load_failed", error = e.to_string()).into_owned(),
                ));
                return;
            }
        };
        let ext_lower = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let data_arc = Arc::from(tex_data.into_boxed_slice());
        // Pass it in as Cached to avoid re-reading inside assign_texture_source_to_material.
        let cached_source = TextureSource::Cached {
            original_name: path.to_string_lossy().into_owned(),
            data: Arc::clone(&data_arc),
            is_psd: ext_lower == "psd",
        };
        // For history: a temp path is saved as Cached, a normal path as File (so reload can re-read it).
        let history_source = if is_temp_path(path) {
            TextureSource::Cached {
                original_name: path.to_string_lossy().into_owned(),
                data: data_arc,
                is_psd: ext_lower == "psd",
            }
        } else {
            TextureSource::File(path.to_path_buf())
        };
        self.assign_texture_source_to_material(material_index, &cached_source);
        // Override the history (for normal file paths, save as File to reduce memory usage).
        self.tex
            .assignments
            .insert(material_index, history_source.clone());
        if self.tex.link_same_name {
            if let Some(ref loaded) = self.loaded {
                let siblings = loaded.same_name_siblings(material_index);
                for sib_idx in siblings {
                    self.tex.assignments.insert(sib_idx, history_source.clone());
                }
            }
        }
    }

    /// Assign a TextureSource to the given material.
    pub fn assign_texture_source_to_material(
        &mut self,
        material_index: usize,
        tex_source: &TextureSource,
    ) {
        // Get bytes from the TextureSource.
        let (tex_data, is_psd, display_name) = match tex_source {
            TextureSource::File(path) => {
                let data = match std::fs::read(path) {
                    Ok(d) => d,
                    Err(e) => {
                        log::error!("File read failed: {e}");
                        self.convert_message = Some(ConvertMessage::failure(
                            t!("viewer.toast.tex.load_failed", error = e.to_string()).into_owned(),
                        ));
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

        if !self.assign_texture_core(
            material_index,
            TextureSlot::BaseColor,
            &tex_data,
            is_psd,
            &display_name,
        ) {
            return;
        }

        // File source: assign_texture_to_material that wraps this handles history override
        self.tex
            .assignments
            .insert(material_index, tex_source.clone());
        if self.tex.link_same_name {
            if let Some(ref loaded) = self.loaded {
                let siblings = loaded.same_name_siblings(material_index);
                for sib_idx in siblings {
                    self.tex.assignments.insert(sib_idx, tex_source.clone());
                }
            }
        }

        self.update_mat_cache();
    }

    /// Assign in-package texture data directly (from bytes) to a material.
    /// Returns true on success, false on decode / upload failure.
    pub fn assign_texture_data_to_material(
        &mut self,
        material_index: usize,
        tex_name: &str,
        tex_data: &[u8],
    ) -> bool {
        let is_psd = super::super::texture::is_psd_filename(tex_name);
        if !self.assign_texture_core(
            material_index,
            TextureSlot::BaseColor,
            tex_data,
            is_psd,
            tex_name,
        ) {
            return false;
        }
        self.update_mat_cache();
        true
    }

    /// GPU upload, IrTexture registration, material update, PSD BG conversion,
    /// linked sibling assignment -- shared by both file-path and raw-byte callers.
    /// Returns false on upload failure or missing loaded model.
    ///
    /// The `slot` argument corresponds to the `TextureSlot` enum introduced in §B.
    /// Step 4-16a implements the write path for all 11 slots. BaseColor updates
    /// the existing texture_bind_group immediately; the other slots rewrite the
    /// IrMaterial field + `mark_material_dirty` -> the bind group is regenerated
    /// in `rebuild_material_bind_groups`.
    pub(crate) fn assign_texture_core(
        &mut self,
        material_index: usize,
        slot: TextureSlot,
        tex_data: &[u8],
        is_psd: bool,
        display_name: &str,
    ) -> bool {
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;

        let (texture_view, texture_view_unorm) =
            match super::super::texture::upload_texture_from_bytes(tex_data, is_psd, device, queue)
            {
                Ok(views) => views,
                Err(e) => {
                    log::error!("Texture upload failed: {e}");
                    self.convert_message = Some(ConvertMessage::failure(
                        t!("viewer.toast.tex.load_failed", error = e.to_string()).into_owned(),
                    ));
                    return false;
                }
            };

        let Some(ref mut loaded) = self.loaded else {
            return false;
        };

        let basename = Path::new(display_name)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let ext_lower = if is_psd {
            "psd".to_string()
        } else {
            Path::new(display_name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase()
        };

        // PSD: keep raw PSD data temporarily; non-PSD: derive filename/mime from display_name
        let (ir_data, ir_filename, ir_mime, spawn_psd_bg) = if is_psd {
            let psd_filename = format!("{}.psd", basename);
            (
                tex_data.to_vec(),
                psd_filename,
                "image/vnd.adobe.photoshop".to_string(),
                true,
            )
        } else {
            let filename = Path::new(display_name)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let mime = crate::intermediate::types::mime_for_ext(&ext_lower);
            (tex_data.to_vec(), filename, mime.to_string(), false)
        };

        // Reuse existing IrTexture with same name+content (dedup)
        // review_012 [P1] fix: when this is a new texture, also push to gpu_texture_views.
        // Do not push when reusing an existing texture (TODO-1: keep alignment via the dedup condition).
        let existing_idx = loaded.ir.textures.iter().position(|t| {
            t.filename == ir_filename
                && t.data.len() == ir_data.len()
                && t.data.as_bytes() == ir_data
        });
        let tex_idx = if let Some(idx) = existing_idx {
            idx
        } else {
            let idx = loaded.ir.textures.len();
            loaded
                .ir
                .textures
                .push(crate::intermediate::types::IrTexture {
                    filename: ir_filename,
                    data: TextureData::Encoded(Arc::from(ir_data)),
                    mime_type: ir_mime,
                    source_path: display_name.to_string(),
                    mip_chain: None,
                });
            // [P1] New texture: also append to the GPU view array (so rebuild can index by tex_idx).
            loaded
                .gpu_model
                .push_gpu_texture_view(texture_view.clone(), texture_view_unorm.clone());
            // v0.5.2 [review_02 P1] fix: append the missing tail rather than a single push.
            // `ir_thumb_cache` stays empty (length 0) until the material edit window opens,
            // so a `push` without going through `sync_ir_thumb_cache()` would put the new
            // thumbnail at index 0, misaligning every existing slot's thumbnail.
            //
            // The correct behavior is "append everything missing from the current cache
            // length up to ir.textures length". Normally cache_len == textures.len() - 1,
            // so a single append; when the cache is uninitialized, build all entries
            // including the new texture in one shot.
            //
            // `device` / `queue` are already borrowed via `&self.render_state.device` /
            // queue, and `self.render_state.renderer` and `self.tex.ir_thumb_cache` are
            // disjoint fields, so the write lock and the cache push coexist here.
            {
                let mut renderer = self.render_state.renderer.write();
                let cache_len = self.tex.ir_thumb_cache.len();
                let target_len = loaded.ir.textures.len();
                if cache_len > target_len {
                    // Anomalous case where the cache is too long: drop everything and rebuild (should not happen).
                    for old in self.tex.ir_thumb_cache.drain(..).flatten() {
                        renderer.free_texture(&old);
                    }
                }
                let start = self.tex.ir_thumb_cache.len();
                for t in &loaded.ir.textures[start..] {
                    let thumb_id = Self::build_ir_thumb_entry(t, 64, device, queue, &mut renderer);
                    self.tex.ir_thumb_cache.push(thumb_id);
                }
            }
            idx
        };

        // PSD BG conversion
        if spawn_psd_bg {
            let psd_data = loaded.ir.textures[tex_idx].data.as_bytes().to_vec();
            let png_filename = format!("{}.png", basename);
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let result = crate::psd::psd_to_png(&psd_data);
                let _ = tx.send(result);
            });
            log::info!("PSD->PNG background conversion started: {}", display_name);
            self.tex.pending_psd_conversions.push(PendingPsdConversion {
                rx,
                tex_idx,
                png_filename,
                display_name: display_name.to_string(),
            });
        }

        // Step 4-16a: set the texture on the IrMaterial field corresponding to slot.
        Self::apply_texture_to_slot(&mut loaded.ir.materials[material_index], slot, tex_idx);

        // GPU DrawCall update:
        // - BaseColor: update the existing texture_bind_group immediately (no defer).
        // - Other slots: mark_material_dirty so the next frame rebuild_material_bind_groups
        //   regenerates every bind group (including mtoon_aux_bind_group).
        let needs_immediate_gpu_update = slot == TextureSlot::BaseColor;
        if needs_immediate_gpu_update {
            let texture_bgl = match self.renderer {
                Some(ref r) => r.texture_bgl(),
                None => return false,
            };
            Self::update_gpu_bind(
                &mut loaded.gpu_model,
                material_index,
                &texture_view,
                device,
                texture_bgl,
                &loaded.ir.materials[material_index],
            );
        }

        log::info!(
            "Texture assignment: mat[{}] '{}' slot={:?} <- {}",
            material_index,
            loaded.ir.materials[material_index].name,
            slot,
            display_name
        );

        // Linked sibling assignment
        if self.tex.link_same_name {
            let siblings = loaded.same_name_siblings(material_index);
            for sib_idx in siblings {
                Self::apply_texture_to_slot(&mut loaded.ir.materials[sib_idx], slot, tex_idx);
                if needs_immediate_gpu_update {
                    let texture_bgl = match self.renderer {
                        Some(ref r) => r.texture_bgl(),
                        None => continue,
                    };
                    Self::update_gpu_bind(
                        &mut loaded.gpu_model,
                        sib_idx,
                        &texture_view,
                        device,
                        texture_bgl,
                        &loaded.ir.materials[sib_idx],
                    );
                }
                log::info!("  Linked assignment: mat[{}] slot={:?}", sib_idx, slot);
            }
        }

        // Non-BaseColor slots queue the rebuild via mark_material_dirty.
        // To finish loaded's mut borrow first, we either return true and let
        // the caller invoke mark_material_dirty, or call it directly here
        // (allowed under NLL since loaded is unused after the if branch above).
        if !needs_immediate_gpu_update {
            // loaded's borrow ends in the block above, so self can be borrowed mutably.
            self.mark_material_dirty(material_index);
            if self.tex.link_same_name {
                if let Some(ref loaded) = self.loaded {
                    let siblings = loaded.same_name_siblings(material_index);
                    for sib_idx in siblings {
                        self.mark_material_dirty(sib_idx);
                    }
                }
            }
        }

        true
    }

    /// Step 4-16a: set the texture index on the IrMaterial field corresponding to slot.
    ///
    /// - BaseColor: existing texture_index + base_color_tex_info + apply_textured_defaults.
    /// - Emissive / Normal: direct IrMaterial fields.
    /// - Shade / ShadingShift / Rim / OutlineWidth / Matcap / UvAnimMask: MtoonParams fields
    ///   + review_012 [P2]: sync `shader_family = Mtoon` (the rendering-side prerequisite for
    ///     building the aux bind group along the MToon path).
    /// - Sphere / Toon: MMD-only fields.
    fn apply_texture_to_slot(
        mat: &mut crate::intermediate::types::IrMaterial,
        slot: TextureSlot,
        tex_idx: usize,
    ) {
        use crate::intermediate::types::{IrTextureInfo, ShaderFamily};

        // review_012 [P2]: when an MToon-side slot is assigned, sync shader_family to Mtoon.
        // Required so the rendering side selects the MToon path under shader_family-based
        // dispatch (review_007). §G's "explicit user action" includes texture-slot assignment.
        let is_mtoon_slot = matches!(
            slot,
            TextureSlot::ShadeMultiply
                | TextureSlot::ShadingShift
                | TextureSlot::RimMultiply
                | TextureSlot::OutlineWidth
                | TextureSlot::Matcap
                | TextureSlot::UvAnimMask
        );
        if is_mtoon_slot {
            mat.shader_family = ShaderFamily::Mtoon;
        }

        match slot {
            TextureSlot::BaseColor => {
                mat.texture_index = Some(tex_idx);
                match mat.base_color_tex_info.as_mut() {
                    Some(info) => info.index = tex_idx,
                    None => {
                        mat.base_color_tex_info = Some(IrTextureInfo::from_index(tex_idx));
                    }
                }
                mat.apply_textured_defaults();
            }
            TextureSlot::Emissive => {
                mat.emissive_texture = Some(IrTextureInfo::from_index(tex_idx));
            }
            TextureSlot::Normal => {
                mat.normal_texture = Some(IrTextureInfo::from_index(tex_idx));
            }
            TextureSlot::ShadeMultiply => {
                mat.mtoon_mut().shade_texture = Some(IrTextureInfo::from_index(tex_idx));
            }
            TextureSlot::ShadingShift => {
                mat.mtoon_mut().shading_shift_texture = Some(IrTextureInfo::from_index(tex_idx));
            }
            TextureSlot::RimMultiply => {
                mat.mtoon_mut().rim_multiply_texture = Some(IrTextureInfo::from_index(tex_idx));
            }
            TextureSlot::OutlineWidth => {
                mat.mtoon_mut().outline_width_texture = Some(IrTextureInfo::from_index(tex_idx));
            }
            TextureSlot::Matcap => {
                mat.mtoon_mut().matcap_texture = Some(IrTextureInfo::from_index(tex_idx));
            }
            TextureSlot::UvAnimMask => {
                mat.mtoon_mut().uv_animation_mask_texture =
                    Some(IrTextureInfo::from_index(tex_idx));
            }
            TextureSlot::Sphere => {
                mat.sphere_texture_index = Some(tex_idx);
                if mat.sphere_mode == 0 {
                    mat.sphere_mode = 1; // multiply is the default
                }
            }
            TextureSlot::Toon => {
                mat.toon_texture_index = Some(tex_idx);
                mat.toon_shared_index = None; // switch to per-material toon
            }
        }
    }

    /// Update GPU bind group for a material and clear stale MMD bind group.
    fn update_gpu_bind(
        gpu_model: &mut GpuModel,
        material_index: usize,
        texture_view: &wgpu::TextureView,
        device: &wgpu::Device,
        texture_bgl: &wgpu::BindGroupLayout,
        mat: &crate::intermediate::types::IrMaterial,
    ) {
        let sampler_info = mat
            .base_color_tex_info
            .as_ref()
            .map(|ti| ti.sampler)
            .unwrap_or_default();
        gpu_model.assign_texture_to_material(
            material_index,
            texture_view,
            device,
            texture_bgl,
            &sampler_info,
        );
        for draw in &mut gpu_model.draws {
            if draw.material_index == material_index {
                draw.mmd_texture_bind_group = None;
            }
        }
    }

    /// Open one texture in the preview dialog.
    pub(super) fn open_texture_preview(&mut self, path: PathBuf) {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let is_psd = ext == "psd";
        // Decide the temp-path check before the file disappears (canonicalize requires file existence).
        let was_temp = is_temp_path(&path);
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                self.convert_message = Some(ConvertMessage::failure(
                    t!("viewer.toast.tex.load_failed", error = e.to_string()).into_owned(),
                ));
                return;
            }
        };
        match super::super::texture::upload_texture_from_bytes(
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
                self.convert_message = Some(ConvertMessage::failure(
                    t!("viewer.toast.tex.load_failed", error = e.to_string()).into_owned(),
                ));
            }
        }
    }

    /// Auto-assign multiple textures (matching by file name and material name).
    pub(super) fn auto_assign_textures(&mut self, image_files: Vec<PathBuf>) {
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

        // File name -> collect indices of matching materials.
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
            // Find materials whose name contains the file name (without extension).
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

        // Run the assignments.
        for (path, mat_indices) in assignments {
            for &mat_idx in &mat_indices {
                self.assign_texture_to_material(mat_idx, &path);
                assigned += 1;
            }
        }

        // Result message.
        let mut msg = t!("viewer.toast.tex.auto_assigned", count = assigned).into_owned();
        if !unmatched.is_empty() {
            msg += &t!(
                "viewer.toast.tex.no_match_suffix",
                names = unmatched.join(", ")
            );
        }
        if assigned > 0 {
            self.convert_message = Some(ConvertMessage::success(msg));
        } else {
            self.convert_message = Some(ConvertMessage::failure(
                t!(
                    "viewer.toast.tex.no_match_failure",
                    names = unmatched.join(", ")
                )
                .into_owned(),
            ));
        }
    }

    /// Sync the texture preview (apply the diff between selection and previewed to the GPU).
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
                // Apply preview: save the original bind group aside and swap in the preview one.
                for (draw_idx, draw) in loaded.gpu_model.draws.iter_mut().enumerate() {
                    if draw.material_index == mat_idx {
                        if let std::collections::hash_map::Entry::Vacant(e) =
                            preview.saved_binds.entry(draw_idx)
                        {
                            e.insert(draw.texture_bind_group.take());
                        }
                        draw.texture_bind_group =
                            Some(super::super::gpu::create_texture_bind_group(
                                device,
                                texture_bgl,
                                &preview.texture_view,
                                sampler,
                            ));
                    }
                }
                preview.previewed[mat_idx] = true;
            } else if !preview.selection[mat_idx] && preview.previewed[mat_idx] {
                // Cancel preview: restore the original bind group from the saved aside.
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

    /// Commit the texture preview as the final assignment.
    pub fn apply_tex_preview(&mut self) {
        let Some(preview) = self.tex.pending_preview.take() else {
            return;
        };
        let path = &preview.path;

        // Collect the indices of the selected materials.
        let selected: Vec<usize> = preview
            .selection
            .iter()
            .enumerate()
            .filter_map(|(i, &v)| if v { Some(i) } else { None })
            .collect();

        if selected.is_empty() {
            // Nothing selected -> revert.
            self.cancel_tex_preview_inner(preview);
            return;
        }

        // Add the texture into the IrModel (only once).
        let Some(ref mut loaded) = self.loaded else {
            return;
        };

        let is_psd = preview.is_psd;
        let tex_data = preview.cached_data.clone();

        // For temp paths, retain bytes for caching (using the flag decided before file disappearance).
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

        // For PSD, temporarily build IrTexture with raw PSD bytes; the BG thread converts to PNG.
        let (ir_data, ir_filename, ir_mime, spawn_psd_bg) = if is_psd {
            // Hold meta info that matches the actual bytes until conversion completes.
            let psd_filename = format!("{}.psd", basename);
            (
                tex_data.clone(),
                psd_filename,
                "image/vnd.adobe.photoshop".to_string(),
                true,
            )
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
            (tex_data, filename, mime.to_string(), false)
        };

        // PNG file name to set after BG PSD conversion completes.
        let png_filename_for_bg = if spawn_psd_bg {
            Some(format!("{}.png", basename))
        } else {
            None
        };

        let tex_idx = loaded.ir.textures.len();
        loaded
            .ir
            .textures
            .push(crate::intermediate::types::IrTexture {
                filename: ir_filename,
                data: TextureData::Encoded(Arc::from(ir_data)),
                mime_type: ir_mime,
                source_path: path.display().to_string(),
                mip_chain: None,
            });

        // v0.5.2 [review_02 P1] fix: catch ir_thumb_cache up to the current length.
        // If the user commits preview -> apply while the material edit window is closed,
        // a `push` without going through `sync_ir_thumb_cache()` would put the new
        // thumbnail at index 0 and misalign every existing slot's display.
        // Bulk-appending the missing tail (cache_len .. ir.textures.len()) guarantees
        // that the new texture lands at the correct `tex_idx`.
        {
            let device = &self.render_state.device;
            let queue = &self.render_state.queue;
            let mut renderer = self.render_state.renderer.write();
            let cache_len = self.tex.ir_thumb_cache.len();
            let target_len = loaded.ir.textures.len();
            if cache_len > target_len {
                for old in self.tex.ir_thumb_cache.drain(..).flatten() {
                    renderer.free_texture(&old);
                }
            }
            let start = self.tex.ir_thumb_cache.len();
            for t in &loaded.ir.textures[start..] {
                let thumb_id = Self::build_ir_thumb_entry(t, 64, device, queue, &mut renderer);
                self.tex.ir_thumb_cache.push(thumb_id);
            }
        }

        // For PSD, start the PNG conversion on the BG thread.
        if spawn_psd_bg {
            let psd_data = loaded.ir.textures[tex_idx].data.as_bytes().to_vec();
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let result = crate::psd::psd_to_png(&psd_data);
                let _ = tx.send(result);
            });
            let display = path.display().to_string();
            log::info!("PSD->PNG background conversion started: {}", display);
            self.tex.pending_psd_conversions.push(PendingPsdConversion {
                rx,
                tex_idx,
                png_filename: png_filename_for_bg.unwrap(),
                display_name: display,
            });
        }

        // Update texture_index on the selected materials.
        let path_buf = path.clone();
        for &mat_idx in &selected {
            let mat = &mut loaded.ir.materials[mat_idx];
            mat.texture_index = Some(tex_idx);
            mat.apply_textured_defaults();
            log::info!(
                "Texture assignment: mat[{}] '{}' <- {}",
                mat_idx,
                mat.name,
                path_buf.display()
            );
        }

        // Record the assignment history (for restoration on reload_current).
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

        // Free the egui thumbnail texture.
        if let Some(tex_id) = preview.preview_tex_id {
            let mut renderer = self.render_state.renderer.write();
            renderer.free_texture(&tex_id);
        }

        // The GPU is already in the preview state -> drop saved_binds and commit.
        // Restore any non-previewed entries left in saved_binds.
        for (draw_idx, orig) in preview.saved_binds.into_iter() {
            let draw = &mut loaded.gpu_model.draws[draw_idx];
            if !selected.contains(&draw.material_index) {
                draw.texture_bind_group = orig;
            }
        }

        // Refresh the material cache.
        self.update_mat_cache();
    }

    /// Cancel the texture preview (revert).
    pub fn cancel_tex_preview(&mut self) {
        let Some(preview) = self.tex.pending_preview.take() else {
            return;
        };
        self.cancel_tex_preview_inner(preview);
    }

    pub(super) fn cancel_tex_preview_inner(&mut self, preview: PendingTexPreview) {
        // Free the egui thumbnail texture.
        if let Some(tex_id) = preview.preview_tex_id {
            let mut renderer = self.render_state.renderer.write();
            renderer.free_texture(&tex_id);
        }
        let Some(ref mut loaded) = self.loaded else {
            return;
        };
        // Restore every saved bind group.
        for (draw_idx, orig) in preview.saved_binds.into_iter() {
            if draw_idx < loaded.gpu_model.draws.len() {
                loaded.gpu_model.draws[draw_idx].texture_bind_group = orig;
            }
        }
    }

    /// Initialize the TextureView slots for pkg textures (deferred load).
    /// The actual GPU upload happens on demand inside sync_tex_match_preview when selected.
    pub fn prepare_tex_match_views(&mut self) {
        let Some(ref mut pending) = self.tex.pending_match else {
            return;
        };
        if !pending.texture_views.is_empty() {
            return; // already initialized
        }
        let pkg_count = self.tex.pkg_textures.as_ref().map(|p| p.len()).unwrap_or(0);
        if pkg_count > 0 {
            pending.texture_views = vec![None; pkg_count];
        }
    }

    /// Real-time preview sync for the manual texture-match dialog.
    /// Apply the diff between selections and previewed to the GPU bind groups.
    /// Textures are uploaded to GPU on demand at selection time (avoids VRAM spikes).
    pub fn sync_tex_match_preview(&mut self) {
        let Some(ref mut pending) = self.tex.pending_match else {
            return;
        };
        let Some(ref mut loaded) = self.loaded else {
            return;
        };
        let Some(ref renderer) = self.renderer else {
            return;
        };
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        let texture_bgl = renderer.texture_bgl();

        for i in 0..pending.mat_indices.len() {
            let mat_idx = pending.mat_indices[i];
            let sel = pending.selections[i];
            let prev = pending.previewed[i];

            if sel == prev {
                continue;
            }

            if let Some(tex_idx) = sel {
                // On-demand upload: upload now if not yet uploaded.
                if tex_idx < pending.texture_views.len()
                    && pending.texture_views[tex_idx].is_none()
                    && !pending.failed_uploads.contains(&tex_idx)
                {
                    if let Some(ref pkg) = self.tex.pkg_textures {
                        if let Some((name, data)) = pkg.get(tex_idx) {
                            let is_psd = super::super::texture::is_psd_filename(name);
                            match super::super::texture::upload_texture_from_bytes(
                                data, is_psd, device, queue,
                            ) {
                                Ok((view, _unorm)) => {
                                    pending.texture_views[tex_idx] = Some(view);
                                }
                                Err(e) => {
                                    log::warn!("pkg texture upload failed ({}): {e}", name);
                                    pending.failed_uploads.insert(tex_idx);
                                }
                            }
                        }
                    }
                }

                // Acquire the texture view (on failure, restore the existing preview — including same-name siblings).
                let Some(Some(ref view)) = pending.texture_views.get(tex_idx) else {
                    if prev.is_some() {
                        let fail_targets: Vec<usize> = if self.tex.link_same_name {
                            let mut t = vec![mat_idx];
                            t.extend(loaded.same_name_siblings(mat_idx));
                            t
                        } else {
                            vec![mat_idx]
                        };
                        for (draw_idx, draw) in loaded.gpu_model.draws.iter_mut().enumerate() {
                            if fail_targets.contains(&draw.material_index) {
                                if let Some((orig_tex, orig_mmd)) =
                                    pending.saved_binds.remove(&draw_idx)
                                {
                                    draw.texture_bind_group = orig_tex;
                                    draw.mmd_texture_bind_group = orig_mmd;
                                }
                            }
                        }
                        pending.previewed[i] = None;
                    }
                    continue;
                };

                // With link_same_name, propagate to materials with the same name (within the same MaterialGroup).
                let target_mats: Vec<usize> = if self.tex.link_same_name {
                    let mut targets = vec![mat_idx];
                    targets.extend(loaded.same_name_siblings(mat_idx));
                    targets
                } else {
                    vec![mat_idx]
                };

                for &target in &target_mats {
                    let sampler_info = loaded
                        .ir
                        .materials
                        .get(target)
                        .and_then(|m| m.base_color_tex_info.as_ref())
                        .map(|ti| ti.sampler)
                        .unwrap_or_default();
                    let sampler =
                        super::super::mesh::create_sampler_from_info(device, &sampler_info);

                    for (draw_idx, draw) in loaded.gpu_model.draws.iter_mut().enumerate() {
                        if draw.material_index == target {
                            if let std::collections::hash_map::Entry::Vacant(e) =
                                pending.saved_binds.entry(draw_idx)
                            {
                                e.insert((
                                    draw.texture_bind_group.take(),
                                    draw.mmd_texture_bind_group.take(),
                                ));
                            }
                            let new_bg = super::super::gpu::create_texture_bind_group(
                                device,
                                texture_bgl,
                                view,
                                &sampler,
                            );
                            draw.texture_bind_group = Some(new_bg);
                            // Set the MMD side to None so the MMD path also reads texture_bind_group.
                            draw.mmd_texture_bind_group = None;
                        }
                    }
                }
            } else {
                // Selection cleared -> restore the original bind group (including same-name materials within the same MaterialGroup).
                let target_mats: Vec<usize> = if self.tex.link_same_name {
                    let mut targets = vec![mat_idx];
                    targets.extend(loaded.same_name_siblings(mat_idx));
                    targets
                } else {
                    vec![mat_idx]
                };
                for &target in &target_mats {
                    for (draw_idx, draw) in loaded.gpu_model.draws.iter_mut().enumerate() {
                        if draw.material_index == target {
                            if let Some((orig_tex, orig_mmd)) =
                                pending.saved_binds.remove(&draw_idx)
                            {
                                draw.texture_bind_group = orig_tex;
                                draw.mmd_texture_bind_group = orig_mmd;
                            }
                        }
                    }
                }
            }
            pending.previewed[i] = sel;
        }
    }

    /// Cancel the manual-texture-match preview (restore the original bind groups).
    pub fn cancel_tex_match_preview(&mut self) {
        let Some(pending) = self.tex.pending_match.take() else {
            return;
        };
        let Some(ref mut loaded) = self.loaded else {
            return;
        };
        for (draw_idx, (orig_tex, orig_mmd)) in pending.saved_binds.into_iter() {
            if draw_idx < loaded.gpu_model.draws.len() {
                loaded.gpu_model.draws[draw_idx].texture_bind_group = orig_tex;
                loaded.gpu_model.draws[draw_idx].mmd_texture_bind_group = orig_mmd;
            }
        }
        // If a D&D preview was alive at the same time, restoring bind groups offsets the display,
        // so reset previewed and let the next frame's sync_tex_preview re-apply.
        if let Some(ref mut preview) = self.tex.pending_preview {
            preview.previewed.iter_mut().for_each(|v| *v = false);
        }
    }

    // -----------------------------------------------------------------------
    // Texture assignment history (popone_history.json)
    // -----------------------------------------------------------------------

    /// Decide whether the current model is history-eligible and return the key.
    pub fn texture_history_key(&self) -> Option<String> {
        use super::helpers::ReloadableSource;
        let loaded = self.loaded.as_ref()?;
        if !loaded.appended_models.is_empty() {
            return None;
        }
        // v0.5.0: extended to return a history key for every format (VRM / PMX / FBX /
        // OBJ etc.) so material parameter edits can be persisted. The previous
        // version was limited to FBX / OBJ; since not only the texture history
        // but also MaterialParamOverride is saved under the same key, return a
        // key for every format on a File source.
        match &loaded.source {
            ReloadableSource::File(path) => Some(super::persistence::normalize_path(path)),
            _ => None,
        }
    }

    /// Save the current texture assignments + material parameter edit deltas to history.
    pub fn do_save_texture_history(&mut self) {
        let Some(key) = self.texture_history_key() else {
            return;
        };
        let Some(loaded) = self.loaded.as_ref() else {
            return;
        };

        // Texture assignment entries (BaseColor: v1 compatibility).
        let mut entries: Vec<super::persistence::TextureHistoryEntry> = self
            .tex
            .assignments
            .iter()
            .filter_map(|(mat_idx, src)| {
                if let TextureSource::File(path) = src {
                    let mat_name = loaded
                        .ir
                        .materials
                        .get(*mat_idx)
                        .map(|m| m.name.clone())
                        .unwrap_or_default();
                    Some(super::persistence::TextureHistoryEntry {
                        material_index: *mat_idx,
                        material_name: mat_name,
                        texture_path: path.to_string_lossy().into_owned(),
                        slot: crate::intermediate::types::TextureSlot::BaseColor,
                    })
                } else {
                    None
                }
            })
            .collect();

        // v0.5.1 added (M5): assignment entries for auxiliary slots.
        for ((mat_idx, slot), path) in &self.slot_texture_paths {
            let mat_name = loaded
                .ir
                .materials
                .get(*mat_idx)
                .map(|m| m.name.clone())
                .unwrap_or_default();
            entries.push(super::persistence::TextureHistoryEntry {
                material_index: *mat_idx,
                material_name: mat_name,
                texture_path: path.to_string_lossy().into_owned(),
                slot: *slot,
            });
        }

        // v0.5.0 added: material parameter edit deltas (§I minimal persistence).
        // Compute the diff against pristine_materials and save.
        // review_025 [P2]: mme_kind is not on IrMaterial so diff_from cannot pick it up.
        // Transcribe it from material_overrides and include it in the save.
        let param_entries: Vec<super::persistence::MaterialParamOverrideEntry> = loaded
            .ir
            .materials
            .iter()
            .enumerate()
            .filter_map(|(mat_idx, mat)| {
                let pristine = self.pristine_materials.get(mat_idx)?;
                let mut diff =
                    super::material_edit::MaterialParamOverride::diff_from(pristine, mat);
                // Transcribe mme_kind from material_overrides.
                let mme_kind = self
                    .material_overrides
                    .get(&mat_idx)
                    .and_then(|o| o.mme_kind);
                if mme_kind.is_some() {
                    diff.get_or_insert_with(Default::default).mme_kind = mme_kind;
                }
                let diff = diff?;
                Some(super::persistence::MaterialParamOverrideEntry {
                    material_index: mat_idx,
                    material_name: mat.name.clone(),
                    overrides: diff,
                })
            })
            .collect();

        // v0.5.5 added: per-vertex UV edit deltas (Phase 1).
        let uv_entries: Vec<super::persistence::VertexUvOverrideEntry> = self.uv_edit.to_entries();

        if entries.is_empty() && param_entries.is_empty() && uv_entries.is_empty() {
            self.convert_message = Some(ConvertMessage::failure(
                t!("viewer.toast.history.no_target").into_owned(),
            ));
            return;
        }

        let tex_count = entries.len();
        let param_count = param_entries.len();
        let uv_count = uv_entries.len();
        self.texture_history.history.insert(key.clone(), entries);
        // review_011 [P2] fix: when param_entries is empty, explicitly drop the old param_overrides.
        // Leaving them around would make a "recall history" after the user reset every edit
        // re-apply the old override.
        if !param_entries.is_empty() {
            self.texture_history
                .param_overrides
                .insert(key.clone(), param_entries);
        } else {
            self.texture_history.param_overrides.remove(&key);
        }
        // v0.5.5: same policy for vertex UV edits (drop explicitly when empty).
        if !uv_entries.is_empty() {
            self.texture_history
                .vertex_uv_overrides
                .insert(key, uv_entries);
        } else {
            self.texture_history.vertex_uv_overrides.remove(&key);
        }
        super::persistence::save_texture_history(&self.data_dir, &self.texture_history);
        self.convert_message = Some(ConvertMessage::success(
            t!(
                "viewer.toast.history.saved",
                tex_count = tex_count,
                param_count = param_count,
                uv_count = uv_count
            )
            .into_owned(),
        ));
    }

    /// Recall texture assignments from history.
    pub fn do_recall_texture_history(&mut self) {
        let Some(key) = self.texture_history_key() else {
            return;
        };
        // review_011 [P1] fix: continue even when texture history is empty as long as param_overrides exist.
        // Avoids the "no history for this model" early return when only parameter edits were saved.
        let has_tex_entries = self.texture_history.history.contains_key(&key);
        let has_param_entries = self.texture_history.param_overrides.contains_key(&key);
        // v0.5.5: also allow recall when only vertex UV edits were saved.
        let has_uv_entries = self.texture_history.vertex_uv_overrides.contains_key(&key);
        if !has_tex_entries && !has_param_entries && !has_uv_entries {
            self.convert_message = Some(ConvertMessage::failure(
                t!("viewer.toast.history.no_history").into_owned(),
            ));
            return;
        }
        let entries = self
            .texture_history
            .history
            .get(&key)
            .cloned()
            .unwrap_or_default();

        // Collect resolutions first (so loaded's immutable borrow is closed).
        // v0.5.1 M5: extended to (mat_idx, slot, path) 3-tuple. Allows multiple slots per material.
        let resolved: Vec<(usize, crate::intermediate::types::TextureSlot, PathBuf)>;
        let mut skipped = 0usize;
        {
            let Some(loaded) = self.loaded.as_ref() else {
                return;
            };
            // v0.5.1 M5: use (mat_idx, slot) as the duplicate-detection key.
            let mut seen = std::collections::HashSet::new();
            let mut tmp = Vec::new();
            for entry in &entries {
                let Some(mat_idx) =
                    super::persistence::resolve_material(&loaded.ir.materials, entry)
                else {
                    skipped += 1;
                    continue;
                };
                if !seen.insert((mat_idx, entry.slot)) {
                    continue;
                }
                let tex_path = PathBuf::from(&entry.texture_path);
                if !tex_path.is_file() {
                    log::warn!(
                        "Texture history: file not found, skipping: {} (slot={:?})",
                        entry.texture_path,
                        entry.slot
                    );
                    skipped += 1;
                    continue;
                }
                tmp.push((mat_idx, entry.slot, tex_path));
            }
            resolved = tmp;
        }

        // v0.5.1 review [P1] fix: order correction — restore pristine before applying texture / params.
        //
        // The previous implementation went texture restore -> pristine restore -> param restore,
        // and the pristine restore cleared the auxiliary-slot texture references
        // (IrMaterial.emissive_texture etc.), so the restored auxiliary slots all disappeared.
        //
        // review_012 [P2] fix: before applying the saved diff, revert every material to pristine
        // and clear material_overrides. This eliminates "unsaved edits before recall remain" and
        // guarantees "recall = exact reproduction of the saved state".
        {
            let mat_count = if let Some(loaded) = self.loaded.as_mut() {
                for (i, mat) in loaded.ir.materials.iter_mut().enumerate() {
                    if let Some(pristine) = self.pristine_materials.get(i) {
                        *mat = pristine.clone();
                    }
                }
                loaded.ir.materials.len()
            } else {
                0
            };
            // dirty flags are batched after the loaded borrow is released.
            for i in 0..mat_count {
                self.mark_material_dirty(i);
            }
        }
        self.material_overrides.clear();
        // The pristine restore also wipes auxiliary-slot references, so clear slot_texture_paths.
        // The texture restore loop just below re-sets it via resolved.
        self.slot_texture_paths.clear();
        // v0.5.1 review 02 [P1] fix: also clear BaseColor tex.assignments / pkg_assignments.
        // Without this, when the history has no BaseColor entry the previous implementation
        // left the old path behind, causing "old BaseColor leaks into the next save" and
        // "old bind shows on the GPU side". Pristine restore = save-time reproduction, so
        // assignments are returned to the baseline as well.
        self.tex.assignments.clear();
        self.tex.pkg_assignments.clear();

        // Temporarily disable link_same_name (same pattern as reload_current).
        let saved_link = self.tex.link_same_name;
        self.tex.link_same_name = false;

        let mut applied = 0usize;
        for (mat_idx, slot, tex_path) in &resolved {
            self.convert_message = None;
            // v0.5.1 M5: branch by slot for BaseColor vs auxiliary slots.
            if *slot == crate::intermediate::types::TextureSlot::BaseColor {
                self.assign_texture_to_material(*mat_idx, tex_path);
            } else {
                // Auxiliary slot: read the file and call assign_texture_core directly.
                let data = match std::fs::read(tex_path) {
                    Ok(d) => d,
                    Err(e) => {
                        log::warn!(
                            "Texture history (aux slot): failed to read {}: {}",
                            tex_path.display(),
                            e
                        );
                        skipped += 1;
                        continue;
                    }
                };
                let is_psd = tex_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("psd"))
                    .unwrap_or(false);
                let display_name = tex_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                self.assign_texture_core(*mat_idx, *slot, &data, is_psd, &display_name);
                // Also record the auxiliary slot path in slot_texture_paths.
                self.slot_texture_paths
                    .insert((*mat_idx, *slot), tex_path.clone());
            }
            // assign_texture_to_material sets convert_message to Failure on failure.
            let failed = self
                .convert_message
                .as_ref()
                .is_some_and(|m| matches!(m.result, super::ConvertResult::Failure(_)));
            if failed {
                skipped += 1;
            } else {
                applied += 1;
            }
        }

        self.tex.link_same_name = saved_link;

        // Use the same "resolve -> apply" 2-phase split as the texture restore to
        // avoid an immutable borrow (resolve) clashing with a mutable borrow (apply).
        let mut param_applied = 0usize;
        let resolved_params: Vec<(usize, super::material_edit::MaterialParamOverride)> = {
            let param_entries = self
                .texture_history
                .param_overrides
                .get(&key)
                .cloned()
                .unwrap_or_default();
            let Some(loaded) = self.loaded.as_ref() else {
                return;
            };
            param_entries
                .into_iter()
                .filter_map(|entry| {
                    let dummy = super::persistence::TextureHistoryEntry {
                        material_index: entry.material_index,
                        material_name: entry.material_name,
                        texture_path: String::new(),
                        slot: crate::intermediate::types::TextureSlot::BaseColor,
                    };
                    let mat_idx =
                        super::persistence::resolve_material(&loaded.ir.materials, &dummy)?;
                    Some((mat_idx, entry.overrides))
                })
                .collect()
        };
        for (mat_idx, overrides) in resolved_params {
            self.material_overrides.insert(mat_idx, overrides.clone());
            if let Some(loaded) = self.loaded.as_mut() {
                if let Some(mat) = loaded.ir.materials.get_mut(mat_idx) {
                    overrides.apply_to(mat);
                }
            }
            self.mark_material_dirty(mat_idx);
            param_applied += 1;
            // v0.5.5: vertex UV edits are restored in a single batch downstream (see uv_applied below).
        }

        // v0.5.5 added: restore vertex UV edits (write to IR + sync GPU).
        let mut uv_applied = 0usize;
        if has_uv_entries {
            let uv_entries = self
                .texture_history
                .vertex_uv_overrides
                .get(&key)
                .cloned()
                .unwrap_or_default();
            if !uv_entries.is_empty() {
                self.uv_edit.stage_restore(uv_entries);
                if let Some(loaded) = self.loaded.as_mut() {
                    self.uv_edit.apply_pending_restore(&mut loaded.ir);
                    let queue = self.render_state.queue.clone();
                    loaded.gpu_model.sync_uvs_from_ir(&loaded.ir, &queue);
                    uv_applied = self.uv_edit.overrides.len();
                }
            }
        }

        let msg = if skipped > 0 || param_applied > 0 || uv_applied > 0 {
            let parts: Vec<String> = [
                if applied > 0 {
                    Some(t!("viewer.toast.history.tex_n", count = applied).into_owned())
                } else {
                    None
                },
                if param_applied > 0 {
                    Some(t!("viewer.toast.history.param_n", count = param_applied).into_owned())
                } else {
                    None
                },
                if uv_applied > 0 {
                    Some(t!("viewer.toast.history.uv_n", count = uv_applied).into_owned())
                } else {
                    None
                },
                if skipped > 0 {
                    Some(t!("viewer.toast.history.skip_n", count = skipped).into_owned())
                } else {
                    None
                },
            ]
            .into_iter()
            .flatten()
            .collect();
            t!(
                "viewer.toast.history.recall_summary",
                parts = parts.join(", ")
            )
            .into_owned()
        } else if applied > 0 {
            t!("viewer.toast.history.tex_applied", count = applied).into_owned()
        } else {
            t!("viewer.toast.history.empty").into_owned()
        };
        self.convert_message = Some(if applied > 0 || param_applied > 0 || uv_applied > 0 {
            ConvertMessage::success(msg)
        } else {
            ConvertMessage::failure(msg)
        });
    }

    /// Poll the PSD -> PNG background conversion results and swap the IrTexture in.
    pub(super) fn poll_pending_psd_conversions(&mut self) {
        if self.tex.pending_psd_conversions.is_empty() {
            return;
        }

        let loaded = match self.loaded.as_mut() {
            Some(l) => l,
            None => {
                // Drop everything if the model was unloaded.
                self.tex.pending_psd_conversions.clear();
                return;
            }
        };

        // Process completed conversions in reverse order (so indices do not shift).
        let mut i = 0;
        while i < self.tex.pending_psd_conversions.len() {
            match self.tex.pending_psd_conversions[i].rx.try_recv() {
                Ok(Ok(png_data)) => {
                    let conv = self.tex.pending_psd_conversions.remove(i);
                    // Swap the IrTexture's data / file name / MIME from PSD to PNG.
                    if conv.tex_idx < loaded.ir.textures.len() {
                        let tex = &mut loaded.ir.textures[conv.tex_idx];
                        tex.data = TextureData::Encoded(Arc::from(png_data));
                        tex.filename = conv.png_filename;
                        tex.mime_type = "image/png".to_string();
                        log::info!(
                            "PSD->PNG background conversion completed: {} (tex_idx={})",
                            conv.display_name,
                            conv.tex_idx,
                        );

                        // v0.5.2 [review_01 P2]: regenerate the thumbnail after the PSD -> PNG swap.
                        // `sync_ir_thumb_cache()` decides by length comparison, so an in-place
                        // update would not rebuild and a `None` left by a PSD decode failure
                        // would stay empty forever.
                        let tex_ref: &crate::intermediate::types::IrTexture =
                            &loaded.ir.textures[conv.tex_idx];
                        let device = &self.render_state.device;
                        let queue = &self.render_state.queue;
                        let mut renderer = self.render_state.renderer.write();
                        if let Some(Some(old_id)) =
                            self.tex.ir_thumb_cache.get(conv.tex_idx).copied()
                        {
                            renderer.free_texture(&old_id);
                        }
                        let new_id =
                            Self::build_ir_thumb_entry(tex_ref, 64, device, queue, &mut renderer);
                        if conv.tex_idx < self.tex.ir_thumb_cache.len() {
                            self.tex.ir_thumb_cache[conv.tex_idx] = new_id;
                        }
                    } else {
                        log::warn!(
                            "PSD->PNG conversion result discarded (tex_idx {} out of range): {}",
                            conv.tex_idx,
                            conv.display_name,
                        );
                    }
                    // Do not advance i (remove shifted things).
                }
                Ok(Err(e)) => {
                    let conv = self.tex.pending_psd_conversions.remove(i);
                    log::warn!(
                        "PSD->PNG background conversion failed: {} - {}",
                        conv.display_name,
                        e,
                    );
                    // On failure, the raw PSD data stays in the IrTexture.
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // Still converting.
                    i += 1;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    let conv = self.tex.pending_psd_conversions.remove(i);
                    log::warn!(
                        "PSD->PNG background conversion thread disconnected: {}",
                        conv.display_name,
                    );
                }
            }
        }
    }
}
