use crate::error::Result;
use glam::{Mat3, Mat4, Vec2, Vec3, Vec4};
use gltf::buffer::Data;
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};

use crate::convert::coord::PMX_SCALE;
use crate::intermediate::types::*;

/// sRGB ガンマ空間の単一チャンネル値をリニア空間に変換する
fn srgb_to_linear_channel(x: f32) -> f32 {
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}

/// sRGB Vec3 (RGB) をリニア空間に変換する
fn srgb_vec3_to_linear(v: Vec3) -> Vec3 {
    Vec3::new(
        srgb_to_linear_channel(v.x),
        srgb_to_linear_channel(v.y),
        srgb_to_linear_channel(v.z),
    )
}

/// sRGB Vec4 の RGB 成分のみリニア空間に変換する（アルファはそのまま）
fn srgb_vec4_rgb_to_linear(v: Vec4) -> Vec4 {
    Vec4::new(
        srgb_to_linear_channel(v.x),
        srgb_to_linear_channel(v.y),
        srgb_to_linear_channel(v.z),
        v.w,
    )
}

/// ボーン抽出結果: (ボーン配列, ノード→ボーンindex マップ, グローバル行列配列)
type BoneExtractResult = (Vec<IrBone>, HashMap<usize, usize>, Vec<Mat4>);
use crate::vrm::{
    detect::VrmVersion,
    types_v0::VrmV0,
    types_v1::{SpringBoneV1, VrmV1},
};

/// VRM拡張JSONを1回だけデシリアライズした結果を保持する列挙型
enum VrmTyped {
    V0(VrmV0),
    V1(VrmV1),
    /// VRM 拡張なしの plain GLB
    Unknown,
}

/// MToon テクスチャ情報を JSON オブジェクトから読み取る（texCoord + KHR_texture_transform 対応）
/// glTF の texture index を image index に正規化して格納する
fn read_texture_info(obj: &Value, document: &gltf::Document) -> Option<IrTextureInfo> {
    let texture_index = obj.get("index")?.as_u64()? as usize;
    // glTF texture index → image index に解決
    let image_index = document.textures().nth(texture_index)?.source().index();
    let base_tex_coord = obj.get("texCoord").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let (tex_coord, offset, scale, rotation) = if let Some(ext) = obj
        .get("extensions")
        .and_then(|e| e.get("KHR_texture_transform"))
    {
        // KHR_texture_transform.texCoord は TextureInfo 本体の texCoord を override する
        let tex_coord = ext
            .get("texCoord")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(base_tex_coord);
        let offset = ext
            .get("offset")
            .and_then(|v| v.as_array())
            .map(|a| {
                Vec2::new(
                    a.first().and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                )
            })
            .unwrap_or(Vec2::ZERO);
        let scale = ext
            .get("scale")
            .and_then(|v| v.as_array())
            .map(|a| {
                Vec2::new(
                    a.first().and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                    a.get(1).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                )
            })
            .unwrap_or(Vec2::ONE);
        let rotation = ext.get("rotation").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        (tex_coord, offset, scale, rotation)
    } else {
        (base_tex_coord, Vec2::ZERO, Vec2::ONE, 0.0)
    };
    // texCoord >= 2 は非対応 → texCoord=0 にフォールバック（graceful degradation）:
    // - VRM 1.0 / MToon 仕様で使用する UV セットは TEXCOORD_0 と TEXCOORD_1 の2系統のみ
    // - UniVRM の MToon 実装（vrmc_materials_mtoon_geometry_uv.hlsl）でも UV0/UV1 しか使わない
    // - glTF 仕様では任意数の UV セットを許容するが、VRM モデルで TEXCOORD_2+ を使うケースは
    //   実質存在しない
    // - テクスチャを消失させるよりも texCoord=0 で描画を維持する方が被害が小さい
    let tex_coord = if tex_coord > 1 {
        log::warn!(
            "texCoord={} は非対応のため texCoord=0 にフォールバックします (texture index={})",
            tex_coord,
            texture_index,
        );
        0
    } else {
        tex_coord
    };
    // glTF sampler 情報を読み取り
    let sampler_info = document
        .textures()
        .nth(texture_index)
        .map(|tex| {
            let s = tex.sampler();
            let wrap_u = match s.wrap_s() {
                gltf::texture::WrappingMode::ClampToEdge => IrWrapMode::ClampToEdge,
                gltf::texture::WrappingMode::MirroredRepeat => IrWrapMode::MirroredRepeat,
                gltf::texture::WrappingMode::Repeat => IrWrapMode::Repeat,
            };
            let wrap_v = match s.wrap_t() {
                gltf::texture::WrappingMode::ClampToEdge => IrWrapMode::ClampToEdge,
                gltf::texture::WrappingMode::MirroredRepeat => IrWrapMode::MirroredRepeat,
                gltf::texture::WrappingMode::Repeat => IrWrapMode::Repeat,
            };
            let mag_filter = match s.mag_filter() {
                Some(gltf::texture::MagFilter::Nearest) => IrMagFilter::Nearest,
                _ => IrMagFilter::Linear, // デフォルトは Linear
            };
            let min_filter = match s.min_filter() {
                Some(gltf::texture::MinFilter::Nearest) => IrMinFilter::Nearest,
                Some(gltf::texture::MinFilter::Linear) => IrMinFilter::Linear,
                Some(gltf::texture::MinFilter::NearestMipmapNearest) => {
                    IrMinFilter::NearestMipmapNearest
                }
                Some(gltf::texture::MinFilter::LinearMipmapNearest) => {
                    IrMinFilter::LinearMipmapNearest
                }
                Some(gltf::texture::MinFilter::NearestMipmapLinear) => {
                    IrMinFilter::NearestMipmapLinear
                }
                Some(gltf::texture::MinFilter::LinearMipmapLinear) | None => {
                    IrMinFilter::LinearMipmapLinear // デフォルトは LinearMipmapLinear
                }
            };
            IrSamplerInfo {
                wrap_u,
                wrap_v,
                mag_filter,
                min_filter,
            }
        })
        .unwrap_or_default();

    Some(IrTextureInfo {
        index: image_index,
        tex_coord,
        offset,
        scale,
        rotation,
        sampler: sampler_info,
    })
}

pub fn extract_ir_model(
    document: &gltf::Document,
    buffers: &[Data],
    images: &[gltf::image::Data],
    vrm_ext: &Value,
    version: &VrmVersion,
    all_extensions: &Value,
) -> Result<IrModel> {
    extract_ir_model_with_options(
        document,
        buffers,
        images,
        vrm_ext,
        version,
        all_extensions,
        false,
    )
}

pub fn extract_ir_model_with_options(
    document: &gltf::Document,
    buffers: &[Data],
    images: &[gltf::image::Data],
    vrm_ext: &Value,
    version: &VrmVersion,
    all_extensions: &Value,
    normalize_pose: bool,
) -> Result<IrModel> {
    let mut model = IrModel::default();
    model.source_format = if matches!(version, VrmVersion::V0) {
        SourceFormat::Vrm0
    } else {
        SourceFormat::Vrm1
    };

    // VRM拡張JSONを1回だけデシリアライズ（以後 typed を参照渡しで使い回す）
    let typed = match version {
        VrmVersion::V0 => {
            let v0: VrmV0 = serde_json::from_value(vrm_ext.clone()).unwrap_or_else(|e| {
                log::warn!("VrmV0 デシリアライズエラー: {}", e);
                VrmV0::default()
            });
            VrmTyped::V0(v0)
        }
        VrmVersion::V1 => {
            let v1: VrmV1 = serde_json::from_value(vrm_ext.clone()).unwrap_or_else(|e| {
                log::warn!("VrmV1 デシリアライズエラー: {}", e);
                VrmV1::default()
            });
            VrmTyped::V1(v1)
        }
        VrmVersion::Unknown => VrmTyped::Unknown,
    };

    // テクスチャ抽出
    model.textures = extract_textures(document, images)?;

    // 材質抽出
    model.materials = extract_materials(document, &typed, version, &model.textures)?;

    // ボーン抽出（ノード→ボーン構造）
    let (bones, node_to_bone, mut global_mats) = extract_bones(document, &typed)?;
    model.bones = bones;
    model.node_to_bone = node_to_bone;

    // T→Aスタンス変換（オプション）
    if normalize_pose {
        model.astance_result = crate::intermediate::pose::normalize_pose_to_astance(
            &mut model.bones,
            &mut global_mats,
        );
    }

    // モデル名・コメント
    model.name = extract_model_name(&typed);
    model.comment = extract_meta_comment(&typed);

    // メッシュ抽出（補正済み global_mats を使用）
    model.meshes = extract_meshes(
        document,
        buffers,
        images,
        &model.node_to_bone,
        &model.materials,
        &global_mats,
    )?;

    // モーフ抽出
    model.morphs = extract_morphs(document, &typed, &model.meshes, &model.node_to_bone)?;

    // 物理抽出
    model.physics = extract_physics(&typed, all_extensions, &model.node_to_bone, &model.bones)?;

    // 物理演算ボーン（physics_mode=1）に is_physics フラグを立てる
    // → build_bones() で BONE_FLAG_PHYS_AFTER に変換される
    for rb in &model.physics.rigid_bodies {
        if rb.physics_mode == 1 {
            if let Some(bi) = rb.bone_index {
                if bi < model.bones.len() {
                    model.bones[bi].is_physics = true;
                }
            }
        }
    }

    // VRMは常にヒューマノイド
    model.humanoid_bone_count = model
        .bones
        .iter()
        .filter(|b| b.vrm_bone_name.is_some())
        .count();

    Ok(model)
}

fn extract_model_name(typed: &VrmTyped) -> String {
    match typed {
        VrmTyped::V1(v1) => {
            if let Some(meta) = &v1.meta {
                if let Some(name) = &meta.name {
                    return name.clone();
                }
            }
        }
        VrmTyped::V0(v0) => {
            if let Some(meta) = &v0.meta {
                if let Some(title) = &meta.title {
                    return title.clone();
                }
            }
        }
        VrmTyped::Unknown => {}
    }
    "Unknown".to_string()
}

