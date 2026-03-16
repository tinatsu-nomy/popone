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
    pub show_normal_map: f32, // 0.0 = 通常, 1.0 = 法線マップ表示
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
    show_normal_map: f32,
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

    // 法線マップ表示: 法線ベクトル → RGB
    if camera.show_normal_map > 0.5 {
        let rgb = (n + vec3<f32>(1.0)) * 0.5;
        return vec4<f32>(rgb, 1.0);
    }

    // Half-Lambert: 裏面にも柔らかく光が回る
    let ndotl = dot(n, camera.light_dir) * 0.5 + 0.5;
    let light = camera.ambient + vec3<f32>(camera.light_intensity) * ndotl;

    let tex_color = textureSample(t_diffuse, s_diffuse, in.uv);
    let color = tex_color * material.diffuse;
    return vec4<f32>(color.rgb * light, color.a);
}
"#;

/// ワイヤーフレームオーバーレイ用シェーダー（黒色で描画）
const WIRE_OVERLAY_SHADER_SRC: &str = r#"
struct CameraUniform {
    view_proj: mat4x4<f32>,
    light_dir: vec3<f32>,
    light_intensity: f32,
    ambient: vec3<f32>,
    show_normal_map: f32,
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
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}
"#;

const GRID_SHADER_SRC: &str = r#"
struct CameraUniform {
    view_proj: mat4x4<f32>,
    light_dir: vec3<f32>,
    light_intensity: f32,
    ambient: vec3<f32>,
    show_normal_map: f32,
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
    /// アニメーション済みボーングローバル行列（glTF空間、None=レストポーズ）
    pub animated_bone_globals: Option<&'a [glam::Mat4]>,
    /// VRM 0.0 かどうか（座標変換用）
    pub is_vrm0: bool,
}

/// 描画モード
#[derive(Clone, Copy, PartialEq)]
pub enum DrawMode {
    Solid,
    Wireframe,
    SolidWireframe,
}

/// ライトモード
#[derive(Clone, Copy, PartialEq)]
pub enum LightMode {
    CameraFollow,
    Fixed,
}

/// サンプル数ごとのパイプラインセット
struct PipelineSet {
    pipeline_cull: wgpu::RenderPipeline,
    pipeline_no_cull: wgpu::RenderPipeline,
    pipeline_wireframe: Option<wgpu::RenderPipeline>,
    /// ワイヤーフレームオーバーレイ（Solid+Wire用、depth bias付き）
    pipeline_wire_overlay: Option<wgpu::RenderPipeline>,
    pipeline_alpha_cull: wgpu::RenderPipeline,
    pipeline_alpha_no_cull: wgpu::RenderPipeline,
    pipeline_grid: wgpu::RenderPipeline,
    pipeline_bone: wgpu::RenderPipeline,
    pipeline_line_overlay: wgpu::RenderPipeline,
}

pub struct GpuRenderer {
    /// MSAA パイプラインセット (sample_count=4)
    pipelines_msaa: PipelineSet,
    /// 非MSAA パイプラインセット (sample_count=1)
    pipelines_no_msaa: PipelineSet,
    /// カメラ uniform バッファ
    camera_buf: wgpu::Buffer,
    /// カメラ bind group
    camera_bind_group: wgpu::BindGroup,
    /// カメラ bind group layout（BindGroup の lifetime 維持に必要）
    #[allow(dead_code)]
    camera_bgl: wgpu::BindGroupLayout,
    /// テクスチャ bind group layout
    texture_bgl: wgpu::BindGroupLayout,
    /// 材質 bind group layout
    material_bgl: wgpu::BindGroupLayout,
    /// デフォルト白テクスチャ bind group
    default_tex_bind_group: wgpu::BindGroup,
    /// 共通テクスチャサンプラー（毎回生成を回避）
    default_sampler: wgpu::Sampler,
    /// グリッド頂点バッファ
    grid_vbuf: wgpu::Buffer,
    grid_vertex_count: u32,
    /// ボーン頂点バッファ（毎フレーム更新）
    bone_buf: Option<wgpu::Buffer>,
    bone_buf_capacity: usize,
    bone_vertex_count: u32,
    /// SpringBone頂点バッファ
    spring_buf: Option<wgpu::Buffer>,
    spring_buf_capacity: usize,
    spring_vertex_count: u32,
    joint_buf: Option<wgpu::Buffer>,
    joint_buf_capacity: usize,
    joint_vertex_count: u32,
    joint_edge_buf: Option<wgpu::Buffer>,
    joint_edge_buf_capacity: usize,
    joint_edge_vertex_count: u32,
    /// 法線表示頂点バッファ
    normal_buf: Option<wgpu::Buffer>,
    normal_buf_capacity: usize,
    normal_vertex_count: u32,
    /// 法線キャッシュ無効フラグ（true = 再生成が必要）
    normal_dirty: bool,
    /// 法線キャッシュ用: 前回の normal_length
    normal_cache_length: f32,
    /// 法線キャッシュ用: 前回の material_visibility
    normal_cache_visibility: Vec<bool>,
    /// オフスクリーンテクスチャキャッシュ
    offscreen: Option<OffscreenTarget>,
    /// 現在の MSAA 有効状態
    current_msaa: bool,
    /// ボーン頂点生成用作業バッファ（毎フレーム Vec 再割り当て回避）
    bone_work: Vec<GridVertex>,
    /// SpringBone頂点生成用作業バッファ
    spring_work: Vec<GridVertex>,
    joint_work: Vec<GridVertex>,
    joint_edge_work: Vec<GridVertex>,
    /// ボーン頂点キャッシュ: 前回のカメラ位置
    bone_cache_eye: Vec3,
    /// ボーン頂点キャッシュ: 前回のボーン不透明度
    bone_cache_opacity: f32,
    /// SpringBone/Joint キャッシュ: 前回のSpringBone不透明度
    spring_cache_opacity: f32,
    /// SpringBone/Joint キャッシュ: 前回のジョイント不透明度
    joint_cache_opacity: f32,
    /// SpringBone/Joint キャッシュ: 前回の align_rigid_rotation
    spring_cache_align: bool,
    /// 前フレームでアニメーションが有効だったか（Some→None 遷移検出用）
    cache_had_anim: bool,
}

/// MSAA サンプル数
const MSAA_SAMPLE_COUNT: u32 = 4;

struct OffscreenTarget {
    _color: wgpu::Texture,
    color_view: wgpu::TextureView,
    _msaa_color: Option<wgpu::Texture>,
    msaa_color_view: Option<wgpu::TextureView>,
    _depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    width: u32,
    height: u32,
    msaa: bool,
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

