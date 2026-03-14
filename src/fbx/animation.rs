use anyhow::{Context, Result};
use glam::{Mat3, Quat, Vec3};
use std::collections::HashMap;
use std::path::Path;

use super::bone::euler_deg_to_quat;
use super::humanoid::detect_humanoid;
use super::parser::FbxDocument;
use super::scene::{ConnectionType, FbxScene};
use crate::intermediate::animation::*;

/// FBX time unit: 1秒 = 46186158000 units
const FBX_TIME_UNIT: f64 = 46186158000.0;

/// FBX座標系情報
struct FbxAxisConfig {
    /// 軸変換行列（3x3、スケールなし）
    axis_mat: Mat3,
    /// 軸変換がidentityかどうか（Y-Up標準の場合true、mat3往復を省略）
    is_identity: bool,
    /// メートルへの変換スケール
    to_meters: f32,
}

/// FBXファイルからアニメーションを読み込む
pub fn load_fbx_animation(path: &Path) -> Result<Vec<VrmaAnimation>> {
    let data = std::fs::read(path)
        .with_context(|| format!("FBXファイルの読み込みに失敗: {}", path.display()))?;
    load_fbx_animation_from_data(&data)
}

/// FBXバイナリデータからアニメーションを読み込む
pub fn load_fbx_animation_from_data(data: &[u8]) -> Result<Vec<VrmaAnimation>> {
    let doc = super::parser::parse(data)
        .with_context(|| "FBXパースに失敗")?;
    let scene = FbxScene::from_document(&doc);
    let axis_config = read_axis_config(&doc);

    extract_animations(&scene, &axis_config)
}

/// FBX GlobalSettings から座標系情報を読み取る
fn read_axis_config(doc: &FbxDocument) -> FbxAxisConfig {
    let mut up_axis = 1i32;
    let mut up_sign = 1i32;
    let mut front_axis = 2i32;
    let mut front_sign = 1i32;
    let mut coord_axis = 0i32;
    let mut coord_sign = 1i32;
    let mut unit_scale_factor = 1.0f64;

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

    let to_meters = (unit_scale_factor / 100.0) as f32;

    log::info!(
        "FBXアニメーション座標系: UpAxis={} (sign={}), FrontAxis={} (sign={}), CoordAxis={} (sign={}), UnitScale={}(→×{}m)",
        up_axis, up_sign, front_axis, front_sign, coord_axis, coord_sign,
        unit_scale_factor, to_meters
    );

    // 軸変換行列を構築（FBX空間 → glTF Y-Up空間）
    let mut m = [[0.0f32; 3]; 3];
    m[0][coord_axis as usize] = coord_sign as f32;
    m[1][up_axis as usize] = up_sign as f32;
    m[2][front_axis as usize] = front_sign as f32;

    let axis_mat = Mat3::from_cols_array_2d(&m).transpose();
    let is_identity = (axis_mat - Mat3::IDENTITY).to_cols_array().iter().all(|&v| v.abs() < 1e-6);

    if is_identity {
        log::info!("FBX座標系: Y-Up標準（軸変換なし）");
    }

    FbxAxisConfig { axis_mat, is_identity, to_meters }
}

/// 軸変換行列でクォータニオンを変換
/// R_gltf = M * R_fbx * M^T（identityの場合はそのまま返す）
fn transform_quat(q: Quat, config: &FbxAxisConfig) -> Quat {
    if config.is_identity {
        return q;
    }
    let r = Mat3::from_quat(q);
    let r_new = config.axis_mat * r * config.axis_mat.transpose();
    Quat::from_mat3(&r_new).normalize()
}

/// 軸変換行列で位置を変換（スケール付き）
fn transform_pos(v: Vec3, config: &FbxAxisConfig) -> Vec3 {
    if config.is_identity {
        return v * config.to_meters;
    }
    config.axis_mat * v * config.to_meters
}