fn extract_meta_comment(typed: &VrmTyped) -> String {
    let mut lines: Vec<String> = Vec::new();

    macro_rules! section {
        ($title:expr) => {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.push(format!("[{}]", $title));
        };
    }
    macro_rules! field {
        ($label:expr, $val:expr) => {
            if let Some(v) = $val {
                lines.push(format!("  {:<36}: {}", $label, v));
            }
        };
        ($label:expr, bool $val:expr) => {
            if let Some(v) = $val {
                lines.push(format!("  {:<36}: {}", $label, v));
            }
        };
        ($label:expr, vec $val:expr) => {
            if !$val.is_empty() {
                lines.push(format!("  {:<36}: {}", $label, $val.join(", ")));
            }
        };
    }

    match typed {
        VrmTyped::V0(v0) => {
            if let Some(m) = &v0.meta {
                section!("Model Info");
                field!("model name", m.title.as_deref());
                field!("version", m.version.as_deref());

                section!("Author");
                field!("author", m.author.as_deref());
                field!("contact information", m.contact_information.as_deref());
                field!("reference", m.reference.as_deref());

                section!("Permissions");
                field!("allowed user", m.allowed_user_name.as_deref());
                field!("violent ussage", m.violent_ussage_name.as_deref());
                field!("sexual ussage", m.sexual_ussage_name.as_deref());
                field!("commercial ussage", m.commercial_ussage_name.as_deref());
                field!("other permission", m.other_permission_url.as_deref());

                section!("License");
                field!("license", m.license_name.as_deref());
                field!("other license", m.other_license_url.as_deref());
            }
        }
        VrmTyped::V1(v1) => {
            if let Some(m) = &v1.meta {
                section!("Model Info");
                field!("model name", m.name.as_deref());
                field!("version", m.version.as_deref());

                section!("Author");
                field!("author", vec m.authors);
                field!("copyright information", m.copyright_information.as_deref());
                field!("contact information", m.contact_information.as_deref());
                field!("reference", vec m.references);
                field!("third party licenses", m.third_party_licenses.as_deref());

                section!("License");
                field!("license", m.license_url.as_deref());
                field!("other license", m.other_license_url.as_deref());

                section!("Permissions");
                field!("avatar permission", m.avatar_permission.as_deref());
                field!("allow excessively violent usage", bool m.allow_excessively_violent_usage.map(|v| v.to_string()));
                field!("allow excessively sexual usage", bool m.allow_excessively_sexual_usage.map(|v| v.to_string()));
                field!("commercial usage", m.commercial_usage.as_deref());
                field!("allow political or religious usage", bool m.allow_political_or_religious_usage.map(|v| v.to_string()));
                field!("allow antisocial or hate usage", bool m.allow_antisocial_or_hate_usage.map(|v| v.to_string()));
                field!("credit notation", m.credit_notation.as_deref());
                field!("allow redistribution", bool m.allow_redistribution.map(|v| v.to_string()));
                field!("modification", m.modification.as_deref());
            }
        }
        VrmTyped::Unknown => {}
    }

    let comment = lines.join("\r\n");
    log::info!("=== VRM Meta ===\n{}", comment.replace("\r\n", "\n"));
    comment
}

