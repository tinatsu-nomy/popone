use crate::error::Result;
use glam::Vec3;
use std::collections::HashMap;

use crate::convert::coord::{gltf_pos_to_pmx, gltf_pos_to_pmx_v0, PMX_SCALE};
use crate::intermediate::types::*;
use crate::vrm::types_v0::SecondaryAnimation;
use crate::vrm::types_v1::SpringBoneV1;

// Spring-bone rigid bodies are always assigned to PMX group 15
const SPRING_PMX_GROUP: u8 = 15;

/// Type for the coord-conversion function (lets V0/V1 pass different transforms to shared helpers).
type CoordFn = fn(Vec3) -> Vec3;

/// Parameters for building a spring rigid body.
struct SpringRigidParams {
    name: String,
    bone_index: usize,
    spring_mask: u16,
    hit_radius: f32,
    drag: f32,
    is_root: bool,
    /// Bone position in glTF coords.
    bone_pos: Vec3,
    /// Next-bone position in glTF coords.
    next_pos: Vec3,
}

/// Parameters for building a spring joint.
struct SpringJointParams {
    name: String,
    parent_rigid_idx: usize,
    rigid_idx: usize,
    stiffness: f32,
    spring_rot_val: f32,
    spring_move_val: f32,
    gravity_power: f32,
    gravity_dir: Vec3,
    /// Bone position in PMX coords (joint anchor).
    pmx_bone_pos: Vec3,
    /// Bone separation in glTF coords (used to compute the move limit).
    bone_pos: Vec3,
    next_pos: Vec3,
}

/// Shared helper that builds a spring rigid body and pushes it into `physics`.
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

/// Shared helper that builds a spring joint and pushes it into `physics`.
fn push_spring_joint(physics: &mut IrPhysics, params: &SpringJointParams, coord_fn: CoordFn) {
    // Rotation limit: dynamically computed from stiffness
    let base_limit = std::f32::consts::FRAC_PI_4; // 45 degrees
    let limit = base_limit + (1.0 - params.stiffness.min(1.0)) * std::f32::consts::FRAC_PI_4;
    // stiffness=1.0 -> +/-45 deg, stiffness=0.0 -> +/-90 deg

    // Bias the rotation limit by gravity
    let pmx_gravity = coord_fn(params.gravity_dir).normalize_or_zero();
    let gravity_bias = pmx_gravity * params.gravity_power * std::f32::consts::FRAC_PI_4;

    let rot_limit_lo = Vec3::splat(-limit) + gravity_bias.min(Vec3::ZERO);
    let rot_limit_hi = Vec3::splat(limit) + gravity_bias.max(Vec3::ZERO);

    // Move limit: 30% of bone length
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

/// VRM 0.0 SecondaryAnimation -> IrPhysics.
pub fn build_physics_v0(
    sec: &SecondaryAnimation,
    node_to_bone: &HashMap<usize, usize>,
    bones: &[IrBone],
) -> Result<IrPhysics> {
    let mut physics = IrPhysics::default();

    let num_collider_groups = sec.collider_groups.len().min(14);

    // Collider groups -> static rigid bodies that follow their bone.
    // Each VRM collider group is mapped to PMX groups 1, 2, 3, ...
    for (cg_idx, cg) in sec.collider_groups.iter().enumerate() {
        let node_idx = cg.node as usize;
        let bone_idx = node_to_bone.get(&node_idx).copied();
        let pmx_group = (cg_idx + 1).min(14) as u8;

        // Colliders only collide with the spring group (G15)
        let collider_mask = !(1u16 << SPRING_PMX_GROUP);

        for (ci, collider) in cg.colliders.iter().enumerate() {
            let name = format!("collider_{}_{}", cg.node, ci);
            let raw_offset = Vec3::from(collider.offset);

            // Fix: convert local -> world via global_mat (handles rotation)
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
                physics_mode: 0, // Follow bone
            });
        }
    }

    // SpringBone groups -> dynamic rigid bodies + joints
    for group in &sec.bone_groups {
        let stiffness = group.stiffiness.unwrap_or(1.0);
        let drag = group.drag_force.unwrap_or(0.5);
        let hit_radius = group.hit_radius.unwrap_or(0.02);
        let gravity_power = group.gravity_power.unwrap_or(0.0);
        let gravity_dir = group
            .gravity_dir
            .map(Vec3::from)
            .unwrap_or(Vec3::new(0.0, -1.0, 0.0));

        // Warn about a center node (not supported in PMX)
        if let Some(center) = group.center {
            if center >= 0 {
                log::warn!(
                    "BoneGroup center node ({}) is not supported in PMX (ignored)",
                    center
                );
            }
        }

        // Build the mask from collider-group references
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

/// Build the spring's `no_collision_mask`.
/// Only enables collisions with the referenced collider groups.
fn build_spring_mask(collider_group_refs: &[i32], num_collider_groups: usize) -> u16 {
    let mut mask = 0xFFFF_u16;
    // Clear the bits for referenced collider groups (= enable collision)
    for &cg_idx in collider_group_refs {
        let pmx_group = (cg_idx as usize + 1).min(14);
        if pmx_group <= num_collider_groups {
            mask &= !(1u16 << pmx_group);
        }
    }
    mask
}

/// Maximum depth of a spring chain (prevents runaway loops).
const MAX_SPRING_CHAIN_DEPTH: u32 = 64;

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

    // DFS through the chain with a depth cap
    let mut stack = vec![(root_bone, None::<usize>, 0u32)]; // (bone index, parent rigid index, depth)

    while let Some((bone_idx, parent_rigid_idx, depth)) = stack.pop() {
        if depth >= MAX_SPRING_CHAIN_DEPTH {
            log::warn!(
                "Spring chain reached max depth ({}) - truncated (bone={})",
                MAX_SPRING_CHAIN_DEPTH,
                bones.get(bone_idx).map(|b| b.name.as_str()).unwrap_or("?")
            );
            continue;
        }
        let bone = &bones[bone_idx];

        // Next-bone position (shared by the rigid shape and the joint)
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

        // Joint (parent rigid -> this rigid)
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

        // Process child bones
        for &child_idx in &bone.children {
            stack.push((child_idx, Some(rigid_idx), depth + 1));
        }
    }
}

