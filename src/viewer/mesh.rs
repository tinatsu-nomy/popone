use std::collections::{HashMap, HashSet};

use anyhow::Result;
use eframe::wgpu;
use glam::Vec3;
use wgpu::util::DeviceExt;

use crate::convert::coord::{
    flip_face_winding, gltf_normal_to_pmx, gltf_normal_to_pmx_v0, gltf_pos_to_pmx,
    gltf_pos_to_pmx_v0,
};
use crate::intermediate::types::{
    AlphaMode, CullMode, IrMagFilter, IrMaterial, IrMinFilter, IrModel, IrMorphKind, IrSamplerInfo,
    IrTextureInfo, IrWrapMode, MaterialColorBindType, OutlineWidthMode, ShaderFamily,
};

use super::gpu::{self, Vertex};

/// Per-material build flags bundled into a single struct to keep
/// function signatures compact and prevent parallel-slice mismatch.
#[derive(Debug, Clone)]
pub struct MaterialBuildFlags {
    pub smooth: Vec<bool>,
    pub clear: Vec<bool>,
    pub normal_map: Vec<bool>,
    pub emissive: Vec<bool>,
}

impl MaterialBuildFlags {
    /// All-default flags for `mat_count` materials
    /// (smooth/clear off, normal_map/emissive on).
    pub fn default_for(mat_count: usize) -> Self {
        Self {
            smooth: vec![false; mat_count],
            clear: vec![false; mat_count],
            normal_map: vec![true; mat_count],
            emissive: vec![true; mat_count],
        }
    }
}

/// Morph data deduplicated and coordinate-converted into GPU space.
#[allow(clippy::type_complexity)]
pub(crate) enum GpuMorphEntry {
    /// Vertex morph: (gpu_vi, pos_delta, normal_delta, tangent_delta).
    Vertex(Vec<(u32, [f32; 3], [f32; 3], [f32; 3])>),
    /// Group morph: (sub-morph index, weight).
    Group(Vec<(usize, f32)>),
    /// Material morph: VRM 1.0 Expression's materialColorBinds / textureTransformBinds.
    Material {
        color_binds: Vec<crate::intermediate::types::IrMaterialColorBind>,
        uv_binds: Vec<crate::intermediate::types::IrTextureTransformBind>,
    },
    /// UV morph (Phase 3 A-2): add (gpu_vi, [du, dv]) to `channel` (0 = UV0, 1 = UV1).
    /// channel >= 2 is kept as an empty Vec because the GPU vertex has no UV2..UV4 (apply is a no-op).
    Uv {
        channel: u8,
        offsets: Vec<(u32, [f32; 2])>,
    },
}

/// Render style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderStyle {
    Standard,
    Mmd,
}

/// Render queue based on the MToon spec (draw-order category).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RenderQueue {
    /// Opaque (depth write enabled).
    Opaque = 0,
    /// Cut out by alphaCutoff (depth write enabled).
    Mask = 1,
    /// Translucent with depth write (MToon transparentWithZWrite).
    BlendZWrite = 2,
    /// Translucent without depth write.
    Blend = 3,
}

/// Per-material draw info.
pub struct DrawCall {
    pub index_offset: u32,
    pub index_count: u32,
    pub cull_mode: CullMode,
    pub is_alpha: bool,
    /// Render queue per the MToon spec.
    pub render_queue: RenderQueue,
    /// renderQueueOffsetNumber (used for sorting within BLEND).
    pub render_queue_offset: i32,
    /// alphaCutoff for MASK mode.
    pub alpha_cutoff: f32,
    pub texture_bind_group: Option<wgpu::BindGroup>,
    /// Material uniform buffer (UNIFORM | COPY_DST). Partially updatable via `queue.write_buffer`.
    pub material_buf: wgpu::Buffer,
    pub material_bind_group: wgpu::BindGroup,
    pub material_index: usize,
    pub render_style: RenderStyle,
    pub has_edge: bool,
    /// MToon outline target.
    pub has_outline: bool,
    /// MToon auxiliary texture bind group (group 3: matcap + shade + shift + rim + uvMask).
    pub mtoon_aux_bind_group: Option<wgpu::BindGroup>,
    /// Centroid of the draw mesh (for translucent distance sorting).
    pub center: glam::Vec3,
    // MMD bind groups (set in prepare_mmd_resources).
    pub mmd_material_buf: Option<wgpu::Buffer>,
    pub mmd_material_bind_group: Option<wgpu::BindGroup>,
    pub mmd_aux_bind_group: Option<wgpu::BindGroup>,
    /// MMD texture bind group (uses the Unorm view).
    pub mmd_texture_bind_group: Option<wgpu::BindGroup>,
}

/// Model data on the GPU.
pub struct GpuModel {
    pub vertex_buf: wgpu::Buffer,
    pub index_buf: wgpu::Buffer,
    pub draws: Vec<DrawCall>,
    pub has_alpha: bool,
    /// Edge-scale buffer (for MMD edges; f32 x vertex count).
    pub edge_scale_buf: Option<wgpu::Buffer>,
    /// GPU texture views, sRGB (for standard rendering).
    pub gpu_texture_views: Vec<wgpu::TextureView>,
    /// GPU texture views, Unorm (for MMD rendering).
    pub gpu_texture_views_unorm: Vec<wgpu::TextureView>,
    /// Base vertices (before morphs).
    base_vertices: Vec<Vertex>,
    /// Raw index buffer data (used by the normal-display filter).
    base_indices: Vec<u32>,
    /// IrModel global vertex index -> GPU vertex index.
    global_to_gpu: Vec<u32>,
    /// Whether to use the VRM 0.0 coordinate transform.
    use_vrm0_coords: bool,
    /// Cached bounding box (min, max).
    cached_bbox: (Vec3, Vec3),
    /// Working buffer for morph application (avoids cloning every frame).
    morph_work: Vec<Vertex>,
    /// GPU-space morph data (deduplicated and coordinate-converted).
    pub(crate) gpu_morphs: Vec<GpuMorphEntry>,
    /// Buffer for group-morph cycle detection (avoids per-call alloc).
    morph_visited: Vec<bool>,
    /// Morph weights from the previous apply (skip recomputation if unchanged).
    last_weights: Vec<f32>,
    /// Morph-weight cache invalidation flag (used when an animation is detached, etc.).
    morph_cache_dirty: bool,
    /// Cache of animated vertices (for normal-display sync).
    animated_vertices: Option<Vec<Vertex>>,
    /// Base material parameter values at load time, used for Expression material blending.
    pub material_base_values: Vec<MaterialBaseValues>,
}

/// Base values for Expression material binds (material parameters captured at load time).
/// Used by Expression's additive blend `final = base + Σ((target - base) * weight)`.
#[derive(Debug, Clone)]
pub struct MaterialBaseValues {
    pub diffuse: [f32; 4],
    pub emissive_factor: [f32; 3],
    pub shade_color: [f32; 3],
    pub matcap_factor: [f32; 3],
    pub rim_color: [f32; 3],
    pub outline_color: [f32; 4],
    pub base_uv_offset: [f32; 2],
    pub base_uv_scale: [f32; 2],
}

impl MaterialBaseValues {
    /// Capture base values from an `IrMaterial`.
    pub fn from_ir(mat: &IrMaterial) -> Self {
        let mp = mat.mtoon();
        let uv = mat
            .base_color_tex_info
            .as_ref()
            .map(|ti| (ti.offset, ti.scale))
            .unwrap_or((glam::Vec2::ZERO, glam::Vec2::ONE));
        Self {
            diffuse: mat.diffuse.to_array(),
            emissive_factor: mat.emissive_factor.to_array(),
            shade_color: mp.shade_color.unwrap_or(Vec3::ZERO).to_array(),
            matcap_factor: mp.matcap_factor.to_array(),
            rim_color: mp.parametric_rim_color.to_array(),
            outline_color: mat.edge_color.to_array(),
            base_uv_offset: uv.0.to_array(),
            base_uv_scale: uv.1.to_array(),
        }
    }
}

