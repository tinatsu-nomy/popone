use glam::{Quat, Vec3};
use std::collections::HashMap;

/// glTF animation interpolation method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    Linear,
    Step,
    CubicSpline,
}

/// Rotation keyframe.
#[derive(Debug, Clone)]
pub struct RotationKeyframe {
    pub time: f32,
    pub value: Quat,
}

/// Translation keyframe.
#[derive(Debug, Clone)]
pub struct TranslationKeyframe {
    pub time: f32,
    pub value: Vec3,
}

/// Scalar keyframe (used for expression weights).
#[derive(Debug, Clone)]
pub struct ScalarKeyframe {
    pub time: f32,
    pub value: f32,
}

/// Animation channel for a bone.
#[derive(Debug, Clone)]
pub struct BoneChannel {
    pub rotation: Vec<RotationKeyframe>,
    pub rotation_interp: Interpolation,
    /// Translation is only present for Hips.
    pub translation: Option<Vec<TranslationKeyframe>>,
    pub translation_interp: Option<Interpolation>,
}

/// Animation channel for an expression.
#[derive(Debug, Clone)]
pub struct ExpressionChannel {
    pub keyframes: Vec<ScalarKeyframe>,
    pub interp: Interpolation,
}

/// Rest data for a VRMA bone (used for retargeting).
#[derive(Debug, Clone)]
pub struct VrmaBoneRest {
    /// Local rest rotation (node-local rotation at T-pose).
    pub local_rotation: Quat,
    /// World rest rotation (global rotation at T-pose).
    pub global_rotation: Quat,
    /// Local rest translation (node-local position at T-pose).
    pub local_translation: Vec3,
}

/// Bone matching mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoneMatchMode {
    /// VRMA: match by humanoid bone name (with retargeting).
    Humanoid,
    /// GLB/glTF/FBX: match by node name directly (no retargeting).
    NodeName,
}

/// Animation data shared by VRMA / GLB / glTF / FBX.
#[derive(Debug, Clone)]
pub struct VrmaAnimation {
    /// Animation name.
    pub name: String,
    /// Total duration in seconds.
    pub duration: f32,
    /// Bone name -> bone channel (key meaning depends on `match_mode`).
    pub bone_channels: HashMap<String, BoneChannel>,
    /// Expression name -> expression channel.
    pub expression_channels: HashMap<String, ExpressionChannel>,
    /// Rest rotation for VRMA bones (used for retargeting; Humanoid mode only).
    pub bone_rests: HashMap<String, VrmaBoneRest>,
    /// Bone matching mode.
    pub match_mode: BoneMatchMode,
    /// Whether the source model is rotated 180 degrees about Y (+Z forward vs. VRM's -Z forward).
    pub facing_flip_y: bool,
    /// Whether rotations are deltas relative to the rest pose (e.g. Unity Muscle).
    pub is_additive: bool,
    /// Whether deltas are expressed in bone-local space (true: skip parent-conjugate transform).
    /// True for Unity Muscle (rotations are already in bone-local space).
    pub is_bone_local_delta: bool,
}

impl VrmaAnimation {
    /// Interpolate the bone rotation at the given time.
    pub fn sample_bone_rotation(&self, bone_name: &str, time: f32) -> Option<Quat> {
        let ch = self.bone_channels.get(bone_name)?;
        Some(sample_rotation(&ch.rotation, ch.rotation_interp, time))
    }

    /// Interpolate the bone translation at the given time (Hips only).
    pub fn sample_bone_translation(&self, bone_name: &str, time: f32) -> Option<Vec3> {
        let ch = self.bone_channels.get(bone_name)?;
        let kfs = ch.translation.as_ref()?;
        let interp = ch.translation_interp.unwrap_or(Interpolation::Linear);
        Some(sample_translation(kfs, interp, time))
    }

    /// Interpolate the expression weight at the given time.
    pub fn sample_expression(&self, expr_name: &str, time: f32) -> Option<f32> {
        let ch = self.expression_channels.get(expr_name)?;
        Some(sample_scalar(&ch.keyframes, ch.interp, time).clamp(0.0, 1.0))
    }
}

/// Rotation keyframe interpolation.
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

    // Binary search to locate the segment
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

/// Translation keyframe interpolation.
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

/// Scalar keyframe interpolation.
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
        // Step interpolation holds the previous keyframe's value
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
