use std::sync::Arc;

use glam::{Mat4, Vec2, Vec3, Vec4};

/// テクスチャの UV ラッピングモード
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IrWrapMode {
    Repeat,
    ClampToEdge,
    MirroredRepeat,
}

/// テクスチャの拡大フィルタリングモード（mag_filter）
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IrMagFilter {
    Nearest,
    Linear,
}

/// テクスチャの縮小フィルタリングモード（min_filter + mipmap_filter）
/// glTF の minFilter 6 値をそのまま保持する
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IrMinFilter {
    Nearest,
    Linear,
    NearestMipmapNearest,
    LinearMipmapNearest,
    NearestMipmapLinear,
    LinearMipmapLinear,
}

/// glTF sampler に対応するサンプラー情報
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IrSamplerInfo {
    pub wrap_u: IrWrapMode,
    pub wrap_v: IrWrapMode,
    pub mag_filter: IrMagFilter,
    pub min_filter: IrMinFilter,
}

impl Default for IrSamplerInfo {
    fn default() -> Self {
        Self {
            wrap_u: IrWrapMode::Repeat,
            wrap_v: IrWrapMode::Repeat,
            mag_filter: IrMagFilter::Linear,
            min_filter: IrMinFilter::LinearMipmapLinear,
        }
    }
}

/// 材質編集時にテクスチャを割り当てる先のスロットを表す enum（§B）。
///
/// `assign_texture_core` が slot 引数で受け取り、MToon 補助テクスチャの 8 スロット +
/// 標準 3 スロット + MMD 専用 2 スロットを区別する。Step 1-4 時点では `BaseColor`
/// のみ既存経路として機能し、その他のスロットは Step 2 以降で書き込み経路を追加する。
///
/// | Slot | 書き込み先 | 色空間 |
/// |---|---|---|
/// | `BaseColor` | `IrMaterial.texture_index` + `base_color_tex_info` | sRGB |
/// | `Emissive` | `IrMaterial.emissive_texture` | sRGB |
/// | `Normal` | `IrMaterial.normal_texture` | **Linear (Unorm view)** |
/// | `ShadeMultiply` | `MtoonParams.shade_texture` | sRGB |
/// | `ShadingShift` | `MtoonParams.shading_shift_texture` | Linear |
/// | `RimMultiply` | `MtoonParams.rim_multiply_texture` | sRGB |
/// | `OutlineWidth` | `MtoonParams.outline_width_texture` | Linear |
/// | `Matcap` | `MtoonParams.matcap_texture` | sRGB |
/// | `UvAnimMask` | `MtoonParams.uv_animation_mask_texture` | Linear |
/// | `Sphere` / `Toon` | `sphere_texture_index` / `toon_texture_index` | sRGB (MMD 専用) |
///
/// `serde` 派生は Step 3 の `MaterialEditRecord` 永続化で `TextureSlotRecord<S>` の
/// キーとして使う前提で先行付与している。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextureSlot {
    BaseColor,
    Emissive,
    Normal,
    ShadeMultiply,
    ShadingShift,
    RimMultiply,
    OutlineWidth,
    Matcap,
    UvAnimMask,
    Sphere,
    Toon,
}

impl TextureSlot {
    /// `true` の場合、GPU へのアップロードは Linear / Unorm ビューを使用する。
    /// 通常値マップや UV マスクなど、sRGB デコードを挟んではいけないスロットを返す。
    ///
    /// このメソッドは `rebuild_mtoon_aux_bind_group` / `assign_texture_core` 側で
    /// sRGB / Unorm のビューを選択するために使われる（§B の判定表に対応）。
    pub fn is_linear(self) -> bool {
        matches!(
            self,
            Self::Normal | Self::ShadingShift | Self::OutlineWidth | Self::UvAnimMask
        )
    }
}

/// MToon 補助テクスチャ情報（texCoord + KHR_texture_transform + sampler）
#[derive(Clone, Debug)]
pub struct IrTextureInfo {
    pub index: usize,
    /// TEXCOORD セット番号（デフォルト 0）
    pub tex_coord: u32,
    /// KHR_texture_transform offset（デフォルト (0,0)）
    pub offset: Vec2,
    /// KHR_texture_transform scale（デフォルト (1,1)）
    pub scale: Vec2,
    /// KHR_texture_transform rotation（デフォルト 0）
    pub rotation: f32,
    /// glTF sampler 情報（デフォルト Repeat / Linear）
    pub sampler: IrSamplerInfo,
}

impl IrTextureInfo {
    /// テクスチャインデックスのみ指定（デフォルト texCoord=0, transform なし）
    pub fn from_index(index: usize) -> Self {
        Self {
            index,
            tex_coord: 0,
            offset: Vec2::ZERO,
            scale: Vec2::ONE,
            rotation: 0.0,
            sampler: IrSamplerInfo::default(),
        }
    }

    /// テクスチャインデックスをオフセットする（merge 時に使用）
    pub fn offset_index(&mut self, offset: usize) {
        self.index += offset;
    }

    /// テクスチャインデックスをリマップする（export_filter 時に使用）
    pub fn remap_index(self, remap: &std::collections::HashMap<usize, usize>) -> Option<Self> {
        remap.get(&self.index).map(|&new_idx| Self {
            index: new_idx,
            ..self
        })
    }
}

/// MToon アウトライン幅モード
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub enum OutlineWidthMode {
    #[default]
    None,
    WorldCoordinates,
    ScreenCoordinates,
}

/// glTF alphaMode + MToon transparentWithZWrite を統合したアルファモード
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum AlphaMode {
    /// 完全不透明（デプス書込あり）
    #[default]
    Opaque,
    /// alphaCutoff でピクセル単位切り抜き（デプス書込あり）
    Mask,
    /// 半透明・デプス書込あり（MToon transparentWithZWrite=true）
    BlendWithZWrite,
    /// 半透明・デプス書込なし（通常 BLEND）
    Blend,
}

/// ソースファイル形式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SourceFormat {
    #[default]
    Vrm1,
    Vrm0,
    Fbx,
    Pmx,
    Pmd,
    Obj,
    Stl,
    DirectX,
}