/// Accumulate Expression material binds: combine the weights of every active
/// Expression into per-material parameter deltas, and return `MaterialParams`
/// only for materials that actually changed.
pub(crate) fn accumulate_expression_materials(
    gpu_morphs: &[GpuMorphEntry],
    morph_weights: &[f32],
    base_values: &[MaterialBaseValues],
    ir_materials: &[IrMaterial],
    mat_count: usize,
    flags: &MaterialBuildFlags,
) -> Vec<Option<gpu::MaterialParams>> {
    // Accumulator: per-material delta of each color property.
    #[derive(Default)]
    struct ColorAccum {
        diffuse: [f32; 4],
        emissive: [f32; 3],
        shade: [f32; 3],
        matcap: [f32; 3],
        rim: [f32; 3],
        outline: [f32; 4],
        uv_offset: [f32; 2],
        uv_scale: [f32; 2],
        dirty: bool,
    }

    let mut accum: Vec<ColorAccum> = (0..mat_count).map(|_| ColorAccum::default()).collect();

    // v0.5.1 review [P1] fix: pre-mark every material referenced by a Material
    // morph as dirty.
    //
    // The previous implementation skipped morphs with weight < 1e-6 entirely.
    // On the frame an Expression dropped from 1.0 back to 0.0, the
    // "write the base value back" path never ran and the last applied color /
    // UV stayed on the GPU side.
    //
    // Fix: always mark referenced materials as dirty. With weight = 0 the
    // accumulator is zero, so the final value = base (the base is restored by
    // write_material_buffer). In practice, the materials Expressions touch are
    // a handful (face skin, eyes, lips, etc.), so the per-frame overhead is small.
    for entry in gpu_morphs.iter() {
        if let GpuMorphEntry::Material {
            color_binds,
            uv_binds,
        } = entry
        {
            for b in color_binds {
                if b.material_index < mat_count {
                    accum[b.material_index].dirty = true;
                }
            }
            for b in uv_binds {
                if b.material_index < mat_count {
                    accum[b.material_index].dirty = true;
                }
            }
        }
    }

    for (morph_idx, entry) in gpu_morphs.iter().enumerate() {
        let weight = morph_weights.get(morph_idx).copied().unwrap_or(0.0);
        if weight.abs() < 1e-6 {
            continue;
        }
        if let GpuMorphEntry::Material {
            color_binds,
            uv_binds,
        } = entry
        {
            for b in color_binds {
                let mi = b.material_index;
                if mi >= mat_count {
                    continue;
                }
                let base = &base_values[mi];
                let a = &mut accum[mi];
                a.dirty = true;
                match b.bind_type {
                    MaterialColorBindType::Color => {
                        for i in 0..4 {
                            a.diffuse[i] += (b.target_value[i] - base.diffuse[i]) * weight;
                        }
                    }
                    MaterialColorBindType::EmissionColor => {
                        for i in 0..3 {
                            a.emissive[i] += (b.target_value[i] - base.emissive_factor[i]) * weight;
                        }
                    }
                    MaterialColorBindType::ShadeColor => {
                        for i in 0..3 {
                            a.shade[i] += (b.target_value[i] - base.shade_color[i]) * weight;
                        }
                    }
                    MaterialColorBindType::MatcapColor => {
                        for i in 0..3 {
                            a.matcap[i] += (b.target_value[i] - base.matcap_factor[i]) * weight;
                        }
                    }
                    MaterialColorBindType::RimColor => {
                        for i in 0..3 {
                            a.rim[i] += (b.target_value[i] - base.rim_color[i]) * weight;
                        }
                    }
                    MaterialColorBindType::OutlineColor => {
                        for i in 0..4 {
                            a.outline[i] += (b.target_value[i] - base.outline_color[i]) * weight;
                        }
                    }
                }
            }
            for b in uv_binds {
                let mi = b.material_index;
                if mi >= mat_count {
                    continue;
                }
                let base = &base_values[mi];
                let a = &mut accum[mi];
                a.dirty = true;
                for i in 0..2 {
                    a.uv_offset[i] += (b.offset[i] - base.base_uv_offset[i]) * weight;
                    a.uv_scale[i] += (b.scale[i] - base.base_uv_scale[i]) * weight;
                }
            }
        }
    }

    // Compute final values only for dirty materials and return MaterialParams.
    accum
        .into_iter()
        .enumerate()
        .map(|(mi, a)| {
            if !a.dirty || mi >= ir_materials.len() {
                return None;
            }
            let base = &base_values[mi];
            // Clone the IrMaterial temporarily and apply the accumulation result.
            let mut mat = ir_materials[mi].clone();
            mat.diffuse = glam::Vec4::from_array([
                base.diffuse[0] + a.diffuse[0],
                base.diffuse[1] + a.diffuse[1],
                base.diffuse[2] + a.diffuse[2],
                base.diffuse[3] + a.diffuse[3],
            ]);
            mat.emissive_factor = Vec3::new(
                base.emissive_factor[0] + a.emissive[0],
                base.emissive_factor[1] + a.emissive[1],
                base.emissive_factor[2] + a.emissive[2],
            );
            mat.edge_color = glam::Vec4::from_array([
                base.outline_color[0] + a.outline[0],
                base.outline_color[1] + a.outline[1],
                base.outline_color[2] + a.outline[2],
                base.outline_color[3] + a.outline[3],
            ]);
            if let Some(ref mut mtoon) = mat.mtoon {
                mtoon.shade_color = Some(Vec3::new(
                    base.shade_color[0] + a.shade[0],
                    base.shade_color[1] + a.shade[1],
                    base.shade_color[2] + a.shade[2],
                ));
                mtoon.matcap_factor = Vec3::new(
                    base.matcap_factor[0] + a.matcap[0],
                    base.matcap_factor[1] + a.matcap[1],
                    base.matcap_factor[2] + a.matcap[2],
                );
                mtoon.parametric_rim_color = Vec3::new(
                    base.rim_color[0] + a.rim[0],
                    base.rim_color[1] + a.rim[1],
                    base.rim_color[2] + a.rim[2],
                );
            }
            // UV transform.
            if a.uv_offset != [0.0; 2] || a.uv_scale != [0.0; 2] {
                let ti = mat
                    .base_color_tex_info
                    .get_or_insert_with(|| IrTextureInfo::from_index(0));
                ti.offset = glam::Vec2::new(
                    base.base_uv_offset[0] + a.uv_offset[0],
                    base.base_uv_offset[1] + a.uv_offset[1],
                );
                ti.scale = glam::Vec2::new(
                    base.base_uv_scale[0] + a.uv_scale[0],
                    base.base_uv_scale[1] + a.uv_scale[1],
                );
            }
            Some(build_material_params_for(&mat, mi, flags))
        })
        .collect()
}

impl GpuModel {
    /// Append a newly uploaded GPU texture view pair (sRGB / Unorm with the
    /// same data) to the end and return the assignable texture index (matches
    /// `self.gpu_texture_views.len() - 1`; §D / TODO-7).
    ///
    /// **Caller responsibility**: do not call when an existing `IrTexture` is
    /// reused (those are dedup'd by filename + content). Only call **when a
    /// new `IrTexture` is pushed onto `ir.textures`**, so the index alignment
    /// with the GPU view list is preserved (TODO-1).
    pub fn push_gpu_texture_view(
        &mut self,
        srgb: wgpu::TextureView,
        unorm: wgpu::TextureView,
    ) -> usize {
        let idx = self.gpu_texture_views.len();
        debug_assert_eq!(
            self.gpu_texture_views.len(),
            self.gpu_texture_views_unorm.len(),
            "gpu_texture_views と gpu_texture_views_unorm のインデックスが乖離しています"
        );
        self.gpu_texture_views.push(srgb);
        self.gpu_texture_views_unorm.push(unorm);
        idx
    }

    /// Assign a texture to the given material (updates the DrawCall's bind group).
    /// Builds a per-material sampler from sampler_info.
    pub fn assign_texture_to_material(
        &mut self,
        material_index: usize,
        texture_view: &wgpu::TextureView,
        device: &wgpu::Device,
        texture_bgl: &wgpu::BindGroupLayout,
        sampler_info: &IrSamplerInfo,
    ) {
        let sampler = create_sampler_from_info(device, sampler_info);
        for draw in &mut self.draws {
            if draw.material_index == material_index {
                draw.texture_bind_group = Some(gpu::create_texture_bind_group(
                    device,
                    texture_bgl,
                    texture_view,
                    &sampler,
                ));
            }
        }
    }

    /// Get the bounding box (cached).
    pub fn bbox(&self) -> (Vec3, Vec3) {
        self.cached_bbox
    }

    /// Get the global vertex index -> GPU vertex index map (used by animation).
    pub fn global_to_gpu_map(&self) -> &[u32] {
        &self.global_to_gpu
    }

    /// Get the base vertices (used for normal display, etc.).
    pub fn base_vertices(&self) -> &[Vertex] {
        &self.base_vertices
    }

    /// Get the current vertices (animated if available, otherwise the base).
    pub fn current_vertices(&self) -> &[Vertex] {
        self.animated_vertices
            .as_deref()
            .unwrap_or(&self.base_vertices)
    }

    /// Cache the animated vertices.
    pub fn set_animated_vertices(&mut self, verts: Vec<Vertex>) {
        self.animated_vertices = Some(verts);
    }

    /// Invalidate the morph weight cache (forces recomputation on the next apply_morphs).
    pub fn invalidate_morph_cache(&mut self) {
        self.morph_cache_dirty = true;
    }

    /// Copy base vertices into animated_vertices (buffer reuse, avoids per-frame alloc).
    pub fn reset_animated_to_base(&mut self) {
        match self.animated_vertices {
            Some(ref mut v) => {
                v.clear();
                v.extend_from_slice(&self.base_vertices);
            }
            None => {
                self.animated_vertices = Some(self.base_vertices.clone());
            }
        }
    }

    /// Mutable reference to the animated vertices.
    pub fn animated_vertices_mut(&mut self) -> &mut [Vertex] {
        self.animated_vertices.as_deref_mut().unwrap_or(&mut [])
    }

    /// Clear the animated-vertex cache.
    pub fn clear_animated_vertices(&mut self) {
        self.animated_vertices = None;
    }

    /// Get the raw index buffer data (used by the normal-display filter).
    pub fn base_indices(&self) -> &[u32] {
        &self.base_indices
    }

