<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [Changelog](#changelog)
  - [v0.5.12 (2026-06-11)](#v0512-2026-06-11)
    - [Bug Fixes](#bug-fixes)
    - [Internals](#internals)
    - [Tests](#tests)
    - [Scope Notes](#scope-notes)
  - [v0.5.11 (2026-05-16)](#v0511-2026-05-16)
    - [New Features](#new-features)
    - [Tests](#tests-1)
    - [Internals](#internals-1)
    - [Scope Notes](#scope-notes-1)
  - [v0.5.10 (2026-05-15)](#v0510-2026-05-15)
    - [Bug Fixes](#bug-fixes-1)
    - [Internals](#internals-2)
    - [Scope Notes](#scope-notes-2)
  - [v0.5.9 (2026-05-05)](#v059-2026-05-05)
    - [New Features / Improvements](#new-features--improvements)
    - [Internals (i18n housekeeping)](#internals-i18n-housekeeping)
    - [Scope Notes](#scope-notes-3)
  - [v0.5.8 (2026-04-22)](#v058-2026-04-22)
    - [Internals](#internals-3)
  - [v0.5.7 (2026-04-22)](#v057-2026-04-22)
    - [New Features](#new-features-1)
    - [Internals](#internals-4)
  - [v0.5.6 (2026-04-14)](#v056-2026-04-14)
    - [New Features](#new-features-2)
    - [Internals](#internals-5)
    - [Bug Fixes (Pre-Release Review)](#bug-fixes-pre-release-review)
  - [v0.5.5 (2026-04-13)](#v055-2026-04-13)
    - [New Features (Phase 1)](#new-features-phase-1)
    - [New Features (Phase 2)](#new-features-phase-2)
    - [New Features (Phase 3)](#new-features-phase-3)
    - [Internals](#internals-6)
    - [Scope Notes](#scope-notes-4)
    - [Bug Fixes (Pre-Release Review)](#bug-fixes-pre-release-review-1)
    - [Tests](#tests-2)
  - [v0.5.4 (2026-04-13)](#v054-2026-04-13)
    - [New Features](#new-features-3)
    - [Internals](#internals-7)
    - [Bug Fixes (Pre-Release Review)](#bug-fixes-pre-release-review-2)
    - [Tests](#tests-3)
  - [v0.5.3 (2026-04-13)](#v053-2026-04-13)
    - [New Features](#new-features-4)
    - [Internals](#internals-8)
  - [v0.5.2 (2026-04-13)](#v052-2026-04-13)
    - [New Features](#new-features-5)
    - [Internals](#internals-9)
    - [Bug Fixes (Pre-Release Review)](#bug-fixes-pre-release-review-3)
  - [v0.5.1 (2026-04-13)](#v051-2026-04-13)
    - [New Features](#new-features-6)
    - [Performance](#performance)
    - [Internals](#internals-10)
    - [Bug Fixes (Pre-Release Review)](#bug-fixes-pre-release-review-4)
    - [Tests](#tests-4)
    - [Deferred → v0.6.0](#deferred-%E2%86%92-v060)
  - [v0.5.0 (2026-04-13)](#v050-2026-04-13)
    - [New Features](#new-features-7)
    - [Behavior Changes](#behavior-changes)
    - [Tests](#tests-5)
  - [v0.4.0 (2026-04-11)](#v040-2026-04-11)
    - [New Features](#new-features-8)
    - [Behavior Changes](#behavior-changes-1)
    - [Internals](#internals-11)
  - [v0.3.0 (2026-04-11)](#v030-2026-04-11)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# Changelog

[日本語](CHANGELOG.jp.md)

## v0.5.12 (2026-06-11)

A bug-fix release. OBJ files that reference a `.mtl` living in a subdirectory now resolve the textures named inside that `.mtl` relative to the `.mtl`'s own directory instead of the `.obj`'s directory, so subdirectory-organized OBJ assets no longer load with missing textures.

### Bug Fixes

- **MTL subdirectory texture resolution** — When an `.obj` references a material library via `mtllib sub/dir/model.mtl`, the texture names written inside that `.mtl` (e.g. `map_Kd body.png`) are relative to the `.mtl`, not the `.obj`. Previously every texture was resolved only against the `.obj`'s directory, so any OBJ that kept its `.mtl` + textures in a subdirectory loaded with missing (white-fallback) textures. The loader now records each `mtllib` directory and tries it as a prefix before falling back to the `.obj` directory, so both subdirectory and flat layouts resolve correctly.

### Internals

- **Unified disk and in-memory OBJ paths** — `load_obj_with_params` now reads the file and delegates to `load_obj_from_data_with_params`, so disk loads go through the same custom `mtl_loader` closure used by archive / in-memory loads. This is what lets the disk path capture the `.mtl` directories (the previous `tobj::load_obj` default loader discarded them).

### Tests

- **OBJ MTL-subdirectory resolution tests** — Added `texture_resolves_relative_to_mtl_subdirectory` (a `.mtl` + texture nested under a subdirectory) and `texture_resolves_in_flat_layout` (regression for the `.mtl`-next-to-`.obj` case) to `obj/extract.rs`. Both write real files to a temp directory and assert the resolved texture bytes.

### Scope Notes

- No output format changes. Flat-layout OBJ assets (the common case) are unaffected — the `.obj`-directory lookup remains as a fallback.

## v0.5.11 (2026-05-16)

A quality-hardening follow-up to v0.5.10. It closes the verification gap left by the PSD→PSB auto-promotion (which previously had only estimator unit tests), adds automated WGSL shader compilation tests so a broken shader fragment fails `cargo test` instead of at viewer launch, and makes the PSD→PSB promotion explicit in the success toast. No output format changes: ordinary models still write a byte-identical `.psd`.

### New Features

- **Explicit toast when UV export auto-promotes PSD → PSB** — When the UV map writer crosses the 1.9 GiB threshold and switches the output to `.psb`, the success toast now uses a dedicated message stating that the format was auto-switched from PSD because the layer data exceeded the PSD 2 GiB limit (instead of silently showing a `.psb` path the user did not ask for). Ordinary `.psd` exports are unchanged. Added `viewer.toast.uvmap.exported_psb` to the `en` / `ja` / `zh` locale catalogs.

### Tests

- **PSD/PSB round-trip parse-back test** — `convert/uvmap.rs` gains a test that writes both a PSD and a PSB on a small canvas and re-parses the produced bytes, asserting the signature / version / header fields and the structural invariant `section_start + declared_layer_section_len + image_data_len == file_len`. This invariant fails the instant a length field overflows or is written at the wrong width — exactly the v0.5.10 silent-corruption failure mode — so the PSB container is now provably openable rather than only size-estimated.
- **naga WGSL shader compile tests** — The macro-assembled shader sources in `viewer/gpu.rs` (10 shaders) and `viewer/bloom.rs` (`BLOOM_SHADER_SRC`) are now fed through naga's WGSL front-end + validator (the same front-end wgpu uses). A syntax mistake in any `macro_rules!` fragment now fails `cargo test` instead of only surfacing when the viewer launches.

### Internals

- **`naga` dev-dependency** — Added `naga = { version = "24", features = ["wgsl-in"] }` under `[dev-dependencies]`. Version `24` resolves to the same `naga 24.0.0` that `wgpu 24` already pulls transitively, so no extra copy is added to the dependency tree.
- **CI runs viewer-gated tests** — `.github/workflows/ci.yml` gained a `cargo test --features viewer` step. The shader compile tests live under the `viewer` feature, so the pre-existing `cargo test` step (CLI-only) would never have executed them.

### Scope Notes

- No change to exported file bytes for ordinary models — `.psd` outputs remain bit-identical to v0.5.10. The only user-visible change is the more explicit toast wording when auto-promotion to PSB occurs.

## v0.5.10 (2026-05-15)

A targeted bug-fix release for the **UV map PSD output 2 GiB silent-failure**. UV map exports that previously produced a corrupt `.psd` (unopenable in Photoshop / Krita / Affinity / GIMP) now transparently switch to the **PSB (Large Document Format / `.psb`)** container when the estimated layer section would exceed the PSD `u32` length limit. No other behaviour changes; small models still write a regular `.psd`.

### Bug Fixes

- **UV map PSD 2 GiB silent corruption resolved** — Previously, exporting a UV map for a high-resolution / many-material merged model could silently produce a corrupt `.psd` because the PSD format encodes the layer-and-mask information section length as `u32` (≈ 2 GiB limit). The writer now estimates the layer section size up front and, when it crosses a conservative 1.9 GiB threshold, auto-promotes to PSB: the file signature flips to `8BPB`, the version to `2`, the relevant section / channel length fields widen from `u32` to `u64`, and the output extension is rewritten from `.psd` to `.psb`. The path actually written is returned through the export API so the toast and log lines reflect the real filename. Small-to-mid models continue to be written as ordinary PSD with no change in output.

### Internals

- **`PsFormat::Psd` / `PsFormat::Psb` enum in `convert/uvmap.rs`** — The writer now carries a format flag end-to-end. The three PSD/PSB structural deltas (outer "Layer and Mask Information" length, inner "Layer Info" length, and per-channel data length) are localised inside `write_section_length()` / `push_section_length()` helpers, so the body of the writer stays format-neutral.
- **`estimate_layer_section_bytes()` helper** — Computes a slight over-approximation of the layer section size (per-layer overhead rounded up to 512 bytes, plus `4 × (2 + pixel_count)` for content layers) and is compared against the new `PSD_TO_PSB_THRESHOLD_BYTES = 1.9 GiB` constant to decide format.
- **`export_uv_map_grouped()` return type** — Changed from `io::Result<()>` to `io::Result<PathBuf>` so the caller learns the actual path (including the `.psb` rewrite). `viewer/app/pending.rs` was updated to feed that path back into the success toast.
- **Tests** — Six new unit tests cover extension rewriting (`.psd` ↔ `.psb`), PSD vs PSB header bytes (`8BPS` / version 1 vs `8BPB` / version 2), the +24-byte expected size delta of the length fields between formats, layer-section size estimate monotonicity, and the realistic-payload threshold crossing (4096 × 4096 × 30 layers crosses the boundary; a single 4 k layer stays well below).

### Scope Notes

- File handling remains unchanged for ordinary models — no migration is required and existing `.psd` outputs are bit-identical to those produced by v0.5.9.
- PSB (`.psb`) is supported by Photoshop CS / 2021+, Krita, Affinity Photo, and GIMP (via plug-in). The promotion threshold is intentionally conservative (1.9 GiB rather than the hard 2 GiB limit) to leave headroom for the per-layer record overhead the estimator cannot tightly bound.

## v0.5.9 (2026-05-05)

An **internal i18n housekeeping release** for `popone`. CLI help / error messages / viewer UI strings are now resolved dynamically through `rust-i18n`, and every Japanese comment, `assert!` / `expect()` / `panic!` message remaining inside the Rust sources has been translated to English. End-user behaviour is unchanged from v0.5.8 — the UI labels still render in Japanese, just resolved through the i18n catalog. The release also bundles small UI improvements: a resizable right-side panel, better UV-edit-window resize behaviour, and a unified format for `log_viewer.toml`.

### New Features / Improvements

- **Resizable right-side panel with width persistence** — The right-side tab panel (Info / Display / Export / Animation / Archive / etc.) is now an `egui::SidePanel::resizable(true)`. The width can be adjusted by dragging the panel border, and the chosen width is persisted to `popone.toml` under `[window] right_panel_width`. The previous fixed-width layout cramped the material editor and the file tree on smaller windows.
- **Stable on-screen model size when toggling the material editor** — Because changing the right-panel width also resizes the 3D viewport, opening or closing the material editor used to make the model visibly grow or shrink. The fix adjusts camera distance (rather than FOV) when the right panel toggles, so the rendered model keeps the same on-screen size before and after.
- **UV edit window: improved resize behaviour and locked 1:1 UV aspect** — The UV edit window is now a resizable `egui::Window` with a sane minimum size, so it can be enlarged or shrunk freely to inspect the texture background and UV wireframe. Inside the canvas the UV space is rendered with a fixed **1:1 aspect ratio**, so resizing the window in either direction never distorts the UV layout.
- **Unify `log_viewer.toml` format with `popone.toml`** — The persistence format for the log viewer window position / size is now the same `[window]` section layout (`outer_x` / `outer_y` / `inner_w` / `inner_h`) used by the main `popone.toml`. Both toml files now share an identical schema, simplifying external tooling and manual edits.

### Internals (i18n housekeeping)

The bulk of this release is the **`rust-i18n`-based dynamic resolution of CLI and viewer strings, plus an English-language sweep across Rust sources**, split across roughly 50 commits in the `v0.5.8..v0.5.9` range. The project policy is now: **logs = English (fixed) / UI = locale-switchable via `t!()` / source comments = English**.

- **CLI and internal error i18n** — Commits `89a00e0` through `bb8d7e3` localised CLI help, `--dump` output, error messages, the `Error:` prefix, every Japanese string baked into `anyhow::Context` chains, every `#[error]` attribute on `thiserror` derives, and the loader code under `loaders` / `archive` / `vrm/extract` / `pmx/build` / `unitypackage` / `obj/directx`. The previous "untranslatable Japanese strings deep inside texture decoding and material loading" problem is gone — every user-facing message now goes through `t!()`.
- **Viewer UI literal strings → `t!()`** — Stages A-1 through A-9 incrementally migrated every viewer-side string literal to `t!()`. Order: side-panel skeleton (tabs + section headings) → top / status / shortcut bars → 6 dialogs (33 keys) → toasts (cancel / precondition / bg_failure / progress / append / anim / reload / uvmap / texture / history) → material editor + texture drop dialog → UV edit window → animation controls → VRM meta panel (permissions / license dictionary) → display tab + morph filter (A-1) → info tab + texture picker + PMX badge (A-2) → material list / texture column (A-4) → export tab + convert toast + uv_edit hints (A-5) → log viewer window (A-6) → status bar + D&D overlay (A-7) → ImportUnit + progress overlay + cancel (A-8) → file tree + MMD texture section (A-9) → leftover cleanup (PMX log + IPC eprintln). Each batch was kept warning-free under `cargo clippy --features viewer -- -D warnings`.
- **Viewer `assert!` / `expect()` / `panic!` messages translated to English** — Panic-path messages cannot rely on the i18n catalog being loaded, so they are explicitly **excluded** from `t!()` and standardised to English. Initial sweep of 11 sites + a follow-up batch of 40 = 51 sites in total.
- **Viewer source comments in English** — Comments under `viewer/` were translated batch by batch (small files batches 1–5, large files batch 1, the largest file `app/file_io.rs`, then `app/mod.rs`, `gpu.rs`, and `ui.rs`). The remaining Japanese-comment count under `viewer/` went from **3,646 → 0**.
- **Non-UI source comments in English** — Comments outside the viewer (convert pipeline, mid-size files, archive, unity, ray-mmd MME, intermediate types, `pmx/build`, `vrm/extract`, test code) were translated through batches 2 through 4f.
- **uvmap test fix-up** — Two `uvmap` unit tests had been matching error strings literally and broke when those strings moved into `t!()`. They were re-pointed at the resolved English messages so the test suite stays green.

### Scope Notes

- The Japanese UI labels users see are now routed through `t!()` instead of being baked in, but the wording and layout are unchanged from v0.5.8. The persistence schema for `popone.toml` is also fully backward-compatible — only the new `right_panel_width` field has been added.
- Log output language remains **fixed to English** (the `log` crate's sink). UI locale is now structurally swappable via `popone.toml`, but only the default Japanese locale ships in this release.

## v0.5.8 (2026-04-22)

A maintenance release that pins the CI Rust toolchain in-repo so that local and GitHub Actions builds use the exact same compiler. No behavioral or feature changes.

### Internals

- **Add `rust-toolchain.toml`** — A new `popone/rust-toolchain.toml` declares `channel = "1.93.1"`, `components = ["rustfmt", "clippy"]`, and `profile = "minimal"`. Running any `cargo` command under `popone/` now causes `rustup` to install and switch to that exact version automatically, eliminating "works on my machine" issues caused by stable point-release drift between contributors.
- **Add `rust-version = "1.93"` to `Cargo.toml`** — The Cargo MSRV metadata is now explicit, so older toolchains running `cargo install` or `cargo build` get a clear error message before compilation starts.
- **Switch CI Rust install from `dtolnay/rust-toolchain@stable` to `actions-rust-lang/setup-rust-toolchain@v1`** — `dtolnay/rust-toolchain` does not auto-read `rust-toolchain.toml` and requires an explicit `toolchain` input, which would force a duplicate of the version string in CI. `actions-rust-lang/setup-rust-toolchain@v1` (officially maintained by the Rust workgroup) reads `rust-toolchain.toml` by default and respects the `channel`, `components`, and `profile` declared there, so the CI workflow no longer duplicates the toolchain version or component list.
- **Include `rust-toolchain.toml` in the CI cache key** — `actions/cache@v4`'s key changed from `hashFiles('Cargo.lock')` to `hashFiles('rust-toolchain.toml', 'Cargo.lock')`, so the `target/` cache invalidates automatically when the Rust version is bumped, preventing stale artifacts compiled by a different rustc from being reused.

## v0.5.7 (2026-04-22)

Fixes a visible regression on PMX models whose internal texture list references a file that does not exist on disk, and adds a runtime toggle for the fallback color.

### New Features

- **White texture fallback (default)** — When a PMX texture references a path that has no real file on disk (e.g. `textures\Skin.png` while the actual asset lives under `toon\`), or when decoding fails for any other reason, the previous behaviour baked a **1×1 magenta** pixel into GPU and used it everywhere the missing texture was referenced. For toon / sphere slots used in multiplicative / additive composition, this caused strong pink/magenta color bleeding on the affected material (commonly the face). v0.5.7 replaces the fallback pixel with **1×1 white (255,255,255,255)** by default, which neutralises the bleed without changing any other lighting path.
- **Display option: `テクスチャ欠落時フォールバックを白に` toggle** — A new checkbox under the Display tab (below "MSAA") lets you switch between the white (default) and the historical magenta fallback on demand. The magenta mode is kept as a diagnostic option for spotting missing assets quickly. The preference persists across sessions under `[display] white_texture_fallback` in `popone.toml`.
- **Dynamic switchover** — Toggling the option is immediate: no model reload required. All failure paths now share a single 1×1 fallback texture via `queue.write_texture`, so only 1 pixel needs to be rewritten on the GPU — material BindGroups and draw pipelines are left untouched, and the new color appears in the next frame.

### Internals

- `viewer/texture.rs` gains a `SharedFallback { tex, srgb_view, unorm_view }` singleton behind a `Mutex<Option<_>>`, lazily initialised on first failure-path upload. All three fallback paths — empty `IrTexture.data` (`upload_single_texture`), `decode_image_to_rgba_with_hint` failure (same), and the unsupported `gltf::image::Format` branch (`upload_textures`) — return clones of the shared sRGB / Unorm `TextureView` pair. Because wgpu `TextureView::clone` only bumps an internal Arc, per-failure allocation goes from one 1×1 `wgpu::Texture` to zero.
- `set_white_texture_fallback_dynamic(enabled, &queue)` flips the `AtomicBool` and, if the shared texture is already initialised, calls `queue.write_texture` with 4 bytes. Not reloading the GPU view means no BindGroup re-binding, so the toggle is safe to press mid-frame.
- `DisplaySettings` gains a `white_texture_fallback: bool` field (default `true`) mirrored into a new `AppConfig.display: DisplayConfig` section for persistence. `DisplayConfig` uses `#[serde(default)]` throughout so existing `popone.toml` files without a `[display]` block load without issues.

## v0.5.6 (2026-04-14)

Two follow-up improvements to the UV editor.

### New Features

- **PMX UV morph IR→PMX roundtrip writeback** — Up through v0.5.5, `IrMorphKind::Uv` was stubbed out as an empty Group by the PMX writer, so any UV morph edits were lost on PMX save. `build_morphs` in `build.rs` now emits `PmxMorphOffsets::Uv` directly, leveraging the fact that the IR global vertex index produced during `build_vertices_and_faces` (sequential `mesh.vertices` push) is identical to the resulting PMX vertex index. The morph type byte (UV0=3, UV1..4=4..7) is reconstructed from `channel`. Duplicate offsets on the same vertex are coalesced and the output is sorted by `vertex_index` for deterministic writes. The full "PMX load → UV morph edit → PMX save → reload" loop now round-trips.
- **Auto-set morph weight to 1.0 during edit mode** — Entering UV morph edit mode in the UV editor immediately stashes `app.morph_weights[active_morph]` and sets it to `1.0`, restoring the original value on exit (whether via ComboBox switch, "out-of-list" fallback after IR change, or any future code path). The stash/restore logic is centralised in the new `UvEditState::switch_active_morph` helper, so all entry points share consistent behaviour.
- **Side-panel slider lock during edit** — While a UV morph is being edited, the corresponding row in the "表情モーフ" side panel disables its slider, `0`/`1` buttons, and DragValue, and shows a `(UV編集中)` hint next to the morph name. The "全リセット" (reset all) button also skips the locked morph to prevent the stash/live-value drift.

### Internals

- `UvEditState` gains a `morph_weight_saved: Option<f32>` field. The new `switch_active_morph(new_morph, &mut weights)` helper is the only sanctioned way to change `active_morph`; direct assignment is by design discouraged. `reset()` also clears `morph_weight_saved` to ensure stale indices from a previous IR are dropped on reload.
- `pmx/build.rs` `build_morphs` log statistics now include a `uv` count (`Morphs: N (vertex=A, group=B, uv=C)`). Out-of-range vertex indices are warned and skipped defensively.

### Bug Fixes (Pre-Release Review)

- **[Codex 0.5.6/01 P1]** Reloading or A-/T-stance conversion while a UV morph was being edited would persist the locked `1.0` weight: `save_reload_snapshot` captured the temporary value, and `finish_load_with_gpu`'s `uv_edit.reset()` then dropped `morph_weight_saved`, so the post-reload weight stayed at `1.0` with no way back. Fixed by calling `switch_active_morph(None, &mut self.morph_weights)` at the start of `save_reload_snapshot` so the snapshot always captures the user-intended weight.
- **[Codex 0.5.6/02 P1]** UV morph edit overrides held the displayed value (`base + morph offset`) under a key that had no way to distinguish base vs morph. After reload, `apply_to_ir` was writing those overrides back to the base UV, baking the morph offset into base; re-enabling the morph then double-applied the offset, corrupting both the visible mesh and IR state. Fixed by clearing `overrides` / `pristine_uvs` / `undo` / `redo` / `selected` in `save_reload_snapshot` whenever a UV morph was being edited. Morph edits are already written to the IR via `write_displayed_uv`, so there is no need to preserve the overrides across a reload (and `overrides` is now strictly scoped to base-UV edits).
- **[Codex 0.5.6/03 P1]** As a consequence of fix 0.5.6/02, unsaved UV morph edits were being silently discarded on reload (the offsets `write_displayed_uv` wrote to the old IR were lost when the new IR was rebuilt from source). Fixed by adding `uv_morph_offsets` to `ReloadSnapshot`, capturing all UV morph offsets from the old IR in `save_reload_snapshot`, and writing them back to same-named morphs in `restore_snapshot_on_success`. Channel mismatches are warned and skipped. Morphs that were not edited simply overwrite themselves with identical values (no-op).
- **[Codex 0.5.6/04 P1]** The `uv_morph_offsets` map introduced in 0.5.6/03 used `HashMap<name, ...>`, which collapsed same-named UV morphs on `.collect()` (VRM/glTF files can contain multiple morphs sharing a name when `name_en` is empty). Switched to a `Vec<UvMorphOffsetEntry { name, name_en, channel, offsets }>` and changed the restore path to use **a used-flag array + full `(name, name_en, channel)` equality match**, so the Nth same-named morph is correctly restored from the Nth snapshot entry. Snapshot entries that fail to match any new-IR morph are logged as a warning and discarded.

## v0.5.5 (2026-04-13)

Introduces a **per-vertex UV editing window** invoked from the material editor panel. v0.5.4 delivered material-level UV transform (offset / scale / rotation). v0.5.5 goes one layer deeper with **Phase 1** (single-vertex editor + persistence + reload-safe state), **Phase 2** (texture-background preview, rectangle selection, zoom/pan, rotate/scale, undo/redo, Ctrl+A), and **Phase 3** follow-ups (additive/subtractive rect selection, detachable independent OS window, UV1 editing, 2D gizmo handles, PMX UV morph editing).

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
- **PMX UV Morph Editing (A-2)** — PMX morph types 3〜7 (UV0 morph / additional UV1〜UV4 morphs) are now read into the IR as `IrMorphKind::Uv { channel, offsets: Vec<(global_vi, [f32; 4])> }` instead of being silently dropped at extract time. A new `GpuMorphEntry::Uv` variant is added to the GPU morph pipeline and `apply_gpu_morph_recursive` now accumulates `(du, dv) * weight` into `vertex.uv` (channel=0) or `vertex.uv1` (channel=1) on every morph apply. The UV editor gains a "編集対象" ComboBox that lists every `Uv` morph in the active model; picking one switches the canvas into "morph edit mode" where read/draw/pick/drag/gizmo operate on `base_uv + morph_offset` and writes update the morph's per-vertex offset map (via `write_displayed_uv` / `read_displayed_uv`). Selecting a morph auto-syncs `active_uv_set` to the morph's channel and resets in-flight drag state, selection, and undo history so UV0, UV1, and per-morph edit spaces stay isolated. **Limitations:** (1) channels ≥ 2 (UV2〜UV4) are read and retained but not GPU-applied (vertex shader has UV0/UV1 only); (2) IR→PMX writer currently emits empty group morphs for `IrMorphKind::Uv` because the PMX-vertex-index reverse map is not kept, so round-trip writing of UV morphs is deferred to a later version; (3) the UV editor preview assumes weight=1.0 so users need to scrub the morph weight slider in the side panel to see the effect in the 3D viewport.

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
