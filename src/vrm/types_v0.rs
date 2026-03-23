use serde::{Deserialize, Serialize};

/// VRM 0.0 ルート拡張
#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VrmV0 {
    pub exporter_version: Option<String>,
    pub spec_version: Option<String>,
    pub meta: Option<VrmMeta>,
    pub humanoid: Option<VrmHumanoid>,
    pub first_person: Option<VrmFirstPerson>,
    pub blend_shape_master: Option<BlendShapeMaster>,
    pub secondary_animation: Option<SecondaryAnimation>,
    pub material_properties: Vec<VrmMaterialProperty>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VrmMeta {
    pub title: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub contact_information: Option<String>,
    pub reference: Option<String>,
    pub allowed_user_name: Option<String>,
    pub violent_ussage_name: Option<String>,
    pub sexual_ussage_name: Option<String>,
    pub commercial_ussage_name: Option<String>,
    pub other_permission_url: Option<String>,
    pub license_name: Option<String>,
    pub other_license_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VrmHumanoid {
    pub human_bones: Vec<HumanBone>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanBone {
    pub bone: String,
    pub node: i32,
    pub use_default_values: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VrmFirstPerson {
    pub first_person_bone: Option<i32>,
    pub first_person_bone_offset: Option<Vec3Json>,
    pub mesh_annotations: Vec<MeshAnnotation>,
    pub look_at_type_name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshAnnotation {
    pub mesh: Option<i32>,
    pub first_person_flag: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BlendShapeMaster {
    pub blend_shape_groups: Vec<BlendShapeGroup>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlendShapeGroup {
    pub name: String,
    pub preset_name: String,
    pub binds: Vec<BlendShapeBind>,
    pub material_values: Vec<MaterialValueBind>,
    pub is_binary: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlendShapeBind {
    pub mesh: i32,
    pub index: i32,
    pub weight: f32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialValueBind {
    pub material_name: String,
    pub property_name: String,
    pub target_value: Vec<f32>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SecondaryAnimation {
    pub bone_groups: Vec<BoneGroup>,
    pub collider_groups: Vec<ColliderGroup>,
}

/// VRM 0.0 の Vec3フィールドは配列でなくオブジェクト {"x":...,"y":...,"z":...} 形式
#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub struct Vec3Json {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl From<Vec3Json> for glam::Vec3 {
    fn from(v: Vec3Json) -> Self {
        glam::Vec3::new(v.x, v.y, v.z)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoneGroup {
    pub comment: Option<String>,
    pub stiffiness: Option<f32>, // VRM0.0はtypoあり
    pub gravity_power: Option<f32>,
    pub gravity_dir: Option<Vec3Json>,
    pub drag_force: Option<f32>,
    pub center: Option<i32>,
    pub hit_radius: Option<f32>,
    pub bones: Vec<i32>,
    pub collider_groups: Vec<i32>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColliderGroup {
    pub node: i32,
    pub colliders: Vec<SphereCollider>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SphereCollider {
    pub offset: Vec3Json,
    pub radius: f32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VrmMaterialProperty {
    pub name: String,
    pub shader: String,
    pub render_queue: Option<i32>,
    pub float_properties: Option<serde_json::Value>,
    pub vector_properties: Option<serde_json::Value>,
    pub texture_properties: Option<serde_json::Value>,
    pub keyword_map: Option<serde_json::Value>,
    pub tag_map: Option<serde_json::Value>,
}
