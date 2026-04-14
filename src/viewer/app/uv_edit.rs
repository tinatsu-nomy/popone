//! 頂点単位 UV 編集の状態管理 (v0.5.5 / Phase 1 / Phase 3 A-1)。
//!
//! 材質編集ドロワー（`material_edit.rs`）が「材質単位」の差分を持つのに対し、こちらは
//! 「頂点単位」の UV 差分を持つ。キーは `(mesh_index, vertex_index_in_mesh, uv_set)` の
//! 3 要素で、`uv_set` は 0 = UV0 (`IrVertex.uv`), 1 = UV1 (`IrMesh.uvs1`) を指す。
//! UV セットが違えば overrides / selected / undo 履歴がすべて別空間として扱われるため、
//! UV0 編集中の選択集合と UV1 編集中の選択集合が混線することはない。
//!
//! ## 設計判断
//!
//! - **IR を単一真実源とする**: `apply_to_ir` で直接 `IrMesh.vertices_mut()` に書き込み、
//!   再エクスポート（PMX writer 等）が IR をそのまま読めば自動で編集結果が反映される。
//! - **overrides は永続化専用**: 履歴ファイル (`popone_history.json`) への書き戻しと、
//!   リロード時の復元に用いる。IR 側が既に編集済みでも、overrides にエントリが残って
//!   いれば「編集された頂点」として PSD 出力・再計算の対象として扱える。
//! - **pristine 不要**: Phase 1 では undo を提供しないため、編集前の UV を記憶しない。
//!   Phase 2 で undo / リセット対応時に追加する。

use std::collections::{HashMap, HashSet};

use crate::intermediate::types::{IrModel, IrMorphKind};

use super::persistence::VertexUvOverrideEntry;

/// 頂点編集のキー: `(mesh_index, vertex_index_in_mesh, uv_set)`。
/// `uv_set = 0` なら UV0 (`IrVertex.uv`)、`uv_set = 1` なら UV1 (`IrMesh.uvs1`) を指す。
/// 将来 UV2/UV3 を足すなら同じ u8 スロットの 2..= を使う余地がある。
pub type VertexKey = (u32, u32, u8);

/// undo/redo の 1 コマンド (v0.5.5 Phase 2-5)。
///
/// ドラッグ 1 回単位で記録し、`before` (変更前) / `after` (変更後) の 2 つの
/// UV マップを持つ。`before` のキーは 必ず `after` のキーと一致する（同じ頂点集合）。
#[derive(Debug, Clone)]
pub struct UvUndoEntry {
    pub before: HashMap<VertexKey, [f32; 2]>,
    pub after: HashMap<VertexKey, [f32; 2]>,
}

/// undo スタックの最大エントリ数（古いものから破棄）。
pub const UV_UNDO_MAX: usize = 50;

/// ドラッグモード (v0.5.5 Phase 2-2)。
/// `drag_started()` 時に決定し、`dragged()` の分岐で使う。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum UvDragMode {
    /// ドラッグ外
    #[default]
    None,
    /// 選択頂点を平行移動（プレス位置が頂点近傍 12 px 以内）
    Move,
    /// 矩形選択（プレス位置が頂点から遠い場合）
    Rect,
}

/// 矩形選択の動作モード (v0.5.5 Phase 3 / A-4)。`drag_started()` 時の修飾キーで決定する。
/// Alt は Move モードの「回転」と競合するため Rect モードでは使わず、除外は Ctrl を使う。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum UvRectBehavior {
    /// 既存選択をクリアして矩形内頂点で置換（無修飾、Phase 2-2 の挙動）
    #[default]
    Replace,
    /// 既存選択に矩形内頂点を追加（Shift+ドラッグ）
    Add,
    /// 既存選択から矩形内頂点を除外（Ctrl+ドラッグ）
    Subtract,
}