/// FBXシーンからアニメーションデータを抽出
fn extract_animations(scene: &FbxScene, axis_config: &FbxAxisConfig) -> Result<Vec<VrmaAnimation>> {
    let mut animations = Vec::new();

    // AnimationStack を列挙
    let anim_stacks: Vec<i64> = scene.objects.iter()
        .filter(|(_, obj)| obj.class == "AnimationStack")
        .map(|(&id, _)| id)
        .collect();

    if anim_stacks.is_empty() {
        anyhow::bail!("FBXにアニメーションが含まれていません");
    }

    // 各ボーンの PreRotation を収集（アニメーション回転にPreRotationを適用するため）
    let bone_pre_rotations = collect_bone_pre_rotations(scene);

    // ボーン（Model）のレストポーズを収集（座標変換済み）
    let (bone_rests, global_positions) = collect_bone_rests(scene, axis_config);

    // 向き検出用: Leftボーンのグローバル位置X平均
    let facing_flip_y = detect_facing(&global_positions);

    for &stack_id in &anim_stacks {
        let stack_name = scene.objects.get(&stack_id)
            .map(|o| o.name.clone())
            .unwrap_or_else(|| "animation".to_string());

        // AnimationStack → AnimationLayer
        let layer_ids: Vec<i64> = scene.children_of(stack_id).iter()
            .filter(|&&id| scene.objects.get(&id).map_or(false, |o| o.class == "AnimationLayer"))
            .copied()
            .collect();

        let mut bone_channels: HashMap<String, BoneChannel> = HashMap::new();
        let expression_channels: HashMap<String, ExpressionChannel> = HashMap::new();
        let mut duration: f32 = 0.0;

        for &layer_id in &layer_ids {
            // AnimationLayer → AnimationCurveNode
            let curve_node_ids: Vec<i64> = scene.children_of(layer_id).iter()
                .filter(|&&id| scene.objects.get(&id).map_or(false, |o| o.class == "AnimationCurveNode"))
                .copied()
                .collect();

            for &cn_id in &curve_node_ids {
                let cn_obj = match scene.objects.get(&cn_id) {
                    Some(o) => o,
                    None => continue,
                };

                // CurveNodeの接続先（OP接続）からターゲットのModel IDとプロパティを取得
                let mut target_model_id: Option<i64> = None;
                let mut target_property: Option<String> = None;

                for conn in &scene.connections {
                    if conn.child_id == cn_id && conn.conn_type == ConnectionType::OP {
                        if let Some(ref prop) = conn.property {
                            if prop.starts_with("Lcl Rotation") || prop.starts_with("Lcl Translation") {
                                target_model_id = Some(conn.parent_id);
                                target_property = Some(prop.clone());
                            }
                        }
                    }
                }

                // CurveNode名からプロパティ判定（OO接続の場合）
                let prop_type = target_property.as_deref()
                    .or_else(|| {
                        let name = &cn_obj.name;
                        if name.contains("Rotation") || name.contains("R") { Some("Lcl Rotation") }
                        else if name.contains("Translation") || name.contains("T") { Some("Lcl Translation") }
                        else { None }
                    });

                // OP接続がない場合、OO接続でターゲットModelを探す
                if target_model_id.is_none() {
                    for &parent_id in scene.parents_of(cn_id) {
                        if let Some(obj) = scene.objects.get(&parent_id) {
                            if obj.class == "Model" {
                                target_model_id = Some(parent_id);
                                break;
                            }
                        }
                    }
                }

                let model_id = match target_model_id {
                    Some(id) => id,
                    None => continue,
                };

                let bone_name = match scene.objects.get(&model_id) {
                    Some(obj) => obj.name.clone(),
                    None => continue,
                };

                // AnimationCurveNode → AnimationCurve（X, Y, Z の3カーブ）
                let curve_ids: Vec<i64> = scene.children_of(cn_id).iter()
                    .filter(|&&id| scene.objects.get(&id).map_or(false, |o| o.class == "AnimationCurve"))
                    .copied()
                    .collect();

                // 各カーブからキーフレームデータを読み出し
                let mut axis_curves: Vec<(Vec<f32>, Vec<f32>)> = Vec::new();

                // OP接続の順序でカーブを並べる（d|X, d|Y, d|Z）
                let mut ordered_curve_ids = Vec::new();
                for axis in &["d|X", "d|Y", "d|Z"] {
                    for conn in &scene.connections {
                        if conn.parent_id == cn_id && conn.conn_type == ConnectionType::OP {
                            if conn.property.as_deref() == Some(axis) {
                                ordered_curve_ids.push(conn.child_id);
                            }
                        }
                    }
                }
                // OP接続がない場合はOO接続順
                if ordered_curve_ids.is_empty() {
                    ordered_curve_ids = curve_ids;
                }

                for &curve_id in &ordered_curve_ids {
                    if let Some(obj) = scene.objects.get(&curve_id) {
                        let (times, values) = read_animation_curve(obj.node);
                        axis_curves.push((times, values));
                    }
                }

                if axis_curves.is_empty() {
                    continue;
                }

                match prop_type {
                    Some(p) if p.starts_with("Lcl Rotation") => {
                        // ボーンの PreRotation を取得（アニメーションは Lcl Rotation のみなので
                        // PreRotation を掛けてレストと同じ空間にする必要がある）
                        let pre_rot = bone_pre_rotations.get(&bone_name)
                            .copied()
                            .unwrap_or(Quat::IDENTITY);
                        let keyframes = build_rotation_keyframes(
                            &axis_curves, &mut duration, axis_config, pre_rot,
                        );
                        if !keyframes.is_empty() {
                            let entry = bone_channels.entry(bone_name).or_insert_with(|| BoneChannel {
                                rotation: Vec::new(),
                                rotation_interp: Interpolation::Linear,
                                translation: None,
                                translation_interp: None,
                            });
                            entry.rotation = keyframes;
                        }
                    }
                    Some(p) if p.starts_with("Lcl Translation") => {
                        let keyframes = build_translation_keyframes(&axis_curves, &mut duration, axis_config);
                        if !keyframes.is_empty() {
                            let entry = bone_channels.entry(bone_name).or_insert_with(|| BoneChannel {
                                rotation: Vec::new(),
                                rotation_interp: Interpolation::Linear,
                                translation: None,
                                translation_interp: None,
                            });
                            entry.translation = Some(keyframes);
                            entry.translation_interp = Some(Interpolation::Linear);
                        }
                    }
                    _ => {}
                }
            }
        }

        if !bone_channels.is_empty() || !expression_channels.is_empty() {
            // FBXボーン名 → VRMヒューマノイド名へのマッピングを試行
            let fbx_names: Vec<(usize, &str)> = bone_channels.keys()
                .enumerate()
                .map(|(i, name)| (i, name.as_str()))
                .collect();
            let humanoid = detect_humanoid(&fbx_names);

            let (final_channels, match_mode) = if humanoid.mapping.len() >= 5 {
                // 十分なマッピングがあればヒューマノイドモードに切り替え
                let fbx_name_list: Vec<String> = bone_channels.keys().cloned().collect();
                let mut renamed: HashMap<String, BoneChannel> = HashMap::new();
                let mut mapped_count = 0;
                for (i, fbx_name) in fbx_name_list.iter().enumerate() {
                    if let Some(human_bone) = humanoid.mapping.get(&i) {
                        let vrm_name = human_bone.as_vrm_name().to_string();
                        if let Some(ch) = bone_channels.remove(fbx_name) {
                            renamed.insert(vrm_name, ch);
                            mapped_count += 1;
                        }
                    }
                }
                log::info!(
                    "FBXヒューマノイド検出: {} ({}→{}/{}ch マッピング)",
                    humanoid.rig_type.label(), bone_channels.len() + mapped_count,
                    mapped_count, fbx_name_list.len(),
                );
                (renamed, BoneMatchMode::Humanoid)
            } else {
                (bone_channels, BoneMatchMode::NodeName)
            };

            // bone_rests もマッチモードに合わせてマッピング
            let matched_rests: HashMap<String, VrmaBoneRest> = if match_mode == BoneMatchMode::Humanoid {
                // FBXボーン名 → VRMヒューマノイド名にマッピングしたレスト
                let all_fbx_names: Vec<(usize, &str)> = bone_rests.keys()
                    .enumerate()
                    .map(|(i, name)| (i, name.as_str()))
                    .collect();
                let rest_humanoid = detect_humanoid(&all_fbx_names);
                let rest_name_list: Vec<String> = bone_rests.keys().cloned().collect();
                let mut mapped_rests = HashMap::new();
                for (i, fbx_name) in rest_name_list.iter().enumerate() {
                    if let Some(human_bone) = rest_humanoid.mapping.get(&i) {
                        let vrm_name = human_bone.as_vrm_name().to_string();
                        if final_channels.contains_key(&vrm_name) {
                            if let Some(rest) = bone_rests.get(fbx_name) {
                                mapped_rests.insert(vrm_name, rest.clone());
                            }
                        }
                    }
                }
                mapped_rests
            } else {
                bone_rests.iter()
                    .filter(|(name, _)| final_channels.contains_key(name.as_str()))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            };

            log::info!(
                "FBXアニメーション読み込み: '{}' ボーン{}ch, 表情{}ch, レスト{}件, {:.2}秒, モード={:?}",
                stack_name, final_channels.len(), expression_channels.len(),
                matched_rests.len(), duration, match_mode,
            );

            animations.push(VrmaAnimation {
                name: stack_name,
                duration,
                bone_channels: final_channels,
                expression_channels,
                bone_rests: matched_rests,
                match_mode,
                facing_flip_y,
                is_additive: false,
                is_bone_local_delta: false,
            });
        }
    }

    if animations.is_empty() {
        anyhow::bail!("FBXにアニメーションチャネルが含まれていません");
    }

    Ok(animations)
}

