use bytemuck::{Pod, Zeroable};
use eframe::{egui_wgpu, wgpu};
use glam::Vec3;
use wgpu::util::DeviceExt;

use super::camera::OrbitCamera;
use super::mesh::{DrawCall, GpuModel, MaterialBuildFlags, RenderQueue};
use crate::intermediate::types::{CullMode, IrModel};

/// MMD ライティングのアンビエントスケール（154/255 ≈ 0.604）
const MMD_LIGHT_AMBIENT: f32 = 154.0 / 255.0;
/// MMD ライトのデフォルト強度（mmd_ambient_scale 算出の基準値）
const MMD_DEFAULT_LIGHT_INTENSITY: f32 = 0.7;
/// ボーン表示の外側（通常）半径係数
const BONE_DISPLAY_RADIUS: f32 = 0.004;
/// ボーン表示の内側（移動ボーン）半径係数
const BONE_JOINT_RADIUS: f32 = 0.0022;
/// ボーン・物理表示の球セグメント数
const SPHERE_SEGMENTS: u32 = 16;

/// bool を f32 に変換（シェーダー uniform 用）
#[inline]
fn b2f(b: bool) -> f32 {
    if b {
        1.0
    } else {
        0.0
    }
}

/// 材質用 BindGroupLayout を作成（共通定義、gpu.rs と mesh.rs で共有）
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

// encase::ShaderType derive マクロが内部的に `check` 関数を生成し dead_code 警告が出るため、
// サブモジュールで #![allow(dead_code)] を適用して抑制する。
mod encase_uniforms {
    #![allow(dead_code)]

    /// カメラ uniform バッファ
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
        /// 累積時間（秒、UVアニメーション用）
        pub time: f32,
        /// アスペクト比 (width / height)（MToon アウトライン: 1/aspect で X 補正）
        pub aspect: f32,
        /// 射影行列 [1][1] = 1/tan(halfFov)（MToon アウトライン距離クランプ用）
        pub proj_11: f32,
        // encase auto-pads to align next vec3
        /// SH ベース GI の均一化値: (rawGi(up) + rawGi(down)) / 2（CPU 事前計算）
        pub gi_equalized: glam::Vec3,
        /// 透視投影フラグ（1.0 = 透視, 0.0 = 正射影）
        pub is_perspective: f32,
        /// カメラ前方ベクトル（正射影時の view direction 用）
        pub camera_forward: glam::Vec3,
        // encase auto-pads vec3 trailing 4 bytes
        /// ライト色 RGB (linear)
        pub light_color: glam::Vec3,
        // encase auto-pads vec3 trailing 4 bytes
        /// 環境光 Ground 色 RGB (linear, 半球 ambient 補間用)
        pub ambient_ground: glam::Vec3,
        // encase auto-pads vec3 trailing 4 bytes (struct tail)
    }

    /// 材質 uniform バッファ（MToon パラメータ含む）
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
        /// MASK モード時の alphaCutoff（0.0 = 無効）
        pub alpha_cutoff: f32,
        // --- テクスチャ UV パラメータ（texCoord + KHR_texture_transform）---
        // 各テクスチャ: [tex_coord, offset.x, offset.y, rotation] + [scale.x, scale.y, 0, 0]
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
        // --- 法線マップパラメータ ---
        pub has_normal_tex: f32,
        pub normal_scale: f32,
        pub gi_equalization_factor: f32,
        /// outlineWidthTexture 参照チャネル（0.0=R, 1.0=G, 2.0=B）
        pub outline_width_channel: f32,
        pub normal_uv_a: glam::Vec4,
        pub normal_uv_b: glam::Vec4,
        /// uvAnimationMaskTexture 参照チャネル（0.0=R, 1.0=G, 2.0=B）
        pub uv_anim_mask_channel: f32,
        // encase auto-pads 3 x f32 to align next vec4
        // --- matcapTexture UV パラメータ（KHR_texture_transform）---
        pub matcap_uv_a: glam::Vec4,
        pub matcap_uv_b: glam::Vec4,
    }
}
pub use encase_uniforms::{CameraUniform, MaterialUniform};

/// MMD 材質 uniform バッファ
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
    /// PMX/PMD 自己発光色（Bloom 用、derive_pmx_bloom で算出）
    pub bloom_emissive: [f32; 3],
}

/// 頂点フォーマット
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    /// TEXCOORD_1（セカンダリUV、MToon 補助テクスチャ用）。UV1 なしなら UV0 コピー。
    pub uv1: [f32; 2],
    /// 接線ベクトル（xyz=tangent方向, w=handedness ±1）
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

/// 可視化バッファの共通パターン（GPU バッファ + 容量 + 頂点数）
struct DynamicBuffer {
    buf: Option<wgpu::Buffer>,
    capacity: usize,
    vertex_count: u32,
}

impl DynamicBuffer {
    /// 空の DynamicBuffer を作成
    fn new() -> Self {
        Self {
            buf: None,
            capacity: 0,
            vertex_count: 0,
        }
    }