impl SourceFormat {
    pub fn label(&self) -> &str {
        match self {
            SourceFormat::Vrm1 => "VRM 1.0",
            SourceFormat::Vrm0 => "VRM 0.0",
            SourceFormat::Fbx => "FBX",
            SourceFormat::Pmx => "PMX",
            SourceFormat::Pmd => "PMD",
            SourceFormat::Obj => "OBJ",
            SourceFormat::Stl => "STL",
            SourceFormat::DirectX => "DirectX",
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
#[derive(Debug, Default, Clone)]
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
    /// PMX 変換向け軽量 clone。GPU 専用データ（mip_chain, uvs1）を除外する。
    /// vertices / indices / morph_targets は Arc 共有で O(1) clone。
    pub fn clone_for_export(&self) -> Self {
        Self {
            name: self.name.clone(),
            comment: self.comment.clone(),
            bones: self.bones.clone(),
            meshes: self
                .meshes
                .iter()
                .map(|m| IrMesh {
                    name: m.name.clone(),
                    vertices: Arc::clone(&m.vertices),
                    indices: Arc::clone(&m.indices),
                    material_index: m.material_index,
                    morph_targets: Arc::clone(&m.morph_targets),
                    node_index: m.node_index,
                    uvs1: Vec::new(), // PMX では不使用
                })
                .collect(),
            materials: self.materials.clone(),
            textures: self
                .textures
                .iter()
                .map(|t| IrTexture {
                    filename: t.filename.clone(),
                    data: t.data.clone(), // Arc 共有で軽量
                    mime_type: t.mime_type.clone(),
                    source_path: t.source_path.clone(),
                    mip_chain: None, // GPU 専用、PMX では不使用
                })
                .collect(),
            morphs: self.morphs.clone(),
            physics: self.physics.clone(),
            node_to_bone: self.node_to_bone.clone(),
            source_format: self.source_format,
            rig_type: self.rig_type.clone(),
            humanoid_bone_count: self.humanoid_bone_count,
            astance_result: self.astance_result,
        }
    }

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

        // ── 3段フォールバック候補決定 ──
        // candidate[i] = Some(self_idx) なら統合候補
        let mut candidate: Vec<Option<usize>> = vec![None; other_bone_count];
        // vrm_bone_name マッチは確定扱い（パス2の親伝播取り消し対象外）
        let mut is_vrm_match: Vec<bool> = vec![false; other_bone_count];

        // Pass 1a: vrm_bone_name 照合（最高信頼度、親チェック不要 — VRM名は全身で一意）
        let mut vrm_to_self: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for (i, bone) in self.bones.iter().enumerate() {
            if let Some(ref vrm) = bone.vrm_bone_name {
                vrm_to_self.entry(vrm.as_str()).or_insert(i);
            }
        }
        for (i, other_bone) in other.bones.iter().enumerate() {
            if let Some(ref vrm) = other_bone.vrm_bone_name {
                if let Some(&self_idx) = vrm_to_self.get(vrm.as_str()) {
                    candidate[i] = Some(self_idx);
                    is_vrm_match[i] = true;
                }
            }
        }

        // Pass 1b: original_name 照合（親整合性チェック付き）
        // to_lowercase() を事前キャッシュして繰り返し呼び出しを回避
        let self_lower_names: Vec<String> = self
            .bones
            .iter()
            .map(|b| b.original_name.to_lowercase())
            .collect();
        let other_lower_names: Vec<String> = other
            .bones
            .iter()
            .map(|b| b.original_name.to_lowercase())
            .collect();
        let mut orig_to_self: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for (i, key) in self_lower_names.iter().enumerate() {
            orig_to_self.entry(key.as_str()).or_insert(i);
        }
        for (i, other_bone) in other.bones.iter().enumerate() {
            if candidate[i].is_some() {
                continue;
            }
            let key = &other_lower_names[i];
            if let Some(&self_idx) = orig_to_self.get(key.as_str()) {
                let parent_ok = match (other_bone.parent, self.bones[self_idx].parent) {
                    (None, None) => true,
                    (Some(op), Some(sp)) => {
                        candidate[op] == Some(sp) || self_lower_names[sp] == other_lower_names[op]
                    }
                    _ => false,
                };
                if parent_ok {
                    candidate[i] = Some(self_idx);
                }
            }
        }

        // Pass 1c: bone.name 照合（既存ロジック — 後方互換）
        for (i, other_bone) in other.bones.iter().enumerate() {
            if candidate[i].is_some() {
                continue;
            }
            if let Some(&self_idx) = bone_name_to_self.get(other_bone.name.as_str()) {
                let self_parent_name = self.bones[self_idx]
                    .parent
                    .map(|p| self.bones[p].name.as_str());
                let other_parent_name = other_bone.parent.map(|p| other.bones[p].name.as_str());
                if self_parent_name == other_parent_name {
                    candidate[i] = Some(self_idx);
                }
            }
        }

        // パス2: 親の統合状態を伝播して最終決定（順序非依存）
        // 候補ボーンの親が候補でない場合は統合を取り消す（異なる部分木の子孫が誤統合されるのを防ぐ）
        // vrm_bone_name マッチは意味的に確定なので取り消し対象外
        let mut changed = true;
        while changed {
            changed = false;
            for i in 0..other_bone_count {
                if candidate[i].is_none() || is_vrm_match[i] {
                    continue;
                }
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
            if let Some(ref mut ti) = mat.base_color_tex_info {
                ti.offset_index(tex_offset);
            }
            if let Some(ref mut m) = mat.mtoon {
                if let Some(ref mut ti) = m.shade_texture {
                    ti.offset_index(tex_offset);
                }
                if let Some(ref mut ti) = m.outline_width_texture {
                    ti.offset_index(tex_offset);
                }
                if let Some(ref mut ti) = m.matcap_texture {
                    ti.offset_index(tex_offset);
                }
                if let Some(ref mut ti) = m.shading_shift_texture {
                    ti.offset_index(tex_offset);
                }
                if let Some(ref mut ti) = m.rim_multiply_texture {
                    ti.offset_index(tex_offset);
                }
                if let Some(ref mut ti) = m.uv_animation_mask_texture {
                    ti.offset_index(tex_offset);
                }
            }
            if let Some(ref mut ti) = mat.emissive_texture {
                ti.offset_index(tex_offset);
            }
            if let Some(ref mut ti) = mat.normal_texture {
                ti.offset_index(tex_offset);
            }
            if let Some(ref mut idx) = mat.sphere_texture_index {
                *idx += tex_offset;
            }
            if let Some(ref mut idx) = mat.toon_texture_index {
                *idx += tex_offset;
            }
        }
        self.materials.append(&mut other.materials);

        // ── メッシュ: 材質Index・頂点ウェイトのボーンIndexをリマップ ──
        for mesh in &mut other.meshes {
            mesh.material_index += mat_offset;
            mesh.node_index += node_offset;
            for vtx in mesh.vertices_mut() {
                for (bone_idx, _) in vtx.active_weights_mut() {
                    *bone_idx = bone_remap[*bone_idx];
                }
            }
        }
        self.meshes.append(&mut other.meshes);

        // ── モーフ: グローバル頂点Indexをオフセット ──
        for morph in &mut other.morphs {
            match &mut morph.kind {
                IrMorphKind::Vertex {
                    positions,
                    normals,
                    tangents,
                } => {
                    for (global_idx, _) in positions.iter_mut() {
                        *global_idx += vtx_offset;
                    }
                    for (global_idx, _) in normals.iter_mut() {
                        *global_idx += vtx_offset;
                    }
                    for (global_idx, _) in tangents.iter_mut() {
                        *global_idx += vtx_offset;
                    }
                }
                IrMorphKind::Group(entries) => {
                    for (morph_idx, _) in entries.iter_mut() {
                        *morph_idx += morph_offset;
                    }
                }
                IrMorphKind::Material {
                    color_binds,
                    uv_binds,
                } => {
                    for b in color_binds.iter_mut() {
                        b.material_index += mat_offset;
                    }
                    for b in uv_binds.iter_mut() {
                        b.material_index += mat_offset;
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
        self.physics
            .rigid_bodies
            .append(&mut other.physics.rigid_bodies);

        for joint in &mut other.physics.joints {
            joint.rigid_a += rigid_offset;
            joint.rigid_b += rigid_offset;
        }
        self.physics.joints.append(&mut other.physics.joints);

        // ── メタ情報更新 ──
        self.name = format!("{} + {}", self.name, other.name);
        // ヒューマノイドボーン数を再計算（共有ボーンへの補完分も含めるため）
        self.humanoid_bone_count = self
            .bones
            .iter()
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
            (AStanceResult::Applied(n), _) | (_, AStanceResult::Applied(n)) => {
                AStanceResult::Applied(n)
            }
            // 両方 NotFound
            (AStanceResult::NotFound, AStanceResult::NotFound) => AStanceResult::NotFound,
            // AlreadyAStance + NotFound → AlreadyAStance を優先
            (AStanceResult::AlreadyAStance, _) | (_, AStanceResult::AlreadyAStance) => {
                AStanceResult::AlreadyAStance
            }
        };

        (merged_count, new_bone_count)
    }

    /// 各材質に割り当てられたテクスチャ情報をログ出力する
    pub fn log_texture_assignments(&self) {
        // テクスチャ名 + ソースパスを表示するヘルパー
        let tex_label = |idx: usize| -> String {
            self.textures.get(idx).map_or("?".to_string(), |t| {
                if t.source_path.is_empty() {
                    format!("\"{}\"", t.filename)
                } else {
                    format!("\"{}\" ({})", t.filename, t.source_path)
                }
            })
        };
        let info_label = |info: &IrTextureInfo| -> String { tex_label(info.index) };

        log::info!("=== Texture assignments ===");
        for (i, mat) in self.materials.iter().enumerate() {
            let mut parts: Vec<String> = Vec::new();

            if let Some(idx) = mat.texture_index {
                parts.push(format!("base={}", tex_label(idx)));
            } else {
                parts.push("base=none".to_string());
            }
            if let Some(ref info) = mat.normal_texture {
                parts.push(format!("normal={}", info_label(info)));
            }
            if let Some(ref info) = mat.emissive_texture {
                parts.push(format!("emissive={}", info_label(info)));
            }
            if let Some(idx) = mat.sphere_texture_index {
                let mode = match mat.sphere_mode {
                    1 => "mul",
                    2 => "add",
                    _ => "?",
                };
                parts.push(format!("sphere={} [{}]", tex_label(idx), mode));
            }
            if let Some(idx) = mat.toon_texture_index {
                parts.push(format!("toon={}", tex_label(idx)));
            } else if let Some(shared) = mat.toon_shared_index {
                parts.push(format!("toon=shared(toon{:02}.bmp)", shared + 1));
            }

            if let Some(ref mtoon) = mat.mtoon {
                if let Some(ref info) = mtoon.shade_texture {
                    parts.push(format!("shade={}", info_label(info)));
                }
                if let Some(ref info) = mtoon.rim_multiply_texture {
                    parts.push(format!("rim={}", info_label(info)));
                }
                if let Some(ref info) = mtoon.matcap_texture {
                    parts.push(format!("matcap={}", info_label(info)));
                }
                if let Some(ref info) = mtoon.outline_width_texture {
                    parts.push(format!("outline={}", info_label(info)));
                }
                if let Some(ref info) = mtoon.shading_shift_texture {
                    parts.push(format!("shading_shift={}", info_label(info)));
                }
                if let Some(ref info) = mtoon.uv_animation_mask_texture {
                    parts.push(format!("uv_anim_mask={}", info_label(info)));
                }
            }

            log::info!("Material[{}] \"{}\": {}", i, mat.name, parts.join(", "));
        }
        log::info!(
            "=== Texture assignments done ({} materials) ===",
            self.materials.len()
        );
    }
}

/// 中間ボーン
#[derive(Debug, Clone)]
pub struct IrBone {
    pub name: String,
    pub name_en: String,
    /// ソースファイルでの元のボーン名（FBX: ノード名、VRM: glTFノード名）
    pub original_name: String,
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
    /// ボーンテイル位置（glTF座標系、PMX/PMDのみ）
    /// PMXの BoneTail（オフセットまたはボーンIndex）から計算した先端位置（レストポーズ）
    pub tail_position: Option<Vec3>,
    /// テイル先ボーンIndex（BoneTail::BoneIndex由来、アニメーション時の動的追従用）
    pub tail_bone_index: Option<usize>,
    /// IK影響下フラグ（IKの Target + Link に登録されているボーン）
    pub is_ik: bool,
    /// IKコントローラフラグ（PMX: BONE_FLAG_IK、PMD: bone_type==2）
    pub is_ik_bone: bool,
    /// 移動可能フラグ（PMX: BONE_FLAG_TRANSLATABLE、PMD: bone_type==1）
    pub is_translatable: bool,
    /// 軸制限フラグ（PMX: BONE_FLAG_AXIS_FIXED）
    pub is_axis_fixed: bool,
    /// 表示フラグ（PMX: BONE_FLAG_VISIBLE、PMD: bone_type!=7）
    pub is_visible: bool,
    /// 付与データ（PMX回転付与・移動付与）
    pub grant: Option<IrGrant>,
}

/// 付与データ（PMX回転付与・移動付与）
#[derive(Debug, Clone)]
pub struct IrGrant {
    /// 付与親ボーンIndex
    pub parent_index: usize,
    /// 付与率
    pub ratio: f32,
    /// 回転付与フラグ
    pub is_rotation: bool,
    /// 移動付与フラグ
    pub is_move: bool,
    /// ローカル付与フラグ
    pub is_local: bool,
}

/// 中間メッシュ
///
/// `vertices`, `indices`, `morph_targets` は `Arc` で共有され、
/// `clone` 時のコピーコストを O(1) に削減する。
/// mutation が必要な場合は `vertices_mut()` 等のヘルパーを使用する
/// （内部で `Arc::make_mut` による COW が行われる）。
#[derive(Debug, Clone)]
pub struct IrMesh {
    pub name: String,
    pub vertices: Arc<Vec<IrVertex>>,
    pub indices: Arc<Vec<u32>>,
    pub material_index: usize,
    /// モーフターゲット（頂点オフセット）
    pub morph_targets: Arc<Vec<IrMorphTarget>>,
    /// このメッシュが属するglTFノードIndex
    pub node_index: usize,
    /// TEXCOORD_1（セカンダリUV）。空なら UV1 なし。
    pub uvs1: Vec<[f32; 2]>,
}

impl IrMesh {
    /// COW mutable access to vertices.
    #[inline]
    pub fn vertices_mut(&mut self) -> &mut Vec<IrVertex> {
        Arc::make_mut(&mut self.vertices)
    }

    /// COW mutable access to indices.
    #[inline]
    pub fn indices_mut(&mut self) -> &mut Vec<u32> {
        Arc::make_mut(&mut self.indices)
    }

    /// COW mutable access to morph_targets.
    #[inline]
    pub fn morph_targets_mut(&mut self) -> &mut Vec<IrMorphTarget> {
        Arc::make_mut(&mut self.morph_targets)
    }
}

/// 中間頂点
#[derive(Debug, Clone, Copy)]
pub struct IrVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
    /// 接線ベクトル（xyz=tangent方向, w=handedness ±1）。
    /// glTF TANGENT 属性から読み込むか、MikkTSpace で生成する。
    pub tangent: Vec4,
    /// ボーンウェイト固定配列 (ボーンIndex, ウェイト)。有効要素数は weight_count。
    pub weights: [(usize, f32); 4],
    /// 有効なウェイト数 (0..=4)
    pub weight_count: u8,
    pub edge_scale: f32, // エッジ倍率（outlineWidthMultiplyTexture由来）
}

impl IrVertex {
    /// ウェイトの有効スライスを返す
    #[inline]
    pub fn active_weights(&self) -> &[(usize, f32)] {
        &self.weights[..self.weight_count as usize]
    }

    /// ウェイトの有効スライスを可変で返す
    #[inline]
    pub fn active_weights_mut(&mut self) -> &mut [(usize, f32)] {
        &mut self.weights[..self.weight_count as usize]
    }

    /// Vec からウェイトを設定する（最大4要素、超過分は切り捨て）
    pub fn set_weights_from_vec(&mut self, src: &[(usize, f32)]) {
        let n = src.len().min(4);
        self.weights = [(0, 0.0); 4];
        self.weights[..n].copy_from_slice(&src[..n]);
        self.weight_count = n as u8;
    }

    /// Vec<(usize, f32)> からウェイト付き IrVertex を構築するヘルパー
    pub fn from_weights(src: Vec<(usize, f32)>) -> ([(usize, f32); 4], u8) {
        let mut arr = [(0usize, 0.0f32); 4];
        let n = src.len().min(4);
        for (i, &val) in src.iter().take(4).enumerate() {
            arr[i] = val;
        }
        (arr, n as u8)
    }
}

/// モーフターゲット（メッシュ内）
#[derive(Debug, Clone)]
pub struct IrMorphTarget {
    pub name: String,
    /// 影響のある頂点の位置オフセット（疎表現: 頂点Index昇順）
    pub position_offsets: Vec<(u32, Vec3)>,
    /// 影響のある頂点の法線オフセット（疎表現: 頂点Index昇順）
    pub normal_offsets: Vec<(u32, Vec3)>,
    /// 影響のある頂点の接線オフセット（疎表現: 頂点Index昇順）
    pub tangent_offsets: Vec<(u32, Vec3)>,
}

/// カリングモード
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CullMode {
    /// 背面カリング（デフォルト、片面描画）
    Back,
    /// カリングなし（両面描画）
    None,
    /// 前面カリング（VRM 0.x _CullMode=1 用。glTF 仕様に存在しないため UniVRM では
    /// doubleSided=true にフォールバックするが、ランタイムレンダラでは再現可能）
    Front,
}

/// テクスチャマスクの参照カラーチャネル
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChannel {
    R,
    G,
    B,
}

impl ColorChannel {
    /// GPU uniform 用の f32 値（0.0=R, 1.0=G, 2.0=B）
    pub fn to_f32(self) -> f32 {
        match self {
            Self::R => 0.0,
            Self::G => 1.0,
            Self::B => 2.0,
        }
    }
}

/// シェーダー種別（Phase 3: 複数シェーダー検出対応）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShaderFamily {
    #[default]
    Other,
    Mtoon,
    Uts2,
    LilToon,
    Poiyomi,
}

impl std::fmt::Display for ShaderFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Other => f.write_str("-"),
            Self::Mtoon => f.write_str("MToon"),
            Self::Uts2 => f.write_str("UTS2"),
            Self::LilToon => f.write_str("lilToon"),
            Self::Poiyomi => f.write_str("Poiyomi"),
        }
    }
}

