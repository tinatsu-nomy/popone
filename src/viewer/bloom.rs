use bytemuck::{Pod, Zeroable};
use eframe::wgpu;
use glam::Vec3;

use crate::intermediate::types::IrMaterial;

/// Uniform for the Bloom pass.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct BloomParams {
    threshold: f32,
    intensity: f32,
    is_first_pass: f32,
    _pad: f32,
}

/// Texture cache for the downsample chain plus the composite output.
struct BloomTextures {
    /// levels[i] = 1 / 2^(i+1) resolution (Rgba16Float, linear).
    levels: Vec<(wgpu::Texture, wgpu::TextureView)>,
    /// Composite output (same size as the main view, Rgba8UnormSrgb).
    _composite: wgpu::Texture,
    composite_view: wgpu::TextureView,
    /// Downsample input bind groups ([1..N] are level-to-level; [0] is rebuilt every frame).
    down_bind_groups_internal: Vec<wgpu::BindGroup>,
    /// Upsample input bind groups.
    up_bind_groups: Vec<wgpu::BindGroup>,
    width: u32,
    height: u32,
    /// Number of levels at construction time (used for rebuild decisions).
    num_levels: usize,
}

/// Dual-Kawase Bloom post-effect.
pub struct BloomPass {
    downsample_pipeline: wgpu::RenderPipeline,
    upsample_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    /// texture + sampler BGL (downsample / upsample input)
    tex_bgl: wgpu::BindGroupLayout,
    /// composite BGL (scene texture + bloom texture + sampler)
    composite_bgl: wgpu::BindGroupLayout,
    /// params uniform BGL
    #[expect(dead_code)]
    params_bgl: wgpu::BindGroupLayout,
    /// Params for the bright-extract pass (is_first_pass=1.0).
    params_buf_extract: wgpu::Buffer,
    params_bg_extract: wgpu::BindGroup,
    /// Params for the blur / composite passes (is_first_pass=0.0).
    params_buf_blur: wgpu::Buffer,
    params_bg_blur: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    textures: Option<BloomTextures>,
    /// BindGroup cache for the down0 pass (depends on the external bloom_input texture).
    cached_down0_bind_group: Option<wgpu::BindGroup>,
    /// BindGroup cache for the composite pass (depends on the external scene_view texture).
    cached_composite_bind_group: Option<wgpu::BindGroup>,
}

/// Format of the bloom intermediate buffers (blur runs in linear space; f16 for HDR precision).
const BLOOM_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Format of the composite output (for egui display, same as offscreen).
const COMPOSITE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// Maximum number of downsample levels.
const MAX_LEVELS: usize = 6;
/// Default number of levels.
pub const DEFAULT_BLOOM_RADIUS: u32 = 4;

// ---------------------------------------------------------------------------
// WGSL shader
// ---------------------------------------------------------------------------

const BLOOM_SHADER_SRC: &str = r#"
// ---- shared vertex ----
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) vi: u32) -> VsOut {
    // Single 3-vertex large triangle that covers the whole screen.
    let x = f32((vi << 1u) & 2u);
    let y = f32(vi & 2u);
    var out: VsOut;
    out.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, y);
    return out;
}

// ---- bindings (downsample / upsample) ----
@group(0) @binding(0) var t_input: texture_2d<f32>;
@group(0) @binding(1) var s_input: sampler;

struct BloomParams {
    threshold: f32,
    intensity: f32,
    is_first_pass: f32,
    _pad: f32,
};
@group(1) @binding(0) var<uniform> params: BloomParams;

// ---- threshold filter ----
fn apply_threshold(color: vec3<f32>, threshold: f32) -> vec3<f32> {
    let brightness = max(color.r, max(color.g, color.b));
    let factor = max(brightness - threshold, 0.0) / max(brightness, 0.00001);
    return color * factor;
}

