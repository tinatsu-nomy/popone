use crate::error::{PoponeError, Result, ResultExt};
use glam::{Quat, Vec3};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::intermediate::animation::*;

/// Unity .anim ファイルからアニメーションを読み込む
///
/// `muscle_scale`: Muscle角度の倍率（デフォルト1.0）。通常は変更不要。
pub fn load_unity_anim(path: &Path, muscle_scale: f32) -> Result<VrmaAnimation> {
    load_unity_anim_with_params(path, muscle_scale, None)
}

/// Unity .anim ファイルからアニメーションを読み込む（パラメータJSON指定可能）
///
/// `params_path`: DumpHumanoidParams.cs で出力した JSON パス（任意）。
/// 指定すると model-specific な preQ/postQ/sign を使用して高精度な変換を行う。
/// 未指定の場合は V-Sekai 正規化スケルトンのフォールバック値を使用。
pub fn load_unity_anim_with_params(
    path: &Path,
    muscle_scale: f32,
    params_path: Option<&Path>,
) -> Result<VrmaAnimation> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("Unity .animファイルの読み込みに失敗: {}", path.display()))?;
    let reader = BufReader::new(file);

    let parsed = parse_anim_yaml(reader)?;

    let name = parsed.name.unwrap_or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unity_anim".to_string())
    });

    // Unity パラメータ読み込み
    let params = if let Some(pp) = params_path {
        Some(load_humanoid_params(pp)?)
    } else {
        None
    };
    let has_params = params.is_some();

    // Muscle カーブを VRM ヒューマノイドボーンチャネルに変換
    let bone_channels = build_bone_channels(
        &parsed.float_curves,
        parsed.duration,
        muscle_scale,
        params.as_ref(),
    );

    let mode_name = if has_params { "params" } else { "fallback" };
    log::info!(
        "Unity .anim読み込み: '{}' ボーン{}ch, {:.2}秒, muscle_scale={:.2}, mode={}",
        name,
        bone_channels.len(),
        parsed.duration,
        muscle_scale,
        mode_name,
    );

    // パラメータ使用時: 絶対ローカル回転出力（is_additive=false）
    // フォールバック時: 正規化デルタ出力（is_additive=true, is_bone_local_delta=true）
    Ok(VrmaAnimation {
        name,
        duration: parsed.duration,
        bone_channels,
        expression_channels: HashMap::new(),
        bone_rests: HashMap::new(),
        match_mode: BoneMatchMode::Humanoid,
        facing_flip_y: false,
        is_additive: !has_params,
        is_bone_local_delta: !has_params,
    })
}

// ─── YAML パーサー ───

struct ParsedAnim {
    name: Option<String>,
    duration: f32,
    float_curves: Vec<AnimCurve>,
}

struct AnimCurve {
    attribute: String,
    keyframes: Vec<(f32, f32)>, // (time, value)
}

/// 行ベースの高速 Unity YAML パーサー
fn parse_anim_yaml(reader: BufReader<std::fs::File>) -> Result<ParsedAnim> {
    let mut name: Option<String> = None;
    let mut duration: f32 = 0.0;
    let mut float_curves: Vec<AnimCurve> = Vec::new();

    // 現在の状態
    let mut in_float_curves = false;
    let mut in_curve_data = false; // curve.m_Curve 配列内
    let mut current_attribute = String::new();
    let mut current_keyframes: Vec<(f32, f32)> = Vec::new();
    let mut current_time: Option<f32> = None;
    let mut current_value: Option<f32> = None;
    let mut indent_float_curves = 0usize; // m_FloatCurves のインデントレベル

    let mut line_num = 0u32;
    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(_) => continue,
        };
        line_num += 1;

        // ヘッダチェック（最初の数行のみ）
        if line_num <= 3 {
            if line_num == 1 && !line.starts_with("%YAML") {
                return Err(PoponeError::Other(
                    "Unity YAMLファイルではありません".into(),
                ));
            }
            continue;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let indent = line.len() - line.trim_start().len();

        // m_Name
        if trimmed.starts_with("m_Name:") {
            if name.is_none() {
                name = Some(trimmed.trim_start_matches("m_Name:").trim().to_string());
            }
            continue;
        }

        // m_StopTime
        if trimmed.starts_with("m_StopTime:") {
            if let Some(val) = trimmed.split(':').nth(1) {
                duration = val.trim().parse().unwrap_or(0.0);
            }
            continue;
        }

        // m_FloatCurves セクション開始
        if trimmed == "m_FloatCurves:" || trimmed == "m_FloatCurves: []" {
            in_float_curves = trimmed != "m_FloatCurves: []";
            indent_float_curves = indent;
            continue;
        }

        // m_FloatCurves セクション終了判定（同レベルか浅い別フィールドが来たら）
        if in_float_curves
            && indent <= indent_float_curves
            && !trimmed.starts_with('-')
            && trimmed.contains(':')
        {
            // 現在のカーブを保存
            if !current_attribute.is_empty() && !current_keyframes.is_empty() {
                float_curves.push(AnimCurve {
                    attribute: std::mem::take(&mut current_attribute),
                    keyframes: std::mem::take(&mut current_keyframes),
                });
            }
            in_float_curves = false;
            in_curve_data = false;
            continue;
        }

        if !in_float_curves {
            continue;
        }

        // attribute フィールド（m_Curve データの後に来る）
        // Unity YAML: 各エントリ = { curve: { m_Curve: [...] }, attribute: "名前", ... }
        // m_Curve → attribute の順なので、attribute 到達時にキーフレームは収集済み
        if trimmed.starts_with("attribute:") {
            current_attribute = trimmed.trim_start_matches("attribute:").trim().to_string();
            if !current_attribute.is_empty() && !current_keyframes.is_empty() {
                float_curves.push(AnimCurve {
                    attribute: std::mem::take(&mut current_attribute),
                    keyframes: std::mem::take(&mut current_keyframes),
                });
            }
            in_curve_data = false;
            continue;
        }

        // m_Curve 配列開始
        if trimmed == "m_Curve:" {
            in_curve_data = true;
            continue;
        }

        // m_Curve 配列内のキーフレーム
        if in_curve_data {
            // キーフレームエントリ開始
            if trimmed.starts_with("- serializedVersion:") {
                // 前のキーフレームを保存
                if let (Some(t), Some(v)) = (current_time.take(), current_value.take()) {
                    current_keyframes.push((t, v));
                }
                continue;
            }

            if trimmed.starts_with("time:") {
                current_time = trimmed
                    .split(':')
                    .nth(1)
                    .and_then(|s| s.trim().parse().ok());
                continue;
            }

            if trimmed.starts_with("value:") {
                current_value = trimmed
                    .split(':')
                    .nth(1)
                    .and_then(|s| s.trim().parse().ok());
                continue;
            }

            // m_Curve 配列終了（m_PreInfinity 等が来たら）
            if !trimmed.starts_with('-')
                && !trimmed.starts_with("time:")
                && !trimmed.starts_with("value:")
                && !trimmed.starts_with("inSlope:")
                && !trimmed.starts_with("outSlope:")
                && !trimmed.starts_with("tangentMode:")
                && !trimmed.starts_with("weightedMode:")
                && !trimmed.starts_with("inWeight:")
                && !trimmed.starts_with("outWeight:")
                && !trimmed.starts_with("serializedVersion:")
            {
                // 最後のキーフレームを保存
                if let (Some(t), Some(v)) = (current_time.take(), current_value.take()) {
                    current_keyframes.push((t, v));
                }
                in_curve_data = false;
            }
        }
    }

    // 最後のカーブを保存
    if !current_attribute.is_empty() && !current_keyframes.is_empty() {
        float_curves.push(AnimCurve {
            attribute: std::mem::take(&mut current_attribute),
            keyframes: std::mem::take(&mut current_keyframes),
        });
    }

    log::info!(
        "Unity .animパース完了: カーブ{}本, duration={:.2}秒",
        float_curves.len(),
        duration,
    );

    Ok(ParsedAnim {
        name,
        duration,
        float_curves,
    })
}

