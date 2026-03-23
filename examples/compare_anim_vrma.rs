/// .anim と .vrma の t=0 回転比較ツール（VRM モデルのスケルトンを使用）
///
/// 使い方:
///   cargo run --example compare_anim_vrma
///
/// VRM モデルを読み込み、両アニメーションの t=0 での最終グローバル回転を比較する。
/// .anim は is_additive=true（Muscle→ローカルデルタ）、
/// VRMA は is_additive=false（リターゲティング前のローカル回転）。
use std::collections::HashMap;
use std::path::Path;

use glam::{Mat4, Quat, Vec3};
use popone::intermediate::animation::BoneMatchMode;
use popone::unity::animation::load_unity_anim_with_params;
use popone::vrm::animation::load_vrma;

/// VRM モデルのスケルトンデータ（viewer の SkinningData の簡易版）
struct SkeletonData {
    rest_local_rotations: Vec<Quat>,
    rest_global_rotations: Vec<Quat>,
    rest_local_translations: Vec<Vec3>,
    rest_local_scales: Vec<Vec3>,
    rest_local_mats: Vec<Mat4>,
    bone_parents: Vec<Option<usize>>,
    bone_children: Vec<Vec<usize>>,
    /// ボーンインデックス → VRM ヒューマノイドボーン名
    #[allow(dead_code)]
    bone_idx_to_name: HashMap<usize, String>,
    /// VRM ヒューマノイドボーン名 → ボーンインデックス
    bone_name_to_idx: HashMap<String, usize>,
}

fn load_vrm_skeleton(vrm_path: &Path) -> anyhow::Result<SkeletonData> {
    let glb = popone::vrm::loader::load_glb(vrm_path)?;
    let version = popone::vrm::detect::detect_version(&glb.document);
    let all_extensions = popone::vrm::loader::get_raw_extensions(&glb.document);

    let ir = popone::vrm::extract::extract_ir_model(
        &glb.document,
        &glb.buffers,
        &glb.images,
        &glb.vrm_extension,
        &version,
        &all_extensions,
    )?;

    let bone_count = ir.bones.len();
    let mut rest_local_mats = vec![Mat4::IDENTITY; bone_count];
    let mut rest_local_rotations = vec![Quat::IDENTITY; bone_count];
    let mut rest_global_rotations = vec![Quat::IDENTITY; bone_count];
    let mut rest_local_translations = vec![Vec3::ZERO; bone_count];
    let mut rest_local_scales = vec![Vec3::ONE; bone_count];

    let bone_parents: Vec<Option<usize>> = ir.bones.iter().map(|b| b.parent).collect();
    let bone_children: Vec<Vec<usize>> = ir.bones.iter().map(|b| b.children.clone()).collect();

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

        let (_, global_rot, _) = bone.global_mat.to_scale_rotation_translation();
        rest_global_rotations[i] = global_rot;
    }

    // ヒューマノイドボーン名でマッピング
    let mut bone_name_to_idx: HashMap<String, usize> = HashMap::new();
    for (i, bone) in ir.bones.iter().enumerate() {
        if let Some(ref vrm_name) = bone.vrm_bone_name {
            bone_name_to_idx.insert(vrm_name.clone(), i);
        }
    }
    let bone_idx_to_name: HashMap<usize, String> = bone_name_to_idx
        .iter()
        .map(|(name, &idx)| (idx, name.clone()))
        .collect();

    println!(
        "  VRM スケルトン: ボーン数={}, ヒューマノイドボーン数={}",
        bone_count,
        bone_name_to_idx.len()
    );

    Ok(SkeletonData {
        rest_local_rotations,
        rest_global_rotations,
        rest_local_translations,
        rest_local_scales,
        rest_local_mats,
        bone_parents,
        bone_children,
        bone_idx_to_name,
        bone_name_to_idx,
    })
}