    /// 作業バッファの内容を GPU にアップロードする。
    /// 容量不足なら新規バッファを作成し、十分なら既存バッファに書き込む。
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

/// WGSL 共通: CameraUniform 構造体定義（全シェーダーで共有）
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

/// WGSL 共通: MmdMaterialUniform 構造体定義（MMD シェーダーで共有）
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

/// WGSL 共通: MaterialUniform 構造体定義（基本シェーダーで共有）
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

/// WGSL 共通: MToon テクスチャバインディング宣言（メイン/アウトラインシェーダーで共有）
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

/// WGSL 共通: MToon ヘルパー関数群（メイン/アウトラインシェーダーで共有）
/// apply_texture_transform, resolve_mtoon_uv, apply_uv_anim_core, select_channel, apply_normal_map
macro_rules! wgsl_mtoon_helpers {
    () => {
        r#"
/// KHR_texture_transform 適用（uv_a = [texCoord, offset.x, offset.y, rotation], uv_b = [scale.x, scale.y, 0, 0]）
fn apply_texture_transform(uv: vec2<f32>, uv_a: vec4<f32>, uv_b: vec4<f32>) -> vec2<f32> {
    let offset = vec2<f32>(uv_a.y, uv_a.z);
    let rotation = uv_a.w;
    let scale = vec2<f32>(uv_b.x, uv_b.y);
    // scale/rotation が既定値なら早期リターン
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

/// MToon 補助テクスチャ用 UV 解決: texCoord 選択 → KHR_texture_transform
/// UVアニメーション対象テクスチャは animated UV を渡し、非対象は raw UV を渡す
fn resolve_mtoon_uv(uv0: vec2<f32>, uv1: vec2<f32>, uv_a: vec4<f32>, uv_b: vec4<f32>) -> vec2<f32> {
    let base_uv = select(uv0, uv1, u32(uv_a.x) == 1u);
    return apply_texture_transform(base_uv, uv_a, uv_b);
}

/// UVアニメーション（スクロール+回転）の計算本体（マスク値は呼び出し元で決定）
/// UniVRM互換順序: scroll → pivot(-0.5) → rotation → pivot(+0.5)
/// ※ VRM仕様書は rotate→scroll だが、UniVRM 実装は scroll→rotate。互換性を優先
/// UniVRM: vrmc_materials_mtoon_geometry_uv.hlsl — rotate(uv + translate - pivot) + pivot
fn apply_uv_anim_core(uv: vec2<f32>, anim_mask: f32) -> vec2<f32> {
    let translate = vec2<f32>(
        camera.time * material.uv_anim_scroll_x,
        camera.time * material.uv_anim_scroll_y,
    ) * anim_mask;

    // 2π 周期で wrap して長時間稼働時の float 精度劣化を防止（UniVRM 準拠）
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

/// テクセルからチャネル選択（0=R, 1=G, 2=B）
fn select_channel(texel: vec4<f32>, ch: f32) -> f32 {
    if ch < 0.5 {
        return texel.r;
    } else if ch < 1.5 {
        return texel.g;
    }
    return texel.b;
}

/// 頂点接線から TBN 行列を構築して法線マップを適用（UniVRM MToon_GetTangentToWorld 準拠）
/// tangent.w の符号で bitangent の向きを制御（ミラー UV 対応）
fn apply_normal_map(base_n: vec3<f32>, tangent: vec4<f32>, normal_uv: vec2<f32>) -> vec3<f32> {
    // ゼロ接線ガード: 退化した tangent では法線マップをスキップし基底法線を返す
    if dot(tangent.xyz, tangent.xyz) < 1e-6 {
        return normalize(base_n);
    }
    let normal_sample = textureSample(t_normal, s_normal, normal_uv).xyz * 2.0 - 1.0;
    let n = normalize(base_n);
    let t = normalize(tangent.xyz);
    // UniVRM 準拠: tangent.w を二値化して NaN 回避（vrmc_materials_mtoon_utility.hlsl:64）
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

/// アルファモード処理（OPAQUE / MASK+A2C / BLEND）
fn apply_alpha_mode(alpha: f32, cutoff: f32) -> f32 {
    if cutoff < -0.75 {
        // OPAQUE: テクスチャ alpha をそのまま返す
        // VRM OPAQUE 材質はテクスチャ alpha=1.0 のため影響なし
        // PMX/PMD 材質ではテクスチャ alpha による透過が反映される
        if alpha <= 0.001 { discard; }
        return alpha;
    }
    if cutoff >= -0.25 {
        // MASK + AlphaToCoverage（UniVRM vrmc_materials_mtoon_geometry_alpha.hlsl 準拠）
        let a2c_alpha = (alpha - cutoff) / max(fwidth(alpha), 1e-5) + 0.5;
        if a2c_alpha < cutoff { discard; }
        return 1.0;
    }
    // BLEND: 完全透明ピクセルを破棄（深度汚染防止）
    if alpha <= 0.001 { discard; }
    return alpha;
}

struct FsOutput {
    @location(0) color: vec4<f32>,
    @location(1) bloom: vec4<f32>,
};

@fragment
fn fs_main(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FsOutput {
    // doubleSided 材質の背面法線反転（UniVRM 準拠: 法線マップ適用前に反転）
    let face_sign = select(-1.0, 1.0, is_front);
    var n = normalize(in.normal) * face_sign;

    // --- MToon UVアニメーション事前計算（normalTexture にも適用: 仕様準拠）---
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

    // 法線マップ適用（MToon: animated UV, 非MToon: raw UV）
    if material.has_normal_tex > 0.5 {
        let normal_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.normal_uv_a, material.normal_uv_b);
        n = apply_normal_map(n, in.tangent, normal_uv);
    }

    // === シェーダーオーバーライド ===
    // プレビュー用モードではテクスチャ alpha をそのまま使用（PMX/PMD の OPAQUE 材質でも透過を反映）
    if camera.shader_mode == 1u {
        // Normal: ジオメトリ法線→RGB
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
        // Unlit: テクスチャ色のみ、ライティングなし
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
        // GGX Preview: 簡易 Cook-Torrance スペキュラ
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

        // 半球アンビエント
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

        // テクスチャサンプリング（UVアニメーション + texCoord/KHR_texture_transform 適用）
        let base_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.base_uv_a, material.base_uv_b);
        let tex_color = textureSample(t_diffuse, s_diffuse, base_uv);
        let base_color = tex_color * material.diffuse;
        out_alpha = base_color.a;

        // dot(N,L) — 仕様準拠: [-1, 1] レンジ（half-lambert ではない）
        // camera.light_dir は光の進行方向（光源→表面）なので反転して表面→光源方向にする
        let dot_nl = dot(n, -camera.light_dir);

        // shadeMultiplyTexture 適用（UV Animation 対象）
        var shade_mul = vec3<f32>(1.0);
        if material.has_shade_multiply_tex > 0.5 {
            let shade_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.shade_uv_a, material.shade_uv_b);
            shade_mul = textureSample(t_shade_multiply, s_shade_multiply, shade_uv).rgb;
        }
        let shade = material.shade_color * shade_mul;

        // shadingShiftTexture 適用（UV Animation 対象、UniVRM 準拠）
        var shading = dot_nl + material.shading_shift;
        if material.has_shading_shift_tex > 0.5 {
            let shift_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.shift_uv_a, material.shift_uv_b);
            let shift_tex = textureSample(t_shading_shift, s_shading_shift, shift_uv).r;
            shading += shift_tex * material.shading_shift_tex_scale;
        }

        // MToon 2色トゥーン: linearstep で lit/shade を補間（仕様準拠）
        let edge0 = -1.0 + material.shading_toony;
        let edge1 = 1.0 - material.shading_toony;
        let t = clamp((shading - edge0) / max(edge1 - edge0, 0.001), 0.0, 1.0);
        lit = mix(shade, base_color.rgb, t);

        // ライティング: direct と GI（indirect）を分離（UniVRM 準拠）
        // direct = toon_color * directLightColor（ForwardBase: shadow=1）
        // indirect = litColor * lerp(passthroughGi, uniformedGi, giEqualizationFactor)
        // 半球 ambient: sky/ground を最終法線Y成分で補間（SH 近似）
        let hemi_t = n.y * 0.5 + 0.5;
        let raw_indirect = mix(camera.ambient_ground, camera.ambient, vec3<f32>(hemi_t));
        let gi = mix(raw_indirect, camera.gi_equalized, material.gi_equalization_factor);
        let direct_light = camera.light_intensity * camera.light_color;
        let lighting = lit * direct_light + lit * gi;

        // --- リムライティング + MatCap ---
        // 透視投影: camera_pos → world_pos、正射影: camera_forward（UniVRM 準拠）
        var v: vec3<f32>;
        if camera.is_perspective > 0.5 {
            v = normalize(camera.camera_pos - in.world_pos);
        } else {
            v = normalize(camera.camera_forward);
        }
        var rim = vec3<f32>(0.0);

        // MatCap: ビュー空間法線からUV算出（UV Animation 非対象）
        // UniVRM 準拠: right = cross(viewDir, worldUp), up = cross(right, viewDir)
        // KHR_texture_transform は最終 matcap UV に適用
        if material.has_matcap > 0.5 {
            let world_view_x = normalize(vec3<f32>(-v.z, 0.0, v.x));
            let world_view_y = cross(world_view_x, v);
            let raw_matcap_uv = vec2<f32>(dot(world_view_x, n), dot(world_view_y, n)) * 0.495 + 0.5;
            let matcap_uv = apply_texture_transform(raw_matcap_uv, material.matcap_uv_a, material.matcap_uv_b);
            rim = material.matcap_factor * textureSample(t_matcap, s_matcap, matcap_uv).rgb;
        }

        // パラメトリックリム: フレネル効果
        let ndotv = dot(n, v);
        let parametric_rim = pow(
            saturate(1.0 - ndotv + material.rim_lift),
            max(material.rim_fresnel_power, 0.00001)
        );
        rim = rim + parametric_rim * material.rim_color;

        // rimMultiplyTexture 適用（UV Animation 対象）
        if material.has_rim_multiply_tex > 0.5 {
            let rim_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.rim_uv_a, material.rim_uv_b);
            rim *= textureSample(t_rim_multiply, s_rim_multiply, rim_uv).rgb;
        }

        // リムのライティング混合（VRM 1.0 仕様: rim * lerp(white, lighting, mix)）
        // UniVRM 準拠: rim には未均一化の raw indirect を使用（GI equalization 非適用）
        let rim_light_factor = direct_light + raw_indirect;
        let rim_lit = rim * mix(vec3<f32>(1.0), rim_light_factor, material.rim_lighting_mix);

        // emissive（glTF 標準 + MToon 仕様: baseCol = lighting + emissive + rim）
        var emissive = material.emissive_factor;
        if material.has_emissive_tex > 0.5 {
            let emissive_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.emissive_uv_a, material.emissive_uv_b);
            emissive *= textureSample(t_emissive, s_emissive, emissive_uv).rgb;
        }

        bloom_color = emissive;
        lit = lighting + rim_lit + emissive;
    } else {
        // 非MToon: 既存 Half-Lambert（texCoord + KHR_texture_transform 適用）
        let half_lambert = dot(n, -camera.light_dir) * 0.5 + 0.5;
        let base_uv = resolve_mtoon_uv(in.uv, in.uv1, material.base_uv_a, material.base_uv_b);
        let tex_color = textureSample(t_diffuse, s_diffuse, base_uv);
        let base_color = tex_color * material.diffuse;
        let hemi_t_hl = n.y * 0.5 + 0.5;
        let hemi_ambient = mix(camera.ambient_ground, camera.ambient, vec3<f32>(hemi_t_hl));
        let light = hemi_ambient + camera.light_intensity * camera.light_color * half_lambert;
        lit = base_color.rgb * light;
        out_alpha = base_color.a;

        // 非 MToon でも emissive は glTF 標準として適用（texCoord + KHR_texture_transform 適用）
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

/// MMD メインシェーダー共通部（頂点シェーダー + ライティング本体）
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
    // スフィアUV: ビュー空間法線の XY を [0,1] にマッピング
    // X反転座標系のため normalWv.x を反転
    let vn_x = dot(normal, camera.view_row0);
    let vn_y = dot(normal, camera.view_row1);
    out.sphere_uv = vec2<f32>(vn_x * -0.5 + 0.5, vn_y * -0.5 + 0.5);
    return out;
}

fn compute_mmd_lighting(in: MmdVertexOutput) -> vec4<f32> {
    let n = normalize(in.normal);

    // ライティング:
    // AmbientColor = saturate(MaterialAmbient * LightAmbient + MaterialEmissive)
    // PMX ambient は D3D の emissive に相当、PMX diffuse は D3D の ambient に相当
    // LightAmbient = mmd_ambient_scale × light_color (ライト色調・強度を反映)
    let mmd_light = vec3<f32>(camera.mmd_ambient_scale) * camera.light_color;
    let base_color = clamp(material.diffuse_rgb * mmd_light + material.ambient, vec3<f32>(0.0), vec3<f32>(1.0));

    // テクスチャサンプリング
    let tex_color = textureSample(t_diffuse, s_diffuse, in.uv);
    var out_rgb = base_color * tex_color.rgb;
    var out_a = tex_color.a * material.alpha;

    // スフィアマップ (RGB のみ、アルファは影響させない)
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

    // トゥーン (NdotL依存サンプリング + 乗算)
    let has_toon = (material.flags & 4u) != 0u;
    if has_toon {
        let lightNormal = dot(n, -camera.light_dir);
        let toon_uv = vec2<f32>(0.0, 0.5 - lightNormal * 0.5);
        let toon_color = textureSample(t_toon, s_toon, toon_uv);
        out_rgb *= toon_color.rgb;
        out_a *= toon_color.a;
    }

    // アルファテスト
    if out_a < ALPHA_DISCARD_THRESHOLD { discard; }

    // スペキュラ (最後に加算、トゥーンの影響を受けない)
    // LightSpecular = mmd_ambient_scale × light_color
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

/// MMD エッジシェーダー共通部（頂点シェーダー）
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

/// MMD エッジシェーダー（inverted hull 法、sRGB版）
const MMD_EDGE_SHADER_SRC: &str = concat!(
    wgsl_camera_uniform!(),
    "\n",
    wgsl_mmd_material_uniform!(),
    wgsl_mmd_edge_body!(),
    r#"
@fragment
fn fs_edge() -> MmdFsOutput {
    // sRGBレンダーターゲットの自動エンコードを打ち消す
    let c = material.edge_color;
    var out: MmdFsOutput;
    out.color = vec4<f32>(pow(max(c.rgb, vec3<f32>(0.0)), vec3<f32>(2.2)), c.a);
    out.bloom = vec4<f32>(0.0);
    return out;
}
"#
);

/// MMD メインシェーダー（sRGB版: pow(2.2) でガンマ補正）
const MMD_MAIN_SHADER_SRC: &str = concat!(
    wgsl_camera_uniform!(),
    "\n",
    wgsl_mmd_material_uniform!(),
    wgsl_mmd_main_body!(),
    r#"
@fragment
fn fs_mmd(in: MmdVertexOutput) -> MmdFsOutput {
    let result = compute_mmd_lighting(in);
    // sRGBレンダーターゲットの自動エンコードを打ち消す（MMDはガンマ空間で計算）
    let output = pow(max(result.rgb, vec3<f32>(0.0)), vec3<f32>(2.2));
    var out: MmdFsOutput;
    out.color = vec4<f32>(output, result.a);
    out.bloom = vec4<f32>(material.bloom_emissive_r, material.bloom_emissive_g, material.bloom_emissive_b, result.a);
    return out;
}
"#
);

/// MMD エッジシェーダー Unorm 版（pow(2.2) 除去 — ガンマ空間直接出力）
const MMD_EDGE_SHADER_UNORM_SRC: &str = concat!(
    wgsl_camera_uniform!(),
    "\n",
    wgsl_mmd_material_uniform!(),
    wgsl_mmd_edge_body!(),
    r#"
@fragment
fn fs_edge() -> MmdFsOutput {
    // Unorm ターゲット: ガンマ空間値をそのまま出力（pow(2.2) 不要）
    var out: MmdFsOutput;
    out.color = material.edge_color;
    out.bloom = vec4<f32>(0.0);
    return out;
}
"#
);

/// MMD メインシェーダー Unorm 版（pow(2.2) 除去 — ガンマ空間直接出力）
const MMD_MAIN_SHADER_UNORM_SRC: &str = concat!(
    wgsl_camera_uniform!(),
    "\n",
    wgsl_mmd_material_uniform!(),
    wgsl_mmd_main_body!(),
    r#"
@fragment
fn fs_mmd(in: MmdVertexOutput) -> MmdFsOutput {
    let result = compute_mmd_lighting(in);
    // Unorm ターゲット: ガンマ空間値をそのまま出力（pow(2.2) 不要）
    var out: MmdFsOutput;
    out.color = vec4<f32>(clamp(result.rgb, vec3<f32>(0.0), vec3<f32>(1.0)), result.a);
    out.bloom = vec4<f32>(material.bloom_emissive_r, material.bloom_emissive_g, material.bloom_emissive_b, result.a);
    return out;
}
"#
);

/// グリッドシェーダー共通部（頂点シェーダー）
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

/// グリッドシェーダー Unorm 版（linear_to_srgb 変換付き）
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

/// ワイヤーフレームオーバーレイ用シェーダー（黒色で描画）
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

/// MToon アウトラインシェーダー共通部（inverted hull 法）
/// 本体と同等の MToon ライティング計算を行い、outlineLightingMixFactor で混合する
/// バインディング宣言とヘルパー関数は wgsl_mtoon_bindings! / wgsl_mtoon_helpers! で共通化
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

/// UV Animation を適用（頂点シェーダー用、UV0/UV1 ペア対応）
/// 戻り値: vec4(anim_uv0.xy, anim_uv1.zw)
fn apply_uv_animation_pair(uv0: vec2<f32>, uv1: vec2<f32>) -> vec4<f32> {
    let has_uv_anim = material.uv_anim_scroll_x != 0.0
        || material.uv_anim_scroll_y != 0.0
        || material.uv_anim_rotation != 0.0;
    if !has_uv_anim { return vec4<f32>(uv0, uv1); }
    // マスクテクスチャ用UV（texCoord+transform、UV Animation 非対象）
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
    // outlineWidthMultiplyTexture: UV Animation 対象、texCoord+transform 適用（UV0/UV1 ペア）
    let anim_pair = apply_uv_animation_pair(uv, uv1_in);
    let width_uv = resolve_mtoon_uv(anim_pair.xy, anim_pair.zw, material.outline_uv_a, material.outline_uv_b);
    let width_tex = select_channel(textureSampleLevel(t_outline_width, s_outline_width, width_uv, 0.0), material.outline_width_channel);
    let width = material.outline_width * width_tex;
    if material.outline_mode > 1.5 {
        // screenCoordinates: clip 空間で法線方向にオフセット（UniVRM 準拠）
        let clip = camera.view_proj * vec4<f32>(position, 1.0);
        // ビュー空間法線
        let nv_x = dot(camera.view_row0, n);
        let nv_y = dot(camera.view_row1, n);
        let view_row2 = cross(camera.view_row0, camera.view_row1);
        let nv_z = dot(view_row2, n);
        // UniVRM 準拠: 先に正規化 → 後から aspect で X 引き伸ばし
        let raw = vec2<f32>(nv_x, nv_y);
        let len = length(raw);
        var projected = select(vec2<f32>(0.0), raw / len, len > 0.0001);
        // 距離クランプ: 広角カメラでの太すぎ防止（UniVRM MToon_GetOutlineVertex_ScreenCoordinatesWidthMultiplier 準拠）
        let max_view_frustum_plane_height = 2.0;
        let width_scaled_max_distance = max_view_frustum_plane_height * camera.proj_11 * 0.5;
        let width_multiplier = min(clip.w, width_scaled_max_distance);
        projected *= 2.0 * width * width_multiplier;
        projected.x /= camera.aspect;
        // カメラ正面法線の抑制（正面向き頂点の XY ずれを防ぐ）
        projected *= saturate(1.0 - nv_z * nv_z);
        out.clip_position = vec4<f32>(clip.xy + projected, clip.zw);
    } else {
        // worldCoordinates: ワールド空間でメートル単位
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

/// 本体シェーダーと同等の MToon ライティング計算（アウトライン用）
/// 返り値: vec4(表面シェーディング結果 RGB, 処理済みアルファ)
/// alphaMode に基づく discard もここで実行（UniVRM 準拠: アウトラインにも適用）
fn compute_mtoon_surface_lighting(n: vec3<f32>, uv: vec2<f32>, uv1: vec2<f32>, world_pos: vec3<f32>) -> vec4<f32> {
    // --- UVアニメーション ---
    let has_uv_anim = material.uv_anim_scroll_x != 0.0
        || material.uv_anim_scroll_y != 0.0
        || material.uv_anim_rotation != 0.0;
    // マスクテクスチャ用UV（texCoord+transform、UV Animation 非対象）
    let uv_mask_uv = resolve_mtoon_uv(uv, uv1, material.uv_mask_uv_a, material.uv_mask_uv_b);
    var anim_mask = 1.0;
    if has_uv_anim && material.has_uv_anim_mask > 0.5 {
        anim_mask = select_channel(textureSample(t_uv_anim_mask, s_uv_anim_mask, uv_mask_uv), material.uv_anim_mask_channel);
    }
    let anim_uv = select(uv, apply_uv_anim_core(uv, anim_mask), has_uv_anim);
    let anim_uv1 = select(uv1, apply_uv_anim_core(uv1, anim_mask), has_uv_anim);

    // テクスチャサンプリング（UVアニメーション + texCoord/KHR_texture_transform 適用）
    let base_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.base_uv_a, material.base_uv_b);
    let tex_color = textureSample(t_diffuse, s_diffuse, base_uv);
    let base_color = tex_color * material.diffuse;

    // alphaMode 処理（本体 fs_main と同一ロジック）
    var out_alpha = base_color.a;
    if material.alpha_cutoff < -0.75 {
        out_alpha = 1.0;
    } else if material.alpha_cutoff >= -0.25 {
        // MASK + AlphaToCoverage（UniVRM 準拠、fs_main と同一）
        let a2c_alpha = (out_alpha - material.alpha_cutoff)
            / max(fwidth(out_alpha), 1e-5) + 0.5;
        if a2c_alpha < material.alpha_cutoff { discard; }
        out_alpha = 1.0; // UniVRM 準拠: A2C はカバレッジ制御のみ、最終 alpha は不透明
    } else {
        if out_alpha <= 0.001 { discard; }
    }

    // dot(N,L) — 仕様準拠: [-1, 1] レンジ
    // camera.light_dir は光の進行方向（光源→表面）なので反転して表面→光源方向にする
    let dot_nl = dot(n, -camera.light_dir);

    // shadeMultiplyTexture 適用（UV Animation 対象）
    var shade_mul = vec3<f32>(1.0);
    if material.has_shade_multiply_tex > 0.5 {
        let shade_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.shade_uv_a, material.shade_uv_b);
        shade_mul = textureSample(t_shade_multiply, s_shade_multiply, shade_uv).rgb;
    }
    let shade = material.shade_color * shade_mul;

    // shadingShiftTexture 適用（UV Animation 対象、UniVRM 準拠）
    var shading = dot_nl + material.shading_shift;
    if material.has_shading_shift_tex > 0.5 {
        let shift_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.shift_uv_a, material.shift_uv_b);
        let shift_tex = textureSample(t_shading_shift, s_shading_shift, shift_uv).r;
        shading += shift_tex * material.shading_shift_tex_scale;
    }

    // MToon 2色トゥーン: linearstep で lit/shade を補間（仕様準拠）
    let edge0 = -1.0 + material.shading_toony;
    let edge1 = 1.0 - material.shading_toony;
    let t = clamp((shading - edge0) / max(edge1 - edge0, 0.001), 0.0, 1.0);
    let toon_color = mix(shade, base_color.rgb, t);

    // ライティング: direct と GI（indirect）を分離（UniVRM 準拠）
    // 半球 ambient: sky/ground を最終法線Y成分で補間（SH 近似）
    let hemi_t_o = n.y * 0.5 + 0.5;
    let raw_indirect = mix(camera.ambient_ground, camera.ambient, vec3<f32>(hemi_t_o));
    let gi = mix(raw_indirect, camera.gi_equalized, material.gi_equalization_factor);
    let direct_light = camera.light_intensity * camera.light_color;
    let lighting = toon_color * direct_light + toon_color * gi;

    // --- リムライティング + MatCap ---
    // 透視投影: camera_pos → world_pos、正射影: camera_forward（UniVRM 準拠）
    var v: vec3<f32>;
    if camera.is_perspective > 0.5 {
        v = normalize(camera.camera_pos - world_pos);
    } else {
        v = normalize(camera.camera_forward);
    }
    var rim = vec3<f32>(0.0);

    // MatCap: ビュー空間法線からUV算出（UV Animation 非対象）
    // UniVRM 準拠: right = cross(viewDir, worldUp), up = cross(right, viewDir)
    // KHR_texture_transform は最終 matcap UV に適用
    if material.has_matcap > 0.5 {
        let world_view_x = normalize(vec3<f32>(-v.z, 0.0, v.x));
        let world_view_y = cross(world_view_x, v);
        let raw_matcap_uv = vec2<f32>(dot(world_view_x, n), dot(world_view_y, n)) * 0.495 + 0.5;
        let matcap_uv = apply_texture_transform(raw_matcap_uv, material.matcap_uv_a, material.matcap_uv_b);
        rim = material.matcap_factor * textureSample(t_matcap, s_matcap, matcap_uv).rgb;
    }

    // パラメトリックリム: フレネル効果
    let ndotv = dot(n, v);
    let parametric_rim = pow(
        saturate(1.0 - ndotv + material.rim_lift),
        max(material.rim_fresnel_power, 0.00001)
    );
    rim = rim + parametric_rim * material.rim_color;

    // rimMultiplyTexture 適用（UV Animation 対象）
    if material.has_rim_multiply_tex > 0.5 {
        let rim_uv = resolve_mtoon_uv(anim_uv, anim_uv1, material.rim_uv_a, material.rim_uv_b);
        rim *= textureSample(t_rim_multiply, s_rim_multiply, rim_uv).rgb;
    }

    // リムのライティング混合（VRM 1.0 仕様: rim * lerp(white, lighting, mix)）
    // UniVRM 準拠: rim には未均一化の raw indirect を使用（GI equalization 非適用）
    let rim_light_factor = direct_light + raw_indirect;
    let rim_lit = rim * mix(vec3<f32>(1.0), rim_light_factor, material.rim_lighting_mix);

    // emissive（UniVRM 準拠: baseCol = lighting + emissive + rim）
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

/// WGSL 共通: fs_outline フラグメントシェーダー本体（sRGB/Unorm の出力式のみパラメータ化）
/// $output_expr: sRGB 版では `lit`、Unorm 版では `clamp(lit, vec3<f32>(0.0), vec3<f32>(1.0))`
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
    // doubleSided 背面法線反転（UniVRM 準拠）
    let face_sign = select(-1.0, 1.0, is_front);
    var n = normalize(in.normal) * face_sign;
    // UVアニメーション事前計算（normalTexture にも適用: 仕様準拠）
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
    // 法線マップ適用（animated UV）
    if material.has_normal_tex > 0.5 {
        let normal_uv = resolve_mtoon_uv(anim_uv_o, anim_uv1_o, material.normal_uv_a, material.normal_uv_b);
        n = apply_normal_map(n, in.tangent, normal_uv);
    }
    // 本体と同等の MToon ライティング計算結果を取得（アルファ処理・discard 含む）
    let surface = compute_mtoon_surface_lighting(n, in.uv, in.uv1, in.world_pos);
    // UniVRM 準拠: outlineColor * lerp(1, baseCol, outlineLightingMix)
    let lit = base.rgb * mix(vec3<f32>(1.0), surface.rgb, material.outline_lighting_mix);
    var out: FsOutput;
    out.color = vec4<f32>("#, $output_expr, r#", surface.a);
    out.bloom = vec4<f32>(0.0);
    return out;
}
"#)
    };
}