/// 各ボーンの PreRotation クォータニオンを収集
fn collect_bone_pre_rotations(scene: &FbxScene) -> HashMap<String, Quat> {
    use super::bone::extract_transform;

    let mut pre_rots = HashMap::new();

    for obj in scene.objects.values() {
        if obj.class == "Model" && matches!(
            obj.sub_type.as_str(),
            "LimbNode" | "Root" | "Null" | ""
        ) {
            let (_, _, pre_rot_deg, _) = extract_transform(obj.node);
            let pre_rot = euler_deg_to_quat(pre_rot_deg);
            // すべてのボーンの PreRotation を登録（レストとの整合性のため）
            pre_rots.insert(obj.name.clone(), pre_rot);
        }
    }

    let non_identity_count = pre_rots.values()
        .filter(|q| q.dot(Quat::IDENTITY).abs() < 0.9999)
        .count();
    log::info!("FBX PreRotation: 全{}ボーン（非identity {}件）", pre_rots.len(), non_identity_count);

    pre_rots
}

/// AnimationCurve ノードからキータイム・キー値を読み出す
fn read_animation_curve(node: &super::parser::FbxNode) -> (Vec<f32>, Vec<f32>) {
    let times: Vec<f32> = node.child("KeyTime")
        .and_then(|n| n.properties.first())
        .and_then(|p| p.as_i64_array())
        .map(|arr| arr.iter().map(|&t| (t as f64 / FBX_TIME_UNIT) as f32).collect())
        .unwrap_or_default();

    let values: Vec<f32> = node.child("KeyValueFloat")
        .and_then(|n| n.properties.first())
        .and_then(|p| {
            // KeyValueFloat は f32 配列か f64 配列
            p.as_f32_array().map(|a| a.to_vec())
                .or_else(|| p.as_f64_array().map(|a| a.iter().map(|&v| v as f32).collect()))
        })
        .unwrap_or_default();

    (times, values)
}

