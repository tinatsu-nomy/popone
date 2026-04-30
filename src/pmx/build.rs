use crate::error::{PoponeError, Result};
use glam::Vec3;
use rust_i18n::t;
use std::collections::HashMap;
use std::f32::consts::PI;

use crate::convert::bone_map::vrm_bone_to_pmx_name;
use crate::convert::coord::{
    gltf_normal_to_pmx, gltf_normal_to_pmx_v0, gltf_pos_to_pmx, gltf_pos_to_pmx_v0,
};
use crate::convert::material::ir_material_to_pmx;
use crate::intermediate::types::{CullMode, IrModel, IrMorphKind, IrTexture, RigidShape};
use crate::pmx::types::*;

/// Auto-determine index size (vertex: unsigned)
pub fn vertex_idx_size(n: usize) -> u8 {
    if n <= 255 {
        1
    } else if n <= 65535 {
        2
    } else {
        4
    }
}

/// Auto-determine index size (others: signed)
pub fn idx_size(n: usize) -> u8 {
    if n <= 127 {
        1
    } else if n <= 32767 {
        2
    } else {
        4
    }
}

/// PMX model build options
#[derive(Debug, Clone)]
pub struct PmxBuildOptions {
    /// Align rigid body rotation to bone direction
    pub align_rigid_rotation: bool,
    /// Do not output physics (rigid bodies / joints)
    pub no_physics: bool,
    /// Skip standard bone insertion (preserve original bone structure)
    pub raw_structure: bool,
    /// PMX output scale factor (default: 1.0)
    pub scale: f32,
}

impl Default for PmxBuildOptions {
    fn default() -> Self {
        Self {
            align_rigid_rotation: false,
            no_physics: false,
            raw_structure: false,
            scale: 1.0,
        }
    }
}

pub fn build_pmx_model(ir: &IrModel) -> Result<(PmxModel, Vec<IrTexture>)> {
    build_pmx_model_with_options(ir, &PmxBuildOptions::default())
}

#[allow(clippy::field_reassign_with_default)]
pub fn build_pmx_model_with_options(
    ir: &IrModel,
    options: &PmxBuildOptions,
) -> Result<(PmxModel, Vec<IrTexture>)> {
    log::info!("=== PMX model build start ===");
    log::info!("Model name: {}", ir.name);
    log::info!("Source format: {}", ir.source_format.label());

    // Input VRM statistics
    log::info!("Input: bones={}, meshes={}, vertices={}, faces={}, materials={}, textures={}, morphs={}, rigidbodies={}, joints={}",
        ir.bones.len(), ir.meshes.len(), ir.total_vertices(), ir.total_faces(),
        ir.materials.len(), ir.textures.len(), ir.morphs.len(),
        ir.physics.rigid_bodies.len(), ir.physics.joints.len());

    // Mesh details
    log::debug!("--- Mesh list ---");
    for (i, mesh) in ir.meshes.iter().enumerate() {
        log::debug!(
            "  [{:2}] vertices={:5}, faces={:5}, material_idx={}",
            i,
            mesh.vertices.len(),
            mesh.indices.len() / 3,
            mesh.material_index
        );
    }

    let mut model = PmxModel::default();

    // Model info
    model.model_info = PmxModelInfo {
        name: ir.name.clone(),
        name_en: ir.name.clone(),
        comment: ir.comment.clone(),
        comment_en: String::new(),
    };

    // Texture paths (relative to textures\ folder, Windows separator)
    model.textures = ir
        .textures
        .iter()
        .map(|t| format!("textures\\{}", t.filename))
        .collect();
    log::debug!("--- Texture list ---");
    for (i, tex) in ir.textures.iter().enumerate() {
        log::debug!(
            "  [{:2}] {} ({} {}bytes)",
            i,
            tex.filename,
            tex.mime_type,
            tex.data.len()
        );
    }

    // Material -> texture index mapping
    let mat_to_tex: Vec<Option<i32>> = ir
        .materials
        .iter()
        .map(|m| m.texture_index.map(|i| i as i32))
        .collect();

    // Materials (including toon texture generation)
    // Collect existing texture names (to avoid collisions with toon names)
    let base_tex_count = ir.textures.len();
    let mut used_names: std::collections::HashSet<String> =
        ir.textures.iter().map(|t| t.filename.clone()).collect();
    let mut toon_textures: Vec<IrTexture> = Vec::new();
    model.materials = ir
        .materials
        .iter()
        .enumerate()
        .map(|(i, m)| {
            ir_material_to_pmx(
                m,
                mat_to_tex[i],
                &mut toon_textures,
                base_tex_count,
                &mut used_names,
            )
        })
        .collect();

    // Append generated toon textures to the texture list
    if !toon_textures.is_empty() {
        log::info!(
            "Generated {} toon textures (indices {}..{})",
            toon_textures.len(),
            base_tex_count,
            base_tex_count + toon_textures.len() - 1
        );
        for tex in &toon_textures {
            model.textures.push(format!("textures\\{}", tex.filename));
        }
    }

    // Material detail log
    log::debug!("--- Material list ---");
    for (i, mat) in ir.materials.iter().enumerate() {
        log::debug!("  [{:2}] \"{}\" diffuse=({:.2},{:.2},{:.2},{:.2}) tex={:?} double={} shader={} edge={:.3}",
            i, mat.name,
            mat.diffuse.x, mat.diffuse.y, mat.diffuse.z, mat.diffuse.w,
            mat.texture_index, mat.cull_mode != CullMode::Back, mat.shader_family, mat.edge_size);
    }
    let mtoon_count = ir.materials.iter().filter(|m| m.is_mtoon()).count();
    let double_count = ir
        .materials
        .iter()
        .filter(|m| m.cull_mode != CullMode::Back)
        .count();
    let edge_count = ir.materials.iter().filter(|m| m.edge_size > 0.0).count();
    log::info!(
        "Materials: {} (MToon={}, double_sided={}, has_edge={})",
        ir.materials.len(),
        mtoon_count,
        double_count,
        edge_count
    );

    let scale = options.scale;
    if (scale - 1.0).abs() > f32::EPSILON {
        log::info!("PMX output scale: {:.3}", scale);
    }

    // Bone conversion
    model.bones = build_bones(ir, options.raw_structure, scale);

    // If there are zero bones, create one dummy bone at the origin using the model name
    let no_source_bones = model.bones.is_empty();
    if no_source_bones {
        log::info!(
            "No bones found, creating dummy bone '{}' at origin",
            ir.name
        );
        model.bones.push(PmxBone {
            name: ir.name.clone(),
            name_en: ir.name.clone(),
            position: Vec3::ZERO,
            parent_index: -1,
            deform_layer: 0,
            flags: BONE_FLAG_ROTATABLE | BONE_FLAG_OPERABLE | BONE_FLAG_VISIBLE,
            tail: BoneTail::Offset(Vec3::ZERO),
            ik: None,
            grant: None,
        });
    }

    // Vertex / face consolidation
    let (vertices, faces, mat_face_counts) =
        build_vertices_and_faces(ir, ir.source_format.is_vrm0(), scale);
    model.vertices = vertices;
    model.faces = faces;

    // Set face count per material
    for (i, mat) in model.materials.iter_mut().enumerate() {
        mat.face_count = mat_face_counts.get(i).copied().unwrap_or(0);
    }

    // Per-material face count log
    log::debug!("--- Faces per material ---");
    for (i, mat) in model.materials.iter().enumerate() {
        log::debug!(
            "  [{:2}] \"{}\" face_vertices={} (faces={})",
            i,
            mat.name,
            mat.face_count,
            mat.face_count / 3
        );
    }

    // Morph conversion
    model.morphs = build_morphs(ir, ir.source_format.is_vrm0(), scale);

    // Rigid bodies / joints
    if options.no_physics {
        log::info!("Skipping physics output (no_physics)");
    } else {
        model.rigid_bodies = build_rigid_bodies(ir, options.align_rigid_rotation, scale);
        model.joints = build_joints(ir, scale);
    }

    // After all data is ready, insert standard bones (vertex / rigid body / existing bone index adjustment is also done here)
    // Static meshes (OBJ/STL/DirectX, etc.) and FBX without humanoid mapping
    // do not have a single VRM humanoid bone name set.
    // MMD standard bones (center/groove/waist/leg IK/IK parent/IK tail) assume humanoid,
    // so treat them as ineligible for insertion to avoid spurious dummy leg IK and similar bones.
    let has_humanoid_mapping = ir.bones.iter().any(|b| b.vrm_bone_name.is_some());
    if options.raw_structure {
        log::info!("Skipping standard bone insertion (raw_structure)");
    } else if no_source_bones {
        log::info!("Skipping standard bone insertion (no original bones)");
    } else if !has_humanoid_mapping {
        log::info!("Skipping standard bone insertion (no humanoid bone mapping)");
    } else {
        insert_standard_bones(&mut model)?;
    }

    // Resolve duplicate bone names (NameDupliBones countermeasure)
    fix_duplicate_names(&mut model.bones);

    // Reorder according to deformation order (IllegalOrderBones countermeasure)
    sort_bones_topological(&mut model);

    // Log the final bone order after sorting
    log::debug!("=== Sorted bone list ({} bones) ===", model.bones.len());
    for (i, b) in model.bones.iter().enumerate() {
        log::debug!(
            "  [{:3}] \"{}\" (parent={:3}, layer={}, flags=0x{:04X})",
            i,
            b.name,
            b.parent_index,
            b.deform_layer,
            b.flags
        );
    }

    // Display frames are built after bone insertion and sorting (after indices are finalized)
    model.display_frames = build_display_frames(&model.bones, &model.morphs);

    // Display frame log
    log::debug!("--- Display frames ---");
    for (i, frame) in model.display_frames.iter().enumerate() {
        let bone_count = frame
            .elements
            .iter()
            .filter(|e| matches!(e, DisplayFrameElement::Bone(_)))
            .count();
        let morph_count = frame
            .elements
            .iter()
            .filter(|e| matches!(e, DisplayFrameElement::Morph(_)))
            .count();
        let special = if frame.is_special != 0 {
            " [特殊]"
        } else {
            ""
        };
        log::debug!(
            "  [{:1}] \"{}\" bones={}, morphs={}{}",
            i,
            frame.name,
            bone_count,
            morph_count,
            special
        );
    }

    // Final PMX model statistics
    log::info!("=== PMX model build complete ===");
    log::info!("Output PMX: bones={}, vertices={}, faces={}, materials={}, textures={}, morphs={}, rigidbodies={}, joints={}, frames={}",
        model.bones.len(), model.vertices.len(), model.faces.len(),
        model.materials.len(), model.textures.len(), model.morphs.len(),
        model.rigid_bodies.len(), model.joints.len(), model.display_frames.len());

    // Auto-determine header index sizes
    model.header = PmxHeader {
        version: 2.0,
        encoding: 0, // UTF16LE
        additional_uvs: 0,
        vertex_index_size: vertex_idx_size(model.vertices.len()),
        texture_index_size: idx_size(model.textures.len()),
        material_index_size: idx_size(model.materials.len()),
        bone_index_size: idx_size(model.bones.len()),
        morph_index_size: idx_size(model.morphs.len()),
        rigid_body_index_size: idx_size(model.rigid_bodies.len()),
    };

    Ok((model, toon_textures))
}