fn extract_textures(
    document: &gltf::Document,
    images: &[gltf::image::Data],
) -> Result<Vec<IrTexture>> {
    let mut textures = Vec::with_capacity(images.len());
    for (i, image_data) in images.iter().enumerate() {
        let filename = format!("tex_{:03}.png", i);
        let mime_type = "image/png".to_string();
        textures.push(IrTexture {
            filename,
            data: image_data.pixels.clone(),
            mime_type,
        });
    }

    // テクスチャ名をgltfのイメージ名で上書き
    for (i, image) in document.images().enumerate() {
        if i < textures.len() {
            if let Some(name) = image.name() {
                textures[i].filename = format!("{}.png", sanitize_filename(name));
            }
        }
    }

    Ok(textures)
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[allow(clippy::field_reassign_with_default)]
fn extract_materials(
    document: &gltf::Document,
    typed: &VrmTyped,
    version: &VrmVersion,
    _textures: &[IrTexture],
) -> Result<Vec<IrMaterial>> {
    let mut materials = Vec::new();

    // VRM 0.0 のmaterialPropertiesを優先使用
    let v0_mat_props: &[crate::vrm::types_v0::VrmMaterialProperty] = match typed {
        VrmTyped::V0(v0) => &v0.material_properties,
        VrmTyped::V1(_) | VrmTyped::Unknown => &[],
    };

    for (i, mat) in document.materials().enumerate() {
        let mut ir_mat = IrMaterial::default();
        ir_mat.name = mat.name().unwrap_or(&format!("material_{}", i)).to_string();

        let pbr = mat.pbr_metallic_roughness();

        // ベースカラー
        let bc = pbr.base_color_factor();
        ir_mat.diffuse = Vec4::new(bc[0], bc[1], bc[2], bc[3]);

        // ベーステクスチャ
        if let Some(tex_info) = pbr.base_color_texture() {
            let src_idx = tex_info.texture().source().index();
            ir_mat.texture_index = Some(src_idx);
            // VRM埋め込みテクスチャの名前を source_texture_name に設定
            let img_name = tex_info
                .texture()
                .source()
                .name()
                .map(|n| n.to_string())
                .or_else(|| _textures.get(src_idx).map(|t| t.filename.clone()));
            ir_mat.source_texture_name = img_name;
        }

        ir_mat.cull_mode = if mat.double_sided() {
            CullMode::None
        } else {
            CullMode::Back
        };

        // glTF alphaMode / alphaCutoff
        ir_mat.alpha_mode = match mat.alpha_mode() {
            gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
            gltf::material::AlphaMode::Mask => AlphaMode::Mask,
            gltf::material::AlphaMode::Blend => AlphaMode::Blend,
        };
        if let Some(cutoff) = mat.alpha_cutoff() {
            ir_mat.alpha_cutoff = cutoff;
        }

        // glTF emissiveFactor / emissiveTexture
        let ef = mat.emissive_factor();
        ir_mat.emissive_factor = Vec3::new(ef[0], ef[1], ef[2]);
        if let Some(et) = mat.emissive_texture() {
            ir_mat.emissive_texture =
                Some(IrTextureInfo::from_index(et.texture().source().index()));
        }

        // glTF normalTexture
        if let Some(nt) = mat.normal_texture() {
            ir_mat.normal_texture = Some(IrTextureInfo::from_index(nt.texture().source().index()));
            ir_mat.normal_texture_scale = nt.scale();
        }

        // emissiveTexture / normalTexture の texCoord + KHR_texture_transform 抽出（raw JSON 経由）
        // read_texture_info が None（texCoord >= 2 等）の場合は core API の仮設定もクリアする
        {
            let json = document.as_json();
            if let Some(mat_json) = json.materials.get(i) {
                if let Some(ref et) = mat_json.emissive_texture {
                    ir_mat.emissive_texture = serde_json::to_value(et)
                        .ok()
                        .and_then(|val| read_texture_info(&val, document));
                }
                if let Some(ref nt) = mat_json.normal_texture {
                    match serde_json::to_value(nt)
                        .ok()
                        .and_then(|val| read_texture_info(&val, document))
                    {
                        Some(ti) => {
                            ir_mat.normal_texture_scale = nt.scale;
                            ir_mat.normal_texture = Some(ti);
                        }
                        None => {
                            ir_mat.normal_texture = None;
                        }
                    }
                }
                // KHR_materials_emissive_strength: HDR emissive 倍率
                // UniVRM は maxComponent > 1.0 のとき emissiveStrength を書き出す
                if let Some(strength) = mat_json
                    .extensions
                    .as_ref()
                    .and_then(|exts| exts.others.get("KHR_materials_emissive_strength"))
                    .and_then(|v| v.get("emissiveStrength"))
                    .and_then(|v| v.as_f64())
                {
                    ir_mat.emissive_factor *= strength as f32;
                }
            }
        }

        // VRM 0.x _MainTex ST（resolve_tex + base_color_tex_info に伝播用）
        let mut main_tex_st: Option<(Vec2, Vec2)> = None;
        // VRM 0.x MToon で _MainTex が解決済みかどうか（後段の raw JSON 上書き抑制用）
        let mut v0_main_tex_resolved = false;

        // VRM 0.0 マテリアルプロパティ
        if let Some(v0_prop) = v0_mat_props.get(i) {
            let v0_is_mtoon = v0_prop.shader.contains("MToon");

            if v0_is_mtoon {
                ir_mat.mtoon = Some(MtoonParams::default());
                let mtoon = ir_mat.mtoon.as_mut().unwrap();

                // _OutlineWidthMode: 0=None, 1=WorldCoordinates, 2=ScreenCoordinates
                let outline_mode = v0_prop
                    .float_properties
                    .as_ref()
                    .and_then(|fp| fp.get("_OutlineWidthMode"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as i32;

                // OutlineWidthMode を保存（ビューア描画用）
                mtoon.outline_width_mode = match outline_mode {
                    1 => OutlineWidthMode::WorldCoordinates,
                    2 => OutlineWidthMode::ScreenCoordinates,
                    _ => OutlineWidthMode::None,
                };

                if outline_mode != 0 {
                    if let Some(vec_props) = &v0_prop.vector_properties {
                        if let Some(outline_color) = vec_props.get("_OutlineColor") {
                            if let Some(arr) = outline_color.as_array() {
                                let r = arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                                let g = arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                                let b = arr.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                                let a = arr.get(3).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                                ir_mat.edge_color = srgb_vec4_rgb_to_linear(Vec4::new(r, g, b, a));
                            }
                        }
                    }
                    if let Some(float_props) = &v0_prop.float_properties {
                        if let Some(width) = float_props.get("_OutlineWidth") {
                            let w = width.as_f64().unwrap_or(0.0) as f32;
                            // UniVRM MigrationMToonMaterial.cs 準拠:
                            // WorldCoordinates: w * 0.01 (cm→m)
                            // ScreenCoordinates: w * 0.01 * 0.5 (旧:縦半分の%値 → 新:縦全体の比率)
                            mtoon.outline_width_factor = match outline_mode {
                                1 => w * 0.01,       // WorldCoordinates: メートル
                                2 => w * 0.01 * 0.5, // ScreenCoordinates: 1/200 換算
                                _ => 0.0,
                            };
                            ir_mat.edge_size = match outline_mode {
                                1 => mtoon.outline_width_factor * PMX_SCALE * 10.0,
                                2 => mtoon.outline_width_factor * 100.0,
                                _ => 0.0,
                            };
                        }
                    }
                    // _OutlineWidthTexture は _MainTex ST 取得後に resolve_tex() で処理する
                    // （ここでは設定しない: main_tex_st 伝播のため後段で統一処理）
                    // _OutlineLightingMix
                    if let Some(float_props) = &v0_prop.float_properties {
                        mtoon.outline_lighting_mix = float_props
                            .get("_OutlineLightingMix")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(1.0)
                            as f32;
                    }
                }

                log::debug!("材質[{}] \"{}\" is_mtoon=true, outline_mode={}, edge_size={:.3}, edge_color=({:.2},{:.2},{:.2},{:.2})",
                    i, ir_mat.name, outline_mode, ir_mat.edge_size,
                    ir_mat.edge_color.x, ir_mat.edge_color.y, ir_mat.edge_color.z, ir_mat.edge_color.w);

                // --- VRM 0.x → 1.0 正規化（UniVRM MigrationMToonMaterial.cs 準拠）---

                // ヘルパー: float プロパティ取得
                let get_float = |key: &str, default: f64| -> f32 {
                    v0_prop
                        .float_properties
                        .as_ref()
                        .and_then(|fp| fp.get(key))
                        .and_then(|v| v.as_f64())
                        .unwrap_or(default) as f32
                };

                // ヘルパー: vec3 カラー取得
                let get_color3 = |key: &str, dr: f64, dg: f64, db: f64| -> Vec3 {
                    v0_prop
                        .vector_properties
                        .as_ref()
                        .and_then(|vp| vp.get(key))
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            Vec3::new(
                                arr.first().and_then(|v| v.as_f64()).unwrap_or(dr) as f32,
                                arr.get(1).and_then(|v| v.as_f64()).unwrap_or(dg) as f32,
                                arr.get(2).and_then(|v| v.as_f64()).unwrap_or(db) as f32,
                            )
                        })
                        .unwrap_or(Vec3::new(dr as f32, dg as f32, db as f32))
                };

                // _MainTex ST 取得（UniVRM 準拠: 全MToonテクスチャに伝播）
                // VRM 0.x vectorProperties 格納順: [offsetX, offsetY, scaleX, scaleY]
                // Unity ST → glTF KHR_texture_transform 変換:
                //   offset.y = 1.0 - unityOffset.y - unityScale.y
                // (Vrm10MaterialExportUtils.ExportTextureTransform 準拠)
                main_tex_st = v0_prop
                    .vector_properties
                    .as_ref()
                    .and_then(|vp| vp.get("_MainTex"))
                    .and_then(|v| v.as_array())
                    .and_then(|arr| {
                        let unity_offset_x =
                            arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                        let unity_offset_y =
                            arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                        let scale = Vec2::new(
                            arr.get(2).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                            arr.get(3).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                        );
                        let offset = Vec2::new(unity_offset_x, 1.0 - unity_offset_y - scale.y);
                        // identity transform (scale=1, offset=0) ならスキップ
                        let is_identity =
                            (scale - Vec2::ONE).length() < 1e-6 && offset.length() < 1e-6;
                        if is_identity {
                            None
                        } else {
                            Some((scale, offset))
                        }
                    });

                // ヘルパー: テクスチャプロパティ → IrTextureInfo
                // inherit_st=true の場合、_MainTex ST を適用する（MatCap は除外）
                let resolve_tex = |key: &str, inherit_st: bool| -> Option<IrTextureInfo> {
                    v0_prop
                        .texture_properties
                        .as_ref()
                        .and_then(|tp| tp.get(key))
                        .and_then(|v| v.as_u64())
                        .and_then(|idx| {
                            document.textures().nth(idx as usize).map(|t| {
                                let mut ti = IrTextureInfo::from_index(t.source().index());
                                // glTF sampler 情報を反映
                                let s = t.sampler();
                                ti.sampler = IrSamplerInfo {
                                    wrap_u: match s.wrap_s() {
                                        gltf::texture::WrappingMode::ClampToEdge => {
                                            IrWrapMode::ClampToEdge
                                        }
                                        gltf::texture::WrappingMode::MirroredRepeat => {
                                            IrWrapMode::MirroredRepeat
                                        }
                                        gltf::texture::WrappingMode::Repeat => IrWrapMode::Repeat,
                                    },
                                    wrap_v: match s.wrap_t() {
                                        gltf::texture::WrappingMode::ClampToEdge => {
                                            IrWrapMode::ClampToEdge
                                        }
                                        gltf::texture::WrappingMode::MirroredRepeat => {
                                            IrWrapMode::MirroredRepeat
                                        }
                                        gltf::texture::WrappingMode::Repeat => IrWrapMode::Repeat,
                                    },
                                    mag_filter: match s.mag_filter() {
                                        Some(gltf::texture::MagFilter::Nearest) => {
                                            IrMagFilter::Nearest
                                        }
                                        _ => IrMagFilter::Linear,
                                    },
                                    min_filter: match s.min_filter() {
                                        Some(gltf::texture::MinFilter::Nearest) => {
                                            IrMinFilter::Nearest
                                        }
                                        Some(gltf::texture::MinFilter::Linear) => {
                                            IrMinFilter::Linear
                                        }
                                        Some(gltf::texture::MinFilter::NearestMipmapNearest) => {
                                            IrMinFilter::NearestMipmapNearest
                                        }
                                        Some(gltf::texture::MinFilter::LinearMipmapNearest) => {
                                            IrMinFilter::LinearMipmapNearest
                                        }
                                        Some(gltf::texture::MinFilter::NearestMipmapLinear) => {
                                            IrMinFilter::NearestMipmapLinear
                                        }
                                        Some(gltf::texture::MinFilter::LinearMipmapLinear)
                                        | None => IrMinFilter::LinearMipmapLinear,
                                    },
                                };
                                if inherit_st {
                                    if let Some((scale, offset)) = &main_tex_st {
                                        ti.scale = *scale;
                                        ti.offset = *offset;
                                    }
                                }
                                ti
                            })
                        })
                };

                // _Color / _MainTex → lit色/テクスチャ正規化（UniVRM MigrationMToonMaterial.cs:148-164 準拠）
                // glTF core の baseColorFactor/baseColorTexture は近似値の場合があるため、
                // materialProperties 側を優先する
                if let Some(color) = v0_prop
                    .vector_properties
                    .as_ref()
                    .and_then(|vp| vp.get("_Color"))
                    .and_then(|v| v.as_array())
                {
                    let r = color.first().and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                    let g = color.get(1).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                    let b = color.get(2).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                    let a = color.get(3).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                    ir_mat.diffuse = srgb_vec4_rgb_to_linear(Vec4::new(r, g, b, a));
                }
                if let Some(base_tex) = resolve_tex("_MainTex", true) {
                    ir_mat.texture_index = Some(base_tex.index);
                    // _MainTex を authoritative source にしたので source_texture_name も同期
                    ir_mat.source_texture_name = document
                        .images()
                        .nth(base_tex.index)
                        .and_then(|img| img.name().map(|s| s.to_string()))
                        .or_else(|| _textures.get(base_tex.index).map(|t| t.filename.clone()));
                    ir_mat.base_color_tex_info = Some(base_tex);
                    v0_main_tex_resolved = true;
                }

                // _BlendMode: 0=Opaque, 1=Cutout, 2=Transparent, 3=TransparentWithZWrite
                let blend_mode = get_float("_BlendMode", 0.0) as i32;
                ir_mat.alpha_mode = match blend_mode {
                    0 => AlphaMode::Opaque,
                    1 => {
                        ir_mat.alpha_cutoff = get_float("_Cutoff", 0.5);
                        AlphaMode::Mask
                    }
                    2 => AlphaMode::Blend,
                    3 => AlphaMode::BlendWithZWrite,
                    _ => AlphaMode::Opaque,
                };

                // _CullMode: 0=Off(両面), 1=Front(前面カリング), 2=Back(片面)
                // UniVRM では Front→doubleSided=true にフォールバックするが、
                // ランタイムレンダラでは Front cull を再現可能
                let cull_mode_val = get_float("_CullMode", 2.0) as i32;
                ir_mat.cull_mode = match cull_mode_val {
                    0 => CullMode::None,
                    1 => CullMode::Front,
                    _ => CullMode::Back,
                };

                // 再取得（ir_mat のフィールドアクセス後に mtoon を再借用）
                let mtoon = ir_mat.mtoon.as_mut().unwrap();

                // _ShadeColor
                mtoon.shade_color = Some(srgb_vec3_to_linear(get_color3(
                    "_ShadeColor",
                    0.5,
                    0.5,
                    0.5,
                )));

                // _ShadeTexture（未設定時は _MainTex を使用: UniVRM 破壊的マイグレーション準拠）
                mtoon.shade_texture =
                    resolve_tex("_ShadeTexture", true).or_else(|| resolve_tex("_MainTex", true));

                // _ShadeToony / _ShadeShift → UniVRM MigrateToShadingToony/Shift 変換式
                let toony_0x = get_float("_ShadeToony", 0.9);
                let shift_0x = get_float("_ShadeShift", 0.0);
                let range_min = shift_0x;
                let range_max = 1.0 + (shift_0x - 1.0) * toony_0x; // lerp(1, shift, toony)
                mtoon.shading_toony_factor =
                    ((2.0 - (range_max - range_min)) * 0.5).clamp(0.0, 1.0);
                mtoon.shading_shift_factor =
                    ((range_max + range_min) * 0.5 * -1.0).clamp(-1.0, 1.0);

                // _BumpMap / _BumpScale（法線マップ）
                if let Some(tex_info) = resolve_tex("_BumpMap", true) {
                    ir_mat.normal_texture = Some(tex_info);
                    ir_mat.normal_texture_scale = get_float("_BumpScale", 1.0);
                }

                // _EmissionColor / _EmissionMap
                ir_mat.emissive_factor = get_color3("_EmissionColor", 0.0, 0.0, 0.0);
                if let Some(tex_info) = resolve_tex("_EmissionMap", true) {
                    ir_mat.emissive_texture = Some(tex_info);
                }

                // _RimColor / _RimFresnelPower / _RimLift
                // 再取得（ir_mat のフィールドアクセス後に mtoon を再借用）
                let mtoon = ir_mat.mtoon.as_mut().unwrap();
                mtoon.parametric_rim_color =
                    srgb_vec3_to_linear(get_color3("_RimColor", 0.0, 0.0, 0.0));
                mtoon.parametric_rim_fresnel_power = get_float("_RimFresnelPower", 1.0);
                mtoon.parametric_rim_lift = get_float("_RimLift", 0.0);
                // rimLightingMixFactor: UniVRM は破壊的マイグレーションで常に 1.0 に設定
                mtoon.rim_lighting_mix = 1.0;

                // _RimTexture → rimMultiplyTexture
                mtoon.rim_multiply_texture = resolve_tex("_RimTexture", true);

                // _SphereAdd → matcapTexture（VRM 1.0 では MatCap に変換）
                // MatCap は VRM 1.0 で ST 不要（UniVRM MigrationMToonMaterial 準拠）
                if let Some(tex_info) = resolve_tex("_SphereAdd", false) {
                    mtoon.matcap_texture = Some(tex_info);
                    mtoon.matcap_factor = Vec3::ONE;
                } else {
                    mtoon.matcap_factor = Vec3::ZERO;
                }

                // _UvAnimScrollX / _UvAnimScrollY / _UvAnimRotation
                mtoon.uv_animation_scroll_x_speed = get_float("_UvAnimScrollX", 0.0);
                // Y 反転（UniVRM 準拠: invertY = -1）
                mtoon.uv_animation_scroll_y_speed = get_float("_UvAnimScrollY", 0.0) * -1.0;
                // 回転: rotations/sec → rad/sec（× 2π）
                mtoon.uv_animation_rotation_speed =
                    get_float("_UvAnimRotation", 0.0) * std::f32::consts::TAU;

                // _UvAnimMaskTexture（VRM 0.x: Rチャネル参照、UniVRM MToonCore.cginc:129 準拠）
                mtoon.uv_animation_mask_texture = resolve_tex("_UvAnimMaskTexture", true);
                mtoon.uv_anim_mask_tex_channel = ColorChannel::R;

                // _OutlineWidthTexture（VRM 0.x: Rチャネル参照、UniVRM MToonCore.cginc:86 準拠）
                // _MainTex ST 伝播: UniVRM MigrationMToonMaterial 準拠
                if outline_mode != 0 {
                    mtoon.outline_width_texture = resolve_tex("_OutlineWidthTexture", true);
                    mtoon.outline_width_tex_channel = ColorChannel::R;
                }

                // _OutlineColorMode: 0=FixedColor → outlineLightingMix=0.0, 1=MixedLighting → 元値使用
                if outline_mode != 0 {
                    let outline_color_mode = get_float("_OutlineColorMode", 0.0) as i32;
                    if outline_color_mode == 0 {
                        mtoon.outline_lighting_mix = 0.0;
                    }
                }

                // _IndirectLightIntensity → giEqualizationFactor (UniVRM MigrationMToonMaterial.cs:231-232 準拠)
                let gi_intensity = get_float("_IndirectLightIntensity", 0.1);
                mtoon.gi_equalization_factor = (1.0 - gi_intensity).clamp(0.0, 1.0);
            }
        }

        // baseColorTexture の texCoord + KHR_texture_transform 抽出（raw JSON 経由）
        // VRM 0.x MToon で _MainTex が解決済みの場合は materialProperties 側を authoritative source とし、
        // glTF core の baseColorTexture による上書きをスキップする（近似値の可能性があるため）
        if !v0_main_tex_resolved {
            let json = document.as_json();
            if let Some(mat_json) = json.materials.get(i) {
                if let Some(ref bct) = mat_json.pbr_metallic_roughness.base_color_texture {
                    match serde_json::to_value(bct)
                        .ok()
                        .and_then(|val| read_texture_info(&val, document))
                    {
                        Some(ti) => {
                            ir_mat.texture_index = Some(ti.index);
                            ir_mat.base_color_tex_info = Some(ti);
                        }
                        None => {
                            ir_mat.texture_index = None;
                            ir_mat.base_color_tex_info = None;
                        }
                    }
                }
            }
        }

        // VRM 0.x _MainTex ST を baseColorTexture にも適用
        if let Some((scale, offset)) = main_tex_st {
            if let Some(ref mut ti) = ir_mat.base_color_tex_info {
                ti.scale = scale;
                ti.offset = offset;
            }
        }

        // VRM 1.0 MToon拡張からアウトライン情報を抽出
        if *version == VrmVersion::V1 {
            let json = document.as_json();
            if let Some(mat_json) = json.materials.get(i) {
                if let Some(exts) = &mat_json.extensions {
                    if let Some(mtoon_json) = exts.others.get("VRMC_materials_mtoon") {
                        ir_mat.mtoon = Some(MtoonParams::default());
                        let mp = ir_mat.mtoon.as_mut().unwrap();

                        // outlineWidthMode が "none" 以外ならエッジ有効
                        let mode = mtoon_json
                            .get("outlineWidthMode")
                            .and_then(|v| v.as_str())
                            .unwrap_or("none");

                        // OutlineWidthMode を保存（ビューア描画用）
                        mp.outline_width_mode = match mode {
                            "worldCoordinates" => OutlineWidthMode::WorldCoordinates,
                            "screenCoordinates" => OutlineWidthMode::ScreenCoordinates,
                            _ => OutlineWidthMode::None,
                        };

                        if mode != "none" {
                            let width = mtoon_json
                                .get("outlineWidthFactor")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0) as f32;

                            // worldCoordinates: メートル→PMXスケール変換
                            // screenCoordinates: 比率→固定値変換
                            ir_mat.edge_size = match mode {
                                "worldCoordinates" => width * PMX_SCALE * 10.0,
                                "screenCoordinates" => width * 100.0,
                                _ => 0.0,
                            };

                            // ビューア用生値（メートル単位 / 比率）
                            mp.outline_width_factor = width;

                            // outlineColorFactor [r,g,b] → Vec4(r,g,b,1.0)
                            if let Some(color) = mtoon_json.get("outlineColorFactor") {
                                if let Some(arr) = color.as_array() {
                                    let r =
                                        arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                                    let g =
                                        arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                                    let b =
                                        arr.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                                    ir_mat.edge_color = Vec4::new(r, g, b, 1.0);
                                }
                            }

                            // outlineWidthMultiplyTexture → Gチャネルで頂点エッジ倍率を制御
                            if let Some(wtex) = mtoon_json.get("outlineWidthMultiplyTexture") {
                                mp.outline_width_texture = read_texture_info(wtex, document);
                            }

                            // outlineLightingMixFactor (default: 1.0)
                            mp.outline_lighting_mix = mtoon_json
                                .get("outlineLightingMixFactor")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(1.0)
                                as f32;
                        }

                        // giEqualizationFactor (default: 0.9)
                        mp.gi_equalization_factor = mtoon_json
                            .get("giEqualizationFactor")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.9)
                            as f32;

                        // shadeColorFactor (default: [0, 0, 0] — VRM 1.0 MToon 仕様準拠)
                        mp.shade_color = Some(
                            mtoon_json
                                .get("shadeColorFactor")
                                .and_then(|shade| shade.as_array())
                                .map(|arr| {
                                    Vec3::new(
                                        arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                                        arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                                        arr.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                                    )
                                })
                                .unwrap_or(Vec3::ZERO),
                        );

                        // shadingToonyFactor (default: 0.9)
                        mp.shading_toony_factor = mtoon_json
                            .get("shadingToonyFactor")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.9)
                            as f32;

                        // shadingShiftFactor (default: 0.0)
                        mp.shading_shift_factor = mtoon_json
                            .get("shadingShiftFactor")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0)
                            as f32;

                        // parametricRimColorFactor (default: [0,0,0])
                        if let Some(rim) = mtoon_json.get("parametricRimColorFactor") {
                            if let Some(arr) = rim.as_array() {
                                let r = arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                                let g = arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                                let b = arr.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                                mp.parametric_rim_color = Vec3::new(r, g, b);
                            }
                        }

                        // parametricRimFresnelPowerFactor (default: 5.0)
                        mp.parametric_rim_fresnel_power = mtoon_json
                            .get("parametricRimFresnelPowerFactor")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(5.0)
                            as f32;

                        // parametricRimLiftFactor (default: 0.0)
                        mp.parametric_rim_lift = mtoon_json
                            .get("parametricRimLiftFactor")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0) as f32;

                        // rimLightingMixFactor (default: 1.0)
                        mp.rim_lighting_mix = mtoon_json
                            .get("rimLightingMixFactor")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(1.0) as f32;

                        // matcapFactor (default: [1,1,1])
                        if let Some(mcf) = mtoon_json.get("matcapFactor") {
                            if let Some(arr) = mcf.as_array() {
                                let r = arr.first().and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                                let g = arr.get(1).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                                let b = arr.get(2).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                                mp.matcap_factor = Vec3::new(r, g, b);
                            }
                        }

                        // matcapTexture
                        if let Some(mc_tex) = mtoon_json.get("matcapTexture") {
                            mp.matcap_texture = read_texture_info(mc_tex, document);
                        }

                        // shadeMultiplyTexture → shade_texture
                        if let Some(tex) = mtoon_json.get("shadeMultiplyTexture") {
                            mp.shade_texture = read_texture_info(tex, document);
                        }

                        // shadingShiftTexture (Rチャネル) + scale
                        if let Some(tex) = mtoon_json.get("shadingShiftTexture") {
                            mp.shading_shift_texture = read_texture_info(tex, document);
                            if let Some(scale) = tex.get("scale").and_then(|v| v.as_f64()) {
                                mp.shading_shift_texture_scale = scale as f32;
                            }
                        }

                        // rimMultiplyTexture
                        if let Some(tex) = mtoon_json.get("rimMultiplyTexture") {
                            mp.rim_multiply_texture = read_texture_info(tex, document);
                        }

                        // uvAnimation パラメータ
                        mp.uv_animation_scroll_x_speed = mtoon_json
                            .get("uvAnimationScrollXSpeedFactor")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0)
                            as f32;
                        mp.uv_animation_scroll_y_speed = mtoon_json
                            .get("uvAnimationScrollYSpeedFactor")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0)
                            as f32;
                        mp.uv_animation_rotation_speed = mtoon_json
                            .get("uvAnimationRotationSpeedFactor")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0)
                            as f32;

                        // uvAnimationMaskTexture
                        if let Some(tex) = mtoon_json.get("uvAnimationMaskTexture") {
                            mp.uv_animation_mask_texture = read_texture_info(tex, document);
                        }

                        // transparentWithZWrite: BLEND + ZWrite On
                        if ir_mat.alpha_mode == AlphaMode::Blend {
                            let z_write = mtoon_json
                                .get("transparentWithZWrite")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            if z_write {
                                ir_mat.alpha_mode = AlphaMode::BlendWithZWrite;
                            }
                        }

                        // renderQueueOffsetNumber（BLEND 時のみ有効、仕様準拠 clamp）
                        let raw_offset = mtoon_json
                            .get("renderQueueOffsetNumber")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0) as i32;
                        mp.render_queue_offset = match ir_mat.alpha_mode {
                            AlphaMode::Opaque | AlphaMode::Mask => 0,
                            AlphaMode::BlendWithZWrite => raw_offset.clamp(0, 9),
                            AlphaMode::Blend => raw_offset.clamp(-9, 0),
                        };

                        log::debug!("材質[{}] \"{}\" is_mtoon=true, edge_size={:.3}, edge_color=({:.2},{:.2},{:.2},{:.2}), rim=({:.2},{:.2},{:.2}), matcap_tex={:?}",
                            i, ir_mat.name, ir_mat.edge_size,
                            ir_mat.edge_color.x, ir_mat.edge_color.y, ir_mat.edge_color.z, ir_mat.edge_color.w,
                            mp.parametric_rim_color.x, mp.parametric_rim_color.y, mp.parametric_rim_color.z,
                            mp.matcap_texture.as_ref().map(|t| t.index));
                    }
                }
            }
        }

        // Ambient を diffuseから計算
        ir_mat.ambient = Vec3::new(
            ir_mat.diffuse.x * 0.4,
            ir_mat.diffuse.y * 0.4,
            ir_mat.diffuse.z * 0.4,
        );

        materials.push(ir_mat);
    }

    // VRM 0.x renderQueue → render_queue_offset 移行（UniVRM MigrationMToonMaterial 準拠）
    // rank 圧縮: 透明材質群の相対順序を保ちつつ連番に圧縮する
    if *version == VrmVersion::V0 {
        remap_vrm0_render_queue_offsets(&mut materials, v0_mat_props);
    }

    // 材質が0個なら仮材質を追加
    if materials.is_empty() {
        materials.push(IrMaterial::default());
    }

    Ok(materials)
}

