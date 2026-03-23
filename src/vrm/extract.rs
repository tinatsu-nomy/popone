use anyhow::Result;
use glam::{Mat3, Mat4, Vec2, Vec3, Vec4};
use gltf::buffer::Data;
use serde_json::Value;
use std::collections::HashMap;

use crate::convert::coord::PMX_SCALE;
use crate::intermediate::types::*;

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

        ir_mat.is_double_sided = mat.double_sided();

        // VRM 0.0 マテリアルプロパティ
        if let Some(v0_prop) = v0_mat_props.get(i) {
            ir_mat.is_mtoon = v0_prop.shader.contains("MToon");

            if ir_mat.is_mtoon {
                // _OutlineWidthMode: 0=None, 1=WorldCoordinates, 2=ScreenCoordinates
                let outline_mode = v0_prop
                    .float_properties
                    .as_ref()
                    .and_then(|fp| fp.get("_OutlineWidthMode"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as i32;

                if outline_mode != 0 {
                    if let Some(vec_props) = &v0_prop.vector_properties {
                        if let Some(outline_color) = vec_props.get("_OutlineColor") {
                            if let Some(arr) = outline_color.as_array() {
                                let r = arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                                let g = arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                                let b = arr.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                                let a = arr.get(3).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                                ir_mat.edge_color = Vec4::new(r, g, b, a);
                            }
                        }
                    }
                    if let Some(float_props) = &v0_prop.float_properties {
                        if let Some(width) = float_props.get("_OutlineWidth") {
                            let w = width.as_f64().unwrap_or(0.0) as f32;
                            // MToonシェーダーは WorldCoordinates で ×0.01 係数を適用するため
                            // VRM 1.0 outlineWidthFactor と同等にするには w * 0.01 が実効幅(メートル)
                            ir_mat.edge_size = match outline_mode {
                                1 => w * 0.01 * PMX_SCALE * 10.0, // WorldCoordinates
                                2 => w * 100.0,                   // ScreenCoordinates
                                _ => 0.0,
                            };
                        }
                    }
                    // _OutlineWidthTexture (glTFテクスチャIndex)
                    if let Some(tex_props) = &v0_prop.texture_properties {
                        if let Some(tex_idx) = tex_props.get("_OutlineWidthTexture") {
                            if let Some(idx) = tex_idx.as_u64() {
                                ir_mat.outline_width_texture_index = Some(idx as usize);
                            }
                        }
                    }
                }

                log::debug!("材質[{}] \"{}\" is_mtoon=true, outline_mode={}, edge_size={:.3}, edge_color=({:.2},{:.2},{:.2},{:.2})",
                    i, ir_mat.name, outline_mode, ir_mat.edge_size,
                    ir_mat.edge_color.x, ir_mat.edge_color.y, ir_mat.edge_color.z, ir_mat.edge_color.w);

                // _ShadeColor（outline_mode に関係なく適用）
                if let Some(vec_props) = &v0_prop.vector_properties {
                    if let Some(shade_color) = vec_props.get("_ShadeColor") {
                        if let Some(arr) = shade_color.as_array() {
                            let r = arr.first().and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                            let g = arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                            let b = arr.get(2).and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                            ir_mat.shade_color = Some(Vec3::new(r, g, b));
                        }
                    }
                }
            }
        }

        // VRM 1.0 MToon拡張からアウトライン情報を抽出
        if *version == VrmVersion::V1 {
            let json = document.as_json();
            if let Some(mat_json) = json.materials.get(i) {
                if let Some(exts) = &mat_json.extensions {
                    if let Some(mtoon) = exts.others.get("VRMC_materials_mtoon") {
                        ir_mat.is_mtoon = true;

                        // outlineWidthMode が "none" 以外ならエッジ有効
                        let mode = mtoon
                            .get("outlineWidthMode")
                            .and_then(|v| v.as_str())
                            .unwrap_or("none");

                        if mode != "none" {
                            let width = mtoon
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

                            // outlineColorFactor [r,g,b] → Vec4(r,g,b,1.0)
                            if let Some(color) = mtoon.get("outlineColorFactor") {
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
                            if let Some(wtex) = mtoon.get("outlineWidthMultiplyTexture") {
                                if let Some(idx) = wtex.get("index").and_then(|v| v.as_u64()) {
                                    ir_mat.outline_width_texture_index = Some(idx as usize);
                                }
                            }
                        }

                        // shadeColorFactor
                        if let Some(shade) = mtoon.get("shadeColorFactor") {
                            if let Some(arr) = shade.as_array() {
                                let r = arr.first().and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                                let g = arr.get(1).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                                let b = arr.get(2).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                                ir_mat.shade_color = Some(Vec3::new(r, g, b));
                            }
                        }

                        log::debug!("材質[{}] \"{}\" is_mtoon=true, edge_size={:.3}, edge_color=({:.2},{:.2},{:.2},{:.2})",
                            i, ir_mat.name, ir_mat.edge_size,
                            ir_mat.edge_color.x, ir_mat.edge_color.y, ir_mat.edge_color.z, ir_mat.edge_color.w);
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

                // UV
                let uvs: Vec<[f32; 2]> = reader
                    .read_tex_coords(0)
                    .map(|iter| iter.into_f32().collect())
                    .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);

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

                // 頂点構築（スキニングによりT-ポーズ世界座標を計算）
                let vertices: Vec<IrVertex> = positions
                    .iter()
                    .enumerate()
                    .map(|(i, pos)| {
                        let normal = normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
                        let uv = uvs.get(i).copied().unwrap_or([0.0, 0.0]);

                        // スキニング演算でT-ポーズ頂点位置・法線を計算
                        let (final_pos, final_normal) =
                            if !skin_mats.is_empty() && !joints.is_empty() {
                                let j = joints.get(i).copied().unwrap_or([0; 4]);
                                let w = weights.get(i).copied().unwrap_or([0.0; 4]);
                                let lp = Vec4::new(pos[0], pos[1], pos[2], 1.0);
                                let ln = Vec3::new(normal[0], normal[1], normal[2]);
                                let mut wp = Vec4::ZERO;
                                let mut wn = Vec3::ZERO;
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
                                    }
                                }
                                let fp = Vec3::new(wp.x, wp.y, wp.z);
                                let fn3 = wn.normalize_or_zero();
                                (fp, [fn3.x, fn3.y, fn3.z])
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
                                (fp, [fn3.x, fn3.y, fn3.z])
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
                let mut vertices = vertices;
                if let Some(ir_mat) = materials.get(material_index) {
                    if let Some(tex_idx) = ir_mat.outline_width_texture_index {
                        if let Some(gltf_tex) = document.textures().nth(tex_idx) {
                            let src_idx = gltf_tex.source().index();
                            if let Some(img) = images.get(src_idx) {
                                for vtx in &mut vertices {
                                    vtx.edge_scale =
                                        sample_image_g_channel(img, vtx.uv.x, vtx.uv.y);
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
                }

                ir_meshes.push(IrMesh {
                    name: mesh
                        .name()
                        .unwrap_or(&format!("mesh_{}", mesh.index()))
                        .to_string(),
                    vertices,
                    indices,
                    material_index,
                    morph_targets,
                    node_index: node_idx,
                });
            }
        }
    }

    Ok(ir_meshes)
}

/// 画像のGチャネル値をUV座標でサンプリング（0.0〜1.0）
/// VRM 1.0 MToon仕様: outlineWidthMultiplyTexture は Gチャネルを使用
fn sample_image_g_channel(img: &gltf::image::Data, u: f32, v: f32) -> f32 {
    let w = img.width as usize;
    let h = img.height as usize;
    if w == 0 || h == 0 {
        return 1.0;
    }

    // UV → ピクセル座標（繰り返しラップ）
    let fu = u.fract();
    let fv = v.fract();
    let fu = if fu < 0.0 { fu + 1.0 } else { fu };
    let fv = if fv < 0.0 { fv + 1.0 } else { fv };
    let px = ((fu * w as f32) as usize).min(w - 1);
    let py = ((fv * h as f32) as usize).min(h - 1);

    use gltf::image::Format;
    let (bpp, g_offset) = match img.format {
        Format::R8 => (1, 0), // 単チャネル → R値を使用
        Format::R8G8 => (2, 1),
        Format::R8G8B8 => (3, 1),
        Format::R8G8B8A8 => (4, 1),
        _ => return 1.0, // 16bit/float形式は非対応
    };
    let idx = (py * w + px) * bpp + g_offset;
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

    for (i, (positions_opt, _normals_opt, _tangents_opt)) in reader.read_morph_targets().enumerate()
    {
        let positions: Vec<[f32; 3]> = positions_opt.map(|iter| iter.collect()).unwrap_or_default();

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
        targets.push(IrMorphTarget {
            name: format!("morph_{}", i),
            position_offsets,
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

        for bind in &group.binds {
            let target_mesh_idx = bind.mesh as usize;
            // document.meshes().nth(target_mesh_idx) からメッシュ名を取得してir_meshを検索
            let mesh_name = document
                .meshes()
                .nth(target_mesh_idx)
                .and_then(|m| m.name().map(|s| s.to_string()));

            // IrMeshの中から対応するものを探す
            for (ir_idx, ir_mesh) in ir_meshes.iter().enumerate() {
                // メッシュ名が一致するか、ノードのメッシュindexが一致
                let name_match = mesh_name
                    .as_deref()
                    .map(|n| n == ir_mesh.name)
                    .unwrap_or(false);
                if !name_match {
                    // 名前が違う場合はnodeのmeshインデックスで突合
                    // nodeがmeshを持ち、そのmesh.index() == target_mesh_idxの場合
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
                }
            }
        }

        if !vertex_offsets.is_empty() {
            morphs.push(IrMorph {
                name: name.clone(),
                name_en: group.preset_name.clone(),
                panel,
                kind: IrMorphKind::Vertex(vertex_offsets),
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
    let collect_offsets = |binds: &[crate::vrm::types_v1::MorphTargetBind]| -> Vec<(usize, Vec3)> {
        let mut vertex_offsets: Vec<(usize, Vec3)> = Vec::new();
        for bind in binds {
            if bind.weight == 0.0 {
                continue; // weight=0 のバインドはスキップ
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
                    }
                }
            }
        }
        vertex_offsets
    };

    let mut morphs = Vec::new();

    macro_rules! process_expr {
        ($expr_opt:expr, $preset_name:expr) => {
            if let Some(expr) = &$expr_opt {
                let (jp_name, panel) = crate::convert::morph::preset_to_jp_v1($preset_name);
                let vertex_offsets = if let Some(binds) = &expr.morph_target_binds {
                    collect_offsets(binds)
                } else {
                    Vec::new()
                };
                if !vertex_offsets.is_empty() {
                    morphs.push(IrMorph {
                        name: jp_name,
                        name_en: $preset_name.to_string(),
                        panel,
                        kind: IrMorphKind::Vertex(vertex_offsets),
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
            let vertex_offsets = if let Some(binds) = &expr.morph_target_binds {
                collect_offsets(binds)
            } else {
                Vec::new()
            };
            if !vertex_offsets.is_empty() {
                morphs.push(IrMorph {
                    name: name.clone(),
                    name_en: name.clone(),
                    panel: 4,
                    kind: IrMorphKind::Vertex(vertex_offsets),
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