fn find_bone_idx(bones: &[PmxBone], name: &str) -> Option<i32> {
    bones.iter().position(|b| b.name == name).map(|i| i as i32)
}

fn apply_remap(idx: i32, remap: &[i32]) -> i32 {
    if idx >= 0 && (idx as usize) < remap.len() {
        remap[idx as usize]
    } else {
        idx
    }
}

/// Bulk-remap all bone references in the model (inter-bone references, vertex weights, rigid bodies).
///
/// `f` is a closure that takes a bone index and returns the new index.
/// Whether to skip negative values (e.g. -1) is controlled by the closure.
fn remap_all_bone_indices(model: &mut PmxModel, f: impl Fn(i32) -> i32) {
    for bone in &mut model.bones {
        bone.parent_index = f(bone.parent_index);
        if let BoneTail::BoneIndex(i) = &mut bone.tail {
            *i = f(*i);
        }
        if let Some(ik) = &mut bone.ik {
            ik.target_bone = f(ik.target_bone);
            for link in &mut ik.links {
                link.bone_index = f(link.bone_index);
            }
        }
        if let Some(g) = &mut bone.grant {
            g.parent_index = f(g.parent_index);
        }
    }
    for vtx in &mut model.vertices {
        match &mut vtx.weight {
            PmxWeightType::Bdef1 { bone } => {
                *bone = f(*bone);
            }
            PmxWeightType::Bdef2 { bone1, bone2, .. } => {
                *bone1 = f(*bone1);
                *bone2 = f(*bone2);
            }
            PmxWeightType::Bdef4 { bones, .. } => {
                for b in bones.iter_mut() {
                    *b = f(*b);
                }
            }
        }
    }
    for rb in &mut model.rigid_bodies {
        rb.bone_index = f(rb.bone_index);
    }
}

/// Move a bone from index `from` to `to`, updating all bone references, vertex weights, and rigid bodies in the model
fn move_bone_in_model(model: &mut PmxModel, from: usize, to: usize) {
    if from == to {
        return;
    }
    let n = model.bones.len();
    let mut remap: Vec<i32> = (0..n as i32).collect();
    if from < to {
        for (i, slot) in remap[(from + 1)..=to].iter_mut().enumerate() {
            *slot = (from + i) as i32;
        }
        remap[from] = to as i32;
    } else {
        for (i, slot) in remap[to..from].iter_mut().enumerate() {
            *slot = (to + i + 1) as i32;
        }
        remap[from] = to as i32;
    }

    remap_all_bone_indices(model, |idx| apply_remap(idx, &remap));

    let bone = model.bones.remove(from);
    model.bones.insert(to, bone);
}

fn insert_standard_bones(model: &mut PmxModel) -> Result<()> {
    log::debug!(
        "=== insert_standard_bones start (existing bones: {}) ===",
        model.bones.len()
    );

    // Bone name -> index reverse-lookup map (optimizes O(n) linear scan to O(1))
    // For duplicate names, keep the first occurrence (same semantics as position())
    fn build_bone_map(bones: &[PmxBone]) -> HashMap<String, usize> {
        let mut map = HashMap::with_capacity(bones.len());
        for (i, b) in bones.iter().enumerate() {
            map.entry(b.name.clone()).or_insert(i);
        }
        map
    }

    let mut bone_map = build_bone_map(&model.bones);

    // 1. Capture positions and indices before shifting
    let hips_y = bone_map
        .get("下半身")
        .map(|&i| model.bones[i].position.y)
        .unwrap_or(10.0);

    let l_ankle = bone_map
        .get("左足首")
        .map(|&i| model.bones[i].position)
        .unwrap_or(Vec3::new(-2.5, 2.0, 0.0));
    let r_ankle = bone_map
        .get("右足首")
        .map(|&i| model.bones[i].position)
        .unwrap_or(Vec3::new(2.5, 2.0, 0.0));

    let has_toes = bone_map.contains_key("左つま先") && bone_map.contains_key("右つま先");

    let l_toe = bone_map
        .get("左つま先")
        .map(|&i| model.bones[i].position)
        .unwrap_or(Vec3::new(l_ankle.x, l_ankle.y - 1.5, l_ankle.z + 3.0));
    let r_toe = bone_map
        .get("右つま先")
        .map(|&i| model.bones[i].position)
        .unwrap_or(Vec3::new(r_ankle.x, r_ankle.y - 1.5, r_ankle.z + 3.0));

    // [B-2] Waist bone position (per the semi-standard plugin: lerp("下半身".y, "右足".y, 0.6))
    let r_leg_y = bone_map
        .get("右足")
        .map(|&i| model.bones[i].position.y)
        .unwrap_or(hips_y);
    let waist_y = hips_y * 0.4 + r_leg_y * 0.6;
    let waist_z = hips_y * 0.02;

    // Total number of bones inserted at the head (only the 4: "全ての親"/"センター"/"グルーブ"/"腰")
    // IK bones are appended to the end (per Animasa / Miku Ver2 convention)
    let n = 4i32;

    log::debug!(
        "[step1] \"lower_body\".y={:.3}, \"waist\"y={:.3}(z={:.3}), has_toe={}",
        hips_y,
        waist_y,
        waist_z,
        has_toes
    );
    log::debug!(
        "[step1] ankle_L=({:.3},{:.3},{:.3}), ankle_R=({:.3},{:.3},{:.3})",
        l_ankle.x,
        l_ankle.y,
        l_ankle.z,
        r_ankle.x,
        r_ankle.y,
        r_ankle.z
    );
    log::debug!(
        "[step2] standard bones added={} -> shifting existing indices by +{}",
        n,
        n
    );

    // 2,4,5. Shift all indices of existing bones, vertex weights, and rigid bodies by +n
    remap_all_bone_indices(model, |idx| if idx >= 0 { idx + n } else { idx });
    log::debug!(
        "[step2,4,5] shifting all bone refs, vertex weights, rigidbody bone_index by +{}",
        n
    );

    // 3. Reparent "下半身" and "上半身" to "腰" (index 3)
    log::debug!("[step3] \"lower_body\" & \"upper_body\" parent -> \"waist\"(idx=3)");
    for bone in model.bones.iter_mut() {
        if bone.name == "下半身" || bone.name == "上半身" {
            bone.parent_index = 3;
        }
    }

    // 3.5 Explicitly set "上半身"'s tail to "上半身2" (eliminates dependency on children order, fixes bone direction)
    //     Note: this runs before concatenation, so add +n to the in-VRM Vec position to obtain the final index
    {
        let upper2_idx = bone_map.get("上半身2").map(|&i| i as i32);
        if let Some(idx) = upper2_idx {
            if let Some(b) = model.bones.iter_mut().find(|b| b.name == "上半身") {
                b.tail = BoneTail::BoneIndex(idx + n);
                b.flags |= BONE_FLAG_TAIL_IS_BONE;
                log::debug!(
                    "[step3.5] \"upper_body\" tail -> \"upper_body2\"(idx={})",
                    idx + n
                );
            }
        }
    }

    // 6. Build the 4 standard bones ("全ての親"/"センター"/"グルーブ"/"腰")
    // IK bones are appended to the end (step18)
    let base_flags = BONE_FLAG_ROTATABLE | BONE_FLAG_VISIBLE | BONE_FLAG_OPERABLE;
    let trans_flags = base_flags | BONE_FLAG_TRANSLATABLE;

    let mut new_bones: Vec<PmxBone> = Vec::with_capacity(4);

    // 0: "全ての親"
    new_bones.push(PmxBone {
        name: "全ての親".to_string(),
        name_en: "master".to_string(),
        position: Vec3::ZERO,
        parent_index: -1,
        deform_layer: 0,
        flags: trans_flags | BONE_FLAG_TAIL_IS_BONE,
        tail: BoneTail::BoneIndex(1),
        ik: None,
        grant: None,
    });

    // 1: "センター"
    new_bones.push(PmxBone {
        name: "センター".to_string(),
        name_en: "center".to_string(),
        position: Vec3::new(0.0, hips_y, 0.0),
        parent_index: 0,
        deform_layer: 0,
        flags: trans_flags | BONE_FLAG_TAIL_IS_BONE,
        tail: BoneTail::BoneIndex(2),
        ik: None,
        grant: None,
    });

    // 2: "グルーブ"
    new_bones.push(PmxBone {
        name: "グルーブ".to_string(),
        name_en: "groove".to_string(),
        position: Vec3::new(0.0, hips_y, 0.0),
        parent_index: 1,
        deform_layer: 0,
        flags: trans_flags,
        tail: BoneTail::Offset(Vec3::new(0.0, 2.0, 0.0)),
        ik: None,
        grant: None,
    });

    // 3: "腰" (rotation only, no translation)
    new_bones.push(PmxBone {
        name: "腰".to_string(),
        name_en: "waist".to_string(),
        position: Vec3::new(0.0, waist_y, waist_z),
        parent_index: 2,
        deform_layer: 0,
        flags: base_flags,
        tail: BoneTail::Offset(Vec3::new(0.0, 2.0, 0.0)),
        ik: None,
        grant: None,
    });

    log::debug!("[step6] built {} standard bones:", new_bones.len());
    for (i, b) in new_bones.iter().enumerate() {
        log::debug!(
            "  [{:2}] \"{}\" pos=({:.3},{:.3},{:.3})",
            i,
            b.name,
            b.position.x,
            b.position.y,
            b.position.z
        );
    }

    // Append existing bones afterward and replace
    new_bones.append(&mut model.bones);
    model.bones = new_bones;
    log::debug!(
        "[step6] concatenated existing bones -> total {} bones",
        model.bones.len()
    );
    bone_map = build_bone_map(&model.bones);

    // 9. Place the upper-body group / "首" / "頭" / "下半身" right after IK (index n)
    // Order: IK -> "上半身" -> "上半身2" -> "上半身3" (if present) -> "首" -> "頭" -> "下半身" -> ... (per Miku Ver2)
    log::debug!("[step9] aligning upper body group after IK (idx={})", n);
    let mut next_target = n as usize;
    for name in ["上半身", "上半身2", "上半身3", "首", "頭"] {
        if let Some(&cur_idx) = bone_map.get(name) {
            if cur_idx != next_target {
                log::debug!("[step9]   \"{}\" #{} -> #{}", name, cur_idx, next_target);
                move_bone_in_model(model, cur_idx, next_target);
                bone_map = build_bone_map(&model.bones);
            }
            next_target += 1;
        }
    }
    if let Some(&cur_idx) = bone_map.get("下半身") {
        if cur_idx != next_target {
            log::debug!("[step9]   \"lower_body\" #{} -> #{}", cur_idx, next_target);
            move_bone_in_model(model, cur_idx, next_target);
            bone_map = build_bone_map(&model.bones);
        }
    }

    // 10. Invert the "下半身" bone
    // (1) Swap the absolute positions of position and tail (bone now points top->bottom)
    // (2) Set parent to "腰" (verify)
    {
        let waist_idx = bone_map.get("腰").map(|&i| i as i32);
        let lower_idx = bone_map.get("下半身").copied();
        if let Some(li) = lower_idx {
            if let Some(wi) = waist_idx {
                model.bones[li].parent_index = wi;
            }
            let old_pos = model.bones[li].position;
            let tail_abs = match model.bones[li].tail.clone() {
                BoneTail::BoneIndex(ti) => model
                    .bones
                    .get(ti as usize)
                    .map(|b| b.position)
                    .unwrap_or(old_pos),
                BoneTail::Offset(off) => old_pos + off,
            };
            log::debug!(
                "[step10] \"lower_body\" inversion: pos ({:.3},{:.3},{:.3}) <-> tail ({:.3},{:.3},{:.3})",
                old_pos.x,
                old_pos.y,
                old_pos.z,
                tail_abs.x,
                tail_abs.y,
                tail_abs.z
            );
            model.bones[li].position = tail_abs;
            model.bones[li].tail = BoneTail::Offset(old_pos - tail_abs);
            model.bones[li].flags &= !BONE_FLAG_TAIL_IS_BONE;
        }
    }

    // 11. [B-1] Add waist-cancel bones -> add_waist_cancel_bones()
    add_waist_cancel_bones(model)?;

    // 12-13. [C] Leg D-bone group + toe-EX bones -> add_d_and_toe_ex_bones()
    add_d_and_toe_ex_bones(model, has_toes);

    // 14. [C] Reparent IK-influenced bone children to the D-bones -> reparent_d_bone_children()
    reparent_d_bone_children(model);

    // step 15: Add arm-twist / wrist-twist bones
    log::debug!("=== [step15] arm twist & wrist twist bones added ===");
    add_twist_bones(model);
    log::debug!("=== [step15] done, bone count: {} ===", model.bones.len());

    // step 16: Add shoulder-cancel bones
    log::debug!("=== [step16] shoulder cancel bones added ===");
    add_shoulder_cancel_bones(model)?;
    log::debug!("=== [step16] done, bone count: {} ===", model.bones.len());

    // step 17: Append IK bone group at the end -> add_ik_bones()
    add_ik_bones(model, l_ankle, r_ankle, l_toe, r_toe, has_toes);
    log::debug!("=== [step17] done, bone count: {} ===", model.bones.len());

    // Bones changed extensively in steps 11-17, so rebuild the map
    bone_map = build_bone_map(&model.bones);

    // step 18: Align D-bone group / toe-EX bones after the IK bones at the very end (right -> left, per Animasa / Miku Ver2)
    // Since IK bones were added first, D-bones get higher indices than IK and the IK -> D order is preserved after sorting
    log::debug!("=== [step18] aligning D-bones to end (right->left) ===");
    {
        let d_end_order: &[&str] = if has_toes {
            &[
                "右足D",
                "右ひざD",
                "右足首D",
                "右足先EX",
                "左足D",
                "左ひざD",
                "左足首D",
                "左足先EX",
            ]
        } else {
            &["右足D", "右ひざD", "左足D", "左ひざD"]
        };
        for &name in d_end_order {
            if let Some(&cur_idx) = bone_map.get(name) {
                let last = model.bones.len() - 1;
                if cur_idx != last {
                    log::debug!("[step18] \"{}\" #{} -> #{} (end)", name, cur_idx, last);
                    move_bone_in_model(model, cur_idx, last);
                    bone_map = build_bone_map(&model.bones);
                }
            }
        }
    }
    log::debug!("=== [step18] done, bone count: {} ===", model.bones.len());

    // Final bone list (all entries)
    log::debug!("=== Bone list ({} bones) ===", model.bones.len());
    for (i, b) in model.bones.iter().enumerate() {
        log::debug!(
            "  [{:3}] \"{}\" (parent={:3}, layer={}, flags=0x{:04X})",
            i,
            b.name,
            b.parent_index,
            b.deform_layer,
            b.flags
        );
    }
    log::debug!("=== insert_standard_bones complete ===");
    Ok(())
}