/// MToon アウトラインシェーダー（sRGB版）
/// 本体と同等の MToon ライティングを計算し、outlineLightingMixFactor で混合
const OUTLINE_SHADER_SRC: &str = concat!(
    wgsl_camera_uniform!(),
    "\n",
    wgsl_material_uniform!(),
    wgsl_mtoon_bindings!(),
    wgsl_mtoon_helpers!(),
    wgsl_outline_body!(),
    wgsl_fs_outline!("lit"),
);

/// MToon アウトラインシェーダー Unorm版（clamp で 0..1 に制限）
/// 本体と同等の MToon ライティングを計算し、outlineLightingMixFactor で混合
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

/// 描画パラメータ（render_to_texture に渡す設定をまとめた構造体）
pub struct RenderParams<'a> {
    pub camera: &'a OrbitCamera,
    pub width: u32,
    pub height: u32,
    /// 中央ビューポート上に被さる下部オーバーレイ（材質編集パネル等）の実 px 高。
    /// view_proj/proj_11 で FOV を補正してパネル開閉前後のモデル見た目サイズを保つ。
    pub overlay_h_pixels: f32,
    pub material_visibility: &'a [bool],
    pub display: &'a super::app::DisplaySettings,
    /// アニメーション済みボーングローバル行列（glTF空間、None=レストポーズ）
    pub animated_bone_globals: Option<&'a [glam::Mat4]>,
    /// VRM 0.0 かどうか（座標変換用）
    pub is_vrm0: bool,
    /// 累積時間（秒、UVアニメーション用）
    pub time: f32,
    /// ホバー中の draw_index 群（オレンジワイヤーフレームでハイライト表示）
    pub hovered_draw_indices: &'a [usize],
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

