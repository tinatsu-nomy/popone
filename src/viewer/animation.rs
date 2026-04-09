use std::collections::HashMap;
use std::sync::Arc;

use eframe::wgpu;
use glam::{Mat4, Quat, Vec3};

use crate::convert::coord::PMX_SCALE;
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
    /// レストポーズのボーングローバル逆行列（毎フレームの inverse() 回避用キャッシュ）
    rest_global_inv_mats: Vec<Mat4>,
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
    /// IrBone インデックス → VRM ヒューマノイドボーン名（逆引き）
    bone_idx_to_name: HashMap<usize, String>,
    /// VRM 表情名 → モーフインデックス
    expr_name_to_morph: HashMap<String, usize>,
    /// VRM 0.0 かどうか
    is_vrm0: bool,
    /// 付与データ（PMX回転付与・移動付与）
    grants: Vec<Option<GrantInfo>>,
    /// 付与処理順序（付与親が先に来るトポロジカル順、付与を持つボーンのみ）
    grant_order: Vec<usize>,
}

/// 付与情報（アニメーション用）
struct GrantInfo {
    parent_index: usize,
    ratio: f32,
    is_rotation: bool,
    is_move: bool,
    /// ローカル付与フラグ（true: 子ボーンのローカル空間でデルタを適用）
    is_local: bool,
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
    /// ボーンローカル行列の作業バッファ（付与処理用、毎フレーム alloc 回避）
    work_local_mats: Vec<Mat4>,
    /// 表情チャネル名 → モーフインデックスの事前マッピング（毎フレームの HashMap 走査回避）
    expr_mapping: Vec<(String, usize)>,
}

impl AnimationState {
    /// IrModel と GpuModel からアニメーション再生状態を構築
    pub fn new(animation: Arc<VrmaAnimation>, ir: &IrModel, gpu_model: &GpuModel) -> Self {
        let skin = build_skinning_data(ir, gpu_model, &animation);
        // 表情チャネル名 → モーフインデックスの事前マッピングを構築（毎フレームの HashMap 走査回避）
        let expr_mapping: Vec<(String, usize)> = animation
            .expression_channels
            .keys()
            .filter_map(|name| {
                skin.expr_name_to_morph
                    .get(name.as_str())
                    .map(|&idx| (name.clone(), idx))
            })
            .collect();
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
            work_local_mats: vec![Mat4::IDENTITY; skin.rest_global_mats.len()],
            cached_animated_globals: skin.rest_global_mats.clone(),
            expr_mapping,
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
        let delta = if forward {
            FRAME_DURATION
        } else {
            -FRAME_DURATION
        };
        self.current_time += delta;
        let (lo, hi) = self.effective_range();
        self.current_time = self.current_time.clamp(lo, hi);
    }