/// VRM 1.0 VRMC_springBone -> IrPhysics.
pub fn build_physics_v1(
    spring_bone: &SpringBoneV1,
    node_to_bone: &HashMap<usize, usize>,
    bones: &[IrBone],
) -> Result<IrPhysics> {
    let mut physics = IrPhysics::default();

    // Assign colliders to PMX groups.
    // Each VRM colliderGroup is mapped to PMX groups 1, 2, 3, ...
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
    // Unassigned colliders fall back to group 1
    for g in &mut collider_pmx_group {
        if *g == 0 {
            *g = 1;
        }
    }

    // Colliders -> static rigid bodies that follow their bone
    for (ci, collider) in spring_bone.colliders.iter().enumerate() {
        let node_idx = collider.node as usize;
        let bone_idx = node_to_bone.get(&node_idx).copied();

        let global_mat = bone_idx
            .map(|bi| bones[bi].global_mat)
            .unwrap_or(glam::Mat4::IDENTITY);
        let pmx_group = collider_pmx_group.get(ci).copied().unwrap_or(1);

        // Colliders only collide with the spring group (G15)
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

    // SpringChain -> dynamic rigid bodies + joints
    for spring in &spring_bone.springs {
        let joints = &spring.joints;
        if joints.is_empty() {
            continue;
        }

        // Warn about a center node (not supported in PMX)
        if let Some(center) = spring.center {
            log::warn!(
                "Spring \"{}\" center node ({}) is not supported in PMX (ignored)",
                spring.name.as_deref().unwrap_or("?"),
                center
            );
        }

        log::debug!(
            "spring \"{}\" ({} joints):",
            spring.name.as_deref().unwrap_or("?"),
            joints.len()
        );

        // Build the mask from the collider groups this spring references
        let spring_mask = if let Some(ref cg_refs) = spring.collider_groups {
            build_spring_mask(cg_refs, num_collider_groups)
        } else {
            0xFFFF_u16 // No collider-group references -> no collisions
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

            // Compute the distance to the next joint
            let next_pos = if ji + 1 < joints.len() {
                let next_node = joints[ji + 1].node as usize;
                node_to_bone
                    .get(&next_node)
                    .map(|&bi| bones[bi].position)
                    .unwrap_or(bone.position + Vec3::new(0.0, -0.07, 0.0))
            } else {
                // Append a virtual tail for the leaf (spec-compliant: 7 cm down)
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

/// Return the Euler angles that align the rigid body's local Y axis with the bone direction.
///
/// 1. Y axis = bone direction (negate when its Y component is negative).
/// 2. X axis = Y x Z unit vector.
/// 3. Z axis = X x Y.
/// 4. ZXY Euler decomposition (R = Rz * Rx * Ry).
fn bone_rotation(from_pmx: Vec3, to_pmx: Vec3) -> Vec3 {
    let mut dir = (to_pmx - from_pmx).normalize_or_zero();
    if dir.length_squared() < 1e-6 {
        return Vec3::ZERO;
    }

    // Flip the direction when the Y component is negative (rigid Y axis is always biased up)
    if dir.y < 0.0 {
        dir = -dir;
    }

    // Build the basis: Y = dir, X = Y x Z unit, Z = X x Y
    let y_axis = dir;
    let x_raw = y_axis.cross(Vec3::Z);
    let x_axis = if x_raw.length_squared() < 1e-6 {
        // When the Y axis is parallel to Z, fall back to deriving X from the X axis
        y_axis.cross(Vec3::X).normalize()
    } else {
        x_raw.normalize()
    };
    let z_axis = x_axis.cross(y_axis).normalize();

    // YXZ intrinsic = ZXY extrinsic Euler decomposition (R = Rz * Rx * Ry).
    // D3DX row-major: v * Ry * Rx * Rz -> glam column-major: Rz * Rx * Ry.
    let mat = glam::Mat3::from_cols(x_axis, y_axis, z_axis);
    let quat = glam::Quat::from_mat3(&mat);
    let (ry, rx, rz) = quat.to_euler(glam::EulerRot::YXZ);
    Vec3::new(rx, ry, rz)
}
