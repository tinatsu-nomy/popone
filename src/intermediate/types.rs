use glam::{Mat4, Vec2, Vec3, Vec4};

/// ソースファイル形式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SourceFormat {
    #[default]
    Vrm1,
    Vrm0,
    Fbx,
}

impl SourceFormat {
    pub fn label(&self) -> &str {
        match self {
            SourceFormat::Vrm1 => "VRM 1.0",
            SourceFormat::Vrm0 => "VRM 0.0",
            SourceFormat::Fbx => "FBX",
        }
    }

    /// VRM 0.0 の座標変換を使うか
    pub fn is_vrm0(&self) -> bool {
        matches!(self, SourceFormat::Vrm0)
    }
}

/// 中間表現モデル
#[derive(Debug, Default)]
pub struct IrModel {
    pub name: String,
    pub comment: String,
    pub bones: Vec<IrBone>,
    pub meshes: Vec<IrMesh>,
    pub materials: Vec<IrMaterial>,
    pub textures: Vec<IrTexture>,
    pub morphs: Vec<IrMorph>,
    pub physics: IrPhysics,
    /// ノードIndex → ボーンIndex のマッピング
    pub node_to_bone: std::collections::HashMap<usize, usize>,
    /// ソースファイル形式（座標変換の分岐に使用）
    pub source_format: SourceFormat,
    /// ヒューマノイドリグ種別（FBX 用、None = 未検出/VRM）
    pub rig_type: Option<String>,
    /// マッピングされたヒューマノイドボーン数
    pub humanoid_bone_count: usize,
}

impl IrModel {
    /// 全メッシュの頂点数合計
    pub fn total_vertices(&self) -> usize {
        self.meshes.iter().map(|m| m.vertices.len()).sum()
    }

    /// 全メッシュの面数合計
    pub fn total_faces(&self) -> usize {
        self.meshes.iter().map(|m| m.indices.len() / 3).sum()
    }
}

/// 中間ボーン
#[derive(Debug, Clone)]
pub struct IrBone {
    pub name: String,
    pub name_en: String,
    /// VRMヒューマノイドボーン名（例: "hips", "spine"）
    pub vrm_bone_name: Option<String>,
    /// グローバル位置（glTF座標系）
    pub position: Vec3,
    /// グローバル変換行列（glTF座標系）：コライダーのローカルオフセット変換に使用
    pub global_mat: Mat4,
    /// 親ボーンIndex（なければNone）
    pub parent: Option<usize>,
    /// 子ボーンIndexリスト
    pub children: Vec<usize>,
    /// glTFノードIndex
    pub node_index: usize,
    /// ボーン追従/物理フラグ
    pub is_physics: bool,
}

/// 中間メッシュ
#[derive(Debug)]
pub struct IrMesh {
    pub name: String,
    pub vertices: Vec<IrVertex>,
    pub indices: Vec<u32>,
    pub material_index: usize,
    /// モーフターゲット（頂点オフセット）
    pub morph_targets: Vec<IrMorphTarget>,
    /// このメッシュが属するglTFノードIndex
    pub node_index: usize,
}

/// 中間頂点
#[derive(Debug, Clone)]
pub struct IrVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
    pub weights: Vec<(usize, f32)>, // (ボーンIndex, ウェイト) 最大4
    pub edge_scale: f32,            // エッジ倍率（outlineWidthMultiplyTexture由来）
}

/// モーフターゲット（メッシュ内）
#[derive(Debug, Clone)]
pub struct IrMorphTarget {
    pub name: String,
    /// 各頂点の位置オフセット（Noneなら変化なし）
    pub position_offsets: Vec<Option<Vec3>>,
}

