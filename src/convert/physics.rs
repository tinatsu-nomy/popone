use anyhow::Result;
use glam::Vec3;
use std::collections::HashMap;

use crate::convert::coord::{gltf_pos_to_pmx, gltf_pos_to_pmx_v0, PMX_SCALE};
use crate::intermediate::types::*;
use crate::vrm::types_v0::SecondaryAnimation;
use crate::vrm::types_v1::SpringBoneV1;

// スプリングボーン剛体は PMX グループ15 に統一
const SPRING_PMX_GROUP: u8 = 15;

/// 座標変換関数の型（V0/V1 で異なる変換を共通ヘルパーに渡すため）
type CoordFn = fn(Vec3) -> Vec3;

/// スプリング剛体の構築パラメータ
struct SpringRigidParams {
    name: String,
    bone_index: usize,
    spring_mask: u16,
    hit_radius: f32,
    drag: f32,
    is_root: bool,
    /// gltf 座標系でのボーン位置
    bone_pos: Vec3,
    /// gltf 座標系での次ボーン位置
    next_pos: Vec3,
}

/// スプリングジョイントの構築パラメータ
struct SpringJointParams {
    name: String,
    parent_rigid_idx: usize,
    rigid_idx: usize,
    stiffness: f32,
    spring_rot_val: f32,
    spring_move_val: f32,
    gravity_power: f32,
    gravity_dir: Vec3,
    /// PMX 座標系でのボーン位置（ジョイント起点）
    pmx_bone_pos: Vec3,
    /// gltf 座標系でのボーン間距離（移動制限計算用）
    bone_pos: Vec3,
    next_pos: Vec3,
}

/// スプリング剛体を構築して physics に追加する共通ヘルパー
fn push_spring_rigid(
    physics: &mut IrPhysics,
    params: &SpringRigidParams,
    coord_fn: CoordFn,
) -> (usize, Vec3) {
    let rigid_idx = physics.rigid_bodies.len();
    let bone_length = (params.next_pos - params.bone_pos).length().max(0.01) * PMX_SCALE;

    let pmx_bone_pos = coord_fn(params.bone_pos);
    let pmx_next_pos = coord_fn(params.next_pos);
    let rb_rotation = bone_rotation(pmx_bone_pos, pmx_next_pos);
    let rb_center = (pmx_bone_pos + pmx_next_pos) * 0.5;
    let physics_mode = if params.is_root { 0 } else { 1 };

    physics.rigid_bodies.push(IrRigidBody {
        name: params.name.clone(),
        bone_index: Some(params.bone_index),
        group: SPRING_PMX_GROUP,
        no_collision_mask: params.spring_mask,
        shape: RigidShape::Capsule {
            radius: params.hit_radius * PMX_SCALE,
            height: bone_length,
        },
        position: rb_center,
        rotation: rb_rotation,
        mass: 1.0,
        linear_damping: params.drag,
        angular_damping: params.drag,
        restitution: 0.0,
        friction: 0.5,
        physics_mode,
    });

    (rigid_idx, pmx_bone_pos)
}

/// スプリングジョイントを構築して physics に追加する共通ヘルパー
fn push_spring_joint(physics: &mut IrPhysics, params: &SpringJointParams, coord_fn: CoordFn) {
    // 回転制限: stiffnessに基づく動的計算
    let base_limit = std::f32::consts::FRAC_PI_4; // 45°
    let limit = base_limit + (1.0 - params.stiffness.min(1.0)) * std::f32::consts::FRAC_PI_4;
    // stiffness=1.0 → ±45°, stiffness=0.0 → ±90°

    // gravity による回転制限バイアス
    let pmx_gravity = coord_fn(params.gravity_dir).normalize_or_zero();
    let gravity_bias = pmx_gravity * params.gravity_power * std::f32::consts::FRAC_PI_4;

    let rot_limit_lo = Vec3::splat(-limit) + gravity_bias.min(Vec3::ZERO);
    let rot_limit_hi = Vec3::splat(limit) + gravity_bias.max(Vec3::ZERO);

    // 移動制限: ボーン長の30%
    let bone_length = (params.next_pos - params.bone_pos).length().max(0.01) * PMX_SCALE;
    let move_limit = bone_length * 0.3;

    physics.joints.push(IrJoint {
        name: params.name.clone(),
        rigid_a: params.parent_rigid_idx,
        rigid_b: params.rigid_idx,
        position: params.pmx_bone_pos,
        rotation: Vec3::ZERO,
        move_limit_lo: Vec3::splat(-move_limit),
        move_limit_hi: Vec3::splat(move_limit),
        rot_limit_lo,
        rot_limit_hi,
        spring_move: Vec3::splat(params.spring_move_val),
        spring_rot: Vec3::splat(params.spring_rot_val),
    });
}