/// 階層を辿ってグローバル行列を計算
fn compute_globals(skel: &SkeletonData, local_rotations: &HashMap<usize, Quat>) -> Vec<Mat4> {
    let bone_count = skel.bone_parents.len();
    let mut globals = vec![Mat4::IDENTITY; bone_count];
    let mut computed = vec![false; bone_count];

    for i in 0..bone_count {
        if skel.bone_parents[i].is_none() {
            compute_global_recursive(
                skel,
                local_rotations,
                i,
                Mat4::IDENTITY,
                &mut globals,
                &mut computed,
            );
        }
    }
    globals
}

fn compute_global_recursive(
    skel: &SkeletonData,
    local_rotations: &HashMap<usize, Quat>,
    bone_idx: usize,
    parent_global: Mat4,
    globals: &mut [Mat4],
    computed: &mut [bool],
) {
    if computed[bone_idx] {
        return;
    }
    computed[bone_idx] = true;

    if let Some(&local_rot) = local_rotations.get(&bone_idx) {
        // アニメーション適用: 回転のみ変更、平行移動・スケールはレストポーズ
        let local_mat = Mat4::from_scale_rotation_translation(
            skel.rest_local_scales[bone_idx],
            local_rot,
            skel.rest_local_translations[bone_idx],
        );
        globals[bone_idx] = parent_global * local_mat;
    } else {
        // 非アニメーションボーン: レストポーズそのまま
        globals[bone_idx] = parent_global * skel.rest_local_mats[bone_idx];
    }

    for &child_idx in &skel.bone_children[bone_idx] {
        compute_global_recursive(
            skel,
            local_rotations,
            child_idx,
            globals[bone_idx],
            globals,
            computed,
        );
    }
}

