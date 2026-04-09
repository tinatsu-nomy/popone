use std::collections::HashSet;

use glam::{Mat4, Vec3};

use super::types::{AStanceResult, IrBone, IrMesh, IrMorph, IrMorphKind};

/// Tポーズ→Aスタンス変換（ボーンのみ）
/// VRM用: メッシュはスキニング経由で global_mats から変形される
pub fn normalize_pose_to_astance(bones: &mut [IrBone], global_mats: &mut [Mat4]) -> AStanceResult {
    let (corrections, result) = compute_astance_corrections(bones, global_mats);
    apply_bone_corrections(bones, global_mats, &corrections);
    result
}

/// Tポーズ→Aスタンス変換（ボーン＋メッシュ頂点＋モーフオフセット）
/// FBX用: メッシュ頂点がワールド空間に展開済みなので、スキンウェイトで直接変形する
pub fn normalize_pose_to_astance_with_meshes(
    bones: &mut [IrBone],
    global_mats: &mut [Mat4],
    meshes: &mut [IrMesh],
    morphs: &mut [IrMorph],
) -> AStanceResult {
    let (corrections, result) = compute_astance_corrections(bones, global_mats);
    if corrections.is_empty() {
        return result;
    }
    apply_bone_corrections(bones, global_mats, &corrections);
    let vertex_rot3s = apply_mesh_corrections(bones, meshes, &corrections);
    apply_morph_corrections(morphs, &vertex_rot3s);
    result
}

struct AStanceCorrection {
    bone_idx: usize,
    pivot: Vec3,
    rotation: glam::Quat,
}

/// 戻り値: (補正リスト, 結果ステータス)
fn compute_astance_corrections(
    bones: &[IrBone],
    global_mats: &[Mat4],
) -> (Vec<AStanceCorrection>, AStanceResult) {
    const A_STANCE_ANGLE_DEG: f32 = 30.0;

    let find_bone = |vrm_name: &str| -> Option<usize> {
        bones
            .iter()
            .position(|b| b.vrm_bone_name.as_deref() == Some(vrm_name))
    };

    let arm_pairs = [
        ("leftUpperArm", "leftLowerArm"),
        ("rightUpperArm", "rightLowerArm"),
    ];

    // 腕ボーンが存在するかチェック
    let has_arms = arm_pairs
        .iter()
        .any(|(upper, lower)| find_bone(upper).is_some() && find_bone(lower).is_some());
    if !has_arms {
        return (Vec::new(), AStanceResult::NotFound);
    }

    let mut already_target_count = 0usize;
    let corrections: Vec<AStanceCorrection> = arm_pairs
        .iter()
        .filter_map(|(upper, lower)| {
            let ua_idx = find_bone(upper)?;
            let la_idx = find_bone(lower)?;
            let ua_node = bones[ua_idx].node_index;
            let la_node = bones[la_idx].node_index;

            let ua_pos = global_mats[ua_node].transform_point3(Vec3::ZERO);
            let la_pos = global_mats[la_node].transform_point3(Vec3::ZERO);
            let dir = (la_pos - ua_pos).normalize_or_zero();

            let horizontal = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
            if horizontal.length_squared() < 0.001 {
                // 退化ケース（腕が真上/真下）— 補正不要だが「既にAスタンス」ではない
                return None;
            }

            let current_angle = dir.dot(horizontal).clamp(-1.0, 1.0).acos().to_degrees();
            if current_angle > A_STANCE_ANGLE_DEG - 5.0 && dir.y < 0.0 {
                log::info!(
                    "A-stance conversion: {} already near A-stance ({:.1} deg), skipping",
                    upper,
                    current_angle
                );
                already_target_count += 1;
                return None;
            }

            let axis = Vec3::Y.cross(dir).normalize_or_zero();
            if axis.length_squared() < 0.001 {
                // 退化ケース（回転軸計算不能）— 補正不要だが「既にAスタンス」ではない
                return None;
            }
            let correction = glam::Quat::from_axis_angle(axis, A_STANCE_ANGLE_DEG.to_radians());

            log::info!(
                "A-stance conversion: {} rotated {:.1} deg to A-stance",
                upper,
                A_STANCE_ANGLE_DEG
            );
            Some(AStanceCorrection {
                bone_idx: ua_idx,
                pivot: ua_pos,
                rotation: correction,
            })
        })
        .collect();

    let result = if !corrections.is_empty() {
        AStanceResult::Applied(corrections.len())
    } else if already_target_count > 0 {
        AStanceResult::AlreadyAStance
    } else {
        AStanceResult::NotFound
    };
    (corrections, result)
}

