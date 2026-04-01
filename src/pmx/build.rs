use crate::error::{PoponeError, Result};
use glam::Vec3;
use std::collections::HashMap;
use std::f32::consts::PI;

use crate::convert::bone_map::vrm_bone_to_pmx_name;
use crate::convert::coord::{
    gltf_normal_to_pmx, gltf_normal_to_pmx_v0, gltf_pos_to_pmx, gltf_pos_to_pmx_v0,
};
use crate::convert::material::ir_material_to_pmx;
use crate::intermediate::types::{CullMode, IrModel, IrMorphKind, RigidShape};
use crate::pmx::types::*;

/// インデックスサイズ自動決定（頂点：符号なし）
pub fn vertex_idx_size(n: usize) -> u8 {
    if n <= 255 {
        1
    } else if n <= 65535 {
        2
    } else {
        4
    }
}

/// インデックスサイズ自動決定（その他：符号あり）
pub fn idx_size(n: usize) -> u8 {
    if n <= 127 {
        1
    } else if n <= 32767 {
        2
    } else {
        4
    }
}

/// PMXモデル構築オプション
#[derive(Debug, Clone, Default)]
pub struct PmxBuildOptions {
    /// 剛体回転をボーン方向に揃える
    pub align_rigid_rotation: bool,
    /// 物理（剛体・ジョイント）を出力しない
    pub no_physics: bool,
    /// 標準ボーン挿入をスキップ（元のボーン構造を維持）
    pub raw_structure: bool,
}

pub fn build_pmx_model(ir: &IrModel) -> Result<PmxModel> {
    build_pmx_model_with_options(ir, &PmxBuildOptions::default())
}

#[allow(clippy::field_reassign_with_default)]
pub fn build_pmx_model_with_options(ir: &IrModel, options: &PmxBuildOptions) -> Result<PmxModel> {
    log::info!("=== PMXモデル構築開始 ===");
    log::info!("モデル名: {}", ir.name);
    log::info!("ソース形式: {}", ir.source_format.label());

    // 入力VRM統計
    log::info!("入力VRM: ボーン={}, メッシュ={}, 頂点={}, 面={}, 材質={}, テクスチャ={}, モーフ={}, 剛体={}, ジョイント={}",
        ir.bones.len(), ir.meshes.len(), ir.total_vertices(), ir.total_faces(),
        ir.materials.len(), ir.textures.len(), ir.morphs.len(),
        ir.physics.rigid_bodies.len(), ir.physics.joints.len());

    // メッシュ詳細
    log::debug!("--- メッシュ一覧 ---");
    for (i, mesh) in ir.meshes.iter().enumerate() {
        log::debug!(
            "  [{:2}] 頂点={:5}, 面={:5}, 材質idx={}",
            i,
            mesh.vertices.len(),
            mesh.indices.len() / 3,
            mesh.material_index
        );
    }

    let mut model = PmxModel::default();

    // モデル情報
    model.model_info = PmxModelInfo {
        name: ir.name.clone(),
        name_en: ir.name.clone(),
        comment: ir.comment.clone(),
        comment_en: String::new(),
    };

    // テクスチャパス（textures\フォルダ相対、Windows区切り）
    model.textures = ir
        .textures
        .iter()
        .map(|t| format!("textures\\{}", t.filename))
        .collect();
    log::debug!("--- テクスチャ一覧 ---");
    for (i, tex) in ir.textures.iter().enumerate() {
        log::debug!(
            "  [{:2}] {} ({} {}bytes)",
            i,
            tex.filename,
            tex.mime_type,
            tex.data.len()
        );
    }

    // 材質 → テクスチャIndexマッピング
    let mat_to_tex: Vec<Option<i32>> = ir
        .materials
        .iter()
        .map(|m| m.texture_index.map(|i| i as i32))
        .collect();

    // 材質
    model.materials = ir
        .materials
        .iter()
        .enumerate()
        .map(|(i, m)| ir_material_to_pmx(m, mat_to_tex[i]))
        .collect();

    // 材質詳細ログ
    log::debug!("--- 材質一覧 ---");
    for (i, mat) in ir.materials.iter().enumerate() {
        log::debug!("  [{:2}] \"{}\" diffuse=({:.2},{:.2},{:.2},{:.2}) tex={:?} double={} mtoon={} edge={:.3}",
            i, mat.name,
            mat.diffuse.x, mat.diffuse.y, mat.diffuse.z, mat.diffuse.w,
            mat.texture_index, mat.cull_mode != CullMode::Back, mat.is_mtoon(), mat.edge_size);
    }
    let mtoon_count = ir.materials.iter().filter(|m| m.is_mtoon()).count();
    let double_count = ir
        .materials
        .iter()
        .filter(|m| m.cull_mode != CullMode::Back)
        .count();
    let edge_count = ir.materials.iter().filter(|m| m.edge_size > 0.0).count();
    log::info!(
        "材質: {}個 (MToon={}, 両面={}, エッジ有={})",
        ir.materials.len(),
        mtoon_count,
        double_count,
        edge_count
    );

    // ボーン変換
    model.bones = build_bones(ir, options.raw_structure);

    // 頂点・面 統合
    let (vertices, faces, mat_face_counts) =
        build_vertices_and_faces(ir, ir.source_format.is_vrm0());
    model.vertices = vertices;
    model.faces = faces;

    // 材質の面数設定
    for (i, mat) in model.materials.iter_mut().enumerate() {
        mat.face_count = mat_face_counts.get(i).copied().unwrap_or(0);
    }

    // 材質ごとの面数ログ
    log::debug!("--- 材質別面数 ---");
    for (i, mat) in model.materials.iter().enumerate() {
        log::debug!(
            "  [{:2}] \"{}\" 面頂点数={} (面数={})",
            i,
            mat.name,
            mat.face_count,
            mat.face_count / 3
        );
    }

    // モーフ変換
    model.morphs = build_morphs(ir, ir.source_format.is_vrm0());

    // 剛体・ジョイント
    if options.no_physics {
        log::info!("物理出力をスキップ（no_physics）");
    } else {
        model.rigid_bodies = build_rigid_bodies(ir, options.align_rigid_rotation);
        model.joints = build_joints(ir);
    }

    // 全データ揃った後に標準ボーン挿入（頂点・剛体・既存ボーンのindex調整もここで）
    if options.raw_structure {
        log::info!("標準ボーン挿入をスキップ（raw_structure）");
    } else {
        insert_standard_bones(&mut model)?;
    }

    // 重複ボーン名を解決（NameDupliBones 対策）
    fix_duplicate_names(&mut model.bones);

    // 変形順序に従い並び替え（IllegalOrderBones 対策）
    sort_bones_topological(&mut model);

    // ソート後の最終ボーン順序をログ出力
    log::debug!("=== ソート後ボーン一覧 ({} 本) ===", model.bones.len());
    for (i, b) in model.bones.iter().enumerate() {
        log::debug!(
            "  [{:3}] \"{}\" (parent={:3}, layer={}, flags=0x{:04X})",
            i,
            b.name,
            b.parent_index,
            b.deform_layer,
            b.flags
        );
    }

    // 表示枠はボーン挿入・ソート後（index確定後）
    model.display_frames = build_display_frames(&model.bones, &model.morphs);

    // 表示枠ログ
    log::debug!("--- 表示枠 ---");
    for (i, frame) in model.display_frames.iter().enumerate() {
        let bone_count = frame
            .elements
            .iter()
            .filter(|e| matches!(e, DisplayFrameElement::Bone(_)))
            .count();
        let morph_count = frame
            .elements
            .iter()
            .filter(|e| matches!(e, DisplayFrameElement::Morph(_)))
            .count();
        let special = if frame.is_special != 0 {
            " [特殊]"
        } else {
            ""
        };
        log::debug!(
            "  [{:1}] \"{}\" ボーン={}, モーフ={}{}",
            i,
            frame.name,
            bone_count,
            morph_count,
            special
        );
    }

    // 最終PMXモデル統計
    log::info!("=== PMXモデル構築完了 ===");
    log::info!("出力PMX: ボーン={}, 頂点={}, 面={}, 材質={}, テクスチャ={}, モーフ={}, 剛体={}, ジョイント={}, 表示枠={}",
        model.bones.len(), model.vertices.len(), model.faces.len(),
        model.materials.len(), model.textures.len(), model.morphs.len(),
        model.rigid_bodies.len(), model.joints.len(), model.display_frames.len());

    // ヘッダのインデックスサイズ自動決定
    model.header = PmxHeader {
        version: 2.0,
        encoding: 0, // UTF16LE
        additional_uvs: 0,
        vertex_index_size: vertex_idx_size(model.vertices.len()),
        texture_index_size: idx_size(model.textures.len()),
        material_index_size: idx_size(model.materials.len()),
        bone_index_size: idx_size(model.bones.len()),
        morph_index_size: idx_size(model.morphs.len()),
        rigid_body_index_size: idx_size(model.rigid_bodies.len()),
    };

    Ok(model)
}

fn find_bone_idx(bones: &[PmxBone], name: &str) -> Option<i32> {
    bones.iter().position(|b| b.name == name).map(|i| i as i32)
}

fn apply_remap(idx: i32, remap: &[i32]) -> i32 {
    if idx >= 0 && (idx as usize) < remap.len() {
        remap[idx as usize]
    } else {
        idx
    }
}

/// モデル内の全ボーン参照（ボーン間参照・頂点ウェイト・剛体）を一括リマップする。
///
/// `f` はボーンインデックスを受け取り、新しいインデックスを返すクロージャ。
/// 負値（-1 等）をスキップするかどうかはクロージャ側で制御する。
fn remap_all_bone_indices(model: &mut PmxModel, f: impl Fn(i32) -> i32) {
    for bone in &mut model.bones {
        bone.parent_index = f(bone.parent_index);
        if let BoneTail::BoneIndex(i) = &mut bone.tail {
            *i = f(*i);
        }
        if let Some(ik) = &mut bone.ik {
            ik.target_bone = f(ik.target_bone);
            for link in &mut ik.links {
                link.bone_index = f(link.bone_index);
            }
        }
        if let Some(g) = &mut bone.grant {
            g.parent_index = f(g.parent_index);
        }
    }
    for vtx in &mut model.vertices {
        match &mut vtx.weight {
            PmxWeightType::Bdef1 { bone } => {
                *bone = f(*bone);
            }
            PmxWeightType::Bdef2 { bone1, bone2, .. } => {
                *bone1 = f(*bone1);
                *bone2 = f(*bone2);
            }
            PmxWeightType::Bdef4 { bones, .. } => {
                for b in bones.iter_mut() {
                    *b = f(*b);
                }
            }
        }
    }
    for rb in &mut model.rigid_bodies {
        rb.bone_index = f(rb.bone_index);
    }
}

