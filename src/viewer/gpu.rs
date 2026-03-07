use bytemuck::{Pod, Zeroable};
use eframe::{egui_wgpu, wgpu};
use glam::Vec3;
use wgpu::util::DeviceExt;

use super::camera::OrbitCamera;
use crate::intermediate::types::IrModel;
use super::mesh::GpuModel;

/// テクスチャ用 BindGroupLayout を作成（共通定義）
pub fn create_texture_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("texture_bgl"),
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
    })
}

/// カメラ uniform バッファ
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub light_dir: [f32; 3],
    pub light_intensity: f32,
    pub ambient: [f32; 3],
    pub _pad1: f32,
}

/// 材質 uniform バッファ
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct MaterialUniform {
    pub diffuse: [f32; 4],
}

/// 頂点フォーマット
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

impl Vertex {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // position
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // normal
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // uv
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

/// グリッド用頂点
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct GridVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

impl GridVertex {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

const SHADER_SRC: &str = r#"
struct CameraUniform {
    view_proj: mat4x4<f32>,
    light_dir: vec3<f32>,
    light_intensity: f32,
    ambient: vec3<f32>,
};

struct MaterialUniform {
    diffuse: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(1) @binding(0) var t_diffuse: texture_2d<f32>;
@group(1) @binding(1) var s_diffuse: sampler;
@group(2) @binding(0) var<uniform> material: MaterialUniform;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(position, 1.0);
    out.normal = normal;
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    // Half-Lambert: 裏面にも柔らかく光が回る
    let ndotl = dot(n, camera.light_dir) * 0.5 + 0.5;
    let light = camera.ambient + vec3<f32>(camera.light_intensity) * ndotl;

