use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use glam::{Mat4, Vec2, Vec3, Vec4};

use crate::intermediate::types::*;

use super::blendshape;
use super::bone::{self, BoneHierarchy};
use super::humanoid;
use super::mesh;
use super::parser;
use super::scene::FbxScene;
use super::skin;
use super::texture;

/// FBX バイナリデータにメッシュ（Geometry）が含まれるかを高速チェック
pub fn fbx_has_mesh(data: &[u8]) -> bool {
    let Ok(doc) = parser::parse(data) else {
        return false;
    };
    let scene = FbxScene::from_document(&doc);
    !scene.geometries().is_empty()
}

/// FBX バイナリデータから IrModel を構築
pub fn extract_ir_model_from_fbx(
    data: &[u8],
    fbx_path: Option<&Path>,
) -> Result<IrModel> {
    extract_ir_model_from_fbx_with_options(data, fbx_path, false)
}

/// FBX バイナリデータから IrModel を構築（オプション付き）
pub fn extract_ir_model_from_fbx_with_options(
    data: &[u8],
    fbx_path: Option<&Path>,
    normalize_pose: bool,
) -> Result<IrModel> {
    let doc = parser::parse(data)?;
    let scene = FbxScene::from_document(&doc);

    // 座標系変換関数（FBX → glTF Y-Up 右手系）
    let coord_fn = build_coord_transform(&doc);

    // ボーン抽出
    let hierarchy = BoneHierarchy::from_scene(&scene);
    let bone_names: Vec<(usize, &str)> = hierarchy
        .bones
        .iter()
        .enumerate()
        .map(|(i, b)| (i, b.name.as_str()))
        .collect();
    let humanoid_mapping = humanoid::detect_humanoid(&bone_names);

    log::info!(
        "FBX ボーン: {} 本, リグ: {}",
        hierarchy.bones.len(),
        humanoid_mapping.rig_type.label()
    );

    // ボーン → IrBone
    let (mut ir_bones, bone_id_to_ir) = convert_bones(&hierarchy, &humanoid_mapping, &coord_fn);

    // メッシュ・材質・テクスチャ抽出
    let mut ir_textures: Vec<IrTexture> = Vec::new();
    let mut ir_materials: Vec<IrMaterial> = Vec::new();
    let mut ir_meshes: Vec<IrMesh> = Vec::new();
    let mut ir_morphs: Vec<IrMorph> = Vec::new();
    for geom_obj in scene.geometries() {
        let geom = geom_obj.node;
        let geom_id = geom_obj.id;

        let Some(positions) = mesh::extract_vertices(geom) else {
            continue;
        };
        let Some(poly_indices) = mesh::extract_polygon_indices(geom) else {
            continue;
        };
        let normals = mesh::extract_normals(geom);
        let normal_indices = mesh::extract_normal_indices(geom);
        let mat_per_polygon = mesh::extract_material_indices(geom);
        let (uvs, uv_indices, uv_mapping) = mesh::extract_uvs(geom);

        // 親 Model のワールド変換（PreRotation 含む）
        let model_transform = compute_geometry_world_transform(&scene, geom_id);
        let has_model_transform = model_transform != Mat4::IDENTITY;
        // 法線用: 逆転置行列（上位3x3）
        let normal_matrix = if has_model_transform {
            Mat4::from_mat3(glam::Mat3::from_mat4(model_transform).inverse().transpose())
        } else {
            Mat4::IDENTITY
        };

        // 材質
        let geom_materials = scene.materials_for_geometry(geom_id);
        let mat_base = ir_materials.len();

        for mat_obj in &geom_materials {
            let diffuse = mesh::extract_diffuse_color(mat_obj.node);
            let props = mesh::extract_material_props(mat_obj.node);

            let tex_idx = texture::extract_texture_for_material(&scene, mat_obj.id, fbx_path)
                .and_then(|tex| texture_to_ir(&tex, &mut ir_textures));
            let source_tex_name = texture::extract_texture_name_for_material(&scene, mat_obj.id);

            // PMX 材質パラメータ
            let opacity = props.opacity.min(1.0);
            let d = Vec3::new(diffuse[0], diffuse[1], diffuse[2]);
            let mut mat = IrMaterial {
                name: mat_obj.name.clone(),
                diffuse: Vec4::new(d.x, d.y, d.z, opacity),
                ambient: d * 0.5,
                texture_index: tex_idx,
                source_texture_name: source_tex_name,
                ..Default::default()
            };
            if tex_idx.is_some() {
                mat.apply_textured_defaults();
            }
            ir_materials.push(mat);
        }

        // デフォルト材質
        if geom_materials.is_empty() {
            ir_materials.push(IrMaterial {
                name: "Default".to_string(),
                ..IrMaterial::default()
            });
        }

        // 頂点展開
        let mut vert_positions: Vec<[f32; 3]> = Vec::with_capacity(poly_indices.len());
        let mut vert_normals: Vec<[f32; 3]> = Vec::with_capacity(poly_indices.len());
        let mut vert_uvs: Vec<[f32; 2]> = Vec::with_capacity(poly_indices.len());
        let mut cp_to_verts: HashMap<usize, Vec<u32>> = HashMap::new();

        for (poly_vert_idx, &idx) in poly_indices.iter().enumerate() {
            let actual_idx = if idx < 0 { -(idx + 1) } else { idx } as usize;
            let pos = positions
                .get3(actual_idx)
                .map(|p| {
                    if has_model_transform {
                        let transformed = model_transform.transform_point3(Vec3::from(p));
                        coord_fn(transformed.to_array())
                    } else {
                        coord_fn(p)
                    }
                })
                .unwrap_or([0.0; 3]);

            let raw_normal = mesh::get_normal(
                normals.as_ref(),
                &normal_indices,
                poly_vert_idx,
            );
            let normal = if has_model_transform {
                let n = normal_matrix.transform_vector3(Vec3::from(raw_normal)).normalize_or_zero();
                coord_fn(n.to_array())
            } else {
                coord_fn(raw_normal)
            };

            let uv = mesh::get_uv(
                uvs.as_ref(),
                &uv_indices,
                &uv_mapping,
                poly_vert_idx,
                actual_idx,
            );

            cp_to_verts
                .entry(actual_idx)
                .or_default()
                .push(poly_vert_idx as u32);

            vert_positions.push(pos);
            vert_normals.push(normal);
            vert_uvs.push(uv);
        }

        // 三角化（材質ごと）
        let num_geom_mats = geom_materials.len().max(1);
        let mut mat_triangles: Vec<Vec<[u32; 3]>> = vec![Vec::new(); num_geom_mats];

        let mut polygon_start = 0usize;
        let mut polygon_idx = 0usize;
        for (i, &idx) in poly_indices.iter().enumerate() {
            if idx < 0 {
                let mat_local = mat_per_polygon.get(polygon_idx).copied().unwrap_or(0) as usize;
                let mat_local = mat_local.min(num_geom_mats - 1);
                let polygon_len = i - polygon_start + 1;
                for j in 1..polygon_len - 1 {
                    // coord_fn det=+1 なので面巻き順は通常の fan 順序
                    // gltf_pos_to_pmx (det=-1) 用の反転は mesh.rs の flip_face_winding で処理
                    let tri = [
                        polygon_start as u32,
                        (polygon_start + j) as u32,
                        (polygon_start + j + 1) as u32,
                    ];
                    mat_triangles[mat_local].push(tri);
                }
                polygon_start = i + 1;
                polygon_idx += 1;
            }
        }

        // フラットシェーディングフォールバック（ゼロ法線がある面をフラット法線で補完）
        if vert_normals.iter().any(|n| *n == [0.0f32; 3]) {
            let all_indices: Vec<u32> = mat_triangles
                .iter()
                .flatten()
                .flat_map(|t| t.iter().copied())
                .collect();
            mesh::fill_missing_normals(&vert_positions, &mut vert_normals, &all_indices);
        }

        // スキンウェイト
        let skin_weights = skin::extract_skin(&scene, geom_id).map(|skin_data| {
            mesh::build_vertex_weights(
                &skin_data,
                &bone_id_to_ir,
                &cp_to_verts,
                vert_positions.len(),
            )
        });

        // IrMesh を材質ごとに生成
        // ジオメトリローカル展開インデックス → グローバル IrMesh 頂点インデックスのマッピング
        let mut geom_local_to_global: HashMap<u32, usize> = HashMap::new();
        let mut global_vertex_offset: usize = ir_meshes.iter().map(|m| m.vertices.len()).sum();

        for (mat_local, triangles) in mat_triangles.iter().enumerate() {
            if triangles.is_empty() {
                continue;
            }

            // この材質で使われる頂点を収集
            let mut used_verts: Vec<u32> = triangles
                .iter()
                .flat_map(|t| t.iter().copied())
                .collect();
            used_verts.sort_unstable();
            used_verts.dedup();
            let mut old_to_new: HashMap<u32, u32> = HashMap::new();
            let mut ir_vertices = Vec::with_capacity(used_verts.len());
            for &old_idx in &used_verts {
                let new_idx = ir_vertices.len() as u32;
                old_to_new.insert(old_idx, new_idx);
                // グローバルマッピングを記録
                geom_local_to_global.insert(old_idx, global_vertex_offset + new_idx as usize);
                let i = old_idx as usize;
                ir_vertices.push(IrVertex {
                    position: Vec3::from(vert_positions[i]),
                    normal: Vec3::from(vert_normals[i]),
                    uv: Vec2::from(vert_uvs[i]),
                    weights: skin_weights
                        .as_ref()
                        .map(|w| w[i].clone())
                        .unwrap_or_default(),
                    edge_scale: 1.0,
                });
            }

            let indices: Vec<u32> = triangles
                .iter()
                .flat_map(|t| t.iter().map(|&idx| old_to_new[&idx]))
                .collect();

            let mat_index = mat_base + mat_local;
            global_vertex_offset += ir_vertices.len();

            ir_meshes.push(IrMesh {
                name: geom_obj.name.clone(),
                vertices: ir_vertices,
                indices,
                material_index: mat_index,
                morph_targets: Vec::new(),
                node_index: 0,
            });
        }

        // ブレンドシェイプ
        let raw_shapes = blendshape::extract_blend_shapes_raw(&scene, geom_id);
        let model_xform = model_transform; // capture for closure
        for shape in blendshape::expand_blend_shapes(raw_shapes, &cp_to_verts, |v| {
            if has_model_transform {
                let transformed = model_xform.transform_vector3(Vec3::from(v));
                coord_fn(transformed.to_array())
            } else {
                coord_fn(v)
            }
        }) {
            let deltas: Vec<(usize, Vec3)> = shape
                .deltas
                .iter()
                .filter_map(|&(vi, offset)| {
                    // ジオメトリローカルインデックス → グローバル IrMesh 頂点インデックスに変換
                    geom_local_to_global.get(&vi).map(|&global_vi| {
                        (global_vi, Vec3::from(offset))
                    })
                })
                .collect();
            if !deltas.is_empty() {
                ir_morphs.push(IrMorph {
                    name: shape.name.clone(),
                    name_en: shape.name.clone(),
                    panel: 4,
                    kind: IrMorphKind::Vertex(deltas),
                });
            }
        }

    }

    // T→Aスタンス変換（オプション）
    if normalize_pose {
        let mut global_mats: Vec<Mat4> = ir_bones.iter().map(|b| b.global_mat).collect();
        crate::intermediate::pose::normalize_pose_to_astance_with_meshes(
            &mut ir_bones, &mut global_mats, &mut ir_meshes, &mut ir_morphs,
        );
    }

    // モデル名
    let model_name = fbx_path
        .and_then(|p| p.file_stem())
        .and_then(|s| s.to_str())
        .unwrap_or("FBX Model")
        .to_string();

    log::info!(
        "FBX 抽出完了: ボーン={}, メッシュ={}, 材質={}, テクスチャ={}, モーフ={}",
        ir_bones.len(),
        ir_meshes.len(),
        ir_materials.len(),
        ir_textures.len(),
        ir_morphs.len(),
    );

    Ok(IrModel {
        name: model_name,
        comment: String::new(),
        bones: ir_bones,
        meshes: ir_meshes,
        materials: ir_materials,
        textures: ir_textures,
        morphs: ir_morphs,
        physics: IrPhysics::default(),
        node_to_bone: HashMap::new(),
        source_format: SourceFormat::Fbx,
        rig_type: Some(humanoid_mapping.rig_type.label().to_string()),
        humanoid_bone_count: humanoid_mapping.mapping.len(),
    })
}