fn apply_bone_corrections(
    bones: &mut [IrBone],
    global_mats: &mut [Mat4],
    corrections: &[AStanceCorrection],
) {
    for corr in corrections {
        let descendants = collect_descendants_inclusive(bones, corr.bone_idx);

        let rot_mat = Mat4::from_translation(corr.pivot)
            * Mat4::from_quat(corr.rotation)
            * Mat4::from_translation(-corr.pivot);

        for &desc_idx in &descendants {
            let node = bones[desc_idx].node_index;
            global_mats[node] = rot_mat * global_mats[node];
            bones[desc_idx].position = global_mats[node].transform_point3(Vec3::ZERO);
            bones[desc_idx].global_mat = global_mats[node];
        }
    }
}

/// メッシュ頂点をスキンウェイトに基づいて回転
/// 戻り値: グローバル頂点インデックスごとの回転行列（モーフオフセット変換用）
fn apply_mesh_corrections(
    bones: &[IrBone],
    meshes: &mut [IrMesh],
    corrections: &[AStanceCorrection],
) -> Vec<glam::Mat3> {
    // 各補正の影響ボーン集合・位置変換行列・回転行列を事前計算
    let corr_data: Vec<(HashSet<usize>, Mat4, glam::Mat3)> = corrections
        .iter()
        .map(|corr| {
            let descendants: HashSet<usize> = collect_descendants_inclusive(bones, corr.bone_idx)
                .into_iter()
                .collect();
            let rot_mat = Mat4::from_translation(corr.pivot)
                * Mat4::from_quat(corr.rotation)
                * Mat4::from_translation(-corr.pivot);
            // モーフオフセット（方向ベクトル）用の純粋な回転行列
            let rot3 = glam::Mat3::from_quat(corr.rotation);
            (descendants, rot_mat, rot3)
        })
        .collect();

    let total_verts: usize = meshes.iter().map(|m| m.vertices.len()).sum();
    let mut vertex_rot3s = vec![glam::Mat3::IDENTITY; total_verts];
    let mut global_offset = 0usize;

    for mesh in meshes.iter_mut() {
        for (local_vi, vert) in mesh.vertices_mut().iter_mut().enumerate() {
            // この頂点に影響する補正の加重平均を計算
            let mut total_weight = 0.0f32;
            let mut blended_pos = Vec3::ZERO;
            let mut blended_norm = Vec3::ZERO;
            let mut any_correction = false;

            for (affected_bones, rot_mat, rot3) in &corr_data {
                let mut corr_weight = 0.0f32;
                for &(bone_idx, w) in vert.active_weights() {
                    if affected_bones.contains(&bone_idx) {
                        corr_weight += w;
                    }
                }
                if corr_weight > 0.0 {
                    any_correction = true;
                    let rotated_pos = rot_mat.transform_point3(vert.position);
                    let rotated_norm = rot3.mul_vec3(vert.normal);
                    blended_pos += rotated_pos * corr_weight;
                    blended_norm += rotated_norm * corr_weight;
                    total_weight += corr_weight;
                }
            }

            if any_correction {
                let remaining = 1.0 - total_weight;
                vert.position = blended_pos + vert.position * remaining;
                vert.normal = (blended_norm + vert.normal * remaining).normalize_or_zero();

                // モーフオフセット用: 加重ブレンドされた回転行列を記録
                // R_blend = Σ(R_i * w_i) + I * (1 - Σw_i)
                let global_vi = global_offset + local_vi;
                let mut blended_rot = glam::Mat3::IDENTITY * remaining;
                for (affected_bones, _rot_mat, rot3) in &corr_data {
                    let mut corr_weight = 0.0f32;
                    for &(bone_idx, w) in vert.active_weights() {
                        if affected_bones.contains(&bone_idx) {
                            corr_weight += w;
                        }
                    }
                    if corr_weight > 0.0 {
                        blended_rot = glam::Mat3::from_cols(
                            blended_rot.x_axis + rot3.x_axis * corr_weight,
                            blended_rot.y_axis + rot3.y_axis * corr_weight,
                            blended_rot.z_axis + rot3.z_axis * corr_weight,
                        );
                    }
                }
                vertex_rot3s[global_vi] = blended_rot;
            }
        }
        global_offset += mesh.vertices.len();
    }

    vertex_rot3s
}