/// 2D ギズモハンドルによるドラッグ操作の種別 (v0.5.5 Phase 3 / A-5)。
/// `drag_started()` 時に「press 位置がハンドル内か」を判定して決定する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UvGizmoAction {
    /// 選択 bbox の角ハンドルをドラッグ（スケール操作、ピボットは反対角）。
    /// `sign_u`, `sign_v` は掴んだ角を示す: (1,1)=u_max/v_max、(-1,-1)=u_min/v_min
    /// 反対角は `(-sign_u, -sign_v)` で求められる。
    ScaleCorner { sign_u: i8, sign_v: i8 },
    /// 回転ハンドル（bbox 上辺外側）をドラッグ。ピボットは選択 bbox の中心。
    Rotate,
}

/// 頂点単位 UV 編集の状態（ViewerApp に 1 つだけ保持）。
///
/// `Default` は手書き（`view_zoom` の初期値を 1.0 にするため）。
#[derive(Debug)]
pub struct UvEditState {
    /// 編集された頂点の現在 UV。IR へ反映する前の保留中の差分と、既に反映済みの差分の
    /// どちらも同じ経路で記録する（保存時に IR から読み直すのではなく、ここを信頼する）。
    pub overrides: HashMap<VertexKey, [f32; 2]>,
    /// UI キャンバス上での選択頂点。
    pub selected: HashSet<VertexKey>,
    /// キャンバスで編集対象としている材質 Index（`ir.materials` のインデックス）。
    pub active_material: usize,
    /// ドラッグ中フラグ（`true` の間は GPU 書き換えを遅延する）。
    pub dragging: bool,
    /// 現在のドラッグモード (Phase 2-2 で追加)。
    pub drag_mode: UvDragMode,
    /// ドラッグ開始時点での選択頂点の UV（`start_uv + cumulative_cursor_delta` 方式で
    /// 過加算を防ぐため、review_result_02 [P1] 対応）。ドラッグ終了でクリア。
    pub drag_start_uvs: HashMap<VertexKey, [f32; 2]>,
    /// ドラッグ開始時のカーソル UV 位置（キャンバス座標から変換済み）。
    /// `None` = ドラッグ外。
    pub drag_press_uv: Option<[f32; 2]>,
    /// ドラッグ開始時に算出した選択頂点群の bbox 中心（Phase 2-4 の回転/スケールピボット）。
    /// `None` = ピボット未設定（選択 0 件 or Rect モード等）。
    pub drag_pivot: Option<[f32; 2]>,
    /// ロード後に persistence 側から受け取った復元予定エントリ。
    /// `apply_pending_restore` 呼び出し後は `None` に戻る。
    pub pending_restore: Option<Vec<VertexUvOverrideEntry>>,
    /// キャンバスの表示オフセット (UV 空間) — キャンバス左上端に置く UV 座標。
    /// デフォルト `[0.0, 0.0]` で UV 原点がキャンバス左上に一致する (Phase 2-3)。
    pub view_offset: [f32; 2],
    /// キャンバスのズーム倍率。`1.0` で UV [0,1] がちょうどキャンバスにフィットする。
    /// ホイールでカーソル位置中心にスケーリング、0.1〜32.0 の範囲にクランプ (Phase 2-3)。
    pub view_zoom: f32,
    /// undo スタック（新しいほど末尾、Phase 2-5）。
    pub undo_stack: Vec<UvUndoEntry>,
    /// redo スタック（undo で捨てたエントリを保持、新しいほど末尾）。
    /// 新しい編集が push_undo されると自動でクリアされる。
    pub redo_stack: Vec<UvUndoEntry>,
    /// 編集前（初回ドラッグ時点）の UV を頂点単位で記録する (review_result_05 [P2])。
    /// `apply_undo` / `apply_redo` で「pristine に戻った頂点」を `overrides` から削除する
    /// 判定に使う。メモリは「一度でも編集された頂点」分のみで済むため常に軽量。
    pub pristine_uvs: HashMap<VertexKey, [f32; 2]>,
    /// 矩形選択の現在の動作モード (Phase 3 / A-4)。drag_started で決定、drag_stopped でリセット。
    pub rect_behavior: UvRectBehavior,
    /// 矩形選択開始時点の `selected` スナップショット (Phase 3 / A-4)。
    /// Add/Subtract モードで「initial ± rect_inside」の再計算基点に使う。
    pub rect_initial_selected: HashSet<VertexKey>,
    /// UV 編集ウィンドウを OS の独立ウィンドウとして分離する (Phase 3 / A-3)。
    /// `true` = `ctx.show_viewport_immediate` で別ネイティブウィンドウ。
    /// `false` = 従来通り `egui::Window` でメインウィンドウ内のフローティング。
    /// セッション中のユーザー設定として維持し、リロード (`reset`) では変更しない。
    pub detached: bool,
    /// 現在編集対象の UV セット (Phase 3 / A-1)。`0 = UV0`, `1 = UV1`。
    /// ComboBox から切り替え、キャンバス描画 / ピック / ドラッグ / Ctrl+A はすべて
    /// この値に応じた頂点のみを対象とする。selected / overrides 自体は
    /// `VertexKey` にチャネル情報を含むため、切り替えたときも別集合として温存される。
    pub active_uv_set: u8,
    /// 2D ギズモハンドル経由でドラッグ中のアクション (Phase 3 / A-5)。
    /// `drag_started()` 時に「press 位置がハンドル上か」を判定して Some にセット、
    /// ドラッグ終了でクリア。`Some` の間は修飾キーよりも優先してスケール/回転を適用する。
    pub gizmo_action: Option<UvGizmoAction>,
    /// 現在編集対象の UV モーフ (Phase 3 / A-2)。
    /// `None` = ベース UV 編集（従来通り `IrVertex.uv` / `IrMesh.uvs1` を直接触る）。
    /// `Some(idx)` = `ir.morphs[idx]` が `IrMorphKind::Uv` である前提で、そのモーフの
    /// 頂点別オフセットを編集する。read/write は `read_displayed_uv` / `write_displayed_uv`
    /// 経由で、キャンバスには「ベース + 現在のモーフオフセット (weight=1 相当)」が表示される。
    pub active_morph: Option<usize>,
    /// モーフ編集モード進入時に退避した `app.morph_weights[active_morph]` の元値 (v0.5.6)。
    /// 編集モード中は対象モーフのウェイトが 1.0 に強制されるため、終了時にこの値で復元する。
    /// 復元/再設定は `switch_active_morph` ヘルパー経由でのみ行うこと（呼び出し側が直接
    /// `active_morph` を書き換えると退避値が消えて復元できなくなる）。
    pub morph_weight_saved: Option<f32>,
}