/// After inserting a bone at position `insert_at`, shift all references at or after `insert_at` by +1
fn shift_indices_after_insert(model: &mut PmxModel, insert_at: usize) {
    let threshold = insert_at as i32;
    remap_all_bone_indices(model, |idx| if idx >= threshold { idx + 1 } else { idx });
}

/// Project a vertex onto the parent->child direction and return t in [0,1] (0 = parent side, 1 = child side)
fn project_on_bone(vtx_pos: Vec3, start: Vec3, end: Vec3) -> f32 {
    let dir = end - start;
    let len_sq = dir.length_squared();
    if len_sq < 1e-6 {
        return 0.5;
    }
    ((vtx_pos - start).dot(dir) / len_sq).clamp(0.0, 1.0)
}

/// Split the parent bone (arm_idx) weight with the twist bone (twist_idx) using the projected value t
fn redistribute_twist_weight(
    vertices: &mut [PmxVertex],
    parent_pos: Vec3,
    child_pos: Vec3,
    arm_idx: i32,
    twist_idx: i32,
) {
    for vtx in vertices.iter_mut() {
        let t = project_on_bone(vtx.position, parent_pos, child_pos);
        if t <= 0.01 {
            continue;
        }

        match &mut vtx.weight {
            PmxWeightType::Bdef1 { bone } => {
                if *bone != arm_idx {
                    continue;
                }
                // Bdef1{arm} → Bdef2{arm:1-t, twist:t}
                vtx.weight = PmxWeightType::Bdef2 {
                    bone1: arm_idx,
                    bone2: twist_idx,
                    weight1: 1.0 - t,
                };
            }
            PmxWeightType::Bdef2 {
                bone1,
                bone2,
                weight1,
            } => {
                let (w_arm, other_bone, w_other) = if *bone1 == arm_idx {
                    (*weight1, *bone2, 1.0 - *weight1)
                } else if *bone2 == arm_idx {
                    (1.0 - *weight1, *bone1, *weight1)
                } else {
                    continue;
                };
                // Bdef2{arm,other} → Bdef4{arm:w*(1-t), twist:w*t, other, -1:0}
                vtx.weight = PmxWeightType::Bdef4 {
                    bones: [arm_idx, twist_idx, other_bone, -1],
                    weights: [w_arm * (1.0 - t), w_arm * t, w_other, 0.0],
                };
            }
            PmxWeightType::Bdef4 { bones, weights } => {
                let Some(arm_slot) = bones.iter().position(|&b| b == arm_idx) else {
                    continue;
                };
                let w = weights[arm_slot];
                if w < 0.001 {
                    continue;
                }
                // Empty slot = bone == -1 or weight ~ 0 (excluding arm_slot)
                let Some(empty) =
                    (0..4).find(|&i| i != arm_slot && (bones[i] == -1 || weights[i] < 1e-6))
                else {
                    continue; // All 4 slots in use -> skip
                };
                weights[arm_slot] = w * (1.0 - t);
                bones[empty] = twist_idx;
                weights[empty] = w * t;
            }
        }
    }
}

/// [step11] Add waist-cancel bones (right and left) and place them immediately before "右足"/"左足"
fn add_waist_cancel_bones(model: &mut PmxModel) -> Result<()> {
    let waist_idx = model
        .bones
        .iter()
        .position(|b| b.name == "腰")
        .map(|i| i as i32);
    let r_leg_info = model
        .bones
        .iter()
        .find(|b| b.name == "右足")
        .map(|b| (b.position, b.parent_index));
    let l_leg_info = model
        .bones
        .iter()
        .find(|b| b.name == "左足")
        .map(|b| (b.position, b.parent_index));

    if let (Some(waist_idx), Some((r_pos, r_parent)), Some((l_pos, l_parent))) =
        (waist_idx, r_leg_info, l_leg_info)
    {
        let cancel_flags = BONE_FLAG_ROTATABLE | BONE_FLAG_ROTATION_GRANT;

        log::debug!(
            "[step11] \"waist_cancel_R\" added pos=({:.3},{:.3},{:.3})",
            r_pos.x,
            r_pos.y,
            r_pos.z
        );
        // Append "腰キャンセル右" at the end, then move it just before "右足"
        let r_cancel_at = model.bones.len();
        model.bones.push(PmxBone {
            name: "腰キャンセル右".to_string(),
            name_en: "waist cancel_R".to_string(),
            position: r_pos,
            parent_index: r_parent,
            deform_layer: 0,
            flags: cancel_flags,
            tail: BoneTail::Offset(Vec3::ZERO),
            ik: None,
            grant: Some(PmxGrant {
                parent_index: waist_idx,
                ratio: -1.0,
            }),
        });
        if let Some(b) = model.bones.iter_mut().find(|b| b.name == "右足") {
            b.parent_index = r_cancel_at as i32;
        }
        let r_leg_at = model
            .bones
            .iter()
            .position(|b| b.name == "右足")
            .ok_or_else(|| {
                PoponeError::Build(
                    t!("error.pmx_build.bone_not_found", name = "右足".to_string()).to_string(),
                )
            })?;
        move_bone_in_model(model, r_cancel_at, r_leg_at);

        log::debug!(
            "[step11] \"waist_cancel_L\" added pos=({:.3},{:.3},{:.3})",
            l_pos.x,
            l_pos.y,
            l_pos.z
        );
        // Append "腰キャンセル左" at the end, then move it just before "左足" (using the index after the right-side move)
        let waist_idx_now = model
            .bones
            .iter()
            .position(|b| b.name == "腰")
            .map(|i| i as i32)
            .unwrap_or(waist_idx);
        let l_parent_now = model
            .bones
            .iter()
            .find(|b| b.name == "左足")
            .map(|b| b.parent_index)
            .unwrap_or(l_parent);
        let l_cancel_at = model.bones.len();
        model.bones.push(PmxBone {
            name: "腰キャンセル左".to_string(),
            name_en: "waist cancel_L".to_string(),
            position: l_pos,
            parent_index: l_parent_now,
            deform_layer: 0,
            flags: cancel_flags,
            tail: BoneTail::Offset(Vec3::ZERO),
            ik: None,
            grant: Some(PmxGrant {
                parent_index: waist_idx_now,
                ratio: -1.0,
            }),
        });
        if let Some(b) = model.bones.iter_mut().find(|b| b.name == "左足") {
            b.parent_index = l_cancel_at as i32;
        }
        let l_leg_at = model
            .bones
            .iter()
            .position(|b| b.name == "左足")
            .ok_or_else(|| {
                PoponeError::Build(
                    t!("error.pmx_build.bone_not_found", name = "左足".to_string()).to_string(),
                )
            })?;
        move_bone_in_model(model, l_cancel_at, l_leg_at);
    }
    Ok(())
}