    let tex_color = textureSample(t_diffuse, s_diffuse, in.uv);
    let color = tex_color * material.diffuse;
    return vec4<f32>(color.rgb * light, color.a);
}
"#;

const GRID_SHADER_SRC: &str = r#"
struct CameraUniform {
    view_proj: mat4x4<f32>,
    light_dir: vec3<f32>,
    ambient: vec3<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_grid(
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(position, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_grid(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

/// 描画パラメータ（render_to_texture に渡す設定をまとめた構造体）
pub struct RenderParams<'a> {
    pub camera: &'a OrbitCamera,
    pub width: u32,
    pub height: u32,
    pub material_visibility: &'a [bool],
    pub display: &'a super::app::DisplaySettings,
}

/// 描画モード
#[derive(Clone, Copy, PartialEq)]
pub enum DrawMode {
    Solid,
    Wireframe,
}

/// ライトモード
#[derive(Clone, Copy, PartialEq)]
pub enum LightMode {
    CameraFollow,
    Fixed,
}

pub struct GpuRenderer {
    /// メッシュ描画パイプライン（カリングあり）
    pipeline_cull: wgpu::RenderPipeline,
    /// メッシュ描画パイプライン（両面描画）
    pipeline_no_cull: wgpu::RenderPipeline,
    /// メッシュ描画パイプライン（ワイヤーフレーム・カリングなし）
    pipeline_wireframe: Option<wgpu::RenderPipeline>,
    /// 半透明用パイプライン（カリングあり、デプス書き込みなし）
    pipeline_alpha_cull: wgpu::RenderPipeline,
    /// 半透明用パイプライン（両面、デプス書き込みなし）
    pipeline_alpha_no_cull: wgpu::RenderPipeline,
    /// グリッドパイプライン
    pipeline_grid: wgpu::RenderPipeline,
    /// カメラ uniform バッファ
    camera_buf: wgpu::Buffer,
    /// カメラ bind group
    camera_bind_group: wgpu::BindGroup,
    /// カメラ bind group layout
    #[allow(dead_code)]
    camera_bgl: wgpu::BindGroupLayout,
    /// テクスチャ bind group layout
    texture_bgl: wgpu::BindGroupLayout,
    /// 材質 bind group layout
    material_bgl: wgpu::BindGroupLayout,
    /// デフォルト白テクスチャ bind group
    default_tex_bind_group: wgpu::BindGroup,
    /// グリッド頂点バッファ
    grid_vbuf: wgpu::Buffer,
    grid_vertex_count: u32,
    /// ボーン描画パイプライン（TriangleList, depth always）
    pipeline_bone: wgpu::RenderPipeline,
    /// ボーン頂点バッファ（毎フレーム更新）
    bone_buf: Option<wgpu::Buffer>,
    bone_buf_capacity: usize,
    bone_vertex_count: u32,
    /// SpringBone頂点バッファ
    spring_buf: Option<wgpu::Buffer>,
    spring_buf_capacity: usize,
    spring_vertex_count: u32,
    /// オフスクリーンテクスチャキャッシュ
    offscreen: Option<OffscreenTarget>,
}

struct OffscreenTarget {
    _color: wgpu::Texture,
    color_view: wgpu::TextureView,
    _depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    width: u32,
    height: u32,
}

impl GpuRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, _has_alpha: bool) -> Self {
        // Bind group layouts
        let camera_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let texture_bgl = create_texture_bind_group_layout(device);

        let material_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material_bgl"),
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

        // Camera uniform buffer
        let camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera_uniform"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera_bg"),
            layout: &camera_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buf.as_entire_binding(),
            }],
        });

        // Default white texture
        let default_tex_bind_group = create_white_texture_bind_group(device, queue, &texture_bgl);

        // Shader modules
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mesh_shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
        });

        let grid_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("grid_shader"),
            source: wgpu::ShaderSource::Wgsl(GRID_SHADER_SRC.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh_pipeline_layout"),
            bind_group_layouts: &[&camera_bgl, &texture_bgl, &material_bgl],
            push_constant_ranges: &[],
        });

        let grid_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("grid_pipeline_layout"),
                bind_group_layouts: &[&camera_bgl],
                push_constant_ranges: &[],
            });

        let color_target_opaque = wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        };

        let depth_stencil_write = wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: Default::default(),
            bias: Default::default(),
        };

        let depth_stencil_no_write = wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: Default::default(),
            bias: Default::default(),
        };

        // 不透明: カリングあり
        let pipeline_cull = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mesh_pipeline_cull"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout()],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                front_face: wgpu::FrontFace::Cw,
                ..Default::default()
            },
            depth_stencil: Some(depth_stencil_write.clone()),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(color_target_opaque.clone())],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        // 不透明: 両面
        let pipeline_no_cull = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mesh_pipeline_no_cull"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout()],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                front_face: wgpu::FrontFace::Cw,
                ..Default::default()
            },
            depth_stencil: Some(depth_stencil_write.clone()),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(color_target_opaque.clone())],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        // ワイヤーフレーム（デバイスが対応している場合のみ）
        let pipeline_wireframe = if device.features().contains(wgpu::Features::POLYGON_MODE_LINE) {
            Some(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("mesh_pipeline_wireframe"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Vertex::layout()],
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    front_face: wgpu::FrontFace::Cw,
                    polygon_mode: wgpu::PolygonMode::Line,
                    ..Default::default()
                },
                depth_stencil: Some(depth_stencil_write.clone()),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(color_target_opaque.clone())],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            }))
        } else {
            log::warn!("POLYGON_MODE_LINE 非対応: ワイヤーフレーム無効");
            None
        };

        // 半透明: カリングあり、デプス書き込みなし
        let pipeline_alpha_cull = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mesh_pipeline_alpha_cull"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout()],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                front_face: wgpu::FrontFace::Cw,
                ..Default::default()
            },
            depth_stencil: Some(depth_stencil_no_write.clone()),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(color_target_opaque.clone())],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        // 半透明: 両面、デプス書き込みなし
        let pipeline_alpha_no_cull =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("mesh_pipeline_alpha_no_cull"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Vertex::layout()],
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    front_face: wgpu::FrontFace::Cw,
                    ..Default::default()
                },
                depth_stencil: Some(depth_stencil_no_write),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(color_target_opaque.clone())],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            });

        // Grid pipeline
        let pipeline_grid = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("grid_pipeline"),
            layout: Some(&grid_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &grid_shader,
                entry_point: Some("vs_grid"),
                buffers: &[GridVertex::layout()],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: Some(depth_stencil_write),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &grid_shader,
                entry_point: Some("fs_grid"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        // ボーン描画パイプライン（TriangleList, depth=Always で常に手前に表示）
        let pipeline_bone = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bone_pipeline"),
            layout: Some(&grid_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &grid_shader,
                entry_point: Some("vs_grid"),
                buffers: &[GridVertex::layout()],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &grid_shader,
                entry_point: Some("fs_grid"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        // Grid vertices
        let (grid_verts, grid_vertex_count) = super::grid::build_grid_vertices();
        let grid_vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("grid_vbuf"),
            contents: bytemuck::cast_slice(&grid_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            pipeline_cull,
            pipeline_no_cull,
            pipeline_wireframe,
            pipeline_alpha_cull,
            pipeline_alpha_no_cull,
            pipeline_grid,
            camera_buf,
            camera_bind_group,
            camera_bgl,
            texture_bgl,
            material_bgl,
            default_tex_bind_group,
            pipeline_bone,
            bone_buf: None,
            bone_buf_capacity: 0,
            bone_vertex_count: 0,
            spring_buf: None,
            spring_buf_capacity: 0,
            spring_vertex_count: 0,
            grid_vbuf,
            grid_vertex_count,
            offscreen: None,
        }
    }

    /// ワイヤーフレーム対応かどうか
    pub fn supports_wireframe(&self) -> bool {
        self.pipeline_wireframe.is_some()
    }

    /// テクスチャ bind group layout への参照
    pub fn texture_bgl(&self) -> &wgpu::BindGroupLayout {
        &self.texture_bgl
    }

    /// 材質 bind group layout への参照
    pub fn material_bgl(&self) -> &wgpu::BindGroupLayout {
        &self.material_bgl
    }

    /// オフスクリーンテクスチャを確保（サイズ変更時のみ再作成）
    fn ensure_offscreen(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let need_recreate = self
            .offscreen
            .as_ref()
            .map(|o| o.width != width || o.height != height)
            .unwrap_or(true);

        if !need_recreate {
            return;
        }

        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen_color"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let color_view = color.create_view(&Default::default());

        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen_depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&Default::default());

        self.offscreen = Some(OffscreenTarget {
            _color: color,
            color_view,
            _depth: depth,
            depth_view,
            width,
            height,
        });
    }

    /// オフスクリーンにモデルを描画し、結果テクスチャの egui TextureId を返す
    #[allow(clippy::too_many_arguments)]
    pub fn render_to_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        egui_renderer: &mut egui_wgpu::Renderer,
        model: &GpuModel,
        ir: &IrModel,
        params: &RenderParams,
        cached_id: &mut Option<eframe::egui::TextureId>,
    ) -> (eframe::egui::TextureId, ()) {
        // オフスクリーンテクスチャの確保（サイズ変更時のみ再作成）
        self.ensure_offscreen(device, params.width, params.height);
        let offscreen = self.offscreen.as_ref().unwrap();

        // ボーン頂点を毎フレーム更新（ビルボード）
        if params.display.show_bones && !ir.bones.is_empty() {
            let pos_fn: fn(Vec3) -> Vec3 = if ir.source_format.is_vrm0() {
                crate::convert::coord::gltf_pos_to_pmx_v0
            } else {
                crate::convert::coord::gltf_pos_to_pmx
            };
            let verts = generate_bone_vertices(ir, pos_fn, params.camera.eye(), params.display.bone_opacity);
            self.bone_vertex_count = verts.len() as u32;
            let data = bytemuck::cast_slice(&verts);
            if data.len() > self.bone_buf_capacity {
                self.bone_buf = Some(device.create_buffer_init(
                    &wgpu::util::BufferInitDescriptor {
                        label: Some("bone_vbuf"),
                        contents: data,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    },
                ));
                self.bone_buf_capacity = data.len();
            } else if let Some(ref buf) = self.bone_buf {
                queue.write_buffer(buf, 0, data);
            }
        }

        // SpringBone頂点を毎フレーム更新
        if !params.display.show_spring_bones || (ir.physics.rigid_bodies.is_empty() && ir.physics.joints.is_empty()) {
            self.spring_vertex_count = 0;
        }
        if params.display.show_spring_bones && (!ir.physics.rigid_bodies.is_empty() || !ir.physics.joints.is_empty()) {
            let verts = generate_spring_bone_vertices(ir, params.display.spring_bone_opacity, params.display.align_rigid_rotation);
            self.spring_vertex_count = verts.len() as u32;
            let data = bytemuck::cast_slice(&verts);
            if data.len() > self.spring_buf_capacity {
                self.spring_buf = Some(device.create_buffer_init(
                    &wgpu::util::BufferInitDescriptor {
                        label: Some("spring_vbuf"),
                        contents: data,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    },
                ));
                self.spring_buf_capacity = data.len();
            } else if let Some(ref buf) = self.spring_buf {
                queue.write_buffer(buf, 0, data);
            }
        }

        // Update camera uniform
        let aspect = params.width as f32 / params.height as f32;
        let light_dir = match params.display.light_mode {
            LightMode::CameraFollow => params.camera.camera_following_light_dir(),
            LightMode::Fixed => OrbitCamera::fixed_light_dir(),
        };
        let cam_uniform = CameraUniform {
            view_proj: params.camera.view_proj(aspect).to_cols_array_2d(),
            light_dir: light_dir.to_array(),
            light_intensity: params.display.light_intensity,
            ambient: [params.display.ambient_intensity; 3],
            _pad1: 0.0,
        };
        queue.write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(&cam_uniform));

        // Encode
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("offscreen_encoder"),
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("offscreen_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &offscreen.color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: params.display.bg_brightness as f64,
                            g: params.display.bg_brightness as f64,
                            b: params.display.bg_brightness as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &offscreen.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            // グリッド描画
            if params.display.show_grid {
                pass.set_pipeline(&self.pipeline_grid);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.grid_vbuf.slice(..));
                pass.draw(0..self.grid_vertex_count, 0..1);
            }

            // メッシュ描画
            pass.set_vertex_buffer(0, model.vertex_buf.slice(..));
            pass.set_index_buffer(model.index_buf.slice(..), wgpu::IndexFormat::Uint32);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);

            let use_wireframe = params.display.draw_mode == DrawMode::Wireframe
                && self.pipeline_wireframe.is_some();

            // パス1: 不透明材質（デプス書き込みあり）
            for (draw_idx, draw) in model.draws.iter().enumerate() {
                if !params.material_visibility.get(draw_idx).copied().unwrap_or(true) {
                    continue;
                }
                if draw.is_alpha {
                    continue; // 半透明は後で
                }

                if use_wireframe {
                    pass.set_pipeline(self.pipeline_wireframe.as_ref().unwrap());
                } else if draw.double_sided {
                    pass.set_pipeline(&self.pipeline_no_cull);
                } else {
                    pass.set_pipeline(&self.pipeline_cull);
                }
                pass.set_bind_group(0, &self.camera_bind_group, &[]);

                let tex_bg = draw
                    .texture_bind_group
                    .as_ref()
                    .unwrap_or(&self.default_tex_bind_group);
                pass.set_bind_group(1, tex_bg, &[]);
                pass.set_bind_group(2, &draw.material_bind_group, &[]);

                pass.draw_indexed(
                    draw.index_offset..(draw.index_offset + draw.index_count),
                    0,
                    0..1,
                );
            }

            // パス2: 半透明材質（デプス書き込みなし）
            for (draw_idx, draw) in model.draws.iter().enumerate() {
                if !params.material_visibility.get(draw_idx).copied().unwrap_or(true) {
                    continue;
                }
                if !draw.is_alpha {
                    continue;
                }

                if use_wireframe {
                    pass.set_pipeline(self.pipeline_wireframe.as_ref().unwrap());
                } else if draw.double_sided {
                    pass.set_pipeline(&self.pipeline_alpha_no_cull);
                } else {
                    pass.set_pipeline(&self.pipeline_alpha_cull);
                }
                pass.set_bind_group(0, &self.camera_bind_group, &[]);

                let tex_bg = draw
                    .texture_bind_group
                    .as_ref()
                    .unwrap_or(&self.default_tex_bind_group);
                pass.set_bind_group(1, tex_bg, &[]);
                pass.set_bind_group(2, &draw.material_bind_group, &[]);

                pass.draw_indexed(
                    draw.index_offset..(draw.index_offset + draw.index_count),
                    0,
                    0..1,
                );
            }

            // ボーン描画（メッシュの上にオーバーレイ、depth=Always）
            if params.display.show_bones && self.bone_vertex_count > 0 {
                if let Some(ref bone_buf) = self.bone_buf {
                    pass.set_pipeline(&self.pipeline_bone);
                    pass.set_bind_group(0, &self.camera_bind_group, &[]);
                    pass.set_vertex_buffer(0, bone_buf.slice(..));
                    pass.draw(0..self.bone_vertex_count, 0..1);
                }
            }

            // SpringBone物理描画（オーバーレイ）
            if params.display.show_spring_bones && self.spring_vertex_count > 0 {
                if let Some(ref spring_buf) = self.spring_buf {
                    pass.set_pipeline(&self.pipeline_bone);
                    pass.set_bind_group(0, &self.camera_bind_group, &[]);
                    pass.set_vertex_buffer(0, spring_buf.slice(..));
                    pass.draw(0..self.spring_vertex_count, 0..1);
                }
            }
        }

        queue.submit(std::iter::once(encoder.finish()));

        // 前回のテクスチャを解放
        if let Some(old_id) = cached_id.take() {
            egui_renderer.free_texture(&old_id);
        }

        // egui にテクスチャを登録
        let tex_id = egui_renderer.register_native_texture(
            device,
            &offscreen.color_view,
            wgpu::FilterMode::Linear,
        );
        *cached_id = Some(tex_id);

        (tex_id, ())
    }
}

