<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [Changelog](#changelog)
  - [v0.5.5 (2026-04-13)](#v055-2026-04-13)
    - [New Features (Phase 1)](#new-features-phase-1)
    - [New Features (Phase 2)](#new-features-phase-2)
    - [New Features (Phase 3)](#new-features-phase-3)
    - [Internals](#internals)
    - [Scope Notes](#scope-notes)
    - [Bug Fixes (Pre-Release Review)](#bug-fixes-pre-release-review)
    - [Tests](#tests)
  - [v0.5.4 (2026-04-13)](#v054-2026-04-13)
    - [New Features](#new-features)
    - [Internals](#internals-1)
    - [Bug Fixes (Pre-Release Review)](#bug-fixes-pre-release-review-1)
    - [Tests](#tests-1)
  - [v0.5.3 (2026-04-13)](#v053-2026-04-13)
    - [New Features](#new-features-1)
    - [Internals](#internals-2)
  - [v0.5.2 (2026-04-13)](#v052-2026-04-13)
    - [New Features](#new-features-2)
    - [Internals](#internals-3)
    - [Bug Fixes (Pre-Release Review)](#bug-fixes-pre-release-review-2)
  - [v0.5.1 (2026-04-13)](#v051-2026-04-13)
    - [New Features](#new-features-3)
    - [Performance](#performance)
    - [Internals](#internals-4)
    - [Bug Fixes (Pre-Release Review)](#bug-fixes-pre-release-review-3)
    - [Tests](#tests-2)
    - [Deferred → v0.6.0](#deferred-%E2%86%92-v060)
  - [v0.5.0 (2026-04-13)](#v050-2026-04-13)
    - [New Features](#new-features-4)
    - [Behavior Changes](#behavior-changes)
    - [Tests](#tests-3)
  - [v0.4.0 (2026-04-11)](#v040-2026-04-11)
    - [New Features](#new-features-5)
    - [Behavior Changes](#behavior-changes-1)
    - [Internals](#internals-5)
  - [v0.3.0 (2026-04-11)](#v030-2026-04-11)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# Changelog

[日本語](CHANGELOG.jp.md)

## v0.5.5 (2026-04-13)

Introduces a **per-vertex UV editing window** invoked from the material editor panel. v0.5.4 delivered material-level UV transform (offset / scale / rotation). v0.5.5 goes one layer deeper with **Phase 1** (single-vertex editor + persistence + reload-safe state), **Phase 2** (texture-background preview, rectangle selection, zoom/pan, rotate/scale, undo/redo, Ctrl+A), and **Phase 3** follow-ups (additive/subtractive rect selection, detachable independent OS window, UV1 editing, 2D gizmo handles).

### New Features (Phase 1)

- **UV Edit Window** — A new "UV 編集" button appears in the material editor panel header. Clicking it opens a dedicated floating `egui::Window` (`Id::new("uv_edit_window")`, 1-instance) whose title reflects the active material name. The window renders a square UV canvas (up to 260×260 px) of the active material's triangle wireframe in UV space with **v=0 at the top** / **v=1 at the bottom**, matching the `convert/uvmap.rs` PSD export so both views are directly comparable.
- **Vertex Pick & Drag** — Clicking within 12 px of a UV vertex selects it (yellow). Dragging translates the selected vertex in UV space. Edits are written directly to `IrMesh.vertices_mut()[*].uv`, so re-exports (PMX writer) immediately reflect the change.
- **Material Filter** — The window's ComboBox chooses which material's UVs are visible on the canvas. The button in the material editor panel header auto-sets the active material to whichever material is currently being edited.
- **Vertex UV Persistence** — `TextureHistoryFile` gains `vertex_uv_overrides: HashMap<path, Vec<VertexUvOverrideEntry>>` (JSON version bumped to v3). "履歴を保存" now writes per-vertex UV deltas alongside texture and parameter deltas; "履歴呼出" restores them and syncs the GPU vertex buffer.
- **GPU Sync on Mouse-Up** — `GpuModel::sync_uvs_from_ir` re-uploads the full vertex buffer only on drag-stop. Per-frame edits during the drag touch only CPU-side `IrVertex.uv` to keep the frame rate unaffected.

### New Features (Phase 2)

- **Texture Background (2-1)** — The active material's BaseColor texture is rendered as a canvas background via `register_native_texture` + `painter.image`, scoped to the UV [0,1] region. A 1-entry cache (`ViewerApp.uv_edit_bg_tex`) re-registers only when the active material changes, and `finish_load_with_gpu` frees the `egui::TextureId` on model switch to avoid GPU leaks. PMX/PMD materials that lack `base_color_tex_info` fall back to `mat.texture_index`.
- **Rectangle Selection + Bulk Translate (2-2)** — Left-drag starting far from any vertex begins rectangle selection (clears existing selection, re-fills on every frame from vertices inside the rect). Left-drag starting near a vertex enters Move mode (auto-selects the vertex under cursor if none was selected) and translates the whole selection. `UvDragMode { None, Move, Rect }` drives the branch, and `drag_start_uvs` is a `HashMap` so 1-vertex and N-vertex drags share the same code path.
- **Zoom / Pan / Snap (2-3)** — Wheel zooms around cursor (0.1×〜32×, log-scale factor `* 0.002.exp()`). Middle-drag pans in UV space (scaled by zoom). Shift + drag snaps translation to 1/16 (= 0.0625) grid. `uv_to_canvas` / `canvas_to_uv` were extended with `view_offset` / `view_zoom`, so pick/draw/drag/rect all follow the view transform. "表示リセット" button returns to zoom=1.0 / offset=[0,0].
- **Rotate / Scale (2-4)** — Alt + drag rotates around the selection-bbox center (angle diff via `atan2` → `sin_cos`). Ctrl + drag scales around the same pivot (distance ratio). A cross-hair marker is drawn at the pivot during Move drag for visual feedback. Ctrl takes precedence over Alt if both are held.
- **Undo / Redo (2-5)** — `UvUndoEntry { before, after }` records one entry per drag (on `drag_stopped` Move). `Ctrl+Z` undoes, `Ctrl+Y` / `Ctrl+Shift+Z` redoes. GUI buttons "⟲ 元に戻す" / "⟳ やり直す" mirror the shortcuts. Undo stack is capped at 50 entries (FIFO). New edits clear the redo stack (standard semantics). `wants_keyboard_input()` guard prevents collision with TextEdit widgets elsewhere in the app.
- **Select All (Ctrl+A)** — Adds all vertices of the active material to `selected` (existing selection is preserved, not replaced). A "全選択" button provides the same action from the GUI.

### New Features (Phase 3)

- **Additive / Subtractive Rectangle Selection (A-4)** — Previously rectangle selection always replaced the existing selection. Now Shift+drag adds the rect-inside vertices to the current selection, Ctrl+drag removes them. `UvRectBehavior { Replace, Add, Subtract }` is decided at `drag_started()` time from the modifier keys; `rect_initial_selected` snapshots the pre-drag selection so the per-frame rebuild `initial ± inside` stays consistent when the rect shrinks or expands. Ctrl is reused instead of Alt to avoid collision with Move-mode's "rotate" assignment (mode is already fixed by drag-start position, so Ctrl's meaning is unambiguous per mode).
- **Detachable Independent OS Window (A-3)** — A new "⬈ 分離" button in the UV edit toolbar promotes the floating `egui::Window` to a real OS-level window via `ctx.show_viewport_immediate` (eframe 0.31 viewport API). Once detached, the UV editor lives in its own desktop window with its own title bar, resize handles, and minimize/close buttons; the main viewer keeps running the 3D scene behind. "⬓ 結合" snaps it back into the main window. `UvEditState.detached: bool` carries the preference across the session (but not across reloads, since `reset` preserves it). `ViewportId::from_hash_of("uv_edit_viewport")` keeps OS-level window position and size stable between toggles. The × button on the native window frame closes the editor by flipping `uv_edit_window_open = false`; the detached preference is preserved so reopening goes straight back to a separate window.
- **UV1 Editing (A-1)** — `VertexKey` expands from `(mesh_idx, vertex_idx)` to `(mesh_idx, vertex_idx, uv_set)` where `uv_set = 0` maps to `IrVertex.uv` (UV0) and `uv_set = 1` maps to `IrMesh.uvs1[vi]` (UV1, `TEXCOORD_1`). A new "UV セット" ComboBox lets the user switch between UV0 and UV1; the UV1 option is auto-disabled when no mesh in the active material carries UV1. Switching UV sets cancels any in-progress drag but keeps selection across sets — `selected` / `overrides` / undo history live in separate per-channel subspaces so UV0 and UV1 edits never cross-contaminate. All pick / draw / drag / rect-select / Ctrl+A paths filter by `active_uv_set`, skip meshes without UV1 when UV1 is chosen, and route writes via a new `write_vertex_uv(ir, mi, vi, uv, chan)` helper. `sync_uvs_from_ir` now uploads both UV0 and UV1 to the GPU vertex buffer (and to `animated_vertices` when present) so UV1 edits show up in lit shaders (MToon UV1 lookups, Matcap, etc.) on drag-stop. `VertexUvOverrideEntry` gains `uv_set: u8` with `#[serde(default)]`, so v0.5.5 Phase-1 JSON files (UV0-only entries) load unchanged.
- **Visual 2D Gizmo Handles (A-5)** — A selection-bbox gizmo is drawn whenever 2 or more vertices are selected (with non-degenerate bbox). Four orange square handles appear at the bbox corners for scale, and a blue circle above the top edge (24 px outside) is the rotate handle. Dragging a corner scales around the opposite corner (Photoshop/Blender convention); dragging the rotate handle rotates around the bbox center. Gizmo-initiated drags do not require any modifier key — `UvGizmoAction { ScaleCorner { sign_u, sign_v }, Rotate }` is decided at `drag_started()` time from the hit test and takes priority over the modifier-key interpretation in the Move branch. Modifier-based scale/rotate (Ctrl/Alt) is still available for users who prefer keyboard-driven transforms. Handle pick radius is 10 px; rotate handle is placed first in the hit test so it wins when it overlaps a corner's pick region.

### Internals

- New `src/viewer/app/uv_edit.rs` introduces `UvEditState` and grows across phases:
  - Phase 1: `overrides`, `selected`, `active_material`, `dragging`, `pending_restore`
  - Phase 2-2: `UvDragMode` enum, `drag_mode`, `drag_start_uvs`, `drag_press_uv`
  - Phase 2-3: `view_offset`, `view_zoom` (manual `Default` impl for `view_zoom = 1.0`), `reset_view()`
  - Phase 2-4: `drag_pivot`
  - Phase 2-5: `UvUndoEntry`, `undo_stack`, `redo_stack`, `push_undo`, `apply_undo`, `apply_redo`, const `UV_UNDO_MAX = 50`
  - Review 05 fix: `pristine_uvs`, `record_pristine`, `matches_pristine` (lazy per-vertex pristine; keeps overrides clean after undo restores to initial value)
- `ViewerApp` gains `uv_edit_window_open: bool` and `uv_edit_bg_tex: Option<(usize, egui::TextureId)>`. `show_uv_edit_window` is invoked from `update()` right after `show_material_editor_window`. Both are cleared / freed on `finish_load_with_gpu`.
- New `VertexUvOverrideEntry { mesh_index, vertex_index, uv: [f32; 2] }` in `persistence.rs` — flat-array JSON layout (~30 byte per vertex). `TextureHistoryFile` preserves backward compatibility: v0.5.4 and earlier `popone_history.json` files load unchanged via `#[serde(default)]`.
- `GpuModel::sync_uvs_from_ir` walks IR meshes, uses the existing `global_to_gpu` map to find each GPU vertex index, updates `base_vertices[*].uv`, re-uploads via `queue.write_buffer`, and invalidates the morph cache so the next animation frame re-composites from fresh base data.
- `uv_to_canvas` / `canvas_to_uv` now take `view_offset: [f32; 2]` and `view_zoom: f32` parameters. Since all rendering/picking/drag/rect math flows through these two functions, Phase 2-3's pan/zoom automatically applies to every interaction with a single centralized change.
- Drag handling uses a `start_uv + (cursor_uv - press_uv)` formulation (and rotation/scale analogues) to stay robust regardless of the `drag_delta()` frame/cumulative semantics across egui versions.

### Scope Notes

- **UV0 only.** UV1 (`IrMesh.uvs1`), morph UV, and multi UV-set switching are planned for Phase 3 (v0.5.7+).
- **Single window instance.** Multi-material side-by-side editing is Phase 3.
- **Clear-all is destructive.** "編集をすべてクリア" drops overrides / selected / undo stack / redo stack / pristine_uvs. Reload is required to restore the original (pre-edit) UVs since no global pristine snapshot is kept.

### Bug Fixes (Pre-Release Review)

- **review_result_01 [P1] Reload dropped per-vertex UV edits** — `finish_load_with_gpu` unconditionally called `self.uv_edit.reset()`, so any A-stance / T-stance conversion or `reload_current()` discarded unsaved UV edits. `ReloadSnapshot` now carries `uv_edit_overrides` / `uv_edit_active_material` / `uv_edit_window_open`; `restore_snapshot_on_success` reapplies overrides via `apply_to_ir` and re-uploads the GPU vertex buffer via `sync_uvs_from_ir`. `restore_snapshot_on_failure` also restores the override map so the in-memory state stays consistent after a failed reload.
- **review_result_02 [P1] Drag-move over-accumulated per frame** — The original implementation added `response.drag_delta()` to the current UV every `dragged()` frame. Because the delta behavior depends on the egui version and can be cumulative from drag-press, long drags caused the vertex UV to fly far beyond the cursor. The implementation now captures each selected vertex's UV *and* the cursor UV position on `drag_started()`, then on every `dragged()` frame recomputes `new_uv = start_uv + (cursor_uv - press_uv)`. Added `canvas_to_uv` helper and `UvEditState::{drag_start_uvs, drag_press_uv}` fields (cleared on drag-stop and `reset()`).
- **Hand-test feedback [P1] Canvas Y-axis disagreed with PSD export** — The initial Phase 1 canvas used the Blender/Maya convention (v=0 at the bottom), but `convert/uvmap.rs` rasterizes UV v directly to image Y (v=0 at the top). `uv_to_canvas` / `canvas_to_uv` are now both non-flipping on Y, so the editor visually matches the PSD output and side-by-side workflows work as expected.
- **review_result_03 [P2] Clear-all left undo/redo stacks intact** — "編集をすべてクリア" only cleared `overrides` / `selected`, so pressing `Ctrl+Z` right after resurrected the discarded edits, violating the button's tooltip contract. The button now also clears `undo_stack` / `redo_stack`, and the tooltip explicitly mentions "undo/redo 履歴を破棄".
- **review_result_05 [P2] Undo left stale entries in overrides** — `apply_undo` / `apply_redo` unconditionally did `overrides.insert(k, v)`, so a vertex returned to its initial UV by undo still appeared as "edited" in the UI count and in `to_entries()`-based history save. A lazy-recorded `pristine_uvs` (populated by `record_pristine` on the first drag of each vertex, `or_insert` semantics) now lets undo/redo call `overrides.remove` whenever the resulting UV equals the pristine value. Memory cost scales with the number of *ever-edited* vertices only, avoiding the all-vertex snapshot that was explicitly rejected in Phase 1.
- **review_result_06 [P2] Clear-all ignored pristine_uvs** — Following review 05, the clear-all button also needs to drop `pristine_uvs`, otherwise a later edit session reuses a stale pristine baseline and undo cannot return to an "unedited" state.

### Tests

- Full test suite (179 tests) continues to pass. UV-edit logic is UI-driven; downstream data flow (IR → GpuModel → PMX writer) is covered by existing round-trip tests.

## v0.5.4 (2026-04-13)

Adds per-slot UV transform (offset / scale / rotation) editing to the material editor panel. Nine slots that carry `KHR_texture_transform` data are supported: BaseColor / Emissive / Normal / Shade / ShadingShift / RimMultiply / OutlineWidth / Matcap / UvAnimMask.

### New Features

- **Per-slot UV Transform Editing** — A new compact widget (offset X/Y, scale X/Y, rotation°, and a ⟲ reset button) appears directly beneath each texture slot thumbnail. It is displayed only when a texture is actually assigned, writes into `IrTextureInfo.offset / scale / rotation`, and flows through the existing 9 uniform pairs (`base_uv / shade_uv / ...`) to the shader. Rotation is entered in degrees and stored as radians.
- **UV Transform Persistence** — `MaterialParamOverride` gains nine `TextureUvOverride { offset, scale, rotation }` fields. Edits are persisted per normalized model path in `popone_history.json` and restored across reload / A-stance conversion / viewer restart. Pre-v0.5.4 JSON files remain fully readable via `#[serde(default)]`.
- **Coexistence with Expression-driven UV Animation** — The v0.5.1 `IrTextureTransformBind` pipeline remains independent; static UV overrides apply first and Expression animation accumulates on top without interference.

### Internals

- Introduces `TextureUvOverride { offset: Option<[f32;2]>, scale: Option<[f32;2]>, rotation: Option<f32> }`. All fields are optional so partial saves are possible and serialized size stays minimal.
- `apply_to` only writes UV parameters when the corresponding `IrTextureInfo` is already `Some`. Unassigned slots are a no-op (the override never synthesizes a default `IrTextureInfo::from_index(0)` and cannot bind the wrong texture).
- MToon-slot UV writes go through `mat.mtoon.as_mut()` rather than `mtoon_mut()`, so non-MToon materials never get an unintended default `MtoonParams` block.
- `diff_from` skips all six MToon-slot UV overrides when `enable_mtoon == Some(false)`, preserving round-trip consistency.
- Adds `uv_transform_widget` / `record_uv_override` helpers in `ui.rs`, invoked from all nine slots.

### Bug Fixes (Pre-Release Review)

- **review_result_01 [P1] Newly-assigned slot UV edits were not persisted** — `TextureUvOverride::diff()` only produced a delta for `(Some, Some)` cases, so when a user assigned a texture to a previously-empty slot and then tweaked its UV transform, the UV delta was silently dropped at save time. The diff now falls back to comparing against the identity transform (offset=0 / scale=1 / rotation=0) when `pristine` is `None`, so newly-assigned slot UV edits round-trip through `popone_history.json`.

### Tests

- UV round-trip coverage for BaseColor and MToon-slot (shade) diff → apply.
- Verification that unassigned slots remain unassigned after `apply_to`, and that MToon is never auto-initialized by UV overrides alone.
- Verification that MToon OFF excludes MToon-slot UV from `diff_from`.
- `TextureUvOverride::default().is_empty()` and `merge_from` UV merge behavior.
- Newly-assigned slot UV persistence and slot-removal UV behavior (2 tests for review_result_01 [P1]).
- Nine new tests (material_edit module now has 19 tests, 244 total passing).

## v0.5.3 (2026-04-13)

Material editor UI refresh: the floating window has been replaced with a fixed bottom-dock panel above the shortcut hint bar. Material name editing, row thumbnail buttons, emoji icons, and on/off bulk normal toggles are introduced in a single pass.

### New Features

- **Material Name Editing** — A `TextEdit` field is added at the top of the material editor panel, allowing in-place name changes. Edits are recorded via `MaterialParamOverride.name: Option<String>` into `material_overrides`, and are restored across reload / A-stance conversion / `popone_history.json` save. After editing, `update_mat_cache()` refreshes the side panel material list immediately.
- **Dockable Material Editor Panel** — The old floating `egui::Window` has been converted to `egui::TopBottomPanel::bottom("material_editor_panel")`. It is pinned directly above the shortcut hint bar, resizable via its top edge, and its content is wrapped in `ScrollArea::vertical`. A `[×]` button in the header closes the panel. When the edit icon is off, the panel is not declared at all, so the central viewport automatically expands.
- **Thumbnail Leading Button** — The old □/■ character indicator in each material row is replaced by a 14×14 px texture thumbnail `ImageButton` backed by `ir_thumb_cache`. A compact frame preset (`spacing.button_padding = (1,1)` + `stroke = 0.5`) is applied locally via `ui.scope`, leaving other buttons untouched. When no thumbnail is available it falls back to the old □/■ (the filled ■ still signals "assigned but thumbnail not yet built").
- **Emoji Icon Set** — The `[S][C][N][B][編]` labels on material rows and group headers are replaced with `✨🗑🗺💡✏`. The constants `ICON_SMOOTH / ICON_CLEAR_NORMAL / ICON_NORMAL_MAP / ICON_EMISSIVE / ICON_EDIT` are consolidated at the top of `ui.rs`.
- **On/Off Bulk Normal Controls** — The "法線平滑化（一括）" / "カスタム法線クリア（一括）" checkboxes are replaced with `label + [on] + [off]` compact button rows. The existing rule of skipping normal-textured materials on the "on" path is preserved.

### Internals

- Added `name: Option<String>` to `MaterialParamOverride`. Because `String` is not `Copy`, it is handled outside the existing `merge!` / `diff_field!` macros with explicit `clone()` in `merge_from` / `diff_from` / `apply_to`.
- Expanded the visibility of `update_mat_cache` from `pub(super)` to `pub(in crate::viewer)` so that `ui.rs` can rebuild the name cache after an edit.
- Added a call to `app.sync_ir_thumb_cache()` at the top of `show_side_panel` (length-compare early-return keeps it zero-cost when already synced).
- Moved the material editor panel call site from before `apply_pending_material_rebuilds()` to after the `shortcut_hints` panel declaration to enforce the bottom-panel stacking order "bottom = status_bar / middle = shortcut_hints / top = editor panel".

## v0.5.2 (2026-04-13)

Texture thumbnails are now integrated into each material editor parameter section. Texture assignments and their related parameters can be seen in one place.

### New Features

- **Texture Thumbnails Integrated into Sections** — The old consolidated "テクスチャスロット" section has been dissolved, and thumbnail + assignment UI is now placed at the top of each material parameter section:
  - **Basic**: BaseColor
  - **Shade (影)**: Shade / ShadingShift
  - **Outline (アウトライン)**: OutlineWidth
  - **Rim (リム)**: RimMultiply
  - **MatCap**: Matcap
  - **UV Animation (UV アニメ)**: UvAnimMask
  - **Emissive / Normal (エミッシブ / 法線)**: Emissive / Normal
  - **MMD Textures (Sphere / Toon)**: Sphere / Toon — kept as a separate section since they are MMD/PMX-specific.
- **Thumbnail as Button** — The 32px thumbnail image itself is now an `ImageButton`; clicking it opens the file dialog (the previous separate text button has been removed). Hovering shows the filename as a tooltip.
- **X Icon for Unassigned Slots** — Unassigned slots render a placeholder button with an `×` symbol, using theme-consistent colors from `widgets.inactive`. Clicking opens the file dialog for new assignment.
- **Per-Slot Reset** — The existing small `×` reset button at the end of each row is preserved, visible only when a texture is assigned.

### Internals

- Added `TextureState::ir_thumb_cache: Vec<Option<egui::TextureId>>`, a cache of 64px thumbnail TextureIds parallel to `loaded.ir.textures`. Reuses the same thumbnail pipeline as the existing `pkg_thumb_cache` (UnityPackage textures).
- Added four methods: `rebuild_ir_thumb_cache` / `append_ir_thumb_cache` / `clear_ir_thumb_cache` / `sync_ir_thumb_cache`. `sync` compares `loaded.ir.textures.len()` with cache length and performs differential updates: append for growth, rebuild for shrinkage, clear when no model is loaded.
- In `assign_texture_core`, `build_ir_thumb_entry` is called inline on the new-texture push path to avoid `&mut self` re-borrow conflicts with existing `device`/`queue` borrows.
- `show_material_editor_window` calls `sync_ir_thumb_cache` at entry so UI thumbnails track external changes (model swap, BG load completion) to `ir.textures` length.
- Added shared `texture_slot_widget()` helper function. Each section calls it and it returns `(assign_clicked, reset_clicked)` bool pair — the caller then sets `pending_tex_request` / `pending_tex_clear`, keeping borrow boundaries clean.

### Bug Fixes (Pre-Release Review)

- **[review_01 P1] Stale thumbnails after model swap** — `finish_load_with_gpu` / `cancel_gpu_build` / `cancel_bg_index_load` now call `clear_ir_thumb_cache()`. Without this, when the previous and next models happened to have the same texture count, `sync_ir_thumb_cache()` would early-return on length comparison and reuse the previous model's `TextureId`s, showing incorrect thumbnails.
- **[review_01 P2] Thumbnail not updated after PSD→PNG conversion** — `poll_pending_psd_conversions()` now regenerates the `TextureId` for the converted index on completion. Since length is unchanged, `sync_ir_thumb_cache()` would not rebuild, and in cases where the initial PSD decode failed (`None` thumbnail), the slot would remain permanently blank even after successful PNG conversion.
- **[review_02 P1] Index misalignment when adding texture before material editor opens** — `assign_texture_core()` and `apply_tex_preview()` new-texture push paths were changed from a simple `cache.push()` to a "append missing prefix" logic. `ir_thumb_cache` stays at length 0 until the material editor is opened, so the previous push would place the new thumbnail at `cache[0]`, misaligning all subsequent slot displays. A single BaseColor reassignment right after model load was enough to corrupt every slot thumbnail.

## v0.5.1 (2026-04-13)

VRM 1.0 Expression material binds playback, auxiliary texture slot persistence, and material editor drawer UX improvements.

### New Features

- **Expression Material Binds (VRM 1.0)** — The viewer now plays back `materialColorBinds` and `textureTransformBinds` in VRM 1.0 Expressions. Six color targets (`color` / `emissionColor` / `shadeColor` / `matcapColor` / `rimColor` / `outlineColor`) and UV scale/offset blend additively across multiple simultaneously-active expressions following the VRM 1.0 spec algorithm: `finalValue = baseValue + Σ((targetValue − baseValue) × weight)`. Base values are captured at load time and refreshed when the material editor modifies a material, so editor-adjusted values become the new base that expressions blend against.
- **Sphere / Toon Texture Slot Editing** — The material editor drawer "テクスチャスロット" section now exposes the MMD-specific `Sphere` and `Toon` slots. Each slot has a file dialog button and `×` reset button matching the existing 8 auxiliary slots.
- **Auxiliary Texture Slot Persistence** — All 10 non-BaseColor texture slot assignments (Emissive / Normal / Shade / ShadingShift / RimMultiply / OutlineWidth / Matcap / UvAnimMask / Sphere / Toon) are now saved to `popone_history.json` with a `slot` field on each `TextureHistoryEntry`. Previously only BaseColor assignments were persisted and auxiliary slots were lost on restart. The new `slot` field uses `#[serde(default)]` so v0.5.0 history files load as `BaseColor` (backward compatible) and v0.5.1 files are silently accepted by v0.5.0 (forward compatible via unknown-field ignoring).
- **Dirty Indicator in Material Editor Title** — The window title now shows a trailing `*` when the currently-edited material has any edit difference: parameter overrides, BaseColor texture assignment, or auxiliary slot texture assignment. This lets users identify "touched" materials at a glance.
- **Material Parameter Copy / Paste** — The material editor drawer toolbar row gains "コピー" and "ペースト" buttons. Copy captures the `diff_from(pristine, current)` result into a session-local clipboard; paste applies it to the currently-edited material with the standard dirty-tracking flow. Texture assignments are intentionally excluded (path-dependent) so only color/scalar values transfer between materials.
- **PMX-Unsupported Badge Visual Enhancement** — The Rim / MatCap / UV Animation collapsing sections previously embedded `(PMX非対応)` as plain text in the section title. The title is now clean and each section body opens with a color-coded `⚠ PMX 非対応` badge with a hover tooltip: "この項目は PMX 出力では反映されません。MME (.fx) 出力やビューアプレビューでは反映されます。"

### Performance

- **DrawCall Uniform Buffer Optimization** — `DrawCall` now holds a persistent `wgpu::Buffer` (`material_buf`) with `UNIFORM | COPY_DST` usage. `create_material_bind_group` was split into `serialize_material_uniform` / `create_material_buffer_and_bind_group` / `write_material_buffer`. The existing bind group rebuild path now uses `queue.write_buffer` for uniform-only updates, avoiding full bind group recreation on every material edit or expression frame. This is the enabler for per-frame Expression material bind updates without GPU resource churn.

### Internals

- New enum variant `IrMorphKind::Material { color_binds, uv_binds }` alongside existing `Vertex` and `Group`. VRM Expressions with both vertex morphs and material binds are emitted as two separate IrMorphs with the same name, so the existing name-based `morph_weights` mapping drives both simultaneously without requiring a compound variant.
- New types `MaterialColorBindType`, `IrMaterialColorBind`, `IrTextureTransformBind` in `intermediate::types`. `MaterialColorBindType::from_vrm_str` parses the VRM 1.0 `type` string to the enum.
- New `GpuMorphEntry::Material` variant and `MaterialBaseValues` struct on `GpuModel`, plus a pure function `accumulate_expression_materials()` that iterates active material morphs, accumulates color and UV deltas per material, and returns `Vec<Option<MaterialParams>>` for dirty materials.
- `IrModel::merge()` now offsets `material_index` in `IrMorphKind::Material` variants (color_binds and uv_binds) when merging models.

### Bug Fixes (Pre-Release Review)

A series of five Codex review rounds surfaced integration issues between the new Expression material-bind path, the history-recall flow, and the existing material editor. All were resolved before shipping:

- **Texture history recall order** — Previously the recall flow restored textures first and then reset all materials to `pristine`, which destroyed auxiliary-slot texture references (`emissive_texture`, `normal_texture`, etc.). Fixed by restoring pristine first, then applying textures and parameter overrides. Also clear `tex.assignments`, `tex.pkg_assignments`, and `slot_texture_paths` as part of the pristine reset so the recalled state is a complete reproduction of the saved point.
- **Expression weight-zero revert** — `accumulate_expression_materials` previously skipped morphs with `weight.abs() < 1e-6`, which meant a `1.0 → 0.0` weight transition never wrote the base value back and the last-applied color / UV remained stuck on the GPU. Fixed by pre-marking every material referenced by any `GpuMorphEntry::Material` as dirty so the base value is written back when deltas collapse to zero.
- **Material-editor edits not seen as new base** — `material_base_values` was captured once at load time, so editor-adjusted values were ignored by the Expression blend basis. Fixed by re-capturing `MaterialBaseValues::from_ir(mat)` inside `apply_pending_material_rebuilds` whenever a material is marked dirty.
- **Expression reflection lost when editing during manual morph** — Editing a material while a manual morph slider holds a non-zero weight previously dropped the Expression material reflection until the user moved the slider again. Fixed by running the Expression material accumulation pass at the end of `apply_pending_material_rebuilds` whenever any morph weight is non-zero.
- **BaseColor texture-bind not regenerated on full rebuild** — `rebuild_material_bind_groups` only updated `material_buf` / aux / MMD bind groups but not the standard-path `texture_bind_group`. This left stale BaseColor textures on screen after pristine restoration. Fixed by regenerating the standard-path texture bind group in the full-rebuild path as well.
- **BaseColor bind regression for PMX / PMD materials** — The initial fix above only looked at `mat.base_color_tex_info`, which is `None` for PMX / PMD materials, so their BaseColor disappeared after any full rebuild. Fixed by matching the initial DrawCall construction's information source: `mat.texture_index` as the primary index, with `base_color_tex_info.sampler` as a sampler source and a default `IrSamplerInfo` fallback.
- **`material_index` not remapped in visible-only export** — `build_filtered_ir()` cloned `IrMorphKind::Material` binds verbatim, leaving stale indices into the pre-filter material array. Fixed by running `color_binds` / `uv_binds` through `mat_remap` with `filter_map`, dropping binds whose target material was excluded.
- **Empty Material morphs leaking into filtered IR** — Material morphs whose binds all pointed at excluded materials still survived as empty morphs, producing "dead expressions" in PMX output. Fixed by deriving `morph_alive[i]` for `IrMorphKind::Material` from whether any bind targets a surviving material after remap, cascading through the existing Group convergence pass.

### Tests

- 235 unit tests (up from 230 in v0.5.0). New coverage: `MaterialColorBindType::from_vrm_str` for all 6 valid strings plus unknown/empty fallback, `IrModel::merge()` material index offset for Material morphs, and `TextureHistoryEntry` slot field serde (backward-compat default, explicit slot parse, roundtrip).

### Deferred → v0.6.0

- UV transform editing UI for the remaining texture slots (RimMultiply / OutlineWidth / Matcap / UvAnimMask) — existing code has no UV transform editing UI for any slot yet, so this becomes a larger scope than originally anticipated.
- Drag-and-drop slot selection dialog when material editor is open.
- Auto-assign slot hint matching (`*_normal*` → Normal, etc.).
- Texture slot thumbnail preview in editor drawer.
- Section collapse state persistence across sessions.
- User-defined custom preset save/load.
- sdPBR `.fx` generation, MaskedMaterial support, MME `.fx` import.


## v0.5.0 (2026-04-13)

Material editor drawer with full-parameter editing for MToon + lilToon, and MME (ray-mmd) material file generator.

### New Features

- **Material Editor Drawer** — A floating `egui::Window` opens from a "編" button on each material row. Sections: Basic (diffuse / alpha mode / base color texture), Shade (shade_color / shading_toony / shading_shift + texture), Outline (edge_color / width mode / outline width texture), Rim (parametric rim / rim multiply texture), MatCap, UV animation, Emissive / Normal, Other, and an MME output preview.
- **Full MToon / lilToon parameter editing** — All 25+ colors, scalars, and aux texture slots (normal / emissive / shade / shadingShift / rim / outline / matcap / uvAnimMask) are editable. Edited values are reflected live on both the standard and MMD-compatible render paths.
- **Per-slot and per-material reset** — Each texture slot has a `×` button to clear it; each material has a "初期値に戻す" action that restores the state captured at load time.
- **Built-in material presets** — MToon 1.0 default, lilToon standard, and PMX-compat presets (3 types).
- **Edit persistence in `popone_history.json`** — Per-material edit records (color/scalar diffs + MME category override) are saved alongside the existing texture history and restored on reload.
- **MME (ray-mmd) material file generator** — The Export tab includes an "MME マテリアル (.fx) も出力" checkbox under the PMX conversion section. When checked, PMX conversion also emits `<model>_mme/material_<name>.fx` files using `CUSTOM_ENABLE`-based templates (Standard / Skin / HairAniso / Glass / Cloth / ClearCoat / Emissive). The ray-mmd root folder can be set via folder picker in the Export tab; defaults to the current directory (`.\`) when not configured. `#include` paths are resolved with `pathdiff` + `dunce` canonicalization, with a fallback when relative path computation fails. Non-PMX-capable textures (normal / emissive / matcap / rim / shading shift) are copied to `<model>_mme/textures/` with relative path references. All `.fx` and `README.txt` files are encoded in Shift-JIS with CR+LF line endings for MMD/MME compatibility. If the `#include` target (`material_common_2.0.fxsub`) does not exist at the resolved path, a warning is shown in the conversion result (files are still written).

### Behavior Changes

- **`ShaderFamily::Mtoon` is now the primary decision axis** for PMX conversion, replacing the older `is_mtoon()` (`mtoon.is_some()`) check. This lets the material editor safely surface MToon parameters on non-MToon materials without flipping the PMX export into MToon-style ambient/specular output. Users must explicitly tick an "MToon 有効化" checkbox in the drawer to promote a material.

### Tests

- 230 unit tests (up from 185 in v0.4.0). New coverage includes: `MaterialParamOverride` diff/apply round-trips, `RayMmdMaterialKind` category inference (Japanese/mixed-case/prefixed names), `generate_fx` section completeness and CR+LF encoding, `TextureSlot::is_linear` full-variant coverage.


## v0.4.0 (2026-04-11)

Added a separate-window log viewer and reworked log file persistence around the principle that no log files are written unless the user explicitly asks for them or a panic occurs.

### New Features

- **Log Viewer (Separate OS Window)** — A new top-level "ログ" toolbar button now opens an independent OS window that streams the in-memory log buffer in real time. Built on `eframe`'s `show_viewport_deferred`, the log viewer is independent of the main 3D viewport: it can be moved to a different monitor, minimized separately, and does not force the main 3D scene to re-render when new log lines arrive (~150ms polling cadence inside the deferred closure).
- **Level Filter** — Toggle Error / Warn / Info / Debug visibility independently. Lines are color-coded (Error = red, Warn = yellow, Info = white, Debug / Trace = gray, Unknown = light gray). Multi-line messages such as backtraces are kept as a single logical entry.
- **Auto Tail Following** — When enabled, the view sticks to the bottom and scrolls as new lines arrive. Manually scrolling away pauses following; scrolling back to the bottom resumes it.
- **Manual Log Export** — A "ログ保存" button writes the current in-memory log to a user-chosen path via the native file dialog. A "フォルダを開く" button opens the logs directory in the OS file explorer.
- **Persistence** — Log viewer visibility, window position/size, and level filter state are saved to `popone.toml` (`[log_viewer]` section) and restored on next launch.

### Behavior Changes

- **No automatic log file generation on normal exit.** Previously the in-memory log buffer was flushed to `popone_<ts>.log` on every clean exit. v0.4.0 removes this; the buffer stays in memory and is discarded when the process exits cleanly. Use the new "ログ保存" button if you need to keep a session's logs.
- **Panic dumps go directly to `panic_<ts>.log`.** The previous "write to `popone_<ts>.log` then copy to `panic_<ts>.log`" path produced two files per crash. Now a single `panic_<ts>.log` is written.
- **Log rotation removed.** `rotate_logs` and the related `[log] keep` setting have been removed. Files in `%LOCALAPPDATA%\popone\logs\` now only exist as a result of explicit user action (manual export) or panics, so the auto-deletion bucket is no longer appropriate. Existing `popone.toml` files with a `[log] keep = N` line continue to load (the field is silently ignored).

### Internals

- New module `popone/src/viewer/log_viewer.rs` with handwritten `[HH:MM:SS.mmm][LEVEL] message` parser, ring buffer (20,000 line cap), incremental filter index, and 17 unit tests covering parser edge cases (multi-line concat, leading fragment after byte-level drain, CRLF, level filtering, geometry round-trip, in-session reopen).
- `LogViewerModel` is held behind `Arc<Mutex<LogViewerModel>>` so the `show_viewport_deferred` closure (which requires `Fn + Send + Sync + 'static`) can capture it via `Arc::clone`.
- Window position/size are captured every frame from the child viewport so the geometry round-trips correctly across in-session close/reopen and across process restarts.

## v0.3.0 (2026-04-11)

Initial public release. Focused on documentation MECE restructuring, UX improvements, and UnityPackage-related bug fixes.
