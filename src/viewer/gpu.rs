use bytemuck::{Pod, Zeroable};
use eframe::{egui_wgpu, wgpu};
use glam::Vec3;
use wgpu::util::DeviceExt;

use super::camera::OrbitCamera;
use super::mesh::{DrawCall, GpuModel, MaterialBuildFlags, RenderQueue};
use crate::intermediate::types::{CullMode, IrModel};

/// MMD lighting ambient scale (154/255 ~= 0.604).
const MMD_LIGHT_AMBIENT: f32 = 154.0 / 255.0;
/// Default MMD light intensity (the reference for `mmd_ambient_scale` derivation).
const MMD_DEFAULT_LIGHT_INTENSITY: f32 = 0.7;
/// Outer (normal) radius coefficient for bone display.
const BONE_DISPLAY_RADIUS: f32 = 0.004;
/// Inner (movement bone) radius coefficient for bone display.
const BONE_JOINT_RADIUS: f32 = 0.0022;
/// Number of sphere segments for bone / physics display.
const SPHERE_SEGMENTS: u32 = 16;

/// Convert `bool` to `f32` (for shader uniforms).
#[inline]
fn b2f(b: bool) -> f32 {
    if b {
        1.0
    } else {
        0.0
    }
}

/// Create the material `BindGroupLayout` (shared between gpu.rs and mesh.rs).
pub fn create_material_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("material_bgl"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

/// Create the texture `BindGroupLayout` (shared definition).
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

// `encase::ShaderType` derive macro internally emits a `check` function that
// triggers `dead_code` warnings, so suppress it in a submodule with
// `#![allow(dead_code)]`.
mod encase_uniforms {
    #![allow(dead_code)]

    /// Camera uniform buffer.
    #[derive(Clone, encase::ShaderType)]
    pub struct CameraUniform {
        pub view_proj: glam::Mat4,
        pub light_dir: glam::Vec3,
        pub light_intensity: f32,
        pub ambient: glam::Vec3,
        pub shader_mode: u32, // ShaderOverride as u32
        pub camera_pos: glam::Vec3,
        pub mmd_edge_thickness: f32,
        pub view_row0: glam::Vec3,
        // encase auto-pads vec3 trailing 4 bytes
        pub view_row1: glam::Vec3,
        pub mmd_ambient_scale: f32,
        /// Accumulated time (seconds; for UV animation).
        pub time: f32,
        /// Aspect ratio (width / height) (MToon outline: X is corrected by 1/aspect).
        pub aspect: f32,
        /// Projection matrix [1][1] = 1/tan(halfFov) (for MToon outline distance clamping).
        pub proj_11: f32,
        // encase auto-pads to align next vec3
        /// SH-based GI equalized value: (rawGi(up) + rawGi(down)) / 2 (CPU-precomputed).
        pub gi_equalized: glam::Vec3,
        /// Perspective projection flag (1.0 = perspective, 0.0 = orthographic).
        pub is_perspective: f32,
        /// Camera forward vector (used as view direction in orthographic mode).
        pub camera_forward: glam::Vec3,
        // encase auto-pads vec3 trailing 4 bytes
        /// Light color RGB (linear).
        pub light_color: glam::Vec3,
        // encase auto-pads vec3 trailing 4 bytes
        /// Ambient ground color RGB (linear; for hemisphere ambient interpolation).
        pub ambient_ground: glam::Vec3,
        // encase auto-pads vec3 trailing 4 bytes (struct tail)
    }

    /// Material uniform buffer (includes MToon parameters).
    #[derive(Clone, encase::ShaderType)]
    pub struct MaterialUniform {
        pub diffuse: glam::Vec4,
        pub shade_color: glam::Vec3,
        pub is_mtoon: f32,
        pub shading_toony: f32,
        pub shading_shift: f32,
        pub outline_width: f32,
        pub outline_mode: f32, // 0=none, 1=world, 2=screen
        pub outline_color: glam::Vec4,
        pub outline_lighting_mix: f32,
        pub rim_fresnel_power: f32,
        pub rim_lift: f32,
        pub rim_lighting_mix: f32,
        pub rim_color: glam::Vec3,
        pub has_matcap: f32,
        pub matcap_factor: glam::Vec3,
        pub has_shade_multiply_tex: f32,
        pub has_shading_shift_tex: f32,
        pub shading_shift_tex_scale: f32,
        pub has_rim_multiply_tex: f32,
        pub uv_anim_scroll_x: f32,
        pub uv_anim_scroll_y: f32,
        pub uv_anim_rotation: f32,
        pub has_uv_anim_mask: f32,
        /// `alphaCutoff` for MASK mode (0.0 = disabled).
        pub alpha_cutoff: f32,
        // --- Texture UV parameters (texCoord + KHR_texture_transform) ---
        // Per texture: [tex_coord, offset.x, offset.y, rotation] + [scale.x, scale.y, 0, 0]
        pub base_uv_a: glam::Vec4,
        pub base_uv_b: glam::Vec4,
        pub shade_uv_a: glam::Vec4,
        pub shade_uv_b: glam::Vec4,
        pub shift_uv_a: glam::Vec4,
        pub shift_uv_b: glam::Vec4,
        pub rim_uv_a: glam::Vec4,
        pub rim_uv_b: glam::Vec4,
        pub outline_uv_a: glam::Vec4,
        pub outline_uv_b: glam::Vec4,
        pub uv_mask_uv_a: glam::Vec4,
        pub uv_mask_uv_b: glam::Vec4,
        pub emissive_factor: glam::Vec3,
        pub has_emissive_tex: f32,
        pub emissive_uv_a: glam::Vec4,
        pub emissive_uv_b: glam::Vec4,
        // --- Normal map parameters ---
        pub has_normal_tex: f32,
        pub normal_scale: f32,
        pub gi_equalization_factor: f32,
        /// `outlineWidthTexture` source channel (0.0=R, 1.0=G, 2.0=B).
        pub outline_width_channel: f32,
        pub normal_uv_a: glam::Vec4,
        pub normal_uv_b: glam::Vec4,
        /// `uvAnimationMaskTexture` source channel (0.0=R, 1.0=G, 2.0=B).
        pub uv_anim_mask_channel: f32,
        // encase auto-pads 3 x f32 to align next vec4
        // --- matcapTexture UV parameters (KHR_texture_transform) ---
        pub matcap_uv_a: glam::Vec4,
        pub matcap_uv_b: glam::Vec4,
    }
}
pub use encase_uniforms::{CameraUniform, MaterialUniform};

/// MMD material uniform buffer.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct MmdMaterialUniform {
    pub ambient: [f32; 3],
    pub alpha: f32,
    pub specular: [f32; 3],
    pub specular_power: f32,
    pub diffuse_rgb: [f32; 3],
    pub flags: u32, // bit0=has_sphere, bit1=sphere_add, bit2=has_toon
    pub edge_color: [f32; 4],
    pub edge_size: f32,
    /// PMX/PMD self-emission color (for Bloom; derived in `derive_pmx_bloom`).
    pub bloom_emissive: [f32; 3],
}

/// Vertex format.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    /// TEXCOORD_1 (secondary UV; for MToon auxiliary textures). When UV1 is absent, copies UV0.
    pub uv1: [f32; 2],
    /// Tangent vector (xyz = tangent direction, w = handedness +/- 1).
    pub tangent: [f32; 4],
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
                // uv1
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // tangent
                wgpu::VertexAttribute {
                    offset: 40,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// Common pattern for visualization buffers (GPU buffer + capacity + vertex count).
struct DynamicBuffer {
    buf: Option<wgpu::Buffer>,
    capacity: usize,
    vertex_count: u32,
}

impl DynamicBuffer {
    /// Create an empty `DynamicBuffer`.
    fn new() -> Self {
        Self {
            buf: None,
            capacity: 0,
            vertex_count: 0,
        }
    }

    /// Upload the staging buffer's contents to the GPU.
    /// Allocate a new buffer if capacity is insufficient; otherwise write into the existing buffer.
    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &[u8], label: &str) {
        if data.len() > self.capacity {
            self.buf = Some(
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: data,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                }),
            );
            self.capacity = data.len();
        } else if let Some(ref buf) = self.buf {
            queue.write_buffer(buf, 0, data);
        }
    }
}

/// Grid vertex.
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

/// WGSL common: `CameraUniform` struct definition (shared across all shaders).
macro_rules! wgsl_camera_uniform {
    () => {
        r#"struct CameraUniform {
    view_proj: mat4x4<f32>,
    light_dir: vec3<f32>,
    light_intensity: f32,
    ambient: vec3<f32>,
    shader_mode: u32,
    camera_pos: vec3<f32>,
    mmd_edge_thickness: f32,
    view_row0: vec3<f32>,
    view_row1: vec3<f32>,
    mmd_ambient_scale: f32,
    time: f32,
    aspect: f32,
    proj_11: f32,
    gi_equalized: vec3<f32>,
    is_perspective: f32,
    camera_forward: vec3<f32>,
    light_color: vec3<f32>,
    ambient_ground: vec3<f32>,
};"#
    };
}

/// WGSL common: `MmdMaterialUniform` struct definition (shared across MMD shaders).
macro_rules! wgsl_mmd_material_uniform {
    () => {
        r#"struct MmdMaterialUniform {
    ambient: vec3<f32>,
    alpha: f32,
    specular: vec3<f32>,
    specular_power: f32,
    diffuse_rgb: vec3<f32>,
    flags: u32,
    edge_color: vec4<f32>,
    edge_size: f32,
    bloom_emissive_r: f32,
    bloom_emissive_g: f32,
    bloom_emissive_b: f32,
};

struct MmdFsOutput {
    @location(0) color: vec4<f32>,
    @location(1) bloom: vec4<f32>,
};"#
    };
}

/// WGSL common: `MaterialUniform` struct definition (shared across base shaders).
macro_rules! wgsl_material_uniform {
    () => {
        r#"struct MaterialUniform {
    diffuse: vec4<f32>,
    shade_color: vec3<f32>,
    is_mtoon: f32,
    shading_toony: f32,
    shading_shift: f32,
    outline_width: f32,
    outline_mode: f32,
    outline_color: vec4<f32>,
    outline_lighting_mix: f32,
    rim_fresnel_power: f32,
    rim_lift: f32,
    rim_lighting_mix: f32,
    rim_color: vec3<f32>,
    has_matcap: f32,
    matcap_factor: vec3<f32>,
    has_shade_multiply_tex: f32,
    has_shading_shift_tex: f32,
    shading_shift_tex_scale: f32,
    has_rim_multiply_tex: f32,
    uv_anim_scroll_x: f32,
    uv_anim_scroll_y: f32,
    uv_anim_rotation: f32,
    has_uv_anim_mask: f32,
    alpha_cutoff: f32,
    base_uv_a: vec4<f32>,
    base_uv_b: vec4<f32>,
    shade_uv_a: vec4<f32>,
    shade_uv_b: vec4<f32>,
    shift_uv_a: vec4<f32>,
    shift_uv_b: vec4<f32>,
    rim_uv_a: vec4<f32>,
    rim_uv_b: vec4<f32>,
    outline_uv_a: vec4<f32>,
    outline_uv_b: vec4<f32>,
    uv_mask_uv_a: vec4<f32>,
    uv_mask_uv_b: vec4<f32>,
    emissive_factor: vec3<f32>,
    has_emissive_tex: f32,
    emissive_uv_a: vec4<f32>,
    emissive_uv_b: vec4<f32>,
    has_normal_tex: f32,
    normal_scale: f32,
    gi_equalization_factor: f32,
    outline_width_channel: f32,
    normal_uv_a: vec4<f32>,
    normal_uv_b: vec4<f32>,
    uv_anim_mask_channel: f32,
    matcap_uv_a: vec4<f32>,
    matcap_uv_b: vec4<f32>,
};"#
    };
}

/// WGSL common: MToon texture binding declarations (shared between main / outline shaders).
macro_rules! wgsl_mtoon_bindings {
    () => {
        r#"
@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(1) @binding(0) var t_diffuse: texture_2d<f32>;
@group(1) @binding(1) var s_diffuse: sampler;
@group(2) @binding(0) var<uniform> material: MaterialUniform;
@group(3) @binding(0) var s_matcap: sampler;
@group(3) @binding(1) var t_matcap: texture_2d<f32>;
@group(3) @binding(2) var s_shade_multiply: sampler;
@group(3) @binding(3) var t_shade_multiply: texture_2d<f32>;
@group(3) @binding(4) var s_shading_shift: sampler;
@group(3) @binding(5) var t_shading_shift: texture_2d<f32>;
@group(3) @binding(6) var s_rim_multiply: sampler;
@group(3) @binding(7) var t_rim_multiply: texture_2d<f32>;
@group(3) @binding(8) var s_uv_anim_mask: sampler;
@group(3) @binding(9) var t_uv_anim_mask: texture_2d<f32>;
@group(3) @binding(10) var s_outline_width: sampler;
@group(3) @binding(11) var t_outline_width: texture_2d<f32>;
@group(3) @binding(12) var s_emissive: sampler;
@group(3) @binding(13) var t_emissive: texture_2d<f32>;
@group(3) @binding(14) var s_normal: sampler;
@group(3) @binding(15) var t_normal: texture_2d<f32>;
"#
    };
}

/// WGSL common: MToon helper functions (shared between main / outline shaders).
/// `apply_texture_transform`, `resolve_mtoon_uv`, `apply_uv_anim_core`,
/// `select_channel`, `apply_normal_map`.
macro_rules! wgsl_mtoon_helpers {
    () => {
        r#"
/// Apply KHR_texture_transform (uv_a = [texCoord, offset.x, offset.y, rotation], uv_b = [scale.x, scale.y, 0, 0]).
fn apply_texture_transform(uv: vec2<f32>, uv_a: vec4<f32>, uv_b: vec4<f32>) -> vec2<f32> {
    let offset = vec2<f32>(uv_a.y, uv_a.z);
    let rotation = uv_a.w;
    let scale = vec2<f32>(uv_b.x, uv_b.y);
    // Early return if scale/rotation/offset are all defaults.
    if abs(rotation) < 0.00001 && abs(scale.x - 1.0) < 0.00001 && abs(scale.y - 1.0) < 0.00001
       && abs(offset.x) < 0.00001 && abs(offset.y) < 0.00001 {
        return uv;
    }
    let scaled = uv * scale;
    let c = cos(rotation);
    let s = sin(rotation);
    let rotated = vec2<f32>(scaled.x * c - scaled.y * s, scaled.x * s + scaled.y * c);
    return rotated + offset;
}

/// UV resolution for MToon auxiliary textures: texCoord selection -> KHR_texture_transform.
/// Pass animated UVs for textures subject to UV animation; raw UVs otherwise.
fn resolve_mtoon_uv(uv0: vec2<f32>, uv1: vec2<f32>, uv_a: vec4<f32>, uv_b: vec4<f32>) -> vec2<f32> {
    let base_uv = select(uv0, uv1, u32(uv_a.x) == 1u);
    return apply_texture_transform(base_uv, uv_a, uv_b);
}

/// Body of UV animation (scroll + rotate); the mask value is decided by the caller.
/// UniVRM-compatible order: scroll -> pivot(-0.5) -> rotation -> pivot(+0.5).
/// Note: the VRM spec is rotate -> scroll, but UniVRM implements scroll -> rotate. Compatibility wins.
/// UniVRM: vrmc_materials_mtoon_geometry_uv.hlsl - rotate(uv + translate - pivot) + pivot.
fn apply_uv_anim_core(uv: vec2<f32>, anim_mask: f32) -> vec2<f32> {
    let translate = vec2<f32>(
        camera.time * material.uv_anim_scroll_x,
        camera.time * material.uv_anim_scroll_y,
    ) * anim_mask;

    // Wrap with a 2*PI period to avoid float precision loss during long-running sessions (UniVRM-compliant).
    let tau = 6.28318530718;
    let turns = (camera.time * material.uv_anim_rotation * anim_mask) / tau;
    let angle = fract(turns) * tau;
    let cos_a = cos(angle);
    let sin_a = sin(angle);
    let centered = (uv + translate) - vec2<f32>(0.5);

    return vec2<f32>(
        centered.x * cos_a - centered.y * sin_a,
        centered.x * sin_a + centered.y * cos_a,
    ) + vec2<f32>(0.5);
}

/// Select a channel from a texel (0=R, 1=G, 2=B).
fn select_channel(texel: vec4<f32>, ch: f32) -> f32 {
    if ch < 0.5 {
        return texel.r;
    } else if ch < 1.5 {
        return texel.g;
    }
    return texel.b;
}

/// Build a TBN matrix from the vertex tangent and apply the normal map
/// (per UniVRM `MToon_GetTangentToWorld`).
/// The sign of tangent.w controls the bitangent direction (mirror UV handling).
fn apply_normal_map(base_n: vec3<f32>, tangent: vec4<f32>, normal_uv: vec2<f32>) -> vec3<f32> {
    // Zero-tangent guard: skip the normal map and return the base normal for degenerate tangents.
    if dot(tangent.xyz, tangent.xyz) < 1e-6 {
        return normalize(base_n);
    }
    let normal_sample = textureSample(t_normal, s_normal, normal_uv).xyz * 2.0 - 1.0;
    let n = normalize(base_n);
    let t = normalize(tangent.xyz);
    // UniVRM-compliant: binarize tangent.w to avoid NaN (vrmc_materials_mtoon_utility.hlsl:64).
    let tangent_sign = select(-1.0, 1.0, tangent.w > 0.0);
    let b = normalize(cross(n, t) * tangent_sign);
    let scaled_normal = vec3<f32>(
        normal_sample.x * material.normal_scale,
        normal_sample.y * material.normal_scale,
        normal_sample.z,
    );
    return normalize(t * scaled_normal.x + b * scaled_normal.y + n * scaled_normal.z);
}
"#
    };
}

const SHADER_SRC: &str = concat!(
    wgsl_camera_uniform!(),
    "\n",
    wgsl_material_uniform!(),
    wgsl_mtoon_bindings!(),
    r#"
const PI: f32 = 3.14159265;
const ALPHA_DISCARD_THRESHOLD: f32 = 0.004;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) world_pos: vec3<f32>,
    @location(3) uv1: vec2<f32>,
    @location(4) tangent: vec4<f32>,
};
"#,
    wgsl_mtoon_helpers!(),
    r#"
@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) uv1_in: vec2<f32>,
    @location(4) tangent_in: vec4<f32>,
) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(position, 1.0);
    out.normal = normal;
    out.uv = uv;
    out.world_pos = position;
    out.uv1 = uv1_in;
    out.tangent = tangent_in;
    return out;
}

/// Alpha mode handling (OPAQUE / MASK+A2C / BLEND).
fn apply_alpha_mode(alpha: f32, cutoff: f32) -> f32 {
    if cutoff < -0.75 {
        // OPAQUE: return the texture alpha as is.
        // No effect on VRM OPAQUE materials since texture alpha = 1.0.
        // For PMX/PMD materials, transparency from texture alpha is reflected.
        if alpha <= 0.001 { discard; }
        return alpha;
    }
    if cutoff >= -0.25 {
        // MASK + AlphaToCoverage (per UniVRM vrmc_materials_mtoon_geometry_alpha.hlsl).
        let a2c_alpha = (alpha - cutoff) / max(fwidth(alpha), 1e-5) + 0.5;
        if a2c_alpha < cutoff { discard; }
        return 1.0;
    }
    // BLEND: discard fully transparent pixels (prevents depth pollution).
    if alpha <= 0.001 { discard; }
    return alpha;
}

struct FsOutput {
    @location(0) color: vec4<f32>,
    @location(1) bloom: vec4<f32>,
};

