use bytemuck::{Pod, Zeroable};
use eframe::wgpu;
use glam::Vec3;

use crate::intermediate::types::IrMaterial;

/// Bloom パス用 Uniform
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct BloomParams {
    threshold: f32,
    intensity: f32,
    is_first_pass: f32,
    _pad: f32,
}

/// ダウンサンプルチェーン + composite 出力のテクスチャキャッシュ
struct BloomTextures {
    /// levels[i] = 1/(2^(i+1)) 解像度 (Rgba16Float, linear)
    levels: Vec<(wgpu::Texture, wgpu::TextureView)>,
    /// Composite 出力（メインと同サイズ、Rgba8UnormSrgb）
    _composite: wgpu::Texture,
    composite_view: wgpu::TextureView,
    /// Downsample 入力 bind groups（[1..N] はレベル間、[0] は毎フレーム再作成）
    down_bind_groups_internal: Vec<wgpu::BindGroup>,
    /// Upsample 入力 bind groups
    up_bind_groups: Vec<wgpu::BindGroup>,
    width: u32,
    height: u32,
    /// 構築時の段数（再作成判定用）
    num_levels: usize,
}

/// Dual Kawase Bloom ポストエフェクト
pub struct BloomPass {
    downsample_pipeline: wgpu::RenderPipeline,
    upsample_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    /// texture + sampler BGL (downsample/upsample 入力)
    tex_bgl: wgpu::BindGroupLayout,
    /// composite BGL (scene texture + bloom texture + sampler)
    composite_bgl: wgpu::BindGroupLayout,
    /// params uniform BGL
    #[expect(dead_code)]
    params_bgl: wgpu::BindGroupLayout,
    /// Bright extract 用 params (is_first_pass=1.0)
    params_buf_extract: wgpu::Buffer,
    params_bg_extract: wgpu::BindGroup,
    /// Blur / composite 用 params (is_first_pass=0.0)
    params_buf_blur: wgpu::Buffer,
    params_bg_blur: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    textures: Option<BloomTextures>,
    /// down0 パス用 BindGroup キャッシュ（外部 bloom_input テクスチャ依存）
    cached_down0_bind_group: Option<wgpu::BindGroup>,
    /// composite パス用 BindGroup キャッシュ（外部 scene_view テクスチャ依存）
    cached_composite_bind_group: Option<wgpu::BindGroup>,
}

/// Bloom 中間バッファのフォーマット（linear 空間で blur 演算、HDR 精度確保のため f16）
const BLOOM_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Composite 出力のフォーマット（egui 表示用、offscreen と同じ）
const COMPOSITE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// ダウンサンプル最大段数
const MAX_LEVELS: usize = 6;
/// デフォルト段数
pub const DEFAULT_BLOOM_RADIUS: u32 = 4;

// ---------------------------------------------------------------------------
// WGSL シェーダー
// ---------------------------------------------------------------------------