impl Default for UvEditState {
    fn default() -> Self {
        Self {
            overrides: HashMap::new(),
            selected: HashSet::new(),
            active_material: 0,
            dragging: false,
            drag_mode: UvDragMode::None,
            drag_start_uvs: HashMap::new(),
            drag_press_uv: None,
            drag_pivot: None,
            pending_restore: None,
            view_offset: [0.0, 0.0],
            view_zoom: 1.0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            pristine_uvs: HashMap::new(),
            rect_behavior: UvRectBehavior::Replace,
            rect_initial_selected: HashSet::new(),
            detached: false,
            active_uv_set: 0,
            gizmo_action: None,
            active_morph: None,
            morph_weight_saved: None,
        }
    }
}

impl UvEditState {
    /// 新規ロード時にクリアする（履歴復元エントリは呼び出し側で別途セット）。
    /// `detached` はユーザー設定としてリロード越しに維持するので、ここでは触らない。
    pub fn reset(&mut self) {
        self.overrides.clear();
        self.selected.clear();
        self.active_material = 0;
        self.dragging = false;
        self.drag_mode = UvDragMode::None;
        self.drag_start_uvs.clear();
        self.drag_press_uv = None;
        self.drag_pivot = None;
        self.pending_restore = None;
        self.view_offset = [0.0, 0.0];
        self.view_zoom = 1.0;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.pristine_uvs.clear();
        self.rect_behavior = UvRectBehavior::Replace;
        self.rect_initial_selected.clear();
        self.active_uv_set = 0;
        self.gizmo_action = None;
        self.active_morph = None;
        // リロード時は `morph_weights` 自体が再構築されるため、退避値を保持しても
        // 復元先が存在しない（古い IR の index）。確実に破棄する。
        self.morph_weight_saved = None;
    }

