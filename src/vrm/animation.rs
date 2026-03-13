use anyhow::{Context, Result};
use glam::{Quat, Vec3};
use gltf::buffer::Data;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

use crate::intermediate::animation::*;

/// VRMAファイルを読み込みアニメーションデータを返す
pub fn load_vrma(path: &Path) -> Result<VrmaAnimation> {
    let (document, buffers, _images) = gltf::import(path)
        .with_context(|| format!("VRMAファイルの読み込みに失敗: {}", path.display()))?;

    let vrma_ext = extract_vrma_extension(&document)?;
    parse_vrma(&document, &buffers, &vrma_ext)
}

/// VRMC_vrm_animation 拡張を取得
fn extract_vrma_extension(document: &gltf::Document) -> Result<Value> {
    let json = document.as_json();
    if let Some(exts) = &json.extensions {
        if let Some(val) = exts.others.get("VRMC_vrm_animation") {
            return Ok(val.clone());
        }
    }
    anyhow::bail!("VRMC_vrm_animation 拡張が見つかりません");
}

/// VRMA拡張 + glTFアニメーションをパース
fn parse_vrma(
    document: &gltf::Document,
    buffers: &[Data],
    vrma_ext: &Value,
) -> Result<VrmaAnimation> {
    // ノード→ヒューマノイドボーン名のマッピング
    let bone_node_map = parse_humanoid_mapping(vrma_ext);
    // ノード→表情名のマッピング
    let expr_node_map = parse_expression_mapping(vrma_ext);

    // VRMAノードのレスト回転を抽出（リターゲティング用）
    let bone_rests = extract_vrma_bone_rests(document, &bone_node_map);

    // 最初のアニメーションを読み込む（仕様: animations の最初を使用）
    let anim = document.animations().next()
        .context("glTF アニメーションが含まれていません")?;

    let name = anim.name().unwrap_or("vrma").to_string();

    let mut bone_channels: HashMap<String, BoneChannel> = HashMap::new();
    let mut expression_channels: HashMap<String, ExpressionChannel> = HashMap::new();
    let mut duration: f32 = 0.0;

    for channel in anim.channels() {
        let target = channel.target();
        let node_idx = target.node().index();
        let sampler = channel.sampler();

        let reader = channel.reader(|buf| Some(&buffers[buf.index()]));

        // キーフレームの時間を読み込み
        let times: Vec<f32> = reader.read_inputs()
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

        // ヒューマノイドボーンチャネル
        if let Some(bone_name) = bone_node_map.get(&node_idx) {
            match target.property() {
                gltf::animation::Property::Rotation => {
                    let rotations: Vec<Quat> = reader.read_outputs()
                        .map(|out| {
                            if let gltf::animation::util::ReadOutputs::Rotations(rots) = out {
                                rots.into_f32().map(|r| Quat::from_array(r)).collect()
                            } else {
                                Vec::new()
                            }
                        })
                        .unwrap_or_default();

                    let keyframes: Vec<RotationKeyframe> = times.iter()
                        .zip(rotations.iter())
                        .map(|(&t, &v)| RotationKeyframe { time: t, value: v })
                        .collect();

                    let entry = bone_channels.entry(bone_name.clone()).or_insert_with(|| BoneChannel {
                        rotation: Vec::new(),
                        rotation_interp: interp,
                        translation: None,
                        translation_interp: None,
                    });
                    entry.rotation = keyframes;
                    entry.rotation_interp = interp;
                }
                gltf::animation::Property::Translation => {
                    let translations: Vec<Vec3> = reader.read_outputs()
                        .map(|out| {
                            if let gltf::animation::util::ReadOutputs::Translations(trans) = out {
                                trans.map(|t| Vec3::from(t)).collect()
                            } else {
                                Vec::new()
                            }
                        })
                        .unwrap_or_default();

                    let keyframes: Vec<TranslationKeyframe> = times.iter()
                        .zip(translations.iter())
                        .map(|(&t, &v)| TranslationKeyframe { time: t, value: v })
                        .collect();

                    let entry = bone_channels.entry(bone_name.clone()).or_insert_with(|| BoneChannel {
                        rotation: Vec::new(),
                        rotation_interp: Interpolation::Linear,
                        translation: None,
                        translation_interp: None,
                    });
                    entry.translation = Some(keyframes);
                    entry.translation_interp = Some(interp);
                }
                _ => {} // Scale は無視（仕様）
            }
        }

        // 表情チャネル: translation.x をウェイトとして解釈
        if let Some(expr_name) = expr_node_map.get(&node_idx) {
            if matches!(target.property(), gltf::animation::Property::Translation) {
                let translations: Vec<Vec3> = reader.read_outputs()
                    .map(|out| {
                        if let gltf::animation::util::ReadOutputs::Translations(trans) = out {
                            trans.map(|t| Vec3::from(t)).collect()
                        } else {
                            Vec::new()
                        }
                    })
                    .unwrap_or_default();

                let keyframes: Vec<ScalarKeyframe> = times.iter()
                    .zip(translations.iter())
                    .map(|(&t, v)| ScalarKeyframe { time: t, value: v.x })
                    .collect();

                expression_channels.insert(expr_name.clone(), ExpressionChannel {
                    keyframes,
                    interp,
                });
            }
        }
    }

    log::info!(
        "VRMA読み込み: ボーン{}ch, 表情{}ch, {:.2}秒",
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
    })
}

