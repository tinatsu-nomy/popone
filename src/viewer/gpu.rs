use bytemuck::{Pod, Zeroable};
use eframe::{egui_wgpu, wgpu};
use glam::Vec3;
use wgpu::util::DeviceExt;

use super::camera::OrbitCamera;
use super::mesh::GpuModel;
use crate::intermediate::types::IrModel;

/// 材質用 BindGroupLayout を作成（共通定義、gpu.rs と mesh.rs で共有）
pub fn create_material_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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

/// カメラ uniform バッファ
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub light_dir: [f32; 3],
    pub light_intensity: f32,
    pub ambient: [f32; 3],
    pub show_normal_map: f32, // 0.0 = 通常, 1.0 = 法線マップ表示
    pub camera_pos: [f32; 3],
    pub mmd_edge_thickness: f32,
    pub view_row0: [f32; 3],
    pub _pad1: f32,
    pub view_row1: [f32; 3],
    pub mmd_ambient_scale: f32,
}

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
    pub _pad: [f32; 3],
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

/// WGSL 共通: CameraUniform 構造体定義（全シェーダーで共有）
macro_rules! wgsl_camera_uniform {
    () => {
        r#"struct CameraUniform {
    view_proj: mat4x4<f32>,
    light_dir: vec3<f32>,
    light_intensity: f32,
    ambient: vec3<f32>,
    show_normal_map: f32,
    camera_pos: vec3<f32>,
    mmd_edge_thickness: f32,
    view_row0: vec3<f32>,
    _pad1: f32,
    view_row1: vec3<f32>,
    mmd_ambient_scale: f32,
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
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};"#
    };
}

/// WGSL 共通: MaterialUniform 構造体定義（基本シェーダーで共有）
macro_rules! wgsl_material_uniform {
    () => {
        r#"struct MaterialUniform {
    diffuse: vec4<f32>,
};"#
    };
}

const SHADER_SRC: &str = concat!(
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
"#
);