/// VRM 0.0 SecondaryAnimation → IrPhysics
pub fn build_physics_v0(
    sec: &SecondaryAnimation,
    node_to_bone: &HashMap<usize, usize>,
    bones: &[IrBone],
) -> Result<IrPhysics> {
    let mut physics = IrPhysics::default();

    let num_collider_groups = sec.collider_groups.len().min(14);

    // コライダーグループ → ボーン追従静的剛体
    // 各 VRM collider group を PMX グループ 1,2,3,... に割り当て
    for (cg_idx, cg) in sec.collider_groups.iter().enumerate() {
        let node_idx = cg.node as usize;
        let bone_idx = node_to_bone.get(&node_idx).copied();
        let pmx_group = (cg_idx + 1).min(14) as u8;

        // コライダーはスプリング(G15)とのみ衝突
        let collider_mask = !(1u16 << SPRING_PMX_GROUP);

        for (ci, collider) in cg.colliders.iter().enumerate() {
            let name = format!("collider_{}_{}", cg.node, ci);
            let raw_offset = Vec3::from(collider.offset);

            // Fix: global_mat でローカル→グローバル変換（回転考慮）
            let global_mat = bone_idx
                .map(|bi| bones[bi].global_mat)
                .unwrap_or(glam::Mat4::IDENTITY);
            let world_pos = global_mat.transform_point3(raw_offset);

            physics.rigid_bodies.push(IrRigidBody {
                name,
                bone_index: bone_idx,
                group: pmx_group,
                no_collision_mask: collider_mask,
                shape: RigidShape::Sphere {
                    radius: collider.radius * PMX_SCALE,
                },
                position: gltf_pos_to_pmx_v0(world_pos),
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
        let gravity_power = group.gravity_power.unwrap_or(0.0);
        let gravity_dir = group
            .gravity_dir
            .map(Vec3::from)
            .unwrap_or(Vec3::new(0.0, -1.0, 0.0));

        // center ノード警告
        if let Some(center) = group.center {
            if center >= 0 {
                log::warn!(
                    "BoneGroup center ノード({}) は PMX では未サポート（無視）",
                    center
                );
            }
        }

        // コライダーグループ参照からマスク構築
        let spring_mask = build_spring_mask(&group.collider_groups, num_collider_groups);

        for &root_node in &group.bones {
            build_spring_chain_v0(
                root_node as usize,
                node_to_bone,
                bones,
                hit_radius,
                stiffness,
                drag,
                gravity_power,
                gravity_dir,
                spring_mask,
                &mut physics,
            );
        }
    }

    Ok(physics)
}

/// スプリングの no_collision_mask を構築
/// 参照するコライダーグループとの衝突のみ有効にする
fn build_spring_mask(collider_group_refs: &[i32], num_collider_groups: usize) -> u16 {
    let mut mask = 0xFFFF_u16;
    // 参照するコライダーグループのビットをクリア（衝突有効化）
    for &cg_idx in collider_group_refs {
        let pmx_group = (cg_idx as usize + 1).min(14);
        if pmx_group <= num_collider_groups {
            mask &= !(1u16 << pmx_group);
        }
    }
    mask
}

#[allow(clippy::too_many_arguments)]
fn build_spring_chain_v0(
    root_node: usize,
    node_to_bone: &HashMap<usize, usize>,
    bones: &[IrBone],
    hit_radius: f32,
    stiffness: f32,
    drag: f32,
    gravity_power: f32,
    gravity_dir: Vec3,
    spring_mask: u16,
    physics: &mut IrPhysics,
) {
    let root_bone = match node_to_bone.get(&root_node) {
        Some(&bi) => bi,
        None => return,
    };

    let spring_rot_val = stiffness * 5.0;
    let spring_move_val = spring_rot_val * 2.0;

    // チェーンをDFS走査
    let mut stack = vec![(root_bone, None::<usize>)]; // (ボーンIndex, 親剛体Index)

    while let Some((bone_idx, parent_rigid_idx)) = stack.pop() {
        let bone = &bones[bone_idx];

        // 次ボーン位置（剛体形状・ジョイント共用）
        let next_pos = bone
            .children
            .first()
            .map(|&ci| bones[ci].position)
            .unwrap_or(bone.position + Vec3::new(0.0, -0.07, 0.0));

        let (rigid_idx, pmx_bone_pos) = push_spring_rigid(
            physics,
            &SpringRigidParams {
                name: format!("spring_{}", bone.name),
                bone_index: bone_idx,
                spring_mask,
                hit_radius,
                drag,
                is_root: parent_rigid_idx.is_none(),
                bone_pos: bone.position,
                next_pos,
            },
            gltf_pos_to_pmx_v0,
        );

        // ジョイント（親剛体→この剛体）
        if let Some(parent_idx) = parent_rigid_idx {
            push_spring_joint(
                physics,
                &SpringJointParams {
                    name: format!("joint_{}", bone.name),
                    parent_rigid_idx: parent_idx,
                    rigid_idx,
                    stiffness,
                    spring_rot_val,
                    spring_move_val,
                    gravity_power,
                    gravity_dir,
                    pmx_bone_pos,
                    bone_pos: bone.position,
                    next_pos,
                },
                gltf_pos_to_pmx_v0,
            );
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

    // コライダー → PMX グループ割り当て
    // 各 VRM colliderGroup を PMX グループ 1,2,3,... に割り当て
    let num_collider_groups = spring_bone.collider_groups.len().min(14);
    let mut collider_pmx_group: Vec<u8> = vec![0; spring_bone.colliders.len()];
    for (cg_idx, cg) in spring_bone.collider_groups.iter().enumerate() {
        let pmx_group = (cg_idx + 1) as u8;
        for &ci in &cg.colliders {
            let ci = ci as usize;
            if ci < collider_pmx_group.len() && collider_pmx_group[ci] == 0 {
                collider_pmx_group[ci] = pmx_group;
            }
        }
    }
    // 未割り当てコライダーはグループ1にフォールバック
    for g in &mut collider_pmx_group {
        if *g == 0 {
            *g = 1;
        }
    }

    // コライダー → ボーン追従静的剛体
    for (ci, collider) in spring_bone.colliders.iter().enumerate() {
        let node_idx = collider.node as usize;
        let bone_idx = node_to_bone.get(&node_idx).copied();

        let global_mat = bone_idx
            .map(|bi| bones[bi].global_mat)
            .unwrap_or(glam::Mat4::IDENTITY);
        let pmx_group = collider_pmx_group.get(ci).copied().unwrap_or(1);

        // コライダーはスプリング(G15)とのみ衝突
        let collider_mask = !(1u16 << SPRING_PMX_GROUP);

        let (shape, world_pos, rotation) =
            if let Some(sphere) = &collider.shape.sphere {
                let offset_v = Vec3::from(sphere.offset.unwrap_or([0.0; 3]));
                let radius = sphere.radius.unwrap_or(0.05);
                let world_offset = global_mat.transform_point3(offset_v);
                let pos = gltf_pos_to_pmx(world_offset);
                (
                    RigidShape::Sphere {
                        radius: radius * PMX_SCALE,
                    },
                    pos,
                    Vec3::ZERO,
                )
            } else if let Some(capsule) = &collider.shape.capsule {
                let offset_v = Vec3::from(capsule.offset.unwrap_or([0.0; 3]));
                let tail_v = Vec3::from(capsule.tail.unwrap_or([0.0, 0.1, 0.0]));
                let radius = capsule.radius.unwrap_or(0.05);

                let world_offset = global_mat.transform_point3(offset_v);
                let world_tail = global_mat.transform_point3(tail_v);

                let height = (world_tail - world_offset).length().max(1e-4);
                let pmx_center = gltf_pos_to_pmx((world_offset + world_tail) * 0.5);

                let pmx_offset = gltf_pos_to_pmx(world_offset);
                let pmx_tail = gltf_pos_to_pmx(world_tail);
                let rot = bone_rotation(pmx_offset, pmx_tail);

                log::debug!(
                "  capsule local_offset=({:.3},{:.3},{:.3}) local_tail=({:.3},{:.3},{:.3}) h={:.3}",
                offset_v.x, offset_v.y, offset_v.z,
                tail_v.x, tail_v.y, tail_v.z, height
            );
                (
                    RigidShape::Capsule {
                        radius: radius * PMX_SCALE,
                        height: height * PMX_SCALE,
                    },
                    pmx_center,
                    rot,
                )
            } else {
                let bone_pos = bone_idx.map(|bi| bones[bi].position).unwrap_or_default();
                (
                    RigidShape::Sphere {
                        radius: 0.05 * PMX_SCALE,
                    },
                    gltf_pos_to_pmx(bone_pos),
                    Vec3::ZERO,
                )
            };

        let bone_name = bone_idx.map(|bi| bones[bi].name.as_str()).unwrap_or("?");
        let shape_desc = match &shape {
            RigidShape::Sphere { radius } => format!("Sphere r={:.3}", radius),
            RigidShape::Capsule { radius, height } => {
                format!("Capsule r={:.3} h={:.3}", radius, height)
            }
            _ => "Other".to_string(),
        };
        log::debug!(
            "collider[{ci}] bone=\"{bone_name}\" node={} {shape_desc} group={} pmx=({:.3},{:.3},{:.3}) rot=({:.3},{:.3},{:.3})",
            collider.node, pmx_group,
            world_pos.x, world_pos.y, world_pos.z,
            rotation.x, rotation.y, rotation.z,
        );

        physics.rigid_bodies.push(IrRigidBody {
            name: format!("collider_{}", ci),
            bone_index: bone_idx,
            group: pmx_group,
            no_collision_mask: collider_mask,
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

        // center ノード警告
        if let Some(center) = spring.center {
            log::warn!(
                "Spring \"{}\" の center ノード({}) は PMX では未サポート（無視）",
                spring.name.as_deref().unwrap_or("?"),
                center
            );
        }

        log::debug!(
            "spring \"{}\" ({} joints):",
            spring.name.as_deref().unwrap_or("?"),
            joints.len()
        );

        // このスプリングが参照するコライダーグループからマスク構築
        let spring_mask = if let Some(ref cg_refs) = spring.collider_groups {
            build_spring_mask(cg_refs, num_collider_groups)
        } else {
            0xFFFF_u16 // コライダーグループ未参照 → 衝突なし
        };

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
            let spring_rot_val = stiffness * 5.0;
            let spring_move_val = spring_rot_val * 2.0;

            let bone = &bones[bone_idx];

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

            log::debug!(
                "  [{ji}] bone=\"{}\" gltf_pos=({:.3},{:.3},{:.3}) next=({:.3},{:.3},{:.3}) h={:.3}",
                bone.name,
                bone.position.x, bone.position.y, bone.position.z,
                next_pos.x, next_pos.y, next_pos.z,
                (next_pos - bone.position).length().max(0.01) * PMX_SCALE
            );

            let (rigid_idx, pmx_bone_pos) = push_spring_rigid(
                &mut physics,
                &SpringRigidParams {
                    name: format!(
                        "spring_{}_{}",
                        spring.name.as_deref().unwrap_or("chain"),
                        ji
                    ),
                    bone_index: bone_idx,
                    spring_mask,
                    hit_radius,
                    drag,
                    is_root: prev_rigid_idx.is_none(),
                    bone_pos: bone.position,
                    next_pos,
                },
                gltf_pos_to_pmx,
            );

            if let Some(pa) = prev_rigid_idx {
                let gravity_power = joint.gravity_power.unwrap_or(0.0);
                let gravity_dir = joint
                    .gravity_dir
                    .map(Vec3::from)
                    .unwrap_or(Vec3::new(0.0, -1.0, 0.0));

                push_spring_joint(
                    &mut physics,
                    &SpringJointParams {
                        name: format!("joint_{}_{}", spring.name.as_deref().unwrap_or("chain"), ji),
                        parent_rigid_idx: pa,
                        rigid_idx,
                        stiffness,
                        spring_rot_val,
                        spring_move_val,
                        gravity_power,
                        gravity_dir,
                        pmx_bone_pos,
                        bone_pos: bone.position,
                        next_pos,
                    },
                    gltf_pos_to_pmx,
                );
            }

            prev_rigid_idx = Some(rigid_idx);
        }
    }

    Ok(physics)
}

/// 剛体のローカルY軸をボーン方向に揃えるオイラー角を返す
///
/// 1. Y軸 = ボーン方向（Y成分が負なら反転）
/// 2. X軸 = Y × Z単位ベクトル
/// 3. Z軸 = X × Y
/// 4. ZXY オイラー分解（R = Rz * Rx * Ry）
fn bone_rotation(from_pmx: Vec3, to_pmx: Vec3) -> Vec3 {
    let mut dir = (to_pmx - from_pmx).normalize_or_zero();
    if dir.length_squared() < 1e-6 {
        return Vec3::ZERO;
    }

    // Y成分が負なら方向を反転（剛体Y軸は常に上向き寄り）
    if dir.y < 0.0 {
        dir = -dir;
    }

    // 基底構築: Y=dir, X=Y×Z単位, Z=X×Y
    let y_axis = dir;
    let x_raw = y_axis.cross(Vec3::Z);
    let x_axis = if x_raw.length_squared() < 1e-6 {
        // Y軸がZ軸と平行な場合、X軸を基準に
        y_axis.cross(Vec3::X).normalize()
    } else {
        x_raw.normalize()
    };
    let z_axis = x_axis.cross(y_axis).normalize();

    // YXZ intrinsic = ZXY extrinsic オイラー分解（R = Rz * Rx * Ry）
    // D3DX行優先: v * Ry * Rx * Rz → glam列優先: Rz * Rx * Ry
    let mat = glam::Mat3::from_cols(x_axis, y_axis, z_axis);
    let quat = glam::Quat::from_mat3(&mat);
    let (ry, rx, rz) = quat.to_euler(glam::EulerRot::YXZ);
    Vec3::new(rx, ry, rz)
}
