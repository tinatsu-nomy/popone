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

/// GPU空間で重複排除・座標変換済みのモーフデータ
enum GpuMorphEntry {
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
            for (local_vi, v) in mesh.vertices.iter_mut().enumerate() {
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
    pub fn apply_morphs(&mut self, weights: &[f32], queue: &wgpu::Queue) {
        self.morph_work.clear();
        self.morph_work.extend_from_slice(&self.base_vertices);

        let morph_len = self.gpu_morphs.len();
        for morph_idx in 0..morph_len {
            let w = weights.get(morph_idx).copied().unwrap_or(0.0);
            if w.abs() < 1e-6 {
                continue;
            }
            self.morph_visited.clear();
            self.morph_visited.resize(morph_len, false);
            Self::apply_gpu_morph_recursive(
                &self.gpu_morphs,
                morph_idx,
                w,
                &mut self.morph_work,
                &mut self.morph_visited,
            );
        }

        // CPU 側の現在頂点も同期 — swap でアロケーション回避
        let mut swap_buf = self.animated_vertices.take().unwrap_or_default();
        std::mem::swap(&mut self.morph_work, &mut swap_buf);
        self.animated_vertices = Some(swap_buf);

        queue.write_buffer(
            &self.vertex_buf,
            0,
            bytemuck::cast_slice(self.animated_vertices.as_ref().unwrap()),
        );
    }

    /// モーフウェイトを外部バッファに適用（アニメーション用：GPU アップロードはしない）
    pub fn apply_morphs_to_buf(&self, weights: &[f32], vertices: &mut [Vertex]) {
        let morph_len = self.gpu_morphs.len();
        let mut visited = vec![false; morph_len];
        for morph_idx in 0..morph_len {
            let w = weights.get(morph_idx).copied().unwrap_or(0.0);
            if w.abs() < 1e-6 {
                continue;
            }
            visited.fill(false);
            Self::apply_gpu_morph_recursive(&self.gpu_morphs, morph_idx, w, vertices, &mut visited);
        }
    }

    /// animated_vertices にモーフを直接適用（借用衝突回避版）
    pub fn apply_morphs_to_animated(&mut self, weights: &[f32]) {
        if let Some(ref mut verts) = self.animated_vertices {
            let morph_len = self.gpu_morphs.len();
            for morph_idx in 0..morph_len {
                let w = weights.get(morph_idx).copied().unwrap_or(0.0);
                if w.abs() < 1e-6 {
                    continue;
                }
                self.morph_visited.clear();
                self.morph_visited.resize(morph_len, false);
                Self::apply_gpu_morph_recursive(
                    &self.gpu_morphs,
                    morph_idx,
                    w,
                    verts,
                    &mut self.morph_visited,
                );
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
                            "グループモーフ[{}]: サブインデックス {} が範囲外 (len={})",
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
    smooth_per_mat: &[bool],
    clear_per_mat: &[bool],
) -> Result<GpuModel> {
    let gpu_textures = super::texture::upload_textures(ir, images, device, queue)?;
    build_gpu_model_inner(
        ir,
        gpu_textures,
        device,
        queue,
        smooth_per_mat,
        clear_per_mat,
    )
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
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("preview_sampler"),
        mag_filter: match info.mag_filter {
            IrMagFilter::Nearest => wgpu::FilterMode::Nearest,
            IrMagFilter::Linear => wgpu::FilterMode::Linear,
        },
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
        device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ir_sampler"),
            mag_filter: match info.mag_filter {
                IrMagFilter::Nearest => wgpu::FilterMode::Nearest,
                IrMagFilter::Linear => wgpu::FilterMode::Linear,
            },
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
            ..Default::default()
        })
    })
}

/// IrModel のみから GPU バッファを構築（FBX 用）
pub fn build_gpu_model_from_ir(
    ir: &IrModel,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    smooth_per_mat: &[bool],
    clear_per_mat: &[bool],
) -> Result<GpuModel> {
    let gpu_textures = super::texture::upload_textures_from_ir(ir, device, queue)?;
    build_gpu_model_inner(
        ir,
        gpu_textures,
        device,
        queue,
        smooth_per_mat,
        clear_per_mat,
    )
}