/// VRMAノード階層からヒューマノイドボーンのレスト回転を抽出
fn extract_vrma_bone_rests(
    document: &gltf::Document,
    bone_node_map: &HashMap<usize, String>,
) -> HashMap<String, VrmaBoneRest> {
    let nodes: Vec<gltf::Node> = document.nodes().collect();
    let n = nodes.len();

    // 全ノードのローカル回転・平行移動とグローバル回転を計算
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

    // グローバル回転を伝搬（ルートから）
    let mut computed = vec![false; n];
    let mut stack: Vec<(usize, Quat)> = nodes.iter()
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

    // ヒューマノイドボーンのレスト回転を収集
    let mut rests = HashMap::new();
    for (&node_idx, bone_name) in bone_node_map {
        if node_idx < n {
            rests.insert(bone_name.clone(), VrmaBoneRest {
                local_rotation: local_rotations[node_idx],
                global_rotation: global_rotations[node_idx],
                local_translation: local_translations[node_idx],
            });
        }
    }

    rests
}

/// humanoid.humanBones のノードマッピングをパース
fn parse_humanoid_mapping(vrma_ext: &Value) -> HashMap<usize, String> {
    let mut map = HashMap::new();
    if let Some(bones) = vrma_ext.pointer("/humanoid/humanBones").and_then(|v| v.as_object()) {
        for (bone_name, bone_val) in bones {
            if let Some(node) = bone_val.get("node").and_then(|v| v.as_u64()) {
                map.insert(node as usize, bone_name.clone());
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
            eprintln!("VRMA サンプルファイルが見つかりません（スキップ）");
            return;
        }
        let anim = load_vrma(path).expect("VRMA 読み込み失敗");
        assert!(anim.duration > 0.0, "アニメーション長さが0");
        assert!(!anim.bone_channels.is_empty(), "ボーンチャネルが空");
        eprintln!(
            "VRMA: {} ({:.2}s), bones={}, exprs={}",
            anim.name, anim.duration,
            anim.bone_channels.len(),
            anim.expression_channels.len(),
        );

        // Hips チャネルの存在確認
        assert!(anim.bone_channels.contains_key("hips"), "hips チャネルなし");

        // サンプリングテスト
        let rot = anim.sample_bone_rotation("hips", 0.0);
        assert!(rot.is_some(), "hips rotation サンプリング失敗");
    }
}

/// expressions のノードマッピングをパース（preset + custom）
fn parse_expression_mapping(vrma_ext: &Value) -> HashMap<usize, String> {
    let mut map = HashMap::new();

    // プリセット表情
    if let Some(preset) = vrma_ext.pointer("/expressions/preset").and_then(|v| v.as_object()) {
        for (name, val) in preset {
            if let Some(node) = val.get("node").and_then(|v| v.as_u64()) {
                map.insert(node as usize, name.clone());
            }
        }
    }

    // カスタム表情
    if let Some(custom) = vrma_ext.pointer("/expressions/custom").and_then(|v| v.as_object()) {
        for (name, val) in custom {
            if let Some(node) = val.get("node").and_then(|v| v.as_u64()) {
                map.insert(node as usize, name.clone());
            }
        }
    }

    map
}
