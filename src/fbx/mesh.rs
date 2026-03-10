use std::collections::HashMap;
use super::parser::{FbxNode, FbxProperty};

/// f64/f32 配列の統一アクセス
pub(crate) enum VertexData<'a> {
    F64(&'a [f64]),
    F32(&'a [f32]),
}

impl VertexData<'_> {
    pub fn get3(&self, idx: usize) -> Option<[f32; 3]> {
        let base = idx * 3;
        match self {
            VertexData::F64(v) => {
                if base + 2 < v.len() {
                    Some([v[base] as f32, v[base + 1] as f32, v[base + 2] as f32])
                } else {
                    None
                }
            }
            VertexData::F32(v) => {
                if base + 2 < v.len() {
                    Some([v[base], v[base + 1], v[base + 2]])
                } else {
                    None
                }
            }
        }
    }

    pub fn get2(&self, idx: usize) -> Option<[f32; 2]> {
        let base = idx * 2;
        match self {
            VertexData::F64(v) => {
                if base + 1 < v.len() {
                    Some([v[base] as f32, 1.0 - v[base + 1] as f32])
                } else {
                    None
                }
            }
            VertexData::F32(v) => {
                if base + 1 < v.len() {
                    Some([v[base], 1.0 - v[base + 1]])
                } else {
                    None
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum UvMapping {
    ByPolygonVertex,
    ByControlPoint,
}

pub(crate) fn extract_vertices(geom: &FbxNode) -> Option<VertexData<'_>> {
    let node = geom.child("Vertices")?;
    let prop = node.properties.first()?;
    match prop {
        FbxProperty::F64Array(v) => Some(VertexData::F64(v)),
        FbxProperty::F32Array(v) => Some(VertexData::F32(v)),
        _ => None,
    }
}

pub(crate) fn extract_polygon_indices(geom: &FbxNode) -> Option<Vec<i32>> {
    let node = geom.child("PolygonVertexIndex")?;
    node.properties.first()?.as_i32_array().map(|v| v.to_vec())
}

pub(crate) fn extract_normals(geom: &FbxNode) -> Option<VertexData<'_>> {
    let layer = geom.child("LayerElementNormal")?;
    let node = layer.child("Normals")?;
    let prop = node.properties.first()?;
    match prop {
        FbxProperty::F64Array(v) => Some(VertexData::F64(v)),
        FbxProperty::F32Array(v) => Some(VertexData::F32(v)),
        _ => None,
    }
}

pub(crate) fn extract_normal_indices(geom: &FbxNode) -> Vec<i32> {
    let Some(layer) = geom.child("LayerElementNormal") else {
        return Vec::new();
    };
    let Some(node) = layer.child("NormalsIndex") else {
        return Vec::new();
    };
    let Some(prop) = node.properties.first() else {
        return Vec::new();
    };
    prop.as_i32_array().map(|v| v.to_vec()).unwrap_or_default()
}

pub(crate) fn extract_material_indices(geom: &FbxNode) -> Vec<i32> {
    let Some(layer) = geom.child("LayerElementMaterial") else {
        return Vec::new();
    };
    let Some(node) = layer.child("Materials") else {
        return Vec::new();
    };
    let Some(prop) = node.properties.first() else {
        return Vec::new();
    };
    prop.as_i32_array().map(|v| v.to_vec()).unwrap_or_default()
}

pub(crate) fn extract_uvs(geom: &FbxNode) -> (Option<VertexData<'_>>, Vec<i32>, UvMapping) {
    let Some(layer) = geom.child("LayerElementUV") else {
        return (None, Vec::new(), UvMapping::ByPolygonVertex);
    };

    let mapping_str = layer
        .child("MappingInformationType")
        .and_then(|n| n.properties.first())
        .and_then(|p| p.as_string())
        .unwrap_or("ByPolygonVertex");

    let mapping = match mapping_str {
        "ByControlPoint" => UvMapping::ByControlPoint,
        _ => UvMapping::ByPolygonVertex,
    };

    let uvs = layer
        .child("UV")
        .and_then(|n| n.properties.first())
        .and_then(|p| match p {
            FbxProperty::F64Array(v) => Some(VertexData::F64(v)),
            FbxProperty::F32Array(v) => Some(VertexData::F32(v)),
            _ => None,
        });

    let uv_indices = layer
        .child("UVIndex")
        .and_then(|n| n.properties.first())
        .and_then(|p| p.as_i32_array())
        .map(|v| v.to_vec())
        .unwrap_or_default();

    (uvs, uv_indices, mapping)
}

pub(crate) fn get_uv(
    uvs: Option<&VertexData<'_>>,
    uv_indices: &[i32],
    mapping: &UvMapping,
    poly_vert_idx: usize,
    control_point_idx: usize,
) -> [f32; 2] {
    let Some(uvs) = uvs else {
        return [0.0; 2];
    };

    let uv_idx = match mapping {
        UvMapping::ByPolygonVertex => {
            if !uv_indices.is_empty() {
                uv_indices
                    .get(poly_vert_idx)
                    .copied()
                    .unwrap_or(0) as usize
            } else {
                poly_vert_idx
            }
        }
        UvMapping::ByControlPoint => {
            if !uv_indices.is_empty() {
                uv_indices
                    .get(control_point_idx)
                    .copied()
                    .unwrap_or(0) as usize
            } else {
                control_point_idx
            }
        }
    };

    uvs.get2(uv_idx).unwrap_or([0.0; 2])
}

pub(crate) fn get_normal(
    normals: Option<&VertexData<'_>>,
    normal_indices: &[i32],
    poly_vert_idx: usize,
) -> [f32; 3] {
    let Some(normals) = normals else {
        return [0.0; 3];
    };
    let idx = if !normal_indices.is_empty() {
        if poly_vert_idx < normal_indices.len() {
            normal_indices[poly_vert_idx] as usize
        } else {
            return [0.0; 3];
        }
    } else {
        poly_vert_idx
    };
    normals.get3(idx).unwrap_or([0.0; 3])
}

/// ゼロ法線の頂点をフラット法線で補完する
/// 全頂点がゼロの場合は全て計算し、一部がゼロの場合はその面の法線のみ補完
pub(crate) fn fill_missing_normals(
    positions: &[[f32; 3]],
    normals: &mut [[f32; 3]],
    indices: &[u32],
) {
    use glam::Vec3;
    let zero = [0.0f32; 3];
    for tri in indices.chunks_exact(3) {
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        if i0 >= positions.len() || i1 >= positions.len() || i2 >= positions.len() {
            continue;
        }
        // この三角形の頂点にゼロ法線があるか
        let has_zero = normals[i0] == zero || normals[i1] == zero || normals[i2] == zero;
        if !has_zero {
            continue;
        }
        let v0 = Vec3::from(positions[i0]);
        let v1 = Vec3::from(positions[i1]);
        let v2 = Vec3::from(positions[i2]);
        let normal = (v1 - v0).cross(v2 - v0).normalize_or_zero();
        let n = normal.to_array();
        if normals[i0] == zero { normals[i0] = n; }
        if normals[i1] == zero { normals[i1] = n; }
        if normals[i2] == zero { normals[i2] = n; }
    }
}

pub(crate) fn extract_diffuse_color(mat_node: &FbxNode) -> [f32; 3] {
    let Some(props) = mat_node.child("Properties70") else {
        return [0.8, 0.8, 0.8];
    };
    for p in &props.children {
        if p.name == "P" {
            let name = p.properties.first().and_then(|v| v.as_string());
            if name == Some("DiffuseColor") {
                let r = p.properties.get(4).and_then(|v| v.as_f64_value()).unwrap_or(0.8);
                let g = p.properties.get(5).and_then(|v| v.as_f64_value()).unwrap_or(0.8);
                let b = p.properties.get(6).and_then(|v| v.as_f64_value()).unwrap_or(0.8);
                return [r as f32, g as f32, b as f32];
            }
        }
    }
    [0.8, 0.8, 0.8]
}

/// 材質の追加プロパティ
pub(crate) struct MaterialProps {
    pub specular: [f32; 3],
    pub shininess: f32,
    pub ambient: [f32; 3],
    pub opacity: f32,
    pub reflection_color: [f32; 3],
    pub reflection_factor: f32,
}

pub(crate) fn extract_material_props(mat_node: &FbxNode) -> MaterialProps {
    let mut props = MaterialProps {
        specular: [0.0; 3],
        shininess: 0.0,
        ambient: [0.4, 0.4, 0.4],
        opacity: 1.0,
        reflection_color: [0.0; 3],
        reflection_factor: 0.0,
    };
    let Some(p70) = mat_node.child("Properties70") else {
        return props;
    };
    let mut has_opacity = false;
    for p in &p70.children {
        if p.name != "P" {
            continue;
        }
        let name = p.properties.first().and_then(|v| v.as_string()).unwrap_or("");
        match name {
            "SpecularColor" => {
                props.specular[0] = p.properties.get(4).and_then(|v| v.as_f64_value()).unwrap_or(0.0) as f32;
                props.specular[1] = p.properties.get(5).and_then(|v| v.as_f64_value()).unwrap_or(0.0) as f32;
                props.specular[2] = p.properties.get(6).and_then(|v| v.as_f64_value()).unwrap_or(0.0) as f32;
            }
            "Shininess" | "ShininessExponent" => {
                props.shininess = p.properties.get(4).and_then(|v| v.as_f64_value()).unwrap_or(0.0) as f32;
            }
            "AmbientColor" => {
                props.ambient[0] = p.properties.get(4).and_then(|v| v.as_f64_value()).unwrap_or(0.4) as f32;
                props.ambient[1] = p.properties.get(5).and_then(|v| v.as_f64_value()).unwrap_or(0.4) as f32;
                props.ambient[2] = p.properties.get(6).and_then(|v| v.as_f64_value()).unwrap_or(0.4) as f32;
            }
            "Opacity" => {
                props.opacity = p.properties.get(4).and_then(|v| v.as_f64_value()).unwrap_or(1.0) as f32;
                has_opacity = true;
            }
            "TransparencyFactor" => {
                // Opacity が明示されていなければ TransparencyFactor から算出
                if !has_opacity {
                    let tf = p.properties.get(4).and_then(|v| v.as_f64_value()).unwrap_or(0.0) as f32;
                    props.opacity = 1.0 - tf;
                }
            }
            "ReflectionColor" => {
                props.reflection_color[0] = p.properties.get(4).and_then(|v| v.as_f64_value()).unwrap_or(0.0) as f32;
                props.reflection_color[1] = p.properties.get(5).and_then(|v| v.as_f64_value()).unwrap_or(0.0) as f32;
                props.reflection_color[2] = p.properties.get(6).and_then(|v| v.as_f64_value()).unwrap_or(0.0) as f32;
            }
            "ReflectionFactor" => {
                props.reflection_factor = p.properties.get(4).and_then(|v| v.as_f64_value()).unwrap_or(0.0) as f32;
            }
            _ => {}
        }
    }
    props
}

/// Skin ウェイトをコントロールポイント→展開頂点にマッピング
pub(crate) fn build_vertex_weights(
    skin: &super::skin::SkinData,
    bone_id_to_ir: &HashMap<i64, usize>,
    cp_to_verts: &HashMap<usize, Vec<u32>>,
    num_vertices: usize,
) -> Vec<Vec<(usize, f32)>> {
    let mut weights: Vec<Vec<(usize, f32)>> = vec![Vec::new(); num_vertices];

    for cluster in &skin.clusters {
        let Some(&bone_idx) = bone_id_to_ir.get(&cluster.bone_id) else {
            continue;
        };
        for (i, &cp_idx) in cluster.indices.iter().enumerate() {
            let w = cluster.weights.get(i).copied().unwrap_or(0.0) as f32;
            if w.abs() < 1e-8 {
                continue;
            }
            if let Some(verts) = cp_to_verts.get(&(cp_idx as usize)) {
                for &vi in verts {
                    weights[vi as usize].push((bone_idx, w));
                }
            }
        }
    }

    // 頂点あたり最大4ボーンに制限・正規化
    for w in &mut weights {
        w.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        w.truncate(4);
        let sum: f32 = w.iter().map(|(_, v)| v).sum();
        if sum > 1e-8 {
            for (_, v) in w.iter_mut() {
                *v /= sum;
            }
        }
    }

    weights
}
