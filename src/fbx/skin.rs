use super::parser::FbxProperty;
use super::scene::FbxScene;
use glam::Mat4;

pub struct Cluster {
    pub bone_id: i64,
    pub bone_name: String,
    pub indices: Vec<i32>,
    pub weights: Vec<f64>,
    pub transform: Mat4,
    pub transform_link: Mat4,
}

pub struct SkinData {
    pub clusters: Vec<Cluster>,
}

pub fn extract_skin(scene: &FbxScene, geom_id: i64) -> Option<SkinData> {
    // Geometry ← Deformer(Skin) ← SubDeformer(Cluster) ← Model(bone)
    let skin_deformer = scene
        .children_of(geom_id)
        .iter()
        .filter_map(|&id| scene.objects.get(&id))
        .find(|obj| obj.class == "Deformer" && obj.sub_type == "Skin")?;

    let mut clusters = Vec::new();

    for &sub_id in scene.children_of(skin_deformer.id) {
        let Some(sub_obj) = scene.objects.get(&sub_id) else {
            continue;
        };
        if sub_obj.sub_type != "Cluster" {
            continue;
        }

        // Bone model is a child of the cluster in FBX connections
        let bone_id = scene
            .children_of(sub_id)
            .iter()
            .filter_map(|&id| scene.objects.get(&id))
            .find(|obj| obj.class == "Model")
            .map(|obj| obj.id)
            .unwrap_or(0);

        let bone_name = scene
            .objects
            .get(&bone_id)
            .map(|obj| obj.name.clone())
            .unwrap_or_default();

        let indices = sub_obj
            .node
            .child("Indexes")
            .and_then(|n| n.properties.first())
            .and_then(|p| p.as_i32_array())
            .map(|v| v.to_vec())
            .unwrap_or_default();

        let weights = sub_obj
            .node
            .child("Weights")
            .and_then(|n| n.properties.first())
            .and_then(|p| p.as_f64_array())
            .map(|v| v.to_vec())
            .unwrap_or_default();

        let transform = extract_matrix(sub_obj.node, "Transform");
        let transform_link = extract_matrix(sub_obj.node, "TransformLink");

        clusters.push(Cluster {
            bone_id,
            bone_name,
            indices,
            weights,
            transform,
            transform_link,
        });
    }

    if clusters.is_empty() {
        None
    } else {
        Some(SkinData { clusters })
    }
}

fn extract_matrix(node: &super::parser::FbxNode, name: &str) -> Mat4 {
    let Some(child) = node.child(name) else {
        return Mat4::IDENTITY;
    };
    let Some(prop) = child.properties.first() else {
        return Mat4::IDENTITY;
    };
    let values = match prop {
        FbxProperty::F64Array(v) if v.len() >= 16 => v,
        _ => return Mat4::IDENTITY,
    };

    // FBX row-major → glam column-major (transpose)
    Mat4::from_cols_array(&[
        values[0] as f32,
        values[4] as f32,
        values[8] as f32,
        values[12] as f32,
        values[1] as f32,
        values[5] as f32,
        values[9] as f32,
        values[13] as f32,
        values[2] as f32,
        values[6] as f32,
        values[10] as f32,
        values[14] as f32,
        values[3] as f32,
        values[7] as f32,
        values[11] as f32,
        values[15] as f32,
    ])
}