/// 骨をインデックス from → to へ移動し、モデル内の全骨参照・頂点ウェイト・剛体を更新する
fn move_bone_in_model(model: &mut PmxModel, from: usize, to: usize) {
    if from == to {
        return;
    }
    let n = model.bones.len();
    let mut remap: Vec<i32> = (0..n as i32).collect();
    if from < to {
        for (i, slot) in remap[(from + 1)..=to].iter_mut().enumerate() {
            *slot = (from + i) as i32;
        }
        remap[from] = to as i32;
    } else {
        for (i, slot) in remap[to..from].iter_mut().enumerate() {
            *slot = (to + i + 1) as i32;
        }
        remap[from] = to as i32;
    }

    remap_all_bone_indices(model, |idx| apply_remap(idx, &remap));

    let bone = model.bones.remove(from);
    model.bones.insert(to, bone);
}

fn insert_standard_bones(model: &mut PmxModel) -> Result<()> {
    log::debug!(
        "=== insert_standard_bones 開始 (既存ボーン数: {}) ===",
        model.bones.len()
    );

    // ボーン名 → インデックスの逆引きマップ（O(n) 線形探索を O(1) に最適化）
    // 重複名がある場合は最初の出現を保持（position() と同じセマンティクス）
    fn build_bone_map(bones: &[PmxBone]) -> HashMap<String, usize> {
        let mut map = HashMap::with_capacity(bones.len());
        for (i, b) in bones.iter().enumerate() {
            map.entry(b.name.clone()).or_insert(i);
        }
        map
    }

    let mut bone_map = build_bone_map(&model.bones);

    // 1. シフト前に位置・インデックスを取得
    let hips_y = bone_map
        .get("下半身")
        .map(|&i| model.bones[i].position.y)
        .unwrap_or(10.0);

    let l_ankle = bone_map
        .get("左足首")
        .map(|&i| model.bones[i].position)
        .unwrap_or(Vec3::new(-2.5, 2.0, 0.0));
    let r_ankle = bone_map
        .get("右足首")
        .map(|&i| model.bones[i].position)
        .unwrap_or(Vec3::new(2.5, 2.0, 0.0));

    let has_toes = bone_map.contains_key("左つま先") && bone_map.contains_key("右つま先");

    let l_toe = bone_map
        .get("左つま先")
        .map(|&i| model.bones[i].position)
        .unwrap_or(Vec3::new(l_ankle.x, l_ankle.y - 1.5, l_ankle.z + 3.0));
    let r_toe = bone_map
        .get("右つま先")
        .map(|&i| model.bones[i].position)
        .unwrap_or(Vec3::new(r_ankle.x, r_ankle.y - 1.5, r_ankle.z + 3.0));

    // [B-2] 腰ボーン位置（準標準プラグイン準拠: lerp(下半身.y, 右足.y, 0.6)）
    let r_leg_y = bone_map
        .get("右足")
        .map(|&i| model.bones[i].position.y)
        .unwrap_or(hips_y);
    let waist_y = hips_y * 0.4 + r_leg_y * 0.6;
    let waist_z = hips_y * 0.02;

    // 先頭に挿入するボーン総数（全ての親・センター・グルーブ・腰の4本のみ）
    // IKボーンは末尾に追加（あにまさ/ミクVer2準拠）
    let n = 4i32;

    log::debug!(
        "[step1] \"下半身\".y={:.3}, \"腰\"y={:.3}(z={:.3}), つま先あり={}",
        hips_y,
        waist_y,
        waist_z,
        has_toes
    );
    log::debug!(
        "[step1] 足首L=({:.3},{:.3},{:.3}), 足首R=({:.3},{:.3},{:.3})",
        l_ankle.x,
        l_ankle.y,
        l_ankle.z,
        r_ankle.x,
        r_ankle.y,
        r_ankle.z
    );
    log::debug!(
        "[step2] 標準ボーン追加数={} → 既存インデックスを+{}シフト",
        n,
        n
    );

    // 2,4,5. 既存ボーン・頂点ウェイト・剛体の全インデックスを +n シフト
    remap_all_bone_indices(model, |idx| if idx >= 0 { idx + n } else { idx });
    log::debug!(
        "[step2,4,5] 全ボーン参照・頂点ウェイト・剛体bone_indexを +{} シフト",
        n
    );

    // 3. 下半身・上半身の親を腰(index 3)に付け替え
    log::debug!("[step3] \"下半身\"・\"上半身\" の親 → \"腰\"(idx=3)");
    for bone in model.bones.iter_mut() {
        if bone.name == "下半身" || bone.name == "上半身" {
            bone.parent_index = 3;
        }
    }

    // 3.5 上半身のtailを上半身2に明示設定（children順序依存を排除してボーン方向を正す）
    // ※ここは連結前なのでVRM内Vec位置に+nして最終インデックスにする
    {
        let upper2_idx = bone_map.get("上半身2").map(|&i| i as i32);
        if let Some(idx) = upper2_idx {
            if let Some(b) = model.bones.iter_mut().find(|b| b.name == "上半身") {
                b.tail = BoneTail::BoneIndex(idx + n);
                b.flags |= BONE_FLAG_TAIL_IS_BONE;
                log::debug!("[step3.5] \"上半身\" tail → \"上半身2\"(idx={})", idx + n);
            }
        }
    }

    // 6. 標準ボーン4本（全ての親・センター・グルーブ・腰）を構築
    // IKボーンは末尾に追加（step18）
    let base_flags = BONE_FLAG_ROTATABLE | BONE_FLAG_VISIBLE | BONE_FLAG_OPERABLE;
    let trans_flags = base_flags | BONE_FLAG_TRANSLATABLE;

    let mut new_bones: Vec<PmxBone> = Vec::with_capacity(4);

    // 0: 全ての親
    new_bones.push(PmxBone {
        name: "全ての親".to_string(),
        name_en: "master".to_string(),
        position: Vec3::ZERO,
        parent_index: -1,
        deform_layer: 0,
        flags: trans_flags | BONE_FLAG_TAIL_IS_BONE,
        tail: BoneTail::BoneIndex(1),
        ik: None,
        grant: None,
    });

    // 1: センター
    new_bones.push(PmxBone {
        name: "センター".to_string(),
        name_en: "center".to_string(),
        position: Vec3::new(0.0, hips_y, 0.0),
        parent_index: 0,
        deform_layer: 0,
        flags: trans_flags | BONE_FLAG_TAIL_IS_BONE,
        tail: BoneTail::BoneIndex(2),
        ik: None,
        grant: None,
    });

    // 2: グルーブ
    new_bones.push(PmxBone {
        name: "グルーブ".to_string(),
        name_en: "groove".to_string(),
        position: Vec3::new(0.0, hips_y, 0.0),
        parent_index: 1,
        deform_layer: 0,
        flags: trans_flags,
        tail: BoneTail::Offset(Vec3::new(0.0, 2.0, 0.0)),
        ik: None,
        grant: None,
    });

    // 3: 腰（回転のみ、移動不可）
    new_bones.push(PmxBone {
        name: "腰".to_string(),
        name_en: "waist".to_string(),
        position: Vec3::new(0.0, waist_y, waist_z),
        parent_index: 2,
        deform_layer: 0,
        flags: base_flags,
        tail: BoneTail::Offset(Vec3::new(0.0, 2.0, 0.0)),
        ik: None,
        grant: None,
    });

    log::debug!("[step6] 標準ボーン{}本を構築:", new_bones.len());
    for (i, b) in new_bones.iter().enumerate() {
        log::debug!(
            "  [{:2}] \"{}\" pos=({:.3},{:.3},{:.3})",
            i,
            b.name,
            b.position.x,
            b.position.y,
            b.position.z
        );
    }

    // 既存ボーンを後ろに連結して置き換え
    new_bones.append(&mut model.bones);
    model.bones = new_bones;
    log::debug!("[step6] 既存ボーンを連結 → 合計{}本", model.bones.len());
    bone_map = build_bone_map(&model.bones);

    // 9. 上半身N群・首・頭・下半身をIK直後（index n）に配置
    // IK → 上半身 → 上半身2 → 上半身3（存在すれば）→ 首 → 頭 → 下半身 → … の順（ミクVer2準拠）
    log::debug!("[step9] 上半身群をIK直後(idx={})に整列", n);
    let mut next_target = n as usize;
    for name in ["上半身", "上半身2", "上半身3", "首", "頭"] {
        if let Some(&cur_idx) = bone_map.get(name) {
            if cur_idx != next_target {
                log::debug!("[step9]   \"{}\" {}番 → {}番", name, cur_idx, next_target);
                move_bone_in_model(model, cur_idx, next_target);
                bone_map = build_bone_map(&model.bones);
            }
            next_target += 1;
        }
    }
    if let Some(&cur_idx) = bone_map.get("下半身") {
        if cur_idx != next_target {
            log::debug!("[step9]   \"下半身\" {}番 → {}番", cur_idx, next_target);
            move_bone_in_model(model, cur_idx, next_target);
            bone_map = build_bone_map(&model.bones);
        }
    }

    // 10. 下半身ボーンを逆転させる
    // (1) positionとtailの絶対座標を入れ替える（ボーンが上→下向きになる）
    // (2) 親を腰に設定（確認）
    {
        let waist_idx = bone_map.get("腰").map(|&i| i as i32);
        let lower_idx = bone_map.get("下半身").copied();
        if let Some(li) = lower_idx {
            if let Some(wi) = waist_idx {
                model.bones[li].parent_index = wi;
            }
            let old_pos = model.bones[li].position;
            let tail_abs = match model.bones[li].tail.clone() {
                BoneTail::BoneIndex(ti) => model
                    .bones
                    .get(ti as usize)
                    .map(|b| b.position)
                    .unwrap_or(old_pos),
                BoneTail::Offset(off) => old_pos + off,
            };
            log::debug!(
                "[step10] \"下半身\"逆転: pos ({:.3},{:.3},{:.3}) ↔ tail ({:.3},{:.3},{:.3})",
                old_pos.x,
                old_pos.y,
                old_pos.z,
                tail_abs.x,
                tail_abs.y,
                tail_abs.z
            );
            model.bones[li].position = tail_abs;
            model.bones[li].tail = BoneTail::Offset(old_pos - tail_abs);
            model.bones[li].flags &= !BONE_FLAG_TAIL_IS_BONE;
        }
    }

    // 11. [B-1] 腰キャンセルボーン追加 → add_waist_cancel_bones()
    add_waist_cancel_bones(model)?;

    // 12-13. [C] 足Dボーン群 + 足先EX追加 → add_d_and_toe_ex_bones()
    add_d_and_toe_ex_bones(model, has_toes);

    // 14. [C] IK影響下ボーンの親をDボーンへ変更 → reparent_d_bone_children()
    reparent_d_bone_children(model);

    // step 15: 腕捩り・手捩りボーン追加
    log::debug!("=== [step15] 腕捩り・手捩りボーン追加 ===");
    add_twist_bones(model);
    log::debug!("=== [step15] 完了 ボーン数: {} ===", model.bones.len());

    // step 16: 肩キャンセルボーン追加
    log::debug!("=== [step16] 肩キャンセルボーン追加 ===");
    add_shoulder_cancel_bones(model)?;
    log::debug!("=== [step16] 完了 ボーン数: {} ===", model.bones.len());

    // step 17: IKボーン群を末尾に追加 → add_ik_bones()
    add_ik_bones(model, l_ankle, r_ankle, l_toe, r_toe, has_toes);
    log::debug!("=== [step17] 完了 ボーン数: {} ===", model.bones.len());

    // step 11〜17 でボーンが大幅に変更されたためマップを再構築
    bone_map = build_bone_map(&model.bones);

    // step 18: Dボーン群・足先EXをIKボーンの後（最末尾）に整列（あにまさ/ミクVer2準拠: 右→左順）
    // IKボーンが先に追加されているためDボーンはIKより高インデックスになり、ソート後もIK→Dの順が保たれる
    log::debug!("=== [step18] Dボーン群を末尾に整列（右→左） ===");
    {
        let d_end_order: &[&str] = if has_toes {
            &[
                "右足D",
                "右ひざD",
                "右足首D",
                "右足先EX",
                "左足D",
                "左ひざD",
                "左足首D",
                "左足先EX",
            ]
        } else {
            &["右足D", "右ひざD", "左足D", "左ひざD"]
        };
        for &name in d_end_order {
            if let Some(&cur_idx) = bone_map.get(name) {
                let last = model.bones.len() - 1;
                if cur_idx != last {
                    log::debug!("[step18] \"{}\" {}番 → {}番(末尾)", name, cur_idx, last);
                    move_bone_in_model(model, cur_idx, last);
                    bone_map = build_bone_map(&model.bones);
                }
            }
        }
    }
    log::debug!("=== [step18] 完了 ボーン数: {} ===", model.bones.len());

    // 最終ボーン一覧（全件）
    log::debug!("=== ボーン一覧 ({} 本) ===", model.bones.len());
    for (i, b) in model.bones.iter().enumerate() {
        log::debug!(
            "  [{:3}] \"{}\" (parent={:3}, layer={}, flags=0x{:04X})",
            i,
            b.name,
            b.parent_index,
            b.deform_layer,
            b.flags
        );
    }
    log::debug!("=== insert_standard_bones 完了 ===");
    Ok(())
}

