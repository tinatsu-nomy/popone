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
    AlphaMode, CullMode, IrMagFilter, IrMinFilter, IrModel, IrMorphKind, IrSamplerInfo,
    IrTextureInfo, IrWrapMode, OutlineWidthMode,
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

/// GPU空間で重複排除・座標変換済みのモーフデータ
#[allow(clippy::type_complexity)]
pub(crate) enum GpuMorphEntry {
    /// 頂点モーフ: (gpu_vi, pos_delta, normal_delta, tangent_delta)
    Vertex(Vec<(u32, [f32; 3], [f32; 3], [f32; 3])>),
    /// グループモーフ: (サブモーフIndex, ウェイト)
    Group(Vec<(usize, f32)>),
}

/// 描画方式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderStyle {
    Standard,
    Mmd,
}

/// MToon 仕様に基づくレンダーキュー（描画順序カテゴリ）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RenderQueue {
    /// 不透明（デプス書込あり）
    Opaque = 0,
    /// alphaCutoff でカットアウト（デプス書込あり）
    Mask = 1,
    /// 半透明・デプス書込あり（MToon transparentWithZWrite）
    BlendZWrite = 2,
    /// 半透明・デプス書込なし
    Blend = 3,
}

/// 材質ごとの描画情報
pub struct DrawCall {
    pub index_offset: u32,
    pub index_count: u32,
    pub cull_mode: CullMode,
    pub is_alpha: bool,
    /// MToon 仕様準拠レンダーキュー
    pub render_queue: RenderQueue,
    /// renderQueueOffsetNumber（BLEND 内ソート用）
    pub render_queue_offset: i32,
    /// MASK モード時の alphaCutoff
    pub alpha_cutoff: f32,
    pub texture_bind_group: Option<wgpu::BindGroup>,
    pub material_bind_group: wgpu::BindGroup,
    pub material_index: usize,
    pub render_style: RenderStyle,
    pub has_edge: bool,
    /// MToon アウトライン描画対象
    pub has_outline: bool,
    /// MToon 補助テクスチャ bind group（group 3: matcap + shade + shift + rim + uvMask）
    pub mtoon_aux_bind_group: Option<wgpu::BindGroup>,
    /// 描画メッシュの重心位置（半透明距離ソート用）
    pub center: glam::Vec3,
    // MMD 用 BindGroup（prepare_mmd_resources で設定）
    pub mmd_material_buf: Option<wgpu::Buffer>,
    pub mmd_material_bind_group: Option<wgpu::BindGroup>,
    pub mmd_aux_bind_group: Option<wgpu::BindGroup>,
    /// MMD 用テクスチャ bind group（Unorm ビュー使用）
    pub mmd_texture_bind_group: Option<wgpu::BindGroup>,
}

/// GPU上のモデルデータ
pub struct GpuModel {
    pub vertex_buf: wgpu::Buffer,
    pub index_buf: wgpu::Buffer,
    pub draws: Vec<DrawCall>,
    pub has_alpha: bool,
    /// エッジスケールバッファ（MMD エッジ用、f32 × 頂点数）
    pub edge_scale_buf: Option<wgpu::Buffer>,
    /// GPU テクスチャビュー sRGB（標準描画用）
    pub gpu_texture_views: Vec<wgpu::TextureView>,
    /// GPU テクスチャビュー Unorm（MMD 描画用）
    pub gpu_texture_views_unorm: Vec<wgpu::TextureView>,
    /// ベース頂点（モーフ適用前）
    base_vertices: Vec<Vertex>,
    /// インデックスバッファの生データ（法線表示フィルタ用）
    base_indices: Vec<u32>,
    /// IrModel グローバル頂点Index → GPU 頂点Index
    global_to_gpu: Vec<u32>,
    /// VRM 0.0 座標変換を使うか
    use_vrm0_coords: bool,
    /// キャッシュ済みバウンディングボックス (min, max)
    cached_bbox: (Vec3, Vec3),
    /// モーフ適用用作業バッファ（毎フレーム clone を回避）
    morph_work: Vec<Vertex>,
    /// GPU空間モーフデータ（重複排除・座標変換済み）
    gpu_morphs: Vec<GpuMorphEntry>,
    /// グループモーフ循環検出用バッファ（毎回 alloc を回避）
    morph_visited: Vec<bool>,
    /// 前回適用時の morph weights（変化がなければ再計算をスキップ）
    last_weights: Vec<f32>,
    /// morph weights キャッシュ無効化フラグ（アニメーション解除時等に使用）
    morph_cache_dirty: bool,
    /// アニメーション済み頂点キャッシュ（法線表示同期用）
    animated_vertices: Option<Vec<Vertex>>,
}

