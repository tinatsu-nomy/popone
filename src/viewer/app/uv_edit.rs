//! Per-vertex UV editing state (v0.5.5 / Phase 1 / Phase 3 A-1).
//!
//! While the material edit drawer (`material_edit.rs`) carries per-material deltas,
//! this module carries per-vertex UV deltas. The key is the 3-tuple
//! `(mesh_index, vertex_index_in_mesh, uv_set)` where `uv_set = 0 = UV0
//! (`IrVertex.uv`)`, `uv_set = 1 = UV1 (`IrMesh.uvs1`)`. Different UV sets keep
//! their own overrides / selected / undo history, so the UV0 selection set never
//! crosses with the UV1 selection set.
//!
//! ## Design decisions
//!
//! - **IR is the single source of truth**: `apply_to_ir` writes directly into
//!   `IrMesh.vertices_mut()`, so a re-export (PMX writer etc.) automatically picks
//!   up the edit just by reading the IR.
//! - **`overrides` is for persistence only**: used to write back to the history
//!   file (`popone_history.json`) and to restore on reload. Even when the IR is
//!   already edited, an entry remaining in `overrides` flags the vertex as
//!   "edited" so it can participate in PSD output / recomputation.
//! - **No pristine snapshot**: Phase 1 does not provide undo, so the pre-edit
//!   UV is not stored. Added in Phase 2 alongside undo / reset.

use std::collections::{HashMap, HashSet};

use crate::intermediate::types::{IrModel, IrMorphKind};

use super::persistence::VertexUvOverrideEntry;

/// Vertex-edit key: `(mesh_index, vertex_index_in_mesh, uv_set)`.
/// `uv_set = 0` -> UV0 (`IrVertex.uv`), `uv_set = 1` -> UV1 (`IrMesh.uvs1`).
/// If UV2 / UV3 are added later, the same u8 slot has 2..= available.
pub type VertexKey = (u32, u32, u8);

/// One undo / redo command (v0.5.5 Phase 2-5).
///
/// Recorded per drag, holding two UV maps: `before` (pre-change) and `after`
/// (post-change). The keys in `before` always match those in `after` (same vertex set).
#[derive(Debug, Clone)]
pub struct UvUndoEntry {
    pub before: HashMap<VertexKey, [f32; 2]>,
    pub after: HashMap<VertexKey, [f32; 2]>,
}

/// Maximum entries on the undo stack (oldest are dropped first).
pub const UV_UNDO_MAX: usize = 50;

/// Drag mode (v0.5.5 Phase 2-2).
/// Decided in `drag_started()` and used by `dragged()` to branch.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum UvDragMode {
    /// Not dragging.
    #[default]
    None,
    /// Translate selected vertices (the press position is within 12 px of a vertex).
    Move,
    /// Rectangle select (the press position is far from any vertex).
    Rect,
}

/// Behavior of the rectangle selection (v0.5.5 Phase 3 / A-4). Decided in `drag_started()` from modifier keys.
/// Alt would conflict with Move-mode rotation, so it is not used here; Ctrl handles subtraction.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum UvRectBehavior {
    /// Replace the existing selection with the vertices inside the rectangle (no modifier; Phase 2-2 behavior).
    #[default]
    Replace,
    /// Add the vertices inside the rectangle to the existing selection (Shift + drag).
    Add,
    /// Remove the vertices inside the rectangle from the existing selection (Ctrl + drag).
    Subtract,
}

/// Kind of drag operation triggered by a 2D gizmo handle (v0.5.5 Phase 3 / A-5).
/// Decided in `drag_started()` based on whether the press position is inside a handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UvGizmoAction {
    /// Drag a corner handle of the selection bbox (scale; the pivot is the opposite corner).
    /// `sign_u`, `sign_v` indicate the grabbed corner: (1, 1) = u_max / v_max, (-1, -1) = u_min / v_min.
    /// The opposite corner is `(-sign_u, -sign_v)`.
    ScaleCorner { sign_u: i8, sign_v: i8 },
    /// Drag the rotation handle (above the bbox top edge). The pivot is the center of the selection bbox.
    Rotate,
}