/// X, Y, Z の3カーブから回転キーフレームを構築（PreRotation・座標変換付き）
fn build_rotation_keyframes(
    axis_curves: &[(Vec<f32>, Vec<f32>)],
    duration: &mut f32,
    axis_config: &FbxAxisConfig,
    pre_rotation: Quat,
) -> Vec<RotationKeyframe> {
    if axis_curves.len() < 3 {
        return Vec::new();
    }

    let all_times = merge_times(axis_curves);

    all_times.iter().map(|&t| {
        *duration = duration.max(t);
        let x = sample_curve(&axis_curves[0], t);
        let y = sample_curve(&axis_curves[1], t);
        let z = sample_curve(&axis_curves[2], t);
        // FBX Euler角（度） → Quaternion（Lcl Rotation のみ）
        let lcl_rot = euler_deg_to_quat(Vec3::new(x, y, z));
        // PreRotation を適用してレストと同じ空間にする
        let rot_fbx = pre_rotation * lcl_rot;
        // FBX空間 → glTF空間に変換
        let rot = transform_quat(rot_fbx, axis_config);
        RotationKeyframe { time: t, value: rot }
    }).collect()
}

/// X, Y, Z の3カーブから平行移動キーフレームを構築（座標変換付き）
fn build_translation_keyframes(
    axis_curves: &[(Vec<f32>, Vec<f32>)],
    duration: &mut f32,
    axis_config: &FbxAxisConfig,
) -> Vec<TranslationKeyframe> {
    if axis_curves.len() < 3 {
        return Vec::new();
    }

    let all_times = merge_times(axis_curves);

    all_times.iter().map(|&t| {
        *duration = duration.max(t);
        let x = sample_curve(&axis_curves[0], t);
        let y = sample_curve(&axis_curves[1], t);
        let z = sample_curve(&axis_curves[2], t);
        // FBX空間 → glTF空間（メートル単位に変換）
        let pos = transform_pos(Vec3::new(x, y, z), axis_config);
        TranslationKeyframe { time: t, value: pos }
    }).collect()
}