    /// `active_morph` を切り替え、ウェイトの退避/復元を行う (v0.5.6)。
    ///
    /// - 旧 `active_morph` が `Some` だった場合、退避値で `weights` を復元する。
    /// - 新 `active_morph` が `Some` の場合、現在のウェイトを退避した上で `1.0` にセットする。
    /// - `new_morph == self.active_morph` の場合は何もせず `false` を返す。
    ///
    /// 戻り値: モードが切り替わったかどうか（呼び出し側が `morph_dirty = true` や
    /// 選択集合のクリア等の副作用を実行する判定に使う）。
    /// `weights` の境界外 index は静かに無視する（履歴復元等で IR と weights が
    /// 一時的にズレている可能性を考慮）。
    pub fn switch_active_morph(&mut self, new_morph: Option<usize>, weights: &mut [f32]) -> bool {
        if new_morph == self.active_morph {
            return false;
        }
        if let (Some(old_idx), Some(saved)) = (self.active_morph, self.morph_weight_saved.take()) {
            if let Some(w) = weights.get_mut(old_idx) {
                *w = saved;
            }
        }
        if let Some(new_idx) = new_morph {
            if let Some(w) = weights.get_mut(new_idx) {
                self.morph_weight_saved = Some(*w);
                *w = 1.0;
            }
        }
        self.active_morph = new_morph;
        true
    }

    /// 頂点 UV の pristine を記録する（初回ドラッグ開始時に呼ぶ）。
    /// 既に記録済みなら何もしない（`or_insert` セマンティクス）。
    pub fn record_pristine(&mut self, key: VertexKey, uv: [f32; 2]) {
        self.pristine_uvs.entry(key).or_insert(uv);
    }

    /// `uv` が pristine と一致するかを判定する（review_result_05 [P2]）。
    /// pristine 未記録なら `false`（= 一度も編集していない扱いにはしないで安全側に倒す）。
    fn matches_pristine(&self, key: VertexKey, uv: [f32; 2]) -> bool {
        self.pristine_uvs.get(&key).is_some_and(|p| *p == uv)
    }

    /// undo エントリを記録する。`before == after` の空操作なら何もしない。
    /// 新規 push で redo スタックは破棄される (標準の undo/redo セマンティクス)。
    pub fn push_undo(
        &mut self,
        before: HashMap<VertexKey, [f32; 2]>,
        after: HashMap<VertexKey, [f32; 2]>,
    ) {
        if before == after {
            return;
        }
        self.undo_stack.push(UvUndoEntry { before, after });
        if self.undo_stack.len() > UV_UNDO_MAX {
            let drop_n = self.undo_stack.len() - UV_UNDO_MAX;
            self.undo_stack.drain(..drop_n);
        }
        self.redo_stack.clear();
    }