/// [step12-13] Add the leg D-bone group (D auxiliary bones under IK influence) and toe-EX bones
fn add_d_and_toe_ex_bones(model: &mut PmxModel, has_toes: bool) {
    // 12. [C] Leg D-bone group (D auxiliary bones under IK influence)
    // Duplicate each IK-linked bone (a) and create a D auxiliary that follows it via rotation grant (x1.0).
    // The parent/child relationships of the original bone (a) are not changed at all.
    // Only D-bones form their own dedicated chain among D-bones:
    //   if a D-bone corresponding to the parent bone already exists, use it as the parent.
    {
        let d_pairs: &[(&str, &str, &str)] = if has_toes {
            &[
                ("左足", "左足D", "leg_LD"),
                ("左ひざ", "左ひざD", "knee_LD"),
                ("右足", "右足D", "leg_RD"),
                ("右ひざ", "右ひざD", "knee_RD"),
                ("左足首", "左足首D", "ankle_LD"),
                ("右足首", "右足首D", "ankle_RD"),
            ]
        } else {
            &[
                ("左足", "左足D", "leg_LD"),
                ("左ひざ", "左ひざD", "knee_LD"),
                ("右足", "右足D", "leg_RD"),
                ("右ひざ", "右ひざD", "knee_RD"),
            ]
        };

        for &(src_name, d_name, d_en) in d_pairs {
            let Some(src_idx) = find_bone_idx(&model.bones, src_name) else {
                continue;
            };
            let src_pos = model.bones[src_idx as usize].position;
            let src_parent = model.bones[src_idx as usize].parent_index;

            // Parent of the D auxiliary: if a D-bone corresponding to the original bone's parent already exists, use it
            // (e.g. parent of "左ひざD" -> parent of "左ひざ" is "左足"; if "左足"+"D"="左足D" exists, use "左足D")
            let d_parent = if src_parent >= 0 {
                let parent_d_name = format!("{}D", &model.bones[src_parent as usize].name);
                find_bone_idx(&model.bones, &parent_d_name).unwrap_or(src_parent)
            } else {
                src_parent
            };

            log::debug!(
                "[step12] \"{}\" added pos=({:.3},{:.3},{:.3}) grant<-\"{}\"(idx={})",
                d_name,
                src_pos.x,
                src_pos.y,
                src_pos.z,
                src_name,
                src_idx
            );
            // Append D auxiliary at the end (will be aligned to the end in step17)
            model.bones.push(PmxBone {
                name: d_name.to_string(),
                name_en: d_en.to_string(),
                position: src_pos,
                parent_index: d_parent,
                deform_layer: 1,
                flags: BONE_FLAG_ROTATABLE | BONE_FLAG_ROTATION_GRANT,
                tail: BoneTail::Offset(Vec3::ZERO),
                ik: None,
                grant: Some(PmxGrant {
                    parent_index: src_idx,
                    ratio: 1.0,
                }),
            });
        }
    }

    // 13. Add toe-EX bones (immediately after "左足首D" / "右足首D")
    // The parent of the toe-EX is the ankle-D (the D-bone corresponding to "足首" under IK influence).
    // Do not change the parent of "左つま先" / "右つま先" (per Miku convention: toe parent stays at the ankle).
    if has_toes {
        for (ex_name, ex_en, parent_d) in [
            ("左足先EX", "ex toe_L", "左足首D"),
            ("右足先EX", "ex toe_R", "右足首D"),
        ] {
            let Some(parent_idx) = find_bone_idx(&model.bones, parent_d) else {
                continue;
            };
            let pos = model.bones[parent_idx as usize].position;
            log::debug!(
                "[step13] \"{}\" added pos=({:.3},{:.3},{:.3}) parent=\"{}\"(idx={})",
                ex_name,
                pos.x,
                pos.y,
                pos.z,
                parent_d,
                parent_idx
            );

            model.bones.push(PmxBone {
                name: ex_name.to_string(),
                name_en: ex_en.to_string(),
                position: pos,
                parent_index: parent_idx,
                deform_layer: 1,
                flags: BONE_FLAG_ROTATABLE | BONE_FLAG_VISIBLE | BONE_FLAG_OPERABLE,
                tail: BoneTail::Offset(Vec3::new(0.0, -1.0, 0.0)),
                ik: None,
                grant: None,
            });
            // (step17 will arrange them at the tail)
        }
    }
}

/// [step14] Reparent auxiliary bones whose parent is an IK-affected bone (leg/knee/ankle) to the
/// corresponding D bone, then propagate the deform layer recursively down the descendants.
fn reparent_d_bone_children(model: &mut PmxModel) {
    let remap_pairs: &[(&str, &str)] = &[
        ("左足", "左足D"),
        ("左ひざ", "左ひざD"),
        ("左足首", "左足首D"),
        ("右足", "右足D"),
        ("右ひざ", "右ひざD"),
        ("右足首", "右足首D"),
    ];

    let exclude: &[&str] = &[
        "左足",
        "左ひざ",
        "左足首",
        "左つま先",
        "右足",
        "右ひざ",
        "右足首",
        "右つま先",
        "左足D",
        "左ひざD",
        "左足首D",
        "右足D",
        "右ひざD",
        "右足首D",
        "左足先EX",
        "右足先EX",
    ];

    // Record the indices of bones whose deform layer actually changed
    let mut changed: std::collections::HashSet<usize> = std::collections::HashSet::new();

    for &(src_name, d_name) in remap_pairs {
        let Some(src_idx) = find_bone_idx(&model.bones, src_name) else {
            continue;
        };
        let Some(d_idx) = find_bone_idx(&model.bones, d_name) else {
            continue;
        };

        for (i, bone) in model.bones.iter_mut().enumerate() {
            if exclude.contains(&bone.name.as_str()) {
                continue;
            }
            if bone.parent_index == src_idx {
                bone.parent_index = d_idx;
                let old_layer = bone.deform_layer;
                let new_layer = bone.deform_layer.max(1);
                if new_layer != old_layer {
                    bone.deform_layer = new_layer;
                    changed.insert(i);
                    log::debug!(
                        "[step14] \"{}\" parent changed: \"{}\"(idx={}) -> \"{}\"(idx={}), layer {} -> {}",
                        bone.name,
                        src_name,
                        src_idx,
                        d_name,
                        d_idx,
                        old_layer,
                        new_layer
                    );
                } else {
                    log::debug!("[step14] \"{}\" parent changed: \"{}\"(idx={}) -> \"{}\"(idx={}), layer {} (unchanged)",
                        bone.name, src_name, src_idx, d_name, d_idx, bone.deform_layer);
                }
            }
        }
    }

    // Recursively propagate the deform layer to descendants of changed bones (parent -> child -> grandchild -> ...)
    loop {
        let mut any_updated = false;
        for i in 0..model.bones.len() {
            let parent_idx = model.bones[i].parent_index;
            if parent_idx < 0 {
                continue;
            }
            if changed.contains(&(parent_idx as usize)) {
                let parent_layer = model.bones[parent_idx as usize].deform_layer;
                if model.bones[i].deform_layer < parent_layer {
                    let old_layer = model.bones[i].deform_layer;
                    let bone_name = model.bones[i].name.clone();
                    let parent_name = model.bones[parent_idx as usize].name.clone();
                    model.bones[i].deform_layer = parent_layer;
                    changed.insert(i);
                    any_updated = true;
                    log::debug!(
                        "[step14] deform_layer propagation: \"{}\" {} -> {} (parent: \"{}\")",
                        bone_name,
                        old_layer,
                        parent_layer,
                        parent_name
                    );
                }
            }
        }
        if !any_updated {
            break;
        }
    }
}