/// モーフオフセットにAスタンス回転を適用
fn apply_morph_corrections(morphs: &mut [IrMorph], vertex_rot3s: &[glam::Mat3]) {
    for morph in morphs.iter_mut() {
        if let IrMorphKind::Vertex {
            ref mut positions,
            ref mut normals,
            ref mut tangents,
        } = morph.kind
        {
            for (global_vi, offset) in positions.iter_mut() {
                if let Some(rot3) = vertex_rot3s.get(*global_vi) {
                    if *rot3 != glam::Mat3::IDENTITY {
                        *offset = rot3.mul_vec3(*offset);
                    }
                }
            }
            for (global_vi, offset) in normals.iter_mut() {
                if let Some(rot3) = vertex_rot3s.get(*global_vi) {
                    if *rot3 != glam::Mat3::IDENTITY {
                        *offset = rot3.mul_vec3(*offset);
                    }
                }
            }
            for (global_vi, offset) in tangents.iter_mut() {
                if let Some(rot3) = vertex_rot3s.get(*global_vi) {
                    if *rot3 != glam::Mat3::IDENTITY {
                        *offset = rot3.mul_vec3(*offset);
                    }
                }
            }
        }
    }
}

/// Aスタンス→Tスタンス変換（ボーン＋メッシュ頂点＋モーフオフセット＋物理）
/// PMX/PMD用: Aスタンスモデルを水平（T字）に変換
#[must_use]
pub fn normalize_pose_to_tstance_with_meshes(
    bones: &mut [IrBone],
    meshes: &mut [IrMesh],
    morphs: &mut [IrMorph],
) -> AStanceResult {
    let (corrections, result) = compute_tstance_corrections(bones);
    if corrections.is_empty() {
        return result;
    }
    let mut global_mats: Vec<Mat4> = bones.iter().map(|b| b.global_mat).collect();
    apply_bone_corrections(bones, &mut global_mats, &corrections);
    let vertex_rot3s = apply_mesh_corrections(bones, meshes, &corrections);
    apply_morph_corrections(morphs, &vertex_rot3s);
    result
}

/// Aスタンス→Tスタンス変換（物理含む全データ）
#[must_use]
pub fn normalize_pose_to_tstance_full(
    bones: &mut [IrBone],
    meshes: &mut [IrMesh],
    morphs: &mut [IrMorph],
    physics: &mut super::types::IrPhysics,
    pos_fn: fn(Vec3) -> Vec3,
) -> AStanceResult {
    let (corrections, result) = compute_tstance_corrections(bones);
    if corrections.is_empty() {
        return result;
    }
    let mut global_mats: Vec<Mat4> = bones.iter().map(|b| b.global_mat).collect();
    apply_bone_corrections(bones, &mut global_mats, &corrections);
    let vertex_rot3s = apply_mesh_corrections(bones, meshes, &corrections);
    apply_morph_corrections(morphs, &vertex_rot3s);
    apply_physics_corrections(bones, physics, &corrections, pos_fn);
    result
}

/// 物理データ（剛体・ジョイント）にスタンス補正を適用
/// 剛体・ジョイントの位置はPMX座標系なので、pos_fn で変換してから回転し、逆変換する
fn apply_physics_corrections(
    bones: &[IrBone],
    physics: &mut super::types::IrPhysics,
    corrections: &[AStanceCorrection],
    pos_fn: fn(Vec3) -> Vec3,
) {
    for corr in corrections {
        let descendants: HashSet<usize> = collect_descendants_inclusive(bones, corr.bone_idx)
            .into_iter()
            .collect();
        let pivot_pmx = pos_fn(corr.pivot);
        let rot_mat = Mat4::from_translation(pivot_pmx)
            * Mat4::from_quat(corr.rotation)
            * Mat4::from_translation(-pivot_pmx);

        // 剛体
        for rb in &mut physics.rigid_bodies {
            if let Some(bi) = rb.bone_index {
                if descendants.contains(&bi) {
                    rb.position = rot_mat.transform_point3(rb.position);
                    // 回転も補正（Euler ZXY → Quat → 回転 → Euler ZXY）
                    let rb_quat = glam::Quat::from_euler(
                        glam::EulerRot::ZXY,
                        rb.rotation.z,
                        rb.rotation.x,
                        rb.rotation.y,
                    );
                    let new_quat = corr.rotation * rb_quat;
                    let (rz, rx, ry) = new_quat.to_euler(glam::EulerRot::ZXY);
                    rb.rotation = Vec3::new(rx, ry, rz);
                }
            }
        }

        // ジョイント（rigid_a のボーンで判定）
        for joint in &mut physics.joints {
            let should_transform = if joint.rigid_a < physics.rigid_bodies.len() {
                physics.rigid_bodies[joint.rigid_a]
                    .bone_index
                    .is_some_and(|bi| descendants.contains(&bi))
            } else {
                false
            };
            if should_transform {
                joint.position = rot_mat.transform_point3(joint.position);
                let j_quat = glam::Quat::from_euler(
                    glam::EulerRot::ZXY,
                    joint.rotation.z,
                    joint.rotation.x,
                    joint.rotation.y,
                );
                let new_quat = corr.rotation * j_quat;
                let (rz, rx, ry) = new_quat.to_euler(glam::EulerRot::ZXY);
                joint.rotation = Vec3::new(rx, ry, rz);
            }
        }
    }
}