impl GpuModel {
    /// 指定材質にテクスチャを割り当て（DrawCall の bind group を更新）
    /// sampler_info から材質固有のサンプラーを生成して使用する
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

    /// バウンディングボックスを取得（キャッシュ済み）
    pub fn bbox(&self) -> (Vec3, Vec3) {
        self.cached_bbox
    }

    /// グローバル頂点Index → GPU頂点Index マッピングを取得（アニメーション用）
    pub fn global_to_gpu_map(&self) -> &[u32] {
        &self.global_to_gpu
    }

    /// ベース頂点を取得（法線表示等に使用）
    pub fn base_vertices(&self) -> &[Vertex] {
        &self.base_vertices
    }

    /// アニメーション済み頂点を取得（あればアニメ済み、なければベース）
    pub fn current_vertices(&self) -> &[Vertex] {
        self.animated_vertices
            .as_deref()
            .unwrap_or(&self.base_vertices)
    }

    /// アニメーション済み頂点をキャッシュ
    pub fn set_animated_vertices(&mut self, verts: Vec<Vertex>) {
        self.animated_vertices = Some(verts);
    }

    /// モーフウェイトキャッシュを無効化（次回 apply_morphs で強制再計算）
    pub fn invalidate_morph_cache(&mut self) {
        self.morph_cache_dirty = true;
    }

    /// ベース頂点を animated_vertices にコピー（バッファ再利用、毎フレーム alloc 回避）
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

    /// アニメーション済み頂点への可変参照
    pub fn animated_vertices_mut(&mut self) -> &mut [Vertex] {
        self.animated_vertices.as_deref_mut().unwrap_or(&mut [])
    }

    /// アニメーション済み頂点キャッシュをクリア
    pub fn clear_animated_vertices(&mut self) {
        self.animated_vertices = None;
    }

    /// インデックスバッファの生データを取得（法線表示のフィルタ用）
    pub fn base_indices(&self) -> &[u32] {
        &self.base_indices
    }

    /// GPU モデルの法線を IrModel に書き戻す（PMX 変換時に再計算済み法線を反映）
    /// 座標変換は自己逆（Z反転/X反転を2回で元に戻る）なので同じ関数で逆変換
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
                        // GPU法線(PMX座標系) → glTF座標系に逆変換
                        v.normal = inv_normal_fn(Vec3::from(gpu_v.normal));
                    }
                }
            }
        }
    }

    /// モーフウェイトを適用して頂点バッファを更新
    /// weights が前回と同一なら早期リターンして再計算をスキップする
    pub fn apply_morphs(&mut self, weights: &[f32], queue: &wgpu::Queue) {
        // weights が前回から変化していなければ何もしない（キャッシュ無効化時は強制実行）
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
        // visited バッファを1回だけ確保し、各モーフ後は fill(false) で再利用
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

        // CPU 側の現在頂点も同期 — swap でアロケーション回避
        let mut swap_buf = self.animated_vertices.take().unwrap_or_default();
        std::mem::swap(&mut self.morph_work, &mut swap_buf);
        self.animated_vertices = Some(swap_buf);

        queue.write_buffer(
            &self.vertex_buf,
            0,
            bytemuck::cast_slice(
                self.animated_vertices
                    .as_ref()
                    .expect("animated_vertices は apply_morphs 内で必ず Some に設定済み"),
            ),
        );

        // 次回比較用に weights を記録
        self.last_weights.clear();
        self.last_weights.extend_from_slice(weights);
    }

    /// モーフウェイトを外部バッファに適用（アニメーション用：GPU アップロードはしない）
    pub fn apply_morphs_to_buf(&mut self, weights: &[f32], vertices: &mut [Vertex]) {
        let morph_len = self.gpu_morphs.len();
        // visited バッファを1回だけ確保し fill(false) で再利用
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

    /// animated_vertices にモーフを直接適用（借用衝突回避版）
    pub fn apply_morphs_to_animated(&mut self, weights: &[f32]) {
        if let Some(ref mut verts) = self.animated_vertices {
            let morph_len = self.gpu_morphs.len();
            // visited バッファを1回だけ確保し fill(false) で再利用
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
            return; // 循環参照を検出 — スキップ
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
        }
    }
}

/// IrModel + GlbData から GPU バッファを構築
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

/// IrMinFilter を wgpu の (min_filter, mipmap_filter) ペアに変換する
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