/// [step17] Append the IK bone set ("足IK親" / "足ＩＫ" / "つま先ＩＫ" / "ＩＫ先") at the tail.
fn add_ik_bones(
    model: &mut PmxModel,
    l_ankle: Vec3,
    r_ankle: Vec3,
    l_toe: Vec3,
    r_toe: Vec3,
    has_toes: bool,
) {
    log::debug!("=== [step17] appending IK bones to end ===");

    let ik_bone_flags = BONE_FLAG_ROTATABLE
        | BONE_FLAG_VISIBLE
        | BONE_FLAG_OPERABLE
        | BONE_FLAG_IK
        | BONE_FLAG_TRANSLATABLE;
    let trans_flags_ik =
        BONE_FLAG_ROTATABLE | BONE_FLAG_VISIBLE | BONE_FLAG_OPERABLE | BONE_FLAG_TRANSLATABLE;

    // Look up the current indices after every move has finished
    let l_ankle_fi = find_bone_idx(&model.bones, "左足首");
    let r_ankle_fi = find_bone_idx(&model.bones, "右足首");
    let l_knee_fi = find_bone_idx(&model.bones, "左ひざ");
    let r_knee_fi = find_bone_idx(&model.bones, "右ひざ");
    let l_leg_fi = find_bone_idx(&model.bones, "左足");
    let r_leg_fi = find_bone_idx(&model.bones, "右足");
    let l_toe_fi = find_bone_idx(&model.bones, "左つま先");
    let r_toe_fi = find_bone_idx(&model.bones, "右つま先");

    // Pre-compute the placement indices of the appended bones.
    // Left-then-right order: "左足IK親"(+0), "左足ＩＫ"(+1), "右足IK親"(+2), "右足ＩＫ"(+3),
    //          [has_toes] "左つま先ＩＫ"(+4), "右つま先ＩＫ"(+5).
    // IK-tail bones: "左足ＩＫ先", "右足ＩＫ先" [, "左つま先ＩＫ先", "右つま先ＩＫ先"].
    let base = model.bones.len() as i32;
    let l_ik_parent_idx = base;
    let l_ik_idx = base + 1;
    let r_ik_parent_idx = base + 2;
    let r_ik_idx = base + 3;
    let (l_toe_ik_idx, r_toe_ik_idx, ik_tail_base) = if has_toes {
        (base + 4, base + 5, base + 6)
    } else {
        (-1, -1, base + 4)
    };
    let l_ik_tail_idx = ik_tail_base;
    let r_ik_tail_idx = ik_tail_base + 1;
    let l_toe_ik_tail_idx = ik_tail_base + 2;
    let r_toe_ik_tail_idx = ik_tail_base + 3;

    // Build IK data (referencing the post-move indices directly)
    let l_leg_ik = l_ankle_fi.map(|target| {
        let mut links = Vec::new();
        if let Some(ki) = l_knee_fi {
            links.push(IkLink {
                bone_index: ki,
                angle_limit: true,
                limit_min: Vec3::new(-PI, 0.0, 0.0),
                limit_max: Vec3::new(-0.005, 0.0, 0.0),
            });
        }
        if let Some(li) = l_leg_fi {
            links.push(IkLink {
                bone_index: li,
                angle_limit: false,
                limit_min: Vec3::ZERO,
                limit_max: Vec3::ZERO,
            });
        }
        PmxIk {
            target_bone: target,
            loop_count: 40,
            limit_angle: 2.0,
            links,
        }
    });
    let r_leg_ik = r_ankle_fi.map(|target| {
        let mut links = Vec::new();
        if let Some(ki) = r_knee_fi {
            links.push(IkLink {
                bone_index: ki,
                angle_limit: true,
                limit_min: Vec3::new(-PI, 0.0, 0.0),
                limit_max: Vec3::new(-0.005, 0.0, 0.0),
            });
        }
        if let Some(li) = r_leg_fi {
            links.push(IkLink {
                bone_index: li,
                angle_limit: false,
                limit_min: Vec3::ZERO,
                limit_max: Vec3::ZERO,
            });
        }
        PmxIk {
            target_bone: target,
            loop_count: 40,
            limit_angle: 2.0,
            links,
        }
    });
    let l_toe_ik = if has_toes {
        l_toe_fi.map(|target| {
            let mut links = Vec::new();
            if let Some(ai) = l_ankle_fi {
                links.push(IkLink {
                    bone_index: ai,
                    angle_limit: false,
                    limit_min: Vec3::ZERO,
                    limit_max: Vec3::ZERO,
                });
            }
            PmxIk {
                target_bone: target,
                loop_count: 3,
                limit_angle: 4.0,
                links,
            }
        })
    } else {
        None
    };
    let r_toe_ik = if has_toes {
        r_toe_fi.map(|target| {
            let mut links = Vec::new();
            if let Some(ai) = r_ankle_fi {
                links.push(IkLink {
                    bone_index: ai,
                    angle_limit: false,
                    limit_min: Vec3::ZERO,
                    limit_max: Vec3::ZERO,
                });
            }
            PmxIk {
                target_bone: target,
                loop_count: 3,
                limit_angle: 4.0,
                links,
            }
        })
    } else {
        None
    };

    // "左足IK親"
    model.bones.push(PmxBone {
        name: "左足IK親".to_string(),
        name_en: "leg IK parent_L".to_string(),
        position: Vec3::new(l_ankle.x, 0.0, 0.0),
        parent_index: 0,
        deform_layer: 0,
        flags: trans_flags_ik | BONE_FLAG_TAIL_IS_BONE,
        tail: BoneTail::BoneIndex(l_ik_idx),
        ik: None,
        grant: None,
    });
    // "左足ＩＫ" (tail -> "左足ＩＫ先")
    model.bones.push(PmxBone {
        name: "左足ＩＫ".to_string(),
        name_en: "leg IK_L".to_string(),
        position: l_ankle,
        parent_index: l_ik_parent_idx,
        deform_layer: 1,
        flags: ik_bone_flags | BONE_FLAG_TAIL_IS_BONE,
        tail: BoneTail::BoneIndex(l_ik_tail_idx),
        ik: l_leg_ik,
        grant: None,
    });
    // "右足IK親"
    model.bones.push(PmxBone {
        name: "右足IK親".to_string(),
        name_en: "leg IK parent_R".to_string(),
        position: Vec3::new(r_ankle.x, 0.0, 0.0),
        parent_index: 0,
        deform_layer: 0,
        flags: trans_flags_ik | BONE_FLAG_TAIL_IS_BONE,
        tail: BoneTail::BoneIndex(r_ik_idx),
        ik: None,
        grant: None,
    });
    // "右足ＩＫ" (tail -> "右足ＩＫ先")
    model.bones.push(PmxBone {
        name: "右足ＩＫ".to_string(),
        name_en: "leg IK_R".to_string(),
        position: r_ankle,
        parent_index: r_ik_parent_idx,
        deform_layer: 1,
        flags: ik_bone_flags | BONE_FLAG_TAIL_IS_BONE,
        tail: BoneTail::BoneIndex(r_ik_tail_idx),
        ik: r_leg_ik,
        grant: None,
    });

    if has_toes {
        // "左つま先ＩＫ" (tail -> "左つま先ＩＫ先")
        model.bones.push(PmxBone {
            name: "左つま先ＩＫ".to_string(),
            name_en: "toe IK_L".to_string(),
            position: l_toe,
            parent_index: l_ik_idx,
            deform_layer: 1,
            flags: ik_bone_flags | BONE_FLAG_TAIL_IS_BONE,
            tail: BoneTail::BoneIndex(l_toe_ik_tail_idx),
            ik: l_toe_ik,
            grant: None,
        });
        // "右つま先ＩＫ" (tail -> "右つま先ＩＫ先")
        model.bones.push(PmxBone {
            name: "右つま先ＩＫ".to_string(),
            name_en: "toe IK_R".to_string(),
            position: r_toe,
            parent_index: r_ik_idx,
            deform_layer: 1,
            flags: ik_bone_flags | BONE_FLAG_TAIL_IS_BONE,
            tail: BoneTail::BoneIndex(r_toe_ik_tail_idx),
            ik: r_toe_ik,
            grant: None,
        });
    }

    // IK-tail bones (used as display tails; hidden and non-operable)
    model.bones.push(PmxBone {
        name: "左足ＩＫ先".to_string(),
        name_en: "leg IK tail_L".to_string(),
        position: l_ankle + Vec3::new(0.0, 0.0, 1.0),
        parent_index: l_ik_idx,
        deform_layer: 1,
        flags: 0,
        tail: BoneTail::Offset(Vec3::ZERO),
        ik: None,
        grant: None,
    });
    model.bones.push(PmxBone {
        name: "右足ＩＫ先".to_string(),
        name_en: "leg IK tail_R".to_string(),
        position: r_ankle + Vec3::new(0.0, 0.0, 1.0),
        parent_index: r_ik_idx,
        deform_layer: 1,
        flags: 0,
        tail: BoneTail::Offset(Vec3::ZERO),
        ik: None,
        grant: None,
    });
    if has_toes {
        model.bones.push(PmxBone {
            name: "左つま先ＩＫ先".to_string(),
            name_en: "toe IK tail_L".to_string(),
            position: l_toe + Vec3::new(0.0, -1.0, 0.0),
            parent_index: l_toe_ik_idx,
            deform_layer: 1,
            flags: 0,
            tail: BoneTail::Offset(Vec3::ZERO),
            ik: None,
            grant: None,
        });
        model.bones.push(PmxBone {
            name: "右つま先ＩＫ先".to_string(),
            name_en: "toe IK tail_R".to_string(),
            position: r_toe + Vec3::new(0.0, -1.0, 0.0),
            parent_index: r_toe_ik_idx,
            deform_layer: 1,
            flags: 0,
            tail: BoneTail::Offset(Vec3::ZERO),
            ik: None,
            grant: None,
        });
    }
    log::debug!(
        "[step17] IK + IK tip bones added -> bone count: {}",
        model.bones.len()
    );
}

/// Add the four arm/wrist twist bones and redistribute the weights.
fn add_twist_bones(model: &mut PmxModel) {
    let pairs = [
        ("右腕", "右ひじ", "右腕捩", "arm twist_R"),
        ("左腕", "左ひじ", "左腕捩", "arm twist_L"),
        ("右ひじ", "右手首", "右手捩", "wrist twist_R"),
        ("左ひじ", "左手首", "左手捩", "wrist twist_L"),
    ];
    let base_flags = BONE_FLAG_ROTATABLE | BONE_FLAG_VISIBLE | BONE_FLAG_OPERABLE;

    for (parent_name, child_name, twist_jp, twist_en) in pairs {
        // 1. Look up the parent/child indices and positions from the current `bones`
        let Some(parent_idx) = find_bone_idx(&model.bones, parent_name) else {
            log::debug!("[step15] \"{}\" not found, skipping", parent_name);
            continue;
        };
        let Some(child_idx) = find_bone_idx(&model.bones, child_name) else {
            log::debug!("[step15] \"{}\" not found, skipping", child_name);
            continue;
        };
        let parent_pos = model.bones[parent_idx as usize].position;
        let child_pos = model.bones[child_idx as usize].position;
        let parent_layer = model.bones[parent_idx as usize].deform_layer;

        // 2. Twist-bone position = midpoint
        let twist_pos = parent_pos.lerp(child_pos, 0.5);

        log::debug!(
            "[step15] \"{}\" added pos=({:.3},{:.3},{:.3}) parent=\"{}\"({})",
            twist_jp,
            twist_pos.x,
            twist_pos.y,
            twist_pos.z,
            parent_name,
            parent_idx
        );

        // 3. Build the twist bone (parent = parent_idx; no children, no grant)
        let twist_bone = PmxBone {
            name: twist_jp.to_string(),
            name_en: twist_en.to_string(),
            position: twist_pos,
            parent_index: parent_idx,
            deform_layer: parent_layer,
            flags: base_flags,
            tail: BoneTail::Offset(Vec3::ZERO),
            ik: None,
            grant: None,
        };

        // 4. Insert immediately after the parent bone
        let insert_at = parent_idx as usize + 1;
        model.bones.insert(insert_at, twist_bone);

        // 5. Shift every reference at or after `insert_at` by +1 to account for the insertion.
        //    (The new bone's own `parent_index` stays put because parent_idx < insert_at.)
        shift_indices_after_insert(model, insert_at);

        // 6. Redistribute the weights.
        //    The parent bone stays at parent_idx after the shift (it sits before insert_at).
        //    The twist bone lives at insert_at.
        redistribute_twist_weight(
            &mut model.vertices,
            parent_pos,
            child_pos,
            parent_idx,       // arm_idx: still parent_idx after the shift
            insert_at as i32, // twist_idx
        );
    }
}