    /// undo を適用する。IR の UV を `before` に書き戻し、`overrides` にも反映する。
    /// pristine と一致した頂点は「未編集状態に戻った」として `overrides` から削除する
    /// (review_result_05 [P2])。戻り値: 適用されたら `true`、スタックが空なら `false`。
    pub fn apply_undo(&mut self, ir: &mut IrModel) -> bool {
        let Some(entry) = self.undo_stack.pop() else {
            return false;
        };
        for (&(mi, vi, chan), &uv) in &entry.before {
            write_uv_to_ir(ir, mi, vi, uv, chan);
            if self.matches_pristine((mi, vi, chan), uv) {
                self.overrides.remove(&(mi, vi, chan));
            } else {
                self.overrides.insert((mi, vi, chan), uv);
            }
        }
        self.redo_stack.push(entry);
        true
    }

    /// redo を適用する。IR の UV を `after` に書き戻し、`overrides` にも反映する。
    /// pristine と一致するケース（= 初期値への redo）は overrides から削除する。
    pub fn apply_redo(&mut self, ir: &mut IrModel) -> bool {
        let Some(entry) = self.redo_stack.pop() else {
            return false;
        };
        for (&(mi, vi, chan), &uv) in &entry.after {
            write_uv_to_ir(ir, mi, vi, uv, chan);
            if self.matches_pristine((mi, vi, chan), uv) {
                self.overrides.remove(&(mi, vi, chan));
            } else {
                self.overrides.insert((mi, vi, chan), uv);
            }
        }
        self.undo_stack.push(entry);
        true
    }

    /// ビュー状態のみをリセットする（表示リセットボタン用）。編集差分は保持。
    pub fn reset_view(&mut self) {
        self.view_offset = [0.0, 0.0];
        self.view_zoom = 1.0;
    }

    /// 1 頂点の UV を記録する（IR の書き込みは別経路で `apply_to_ir` 経由）。
    pub fn set_uv(&mut self, key: VertexKey, uv: [f32; 2]) {
        self.overrides.insert(key, uv);
    }

    /// 永続化エントリ（JSON 出力用）を生成する。メッシュ / 頂点Index / UV セット昇順にソート。
    pub fn to_entries(&self) -> Vec<VertexUvOverrideEntry> {
        let mut out: Vec<VertexUvOverrideEntry> = self
            .overrides
            .iter()
            .map(|(&(mi, vi, chan), &uv)| VertexUvOverrideEntry {
                mesh_index: mi,
                vertex_index: vi,
                uv_set: chan,
                uv,
            })
            .collect();
        out.sort_by_key(|e| (e.mesh_index, e.vertex_index, e.uv_set));
        out
    }

    /// 保存済みエントリを復元予定としてセットする。実際の IR 反映は次回 `apply_pending_restore` で。
    pub fn stage_restore(&mut self, entries: Vec<VertexUvOverrideEntry>) {
        self.pending_restore = Some(entries);
    }

    /// `stage_restore` で受け取ったエントリを IR へ反映しつつ `overrides` に取り込む。
    /// IR 側のメッシュ/頂点数が変わっていた場合（リロード失敗など）は安全に無視する。
    pub fn apply_pending_restore(&mut self, ir: &mut IrModel) {
        let Some(entries) = self.pending_restore.take() else {
            return;
        };
        let mut applied = 0usize;
        let mut skipped = 0usize;
        for e in entries {
            if write_uv_to_ir(ir, e.mesh_index, e.vertex_index, e.uv, e.uv_set) {
                self.overrides
                    .insert((e.mesh_index, e.vertex_index, e.uv_set), e.uv);
                applied += 1;
            } else {
                skipped += 1;
            }
        }
        if applied + skipped > 0 {
            log::info!(
                "Restored vertex UV overrides: applied={} skipped={}",
                applied,
                skipped
            );
        }
    }

    /// 現在の `overrides` をすべて IR に書き戻す（リロード後の再適用用）。
    /// `apply_pending_restore` とは独立経路で、既に `overrides` に入っている分を反映する。
    pub fn apply_to_ir(&self, ir: &mut IrModel) {
        for (&(mi, vi, chan), &uv) in &self.overrides {
            write_uv_to_ir(ir, mi, vi, uv, chan);
        }
    }
}