    /// Sync the IR vertex UVs to the GPU side (`base_vertices` and `vertex_buf`) (v0.5.5).
    ///
    /// Call this after the vertex editor (`UvEditState`) commits a UV edit
    /// (e.g. on mouse-up). Not intended to be called every frame (it
    /// re-uploads the entire vertex_buf). When animated_vertices exists, only
    /// the UV is synced (position / normal / etc. update on the next morph apply).
    /// From Phase 3 A-1 onward, both UV0 (`IrVertex.uv`) and UV1 (`IrMesh.uvs1[vi]`) are reflected.
    pub fn sync_uvs_from_ir(&mut self, ir: &IrModel, queue: &wgpu::Queue) {
        let mut global_offset = 0usize;
        for mesh in &ir.meshes {
            let has_uv1 = mesh.uvs1.len() == mesh.vertices.len();
            for (local_vi, v) in mesh.vertices.iter().enumerate() {
                let global_vi = global_offset + local_vi;
                if let Some(&gpu_vi) = self.global_to_gpu.get(global_vi) {
                    if let Some(base_v) = self.base_vertices.get_mut(gpu_vi as usize) {
                        base_v.uv = v.uv.to_array();
                        if has_uv1 {
                            base_v.uv1 = mesh.uvs1[local_vi];
                        }
                    }
                }
            }
            global_offset += mesh.vertices.len();
        }
        let base = &self.base_vertices;
        if let Some(av) = self.animated_vertices.as_mut() {
            for (i, v) in av.iter_mut().enumerate() {
                if let Some(bv) = base.get(i) {
                    v.uv = bv.uv;
                    v.uv1 = bv.uv1;
                }
            }
        }
        queue.write_buffer(
            &self.vertex_buf,
            0,
            bytemuck::cast_slice(&self.base_vertices),
        );
        self.morph_cache_dirty = true;
    }

    /// Write GPU model normals back into the IrModel (reflects recomputed normals at PMX export time).
    /// The coordinate transform is self-inverse (Z flip / X flip cancels after two applications),
    /// so the same function is used for the inverse transform.
    pub fn write_normals_back(&self, ir: &mut IrModel) {
        let inv_normal_fn: fn(Vec3) -> Vec3 = if self.use_vrm0_coords {
            gltf_normal_to_pmx_v0
        } else {
            gltf_normal_to_pmx
        };

        let mut mesh_offsets = Vec::with_capacity(ir.meshes.len());
        let mut offset = 0usize;
        for mesh in &ir.meshes {
            mesh_offsets.push(offset);
            offset += mesh.vertices.len();
        }

        for (mi, mesh) in ir.meshes.iter_mut().enumerate() {
            let global_offset = mesh_offsets[mi];
            for (local_vi, v) in mesh.vertices_mut().iter_mut().enumerate() {
                let global_vi = global_offset + local_vi;
                if let Some(&gpu_vi) = self.global_to_gpu.get(global_vi) {
                    if let Some(gpu_v) = self.base_vertices.get(gpu_vi as usize) {
                        // GPU normal (PMX coordinates) -> inverse transform back to glTF coordinates.
                        v.normal = inv_normal_fn(Vec3::from(gpu_v.normal));
                    }
                }
            }
        }
    }

    /// Apply morph weights and update the vertex buffer.
    /// Early-out when weights have not changed since the previous call (skips recomputation).
    pub fn apply_morphs(&mut self, weights: &[f32], queue: &wgpu::Queue) {
        // Do nothing when weights are unchanged (forced when the cache is invalidated).
        if !self.morph_cache_dirty
            && self.last_weights.len() == weights.len()
            && self.last_weights == weights
        {
            return;
        }
        self.morph_cache_dirty = false;

        self.morph_work.clear();
        self.morph_work.extend_from_slice(&self.base_vertices);

        let morph_len = self.gpu_morphs.len();
        // Allocate the visited buffer once, then reset with fill(false) after each morph.
        self.morph_visited.resize(morph_len, false);
        for morph_idx in 0..morph_len {
            let w = weights.get(morph_idx).copied().unwrap_or(0.0);
            if w.abs() < 1e-6 {
                continue;
            }
            Self::apply_gpu_morph_recursive(
                &self.gpu_morphs,
                morph_idx,
                w,
                &mut self.morph_work,
                &mut self.morph_visited,
            );
            self.morph_visited.fill(false);
        }

        // Sync the CPU-side current vertices too — swap to avoid alloc.
        let mut swap_buf = self.animated_vertices.take().unwrap_or_default();
        std::mem::swap(&mut self.morph_work, &mut swap_buf);
        self.animated_vertices = Some(swap_buf);

        queue.write_buffer(
            &self.vertex_buf,
            0,
            bytemuck::cast_slice(
                self.animated_vertices
                    .as_ref()
                    .expect("animated_vertices is always set to Some inside apply_morphs"),
            ),
        );

        // Record weights for the next-call comparison.
        self.last_weights.clear();
        self.last_weights.extend_from_slice(weights);
    }

    /// Apply morph weights to an external buffer (for animation; does not upload to the GPU).
    pub fn apply_morphs_to_buf(&mut self, weights: &[f32], vertices: &mut [Vertex]) {
        let morph_len = self.gpu_morphs.len();
        // Allocate the visited buffer once, then reset with fill(false).
        self.morph_visited.resize(morph_len, false);
        for morph_idx in 0..morph_len {
            let w = weights.get(morph_idx).copied().unwrap_or(0.0);
            if w.abs() < 1e-6 {
                continue;
            }
            Self::apply_gpu_morph_recursive(
                &self.gpu_morphs,
                morph_idx,
                w,
                vertices,
                &mut self.morph_visited,
            );
            self.morph_visited.fill(false);
        }
    }

    /// Apply morphs directly to animated_vertices (avoids borrow conflict).
    pub fn apply_morphs_to_animated(&mut self, weights: &[f32]) {
        if let Some(ref mut verts) = self.animated_vertices {
            let morph_len = self.gpu_morphs.len();
            // Allocate the visited buffer once, then reset with fill(false).
            self.morph_visited.resize(morph_len, false);
            for morph_idx in 0..morph_len {
                let w = weights.get(morph_idx).copied().unwrap_or(0.0);
                if w.abs() < 1e-6 {
                    continue;
                }
                Self::apply_gpu_morph_recursive(
                    &self.gpu_morphs,
                    morph_idx,
                    w,
                    verts,
                    &mut self.morph_visited,
                );
                self.morph_visited.fill(false);
            }
        }
    }

