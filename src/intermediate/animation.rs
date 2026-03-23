use glam::{Quat, Vec3};
use std::collections::HashMap;

/// glTF アニメーション補間方法
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    Linear,
    Step,
    CubicSpline,
}

/// 回転キーフレーム
#[derive(Debug, Clone)]
pub struct RotationKeyframe {
    pub time: f32,
    pub value: Quat,
}

/// 平行移動キーフレーム
#[derive(Debug, Clone)]
pub struct TranslationKeyframe {
    pub time: f32,
    pub value: Vec3,
}

/// スカラーキーフレーム（表情ウェイト用）
#[derive(Debug, Clone)]
pub struct ScalarKeyframe {
    pub time: f32,
    pub value: f32,
}

/// ボーンのアニメーションチャネル
#[derive(Debug, Clone)]
pub struct BoneChannel {
    pub rotation: Vec<RotationKeyframe>,
    pub rotation_interp: Interpolation,
    /// Hips のみ平行移動あり
    pub translation: Option<Vec<TranslationKeyframe>>,
    pub translation_interp: Option<Interpolation>,
}

/// 表情のアニメーションチャネル
#[derive(Debug, Clone)]
pub struct ExpressionChannel {
    pub keyframes: Vec<ScalarKeyframe>,
    pub interp: Interpolation,
}

/// VRMAボーンのレストデータ（リターゲティング用）
#[derive(Debug, Clone)]
pub struct VrmaBoneRest {
    /// ローカルレスト回転（T-Pose時のノードローカル回転）
    pub local_rotation: Quat,
    /// ワールドレスト回転（T-Pose時のグローバル回転）
    pub global_rotation: Quat,
    /// ローカルレスト平行移動（T-Pose時のノードローカル位置）
    pub local_translation: Vec3,
}

/// ボーンマッチングモード
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoneMatchMode {
    /// VRMA: ヒューマノイドボーン名でマッチ（リターゲティングあり）
    Humanoid,
    /// GLB/glTF/FBX: ノード名で直接マッチ（リターゲティングなし）
    NodeName,
}

/// アニメーションデータ（VRMA / GLB / glTF / FBX 共通）
#[derive(Debug, Clone)]
pub struct VrmaAnimation {
    /// アニメーション名
    pub name: String,
    /// 総再生時間（秒）
    pub duration: f32,
    /// ボーン名 → ボーンチャネル（キーはマッチモードに依存）
    pub bone_channels: HashMap<String, BoneChannel>,
    /// 表情名 → 表情チャネル
    pub expression_channels: HashMap<String, ExpressionChannel>,
    /// VRMAボーンのレスト回転（リターゲティング用、Humanoidモードのみ使用）
    pub bone_rests: HashMap<String, VrmaBoneRest>,
    /// ボーンマッチングモード
    pub match_mode: BoneMatchMode,
    /// ソースモデルがY軸180°反転しているか（+Z向き vs VRMの-Z向き）
    pub facing_flip_y: bool,
    /// 回転がレストポーズからのデルタか（Unity Muscle等）
    pub is_additive: bool,
    /// ボーンローカル空間でのデルタか（true: parent共役変換省略）
    /// Unity Muscle の場合 true（回転は既にボーンローカル空間）
    pub is_bone_local_delta: bool,
}

impl VrmaAnimation {
    /// 指定時刻のボーン回転を補間して取得
    pub fn sample_bone_rotation(&self, bone_name: &str, time: f32) -> Option<Quat> {
        let ch = self.bone_channels.get(bone_name)?;
        Some(sample_rotation(&ch.rotation, ch.rotation_interp, time))
    }

    /// 指定時刻のボーン平行移動を補間して取得（Hipsのみ）
    pub fn sample_bone_translation(&self, bone_name: &str, time: f32) -> Option<Vec3> {
        let ch = self.bone_channels.get(bone_name)?;
        let kfs = ch.translation.as_ref()?;
        let interp = ch.translation_interp.unwrap_or(Interpolation::Linear);
        Some(sample_translation(kfs, interp, time))
    }

    /// 指定時刻の表情ウェイトを補間して取得
    pub fn sample_expression(&self, expr_name: &str, time: f32) -> Option<f32> {
        let ch = self.expression_channels.get(expr_name)?;
        Some(sample_scalar(&ch.keyframes, ch.interp, time).clamp(0.0, 1.0))
    }
}

/// 回転キーフレーム補間
fn sample_rotation(keyframes: &[RotationKeyframe], interp: Interpolation, time: f32) -> Quat {
    if keyframes.is_empty() {
        return Quat::IDENTITY;
    }
    if keyframes.len() == 1 || time <= keyframes[0].time {
        return keyframes[0].value;
    }
    let last = keyframes.last().expect("keyframes は空でない");
    if time >= last.time {
        return last.value;
    }

    // 二分探索で区間を見つける
    let idx = keyframes.partition_point(|kf| kf.time <= time);
    let idx = idx.min(keyframes.len() - 1).max(1);
    let a = &keyframes[idx - 1];
    let b = &keyframes[idx];

    match interp {
        Interpolation::Step => a.value,
        Interpolation::Linear | Interpolation::CubicSpline => {
            let t = if (b.time - a.time).abs() < 1e-9 {
                0.0
            } else {
                (time - a.time) / (b.time - a.time)
            };
            a.value.slerp(b.value, t)
        }
    }
}

