use std::collections::HashMap;
use std::sync::Arc;

use eframe::wgpu;
use glam::{Mat4, Quat, Vec3};

use crate::convert::coord::{pmx_pos_to_gltf, gltf_pos_to_pmx_unified, pmx_normal_to_gltf, gltf_normal_to_pmx_unified};
use crate::intermediate::animation::{BoneMatchMode, VrmaAnimation};
use crate::intermediate::types::IrModel;

use super::mesh::GpuModel;

/// 1フレームの長さ（秒）
const FRAME_DURATION: f32 = 1.0 / 30.0;

/// ループモード
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopMode {
    /// ループなし
    None,
    /// 通常ループ
    Normal,
    /// A-Bループ（区間リピート）
    AB,
    /// ピンポン（往復ループ）
    PingPong,
}

/// ボーンごとのスキニングウェイト（GPU頂点単位）
struct VertexSkinWeight {
    /// (bone_index, weight) 最大4
    bones: [(usize, f32); 4],
}

/// アニメーション再生に必要なスキニングデータ
struct SkinningData {
    /// GPU頂点ごとのボーンウェイト
    vertex_weights: Vec<VertexSkinWeight>,
    /// レストポーズのボーングローバル行列（glTF空間）
    rest_global_mats: Vec<Mat4>,
    /// ボーンのローカル行列（レストポーズ、分解前の生行列）
    rest_local_mats: Vec<Mat4>,
    /// ボーンのローカル回転（レストポーズ）
    rest_local_rotations: Vec<Quat>,
    /// ボーンのグローバル回転（レストポーズ、リターゲティング用）
    rest_global_rotations: Vec<Quat>,
    /// ボーンのローカル平行移動（レストポーズ）
    rest_local_translations: Vec<Vec3>,
    /// ボーンのローカルスケール（レストポーズ）
    rest_local_scales: Vec<Vec3>,
    /// ボーンの親インデックス
    bone_parents: Vec<Option<usize>>,
    /// ボーンの子インデックスリスト
    bone_children: Vec<Vec<usize>>,
    /// IrBone インデックス → VRM ヒューマノイドボーン名（逆引き）
    bone_idx_to_name: HashMap<usize, String>,
    /// VRM 表情名 → モーフインデックス
    expr_name_to_morph: HashMap<String, usize>,
    /// VRM 0.0 かどうか
    is_vrm0: bool,
}

/// アニメーション再生状態
pub struct AnimationState {
    pub animation: Arc<VrmaAnimation>,
    pub playing: bool,
    pub loop_mode: LoopMode,
    pub speed: f32,
    pub current_time: f32,
    /// A-Bループ開始点（秒）
    pub ab_start: Option<f32>,
    /// A-Bループ終了点（秒）
    pub ab_end: Option<f32>,
    /// ピンポン再生方向（1.0: 順方向, -1.0: 逆方向）
    pub ping_pong_direction: f32,
    skin: SkinningData,
    /// アニメーション済みグローバル行列キャッシュ（glTF空間）
    cached_animated_globals: Vec<Mat4>,
    /// デルタ行列の作業バッファ（毎フレーム alloc 回避）
    work_deltas: Vec<Mat4>,
    /// compute_animated_globals 用フラグバッファ（毎フレーム alloc 回避）
    work_computed: Vec<bool>,
}

impl AnimationState {
    /// IrModel と GpuModel からアニメーション再生状態を構築
    pub fn new(animation: Arc<VrmaAnimation>, ir: &IrModel, gpu_model: &GpuModel) -> Self {
        let skin = build_skinning_data(ir, gpu_model, &animation);
        Self {
            animation,
            playing: true,
            loop_mode: LoopMode::Normal,
            speed: 1.0,
            current_time: 0.0,
            ab_start: None,
            ab_end: None,
            ping_pong_direction: 1.0,
            work_deltas: vec![Mat4::IDENTITY; skin.rest_global_mats.len()],
            work_computed: vec![false; skin.rest_global_mats.len()],
            cached_animated_globals: skin.rest_global_mats.clone(),
            skin,
        }
    }

