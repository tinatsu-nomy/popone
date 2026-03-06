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
    /// IrModel グローバル頂点Index → GPU 頂点Index
    global_to_gpu: Vec<u32>,
    /// 座標変換関数の選択（VRM 0.0 / 1.0）
    is_vrm0: bool,
}

impl GpuModel {
    /// バウンディングボックスを計算 (min, max)
    pub fn compute_bbox(&self) -> (Vec3, Vec3) {
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for v in &self.base_vertices {
            let p = Vec3::from(v.position);
            min = min.min(p);
            max = max.max(p);
        }
        (min, max)
    }

    /// モーフウェイトを適用して頂点バッファを更新
    pub fn apply_morphs(
        &self,
        ir: &IrModel,
        weights: &[f32],
        queue: &wgpu::Queue,
    ) {
        let pos_fn: fn(Vec3) -> Vec3 = if self.is_vrm0 {
            gltf_pos_to_pmx_v0
        } else {
            gltf_pos_to_pmx
        };

        let mut vertices = self.base_vertices.clone();

        for (morph_idx, _morph) in ir.morphs.iter().enumerate() {
            let w = weights.get(morph_idx).copied().unwrap_or(0.0);
            if w.abs() < 1e-6 {
                continue;
            }
            self.apply_single_morph(ir, weights, morph_idx, w, pos_fn, &mut vertices);
        }

        queue.write_buffer(&self.vertex_buf, 0, bytemuck::cast_slice(&vertices));
    }

    fn apply_single_morph(
        &self,
        ir: &IrModel,
        weights: &[f32],
        morph_idx: usize,
        weight: f32,
        pos_fn: fn(Vec3) -> Vec3,
        vertices: &mut [Vertex],
    ) {
        match &ir.morphs[morph_idx].kind {
            IrMorphKind::Vertex(voffs) => {
                for &(global_vi, offset) in voffs {
                    if let Some(&gpu_vi) = self.global_to_gpu.get(global_vi) {
                        let gpu_vi = gpu_vi as usize;
                        if gpu_vi < vertices.len() {
                            let transformed = pos_fn(offset);
                            vertices[gpu_vi].position[0] += transformed.x * weight;
                            vertices[gpu_vi].position[1] += transformed.y * weight;
                            vertices[gpu_vi].position[2] += transformed.z * weight;
                        }
                    }
                }
            }
            IrMorphKind::Group(goffs) => {
                for &(sub_idx, sub_weight) in goffs {
                    let effective = weight * sub_weight;
                    if effective.abs() < 1e-6 || sub_idx >= ir.morphs.len() {
                        continue;
                    }
                    self.apply_single_morph(
                        ir, weights, sub_idx, effective, pos_fn, vertices,
                    );
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
) -> Result<GpuModel> {
    let pos_fn = if ir.is_vrm0 {
        gltf_pos_to_pmx_v0
    } else {
        gltf_pos_to_pmx
    };
    let normal_fn = if ir.is_vrm0 {
        gltf_normal_to_pmx_v0
    } else {
        gltf_normal_to_pmx
    };

    let mut all_vertices: Vec<Vertex> = Vec::new();
    let mut all_indices: Vec<u32> = Vec::new();
    let mut draws: Vec<DrawCall> = Vec::new();
    let mut has_alpha = false;

    // グローバル頂点Index → GPU頂点Index マッピング
    // グローバルIndex = メッシュ順に連結した頂点の通し番号
    let total_global_verts: usize = ir.meshes.iter().map(|m| m.vertices.len()).sum();
    let mut global_to_gpu = vec![0u32; total_global_verts];

    // テクスチャアップロード
    let gpu_textures = super::texture::upload_textures(ir, images, device, queue)?;

    let texture_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("texture_bgl_mesh"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
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

        for &mi in mesh_indices {
            let mesh = &ir.meshes[mi];
            let base_vertex = all_vertices.len() as u32;
            let global_offset = mesh_global_offsets[mi];

            // 頂点変換 + マッピング構築
            for (local_vi, v) in mesh.vertices.iter().enumerate() {
                let gpu_vi = all_vertices.len() as u32;
                global_to_gpu[global_offset + local_vi] = gpu_vi;

                let pos = pos_fn(v.position);
                let normal = normal_fn(v.normal);
                all_vertices.push(Vertex {
                    position: pos.to_array(),
                    normal: normal.to_array(),
                    uv: v.uv.to_array(),
                });
            }

            // インデックス
            let mut indices: Vec<u32> = mesh.indices.iter().map(|&i| i + base_vertex).collect();
            flip_face_winding(&mut indices);
            all_indices.extend_from_slice(&indices);
        }

        let index_count = all_indices.len() as u32 - index_offset;

        // テクスチャ bind group
        let tex_bg = mat.texture_index.and_then(|ti| {
            gpu_textures.get(ti).map(|view| {
                gpu::create_texture_bind_group(device, &texture_bgl, view)
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

    // ベース頂点を保存
    let base_vertices = all_vertices.clone();

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

    Ok(GpuModel {
        vertex_buf,
        index_buf,
        draws,
        has_alpha,
        base_vertices,
        global_to_gpu,
        is_vrm0: ir.is_vrm0,
    })
}