/// 複数カーブのタイムスタンプを統合・ソート・重複除去
fn merge_times(curves: &[(Vec<f32>, Vec<f32>)]) -> Vec<f32> {
    let mut times: Vec<f32> = curves.iter()
        .flat_map(|(t, _)| t.iter().copied())
        .collect();
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    times.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    times
}

/// 指定時刻での値を線形補間
fn sample_curve(curve: &(Vec<f32>, Vec<f32>), time: f32) -> f32 {
    let (times, values) = curve;
    if times.is_empty() || values.is_empty() {
        return 0.0;
    }
    if times.len() == 1 || time <= times[0] {
        return values[0];
    }
    if time >= *times.last().unwrap() {
        return *values.last().unwrap();
    }
    let idx = times.partition_point(|&t| t <= time);
    let idx = idx.min(times.len() - 1).max(1);
    let t0 = times[idx - 1];
    let t1 = times[idx];
    let v0 = values.get(idx - 1).copied().unwrap_or(0.0);
    let v1 = values.get(idx).copied().unwrap_or(0.0);
    let frac = if (t1 - t0).abs() < 1e-9 { 0.0 } else { (time - t0) / (t1 - t0) };
    v0 + (v1 - v0) * frac
}

/// ボーンのレストポーズを収集（座標変換済み、リターゲティング用）
/// グローバル位置も返す（facing検出用）
fn collect_bone_rests(scene: &FbxScene, axis_config: &FbxAxisConfig) -> (HashMap<String, VrmaBoneRest>, HashMap<String, Vec3>) {
    use super::bone::extract_transform;
    use glam::Mat4;

    let mut rests = HashMap::new();
    let mut global_positions: HashMap<String, Vec3> = HashMap::new();
    let mut bone_globals: HashMap<i64, Mat4> = HashMap::new();

    // Model ノード（LimbNode, Root, Null）を収集
    let bone_ids: Vec<i64> = scene.objects.iter()
        .filter(|(_, obj)| {
            obj.class == "Model" && matches!(
                obj.sub_type.as_str(),
                "LimbNode" | "Root" | "Null" | ""
            )
        })
        .map(|(&id, _)| id)
        .collect();

    // 親子関係から順序を決定してグローバル変換を計算（FBX空間）
    fn compute_global(
        id: i64,
        scene: &FbxScene,
        globals: &mut HashMap<i64, Mat4>,
    ) -> Mat4 {
        if let Some(&g) = globals.get(&id) {
            return g;
        }

        let obj = match scene.objects.get(&id) {
            Some(o) => o,
            None => { globals.insert(id, Mat4::IDENTITY); return Mat4::IDENTITY; }
        };

        let (trans, rot_deg, pre_rot_deg, _scale) = extract_transform(obj.node);
        let pre_rot = euler_deg_to_quat(pre_rot_deg);
        let rot = euler_deg_to_quat(rot_deg);
        let local_mat = Mat4::from_rotation_translation(pre_rot * rot, trans);

        // 親を探す
        let parent_global = scene.parents_of(id).iter()
            .find(|&&pid| scene.objects.get(&pid).map_or(false, |o| o.class == "Model"))
            .map(|&pid| compute_global(pid, scene, globals))
            .unwrap_or(Mat4::IDENTITY);

        let global = parent_global * local_mat;
        globals.insert(id, global);
        global
    }

    for &id in &bone_ids {
        let global_fbx = compute_global(id, scene, &mut bone_globals);

        if let Some(obj) = scene.objects.get(&id) {
            let (trans, rot_deg, pre_rot_deg, _) = extract_transform(obj.node);
            let pre_rot = euler_deg_to_quat(pre_rot_deg);
            let rot = euler_deg_to_quat(rot_deg);
            let local_rot_fbx = pre_rot * rot;
            let (_, global_rot_fbx, _) = global_fbx.to_scale_rotation_translation();

            // FBX空間 → glTF空間に変換
            let local_rot = transform_quat(local_rot_fbx, axis_config);
            let global_rot = transform_quat(global_rot_fbx, axis_config);
            let local_trans = transform_pos(trans, axis_config);

            // グローバル位置（facing検出用）
            let global_pos_fbx = global_fbx.col(3).truncate();
            let global_pos = transform_pos(global_pos_fbx, axis_config);

            rests.insert(obj.name.clone(), VrmaBoneRest {
                local_rotation: local_rot,
                global_rotation: global_rot,
                local_translation: local_trans,
            });
            global_positions.insert(obj.name.clone(), global_pos);
        }
    }

    (rests, global_positions)
}

/// ソースモデルの向きを検出（Leftボーンのグローバル位置Xで判定）
fn detect_facing(global_positions: &HashMap<String, Vec3>) -> bool {
    let mut left_x_sum = 0.0f32;
    let mut count = 0;

    for (name, pos) in global_positions {
        let lower = name.to_lowercase();
        if lower.contains("left")
            && (lower.contains("arm") || lower.contains("leg") || lower.contains("shoulder")
                || lower.contains("upleg"))
        {
            left_x_sum += pos.x;
            count += 1;
        }
    }

    let result = count > 0 && left_x_sum / count as f32 > 0.005;
    log::info!("facing検出: Left {}件, avg_x={:.4}, flip={}", count,
        if count > 0 { left_x_sum / count as f32 } else { 0.0 }, result);
    result
}
