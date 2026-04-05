<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [Changelog](#changelog)
  - [v0.2.28](#v0228)
    - [Bug Fixes](#bug-fixes)
    - [Code Quality & Performance Improvements](#code-quality--performance-improvements)
  - [v0.2.27](#v0227)
    - [New Features](#new-features)
    - [Bug Fixes](#bug-fixes-1)
    - [Code Quality & Performance Improvements](#code-quality--performance-improvements-1)
  - [v0.2.26](#v0226)
    - [New Features](#new-features-1)
    - [Bug Fixes](#bug-fixes-2)
    - [Code Quality & Performance](#code-quality--performance)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# Changelog

[日本語](CHANGELOG.jp.md)

## v0.2.28

### Bug Fixes

- **Animation load incorrectly cancelling in-progress model load** — Fixed a regression where `route_load_dispatch` performed cancellation *before* determining dispatch intent. Dropping a `.vrma` / `.anim` / glTF-animation / animation-only FBX onto the window while a model load was in progress would cancel the prior model load, and then the animation side would also fail because `self.loaded.is_none()` — losing both. The fix moves intent detection before cancellation: animation-only requests no longer cancel the prior `bg_load`. If a `bg_load` is in progress when an animation request arrives, the animation request is rejected with a message telling the user to retry after the model load finishes, preserving the current model load
- **Cancellation granularity** — Fixed the issue where the cancel-flag check in `cpu_parse_model` existed only at the function entry, leaving threads that had already entered parsing to run to completion (wasting CPU/I/O). Added check points at the start of each format arm, after heavy I/O such as `read_data` / `load_glb` / `read_pmx`, and around the `extract` call, so cancellation propagates incrementally at dispatch boundaries

### Code Quality & Performance Improvements

- **`BackgroundLoadState` enum consolidation** — Merged `PendingState`'s `load_dispatch: Option<PendingLoadDispatch>` and `bg_load: Option<BgLoadHandle>` fields into a single `bg_state: BackgroundLoadState`. The 3-variant state machine (`Idle` / `PendingDispatch { dispatch, prior_loading }` / `Loading(BgLoadHandle)`) represents the load state at the type level, eliminating invalid combinations like "both Some" or "one left stale". The `PendingDispatch.prior_loading: Option<BgLoadHandle>` field carries the prior handle when a new dispatch arrives while a load is in progress, so `route_load_dispatch` can decide per intent (model request vs animation-only request) whether to cancel or preserve it. A new `BackgroundLoadState::submit_dispatch` helper unifies all 4 dispatch entry points (file dialog result, D&D, IPC, command-line argument)
- **Background load cancellation** — Introduced an `Arc<AtomicBool>` cancel token so that a new load request can cancel an in-progress load. The initial v0.2.27 implementation **rejected** new requests with an error while a prior load was running; v0.2.28 instead **cancels the prior load and accepts the new request** (except for animation-only requests that depend on the in-progress model, which are still rejected). `cpu_parse_model` checks the cancel flag at multiple points and bails out immediately with `"bg load cancelled"`. Cancellation-origin errors are logged via `log::info!` only and are not surfaced to the UI
- **Background load generation tracking (request_id)** — New `BgLoadHandle { rx, cancel, request_id }` struct issues a fresh id from `ViewerApp.next_request_id` on every `spawn_bg_load` call. On receive, `BgLoadResult.request_id` is matched against the current handle's `request_id`, and mismatches are discarded as stale. This prevents the case where a prior load just barely completes and sends its result after being cancelled, which would otherwise overwrite the current-generation model
- **FBX reload temp directory collision fix** — The fixed-name `%TEMP%\popone_fbx_reload` directory used to stage external textures during Snapshot reload has been replaced with `tempfile::TempDir`. Each invocation now gets a unique `popone_fbx_reload_XXXXXX` directory, eliminating collisions during concurrent reloads, and `Drop` handles automatic cleanup so the explicit `remove_dir_all` call is no longer needed

## v0.2.27

### New Features

- **Asynchronous model loading** — VRM / FBX / PMX / PMD / OBJ / STL / DirectX .x model parsing now runs on a background thread, eliminating UI freezes that previously lasted several seconds after file selection. Implemented with `std::thread::spawn` + `mpsc::channel`; the main thread polls results each frame via `try_recv()`. Camera and window operations remain responsive while the "Loading..." overlay is displayed
- **Asynchronous file dialogs** — Texture replacement, model open, and model append dialogs are now non-blocking. UI stays responsive while the dialog is shown
- **Raw RGBA texture bypass** — VRM/GLB raw pixel data is now stored directly in `IrTexture.data` without PNG encoding, and GPU upload skips PNG decoding. Identified by `mime_type = "image/x-raw-rgba8"` + `raw_dims: Option<(u32, u32)>`. Eliminates the PNG encode/decode roundtrip for VRMs with many 4K textures
- **Background mipmap pre-generation** — Added `IrTexture.mip_chain` field. VRM extraction now pre-generates the mip chain (level 1 onward) on the background thread. The main thread only uploads via `queue.write_texture`. UI freeze time for KizunaAI_KAMATTE.vrm (26 textures, 4K resolution) reduced from 7.6s to 0.5s (15x improvement)

### Bug Fixes

- **IrTexture test compilation fix** — Fixed 4 test `IrTexture` literals in `export_filter.rs` that were missing the `source_path` field, which broke `cargo test --features viewer`
- **Archive VRM/GLB PNG normalization** — Fixed missing `encode_ir_textures_as_png` call in the VRM branch of `build_ir_from_archive_bundle`
- **PMD/PMX sphere map read regression** — Fixed `.sph`/`.spa` textures becoming empty (magenta fallback) when loading non-temp PMD/PMX files. `cpu_parse_model` always collected aux files via `collect_image_files_recursive`, but that function doesn't include `.sph`/`.spa` extensions, so sphere maps were missing. Non-temp PMD/PMX now uses `pmd_to_ir(path)` / `pmx_to_ir(pmx_dir)` to read from disk directly (same as pre-v0.2.26 path), and only temp/D&D uses the `*_with_aux` path via `preloaded.aux_files`
- **Asynchronous load result discard on re-submission** — Fixed the bug where submitting a new load while one is in progress would overwrite the existing receiver channel, silently discarding the previous thread's completion result. `route_load_dispatch` now rejects new dispatches when `bg_load` is in progress and shows an error message
- **Asynchronous texture dialog stale material index** — Fixed the issue where opening the texture dialog, then loading a different model, and confirming the dialog would apply the stored `mat_idx` to the new model's materials (or panic). The dialog result reception now verifies `mat_idx < loaded.ir.materials.len()` and discards out-of-range results. Additionally, `finish_load_with_gpu` clears `pending_file_dialog` on model switch
- **DirectX .x texture Y flip** — Fixed textures being displayed upside-down for `.x` files (a regression introduced in v0.2.24 when DirectX .x support was added). Removed the `Vec2::new(tc.x, 1.0 - tc.y)` Y flip. DirectX .x uses D3D convention with UV (0,0) at the upper-left, the same as PMX/FBX, so no flip is needed (OBJ uses OpenGL convention with lower-left origin and does need the flip)
- **Non-viewer build regression** — Fixed `cargo check` / `cargo test` (without `--features viewer`) failing with `could not find viewer in the crate root`. The mipmap generation helper in `vrm/extract.rs` was calling `crate::viewer::texture::rgba8_to_linear_f32` / `linear_f32_to_rgba8`, but the `viewer` module is gated behind `#[cfg(feature = "viewer")]`. Extracted the sRGB↔linear LUT conversion helpers into a new feature-independent `crate::color` module, consumed by both `vrm::extract` (CLI path) and `viewer::texture` (GPU path)

### Code Quality & Performance Improvements

- **Unified load entry points** — New `PendingLoadDispatch` struct unifies all load entry points (file dialog result, IPC receive, D&D including temp files, command-line args) through `pending.load_dispatch`. Removes `self.preloaded` from global state by embedding `preloaded: Option<PreloadedData>` in the dispatch
- **Centralized post-processing** — New `apply_bg_load_result` method centralizes post-load work (animation clear, FBX auto-animation, coordinate system compatibility check)
- **`cpu_parse_model` free function** — Extracted CPU parsing from `try_load_*` methods into a free function (no `&self`) that is safe to call from background threads
- **`route_load_dispatch` routing** — Handles format detection, animation detection, FBX choice dialog, and archive/UnityPackage sync fallbacks on the main thread, sending only regular model loads to the background
- **sRGB↔linear conversion LUT** — `srgb_to_linear` (256 entries) and `linear_to_srgb` (4096 entries) are now lookup tables initialized lazily via `OnceLock`, eliminating `powf` calls. Color space conversion during mipmap generation is several times faster

## v0.2.26

### New Features

- **Mipmap generation** — Textures now generate a full mipmap chain on upload. Mip levels are computed as `floor(log2(max(w,h))) + 1` and downsampled in linear color space (sRGB decode → resize → sRGB encode) for physically correct blending. Eliminates moiré and aliasing when the camera is pulled back. NPOT textures are fully supported
- **Texture assignment tracing** — All texture assignments are logged with `source_path` showing the origin: `embedded`, `prefab(name.prefab): Assets/...`, `archive(name.zip): file.png`, or file path. Enables troubleshooting of texture mismatch issues
- **File open / archive model selection logging** — `Open file`, `Append file`, `Load from archive`, `Unitypackage indexed`, `Archive indexed`, `Model loaded` events logged for full traceability

### Bug Fixes

- **UnityPackage texture isolation** — FBX nearby texture search is now disabled for archive-sourced models (`fbx_path=None`), preventing accidental assignment of textures from unrelated folders
- **UNC path normalization** — `\\?\UNC\server\share` paths are now correctly normalized to `\\server\share` (previously became `UNC\server\share`)
- **MMD shader constant** — Added `ALPHA_DISCARD_THRESHOLD` constant to MMD shader macro (was missing after magic number refactoring, causing PMX/PMD load crash)
- **Dark theme persistence** — `setup_dark_theme()` is now re-applied on first `update()` frame via flag, working around eframe's post-init style reset

### Code Quality & Performance

- **Dark theme initialization** — `setup_dark_theme()` is now called once at startup instead of every frame, eliminating redundant `Style` clone and `set_style()` overhead per frame
- **Morph buffer reuse** — `apply_morphs_to_buf` now reuses the existing `morph_visited` buffer via `fill(false)` + `resize()` instead of allocating a new `Vec<bool>` each frame
- **WGSL shader deduplication** — Introduced `wgsl_mtoon_bindings!`, `wgsl_mtoon_helpers!`, and `wgsl_fs_outline!` macros to eliminate copy-paste duplication between main and outline shaders (-107 lines). sRGB/Unorm variants are parameterized
- **MaterialParams struct** — Replaced `create_material_bind_group`'s 43 positional arguments with a named `MaterialParams` struct (3 arguments), preventing argument order bugs
- **unwrap() elimination** — Removed all `unwrap()` calls from production code (53+ locations). Render path uses `if let` guards with draw-skip fallback; parsers use `expect()` with invariant descriptions or error propagation via `?`
- **Bloom BindGroup caching** — Bloom pass BindGroups are now cached and only recreated when offscreen textures change (resize/MSAA toggle), eliminating 2 `create_bind_group` calls per frame
- **Bloom intermediate buffer precision** — Bloom downsample/upsample chain upgraded from `Rgba8Unorm` (8-bit, 256 levels) to `Rgba16Float`, eliminating banding artifacts in HDR emissive bloom gradients
- **Transparent sort caching** — Transparent draw call centroid calculation and depth sort are now skipped when camera eye position, vertex buffer, and draw count are unchanged from the previous frame
- **Skinning coordinate pre-transform** — Per-vertex PMX→glTF→skin→PMX triple conversion (6 calls/vertex/frame) replaced with a single per-bone `M*delta*M` conjugation in PMX space, removing all per-vertex coordinate transforms
- **render_to_texture decomposition** — 835-line God Function split into 6 helper methods (`build_camera_uniform`, `build_draw_queue`, `draw_standard_meshes`, `draw_mmd_meshes`, `draw_highlight`, `draw_overlays`) with a 265-line orchestrator
- **MaterialDisplayState struct** — 4 parallel `Vec<bool>` fields (smooth_normals, clear_normals, normal_map, bloom per material) consolidated into `Vec<MaterialDisplayState>`
- **DynamicBuffer struct** — 7 visualization buffer triplets (buf/capacity/vertex_count) consolidated into `DynamicBuffer` with shared `upload()` method
- **Lazy pipeline creation** — 4 pipeline sets (100+ pipelines) no longer compiled at startup; only the needed set is created on first use via `ensure_pipelines()`
- **ReloadSnapshot** — 20+ manually saved/restored fields in `reload()` consolidated into `ReloadSnapshot` struct with symmetric `save`/`restore_on_success`/`restore_on_failure` methods
- **Named Pipe robustness** — Pipe buffer enlarged from 4KB to 32KB; `\\?\` prefix stripped from `canonicalize` paths for compatibility (UNC paths correctly normalized to `\\server\share`)
- **bone_children Clone elimination** — `SkinningData.bone_children` field removed; animation now references `IrBone.children` directly, eliminating 200+ heap allocations per model load
- **Fuzzy bone match O(n) optimization** — `bone_name_to_idx.values().any()` O(n²) lookup replaced with `HashSet<usize>` O(1) containment check
- **create_pipeline_set simplification** — 14 positional arguments reduced to `&self` method with 3 arguments (`device`, `use_unorm`, `msaa`)
- **Reverse-Z depth buffer** — Depth clear changed from 1.0→0.0, compare from Less→Greater, projection near/far swapped. Dramatically improves depth precision at distance, eliminating Z-fighting on large models
- **Grid integer loop** — Float accumulation loop (`x += step`) replaced with integer-indexed loop to eliminate precision drift on large grids
- **WGSL PI constant** — Hardcoded `3.14159` (5 digits) replaced with `const PI: f32 = 3.14159265` (8 digits) in shader
- **Magic number elimination** — Named constants for MMD ambient scale, edge offset, bone display radii, sphere segments, dark theme colors (Rust + WGSL)
- **FileFormat enum** — Centralized file extension detection via `detect_format()`, replacing 4 scattered match blocks
- **bool-to-f32 helper** — `b2f()` function replaces 9 instances of `if x { 1.0 } else { 0.0 }`
- **pos_fn utility** — `coord::pos_fn(is_vrm0)` replaces 4 duplicated VRM0/VRM1 coordinate function selections
- **Toon texture compression** — 100 lines of hardcoded RGB data replaced with `toon_step()`/`toon_rle()` const fn generators (~45 lines)
- **Error chain preservation** — `ResultExt::context()` now wraps errors in `WithContext` variant preserving `source()` chain instead of stringifying; `PoponeError::Anyhow` variant holds `anyhow::Error` structurally
- **Expression channel pre-mapping** — Per-frame HashMap iteration in `apply_expressions` replaced with pre-built `Vec<(String, usize)>` index mapping
- **`#[expect(dead_code)]`** — 5 `#[allow(dead_code)]` converted to `#[expect(dead_code)]`; 1 removed (code was actually used)
- **format_number optimization** — Double-reverse char iteration replaced with single forward pass
- **Log messages in English** — All `log::info/warn/error/debug` messages converted to English for searchability. UI-facing messages (`ConvertMessage`) remain in Japanese