// ─── SwingTwist 変換 (ShaderMotion 準拠) ───

/// SwingTwist分解による回転構築
///
/// degrees = (twist_x, swing_y, swing_z) in degrees
/// SwingTwist(deg) = AngleAxis(|yz|, normalize(yz)) × AngleAxis(x, (1,0,0))
fn swing_twist(x_deg: f32, y_deg: f32, z_deg: f32) -> Quat {
    // Twist around X axis
    let twist = Quat::from_axis_angle(Vec3::X, x_deg.to_radians());
    // Swing in YZ plane
    let yz_mag = (y_deg * y_deg + z_deg * z_deg).sqrt();
    if yz_mag < 1e-6 {
        return twist;
    }
    let swing_axis = Vec3::new(0.0, y_deg / yz_mag, z_deg / yz_mag);
    let swing = Quat::from_axis_angle(swing_axis, yz_mag.to_radians());
    swing * twist
}

// ─── Unity Humanoid 定数テーブル ───

/// Muscle名一覧 (index = muscle index)
const MUSCLE_NAMES: [&str; 95] = [
    "Spine Front-Back",
    "Spine Left-Right",
    "Spine Twist Left-Right",
    "Chest Front-Back",
    "Chest Left-Right",
    "Chest Twist Left-Right",
    "UpperChest Front-Back",
    "UpperChest Left-Right",
    "UpperChest Twist Left-Right",
    "Neck Nod Down-Up",
    "Neck Tilt Left-Right",
    "Neck Turn Left-Right",
    "Head Nod Down-Up",
    "Head Tilt Left-Right",
    "Head Turn Left-Right",
    "Left Eye Down-Up",
    "Left Eye In-Out",
    "Right Eye Down-Up",
    "Right Eye In-Out",
    "Jaw Close",
    "Jaw Left-Right",
    "Left Upper Leg Front-Back",
    "Left Upper Leg In-Out",
    "Left Upper Leg Twist In-Out",
    "Left Lower Leg Stretch",
    "Left Lower Leg Twist In-Out",
    "Left Foot Up-Down",
    "Left Foot Twist In-Out",
    "Left Toes Up-Down",
    "Right Upper Leg Front-Back",
    "Right Upper Leg In-Out",
    "Right Upper Leg Twist In-Out",
    "Right Lower Leg Stretch",
    "Right Lower Leg Twist In-Out",
    "Right Foot Up-Down",
    "Right Foot Twist In-Out",
    "Right Toes Up-Down",
    "Left Shoulder Down-Up",
    "Left Shoulder Front-Back",
    "Left Arm Down-Up",
    "Left Arm Front-Back",
    "Left Arm Twist In-Out",
    "Left Forearm Stretch",
    "Left Forearm Twist In-Out",
    "Left Hand Down-Up",
    "Left Hand In-Out",
    "Right Shoulder Down-Up",
    "Right Shoulder Front-Back",
    "Right Arm Down-Up",
    "Right Arm Front-Back",
    "Right Arm Twist In-Out",
    "Right Forearm Stretch",
    "Right Forearm Twist In-Out",
    "Right Hand Down-Up",
    "Right Hand In-Out",
    "LeftHand.Thumb.1 Stretched",
    "LeftHand.Thumb.Spread",
    "LeftHand.Thumb.2 Stretched",
    "LeftHand.Thumb.3 Stretched",
    "LeftHand.Index.1 Stretched",
    "LeftHand.Index.Spread",
    "LeftHand.Index.2 Stretched",
    "LeftHand.Index.3 Stretched",
    "LeftHand.Middle.1 Stretched",
    "LeftHand.Middle.Spread",
    "LeftHand.Middle.2 Stretched",
    "LeftHand.Middle.3 Stretched",
    "LeftHand.Ring.1 Stretched",
    "LeftHand.Ring.Spread",
    "LeftHand.Ring.2 Stretched",
    "LeftHand.Ring.3 Stretched",
    "LeftHand.Little.1 Stretched",
    "LeftHand.Little.Spread",
    "LeftHand.Little.2 Stretched",
    "LeftHand.Little.3 Stretched",
    "RightHand.Thumb.1 Stretched",
    "RightHand.Thumb.Spread",
    "RightHand.Thumb.2 Stretched",
    "RightHand.Thumb.3 Stretched",
    "RightHand.Index.1 Stretched",
    "RightHand.Index.Spread",
    "RightHand.Index.2 Stretched",
    "RightHand.Index.3 Stretched",
    "RightHand.Middle.1 Stretched",
    "RightHand.Middle.Spread",
    "RightHand.Middle.2 Stretched",
    "RightHand.Middle.3 Stretched",
    "RightHand.Ring.1 Stretched",
    "RightHand.Ring.Spread",
    "RightHand.Ring.2 Stretched",
    "RightHand.Ring.3 Stretched",
    "RightHand.Little.1 Stretched",
    "RightHand.Little.Spread",
    "RightHand.Little.2 Stretched",
    "RightHand.Little.3 Stretched",
];