    /// 有効な再生範囲を返す
    pub fn effective_range(&self) -> (f32, f32) {
        match self.loop_mode {
            LoopMode::AB | LoopMode::PingPong => {
                let lo = self.ab_start.unwrap_or(0.0);
                let hi = self.ab_end.unwrap_or(self.animation.duration);
                (lo.min(hi), lo.max(hi))
            }
            _ => (0.0, self.animation.duration),
        }
    }

    /// アニメーション時間を進める
    pub fn advance(&mut self, dt: f32) {
        if !self.playing {
            return;
        }

        let effective_speed = match self.loop_mode {
            LoopMode::PingPong => self.speed.abs() * self.ping_pong_direction,
            _ => self.speed,
        };
        self.current_time += dt * effective_speed;

        let (lo, hi) = self.effective_range();
        let range = hi - lo;

        match self.loop_mode {
            LoopMode::None => {
                if self.current_time > hi {
                    self.current_time = hi;
                    self.playing = false;
                } else if self.current_time < lo {
                    self.current_time = lo;
                    self.playing = false;
                }
            }
            LoopMode::Normal | LoopMode::AB => {
                if range > 0.0 {
                    if self.current_time > hi {
                        self.current_time = lo + (self.current_time - lo) % range;
                    } else if self.current_time < lo {
                        self.current_time = hi - (lo - self.current_time) % range;
                    }
                }
            }
            LoopMode::PingPong => {
                if self.current_time > hi {
                    self.current_time = hi - (self.current_time - hi).min(range);
                    self.ping_pong_direction = -1.0;
                } else if self.current_time < lo {
                    self.current_time = lo + (lo - self.current_time).min(range);
                    self.ping_pong_direction = 1.0;
                }
            }
        }
    }

    /// 1フレーム分進める/戻す（一時停止中に使用）
    pub fn step_frame(&mut self, forward: bool) {
        let delta = if forward { FRAME_DURATION } else { -FRAME_DURATION };
        self.current_time += delta;
        let (lo, hi) = self.effective_range();
        self.current_time = self.current_time.clamp(lo, hi);
    }

    /// 現在時刻の表情ウェイトをモーフウェイト配列に書き込む
    /// 戻り値: 何か変更があったか
    pub fn apply_expressions(&self, morph_weights: &mut [f32]) -> bool {
        let mut changed = false;
        for expr_name in self.animation.expression_channels.keys() {
            if let Some(&morph_idx) = self.skin.expr_name_to_morph.get(expr_name.as_str()) {
                if morph_idx < morph_weights.len() {
                    let w = self.animation.sample_expression(expr_name, self.current_time)
                        .unwrap_or(0.0);
                    if (morph_weights[morph_idx] - w).abs() > 1e-6 {
                        morph_weights[morph_idx] = w;
                        changed = true;
                    }
                }
            }
        }
        changed
    }

    /// アニメーション済みグローバル行列を取得（glTF空間）
    pub fn animated_globals(&self) -> &[Mat4] {
        &self.cached_animated_globals
    }

    /// VRM 0.0 かどうか
    pub fn is_vrm0(&self) -> bool {
        self.skin.is_vrm0
    }

