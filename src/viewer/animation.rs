use std::collections::HashMap;
use std::sync::Arc;

use eframe::wgpu;
use glam::{Mat4, Quat, Vec3};

use crate::convert::coord::PMX_SCALE;
use crate::intermediate::animation::{BoneMatchMode, VrmaAnimation};
use crate::intermediate::types::IrModel;

use super::mesh::GpuModel;

/// One frame's duration in seconds.
const FRAME_DURATION: f32 = 1.0 / 30.0;

/// Loop mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopMode {
    /// No loop.
    None,
    /// Standard loop.
    Normal,
    /// A-B loop (segment repeat).
    AB,
    /// Ping-pong (back-and-forth loop).
    PingPong,
}

/// Skinning weights per bone (per GPU vertex).
struct VertexSkinWeight {
    /// (bone_index, weight), up to four entries.
    bones: [(usize, f32); 4],
}

/// Data needed to play an animation.
struct SkinningData {
    /// Bone weights per GPU vertex.
    vertex_weights: Vec<VertexSkinWeight>,
    /// Rest-pose bone global matrices (glTF space).
    rest_global_mats: Vec<Mat4>,
    /// Inverse rest-pose global matrices (cached to avoid `inverse()` every frame).
    rest_global_inv_mats: Vec<Mat4>,
    /// Bone local matrices (rest pose, raw matrix before decomposition).
    rest_local_mats: Vec<Mat4>,
    /// Bone local rotations (rest pose).
    rest_local_rotations: Vec<Quat>,
    /// Bone global rotations (rest pose, used for retargeting).
    rest_global_rotations: Vec<Quat>,
    /// Bone local translations (rest pose).
    rest_local_translations: Vec<Vec3>,
    /// Bone local scales (rest pose).
    rest_local_scales: Vec<Vec3>,
    /// Parent index for each bone.
    bone_parents: Vec<Option<usize>>,
    /// Reverse map: IrBone index -> VRM humanoid bone name.
    bone_idx_to_name: HashMap<usize, String>,
    /// VRM expression name -> morph index.
    expr_name_to_morph: HashMap<String, usize>,
    /// Whether this is VRM 0.0.
    is_vrm0: bool,
    /// Grant data (PMX rotation grant / translation grant).
    grants: Vec<Option<GrantInfo>>,
    /// Order in which grants are processed (topological so the parent comes first; only bones with grants).
    grant_order: Vec<usize>,
}

/// Grant info (animation runtime).
struct GrantInfo {
    parent_index: usize,
    ratio: f32,
    is_rotation: bool,
    is_move: bool,
    /// Local-grant flag (true: apply the delta in the child bone's local space).
    is_local: bool,
}

/// Animation playback state.
pub struct AnimationState {
    pub animation: Arc<VrmaAnimation>,
    pub playing: bool,
    pub loop_mode: LoopMode,
    pub speed: f32,
    pub current_time: f32,
    /// A-B loop start point (seconds).
    pub ab_start: Option<f32>,
    /// A-B loop end point (seconds).
    pub ab_end: Option<f32>,
    /// Ping-pong direction (1.0: forward, -1.0: reverse).
    pub ping_pong_direction: f32,
    skin: SkinningData,
    /// Cached animated global matrices (glTF space).
    cached_animated_globals: Vec<Mat4>,
    /// Working buffer for delta matrices (avoids per-frame allocation).
    work_deltas: Vec<Mat4>,
    /// Flag buffer for compute_animated_globals (avoids per-frame allocation).
    work_computed: Vec<bool>,
    /// Working buffer for bone local matrices (used by grants; avoids per-frame allocation).
    work_local_mats: Vec<Mat4>,
    /// Pre-built mapping from expression channel name to morph index (avoids HashMap lookup every frame).
    expr_mapping: Vec<(String, usize)>,
}