/// フラグメントシェーダーのオーバーライドモード（GPU uniform に渡す値）
#[derive(Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum ShaderOverride {
    #[default]
    Default = 0,
    Normal = 1,
    Unlit = 2,
    GgxPreview = 3,
}

/// UI ドロップダウン用
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ShaderSelection {
    #[default]
    Auto, // モデル形式に応じて Standard/MMD を自動選択
    Mtoon, // MToon/Lambert 強制（PMX/PMD でも Standard パス）
    Unlit,
    GgxPreview,
    Normal,
    Mmd,
}

/// サンプル数ごとのパイプラインセット
struct PipelineSet {
    pipeline_cull: wgpu::RenderPipeline,
    pipeline_no_cull: wgpu::RenderPipeline,
    pipeline_wireframe: Option<wgpu::RenderPipeline>,
    /// ワイヤーフレームオーバーレイ（Solid+Wire用、depth bias付き）
    pipeline_wire_overlay: Option<wgpu::RenderPipeline>,
    /// 材質ホバーハイライト（オレンジワイヤーフレーム）
    pipeline_highlight: Option<wgpu::RenderPipeline>,
    pipeline_mask_cull: wgpu::RenderPipeline,
    pipeline_mask_no_cull: wgpu::RenderPipeline,
    pipeline_alpha_cull: wgpu::RenderPipeline,
    pipeline_alpha_no_cull: wgpu::RenderPipeline,
    /// 半透明 + デプス書込あり（MToon transparentWithZWrite）
    pipeline_alpha_zwrite_cull: wgpu::RenderPipeline,
    pipeline_alpha_zwrite_no_cull: wgpu::RenderPipeline,
    /// VRM 0.x _CullMode=Front 用（前面カリング）
    pipeline_front_cull: wgpu::RenderPipeline,
    pipeline_mask_front_cull: wgpu::RenderPipeline,
    pipeline_alpha_front_cull: wgpu::RenderPipeline,
    pipeline_alpha_zwrite_front_cull: wgpu::RenderPipeline,
    pipeline_grid: wgpu::RenderPipeline,
    pipeline_bone: wgpu::RenderPipeline,
    pipeline_line_overlay: wgpu::RenderPipeline,
    // MMD パイプライン
    pipeline_mmd_main_cull: Option<wgpu::RenderPipeline>,
    pipeline_mmd_main_no_cull: Option<wgpu::RenderPipeline>,
    pipeline_mmd_alpha_cull: Option<wgpu::RenderPipeline>,
    pipeline_mmd_alpha_no_cull: Option<wgpu::RenderPipeline>,
    pipeline_mmd_edge: Option<wgpu::RenderPipeline>,
    // MToon アウトラインパイプライン（inverted hull 法、Front cull）
    pipeline_outline: wgpu::RenderPipeline,
    // MToon アウトラインパイプライン（BLEND 用、ZWrite OFF）
    pipeline_outline_blend: wgpu::RenderPipeline,
    // MToon アウトラインパイプライン（MASK 用、AlphaToCoverage 有効）
    pipeline_outline_mask: wgpu::RenderPipeline,
}

/// 補助テクスチャスロットで「未割当」を表すためのデフォルト TextureView 集合。
///
/// `GpuRenderer::new()` で一度だけ生成し、`rebuild_material_bind_groups()` から
/// 参照するだけで bind group を作れるようにする（§D / TODO-4）。モデルごとに
/// 複製しないため、GpuRenderer に持たせている。
///
/// - `white_srgb`: shade / rim / outline width などの sRGB スロット既定値
/// - `black_srgb`: matcap の既定値（加算ゼロ）
/// - `flat_normal_unorm`: normal map の既定値（(0,0,1) 相当）
pub struct DefaultViews {
    pub white_srgb: wgpu::TextureView,
    pub black_srgb: wgpu::TextureView,
    pub flat_normal_unorm: wgpu::TextureView,
}

pub struct GpuRenderer {
    /// MSAA パイプラインセット (sample_count=4, sRGB) — 遅延生成
    pipelines_msaa_srgb: Option<PipelineSet>,
    /// 非MSAA パイプラインセット (sample_count=1, sRGB) — 遅延生成
    pipelines_no_msaa_srgb: Option<PipelineSet>,
    /// MSAA パイプラインセット (sample_count=4, Unorm) — 遅延生成
    pipelines_msaa_unorm: Option<PipelineSet>,
    /// 非MSAA パイプラインセット (sample_count=1, Unorm) — 遅延生成
    pipelines_no_msaa_unorm: Option<PipelineSet>,
    // パイプライン遅延生成用リソース
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
    /// カメラ uniform バッファ
    camera_buf: wgpu::Buffer,
    /// カメラ bind group
    camera_bind_group: wgpu::BindGroup,
    /// カメラ bind group layout（BindGroup の lifetime 維持に必要）
    #[expect(dead_code)]
    camera_bgl: wgpu::BindGroupLayout,
    /// テクスチャ bind group layout
    texture_bgl: wgpu::BindGroupLayout,
    /// 材質 bind group layout
    material_bgl: wgpu::BindGroupLayout,
    /// デフォルト白テクスチャ bind group
    default_tex_bind_group: wgpu::BindGroup,
    /// MToon 補助テクスチャ bind group layout (group 3)
    mtoon_aux_bgl: wgpu::BindGroupLayout,
    /// デフォルト MToon 補助 bind group（matcap=黒、他=白）
    default_mtoon_aux_bind_group: wgpu::BindGroup,
    /// 共通テクスチャサンプラー（毎回生成を回避）
    default_sampler: wgpu::Sampler,
    /// 材質編集時の bind group 再生成に使う「未割当」デフォルト TextureView 集合（§D）
    default_views: DefaultViews,
    /// グリッド頂点バッファ
    grid_vbuf: wgpu::Buffer,
    grid_vertex_count: u32,
    /// ボーンテールバッファ（LineList、テール三角形）
    bone_tail: DynamicBuffer,
    /// ボーン塗りつぶしバッファ（TriangleList、マーカー塗り面）
    bone_fill: DynamicBuffer,
    /// ボーン外枠バッファ（LineList、マーカー外枠線）
    bone_line: DynamicBuffer,
    /// SpringBone頂点バッファ
    spring: DynamicBuffer,
    /// ジョイント面バッファ（TriangleList）
    joint: DynamicBuffer,
    /// ジョイントエッジバッファ（LineList）
    joint_edge: DynamicBuffer,
    /// 法線表示頂点バッファ
    normal: DynamicBuffer,
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
    /// ボーンテール頂点生成用作業バッファ
    bone_tail_work: Vec<GridVertex>,
    /// ボーン塗りつぶし頂点生成用作業バッファ
    bone_fill_work: Vec<GridVertex>,
    /// ボーン外枠線頂点生成用作業バッファ
    bone_work: Vec<GridVertex>,
    /// 法線頂点生成用作業バッファ
    normal_work: Vec<GridVertex>,
    /// 法線頂点dedup用作業バッファ
    normal_seen: std::collections::HashSet<(u32, u32, u32, u32, u32, u32)>,
    /// 法線頂点可視フラグ作業バッファ
    normal_visible_work: Vec<bool>,
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
    /// 半透明ソート用: DrawCall 重心の作業バッファ
    work_draw_centers: Vec<glam::Vec3>,
    /// 半透明ソート用: ソート済みインデックスの作業バッファ
    work_sorted_indices: Vec<usize>,
    /// 半透明ソートキャッシュ: 前回ソート時のカメラ eye 位置
    cache_sort_eye: Option<glam::Vec3>,
    /// 半透明ソートキャッシュ: 前回ソート時の DrawCall 数
    cache_sort_draw_count: usize,
    /// 半透明ソートキャッシュ: 前回の頂点ポインタ（アニメーション変更検出用）
    cache_sort_vert_ptr: usize,
    /// 半透明ソート強制再計算フラグ
    sort_dirty: bool,
    /// encase uniform シリアライズ用再利用バッファ（毎フレーム heap allocation 回避）
    encase_work: Vec<u8>,
    // MMD リソース
    mmd_material_bgl: wgpu::BindGroupLayout,
    mmd_aux_bgl: wgpu::BindGroupLayout,
    #[expect(dead_code)]
    shared_toon_textures: [wgpu::TextureView; 10],
    shared_toon_textures_unorm: [wgpu::TextureView; 10],
    shared_toon_sampler: wgpu::Sampler,
    default_mmd_aux_bind_group: wgpu::BindGroup,
    /// Bloom ポストエフェクト
    bloom: super::bloom::BloomPass,
}

