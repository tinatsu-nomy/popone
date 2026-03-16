use super::parser::FbxProperty;
use super::scene::FbxScene;

/// Blend shape with per-vertex deltas already mapped to expanded vertex indices
pub(crate) struct BlendShape {
    pub name: String,
    /// (global_vertex_index, position_offset) in view-space coordinates
    pub deltas: Vec<(u32, [f32; 3])>,
}

/// Intermediate: raw blend shape data before expansion
pub(crate) struct RawBlendShape {
    name: String,
    /// Position offsets per affected control point
    vertices: Vec<[f32; 3]>,
    /// Control point indices
    indices: Vec<i32>,
}

/// Extract raw blend shapes for a geometry
pub(crate) fn extract_blend_shapes_raw(scene: &FbxScene, geom_id: i64) -> Vec<RawBlendShape> {
    let mut shapes = Vec::new();

    // Geometry ← Deformer(BlendShape) ← SubDeformer(BlendShapeChannel) ← Geometry(Shape)
    for &deformer_id in scene.children_of(geom_id) {
        let Some(obj) = scene.objects.get(&deformer_id) else {
            continue;
        };
        if obj.class != "Deformer" || obj.sub_type != "BlendShape" {
            continue;
        }

        for &channel_id in scene.children_of(deformer_id) {
            let Some(channel) = scene.objects.get(&channel_id) else {
                continue;
            };
            if channel.sub_type != "BlendShapeChannel" {
                continue;
            }

            let channel_name = &channel.name;

            for &shape_id in scene.children_of(channel_id) {
                let Some(shape_obj) = scene.objects.get(&shape_id) else {
                    continue;
                };
                if shape_obj.class != "Geometry" || shape_obj.sub_type != "Shape" {
                    continue;
                }

                let vertices = shape_obj
                    .node
                    .child("Vertices")
                    .and_then(|n| n.properties.first())
                    .and_then(extract_vec3_array)
                    .unwrap_or_default();

                let indices = shape_obj
                    .node
                    .child("Indexes")
                    .and_then(|n| n.properties.first())
                    .and_then(|p| p.as_i32_array())
                    .map(|v| v.to_vec())
                    .unwrap_or_default();

                let name = if shape_obj.name.is_empty() {
                    channel_name.clone()
                } else {
                    shape_obj.name.clone()
                };

                shapes.push(RawBlendShape {
                    name,
                    vertices,
                    indices,
                });
            }
        }
    }

    shapes
}

/// Expand raw blend shapes using control-point-to-vertex mapping and coordinate conversion
pub(crate) fn expand_blend_shapes(
    raw_shapes: Vec<RawBlendShape>,
    cp_to_vertices: &std::collections::HashMap<usize, Vec<u32>>,
    convert_fn: impl Fn([f32; 3]) -> [f32; 3],
) -> Vec<BlendShape> {
    raw_shapes
        .into_iter()
        .map(|raw| {
            let mut deltas = Vec::new();
            for (j, &cp_idx) in raw.indices.iter().enumerate() {
                let cp = cp_idx as usize;
                if j >= raw.vertices.len() {
                    break;
                }
                let offset = convert_fn(raw.vertices[j]);
                if let Some(expanded) = cp_to_vertices.get(&cp) {
                    for &vi in expanded {
                        deltas.push((vi, offset));
                    }
                }
            }
            BlendShape {
                name: raw.name,
                deltas,
            }
        })
        .collect()
}

fn extract_vec3_array(prop: &FbxProperty) -> Option<Vec<[f32; 3]>> {
    match prop {
        FbxProperty::F64Array(v) => Some(
            v.chunks_exact(3)
                .map(|c| [c[0] as f32, c[1] as f32, c[2] as f32])
                .collect(),
        ),
        FbxProperty::F32Array(v) => Some(
            v.chunks_exact(3)
                .map(|c| [c[0], c[1], c[2]])
                .collect(),
        ),
        _ => None,
    }
}