/// Muscle デフォルト最小角度 (HumanTrait.GetMuscleDefaultMin)
const MUSCLE_DEFAULT_MIN: [f32; 95] = [
    -40.0, -40.0, -40.0, -40.0, -40.0, -40.0, -20.0, -20.0, -20.0, -40.0, -40.0, -40.0, -40.0,
    -40.0, -40.0, -10.0, -20.0, -10.0, -20.0, -10.0, -10.0, -90.0, -60.0, -60.0, -80.0, -90.0,
    -50.0, -30.0, -50.0, -90.0, -60.0, -60.0, -80.0, -90.0, -50.0, -30.0, -50.0, -15.0, -15.0,
    -60.0, -100.0, -90.0, -80.0, -90.0, -80.0, -40.0, -15.0, -15.0, -60.0, -100.0, -90.0, -80.0,
    -90.0, -80.0, -40.0, -20.0, -25.0, -40.0, -40.0, -50.0, -20.0, -45.0, -45.0, -50.0, -7.5,
    -45.0, -45.0, -50.0, -7.5, -45.0, -45.0, -50.0, -20.0, -45.0, -45.0, -20.0, -25.0, -40.0,
    -40.0, -50.0, -20.0, -45.0, -45.0, -50.0, -7.5, -45.0, -45.0, -50.0, -7.5, -45.0, -45.0, -50.0,
    -20.0, -45.0, -45.0,
];

/// Muscle デフォルト最大角度 (HumanTrait.GetMuscleDefaultMax)
const MUSCLE_DEFAULT_MAX: [f32; 95] = [
    40.0, 40.0, 40.0, 40.0, 40.0, 40.0, 20.0, 20.0, 20.0, 40.0, 40.0, 40.0, 40.0, 40.0, 40.0, 15.0,
    20.0, 15.0, 20.0, 10.0, 10.0, 50.0, 60.0, 60.0, 80.0, 90.0, 50.0, 30.0, 50.0, 50.0, 60.0, 60.0,
    80.0, 90.0, 50.0, 30.0, 50.0, 30.0, 15.0, 100.0, 100.0, 90.0, 80.0, 90.0, 80.0, 40.0, 30.0,
    15.0, 100.0, 100.0, 90.0, 80.0, 90.0, 80.0, 40.0, 20.0, 25.0, 35.0, 35.0, 50.0, 20.0, 45.0,
    45.0, 50.0, 7.5, 45.0, 45.0, 50.0, 7.5, 45.0, 45.0, 50.0, 20.0, 45.0, 45.0, 20.0, 25.0, 35.0,
    35.0, 50.0, 20.0, 45.0, 45.0, 50.0, 7.5, 45.0, 45.0, 50.0, 7.5, 45.0, 45.0, 50.0, 20.0, 45.0,
    45.0,
];

/// ボーンインデックス → (twist_muscle, swing_y_muscle, swing_z_muscle)
/// -1 は該当 DOF なし
const MUSCLE_FROM_BONE: [(i8, i8, i8); 55] = [
    (-1, -1, -1), // 0: Hips
    (23, 22, 21), // 1: LeftUpperLeg
    (31, 30, 29), // 2: RightUpperLeg
    (25, -1, 24), // 3: LeftLowerLeg
    (33, -1, 32), // 4: RightLowerLeg
    (-1, 27, 26), // 5: LeftFoot
    (-1, 35, 34), // 6: RightFoot
    (2, 1, 0),    // 7: Spine
    (5, 4, 3),    // 8: Chest
    (11, 10, 9),  // 9: Neck
    (14, 13, 12), // 10: Head
    (-1, 38, 37), // 11: LeftShoulder
    (-1, 47, 46), // 12: RightShoulder
    (41, 40, 39), // 13: LeftUpperArm
    (50, 49, 48), // 14: RightUpperArm
    (43, -1, 42), // 15: LeftLowerArm
    (52, -1, 51), // 16: RightLowerArm
    (-1, 45, 44), // 17: LeftHand
    (-1, 54, 53), // 18: RightHand
    (-1, -1, 28), // 19: LeftToes
    (-1, -1, 36), // 20: RightToes
    (-1, 16, 15), // 21: LeftEye
    (-1, 18, 17), // 22: RightEye
    (-1, 20, 19), // 23: Jaw
    (-1, 56, 55), // 24: LeftThumbMetacarpal
    (-1, -1, 57), // 25: LeftThumbProximal
    (-1, -1, 58), // 26: LeftThumbDistal
    (-1, 60, 59), // 27: LeftIndexProximal
    (-1, -1, 61), // 28: LeftIndexIntermediate
    (-1, -1, 62), // 29: LeftIndexDistal
    (-1, 64, 63), // 30: LeftMiddleProximal
    (-1, -1, 65), // 31: LeftMiddleIntermediate
    (-1, -1, 66), // 32: LeftMiddleDistal
    (-1, 68, 67), // 33: LeftRingProximal
    (-1, -1, 69), // 34: LeftRingIntermediate
    (-1, -1, 70), // 35: LeftRingDistal
    (-1, 72, 71), // 36: LeftLittleProximal
    (-1, -1, 73), // 37: LeftLittleIntermediate
    (-1, -1, 74), // 38: LeftLittleDistal
    (-1, 76, 75), // 39: RightThumbMetacarpal
    (-1, -1, 77), // 40: RightThumbProximal
    (-1, -1, 78), // 41: RightThumbDistal
    (-1, 80, 79), // 42: RightIndexProximal
    (-1, -1, 81), // 43: RightIndexIntermediate
    (-1, -1, 82), // 44: RightIndexDistal
    (-1, 84, 83), // 45: RightMiddleProximal
    (-1, -1, 85), // 46: RightMiddleIntermediate
    (-1, -1, 86), // 47: RightMiddleDistal
    (-1, 88, 87), // 48: RightRingProximal
    (-1, -1, 89), // 49: RightRingIntermediate
    (-1, -1, 90), // 50: RightRingDistal
    (-1, 92, 91), // 51: RightLittleProximal
    (-1, -1, 93), // 52: RightLittleIntermediate
    (-1, -1, 94), // 53: RightLittleDistal
    (8, 7, 6),    // 54: UpperChest
];

