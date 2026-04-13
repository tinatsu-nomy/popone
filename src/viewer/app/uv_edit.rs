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

/// 頂点単位 UV 編集の状態（ViewerApp に 1 つだけ保持）。
#[derive(Debug, Default)]
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
    /// ドラッグ開始時点での選択頂点の UV（`start_uv + cumulative_cursor_delta` 方式で
    /// 過加算を防ぐため、review_result_02 [P1] 対応）。ドラッグ終了でクリア。
    pub drag_start_uvs: HashMap<VertexKey, [f32; 2]>,
    /// ドラッグ開始時のカーソル UV 位置（キャンバス座標から変換済み）。
    /// `None` = ドラッグ外。
    pub drag_press_uv: Option<[f32; 2]>,
    /// ロード後に persistence 側から受け取った復元予定エントリ。
    /// `apply_pending_restore` 呼び出し後は `None` に戻る。
    pub pending_restore: Option<Vec<VertexUvOverrideEntry>>,
}

impl UvEditState {
    /// 新規ロード時にクリアする（履歴復元エントリは呼び出し側で別途セット）。
    pub fn reset(&mut self) {
        self.overrides.clear();
        self.selected.clear();
        self.active_material = 0;
        self.dragging = false;
        self.drag_start_uvs.clear();
        self.drag_press_uv = None;
        self.pending_restore = None;
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
