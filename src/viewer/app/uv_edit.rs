//! 頂点単位 UV 編集の状態管理 (v0.5.5 / Phase 1)。
//!
//! 材質編集ドロワー（`material_edit.rs`）が「材質単位」の差分を持つのに対し、こちらは
//! 「頂点単位」の UV 差分を持つ。キーは `(mesh_index, vertex_index_in_mesh)` の組で、
//! UV0 のみを対象とする（UV1 / モーフ UV は Phase 3 以降）。
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

use crate::intermediate::types::IrModel;

use super::persistence::VertexUvOverrideEntry;

/// 頂点編集のキー: `(mesh_index, vertex_index_in_mesh)`。
pub type VertexKey = (u32, u32);

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
        }
    }
}

impl UvEditState {
    /// 新規ロード時にクリアする（履歴復元エントリは呼び出し側で別途セット）。
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
        for (&(mi, vi), &uv) in &entry.before {
            write_uv_to_ir(ir, mi, vi, uv);
            if self.matches_pristine((mi, vi), uv) {
                self.overrides.remove(&(mi, vi));
            } else {
                self.overrides.insert((mi, vi), uv);
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
        for (&(mi, vi), &uv) in &entry.after {
            write_uv_to_ir(ir, mi, vi, uv);
            if self.matches_pristine((mi, vi), uv) {
                self.overrides.remove(&(mi, vi));
            } else {
                self.overrides.insert((mi, vi), uv);
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

    /// 永続化エントリ（JSON 出力用）を生成する。メッシュ / 頂点Index 昇順にソート。
    pub fn to_entries(&self) -> Vec<VertexUvOverrideEntry> {
        let mut out: Vec<VertexUvOverrideEntry> = self
            .overrides
            .iter()
            .map(|(&(mi, vi), &uv)| VertexUvOverrideEntry {
                mesh_index: mi,
                vertex_index: vi,
                uv,
            })
            .collect();
        out.sort_by_key(|e| (e.mesh_index, e.vertex_index));
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
            if write_uv_to_ir(ir, e.mesh_index, e.vertex_index, e.uv) {
                self.overrides.insert((e.mesh_index, e.vertex_index), e.uv);
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
        for (&(mi, vi), &uv) in &self.overrides {
            write_uv_to_ir(ir, mi, vi, uv);
        }
    }
}

/// IR の指定メッシュ/頂点に UV を書き込む。範囲外なら `false` を返して何もしない。
fn write_uv_to_ir(ir: &mut IrModel, mesh_idx: u32, vert_idx: u32, uv: [f32; 2]) -> bool {
    let Some(mesh) = ir.meshes.get_mut(mesh_idx as usize) else {
        return false;
    };
    let verts = mesh.vertices_mut();
    let Some(v) = verts.get_mut(vert_idx as usize) else {
        return false;
    };
    v.uv = glam::Vec2::from_array(uv);
    true
}
