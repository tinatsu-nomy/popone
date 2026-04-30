use crate::error::{PoponeError, Result, ResultExt};
use glam::{Quat, Vec3};
use gltf::buffer::Data;
use rust_i18n::t;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

use crate::fbx::humanoid::detect_humanoid;
use crate::intermediate::animation::*;

/// Load a VRMA file and return its animation data.
pub fn load_vrma(path: &Path) -> Result<VrmaAnimation> {
    let (document, buffers, _images) = gltf::import(path).with_context(|| {
        t!(
            "error.vrma.vrma_load_failed",
            path = path.display().to_string()
        )
        .to_string()
    })?;

    let vrma_ext = extract_vrma_extension(&document)?;
    parse_vrma(&document, &buffers, &vrma_ext)
}

/// Load animations from a GLB/glTF file (node-name based).
/// If the VRMC_vrm_animation extension is present, load it as VRMA instead.
pub fn load_gltf_animation(path: &Path) -> Result<Vec<VrmaAnimation>> {
    let (document, buffers, _images) = gltf::import(path).with_context(|| {
        t!(
            "error.vrma.gltf_load_failed",
            path = path.display().to_string()
        )
        .to_string()
    })?;

    // If the VRMC_vrm_animation extension is present, load as VRMA
    if let Ok(vrma_ext) = extract_vrma_extension(&document) {
        let anim = parse_vrma(&document, &buffers, &vrma_ext)?;
        return Ok(vec![anim]);
    }

    // Generic glTF animation: load using node names as channel keys
    let mut animations = Vec::new();
    let nodes: Vec<gltf::Node> = document.nodes().collect();

    for anim in document.animations() {
        let anim_name = anim.name().unwrap_or("animation").to_string();

        let mut bone_channels: HashMap<String, BoneChannel> = HashMap::new();
        let mut expression_channels: HashMap<String, ExpressionChannel> = HashMap::new();
        let mut duration: f32 = 0.0;

        for channel in anim.channels() {
            let target = channel.target();
            let node_idx = target.node().index();
            let sampler = channel.sampler();

            let reader = channel.reader(|buf| Some(&buffers[buf.index()]));

            let times: Vec<f32> = reader
                .read_inputs()
                .map(|iter| iter.collect())
                .unwrap_or_default();

            if let Some(&last_t) = times.last() {
                duration = duration.max(last_t);
            }

            let interp = match sampler.interpolation() {
                gltf::animation::Interpolation::Linear => Interpolation::Linear,
                gltf::animation::Interpolation::Step => Interpolation::Step,
                gltf::animation::Interpolation::CubicSpline => Interpolation::CubicSpline,
            };

            // Use the node name as the channel key
            let node_name = nodes
                .get(node_idx)
                .and_then(|n| n.name())
                .unwrap_or("")
                .to_string();

            if node_name.is_empty() {
                continue;
            }

            match target.property() {
                gltf::animation::Property::Rotation => {
                    let rotations: Vec<Quat> = reader
                        .read_outputs()
                        .map(|out| {
                            if let gltf::animation::util::ReadOutputs::Rotations(rots) = out {
                                rots.into_f32().map(Quat::from_array).collect()
                            } else {
                                Vec::new()
                            }
                        })
                        .unwrap_or_default();

                    let keyframes: Vec<RotationKeyframe> = times
                        .iter()
                        .zip(rotations.iter())
                        .map(|(&t, &v)| RotationKeyframe { time: t, value: v })
                        .collect();

                    let entry = bone_channels
                        .entry(node_name)
                        .or_insert_with(|| BoneChannel {
                            rotation: Vec::new(),
                            rotation_interp: interp,
                            translation: None,
                            translation_interp: None,
                        });
                    entry.rotation = keyframes;
                    entry.rotation_interp = interp;
                }
                gltf::animation::Property::Translation => {
                    let translations: Vec<Vec3> = reader
                        .read_outputs()
                        .map(|out| {
                            if let gltf::animation::util::ReadOutputs::Translations(trans) = out {
                                trans.map(Vec3::from).collect()
                            } else {
                                Vec::new()
                            }
                        })
                        .unwrap_or_default();

                    let keyframes: Vec<TranslationKeyframe> = times
                        .iter()
                        .zip(translations.iter())
                        .map(|(&t, &v)| TranslationKeyframe { time: t, value: v })
                        .collect();

                    let entry = bone_channels
                        .entry(node_name)
                        .or_insert_with(|| BoneChannel {
                            rotation: Vec::new(),
                            rotation_interp: Interpolation::Linear,
                            translation: None,
                            translation_interp: None,
                        });
                    entry.translation = Some(keyframes);
                    entry.translation_interp = Some(interp);
                }
                gltf::animation::Property::MorphTargetWeights => {
                    // Morph target weights: pair them with the mesh's morph target names
                    let weights: Vec<f32> = reader
                        .read_outputs()
                        .map(|out| {
                            if let gltf::animation::util::ReadOutputs::MorphTargetWeights(w) = out {
                                w.into_f32().collect()
                            } else {
                                Vec::new()
                            }
                        })
                        .unwrap_or_default();

                    // Pull the morph target names from the target node's mesh
                    let morph_names: Vec<String> = nodes
                        .get(node_idx)
                        .and_then(|n| n.mesh())
                        .map(|mesh| {
                            mesh.primitives()
                                .next()
                                .map(|prim| {
                                    prim.morph_targets()
                                        .enumerate()
                                        .map(|(i, _)| {
                                            // glTF morph target names live in mesh.extras or target_names
                                            format!("morph_{}", i)
                                        })
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default()
                        })
                        .unwrap_or_default();

                    // Pull the mesh's `targetNames` if available
                    let target_names: Vec<String> = nodes
                        .get(node_idx)
                        .and_then(|n| n.mesh())
                        .map(|mesh| {
                            let json_mesh = &document.as_json().meshes[mesh.index()];
                            json_mesh
                                .extras
                                .as_deref()
                                .and_then(|e: &serde_json::value::RawValue| {
                                    serde_json::from_str::<serde_json::Value>(e.get()).ok()
                                })
                                .and_then(|v: serde_json::Value| v.get("targetNames").cloned())
                                .and_then(|v: serde_json::Value| {
                                    v.as_array().map(|a: &Vec<serde_json::Value>| {
                                        a.iter()
                                            .filter_map(|n| n.as_str().map(|s| s.to_string()))
                                            .collect::<Vec<String>>()
                                    })
                                })
                                .unwrap_or_else(|| morph_names.clone())
                        })
                        .unwrap_or(morph_names);

                    let morph_count = target_names.len();
                    if morph_count > 0 {
                        // weights is ordered as [frame0_target0, frame0_target1, ..., frame1_target0, ...]
                        for (mi, morph_name) in target_names.iter().enumerate() {
                            let keyframes: Vec<ScalarKeyframe> = times
                                .iter()
                                .enumerate()
                                .filter_map(|(fi, &t)| {
                                    let idx = fi * morph_count + mi;
                                    weights
                                        .get(idx)
                                        .map(|&w| ScalarKeyframe { time: t, value: w })
                                })
                                .collect();
                            if !keyframes.is_empty() {
                                expression_channels.insert(
                                    morph_name.clone(),
                                    ExpressionChannel { keyframes, interp },
                                );
                            }
                        }
                    }
                }
                _ => {} // Scale is ignored
            }
        }

        if !bone_channels.is_empty() || !expression_channels.is_empty() {
            // Collect rest poses of GLB nodes (used for retargeting)
            let mut bone_rests: HashMap<String, VrmaBoneRest> = HashMap::new();
            // Compute global transforms per node
            let mut node_globals: Vec<glam::Mat4> = vec![glam::Mat4::IDENTITY; nodes.len()];
            // Recursive global-transform propagation from roots
            fn compute_node_globals(
                nodes: &[gltf::Node],
                node_globals: &mut [glam::Mat4],
                node_idx: usize,
                parent_global: glam::Mat4,
            ) {
                let node = &nodes[node_idx];
                let (t, r, _s) = node.transform().decomposed();
                let local_mat =
                    glam::Mat4::from_rotation_translation(Quat::from_array(r), Vec3::from(t));
                node_globals[node_idx] = parent_global * local_mat;
                for child in node.children() {
                    compute_node_globals(
                        nodes,
                        node_globals,
                        child.index(),
                        node_globals[node_idx],
                    );
                }
            }
            for scene in document.scenes() {
                for root_node in scene.nodes() {
                    compute_node_globals(
                        &nodes,
                        &mut node_globals,
                        root_node.index(),
                        glam::Mat4::IDENTITY,
                    );
                }
            }
            // Store the rest pose for every bone channel
            for name in bone_channels.keys() {
                // Resolve the node index from the node name
                if let Some(node) = nodes.iter().find(|n| n.name() == Some(name.as_str())) {
                    let (t, r, _s) = node.transform().decomposed();
                    let local_rot = Quat::from_array(r);
                    let local_trans = Vec3::from(t);
                    let (_, global_rot, _) =
                        node_globals[node.index()].to_scale_rotation_translation();
                    bone_rests.insert(
                        name.clone(),
                        VrmaBoneRest {
                            local_rotation: local_rot,
                            global_rotation: global_rot,
                            local_translation: local_trans,
                        },
                    );
                }
            }

            // Detect the source model's facing:
            // use the global X coordinate of bones whose name contains "Left" (+X = Left implies +Z forward, opposite to VRM).
            let facing_flip_y = {
                let mut left_x_sum = 0.0f32;
                let mut count = 0;
                for node in &nodes {
                    if let Some(name) = node.name() {
                        let lower = name.to_lowercase();
                        if lower.contains("left")
                            && (lower.contains("arm")
                                || lower.contains("leg")
                                || lower.contains("shoulder"))
                        {
                            let (_, _, global_t) =
                                node_globals[node.index()].to_scale_rotation_translation();
                            left_x_sum += global_t.x;
                            count += 1;
                        }
                    }
                }
                count > 0 && left_x_sum / count as f32 > 0.01
            };

            // Try to derive a humanoid mapping from bone names
            let glb_names: Vec<(usize, &str)> = bone_channels
                .keys()
                .enumerate()
                .map(|(i, name)| (i, name.as_str()))
                .collect();
            let humanoid = detect_humanoid(&glb_names);

            let (final_channels, final_rests, match_mode) = if humanoid.mapping.len() >= 5 {
                let name_list: Vec<String> = bone_channels.keys().cloned().collect();
                let mut renamed: HashMap<String, BoneChannel> = HashMap::new();
                let mut renamed_rests: HashMap<String, VrmaBoneRest> = HashMap::new();
                let mut mapped_count = 0;
                for (i, glb_name) in name_list.iter().enumerate() {
                    if let Some(human_bone) = humanoid.mapping.get(&i) {
                        let vrm_name = human_bone.as_vrm_name().to_string();
                        if let Some(ch) = bone_channels.remove(glb_name) {
                            if let Some(rest) = bone_rests.get(glb_name) {
                                renamed_rests.insert(vrm_name.clone(), rest.clone());
                            }
                            renamed.insert(vrm_name, ch);
                            mapped_count += 1;
                        }
                    }
                }
                log::info!(
                    "glTF humanoid detected: {} ({}/{}ch mapping)",
                    humanoid.rig_type.label(),
                    mapped_count,
                    name_list.len(),
                );
                (renamed, renamed_rests, BoneMatchMode::Humanoid)
            } else {
                (bone_channels, bone_rests, BoneMatchMode::NodeName)
            };

            log::info!(
                "glTF animation loaded: '{}' bone {}ch, expression {}ch, rest {} entries, {:.2}s, mode={:?}",
                anim_name,
                final_channels.len(),
                expression_channels.len(),
                final_rests.len(),
                duration,
                match_mode,
            );

            animations.push(VrmaAnimation {
                name: anim_name,
                duration,
                bone_channels: final_channels,
                expression_channels,
                bone_rests: final_rests,
                match_mode,
                facing_flip_y,
                is_additive: false,
                is_bone_local_delta: false,
            });
        }
    }

    if animations.is_empty() {
        return Err(PoponeError::Other(
            t!("error.vrma.no_animations").to_string(),
        ));
    }

    Ok(animations)
}

/// Fetch the VRMC_vrm_animation extension.
fn extract_vrma_extension(document: &gltf::Document) -> Result<Value> {
    let json = document.as_json();
    if let Some(exts) = &json.extensions {
        if let Some(val) = exts.others.get("VRMC_vrm_animation") {
            return Ok(val.clone());
        }
    }
    Err(PoponeError::Other(
        t!("error.vrma.extension_missing").to_string(),
    ))
}

/// Parse the VRMA extension + glTF animation.
fn parse_vrma(
    document: &gltf::Document,
    buffers: &[Data],
    vrma_ext: &Value,
) -> Result<VrmaAnimation> {
    // Node -> humanoid bone-name mapping
    let bone_node_map = parse_humanoid_mapping(vrma_ext);
    // Node -> expression-name mapping
    let expr_node_map = parse_expression_mapping(vrma_ext);

    // Extract rest rotations of VRMA nodes (used for retargeting)
    let bone_rests = extract_vrma_bone_rests(document, &bone_node_map);

    // Load the first animation (spec: use the first entry in `animations`)
    let anim = document
        .animations()
        .next()
        .with_context(|| t!("error.vrma.gltf_no_animations").to_string())?;

    let name = anim.name().unwrap_or("vrma").to_string();

    let mut bone_channels: HashMap<String, BoneChannel> = HashMap::new();
    let mut expression_channels: HashMap<String, ExpressionChannel> = HashMap::new();
    let mut duration: f32 = 0.0;

    for channel in anim.channels() {
        let target = channel.target();
        let node_idx = target.node().index();
        let sampler = channel.sampler();

        let reader = channel.reader(|buf| Some(&buffers[buf.index()]));

        // Read keyframe times
        let times: Vec<f32> = reader
            .read_inputs()
            .map(|iter| iter.collect())
            .unwrap_or_default();

        if let Some(&last_t) = times.last() {
            duration = duration.max(last_t);
        }

        let interp = match sampler.interpolation() {
            gltf::animation::Interpolation::Linear => Interpolation::Linear,
            gltf::animation::Interpolation::Step => Interpolation::Step,
            gltf::animation::Interpolation::CubicSpline => Interpolation::CubicSpline,
        };

        // Humanoid bone channels
        if let Some(bone_name) = bone_node_map.get(&node_idx) {
            match target.property() {
                gltf::animation::Property::Rotation => {
                    let rotations: Vec<Quat> = reader
                        .read_outputs()
                        .map(|out| {
                            if let gltf::animation::util::ReadOutputs::Rotations(rots) = out {
                                rots.into_f32().map(Quat::from_array).collect()
                            } else {
                                Vec::new()
                            }
                        })
                        .unwrap_or_default();

                    let keyframes: Vec<RotationKeyframe> = times
                        .iter()
                        .zip(rotations.iter())
                        .map(|(&t, &v)| RotationKeyframe { time: t, value: v })
                        .collect();

                    let entry =
                        bone_channels
                            .entry(bone_name.clone())
                            .or_insert_with(|| BoneChannel {
                                rotation: Vec::new(),
                                rotation_interp: interp,
                                translation: None,
                                translation_interp: None,
                            });
                    entry.rotation = keyframes;
                    entry.rotation_interp = interp;
                }
                gltf::animation::Property::Translation => {
                    let translations: Vec<Vec3> = reader
                        .read_outputs()
                        .map(|out| {
                            if let gltf::animation::util::ReadOutputs::Translations(trans) = out {
                                trans.map(Vec3::from).collect()
                            } else {
                                Vec::new()
                            }
                        })
                        .unwrap_or_default();

                    let keyframes: Vec<TranslationKeyframe> = times
                        .iter()
                        .zip(translations.iter())
                        .map(|(&t, &v)| TranslationKeyframe { time: t, value: v })
                        .collect();

                    let entry =
                        bone_channels
                            .entry(bone_name.clone())
                            .or_insert_with(|| BoneChannel {
                                rotation: Vec::new(),
                                rotation_interp: Interpolation::Linear,
                                translation: None,
                                translation_interp: None,
                            });
                    entry.translation = Some(keyframes);
                    entry.translation_interp = Some(interp);
                }
                _ => {} // Scale is ignored (per spec)
            }
        }

        // Expression channel: interpret translation.x as the weight
        if let Some(expr_name) = expr_node_map.get(&node_idx) {
            if matches!(target.property(), gltf::animation::Property::Translation) {
                let translations: Vec<Vec3> = reader
                    .read_outputs()
                    .map(|out| {
                        if let gltf::animation::util::ReadOutputs::Translations(trans) = out {
                            trans.map(Vec3::from).collect()
                        } else {
                            Vec::new()
                        }
                    })
                    .unwrap_or_default();

                let keyframes: Vec<ScalarKeyframe> = times
                    .iter()
                    .zip(translations.iter())
                    .map(|(&t, v)| ScalarKeyframe {
                        time: t,
                        value: v.x,
                    })
                    .collect();

                expression_channels
                    .insert(expr_name.clone(), ExpressionChannel { keyframes, interp });
            }
        }
    }

    log::info!(
        "VRMA loaded: bone {}ch, expression {}ch, {:.2}s",
        bone_channels.len(),
        expression_channels.len(),
        duration,
    );

    Ok(VrmaAnimation {
        name,
        duration,
        bone_channels,
        expression_channels,
        bone_rests,
        match_mode: BoneMatchMode::Humanoid,
        facing_flip_y: false,
        is_additive: false,
        is_bone_local_delta: false,
    })
}

/// Extract rest rotations of humanoid bones from the VRMA node hierarchy.
fn extract_vrma_bone_rests(
    document: &gltf::Document,
    bone_node_map: &HashMap<usize, String>,
) -> HashMap<String, VrmaBoneRest> {
    let nodes: Vec<gltf::Node> = document.nodes().collect();
    let n = nodes.len();

    // Compute local rotation/translation and global rotation for every node
    let mut local_rotations = vec![Quat::IDENTITY; n];
    let mut local_translations = vec![Vec3::ZERO; n];
    let mut global_rotations = vec![Quat::IDENTITY; n];
    let mut has_parent = vec![false; n];

    for node in &nodes {
        let (t, r, _) = node.transform().decomposed();
        local_rotations[node.index()] = Quat::from_array(r);
        local_translations[node.index()] = Vec3::from(t);
        for child in node.children() {
            if child.index() < n {
                has_parent[child.index()] = true;
            }
        }
    }

    // Propagate global rotations starting from roots
    let mut computed = vec![false; n];
    let mut stack: Vec<(usize, Quat)> = nodes
        .iter()
        .filter(|node| !has_parent[node.index()])
        .map(|node| (node.index(), Quat::IDENTITY))
        .collect();

    while let Some((idx, parent_rot)) = stack.pop() {
        if computed[idx] || idx >= n {
            continue;
        }
        computed[idx] = true;
        global_rotations[idx] = parent_rot * local_rotations[idx];

        for child in nodes[idx].children() {
            stack.push((child.index(), global_rotations[idx]));
        }
    }

    // Collect rest rotations for humanoid bones
    let mut rests = HashMap::new();
    for (&node_idx, bone_name) in bone_node_map {
        if node_idx < n {
            rests.insert(
                bone_name.clone(),
                VrmaBoneRest {
                    local_rotation: local_rotations[node_idx],
                    global_rotation: global_rotations[node_idx],
                    local_translation: local_translations[node_idx],
                },
            );
        }
    }

    rests
}

/// Parse the node mapping in `humanoid.humanBones`.
fn parse_humanoid_mapping(vrma_ext: &Value) -> HashMap<usize, String> {
    let mut map = HashMap::new();
    if let Some(bones) = vrma_ext
        .pointer("/humanoid/humanBones")
        .and_then(|v| v.as_object())
    {
        for (bone_name, bone_val) in bones {
            if let Some(node) = bone_val.get("node").and_then(|v| v.as_u64()) {
                map.insert(node as usize, bone_name.clone());
            }
        }
    }
    map
}

/// Parse the node mapping in `expressions` (preset + custom).
fn parse_expression_mapping(vrma_ext: &Value) -> HashMap<usize, String> {
    let mut map = HashMap::new();

    // Preset expressions
    if let Some(preset) = vrma_ext
        .pointer("/expressions/preset")
        .and_then(|v| v.as_object())
    {
        for (name, val) in preset {
            if let Some(node) = val.get("node").and_then(|v| v.as_u64()) {
                map.insert(node as usize, name.clone());
            }
        }
    }

    // Custom expressions
    if let Some(custom) = vrma_ext
        .pointer("/expressions/custom")
        .and_then(|v| v.as_object())
    {
        for (name, val) in custom {
            if let Some(node) = val.get("node").and_then(|v| v.as_u64()) {
                map.insert(node as usize, name.clone());
            }
        }
    }

    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_vrma_sample() {
        let path = Path::new("../tmp/vrma/VRMA_MotionPack/vrma/VRMA_01.vrma");
        if !path.exists() {
            eprintln!("VRMA sample file not found (skipping)");
            return;
        }
        let anim = load_vrma(path).expect("VRMA 読み込み失敗");
        assert!(anim.duration > 0.0, "アニメーション長さが0");
        assert!(!anim.bone_channels.is_empty(), "ボーンチャネルが空");
        eprintln!(
            "VRMA: {} ({:.2}s), bones={}, exprs={}",
            anim.name,
            anim.duration,
            anim.bone_channels.len(),
            anim.expression_channels.len(),
        );

        // Verify the hips channel exists
        assert!(anim.bone_channels.contains_key("hips"), "hips チャネルなし");

        // Sampling smoke test
        let rot = anim.sample_bone_rotation("hips", 0.0);
        assert!(rot.is_some(), "hips rotation サンプリング失敗");
    }
}