/// 各メッシュのグローバル頂点オフセットを返す (Phase 3 A-2 helper)。
/// UV モーフは `(global_vertex_index, [f32; 4])` で頂点を指すため、`(mi, vi) ↔ global_vi`
/// 変換のために使う。計算量は O(meshes.len()) で毎フレーム呼んでも実用上問題ない。
pub fn mesh_global_offsets_of(ir: &IrModel) -> Vec<usize> {
    let mut offs = Vec::with_capacity(ir.meshes.len());
    let mut cum = 0usize;
    for m in &ir.meshes {
        offs.push(cum);
        cum += m.vertices.len();
    }
    offs
}

/// UV 編集キャンバス上に表示する UV を返す (Phase 3 A-2 helper)。
/// - `active_morph = None`: ベース UV (`IrVertex.uv` / `IrMesh.uvs1`) をそのまま返す
/// - `active_morph = Some(idx)`: ベース + モーフのオフセット (weight=1 相当) を返す
///   - モーフの `channel` が `chan` と不一致なら、そのモーフは対象外として base のみ返す
pub fn read_displayed_uv(
    ir: &IrModel,
    mi: u32,
    vi: u32,
    chan: u8,
    active_morph: Option<usize>,
    mesh_global_offsets: &[usize],
) -> Option<[f32; 2]> {
    let mesh = ir.meshes.get(mi as usize)?;
    let base = read_mesh_vertex_uv(mesh, vi as usize, chan)?;
    let Some(morph_idx) = active_morph else {
        return Some(base);
    };
    let morph = ir.morphs.get(morph_idx)?;
    let IrMorphKind::Uv { channel, offsets } = &morph.kind else {
        return Some(base);
    };
    if *channel != chan {
        return Some(base);
    }
    let global_vi = mesh_global_offsets.get(mi as usize)? + vi as usize;
    if let Some((_, off)) = offsets.iter().find(|(v, _)| *v == global_vi) {
        Some([base[0] + off[0], base[1] + off[1]])
    } else {
        Some(base)
    }
}

/// 表示 UV を新しい値に更新する (Phase 3 A-2 helper)。
/// - `active_morph = None`: `write_vertex_uv` にフォールバック（ベース UV を直接書き換え）
/// - `active_morph = Some(idx)`: ベース UV は維持し、モーフオフセットを `new_uv - base` で更新
pub fn write_displayed_uv(
    ir: &mut IrModel,
    mi: u32,
    vi: u32,
    new_uv: [f32; 2],
    chan: u8,
    active_morph: Option<usize>,
    mesh_global_offsets: &[usize],
) -> bool {
    let Some(morph_idx) = active_morph else {
        return write_vertex_uv(ir, mi, vi, new_uv, chan);
    };
    // split borrow: base UV はメッシュの不変借用で先に取得 → morph の可変借用に進む
    let base = {
        let Some(mesh) = ir.meshes.get(mi as usize) else {
            return false;
        };
        let Some(uv) = read_mesh_vertex_uv(mesh, vi as usize, chan) else {
            return false;
        };
        uv
    };
    let Some(&mesh_off) = mesh_global_offsets.get(mi as usize) else {
        return false;
    };
    let global_vi = mesh_off + vi as usize;
    let Some(morph) = ir.morphs.get_mut(morph_idx) else {
        return false;
    };
    let IrMorphKind::Uv { channel, offsets } = &mut morph.kind else {
        return false;
    };
    if *channel != chan {
        return false;
    }
    let du = new_uv[0] - base[0];
    let dv = new_uv[1] - base[1];
    // PMX UV モーフは 4 成分。UV0/UV1 編集で実際に意味があるのは xy のみで、zw は 0 で固定。
    let entry = [du, dv, 0.0, 0.0];
    if let Some(e) = offsets.iter_mut().find(|(v, _)| *v == global_vi) {
        e.1 = entry;
    } else {
        offsets.push((global_vi, entry));
    }
    true
}

