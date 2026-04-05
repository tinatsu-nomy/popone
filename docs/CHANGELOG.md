<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [Changelog](#changelog)
  - [v0.2.26](#v0226)
    - [New Features](#new-features)
    - [Bug Fixes](#bug-fixes)
    - [Code Quality & Performance](#code-quality--performance)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# Changelog

[日本語](CHANGELOG.jp.md)

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