/// 位置 insert_at にボーンを挿入した後、insert_at 以降の全参照を +1 シフトする
fn shift_indices_after_insert(model: &mut PmxModel, insert_at: usize) {
    let threshold = insert_at as i32;
    remap_all_bone_indices(model, |idx| if idx >= threshold { idx + 1 } else { idx });
}

/// 頂点を parent→child 方向に投影した値 t を [0,1] で返す（0=親側, 1=子側）
fn project_on_bone(vtx_pos: Vec3, start: Vec3, end: Vec3) -> f32 {
    let dir = end - start;
    let len_sq = dir.length_squared();
    if len_sq < 1e-6 {
        return 0.5;
    }
    ((vtx_pos - start).dot(dir) / len_sq).clamp(0.0, 1.0)
}

/// 親ボーン(arm_idx)のウェイトを投影値 t で捩りボーン(twist_idx)と分割する
fn redistribute_twist_weight(
    vertices: &mut [PmxVertex],
    parent_pos: Vec3,
    child_pos: Vec3,
    arm_idx: i32,
    twist_idx: i32,
) {
    for vtx in vertices.iter_mut() {
        let t = project_on_bone(vtx.position, parent_pos, child_pos);
        if t <= 0.01 {
            continue;
        }

        match &mut vtx.weight {
            PmxWeightType::Bdef1 { bone } => {
                if *bone != arm_idx {
                    continue;
                }
                // Bdef1{arm} → Bdef2{arm:1-t, twist:t}
                vtx.weight = PmxWeightType::Bdef2 {
                    bone1: arm_idx,
                    bone2: twist_idx,
                    weight1: 1.0 - t,
                };
            }
            PmxWeightType::Bdef2 {
                bone1,
                bone2,
                weight1,
            } => {
                let (w_arm, other_bone, w_other) = if *bone1 == arm_idx {
                    (*weight1, *bone2, 1.0 - *weight1)
                } else if *bone2 == arm_idx {
                    (1.0 - *weight1, *bone1, *weight1)
                } else {
                    continue;
                };
                // Bdef2{arm,other} → Bdef4{arm:w*(1-t), twist:w*t, other, -1:0}
                vtx.weight = PmxWeightType::Bdef4 {
                    bones: [arm_idx, twist_idx, other_bone, -1],
                    weights: [w_arm * (1.0 - t), w_arm * t, w_other, 0.0],
                };
            }
            PmxWeightType::Bdef4 { bones, weights } => {
                let Some(arm_slot) = bones.iter().position(|&b| b == arm_idx) else {
                    continue;
                };
                let w = weights[arm_slot];
                if w < 0.001 {
                    continue;
                }
                // 空きスロット = bone==-1 または weight≈0（arm_slot除く）
                let Some(empty) =
                    (0..4).find(|&i| i != arm_slot && (bones[i] == -1 || weights[i] < 1e-6))
                else {
                    continue; // 4本全使用 → スキップ
                };
                weights[arm_slot] = w * (1.0 - t);
                bones[empty] = twist_idx;
                weights[empty] = w * t;
            }
        }
    }
}

/// [step11] 腰キャンセルボーン（右・左）を追加し、右足/左足の直前に配置する
fn add_waist_cancel_bones(model: &mut PmxModel) -> Result<()> {
    let waist_idx = model
        .bones
        .iter()
        .position(|b| b.name == "腰")
        .map(|i| i as i32);
    let r_leg_info = model
        .bones
        .iter()
        .find(|b| b.name == "右足")
        .map(|b| (b.position, b.parent_index));
    let l_leg_info = model
        .bones
        .iter()
        .find(|b| b.name == "左足")
        .map(|b| (b.position, b.parent_index));

    if let (Some(waist_idx), Some((r_pos, r_parent)), Some((l_pos, l_parent))) =
        (waist_idx, r_leg_info, l_leg_info)
    {
        let cancel_flags = BONE_FLAG_ROTATABLE | BONE_FLAG_ROTATION_GRANT;

        log::debug!(
            "[step11] \"腰キャンセル右\" 追加 pos=({:.3},{:.3},{:.3})",
            r_pos.x,
            r_pos.y,
            r_pos.z
        );
        // 腰キャンセル右を末尾に追加し、右足の直前へ移動
        let r_cancel_at = model.bones.len();
        model.bones.push(PmxBone {
            name: "腰キャンセル右".to_string(),
            name_en: "waist cancel_R".to_string(),
            position: r_pos,
            parent_index: r_parent,
            deform_layer: 0,
            flags: cancel_flags,
            tail: BoneTail::Offset(Vec3::ZERO),
            ik: None,
            grant: Some(PmxGrant {
                parent_index: waist_idx,
                ratio: -1.0,
            }),
        });
        if let Some(b) = model.bones.iter_mut().find(|b| b.name == "右足") {
            b.parent_index = r_cancel_at as i32;
        }
        let r_leg_at = model
            .bones
            .iter()
            .position(|b| b.name == "右足")
            .ok_or_else(|| PoponeError::Build("ボーン「右足」が見つかりません".into()))?;
        move_bone_in_model(model, r_cancel_at, r_leg_at);

        log::debug!(
            "[step11] \"腰キャンセル左\" 追加 pos=({:.3},{:.3},{:.3})",
            l_pos.x,
            l_pos.y,
            l_pos.z
        );
        // 腰キャンセル左を末尾に追加し、左足の直前へ移動（右移動後のindexで）
        let waist_idx_now = model
            .bones
            .iter()
            .position(|b| b.name == "腰")
            .map(|i| i as i32)
            .unwrap_or(waist_idx);
        let l_parent_now = model
            .bones
            .iter()
            .find(|b| b.name == "左足")
            .map(|b| b.parent_index)
            .unwrap_or(l_parent);
        let l_cancel_at = model.bones.len();
        model.bones.push(PmxBone {
            name: "腰キャンセル左".to_string(),
            name_en: "waist cancel_L".to_string(),
            position: l_pos,
            parent_index: l_parent_now,
            deform_layer: 0,
            flags: cancel_flags,
            tail: BoneTail::Offset(Vec3::ZERO),
            ik: None,
            grant: Some(PmxGrant {
                parent_index: waist_idx_now,
                ratio: -1.0,
            }),
        });
        if let Some(b) = model.bones.iter_mut().find(|b| b.name == "左足") {
            b.parent_index = l_cancel_at as i32;
        }
        let l_leg_at = model
            .bones
            .iter()
            .position(|b| b.name == "左足")
            .ok_or_else(|| PoponeError::Build("ボーン「左足」が見つかりません".into()))?;
        move_bone_in_model(model, l_cancel_at, l_leg_at);
    }
    Ok(())
}