/// IrSamplerInfo から wgpu::Sampler を作成（プレビュー等で単発使用）
pub fn create_sampler_from_info(device: &wgpu::Device, info: &IrSamplerInfo) -> wgpu::Sampler {
    let (min_filter, mipmap_filter) = ir_min_filter_to_wgpu(info.min_filter);
    let mag_filter = match info.mag_filter {
        IrMagFilter::Nearest => wgpu::FilterMode::Nearest,
        IrMagFilter::Linear => wgpu::FilterMode::Linear,
    };
    // anisotropy_clamp > 1 は全フィルタが Linear の場合のみ有効（wgpu/WebGPU 仕様）
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

/// IrSamplerInfo に対応する wgpu::Sampler をキャッシュから取得（なければ作成）
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

/// IrModel のみから GPU バッファを構築（FBX 用）
pub fn build_gpu_model_from_ir(
    ir: &IrModel,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    flags: &MaterialBuildFlags,
) -> Result<GpuModel> {
    let gpu_textures = super::texture::upload_textures_from_ir(ir, device, queue)?;
    build_gpu_model_inner(ir, gpu_textures, device, queue, flags)
}

/// MToon 補助テクスチャの参照情報（CPU フェーズ用）
pub(crate) struct AuxTexRef {
    pub tex_index: Option<usize>,
    pub sampler: IrSamplerInfo,
    /// true = sRGB ビュー使用, false = linear/Unorm ビュー使用
    pub use_srgb: bool,
}

/// MToon 補助テクスチャ参照（8 スロット）
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

/// 材質ごとの描画計画（GPU bind group 作成前の CPU 側データ）
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
    // Bind group 構築用メタデータ
    pub base_tex_index: Option<usize>,
    pub base_sampler: IrSamplerInfo,
    pub material_params: gpu::MaterialParams,
    pub needs_aux: bool,
    pub aux_refs: Option<AuxTexRefs>,
}

/// CPU プリプロセスフェーズの出力（GPU API 不要）
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
}