/// MSAA サンプル数
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
    /// MRT bloom source テクスチャ (Rgba8Unorm, linear, sample_count=1)
    _bloom_source: wgpu::Texture,
    bloom_source_view: wgpu::TextureView,
    /// MRT bloom source MSAA テクスチャ（MSAA 有効時のみ）
    _msaa_bloom_source: Option<wgpu::Texture>,
    msaa_bloom_source_view: Option<wgpu::TextureView>,
    width: u32,
    height: u32,
    msaa: bool,
}

impl GpuRenderer {
    /// 材質編集時の bind group 再生成から参照する「未割当」既定 view 集合（§D）。
    pub fn default_views(&self) -> &DefaultViews {
        &self.default_views
    }

    /// 共通サンプラー。材質編集 UI の rebuild 経路からも使えるよう公開する（§D）。
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

        // MToon 補助テクスチャ bind group layout (group 3)
        let mtoon_aux_bgl = create_mtoon_aux_bind_group_layout(device);

        // Default MToon 補助 bind group（matcap=黒、他=白、normal=フラット）
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
            }, // matcap: 黒
            AuxTexEntry {
                view: &white_srgb_view,
                sampler: s,
            }, // shade_multiply: 白
            AuxTexEntry {
                view: &white_srgb_view,
                sampler: s,
            }, // shading_shift: 白
            AuxTexEntry {
                view: &white_srgb_view,
                sampler: s,
            }, // rim_multiply: 白
            AuxTexEntry {
                view: &white_srgb_view,
                sampler: s,
            }, // uv_anim_mask: 白
            AuxTexEntry {
                view: &white_srgb_view,
                sampler: s,
            }, // outline_width: 白
            AuxTexEntry {
                view: &white_srgb_view,
                sampler: s,
            }, // emissive: 白
            AuxTexEntry {
                view: &flat_normal_view,
                sampler: s,
            }, // normal: フラット
        );

        // §D: bind group 生成で使い終わった view を DefaultViews に集約し、
        // GpuRenderer が所有する。材質編集による rebuild_material_bind_groups
        // から「未割当」スロット用に参照できるようになる。
        let default_views = DefaultViews {
            white_srgb: white_srgb_view,
            black_srgb: black_view,
            flat_normal_unorm: flat_normal_view,
        };

        // Shader modules（パイプライン遅延生成のため保持）
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

        // MMD シェーダーモジュール（sRGB 版: pow(2.2) 付き）
        let mmd_edge_shader_srgb = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mmd_edge_shader"),
            source: wgpu::ShaderSource::Wgsl(MMD_EDGE_SHADER_SRC.into()),
        });
        let mmd_main_shader_srgb = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mmd_main_shader"),
            source: wgpu::ShaderSource::Wgsl(MMD_MAIN_SHADER_SRC.into()),
        });

        // MMD シェーダーモジュール（Unorm 版: pow(2.2) 除去）
        let mmd_edge_shader_unorm = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mmd_edge_shader_unorm"),
            source: wgpu::ShaderSource::Wgsl(MMD_EDGE_SHADER_UNORM_SRC.into()),
        });
        let mmd_main_shader_unorm = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mmd_main_shader_unorm"),
            source: wgpu::ShaderSource::Wgsl(MMD_MAIN_SHADER_UNORM_SRC.into()),
        });

        // MToon アウトラインシェーダー（sRGB 版 / Unorm 版）
        let outline_shader_srgb = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("outline_shader"),
            source: wgpu::ShaderSource::Wgsl(OUTLINE_SHADER_SRC.into()),
        });
        let outline_shader_unorm = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("outline_shader_unorm"),
            source: wgpu::ShaderSource::Wgsl(OUTLINE_SHADER_UNORM_SRC.into()),
        });

        // グリッドシェーダー（Unorm 版: linear_to_srgb 付き）
        let grid_shader_unorm = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("grid_shader_unorm"),
            source: wgpu::ShaderSource::Wgsl(GRID_SHADER_UNORM_SRC.into()),
        });

        // MMD パイプラインレイアウト
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

        // 共有トゥーンテクスチャ (toon01-10)
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

        // デフォルト MMD aux bind group (白sphere + 白toon、Unorm ビュー)
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

        // パイプラインセットは遅延生成（初回描画時に必要なセットのみ作成）

        // 共通サンプラー（テクスチャ bind group 作成時に使い回す）
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

    /// 半透明ソートの強制再計算フラグを立てる（モデル追加・リロード時に呼ぶ）
    pub fn mark_sort_dirty(&mut self) {
        self.sort_dirty = true;
    }

    /// モデルの bbox に合わせてグリッドバッファを再構築する
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

    /// 可視化バッファのキャッシュを無効化（モデル再読み込み時に呼ぶ）
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
        // MASK 用: blend なし（UniVRM MToonValidator 準拠: SrcBlend=One, DstBlend=Zero）
        // AlphaToCoverage がカバレッジマスクを制御するため、アルファブレンドは不要
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
        // アウトライン用 depth bias（UniVRM Offset 1,1 相当）— Reverse-Z で符号反転
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

        // bloom MRT ターゲット（emissive-only、Rgba8Unorm linear）
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

        // ワイヤーフレームオーバーレイ（Solid+Wire用: depth bias でZファイティング回避）
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
            // ワイヤーオーバーレイ用カラーターゲット（アルファブレンド）
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

        // ハイライト用パイプライン（半透明オレンジ塗りつぶし）
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

        // BLEND + ZWrite On パイプライン（MToon transparentWithZWrite 用）
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

        // VRM 0.x _CullMode=Front 用パイプライン（前面カリング）
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

        // MMD エッジパイプライン: Front cull, 2スロット頂点バッファ
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

        // MMD メインパイプライン（4種: cull/no_cull × opaque/alpha）
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

        // MToon アウトラインパイプライン: Front cull (inverted hull)
        // edge_scale は GPU 側で outlineWidthMultiplyTexture をサンプリングするため不要
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
        // BLEND 用アウトラインパイプライン（ZWrite OFF）
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
        // MASK 用アウトラインパイプライン（AlphaToCoverage 有効）
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

    /// ワイヤーフレーム対応かどうか
    pub fn supports_wireframe(&self) -> bool {
        self.supports_wireframe
    }

    /// 必要なパイプラインセットが未生成なら遅延生成する
    pub(crate) fn ensure_pipelines(&mut self, device: &wgpu::Device, use_unorm: bool, msaa: bool) {
        // 既に生成済みなら何もしない
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

    /// 現在の MSAA 設定と Unorm フラグに応じたパイプラインセットを取得
    /// （事前に ensure_pipelines を呼んでおくこと）
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

    /// テクスチャ bind group layout への参照
    pub fn texture_bgl(&self) -> &wgpu::BindGroupLayout {
        &self.texture_bgl
    }

    /// 材質 bind group layout への参照
    pub fn material_bgl(&self) -> &wgpu::BindGroupLayout {
        &self.material_bgl
    }

    /// MToon 補助テクスチャ bind group layout への参照
    pub fn mtoon_aux_bgl(&self) -> &wgpu::BindGroupLayout {
        &self.mtoon_aux_bgl
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

        // offscreen テクスチャ再作成に伴い Bloom の外部依存キャッシュを無効化
        self.bloom.invalidate_external_cache();

        let tex_size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        // MSAA カラーテクスチャ（マルチサンプル、描画先）— MSAA 有効時のみ
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

        // リゾルブ先カラーテクスチャ（sample_count=1、egui表示用）
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

        // MRT bloom source テクスチャ (Rgba8Unorm, linear)
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
                // テールバッファ（LineList）
                self.bone_tail.vertex_count = self.bone_tail_work.len() as u32;
                self.bone_tail.upload(
                    device,
                    queue,
                    bytemuck::cast_slice(&self.bone_tail_work),
                    "bone_tail_vbuf",
                );
                // 塗りバッファ（TriangleList）
                self.bone_fill.vertex_count = self.bone_fill_work.len() as u32;
                self.bone_fill.upload(
                    device,
                    queue,
                    bytemuck::cast_slice(&self.bone_fill_work),
                    "bone_fill_vbuf",
                );
                // 外枠バッファ（LineList）
                self.bone_line.vertex_count = self.bone_work.len() as u32;
                self.bone_line.upload(
                    device,
                    queue,
                    bytemuck::cast_slice(&self.bone_work),
                    "bone_vbuf",
                );
            }
        }

        // 法線表示頂点を更新（入力が変わった時、またはアニメーション中に再生成）
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
                self.normal_dirty = true; // 再表示時に再生成するためフラグを立てる
            }
            self.normal.vertex_count = 0;
        }

        // SpringBone/Joint共通: ボーンデルタを1回だけ計算
        let need_spring = params.display.show_spring_bones
            && (!ir.physics.rigid_bodies.is_empty() || !ir.physics.joints.is_empty());
        let need_joint = params.display.show_joints && !ir.physics.joints.is_empty();
        let bone_deltas = if (need_spring || need_joint) && has_anim {
            compute_bone_deltas(ir, params.animated_bone_globals, params.is_vrm0)
        } else {
            None
        };

        // SpringBone頂点を毎フレーム更新
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

        // ジョイント頂点を毎フレーム更新
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
                // 面バッファ（TriangleList）
                self.joint.vertex_count = self.joint_work.len() as u32;
                self.joint.upload(
                    device,
                    queue,
                    bytemuck::cast_slice(&self.joint_work),
                    "joint_vbuf",
                );
                // エッジバッファ（LineList）
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

        let mmd_mode = params.display.use_mmd_path;
        let mmd_edge_enabled = params.display.mmd_edge_enabled;
        // ワイヤーフレーム/シェーダーオーバーライド時は MMD パスを使わず既存パイプラインにフォールバック
        let mmd_solid = mmd_mode
            && params.display.draw_mode == DrawMode::Solid
            && params.display.shader_override == ShaderOverride::Default;

        // MMD 描画が必要かどうかを事前チェック
        let has_mmd_draws = mmd_solid
            && model.draws.iter().any(|d| {
                d.render_style == super::mesh::RenderStyle::Mmd
                    && d.mmd_material_bind_group.is_some()
            });

        // Unorm フレーム判定: MMD 専用パスに完全に乗るフレームのみ
        let use_unorm = can_use_unorm_frame(model, params.material_visibility, mmd_solid);

        // パイプラインセットの遅延生成（未作成なら初回のみコンパイル）
        self.ensure_pipelines(device, use_unorm, self.current_msaa);

        let offscreen = self
            .offscreen
            .as_ref()
            .expect("ensure_offscreen で初期化済み");

        // カメラユニフォーム更新（再利用バッファで heap allocation 回避）
        let cam_uniform = Self::build_camera_uniform(params);
        self.encase_work.clear();
        let mut encase_buf = encase::UniformBuffer::new(&mut self.encase_work);
        encase_buf.write(&cam_uniform).expect("encase write");
        queue.write_buffer(&self.camera_buf, 0, encase_buf.as_ref());

        // Encode
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("offscreen_encoder"),
        });

        // take で借用衝突を回避（self.pipelines() が self 全体を immutable borrow するため）
        let mut work_draw_centers = std::mem::take(&mut self.work_draw_centers);
        let mut work_sorted_indices = std::mem::take(&mut self.work_sorted_indices);

        // 描画順インデックス構築
        let pending_sort_cache = self.build_draw_queue(
            model,
            params,
            &mut work_draw_centers,
            &mut work_sorted_indices,
        );

        let ps = self.pipelines(use_unorm);

        // カラービュー選択: use_unorm に応じて Unorm / sRGB ビュー
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

        // クリアカラー補正: Unorm ターゲットに書く値は egui が sRGB デコードするため事前エンコード
        let bg = if use_unorm {
            linear_to_srgb_f64(params.display.bg_brightness as f64)
        } else {
            params.display.bg_brightness as f64
        };

        // bloom source ビュー選択（MRT 2つ目のターゲット、常に Rgba8Unorm）
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

        // ===== Pass 1 (MRT): メッシュ描画 — color + bloom_source の 2 ターゲット =====
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

            // Standard メッシュ描画（空モデルの場合はスキップ）
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

            // MMD 描画（材質インデックス順 — PMX の描画順序を維持）
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

            // 材質ホバーハイライト（オレンジワイヤーフレーム）
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

        // ===== Pass 2 (1ターゲット): グリッド + オーバーレイ（法線・ボーン・剛体・ジョイント）=====
        // NOTE: MSAA 有効時、LoadOp::Load はタイルベース GPU（Intel iGPU 等）で MSAA カラー
        // テクスチャの VRAM→タイル読み戻しが発生し帯域コストがかかる。
        // Pass 1 (MRT: 2ターゲット) と統合すればレンダーパス開始を1回に削減できるが、
        // オーバーレイパイプラインが1ターゲットで作成されているため MRT 互換化
        // （2番目のターゲットに write_mask::EMPTY を追加）が必要になる。
        // 現時点では実測での問題報告がなく、パイプライン複雑化のコストに見合わないため
        // 現状維持とする。将来 MSAA+タイル GPU で帯域がボトルネックになった場合に再検討。
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

        // 作業バッファを返却（容量を保持して次フレームで再利用）
        self.work_draw_centers = work_draw_centers;
        self.work_sorted_indices = work_sorted_indices;

        // 半透明ソートキャッシュ更新（ps 借用解放後にまとめて書き込み）
        if let Some((eye, draw_count, vert_ptr)) = pending_sort_cache {
            self.cache_sort_eye = Some(eye);
            self.cache_sort_draw_count = draw_count;
            self.cache_sort_vert_ptr = vert_ptr;
            self.sort_dirty = false;
        }

        // --- Bloom ポストエフェクト ---
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

        // テクスチャ登録（bloom 有効時は composite 出力を、無効時は offscreen をそのまま使う）
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
    // render_to_texture ヘルパー群
    // -----------------------------------------------------------------------

    /// カメラユニフォームを構築する
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

    /// 描画順インデックスをソートする（半透明のカメラ距離ソート含む）
    ///
    /// ソートが行われた場合は `Some((eye, draw_count, vert_ptr))` を返す。
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

    /// MToon 4段階描画（Standard RenderStyle のメッシュ描画 + アウトライン）
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
                            .expect("wireframe パイプラインは supports_wireframe チェック済み"),
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

                // BLEND/BlendZWrite: サーフェス直後にアウトライン描画（インターリーブ）
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

            // OPAQUE/MASK: アウトラインをフェーズ後にまとめて描画
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

        // Solid+Wire オーバーレイ
        if use_solid_wire {
            let wire_pl = ps
                .pipeline_wire_overlay
                .as_ref()
                .expect("wire_overlay パイプラインは supports_wireframe チェック済み");
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

    /// MMD 描画パス（材質インデックス順）
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
                        .expect("wireframe パイプラインは supports_wireframe チェック済み"),
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

            // 不透明材質のエッジをその場で描画（Wire モードではスキップ）
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
                    // エッジ描画後にメインバッファを復元
                    pass.set_vertex_buffer(0, model.vertex_buf.slice(..));
                    pass.set_index_buffer(model.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                }
            }
        }
    }

    /// 材質ホバーハイライト描画（オレンジワイヤーフレーム）
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

    /// Pass 2: グリッド + 可視化オーバーレイ（法線・ボーン・剛体・ジョイント）
    fn draw_overlays<'p>(
        &'p self,
        pass: &mut wgpu::RenderPass<'p>,
        params: &RenderParams,
        ps: &'p PipelineSet,
    ) {
        // グリッド描画
        if params.display.show_grid {
            pass.set_pipeline(&ps.pipeline_grid);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_vertex_buffer(0, self.grid_vbuf.slice(..));
            pass.draw(0..self.grid_vertex_count, 0..1);
        }

        // 法線表示（LineList オーバーレイ）
        if params.display.show_normals && self.normal.vertex_count > 0 {
            if let Some(ref buf) = self.normal.buf {
                pass.set_pipeline(&ps.pipeline_line_overlay);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..self.normal.vertex_count, 0..1);
            }
        }

        // ボーン描画（3段階: テール → 塗り → 外枠）
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

        // 剛体描画
        if params.display.show_spring_bones && self.spring.vertex_count > 0 {
            if let Some(ref buf) = self.spring.buf {
                pass.set_pipeline(&ps.pipeline_line_overlay);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..self.spring.vertex_count, 0..1);
            }
        }

        // ジョイント描画
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

    /// MMD 用 GPU リソースを DrawCall に構築（全 GPU モデル生成経路から呼ぶ）
    pub fn prepare_mmd_resources(
        &self,
        device: &wgpu::Device,
        model: &mut GpuModel,
        ir: &IrModel,
        emissive_per_mat: &[bool],
    ) {
        use super::mesh::RenderStyle;

        // draws を一時的に取り出して借用衝突を回避
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

    /// 1 つの DrawCall に対する MMD 互換パスの bind group 3 系統（material / texture / aux）を
    /// 再生成する（§C）。
    ///
    /// `prepare_mmd_resources` の一括処理と `rebuild_material_bind_groups` の per-material
    /// 経路の両方から呼ばれる。`RenderStyle::Mmd` 以外が渡された場合は何もせず早期 return する。
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

        // MMD メインテクスチャ bind group（Unorm ビュー）
        let mmd_tex_bg = mat.texture_index.and_then(|ti| {
            gpu_textures_unorm.get(ti).map(|unorm_view| {
                create_texture_bind_group(device, &self.texture_bgl, unorm_view, tex_sampler)
            })
        });
        draw.mmd_texture_bind_group = mmd_tex_bg;

        // sphere/toon aux bind group（Unorm ビュー）
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

        // sphere/toon がない場合は shared_toon_textures_unorm[0]（白グラデ）をフォールバック
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

    /// 材質編集ドロワーからの dirty 通知を受けて、1 材質ぶんの全 bind group を再生成する（§C）。
    ///
    /// 標準パス（material_bind_group + mtoon_aux_bind_group）と MMD 互換パス
    /// （mmd_material/mmd_texture/mmd_aux の 3 系統）の両方を同時に更新する。
    /// `use_mmd_path` 切替時に編集が片方のパスで消えないように、必ず両方を走らせる。
    ///
    /// 現行 `DrawCall` は `wgpu::BindGroup` のみを保持して `wgpu::Buffer` ハンドルを持たないため
    /// `queue.write_buffer` による部分更新は構造上不可能。bind group ごと再生成する方針で割り切り
    /// （プラン §C に明記、将来最適化は DrawCall.material_buf 追加で別 PR にて実施）。
    /// 材質パラメータの変更を GPU に反映する。
    ///
    /// - `uniform_only = true`: カラー/スカラーのみ変更。`queue.write_buffer` で部分更新し、
    ///   bind group 再生成をスキップする（材質エディタのスライダー操作、Expression 材質バインド用）。
    /// - `uniform_only = false`: テクスチャ変更を含む。bind group + aux bind group を再生成する。
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

        // §C: 純関数で params / aux_refs を計算
        let params = build_material_params_for(mat, mat_idx, flags);

        if uniform_only {
            // ファストパス: buffer 書き込みのみ、bind group 再生成なし
            for draw in &mut model.draws {
                if draw.material_index == mat_idx {
                    write_material_buffer(queue, &draw.material_buf, &params);
                }
            }
            return;
        }

        // フルパス: bind group 再生成（テクスチャ変更時）
        let aux_refs = build_aux_refs_for(mat);

        // 借用衝突を回避するため draws を一時的に奪う（prepare_mmd_resources と同じパターン）
        let mut draws = std::mem::take(&mut model.draws);
        let gpu_texture_views = &model.gpu_texture_views;
        let gpu_texture_views_unorm = &model.gpu_texture_views_unorm;

        for draw in &mut draws {
            if draw.material_index != mat_idx {
                continue;
            }

            // 標準パス: material uniform buffer を更新
            write_material_buffer(queue, &draw.material_buf, &params);

            // v0.5.1 レビュー [P1] 対応: 標準パスの texture_bind_group（BaseColor）も再生成する。
            // 旧実装は aux / mmd のみ更新していたため、pristine 復元や texture_index の変更が
            // GPU 側に反映されず、古い BaseColor テクスチャ bind が画面に残る不具合があった。
            //
            // レビュー 04 [P1] 対応: 情報源を初回 DrawCall 構築と揃える（mesh.rs:1256）。
            // 旧実装は `mat.base_color_tex_info` のみを参照していたが、PMX/PMD 材質は
            // `texture_index` を持つ一方で `base_color_tex_info` が None のため、
            // full rebuild で `texture_bind_group = None` になり白テクスチャに後退していた。
            // 修正: `texture_index` を優先し、サンプラーは `base_color_tex_info.sampler` があれば
            // それ、なければデフォルト `IrSamplerInfo` にフォールバック。
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

            // 標準パス: mtoon_aux_bind_group（aux_refs が Some の材質のみ）
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

            // MMD 互換パス: RenderStyle::Mmd のみ（prepare_mmd_resources と同じ条件）
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

    /// MMD Material BGL への参照（外部用）
    pub fn mmd_material_bgl(&self) -> &wgpu::BindGroupLayout {
        &self.mmd_material_bgl
    }

    /// デフォルト MMD aux bind group への参照
    pub fn default_mmd_aux_bind_group(&self) -> &wgpu::BindGroup {
        &self.default_mmd_aux_bind_group
    }
}

/// MMD 専用パスに完全に乗るフレームかどうかを判定
/// true の場合 Unorm レンダーターゲットを使用し、ガンマ空間で正確な描画を行う
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

/// 正規 sRGB 変換（f64精度、クリアカラー補正用）
fn linear_to_srgb_f64(v: f64) -> f64 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
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
/// IrTextureInfo から MaterialUniform 用 UV パラメータをパック
/// 返り値: ([tex_coord, offset.x, offset.y, rotation], [scale.x, scale.y, 0, 0])
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

/// `create_material_bind_group` に渡すマテリアルパラメータ群
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

/// `MaterialParams` を `MaterialUniform` に変換し encase でシリアライズする。
/// `create_material_buffer_and_bind_group` / `write_material_buffer` の共通パス。
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

/// 材質 uniform バッファ（`UNIFORM | COPY_DST`）と bind group を同時に作成する。
/// モデルロード時に使用。返された `wgpu::Buffer` は `DrawCall.material_buf` に保持し、
/// `write_material_buffer` で per-frame 部分更新が可能。
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

/// 既存の材質 uniform バッファに新しいパラメータを書き込む。
/// bind group 再生成なしで GPU 上の材質パラメータを更新する。
/// Expression 材質バインドや材質エディタのカラー/スカラー変更時に使用。
pub fn write_material_buffer(queue: &wgpu::Queue, buf: &wgpu::Buffer, params: &MaterialParams) {
    let bytes = serialize_material_uniform(params);
    queue.write_buffer(buf, 0, &bytes);
}

/// 後方互換シム: `create_material_bind_group` のシグネチャを維持。
/// 新規コードでは `create_material_buffer_and_bind_group` を使うこと。
pub fn create_material_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    params: &MaterialParams,
) -> wgpu::BindGroup {
    let (_, bg) = create_material_buffer_and_bind_group(device, layout, params);
    bg
}

