//! MikkTSpace アルゴリズムによる接線ベクトル生成
//!
//! VRM 1.0 仕様: 「TANGENT はエクスポートせず、インポート時に MikkTSpace アルゴリズムで計算」
//! UniVRM 参照実装: vrmc_materials_mtoon_utility.hlsl の MToon_GetTangentToWorld()

use glam::Vec4;

use super::types::IrMesh;

/// tangent の xyz 成分が有効（非退化）かどうかを判定する閾値
const TANGENT_VALID_THRESHOLD: f32 = 1e-8;

/// MikkTSpace の Geometry トレイト実装用ラッパー
struct MikkGeometry<'a> {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: &'a [u32],
    /// コーナー単位の接線ベクトル（corner = face * 3 + vert）
    corner_tangents: Vec<[f32; 4]>,
}

impl<'a> mikktspace::Geometry for MikkGeometry<'a> {
    fn num_faces(&self) -> usize {
        self.indices.len() / 3
    }

    fn num_vertices_of_face(&self, _face: usize) -> usize {
        3
    }

    fn position(&self, face: usize, vert: usize) -> [f32; 3] {
        self.positions[self.indices[face * 3 + vert] as usize]
    }

    fn normal(&self, face: usize, vert: usize) -> [f32; 3] {
        self.normals[self.indices[face * 3 + vert] as usize]
    }

    fn tex_coord(&self, face: usize, vert: usize) -> [f32; 2] {
        self.uvs[self.indices[face * 3 + vert] as usize]
    }

    fn set_tangent_encoded(&mut self, tangent: [f32; 4], face: usize, vert: usize) {
        self.corner_tangents[face * 3 + vert] = tangent;
    }
}

/// tangent の xyz が有効かどうかを判定する
#[inline]
fn has_valid_tangent(tangent: Vec4) -> bool {
    tangent.truncate().length_squared() > TANGENT_VALID_THRESHOLD
}