@fragment
fn fs_main(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FsOutput {
    // Backface normal flip for doubleSided materials (UniVRM-compliant: flip before normal mapping).
    let face_sign = select(-1.0, 1.0, is_front);
    var n = normalize(in.normal) * face_sign;

    // --- MToon UV-animation precomputation (also applied to normalTexture per spec) ---
    var anim_uv = in.uv;
    var anim_uv1 = in.uv1;
    if material.is_mtoon > 0.5 {
        let has_uv_anim = material.uv_anim_scroll_x != 0.0
            || material.uv_anim_scroll_y != 0.0
            || material.uv_anim_rotation != 0.0;
        if has_uv_anim {
            let uv_mask_uv = resolve_mtoon_uv(in.uv, in.uv1, material.uv_mask_uv_a, material.uv_mask_uv_b);
            var anim_mask = 1.0;
            if material.has_uv_anim_mask > 0.5 {
                anim_mask = select_channel(textureSample(t_uv_anim_mask, s_uv_anim_mask, uv_mask_uv), material.uv_anim_mask_channel);
            }
            anim_uv = apply_uv_anim_core(in.uv, anim_mask);
            anim_uv1 = apply_uv_anim_core(in.uv1, anim_mask);
        }
    }

    // Normal-map application (MToon: animated UV, non-MToon: raw UV).
    if material.has_normal_tex > 0.5 {
        let normal_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.normal_uv_a, material.normal_uv_b);
        n = apply_normal_map(n, in.tangent, normal_uv);
    }

    // === Shader override ===
    // Preview modes use the texture alpha as is (transparency is reflected even for PMX/PMD OPAQUE materials).
    if camera.shader_mode == 1u {
        // Normal: geometric normal -> RGB.
        let base_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.base_uv_a, material.base_uv_b);
        let raw_alpha = textureSample(t_diffuse, s_diffuse, base_uv).a * material.diffuse.a;
        if raw_alpha <= 0.001 { discard; }
        let vis_n = normalize(in.normal) * face_sign;
        var out_n: FsOutput;
        out_n.color = vec4<f32>(vis_n * 0.5 + vec3<f32>(0.5), raw_alpha);
        out_n.bloom = vec4<f32>(0.0);
        return out_n;
    }
    if camera.shader_mode == 2u {
        // Unlit: texture color only, no lighting.
        let base_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.base_uv_a, material.base_uv_b);
        let tex = textureSample(t_diffuse, s_diffuse, base_uv);
        let c = tex * material.diffuse;
        if c.a <= 0.001 { discard; }
        var out_u: FsOutput;
        out_u.color = vec4<f32>(c.rgb, c.a);
        out_u.bloom = vec4<f32>(0.0);
        return out_u;
    }
    if camera.shader_mode == 3u {
        // GGX Preview: simplified Cook-Torrance specular.
        let base_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.base_uv_a, material.base_uv_b);
        let tex = textureSample(t_diffuse, s_diffuse, base_uv);
        let base_color = tex * material.diffuse;
        if base_color.a <= 0.001 { discard; }
        let out_a = base_color.a;

        const METALLIC: f32 = 0.0;
        const ROUGHNESS: f32 = 0.8;

        // View direction
        var v: vec3<f32>;
        if camera.is_perspective > 0.5 {
            v = normalize(camera.camera_pos - in.world_pos);
        } else {
            v = -normalize(camera.camera_forward);
        }
        let l = -camera.light_dir;
        let h = normalize(v + l);
        let n_dot_l = max(dot(n, l), 0.0);
        let n_dot_v = max(dot(n, v), 0.001);
        let n_dot_h = max(dot(n, h), 0.0);

        // Schlick Fresnel
        let f0 = mix(vec3<f32>(0.04), base_color.rgb, METALLIC);
        let f = f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - max(dot(h, v), 0.0), 5.0);

        // GGX NDF
        let a = ROUGHNESS * ROUGHNESS;
        let a2 = a * a;
        let d_denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
        let d = a2 / (PI * d_denom * d_denom);

        // Smith GGX geometry
        let k = (ROUGHNESS + 1.0) * (ROUGHNESS + 1.0) / 8.0;
        let g1_v = n_dot_v / (n_dot_v * (1.0 - k) + k);
        let g1_l = n_dot_l / (n_dot_l * (1.0 - k) + k);
        let g = g1_v * g1_l;

        let specular = (d * f * g) / (4.0 * n_dot_v * n_dot_l + 0.001);
        let diffuse_brdf = (vec3<f32>(1.0) - f) * (1.0 - METALLIC) * base_color.rgb / PI;

        let direct = (diffuse_brdf + specular) * camera.light_intensity * camera.light_color * n_dot_l;

        // Hemisphere ambient.
        let hemi_t = n.y * 0.5 + 0.5;
        let ambient = mix(camera.ambient_ground, camera.ambient, vec3<f32>(hemi_t));
        let indirect = base_color.rgb * ambient;

        var out_g: FsOutput;
        out_g.color = vec4<f32>(direct + indirect, out_a);
        out_g.bloom = vec4<f32>(0.0);
        return out_g;
    }

    var lit: vec3<f32>;
    var out_alpha: f32 = 1.0;
    var bloom_color: vec3<f32> = vec3<f32>(0.0);
    if material.is_mtoon > 0.5 {

        // Texture sampling (apply UV animation + texCoord/KHR_texture_transform).
        let base_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.base_uv_a, material.base_uv_b);
        let tex_color = textureSample(t_diffuse, s_diffuse, base_uv);
        let base_color = tex_color * material.diffuse;
        out_alpha = base_color.a;

        // dot(N,L) - spec-compliant: [-1, 1] range (not half-lambert).
        // `camera.light_dir` is the light direction of travel (source -> surface), so invert to (surface -> source).
        let dot_nl = dot(n, -camera.light_dir);

        // shadeMultiplyTexture (subject to UV animation).
        var shade_mul = vec3<f32>(1.0);
        if material.has_shade_multiply_tex > 0.5 {
            let shade_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.shade_uv_a, material.shade_uv_b);
            shade_mul = textureSample(t_shade_multiply, s_shade_multiply, shade_uv).rgb;
        }
        let shade = material.shade_color * shade_mul;

        // shadingShiftTexture (subject to UV animation; UniVRM-compliant).
        var shading = dot_nl + material.shading_shift;
        if material.has_shading_shift_tex > 0.5 {
            let shift_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.shift_uv_a, material.shift_uv_b);
            let shift_tex = textureSample(t_shading_shift, s_shading_shift, shift_uv).r;
            shading += shift_tex * material.shading_shift_tex_scale;
        }

        // MToon 2-tone toon: interpolate lit/shade with linearstep (spec-compliant).
        let edge0 = -1.0 + material.shading_toony;
        let edge1 = 1.0 - material.shading_toony;
        let t = clamp((shading - edge0) / max(edge1 - edge0, 0.001), 0.0, 1.0);
        lit = mix(shade, base_color.rgb, t);

        // Lighting: separate direct from GI (indirect) (UniVRM-compliant).
        // direct = toon_color * directLightColor (ForwardBase: shadow=1).
        // indirect = litColor * lerp(passthroughGi, uniformedGi, giEqualizationFactor).
        // Hemisphere ambient: interpolate sky/ground by the final normal's Y component (SH approximation).
        let hemi_t = n.y * 0.5 + 0.5;
        let raw_indirect = mix(camera.ambient_ground, camera.ambient, vec3<f32>(hemi_t));
        let gi = mix(raw_indirect, camera.gi_equalized, material.gi_equalization_factor);
        let direct_light = camera.light_intensity * camera.light_color;
        let lighting = lit * direct_light + lit * gi;

        // --- Rim lighting + MatCap ---
        // Perspective: camera_pos -> world_pos. Orthographic: camera_forward (UniVRM-compliant).
        var v: vec3<f32>;
        if camera.is_perspective > 0.5 {
            v = normalize(camera.camera_pos - in.world_pos);
        } else {
            v = normalize(camera.camera_forward);
        }
        var rim = vec3<f32>(0.0);

        // MatCap: derive UV from view-space normal (not subject to UV animation).
        // UniVRM-compliant: right = cross(viewDir, worldUp), up = cross(right, viewDir).
        // KHR_texture_transform is applied to the final matcap UV.
        if material.has_matcap > 0.5 {
            let world_view_x = normalize(vec3<f32>(-v.z, 0.0, v.x));
            let world_view_y = cross(world_view_x, v);
            let raw_matcap_uv = vec2<f32>(dot(world_view_x, n), dot(world_view_y, n)) * 0.495 + 0.5;
            let matcap_uv = apply_texture_transform(raw_matcap_uv, material.matcap_uv_a, material.matcap_uv_b);
            rim = material.matcap_factor * textureSample(t_matcap, s_matcap, matcap_uv).rgb;
        }

        // Parametric rim: Fresnel effect.
        let ndotv = dot(n, v);
        let parametric_rim = pow(
            saturate(1.0 - ndotv + material.rim_lift),
            max(material.rim_fresnel_power, 0.00001)
        );
        rim = rim + parametric_rim * material.rim_color;

        // rimMultiplyTexture (subject to UV animation).
        if material.has_rim_multiply_tex > 0.5 {
            let rim_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.rim_uv_a, material.rim_uv_b);
            rim *= textureSample(t_rim_multiply, s_rim_multiply, rim_uv).rgb;
        }

        // Rim lighting mix (VRM 1.0 spec: rim * lerp(white, lighting, mix)).
        // UniVRM-compliant: use the raw (non-equalized) indirect for rim (GI equalization not applied).
        let rim_light_factor = direct_light + raw_indirect;
        let rim_lit = rim * mix(vec3<f32>(1.0), rim_light_factor, material.rim_lighting_mix);

        // emissive (glTF standard + MToon spec: baseCol = lighting + emissive + rim).
        var emissive = material.emissive_factor;
        if material.has_emissive_tex > 0.5 {
            let emissive_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.emissive_uv_a, material.emissive_uv_b);
            emissive *= textureSample(t_emissive, s_emissive, emissive_uv).rgb;
        }

        bloom_color = emissive;
        lit = lighting + rim_lit + emissive;
    } else {
        // Non-MToon: existing Half-Lambert (texCoord + KHR_texture_transform applied).
        let half_lambert = dot(n, -camera.light_dir) * 0.5 + 0.5;
        let base_uv = resolve_mtoon_uv(in.uv, in.uv1, material.base_uv_a, material.base_uv_b);
        let tex_color = textureSample(t_diffuse, s_diffuse, base_uv);
        let base_color = tex_color * material.diffuse;
        let hemi_t_hl = n.y * 0.5 + 0.5;
        let hemi_ambient = mix(camera.ambient_ground, camera.ambient, vec3<f32>(hemi_t_hl));
        let light = hemi_ambient + camera.light_intensity * camera.light_color * half_lambert;
        lit = base_color.rgb * light;
        out_alpha = base_color.a;

        // Apply emissive even for non-MToon as per glTF standard (texCoord + KHR_texture_transform applied).
        var emissive = material.emissive_factor;
        if material.has_emissive_tex > 0.5 {
            let emissive_uv = resolve_mtoon_uv(in.uv, in.uv1, material.emissive_uv_a, material.emissive_uv_b);
            emissive *= textureSample(t_emissive, s_emissive, emissive_uv).rgb;
        }
        bloom_color = emissive;
        lit += emissive;
    }

    out_alpha = apply_alpha_mode(out_alpha, material.alpha_cutoff);
    var out: FsOutput;
    out.color = vec4<f32>(lit, out_alpha);
    out.bloom = vec4<f32>(bloom_color, out_alpha);
    return out;
}
"#
);

/// MMD main shader common part (vertex shader + lighting body).
macro_rules! wgsl_mmd_main_body {
    () => {
        r#"
const ALPHA_DISCARD_THRESHOLD: f32 = 0.004;

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(1) @binding(0) var t_diffuse: texture_2d<f32>;
@group(1) @binding(1) var s_diffuse: sampler;
@group(2) @binding(0) var<uniform> material: MmdMaterialUniform;
@group(3) @binding(0) var t_sphere: texture_2d<f32>;
@group(3) @binding(1) var s_sphere: sampler;
@group(3) @binding(2) var t_toon: texture_2d<f32>;
@group(3) @binding(3) var s_toon: sampler;

struct MmdVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) world_pos: vec3<f32>,
    @location(3) sphere_uv: vec2<f32>,
};

@vertex
fn vs_mmd(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
) -> MmdVertexOutput {
    var out: MmdVertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(position, 1.0);
    out.normal = normal;
    out.uv = uv;
    out.world_pos = position;
    // Sphere UV: map XY of the view-space normal to [0, 1].
    // Invert `normalWv.x` because the coordinate system has X flipped.
    let vn_x = dot(normal, camera.view_row0);
    let vn_y = dot(normal, camera.view_row1);
    out.sphere_uv = vec2<f32>(vn_x * -0.5 + 0.5, vn_y * -0.5 + 0.5);
    return out;
}

fn compute_mmd_lighting(in: MmdVertexOutput) -> vec4<f32> {
    let n = normalize(in.normal);

    // Lighting:
    // AmbientColor = saturate(MaterialAmbient * LightAmbient + MaterialEmissive)
    // PMX ambient corresponds to D3D's emissive; PMX diffuse corresponds to D3D's ambient.
    // LightAmbient = mmd_ambient_scale * light_color (reflects light tone / intensity).
    let mmd_light = vec3<f32>(camera.mmd_ambient_scale) * camera.light_color;
    let base_color = clamp(material.diffuse_rgb * mmd_light + material.ambient, vec3<f32>(0.0), vec3<f32>(1.0));

    // Texture sampling.
    let tex_color = textureSample(t_diffuse, s_diffuse, in.uv);
    var out_rgb = base_color * tex_color.rgb;
    var out_a = tex_color.a * material.alpha;

    // Sphere map (RGB only; alpha is unaffected).
    let has_sphere = (material.flags & 1u) != 0u;
    let sphere_add  = (material.flags & 2u) != 0u;
    if has_sphere {
        let sph_color = textureSample(t_sphere, s_sphere, in.sphere_uv).rgb;
        if sphere_add {
            out_rgb += sph_color;
        } else {
            out_rgb *= sph_color;
        }
    }

    // Toon (NdotL-dependent sampling + multiply).
    let has_toon = (material.flags & 4u) != 0u;
    if has_toon {
        let lightNormal = dot(n, -camera.light_dir);
        let toon_uv = vec2<f32>(0.0, 0.5 - lightNormal * 0.5);
        let toon_color = textureSample(t_toon, s_toon, toon_uv);
        out_rgb *= toon_color.rgb;
        out_a *= toon_color.a;
    }

    // Alpha test.
    if out_a < ALPHA_DISCARD_THRESHOLD { discard; }

    // Specular (added last; not affected by toon).
    // LightSpecular = mmd_ambient_scale * light_color.
    let spec_color = material.specular * mmd_light;
    var eye_dir: vec3<f32>;
    if camera.is_perspective > 0.5 {
        eye_dir = normalize(camera.camera_pos - in.world_pos);
    } else {
        eye_dir = normalize(camera.camera_forward);
    }
    let half_vec = normalize(eye_dir - camera.light_dir);
    let spec_factor = pow(max(dot(n, half_vec), 0.0), max(0.000001, material.specular_power));
    out_rgb += spec_color * spec_factor;

    return vec4<f32>(out_rgb, out_a);
}
"#
    };
}

/// MMD edge shader common part (vertex shader).
macro_rules! wgsl_mmd_edge_body {
    () => {
        r#"
const EDGE_OFFSET_BASE: f32 = 0.003;
const EDGE_OFFSET_DIST_POW: f32 = 0.7;

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(1) @binding(0) var<uniform> material: MmdMaterialUniform;

struct EdgeVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
};

@vertex
fn vs_edge(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) _uv1: vec2<f32>,
    @location(4) _tangent: vec4<f32>,
    @location(5) edge_scale: f32,
) -> EdgeVertexOutput {
    var out: EdgeVertexOutput;
    let dist = max(length(position - camera.camera_pos), 5.0);
    let offset = edge_scale * material.edge_size * camera.mmd_edge_thickness
                 * pow(dist, EDGE_OFFSET_DIST_POW) * EDGE_OFFSET_BASE;
    let expanded = position + normalize(normal) * offset;
    out.clip_position = camera.view_proj * vec4<f32>(expanded, 1.0);
    return out;
}
"#
    };
}

/// MMD edge shader (inverted hull method; sRGB version).
const MMD_EDGE_SHADER_SRC: &str = concat!(
    wgsl_camera_uniform!(),
    "\n",
    wgsl_mmd_material_uniform!(),
    wgsl_mmd_edge_body!(),
    r#"
@fragment
fn fs_edge() -> MmdFsOutput {
    // Cancel out the sRGB render target's automatic encoding.
    let c = material.edge_color;
    var out: MmdFsOutput;
    out.color = vec4<f32>(pow(max(c.rgb, vec3<f32>(0.0)), vec3<f32>(2.2)), c.a);
    out.bloom = vec4<f32>(0.0);
    return out;
}
"#
);

/// MMD main shader (sRGB version: gamma-correct via pow(2.2)).
const MMD_MAIN_SHADER_SRC: &str = concat!(
    wgsl_camera_uniform!(),
    "\n",
    wgsl_mmd_material_uniform!(),
    wgsl_mmd_main_body!(),
    r#"
@fragment
fn fs_mmd(in: MmdVertexOutput) -> MmdFsOutput {
    let result = compute_mmd_lighting(in);
    // Cancel the sRGB render target's automatic encoding (MMD computes in gamma space).
    let output = pow(max(result.rgb, vec3<f32>(0.0)), vec3<f32>(2.2));
    var out: MmdFsOutput;
    out.color = vec4<f32>(output, result.a);
    out.bloom = vec4<f32>(material.bloom_emissive_r, material.bloom_emissive_g, material.bloom_emissive_b, result.a);
    return out;
}
"#
);

/// MMD edge shader Unorm version (pow(2.2) removed - direct gamma-space output).
const MMD_EDGE_SHADER_UNORM_SRC: &str = concat!(
    wgsl_camera_uniform!(),
    "\n",
    wgsl_mmd_material_uniform!(),
    wgsl_mmd_edge_body!(),
    r#"
@fragment
fn fs_edge() -> MmdFsOutput {
    // Unorm target: emit gamma-space values directly (no pow(2.2) needed).
    var out: MmdFsOutput;
    out.color = material.edge_color;
    out.bloom = vec4<f32>(0.0);
    return out;
}
"#
);

/// MMD main shader Unorm version (pow(2.2) removed - direct gamma-space output).
const MMD_MAIN_SHADER_UNORM_SRC: &str = concat!(
    wgsl_camera_uniform!(),
    "\n",
    wgsl_mmd_material_uniform!(),
    wgsl_mmd_main_body!(),
    r#"
@fragment
fn fs_mmd(in: MmdVertexOutput) -> MmdFsOutput {
    let result = compute_mmd_lighting(in);
    // Unorm target: emit gamma-space values directly (no pow(2.2) needed).
    var out: MmdFsOutput;
    out.color = vec4<f32>(clamp(result.rgb, vec3<f32>(0.0), vec3<f32>(1.0)), result.a);
    out.bloom = vec4<f32>(material.bloom_emissive_r, material.bloom_emissive_g, material.bloom_emissive_b, result.a);
    return out;
}
"#
);

/// Grid shader common part (vertex shader).
macro_rules! wgsl_grid_body {
    () => {
        r#"
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
"#
    };
}

/// Grid shader Unorm version (with `linear_to_srgb` conversion).
const GRID_SHADER_UNORM_SRC: &str = concat!(
    wgsl_camera_uniform!(),
    wgsl_grid_body!(),
    r#"
fn linear_to_srgb(rgb: vec3<f32>) -> vec3<f32> {
    let cutoff = rgb < vec3<f32>(0.0031308);
    let lower = rgb * vec3<f32>(12.92);
    let higher = vec3<f32>(1.055) * pow(rgb, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
    return select(higher, lower, cutoff);
}

@fragment
fn fs_grid(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(linear_to_srgb(max(in.color.rgb, vec3<f32>(0.0))), in.color.a);
}
"#
);

/// Wireframe overlay shader (drawn in black).
const WIRE_OVERLAY_SHADER_SRC: &str = concat!(
    wgsl_camera_uniform!(),
    "\n",
    wgsl_material_uniform!(),
    r#"

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

struct FsOutput {
    @location(0) color: vec4<f32>,
    @location(1) bloom: vec4<f32>,
};

@fragment
fn fs_main(in: VertexOutput) -> FsOutput {
    var out: FsOutput;
    out.color = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    out.bloom = vec4<f32>(0.0);
    return out;
}

@fragment
fn fs_highlight_fill(in: VertexOutput) -> FsOutput {
    var out: FsOutput;
    out.color = vec4<f32>(1.0, 0.5, 0.0, 0.35);
    out.bloom = vec4<f32>(0.0);
    return out;
}
"#
);

/// MToon outline shader common part (inverted hull method).
/// Performs MToon lighting equivalent to the main shader and blends it via
/// `outlineLightingMixFactor`.
/// Binding declarations and helper functions are shared via
/// `wgsl_mtoon_bindings!` / `wgsl_mtoon_helpers!`.
macro_rules! wgsl_outline_body {
    () => {
        r#"
struct OutlineVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) world_pos: vec3<f32>,
    @location(3) uv1: vec2<f32>,
    @location(4) tangent: vec4<f32>,
};

/// Apply UV animation (for vertex shaders; supports UV0/UV1 pair).
/// Return value: vec4(anim_uv0.xy, anim_uv1.zw).
fn apply_uv_animation_pair(uv0: vec2<f32>, uv1: vec2<f32>) -> vec4<f32> {
    let has_uv_anim = material.uv_anim_scroll_x != 0.0
        || material.uv_anim_scroll_y != 0.0
        || material.uv_anim_rotation != 0.0;
    if !has_uv_anim { return vec4<f32>(uv0, uv1); }
    // UV for the mask texture (texCoord + transform; not subject to UV animation).
    let uv_mask_uv = resolve_mtoon_uv(uv0, uv1, material.uv_mask_uv_a, material.uv_mask_uv_b);
    var mask = 1.0;
    if material.has_uv_anim_mask > 0.5 {
        mask = select_channel(textureSampleLevel(t_uv_anim_mask, s_uv_anim_mask, uv_mask_uv, 0.0), material.uv_anim_mask_channel);
    }
    return vec4<f32>(apply_uv_anim_core(uv0, mask), apply_uv_anim_core(uv1, mask));
}

@vertex
fn vs_outline(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) uv1_in: vec2<f32>,
    @location(4) tangent_in: vec4<f32>,
) -> OutlineVertexOutput {
    var out: OutlineVertexOutput;
    let n = normalize(normal);
    // outlineWidthMultiplyTexture: subject to UV animation, applies texCoord + transform (UV0/UV1 pair).
    let anim_pair = apply_uv_animation_pair(uv, uv1_in);
    let width_uv = resolve_mtoon_uv(anim_pair.xy, anim_pair.zw, material.outline_uv_a, material.outline_uv_b);
    let width_tex = select_channel(textureSampleLevel(t_outline_width, s_outline_width, width_uv, 0.0), material.outline_width_channel);
    let width = material.outline_width * width_tex;
    if material.outline_mode > 1.5 {
        // screenCoordinates: offset along the normal in clip space (UniVRM-compliant).
        let clip = camera.view_proj * vec4<f32>(position, 1.0);
        // View-space normal.
        let nv_x = dot(camera.view_row0, n);
        let nv_y = dot(camera.view_row1, n);
        let view_row2 = cross(camera.view_row0, camera.view_row1);
        let nv_z = dot(view_row2, n);
        // UniVRM-compliant: normalize first, then stretch X by `aspect`.
        let raw = vec2<f32>(nv_x, nv_y);
        let len = length(raw);
        var projected = select(vec2<f32>(0.0), raw / len, len > 0.0001);
        // Distance clamp: prevent excessively thick outlines on wide cameras
        // (per UniVRM `MToon_GetOutlineVertex_ScreenCoordinatesWidthMultiplier`).
        let max_view_frustum_plane_height = 2.0;
        let width_scaled_max_distance = max_view_frustum_plane_height * camera.proj_11 * 0.5;
        let width_multiplier = min(clip.w, width_scaled_max_distance);
        projected *= 2.0 * width * width_multiplier;
        projected.x /= camera.aspect;
        // Suppress camera-facing normals (prevents XY drift for front-facing vertices).
        projected *= saturate(1.0 - nv_z * nv_z);
        out.clip_position = vec4<f32>(clip.xy + projected, clip.zw);
    } else {
        // worldCoordinates: meters in world space.
        let expanded = position + n * width;
        out.clip_position = camera.view_proj * vec4<f32>(expanded, 1.0);
    }
    out.normal = n;
    out.uv = uv;
    out.world_pos = position;
    out.uv1 = uv1_in;
    out.tangent = tangent_in;
    return out;
}

/// MToon lighting equivalent to the main shader (for outline use).
/// Return value: vec4(surface shading RGB, processed alpha).
/// `alphaMode`-based `discard` is applied here as well (UniVRM-compliant: also applied to outlines).
fn compute_mtoon_surface_lighting(n: vec3<f32>, uv: vec2<f32>, uv1: vec2<f32>, world_pos: vec3<f32>) -> vec4<f32> {
    // --- UV animation ---
    let has_uv_anim = material.uv_anim_scroll_x != 0.0
        || material.uv_anim_scroll_y != 0.0
        || material.uv_anim_rotation != 0.0;
    // UV for the mask texture (texCoord + transform; not subject to UV animation).
    let uv_mask_uv = resolve_mtoon_uv(uv, uv1, material.uv_mask_uv_a, material.uv_mask_uv_b);
    var anim_mask = 1.0;
    if has_uv_anim && material.has_uv_anim_mask > 0.5 {
        anim_mask = select_channel(textureSample(t_uv_anim_mask, s_uv_anim_mask, uv_mask_uv), material.uv_anim_mask_channel);
    }
    let anim_uv = select(uv, apply_uv_anim_core(uv, anim_mask), has_uv_anim);
    let anim_uv1 = select(uv1, apply_uv_anim_core(uv1, anim_mask), has_uv_anim);

    // Texture sampling (apply UV animation + texCoord/KHR_texture_transform).
    let base_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.base_uv_a, material.base_uv_b);
    let tex_color = textureSample(t_diffuse, s_diffuse, base_uv);
    let base_color = tex_color * material.diffuse;

    // alphaMode handling (same logic as main `fs_main`).
    var out_alpha = base_color.a;
    if material.alpha_cutoff < -0.75 {
        out_alpha = 1.0;
    } else if material.alpha_cutoff >= -0.25 {
        // MASK + AlphaToCoverage (UniVRM-compliant; same as `fs_main`).
        let a2c_alpha = (out_alpha - material.alpha_cutoff)
            / max(fwidth(out_alpha), 1e-5) + 0.5;
        if a2c_alpha < material.alpha_cutoff { discard; }
        out_alpha = 1.0; // UniVRM-compliant: A2C is coverage control only; final alpha is opaque.
    } else {
        if out_alpha <= 0.001 { discard; }
    }

    // dot(N,L) - spec-compliant: [-1, 1] range.
    // `camera.light_dir` is the light direction of travel (source -> surface), so invert to (surface -> source).
    let dot_nl = dot(n, -camera.light_dir);

    // shadeMultiplyTexture (subject to UV animation).
    var shade_mul = vec3<f32>(1.0);
    if material.has_shade_multiply_tex > 0.5 {
        let shade_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.shade_uv_a, material.shade_uv_b);
        shade_mul = textureSample(t_shade_multiply, s_shade_multiply, shade_uv).rgb;
    }
    let shade = material.shade_color * shade_mul;

    // shadingShiftTexture (subject to UV animation; UniVRM-compliant).
    var shading = dot_nl + material.shading_shift;
    if material.has_shading_shift_tex > 0.5 {
        let shift_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.shift_uv_a, material.shift_uv_b);
        let shift_tex = textureSample(t_shading_shift, s_shading_shift, shift_uv).r;
        shading += shift_tex * material.shading_shift_tex_scale;
    }

    // MToon 2-tone toon: interpolate lit/shade with linearstep (spec-compliant).
    let edge0 = -1.0 + material.shading_toony;
    let edge1 = 1.0 - material.shading_toony;
    let t = clamp((shading - edge0) / max(edge1 - edge0, 0.001), 0.0, 1.0);
    let toon_color = mix(shade, base_color.rgb, t);

    // Lighting: separate direct from GI (indirect) (UniVRM-compliant).
    // Hemisphere ambient: interpolate sky/ground by the final normal's Y component (SH approximation).
    let hemi_t_o = n.y * 0.5 + 0.5;
    let raw_indirect = mix(camera.ambient_ground, camera.ambient, vec3<f32>(hemi_t_o));
    let gi = mix(raw_indirect, camera.gi_equalized, material.gi_equalization_factor);
    let direct_light = camera.light_intensity * camera.light_color;
    let lighting = toon_color * direct_light + toon_color * gi;

    // --- Rim lighting + MatCap ---
    // Perspective: camera_pos -> world_pos. Orthographic: camera_forward (UniVRM-compliant).
    var v: vec3<f32>;
    if camera.is_perspective > 0.5 {
        v = normalize(camera.camera_pos - world_pos);
    } else {
        v = normalize(camera.camera_forward);
    }
    var rim = vec3<f32>(0.0);

    // MatCap: derive UV from view-space normal (not subject to UV animation).
    // UniVRM-compliant: right = cross(viewDir, worldUp), up = cross(right, viewDir).
    // KHR_texture_transform is applied to the final matcap UV.
    if material.has_matcap > 0.5 {
        let world_view_x = normalize(vec3<f32>(-v.z, 0.0, v.x));
        let world_view_y = cross(world_view_x, v);
        let raw_matcap_uv = vec2<f32>(dot(world_view_x, n), dot(world_view_y, n)) * 0.495 + 0.5;
        let matcap_uv = apply_texture_transform(raw_matcap_uv, material.matcap_uv_a, material.matcap_uv_b);
        rim = material.matcap_factor * textureSample(t_matcap, s_matcap, matcap_uv).rgb;
    }

    // Parametric rim: Fresnel effect.
    let ndotv = dot(n, v);
    let parametric_rim = pow(
        saturate(1.0 - ndotv + material.rim_lift),
        max(material.rim_fresnel_power, 0.00001)
    );
    rim = rim + parametric_rim * material.rim_color;

    // rimMultiplyTexture (subject to UV animation).
    if material.has_rim_multiply_tex > 0.5 {
        let rim_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.rim_uv_a, material.rim_uv_b);
        rim *= textureSample(t_rim_multiply, s_rim_multiply, rim_uv).rgb;
    }

    // Rim lighting mix (VRM 1.0 spec: rim * lerp(white, lighting, mix)).
    // UniVRM-compliant: use the raw (non-equalized) indirect for rim (GI equalization not applied).
    let rim_light_factor = direct_light + raw_indirect;
    let rim_lit = rim * mix(vec3<f32>(1.0), rim_light_factor, material.rim_lighting_mix);

    // emissive (UniVRM-compliant: baseCol = lighting + emissive + rim).
    var emissive_out = material.emissive_factor;
    if material.has_emissive_tex > 0.5 {
        let emissive_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.emissive_uv_a, material.emissive_uv_b);
        emissive_out *= textureSample(t_emissive, s_emissive, emissive_uv).rgb;
    }

    return vec4<f32>(lighting + rim_lit + emissive_out, out_alpha);
}
"#
    };
}