impl AnimationState {
    /// Build the animation playback state from IrModel and GpuModel.
    pub fn new(animation: Arc<VrmaAnimation>, ir: &IrModel, gpu_model: &GpuModel) -> Self {
        let skin = build_skinning_data(ir, gpu_model, &animation);
        // Pre-build the expression-channel-name -> morph-index map (avoids HashMap lookup every frame).
        let expr_mapping: Vec<(String, usize)> = animation
            .expression_channels
            .keys()
            .filter_map(|name| {
                skin.expr_name_to_morph
                    .get(name.as_str())
                    .map(|&idx| (name.clone(), idx))
            })
            .collect();
        Self {
            animation,
            playing: true,
            loop_mode: LoopMode::Normal,
            speed: 1.0,
            current_time: 0.0,
            ab_start: None,
            ab_end: None,
            ping_pong_direction: 1.0,
            work_deltas: vec![Mat4::IDENTITY; skin.rest_global_mats.len()],
            work_computed: vec![false; skin.rest_global_mats.len()],
            work_local_mats: vec![Mat4::IDENTITY; skin.rest_global_mats.len()],
            cached_animated_globals: skin.rest_global_mats.clone(),
            expr_mapping,
            skin,
        }
    }

    /// Return the effective playback range.
    pub fn effective_range(&self) -> (f32, f32) {
        match self.loop_mode {
            LoopMode::AB | LoopMode::PingPong => {
                let lo = self.ab_start.unwrap_or(0.0);
                let hi = self.ab_end.unwrap_or(self.animation.duration);
                (lo.min(hi), lo.max(hi))
            }
            _ => (0.0, self.animation.duration),
        }
    }

    /// Advance the animation time.
    pub fn advance(&mut self, dt: f32) {
        if !self.playing {
            return;
        }

        let effective_speed = match self.loop_mode {
            LoopMode::PingPong => self.speed.abs() * self.ping_pong_direction,
            _ => self.speed,
        };
        self.current_time += dt * effective_speed;

        let (lo, hi) = self.effective_range();
        let range = hi - lo;

        match self.loop_mode {
            LoopMode::None => {
                if self.current_time > hi {
                    self.current_time = hi;
                    self.playing = false;
                } else if self.current_time < lo {
                    self.current_time = lo;
                    self.playing = false;
                }
            }
            LoopMode::Normal | LoopMode::AB => {
                if range > 0.0 {
                    if self.current_time > hi {
                        self.current_time = lo + (self.current_time - lo) % range;
                    } else if self.current_time < lo {
                        self.current_time = hi - (lo - self.current_time) % range;
                    }
                }
            }
            LoopMode::PingPong => {
                if self.current_time > hi {
                    self.current_time = hi - (self.current_time - hi).min(range);
                    self.ping_pong_direction = -1.0;
                } else if self.current_time < lo {
                    self.current_time = lo + (lo - self.current_time).min(range);
                    self.ping_pong_direction = 1.0;
                }
            }
        }
    }

    /// Step one frame forward / backward (used while paused).
    pub fn step_frame(&mut self, forward: bool) {
        let delta = if forward {
            FRAME_DURATION
        } else {
            -FRAME_DURATION
        };
        self.current_time += delta;
        let (lo, hi) = self.effective_range();
        self.current_time = self.current_time.clamp(lo, hi);
    }

    /// Write the current expression weights into the morph weight array.
    /// Returns whether anything changed.
    pub fn apply_expressions(&self, morph_weights: &mut [f32]) -> bool {
        let mut changed = false;
        for (expr_name, morph_idx) in &self.expr_mapping {
            if *morph_idx < morph_weights.len() {
                let w = self
                    .animation
                    .sample_expression(expr_name, self.current_time)
                    .unwrap_or(0.0);
                if (morph_weights[*morph_idx] - w).abs() > 1e-6 {
                    morph_weights[*morph_idx] = w;
                    changed = true;
                }
            }
        }
        changed
    }

    /// Get the animated global matrices (glTF space).
    pub fn animated_globals(&self) -> &[Mat4] {
        &self.cached_animated_globals
    }

    /// Whether this is VRM 0.0.
    pub fn is_vrm0(&self) -> bool {
        self.skin.is_vrm0
    }

