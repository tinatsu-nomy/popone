use glam::{Quat, Vec3};

/// glTF (meters) -> PMX (MMD scale) conversion factor.
/// 1 m = 12.5 PMX units (Hatsune Miku Ver2 reference: ~160 cm = ~20 units).
pub const PMX_SCALE: f32 = 12.5;

/// VRM 1.0 coord conversion (glTF right-handed -> PMX).
/// VRM 1.0 faces -Z in glTF space with right hand on +X.
/// Flipping Z alone aligns the model with PMX's +Z facing while keeping left/right correct (X is not flipped).
/// Determinant = -1, so face winding gets reversed and `flip_face_winding` is required.
#[inline]
pub fn gltf_pos_to_pmx(v: Vec3) -> Vec3 {
    Vec3::new(v.x * PMX_SCALE, v.y * PMX_SCALE, -v.z * PMX_SCALE)
}

/// VRM 0.0 coord conversion.
/// VRM 0.0 root nodes carry a 180-degree Y rotation, so in world coords the model faces +Z with right hand on +X.
/// Z does not need to flip (+Z stays +Z); flip X to keep left/right correct.
/// Determinant = -1, so face winding gets reversed and `flip_face_winding` is required.
#[inline]
pub fn gltf_pos_to_pmx_v0(v: Vec3) -> Vec3 {
    Vec3::new(-v.x * PMX_SCALE, v.y * PMX_SCALE, v.z * PMX_SCALE)
}

/// VRM 1.0 normal conversion (flip Z only).
#[inline]
pub fn gltf_normal_to_pmx(n: Vec3) -> Vec3 {
    Vec3::new(n.x, n.y, -n.z)
}

/// VRM 0.0 normal conversion (flip X only).
#[inline]
pub fn gltf_normal_to_pmx_v0(n: Vec3) -> Vec3 {
    Vec3::new(-n.x, n.y, n.z)
}

/// Quaternion conversion matching the (-x, y, -z) transform (equivalent to a 180-degree Y rotation).
#[inline]
pub fn gltf_quat_to_pmx(q: Quat) -> Quat {
    Quat::from_xyzw(-q.x, q.y, -q.z, q.w)
}

/// Reverse face winding (front faces flip when X is mirrored).
/// [a, b, c] -> [a, c, b].
pub fn flip_face_winding(indices: &mut [u32]) {
    let n = indices.len();
    let mut i = 0;
    while i + 2 < n {
        indices.swap(i + 1, i + 2);
        i += 3;
    }
}

/// Return a function pointer for the glTF -> PMX coord transform, dispatched on the `is_vrm0` flag.
#[inline]
pub fn pos_fn(is_vrm0: bool) -> fn(Vec3) -> Vec3 {
    if is_vrm0 {
        gltf_pos_to_pmx_v0
    } else {
        gltf_pos_to_pmx
    }
}

/// PMX position -> glTF position (inverse transform: descale + mirror).
/// Handles VRM 0.0 and 1.0 uniformly via the `is_vrm0` flag.
#[inline]
pub fn pmx_pos_to_gltf(v: Vec3, is_vrm0: bool) -> Vec3 {
    if is_vrm0 {
        Vec3::new(-v.x / PMX_SCALE, v.y / PMX_SCALE, v.z / PMX_SCALE)
    } else {
        Vec3::new(v.x / PMX_SCALE, v.y / PMX_SCALE, -v.z / PMX_SCALE)
    }
}

/// glTF position -> PMX position (unified for VRM 0.0 / 1.0).
#[inline]
pub fn gltf_pos_to_pmx_unified(v: Vec3, is_vrm0: bool) -> Vec3 {
    if is_vrm0 {
        gltf_pos_to_pmx_v0(v)
    } else {
        gltf_pos_to_pmx(v)
    }
}

/// PMX normal -> glTF normal (mirror only, no scale).
/// Mirror is self-inverse, so this is identical to `gltf_normal_to_pmx_unified`.
#[inline]
pub fn pmx_normal_to_gltf(n: Vec3, is_vrm0: bool) -> Vec3 {
    if is_vrm0 {
        Vec3::new(-n.x, n.y, n.z)
    } else {
        Vec3::new(n.x, n.y, -n.z)
    }
}

/// glTF normal -> PMX normal (unified for VRM 0.0 / 1.0).
/// Mirror is self-inverse, so this is identical to `pmx_normal_to_gltf`.
#[inline]
pub fn gltf_normal_to_pmx_unified(n: Vec3, is_vrm0: bool) -> Vec3 {
    pmx_normal_to_gltf(n, is_vrm0)
}