/// WGSL common: `fs_outline` fragment shader body (only the output expression
/// is parameterized for sRGB/Unorm).
/// `$output_expr`: in the sRGB version, `lit`; in the Unorm version,
/// `clamp(lit, vec3<f32>(0.0), vec3<f32>(1.0))`.
macro_rules! wgsl_fs_outline {
    ($output_expr:expr) => {
        concat!(r#"
struct FsOutput {
    @location(0) color: vec4<f32>,
    @location(1) bloom: vec4<f32>,
};

@fragment
fn fs_outline(in: OutlineVertexOutput, @builtin(front_facing) is_front: bool) -> FsOutput {
    let base = material.outline_color;
    // doubleSided backface normal flip (UniVRM-compliant).
    let face_sign = select(-1.0, 1.0, is_front);
    var n = normalize(in.normal) * face_sign;
    // UV-animation precomputation (also applied to normalTexture per spec).
    var anim_uv_o = in.uv;
    var anim_uv1_o = in.uv1;
    let has_uv_anim_o = material.uv_anim_scroll_x != 0.0
        || material.uv_anim_scroll_y != 0.0
        || material.uv_anim_rotation != 0.0;
    if has_uv_anim_o {
        let uv_mask_uv = resolve_mtoon_uv(in.uv, in.uv1, material.uv_mask_uv_a, material.uv_mask_uv_b);
        var anim_mask_o = 1.0;
        if material.has_uv_anim_mask > 0.5 {
            anim_mask_o = select_channel(textureSample(t_uv_anim_mask, s_uv_anim_mask, uv_mask_uv), material.uv_anim_mask_channel);
        }
        anim_uv_o = apply_uv_anim_core(in.uv, anim_mask_o);
        anim_uv1_o = apply_uv_anim_core(in.uv1, anim_mask_o);
    }
    // Normal-map application (animated UV).
    if material.has_normal_tex > 0.5 {
        let normal_uv = resolve_mtoon_uv(anim_uv_o, anim_uv1_o, material.normal_uv_a, material.normal_uv_b);
        n = apply_normal_map(n, in.tangent, normal_uv);
    }
    // Get the MToon lighting result equivalent to the main shader (includes alpha handling / discard).
    let surface = compute_mtoon_surface_lighting(n, in.uv, in.uv1, in.world_pos);
    // UniVRM-compliant: outlineColor * lerp(1, baseCol, outlineLightingMix).
    let lit = base.rgb * mix(vec3<f32>(1.0), surface.rgb, material.outline_lighting_mix);
    var out: FsOutput;
    out.color = vec4<f32>("#, $output_expr, r#", surface.a);
    out.bloom = vec4<f32>(0.0);
    return out;
}
"#)
    };
}

/// MToon outline shader (sRGB version).
/// Computes MToon lighting equivalent to the main shader and blends via
/// `outlineLightingMixFactor`.
const OUTLINE_SHADER_SRC: &str = concat!(
    wgsl_camera_uniform!(),
    "\n",
    wgsl_material_uniform!(),
    wgsl_mtoon_bindings!(),
    wgsl_mtoon_helpers!(),
    wgsl_outline_body!(),
    wgsl_fs_outline!("lit"),
);

/// MToon outline shader Unorm version (clamps to 0..1).
/// Computes MToon lighting equivalent to the main shader and blends via
/// `outlineLightingMixFactor`.
const OUTLINE_SHADER_UNORM_SRC: &str = concat!(
    wgsl_camera_uniform!(),
    "\n",
    wgsl_material_uniform!(),
    wgsl_mtoon_bindings!(),
    wgsl_mtoon_helpers!(),
    wgsl_outline_body!(),
    wgsl_fs_outline!("clamp(lit, vec3<f32>(0.0), vec3<f32>(1.0))"),
);

const GRID_SHADER_SRC: &str = concat!(
    wgsl_camera_uniform!(),
    wgsl_grid_body!(),
    r#"
@fragment
fn fs_grid(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
"#
);

/// Render parameters (settings bundled for `render_to_texture`).
pub struct RenderParams<'a> {
    pub camera: &'a OrbitCamera,
    pub width: u32,
    pub height: u32,
    /// Pixel height of the bottom overlay (e.g. material edit panel) over the
    /// central viewport. `view_proj` / `proj_11` compensate FOV so the model's
    /// on-screen size is preserved across panel open/close.
    pub overlay_h_pixels: f32,
    pub material_visibility: &'a [bool],
    pub display: &'a super::app::DisplaySettings,
    /// Animated bone global matrices (glTF space; `None` = rest pose).
    pub animated_bone_globals: Option<&'a [glam::Mat4]>,
    /// Whether this is VRM 0.0 (for coordinate conversion).
    pub is_vrm0: bool,
    /// Accumulated time (seconds; for UV animation).
    pub time: f32,
    /// `draw_index` set being hovered (highlighted with an orange wireframe).
    pub hovered_draw_indices: &'a [usize],
}

/// Draw mode.
#[derive(Clone, Copy, PartialEq)]
pub enum DrawMode {
    Solid,
    Wireframe,
    SolidWireframe,
}

/// Light mode.
#[derive(Clone, Copy, PartialEq)]
pub enum LightMode {
    CameraFollow,
    Fixed,
}

/// Fragment-shader override mode (the value passed to the GPU uniform).
#[derive(Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum ShaderOverride {
    #[default]
    Default = 0,
    Normal = 1,
    Unlit = 2,
    GgxPreview = 3,
}

/// For the UI dropdown.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ShaderSelection {
    #[default]
    Auto, // pick Standard / MMD automatically based on the model format
    Mtoon, // force MToon / Lambert (use Standard path even for PMX / PMD)
    Unlit,
    GgxPreview,
    Normal,
    Mmd,
}

/// Pipeline set per sample count.
struct PipelineSet {
    pipeline_cull: wgpu::RenderPipeline,
    pipeline_no_cull: wgpu::RenderPipeline,
    pipeline_wireframe: Option<wgpu::RenderPipeline>,
    /// Wireframe overlay (for Solid+Wire; with depth bias).
    pipeline_wire_overlay: Option<wgpu::RenderPipeline>,
    /// Material hover highlight (orange wireframe).
    pipeline_highlight: Option<wgpu::RenderPipeline>,
    pipeline_mask_cull: wgpu::RenderPipeline,
    pipeline_mask_no_cull: wgpu::RenderPipeline,
    pipeline_alpha_cull: wgpu::RenderPipeline,
    pipeline_alpha_no_cull: wgpu::RenderPipeline,
    /// Translucent + depth write enabled (MToon `transparentWithZWrite`).
    pipeline_alpha_zwrite_cull: wgpu::RenderPipeline,
    pipeline_alpha_zwrite_no_cull: wgpu::RenderPipeline,
    /// For VRM 0.x `_CullMode=Front` (front-face culling).
    pipeline_front_cull: wgpu::RenderPipeline,
    pipeline_mask_front_cull: wgpu::RenderPipeline,
    pipeline_alpha_front_cull: wgpu::RenderPipeline,
    pipeline_alpha_zwrite_front_cull: wgpu::RenderPipeline,
    pipeline_grid: wgpu::RenderPipeline,
    pipeline_bone: wgpu::RenderPipeline,
    pipeline_line_overlay: wgpu::RenderPipeline,
    // MMD pipelines.
    pipeline_mmd_main_cull: Option<wgpu::RenderPipeline>,
    pipeline_mmd_main_no_cull: Option<wgpu::RenderPipeline>,
    pipeline_mmd_alpha_cull: Option<wgpu::RenderPipeline>,
    pipeline_mmd_alpha_no_cull: Option<wgpu::RenderPipeline>,
    pipeline_mmd_edge: Option<wgpu::RenderPipeline>,
    // MToon outline pipeline (inverted hull method; front cull).
    pipeline_outline: wgpu::RenderPipeline,
    // MToon outline pipeline (for BLEND; ZWrite OFF).
    pipeline_outline_blend: wgpu::RenderPipeline,
    // MToon outline pipeline (for MASK; AlphaToCoverage enabled).
    pipeline_outline_mask: wgpu::RenderPipeline,
}

/// Set of default `TextureView`s representing "unassigned" auxiliary texture slots.
///
/// Created once in `GpuRenderer::new()`; `rebuild_material_bind_groups()` only
/// references them to build bind groups (§D / TODO-4). Held on `GpuRenderer`
/// since they are not duplicated per model.
///
/// - `white_srgb`: default for sRGB slots like shade / rim / outline width.
/// - `black_srgb`: default for matcap (zero contribution).
/// - `flat_normal_unorm`: default for normal map (equivalent to (0, 0, 1)).
pub struct DefaultViews {
    pub white_srgb: wgpu::TextureView,
    pub black_srgb: wgpu::TextureView,
    pub flat_normal_unorm: wgpu::TextureView,
}

pub struct GpuRenderer {
    /// MSAA pipeline set (sample_count=4, sRGB) - lazily created.
    pipelines_msaa_srgb: Option<PipelineSet>,
    /// Non-MSAA pipeline set (sample_count=1, sRGB) - lazily created.
    pipelines_no_msaa_srgb: Option<PipelineSet>,
    /// MSAA pipeline set (sample_count=4, Unorm) - lazily created.
    pipelines_msaa_unorm: Option<PipelineSet>,
    /// Non-MSAA pipeline set (sample_count=1, Unorm) - lazily created.
    pipelines_no_msaa_unorm: Option<PipelineSet>,
    // Resources for lazy pipeline creation.
    shader: wgpu::ShaderModule,
    grid_shader_srgb: wgpu::ShaderModule,
    grid_shader_unorm: wgpu::ShaderModule,
    wire_overlay_shader: wgpu::ShaderModule,
    mmd_edge_shader_srgb: wgpu::ShaderModule,
    mmd_edge_shader_unorm: wgpu::ShaderModule,
    mmd_main_shader_srgb: wgpu::ShaderModule,
    mmd_main_shader_unorm: wgpu::ShaderModule,
    outline_shader_srgb: wgpu::ShaderModule,
    outline_shader_unorm: wgpu::ShaderModule,
    pipeline_layout: wgpu::PipelineLayout,
    grid_pipeline_layout: wgpu::PipelineLayout,
    mmd_edge_pipeline_layout: wgpu::PipelineLayout,
    mmd_main_pipeline_layout: wgpu::PipelineLayout,
    supports_wireframe: bool,
    /// Camera uniform buffer.
    camera_buf: wgpu::Buffer,
    /// Camera bind group.
    camera_bind_group: wgpu::BindGroup,
    /// Camera bind group layout (kept to maintain the BindGroup's lifetime).
    #[expect(dead_code)]
    camera_bgl: wgpu::BindGroupLayout,
    /// Texture bind group layout.
    texture_bgl: wgpu::BindGroupLayout,
    /// Material bind group layout.
    material_bgl: wgpu::BindGroupLayout,
    /// Default white-texture bind group.
    default_tex_bind_group: wgpu::BindGroup,
    /// MToon auxiliary texture bind group layout (group 3).
    mtoon_aux_bgl: wgpu::BindGroupLayout,
    /// Default MToon auxiliary bind group (matcap = black, others = white).
    default_mtoon_aux_bind_group: wgpu::BindGroup,
    /// Shared texture sampler (avoids per-frame creation).
    default_sampler: wgpu::Sampler,
    /// "Unassigned" default `TextureView` set used when rebuilding bind groups during material edit (§D).
    default_views: DefaultViews,
    /// Grid vertex buffer.
    grid_vbuf: wgpu::Buffer,
    grid_vertex_count: u32,
    /// Bone-tail buffer (LineList; tail triangles).
    bone_tail: DynamicBuffer,
    /// Bone-fill buffer (TriangleList; marker fill faces).
    bone_fill: DynamicBuffer,
    /// Bone-outline buffer (LineList; marker outlines).
    bone_line: DynamicBuffer,
    /// SpringBone vertex buffer.
    spring: DynamicBuffer,
    /// Joint face buffer (TriangleList).
    joint: DynamicBuffer,
    /// Joint edge buffer (LineList).
    joint_edge: DynamicBuffer,
    /// Normal-display vertex buffer.
    normal: DynamicBuffer,
    /// Normal-cache invalidation flag (true = needs regeneration).
    normal_dirty: bool,
    /// Normal cache: previous `normal_length`.
    normal_cache_length: f32,
    /// Normal cache: previous `material_visibility`.
    normal_cache_visibility: Vec<bool>,
    /// Offscreen texture cache.
    offscreen: Option<OffscreenTarget>,
    /// Current MSAA-enabled state.
    current_msaa: bool,
    /// Working buffer for generating bone-tail vertices.
    bone_tail_work: Vec<GridVertex>,
    /// Working buffer for generating bone-fill vertices.
    bone_fill_work: Vec<GridVertex>,
    /// Working buffer for generating bone-outline vertices.
    bone_work: Vec<GridVertex>,
    /// Working buffer for generating normal vertices.
    normal_work: Vec<GridVertex>,
    /// Working buffer used to dedupe normal vertices.
    normal_seen: std::collections::HashSet<(u32, u32, u32, u32, u32, u32)>,
    /// Working buffer for normal-vertex visibility flags.
    normal_visible_work: Vec<bool>,
    /// Working buffer for generating SpringBone vertices.
    spring_work: Vec<GridVertex>,
    joint_work: Vec<GridVertex>,
    joint_edge_work: Vec<GridVertex>,
    /// Bone-vertex cache: previous camera position.
    bone_cache_eye: Vec3,
    /// Bone-vertex cache: previous bone opacity.
    bone_cache_opacity: f32,
    /// SpringBone / Joint cache: previous SpringBone opacity.
    spring_cache_opacity: f32,
    /// SpringBone / Joint cache: previous joint opacity.
    joint_cache_opacity: f32,
    /// SpringBone / Joint cache: previous `align_rigid_rotation`.
    spring_cache_align: bool,
    /// Whether animation was enabled in the previous frame (for Some -> None transition detection).
    cache_had_anim: bool,
    /// Translucent sort: working buffer for DrawCall centroids.
    work_draw_centers: Vec<glam::Vec3>,
    /// Translucent sort: working buffer for sorted indices.
    work_sorted_indices: Vec<usize>,
    /// Translucent sort cache: camera eye at the previous sort.
    cache_sort_eye: Option<glam::Vec3>,
    /// Translucent sort cache: DrawCall count at the previous sort.
    cache_sort_draw_count: usize,
    /// Translucent sort cache: previous vertex pointer (for animation change detection).
    cache_sort_vert_ptr: usize,
    /// Translucent sort force-recompute flag.
    sort_dirty: bool,
    /// Reusable buffer for encase uniform serialization (avoids per-frame heap allocation).
    encase_work: Vec<u8>,
    // MMD resources.
    mmd_material_bgl: wgpu::BindGroupLayout,
    mmd_aux_bgl: wgpu::BindGroupLayout,
    #[expect(dead_code)]
    shared_toon_textures: [wgpu::TextureView; 10],
    shared_toon_textures_unorm: [wgpu::TextureView; 10],
    shared_toon_sampler: wgpu::Sampler,
    default_mmd_aux_bind_group: wgpu::BindGroup,
    /// Bloom post effect.
    bloom: super::bloom::BloomPass,
}

/// MSAA sample count.
const MSAA_SAMPLE_COUNT: u32 = 4;

struct OffscreenTarget {
    _color: wgpu::Texture,
    color_view: wgpu::TextureView,
    color_view_unorm: wgpu::TextureView,
    _msaa_color: Option<wgpu::Texture>,
    msaa_color_view: Option<wgpu::TextureView>,
    msaa_color_view_unorm: Option<wgpu::TextureView>,
    _depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    /// MRT bloom source texture (Rgba8Unorm, linear, sample_count=1).
    _bloom_source: wgpu::Texture,
    bloom_source_view: wgpu::TextureView,
    /// MRT bloom source MSAA texture (only when MSAA is enabled).
    _msaa_bloom_source: Option<wgpu::Texture>,
    msaa_bloom_source_view: Option<wgpu::TextureView>,
    width: u32,
    height: u32,
    msaa: bool,
}

impl GpuRenderer {
    /// Default "unassigned" view set referenced when rebuilding bind groups during material edit (§D).
    pub fn default_views(&self) -> &DefaultViews {
        &self.default_views
    }

    /// Shared sampler. Public so it can be used from the material-edit UI rebuild path (§D).
    pub fn default_sampler(&self) -> &wgpu::Sampler {
        &self.default_sampler
    }

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

        let material_bgl = create_material_bind_group_layout(device);

        // Camera uniform buffer
        let camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera_uniform"),
            size: <CameraUniform as encase::ShaderType>::METADATA
                .min_size()
                .get(),
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

        // MToon auxiliary texture bind group layout (group 3).
        let mtoon_aux_bgl = create_mtoon_aux_bind_group_layout(device);

        // Default MToon aux bind group (matcap = black, others = white, normal = flat).
        let black_view = create_black_texture_view(device, queue);
        let (white_srgb_view, _white_unorm_view) = create_white_texture_view(device, queue);
        let flat_normal_view = create_flat_normal_texture_view(device, queue);
        let mtoon_aux_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mtoon_aux_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let s = &mtoon_aux_sampler;
        let default_mtoon_aux_bind_group = create_mtoon_aux_bind_group(
            device,
            &mtoon_aux_bgl,
            AuxTexEntry {
                view: &black_view,
                sampler: s,
            }, // matcap: black
            AuxTexEntry {
                view: &white_srgb_view,
                sampler: s,
            }, // shade_multiply: white
            AuxTexEntry {
                view: &white_srgb_view,
                sampler: s,
            }, // shading_shift: white
            AuxTexEntry {
                view: &white_srgb_view,
                sampler: s,
            }, // rim_multiply: white
            AuxTexEntry {
                view: &white_srgb_view,
                sampler: s,
            }, // uv_anim_mask: white
            AuxTexEntry {
                view: &white_srgb_view,
                sampler: s,
            }, // outline_width: white
            AuxTexEntry {
                view: &white_srgb_view,
                sampler: s,
            }, // emissive: white
            AuxTexEntry {
                view: &flat_normal_view,
                sampler: s,
            }, // normal: flat
        );

        // §D: aggregate the views consumed by bind-group creation into
        // `DefaultViews` and have `GpuRenderer` own them. This lets
        // `rebuild_material_bind_groups` (driven by material edits) reference
        // them as "unassigned" slots.
        let default_views = DefaultViews {
            white_srgb: white_srgb_view,
            black_srgb: black_view,
            flat_normal_unorm: flat_normal_view,
        };