/// CPU プリプロセスフェーズ: 頂点変換・法線平滑化・モーフ前計算（GPU API 呼び出しなし）
pub(crate) fn cpu_prep_model(ir: &IrModel, flags: &MaterialBuildFlags) -> Result<CpuPrepResult> {
    let smooth_per_mat = &flags.smooth;
    let clear_per_mat = &flags.clear;
    let normal_map_per_mat = &flags.normal_map;
    let emissive_per_mat = &flags.emissive;
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

    // グローバル頂点Index → GPU頂点Index マッピング
    let total_global_verts: usize = ir.meshes.iter().map(|m| m.vertices.len()).sum();
    let mut global_to_gpu = vec![0u32; total_global_verts];

    // 頂点統合（vertex welding）用マップ: 位置+UV キー → GPU頂点Index
    let mut vertex_dedup: HashMap<PosUvKey, u32> = HashMap::with_capacity(total_verts);
    // 法線累積カウント（平均化用）
    let mut normal_accum: Vec<([f32; 3], u32)> = Vec::with_capacity(total_verts);

    let default_sampler_info = IrSamplerInfo::default();

    // 各メッシュのグローバル頂点オフセット（メッシュ元順序）
    let mut mesh_global_offsets = Vec::with_capacity(ir.meshes.len());
    let mut offset = 0usize;
    for mesh in &ir.meshes {
        mesh_global_offsets.push(offset);
        offset += mesh.vertices.len();
    }

    // 材質ごとにメッシュを集計
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

        // 材質ごとの法線平滑化フラグ（法線マップと併用可: TBN 基底法線の平滑化で品質向上）
        let mat_smooth = smooth_per_mat.get(mat_idx).copied().unwrap_or(false);

        // 材質ごとに vertex_dedup をリセット（異なる材質間で頂点を共有しない）
        vertex_dedup.clear();

        for &mi in mesh_indices {
            let mesh = &ir.meshes[mi];
            let global_offset = mesh_global_offsets[mi];

            // 頂点変換 + マッピング構築
            let has_uv1 = !mesh.uvs1.is_empty();
            for (local_vi, v) in mesh.vertices.iter().enumerate() {
                let pos = pos_fn(v.position);
                let normal = normal_fn(v.normal);
                // UV1: 存在すれば使用、なければゼロ（UniVRM MeshData.cs 準拠）
                let uv1 = if has_uv1 {
                    mesh.uvs1[local_vi]
                } else {
                    [0.0, 0.0]
                };

                // ミラー変換(det=-1)では cross(M*N, M*T) = -M*cross(N,T) となるため
                // bitangent の向きを維持するには tangent.w を反転する必要がある
                let tangent = normal_fn(v.tangent.truncate())
                    .normalize_or_zero()
                    .extend(-v.tangent.w);

                let gpu_vi = if mat_smooth {
                    // 位置+UVで統合、法線は累積して後で平均化
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
                    // normal_accum を all_vertices と同期（count=0 で平均化対象外）
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

            // インデックス
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

        // ベースカラーテクスチャのサンプラー情報
        let base_sampler = mat
            .base_color_tex_info
            .as_ref()
            .map(|ti| ti.sampler)
            .unwrap_or(default_sampler_info);

        // 材質パラメータ（純粋な計算 — GPU API 不要）
        let diffuse = mat.diffuse;
        let mp = mat.mtoon();
        let shade_color = mp.shade_color.unwrap_or(Vec3::ZERO).to_array();
        let outline_mode = match mp.outline_width_mode {
            OutlineWidthMode::None => 0.0,
            OutlineWidthMode::WorldCoordinates => 1.0,
            OutlineWidthMode::ScreenCoordinates => 2.0,
        };
        let material_params = gpu::MaterialParams {
            diffuse: diffuse.to_array(),
            shade_color,
            is_mtoon: mat.is_mtoon(),
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
            // alphaMode エンコーディング: OPAQUE=-1.0, MASK=cutoff(>=0.0), BLEND=-0.5
            alpha_cutoff: match mat.alpha_mode {
                AlphaMode::Opaque => -1.0,
                AlphaMode::Mask => mat.alpha_cutoff, // 0.0 も合法値
                _ => -0.5,                           // Blend / BlendZWrite
            },
            base_uv: gpu::pack_uv_params(mat.base_color_tex_info.as_ref()),
            shade_uv: gpu::pack_uv_params(mp.shade_texture.as_ref()),
            shift_uv: gpu::pack_uv_params(mp.shading_shift_texture.as_ref()),
            rim_uv: gpu::pack_uv_params(mp.rim_multiply_texture.as_ref()),
            outline_uv: gpu::pack_uv_params(mp.outline_width_texture.as_ref()),
            uv_mask_uv: gpu::pack_uv_params(mp.uv_animation_mask_texture.as_ref()),
            emissive_factor: if emissive_per_mat.get(mat_idx).copied().unwrap_or(true) {
                mat.emissive_factor.to_array()
            } else {
                [0.0; 3]
            },
            has_emissive_tex: mat.emissive_texture.is_some()
                && emissive_per_mat.get(mat_idx).copied().unwrap_or(true),
            emissive_uv: gpu::pack_uv_params(mat.emissive_texture.as_ref()),
            has_normal_tex: mat.normal_texture.is_some()
                && normal_map_per_mat.get(mat_idx).copied().unwrap_or(true),
            normal_scale: mat.normal_texture_scale,
            normal_uv: gpu::pack_uv_params(mat.normal_texture.as_ref()),
            gi_equalization_factor: mp.gi_equalization_factor,
            outline_width_channel: mp.outline_width_tex_channel.to_f32(),
            uv_anim_mask_channel: mp.uv_anim_mask_tex_channel.to_f32(),
            matcap_uv: gpu::pack_uv_params(mp.matcap_texture.as_ref()),
        };

        // MToon 補助テクスチャ参照（group 3）
        // MToon 材質だけでなく emissiveTexture を持つ非 MToon 材質にも必要
        let needs_aux =
            mat.is_mtoon() || mat.emissive_texture.is_some() || mat.normal_texture.is_some();
        let sampler_of = |ti: Option<&IrTextureInfo>| -> IrSamplerInfo {
            ti.map(|t| t.sampler).unwrap_or_default()
        };
        let aux_refs = if needs_aux {
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
        } else {
            None
        };

        // alphaMode ベースでレンダーキューを決定
        let render_queue = match mat.alpha_mode {
            AlphaMode::Opaque => RenderQueue::Opaque,
            AlphaMode::Mask => RenderQueue::Mask,
            AlphaMode::BlendWithZWrite => RenderQueue::BlendZWrite,
            AlphaMode::Blend => RenderQueue::Blend,
        };
        // is_alpha は後方互換（BLEND 系 or diffuse.w < 1.0）
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

        // 描画メッシュの重心を計算（半透明距離ソート用）
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

    // 累積法線を平均化・正規化（smooth 有効材質の頂点のみ、count > 0 で自動フィルタ）
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

    // カスタム法線クリア: 対象材質の頂点のみジオメトリから法線を再計算
    if any_clear {
        recalculate_normals_selective(&mut all_vertices, &all_indices, &draw_plans, clear_per_mat);
    }

    // normal 再計算後の tangent 再直交化（Gram-Schmidt）
    // smooth / clear で normal が変わると TBN 行列が不整合になるため
    if any_smooth || any_clear {
        reorthogonalize_tangents(&mut all_vertices);
    }

    // GPU空間モーフデータを事前計算（重複排除 + 座標変換済み）
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
                // positions / normals / tangents の和集合で影響頂点を収集
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
        })
        .collect();

    // ベース頂点を保存 + bbox 計算
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

    // エッジスケール計算（MMD エッジ専用、CPU のみ）
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
    })
}

