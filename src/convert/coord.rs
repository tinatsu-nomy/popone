use glam::{Quat, Vec3};

/// glTF（メートル）→ PMX（MMDスケール）変換係数
/// 1m = 12.5 PMX単位（初音ミクVer2基準: 身長≈160cm≈20単位）
pub const PMX_SCALE: f32 = 12.5;

/// VRM 1.0 用座標変換（glTF 右手系 → PMX）
/// VRM 1.0 は glTF 空間で -Z 向き・右手が +X。
/// Z 反転のみで PMX +Z 向きに揃え、X は反転しないことで左右を正す。
/// 行列式 = -1 → 面の巻き順が逆になるので flip_face_winding が必要。
#[inline]
pub fn gltf_pos_to_pmx(v: Vec3) -> Vec3 {
    Vec3::new(v.x * PMX_SCALE, v.y * PMX_SCALE, -v.z * PMX_SCALE)
}

/// VRM 0.0 用座標変換
/// VRM 0.0 のルートノードは Y=180° 回転を持つため、ワールド座標では +Z 向き・右手が +X 側。
/// Z 反転は不要（+Z → +Z を維持）、X のみ反転して左右を正す。
/// 行列式 = -1 → 面の巻き順が逆になるので flip_face_winding が必要。
#[inline]
pub fn gltf_pos_to_pmx_v0(v: Vec3) -> Vec3 {
    Vec3::new(-v.x * PMX_SCALE, v.y * PMX_SCALE, v.z * PMX_SCALE)
}

/// VRM 1.0 法線変換（Z のみ反転）
#[inline]
pub fn gltf_normal_to_pmx(n: Vec3) -> Vec3 {
    Vec3::new(n.x, n.y, -n.z)
}

/// VRM 0.0 法線変換（X のみ反転）
#[inline]
pub fn gltf_normal_to_pmx_v0(n: Vec3) -> Vec3 {
    Vec3::new(-n.x, n.y, n.z)
}

/// クォータニオン変換: (-x, y, -z) 変換に対応（Y軸180°回転相当）
#[inline]
pub fn gltf_quat_to_pmx(q: Quat) -> Quat {
    Quat::from_xyzw(-q.x, q.y, -q.z, q.w)
}

/// 面の巻き順反転（X反転でフロントフェースが逆になるため）
/// [a, b, c] → [a, c, b]
pub fn flip_face_winding(indices: &mut Vec<u32>) {
    let n = indices.len();
    let mut i = 0;
    while i + 2 < n {
        indices.swap(i + 1, i + 2);
        i += 3;
    }
}

/// glTF配列 [f32;3] → PMX Vec3
pub fn arr3_to_pmx(arr: [f32; 3]) -> Vec3 {
    gltf_pos_to_pmx(Vec3::new(arr[0], arr[1], arr[2]))
}

/// 単体テスト
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coord_x_flip() {
        // VRM 1.0: (x, y, -z) × scale → PMX
        let v = Vec3::new(1.0, 2.0, 3.0);
        let pmx = gltf_pos_to_pmx(v);
        assert!((pmx.x - 12.5).abs() < 1e-3);   // X は反転しない
        assert!((pmx.y - 25.0).abs() < 1e-3);
        assert!((pmx.z - (-37.5)).abs() < 1e-3); // Z のみ反転
    }

    #[test]
    fn test_face_winding() {
        let mut idx = vec![0, 1, 2, 3, 4, 5];
        flip_face_winding(&mut idx);
        assert_eq!(idx, vec![0, 2, 1, 3, 5, 4]);
    }
}