        // Shader modules (kept for lazy pipeline creation).
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mesh_shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
        });

        let grid_shader_srgb = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("grid_shader"),
            source: wgpu::ShaderSource::Wgsl(GRID_SHADER_SRC.into()),
        });

        let wire_overlay_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wire_overlay_shader"),
            source: wgpu::ShaderSource::Wgsl(WIRE_OVERLAY_SHADER_SRC.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh_pipeline_layout"),
            bind_group_layouts: &[&camera_bgl, &texture_bgl, &material_bgl, &mtoon_aux_bgl],
            push_constant_ranges: &[],
        });

        let grid_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("grid_pipeline_layout"),
            bind_group_layouts: &[&camera_bgl],
            push_constant_ranges: &[],
        });

        // MMD bind group layouts
        let mmd_material_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mmd_material_bgl"),
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

        let mmd_aux_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mmd_aux_bgl"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // MMD shader modules (sRGB version: with pow(2.2)).
        let mmd_edge_shader_srgb = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mmd_edge_shader"),
            source: wgpu::ShaderSource::Wgsl(MMD_EDGE_SHADER_SRC.into()),
        });
        let mmd_main_shader_srgb = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mmd_main_shader"),
            source: wgpu::ShaderSource::Wgsl(MMD_MAIN_SHADER_SRC.into()),
        });

        // MMD shader modules (Unorm version: pow(2.2) removed).
        let mmd_edge_shader_unorm = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mmd_edge_shader_unorm"),
            source: wgpu::ShaderSource::Wgsl(MMD_EDGE_SHADER_UNORM_SRC.into()),
        });
        let mmd_main_shader_unorm = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mmd_main_shader_unorm"),
            source: wgpu::ShaderSource::Wgsl(MMD_MAIN_SHADER_UNORM_SRC.into()),
        });

        // MToon outline shaders (sRGB / Unorm versions).
        let outline_shader_srgb = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("outline_shader"),
            source: wgpu::ShaderSource::Wgsl(OUTLINE_SHADER_SRC.into()),
        });
        let outline_shader_unorm = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("outline_shader_unorm"),
            source: wgpu::ShaderSource::Wgsl(OUTLINE_SHADER_UNORM_SRC.into()),
        });

        // Grid shader (Unorm version: with `linear_to_srgb`).
        let grid_shader_unorm = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("grid_shader_unorm"),
            source: wgpu::ShaderSource::Wgsl(GRID_SHADER_UNORM_SRC.into()),
        });

        // MMD pipeline layouts.
        let mmd_edge_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("mmd_edge_pl"),
                bind_group_layouts: &[&camera_bgl, &mmd_material_bgl],
                push_constant_ranges: &[],
            });
        let mmd_main_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("mmd_main_pl"),
                bind_group_layouts: &[&camera_bgl, &texture_bgl, &mmd_material_bgl, &mmd_aux_bgl],
                push_constant_ranges: &[],
            });

        // Shared toon textures (toon01-10).
        let shared_toon_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("toon_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let (shared_toon_textures, shared_toon_textures_unorm) =
            generate_shared_toon_textures(device, queue);

        // Default MMD aux bind group (white sphere + white toon; Unorm views).
        let (_white_view_srgb, white_view_unorm) = create_white_texture_view(device, queue);
        let default_mmd_aux_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("default_mmd_aux_bg"),
            layout: &mmd_aux_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&white_view_unorm),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&shared_toon_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&shared_toon_textures_unorm[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&shared_toon_sampler),
                },
            ],
        });

        let supports_wireframe = device
            .features()
            .contains(wgpu::Features::POLYGON_MODE_LINE);
        if !supports_wireframe {
            log::warn!("POLYGON_MODE_LINE not supported: wireframe disabled");
        }

        // Pipeline sets are created lazily (only the ones needed at first draw).

        // Shared sampler (reused when creating texture bind groups).
        let default_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("default_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            anisotropy_clamp: 16,
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
            pipelines_msaa_srgb: None,
            pipelines_no_msaa_srgb: None,
            pipelines_msaa_unorm: None,
            pipelines_no_msaa_unorm: None,
            shader,
            grid_shader_srgb,
            grid_shader_unorm,
            wire_overlay_shader,
            mmd_edge_shader_srgb,
            mmd_edge_shader_unorm,
            mmd_main_shader_srgb,
            mmd_main_shader_unorm,
            outline_shader_srgb,
            outline_shader_unorm,
            pipeline_layout,
            grid_pipeline_layout,
            mmd_edge_pipeline_layout,
            mmd_main_pipeline_layout,
            supports_wireframe,
            camera_buf,
            camera_bind_group,
            camera_bgl,
            texture_bgl,
            material_bgl,
            default_tex_bind_group,
            mtoon_aux_bgl,
            default_mtoon_aux_bind_group,
            default_sampler,
            default_views,
            bone_tail: DynamicBuffer::new(),
            bone_fill: DynamicBuffer::new(),
            bone_line: DynamicBuffer::new(),
            spring: DynamicBuffer::new(),
            joint: DynamicBuffer::new(),
            joint_edge: DynamicBuffer::new(),
            normal: DynamicBuffer::new(),
            normal_dirty: true,
            normal_cache_length: 0.0,
            normal_cache_visibility: Vec::new(),
            grid_vbuf,
            grid_vertex_count,
            offscreen: None,
            current_msaa: true,
            bone_tail_work: Vec::new(),
            bone_fill_work: Vec::new(),
            bone_work: Vec::new(),
            normal_work: Vec::new(),
            normal_seen: std::collections::HashSet::new(),
            normal_visible_work: Vec::new(),
            spring_work: Vec::new(),
            joint_work: Vec::new(),
            joint_edge_work: Vec::new(),
            bone_cache_eye: Vec3::ZERO,
            bone_cache_opacity: -1.0,
            spring_cache_opacity: -1.0,
            joint_cache_opacity: -1.0,
            spring_cache_align: false,
            cache_had_anim: false,
            work_draw_centers: Vec::new(),
            work_sorted_indices: Vec::new(),
            cache_sort_eye: None,
            cache_sort_draw_count: 0,
            cache_sort_vert_ptr: 0,
            sort_dirty: true,
            encase_work: Vec::with_capacity(512),
            mmd_material_bgl,
            mmd_aux_bgl,
            shared_toon_textures,
            shared_toon_textures_unorm,
            shared_toon_sampler,
            default_mmd_aux_bind_group,
            bloom: super::bloom::BloomPass::new(device),
        }
    }

    /// Set the translucent-sort force-recompute flag (call on model append / reload).
    pub fn mark_sort_dirty(&mut self) {
        self.sort_dirty = true;
    }

    /// Rebuild the grid buffer to match the model's bbox.
    pub fn rebuild_grid(&mut self, device: &wgpu::Device, bbox_min: Vec3, bbox_max: Vec3) {
        let (extent, step) = super::grid::compute_grid_params(bbox_min, bbox_max);
        let (grid_verts, grid_vertex_count) =
            super::grid::build_grid_vertices_with_params(extent, step);
        self.grid_vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("grid_vbuf"),
            contents: bytemuck::cast_slice(&grid_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        self.grid_vertex_count = grid_vertex_count;
    }

    /// Invalidate the visualization-buffer cache (call on model reload).
    pub fn invalidate_visualization_cache(&mut self) {
        self.bone_cache_eye = Vec3::ZERO;
        self.bone_cache_opacity = -1.0;
        self.spring_cache_opacity = -1.0;
        self.joint_cache_opacity = -1.0;
        self.spring_cache_align = false;
        self.cache_had_anim = false;
        self.bone_tail.vertex_count = 0;
        self.bone_fill.vertex_count = 0;
        self.bone_line.vertex_count = 0;
        self.spring.vertex_count = 0;
        self.joint.vertex_count = 0;
        self.joint_edge.vertex_count = 0;
        self.normal_dirty = true;
    }

    fn create_pipeline_set(
        &self,
        device: &wgpu::Device,
        use_unorm: bool,
        msaa: bool,
    ) -> PipelineSet {
        let (grid_shader, mmd_edge_shader, mmd_main_shader, outline_shader, target_format) =
            if use_unorm {
                (
                    &self.grid_shader_unorm,
                    &self.mmd_edge_shader_unorm,
                    &self.mmd_main_shader_unorm,
                    &self.outline_shader_unorm,
                    wgpu::TextureFormat::Rgba8Unorm,
                )
            } else {
                (
                    &self.grid_shader_srgb,
                    &self.mmd_edge_shader_srgb,
                    &self.mmd_main_shader_srgb,
                    &self.outline_shader_srgb,
                    wgpu::TextureFormat::Rgba8UnormSrgb,
                )
            };
        let bloom_format = wgpu::TextureFormat::Rgba8Unorm;
        let sample_count = if msaa { MSAA_SAMPLE_COUNT } else { 1 };
        let shader = &self.shader;
        let wire_overlay_shader = &self.wire_overlay_shader;
        let pipeline_layout = &self.pipeline_layout;
        let grid_pipeline_layout = &self.grid_pipeline_layout;
        let mmd_edge_pipeline_layout = &self.mmd_edge_pipeline_layout;
        let mmd_main_pipeline_layout = &self.mmd_main_pipeline_layout;
        let supports_wireframe = self.supports_wireframe;
        let ms = wgpu::MultisampleState {
            count: sample_count,
            ..Default::default()
        };
        let ms_mask = wgpu::MultisampleState {
            count: sample_count,
            alpha_to_coverage_enabled: sample_count > 1,
            ..Default::default()
        };

        let color_target = wgpu::ColorTargetState {
            format: target_format,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        };
        // For MASK: no blend (UniVRM MToonValidator-compliant: SrcBlend=One, DstBlend=Zero).
        // AlphaToCoverage controls the coverage mask, so alpha blending is unnecessary.
        let color_target_mask = wgpu::ColorTargetState {
            format: target_format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        };
        let depth_write = wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Greater,
            stencil: Default::default(),
            bias: Default::default(),
        };
        let depth_no_write = wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Greater,
            stencil: Default::default(),
            bias: Default::default(),
        };
        // Outline depth bias (equivalent to UniVRM Offset 1,1) - sign flipped for Reverse-Z.
        let outline_bias = wgpu::DepthBiasState {
            constant: -1,
            slope_scale: -1.0,
            clamp: 0.0,
        };
        let depth_outline_write = wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Greater,
            stencil: Default::default(),
            bias: outline_bias,
        };
        let depth_outline_no_write = wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Greater,
            stencil: Default::default(),
            bias: outline_bias,
        };

        let mmd_color_target = wgpu::ColorTargetState {
            format: target_format,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        };

        // bloom MRT target (emissive-only; Rgba8Unorm linear).
        let bloom_target = wgpu::ColorTargetState {
            format: bloom_format,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        };
        let bloom_target_mask = wgpu::ColorTargetState {
            format: bloom_format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        };

        let format_suffix = if target_format == wgpu::TextureFormat::Rgba8Unorm {
            "_unorm"
        } else {
            "_srgb"
        };
        let msaa_suffix = if sample_count > 1 { "_msaa" } else { "" };
        let suffix = format!("{format_suffix}{msaa_suffix}");

        let pipeline_cull = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("mesh_cull{suffix}")),
            layout: Some(pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
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
            depth_stencil: Some(depth_write.clone()),
            multisample: ms,
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(color_target.clone()), Some(bloom_target.clone())],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        let pipeline_no_cull = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("mesh_no_cull{suffix}")),
            layout: Some(pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
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
            depth_stencil: Some(depth_write.clone()),
            multisample: ms,
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(color_target.clone()), Some(bloom_target.clone())],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        let pipeline_mask_cull = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("mesh_mask_cull{suffix}")),
            layout: Some(pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
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
            depth_stencil: Some(depth_write.clone()),
            multisample: ms_mask,
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[
                    Some(color_target_mask.clone()),
                    Some(bloom_target_mask.clone()),
                ],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        let pipeline_mask_no_cull =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&format!("mesh_mask_no_cull{suffix}")),
                layout: Some(pipeline_layout),
                vertex: wgpu::VertexState {
                    module: shader,
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
                depth_stencil: Some(depth_write.clone()),
                multisample: ms_mask,
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_main"),
                    targets: &[
                        Some(color_target_mask.clone()),
                        Some(bloom_target_mask.clone()),
                    ],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            });

        let pipeline_wireframe = if supports_wireframe {
            Some(
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(&format!("mesh_wire{suffix}")),
                    layout: Some(pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: shader,
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
                    depth_stencil: Some(depth_write.clone()),
                    multisample: ms,
                    fragment: Some(wgpu::FragmentState {
                        module: shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(color_target.clone()), Some(bloom_target.clone())],
                        compilation_options: Default::default(),
                    }),
                    multiview: None,
                    cache: None,
                }),
            )
        } else {
            None
        };

        // Wireframe overlay (for Solid+Wire: depth bias avoids Z-fighting).
        let pipeline_wire_overlay = if supports_wireframe {
            let depth_bias = wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::GreaterEqual,
                stencil: Default::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 1.0,
                    clamp: 0.0,
                },
            };
            // Color target for the wire overlay (alpha blend).
            let wire_color_target = wgpu::ColorTargetState {
                format: target_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            };
            Some(
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(&format!("mesh_wire_overlay{suffix}")),
                    layout: Some(pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: wire_overlay_shader,
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
                    depth_stencil: Some(depth_bias),
                    multisample: ms,
                    fragment: Some(wgpu::FragmentState {
                        module: wire_overlay_shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wire_color_target), Some(bloom_target.clone())],
                        compilation_options: Default::default(),
                    }),
                    multiview: None,
                    cache: None,
                }),
            )
        } else {
            None
        };

        // Highlight pipeline (translucent orange fill).
        let pipeline_highlight = {
            let highlight_depth = wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: Default::default(),
                bias: wgpu::DepthBiasState::default(),
            };
            let highlight_color_target = wgpu::ColorTargetState {
                format: target_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            };
            Some(
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(&format!("mesh_highlight_fill{suffix}")),
                    layout: Some(pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: wire_overlay_shader,
                        entry_point: Some("vs_main"),
                        buffers: &[Vertex::layout()],
                        compilation_options: Default::default(),
                    },
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        cull_mode: None,
                        front_face: wgpu::FrontFace::Cw,
                        polygon_mode: wgpu::PolygonMode::Fill,
                        ..Default::default()
                    },
                    depth_stencil: Some(highlight_depth),
                    multisample: ms,
                    fragment: Some(wgpu::FragmentState {
                        module: wire_overlay_shader,
                        entry_point: Some("fs_highlight_fill"),
                        targets: &[Some(highlight_color_target), Some(bloom_target.clone())],
                        compilation_options: Default::default(),
                    }),
                    multiview: None,
                    cache: None,
                }),
            )
        };

        let pipeline_alpha_cull = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("mesh_alpha_cull{suffix}")),
            layout: Some(pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
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
            depth_stencil: Some(depth_no_write.clone()),
            multisample: ms,
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(color_target.clone()), Some(bloom_target.clone())],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        let pipeline_alpha_no_cull =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&format!("mesh_alpha_no_cull{suffix}")),
                layout: Some(pipeline_layout),
                vertex: wgpu::VertexState {
                    module: shader,
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
                depth_stencil: Some(depth_no_write.clone()),
                multisample: ms,
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(color_target.clone()), Some(bloom_target.clone())],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            });

        // BLEND + ZWrite On pipeline (for MToon `transparentWithZWrite`).
        let pipeline_alpha_zwrite_cull =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&format!("mesh_alpha_zwrite_cull{suffix}")),
                layout: Some(pipeline_layout),
                vertex: wgpu::VertexState {
                    module: shader,
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
                depth_stencil: Some(depth_write.clone()),
                multisample: ms,
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(color_target.clone()), Some(bloom_target.clone())],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            });

        let pipeline_alpha_zwrite_no_cull =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&format!("mesh_alpha_zwrite_no_cull{suffix}")),
                layout: Some(pipeline_layout),
                vertex: wgpu::VertexState {
                    module: shader,
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
                depth_stencil: Some(depth_write.clone()),
                multisample: ms,
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(color_target.clone()), Some(bloom_target.clone())],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            });

        // Pipeline for VRM 0.x `_CullMode=Front` (front-face culling).
        let front_cull_primitive = wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: Some(wgpu::Face::Front),
            front_face: wgpu::FrontFace::Cw,
            ..Default::default()
        };
        let pipeline_front_cull = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("mesh_front_cull{suffix}")),
            layout: Some(pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout()],
                compilation_options: Default::default(),
            },
            primitive: front_cull_primitive,
            depth_stencil: Some(depth_write.clone()),
            multisample: ms,
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(color_target.clone()), Some(bloom_target.clone())],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });
        let pipeline_mask_front_cull =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&format!("mesh_mask_front_cull{suffix}")),
                layout: Some(pipeline_layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Vertex::layout()],
                    compilation_options: Default::default(),
                },
                primitive: front_cull_primitive,
                depth_stencil: Some(depth_write.clone()),
                multisample: ms_mask,
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_main"),
                    targets: &[
                        Some(color_target_mask.clone()),
                        Some(bloom_target_mask.clone()),
                    ],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            });
        let pipeline_alpha_front_cull =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&format!("mesh_alpha_front_cull{suffix}")),
                layout: Some(pipeline_layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Vertex::layout()],
                    compilation_options: Default::default(),
                },
                primitive: front_cull_primitive,
                depth_stencil: Some(depth_no_write.clone()),
                multisample: ms,
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(color_target.clone()), Some(bloom_target.clone())],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            });
        let pipeline_alpha_zwrite_front_cull =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&format!("mesh_alpha_zwrite_front_cull{suffix}")),
                layout: Some(pipeline_layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Vertex::layout()],
                    compilation_options: Default::default(),
                },
                primitive: front_cull_primitive,
                depth_stencil: Some(depth_write.clone()),
                multisample: ms,
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(color_target.clone()), Some(bloom_target.clone())],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            });

        let pipeline_grid = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("grid{suffix}")),
            layout: Some(grid_pipeline_layout),
            vertex: wgpu::VertexState {
                module: grid_shader,
                entry_point: Some("vs_grid"),
                buffers: &[GridVertex::layout()],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: Some(depth_write.clone()),
            multisample: ms,
            fragment: Some(wgpu::FragmentState {
                module: grid_shader,
                entry_point: Some("fs_grid"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        let pipeline_bone = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("bone{suffix}")),
            layout: Some(grid_pipeline_layout),
            vertex: wgpu::VertexState {
                module: grid_shader,
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
            multisample: ms,
            fragment: Some(wgpu::FragmentState {
                module: grid_shader,
                entry_point: Some("fs_grid"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        let pipeline_line_overlay =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&format!("line_overlay{suffix}")),
                layout: Some(grid_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: grid_shader,
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
                    depth_write_enabled: false,
                    depth_compare: wgpu::CompareFunction::Always,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: ms,
                fragment: Some(wgpu::FragmentState {
                    module: grid_shader,
                    entry_point: Some("fs_grid"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            });

        // MMD edge pipeline: Front cull, 2-slot vertex buffer.
        let edge_vertex_buffers = &[
            Vertex::layout(),
            wgpu::VertexBufferLayout {
                array_stride: 4,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 5, // location 4 = tangent (Vertex), location 5 = edge_scale
                    format: wgpu::VertexFormat::Float32,
                }],
            },
        ];
        let pipeline_mmd_edge = Some(device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some(&format!("mmd_edge{suffix}")),
                layout: Some(mmd_edge_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: mmd_edge_shader,
                    entry_point: Some("vs_edge"),
                    buffers: edge_vertex_buffers,
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: Some(wgpu::Face::Front),
                    front_face: wgpu::FrontFace::Cw,
                    ..Default::default()
                },
                depth_stencil: Some(depth_write.clone()),
                multisample: ms,
                fragment: Some(wgpu::FragmentState {
                    module: mmd_edge_shader,
                    entry_point: Some("fs_edge"),
                    targets: &[Some(mmd_color_target.clone()), Some(bloom_target.clone())],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            },
        ));

        // MMD main pipelines (4 variants: cull / no_cull * opaque / alpha).
        let pipeline_mmd_main_cull = Some(device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some(&format!("mmd_main_cull{suffix}")),
                layout: Some(mmd_main_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: mmd_main_shader,
                    entry_point: Some("vs_mmd"),
                    buffers: &[Vertex::layout()],
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: Some(wgpu::Face::Back),
                    front_face: wgpu::FrontFace::Cw,
                    ..Default::default()
                },
                depth_stencil: Some(depth_write.clone()),
                multisample: ms,
                fragment: Some(wgpu::FragmentState {
                    module: mmd_main_shader,
                    entry_point: Some("fs_mmd"),
                    targets: &[Some(mmd_color_target.clone()), Some(bloom_target.clone())],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            },
        ));
        let pipeline_mmd_main_no_cull = Some(device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some(&format!("mmd_main_no_cull{suffix}")),
                layout: Some(mmd_main_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: mmd_main_shader,
                    entry_point: Some("vs_mmd"),
                    buffers: &[Vertex::layout()],
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    front_face: wgpu::FrontFace::Cw,
                    ..Default::default()
                },
                depth_stencil: Some(depth_write.clone()),
                multisample: ms,
                fragment: Some(wgpu::FragmentState {
                    module: mmd_main_shader,
                    entry_point: Some("fs_mmd"),
                    targets: &[Some(mmd_color_target.clone()), Some(bloom_target.clone())],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            },
        ));
        let pipeline_mmd_alpha_cull = Some(device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some(&format!("mmd_alpha_cull{suffix}")),
                layout: Some(mmd_main_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: mmd_main_shader,
                    entry_point: Some("vs_mmd"),
                    buffers: &[Vertex::layout()],
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: Some(wgpu::Face::Back),
                    front_face: wgpu::FrontFace::Cw,
                    ..Default::default()
                },
                depth_stencil: Some(depth_write.clone()),
                multisample: ms,
                fragment: Some(wgpu::FragmentState {
                    module: mmd_main_shader,
                    entry_point: Some("fs_mmd"),
                    targets: &[Some(mmd_color_target.clone()), Some(bloom_target.clone())],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            },
        ));
        let pipeline_mmd_alpha_no_cull = Some(device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some(&format!("mmd_alpha_no_cull{suffix}")),
                layout: Some(mmd_main_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: mmd_main_shader,
                    entry_point: Some("vs_mmd"),
                    buffers: &[Vertex::layout()],
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    front_face: wgpu::FrontFace::Cw,
                    ..Default::default()
                },
                depth_stencil: Some(depth_write.clone()),
                multisample: ms,
                fragment: Some(wgpu::FragmentState {
                    module: mmd_main_shader,
                    entry_point: Some("fs_mmd"),
                    targets: &[Some(mmd_color_target), Some(bloom_target.clone())],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            },
        ));

        // MToon outline pipeline: Front cull (inverted hull).
        // `edge_scale` is unnecessary because the GPU samples `outlineWidthMultiplyTexture`.
        let outline_vertex_buffers = &[Vertex::layout()];
        let pipeline_outline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("outline{suffix}")),
            layout: Some(pipeline_layout),
            vertex: wgpu::VertexState {
                module: outline_shader,
                entry_point: Some("vs_outline"),
                buffers: outline_vertex_buffers,
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Front),
                front_face: wgpu::FrontFace::Cw,
                ..Default::default()
            },
            depth_stencil: Some(depth_outline_write.clone()),
            multisample: ms,
            fragment: Some(wgpu::FragmentState {
                module: outline_shader,
                entry_point: Some("fs_outline"),
                targets: &[Some(color_target.clone()), Some(bloom_target.clone())],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });
        // Outline pipeline for BLEND (ZWrite OFF).
        let pipeline_outline_blend =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&format!("outline_blend{suffix}")),
                layout: Some(pipeline_layout),
                vertex: wgpu::VertexState {
                    module: outline_shader,
                    entry_point: Some("vs_outline"),
                    buffers: outline_vertex_buffers,
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: Some(wgpu::Face::Front),
                    front_face: wgpu::FrontFace::Cw,
                    ..Default::default()
                },
                depth_stencil: Some(depth_outline_no_write.clone()),
                multisample: ms,
                fragment: Some(wgpu::FragmentState {
                    module: outline_shader,
                    entry_point: Some("fs_outline"),
                    targets: &[Some(color_target.clone()), Some(bloom_target.clone())],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            });
        // Outline pipeline for MASK (AlphaToCoverage enabled).
        let pipeline_outline_mask =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&format!("outline_mask{suffix}")),
                layout: Some(pipeline_layout),
                vertex: wgpu::VertexState {
                    module: outline_shader,
                    entry_point: Some("vs_outline"),
                    buffers: outline_vertex_buffers,
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: Some(wgpu::Face::Front),
                    front_face: wgpu::FrontFace::Cw,
                    ..Default::default()
                },
                depth_stencil: Some(depth_outline_write.clone()),
                multisample: ms_mask,
                fragment: Some(wgpu::FragmentState {
                    module: outline_shader,
                    entry_point: Some("fs_outline"),
                    targets: &[
                        Some(color_target_mask.clone()),
                        Some(bloom_target_mask.clone()),
                    ],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            });

        PipelineSet {
            pipeline_cull,
            pipeline_no_cull,
            pipeline_wireframe,
            pipeline_wire_overlay,
            pipeline_highlight,
            pipeline_mask_cull,
            pipeline_mask_no_cull,
            pipeline_alpha_cull,
            pipeline_alpha_no_cull,
            pipeline_alpha_zwrite_cull,
            pipeline_alpha_zwrite_no_cull,
            pipeline_front_cull,
            pipeline_mask_front_cull,
            pipeline_alpha_front_cull,
            pipeline_alpha_zwrite_front_cull,
            pipeline_grid,
            pipeline_bone,
            pipeline_line_overlay,
            pipeline_mmd_main_cull,
            pipeline_mmd_main_no_cull,
            pipeline_mmd_alpha_cull,
            pipeline_mmd_alpha_no_cull,
            pipeline_mmd_edge,
            pipeline_outline,
            pipeline_outline_blend,
            pipeline_outline_mask,
        }
    }

    /// Whether wireframe is supported.
    pub fn supports_wireframe(&self) -> bool {
        self.supports_wireframe
    }

    /// Lazily create the required pipeline set if not yet present.
    pub(crate) fn ensure_pipelines(&mut self, device: &wgpu::Device, use_unorm: bool, msaa: bool) {
        // Skip if already created.
        let already = match (msaa, use_unorm) {
            (true, false) => self.pipelines_msaa_srgb.is_some(),
            (false, false) => self.pipelines_no_msaa_srgb.is_some(),
            (true, true) => self.pipelines_msaa_unorm.is_some(),
            (false, true) => self.pipelines_no_msaa_unorm.is_some(),
        };
        if already {
            return;
        }
        let ps = self.create_pipeline_set(device, use_unorm, msaa);
        match (msaa, use_unorm) {
            (true, false) => self.pipelines_msaa_srgb = Some(ps),
            (false, false) => self.pipelines_no_msaa_srgb = Some(ps),
            (true, true) => self.pipelines_msaa_unorm = Some(ps),
            (false, true) => self.pipelines_no_msaa_unorm = Some(ps),
        }
    }

    /// Get the pipeline set matching the current MSAA setting and Unorm flag
    /// (call `ensure_pipelines` beforehand).
    fn pipelines(&self, use_unorm: bool) -> &PipelineSet {
        match (self.current_msaa, use_unorm) {
            (true, false) => self
                .pipelines_msaa_srgb
                .as_ref()
                .expect("ensure_pipelines must be called before pipelines()"),
            (true, true) => self
                .pipelines_msaa_unorm
                .as_ref()
                .expect("ensure_pipelines must be called before pipelines()"),
            (false, false) => self
                .pipelines_no_msaa_srgb
                .as_ref()
                .expect("ensure_pipelines must be called before pipelines()"),
            (false, true) => self
                .pipelines_no_msaa_unorm
                .as_ref()
                .expect("ensure_pipelines must be called before pipelines()"),
        }
    }

    /// Reference to the texture bind group layout.
    pub fn texture_bgl(&self) -> &wgpu::BindGroupLayout {
        &self.texture_bgl
    }

    /// Reference to the material bind group layout.
    pub fn material_bgl(&self) -> &wgpu::BindGroupLayout {
        &self.material_bgl
    }

    /// Reference to the MToon auxiliary texture bind group layout.
    pub fn mtoon_aux_bgl(&self) -> &wgpu::BindGroupLayout {
        &self.mtoon_aux_bgl
    }

    /// Reference to the shared sampler.
    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.default_sampler
    }

    /// Invalidate the normal cache (call on model change / normal recomputation).
    pub fn invalidate_normal_cache(&mut self) {
        self.normal_dirty = true;
        self.normal_cache_visibility.clear();
        self.normal_cache_length = 0.0;
    }

    /// Ensure the offscreen texture is allocated (recreate on size change or MSAA toggle).
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

        // Invalidate Bloom's external cache when offscreen textures are recreated.
        self.bloom.invalidate_external_cache();

        let tex_size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        // MSAA color texture (multisampled, render target) - only when MSAA is enabled.
        let (msaa_tex, msaa_view, msaa_view_unorm) = if msaa {
            let t = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("offscreen_msaa_color"),
                size: tex_size,
                mip_level_count: 1,
                sample_count: MSAA_SAMPLE_COUNT,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
            });
            let v = t.create_view(&Default::default());
            let v_unorm = t.create_view(&wgpu::TextureViewDescriptor {
                format: Some(wgpu::TextureFormat::Rgba8Unorm),
                ..Default::default()
            });
            (Some(t), Some(v), Some(v_unorm))
        } else {
            (None, None, None)
        };

        // Resolve target color texture (sample_count=1, for egui display).
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen_color"),
            size: tex_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
        });
        let color_view = color.create_view(&Default::default());
        let color_view_unorm = color.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Rgba8Unorm),
            ..Default::default()
        });

        // Depth texture (multisampled when MSAA is enabled).
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

        // MRT bloom source texture (Rgba8Unorm, linear).
        let bloom_format = wgpu::TextureFormat::Rgba8Unorm;
        let (msaa_bloom_tex, msaa_bloom_view) = if msaa {
            let t = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("offscreen_msaa_bloom_source"),
                size: tex_size,
                mip_level_count: 1,
                sample_count: MSAA_SAMPLE_COUNT,
                dimension: wgpu::TextureDimension::D2,
                format: bloom_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let v = t.create_view(&Default::default());
            (Some(t), Some(v))
        } else {
            (None, None)
        };
        let bloom_source = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen_bloom_source"),
            size: tex_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: bloom_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let bloom_source_view = bloom_source.create_view(&Default::default());

        self.offscreen = Some(OffscreenTarget {
            _color: color,
            color_view,
            color_view_unorm,
            _msaa_color: msaa_tex,
            msaa_color_view: msaa_view,
            msaa_color_view_unorm: msaa_view_unorm,
            _depth: depth,
            depth_view,
            _bloom_source: bloom_source,
            bloom_source_view,
            _msaa_bloom_source: msaa_bloom_tex,
            msaa_bloom_source_view: msaa_bloom_view,
            width,
            height,
            msaa,
        });
    }

    /// Generate visualization buffer vertices (bones / normals / rigid bodies / joints) and upload them to GPU.
    fn prepare_visualization_buffers(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        model: &GpuModel,
        ir: &IrModel,
        params: &RenderParams,
    ) {
        // Detect animation state transitions (Some -> None requires reverting to the rest pose).
        let has_anim = params.animated_bone_globals.is_some();
        let anim_just_cleared = self.cache_had_anim && !has_anim;
        self.cache_had_anim = has_anim;

        // Update bone vertices (only when changed).
        if params.display.show_bones && !ir.bones.is_empty() {
            let eye = params.camera.eye();
            let bone_changed = self.bone_line.vertex_count == 0
                || has_anim
                || anim_just_cleared
                || eye != self.bone_cache_eye
                || params.display.bone_opacity != self.bone_cache_opacity;
            if bone_changed {
                self.bone_cache_eye = eye;
                self.bone_cache_opacity = params.display.bone_opacity;
                let pos_fn = crate::convert::coord::pos_fn(ir.source_format.is_vrm0());
                generate_bone_vertices(
                    &mut self.bone_tail_work,
                    &mut self.bone_fill_work,
                    &mut self.bone_work,
                    ir,
                    pos_fn,
                    params.camera.eye(),
                    params.display.bone_opacity,
                    params.animated_bone_globals,
                );
                // Tail buffer (LineList).
                self.bone_tail.vertex_count = self.bone_tail_work.len() as u32;
                self.bone_tail.upload(
                    device,
                    queue,
                    bytemuck::cast_slice(&self.bone_tail_work),
                    "bone_tail_vbuf",
                );
                // Fill buffer (TriangleList).
                self.bone_fill.vertex_count = self.bone_fill_work.len() as u32;
                self.bone_fill.upload(
                    device,
                    queue,
                    bytemuck::cast_slice(&self.bone_fill_work),
                    "bone_fill_vbuf",
                );
                // Outline buffer (LineList).
                self.bone_line.vertex_count = self.bone_work.len() as u32;
                self.bone_line.upload(
                    device,
                    queue,
                    bytemuck::cast_slice(&self.bone_work),
                    "bone_vbuf",
                );
            }
        }

        // Update normal-display vertices (regenerate when input changes or during animation).
        if params.display.show_normals {
            let length_changed =
                (params.display.normal_length - self.normal_cache_length).abs() > 1e-6;
            let vis_changed = self.normal_cache_visibility.as_slice() != params.material_visibility;
            let has_animation = model.current_vertices().as_ptr() != model.base_vertices().as_ptr();
            if self.normal_dirty || length_changed || vis_changed || has_animation {
                generate_normal_vertices(
                    &mut self.normal_work,
                    &mut self.normal_seen,
                    &mut self.normal_visible_work,
                    model,
                    params.display.normal_length,
                    params.material_visibility,
                );
                self.normal.vertex_count = self.normal_work.len() as u32;
                self.normal.upload(
                    device,
                    queue,
                    bytemuck::cast_slice(&self.normal_work),
                    "normal_vbuf",
                );
                self.normal_dirty = false;
                self.normal_cache_length = params.display.normal_length;
                self.normal_cache_visibility.clear();
                self.normal_cache_visibility
                    .extend_from_slice(params.material_visibility);
            }
        } else {
            if self.normal.vertex_count > 0 {
                self.normal_dirty = true; // Mark dirty so we regenerate on the next show.
            }
            self.normal.vertex_count = 0;
        }

        // Common to SpringBone/Joint: compute bone deltas only once.
        let need_spring = params.display.show_spring_bones
            && (!ir.physics.rigid_bodies.is_empty() || !ir.physics.joints.is_empty());
        let need_joint = params.display.show_joints && !ir.physics.joints.is_empty();
        let bone_deltas = if (need_spring || need_joint) && has_anim {
            compute_bone_deltas(ir, params.animated_bone_globals, params.is_vrm0)
        } else {
            None
        };

        // Update SpringBone vertices every frame.
        if !params.display.show_spring_bones
            || (ir.physics.rigid_bodies.is_empty() && ir.physics.joints.is_empty())
        {
            self.spring.vertex_count = 0;
        }
        if need_spring {
            let spring_changed = self.spring.vertex_count == 0
                || has_anim
                || anim_just_cleared
                || params.display.spring_bone_opacity != self.spring_cache_opacity
                || params.display.align_rigid_rotation != self.spring_cache_align;
            if spring_changed {
                self.spring_cache_opacity = params.display.spring_bone_opacity;
                self.spring_cache_align = params.display.align_rigid_rotation;
                generate_spring_bone_vertices(
                    &mut self.spring_work,
                    ir,
                    params.display.spring_bone_opacity,
                    params.display.align_rigid_rotation,
                    &bone_deltas,
                    params.is_vrm0,
                );
                self.spring.vertex_count = self.spring_work.len() as u32;
                self.spring.upload(
                    device,
                    queue,
                    bytemuck::cast_slice(&self.spring_work),
                    "spring_vbuf",
                );
            }
        }

        // Update joint vertices every frame.
        if !params.display.show_joints || ir.physics.joints.is_empty() {
            self.joint.vertex_count = 0;
            self.joint_edge.vertex_count = 0;
        }
        if need_joint {
            let joint_changed = self.joint.vertex_count == 0
                || has_anim
                || anim_just_cleared
                || params.display.joint_opacity != self.joint_cache_opacity;
            if joint_changed {
                self.joint_cache_opacity = params.display.joint_opacity;
                generate_joint_vertices(
                    &mut self.joint_work,
                    &mut self.joint_edge_work,
                    ir,
                    params.display.joint_opacity,
                    &bone_deltas,
                    params.is_vrm0,
                );
                // Face buffer (TriangleList).
                self.joint.vertex_count = self.joint_work.len() as u32;
                self.joint.upload(
                    device,
                    queue,
                    bytemuck::cast_slice(&self.joint_work),
                    "joint_vbuf",
                );
                // Edge buffer (LineList).
                self.joint_edge.vertex_count = self.joint_edge_work.len() as u32;
                self.joint_edge.upload(
                    device,
                    queue,
                    bytemuck::cast_slice(&self.joint_edge_work),
                    "joint_edge_vbuf",
                );
            }
        }
    }

    /// Render the model to the offscreen texture and return its egui `TextureId`.
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
        // Ensure the offscreen texture (recreate on size change or MSAA toggle).
        self.ensure_offscreen(device, params.width, params.height, params.display.msaa);

        // Prepare visualization buffers (bones / normals / rigid bodies / joints).
        self.prepare_visualization_buffers(device, queue, model, ir, params);

        let mmd_mode = params.display.use_mmd_path;
        let mmd_edge_enabled = params.display.mmd_edge_enabled;
        // For wireframe / shader override, fall back to existing pipelines instead of the MMD path.
        let mmd_solid = mmd_mode
            && params.display.draw_mode == DrawMode::Solid
            && params.display.shader_override == ShaderOverride::Default;

        // Check upfront whether MMD drawing is needed.
        let has_mmd_draws = mmd_solid
            && model.draws.iter().any(|d| {
                d.render_style == super::mesh::RenderStyle::Mmd
                    && d.mmd_material_bind_group.is_some()
            });

        // Decide Unorm frame: only when the frame fully runs on the MMD-dedicated path.
        let use_unorm = can_use_unorm_frame(model, params.material_visibility, mmd_solid);

        // Lazily create the pipeline set (compiled only on first use).
        self.ensure_pipelines(device, use_unorm, self.current_msaa);

        let offscreen = self
            .offscreen
            .as_ref()
            .expect("already initialized by ensure_offscreen");

        // Update the camera uniform (reuse buffer to avoid heap allocation).
        let cam_uniform = Self::build_camera_uniform(params);
        self.encase_work.clear();
        let mut encase_buf = encase::UniformBuffer::new(&mut self.encase_work);
        encase_buf.write(&cam_uniform).expect("encase write");
        queue.write_buffer(&self.camera_buf, 0, encase_buf.as_ref());

        // Encode
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("offscreen_encoder"),
        });

        // Use `take` to avoid borrow conflicts (`self.pipelines()` borrows all of `self` immutably).
        let mut work_draw_centers = std::mem::take(&mut self.work_draw_centers);
        let mut work_sorted_indices = std::mem::take(&mut self.work_sorted_indices);

        // Build the draw-order index.
        let pending_sort_cache = self.build_draw_queue(
            model,
            params,
            &mut work_draw_centers,
            &mut work_sorted_indices,
        );

        let ps = self.pipelines(use_unorm);

        // Color view selection: Unorm / sRGB view depending on `use_unorm`.
        let (color_view, resolve_target): (&wgpu::TextureView, Option<&wgpu::TextureView>) =
            if use_unorm {
                if let Some(ref msaa_view_unorm) = offscreen.msaa_color_view_unorm {
                    (msaa_view_unorm, Some(&offscreen.color_view_unorm))
                } else {
                    (&offscreen.color_view_unorm, None)
                }
            } else if let Some(ref msaa_view) = offscreen.msaa_color_view {
                (msaa_view, Some(&offscreen.color_view))
            } else {
                (&offscreen.color_view, None)
            };

        // Clear-color compensation: pre-encode values written to the Unorm target because egui sRGB-decodes them.
        let bg = if use_unorm {
            linear_to_srgb_f64(params.display.bg_brightness as f64)
        } else {
            params.display.bg_brightness as f64
        };

        // bloom source view selection (the second MRT target, always Rgba8Unorm).
        let (bloom_view, bloom_resolve): (&wgpu::TextureView, Option<&wgpu::TextureView>) =
            if offscreen.msaa {
                if let Some(ref msaa_bv) = offscreen.msaa_bloom_source_view {
                    (msaa_bv, Some(&offscreen.bloom_source_view))
                } else {
                    (&offscreen.bloom_source_view, None)
                }
            } else {
                (&offscreen.bloom_source_view, None)
            };

        // ===== Pass 1 (MRT): mesh drawing - 2 targets (color + bloom_source) =====
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(if use_unorm {
                    "pass1_mrt_unorm"
                } else {
                    "pass1_mrt_srgb"
                }),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: color_view,
                        resolve_target,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: bg,
                                g: bg,
                                b: bg,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: bloom_view,
                        resolve_target: bloom_resolve,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &offscreen.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            // Standard mesh draw (skipped for empty models).
            if !model.draws.is_empty() {
                pass.set_vertex_buffer(0, model.vertex_buf.slice(..));
                pass.set_index_buffer(model.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);

                Self::draw_standard_meshes(
                    &mut pass,
                    model,
                    params,
                    ps,
                    &work_sorted_indices,
                    &self.camera_bind_group,
                    &self.default_tex_bind_group,
                    &self.default_mtoon_aux_bind_group,
                    mmd_solid,
                    mmd_mode,
                );
            }

            // MMD draw (material-index order - preserves PMX's draw order).
            if has_mmd_draws && !model.draws.is_empty() {
                Self::draw_mmd_meshes(
                    &mut pass,
                    model,
                    params,
                    ps,
                    &self.camera_bind_group,
                    &self.default_tex_bind_group,
                    &self.default_mtoon_aux_bind_group,
                    &self.default_mmd_aux_bind_group,
                    mmd_edge_enabled,
                );
            }

            // Material hover highlight (orange wireframe).
            Self::draw_highlight(
                &mut pass,
                model,
                params,
                ps,
                &self.camera_bind_group,
                &self.default_tex_bind_group,
                &self.default_mtoon_aux_bind_group,
            );
        } // end Pass 1 (MRT)

        // ===== Pass 2 (single target): grid + overlays (normals / bones / rigid bodies / joints) =====
        // NOTE: when MSAA is enabled, `LoadOp::Load` causes a VRAM -> tile
        // readback of the MSAA color texture on tile-based GPUs (Intel iGPU
        // etc.), which incurs bandwidth cost.
        // Merging with Pass 1 (MRT: 2 targets) would reduce render-pass starts
        // to one, but the overlay pipelines are created with a single target,
        // so MRT compatibility (adding `write_mask::EMPTY` on the second
        // target) would be required.
        // No measured issues have been reported so far, and the cost of
        // complicating the pipeline is not worth it; keep as is. Revisit if
        // bandwidth becomes a bottleneck on future MSAA + tile GPUs.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pass2_overlay"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &offscreen.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            self.draw_overlays(&mut pass, params, ps);
        } // end Pass 2 (overlay)

        // Return work buffers (preserve capacity for reuse on the next frame).
        self.work_draw_centers = work_draw_centers;
        self.work_sorted_indices = work_sorted_indices;

        // Update the translucent-sort cache (write all at once after the `ps` borrow is released).
        if let Some((eye, draw_count, vert_ptr)) = pending_sort_cache {
            self.cache_sort_eye = Some(eye);
            self.cache_sort_draw_count = draw_count;
            self.cache_sort_vert_ptr = vert_ptr;
            self.sort_dirty = false;
        }

        // --- Bloom post effect ---
        let bloom_enabled = params.display.bloom_enabled && params.display.bloom_intensity > 0.0;
        if bloom_enabled {
            self.bloom.execute(
                device,
                queue,
                &mut encoder,
                &offscreen.bloom_source_view,
                &offscreen.color_view,
                params.width,
                params.height,
                params.display.bloom_threshold,
                params.display.bloom_intensity,
                params.display.bloom_radius as usize,
            );
        }

        queue.submit(std::iter::once(encoder.finish()));

        // Register the texture (use the composite output when bloom is on; the offscreen view otherwise).
        let present_view = if bloom_enabled {
            self.bloom.composite_view().unwrap_or(&offscreen.color_view)
        } else {
            &offscreen.color_view
        };

        let tex_id = if let Some(existing_id) = *cached_id {
            egui_renderer.update_egui_texture_from_wgpu_texture(
                device,
                present_view,
                wgpu::FilterMode::Linear,
                existing_id,
            );
            existing_id
        } else {
            let id = egui_renderer.register_native_texture(
                device,
                present_view,
                wgpu::FilterMode::Linear,
            );
            *cached_id = Some(id);
            id
        };

        (tex_id, ())
    }

    // -----------------------------------------------------------------------
    // `render_to_texture` helpers
    // -----------------------------------------------------------------------

    /// Build the camera uniform.
    fn build_camera_uniform(params: &RenderParams) -> CameraUniform {
        let viewport_w = params.width as f32;
        let viewport_h = params.height as f32;
        let aspect = viewport_w / viewport_h;
        let overlay_h = params.overlay_h_pixels;
        let light_dir = match params.display.light_mode {
            LightMode::CameraFollow => params.camera.camera_following_light_dir(),
            LightMode::Fixed => OrbitCamera::fixed_light_dir(),
        };
        let view_mat = params.camera.view_matrix();
        let ai = params.display.ambient_intensity;
        let s = &params.display.ambient_sky_color;
        let g = &params.display.ambient_ground_color;
        CameraUniform {
            view_proj: params.camera.view_proj(viewport_w, viewport_h, overlay_h),
            light_dir,
            light_intensity: params.display.light_intensity,
            ambient: glam::Vec3::new(s[0] * ai, s[1] * ai, s[2] * ai),
            shader_mode: params.display.shader_override as u32,
            camera_pos: params.camera.eye(),
            mmd_edge_thickness: params.display.mmd_edge_thickness,
            view_row0: glam::Vec3::new(view_mat.x_axis.x, view_mat.y_axis.x, view_mat.z_axis.x),
            view_row1: glam::Vec3::new(view_mat.x_axis.y, view_mat.y_axis.y, view_mat.z_axis.y),
            mmd_ambient_scale: if params.display.use_mmd_path {
                MMD_LIGHT_AMBIENT * (params.display.light_intensity / MMD_DEFAULT_LIGHT_INTENSITY)
            } else {
                ai
            },
            time: params.time,
            aspect,
            proj_11: params.camera.proj_11(viewport_h, overlay_h),
            gi_equalized: glam::Vec3::new(
                (s[0] + g[0]) * 0.5 * ai,
                (s[1] + g[1]) * 0.5 * ai,
                (s[2] + g[2]) * 0.5 * ai,
            ),
            is_perspective: b2f(params.camera.perspective),
            camera_forward: (params.camera.target - params.camera.eye()).normalize(),
            light_color: glam::Vec3::from(params.display.light_color),
            ambient_ground: glam::Vec3::new(g[0] * ai, g[1] * ai, g[2] * ai),
        }
    }

    /// Sort the draw-order index (includes camera-distance sorting for translucents).
    ///
    /// Returns `Some((eye, draw_count, vert_ptr))` if a sort was performed.
    fn build_draw_queue(
        &self,
        model: &GpuModel,
        params: &RenderParams,
        work_draw_centers: &mut Vec<glam::Vec3>,
        work_sorted_indices: &mut Vec<usize>,
    ) -> Option<(glam::Vec3, usize, usize)> {
        let eye = params.camera.eye();
        let vert_ptr = model.current_vertices().as_ptr() as usize;
        let draw_count = model.draws.len();
        let sort_needed = self.sort_dirty
            || self.cache_sort_draw_count != draw_count
            || self.cache_sort_vert_ptr != vert_ptr
            || self
                .cache_sort_eye
                .is_none_or(|prev_eye| prev_eye.to_array() != eye.to_array())
            || work_sorted_indices.len() != draw_count;

        if !sort_needed {
            return None;
        }

        let verts = model.current_vertices();
        let indices = model.base_indices();
        work_draw_centers.clear();
        work_draw_centers.extend(model.draws.iter().map(|draw| {
            if !matches!(
                draw.render_queue,
                RenderQueue::Blend | RenderQueue::BlendZWrite
            ) || draw.index_count == 0
            {
                return draw.center;
            }
            let start = draw.index_offset as usize;
            let total = draw.index_count as usize;
            let max_samples = 30;
            let mut sum = glam::Vec3::ZERO;
            if total <= max_samples {
                for &idx in &indices[start..start + total] {
                    sum += glam::Vec3::from(verts[idx as usize].position);
                }
                sum / total as f32
            } else {
                let step = total / max_samples;
                let mut count = 0u32;
                let mut i = 0;
                while i < total {
                    sum += glam::Vec3::from(verts[indices[start + i] as usize].position);
                    count += 1;
                    i += step;
                }
                sum / count as f32
            }
        }));

        work_sorted_indices.clear();
        work_sorted_indices.extend(0..model.draws.len());
        work_sorted_indices.sort_by(|&a, &b| {
            let da = &model.draws[a];
            let db = &model.draws[b];
            da.render_queue
                .cmp(&db.render_queue)
                .then(da.render_queue_offset.cmp(&db.render_queue_offset))
                .then_with(|| {
                    if matches!(
                        da.render_queue,
                        RenderQueue::Blend | RenderQueue::BlendZWrite
                    ) {
                        let za = work_draw_centers[a].distance_squared(eye);
                        let zb = work_draw_centers[b].distance_squared(eye);
                        zb.partial_cmp(&za).unwrap_or(std::cmp::Ordering::Equal)
                    } else {
                        std::cmp::Ordering::Equal
                    }
                })
        });

        Some((eye, draw_count, vert_ptr))
    }

    /// MToon 4-phase drawing (mesh draw for Standard RenderStyle + outline).
    #[allow(clippy::too_many_arguments)]
    fn draw_standard_meshes<'a>(
        pass: &mut wgpu::RenderPass<'a>,
        model: &'a GpuModel,
        params: &RenderParams,
        ps: &'a PipelineSet,
        work_sorted_indices: &[usize],
        camera_bind_group: &'a wgpu::BindGroup,
        default_tex_bind_group: &'a wgpu::BindGroup,
        default_mtoon_aux_bind_group: &'a wgpu::BindGroup,
        mmd_solid: bool,
        mmd_mode: bool,
    ) {
        let use_wireframe =
            params.display.draw_mode == DrawMode::Wireframe && ps.pipeline_wireframe.is_some();
        let use_solid_wire = params.display.draw_mode == DrawMode::SolidWireframe
            && ps.pipeline_wire_overlay.is_some();

        let queue_phases: &[RenderQueue] = &[
            RenderQueue::Opaque,
            RenderQueue::Mask,
            RenderQueue::BlendZWrite,
            RenderQueue::Blend,
        ];

        for target_queue in queue_phases {
            let interleave_outline =
                matches!(target_queue, RenderQueue::Blend | RenderQueue::BlendZWrite);

            for &draw_idx in work_sorted_indices {
                let draw = &model.draws[draw_idx];
                if draw.render_queue != *target_queue {
                    continue;
                }
                if !params
                    .material_visibility
                    .get(draw_idx)
                    .copied()
                    .unwrap_or(true)
                {
                    continue;
                }
                let is_mmd_draw = mmd_solid
                    && draw.render_style == super::mesh::RenderStyle::Mmd
                    && draw.mmd_material_bind_group.is_some();
                if is_mmd_draw {
                    continue;
                }

                if use_wireframe {
                    pass.set_pipeline(
                        ps.pipeline_wireframe
                            .as_ref()
                            .expect("wireframe pipeline already verified by supports_wireframe"),
                    );
                } else {
                    match (draw.render_queue, draw.cull_mode) {
                        (RenderQueue::Opaque, CullMode::Back) => {
                            pass.set_pipeline(&ps.pipeline_cull)
                        }
                        (RenderQueue::Opaque, CullMode::None) => {
                            pass.set_pipeline(&ps.pipeline_no_cull)
                        }
                        (RenderQueue::Opaque, CullMode::Front) => {
                            pass.set_pipeline(&ps.pipeline_front_cull)
                        }
                        (RenderQueue::Mask, CullMode::Back) => {
                            pass.set_pipeline(&ps.pipeline_mask_cull)
                        }
                        (RenderQueue::Mask, CullMode::None) => {
                            pass.set_pipeline(&ps.pipeline_mask_no_cull)
                        }
                        (RenderQueue::Mask, CullMode::Front) => {
                            pass.set_pipeline(&ps.pipeline_mask_front_cull)
                        }
                        (RenderQueue::BlendZWrite, CullMode::Back) => {
                            pass.set_pipeline(&ps.pipeline_alpha_zwrite_cull)
                        }
                        (RenderQueue::BlendZWrite, CullMode::None) => {
                            pass.set_pipeline(&ps.pipeline_alpha_zwrite_no_cull)
                        }
                        (RenderQueue::BlendZWrite, CullMode::Front) => {
                            pass.set_pipeline(&ps.pipeline_alpha_zwrite_front_cull)
                        }
                        (RenderQueue::Blend, CullMode::Back) => {
                            pass.set_pipeline(&ps.pipeline_alpha_cull)
                        }
                        (RenderQueue::Blend, CullMode::None) => {
                            pass.set_pipeline(&ps.pipeline_alpha_no_cull)
                        }
                        (RenderQueue::Blend, CullMode::Front) => {
                            pass.set_pipeline(&ps.pipeline_alpha_front_cull)
                        }
                    }
                }
                pass.set_bind_group(0, camera_bind_group, &[]);
                let tex_bg = draw
                    .texture_bind_group
                    .as_ref()
                    .unwrap_or(default_tex_bind_group);
                pass.set_bind_group(1, tex_bg, &[]);
                pass.set_bind_group(2, &draw.material_bind_group, &[]);
                let mtoon_aux_bg = draw
                    .mtoon_aux_bind_group
                    .as_ref()
                    .unwrap_or(default_mtoon_aux_bind_group);
                pass.set_bind_group(3, mtoon_aux_bg, &[]);

                pass.draw_indexed(
                    draw.index_offset..(draw.index_offset + draw.index_count),
                    0,
                    0..1,
                );

                // BLEND / BlendZWrite: draw the outline immediately after the surface (interleaved).
                if interleave_outline
                    && !use_wireframe
                    && params.display.outline_enabled
                    && params.display.shader_override == ShaderOverride::Default
                    && !mmd_mode
                    && draw.render_style == super::mesh::RenderStyle::Standard
                    && draw.has_outline
                {
                    let outline_pipeline = match draw.render_queue {
                        RenderQueue::Blend => &ps.pipeline_outline_blend,
                        RenderQueue::Mask => &ps.pipeline_outline_mask,
                        _ => &ps.pipeline_outline,
                    };
                    pass.set_pipeline(outline_pipeline);
                    pass.set_vertex_buffer(0, model.vertex_buf.slice(..));
                    pass.set_index_buffer(model.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.set_bind_group(0, camera_bind_group, &[]);
                    let tex_bg = draw
                        .texture_bind_group
                        .as_ref()
                        .unwrap_or(default_tex_bind_group);
                    pass.set_bind_group(1, tex_bg, &[]);
                    pass.set_bind_group(2, &draw.material_bind_group, &[]);
                    let outline_aux_bg = draw
                        .mtoon_aux_bind_group
                        .as_ref()
                        .unwrap_or(default_mtoon_aux_bind_group);
                    pass.set_bind_group(3, outline_aux_bg, &[]);
                    pass.draw_indexed(
                        draw.index_offset..(draw.index_offset + draw.index_count),
                        0,
                        0..1,
                    );
                }
            }

            // OPAQUE / MASK: draw outlines all at once after the phase.
            if !interleave_outline
                && !use_wireframe
                && params.display.outline_enabled
                && params.display.shader_override == ShaderOverride::Default
                && !mmd_mode
            {
                for &draw_idx in work_sorted_indices {
                    let draw = &model.draws[draw_idx];
                    if draw.render_queue != *target_queue {
                        continue;
                    }
                    if !params
                        .material_visibility
                        .get(draw_idx)
                        .copied()
                        .unwrap_or(true)
                    {
                        continue;
                    }
                    if draw.render_style != super::mesh::RenderStyle::Standard {
                        continue;
                    }
                    if !draw.has_outline {
                        continue;
                    }

                    let outline_pipeline = match draw.render_queue {
                        RenderQueue::Mask => &ps.pipeline_outline_mask,
                        _ => &ps.pipeline_outline,
                    };
                    pass.set_pipeline(outline_pipeline);
                    pass.set_vertex_buffer(0, model.vertex_buf.slice(..));
                    pass.set_index_buffer(model.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.set_bind_group(0, camera_bind_group, &[]);
                    let tex_bg = draw
                        .texture_bind_group
                        .as_ref()
                        .unwrap_or(default_tex_bind_group);
                    pass.set_bind_group(1, tex_bg, &[]);
                    pass.set_bind_group(2, &draw.material_bind_group, &[]);
                    let outline_aux_bg = draw
                        .mtoon_aux_bind_group
                        .as_ref()
                        .unwrap_or(default_mtoon_aux_bind_group);
                    pass.set_bind_group(3, outline_aux_bg, &[]);
                    pass.draw_indexed(
                        draw.index_offset..(draw.index_offset + draw.index_count),
                        0,
                        0..1,
                    );
                }
            }
        }

        // Solid+Wire overlay.
        if use_solid_wire {
            let wire_pl = ps
                .pipeline_wire_overlay
                .as_ref()
                .expect("wire_overlay pipeline already verified by supports_wireframe");
            pass.set_pipeline(wire_pl);
            pass.set_bind_group(0, camera_bind_group, &[]);
            for (draw_idx, draw) in model.draws.iter().enumerate() {
                if !params
                    .material_visibility
                    .get(draw_idx)
                    .copied()
                    .unwrap_or(true)
                {
                    continue;
                }
                let tex_bg = draw
                    .texture_bind_group
                    .as_ref()
                    .unwrap_or(default_tex_bind_group);
                pass.set_bind_group(1, tex_bg, &[]);
                pass.set_bind_group(2, &draw.material_bind_group, &[]);
                pass.set_bind_group(3, default_mtoon_aux_bind_group, &[]);
                pass.draw_indexed(
                    draw.index_offset..(draw.index_offset + draw.index_count),
                    0,
                    0..1,
                );
            }
        }
    }

    /// MMD draw pass (material-index order).
    #[allow(clippy::too_many_arguments)]
    fn draw_mmd_meshes<'a>(
        pass: &mut wgpu::RenderPass<'a>,
        model: &'a GpuModel,
        params: &RenderParams,
        ps: &'a PipelineSet,
        camera_bind_group: &'a wgpu::BindGroup,
        default_tex_bind_group: &'a wgpu::BindGroup,
        default_mtoon_aux_bind_group: &'a wgpu::BindGroup,
        default_mmd_aux_bind_group: &'a wgpu::BindGroup,
        mmd_edge_enabled: bool,
    ) {
        pass.set_vertex_buffer(0, model.vertex_buf.slice(..));
        pass.set_index_buffer(model.index_buf.slice(..), wgpu::IndexFormat::Uint32);

        let use_wireframe =
            params.display.draw_mode == DrawMode::Wireframe && ps.pipeline_wireframe.is_some();
        let can_edge =
            mmd_edge_enabled && model.edge_scale_buf.is_some() && ps.pipeline_mmd_edge.is_some();

        for (draw_idx, draw) in model.draws.iter().enumerate() {
            if !params
                .material_visibility
                .get(draw_idx)
                .copied()
                .unwrap_or(true)
            {
                continue;
            }
            if draw.render_style != super::mesh::RenderStyle::Mmd
                || draw.mmd_material_bind_group.is_none()
            {
                continue;
            }

            if use_wireframe {
                pass.set_pipeline(
                    ps.pipeline_wireframe
                        .as_ref()
                        .expect("wireframe pipeline already verified by supports_wireframe"),
                );
            } else {
                let is_no_cull = draw.cull_mode != CullMode::Back;
                let mmd_pipeline = if draw.is_alpha {
                    if is_no_cull {
                        ps.pipeline_mmd_alpha_no_cull.as_ref()
                    } else {
                        ps.pipeline_mmd_alpha_cull.as_ref()
                    }
                } else if is_no_cull {
                    ps.pipeline_mmd_main_no_cull.as_ref()
                } else {
                    ps.pipeline_mmd_main_cull.as_ref()
                };
                let Some(mmd_pipeline) = mmd_pipeline else {
                    continue;
                };
                pass.set_pipeline(mmd_pipeline);
            }
            pass.set_bind_group(0, camera_bind_group, &[]);
            if use_wireframe {
                let tex_bg = draw
                    .texture_bind_group
                    .as_ref()
                    .unwrap_or(default_tex_bind_group);
                pass.set_bind_group(1, tex_bg, &[]);
                pass.set_bind_group(2, &draw.material_bind_group, &[]);
                let mtoon_aux_bg = draw
                    .mtoon_aux_bind_group
                    .as_ref()
                    .unwrap_or(default_mtoon_aux_bind_group);
                pass.set_bind_group(3, mtoon_aux_bg, &[]);
            } else {
                let tex_bg = draw
                    .mmd_texture_bind_group
                    .as_ref()
                    .or(draw.texture_bind_group.as_ref())
                    .unwrap_or(default_tex_bind_group);
                pass.set_bind_group(1, tex_bg, &[]);
                let Some(ref mmd_mat_bg_main) = draw.mmd_material_bind_group else {
                    continue;
                };
                pass.set_bind_group(2, mmd_mat_bg_main, &[]);
                let aux_bg = draw
                    .mmd_aux_bind_group
                    .as_ref()
                    .unwrap_or(default_mmd_aux_bind_group);
                pass.set_bind_group(3, aux_bg, &[]);
            }
            pass.draw_indexed(
                draw.index_offset..(draw.index_offset + draw.index_count),
                0,
                0..1,
            );

            // Draw the edge for opaque materials inline (skipped in Wire mode).
            if !use_wireframe && can_edge && !draw.is_alpha && draw.has_edge {
                if let (Some(ref mmd_mat_bg), Some(edge_scale_buf), Some(edge_pipeline)) = (
                    &draw.mmd_material_bind_group,
                    model.edge_scale_buf.as_ref(),
                    ps.pipeline_mmd_edge.as_ref(),
                ) {
                    pass.set_pipeline(edge_pipeline);
                    pass.set_vertex_buffer(0, model.vertex_buf.slice(..));
                    pass.set_vertex_buffer(1, edge_scale_buf.slice(..));
                    pass.set_index_buffer(model.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.set_bind_group(0, camera_bind_group, &[]);
                    pass.set_bind_group(1, mmd_mat_bg, &[]);
                    pass.draw_indexed(
                        draw.index_offset..(draw.index_offset + draw.index_count),
                        0,
                        0..1,
                    );
                    // Restore the main buffer after edge drawing.
                    pass.set_vertex_buffer(0, model.vertex_buf.slice(..));
                    pass.set_index_buffer(model.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                }
            }
        }
    }

    /// Material hover-highlight draw (orange wireframe).
    fn draw_highlight<'a>(
        pass: &mut wgpu::RenderPass<'a>,
        model: &'a GpuModel,
        params: &RenderParams,
        ps: &'a PipelineSet,
        camera_bind_group: &'a wgpu::BindGroup,
        default_tex_bind_group: &'a wgpu::BindGroup,
        default_mtoon_aux_bind_group: &'a wgpu::BindGroup,
    ) {
        if params.hovered_draw_indices.is_empty() || model.draws.is_empty() {
            return;
        }
        let Some(ref highlight_pl) = ps.pipeline_highlight else {
            return;
        };
        pass.set_pipeline(highlight_pl);
        pass.set_vertex_buffer(0, model.vertex_buf.slice(..));
        pass.set_index_buffer(model.index_buf.slice(..), wgpu::IndexFormat::Uint32);
        pass.set_bind_group(0, camera_bind_group, &[]);
        for &draw_idx in params.hovered_draw_indices {
            if let Some(draw) = model.draws.get(draw_idx) {
                let tex_bg = draw
                    .texture_bind_group
                    .as_ref()
                    .unwrap_or(default_tex_bind_group);
                pass.set_bind_group(1, tex_bg, &[]);
                pass.set_bind_group(2, &draw.material_bind_group, &[]);
                pass.set_bind_group(3, default_mtoon_aux_bind_group, &[]);
                pass.draw_indexed(
                    draw.index_offset..(draw.index_offset + draw.index_count),
                    0,
                    0..1,
                );
            }
        }
    }

    /// Pass 2: grid + visualization overlays (normals / bones / rigid bodies / joints).
    fn draw_overlays<'p>(
        &'p self,
        pass: &mut wgpu::RenderPass<'p>,
        params: &RenderParams,
        ps: &'p PipelineSet,
    ) {
        // Grid draw.
        if params.display.show_grid {
            pass.set_pipeline(&ps.pipeline_grid);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_vertex_buffer(0, self.grid_vbuf.slice(..));
            pass.draw(0..self.grid_vertex_count, 0..1);
        }

        // Normal display (LineList overlay).
        if params.display.show_normals && self.normal.vertex_count > 0 {
            if let Some(ref buf) = self.normal.buf {
                pass.set_pipeline(&ps.pipeline_line_overlay);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..self.normal.vertex_count, 0..1);
            }
        }

        // Bone draw (3 phases: tail -> fill -> outline).
        if params.display.show_bones {
            if self.bone_tail.vertex_count > 0 {
                if let Some(ref buf) = self.bone_tail.buf {
                    pass.set_pipeline(&ps.pipeline_line_overlay);
                    pass.set_bind_group(0, &self.camera_bind_group, &[]);
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw(0..self.bone_tail.vertex_count, 0..1);
                }
            }
            if self.bone_fill.vertex_count > 0 {
                if let Some(ref buf) = self.bone_fill.buf {
                    pass.set_pipeline(&ps.pipeline_bone);
                    pass.set_bind_group(0, &self.camera_bind_group, &[]);
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw(0..self.bone_fill.vertex_count, 0..1);
                }
            }
            if self.bone_line.vertex_count > 0 {
                if let Some(ref buf) = self.bone_line.buf {
                    pass.set_pipeline(&ps.pipeline_line_overlay);
                    pass.set_bind_group(0, &self.camera_bind_group, &[]);
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw(0..self.bone_line.vertex_count, 0..1);
                }
            }
        }

        // Rigid-body draw.
        if params.display.show_spring_bones && self.spring.vertex_count > 0 {
            if let Some(ref buf) = self.spring.buf {
                pass.set_pipeline(&ps.pipeline_line_overlay);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..self.spring.vertex_count, 0..1);
            }
        }

        // Joint draw.
        if params.display.show_joints {
            if self.joint.vertex_count > 0 {
                if let Some(ref buf) = self.joint.buf {
                    pass.set_pipeline(&ps.pipeline_bone);
                    pass.set_bind_group(0, &self.camera_bind_group, &[]);
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw(0..self.joint.vertex_count, 0..1);
                }
            }
            if self.joint_edge.vertex_count > 0 {
                if let Some(ref buf) = self.joint_edge.buf {
                    pass.set_pipeline(&ps.pipeline_line_overlay);
                    pass.set_bind_group(0, &self.camera_bind_group, &[]);
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw(0..self.joint_edge.vertex_count, 0..1);
                }
            }
        }
    }

    /// Build MMD GPU resources on `DrawCall`s (called from all GPU-model creation paths).
    pub fn prepare_mmd_resources(
        &self,
        device: &wgpu::Device,
        model: &mut GpuModel,
        ir: &IrModel,
        emissive_per_mat: &[bool],
    ) {
        use super::mesh::RenderStyle;

        // Take `draws` temporarily to avoid borrow conflicts.
        let mut draws = std::mem::take(&mut model.draws);
        let gpu_textures_unorm = &model.gpu_texture_views_unorm;

        for draw in &mut draws {
            if draw.render_style != RenderStyle::Mmd {
                continue;
            }
            self.rebuild_mmd_for_draw(device, draw, ir, gpu_textures_unorm, emissive_per_mat);
        }

        model.draws = draws;
    }

    /// Rebuild the 3 bind groups (material / texture / aux) of the
    /// MMD-compatible path for a single `DrawCall` (§C).
    ///
    /// Called from both the bulk processing in `prepare_mmd_resources` and the
    /// per-material path in `rebuild_material_bind_groups`. Early-returns
    /// without doing anything if not `RenderStyle::Mmd`.
    fn rebuild_mmd_for_draw(
        &self,
        device: &wgpu::Device,
        draw: &mut DrawCall,
        ir: &IrModel,
        gpu_textures_unorm: &[wgpu::TextureView],
        emissive_per_mat: &[bool],
    ) {
        use super::mesh::RenderStyle;
        if draw.render_style != RenderStyle::Mmd {
            return;
        }

        let mat = &ir.materials[draw.material_index];
        let tex_sampler = &self.default_sampler;

        // MmdMaterialUniform
        let mut flags = 0u32;
        if mat.sphere_texture_index.is_some() && mat.sphere_mode > 0 {
            flags |= 1; // has_sphere
            if mat.sphere_mode == 2 {
                flags |= 2; // sphere_add
            }
        }
        if mat.toon_texture_index.is_some() || mat.toon_shared_index.is_some() {
            flags |= 4; // has_toon
        }

        let bloom_emissive = if emissive_per_mat
            .get(draw.material_index)
            .copied()
            .unwrap_or(true)
        {
            super::bloom::derive_pmx_bloom(mat).0
        } else {
            [0.0; 3]
        };

        let uniform = MmdMaterialUniform {
            ambient: mat.ambient.to_array(),
            alpha: mat.diffuse.w.clamp(0.0, 1.0),
            specular: mat.specular.to_array(),
            specular_power: mat.specular_power,
            diffuse_rgb: [mat.diffuse.x, mat.diffuse.y, mat.diffuse.z],
            flags,
            edge_color: mat.edge_color.to_array(),
            edge_size: mat.edge_size,
            bloom_emissive,
        };

        let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mmd_mat_uniform"),
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mmd_mat_bg"),
            layout: &self.mmd_material_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buf.as_entire_binding(),
            }],
        });

        draw.mmd_material_buf = Some(buf);
        draw.mmd_material_bind_group = Some(bind_group);

        // MMD main texture bind group (Unorm view).
        let mmd_tex_bg = mat.texture_index.and_then(|ti| {
            gpu_textures_unorm.get(ti).map(|unorm_view| {
                create_texture_bind_group(device, &self.texture_bgl, unorm_view, tex_sampler)
            })
        });
        draw.mmd_texture_bind_group = mmd_tex_bg;

        // sphere / toon aux bind group (Unorm view).
        let sphere_view = mat
            .sphere_texture_index
            .and_then(|i| gpu_textures_unorm.get(i));
        let toon_view = mat
            .toon_texture_index
            .and_then(|i| gpu_textures_unorm.get(i))
            .or_else(|| {
                mat.toon_shared_index
                    .map(|i| &self.shared_toon_textures_unorm[i as usize])
            });

        // Fallback to `shared_toon_textures_unorm[0]` (white gradient) when sphere / toon is absent.
        let sv = sphere_view.unwrap_or(&self.shared_toon_textures_unorm[0]);
        let tv = toon_view.unwrap_or(&self.shared_toon_textures_unorm[0]);

        let aux_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mmd_aux_bg"),
            layout: &self.mmd_aux_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(sv),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.shared_toon_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(tv),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.shared_toon_sampler),
                },
            ],
        });
        draw.mmd_aux_bind_group = Some(aux_bg);
    }

    /// On a dirty notification from the material edit drawer, rebuild all
    /// bind groups for one material (§C).
    ///
    /// Updates both the standard path (`material_bind_group` +
    /// `mtoon_aux_bind_group`) and the MMD-compatible path
    /// (`mmd_material` / `mmd_texture` / `mmd_aux` - 3 groups) at the same
    /// time. Always runs both so edits don't disappear on one path when
    /// `use_mmd_path` is toggled.
    ///
    /// The current `DrawCall` only holds `wgpu::BindGroup` and does not have
    /// a `wgpu::Buffer` handle, so structurally `queue.write_buffer` partial
    /// updates are impossible. Decided to rebuild whole bind groups instead
    /// (documented in plan §C; future optimization could add
    /// `DrawCall.material_buf` in a separate PR).
    /// Reflects material-parameter changes on the GPU.
    ///
    /// - `uniform_only = true`: only color / scalar values change. Performs a
    ///   partial update via `queue.write_buffer` and skips bind-group
    ///   rebuilds (used for material-editor slider edits and Expression
    ///   material bind).
    /// - `uniform_only = false`: includes texture changes. Rebuilds the bind
    ///   group + aux bind group.
    #[allow(clippy::too_many_arguments)]
    pub fn rebuild_material_bind_groups(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        model: &mut GpuModel,
        ir: &IrModel,
        mat_idx: usize,
        flags: &MaterialBuildFlags,
        uniform_only: bool,
    ) {
        use super::mesh::{
            build_aux_refs_for, build_material_params_for, rebuild_mtoon_aux_bind_group,
            RenderStyle,
        };

        if mat_idx >= ir.materials.len() {
            return;
        }
        let mat = &ir.materials[mat_idx];

        // §C: compute params / aux_refs as pure functions.
        let params = build_material_params_for(mat, mat_idx, flags);

        if uniform_only {
            // Fast path: buffer write only; no bind-group rebuild.
            for draw in &mut model.draws {
                if draw.material_index == mat_idx {
                    write_material_buffer(queue, &draw.material_buf, &params);
                }
            }
            return;
        }

        // Full path: rebuild bind groups (texture changes).
        let aux_refs = build_aux_refs_for(mat);

        // Take `draws` temporarily to avoid borrow conflicts (same pattern as `prepare_mmd_resources`).
        let mut draws = std::mem::take(&mut model.draws);
        let gpu_texture_views = &model.gpu_texture_views;
        let gpu_texture_views_unorm = &model.gpu_texture_views_unorm;

        for draw in &mut draws {
            if draw.material_index != mat_idx {
                continue;
            }

            // Standard path: update the material uniform buffer.
            write_material_buffer(queue, &draw.material_buf, &params);

            // v0.5.1 review [P1]: also rebuild the standard path's
            // `texture_bind_group` (BaseColor). The old implementation only
            // updated aux / mmd, so pristine restoration and `texture_index`
            // changes were not reflected on the GPU; the previous BaseColor
            // texture bind remained on screen.
            //
            // Review 04 [P1]: align the source of truth with the initial
            // `DrawCall` construction (mesh.rs:1256). The old implementation
            // only referenced `mat.base_color_tex_info`, but PMX / PMD
            // materials have `texture_index` while `base_color_tex_info` is
            // None, so a full rebuild left `texture_bind_group = None` and
            // regressed to a white texture. Fix: prefer `texture_index`, and
            // for the sampler use `base_color_tex_info.sampler` if present;
            // otherwise fall back to the default `IrSamplerInfo`.
            draw.texture_bind_group = mat.texture_index.and_then(|tex_idx| {
                gpu_texture_views.get(tex_idx).map(|srgb_view| {
                    let sampler_info = mat
                        .base_color_tex_info
                        .as_ref()
                        .map(|ti| ti.sampler)
                        .unwrap_or_default();
                    let sampler = super::mesh::create_sampler_from_info(device, &sampler_info);
                    create_texture_bind_group(device, &self.texture_bgl, srgb_view, &sampler)
                })
            });

            // Standard path: `mtoon_aux_bind_group` (only for materials whose `aux_refs` is `Some`).
            if let Some(refs) = &aux_refs {
                draw.mtoon_aux_bind_group = Some(rebuild_mtoon_aux_bind_group(
                    device,
                    &self.mtoon_aux_bgl,
                    refs,
                    gpu_texture_views,
                    gpu_texture_views_unorm,
                    &self.default_views,
                ));
            } else {
                draw.mtoon_aux_bind_group = None;
            }

            // MMD-compatible path: only `RenderStyle::Mmd` (same condition as `prepare_mmd_resources`).
            if draw.render_style == RenderStyle::Mmd {
                self.rebuild_mmd_for_draw(
                    device,
                    draw,
                    ir,
                    gpu_texture_views_unorm,
                    &flags.emissive,
                );
            }
        }

        model.draws = draws;
    }

    /// Reference to the MMD material BGL (for external use).
    pub fn mmd_material_bgl(&self) -> &wgpu::BindGroupLayout {
        &self.mmd_material_bgl
    }

    /// Reference to the default MMD aux bind group.
    pub fn default_mmd_aux_bind_group(&self) -> &wgpu::BindGroup {
        &self.default_mmd_aux_bind_group
    }
}