/// MToon シェーダー固有パラメータ
#[derive(Debug, Clone)]
pub struct MtoonParams {
    /// shadeColorFactor (デフォルト [0,0,0])
    pub shade_color: Option<Vec3>,
    /// shadeMultiplyTexture
    pub shade_texture: Option<IrTextureInfo>,
    /// shadingToonyFactor (0.0~1.0, 影境界の硬さ)
    pub shading_toony_factor: f32,
    /// shadingShiftFactor (-1.0~1.0, 影の閾値シフト)
    pub shading_shift_factor: f32,
    /// shadingShiftTexture (Rチャネル)
    pub shading_shift_texture: Option<IrTextureInfo>,
    /// shadingShiftTexture.scale (デフォルト 1.0)
    pub shading_shift_texture_scale: f32,
    /// アウトライン幅テクスチャ（glTFテクスチャIndex）
    /// VRM 1.0: outlineWidthMultiplyTexture (Gチャネル)
    /// VRM 0.0: _OutlineWidthTexture (Rチャネル)
    pub outline_width_texture: Option<IrTextureInfo>,
    /// outlineWidthTexture の参照チャネル（VRM 1.0=G, VRM 0.x=R）
    pub outline_width_tex_channel: ColorChannel,
    /// アウトライン幅モード（ビューア描画用）
    pub outline_width_mode: OutlineWidthMode,
    /// アウトライン幅の生値（world=メートル, screen=比率）
    pub outline_width_factor: f32,
    /// outlineLightingMixFactor (0.0=純色, 1.0=ライト混合)
    pub outline_lighting_mix: f32,
    /// parametricRimColorFactor (デフォルト [0,0,0])
    pub parametric_rim_color: Vec3,
    /// parametricRimFresnelPowerFactor (デフォルト 5.0)
    pub parametric_rim_fresnel_power: f32,
    /// parametricRimLiftFactor (デフォルト 0.0)
    pub parametric_rim_lift: f32,
    /// rimLightingMixFactor (0.0=放射, 1.0=ライト混合, デフォルト 1.0)
    pub rim_lighting_mix: f32,
    /// rimMultiplyTexture
    pub rim_multiply_texture: Option<IrTextureInfo>,
    /// giEqualizationFactor (0.0~1.0, GI均一化係数, デフォルト 0.9)
    pub gi_equalization_factor: f32,
    /// matcapFactor (デフォルト [1,1,1])
    pub matcap_factor: Vec3,
    /// matcapTexture
    pub matcap_texture: Option<IrTextureInfo>,
    /// uvAnimationScrollXSpeedFactor (デフォルト 0.0)
    pub uv_animation_scroll_x_speed: f32,
    /// uvAnimationScrollYSpeedFactor (デフォルト 0.0)
    pub uv_animation_scroll_y_speed: f32,
    /// uvAnimationRotationSpeedFactor (デフォルト 0.0)
    pub uv_animation_rotation_speed: f32,
    /// uvAnimationMaskTexture
    pub uv_animation_mask_texture: Option<IrTextureInfo>,
    /// uvAnimationMaskTexture の参照チャネル（VRM 1.0=B, VRM 0.x=R）
    pub uv_anim_mask_tex_channel: ColorChannel,
    /// renderQueueOffsetNumber（BLEND 内描画順オフセット）
    pub render_queue_offset: i32,
}