/// Per-vertex UV editing state (one instance held by ViewerApp).
///
/// `Default` is hand-written so that `view_zoom` initializes to 1.0.
#[derive(Debug)]
pub struct UvEditState {
    /// The current UV of edited vertices. Both pending deltas (not yet pushed
    /// to the IR) and already-pushed deltas are recorded the same way (we trust
    /// this map at save time rather than re-reading from the IR).
    pub overrides: HashMap<VertexKey, [f32; 2]>,
    /// Selected vertices on the UI canvas.
    pub selected: HashSet<VertexKey>,
    /// Material index currently being edited on the canvas (an index into `ir.materials`).
    pub active_material: usize,
    /// Drag-in-progress flag (defers GPU writes while true).
    pub dragging: bool,
    /// Current drag mode (added in Phase 2-2).
    pub drag_mode: UvDragMode,
    /// UV of selected vertices at drag start (used by the
    /// `start_uv + cumulative_cursor_delta` scheme to avoid over-accumulation;
    /// review_result_02 [P1] fix). Cleared at drag end.
    pub drag_start_uvs: HashMap<VertexKey, [f32; 2]>,
    /// Cursor UV at drag start (already converted from canvas coordinates).
    /// `None` = not dragging.
    pub drag_press_uv: Option<[f32; 2]>,
    /// Bbox center of the selected vertices at drag start (Phase 2-4 rotation / scale pivot).
    /// `None` = no pivot set (selection is empty, or in Rect mode, etc.).
    pub drag_pivot: Option<[f32; 2]>,
    /// Restoration entries received from the persistence side after load.
    /// Cleared back to `None` once `apply_pending_restore` runs.
    pub pending_restore: Option<Vec<VertexUvOverrideEntry>>,
    /// Display offset of the canvas (in UV space) — the UV coordinate placed at
    /// the top-left of the canvas. Default `[0.0, 0.0]` matches UV origin to
    /// the canvas top-left (Phase 2-3).
    pub view_offset: [f32; 2],
    /// Zoom factor of the canvas. `1.0` fits UV [0, 1] exactly to the canvas.
    /// Wheel zoom centers on the cursor position, clamped to 0.1..=32.0 (Phase 2-3).
    pub view_zoom: f32,
    /// Undo stack (newest at the back; Phase 2-5).
    pub undo_stack: Vec<UvUndoEntry>,
    /// Redo stack (entries discarded by undo; newest at the back).
    /// Cleared automatically when a new edit is push_undo'd.
    pub redo_stack: Vec<UvUndoEntry>,
    /// Per-vertex UV at the time of the very first edit (review_result_05 [P2]).
    /// Used in `apply_undo` / `apply_redo` to decide which vertices have
    /// "returned to pristine" and should be removed from `overrides`. Memory is
    /// limited to vertices that were ever edited, so it stays light.
    pub pristine_uvs: HashMap<VertexKey, [f32; 2]>,
    /// Current rectangle-selection behavior (Phase 3 / A-4). Set in drag_started, reset in drag_stopped.
    pub rect_behavior: UvRectBehavior,
    /// Snapshot of `selected` at the moment the rectangle drag began (Phase 3 / A-4).
    /// In Add / Subtract mode it is the base point for "initial ± rect_inside" recomputation.
    pub rect_initial_selected: HashSet<VertexKey>,
    /// Detach the UV edit window into a standalone OS window (Phase 3 / A-3).
    /// `true` = `ctx.show_viewport_immediate` opens a separate native window.
    /// `false` = legacy floating `egui::Window` inside the main window.
    /// This is a per-session user preference and survives reload (`reset` does not touch it).
    pub detached: bool,
    /// UV set currently being edited (Phase 3 / A-1). `0 = UV0`, `1 = UV1`.
    /// Switched from a ComboBox; canvas drawing / picking / drag / Ctrl+A all
    /// target the vertices on this UV set only. `selected` / `overrides`
    /// themselves include the channel in `VertexKey`, so switching keeps the
    /// other set's data intact as a separate space.
    pub active_uv_set: u8,
    /// Action being dragged via a 2D gizmo handle (Phase 3 / A-5).
    /// Set to Some by `drag_started()` when the press is on a handle, cleared at drag end.
    /// While Some, scale / rotate take priority over the modifier-key behavior.
    pub gizmo_action: Option<UvGizmoAction>,
    /// UV morph currently being edited (Phase 3 / A-2).
    /// `None` = base-UV edit (touches `IrVertex.uv` / `IrMesh.uvs1` directly, the legacy behavior).
    /// `Some(idx)` = assumes `ir.morphs[idx]` is `IrMorphKind::Uv` and edits its
    /// per-vertex offsets. Reads / writes go through `read_displayed_uv` /
    /// `write_displayed_uv`, and the canvas shows "base + current morph offset (weight = 1)".
    pub active_morph: Option<usize>,
    /// `app.morph_weights[active_morph]` saved when entering morph edit mode (v0.5.6).
    /// While in edit mode the target morph weight is forced to 1.0; on exit we
    /// restore from this saved value. Always restore / reseed via the
    /// `switch_active_morph` helper (overwriting `active_morph` directly would
    /// drop the saved value and break restore).
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
    /// Clear on a new model load (history restoration entries are seeded separately by the caller).
    /// `detached` is a user preference and survives reload, so it is not touched here.
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
        // On reload, `morph_weights` itself is rebuilt, so the saved value's
        // restore target (the old IR's index) no longer exists. Discard it for sure.
        self.morph_weight_saved = None;
    }

    /// Switch `active_morph` and save / restore the weight (v0.5.6).
    ///
    /// - When the previous `active_morph` was Some, restore `weights` from the saved value.
    /// - When the new `active_morph` is Some, save the current weight and set it to 1.0.
    /// - If `new_morph == self.active_morph`, do nothing and return false.
    ///
    /// Return value: whether the mode actually switched (the caller uses this to
    /// decide side effects like `morph_dirty = true` or clearing the selection set).
    /// Out-of-range indices in `weights` are silently ignored (the IR and weights
    /// can be temporarily out of sync during history restoration etc.).
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

    /// Record the pristine UV for a vertex (called on the first drag start).
    /// If already recorded, do nothing (`or_insert` semantics).
    pub fn record_pristine(&mut self, key: VertexKey, uv: [f32; 2]) {
        self.pristine_uvs.entry(key).or_insert(uv);
    }

    /// Whether `uv` matches the pristine value (review_result_05 [P2]).
    /// If pristine is not recorded, returns false (errs on the safe side: do not treat as never-edited).
    fn matches_pristine(&self, key: VertexKey, uv: [f32; 2]) -> bool {
        self.pristine_uvs.get(&key).is_some_and(|p| *p == uv)
    }

    /// Push an undo entry. If `before == after` (no-op), do nothing.
    /// A new push clears the redo stack (standard undo / redo semantics).
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

    /// Apply undo. Write `before` back to the IR UV and reflect into `overrides`.
    /// Vertices that match pristine are removed from `overrides` as
    /// "returned to the unedited state" (review_result_05 [P2]).
    /// Returns true on success, false if the stack is empty.
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

    /// Apply redo. Write `after` back to the IR UV and reflect into `overrides`.
    /// When matching pristine (= redo back to the initial value) the entry is removed from overrides.
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

    /// Reset only the view (the "reset view" button). Edit deltas are preserved.
    pub fn reset_view(&mut self) {
        self.view_offset = [0.0, 0.0];
        self.view_zoom = 1.0;
    }

    /// Record one vertex's UV (writes to the IR happen on a separate path through `apply_to_ir`).
    pub fn set_uv(&mut self, key: VertexKey, uv: [f32; 2]) {
        self.overrides.insert(key, uv);
    }

    /// Build persistence entries (for JSON output). Sorted by mesh / vertex index / UV set.
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

    /// Stage saved entries for restoration. The actual IR push happens on the next `apply_pending_restore`.
    pub fn stage_restore(&mut self, entries: Vec<VertexUvOverrideEntry>) {
        self.pending_restore = Some(entries);
    }

    /// Push the entries received via `stage_restore` into the IR and into `overrides`.
    /// If the IR's mesh / vertex count has changed (reload failure etc.), the entry is silently skipped.
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

    /// Write all current `overrides` back into the IR (used to re-apply after reload).
    /// Independent of `apply_pending_restore`; reflects whatever is already in `overrides`.
    pub fn apply_to_ir(&self, ir: &mut IrModel) {
        for (&(mi, vi, chan), &uv) in &self.overrides {
            write_uv_to_ir(ir, mi, vi, uv, chan);
        }
    }
}