fn extract_bones(document: &gltf::Document, typed: &VrmTyped) -> Result<BoneExtractResult> {
    let nodes: Vec<gltf::Node> = document.nodes().collect();
    let mut bones: Vec<IrBone> = Vec::with_capacity(nodes.len());
    let mut node_to_bone: HashMap<usize, usize> = HashMap::new();

    // VRMヒューマノイドボーンのノードIndex → ボーン名 のマップを構築
    let mut humanoid_map: HashMap<usize, String> = HashMap::new();
    build_humanoid_map(typed, &mut humanoid_map)?;

    // 全ノードのグローバル変換行列を計算
    let global_mats = compute_global_transforms(&nodes);

    // ノードIndex順にボーンを割り当て
    for node in &nodes {
        let idx = node.index();
        let bone_idx = bones.len();
        node_to_bone.insert(idx, bone_idx);

        let global_mat = global_mats.get(idx).copied().unwrap_or(Mat4::IDENTITY);
        let pos = global_mat.transform_point3(Vec3::ZERO);
        let vrm_name = humanoid_map.get(&idx).cloned();

        let node_name = node.name().unwrap_or(&format!("bone_{}", idx)).to_string();
        bones.push(IrBone {
            name: node_name.clone(),
            name_en: node_name.clone(),
            original_name: node_name,
            vrm_bone_name: vrm_name,
            position: pos,
            global_mat,
            parent: None,
            children: Vec::new(),
            node_index: idx,
            is_physics: false,
            tail_position: None,
            tail_bone_index: None,
            is_ik: false,
            is_ik_bone: false,
            is_translatable: false,
            is_axis_fixed: false,
            is_visible: true,
            grant: None,
        });
    }

    // 親子関係を設定
    for node in &nodes {
        let parent_bone_idx = node_to_bone[&node.index()];
        for child in node.children() {
            let child_bone_idx = node_to_bone[&child.index()];
            bones[child_bone_idx].parent = Some(parent_bone_idx);
            bones[parent_bone_idx].children.push(child_bone_idx);
        }
    }

    Ok((bones, node_to_bone, global_mats))
}

