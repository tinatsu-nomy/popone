use glam::{Vec2, Vec3, Vec4};

/// PMXモデル全体
#[derive(Debug, Default)]
pub struct PmxModel {
    pub header: PmxHeader,
    pub model_info: PmxModelInfo,
    pub vertices: Vec<PmxVertex>,
    pub faces: Vec<[u32; 3]>,
    pub textures: Vec<String>,
    pub materials: Vec<PmxMaterial>,
    pub bones: Vec<PmxBone>,
    pub morphs: Vec<PmxMorph>,
    pub display_frames: Vec<PmxDisplayFrame>,
    pub rigid_bodies: Vec<PmxRigidBody>,
    pub joints: Vec<PmxJoint>,
}

#[derive(Debug, Clone)]
pub struct PmxHeader {
    pub version: f32,
    pub encoding: u8,      // 0:UTF16 1:UTF8
    pub additional_uvs: u8,
    pub vertex_index_size: u8,
    pub texture_index_size: u8,
    pub material_index_size: u8,
    pub bone_index_size: u8,
    pub morph_index_size: u8,
    pub rigid_body_index_size: u8,
}

impl Default for PmxHeader {
    fn default() -> Self {
        Self {
            version: 2.0,
            encoding: 0, // UTF16LE
            additional_uvs: 0,
            vertex_index_size: 2,
            texture_index_size: 1,
            material_index_size: 1,
            bone_index_size: 2,
            morph_index_size: 2,
            rigid_body_index_size: 2,
        }
    }
}

#[derive(Debug, Default)]
pub struct PmxModelInfo {
    pub name: String,
    pub name_en: String,
    pub comment: String,
    pub comment_en: String,
}

/// 頂点
#[derive(Debug, Clone)]
pub struct PmxVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
    pub weight: PmxWeightType,
    pub edge_scale: f32,
}

#[derive(Debug, Clone)]
pub enum PmxWeightType {
    Bdef1 { bone: i32 },
    Bdef2 { bone1: i32, bone2: i32, weight1: f32 },
    Bdef4 { bones: [i32; 4], weights: [f32; 4] },
}

/// 材質
#[derive(Debug, Clone)]
pub struct PmxMaterial {
    pub name: String,
    pub name_en: String,
    pub diffuse: Vec4,
    pub specular: Vec3,
    pub specular_power: f32,
    pub ambient: Vec3,
    pub draw_flags: u8,
    pub edge_color: Vec4,
    pub edge_size: f32,
    pub texture_index: Option<i32>,
    pub sphere_texture_index: Option<i32>,
    pub sphere_mode: u8,
    pub toon_ref: PmxToonRef,
    pub memo: String,
    pub face_count: u32, // 面数×3
}

#[derive(Debug, Clone, PartialEq)]
pub enum PmxToonRef {
    Texture(i32),
    Shared(u8),
}

/// 付与データ（回転付与・移動付与）
#[derive(Debug, Clone)]
pub struct PmxGrant {
    pub parent_index: i32,
    pub ratio: f32,
}

/// ボーン
#[derive(Debug, Clone)]
pub struct PmxBone {
    pub name: String,
    pub name_en: String,
    pub position: Vec3,
    pub parent_index: i32,
    pub deform_layer: i32,
    pub flags: u16,
    pub tail: BoneTail,
    pub ik: Option<PmxIk>,
    pub grant: Option<PmxGrant>,
}

#[derive(Debug, Clone)]
pub enum BoneTail {
    Offset(Vec3),
    BoneIndex(i32),
}

#[derive(Debug, Clone)]
pub struct PmxIk {
    pub target_bone: i32,
    pub loop_count: i32,
    pub limit_angle: f32,
    pub links: Vec<IkLink>,
}

#[derive(Debug, Clone)]
pub struct IkLink {
    pub bone_index: i32,
    pub angle_limit: bool,
    pub limit_min: Vec3,
    pub limit_max: Vec3,
}