/// [step12-13] 足Dボーン群（IK影響下のD補助ボーン）と足先EXボーンを追加する
fn add_d_and_toe_ex_bones(model: &mut PmxModel, has_toes: bool) {
    // 12. [C] 足Dボーン群（IK影響下のD補助ボーン）
    // 各IKリンクボーン(a)を複製し回転付与(×1.0)で追従するD補助を作る。
    // 元ボーン(a)の親子関係は一切変更しない。
    // DボーンのみがDボーン同士の独自チェーンを形成する:
    //   親ボーンに対応するDボーンが既に存在する場合はそれを親とする。
    {
        let d_pairs: &[(&str, &str, &str)] = if has_toes {
            &[
                ("左足", "左足D", "leg_LD"),
                ("左ひざ", "左ひざD", "knee_LD"),
                ("右足", "右足D", "leg_RD"),
                ("右ひざ", "右ひざD", "knee_RD"),
                ("左足首", "左足首D", "ankle_LD"),
                ("右足首", "右足首D", "ankle_RD"),
            ]
        } else {
            &[
                ("左足", "左足D", "leg_LD"),
                ("左ひざ", "左ひざD", "knee_LD"),
                ("右足", "右足D", "leg_RD"),
                ("右ひざ", "右ひざD", "knee_RD"),
            ]
        };

        for &(src_name, d_name, d_en) in d_pairs {
            let Some(src_idx) = find_bone_idx(&model.bones, src_name) else {
                continue;
            };
            let src_pos = model.bones[src_idx as usize].position;
            let src_parent = model.bones[src_idx as usize].parent_index;

            // D補助の親: 元ボーンの親に対応するDボーンが既にあればそれを使う
            // （例: 左ひざDの親 → 左足の親名"左足"+"D"="左足D" が存在 → 左足D）
            let d_parent = if src_parent >= 0 {
                let parent_d_name = format!("{}D", &model.bones[src_parent as usize].name);
                find_bone_idx(&model.bones, &parent_d_name).unwrap_or(src_parent)
            } else {
                src_parent
            };

            log::debug!(
                "[step12] \"{}\"追加 pos=({:.3},{:.3},{:.3}) grant←\"{}\"(idx={})",
                d_name,
                src_pos.x,
                src_pos.y,
                src_pos.z,
                src_name,
                src_idx
            );
            // D補助を末尾に追加（step17で末尾に整列）
            model.bones.push(PmxBone {
                name: d_name.to_string(),
                name_en: d_en.to_string(),
                position: src_pos,
                parent_index: d_parent,
                deform_layer: 1,
                flags: BONE_FLAG_ROTATABLE | BONE_FLAG_ROTATION_GRANT,
                tail: BoneTail::Offset(Vec3::ZERO),
                ik: None,
                grant: Some(PmxGrant {
                    parent_index: src_idx,
                    ratio: 1.0,
                }),
            });
        }
    }

    // 13. 足先EX追加（左足首D / 右足首Dの直後）
    // 足先EXの親は足首D（IK影響下ボーン「足首」のDボーン）とする。
    // 左つま先 / 右つま先の親は変更しない（ミク準拠: つま先の親は足首のまま）。
    if has_toes {
        for (ex_name, ex_en, parent_d) in [
            ("左足先EX", "ex toe_L", "左足首D"),
            ("右足先EX", "ex toe_R", "右足首D"),
        ] {
            let Some(parent_idx) = find_bone_idx(&model.bones, parent_d) else {
                continue;
            };
            let pos = model.bones[parent_idx as usize].position;
            log::debug!(
                "[step13] \"{}\"追加 pos=({:.3},{:.3},{:.3}) parent=\"{}\"(idx={})",
                ex_name,
                pos.x,
                pos.y,
                pos.z,
                parent_d,
                parent_idx
            );

            model.bones.push(PmxBone {
                name: ex_name.to_string(),
                name_en: ex_en.to_string(),
                position: pos,
                parent_index: parent_idx,
                deform_layer: 1,
                flags: BONE_FLAG_ROTATABLE | BONE_FLAG_VISIBLE | BONE_FLAG_OPERABLE,
                tail: BoneTail::Offset(Vec3::new(0.0, -1.0, 0.0)),
                ik: None,
                grant: None,
            });
            // （step17で末尾に整列）
        }
    }
}

/// [step14] IK影響下ボーン（足・ひざ・足首）を親に持つ補助ボーンの親をDボーンへ変更し、
/// 変形階層を子孫へ再帰的に伝播する
fn reparent_d_bone_children(model: &mut PmxModel) {
    let remap_pairs: &[(&str, &str)] = &[
        ("左足", "左足D"),
        ("左ひざ", "左ひざD"),
        ("左足首", "左足首D"),
        ("右足", "右足D"),
        ("右ひざ", "右ひざD"),
        ("右足首", "右足首D"),
    ];

    let exclude: &[&str] = &[
        "左足",
        "左ひざ",
        "左足首",
        "左つま先",
        "右足",
        "右ひざ",
        "右足首",
        "右つま先",
        "左足D",
        "左ひざD",
        "左足首D",
        "右足D",
        "右ひざD",
        "右足首D",
        "左足先EX",
        "右足先EX",
    ];

    // 変形階層が実際に変化したボーンのインデックスを記録
    let mut changed: std::collections::HashSet<usize> = std::collections::HashSet::new();

    for &(src_name, d_name) in remap_pairs {
        let Some(src_idx) = find_bone_idx(&model.bones, src_name) else {
            continue;
        };
        let Some(d_idx) = find_bone_idx(&model.bones, d_name) else {
            continue;
        };

        for (i, bone) in model.bones.iter_mut().enumerate() {
            if exclude.contains(&bone.name.as_str()) {
                continue;
            }
            if bone.parent_index == src_idx {
                bone.parent_index = d_idx;
                let old_layer = bone.deform_layer;
                let new_layer = bone.deform_layer.max(1);
                if new_layer != old_layer {
                    bone.deform_layer = new_layer;
                    changed.insert(i);
                    log::debug!(
                        "[step14] \"{}\" 親変更: \"{}\"(idx={}) → \"{}\"(idx={}), layer {} → {}",
                        bone.name,
                        src_name,
                        src_idx,
                        d_name,
                        d_idx,
                        old_layer,
                        new_layer
                    );
                } else {
                    log::debug!("[step14] \"{}\" 親変更: \"{}\"(idx={}) → \"{}\"(idx={}), layer {} (変更なし)",
                        bone.name, src_name, src_idx, d_name, d_idx, bone.deform_layer);
                }
            }
        }
    }

    // 変形階層を変更したボーンの子孫へ再帰的に伝播（親 → 子 → 孫 ...）
    loop {
        let mut any_updated = false;
        for i in 0..model.bones.len() {
            let parent_idx = model.bones[i].parent_index;
            if parent_idx < 0 {
                continue;
            }
            if changed.contains(&(parent_idx as usize)) {
                let parent_layer = model.bones[parent_idx as usize].deform_layer;
                if model.bones[i].deform_layer < parent_layer {
                    let old_layer = model.bones[i].deform_layer;
                    let bone_name = model.bones[i].name.clone();
                    let parent_name = model.bones[parent_idx as usize].name.clone();
                    model.bones[i].deform_layer = parent_layer;
                    changed.insert(i);
                    any_updated = true;
                    log::debug!(
                        "[step14] deform_layer伝播: \"{}\" {} → {}（親: \"{}\"）",
                        bone_name,
                        old_layer,
                        parent_layer,
                        parent_name
                    );
                }
            }
        }
        if !any_updated {
            break;
        }
    }
}