/// Decide whether the frame fully runs on the MMD-dedicated path.
/// When true, use the Unorm render target for accurate gamma-space rendering.
fn can_use_unorm_frame(model: &GpuModel, visible: &[bool], mmd_solid: bool) -> bool {
    if !mmd_solid {
        return false;
    }
    let mut has_visible_mmd = false;
    for (i, draw) in model.draws.iter().enumerate() {
        if !visible.get(i).copied().unwrap_or(true) {
            continue;
        }
        match draw.render_style {
            super::mesh::RenderStyle::Mmd if draw.mmd_material_bind_group.is_some() => {
                has_visible_mmd = true;
            }
            _ => return false,
        }
    }
    has_visible_mmd
}

/// Canonical sRGB conversion (f64 precision; for clear-color compensation).
fn linear_to_srgb_f64(v: f64) -> f64 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// Create a 1x1 white-texture bind group.
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

/// Create a texture bind group (for external callers).
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

/// Create the material bind group.
/// Pack UV parameters for `MaterialUniform` from `IrTextureInfo`.
/// Return value: ([tex_coord, offset.x, offset.y, rotation], [scale.x, scale.y, 0, 0]).
pub fn pack_uv_params(
    info: Option<&crate::intermediate::types::IrTextureInfo>,
) -> ([f32; 4], [f32; 4]) {
    match info {
        Some(ti) => (
            [ti.tex_coord as f32, ti.offset.x, ti.offset.y, ti.rotation],
            [ti.scale.x, ti.scale.y, 0.0, 0.0],
        ),
        None => ([0.0, 0.0, 0.0, 0.0], [1.0, 1.0, 0.0, 0.0]),
    }
}

