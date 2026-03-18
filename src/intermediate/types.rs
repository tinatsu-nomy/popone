use glam::{Mat4, Vec2, Vec3, Vec4};

/// ソースファイル形式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SourceFormat {
    #[default]
    Vrm1,
    Vrm0,
    Fbx,
    Pmx,
    Pmd,
}

impl SourceFormat {
    pub fn label(&self) -> &str {
        match self {
            SourceFormat::Vrm1 => "VRM 1.0",
            SourceFormat::Vrm0 => "VRM 0.0",
            SourceFormat::Fbx => "FBX",
            SourceFormat::Pmx => "PMX",
            SourceFormat::Pmd => "PMD",
        }
    }

    /// VRM 0.0 の座標変換を使うか
    pub fn is_vrm0(&self) -> bool {
        matches!(self, SourceFormat::Vrm0)
    }

    /// PMX/PMD 形式か
    pub fn is_pmx_pmd(&self) -> bool {
        matches!(self, SourceFormat::Pmx | SourceFormat::Pmd)
    }
}

/// Aスタンス変換の結果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AStanceResult {
    /// 未適用（チェックボックスOFF、または非対応形式）
    #[default]
    NotRequested,
    /// 適用成功（補正した腕の数: 通常2）
    Applied(usize),
    /// 既にAスタンスに近いためスキップ
    AlreadyAStance,
    /// 腕ボーンが見つからず変換失敗
    NotFound,
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
    /// Aスタンス変換の結果
    pub astance_result: AStanceResult,
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

    /// 別の IrModel をこのモデルにマージ（追加読み込み）
    ///
    /// 同名ボーンは既存側に統合し、固有ボーンのみ新規追加する。
    /// テクスチャ・材質・メッシュ・モーフ・物理のインデックスはリマップテーブルで変換。
    ///
    /// 返り値: (統合されたボーン数, 新規追加されたボーン数)
    pub fn merge(&mut self, mut other: IrModel) -> (usize, usize) {
        let tex_offset = self.textures.len();
        let mat_offset = self.materials.len();
        let rigid_offset = self.physics.rigid_bodies.len();
        let morph_offset = self.morphs.len();
        let vtx_offset = self.total_vertices();

        // ── ボーンリマップテーブル構築 ──
        // other_bone_idx → self_bone_idx のマッピング
        let mut bone_name_to_self: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::with_capacity(self.bones.len());
        for (i, bone) in self.bones.iter().enumerate() {
            bone_name_to_self.insert(&bone.name, i);
        }

        let other_bone_count = other.bones.len();
        // リマップテーブル: other 側の各ボーンが self 側のどのIndexに対応するか
        let mut bone_remap: Vec<usize> = vec![usize::MAX; other_bone_count];
        // other 側のどのボーンが新規追加されたか
        let mut is_new_bone: Vec<bool> = vec![true; other_bone_count];
        let mut merged_count: usize = 0;

        // パス1: 同名+同親名の候補を暫定マーク（順序非依存）
        // candidate[i] = Some(self_idx) なら統合候補
        let mut candidate: Vec<Option<usize>> = vec![None; other_bone_count];
        for (i, other_bone) in other.bones.iter().enumerate() {
            if let Some(&self_idx) = bone_name_to_self.get(other_bone.name.as_str()) {
                let self_parent_name = self.bones[self_idx].parent
                    .map(|p| self.bones[p].name.as_str());
                let other_parent_name = other_bone.parent
                    .map(|p| other.bones[p].name.as_str());
                if self_parent_name == other_parent_name {
                    candidate[i] = Some(self_idx);
                }
            }
        }

        // パス2: 親の統合状態を伝播して最終決定（順序非依存）
        // 候補ボーンの親が候補でない場合は統合を取り消す（異なる部分木の子孫が誤統合されるのを防ぐ）
        let mut changed = true;
        while changed {
            changed = false;
            for i in 0..other_bone_count {
                if candidate[i].is_none() { continue; }
                if let Some(parent_idx) = other.bones[i].parent {
                    // 親が候補でない → この子も統合不可
                    if candidate[parent_idx].is_none() {
                        candidate[i] = None;
                        changed = true;
                    }
                }
            }
        }

        // 候補を確定
        for i in 0..other_bone_count {
            if let Some(self_idx) = candidate[i] {
                bone_remap[i] = self_idx;
                is_new_bone[i] = false;
                merged_count += 1;
            }
        }

        // 新規ボーンの正確なIndex割り当て
        let mut next_new_idx = self.bones.len();
        for i in 0..other_bone_count {
            if is_new_bone[i] {
                bone_remap[i] = next_new_idx;
                next_new_idx += 1;
            }
        }
        let new_bone_count = next_new_idx - self.bones.len();

        // node_index 衝突回避: 既存の最大node_id + 1をオフセットに使用
        let max_existing_node = self.bones.iter().map(|b| b.node_index).max().unwrap_or(0);
        let max_mesh_node = self.meshes.iter().map(|m| m.node_index).max().unwrap_or(0);
        let node_offset = max_existing_node.max(max_mesh_node) + 1;

        // 新規ボーンを self.bones に追加（parent/children をリマップ）
        for (other_idx, other_bone) in other.bones.iter().enumerate() {
            if !is_new_bone[other_idx] {
                // 既存ボーンに統合 → 追加モデル側の子を既存ボーンの children に補完
                // （新規ボーンだけでなく、既存ボーン同士の接続も補完する）
                let self_idx = bone_remap[other_idx];
                for &child_other_idx in &other_bone.children {
                    let child_self_idx = bone_remap[child_other_idx];
                    if !self.bones[self_idx].children.contains(&child_self_idx) {
                        self.bones[self_idx].children.push(child_self_idx);
                    }
                }
                // ヒューマノイドメタデータの補完（既存側が未設定の場合のみ）
                if self.bones[self_idx].vrm_bone_name.is_none() {
                    if let Some(ref vrm_name) = other_bone.vrm_bone_name {
                        self.bones[self_idx].vrm_bone_name = Some(vrm_name.clone());
                    }
                }
                continue;
            }

            let mut new_bone = other_bone.clone();
            // parent をリマップ
            if let Some(ref mut p) = new_bone.parent {
                *p = bone_remap[*p];
            }
            // children をリマップ
            for child in &mut new_bone.children {
                *child = bone_remap[*child];
            }
            // node_index の衝突回避（固定オフセットで全構造体と一致させる）
            new_bone.node_index += node_offset;
            self.bones.push(new_bone);
        }

        // node_to_bone マッピング更新（同じ node_offset を使用）
        for (node, bone_idx) in other.node_to_bone {
            let remapped = bone_remap[bone_idx];
            self.node_to_bone.insert(node + node_offset, remapped);
        }

        // ── テクスチャ ──
        self.textures.append(&mut other.textures);

        // ── 材質: テクスチャIndexをオフセット ──
        for mat in &mut other.materials {
            if let Some(ref mut idx) = mat.texture_index {
                *idx += tex_offset;
            }
            if let Some(ref mut idx) = mat.shade_texture_index {
                *idx += tex_offset;
            }
            if let Some(ref mut idx) = mat.outline_width_texture_index {
                *idx += tex_offset;
            }
        }
        self.materials.append(&mut other.materials);

        // ── メッシュ: 材質Index・頂点ウェイトのボーンIndexをリマップ ──
        for mesh in &mut other.meshes {
            mesh.material_index += mat_offset;
            mesh.node_index += node_offset;
            for vtx in &mut mesh.vertices {
                for (bone_idx, _) in &mut vtx.weights {
                    *bone_idx = bone_remap[*bone_idx];
                }
            }
        }
        self.meshes.append(&mut other.meshes);

        // ── モーフ: グローバル頂点Indexをオフセット ──
        for morph in &mut other.morphs {
            match &mut morph.kind {
                IrMorphKind::Vertex(entries) => {
                    for (global_idx, _) in entries.iter_mut() {
                        *global_idx += vtx_offset;
                    }
                }
                IrMorphKind::Group(entries) => {
                    for (morph_idx, _) in entries.iter_mut() {
                        *morph_idx += morph_offset;
                    }
                }
            }
        }
        self.morphs.append(&mut other.morphs);

        // ── 物理: ボーンIndexをリマップ、剛体Indexをオフセット ──
        for rb in &mut other.physics.rigid_bodies {
            if let Some(ref mut idx) = rb.bone_index {
                *idx = bone_remap[*idx];
            }
        }
        self.physics.rigid_bodies.append(&mut other.physics.rigid_bodies);

        for joint in &mut other.physics.joints {
            joint.rigid_a += rigid_offset;
            joint.rigid_b += rigid_offset;
        }
        self.physics.joints.append(&mut other.physics.joints);

        // ── メタ情報更新 ──
        self.name = format!("{} + {}", self.name, other.name);
        // ヒューマノイドボーン数を再計算（共有ボーンへの補完分も含めるため）
        self.humanoid_bone_count = self.bones.iter()
            .filter(|b| b.vrm_bone_name.is_some())
            .count();
        // Aスタンス変換結果の統合
        // NotRequested は透過、Applied は NotFound より優先（小物アペンド対策）
        self.astance_result = match (self.astance_result, other.astance_result) {
            // NotRequested は無視して相手の値を採用
            (AStanceResult::NotRequested, other) => other,
            (host, AStanceResult::NotRequested) => host,
            // Applied 同士は合算
            (AStanceResult::Applied(a), AStanceResult::Applied(b)) => AStanceResult::Applied(a + b),
            // Applied + NotFound/AlreadyAStance → Applied を優先
            // （メインモデルが変換済みなら、小物の NotFound は問題なし）
            (AStanceResult::Applied(n), _) | (_, AStanceResult::Applied(n)) => AStanceResult::Applied(n),
            // 両方 NotFound
            (AStanceResult::NotFound, AStanceResult::NotFound) => AStanceResult::NotFound,
            // AlreadyAStance + NotFound → AlreadyAStance を優先
            (AStanceResult::AlreadyAStance, _) | (_, AStanceResult::AlreadyAStance) => AStanceResult::AlreadyAStance,
        };

        (merged_count, new_bone_count)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// テスト用ヘルパー: 最小限のボーンを作成
    fn bone(name: &str, parent: Option<usize>, children: Vec<usize>) -> IrBone {
        IrBone {
            name: name.to_string(),
            name_en: String::new(),
            vrm_bone_name: None,
            position: Vec3::ZERO,
            global_mat: Mat4::IDENTITY,
            parent,
            children,
            node_index: 0,
            is_physics: false,
        }
    }

    /// テスト用ヘルパー: ウェイト付き頂点を含むメッシュ
    fn mesh_with_weights(name: &str, mat_idx: usize, bone_indices: &[usize]) -> IrMesh {
        let vertices = bone_indices.iter().map(|&bi| IrVertex {
            position: Vec3::ZERO,
            normal: Vec3::Y,
            uv: Vec2::ZERO,
            weights: vec![(bi, 1.0)],
            edge_scale: 1.0,
        }).collect();
        IrMesh {
            name: name.to_string(),
            vertices,
            indices: vec![0, 1, 2],
            material_index: mat_idx,
            morph_targets: vec![],
            node_index: 0,
        }
    }

    #[test]
    fn test_merge_shared_bones_are_unified() {
        // ホストモデル: Armature → Spine → Head
        let mut host = IrModel {
            name: "Host".into(),
            bones: vec![
                bone("Armature", None, vec![1]),
                bone("Spine", Some(0), vec![2]),
                bone("Head", Some(1), vec![]),
            ],
            meshes: vec![mesh_with_weights("body", 0, &[1, 2, 1])],
            materials: vec![IrMaterial { name: "mat_body".into(), ..Default::default() }],
            ..Default::default()
        };

        // 追加モデル: Armature → Spine → Ribbon（Ribbonだけが新規）
        let other = IrModel {
            name: "Costume".into(),
            bones: vec![
                bone("Armature", None, vec![1]),
                bone("Spine", Some(0), vec![2]),
                bone("Ribbon", Some(1), vec![]),
            ],
            meshes: vec![mesh_with_weights("costume", 0, &[1, 2, 1])],
            materials: vec![IrMaterial { name: "mat_costume".into(), ..Default::default() }],
            ..Default::default()
        };

        let (merged, new) = host.merge(other);

        // Armature(0), Spine(1) が統合、Ribbon(3) が新規追加
        assert_eq!(merged, 2, "Armature と Spine が統合されるべき");
        assert_eq!(new, 1, "Ribbon のみ新規追加");
        assert_eq!(host.bones.len(), 4, "Host(3) + New(1) = 4");

        // ボーン名確認
        assert_eq!(host.bones[0].name, "Armature");
        assert_eq!(host.bones[1].name, "Spine");
        assert_eq!(host.bones[2].name, "Head");
        assert_eq!(host.bones[3].name, "Ribbon");

        // Ribbon の parent は既存の Spine(1) を指すべき
        assert_eq!(host.bones[3].parent, Some(1));

        // Spine の children に Ribbon(3) が追加されているべき
        assert!(host.bones[1].children.contains(&3), "Spine の children に Ribbon(3) がない");
        // Head(2) も残っている
        assert!(host.bones[1].children.contains(&2), "Spine の children に Head(2) がない");

        // 衣装メッシュの頂点ウェイトが既存ボーンにリマップされていること
        let costume_mesh = &host.meshes[1];
        // other の Spine(idx=1) → self の Spine(idx=1)
        assert_eq!(costume_mesh.vertices[0].weights[0].0, 1, "Spine にリマップ");
        // other の Ribbon(idx=2) → self の Ribbon(idx=3)
        assert_eq!(costume_mesh.vertices[1].weights[0].0, 3, "Ribbon にリマップ");

        // 材質が2つ
        assert_eq!(host.materials.len(), 2);
        assert_eq!(host.materials[1].name, "mat_costume");
        // 衣装メッシュの material_index がオフセット済み
        assert_eq!(costume_mesh.material_index, 1);

        // モデル名
        assert_eq!(host.name, "Host + Costume");
    }

    #[test]
    fn test_merge_physics_bone_remap() {
        let mut host = IrModel {
            name: "Host".into(),
            bones: vec![
                bone("Armature", None, vec![1]),
                bone("Spine", Some(0), vec![]),
            ],
            physics: IrPhysics {
                rigid_bodies: vec![IrRigidBody {
                    name: "rb_spine".into(),
                    bone_index: Some(1),
                    group: 0, no_collision_mask: 0xFFFF,
                    shape: RigidShape::Sphere { radius: 0.1 },
                    position: Vec3::ZERO, rotation: Vec3::ZERO,
                    mass: 1.0, linear_damping: 0.5, angular_damping: 0.5,
                    restitution: 0.0, friction: 0.5, physics_mode: 0,
                }],
                joints: vec![],
            },
            ..Default::default()
        };

        // 追加: Armature(共通) → NewBone(新規)、剛体はNewBoneに紐づく
        let other = IrModel {
            name: "Accessory".into(),
            bones: vec![
                bone("Armature", None, vec![1]),
                bone("NewBone", Some(0), vec![]),
            ],
            physics: IrPhysics {
                rigid_bodies: vec![IrRigidBody {
                    name: "rb_new".into(),
                    bone_index: Some(1), // other の NewBone(1)
                    group: 1, no_collision_mask: 0xFFFF,
                    shape: RigidShape::Sphere { radius: 0.05 },
                    position: Vec3::ZERO, rotation: Vec3::ZERO,
                    mass: 0.5, linear_damping: 0.5, angular_damping: 0.5,
                    restitution: 0.0, friction: 0.5, physics_mode: 1,
                }],
                joints: vec![IrJoint {
                    name: "joint_new".into(),
                    rigid_a: 0, // other 側の最初の剛体（ただし実際はホスト側）
                    rigid_b: 0, // other 側の剛体 (rb_new)
                    position: Vec3::ZERO, rotation: Vec3::ZERO,
                    move_limit_lo: Vec3::ZERO, move_limit_hi: Vec3::ZERO,
                    rot_limit_lo: Vec3::ZERO, rot_limit_hi: Vec3::ZERO,
                    spring_move: Vec3::ZERO, spring_rot: Vec3::ZERO,
                }],
            },
            ..Default::default()
        };

        let (merged, new) = host.merge(other);
        assert_eq!(merged, 1); // Armature
        assert_eq!(new, 1);    // NewBone

        // NewBone は self[2] に追加
        assert_eq!(host.bones[2].name, "NewBone");
        assert_eq!(host.bones[2].parent, Some(0)); // Armature

        // 追加側の剛体の bone_index がリマップ: other NewBone(1) → self NewBone(2)
        assert_eq!(host.physics.rigid_bodies[1].bone_index, Some(2));

        // ジョイントの rigid_a/b がオフセット: +1 (ホスト側の剛体数)
        assert_eq!(host.physics.joints[0].rigid_a, 1); // 0 + rigid_offset(1)
        assert_eq!(host.physics.joints[0].rigid_b, 1);
    }

    #[test]
    fn test_merge_no_shared_bones() {
        // 共通ボーンなしの場合は全ボーンが新規追加される
        let mut host = IrModel {
            name: "A".into(),
            bones: vec![bone("Root_A", None, vec![])],
            ..Default::default()
        };
        let other = IrModel {
            name: "B".into(),
            bones: vec![bone("Root_B", None, vec![])],
            ..Default::default()
        };

        let (merged, new) = host.merge(other);
        assert_eq!(merged, 0);
        assert_eq!(new, 1);
        assert_eq!(host.bones.len(), 2);
        assert_eq!(host.bones[1].name, "Root_B");
    }

    #[test]
    fn test_merge_morph_vertex_index_offset() {
        let mut host = IrModel {
            name: "Host".into(),
            meshes: vec![mesh_with_weights("m", 0, &[0, 0, 0])], // 3頂点
            bones: vec![bone("Root", None, vec![])],
            materials: vec![IrMaterial::default()],
            morphs: vec![IrMorph {
                name: "smile".into(), name_en: String::new(), panel: 3,
                kind: IrMorphKind::Vertex(vec![(0, Vec3::Y)]),
            }],
            ..Default::default()
        };
        let other = IrModel {
            name: "Other".into(),
            meshes: vec![mesh_with_weights("m2", 0, &[0, 0])], // 2頂点
            bones: vec![bone("Root", None, vec![])],
            materials: vec![IrMaterial::default()],
            morphs: vec![IrMorph {
                name: "blink".into(), name_en: String::new(), panel: 2,
                kind: IrMorphKind::Vertex(vec![(1, Vec3::X)]),
            }],
            ..Default::default()
        };

        host.merge(other);

        // other のモーフの頂点Index は +3 (host の頂点数) されるべき
        if let IrMorphKind::Vertex(ref entries) = host.morphs[1].kind {
            assert_eq!(entries[0].0, 4, "vtx_offset=3, 元Index=1 → 4");
        } else {
            panic!("頂点モーフであるべき");
        }
    }
}