    /// Apply the bone animation at the current time to the vertex buffer.
    pub fn apply_bone_animation(
        &mut self,
        gpu_model: &mut GpuModel,
        queue: &wgpu::Queue,
        morph_weights: &[f32],
        ir: &IrModel,
    ) {
        // Compute global matrices in-place (avoids alloc).
        self.compute_animated_globals_inplace(ir);

        // Grant pass: copy rotation / translation from the grant parent.
        self.apply_grants();

        // Compute delta matrices in the work buffer and convert to PMX space ahead of time (avoids alloc).
        // M * delta * M yields the PMX-space delta matrix (M is the mirror matrix; M² = I),
        // which removes the per-vertex pmx_pos_to_gltf / gltf_pos_to_pmx conversions inside the inner loop.
        let bone_count = self.skin.rest_global_mats.len();
        self.work_deltas.resize(bone_count, Mat4::IDENTITY);
        let is_vrm0 = self.skin.is_vrm0;
        for i in 0..bone_count {
            let delta = self.cached_animated_globals[i] * self.skin.rest_global_inv_mats[i];
            self.work_deltas[i] = conjugate_delta_to_pmx(delta, is_vrm0);
        }

        // Reuse the vertex buffer (allocated only on the first call; subsequent calls reuse the capacity).
        gpu_model.reset_animated_to_base();
        {
            let work = gpu_model.animated_vertices_mut();
            let deltas = &self.work_deltas;

            for (vi, vw) in self.skin.vertex_weights.iter().enumerate() {
                if vi >= work.len() {
                    break;
                }

                let mut blended = Mat4::ZERO;
                let mut total_w = 0.0f32;
                for &(bone_idx, weight) in &vw.bones {
                    if weight > 0.0 && bone_idx < deltas.len() {
                        blended += weight * deltas[bone_idx];
                        total_w += weight;
                    }
                }

                if total_w < 1e-6 {
                    continue;
                }

                if (total_w - 1.0).abs() > 1e-4 {
                    blended *= 1.0 / total_w;
                }

                // Delta matrices are pre-converted to PMX space, so they apply directly.
                let pmx_pos = Vec3::from(work[vi].position);
                work[vi].position = blended.transform_point3(pmx_pos).to_array();

                // Normal (transformed directly in PMX space).
                let pmx_normal = Vec3::from(work[vi].normal);
                let skinned_n = blended.transform_vector3(pmx_normal).normalize_or_zero();
                work[vi].normal = skinned_n.to_array();

                // Tangent (tangent.w = handedness is preserved; xyz transformed directly in PMX space).
                let pmx_tangent = Vec3::from_slice(&work[vi].tangent[..3]);
                let skinned_t = blended.transform_vector3(pmx_tangent).normalize_or_zero();
                // Gram-Schmidt re-orthogonalization: project the tangent perpendicular to the normal.
                let t_ortho =
                    (skinned_t - skinned_n * skinned_n.dot(skinned_t)).normalize_or_zero();
                work[vi].tangent = [t_ortho.x, t_ortho.y, t_ortho.z, work[vi].tangent[3]];
            }
        } // Drop the mutable borrow of `work` here.

        // Apply morphs directly to animated_vertices (avoids borrow conflict).
        gpu_model.apply_morphs_to_animated(morph_weights);

        // Write to the GPU buffer.
        queue.write_buffer(
            &gpu_model.vertex_buf,
            0,
            bytemuck::cast_slice(gpu_model.current_vertices()),
        );
    }