/// 平行移動キーフレーム補間
fn sample_translation(keyframes: &[TranslationKeyframe], interp: Interpolation, time: f32) -> Vec3 {
    if keyframes.is_empty() {
        return Vec3::ZERO;
    }
    if keyframes.len() == 1 || time <= keyframes[0].time {
        return keyframes[0].value;
    }
    let last = keyframes.last().expect("keyframes は空でない");
    if time >= last.time {
        return last.value;
    }

    let idx = keyframes.partition_point(|kf| kf.time <= time);
    let idx = idx.min(keyframes.len() - 1).max(1);
    let a = &keyframes[idx - 1];
    let b = &keyframes[idx];

    match interp {
        Interpolation::Step => a.value,
        Interpolation::Linear | Interpolation::CubicSpline => {
            let t = if (b.time - a.time).abs() < 1e-9 {
                0.0
            } else {
                (time - a.time) / (b.time - a.time)
            };
            a.value.lerp(b.value, t)
        }
    }
}

/// スカラーキーフレーム補間
fn sample_scalar(keyframes: &[ScalarKeyframe], interp: Interpolation, time: f32) -> f32 {
    if keyframes.is_empty() {
        return 0.0;
    }
    if keyframes.len() == 1 || time <= keyframes[0].time {
        return keyframes[0].value;
    }
    let last = keyframes.last().expect("keyframes は空でない");
    if time >= last.time {
        return last.value;
    }

    let idx = keyframes.partition_point(|kf| kf.time <= time);
    let idx = idx.min(keyframes.len() - 1).max(1);
    let a = &keyframes[idx - 1];
    let b = &keyframes[idx];

    match interp {
        Interpolation::Step => a.value,
        Interpolation::Linear | Interpolation::CubicSpline => {
            let t = if (b.time - a.time).abs() < 1e-9 {
                0.0
            } else {
                (time - a.time) / (b.time - a.time)
            };
            a.value + (b.value - a.value) * t
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_scalar_keyframes(pairs: Vec<(f32, f32)>) -> Vec<ScalarKeyframe> {
        pairs
            .into_iter()
            .map(|(time, value)| ScalarKeyframe { time, value })
            .collect()
    }

    #[test]
    fn test_scalar_interpolation_at_keyframe() {
        let kfs = make_scalar_keyframes(vec![(0.0, 0.0), (1.0, 1.0), (2.0, 0.5)]);
        assert!((sample_scalar(&kfs, Interpolation::Linear, 0.0) - 0.0).abs() < 1e-6);
        assert!((sample_scalar(&kfs, Interpolation::Linear, 1.0) - 1.0).abs() < 1e-6);
        assert!((sample_scalar(&kfs, Interpolation::Linear, 2.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_scalar_interpolation_midpoint() {
        let kfs = make_scalar_keyframes(vec![(0.0, 0.0), (1.0, 1.0)]);
        assert!((sample_scalar(&kfs, Interpolation::Linear, 0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_scalar_clamp_before_first() {
        let kfs = make_scalar_keyframes(vec![(1.0, 5.0), (2.0, 10.0)]);
        assert!(
            (sample_scalar(&kfs, Interpolation::Linear, 0.0) - 5.0).abs() < 1e-6,
            "最初のキーフレームより前はクランプされるべき"
        );
    }

    #[test]
    fn test_scalar_clamp_after_last() {
        let kfs = make_scalar_keyframes(vec![(0.0, 0.0), (1.0, 5.0)]);
        assert!(
            (sample_scalar(&kfs, Interpolation::Linear, 99.0) - 5.0).abs() < 1e-6,
            "最後のキーフレームより後はクランプされるべき"
        );
    }

    #[test]
    fn test_scalar_step_interpolation() {
        let kfs = make_scalar_keyframes(vec![(0.0, 0.0), (1.0, 10.0)]);
        // Step補間では前のキーフレームの値を保持
        assert!((sample_scalar(&kfs, Interpolation::Step, 0.5) - 0.0).abs() < 1e-6);
        assert!((sample_scalar(&kfs, Interpolation::Step, 0.99) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_scalar_empty_keyframes() {
        let kfs: Vec<ScalarKeyframe> = vec![];
        assert!((sample_scalar(&kfs, Interpolation::Linear, 0.0) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_scalar_single_keyframe() {
        let kfs = make_scalar_keyframes(vec![(1.0, 42.0)]);
        assert!((sample_scalar(&kfs, Interpolation::Linear, 0.0) - 42.0).abs() < 1e-6);
        assert!((sample_scalar(&kfs, Interpolation::Linear, 5.0) - 42.0).abs() < 1e-6);
    }

    #[test]
    fn test_rotation_identity_when_empty() {
        let kfs: Vec<RotationKeyframe> = vec![];
        let q = sample_rotation(&kfs, Interpolation::Linear, 0.0);
        assert!((q.x.abs() + q.y.abs() + q.z.abs()) < 1e-6);
        assert!((q.w - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_translation_zero_when_empty() {
        let kfs: Vec<TranslationKeyframe> = vec![];
        let v = sample_translation(&kfs, Interpolation::Linear, 0.0);
        assert!(v.length() < 1e-6);
    }
}