/// [step17] IKボーン群（足IK親・足ＩＫ・つま先ＩＫ・ＩＫ先）を末尾に追加する
fn add_ik_bones(
    model: &mut PmxModel,
    l_ankle: Vec3,
    r_ankle: Vec3,
    l_toe: Vec3,
    r_toe: Vec3,
    has_toes: bool,
) {
    log::debug!("=== [step17] IKボーン群を末尾に追加 ===");

    let ik_bone_flags = BONE_FLAG_ROTATABLE
        | BONE_FLAG_VISIBLE
        | BONE_FLAG_OPERABLE
        | BONE_FLAG_IK
        | BONE_FLAG_TRANSLATABLE;
    let trans_flags_ik =
        BONE_FLAG_ROTATABLE | BONE_FLAG_VISIBLE | BONE_FLAG_OPERABLE | BONE_FLAG_TRANSLATABLE;

    // 全移動完了後の現在インデックスを取得
    let l_ankle_fi = find_bone_idx(&model.bones, "左足首");
    let r_ankle_fi = find_bone_idx(&model.bones, "右足首");
    let l_knee_fi = find_bone_idx(&model.bones, "左ひざ");
    let r_knee_fi = find_bone_idx(&model.bones, "右ひざ");
    let l_leg_fi = find_bone_idx(&model.bones, "左足");
    let r_leg_fi = find_bone_idx(&model.bones, "右足");
    let l_toe_fi = find_bone_idx(&model.bones, "左つま先");
    let r_toe_fi = find_bone_idx(&model.bones, "右つま先");

    // 追加ボーンの配置インデックスを事前計算
    // 左→右順: 左足IK親(+0), 左足ＩＫ(+1), 右足IK親(+2), 右足ＩＫ(+3)
    //          [has_toes] 左つま先ＩＫ(+4), 右つま先ＩＫ(+5)
    // ＩＫ先ボーン: 左足ＩＫ先, 右足ＩＫ先 [, 左つま先ＩＫ先, 右つま先ＩＫ先]
    let base = model.bones.len() as i32;
    let l_ik_parent_idx = base;
    let l_ik_idx = base + 1;
    let r_ik_parent_idx = base + 2;
    let r_ik_idx = base + 3;
    let (l_toe_ik_idx, r_toe_ik_idx, ik_tail_base) = if has_toes {
        (base + 4, base + 5, base + 6)
    } else {
        (-1, -1, base + 4)
    };
    let l_ik_tail_idx = ik_tail_base;
    let r_ik_tail_idx = ik_tail_base + 1;
    let l_toe_ik_tail_idx = ik_tail_base + 2;
    let r_toe_ik_tail_idx = ik_tail_base + 3;

    // IKデータ構築（全移動後インデックスを直接参照）
    let l_leg_ik = l_ankle_fi.map(|target| {
        let mut links = Vec::new();
        if let Some(ki) = l_knee_fi {
            links.push(IkLink {
                bone_index: ki,
                angle_limit: true,
                limit_min: Vec3::new(-PI, 0.0, 0.0),
                limit_max: Vec3::new(-0.005, 0.0, 0.0),
            });
        }
        if let Some(li) = l_leg_fi {
            links.push(IkLink {
                bone_index: li,
                angle_limit: false,
                limit_min: Vec3::ZERO,
                limit_max: Vec3::ZERO,
            });
        }
        PmxIk {
            target_bone: target,
            loop_count: 40,
            limit_angle: 2.0,
            links,
        }
    });
    let r_leg_ik = r_ankle_fi.map(|target| {
        let mut links = Vec::new();
        if let Some(ki) = r_knee_fi {
            links.push(IkLink {
                bone_index: ki,
                angle_limit: true,
                limit_min: Vec3::new(-PI, 0.0, 0.0),
                limit_max: Vec3::new(-0.005, 0.0, 0.0),
            });
        }
        if let Some(li) = r_leg_fi {
            links.push(IkLink {
                bone_index: li,
                angle_limit: false,
                limit_min: Vec3::ZERO,
                limit_max: Vec3::ZERO,
            });
        }
        PmxIk {
            target_bone: target,
            loop_count: 40,
            limit_angle: 2.0,
            links,
        }
    });
    let l_toe_ik = if has_toes {
        l_toe_fi.map(|target| {
            let mut links = Vec::new();
            if let Some(ai) = l_ankle_fi {
                links.push(IkLink {
                    bone_index: ai,
                    angle_limit: false,
                    limit_min: Vec3::ZERO,
                    limit_max: Vec3::ZERO,
                });
            }
            PmxIk {
                target_bone: target,
                loop_count: 3,
                limit_angle: 4.0,
                links,
            }
        })
    } else {
        None
    };
    let r_toe_ik = if has_toes {
        r_toe_fi.map(|target| {
            let mut links = Vec::new();
            if let Some(ai) = r_ankle_fi {
                links.push(IkLink {
                    bone_index: ai,
                    angle_limit: false,
                    limit_min: Vec3::ZERO,
                    limit_max: Vec3::ZERO,
                });
            }
            PmxIk {
                target_bone: target,
                loop_count: 3,
                limit_angle: 4.0,
                links,
            }
        })
    } else {
        None
    };

    // 左足IK親
    model.bones.push(PmxBone {
        name: "左足IK親".to_string(),
        name_en: "leg IK parent_L".to_string(),
        position: Vec3::new(l_ankle.x, 0.0, 0.0),
        parent_index: 0,
        deform_layer: 0,
        flags: trans_flags_ik | BONE_FLAG_TAIL_IS_BONE,
        tail: BoneTail::BoneIndex(l_ik_idx),
        ik: None,
        grant: None,
    });
    // 左足ＩＫ（tail → 左足ＩＫ先）
    model.bones.push(PmxBone {
        name: "左足ＩＫ".to_string(),
        name_en: "leg IK_L".to_string(),
        position: l_ankle,
        parent_index: l_ik_parent_idx,
        deform_layer: 1,
        flags: ik_bone_flags | BONE_FLAG_TAIL_IS_BONE,
        tail: BoneTail::BoneIndex(l_ik_tail_idx),
        ik: l_leg_ik,
        grant: None,
    });
    // 右足IK親
    model.bones.push(PmxBone {
        name: "右足IK親".to_string(),
        name_en: "leg IK parent_R".to_string(),
        position: Vec3::new(r_ankle.x, 0.0, 0.0),
        parent_index: 0,
        deform_layer: 0,
        flags: trans_flags_ik | BONE_FLAG_TAIL_IS_BONE,
        tail: BoneTail::BoneIndex(r_ik_idx),
        ik: None,
        grant: None,
    });
    // 右足ＩＫ（tail → 右足ＩＫ先）
    model.bones.push(PmxBone {
        name: "右足ＩＫ".to_string(),
        name_en: "leg IK_R".to_string(),
        position: r_ankle,
        parent_index: r_ik_parent_idx,
        deform_layer: 1,
        flags: ik_bone_flags | BONE_FLAG_TAIL_IS_BONE,
        tail: BoneTail::BoneIndex(r_ik_tail_idx),
        ik: r_leg_ik,
        grant: None,
    });

    if has_toes {
        // 左つま先ＩＫ（tail → 左つま先ＩＫ先）
        model.bones.push(PmxBone {
            name: "左つま先ＩＫ".to_string(),
            name_en: "toe IK_L".to_string(),
            position: l_toe,
            parent_index: l_ik_idx,
            deform_layer: 1,
            flags: ik_bone_flags | BONE_FLAG_TAIL_IS_BONE,
            tail: BoneTail::BoneIndex(l_toe_ik_tail_idx),
            ik: l_toe_ik,
            grant: None,
        });
        // 右つま先ＩＫ（tail → 右つま先ＩＫ先）
        model.bones.push(PmxBone {
            name: "右つま先ＩＫ".to_string(),
            name_en: "toe IK_R".to_string(),
            position: r_toe,
            parent_index: r_ik_idx,
            deform_layer: 1,
            flags: ik_bone_flags | BONE_FLAG_TAIL_IS_BONE,
            tail: BoneTail::BoneIndex(r_toe_ik_tail_idx),
            ik: r_toe_ik,
            grant: None,
        });
    }

    // ＩＫ先ボーン（表示用tail・非表示非操作）
    model.bones.push(PmxBone {
        name: "左足ＩＫ先".to_string(),
        name_en: "leg IK tail_L".to_string(),
        position: l_ankle + Vec3::new(0.0, 0.0, 1.0),
        parent_index: l_ik_idx,
        deform_layer: 1,
        flags: 0,
        tail: BoneTail::Offset(Vec3::ZERO),
        ik: None,
        grant: None,
    });
    model.bones.push(PmxBone {
        name: "右足ＩＫ先".to_string(),
        name_en: "leg IK tail_R".to_string(),
        position: r_ankle + Vec3::new(0.0, 0.0, 1.0),
        parent_index: r_ik_idx,
        deform_layer: 1,
        flags: 0,
        tail: BoneTail::Offset(Vec3::ZERO),
        ik: None,
        grant: None,
    });
    if has_toes {
        model.bones.push(PmxBone {
            name: "左つま先ＩＫ先".to_string(),
            name_en: "toe IK tail_L".to_string(),
            position: l_toe + Vec3::new(0.0, -1.0, 0.0),
            parent_index: l_toe_ik_idx,
            deform_layer: 1,
            flags: 0,
            tail: BoneTail::Offset(Vec3::ZERO),
            ik: None,
            grant: None,
        });
        model.bones.push(PmxBone {
            name: "右つま先ＩＫ先".to_string(),
            name_en: "toe IK tail_R".to_string(),
            position: r_toe + Vec3::new(0.0, -1.0, 0.0),
            parent_index: r_toe_ik_idx,
            deform_layer: 1,
            flags: 0,
            tail: BoneTail::Offset(Vec3::ZERO),
            ik: None,
            grant: None,
        });
    }
    log::debug!(
        "[step17] IK+ＩＫ先ボーン追加 → ボーン数: {}",
        model.bones.len()
    );
}

/// 腕捩り・手捩りボーン（4本）を追加し、ウェイトを再配分する
fn add_twist_bones(model: &mut PmxModel) {
    let pairs = [
        ("右腕", "右ひじ", "右腕捩", "arm twist_R"),
        ("左腕", "左ひじ", "左腕捩", "arm twist_L"),
        ("右ひじ", "右手首", "右手捩", "wrist twist_R"),
        ("左ひじ", "左手首", "左手捩", "wrist twist_L"),
    ];
    let base_flags = BONE_FLAG_ROTATABLE | BONE_FLAG_VISIBLE | BONE_FLAG_OPERABLE;

    for (parent_name, child_name, twist_jp, twist_en) in pairs {
        // 1. 最新の bones から親・子インデックスと位置を取得
        let Some(parent_idx) = find_bone_idx(&model.bones, parent_name) else {
            log::warn!("[step15] \"{}\" が見つからないためスキップ", parent_name);
            continue;
        };
        let Some(child_idx) = find_bone_idx(&model.bones, child_name) else {
            log::warn!("[step15] \"{}\" が見つからないためスキップ", child_name);
            continue;
        };
        let parent_pos = model.bones[parent_idx as usize].position;
        let child_pos = model.bones[child_idx as usize].position;
        let parent_layer = model.bones[parent_idx as usize].deform_layer;

        // 2. 捩りボーン位置 = 中間点
        let twist_pos = parent_pos.lerp(child_pos, 0.5);

        log::debug!(
            "[step15] \"{}\" 追加 pos=({:.3},{:.3},{:.3}) parent=\"{}\"({})",
            twist_jp,
            twist_pos.x,
            twist_pos.y,
            twist_pos.z,
            parent_name,
            parent_idx
        );

        // 3. 捩りボーン生成（親=parent_idx、子なし・grant なし）
        let twist_bone = PmxBone {
            name: twist_jp.to_string(),
            name_en: twist_en.to_string(),
            position: twist_pos,
            parent_index: parent_idx,
            deform_layer: parent_layer,
            flags: base_flags,
            tail: BoneTail::Offset(Vec3::ZERO),
            ik: None,
            grant: None,
        };

        // 4. 親ボーンの直後に挿入
        let insert_at = parent_idx as usize + 1;
        model.bones.insert(insert_at, twist_bone);

        // 5. 挿入によって insert_at 以降の全参照を +1 シフト
        //    （新規挿入ボーン自身の parent_index は parent_idx < insert_at なので不変）
        shift_indices_after_insert(model, insert_at);

        // 6. ウェイト再配分
        //    シフト後も親ボーンは parent_idx のまま（insert_at より前なので不変）
        //    捩りボーンは insert_at にある
        redistribute_twist_weight(
            &mut model.vertices,
            parent_pos,
            child_pos,
            parent_idx,       // arm_idx: シフト後も変わらず parent_idx
            insert_at as i32, // twist_idx
        );
    }
}

