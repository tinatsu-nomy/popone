use glam::{Vec2, Vec3, Vec4};

/// PMD model (root structure).
#[derive(Debug)]
pub struct PmdModel {
    pub header: PmdHeader,
    pub vertices: Vec<PmdVertex>,
    pub faces: Vec<[u16; 3]>,
    pub materials: Vec<PmdMaterial>,
    pub bones: Vec<PmdBone>,
    pub ik_list: Vec<PmdIk>,
    pub morphs: Vec<PmdMorph>,
    pub morph_display: Vec<u16>,
    pub bone_display_names: Vec<String>,
    pub bone_display: Vec<(u16, u8)>, // (bone_index, display_frame)
    pub toon_textures: [String; 10],
    pub rigid_bodies: Vec<PmdRigidBody>,
    pub joints: Vec<PmdJoint>,
    /// Optional English header.
    pub english_header: Option<PmdEnglishHeader>,
}

#[derive(Debug)]
pub struct PmdHeader {
    pub name: String,    // 20 bytes, Shift_JIS
    pub comment: String, // 256 bytes, Shift_JIS
}

#[derive(Debug)]
pub struct PmdEnglishHeader {
    pub name: String,
    pub comment: String,
    pub bone_names: Vec<String>,
    pub morph_names: Vec<String>,
    pub display_names: Vec<String>,
}

/// PMD vertex (fixed 38 bytes).
#[derive(Debug, Clone)]
pub struct PmdVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
    pub bone1: u16,
    pub bone2: u16,
    pub weight: u8,    // weight of bone1 (0..=100)
    pub edge_flag: u8, // 0: edge enabled, 1: edge disabled (note: opposite of material edge_flag)
}

/// PMD material (fixed 70 bytes).
#[derive(Debug, Clone)]
pub struct PmdMaterial {
    pub diffuse: Vec4, // RGBA
    pub specular_power: f32,
    pub specular: Vec3,
    pub ambient: Vec3,
    pub toon_index: u8,
    pub edge_flag: u8,        // 1: edge enabled, 0: edge disabled
    pub face_count: u32,      // face vertex count (number of faces * 3)
    pub texture_name: String, // 20 bytes; sphere texture separated by '*'
}

/// PMD bone (fixed 39 bytes).
#[derive(Debug, Clone)]
pub struct PmdBone {
    pub name: String,   // 20 bytes
    pub parent: u16,    // 0xFFFF = root
    pub child: u16,     // 0 or 0xFFFF = none
    pub bone_type: u8, // 0: rotate, 1: rotate+translate, 2: IK, 3: unknown, 4: under IK, 5: under rotate, 6: IK target, 7: hidden, 8: twist, 9: rotation link
    pub ik_parent: u16, // 0 = none
    pub position: Vec3,
}

/// PMD IK chain.
#[derive(Debug, Clone)]
pub struct PmdIk {
    pub bone_index: u16,  // IK bone
    pub target_bone: u16, // IK target bone
    pub chain_length: u8, // IK chain length
    pub iterations: u16,  // iteration count
    pub limit_angle: f32, // IK angle limit
    pub chain: Vec<u16>,  // IK chain bone indices
}

/// PMD morph (facial expression).
#[derive(Debug, Clone)]
pub struct PmdMorph {
    pub name: String, // 20 bytes
    pub vertex_count: u32,
    pub morph_type: u8, // 0: base, 1: brow, 2: eye, 3: mouth, 4: other
    pub vertices: Vec<PmdMorphVertex>,
}

#[derive(Debug, Clone)]
pub struct PmdMorphVertex {
    pub index: u32,   // base: global vertex index; other: index within the base morph
    pub offset: Vec3, // base: absolute position; other: offset
}

/// PMD rigid body.
#[derive(Debug, Clone)]
pub struct PmdRigidBody {
    pub name: String, // 20 bytes
    pub bone_index: u16,
    pub group: u8,
    pub no_collision_mask: u16,
    pub shape: u8, // 0: sphere, 1: box, 2: capsule
    pub size: Vec3,
    pub position: Vec3,
    pub rotation: Vec3, // radians
    pub mass: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub restitution: f32,
    pub friction: f32,
    pub physics_mode: u8, // 0: follow bone, 1: physics, 2: physics + bone
}

/// PMD joint.
#[derive(Debug, Clone)]
pub struct PmdJoint {
    pub name: String, // 20 bytes
    pub rigid_a: u32,
    pub rigid_b: u32,
    pub position: Vec3,
    pub rotation: Vec3,
    pub move_limit_lo: Vec3,
    pub move_limit_hi: Vec3,
    pub rot_limit_lo: Vec3,
    pub rot_limit_hi: Vec3,
    pub spring_move: Vec3,
    pub spring_rot: Vec3,
}