/// MToon 補助テクスチャ bind group layout (group 3) を作成
/// テクスチャごとに sampler を持つ（glTF の texture 単位 sampler に準拠）
/// binding 2n: sampler, binding 2n+1: texture_2d（8 テクスチャ × 2 = 16 bindings）
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
            sampler_entry(8, vert_frag), // s_uv_anim_mask（頂点シェーダーからも参照）
            tex_entry(9, vert_frag),     // t_uv_anim_mask
            sampler_entry(10, vert),     // s_outline_width（頂点シェーダーのみ）
            tex_entry(11, vert),         // t_outline_width
            sampler_entry(12, frag),     // s_emissive
            tex_entry(13, frag),         // t_emissive
            sampler_entry(14, frag),     // s_normal
            tex_entry(15, frag),         // t_normal
        ],
    })
}

/// 補助テクスチャ 1 枚分（テクスチャビュー + サンプラー）
pub struct AuxTexEntry<'a> {
    pub view: &'a wgpu::TextureView,
    pub sampler: &'a wgpu::Sampler,
}

/// MToon 補助テクスチャ bind group を作成（テクスチャごとに sampler を持つ）
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

/// MToon 補助テクスチャ bind group layout を公開で作成
pub fn create_mtoon_aux_bind_group_layout_pub(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    create_mtoon_aux_bind_group_layout(device)
}

