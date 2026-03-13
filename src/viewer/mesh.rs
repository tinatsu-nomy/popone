use std::collections::{HashMap, HashSet};

use anyhow::Result;
use eframe::wgpu;
use glam::Vec3;
use wgpu::util::DeviceExt;

use crate::convert::coord::{
    flip_face_winding, gltf_normal_to_pmx, gltf_normal_to_pmx_v0, gltf_pos_to_pmx,
    gltf_pos_to_pmx_v0,
};
use crate::intermediate::types::{IrModel, IrMorphKind};

use super::gpu::{self, Vertex};

/// GPU空間で重複排除・座標変換済みのモーフデータ
enum GpuMorphEntry {
    /// 頂点モーフ: (gpu_vi, 変換済みオフセット)
    Vertex(Vec<(u32, [f32; 3])>),
    /// グループモーフ: (サブモーフIndex, ウェイト)
    Group(Vec<(usize, f32)>),
}

/// 材質ごとの描画情報
pub struct DrawCall {
    pub index_offset: u32,
    pub index_count: u32,
    pub double_sided: bool,
    pub is_alpha: bool,
    pub texture_bind_group: Option<wgpu::BindGroup>,
    pub material_bind_group: wgpu::BindGroup,
    pub material_index: usize,
}

/// GPU上のモデルデータ
pub struct GpuModel {
    pub vertex_buf: wgpu::Buffer,
    pub index_buf: wgpu::Buffer,
    pub draws: Vec<DrawCall>,
    pub has_alpha: bool,
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
    /// アニメーション済み頂点キャッシュ（法線表示同期用）
    animated_vertices: Option<Vec<Vertex>>,
}