    /// PMX grant pass: copy rotation / translation from the grant parent bone.
    ///
    /// Walk in PMX bone-index order, take the grant parent's rotation delta (relative to rest)
    /// and apply it to the local matrix scaled by the grant ratio.
    /// Then recompute global matrices in index order so the change propagates to descendants.
    fn apply_grants(&mut self) {
        let bone_count = self.skin.grants.len();
        if bone_count == 0 || self.skin.grant_order.is_empty() {
            return;
        }

        // Phase 1: apply grant deltas to local matrices (topological order ensures the grant parent runs first).
        let mut grant_applied = false;
        for &i in &self.skin.grant_order {
            let Some(ref grant) = self.skin.grants[i] else {
                continue;
            };
            let gp = grant.parent_index;
            if gp >= bone_count {
                continue;
            }

            // Read rotation / translation from the grant parent's local matrix.
            let (_, gp_rot, gp_trans) = self.work_local_mats[gp].to_scale_rotation_translation();
            let (my_scale, mut my_rot, mut my_trans) =
                self.work_local_mats[i].to_scale_rotation_translation();

            let mut changed = false;

            if grant.is_rotation {
                let gp_rest_rot = self.skin.rest_local_rotations[gp];
                let delta = gp_rest_rot.inverse() * gp_rot;
                let applied = if (grant.ratio - 1.0).abs() < 1e-6 {
                    delta
                } else {
                    Quat::IDENTITY.slerp(delta, grant.ratio)
                };
                if grant.is_local {
                    // Local grant: apply the delta on top of the child bone's rest pose.
                    // child_rot = child_rest_rot * slerp(IDENTITY, parent_delta, ratio)
                    let my_rest_rot = self.skin.rest_local_rotations[i];
                    my_rot = my_rest_rot * applied;
                } else {
                    // Non-local grant: multiply the delta into the current rotation (model space).
                    my_rot *= applied;
                }
                changed = true;
            }

            if grant.is_move {
                let gp_rest_trans = self.skin.rest_local_translations[gp];
                let delta = gp_trans - gp_rest_trans;
                if grant.is_local {
                    // Local grant: convert the delta into the child bone's local space and apply.
                    let my_rest_rot = self.skin.rest_local_rotations[i];
                    let local_delta = my_rest_rot.inverse() * delta;
                    my_trans = self.skin.rest_local_translations[i] + local_delta * grant.ratio;
                } else {
                    // Non-local grant: add the delta directly (model space).
                    my_trans += delta * grant.ratio;
                }
                changed = true;
            }

            if changed {
                self.work_local_mats[i] =
                    Mat4::from_scale_rotation_translation(my_scale, my_rot, my_trans);
                grant_applied = true;
            }
        }

        if !grant_applied {
            return;
        }

        // Phase 2: recompute global matrices in index order.
        // PMX guarantees that parents come earlier in the bone index, so a linear scan propagates correctly.
        for i in 0..bone_count {
            let parent_global = self.skin.bone_parents[i]
                .map(|pi| self.cached_animated_globals[pi])
                .unwrap_or(Mat4::IDENTITY);
            self.cached_animated_globals[i] = parent_global * self.work_local_mats[i];
        }
    }

