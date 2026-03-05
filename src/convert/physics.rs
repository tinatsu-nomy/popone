use anyhow::Result;
use glam::Vec3;
use std::collections::HashMap;

use crate::intermediate::types::*;
use crate::vrm::types_v0::SecondaryAnimation;
use crate::vrm::types_v1::SpringBoneV1;
use crate::convert::coord::{gltf_pos_to_pmx, gltf_pos_to_pmx_v0, PMX_SCALE};

/// VRM 0.0 SecondaryAnimation → IrPhysics
pub fn build_physics_v0(
    sec: &SecondaryAnimation,
    node_to_bone: &HashMap<usize, usize>,
    bones: &[IrBone],
) -> Result<IrPhysics> {
    let mut physics = IrPhysics::default();

    // コライダーグループ → ボーン追従静的剛体
    for cg in &sec.collider_groups {
        let node_idx = cg.node as usize;
        let bone_idx = node_to_bone.get(&node_idx).copied();

        for (ci, collider) in cg.colliders.iter().enumerate() {
            let name = format!("collider_{}_{}", cg.node, ci);
            let raw_offset = Vec3::from(collider.offset);
            let pos = if let Some(bi) = bone_idx {
                bones[bi].position + raw_offset
            } else {
                raw_offset
            };

            physics.rigid_bodies.push(IrRigidBody {
                name,
                bone_index: bone_idx,
                group: 1,
                no_collision_mask: 0xFFFD, // G1同士は非衝突、G2（スプリング）とは衝突
                shape: RigidShape::Sphere { radius: collider.radius * PMX_SCALE },
                position: gltf_pos_to_pmx_v0(pos),
                rotation: Vec3::ZERO,
                mass: 0.0,
                linear_damping: 0.5,
                angular_damping: 0.5,
                restitution: 0.0,
                friction: 0.5,
                physics_mode: 0, // ボーン追従
            });
        }
    }

    // SpringBoneグループ → 物理剛体 + ジョイント
    for group in &sec.bone_groups {
        let stiffness = group.stiffiness.unwrap_or(1.0);
        let drag = group.drag_force.unwrap_or(0.5);
        let hit_radius = group.hit_radius.unwrap_or(0.02);

        for &root_node in &group.bones {
            build_spring_chain_v0(
                root_node as usize,
                node_to_bone,
                bones,
                hit_radius,
                stiffness,
                drag,
                &mut physics,
            );
        }
    }

    Ok(physics)
}

fn build_spring_chain_v0(
    root_node: usize,
    node_to_bone: &HashMap<usize, usize>,
    bones: &[IrBone],
    hit_radius: f32,
    stiffness: f32,
    drag: f32,
    physics: &mut IrPhysics,
) {
    let root_bone = match node_to_bone.get(&root_node) {
        Some(&bi) => bi,
        None => return,
    };

    // チェーンをDFS走査
    let mut stack = vec![(root_bone, None::<usize>)]; // (ボーンIndex, 親剛体Index)

    while let Some((bone_idx, parent_rigid_idx)) = stack.pop() {
        let bone = &bones[bone_idx];
        let rigid_idx = physics.rigid_bodies.len();

        let spring_rot = stiffness * 5.0;
        let spring_move = spring_rot * 2.0;

        // 次ボーン位置（剛体形状・ジョイント共用）
        let next_pos = bone.children.first()
            .map(|&ci| bones[ci].position)
            .unwrap_or(bone.position + Vec3::new(0.0, -0.07, 0.0));
        let bone_length = (next_pos - bone.position).length().max(0.01) * PMX_SCALE;

        // PMX座標系で剛体中心・回転を計算
        // カプセルの球体中心がボーン基底と終点に一致するよう中点に配置
        let pmx_bone_pos = gltf_pos_to_pmx_v0(bone.position);
        let pmx_next_pos = gltf_pos_to_pmx_v0(next_pos);
        let rb_rotation = bone_rotation(pmx_bone_pos, pmx_next_pos);

        let physics_mode = if parent_rigid_idx.is_none() { 0 } else { 1 }; // 根本はボーン追従
        let rb_center = (pmx_bone_pos + pmx_next_pos) * 0.5;
        let rigid = IrRigidBody {
            name: format!("spring_{}", bone.name),
            bone_index: Some(bone_idx),
            group: 2,
            no_collision_mask: 0xFFFE, // G1（コライダー）とは衝突、G2同士は非衝突
            shape: RigidShape::Capsule { radius: hit_radius * PMX_SCALE, height: bone_length },
            position: rb_center,
            rotation: rb_rotation,
            mass: 1.0,
            linear_damping: drag,
            angular_damping: drag,
            restitution: 0.0,
            friction: 0.5,
            physics_mode,
        };
        physics.rigid_bodies.push(rigid);

        // ジョイント（親剛体→この剛体）
        if let Some(parent_idx) = parent_rigid_idx {
            // 回転制限: stiffnessに基づく動的計算
            let base_limit = std::f32::consts::FRAC_PI_4; // 45°
            let limit = base_limit + (1.0 - stiffness.min(1.0)) * std::f32::consts::FRAC_PI_4;
            // stiffness=1.0 → ±45°, stiffness=0.0 → ±90°

            // 移動制限: ボーン長の30%
            let move_limit = bone_length * 0.3;

            physics.joints.push(IrJoint {
                name: format!("joint_{}", bone.name),
                rigid_a: parent_idx,
                rigid_b: rigid_idx,
                position: pmx_bone_pos, // ジョイントはボーン起点
                rotation: Vec3::ZERO,
                move_limit_lo: Vec3::splat(-move_limit),
                move_limit_hi: Vec3::splat(move_limit),
                rot_limit_lo: Vec3::splat(-limit),
                rot_limit_hi: Vec3::splat(limit),
                spring_move: Vec3::splat(spring_move),
                spring_rot: Vec3::splat(spring_rot),
            });
        }

        // 子骨を処理
        for &child_idx in &bone.children {
            stack.push((child_idx, Some(rigid_idx)));
        }
    }
}