/// 中間材質
#[derive(Debug, Clone)]
pub struct IrMaterial {
    pub name: String,
    pub diffuse: Vec4,
    pub specular: Vec3,
    pub specular_power: f32,
    pub ambient: Vec3,
    pub texture_index: Option<usize>,
    pub is_double_sided: bool,
    pub is_mtoon: bool,
    /// エッジ（輪郭線）
    pub edge_color: Vec4,
    pub edge_size: f32,
    /// MToon固有
    pub shade_color: Option<Vec3>,
    pub shade_texture_index: Option<usize>,
    /// アウトライン幅テクスチャ（glTFテクスチャIndex）
    /// VRM 1.0: outlineWidthMultiplyTexture (Gチャネル)
    /// VRM 0.0: _OutlineWidthTexture
    pub outline_width_texture_index: Option<usize>,
    /// FBX元テクスチャファイル名（一括割り当て用）
    pub source_texture_name: Option<String>,
}

impl IrMaterial {
    /// テクスチャ有り時の PMX 材質パラメータを設定
    pub fn apply_textured_defaults(&mut self) {
        let alpha = self.diffuse.w;
        self.diffuse = Vec4::new(1.0, 1.0, 1.0, alpha);
        self.ambient = Vec3::new(0.5, 0.5, 0.5);
        self.specular = Vec3::ZERO;
        self.specular_power = 0.0;
    }
}

impl Default for IrMaterial {
    fn default() -> Self {
        Self {
            name: String::new(),
            diffuse: Vec4::new(1.0, 1.0, 1.0, 1.0),
            specular: Vec3::ZERO,
            specular_power: 0.0,
            ambient: Vec3::new(0.4, 0.4, 0.4),
            texture_index: None,
            is_double_sided: false,
            is_mtoon: false,
            edge_color: Vec4::new(0.0, 0.0, 0.0, 1.0),
            edge_size: 0.0,
            shade_color: None,
            shade_texture_index: None,
            outline_width_texture_index: None,
            source_texture_name: None,
        }
    }
}

/// 中間テクスチャ
#[derive(Debug, Clone)]
pub struct IrTexture {
    /// ファイル名（出力時に使用）
    pub filename: String,
    /// 生データ（PNG/JPEG）
    pub data: Vec<u8>,
    pub mime_type: String,
}

/// 中間モーフ
#[derive(Debug, Clone)]
pub struct IrMorph {
    pub name: String,
    pub name_en: String,
    /// パネル種別（1:眉, 2:目, 3:口, 4:その他）
    pub panel: u8,
    pub kind: IrMorphKind,
}

#[derive(Debug, Clone)]
pub enum IrMorphKind {
    /// 頂点モーフ: (グローバル頂点Index, オフセット)
    Vertex(Vec<(usize, Vec3)>),
    /// グループモーフ: (モーフIndex, 率)
    Group(Vec<(usize, f32)>),
}

/// 物理情報
#[derive(Debug, Default)]
pub struct IrPhysics {
    pub rigid_bodies: Vec<IrRigidBody>,
    pub joints: Vec<IrJoint>,
}

/// 剛体
#[derive(Debug, Clone)]
pub struct IrRigidBody {
    pub name: String,
    pub bone_index: Option<usize>,
    pub group: u8,
    pub no_collision_mask: u16,
    pub shape: RigidShape,
    pub position: Vec3,
    pub rotation: Vec3,
    pub mass: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub restitution: f32,
    pub friction: f32,
    pub physics_mode: u8, // 0:ボーン追従 1:物理演算 2:物理+Bone
}

#[derive(Debug, Clone)]
pub enum RigidShape {
    Sphere { radius: f32 },
    Box { size: Vec3 },
    Capsule { radius: f32, height: f32 },
}

/// ジョイント
#[derive(Debug, Clone)]
pub struct IrJoint {
    pub name: String,
    pub rigid_a: usize,
    pub rigid_b: usize,
    pub position: Vec3,
    pub rotation: Vec3,
    pub move_limit_lo: Vec3,
    pub move_limit_hi: Vec3,
    pub rot_limit_lo: Vec3,
    pub rot_limit_hi: Vec3,
    pub spring_move: Vec3,
    pub spring_rot: Vec3,
}