    fn apply_gpu_morph_recursive(
        gpu_morphs: &[GpuMorphEntry],
        morph_idx: usize,
        weight: f32,
        vertices: &mut [Vertex],
        visited: &mut [bool],
    ) {
        if visited[morph_idx] {
            return; // Cycle detected — skip.
        }
        match &gpu_morphs[morph_idx] {
            GpuMorphEntry::Vertex(voffs) => {
                for &(gpu_vi, pos_d, nrm_d, tan_d) in voffs {
                    let vi = gpu_vi as usize;
                    if vi < vertices.len() {
                        vertices[vi].position[0] += pos_d[0] * weight;
                        vertices[vi].position[1] += pos_d[1] * weight;
                        vertices[vi].position[2] += pos_d[2] * weight;
                        vertices[vi].normal[0] += nrm_d[0] * weight;
                        vertices[vi].normal[1] += nrm_d[1] * weight;
                        vertices[vi].normal[2] += nrm_d[2] * weight;
                        vertices[vi].tangent[0] += tan_d[0] * weight;
                        vertices[vi].tangent[1] += tan_d[1] * weight;
                        vertices[vi].tangent[2] += tan_d[2] * weight;
                    }
                }
            }
            GpuMorphEntry::Group(goffs) => {
                visited[morph_idx] = true;
                for &(sub_idx, sub_weight) in goffs {
                    let effective = weight * sub_weight;
                    if effective.abs() < 1e-6 {
                        continue;
                    }
                    if sub_idx >= gpu_morphs.len() {
                        log::warn!(
                            "Group morph[{}]: sub-index {} out of range (len={})",
                            morph_idx,
                            sub_idx,
                            gpu_morphs.len()
                        );
                        continue;
                    }
                    Self::apply_gpu_morph_recursive(
                        gpu_morphs, sub_idx, effective, vertices, visited,
                    );
                }
                visited[morph_idx] = false;
            }
            GpuMorphEntry::Material { .. } => {
                // Material morphs do not affect vertices — handled in accumulate_expression_materials.
            }
            GpuMorphEntry::Uv { channel, offsets } => {
                // Phase 3 A-2: add (du, dv) * weight to UV0 / UV1.
                // The GPU Vertex struct has no UV2..UV4, so offsets is empty for channel >= 2.
                match channel {
                    0 => {
                        for &(gpu_vi, d) in offsets {
                            let vi = gpu_vi as usize;
                            if vi < vertices.len() {
                                vertices[vi].uv[0] += d[0] * weight;
                                vertices[vi].uv[1] += d[1] * weight;
                            }
                        }
                    }
                    1 => {
                        for &(gpu_vi, d) in offsets {
                            let vi = gpu_vi as usize;
                            if vi < vertices.len() {
                                vertices[vi].uv1[0] += d[0] * weight;
                                vertices[vi].uv1[1] += d[1] * weight;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Build GPU buffers from IrModel + GlbData.
pub fn build_gpu_model(
    ir: &IrModel,
    images: &[gltf::image::Data],
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    flags: &MaterialBuildFlags,
) -> Result<GpuModel> {
    let gpu_textures = super::texture::upload_textures(ir, images, device, queue)?;
    build_gpu_model_inner(ir, gpu_textures, device, queue, flags)
}

/// Convert IrMinFilter into the wgpu (min_filter, mipmap_filter) pair.
fn ir_min_filter_to_wgpu(mode: IrMinFilter) -> (wgpu::FilterMode, wgpu::FilterMode) {
    match mode {
        IrMinFilter::Nearest => (wgpu::FilterMode::Nearest, wgpu::FilterMode::Nearest),
        IrMinFilter::Linear => (wgpu::FilterMode::Linear, wgpu::FilterMode::Nearest),
        IrMinFilter::NearestMipmapNearest => (wgpu::FilterMode::Nearest, wgpu::FilterMode::Nearest),
        IrMinFilter::LinearMipmapNearest => (wgpu::FilterMode::Linear, wgpu::FilterMode::Nearest),
        IrMinFilter::NearestMipmapLinear => (wgpu::FilterMode::Nearest, wgpu::FilterMode::Linear),
        IrMinFilter::LinearMipmapLinear => (wgpu::FilterMode::Linear, wgpu::FilterMode::Linear),
    }
}

/// Build a wgpu::Sampler from IrSamplerInfo (one-shot use, e.g. for previews).
pub fn create_sampler_from_info(device: &wgpu::Device, info: &IrSamplerInfo) -> wgpu::Sampler {
    let (min_filter, mipmap_filter) = ir_min_filter_to_wgpu(info.min_filter);
    let mag_filter = match info.mag_filter {
        IrMagFilter::Nearest => wgpu::FilterMode::Nearest,
        IrMagFilter::Linear => wgpu::FilterMode::Linear,
    };
    // anisotropy_clamp > 1 is only effective when every filter is Linear (wgpu / WebGPU spec).
    let all_linear = mag_filter == wgpu::FilterMode::Linear
        && min_filter == wgpu::FilterMode::Linear
        && mipmap_filter == wgpu::FilterMode::Linear;
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("preview_sampler"),
        mag_filter,
        min_filter,
        mipmap_filter,
        address_mode_u: match info.wrap_u {
            IrWrapMode::Repeat => wgpu::AddressMode::Repeat,
            IrWrapMode::ClampToEdge => wgpu::AddressMode::ClampToEdge,
            IrWrapMode::MirroredRepeat => wgpu::AddressMode::MirrorRepeat,
        },
        address_mode_v: match info.wrap_v {
            IrWrapMode::Repeat => wgpu::AddressMode::Repeat,
            IrWrapMode::ClampToEdge => wgpu::AddressMode::ClampToEdge,
            IrWrapMode::MirroredRepeat => wgpu::AddressMode::MirrorRepeat,
        },
        anisotropy_clamp: if all_linear { 16 } else { 1 },
        ..Default::default()
    })
}

/// Get a wgpu::Sampler matching IrSamplerInfo from the cache (creates one on miss).
fn ensure_sampler<'a>(
    cache: &'a mut HashMap<IrSamplerInfo, wgpu::Sampler>,
    device: &wgpu::Device,
    info: &IrSamplerInfo,
) -> &'a wgpu::Sampler {
    cache.entry(*info).or_insert_with(|| {
        let (min_filter, mipmap_filter) = ir_min_filter_to_wgpu(info.min_filter);
        let mag_filter = match info.mag_filter {
            IrMagFilter::Nearest => wgpu::FilterMode::Nearest,
            IrMagFilter::Linear => wgpu::FilterMode::Linear,
        };
        let all_linear = mag_filter == wgpu::FilterMode::Linear
            && min_filter == wgpu::FilterMode::Linear
            && mipmap_filter == wgpu::FilterMode::Linear;
        device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ir_sampler"),
            mag_filter,
            min_filter,
            mipmap_filter,
            address_mode_u: match info.wrap_u {
                IrWrapMode::Repeat => wgpu::AddressMode::Repeat,
                IrWrapMode::ClampToEdge => wgpu::AddressMode::ClampToEdge,
                IrWrapMode::MirroredRepeat => wgpu::AddressMode::MirrorRepeat,
            },
            address_mode_v: match info.wrap_v {
                IrWrapMode::Repeat => wgpu::AddressMode::Repeat,
                IrWrapMode::ClampToEdge => wgpu::AddressMode::ClampToEdge,
                IrWrapMode::MirroredRepeat => wgpu::AddressMode::MirrorRepeat,
            },
            anisotropy_clamp: if all_linear { 16 } else { 1 },
            ..Default::default()
        })
    })
}

/// Build GPU buffers from IrModel only (used by FBX).
pub fn build_gpu_model_from_ir(
    ir: &IrModel,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    flags: &MaterialBuildFlags,
) -> Result<GpuModel> {
    let gpu_textures = super::texture::upload_textures_from_ir(ir, device, queue)?;
    build_gpu_model_inner(ir, gpu_textures, device, queue, flags)
}

/// MToon auxiliary texture reference info (CPU phase).
pub(crate) struct AuxTexRef {
    pub tex_index: Option<usize>,
    pub sampler: IrSamplerInfo,
    /// true = use the sRGB view, false = use the linear / Unorm view.
    pub use_srgb: bool,
}

/// MToon auxiliary texture references (8 slots).
pub(crate) struct AuxTexRefs {
    pub matcap: AuxTexRef,
    pub shade: AuxTexRef,
    pub shift: AuxTexRef,
    pub rim: AuxTexRef,
    pub uv_mask: AuxTexRef,
    pub outline_width: AuxTexRef,
    pub emissive: AuxTexRef,
    pub normal: AuxTexRef,
}

/// Build a single material's `MaterialParams` as a pure function (§C).
///
/// This used to be inlined inside `cpu_prep_model`, but in v0.5.0 the
/// `rebuild_material_bind_groups` path of the material edit drawer needed the
/// same logic, so it was extracted as a pure function. Performs only CPU
/// computation; never touches the GPU API.
pub(crate) fn build_material_params_for(
    mat: &IrMaterial,
    mat_idx: usize,
    flags: &MaterialBuildFlags,
) -> gpu::MaterialParams {
    let mp = mat.mtoon();
    let outline_mode = match mp.outline_width_mode {
        OutlineWidthMode::None => 0.0,
        OutlineWidthMode::WorldCoordinates => 1.0,
        OutlineWidthMode::ScreenCoordinates => 2.0,
    };
    gpu::MaterialParams {
        diffuse: mat.diffuse.to_array(),
        shade_color: mp.shade_color.unwrap_or(Vec3::ZERO).to_array(),
        // review_007 [P2] fix: unify the rendering side too on `shader_family` as the primary axis.
        // With `mat.is_mtoon()` (= `mtoon.is_some()`), the moment a non-MToon
        // material's MToon-related field was edited, the preview "became MToon"
        // and disagreed with the export side (which is `shader_family`-driven).
        // Looking at `shader_family` aligns the checkbox ON / OFF with the
        // preview and the export.
        is_mtoon: matches!(
            mat.shader_family,
            ShaderFamily::Mtoon
                | ShaderFamily::Uts2
                | ShaderFamily::LilToon
                | ShaderFamily::Poiyomi
        ),
        shading_toony: mp.shading_toony_factor,
        shading_shift: mp.shading_shift_factor,
        outline_width: mp.outline_width_factor,
        outline_mode,
        outline_color: mat.edge_color.to_array(),
        outline_lighting_mix: mp.outline_lighting_mix,
        rim_color: mp.parametric_rim_color.to_array(),
        rim_fresnel_power: mp.parametric_rim_fresnel_power,
        rim_lift: mp.parametric_rim_lift,
        rim_lighting_mix: mp.rim_lighting_mix,
        has_matcap: mp.matcap_texture.is_some(),
        matcap_factor: mp.matcap_factor.to_array(),
        has_shade_multiply_tex: mp.shade_texture.is_some(),
        has_shading_shift_tex: mp.shading_shift_texture.is_some(),
        shading_shift_tex_scale: mp.shading_shift_texture_scale,
        has_rim_multiply_tex: mp.rim_multiply_texture.is_some(),
        uv_anim_scroll_x: mp.uv_animation_scroll_x_speed,
        uv_anim_scroll_y: mp.uv_animation_scroll_y_speed,
        uv_anim_rotation: mp.uv_animation_rotation_speed,
        has_uv_anim_mask: mp.uv_animation_mask_texture.is_some(),
        // alphaMode encoding: OPAQUE = -1.0, MASK = cutoff (>= 0.0), BLEND = -0.5.
        alpha_cutoff: match mat.alpha_mode {
            AlphaMode::Opaque => -1.0,
            AlphaMode::Mask => mat.alpha_cutoff, // 0.0 is also a legal value
            _ => -0.5,                           // Blend / BlendZWrite
        },
        base_uv: gpu::pack_uv_params(mat.base_color_tex_info.as_ref()),
        shade_uv: gpu::pack_uv_params(mp.shade_texture.as_ref()),
        shift_uv: gpu::pack_uv_params(mp.shading_shift_texture.as_ref()),
        rim_uv: gpu::pack_uv_params(mp.rim_multiply_texture.as_ref()),
        outline_uv: gpu::pack_uv_params(mp.outline_width_texture.as_ref()),
        uv_mask_uv: gpu::pack_uv_params(mp.uv_animation_mask_texture.as_ref()),
        emissive_factor: if flags.emissive.get(mat_idx).copied().unwrap_or(true) {
            mat.emissive_factor.to_array()
        } else {
            [0.0; 3]
        },
        has_emissive_tex: mat.emissive_texture.is_some()
            && flags.emissive.get(mat_idx).copied().unwrap_or(true),
        emissive_uv: gpu::pack_uv_params(mat.emissive_texture.as_ref()),
        has_normal_tex: mat.normal_texture.is_some()
            && flags.normal_map.get(mat_idx).copied().unwrap_or(true),
        normal_scale: mat.normal_texture_scale,
        normal_uv: gpu::pack_uv_params(mat.normal_texture.as_ref()),
        gi_equalization_factor: mp.gi_equalization_factor,
        outline_width_channel: mp.outline_width_tex_channel.to_f32(),
        uv_anim_mask_channel: mp.uv_anim_mask_tex_channel.to_f32(),
        matcap_uv: gpu::pack_uv_params(mp.matcap_texture.as_ref()),
    }
}

/// Specifically for the material-edit rebuild path: build the MToon auxiliary
/// bind group from `AuxTexRefs` and the split view lists of `GpuModel` (§C).
///
/// The equivalent logic in `gpu_finalize_model` runs over a
/// `gpu_textures_dual: &[(srgb, unorm)]` tuple list with locally-built default
/// views; here we target the path that **updates an already-built `GpuModel`**,
/// so we receive the split `gpu_texture_views` / `gpu_texture_views_unorm`
/// vectors and the `DefaultViews` collection directly.
///
/// The samplers are created via `create_sampler_from_info` every call (rebuild
/// only fires during edits, so caching has little value — a deliberate
/// minimal-scope cut for Step 1-3b).
pub(crate) fn rebuild_mtoon_aux_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    refs: &AuxTexRefs,
    gpu_texture_views: &[wgpu::TextureView],
    gpu_texture_views_unorm: &[wgpu::TextureView],
    default_views: &gpu::DefaultViews,
) -> wgpu::BindGroup {
    let resolve = |r: &AuxTexRef| -> Option<&wgpu::TextureView> {
        r.tex_index.and_then(|idx| {
            if r.use_srgb {
                gpu_texture_views.get(idx)
            } else {
                gpu_texture_views_unorm.get(idx)
            }
        })
    };

    let matcap_view = resolve(&refs.matcap).unwrap_or(&default_views.black_srgb);
    let shade_view = resolve(&refs.shade).unwrap_or(&default_views.white_srgb);
    let shift_view = resolve(&refs.shift).unwrap_or(&default_views.white_srgb);
    let rim_view = resolve(&refs.rim).unwrap_or(&default_views.white_srgb);
    let uv_mask_view = resolve(&refs.uv_mask).unwrap_or(&default_views.white_srgb);
    let outline_view = resolve(&refs.outline_width).unwrap_or(&default_views.white_srgb);
    let emissive_view = resolve(&refs.emissive).unwrap_or(&default_views.white_srgb);
    let normal_view = resolve(&refs.normal).unwrap_or(&default_views.flat_normal_unorm);

    let matcap_sampler = create_sampler_from_info(device, &refs.matcap.sampler);
    let shade_sampler = create_sampler_from_info(device, &refs.shade.sampler);
    let shift_sampler = create_sampler_from_info(device, &refs.shift.sampler);
    let rim_sampler = create_sampler_from_info(device, &refs.rim.sampler);
    let uv_mask_sampler = create_sampler_from_info(device, &refs.uv_mask.sampler);
    let outline_sampler = create_sampler_from_info(device, &refs.outline_width.sampler);
    let emissive_sampler = create_sampler_from_info(device, &refs.emissive.sampler);
    let normal_sampler = create_sampler_from_info(device, &refs.normal.sampler);

    gpu::create_mtoon_aux_bind_group(
        device,
        layout,
        gpu::AuxTexEntry {
            view: matcap_view,
            sampler: &matcap_sampler,
        },
        gpu::AuxTexEntry {
            view: shade_view,
            sampler: &shade_sampler,
        },
        gpu::AuxTexEntry {
            view: shift_view,
            sampler: &shift_sampler,
        },
        gpu::AuxTexEntry {
            view: rim_view,
            sampler: &rim_sampler,
        },
        gpu::AuxTexEntry {
            view: uv_mask_view,
            sampler: &uv_mask_sampler,
        },
        gpu::AuxTexEntry {
            view: outline_view,
            sampler: &outline_sampler,
        },
        gpu::AuxTexEntry {
            view: emissive_view,
            sampler: &emissive_sampler,
        },
        gpu::AuxTexEntry {
            view: normal_view,
            sampler: &normal_sampler,
        },
    )
}

/// Build a material's auxiliary texture references as a pure function (§C).
///
/// `None` means "no aux bind group needed" — the caller falls back to the
/// default bind group. `Some(_)` means the caller will build the bind group
/// via `create_mtoon_aux_bind_group`.
pub(crate) fn build_aux_refs_for(mat: &IrMaterial) -> Option<AuxTexRefs> {
    // review_007 [P2] fix: unify the rendering side too on `shader_family` as the primary axis.
    // emissiveTexture / normalTexture are needed even for non-MToon, so they stay on independent conditions.
    let is_mtoon_like = matches!(
        mat.shader_family,
        ShaderFamily::Mtoon | ShaderFamily::Uts2 | ShaderFamily::LilToon | ShaderFamily::Poiyomi
    );
    let needs_aux = is_mtoon_like || mat.emissive_texture.is_some() || mat.normal_texture.is_some();
    if !needs_aux {
        return None;
    }
    let mp = mat.mtoon();
    let sampler_of =
        |ti: Option<&IrTextureInfo>| -> IrSamplerInfo { ti.map(|t| t.sampler).unwrap_or_default() };
    Some(AuxTexRefs {
        matcap: AuxTexRef {
            tex_index: mp.matcap_texture.as_ref().map(|t| t.index),
            sampler: sampler_of(mp.matcap_texture.as_ref()),
            use_srgb: true,
        },
        shade: AuxTexRef {
            tex_index: mp.shade_texture.as_ref().map(|t| t.index),
            sampler: sampler_of(mp.shade_texture.as_ref()),
            use_srgb: true,
        },
        shift: AuxTexRef {
            tex_index: mp.shading_shift_texture.as_ref().map(|t| t.index),
            sampler: sampler_of(mp.shading_shift_texture.as_ref()),
            use_srgb: false,
        },
        rim: AuxTexRef {
            tex_index: mp.rim_multiply_texture.as_ref().map(|t| t.index),
            sampler: sampler_of(mp.rim_multiply_texture.as_ref()),
            use_srgb: true,
        },
        uv_mask: AuxTexRef {
            tex_index: mp.uv_animation_mask_texture.as_ref().map(|t| t.index),
            sampler: sampler_of(mp.uv_animation_mask_texture.as_ref()),
            use_srgb: false,
        },
        outline_width: AuxTexRef {
            tex_index: mp.outline_width_texture.as_ref().map(|t| t.index),
            sampler: sampler_of(mp.outline_width_texture.as_ref()),
            use_srgb: false,
        },
        emissive: AuxTexRef {
            tex_index: mat.emissive_texture.as_ref().map(|t| t.index),
            sampler: sampler_of(mat.emissive_texture.as_ref()),
            use_srgb: true,
        },
        normal: AuxTexRef {
            tex_index: mat.normal_texture.as_ref().map(|t| t.index),
            sampler: sampler_of(mat.normal_texture.as_ref()),
            use_srgb: false,
        },
    })
}

/// Per-material draw plan (CPU-side data before the GPU bind group is created).
#[allow(dead_code)]
pub(crate) struct CpuDrawPlan {
    pub index_offset: u32,
    pub index_count: u32,
    pub cull_mode: CullMode,
    pub is_alpha: bool,
    pub render_queue: RenderQueue,
    pub render_queue_offset: i32,
    pub alpha_cutoff: f32,
    pub material_index: usize,
    pub render_style: RenderStyle,
    pub has_edge: bool,
    pub has_outline: bool,
    pub center: glam::Vec3,
    // Metadata used to build the bind group.
    pub base_tex_index: Option<usize>,
    pub base_sampler: IrSamplerInfo,
    pub material_params: gpu::MaterialParams,
    pub needs_aux: bool,
    pub aux_refs: Option<AuxTexRefs>,
}

/// Output of the CPU preprocessing phase (no GPU API needed).
pub(crate) struct CpuPrepResult {
    pub all_vertices: Vec<Vertex>,
    pub all_indices: Vec<u32>,
    pub global_to_gpu: Vec<u32>,
    pub draw_plans: Vec<CpuDrawPlan>,
    pub has_alpha: bool,
    pub use_vrm0_coords: bool,
    pub cached_bbox: (Vec3, Vec3),
    pub base_vertices: Vec<Vertex>,
    pub gpu_morphs: Vec<GpuMorphEntry>,
    pub edge_scales: Option<Vec<f32>>,
    pub material_base_values: Vec<MaterialBaseValues>,
}

/// CPU preprocessing phase: vertex transform / normal smoothing / morph pre-compute (no GPU API calls).
pub(crate) fn cpu_prep_model(ir: &IrModel, flags: &MaterialBuildFlags) -> Result<CpuPrepResult> {
    let smooth_per_mat = &flags.smooth;
    let clear_per_mat = &flags.clear;
    // The normal_map / emissive flags are read directly from `flags` inside build_material_params_for.
    let pos_fn = if ir.source_format.is_vrm0() {
        gltf_pos_to_pmx_v0
    } else {
        gltf_pos_to_pmx
    };
    let normal_fn = if ir.source_format.is_vrm0() {
        gltf_normal_to_pmx_v0
    } else {
        gltf_normal_to_pmx
    };

    let total_verts: usize = ir.meshes.iter().map(|m| m.vertices.len()).sum();
    let total_indices: usize = ir.meshes.iter().map(|m| m.indices.len()).sum();
    let mut all_vertices: Vec<Vertex> = Vec::with_capacity(total_verts);
    let mut all_indices: Vec<u32> = Vec::with_capacity(total_indices);
    let mut draw_plans: Vec<CpuDrawPlan> = Vec::with_capacity(ir.materials.len());
    let mut has_alpha = false;

    // Global vertex index -> GPU vertex index map.
    let total_global_verts: usize = ir.meshes.iter().map(|m| m.vertices.len()).sum();
    let mut global_to_gpu = vec![0u32; total_global_verts];

    // Vertex welding map: position + UV key -> GPU vertex index.
    let mut vertex_dedup: HashMap<PosUvKey, u32> = HashMap::with_capacity(total_verts);
    // Normal accumulator (for averaging).
    let mut normal_accum: Vec<([f32; 3], u32)> = Vec::with_capacity(total_verts);

    let default_sampler_info = IrSamplerInfo::default();

    // Global vertex offset of each mesh (in source mesh order).
    let mut mesh_global_offsets = Vec::with_capacity(ir.meshes.len());
    let mut offset = 0usize;
    for mesh in &ir.meshes {
        mesh_global_offsets.push(offset);
        offset += mesh.vertices.len();
    }

    // Group meshes by material.
    let mat_count = ir.materials.len();
    let mut mat_meshes: Vec<Vec<usize>> = vec![Vec::new(); mat_count];
    for (mi, mesh) in ir.meshes.iter().enumerate() {
        if mesh.material_index < mat_count {
            mat_meshes[mesh.material_index].push(mi);
        }
    }

    for (mat_idx, mesh_indices) in mat_meshes.iter().enumerate() {
        if mesh_indices.is_empty() {
            continue;
        }

        let mat = &ir.materials[mat_idx];
        let index_offset = all_indices.len() as u32;

        // Per-material smoothing flag (compatible with normal maps: smoothing the TBN base normal improves quality).
        let mat_smooth = smooth_per_mat.get(mat_idx).copied().unwrap_or(false);

        // Reset vertex_dedup per material (vertices are not shared across different materials).
        vertex_dedup.clear();

        for &mi in mesh_indices {
            let mesh = &ir.meshes[mi];
            let global_offset = mesh_global_offsets[mi];

            // Vertex transform + map building.
            let has_uv1 = !mesh.uvs1.is_empty();
            for (local_vi, v) in mesh.vertices.iter().enumerate() {
                let pos = pos_fn(v.position);
                let normal = normal_fn(v.normal);
                // UV1: use it if present, otherwise zero (UniVRM MeshData.cs convention).
                let uv1 = if has_uv1 {
                    mesh.uvs1[local_vi]
                } else {
                    [0.0, 0.0]
                };

                // Under a mirror transform (det = -1), cross(M*N, M*T) = -M*cross(N, T),
                // so tangent.w must be flipped to keep the bitangent direction.
                let tangent = normal_fn(v.tangent.truncate())
                    .normalize_or_zero()
                    .extend(-v.tangent.w);

                let gpu_vi = if mat_smooth {
                    // Merge by position + UV; accumulate normals to be averaged later.
                    let key = PosUvKey::new(pos.to_array(), v.uv.to_array(), uv1);
                    *vertex_dedup.entry(key).or_insert_with(|| {
                        let idx = all_vertices.len() as u32;
                        all_vertices.push(Vertex {
                            position: pos.to_array(),
                            normal: [0.0; 3],
                            uv: v.uv.to_array(),
                            uv1,
                            tangent: tangent.to_array(),
                        });
                        normal_accum.push(([0.0; 3], 0));
                        idx
                    })
                } else {
                    let idx = all_vertices.len() as u32;
                    all_vertices.push(Vertex {
                        position: pos.to_array(),
                        normal: normal.to_array(),
                        uv: v.uv.to_array(),
                        uv1,
                        tangent: tangent.to_array(),
                    });
                    // Keep normal_accum in sync with all_vertices (count = 0 excludes from averaging).
                    normal_accum.push(([0.0; 3], 0));
                    idx
                };

                if mat_smooth {
                    let acc = &mut normal_accum[gpu_vi as usize];
                    acc.0[0] += normal.x;
                    acc.0[1] += normal.y;
                    acc.0[2] += normal.z;
                    acc.1 += 1;
                }
                global_to_gpu[global_offset + local_vi] = gpu_vi;
            }

            // Indices.
            let mut indices: Vec<u32> = if mat_smooth {
                mesh.indices
                    .iter()
                    .map(|&i| global_to_gpu[global_offset + i as usize])
                    .collect()
            } else {
                let base = global_to_gpu[global_offset];
                mesh.indices.iter().map(|&i| i + base).collect()
            };
            flip_face_winding(&mut indices);
            all_indices.extend_from_slice(&indices);
        }

        let index_count = all_indices.len() as u32 - index_offset;

        // Sampler info of the base color texture.
        let base_sampler = mat
            .base_color_tex_info
            .as_ref()
            .map(|ti| ti.sampler)
            .unwrap_or(default_sampler_info);

        // Material params (pure CPU computation — no GPU API needed).
        // §C: extracted as the pub(crate) functions build_material_params_for / build_aux_refs_for.
        // The same logic is reused from the material-edit drawer's rebuild_material_bind_groups path.
        let diffuse = mat.diffuse;
        let mp = mat.mtoon();
        let material_params = build_material_params_for(mat, mat_idx, flags);
        let aux_refs = build_aux_refs_for(mat);
        let needs_aux = aux_refs.is_some();

        // Decide the render queue from alphaMode.
        let render_queue = match mat.alpha_mode {
            AlphaMode::Opaque => RenderQueue::Opaque,
            AlphaMode::Mask => RenderQueue::Mask,
            AlphaMode::BlendWithZWrite => RenderQueue::BlendZWrite,
            AlphaMode::Blend => RenderQueue::Blend,
        };
        // is_alpha for backward compatibility (BLEND-family or diffuse.w < 1.0).
        let is_alpha = matches!(render_queue, RenderQueue::Blend | RenderQueue::BlendZWrite)
            || diffuse.w < 1.0;

        if is_alpha {
            has_alpha = true;
        }

        let render_style = if mat.source_format.is_pmx_pmd() {
            RenderStyle::Mmd
        } else {
            RenderStyle::Standard
        };
        let has_edge = mat.edge_size > 0.0;
        let has_outline =
            mp.outline_width_mode != OutlineWidthMode::None && mp.outline_width_factor > 0.0;

        // Compute the centroid of the draw mesh (for translucent distance sorting).
        let center = if index_count > 0 {
            let mut sum = glam::Vec3::ZERO;
            let start = index_offset as usize;
            let end = start + index_count as usize;
            for &idx in &all_indices[start..end] {
                let p = all_vertices[idx as usize].position;
                sum += glam::Vec3::from(p);
            }
            sum / index_count as f32
        } else {
            glam::Vec3::ZERO
        };

        draw_plans.push(CpuDrawPlan {
            index_offset,
            index_count,
            cull_mode: mat.cull_mode,
            is_alpha,
            render_queue,
            render_queue_offset: mp.render_queue_offset,
            alpha_cutoff: mat.alpha_cutoff,
            material_index: mat_idx,
            render_style,
            has_edge,
            has_outline,
            center,
            base_tex_index: mat.texture_index,
            base_sampler,
            material_params,
            needs_aux,
            aux_refs,
        });
    }

    let any_smooth = smooth_per_mat.iter().any(|&s| s);
    let any_clear = clear_per_mat.iter().any(|&c| c);

    // Average and normalize the accumulated normals (only the vertices of smooth-enabled materials; count > 0 filters automatically).
    if any_smooth {
        for (vi, v) in all_vertices.iter_mut().enumerate() {
            if let Some(&(sum, count)) = normal_accum.get(vi) {
                if count > 0 {
                    let n = Vec3::new(sum[0], sum[1], sum[2]).normalize_or_zero();
                    v.normal = n.to_array();
                }
            }
        }
    }

    // Custom-normal clear: recompute normals from geometry only for the vertices of the targeted materials.
    if any_clear {
        recalculate_normals_selective(&mut all_vertices, &all_indices, &draw_plans, clear_per_mat);
    }

    // Re-orthogonalize tangents after normal recomputation (Gram-Schmidt).
    // When smooth / clear changes the normal, the TBN matrix becomes inconsistent.
    if any_smooth || any_clear {
        reorthogonalize_tangents(&mut all_vertices);
    }

    // Pre-compute GPU-space morph data (deduplicated + coordinate-converted).
    let gpu_morphs: Vec<GpuMorphEntry> = ir
        .morphs
        .iter()
        .map(|morph| match &morph.kind {
            IrMorphKind::Vertex {
                ref positions,
                ref normals,
                ref tangents,
            } => {
                let mut pos_map: HashMap<usize, Vec3> = HashMap::new();
                for &(vi, off) in positions {
                    *pos_map.entry(vi).or_insert(Vec3::ZERO) += off;
                }
                let mut nrm_map: HashMap<usize, Vec3> = HashMap::new();
                for &(vi, off) in normals {
                    *nrm_map.entry(vi).or_insert(Vec3::ZERO) += off;
                }
                let mut tan_map: HashMap<usize, Vec3> = HashMap::new();
                for &(vi, off) in tangents {
                    *tan_map.entry(vi).or_insert(Vec3::ZERO) += off;
                }
                // Collect affected vertices as the union of positions / normals / tangents.
                let affected: std::collections::BTreeSet<usize> = positions
                    .iter()
                    .map(|(vi, _)| *vi)
                    .chain(normals.iter().map(|(vi, _)| *vi))
                    .chain(tangents.iter().map(|(vi, _)| *vi))
                    .collect();
                let mut seen = HashSet::with_capacity(affected.len());
                let mut deduped = Vec::with_capacity(affected.len());
                for global_vi in &affected {
                    if let Some(&gpu_vi) = global_to_gpu.get(*global_vi) {
                        if seen.insert(gpu_vi) {
                            let p = pos_map
                                .get(global_vi)
                                .map(|&v| pos_fn(v))
                                .unwrap_or(Vec3::ZERO);
                            let n = nrm_map
                                .get(global_vi)
                                .map(|&v| normal_fn(v))
                                .unwrap_or(Vec3::ZERO);
                            let t = tan_map
                                .get(global_vi)
                                .map(|&v| normal_fn(v))
                                .unwrap_or(Vec3::ZERO);
                            deduped.push((gpu_vi, p.to_array(), n.to_array(), t.to_array()));
                        }
                    }
                }
                GpuMorphEntry::Vertex(deduped)
            }
            IrMorphKind::Group(goffs) => GpuMorphEntry::Group(goffs.clone()),
            IrMorphKind::Material {
                color_binds,
                uv_binds,
            } => GpuMorphEntry::Material {
                color_binds: color_binds.clone(),
                uv_binds: uv_binds.clone(),
            },
            IrMorphKind::Uv { channel, offsets } => {
                // Phase 3 A-2: IR global vertex index -> GPU vertex index.
                // Multiple IR vertices may dedup to the same GPU vertex, so for the same gpu_vi
                // we adopt only the first offset we see (split vertices on PMX -> IR share the
                // same base UV, so feeding the same offset to both is mostly fine, but we still
                // avoid the double accumulation).
                let mut seen: HashSet<u32> = HashSet::new();
                let mut deduped: Vec<(u32, [f32; 2])> = Vec::with_capacity(offsets.len());
                // channel >= 2 is UV2..UV4 (no GPU storage). Early-return as an empty vec.
                if *channel < 2 {
                    for &(global_vi, off) in offsets {
                        if let Some(&gpu_vi) = global_to_gpu.get(global_vi) {
                            if seen.insert(gpu_vi) {
                                deduped.push((gpu_vi, [off[0], off[1]]));
                            }
                        }
                    }
                }
                GpuMorphEntry::Uv {
                    channel: *channel,
                    offsets: deduped,
                }
            }
        })
        .collect();

    // Save base vertices + compute the bbox.
    let base_vertices = all_vertices.clone();
    let cached_bbox = {
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for v in &base_vertices {
            let p = Vec3::from(v.position);
            min = min.min(p);
            max = max.max(p);
        }
        (min, max)
    };

    // Compute edge scales (MMD-edge specific, CPU only).
    let has_mmd = draw_plans
        .iter()
        .any(|d| d.render_style == RenderStyle::Mmd);
    let edge_scales = if has_mmd {
        let mut scales = vec![1.0f32; all_vertices.len()];
        let mut global_vi = 0usize;
        for mesh in &ir.meshes {
            for v in mesh.vertices.iter() {
                if let Some(&gpu_vi) = global_to_gpu.get(global_vi) {
                    scales[gpu_vi as usize] = scales[gpu_vi as usize].min(v.edge_scale);
                }
                global_vi += 1;
            }
        }
        Some(scales)
    } else {
        None
    };

    // Capture base values for Expression material binds.
    let material_base_values: Vec<MaterialBaseValues> = ir
        .materials
        .iter()
        .map(MaterialBaseValues::from_ir)
        .collect();

    Ok(CpuPrepResult {
        all_vertices,
        all_indices,
        global_to_gpu,
        draw_plans,
        has_alpha,
        use_vrm0_coords: ir.source_format.is_vrm0(),
        cached_bbox,
        base_vertices,
        gpu_morphs,
        edge_scales,
        material_base_values,
    })
}

/// GPU finalize phase: build GPU resources from the CPU pre-compute result.
pub(crate) fn gpu_finalize_model(
    prep: CpuPrepResult,
    gpu_textures_dual: Vec<(wgpu::TextureView, wgpu::TextureView)>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<GpuModel> {
    let texture_bgl = gpu::create_texture_bind_group_layout(device);
    let material_bgl = gpu::create_material_bind_group_layout(device);
    let mtoon_aux_bgl = gpu::create_mtoon_aux_bind_group_layout_pub(device);

    // Sampler cache: IrSamplerInfo -> wgpu::Sampler (avoids duplicate creation).
    let mut sampler_cache: HashMap<IrSamplerInfo, wgpu::Sampler> = HashMap::new();
    let default_sampler_info = IrSamplerInfo::default();
    ensure_sampler(&mut sampler_cache, device, &default_sampler_info);

    // Default texture views (for the MToon auxiliary bind group).
    let default_white_view = gpu::create_white_texture_view_srgb(device, queue);
    let default_black_view = gpu::create_black_texture_view_pub(device, queue);
    let default_flat_normal_view = gpu::create_flat_normal_texture_view(device, queue);

    let mut draws: Vec<DrawCall> = Vec::with_capacity(prep.draw_plans.len());

    for plan in &prep.draw_plans {
        // Base color texture bind group (uses the sRGB view).
        ensure_sampler(&mut sampler_cache, device, &plan.base_sampler);
        let tex_bg = plan.base_tex_index.and_then(|ti| {
            gpu_textures_dual.get(ti).map(|(srgb_view, _)| {
                let sampler = sampler_cache
                    .get(&plan.base_sampler)
                    .expect("already registered by ensure_sampler");
                gpu::create_texture_bind_group(device, &texture_bgl, srgb_view, sampler)
            })
        });

        // Material bind group + buffer (COPY_DST allows partial updates).
        let (mat_buf, mat_bg) = gpu::create_material_buffer_and_bind_group(
            device,
            &material_bgl,
            &plan.material_params,
        );

        // MToon auxiliary texture bind group (group 3).
        let mtoon_aux_bg = if let Some(ref aux) = plan.aux_refs {
            let resolve = |r: &AuxTexRef| -> Option<&wgpu::TextureView> {
                r.tex_index.and_then(|idx| {
                    gpu_textures_dual
                        .get(idx)
                        .map(|(srgb, unorm)| if r.use_srgb { srgb } else { unorm })
                })
            };
            let matcap_view = resolve(&aux.matcap).unwrap_or(&default_black_view);
            let shade_mul_view = resolve(&aux.shade).unwrap_or(&default_white_view);
            let shift_view = resolve(&aux.shift).unwrap_or(&default_white_view);
            let rim_mul_view = resolve(&aux.rim).unwrap_or(&default_white_view);
            let uv_mask_view = resolve(&aux.uv_mask).unwrap_or(&default_white_view);
            let outline_width_view = resolve(&aux.outline_width).unwrap_or(&default_white_view);
            let emissive_view = resolve(&aux.emissive).unwrap_or(&default_white_view);
            let normal_view = resolve(&aux.normal).unwrap_or(&default_flat_normal_view);

            // Pre-register a sampler per texture.
            for si in [
                &aux.matcap.sampler,
                &aux.shade.sampler,
                &aux.shift.sampler,
                &aux.rim.sampler,
                &aux.uv_mask.sampler,
                &aux.outline_width.sampler,
                &aux.emissive.sampler,
                &aux.normal.sampler,
            ] {
                ensure_sampler(&mut sampler_cache, device, si);
            }
            let expect_msg = "already registered by ensure_sampler";
            let matcap_sampler = sampler_cache.get(&aux.matcap.sampler).expect(expect_msg);
            let shade_sampler = sampler_cache.get(&aux.shade.sampler).expect(expect_msg);
            let shift_sampler = sampler_cache.get(&aux.shift.sampler).expect(expect_msg);
            let rim_sampler = sampler_cache.get(&aux.rim.sampler).expect(expect_msg);
            let uv_mask_sampler = sampler_cache.get(&aux.uv_mask.sampler).expect(expect_msg);
            let outline_sampler = sampler_cache
                .get(&aux.outline_width.sampler)
                .expect(expect_msg);
            let emissive_sampler = sampler_cache.get(&aux.emissive.sampler).expect(expect_msg);
            let normal_sampler = sampler_cache.get(&aux.normal.sampler).expect(expect_msg);
            Some(gpu::create_mtoon_aux_bind_group(
                device,
                &mtoon_aux_bgl,
                gpu::AuxTexEntry {
                    view: matcap_view,
                    sampler: matcap_sampler,
                },
                gpu::AuxTexEntry {
                    view: shade_mul_view,
                    sampler: shade_sampler,
                },
                gpu::AuxTexEntry {
                    view: shift_view,
                    sampler: shift_sampler,
                },
                gpu::AuxTexEntry {
                    view: rim_mul_view,
                    sampler: rim_sampler,
                },
                gpu::AuxTexEntry {
                    view: uv_mask_view,
                    sampler: uv_mask_sampler,
                },
                gpu::AuxTexEntry {
                    view: outline_width_view,
                    sampler: outline_sampler,
                },
                gpu::AuxTexEntry {
                    view: emissive_view,
                    sampler: emissive_sampler,
                },
                gpu::AuxTexEntry {
                    view: normal_view,
                    sampler: normal_sampler,
                },
            ))
        } else {
            None
        };

        draws.push(DrawCall {
            index_offset: plan.index_offset,
            index_count: plan.index_count,
            cull_mode: plan.cull_mode,
            is_alpha: plan.is_alpha,
            render_queue: plan.render_queue,
            render_queue_offset: plan.render_queue_offset,
            alpha_cutoff: plan.alpha_cutoff,
            texture_bind_group: tex_bg,
            material_buf: mat_buf,
            material_bind_group: mat_bg,
            material_index: plan.material_index,
            render_style: plan.render_style,
            has_edge: plan.has_edge,
            has_outline: plan.has_outline,
            center: plan.center,
            mtoon_aux_bind_group: mtoon_aux_bg,
            mmd_material_buf: None,
            mmd_material_bind_group: None,
            mmd_aux_bind_group: None,
            mmd_texture_bind_group: None,
        });
    }

    let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("model_vbuf"),
        contents: bytemuck::cast_slice(&prep.all_vertices),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    });

