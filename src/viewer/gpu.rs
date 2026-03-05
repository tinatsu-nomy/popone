use bytemuck::{Pod, Zeroable};
use eframe::{egui_wgpu, wgpu};
use wgpu::util::DeviceExt;

use super::camera::OrbitCamera;
use super::mesh::GpuModel;

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

pub struct GpuRenderer {
    /// メッシュ描画パイプライン（カリングあり）
    pipeline_cull: wgpu::RenderPipeline,
    /// メッシュ描画パイプライン（両面描画）
    pipeline_no_cull: wgpu::RenderPipeline,
    /// グリッドパイプライン
    pipeline_grid: wgpu::RenderPipeline,
    /// カメラ uniform バッファ
    camera_buf: wgpu::Buffer,
    /// カメラ bind group
    camera_bind_group: wgpu::BindGroup,
    /// カメラ bind group layout（将来拡張用）
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
    /// オフスクリーンテクスチャキャッシュ
    offscreen: Option<OffscreenTarget>,
}

#[allow(dead_code)]
struct OffscreenTarget {
    color: wgpu::Texture,
    color_view: wgpu::TextureView,
    depth: wgpu::Texture,
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

        let texture_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        });

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

        let color_target = wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        };

        // Pipeline with back-face culling
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
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(color_target.clone())],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        // Pipeline without culling (double-sided materials)
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
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(color_target.clone())],
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
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
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
            pipeline_grid,
            camera_buf,
            camera_bind_group,
            camera_bgl,
            texture_bgl,
            material_bgl,
            default_tex_bind_group,
            grid_vbuf,
            grid_vertex_count,
            offscreen: None,
        }
    }

    /// テクスチャ bind group layout への参照
    pub fn texture_bgl(&self) -> &wgpu::BindGroupLayout {
        &self.texture_bgl
    }

    /// 材質 bind group layout への参照
    pub fn material_bgl(&self) -> &wgpu::BindGroupLayout {
        &self.material_bgl
    }

    /// オフスクリーンにモデルを描画し、結果テクスチャの egui TextureId を返す
    pub fn render_to_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        egui_renderer: &mut egui_wgpu::Renderer,
        model: &GpuModel,
        camera: &OrbitCamera,
        width: u32,
        height: u32,
        light_intensity: f32,
        ambient_intensity: f32,
        bg_brightness: f32,
        cached_id: &mut Option<eframe::egui::TextureId>,
    ) -> (eframe::egui::TextureId, ()) {
        // オフスクリーンターゲットの再作成（サイズ変更時）
        let _need_recreate = self
            .offscreen
            .as_ref()
            .map(|o| o.width != width || o.height != height)
            .unwrap_or(true);

        // NOTE: offscreen は &self で持っているため、内部可変性が必要
        // ここでは unsafe を避けるため毎フレームごとに必要なら作り直す方法を取る
        // 実際にはこの関数を &mut self で呼ぶべきだが、eframe のコールバック構造の制約上、
        // ここでは一時的なオフスクリーンテクスチャを作成する

        let color_tex = device.create_texture(&wgpu::TextureDescriptor {
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
        let color_view = color_tex.create_view(&Default::default());

        let depth_tex = device.create_texture(&wgpu::TextureDescriptor {
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
        let depth_view = depth_tex.create_view(&Default::default());

        // Update camera uniform
        let aspect = width as f32 / height as f32;
        let cam_uniform = CameraUniform {
            view_proj: camera.view_proj(aspect).to_cols_array_2d(),
            light_dir: camera.light_dir().to_array(),
            light_intensity,
            ambient: [ambient_intensity; 3],
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
                    view: &color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: bg_brightness as f64,
                            g: bg_brightness as f64,
                            b: bg_brightness as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            // グリッド描画
            pass.set_pipeline(&self.pipeline_grid);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_vertex_buffer(0, self.grid_vbuf.slice(..));
            pass.draw(0..self.grid_vertex_count, 0..1);

            // メッシュ描画
            pass.set_vertex_buffer(0, model.vertex_buf.slice(..));
            pass.set_index_buffer(model.index_buf.slice(..), wgpu::IndexFormat::Uint32);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);

            for draw in &model.draws {
                // パイプライン選択
                if draw.double_sided {
                    pass.set_pipeline(&self.pipeline_no_cull);
                } else {
                    pass.set_pipeline(&self.pipeline_cull);
                }
                pass.set_bind_group(0, &self.camera_bind_group, &[]);

                // テクスチャ bind group
                let tex_bg = draw
                    .texture_bind_group
                    .as_ref()
                    .unwrap_or(&self.default_tex_bind_group);
                pass.set_bind_group(1, tex_bg, &[]);

                // 材質 bind group
                pass.set_bind_group(2, &draw.material_bind_group, &[]);

                pass.draw_indexed(
                    draw.index_offset..(draw.index_offset + draw.index_count),
                    0,
                    0..1,
                );
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
            &color_view,
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