const BLOOM_SHADER_SRC: &str = r#"
// ---- shared vertex ----
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) vi: u32) -> VsOut {
    // 3 頂点の大三角形で画面全体をカバー
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
// PMX/PMD Bloom 判定
// ---------------------------------------------------------------------------

/// PMX/PMD 材質の Bloom 判定。`(bloom_emissive, bloom_strength)` を返す。
/// `bloom_strength == 0.0` なら非 Bloom 対象。
/// bloom_emissive の各成分は 0.0-1.0 にクランプ（MRT bloom 出力が Rgba8Unorm のため）。
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
// BloomPass 実装
// ---------------------------------------------------------------------------

impl BloomPass {
    pub fn new(device: &wgpu::Device) -> Self {
        // --- BGL: texture + sampler (downsample/upsample 入力) ---
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

        // --- Params buffers (extract 用 / blur 用で分離) ---
        // queue.write_buffer は submit 時に一括実行されるため、同一 encoder 内で
        // 複数回書き込むと最後の値が全パスに適用される。2つの buffer で回避。
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

    /// 外部テクスチャ（offscreen）依存の BindGroup キャッシュを無効化する。
    /// offscreen テクスチャが再作成された時に呼ぶこと。
    pub fn invalidate_external_cache(&mut self) {
        self.cached_down0_bind_group = None;
        self.cached_composite_bind_group = None;
    }

    /// Bloom テクスチャを確保（サイズ・段数変更時に再作成）
    /// Bloom テクスチャを確保（サイズ・段数変更時に再作成）
    /// 外部ビュー（bloom_input / scene_view）に依存する bind group は含めない。
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

        // テクスチャ再作成に伴い外部依存キャッシュも無効化
        self.cached_down0_bind_group = None;
        self.cached_composite_bind_group = None;

        // ダウンサンプルチェーン (Rgba16Float, linear)
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

        // Composite 出力 (Rgba8UnormSrgb, メインと同サイズ)
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

        // --- Bind groups（レベル間のみ、外部ビュー依存は execute() で毎フレーム作成）---
        // Downsample [1..N]: level[i-1] → level[i]（[0] は bloom_input 依存なので除外）
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

        // Upsample: 最深レベルから1段ずつ戻る
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

    /// Bloom パスを実行し、composite 結果の TextureView を返す。
    /// `bloom_input`: MRT の bloom 出力（emissive-only、downsample 入力）
    /// `scene_view`: 元のシーンカラー（composite でブレンドする元画像）
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

        // 外部ビュー依存の bind group をキャッシュから取得、無ければ作成
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

        // Uniform 更新
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

        // --- Composite: scene + level0 * intensity → composite_output ---
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

    /// Composite 結果の TextureView（bloom 有効時に egui に登録するビュー）
    pub fn composite_view(&self) -> Option<&wgpu::TextureView> {
        self.textures.as_ref().map(|t| &t.composite_view)
    }
}

// ---------------------------------------------------------------------------
// テスト
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
        // sp=110, ambient=(1,1,1) → strength=1.0, emissive=(1,1,1)
        let mat = make_mat(Vec3::ZERO, 110.0, Vec3::ONE);
        let (emissive, strength) = derive_pmx_bloom(&mat);
        assert!((strength - 1.0).abs() < 1e-5);
        assert!((emissive[0] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_derive_pmx_bloom_below_threshold() {
        // sp=99 → 非 Bloom
        let mat = make_mat(Vec3::ZERO, 99.0, Vec3::ONE);
        let (_, strength) = derive_pmx_bloom(&mat);
        assert_eq!(strength, 0.0);
    }

    #[test]
    fn test_derive_pmx_bloom_at_100() {
        // sp=100 → strength=0 (ゼロ除算回避)
        let mat = make_mat(Vec3::ZERO, 100.0, Vec3::ONE);
        let (_, strength) = derive_pmx_bloom(&mat);
        assert_eq!(strength, 0.0);
    }

    #[test]
    fn test_derive_pmx_bloom_nonzero_specular() {
        // specular != ZERO → 非 Bloom
        let mat = make_mat(Vec3::new(0.5, 0.0, 0.0), 110.0, Vec3::ONE);
        let (_, strength) = derive_pmx_bloom(&mat);
        assert_eq!(strength, 0.0);
    }

    #[test]
    fn test_derive_pmx_bloom_strong() {
        // sp=120 → strength=2.0, ambient=(0.5,0.3,0.1) → emissive=(1.0,0.6,0.2) クランプ内
        let mat = make_mat(Vec3::ZERO, 120.0, Vec3::new(0.5, 0.3, 0.1));
        let (emissive, strength) = derive_pmx_bloom(&mat);
        assert!((strength - 2.0).abs() < 1e-5);
        assert!((emissive[0] - 1.0).abs() < 1e-5);
        assert!((emissive[1] - 0.6).abs() < 1e-5);
        assert!((emissive[2] - 0.2).abs() < 1e-5);
    }

    #[test]
    fn test_derive_pmx_bloom_clamp() {
        // sp=120, ambient=(1,1,1) → strength=2.0, raw=(2,2,2) → クランプ後 (1,1,1)
        let mat = make_mat(Vec3::ZERO, 120.0, Vec3::ONE);
        let (emissive, strength) = derive_pmx_bloom(&mat);
        assert!((strength - 2.0).abs() < 1e-5);
        assert!((emissive[0] - 1.0).abs() < 1e-5);
        assert!((emissive[1] - 1.0).abs() < 1e-5);
        assert!((emissive[2] - 1.0).abs() < 1e-5);
    }
}