/// Per-bone sign values (V-Sekai/unidot_importer, GetLimitSign)
const SIGNS: [(f32, f32, f32); 55] = [
    (1.0, 1.0, 1.0),    // 0: Hips
    (1.0, 1.0, 1.0),    // 1: LeftUpperLeg
    (-1.0, -1.0, 1.0),  // 2: RightUpperLeg
    (1.0, -1.0, -1.0),  // 3: LeftLowerLeg
    (-1.0, 1.0, -1.0),  // 4: RightLowerLeg
    (1.0, 1.0, 1.0),    // 5: LeftFoot
    (-1.0, -1.0, 1.0),  // 6: RightFoot
    (1.0, 1.0, 1.0),    // 7: Spine
    (1.0, 1.0, 1.0),    // 8: Chest
    (1.0, 1.0, 1.0),    // 9: Neck
    (1.0, 1.0, 1.0),    // 10: Head
    (1.0, 1.0, -1.0),   // 11: LeftShoulder
    (-1.0, 1.0, 1.0),   // 12: RightShoulder
    (1.0, 1.0, -1.0),   // 13: LeftUpperArm
    (-1.0, 1.0, 1.0),   // 14: RightUpperArm
    (1.0, 1.0, -1.0),   // 15: LeftLowerArm
    (-1.0, 1.0, 1.0),   // 16: RightLowerArm
    (1.0, 1.0, -1.0),   // 17: LeftHand
    (-1.0, 1.0, 1.0),   // 18: RightHand
    (1.0, 1.0, 1.0),    // 19: LeftToes
    (-1.0, -1.0, 1.0),  // 20: RightToes
    (-1.0, 1.0, -1.0),  // 21: LeftEye
    (1.0, -1.0, -1.0),  // 22: RightEye
    (1.0, 1.0, 1.0),    // 23: Jaw
    (1.0, -1.0, 1.0),   // 24: LeftThumbMetacarpal
    (1.0, -1.0, 1.0),   // 25: LeftThumbProximal
    (1.0, -1.0, 1.0),   // 26: LeftThumbDistal
    (-1.0, -1.0, -1.0), // 27: LeftIndexProximal
    (-1.0, -1.0, -1.0), // 28: LeftIndexIntermediate
    (-1.0, -1.0, -1.0), // 29: LeftIndexDistal
    (-1.0, -1.0, -1.0), // 30: LeftMiddleProximal
    (-1.0, -1.0, -1.0), // 31: LeftMiddleIntermediate
    (-1.0, -1.0, -1.0), // 32: LeftMiddleDistal
    (1.0, 1.0, -1.0),   // 33: LeftRingProximal
    (1.0, 1.0, -1.0),   // 34: LeftRingIntermediate
    (1.0, 1.0, -1.0),   // 35: LeftRingDistal
    (1.0, 1.0, -1.0),   // 36: LeftLittleProximal
    (1.0, 1.0, -1.0),   // 37: LeftLittleIntermediate
    (1.0, 1.0, -1.0),   // 38: LeftLittleDistal
    (-1.0, -1.0, -1.0), // 39: RightThumbMetacarpal
    (-1.0, -1.0, -1.0), // 40: RightThumbProximal
    (-1.0, -1.0, -1.0), // 41: RightThumbDistal
    (1.0, -1.0, 1.0),   // 42: RightIndexProximal
    (1.0, -1.0, 1.0),   // 43: RightIndexIntermediate
    (1.0, -1.0, 1.0),   // 44: RightIndexDistal
    (1.0, -1.0, 1.0),   // 45: RightMiddleProximal
    (1.0, -1.0, 1.0),   // 46: RightMiddleIntermediate
    (1.0, -1.0, 1.0),   // 47: RightMiddleDistal
    (-1.0, 1.0, 1.0),   // 48: RightRingProximal
    (-1.0, 1.0, 1.0),   // 49: RightRingIntermediate
    (-1.0, 1.0, 1.0),   // 50: RightRingDistal
    (-1.0, 1.0, 1.0),   // 51: RightLittleProximal
    (-1.0, 1.0, 1.0),   // 52: RightLittleIntermediate
    (-1.0, 1.0, 1.0),   // 53: RightLittleDistal
    (1.0, 1.0, 1.0),    // 54: UpperChest
];

/// V-Sekai 正規化スケルトンの postQ_inverse (右手系/glTF座標系)
/// postQ = conjugate(postQ_inverse) で復元
/// 左右対称ボーンは意図的に同一値（LeftFoot/RightFoot 等）
/// 数値は V-Sekai 参照データの固定値（計算値ではない）
#[allow(clippy::if_same_then_else, clippy::approx_constant)]
/// 正規化スケルトンでは preQ == postQ
const POSTQ_INV_NORMALIZED: [(f32, f32, f32, f32); 55] = [
    (0.0, 0.0, 0.0, 1.0),                        // 0: Hips
    (0.48977, -0.50952, 0.51876, 0.48105),       // 1: LeftUpperLeg
    (0.51876, -0.48105, 0.48977, 0.50952),       // 2: RightUpperLeg
    (-0.51894, 0.48097, 0.50616, 0.49312),       // 3: LeftLowerLeg
    (-0.50616, 0.49312, 0.51894, 0.48097),       // 4: RightLowerLeg
    (-0.707107, 0.0, -0.707107, 0.0),            // 5: LeftFoot
    (-0.707107, 0.0, -0.707107, 0.0),            // 6: RightFoot
    (-0.46815, 0.52994, -0.46815, -0.52994),     // 7: Spine
    (-0.52661, 0.47189, -0.52661, -0.47189),     // 8: Chest
    (-0.46642, 0.5316, -0.46748, -0.5304),       // 9: Neck
    (0.5, -0.5, 0.5, 0.5),                       // 10: Head
    (-0.523995, 0.469295, -0.557075, -0.441435), // 11: LeftShoulder
    (0.46929, -0.524, -0.44143, -0.55708),       // 12: RightShoulder
    (0.513635, -0.486185, -0.509345, -0.490275), // 13: LeftUpperArm
    (0.486185, -0.513635, 0.490275, 0.509345),   // 14: RightUpperArm
    (0.519596, -0.479517, -0.520728, -0.478471), // 15: LeftLowerArm
    (0.479517, -0.519596, 0.478471, 0.520728),   // 16: RightLowerArm
    (0.520725, -0.478465, -0.479515, -0.519595), // 17: LeftHand
    (0.478465, -0.520725, 0.519595, 0.479515),   // 18: RightHand
    (-0.500002, 0.500002, 0.500002, 0.500002),   // 19: LeftToes
    (-0.500002, 0.500002, 0.500002, 0.500002),   // 20: RightToes
    (-0.500002, 0.500002, 0.500002, 0.500002),   // 21: LeftEye
    (-0.500002, 0.500002, 0.500002, 0.500002),   // 22: RightEye
    (0.0, 0.707107, 0.707107, 0.0),              // 23: Jaw
    (0.56005, -0.437881, 0.528429, 0.464077),    // 24: LeftThumbMetacarpal
    (0.541247, -0.458295, 0.513379, 0.483179),   // 25: LeftThumbProximal
    (0.541247, -0.458295, 0.513379, 0.483179),   // 26: LeftThumbDistal
    (0.53845, -0.45868, -0.46056, -0.53625),     // 27: LeftIndexProximal
    (0.53604, -0.46316, -0.47877, -0.51857),     // 28: LeftIndexIntermediate
    (0.53604, -0.46316, -0.47877, -0.51857),     // 29: LeftIndexDistal
    (0.52555, -0.47434, -0.492, -0.50669),       // 30: LeftMiddleProximal
    (0.536385, -0.463085, -0.514795, -0.482515), // 31: LeftMiddleIntermediate
    (0.536385, -0.463085, -0.514795, -0.482515), // 32: LeftMiddleDistal
    (0.50517, -0.49482, -0.50264, -0.49731),     // 33: LeftRingProximal
    (0.494155, -0.505555, -0.487985, -0.511945), // 34: LeftRingIntermediate
    (0.494155, -0.505555, -0.487985, -0.511945), // 35: LeftRingDistal
    (0.502345, -0.497645, -0.501995, -0.497995), // 36: LeftLittleProximal
    (0.47756, -0.52241, -0.50314, -0.49585),     // 37: LeftLittleIntermediate
    (0.47756, -0.52241, -0.50314, -0.49585),     // 38: LeftLittleDistal
    (0.437905, -0.559994, -0.463881, -0.528644), // 39: RightThumbMetacarpal
    (0.458337, -0.5412, -0.483, -0.513558),      // 40: RightThumbProximal
    (0.458337, -0.5412, -0.483, -0.513558),      // 41: RightThumbDistal
    (0.45868, -0.53845, 0.53625, 0.46056),       // 42: RightIndexProximal
    (0.463165, -0.536035, 0.518565, 0.478775),   // 43: RightIndexIntermediate
    (0.463165, -0.536035, 0.518565, 0.478775),   // 44: RightIndexDistal
    (0.47434, -0.52555, 0.50669, 0.492),         // 45: RightMiddleProximal
    (0.4631, -0.53638, 0.48252, 0.5148),         // 46: RightMiddleIntermediate
    (0.4631, -0.53638, 0.48252, 0.5148),         // 47: RightMiddleDistal
    (0.49482, -0.50517, 0.49731, 0.50264),       // 48: RightRingProximal
    (0.505555, -0.494155, 0.511935, 0.487995),   // 49: RightRingIntermediate
    (0.505555, -0.494155, 0.511935, 0.487995),   // 50: RightRingDistal
    (0.49764, -0.50235, 0.498, 0.50199),         // 51: RightLittleProximal
    (0.52241, -0.47756, 0.49586, 0.50313),       // 52: RightLittleIntermediate
    (0.52241, -0.47756, 0.49586, 0.50313),       // 53: RightLittleDistal
    (-0.56563, 0.42434, -0.56563, -0.42434),     // 54: UpperChest
];

