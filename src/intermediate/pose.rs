use std::collections::HashSet;

use glam::{Mat4, Vec3};

use super::types::{IrBone, IrMesh};

/// Tポーズ→Aスタンス変換（ボーンのみ）
/// VRM用: メッシュはスキニング経由で global_mats から変形される
pub fn normalize_pose_to_astance(bones: &mut [IrBone], global_mats: &mut [Mat4]) {
    let corrections = compute_astance_corrections(bones, global_mats);
    apply_bone_corrections(bones, global_mats, &corrections);
}

/// Tポーズ→Aスタンス変換（ボーン＋メッシュ頂点）
/// FBX用: メッシュ頂点がワールド空間に展開済みなので、スキンウェイトで直接変形する
pub fn normalize_pose_to_astance_with_meshes(
    bones: &mut [IrBone],
    global_mats: &mut [Mat4],
    meshes: &mut [IrMesh],
) {
    let corrections = compute_astance_corrections(bones, global_mats);
    if corrections.is_empty() {
        return;
    }
    apply_bone_corrections(bones, global_mats, &corrections);
    apply_mesh_corrections(bones, meshes, &corrections);
}

struct AStanceCorrection {
    bone_idx: usize,
    pivot: Vec3,
    rotation: glam::Quat,
}

fn compute_astance_corrections(
    bones: &[IrBone],
    global_mats: &[Mat4],
) -> Vec<AStanceCorrection> {
    const A_STANCE_ANGLE_DEG: f32 = 30.0;

    let find_bone = |vrm_name: &str| -> Option<usize> {
        bones.iter().position(|b| b.vrm_bone_name.as_deref() == Some(vrm_name))
    };

    let arm_pairs = [
        ("leftUpperArm", "leftLowerArm"),
        ("rightUpperArm", "rightLowerArm"),
    ];

    arm_pairs
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
                return None;
            }

            let current_angle = dir.dot(horizontal).clamp(-1.0, 1.0).acos().to_degrees();
            if current_angle > A_STANCE_ANGLE_DEG - 5.0 && dir.y < 0.0 {
                log::info!(
                    "Aスタンス変換: {} は既にAスタンスに近い（{:.1}°）、スキップ",
                    upper, current_angle
                );
                return None;
            }

            let axis = Vec3::Y.cross(dir).normalize_or_zero();
            if axis.length_squared() < 0.001 {
                return None;
            }
            let correction = glam::Quat::from_axis_angle(axis, A_STANCE_ANGLE_DEG.to_radians());

            log::info!(
                "Aスタンス変換: {} を {:.1}° 回転してAスタンスに変換",
                upper, A_STANCE_ANGLE_DEG
            );
            Some(AStanceCorrection {
                bone_idx: ua_idx,
                pivot: ua_pos,
                rotation: correction,
            })
        })
        .collect()
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
fn apply_mesh_corrections(
    bones: &[IrBone],
    meshes: &mut [IrMesh],
    corrections: &[AStanceCorrection],
) {
    // 各補正の影響ボーン集合と回転行列を事前計算
    let corr_data: Vec<(HashSet<usize>, Mat4)> = corrections
        .iter()
        .map(|corr| {
            let descendants: HashSet<usize> =
                collect_descendants_inclusive(bones, corr.bone_idx).into_iter().collect();
            let rot_mat = Mat4::from_translation(corr.pivot)
                * Mat4::from_quat(corr.rotation)
                * Mat4::from_translation(-corr.pivot);
            (descendants, rot_mat)
        })
        .collect();

    for mesh in meshes.iter_mut() {
        for vert in mesh.vertices.iter_mut() {
            // この頂点に影響する補正の加重平均を計算
            let mut total_weight = 0.0f32;
            let mut blended_pos = Vec3::ZERO;
            let mut blended_norm = Vec3::ZERO;
            let mut any_correction = false;

            for (affected_bones, rot_mat) in &corr_data {
                let mut corr_weight = 0.0f32;
                for &(bone_idx, w) in &vert.weights {
                    if affected_bones.contains(&bone_idx) {
                        corr_weight += w;
                    }
                }
                if corr_weight > 0.0 {
                    any_correction = true;
                    let rotated_pos = rot_mat.transform_point3(vert.position);
                    let rot3 = glam::Mat3::from_mat4(*rot_mat);
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
            }
        }
    }
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