/// GPU ファイナライズフェーズ: CPU 前計算結果から GPU リソースを生成
pub(crate) fn gpu_finalize_model(
    prep: CpuPrepResult,
    gpu_textures_dual: Vec<(wgpu::TextureView, wgpu::TextureView)>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<GpuModel> {
    let texture_bgl = gpu::create_texture_bind_group_layout(device);
    let material_bgl = gpu::create_material_bind_group_layout(device);
    let mtoon_aux_bgl = gpu::create_mtoon_aux_bind_group_layout_pub(device);

    // サンプラーキャッシュ: IrSamplerInfo → wgpu::Sampler（重複生成を回避）
    let mut sampler_cache: HashMap<IrSamplerInfo, wgpu::Sampler> = HashMap::new();
    let default_sampler_info = IrSamplerInfo::default();
    ensure_sampler(&mut sampler_cache, device, &default_sampler_info);

    // デフォルトテクスチャビュー（MToon 補助 bind group 用）
    let default_white_view = gpu::create_white_texture_view_srgb(device, queue);
    let default_black_view = gpu::create_black_texture_view_pub(device, queue);
    let default_flat_normal_view = gpu::create_flat_normal_texture_view(device, queue);

    let mut draws: Vec<DrawCall> = Vec::with_capacity(prep.draw_plans.len());

    for plan in &prep.draw_plans {
        // ベースカラーテクスチャ bind group（sRGB ビュー使用）
        ensure_sampler(&mut sampler_cache, device, &plan.base_sampler);
        let tex_bg = plan.base_tex_index.and_then(|ti| {
            gpu_textures_dual.get(ti).map(|(srgb_view, _)| {
                let sampler = sampler_cache
                    .get(&plan.base_sampler)
                    .expect("ensure_sampler で登録済み");
                gpu::create_texture_bind_group(device, &texture_bgl, srgb_view, sampler)
            })
        });

        // 材質 bind group
        let mat_bg = gpu::create_material_bind_group(device, &material_bgl, &plan.material_params);

        // MToon 補助テクスチャ bind group（group 3）
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

            // テクスチャごとに sampler を事前登録
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
            let expect_msg = "ensure_sampler で登録済み";
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

    // エッジスケールバッファ（MMD エッジ専用）
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

/// カスタム法線クリア（材質選択版）: clear_per_mat が true の材質の頂点のみ法線を再計算
fn recalculate_normals_selective(
    vertices: &mut [Vertex],
    indices: &[u32],
    draws: &[CpuDrawPlan],
    clear_per_mat: &[bool],
) {
    use std::collections::{HashMap, HashSet};

    let num_verts = vertices.len();

    // clear 対象の頂点インデックスを収集
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

    // 対象頂点の位置グルーピング
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

    // 対象頂点の法線累積
    let mut accum = vec![Vec3::ZERO; num_verts];

    // 対象頂点を含む三角形のみ面法線を角度加重で累積
    for tri in indices.chunks_exact(3) {
        let (i0, i1, i2) = (tri[0], tri[1], tri[2]);
        // この三角形に対象頂点が含まれているか
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

    // 同一位置の対象頂点の法線を合算して正規化
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

/// normal 再計算後の tangent 再直交化（Gram-Schmidt）
/// tangent.w（handedness）は変更せず、tangent.xyz を normal に対して直交射影する
fn reorthogonalize_tangents(vertices: &mut [Vertex]) {
    for v in vertices.iter_mut() {
        let n = Vec3::from(v.normal);
        let t = Vec3::from_slice(&v.tangent[..3]);
        let t_ortho = (t - n * n.dot(t)).normalize_or_zero();
        v.tangent[0] = t_ortho.x;
        v.tangent[1] = t_ortho.y;
        v.tangent[2] = t_ortho.z;
        // tangent[3] (handedness) は変更しない
    }
}

/// 頂点統合用キー（位置+UV0+UV1のビット表現で比較、法線は平均化するため含めない）
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