/// VRM 1.0 VRMC_springBone → IrPhysics
pub fn build_physics_v1(
    spring_bone: &SpringBoneV1,
    node_to_bone: &HashMap<usize, usize>,
    bones: &[IrBone],
) -> Result<IrPhysics> {
    let mut physics = IrPhysics::default();

    // コライダー → ボーン追従静的剛体
    for (ci, collider) in spring_bone.colliders.iter().enumerate() {
        let node_idx = collider.node as usize;
        let bone_idx = node_to_bone.get(&node_idx).copied();

        // VRM 1.0 コライダーのローカル座標をグローバル座標に変換する
        // offset/tailはノードのローカル座標系で定義されているため、
        // ノードのグローバル変換行列で正確に変換する必要がある
        let global_mat = bone_idx.map(|bi| bones[bi].global_mat).unwrap_or(glam::Mat4::IDENTITY);

        let (shape, world_pos, rotation) = if let Some(sphere) = &collider.shape.sphere {
            let offset_v = Vec3::from(sphere.offset.unwrap_or([0.0; 3]));
            let radius = sphere.radius.unwrap_or(0.05);
            // ローカルオフセットをグローバル座標に変換（回転・スケール適用）
            let world_offset = global_mat.transform_point3(offset_v);
            let pos = gltf_pos_to_pmx(world_offset);
            (RigidShape::Sphere { radius: radius * PMX_SCALE }, pos, Vec3::ZERO)
        } else if let Some(capsule) = &collider.shape.capsule {
            let offset_v = Vec3::from(capsule.offset.unwrap_or([0.0; 3]));
            let tail_v   = Vec3::from(capsule.tail.unwrap_or([0.0, 0.1, 0.0]));
            let radius   = capsule.radius.unwrap_or(0.05);

            // ローカル座標のoffset/tailをグローバル座標に変換
            let world_offset = global_mat.transform_point3(offset_v);
            let world_tail   = global_mat.transform_point3(tail_v);

            // 高さ = グローバル座標でのoffset→tail距離
            let height = (world_tail - world_offset).length().max(1e-4);

            // 中心位置 = グローバル座標での中間点
            let pmx_center = gltf_pos_to_pmx((world_offset + world_tail) * 0.5);

            // カプセル軸の回転: PMX座標系でoffset→tailに揃える
            let pmx_offset = gltf_pos_to_pmx(world_offset);
            let pmx_tail   = gltf_pos_to_pmx(world_tail);
            let rot = bone_rotation(pmx_offset, pmx_tail);

            log::debug!(
                "  capsule local_offset=({:.3},{:.3},{:.3}) local_tail=({:.3},{:.3},{:.3}) h={:.3}",
                offset_v.x, offset_v.y, offset_v.z,
                tail_v.x, tail_v.y, tail_v.z, height
            );
            (RigidShape::Capsule { radius: radius * PMX_SCALE, height: height * PMX_SCALE }, pmx_center, rot)
        } else {
            let bone_pos = bone_idx.map(|bi| bones[bi].position).unwrap_or_default();
            (RigidShape::Sphere { radius: 0.05 * PMX_SCALE }, gltf_pos_to_pmx(bone_pos), Vec3::ZERO)
        };

        let bone_name = bone_idx.map(|bi| bones[bi].name.as_str()).unwrap_or("?");
        let shape_desc = match &shape {
            RigidShape::Sphere { radius } => format!("Sphere r={:.3}", radius),
            RigidShape::Capsule { radius, height } => format!("Capsule r={:.3} h={:.3}", radius, height),
            _ => "Other".to_string(),
        };
        log::debug!(
            "collider[{ci}] bone=\"{bone_name}\" node={} {shape_desc} pmx=({:.3},{:.3},{:.3}) rot=({:.3},{:.3},{:.3})",
            collider.node,
            world_pos.x, world_pos.y, world_pos.z,
            rotation.x, rotation.y, rotation.z,
        );

        physics.rigid_bodies.push(IrRigidBody {
            name: format!("collider_{}", ci),
            bone_index: bone_idx,
            group: 1,
            no_collision_mask: 0xFFFD, // G1同士は非衝突、G2（スプリング）とは衝突
            shape,
            position: world_pos,
            rotation,
            mass: 0.0,
            linear_damping: 0.5,
            angular_damping: 0.5,
            restitution: 0.0,
            friction: 0.5,
            physics_mode: 0,
        });
    }

    // SpringChain → 物理剛体 + ジョイント
    for spring in &spring_bone.springs {
        let joints = &spring.joints;
        if joints.is_empty() {
            continue;
        }

        log::debug!("spring \"{}\" ({} joints):", spring.name.as_deref().unwrap_or("?"), joints.len());

        let mut prev_rigid_idx: Option<usize> = None;

        for (ji, joint) in joints.iter().enumerate() {
            let node_idx = joint.node as usize;
            let bone_idx = match node_to_bone.get(&node_idx) {
                Some(&bi) => bi,
                None => continue,
            };

            let hit_radius = joint.hit_radius.unwrap_or(0.02);
            let stiffness = joint.stiffness.unwrap_or(1.0);
            let drag = joint.drag_force.unwrap_or(0.5);
            let spring_rot = stiffness * 5.0;
            let spring_move = spring_rot * 2.0;

            let bone = &bones[bone_idx];
            let rigid_idx = physics.rigid_bodies.len();

            // 次のジョイントとの間の長さを計算
            let next_pos = if ji + 1 < joints.len() {
                let next_node = joints[ji + 1].node as usize;
                node_to_bone
                    .get(&next_node)
                    .map(|&bi| bones[bi].position)
                    .unwrap_or(bone.position + Vec3::new(0.0, -0.07, 0.0))
            } else {
                // 末端に仮想tail追加（仕様準拠: 7cm下）
                bone.position + Vec3::new(0.0, -0.07, 0.0)
            };
            let height = (next_pos - bone.position).length().max(0.01) * PMX_SCALE;

            log::debug!(
                "  [{ji}] bone=\"{}\" gltf_pos=({:.3},{:.3},{:.3}) next=({:.3},{:.3},{:.3}) h={:.3}",
                bone.name,
                bone.position.x, bone.position.y, bone.position.z,
                next_pos.x, next_pos.y, next_pos.z,
                height
            );

            // PMX座標系で剛体中心・回転を計算
            // カプセルの球体中心がボーン基底と終点に一致するよう中点に配置
            let pmx_bone_pos = gltf_pos_to_pmx(bone.position);
            let pmx_next_pos = gltf_pos_to_pmx(next_pos);
            let rb_rotation = bone_rotation(pmx_bone_pos, pmx_next_pos);

            let physics_mode = if prev_rigid_idx.is_none() { 0 } else { 1 }; // 根本はボーン追従
            let rb_center = (pmx_bone_pos + pmx_next_pos) * 0.5;
            physics.rigid_bodies.push(IrRigidBody {
                name: format!(
                    "spring_{}_{}",
                    spring.name.as_deref().unwrap_or("chain"),
                    ji
                ),
                bone_index: Some(bone_idx),
                group: 2,
                no_collision_mask: 0xFFFE, // G1（コライダー）とは衝突、G2同士は非衝突
                shape: RigidShape::Capsule { radius: hit_radius * PMX_SCALE, height },
                position: rb_center,
                rotation: rb_rotation,
                mass: 1.0,
                linear_damping: drag,
                angular_damping: drag,
                restitution: 0.0,
                friction: 0.5,
                physics_mode,
            });

            if let Some(pa) = prev_rigid_idx {
                // 回転制限: stiffnessに基づく動的計算
                let base_limit = std::f32::consts::FRAC_PI_4; // 45°
                let limit = base_limit + (1.0 - stiffness.min(1.0)) * std::f32::consts::FRAC_PI_4;
                // stiffness=1.0 → ±45°, stiffness=0.0 → ±90°

                // gravity による回転制限バイアス
                let gravity_power = joint.gravity_power.unwrap_or(0.0);
                let gravity_dir = joint.gravity_dir.map(Vec3::from)
                    .unwrap_or(Vec3::new(0.0, -1.0, 0.0));

                // 重力方向をPMX座標系に変換してバイアスを計算
                let pmx_gravity = gltf_pos_to_pmx(gravity_dir).normalize_or_zero();
                let gravity_bias = pmx_gravity * gravity_power * std::f32::consts::FRAC_PI_4;

                let rot_limit_lo = Vec3::splat(-limit) + gravity_bias.min(Vec3::ZERO);
                let rot_limit_hi = Vec3::splat(limit) + gravity_bias.max(Vec3::ZERO);

                // 移動制限: ボーン長の30%
                let bone_length = (next_pos - bone.position).length().max(0.01) * PMX_SCALE;
                let move_limit = bone_length * 0.3;

                physics.joints.push(IrJoint {
                    name: format!(
                        "joint_{}_{}",
                        spring.name.as_deref().unwrap_or("chain"),
                        ji
                    ),
                    rigid_a: pa,
                    rigid_b: rigid_idx,
                    position: pmx_bone_pos, // ジョイントはボーン起点
                    rotation: Vec3::ZERO,
                    move_limit_lo: Vec3::splat(-move_limit),
                    move_limit_hi: Vec3::splat(move_limit),
                    rot_limit_lo,
                    rot_limit_hi,
                    spring_move: Vec3::splat(spring_move),
                    spring_rot: Vec3::splat(spring_rot),
                });
            }

            prev_rigid_idx = Some(rigid_idx);
        }
    }

    Ok(physics)
}