/// POSTQ_INV → postQ (conjugate) を取得
fn get_post_q(unity_idx: usize) -> Quat {
    if unity_idx >= POSTQ_INV_NORMALIZED.len() {
        return Quat::IDENTITY;
    }
    let (x, y, z, w) = POSTQ_INV_NORMALIZED[unity_idx];
    // conjugate: negate xyz
    Quat::from_xyzw(-x, -y, -z, w).normalize()
}

// ─── Unity Humanoid パラメータ (DumpHumanoidParams.cs 出力) ───

/// ボーンごとの preQ/postQ/sign パラメータ
struct BoneParams {
    pre_q: Quat,  // glTF 右手系
    post_q: Quat, // glTF 右手系
    sign: (f32, f32, f32),
    /// ボーンのローカル回転 (glTF 右手系、Hips用)
    local_rotation: Quat,
    /// ボーンのローカル位置 (glTF 右手系、Hips用)
    local_position: Vec3,
}

/// DumpHumanoidParams.cs 出力 JSON の読み込み結果
pub struct HumanoidParams {
    /// Unity ボーンインデックス → BoneParams
    bones: HashMap<usize, BoneParams>,
    /// Muscle デフォルト角度上書き (muscle_idx → (min, max))
    muscle_ranges: HashMap<usize, (f32, f32)>,
}

/// JSON配列から Unity左手系 → glTF右手系 のクォータニオンをパース
fn parse_quat_lh_to_rh(json: &serde_json::Value, key: &str) -> Quat {
    if let Some(arr) = json.get(key).and_then(|v| v.as_array()) {
        let x = arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let y = arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let z = arr.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let w = arr.get(3).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
        Quat::from_xyzw(x, -y, -z, w).normalize()
    } else {
        Quat::IDENTITY
    }
}

/// JSON配列からf32×3タプルをパース（デフォルト値指定可）
fn parse_f32x3(json: &serde_json::Value, key: &str, default: (f32, f32, f32)) -> (f32, f32, f32) {
    if let Some(arr) = json.get(key).and_then(|v| v.as_array()) {
        (
            arr.first()
                .and_then(|v| v.as_f64())
                .unwrap_or(default.0 as f64) as f32,
            arr.get(1)
                .and_then(|v| v.as_f64())
                .unwrap_or(default.1 as f64) as f32,
            arr.get(2)
                .and_then(|v| v.as_f64())
                .unwrap_or(default.2 as f64) as f32,
        )
    } else {
        default
    }
}

/// DumpHumanoidParams.cs が出力した JSON を読み込む
pub fn load_humanoid_params(path: &Path) -> Result<HumanoidParams> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("Humanoidパラメータ読み込み失敗: {}", path.display()))?;
    let json: serde_json::Value =
        serde_json::from_str(&text).context("Humanoidパラメータ JSON パース失敗")?;

    let mut bones = HashMap::new();
    let mut muscle_ranges = HashMap::new();

    // bones 配列
    if let Some(bones_arr) = json.get("bones").and_then(|v| v.as_array()) {
        for bd in bones_arr {
            let idx = match bd.get("index").and_then(|v| v.as_u64()) {
                Some(i) => i as usize,
                None => continue,
            };
            if !bd.get("present").and_then(|v| v.as_bool()).unwrap_or(false) {
                continue;
            }

            // preQ/postQ: Unity 左手系 → glTF 右手系 (x, -y, -z, w)
            let pre_q = parse_quat_lh_to_rh(bd, "preQ");
            let post_q = parse_quat_lh_to_rh(bd, "postQ");
            let sign = parse_f32x3(bd, "sign", (1.0, 1.0, 1.0));

            let local_rotation = parse_quat_lh_to_rh(bd, "localRotation");

            // localPosition (Unity LH → glTF RH)
            let (lx, ly, lz) = parse_f32x3(bd, "localPosition", (0.0, 0.0, 0.0));
            let local_position = Vec3::new(-lx, ly, lz);

            bones.insert(
                idx,
                BoneParams {
                    pre_q,
                    post_q,
                    sign,
                    local_rotation,
                    local_position,
                },
            );
        }
    }

    // muscles 配列 (デフォルト角度範囲の上書き)
    if let Some(muscles_arr) = json.get("muscles").and_then(|v| v.as_array()) {
        for md in muscles_arr {
            let mi = match md.get("index").and_then(|v| v.as_u64()) {
                Some(i) => i as usize,
                None => continue,
            };
            let min = md
                .get("defaultMin")
                .and_then(|v| v.as_f64())
                .unwrap_or(MUSCLE_DEFAULT_MIN.get(mi).copied().unwrap_or(-40.0) as f64)
                as f32;
            let max = md
                .get("defaultMax")
                .and_then(|v| v.as_f64())
                .unwrap_or(MUSCLE_DEFAULT_MAX.get(mi).copied().unwrap_or(40.0) as f64)
                as f32;
            muscle_ranges.insert(mi, (min, max));
        }
    }

    log::info!(
        "Humanoidパラメータ読み込み: {}ボーン, {}muscle",
        bones.len(),
        muscle_ranges.len()
    );
    Ok(HumanoidParams {
        bones,
        muscle_ranges,
    })
}