fn compute_global_transforms(nodes: &[gltf::Node]) -> Vec<Mat4> {
    let n = nodes.len();
    let mut global_mat = vec![Mat4::IDENTITY; n];
    let mut visited = vec![false; n];

    // 親のないノードから深さ優先で処理
    let mut has_parent = vec![false; n];
    for node in nodes {
        for child in node.children() {
            if child.index() < n {
                has_parent[child.index()] = true;
            }
        }
    }

    // BFS/DFS用スタック: (ノードIndex, 親グローバル行列)
    let mut stack: Vec<(usize, Mat4)> = nodes
        .iter()
        .filter(|n| !has_parent[n.index()])
        .map(|n| (n.index(), Mat4::IDENTITY))
        .collect();

    while let Some((idx, parent_mat)) = stack.pop() {
        if visited[idx] {
            continue;
        }
        visited[idx] = true;

        let node = &nodes[idx];
        let (t, r, s) = node.transform().decomposed();
        let local_mat = Mat4::from_scale_rotation_translation(
            glam::Vec3::from(s),
            glam::Quat::from_array(r),
            glam::Vec3::from(t),
        );
        global_mat[idx] = parent_mat * local_mat;

        for child in node.children() {
            stack.push((child.index(), global_mat[idx]));
        }
    }

    global_mat
}

fn build_humanoid_map(typed: &VrmTyped, map: &mut HashMap<usize, String>) -> Result<()> {
    match typed {
        VrmTyped::V1(v1) => {
            if let Some(humanoid) = &v1.humanoid {
                let bones = &humanoid.human_bones;
                macro_rules! add_bone {
                    ($field:expr, $name:expr) => {
                        if let Some(b) = &$field {
                            map.insert(b.node as usize, $name.to_string());
                        }
                    };
                }
                add_bone!(bones.hips, "hips");
                add_bone!(bones.spine, "spine");
                add_bone!(bones.chest, "chest");
                add_bone!(bones.upper_chest, "upperChest");
                add_bone!(bones.neck, "neck");
                add_bone!(bones.head, "head");
                add_bone!(bones.left_eye, "leftEye");
                add_bone!(bones.right_eye, "rightEye");
                add_bone!(bones.jaw, "jaw");
                add_bone!(bones.left_upper_leg, "leftUpperLeg");
                add_bone!(bones.left_lower_leg, "leftLowerLeg");
                add_bone!(bones.left_foot, "leftFoot");
                add_bone!(bones.left_toes, "leftToes");
                add_bone!(bones.right_upper_leg, "rightUpperLeg");
                add_bone!(bones.right_lower_leg, "rightLowerLeg");
                add_bone!(bones.right_foot, "rightFoot");
                add_bone!(bones.right_toes, "rightToes");
                add_bone!(bones.left_shoulder, "leftShoulder");
                add_bone!(bones.left_upper_arm, "leftUpperArm");
                add_bone!(bones.left_lower_arm, "leftLowerArm");
                add_bone!(bones.left_hand, "leftHand");
                add_bone!(bones.right_shoulder, "rightShoulder");
                add_bone!(bones.right_upper_arm, "rightUpperArm");
                add_bone!(bones.right_lower_arm, "rightLowerArm");
                add_bone!(bones.right_hand, "rightHand");
                add_bone!(bones.left_thumb_metacarpal, "leftThumbMetacarpal");
                add_bone!(bones.left_thumb_proximal, "leftThumbProximal");
                add_bone!(bones.left_thumb_distal, "leftThumbDistal");
                add_bone!(bones.left_index_proximal, "leftIndexProximal");
                add_bone!(bones.left_index_intermediate, "leftIndexIntermediate");
                add_bone!(bones.left_index_distal, "leftIndexDistal");
                add_bone!(bones.left_middle_proximal, "leftMiddleProximal");
                add_bone!(bones.left_middle_intermediate, "leftMiddleIntermediate");
                add_bone!(bones.left_middle_distal, "leftMiddleDistal");
                add_bone!(bones.left_ring_proximal, "leftRingProximal");
                add_bone!(bones.left_ring_intermediate, "leftRingIntermediate");
                add_bone!(bones.left_ring_distal, "leftRingDistal");
                add_bone!(bones.left_little_proximal, "leftLittleProximal");
                add_bone!(bones.left_little_intermediate, "leftLittleIntermediate");
                add_bone!(bones.left_little_distal, "leftLittleDistal");
                add_bone!(bones.right_thumb_metacarpal, "rightThumbMetacarpal");
                add_bone!(bones.right_thumb_proximal, "rightThumbProximal");
                add_bone!(bones.right_thumb_distal, "rightThumbDistal");
                add_bone!(bones.right_index_proximal, "rightIndexProximal");
                add_bone!(bones.right_index_intermediate, "rightIndexIntermediate");
                add_bone!(bones.right_index_distal, "rightIndexDistal");
                add_bone!(bones.right_middle_proximal, "rightMiddleProximal");
                add_bone!(bones.right_middle_intermediate, "rightMiddleIntermediate");
                add_bone!(bones.right_middle_distal, "rightMiddleDistal");
                add_bone!(bones.right_ring_proximal, "rightRingProximal");
                add_bone!(bones.right_ring_intermediate, "rightRingIntermediate");
                add_bone!(bones.right_ring_distal, "rightRingDistal");
                add_bone!(bones.right_little_proximal, "rightLittleProximal");
                add_bone!(bones.right_little_intermediate, "rightLittleIntermediate");
                add_bone!(bones.right_little_distal, "rightLittleDistal");
            }
        }
        VrmTyped::V0(v0) => {
            if let Some(humanoid) = &v0.humanoid {
                for bone in &humanoid.human_bones {
                    map.insert(bone.node as usize, bone.bone.clone());
                }
            }
        }
        VrmTyped::Unknown => {}
    }
    Ok(())
}