/// 1x1 白テクスチャの sRGB TextureView を作成（MToon 補助 bind group デフォルト用）
pub fn create_white_texture_view_srgb(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> wgpu::TextureView {
    let (srgb, _) = create_white_texture_view(device, queue);
    srgb
}

/// 1x1 黒テクスチャの TextureView を作成（公開版）
pub fn create_black_texture_view_pub(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> wgpu::TextureView {
    create_black_texture_view(device, queue)
}

/// 1x1 白テクスチャの TextureView を作成（MMD デフォルト用）
/// 戻り値: (sRGB ビュー, Unorm ビュー)
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

/// 1x1 黒テクスチャの TextureView を作成（MatCap デフォルト用: RGB=0 で無効化）
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

/// 1x1 フラット法線テクスチャの Unorm TextureView を作成
/// tangent-space (0,0,1) = RGBA(128,128,255,255) — 法線マップなしと等価
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

/// 共有トゥーンテクスチャ (toon01-10) を CPU で生成し GPU にアップロード
/// 戻り値: (sRGB ビュー配列, Unorm ビュー配列)
///
/// MMD 標準の toon01-10.bmp (32×32px) の各行・左端ピクセル色を忠実に再現する。
/// シェーダーは U=0.0 固定でサンプルするため列方向の色差は無視できる。
fn generate_shared_toon_textures(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> ([wgpu::TextureView; 10], [wgpu::TextureView; 10]) {
    // MMD 標準トゥーンの行ごとの RGB 値 (row 0=上端, row 31=下端)
    // ---------------------------------------------------------------
    // toon01-04: row 0-15 = 白, row 16-31 = shadow色 の 2色ステップ
    // toon05-06: MMD 参照データ (不規則グラデーション、LUT)
    // toon07-10: 全白 (トゥーン効果なし)
    // ---------------------------------------------------------------

    /// 上半分=白, 下半分=shadow の 2色ステップテクスチャを生成
    const fn toon_step(shadow: [u8; 3]) -> [[u8; 3]; 32] {
        let mut rows = [[255u8, 255, 255]; 32];
        let mut i = 16;
        while i < 32 {
            rows[i] = shadow;
            i += 1;
        }
        rows
    }

    /// (色, 繰り返し回数) のランレングスから 32行テクスチャを展開
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

    const TOON01: [[u8; 3]; 32] = toon_step([205, 205, 205]); // 白→灰
    const TOON02: [[u8; 3]; 32] = toon_step([245, 225, 225]); // 白→ピンク系
    const TOON03: [[u8; 3]; 32] = toon_step([154, 154, 154]); // 白→暗灰
    const TOON04: [[u8; 3]; 32] = toon_step([248, 239, 235]); // 白→暖色ベージュ

    // toon05: 白→暖ピンクのグラデーション (MMD 参照 LUT)
    #[rustfmt::skip]
    const TOON05: [[u8; 3]; 32] = toon_rle([
        ([255,255,255], 19), ([255,254,254], 1), ([255,250,248], 1),
        ([255,246,242], 1),  ([255,240,234], 1), ([255,236,229], 1),
        ([255,233,224], 1),  ([255,231,222], 1), ([255,231,221], 4),
        ([255,231,222], 1),  ([254,232,223], 1),
    ]);

    // toon06: 黄色系 — 中央ハイライトバンド + 暗黄 (MMD 参照 LUT)
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
        views_srgb
            .try_into()
            .expect("10個のトゥーンテクスチャ(srgb)"),
        views_unorm
            .try_into()
            .expect("10個のトゥーンテクスチャ(unorm)"),
    )
}

/// ボーン形状の種別（優先順: IK > 軸制限 > 移動 > 通常）
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum BoneShape {
    Normal,    // ◎ 二重円（中心塗りつぶし）
    Move,      // ◻ 正方形（中心塗りつぶし）
    AxisFixed, // ⊗ 円＋✕
    Ik,        // ◻ 外枠青＋中オレンジ＋中心青正方形
}

/// ボーン表示用ジオメトリを生成（毎フレーム、カメラ向きビルボード）
/// out_tails: LineList 用（テール三角形）— 最背面
/// out_fill:  TriangleList 用（マーカー塗りつぶし面）— 中間
/// out_lines: LineList 用（マーカー外枠・✕）— 最前面
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
    let orange = [1.0, 0.4, 0.0, opacity]; // 濃いオレンジ #ff6600
    let outer_factor = BONE_DISPLAY_RADIUS;
    let inner_factor = BONE_JOINT_RADIUS;
    let ik_center_factor = inner_factor; // IKコントローラ中心の青正方形（移動ボーンと同サイズ）
    let segments = SPHERE_SEGMENTS;

    // 描画優先順: 通常 → IK影響下(orange) → 軸制限 → IKコントローラ
    // （後に描画されるほど手前に表示される）
    for pass in 0..4u8 {
        for (bone_i, bone) in ir.bones.iter().enumerate() {
            if !bone.is_visible {
                continue;
            }

            // 形状決定（優先順: IK > 軸制限 > 移動 > 通常）
            let shape = if bone.is_ik_bone {
                BoneShape::Ik
            } else if bone.is_axis_fixed {
                BoneShape::AxisFixed
            } else if bone.is_translatable {
                BoneShape::Move
            } else {
                BoneShape::Normal
            };

            // パス振り分け: 0=通常, 1=IK影響下(orange), 2=軸制限, 3=IKコントローラ
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

            // IK影響下ボーンはオレンジ、それ以外は青
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

            // 中心塗りつぶし色: IK影響下ボーンでも中心はブルー
            let center_color = blue;

            // △: self→tail / parent→self の三角形（線）— マーカーより先に描画
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

            // 外枠の太線用オフセット（2本描画で太さ2倍）
            let thick = dist * 0.0003;

            // マーカー描画（形状別）— テールの上に重ねて描画
            // 塗り（TriangleList）→ 線（LineList）の順で描画されるため、
            // 塗りは内円/内正方形サイズいっぱいに描画し、線の外枠がその上に重なる
            match shape {
                BoneShape::Normal => {
                    // ◎: 内円塗りつぶし + 外円（太線）・内円の線
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
                    // ◻: 内正方形塗りつぶし + 外正方形（太線）・内正方形の線
                    draw_filled_square_tri(out_fill, pos, right, up, r_inner, center_color);
                    draw_square(out_lines, pos, right, up, r_outer - thick, color);
                    draw_square(out_lines, pos, right, up, r_outer + thick, color);
                    draw_square(out_lines, pos, right, up, r_inner, color);
                }
                BoneShape::AxisFixed => {
                    // ⊗: 外円（太線） + ✕（太線、外円サイズ）
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
                    // IKコントローラ: オレンジ外枠いっぱい塗り + 青中心塗り + 青外枠（太線）
                    draw_filled_square_tri(out_fill, pos, right, up, r_outer, orange);
                    draw_filled_square_tri(out_fill, pos, right, up, r_ik_center, blue);
                    draw_square(out_lines, pos, right, up, r_outer - thick, blue);
                    draw_square(out_lines, pos, right, up, r_outer + thick, blue);
                }
            }
        }
    } // 4パス終了
}

/// 円を描画（LineList、segments 個の線分）
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

/// 正方形を描画（LineList、4辺）
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

/// 塗りつぶし円（TriangleList、三角形ファン）
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

/// 塗りつぶし正方形（TriangleList、2三角形）
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
    // 三角形1: tl-tr-br
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
    // 三角形2: tl-br-bl
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

/// LineList 用 1線分を push
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

/// ボーン三角形を描画（底辺＝base、頂点＝tip）
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

    // 左辺: base_l → tip
    out.push(GridVertex {
        position: base_l.to_array(),
        color,
    });
    out.push(GridVertex {
        position: tip.to_array(),
        color,
    });
    // 右辺: base_r → tip
    out.push(GridVertex {
        position: base_r.to_array(),
        color,
    });
    out.push(GridVertex {
        position: tip.to_array(),
        color,
    });
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

/// アニメーション用ボーンデルタ（位置差分・回転差分）を計算
/// SpringBone頂点とJoint頂点の両方で共有する
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

/// SpringBone物理ビジュアル用ジオメトリを生成
/// - 剛体: ワイヤフレーム風のリング+接続線で形状を表現
/// - ジョイント: 接続する2剛体間の線
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
    // VRM: group基準（コライダー=赤, スプリング=緑）
    let collider_color = [1.0, 0.0, 0.0, opacity]; // レッド #ff0000（group=1: コライダー）
    let spring_color = [0.0, 1.0, 0.0, opacity]; // グリーン #00ff00（group!=1: スプリングチェーン）
                                                 // PMX/PMD: physics_mode基準（0:ボーン追従=緑, 1:物理演算=赤, 2:物理+ボーン=青）
    let bone_follow_color = [0.0, 1.0, 0.0, opacity]; // グリーン
    let physics_color = [1.0, 0.0, 0.0, opacity]; // レッド
    let physics_bone_color = [0.0, 0.5, 1.0, opacity]; // ブルー

    let segments = SPHERE_SEGMENTS;
    let line_width = 0.0_f32; // 1px描画（draw_ring/draw_line_quad の _width 引数用）

    // bone.position はすべての形式で glTF 空間に格納されている（PMX/PMD も pmx_pos_to_gltf 済み）
    // rb.position は PMX 空間なので、bone 側を PMX 空間に戻して差分を取る
    let pos_fn = crate::convert::coord::pos_fn(is_vrm0);

    // 剛体の形状を描画
    for rb in &ir.physics.rigid_bodies {
        let color = if ir.source_format.is_pmx_pmd() {
            match rb.physics_mode {
                0 => bone_follow_color,  // ボーン追従
                1 => physics_color,      // 物理演算
                _ => physics_bone_color, // 物理+ボーン
            }
        } else if rb.group == 1 {
            collider_color
        } else {
            spring_color
        };

        // PMX Euler → 回転クォータニオン（YXZ intrinsic = ZXY extrinsic: R = Rz * Rx * Ry）
        // D3DX行優先: v * Ry * Rx * Rz → glam列優先: Rz * Rx * Ry
        // PMX/PMD: 回転は常にファイルの値を使用。VRM: align_rigid_rotation 有効時のみ
        let rotation = if ir.source_format.is_pmx_pmd() || align_rigid_rotation {
            rb.rotation
        } else {
            Vec3::ZERO
        };
        let mut quat =
            glam::Quat::from_euler(glam::EulerRot::YXZ, rotation.y, rotation.x, rotation.z);

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
                // 7本の緯線（上から下へ等間隔）
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
                // カプセル: Y軸がボーン方向
                // 高さ = 球体中心間距離（PMX仕様: height = 全長 - 2*radius ではなく球体中心間距離）
                let half_h = height * 0.5;

                // 上端・下端のリング
                let top_offset = quat * Vec3::new(0.0, half_h, 0.0);
                let bot_offset = quat * Vec3::new(0.0, -half_h, 0.0);

                let top_center = rb_pos + top_offset;
                let bot_center = rb_pos + bot_offset;

                // 赤道リング（上端・下端）
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

                // PMX/PMD: 両端に半球ワイヤーフレームを描画
                if ir.source_format.is_pmx_pmd() {
                    let half_pi = std::f32::consts::FRAC_PI_2;
                    let half_seg = segments / 2;

                    // 上半球: 4本の半経線（赤道→北極）
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
                    // 上半球: 3本の緯線
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

                    // 下半球: 4本の半経線（赤道→南極）
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
                    // 下半球: 3本の緯線
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

                // 8本の接続線（上端→下端）
                for i in 0..8u32 {
                    let angle = std::f32::consts::FRAC_PI_4 * i as f32;
                    let local_offset = Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius);
                    let top = top_center + quat * local_offset;
                    let bot = bot_center + quat * local_offset;
                    draw_line_quad(out, top, bot, line_width * 0.5, color);
                }
            }
            RigidShape::Box { size } => {
                // ボックス: 12辺をライン描画
                // PMX仕様: sizeはhalf-extent（Bullet btBoxShapeと同じ）
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
                    (3, 0), // 手前面
                    (4, 5),
                    (5, 6),
                    (6, 7),
                    (7, 4), // 奥面
                    (0, 4),
                    (1, 5),
                    (2, 6),
                    (3, 7), // 接続
                ];
                for (a, b) in edges {
                    let pa = rb_pos + quat * corners[a];
                    let pb = rb_pos + quat * corners[b];
                    draw_line_quad(out, pa, pb, line_width * 0.5, color);
                }
            }
        }
    }

    // ジョイント接続線は generate_joint_vertices で描画するため、ここでは描画しない
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