// ─── Muscle → ボーンチャネル変換 ───

/// Muscle 値を角度（度）に変換
/// muscle=0 → 0°, muscle=+1 → max_deg, muscle=-1 → -min_deg (= max of negative range)
#[inline]
fn muscle_to_deg_with_range(value: f32, min_deg: f32, max_deg: f32) -> f32 {
    if value >= 0.0 {
        value * max_deg
    } else {
        value * (-min_deg)
    }
}

/// Root直値のattribute名パターン
#[derive(Debug)]
enum RootAttr {
    TransX,
    TransY,
    TransZ,
    RotX,
    RotY,
    RotZ,
    RotW,
}

fn parse_root_attr(attr: &str) -> Option<RootAttr> {
    match attr {
        "RootT.x" => Some(RootAttr::TransX),
        "RootT.y" => Some(RootAttr::TransY),
        "RootT.z" => Some(RootAttr::TransZ),
        "RootQ.x" => Some(RootAttr::RotX),
        "RootQ.y" => Some(RootAttr::RotY),
        "RootQ.z" => Some(RootAttr::RotZ),
        "RootQ.w" => Some(RootAttr::RotW),
        _ => None,
    }
}

/// Unity左手系 → glTF右手系 クォータニオン変換
/// reverseX: Y, Z の符号を反転
#[inline]
fn unity_quat_to_gltf(qx: f32, qy: f32, qz: f32, qw: f32) -> Quat {
    Quat::from_xyzw(qx, -qy, -qz, qw).normalize()
}

/// Unity左手系 → glTF右手系 ベクトル変換
#[inline]
fn unity_vec3_to_gltf(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(-x, y, z)
}