/// Return the global vertex offset of each mesh (Phase 3 A-2 helper).
/// UV morphs index vertices as `(global_vertex_index, [f32; 4])`, so this is
/// used for `(mi, vi) <-> global_vi` conversion. Computed in O(meshes.len()),
/// cheap enough to call every frame.
pub fn mesh_global_offsets_of(ir: &IrModel) -> Vec<usize> {
    let mut offs = Vec::with_capacity(ir.meshes.len());
    let mut cum = 0usize;
    for m in &ir.meshes {
        offs.push(cum);
        cum += m.vertices.len();
    }
    offs
}

/// Return the UV to display on the UV edit canvas (Phase 3 A-2 helper).
/// - `active_morph = None`: returns the base UV (`IrVertex.uv` / `IrMesh.uvs1`) as-is.
/// - `active_morph = Some(idx)`: returns base + the morph's offset (weight = 1).
///   - If the morph's `channel` does not match `chan`, the morph is treated as out-of-scope and base alone is returned.
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

/// Update the displayed UV to a new value (Phase 3 A-2 helper).
/// - `active_morph = None`: falls back to `write_vertex_uv` (writes the base UV directly).
/// - `active_morph = Some(idx)`: keeps the base UV unchanged and updates the morph offset to `new_uv - base`.
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
    // Split borrow: read the base UV with an immutable borrow first, then advance to the mutable morph borrow.
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
    // PMX UV morphs have 4 components. For UV0 / UV1 editing only xy is meaningful; zw stays 0.
    let entry = [du, dv, 0.0, 0.0];
    if let Some(e) = offsets.iter_mut().find(|(v, _)| *v == global_vi) {
        e.1 = entry;
    } else {
        offsets.push((global_vi, entry));
    }
    true
}