/// PMX剛体のローカルY軸を from_pmx → to_pmx 方向に揃えるオイラー角を返す
///
/// PMXEditor の GetPoseMatrix_Bone + MatrixToEuler_ZXY に倣い:
/// 1. Y軸 = ボーン方向、X軸 = Cross(Y, Z_unit)、Z軸 = Cross(X, Y) で直交系を構築
/// 2. 回転行列 → クォータニオン → ZXY オイラー角で抽出 → Vec3(rx, ry, rz) で返す
fn bone_rotation(from_pmx: Vec3, to_pmx: Vec3) -> Vec3 {
    let dir = (to_pmx - from_pmx).normalize_or_zero();
    if dir.length_squared() < 1e-6 {
        return Vec3::ZERO;
    }

    // Y軸 = ボーン方向
    let y_axis = dir;

    // X軸 = Y軸 × Z単位ベクトル（PMXEditor: Cross(vector, Vector3.UnitZ)）
    let x_axis_raw = y_axis.cross(Vec3::Z);
    let x_axis = if x_axis_raw.length_squared() < 1e-6 {
        // Y軸がZ軸と平行（真上/真下）の場合はX軸方向にフォールバック
        Vec3::X
    } else {
        x_axis_raw.normalize()
    };

    // Z軸 = X軸 × Y軸（PMXEditor: Cross(left, vector)）
    let z_axis = x_axis.cross(y_axis);

    // 回転行列 → クォータニオン → ZXY オイラー角（PMXEditor: MatrixToEuler_ZXY）
    let mat = glam::Mat3::from_cols(x_axis, y_axis, z_axis);
    let quat = glam::Quat::from_mat3(&mat);
    // PMX は左手系のため X 軸の回転方向が RH と逆（Rx(θ)_RH = Rx(-θ)_LH）
    let (rz, rx, ry) = quat.to_euler(glam::EulerRot::ZXY);
    Vec3::new(-rx, ry, rz)
}