/// Aスタンス→Tスタンスの補正を計算
/// vrm_bone_name がない場合はPMXボーン名（日本語）で検索
fn compute_tstance_corrections(bones: &[IrBone]) -> (Vec<AStanceCorrection>, AStanceResult) {
    let find_bone = |names: &[&str]| -> Option<usize> {
        for name in names {
            // vrm_bone_name で検索
            if let Some(idx) = bones
                .iter()
                .position(|b| b.vrm_bone_name.as_deref() == Some(name))
            {
                return Some(idx);
            }
        }
        // PMXボーン名で検索
        for name in names {
            if let Some(idx) = bones.iter().position(|b| b.name == *name) {
                return Some(idx);
            }
        }
        None
    };

    // (上腕名候補, 前腕名候補) のペア
    let arm_pairs = [
        (
            &["leftUpperArm", "左腕"][..],
            &["leftLowerArm", "左ひじ"][..],
        ),
        (
            &["rightUpperArm", "右腕"][..],
            &["rightLowerArm", "右ひじ"][..],
        ),
    ];

    // 腕ボーンが存在するかチェック
    let has_arms = arm_pairs.iter().any(|(upper_names, lower_names)| {
        find_bone(upper_names).is_some() && find_bone(lower_names).is_some()
    });
    if !has_arms {
        return (Vec::new(), AStanceResult::NotFound);
    }

    let mut already_target_count = 0usize;
    let corrections: Vec<AStanceCorrection> = arm_pairs
        .iter()
        .filter_map(|(upper_names, lower_names)| {
            let ua_idx = find_bone(upper_names)?;
            let la_idx = find_bone(lower_names)?;

            let ua_pos = bones[ua_idx].position;
            let la_pos = bones[la_idx].position;
            let dir = (la_pos - ua_pos).normalize_or_zero();

            // 水平方向の成分
            let horizontal = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
            if horizontal.length_squared() < 0.001 {
                // 退化ケース（腕が真上/真下）— 補正不要だが「既にTスタンス」ではない
                return None;
            }

            // 現在の腕の角度（水平からの下がり角度）
            let current_angle = dir.dot(horizontal).clamp(-1.0, 1.0).acos();
            if current_angle < 5.0f32.to_radians() {
                log::info!(
                    "A->T conversion: {} already near horizontal ({:.1} deg), skipping",
                    bones[ua_idx].name,
                    current_angle.to_degrees()
                );
                already_target_count += 1;
                return None;
            }

            // 腕を上に持ち上げて水平にする → 逆方向に回転
            let axis = Vec3::Y.cross(dir).normalize_or_zero();
            if axis.length_squared() < 0.001 {
                // 退化ケース（回転軸計算不能）— 補正不要だが「既にTスタンス」ではない
                return None;
            }
            // T→A では正の角度で下に曲げた。A→T では負の角度で持ち上げる
            let correction = glam::Quat::from_axis_angle(axis, -current_angle);

            log::info!(
                "A->T conversion: {} rotated {:.1} deg to T-stance",
                bones[ua_idx].name,
                current_angle.to_degrees()
            );
            Some(AStanceCorrection {
                bone_idx: ua_idx,
                pivot: ua_pos,
                rotation: correction,
            })
        })
        .collect();

    let result = if !corrections.is_empty() {
        AStanceResult::Applied(corrections.len())
    } else if already_target_count > 0 {
        AStanceResult::AlreadyAStance
    } else {
        AStanceResult::NotFound
    };
    (corrections, result)
}

fn collect_descendants_inclusive(bones: &[IrBone], root: usize) -> Vec<usize> {
    let mut result = Vec::new();
    let mut stack = vec![root];
    while let Some(idx) = stack.pop() {
        result.push(idx);
        for &child in &bones[idx].children {
            stack.push(child);
        }
    }
    result
}