    let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("model_ibuf"),
        contents: bytemuck::cast_slice(&prep.all_indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    // Edge-scale buffer (MMD-edge specific).
    let edge_scale_buf = prep.edge_scales.as_ref().map(|scales| {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("edge_scale_buf"),
            contents: bytemuck::cast_slice(scales),
            usage: wgpu::BufferUsages::VERTEX,
        })
    });

    let morph_work = Vec::with_capacity(prep.base_vertices.len());
    let (gpu_texture_views, gpu_texture_views_unorm): (Vec<_>, Vec<_>) =
        gpu_textures_dual.into_iter().unzip();
    Ok(GpuModel {
        vertex_buf,
        index_buf,
        draws,
        has_alpha: prep.has_alpha,
        edge_scale_buf,
        gpu_texture_views,
        gpu_texture_views_unorm,
        base_vertices: prep.base_vertices,
        base_indices: prep.all_indices,
        global_to_gpu: prep.global_to_gpu,
        use_vrm0_coords: prep.use_vrm0_coords,
        cached_bbox: prep.cached_bbox,
        morph_work,
        gpu_morphs: prep.gpu_morphs,
        morph_visited: Vec::new(),
        last_weights: Vec::new(),
        morph_cache_dirty: false,
        animated_vertices: None,
        material_base_values: prep.material_base_values,
    })
}