/// Add the shoulder-cancel bones ("肩P" / "肩C") on both sides.
/// "肩P": the user-facing parent of the shoulder.
/// "肩C": the parent of the arm; a grant bone that cancels "肩P"'s rotation with a ratio of -1.
fn add_shoulder_cancel_bones(model: &mut PmxModel) -> Result<()> {
    let pairs = [
        (
            "右肩",
            "右腕",
            "右肩P",
            "shoulderP_R",
            "右肩C",
            "shoulderC_R",
        ),
        (
            "左肩",
            "左腕",
            "左肩P",
            "shoulderP_L",
            "左肩C",
            "shoulderC_L",
        ),
    ];

    for (shoulder_name, arm_name, p_jp, p_en, c_jp, c_en) in pairs {
        // 1. Look up the shoulder/arm indices and positions
        let Some(shoulder_idx) = find_bone_idx(&model.bones, shoulder_name) else {
            log::debug!("[step16] \"{}\" not found, skipping", shoulder_name);
            continue;
        };
        let Some(arm_idx) = find_bone_idx(&model.bones, arm_name) else {
            log::debug!("[step16] \"{}\" not found, skipping", arm_name);
            continue;
        };

        let shoulder_pos = model.bones[shoulder_idx as usize].position;
        let shoulder_original_parent = model.bones[shoulder_idx as usize].parent_index;
        let arm_pos = model.bones[arm_idx as usize].position;

        // 2. Append "肩P" at the tail and move it just before the shoulder
        let p_flags = BONE_FLAG_ROTATABLE | BONE_FLAG_VISIBLE | BONE_FLAG_OPERABLE;
        let p_at = model.bones.len();
        model.bones.push(PmxBone {
            name: p_jp.to_string(),
            name_en: p_en.to_string(),
            position: shoulder_pos,
            parent_index: shoulder_original_parent,
            deform_layer: 0,
            flags: p_flags | BONE_FLAG_TAIL_IS_BONE,
            tail: BoneTail::BoneIndex(shoulder_idx), // tail -> shoulder
            ik: None,
            grant: None,
        });

        // Change the shoulder's parent to "肩P"
        model.bones[shoulder_idx as usize].parent_index = p_at as i32;

        // Move "肩P" to just before the shoulder
        let shoulder_now = model
            .bones
            .iter()
            .position(|b| b.name == shoulder_name)
            .ok_or_else(|| {
                PoponeError::Build(
                    t!(
                        "error.pmx_build.bone_not_found",
                        name = shoulder_name.to_string()
                    )
                    .to_string(),
                )
            })?;
        move_bone_in_model(model, p_at, shoulder_now);

        log::debug!(
            "[step16] \"{}\" added pos=({:.3},{:.3},{:.3}) parent={}",
            p_jp,
            shoulder_pos.x,
            shoulder_pos.y,
            shoulder_pos.z,
            shoulder_original_parent
        );

        // 3. Append "肩C" at the tail and move it just before the arm.
        //    "肩C"'s grant = "肩P" * (-1.0).
        let c_flags = BONE_FLAG_ROTATABLE | BONE_FLAG_ROTATION_GRANT;
        let shoulder_idx_now = find_bone_idx(&model.bones, shoulder_name).ok_or_else(|| {
            PoponeError::Build(
                t!(
                    "error.pmx_build.bone_not_found",
                    name = shoulder_name.to_string()
                )
                .to_string(),
            )
        })?;
        let p_idx_now = find_bone_idx(&model.bones, p_jp).ok_or_else(|| {
            PoponeError::Build(
                t!("error.pmx_build.bone_not_found", name = p_jp.to_string()).to_string(),
            )
        })?;

        let c_at = model.bones.len();
        model.bones.push(PmxBone {
            name: c_jp.to_string(),
            name_en: c_en.to_string(),
            position: arm_pos,
            parent_index: shoulder_idx_now,
            deform_layer: 0,
            flags: c_flags,
            tail: BoneTail::Offset(Vec3::ZERO),
            ik: None,
            grant: Some(PmxGrant {
                parent_index: p_idx_now,
                ratio: -1.0,
            }),
        });

        // Change the arm's parent to "肩C"
        let arm_idx_now = find_bone_idx(&model.bones, arm_name).ok_or_else(|| {
            PoponeError::Build(
                t!(
                    "error.pmx_build.bone_not_found",
                    name = arm_name.to_string()
                )
                .to_string(),
            )
        })?;
        model.bones[arm_idx_now as usize].parent_index = c_at as i32;

        // Move "肩C" to just before the arm
        let arm_now = model
            .bones
            .iter()
            .position(|b| b.name == arm_name)
            .ok_or_else(|| {
                PoponeError::Build(
                    t!(
                        "error.pmx_build.bone_not_found",
                        name = arm_name.to_string()
                    )
                    .to_string(),
                )
            })?;
        move_bone_in_model(model, c_at, arm_now);

        log::debug!(
            "[step16] \"{}\" added pos=({:.3},{:.3},{:.3}) grant<-\"{}\" x -1.0",
            c_jp,
            arm_pos.x,
            arm_pos.y,
            arm_pos.z,
            p_jp
        );
    }
    Ok(())
}

fn build_bones(ir: &IrModel, raw_structure: bool, scale: f32) -> Vec<PmxBone> {
    let mut pmx_bones = Vec::with_capacity(ir.bones.len());
    let pos_fn: fn(glam::Vec3) -> glam::Vec3 = if ir.source_format.is_vrm0() {
        gltf_pos_to_pmx_v0
    } else {
        gltf_pos_to_pmx
    };

    for bone in ir.bones.iter() {
        let pmx_pos = pos_fn(bone.position) * scale;

        // VRM bone name -> PMX Japanese name (preserve the original bone name when `raw_structure`)
        let (jp_name, en_name) = if raw_structure {
            (bone.original_name.clone(), bone.original_name.clone())
        } else if let Some(vrm_name) = &bone.vrm_bone_name {
            if let Some((jp, en)) = vrm_bone_to_pmx_name(vrm_name) {
                (jp.to_string(), en.to_string())
            } else {
                (bone.name.clone(), bone.name_en.clone())
            }
        } else {
            (bone.name.clone(), bone.name_en.clone())
        };

        let parent_index = bone.parent.map(|p| p as i32).unwrap_or(-1);

        // Tail link: if there is a child bone use its index; otherwise use a zero offset
        let tail = if let Some(&child_idx) = bone.children.first() {
            BoneTail::BoneIndex(child_idx as i32)
        } else {
            BoneTail::Offset(Vec3::ZERO)
        };

        // Flags
        let mut flags = BONE_FLAG_ROTATABLE | BONE_FLAG_OPERABLE;
        if !bone.children.is_empty() {
            flags |= BONE_FLAG_TAIL_IS_BONE;
        }
        if bone.is_physics {
            flags |= BONE_FLAG_PHYS_AFTER;
        }

        // Faithfully replay the original flags when `raw_structure` is on
        if raw_structure {
            if bone.is_translatable {
                flags |= BONE_FLAG_TRANSLATABLE;
            }
            if bone.is_visible {
                flags |= BONE_FLAG_VISIBLE;
            }
            if bone.is_axis_fixed {
                flags |= BONE_FLAG_AXIS_FIXED;
            }
        } else {
            flags |= BONE_FLAG_VISIBLE;
        }

        // Convert grant data (only when raw_structure is on)
        let grant = if raw_structure {
            bone.grant.as_ref().map(|g| {
                if g.is_rotation {
                    flags |= BONE_FLAG_ROTATION_GRANT;
                }
                if g.is_move {
                    flags |= BONE_FLAG_MOVE_GRANT;
                }
                if g.is_local {
                    flags |= BONE_FLAG_LOCAL_GRANT;
                }
                PmxGrant {
                    parent_index: g.parent_index as i32,
                    ratio: g.ratio,
                }
            })
        } else {
            None
        };

        pmx_bones.push(PmxBone {
            name: jp_name,
            name_en: en_name,
            position: pmx_pos,
            parent_index,
            deform_layer: 0,
            flags,
            tail,
            ik: None,
            grant,
        });
    }

    log::debug!(
        "build_bones: VRM {} bones -> PMX {} bones",
        ir.bones.len(),
        pmx_bones.len()
    );
    pmx_bones
}

fn build_vertices_and_faces(
    ir: &IrModel,
    use_vrm0_coords: bool,
    scale: f32,
) -> (Vec<PmxVertex>, Vec<[u32; 3]>, Vec<u32>) {
    let total_verts: usize = ir.meshes.iter().map(|m| m.vertices.len()).sum();
    let total_faces: usize = ir.meshes.iter().map(|m| m.indices.len() / 3).sum();
    let mut all_vertices: Vec<PmxVertex> = Vec::with_capacity(total_verts);
    let mut all_faces: Vec<[u32; 3]> = Vec::with_capacity(total_faces);
    let pos_fn: fn(glam::Vec3) -> glam::Vec3 = if use_vrm0_coords {
        gltf_pos_to_pmx_v0
    } else {
        gltf_pos_to_pmx
    };
    let nrm_fn: fn(glam::Vec3) -> glam::Vec3 = if use_vrm0_coords {
        gltf_normal_to_pmx_v0
    } else {
        gltf_normal_to_pmx
    };

    let mat_count = ir.materials.len().max(1);
    let mut mat_face_counts = vec![0u32; mat_count];

    // 1. Lay out vertices in ir_meshes order (must match the morph's `mesh_vertex_start`)
    let mut mesh_vertex_start: Vec<u32> = Vec::with_capacity(ir.meshes.len());
    for mesh in &ir.meshes {
        let vertex_offset = all_vertices.len() as u32;
        mesh_vertex_start.push(vertex_offset);

        for vtx in mesh.vertices.iter() {
            let pmx_pos = pos_fn(vtx.position) * scale;
            let pmx_normal = nrm_fn(vtx.normal);
            let weight = build_weight(vtx.active_weights());

            all_vertices.push(PmxVertex {
                position: pmx_pos,
                normal: pmx_normal,
                uv: glam::Vec2::new(fract_uv(vtx.uv.x), fract_uv(vtx.uv.y)),
                weight,
                edge_scale: vtx.edge_scale,
            });
        }
    }

    // 2. Group faces by material (PMX requires the face array to be contiguous per material).
    // VRM 1.0: (x, y, -z) -> det = -1; VRM 0.0: (-x, y, z) -> det = -1.
    // Both versions yield det = -1, so swap b and c to reverse the winding.
    for (mat_idx, face_count_slot) in mat_face_counts.iter_mut().enumerate() {
        for (mesh_i, mesh) in ir.meshes.iter().enumerate() {
            if mesh.material_index != mat_idx {
                continue;
            }
            let vertex_offset = mesh_vertex_start[mesh_i];
            let indices = &mesh.indices;
            let face_count = indices.len() / 3;
            for i in 0..face_count {
                let a = indices[i * 3] + vertex_offset;
                let b = indices[i * 3 + 1] + vertex_offset;
                let c = indices[i * 3 + 2] + vertex_offset;
                all_faces.push([a, c, b]);
            }
            *face_count_slot += (face_count * 3) as u32;
        }
    }

    // Vertex-weight statistics
    let mut bdef1 = 0usize;
    let mut bdef2 = 0usize;
    let mut bdef4 = 0usize;
    for v in &all_vertices {
        match &v.weight {
            PmxWeightType::Bdef1 { .. } => bdef1 += 1,
            PmxWeightType::Bdef2 { .. } => bdef2 += 1,
            PmxWeightType::Bdef4 { .. } => bdef4 += 1,
        }
    }
    log::info!(
        "Vertices: {} (BDEF1={}, BDEF2={}, BDEF4={})",
        all_vertices.len(),
        bdef1,
        bdef2,
        bdef4
    );
    log::info!("Faces: {}", all_faces.len());

    (all_vertices, all_faces, mat_face_counts)
}