    /// Compute animated bone global matrices from the VRMA keyframes in-place (avoids alloc).
    fn compute_animated_globals_inplace(&mut self, ir: &IrModel) {
        let bone_count = self.skin.rest_global_mats.len();
        self.cached_animated_globals
            .resize(bone_count, Mat4::IDENTITY);
        self.work_computed.resize(bone_count, false);
        self.work_computed.fill(false);
        self.work_local_mats.resize(bone_count, Mat4::IDENTITY);
        for i in 0..bone_count {
            self.cached_animated_globals[i] = Mat4::IDENTITY;
            self.work_local_mats[i] = self.skin.rest_local_mats[i];
        }

        // Walk the hierarchy starting from root bones (no parent).
        for i in 0..bone_count {
            if self.skin.bone_parents[i].is_none() {
                Self::compute_global_recursive_static(
                    &self.skin,
                    &self.animation,
                    self.current_time,
                    i,
                    Mat4::IDENTITY,
                    &mut self.cached_animated_globals,
                    &mut self.work_computed,
                    &mut self.work_local_mats,
                    &ir.bones,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn compute_global_recursive_static(
        skin: &SkinningData,
        animation: &VrmaAnimation,
        current_time: f32,
        bone_idx: usize,
        parent_global: Mat4,
        globals: &mut [Mat4],
        computed: &mut [bool],
        local_mats: &mut [Mat4],
        bones: &[crate::intermediate::types::IrBone],
    ) {
        if computed[bone_idx] {
            return;
        }
        computed[bone_idx] = true;

        // Check whether this bone is animated by the VRMA.
        let mut animated = false;
        let mut local_rot = skin.rest_local_rotations[bone_idx];
        let mut local_trans = skin.rest_local_translations[bone_idx];

        if let Some(bone_name) = skin.bone_idx_to_name.get(&bone_idx) {
            let is_humanoid = matches!(animation.match_mode, BoneMatchMode::Humanoid);

            // Rotation.
            if let Some(anim_rot) = animation.sample_bone_rotation(bone_name, current_time) {
                animated = true;
                if animation.is_additive {
                    if animation.is_bone_local_delta {
                        // Bone-local delta (Unity Muscle SwingTwist):
                        // anim_rot = postQ × SwingTwist(sign × deg) × Inv(postQ)
                        // Delta relative to the normalized skeleton (Identity at muscle = 0).
                        // Final local rotation = rest × anim_rot
                        //   = (rest × postQ) × SwingTwist × Inv(postQ)
                        //   = preQ_model × SwingTwist × Inv(postQ)
                        local_rot = skin.rest_local_rotations[bone_idx] * anim_rot;
                    } else {
                        // World-space delta:
                        // Conjugate by the parent's rest global rotation -> convert to local-space delta.
                        let parent_rest_rot = skin.bone_parents[bone_idx]
                            .map(|pi| skin.rest_global_rotations[pi])
                            .unwrap_or(Quat::IDENTITY);
                        let local_delta = parent_rest_rot.inverse() * anim_rot * parent_rest_rot;
                        local_rot = local_delta * skin.rest_local_rotations[bone_idx];
                    }
                } else if is_humanoid {
                    // VRMA: apply the retargeting formula.
                    if let Some(vrma_rest) = animation.bone_rests.get(bone_name.as_str()) {
                        let w_vrma = vrma_rest.global_rotation;
                        let l_vrma = vrma_rest.local_rotation;
                        let l_model = skin.rest_local_rotations[bone_idx];
                        let w_model = skin.rest_global_rotations[bone_idx];

                        let mut normalized =
                            w_vrma * l_vrma.inverse() * anim_rot * w_vrma.inverse();

                        if skin.is_vrm0 {
                            normalized = Quat::from_xyzw(
                                -normalized.x,
                                normalized.y,
                                -normalized.z,
                                normalized.w,
                            );
                        }

                        local_rot = l_model * w_model.inverse() * normalized * w_model;
                    } else {
                        local_rot = if skin.is_vrm0 {
                            Quat::from_xyzw(-anim_rot.x, anim_rot.y, -anim_rot.z, anim_rot.w)
                        } else {
                            anim_rot
                        };
                    }
                } else {
                    // NodeName: global-space retargeting.
                    if let Some(src_rest) = animation.bone_rests.get(bone_name.as_str()) {
                        let w_src = src_rest.global_rotation;
                        let l_src = src_rest.local_rotation;
                        let l_model = skin.rest_local_rotations[bone_idx];
                        let w_model = skin.rest_global_rotations[bone_idx];

                        // Local delta from the source rest -> conjugate into global space.
                        let local_delta = l_src.inverse() * anim_rot;
                        let mut normalized = w_src * local_delta * w_src.inverse();

                        // If the source faces +Z (VRM faces -Z), apply a 180° Y-axis correction.
                        // Negate the X and Z components of `normalized` (= conjugation by 180° about Y).
                        if animation.facing_flip_y {
                            normalized = Quat::from_xyzw(
                                -normalized.x,
                                normalized.y,
                                -normalized.z,
                                normalized.w,
                            );
                        }

                        // Convert into the target model's local space.
                        local_rot = l_model * w_model.inverse() * normalized * w_model;
                    } else {
                        local_rot = anim_rot;
                    }
                }
            }
            // Translation.
            if let Some(raw_trans) = animation.sample_bone_translation(bone_name, current_time) {
                animated = true;
                if animation.is_additive {
                    // Additive: add the delta value to the rest position.
                    local_trans = skin.rest_local_translations[bone_idx] + raw_trans;
                } else if is_humanoid {
                    // VRMA: apply the delta from the rest position to the model via world space.
                    if let Some(vrma_rest) = animation.bone_rests.get(bone_name.as_str()) {
                        let delta_local = raw_trans - vrma_rest.local_translation;
                        let vrma_parent_rot =
                            vrma_rest.global_rotation * vrma_rest.local_rotation.inverse();
                        let mut delta_world = vrma_parent_rot * delta_local;
                        if skin.is_vrm0 {
                            delta_world = Vec3::new(-delta_world.x, delta_world.y, -delta_world.z);
                        }
                        let model_h = skin.rest_global_mats[bone_idx]
                            .transform_point3(Vec3::ZERO)
                            .y;
                        let vrma_h = (vrma_parent_rot * vrma_rest.local_translation).y;
                        if vrma_h.abs() > 0.01 {
                            delta_world *= model_h / vrma_h;
                        }
                        let model_parent_rot = skin.rest_global_rotations[bone_idx]
                            * skin.rest_local_rotations[bone_idx].inverse();
                        let delta_model_local = model_parent_rot.inverse() * delta_world;
                        local_trans = skin.rest_local_translations[bone_idx] + delta_model_local;
                    } else {
                        local_trans = if skin.is_vrm0 {
                            Vec3::new(-raw_trans.x, raw_trans.y, -raw_trans.z)
                        } else {
                            raw_trans
                        };
                    }
                } else {
                    // NodeName: scale and apply the delta from the source rest.
                    if let Some(src_rest) = animation.bone_rests.get(bone_name.as_str()) {
                        let mut delta = raw_trans - src_rest.local_translation;
                        // If the source faces +Z, negate X and Z of the translation delta (Y180 correction).
                        if animation.facing_flip_y {
                            delta = Vec3::new(-delta.x, delta.y, -delta.z);
                        }
                        let src_len = src_rest.local_translation.length();
                        let model_len = skin.rest_local_translations[bone_idx].length();
                        if src_len > 1e-6 && model_len > 1e-6 {
                            let scale = model_len / src_len;
                            local_trans = skin.rest_local_translations[bone_idx] + delta * scale;
                        }
                        // When src_len ≈ 0 (root etc.), do not apply the delta as-is.
                    }
                }
            }
        }

        if animated {
            // Animated bone: keep the scale and recompose.
            let local_mat = Mat4::from_scale_rotation_translation(
                skin.rest_local_scales[bone_idx],
                local_rot,
                local_trans,
            );
            local_mats[bone_idx] = local_mat;
            globals[bone_idx] = parent_global * local_mat;
        } else {
            // Non-animated bone: use the raw local matrix (avoids decomposition error).
            local_mats[bone_idx] = skin.rest_local_mats[bone_idx];
            globals[bone_idx] = parent_global * skin.rest_local_mats[bone_idx];
        }

        // Recurse into children.
        for &child_idx in &bones[bone_idx].children {
            Self::compute_global_recursive_static(
                skin,
                animation,
                current_time,
                child_idx,
                globals[bone_idx],
                globals,
                computed,
                local_mats,
                bones,
            );
        }
    }
}

/// Build skinning data from IrModel and GpuModel.
fn build_skinning_data(
    ir: &IrModel,
    gpu_model: &GpuModel,
    animation: &VrmaAnimation,
) -> SkinningData {
    let g2g = gpu_model.global_to_gpu_map();
    let gpu_vert_count = gpu_model.base_vertices().len();

    // Build per-GPU-vertex bone weights.
    let mut vertex_weights: Vec<VertexSkinWeight> = (0..gpu_vert_count)
        .map(|_| VertexSkinWeight {
            bones: [(0, 0.0); 4],
        })
        .collect();

    let mut global_vi = 0usize;
    for mesh in &ir.meshes {
        for v in mesh.vertices.iter() {
            if global_vi < g2g.len() {
                let gpu_vi = g2g[global_vi] as usize;
                if gpu_vi < gpu_vert_count {
                    // Set only when the weight has not yet been set.
                    if vertex_weights[gpu_vi].bones[0].1 == 0.0 {
                        vertex_weights[gpu_vi] = VertexSkinWeight { bones: v.weights };
                    }
                }
            }
            global_vi += 1;
        }
    }

    // Compute rest-pose local transforms.
    let bone_count = ir.bones.len();
    let mut rest_local_mats = vec![Mat4::IDENTITY; bone_count];
    let mut rest_local_rotations = vec![Quat::IDENTITY; bone_count];
    let mut rest_global_rotations = vec![Quat::IDENTITY; bone_count];
    let mut rest_local_translations = vec![Vec3::ZERO; bone_count];
    let mut rest_local_scales = vec![Vec3::ONE; bone_count];
    let bone_parents: Vec<Option<usize>> = ir.bones.iter().map(|b| b.parent).collect();
    let rest_global_mats: Vec<Mat4> = ir.bones.iter().map(|b| b.global_mat).collect();
    let rest_global_inv_mats: Vec<Mat4> = rest_global_mats.iter().map(|m| m.inverse()).collect();

    for (i, bone) in ir.bones.iter().enumerate() {
        let parent_global = bone
            .parent
            .map(|pi| ir.bones[pi].global_mat)
            .unwrap_or(Mat4::IDENTITY);
        let local_mat = parent_global.inverse() * bone.global_mat;
        rest_local_mats[i] = local_mat;
        let (scale, rot, trans) = local_mat.to_scale_rotation_translation();
        rest_local_rotations[i] = rot;
        rest_local_translations[i] = trans;
        rest_local_scales[i] = scale;

        // Extract the global rotation (used for retargeting).
        let (_, global_rot, _) = bone.global_mat.to_scale_rotation_translation();
        rest_global_rotations[i] = global_rot;
    }

    // Bone-name -> bone-index mapping (depends on the match mode).
    let mut bone_name_to_idx: HashMap<String, usize> = HashMap::new();
    match animation.match_mode {
        BoneMatchMode::Humanoid => {
            // VRMA: match by humanoid bone name.
            for (i, bone) in ir.bones.iter().enumerate() {
                if let Some(ref vrm_name) = bone.vrm_bone_name {
                    bone_name_to_idx.insert(vrm_name.clone(), i);
                }
            }
        }
        BoneMatchMode::NodeName => {
            // GLB / glTF / FBX: match directly by node name.
            // Cross-reference the channel name in the animation against IrBone's name / name_en.
            let anim_bone_names: std::collections::HashSet<&str> =
                animation.bone_channels.keys().map(|s| s.as_str()).collect();

            for (i, bone) in ir.bones.iter().enumerate() {
                // Exact match (preferring name_en over name).
                if anim_bone_names.contains(bone.name_en.as_str()) {
                    bone_name_to_idx.insert(bone.name_en.clone(), i);
                } else if anim_bone_names.contains(bone.name.as_str()) {
                    bone_name_to_idx.insert(bone.name.clone(), i);
                }
            }

            // For unmatched channels, try a fuzzy match by suffix.
            let matched_names: std::collections::HashSet<String> =
                bone_name_to_idx.keys().cloned().collect();
            let mut used_indices: std::collections::HashSet<usize> =
                bone_name_to_idx.values().copied().collect();
            for anim_name in &anim_bone_names {
                if matched_names.contains(*anim_name) {
                    continue;
                }
                // Suffix match such as "Armature_Hips" -> "Hips".
                for (i, bone) in ir.bones.iter().enumerate() {
                    if used_indices.contains(&i) {
                        continue;
                    }
                    let matches = anim_name.ends_with(&bone.name_en)
                        || anim_name.ends_with(&bone.name)
                        || bone.name_en.ends_with(anim_name)
                        || bone.name.ends_with(anim_name);
                    if matches {
                        bone_name_to_idx.insert(anim_name.to_string(), i);
                        used_indices.insert(i);
                        break;
                    }
                }
            }

            log::info!(
                "Bone matching: {}/{}ch matched",
                bone_name_to_idx.len(),
                animation.bone_channels.len(),
            );
        }
    }

    // Expression-name -> morph-index mapping.
    let mut expr_name_to_morph: HashMap<String, usize> = HashMap::new();
    for (i, morph) in ir.morphs.iter().enumerate() {
        // Match against both VRM expression names (English) and morph names.
        if !morph.name_en.is_empty() {
            expr_name_to_morph.insert(morph.name_en.clone(), i);
        }
        if !morph.name.is_empty() && !expr_name_to_morph.contains_key(&morph.name) {
            expr_name_to_morph.insert(morph.name.clone(), i);
        }
    }

    // Reverse map.
    let bone_idx_to_name: HashMap<usize, String> = bone_name_to_idx
        .iter()
        .map(|(name, &idx)| (idx, name.clone()))
        .collect();

    // Grant data.
    let grants: Vec<Option<GrantInfo>> = ir
        .bones
        .iter()
        .map(|b| {
            b.grant.as_ref().map(|g| GrantInfo {
                parent_index: g.parent_index,
                ratio: g.ratio,
                is_rotation: g.is_rotation,
                is_move: g.is_move,
                is_local: g.is_local,
            })
        })
        .collect();

    // Pre-compute the grant processing order via topological sort.
    // The PMX spec expects the grant parent to come earlier in the bone index, but
    // we still enforce a topological order as a defense against ill-formed PMX files.
    let grant_order = {
        let n = grants.len();
        // Collect indices of bones that have a grant.
        let has_grant: Vec<usize> = (0..n).filter(|&i| grants[i].is_some()).collect();
        if has_grant.is_empty() {
            Vec::new()
        } else {
            // Topological sort over the grant dependency graph (Kahn's algorithm).
            // In-degree: an edge enters this bone if its grant parent itself has a grant.
            let grant_set: std::collections::HashSet<usize> = has_grant.iter().copied().collect();
            let mut in_degree: HashMap<usize, usize> = has_grant.iter().map(|&i| (i, 0)).collect();
            for &i in &has_grant {
                let Some(grant) = grants[i].as_ref() else {
                    continue;
                };
                let gp = grant.parent_index;
                if grant_set.contains(&gp) {
                    *in_degree.entry(i).or_default() += 1;
                }
            }
            let mut queue: std::collections::VecDeque<usize> = has_grant
                .iter()
                .filter(|&&i| in_degree[&i] == 0)
                .copied()
                .collect();
            let mut order = Vec::with_capacity(has_grant.len());
            while let Some(i) = queue.pop_front() {
                order.push(i);
                // Decrement in-degree for bones whose grant parent is i.
                for &j in &has_grant {
                    if grants[j].as_ref().is_some_and(|g| g.parent_index == i) {
                        let Some(deg) = in_degree.get_mut(&j) else {
                            continue;
                        };
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(j);
                        }
                    }
                }
            }
            // Fall back if there is a cycle (append remaining bones in index order).
            if order.len() < has_grant.len() {
                log::warn!(
                    "Grant dependency has circular reference: {} of {} bones unresolved",
                    has_grant.len(),
                    has_grant.len() - order.len()
                );
                let in_order: std::collections::HashSet<usize> = order.iter().copied().collect();
                for &i in &has_grant {
                    if !in_order.contains(&i) {
                        order.push(i);
                    }
                }
            }
            order
        }
    };

    SkinningData {
        vertex_weights,
        rest_global_mats,
        rest_global_inv_mats,
        rest_local_mats,
        rest_local_rotations,
        rest_global_rotations,
        rest_local_translations,
        rest_local_scales,
        bone_parents,
        bone_idx_to_name,
        expr_name_to_morph,
        is_vrm0: ir.source_format.is_vrm0(),
        grants,
        grant_order,
    }
}

/// Convert a delta matrix from glTF space to PMX space.
///
/// The original transform: gltf_pos_to_pmx(delta.transform_point3(pmx_pos_to_gltf(pmx_pos)))
/// = S * M * (R * M * p / S + t) = M * R * M * p + S * M * t
///
/// Here R = the 3x3 rotation part of delta, t = the translation of delta,
/// M = the mirror matrix (self-inverse), S = PMX_SCALE.
///
/// PMX-space delta matrix structure:
/// - 3x3 part: M * R * M (conjugation; the S and 1/S cancel out).
/// - Translation: S * M * t (converts glTF meters to PMX scale and applies the mirror).
///
/// The 3x3 part is just sign flips, the translation is a sign flip plus a scale multiplication.
/// VRM 1.0: M = diag(1, 1, -1). VRM 0.0: M = diag(-1, 1, 1).
///
/// glam uses column-major layout `c[col][row]`.
/// (M * R * M) at row i, col j = mi * R[i][j] * mj (mi = mirror diagonal element).
#[inline]
fn conjugate_delta_to_pmx(delta: Mat4, is_vrm0: bool) -> Mat4 {
    let c = delta.to_cols_array_2d(); // c[col][row]
    if is_vrm0 {
        // M = diag(-1, 1, 1)
        // 3x3: flip the sign of row 0 and column 0 ([0][0] returns to original after two flips).
        // Translation (c[3][0..3]): M * t * S = (-tx*S, ty*S, tz*S).
        let s = PMX_SCALE;
        Mat4::from_cols_array_2d(&[
            [c[0][0], -c[0][1], -c[0][2], c[0][3]],
            [-c[1][0], c[1][1], c[1][2], c[1][3]],
            [-c[2][0], c[2][1], c[2][2], c[2][3]],
            [-c[3][0] * s, c[3][1] * s, c[3][2] * s, c[3][3]],
        ])
    } else {
        // M = diag(1, 1, -1)
        // 3x3: flip the sign of row 2 and column 2 ([2][2] returns to original after two flips).
        // Translation (c[3][0..3]): M * t * S = (tx*S, ty*S, -tz*S).
        let s = PMX_SCALE;
        Mat4::from_cols_array_2d(&[
            [c[0][0], c[0][1], -c[0][2], c[0][3]],
            [c[1][0], c[1][1], -c[1][2], c[1][3]],
            [-c[2][0], -c[2][1], c[2][2], c[2][3]],
            [c[3][0] * s, c[3][1] * s, -c[3][2] * s, c[3][3]],
        ])
    }
}
