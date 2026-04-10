<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [Changelog](#changelog)
  - [v0.2.41](#v0241)
    - [Improvements](#improvements)
    - [Documentation & Build](#documentation--build)
  - [v0.2.40](#v0240)
    - [Security](#security)
    - [Improvements](#improvements-1)
    - [Bug Fixes](#bug-fixes)
    - [Code Quality](#code-quality)
  - [v0.2.39](#v0239)
    - [Performance](#performance)
    - [New Features](#new-features)
    - [Bug Fixes](#bug-fixes-1)
    - [Architecture](#architecture)
    - [Refactoring](#refactoring)
  - [v0.2.38](#v0238)
    - [Performance](#performance-1)
    - [Improvements](#improvements-2)
  - [v0.2.37](#v0237)
    - [Bug Fixes](#bug-fixes-2)
  - [v0.2.36](#v0236)
    - [Improvements](#improvements-3)
  - [v0.2.35](#v0235)
    - [Improvements](#improvements-4)
    - [Documentation](#documentation)
  - [v0.2.34](#v0234)
    - [New Features](#new-features-1)
    - [Improvements](#improvements-5)
  - [v0.2.33](#v0233)
    - [New Features](#new-features-2)
    - [Improvements](#improvements-6)
  - [v0.2.32](#v0232)
    - [New Features](#new-features-3)
    - [Code Quality & Performance Improvements](#code-quality--performance-improvements)
  - [v0.2.31](#v0231)
    - [New Features](#new-features-4)
    - [Improvements](#improvements-7)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# Changelog

[日本語](CHANGELOG.jp.md)

## v0.2.41

### Improvements

- **Chinese text display support** — Added `NotoSansSC-Regular.otf` (Simplified Chinese) as a fallback font after `NotoSansJP-Regular.ttf`. Model names, author names, and other metadata from Chinese-region VRM/FBX models now render correctly instead of showing □ (tofu). The font fallback chain is JP → SC → egui default
- **Info tab auto-switch on model load** — The side panel now automatically switches to the Info tab when a new model is loaded, so the user immediately sees the model metadata. Append loading does not change the current tab
- **Configurable theme colors** — Added `[theme]` section to `popone.toml`. Six key colors are customizable: `panel_bg`, `border`, `accent`, `text`, `widget_bg`, `active`. Values are 6-digit hex strings (e.g. `"4A90D9"` or `"#4A90D9"`). Unspecified fields fall back to the default dark theme

### Documentation & Build

- **Third-party license notices** — Added `THIRD_PARTY_NOTICES.md` documenting the SIL Open Font License for the bundled fonts (NotoSansJP + NotoSansSC). The license section in both READMEs now references this file, clearly separating code license (0BSD) from bundled asset licenses
- **GitHub Actions CI** — Added `.github/workflows/ci.yml` with Windows runner: format check (`cargo fmt`), clippy lint (`-D warnings`), build (CLI + viewer), and test suite
- **Building from Source documentation** — Added build instructions to both READMEs, including Windows SDK recommendation for exe icon embedding
- **Graceful icon embedding** — `build.rs` no longer panics when Windows SDK (`rc.exe`) is unavailable; instead it emits a warning and produces an exe without a custom icon

## v0.2.40

### Security

- **Texture path traversal prevention** — `sanitize_rel_path()` strips `..` components and Windows drive letters (e.g. `C:`) from texture relative paths before joining with the model base directory. Applied to all 4 direct-disk loading paths: DirectX .x, OBJ, PMX, and PMD. Archive-based loading was already protected by `normalize_archive_path()`. This prevents malicious model files from reading arbitrary files outside the model directory
- **Absolute path bypass prevention** — `sanitize_rel_path()` additionally strips any path component containing `:` (Windows drive letters), preventing absolute paths like `C:/secret.png` from bypassing `base_dir.join()` constraints

### Improvements

- **Settings stored in `%LOCALAPPDATA%\popone`** — Configuration (`popone.toml`), texture history (`popone_history.json`), and logs are now stored in `%LOCALAPPDATA%\popone` on Windows instead of next to the executable. This prevents write failures when installed in read-only locations (e.g. `Program Files`). Existing files are automatically migrated on first launch. Falls back to the exe directory on non-Windows platforms
- **7z memory peak reduction** — After extracting entries from a 7z archive, the original compressed data is immediately released when the source file is persistent (non-temp). Previously both compressed and decompressed data were held simultaneously
- **Configurable log level and retention** — Added `[log]` section to `popone.toml` with `level` (error/warn/info/debug, default: debug) and `keep` (log file retention count, default: 5). Config is loaded before logger initialization so settings take effect immediately

### Bug Fixes

- **Shader [Auto] not restored after manual override** — Switching the shader from Auto to another mode (e.g. Unlit) and back to Auto did not restore the original Auto-selected shader. `set_shader_selection(Auto)` set `auto_shader = true` but did not recalculate `use_mmd_path`, which remained at the value set by the previous override. Fixed by calling `normalize_shader_state()` after every UI shader selection change
- **HDR emissive materials defaulting to OFF** — Materials with `emissive_factor` components exceeding 1.0 (via `KHR_materials_emissive_strength`) had their per-material emissive toggle initialized to OFF. This caused VRM 1.0 models like Seed-san to lose emission on load. Removed the HDR auto-detection; all materials now default to emissive ON

### Code Quality

- **Clippy clean (`-D warnings`)** — Resolved all 96 clippy warnings: 57 auto-fixed, 25 manually fixed (iterator patterns, `copy_from_slice`, `Box`-wrapped large enum variant, struct literal initialization), 12 structural warnings suppressed with `#[allow]` (`too_many_arguments`, `type_complexity`)

## v0.2.39

### Performance

- **Asynchronous UnityPackage/archive loading** — File reading, `build_unity_package_index`, and `list_models` for `.unitypackage` and ZIP/7z archives are now executed on a background thread via `spawn_bg_index_load`, eliminating UI freezes during the initial indexing phase
- **Asynchronous model parsing from packages** — CPU-heavy FBX/VRM/Prefab parsing from `.unitypackage` and archive extraction + parsing are now executed on background threads via `spawn_bg_pkg_load` / `spawn_bg_archive_load`. Previously these ran synchronously on the UI thread
- **Background texture pre-decoding** — `pre_decode_textures` converts `TextureData::Encoded` (PNG/JPEG/TGA/PSD) to `TextureData::RawRgba` on the background thread, eliminating image decode cost from the main thread's `upload_textures_from_ir`
- **Frame-split GPU texture upload** — `PendingGpuBuild` uploads textures to GPU in batches of 4 per frame via `upload_single_texture`, preventing the main thread from blocking during large texture uploads. Applies to both initial load and append operations
- **Texture data zero-copy** — `take_fbx_and_textures` / `take_vrm` return types changed from `Vec<u8>` to `Arc<[u8]>`, and `embed_textures_into_ir` generified with `AsRef<[u8]>`. `pkg_textures` and reload paths unified to `Arc<[u8]>`, eliminating redundant `.to_vec()` copies
- **D&D temp file preload removed from UI thread** — `process_drag_and_drop` no longer calls `std::fs::read` / `collect_image_files_recursive` synchronously. File reading is delegated to the background parse thread via `read_data` / `collect_aux` closures
- **Background PMX conversion** — `execute_conversion` now clones the IR and spawns a background thread for `convert_ir_to_pmx_with_cancel`. The UI thread returns immediately, showing a "PMX変換中..." overlay with a cancel button
- **Background reload (File/Snapshot)** — `reload_current` dispatches File and Snapshot sources through the existing `spawn_bg_load` pipeline. Archive/UnityPackage sources remain synchronous. Reload snapshot is saved before dispatch and restored after GPU build completes
- **`TextureData::RawRgba` Arc sharing** — `pixels` field changed from `Vec<u8>` to `Arc<[u8]>`, and `mip_chain` entries from `Vec<u8>` to `Arc<[u8]>`. Cloning `IrModel` for PMX conversion is now near-zero-cost for texture data
- **`IrModel::clone_for_export`** — Lightweight clone that strips GPU-only data (`mip_chain`, `uvs1`) to minimize UI thread copy cost when spawning the PMX conversion thread
- **GPU pipeline warm-up during splash screen** — `GpuRenderer::new()` (shader compilation) and `ensure_pipelines()` (26 render pipelines × 4 configs) are now executed incrementally during splash screen display via `WarmupPhase` state machine, one phase per frame. Eliminates ~10s first-model-load freeze (measured: 76ms total in release build)
- **GPU model build CPU offloading** — `build_gpu_model_inner` split into `cpu_prep_model` (vertex dedup, normal averaging, morph precomputation — runs on background thread) and `gpu_finalize_model` (buffer/bind group creation — runs on main thread in <7ms). `PendingGpuBuild` state machine extended with 3-phase flow: texture upload → BG cpu_prep → GPU finalize
- **Incremental pkg thumbnail cache** — `apply_pkg_append_post` now calls `append_pkg_thumb_cache(start_index)` to generate thumbnails only for newly added textures, instead of `rebuild_pkg_thumb_cache()` which regenerated all thumbnails from scratch. Eliminates cumulative freeze growth during batch append (was: 15s→61s growing, now: ~7.6s constant)

### New Features

- **Load cancellation UI** — A "中止" (Cancel) button appears on the progress overlay during background loading, GPU build, and PMX conversion. Escape key also cancels. During reload, cancellation restores the previous model instead of clearing to empty
- **Escape key for selection dialogs** — FBX choice, OBJ/STL import options, UnityPackage model select, and archive model select dialogs can now be dismissed with the Escape key
- **FBX choice dialog via background** — The FBX model/animation selection dialog (`execute_fbx_choice`) now dispatches to `spawn_bg_load` / `spawn_bg_pkg_load` instead of synchronous parsing
- **PMX conversion cooperative cancel** — `convert_ir_to_pmx_with_cancel` checks a cancel flag between each step (texture write, PMX build, file write). Texture export checks per-texture. All output goes to a temp directory (`.popone_convert_tmp/`) and is moved to the final path only on success; on cancel the temp directory is deleted entirely

### Bug Fixes

- **PMX output filename truncation** — `sanitize_filename` now truncates model names exceeding 80 characters at a Unicode character boundary. Previously, VRM models with very long metadata names (e.g., detailed descriptions in `meta.name`) produced PMX filenames that could exceed Windows path limits or cause filesystem errors

### Architecture

- **`CpuParseInput` expanded** — Added `ArchiveModel`, `PkgModel`, `UnityPackageIndex`, `ArchiveIndex` variants for background processing of all load paths
- **`BgLoadKind` expanded** — Added `ArchiveInitial`, `ArchiveAppend`, `ArchivePreparedUnityPackage`, `PkgInitial`, `PkgAppend`, `NeedsFbxChoice`, `UnityPackageIndexed`, `ArchiveIndexed` variants with `Box<Payload>` structs to separate request and result data
- **`PendingGpuBuild` state machine** — GPU texture upload is split across frames (4 textures/frame). `start_deferred_gpu_build` is used for BG load results; reload paths also use this pipeline for File/Snapshot sources
- **Append GPU build deferred** — Append operations use `start_deferred_append_gpu_build_ext` with rollback support: on GPU build failure, the original model is restored via IR truncation + old GPU model
- **`build_ir_from_archive_bundle` free function** — Extracted from `&self` method to enable background thread invocation
- **`PendingConvertBg`** — Background PMX conversion state with `mpsc::Receiver` for result polling and `AtomicBool` for cancel. Polled in `process_pending_tasks`
- **`reload_snapshot` field** — `ViewerApp` stores a `ReloadSnapshot` during BG reload. On GPU build completion, `finish_reload_from_snapshot` restores the snapshot. On cancel, `restore_snapshot_on_failure` preserves the old model
- **`IrModel` / `IrMesh` / `IrPhysics` Clone derive** — Added `Clone` to support `clone_for_export` for background PMX conversion
- **`watchdog.rs` — Main thread responsiveness monitor** — A background watchdog thread monitors the main thread's heartbeat (`AtomicU64` epoch millis, threshold 5s, check interval 2s). If no heartbeat update is detected within the threshold, logs `[watchdog] Main thread unresponsive`; on recovery, logs total freeze duration. Uses a `PAUSED` sentinel value (`u64::MAX`) to suppress false positives when the window is minimized. The main thread calls `request_repaint_after(3s)` to maintain heartbeat during idle (no input / no animation)
- **`WarmupPhase` state machine** — 5-phase GPU pipeline warm-up during splash screen: `NotStarted` → `RendererCreated` → `SrgbMsaaDone` → `SrgbNoMsaaDone` → `Complete`. `ensure_pipelines` refactored with explicit `msaa: bool` parameter
- **`cpu_prep_model` / `gpu_finalize_model` split** — `build_gpu_model_inner` decomposed into CPU-only `cpu_prep_model` (vertex processing, `Send`-safe, runs on BG thread) and GPU-only `gpu_finalize_model` (bind group/buffer creation, main thread). New types: `CpuPrepResult`, `CpuDrawPlan`, `PerMatGpuMeta`, `AuxTexRefs`. `PendingGpuBuild` extended with `cpu_prep_rx` channel for 3-phase async flow
- **`append_pkg_thumb_cache` incremental method** — Generates thumbnails only for `pkg_textures[start_index..]` instead of full rebuild, preserving existing thumbnail GPU textures

### Refactoring

- **`spawn_bg_task` common helper** — Extracted shared boilerplate (cancel, mpsc channel, request_id, thread spawn) from 4 `spawn_bg_*` functions into a single `spawn_bg_task` helper. Each caller now only builds `CpuParseInput` and `fallback_kind`
- **`process_pending_tasks` split into `poll_*` methods** — Decomposed the ~450-line monolithic method into `poll_file_dialog`, `poll_dispatch_and_bg_load`, `poll_deferred_loads`, `poll_gpu_build`, `poll_export_tasks`, `poll_overlay_tasks`, `poll_convert_bg`. Introduced `poll_receiver` helper to deduplicate `try_recv` 4-branch pattern (7 occurrences)
- **`IrMesh` heavy fields Arc-wrapped** — `vertices`, `indices`, `morph_targets` changed from `Vec<T>` to `Arc<Vec<T>>`. Clone is now O(1) reference count. Mutation via `vertices_mut()` / `indices_mut()` / `morph_targets_mut()` using `Arc::make_mut` (COW)
- **`assign_texture_core` common method** — Unified `assign_texture_source_to_material` (file path) and `assign_texture_data_to_material` (byte data) into a shared core. Fixed missing `mmd_texture_bind_group = None` clear on the file-path path
- **Append rollback typed** — Introduced `IrRollbackSnapshot`, `LoadedModelOwnership`, `AnimationSnapshot` structs to replace manual field-by-field save/restore in append operations
- **`TmpDirGuard` RAII cleanup** — Replaced 5× manual `.inspect_err(|_| cleanup())?` pattern in `convert_ir_to_pmx_with_cancel` with a Drop-based guard. `disarm()` on success path prevents cleanup
- **`MaterialBuildFlags` struct** — Consolidated 4 parallel slice arguments (`smooth_per_mat`, `clear_per_mat`, `normal_map_per_mat`, `emissive_per_mat`) into a single struct across `build_gpu_model` / `cpu_prep_model` / `PendingGpuBuild`
- **`write_model_opt_cancel`** — `PmxWriter` now supports cooperative cancellation between sections (vertices, faces, textures, materials, bones, morphs). `write_pmx_and_stats` passes the cancel flag through

## v0.2.38

### Performance

- **Prefab texture resolution index** — `UnityPackageIndex` now builds a `prefab_by_fbx_guid` reverse-lookup map and `prefab_cache` at index construction time. `resolve_prefab_textures` uses O(1) HashMap lookup instead of O(P×F) full scan of all `.prefab` entries per FBX
- **Variant resolution cache** — `resolve_variant_multi` results are cached in `variant_cache`. `resolve_variant_multi_inner` reads from `prefab_cache` instead of re-parsing Prefab YAML on each recursive call
- **TextureData Arc sharing** — `TextureData::Encoded` changed from `Vec<u8>` to `Arc<[u8]>`. Texture data from `.unitypackage` is now shared via `Arc::clone` (O(1)) instead of `to_vec()` full copy
- **Material GUID deduplication** — Replaced `Vec::contains()` (O(N²)) with `HashSet` (O(N)) for material GUID uniqueness checks in `resolve_prefab_textures` and `resolve_variant_multi_inner`
- **Parallel Prefab parsing** — `build_prefab_fbx_map` uses `rayon::par_iter` to parse all Prefab YAML files in parallel during index construction

### Improvements

- **Batch loading progress toast** — Multi-model batch loading now shows per-model progress as a toast notification (e.g. "読み込み中 (2/5)：model.fbx"). Progress info is stored in `PendingPkgModelLoad.batch_progress` to survive `PendingMultiLoad` cleanup on the last item

## v0.2.37

### Bug Fixes

- **Appended Prefab reload index mismatch** — Fixed a bug where A-stance / T-stance conversion on an appended `.unitypackage` Prefab model caused `Prefab parse failed` errors. `reload_append_unitypackage` was calling `extract_all_assets()` and `build_unity_package_index()` separately, producing two `HashMap`-based entry arrays with non-deterministic iteration order. The asset index returned by pathname lookup in the first array could point to an unrelated file (e.g. a `.shader`) in the second array. The fix builds `UnityPackageIndex` once and derives the `ExtractedAsset` list from its entries, matching the pattern already used in `try_load_unitypackage_for_append`

## v0.2.36

### Improvements

- **lilToon emission screen-blend attenuation** — lilToon materials with `_EmissionBlend: 1` (Screen mode) now have their `emissive_factor` attenuated by 0.5× to approximate screen blend compositing (`base + emission*(1-base)`), which is always darker than additive. Previously, Screen-mode emission was rendered as pure additive, causing excessive brightness and bloom white-out on materials like Refrain_V2
- **Prefab `_UseEmission` priority** — Emission detection in the Prefab path (`unitypackage.rs`) now prioritizes lilToon's `_UseEmission` float over the Standard shader's `_Emission` float. Previously, materials with `_UseEmission: 0` could be falsely detected as emissive due to `_EmissionMap` texture presence fallback
- **UI terminology consistency** — Standardized per-material emission toggle labels from mixed `Bloom/Emissive` / `Bloom (グロー)` to purpose-appropriate terms: post-process effect → `Bloom`, per-material emission → `エミッシブ`. Renamed internal field `MaterialDisplayState::bloom` → `emissive` and all related variables for clarity

## v0.2.35

### Improvements

- **Single instance IPC long path support** — The Named Pipe listener now handles `ERROR_MORE_DATA` by looping reads until the full message is received, supporting arbitrarily long file paths (including deeply nested Japanese directory names in UTF-8). Partial data from failed reads is discarded to prevent corrupted paths from being processed. Buffer size increased from 32KB to 64KB. Pipe handles are now managed via a RAII wrapper (`WinHandle` with `Drop` impl) to prevent handle leaks on early returns or panics
- **LogBuffer performance** — Replaced `Vec<u8>` with `VecDeque<u8>` for the in-memory log buffer. Front truncation on overflow (`drain(..excess)`) is now O(1) amortized instead of O(N) memmove
- **aux_files clone reduction** — PMX/PMD loading in `load_model_from_path_core` now extracts `aux_files` once and reuses it for both model parsing (by reference) and `ReloadableSource` construction (by move), eliminating a redundant `HashMap` clone per load
- **reload_from_source clone reduction** — Removed the intermediate `source_clone` variable; the function now matches directly on the `&ReloadableSource` parameter and clones only once at the point of `finish_load`, halving the number of deep clones per reload
- **Pipeline panic diagnostics** — `gpu.rs` `pipelines()` method now uses `expect()` with a descriptive message instead of `unwrap()`, making it easier to diagnose missing `ensure_pipelines` calls

### Documentation

- **Trademarks & Acknowledgments section** — Added trademark attribution for VRM (VRM Consortium), FBX (Autodesk), glTF (Khronos Group), DirectX (Microsoft), PSD (Adobe), PMX/PMD, OBJ, and STL. Added shader technology credits for MToon, UTS2, lilToon, and Poiyomi with license information
- **Dependency license table updates** — Added 5 missing crates (`toml`, `dunce`, `tempfile`, `encase`, `env_logger`) to both dependency lists and license tables. Fixed `encoding_rs` repository URL (`nickel-org` → `hsivonen`). Fixed `dunce` repository URL (GitHub → GitLab `kornelski/dunce`)

## v0.2.34

### New Features

- **Prefab append loading** — Prefab models can now be appended to an already-loaded model. The `append_from_pkg` function resolves the Prefab's GUID reference chain, extracts all referenced FBX files, applies texture mapping via `embed_textures_with_prefab`, and merges them into the existing scene. Previously, selecting a Prefab in append mode returned an error
- **Multi-model batch loading** — The `.unitypackage` model selection dialog now supports checkboxes for selecting multiple models at once. A "Load selected (N)" button loads the first model normally and appends the rest sequentially via a `PendingMultiLoad` queue. Single-click loading is preserved for quick single-model selection

### Improvements

- **Zero-copy asset sharing** — `take_fbx_and_textures` / `take_vrm` now accept `&[ExtractedAsset]` instead of consuming `Vec<ExtractedAsset>`. `PendingPkgModelLoad.assets` uses `Arc<Vec<ExtractedAsset>>` for shared ownership, eliminating per-model asset duplication during batch loading
- **Batch abort on failure** — When any model in a batch load fails or the FBX load-mode dialog is cancelled, the remaining queue (`PendingMultiLoad`) is immediately cleared to prevent stale appends

## v0.2.33

### New Features

- **lilToon / Poiyomi shader detection (Phase 3)** — Extended `ShaderFamily` enum with `LilToon` and `Poiyomi` variants, auto-detecting these shaders from VRM 0.0 `materialProperties.shader` field. Detected parameters are approximate-mapped to `MtoonParams`: shade color, shadow border/blur, outline (width/color/mask), rim light, MatCap, emissive, normal map, alpha mode, and cull mode. 2nd shadow color is mapped to PMX ambient. PMX conversion preserves extracted ambient/specular values (same pattern as UTS2). Detection uses shader name matching + property-only fallback (`_lilToonVersion` for lilToon, `_EnableShadow` + `_Shadow1stColor` for Poiyomi)

### Improvements

- **Shader family display in logs** — Material list logs now show `shader=MToon` / `shader=lilToon` / `shader=Poiyomi` / `shader=UTS2` / `shader=-` instead of `mtoon=true/false`. Added `Display` trait implementation for `ShaderFamily` enum
- **Shader support documentation** — Added "Shader Support" section to usage docs (both JP/EN) with shader detection criteria table and reproduction fidelity table per shader family

## v0.2.32

### New Features

- **Individual toon texture generation (Phase 2)** — MToon/UTS2 materials now generate per-material toon gradient textures (256×16 PNG) from `shade_color` → `diffuse` instead of using shared toon01–toon10. The generated images are written to `textures/` and referenced via `PmxToonRef::Texture(index)`. Non-MToon materials retain `Shared(0)` and materials without `shade_color` retain `Shared(2)`. Filename collision with existing model textures is prevented via a `used_names` set, and PMX texture paths are corrected after file write
- **OBJ/STL import options dialog** — OBJ and STL files now show an import settings dialog when opened in the viewer, allowing the user to select the coordinate unit (mm / cm / m / inch) and Z-Up → Y-Up conversion toggle. Default values match the previous hardcoded behavior (OBJ: cm/Y-Up, STL: mm/Z-Up). CLI retains the default behavior without dialog

### Code Quality & Performance Improvements

- **`path_ext_lower()` utility** — Extracted the `.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase()` pattern (repeated 35+ times) into a single `path_ext_lower()` function at crate root, accessible from both viewer and non-viewer builds
- **Camera bbox helper consolidation** — Unified 4 identical bbox → camera method call patterns into `camera_reset_to_model()` and `camera_fit_to_model()` helper methods
- **`is_temp_path` cache** — Cached `temp_dir()` canonicalization and lowercase string in `OnceLock`, eliminating redundant computation across 19 call sites
- **anyhow chain cleanup** — Unified `ok_or_else(|| anyhow!(...))` patterns to `.context()` / `.with_context()` for consistency across `file_io.rs`, `main.rs`, and `texture.rs`

## v0.2.31

### New Features

- **Prefab `source_material` matching (Strategy 1)** — FBX extraction now sets `SourceMaterialRef` (renderer_path + slot_index) on each material using `GeometryInstance`, enabling precise texture mapping that matches Prefab renderer paths without relying on material name heuristics. The three-stage fallback is: Strategy 1 (source_material) → Strategy 2 (material_name) → Strategy 3 (source_texture_name)

### Improvements

- **`link_same_name` scope restriction** — The "同名連動" (same-name linking) feature for texture assignment is now scoped to the same `MaterialGroup` (i.e., the same model instance). Previously, appending the same FBX twice and changing a texture on one would propagate to the other; this is no longer the case
- **Reload stable key (`PkgModelLocator`)** — `.unitypackage` reload paths (`reload_archive_unitypackage`, `reload_append_unitypackage`) now use `selected_pkg_model` (GUID/pathname-based) for model re-selection instead of basename-only matching. This prevents misidentification when multiple models share the same filename (e.g., `Assets/A/Body.fbx` and `Assets/B/Body.fbx`). VRM and append models also store `PkgModelLocator` for accurate reload
- **`resolve_pkg_model_for_cli`** — Added CLI model resolver that selects an FBX from the package using `--fbx-name` hint with pathname fuzzy matching, providing structured error messages with candidate lists
- **`apply_resolved_textures` helper** — Extracted common texture application logic (base texture, normal map, emission) from `embed_textures_with_prefab` into a shared helper, reducing code duplication between Strategy 1 and Strategy 2
- **`compute_geometry_world_transform` removal** — Replaced with `GeometryInstance.world_transform` from `FbxScene.geometry_instances()`, eliminating the duplicate world transform computation