/// PMXボーンフラグ定数
pub const BONE_FLAG_TAIL_IS_BONE: u16   = 0x0001;
pub const BONE_FLAG_ROTATABLE: u16      = 0x0002;
pub const BONE_FLAG_TRANSLATABLE: u16   = 0x0004;
pub const BONE_FLAG_VISIBLE: u16        = 0x0008;
pub const BONE_FLAG_OPERABLE: u16       = 0x0010;
pub const BONE_FLAG_IK: u16             = 0x0020;
pub const BONE_FLAG_LOCAL_GRANT: u16    = 0x0080;
pub const BONE_FLAG_ROTATION_GRANT: u16 = 0x0100;
pub const BONE_FLAG_MOVE_GRANT: u16     = 0x0200;
pub const BONE_FLAG_AXIS_FIXED: u16     = 0x0400;
pub const BONE_FLAG_LOCAL_AXIS: u16     = 0x0800;
pub const BONE_FLAG_PHYS_AFTER: u16     = 0x1000;
pub const BONE_FLAG_EXT_PARENT: u16     = 0x2000;

/// モーフ
#[derive(Debug, Clone)]
pub struct PmxMorph {
    pub name: String,
    pub name_en: String,
    pub panel: u8,
    pub morph_type: u8,
    pub offsets: PmxMorphOffsets,
}

#[derive(Debug, Clone)]
pub enum PmxMorphOffsets {
    Vertex(Vec<VertexMorphOffset>),
    Bone(Vec<BoneMorphOffset>),
    Material(Vec<MaterialMorphOffset>),
    Group(Vec<GroupMorphOffset>),
    Uv(Vec<UvMorphOffset>),
}

#[derive(Debug, Clone)]
pub struct VertexMorphOffset {
    pub vertex_index: u32,
    pub offset: Vec3,
}

#[derive(Debug, Clone)]
pub struct BoneMorphOffset {
    pub bone_index: i32,
    pub translation: Vec3,
    pub rotation: glam::Quat,
}

#[derive(Debug, Clone)]
pub struct MaterialMorphOffset {
    pub material_index: i32,
    pub offset_mode: u8,
    pub diffuse: Vec4,
    pub specular: Vec3,
    pub specular_power: f32,
    pub ambient: Vec3,
    pub edge_color: Vec4,
    pub edge_size: f32,
    pub texture_factor: Vec4,
    pub sphere_factor: Vec4,
    pub toon_factor: Vec4,
}

#[derive(Debug, Clone)]
pub struct GroupMorphOffset {
    pub morph_index: i32,
    pub weight: f32,
}

#[derive(Debug, Clone)]
pub struct UvMorphOffset {
    pub vertex_index: u32,
    pub offset: glam::Vec4,
}

/// 表示枠
#[derive(Debug, Clone)]
pub struct PmxDisplayFrame {
    pub name: String,
    pub name_en: String,
    pub is_special: u8,
    pub elements: Vec<DisplayFrameElement>,
}

#[derive(Debug, Clone)]
pub enum DisplayFrameElement {
    Bone(i32),
    Morph(i32),
}

/// 剛体
#[derive(Debug, Clone)]
pub struct PmxRigidBody {
    pub name: String,
    pub name_en: String,
    pub bone_index: i32,
    pub group: u8,
    pub no_collision_mask: u16,
    pub shape: u8,  // 0:球 1:箱 2:カプセル
    pub size: Vec3,
    pub position: Vec3,
    pub rotation: Vec3,
    pub mass: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub restitution: f32,
    pub friction: f32,
    pub physics_mode: u8,
}

/// ジョイント
#[derive(Debug, Clone)]
pub struct PmxJoint {
    pub name: String,
    pub name_en: String,
    pub joint_type: u8,
    pub rigid_a: i32,
    pub rigid_b: i32,
    pub position: Vec3,
    pub rotation: Vec3,
    pub move_limit_lo: Vec3,
    pub move_limit_hi: Vec3,
    pub rot_limit_lo: Vec3,
    pub rot_limit_hi: Vec3,
    pub spring_move: Vec3,
    pub spring_rot: Vec3,
}