impl GpuModel {
    /// 指定材質にテクスチャを割り当て（DrawCall の bind group を更新）
    pub fn assign_texture_to_material(
        &mut self,
        material_index: usize,
        texture_view: &wgpu::TextureView,
        device: &wgpu::Device,
        texture_bgl: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) {
        for draw in &mut self.draws {
            if draw.material_index == material_index {
                draw.texture_bind_group = Some(
                    gpu::create_texture_bind_group(device, texture_bgl, texture_view, sampler),
                );
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
        self.animated_vertices.as_deref().unwrap_or(&self.base_vertices)
    }

    /// アニメーション済み頂点をキャッシュ
    pub fn set_animated_vertices(&mut self, verts: Vec<Vertex>) {
        self.animated_vertices = Some(verts);
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
    pub fn apply_morphs(
        &mut self,
        weights: &[f32],
        queue: &wgpu::Queue,
    ) {
        // 作業バッファにベース頂点をコピー（Vec の再割り当てを回避）
        self.morph_work.clear();
        self.morph_work.extend_from_slice(&self.base_vertices);

        for morph_idx in 0..self.gpu_morphs.len() {
            let w = weights.get(morph_idx).copied().unwrap_or(0.0);
            if w.abs() < 1e-6 {
                continue;
            }
            Self::apply_gpu_morph_to(&self.gpu_morphs, morph_idx, w, &mut self.morph_work);
        }

        queue.write_buffer(&self.vertex_buf, 0, bytemuck::cast_slice(&self.morph_work));
    }

    /// モーフウェイトを外部バッファに適用（アニメーション用：GPU アップロードはしない）
    pub fn apply_morphs_to_buf(
        &self,
        weights: &[f32],
        vertices: &mut [Vertex],
    ) {
        for morph_idx in 0..self.gpu_morphs.len() {
            let w = weights.get(morph_idx).copied().unwrap_or(0.0);
            if w.abs() < 1e-6 {
                continue;
            }
            Self::apply_gpu_morph_to(&self.gpu_morphs, morph_idx, w, vertices);
        }
    }

    fn apply_gpu_morph_to(
        gpu_morphs: &[GpuMorphEntry],
        morph_idx: usize,
        weight: f32,
        vertices: &mut [Vertex],
    ) {
        match &gpu_morphs[morph_idx] {
            GpuMorphEntry::Vertex(voffs) => {
                for &(gpu_vi, offset) in voffs {
                    let vi = gpu_vi as usize;
                    if vi < vertices.len() {
                        vertices[vi].position[0] += offset[0] * weight;
                        vertices[vi].position[1] += offset[1] * weight;
                        vertices[vi].position[2] += offset[2] * weight;
                    }
                }
            }
            GpuMorphEntry::Group(goffs) => {
                for &(sub_idx, sub_weight) in goffs {
                    let effective = weight * sub_weight;
                    if effective.abs() < 1e-6 || sub_idx >= gpu_morphs.len() {
                        continue;
                    }
                    Self::apply_gpu_morph_to(gpu_morphs, sub_idx, effective, vertices);
                }
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
    smooth_normals: bool,
    clear_custom_normals: bool,
) -> Result<GpuModel> {
    let gpu_textures = super::texture::upload_textures(ir, images, device, queue)?;
    build_gpu_model_inner(ir, gpu_textures, device, smooth_normals, clear_custom_normals)
}

/// IrModel のみから GPU バッファを構築（FBX 用）
pub fn build_gpu_model_from_ir(
    ir: &IrModel,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    smooth_normals: bool,
    clear_custom_normals: bool,
) -> Result<GpuModel> {
    let gpu_textures = super::texture::upload_textures_from_ir(ir, device, queue)?;
    build_gpu_model_inner(ir, gpu_textures, device, smooth_normals, clear_custom_normals)
}

fn build_gpu_model_inner(
    ir: &IrModel,
    gpu_textures: Vec<wgpu::TextureView>,
    device: &wgpu::Device,
    smooth_normals: bool,
    clear_custom_normals: bool,
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
    let tex_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("build_sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Linear,
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        ..Default::default()
    });

    let material_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("material_bgl_mesh"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });

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

        // 材質ごとに vertex_dedup をリセット（異なる材質間で頂点を共有しない）
        vertex_dedup.clear();

        for &mi in mesh_indices {
            let mesh = &ir.meshes[mi];
            let global_offset = mesh_global_offsets[mi];

            // 頂点変換 + マッピング構築
            for (local_vi, v) in mesh.vertices.iter().enumerate() {
                let pos = pos_fn(v.position);
                let normal = normal_fn(v.normal);

                let gpu_vi = if smooth_normals {
                    // 位置+UVで統合、法線は累積して後で平均化
                    let key = PosUvKey::new(pos.to_array(), v.uv.to_array());
                    *vertex_dedup.entry(key).or_insert_with(|| {
                        let idx = all_vertices.len() as u32;
                        all_vertices.push(Vertex {
                            position: pos.to_array(),
                            normal: [0.0; 3],
                            uv: v.uv.to_array(),
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
                    });
                    idx
                };

                if smooth_normals {
                    let acc = &mut normal_accum[gpu_vi as usize];
                    acc.0[0] += normal.x;
                    acc.0[1] += normal.y;
                    acc.0[2] += normal.z;
                    acc.1 += 1;
                }
                global_to_gpu[global_offset + local_vi] = gpu_vi;
            }

            // インデックス
            let mut indices: Vec<u32> = if smooth_normals {
                mesh.indices.iter().map(|&i| global_to_gpu[global_offset + i as usize]).collect()
            } else {
                let base = global_to_gpu[global_offset];
                mesh.indices.iter().map(|&i| i + base).collect()
            };
            flip_face_winding(&mut indices);
            all_indices.extend_from_slice(&indices);
        }

        let index_count = all_indices.len() as u32 - index_offset;

        // テクスチャ bind group
        let tex_bg = mat.texture_index.and_then(|ti| {
            gpu_textures.get(ti).map(|view| {
                gpu::create_texture_bind_group(device, &texture_bgl, view, &tex_sampler)
            })
        });

        // 材質 bind group
        let diffuse = mat.diffuse;
        let mat_bg =
            gpu::create_material_bind_group(device, &material_bgl, diffuse.to_array());

        if diffuse.w < 1.0 {
            has_alpha = true;
        }

        let is_alpha = diffuse.w < 1.0;

        draws.push(DrawCall {
            index_offset,
            index_count,
            double_sided: mat.is_double_sided,
            is_alpha,
            texture_bind_group: tex_bg,
            material_bind_group: mat_bg,
            material_index: mat_idx,
        });
    }

    // 累積法線を平均化・正規化（smooth_normals 有効時のみ）
    if smooth_normals {
        for (vi, v) in all_vertices.iter_mut().enumerate() {
            if let Some(&(sum, count)) = normal_accum.get(vi) {
                if count > 0 {
                    let n = Vec3::new(sum[0], sum[1], sum[2]).normalize_or_zero();
                    v.normal = n.to_array();
                }
            }
        }
    }

    // カスタム法線クリア: ジオメトリから法線を再計算（位置ごとに面法線を加重平均）
    if clear_custom_normals {
        recalculate_normals_from_geometry(&mut all_vertices, &all_indices);
    }

    // GPU空間モーフデータを事前計算（重複排除 + 座標変換済み）
    let gpu_morphs: Vec<GpuMorphEntry> = ir.morphs.iter().map(|morph| {
        match &morph.kind {
            IrMorphKind::Vertex(voffs) => {
                let mut seen = HashSet::with_capacity(voffs.len());
                let mut deduped = Vec::with_capacity(voffs.len());
                for &(global_vi, offset) in voffs {
                    if let Some(&gpu_vi) = global_to_gpu.get(global_vi) {
                        if seen.insert(gpu_vi) {
                            let t = pos_fn(offset);
                            deduped.push((gpu_vi, t.to_array()));
                        }
                    }
                }
                GpuMorphEntry::Vertex(deduped)
            }
            IrMorphKind::Group(goffs) => {
                GpuMorphEntry::Group(goffs.clone())
            }
        }
    }).collect();

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

    let morph_work = Vec::with_capacity(base_vertices.len());
    Ok(GpuModel {
        vertex_buf,
        index_buf,
        draws,
        has_alpha,
        base_vertices,
        base_indices: all_indices,
        global_to_gpu,
        use_vrm0_coords: ir.source_format.is_vrm0(),
        cached_bbox,
        morph_work,
        gpu_morphs,
        animated_vertices: None,
    })
}

/// カスタム法線クリア: ジオメトリから法線を再計算
/// 同一位置の頂点をグルーピングし、面法線の角度加重平均を割り当てる
fn recalculate_normals_from_geometry(vertices: &mut [Vertex], indices: &[u32]) {
    use std::collections::HashMap;

    let num_verts = vertices.len();

    // 位置ごとに頂点インデックスをグルーピング
    let mut pos_groups: HashMap<[u32; 3], Vec<usize>> = HashMap::new();
    for (i, v) in vertices.iter().enumerate() {
        let key = [
            v.position[0].to_bits(),
            v.position[1].to_bits(),
            v.position[2].to_bits(),
        ];
        pos_groups.entry(key).or_default().push(i);
    }

    // 各頂点の法線累積
    let mut accum = vec![Vec3::ZERO; num_verts];

    // 各三角形の面法線を角度加重で累積
    for tri in indices.chunks_exact(3) {
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
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

        // 各頂点の角度を計算して加重
        let edges = [
            (i0, v1 - v0, v2 - v0),
            (i1, v0 - v1, v2 - v1),
            (i2, v0 - v2, v1 - v2),
        ];
        for (vi, e1, e2) in edges {
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

    // 同一位置の頂点の法線を合算して正規化
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

/// 頂点統合用キー（位置+UVのビット表現で比較、法線は平均化するため含めない）
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct PosUvKey {
    pos: [u32; 3],
    uv: [u32; 2],
}

impl PosUvKey {
    fn new(pos: [f32; 3], uv: [f32; 2]) -> Self {
        Self {
            pos: [pos[0].to_bits(), pos[1].to_bits(), pos[2].to_bits()],
            uv: [uv[0].to_bits(), uv[1].to_bits()],
        }
    }
}