/// 指定 UV モーフの offsets エントリ数を返す (Phase 3 A-2 helper、UI ステータス表示用)。
/// モーフが存在しないか `IrMorphKind::Uv` でない場合は `0`。
pub fn morph_uv_entry_count(ir: &IrModel, morph_idx: usize) -> usize {
    ir.morphs
        .get(morph_idx)
        .and_then(|m| match &m.kind {
            IrMorphKind::Uv { offsets, .. } => Some(offsets.len()),
            _ => None,
        })
        .unwrap_or(0)
}

/// IR の指定メッシュ/頂点/UV セットから UV を読み取る (Phase 3 A-1 helper)。
/// 存在しない場合 (範囲外、UV1 未所持メッシュ、unknown chan) は `None`。
pub fn read_mesh_vertex_uv(
    mesh: &crate::intermediate::types::IrMesh,
    vi: usize,
    chan: u8,
) -> Option<[f32; 2]> {
    match chan {
        0 => mesh.vertices.get(vi).map(|v| v.uv.to_array()),
        1 => {
            // UV1 を持つのは `uvs1.len() == vertices.len()` の時のみ。
            // 空 (UV1 なし) / サイズ不一致はいずれも UV1 不在扱い。
            if mesh.uvs1.len() == mesh.vertices.len() {
                mesh.uvs1.get(vi).copied()
            } else {
                None
            }
        }
        _ => None,
    }
}

/// 指定材質に属するメッシュのうち、少なくとも 1 つが UV1 を持つかを判定する
/// (Phase 3 A-1 helper)。ComboBox で UV1 選択肢を enable/disable する判定に使う。
pub fn material_has_uv1(ir: &IrModel, material_index: usize) -> bool {
    ir.meshes.iter().any(|m| {
        m.material_index == material_index && !m.uvs1.is_empty() && m.uvs1.len() == m.vertices.len()
    })
}

/// UI から呼ぶ公開版の UV 書き込み関数 (`write_uv_to_ir` の薄いラップ、Phase 3 A-1)。
pub fn write_vertex_uv(
    ir: &mut IrModel,
    mesh_idx: u32,
    vert_idx: u32,
    uv: [f32; 2],
    chan: u8,
) -> bool {
    write_uv_to_ir(ir, mesh_idx, vert_idx, uv, chan)
}

/// IR の指定メッシュ/頂点に UV を書き込む。範囲外 / UV セット未存在なら `false` を返す。
/// `chan = 0` → `IrVertex.uv` (UV0)、`chan = 1` → `IrMesh.uvs1[vi]` (UV1)。UV1 は
/// メッシュが `uvs1.len() == vertices.len()` のときのみ書き込み可（空の場合は UV1 なし）。
fn write_uv_to_ir(ir: &mut IrModel, mesh_idx: u32, vert_idx: u32, uv: [f32; 2], chan: u8) -> bool {
    let Some(mesh) = ir.meshes.get_mut(mesh_idx as usize) else {
        return false;
    };
    match chan {
        0 => {
            let verts = mesh.vertices_mut();
            let Some(v) = verts.get_mut(vert_idx as usize) else {
                return false;
            };
            v.uv = glam::Vec2::from_array(uv);
            true
        }
        1 => {
            let vcount = mesh.vertices.len();
            // UV1 が存在しない (空 or サイズ不一致) メッシュでは書き込みを諦める。
            // 自動的に拡張してしまうと VRM エクスポート時に「もともと UV1 が無かったメッシュが
            // UV1 持ちに変わる」という副作用が起きるため、書き込みは UV1 のある既存メッシュに限定。
            if mesh.uvs1.len() != vcount {
                return false;
            }
            let Some(slot) = mesh.uvs1.get_mut(vert_idx as usize) else {
                return false;
            };
            *slot = uv;
            true
        }
        _ => false,
    }
}