        let wire_overlay_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wire_overlay_shader"),
            source: wgpu::ShaderSource::Wgsl(WIRE_OVERLAY_SHADER_SRC.into()),
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

        let supports_wireframe = device.features().contains(wgpu::Features::POLYGON_MODE_LINE);
        if !supports_wireframe {
            log::warn!("POLYGON_MODE_LINE 非対応: ワイヤーフレーム無効");
        }

        let pipelines_msaa = Self::create_pipeline_set(
            device, &shader, &grid_shader, &wire_overlay_shader,
            &pipeline_layout, &grid_pipeline_layout,
            MSAA_SAMPLE_COUNT, supports_wireframe,
        );
        let pipelines_no_msaa = Self::create_pipeline_set(
            device, &shader, &grid_shader, &wire_overlay_shader,
            &pipeline_layout, &grid_pipeline_layout,
            1, supports_wireframe,
        );

        // 共通サンプラー（テクスチャ bind group 作成時に使い回す）
        let default_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("default_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            ..Default::default()
        });

        // Grid vertices
        let (grid_verts, grid_vertex_count) = super::grid::build_grid_vertices();
        let grid_vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("grid_vbuf"),
            contents: bytemuck::cast_slice(&grid_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            pipelines_msaa,
            pipelines_no_msaa,
            camera_buf,
            camera_bind_group,
            camera_bgl,
            texture_bgl,
            material_bgl,
            default_tex_bind_group,
            default_sampler,
            bone_buf: None,
            bone_buf_capacity: 0,
            bone_vertex_count: 0,
            spring_buf: None,
            spring_buf_capacity: 0,
            spring_vertex_count: 0,
            joint_buf: None,
            joint_buf_capacity: 0,
            joint_vertex_count: 0,
            joint_edge_buf: None,
            joint_edge_buf_capacity: 0,
            joint_edge_vertex_count: 0,
            normal_buf: None,
            normal_buf_capacity: 0,
            normal_vertex_count: 0,
            normal_dirty: true,
            normal_cache_length: 0.0,
            normal_cache_visibility: Vec::new(),
            grid_vbuf,
            grid_vertex_count,
            offscreen: None,
            current_msaa: true,
            bone_work: Vec::new(),
            spring_work: Vec::new(),
            joint_work: Vec::new(),
            joint_edge_work: Vec::new(),
            bone_cache_eye: Vec3::ZERO,
            bone_cache_opacity: -1.0,
            spring_cache_opacity: -1.0,
            joint_cache_opacity: -1.0,
            spring_cache_align: false,
            cache_had_anim: false,
        }
    }

    /// 可視化バッファのキャッシュを無効化（モデル再読み込み時に呼ぶ）
    pub fn invalidate_visualization_cache(&mut self) {
        self.bone_cache_eye = Vec3::ZERO;
        self.bone_cache_opacity = -1.0;
        self.spring_cache_opacity = -1.0;
        self.joint_cache_opacity = -1.0;
        self.spring_cache_align = false;
        self.cache_had_anim = false;
        self.bone_vertex_count = 0;
        self.spring_vertex_count = 0;
        self.joint_vertex_count = 0;
        self.joint_edge_vertex_count = 0;
        self.normal_dirty = true;
    }

    #[allow(clippy::too_many_arguments)]
    fn create_pipeline_set(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        grid_shader: &wgpu::ShaderModule,
        wire_overlay_shader: &wgpu::ShaderModule,
        pipeline_layout: &wgpu::PipelineLayout,
        grid_pipeline_layout: &wgpu::PipelineLayout,
        sample_count: u32,
        supports_wireframe: bool,
    ) -> PipelineSet {
        let ms = wgpu::MultisampleState { count: sample_count, ..Default::default() };

        let color_target = wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        };
        let depth_write = wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: Default::default(),
            bias: Default::default(),
        };
        let depth_no_write = wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: Default::default(),
            bias: Default::default(),
        };

        let suffix = if sample_count > 1 { "_msaa" } else { "" };

        let pipeline_cull = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("mesh_cull{suffix}")),
            layout: Some(pipeline_layout),
            vertex: wgpu::VertexState { module: shader, entry_point: Some("vs_main"), buffers: &[Vertex::layout()], compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: Some(wgpu::Face::Back), front_face: wgpu::FrontFace::Cw, ..Default::default() },
            depth_stencil: Some(depth_write.clone()), multisample: ms,
            fragment: Some(wgpu::FragmentState { module: shader, entry_point: Some("fs_main"), targets: &[Some(color_target.clone())], compilation_options: Default::default() }),
            multiview: None, cache: None,
        });

        let pipeline_no_cull = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("mesh_no_cull{suffix}")),
            layout: Some(pipeline_layout),
            vertex: wgpu::VertexState { module: shader, entry_point: Some("vs_main"), buffers: &[Vertex::layout()], compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, front_face: wgpu::FrontFace::Cw, ..Default::default() },
            depth_stencil: Some(depth_write.clone()), multisample: ms,
            fragment: Some(wgpu::FragmentState { module: shader, entry_point: Some("fs_main"), targets: &[Some(color_target.clone())], compilation_options: Default::default() }),
            multiview: None, cache: None,
        });

        let pipeline_wireframe = if supports_wireframe {
            Some(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&format!("mesh_wire{suffix}")),
                layout: Some(pipeline_layout),
                vertex: wgpu::VertexState { module: shader, entry_point: Some("vs_main"), buffers: &[Vertex::layout()], compilation_options: Default::default() },
                primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, front_face: wgpu::FrontFace::Cw, polygon_mode: wgpu::PolygonMode::Line, ..Default::default() },
                depth_stencil: Some(depth_write.clone()), multisample: ms,
                fragment: Some(wgpu::FragmentState { module: shader, entry_point: Some("fs_main"), targets: &[Some(color_target.clone())], compilation_options: Default::default() }),
                multiview: None, cache: None,
            }))
        } else {
            None
        };

        // ワイヤーフレームオーバーレイ（Solid+Wire用: depth bias でZファイティング回避）
        let pipeline_wire_overlay = if supports_wireframe {
            let depth_bias = wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(),
                bias: wgpu::DepthBiasState {
                    constant: -2,
                    slope_scale: -1.0,
                    clamp: 0.0,
                },
            };
            // ワイヤーオーバーレイ用カラーターゲット（アルファブレンド）
            let wire_color_target = wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            };
            Some(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&format!("mesh_wire_overlay{suffix}")),
                layout: Some(pipeline_layout),
                vertex: wgpu::VertexState { module: wire_overlay_shader, entry_point: Some("vs_main"), buffers: &[Vertex::layout()], compilation_options: Default::default() },
                primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, front_face: wgpu::FrontFace::Cw, polygon_mode: wgpu::PolygonMode::Line, ..Default::default() },
                depth_stencil: Some(depth_bias), multisample: ms,
                fragment: Some(wgpu::FragmentState { module: wire_overlay_shader, entry_point: Some("fs_main"), targets: &[Some(wire_color_target)], compilation_options: Default::default() }),
                multiview: None, cache: None,
            }))
        } else {
            None
        };

        let pipeline_alpha_cull = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("mesh_alpha_cull{suffix}")),
            layout: Some(pipeline_layout),
            vertex: wgpu::VertexState { module: shader, entry_point: Some("vs_main"), buffers: &[Vertex::layout()], compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: Some(wgpu::Face::Back), front_face: wgpu::FrontFace::Cw, ..Default::default() },
            depth_stencil: Some(depth_no_write.clone()), multisample: ms,
            fragment: Some(wgpu::FragmentState { module: shader, entry_point: Some("fs_main"), targets: &[Some(color_target.clone())], compilation_options: Default::default() }),
            multiview: None, cache: None,
        });

        let pipeline_alpha_no_cull = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("mesh_alpha_no_cull{suffix}")),
            layout: Some(pipeline_layout),
            vertex: wgpu::VertexState { module: shader, entry_point: Some("vs_main"), buffers: &[Vertex::layout()], compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, front_face: wgpu::FrontFace::Cw, ..Default::default() },
            depth_stencil: Some(depth_no_write), multisample: ms,
            fragment: Some(wgpu::FragmentState { module: shader, entry_point: Some("fs_main"), targets: &[Some(color_target.clone())], compilation_options: Default::default() }),
            multiview: None, cache: None,
        });

        let pipeline_grid = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("grid{suffix}")),
            layout: Some(grid_pipeline_layout),
            vertex: wgpu::VertexState { module: grid_shader, entry_point: Some("vs_grid"), buffers: &[GridVertex::layout()], compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::LineList, ..Default::default() },
            depth_stencil: Some(depth_write), multisample: ms,
            fragment: Some(wgpu::FragmentState { module: grid_shader, entry_point: Some("fs_grid"), targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba8UnormSrgb, blend: None, write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
            multiview: None, cache: None,
        });

        let pipeline_bone = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("bone{suffix}")),
            layout: Some(grid_pipeline_layout),
            vertex: wgpu::VertexState { module: grid_shader, entry_point: Some("vs_grid"), buffers: &[GridVertex::layout()], compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: false, depth_compare: wgpu::CompareFunction::Always, stencil: Default::default(), bias: Default::default() }),
            multisample: ms,
            fragment: Some(wgpu::FragmentState { module: grid_shader, entry_point: Some("fs_grid"), targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba8UnormSrgb, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
            multiview: None, cache: None,
        });

        let pipeline_line_overlay = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("line_overlay{suffix}")),
            layout: Some(grid_pipeline_layout),
            vertex: wgpu::VertexState { module: grid_shader, entry_point: Some("vs_grid"), buffers: &[GridVertex::layout()], compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::LineList, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: false, depth_compare: wgpu::CompareFunction::Always, stencil: Default::default(), bias: Default::default() }),
            multisample: ms,
            fragment: Some(wgpu::FragmentState { module: grid_shader, entry_point: Some("fs_grid"), targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba8UnormSrgb, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
            multiview: None, cache: None,
        });

        PipelineSet { pipeline_cull, pipeline_no_cull, pipeline_wireframe, pipeline_wire_overlay, pipeline_alpha_cull, pipeline_alpha_no_cull, pipeline_grid, pipeline_bone, pipeline_line_overlay }
    }

    /// ワイヤーフレーム対応かどうか
    pub fn supports_wireframe(&self) -> bool {
        self.pipelines_msaa.pipeline_wireframe.is_some()
    }

    /// 現在の MSAA 設定に応じたパイプラインセットを取得
    fn pipelines(&self) -> &PipelineSet {
        if self.current_msaa { &self.pipelines_msaa } else { &self.pipelines_no_msaa }
    }

    /// テクスチャ bind group layout への参照
    pub fn texture_bgl(&self) -> &wgpu::BindGroupLayout {
        &self.texture_bgl
    }

    /// 材質 bind group layout への参照
    pub fn material_bgl(&self) -> &wgpu::BindGroupLayout {
        &self.material_bgl
    }

    /// 共通サンプラーへの参照
    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.default_sampler
    }

    /// 法線キャッシュを無効化（モデル変更・法線再計算時に呼ぶ）
    pub fn invalidate_normal_cache(&mut self) {
        self.normal_dirty = true;
        self.normal_cache_visibility.clear();
        self.normal_cache_length = 0.0;
    }

    /// オフスクリーンテクスチャを確保（サイズ変更または MSAA 切り替え時に再作成）
    fn ensure_offscreen(&mut self, device: &wgpu::Device, width: u32, height: u32, msaa: bool) {
        self.current_msaa = msaa;
        let need_recreate = self
            .offscreen
            .as_ref()
            .map(|o| o.width != width || o.height != height || o.msaa != msaa)
            .unwrap_or(true);

        if !need_recreate {
            return;
        }

        let tex_size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        // MSAA カラーテクスチャ（マルチサンプル、描画先）— MSAA 有効時のみ
        let (msaa_tex, msaa_view) = if msaa {
            let t = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("offscreen_msaa_color"),
                size: tex_size,
                mip_level_count: 1,
                sample_count: MSAA_SAMPLE_COUNT,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let v = t.create_view(&Default::default());
            (Some(t), Some(v))
        } else {
            (None, None)
        };

        // リゾルブ先カラーテクスチャ（sample_count=1、egui表示用）
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen_color"),
            size: tex_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let color_view = color.create_view(&Default::default());

        // デプステクスチャ（MSAA 時はマルチサンプル）
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen_depth"),
            size: tex_size,
            mip_level_count: 1,
            sample_count: if msaa { MSAA_SAMPLE_COUNT } else { 1 },
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&Default::default());

        self.offscreen = Some(OffscreenTarget {
            _color: color,
            color_view,
            _msaa_color: msaa_tex,
            msaa_color_view: msaa_view,
            _depth: depth,
            depth_view,
            width,
            height,
            msaa,
        });
    }

    /// 可視化バッファ（ボーン・法線・剛体・ジョイント）の頂点生成と GPU アップロード
    fn prepare_visualization_buffers(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        model: &GpuModel,
        ir: &IrModel,
        params: &RenderParams,
    ) {
        // アニメーション状態遷移を検出（Some→None でレストポーズに戻す必要がある）
        let has_anim = params.animated_bone_globals.is_some();
        let anim_just_cleared = self.cache_had_anim && !has_anim;
        self.cache_had_anim = has_anim;

        // ボーン頂点を更新（変化時のみ）
        if params.display.show_bones && !ir.bones.is_empty() {
            let eye = params.camera.eye();
            let bone_changed = self.bone_vertex_count == 0
                || has_anim
                || anim_just_cleared
                || eye != self.bone_cache_eye
                || params.display.bone_opacity != self.bone_cache_opacity;
            if bone_changed {
                self.bone_cache_eye = eye;
                self.bone_cache_opacity = params.display.bone_opacity;
                let pos_fn: fn(Vec3) -> Vec3 = if ir.source_format.is_vrm0() {
                    crate::convert::coord::gltf_pos_to_pmx_v0
                } else {
                    crate::convert::coord::gltf_pos_to_pmx
                };
                generate_bone_vertices(&mut self.bone_work, ir, pos_fn, params.camera.eye(), params.display.bone_opacity, params.animated_bone_globals);
                self.bone_vertex_count = self.bone_work.len() as u32;
                let data = bytemuck::cast_slice(&self.bone_work);
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
        }

        // 法線表示頂点を更新（入力が変わった時、またはアニメーション中に再生成）
        if params.display.show_normals {
            let length_changed = (params.display.normal_length - self.normal_cache_length).abs() > 1e-6;
            let vis_changed = self.normal_cache_visibility.as_slice() != params.material_visibility;
            let has_animation = model.current_vertices().as_ptr() != model.base_vertices().as_ptr();
            if self.normal_dirty || length_changed || vis_changed || has_animation {
                let verts = generate_normal_vertices(model, params.display.normal_length, params.material_visibility);
                self.normal_vertex_count = verts.len() as u32;
                let data = bytemuck::cast_slice(&verts);
                if data.len() > self.normal_buf_capacity {
                    self.normal_buf = Some(device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("normal_vbuf"),
                            contents: data,
                            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        },
                    ));
                    self.normal_buf_capacity = data.len();
                } else if let Some(ref buf) = self.normal_buf {
                    queue.write_buffer(buf, 0, data);
                }
                self.normal_dirty = false;
                self.normal_cache_length = params.display.normal_length;
                self.normal_cache_visibility = params.material_visibility.to_vec();
            }
        } else {
            if self.normal_vertex_count > 0 {
                self.normal_dirty = true; // 再表示時に再生成するためフラグを立てる
            }
            self.normal_vertex_count = 0;
        }

        // SpringBone頂点を毎フレーム更新
        if !params.display.show_spring_bones || (ir.physics.rigid_bodies.is_empty() && ir.physics.joints.is_empty()) {
            self.spring_vertex_count = 0;
        }
        if params.display.show_spring_bones && (!ir.physics.rigid_bodies.is_empty() || !ir.physics.joints.is_empty()) {
            let spring_changed = self.spring_vertex_count == 0
                || has_anim
                || anim_just_cleared
                || params.display.spring_bone_opacity != self.spring_cache_opacity
                || params.display.align_rigid_rotation != self.spring_cache_align;
            if spring_changed {
                self.spring_cache_opacity = params.display.spring_bone_opacity;
                self.spring_cache_align = params.display.align_rigid_rotation;
                generate_spring_bone_vertices(&mut self.spring_work, ir, params.display.spring_bone_opacity, params.display.align_rigid_rotation, params.animated_bone_globals, params.is_vrm0);
                self.spring_vertex_count = self.spring_work.len() as u32;
                let data = bytemuck::cast_slice(&self.spring_work);
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
        }

        // ジョイント頂点を毎フレーム更新
        if !params.display.show_joints || ir.physics.joints.is_empty() {
            self.joint_vertex_count = 0;
            self.joint_edge_vertex_count = 0;
        }
        if params.display.show_joints && !ir.physics.joints.is_empty() {
            let joint_changed = self.joint_vertex_count == 0
                || has_anim
                || anim_just_cleared
                || params.display.joint_opacity != self.joint_cache_opacity;
            if joint_changed {
                self.joint_cache_opacity = params.display.joint_opacity;
                generate_joint_vertices(&mut self.joint_work, &mut self.joint_edge_work, ir, params.display.joint_opacity, params.animated_bone_globals, params.is_vrm0);
                // 面バッファ（TriangleList）
                self.joint_vertex_count = self.joint_work.len() as u32;
                let data = bytemuck::cast_slice(&self.joint_work);
                if data.len() > self.joint_buf_capacity {
                    self.joint_buf = Some(device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("joint_vbuf"),
                            contents: data,
                            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        },
                    ));
                    self.joint_buf_capacity = data.len();
                } else if let Some(ref buf) = self.joint_buf {
                    queue.write_buffer(buf, 0, data);
                }
                // エッジバッファ（LineList）
                self.joint_edge_vertex_count = self.joint_edge_work.len() as u32;
                let edge_data = bytemuck::cast_slice(&self.joint_edge_work);
                if edge_data.len() > self.joint_edge_buf_capacity {
                    self.joint_edge_buf = Some(device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("joint_edge_vbuf"),
                            contents: edge_data,
                            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        },
                    ));
                    self.joint_edge_buf_capacity = edge_data.len();
                } else if let Some(ref buf) = self.joint_edge_buf {
                    queue.write_buffer(buf, 0, edge_data);
                }
            }
        }
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
        // オフスクリーンテクスチャの確保（サイズ変更または MSAA 切り替え時に再作成）
        self.ensure_offscreen(device, params.width, params.height, params.display.msaa);

        // 可視化バッファの準備（ボーン・法線・剛体・ジョイント）
        self.prepare_visualization_buffers(device, queue, model, ir, params);

        let offscreen = self.offscreen.as_ref().expect("ensure_offscreen で初期化済み");

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
            show_normal_map: if params.display.show_normal_map { 1.0 } else { 0.0 },
        };
        queue.write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(&cam_uniform));

        // Encode
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("offscreen_encoder"),
        });

        {
            let (color_view, resolve_target) = if let Some(ref msaa_view) = offscreen.msaa_color_view {
                (msaa_view, Some(&offscreen.color_view))
            } else {
                (&offscreen.color_view, None)
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("offscreen_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    resolve_target,
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

            let ps = self.pipelines();

            // グリッド描画
            if params.display.show_grid {
                pass.set_pipeline(&ps.pipeline_grid);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.grid_vbuf.slice(..));
                pass.draw(0..self.grid_vertex_count, 0..1);
            }

            // メッシュ描画（空モデルの場合はスキップ）
            if model.draws.is_empty() {
                // メッシュなし — グリッド・ボーン・法線のみ描画
            } else {
            pass.set_vertex_buffer(0, model.vertex_buf.slice(..));
            pass.set_index_buffer(model.index_buf.slice(..), wgpu::IndexFormat::Uint32);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);

            let use_wireframe = params.display.draw_mode == DrawMode::Wireframe
                && ps.pipeline_wireframe.is_some();
            let use_solid_wire = params.display.draw_mode == DrawMode::SolidWireframe
                && ps.pipeline_wire_overlay.is_some();

            // パス1: 不透明材質（デプス書き込みあり）
            for (draw_idx, draw) in model.draws.iter().enumerate() {
                if !params.material_visibility.get(draw_idx).copied().unwrap_or(true) {
                    continue;
                }
                if draw.is_alpha {
                    continue;
                }

                if use_wireframe {
                    pass.set_pipeline(ps.pipeline_wireframe.as_ref().expect("wireframe パイプラインは supports_wireframe チェック済み"));
                } else if draw.double_sided {
                    pass.set_pipeline(&ps.pipeline_no_cull);
                } else {
                    pass.set_pipeline(&ps.pipeline_cull);
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
                    pass.set_pipeline(ps.pipeline_wireframe.as_ref().expect("wireframe パイプラインは supports_wireframe チェック済み"));
                } else if draw.double_sided {
                    pass.set_pipeline(&ps.pipeline_alpha_no_cull);
                } else {
                    pass.set_pipeline(&ps.pipeline_alpha_cull);
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

            // パス3: Solid+Wire オーバーレイ（ソリッド描画の上にワイヤーを重ねる）
            if use_solid_wire {
                let wire_pl = ps.pipeline_wire_overlay.as_ref().expect("wire_overlay パイプラインは supports_wireframe チェック済み");
                pass.set_pipeline(wire_pl);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                for (draw_idx, draw) in model.draws.iter().enumerate() {
                    if !params.material_visibility.get(draw_idx).copied().unwrap_or(true) {
                        continue;
                    }
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
            }
            } // end if !model.draws.is_empty()

            // 描画順: ジョイント → ボーン → 剛体（後が最前面）

            // ジョイント描画（オーバーレイ）
            if params.display.show_joints {
                // 面（TriangleList）
                if self.joint_vertex_count > 0 {
                    if let Some(ref joint_buf) = self.joint_buf {
                        pass.set_pipeline(&ps.pipeline_bone);
                        pass.set_bind_group(0, &self.camera_bind_group, &[]);
                        pass.set_vertex_buffer(0, joint_buf.slice(..));
                        pass.draw(0..self.joint_vertex_count, 0..1);
                    }
                }
                // エッジ（LineList, 1px）
                if self.joint_edge_vertex_count > 0 {
                    if let Some(ref edge_buf) = self.joint_edge_buf {
                        pass.set_pipeline(&ps.pipeline_line_overlay);
                        pass.set_bind_group(0, &self.camera_bind_group, &[]);
                        pass.set_vertex_buffer(0, edge_buf.slice(..));
                        pass.draw(0..self.joint_edge_vertex_count, 0..1);
                    }
                }
            }

            // ボーン描画（1px LineList オーバーレイ）
            if params.display.show_bones && self.bone_vertex_count > 0 {
                if let Some(ref bone_buf) = self.bone_buf {
                    pass.set_pipeline(&ps.pipeline_line_overlay);
                    pass.set_bind_group(0, &self.camera_bind_group, &[]);
                    pass.set_vertex_buffer(0, bone_buf.slice(..));
                    pass.draw(0..self.bone_vertex_count, 0..1);
                }
            }

            // 法線表示（LineList オーバーレイ）
            if params.display.show_normals && self.normal_vertex_count > 0 {
                if let Some(ref normal_buf) = self.normal_buf {
                    pass.set_pipeline(&ps.pipeline_line_overlay);
                    pass.set_bind_group(0, &self.camera_bind_group, &[]);
                    pass.set_vertex_buffer(0, normal_buf.slice(..));
                    pass.draw(0..self.normal_vertex_count, 0..1);
                }
            }

            // 剛体描画（1px LineList オーバーレイ、最前面）
            if params.display.show_spring_bones && self.spring_vertex_count > 0 {
                if let Some(ref spring_buf) = self.spring_buf {
                    pass.set_pipeline(&ps.pipeline_line_overlay);
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
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
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
                resource: wgpu::BindingResource::Sampler(sampler),
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
    out: &mut Vec<GridVertex>,
    ir: &IrModel,
    pos_fn: fn(Vec3) -> Vec3,
    camera_eye: Vec3,
    opacity: f32,
    animated_globals: Option<&[glam::Mat4]>,
) {
    out.clear();
    // ◎△ 形状: 二重円（ボーン位置） + 底辺なし三角形（親→子）（1px LineList）
    // 画面上で一定サイズ: 半径 = カメラ距離 × 定数
    let bone_color = [0.0, 0.0, 1.0, opacity];       // ブルー #0000ff
    let ik_color = [1.0, 0.588, 0.0, opacity];        // オレンジ #ff9600
    let outer_factor = 0.004_f32;  // 画面上の外円サイズ
    let inner_factor = 0.0022_f32; // 画面上の内円サイズ
    let segments = 16u32;

    for (bone_i, bone) in ir.bones.iter().enumerate() {
        let pos = if let Some(globals) = animated_globals {
            if bone_i < globals.len() {
                pos_fn(globals[bone_i].transform_point3(Vec3::ZERO))
            } else {
                pos_fn(bone.position)
            }
        } else {
            pos_fn(bone.position)
        };

        // IKボーン判定（名前に "ＩＫ" を含む）
        let is_ik = bone.name.contains("ＩＫ") || bone.name.contains("IK");
        let color = if is_ik { ik_color } else { bone_color };

        // ◎: 二重円（カメラ向きビルボード、画面上一定サイズ）
        let to_cam_vec = camera_eye - pos;
        let dist = to_cam_vec.length().max(0.1);
        let to_cam = to_cam_vec / dist;
        let (right, up) = billboard_axes(to_cam);
        let outer_radius = dist * outer_factor;
        let inner_radius = dist * inner_factor;

        // 外円
        for i in 0..segments {
            let a0 = std::f32::consts::TAU * i as f32 / segments as f32;
            let a1 = std::f32::consts::TAU * (i + 1) as f32 / segments as f32;
            let p0 = pos + (right * a0.cos() + up * a0.sin()) * outer_radius;
            let p1 = pos + (right * a1.cos() + up * a1.sin()) * outer_radius;
            out.push(GridVertex { position: p0.to_array(), color });
            out.push(GridVertex { position: p1.to_array(), color });
        }
        // 内円
        for i in 0..segments {
            let a0 = std::f32::consts::TAU * i as f32 / segments as f32;
            let a1 = std::f32::consts::TAU * (i + 1) as f32 / segments as f32;
            let p0 = pos + (right * a0.cos() + up * a0.sin()) * inner_radius;
            let p1 = pos + (right * a1.cos() + up * a1.sin()) * inner_radius;
            out.push(GridVertex { position: p0.to_array(), color });
            out.push(GridVertex { position: p1.to_array(), color });
        }

        // △: 親◎を底辺、自分を頂点とする三角形（底辺なし＝2辺のみ）
        if let Some(parent_idx) = bone.parent {
            if parent_idx >= ir.bones.len() {
                continue;
            }
            let parent_pos = if let Some(globals) = animated_globals {
                if parent_idx < globals.len() {
                    pos_fn(globals[parent_idx].transform_point3(Vec3::ZERO))
                } else {
                    pos_fn(ir.bones[parent_idx].position)
                }
            } else {
                pos_fn(ir.bones[parent_idx].position)
            };

            let dir = pos - parent_pos;
            let len = dir.length();
            if len < 0.001 {
                continue;
            }
            let dir_n = dir / len;
            let mid = (parent_pos + pos) * 0.5;
            let to_cam_mid = (camera_eye - mid).normalize_or_zero();
            let side = dir_n.cross(to_cam_mid).normalize_or_zero();
            let side = if side.length_squared() < 0.001 {
                let (r, _) = billboard_axes(to_cam_mid);
                r
            } else {
                side
            };
            let parent_dist = (camera_eye - parent_pos).length().max(0.1);
            let base_half = parent_dist * outer_factor;

            let base_l = parent_pos + side * base_half;
            let base_r = parent_pos - side * base_half;
            let tip = pos;

            // 左辺: base_l → tip
            out.push(GridVertex { position: base_l.to_array(), color });
            out.push(GridVertex { position: tip.to_array(), color });
            // 右辺: base_r → tip
            out.push(GridVertex { position: base_r.to_array(), color });
            out.push(GridVertex { position: tip.to_array(), color });
        }
    }
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
    out: &mut Vec<GridVertex>,
    ir: &IrModel,
    opacity: f32,
    align_rigid_rotation: bool,
    animated_globals: Option<&[glam::Mat4]>,
    is_vrm0: bool,
) {
    use crate::intermediate::types::RigidShape;

    out.clear();
    let collider_color = [1.0, 0.0, 0.0, opacity];   // レッド #ff0000（group=1: コライダー）
    let spring_color = [0.0, 1.0, 0.0, opacity];      // グリーン #00ff00（group!=1: スプリングチェーン）
    let joint_color = [1.0, 1.0, 0.5, opacity * 0.6]; // イエロー #ffff80（ジョイント接続線）

    let segments = 16u32;
    let line_width = 0.0_f32; // 1px描画（draw_ring/draw_line_quad の _width 引数用）

    // ボーンごとのデルタ変換を事前計算（アニメーション有効時）
    let pos_fn: fn(Vec3) -> Vec3 = if is_vrm0 {
        crate::convert::coord::gltf_pos_to_pmx_v0
    } else {
        crate::convert::coord::gltf_pos_to_pmx
    };
    let bone_deltas: Option<Vec<(Vec3, glam::Quat)>> = animated_globals.map(|globals| {
        ir.bones.iter().enumerate().map(|(i, bone)| {
            if i < globals.len() {
                let rest_pos_pmx = pos_fn(bone.position);
                let anim_pos_pmx = pos_fn(globals[i].transform_point3(Vec3::ZERO));
                let pos_delta = anim_pos_pmx - rest_pos_pmx;
                // glTF空間での回転デルタ
                let (_, rest_rot, _) = bone.global_mat.to_scale_rotation_translation();
                let (_, anim_rot, _) = globals[i].to_scale_rotation_translation();
                let delta_rot_gltf = anim_rot * rest_rot.inverse();
                // PMX空間への回転変換（ミラー座標系）
                let delta_rot_pmx = if is_vrm0 {
                    // X-flip: (x, -y, -z, w)
                    glam::Quat::from_xyzw(delta_rot_gltf.x, -delta_rot_gltf.y, -delta_rot_gltf.z, delta_rot_gltf.w)
                } else {
                    // Z-flip: (-x, -y, z, w)
                    glam::Quat::from_xyzw(-delta_rot_gltf.x, -delta_rot_gltf.y, delta_rot_gltf.z, delta_rot_gltf.w)
                };
                (pos_delta, delta_rot_pmx)
            } else {
                (Vec3::ZERO, glam::Quat::IDENTITY)
            }
        }).collect()
    });

    // 剛体の形状を描画
    for rb in &ir.physics.rigid_bodies {
        let color = if rb.group == 1 { collider_color } else { spring_color };

        // PMX Euler → 回転クォータニオン（ZXY: R = Rz * Rx * Ry）
        // PMX/PMD: 回転は常にファイルの値を使用。VRM: align_rigid_rotation 有効時のみ
        let rotation = if ir.source_format.is_pmx_pmd() || align_rigid_rotation { rb.rotation } else { Vec3::ZERO };
        let mut quat = glam::Quat::from_euler(
            glam::EulerRot::ZXY,
            rotation.z,
            rotation.x,
            rotation.y,
        );

        // アニメーション適用: 剛体をボーンに追従させる
        let rb_pos = if let (Some(bone_idx), Some(ref deltas)) = (rb.bone_index, &bone_deltas) {
            if bone_idx < deltas.len() {
                let (pos_delta, rot_delta) = deltas[bone_idx];
                let rest_bone_pmx = pos_fn(ir.bones[bone_idx].position);
                // 剛体のボーンからのオフセットを回転適用
                let offset = rb.position - rest_bone_pmx;
                let rotated_offset = rot_delta * offset;
                quat = rot_delta * quat;
                rest_bone_pmx + pos_delta + rotated_offset
            } else {
                rb.position
            }
        } else {
            rb.position
        };

        match &rb.shape {
            RigidShape::Sphere { radius } => {
                // 8本の経線（Y軸周り45°間隔、大円弧）
                for i in 0..8u32 {
                    let angle = std::f32::consts::FRAC_PI_4 * i as f32;
                    let horiz = Vec3::new(angle.cos(), 0.0, angle.sin());
                    // 経線 = Y軸と水平方向で張る大円
                    draw_ring(out, rb_pos, quat, *radius, Vec3::Y, horiz, segments, line_width, color);
                }
                // 7本の緯線（上から下へ等間隔）
                for i in 1..=7u32 {
                    let lat_angle = std::f32::consts::PI * i as f32 / 8.0;
                    let y = lat_angle.cos() * radius;
                    let r = lat_angle.sin() * radius;
                    let center = rb_pos + quat * Vec3::new(0.0, y, 0.0);
                    draw_ring(out, center, quat, r, Vec3::Z, Vec3::X, segments, line_width, color);
                }
            }
            RigidShape::Capsule { radius, height } => {
                // カプセル: Y軸がボーン方向
                // 高さ = 球体中心間距離（PMX仕様: height = 全長 - 2*radius ではなく球体中心間距離）
                let half_h = height * 0.5;

                // 上端・下端のリング
                let top_offset = quat * Vec3::new(0.0, half_h, 0.0);
                let bot_offset = quat * Vec3::new(0.0, -half_h, 0.0);

                draw_ring(out, rb_pos + top_offset, quat, *radius, Vec3::Z, Vec3::X, segments, line_width, color);
                draw_ring(out, rb_pos + bot_offset, quat, *radius, Vec3::Z, Vec3::X, segments, line_width, color);

                // 8本の接続線（上端→下端）
                for i in 0..8u32 {
                    let angle = std::f32::consts::FRAC_PI_4 * i as f32;
                    let local_offset = Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius);
                    let top = rb_pos + top_offset + quat * local_offset;
                    let bot = rb_pos + bot_offset + quat * local_offset;
                    draw_line_quad(out, top, bot, line_width * 0.5, color);
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
                    let pa = rb_pos + quat * corners[a];
                    let pb = rb_pos + quat * corners[b];
                    draw_line_quad(out, pa, pb, line_width * 0.5, color);
                }
            }
        }
    }

    // ジョイント接続線を描画（アニメーション済み剛体位置を使用）
    for joint in &ir.physics.joints {
        if joint.rigid_a < ir.physics.rigid_bodies.len()
            && joint.rigid_b < ir.physics.rigid_bodies.len()
        {
            let rb_a = &ir.physics.rigid_bodies[joint.rigid_a];
            let rb_b = &ir.physics.rigid_bodies[joint.rigid_b];
            let pos_a = compute_animated_rb_pos(rb_a, ir, &bone_deltas, pos_fn);
            let pos_b = compute_animated_rb_pos(rb_b, ir, &bone_deltas, pos_fn);
            draw_line_quad(out, pos_a, pos_b, line_width * 0.4, joint_color);
        }
    }
}

/// 剛体のアニメーション済み位置を計算（ジョイント描画用）
fn compute_animated_rb_pos(
    rb: &crate::intermediate::types::IrRigidBody,
    ir: &IrModel,
    bone_deltas: &Option<Vec<(Vec3, glam::Quat)>>,
    pos_fn: fn(Vec3) -> Vec3,
) -> Vec3 {
    if let (Some(bone_idx), Some(ref deltas)) = (rb.bone_index, bone_deltas) {
        if bone_idx < deltas.len() {
            let (pos_delta, rot_delta) = deltas[bone_idx];
            let rest_bone_pmx = pos_fn(ir.bones[bone_idx].position);
            let offset = rb.position - rest_bone_pmx;
            let rotated_offset = rot_delta * offset;
            rest_bone_pmx + pos_delta + rotated_offset
        } else {
            rb.position
        }
    } else {
        rb.position
    }
}

/// 1px リングライン（LineList）
#[allow(clippy::too_many_arguments)]
fn draw_ring(
    verts: &mut Vec<GridVertex>,
    center: Vec3,
    quat: glam::Quat,
    radius: f32,
    axis_a: Vec3,
    axis_b: Vec3,
    segments: u32,
    _width: f32,
    color: [f32; 4],
) {
    for i in 0..segments {
        let a0 = std::f32::consts::TAU * i as f32 / segments as f32;
        let a1 = std::f32::consts::TAU * (i + 1) as f32 / segments as f32;

        let local0 = axis_a * a0.cos() * radius + axis_b * a0.sin() * radius;
        let local1 = axis_a * a1.cos() * radius + axis_b * a1.sin() * radius;

        let p0 = center + quat * local0;
        let p1 = center + quat * local1;

        verts.push(GridVertex { position: p0.to_array(), color });
        verts.push(GridVertex { position: p1.to_array(), color });
    }
}

/// 法線表示用ジオメトリを生成（LineList: 頂点→先端の2頂点/法線）
fn generate_normal_vertices(model: &GpuModel, length: f32, material_visibility: &[bool]) -> Vec<GridVertex> {
    use std::collections::HashSet;

    let color = [0.3, 0.6, 1.0, 0.9]; // 青系

    // アニメーション済み頂点があればそちらを使用
    let base_verts = model.current_vertices();
    let indices = model.base_indices();
    let mut visible = vec![false; base_verts.len()];

    for (draw_idx, draw) in model.draws.iter().enumerate() {
        if !material_visibility.get(draw_idx).copied().unwrap_or(true) {
            continue;
        }
        let start = draw.index_offset as usize;
        let end = start + draw.index_count as usize;
        for &idx in &indices[start..end] {
            if (idx as usize) < visible.len() {
                visible[idx as usize] = true;
            }
        }
    }

    // 同一位置・同一法線の重複を除去（位置+法線のビット表現でキー化）
    let mut seen = HashSet::new();
    let mut verts = Vec::new();
    for (i, v) in base_verts.iter().enumerate() {
        if !visible[i] {
            continue;
        }
        let normal = Vec3::from(v.normal);
        if normal.length_squared() < 1e-6 {
            continue;
        }
        // 位置と法線をビットキー化（f32 → u32）
        let key = (
            v.position[0].to_bits(), v.position[1].to_bits(), v.position[2].to_bits(),
            v.normal[0].to_bits(), v.normal[1].to_bits(), v.normal[2].to_bits(),
        );
        if !seen.insert(key) {
            continue;
        }
        let pos = Vec3::from(v.position);
        let tip = pos + normal.normalize() * length;
        verts.push(GridVertex { position: pos.to_array(), color });
        verts.push(GridVertex { position: tip.to_array(), color });
    }

    verts
}

/// 2点間のライン（薄いクアッドで描画）
/// 1px ライン（LineList）
fn draw_line_quad(
    verts: &mut Vec<GridVertex>,
    from: Vec3,
    to: Vec3,
    _half_width: f32,
    color: [f32; 4],
) {
    if (to - from).length_squared() < 1e-6 {
        return;
    }
    verts.push(GridVertex { position: from.to_array(), color });
    verts.push(GridVertex { position: to.to_array(), color });
}

/// ジョイント頂点を生成（オレンジ立方体面 + 黒1pxエッジ、回転反映、アニメーション同期）
fn generate_joint_vertices(
    faces_out: &mut Vec<GridVertex>,
    edges_out: &mut Vec<GridVertex>,
    ir: &IrModel,
    opacity: f32,
    animated_globals: Option<&[glam::Mat4]>,
    is_vrm0: bool,
) {
    faces_out.clear();
    edges_out.clear();

    let orange = [1.0, 1.0, 0.0, opacity]; // イエロー #ffff00
    let black = [0.0, 0.0, 0.0, opacity.min(1.0)];
    let size = 0.18_f32;

    let is_pmx_pmd = ir.source_format.is_pmx_pmd();

    let pos_fn: fn(Vec3) -> Vec3 = if is_vrm0 {
        crate::convert::coord::gltf_pos_to_pmx_v0
    } else {
        crate::convert::coord::gltf_pos_to_pmx
    };

    // アニメーション用ボーンデルタを事前計算
    let bone_deltas: Option<Vec<(Vec3, glam::Quat)>> = animated_globals.map(|globals| {
        ir.bones.iter().enumerate().map(|(i, bone)| {
            if i < globals.len() {
                let rest_pos_pmx = pos_fn(bone.position);
                let anim_pos_pmx = pos_fn(globals[i].transform_point3(Vec3::ZERO));
                let pos_delta = anim_pos_pmx - rest_pos_pmx;
                let (_, rest_rot, _) = bone.global_mat.to_scale_rotation_translation();
                let (_, anim_rot, _) = globals[i].to_scale_rotation_translation();
                let delta_rot_gltf = anim_rot * rest_rot.inverse();
                let delta_rot_pmx = if is_vrm0 {
                    glam::Quat::from_xyzw(delta_rot_gltf.x, -delta_rot_gltf.y, -delta_rot_gltf.z, delta_rot_gltf.w)
                } else {
                    glam::Quat::from_xyzw(-delta_rot_gltf.x, -delta_rot_gltf.y, delta_rot_gltf.z, delta_rot_gltf.w)
                };
                (pos_delta, delta_rot_pmx)
            } else {
                (Vec3::ZERO, glam::Quat::IDENTITY)
            }
        }).collect()
    });

    for joint in &ir.physics.joints {
        if joint.rigid_a >= ir.physics.rigid_bodies.len() {
            continue;
        }

        let rb_a = &ir.physics.rigid_bodies[joint.rigid_a];

        // ジョイント位置（PMX座標）
        // PMX/PMD: joint.position は既にPMX座標。VRM: glTF座標なので pos_fn で変換
        let joint_rest_pos = if is_pmx_pmd { joint.position } else { pos_fn(joint.position) };
        // ジョイント回転（Euler ZXY → Quat）
        let joint_rest_quat = glam::Quat::from_euler(
            glam::EulerRot::ZXY,
            joint.rotation.z,
            joint.rotation.x,
            joint.rotation.y,
        );

        // アニメーション適用: rigid_a のボーンからのオフセットで追従
        let (joint_pos, joint_quat) = if let (Some(bone_idx), Some(ref deltas)) = (rb_a.bone_index, &bone_deltas) {
            if bone_idx < deltas.len() {
                let (pos_delta, rot_delta) = deltas[bone_idx];
                let rest_bone_pmx = pos_fn(ir.bones[bone_idx].position);
                let offset = joint_rest_pos - rest_bone_pmx;
                let rotated_offset = rot_delta * offset;
                let pos = rest_bone_pmx + pos_delta + rotated_offset;
                let quat = rot_delta * joint_rest_quat;
                (pos, quat)
            } else {
                (joint_rest_pos, joint_rest_quat)
            }
        } else {
            (joint_rest_pos, joint_rest_quat)
        };

        // 正立方体の8頂点（ローカル座標）
        let h = size * 0.5;
        let cube_verts = [
            Vec3::new(-h, -h, -h), // 0: 左下手前
            Vec3::new( h, -h, -h), // 1: 右下手前
            Vec3::new( h,  h, -h), // 2: 右上手前
            Vec3::new(-h,  h, -h), // 3: 左上手前
            Vec3::new(-h, -h,  h), // 4: 左下奥
            Vec3::new( h, -h,  h), // 5: 右下奥
            Vec3::new( h,  h,  h), // 6: 右上奥
            Vec3::new(-h,  h,  h), // 7: 左上奥
        ];

        // 回転適用してワールド座標に変換
        let wv: Vec<Vec3> = cube_verts.iter().map(|&c| joint_pos + joint_quat * c).collect();

        // 立方体の6面（各面2三角形、オレンジ塗りつぶし）
        let cube_faces: [[usize; 4]; 6] = [
            [0, 1, 2, 3], // 手前 (-Z)
            [5, 4, 7, 6], // 奥 (+Z)
            [4, 0, 3, 7], // 左 (-X)
            [1, 5, 6, 2], // 右 (+X)
            [3, 2, 6, 7], // 上 (+Y)
            [4, 5, 1, 0], // 下 (-Y)
        ];
        for face in &cube_faces {
            faces_out.push(GridVertex { position: wv[face[0]].to_array(), color: orange });
            faces_out.push(GridVertex { position: wv[face[1]].to_array(), color: orange });
            faces_out.push(GridVertex { position: wv[face[2]].to_array(), color: orange });
            faces_out.push(GridVertex { position: wv[face[0]].to_array(), color: orange });
            faces_out.push(GridVertex { position: wv[face[2]].to_array(), color: orange });
            faces_out.push(GridVertex { position: wv[face[3]].to_array(), color: orange });
        }

        // 黒枠: 12本のエッジを1pxライン（LineList）で描画
        let edges: [[usize; 2]; 12] = [
            [0,1],[1,2],[2,3],[3,0],
            [4,5],[5,6],[6,7],[7,4],
            [0,4],[1,5],[2,6],[3,7],
        ];
        for edge in &edges {
            edges_out.push(GridVertex { position: wv[edge[0]].to_array(), color: black });
            edges_out.push(GridVertex { position: wv[edge[1]].to_array(), color: black });
        }
    }
}