fn build_weight(weights: &[(usize, f32)]) -> PmxWeightType {
    match weights.len() {
        0 => PmxWeightType::Bdef1 { bone: 0 },
        1 => PmxWeightType::Bdef1 {
            bone: weights[0].0 as i32,
        },
        2 => PmxWeightType::Bdef2 {
            bone1: weights[0].0 as i32,
            bone2: weights[1].0 as i32,
            weight1: weights[0].1,
        },
        3 | 4 => {
            // 3-4 weights: no need to allocate a Vec; use the input slice directly
            let total: f32 = weights.iter().map(|(_, w)| w).sum();
            let total = if total > 0.0 { total } else { 1.0 };

            let mut bones = [-1i32; 4];
            let mut ws = [0.0f32; 4];
            for (i, &(bi, w)) in weights.iter().enumerate() {
                bones[i] = bi as i32;
                ws[i] = w / total;
            }

            PmxWeightType::Bdef4 { bones, weights: ws }
        }
        _ => {
            // 5+ weights (rare): keep the top 4
            let mut top4 = [(0usize, 0.0f32); 4];
            for &(bi, w) in weights {
                // Find the smallest weight in top4; replace it when the current value is larger
                let mut min_idx = 0;
                let mut min_w = top4[0].1;
                for (j, &(_, tw)) in top4.iter().enumerate().skip(1) {
                    if tw < min_w {
                        min_w = tw;
                        min_idx = j;
                    }
                }
                if w > min_w {
                    top4[min_idx] = (bi, w);
                }
            }

            // Normalize
            let total: f32 = top4.iter().map(|(_, w)| w).sum();
            let total = if total > 0.0 { total } else { 1.0 };

            let mut bones = [-1i32; 4];
            let mut ws = [0.0f32; 4];
            for (i, &(bi, w)) in top4.iter().enumerate() {
                bones[i] = bi as i32;
                ws[i] = w / total;
            }

            PmxWeightType::Bdef4 { bones, weights: ws }
        }
    }
}

fn build_morphs(ir: &IrModel, use_vrm0_coords: bool, scale: f32) -> Vec<PmxMorph> {
    let pos_fn: fn(glam::Vec3) -> glam::Vec3 = if use_vrm0_coords {
        gltf_pos_to_pmx_v0
    } else {
        gltf_pos_to_pmx
    };

    let panel_name = |p: u8| -> &'static str {
        match p {
            1 => "眉",
            2 => "目",
            3 => "口",
            4 => "その他",
            _ => "?",
        }
    };

    log::debug!("--- Morph list ---");
    let mut vertex_count = 0usize;
    let mut group_count = 0usize;
    let mut uv_count = 0usize;
    // Bounds-check helper for the shared index space (matches build_vertices_and_faces)
    let total_vertex_count: usize = ir.meshes.iter().map(|m| m.vertices.len()).sum();

    let morphs: Vec<PmxMorph> = ir
        .morphs
        .iter()
        .map(|m| {
            let (morph_type, offsets) = match &m.kind {
                IrMorphKind::Vertex { ref positions, .. } => {
                    log::debug!(
                        "  [{}:{}] \"{}\" vertex morph (target_vertices={})",
                        panel_name(m.panel),
                        m.panel,
                        m.name,
                        positions.len()
                    );
                    vertex_count += 1;
                    // Sum duplicate offsets that target the same vertex
                    let mut merged: std::collections::HashMap<u32, glam::Vec3> =
                        std::collections::HashMap::new();
                    for &(vi, off) in positions {
                        *merged.entry(vi as u32).or_insert(glam::Vec3::ZERO) += pos_fn(off) * scale;
                    }
                    let mut pmx_offs: Vec<VertexMorphOffset> = merged
                        .into_iter()
                        .filter(|(_, off)| off.length_squared() > 1e-12)
                        .map(|(vi, off)| VertexMorphOffset {
                            vertex_index: vi,
                            offset: off,
                        })
                        .collect();
                    // HashMap iteration order is nondeterministic; sort by vertex_index for stable output
                    pmx_offs.sort_by_key(|o| o.vertex_index);
                    (1u8, PmxMorphOffsets::Vertex(pmx_offs))
                }
                IrMorphKind::Group(goffs) => {
                    if log::log_enabled!(log::Level::Debug) {
                        let sub_names: Vec<String> = goffs
                            .iter()
                            .filter_map(|(mi, w)| {
                                ir.morphs
                                    .get(*mi)
                                    .map(|sub| format!("{}×{:.2}", sub.name, w))
                            })
                            .collect();
                        log::debug!(
                            "  [{}:{}] \"{}\" group morph (children={}) [{}]",
                            panel_name(m.panel),
                            m.panel,
                            m.name,
                            goffs.len(),
                            sub_names.join(", ")
                        );
                    }
                    group_count += 1;
                    let pmx_offs = goffs
                        .iter()
                        .map(|(mi, w)| GroupMorphOffset {
                            morph_index: *mi as i32,
                            weight: *w,
                        })
                        .collect();
                    (0u8, PmxMorphOffsets::Group(pmx_offs))
                }
                IrMorphKind::Material { .. } => {
                    // PMX does have a material-morph type, but VRM Expression's `materialColorBind`
                    // does not line up with PMX material-morph semantics, so we emit an empty
                    // group morph instead.
                    log::debug!(
                        "  [{}:{}] \"{}\" material morph (skipped for PMX)",
                        panel_name(m.panel),
                        m.panel,
                        m.name,
                    );
                    (0u8, PmxMorphOffsets::Group(Vec::new()))
                }
                IrMorphKind::Uv { channel, offsets } => {
                    // The IR global vertex index matches the order in which build_vertices_and_faces
                    // pushes mesh.vertices, so it is identical to the PMX vertex index.
                    log::debug!(
                        "  [{}:{}] \"{}\" uv morph channel={} (target_vertices={})",
                        panel_name(m.panel),
                        m.panel,
                        m.name,
                        channel,
                        offsets.len(),
                    );
                    // PMX morph_type: channel 0 → 3 (UV0), channel 1..=4 → 4..=7 (UV1..UV4)
                    let morph_type_byte: u8 = if *channel <= 4 {
                        3 + *channel
                    } else {
                        log::warn!(
                            "  uv morph channel {} exceeds PMX max (4); clamped to UV0",
                            channel
                        );
                        3
                    };
                    let mut merged: std::collections::HashMap<u32, glam::Vec4> =
                        std::collections::HashMap::new();
                    for &(global_vi, off) in offsets {
                        if global_vi >= total_vertex_count {
                            log::warn!(
                                "  uv morph vertex index {} out of range (vertices={}); skipped",
                                global_vi,
                                total_vertex_count
                            );
                            continue;
                        }
                        *merged.entry(global_vi as u32).or_insert(glam::Vec4::ZERO) +=
                            glam::Vec4::from_array(off);
                    }
                    let mut pmx_offs: Vec<UvMorphOffset> = merged
                        .into_iter()
                        .filter(|(_, off)| off.length_squared() > 1e-12)
                        .map(|(vi, off)| UvMorphOffset {
                            vertex_index: vi,
                            offset: off,
                        })
                        .collect();
                    pmx_offs.sort_by_key(|o| o.vertex_index);
                    uv_count += 1;
                    (morph_type_byte, PmxMorphOffsets::Uv(pmx_offs))
                }
            };

            PmxMorph {
                name: m.name.clone(),
                name_en: m.name_en.clone(),
                panel: m.panel,
                morph_type,
                offsets,
            }
        })
        .collect();

    log::info!(
        "Morphs: {} (vertex={}, group={}, uv={})",
        morphs.len(),
        vertex_count,
        group_count,
        uv_count
    );
    morphs
}

/// Classify a bone into a display-frame category.
#[derive(Debug, Clone, Copy, PartialEq)]
enum BoneCategory {
    Root,    // "全ての親" -> already handled by the Root frame
    Body,    // Body (upper)
    Arms,    // Arms
    Fingers, // Fingers
    Legs,    // Legs
    Others,  // Others
}

fn classify_bone(name: &str) -> BoneCategory {
    // The Root-frame bone
    if name == "全ての親" {
        return BoneCategory::Root;
    }

    // Body (upper)
    const BODY: &[&str] = &[
        "センター",
        "グルーブ",
        "腰",
        "上半身",
        "上半身2",
        "上半身3",
        "首",
        "頭",
        "両目",
        "左目",
        "右目",
        "下半身",
    ];
    if BODY.contains(&name) {
        return BoneCategory::Body;
    }

    // Fingers (left/right finger bone names)
    if name.contains("親指")
        || name.contains("人差指")
        || name.contains("中指")
        || name.contains("薬指")
        || name.contains("小指")
    {
        return BoneCategory::Fingers;
    }

    // Arms (shoulder through wrist)
    const ARM_KEYWORDS: &[&str] = &["肩P", "肩C", "肩", "腕捩", "腕", "ひじ", "手捩", "手首"];
    if ARM_KEYWORDS.iter().any(|kw| name.contains(kw)) {
        return BoneCategory::Arms;
    }

    // Legs (foot through toe; includes IK)
    const LEG_KEYWORDS: &[&str] = &[
        "足先EX",
        "足D",
        "ひざD",
        "足首D",
        "足",
        "ひざ",
        "足首",
        "つま先",
        "ＩＫ",
        "腰キャンセル",
    ];
    if LEG_KEYWORDS.iter().any(|kw| name.contains(kw)) {
        return BoneCategory::Legs;
    }

    BoneCategory::Others
}

fn build_display_frames(bones: &[PmxBone], morphs: &[PmxMorph]) -> Vec<PmxDisplayFrame> {
    let mut frames = Vec::with_capacity(7);

    // Frame 0: Root (special frame)
    frames.push(PmxDisplayFrame {
        name: "Root".to_string(),
        name_en: "Root".to_string(),
        is_special: 1,
        elements: if !bones.is_empty() {
            vec![DisplayFrameElement::Bone(0)]
        } else {
            vec![]
        },
    });

    // Frame 1: Expressions (special frame)
    let morph_elements: Vec<DisplayFrameElement> = (0..morphs.len() as i32)
        .map(DisplayFrameElement::Morph)
        .collect();
    frames.push(PmxDisplayFrame {
        name: "表情".to_string(),
        name_en: "Exp".to_string(),
        is_special: 1,
        elements: morph_elements,
    });

    // Sort bones by category
    let mut body_elems = Vec::new();
    let mut arm_elems = Vec::new();
    let mut finger_elems = Vec::new();
    let mut leg_elems = Vec::new();
    let mut other_elems = Vec::new();

    for (i, bone) in bones.iter().enumerate() {
        let idx = i as i32;
        let cat = classify_bone(&bone.name);

        // Skip the Root-frame bone (it is already in frame 0)
        if cat == BoneCategory::Root {
            continue;
        }

        // Skip bones that are invisible AND non-operable (e.g. grant-only bones).
        // IK bones are still included.
        let is_visible = bone.flags & BONE_FLAG_VISIBLE != 0;
        let is_operable = bone.flags & BONE_FLAG_OPERABLE != 0;
        let is_ik = bone.flags & BONE_FLAG_IK != 0;
        if !is_visible && !is_operable && !is_ik {
            continue;
        }

        match cat {
            BoneCategory::Body => body_elems.push(DisplayFrameElement::Bone(idx)),
            BoneCategory::Arms => arm_elems.push(DisplayFrameElement::Bone(idx)),
            BoneCategory::Fingers => finger_elems.push(DisplayFrameElement::Bone(idx)),
            BoneCategory::Legs => leg_elems.push(DisplayFrameElement::Bone(idx)),
            BoneCategory::Others => other_elems.push(DisplayFrameElement::Bone(idx)),
            BoneCategory::Root => {} // Already skipped above
        }
    }

    // Frame 2: Body (upper)
    frames.push(PmxDisplayFrame {
        name: "体(上)".to_string(),
        name_en: "Body".to_string(),
        is_special: 0,
        elements: body_elems,
    });

    // Frame 3: Arms
    frames.push(PmxDisplayFrame {
        name: "腕".to_string(),
        name_en: "Arms".to_string(),
        is_special: 0,
        elements: arm_elems,
    });

    // Frame 4: Fingers
    frames.push(PmxDisplayFrame {
        name: "指".to_string(),
        name_en: "Fingers".to_string(),
        is_special: 0,
        elements: finger_elems,
    });

    // Frame 5: Legs
    frames.push(PmxDisplayFrame {
        name: "足".to_string(),
        name_en: "Legs".to_string(),
        is_special: 0,
        elements: leg_elems,
    });

    // Frame 6: Others
    frames.push(PmxDisplayFrame {
        name: "その他".to_string(),
        name_en: "Others".to_string(),
        is_special: 0,
        elements: other_elems,
    });

    frames
}