    /// 現在時刻のボーンアニメーションを頂点バッファに適用
    pub fn apply_bone_animation(
        &mut self,
        gpu_model: &mut GpuModel,
        queue: &wgpu::Queue,
        morph_weights: &[f32],
    ) {
        // グローバル行列を in-place で計算（alloc 回避）
        self.compute_animated_globals_inplace();

        // デルタ行列を作業バッファに計算（alloc 回避）
        let bone_count = self.skin.rest_global_mats.len();
        self.work_deltas.resize(bone_count, Mat4::IDENTITY);
        for i in 0..bone_count {
            let inv_rest = self.skin.rest_global_mats[i].inverse();
            self.work_deltas[i] = self.cached_animated_globals[i] * inv_rest;
        }

        let is_vrm0 = self.skin.is_vrm0;

        // 頂点バッファを再利用（初回のみ alloc、以降は capacity 再利用）
        gpu_model.reset_animated_to_base();
        {
            let work = gpu_model.animated_vertices_mut();
            let deltas = &self.work_deltas;

            for (vi, vw) in self.skin.vertex_weights.iter().enumerate() {
                if vi >= work.len() {
                    break;
                }

                let mut blended = Mat4::ZERO;
                let mut total_w = 0.0f32;
                for &(bone_idx, weight) in &vw.bones {
                    if weight > 0.0 && bone_idx < deltas.len() {
                        blended += weight * deltas[bone_idx];
                        total_w += weight;
                    }
                }

                if total_w < 1e-6 {
                    continue;
                }

                if (total_w - 1.0).abs() > 1e-4 {
                    blended *= 1.0 / total_w;
                }

                // PMX位置 → glTF位置 → デルタ適用 → PMX位置
                let pmx_pos = Vec3::from(work[vi].position);
                let gltf_pos = pmx_pos_to_gltf(pmx_pos, is_vrm0);
                let new_pmx = gltf_pos_to_pmx_unified(blended.transform_point3(gltf_pos), is_vrm0);
                work[vi].position = new_pmx.to_array();

                // 法線
                let pmx_normal = Vec3::from(work[vi].normal);
                let gltf_normal = pmx_normal_to_gltf(pmx_normal, is_vrm0);
                let new_pmx_n = gltf_normal_to_pmx_unified(
                    blended.transform_vector3(gltf_normal).normalize_or_zero(), is_vrm0,
                );
                work[vi].normal = new_pmx_n.to_array();
            }
        } // work の可変借用をここでドロップ

        // モーフを animated_vertices に直接適用（借用衝突回避）
        gpu_model.apply_morphs_to_animated(morph_weights);

        // GPU バッファに書き込み
        queue.write_buffer(&gpu_model.vertex_buf, 0, bytemuck::cast_slice(
            gpu_model.current_vertices()
        ));
    }

    /// VRMA のキーフレームからボーンのグローバル行列を in-place 計算（alloc 回避）
    fn compute_animated_globals_inplace(&mut self) {
        let bone_count = self.skin.rest_global_mats.len();
        self.cached_animated_globals.resize(bone_count, Mat4::IDENTITY);
        self.work_computed.resize(bone_count, false);
        self.work_computed.fill(false);
        for i in 0..bone_count {
            self.cached_animated_globals[i] = Mat4::IDENTITY;
        }

        // ルートボーン（親なし）から階層を辿る
        for i in 0..bone_count {
            if self.skin.bone_parents[i].is_none() {
                Self::compute_global_recursive_static(
                    &self.skin, &self.animation, self.current_time,
                    i, Mat4::IDENTITY,
                    &mut self.cached_animated_globals, &mut self.work_computed,
                );
            }
        }
    }