pub fn build_gpu_model_inner(
    ir: &IrModel,
    gpu_textures_dual: Vec<(wgpu::TextureView, wgpu::TextureView)>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    flags: &MaterialBuildFlags,
) -> Result<GpuModel> {
    let prep = cpu_prep_model(ir, flags)?;
    gpu_finalize_model(prep, gpu_textures_dual, device, queue)
}

/// Custom-normal clear (per-material variant): recompute normals only for the vertices of materials with clear_per_mat = true.
fn recalculate_normals_selective(
    vertices: &mut [Vertex],
    indices: &[u32],
    draws: &[CpuDrawPlan],
    clear_per_mat: &[bool],
) {
    use std::collections::{HashMap, HashSet};

    let num_verts = vertices.len();

    // Collect the vertex indices targeted for clear.
    let mut target_verts: HashSet<u32> = HashSet::new();
    for draw in draws {
        if clear_per_mat
            .get(draw.material_index)
            .copied()
            .unwrap_or(false)
        {
            let start = draw.index_offset as usize;
            let end = start + draw.index_count as usize;
            for &idx in &indices[start..end.min(indices.len())] {
                target_verts.insert(idx);
            }
        }
    }

    if target_verts.is_empty() {
        return;
    }

    // Group target vertices by position.
    let mut pos_groups: HashMap<[u32; 3], Vec<usize>> = HashMap::new();
    for &vi in &target_verts {
        let v = &vertices[vi as usize];
        let key = [
            v.position[0].to_bits(),
            v.position[1].to_bits(),
            v.position[2].to_bits(),
        ];
        pos_groups.entry(key).or_default().push(vi as usize);
    }

    // Accumulator for target-vertex normals.
    let mut accum = vec![Vec3::ZERO; num_verts];

    // Accumulate face normals weighted by angle, only for triangles that include a target vertex.
    for tri in indices.chunks_exact(3) {
        let (i0, i1, i2) = (tri[0], tri[1], tri[2]);
        // Whether this triangle contains a target vertex.
        if !target_verts.contains(&i0) && !target_verts.contains(&i1) && !target_verts.contains(&i2)
        {
            continue;
        }
        let (i0, i1, i2) = (i0 as usize, i1 as usize, i2 as usize);
        if i0 >= num_verts || i1 >= num_verts || i2 >= num_verts {
            continue;
        }
        let v0 = Vec3::from(vertices[i0].position);
        let v1 = Vec3::from(vertices[i1].position);
        let v2 = Vec3::from(vertices[i2].position);
        let face_normal = (v1 - v0).cross(v2 - v0);
        let area = face_normal.length();
        if area < 1e-10 {
            continue;
        }
        let fn_normalized = face_normal / area;

        let edges = [
            (i0, v1 - v0, v2 - v0),
            (i1, v0 - v1, v2 - v1),
            (i2, v0 - v2, v1 - v2),
        ];
        for (vi, e1, e2) in edges {
            if !target_verts.contains(&(vi as u32)) {
                continue;
            }
            let l1 = e1.length();
            let l2 = e2.length();
            if l1 < 1e-10 || l2 < 1e-10 {
                continue;
            }
            let cos_angle = (e1.dot(e2) / (l1 * l2)).clamp(-1.0, 1.0);
            let angle = cos_angle.acos();
            accum[vi] += fn_normalized * angle;
        }
    }

    // Sum and normalize the normals of the target vertices that share a position.
    for group in pos_groups.values() {
        let mut sum = Vec3::ZERO;
        for &vi in group {
            sum += accum[vi];
        }
        let n = sum.normalize_or_zero();
        let n_arr = n.to_array();
        for &vi in group {
            vertices[vi].normal = n_arr;
        }
    }
}