fn main() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .try_init();

    let vrm_path = Path::new(r"E:\misc\nomy\vrm_view\tmp\KizunaAI_KAMATTE\KizunaAI_KAMATTE.vrm");
    let anim_path = Path::new(
        r"E:\misc\nomy\vrm_view\tmp\unitypackage\KizunaAI_KAMATTE_VRM&Motion\Assets\KizunaAI\KizunaAI_KAMATTE\Motion\KizunaAI_KAMATTE_Kamacho_Motion.anim",
    );
    let vrma_path =
        Path::new(r"E:\misc\nomy\vrm_view\tmp\vrma\KizunaAI_KAMATTE_Kamacho_Motion.vrma");

    // --- VRM モデル読み込み ---
    println!("=== VRM モデル読み込み ===");
    let skel = match load_vrm_skeleton(vrm_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("VRM 読み込みエラー: {}", e);
            return;
        }
    };

    // --- .anim 読み込み ---
    println!("\n=== .anim 読み込み ===");
    let muscle_scale = 1.0;
    let params_path = Path::new(r"E:\misc\nomy\anim2vrma\KizunaAI_KAMATTE_humanoid_params.json");
    let params_opt = if params_path.exists() {
        println!("  Humanoidパラメータ: {}", params_path.display());
        Some(params_path as &Path)
    } else {
        println!("  Humanoidパラメータなし（V-Sekai フォールバック）");
        None
    };
    let anim = match load_unity_anim_with_params(anim_path, muscle_scale, params_opt) {
        Ok(a) => a,
        Err(e) => {
            eprintln!(".anim 読み込みエラー: {}", e);
            return;
        }
    };
    println!(
        "  名前: {}, duration: {:.2}s, ボーンch: {}, is_additive: {}",
        anim.name,
        anim.duration,
        anim.bone_channels.len(),
        anim.is_additive,
    );

    // --- VRMA 読み込み ---
    println!("\n=== VRMA 読み込み ===");
    let vrma = match load_vrma(vrma_path) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("VRMA 読み込みエラー: {}", e);
            return;
        }
    };
    println!(
        "  名前: {}, duration: {:.2}s, ボーンch: {}, is_additive: {}",
        vrma.name,
        vrma.duration,
        vrma.bone_channels.len(),
        vrma.is_additive,
    );

    // --- t=0 でのローカル回転を計算 ---
    let t = 0.0f32;

    // .anim: is_additive=false, Humanoid mode → retarget パス（VRMA と同じ公式）
    let mut anim_local_rots: HashMap<usize, Quat> = HashMap::new();
    let mut anim_raw_deltas: HashMap<usize, Quat> = HashMap::new();
    let anim_is_humanoid = matches!(anim.match_mode, BoneMatchMode::Humanoid);
    for bone_name in anim.bone_channels.keys() {
        if let Some(&bone_idx) = skel.bone_name_to_idx.get(bone_name.as_str()) {
            if let Some(anim_rot) = anim.sample_bone_rotation(bone_name, t) {
                anim_raw_deltas.insert(bone_idx, anim_rot);

                let local_rot = if anim.is_additive && anim.is_bone_local_delta {
                    // ボーンローカルデルタ（Unity Muscle SwingTwist）:
                    // anim_rot = postQ × SwingTwist × Inv(postQ)
                    // 最終ローカル回転 = rest × anim_rot
                    skel.rest_local_rotations[bone_idx] * anim_rot
                } else if anim.is_additive {
                    // ワールド空間デルタ→共役変換でローカルデルタ
                    let parent_rest_rot = skel.bone_parents[bone_idx]
                        .map(|pi| skel.rest_global_rotations[pi])
                        .unwrap_or(Quat::IDENTITY);
                    let local_delta = parent_rest_rot.inverse() * anim_rot * parent_rest_rot;
                    local_delta * skel.rest_local_rotations[bone_idx]
                } else if anim_is_humanoid {
                    // Retarget モード（VRMA と同じ公式）
                    if let Some(canon_rest) = anim.bone_rests.get(bone_name.as_str()) {
                        let w_canon = canon_rest.global_rotation;
                        let l_canon = canon_rest.local_rotation;
                        let l_model = skel.rest_local_rotations[bone_idx];
                        let w_model = skel.rest_global_rotations[bone_idx];

                        let normalized = w_canon * l_canon.inverse() * anim_rot * w_canon.inverse();
                        l_model * w_model.inverse() * normalized * w_model
                    } else {
                        anim_rot
                    }
                } else {
                    anim_rot
                };
                anim_local_rots.insert(bone_idx, local_rot);
            }
        }
    }

    // VRMA: is_additive=false, Humanoid mode → リターゲティング公式
    let mut vrma_local_rots: HashMap<usize, Quat> = HashMap::new();
    let mut vrma_raw_values: HashMap<usize, Quat> = HashMap::new();
    let is_humanoid = matches!(vrma.match_mode, BoneMatchMode::Humanoid);
    for bone_name in vrma.bone_channels.keys() {
        if let Some(&bone_idx) = skel.bone_name_to_idx.get(bone_name.as_str()) {
            if let Some(anim_rot) = vrma.sample_bone_rotation(bone_name, t) {
                vrma_raw_values.insert(bone_idx, anim_rot);

                let local_rot = if is_humanoid {
                    if let Some(vrma_rest) = vrma.bone_rests.get(bone_name.as_str()) {
                        let w_vrma = vrma_rest.global_rotation;
                        let l_vrma = vrma_rest.local_rotation;
                        let l_model = skel.rest_local_rotations[bone_idx];
                        let w_model = skel.rest_global_rotations[bone_idx];

                        let normalized = w_vrma * l_vrma.inverse() * anim_rot * w_vrma.inverse();
                        l_model * w_model.inverse() * normalized * w_model
                    } else {
                        anim_rot
                    }
                } else {
                    anim_rot
                };
                vrma_local_rots.insert(bone_idx, local_rot);
            }
        }
    }

    // --- グローバル行列を計算 ---
    let anim_globals = compute_globals(&skel, &anim_local_rots);
    let vrma_globals = compute_globals(&skel, &vrma_local_rots);

    // --- 共通ボーンの比較結果を収集 ---
    struct BoneResult {
        name: String,
        #[allow(dead_code)]
        bone_idx: usize,
        anim_global_rot: Quat,
        vrma_global_rot: Quat,
        anim_raw: Quat,
        vrma_raw: Quat,
        diff_deg: f32,
    }

    let mut results: Vec<BoneResult> = Vec::new();

    // 共通ボーン = anim と vrma の両方にチャネルがあるボーン
    let anim_bone_names: std::collections::HashSet<&str> =
        anim.bone_channels.keys().map(|s| s.as_str()).collect();
    let vrma_bone_names: std::collections::HashSet<&str> =
        vrma.bone_channels.keys().map(|s| s.as_str()).collect();

    for bone_name in anim_bone_names.intersection(&vrma_bone_names) {
        if let Some(&bone_idx) = skel.bone_name_to_idx.get(*bone_name) {
            let (_, anim_global_rot, _) = anim_globals[bone_idx].to_scale_rotation_translation();
            let (_, vrma_global_rot, _) = vrma_globals[bone_idx].to_scale_rotation_translation();

            let anim_raw = anim_raw_deltas
                .get(&bone_idx)
                .copied()
                .unwrap_or(Quat::IDENTITY);
            let vrma_raw = vrma_raw_values
                .get(&bone_idx)
                .copied()
                .unwrap_or(Quat::IDENTITY);

            // クォータニオン間の角度差
            let dot = anim_global_rot.dot(vrma_global_rot).abs().min(1.0);
            let diff_deg = 2.0 * dot.acos().to_degrees();

            results.push(BoneResult {
                name: bone_name.to_string(),
                bone_idx,
                anim_global_rot,
                vrma_global_rot,
                anim_raw,
                vrma_raw,
                diff_deg,
            });
        }
    }

    // 角度差の大きい順にソート
    results.sort_by(|a, b| b.diff_deg.partial_cmp(&a.diff_deg).unwrap());

    // --- 出力 ---
    println!("\n=== 最終グローバル回転比較 (t={:.1}) ===", t);
    println!(
        "  .anim: is_additive={} → retarget with canonical bone rests",
        anim.is_additive
    );
    println!("  VRMA:  Humanoid retarget → local = L_model * W_model^-1 * normalized * W_model");
    println!("  差が大きい順にソート");
    println!();
    println!(
        "{:<28} {:>8}  {:>42}  {:>42}",
        "bone", "diff°", ".anim global quat (x,y,z,w)", "VRMA global quat (x,y,z,w)"
    );
    println!("{}", "-".repeat(130));

    for r in &results {
        let ag = r.anim_global_rot;
        let vg = r.vrma_global_rot;
        println!(
            "{:<28} {:>7.2}°  ({:>8.4}, {:>8.4}, {:>8.4}, {:>8.4})  ({:>8.4}, {:>8.4}, {:>8.4}, {:>8.4})",
            r.name, r.diff_deg,
            ag.x, ag.y, ag.z, ag.w,
            vg.x, vg.y, vg.z, vg.w,
        );
    }

    // --- raw 値も表示 ---
    println!("\n=== Raw アニメーション値 (t={:.1}) ===", t);
    println!("  .anim raw = additive delta（IDENTITY=動きなし）");
    println!("  VRMA raw  = VRMA ノードローカル回転（リターゲティング前）");
    println!();
    println!(
        "{:<28} {:>8}  {:>42}  {:>42}  {:>8}  {:>8}",
        "bone", "diff°", ".anim raw delta (x,y,z,w)", "VRMA raw value (x,y,z,w)", "anim°", "vrma°"
    );
    println!("{}", "-".repeat(170));

    for r in &results {
        let ad = r.anim_raw;
        let vr = r.vrma_raw;
        let anim_angle = ad.to_axis_angle().1.to_degrees();
        // VRMA delta from rest
        let vrma_rest_rot = vrma
            .bone_rests
            .get(r.name.as_str())
            .map(|r| r.local_rotation)
            .unwrap_or(Quat::IDENTITY);
        let vrma_delta = vrma_rest_rot.inverse() * vr;
        let vrma_delta_angle = vrma_delta.to_axis_angle().1.to_degrees();

        println!(
            "{:<28} {:>7.2}°  ({:>8.4}, {:>8.4}, {:>8.4}, {:>8.4})  ({:>8.4}, {:>8.4}, {:>8.4}, {:>8.4})  {:>7.2}°  {:>7.2}°",
            r.name, r.diff_deg,
            ad.x, ad.y, ad.z, ad.w,
            vr.x, vr.y, vr.z, vr.w,
            anim_angle, vrma_delta_angle,
        );
    }

    // --- .anim のみに存在するボーン ---
    println!("\n=== .anim のみに存在するボーン（VRM にマッチしたもの） ===");
    for bone_name in &anim_bone_names {
        if !vrma_bone_names.contains(bone_name) {
            if let Some(&bone_idx) = skel.bone_name_to_idx.get(*bone_name) {
                let raw = anim_raw_deltas
                    .get(&bone_idx)
                    .copied()
                    .unwrap_or(Quat::IDENTITY);
                let angle = raw.to_axis_angle().1.to_degrees();
                println!(
                    "  {:<28} delta=({:.4}, {:.4}, {:.4}, {:.4}), angle={:.2}°",
                    bone_name, raw.x, raw.y, raw.z, raw.w, angle
                );
            }
        }
    }

    // --- VRMA のみに存在するボーン ---
    println!("\n=== VRMA のみに存在するボーン（VRM にマッチしたもの） ===");
    for bone_name in &vrma_bone_names {
        if !anim_bone_names.contains(bone_name) {
            if let Some(&bone_idx) = skel.bone_name_to_idx.get(*bone_name) {
                let (_, g, _) = vrma_globals[bone_idx].to_scale_rotation_translation();
                let raw = vrma_raw_values
                    .get(&bone_idx)
                    .copied()
                    .unwrap_or(Quat::IDENTITY);
                println!(
                    "  {:<28} global=({:.4}, {:.4}, {:.4}, {:.4}), raw=({:.4}, {:.4}, {:.4}, {:.4})",
                    bone_name, g.x, g.y, g.z, g.w, raw.x, raw.y, raw.z, raw.w,
                );
            }
        }
    }

    // --- レストポーズ参考情報 ---
    println!("\n=== VRM モデルのレストポーズ（ヒューマノイドボーンのみ） ===");
    let mut sorted_humanoid: Vec<(&String, &usize)> = skel.bone_name_to_idx.iter().collect();
    sorted_humanoid.sort_by_key(|(name, _)| name.to_string());
    for (name, &idx) in &sorted_humanoid {
        let lr = skel.rest_local_rotations[idx];
        let gr = skel.rest_global_rotations[idx];
        let local_angle = lr.to_axis_angle().1.to_degrees();
        println!(
            "  {:<28} local=({:>8.4}, {:>8.4}, {:>8.4}, {:>8.4}) [{:>6.1}°]  global=({:>8.4}, {:>8.4}, {:>8.4}, {:>8.4})",
            name,
            lr.x, lr.y, lr.z, lr.w, local_angle,
            gr.x, gr.y, gr.z, gr.w,
        );
    }

    // --- VRMA レストポーズ ---
    println!("\n=== VRMA レストポーズ（上腕・前腕・手・指のみ） ===");
    let arm_finger_bones = [
        "leftUpperArm",
        "leftLowerArm",
        "leftHand",
        "rightUpperArm",
        "rightLowerArm",
        "rightHand",
        "leftIndexProximal",
        "leftIndexIntermediate",
        "rightIndexProximal",
        "rightIndexIntermediate",
    ];
    for name in &arm_finger_bones {
        if let Some(rest) = vrma.bone_rests.get(*name) {
            println!(
                "  {:<28} local=({:>8.4}, {:>8.4}, {:>8.4}, {:>8.4})  global=({:>8.4}, {:>8.4}, {:>8.4}, {:>8.4})",
                name,
                rest.local_rotation.x, rest.local_rotation.y, rest.local_rotation.z, rest.local_rotation.w,
                rest.global_rotation.x, rest.global_rotation.y, rest.global_rotation.z, rest.global_rotation.w,
            );
        }
    }

    // --- 正解 additive anim_rot の逆算 ---
    // VRMAのローカル回転から、additive方式で必要な anim_rot を逆算する
    // local_rot = local_delta * R_local_rest
    // local_delta = P_g^-1 * anim_rot * P_g
    // → anim_rot = P_g * local_delta * P_g^-1
    // → anim_rot = P_g * (local_rot * R_local_rest^-1) * P_g^-1
    // ここで local_rot = VRMA retarget 後のローカル回転
    println!("\n=== 正解 anim_rot 逆算（VRMA → 期待される additive delta） ===");
    println!(
        "{:<28} {:>42}  {:>42}  {:>8}  {:>8}",
        "bone", "correct anim_rot (x,y,z,w)", ".anim anim_rot (x,y,z,w)", "correct°", ".anim°"
    );
    println!("{}", "-".repeat(150));

    for bone_name in &arm_finger_bones {
        if let Some(&bone_idx) = skel.bone_name_to_idx.get(*bone_name) {
            // VRMAからの正しいローカル回転を取得
            if let Some(vrma_local) = vrma_local_rots.get(&bone_idx) {
                let rest_local = skel.rest_local_rotations[bone_idx];
                let local_delta = *vrma_local * rest_local.inverse();

                let parent_rest_rot = skel.bone_parents[bone_idx]
                    .map(|pi| skel.rest_global_rotations[pi])
                    .unwrap_or(Quat::IDENTITY);
                // anim_rot = P_g * local_delta * P_g^-1
                let correct_anim_rot = parent_rest_rot * local_delta * parent_rest_rot.inverse();
                let correct_angle = correct_anim_rot.to_axis_angle().1.to_degrees();

                // .animの anim_rot
                let anim_raw = anim_raw_deltas
                    .get(&bone_idx)
                    .copied()
                    .unwrap_or(Quat::IDENTITY);
                let anim_angle = anim_raw.to_axis_angle().1.to_degrees();

                // bone rest global (= P_g * R_local)
                let bone_global = skel.rest_global_rotations[bone_idx];

                println!(
                    "{:<28} ({:>8.4}, {:>8.4}, {:>8.4}, {:>8.4})  ({:>8.4}, {:>8.4}, {:>8.4}, {:>8.4})  {:>7.2}°  {:>7.2}°  G=({:.3},{:.3},{:.3},{:.3})",
                    bone_name,
                    correct_anim_rot.x, correct_anim_rot.y, correct_anim_rot.z, correct_anim_rot.w,
                    anim_raw.x, anim_raw.y, anim_raw.z, anim_raw.w,
                    correct_angle, anim_angle,
                    bone_global.x, bone_global.y, bone_global.z, bone_global.w,
                );
            }
        }
    }

    // --- サマリー ---
    println!("\n=== サマリー ===");
    let large_diff: Vec<&BoneResult> = results.iter().filter(|r| r.diff_deg > 5.0).collect();
    let medium_diff: Vec<&BoneResult> = results
        .iter()
        .filter(|r| r.diff_deg > 1.0 && r.diff_deg <= 5.0)
        .collect();
    let small_diff: Vec<&BoneResult> = results.iter().filter(|r| r.diff_deg <= 1.0).collect();

    println!("  共通ボーン数: {}", results.len());
    println!("  大きな差 (>5°): {} ボーン", large_diff.len());
    for r in &large_diff {
        println!("    - {} ({:.1}°)", r.name, r.diff_deg);
    }
    println!("  中程度の差 (1-5°): {} ボーン", medium_diff.len());
    for r in &medium_diff {
        println!("    - {} ({:.1}°)", r.name, r.diff_deg);
    }
    println!("  小さな差 (<1°): {} ボーン", small_diff.len());

    if !results.is_empty() {
        let avg_diff: f32 = results.iter().map(|r| r.diff_deg).sum::<f32>() / results.len() as f32;
        let max_diff = results[0].diff_deg;
        println!("  平均差: {:.2}°", avg_diff);
        println!("  最大差: {:.2}° ({})", max_diff, results[0].name);
    }

    println!("\n完了");
}