/// 円弧ライン（LineList）: start_angle から end_angle まで描画
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

/// 法線表示用ジオメトリを生成（LineList: 頂点→先端の2頂点/法線）
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

    let color = [0.3, 0.6, 1.0, 0.9]; // 青系

    // アニメーション済み頂点があればそちらを使用
    let base_verts = model.current_vertices();
    let indices = model.base_indices();

    // 可視フラグバッファをリサイズ＆クリア
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

    // 同一位置・同一法線の重複を除去（位置+法線のビット表現でキー化）
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
    verts.push(GridVertex {
        position: from.to_array(),
        color,
    });
    verts.push(GridVertex {
        position: to.to_array(),
        color,
    });
}

/// ジョイント頂点を生成（オレンジ立方体面 + 黒1pxエッジ、回転反映、アニメーション同期）
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

    let orange = [1.0, 1.0, 0.0, opacity]; // イエロー #ffff00
    let black = [0.0, 0.0, 0.0, opacity.min(1.0)];
    let size = 0.18_f32;

    let is_pmx_pmd = ir.source_format.is_pmx_pmd();

    // bone.position はすべての形式で glTF 空間（PMX/PMD も pmx_pos_to_gltf 済み）
    let pos_fn = crate::convert::coord::pos_fn(is_vrm0);

    for joint in &ir.physics.joints {
        if joint.rigid_a >= ir.physics.rigid_bodies.len() {
            continue;
        }

        let rb_a = &ir.physics.rigid_bodies[joint.rigid_a];

        // ジョイント位置（PMX座標）
        // PMX/PMD: joint.position は既にPMX座標。VRM: glTF座標なので pos_fn で変換
        let joint_rest_pos = if is_pmx_pmd {
            joint.position
        } else {
            pos_fn(joint.position)
        };
        // ジョイント回転（YXZ intrinsic = ZXY extrinsic: R = Rz * Rx * Ry）
        let joint_rest_quat = glam::Quat::from_euler(
            glam::EulerRot::YXZ,
            joint.rotation.y,
            joint.rotation.x,
            joint.rotation.z,
        );

        // アニメーション適用: rigid_a のボーンからのオフセットで追従
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

        // 正立方体の8頂点（ローカル座標）
        let h = size * 0.5;
        let cube_verts = [
            Vec3::new(-h, -h, -h), // 0: 左下手前
            Vec3::new(h, -h, -h),  // 1: 右下手前
            Vec3::new(h, h, -h),   // 2: 右上手前
            Vec3::new(-h, h, -h),  // 3: 左上手前
            Vec3::new(-h, -h, h),  // 4: 左下奥
            Vec3::new(h, -h, h),   // 5: 右下奥
            Vec3::new(h, h, h),    // 6: 右上奥
            Vec3::new(-h, h, h),   // 7: 左上奥
        ];

        // 回転適用してワールド座標に変換
        let wv: [Vec3; 8] = cube_verts.map(|c| joint_pos + joint_quat * c);

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

        // 黒枠: 12本のエッジを1pxライン（LineList）で描画
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