impl Default for MtoonParams {
    fn default() -> Self {
        Self {
            shade_color: None,
            shade_texture: None,
            shading_toony_factor: 0.9,
            shading_shift_factor: 0.0,
            shading_shift_texture: None,
            shading_shift_texture_scale: 1.0,
            outline_width_texture: None,
            outline_width_tex_channel: ColorChannel::G,
            outline_width_mode: OutlineWidthMode::None,
            outline_width_factor: 0.0,
            outline_lighting_mix: 1.0,
            parametric_rim_color: Vec3::ZERO,
            parametric_rim_fresnel_power: 5.0,
            parametric_rim_lift: 0.0,
            rim_lighting_mix: 1.0,
            rim_multiply_texture: None,
            gi_equalization_factor: 0.9,
            matcap_factor: Vec3::ONE,
            matcap_texture: None,
            uv_animation_scroll_x_speed: 0.0,
            uv_animation_scroll_y_speed: 0.0,
            uv_animation_rotation_speed: 0.0,
            uv_animation_mask_texture: None,
            uv_anim_mask_tex_channel: ColorChannel::B,
            render_queue_offset: 0,
        }
    }
}

/// 空の MtoonParams 定数（非MToon材質からのフィールドアクセス用）
static MTOON_DEFAULT: std::sync::LazyLock<MtoonParams> =
    std::sync::LazyLock::new(MtoonParams::default);