/// 肩キャンセルボーン（肩P/肩C）を左右に追加する
/// 肩P: 肩の親になり、ユーザーが操作するボーン
/// 肩C: 腕の親になり、肩Pの回転を-1倍で打ち消すgrantボーン
fn add_shoulder_cancel_bones(model: &mut PmxModel) -> Result<()> {
    let pairs = [
        (
            "右肩",
            "右腕",
            "右肩P",
            "shoulderP_R",
            "右肩C",
            "shoulderC_R",
        ),
        (
            "左肩",
            "左腕",
            "左肩P",
            "shoulderP_L",
            "左肩C",
            "shoulderC_L",
        ),
    ];

    for (shoulder_name, arm_name, p_jp, p_en, c_jp, c_en) in pairs {
        // 1. 肩・腕のインデックスと位置を取得
        let Some(shoulder_idx) = find_bone_idx(&model.bones, shoulder_name) else {
            log::warn!("[step16] \"{}\" が見つからないためスキップ", shoulder_name);
            continue;
        };
        let Some(arm_idx) = find_bone_idx(&model.bones, arm_name) else {
            log::warn!("[step16] \"{}\" が見つからないためスキップ", arm_name);
            continue;
        };

        let shoulder_pos = model.bones[shoulder_idx as usize].position;
        let shoulder_original_parent = model.bones[shoulder_idx as usize].parent_index;
        let arm_pos = model.bones[arm_idx as usize].position;

        // 2. 肩Pを末尾に追加 → 肩の直前に移動
        let p_flags = BONE_FLAG_ROTATABLE | BONE_FLAG_VISIBLE | BONE_FLAG_OPERABLE;
        let p_at = model.bones.len();
        model.bones.push(PmxBone {
            name: p_jp.to_string(),
            name_en: p_en.to_string(),
            position: shoulder_pos,
            parent_index: shoulder_original_parent,
            deform_layer: 0,
            flags: p_flags | BONE_FLAG_TAIL_IS_BONE,
            tail: BoneTail::BoneIndex(shoulder_idx), // tail → 肩
            ik: None,
            grant: None,
        });

        // 肩の親を肩Pに変更
        model.bones[shoulder_idx as usize].parent_index = p_at as i32;

        // 肩Pを肩の直前に移動
        let shoulder_now = model
            .bones
            .iter()
            .position(|b| b.name == shoulder_name)
            .ok_or_else(|| {
                PoponeError::Build(format!("ボーン「{shoulder_name}」が見つかりません"))
            })?;
        move_bone_in_model(model, p_at, shoulder_now);

        log::debug!(
            "[step16] \"{}\" 追加 pos=({:.3},{:.3},{:.3}) parent={}",
            p_jp,
            shoulder_pos.x,
            shoulder_pos.y,
            shoulder_pos.z,
            shoulder_original_parent
        );

        // 3. 肩Cを末尾に追加 → 腕の直前に移動
        //    肩Cのgrant = 肩P × (-1.0)
        let c_flags = BONE_FLAG_ROTATABLE | BONE_FLAG_ROTATION_GRANT;
        let shoulder_idx_now = find_bone_idx(&model.bones, shoulder_name).ok_or_else(|| {
            PoponeError::Build(format!("ボーン「{shoulder_name}」が見つかりません"))
        })?;
        let p_idx_now = find_bone_idx(&model.bones, p_jp)
            .ok_or_else(|| PoponeError::Build(format!("ボーン「{p_jp}」が見つかりません")))?;

        let c_at = model.bones.len();
        model.bones.push(PmxBone {
            name: c_jp.to_string(),
            name_en: c_en.to_string(),
            position: arm_pos,
            parent_index: shoulder_idx_now,
            deform_layer: 0,
            flags: c_flags,
            tail: BoneTail::Offset(Vec3::ZERO),
            ik: None,
            grant: Some(PmxGrant {
                parent_index: p_idx_now,
                ratio: -1.0,
            }),
        });

        // 腕の親を肩Cに変更
        let arm_idx_now = find_bone_idx(&model.bones, arm_name)
            .ok_or_else(|| PoponeError::Build(format!("ボーン「{arm_name}」が見つかりません")))?;
        model.bones[arm_idx_now as usize].parent_index = c_at as i32;

        // 肩Cを腕の直前に移動
        let arm_now = model
            .bones
            .iter()
            .position(|b| b.name == arm_name)
            .ok_or_else(|| PoponeError::Build(format!("ボーン「{arm_name}」が見つかりません")))?;
        move_bone_in_model(model, c_at, arm_now);

        log::debug!(
            "[step16] \"{}\" 追加 pos=({:.3},{:.3},{:.3}) grant←\"{}\" × -1.0",
            c_jp,
            arm_pos.x,
            arm_pos.y,
            arm_pos.z,
            p_jp
        );
    }
    Ok(())
}

fn build_bones(ir: &IrModel, raw_structure: bool) -> Vec<PmxBone> {
    let mut pmx_bones = Vec::with_capacity(ir.bones.len());
    let pos_fn: fn(glam::Vec3) -> glam::Vec3 = if ir.source_format.is_vrm0() {
        gltf_pos_to_pmx_v0
    } else {
        gltf_pos_to_pmx
    };

    for bone in ir.bones.iter() {
        let pmx_pos = pos_fn(bone.position);

        // VRM骨名 → PMX日本語名（raw_structure 時は元のボーン名を維持）
        let (jp_name, en_name) = if raw_structure {
            (bone.original_name.clone(), bone.original_name.clone())
        } else if let Some(vrm_name) = &bone.vrm_bone_name {
            if let Some((jp, en)) = vrm_bone_to_pmx_name(vrm_name) {
                (jp.to_string(), en.to_string())
            } else {
                (bone.name.clone(), bone.name_en.clone())
            }
        } else {
            (bone.name.clone(), bone.name_en.clone())
        };

        let parent_index = bone.parent.map(|p| p as i32).unwrap_or(-1);

        // 接続先：子ボーンがあればそのIndex、なければオフセット0
        let tail = if let Some(&child_idx) = bone.children.first() {
            BoneTail::BoneIndex(child_idx as i32)
        } else {
            BoneTail::Offset(Vec3::ZERO)
        };

        // フラグ
        let mut flags = BONE_FLAG_ROTATABLE | BONE_FLAG_OPERABLE;
        if !bone.children.is_empty() {
            flags |= BONE_FLAG_TAIL_IS_BONE;
        }
        if bone.is_physics {
            flags |= BONE_FLAG_PHYS_AFTER;
        }

        // raw_structure 時は元のフラグを忠実に反映
        if raw_structure {
            if bone.is_translatable {
                flags |= BONE_FLAG_TRANSLATABLE;
            }
            if bone.is_visible {
                flags |= BONE_FLAG_VISIBLE;
            }
            if bone.is_axis_fixed {
                flags |= BONE_FLAG_AXIS_FIXED;
            }
        } else {
            flags |= BONE_FLAG_VISIBLE;
        }

        // 付与データ変換（raw_structure 時のみ）
        let grant = if raw_structure {
            bone.grant.as_ref().map(|g| {
                if g.is_rotation {
                    flags |= BONE_FLAG_ROTATION_GRANT;
                }
                if g.is_move {
                    flags |= BONE_FLAG_MOVE_GRANT;
                }
                if g.is_local {
                    flags |= BONE_FLAG_LOCAL_GRANT;
                }
                PmxGrant {
                    parent_index: g.parent_index as i32,
                    ratio: g.ratio,
                }
            })
        } else {
            None
        };

        pmx_bones.push(PmxBone {
            name: jp_name,
            name_en: en_name,
            position: pmx_pos,
            parent_index,
            deform_layer: 0,
            flags,
            tail,
            ik: None,
            grant,
        });
    }

    log::debug!(
        "build_bones: VRM {} 本 → PMX {} 本",
        ir.bones.len(),
        pmx_bones.len()
    );
    pmx_bones
}

fn build_vertices_and_faces(
    ir: &IrModel,
    use_vrm0_coords: bool,
) -> (Vec<PmxVertex>, Vec<[u32; 3]>, Vec<u32>) {
    let total_verts: usize = ir.meshes.iter().map(|m| m.vertices.len()).sum();
    let total_faces: usize = ir.meshes.iter().map(|m| m.indices.len() / 3).sum();
    let mut all_vertices: Vec<PmxVertex> = Vec::with_capacity(total_verts);
    let mut all_faces: Vec<[u32; 3]> = Vec::with_capacity(total_faces);
    let pos_fn: fn(glam::Vec3) -> glam::Vec3 = if use_vrm0_coords {
        gltf_pos_to_pmx_v0
    } else {
        gltf_pos_to_pmx
    };
    let nrm_fn: fn(glam::Vec3) -> glam::Vec3 = if use_vrm0_coords {
        gltf_normal_to_pmx_v0
    } else {
        gltf_normal_to_pmx
    };

    let mat_count = ir.materials.len().max(1);
    let mut mat_face_counts = vec![0u32; mat_count];

    // 1. 頂点を ir_meshes 順に配置（モーフの mesh_vertex_start と一致させる）
    let mut mesh_vertex_start: Vec<u32> = Vec::with_capacity(ir.meshes.len());
    for mesh in &ir.meshes {
        let vertex_offset = all_vertices.len() as u32;
        mesh_vertex_start.push(vertex_offset);

        for vtx in &mesh.vertices {
            let pmx_pos = pos_fn(vtx.position);
            let pmx_normal = nrm_fn(vtx.normal);
            let weight = build_weight(vtx.active_weights());

            all_vertices.push(PmxVertex {
                position: pmx_pos,
                normal: pmx_normal,
                uv: glam::Vec2::new(fract_uv(vtx.uv.x), fract_uv(vtx.uv.y)),
                weight,
                edge_scale: vtx.edge_scale,
            });
        }
    }

    // 2. 面を材質順にグループ化（PMX要件: 材質ごとに連続した面配列）
    // VRM 1.0: (x,y,-z) → det=-1、VRM 0.0: (-x,y,z) → det=-1
    // 両バージョンとも行列式 -1 → b,c を swap して巻き順を反転
    for (mat_idx, face_count_slot) in mat_face_counts.iter_mut().enumerate() {
        for (mesh_i, mesh) in ir.meshes.iter().enumerate() {
            if mesh.material_index != mat_idx {
                continue;
            }
            let vertex_offset = mesh_vertex_start[mesh_i];
            let indices = &mesh.indices;
            let face_count = indices.len() / 3;
            for i in 0..face_count {
                let a = indices[i * 3] + vertex_offset;
                let b = indices[i * 3 + 1] + vertex_offset;
                let c = indices[i * 3 + 2] + vertex_offset;
                all_faces.push([a, c, b]);
            }
            *face_count_slot += (face_count * 3) as u32;
        }
    }

    // 頂点ウェイト統計
    let mut bdef1 = 0usize;
    let mut bdef2 = 0usize;
    let mut bdef4 = 0usize;
    for v in &all_vertices {
        match &v.weight {
            PmxWeightType::Bdef1 { .. } => bdef1 += 1,
            PmxWeightType::Bdef2 { .. } => bdef2 += 1,
            PmxWeightType::Bdef4 { .. } => bdef4 += 1,
        }
    }
    log::info!(
        "頂点: {}個 (BDEF1={}, BDEF2={}, BDEF4={})",
        all_vertices.len(),
        bdef1,
        bdef2,
        bdef4
    );
    log::info!("面: {}個", all_faces.len());

    (all_vertices, all_faces, mat_face_counts)
}