    /// 現在時刻の表情ウェイトをモーフウェイト配列に書き込む
    /// 戻り値: 何か変更があったか
    pub fn apply_expressions(&self, morph_weights: &mut [f32]) -> bool {
        let mut changed = false;
        for (expr_name, morph_idx) in &self.expr_mapping {
            if *morph_idx < morph_weights.len() {
                let w = self
                    .animation
                    .sample_expression(expr_name, self.current_time)
                    .unwrap_or(0.0);
                if (morph_weights[*morph_idx] - w).abs() > 1e-6 {
                    morph_weights[*morph_idx] = w;
                    changed = true;
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
        ir: &IrModel,
    ) {
        // グローバル行列を in-place で計算（alloc 回避）
        self.compute_animated_globals_inplace(ir);

        // 付与（grant）処理: 付与親の回転/移動をコピー
        self.apply_grants();

        // デルタ行列を作業バッファに計算し、PMX座標系に事前変換（alloc 回避）
        // M * delta * M でPMX空間のデルタ行列を得る（M はミラー行列、M² = I）
        // これにより頂点ループ内の pmx_pos_to_gltf / gltf_pos_to_pmx 変換を排除
        let bone_count = self.skin.rest_global_mats.len();
        self.work_deltas.resize(bone_count, Mat4::IDENTITY);
        let is_vrm0 = self.skin.is_vrm0;
        for i in 0..bone_count {
            let delta = self.cached_animated_globals[i] * self.skin.rest_global_inv_mats[i];
            self.work_deltas[i] = conjugate_delta_to_pmx(delta, is_vrm0);
        }

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

                // デルタ行列はPMX空間に事前変換済み → 直接適用
                let pmx_pos = Vec3::from(work[vi].position);
                work[vi].position = blended.transform_point3(pmx_pos).to_array();

                // 法線（PMX空間で直接変換）
                let pmx_normal = Vec3::from(work[vi].normal);
                let skinned_n = blended.transform_vector3(pmx_normal).normalize_or_zero();
                work[vi].normal = skinned_n.to_array();

                // 接線（tangent.w = handedness は変更しない、PMX空間で直接変換）
                let pmx_tangent = Vec3::from_slice(&work[vi].tangent[..3]);
                let skinned_t = blended.transform_vector3(pmx_tangent).normalize_or_zero();
                // Gram-Schmidt 再直交化: normal に対して tangent を直交射影
                let t_ortho =
                    (skinned_t - skinned_n * skinned_n.dot(skinned_t)).normalize_or_zero();
                work[vi].tangent = [t_ortho.x, t_ortho.y, t_ortho.z, work[vi].tangent[3]];
            }
        } // work の可変借用をここでドロップ

        // モーフを animated_vertices に直接適用（借用衝突回避）
        gpu_model.apply_morphs_to_animated(morph_weights);

        // GPU バッファに書き込み
        queue.write_buffer(
            &gpu_model.vertex_buf,
            0,
            bytemuck::cast_slice(gpu_model.current_vertices()),
        );
    }

    /// PMX 付与（grant）処理: 付与親ボーンの回転/移動をコピー
    ///
    /// PMX のボーンインデックス順に走査し、付与親の回転デルタ（レストからの差分）を
    /// 付与率に基づいてローカル行列に適用する。
    /// 適用後、グローバル行列をインデックス順に再計算して子孫に変更を伝播させる。
    fn apply_grants(&mut self) {
        let bone_count = self.skin.grants.len();
        if bone_count == 0 || self.skin.grant_order.is_empty() {
            return;
        }

        // フェーズ1: ローカル行列に付与デルタを適用（トポロジカル順で付与親が先に処理される）
        let mut grant_applied = false;
        for &i in &self.skin.grant_order {
            let Some(ref grant) = self.skin.grants[i] else {
                continue;
            };
            let gp = grant.parent_index;
            if gp >= bone_count {
                continue;
            }

            // 付与親のローカル行列から回転/移動を取得
            let (_, gp_rot, gp_trans) = self.work_local_mats[gp].to_scale_rotation_translation();
            let (my_scale, mut my_rot, mut my_trans) =
                self.work_local_mats[i].to_scale_rotation_translation();

            let mut changed = false;

            if grant.is_rotation {
                let gp_rest_rot = self.skin.rest_local_rotations[gp];
                let delta = gp_rest_rot.inverse() * gp_rot;
                let applied = if (grant.ratio - 1.0).abs() < 1e-6 {
                    delta
                } else {
                    Quat::IDENTITY.slerp(delta, grant.ratio)
                };
                if grant.is_local {
                    // ローカル付与: 子ボーンのレスト姿勢を基準にデルタを適用
                    // child_rot = child_rest_rot * slerp(IDENTITY, parent_delta, ratio)
                    let my_rest_rot = self.skin.rest_local_rotations[i];
                    my_rot = my_rest_rot * applied;
                } else {
                    // 非ローカル付与: 現在の回転にデルタを乗算（モデル空間）
                    my_rot = my_rot * applied;
                }
                changed = true;
            }

            if grant.is_move {
                let gp_rest_trans = self.skin.rest_local_translations[gp];
                let delta = gp_trans - gp_rest_trans;
                if grant.is_local {
                    // ローカル付与: デルタを子ボーンのローカル空間に変換して適用
                    let my_rest_rot = self.skin.rest_local_rotations[i];
                    let local_delta = my_rest_rot.inverse() * delta;
                    my_trans = self.skin.rest_local_translations[i] + local_delta * grant.ratio;
                } else {
                    // 非ローカル付与: デルタを直接加算（モデル空間）
                    my_trans += delta * grant.ratio;
                }
                changed = true;
            }

            if changed {
                self.work_local_mats[i] =
                    Mat4::from_scale_rotation_translation(my_scale, my_rot, my_trans);
                grant_applied = true;
            }
        }

        if !grant_applied {
            return;
        }

        // フェーズ2: グローバル行列をインデックス順に再計算
        // PMXはボーンインデックス順で親が先に来ることを保証するため、線形走査で正しく伝播する
        for i in 0..bone_count {
            let parent_global = self.skin.bone_parents[i]
                .map(|pi| self.cached_animated_globals[pi])
                .unwrap_or(Mat4::IDENTITY);
            self.cached_animated_globals[i] = parent_global * self.work_local_mats[i];
        }
    }

    /// VRMA のキーフレームからボーンのグローバル行列を in-place 計算（alloc 回避）
    fn compute_animated_globals_inplace(&mut self, ir: &IrModel) {
        let bone_count = self.skin.rest_global_mats.len();
        self.cached_animated_globals
            .resize(bone_count, Mat4::IDENTITY);
        self.work_computed.resize(bone_count, false);
        self.work_computed.fill(false);
        self.work_local_mats.resize(bone_count, Mat4::IDENTITY);
        for i in 0..bone_count {
            self.cached_animated_globals[i] = Mat4::IDENTITY;
            self.work_local_mats[i] = self.skin.rest_local_mats[i];
        }

        // ルートボーン（親なし）から階層を辿る
        for i in 0..bone_count {
            if self.skin.bone_parents[i].is_none() {
                Self::compute_global_recursive_static(
                    &self.skin,
                    &self.animation,
                    self.current_time,
                    i,
                    Mat4::IDENTITY,
                    &mut self.cached_animated_globals,
                    &mut self.work_computed,
                    &mut self.work_local_mats,
                    &ir.bones,
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
        local_mats: &mut [Mat4],
        bones: &[crate::intermediate::types::IrBone],
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

                        let mut normalized =
                            w_vrma * l_vrma.inverse() * anim_rot * w_vrma.inverse();

                        if skin.is_vrm0 {
                            normalized = Quat::from_xyzw(
                                -normalized.x,
                                normalized.y,
                                -normalized.z,
                                normalized.w,
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
                                -normalized.x,
                                normalized.y,
                                -normalized.z,
                                normalized.w,
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
                        let vrma_parent_rot =
                            vrma_rest.global_rotation * vrma_rest.local_rotation.inverse();
                        let mut delta_world = vrma_parent_rot * delta_local;
                        if skin.is_vrm0 {
                            delta_world = Vec3::new(-delta_world.x, delta_world.y, -delta_world.z);
                        }
                        let model_h = skin.rest_global_mats[bone_idx]
                            .transform_point3(Vec3::ZERO)
                            .y;
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
            local_mats[bone_idx] = local_mat;
            globals[bone_idx] = parent_global * local_mat;
        } else {
            // 非アニメーションボーン: 生のローカル行列を使用（分解誤差を回避）
            local_mats[bone_idx] = skin.rest_local_mats[bone_idx];
            globals[bone_idx] = parent_global * skin.rest_local_mats[bone_idx];
        }

        // 子ボーンを再帰処理
        for &child_idx in &bones[bone_idx].children {
            Self::compute_global_recursive_static(
                skin,
                animation,
                current_time,
                child_idx,
                globals[bone_idx],
                globals,
                computed,
                local_mats,
                bones,
            );
        }
    }
}

/// IrModel と GpuModel からスキニングデータを構築
fn build_skinning_data(
    ir: &IrModel,
    gpu_model: &GpuModel,
    animation: &VrmaAnimation,
) -> SkinningData {
    let g2g = gpu_model.global_to_gpu_map();
    let gpu_vert_count = gpu_model.base_vertices().len();

    // GPU頂点ごとのボーンウェイトを構築
    let mut vertex_weights: Vec<VertexSkinWeight> = (0..gpu_vert_count)
        .map(|_| VertexSkinWeight {
            bones: [(0, 0.0); 4],
        })
        .collect();

    let mut global_vi = 0usize;
    for mesh in &ir.meshes {
        for v in mesh.vertices.iter() {
            if global_vi < g2g.len() {
                let gpu_vi = g2g[global_vi] as usize;
                if gpu_vi < gpu_vert_count {
                    // まだウェイトが設定されていない場合のみ設定
                    if vertex_weights[gpu_vi].bones[0].1 == 0.0 {
                        vertex_weights[gpu_vi] = VertexSkinWeight { bones: v.weights };
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
    let rest_global_mats: Vec<Mat4> = ir.bones.iter().map(|b| b.global_mat).collect();
    let rest_global_inv_mats: Vec<Mat4> = rest_global_mats.iter().map(|m| m.inverse()).collect();

    for (i, bone) in ir.bones.iter().enumerate() {
        let parent_global = bone
            .parent
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
            let anim_bone_names: std::collections::HashSet<&str> =
                animation.bone_channels.keys().map(|s| s.as_str()).collect();

            for (i, bone) in ir.bones.iter().enumerate() {
                // 完全一致（name_en → name の優先順）
                if anim_bone_names.contains(bone.name_en.as_str()) {
                    bone_name_to_idx.insert(bone.name_en.clone(), i);
                } else if anim_bone_names.contains(bone.name.as_str()) {
                    bone_name_to_idx.insert(bone.name.clone(), i);
                }
            }

            // マッチしなかったチャネルをファジーマッチ（サフィックス一致）
            let matched_names: std::collections::HashSet<String> =
                bone_name_to_idx.keys().cloned().collect();
            let mut used_indices: std::collections::HashSet<usize> =
                bone_name_to_idx.values().copied().collect();
            for anim_name in &anim_bone_names {
                if matched_names.contains(*anim_name) {
                    continue;
                }
                // "Armature_Hips" → "Hips" のようなサフィックスマッチ
                for (i, bone) in ir.bones.iter().enumerate() {
                    if used_indices.contains(&i) {
                        continue;
                    }
                    let matches = anim_name.ends_with(&bone.name_en)
                        || anim_name.ends_with(&bone.name)
                        || bone.name_en.ends_with(anim_name)
                        || bone.name.ends_with(anim_name);
                    if matches {
                        bone_name_to_idx.insert(anim_name.to_string(), i);
                        used_indices.insert(i);
                        break;
                    }
                }
            }

            log::info!(
                "Bone matching: {}/{}ch matched",
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

    // 付与データ
    let grants: Vec<Option<GrantInfo>> = ir
        .bones
        .iter()
        .map(|b| {
            b.grant.as_ref().map(|g| GrantInfo {
                parent_index: g.parent_index,
                ratio: g.ratio,
                is_rotation: g.is_rotation,
                is_move: g.is_move,
                is_local: g.is_local,
            })
        })
        .collect();

    // 付与処理順序をトポロジカルソートで事前計算
    // PMX仕様では付与親が先のインデックスに来ることが期待されるが、
    // 不正なPMXファイルに対する防御としてトポロジカル順序を保証する。
    let grant_order = {
        let n = grants.len();
        // 付与を持つボーンのインデックスを収集
        let has_grant: Vec<usize> = (0..n).filter(|&i| grants[i].is_some()).collect();
        if has_grant.is_empty() {
            Vec::new()
        } else {
            // 付与依存グラフでトポロジカルソート（カーン法）
            // 入次数: 付与親が付与を持つボーンなら、そのボーンへの辺がある
            let grant_set: std::collections::HashSet<usize> = has_grant.iter().copied().collect();
            let mut in_degree: HashMap<usize, usize> = has_grant.iter().map(|&i| (i, 0)).collect();
            for &i in &has_grant {
                let Some(grant) = grants[i].as_ref() else {
                    continue;
                };
                let gp = grant.parent_index;
                if grant_set.contains(&gp) {
                    *in_degree.entry(i).or_default() += 1;
                }
            }
            let mut queue: std::collections::VecDeque<usize> = has_grant
                .iter()
                .filter(|&&i| in_degree[&i] == 0)
                .copied()
                .collect();
            let mut order = Vec::with_capacity(has_grant.len());
            while let Some(i) = queue.pop_front() {
                order.push(i);
                // i を付与親とするボーンの入次数を減らす
                for &j in &has_grant {
                    if grants[j].as_ref().is_some_and(|g| g.parent_index == i) {
                        let Some(deg) = in_degree.get_mut(&j) else {
                            continue;
                        };
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(j);
                        }
                    }
                }
            }
            // 循環参照がある場合はフォールバック（残りをインデックス順で追加）
            if order.len() < has_grant.len() {
                log::warn!(
                    "Grant dependency has circular reference: {} of {} bones unresolved",
                    has_grant.len(),
                    has_grant.len() - order.len()
                );
                let in_order: std::collections::HashSet<usize> = order.iter().copied().collect();
                for &i in &has_grant {
                    if !in_order.contains(&i) {
                        order.push(i);
                    }
                }
            }
            order
        }
    };

    SkinningData {
        vertex_weights,
        rest_global_mats,
        rest_global_inv_mats,
        rest_local_mats,
        rest_local_rotations,
        rest_global_rotations,
        rest_local_translations,
        rest_local_scales,
        bone_parents,
        bone_idx_to_name,
        expr_name_to_morph,
        is_vrm0: ir.source_format.is_vrm0(),
        grants,
        grant_order,
    }
}

/// デルタ行列を glTF 空間から PMX 空間に変換する。
///
/// 元の変換: gltf_pos_to_pmx(delta.transform_point3(pmx_pos_to_gltf(pmx_pos)))
/// = S * M * (R * M * p / S + t) = M * R * M * p + S * M * t
///
/// ここで R = delta の 3x3 回転部分、t = delta の平行移動、
/// M = ミラー行列（自己逆行列）、S = PMX_SCALE。
///
/// PMX空間デルタ行列の構成:
/// - 3x3 部分: M * R * M（共役変換、S と 1/S が打ち消し合う）
/// - 平行移動: S * M * t（glTFメートル単位をPMXスケールに変換 + ミラー）
///
/// 3x3 部分は符号反転のみ、平行移動は符号反転 + スケール乗算で計算できる。
/// VRM 1.0: M = diag(1,1,-1), VRM 0.0: M = diag(-1,1,1)
///
/// glam は列優先で `c[col][row]` のレイアウト。
/// M*R*M の行i列j = mi * R[i][j] * mj（mi はミラー対角要素）
#[inline]
fn conjugate_delta_to_pmx(delta: Mat4, is_vrm0: bool) -> Mat4 {
    let c = delta.to_cols_array_2d(); // c[col][row]
    if is_vrm0 {
        // M = diag(-1, 1, 1)
        // 3x3: 行0と列0の符号反転（[0][0]は2回で戻る）
        // 平行移動 (c[3][0..3]): M*t*S = (-tx*S, ty*S, tz*S)
        let s = PMX_SCALE;
        Mat4::from_cols_array_2d(&[
            [c[0][0], -c[0][1], -c[0][2], c[0][3]],
            [-c[1][0], c[1][1], c[1][2], c[1][3]],
            [-c[2][0], c[2][1], c[2][2], c[2][3]],
            [-c[3][0] * s, c[3][1] * s, c[3][2] * s, c[3][3]],
        ])
    } else {
        // M = diag(1, 1, -1)
        // 3x3: 行2と列2の符号反転（[2][2]は2回で戻る）
        // 平行移動 (c[3][0..3]): M*t*S = (tx*S, ty*S, -tz*S)
        let s = PMX_SCALE;
        Mat4::from_cols_array_2d(&[
            [c[0][0], c[0][1], -c[0][2], c[0][3]],
            [c[1][0], c[1][1], -c[1][2], c[1][3]],
            [-c[2][0], -c[2][1], c[2][2], c[2][3]],
            [c[3][0] * s, c[3][1] * s, -c[3][2] * s, c[3][3]],
        ])
    }
}