/// FBX 内のマテリアルのソース位置（renderer 階層パス + スロット番号）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceMaterialRef {
    /// renderer の階層パス（同名 sibling は ordinal 付き: "Root/Body[1]"）
    pub renderer_path: std::sync::Arc<str>,
    /// メッシュ内のマテリアルスロット番号
    pub slot_index: u16,
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
    /// ベースカラーテクスチャの texCoord + KHR_texture_transform 情報（ビューア描画用）
    pub base_color_tex_info: Option<IrTextureInfo>,
    /// カリングモード（Back=片面, None=両面, Front=前面カリング）
    pub cull_mode: CullMode,
    /// エッジ（輪郭線）
    pub edge_color: Vec4,
    pub edge_size: f32,
    /// MToon シェーダー固有パラメータ（None = 非MToon材質）
    pub mtoon: Option<MtoonParams>,
    /// シェーダー種別（MToon / UTS2 / Other）
    pub shader_family: ShaderFamily,
    /// FBX元テクスチャファイル名（一括割り当て用）
    pub source_texture_name: Option<String>,
    /// 材質の出自（RenderStyle 決定に使用）
    pub source_format: SourceFormat,
    /// スフィアマップテクスチャ
    pub sphere_texture_index: Option<usize>,
    /// スフィアモード: 0=無効, 1=乗算, 2=加算 (3=サブテクスチャは非対応)
    pub sphere_mode: u8,
    /// 個別トゥーンテクスチャIndex
    pub toon_texture_index: Option<usize>,
    /// 共有トゥーン番号 (0-9 = toon01-10)
    pub toon_shared_index: Option<u8>,
    /// アルファモード（glTF alphaMode + MToon transparentWithZWrite）
    pub alpha_mode: AlphaMode,
    /// MASK モード時のカットオフ閾値（glTF alphaCutoff, デフォルト 0.5）
    pub alpha_cutoff: f32,
    /// glTF emissiveFactor (デフォルト [0,0,0])
    pub emissive_factor: Vec3,
    /// glTF emissiveTexture
    pub emissive_texture: Option<IrTextureInfo>,
    /// glTF normalTexture
    pub normal_texture: Option<IrTextureInfo>,
    /// glTF normalTexture.scale (デフォルト 1.0)
    pub normal_texture_scale: f32,
    /// FBX 内のマテリアルソース位置（Prefab テクスチャマッピング用）
    pub source_material: Option<SourceMaterialRef>,
}