// ---- downsample (Kawase 5-tap) ----
@fragment
fn fs_downsample(in: VsOut) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(t_input));
    let ts = 0.5 / dims; // half-pixel offset in input texel space
    let uv = in.uv;
    let c  = textureSample(t_input, s_input, uv).rgb;
    let tl = textureSample(t_input, s_input, uv + vec2(-ts.x, -ts.y)).rgb;
    let tr = textureSample(t_input, s_input, uv + vec2( ts.x, -ts.y)).rgb;
    let bl = textureSample(t_input, s_input, uv + vec2(-ts.x,  ts.y)).rgb;
    let br = textureSample(t_input, s_input, uv + vec2( ts.x,  ts.y)).rgb;
    var color = (c + tl + tr + bl + br) * 0.2;
    if params.is_first_pass > 0.5 {
        color = apply_threshold(color, params.threshold);
    }
    return vec4(color, 1.0);
}

// ---- upsample (9-tap tent filter) ----
@fragment
fn fs_upsample(in: VsOut) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(t_input));
    let ts = 1.0 / dims;
    let uv = in.uv;
    var color = textureSample(t_input, s_input, uv).rgb * 4.0;
    color += textureSample(t_input, s_input, uv + vec2(-ts.x, -ts.y)).rgb;
    color += textureSample(t_input, s_input, uv + vec2( ts.x, -ts.y)).rgb;
    color += textureSample(t_input, s_input, uv + vec2(-ts.x,  ts.y)).rgb;
    color += textureSample(t_input, s_input, uv + vec2( ts.x,  ts.y)).rgb;
    color += textureSample(t_input, s_input, uv + vec2(-ts.x, 0.0)).rgb * 2.0;
    color += textureSample(t_input, s_input, uv + vec2( ts.x, 0.0)).rgb * 2.0;
    color += textureSample(t_input, s_input, uv + vec2(0.0, -ts.y)).rgb * 2.0;
    color += textureSample(t_input, s_input, uv + vec2(0.0,  ts.y)).rgb * 2.0;
    color /= 16.0;
    return vec4(color, 1.0);
}

// ---- composite (scene + bloom) ----
// group(0) = composite_bgl: scene tex, bloom tex, sampler
@group(0) @binding(0) var t_scene: texture_2d<f32>;
@group(0) @binding(1) var t_bloom: texture_2d<f32>;
@group(0) @binding(2) var s_composite: sampler;

@fragment
fn fs_composite(in: VsOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let scene = textureSample(t_scene, s_composite, uv);
    let bloom = textureSample(t_bloom, s_composite, uv);
    return vec4(scene.rgb + bloom.rgb * params.intensity, scene.a);
}
"#;

// ---------------------------------------------------------------------------
// PMX/PMD bloom heuristic
// ---------------------------------------------------------------------------

/// Bloom heuristic for PMX/PMD materials. Returns `(bloom_emissive, bloom_strength)`.
/// `bloom_strength == 0.0` means the material is not a bloom emitter.
/// Each component of bloom_emissive is clamped to 0.0..=1.0 (the MRT bloom output is Rgba8Unorm).
pub fn derive_pmx_bloom(mat: &IrMaterial) -> ([f32; 3], f32) {
    if mat.specular == Vec3::ZERO && mat.specular_power >= 100.0 {
        let strength = ((mat.specular_power - 100.0) / 10.0).max(0.0);
        if strength > 0.0 {
            let emissive = (mat.ambient * strength).clamp(Vec3::ZERO, Vec3::ONE);
            return (emissive.to_array(), strength);
        }
    }
    ([0.0; 3], 0.0)
}

// ---------------------------------------------------------------------------
// BloomPass implementation
// ---------------------------------------------------------------------------