fn extract_meshes(
    document: &gltf::Document,
    buffers: &[Data],
    images: &[gltf::image::Data],
    node_to_bone: &HashMap<usize, usize>,
    materials: &[IrMaterial],
    global_mats: &[Mat4],
) -> Result<Vec<IrMesh>> {
    let mut ir_meshes = Vec::new();

    for node in document.nodes() {
        if let Some(mesh) = node.mesh() {
            let node_idx = node.index();
            let bone_idx = node_to_bone.get(&node_idx).copied().unwrap_or(0);

            for primitive in mesh.primitives() {
                let reader = primitive.reader(|buf| Some(&buffers[buf.index()]));

                // 位置
                let positions: Vec<[f32; 3]> = match reader.read_positions() {
                    Some(iter) => iter.collect(),
                    None => continue,
                };

                if positions.is_empty() {
                    continue;
                }

                // 法線
                let normals: Vec<[f32; 3]> = reader
                    .read_normals()
                    .map(|iter| iter.collect())
                    .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);

                // 接線（glTF TANGENT 属性）
                let tangents: Vec<[f32; 4]> = reader
                    .read_tangents()
                    .map(|iter| iter.collect())
                    .unwrap_or_default();

                // UV
                let uvs: Vec<[f32; 2]> = reader
                    .read_tex_coords(0)
                    .map(|iter| iter.into_f32().collect())
                    .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);

                // TEXCOORD_1（セカンダリUV）
                let uvs1: Vec<[f32; 2]> = reader
                    .read_tex_coords(1)
                    .map(|iter| iter.into_f32().collect())
                    .unwrap_or_default();

                // ジョイント・ウェイト
                let joints: Vec<[u16; 4]> = reader
                    .read_joints(0)
                    .map(|iter| iter.into_u16().collect())
                    .unwrap_or_default();
                let weights: Vec<[f32; 4]> = reader
                    .read_weights(0)
                    .map(|iter| iter.into_f32().collect())
                    .unwrap_or_default();

                // スキンのジョイント→ボーンマッピング
                let skin_bone_map: Vec<usize> = if let Some(skin) = node.skin() {
                    skin.joints()
                        .map(|j| *node_to_bone.get(&j.index()).unwrap_or(&0))
                        .collect()
                } else {
                    Vec::new()
                };

                // スキニング行列（joint_world_mat * inv_bind_mat）を事前計算
                // これによりバインドポーズ（Aスタンス等）からT-ポーズ世界座標に変換できる
                let skin_mats: Vec<Mat4> = if let Some(skin) = node.skin() {
                    let inv_binds: Vec<Mat4> = skin
                        .reader(|buf| Some(&buffers[buf.index()]))
                        .read_inverse_bind_matrices()
                        .map(|iter| iter.map(|m| Mat4::from_cols_array_2d(&m)).collect())
                        .unwrap_or_else(|| vec![Mat4::IDENTITY; skin.joints().count()]);

                    skin.joints()
                        .enumerate()
                        .map(|(ji, j)| {
                            let world = global_mats
                                .get(j.index())
                                .copied()
                                .unwrap_or(Mat4::IDENTITY);
                            let inv_bind = inv_binds.get(ji).copied().unwrap_or(Mat4::IDENTITY);
                            world * inv_bind
                        })
                        .collect()
                } else {
                    Vec::new()
                };

                // 法線変換用の逆転置行列を事前計算（glTF仕様準拠）
                // 非一様スケールがある場合に M*n では法線方向が崩れるため (M⁻¹)ᵀ*n を使用
                let normal_mats: Vec<Mat3> = skin_mats
                    .iter()
                    .map(|sm| Mat3::from_mat4(*sm).inverse().transpose())
                    .collect();

                // 接線変換用行列（方向ベクトルなので M の上3x3をそのまま使用、法線とは異なり逆転置不要）
                let tangent_mats: Vec<Mat3> =
                    skin_mats.iter().map(|sm| Mat3::from_mat4(*sm)).collect();

                // 頂点構築（スキニングによりT-ポーズ世界座標を計算）
                let vertices: Vec<IrVertex> = positions
                    .iter()
                    .enumerate()
                    .map(|(i, pos)| {
                        let normal = normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
                        let uv = uvs.get(i).copied().unwrap_or([0.0, 0.0]);

                        // glTF TANGENT 属性（あれば）
                        let src_tangent = tangents.get(i).copied();

                        // スキニング演算でT-ポーズ頂点位置・法線・接線を計算
                        let (final_pos, final_normal, final_tangent) = if !skin_mats.is_empty()
                            && !joints.is_empty()
                        {
                            let j = joints.get(i).copied().unwrap_or([0; 4]);
                            let w = weights.get(i).copied().unwrap_or([0.0; 4]);
                            let lp = Vec4::new(pos[0], pos[1], pos[2], 1.0);
                            let ln = Vec3::new(normal[0], normal[1], normal[2]);
                            let lt = src_tangent.map(|t| Vec3::new(t[0], t[1], t[2]));
                            let lt_w = src_tangent.map(|t| t[3]).unwrap_or(1.0);
                            let mut wp = Vec4::ZERO;
                            let mut wn = Vec3::ZERO;
                            let mut wt = Vec3::ZERO;
                            for k in 0..4 {
                                if w[k] > 0.0 {
                                    let ji = j[k] as usize;
                                    if let Some(sm) = skin_mats.get(ji) {
                                        wp += w[k] * (*sm * lp);
                                    }
                                    // 法線は逆転置行列で変換（glTF仕様準拠）
                                    if let Some(nm) = normal_mats.get(ji) {
                                        wn += w[k] * (*nm * ln);
                                    }
                                    // 接線は通常行列で変換（方向ベクトル）
                                    if let (Some(lt_dir), Some(tm)) = (lt, tangent_mats.get(ji)) {
                                        wt += w[k] * (*tm * lt_dir);
                                    }
                                }
                            }
                            let fp = Vec3::new(wp.x, wp.y, wp.z);
                            let fn3 = wn.normalize_or_zero();
                            let ft = if src_tangent.is_some() {
                                let t3 = wt.normalize_or_zero();
                                // Gram-Schmidt 再直交化: 非一様スケール時に
                                // normal と tangent の直交性が崩れるため（animation.rs と同一方針）
                                let t_ortho = (t3 - fn3 * fn3.dot(t3)).normalize_or_zero();
                                // 退化判定: tangent が normal とほぼ平行だと Gram-Schmidt で
                                // ゼロになる → MikkTSpace 再生成ルートへ流す
                                if t_ortho.length_squared() < 1e-8 || !t_ortho.is_finite() {
                                    Vec4::ZERO
                                } else {
                                    t_ortho.extend(if lt_w >= 0.0 { 1.0 } else { -1.0 })
                                }
                            } else {
                                Vec4::ZERO // MikkTSpace で後から生成
                            };
                            (fp, [fn3.x, fn3.y, fn3.z], ft)
                        } else {
                            // 非スキンメッシュ: ノードのワールド変換を適用
                            let node_mat =
                                global_mats.get(node_idx).copied().unwrap_or(Mat4::IDENTITY);
                            let lp = Vec4::new(pos[0], pos[1], pos[2], 1.0);
                            let wp = node_mat * lp;
                            let fp = Vec3::new(wp.x, wp.y, wp.z);
                            let ln = Vec3::new(normal[0], normal[1], normal[2]);
                            let nmat = Mat3::from_mat4(node_mat).inverse().transpose();
                            let fn3 = (nmat * ln).normalize_or_zero();
                            let ft = if let Some(t) = src_tangent {
                                let tmat = Mat3::from_mat4(node_mat);
                                let lt = Vec3::new(t[0], t[1], t[2]);
                                let wt = (tmat * lt).normalize_or_zero();
                                // Gram-Schmidt 再直交化（非一様スケール対策）
                                let t_ortho = (wt - fn3 * fn3.dot(wt)).normalize_or_zero();
                                // 退化判定: tangent が normal とほぼ平行な場合は
                                // MikkTSpace 再生成ルートへ流す
                                if t_ortho.length_squared() < 1e-8 || !t_ortho.is_finite() {
                                    Vec4::ZERO
                                } else {
                                    t_ortho.extend(if t[3] >= 0.0 { 1.0 } else { -1.0 })
                                }
                            } else {
                                Vec4::ZERO // MikkTSpace で後から生成
                            };
                            (fp, [fn3.x, fn3.y, fn3.z], ft)
                        };

                        let (vtx_weights, vtx_weight_count) = if !joints.is_empty()
                            && !skin_bone_map.is_empty()
                        {
                            let j = joints.get(i).copied().unwrap_or([0; 4]);
                            let w = weights.get(i).copied().unwrap_or([0.0; 4]);
                            let mut arr = [(0usize, 0.0f32); 4];
                            let mut cnt = 0u8;
                            for k in 0..4 {
                                if w[k] > 0.0 {
                                    let bi = skin_bone_map.get(j[k] as usize).copied().unwrap_or(0);
                                    arr[cnt as usize] = (bi, w[k]);
                                    cnt += 1;
                                }
                            }
                            (arr, cnt)
                        } else {
                            ([(bone_idx, 1.0), (0, 0.0), (0, 0.0), (0, 0.0)], 1)
                        };

                        IrVertex {
                            position: final_pos,
                            normal: Vec3::new(final_normal[0], final_normal[1], final_normal[2]),
                            uv: Vec2::new(uv[0], uv[1]),
                            tangent: final_tangent,
                            weights: vtx_weights,
                            weight_count: vtx_weight_count,
                            edge_scale: 1.0, // テクスチャサンプリング後に更新
                        }
                    })
                    .collect();

                // インデックス
                let indices: Vec<u32> = reader
                    .read_indices()
                    .map(|iter| iter.into_u32().collect())
                    .unwrap_or_else(|| (0..positions.len() as u32).collect());

                // 材質インデックス
                let material_index = primitive.material().index().unwrap_or(0);

                // モーフターゲット
                let morph_targets =
                    extract_morph_targets_from_reader(&primitive, buffers, positions.len());

                // outlineWidthMultiplyTexture からエッジ倍率を頂点ごとに設定
                // IrTextureInfo.index は image index に正規化済み
                let mut vertices = vertices;
                if let Some(ir_mat) = materials.get(material_index) {
                    if let Some(tex_info) = ir_mat.mtoon().outline_width_texture.as_ref() {
                        if let Some(img) = images.get(tex_info.index) {
                            for (local_vi, vtx) in vertices.iter_mut().enumerate() {
                                let uv0 = Vec2::new(vtx.uv.x, vtx.uv.y);
                                let uv1 = uvs1.get(local_vi).map(|uv| Vec2::new(uv[0], uv[1]));
                                let uv = resolve_cpu_uv(uv0, uv1, tex_info);
                                vtx.edge_scale = sample_image_channel(
                                    img,
                                    uv.x,
                                    uv.y,
                                    &tex_info.sampler,
                                    ir_mat.mtoon().outline_width_tex_channel,
                                );
                            }
                            let zero_count =
                                vertices.iter().filter(|v| v.edge_scale < 0.01).count();
                            let full_count =
                                vertices.iter().filter(|v| v.edge_scale > 0.99).count();
                            log::debug!("メッシュ \"{}\" outline_width_texture: 頂点{}中 edge_scale≈0:{}, ≈1:{}",
                                mesh.name().unwrap_or("?"), vertices.len(), zero_count, full_count);
                        }
                    }
                }

                let mut ir_mesh = IrMesh {
                    name: mesh
                        .name()
                        .unwrap_or(&format!("mesh_{}", mesh.index()))
                        .to_string(),
                    vertices,
                    indices,
                    material_index,
                    morph_targets,
                    node_index: node_idx,
                    uvs1,
                };
                // glTF に TANGENT 属性がなければ MikkTSpace で接線を生成
                // VRM 仕様: 「TANGENT はエクスポートせず、インポート時に MikkTSpace で計算」
                // normalTexture.texCoord に応じた UV セットで接線を生成
                let normal_tex_coord = materials
                    .get(material_index)
                    .and_then(|m| m.normal_texture.as_ref())
                    .map(|t| t.tex_coord)
                    .unwrap_or(0);
                crate::intermediate::tangent::generate_tangents(&mut ir_mesh, normal_tex_coord);
                ir_meshes.push(ir_mesh);
            }
        }
    }

    Ok(ir_meshes)
}

/// CPU側 UV 解決: texCoord 選択 + KHR_texture_transform 適用
/// GPU 側の resolve_mtoon_uv / apply_texture_transform と同一順序（scale → rotation → offset）
fn resolve_cpu_uv(uv0: Vec2, uv1: Option<Vec2>, info: &IrTextureInfo) -> Vec2 {
    let uv = if info.tex_coord == 1 {
        uv1.unwrap_or(Vec2::ZERO) // UniVRM 準拠: UV1 不在時は zero
    } else {
        uv0
    };
    let scaled = uv * info.scale;
    let (s, c) = info.rotation.sin_cos();
    Vec2::new(scaled.x * c - scaled.y * s, scaled.x * s + scaled.y * c) + info.offset
}

/// UV 座標にラップモードを適用して 0.0〜1.0 に正規化する
fn apply_wrap(coord: f32, mode: IrWrapMode) -> f32 {
    match mode {
        IrWrapMode::Repeat => {
            let f = coord.fract();
            if f < 0.0 {
                f + 1.0
            } else {
                f
            }
        }
        IrWrapMode::ClampToEdge => coord.clamp(0.0, 1.0),
        IrWrapMode::MirroredRepeat => {
            let t = coord.rem_euclid(2.0);
            if t > 1.0 {
                2.0 - t
            } else {
                t
            }
        }
    }
}

/// 画像の指定チャネル値をUV座標でサンプリング（0.0〜1.0）
/// VRM 1.0: outlineWidthMultiplyTexture=G, uvAnimationMaskTexture=B
/// VRM 0.x: 両方ともRチャネル（UniVRM MToonCore.cginc 準拠）
fn sample_image_channel(
    img: &gltf::image::Data,
    u: f32,
    v: f32,
    sampler: &IrSamplerInfo,
    channel: ColorChannel,
) -> f32 {
    let w = img.width as usize;
    let h = img.height as usize;
    if w == 0 || h == 0 {
        return 1.0;
    }

    // UV → ピクセル座標（サンプラーのラップモードに従う）
    let fu = apply_wrap(u, sampler.wrap_u);
    let fv = apply_wrap(v, sampler.wrap_v);
    let px = ((fu * w as f32) as usize).min(w - 1);
    let py = ((fv * h as f32) as usize).min(h - 1);

    use gltf::image::Format;
    let channel_offset = match channel {
        ColorChannel::R => 0,
        ColorChannel::G => 1,
        ColorChannel::B => 2,
    };
    let (bpp, offset) = match img.format {
        Format::R8 => (1, 0), // 単チャネル → 常にR値を使用
        Format::R8G8 => (2, channel_offset.min(1)),
        Format::R8G8B8 => (3, channel_offset),
        Format::R8G8B8A8 => (4, channel_offset),
        _ => return 1.0, // 16bit/float形式は非対応
    };
    let idx = (py * w + px) * bpp + offset;
    img.pixels
        .get(idx)
        .map(|&v| v as f32 / 255.0)
        .unwrap_or(1.0)
}

fn extract_morph_targets_from_reader(
    primitive: &gltf::Primitive,
    buffers: &[Data],
    vertex_count: usize,
) -> Vec<IrMorphTarget> {
    let reader = primitive.reader(|buf| Some(&buffers[buf.index()]));
    let mut targets = Vec::new();

    for (i, (positions_opt, normals_opt, tangents_opt)) in reader.read_morph_targets().enumerate() {
        let positions: Vec<[f32; 3]> = positions_opt.map(|iter| iter.collect()).unwrap_or_default();
        let normals: Vec<[f32; 3]> = normals_opt.map(|iter| iter.collect()).unwrap_or_default();
        let tangents_raw: Vec<[f32; 3]> =
            tangents_opt.map(|iter| iter.collect()).unwrap_or_default();

        let position_offsets: Vec<(u32, Vec3)> = (0..vertex_count)
            .filter_map(|j| {
                positions.get(j).and_then(|p| {
                    if p[0].abs() > 1e-7 || p[1].abs() > 1e-7 || p[2].abs() > 1e-7 {
                        Some((j as u32, Vec3::new(p[0], p[1], p[2])))
                    } else {
                        None
                    }
                })
            })
            .collect();
        let normal_offsets: Vec<(u32, Vec3)> = (0..vertex_count)
            .filter_map(|j| {
                normals.get(j).and_then(|n| {
                    if n[0].abs() > 1e-7 || n[1].abs() > 1e-7 || n[2].abs() > 1e-7 {
                        Some((j as u32, Vec3::new(n[0], n[1], n[2])))
                    } else {
                        None
                    }
                })
            })
            .collect();
        let tangent_offsets: Vec<(u32, Vec3)> = (0..vertex_count)
            .filter_map(|j| {
                tangents_raw.get(j).and_then(|t| {
                    if t[0].abs() > 1e-7 || t[1].abs() > 1e-7 || t[2].abs() > 1e-7 {
                        Some((j as u32, Vec3::new(t[0], t[1], t[2])))
                    } else {
                        None
                    }
                })
            })
            .collect();
        targets.push(IrMorphTarget {
            name: format!("morph_{}", i),
            position_offsets,
            normal_offsets,
            tangent_offsets,
        });
    }

    targets
}