/// Re-orthogonalize tangents after normal recomputation (Gram-Schmidt).
/// tangent.w (handedness) is not modified; tangent.xyz is projected perpendicular to the normal.
fn reorthogonalize_tangents(vertices: &mut [Vertex]) {
    for v in vertices.iter_mut() {
        let n = Vec3::from(v.normal);
        let t = Vec3::from_slice(&v.tangent[..3]);
        let t_ortho = (t - n * n.dot(t)).normalize_or_zero();
        v.tangent[0] = t_ortho.x;
        v.tangent[1] = t_ortho.y;
        v.tangent[2] = t_ortho.z;
        // tangent[3] (handedness) is left untouched.
    }
}

/// Vertex-welding key (compares the bit pattern of position + UV0 + UV1; normals are excluded since they get averaged).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct PosUvKey {
    pos: [u32; 3],
    uv0: [u32; 2],
    uv1: [u32; 2],
}

impl PosUvKey {
    fn new(pos: [f32; 3], uv0: [f32; 2], uv1: [f32; 2]) -> Self {
        Self {
            pos: [pos[0].to_bits(), pos[1].to_bits(), pos[2].to_bits()],
            uv0: [uv0[0].to_bits(), uv0[1].to_bits()],
            uv1: [uv1[0].to_bits(), uv1[1].to_bits()],
        }
    }
}
