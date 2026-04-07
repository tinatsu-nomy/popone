<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [Changelog](#changelog)
  - [v0.2.35](#v0235)
    - [Improvements](#improvements)
    - [Documentation](#documentation)
  - [v0.2.34](#v0234)
    - [New Features](#new-features)
    - [Improvements](#improvements-1)
  - [v0.2.33](#v0233)
    - [New Features](#new-features-1)
    - [Improvements](#improvements-2)
  - [v0.2.32](#v0232)
    - [New Features](#new-features-2)
    - [Code Quality & Performance Improvements](#code-quality--performance-improvements)
  - [v0.2.31](#v0231)
    - [New Features](#new-features-3)
    - [Improvements](#improvements-3)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# Changelog

[日本語](CHANGELOG.jp.md)

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