/// 1x1 白テクスチャ bind group を作成
fn create_white_texture_bind_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
) -> wgpu::BindGroup {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("white_1x1"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &[255u8, 255, 255, 255],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: None,
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    let view = tex.create_view(&Default::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("default_sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("white_tex_bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    })
}

/// テクスチャ bind group を作成（外部から呼ぶ）
pub fn create_texture_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("tex_sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Linear,
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        ..Default::default()
    });

    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("tex_bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    })
}

/// 材質 bind group を作成
pub fn create_material_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    diffuse: [f32; 4],
) -> wgpu::BindGroup {
    let uniform = MaterialUniform { diffuse };
    let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("material_uniform"),
        contents: bytemuck::bytes_of(&uniform),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("material_bg"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buf.as_entire_binding(),
        }],
    })
}

/// ボーン表示用ジオメトリを生成（毎フレーム、カメラ向きビルボード）
/// - ジョイント: カメラ向き円（12角形）
/// - 親→子: カメラ向き三角形（底辺＝親、頂点＝子）
fn generate_bone_vertices(
    ir: &IrModel,
    pos_fn: fn(Vec3) -> Vec3,
    camera_eye: Vec3,
    opacity: f32,
) -> Vec<GridVertex> {
    let joint_color = [1.0, 0.85, 0.1, opacity];
    let bone_color = [0.15, 0.85, 0.3, opacity];
    let joint_radius = 0.35_f32;
    let segments = 12u32;

    let mut verts = Vec::new();

    for bone in &ir.bones {
        let pos = pos_fn(bone.position);

        // --- ジョイント: カメラ向き円 ---
        let to_cam = (camera_eye - pos).normalize_or_zero();
        let (right, up) = billboard_axes(to_cam);

        for i in 0..segments {
            let a0 = std::f32::consts::TAU * i as f32 / segments as f32;
            let a1 = std::f32::consts::TAU * (i + 1) as f32 / segments as f32;
            let p0 = pos + (right * a0.cos() + up * a0.sin()) * joint_radius;
            let p1 = pos + (right * a1.cos() + up * a1.sin()) * joint_radius;
            verts.push(GridVertex { position: pos.to_array(), color: joint_color });
            verts.push(GridVertex { position: p0.to_array(), color: joint_color });
            verts.push(GridVertex { position: p1.to_array(), color: joint_color });
        }

        // --- 親→子: 三角形 ---
        if let Some(parent_idx) = bone.parent {
            if parent_idx >= ir.bones.len() {
                continue;
            }
            let parent_pos = pos_fn(ir.bones[parent_idx].position);
            let dir = pos - parent_pos;
            let len = dir.length();
            if len < 0.001 {
                continue;
            }
            let dir_n = dir / len;
            let mid = (parent_pos + pos) * 0.5;
            let to_cam_mid = (camera_eye - mid).normalize_or_zero();
            let side = dir_n.cross(to_cam_mid).normalize_or_zero();
            // side がゼロになる場合（カメラがボーン方向を向いている）
            let side = if side.length_squared() < 0.001 {
                let (r, _) = billboard_axes(to_cam_mid);
                r
            } else {
                side
            };
            let base_half = (len * 0.10).clamp(0.15, joint_radius);

            let base_l = parent_pos + side * base_half;
            let base_r = parent_pos - side * base_half;
            let tip = pos;

            // 表面
            verts.push(GridVertex { position: base_l.to_array(), color: bone_color });
            verts.push(GridVertex { position: tip.to_array(), color: bone_color });
            verts.push(GridVertex { position: base_r.to_array(), color: bone_color });
            // 裏面（反対からも見えるように）
            verts.push(GridVertex { position: base_r.to_array(), color: bone_color });
            verts.push(GridVertex { position: tip.to_array(), color: bone_color });
            verts.push(GridVertex { position: base_l.to_array(), color: bone_color });
        }
    }

    verts
}