/// Material parameters passed to `create_material_bind_group`.
pub struct MaterialParams {
    pub diffuse: [f32; 4],
    pub shade_color: [f32; 3],
    pub is_mtoon: bool,
    pub shading_toony: f32,
    pub shading_shift: f32,
    pub outline_width: f32,
    pub outline_mode: f32,
    pub outline_color: [f32; 4],
    pub outline_lighting_mix: f32,
    pub rim_color: [f32; 3],
    pub rim_fresnel_power: f32,
    pub rim_lift: f32,
    pub rim_lighting_mix: f32,
    pub has_matcap: bool,
    pub matcap_factor: [f32; 3],
    pub has_shade_multiply_tex: bool,
    pub has_shading_shift_tex: bool,
    pub shading_shift_tex_scale: f32,
    pub has_rim_multiply_tex: bool,
    pub uv_anim_scroll_x: f32,
    pub uv_anim_scroll_y: f32,
    pub uv_anim_rotation: f32,
    pub has_uv_anim_mask: bool,
    pub alpha_cutoff: f32,
    pub base_uv: ([f32; 4], [f32; 4]),
    pub shade_uv: ([f32; 4], [f32; 4]),
    pub shift_uv: ([f32; 4], [f32; 4]),
    pub rim_uv: ([f32; 4], [f32; 4]),
    pub outline_uv: ([f32; 4], [f32; 4]),
    pub uv_mask_uv: ([f32; 4], [f32; 4]),
    pub emissive_factor: [f32; 3],
    pub has_emissive_tex: bool,
    pub emissive_uv: ([f32; 4], [f32; 4]),
    pub has_normal_tex: bool,
    pub normal_scale: f32,
    pub normal_uv: ([f32; 4], [f32; 4]),
    pub gi_equalization_factor: f32,
    pub outline_width_channel: f32,
    pub uv_anim_mask_channel: f32,
    pub matcap_uv: ([f32; 4], [f32; 4]),
}