/// Number of `offsets` entries on a UV morph (Phase 3 A-2 helper, for the UI status display).
/// Returns 0 if the morph does not exist or is not `IrMorphKind::Uv`.
pub fn morph_uv_entry_count(ir: &IrModel, morph_idx: usize) -> usize {
    ir.morphs
        .get(morph_idx)
        .and_then(|m| match &m.kind {
            IrMorphKind::Uv { offsets, .. } => Some(offsets.len()),
            _ => None,
        })
        .unwrap_or(0)
}

/// Read the UV from the given mesh / vertex / UV set in the IR (Phase 3 A-1 helper).
/// Returns None if missing (out of range, mesh has no UV1, unknown chan).
pub fn read_mesh_vertex_uv(
    mesh: &crate::intermediate::types::IrMesh,
    vi: usize,
    chan: u8,
) -> Option<[f32; 2]> {
    match chan {
        0 => mesh.vertices.get(vi).map(|v| v.uv.to_array()),
        1 => {
            // A mesh has UV1 only when `uvs1.len() == vertices.len()`.
            // Empty (no UV1) and length mismatch are both treated as "no UV1".
            if mesh.uvs1.len() == mesh.vertices.len() {
                mesh.uvs1.get(vi).copied()
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Whether at least one of the meshes belonging to the material has UV1
/// (Phase 3 A-1 helper). Used by the ComboBox to enable / disable the UV1 option.
pub fn material_has_uv1(ir: &IrModel, material_index: usize) -> bool {
    ir.meshes.iter().any(|m| {
        m.material_index == material_index && !m.uvs1.is_empty() && m.uvs1.len() == m.vertices.len()
    })
}

/// Public UV write function for the UI (a thin wrapper over `write_uv_to_ir`, Phase 3 A-1).
pub fn write_vertex_uv(
    ir: &mut IrModel,
    mesh_idx: u32,
    vert_idx: u32,
    uv: [f32; 2],
    chan: u8,
) -> bool {
    write_uv_to_ir(ir, mesh_idx, vert_idx, uv, chan)
}

/// Write a UV into the IR at the given mesh / vertex. Returns false if out of range or the UV set is missing.
/// `chan = 0` -> `IrVertex.uv` (UV0). `chan = 1` -> `IrMesh.uvs1[vi]` (UV1). UV1 is
/// writable only when the mesh has `uvs1.len() == vertices.len()` (writes are
/// rejected when uvs1 is empty).
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
            // For meshes with no UV1 (empty or length mismatch), give up on the write.
            // Auto-extending here would silently turn meshes that "did not have UV1"
            // into UV1-bearing ones in the next VRM export, which is a side effect we
            // do not want — writes are restricted to existing UV1 meshes.
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