fn build_gpu_model_inner(
    ir: &IrModel,
    gpu_textures_dual: Vec<(wgpu::TextureView, wgpu::TextureView)>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    smooth_per_mat: &[bool],
    clear_per_mat: &[bool],
) -> Result<GpuModel> {
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
    let mut draws: Vec<DrawCall> = Vec::with_capacity(ir.materials.len());
    let mut has_alpha = false;

    // グローバル頂点Index → GPU頂点Index マッピング
    let total_global_verts: usize = ir.meshes.iter().map(|m| m.vertices.len()).sum();
    let mut global_to_gpu = vec![0u32; total_global_verts];

    // 頂点統合（vertex welding）用マップ: 位置+UV キー → GPU頂点Index
    let mut vertex_dedup: HashMap<PosUvKey, u32> = HashMap::with_capacity(total_verts);
    // 法線累積カウント（平均化用）
    let mut normal_accum: Vec<([f32; 3], u32)> = Vec::with_capacity(total_verts);

    let texture_bgl = gpu::create_texture_bind_group_layout(device);
    // サンプラーキャッシュ: IrSamplerInfo → wgpu::Sampler（重複生成を回避）
    let mut sampler_cache: HashMap<IrSamplerInfo, wgpu::Sampler> = HashMap::new();
    // デフォルトサンプラー（sampler 情報がないテクスチャ用）
    let default_sampler_info = IrSamplerInfo::default();
    ensure_sampler(&mut sampler_cache, device, &default_sampler_info);

    let material_bgl = gpu::create_material_bind_group_layout(device);
    let mtoon_aux_bgl = gpu::create_mtoon_aux_bind_group_layout_pub(device);

    // デフォルトテクスチャビュー（MToon 補助 bind group 用）
    let default_white_view = gpu::create_white_texture_view_srgb(device, queue);
    let default_black_view = gpu::create_black_texture_view_pub(device, queue);
    let default_flat_normal_view = gpu::create_flat_normal_texture_view(device, queue);

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

        // 材質ごとの法線平滑化フラグ（法線マップ付き材質は強制無効化）
        let mat_smooth =
            smooth_per_mat.get(mat_idx).copied().unwrap_or(false) && mat.normal_texture.is_none();

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

        // テクスチャ bind group（sRGB ビューを使用 — 標準描画用）
        // 材質の base_color_tex_info からサンプラー情報を取得
        let base_sampler_info = mat
            .base_color_tex_info
            .as_ref()
            .map(|ti| &ti.sampler)
            .unwrap_or(&default_sampler_info);
        // 材質用サンプラーを事前にキャッシュに登録
        ensure_sampler(&mut sampler_cache, device, base_sampler_info);
        let tex_bg = mat.texture_index.and_then(|ti| {
            gpu_textures_dual.get(ti).map(|(srgb_view, _)| {
                let sampler = sampler_cache.get(base_sampler_info).unwrap();
                gpu::create_texture_bind_group(device, &texture_bgl, srgb_view, sampler)
            })
        });

        // 材質 bind group
        let diffuse = mat.diffuse;
        let mp = mat.mtoon();
        let shade_color = mp.shade_color.unwrap_or(Vec3::ZERO).to_array();
        let outline_mode = match mp.outline_width_mode {
            OutlineWidthMode::None => 0.0,
            OutlineWidthMode::WorldCoordinates => 1.0,
            OutlineWidthMode::ScreenCoordinates => 2.0,
        };
        let mat_bg = gpu::create_material_bind_group(
            device,
            &material_bgl,
            diffuse.to_array(),
            shade_color,
            mat.is_mtoon(),
            mp.shading_toony_factor,
            mp.shading_shift_factor,
            mp.outline_width_factor,
            outline_mode,
            mat.edge_color.to_array(),
            mp.outline_lighting_mix,
            mp.parametric_rim_color.to_array(),
            mp.parametric_rim_fresnel_power,
            mp.parametric_rim_lift,
            mp.rim_lighting_mix,
            mp.matcap_texture.is_some(),
            mp.matcap_factor.to_array(),
            mp.shade_texture.is_some(),
            mp.shading_shift_texture.is_some(),
            mp.shading_shift_texture_scale,
            mp.rim_multiply_texture.is_some(),
            mp.uv_animation_scroll_x_speed,
            mp.uv_animation_scroll_y_speed,
            mp.uv_animation_rotation_speed,
            mp.uv_animation_mask_texture.is_some(),
            // alphaMode エンコーディング: OPAQUE=-1.0, MASK=cutoff(>=0.0), BLEND=-0.5
            match mat.alpha_mode {
                AlphaMode::Opaque => -1.0,
                AlphaMode::Mask => mat.alpha_cutoff, // 0.0 も合法値
                _ => -0.5,                           // Blend / BlendZWrite
            },
            gpu::pack_uv_params(mat.base_color_tex_info.as_ref()),
            gpu::pack_uv_params(mp.shade_texture.as_ref()),
            gpu::pack_uv_params(mp.shading_shift_texture.as_ref()),
            gpu::pack_uv_params(mp.rim_multiply_texture.as_ref()),
            gpu::pack_uv_params(mp.outline_width_texture.as_ref()),
            gpu::pack_uv_params(mp.uv_animation_mask_texture.as_ref()),
            mat.emissive_factor.to_array(),
            mat.emissive_texture.is_some(),
            gpu::pack_uv_params(mat.emissive_texture.as_ref()),
            mat.normal_texture.is_some(),
            mat.normal_texture_scale,
            gpu::pack_uv_params(mat.normal_texture.as_ref()),
            mp.gi_equalization_factor,
            mp.outline_width_tex_channel.to_f32(),
            mp.uv_anim_mask_tex_channel.to_f32(),
            gpu::pack_uv_params(mp.matcap_texture.as_ref()),
        );

        // MToon 補助テクスチャ bind group（group 3）
        // MToon 材質だけでなく emissiveTexture を持つ非 MToon 材質にも必要
        let needs_aux =
            mat.is_mtoon() || mat.emissive_texture.is_some() || mat.normal_texture.is_some();
        let mtoon_aux_bg = if needs_aux {
            let get_srgb = |idx: Option<usize>| -> Option<&wgpu::TextureView> {
                idx.and_then(|ti| gpu_textures_dual.get(ti).map(|(srgb, _)| srgb))
            };
            let get_linear = |idx: Option<usize>| -> Option<&wgpu::TextureView> {
                idx.and_then(|ti| gpu_textures_dual.get(ti).map(|(_, unorm)| unorm))
            };
            let default_white = &default_white_view;
            let default_black = &default_black_view;
            let matcap_view =
                get_srgb(mp.matcap_texture.as_ref().map(|t| t.index)).unwrap_or(default_black);
            let shade_mul_view =
                get_srgb(mp.shade_texture.as_ref().map(|t| t.index)).unwrap_or(default_white);
            // shadingShiftTexture: 仕様でリニア色空間と規定（Unorm ビュー使用）
            let shift_view = get_linear(mp.shading_shift_texture.as_ref().map(|t| t.index))
                .unwrap_or(default_white);
            let rim_mul_view = get_srgb(mp.rim_multiply_texture.as_ref().map(|t| t.index))
                .unwrap_or(default_white);
            // uvAnimationMaskTexture: 仕様でリニア色空間と規定（Unorm ビュー使用）
            let uv_mask_view = get_linear(mp.uv_animation_mask_texture.as_ref().map(|t| t.index))
                .unwrap_or(default_white);
            // outlineWidthMultiplyTexture: 仕様でリニア色空間と規定（Gチャンネル参照）
            let outline_width_view = get_linear(mp.outline_width_texture.as_ref().map(|t| t.index))
                .unwrap_or(default_white);
            // emissiveTexture: sRGB 色空間
            let emissive_view =
                get_srgb(mat.emissive_texture.as_ref().map(|t| t.index)).unwrap_or(default_white);
            // normalTexture: リニア色空間（Unorm ビュー使用）
            let normal_view = get_linear(mat.normal_texture.as_ref().map(|t| t.index))
                .unwrap_or(&default_flat_normal_view);
            // テクスチャごとに sampler を事前登録（glTF texture 単位 sampler に準拠）
            let sampler_of = |ti: Option<&IrTextureInfo>| -> IrSamplerInfo {
                ti.map(|t| t.sampler).unwrap_or_default()
            };
            let matcap_si = sampler_of(mp.matcap_texture.as_ref());
            let shade_si = sampler_of(mp.shade_texture.as_ref());
            let shift_si = sampler_of(mp.shading_shift_texture.as_ref());
            let rim_si = sampler_of(mp.rim_multiply_texture.as_ref());
            let uv_mask_si = sampler_of(mp.uv_animation_mask_texture.as_ref());
            let outline_si = sampler_of(mp.outline_width_texture.as_ref());
            let emissive_si = sampler_of(mat.emissive_texture.as_ref());
            let normal_si = sampler_of(mat.normal_texture.as_ref());
            for si in [
                &matcap_si,
                &shade_si,
                &shift_si,
                &rim_si,
                &uv_mask_si,
                &outline_si,
                &emissive_si,
                &normal_si,
            ] {
                ensure_sampler(&mut sampler_cache, device, si);
            }
            let matcap_sampler = sampler_cache.get(&matcap_si).unwrap();
            let shade_sampler = sampler_cache.get(&shade_si).unwrap();
            let shift_sampler = sampler_cache.get(&shift_si).unwrap();
            let rim_sampler = sampler_cache.get(&rim_si).unwrap();
            let uv_mask_sampler = sampler_cache.get(&uv_mask_si).unwrap();
            let outline_sampler = sampler_cache.get(&outline_si).unwrap();
            let emissive_sampler = sampler_cache.get(&emissive_si).unwrap();
            let normal_sampler = sampler_cache.get(&normal_si).unwrap();
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

        draws.push(DrawCall {
            index_offset,
            index_count,
            cull_mode: mat.cull_mode,
            is_alpha,
            render_queue,
            render_queue_offset: mp.render_queue_offset,
            alpha_cutoff: mat.alpha_cutoff,
            texture_bind_group: tex_bg,
            material_bind_group: mat_bg,
            material_index: mat_idx,
            render_style,
            has_edge,
            has_outline,
            center,
            mtoon_aux_bind_group: mtoon_aux_bg,
            mmd_material_buf: None,
            mmd_material_bind_group: None,
            mmd_aux_bind_group: None,
            mmd_texture_bind_group: None,
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
        recalculate_normals_selective(&mut all_vertices, &all_indices, &draws, clear_per_mat);
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
                let pos_map: HashMap<usize, Vec3> = positions.iter().copied().collect();
                let nrm_map: HashMap<usize, Vec3> = normals.iter().copied().collect();
                let tan_map: HashMap<usize, Vec3> = tangents.iter().copied().collect();
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

    let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("model_vbuf"),
        contents: bytemuck::cast_slice(&all_vertices),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    });

    let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("model_ibuf"),
        contents: bytemuck::cast_slice(&all_indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    // エッジスケールバッファ（MMD エッジ専用）
    // MToon アウトラインは GPU 側で outlineWidthMultiplyTexture をサンプリングするため不要
    // 注意: smooth_normals は材質ごとに制御され、vertex_dedup は材質ごとに
    // clear されるため、material 境界を越えた頂点統合は発生しない
    let has_mmd = draws.iter().any(|d| d.render_style == RenderStyle::Mmd);
    let edge_scale_buf = if has_mmd {
        let mut edge_scales = vec![1.0f32; all_vertices.len()];
        let mut global_vi = 0usize;
        for mesh in &ir.meshes {
            for v in &mesh.vertices {
                if let Some(&gpu_vi) = global_to_gpu.get(global_vi) {
                    edge_scales[gpu_vi as usize] = edge_scales[gpu_vi as usize].min(v.edge_scale);
                }
                global_vi += 1;
            }
        }
        Some(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("edge_scale_buf"),
                contents: bytemuck::cast_slice(&edge_scales),
                usage: wgpu::BufferUsages::VERTEX,
            }),
        )
    } else {
        None
    };

    let morph_work = Vec::with_capacity(base_vertices.len());
    let (gpu_texture_views, gpu_texture_views_unorm): (Vec<_>, Vec<_>) =
        gpu_textures_dual.into_iter().unzip();
    Ok(GpuModel {
        vertex_buf,
        index_buf,
        draws,
        has_alpha,
        edge_scale_buf,
        gpu_texture_views,
        gpu_texture_views_unorm,
        base_vertices,
        base_indices: all_indices,
        global_to_gpu,
        use_vrm0_coords: ir.source_format.is_vrm0(),
        cached_bbox,
        morph_work,
        gpu_morphs,
        morph_visited: Vec::new(),
        animated_vertices: None,
    })
}

/// カスタム法線クリア（材質選択版）: clear_per_mat が true の材質の頂点のみ法線を再計算
fn recalculate_normals_selective(
    vertices: &mut [Vertex],
    indices: &[u32],
    draws: &[DrawCall],
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