/// MMD メインシェーダー共通部（頂点シェーダー + ライティング本体）
macro_rules! wgsl_mmd_main_body {
    () => {
        r#"
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
    let base_color = clamp(material.diffuse_rgb * vec3<f32>(camera.mmd_ambient_scale) + material.ambient, vec3<f32>(0.0), vec3<f32>(1.0));

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
    if out_a < 0.004 { discard; }

    // スペキュラ (最後に加算、トゥーンの影響を受けない)
    // LightSpecular = mmd_ambient_scale (≈0.604)
    let spec_color = material.specular * vec3<f32>(camera.mmd_ambient_scale);
    let eye_dir = normalize(camera.camera_pos - in.world_pos);
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
    @location(3) edge_scale: f32,
) -> EdgeVertexOutput {
    var out: EdgeVertexOutput;
    let dist = max(length(position - camera.camera_pos), 5.0);
    let offset = edge_scale * material.edge_size * camera.mmd_edge_thickness
                 * pow(dist, 0.7) * 0.003;
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
fn fs_edge() -> @location(0) vec4<f32> {
    // sRGBレンダーターゲットの自動エンコードを打ち消す
    let c = material.edge_color;
    return vec4<f32>(pow(max(c.rgb, vec3<f32>(0.0)), vec3<f32>(2.2)), c.a);
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
fn fs_mmd(in: MmdVertexOutput) -> @location(0) vec4<f32> {
    let result = compute_mmd_lighting(in);
    // sRGBレンダーターゲットの自動エンコードを打ち消す（MMDはガンマ空間で計算）
    let output = pow(max(result.rgb, vec3<f32>(0.0)), vec3<f32>(2.2));
    return vec4<f32>(output, result.a);
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
fn fs_edge() -> @location(0) vec4<f32> {
    // Unorm ターゲット: ガンマ空間値をそのまま出力（pow(2.2) 不要）
    return material.edge_color;
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
fn fs_mmd(in: MmdVertexOutput) -> @location(0) vec4<f32> {
    let result = compute_mmd_lighting(in);
    // Unorm ターゲット: ガンマ空間値をそのまま出力（pow(2.2) 不要）
    return vec4<f32>(clamp(result.rgb, vec3<f32>(0.0), vec3<f32>(1.0)), result.a);
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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}
"#
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
    // MMD パイプライン
    pipeline_mmd_main_cull: Option<wgpu::RenderPipeline>,
    pipeline_mmd_main_no_cull: Option<wgpu::RenderPipeline>,
    pipeline_mmd_alpha_cull: Option<wgpu::RenderPipeline>,
    pipeline_mmd_alpha_no_cull: Option<wgpu::RenderPipeline>,
    pipeline_mmd_edge: Option<wgpu::RenderPipeline>,
}

pub struct GpuRenderer {
    /// MSAA パイプラインセット (sample_count=4, sRGB)
    pipelines_msaa_srgb: PipelineSet,
    /// 非MSAA パイプラインセット (sample_count=1, sRGB)
    pipelines_no_msaa_srgb: PipelineSet,
    /// MSAA パイプラインセット (sample_count=4, Unorm)
    pipelines_msaa_unorm: PipelineSet,
    /// 非MSAA パイプラインセット (sample_count=1, Unorm)
    pipelines_no_msaa_unorm: PipelineSet,
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
    /// ボーンテールバッファ（LineList、テール三角形）
    bone_tail_buf: Option<wgpu::Buffer>,
    bone_tail_buf_capacity: usize,
    bone_tail_vertex_count: u32,
    /// ボーン塗りつぶしバッファ（TriangleList、マーカー塗り面）
    bone_fill_buf: Option<wgpu::Buffer>,
    bone_fill_buf_capacity: usize,
    bone_fill_vertex_count: u32,
    /// ボーン外枠バッファ（LineList、マーカー外枠線）
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
    // MMD リソース
    mmd_material_bgl: wgpu::BindGroupLayout,
    mmd_aux_bgl: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    shared_toon_textures: [wgpu::TextureView; 10],
    shared_toon_textures_unorm: [wgpu::TextureView; 10],
    shared_toon_sampler: wgpu::Sampler,
    default_mmd_aux_bind_group: wgpu::BindGroup,
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

        let material_bgl = create_material_bind_group_layout(device);

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
        let mmd_edge_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mmd_edge_shader"),
            source: wgpu::ShaderSource::Wgsl(MMD_EDGE_SHADER_SRC.into()),
        });
        let mmd_main_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
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
            log::warn!("POLYGON_MODE_LINE 非対応: ワイヤーフレーム無効");
        }

        // sRGB パイプラインセット（現行シェーダー: pow(2.2) 付き）
        let pipelines_msaa_srgb = Self::create_pipeline_set(
            device,
            &shader,
            &grid_shader,
            &wire_overlay_shader,
            &mmd_edge_shader,
            &mmd_main_shader,
            &pipeline_layout,
            &grid_pipeline_layout,
            &mmd_edge_pipeline_layout,
            &mmd_main_pipeline_layout,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            MSAA_SAMPLE_COUNT,
            supports_wireframe,
        );
        let pipelines_no_msaa_srgb = Self::create_pipeline_set(
            device,
            &shader,
            &grid_shader,
            &wire_overlay_shader,
            &mmd_edge_shader,
            &mmd_main_shader,
            &pipeline_layout,
            &grid_pipeline_layout,
            &mmd_edge_pipeline_layout,
            &mmd_main_pipeline_layout,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            1,
            supports_wireframe,
        );
        // Unorm パイプラインセット（pow(2.2) 除去 + linear_to_srgb）
        let pipelines_msaa_unorm = Self::create_pipeline_set(
            device,
            &shader,
            &grid_shader_unorm,
            &wire_overlay_shader,
            &mmd_edge_shader_unorm,
            &mmd_main_shader_unorm,
            &pipeline_layout,
            &grid_pipeline_layout,
            &mmd_edge_pipeline_layout,
            &mmd_main_pipeline_layout,
            wgpu::TextureFormat::Rgba8Unorm,
            MSAA_SAMPLE_COUNT,
            supports_wireframe,
        );
        let pipelines_no_msaa_unorm = Self::create_pipeline_set(
            device,
            &shader,
            &grid_shader_unorm,
            &wire_overlay_shader,
            &mmd_edge_shader_unorm,
            &mmd_main_shader_unorm,
            &pipeline_layout,
            &grid_pipeline_layout,
            &mmd_edge_pipeline_layout,
            &mmd_main_pipeline_layout,
            wgpu::TextureFormat::Rgba8Unorm,
            1,
            supports_wireframe,
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
            pipelines_msaa_srgb,
            pipelines_no_msaa_srgb,
            pipelines_msaa_unorm,
            pipelines_no_msaa_unorm,
            camera_buf,
            camera_bind_group,
            camera_bgl,
            texture_bgl,
            material_bgl,
            default_tex_bind_group,
            default_sampler,
            bone_tail_buf: None,
            bone_tail_buf_capacity: 0,
            bone_tail_vertex_count: 0,
            bone_fill_buf: None,
            bone_fill_buf_capacity: 0,
            bone_fill_vertex_count: 0,
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
            mmd_material_bgl,
            mmd_aux_bgl,
            shared_toon_textures,
            shared_toon_textures_unorm,
            shared_toon_sampler,
            default_mmd_aux_bind_group,
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
        self.bone_tail_vertex_count = 0;
        self.bone_fill_vertex_count = 0;
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
        mmd_edge_shader: &wgpu::ShaderModule,
        mmd_main_shader: &wgpu::ShaderModule,
        pipeline_layout: &wgpu::PipelineLayout,
        grid_pipeline_layout: &wgpu::PipelineLayout,
        mmd_edge_pipeline_layout: &wgpu::PipelineLayout,
        mmd_main_pipeline_layout: &wgpu::PipelineLayout,
        target_format: wgpu::TextureFormat,
        sample_count: u32,
        supports_wireframe: bool,
    ) -> PipelineSet {
        let ms = wgpu::MultisampleState {
            count: sample_count,
            ..Default::default()
        };

        let color_target = wgpu::ColorTargetState {
            format: target_format,
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

        let mmd_color_target = wgpu::ColorTargetState {
            format: target_format,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
                targets: &[Some(color_target.clone())],
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
                targets: &[Some(color_target.clone())],
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
                        targets: &[Some(color_target.clone())],
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
                        targets: &[Some(wire_color_target)],
                        compilation_options: Default::default(),
                    }),
                    multiview: None,
                    cache: None,
                }),
            )
        } else {
            None
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
                targets: &[Some(color_target.clone())],
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
                    targets: &[Some(color_target.clone())],
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
                    shader_location: 3,
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
                    targets: &[Some(mmd_color_target.clone())],
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
                    targets: &[Some(mmd_color_target.clone())],
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
                    targets: &[Some(mmd_color_target.clone())],
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
                    targets: &[Some(mmd_color_target.clone())],
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
                    targets: &[Some(mmd_color_target)],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            },
        ));

        PipelineSet {
            pipeline_cull,
            pipeline_no_cull,
            pipeline_wireframe,
            pipeline_wire_overlay,
            pipeline_alpha_cull,
            pipeline_alpha_no_cull,
            pipeline_grid,
            pipeline_bone,
            pipeline_line_overlay,
            pipeline_mmd_main_cull,
            pipeline_mmd_main_no_cull,
            pipeline_mmd_alpha_cull,
            pipeline_mmd_alpha_no_cull,
            pipeline_mmd_edge,
        }
    }

    /// ワイヤーフレーム対応かどうか
    pub fn supports_wireframe(&self) -> bool {
        self.pipelines_msaa_srgb.pipeline_wireframe.is_some()
    }

    /// 現在の MSAA 設定と Unorm フラグに応じたパイプラインセットを取得
    fn pipelines(&self, use_unorm: bool) -> &PipelineSet {
        match (self.current_msaa, use_unorm) {
            (true, false) => &self.pipelines_msaa_srgb,
            (true, true) => &self.pipelines_msaa_unorm,
            (false, false) => &self.pipelines_no_msaa_srgb,
            (false, true) => &self.pipelines_no_msaa_unorm,
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

        self.offscreen = Some(OffscreenTarget {
            _color: color,
            color_view,
            color_view_unorm,
            _msaa_color: msaa_tex,
            msaa_color_view: msaa_view,
            msaa_color_view_unorm: msaa_view_unorm,
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
                self.bone_tail_vertex_count = self.bone_tail_work.len() as u32;
                let tail_data = bytemuck::cast_slice(&self.bone_tail_work);
                if tail_data.len() > self.bone_tail_buf_capacity {
                    self.bone_tail_buf = Some(device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("bone_tail_vbuf"),
                            contents: tail_data,
                            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        },
                    ));
                    self.bone_tail_buf_capacity = tail_data.len();
                } else if let Some(ref buf) = self.bone_tail_buf {
                    queue.write_buffer(buf, 0, tail_data);
                }
                // 塗りバッファ（TriangleList）
                self.bone_fill_vertex_count = self.bone_fill_work.len() as u32;
                let fill_data = bytemuck::cast_slice(&self.bone_fill_work);
                if fill_data.len() > self.bone_fill_buf_capacity {
                    self.bone_fill_buf = Some(device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("bone_fill_vbuf"),
                            contents: fill_data,
                            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        },
                    ));
                    self.bone_fill_buf_capacity = fill_data.len();
                } else if let Some(ref buf) = self.bone_fill_buf {
                    queue.write_buffer(buf, 0, fill_data);
                }
                // 外枠バッファ（LineList）
                self.bone_vertex_count = self.bone_work.len() as u32;
                let line_data = bytemuck::cast_slice(&self.bone_work);
                if line_data.len() > self.bone_buf_capacity {
                    self.bone_buf = Some(device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("bone_vbuf"),
                            contents: line_data,
                            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        },
                    ));
                    self.bone_buf_capacity = line_data.len();
                } else if let Some(ref buf) = self.bone_buf {
                    queue.write_buffer(buf, 0, line_data);
                }
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
                self.normal_vertex_count = self.normal_work.len() as u32;
                let data = bytemuck::cast_slice(&self.normal_work);
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
                self.normal_cache_visibility.clear();
                self.normal_cache_visibility
                    .extend_from_slice(params.material_visibility);
            }
        } else {
            if self.normal_vertex_count > 0 {
                self.normal_dirty = true; // 再表示時に再生成するためフラグを立てる
            }
            self.normal_vertex_count = 0;
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
            self.spring_vertex_count = 0;
        }
        if need_spring {
            let spring_changed = self.spring_vertex_count == 0
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
        if need_joint {
            let joint_changed = self.joint_vertex_count == 0
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

        let offscreen = self
            .offscreen
            .as_ref()
            .expect("ensure_offscreen で初期化済み");

        // Update camera uniform
        let aspect = params.width as f32 / params.height as f32;
        let light_dir = match params.display.light_mode {
            LightMode::CameraFollow => params.camera.camera_following_light_dir(),
            LightMode::Fixed => OrbitCamera::fixed_light_dir(),
        };
        let view_mat = params.camera.view_matrix();
        let cam_uniform = CameraUniform {
            view_proj: params.camera.view_proj(aspect).to_cols_array_2d(),
            light_dir: light_dir.to_array(),
            light_intensity: params.display.light_intensity,
            ambient: [params.display.ambient_intensity; 3],
            show_normal_map: if params.display.show_normal_map {
                1.0
            } else {
                0.0
            },
            camera_pos: params.camera.eye().to_array(),
            mmd_edge_thickness: params.display.mmd_edge_thickness,
            view_row0: [view_mat.x_axis.x, view_mat.y_axis.x, view_mat.z_axis.x],
            _pad1: 0.0,
            view_row1: [view_mat.x_axis.y, view_mat.y_axis.y, view_mat.z_axis.y],
            mmd_ambient_scale: if params.display.mmd_mode {
                154.0 / 255.0
            } else {
                params.display.ambient_intensity
            },
        };
        queue.write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(&cam_uniform));

        // Encode
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("offscreen_encoder"),
        });

        let mmd_mode = params.display.mmd_mode;
        let mmd_edge_enabled = params.display.mmd_edge_enabled;
        // ワイヤーフレーム/法線マップ時は MMD パスを使わず既存パイプラインにフォールバック
        let mmd_solid = mmd_mode
            && params.display.draw_mode == DrawMode::Solid
            && !params.display.show_normal_map;

        // MMD 描画が必要かどうかを事前チェック
        let has_mmd_draws = mmd_solid
            && model.draws.iter().any(|d| {
                d.render_style == super::mesh::RenderStyle::Mmd
                    && d.mmd_material_bind_group.is_some()
            });

        // Unorm フレーム判定: MMD 専用パスに完全に乗るフレームのみ
        let use_unorm = can_use_unorm_frame(model, params.material_visibility, mmd_solid);

        let ps = self.pipelines(use_unorm);

        // カラービュー選択: use_unorm に応じて Unorm / sRGB ビュー
        let (color_view, resolve_target): (&wgpu::TextureView, Option<&wgpu::TextureView>) =
            if use_unorm {
                // Unorm ビューで描画
                if let Some(ref msaa_view_unorm) = offscreen.msaa_color_view_unorm {
                    (msaa_view_unorm, Some(&offscreen.color_view_unorm))
                } else {
                    (&offscreen.color_view_unorm, None)
                }
            } else {
                // sRGB ビューで描画（現行動作）
                if let Some(ref msaa_view) = offscreen.msaa_color_view {
                    (msaa_view, Some(&offscreen.color_view))
                } else {
                    (&offscreen.color_view, None)
                }
            };

        // クリアカラー補正: Unorm ターゲットに書く値は egui が sRGB デコードするため事前エンコード
        let bg = if use_unorm {
            linear_to_srgb_f64(params.display.bg_brightness as f64)
        } else {
            params.display.bg_brightness as f64
        };

        // ===== パス 1 (Clear): グリッド + Standard 不透明 + Standard 半透明 + Wire オーバーレイ =====
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(if use_unorm {
                    "pass1_unorm_clear"
                } else {
                    "pass1_srgb_clear"
                }),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    resolve_target: resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: bg,
                            g: bg,
                            b: bg,
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
                pass.set_pipeline(&ps.pipeline_grid);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.grid_vbuf.slice(..));
                pass.draw(0..self.grid_vertex_count, 0..1);
            }

            // メッシュ描画（空モデルの場合はスキップ）
            if !model.draws.is_empty() {
                pass.set_vertex_buffer(0, model.vertex_buf.slice(..));
                pass.set_index_buffer(model.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);

                let use_wireframe = params.display.draw_mode == DrawMode::Wireframe
                    && ps.pipeline_wireframe.is_some();
                let use_solid_wire = params.display.draw_mode == DrawMode::SolidWireframe
                    && ps.pipeline_wire_overlay.is_some();

                // Standard 不透明材質（デプス書き込みあり）
                for (draw_idx, draw) in model.draws.iter().enumerate() {
                    if !params
                        .material_visibility
                        .get(draw_idx)
                        .copied()
                        .unwrap_or(true)
                    {
                        continue;
                    }
                    if draw.is_alpha {
                        continue;
                    }
                    // MMD 材質は Pass 2 で描画するのでスキップ
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

                // Standard 半透明材質（デプス書き込みなし）
                for (draw_idx, draw) in model.draws.iter().enumerate() {
                    if !params
                        .material_visibility
                        .get(draw_idx)
                        .copied()
                        .unwrap_or(true)
                    {
                        continue;
                    }
                    if !draw.is_alpha {
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

                // Solid+Wire オーバーレイ
                if use_solid_wire {
                    let wire_pl = ps
                        .pipeline_wire_overlay
                        .as_ref()
                        .expect("wire_overlay パイプラインは supports_wireframe チェック済み");
                    pass.set_pipeline(wire_pl);
                    pass.set_bind_group(0, &self.camera_bind_group, &[]);
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

            // MMD 描画（材質インデックス順 — PMX の描画順序を維持）
            // Unorm 時はガンマ空間直接出力、sRGB 時は pow(2.2) で sRGB encode を打ち消す
            if has_mmd_draws && !model.draws.is_empty() {
                pass.set_vertex_buffer(0, model.vertex_buf.slice(..));
                pass.set_index_buffer(model.index_buf.slice(..), wgpu::IndexFormat::Uint32);

                let can_edge = mmd_edge_enabled
                    && model.edge_scale_buf.is_some()
                    && ps.pipeline_mmd_edge.is_some();

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

                    // 不透明 / 半透明でパイプラインを切り替え
                    if draw.is_alpha {
                        if draw.double_sided {
                            pass.set_pipeline(ps.pipeline_mmd_alpha_no_cull.as_ref().unwrap());
                        } else {
                            pass.set_pipeline(ps.pipeline_mmd_alpha_cull.as_ref().unwrap());
                        }
                    } else if draw.double_sided {
                        pass.set_pipeline(ps.pipeline_mmd_main_no_cull.as_ref().unwrap());
                    } else {
                        pass.set_pipeline(ps.pipeline_mmd_main_cull.as_ref().unwrap());
                    }
                    pass.set_bind_group(0, &self.camera_bind_group, &[]);
                    let tex_bg = draw
                        .mmd_texture_bind_group
                        .as_ref()
                        .or(draw.texture_bind_group.as_ref())
                        .unwrap_or(&self.default_tex_bind_group);
                    pass.set_bind_group(1, tex_bg, &[]);
                    pass.set_bind_group(2, draw.mmd_material_bind_group.as_ref().unwrap(), &[]);
                    let aux_bg = draw
                        .mmd_aux_bind_group
                        .as_ref()
                        .unwrap_or(&self.default_mmd_aux_bind_group);
                    pass.set_bind_group(3, aux_bg, &[]);
                    pass.draw_indexed(
                        draw.index_offset..(draw.index_offset + draw.index_count),
                        0,
                        0..1,
                    );

                    // 不透明材質のエッジをその場で描画
                    if can_edge && !draw.is_alpha && draw.has_edge {
                        if let Some(ref mmd_mat_bg) = draw.mmd_material_bind_group {
                            let edge_scale_buf = model.edge_scale_buf.as_ref().unwrap();
                            let edge_pipeline = ps.pipeline_mmd_edge.as_ref().unwrap();
                            pass.set_pipeline(edge_pipeline);
                            pass.set_vertex_buffer(0, model.vertex_buf.slice(..));
                            pass.set_vertex_buffer(1, edge_scale_buf.slice(..));
                            pass.set_index_buffer(
                                model.index_buf.slice(..),
                                wgpu::IndexFormat::Uint32,
                            );
                            pass.set_bind_group(0, &self.camera_bind_group, &[]);
                            pass.set_bind_group(1, mmd_mat_bg, &[]);
                            pass.draw_indexed(
                                draw.index_offset..(draw.index_offset + draw.index_count),
                                0,
                                0..1,
                            );
                            // エッジ描画後にメインバッファを復元
                            pass.set_vertex_buffer(0, model.vertex_buf.slice(..));
                            pass.set_index_buffer(
                                model.index_buf.slice(..),
                                wgpu::IndexFormat::Uint32,
                            );
                        }
                    }
                }
            }

            // 描画順: 法線 → ボーン → 剛体 → ジョイント（後が最前面）

            // 法線表示（LineList オーバーレイ）
            if params.display.show_normals && self.normal_vertex_count > 0 {
                if let Some(ref normal_buf) = self.normal_buf {
                    pass.set_pipeline(&ps.pipeline_line_overlay);
                    pass.set_bind_group(0, &self.camera_bind_group, &[]);
                    pass.set_vertex_buffer(0, normal_buf.slice(..));
                    pass.draw(0..self.normal_vertex_count, 0..1);
                }
            }

            // ボーン描画（3段階: テール → 塗り → 外枠）
            if params.display.show_bones {
                // 1. テール三角形（LineList）— 最背面
                if self.bone_tail_vertex_count > 0 {
                    if let Some(ref tail_buf) = self.bone_tail_buf {
                        pass.set_pipeline(&ps.pipeline_line_overlay);
                        pass.set_bind_group(0, &self.camera_bind_group, &[]);
                        pass.set_vertex_buffer(0, tail_buf.slice(..));
                        pass.draw(0..self.bone_tail_vertex_count, 0..1);
                    }
                }
                // 2. マーカー塗りつぶし（TriangleList）— テールの上
                if self.bone_fill_vertex_count > 0 {
                    if let Some(ref fill_buf) = self.bone_fill_buf {
                        pass.set_pipeline(&ps.pipeline_bone);
                        pass.set_bind_group(0, &self.camera_bind_group, &[]);
                        pass.set_vertex_buffer(0, fill_buf.slice(..));
                        pass.draw(0..self.bone_fill_vertex_count, 0..1);
                    }
                }
                // 3. マーカー外枠（LineList）— 最前面
                if self.bone_vertex_count > 0 {
                    if let Some(ref bone_buf) = self.bone_buf {
                        pass.set_pipeline(&ps.pipeline_line_overlay);
                        pass.set_bind_group(0, &self.camera_bind_group, &[]);
                        pass.set_vertex_buffer(0, bone_buf.slice(..));
                        pass.draw(0..self.bone_vertex_count, 0..1);
                    }
                }
            }

            // 剛体描画（1px LineList オーバーレイ）
            if params.display.show_spring_bones && self.spring_vertex_count > 0 {
                if let Some(ref spring_buf) = self.spring_buf {
                    pass.set_pipeline(&ps.pipeline_line_overlay);
                    pass.set_bind_group(0, &self.camera_bind_group, &[]);
                    pass.set_vertex_buffer(0, spring_buf.slice(..));
                    pass.draw(0..self.spring_vertex_count, 0..1);
                }
            }

            // ジョイント描画（オーバーレイ、最前面）
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
        } // end single render pass

        queue.submit(std::iter::once(encoder.finish()));

        // テクスチャ登録（初回は register、以降は update で再登録を回避）
        let tex_id = if let Some(existing_id) = *cached_id {
            egui_renderer.update_egui_texture_from_wgpu_texture(
                device,
                &offscreen.color_view,
                wgpu::FilterMode::Linear,
                existing_id,
            );
            existing_id
        } else {
            let id = egui_renderer.register_native_texture(
                device,
                &offscreen.color_view,
                wgpu::FilterMode::Linear,
            );
            *cached_id = Some(id);
            id
        };

        (tex_id, ())
    }

    /// MMD 用 GPU リソースを DrawCall に構築（全 GPU モデル生成経路から呼ぶ）
    pub fn prepare_mmd_resources(&self, device: &wgpu::Device, model: &mut GpuModel, ir: &IrModel) {
        use super::mesh::RenderStyle;

        // draws を一時的に取り出して借用衝突を回避
        let mut draws = std::mem::take(&mut model.draws);
        let gpu_textures_unorm = &model.gpu_texture_views_unorm;

        // MMD テクスチャ bind group 用サンプラー
        let tex_sampler = &self.default_sampler;

        for draw in &mut draws {
            if draw.render_style != RenderStyle::Mmd {
                continue;
            }

            let mat = &ir.materials[draw.material_index];

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

            let uniform = MmdMaterialUniform {
                ambient: mat.ambient.to_array(),
                alpha: mat.diffuse.w.clamp(0.0, 1.0),
                specular: mat.specular.to_array(),
                specular_power: mat.specular_power,
                diffuse_rgb: [mat.diffuse.x, mat.diffuse.y, mat.diffuse.z],
                flags,
                edge_color: mat.edge_color.to_array(),
                edge_size: mat.edge_size,
                _pad: [0.0; 3],
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
    // toon01: 白→灰 (境界 row16)
    #[rustfmt::skip]
    const TOON01: [[u8; 3]; 32] = [
        [255,255,255],[255,255,255],[255,255,255],[255,255,255],
        [255,255,255],[255,255,255],[255,255,255],[255,255,255],
        [255,255,255],[255,255,255],[255,255,255],[255,255,255],
        [255,255,255],[255,255,255],[255,255,255],[255,255,255],
        [205,205,205],[205,205,205],[205,205,205],[205,205,205],
        [205,205,205],[205,205,205],[205,205,205],[205,205,205],
        [205,205,205],[205,205,205],[205,205,205],[205,205,205],
        [205,205,205],[205,205,205],[205,205,205],[205,205,205],
    ];
    // toon02: 白→ピンク系 (境界 row16)
    #[rustfmt::skip]
    const TOON02: [[u8; 3]; 32] = [
        [255,255,255],[255,255,255],[255,255,255],[255,255,255],
        [255,255,255],[255,255,255],[255,255,255],[255,255,255],
        [255,255,255],[255,255,255],[255,255,255],[255,255,255],
        [255,255,255],[255,255,255],[255,255,255],[255,255,255],
        [245,225,225],[245,225,225],[245,225,225],[245,225,225],
        [245,225,225],[245,225,225],[245,225,225],[245,225,225],
        [245,225,225],[245,225,225],[245,225,225],[245,225,225],
        [245,225,225],[245,225,225],[245,225,225],[245,225,225],
    ];
    // toon03: 白→暗灰 (境界 row16)
    #[rustfmt::skip]
    const TOON03: [[u8; 3]; 32] = [
        [255,255,255],[255,255,255],[255,255,255],[255,255,255],
        [255,255,255],[255,255,255],[255,255,255],[255,255,255],
        [255,255,255],[255,255,255],[255,255,255],[255,255,255],
        [255,255,255],[255,255,255],[255,255,255],[255,255,255],
        [154,154,154],[154,154,154],[154,154,154],[154,154,154],
        [154,154,154],[154,154,154],[154,154,154],[154,154,154],
        [154,154,154],[154,154,154],[154,154,154],[154,154,154],
        [154,154,154],[154,154,154],[154,154,154],[154,154,154],
    ];
    // toon04: 白→暖色ベージュ (境界 row16)
    #[rustfmt::skip]
    const TOON04: [[u8; 3]; 32] = [
        [255,255,255],[255,255,255],[255,255,255],[255,255,255],
        [255,255,255],[255,255,255],[255,255,255],[255,255,255],
        [255,255,255],[255,255,255],[255,255,255],[255,255,255],
        [255,255,255],[255,255,255],[255,255,255],[255,255,255],
        [248,239,235],[248,239,235],[248,239,235],[248,239,235],
        [248,239,235],[248,239,235],[248,239,235],[248,239,235],
        [248,239,235],[248,239,235],[248,239,235],[248,239,235],
        [248,239,235],[248,239,235],[248,239,235],[248,239,235],
    ];
    // toon05: 白→暖ピンクのグラデーション
    #[rustfmt::skip]
    const TOON05: [[u8; 3]; 32] = [
        [255,255,255],[255,255,255],[255,255,255],[255,255,255],
        [255,255,255],[255,255,255],[255,255,255],[255,255,255],
        [255,255,255],[255,255,255],[255,255,255],[255,255,255],
        [255,255,255],[255,255,255],[255,255,255],[255,255,255],
        [255,255,255],[255,255,255],[255,255,255],[255,254,254],
        [255,250,248],[255,246,242],[255,240,234],[255,236,229],
        [255,233,224],[255,231,222],[255,231,221],[255,231,221],
        [255,231,221],[255,231,221],[255,231,222],[254,232,223],
    ];
    // toon06: 黄色系 (中央ハイライトバンド + 暗黄)
    #[rustfmt::skip]
    const TOON06: [[u8; 3]; 32] = [
        [255,237, 97],[255,237, 97],[255,237, 97],[255,237, 97],
        [255,237, 97],[255,237, 97],[255,237, 97],[255,237, 97],
        [255,238,106],[255,246,175],[255,254,242],[255,242,138],
        [255,237, 97],[255,237, 97],[255,237, 97],[255,237, 97],
        [255,237, 97],[255,237, 97],[255,237, 97],[255,237, 97],
        [255,237, 97],[255,237, 97],[254,235, 94],[238,218, 69],
        [209,187, 24],[197,174,  6],[195,172,  3],[195,172,  3],
        [195,172,  3],[195,172,  3],[195,172,  3],[195,172,  3],
    ];
    // toon07-10: 全白 (トゥーン効果なし)
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
    let outer_factor = 0.004_f32;
    let inner_factor = 0.0022_f32;
    let ik_center_factor = inner_factor; // IKコントローラ中心の青正方形（移動ボーンと同サイズ）
    let segments = 16u32;

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
    let pos_fn: fn(Vec3) -> Vec3 = if is_vrm0 {
        crate::convert::coord::gltf_pos_to_pmx_v0
    } else {
        crate::convert::coord::gltf_pos_to_pmx
    };
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

    let segments = 16u32;
    let line_width = 0.0_f32; // 1px描画（draw_ring/draw_line_quad の _width 引数用）

    // bone.position はすべての形式で glTF 空間に格納されている（PMX/PMD も pmx_pos_to_gltf 済み）
    // rb.position は PMX 空間なので、bone 側を PMX 空間に戻して差分を取る
    let pos_fn: fn(Vec3) -> Vec3 = if is_vrm0 {
        crate::convert::coord::gltf_pos_to_pmx_v0
    } else {
        // PMX/PMD も VRM 1.0 と同じ Z-flip 変換（pmx_pos_to_gltf の逆）
        crate::convert::coord::gltf_pos_to_pmx
    };

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
    let pos_fn: fn(Vec3) -> Vec3 = if is_vrm0 {
        crate::convert::coord::gltf_pos_to_pmx_v0
    } else {
        // PMX/PMD も VRM 1.0 と同じ Z-flip 変換
        crate::convert::coord::gltf_pos_to_pmx
    };

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
