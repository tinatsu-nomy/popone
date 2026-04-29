use serde::{Deserialize, Serialize};

/// VRM 1.0 VRMC_vrm extension root
#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VrmV1 {
    pub spec_version: Option<String>,
    pub meta: Option<VrmMeta>,
    pub humanoid: Option<VrmHumanoid>,
    pub first_person: Option<VrmFirstPerson>,
    pub look_at: Option<VrmLookAt>,
    pub expressions: Option<VrmExpressions>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VrmMeta {
    pub name: Option<String>,
    pub version: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    pub copyright_information: Option<String>,
    pub contact_information: Option<String>,
    #[serde(default)]
    pub references: Vec<String>,
    pub third_party_licenses: Option<String>,
    pub thumbnail_image: Option<i32>,
    pub license_url: Option<String>,
    pub avatar_permission: Option<String>,
    pub allow_excessively_violent_usage: Option<bool>,
    pub allow_excessively_sexual_usage: Option<bool>,
    pub commercial_usage: Option<String>,
    pub allow_political_or_religious_usage: Option<bool>,
    pub allow_antisocial_or_hate_usage: Option<bool>,
    pub credit_notation: Option<String>,
    pub allow_redistribution: Option<bool>,
    pub modification: Option<String>,
    pub other_license_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VrmHumanoid {
    pub human_bones: HumanBones,
}

/// VRM 1.0 humanoid bones are encoded as an object
#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HumanBones {
    pub hips: Option<HumanBoneRef>,
    pub spine: Option<HumanBoneRef>,
    pub chest: Option<HumanBoneRef>,
    pub upper_chest: Option<HumanBoneRef>,
    pub neck: Option<HumanBoneRef>,
    pub head: Option<HumanBoneRef>,
    pub left_eye: Option<HumanBoneRef>,
    pub right_eye: Option<HumanBoneRef>,
    pub jaw: Option<HumanBoneRef>,
    pub left_upper_leg: Option<HumanBoneRef>,
    pub left_lower_leg: Option<HumanBoneRef>,
    pub left_foot: Option<HumanBoneRef>,
    pub left_toes: Option<HumanBoneRef>,
    pub right_upper_leg: Option<HumanBoneRef>,
    pub right_lower_leg: Option<HumanBoneRef>,
    pub right_foot: Option<HumanBoneRef>,
    pub right_toes: Option<HumanBoneRef>,
    pub left_shoulder: Option<HumanBoneRef>,
    pub left_upper_arm: Option<HumanBoneRef>,
    pub left_lower_arm: Option<HumanBoneRef>,
    pub left_hand: Option<HumanBoneRef>,
    pub right_shoulder: Option<HumanBoneRef>,
    pub right_upper_arm: Option<HumanBoneRef>,
    pub right_lower_arm: Option<HumanBoneRef>,
    pub right_hand: Option<HumanBoneRef>,
    // Fingers
    pub left_thumb_metacarpal: Option<HumanBoneRef>,
    pub left_thumb_proximal: Option<HumanBoneRef>,
    pub left_thumb_distal: Option<HumanBoneRef>,
    pub left_index_proximal: Option<HumanBoneRef>,
    pub left_index_intermediate: Option<HumanBoneRef>,
    pub left_index_distal: Option<HumanBoneRef>,
    pub left_middle_proximal: Option<HumanBoneRef>,
    pub left_middle_intermediate: Option<HumanBoneRef>,
    pub left_middle_distal: Option<HumanBoneRef>,
    pub left_ring_proximal: Option<HumanBoneRef>,
    pub left_ring_intermediate: Option<HumanBoneRef>,
    pub left_ring_distal: Option<HumanBoneRef>,
    pub left_little_proximal: Option<HumanBoneRef>,
    pub left_little_intermediate: Option<HumanBoneRef>,
    pub left_little_distal: Option<HumanBoneRef>,
    pub right_thumb_metacarpal: Option<HumanBoneRef>,
    pub right_thumb_proximal: Option<HumanBoneRef>,
    pub right_thumb_distal: Option<HumanBoneRef>,
    pub right_index_proximal: Option<HumanBoneRef>,
    pub right_index_intermediate: Option<HumanBoneRef>,
    pub right_index_distal: Option<HumanBoneRef>,
    pub right_middle_proximal: Option<HumanBoneRef>,
    pub right_middle_intermediate: Option<HumanBoneRef>,
    pub right_middle_distal: Option<HumanBoneRef>,
    pub right_ring_proximal: Option<HumanBoneRef>,
    pub right_ring_intermediate: Option<HumanBoneRef>,
    pub right_ring_distal: Option<HumanBoneRef>,
    pub right_little_proximal: Option<HumanBoneRef>,
    pub right_little_intermediate: Option<HumanBoneRef>,
    pub right_little_distal: Option<HumanBoneRef>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanBoneRef {
    pub node: i32,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VrmFirstPerson {
    #[serde(default)]
    pub mesh_annotations: Vec<MeshAnnotation>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshAnnotation {
    pub node: i32,
    pub r#type: String,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VrmLookAt {
    pub offset_from_head_bone: Option<[f32; 3]>,
    pub r#type: Option<String>,
    pub range_map_horizontal_inner: Option<LookAtRange>,
    pub range_map_horizontal_outer: Option<LookAtRange>,
    pub range_map_vertical_down: Option<LookAtRange>,
    pub range_map_vertical_up: Option<LookAtRange>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LookAtRange {
    pub input_max_value: Option<f32>,
    pub output_scale: Option<f32>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VrmExpressions {
    pub preset: Option<ExpressionPreset>,
    pub custom: Option<std::collections::HashMap<String, Expression>>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExpressionPreset {
    pub happy: Option<Expression>,
    pub angry: Option<Expression>,
    pub sad: Option<Expression>,
    pub relaxed: Option<Expression>,
    pub surprised: Option<Expression>,
    pub aa: Option<Expression>,
    pub ih: Option<Expression>,
    pub ou: Option<Expression>,
    pub ee: Option<Expression>,
    pub oh: Option<Expression>,
    pub blink: Option<Expression>,
    pub blink_left: Option<Expression>,
    pub blink_right: Option<Expression>,
    pub look_up: Option<Expression>,
    pub look_down: Option<Expression>,
    pub look_left: Option<Expression>,
    pub look_right: Option<Expression>,
    pub neutral: Option<Expression>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Expression {
    pub morph_target_binds: Option<Vec<MorphTargetBind>>,
    pub material_color_binds: Option<Vec<MaterialColorBind>>,
    pub texture_transform_binds: Option<Vec<TextureTransformBind>>,
    pub is_binary: Option<bool>,
    pub override_blink: Option<String>,
    pub override_look_at: Option<String>,
    pub override_mouth: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MorphTargetBind {
    pub node: i32,
    pub index: i32,
    pub weight: f32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialColorBind {
    pub material: i32,
    pub r#type: String,
    pub target_value: [f32; 4],
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureTransformBind {
    pub material: i32,
    pub scale: Option<[f32; 2]>,
    pub offset: Option<[f32; 2]>,
}

// VRMC_springBone extension
#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SpringBoneV1 {
    pub spec_version: Option<String>,
    #[serde(default)]
    pub colliders: Vec<SpringCollider>,
    #[serde(default)]
    pub collider_groups: Vec<SpringColliderGroup>,
    #[serde(default)]
    pub springs: Vec<Spring>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpringCollider {
    pub node: i32,
    pub shape: SpringColliderShape,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpringColliderShape {
    pub sphere: Option<ColliderSphere>,
    pub capsule: Option<ColliderCapsule>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColliderSphere {
    pub offset: Option<[f32; 3]>,
    pub radius: Option<f32>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColliderCapsule {
    pub offset: Option<[f32; 3]>,
    pub radius: Option<f32>,
    pub tail: Option<[f32; 3]>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpringColliderGroup {
    pub name: Option<String>,
    #[serde(default)]
    pub colliders: Vec<i32>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spring {
    pub name: Option<String>,
    #[serde(default)]
    pub joints: Vec<SpringJoint>,
    pub collider_groups: Option<Vec<i32>>,
    pub center: Option<i32>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpringJoint {
    pub node: i32,
    pub hit_radius: Option<f32>,
    pub stiffness: Option<f32>,
    pub gravity_power: Option<f32>,
    pub gravity_dir: Option<[f32; 3]>,
    pub drag_force: Option<f32>,
}