/// Convert `MaterialParams` to `MaterialUniform` and serialize via encase.
/// Shared path for `create_material_buffer_and_bind_group` /
/// `write_material_buffer`.
pub fn serialize_material_uniform(params: &MaterialParams) -> Vec<u8> {
    let p = params;
    let uniform = MaterialUniform {
        diffuse: p.diffuse.into(),
        shade_color: p.shade_color.into(),
        is_mtoon: b2f(p.is_mtoon),
        shading_toony: p.shading_toony,
        shading_shift: p.shading_shift,
        outline_width: p.outline_width,
        outline_mode: p.outline_mode,
        outline_color: p.outline_color.into(),
        outline_lighting_mix: p.outline_lighting_mix,
        rim_color: p.rim_color.into(),
        rim_fresnel_power: p.rim_fresnel_power,
        rim_lift: p.rim_lift,
        rim_lighting_mix: p.rim_lighting_mix,
        has_matcap: b2f(p.has_matcap),
        matcap_factor: p.matcap_factor.into(),
        has_shade_multiply_tex: b2f(p.has_shade_multiply_tex),
        has_shading_shift_tex: b2f(p.has_shading_shift_tex),
        shading_shift_tex_scale: p.shading_shift_tex_scale,
        has_rim_multiply_tex: b2f(p.has_rim_multiply_tex),
        uv_anim_scroll_x: p.uv_anim_scroll_x,
        uv_anim_scroll_y: p.uv_anim_scroll_y,
        uv_anim_rotation: p.uv_anim_rotation,
        has_uv_anim_mask: b2f(p.has_uv_anim_mask),
        alpha_cutoff: p.alpha_cutoff,
        base_uv_a: p.base_uv.0.into(),
        base_uv_b: p.base_uv.1.into(),
        shade_uv_a: p.shade_uv.0.into(),
        shade_uv_b: p.shade_uv.1.into(),
        shift_uv_a: p.shift_uv.0.into(),
        shift_uv_b: p.shift_uv.1.into(),
        rim_uv_a: p.rim_uv.0.into(),
        rim_uv_b: p.rim_uv.1.into(),
        outline_uv_a: p.outline_uv.0.into(),
        outline_uv_b: p.outline_uv.1.into(),
        uv_mask_uv_a: p.uv_mask_uv.0.into(),
        uv_mask_uv_b: p.uv_mask_uv.1.into(),
        emissive_factor: p.emissive_factor.into(),
        has_emissive_tex: b2f(p.has_emissive_tex),
        emissive_uv_a: p.emissive_uv.0.into(),
        emissive_uv_b: p.emissive_uv.1.into(),
        has_normal_tex: b2f(p.has_normal_tex),
        normal_scale: p.normal_scale,
        gi_equalization_factor: p.gi_equalization_factor,
        outline_width_channel: p.outline_width_channel,
        normal_uv_a: p.normal_uv.0.into(),
        normal_uv_b: p.normal_uv.1.into(),
        uv_anim_mask_channel: p.uv_anim_mask_channel,
        matcap_uv_a: p.matcap_uv.0.into(),
        matcap_uv_b: p.matcap_uv.1.into(),
    };
    let mut encase_buf = encase::UniformBuffer::new(Vec::new());
    encase_buf.write(&uniform).expect("encase write");
    encase_buf.into_inner()
}

/// Create the material uniform buffer (`UNIFORM | COPY_DST`) and its bind
/// group at the same time. Used at model load. The returned `wgpu::Buffer`
/// is stored in `DrawCall.material_buf` and supports per-frame partial
/// updates via `write_material_buffer`.
pub fn create_material_buffer_and_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    params: &MaterialParams,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    let bytes = serialize_material_uniform(params);
    let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("material_uniform"),
        contents: &bytes,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("material_bg"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buf.as_entire_binding(),
        }],
    });
    (buf, bg)
}

/// Write new parameters into an existing material uniform buffer.
/// Updates GPU-side material parameters without rebuilding the bind group.
/// Used by Expression material bind and the material editor for color /
/// scalar edits.
pub fn write_material_buffer(queue: &wgpu::Queue, buf: &wgpu::Buffer, params: &MaterialParams) {
    let bytes = serialize_material_uniform(params);
    queue.write_buffer(buf, 0, &bytes);
}

/// Backward-compatibility shim: keeps the `create_material_bind_group`
/// signature. New code should use `create_material_buffer_and_bind_group`.
pub fn create_material_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    params: &MaterialParams,
) -> wgpu::BindGroup {
    let (_, bg) = create_material_buffer_and_bind_group(device, layout, params);
    bg
}

/// Create the MToon auxiliary texture bind group layout (group 3).
/// Has a sampler per texture (matches glTF's per-texture sampler model).
/// binding 2n: sampler, binding 2n+1: texture_2d (8 textures * 2 = 16 bindings).
fn create_mtoon_aux_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let frag = wgpu::ShaderStages::FRAGMENT;
    let vert_frag = wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::VERTEX;
    let vert = wgpu::ShaderStages::VERTEX;

    let sampler_entry = |binding: u32, vis: wgpu::ShaderStages| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: vis,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    };
    let tex_entry = |binding: u32, vis: wgpu::ShaderStages| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: vis,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("mtoon_aux_bgl"),
        entries: &[
            sampler_entry(0, frag),      // s_matcap
            tex_entry(1, frag),          // t_matcap
            sampler_entry(2, frag),      // s_shade_multiply
            tex_entry(3, frag),          // t_shade_multiply
            sampler_entry(4, frag),      // s_shading_shift
            tex_entry(5, frag),          // t_shading_shift
            sampler_entry(6, frag),      // s_rim_multiply
            tex_entry(7, frag),          // t_rim_multiply
            sampler_entry(8, vert_frag), // s_uv_anim_mask (also referenced from the vertex shader)
            tex_entry(9, vert_frag),     // t_uv_anim_mask
            sampler_entry(10, vert),     // s_outline_width (vertex shader only)
            tex_entry(11, vert),         // t_outline_width
            sampler_entry(12, frag),     // s_emissive
            tex_entry(13, frag),         // t_emissive
            sampler_entry(14, frag),     // s_normal
            tex_entry(15, frag),         // t_normal
        ],
    })
}

/// One auxiliary texture (texture view + sampler).
pub struct AuxTexEntry<'a> {
    pub view: &'a wgpu::TextureView,
    pub sampler: &'a wgpu::Sampler,
}

/// Create the MToon auxiliary texture bind group (one sampler per texture).
#[allow(clippy::too_many_arguments)]
pub fn create_mtoon_aux_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    matcap: AuxTexEntry<'_>,
    shade_multiply: AuxTexEntry<'_>,
    shading_shift: AuxTexEntry<'_>,
    rim_multiply: AuxTexEntry<'_>,
    uv_anim_mask: AuxTexEntry<'_>,
    outline_width: AuxTexEntry<'_>,
    emissive: AuxTexEntry<'_>,
    normal: AuxTexEntry<'_>,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("mtoon_aux_bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Sampler(matcap.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(matcap.view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(shade_multiply.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(shade_multiply.view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(shading_shift.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(shading_shift.view),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::Sampler(rim_multiply.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::TextureView(rim_multiply.view),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: wgpu::BindingResource::Sampler(uv_anim_mask.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 9,
                resource: wgpu::BindingResource::TextureView(uv_anim_mask.view),
            },
            wgpu::BindGroupEntry {
                binding: 10,
                resource: wgpu::BindingResource::Sampler(outline_width.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 11,
                resource: wgpu::BindingResource::TextureView(outline_width.view),
            },
            wgpu::BindGroupEntry {
                binding: 12,
                resource: wgpu::BindingResource::Sampler(emissive.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 13,
                resource: wgpu::BindingResource::TextureView(emissive.view),
            },
            wgpu::BindGroupEntry {
                binding: 14,
                resource: wgpu::BindingResource::Sampler(normal.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 15,
                resource: wgpu::BindingResource::TextureView(normal.view),
            },
        ],
    })
}

/// Public wrapper that creates the MToon auxiliary texture bind group layout.
pub fn create_mtoon_aux_bind_group_layout_pub(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    create_mtoon_aux_bind_group_layout(device)
}

/// Create the sRGB `TextureView` for a 1x1 white texture (default for the MToon aux bind group).
pub fn create_white_texture_view_srgb(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> wgpu::TextureView {
    let (srgb, _) = create_white_texture_view(device, queue);
    srgb
}

/// Create the `TextureView` for a 1x1 black texture (public version).
pub fn create_black_texture_view_pub(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> wgpu::TextureView {
    create_black_texture_view(device, queue)
}

/// Create the `TextureView` for a 1x1 white texture (MMD default).
/// Return value: (sRGB view, Unorm view).
fn create_white_texture_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> (wgpu::TextureView, wgpu::TextureView) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("white_1x1_view"),
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
        view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
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
    let srgb_view = tex.create_view(&Default::default());
    let unorm_view = tex.create_view(&wgpu::TextureViewDescriptor {
        format: Some(wgpu::TextureFormat::Rgba8Unorm),
        ..Default::default()
    });
    (srgb_view, unorm_view)
}

/// Create the `TextureView` for a 1x1 black texture (MatCap default: RGB=0 disables it).
fn create_black_texture_view(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("black_1x1_view"),
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
        &[0u8, 0, 0, 255],
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
    tex.create_view(&Default::default())
}

/// Create the Unorm `TextureView` for a 1x1 flat-normal texture.
/// tangent-space (0, 0, 1) = RGBA(128, 128, 255, 255) - equivalent to no normal map.
pub fn create_flat_normal_texture_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("flat_normal_1x1"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
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
        &[128u8, 128, 255, 255],
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
    tex.create_view(&Default::default())
}

/// Generate shared toon textures (toon01-10) on the CPU and upload to GPU.
/// Return value: (array of sRGB views, array of Unorm views).
///
/// Faithfully reproduces the per-row, leftmost-pixel color of MMD's standard
/// toon01-10.bmp (32x32 px). Shaders sample at fixed U=0.0, so column-wise
/// color differences are negligible.
fn generate_shared_toon_textures(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> ([wgpu::TextureView; 10], [wgpu::TextureView; 10]) {
    // Per-row RGB values of MMD's standard toon (row 0 = top, row 31 = bottom).
    // ---------------------------------------------------------------
    // toon01-04: row 0-15 = white, row 16-31 = shadow color (2-tone step).
    // toon05-06: MMD reference data (irregular gradient, LUT).
    // toon07-10: all white (no toon effect).
    // ---------------------------------------------------------------

    /// Produce a 2-tone step texture (top half = white, bottom half = shadow).
    const fn toon_step(shadow: [u8; 3]) -> [[u8; 3]; 32] {
        let mut rows = [[255u8, 255, 255]; 32];
        let mut i = 16;
        while i < 32 {
            rows[i] = shadow;
            i += 1;
        }
        rows
    }

    /// Expand a 32-row texture from a run-length list of (color, count).
    const fn toon_rle<const N: usize>(runs: [([u8; 3], u8); N]) -> [[u8; 3]; 32] {
        let mut rows = [[0u8; 3]; 32];
        let mut pos = 0usize;
        let mut r = 0;
        while r < N {
            let (color, count) = runs[r];
            let mut c = 0u8;
            while c < count {
                rows[pos] = color;
                pos += 1;
                c += 1;
            }
            r += 1;
        }
        rows
    }

    const TOON01: [[u8; 3]; 32] = toon_step([205, 205, 205]); // white -> gray
    const TOON02: [[u8; 3]; 32] = toon_step([245, 225, 225]); // white -> pinkish
    const TOON03: [[u8; 3]; 32] = toon_step([154, 154, 154]); // white -> dark gray
    const TOON04: [[u8; 3]; 32] = toon_step([248, 239, 235]); // white -> warm beige

    // toon05: gradient white -> warm pink (MMD reference LUT).
    #[rustfmt::skip]
    const TOON05: [[u8; 3]; 32] = toon_rle([
        ([255,255,255], 19), ([255,254,254], 1), ([255,250,248], 1),
        ([255,246,242], 1),  ([255,240,234], 1), ([255,236,229], 1),
        ([255,233,224], 1),  ([255,231,222], 1), ([255,231,221], 4),
        ([255,231,222], 1),  ([254,232,223], 1),
    ]);

    // toon06: yellowish - central highlight band + dark yellow (MMD reference LUT).
    #[rustfmt::skip]
    const TOON06: [[u8; 3]; 32] = toon_rle([
        ([255,237, 97], 8),  ([255,238,106], 1), ([255,246,175], 1),
        ([255,254,242], 1),  ([255,242,138], 1), ([255,237, 97], 10),
        ([254,235, 94], 1),  ([238,218, 69], 1), ([209,187, 24], 1),
        ([197,174,  6], 1),  ([195,172,  3], 6),
    ]);

    const TOON_WHITE: [[u8; 3]; 32] = [[255, 255, 255]; 32];

    let toon_data: [&[[u8; 3]; 32]; 10] = [
        &TOON01,
        &TOON02,
        &TOON03,
        &TOON04,
        &TOON05,
        &TOON06,
        &TOON_WHITE,
        &TOON_WHITE,
        &TOON_WHITE,
        &TOON_WHITE,
    ];

    let width = 1u32;
    let height = 32u32;
    let mut views_srgb: Vec<wgpu::TextureView> = Vec::with_capacity(10);
    let mut views_unorm: Vec<wgpu::TextureView> = Vec::with_capacity(10);

    for (i, rows) in toon_data.iter().enumerate() {
        let mut rgba = Vec::with_capacity((height * 4) as usize);
        for row in rows.iter() {
            rgba.extend_from_slice(&[row[0], row[1], row[2], 255]);
        }

        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("shared_toon_{:02}", i + 1)),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        views_srgb.push(tex.create_view(&Default::default()));
        views_unorm.push(tex.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Rgba8Unorm),
            ..Default::default()
        }));
    }

    (
        views_srgb.try_into().expect("10 toon textures (srgb)"),
        views_unorm.try_into().expect("10 toon textures (unorm)"),
    )
}

/// Bone shape kind (priority: IK > axis-fixed > movable > normal).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum BoneShape {
    Normal,    // double circle (center filled)
    Move,      // square (center filled)
    AxisFixed, // circle + X
    Ik,        // blue outer outline + orange fill + blue center square
}

/// Generate bone-display geometry (every frame; camera-facing billboards).
/// `out_tails`: for LineList (tail triangles) - backmost.
/// `out_fill`:  for TriangleList (marker fill faces) - middle.
/// `out_lines`: for LineList (marker outlines / X) - frontmost.
#[allow(clippy::too_many_arguments)]
fn generate_bone_vertices(
    out_tails: &mut Vec<GridVertex>,
    out_fill: &mut Vec<GridVertex>,
    out_lines: &mut Vec<GridVertex>,
    ir: &IrModel,
    pos_fn: fn(Vec3) -> Vec3,
    camera_eye: Vec3,
    opacity: f32,
    animated_globals: Option<&[glam::Mat4]>,
) {
    out_tails.clear();
    out_fill.clear();
    out_lines.clear();
    let blue = [0.0, 0.0, 1.0, opacity];
    let orange = [1.0, 0.4, 0.0, opacity]; // dark orange #ff6600
    let outer_factor = BONE_DISPLAY_RADIUS;
    let inner_factor = BONE_JOINT_RADIUS;
    let ik_center_factor = inner_factor; // blue square at IK-controller center (same size as movable bones)
    let segments = SPHERE_SEGMENTS;

    // Draw priority: normal -> IK-influenced (orange) -> axis-fixed -> IK controller.
    // (Drawn later -> shown on top.)
    for pass in 0..4u8 {
        for (bone_i, bone) in ir.bones.iter().enumerate() {
            if !bone.is_visible {
                continue;
            }

            // Decide the shape (priority: IK > axis-fixed > movable > normal).
            let shape = if bone.is_ik_bone {
                BoneShape::Ik
            } else if bone.is_axis_fixed {
                BoneShape::AxisFixed
            } else if bone.is_translatable {
                BoneShape::Move
            } else {
                BoneShape::Normal
            };

            // Pass dispatch: 0 = normal, 1 = IK-influenced (orange), 2 = axis-fixed, 3 = IK controller.
            let bone_pass = if shape == BoneShape::Ik {
                3
            } else if shape == BoneShape::AxisFixed {
                2
            } else if bone.is_ik {
                1
            } else {
                0
            };
            if pass != bone_pass {
                continue;
            }

            let pos = if let Some(globals) = animated_globals {
                if bone_i < globals.len() {
                    pos_fn(globals[bone_i].transform_point3(Vec3::ZERO))
                } else {
                    pos_fn(bone.position)
                }
            } else {
                pos_fn(bone.position)
            };

            // IK-influenced bones in orange; everything else in blue.
            let color = if bone.is_ik && shape != BoneShape::Ik {
                orange
            } else {
                blue
            };

            let to_cam_vec = camera_eye - pos;
            let dist = to_cam_vec.length().max(0.1);
            let to_cam = to_cam_vec / dist;
            let (right, up) = billboard_axes(to_cam);
            let r_outer = dist * outer_factor;
            let r_inner = dist * inner_factor;
            let r_ik_center = dist * ik_center_factor;

            // Center fill color: blue even for IK-influenced bones.
            let center_color = blue;

            // Triangle (lines): self -> tail / parent -> self - drawn before markers.
            if let Some(tbi) = bone.tail_bone_index {
                let tail_pos = if let Some(globals) = animated_globals {
                    if tbi < globals.len() {
                        pos_fn(globals[tbi].transform_point3(Vec3::ZERO))
                    } else {
                        pos_fn(bone.tail_position.unwrap_or(bone.position))
                    }
                } else {
                    pos_fn(bone.tail_position.unwrap_or(bone.position))
                };
                draw_bone_triangle(out_tails, pos, tail_pos, camera_eye, outer_factor, color);
            } else if let Some(tail_gltf) = bone.tail_position {
                let tail_pos = pos_fn(tail_gltf);
                draw_bone_triangle(out_tails, pos, tail_pos, camera_eye, outer_factor, color);
            } else if let Some(parent_idx) = bone.parent {
                if parent_idx < ir.bones.len() {
                    let parent_pos = if let Some(globals) = animated_globals {
                        if parent_idx < globals.len() {
                            pos_fn(globals[parent_idx].transform_point3(Vec3::ZERO))
                        } else {
                            pos_fn(ir.bones[parent_idx].position)
                        }
                    } else {
                        pos_fn(ir.bones[parent_idx].position)
                    };
                    draw_bone_triangle(out_tails, parent_pos, pos, camera_eye, outer_factor, color);
                }
            }

            // Offset for outline thickness (draw twice for 2x thickness).
            let thick = dist * 0.0003;

            // Marker drawing (per shape) - drawn on top of tails.
            // Order: fill (TriangleList) -> line (LineList). The fill spans the
            // inner circle / inner square, and the outline overlays on top.
            match shape {
                BoneShape::Normal => {
                    // double circle: filled inner circle + thick outer circle / inner-circle line.
                    draw_filled_circle_tri(
                        out_fill,
                        pos,
                        right,
                        up,
                        r_inner,
                        segments,
                        center_color,
                    );
                    draw_circle(out_lines, pos, right, up, r_outer - thick, segments, color);
                    draw_circle(out_lines, pos, right, up, r_outer + thick, segments, color);
                    draw_circle(out_lines, pos, right, up, r_inner, segments, color);
                }
                BoneShape::Move => {
                    // square: filled inner square + thick outer square / inner-square line.
                    draw_filled_square_tri(out_fill, pos, right, up, r_inner, center_color);
                    draw_square(out_lines, pos, right, up, r_outer - thick, color);
                    draw_square(out_lines, pos, right, up, r_outer + thick, color);
                    draw_square(out_lines, pos, right, up, r_inner, color);
                }
                BoneShape::AxisFixed => {
                    // circle + X: thick outer circle + thick X spanning the outer circle.
                    draw_circle(out_lines, pos, right, up, r_outer - thick, segments, color);
                    draw_circle(out_lines, pos, right, up, r_outer + thick, segments, color);
                    let d = r_outer * 0.707;
                    let x_thick = thick * 0.707;
                    let diag1 = (right - up).normalize_or_zero() * x_thick;
                    let diag2 = (right + up).normalize_or_zero() * x_thick;
                    push_line(
                        out_lines,
                        pos + (-right + up) * d - diag1,
                        pos + (right - up) * d - diag1,
                        color,
                    );
                    push_line(
                        out_lines,
                        pos + (-right + up) * d + diag1,
                        pos + (right - up) * d + diag1,
                        color,
                    );
                    push_line(
                        out_lines,
                        pos + (right + up) * d - diag2,
                        pos + (-right - up) * d - diag2,
                        color,
                    );
                    push_line(
                        out_lines,
                        pos + (right + up) * d + diag2,
                        pos + (-right - up) * d + diag2,
                        color,
                    );
                }
                BoneShape::Ik => {
                    // IK controller: orange fill spanning the outline + blue center fill + thick blue outer outline.
                    draw_filled_square_tri(out_fill, pos, right, up, r_outer, orange);
                    draw_filled_square_tri(out_fill, pos, right, up, r_ik_center, blue);
                    draw_square(out_lines, pos, right, up, r_outer - thick, blue);
                    draw_square(out_lines, pos, right, up, r_outer + thick, blue);
                }
            }
        }
    } // 4 passes done
}