/// Muscle カーブからボーンチャネルを構築
///
/// params あり: preQ × SwingTwist(sign × degrees) × Inv(postQ) → 絶対ローカル回転
/// params なし: postQ_norm × SwingTwist(sign × degrees) × Inv(postQ_norm) → 正規化デルタ
///   （ビューアでは rest × result で最終ローカル回転を得る）
fn build_bone_channels(
    curves: &[AnimCurve],
    duration: f32,
    muscle_scale: f32,
    params: Option<&HumanoidParams>,
) -> HashMap<String, BoneChannel> {
    // Muscle名 → (muscle_index, カーブ) のマップ
    let muscle_name_to_idx: HashMap<&str, usize> = MUSCLE_NAMES
        .iter()
        .enumerate()
        .map(|(i, &name)| (name, i))
        .collect();

    // カーブを分類
    let mut muscle_curves: HashMap<usize, &AnimCurve> = HashMap::new(); // muscle_idx → curve
    let mut root_tx: Option<&AnimCurve> = None;
    let mut root_ty: Option<&AnimCurve> = None;
    let mut root_tz: Option<&AnimCurve> = None;
    let mut root_qx: Option<&AnimCurve> = None;
    let mut root_qy: Option<&AnimCurve> = None;
    let mut root_qz: Option<&AnimCurve> = None;
    let mut root_qw: Option<&AnimCurve> = None;

    for curve in curves {
        if let Some(root_attr) = parse_root_attr(&curve.attribute) {
            match root_attr {
                RootAttr::TransX => root_tx = Some(curve),
                RootAttr::TransY => root_ty = Some(curve),
                RootAttr::TransZ => root_tz = Some(curve),
                RootAttr::RotX => root_qx = Some(curve),
                RootAttr::RotY => root_qy = Some(curve),
                RootAttr::RotZ => root_qz = Some(curve),
                RootAttr::RotW => root_qw = Some(curve),
            }
            continue;
        }

        if let Some(&idx) = muscle_name_to_idx.get(curve.attribute.as_str()) {
            muscle_curves.insert(idx, curve);
        }
    }

    let mut channels: HashMap<String, BoneChannel> = HashMap::new();

    // Unity ボーンインデックス → VRM ボーン名
    let unity_bone_to_vrm: [(usize, &str); 54] = [
        (1, "leftUpperLeg"),
        (2, "rightUpperLeg"),
        (3, "leftLowerLeg"),
        (4, "rightLowerLeg"),
        (5, "leftFoot"),
        (6, "rightFoot"),
        (7, "spine"),
        (8, "chest"),
        (9, "neck"),
        (10, "head"),
        (11, "leftShoulder"),
        (12, "rightShoulder"),
        (13, "leftUpperArm"),
        (14, "rightUpperArm"),
        (15, "leftLowerArm"),
        (16, "rightLowerArm"),
        (17, "leftHand"),
        (18, "rightHand"),
        (19, "leftToes"),
        (20, "rightToes"),
        (21, "leftEye"),
        (22, "rightEye"),
        (23, "jaw"),
        (24, "leftThumbMetacarpal"),
        (25, "leftThumbProximal"),
        (26, "leftThumbDistal"),
        (27, "leftIndexProximal"),
        (28, "leftIndexIntermediate"),
        (29, "leftIndexDistal"),
        (30, "leftMiddleProximal"),
        (31, "leftMiddleIntermediate"),
        (32, "leftMiddleDistal"),
        (33, "leftRingProximal"),
        (34, "leftRingIntermediate"),
        (35, "leftRingDistal"),
        (36, "leftLittleProximal"),
        (37, "leftLittleIntermediate"),
        (38, "leftLittleDistal"),
        (39, "rightThumbMetacarpal"),
        (40, "rightThumbProximal"),
        (41, "rightThumbDistal"),
        (42, "rightIndexProximal"),
        (43, "rightIndexIntermediate"),
        (44, "rightIndexDistal"),
        (45, "rightMiddleProximal"),
        (46, "rightMiddleIntermediate"),
        (47, "rightMiddleDistal"),
        (48, "rightRingProximal"),
        (49, "rightRingIntermediate"),
        (50, "rightRingDistal"),
        (51, "rightLittleProximal"),
        (52, "rightLittleIntermediate"),
        (53, "rightLittleDistal"),
        (54, "upperChest"),
    ];

    // 各ボーンのチャネルを構築
    for &(unity_idx, vrm_bone) in &unity_bone_to_vrm {
        let mfb = MUSCLE_FROM_BONE[unity_idx];

        // パラメータがあればmodel-specific値を使用、なければフォールバック
        let (pre_q, post_q, sign) = if let Some(p) = params {
            if let Some(bp) = p.bones.get(&unity_idx) {
                (bp.pre_q, bp.post_q, bp.sign)
            } else {
                // params にこのボーンがない場合はフォールバック
                let pq = get_post_q(unity_idx);
                (pq, pq, SIGNS[unity_idx])
            }
        } else {
            // V-Sekai フォールバック: preQ == postQ
            let pq = get_post_q(unity_idx);
            (pq, pq, SIGNS[unity_idx])
        };
        let post_q_inv = post_q.inverse();

        // このボーンに関連する muscle カーブがあるか
        let has_curve = |mi: i8| -> bool { mi >= 0 && muscle_curves.contains_key(&(mi as usize)) };
        if !has_curve(mfb.0) && !has_curve(mfb.1) && !has_curve(mfb.2) {
            continue;
        }

        // 全DOFのキーフレーム時刻を統合
        let mut all_times: Vec<f32> = Vec::new();
        for &mi in &[mfb.0, mfb.1, mfb.2] {
            if mi >= 0 {
                if let Some(curve) = muscle_curves.get(&(mi as usize)) {
                    all_times.extend(curve.keyframes.iter().map(|(t, _)| *t));
                }
            }
        }
        all_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        all_times.dedup_by(|a, b| (*a - *b).abs() < 1e-6);

        if all_times.is_empty() {
            continue;
        }

        let keyframes: Vec<RotationKeyframe> = all_times
            .iter()
            .map(|&t| {
                let mut degrees = [0.0f32; 3]; // [twist_x, swing_y, swing_z]

                for (dof_idx, &muscle_idx) in [mfb.0, mfb.1, mfb.2].iter().enumerate() {
                    if muscle_idx < 0 {
                        continue;
                    }
                    let mi = muscle_idx as usize;
                    let mv = if let Some(curve) = muscle_curves.get(&mi) {
                        sample_curve_linear(&curve.keyframes, t)
                    } else {
                        0.0
                    };
                    // Muscle角度範囲: params の上書きがあればそちらを使用
                    let (min_deg, max_deg) = if let Some(p) = params {
                        p.muscle_ranges
                            .get(&mi)
                            .copied()
                            .unwrap_or((MUSCLE_DEFAULT_MIN[mi], MUSCLE_DEFAULT_MAX[mi]))
                    } else {
                        (MUSCLE_DEFAULT_MIN[mi], MUSCLE_DEFAULT_MAX[mi])
                    };
                    let deg = muscle_to_deg_with_range(mv, min_deg, max_deg) * muscle_scale;
                    let s = match dof_idx {
                        0 => sign.0,
                        1 => sign.1,
                        _ => sign.2,
                    };
                    degrees[dof_idx] = s * deg;
                }

                // SwingTwist 変換
                let st = swing_twist(degrees[0], degrees[1], degrees[2]);

                // preQ × SwingTwist × Inv(postQ)
                // params あり: 絶対ローカル回転
                // params なし: postQ × SwingTwist × Inv(postQ)（正規化デルタ、muscle=0でIdentity）
                let anim_rot = (pre_q * st * post_q_inv).normalize();

                RotationKeyframe {
                    time: t,
                    value: anim_rot,
                }
            })
            .collect();

        channels.insert(
            vrm_bone.to_string(),
            BoneChannel {
                rotation: keyframes,
                rotation_interp: Interpolation::Linear,
                translation: None,
                translation_interp: None,
            },
        );
    }

    // Hips の rest 情報（params から取得可能な場合）
    let hips_rest_rot = params
        .and_then(|p| p.bones.get(&0))
        .map(|bp| bp.local_rotation);
    let hips_rest_pos = params
        .and_then(|p| p.bones.get(&0))
        .map(|bp| bp.local_position);

    // Root → hips: 回転（RootQ）
    if root_qx.is_some() || root_qy.is_some() || root_qz.is_some() || root_qw.is_some() {
        let mut all_times: Vec<f32> = Vec::new();
        for c in [&root_qx, &root_qy, &root_qz, &root_qw]
            .into_iter()
            .flatten()
        {
            all_times.extend(c.keyframes.iter().map(|(t, _)| *t));
        }
        all_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        all_times.dedup_by(|a, b| (*a - *b).abs() < 1e-6);

        if !all_times.is_empty() {
            // 初期フレームの回転（glTF座標系）
            let q0_x = root_qx
                .map(|c| sample_curve_linear(&c.keyframes, 0.0))
                .unwrap_or(0.0);
            let q0_y = root_qy
                .map(|c| sample_curve_linear(&c.keyframes, 0.0))
                .unwrap_or(0.0);
            let q0_z = root_qz
                .map(|c| sample_curve_linear(&c.keyframes, 0.0))
                .unwrap_or(0.0);
            let q0_w = root_qw
                .map(|c| sample_curve_linear(&c.keyframes, 0.0))
                .unwrap_or(1.0);
            let q0_gltf = unity_quat_to_gltf(q0_x, q0_y, q0_z, q0_w);
            let q0_inv = q0_gltf.inverse();

            let mut prev_q = q0_gltf;

            let rot_keyframes: Vec<RotationKeyframe> = all_times
                .iter()
                .map(|&t| {
                    let qx = root_qx
                        .map(|c| sample_curve_linear(&c.keyframes, t))
                        .unwrap_or(0.0);
                    let qy = root_qy
                        .map(|c| sample_curve_linear(&c.keyframes, t))
                        .unwrap_or(0.0);
                    let qz = root_qz
                        .map(|c| sample_curve_linear(&c.keyframes, t))
                        .unwrap_or(0.0);
                    let qw = root_qw
                        .map(|c| sample_curve_linear(&c.keyframes, t))
                        .unwrap_or(1.0);

                    let mut qi = unity_quat_to_gltf(qx, qy, qz, qw);

                    // 符号一貫性: 前フレームとの内積が負なら反転
                    if prev_q.dot(qi) < 0.0 {
                        qi = -qi;
                    }
                    prev_q = qi;

                    // デルタ: Inv(q0) × qi — rest からの回転差分
                    let delta = (q0_inv * qi).normalize();

                    if let Some(rest) = hips_rest_rot {
                        // params あり: 絶対ローカル回転 = rest × delta
                        RotationKeyframe {
                            time: t,
                            value: (rest * delta).normalize(),
                        }
                    } else {
                        // フォールバック: デルタのまま（ビューアで rest × delta）
                        RotationKeyframe {
                            time: t,
                            value: delta,
                        }
                    }
                })
                .collect();

            let entry = channels
                .entry("hips".to_string())
                .or_insert_with(|| BoneChannel {
                    rotation: Vec::new(),
                    rotation_interp: Interpolation::Linear,
                    translation: None,
                    translation_interp: None,
                });
            entry.rotation = rot_keyframes;
        }
    }

    // Root → hips: 平行移動（RootT）
    if root_tx.is_some() || root_ty.is_some() || root_tz.is_some() {
        let mut all_times: Vec<f32> = Vec::new();
        for c in [&root_tx, &root_ty, &root_tz].into_iter().flatten() {
            all_times.extend(c.keyframes.iter().map(|(t, _)| *t));
        }
        all_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        all_times.dedup_by(|a, b| (*a - *b).abs() < 1e-6);

        if !all_times.is_empty() {
            // 初期フレームの値をデルタ基準にする
            let t0_tx = root_tx
                .map(|c| sample_curve_linear(&c.keyframes, 0.0))
                .unwrap_or(0.0);
            let t0_ty = root_ty
                .map(|c| sample_curve_linear(&c.keyframes, 0.0))
                .unwrap_or(0.0);
            let t0_tz = root_tz
                .map(|c| sample_curve_linear(&c.keyframes, 0.0))
                .unwrap_or(0.0);

            let trans_keyframes: Vec<TranslationKeyframe> = all_times
                .iter()
                .map(|&t| {
                    let tx = root_tx
                        .map(|c| sample_curve_linear(&c.keyframes, t))
                        .unwrap_or(0.0)
                        - t0_tx;
                    let ty = root_ty
                        .map(|c| sample_curve_linear(&c.keyframes, t))
                        .unwrap_or(0.0)
                        - t0_ty;
                    let tz = root_tz
                        .map(|c| sample_curve_linear(&c.keyframes, t))
                        .unwrap_or(0.0)
                        - t0_tz;
                    // Unity左手系 → glTF右手系 のデルタ
                    let delta = unity_vec3_to_gltf(tx, ty, tz);

                    if let Some(rest_pos) = hips_rest_pos {
                        // params あり: 絶対位置 = rest + delta
                        TranslationKeyframe {
                            time: t,
                            value: rest_pos + delta,
                        }
                    } else {
                        // フォールバック: デルタのまま
                        TranslationKeyframe {
                            time: t,
                            value: delta,
                        }
                    }
                })
                .collect();

            let entry = channels
                .entry("hips".to_string())
                .or_insert_with(|| BoneChannel {
                    rotation: Vec::new(),
                    rotation_interp: Interpolation::Linear,
                    translation: None,
                    translation_interp: None,
                });
            entry.translation = Some(trans_keyframes);
            entry.translation_interp = Some(Interpolation::Linear);
        }
    }

    let _ = duration;

    channels
}