    fn compute_global_recursive_static(
        skin: &SkinningData,
        animation: &VrmaAnimation,
        current_time: f32,
        bone_idx: usize,
        parent_global: Mat4,
        globals: &mut [Mat4],
        computed: &mut [bool],
    ) {
        if computed[bone_idx] {
            return;
        }
        computed[bone_idx] = true;

        // VRMA アニメーションが適用されるボーンかチェック
        let mut animated = false;
        let mut local_rot = skin.rest_local_rotations[bone_idx];
        let mut local_trans = skin.rest_local_translations[bone_idx];

        if let Some(bone_name) = skin.bone_idx_to_name.get(&bone_idx) {
            let is_humanoid = matches!(animation.match_mode, BoneMatchMode::Humanoid);

            // 回転
            if let Some(anim_rot) = animation.sample_bone_rotation(bone_name, current_time) {
                animated = true;
                if animation.is_additive {
                    if animation.is_bone_local_delta {
                        // ボーンローカルデルタ（Unity Muscle SwingTwist）:
                        // anim_rot = postQ × SwingTwist(sign×deg) × Inv(postQ)
                        // 正規化スケルトン基準のデルタ（muscle=0で Identity）
                        // 最終ローカル回転 = rest × anim_rot
                        //   = (rest × postQ) × SwingTwist × Inv(postQ)
                        //   = preQ_model × SwingTwist × Inv(postQ)
                        local_rot = skin.rest_local_rotations[bone_idx] * anim_rot;
                    } else {
                        // ワールド空間デルタ:
                        // 親のレストグローバル回転で共役変換 → ローカル空間デルタに変換
                        let parent_rest_rot = skin.bone_parents[bone_idx]
                            .map(|pi| skin.rest_global_rotations[pi])
                            .unwrap_or(Quat::IDENTITY);
                        let local_delta = parent_rest_rot.inverse() * anim_rot * parent_rest_rot;
                        local_rot = local_delta * skin.rest_local_rotations[bone_idx];
                    }
                } else if is_humanoid {
                    // VRMA: リターゲティング公式適用
                    if let Some(vrma_rest) = animation.bone_rests.get(bone_name.as_str()) {
                        let w_vrma = vrma_rest.global_rotation;
                        let l_vrma = vrma_rest.local_rotation;
                        let l_model = skin.rest_local_rotations[bone_idx];
                        let w_model = skin.rest_global_rotations[bone_idx];

                        let mut normalized = w_vrma * l_vrma.inverse() * anim_rot * w_vrma.inverse();

                        if skin.is_vrm0 {
                            normalized = Quat::from_xyzw(
                                -normalized.x, normalized.y, -normalized.z, normalized.w,
                            );
                        }

                        local_rot = l_model * w_model.inverse() * normalized * w_model;
                    } else {
                        local_rot = if skin.is_vrm0 {
                            Quat::from_xyzw(-anim_rot.x, anim_rot.y, -anim_rot.z, anim_rot.w)
                        } else {
                            anim_rot
                        };
                    }
                } else {
                    // NodeName: グローバル空間リターゲティング
                    if let Some(src_rest) = animation.bone_rests.get(bone_name.as_str()) {
                        let w_src = src_rest.global_rotation;
                        let l_src = src_rest.local_rotation;
                        let l_model = skin.rest_local_rotations[bone_idx];
                        let w_model = skin.rest_global_rotations[bone_idx];

                        // ソースレストからのローカルデルタ → グローバル空間に変換（共役）
                        let local_delta = l_src.inverse() * anim_rot;
                        let mut normalized = w_src * local_delta * w_src.inverse();

                        // ソースが+Z向き（VRMは-Z向き）の場合、Y軸180°補正
                        // normalized の X,Z 成分を反転（= Y軸180°共役）
                        if animation.facing_flip_y {
                            normalized = Quat::from_xyzw(
                                -normalized.x, normalized.y, -normalized.z, normalized.w,
                            );
                        }

                        // ターゲットモデルのローカル空間に変換
                        local_rot = l_model * w_model.inverse() * normalized * w_model;
                    } else {
                        local_rot = anim_rot;
                    }
                }
            }
            // 平行移動
            if let Some(raw_trans) = animation.sample_bone_translation(bone_name, current_time) {
                animated = true;
                if animation.is_additive {
                    // Additive: デルタ値をレスト位置に加算
                    local_trans = skin.rest_local_translations[bone_idx] + raw_trans;
                } else if is_humanoid {
                    // VRMA: レスト位置からのデルタをワールド空間経由でモデルに適用
                    if let Some(vrma_rest) = animation.bone_rests.get(bone_name.as_str()) {
                        let delta_local = raw_trans - vrma_rest.local_translation;
                        let vrma_parent_rot = vrma_rest.global_rotation * vrma_rest.local_rotation.inverse();
                        let mut delta_world = vrma_parent_rot * delta_local;
                        if skin.is_vrm0 {
                            delta_world = Vec3::new(-delta_world.x, delta_world.y, -delta_world.z);
                        }
                        let model_h = skin.rest_global_mats[bone_idx]
                            .transform_point3(Vec3::ZERO).y;
                        let vrma_h = (vrma_parent_rot * vrma_rest.local_translation).y;
                        if vrma_h.abs() > 0.01 {
                            delta_world *= model_h / vrma_h;
                        }
                        let model_parent_rot = skin.rest_global_rotations[bone_idx]
                            * skin.rest_local_rotations[bone_idx].inverse();
                        let delta_model_local = model_parent_rot.inverse() * delta_world;
                        local_trans = skin.rest_local_translations[bone_idx] + delta_model_local;
                    } else {
                        local_trans = if skin.is_vrm0 {
                            Vec3::new(-raw_trans.x, raw_trans.y, -raw_trans.z)
                        } else {
                            raw_trans
                        };
                    }
                } else {
                    // NodeName: ソースレストからのデルタをスケーリングして適用
                    if let Some(src_rest) = animation.bone_rests.get(bone_name.as_str()) {
                        let mut delta = raw_trans - src_rest.local_translation;
                        // ソースが+Z向きの場合、平行移動デルタのX,Zを反転（Y180補正）
                        if animation.facing_flip_y {
                            delta = Vec3::new(-delta.x, delta.y, -delta.z);
                        }
                        let src_len = src_rest.local_translation.length();
                        let model_len = skin.rest_local_translations[bone_idx].length();
                        if src_len > 1e-6 && model_len > 1e-6 {
                            let scale = model_len / src_len;
                            local_trans = skin.rest_local_translations[bone_idx] + delta * scale;
                        }
                        // src_len が 0 に近い場合（ルートなど）はデルタをそのまま適用しない
                    }
                }
            }
        }

        if animated {
            // アニメーション適用ボーン: スケールを保持して再構成
            let local_mat = Mat4::from_scale_rotation_translation(
                skin.rest_local_scales[bone_idx],
                local_rot,
                local_trans,
            );
            globals[bone_idx] = parent_global * local_mat;
        } else {
            // 非アニメーションボーン: 生のローカル行列を使用（分解誤差を回避）
            globals[bone_idx] = parent_global * skin.rest_local_mats[bone_idx];
        }

        // 子ボーンを再帰処理
        for &child_idx in &skin.bone_children[bone_idx] {
            Self::compute_global_recursive_static(skin, animation, current_time, child_idx, globals[bone_idx], globals, computed);
        }
    }
}