fn build_rigid_bodies(ir: &IrModel, align_rigid_rotation: bool, scale: f32) -> Vec<PmxRigidBody> {
    let mode_name = |m: u8| -> &'static str {
        match m {
            0 => "bone-follow",
            1 => "physics",
            2 => "physics + bone",
            _ => "?",
        }
    };

    log::debug!("--- Rigidbody list ---");
    let mut sphere_count = 0usize;
    let mut box_count = 0usize;
    let mut capsule_count = 0usize;
    let mut mode_counts = [0usize; 3];

    let bodies: Vec<PmxRigidBody> = ir.physics.rigid_bodies.iter().enumerate().map(|(i, rb)| {
        let (shape, size) = match &rb.shape {
            RigidShape::Sphere { radius } => { sphere_count += 1; (0u8, Vec3::new(*radius, 0.0, 0.0)) }
            RigidShape::Box { size } => { box_count += 1; (1u8, *size) }
            RigidShape::Capsule { radius, height } => { capsule_count += 1; (2u8, Vec3::new(*radius, *height, 0.0)) }
        };
        if rb.physics_mode < 3 { mode_counts[rb.physics_mode as usize] += 1; }

        if log::log_enabled!(log::Level::Debug) {
            let shape_name = match &rb.shape {
                RigidShape::Sphere { radius } => format!("Sphere r={:.3}", radius),
                RigidShape::Box { size } => {
                    format!("Box ({:.3},{:.3},{:.3})", size.x, size.y, size.z)
                }
                RigidShape::Capsule { radius, height } => {
                    format!("Capsule r={:.3} h={:.3}", radius, height)
                }
            };
            log::debug!("  [{:2}] \"{}\" {} bone={:?} group={} mode={} mass={:.2} pos=({:.3},{:.3},{:.3})",
                i, rb.name, shape_name, rb.bone_index, rb.group, mode_name(rb.physics_mode), rb.mass,
                rb.position.x, rb.position.y, rb.position.z);
        }

        PmxRigidBody {
            name: rb.name.clone(),
            name_en: rb.name.clone(),
            bone_index: rb.bone_index.map(|i| i as i32).unwrap_or(-1),
            group: rb.group,
            no_collision_mask: rb.no_collision_mask,
            shape,
            size: size * scale,
            position: rb.position * scale,
            rotation: if align_rigid_rotation { rb.rotation } else { Vec3::ZERO },
            mass: rb.mass,
            linear_damping: rb.linear_damping,
            angular_damping: rb.angular_damping,
            restitution: rb.restitution,
            friction: rb.friction,
            physics_mode: rb.physics_mode,
        }
    }).collect();

    log::info!(
        "Rigidbodies: {} (sphere={}, box={}, capsule={}) mode: bone_follow={}, physics={}, physics+bone={}",
        bodies.len(),
        sphere_count,
        box_count,
        capsule_count,
        mode_counts[0],
        mode_counts[1],
        mode_counts[2]
    );
    bodies
}

/// Resolve duplicate bone names by appending a "_N" suffix from the second occurrence onwards.
fn fix_duplicate_names(bones: &mut [PmxBone]) {
    use std::collections::HashMap;
    // Count occurrences of each bone name
    let mut count: HashMap<String, usize> = HashMap::new();
    for bone in bones.iter() {
        *count.entry(bone.name.clone()).or_insert(0) += 1;
    }
    // Only process bones that have duplicates (rename the second and later occurrences)
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut renamed = 0usize;
    for bone in bones.iter_mut() {
        if count.get(&bone.name).copied().unwrap_or(0) > 1 {
            let n = seen.entry(bone.name.clone()).or_insert(0);
            *n += 1;
            if *n >= 2 {
                let new_name = format!("{}_{}", bone.name, n);
                log::debug!("fix_duplicate_names: \"{}\" -> \"{}\"", bone.name, new_name);
                bone.name = new_name;
                renamed += 1;
            }
        }
    }
    if renamed > 0 {
        log::info!(
            "fix_duplicate_names: {} bone names renamed to avoid duplicates",
            renamed
        );
    }
}

/// Sort bones to satisfy the deformation-order rules (parents must come before their children).
///
/// Priority order:
///   1. Bones with AfterPhysics = OFF come first; ON afterwards.
///   2. Within each group, parents precede their children (BFS topological sort).
///
/// BFS gives a stable order that preserves the input order as much as possible.
fn sort_bones_topological(model: &mut PmxModel) {
    let n = model.bones.len();
    if n == 0 {
        return;
    }

    let phys: Vec<bool> = model
        .bones
        .iter()
        .map(|b| b.flags & BONE_FLAG_PHYS_AFTER != 0)
        .collect();

    // Pre-build the adjacency list (O(n) child lookup)
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, bone) in model.bones.iter().enumerate() {
        let p = bone.parent_index;
        if p >= 0 && (p as usize) < n {
            children[p as usize].push(i);
        }
    }

    let mut result: Vec<usize> = Vec::with_capacity(n);
    let mut added = vec![false; n];

    for pass_phys in [false, true] {
        // Push this group's roots (bones with no parent inside the group) into a min-heap.
        // BinaryHeap<Reverse> behaves as a min-heap, so we always pop the smallest index first
        // and preserve as much of the order from insert_standard_bones as possible.
        let mut heap: std::collections::BinaryHeap<std::cmp::Reverse<usize>> = (0..n)
            .filter(|&i| {
                phys[i] == pass_phys && {
                    let p = model.bones[i].parent_index;
                    p < 0 || phys[p as usize] != pass_phys
                }
            })
            .map(std::cmp::Reverse)
            .collect();

        while let Some(std::cmp::Reverse(i)) = heap.pop() {
            if added[i] {
                continue;
            }
            added[i] = true;
            result.push(i);

            // From the adjacency list, push the same-group children into the heap by smallest index
            for &j in &children[i] {
                if !added[j] && phys[j] == pass_phys {
                    heap.push(std::cmp::Reverse(j));
                }
            }
        }
    }

    // Append cycle / orphan bones at the tail (fallback)
    for (i, &is_added) in added.iter().enumerate() {
        if !is_added {
            log::warn!(
                "sort_bones_topological: \"{}\"(idx={}) unreachable -> appended to end",
                model.bones[i].name,
                i
            );
            result.push(i);
        }
    }

    // Early return when nothing changed
    if result.iter().enumerate().all(|(new, &old)| new == old) {
        return;
    }

    // Remap table (old index -> new index)
    let mut remap = vec![0i32; n];
    for (new_idx, &old_idx) in result.iter().enumerate() {
        remap[old_idx] = new_idx as i32;
    }

    log::debug!("sort_bones_topological: sorting {} bones", n);

    // Reorder the bone array (no clone needed; reorder by ownership since we already took it)
    let mut old_bones: Vec<Option<PmxBone>> = std::mem::take(&mut model.bones)
        .into_iter()
        .map(Some)
        .collect();
    model.bones = result
        .iter()
        .map(|&i| {
            old_bones[i]
                .take()
                .expect("sort_bones_topological: 同一ボーンが2回参照された")
        })
        .collect();

    // Update every reference via the remap
    remap_all_bone_indices(
        model,
        |idx| {
            if idx >= 0 {
                remap[idx as usize]
            } else {
                idx
            }
        },
    );
}

fn build_joints(ir: &IrModel, scale: f32) -> Vec<PmxJoint> {
    log::debug!("--- Joint list ---");
    let joints: Vec<PmxJoint> = ir
        .physics
        .joints
        .iter()
        .enumerate()
        .map(|(i, j)| {
            let rb_a_name = ir
                .physics
                .rigid_bodies
                .get(j.rigid_a)
                .map(|r| r.name.as_str())
                .unwrap_or("?");
            let rb_b_name = ir
                .physics
                .rigid_bodies
                .get(j.rigid_b)
                .map(|r| r.name.as_str())
                .unwrap_or("?");
            log::debug!(
                "  [{:2}] \"{}\" A=\"{}\"({}) <-> B=\"{}\"({}) pos=({:.3},{:.3},{:.3})",
                i,
                j.name,
                rb_a_name,
                j.rigid_a,
                rb_b_name,
                j.rigid_b,
                j.position.x,
                j.position.y,
                j.position.z
            );

            PmxJoint {
                name: j.name.clone(),
                name_en: j.name.clone(),
                joint_type: 0, // 6-DOF spring
                rigid_a: j.rigid_a as i32,
                rigid_b: j.rigid_b as i32,
                position: j.position * scale,
                rotation: j.rotation,
                move_limit_lo: j.move_limit_lo * scale,
                move_limit_hi: j.move_limit_hi * scale,
                rot_limit_lo: j.rot_limit_lo,
                rot_limit_hi: j.rot_limit_hi,
                spring_move: j.spring_move,
                spring_rot: j.spring_rot,
            }
        })
        .collect();
    log::info!("Joints: {}", joints.len());
    joints
}

/// Normalize a UV to 0..1 (`fract` with negative-value support).
/// Values inside [0, 1] are kept as-is (prevents 1.0 % 1.0 = 0.0 rounding).
#[inline]
fn fract_uv(v: f32) -> f32 {
    if (0.0..=1.0).contains(&v) {
        return v;
    }
    let f = v % 1.0;
    if f < 0.0 {
        f + 1.0
    } else {
        f
    }
}
