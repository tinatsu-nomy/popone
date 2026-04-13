<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [Changelog](#changelog)
  - [v0.5.3 (2026-04-13)](#v053-2026-04-13)
    - [New Features](#new-features)
    - [Internals](#internals)
  - [v0.5.2 (2026-04-13)](#v052-2026-04-13)
    - [New Features](#new-features-1)
    - [Internals](#internals-1)
    - [Bug Fixes (Pre-Release Review)](#bug-fixes-pre-release-review)
  - [v0.5.1 (2026-04-13)](#v051-2026-04-13)
    - [New Features](#new-features-2)
    - [Performance](#performance)
    - [Internals](#internals-2)
    - [Bug Fixes (Pre-Release Review)](#bug-fixes-pre-release-review-1)
    - [Tests](#tests)
    - [Deferred → v0.6.0](#deferred-%E2%86%92-v060)
  - [v0.5.0 (2026-04-13)](#v050-2026-04-13)
    - [New Features](#new-features-3)
    - [Behavior Changes](#behavior-changes)
    - [Tests](#tests-1)
  - [v0.4.0 (2026-04-11)](#v040-2026-04-11)
    - [New Features](#new-features-4)
    - [Behavior Changes](#behavior-changes-1)
    - [Internals](#internals-3)
  - [v0.3.0 (2026-04-11)](#v030-2026-04-11)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# Changelog

[日本語](CHANGELOG.jp.md)

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