/// glTF array [f32; 3] -> PMX Vec3.
pub fn arr3_to_pmx(arr: [f32; 3]) -> Vec3 {
    gltf_pos_to_pmx(Vec3::new(arr[0], arr[1], arr[2]))
}

/// Unit tests.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coord_x_flip() {
        // VRM 1.0: (x, y, -z) * scale -> PMX
        let v = Vec3::new(1.0, 2.0, 3.0);
        let pmx = gltf_pos_to_pmx(v);
        assert!((pmx.x - 12.5).abs() < 1e-3); // X is not flipped
        assert!((pmx.y - 25.0).abs() < 1e-3);
        assert!((pmx.z - (-37.5)).abs() < 1e-3); // Only Z is flipped
    }

    #[test]
    fn test_face_winding() {
        let mut idx = vec![0, 1, 2, 3, 4, 5];
        flip_face_winding(&mut idx);
        assert_eq!(idx, vec![0, 2, 1, 3, 5, 4]);
    }

    #[test]
    fn test_coord_v0_x_flip() {
        let v = Vec3::new(1.0, 2.0, 3.0);
        let pmx = gltf_pos_to_pmx_v0(v);
        assert!((pmx.x - (-12.5)).abs() < 1e-3); // X is flipped
        assert!((pmx.y - 25.0).abs() < 1e-3);
        assert!((pmx.z - 37.5).abs() < 1e-3); // Z is not flipped
    }

    #[test]
    fn test_pos_roundtrip_v1() {
        // glTF -> PMX -> glTF must round-trip within 1e-4
        let original = Vec3::new(1.5, -0.3, 2.7);
        let pmx = gltf_pos_to_pmx(original);
        let back = pmx_pos_to_gltf(pmx, false);
        assert!(
            (original - back).length() < 1e-4,
            "V1 roundtrip failed: {original} → {pmx} → {back}"
        );
    }

    #[test]
    fn test_pos_roundtrip_v0() {
        let original = Vec3::new(1.5, -0.3, 2.7);
        let pmx = gltf_pos_to_pmx_v0(original);
        let back = pmx_pos_to_gltf(pmx, true);
        assert!(
            (original - back).length() < 1e-4,
            "V0 roundtrip failed: {original} → {pmx} → {back}"
        );
    }

    #[test]
    fn test_normal_roundtrip_v1() {
        let n = Vec3::new(0.577, 0.577, 0.577);
        let pmx_n = gltf_normal_to_pmx(n);
        let back = pmx_normal_to_gltf(pmx_n, false);
        assert!((n - back).length() < 1e-4, "V1 normal roundtrip failed");
    }

    #[test]
    fn test_normal_roundtrip_v0() {
        let n = Vec3::new(0.577, 0.577, 0.577);
        let pmx_n = gltf_normal_to_pmx_v0(n);
        let back = pmx_normal_to_gltf(pmx_n, true);
        assert!((n - back).length() < 1e-4, "V0 normal roundtrip failed");
    }

    #[test]
    fn test_normal_is_self_inverse() {
        // Mirror is self-inverse: f(f(x)) == x
        let n = Vec3::new(0.3, 0.9, -0.2);
        let once = gltf_normal_to_pmx(n);
        let _twice = gltf_normal_to_pmx(once);
        // V1: (x, y, -z) applied twice -> (x, y, z) (mismatch: -z flips back after two applies)
        // Self-inverse: pmx_normal_to_gltf == gltf_normal_to_pmx_unified (for the same version)
        let pmx_n = gltf_normal_to_pmx(n);
        let back = pmx_normal_to_gltf(pmx_n, false);
        assert!((n - back).length() < 1e-6);
    }

    #[test]
    fn test_unified_pos_matches_versioned() {
        let v = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(gltf_pos_to_pmx_unified(v, false), gltf_pos_to_pmx(v));
        assert_eq!(gltf_pos_to_pmx_unified(v, true), gltf_pos_to_pmx_v0(v));
    }

    #[test]
    fn test_unified_normal_matches_versioned() {
        let n = Vec3::new(0.5, 0.5, 0.707);
        assert_eq!(gltf_normal_to_pmx_unified(n, false), gltf_normal_to_pmx(n));
        assert_eq!(
            gltf_normal_to_pmx_unified(n, true),
            gltf_normal_to_pmx_v0(n)
        );
    }
}