/// IrModel と GpuModel からスキニングデータを構築
fn build_skinning_data(ir: &IrModel, gpu_model: &GpuModel, animation: &VrmaAnimation) -> SkinningData {
    let g2g = gpu_model.global_to_gpu_map();
    let gpu_vert_count = gpu_model.base_vertices().len();

    // GPU頂点ごとのボーンウェイトを構築
    let mut vertex_weights: Vec<VertexSkinWeight> = (0..gpu_vert_count)
        .map(|_| VertexSkinWeight { bones: [(0, 0.0); 4] })
        .collect();

    let mut global_vi = 0usize;
    for mesh in &ir.meshes {
        for v in &mesh.vertices {
            if global_vi < g2g.len() {
                let gpu_vi = g2g[global_vi] as usize;
                if gpu_vi < gpu_vert_count {
                    // まだウェイトが設定されていない場合のみ設定
                    if vertex_weights[gpu_vi].bones[0].1 == 0.0 {
                        let mut bones = [(0usize, 0.0f32); 4];
                        for (k, &(bi, w)) in v.weights.iter().take(4).enumerate() {
                            bones[k] = (bi, w);
                        }
                        vertex_weights[gpu_vi] = VertexSkinWeight { bones };
                    }
                }
            }
            global_vi += 1;
        }
    }

    // レストポーズのローカル変換を計算
    let bone_count = ir.bones.len();
    let mut rest_local_mats = vec![Mat4::IDENTITY; bone_count];
    let mut rest_local_rotations = vec![Quat::IDENTITY; bone_count];
    let mut rest_global_rotations = vec![Quat::IDENTITY; bone_count];
    let mut rest_local_translations = vec![Vec3::ZERO; bone_count];
    let mut rest_local_scales = vec![Vec3::ONE; bone_count];
    let bone_parents: Vec<Option<usize>> = ir.bones.iter().map(|b| b.parent).collect();
    let bone_children: Vec<Vec<usize>> = ir.bones.iter().map(|b| b.children.clone()).collect();
    let rest_global_mats: Vec<Mat4> = ir.bones.iter().map(|b| b.global_mat).collect();

    for (i, bone) in ir.bones.iter().enumerate() {
        let parent_global = bone.parent
            .map(|pi| ir.bones[pi].global_mat)
            .unwrap_or(Mat4::IDENTITY);
        let local_mat = parent_global.inverse() * bone.global_mat;
        rest_local_mats[i] = local_mat;
        let (scale, rot, trans) = local_mat.to_scale_rotation_translation();
        rest_local_rotations[i] = rot;
        rest_local_translations[i] = trans;
        rest_local_scales[i] = scale;

        // グローバル回転を抽出（リターゲティング用）
        let (_, global_rot, _) = bone.global_mat.to_scale_rotation_translation();
        rest_global_rotations[i] = global_rot;
    }

    // ボーン名 → ボーンインデックスのマッピング（マッチモードに依存）
    let mut bone_name_to_idx: HashMap<String, usize> = HashMap::new();
    match animation.match_mode {
        BoneMatchMode::Humanoid => {
            // VRMA: ヒューマノイドボーン名でマッチ
            for (i, bone) in ir.bones.iter().enumerate() {
                if let Some(ref vrm_name) = bone.vrm_bone_name {
                    bone_name_to_idx.insert(vrm_name.clone(), i);
                }
            }
        }
        BoneMatchMode::NodeName => {
            // GLB/glTF/FBX: ノード名で直接マッチ
            // アニメーション内のチャネル名と IrBone の name/name_en を照合
            let anim_bone_names: std::collections::HashSet<&str> = animation.bone_channels
                .keys().map(|s| s.as_str()).collect();

            for (i, bone) in ir.bones.iter().enumerate() {
                // 完全一致（name_en → name の優先順）
                if anim_bone_names.contains(bone.name_en.as_str()) {
                    bone_name_to_idx.insert(bone.name_en.clone(), i);
                } else if anim_bone_names.contains(bone.name.as_str()) {
                    bone_name_to_idx.insert(bone.name.clone(), i);
                }
            }

            // マッチしなかったチャネルをファジーマッチ（サフィックス一致）
            let matched_names: std::collections::HashSet<String> = bone_name_to_idx
                .keys().cloned().collect();
            for anim_name in &anim_bone_names {
                if matched_names.contains(*anim_name) {
                    continue;
                }
                // "Armature_Hips" → "Hips" のようなサフィックスマッチ
                for (i, bone) in ir.bones.iter().enumerate() {
                    if bone_name_to_idx.values().any(|&idx| idx == i) {
                        continue;
                    }
                    let matches = anim_name.ends_with(&bone.name_en)
                        || anim_name.ends_with(&bone.name)
                        || bone.name_en.ends_with(anim_name)
                        || bone.name.ends_with(anim_name);
                    if matches {
                        bone_name_to_idx.insert(anim_name.to_string(), i);
                        break;
                    }
                }
            }

            log::info!(
                "ボーンマッチング: {}/{}ch マッチ",
                bone_name_to_idx.len(),
                animation.bone_channels.len(),
            );
        }
    }

    // 表情名 → モーフインデックスのマッピング
    let mut expr_name_to_morph: HashMap<String, usize> = HashMap::new();
    for (i, morph) in ir.morphs.iter().enumerate() {
        // VRM表情名（英語名）とモーフ名の両方で照合
        if !morph.name_en.is_empty() {
            expr_name_to_morph.insert(morph.name_en.clone(), i);
        }
        if !morph.name.is_empty() && !expr_name_to_morph.contains_key(&morph.name) {
            expr_name_to_morph.insert(morph.name.clone(), i);
        }
    }

    // 逆引きマップ
    let bone_idx_to_name: HashMap<usize, String> = bone_name_to_idx
        .iter()
        .map(|(name, &idx)| (idx, name.clone()))
        .collect();

    SkinningData {
        vertex_weights,
        rest_global_mats,
        rest_local_mats,
        rest_local_rotations,
        rest_global_rotations,
        rest_local_translations,
        rest_local_scales,
        bone_parents,
        bone_children,
        bone_idx_to_name,
        expr_name_to_morph,
        is_vrm0: ir.source_format.is_vrm0(),
    }
}

// 座標変換関数は crate::convert::coord に統一
// pmx_pos_to_gltf, gltf_pos_to_pmx_unified, pmx_normal_to_gltf, gltf_normal_to_pmx_unified