fn build_weight(weights: &[(usize, f32)]) -> PmxWeightType {
    match weights.len() {
        0 => PmxWeightType::Bdef1 { bone: 0 },
        1 => PmxWeightType::Bdef1 {
            bone: weights[0].0 as i32,
        },
        2 => PmxWeightType::Bdef2 {
            bone1: weights[0].0 as i32,
            bone2: weights[1].0 as i32,
            weight1: weights[0].1,
        },
        3 | 4 => {
            // 3〜4ウェイト: Vec割当不要、入力スライスをそのまま使用
            let total: f32 = weights.iter().map(|(_, w)| w).sum();
            let total = if total > 0.0 { total } else { 1.0 };

            let mut bones = [-1i32; 4];
            let mut ws = [0.0f32; 4];
            for (i, &(bi, w)) in weights.iter().enumerate() {
                bones[i] = bi as i32;
                ws[i] = w / total;
            }

            PmxWeightType::Bdef4 { bones, weights: ws }
        }
        _ => {
            // 5ウェイト以上（稀）: 上位4ウェイトを選択
            let mut top4 = [(0usize, 0.0f32); 4];
            for &(bi, w) in weights {
                // top4 内の最小ウェイトを探し、現在の値が大きければ置換
                let mut min_idx = 0;
                let mut min_w = top4[0].1;
                for j in 1..4 {
                    if top4[j].1 < min_w {
                        min_w = top4[j].1;
                        min_idx = j;
                    }
                }
                if w > min_w {
                    top4[min_idx] = (bi, w);
                }
            }

            // 正規化
            let total: f32 = top4.iter().map(|(_, w)| w).sum();
            let total = if total > 0.0 { total } else { 1.0 };

            let mut bones = [-1i32; 4];
            let mut ws = [0.0f32; 4];
            for (i, &(bi, w)) in top4.iter().enumerate() {
                bones[i] = bi as i32;
                ws[i] = w / total;
            }

            PmxWeightType::Bdef4 { bones, weights: ws }
        }
    }
}

fn build_morphs(ir: &IrModel, use_vrm0_coords: bool) -> Vec<PmxMorph> {
    let pos_fn: fn(glam::Vec3) -> glam::Vec3 = if use_vrm0_coords {
        gltf_pos_to_pmx_v0
    } else {
        gltf_pos_to_pmx
    };

    let panel_name = |p: u8| -> &'static str {
        match p {
            1 => "眉",
            2 => "目",
            3 => "口",
            4 => "その他",
            _ => "?",
        }
    };

    log::debug!("--- モーフ一覧 ---");
    let mut vertex_count = 0usize;
    let mut group_count = 0usize;

    let morphs: Vec<PmxMorph> = ir
        .morphs
        .iter()
        .map(|m| {
            let (morph_type, offsets) = match &m.kind {
                IrMorphKind::Vertex { ref positions, .. } => {
                    log::debug!(
                        "  [{}:{}] \"{}\" 頂点モーフ (対象頂点={})",
                        panel_name(m.panel),
                        m.panel,
                        m.name,
                        positions.len()
                    );
                    vertex_count += 1;
                    // 同一頂点の重複オフセットを合算
                    let mut merged: std::collections::HashMap<u32, glam::Vec3> =
                        std::collections::HashMap::new();
                    for &(vi, off) in positions {
                        *merged.entry(vi as u32).or_insert(glam::Vec3::ZERO) += pos_fn(off);
                    }
                    let mut pmx_offs: Vec<VertexMorphOffset> = merged
                        .into_iter()
                        .filter(|(_, off)| off.length_squared() > 1e-12)
                        .map(|(vi, off)| VertexMorphOffset {
                            vertex_index: vi,
                            offset: off,
                        })
                        .collect();
                    // HashMap の走査順は非決定的なので vertex_index でソート（出力安定化）
                    pmx_offs.sort_by_key(|o| o.vertex_index);
                    (1u8, PmxMorphOffsets::Vertex(pmx_offs))
                }
                IrMorphKind::Group(goffs) => {
                    if log::log_enabled!(log::Level::Debug) {
                        let sub_names: Vec<String> = goffs
                            .iter()
                            .filter_map(|(mi, w)| {
                                ir.morphs
                                    .get(*mi)
                                    .map(|sub| format!("{}×{:.2}", sub.name, w))
                            })
                            .collect();
                        log::debug!(
                            "  [{}:{}] \"{}\" グループモーフ (子={}) [{}]",
                            panel_name(m.panel),
                            m.panel,
                            m.name,
                            goffs.len(),
                            sub_names.join(", ")
                        );
                    }
                    group_count += 1;
                    let pmx_offs = goffs
                        .iter()
                        .map(|(mi, w)| GroupMorphOffset {
                            morph_index: *mi as i32,
                            weight: *w,
                        })
                        .collect();
                    (0u8, PmxMorphOffsets::Group(pmx_offs))
                }
            };

            PmxMorph {
                name: m.name.clone(),
                name_en: m.name_en.clone(),
                panel: m.panel,
                morph_type,
                offsets,
            }
        })
        .collect();

    log::info!(
        "モーフ: {}個 (頂点モーフ={}, グループモーフ={})",
        morphs.len(),
        vertex_count,
        group_count
    );
    morphs
}

/// ボーンを表示枠カテゴリに分類する
#[derive(Debug, Clone, Copy, PartialEq)]
enum BoneCategory {
    Root,    // 全ての親 → Root枠で処理済み
    Body,    // 体(上)
    Arms,    // 腕
    Fingers, // 指
    Legs,    // 足
    Others,  // その他
}

fn classify_bone(name: &str) -> BoneCategory {
    // Root枠のボーン
    if name == "全ての親" {
        return BoneCategory::Root;
    }

    // 体(上)
    const BODY: &[&str] = &[
        "センター",
        "グルーブ",
        "腰",
        "上半身",
        "上半身2",
        "上半身3",
        "首",
        "頭",
        "両目",
        "左目",
        "右目",
        "下半身",
    ];
    if BODY.contains(&name) {
        return BoneCategory::Body;
    }

    // 指（左右の指ボーン名）
    if name.contains("親指")
        || name.contains("人差指")
        || name.contains("中指")
        || name.contains("薬指")
        || name.contains("小指")
    {
        return BoneCategory::Fingers;
    }

    // 腕（肩〜手首）
    const ARM_KEYWORDS: &[&str] = &["肩P", "肩C", "肩", "腕捩", "腕", "ひじ", "手捩", "手首"];
    if ARM_KEYWORDS.iter().any(|kw| name.contains(kw)) {
        return BoneCategory::Arms;
    }

    // 足（足〜つま先、IK含む）
    const LEG_KEYWORDS: &[&str] = &[
        "足先EX",
        "足D",
        "ひざD",
        "足首D",
        "足",
        "ひざ",
        "足首",
        "つま先",
        "ＩＫ",
        "腰キャンセル",
    ];
    if LEG_KEYWORDS.iter().any(|kw| name.contains(kw)) {
        return BoneCategory::Legs;
    }

    BoneCategory::Others
}

fn build_display_frames(bones: &[PmxBone], morphs: &[PmxMorph]) -> Vec<PmxDisplayFrame> {
    let mut frames = Vec::with_capacity(7);

    // 枠0: Root（特殊枠）
    frames.push(PmxDisplayFrame {
        name: "Root".to_string(),
        name_en: "Root".to_string(),
        is_special: 1,
        elements: if !bones.is_empty() {
            vec![DisplayFrameElement::Bone(0)]
        } else {
            vec![]
        },
    });

    // 枠1: 表情（特殊枠）
    let morph_elements: Vec<DisplayFrameElement> = (0..morphs.len() as i32)
        .map(DisplayFrameElement::Morph)
        .collect();
    frames.push(PmxDisplayFrame {
        name: "表情".to_string(),
        name_en: "Exp".to_string(),
        is_special: 1,
        elements: morph_elements,
    });

    // ボーンをカテゴリ別に分類
    let mut body_elems = Vec::new();
    let mut arm_elems = Vec::new();
    let mut finger_elems = Vec::new();
    let mut leg_elems = Vec::new();
    let mut other_elems = Vec::new();

    for (i, bone) in bones.iter().enumerate() {
        let idx = i as i32;
        let cat = classify_bone(&bone.name);

        // Root枠のボーンはスキップ（既に枠0に含まれている）
        if cat == BoneCategory::Root {
            continue;
        }

        // 非表示かつ操作不可のボーンは表示枠に含めない（grant専用ボーン等）
        // ただしIK系ボーンは含める
        let is_visible = bone.flags & BONE_FLAG_VISIBLE != 0;
        let is_operable = bone.flags & BONE_FLAG_OPERABLE != 0;
        let is_ik = bone.flags & BONE_FLAG_IK != 0;
        if !is_visible && !is_operable && !is_ik {
            continue;
        }

        match cat {
            BoneCategory::Body => body_elems.push(DisplayFrameElement::Bone(idx)),
            BoneCategory::Arms => arm_elems.push(DisplayFrameElement::Bone(idx)),
            BoneCategory::Fingers => finger_elems.push(DisplayFrameElement::Bone(idx)),
            BoneCategory::Legs => leg_elems.push(DisplayFrameElement::Bone(idx)),
            BoneCategory::Others => other_elems.push(DisplayFrameElement::Bone(idx)),
            BoneCategory::Root => {} // 上でスキップ済み
        }
    }

    // 枠2: 体(上)
    frames.push(PmxDisplayFrame {
        name: "体(上)".to_string(),
        name_en: "Body".to_string(),
        is_special: 0,
        elements: body_elems,
    });

    // 枠3: 腕
    frames.push(PmxDisplayFrame {
        name: "腕".to_string(),
        name_en: "Arms".to_string(),
        is_special: 0,
        elements: arm_elems,
    });

    // 枠4: 指
    frames.push(PmxDisplayFrame {
        name: "指".to_string(),
        name_en: "Fingers".to_string(),
        is_special: 0,
        elements: finger_elems,
    });

    // 枠5: 足
    frames.push(PmxDisplayFrame {
        name: "足".to_string(),
        name_en: "Legs".to_string(),
        is_special: 0,
        elements: leg_elems,
    });

    // 枠6: その他
    frames.push(PmxDisplayFrame {
        name: "その他".to_string(),
        name_en: "Others".to_string(),
        is_special: 0,
        elements: other_elems,
    });

    frames
}

