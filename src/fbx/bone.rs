use super::parser::FbxNode;
use super::scene::FbxScene;
use glam::{EulerRot, Mat4, Quat, Vec3};
use std::collections::HashMap;

pub struct Bone {
    pub id: i64,
    pub name: String,
    pub parent_index: Option<usize>,
    pub children_indices: Vec<usize>,
    pub local_translation: Vec3,
    pub local_rotation: Quat,
    pub local_scale: Vec3,
    pub world_transform: Mat4,
}

#[derive(Default)]
pub struct BoneHierarchy {
    pub bones: Vec<Bone>,
    pub id_to_index: HashMap<i64, usize>,
}

impl BoneHierarchy {
    pub fn from_scene(scene: &FbxScene) -> Self {
        let mut bone_ids: Vec<i64> = Vec::new();
        for obj in scene.objects.values() {
            if obj.class == "Model"
                && (obj.sub_type == "LimbNode" || obj.sub_type == "Root" || obj.sub_type == "Null")
            {
                bone_ids.push(obj.id);
            }
        }

        if bone_ids.is_empty() {
            return Self::default();
        }

        bone_ids.sort();

        let mut bones = Vec::new();
        let mut id_to_index = HashMap::new();

        for &id in &bone_ids {
            let obj = &scene.objects[&id];
            let (translation, rotation_euler, pre_rotation_euler, scale) =
                extract_transform(obj.node);
            let pre_rotation = euler_deg_to_quat(pre_rotation_euler);
            let rotation = pre_rotation * euler_deg_to_quat(rotation_euler);

            let index = bones.len();
            id_to_index.insert(id, index);

            bones.push(Bone {
                id,
                name: obj.name.clone(),
                parent_index: None,
                children_indices: Vec::new(),
                local_translation: translation,
                local_rotation: rotation,
                local_scale: scale,
                world_transform: Mat4::IDENTITY,
            });
        }

        // Set parent/child from connections
        for i in 0..bones.len() {
            let id = bones[i].id;
            for &pid in scene.parents_of(id) {
                if let Some(&parent_idx) = id_to_index.get(&pid) {
                    bones[i].parent_index = Some(parent_idx);
                    bones[parent_idx].children_indices.push(i);
                    break;
                }
            }
        }

        let mut hierarchy = BoneHierarchy { bones, id_to_index };
        hierarchy.compute_world_transforms();
        hierarchy
    }

    fn compute_world_transforms(&mut self) {
        let roots: Vec<usize> = (0..self.bones.len())
            .filter(|&i| self.bones[i].parent_index.is_none())
            .collect();

        for root in roots {
            self.compute_world_recursive(root, Mat4::IDENTITY);
        }
    }

    fn compute_world_recursive(&mut self, index: usize, parent_world: Mat4) {
        let local = Mat4::from_scale_rotation_translation(
            self.bones[index].local_scale,
            self.bones[index].local_rotation,
            self.bones[index].local_translation,
        );
        self.bones[index].world_transform = parent_world * local;

        let children = self.bones[index].children_indices.clone();
        let world = self.bones[index].world_transform;
        for child_idx in children {
            self.compute_world_recursive(child_idx, world);
        }
    }

    pub fn root_bones(&self) -> Vec<usize> {
        (0..self.bones.len())
            .filter(|&i| self.bones[i].parent_index.is_none())
            .collect()
    }
}

pub(crate) fn extract_transform(node: &FbxNode) -> (Vec3, Vec3, Vec3, Vec3) {
    let mut translation = Vec3::ZERO;
    let mut rotation = Vec3::ZERO;
    let mut pre_rotation = Vec3::ZERO;
    let mut scale = Vec3::ONE;

    if let Some(props) = node.child("Properties70") {
        for p in &props.children {
            if p.name != "P" {
                continue;
            }
            let name = p
                .properties
                .first()
                .and_then(|v| v.as_string())
                .unwrap_or("");
            match name {
                "Lcl Translation" => {
                    translation.x = p
                        .properties
                        .get(4)
                        .and_then(|v| v.as_f64_value())
                        .unwrap_or(0.0) as f32;
                    translation.y = p
                        .properties
                        .get(5)
                        .and_then(|v| v.as_f64_value())
                        .unwrap_or(0.0) as f32;
                    translation.z = p
                        .properties
                        .get(6)
                        .and_then(|v| v.as_f64_value())
                        .unwrap_or(0.0) as f32;
                }
                "Lcl Rotation" => {
                    rotation.x = p
                        .properties
                        .get(4)
                        .and_then(|v| v.as_f64_value())
                        .unwrap_or(0.0) as f32;
                    rotation.y = p
                        .properties
                        .get(5)
                        .and_then(|v| v.as_f64_value())
                        .unwrap_or(0.0) as f32;
                    rotation.z = p
                        .properties
                        .get(6)
                        .and_then(|v| v.as_f64_value())
                        .unwrap_or(0.0) as f32;
                }
                "PreRotation" => {
                    pre_rotation.x = p
                        .properties
                        .get(4)
                        .and_then(|v| v.as_f64_value())
                        .unwrap_or(0.0) as f32;
                    pre_rotation.y = p
                        .properties
                        .get(5)
                        .and_then(|v| v.as_f64_value())
                        .unwrap_or(0.0) as f32;
                    pre_rotation.z = p
                        .properties
                        .get(6)
                        .and_then(|v| v.as_f64_value())
                        .unwrap_or(0.0) as f32;
                }
                "Lcl Scaling" => {
                    scale.x = p
                        .properties
                        .get(4)
                        .and_then(|v| v.as_f64_value())
                        .unwrap_or(1.0) as f32;
                    scale.y = p
                        .properties
                        .get(5)
                        .and_then(|v| v.as_f64_value())
                        .unwrap_or(1.0) as f32;
                    scale.z = p
                        .properties
                        .get(6)
                        .and_then(|v| v.as_f64_value())
                        .unwrap_or(1.0) as f32;
                }
                _ => {}
            }
        }
    }

    (translation, rotation, pre_rotation, scale)
}

/// FBX Euler angles: degrees, default rotation order XYZ (extrinsic) = ZYX (intrinsic)
pub(crate) fn euler_deg_to_quat(deg: Vec3) -> Quat {
    let rad = deg * (std::f32::consts::PI / 180.0);
    Quat::from_euler(EulerRot::ZYX, rad.z, rad.y, rad.x)
}