/// Draw a circle (LineList, `segments` line segments).
fn draw_circle(
    out: &mut Vec<GridVertex>,
    pos: Vec3,
    right: Vec3,
    up: Vec3,
    radius: f32,
    segments: u32,
    color: [f32; 4],
) {
    for i in 0..segments {
        let a0 = std::f32::consts::TAU * i as f32 / segments as f32;
        let a1 = std::f32::consts::TAU * (i + 1) as f32 / segments as f32;
        let p0 = pos + (right * a0.cos() + up * a0.sin()) * radius;
        let p1 = pos + (right * a1.cos() + up * a1.sin()) * radius;
        push_line(out, p0, p1, color);
    }
}

/// Draw a square (LineList, 4 edges).
fn draw_square(
    out: &mut Vec<GridVertex>,
    pos: Vec3,
    right: Vec3,
    up: Vec3,
    half: f32,
    color: [f32; 4],
) {
    let tl = pos + (-right + up) * half;
    let tr = pos + (right + up) * half;
    let br = pos + (right - up) * half;
    let bl = pos + (-right - up) * half;
    push_line(out, tl, tr, color);
    push_line(out, tr, br, color);
    push_line(out, br, bl, color);
    push_line(out, bl, tl, color);
}

/// Filled circle (TriangleList, triangle fan).
fn draw_filled_circle_tri(
    out: &mut Vec<GridVertex>,
    pos: Vec3,
    right: Vec3,
    up: Vec3,
    radius: f32,
    segments: u32,
    color: [f32; 4],
) {
    let c = GridVertex {
        position: pos.to_array(),
        color,
    };
    for i in 0..segments {
        let a0 = std::f32::consts::TAU * i as f32 / segments as f32;
        let a1 = std::f32::consts::TAU * (i + 1) as f32 / segments as f32;
        let p0 = pos + (right * a0.cos() + up * a0.sin()) * radius;
        let p1 = pos + (right * a1.cos() + up * a1.sin()) * radius;
        out.push(c);
        out.push(GridVertex {
            position: p0.to_array(),
            color,
        });
        out.push(GridVertex {
            position: p1.to_array(),
            color,
        });
    }
}

/// Filled square (TriangleList, 2 triangles).
fn draw_filled_square_tri(
    out: &mut Vec<GridVertex>,
    pos: Vec3,
    right: Vec3,
    up: Vec3,
    half: f32,
    color: [f32; 4],
) {
    let tl = pos + (-right + up) * half;
    let tr = pos + (right + up) * half;
    let br = pos + (right - up) * half;
    let bl = pos + (-right - up) * half;
    // Triangle 1: tl-tr-br.
    out.push(GridVertex {
        position: tl.to_array(),
        color,
    });
    out.push(GridVertex {
        position: tr.to_array(),
        color,
    });
    out.push(GridVertex {
        position: br.to_array(),
        color,
    });
    // Triangle 2: tl-br-bl.
    out.push(GridVertex {
        position: tl.to_array(),
        color,
    });
    out.push(GridVertex {
        position: br.to_array(),
        color,
    });
    out.push(GridVertex {
        position: bl.to_array(),
        color,
    });
}

/// Push a single line segment for LineList.
fn push_line(out: &mut Vec<GridVertex>, a: Vec3, b: Vec3, color: [f32; 4]) {
    out.push(GridVertex {
        position: a.to_array(),
        color,
    });
    out.push(GridVertex {
        position: b.to_array(),
        color,
    });
}

/// Draw a bone triangle (base = base, apex = tip).
fn draw_bone_triangle(
    out: &mut Vec<GridVertex>,
    base: Vec3,
    tip: Vec3,
    camera_eye: Vec3,
    outer_factor: f32,
    color: [f32; 4],
) {
    let dir = tip - base;
    let len = dir.length();
    if len < 0.001 {
        return;
    }
    let dir_n = dir / len;
    let mid = (base + tip) * 0.5;
    let to_cam_mid = (camera_eye - mid).normalize_or_zero();
    let side = dir_n.cross(to_cam_mid).normalize_or_zero();
    let side = if side.length_squared() < 0.001 {
        let (r, _) = billboard_axes(to_cam_mid);
        r
    } else {
        side
    };
    let base_dist = (camera_eye - base).length().max(0.1);
    let base_half = base_dist * outer_factor;

    let base_l = base + side * base_half;
    let base_r = base - side * base_half;

    // Left edge: base_l -> tip.
    out.push(GridVertex {
        position: base_l.to_array(),
        color,
    });
    out.push(GridVertex {
        position: tip.to_array(),
        color,
    });
    // Right edge: base_r -> tip.
    out.push(GridVertex {
        position: base_r.to_array(),
        color,
    });
    out.push(GridVertex {
        position: tip.to_array(),
        color,
    });
}

/// Derive right / up axes for billboarding from the camera direction.
fn billboard_axes(to_camera: Vec3) -> (Vec3, Vec3) {
    let right = to_camera.cross(Vec3::Y).normalize_or_zero();
    if right.length_squared() < 0.001 {
        // Camera is looking straight up / straight down.
        let right = to_camera.cross(Vec3::Z).normalize();
        let up = right.cross(to_camera).normalize();
        (right, up)
    } else {
        let up = right.cross(to_camera).normalize();
        (right, up)
    }
}

/// Compute animation bone deltas (position / rotation differences).
/// Shared by both SpringBone and Joint vertex generation.
fn compute_bone_deltas(
    ir: &IrModel,
    animated_globals: Option<&[glam::Mat4]>,
    is_vrm0: bool,
) -> Option<Vec<(Vec3, glam::Quat)>> {
    let pos_fn = crate::convert::coord::pos_fn(is_vrm0);
    animated_globals.map(|globals| {
        ir.bones
            .iter()
            .enumerate()
            .map(|(i, bone)| {
                if i < globals.len() {
                    let rest_pos_pmx = pos_fn(bone.position);
                    let anim_pos_pmx = pos_fn(globals[i].transform_point3(Vec3::ZERO));
                    let pos_delta = anim_pos_pmx - rest_pos_pmx;
                    let (_, rest_rot, _) = bone.global_mat.to_scale_rotation_translation();
                    let (_, anim_rot, _) = globals[i].to_scale_rotation_translation();
                    let delta_rot_gltf = anim_rot * rest_rot.inverse();
                    let delta_rot_pmx = if is_vrm0 {
                        glam::Quat::from_xyzw(
                            delta_rot_gltf.x,
                            -delta_rot_gltf.y,
                            -delta_rot_gltf.z,
                            delta_rot_gltf.w,
                        )
                    } else {
                        glam::Quat::from_xyzw(
                            -delta_rot_gltf.x,
                            -delta_rot_gltf.y,
                            delta_rot_gltf.z,
                            delta_rot_gltf.w,
                        )
                    };
                    (pos_delta, delta_rot_pmx)
                } else {
                    (Vec3::ZERO, glam::Quat::IDENTITY)
                }
            })
            .collect()
    })
}

/// Generate visualization geometry for SpringBone physics.
/// - Rigid bodies: rings + connecting lines, wireframe-style shape representation.
/// - Joints: line between the two connected rigid bodies.
fn generate_spring_bone_vertices(
    out: &mut Vec<GridVertex>,
    ir: &IrModel,
    opacity: f32,
    align_rigid_rotation: bool,
    bone_deltas: &Option<Vec<(Vec3, glam::Quat)>>,
    is_vrm0: bool,
) {
    use crate::intermediate::types::RigidShape;

    out.clear();
    // VRM: by group (collider = red, spring = green).
    let collider_color = [1.0, 0.0, 0.0, opacity]; // red #ff0000 (group=1: collider)
    let spring_color = [0.0, 1.0, 0.0, opacity]; // green #00ff00 (group!=1: spring chain)
                                                 // PMX/PMD: by physics_mode (0: bone-follow = green, 1: physics = red, 2: physics + bone = blue).
    let bone_follow_color = [0.0, 1.0, 0.0, opacity]; // green
    let physics_color = [1.0, 0.0, 0.0, opacity]; // red
    let physics_bone_color = [0.0, 0.5, 1.0, opacity]; // blue

    let segments = SPHERE_SEGMENTS;
    let line_width = 0.0_f32; // 1px draw (for the `_width` parameter of `draw_ring` / `draw_line_quad`)

    // `bone.position` is stored in glTF space for all formats (PMX/PMD already passed through `pmx_pos_to_gltf`).
    // `rb.position` is in PMX space, so convert the bone side back to PMX space and take the delta.
    let pos_fn = crate::convert::coord::pos_fn(is_vrm0);

    // Draw rigid-body shapes.
    for rb in &ir.physics.rigid_bodies {
        let color = if ir.source_format.is_pmx_pmd() {
            match rb.physics_mode {
                0 => bone_follow_color,  // bone-follow
                1 => physics_color,      // physics
                _ => physics_bone_color, // physics + bone
            }
        } else if rb.group == 1 {
            collider_color
        } else {
            spring_color
        };

        // PMX Euler -> rotation quaternion (YXZ intrinsic = ZXY extrinsic: R = Rz * Rx * Ry).
        // D3DX row-major: v * Ry * Rx * Rz -> glam column-major: Rz * Rx * Ry.
        // PMX/PMD: always use the file's rotation. VRM: only when `align_rigid_rotation` is on.
        let rotation = if ir.source_format.is_pmx_pmd() || align_rigid_rotation {
            rb.rotation
        } else {
            Vec3::ZERO
        };
        let mut quat =
            glam::Quat::from_euler(glam::EulerRot::YXZ, rotation.y, rotation.x, rotation.z);

        // Apply animation: make the rigid body follow the bone.
        let rb_pos = if let (Some(bone_idx), Some(ref deltas)) = (rb.bone_index, &bone_deltas) {
            if bone_idx < deltas.len() {
                let (pos_delta, rot_delta) = deltas[bone_idx];
                let rest_bone_pmx = pos_fn(ir.bones[bone_idx].position);
                // Apply rotation to the rigid body's offset from the bone.
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
                // 8 meridians (45 degrees apart around Y; great-circle arcs).
                for i in 0..8u32 {
                    let angle = std::f32::consts::FRAC_PI_4 * i as f32;
                    let horiz = Vec3::new(angle.cos(), 0.0, angle.sin());
                    // Meridian = great circle spanned by Y and the horizontal axis.
                    draw_ring(
                        out,
                        rb_pos,
                        quat,
                        *radius,
                        Vec3::Y,
                        horiz,
                        segments,
                        line_width,
                        color,
                    );
                }
                // 7 latitudes (evenly spaced from top to bottom).
                for i in 1..=7u32 {
                    let lat_angle = std::f32::consts::PI * i as f32 / 8.0;
                    let y = lat_angle.cos() * radius;
                    let r = lat_angle.sin() * radius;
                    let center = rb_pos + quat * Vec3::new(0.0, y, 0.0);
                    draw_ring(
                        out,
                        center,
                        quat,
                        r,
                        Vec3::Z,
                        Vec3::X,
                        segments,
                        line_width,
                        color,
                    );
                }
            }
            RigidShape::Capsule { radius, height } => {
                // Capsule: Y axis is the bone direction.
                // Height = distance between sphere centers (PMX spec: `height` is sphere-center-to-sphere-center, not (total length - 2*radius)).
                let half_h = height * 0.5;

                // Top and bottom rings.
                let top_offset = quat * Vec3::new(0.0, half_h, 0.0);
                let bot_offset = quat * Vec3::new(0.0, -half_h, 0.0);

                let top_center = rb_pos + top_offset;
                let bot_center = rb_pos + bot_offset;

                // Equatorial rings (top and bottom).
                draw_ring(
                    out,
                    top_center,
                    quat,
                    *radius,
                    Vec3::Z,
                    Vec3::X,
                    segments,
                    line_width,
                    color,
                );
                draw_ring(
                    out,
                    bot_center,
                    quat,
                    *radius,
                    Vec3::Z,
                    Vec3::X,
                    segments,
                    line_width,
                    color,
                );

                // PMX/PMD: draw hemisphere wireframes on both ends.
                if ir.source_format.is_pmx_pmd() {
                    let half_pi = std::f32::consts::FRAC_PI_2;
                    let half_seg = segments / 2;

                    // Upper hemisphere: 4 half-meridians (equator -> north pole).
                    for i in 0..4u32 {
                        let angle = half_pi * i as f32;
                        let horiz = Vec3::new(angle.cos(), 0.0, angle.sin());
                        draw_arc(
                            out,
                            top_center,
                            quat,
                            *radius,
                            horiz,
                            Vec3::Y,
                            half_seg,
                            0.0,
                            half_pi,
                            color,
                        );
                    }
                    // Upper hemisphere: 3 latitudes.
                    for i in 1..=3u32 {
                        let lat = half_pi * i as f32 / 4.0;
                        let y = lat.sin() * radius;
                        let r = lat.cos() * radius;
                        let center = top_center + quat * Vec3::new(0.0, y, 0.0);
                        draw_ring(
                            out,
                            center,
                            quat,
                            r,
                            Vec3::Z,
                            Vec3::X,
                            segments,
                            line_width,
                            color,
                        );
                    }

                    // Lower hemisphere: 4 half-meridians (equator -> south pole).
                    for i in 0..4u32 {
                        let angle = half_pi * i as f32;
                        let horiz = Vec3::new(angle.cos(), 0.0, angle.sin());
                        draw_arc(
                            out,
                            bot_center,
                            quat,
                            *radius,
                            horiz,
                            Vec3::Y,
                            half_seg,
                            -half_pi,
                            0.0,
                            color,
                        );
                    }
                    // Lower hemisphere: 3 latitudes.
                    for i in 1..=3u32 {
                        let lat = half_pi * i as f32 / 4.0;
                        let y = -lat.sin() * radius;
                        let r = lat.cos() * radius;
                        let center = bot_center + quat * Vec3::new(0.0, y, 0.0);
                        draw_ring(
                            out,
                            center,
                            quat,
                            r,
                            Vec3::Z,
                            Vec3::X,
                            segments,
                            line_width,
                            color,
                        );
                    }
                }

                // 8 connecting lines (top -> bottom).
                for i in 0..8u32 {
                    let angle = std::f32::consts::FRAC_PI_4 * i as f32;
                    let local_offset = Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius);
                    let top = top_center + quat * local_offset;
                    let bot = bot_center + quat * local_offset;
                    draw_line_quad(out, top, bot, line_width * 0.5, color);
                }
            }
            RigidShape::Box { size } => {
                // Box: draw 12 edges as lines.
                // PMX spec: `size` is half-extent (same as Bullet `btBoxShape`).
                let hx = size.x;
                let hy = size.y;
                let hz = size.z;
                let corners = [
                    Vec3::new(-hx, -hy, -hz),
                    Vec3::new(hx, -hy, -hz),
                    Vec3::new(hx, hy, -hz),
                    Vec3::new(-hx, hy, -hz),
                    Vec3::new(-hx, -hy, hz),
                    Vec3::new(hx, -hy, hz),
                    Vec3::new(hx, hy, hz),
                    Vec3::new(-hx, hy, hz),
                ];
                let edges = [
                    (0, 1),
                    (1, 2),
                    (2, 3),
                    (3, 0), // front face
                    (4, 5),
                    (5, 6),
                    (6, 7),
                    (7, 4), // back face
                    (0, 4),
                    (1, 5),
                    (2, 6),
                    (3, 7), // connection
                ];
                for (a, b) in edges {
                    let pa = rb_pos + quat * corners[a];
                    let pb = rb_pos + quat * corners[b];
                    draw_line_quad(out, pa, pb, line_width * 0.5, color);
                }
            }
        }
    }

    // Joint connection lines are drawn by `generate_joint_vertices`, so do nothing here.
}

/// 1px ring line (LineList).
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

        verts.push(GridVertex {
            position: p0.to_array(),
            color,
        });
        verts.push(GridVertex {
            position: p1.to_array(),
            color,
        });
    }
}

/// Arc line (LineList): draws from `start_angle` to `end_angle`.
#[allow(clippy::too_many_arguments)]
fn draw_arc(
    verts: &mut Vec<GridVertex>,
    center: Vec3,
    quat: glam::Quat,
    radius: f32,
    axis_a: Vec3,
    axis_b: Vec3,
    segments: u32,
    start_angle: f32,
    end_angle: f32,
    color: [f32; 4],
) {
    let range = end_angle - start_angle;
    for i in 0..segments {
        let a0 = start_angle + range * i as f32 / segments as f32;
        let a1 = start_angle + range * (i + 1) as f32 / segments as f32;

        let local0 = axis_a * a0.cos() * radius + axis_b * a0.sin() * radius;
        let local1 = axis_a * a1.cos() * radius + axis_b * a1.sin() * radius;

        let p0 = center + quat * local0;
        let p1 = center + quat * local1;

        verts.push(GridVertex {
            position: p0.to_array(),
            color,
        });
        verts.push(GridVertex {
            position: p1.to_array(),
            color,
        });
    }
}

/// Generate geometry for normal display (LineList: 2 vertices per normal, vertex -> tip).
fn generate_normal_vertices(
    out: &mut Vec<GridVertex>,
    seen: &mut std::collections::HashSet<(u32, u32, u32, u32, u32, u32)>,
    visible: &mut Vec<bool>,
    model: &GpuModel,
    length: f32,
    material_visibility: &[bool],
) {
    out.clear();
    seen.clear();

    let color = [0.3, 0.6, 1.0, 0.9]; // bluish

    // Use animated vertices when available.
    let base_verts = model.current_vertices();
    let indices = model.base_indices();

    // Resize and clear the visibility-flag buffer.
    visible.resize(base_verts.len(), false);
    visible.iter_mut().for_each(|v| *v = false);

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

    // Deduplicate same-position / same-normal entries (key by bit representation of position + normal).
    for (i, v) in base_verts.iter().enumerate() {
        if !visible[i] {
            continue;
        }
        let normal = Vec3::from(v.normal);
        if normal.length_squared() < 1e-6 {
            continue;
        }
        // Bit-key the position and normal (f32 -> u32).
        let key = (
            v.position[0].to_bits(),
            v.position[1].to_bits(),
            v.position[2].to_bits(),
            v.normal[0].to_bits(),
            v.normal[1].to_bits(),
            v.normal[2].to_bits(),
        );
        if !seen.insert(key) {
            continue;
        }
        let pos = Vec3::from(v.position);
        let tip = pos + normal.normalize() * length;
        out.push(GridVertex {
            position: pos.to_array(),
            color,
        });
        out.push(GridVertex {
            position: tip.to_array(),
            color,
        });
    }
}

/// Line between two points (drawn as a thin quad).
/// 1px line (LineList).
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
    verts.push(GridVertex {
        position: from.to_array(),
        color,
    });
    verts.push(GridVertex {
        position: to.to_array(),
        color,
    });
}

/// Generate joint vertices (orange cube faces + 1px black edges; rotation-aware; animation-synced).
fn generate_joint_vertices(
    faces_out: &mut Vec<GridVertex>,
    edges_out: &mut Vec<GridVertex>,
    ir: &IrModel,
    opacity: f32,
    bone_deltas: &Option<Vec<(Vec3, glam::Quat)>>,
    is_vrm0: bool,
) {
    faces_out.clear();
    edges_out.clear();

    let orange = [1.0, 1.0, 0.0, opacity]; // yellow #ffff00
    let black = [0.0, 0.0, 0.0, opacity.min(1.0)];
    let size = 0.18_f32;

    let is_pmx_pmd = ir.source_format.is_pmx_pmd();

    // `bone.position` is in glTF space for all formats (PMX/PMD already passed through `pmx_pos_to_gltf`).
    let pos_fn = crate::convert::coord::pos_fn(is_vrm0);

    for joint in &ir.physics.joints {
        if joint.rigid_a >= ir.physics.rigid_bodies.len() {
            continue;
        }

        let rb_a = &ir.physics.rigid_bodies[joint.rigid_a];

        // Joint position (PMX coordinates).
        // PMX/PMD: `joint.position` is already in PMX coordinates. VRM: it's in glTF coordinates, so convert via `pos_fn`.
        let joint_rest_pos = if is_pmx_pmd {
            joint.position
        } else {
            pos_fn(joint.position)
        };
        // Joint rotation (YXZ intrinsic = ZXY extrinsic: R = Rz * Rx * Ry).
        let joint_rest_quat = glam::Quat::from_euler(
            glam::EulerRot::YXZ,
            joint.rotation.y,
            joint.rotation.x,
            joint.rotation.z,
        );

        // Apply animation: follow with the offset from `rigid_a`'s bone.
        let (joint_pos, joint_quat) =
            if let (Some(bone_idx), Some(ref deltas)) = (rb_a.bone_index, &bone_deltas) {
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

        // 8 vertices of the cube (local coordinates).
        let h = size * 0.5;
        let cube_verts = [
            Vec3::new(-h, -h, -h), // 0: bottom-left front
            Vec3::new(h, -h, -h),  // 1: bottom-right front
            Vec3::new(h, h, -h),   // 2: top-right front
            Vec3::new(-h, h, -h),  // 3: top-left front
            Vec3::new(-h, -h, h),  // 4: bottom-left back
            Vec3::new(h, -h, h),   // 5: bottom-right back
            Vec3::new(h, h, h),    // 6: top-right back
            Vec3::new(-h, h, h),   // 7: top-left back
        ];

        // Apply rotation and convert to world coordinates.
        let wv: [Vec3; 8] = cube_verts.map(|c| joint_pos + joint_quat * c);

        // 6 cube faces (2 triangles each, orange fill).
        let cube_faces: [[usize; 4]; 6] = [
            [0, 1, 2, 3], // front (-Z)
            [5, 4, 7, 6], // back (+Z)
            [4, 0, 3, 7], // left (-X)
            [1, 5, 6, 2], // right (+X)
            [3, 2, 6, 7], // top (+Y)
            [4, 5, 1, 0], // bottom (-Y)
        ];
        for face in &cube_faces {
            faces_out.push(GridVertex {
                position: wv[face[0]].to_array(),
                color: orange,
            });
            faces_out.push(GridVertex {
                position: wv[face[1]].to_array(),
                color: orange,
            });
            faces_out.push(GridVertex {
                position: wv[face[2]].to_array(),
                color: orange,
            });
            faces_out.push(GridVertex {
                position: wv[face[0]].to_array(),
                color: orange,
            });
            faces_out.push(GridVertex {
                position: wv[face[2]].to_array(),
                color: orange,
            });
            faces_out.push(GridVertex {
                position: wv[face[3]].to_array(),
                color: orange,
            });
        }

        // Black outline: draw 12 edges as 1px lines (LineList).
        let edges: [[usize; 2]; 12] = [
            [0, 1],
            [1, 2],
            [2, 3],
            [3, 0],
            [4, 5],
            [5, 6],
            [6, 7],
            [7, 4],
            [0, 4],
            [1, 5],
            [2, 6],
            [3, 7],
        ];
        for edge in &edges {
            edges_out.push(GridVertex {
                position: wv[edge[0]].to_array(),
                color: black,
            });
            edges_out.push(GridVertex {
                position: wv[edge[1]].to_array(),
                color: black,
            });
        }
    }
}