/// FBX GlobalSettings から座標系変換関数を構築
/// UnitScaleFactor も読み取りメートル単位に正規化
fn build_coord_transform(
    doc: &parser::FbxDocument,
) -> impl Fn([f32; 3]) -> [f32; 3] {
    let mut up_axis = 1i32; // デフォルト Y-Up
    let mut up_sign = 1i32;
    let mut front_axis = 2i32; // デフォルト Z
    let mut front_sign = 1i32;
    let mut coord_axis = 0i32; // デフォルト X
    let mut coord_sign = 1i32;
    let mut unit_scale_factor = 1.0f64; // デフォルト cm

    if let Some(settings) = doc.nodes.iter().find(|n| n.name == "GlobalSettings") {
        if let Some(props) = settings.child("Properties70") {
            for p in &props.children {
                if p.name != "P" {
                    continue;
                }
                let name = p.properties.first().and_then(|v| v.as_string()).unwrap_or("");
                match name {
                    "UnitScaleFactor" => {
                        unit_scale_factor = p.properties.get(4)
                            .and_then(|v| v.as_f64_value())
                            .unwrap_or(1.0);
                    }
                    _ => {
                        let val = p.properties.get(4).and_then(|v| v.as_i64_value()).unwrap_or(0) as i32;
                        match name {
                            "UpAxis" => up_axis = val,
                            "UpAxisSign" => up_sign = val,
                            "FrontAxis" => front_axis = val,
                            "FrontAxisSign" => front_sign = val,
                            "CoordAxis" => coord_axis = val,
                            "CoordAxisSign" => coord_sign = val,
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    // UnitScaleFactor → メートル変換
    // FBX の UnitScaleFactor: 1.0 = 1cm, 100.0 = 1m
    // glTF/IrModel はメートル単位
    let to_meters = (unit_scale_factor / 100.0) as f32;

    log::info!(
        "FBX 座標系: UpAxis={} (sign={}), FrontAxis={} (sign={}), CoordAxis={} (sign={}), UnitScale={}(→×{}m)",
        up_axis, up_sign, front_axis, front_sign, coord_axis, coord_sign,
        unit_scale_factor, to_meters
    );

    // FBX FrontAxis はシーンの奥行き軸を定義するが、
    // キャラクターは通常その逆（カメラ側）を向く。
    // Z 反転なしで軸を再マッピングすると、キャラクターは
    // glTF の -Z forward 方向を自然に向く。
    move |v: [f32; 3]| -> [f32; 3] {
        [
            v[coord_axis as usize] * coord_sign as f32 * to_meters,
            v[up_axis as usize] * up_sign as f32 * to_meters,
            v[front_axis as usize] * front_sign as f32 * to_meters,
        ]
    }
}

/// BoneHierarchy → IrBone 配列
fn convert_bones(
    hierarchy: &BoneHierarchy,
    humanoid_mapping: &humanoid::HumanoidMapping,
    coord_fn: &impl Fn([f32; 3]) -> [f32; 3],
) -> (Vec<IrBone>, HashMap<i64, usize>) {
    let mut ir_bones = Vec::with_capacity(hierarchy.bones.len());
    let mut bone_id_to_ir: HashMap<i64, usize> = HashMap::new();

    for (i, bone) in hierarchy.bones.iter().enumerate() {
        let world_pos = bone.world_transform.col(3);
        let pos = coord_fn([world_pos.x, world_pos.y, world_pos.z]);

        let vrm_name = humanoid_mapping
            .mapping
            .get(&i)
            .map(|hb| hb.as_vrm_name().to_string());

        // ボーン名: humanoid 検出済みなら bone_map で PMX 名取得
        let (name, name_en) = if let Some(ref vrm) = vrm_name {
            if let Some((jp, en)) = crate::convert::bone_map::vrm_bone_to_pmx_name(vrm) {
                (jp.to_string(), en.to_string())
            } else {
                (bone.name.clone(), bone.name.clone())
            }
        } else {
            (bone.name.clone(), bone.name.clone())
        };

        bone_id_to_ir.insert(bone.id, i);

        ir_bones.push(IrBone {
            name,
            name_en,
            vrm_bone_name: vrm_name,
            position: Vec3::from(pos),
            global_mat: convert_mat4(bone.world_transform, coord_fn),
            parent: bone.parent_index,
            children: bone.children_indices.clone(),
            node_index: i,
            is_physics: false,
        });
    }

    (ir_bones, bone_id_to_ir)
}

/// Mat4 の座標系変換（位置列のみ変換、回転は近似的に処理）
fn convert_mat4(m: Mat4, coord_fn: &impl Fn([f32; 3]) -> [f32; 3]) -> Mat4 {
    let pos = m.col(3);
    let new_pos = coord_fn([pos.x, pos.y, pos.z]);
    Mat4::from_cols(
        m.col(0),
        m.col(1),
        m.col(2),
        Vec4::new(new_pos[0], new_pos[1], new_pos[2], 1.0).into(),
    )
}

/// FBX TextureData → IrTexture（PNG エンコード）
fn texture_to_ir(
    tex: &texture::TextureData,
    ir_textures: &mut Vec<IrTexture>,
) -> Option<usize> {
    // RGBA → PNG エンコード
    let mut png_data = Vec::new();
    {
        let encoder = image::codecs::png::PngEncoder::new(&mut png_data);
        use image::ImageEncoder;
        if encoder
            .write_image(&tex.rgba, tex.width, tex.height, image::ExtendedColorType::Rgba8)
            .is_err()
        {
            log::warn!("テクスチャ '{}' の PNG エンコード失敗", tex.name);
            return None;
        }
    }

    let idx = ir_textures.len();
    ir_textures.push(IrTexture {
        filename: format!("{}.png", sanitize_filename(&tex.name)),
        data: png_data,
        mime_type: "image/png".to_string(),
    });
    Some(idx)
}

/// Geometry の親 Model ノードのワールド変換を計算
/// Model 階層を辿り、各ノードの PreRotation * Rotation を含むローカル変換を累積
fn compute_geometry_world_transform(scene: &FbxScene, geom_id: i64) -> Mat4 {
    // Geometry → 親 Model を探す
    let parent_model_id = scene
        .parents_of(geom_id)
        .iter()
        .find(|&&pid| {
            scene
                .objects
                .get(&pid)
                .map_or(false, |o| o.class == "Model")
        })
        .copied();

    let Some(start_id) = parent_model_id else {
        return Mat4::IDENTITY;
    };

    // Model 階層を root まで辿ってローカル変換を収集
    let mut chain = Vec::new();
    let mut current_id = start_id;
    loop {
        if let Some(obj) = scene.objects.get(&current_id) {
            if obj.class == "Model" {
                let local = compute_model_local_transform(obj.node);
                chain.push(local);
            }
        }
        // 親 Model を探す
        let parent = scene
            .parents_of(current_id)
            .iter()
            .find(|&&pid| {
                scene
                    .objects
                    .get(&pid)
                    .map_or(false, |o| o.class == "Model")
            })
            .copied();
        match parent {
            Some(pid) => current_id = pid,
            None => break,
        }
    }

    // root → leaf の順で累積
    let mut world = Mat4::IDENTITY;
    for local in chain.into_iter().rev() {
        world = world * local;
    }
    world
}

/// Model ノードのローカル変換行列（T * PreRotation * Rotation * S）
fn compute_model_local_transform(node: &parser::FbxNode) -> Mat4 {
    let (translation, rotation_euler, pre_rotation_euler, scale) =
        bone::extract_transform(node);
    let pre_rot = bone::euler_deg_to_quat(pre_rotation_euler);
    let rot = bone::euler_deg_to_quat(rotation_euler);
    let combined = pre_rot * rot;
    Mat4::from_scale_rotation_translation(scale, combined, translation)
}

fn sanitize_filename(name: &str) -> String {
    name.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
}