fn extract_morphs(
    document: &gltf::Document,
    typed: &VrmTyped,
    ir_meshes: &[IrMesh],
    node_to_bone: &HashMap<usize, usize>,
) -> Result<Vec<IrMorph>> {
    match typed {
        VrmTyped::V0(v0) => extract_morphs_v0(document, v0, ir_meshes),
        VrmTyped::V1(v1) => extract_morphs_v1(document, v1, ir_meshes, node_to_bone),
        VrmTyped::Unknown => Ok(Vec::new()),
    }
}

fn extract_morphs_v0(
    document: &gltf::Document,
    v0: &VrmV0,
    ir_meshes: &[IrMesh],
) -> Result<Vec<IrMorph>> {
    let bsm = match &v0.blend_shape_master {
        Some(b) => b,
        None => return Ok(Vec::new()),
    };

    // グローバル頂点オフセット計算（IrMeshes内での先頭位置）
    let mut mesh_vertex_start: Vec<usize> = vec![0; ir_meshes.len()];
    {
        let mut offset = 0usize;
        for (i, m) in ir_meshes.iter().enumerate() {
            mesh_vertex_start[i] = offset;
            offset += m.vertices.len();
        }
    }

    // document.mesh(index)のnameとir_meshのname/indexの対応を構築
    // meshIndexをベースに: document内mesh[bind.mesh]のprimitiveは複数あり得るが
    // VRM 0.0はmeshとprimitiveが1:1が多いのでmesh_indexで検索
    let mut morphs = Vec::new();
    for group in &bsm.blend_shape_groups {
        let (jp_name, panel) = crate::convert::morph::preset_to_jp_v0(&group.preset_name);
        let name = if jp_name.is_empty() {
            group.name.clone()
        } else {
            jp_name
        };

        let mut vertex_offsets: Vec<(usize, Vec3)> = Vec::new();
        let mut normal_offsets_all: Vec<(usize, Vec3)> = Vec::new();
        let mut tangent_offsets_all: Vec<(usize, Vec3)> = Vec::new();

        for bind in &group.binds {
            let target_mesh_idx = bind.mesh as usize;
            let mesh_name = document
                .meshes()
                .nth(target_mesh_idx)
                .and_then(|m| m.name().map(|s| s.to_string()));

            for (ir_idx, ir_mesh) in ir_meshes.iter().enumerate() {
                let name_match = mesh_name
                    .as_deref()
                    .map(|n| n == ir_mesh.name)
                    .unwrap_or(false);
                if !name_match {
                    let node_has_mesh = document.nodes().any(|n| {
                        n.index() == ir_mesh.node_index
                            && n.mesh()
                                .map(|m| m.index() == target_mesh_idx)
                                .unwrap_or(false)
                    });
                    if !node_has_mesh {
                        continue;
                    }
                }

                let morph_target = ir_mesh.morph_targets.get(bind.index as usize);
                if let Some(mt) = morph_target {
                    let scale = bind.weight / 100.0; // VRM0.0は0-100スケール
                    let vstart = mesh_vertex_start[ir_idx];
                    for &(vi, off) in &mt.position_offsets {
                        vertex_offsets.push((vstart + vi as usize, off * scale));
                    }
                    for &(vi, off) in &mt.normal_offsets {
                        normal_offsets_all.push((vstart + vi as usize, off * scale));
                    }
                    for &(vi, off) in &mt.tangent_offsets {
                        tangent_offsets_all.push((vstart + vi as usize, off * scale));
                    }
                }
            }
        }

        if !vertex_offsets.is_empty()
            || !normal_offsets_all.is_empty()
            || !tangent_offsets_all.is_empty()
        {
            morphs.push(IrMorph {
                name: name.clone(),
                name_en: group.preset_name.clone(),
                panel,
                kind: IrMorphKind::Vertex {
                    positions: vertex_offsets,
                    normals: normal_offsets_all,
                    tangents: tangent_offsets_all,
                },
            });
        }
    }

    Ok(morphs)
}

fn extract_morphs_v1(
    _document: &gltf::Document,
    v1: &VrmV1,
    ir_meshes: &[IrMesh],
    _node_to_bone: &HashMap<usize, usize>,
) -> Result<Vec<IrMorph>> {
    let expressions = match &v1.expressions {
        Some(e) => e,
        None => return Ok(Vec::new()),
    };

    // グローバル頂点オフセット計算
    let mut mesh_vertex_start: Vec<usize> = vec![0; ir_meshes.len()];
    {
        let mut offset = 0usize;
        for (i, m) in ir_meshes.iter().enumerate() {
            mesh_vertex_start[i] = offset;
            offset += m.vertices.len();
        }
    }

    // ノードIndex → IrMeshIndexリスト マッピング（1ノードに複数プリミティブ対応）
    let mut node_to_ir_meshes: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, m) in ir_meshes.iter().enumerate() {
        node_to_ir_meshes.entry(m.node_index).or_default().push(i);
    }

    // バインドを処理してオフセットを収集するヘルパー
    let collect_offsets = |binds: &[crate::vrm::types_v1::MorphTargetBind]|
     -> (Vec<(usize, Vec3)>, Vec<(usize, Vec3)>, Vec<(usize, Vec3)>) {
        let mut vertex_offsets: Vec<(usize, Vec3)> = Vec::new();
        let mut normal_offsets: Vec<(usize, Vec3)> = Vec::new();
        let mut tangent_offsets: Vec<(usize, Vec3)> = Vec::new();
        for bind in binds {
            if bind.weight == 0.0 {
                continue;
            }
            let node_idx = bind.node as usize;
            if let Some(ir_indices) = node_to_ir_meshes.get(&node_idx) {
                for &ir_idx in ir_indices {
                    let ir_mesh = &ir_meshes[ir_idx];
                    if let Some(mt) = ir_mesh.morph_targets.get(bind.index as usize) {
                        let scale = bind.weight;
                        let vstart = mesh_vertex_start[ir_idx];
                        for &(vi, off) in &mt.position_offsets {
                            vertex_offsets.push((vstart + vi as usize, off * scale));
                        }
                        for &(vi, off) in &mt.normal_offsets {
                            normal_offsets.push((vstart + vi as usize, off * scale));
                        }
                        for &(vi, off) in &mt.tangent_offsets {
                            tangent_offsets.push((vstart + vi as usize, off * scale));
                        }
                    }
                }
            }
        }
        (vertex_offsets, normal_offsets, tangent_offsets)
    };

    let mut morphs = Vec::new();

    macro_rules! process_expr {
        ($expr_opt:expr, $preset_name:expr) => {
            if let Some(expr) = &$expr_opt {
                let (jp_name, panel) = crate::convert::morph::preset_to_jp_v1($preset_name);
                let (vertex_offsets, normal_offs, tangent_offs) =
                    if let Some(binds) = &expr.morph_target_binds {
                        collect_offsets(binds)
                    } else {
                        (Vec::new(), Vec::new(), Vec::new())
                    };
                if !vertex_offsets.is_empty() || !normal_offs.is_empty() || !tangent_offs.is_empty()
                {
                    morphs.push(IrMorph {
                        name: jp_name,
                        name_en: $preset_name.to_string(),
                        panel,
                        kind: IrMorphKind::Vertex {
                            positions: vertex_offsets,
                            normals: normal_offs,
                            tangents: tangent_offs,
                        },
                    });
                }
            }
        };
    }

    if let Some(preset) = &expressions.preset {
        process_expr!(preset.aa, "aa");
        process_expr!(preset.ih, "ih");
        process_expr!(preset.ou, "ou");
        process_expr!(preset.ee, "ee");
        process_expr!(preset.oh, "oh");
        process_expr!(preset.blink, "blink");
        process_expr!(preset.blink_left, "blinkLeft");
        process_expr!(preset.blink_right, "blinkRight");
        process_expr!(preset.happy, "happy");
        process_expr!(preset.angry, "angry");
        process_expr!(preset.sad, "sad");
        process_expr!(preset.relaxed, "relaxed");
        process_expr!(preset.surprised, "surprised");
        process_expr!(preset.neutral, "neutral");
        process_expr!(preset.look_up, "lookUp");
        process_expr!(preset.look_down, "lookDown");
        process_expr!(preset.look_left, "lookLeft");
        process_expr!(preset.look_right, "lookRight");
    }

    // カスタム表情
    if let Some(custom) = &expressions.custom {
        for (name, expr) in custom {
            let (vertex_offsets, normal_offs, tangent_offs) =
                if let Some(binds) = &expr.morph_target_binds {
                    collect_offsets(binds)
                } else {
                    (Vec::new(), Vec::new(), Vec::new())
                };
            if !vertex_offsets.is_empty() || !normal_offs.is_empty() || !tangent_offs.is_empty() {
                morphs.push(IrMorph {
                    name: name.clone(),
                    name_en: name.clone(),
                    panel: 4,
                    kind: IrMorphKind::Vertex {
                        positions: vertex_offsets,
                        normals: normal_offs,
                        tangents: tangent_offs,
                    },
                });
            }
        }
    }

    Ok(morphs)
}

fn extract_physics(
    typed: &VrmTyped,
    all_extensions: &Value,
    node_to_bone: &HashMap<usize, usize>,
    bones: &[IrBone],
) -> Result<IrPhysics> {
    match typed {
        VrmTyped::V0(v0) => extract_physics_v0(v0, node_to_bone, bones),
        // V1 および Unknown: all_extensions から VRMC_springBone を検索
        // （plain GLB でも VRMC_springBone 拡張を持つ可能性がある）
        VrmTyped::V1(_) | VrmTyped::Unknown => {
            extract_physics_v1(all_extensions, node_to_bone, bones)
        }
    }
}

fn extract_physics_v0(
    v0: &VrmV0,
    node_to_bone: &HashMap<usize, usize>,
    bones: &[IrBone],
) -> Result<IrPhysics> {
    let sec = match &v0.secondary_animation {
        Some(s) => s,
        None => return Ok(IrPhysics::default()),
    };

    crate::convert::physics::build_physics_v0(sec, node_to_bone, bones)
}

fn extract_physics_v1(
    all_extensions: &Value,
    node_to_bone: &HashMap<usize, usize>,
    bones: &[IrBone],
) -> Result<IrPhysics> {
    let spring_ext = all_extensions.get("VRMC_springBone");
    let spring_bone = match spring_ext {
        Some(v) => serde_json::from_value::<SpringBoneV1>(v.clone()).unwrap_or_default(),
        None => return Ok(IrPhysics::default()),
    };

    crate::convert::physics::build_physics_v1(&spring_bone, node_to_bone, bones)
}