fn build_rigid_bodies(ir: &IrModel, align_rigid_rotation: bool) -> Vec<PmxRigidBody> {
    let mode_name = |m: u8| -> &'static str {
        match m {
            0 => "ボーン追従",
            1 => "物理演算",
            2 => "物理+Bone",
            _ => "?",
        }
    };

    log::debug!("--- 剛体一覧 ---");
    let mut sphere_count = 0usize;
    let mut box_count = 0usize;
    let mut capsule_count = 0usize;
    let mut mode_counts = [0usize; 3];

    let bodies: Vec<PmxRigidBody> = ir.physics.rigid_bodies.iter().enumerate().map(|(i, rb)| {
        let (shape, size) = match &rb.shape {
            RigidShape::Sphere { radius } => { sphere_count += 1; (0u8, Vec3::new(*radius, 0.0, 0.0)) }
            RigidShape::Box { size } => { box_count += 1; (1u8, *size) }
            RigidShape::Capsule { radius, height } => { capsule_count += 1; (2u8, Vec3::new(*radius, *height, 0.0)) }
        };
        if rb.physics_mode < 3 { mode_counts[rb.physics_mode as usize] += 1; }

        if log::log_enabled!(log::Level::Debug) {
            let shape_name = match &rb.shape {
                RigidShape::Sphere { radius } => format!("球 r={:.3}", radius),
                RigidShape::Box { size } => format!("箱 ({:.3},{:.3},{:.3})", size.x, size.y, size.z),
                RigidShape::Capsule { radius, height } => format!("カプセル r={:.3} h={:.3}", radius, height),
            };
            log::debug!("  [{:2}] \"{}\" {} bone={:?} group={} mode={} mass={:.2} pos=({:.3},{:.3},{:.3})",
                i, rb.name, shape_name, rb.bone_index, rb.group, mode_name(rb.physics_mode), rb.mass,
                rb.position.x, rb.position.y, rb.position.z);
        }

        PmxRigidBody {
            name: rb.name.clone(),
            name_en: rb.name.clone(),
            bone_index: rb.bone_index.map(|i| i as i32).unwrap_or(-1),
            group: rb.group,
            no_collision_mask: rb.no_collision_mask,
            shape,
            size,
            position: rb.position,
            rotation: if align_rigid_rotation { rb.rotation } else { Vec3::ZERO },
            mass: rb.mass,
            linear_damping: rb.linear_damping,
            angular_damping: rb.angular_damping,
            restitution: rb.restitution,
            friction: rb.friction,
            physics_mode: rb.physics_mode,
        }
    }).collect();

    log::info!(
        "剛体: {}個 (球={}, 箱={}, カプセル={}) モード: ボーン追従={}, 物理={}, 物理+Bone={}",
        bodies.len(),
        sphere_count,
        box_count,
        capsule_count,
        mode_counts[0],
        mode_counts[1],
        mode_counts[2]
    );
    bodies
}

/// ボーン名の重複を解決（2番目以降に "_N" サフィックスを付加）
/// 重複ボーン名を連番サフィックスで解消する
fn fix_duplicate_names(bones: &mut [PmxBone]) {
    use std::collections::HashMap;
    // 同名ボーンの出現回数をカウント
    let mut count: HashMap<String, usize> = HashMap::new();
    for bone in bones.iter() {
        *count.entry(bone.name.clone()).or_insert(0) += 1;
    }
    // 重複があるボーンだけ処理（2番目以降をリネーム）
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut renamed = 0usize;
    for bone in bones.iter_mut() {
        if count.get(&bone.name).copied().unwrap_or(0) > 1 {
            let n = seen.entry(bone.name.clone()).or_insert(0);
            *n += 1;
            if *n >= 2 {
                let new_name = format!("{}_{}", bone.name, n);
                log::debug!("fix_duplicate_names: \"{}\" → \"{}\"", bone.name, new_name);
                bone.name = new_name;
                renamed += 1;
            }
        }
    }
    if renamed > 0 {
        log::info!(
            "fix_duplicate_names: {}本のボーン名を重複回避のためリネーム",
            renamed
        );
    }
}

/// ボーンを変形順序規則に従い並べ替え（親が子より先に来るよう保証）
///
/// 規則（優先順位）:
///   1. AfterPhysics=OFF のボーンが先、ON が後
///   2. 各グループ内で親ボーンが子ボーンより先（BFS トポロジカルソート）
///
/// BFS は元の並び順を可能な限り保持する安定ソート。
fn sort_bones_topological(model: &mut PmxModel) {
    let n = model.bones.len();
    if n == 0 {
        return;
    }

    let phys: Vec<bool> = model
        .bones
        .iter()
        .map(|b| b.flags & BONE_FLAG_PHYS_AFTER != 0)
        .collect();

    // 隣接リストを事前構築（O(n) で子探索を可能にする）
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, bone) in model.bones.iter().enumerate() {
        let p = bone.parent_index;
        if p >= 0 && (p as usize) < n {
            children[p as usize].push(i);
        }
    }

    let mut result: Vec<usize> = Vec::with_capacity(n);
    let mut added = vec![false; n];

    for pass_phys in [false, true] {
        // このグループのルート（グループ内に親がいない骨）を最小インデックス優先キューに追加
        // BinaryHeap<Reverse> = 最小ヒープ: 常に最小インデックスのボーンを優先処理し
        // insert_standard_bones で整列した配置順を最大限保持する
        let mut heap: std::collections::BinaryHeap<std::cmp::Reverse<usize>> = (0..n)
            .filter(|&i| {
                phys[i] == pass_phys && {
                    let p = model.bones[i].parent_index;
                    p < 0 || phys[p as usize] != pass_phys
                }
            })
            .map(std::cmp::Reverse)
            .collect();

        while let Some(std::cmp::Reverse(i)) = heap.pop() {
            if added[i] {
                continue;
            }
            added[i] = true;
            result.push(i);

            // 隣接リストから同グループの子を最小インデックス優先でヒープに追加
            for &j in &children[i] {
                if !added[j] && phys[j] == pass_phys {
                    heap.push(std::cmp::Reverse(j));
                }
            }
        }
    }

    // 循環参照・孤立ボーンは末尾に追加（フォールバック）
    for (i, &is_added) in added.iter().enumerate() {
        if !is_added {
            log::warn!(
                "sort_bones_topological: \"{}\"(idx={}) が到達不能 → 末尾に追加",
                model.bones[i].name,
                i
            );
            result.push(i);
        }
    }

    // 変化がなければ早期リターン
    if result.iter().enumerate().all(|(new, &old)| new == old) {
        return;
    }

    // remap テーブル（旧インデックス → 新インデックス）
    let mut remap = vec![0i32; n];
    for (new_idx, &old_idx) in result.iter().enumerate() {
        remap[old_idx] = new_idx as i32;
    }

    log::debug!("sort_bones_topological: {}本のボーンを並び替え", n);

    // ボーン配列を並び替え（clone 不要: take 済みなので所有権ベースで並べ替え）
    let mut old_bones: Vec<Option<PmxBone>> = std::mem::take(&mut model.bones)
        .into_iter()
        .map(Some)
        .collect();
    model.bones = result
        .iter()
        .map(|&i| {
            old_bones[i]
                .take()
                .expect("sort_bones_topological: 同一ボーンが2回参照された")
        })
        .collect();

    // 全参照を remap で更新
    remap_all_bone_indices(
        model,
        |idx| {
            if idx >= 0 {
                remap[idx as usize]
            } else {
                idx
            }
        },
    );
}

fn build_joints(ir: &IrModel) -> Vec<PmxJoint> {
    log::debug!("--- ジョイント一覧 ---");
    let joints: Vec<PmxJoint> = ir
        .physics
        .joints
        .iter()
        .enumerate()
        .map(|(i, j)| {
            let rb_a_name = ir
                .physics
                .rigid_bodies
                .get(j.rigid_a)
                .map(|r| r.name.as_str())
                .unwrap_or("?");
            let rb_b_name = ir
                .physics
                .rigid_bodies
                .get(j.rigid_b)
                .map(|r| r.name.as_str())
                .unwrap_or("?");
            log::debug!(
                "  [{:2}] \"{}\" A=\"{}\"({}) ↔ B=\"{}\"({}) pos=({:.3},{:.3},{:.3})",
                i,
                j.name,
                rb_a_name,
                j.rigid_a,
                rb_b_name,
                j.rigid_b,
                j.position.x,
                j.position.y,
                j.position.z
            );

            PmxJoint {
                name: j.name.clone(),
                name_en: j.name.clone(),
                joint_type: 0, // スプリング6DOF
                rigid_a: j.rigid_a as i32,
                rigid_b: j.rigid_b as i32,
                position: j.position,
                rotation: j.rotation,
                move_limit_lo: j.move_limit_lo,
                move_limit_hi: j.move_limit_hi,
                rot_limit_lo: j.rot_limit_lo,
                rot_limit_hi: j.rot_limit_hi,
                spring_move: j.spring_move,
                spring_rot: j.spring_rot,
            }
        })
        .collect();
    log::info!("ジョイント: {}個", joints.len());
    joints
}

/// UV値を 0..1 に正規化（負値対応の fract）
#[inline]
fn fract_uv(v: f32) -> f32 {
    let f = v % 1.0;
    if f < 0.0 {
        f + 1.0
    } else {
        f
    }
}
