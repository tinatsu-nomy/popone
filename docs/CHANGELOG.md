<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [Changelog](#changelog)
  - [v0.2.25](#v0225)
    - [New Features](#new-features)
    - [Improvements](#improvements)
  - [v0.2.24](#v0224)
    - [New Features](#new-features-1)
    - [Improvements](#improvements-1)
  - [v0.2.23](#v0223)
    - [New Features](#new-features-2)
    - [Improvements](#improvements-2)
  - [v0.2.22](#v0222)
    - [New Features](#new-features-3)
    - [Bug Fixes](#bug-fixes)
    - [Improvements](#improvements-3)
  - [v0.2.21](#v0221)
    - [Improvements](#improvements-4)
  - [v0.2.20](#v0220)
    - [Improvements](#improvements-5)
  - [v0.2.19](#v0219)
    - [New Features](#new-features-4)
    - [Improvements](#improvements-6)
  - [v0.2.18](#v0218)
    - [New Features](#new-features-5)
    - [Improvements](#improvements-7)
  - [v0.2.17](#v0217)
    - [Bug Fixes](#bug-fixes-1)
  - [v0.2.16](#v0216)
    - [New Features](#new-features-6)
    - [Bug Fixes](#bug-fixes-2)
    - [Improvements](#improvements-8)
  - [v0.2.15](#v0215)
    - [New Features](#new-features-7)
    - [Improvements](#improvements-9)
  - [v0.2.14](#v0214)
    - [Improvements](#improvements-10)
  - [v0.2.13](#v0213)
    - [Improvements](#improvements-11)
  - [v0.2.12](#v0212)
    - [Bug Fixes](#bug-fixes-3)
    - [New Features](#new-features-8)
    - [Improvements](#improvements-12)
  - [v0.2.11](#v0211)
    - [New Features](#new-features-9)
    - [Improvements](#improvements-13)
  - [v0.2.10](#v0210)
    - [New Features](#new-features-10)
    - [UTS2 Mapped Parameters](#uts2-mapped-parameters)
    - [Bug Fixes](#bug-fixes-4)
    - [Improvements](#improvements-14)
    - [v0.2.10 Not Yet Supported (Future)](#v0210-not-yet-supported-future)
  - [v0.2.9](#v029)
    - [New Features](#new-features-11)
    - [Improvements](#improvements-15)
    - [Bug Fixes](#bug-fixes-5)
    - [Implementation Details](#implementation-details)
    - [Code Quality & Performance](#code-quality--performance)
  - [v0.2.8](#v028)
    - [New Features](#new-features-12)
    - [Improvements](#improvements-16)
  - [v0.2.7](#v027)
    - [New Features](#new-features-13)
    - [Bug Fixes](#bug-fixes-6)
    - [Improvements](#improvements-17)
    - [Code Quality](#code-quality)
  - [v0.2.6](#v026)
    - [Bug Fixes](#bug-fixes-7)
    - [New Features](#new-features-14)
    - [Improvements](#improvements-18)
    - [Code Quality & Performance](#code-quality--performance-1)
  - [v0.2.5](#v025)
    - [Improvements](#improvements-19)
    - [Code Quality & Performance](#code-quality--performance-2)
  - [v0.2.4](#v024)
    - [Improvements](#improvements-20)
  - [v0.2.3](#v023)
    - [Improvements](#improvements-21)
  - [v0.2.2](#v022)
    - [Code Quality & Performance](#code-quality--performance-3)
  - [FBX Support](#fbx-support)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# Changelog

[日本語](CHANGELOG.jp.md)

## v0.2.25

### New Features

- **Session settings persistence** — Window size/position and last-opened model/texture directories are saved to `popone.toml` and restored on next launch. Supports multi-display setups (including sub-displays with negative coordinates). Position stored as `outer_rect`, size as `inner_rect` to prevent drift. Graceful fallback on corrupted config with auto-recovery from `.bak` files
- **Texture assignment history** — Manually assigned textures for FBX/OBJ models can be saved to `popone_history.json` and recalled later. Material matching uses index+name exact match with name-only unique fallback. Missing texture files are auto-skipped with result notification. Overwrite confirmation dialog displayed when history already exists
- **FBX animation confirmation on first load** — When an FBX file contains both mesh and animation, the selection dialog now appears even on first load (no model loaded yet). "Load model" checkbox is required (disabled) on first load. Content detection unified via `inspect_fbx()` helper

### Improvements

- **FBX PSD texture decoding** — PSD files in FBX auto-texture loading are now decoded using the built-in PSD decoder (`psd::decode_psd`). Previously all PSD textures failed to decode since the `image` crate does not support PSD format
- **FBX animation auto-load pre-check** — Auto animation loading after FBX model load now uses `inspect_fbx()` pre-check. Eliminates unnecessary WARN for FBX files without animations
- **Safe file writing** — Settings and history files use backup-based atomic write. Original file preserved on rename failure with auto-recovery from `.bak` files
- **Texture history recall accuracy** — `link_same_name` temporarily disabled during recall batch; assignment failures detected via `ConvertResult::Failure` and counted as skipped
- **Empty FBX animation no-op** — Suppresses unnecessary success messages for FBX files with empty animation channels
- **Window title** — Changed from "Model Viewer" to "POPONE Model Viewer"
- **Documentation restructure** — English docs are now the primary files (`.md`), Japanese docs renamed to `.jp.md`

## v0.2.24

### New Features

- **DirectX .x file loading** — Added support for DirectX text format (.x) files. Static meshes such as MMD accessories and stages can be viewed and converted to PMX. Custom text parser fully supports Mesh / MeshNormals / MeshTextureCoords / MeshMaterialList / Frame hierarchy transforms
- **DDS texture support** — Added DDS (DirectDraw Surface) texture decoding and display. Enabled `dds` feature in the `image` crate. DDS files are also selectable in viewer D&D and manual texture assignment dialogs

### Improvements

- **Frame hierarchy transforms** — Correctly applies `FrameTransformMatrix` to vertex positions and normals. Dynamically determines face winding based on world transform determinant for mirrored frames
- **Material reference resolution** — Supports `{ MaterialName }` shared material references. Materials from top-level, Frame scope, and MeshMaterialList are registered in a unified lookup table. Forward references resolved via 2-pass re-binding
- **Hard-edge normals** — Face-vertex normals from `MeshNormals` use `(position_index, normal_index)` keys to split vertices, correctly preserving hard edges
- **Binary/compressed .x detection** — Detects `xof ... bin` / `xof ... cmp` headers and returns clear error messages
- **SkinWeights detection** — Rejects .x files containing skinning data with an early error, preventing silent rig loss
- **Archive .x support** — Auto-detects and extracts .x files from ZIP/7z archives. Texture collection scope expanded to parent directory
- **Dot-separated names** — Correctly parses Blender-style sequential names like `Cube.001` / `Material.002`
- **Texture path normalization** — Improved robustness with `./` removal, `..` resolution, and case-insensitive matching
- **UV normalization fix** — Fixed `fract_uv(1.0)` incorrectly rounding to `0.0`. UV=1.0 is now preserved correctly, with UV map rendering edge clamping added

## v0.2.23

### New Features

- **OBJ file loading** — Added support for Wavefront OBJ (.obj) files. Automatic MTL material/texture loading; falls back to default white material if MTL is missing. Coordinates assumed as cm units, displayed at the same scale as FBX. Uses the tobj crate
- **STL file loading** — Added support for STL (.stl) files. Both ASCII and binary formats supported. Coordinates assumed as mm units with Z-Up, automatically converted to Y-Up. Zero-normal triangles have face normals recalculated
- **Dynamic grid** — Grid floor size and spacing automatically adjust to the model's bounding box. Default (±100 PMX units, spacing 5) is the minimum; only enlarged for large models. Also recalculated on model append

### Improvements

- **OBJ/STL in archives** — OBJ/STL files inside ZIP/7z archives are automatically detected and extracted. OBJ sidecar files (MTL, textures) are collected; STL extracts model only
- **OBJ normal generation** — OBJ files without normal data automatically compute smooth shading vertex normals from face normals
- **Robust OBJ sidecar resolution** — Case-insensitive path resolution and relative path normalization (`./` removal, `..` resolution) for archive/snapshot OBJ loading. Disk fallback disabled for archive sources to prevent environment-dependent misreads

## v0.2.22

### New Features

- **PMX export scale** — Allows specifying a scale multiplier when converting to PMX. Use `--scale 2.0` in CLI, or the "Scale" DragValue in the viewer's export tab. Applied to bone positions, vertex positions, morph vertex offsets, rigid body positions/sizes, and joint positions/move limits. Does not affect normals, rotations, or UVs. Default: 1.0
- **Auto-create dummy bone for boneless models** — When converting a model with no bones (e.g., static FBX meshes) to PMX, a single bone named after the model is automatically created at the origin `(0,0,0)`, with all vertex weights assigned to it. Standard bone insertion (IK, etc.) is skipped, producing a minimal valid PMX

### Bug Fixes

- **Boneless model PMX conversion** — Fixed an issue where converting a boneless FBX to PMX generated invalid IK references to non-existent leg bones, causing load errors in other PMX viewers
- **TGA texture decode failure** — Fixed FBX embedded and external TGA textures failing with `image::load_from_memory` format auto-detection. Added extension-based format fallback. Also corrected misleading "not found" log messages when the file exists but decode fails

### Improvements

- **Log level optimization** — Changed FBX animation load failure from ERROR to WARN (animation load attempt on static FBX is normal behavior). Changed missing arm/shoulder bone warnings during standard bone insertion from WARN to DEBUG (expected for non-humanoid models)

## v0.2.21

### Improvements

- **Material name tooltip** — Hovering over a material name in the material display panel shows a tooltip listing the texture files referenced by that material. Displays up to 5 types: base texture, sphere map, toon, normal map, and emissive
- **UV map export default directory** — The UV map export dialog now defaults to the directory of the loaded model file. If a PMX output path is already set, that directory takes priority
- **Edge drawing UI in Auto mode** — When the shader is set to Auto and the MMD rendering path is automatically applied (e.g., when loading PMX/PMD), the edge drawing ON/OFF toggle and edge thickness slider are now shown. Previously, these controls were only visible when MMD mode was explicitly selected

## v0.2.20

### Improvements

- **Expression morph filter** — Added text-based filter to the expression morph section in the control tab. Supports partial match search by Japanese and English names (case-insensitive). Filtering only affects display; hidden morph weights are preserved
- **Pose reset on animation clear** — Clearing animation (via the "Clear Animation" button or removing via ✕ button) now resets bone poses and animation-controlled expression morph weights. Previously, bone deformations and expression weights remained at their last-frame values
- **VRM permission/license color badges** — Permission and license values in the meta info panel are now displayed as colored badges: allow (green) / conditional (yellow) / deny (red) / neutral (gray). Both badges and labels show tooltips on hover (value meaning + VRM spec property name). Supports VRM 0.0/1.0 and all CC license types. Section and label names are shown in Japanese in the UI while PMX comment field remains in English
- **Splash image** — Displays a rounded-corner logo image centered in the viewport when no model is loaded. Embedded in the exe via `include_bytes!`, auto-scaled to fit the viewport. Automatically hidden once a model is loaded

## v0.2.19

### New Features

- **Per-material normal map toggle [N]** — Added `[N]` button to the material panel. Allows toggling normal map application ON/OFF for individual materials that have normal textures. When OFF, `MaterialUniform.has_normal_tex` is zeroed, skipping normal map sampling in the shader. Group header `[N]` enables batch toggling
- **Per-material Bloom/Emissive toggle [B]** — Added `[B]` button to the material panel. Allows toggling Bloom/Emissive ON/OFF for materials with emissive components. When OFF, `emissive_factor` is zeroed, disabling both the shader's `lit += emissive` addition and MRT bloom output. For PMX/PMD materials, `bloom_emissive` is also zeroed. Group header `[B]` enables batch toggling
- **HDR Emissive auto-detection** — Materials with any `emissive_factor` component exceeding 1.0 (HDR) are initialized with `bloom_per_mat` defaulting to OFF. Automatically prevents white-out caused by `lit += emissive` clamping in the viewer (which lacks tonemapping)

### Improvements

- **Removed [S]/[C] gating on normal-mapped materials** — Normal smoothing `[S]` and custom normal clear `[C]` are now always available, even for materials with normal maps. Since normal maps perturb normals via the TBN matrix (built from vertex normals + tangents), smoothing the base normals improves polygon edge visibility. The `mat.normal_texture.is_none()` guard in `mesh.rs` has been removed
- **Extended group header** — Now uses `▶ [S] [C] [N] [B] [☑] GroupName` layout with 4+1 buttons

## v0.2.18

### New Features

- **Prefab-based normal map application** — Resolves normal map textures (`_BumpMap` / `_NormalMap`) from `.mat` files within `.unitypackage` and applies them to FBX model viewer display. Also reads `_BumpScale` from the `.mat` `m_Floats` section to reflect normal map intensity. Prioritizes `_BumpMap` (Standard / lilToon / Poiyomi / AXCS / WF) with fallback to `_NormalMap` (UTS2)
- **Bloom (glow) post-effect** — Dual Kawase (Dual Filtering) bloom implementation. MRT (Multiple Render Target) separates emissive components only — grids and non-emissive surfaces do not glow. Render pass split into mesh drawing (MRT 2 targets) and overlay (1 target). Bloom intermediate buffers use Rgba8Unorm (linear) to avoid sRGB arithmetic artifacts. UI parameters: ON/OFF, intensity (0.0–4.0, default 0.8), threshold (0.0–1.0, default 0.0), radius (3–6 downsample stages, default 4). Bloom pass skipped when disabled (MRT 2-target rendering remains active; only additional bandwidth cost)
- **PMX/PMD self-emissive material bloom** — Materials with specular=(0,0,0) and specular_power≥100 are detected as self-emissive. Bloom intensity = (specular_power − 100) / 10 (sp=110 equals VRM emissive=1.0). bloom_emissive is output only to MRT @location(1), not added to scene color
- **Prefab Emission texture/color auto-application** — Added `m_Colors` section and `m_ShaderKeywords` / `m_ValidKeywords` parsing in `.mat` files. Auto-assigns `_EmissionColor` / `_EmissionMap`. Emission enabled by priority: `_Emission` float → `_EMISSION` keyword → `_EmissionMap` presence → `_EmissionColor` non-black non-white. When `_EmissionMap` is present but `_EmissionColor` is black, color is corrected to white

### Improvements

- **`.mat` parser MatSection enum refactoring** — Section transitions between `m_TexEnvs`, `m_Floats`, `m_Colors`, and `m_ShaderKeywords` are now safely managed via a `MatSection` enum. Supports YAML multi-line list format (`- _EMISSION`)
- **Texture assignment log improvements** — `embed_textures_with_prefab()` logs now separate base color and normal map counts (`base=X/Y, normal=Z/Y`). Unmatched detection remains base-color-only, preserving manual assignment dialog behavior
- **PMX output determinism** — `build_morphs()` now sorts by `vertex_index` after HashMap merge, ensuring stable PMX binary output across runs

## v0.2.17

### Bug Fixes

- **Zero-weight bind filtering for VRM 0.0 morph conversion** — VRM 0.0 BlendShapeGroup processing did not filter `weight=0` binds, causing zero-offset entries from all morph targets to leak into IrMorph. This generated tens of thousands of unnecessary entries per Expression, bloating PMX files. Fixed by skipping `weight == 0.0` binds, matching VRM 1.0 behavior
- **Viewer morph offset accumulation bug fix** — GPU morph data construction used `HashMap::collect()`, which overwrote duplicate vertex offsets (only the last entry survived). For Expressions where multiple morph target binds affect the same vertex (e.g., mouth_a + mouth_small sharing lip vertices), earlier offsets were silently lost. Fixed by using `entry().or_insert() += off` for proper additive blending
- **PMX morph vertex deduplication** — PMX export now merges multiple offsets for the same vertex by summing, and removes zero-result entries. Improves compatibility with PMXEditor

## v0.2.16

### New Features

- **Prefab-based texture mapping** — Select `.prefab` entries from `.unitypackage` files. Resolves textures by following Unity's GUID reference chain (`.prefab` → FBX `.meta` → `.mat` → texture), enabling accurate texture assignment independent of file names
- **Multiple Prefab format support** — Supports New (Unity 2018.3+), Old, Unpacked, and Mixed (Old + New coexisting) formats. Recursive Prefab Variant resolution with cycle detection and depth limiting
- **Multi-FBX merge from Prefab** — When a single Prefab references multiple FBX files (via Nested PrefabInstance / m_Mesh mix, etc.), all FBX are extracted and merged for display
- **Unified model selection dialog** — Prefab / FBX / VRM entries shown in a single selection dialog when loading `.unitypackage`. Prefab entries labeled with `[Prefab]`

- **File hierarchy tree** — Below the material display in the Display tab, shows the load chain (opened file → intermediate files → final model) as a tree. For Prefab loads, displays `.unitypackage` → `.prefab` → individual FBX tree structure. Textures, animations, and package textures are also visible
- **Always-on material grouping** — Materials are always grouped by model name with collapsible headers, even for single models. Unified UI experience with multi-model append
- **Per-FBX material group splitting** — Multiple FBX files within a Prefab are shown as separate material groups. Each FBX's materials are managed under independent collapsible headers
- **Per-group batch controls** — Group headers now include `[S]` (normal smoothing), `[C]` (custom normal clear), and `[☑]` (visibility) buttons for batch toggling all materials in the group. Hovering over the header row highlights all meshes in the group

### Bug Fixes

- **Prefab model texture reset on A/T stance conversion** — Fixed an issue where toggling A-stance / T-stance conversion on a Prefab-loaded model would reset all texture assignments. The root cause was `reload_unitypackage` not recognizing the Prefab's multi-FBX merge structure and reloading as a single FBX instead. Added `prefab_entry_path` to `LoadedModel` and a `reload_as_prefab` method that rebuilds `UnityPackageIndex` and reloads via the Prefab path
- **lilToon texture slot priority fix** — Fixed incorrect texture assignment when lilToon shader's `.mat` file has `_BaseColorMap` referencing a different texture than `_MainTex`. Texture slot priority is now explicitly `_MainTex` > `_BaseMap` > `_BaseColorMap`
- **Archive Prefab reload failure fix** — Fixed "Failed to load GLB file" error when reloading (e.g., A-stance conversion) a model loaded via Prefab from a `.unitypackage` inside a ZIP / 7z archive. The cause was `reload_as_prefab` not preserving the `Archive` source and falling back to `File(zip_path)`
- **FBX texture reset on A/T stance conversion fix** — Fixed an issue where toggling A-stance / T-stance conversion on an FBX directly selected from a `.unitypackage` would reset all texture assignments. The cause was `reload_unitypackage` falling back to simple name matching (`embed_textures_into_ir`) instead of Prefab-aware texture mapping (`embed_textures_with_prefab`). Added a branch that detects Prefab-aware loading via `pkg_material_keys` and uses `prepare_pkg_fbx` + `embed_textures_with_prefab` for re-mapping

### Improvements

- **State reset on new model load** — When opening a different model, shader settings (reset to Auto) and A-stance / T-stance conversion flags are now reset to defaults. Prevents unintended carry-over of previous model's settings
- **FBX parser input validation hardening** — Improved resilience against malicious FBX files: property count limit (1M), node recursion depth limit (64), array size limit (512MB), `end_offset` range validation, `checked_mul` for overflow prevention, `compressed_len` vs remaining bytes validation
- **unitypackage extraction size accounting** — `pathname` and `asset.meta` read bytes are now counted toward `total_bytes`, preventing bypass of the 2GB limit
- **Morph application performance** — Weights diff check skips unnecessary full vertex copy + GPU upload; `morph_visited` buffer reuse eliminates per-frame allocation
- **unitypackage memory reduction** — Changed `ExtractedAsset.data` from `Vec<u8>` to `Arc<[u8]>` to eliminate data duplication with `AssetEntry`
- **FBX animation double-read elimination** — Merged animation existence check and application into a single read after FBX model loading
- **Bone merge `to_lowercase()` caching** — Pre-cache `to_lowercase()` results in `IrModel::merge()` to avoid mass temporary `String` allocation
- **Spring bone chain depth limit** — Added `MAX_SPRING_CHAIN_DEPTH = 64` to `build_spring_chain_v0`
- **Win32 console handle fix** — Assign separate `CONOUT$` handles for stdout and stderr

- **UnityPackageIndex** — GUID-based index structure for efficient Prefab resolution and texture references. Index is also built for `.unitypackage` files loaded via archives (ZIP / 7z)
- **FBX material name matching improvement** — Extracts FBX internal material names (`fbx_material_name`) from FBX `.meta` `externalObjects`, enabling texture matching even when `.mat` file names differ from FBX internal names
- **Unity YAML escape support** — Auto-decode `\uXXXX` escape sequences and YAML quoted strings. Japanese material names are processed accurately
- **Three-stage texture matching** — `source_material` (renderer_path + slot) → `material_name` / `fbx_material_name` → `source_texture_name` (filename match) — 3-level fallback for maximum match rate

## v0.2.15

### New Features

- **Dark theme (Blender/Substance Painter style)** — Panel background `#1D1D1D`, widget background `#252525`, accent color `#4A90D9`. All buttons show accent blue on hover. Tooltips, popups, and ComboBoxes are unified
- **Status bar separation** — Keyboard shortcut hints and file info split into two independent bars. Viewport overlay shortcut text removed

### Improvements

- **Fixed side panel width** — 280px fixed (no resize). Flat-style equal-width tab bar (active tab = accent blue background)
- **Compact info tab** — 4-column Grid pairs: bone/vertex, face/material, texture/morph. Numbers formatted with comma separators
- **Export tab 2-column checkboxes** — PMX conversion options (rigid body alignment / no physics / original bone structure / visible materials only / log output) in 2-column grid layout
- **Top bar improvements** — Fit/Reset buttons moved from viewport overlay to top bar right side. All buttons: white text, transparent background, blue on hover
- **Animation list improvement** — `[▶][×] filename` layout. ▶ click to switch, × always visible
- **White text unification** — Side panel, status bar, and shortcut bar text unified to white

## v0.2.14

### Improvements

- **FBX external texture nearby search** — When FBX `RelativeFilename` / `FileName` paths don't match the actual directory structure (common with Unity/Blender project exports), added a fallback that recursively searches directories near the FBX file. `TextureSearchCache` ensures directory scanning happens only once
- **FBX texture name improvement** — Uses the actual filename (`LL_Skin`, etc.) instead of the FBX object name (`file3`, etc.) as the texture name
- **Mixed Japanese/English FBX bone mapping** — Added support for Blender-exported FBX files with mixed Japanese bone names (下半身/上半身/頭, etc.) and English bone names (RightArm/LeftForeArm, etc.). Added Japanese bone names to rig detection and extended `BLENDER_MAP` with Mixamo-style English names and Japanese bone names

## v0.2.13

### Improvements

- **Bone merge algorithm improvement** — Extended bone merging during append loading to a 3-level fallback method. Models with different bone naming conventions (Japanese vs English names) are now correctly merged
  - **Pass 1a**: Match by `vrm_bone_name` (humanoid bone name). VRM names are unique per skeleton, no parent check needed
  - **Pass 1b**: Match by `original_name` (FBX node name) with lowercase normalization. Parent consistency check included
  - **Pass 1c**: Match by `bone.name` (PMX name) + same parent name check (existing behavior, backward compatible)
- **Relaxed Blender rig detection** — Changed `detect_rig_type` Blender condition from `hips && head` to `hips && (head || spine)`. Partial skeletons such as costume FBX without Head bone are now detected as Blender rig
- **Pre-merge humanoid completion** — When appended model lacks humanoid information, `detect_humanoid` is re-run against `original_name` before merge to fill in `vrm_bone_name`
- **VRM confirmed flag** — Bones matched by `vrm_bone_name` are exempted from parent propagation cancellation (pass 2), ensuring semantically correct merging

## v0.2.12

### Bug Fixes

- **PSD textures not converted during PMX export** — PSD format textures are now decoded and saved as PNG during PMX conversion. PSD decode functions extracted to `src/psd.rs` shared module, available in CLI builds

### New Features

- **Material hover highlight** — Hovering over a material row in the material list highlights the corresponding mesh in the 3D view with semi-transparent orange overlay. Works across texture match dialog, D&D texture dialog, and side panel material list. Responds to indicator icons, checkboxes, and dropdown interactions
- **Real-time texture preview in manual assignment** — Selecting a texture in the archive texture match dialog immediately reflects it in the 3D view before pressing "Apply". Uses lazy GPU upload for VRAM efficiency
- **Auto-organized PMX output** — Each conversion creates an auto-numbered `converted_model01/`, `converted_model02/`... directory containing PMX + textures. Base output directory is configurable via UI
- **One-click PMX export** — Removed the file save dialog; "PMX Convert" button exports immediately. Output folder opens automatically in Explorer after conversion
- **Panic log preservation** — On panic, the log file is automatically copied to `panic_yyyymmdd_hhmmss.log`, excluded from log rotation cleanup

### Improvements

- **Dialog placement** — Texture match and D&D texture dialogs now open at the top-left corner, are draggable, collapsible, and resizable. Model visibility improved
- **State preservation** — Material visibility ON/OFF and "export visible only" settings are now preserved across A/T-stance toggle, normal smoothing, and custom normal clear operations

## v0.2.11

### New Features

- **Shader Override** — Added 6 shader modes to the viewer, switchable via ▲ ComboBox ▼
  - **Auto** — Automatically selects Standard (MToon/Lambert) or MMD based on model format (existing behavior)
  - **MToon/Lambert** — Forces Standard path. Displays PMX/PMD with MToon/Lambert shader
  - **Unlit** — No lighting, texture color only. Useful for texture inspection
  - **GGX Preview** — Simplified Cook-Torrance specular (metallic=0, roughness=0.8 fixed). Schlick Fresnel + GGX NDF + Smith geometry + hemisphere ambient
  - **Normal** — Visualizes normal direction as RGB color
  - **MMD** — MMD dedicated render path for PMX/PMD (consolidates the former MMD rendering checkbox)

### Improvements

- **2-axis shader state separation** — `shader_override` (GPU shader branching) and `use_mmd_path` (CPU render path selection) are managed independently. UI presents a unified 6-choice dropdown
- **Unified alpha processing** — Introduced `apply_alpha_mode()` WGSL helper function. Consistent alpha handling (OPAQUE / MASK+A2C / BLEND) across all shader modes
- **Texture alpha in OPAQUE materials** — OPAQUE mode now passes through texture alpha instead of forcing 1.0. PMX/PMD texture transparency displays correctly in all shader modes
- **CameraUniform shader_mode changed to u32** — Replaced `show_normal_map: f32` with `shader_mode: u32`. Integer comparison branching in WGSL
- **Mode-specific UI disabling** — Light/ambient sliders disabled in Unlit/Normal modes. Ambient disabled in MMD mode (existing behavior preserved)
- **Shader selection persistence** — Explicit selections (Unlit / GGX / Normal / MToon / MMD) are preserved across model loads. Only Auto mode auto-detects based on model format
- **Consolidated `show_normal_map` / `mmd_mode`** — Former individual checkboxes merged into shader selection dropdown

## v0.2.10

### New Features

- **UTS2 (Unity-Chan Toon Shader Ver.2) Support** — Auto-detect UTS2 shaders in VRM 0.0 models, approximate-map to existing MToon rendering pipeline for viewer display and PMX conversion
  - `ShaderFamily` enum (`Other` / `Mtoon` / `Uts2`) for multi-shader classification
  - Triple detection: shader name (`UnityChanToonShader/*`, `Toon/Toon`) + UTS2-specific properties (`_utsVersion`, `_BaseColor_Step`)
  - Explicit `ShaderFamily::Mtoon` for VRM 0.0 / VRM 1.0 MToon materials

### UTS2 Mapped Parameters

| UTS2 Property | Maps To |
|---|---|
| `_BaseColor` / `_MainTex` | Base color / texture |
| `_1st_ShadeColor` / `_1st_ShadeMap` | MToon shade_color / shade_texture |
| `_2nd_ShadeColor` | PMX ambient (`color * 0.5`) |
| `_BaseColor_Step` / `_BaseShade_Feather` | shading_toony / shading_shift |
| `_Outline_Width` / `_Outline_Color` | Outline (NML/POS → WorldCoordinates approx.) |
| `_RimLight` / `_RimLightColor` / `_RimLight_Power` | Rim lighting |
| `_MatCap` / `_MatCap_Sampler` / `_MatCapColor` | MatCap texture |
| `_Emissive_Tex` / `_Emissive_Color` | Emissive (HDR: kept linear) |
| `_NormalMap` / `_BumpScale` | Normal map |
| `_HighColor` / `_HighColor_Power` | PMX specular (PMX output only) |
| `_GI_Intensity` | GI (safe default 0.0) |
| `_CullMode` | Culling mode |

### Bug Fixes

- **Fixed PMX/PMD morphs not working correctly** — `generate_tangents` (MikkTSpace tangent generation) added in v0.2.9 splits vertices on tangent w mismatch, but the morph pipeline was not updated. Three bugs fixed:
  1. `ir_vertex_offset` used pre-split vertex count → global indices for subsequent meshes were shifted
  2. `ir.morphs` built from `pmx_to_ir_vertex` → split vertices not included in morph data
  3. Face winding order in `distribute_vertex_morphs` differed from `extract_meshes` → local index mismatch
  - Fix: Reordered to `mesh build → morph_targets distribution → generate_tangents (split + morph duplication)`, and `ir.morphs` now built from `mesh.morph_targets`. Same pattern applied to PMD
- **Fixed outline/MMD edge rendering as solid faces in Wire mode** — In wireframe mode, outline pipelines (`PolygonMode::Fill`) and MMD edge pipelines were not skipped, causing solid faces to appear. Now skips outline drawing and switches MMD materials to wireframe pipeline in Wire mode

### Improvements

- **Outline checkbox grayed out for non-MToon models** — "Outline drawing" checkbox is disabled (`ui.add_enabled`) for models without MToon outlines (PMD/PMX, etc.). Non-functional UI elements are now clearly non-interactive
- **Light settings color button alignment** — HSV color wheel buttons for Light, Ambient, and Ground are now column-aligned using `egui::Grid` layout. Previously, width differences between sliders and labels caused button misalignment
- **UTS2 alpha mode detection** — Shader variant name-based (`_TransClipping` → Blend, `_Clipping` → Mask). Falls back to glTF core `alpha_mode`
- **UTS2 outline POS mode** — POS outline approximated as WorldCoordinates (differs from MToon ScreenCoordinates), with warning
- **UTS2 ClippingMask warning** — Warning for unsupported `_ClippingMask` texture, falls back to base alpha
- **Ambient overwrite prevention** — UTS2 `_2nd_ShadeColor` ambient preserved (not overwritten by `diffuse * 0.4` recalculation)
- **PMX conversion UTS2 branch** — UTS2 materials preserve HighColor → specular, 2nd_ShadeColor → ambient (skips MToon specular suppression)
- **VRM 0.x helper consolidation** — `get_float` / `get_color3` / `resolve_tex` / `main_tex_st` shared between MToon/UTS2. `adopt_main_tex` centralizes `_MainTex` authoritative handling
- **MMD shader now reflects light color & intensity** — AmbientColor/SpecularColor in MMD rendering mode now multiply by light color (`light_color`) and intensity (`light_intensity`). Previously used a fixed scalar (154/255 ≈ 0.604) ignoring color/intensity changes. Default values (white, 0.7) produce identical results to before
- **MToon specular for PMX output** — MToon material PMX specular changed from zero to `diffuse × 0.2` (power=10). Specular highlights now respond to light direction changes in MMD
- **Ambient UI grayed out in MMD mode** — In MMD spec, LightAmbient serves as scene ambient, so ambient slider/Sky color/Ground color are disabled in MMD mode to prevent confusion

### v0.2.10 Not Yet Supported (Future)

- ClippingMask texture / HighColor viewer rendering / ShadingGradeMap / 2nd_ShadeMap texture / AngelRing / Stencil variants

## v0.2.9

### New Features

- **MToon 2-Color Toon Shading** — VRM MToon materials are now displayed with 2-color toon (lit/shade) shading in the viewer. `shadingToonyFactor` controls shadow boundary sharpness, `shadingShiftFactor` controls shadow threshold shift. Supports both VRM 1.0 (`VRMC_materials_mtoon`) and VRM 0.0 (`_ShadeToony` / `_ShadeShift`). Non-MToon materials continue to use Half-Lambert
  - Extended `MaterialUniform` from 16 to 80 bytes, adding `shade_color` / `is_mtoon` / `shading_toony` / `shading_shift` + outline parameters
  - Implemented spec-compliant `linearstep`-based lit/shade interpolation in the fragment shader (`dot(N,L)` [-1,1] range)
  - Added `shading_toony_factor` / `shading_shift_factor` fields to `IrMaterial`
- **MToon Outline Rendering** — Outline (contour) rendering using inverted hull method. Supports `outlineWidthFactor` (world coordinates / screen coordinates) and `outlineColorFactor`. `outlineLightingMixFactor` controls lighting mix ratio. Togglable via UI checkbox
  - Added `pipeline_outline` (front-cull pipeline) to `PipelineSet` (sRGB / Unorm variants)
  - Added `OutlineWidthMode` enum, `outline_width_factor`, `outline_lighting_mix` to `IrMaterial`
  - Reads from both VRM 1.0 (`outlineWidthMode` / `outlineWidthFactor` / `outlineLightingMixFactor`) and VRM 0.0 (`_OutlineWidthMode` / `_OutlineWidth` / `_OutlineLightingMix`)
  - Added `has_outline` flag to `DrawCall`, rendering outlines for all alphaMode materials (BLEND uses ZWrite OFF)
- **MToon Rim Lighting + MatCap** — Supports VRM 1.0 MToon parametric rim lighting and MatCap texture
  - Parametric rim: controlled by `parametricRimColorFactor` (color), `parametricRimFresnelPowerFactor` (Fresnel exponent), `parametricRimLiftFactor` (lift). Creates glowing silhouette edges via Fresnel effect
  - MatCap: supports `matcapTexture` / `matcapFactor`. Constructs orthonormal basis from view-space normal for UV calculation and samples the MatCap texture
  - `rimLightingMixFactor` controls ambient light mix ratio (0.0 = emission, 1.0 = fully mixed)
  - Extended `MaterialUniform` from 80 to 112 bytes, added MatCap texture bind group(3) to pipeline layout
  - Added world position output to vertex shader, implemented view-direction-based rim calculation in fragment shader
- **MToon Auxiliary Textures** — Support for 3 VRM 1.0 MToon auxiliary texture types, improving rendering quality
  - `shadeMultiplyTexture`: Shade color texture multiply (RGB). Enables per-pixel shade color variation for finer shadow expression
  - `shadingShiftTexture`: Per-pixel shading shift (R channel × scale). Controls shadow behavior per body region
  - `rimMultiplyTexture`: Rim lighting multiply texture (RGB). Controls rim effect application area via texture
  - Restructured bind group(3) as MToon auxiliary texture pack (per-texture samplers, 16 bindings). Extended `MaterialUniform` from 112 to 144 bytes
  - Materials without textures automatically bind default textures (white or black), eliminating pipeline switching
- **MToon UV Animation** — Support for VRM 1.0 MToon UV scroll and rotation animation
  - `uvAnimationScrollXSpeedFactor` / `uvAnimationScrollYSpeedFactor`: Horizontal/vertical UV scrolling
  - `uvAnimationRotationSpeedFactor`: UV center rotation
  - `uvAnimationMaskTexture`: B channel controls animation application area
  - Added cumulative `time` field to `CameraUniform`, updated every frame

### Improvements

- **MToon outline `outlineWidthMultiplyTexture` support** — Sample `outlineWidthMultiplyTexture` G channel in outline vertex shader via `textureSampleLevel`, added to mtoon_aux bind group (binding 6) with material-specific bind group used in outline draw calls. VRM models that suppress outlines on face/hair now display correctly
- **UV Animation for `outlineWidthMultiplyTexture` (MToon spec compliance)** — Extracted UV Animation calculation into shared `apply_uv_animation()` function, now also applied in outline vertex shader `vs_outline` before sampling `outlineWidthMultiplyTexture`. All 5 MToon UV Animation target textures (shadeMultiply / shadingShift / rimMultiply / outlineWidthMultiply + 3 glTF core) now have UV Animation applied. Added `VERTEX` to `uvAnimationMaskTexture` bind group visibility
- **MToon screenCoordinates outline improvement** — Implemented UniVRM-compliant clip-space normal transformation, aspect ratio correction (`height/width` for X-axis shrink), and camera-facing normal suppression. Eliminates outline thickness variation across different FOV and aspect ratios
- **MToon auxiliary texture color space fix** — `shadingShiftTexture` and `uvAnimationMaskTexture` now use linear color space (Unorm view) per spec. Fixes value distortion from double gamma conversion when using sRGB view
- **`shadingShiftTexture` formula spec compliance** — Changed from `(tex * 2.0 - 1.0) * scale` to VRM 1.0 spec-compliant `tex * scale`
- **`shadingToony/shadingShift` shading formula spec compliance** — Changed from `half_lambert` [0,1] + `smoothstep` to spec-compliant `dot(N,L)` [-1,1] + `linearstep(-1+toony, 1-toony, shading+shift)`. Shadow boundary sharpness and position now match UniVRM
- **`shadeColorFactor` default value spec compliance** — Fixed VRM 1.0 MToon `shadeColorFactor` default to `Vec3::ZERO` (black) per spec default `[0,0,0]`. Now always stores `Some(...)` during extraction so that the viewer and PMX conversion correctly distinguish between `None` (no shade_color) and "default black"
- **VRM 0.x `_Color` / `_MainTex` lit color/texture normalization** — Added normalization of VRM 0.x MToon `_Color` → `ir_mat.diffuse` and `_MainTex` → `ir_mat.texture_index` / `base_color_tex_info` from `materialProperties`. For VRM 0.x, the glTF core `baseColorFactor` / `baseColorTexture` may be approximate values, so `materialProperties` takes priority after MToon detection (per UniVRM `MigrationMToonMaterial.cs:148-164`)
- **VRM 0.x `_MainTex` ST Y offset conversion** — Added `offset.y = 1.0 - unityOffset.y - scale.y` conversion when transforming VRM 0.x `_MainTex` ST (Scale/Translation) to glTF `KHR_texture_transform`. Accounts for the Y-axis interpretation difference between Unity's texture coordinate system (top-left origin) and glTF (bottom-left origin) (per UniVRM `Vrm10MaterialExportUtils.ExportTextureTransform`)
- **`renderQueueOffsetNumber` range clamping** — Opaque/Mask forced to 0, BlendWithZWrite clamped to [0,+9], Blend clamped to [-9,0]. Matches UniVRM MToonValidator behavior
- **VRM 0.x `renderQueue` out-of-range validation** — Added UniVRM `GetRenderQueueRequirement`-compliant range check. Returns offset=0 when `renderQueue` falls outside the permitted range (Blend: 2951–3000, BlendWithZWrite: 2501–2550). Fixes draw order pinned to extreme values on broken/hand-edited VRM 0.x inputs
- **Fix `rimLightingMixFactor` light factor to be N·L independent** — Removed `dot(N,L) * 0.5 + 0.5` (Half-Lambert) from `light_factor` per UniVRM, now uses direct `light_intensity + ambient` composition. Rim lighting is a view-angle Fresnel effect and the light factor should remain constant regardless of N·L. Fixes rim appearing excessively dark at backlit/silhouette poses
- **Per-texture glTF sampler support** — Added `IrSamplerInfo` (wrap_u / wrap_v / mag_filter / min_filter) to `IrTextureInfo`, reading per-texture wrapS / wrapT / magFilter / minFilter from glTF `sampler` objects. Viewer GPU side uses `HashMap<IrSamplerInfo, wgpu::Sampler>` cache to share samplers with identical settings. CPU-side `sample_image_g_channel` also applies wrap mode-aware UV coordinate transformation. `CLAMP_TO_EDGE` specifications on `outlineWidthMultiplyTexture` / `uvAnimationMaskTexture` etc. are now correctly honored
- **Per-texture sampler in MToon auxiliary bind group** — Changed bind group(3) from a single shared sampler (1 sampler + 8 textures) to per-texture samplers (8 samplers + 8 textures = 16 bindings). Fully compliant with glTF's per-texture sampler model, ensuring different wrap/filter settings on auxiliary textures are correctly honored. WGSL replaced the shared `s_mtoon_aux` sampler with texture-specific samplers (`s_matcap` / `s_shade_multiply` / `s_normal`, etc.)
- **Preserve glTF minFilter mipmap information** — Split `IrFilterMode` (2 values: Nearest / Linear) into `IrMagFilter` + `IrMinFilter` (6 values: Nearest / Linear / NearestMipmapNearest / LinearMipmapNearest / NearestMipmapLinear / LinearMipmapLinear). Preserves the glTF `minFilter` mipmap selection mode as-is, with `ensure_sampler()` correctly separating wgpu's `min_filter` and `mipmap_filter`
- **Added `aspect` field to `CameraUniform`** — Used for MToon screenCoordinates outline aspect ratio correction
- **MToon transparent draw order control** — Support for glTF `alphaMode` (OPAQUE / MASK / BLEND) and MToon extensions `transparentWithZWrite` / `renderQueueOffsetNumber`. Rendering separated into spec-compliant 4 phases, correctly reproducing depth ordering for transparent bangs and accessories
  - Added `AlphaMode` enum (Opaque / Mask / BlendWithZWrite / Blend), `alpha_cutoff`, `render_queue_offset` to `IrMaterial`
  - Added `RenderQueue` enum to `DrawCall` with stable sort by `renderQueueOffsetNumber` within BLEND categories
  - MASK mode: implemented `alphaCutoff`-based `discard` in fragment shader
  - BlendWithZWrite: new transparent + depth-write pipelines (`pipeline_alpha_zwrite_cull` / `pipeline_alpha_zwrite_no_cull`)
  - Draw order: OPAQUE → MASK → BlendZWrite → Blend. OPAQUE/MASK outlines drawn after each phase, BLEND/BlendZWrite outlines interleaved per draw call
- **MToon auxiliary texture `texCoord` / `KHR_texture_transform` preservation** — Introduced `IrTextureInfo` struct to hold `texCoord` and `KHR_texture_transform` (offset / scale / rotation) for all 6 MToon auxiliary textures (shade / matcap / shadingShift / rimMultiply / uvAnimationMask / outlineWidth) at the IR layer. Mesh `TEXCOORD_1` is also read into `IrMesh.uvs1`. GPU shader applies `resolve_mtoon_uv()` for texCoord selection + KHR_texture_transform
- **Texture pruning for all MToon textures** — Rewrote export filter texture pruning to `IrTextureInfo`-based approach, adding matcap / shadingShift / rimMultiply / uvAnimationMask textures to the collection and remapping targets
- **MToon ScreenCoordinates outline formula to full UniVRM compliance** — (1) Fixed normal normalization order to match UniVRM (normalize → aspect multiply). (2) Added projection matrix `proj_11` (= 1/tan(fov/2)) to `CameraUniform` and implemented UniVRM's `MToon_GetOutlineVertex_ScreenCoordinatesWidthMultiplier` equivalent distance clamp (`min(clip.w, maxDistance)`). Suppresses overly thick outlines at wide FOV and long distances
- **Connect MToon auxiliary texture `texCoord` / `KHR_texture_transform` to shader** — Added UV parameters (texCoord, offset, scale, rotation) for 5 auxiliary textures to `MaterialUniform` (144→304 bytes). Added `uv1` (TEXCOORD_1) to `Vertex`. Added `resolve_mtoon_uv()` / `apply_texture_transform()` / `apply_uv_anim_core()` WGSL helper functions for per-texture texCoord selection + KHR_texture_transform application. UV Animation targets (shade / shift / rim / outline_width) and non-targets (uv_mask / matcap) are distinguished per UniVRM
- **`baseColorTexture` `texCoord` / `KHR_texture_transform` support** — Added `base_color_tex_info: Option<IrTextureInfo>` to `IrMaterial` to hold `texCoord` / `KHR_texture_transform` (offset / scale / rotation) for the base color texture. Added `base_uv_a` / `base_uv_b` to `MaterialUniform` (304→336 bytes), applying `resolve_mtoon_uv()` to base color texture sampling in the fragment shader. Unified with the same UV pipeline used for auxiliary textures
- **Outline vertex shader UV1 support** — Changed `apply_uv_animation()` to `apply_uv_animation_pair(uv0, uv1)`, returning UV0/UV1 pair as `vec4`. Fixes `uv1_in` being ignored in the outline vertex shader, enabling `texCoord=1` to work correctly for `outlineWidthMultiplyTexture` and `uvAnimationMaskTexture`
- **Camera distance sorting for BLEND materials** — Added `center` (centroid position) to `DrawCall`. BLEND materials within the same `renderQueueOffsetNumber` are now sorted back-to-front by camera distance (`distance_squared`). Improves depth ordering for overlapping transparent meshes
- **Interleaved BLEND/BlendZWrite outline draw order** — Transparent phases (BLEND / BlendZWrite) now issue surface and outline draws consecutively per draw call. Fixes ZWrite-OFF transparent outlines floating above foreground surfaces (equivalent to UniVRM's multi-pass draw order). OPAQUE / MASK retains 2-pass structure as depth buffer protection is sufficient
- **Dynamic transparent sort distance key update** — BLEND / BlendZWrite draw centroids are now recalculated from `current_vertices()` every frame during animation playback. Fixes back-to-front sort breakdown when rest-pose fixed centroids diverge from actual animated positions. Opaque draws retain build-time fixed centroids (no recalculation needed)
- **glTF emissive (emission) support** — `emissiveFactor` + `emissiveTexture` supported as glTF standard properties across all formats (VRM / FBX / PMX / PMD). MToon shader adds emissive per UniVRM: `baseCol = lighting + emissive + rim`. Non-MToon materials also apply `lit += emissive`. Outline `compute_mtoon_surface_lighting()` includes emissive, affecting outline color via `outlineLightingMixFactor`. Added `emissive_factor` / `emissive_texture` / `normal_texture` / `normal_texture_scale` to `IrMaterial`. Normal mapping applied via screen-space derivative tangent construction
- **VRM 0.x MToon full property normalization** — Normalizes previously unimplemented VRM 0.x `materialProperties` into VRM 1.0 `IrMaterial`. Follows UniVRM `MigrationMToonMaterial.cs` / `MToon10Migrator.cs` conversion formulas. Covers:
  - Render mode: `_BlendMode` → `AlphaMode`, `_Cutoff` → `alpha_cutoff`, `_CullMode` → `is_double_sided`
  - Textures: `_ShadeTexture` (falls back to `_MainTex` when absent: UniVRM destructive migration), `_RimTexture`, `_EmissionMap`, `_UvAnimMaskTexture`, `_SphereAdd` (→ matcapTexture), `_BumpMap` (→ normalTexture)
  - Rim: `_RimColor`, `_RimFresnelPower`, `_RimLift`, `rimLightingMixFactor` = 1.0 (UniVRM destructive migration)
  - Emission: `_EmissionColor`
  - UV animation: `_UvAnimScrollX`, `_UvAnimScrollY` (Y inverted × -1), `_UvAnimRotation` (× 2π rad/s conversion)
  - Shading: `_ShadeToony` / `_ShadeShift` → UniVRM `GetShadingRange0X` + `MigrateToShadingToony/Shift` formula
  - Outline: `_OutlineColorMode` → `outlineLightingMixFactor` (FixedColor = 0.0, MixedLighting = original value)
- **`KHR_texture_transform.texCoord` override support** — `read_texture_info()` now prioritizes `extensions.KHR_texture_transform.texCoord` over the TextureInfo-level `texCoord` when present. glTF spec compliant
- **VRM 0.x `renderQueue` → `render_queue_offset` migration** — Implements UniVRM `MigrationMToonMaterial.cs`-compliant rank compression. Collects transparent material source offsets (`renderQueue - DefaultValue`) into a `BTreeSet`, assigns sequential ranks (Blend: descending 0, -1, -2, ...; BlendWithZWrite: ascending 0, 1, 2, ...) to compress into VRM 1.0 spec range (Blend: -9..0, BlendWithZWrite: 0..+9) while preserving relative order. Simple clamping would collapse values to the same offset, losing relative ordering. Out-of-range `renderQueue` values return offset=0
- **VRM 0.x `_MainTex` ST (Scale/Translation) propagation to MToon textures** — Applies VRM 0.x `vectorProperties._MainTex` (`[offsetX, offsetY, scaleX, scaleY]` order) to MToon textures' `IrTextureInfo.offset` / `.scale` (per UniVRM `Vrm0XMToonValue.cs`). Also applies to `baseColorTexture`. MatCap (`_SphereAdd`) is excluded from ST propagation (per UniVRM `MigrationMToonMaterial.cs:255-260`: "Texture transform is not required"). Identity transforms (scale=1, offset=0) are skipped
- **Normalize VRM 0.x `ScreenCoordinates` outline width to UniVRM-compliant** — Fixed `outline_width_factor` to `w * 0.01 * 0.5` (old: percent of half-height → new: ratio of full height, 1/200 conversion). VRM 0.x ScreenCoordinates outlines now match Unity
- **VRM 0.x color property sRGB→Linear conversion** — VRM 0.x MToon `_ShadeColor`, `_RimColor`, and `_OutlineColor` are now converted from sRGB to linear color space during extraction. Equivalent to UniVRM `MigrationMToonMaterial.cs` `.ToFloat3(ColorSpace.sRGB, ColorSpace.Linear)`. `_EmissionColor` is excluded as it is Linear→Linear per UniVRM
- **MASK material alpha_to_coverage** — Added dedicated pipelines (`pipeline_mask_cull` / `pipeline_mask_no_cull`) for `RenderQueue::Mask` materials with `alpha_to_coverage_enabled = true` when MSAA is active (sample_count > 1). Equivalent to UniVRM `MToonValidator.cs` `UnityAlphaToMask = On`. Reduces MSAA jaggies on cutout materials like eyelashes and hair cards
- **`giEqualizationFactor` GI implementation (UniVRM-compliant)** — Implements VRM spec-compliant `lerp(passthroughGi, uniformedGi, giEqualizationFactor)`. Without SH/IBL, `passthroughGi` = `uniformedGi` = ambient, ensuring direct light is not mixed into GI (same separation structure as UniVRM's `indirectLight` / `indirectLightEqualized`). Supports both VRM 1.0 `giEqualizationFactor` and VRM 0.x `_IndirectLightIntensity` (converted via `1.0 - value`)
- **Outline pipeline depth bias** — Added UniVRM `Offset 1, 1` equivalent `DepthBiasState` (`constant: 1, slope_scale: 1.0`) to `pipeline_outline` / `pipeline_outline_blend`. Prevents Z-fighting (outline gaps, flickering) caused by near-identical depth values between the main surface and inverted hull outline. Particularly effective on hair, thin polygons, and surfaces parallel to the view direction
- **MASK material outline AlphaToCoverage** — Added `pipeline_outline_mask` (MASK material-specific outline pipeline) with `alpha_to_coverage_enabled = true` when MSAA is active. Cutout boundaries are now smooth on both the main pass and outline pass, ensuring consistent edge quality between surface and outline on hair cards, eyelashes, etc. Equivalent to UniVRM `AlphaToMask = On`
- **Apply UV Animation to `shadingShiftTexture` (UniVRM-compliant)** — Fixed `shadingShiftTexture` sampling to use animated UV (`anim_uv`) instead of raw UV (`in.uv`). In UniVRM, `GetMToonGeometry_Uv()` transforms the base UV once and all textures use the animated result — `shadingShiftTexture` is no exception. Shadow boundaries now correctly follow UV scroll/rotation animations. Both forward and outline passes fixed
- **Morph target normal/tangent delta tracking** — Extended `IrMorphTarget` with `normal_offsets` / `tangent_offsets` to retain glTF morph target normal and tangent deltas in sparse representation (threshold 1e-7 filter). Viewer GPU morph application (`apply_gpu_morph_recursive`) now adds weight × delta to normals and tangents alongside positions. MToon shading boundaries, outline extrusion direction, and normal maps now follow the deformed surface direction during expression morphing. Normal and tangent deltas are correctly propagated through A-stance conversion, vertex splitting, and export filtering
- **NORMAL/TANGENT-only morph end-to-end support** — Fixed morph targets with only NORMAL/TANGENT deltas (no POSITION) being dropped at all stages: extraction → export filter → GPU application. (1) `extract.rs`: Extended `IrMorph` generation condition from `positions` only to `positions || normals || tangents` OR. (2) `export_filter.rs`: Changed morph liveness check to union of all 3 attribute sets. (3) `mesh.rs`: GPU morph affected vertices now collected via `BTreeSet` union of positions/normals/tangents, with per-attribute `HashMap` lookup (POSITION-less morph targets are legal per glTF 2.0 spec)
- **Morph-only CPU vertex cache sync** — `apply_morphs()` only updated the GPU vertex buffer without updating `animated_vertices` (CPU-side cache), causing `current_vertices()` to return rest-pose vertices on morph-only frames. This resulted in MToon transparent (Blend / BlendZWrite) distance sorting using rest-pose centroids instead of morphed positions. Fixed by syncing `morph_work` to `animated_vertices` at the end of `apply_morphs()`, keeping CPU and GPU vertex data consistent

- **MikkTSpace tangent generation for normal maps (UniVRM-compliant)** — Replaced screen-space derivative (`dpdxCoarse`/`dpdyCoarse`) approximate TBN with MikkTSpace algorithm (`mikktspace` crate) for vertex tangent generation. When glTF provides `TANGENT` attributes, they are skinning-transformed and used directly; otherwise (VRM spec: TANGENT is not exported), MikkTSpace tangents are auto-generated. Added `tangent: Vec4` (xyz=direction, w=handedness) to `IrVertex` and `tangent: [f32; 4]` to GPU vertex. Shader now uses UniVRM `MToon_GetTangentToWorld()`-compliant TBN construction with binarized `tangent.w` (NaN prevention). Fixes normal map breakage on mirrored UVs and tangent seams
- **`CullMode` enum (VRM 0.x Front cull support)** — Replaced `is_double_sided: bool` with `CullMode` enum (`Back` / `None` / `Front`). VRM 0.x `_CullMode=1` (Front cull) is now accurately reproduced via `wgpu::Face::Front` pipeline instead of falling back to `doubleSided`. Front cull pipelines added for all render queues (Opaque / Mask / BlendZWrite / Blend). PMX export sets double-sided flag (0x01) for both `Front` and `None` (PMX has no Front cull concept)
- **`texCoord >= 2` graceful degradation** — Changed `read_texture_info()` behavior for `texCoord > 1` from disabling the texture (`None`) to falling back to `texCoord=0` with a `warn` log. Texture UV will be inaccurate but rendering is preserved. Design rationale documented in source and docs (confirmed against UniVRM implementation)

### Bug Fixes

- **Separate direct light from GI calculation (UniVRM-compliant)** — `passthrough_gi` included `light_intensity * max(dot(N, light_dir), 0)`, causing direct light to be double-counted in both the direct and GI terms. In UniVRM, `indirectLight` is SH sampling result (ambient only), with direct light processed separately. For the viewer without SH/IBL, `passthrough_gi = ambient` is the correct approximation. CPU-side `gi_equalized` computation (`CameraUniform`) also fixed to ambient-only. Resolves excessive brightness on front-facing surfaces and incorrect light factor in `giEqualizationFactor` / `rimLightingMixFactor`. Fixed in both main shader and outline shared function
- **Restore alpha=1.0 after MASK AlphaToCoverage (UniVRM-compliant)** — After `fwidth`-based A2C calculation in MASK branch, `out_alpha = a2c_alpha` left surviving pixels with semi-transparent intermediate values, causing cutout material edges to bleed during egui offscreen compositing. UniVRM `vrmc_materials_mtoon_geometry_alpha.hlsl` returns `1.0` after `clip()` to restore full opacity. A2C is used for coverage control only; final alpha is now fixed to opaque. Fixed in both main shader and outline shared function
- **Fix `tangent.w` mirror coordinate transform flip** — The viewer's coordinate transform (VRM 1.0: Z-flip, VRM 0.0: X-flip) is a mirror transform with determinant -1, causing `cross(M*N, M*T) = -M*cross(N,T)` which flips the bitangent direction. Fixed by negating `tangent.w` to preserve tangent space handedness. Resolves left-right inversion of normal map bump direction
- **MikkTSpace tangent generation `normalTexture.texCoord` support** — Added `normal_tex_coord` parameter to `generate_tangents()`, generating tangents from UV1 when `normalTexture.texCoord=1`. VRM materials pass the `texCoord` from `normalTexture`, while FBX/PMX/PMD use texCoord=0. Fixes tangent/normal map UV set mismatch on models where the normal map references UV1
- **Fix glTF sampler default `min_filter` to `LinearMipmapLinear`** — Changed `IrSamplerInfo::default()` `min_filter` from `Linear` (no mipmap) to `LinearMipmapLinear`. Aligns with UniVRM's `SamplerParam.Default` (Bilinear + EnableMipMap=true) and `TextureSamplerUtil`'s `glFilter.NONE` → mipmap-enabled default behavior. Reduces flickering on textures without explicit sampler at oblique/distant views
- **Fix MToon ScreenCoordinates outline aspect correction** — Changed `projected.x *= camera.aspect` (`width/height`) to `projected.x /= camera.aspect`. UniVRM multiplies by `height/width`; the previous implementation caused X-direction outline bloat on wide windows
- **Remove double gamma correction from MToon sRGB outline** — Removed `pow(2.2)` from sRGB `fs_outline`. MToon computes in linear space, so the sRGB render target's automatic conversion should be trusted. `pow(2.2)` is only needed for MMD (gamma-space computation) shaders. Fixes outlines appearing darker than the surface
- **Fix UV1 absent fallback value** — Changed GPU-side (`viewer/mesh.rs`) UV1 absent fallback from UV0 copy to zero (`[0.0, 0.0]`). Unified with CPU-side (`resolve_cpu_uv`) behavior, matching UniVRM `MeshData.cs` zero fallback convention
- **Skinning/normal recalculation TBN sync** — Fixed tangent not being updated during animation skinning (only normals were transformed). Tangent.xyz is now transformed by the skinning matrix followed by Gram-Schmidt re-orthogonalization to maintain orthogonality with the normal. Same re-orthogonalization applied after `smooth_normals` / `clear_custom_normals`. Fixes incorrect shading, rim, and highlight direction on normal-mapped materials during animation or after normal recalculation
- **Fix VRM 0.x `_MainTex` overwritten by raw JSON `baseColorTexture`** — After setting `materialProperties._MainTex` as the authoritative source for VRM 0.x MToon, the glTF core `pbrMetallicRoughness.baseColorTexture` was unconditionally reapplied, overwriting the `_MainTex` setting. Introduced `v0_main_tex_resolved` flag to skip raw JSON application when VRM 0.x MToon `_MainTex` is already resolved
- **Add warning for smooth normals + normal map combination** — Emit `warn` log when `smooth_normals` is enabled and normal-mapped materials are present. Normal smoothing welds vertices by `PosUvKey` (position + UV) only, which can cause inaccurate tangent basis at UV seam boundaries (MikkTSpace regeneration would be ideal but is too costly for real-time toggling)
- **Fix shade color composition to match VRM spec and UniVRM** — Changed `shade = base_color.rgb * shade_color * shade_mul` to `shade = shade_color * shade_mul`. Per VRM 1.0 spec pseudocode, `shadeColorTerm = shadeColorFactor * texture(shadeMultiplyTexture)` — `baseColorFactor * baseColorTexture` applies only to the lit side. Previously, shade color was double-dependent on `baseColor`, making shadows excessively dark. Fixed in both main and outline shaders
- **Fix orthographic view direction to match UniVRM** — Changed view direction in orthographic projection from `normalize(camera_pos - world_pos)` to `normalize(camera_forward)`. Added `is_perspective` and `camera_forward` to `CameraUniform`. Perspective projection unchanged. Fixed in MToon rim lighting, MatCap, and MMD specular. Per UniVRM `MToon_GetWorldSpaceNormalizedViewDir()`
- **Build-layer forced disable of normal smoothing / custom normal clear** — `build_gpu_model` / `build_gpu_model_from_ir` now check for normal-mapped materials at entry and force `smooth_normals` / `clear_custom_normals` to `false`. Provides defense-in-depth alongside the UI-level disable, ensuring the invariant holds for non-UI call paths (CLI, tests, benchmarks)
- **Disable normal smoothing UI for normal-mapped materials** — Gray out normal smoothing checkbox when materials with `normal_texture` are present, preventing tangent basis corruption at UV seam boundaries. Hover text shows the reason
- **Fix MatCap UV basis X-axis inversion (UniVRM-compliant)** — Fixed MatCap UV calculation where `world_view_x` had opposite sign from UniVRM (`(v.z, 0, -v.x)` → `(-v.z, 0, v.x)`) and `world_view_y` cross product order was inconsistent. Unified to `right = cross(viewDir, worldUp)`, `up = cross(right, viewDir)` (per UniVRM `vrmc_materials_mtoon_lighting_mtoon.hlsl`). Fixes left-right mirroring of asymmetric MatCap textures. Both main and outline shaders fixed
- **Disable custom normal clear UI for normal-mapped materials** — Gray out custom normal clear checkbox (in addition to normal smoothing) when materials with `normal_texture` are present. `recalculate_normals_from_geometry` followed by Gram-Schmidt re-orthogonalization has the same UV seam tangent basis inaccuracy as `smooth_normals`
- **Add Gram-Schmidt re-orthogonalization to initial glTF tangent load** — Added `t_ortho = (t - n * dot(n, t)).normalize()` re-orthogonalization after tangent transformation in both skinned and non-skinned mesh paths in `extract.rs`. Already implemented in `animation.rs` skinning update path but was missing from the initial load path. Fixes normal/tangent orthogonality loss with non-uniform scale skin matrices
- **Unify `texCoord=1` fallback when TEXCOORD_1 is absent** — Added a post-extraction step that checks all meshes for UV1 presence and normalizes `tex_coord=1` to `tex_coord=0` on all material textures when no mesh has UV1. Eliminates the root cause of UV set divergence between tangent generation (UV0 fallback) and rendering (zero fallback)
- **Per-mesh `texCoord=1` fallback granularity** — Changed UV1 fallback check from model-wide (`any_mesh_has_uv1`) to per-mesh granularity. Only materials referenced by meshes without UV1 have their `texCoord=1` normalized to `texCoord=0`. Correctly handles models where only some meshes have UV1. Also added `base_color_tex_info` to the fallback target list
- **Preserve per-texture sampler on texture replacement** — Texture replacement via UI now uses the material's `IrSamplerInfo` to recreate the sampler instead of falling back to `default_sampler` (Linear + Repeat). `ClampToEdge` / `MirroredRepeat` / `Nearest` and other per-texture sampler settings are now preserved after replacement. Fixed in both same-name material linking and package texture assignment paths
- **Sync `source_texture_name` on VRM 0.x `_MainTex` adoption** — When VRM 0.x MToon `_MainTex` overwrites `texture_index` / `base_color_tex_info` as the authoritative source, `source_texture_name` is now also re-read from the same texture source. Fixes UnityPackage automatic texture matching (`embed_textures_into_ir`) using stale glTF core texture names instead of the `_MainTex` source
- **Fix VRM 0.x `outlineWidthTexture` channel reference** — VRM 0.x `_OutlineWidthTexture` references the R channel (per UniVRM `MToonCore.cginc:86`), but was being read as G channel (VRM 1.0). Added `ColorChannel` enum to `IrMaterial` and dynamic channel selection (VRM 0.x=R, VRM 1.0=G) in both CPU (`sample_image_channel`) and GPU (WGSL `select_channel`)
- **Fix VRM 0.x `uvAnimationMaskTexture` channel reference** — VRM 0.x `_UvAnimMaskTexture` references the R channel (per UniVRM `MToonCore.cginc:129`), but was being read as B channel (VRM 1.0). Same `ColorChannel` version-based channel selection applied
- **Remove shared material `texCoord=1` rewrite** — Removed the post-extraction step that rewrites `texCoord=1` to `texCoord=0` on materials referenced by meshes without UV1. This was breaking UV1-bearing meshes sharing the same material. Tangent generation fallback changed from UV0 to zero UV to match the rendering side (`mesh.rs`)
- **Fix MToon `dot(N,L)` light direction sign** — `camera.light_dir` (light travel direction: light→surface) was used directly in MToon / non-MToon `dot(N,L)` calculations. Changed to `dot(n, -camera.light_dir)` to match the spec's "surface→light" convention. MMD shader already correctly used `-camera.light_dir`. Fixes toon shading lit/shade boundary and Half-Lambert lighting direction, resolving front-facing surfaces appearing in shadow. Fixed in main, outline, and non-MToon shaders
- **Apply `KHR_texture_transform` to `matcapTexture`** — `matcapTexture` had `texCoord` / `offset` / `scale` / `rotation` extracted via `read_texture_info()` but the shader used raw matcap UV without transform. Added `matcap_uv_a` / `matcap_uv_b` to `MaterialUniform` and applied `apply_texture_transform()`. Fixed in both main and outline shaders
- **`KHR_materials_emissive_strength` support** — glTF `emissiveFactor` is limited to [0,1] range; HDR emissive uses `KHR_materials_emissive_strength` extension's `emissiveStrength` multiplier. UniVRM exports this extension when `maxComponent > 1.0`, but the reader did not support it. Added `emissiveStrength` reading in `extract.rs` and multiply into `emissive_factor`
- **Light color support** — Added `light_color: vec3<f32>` to `CameraUniform`, computing direct light as `light_intensity * light_color`. Added color picker to UI. Enables warm/cool lighting expressions
- **Hemisphere ambient (Sky/Ground 2-color interpolation)** — Replaced uniform gray ambient with Sky/Ground 2-color interpolation using the normal Y component (`mix(ground, sky, normal.y * 0.5 + 0.5)`). Approximates SH9 L1 component (vertical brightness gradient), closely matching VRoidHub / UniVRM's `SampleSH(normal)`. `gi_equalized` updated to `(sky + ground) / 2` (per UniVRM `(SH(up) + SH(down)) / 2`). Added Sky/Ground color pickers to UI
- **Default light mode changed to Fixed** — Changed from `LightMode::CameraFollow` to `LightMode::Fixed`. Matches VRoidHub's fixed directional light environment by default
- **Fix MToon main pass GI hemisphere interpolation using vertex normal instead of final normal** — GI hemisphere interpolation used `in.normal.y` (vertex normal) instead of the final normal `n.y` (after normal map application), so normal map bumps and `doubleSided` back-face flipping were not reflected in indirect lighting. The outline pass already used `n.y`, causing a shading mismatch between main and outline passes. Unified main pass to use `n.y`, matching UniVRM's `MToon_SampleSH(normalWS)`
- **Fix `rimLightingMixFactor` using equalized GI instead of raw indirect (UniVRM-compliant)** — The rim lighting factor `light_factor` included `gi` after `giEqualizationFactor` application, causing rim light intensity to flatten on materials with high GI equalization. UniVRM uses `unityLight.indirectLight` (raw, non-equalized indirect) for `rimLightingMixFactor`. Separated `raw_indirect` from `gi` (equalized), and changed rim to use `rim_light_factor = direct_light + raw_indirect`. Fixed in both main and outline passes
- **Fix `base_color_tex_info.index` not synced on texture replacement** — `assign_texture_to_material` / `assign_texture_data_to_material` updated only `texture_index` without syncing `base_color_tex_info.index`, causing GPU rendering to be correct but IR-based downstream processing (export filter, reload) to retain stale texture references. Fixed all 4 code paths including same-name material linking. Creates `IrTextureInfo::from_index()` when `base_color_tex_info` is `None`
- **MikkTSpace tangent handedness (w) mismatch vertex splitting** — Changed `set_tangent_encoded()` output to be stored per-corner (`face * 3 + vert`) instead of accumulated per-vertex. When corners sharing the same vertex have differing `tangent.w` (handedness ±1), minority corners are automatically split into new vertices with indices / morph targets / UV1 updated accordingly. Fixes normal map bump twisting at mirrored UV boundaries. Seed-san.vrm splits 202 vertices (hair: 70, head: 88, wear: 44)
- **Degenerate tangent detection after Gram-Schmidt re-orthogonalization** — In both skinned and non-skinned mesh paths in `extract.rs`, when `t_ortho` length falls below threshold or is non-finite after Gram-Schmidt, fall back to `Vec4::ZERO` to route through MikkTSpace regeneration. Fixes degenerate tangent `[0,0,0,w]` being incorrectly treated as valid when tangent is nearly parallel to normal (non-uniform scale or bad tangent data)
- **Change tangent validity check to `length_squared`-based** — Changed `generate_tangents()` validity check from `v.tangent == Vec4::ZERO` (exact match) to `v.tangent.truncate().length_squared() < 1e-8` (xyz length-based). Degenerate tangents with non-zero w component (e.g., `[0,0,0,1]`) are now correctly identified for regeneration
- **Shader zero tangent guard** — Added `dot(tangent.xyz, tangent.xyz) < 1e-6` check at the start of `apply_normal_map()`, returning base normal when tangent is degenerate. Defense-in-depth against WGSL undefined behavior from `normalize(vec3(0))`. Applied to both main and outline shaders
- **Fix GI indirect light to multiply `litColor` per VRM spec** — GI (indirect lighting) term used `base_color.rgb * gi` instead of the toon-interpolated lit color, causing toon boundaries to break under indirect lighting. VRM 1.0 spec defines `giLighting = gi(n) * litColor`, and UniVRM uses `input.litColor * lerp(indirectLight, indirectLightEqualized, _GiEqualization)`. Changed to `lit * gi` (main) / `toon_color * gi` (outline), preserving toon shade boundaries under shadow, backlight, and low-light conditions

### Implementation Details

- **Normal mapping (normal map) support** — Apply glTF `normalTexture` to shading. Builds TBN matrix from vertex tangent (`tangent: Vec4`) per UniVRM `MToon_GetTangentToWorld()`. When glTF lacks `TANGENT` attributes, generates MikkTSpace tangents via `mikktspace` crate (VRM spec-compliant). Applied to both MToon and non-MToon materials. Supports `normalTexture.scale` intensity control, `texCoord` / `KHR_texture_transform` / UV Animation. Materials without normal maps automatically bind a flat normal texture (RGB=(0.5, 0.5, 1.0))
- **`alphaMode` shader processing** — `alpha_cutoff` field encodes alphaMode using sentinel values (`-1.0`=OPAQUE, `-0.5`=BLEND, `>=0.0`=MASK cutoff). OPAQUE output alpha fixed to 1.0, MASK uses UniVRM-compliant `fwidth`-based AlphaToCoverage calculation for smooth cutoff edge transition (pipeline uses blend: None), BLEND discards fully transparent pixels. Same alpha handling applied to outline pass
- **`outlineLightingMixFactor` full UniVRM compliance** — Full MToon lighting computation shared via `compute_mtoon_surface_lighting()` function. Outline color composited using UniVRM's exact formula: `outlineColor * lerp(1, baseCol, mix)`
- **glTF texture index to image index normalization** — `read_texture_info()` resolves glTF texture index to image index via `document.textures().nth(i).source().index()`. Correctly references images when `textures[]` and `images[]` ordering diverges
- **`outlineWidthMultiplyTexture` GPU-only sampling** — Outline vertex shader uses GPU sampling result only (`edge_scale` retained for PMX export). CPU-side `resolve_cpu_uv()` applies the same texCoord selection + KHR_texture_transform as GPU
- **`doubleSided` back-face normal flipping (UniVRM-compliant)** — `@builtin(front_facing)` flips back-face normals before normal map application, matching UniVRM's `MTOON_IS_FRONT_VFACE`. Applied to all shader variants
- **UV animation rotation precision** — Wraps angle within one period via `fract(turns) * 2π` per UniVRM, preventing float precision loss during long sessions
- **Limitation: Only `TEXCOORD_0` / `TEXCOORD_1` UV sets are supported** — While the glTF spec allows arbitrary UV sets, VRM/MToon only uses UV0/UV1 (confirmed against UniVRM implementation). Textures with `texCoord >= 2` fall back to `texCoord=0` (`warn` log emitted)

### Code Quality & Performance

- **Reuse work buffers for translucent sorting** — Fixed per-frame allocation of `Vec<Vec3>` (centroids) and `Vec<usize>` (sorted indices) inside `render_to_texture`. Added `work_draw_centers` / `work_sorted_indices` work buffers to `GpuRenderer`, using a `std::mem::take` + return pattern to retain capacity while avoiding borrow conflicts
- **Uniform sampling for translucent DrawCall centroids** — Changed centroid computation for translucent sorting from full index traversal to uniform interval sampling (max 30 points). Meshes with 30 or fewer indices use full traversal; larger meshes sample at `total / 30` step intervals. Produces spatially representative centroids even for spread-out meshes (hair, skirts, etc.) while keeping computation O(k)
- **Reuse morph cycle-detection buffer** — Fixed `apply_gpu_morph_to` allocating `vec![false; N]` per morph. Added `morph_visited: Vec<bool>` to `GpuModel`, reused via `clear()` + `resize()`. Removed the `apply_gpu_morph_to` wrapper; callers now invoke `apply_gpu_morph_recursive` directly
- **Swap-based `morph_work` / `animated_vertices` integration** — Replaced `extend_from_slice` / `clone` (~1.9 MB/frame) from `morph_work` to `animated_vertices` in `apply_morphs` with `std::mem::swap`. GPU writes now reference the swapped `animated_vertices`, eliminating redundant vertex buffer copies
- **Eliminate clone in texture export** — Changed `convert/texture.rs` from `ImageBuffer::from_raw(w, h, tex.data.clone())` to `image::save_buffer(&out_path, &tex.data, ...)`, completely avoiding data clones up to 64 MB (4K RGBA). Removed `ImageBuffer` import
- **Fix `convert_fbx_to_pmx` `normalize_pose` passthrough** — Fixed public API `convert_fbx_to_pmx` not passing `options.normalize_pose` to `extract_ir_model_from_fbx`. Switched to `extract_ir_model_from_fbx_with_options`
- **Add SAFETY comments to `unsafe` blocks** — Added `// SAFETY:` comments to all `unsafe` blocks in `main.rs` (`attach_parent_console` / `detach_console`) and `viewer/single_instance.rs` (all Win32 API calls)
- **Extract MToon fields from `IrMaterial`** — Moved 25 MToon-specific fields from `IrMaterial` into a new `MtoonParams` struct, held as `mtoon: Option<MtoonParams>`. Reduced field count from 35+ to ~18. Added `is_mtoon()` / `mtoon()` / `mtoon_mut()` helper methods (returns static default `MTOON_DEFAULT` for non-MToon materials)
- **Split `viewer/app.rs` into submodules** — Split `app.rs` into 5 responsibility-based submodules: `mod.rs` (struct definitions, initialization, eframe::App impl), `file_io.rs` (file loading, D&D, reload), `texture_mgmt.rs` (texture assignment & preview), `pending.rs` (deferred task processing), `helpers.rs` (utility types & functions). External API preserved via `pub use`
- **Unify `anyhow` → `PoponeError`** — Migrated 19 internal library files from `anyhow::Result` to `crate::error::Result`. Added 7 new variants to `PoponeError` (`FbxParse` / `PmxParse` / `PmdParse` / `Build` / `Archive` / `UnityPackage` / `Other`). Added `ResultExt` trait (`.context()` / `.with_context()` compatible). `main.rs` / `viewer/` retain `anyhow`
- **Prevent `render_queue_offset` mis-assignment on non-MToon materials** — VRM 0.x `remap_vrm0_render_queue_offsets` called `mat.mtoon_mut()` on all materials, causing non-MToon materials to acquire `mtoon: Some(Default)` and report `is_mtoon() == true`. Fixed by using `if let Some(ref mut mtoon) = mat.mtoon` to restrict to MToon materials only

## v0.2.8

### New Features

- **Single Instance** — When the viewer is already running and launched again, the file path is forwarded to the existing window and the new process exits automatically (Windows Named Mutex + Named Pipe IPC). Restores from minimized state
- **FPS Accuracy** — Changed from exponential moving average (EMA) to frame counting method (actual frame count over the last 1 second). Also displays average frame time (ms)

### Improvements

- **Log Preservation** — Single instance check runs before log initialization, preventing unnecessary log file creation and rotation by second processes. Log rotation is also skipped on fallback startup when IPC fails
- **IPC Error Handling** — WriteFile failure or short writes now trigger FallbackStart (prevents silent loss of file-open requests). ReadFile errors (ERROR_MORE_DATA, etc.) are distinguished from intentional empty messages

## v0.2.7

### New Features

- **PMX Export Options** — Added the following options to the viewer export tab and CLI. Introduced `PmxBuildOptions` struct to unify build-time options
  - **No-physics export** (`--no-physics`): Exclude rigid bodies and joints from output. In the viewer, physics visualization is preserved while only skipping at export time
  - **Raw structure export** (`--raw-structure`): Skip MMD standard bone insertion (master, center, groove, waist, IK, twist, etc.) and output original VRM/FBX bone names as-is in PMX. Added `original_name` field to `IrBone` to preserve FBX original node names (before humanoid detection renames to PMX names)
- **App Icon** — Display icon in both the window title bar and the exe file
- **Grid Y-Axis Line** — Added green Y-axis (up direction) guide line to the grid floor

### Bug Fixes

- **PMD Vertex edge_flag Fix** — Fixed PMD vertex `edge_flag` interpretation
- **PMX Group Morph Index Remapping Fix** — Fixed incorrect deformation when PMX group morphs referenced sub-morphs whose indices shifted due to bone/material/UV morphs being skipped during PMX → IrModel conversion. Now builds an index remapping table and correctly excludes skipped morphs from group morph references
- **Viewer Stack Overflow Fix** — Fixed stack overflow on Windows where the default 1MB stack was insufficient for the deep eframe/winit/wgpu callback chain. Stack size is now set to 8MB via `/STACK:8388608` linker flag in `build.rs` when the viewer feature is enabled. Also added recursion depth limit (max 16) to group morph expansion to prevent infinite recursion from circular references

### Improvements

- **Texture Manual Assignment Search Filter Relocation** — Moved the search filter in the texture manual assignment dialog (for archives like UnityPackage / ZIP) from the top of the dialog into each texture dropdown. Opening a dropdown now shows "(none)" → search filter → texture list, matching the behavior of the material panel texture assignment popup

### Code Quality

- **Public API Consolidation** — Merged the 3-level `convert_vrm_to_pmx` wrapper chain into a single function with `VrmConvertOptions` struct. Prevents function proliferation when adding new options
- **Unified `no_physics` Application** — Removed direct `ir.physics` clearing in `main.rs`, consolidated control through `PmxBuildOptions`
- **Group Morph Cycle Detection** — Replaced depth-only guard with visited bitset (backtracking) for O(N) cycle detection in recursive group morph expansion
- **Grant Data Preservation in `raw_structure`** — When exporting with raw bone structure, PMX grant (rotation/move grant), translatable, axis-fixed, and visibility flags are now correctly restored from IrBone. Prevents data loss in PMX → PMX round-trips
- **Cross-compilation Support in build.rs** — Restricted `winres` to `[target.'cfg(windows)'.build-dependencies]`. Stack size linker flags now branch between MSVC (`/STACK`) and GNU (`-Wl,--stack`)
- **Coordinate Function Deduplication** — Consolidated `pmx_pos_to_gltf` / `pmx_normal_to_gltf` into `convert/coord.rs`, eliminating duplicate definitions in `pmd/extract.rs` and `pmx/extract.rs`
- **Icon PNG Optimization** — Reduced window icon PNG from 512×512 (99KB) to 64×64 (4KB)
- **Error Handling Improvement** — Changed icon loading from `expect` (panic) to `?` operator (error propagation)
- **Group Morph Warning Logs** — Report skipped sub-morphs during PMX loading and out-of-range sub-indices in viewer via `log::warn`
- **Convergence Loop Safety** — Added morph count upper bound guard to the group morph liveness loop in export filter

## v0.2.6

### Bug Fixes

- **Rigid Body / Joint Euler Rotation Order Fix** — Changed Euler decomposition/reconstruction for rigid bodies and joints from `ZXY` (intrinsic ZXY = extrinsic YXZ) to `YXZ` (intrinsic YXZ = extrinsic ZXY). Now conforms to D3DX row-major convention `v * Ry * Rx * Rz` (in glam column-major: `Rz * Rx * Ry`). The mismatch was inconspicuous for spheres/capsules but clearly visible for box rigid bodies. Both conversion output (`convert/physics.rs`) and viewer rendering (`gpu.rs`) are fixed
- **PMD/PMX Rigid Body bone_index Fallback** — Rigid bodies with PMD `bone_index=0xFFFF` (no associated bone) and PMX `bone_index=-1` now fall back to bone 0 (center). Previously these were `None`, leaving no base point for position calculation
- **Joint Connection Line Display Separation** — Removed joint connection lines (yellow lines) from `generate_spring_bone_vertices` (physics display (P) toggle). Joint lines are already independently drawn by `generate_joint_vertices` and controlled by the joint display toggle
- **MMD Draw Order Fix** — Merged separate opaque/transparent draw loops into a single material-index-order loop. Now correctly preserves PMX/PMD material order (the front-to-back order intended by the model author). Edges are drawn immediately after each opaque material
- **MMD Transparent Depth Write Enabled** — Enabled depth write for MMD transparent pipelines (MMD-compliant). Combined with material-order drawing, materials with alpha=0.99 (effectively opaque) now correctly occlude subsequent materials
- **PMD Custom Toon Texture Fix** — Fixed `build_tex_map()` not registering custom toon texture indices. Now builds the mapping from `extract_textures()` results, ensuring model-bundled toon textures are correctly referenced (eliminates incorrect fallback to shared toon)
- **PMX/PMD Rigid Body Animation Tracking Fix** — Fixed rigid bodies and joints not correctly following bones during VRMA animation playback on PMX/PMD models. Root cause was coordinate space mismatch between `bone.position` (converted to glTF space) and `rb.position` (kept in PMX space). Since PMX/PMD's `pmx_pos_to_gltf` uses the same Z-flip as VRM 1.0, the rigid body tracking delta computation now applies the same `gltf_pos_to_pmx` conversion and rotation delta Z-flip as VRM 1.0
- **FBX Humanoid Bone Detection Improvement** — Fixed Blender rig CamelCase bone names (e.g., `UpperLeg.L` → `upperleg_l`) failing to match `upper_leg_l` patterns. Added underscore-free alternative patterns (`upperleg_l` / `lowerleg_l` / `upperarm_l` / `lowerarm_l`), singular toe (`toe_l` / `toe_r`), reversed finger patterns (`index_proximal_l`, etc.), and pinky aliases. Also strips Unity FBX export namespace prefixes (`Model::Hips`, etc.) via `strip_namespace_lower()` for rig detection and pattern matching
- **UnityPackage Texture MIME Type Fix** — Fixed all textures appearing as magenta (1x1 pink) when loading FBX models from UnityPackage files. `embed_textures_into_ir` was creating IrTexture with an empty `mime_type`, causing `image::load_from_memory` auto-detection to fail for formats without magic numbers (e.g., TGA). Now derives MIME type from the file extension. Also added `"image/x-tga"` to the TGA MIME match in `decode_image_to_rgba_with_hint` to fix mismatch with the value returned by `mime_for_ext`

### New Features

- **PMX Grant (付与) Animation Support** — Rotation/move grants on PMX bones are now processed during animation playback. D-bones (leg D, knee D, etc.) in models like Tda Miku copy FK bone rotations via grants, but this mechanism was unimplemented, causing legs to not follow VRMA animations. Added `IrGrant` (parent index, ratio, rotation/move/local flags) to `IrBone` and extracts grant data during PMX loading. Implemented as a 2-phase post-process after animation computation: apply grant deltas in topological order, then recompute global matrices linearly. Local grants (`is_local`) apply deltas relative to the child bone's rest pose. Grant processing order is pre-computed via topological sort (Kahn's BFS algorithm), ensuring correct dependency order even for malformed PMX files
- **Bone Display Improvements** — Draws PMX/PMD bones with flag-based shapes. Normal = ◎ (double circle + filled center), Move = ◻ (square + filled center), Axis-fixed = ⊗ (circle + ✕), IK Controller = ◻ (blue outline + orange fill + blue center). IK-affected bones (Link) displayed in orange. Tail-based drawing (self→tail) shows bone direction like PMXEditor. Full solid fills via TriangleList, 3-stage pipeline (tail → fill → outline), 4-pass priority rendering (normal → IK-affected → axis-fixed → IK controller)

- **FBX T-Stance Conversion** — Added A-to-T stance conversion for FBX models. In the viewer, a "T-Stance Conversion" checkbox appears when an FBX model is loaded (mutually exclusive with A-stance conversion). Available via `--normalize-to-tstance` CLI option
- **MMD Rendering Mode** — Auto-enabled on PMX/PMD load. Displays with MMD-specific toon shading, Blinn-Phong specular, and sphere maps (multiply/add)
- **Edge (Outline) Drawing** — Inverted hull method outlines. Per-material edge color/size, distance attenuation, UI toggle and thickness slider (0.1–3.0)
- **Shared Toon Textures** — CPU-generated MMD standard toon01–toon10 gradients. Individual toon textures also supported
- **Sphere Maps** — PMX sphere_mode (multiply/add), PMD .sph/.spa file support. Sphere UV computed from view-space normals
- **Color Space Reproduction** — Reproduces MMD's gamma-space rendering. PMX/PMD-only frames switch to `Rgba8Unorm` render target for correct gamma-space alpha blending. Falls back to `Rgba8UnormSrgb` when VRM is mixed
- **PMD Sphere/Toon Extraction** — `parse_pmd_texture_slots` separates main/sphere textures via `*` delimiter. Toon texture registration with file existence check

### Improvements

- **Rigid Body Display Fix** — Removed unnecessary X flip correction (`adjust_pmd_rigid_rotation` / `adjust_pmx_rigid_rotation`) from PMD/PMX rigid body rotation. PMX/PMD model coordinates are already in PMX space, so glTF→PMX coordinate conversion is now skipped during viewer rendering. Fixed Box rigid body size to correctly treat values as half-extents (removed erroneous `* 0.5` double-halving). Added hemisphere wireframes (4 meridians + 3 parallels × top/bottom) to capsule rigid bodies for PMXEditor-compliant display
- **Rigid Body physics_mode Coloring** — PMX/PMD rigid bodies now colored by `physics_mode` (0: bone-follow = green, 1: physics = red, 2: physics+bone = blue). VRM retains group-based coloring (collider = red, spring = green)
- **Overlay Draw Order Change** — Changed visualization overlay draw order to "Normals → Bones → Rigid Bodies → Joints" (joints are frontmost). Normals are drawn farthest back as they relate to mesh surfaces, while joints are frontmost for better visibility of connection relationships
- **MMD Lighting Overhaul** — Switched to toon multiply method (removed lit/shadow lerp). Fixed D3D ambient/emissive mapping with `base_color = saturate(diffuse × LightAmbient + ambient)`. Specular now added independently after toon (highlights preserved in shadow regions). LightAmbient = 154/255 ≈ 0.604, LightSpecular unified to same value
- **NdotL-Dependent Toon Sampling** — Changed from fixed UV `(0.5, 0.85)` to `(0, 0.5 − NdotL × 0.5)`, reproducing shade gradients based on normal-light angle
- **Real Shared Toon Texture Data** — Replaced estimated gradients (256×16) with actual MMD standard toon01-10 pixel data (1×32, 32-row RGB values). toon01-04: 2-color step, toon05: warm pink gradient, toon06: yellow + highlight band, toon07-10: all white
- **Sphere UV X Inversion** — Fixed for X-inverted coordinate system with `vn_x × -0.5 + 0.5`. Sphere map applies RGB only (alpha safety for BMP etc.)
- **PMD Edge Flag Fix** — Changed `edge_flag` interpretation from `0=enabled` to `1=edge present`
- **PMX Toon Unset Handling** — `PmxToonRef::Texture(-1)` now treated as `(None, None)`, correctly handling no-toon materials
- **MMD-Compliant Camera & Lighting** — FOV 45° → 30° (MMD standard), light direction changed to MMD-compliant (fixed: inversion of (-0.5,-1.0,0.5), camera-follow: MMD-style upper-left bias). Light intensity 0.6, ambient 0.5
- **View-Dependent Fit** — Improved bounding box fit calculation to be view-dependent. Projects bbox 8 corners onto camera axes, computing distance that fits width, height, and depth within the frustum. Supports both aspect ratio and perspective/orthographic modes
- **Shift Precision Mode** — Hold Shift for 1/3 speed precision camera control (rotation, pan, and zoom)
- **Double-Click Fit** — Double-click viewport to fit model
- **MMD Ambient Separation** — Separated MMD rendering ambient from the standard path. Controlled via `mmd_ambient_scale` in CameraUniform, so MMD mode toggle no longer affects standard material brightness
- **IrMaterial Extension** — Added `source_format`, `sphere_texture_index`, `sphere_mode`, `toon_texture_index`, `toon_shared_index` fields. Index remap on merge supported
- **Texture Dual Views** — GPU textures managed with both `Rgba8UnormSrgb` (standard) and `Rgba8Unorm` (MMD) views. Zero memory overhead
- **Wireframe Coexistence** — Wire / S+W / normal map display falls back to standard pipeline even with MMD mode ON

### Code Quality & Performance

- **Animation Inverse Matrix Cache** — Cache rest-pose bone global inverse matrices at `SkinningData` construction time. Eliminates per-frame `Mat4::inverse()` computation for 175 bones
- **WGSL Shader Consolidation** — Unified `CameraUniform` (8 duplicates) and `MmdMaterialUniform` (4 duplicates) struct definitions via `macro_rules!` + `concat!`. Shared MMD main shader body via `compute_mmd_lighting` function, localizing sRGB/Unorm differences to a single fragment shader function
- **Duplicate Code Extraction** — Extracted `build_pkg_model_list` (unitypackage model list ×3), `load_animation_file` (animation load routing ×2), `mime_for_ext` (MIME type detection ×4) into shared functions
- **`to_string_lossy()` Improvement** — Changed `.to_string_lossy().to_string()` to `.to_string_lossy().into_owned()` across 7 files (18 occurrences). Avoids unnecessary allocation for UTF-8 compatible paths
- **`is_psd_filename` Optimization** — Replaced `to_lowercase()` String allocation with `eq_ignore_ascii_case`
- **`update_mat_cache` Simplification** — Removed unnecessary double `if let` borrow pattern using NLL
- **PMX Reader Safety Hardening** — Added negative value checks to all 14 `i32 as usize` count casts via `checked_count` helper. Prevents OOM panic on corrupt files. Removed unnecessary `BufReader` wrapping around `Cursor` (both PMX and PMD)
- **`sort_bones_topological` Optimization** — Changed child bone search from O(n²) linear scan to O(n) adjacency list. Replaced post-sort `clone()` with `Option::take()` pattern, eliminating deep copy of all bones
- **PSD Output I/O Optimization** — Changed UV map PSD channel data writing from per-byte `write_all` to batch buffer writes (reduced from up to 64M calls to 4 for 4096×4096). Added `reserve` to layer data buffers
- **Texture Upload Optimization** — Eliminated `rgba.to_vec()` copy in `upload_rgba_to_gpu` when no downscaling is needed (changed to pass-by-reference). Also eliminated `img.pixels.clone()` for RGBA8 format textures by uploading directly
- **GPU Rendering Minor Improvements** — Changed joint cube vertices from `Vec<Vec3>` to `[Vec3; 8]` fixed-size array. Changed normal cache update from `to_vec()` to `clear()` + `extend_from_slice()` for heap reuse
- **PMX Writer Optimization** — Changed UTF-16LE encoding from manual byte push to `to_le_bytes()` + `extend_from_slice()`. UTF-8 paths now written directly without intermediate `Vec` copy
- **Camera Matrix Reuse** — `view_proj()` now reuses `view_matrix()` instead of calling `look_at_lh` directly
- **Dead Code Removal** — Removed empty loop (no-op for loop) in `pmx/extract.rs`
- **`build_composite` Redundant Loop Removal** — Removed unnecessary alpha-setting loop after `vec![255u8; ...]` initialization (all bytes already 255)

## v0.2.5

### Improvements

- **Automatic Texture Downscaling** — Textures exceeding the GPU's maximum texture size (typically 8192px) are automatically downscaled while preserving aspect ratio. Prevents crashes with models containing oversized textures
- **Direct Archive Loading (ZIP / 7z)** — Open ZIP / 7z archives directly via D&D or dialog, auto-detecting VRM / FBX / PMX / PMD models inside. Shows selection dialog when multiple models are found. For PMX/PMD, analyzes texture reference paths to auto-collect related files
- **CLI Archive Support** — `popone archive.zip output.pmx` for direct conversion. `--list-models` to list models, `--model-name` to select a specific model (exact → prefix → substring match, unique match only at each stage)
- **Shift_JIS Filename Support** — Correctly decodes Japanese filenames in ZIP via UTF-8 → Shift_JIS fallback
- **Zip Bomb Protection** — 2GB total extraction size limit. ZIP uses `take()` for hard limits, 7z uses chunked reading to verify actual bytes read
- **Path Traversal Defense** — Rejects archive paths containing `..` (ZipSlip attack prevention)
- **Reload Support** — Supports reload (e.g., A-stance toggle) for models loaded from archives. `ReloadableSource::Archive` preserves selected model path
- **Nested UnityPackage in Archives** — Auto-detects `.unitypackage` files inside ZIP / 7z and double-extracts to load inner VRM / FBX models. Supports reload, append, and texture restoration
- **Extraction Size Limit** — `.unitypackage` (tar.gz) extraction now enforces the same 2GB size limit. Both outer archive and inner package are protected
- **Persistent Stance Conversion Warning** — When A-stance/T-stance conversion is enabled but not applied, a persistent warning is shown at the bottom-left of the viewport. Two warning types: arm bones not found (red) / already in target pose (yellow). PMX export warning messages also branch between A/T stance labels
- **UV Map PSD Layer Grouping** — When multiple models are merged, UV map PSD output groups layers into folders by model name. Single models are also grouped. Uses PSD lsct (Section Divider Setting) for Photoshop / CLIP STUDIO Paint compatibility
- **MaterialGroup Struct** — Changed viewer material group management from `(String, usize, usize)` tuple to `MaterialGroup` struct. Separates `material_range` (material index range) and `draw_range` (DrawCall range) for proper usage in UV export and UI display

### Code Quality & Performance

- **Structured Error Type** — Defined `PoponeError` enum with `thiserror`, migrated public API to `error::Result`. Internal code continues using `anyhow` with `From<anyhow::Error>` bridge for compatibility
- **ViewerApp Struct Split** — Extracted `PendingState` (10 deferred processing fields) and `ExportState` (4 PMX export fields). Reduced field count from 43 to 27
- **Per-Frame GPU Texture Re-Registration Eliminated** — Switched viewport texture from register/free to `update_egui_texture_from_wgpu_texture`, improving frame rate
- **Status Bar format! Cache** — Pre-format model statistics string at load time, eliminating per-frame heap allocation
- **Reload clone → take** — Changed `reload_current()` to use `std::mem::take()` for `morph_weights`, `material_visibility`, etc. (avoids heap reallocation)
- **GLB Double-Read Eliminated** — Hold GLB as `(ir, glb_for_tex)` tuple during VRM conversion, eliminating re-read for texture export
- **BindGroupLayout Shared Function** — Centralized material layout definition in `gpu::create_material_bind_group_layout()`
- **Dump Code Deduplication** — Extracted `dump_ir()` function, removing duplicate code in `run_main` and `run_archive_convert`

<details>
<summary>Internal Improvement Details</summary>

#### Structured Error Type (thiserror)

Defined `PoponeError` enum in `error.rs` and migrated public API in `lib.rs` to `error::Result`.

```rust
#[derive(Debug, thiserror::Error)]
pub enum PoponeError {
    #[error("File read failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("GLB/VRM parse failed: {0}")]
    GltfParse(#[from] gltf::Error),
    #[error("FBX parse failed: {0}")]
    FbxParse(String),
    // ... PmxParse, PmdParse, Extraction, Build, Texture, Image, UnityPackage, Archive, Other
}

/// Convert anyhow::Error to PoponeError (compatibility with existing code)
impl From<anyhow::Error> for PoponeError { ... }

pub type Result<T> = std::result::Result<T, PoponeError>;
```

- Public API: `error::Result<T>` (structured via `PoponeError`)
- Internal: Continues using `anyhow::Result` (retaining `bail!`, `context()` convenience)
- Bridge: `From<anyhow::Error> for PoponeError` enables automatic `?` operator conversion

#### Further ViewerApp Struct Split

In addition to v0.2.2's `TextureState` / `AnimLibrary`, extracted `PendingState` / `ExportState`:

| Sub-struct | Field | Access | Contents |
|------------|-------|--------|----------|
| `TextureState` | `self.tex.*` | 9 fields | Texture assignment, package textures, preview, matching |
| `AnimLibrary` | `self.anim.*` | 4 fields | Animation playback state, library, Muscle scale |
| `PendingState` | `self.pending.*` | 10 fields | Deferred processing (file load, GPU rebuild, PMX conversion, etc.) |
| `ExportState` | `self.export.*` | 4 fields | PMX export (output path, log, visible-only, UV resolution) |

ViewerApp field count: 43 (v0.2.1) → 30 (v0.2.2) → 27 (v0.2.5).

#### Per-Frame GPU Texture Re-Registration Eliminated

Changed viewport offscreen texture registration from per-frame free + register to initial register + subsequent update:

```rust
// Before: free + register every frame
egui_renderer.free_texture(&old_id);
let tex_id = egui_renderer.register_native_texture(device, &view, FilterMode::Linear);

// After: register once, then update
let tex_id = if let Some(existing_id) = *cached_id {
    egui_renderer.update_egui_texture_from_wgpu_texture(device, &view, FilterMode::Linear, existing_id);
    existing_id
} else {
    let id = egui_renderer.register_native_texture(device, &view, FilterMode::Linear);
    *cached_id = Some(id);
    id
};
```

#### Status Bar format! Cache

Pre-format model statistics string at load time via `CachedStats::new()`:

```rust
pub struct CachedStats {
    pub total_vertices: usize,
    pub total_faces: usize,
    pub status_text: String,  // Pre-formatted
}

impl CachedStats {
    fn new(ir: &IrModel) -> Self {
        let status_text = format!(
            "Vertices:{} Faces:{} Materials:{} Textures:{} Bones:{} Morphs:{}",
            ...
        );
        Self { total_vertices, total_faces, status_text }
    }
}
```

Also added `tex_status_text` field to `CachedMaterialInfo` to cache the texture assignment status string.

#### Reload clone → take

Changed state saving in `reload_current()` from `clone()` to `std::mem::take()`:

| Target | Before | After |
|--------|--------|-------|
| `morph_weights` | `.clone()` | `std::mem::take()` |
| `material_visibility` | `.clone()` | `std::mem::take()` |
| `material_filter` | `.clone()` | `std::mem::take()` |
| `pmx_output_path` | `.clone()` | `std::mem::take()` |
| `tex.assignments` | `.clone()` | `std::mem::take()` |
| `tex.pkg_assignments` | `.clone()` | `std::mem::take()` |

`take()` moves ownership, avoiding Vec / HashMap heap reallocation. Since the same data is restored after successful reload, the source being empty is not a problem.

#### GLB Double-Read Eliminated

Fixed CLI conversion (`run_main`) reading GLB twice during VRM → PMX conversion:

```rust
// Before: 2 reads for extract + texture export
let ir = vrm::extract::extract_ir_model(...)?;
let glb = vrm::loader::load_glb(&input)?;  // 2nd read
convert::texture::write_all_textures(&ir.textures, &glb.images, &tex_dir)?;

// After: hold as tuple and reuse
let (mut ir, glb_for_tex) = match ext.as_str() {
    _ => {
        let glb = vrm::loader::load_glb(&input)?;
        let ir = vrm::extract::extract_ir_model(...)?;
        (ir, Some(glb))
    }
};
if let Some(ref glb) = glb_for_tex {
    convert::texture::write_all_textures(&ir.textures, &glb.images, &tex_dir)?;
}
```

#### aux_files clone → take

Changed `take_or_collect_aux()` from `preloaded.aux_files.clone()` to `take()`, avoiding HashMap bucket reallocation. An empty HashMap is placed back in `preloaded`, while `main_bytes` is preserved.

#### BindGroupLayout Shared Function

Centralized the material `BindGroupLayout` descriptor definition in `gpu::create_material_bind_group_layout()`, eliminating code duplication between `gpu.rs` and `mesh.rs`.

#### Dump Code Deduplication

Extracted `dump_ir()` function from duplicate dump output code in `run_main` and `run_archive_convert`.

</details>

## v0.2.4

### Improvements

- **Archive D&D Reload Support** — Handles files D&D'd from zip/7z that are extracted to OS temp directories. Model body + auxiliary files (textures, .txt) are snapshot-cached in memory, enabling reload even after temp files are deleted. Supports VRM/FBX/PMX/PMD
- **Archive D&D Preload Cache** — At D&D detection time, model body + adjacent texture bytes are pre-read into `PreloadedData`. The entire load chain uses the cache, ensuring reliable loading even after temp file deletion. Data is passed through `PendingFbxChoice` for FBX selection dialog paths. Supports all formats: VRM/FBX/PMX/PMD/UnityPackage
- **Archive D&D Immediate Load** — Fixed error where temp files from zip archives would be deleted during the 2-frame delay before loading. When a temp path is detected, the progress overlay is skipped and the file is loaded immediately
- **Texture D&D Cache** — When D&D'ing textures from ZIP archives, byte data, PSD detection, and temp path flag are cached at preview stage. Eliminates file re-read on confirmation, ensuring texture assignments are reliably recorded even after temp file deletion
- **UnityPackage Archive Snapshot** — When D&D'ing .unitypackage from ZIP archives, archive data is snapshot-cached as `Arc<[u8]>`. Enables reload/append from memory without depending on temp files
- **Shader-Aware PMX Materials** — Automatic toon texture selection (5 levels) based on MToon shade_color/diffuse luminance ratio. MToon materials get shade_color-based ambient and zero specular. Non-MToon materials retain existing behavior
- **A-Stance Conversion Warning** — Red text overlay warning when A-stance conversion is enabled but arm bones are not found during PMX conversion. Shows skip notification when already in A-stance
- **ConvertResult::Warning** — New message type for successful conversions with caveats (red text, distinct from Failure)
- **AStanceResult enum** — Type-safe management of A-stance conversion results (NotRequested / Applied / AlreadyAStance / NotFound). Includes merge logic for IrModel::merge()
- **Reload Texture Normalization** — Fixed PSD→PNG conversion bypass during UnityPackage reload. MIME type settings now consistent with the normal assignment path
- **IrTexture Deduplication** — Texture assignment now checks filename + data for identity, preventing duplicate IrTexture entries

## v0.2.3

### Improvements

- **Visible-Only Material Export** — Option to exclude hidden materials from PMX output (default OFF). Consistently filters materials, meshes, textures, vertex morphs, and group morphs
- **2-Pass Bone Merge** — Order-independent candidate collection + propagation loop for same-name bone unification. Fixes incorrect merge of descendants in different subtrees
- **Pkg Texture Namespace** — Prevents texture name collisions when appending multiple UnityPackages (`{pkg_name}_pkg{seq}_{texture_name}` format). Also applied to auto-matched textures
- **ASCII FBX Content Handling** — Content blocks preserved as strings, maintaining parser-layer completeness
- **61 Tests** — Added bone merge, physics remap, morph vertex offset, export filter tests

## v0.2.2

### Code Quality & Performance

- **Performance** — Eliminated per-frame vertex buffer allocation, HashMap O(1) bone lookup, GPU visualization dirty flags
- **Tests** — 10 → 51 tests. Coordinate roundtrip, bone name mapping, PMX write/read roundtrip, VRM→PMX E2E
- **Zero Clippy warnings** — `cargo clippy --all-targets --all-features -- -D warnings` fully clean
- **UX** — 4-pattern D&D overlay, 2-line operation hints, disabled UI tooltips

<details>
<summary>Internal Improvement Details</summary>

#### ViewerApp Sub-Struct Refactoring

In v0.2.2, ViewerApp's 43 fields were reduced to 30:

| Sub-struct | Field | Access | Contents |
|------------|-------|--------|----------|
| `TextureState` | `self.tex.*` | 9 fields | Texture assignment, package textures, preview, matching |
| `AnimLibrary` | `self.anim.*` | 4 fields | Animation playback state, library, Muscle scale |

Rust's partial borrowing allows simultaneous borrowing of `&mut self.tex` and `&self.anim`.

#### GPU Visualization Buffer Cache Strategy

Bone, physics, and joint visualization vertices are managed with dirty flags:

| Input | Cache Key | Regeneration Condition |
|-------|-----------|----------------------|
| Bone vertices | `camera.eye()`, `bone_opacity` | Camera movement / opacity change / animation playing |
| SpringBone vertices | `spring_bone_opacity`, `align_rigid_rotation` | Settings change / animation playing |
| Joint vertices | `joint_opacity` | Settings change / animation playing |

Common to all buffers:
- `vertex_count == 0` → forced regeneration (recovery from hidden → visible toggle)
- `cache_had_anim && !has_anim` → forced 1-frame regeneration when animation is released

#### Animation Vertex Buffer Optimization

Hot path improvement for `apply_bone_animation()`:

| Item | Before | After |
|------|--------|-------|
| Vertex buffer | `base.to_vec()` per-frame alloc | `reset_animated_to_base()` capacity reuse |
| Delta matrices | `Vec::with_capacity()` per-frame | Reuse `work_deltas` field |
| Globals computation | New `Vec` + clone | In-place update (`work_computed` flag reuse) |
| Morph application | `apply_morphs_to_buf(&self, &mut [Vertex])` | `apply_morphs_to_animated(&mut self)` borrow conflict avoidance |

#### Bone Name Lookup HashMap Conversion

O(n) linear search in `insert_standard_bones()` converted to HashMap O(1):

```rust
// Reverse lookup of bone name → index (keep first occurrence for duplicate names)
fn build_bone_map(bones: &[PmxBone]) -> HashMap<String, usize> {
    let mut map = HashMap::with_capacity(bones.len());
    for (i, b) in bones.iter().enumerate() {
        map.entry(b.name.clone()).or_insert(i);
    }
    map
}
```

Rebuild with `bone_map = build_bone_map(&model.bones)` after bone array changes (insertion/movement).

#### Test Data Path Resolution

Integration test file paths can be configured via environment variables:

| Priority | Source | Example |
|----------|--------|---------|
| 1 | Per-file environment variable | `POPONE_TEST_VRM_SEED_SAN=/path/to/Seed-san.vrm` |
| 2 | Root environment variable + relative path | `POPONE_TEST_DATA=/fixtures` → `/fixtures/vrm-spec/.../Seed-san.vrm` |
| 3 | `CARGO_MANIFEST_DIR/..` | Default for local development |

</details>

## FBX Support

- Custom binary / ASCII FBX parser (scene graph, coordinate system conversion, PreRotation, UnitScaleFactor)
- ASCII FBX: Content blocks (embedded textures) preserved as strings; external file fallback for texture recovery
- Skin weights (up to 4 bones, normalized), blend shapes, UV mapping
- Humanoid rig auto-detection (Mixamo / 3ds Max Biped / Maya HumanIK / VRoid / Unreal / Blender). CamelCase bone names and namespace prefixes (`Model::`, etc.) supported
- Zero-normal auto-repair, embedded/external texture support