/// VRM 0.x renderQueue → VRM 1.0 render_queue_offset rank 圧縮
/// UniVRM MigrationMToonMaterial.cs:47-70 準拠
///
/// 1. 全透明材質の source offset を BTreeSet に集める（Blend / BlendWithZWrite 別）
/// 2. Blend: 降順で 0, -1, -2, ... と連番マッピング → clamp(-9, 0)
/// 3. BlendWithZWrite: 昇順で 0, 1, 2, ... と連番マッピング → clamp(0, 9)
/// 4. 各材質に rank 圧縮後の offset を適用
fn remap_vrm0_render_queue_offsets(
    materials: &mut [IrMaterial],
    v0_mat_props: &[crate::vrm::types_v0::VrmMaterialProperty],
) {
    // source offset = renderQueue - DefaultValue
    // Blend: Default=3000, 有効範囲 2951..=3000 → offset -49..=0
    // BlendWithZWrite: Default=2501, 有効範囲 2501..=2550 → offset 0..=49
    let mut blend_offsets = BTreeSet::new();
    let mut blend_zw_offsets = BTreeSet::new();

    for (i, mat) in materials.iter().enumerate() {
        if let Some(v0_prop) = v0_mat_props.get(i) {
            if let Some(rq) = v0_prop.render_queue {
                match mat.alpha_mode {
                    AlphaMode::Blend => {
                        if (2951..=3000).contains(&rq) {
                            blend_offsets.insert(rq - 3000);
                        }
                    }
                    AlphaMode::BlendWithZWrite => {
                        if (2501..=2550).contains(&rq) {
                            blend_zw_offsets.insert(rq - 2501);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Blend: 降順 (大きい offset → 小さい offset) で 0, -1, -2, ...
    let blend_map: HashMap<i32, i32> = blend_offsets
        .iter()
        .rev()
        .enumerate()
        .map(|(rank, &src)| (src, (-(rank as i32)).clamp(-9, 0)))
        .collect();

    // BlendWithZWrite: 昇順 (小さい offset → 大きい offset) で 0, 1, 2, ...
    let blend_zw_map: HashMap<i32, i32> = blend_zw_offsets
        .iter()
        .enumerate()
        .map(|(rank, &src)| (src, (rank as i32).clamp(0, 9)))
        .collect();

    // 各材質に適用（MToon 材質のみ — 非MToon に mtoon_mut() を呼ぶと誤って MToon 化される）
    for (i, mat) in materials.iter_mut().enumerate() {
        if let Some(v0_prop) = v0_mat_props.get(i) {
            if let Some(rq) = v0_prop.render_queue {
                if let Some(ref mut mtoon) = mat.mtoon {
                    mtoon.render_queue_offset = match mat.alpha_mode {
                        AlphaMode::Blend => {
                            if (2951..=3000).contains(&rq) {
                                *blend_map.get(&(rq - 3000)).unwrap_or(&0)
                            } else {
                                0
                            }
                        }
                        AlphaMode::BlendWithZWrite => {
                            if (2501..=2550).contains(&rq) {
                                *blend_zw_map.get(&(rq - 2501)).unwrap_or(&0)
                            } else {
                                0
                            }
                        }
                        _ => 0,
                    };
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vrm::types_v0::VrmMaterialProperty;

    /// テスト用ヘルパー: 指定された alpha_mode と render_queue の材質・v0_prop ペアを生成
    /// render_queue_offset は MToon 材質にのみ設定されるため、mtoon を初期化する
    fn make_test_data(
        entries: &[(AlphaMode, Option<i32>)],
    ) -> (Vec<IrMaterial>, Vec<VrmMaterialProperty>) {
        let mut mats = Vec::new();
        let mut props = Vec::new();
        for (alpha, rq) in entries {
            let mut m = IrMaterial::default();
            m.alpha_mode = alpha.clone();
            m.mtoon = Some(MtoonParams::default());
            mats.push(m);
            props.push(VrmMaterialProperty {
                name: String::new(),
                shader: String::new(),
                render_queue: *rq,
                float_properties: None,
                vector_properties: None,
                texture_properties: None,
                keyword_map: None,
                tag_map: None,
            });
        }
        (mats, props)
    }

    #[test]
    fn rank_compress_blend_single() {
        // renderQueue=3000 (offset=0) が1つだけ → rank 0 → output 0
        let (mut mats, props) = make_test_data(&[(AlphaMode::Blend, Some(3000))]);
        remap_vrm0_render_queue_offsets(&mut mats, &props);
        assert_eq!(mats[0].mtoon().render_queue_offset, 0);
    }

    #[test]
    fn rank_compress_blend_multiple() {
        // renderQueue: 3000, 2998, 2995 → source offset: 0, -2, -5
        // 降順 (0, -2, -5) → rank 圧縮 (0, -1, -2)
        let (mut mats, props) = make_test_data(&[
            (AlphaMode::Blend, Some(3000)),
            (AlphaMode::Blend, Some(2998)),
            (AlphaMode::Blend, Some(2995)),
        ]);
        remap_vrm0_render_queue_offsets(&mut mats, &props);
        assert_eq!(mats[0].mtoon().render_queue_offset, 0);
        assert_eq!(mats[1].mtoon().render_queue_offset, -1);
        assert_eq!(mats[2].mtoon().render_queue_offset, -2);
    }

    #[test]
    fn rank_compress_blend_same_queue() {
        // 同一 renderQueue は同じ offset にマッピング
        let (mut mats, props) = make_test_data(&[
            (AlphaMode::Blend, Some(2995)),
            (AlphaMode::Blend, Some(3000)),
            (AlphaMode::Blend, Some(2995)),
        ]);
        remap_vrm0_render_queue_offsets(&mut mats, &props);
        // source offsets: {-5, 0} → 降順 (0, -5) → rank (0, -1)
        assert_eq!(mats[0].mtoon().render_queue_offset, -1); // rq=2995 → offset=-5 → rank=-1
        assert_eq!(mats[1].mtoon().render_queue_offset, 0); // rq=3000 → offset=0 → rank=0
        assert_eq!(mats[2].mtoon().render_queue_offset, -1); // rq=2995 → offset=-5 → rank=-1
    }

    #[test]
    fn rank_compress_blend_out_of_range() {
        // 範囲外の renderQueue → 0
        let (mut mats, props) = make_test_data(&[
            (AlphaMode::Blend, Some(2950)),
            (AlphaMode::Blend, Some(3001)),
            (AlphaMode::Blend, Some(2000)),
        ]);
        remap_vrm0_render_queue_offsets(&mut mats, &props);
        assert_eq!(mats[0].mtoon().render_queue_offset, 0);
        assert_eq!(mats[1].mtoon().render_queue_offset, 0);
        assert_eq!(mats[2].mtoon().render_queue_offset, 0);
    }

    #[test]
    fn rank_compress_blend_clamp_at_minus9() {
        // 10種類以上の distinct offset → 10番目以降は -9 にクランプ
        let queues: Vec<(AlphaMode, Option<i32>)> = (0..11)
            .map(|i| (AlphaMode::Blend, Some(3000 - i)))
            .collect();
        let (mut mats, props) = make_test_data(&queues);
        remap_vrm0_render_queue_offsets(&mut mats, &props);
        // rank: 0, -1, -2, ..., -9, -9 (clamp)
        for i in 0..10 {
            assert_eq!(mats[i].mtoon().render_queue_offset, -(i as i32));
        }
        assert_eq!(mats[10].mtoon().render_queue_offset, -9); // clamped
    }

    #[test]
    fn rank_compress_blend_with_zwrite_multiple() {
        // renderQueue: 2501, 2505, 2510 → source offset: 0, 4, 9
        // 昇順 (0, 4, 9) → rank 圧縮 (0, 1, 2)
        let (mut mats, props) = make_test_data(&[
            (AlphaMode::BlendWithZWrite, Some(2501)),
            (AlphaMode::BlendWithZWrite, Some(2505)),
            (AlphaMode::BlendWithZWrite, Some(2510)),
        ]);
        remap_vrm0_render_queue_offsets(&mut mats, &props);
        assert_eq!(mats[0].mtoon().render_queue_offset, 0);
        assert_eq!(mats[1].mtoon().render_queue_offset, 1);
        assert_eq!(mats[2].mtoon().render_queue_offset, 2);
    }

    #[test]
    fn rank_compress_blend_with_zwrite_same_queue() {
        // 同一 renderQueue は同じ offset にマッピング
        let (mut mats, props) = make_test_data(&[
            (AlphaMode::BlendWithZWrite, Some(2510)),
            (AlphaMode::BlendWithZWrite, Some(2501)),
            (AlphaMode::BlendWithZWrite, Some(2510)),
        ]);
        remap_vrm0_render_queue_offsets(&mut mats, &props);
        assert_eq!(mats[0].mtoon().render_queue_offset, 1); // rq=2510 → offset=9 → rank=1
        assert_eq!(mats[1].mtoon().render_queue_offset, 0); // rq=2501 → offset=0 → rank=0
        assert_eq!(mats[2].mtoon().render_queue_offset, 1);
    }

    #[test]
    fn rank_compress_blend_with_zwrite_out_of_range() {
        // 範囲外 → 0
        let (mut mats, props) = make_test_data(&[
            (AlphaMode::BlendWithZWrite, Some(2500)),
            (AlphaMode::BlendWithZWrite, Some(2551)),
            (AlphaMode::BlendWithZWrite, Some(2600)),
        ]);
        remap_vrm0_render_queue_offsets(&mut mats, &props);
        assert_eq!(mats[0].mtoon().render_queue_offset, 0);
        assert_eq!(mats[1].mtoon().render_queue_offset, 0);
        assert_eq!(mats[2].mtoon().render_queue_offset, 0);
    }

    #[test]
    fn rank_compress_blend_with_zwrite_clamp_at_9() {
        // 10種類以上 → 10番目以降は 9 にクランプ
        let queues: Vec<(AlphaMode, Option<i32>)> = (0..11)
            .map(|i| (AlphaMode::BlendWithZWrite, Some(2501 + i)))
            .collect();
        let (mut mats, props) = make_test_data(&queues);
        remap_vrm0_render_queue_offsets(&mut mats, &props);
        for i in 0..10 {
            assert_eq!(mats[i].mtoon().render_queue_offset, i as i32);
        }
        assert_eq!(mats[10].mtoon().render_queue_offset, 9); // clamped
    }

    #[test]
    fn rank_compress_opaque_always_zero() {
        let (mut mats, props) = make_test_data(&[
            (AlphaMode::Opaque, Some(3000)),
            (AlphaMode::Mask, Some(2501)),
        ]);
        remap_vrm0_render_queue_offsets(&mut mats, &props);
        assert_eq!(mats[0].mtoon().render_queue_offset, 0);
        assert_eq!(mats[1].mtoon().render_queue_offset, 0);
    }

    #[test]
    fn rank_compress_mixed_modes() {
        // Blend と BlendWithZWrite が混在: 各集合は独立に rank 圧縮される
        let (mut mats, props) = make_test_data(&[
            (AlphaMode::Blend, Some(2998)),
            (AlphaMode::BlendWithZWrite, Some(2505)),
            (AlphaMode::Blend, Some(3000)),
            (AlphaMode::BlendWithZWrite, Some(2501)),
            (AlphaMode::Opaque, Some(2000)),
        ]);
        remap_vrm0_render_queue_offsets(&mut mats, &props);
        // Blend offsets: {-2, 0} → 降順 (0, -2) → rank (0, -1)
        assert_eq!(mats[0].mtoon().render_queue_offset, -1); // rq=2998
        assert_eq!(mats[2].mtoon().render_queue_offset, 0); // rq=3000
                                                            // BlendWithZWrite offsets: {0, 4} → 昇順 (0, 4) → rank (0, 1)
        assert_eq!(mats[3].mtoon().render_queue_offset, 0); // rq=2501
        assert_eq!(mats[1].mtoon().render_queue_offset, 1); // rq=2505
                                                            // Opaque
        assert_eq!(mats[4].mtoon().render_queue_offset, 0);
    }

    #[test]
    fn rank_compress_no_render_queue() {
        // render_queue が None → offset 変更なし（デフォルト 0 のまま）
        let (mut mats, props) = make_test_data(&[(AlphaMode::Blend, None)]);
        remap_vrm0_render_queue_offsets(&mut mats, &props);
        assert_eq!(mats[0].mtoon().render_queue_offset, 0);
    }
}