/// キーフレーム配列から指定時刻の値を線形補間
fn sample_curve_linear(keyframes: &[(f32, f32)], time: f32) -> f32 {
    if keyframes.is_empty() {
        return 0.0;
    }
    if keyframes.len() == 1 || time <= keyframes[0].0 {
        return keyframes[0].1;
    }
    if time >= keyframes.last().expect("keyframes は空でない").0 {
        return keyframes.last().expect("keyframes は空でない").1;
    }
    let idx = keyframes.partition_point(|&(t, _)| t <= time);
    let idx = idx.min(keyframes.len() - 1).max(1);
    let (t0, v0) = keyframes[idx - 1];
    let (t1, v1) = keyframes[idx];
    let frac = if (t1 - t0).abs() < 1e-9 {
        0.0
    } else {
        (time - t0) / (t1 - t0)
    };
    v0 + (v1 - v0) * frac
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_kizuna_anim() {
        let path = std::path::Path::new(
            r"E:\misc\nomy\vrm_view\tmp\unitypackage\KizunaAI_KAMATTE_VRM&Motion\Assets\KizunaAI\KizunaAI_KAMATTE\Motion\KizunaAI_KAMATTE_Kamacho_Motion.anim",
        );
        if !path.exists() {
            eprintln!("テストファイルが見つからない: {}", path.display());
            return;
        }
        let _ = env_logger::try_init();
        let anim = load_unity_anim(path, 1.0).expect("Unity .anim読み込み失敗");
        eprintln!("アニメ名: {}", anim.name);
        eprintln!("duration: {:.2}秒", anim.duration);
        eprintln!("ボーンch数: {}", anim.bone_channels.len());
        for (name, ch) in &anim.bone_channels {
            eprintln!(
                "  {} : rot={}kf, trans={}kf",
                name,
                ch.rotation.len(),
                ch.translation.as_ref().map(|t| t.len()).unwrap_or(0),
            );
        }
        assert!(
            anim.bone_channels.len() > 5,
            "ボーンチャネルが少なすぎる: {}",
            anim.bone_channels.len()
        );
    }

    #[test]
    fn test_swing_twist_identity() {
        let q = swing_twist(0.0, 0.0, 0.0);
        assert!((q.x.abs() + q.y.abs() + q.z.abs()) < 1e-6);
        assert!((q.w - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_swing_twist_pure_twist() {
        let q = swing_twist(90.0, 0.0, 0.0);
        let expected = Quat::from_axis_angle(Vec3::X, 90.0f32.to_radians());
        let diff = q * expected.inverse();
        assert!(
            (diff.w.abs() - 1.0).abs() < 1e-4,
            "pure twist mismatch: {:?}",
            diff
        );
    }

    #[test]
    fn test_postq_identity_at_rest() {
        // muscle=0 のとき postQ × Id × Inv(postQ) = Identity
        for unity_idx in 1..55 {
            let post_q = get_post_q(unity_idx);
            let result = post_q * Quat::IDENTITY * post_q.inverse();
            let angle = result.to_axis_angle().1.abs();
            assert!(
                angle < 1e-4,
                "bone {} rest not identity: angle={}",
                unity_idx,
                angle
            );
        }
    }
}