/// カメラ方向からビルボード用の right/up 軸を算出
fn billboard_axes(to_camera: Vec3) -> (Vec3, Vec3) {
    let right = to_camera.cross(Vec3::Y).normalize_or_zero();
    if right.length_squared() < 0.001 {
        // カメラが真上/真下を向いている場合
        let right = to_camera.cross(Vec3::Z).normalize();
        let up = right.cross(to_camera).normalize();
        (right, up)
    } else {
        let up = right.cross(to_camera).normalize();
        (right, up)
    }
}

/// SpringBone物理ビジュアル用ジオメトリを生成
/// - 剛体: ワイヤフレーム風のリング+接続線で形状を表現
/// - ジョイント: 接続する2剛体間の線
fn generate_spring_bone_vertices(
    ir: &IrModel,
    opacity: f32,
    align_rigid_rotation: bool,
) -> Vec<GridVertex> {
    use crate::intermediate::types::RigidShape;

    let collider_color = [0.0, 0.85, 0.9, opacity]; // シアン（group=1: コライダー）
    let spring_color = [0.9, 0.2, 0.85, opacity];   // マゼンタ（group=2: スプリングチェーン）
    let joint_color = [1.0, 0.85, 0.1, opacity * 0.6]; // 黄色（ジョイント接続線）

    let segments = 16u32;
    let line_width = 0.15_f32; // 線の太さ（ワイヤフレーム風の三角ストリップ幅）

    let mut verts = Vec::new();

    // 剛体の形状を描画
    for rb in &ir.physics.rigid_bodies {
        let color = if rb.group == 1 { collider_color } else { spring_color };

        // PMX Euler → 回転クォータニオン（ZXY: R = Rz * Rx * Ry）
        let rotation = if align_rigid_rotation { rb.rotation } else { Vec3::ZERO };
        let quat = glam::Quat::from_euler(
            glam::EulerRot::ZXY,
            rotation.z,
            rotation.x,
            rotation.y,
        );

        match &rb.shape {
            RigidShape::Sphere { radius } => {
                // 3つの大円リング（XY, XZ, YZ平面）
                draw_ring(&mut verts, rb.position, quat, *radius, Vec3::Z, Vec3::X, segments, line_width, color);
                draw_ring(&mut verts, rb.position, quat, *radius, Vec3::Y, Vec3::X, segments, line_width, color);
                draw_ring(&mut verts, rb.position, quat, *radius, Vec3::Z, Vec3::Y, segments, line_width, color);
            }
            RigidShape::Capsule { radius, height } => {
                // カプセル: Y軸がボーン方向
                // 高さ = 球体中心間距離（PMX仕様: height = 全長 - 2*radius ではなく球体中心間距離）
                let half_h = height * 0.5;

                // 上端・下端のリング
                let top_offset = quat * Vec3::new(0.0, half_h, 0.0);
                let bot_offset = quat * Vec3::new(0.0, -half_h, 0.0);

                draw_ring(&mut verts, rb.position + top_offset, quat, *radius, Vec3::Z, Vec3::X, segments, line_width, color);
                draw_ring(&mut verts, rb.position + bot_offset, quat, *radius, Vec3::Z, Vec3::X, segments, line_width, color);

                // 4本の接続線（上端→下端）
                for i in 0..4u32 {
                    let angle = std::f32::consts::FRAC_PI_2 * i as f32;
                    let local_offset = Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius);
                    let top = rb.position + top_offset + quat * local_offset;
                    let bot = rb.position + bot_offset + quat * local_offset;
                    draw_line_quad(&mut verts, top, bot, line_width * 0.5, color);
                }
            }
            RigidShape::Box { size } => {
                // ボックス: 12辺をライン描画
                let hx = size.x * 0.5;
                let hy = size.y * 0.5;
                let hz = size.z * 0.5;
                let corners = [
                    Vec3::new(-hx, -hy, -hz), Vec3::new( hx, -hy, -hz),
                    Vec3::new( hx,  hy, -hz), Vec3::new(-hx,  hy, -hz),
                    Vec3::new(-hx, -hy,  hz), Vec3::new( hx, -hy,  hz),
                    Vec3::new( hx,  hy,  hz), Vec3::new(-hx,  hy,  hz),
                ];
                let edges = [
                    (0,1),(1,2),(2,3),(3,0), // 手前面
                    (4,5),(5,6),(6,7),(7,4), // 奥面
                    (0,4),(1,5),(2,6),(3,7), // 接続
                ];
                for (a, b) in edges {
                    let pa = rb.position + quat * corners[a];
                    let pb = rb.position + quat * corners[b];
                    draw_line_quad(&mut verts, pa, pb, line_width * 0.5, color);
                }
            }
        }
    }

    // ジョイント接続線を描画
    for joint in &ir.physics.joints {
        if joint.rigid_a < ir.physics.rigid_bodies.len()
            && joint.rigid_b < ir.physics.rigid_bodies.len()
        {
            let pos_a = ir.physics.rigid_bodies[joint.rigid_a].position;
            let pos_b = ir.physics.rigid_bodies[joint.rigid_b].position;
            draw_line_quad(&mut verts, pos_a, pos_b, line_width * 0.4, joint_color);
        }
    }

    verts
}

