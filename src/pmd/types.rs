use glam::{Vec2, Vec3, Vec4};

/// PMDモデル全体
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
    /// 英語ヘッダ（あれば）
    pub english_header: Option<PmdEnglishHeader>,
}

#[derive(Debug)]
pub struct PmdHeader {
    pub name: String,    // 20byte Shift_JIS
    pub comment: String, // 256byte Shift_JIS
}

#[derive(Debug)]
pub struct PmdEnglishHeader {
    pub name: String,
    pub comment: String,
    pub bone_names: Vec<String>,
    pub morph_names: Vec<String>,
    pub display_names: Vec<String>,
}

/// PMD頂点 (38byte固定)
#[derive(Debug, Clone)]
pub struct PmdVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
    pub bone1: u16,
    pub bone2: u16,
    pub weight: u8,    // bone1のウェイト (0〜100)
    pub edge_flag: u8, // 1:エッジあり 0:エッジなし
}

/// PMD材質 (70byte固定)
#[derive(Debug, Clone)]
pub struct PmdMaterial {
    pub diffuse: Vec4, // RGBA
    pub specular_power: f32,
    pub specular: Vec3,
    pub ambient: Vec3,
    pub toon_index: u8,
    pub edge_flag: u8,        // 1:エッジあり 0:エッジなし
    pub face_count: u32,      // 面頂点数（面数×3）
    pub texture_name: String, // 20byte "*"でスフィア区切り
}

/// PMDボーン (39byte固定)
#[derive(Debug, Clone)]
pub struct PmdBone {
    pub name: String,   // 20byte
    pub parent: u16,    // 0xFFFF = ルート
    pub child: u16,     // 0 or 0xFFFF = なし
    pub bone_type: u8, // 0:回転 1:回転移動 2:IK 3:不明 4:IK影響下 5:回転影響下 6:IK接続先 7:非表示 8:捩り 9:回転連動
    pub ik_parent: u16, // 0 = なし
    pub position: Vec3,
}

/// PMD IK
#[derive(Debug, Clone)]
pub struct PmdIk {
    pub bone_index: u16,  // IKボーン
    pub target_bone: u16, // IKターゲットボーン
    pub chain_length: u8, // IKチェーンの長さ
    pub iterations: u16,  // 再帰演算回数
    pub limit_angle: f32, // IK値制限角度
    pub chain: Vec<u16>,  // IKチェーンボーンIndex
}

/// PMDモーフ（表情）
#[derive(Debug, Clone)]
pub struct PmdMorph {
    pub name: String, // 20byte
    pub vertex_count: u32,
    pub morph_type: u8, // 0:base 1:眉 2:目 3:口 4:その他
    pub vertices: Vec<PmdMorphVertex>,
}

#[derive(Debug, Clone)]
pub struct PmdMorphVertex {
    pub index: u32,   // base: グローバル頂点Index, other: baseモーフ内のIndex
    pub offset: Vec3, // base: 絶対位置, other: オフセット
}

/// PMD剛体
#[derive(Debug, Clone)]
pub struct PmdRigidBody {
    pub name: String, // 20byte
    pub bone_index: u16,
    pub group: u8,
    pub no_collision_mask: u16,
    pub shape: u8, // 0:球 1:箱 2:カプセル
    pub size: Vec3,
    pub position: Vec3,
    pub rotation: Vec3, // ラジアン
    pub mass: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub restitution: f32,
    pub friction: f32,
    pub physics_mode: u8, // 0:ボーン追従 1:物理 2:物理+Bone
}

/// PMDジョイント
#[derive(Debug, Clone)]
pub struct PmdJoint {
    pub name: String, // 20byte
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