/// IrMesh の全頂点に対して MikkTSpace 接線を生成する。
///
/// `normal_tex_coord` は normalTexture が参照する TEXCOORD セット番号。
/// texCoord=1 かつ UV1 が存在する場合は UV1 を使って接線を生成する。
/// 既に有効な接線を持つ頂点（tangent.xyz の長さが閾値以上）はスキップされる。
/// MikkTSpace 生成に失敗した場合はデフォルト接線 (1,0,0,1) を設定する。
///
/// handedness (w) が異なるコーナーを共有する頂点は自動的に分割される。
pub fn generate_tangents(mesh: &mut IrMesh, normal_tex_coord: u32) {
    // 全頂点が既に有効な接線を持っているならスキップ
    let needs_generation = mesh.vertices.iter().any(|v| !has_valid_tangent(v.tangent));
    if !needs_generation {
        return;
    }

    let vertex_count = mesh.vertices.len();
    let positions: Vec<[f32; 3]> = mesh
        .vertices
        .iter()
        .map(|v| v.position.to_array())
        .collect();
    let normals: Vec<[f32; 3]> = mesh.vertices.iter().map(|v| v.normal.to_array()).collect();
    // normalTexture.texCoord に対応する UV セットで接線を生成
    // UV1 不在時は zero UV を使用（描画側 mesh.rs と同一のフォールバック、UniVRM MeshData.cs 準拠）
    // 材質の texCoord はメッシュ間で共有されるため、材質側の書き換えは行わない
    let uvs: Vec<[f32; 2]> = if normal_tex_coord == 1 {
        if mesh.uvs1.len() == vertex_count {
            mesh.uvs1.clone()
        } else {
            log::debug!("Tangent generation: texCoord=1 but UV1 absent, using zero UV");
            vec![[0.0, 0.0]; vertex_count]
        }
    } else {
        mesh.vertices.iter().map(|v| v.uv.to_array()).collect()
    };

    let corner_count = mesh.indices.len();

    // MikkTSpace 生成（ブロックスコープで indices の借用を限定）
    let corner_tangents_opt = {
        let mut geom = MikkGeometry {
            positions,
            normals,
            uvs,
            indices: &mesh.indices,
            corner_tangents: vec![[0.0; 4]; corner_count],
        };
        let ok = mikktspace::generate_tangents(&mut geom);
        if ok {
            Some(geom.corner_tangents)
        } else {
            None
        }
    };
    // geom がドロップされ mesh.indices の借用が解放される

    if let Some(corner_tangents) = corner_tangents_opt {
        // --- w 不一致による頂点分割 ---
        // 各頂点のコーナーを正 w / 負 w にグループ分け
        let mut split_count = 0usize;
        let mut vert_neg_corners: Vec<Vec<usize>> = vec![Vec::new(); vertex_count];
        let mut vert_pos_corners: Vec<Vec<usize>> = vec![Vec::new(); vertex_count];

        for (corner, &vi) in mesh.indices.iter().enumerate() {
            let vi = vi as usize;
            // glTF 由来の有効な tangent を持つ頂点は分割不要
            if has_valid_tangent(mesh.vertices[vi].tangent) {
                continue;
            }
            if corner_tangents[corner][3] >= 0.0 {
                vert_pos_corners[vi].push(corner);
            } else {
                vert_neg_corners[vi].push(corner);
            }
        }

        for vi in 0..vertex_count {
            if vert_pos_corners[vi].is_empty() || vert_neg_corners[vi].is_empty() {
                continue; // 全コーナーが同一 w → 分割不要
            }
            // 少数派のコーナーを新頂点にリマップ
            let (minority_corners, minority_w) =
                if vert_pos_corners[vi].len() <= vert_neg_corners[vi].len() {
                    (&vert_pos_corners[vi], 1.0f32)
                } else {
                    (&vert_neg_corners[vi], -1.0f32)
                };

            let new_vi = mesh.vertices.len() as u32;
            mesh.vertices.push(mesh.vertices[vi]);
            if !mesh.uvs1.is_empty() {
                let uv1 = if vi < mesh.uvs1.len() {
                    mesh.uvs1[vi]
                } else {
                    [0.0, 0.0]
                };
                mesh.uvs1.push(uv1);
            }
            // モーフターゲットの頂点インデックスも複製
            for mt in &mut mesh.morph_targets {
                if let Some(&(_, offset)) = mt
                    .position_offsets
                    .iter()
                    .find(|(idx, _)| *idx == vi as u32)
                {
                    mt.position_offsets.push((new_vi, offset));
                }
                if let Some(&(_, offset)) =
                    mt.normal_offsets.iter().find(|(idx, _)| *idx == vi as u32)
                {
                    mt.normal_offsets.push((new_vi, offset));
                }
                if let Some(&(_, offset)) =
                    mt.tangent_offsets.iter().find(|(idx, _)| *idx == vi as u32)
                {
                    mt.tangent_offsets.push((new_vi, offset));
                }
            }
            // 少数派コーナーのインデックスを新頂点に張り替え
            for &corner in minority_corners {
                mesh.indices[corner] = new_vi;
            }
            split_count += 1;
            log::trace!(
                "tangent w 分割: mesh='{}' vertex={} → new={} (minority_w={:.0})",
                mesh.name,
                vi,
                new_vi,
                minority_w
            );
        }

        if split_count > 0 {
            log::info!(
                "MikkTSpace w mismatch vertex split: mesh='{}' splits={} ({}->{}vertices)",
                mesh.name,
                split_count,
                vertex_count,
                mesh.vertices.len()
            );
        }

        // --- コーナー tangent を頂点単位に集約 ---
        let new_vertex_count = mesh.vertices.len();
        let mut tangent_acc: Vec<([f32; 3], f32, u32)> = vec![([0.0; 3], 1.0, 0); new_vertex_count];

        for (corner, &vi) in mesh.indices.iter().enumerate() {
            let vi = vi as usize;
            if has_valid_tangent(mesh.vertices[vi].tangent) {
                continue; // glTF 由来 tangent を保持
            }
            let ct = &corner_tangents[corner];
            let acc = &mut tangent_acc[vi];
            acc.0[0] += ct[0];
            acc.0[1] += ct[1];
            acc.0[2] += ct[2];
            acc.1 = ct[3]; // w（分割済みなので頂点内で一貫）
            acc.2 += 1;
        }

        for (i, v) in mesh.vertices.iter_mut().enumerate() {
            if has_valid_tangent(v.tangent) {
                continue; // glTF から読み込み済み
            }
            let (xyz, w, count) = &tangent_acc[i];
            if *count > 0 {
                let inv = 1.0 / (*count as f32);
                let tx = xyz[0] * inv;
                let ty = xyz[1] * inv;
                let tz = xyz[2] * inv;
                let len = (tx * tx + ty * ty + tz * tz).sqrt();
                if len > TANGENT_VALID_THRESHOLD {
                    v.tangent = Vec4::new(tx / len, ty / len, tz / len, *w);
                } else {
                    v.tangent = Vec4::new(1.0, 0.0, 0.0, 1.0);
                }
            } else {
                // MikkTSpace が生成しなかった頂点（孤立頂点等）
                v.tangent = Vec4::new(1.0, 0.0, 0.0, 1.0);
            }
        }
        log::debug!(
            "MikkTSpace tangent generation complete: mesh='{}' vertices={}",
            mesh.name,
            new_vertex_count
        );
    } else {
        log::warn!(
            "MikkTSpace tangent generation failed: mesh='{}' - using default tangents",
            mesh.name
        );
        for v in &mut mesh.vertices {
            if !has_valid_tangent(v.tangent) {
                v.tangent = Vec4::new(1.0, 0.0, 0.0, 1.0);
            }
        }
    }
}