/// ワイヤフレーム風リング（三角形ストリップで薄い帯を描画）
#[allow(clippy::too_many_arguments)]
fn draw_ring(
    verts: &mut Vec<GridVertex>,
    center: Vec3,
    quat: glam::Quat,
    radius: f32,
    axis_a: Vec3, // リング平面の第1軸
    axis_b: Vec3, // リング平面の第2軸
    segments: u32,
    width: f32,
    color: [f32; 4],
) {
    let half_w = width * 0.5;
    // リングの法線方向（帯の厚み方向）
    let normal = axis_a.cross(axis_b).normalize();

    for i in 0..segments {
        let a0 = std::f32::consts::TAU * i as f32 / segments as f32;
        let a1 = std::f32::consts::TAU * (i + 1) as f32 / segments as f32;

        let local0 = axis_a * a0.cos() * radius + axis_b * a0.sin() * radius;
        let local1 = axis_a * a1.cos() * radius + axis_b * a1.sin() * radius;

        let p0 = center + quat * local0;
        let p1 = center + quat * local1;
        let n = quat * normal * half_w;

        // 薄い帯（2三角形のクアッド）
        let p0_inner = p0 - n;
        let p0_outer = p0 + n;
        let p1_inner = p1 - n;
        let p1_outer = p1 + n;

        verts.push(GridVertex { position: p0_inner.to_array(), color });
        verts.push(GridVertex { position: p0_outer.to_array(), color });
        verts.push(GridVertex { position: p1_outer.to_array(), color });

        verts.push(GridVertex { position: p0_inner.to_array(), color });
        verts.push(GridVertex { position: p1_outer.to_array(), color });
        verts.push(GridVertex { position: p1_inner.to_array(), color });
    }
}

/// 2点間のライン（薄いクアッドで描画）
fn draw_line_quad(
    verts: &mut Vec<GridVertex>,
    from: Vec3,
    to: Vec3,
    half_width: f32,
    color: [f32; 4],
) {
    let dir = to - from;
    if dir.length_squared() < 1e-6 {
        return;
    }
    let dir_n = dir.normalize();

    // 線に直交する方向を求める
    let up = if dir_n.cross(Vec3::Y).length_squared() > 0.001 {
        dir_n.cross(Vec3::Y).normalize()
    } else {
        dir_n.cross(Vec3::Z).normalize()
    };

    let offset = up * half_width;

    let a = from - offset;
    let b = from + offset;
    let c = to + offset;
    let d = to - offset;

    verts.push(GridVertex { position: a.to_array(), color });
    verts.push(GridVertex { position: b.to_array(), color });
    verts.push(GridVertex { position: c.to_array(), color });

    verts.push(GridVertex { position: a.to_array(), color });
    verts.push(GridVertex { position: c.to_array(), color });
    verts.push(GridVertex { position: d.to_array(), color });
}