impl IrMaterial {
    /// MToon 材質かどうか
    pub fn is_mtoon(&self) -> bool {
        self.mtoon.is_some()
    }

    /// MToon パラメータへの参照（非MToon時はデフォルト値を返す）
    pub fn mtoon(&self) -> &MtoonParams {
        self.mtoon.as_ref().unwrap_or(&MTOON_DEFAULT)
    }

    /// MToon パラメータへの可変参照を取得（None の場合はデフォルト値で初期化）
    pub fn mtoon_mut(&mut self) -> &mut MtoonParams {
        self.mtoon.get_or_insert_with(MtoonParams::default)
    }

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
            base_color_tex_info: None,
            cull_mode: CullMode::Back,
            edge_color: Vec4::new(0.0, 0.0, 0.0, 1.0),
            edge_size: 0.0,
            mtoon: None,
            shader_family: ShaderFamily::Other,
            source_texture_name: None,
            source_format: SourceFormat::Vrm1,
            sphere_texture_index: None,
            sphere_mode: 0,
            toon_texture_index: None,
            toon_shared_index: None,
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            emissive_factor: Vec3::ZERO,
            emissive_texture: None,
            normal_texture: None,
            normal_texture_scale: 1.0,
            source_material: None,
        }
    }
}

/// テクスチャの実データ
#[derive(Debug, Clone)]
pub enum TextureData {
    /// PNG/JPEG/TGA 等のエンコード済みバイナリ
    Encoded(Arc<[u8]>),
    /// デコード済み生 RGBA ピクセル（GPU アップロード時にデコード不要）
    /// pixels は Arc で共有し、IrModel clone 時のコピーコストを排除する。
    RawRgba {
        pixels: Arc<[u8]>,
        width: u32,
        height: u32,
    },
}

impl TextureData {
    /// バイト列への参照を返す
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Encoded(v) => v,
            Self::RawRgba { pixels, .. } => pixels,
        }
    }

    /// データ長を返す
    pub fn len(&self) -> usize {
        self.as_bytes().len()
    }

    /// データが空かどうか
    pub fn is_empty(&self) -> bool {
        self.as_bytes().is_empty()
    }
}

/// 中間テクスチャ
#[derive(Debug, Clone)]
#[allow(clippy::type_complexity)]
pub struct IrTexture {
    /// ファイル名（���力時に使用）
    pub filename: String,
    /// テクスチャデータ（エンコード済みまたは生 RGBA）
    pub data: TextureData,
    /// MIME タイプ（エンコード済みデータのフォーマットヒント、ログ用）
    pub mime_type: String,
    /// ��クスチャの出自パス（トラブルシュート用ログ表示）
    /// embedded/アーカイブ内パス/外部ファイ���パス等
    pub source_path: String,
    /// ミップチェーン（レベル1以降のダウンサンプル済みRGBA）。
    /// バックグラウンドスレッドで事前生成することでメインスレッドの GPU 構築を高速化する。
    /// Vec<(width, height, RGBA bytes)>  — Arc で共有し clone コストを排除。
    pub mip_chain: Option<Vec<(u32, u32, Arc<[u8]>)>>,
}

impl IrTexture {
    /// `data` が生 RGBA バイト列（PNG デコード不要）かどうか
    pub fn is_raw_rgba(&self) -> bool {
        matches!(self.data, TextureData::RawRgba { .. })
    }

    /// 生 RGBA の場合の寸法を返す（後方互換ヘルパー）
    pub fn raw_dims(&self) -> Option<(u32, u32)> {
        match &self.data {
            TextureData::RawRgba { width, height, .. } => Some((*width, *height)),
            _ => None,
        }
    }
}

/// 拡張子から MIME タイプを返す（小文字の拡張子を期待）
pub fn mime_for_ext(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "tga" => "image/x-tga",
        "bmp" => "image/bmp",
        "dds" => "image/vnd.ms-dds",
        _ => "image/jpeg",
    }
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
    /// 頂点モーフ: position=(グローバル頂点Index, オフセット), normal/tangent も同形式
    Vertex {
        positions: Vec<(usize, Vec3)>,
        normals: Vec<(usize, Vec3)>,
        tangents: Vec<(usize, Vec3)>,
    },
    /// グループモーフ: (モーフIndex, 率)
    Group(Vec<(usize, f32)>),
    /// 材質モーフ: VRM 1.0 Expression の materialColorBinds / textureTransformBinds
    Material {
        color_binds: Vec<IrMaterialColorBind>,
        uv_binds: Vec<IrTextureTransformBind>,
    },
}

/// VRM 1.0 materialColorBind の対象プロパティ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialColorBindType {
    /// baseColorFactor → IrMaterial.diffuse
    Color,
    /// emissiveFactor → IrMaterial.emissive_factor
    EmissionColor,
    /// shadeColorFactor → MtoonParams.shade_color
    ShadeColor,
    /// matcapFactor → MtoonParams.matcap_factor
    MatcapColor,
    /// parametricRimColorFactor → MtoonParams.parametric_rim_color
    RimColor,
    /// outlineColorFactor → IrMaterial.edge_color
    OutlineColor,
}

impl MaterialColorBindType {
    /// VRM 1.0 Expression の `type` 文字列からパー���
    pub fn from_vrm_str(s: &str) -> Option<Self> {
        match s {
            "color" => Some(Self::Color),
            "emissionColor" => Some(Self::EmissionColor),
            "shadeColor" => Some(Self::ShadeColor),
            "matcapColor" => Some(Self::MatcapColor),
            "rimColor" => Some(Self::RimColor),
            "outlineColor" => Some(Self::OutlineColor),
            _ => None,
        }
    }
}