impl BloomPass {
    pub fn new(device: &wgpu::Device) -> Self {
        // --- BGL: texture + sampler (downsample / upsample input) ---
        let tex_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom_tex_bgl"),
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

        // --- BGL: composite (scene tex + bloom tex + sampler) ---
        let composite_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom_composite_bgl"),
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
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // --- BGL: params uniform ---
        let params_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom_params_bgl"),
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

        // --- Params buffers (separate buffers for extract / blur) ---
        // queue.write_buffer is batched at submit time, so multiple writes to the same
        // buffer within a single encoder all collapse to the last value applied to every
        // pass. Two buffers avoid that.
        let buf_size = std::mem::size_of::<BloomParams>() as u64;
        let params_buf_extract = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bloom_params_extract"),
            size: buf_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let params_bg_extract = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bloom_params_bg_extract"),
            layout: &params_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buf_extract.as_entire_binding(),
            }],
        });
        let params_buf_blur = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bloom_params_blur"),
            size: buf_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let params_bg_blur = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bloom_params_bg_blur"),
            layout: &params_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buf_blur.as_entire_binding(),
            }],
        });

        // --- Sampler (linear, clamp) ---
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("bloom_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        // --- Shader module ---
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bloom_shader"),
            source: wgpu::ShaderSource::Wgsl(BLOOM_SHADER_SRC.into()),
        });

        // --- Pipeline layouts ---
        let down_up_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bloom_down_up_pl"),
            bind_group_layouts: &[&tex_bgl, &params_bgl],
            push_constant_ranges: &[],
        });
        let composite_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bloom_composite_pl"),
            bind_group_layouts: &[&composite_bgl, &params_bgl],
            push_constant_ranges: &[],
        });

        let vertex_state = wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_fullscreen"),
            buffers: &[],
            compilation_options: Default::default(),
        };

        // --- Downsample pipeline (output: Rgba16Float) ---
        let downsample_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bloom_downsample"),
            layout: Some(&down_up_layout),
            vertex: vertex_state.clone(),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_downsample"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: BLOOM_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        // --- Upsample pipeline (output: Rgba16Float, additive blend) ---
        let upsample_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bloom_upsample"),
            layout: Some(&down_up_layout),
            vertex: vertex_state.clone(),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_upsample"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: BLOOM_FORMAT,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            operation: wgpu::BlendOperation::Add,
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                        },
                        alpha: wgpu::BlendComponent {
                            operation: wgpu::BlendOperation::Add,
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        // --- Composite pipeline (output: Rgba8UnormSrgb) ---
        let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bloom_composite"),
            layout: Some(&composite_layout),
            vertex: vertex_state,
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_composite"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COMPOSITE_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        Self {
            downsample_pipeline,
            upsample_pipeline,
            composite_pipeline,
            tex_bgl,
            composite_bgl,
            params_bgl,
            params_buf_extract,
            params_bg_extract,
            params_buf_blur,
            params_bg_blur,
            sampler,
            textures: None,
            cached_down0_bind_group: None,
            cached_composite_bind_group: None,
        }
    }

    /// Invalidate BindGroup caches that depend on external textures (offscreen).
    /// Call this whenever the offscreen texture is recreated.
    pub fn invalidate_external_cache(&mut self) {
        self.cached_down0_bind_group = None;
        self.cached_composite_bind_group = None;
    }

    /// Allocate the bloom textures (rebuilt when size or level count changes).
    /// Bind groups that depend on external views (bloom_input / scene_view) are not included here.
    fn ensure_textures(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        num_levels: usize,
    ) {
        let num_levels = num_levels.clamp(1, MAX_LEVELS);
        if let Some(ref t) = self.textures {
            if t.width == width && t.height == height && t.num_levels == num_levels {
                return;
            }
        }

        // External-dependent caches are also invalidated when textures are recreated.
        self.cached_down0_bind_group = None;
        self.cached_composite_bind_group = None;

        // Downsample chain (Rgba16Float, linear).
        let mut levels: Vec<(wgpu::Texture, wgpu::TextureView)> = Vec::with_capacity(num_levels);
        let mut w = width / 2;
        let mut h = height / 2;
        for i in 0..num_levels {
            w = w.max(1);
            h = h.max(1);
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&format!("bloom_level_{i}")),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: BLOOM_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = tex.create_view(&Default::default());
            levels.push((tex, view));
            w /= 2;
            h /= 2;
        }

        // Composite output (Rgba8UnormSrgb, same size as the main view).
        let composite_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bloom_composite"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COMPOSITE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let composite_view = composite_tex.create_view(&Default::default());

        // --- Bind groups (level-to-level only; external-view-dependent ones are built per frame in execute()) ---
        // Downsample [1..N]: level[i-1] -> level[i] ([0] depends on bloom_input, so it is excluded).
        let down_bind_groups_internal: Vec<wgpu::BindGroup> = (1..num_levels)
            .map(|i| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&format!("bloom_down_bg_{i}")),
                    layout: &self.tex_bgl,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&levels[i - 1].1),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                })
            })
            .collect();

        // Upsample: walk back one level at a time from the deepest one.
        let up_bind_groups: Vec<wgpu::BindGroup> = (0..num_levels.saturating_sub(1))
            .map(|i| {
                let src_idx = num_levels - 1 - i;
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&format!("bloom_up_bg_{i}")),
                    layout: &self.tex_bgl,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&levels[src_idx].1),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                })
            })
            .collect();

        self.textures = Some(BloomTextures {
            levels,
            _composite: composite_tex,
            composite_view,
            down_bind_groups_internal,
            up_bind_groups,
            width,
            height,
            num_levels,
        });
    }

    /// Run the bloom pass and return the composite TextureView.
    /// `bloom_input`: the bloom output of the MRT (emissive-only, downsample input).
    /// `scene_view`: the original scene color (the source image blended in composite).
    #[allow(clippy::too_many_arguments)]
    pub fn execute<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        bloom_input: &wgpu::TextureView,
        scene_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        threshold: f32,
        intensity: f32,
        num_levels: usize,
    ) -> &'a wgpu::TextureView {
        let num_levels = num_levels.clamp(1, MAX_LEVELS);
        self.ensure_textures(device, width, height, num_levels);

        // Reuse the cached external-view-dependent bind groups, or build them on demand.
        if self.cached_down0_bind_group.is_none() {
            self.cached_down0_bind_group =
                Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("bloom_down_bg_0"),
                    layout: &self.tex_bgl,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(bloom_input),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                }));
        }
        if self.cached_composite_bind_group.is_none() {
            let level0_view = &self
                .textures
                .as_ref()
                .expect("already initialized by ensure_textures")
                .levels[0]
                .1;
            self.cached_composite_bind_group =
                Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("bloom_composite_bg"),
                    layout: &self.composite_bgl,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(scene_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(level0_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                }));
        }
        let down0_bind_group = self.cached_down0_bind_group.as_ref().unwrap();
        let composite_bind_group = self.cached_composite_bind_group.as_ref().unwrap();
        let textures = self
            .textures
            .as_ref()
            .expect("already initialized by ensure_textures");

        // Update uniforms.
        let extract_params = BloomParams {
            threshold,
            intensity,
            is_first_pass: 1.0,
            _pad: 0.0,
        };
        let blur_params = BloomParams {
            threshold,
            intensity,
            is_first_pass: 0.0,
            _pad: 0.0,
        };
        queue.write_buffer(
            &self.params_buf_extract,
            0,
            bytemuck::bytes_of(&extract_params),
        );
        queue.write_buffer(&self.params_buf_blur, 0, bytemuck::bytes_of(&blur_params));

        // --- Downsample chain ---
        for i in 0..num_levels {
            let (params_bg, down_bg) = if i == 0 {
                (&self.params_bg_extract, down0_bind_group)
            } else {
                (
                    &self.params_bg_blur,
                    &textures.down_bind_groups_internal[i - 1],
                )
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bloom_down"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &textures.levels[i].1,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            pass.set_pipeline(&self.downsample_pipeline);
            pass.set_bind_group(0, down_bg, &[]);
            pass.set_bind_group(1, params_bg, &[]);
            pass.draw(0..3, 0..1);
        }

        // --- Upsample chain (additive blend onto existing downsample content) ---
        let up_count = num_levels.saturating_sub(1);
        for i in 0..up_count {
            let dst_idx = num_levels - 2 - i;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bloom_up"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &textures.levels[dst_idx].1,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            pass.set_pipeline(&self.upsample_pipeline);
            pass.set_bind_group(0, &textures.up_bind_groups[i], &[]);
            pass.set_bind_group(1, &self.params_bg_blur, &[]);
            pass.draw(0..3, 0..1);
        }

        // --- Composite: scene + level0 * intensity -> composite_output ---
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bloom_composite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &textures.composite_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            pass.set_pipeline(&self.composite_pipeline);
            pass.set_bind_group(0, composite_bind_group, &[]);
            pass.set_bind_group(1, &self.params_bg_blur, &[]);
            pass.draw(0..3, 0..1);
        }

        &self
            .textures
            .as_ref()
            .expect("ensure_textures already called inside execute")
            .composite_view
    }

    /// The composite TextureView (the view registered with egui when bloom is enabled).
    pub fn composite_view(&self) -> Option<&wgpu::TextureView> {
        self.textures.as_ref().map(|t| &t.composite_view)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn make_mat(specular: Vec3, specular_power: f32, ambient: Vec3) -> IrMaterial {
        IrMaterial {
            specular,
            specular_power,
            ambient,
            ..Default::default()
        }
    }

    #[test]
    fn test_derive_pmx_bloom_standard() {
        // sp=110, ambient=(1,1,1) -> strength=1.0, emissive=(1,1,1)
        let mat = make_mat(Vec3::ZERO, 110.0, Vec3::ONE);
        let (emissive, strength) = derive_pmx_bloom(&mat);
        assert!((strength - 1.0).abs() < 1e-5);
        assert!((emissive[0] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_derive_pmx_bloom_below_threshold() {
        // sp=99 -> not a bloom emitter
        let mat = make_mat(Vec3::ZERO, 99.0, Vec3::ONE);
        let (_, strength) = derive_pmx_bloom(&mat);
        assert_eq!(strength, 0.0);
    }

    #[test]
    fn test_derive_pmx_bloom_at_100() {
        // sp=100 -> strength=0 (avoids divide-by-zero edge case)
        let mat = make_mat(Vec3::ZERO, 100.0, Vec3::ONE);
        let (_, strength) = derive_pmx_bloom(&mat);
        assert_eq!(strength, 0.0);
    }

    #[test]
    fn test_derive_pmx_bloom_nonzero_specular() {
        // specular != ZERO -> not a bloom emitter
        let mat = make_mat(Vec3::new(0.5, 0.0, 0.0), 110.0, Vec3::ONE);
        let (_, strength) = derive_pmx_bloom(&mat);
        assert_eq!(strength, 0.0);
    }

    #[test]
    fn test_derive_pmx_bloom_strong() {
        // sp=120 -> strength=2.0, ambient=(0.5,0.3,0.1) -> emissive=(1.0,0.6,0.2) within clamp
        let mat = make_mat(Vec3::ZERO, 120.0, Vec3::new(0.5, 0.3, 0.1));
        let (emissive, strength) = derive_pmx_bloom(&mat);
        assert!((strength - 2.0).abs() < 1e-5);
        assert!((emissive[0] - 1.0).abs() < 1e-5);
        assert!((emissive[1] - 0.6).abs() < 1e-5);
        assert!((emissive[2] - 0.2).abs() < 1e-5);
    }

    #[test]
    fn test_derive_pmx_bloom_clamp() {
        // sp=120, ambient=(1,1,1) -> strength=2.0, raw=(2,2,2) -> clamped to (1,1,1)
        let mat = make_mat(Vec3::ZERO, 120.0, Vec3::ONE);
        let (emissive, strength) = derive_pmx_bloom(&mat);
        assert!((strength - 2.0).abs() < 1e-5);
        assert!((emissive[0] - 1.0).abs() < 1e-5);
        assert!((emissive[1] - 1.0).abs() < 1e-5);
        assert!((emissive[2] - 1.0).abs() < 1e-5);
    }
}