/// VRM 1.0 Expression の materialColorBind
#[derive(Debug, Clone)]
pub struct IrMaterialColorBind {
    pub material_index: usize,
    pub bind_type: MaterialColorBindType,
    pub target_value: [f32; 4],
}

/// VRM 1.0 Expression ��� textureTransformBind
#[derive(Debug, Clone)]
pub struct IrTextureTransformBind {
    pub material_index: usize,
    pub scale: [f32; 2],
    pub offset: [f32; 2],
}

/// 物理��報
#[derive(Debug, Default, Clone)]
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
            original_name: name.to_string(),
            vrm_bone_name: None,
            position: Vec3::ZERO,
            global_mat: Mat4::IDENTITY,
            parent,
            children,
            node_index: 0,
            is_physics: false,
            tail_position: None,
            tail_bone_index: None,
            is_ik: false,
            is_ik_bone: false,
            is_translatable: false,
            is_axis_fixed: false,
            is_visible: true,
            grant: None,
        }
    }

    /// テスト用ヘルパー: ウェイト付き頂点を含むメッシュ
    fn mesh_with_weights(name: &str, mat_idx: usize, bone_indices: &[usize]) -> IrMesh {
        let vertices: Vec<IrVertex> = bone_indices
            .iter()
            .map(|&bi| IrVertex {
                position: Vec3::ZERO,
                normal: Vec3::Y,
                uv: Vec2::ZERO,
                tangent: Vec4::new(1.0, 0.0, 0.0, 1.0),
                weights: [(bi, 1.0), (0, 0.0), (0, 0.0), (0, 0.0)],
                weight_count: 1,
                edge_scale: 1.0,
            })
            .collect();
        IrMesh {
            name: name.to_string(),
            vertices: vertices.into(),
            indices: Arc::new(vec![0, 1, 2]),
            material_index: mat_idx,
            morph_targets: Arc::new(Vec::new()),
            node_index: 0,
            uvs1: Vec::new(),
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
            materials: vec![IrMaterial {
                name: "mat_body".into(),
                ..Default::default()
            }],
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
            materials: vec![IrMaterial {
                name: "mat_costume".into(),
                ..Default::default()
            }],
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
        assert!(
            host.bones[1].children.contains(&3),
            "Spine の children に Ribbon(3) がない"
        );
        // Head(2) も残っている
        assert!(
            host.bones[1].children.contains(&2),
            "Spine の children に Head(2) がない"
        );

        // 衣装メッシュの頂点ウェイトが既存ボーンにリマップされていること
        let costume_mesh = &host.meshes[1];
        // other の Spine(idx=1) → self の Spine(idx=1)
        assert_eq!(
            costume_mesh.vertices[0].active_weights()[0].0,
            1,
            "Spine にリマップ"
        );
        // other の Ribbon(idx=2) → self の Ribbon(idx=3)
        assert_eq!(
            costume_mesh.vertices[1].active_weights()[0].0,
            3,
            "Ribbon にリマップ"
        );

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
                    group: 0,
                    no_collision_mask: 0xFFFF,
                    shape: RigidShape::Sphere { radius: 0.1 },
                    position: Vec3::ZERO,
                    rotation: Vec3::ZERO,
                    mass: 1.0,
                    linear_damping: 0.5,
                    angular_damping: 0.5,
                    restitution: 0.0,
                    friction: 0.5,
                    physics_mode: 0,
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
                    group: 1,
                    no_collision_mask: 0xFFFF,
                    shape: RigidShape::Sphere { radius: 0.05 },
                    position: Vec3::ZERO,
                    rotation: Vec3::ZERO,
                    mass: 0.5,
                    linear_damping: 0.5,
                    angular_damping: 0.5,
                    restitution: 0.0,
                    friction: 0.5,
                    physics_mode: 1,
                }],
                joints: vec![IrJoint {
                    name: "joint_new".into(),
                    rigid_a: 0, // other 側の最初の剛体（ただし実際はホスト側）
                    rigid_b: 0, // other 側の剛体 (rb_new)
                    position: Vec3::ZERO,
                    rotation: Vec3::ZERO,
                    move_limit_lo: Vec3::ZERO,
                    move_limit_hi: Vec3::ZERO,
                    rot_limit_lo: Vec3::ZERO,
                    rot_limit_hi: Vec3::ZERO,
                    spring_move: Vec3::ZERO,
                    spring_rot: Vec3::ZERO,
                }],
            },
            ..Default::default()
        };

        let (merged, new) = host.merge(other);
        assert_eq!(merged, 1); // Armature
        assert_eq!(new, 1); // NewBone

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
                name: "smile".into(),
                name_en: String::new(),
                panel: 3,
                kind: IrMorphKind::Vertex {
                    positions: vec![(0, Vec3::Y)],
                    normals: Vec::new(),
                    tangents: Vec::new(),
                },
            }],
            ..Default::default()
        };
        let other = IrModel {
            name: "Other".into(),
            meshes: vec![mesh_with_weights("m2", 0, &[0, 0])], // 2頂点
            bones: vec![bone("Root", None, vec![])],
            materials: vec![IrMaterial::default()],
            morphs: vec![IrMorph {
                name: "blink".into(),
                name_en: String::new(),
                panel: 2,
                kind: IrMorphKind::Vertex {
                    positions: vec![(1, Vec3::X)],
                    normals: Vec::new(),
                    tangents: Vec::new(),
                },
            }],
            ..Default::default()
        };

        host.merge(other);

        // other のモーフの頂点Index は +3 (host の頂点数) されるべき
        if let IrMorphKind::Vertex { ref positions, .. } = host.morphs[1].kind {
            assert_eq!(positions[0].0, 4, "vtx_offset=3, 元Index=1 → 4");
        } else {
            panic!("頂点モーフであるべき");
        }
    }

    /// merge で base_color_tex_info の index がテクスチャオフセット分加算されることを確認
    #[test]
    fn test_merge_base_color_tex_info_offset() {
        let mut host = IrModel {
            name: "Host".into(),
            bones: vec![bone("Root", None, vec![])],
            meshes: vec![mesh_with_weights("m0", 0, &[0, 0, 0])],
            materials: vec![IrMaterial {
                name: "mat0".into(),
                texture_index: Some(0),
                ..Default::default()
            }],
            textures: vec![IrTexture {
                filename: "host_tex.png".into(),
                data: TextureData::Encoded(Arc::from(vec![0u8])),
                mime_type: "image/png".into(),
                source_path: String::new(),
                mip_chain: None,
            }],
            ..Default::default()
        };

        let other = IrModel {
            name: "Other".into(),
            bones: vec![bone("Root", None, vec![])],
            meshes: vec![mesh_with_weights("m1", 0, &[0, 0, 0])],
            materials: vec![IrMaterial {
                name: "mat1".into(),
                texture_index: Some(0),
                base_color_tex_info: Some(IrTextureInfo {
                    index: 0,
                    tex_coord: 1,
                    offset: Vec2::new(0.1, 0.2),
                    scale: Vec2::new(2.0, 3.0),
                    rotation: 0.5,
                    sampler: IrSamplerInfo::default(),
                }),
                ..Default::default()
            }],
            textures: vec![IrTexture {
                filename: "other_tex.png".into(),
                data: TextureData::Encoded(Arc::from(vec![1u8])),
                mime_type: "image/png".into(),
                source_path: String::new(),
                mip_chain: None,
            }],
            ..Default::default()
        };

        host.merge(other);

        // テクスチャ2つ
        assert_eq!(host.textures.len(), 2);

        // other の材質の texture_index: 0 → 1 (host のテクスチャ数=1 でオフセット)
        let mat1 = &host.materials[1];
        assert_eq!(mat1.texture_index, Some(1));

        // base_color_tex_info の index も同様に 0 → 1
        let ti = mat1.base_color_tex_info.as_ref().unwrap();
        assert_eq!(
            ti.index, 1,
            "base_color_tex_info.index がオフセットされるべき"
        );
        // UV transform 情報は維持
        assert_eq!(ti.tex_coord, 1);
        assert!((ti.offset.x - 0.1).abs() < 1e-6);
        assert!((ti.scale.y - 3.0).abs() < 1e-6);
        assert!((ti.rotation - 0.5).abs() < 1e-6);
    }

    // ===== Step 7-36: TextureSlot::is_linear テスト =====

    #[test]
    fn test_texture_slot_is_linear() {
        // Linear スロット: 法線・ShadingShift・OutlineWidth・UvAnimMask
        assert!(TextureSlot::Normal.is_linear());
        assert!(TextureSlot::ShadingShift.is_linear());
        assert!(TextureSlot::OutlineWidth.is_linear());
        assert!(TextureSlot::UvAnimMask.is_linear());

        // sRGB スロット: それ以外すべて
        assert!(!TextureSlot::BaseColor.is_linear());
        assert!(!TextureSlot::Emissive.is_linear());
        assert!(!TextureSlot::ShadeMultiply.is_linear());
        assert!(!TextureSlot::RimMultiply.is_linear());
        assert!(!TextureSlot::Matcap.is_linear());
        assert!(!TextureSlot::Sphere.is_linear());
        assert!(!TextureSlot::Toon.is_linear());
    }

    #[test]
    fn test_texture_slot_all_variants_covered() {
        // 全 11 バリアントを列挙して is_linear が panic しないこと
        let all = [
            TextureSlot::BaseColor,
            TextureSlot::Emissive,
            TextureSlot::Normal,
            TextureSlot::ShadeMultiply,
            TextureSlot::ShadingShift,
            TextureSlot::RimMultiply,
            TextureSlot::OutlineWidth,
            TextureSlot::Matcap,
            TextureSlot::UvAnimMask,
            TextureSlot::Sphere,
            TextureSlot::Toon,
        ];
        let linear_count = all.iter().filter(|s| s.is_linear()).count();
        assert_eq!(linear_count, 4, "Linear スロットは 4 種");
        assert_eq!(all.len(), 11, "全 11 バリアント");
    }

    #[test]
    fn material_color_bind_type_from_vrm_str() {
        assert_eq!(
            MaterialColorBindType::from_vrm_str("color"),
            Some(MaterialColorBindType::Color)
        );
        assert_eq!(
            MaterialColorBindType::from_vrm_str("emissionColor"),
            Some(MaterialColorBindType::EmissionColor)
        );
        assert_eq!(
            MaterialColorBindType::from_vrm_str("shadeColor"),
            Some(MaterialColorBindType::ShadeColor)
        );
        assert_eq!(
            MaterialColorBindType::from_vrm_str("matcapColor"),
            Some(MaterialColorBindType::MatcapColor)
        );
        assert_eq!(
            MaterialColorBindType::from_vrm_str("rimColor"),
            Some(MaterialColorBindType::RimColor)
        );
        assert_eq!(
            MaterialColorBindType::from_vrm_str("outlineColor"),
            Some(MaterialColorBindType::OutlineColor)
        );
        assert_eq!(
            MaterialColorBindType::from_vrm_str("unknownType"),
            None,
            "Unknown type should return None"
        );
        assert_eq!(
            MaterialColorBindType::from_vrm_str(""),
            None,
            "Empty string should return None"
        );
    }

    #[test]
    fn merge_material_morph_offsets_material_index() {
        let mut host = IrModel::default();
        host.materials.push(IrMaterial::default());
        host.materials.push(IrMaterial::default());

        let mut guest = IrModel::default();
        guest.materials.push(IrMaterial::default());
        guest.morphs.push(IrMorph {
            name: "mat_morph".to_string(),
            name_en: "mat_morph".to_string(),
            panel: 4,
            kind: IrMorphKind::Material {
                color_binds: vec![IrMaterialColorBind {
                    material_index: 0,
                    bind_type: MaterialColorBindType::Color,
                    target_value: [1.0, 0.0, 0.0, 1.0],
                }],
                uv_binds: vec![IrTextureTransformBind {
                    material_index: 0,
                    scale: [2.0, 2.0],
                    offset: [0.5, 0.5],
                }],
            },
        });

        host.merge(guest);

        // host had 2 materials, so guest's material_index 0 should become 2
        if let IrMorphKind::Material {
            ref color_binds,
            ref uv_binds,
        } = host.morphs[0].kind
        {
            assert_eq!(color_binds[0].material_index, 2);
            assert_eq!(uv_binds[0].material_index, 2);
        } else {
            panic!("Expected Material morph kind");
        }
    }
}
