<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [Technical Details](#technical-details)
  - [Coordinate Transformation](#coordinate-transformation)
    - [PMX/PMD → IrModel Reverse Conversion](#pmxpmd-%E2%86%92-irmodel-reverse-conversion)
  - [Bone Display](#bone-display)
    - [Shape Determination (Priority Order)](#shape-determination-priority-order)
    - [IK-Affected Bones](#ik-affected-bones)
    - [Drawing Direction](#drawing-direction)
    - [Rendering Pipeline](#rendering-pipeline)
    - [IrBone Fields](#irbone-fields)
  - [MMD Standard Bone Insertion](#mmd-standard-bone-insertion)
    - [Base Bones](#base-bones)
    - [IK Bones](#ik-bones)
    - [Semi-Standard Bones](#semi-standard-bones)
    - [insert_standard_bones Step Details](#insert_standard_bones-step-details)
    - [PmxBuildOptions](#pmxbuildoptions)
  - [PMX Grant Animation](#pmx-grant-animation)
    - [D-bones Mechanism](#d-bones-mechanism)
    - [Processing Flow](#processing-flow)
    - [IrGrant Data Structure](#irgrant-data-structure)
  - [OBJ/STL Loading](#objstl-loading)
    - [OBJ Reader](#obj-reader)
    - [STL Reader](#stl-reader)
    - [Coordinate Conversion](#coordinate-conversion)
    - [IrModel Construction](#irmodel-construction)
    - [Dynamic Grid](#dynamic-grid)
  - [PMX/PMD Loading](#pmxpmd-loading)
    - [PMX Reader](#pmx-reader)
    - [PMD Reader](#pmd-reader)
    - [IrModel Conversion](#irmodel-conversion)
    - [T-Stance Conversion](#t-stance-conversion)
    - [Rigid Body Rotation](#rigid-body-rotation)
    - [Texture Loading](#texture-loading)
  - [MMD Rendering](#mmd-rendering)
    - [Architecture](#architecture)
    - [MMD Shaders](#mmd-shaders)
    - [Pipeline Configuration](#pipeline-configuration)
    - [Color Space](#color-space)
    - [Shared Toon Textures](#shared-toon-textures)
  - [Shader Override](#shader-override)
    - [Shader Mode List](#shader-mode-list)
    - [Alpha Processing](#alpha-processing)
    - [State Normalization](#state-normalization)
  - [MToon Shading](#mtoon-shading)
    - [MaterialUniform](#materialuniform)
    - [lit/shade Interpolation Formula](#litshade-interpolation-formula)
    - [Outline Rendering](#outline-rendering)
    - [Rim Lighting](#rim-lighting)
    - [MatCap Texture](#matcap-texture)
    - [VRM Parameter Mapping](#vrm-parameter-mapping)
    - [UV Animation](#uv-animation)
    - [Transparent Draw Order Control (alphaMode / transparentWithZWrite / renderQueueOffsetNumber)](#transparent-draw-order-control-alphamode--transparentwithzwrite--renderqueueoffsetnumber)
  - [Bloom Post-Effect (v0.2.18)](#bloom-post-effect-v0218)
    - [Dual Kawase Algorithm](#dual-kawase-algorithm)
    - [MRT (Multiple Render Target) Emissive Separation](#mrt-multiple-render-target-emissive-separation)
    - [UI Parameters](#ui-parameters)
    - [Per-Material Bloom/Emissive Toggle (v0.2.19)](#per-material-bloomemissive-toggle-v0219)
    - [PMX/PMD Self-Emissive Material Bloom Detection](#pmxpmd-self-emissive-material-bloom-detection)
    - [Prefab Emission Support](#prefab-emission-support)
  - [Viewer Display Styles](#viewer-display-styles)
    - [Dark Theme (v0.2.15)](#dark-theme-v0215)
    - [VRM Meta Info Color Badges (v0.2.20)](#vrm-meta-info-color-badges-v0220)
    - [Splash Image (v0.2.20)](#splash-image-v0220)
    - [Bone Display](#bone-display-1)
    - [Rigid Body Display](#rigid-body-display)
    - [Joint Display (PMX/PMD only)](#joint-display-pmxpmd-only)
    - [Wireframe Draw Modes](#wireframe-draw-modes)
    - [Normal Map Display](#normal-map-display)
    - [Normal Map Tangent Space (TBN)](#normal-map-tangent-space-tbn)
    - [Render Order](#render-order)
  - [Camera & Lighting](#camera--lighting)
    - [Camera](#camera)
    - [Fit Calculation (compute_fit)](#fit-calculation-compute_fit)
    - [Lighting](#lighting)
    - [MMD Ambient Separation](#mmd-ambient-separation)
  - [Log Output](#log-output)
    - [Overall Log Structure](#overall-log-structure)
    - [Panic Log](#panic-log)
  - [Single Instance](#single-instance)
  - [FPS Measurement](#fps-measurement)
  - [Animation Playback](#animation-playback)
    - [Pose Reset on Animation Clear (v0.2.20)](#pose-reset-on-animation-clear-v0220)
    - [Supported Formats](#supported-formats)
    - [Animation Playback for PMX/PMD](#animation-playback-for-pmxpmd)
    - [Humanoid Retargeting](#humanoid-retargeting)
    - [FBX Animation Coordinate Transformation](#fbx-animation-coordinate-transformation)
    - [Unity .anim Muscle Conversion (Hidden Feature)](#unity-anim-muscle-conversion-hidden-feature)
    - [Loop Modes](#loop-modes)
  - [Model Append Loading](#model-append-loading)
    - [Bone Merge 3-Level Fallback Method](#bone-merge-3-level-fallback-method)
    - [ASCII FBX Content Block Processing](#ascii-fbx-content-block-processing)
    - [FBX Parser Input Validation](#fbx-parser-input-validation)
    - [pkg Texture Namespace](#pkg-texture-namespace)
  - [Direct Archive Loading](#direct-archive-loading)
    - [archive Module](#archive-module)
    - [Viewer Integration](#viewer-integration)
    - [CLI](#cli)
  - [Archive D&D Reload Support](#archive-dd-reload-support)
    - [ReloadableSource enum](#reloadablesource-enum)
    - [Temp Path Detection](#temp-path-detection)
    - [Immediate Load for Temp Paths](#immediate-load-for-temp-paths)
    - [D&D Preload Cache (PreloadedData)](#dd-preload-cache-preloadeddata)
    - [Auxiliary File Cache](#auxiliary-file-cache)
    - [TextureSource enum](#texturesource-enum)
    - [reload_from_source](#reload_from_source)
    - [Texture D&D Preview Cache](#texture-dd-preview-cache)
    - [UnityPackage Archive Snapshot](#unitypackage-archive-snapshot)
    - [.gltf Exclusion](#gltf-exclusion)
  - [Prefab Texture Mapping (v0.2.16)](#prefab-texture-mapping-v0216)
    - [GUID Reference Chain](#guid-reference-chain)
    - [UnityPackageIndex](#unitypackageindex)
    - [Prefab Format Detection](#prefab-format-detection)
    - [Prefab Variant Resolution](#prefab-variant-resolution)
    - [Three-Stage Texture Matching Fallback](#three-stage-texture-matching-fallback)
    - [Unity YAML Parsers](#unity-yaml-parsers)
    - [Key Data Types](#key-data-types)
    - [Per-FBX MaterialGroup Splitting from Prefab](#per-fbx-materialgroup-splitting-from-prefab)
    - [File Hierarchy Tree](#file-hierarchy-tree)
    - [Always-On Material Grouping](#always-on-material-grouping)
    - [Prefab Reload (A/T Stance Conversion Support)](#prefab-reload-at-stance-conversion-support)
    - [FBX Direct Selection: Prefab-Aware Reload](#fbx-direct-selection-prefab-aware-reload)
  - [Reload Texture Normalization](#reload-texture-normalization)
    - [reload_unitypackage Texture Restoration](#reload_unitypackage-texture-restoration)
    - [IrTexture Deduplication in assign_texture_source_to_material](#irtexture-deduplication-in-assign_texture_source_to_material)
  - [Shader-Aware PMX Material Conversion](#shader-aware-pmx-material-conversion)
    - [select_toon()](#select_toon)
    - [MToon ambient/specular Correction](#mtoon-ambientspecular-correction)
    - [UTS2 (Unity-Chan Toon Shader Ver.2) Approximate Conversion](#uts2-unity-chan-toon-shader-ver2-approximate-conversion)
  - [A-Stance Conversion Result Management](#a-stance-conversion-result-management)
    - [AStanceResult enum](#astanceresult-enum)
    - [Determination Logic](#determination-logic)
    - [primary_astance_result](#primary_astance_result)
    - [IrModel::merge() Integration](#irmodelmerge-integration)
    - [Viewer Warning Display](#viewer-warning-display)
  - [UV Map PSD Layer Grouping](#uv-map-psd-layer-grouping)
    - [PSD Group Folder Mechanism](#psd-group-folder-mechanism)
    - [Data Flow](#data-flow)
    - [Input Validation (`validate_groups`)](#input-validation-validate_groups)
    - [Entry Construction (`build_entries`)](#entry-construction-build_entries)
    - [`MaterialGroup` Struct (`viewer/app/mod.rs`)](#materialgroup-struct-viewerappmodrs)
  - [Visible Materials Only Export](#visible-materials-only-export)
    - [Design Principles](#design-principles)
    - [Processing Flow (`build_filtered_ir`)](#processing-flow-build_filtered_ir)
    - [Recursive Morph Validity Check](#recursive-morph-validity-check)
    - [Texture Pruning](#texture-pruning)
    - [Specification](#specification)
  - [Architecture](#architecture-1)
  - [Source File Structure](#source-file-structure)
  - [Library API](#library-api)
  - [Tests](#tests)
  - [Changelog](#changelog)
  - [Limitations](#limitations)
  - [References](#references)
    - [Key Points of the VRM Specification](#key-points-of-the-vrm-specification)
    - [Key Points of the PMX Specification](#key-points-of-the-pmx-specification)
    - [Key Points of the PMD Specification](#key-points-of-the-pmd-specification)
  - [WGSL Shader Architecture](#wgsl-shader-architecture)
    - [Common Macros](#common-macros)
    - [Shader Constants](#shader-constants)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

[日本語](technical.md)

# Technical Details

Detailed documentation on the internal implementation of popone.

## Coordinate Transformation

Conversion from glTF right-handed coordinate system to PMX left-handed coordinate system. Scale factor: `PMX_SCALE = 12.5` (1m = 12.5 PMX units).

| | VRM 0.0 | VRM 1.0 | FBX |
|--|---------|---------|-----|
| Input coordinate system | glTF (+Z facing, root Y=180° rotation) | glTF (-Z facing) | Depends on GlobalSettings (Y-Up / Z-Up) |
| Position transform | `(-x, y, z) × scale` | `(x, y, -z) × scale` | coord_fn (based on GlobalSettings) → glTF space |
| Normal transform | `(-x, y, z)` | `(x, y, -z)` | Same as above (inverse transpose matrix) |
| Face winding | b↔c swap (determinant -1) | b↔c swap (determinant -1) | b↔c swap (determinant -1) |
| Scale | glTF meter units | glTF meter units | UnitScaleFactor / 100 (cm → m conversion) |
| PreRotation | None | None | Apply Model node's PreRotation to world transform |

### PMX/PMD → IrModel Reverse Conversion

To display PMX/PMD files in the viewer, PMX coordinates are reverse-converted to glTF coordinates.

| Target | Conversion |
|--------|------------|
| Position | `(x, y, -z) / 12.5` |
| Normal | `(x, y, -z)` |
| Morph offset (position) | `(x, y, -z) / 12.5` (displacement vector, scale required) |
| Morph offset (normal/tangent) | `(x, y, -z)` (direction vector, no scale) |
| Face winding | b↔c swap (reversed in inverse conversion) |
| Rigid body / Joint position | Kept in PMX coordinates as-is (viewer renders in PMX coordinates) |

#### PMD-Specific Conversion

| Target | Processing |
|--------|-----------|
| Rigid body position | Bone-relative offset → converted to absolute coordinates via `bone.position + offset` |
| Rigid body rotation | Absolute Euler angles (used as-is, no conversion needed) |

## Bone Display

The viewer draws bones with 4 shape types based on bone flags.

### Shape Determination (Priority Order)

| Priority | Condition | Shape | Drawing |
|----------|-----------|-------|---------|
| 1 | `BONE_FLAG_IK` / PMD type=2 | ◻ IK Controller | Blue outline square + orange fill + blue center square |
| 2 | `BONE_FLAG_AXIS_FIXED` | ⊗ Axis-fixed | Blue outer circle (thick) + ✕ (thick) |
| 3 | `BONE_FLAG_TRANSLATABLE` / PMD type=1 | ◻ Move | Blue outer square + blue inner square + blue center fill |
| 4 | None | ◎ Normal | Blue outer circle + blue inner circle + blue center fill |

### IK-Affected Bones

Bones registered in IK Link chains are displayed in orange (outline and tail triangle in orange, center fill in blue). Target bones use normal color (blue).

### Drawing Direction

| Source | Method |
|--------|--------|
| PMX/PMD | self→tail (`BoneTail::BoneIndex` / `BoneTail::Offset`) |
| VRM/FBX | parent→self (fallback) |

During animation, `tail_bone_index` (from `BoneTail::BoneIndex`) references the dynamic position from `animated_globals`, keeping the tail synced with the model.

### Rendering Pipeline

3-stage rendering controls overlap order.

| Order | Pipeline | Content |
|-------|----------|---------|
| 1 | LineList | Tail triangles (backmost) |
| 2 | TriangleList | Marker fill faces (over tail) |
| 3 | LineList | Marker outlines (frontmost) |

4 passes ensure higher-priority bones are always drawn in front: Normal(0) → IK-affected(1) → Axis-fixed(2) → IK Controller(3).

### IrBone Fields

| Field | Type | PMX | PMD | VRM/FBX |
|-------|------|-----|-----|---------|
| `tail_position` | `Option<Vec3>` | BoneTail → glTF coords | child → glTF coords | None |
| `tail_bone_index` | `Option<usize>` | BoneTail::BoneIndex | child index | None |
| `is_ik` | `bool` | IK Link bones | IK Chain bones | false |
| `is_ik_bone` | `bool` | BONE_FLAG_IK | bone_type==2 | false |
| `is_translatable` | `bool` | BONE_FLAG_TRANSLATABLE | bone_type==1 | false |
| `is_axis_fixed` | `bool` | BONE_FLAG_AXIS_FIXED | false | false |
| `is_visible` | `bool` | BONE_FLAG_VISIBLE | bone_type!=7 | true |

## MMD Standard Bone Insertion

`insert_standard_bones()` automatically inserts the following bones required for VMD motion playback.

### Base Bones

| Japanese Name | English Name | Description |
|---------------|--------------|-------------|
| 全ての親 | master | Root bone |
| センター | center | Torso movement |
| グルーブ | groove | Vertical movement |
| 腰 | waist | Branch point between upper and lower body |

### IK Bones

| Japanese Name | Description |
|---------------|-------------|
| Left/Right leg IK parent (左足ＩＫ親 / 右足ＩＫ親) | Movement parent of leg IK |
| Left/Right leg IK (左足ＩＫ / 右足ＩＫ) | Ankle IK (links: knee → leg) |
| Left/Right toe IK (左つま先ＩＫ / 右つま先ＩＫ) | Toe IK (links: ankle) |

### Semi-Standard Bones

| Japanese Name | Description |
|---------------|-------------|
| Waist cancel left/right (腰キャンセル左 / 右) | Cancels waist rotation |
| Left/Right leg D and others (左足D / 右足D etc.) | Leg grant bones (leg, knee, ankle) × left/right |
| Left/Right toe EX (左足先EX / 右足先EX) | Toe grant bones |
| Left/Right arm twist (左腕捩 / 右腕捩) | Upper arm twist bones |
| Left/Right wrist twist (左手捩 / 右手捩) | Forearm twist bones |
| Left/Right shoulder C (左肩C / 右肩C) | Shoulder cancel bones |
| Left/Right shoulder P (左肩P / 右肩P) | Shoulder parent bones |

### insert_standard_bones Step Details

Standard bone insertion consists of 18 steps. Each step is logged with a `[stepN]` tag.

| Step | Processing | Description |
|------|-----------|-------------|
| 1 | Position / index acquisition | Get positions of lower body, ankle, and toe; calculate waist bone Y coordinate |
| 2 | Existing index shift | Shift existing bone parent/tail/IK/grant indices by +4 for the 4 bones inserted at the front (master parent, center, groove, waist) |
| 3 | Parent-child setup | Set lower body and upper body parent to waist |
| 3.5 | Upper body tail setup | Set upper body tail to upper body 2 (if it exists) |
| 4 | Vertex weight shift | Shift all vertex bone_index by +4 |
| 5 | Rigid body bone_index shift | Shift all rigid body bone_index by +4 |
| 6 | Standard bone construction / linking | Construct the 4 bones (master parent, center, groove, waist), place at front, and link to existing bones |
| 9 | Upper body group alignment | Move upper body → upper body 2 → upper body 3 → neck → head → lower body in order to right after IK (idx=4) |
| 10 | Lower body bone reversal | Swap lower body bone position and tail so the bone points downward |
| 11 | Waist cancel bone addition | Add waist cancel right/left. Inherit waist rotation at ×(-1.0), become parent of leg bones |
| 12 | Leg D bone group addition | Add D auxiliary bones for IK link bones (leg, knee, ankle). Inherit original bone rotation at ×1.0 via grant |
| 13 | Toe EX addition | Add left/right toe EX (左足先EX / 右足先EX) as children of ankle D (only if toes exist) |
| 14 | D bone parent change | Change parent of auxiliary bones parented to IK-influenced bones to corresponding D bones. Propagate deform layer recursively |
| 15 | Arm twist / wrist twist addition | Add left/right arm twist (左腕捩 / 右腕捩) and left/right wrist twist (左手捩 / 右手捩) at midpoint between upper arm–elbow and elbow–wrist |
| 16 | Shoulder cancel bone addition | Add left/right shoulder P (左肩P / 右肩P, shoulder parent) and left/right shoulder C (左肩C / 右肩C, shoulder cancel) |
| 17 | IK bone group addition | Add leg IK parent, leg IK (足ＩＫ), toe IK (つま先ＩＫ), and IK tip bones at the end (left → right order, Animasa / Miku Ver2 compliant) |
| 18 | D bone group tail alignment | Align D bones and toe EX after IK bones (at the very end) in right → left order |

After these steps, `fix_duplicate_names` (duplicate bone name resolution) and `sort_bones_topological` (deform order sorting) are executed to finalize the bone array.

### PmxBuildOptions

PMX model build options are managed by the `PmxBuildOptions` struct.

| Field | CLI | Description |
|-------|-----|-------------|
| `align_rigid_rotation` | `--align-rigid-rotation` | Align rigid body rotation to bone direction |
| `no_physics` | `--no-physics` | Skip rigid body and joint output |
| `raw_structure` | `--raw-structure` | Skip standard bone insertion and keep original bone names |
| `scale` | `--scale` | PMX export scale multiplier (default: 1.0). Applied to bone positions, vertex positions, morph offsets, rigid body positions/sizes, joint positions/move limits |

When the source model has 0 bones (e.g., static FBX meshes), a single dummy bone named after the model is automatically created at the origin, with all vertex weights assigned as BDEF1(bone=0). In this case, `insert_standard_bones()` is skipped (humanoid-specific bones like IK would generate invalid references).

When `raw_structure` is enabled, `insert_standard_bones()` is completely skipped. `fix_duplicate_names` and `sort_bones_topological` are always executed. Bone names use `IrBone.original_name` (VRM: glTF node name, FBX: FBX node name) directly in PMX output.

When `raw_structure` is enabled, `IrBone.grant` is converted to `PmxGrant` with corresponding `BONE_FLAG_ROTATION_GRANT` / `BONE_FLAG_MOVE_GRANT` / `BONE_FLAG_LOCAL_GRANT` flags. Additionally, `is_translatable` (`BONE_FLAG_TRANSLATABLE`), `is_axis_fixed` (`BONE_FLAG_AXIS_FIXED`), and `is_visible` (`BONE_FLAG_VISIBLE`) are faithfully reflected from `IrBone` values. This preserves bone flags and grant data during PMX → IrModel → PMX round-trips.

#### VrmConvertOptions

The public API for VRM → PMX conversion manages options via the `VrmConvertOptions` struct.

| Field | Description |
|-------|-------------|
| `no_physics` | Skip physics (rigid bodies & joints) output |
| `align_rigid_rotation` | Align rigid body rotation to bone direction |
| `normalize_pose` | Normalize to A-stance |
| `raw_structure` | Skip standard bone insertion (preserve original bone structure) |
| `scale` | PMX export scale multiplier (default: 1.0) |

`VrmConvertOptions` is internally converted to `PmxBuildOptions`. `convert_ir_to_pmx` (for the viewer) accepts `PmxBuildOptions` directly.

## PMX Grant Animation

Processes rotation grants (`BONE_FLAG_ROTATION_GRANT`) and move grants (`BONE_FLAG_MOVE_GRANT`) during animation playback.

### D-bones Mechanism

Standard MMD models (e.g., Tda Miku) have "D-bones" (leg D, knee D, ankle D) corresponding to IK link bones (leg, knee, ankle). Vertex weights are assigned to D-bones, which copy FK bone rotations via rotation grants (ratio=1.0).

```
Lower body
├ Left leg     ← VRMA "leftUpperLeg" rotation applied here
├ Left leg D   ← Rotation grant copies "Left leg" rotation (ratio=1.0)
│ └ Left knee D ← Rotation grant copies "Left knee" rotation
│   └ Left ankle D
```

### Processing Flow

```
1. compute_animated_globals_inplace()  — Apply VRMA retargeted rotations
2. apply_grants()                      — Apply grant deltas and recompute globals
   Phase 1: Iterate bones in index order, extract grant parent's local rotation/
            translation delta, apply with ratio to work buffer (work_local_mats)
   Phase 2: Recompute all bone global matrices in index order (parent→child propagation)
3. Delta matrix computation → vertex skinning
```

### IrGrant Data Structure

| Field | Type | Description |
|-------|------|-------------|
| `parent_index` | `usize` | Grant parent bone index |
| `ratio` | `f32` | Grant ratio (1.0 = full copy, -1.0 = inverse rotation) |
| `is_rotation` | `bool` | Rotation grant flag |
| `is_move` | `bool` | Move grant flag |
| `is_local` | `bool` | Local grant flag |

## OBJ/STL Loading

### OBJ Reader

- Parsed with `tobj` crate (`GPU_LOAD_OPTIONS`: triangulation + single-index)
- Auto-loads MTL material files. Falls back to default white material with warning if MTL is missing
- Textures embedded as byte data in `IrTexture` (not file path references)
- Memory loading (archive/snapshot): custom MTL loader resolves from `aux_files`
- Sidecar resolution (`resolve_sidecar`):
  - Archive/snapshot source: path normalization → exact match → case-insensitive → basename fallback. Disk fallback disabled
  - Normal file: `base_dir.join(rel)` direct disk read (`..` paths preserved as-is)
- OBJ without normals: face normals accumulated and normalized for smooth shading

### STL Reader

- ASCII and binary format support (custom parser)
- Format detection: binary length validation (`84 + tri_count × 50 == data.len()`) prioritized, falls back to ASCII on mismatch
- Binary: 80-byte header + u32 triangle count + triangle data (normal 3×f32 + vertices 3×3×f32 + u16 attribute = 50 bytes/face)
- Zero/invalid normals: recalculated from vertex positions when `length_squared < 1e-8`

### Coordinate Conversion

| Format | Assumed Unit | Assumed Coordinate System | Conversion |
|--------|-------------|--------------------------|------------|
| OBJ | cm | Y-Up right-hand | ÷100 (cm → m) only. No axis conversion |
| STL | mm | Z-Up | ÷1000 (mm → m) + Y↔Z swap + face winding reversal (b↔c swap) |

- Y↔Z swap has determinant = -1 → face winding reverses, requiring b↔c swap
- After conversion: glTF space (Y-Up right-hand, meters) → viewer applies `gltf_pos_to_pmx` (×12.5) for PMX units

### IrModel Construction

- Static mesh: single root bone ("全ての親"), all vertex weights `(0, 1.0)` as BDEF1
- OBJ: meshes split per material (tobj Model unit). MTL `Kd`/`Ks`/`Ns`/`d` → `IrMaterial`, `map_Kd` → `IrTexture`
- STL: single default white material. No textures or UVs. Flat shading (3 independent vertices per triangle)

### Dynamic Grid

- `compute_grid_params()` auto-calculates grid extent and step from model bbox
- Default (extent=100, step=5) is the minimum; only enlarged when bbox exceeds ±100 PMX units
- Rounded to nice values: extent → 200, 500, 1000, ...; step → 10, 20, 50, ...
- `GpuRenderer::rebuild_grid()` rebuilds GPU buffer (on model load + append)

## PMX/PMD Loading

### PMX Reader

- PMX 2.0 / 2.1 binary support
- UTF-16LE / UTF-8 text auto-detection (follows header encoding)
- Variable index size: vertex (unsigned 1/2/4), others (signed 1/2/4)
- SDEF → BDEF2 fallback, QDEF → treated as BDEF4
- PMX 2.1: flip morph → treated as Group, impulse morph → skipped, SoftBody → skipped

### PMD Reader

- Shift_JIS → UTF-8 conversion via `encoding_rs`
- Fixed-length structure parsing (vertex 38 bytes, material 70 bytes, bone 39 bytes)
- IK is in a separate section → not merged into bone info, kept as `PmdIk`
- Morphs: base + offset format → expanded to global vertex indices
- English header, toon textures, rigid bodies, and joints are optional (skipped at EOF)
- Material name text file: if a `.txt` file (S-JIS) with the same name as the PMD exists and its line count matches the material count, lines are applied as material names

### IrModel Conversion

- Vertex index mapping: When splitting meshes, build a mapping table from PMX/PMD global vertices → IrModel sequential numbers, and convert morph vertex indices
- Bone name mapping: `pmx_name_to_vrm_bone()` provides reverse lookup from PMX Japanese bone name → VRM humanoid name (for VRMA animation playback)
- **Important**: `"センター"` → `"hips"` mapping (PMX center (センター) corresponds to VRM hips, not the lower body)
- **Morph index remapping**: PMX includes bone/material/UV morphs, but IrModel only retains vertex and group morphs. Since skipping morphs shifts indices, `extract_morphs` performs a 2-pass conversion:
  1. Build PMX morph index → IrModel morph index mapping table (skipped morphs map to `None`)
  2. Remap group morph sub-morph references to remapped indices. References to skipped morphs are excluded
- **Group morph recursion depth limit**: The viewer's `apply_gpu_morph_recursive` recursively expands group morphs. To prevent infinite recursion → stack overflow from circular or self-referencing models, expansion is capped at max depth 16

### T-Stance Conversion

`normalize_pose_to_tstance_full()` converts A-stance → T-stance:

1. Detect left/right upper arms (`vrm_bone_name` or PMX name `"左腕"` / `"右腕"`)
2. Calculate angle from arm direction to horizontal and generate inverse rotation correction quaternion
3. Correct bone positions and global matrices
4. Rotate mesh vertices and normals based on skin weights
5. Apply rotation to morph offsets (position, normal, tangent)
6. Rigid bodies / joints: correct position and rotation of those belonging to descendants of affected bones

### Rigid Body Rotation

PMX/PMD rigid body rotation is stored as Euler angles. Following the D3DX row-major convention `v * Ry * Rx * Rz` (extrinsic ZXY), reconstructed in glam column-major as `Rz * Rx * Ry` (intrinsic YXZ). File values are used as-is (no coordinate conversion needed).

#### Rigid Body Animation Tracking Coordinate Conversion

The viewer renders rigid bodies and joints in PMX space. `rb.position` and `joint.position` are kept in PMX coordinates, but `bone.position` and `bone.global_mat` are converted to glTF space during PMX/PMD extraction (`pmx_pos_to_gltf`). Therefore, the animation tracking delta computation applies glTF→PMX coordinate conversion uniformly across all formats:

- **Position conversion**: PMX/PMD uses the same Z-flip as VRM 1.0 (`pmx_pos_to_gltf(v) = (x/S, y/S, -z/S)`), so `gltf_pos_to_pmx` is used for inverse conversion
- **Rotation delta**: Z-flip `Quat(-x, -y, z, w)` is applied (same path as VRM 1.0)

### Texture Loading

- PMX: Load from relative paths in the texture path table
- PMD: `parse_pmd_texture_slots` separates main/sphere textures via `*` delimiter. `.sph`→multiply, `.spa`→add. Toon textures registered with file existence check, falling back to shared toon if not found
- MIME hint: Infer MIME type from extension and explicitly specify via `image::load_from_memory_with_format` (TGA has no magic number so auto-detection fails). `.sph/.spa` treated as `image/bmp`
- UnityPackage textures: `embed_textures_into_ir` derives MIME type from file extension via `mime_for_ext`. Without MIME hints, TGA/BMP auto-detection fails and falls back to magenta

## MMD Rendering

MMD rendering mode that auto-enables on PMX/PMD load.

### Architecture

- **RenderStyle enum** — Per-DrawCall `Standard` / `Mmd` determination (based on material's `source_format.is_pmx_pmd()`). Works correctly with append-mixed models
- **Per-frame sRGB/Unorm switching** — PMX/PMD-only frames (all visible materials are MMD) use `Rgba8Unorm` render target for correct gamma-space alpha blending. Falls back to `Rgba8UnormSrgb` when VRM is mixed
- **4 pipeline sets** — `(MSAA on/off) × (sRGB/Unorm)` = 4 sets created at init. Runtime cost is pipeline reference switching only
- **Texture dual views** — `view_formats: [Rgba8Unorm]` creates both sRGB/Unorm views for the same texture. MMD reads via Unorm view (gamma space, zero memory overhead)

### MMD Shaders

#### Main Shader (`MMD_MAIN_SHADER_SRC` / `MMD_MAIN_SHADER_UNORM_SRC`)

```
Preshader:
  // AmbientColor = saturate(MaterialAmbient × LightAmbient + MaterialEmissive)
  // PMX ambient = D3D emissive, PMX diffuse = D3D ambient
  base_color = clamp(mat.diffuse_rgb * LightAmbient + mat.ambient, 0, 1)
  // LightAmbient = 154/255 ≈ 0.604

Pixel:
  tex = texture(Unorm)
  out_rgb = base_color * tex.rgb
  out_a   = tex.a * mat.alpha

  // Sphere map (RGB only, no alpha influence)
  // sphere_uv: X-inverted coord → vn_x * -0.5 + 0.5, vn_y * -0.5 + 0.5
  sph = sphere_texture(sphere_uv).rgb
  out_rgb += sph  // add mode
  out_rgb *= sph  // mul mode

  // Toon (NdotL-dependent sampling + multiply)
  lightNormal = dot(N, -L)
  toon_uv = (0, 0.5 - lightNormal * 0.5)
  toon = toon_texture(toon_uv)
  out_rgb *= toon.rgb

  // Alpha test
  if out_a < 0.004: discard

  // Specular (added last, unaffected by toon)
  // LightSpecular = LightAmbient (≈0.604)
  spec_color = mat.specular * LightSpecular
  out_rgb += spec_color * pow(NdotH, specular_power)

  // sRGB version: pow(2.2) to counteract sRGB encode
  // Unorm version: output gamma values directly
```

#### Edge Shader (`MMD_EDGE_SHADER_SRC` / `MMD_EDGE_SHADER_UNORM_SRC`)

- Inverted hull method (Front cull)
- Normal expansion: `offset = edge_scale × mat.edge_size × camera.edge_thickness × pow(dist, 0.7) × 0.003`
- 2-slot vertex buffer: slot0=existing Vertex, slot1=edge_scale(f32)
- sRGB version: `pow(edge_color, 2.2)` to counteract sRGB encode
- Unorm version: output edge_color directly

### Pipeline Configuration

Each sRGB/Unorm set contains identical pipeline structure (2×2=4 sets total).

| Pipeline | Cull | Depth Write | Purpose |
|----------|------|-------------|---------|
| mmd_main_cull | Back | yes | MMD opaque (single-sided) |
| mmd_main_no_cull | none | yes | MMD opaque (double-sided) |
| mmd_alpha_cull | Back | yes | MMD transparent (single-sided) |
| mmd_alpha_no_cull | none | yes | MMD transparent (double-sided) |
| mmd_edge | Front | yes | Edge |

MMD transparent pipelines also write to depth (MMD-compliant). Since materials are drawn in index order, the model author's intended front-to-back order is preserved.

#### MMD Draw Order

MMD draws materials one by one in material index order. popone likewise uses a single loop to draw in material order, switching pipelines (opaque/alpha) based on `is_alpha`. For opaque materials, edges are drawn immediately after the main draw.

```
for each material (in index order):
    select pipeline (opaque or alpha based on diffuse.w < 1.0)
    draw material
    if opaque && has_edge:
        draw edge
```

`can_use_unorm_frame()` determines per-frame; Unorm set used only when all visible materials are MMD.

### Color Space

MMD (D3D9) operates in gamma space. wgpu reproduction:

| Element | Standard (VRM/FBX) | MMD sRGB fallback | MMD Unorm (preferred) |
|---------|-------|------|------|
| Texture read | Rgba8UnormSrgb (auto sRGB→linear) | Rgba8Unorm (gamma space) | Rgba8Unorm (gamma space) |
| Lighting | Linear space | Gamma space | Gamma space |
| Alpha blending | Linear space (correct) | Linear space (inaccurate) | Gamma space (MMD-compliant) |
| Output | As-is | pow(2.2) to counteract sRGB encode | As-is (gamma values directly) |

### Shared Toon Textures

Actual MMD standard toon01-10 pixel data (32 rows of RGB values) stored as constant arrays, uploaded as 1×32 RGBA textures to GPU. Sampler: `ClampToEdge` + `Linear`. Shader samples with NdotL-dependent UV `(0, 0.5 − NdotL × 0.5)`, reproducing toon shading based on normal-light angle.

| Toon | Characteristics |
|------|----------------|
| toon01 | White → gray (205,205,205), 2-color step |
| toon02 | White → pink (245,225,225), 2-color step |
| toon03 | White → dark gray (154,154,154), 2-color step |
| toon04 | White → warm beige (248,239,235), 2-color step |
| toon05 | White → warm pink gradient |
| toon06 | Yellow, center highlight band + dark yellow |
| toon07-10 | All white (no toon effect) |

## Shader Override

The viewer supports 6 shader modes. Internal state is managed on 2 axes.

| Internal Field | Type | Role |
|---|---|---|
| `shader_override` | `ShaderOverride` (u32) | GPU fragment shader branching (Default=0 / Normal=1 / Unlit=2 / GgxPreview=3) |
| `use_mmd_path` | `bool` | CPU-side MMD dedicated render path toggle |
| `auto_shader` | `bool` | Auto mode (auto-detection based on model format) |

Passed to the fragment shader as `CameraUniform.shader_mode: u32`, with early-return branching via integer comparison at the top of `fs_main`. MMD uses separate pipelines so it is not included in `shader_mode`; the CPU-side `mmd_solid` flag controls the draw path.

### Shader Mode List

| Mode | shader_mode | Draw Path | Description |
|---|---|---|---|
| Auto | 0 | Auto | Auto-selects Standard/MMD based on model format |
| MToon/Lambert | 0 | Standard (forced) | Displays PMX/PMD with MToon/Lambert |
| Unlit | 2 | Standard | Texture color only, no lighting |
| GGX Preview | 3 | Standard | Cook-Torrance GGX (metallic=0, roughness=0.8 fixed) |
| Normal | 1 | Standard | Geometry normal → RGB |
| MMD | 0 | MMD (forced) | Blinn-Phong + sphere + toon |

### Alpha Processing

The `apply_alpha_mode()` WGSL function provides unified alpha handling across all modes.

```
OPAQUE  (cutoff < -0.75): Returns texture alpha as-is (PMX/PMD transparency support)
MASK    (cutoff >= -0.25): AlphaToCoverage + fwidth smoothing
BLEND   (else):           Discard fully transparent pixels only
```

Override modes (Unlit / GGX / Normal) output texture alpha directly without `apply_alpha_mode`, ensuring PMX/PMD OPAQUE materials still show texture transparency.

### State Normalization

`normalize_shader_state()` is called on all model load / rebuild / append paths. Only Auto mode auto-sets `use_mmd_path` based on model format. Explicit user selections are preserved across model loads.

The edge drawing UI (ON/OFF toggle and thickness slider) is shown both when MMD mode is explicitly selected and when Auto mode has `use_mmd_path == true` (i.e., PMX/PMD loaded).

## MToon Shading

VRM MToon materials use a fragment shader branch within the Standard pipeline for 2-color toon shading + rim lighting + MatCap, and a dedicated pipeline for outline rendering.

### MaterialUniform

```rust
// 448 bytes (gpu.rs)
pub struct MaterialUniform {
    pub diffuse: [f32; 4],              // Base color (16 bytes)
    pub shade_color: [f32; 3],          // MToon shade color (12 bytes)
    pub is_mtoon: f32,                  // 0.0 or 1.0 (4 bytes)
    pub shading_toony: f32,             // Shadow boundary sharpness 0.0~1.0 (4 bytes)
    pub shading_shift: f32,             // Shadow threshold shift -1.0~1.0 (4 bytes)
    pub outline_width: f32,             // Outline width (4 bytes)
    pub outline_mode: f32,              // 0=none, 1=world, 2=screen (4 bytes)
    pub outline_color: [f32; 4],        // Outline color (16 bytes)
    pub outline_lighting_mix: f32,      // Lighting mix ratio 0~1 (4 bytes)
    pub rim_fresnel_power: f32,         // Rim Fresnel exponent (4 bytes)
    pub rim_lift: f32,                  // Rim lift amount (4 bytes)
    pub rim_lighting_mix: f32,          // Rim lighting mix ratio (4 bytes)
    pub rim_color: [f32; 3],            // Rim color (12 bytes)
    pub has_matcap: f32,                // MatCap enabled flag 0.0/1.0 (4 bytes)
    pub matcap_factor: [f32; 3],        // MatCap multiply color (12 bytes)
    pub has_shade_multiply_tex: f32,    // shadeMultiplyTexture present (4 bytes)
    pub has_shading_shift_tex: f32,     // shadingShiftTexture present (4 bytes)
    pub shading_shift_tex_scale: f32,   // shadingShiftTexture scale (4 bytes)
    pub has_rim_multiply_tex: f32,      // rimMultiplyTexture present (4 bytes)
    pub uv_anim_scroll_x: f32,         // UV scroll X speed (4 bytes)
    pub uv_anim_scroll_y: f32,         // UV scroll Y speed (4 bytes)
    pub uv_anim_rotation: f32,         // UV rotation speed (4 bytes)
    pub has_uv_anim_mask: f32,          // uvAnimationMaskTexture present (4 bytes)
    pub alpha_cutoff: f32,              // alphaMode sentinel (4 bytes: -1.0=OPAQUE, -0.5=BLEND, >=0.0=MASK cutoff)
    // --- Texture UV parameters ---
    pub base_uv_a: [f32; 4],            // baseColor texCoord+transform (16 bytes)
    pub base_uv_b: [f32; 4],            // baseColor texCoord+transform (16 bytes)
    pub shade_uv_a: [f32; 4],           // shade texCoord+transform (16 bytes)
    pub shade_uv_b: [f32; 4],           // shade texCoord+transform (16 bytes)
    pub shift_uv_a: [f32; 4],           // shift texCoord+transform (16 bytes)
    pub shift_uv_b: [f32; 4],           // shift texCoord+transform (16 bytes)
    pub rim_uv_a: [f32; 4],             // rim texCoord+transform (16 bytes)
    pub rim_uv_b: [f32; 4],             // rim texCoord+transform (16 bytes)
    pub outline_uv_a: [f32; 4],         // outline texCoord+transform (16 bytes)
    pub outline_uv_b: [f32; 4],         // outline texCoord+transform (16 bytes)
    pub uv_mask_uv_a: [f32; 4],         // uv_mask texCoord+transform (16 bytes)
    pub uv_mask_uv_b: [f32; 4],         // uv_mask texCoord+transform (16 bytes)
    pub emissive_factor: [f32; 3],      // glTF emissiveFactor (12 bytes)
    pub has_emissive_tex: f32,          // emissiveTexture presence (4 bytes)
    pub emissive_uv_a: [f32; 4],       // emissive texCoord+transform (16 bytes)
    pub emissive_uv_b: [f32; 4],       // emissive texCoord+transform (16 bytes)
    // --- Normal map + GI ---
    pub has_normal_tex: f32,            // normalTexture presence (4 bytes)
    pub normal_scale: f32,              // normalTexture.scale (4 bytes)
    pub gi_equalization_factor: f32,    // GI equalization factor 0.0~1.0 (4 bytes)
    pub outline_width_channel: f32,    // outlineWidthTexture channel 0=R,1=G,2=B (4 bytes)
    pub normal_uv_a: [f32; 4],         // normal texCoord+transform (16 bytes)
    pub normal_uv_b: [f32; 4],         // normal texCoord+transform (16 bytes)
    pub uv_anim_mask_channel: f32,     // uvAnimMaskTexture channel 0=R,1=G,2=B (4 bytes)
    pub _pad: [f32; 3],               // Padding (12 bytes)
    // --- matcap UV parameters ---
    pub matcap_uv_a: [f32; 4],        // matcap texCoord+transform (16 bytes)
    pub matcap_uv_b: [f32; 4],        // matcap texCoord+transform (16 bytes)
}
```

### lit/shade Interpolation Formula

```wgsl
// Spec-compliant: dot(N,L) [-1,1] range (not half-lambert)
// camera.light_dir is light travel direction (light→surface), negate to get surface→light
let dot_nl = dot(n, -light_dir);

// shadeMultiplyTexture: multiply shade color by texture
var shade_mul = vec3(1.0);
if has_shade_multiply_tex > 0.5 { shade_mul = textureSample(t_shade_multiply, ...).rgb; }
let shade = shade_color * shade_mul;

// shadingShiftTexture: per-pixel shadow threshold shift (VRM 1.0 spec: tex.r * scale)
var shading = dot_nl + shading_shift;
if has_shading_shift_tex > 0.5 {
    shading += textureSample(t_shading_shift, ...).r * shading_shift_tex_scale;
}

// Spec-compliant linearstep: clamp((x - edge0) / (edge1 - edge0), 0, 1)
let edge0 = -1.0 + shading_toony;
let edge1 = 1.0 - shading_toony;
let t = clamp((shading - edge0) / max(edge1 - edge0, 0.001), 0.0, 1.0);
lit = mix(shade, base_color.rgb, t);

// GI Equalization (UniVRM-compliant: indirect light only, no direct light)
// Hemisphere ambient: interpolate sky/ground by final normal Y (SH approximation, using n after normal map)
let raw_indirect = mix(ambient_ground, ambient, n.y * 0.5 + 0.5);
// uniformedGi = ambient (uniform when no SH/IBL, CameraUniform.gi_equalized)
let gi = mix(raw_indirect, gi_equalized, gi_equalization_factor);
// Rim light factor uses raw (non-equalized) indirect (UniVRM-compliant)
let rim_light_factor = light_intensity + raw_indirect;

// Final lighting composition (VRM spec: giLighting = gi(n) * litColor)
let direct_light = light_intensity * light_color;
let lighting = lit * direct_light + lit * gi;
```

- Uses `dot(N,L)` [-1,1] as input per spec (not half-lambert [0,1])
- `linearstep` interpolation (linear, not `smoothstep` cubic)
- `shading_toony = 0.9` (default) → `edge0 = -0.1, edge1 = 0.1` → very sharp shadow boundary (anime-style)
- `shading_toony = 0.0` → `edge0 = -1.0, edge1 = 1.0` → soft gradient
- `shading_shift` shifts the overall shadow position (negative = more shadow)
- Default `shadeColorFactor` when unspecified is `[0,0,0]` (black) — per spec

### Outline Rendering

Outlines are rendered using the inverted hull method via `pipeline_outline` (front-cull pipeline), expanding vertices along their normals. `outlineWidthMultiplyTexture` is sampled in the vertex shader via `textureSampleLevel` for region-specific outline width control (channel: VRM 1.0=G, VRM 0.x=R, dynamically selected via `ColorChannel` enum). Stored in mtoon_aux bind group binding 6, with material-specific bind groups used in outline draw calls. The `edge_scale` vertex attribute is not used (GPU sampling only), so the pipeline vertex layout uses a single `Vertex` buffer. `edge_scale_buf` is MMD edge-only.

```wgsl
// Vertex shader: sample outlineWidthMultiplyTexture with UV Animation applied (spec-compliant)
// No edge_scale vertex input (GPU samples texture directly, CPU-side edge_scale is for PMX export)
let width_uv = apply_uv_animation(uv);
let width_tex = select_channel(textureSampleLevel(t_outline_width, ..., width_uv, 0.0), material.outline_width_channel);
let width = outline_width * width_tex;
if outline_mode > 1.5 {
    // screenCoordinates: full UniVRM-compliant clip-space offset
    let clip = view_proj * vec4(position, 1.0);
    let nv = vec3(dot(view_row0, n), dot(view_row1, n), dot(cross(view_row0, view_row1), n));
    var projected = normalize(vec2(nv.x, nv.y));      // normalize first (UniVRM order)
    let max_dist = proj_11;                           // 1/tan(fov/2) — UniVRM maxDistance equivalent
    let clamped_w = min(clip.w, max_dist);            // distance clamp (suppress thick outlines at wide FOV/far)
    projected *= 2.0 * width * clamped_w;
    projected.x /= aspect;                            // divide by aspect(=w/h) for X correction (UniVRM multiplies h/w)
    projected *= saturate(1.0 - nv.z * nv.z);        // camera-facing suppression
    clip_position = vec4(clip.xy + projected, clip.zw);
} else {
    // worldCoordinates: world-space width in meters
    let expanded = position + n * width;
    clip_position = view_proj * vec4(expanded, 1.0);
}
```

```wgsl
// Fragment shader: compute full MToon lighting and mix with outline color
// via outlineLightingMixFactor (UniVRM-compliant)
let surface = compute_mtoon_surface_lighting(n, uv, world_pos);  // vec4: .rgb=color, .a=processed alpha
// Outline pass also discards based on base texture alpha (UniVRM-compliant)
// MASK material: surface.a < alpha_cutoff → discard, BLEND material: surface.a ≤ 0.001 → discard
// UniVRM-compliant: outlineColor * lerp(1, baseCol, outlineLightingMix)
let lit = outline_color.rgb * mix(vec3(1.0), surface.rgb, outline_lighting_mix);
```

- `compute_mtoon_surface_lighting()` is defined as a WGSL function within `wgsl_outline_body!()` macro. Performs the same calculation as the main fragment shader (2-color toon, shadeMultiply, shadingShift, rim, MatCap, rimMultiply, UV animation) and returns `vec4` (.rgb = surface shading color, .a = processed alpha). The outline pass also discards based on base texture alpha (UniVRM-compliant)
- Outline rendering binds the same `texture_bind_group` as the main pass, ensuring `baseColorTexture` is correctly referenced
- `OutlineVertexOutput` extended with `uv` and `world_pos` for texture sampling and rim calculation in the fragment shader
- Draw order: outlines are rendered after each render queue phase (OPAQUE / MASK / BlendZWrite / Blend). BLEND materials use `pipeline_outline_blend` (ZWrite OFF), rendering outlines for transparent hair and accessories per UniVRM behavior
- Togglable via "Outline Rendering" UI checkbox

### Rim Lighting

Parametric rim lighting creates glowing silhouette edges via the Fresnel effect. World position is passed from the vertex shader, and view direction V is computed in the fragment shader.

```wgsl
let v = normalize(camera_pos - world_pos);
let parametric_rim = pow(
    saturate(1.0 - dot(n, v) + rim_lift),
    max(rim_fresnel_power, 0.00001)
);
rim = parametric_rim * rim_color;
```

### MatCap Texture

Following the VRM spec, an orthonormal basis is constructed from the view direction to compute MatCap UV coordinates.

```wgsl
// UniVRM compliant: right = cross(viewDir, worldUp), up = cross(right, viewDir)
let world_view_x = normalize(vec3(-v.z, 0.0, v.x));
let world_view_y = cross(world_view_x, v);
let raw_matcap_uv = vec2(dot(world_view_x, n), dot(world_view_y, n)) * 0.495 + 0.5;
// Apply KHR_texture_transform (matcap_uv_a/b for offset/scale/rotation)
let matcap_uv = apply_texture_transform(raw_matcap_uv, matcap_uv_a, matcap_uv_b);
rim += matcap_factor * textureSample(t_matcap, matcap_uv).rgb;
```

- Bind group(3) is the MToon auxiliary texture pack (8 samplers + 8 textures = 16 bindings). Each texture has its own sampler, fully compliant with glTF's per-texture sampler model. Binding layout uses 2n = sampler, 2n+1 = texture pairs:
  - binding 0-1: s_matcap / t_matcap (FRAGMENT)
  - binding 2-3: s_shade_multiply / t_shade_multiply (FRAGMENT)
  - binding 4-5: s_shading_shift / t_shading_shift (FRAGMENT)
  - binding 6-7: s_rim_multiply / t_rim_multiply (FRAGMENT)
  - binding 8-9: s_uv_anim_mask / t_uv_anim_mask (VERTEX + FRAGMENT)
  - binding 10-11: s_outline_width / t_outline_width (VERTEX)
  - binding 12-13: s_emissive / t_emissive (FRAGMENT)
  - binding 14-15: s_normal / t_normal (FRAGMENT)
- Bind group(3) is also created for non-MToon materials that have `emissiveTexture` / `normalTexture` (MToon-specific textures fall back to defaults)
- emissiveTexture is a glTF standard property, used by both MToon and non-MToon materials
- glTF standard `emissiveTexture` / `normalTexture` also preserve `texCoord` / `KHR_texture_transform` via `read_texture_info()`
- normalTexture (binding 14-15) has FRAGMENT visibility, linear color space (Unorm view). Constructs TBN matrix from MikkTSpace-generated vertex tangents and transforms tangent-space normals to world space (per UniVRM `MToon_GetTangentToWorld()`). `normalTexture.scale` controls intensity. Materials without normal maps automatically bind a flat normal texture (1x1, RGBA=(128,128,255,255) = tangent-space (0,0,1)). Falls back to the base normal for degenerate UVs (`det ≈ 0` or near-zero vectors) to avoid undefined behavior from `normalize(vec3(0))`
- `doubleSided` materials flip back-face normals before normal map application using `@builtin(front_facing)` (equivalent to UniVRM's `MTOON_IS_FRONT_VFACE`). Applied to `fs_main` / `fs_outline` (both sRGB and Unorm variants)
- Materials without textures automatically bind default textures (matcap=black, others=white)
- `rimMultiplyTexture` applies texture-based masking to rim effect
- `rimLightingMixFactor` controls mix ratio between rim and light factor (0.0 = emission, 1.0 = fully mixed). Uses material-color-free `light_factor` (`light_intensity + ambient`, N·L independent) per UniVRM (`lerp(white, light_factor, mix)`)
- `shadingShiftTexture` / `uvAnimationMaskTexture` are loaded in linear color space (Unorm view). Color space is managed separately from sRGB textures (shadeMultiply / rimMultiply / matcap)

### VRM Parameter Mapping

| VRM 1.0 (`VRMC_materials_mtoon`) | VRM 0.0 (float_properties) | IrMaterial / MtoonParams Field |
|---|---|---|
| `shadeColorFactor` | `_ShadeColor` (vector) | `shade_color` |
| `shadingToonyFactor` | `_ShadeToony` | `shading_toony_factor` |
| `shadingShiftFactor` | `_ShadeShift` | `shading_shift_factor` |
| `outlineWidthMode` | `_OutlineWidthMode` | `outline_width_mode` |
| `outlineWidthFactor` | `_OutlineWidth` | `outline_width_factor` |
| `outlineColorFactor` | `_OutlineColor` | `edge_color` |
| `outlineLightingMixFactor` | `_OutlineLightingMix` | `outline_lighting_mix` |
| `parametricRimColorFactor` | `_RimColor` (vector) | `parametric_rim_color` |
| `parametricRimFresnelPowerFactor` | `_RimFresnelPower` | `parametric_rim_fresnel_power` |
| `parametricRimLiftFactor` | `_RimLift` | `parametric_rim_lift` |
| `rimLightingMixFactor` | Always 1.0 (destructive migration) | `rim_lighting_mix` |
| `matcapFactor` | `_SphereAdd` present→[1,1,1], absent→[0,0,0] | `matcap_factor` |
| `matcapTexture` | `_SphereAdd` | `matcap_texture: Option<IrTextureInfo>` |
| `shadeMultiplyTexture` | `_ShadeTexture` (falls back to `_MainTex`) | `shade_texture: Option<IrTextureInfo>` |
| `shadingShiftTexture` + `scale` | — | `shading_shift_texture: Option<IrTextureInfo>` + `shading_shift_texture_scale` |
| `rimMultiplyTexture` | `_RimTexture` | `rim_multiply_texture: Option<IrTextureInfo>` |
| `uvAnimationScrollXSpeedFactor` | `_UvAnimScrollX` | `uv_animation_scroll_x_speed` |
| `uvAnimationScrollYSpeedFactor` | `_UvAnimScrollY` (Y inverted × -1) | `uv_animation_scroll_y_speed` |
| `uvAnimationRotationSpeedFactor` | `_UvAnimRotation` (× 2π) | `uv_animation_rotation_speed` |
| `uvAnimationMaskTexture` | `_UvAnimMaskTexture` | `uv_animation_mask_texture: Option<IrTextureInfo>` |
| glTF `emissiveFactor` | `_EmissionColor` (vector) | `emissive_factor` |
| glTF `emissiveTexture` | `_EmissionMap` | `emissive_texture: Option<IrTextureInfo>` |
| glTF `normalTexture` | `_BumpMap` | `normal_texture: Option<IrTextureInfo>` |
| glTF `normalTexture.scale` | `_BumpScale` | `normal_texture_scale` |
| `alphaMode` | `_BlendMode` (0=OPAQUE,1=MASK,2=BLEND,3=BlendZWrite) | `alpha_mode` |
| glTF `alphaCutoff` | `_Cutoff` | `alpha_cutoff` |
| glTF `doubleSided` | `_CullMode` (0=Off→None, 1=Front→Front, 2=Back→Back) | `cull_mode: CullMode` |
| — | `renderQueue` | `render_queue_offset` (computed in post-pass) |
| glTF `baseColorFactor` | `_Color` (vector, sRGB→Linear) | `diffuse` |
| glTF `baseColorTexture` | `_MainTex` | `texture_index` / `base_color_tex_info` |
| — | `_MainTex` ST | All textures' `IrTextureInfo.offset` / `.scale` |
| `giEqualizationFactor` | `_IndirectLightIntensity` (`1.0 - value`) | `gi_equalization_factor` |

VRM 0.x-specific additional migration:

- **`_Color` / `_MainTex` lit color/texture normalization**: For VRM 0.x MToon, the glTF core `baseColorFactor` / `baseColorTexture` may be approximate values, so after MToon detection, `materialProperties._Color` (sRGB→Linear) → `diffuse` and `_MainTex` → `texture_index` / `base_color_tex_info` take priority (per UniVRM `MigrationMToonMaterial.cs:148-164`)
- **`renderQueue` → `render_queue_offset`**: Per UniVRM `MigrationMToonMaterial.cs` rank compression. Collects transparent material source offsets (`renderQueue - DefaultValue`) into a `BTreeSet`, assigns sequential ranks (Blend: descending 0, -1, -2, ...; BlendWithZWrite: ascending 0, 1, 2, ...) to compress into VRM 1.0 spec range (Blend: -9..0, BlendWithZWrite: 0..+9) while preserving relative order. Returns offset=0 when `renderQueue` falls outside the permitted range (Blend: 2951–3000, BlendWithZWrite: 2501–2550)
- **`_MainTex` ST (texture Scale/Translation) propagation**: VRM 0.x `vectorProperties._MainTex` stores `[offsetX, offsetY, scaleX, scaleY]`. Since Unity's texture coordinate system (top-left origin) and glTF `KHR_texture_transform` (bottom-left origin) have different Y-axis conventions, the offset is converted via `offset.y = 1.0 - unityOffset.y - scale.y` (per UniVRM `Vrm10MaterialExportUtils.ExportTextureTransform`). UniVRM migrates `_MainTex` ST to all MToon textures as `KHR_texture_transform`, **except MatCap (`_SphereAdd`)** which does not require texture transform in VRM 1.0 (per UniVRM `MigrationMToonMaterial.cs:255-260`: "Texture transform is not required"). Identity transforms (scale=1, offset=0) are skipped. `_OutlineWidthTexture` also propagates ST via the `resolve_tex()` helper (per UniVRM `MigrationMToonMaterial.cs`)
- **`ScreenCoordinates` outline width normalization**: `outline_width_factor = w * 0.01 * 0.5` (UniVRM-compliant: old half-height percent → new full-height ratio, 1/200 conversion)
- **Color property sRGB→Linear conversion**: VRM 0.x `_ShadeColor`, `_RimColor`, and `_OutlineColor` are stored in sRGB gamma space, so IEC 61966-2-1 compliant sRGB→Linear conversion is applied during extraction (equivalent to UniVRM `MigrationMToonMaterial.cs` `.ToFloat3(ColorSpace.sRGB, ColorSpace.Linear)`). `_EmissionColor` is excluded as it is Linear→Linear per UniVRM
- **`_IndirectLightIntensity` → `gi_equalization_factor`**: Applies UniVRM-compliant conversion formula `gi_equalization_factor = (1.0 - gi_intensity).clamp(0.0, 1.0)`. Sent to GPU shader via `MaterialUniform` and applies `lerp(passthroughGi, uniformedGi, giEqualizationFactor)` for GI equalization. Without SH/IBL, `passthroughGi` = `uniformedGi` = ambient (equivalent to UniVRM's `indirectLight` / `indirectLightEqualized`, excludes direct light)

`IrTextureInfo` holds texture index plus `tex_coord` (TEXCOORD set number), `KHR_texture_transform` (offset / scale / rotation), and `IrSamplerInfo` (wrap_u / wrap_v / mag_filter: `IrMagFilter` / min_filter: `IrMinFilter`). `IrMinFilter` preserves all 6 glTF `minFilter` values (Nearest / Linear / NearestMipmapNearest / LinearMipmapNearest / NearestMipmapLinear / LinearMipmapLinear), which are correctly split into wgpu's `min_filter` and `mipmap_filter`. The glTF `sampler` object's wrapS / wrapT / magFilter / minFilter are read per-texture, and the viewer GPU side uses a `HashMap<IrSamplerInfo, wgpu::Sampler>` cache to share samplers. Bind group(3) assigns individual samplers per texture, fully compliant with glTF's per-texture sampler model. CPU-side sampling (`sample_image_g_channel`) also applies wrap mode-aware UV transformation. Both the base color texture (`base_color_tex_info`) and all MToon auxiliary textures use the `resolve_mtoon_uv()` helper for unified texCoord selection + KHR_texture_transform application. Non-MToon materials also apply `resolve_mtoon_uv()` to `baseColorTexture` / `emissiveTexture` for `texCoord` / `KHR_texture_transform` support. UV Animation targets (baseColor / shade / rim / outline_width / emissive / normalTexture) and non-targets (shift / uv_mask / matcap) are distinguished per spec. When `KHR_texture_transform.texCoord` is present, it takes priority over the TextureInfo-level `texCoord` (glTF spec compliant). When a texture requires `texCoord=1` but the mesh has no `TEXCOORD_1`, both GPU and CPU sides fall back to `Vec2::ZERO` (per UniVRM `MeshData.cs`). After extraction, UV1 presence is checked per-mesh and all textures (including `base_color_tex_info`) on materials referenced by UV1-absent meshes have their `texCoord=1` normalized to `texCoord=0`, preventing UV set divergence between tangent generation and rendering. Texture replacement via UI also recreates samplers from the material's `IrSamplerInfo`, preserving `ClampToEdge` / `Nearest` and other per-texture sampler settings.

#### Texture Index Normalization

In glTF, `textures[]` and `images[]` are separate arrays, and `TextureInfo.index` refers to a texture index. Since `IrModel.textures` is built by image array order, `read_texture_info()` normalizes glTF texture indices to **image indices** via `document.textures().nth(i).source().index()` before storing in `IrTextureInfo.index`. This ensures all downstream consumers (viewer bind groups, export_filter pruning, merge offset) safely operate on image indices. VRM 0.0 `_OutlineWidthTexture` is similarly resolved to image index. `texCoord >= 2` is unsupported; an error is logged and the texture is disabled (`None` is returned) to prevent silent misrendering. Texture references previously set via core glTF API are also explicitly cleared by the raw JSON result, ensuring fail-close behavior.

### UV Animation

Cumulative `time` is added to `CameraUniform`, and the shader transforms texture UVs every frame.

```wgsl
// Spec-compliant order: scroll → pivot(-0.5) → rotation → pivot(+0.5)
// UniVRM: vrmc_materials_mtoon_geometry_uv.hlsl — rotate(uv + translate - pivot) + pivot
let translate = vec2(time * scroll_x, time * scroll_y) * mask;
// Wrap within 2π period to prevent float precision degradation during long runtime (UniVRM-compliant)
let tau = 6.28318530718;
let turns = (time * uv_anim_rotation * mask) / tau;
let angle = fract(turns) * tau;
let centered = (uv + translate) - vec2(0.5);
anim_uv = vec2(centered.x * cos(angle) - centered.y * sin(angle),
               centered.x * sin(angle) + centered.y * cos(angle)) + vec2(0.5);
```

- UV Animation calculation uses the shared `apply_uv_anim_core()` function for both main and outline shaders. Hoisted before the MToon branch to also apply to normal maps
- Rotation angle is wrapped via `fract(turns) * 2π` to prevent float precision degradation during long runtime (UniVRM-compliant)
- Application order: scroll → rotation (per VRM spec: `scroll → pivot → rotation → pivot back`)
- `uvAnimationMaskTexture` controls application area (0.0–1.0) (channel: VRM 1.0=B, VRM 0.x=R, dynamically selected via `ColorChannel` enum)
- Affected textures: baseColor / shadeMultiply / **shadingShiftTexture** / rimMultiply / outlineWidthMultiply / emissive / **normalTexture** UV coordinates (UniVRM-compliant: all textures use `GetMToonGeometry_Uv()`-applied UV; matcap excluded)

### Transparent Draw Order Control (alphaMode / transparentWithZWrite / renderQueueOffsetNumber)

MToon spec-compliant 4-phase render queue controls draw order.

#### AlphaMode

`AlphaMode` enum unifying glTF `alphaMode` and MToon `transparentWithZWrite`:

| AlphaMode | glTF alphaMode | transparentWithZWrite | depth write | Description |
|-----------|---------------|----------------------|-------------|-------------|
| Opaque | OPAQUE | — | on | Fully opaque |
| Mask | MASK | — | on | alphaCutoff-based discard |
| BlendWithZWrite | BLEND | true | on | Transparent + depth write |
| Blend | BLEND | false | off | Standard transparent |

#### Draw Order

```
1. OPAQUE (depth write on)
   → outline rendering
2. MASK (depth write on, alphaCutoff discard)
   → outline rendering
3. BlendZWrite (depth write on, alpha blend)
   → outline rendering
4. Blend (depth write off, alpha blend)
   → outline rendering (ZWrite OFF)
```

- MASK pipeline: `alpha_to_coverage_enabled = true` (when MSAA active) reduces jaggies at cutout boundaries. Equivalent to UniVRM `MToonValidator.cs` `UnityAlphaToMask = On`. The MASK outline pipeline (`pipeline_outline_mask`) also enables AlphaToCoverage, ensuring consistent edge quality between surface and outline

Within each category, materials are stable-sorted by `renderQueueOffsetNumber`. Only effective for BLEND modes (Opaque/Mask forced to 0). BlendZWrite clamped to `[0, +9]`, Blend clamped to `[-9, 0]` (per UniVRM MToonValidator). Additionally, `RenderQueue::Blend` / `RenderQueue::BlendZWrite` materials with the same `renderQueueOffsetNumber` are sorted back-to-front by camera distance (`distance_squared`) to improve depth ordering for overlapping transparent meshes. Distance keys are recalculated from `current_vertices()` every frame during animation (opaque draws retain build-time fixed centroids).

BLEND / BlendZWrite phases issue surface and outline draws interleaved per draw call (since ZWrite OFF means draw order = compositing order). OPAQUE / MASK phases retain the traditional 2-pass structure as depth buffer protection is sufficient.

#### alphaMode Shader Processing

The `MaterialUniform.alpha_cutoff` field encodes alphaMode using sentinel values, with branching in the fragment shader:

| alphaMode | sentinel value | condition |
|-----------|---------------|-----------|
| OPAQUE | `-1.0` | `< -0.75` |
| BLEND | `-0.5` | `-0.75` ≤ x `< -0.25` |
| MASK | `>=0.0` (actual cutoff) | `>= -0.25` |

```wgsl
// alphaMode processing (alpha_cutoff encoding: <-0.75=OPAQUE, >=-0.25=MASK, else=BLEND)
if material.alpha_cutoff < -0.75 {
    // OPAQUE (-1.0): ignore alpha, always fully opaque
    out_alpha = 1.0;
} else if material.alpha_cutoff >= -0.25 {
    // MASK (>=0.0): discard below cutoff, then force opaque
    if out_alpha < material.alpha_cutoff { discard; }
    out_alpha = 1.0;
} else {
    // BLEND (-0.5) / BlendZWrite: discard fully transparent pixels (prevent depth pollution)
    if out_alpha <= 0.001 { discard; }
}
```

- OPAQUE / MASK: Output alpha fixed to 1.0, preventing unintended transparency from texture alpha values
- BLEND / BlendZWrite: `discard` of fully transparent pixels prevents depth buffer pollution (avoids invisible pixels from `transparentWithZWrite` occluding subsequent meshes)

#### Pipelines

| Pipeline | cull | depth write | Purpose |
|----------|------|-------------|---------|
| cull / no_cull | Back / none | on | OPAQUE / MASK |
| alpha_zwrite_cull / alpha_zwrite_no_cull | Back / none | on | BlendZWrite |
| alpha_cull / alpha_no_cull | Back / none | off | Blend |
| outline | Front | on | MToon outline (OPAQUE / BlendZWrite). With depth bias (UniVRM `Offset 1, 1` equivalent) |
| outline_mask | Front | on | MToon outline (MASK). With depth bias + AlphaToCoverage |
| outline_blend | Front | off | MToon outline (Blend). With depth bias |

## Bloom Post-Effect (v0.2.18)

### Dual Kawase Algorithm

Dual Kawase (Dual Filtering) bloom implemented in `bloom.rs` (~500 lines). Alternates between downsample and upsample passes to achieve wide-area blur at low cost.

1. **Brightness extraction**: Extract pixels above threshold from emissive buffer
2. **Downsample**: 3–6 progressive half-resolution passes (Kawase filter kernel)
3. **Upsample**: Reverse-order upscale with additive blending
4. **Final composite**: Add bloom result to scene color with intensity factor

### MRT (Multiple Render Target) Emissive Separation

The render pass is split into mesh drawing (MRT with 2 targets) and overlay drawing (1 target). The mesh drawing pass outputs scene color at `@location(0)` and emissive component at `@location(1)`. Grids and non-emissive surfaces write zero to `@location(1)`, so they are excluded from bloom.

Bloom intermediate buffers use `Rgba8Unorm` (linear) to avoid arithmetic artifacts from sRGB texture formats.

### UI Parameters

| Parameter | Range | Default | Description |
|-----------|-------|---------|-------------|
| ON/OFF | — | OFF | Enable bloom. When disabled, bloom pass execution is skipped (MRT 2-target rendering remains active; only additional bandwidth cost) |
| Intensity | 0.0–4.0 | 0.8 | Bloom brightness |
| Threshold | 0.0–1.0 | 0.0 | Cuts emissive below this luminance |
| Radius | 3–6 | 4 | Downsample stages. Larger = wider blur |

### Per-Material Bloom/Emissive Toggle (v0.2.19)

`bloom_per_mat: Vec<bool>` controls Bloom/Emissive ON/OFF per material.

- **glTF materials**: When `bloom_per_mat[i]` is false, `MaterialUniform.emissive_factor` is zeroed and `has_emissive_tex` is set to false. Both the shader's `lit += emissive` and `out.bloom = vec4(bloom_color, ...)` become zero
- **PMX/PMD materials**: When `bloom_per_mat[i]` is false, `MmdMaterialUniform.bloom_emissive` is zeroed
- **HDR auto-detection**: Materials with any `emissive_factor` component exceeding 1.0 are initialized with default OFF (`default_bloom_per_mat()`). Prevents white-out in the viewer which lacks tonemapping

### PMX/PMD Self-Emissive Material Bloom Detection

`derive_pmx_bloom()` common function detects self-emissive PMX/PMD materials:

- **Condition**: `specular == (0, 0, 0)` and `specular_power >= 100`
- **Bloom intensity**: `(specular_power - 100) / 10` (sp=110 equals VRM emissive=1.0)
- `bloom_emissive` is output only to MRT `@location(1)` (not added to scene color)
- Added `bloom_emissive` field to `MmdMaterialUniform`, referenced by shaders
- Emissive values are clamped to 0.0–1.0 (Rgba8Unorm MRT saturation avoidance)
- 6 unit tests validate detection logic and clamping

### Prefab Emission Support

Added `m_Colors` section and `m_ShaderKeywords` / `m_ValidKeywords` parsing to the `.mat` file parser.

- Auto-assigns `_EmissionColor` / `_EmissionMap` textures
- Emission enabled by priority:
  1. `_Emission` float if explicitly present
  2. `_EMISSION` keyword in `m_ShaderKeywords` / `m_ValidKeywords`
  3. `_EmissionMap` texture present
  4. `_EmissionColor` non-black and non-white (white excluded as default in many shaders)
- When `_EmissionMap` is present but `_EmissionColor` is black, emissive_factor corrected to white (1,1,1) to avoid shader 0 × texture = 0
- `m_ShaderKeywords` / `m_ValidKeywords` supports both YAML inline format (space-separated string) and multi-line list format (`- _EMISSION`)
- Added `emission_texture_guid` / `emission_color` / `emission_enabled` fields to `ResolvedMaterialTextures`

## Viewer Display Styles

### Dark Theme (v0.2.15)

Blender / Substance Painter style dark theme applied every frame via `setup_dark_theme()`. Each panel (top bar, side panel, status bar) uses explicit `egui::Frame::new().fill().stroke()` to bypass egui's default panel frame generation.

| Element | Color |
|---------|-------|
| Panel background | `#1D1D1D` |
| Section header | `#2A2A2A` |
| Widget background | `#252525` |
| Border | `#333333` |
| Accent (selection/hover) | `#4A90D9` |
| Active (pressed) | `#2A5A8A` |
| Text | `#FFFFFF` (`override_text_color`) |

Notes:
- `Button::fill()` overrides all states (inactive/hovered/active) — do not use. Hover color is controlled by global `widgets.hovered`
- `Button::stroke()` similarly overrides hover border color — do not use
- Side panel width fixed with `width_range(280.0..=280.0)` + `resizable(false)`

### VRM Meta Info Color Badges (v0.2.20)

Permission and license values are displayed as colored badges using `egui::RichText::background_color()`. This approach is used because egui's default font lacks color emoji glyphs.

| Badge Type | Background | Foreground | Usage |
|-----------|------------|------------|-------|
| Allow | `#206020` | `#80FF80` | Permitted / unrestricted (allow, Everyone, CC0, CC_BY, etc.) |
| Warn | `#605010` | `#FFE060` | Conditional (OnlyAuthor, personalProfit, CC_BY_NC, etc.) |
| Deny | `#601818` | `#FF8080` | Prohibited (disallow, prohibited, Redistribution_Prohibited, etc.) |
| Neutral | `#404040` | `#A0A0A0` | Neutral (unnecessary, Other) |

The data layer (`ir.comment`) retains English labels for PMX comment field output. Japanese labels are applied only at UI display time via `meta_section_ja()` / `meta_label_ja()`, with tooltips and badges from `meta_label_tooltip()` / `format_meta_value()`.

### Splash Image (v0.2.20)

Displays a logo image centered in the viewport when no model is loaded.

- PNG embedded in the exe via `include_bytes!("../../../assets/popone_image.png")`
- `image::load_from_memory` → `egui::ColorImage` → `ctx.load_texture` for egui texture registration
- Auto-scaled to fit viewport with `min(width_ratio, height_ratio, 1.0)`, centered via `Rect::from_center_size`
- Rounded corners via `egui::Image::corner_radius(CornerRadius::same(16))` (shader-level masking)
- Placed using `viewport.put(img_rect, image)` for explicit layout positioning

### Bone Display

- Shape: Double circle + triangle without base (◎△)
- Rendering: 1px LineList (`pipeline_line_overlay`)
- Color: Normal bone = blue `#0000ff`, IK bone = orange `#ff9600`
- Size: Scales with camera distance (constant screen size)
- IK detection: Whether bone name contains "ＩＫ" or "IK"

### Rigid Body Display

- Rendering: 1px LineList
- Color (PMX/PMD): By `physics_mode` — bone-follow(0) = green `#00ff00`, physics(1) = red `#ff0000`, physics+bone(2) = blue `#0080ff`
- Color (VRM): By `group` — collider(group=1) = red `#ff0000`, spring(group!=1) = green `#00ff00`
- Sphere: 8 meridians (great circle arcs) + 7 parallels
- Capsule: Top/bottom equator rings + 8 connecting lines + hemisphere wireframes (4 meridians + 3 parallels × top/bottom, PMX/PMD only)
- Box: 12 edges (size treated as half-extent)

### Joint Display (PMX/PMD only)

- Shape: Unit cube (faces = yellow `#ffff00`, edges = 1px black lines)
- Size: 0.18 PMX units
- Rotation: Euler YXZ intrinsic (= ZXY extrinsic) → Quat for pose reflection
- Animation sync: Follows via offset from rigid_a's bone
- Opacity: Adjustable via slider

### Wireframe Draw Modes

- `DrawMode` enum: `Solid` / `Wireframe` / `SolidWireframe`
- **Solid**: Normal solid rendering (`PolygonMode::Fill`)
- **Wire**: All meshes drawn with `pipeline_wireframe` (`PolygonMode::Line`, cull_mode=None). Outline rendering (`pipeline_outline*`) and MMD edge rendering (`pipeline_mmd_edge`) are skipped. MMD materials also switch to wireframe pipeline (using standard bind group layout)
- **S+W**: Solid rendering followed by wireframe overlay (`pipeline_wire_overlay`, depth bias -2 to avoid Z-fighting, semi-transparent black)
- Wire / S+W disabled when GPU feature `POLYGON_MODE_LINE` is unavailable (hidden in UI)
- "Outline drawing" checkbox is only enabled when `RenderStyle::Standard` draws with MToon outlines exist. Grayed out for PMD/PMX (`RenderStyle::Mmd`)

### Normal Map Display

- In-shader normal vector → RGB conversion: `rgb = (normalize(normal) + 1.0) * 0.5`
- Toggled via CameraUniform's `show_normal_map` flag

### Normal Map Tangent Space (TBN)

- Vertex tangent stored as `IrVertex.tangent: Vec4` (xyz=direction, w=handedness ±1)
- If glTF `TANGENT` attribute is present, it is skinning-transformed and used directly; otherwise, MikkTSpace tangents are auto-generated via `mikktspace` crate (VRM spec: TANGENT is not exported, compute MikkTSpace on import)
- MikkTSpace generation uses the UV set corresponding to `normalTexture.texCoord` (generates from UV1 when texCoord=1 and UV1 is available)
- MikkTSpace corner tangent handling: `set_tangent_encoded()` output is stored per-corner (`face * 3 + vert`). When corners sharing the same vertex have differing `tangent.w` (handedness), minority corners are automatically split into new vertices (indices / morph targets / UV1 updated accordingly). After splitting, xyz values within the same w-group are averaged and normalized into the vertex tangent
- Imported tangent degeneration detection: After Gram-Schmidt re-orthogonalization of skinning-transformed glTF TANGENT attributes, if `t_ortho` length falls below threshold or is non-finite, it is reset to `Vec4::ZERO` to route through MikkTSpace regeneration. Tangent validity is checked via `xyz.length_squared() > 1e-8` (not exact `Vec4::ZERO` match — degenerate tangents with non-zero w like `[0,0,0,1]` are also regenerated)
- The viewer's coordinate transform (VRM 1.0: Z-flip, VRM 0.0: X-flip) is a mirror transform with determinant -1. Since `cross(M*N, M*T) = det(M) * M * cross(N,T) = -M * cross(N,T)`, `tangent.w` must be negated to preserve bitangent direction
- Shader TBN construction (per UniVRM `MToon_GetTangentToWorld()`):
  - Zero tangent guard: if `dot(tangent.xyz, tangent.xyz) < 1e-6`, skip normal map and return base normal
  - `T = normalize(tangent.xyz)`
  - `tangent_sign = tangent.w > 0 ? 1.0 : -1.0` (binarized to avoid interpolation NaN)
  - `B = normalize(cross(N, T) * tangent_sign)`
  - `normal_ws = T * sample.x * scale + B * sample.y * scale + N * sample.z`
- Same logic applied to both main and outline shaders
- Skinning TBN sync: `animation.rs` transforms tangent.xyz alongside normals using the skinning matrix, then applies Gram-Schmidt re-orthogonalization (`t' = normalize(t - n * dot(n, t))`) to maintain orthogonality with the normal. tangent.w (handedness) is preserved
- Normal recalculation TBN sync: When `smooth_normals` / `clear_custom_normals` modifies normals, all vertex tangent.xyz are Gram-Schmidt re-orthogonalized against the new normals
- Normal smoothing + normal map compatibility (v0.2.19): Normal maps perturb normals via the TBN matrix (built from vertex normals + tangents), so faceted base normals make polygon edges visible. Using `[S]` to smooth base normals and `[N]` to apply normal maps produces smoother results. The `mat.normal_texture.is_none()` guard in `mesh.rs` has been removed, allowing smoothing on normal-mapped materials
- Per-material normal map toggle `[N]` (v0.2.19): Controlled by `normal_map_per_mat: Vec<bool>`. When OFF, `MaterialUniform.has_normal_tex` is set to 0.0, causing the shader's `if material.has_normal_tex > 0.5` branch to skip normal map sampling
- Morph normal/tangent tracking: `IrMorphTarget` holds `normal_offsets` / `tangent_offsets` in sparse representation (threshold 1e-7) alongside `position_offsets`. GPU morph application (`apply_gpu_morph_recursive`) adds weight × delta to position, normal, and tangent. tangent.w (handedness) is preserved. Normal and tangent deltas are correctly propagated through A-stance conversion (`pose.rs`), vertex splitting (`tangent.rs`), and export filter (`export_filter.rs`)
- NORMAL/TANGENT-only morph support: Morph targets with only NORMAL/TANGENT deltas (no POSITION, legal per glTF 2.0) are supported end-to-end. `IrMorph` generation, export filter liveness check, and GPU morph conversion all collect affected vertices as the union (`BTreeSet`) of positions/normals/tangents
- Morph CPU vertex sync: `apply_morphs()` updates `animated_vertices` (CPU-side cache) alongside the GPU buffer. `current_vertices()` returns morphed vertices even on morph-only frames, ensuring accurate transparent distance sorting
- VRM morph bind duplicate vertex accumulation: When a single Expression has multiple morph target binds where different morph targets affect the same vertex (e.g., mouth_a and mouth_small sharing lip vertices), offsets are accumulated via `HashMap::entry().or_insert() += off`. Common to VRM 0.0 / 1.0. PMX export also merges same-vertex offsets and removes zero-result entries
- VRM 0.0 zero-weight bind filtering: VRM 0.0 BlendShapeGroup skips `weight=0` binds (matching VRM 1.0 behavior). Handles models that include all morph targets in every bind (disabled via weight=0), preventing spurious zero-offset entries
- Default `min_filter` for unspecified glTF samplers is `LinearMipmapLinear` (per UniVRM `SamplerParam.Default`: Bilinear + mipmap enabled)

### Render Order

Items drawn later appear in front:

1. Normals (farthest back)
2. Bones
3. Rigid bodies
4. Joints (frontmost)

## Camera & Lighting

### Camera

| Item | Value |
|------|-------|
| FOV | 30° (MMD-compliant) |
| Projection | Perspective (default) / Orthographic (5 key toggle) |
| Controls | Left drag: rotate, Right/Middle drag: pan, Scroll: zoom |
| Precision | Shift key for 1/3 speed |
| Fit | F / Double-click (preserves yaw/pitch), R (front reset) |

### Fit Calculation (compute_fit)

Projects bbox 8 corners onto current camera view axes (right / up / forward) to compute projected half-width, half-height, and half-depth.

```
distance = max(half_h / tan(effective_fov_y), half_w / tan(fov_x)) + depth_offset
```

- `depth_offset`: `half_depth` in perspective (front-face frustum constraint), 0 in orthographic
- `effective_fov_y`: Effective FOV after subtracting UI overlay (60px)
- `fov_x`: Computed as `atan(tan(fov_y) * aspect)`
- Final distance multiplied by `FIT_MARGIN = 1.15` (15% padding)

### Lighting

| Mode | Direction |
|------|-----------|
| Fixed (default) | `Vec3(0.5, 1.0, -0.5).normalize()` — MMD-compliant (inversion of (-0.5,-1.0,0.5)) |
| Camera-Follow | `(forward + right*(-0.3) + up*0.7).normalize()` — MMD-style upper-left bias |

| Parameter | Default |
|-----------|---------|
| light_intensity | 0.7 |
| light_color | `[1.0, 1.0, 1.0]` (white) |
| ambient_intensity | 0.5 |
| ambient_sky_color | `[1.0, 1.0, 1.0]` (white) |
| ambient_ground_color | `[0.6, 0.55, 0.5]` (warm dark) |

Direct light is computed as `light_intensity * light_color`.

#### Hemisphere Ambient

Ambient light uses a hemisphere model interpolating Sky/Ground colors by the normal Y component:

```
hemi_t = normal.y * 0.5 + 0.5
passthrough_gi = mix(ambient_ground, ambient_sky, hemi_t)
```

`gi_equalized` (UniVRM's `uniformedGi`) is CPU-precomputed as `(sky + ground) / 2`. Approximates the L1 component of SH9 (vertical brightness gradient), closely matching VRoidHub / UniVRM's `SampleSH(normal)`.

### MMD Ambient Separation

The `mmd_ambient_scale` field in CameraUniform separates ambient light between standard and MMD paths:

- MMD mode ON: `mmd_ambient_scale = (154.0 / 255.0) × (light_intensity / 0.7)`
- MMD mode OFF: `mmd_ambient_scale = ambient_intensity` (UI slider value)

Inside the MMD shader, `mmd_light = vec3(mmd_ambient_scale) × light_color` is computed as a common light vector, used for AmbientColor / SpecularColor calculations matching the original MMD:

```
AmbientColor = clamp(diffuse_rgb × mmd_light + ambient, 0, 1)
SpecularColor = specular × mmd_light
```

Standard shaders use `camera.ambient` / `camera.ambient_ground` (hemisphere ambient) and `camera.light_color`. In MMD mode, scene ambient is subsumed by LightAmbient, so the ambient UI (ambient intensity, Sky color, Ground color) is grayed out. Brightness and color tone can be controlled via light color and intensity settings.

## Log Output

During CLI conversion, a `.log` file is generated in the same directory as the output (not generated with `--dump`).
stderr outputs logs at or above the level specified by `--log-level` (default: `info`),
while the log file records all entries down to `debug` level.

### Overall Log Structure

The conversion process outputs logs in the following order, centered on `build_pmx_model()`.

```
=== PMX Model Build Start ===        ← INFO: Model name, VRM version
Input VRM: bones=N, meshes=N...      ← INFO: Input statistics summary
--- Mesh List ---                     ← DEBUG: Vertex count, face count, material idx per mesh
--- Texture List ---                  ← DEBUG: Filename, MIME, data size
--- Material List ---                 ← DEBUG: Diffuse, texture, double-sided, MToon, edge
Materials: N (MToon=N, double-sided=N...)  ← INFO: Material statistics
--- Face Count by Material ---        ← DEBUG: Face vertex count per material
Vertex weight distribution: ...       ← DEBUG: Vertex count distribution of BDEF1/BDEF2/BDEF4
--- Morph List ---                    ← DEBUG: Panel, type, target count per morph
--- Rigid Body List ---               ← DEBUG: Shape, bone, group, physics mode per rigid body
--- Joint List ---                    ← DEBUG: Connected rigid bodies, position per joint
=== insert_standard_bones ===         ← DEBUG: Standard bone insertion (steps 1-18)
=== Post-Sort Bone List ===           ← DEBUG: Final bone order after topological sort
--- Display Frames ---                ← DEBUG: Bone count, morph count per display frame
=== PMX Model Build Complete ===      ← INFO: Output PMX statistics summary
```

### Panic Log

On panic, the current log file (`popone_yyyymmdd_hhmmss.log`) is copied to `panic_yyyymmdd_hhmmss.log`. Files with the `panic_` prefix are excluded from log rotation (`rotate_logs`) cleanup, so they persist until manually deleted.

## Single Instance

When the viewer is already running and launched again, the file path is forwarded to the existing window and the new process exits. Windows only (`#[cfg(target_os = "windows")]`).

- **Detection**: `Local\popone_viewer_single_instance` Named Mutex detects existing process
- **Communication**: `\\.\pipe\popone_viewer_ipc` Named Pipe (MESSAGE mode) sends file path as UTF-8
- **Reception**: Background thread listens → `mpsc::channel` → `update()` feeds into `pending.load`
- **Focus**: `ViewportCommand::Minimized(false)` + `Focus` (restores from minimized state)
- **Path normalization**: `std::fs::canonicalize()` before sending (CWD difference mitigation)
- **Log preservation**: `InstanceCheck` tri-state (`Primary` / `Forwarded` / `FallbackStart`) skips log rotation when existing instance detected

## FPS Measurement

Displays FPS and frame time (ms) in the viewport top-right overlay.

- **Method**: Frame counting (computes `FPS = (frame_count - 1) / time_span` from `VecDeque<Instant>` over the last 1 second)
- **Update interval**: 0.5 seconds (flicker prevention)
- **ms display**: Average frame time within the window (consistent with FPS value)

## Animation Playback

The viewer supports real-time playback of VRMA / glTF / FBX animations.

### Pose Reset on Animation Clear (v0.2.20)

When clearing or removing an animation, the following reset steps are performed:

1. Reset animation-controlled expression morph weights to 0.0
2. Invalidate morph cache via `GpuModel::invalidate_morph_cache()`
3. On the next frame, `apply_morphs` rebuilds vertices from `base_vertices`, fully resetting bone deformations

Since `apply_morphs` has an early-return optimization using `last_weights`, the `morph_cache_dirty` flag forces recalculation.

### Supported Formats

| Format | Loading | Retargeting | Notes |
|--------|---------|-------------|-------|
| VRMA (`.vrma`) | `vrm::animation::load_vrma` | Humanoid normalized coordinate system | VRM Animation spec compliant. Model-to-model conversion via bone_rests |
| glTF / GLB | `vrm::animation::load_gltf_animation` | Humanoid node name matching | Multiple animations supported |
| FBX (`.fbx`) | `fbx::animation::load_fbx_animation` | PreRotation composition / coordinate transform | AnimationStack → Layer → CurveNode → Curve hierarchy analysis |
| Unity .anim | `unity::animation::load_unity_anim` | Muscle → SwingTwist conversion | Hidden feature (D&D only) |

### Animation Playback for PMX/PMD

When applying VRMA animation to PMX/PMD models, bone name mapping via `pmx_name_to_vrm_bone()` is used. Key mappings:

| PMX Bone Name | VRM Humanoid Name |
|---------------|-------------------|
| Center (センター) | hips |
| Upper body (上半身) | spine |
| Upper body 2 (上半身2) | chest |
| Neck (首) | neck |
| Head (頭) | head |
| Left/Right arm (左腕 / 右腕) | leftUpperArm / rightUpperArm |
| Left/Right elbow (左ひじ / 右ひじ) | leftLowerArm / rightLowerArm |
| Left/Right leg (左足 / 右足) | leftUpperLeg / rightUpperLeg |
| (Plus fingers, shoulders, eyes, etc. — 55 bones total) | |

### Humanoid Retargeting

VRMA and glTF humanoid animations are retargeted to apply correctly even when source and target models have different rest poses, using the following formula:

```
normalized = W_src × L_src⁻¹ × anim_rot × W_src⁻¹
local_rot  = L_dst × W_dst⁻¹ × normalized × W_dst
```

- `W_src`, `L_src`: Source (VRMA) global/local rest pose rotation
- `W_dst`, `L_dst`: Target (VRM model) global/local rest pose rotation
- `anim_rot`: Local rotation value specified by the animation

### FBX Animation Coordinate Transformation

FBX animations are converted to glTF coordinate system through the following steps:

1. **GlobalSettings**: Build axis conversion matrix (identity for Y-Up)
2. **Euler rotation**: ZYX extrinsic (= XYZ intrinsic), `Quat::from_euler(EulerRot::ZYX, rz, ry, rx)`
3. **PreRotation composition**: Apply `PreRotation × euler_to_quat(Lcl Rotation)` to keyframes
4. **Facing detection**: Left-side bone global X coordinate is positive → +Z facing → Y180 correction needed
5. **Y180 correction**: Rotation `Quat(-x, y, -z, w)`, translation delta `Vec3(-dx, dy, -dz)`
6. **Time unit**: FBX 1 second = 46186158000

### Unity .anim Muscle Conversion (Hidden Feature)

Conversion from Unity Humanoid Muscle values to bone rotations. Implemented as a hidden feature due to limited stability.

#### SwingTwist Decomposition

Construct rotation from Muscle's 3 DOF (twist, swing_y, swing_z):

```
SwingTwist(x, y, z) = AngleAxis(|yz|, normalize(0, y, z)) × AngleAxis(x, (1,0,0))
```

- Twist: Rotation around X axis
- Swing: Swing in YZ plane

#### Bone Rotation Formula

```
localRotation = preQ × SwingTwist(sign × degrees) × postQ⁻¹
```

- `preQ`, `postQ`: Avatar-specific reference rotations (preQ == postQ for normalized skeletons)
- `sign`: Per-bone sign `(±1, ±1, ±1)` (per V-Sekai `GetLimitSign`)
- `degrees`: Degrees scaled from Muscle value using angle range

#### Muscle Value → Angle

```
muscle ≥ 0: degrees = muscle × max_deg
muscle < 0: degrees = muscle × (-min_deg)
```

`min_deg`, `max_deg` use default values from `HumanTrait.GetMuscleDefaultMin/Max`.

#### Left-Handed → Right-Handed Conversion

- Quaternion: `(x, -y, -z, w)` (reverseX convention, UniVRM compliant)
- Vector: `(-x, y, z)`

#### RootQ / RootT

- RootQ: Delta from initial frame `delta = q0⁻¹ × qi`, applied as `rest × delta`
- RootT: Delta from initial frame (relative movement), applied as `rest_pos + delta`

#### Parameter Mode

When specifying a JSON file output by DumpHumanoidParams.cs, model-specific preQ / postQ / sign values are used for high-precision conversion. When unspecified, V-Sekai normalized skeleton fallback values are used.

### Loop Modes

| Mode | Description |
|------|-------------|
| None | Play once and stop |
| Normal | Loop back to the beginning at the end |
| A-B Repeat | Repeat a user-specified section |
| PingPong | Play back and forth |

## Model Append Loading

### Bone Merge 3-Level Fallback Method

When merging bones into the existing side with `IrModel::merge()`, candidates are determined using a 3-level fallback in order of reliability, with parent-child relationship consistency guaranteed regardless of order.

#### Problem

Matching by `bone.name` (PMX name) alone failed entirely when naming conventions differed between models (e.g., Japanese "下半身" vs English "Hips" for the same bone). Additionally, a 1-pass method could incorrectly merge descendants from different subtrees.

#### Solution: 3-Level Fallback + Parent Propagation

```
Pass 1a (vrm_bone_name match): Match by VRM humanoid bone name
  - VRM names are unique per skeleton, no parent check needed
  - Sets vrm confirmed flag (exempt from pass 2 cancellation)

Pass 1b (original_name match): Match by FBX node name with lowercase normalization
  - Only becomes merge candidate if parent is already matched or parent's original_name matches

Pass 1c (bone.name match): PMX name + same parent name check (existing behavior, backward compatible)

Pass 2 (propagation loop): Cancel candidates whose parent is not a candidate (skip vrm confirmed)
  while changed:
    for i in 0..N:
      if candidate[i].is_some() && !is_vrm_match[i] && parent's candidate is None:
        candidate[i] = None

Finalize: Merge bones with Some candidate, add bones with None candidate as new
```

#### Pre-Merge Humanoid Completion

When the appended model has no `vrm_bone_name` set (e.g., Unknown rig), `detect_humanoid` is re-run against `original_name` before merge to fill in `vrm_bone_name`. This improves Pass 1a matching accuracy.

Pass 2 iteration converges in worst case O(depth) times (at least 1 candidate is cancelled per iteration).

### ASCII FBX Content Block Processing

The `Video/Content` node in ASCII FBX stores embedded data in text representation such as base64. The line-oriented parser cannot analyze this as a regular child node (`:` delimited), so it uses special processing to read until `}` and store as `FbxProperty::String`.

```
Content: {
  <base64 encoded data lines...>
}
→ FbxProperty::String(joined_lines)
```

During texture extraction (`texture.rs`), retrieval is done via `as_binary()` only, so images are not decoded from ASCII FBX Content strings. Instead, recovery is done via external file fallback using `RelativeFilename` / `FileName`.

### FBX Parser Input Validation

To prevent OOM / stack overflow / infinite loops from malicious FBX files, `parser.rs` enforces the following limits:

| Limit | Constant | Value | Purpose |
|-------|----------|-------|---------|
| Property count | `MAX_NUM_PROPERTIES` | 1,000,000 | Prevent `Vec::with_capacity` OOM |
| Node recursion depth | `MAX_NODE_DEPTH` | 64 | Prevent stack overflow |
| Array size | `MAX_ARRAY_SIZE` | 512 MB | Prevent huge allocation |

Additional checks:
- `end_offset` range validation: error if not `cursor.position() < end_offset <= data_len`. Child node recursion passes parent's `end_offset` as boundary
- `array_len * element_size` uses `checked_mul` (prevents overflow wrap in release builds)
- `compressed_len` validated against remaining bytes before buffer allocation

#### FBX External Texture Nearby Search

When `RelativeFilename` / `FileName` paths don't match the actual directory structure (common with Unity/Blender project exports), `TextureSearchCache` is used to recursively search directories near the FBX file (max depth 3). The cache is a `HashMap` of filename (lowercase) → path, targeting only image file extensions (png/jpg/tga/bmp/dds/psd, etc.). Directory scanning runs only once per conversion.

### pkg Texture Namespace

When append-loading multiple UnityPackages, texture name collisions between packages can occur (e.g., both contain `body.png`).

#### Solution

Add a package-specific prefix to texture names during append:

```
{package_filename(no_extension)}_pkg{append_sequence}_{original_texture_name}
e.g.: outfit_pkg1_body.png
```

- **auto-matched textures**: After `embed_textures_into_ir` loads textures into `IrModel`, the `filename` of textures added after merge also receives the prefix (`loaded.ir.textures[tex_count_before..]`)
- **manually assigned textures**: Prefix is added when `extend`ing into the `pkg_textures` Vec. The `pkg_assignments` HashMap naturally achieves uniqueness by using prefixed names as keys
- **path separator avoidance**: Do not use `/` in the prefix (since `IrTexture.filename` is used as PMX export file paths)

## Direct Archive Loading

### archive Module

A unified API for detecting and extracting model files from ZIP / 7z archives.

#### 2-Stage API

| Function | ZIP | 7z | Description |
|----------|-----|-----|-------------|
| `list_models` | Metadata only | Full extraction of target extensions (streaming constraint) | Returns model list |
| `extract_model_bundle` | Extracts selected files only | Uses already-extracted entries | Returns model + textures/aux_files |

Due to `sevenz-rust2`'s streaming API constraint, 7z extracts all files with target extensions into memory at `list_models` time (`MAX_TOTAL_BYTES = 2GB` limit). Extracted entries are held in `ArchiveContents` and reused by `extract_model_bundle` without re-extraction.

#### PMX/PMD Texture Reference Resolution

For PMX/PMD, the model file is parsed to obtain texture reference paths, then matched against archive contents:

1. Exact match
2. Case-insensitive fallback
3. PMD basename-only match

Matched files are stored in `aux_files: HashMap<PathBuf, Arc<[u8]>>` with model parent directory-relative paths as keys.

#### Security

- **Path traversal defense**: `normalize_archive_path` rejects `..` and absolute paths
- **Shift_JIS filenames**: `name_raw()` → UTF-8 → Shift_JIS fallback (`enclosed_name()` not used due to CP437 misparse)
- **Zip bomb protection**: ZIP uses `take(limit)` for hard limits, 7z uses chunked reading to verify actual bytes read (`saturating_add` for overflow safety)
- **ZIP PMX/PMD budget**: Second `extract_files` call receives `remaining = MAX_TOTAL_BYTES - model_size`

### Viewer Integration

#### PendingArchive / PendingArchiveLoad

Same deferred loading pattern as `PendingUnityPackage` / `PendingPkgModelLoad`:

1. `try_load_archive` → `list_models` → 1 model: `pending_archive_load`, multiple: `pending_archive` (selection dialog)
2. `show_archive_select_dialog` (`ui.rs`) → selection → `pending_archive_load`
3. `update_progress_flags` → `shown = true` (overlay display)
4. Next frame → `load_model_from_archive` → `extract_model_bundle` → `build_ir_from_archive_bundle` → `finish_load`

#### Reload

`ReloadableSource::Archive` re-selects the same model via `selected_entry_path`. `load_ir_from_archive_source` is the shared function called from both `reload_from_source` and `append_model_from_source`.

#### Nested UnityPackage in Archives (Double Extraction)

Detects `.unitypackage` files inside ZIP / 7z and double-extracts to load inner VRM / FBX models.

1. `list_models` detects `.unitypackage` as `ArchiveModelKind::UnityPackage`
2. `extract_model_bundle` extracts only the `.unitypackage` body (sibling textures are not needed)
3. `load_unitypackage_from_archive` → `extract_all_assets` for tar.gz double extraction
4. Inner model selection → connects to existing `PendingPkgModelLoad` flow
5. `ReloadableSource::Archive { inner_kind: UnityPackage }` preserves source info
6. On reload, `reload_archive_unitypackage` re-extracts archive → re-extracts unitypackage → re-selects model via `selected_fbx_name`

Extraction size limit: Both the outer archive (`MAX_TOTAL_BYTES = 2GB`) and inner `.unitypackage` (same 2GB) are protected.

### CLI

`--list-models`: Lists models inside the archive and exits (no output required).
`--model-name`: 3-stage search (exact → prefix → substring match). Only unique matches are accepted at each stage; multiple candidates trigger an error with candidate list.

## Archive D&D Reload Support

### ReloadableSource enum

An enum that tracks the model's loading source. Solves the temp file reload problem.

| Variant | Description |
|---------|-------------|
| `File(PathBuf)` | Normal file path. Re-reads file on reload |
| `Snapshot { original_path, main_bytes: Arc<[u8]>, aux_files }` | Snapshot from temp file. Restores from memory on reload |
| `Archive { original_path, archive_bytes, selected_entry_path, inner_kind }` | Model inside archive. Re-extracts archive and re-selects same model on reload |

### Temp Path Detection

`is_temp_path()` checks whether the path is under `std::env::temp_dir()` using a two-stage approach:

1. **canonicalize-based** (when file exists): Normalizes via `canonicalize()`, absorbing symlink and drive letter case differences
2. **String-based fallback** (after file deletion): Normalizes case via `to_string_lossy().to_lowercase()`, ensures path boundary via `MAIN_SEPARATOR` suffix before `starts_with` comparison (prevents false positives like `TempBackup`)

The fallback is necessary to handle cases where temp files from zip archive D&D are immediately deleted.

### Immediate Load for Temp Paths

When `is_temp_path()` returns true in `process_drag_and_drop()`, `load_file()`/`append_model()` is called directly instead of going through `pending_load`/`pending_append`. This avoids the `os error 3` caused by temp files being deleted during the normal 2-frame delay (used for progress overlay display).

### D&D Preload Cache (PreloadedData)

When a temp path is detected in `process_drag_and_drop()`, the model body and adjacent file bytes are cached in `PreloadedData`, eliminating disk access throughout the entire load chain.

```rust
/// D&D temp file preload data
pub struct PreloadedData {
    path: PathBuf,          // Original temp file path
    main_bytes: Arc<[u8]>,  // Model body bytes
    aux_files: HashMap<PathBuf, Arc<[u8]>>,  // Adjacent image files (relative path keys)
}
```

#### Helper Methods

| Method | Description |
|--------|-------------|
| `read_or_preloaded(path)` | Returns from cache if `preloaded.main_bytes` or `aux_files` matches. Falls back to `std::fs::read` otherwise |
| `take_or_collect_aux(path)` | Moves `preloaded.aux_files` via take if matched. Falls back to `collect_image_files_recursive` for disk collection |

#### Data Passing Flow

```
process_drag_and_drop:
  1. std::fs::read(&model_path) → PreloadedData.main_bytes
  2. collect_image_files_recursive() → PreloadedData.aux_files
  3. self.preloaded = Some(PreloadedData { ... })
  4. Call load_file() / append_model()
  5. Clear self.preloaded = None if PendingFbxChoice is not set

FBX selection dialog path:
  load_file() → PendingFbxChoice { preloaded: self.preloaded.take() }
  → execute_fbx_choice() → self.preloaded = choice.preloaded (restore)
  → try_load_fbx() → read_or_preloaded() uses cache
  → self.preloaded = None (clear)
```

#### Usage by Format

| Method | main file | aux files |
|--------|-----------|-----------|
| `try_load_fbx` | `read_or_preloaded` | `take_or_collect_aux` → `ReloadableSource::Snapshot` |
| `try_load_vrm` | `read_or_preloaded` | Embedded (no external refs) |
| `try_load_pmx` | `read_or_preloaded` | `preloaded_aux` preferred → `std::fs::read` fallback |
| `try_load_pmd` | `read_or_preloaded` | `preloaded_aux` preferred → `std::fs::read` fallback |
| `try_load_unitypackage` | `read_or_preloaded` | Contained in archive |
| `try_load_fbx_animation` | `read_or_preloaded` → `load_fbx_animation_from_data` | N/A |
| `append_model` (FBX/PMX/PMD/VRM) | `read_or_preloaded` | N/A (IrModel construction only) |

### Auxiliary File Cache

| Format | aux_files Contents |
|--------|-------------------|
| VRM / GLB | Empty (textures embedded in binary) |
| FBX | Recursively collected adjacent image files (preserving subdirectory structure) |
| PMX | Texture files from `pmx.textures` paths |
| PMD | Textures + same-name `.txt` (material name text) |

FBX external textures are recursively scanned under the parent directory by `collect_image_files_recursive()`, with `strip_prefix(base_dir)` preserving relative paths as keys. On reload, subdirectory structure is restored via `create_dir_all` before passing to the FBX parser.

### TextureSource enum

Tracks the loading source of texture assignments. Value type for `TextureState.assignments`.

| Variant | Description |
|---------|-------------|
| `File(PathBuf)` | Normal file path |
| `Cached { original_name, data: Arc<[u8]>, is_psd }` | Cached from temp file. `Arc<[u8]>` reduces clone cost |

### reload_from_source

Bypasses `load_file()` UI branching (FBX mesh+animation selection dialog, etc.) and directly calls `try_load_*` from `ReloadableSource`. Returns `Result`; on failure, restores saved state and returns early.

### Texture D&D Preview Cache

When D&D'ing textures from ZIP archives, data is cached in `PendingTexPreview` to ensure texture assignments are correctly recorded even after temp files are deleted.

| Field | Type | Description |
|-------|------|-------------|
| `cached_data` | `Vec<u8>` | Byte data cached at file read time |
| `is_psd` | `bool` | Extension detection result (determined at read time) |
| `was_temp` | `bool` | Temp path detection result (`is_temp_path` evaluated **before** `std::fs::read`) |

#### Processing Flow

```
open_texture_preview:
  1. was_temp = is_temp_path(&path)    ← Determined while file exists (canonicalize prerequisite)
  2. data = std::fs::read(&path)       ← Read byte data
  3. upload_texture_from_bytes(&data)   ← Create GPU texture
  4. PendingTexPreview { cached_data: data, is_psd, was_temp, ... }

apply_tex_preview:
  1. tex_data = preview.cached_data.clone()  ← From cache (no re-read)
  2. is_psd = preview.is_psd                 ← From cache
  3. cached_data = if preview.was_temp { Some(...) } else { None }
  4. Branch to TextureSource::Cached or File
```

**Important**: `is_temp_path` evaluation must occur before `std::fs::read`. Since `canonicalize()` requires file existence, evaluating after read risks the file being deleted, causing the check to fail.

### UnityPackage Archive Snapshot

When D&D'ing .unitypackage from ZIP archives, archive data is snapshot-cached as `Arc<[u8]>`.

#### Struct Fields

| Struct | Added Field |
|--------|------------|
| `PendingUnityPackage` | `archive_snapshot: Option<Arc<[u8]>>` |
| `PendingPkgModelLoad` | `archive_snapshot: Option<Arc<[u8]>>` |
| `PendingFbxChoicePkg` | `archive_snapshot: Option<Arc<[u8]>>` |

#### Snapshot Generation Flow

```
try_load_unitypackage:
  1. is_temp = is_temp_path(path)      ← Evaluated before std::fs::read
  2. archive_data = std::fs::read(path)
  3. assets = extract_all_assets(&archive_data)
  4. snapshot = if is_temp { Some(Arc::from(archive_data)) } else { None }
  5. Store snapshot in PendingPkgModelLoad / PendingUnityPackage
```

#### Snapshot Propagation Path

```
try_load_unitypackage / try_load_unitypackage_for_append
  → Stored in PendingUnityPackage / PendingPkgModelLoad
    → Inherited in ui.rs show_fbx_select_dialog to PendingPkgModelLoad
      → Passed to load_fbx_from_assets / load_vrm_from_assets in process_pending_tasks
        → Builds ReloadableSource::Snapshot and passes to finish_load
          → Stored in LoadedModel.source
            → On reload_current, reload_unitypackage(&source, ...) restores from Snapshot
```

#### reload_unitypackage / reload_append_unitypackage Changes

Signature changed from `path: &Path` to `source: &ReloadableSource`. For the Snapshot variant, archive data is restored via `main_bytes.to_vec()`. For the File variant, `std::fs::read` is used as before.

### .gltf Exclusion

`.gltf` files have external buffer references (`.bin`, image files), so they are excluded from snapshotting. `gltf::import_slice` cannot resolve external URIs, so the normal `load_glb(path)` path is used.

## Prefab Texture Mapping (v0.2.16)

Automatically maps textures to FBX models by following Unity's GUID reference chain from `.prefab` files within `.unitypackage`.

### GUID Reference Chain

```
.prefab → m_SourcePrefab / m_Mesh (FBX GUID)
       → FBX .meta → externalObjects (material name → .mat GUID)
       → .mat → m_TexEnvs → _MainTex (texture GUID)
       → texture file
```

### UnityPackageIndex

GUID-based index structure providing O(1) lookup from GUID to pathname, data, and meta.

```rust
pub struct UnityPackageIndex {
    pub entries: Vec<AssetEntry>,
    pub by_guid: HashMap<String, usize>,
    pub by_path: HashMap<String, usize>,
}
```

`build_unity_package_index()` extracts the tar.gz once, then all subsequent lookups use `by_guid` / `by_path`.
Built for both direct viewer loading and archive (ZIP / 7z) loading paths.

### Prefab Format Detection

| Format | Detection | Example Packages |
|--------|-----------|-----------------|
| New | Standalone `PrefabInstance:` line exists | Shinano, FC_Milltina |
| Old | `--- !u!137` (SkinnedMeshRenderer) + `m_Mesh` only | Yukitsune no Oneesama |
| Unpacked | Old-style but contains `m_CorrespondingSourceObject` | Nekoyama, CHR_LML01 |
| Mixed | New (`PrefabInstance`) + Old (`m_Mesh`) coexist | SVST01_common |
| Variant | `m_SourcePrefab` references another `.prefab` | SVST01_01_VRC |

`detect_prefab_format()` uses line-level matching for `PrefabInstance:` (prevents false matches with `m_PrefabInstance:`).
For Mixed format, the Old parser always runs after the New parser to collect FBX references from both patterns.

### Prefab Variant Resolution

`resolve_variant_multi()` recursively follows Variant chains, collecting all referenced FBX GUIDs.
Cycle detection (`HashSet<String>`) and depth limiting (32 levels) prevent infinite loops.

`resolve_single_prefab()` delegates to `resolve_single_prefab_inner()` for recursive resolution,
handling nested Prefabs (where m_SourcePrefab points to a `.prefab` rather than `.fbx`).

### Three-Stage Texture Matching Fallback

1. **source_material** — `SourceMaterialRef` (renderer_path + slot_index) uniquely identifies the material slot in the FBX mesh
2. **material_name / fbx_material_name** — Matches using both `.mat` material name and FBX internal material name (from `.meta` `externalObjects`)
3. **source_texture_name** — Existing filename-based matching (fallback)

### Unity YAML Parsers

- `parse_prefab_new()` — 2-pass approach: `m_Modifications` then `m_SourcePrefab`
- `parse_prefab_old()` — Extracts `m_Mesh` + `m_Materials` from `--- !u!137` (SkinnedMeshRenderer) sections
- `parse_fbx_meta()` — Extracts material name → GUID mapping from `externalObjects`
- `parse_material_textures()` — Extracts main texture and normal map GUIDs from `m_TexEnvs`, and reads `_BumpScale` from `m_Floats`. Section transitions are safely managed via a `MatSection` enum. Slot priority: main=`_MainTex` > `_BaseMap` > `_BaseColorMap`, normal=`_BumpMap` > `_NormalMap`
- `decode_unity_escape()` — `\uXXXX` → Unicode conversion, YAML quote trimming

### Key Data Types

| Type | Purpose |
|------|---------|
| `PkgModelLocator` | Model selection key (GUID + pathname + kind) |
| `PkgModelListItem` | Model selection dialog display item |
| `PackageTexture` | GUID + display name + data bytes |
| `PreparedPkgFbx` | FBX data + textures + resolved materials |
| `ResolvedMaterialTextures` | Material name + main texture GUID + normal map GUID + bump_scale + fbx_material_name |
| `FbxResolveEntry` | Single FBX GUID + index + resolved materials |
| `PrefabResolveResult` | Entire Prefab resolution result (may contain multiple FBX) |
| `SourceMaterialRef` | renderer_path + slot_index (stable key for FBX mesh → material) |

### Per-FBX MaterialGroup Splitting from Prefab

During the `load_prefab_from_assets` merge loop, each FBX's material range `(name, mat_start, mat_count)` is tracked. After `finish_load()`, `gpu_model.draws` is scanned to compute `draw_range`, and the single `MaterialGroup` is split into per-FBX groups.

```
Prefab: Body.fbx(0..12 materials) + Hair.fbx(12..18 materials)
  → MaterialGroup[0] { name:"Body.fbx", material_range:0..12, draw_range:0..15 }
  → MaterialGroup[1] { name:"Hair.fbx", material_range:12..18, draw_range:15..20 }
```

### File Hierarchy Tree

The `show_file_tree()` function displays the load chain as a tree below the material display in the Display tab.

**Display Structure:**

| Load Method | Tree Structure |
|---|---|
| Direct VRM/FBX/PMX | `source.vrm` → textures |
| Archive (ZIP/7z) | `archive.zip` → `entry.vrm` → textures |
| UnityPackage (direct FBX) | `pkg.unitypackage` → textures |
| UnityPackage (Prefab) | `pkg.unitypackage` → `Prefab.prefab` → `Body.fbx` / `Hair.fbx` → textures |

Texture references are collected by `collect_material_tex_indices()`, which gathers all texture indices referenced by a material (base_color, normal, emissive, sphere, toon, 6 MToon types).

### Always-On Material Grouping

`material_groups` always contains at least one group, even for single models. The UI-side `has_groups` condition was changed to `!group_names.is_empty()` (always true), removing the flat list display path. Unified `CollapsingState`-based grouping is now used for all cases.

Group header rows use the layout `▶ [S] [C] [N] [B] [☑] GroupName`, implemented with `CollapsingState` + `ui.horizontal`. Button behavior:

| Button | Target | Behavior |
|--------|--------|----------|
| `[S]` | `smooth_normals_per_mat` | Batch toggle normal smoothing for all materials in the group (compatible with normal maps: smoothing TBN base normals improves polygon edge visibility) |
| `[C]` | `clear_normals_per_mat` | Batch toggle custom normal clear for all materials in the group (compatible with normal maps) |
| `[N]` | `normal_map_per_mat` | Batch toggle normal map application for normal-mapped materials. When OFF, `MaterialUniform.has_normal_tex` is zeroed, skipping normal map sampling in the shader |
| `[B]` | `bloom_per_mat` | Batch toggle Bloom/Emissive for emissive materials. When OFF, `emissive_factor` is zeroed, disabling both `lit += emissive` and MRT bloom output. HDR emissive (component > 1.0) defaults to OFF |
| `[☑]` | `material_visibility` | Batch toggle visibility for all DrawCalls in the group |

Header row hover detection uses `contains_pointer()` (rect-based). `hovered()` is not suitable because child widgets (buttons, etc.) consume the hover event.

### Prefab Reload (A/T Stance Conversion Support)

Toggling A-stance / T-stance conversion triggers `reload_current()` → `reload_unitypackage()`, but `reload_unitypackage()` only loads a single FBX, losing the Prefab's multi-FBX merge structure.

**Fix**: Added `prefab_entry_path: Option<String>` (pathname within pkg_index) to `LoadedModel`. When `reload_unitypackage()` / `reload_archive_unitypackage()` detects a Prefab model, it branches to `reload_as_prefab()`.

```
reload_current()
  → reload_unitypackage() / reload_archive_unitypackage()
    → prefab_entry_path present?
      → reload_as_prefab()
        1. Rebuild pkg_index via build_unity_package_index()
        2. Locate Prefab entry via by_path[prefab_entry_path]
        3. Re-execute multi-FBX merge via load_prefab_from_assets()
        4. Restore manual texture assignments via assign_texture_data_to_material()
```

`assign_texture_data_to_material()` can apply textures after GPU model construction (adds IrTexture + rebuilds bind group). For borrow checker compliance, restoration data is first collected into a `Vec<(usize, String, Vec<u8>)>` before application.

`reload_as_prefab` receives `archive_source: &ReloadableSource` and, when `snapshot` is `None` (archive loaded from a regular file, not a temp file), preserves the original `Archive` source. This ensures reloads correctly enter the `reload_archive_unitypackage` path and prevents ZIP files from being parsed as GLB.

### FBX Direct Selection: Prefab-Aware Reload

When an FBX is directly selected from a `.unitypackage` (not via Prefab), `load_fbx_from_assets` uses `pkg_index` to call `prepare_pkg_fbx` + `embed_textures_with_prefab` for Prefab-aware texture mapping. However, `prefab_entry_path` is not set, so reloads do not branch to `reload_as_prefab`.

**Problem**: `reload_unitypackage` uses `embed_textures_into_ir` (simple name matching), causing all textures to be lost on models where material names don't match texture names.

**Fix**: `reload_unitypackage` checks whether `loaded.pkg_material_keys` is non-empty (indicating Prefab-aware mapping was used during initial load). If so, it rebuilds `UnityPackageIndex` and uses the Prefab-aware path.

```
reload_current()
  → reload_unitypackage()
    → pkg_material_keys non-empty?
      → Yes: Prefab-aware path
        1. Rebuild pkg_index via build_unity_package_index()
        2. Look up FBX index in pkg_index from assets pathname
        3. Resolve Prefab textures via prepare_pkg_fbx()
        4. Embed textures via embed_textures_with_prefab()
        5. Rebuild pkg_material_keys after finish_load()
      → No: Legacy path (embed_textures_into_ir)
```

## Reload Texture Normalization

### reload_unitypackage Texture Restoration

When restoring manually assigned textures during UnityPackage reload, the same PSD→PNG conversion and MIME type settings as the normal path (`assign_texture_source_to_material`) are applied.

| Texture Format | Processing | ir_filename | mime_type |
|---------------|-----------|-------------|-----------|
| PSD | Convert to PNG via `psd_to_png()` | `{basename}.png` | `image/png` |
| PNG | As-is | Original filename | `image/png` |
| TGA | As-is | Original filename | `image/x-tga` |
| BMP | As-is | Original filename | `image/bmp` |
| Other | As-is | Original filename | `image/jpeg` |

On PSD→PNG conversion failure, `continue` skips the material assignment (consistent with normal path failure behavior).

`name_to_ir: HashMap<String, usize>` cache prevents duplicate IrTexture additions for the same texture name. Package texture names are guaranteed unique, so `tex_name` alone is sufficient as a key.

### IrTexture Deduplication in assign_texture_source_to_material

During manual texture assignment, existing IrTextures are searched by `filename + data.len() + data` exact match, reusing the index if found.

```rust
let tex_idx = loaded.ir.textures.iter()
    .position(|t| t.filename == ir_filename
        && t.data.len() == ir_data.len()
        && t.data == ir_data)
    .unwrap_or_else(|| { /* add new */ });
```

- `data.len()` is checked first so textures with different sizes are skipped in O(1)
- External filesystem assignments can have same-name-different-content files, so `data` is also compared (not just `filename`)
- The pkg restoration path uses `tex_name`-keyed cache for deduplication (package texture name uniqueness is guaranteed)

## Shader-Aware PMX Material Conversion

### select_toon()

Selects toon texture based on shade_color/diffuse luminance ratio for MToon materials. Uses Rec. 709 luminance coefficients `(0.2126, 0.7152, 0.0722)`.

| shade/diffuse Luminance Ratio | Toon | Description |
|-------------------------------|------|-------------|
| < 0.25 | Shared(0) = toon01 | Hard shadow (shade << diffuse) |
| 0.25–0.45 | Shared(1) = toon02 | Moderately hard |
| 0.45–0.65 | Shared(2) = toon03 | Medium |
| 0.65–0.85 | Shared(4) = toon05 | Soft |
| ≥ 0.85 | Shared(6) = toon07 | Softest (shade ≈ diffuse) |

Non-MToon retains `Shared(0)` (regression prevention). When shade_color is absent, defaults to `Shared(2)` (medium).

### MToon ambient/specular Correction

Applied only at the conversion stage (`convert/material.rs`). The extraction stage (`vrm/extract.rs`) retains source-faithful values.

| Parameter | MToon | UTS2 | Non-MToon |
|-----------|-------|------|-----------|
| ambient | `shade_color * 0.5` (or `diffuse * 0.4` if no shade_color) | `_2nd_ShadeColor * 0.5` (set during extraction) | Unchanged |
| specular | `diffuse.rgb * 0.2` (light-reactive) | `_HighColor` (set during extraction) | Unchanged |
| specular_power | `10.0` | `_HighColor_Power * 10.0` | Unchanged |

### UTS2 (Unity-Chan Toon Shader Ver.2) Approximate Conversion

Introduces `ShaderFamily` enum (`Other` / `Mtoon` / `Uts2`) to detect UTS2 from VRM 0.0 `materialProperties.shader` field. Detected parameters are approximate-mapped to `MtoonParams`, reusing the existing MToon rendering pipeline (viewer) and PMX conversion path.

#### Shader Detection (Triple Check)

1. **Shader name**: `UnityChanToonShader/*` (legacy)
2. **Shader name + property**: `Toon/Toon` (unified shader) with `_utsVersion` or `_BaseColor_Step` present
3. **Property only**: `_utsVersion` present (fallback for unknown shader names)

Shader names containing "MToon" are excluded (`!v0_is_mtoon &&` guard).

#### Alpha Mode Detection

UTS2 does not have a `_ClippingMode` property. Transparency is determined by shader variant name:

| Variant Name | AlphaMode | Notes |
|---|---|---|
| `_TransClipping` | Blend | Transparent + clipping |
| `_Clipping` | Mask | Cutout |
| Other | Retain glTF core | Opaque by default |

`_ClippingMask` texture is not yet supported in v0.2.10 (warning + base alpha fallback).

#### Outline

UTS2 `_OUTLINE` keyword (NML/POS) detected from `keyword_map`. Both NML and POS approximated as `OutlineWidthMode::WorldCoordinates` (POS uses UTS2-specific camera distance-based transformation differing from MToon ScreenCoordinates; warning emitted).

#### GI

UTS2 `_GI_Intensity` is additive indirect light strength (default 0 = no GI), semantically different from MToon `gi_equalization_factor` (raw/equalized GI interpolation). Fixed to `gi_equalization_factor = 0.0` to avoid semantic inversion.

#### Ambient Overwrite Prevention

The end-of-extraction `ambient = diffuse * 0.4` recalculation for all materials is suppressed for `ShaderFamily::Uts2` to preserve UTS2's `_2nd_ShadeColor * 0.5`.

## A-Stance Conversion Result Management

### AStanceResult enum

A type-safe enum for managing A-stance conversion results. Stored in `IrModel.astance_result`.

| Variant | Description |
|---------|-------------|
| `NotRequested` | Conversion not requested (checkbox OFF, or unsupported format) |
| `Applied(usize)` | Conversion successful. Argument is the number of corrected arms (normally 2) |
| `AlreadyAStance` | Already close to A-stance, skipped |
| `NotFound` | Arm bones not found, conversion failed |

### Determination Logic

`compute_astance_corrections()` / `compute_tstance_corrections()` determine the result with the following priority:

1. **Arm bones absent**: `has_arms` check (no leftUpperArm/leftLowerArm or rightUpperArm/rightLowerArm pair exists) → `NotFound`
2. **Degenerate case**: Zero horizontal component (pointing straight up/down), rotation axis cannot be computed → skipped without counting (distinguished from "already in target pose")
3. **Already in target pose**: For A-stance, current angle exceeds 25° and pointing downward; for T-stance, angle from horizontal is less than 5° → increments `already_target_count`
4. **Normal conversion**: Apply rotation correction → `Applied(n)`
5. **Result determination**: corrections > 0 → `Applied(n)`, already_target_count > 0 → `AlreadyAStance`, otherwise → `NotFound`

### primary_astance_result

Added `primary_astance_result` field to `LoadedModel`. Copies `ir.astance_result` at main model load completion (before merge). UI (viewport persistent warning and PMX export warning) references this field, making it immune to `ir.astance_result` contamination from append/merge operations.

### IrModel::merge() Integration

During append loading, `IrModel::merge()` integrates `astance_result`:

| Host | Appended | Result | Reason |
|------|----------|--------|--------|
| `NotRequested` | any | Appended value | Host did not request, delegate to appended |
| `Applied(a)` | `Applied(b)` | `Applied(a+b)` | Sum |
| `Applied(n)` | `NotFound` | `Applied(n)` | If main model was converted, ignore accessory failure |
| `Applied(n)` | `AlreadyAStance` | `Applied(n)` | Converted takes priority |
| `AlreadyAStance` | `NotFound` | `AlreadyAStance` | AlreadyAStance takes priority |
| `NotFound` | `NotFound` | `NotFound` | Both failed |

### Viewer Warning Display

#### Persistent Warning (Viewport bottom-left, v0.2.5)

When the `normalize_pose` checkbox is ON and `loaded.primary_astance_result` is `NotFound` or `AlreadyAStance`, persistent text is displayed above the operation hints:

- `NotFound` → Red text: `⚠ {A/T}-stance conversion failed: arm bones not found`
- `AlreadyAStance` → Yellow text: `※ Already close to {A/T}-stance, skipped`
- Label switches between "T-stance" / "A-stance" based on `source_format.is_pmx_pmd()`

Hidden when checkbox is OFF.

#### PMX Export Warning

On PMX conversion success, `loaded.primary_astance_result` is checked:

- `NotFound` → `ConvertMessage::Warning` (red text overlay): "Arm bones not found, conversion failed"
- `AlreadyAStance` → `ConvertMessage::Success` with note: "Already close to {A/T}-stance, skipped"
- `Applied(_)` / `NotRequested` → Normal success message

`ConvertResult::Warning` is displayed in red text like `Failure`, but is semantically distinct as the conversion itself succeeded.

## UV Map PSD Layer Grouping

The PSD output in `convert/uvmap.rs` generates model-based group folders when multiple models are merged.

### PSD Group Folder Mechanism

PSD layer groups are implemented using the **lsct (Section Divider Setting)** resource. The following markers are inserted into the layer array (bottom-to-top order):

```
[GroupEnd(lsct type=3)] → [Content layers...] → [GroupStart(lsct type=1)]
```

- **GroupStart**: `lsct type=1` (open folder), blend mode=`pass` (pass-through), name=group name
- **GroupEnd**: `lsct type=3` (bounding section divider), name=`</Layer group>`
- Markers have 0×0 rect, 4 channels with data_length=2 (compression header only)

### Data Flow

```
viewer/app/mod.rs: MaterialGroup { name, material_range, draw_range }
    ↓ (extract material_range only)
viewer/ui.rs: Vec<(String, Range<usize>)>
    ↓
convert/uvmap.rs: export_uv_map_grouped(ir, path, size, groups)
    ↓ validate_groups → build_entries → write_psd_file
PSD file (with layer groups)
```

### Input Validation (`validate_groups`)

- Rejects reversed ranges (`start > end`)
- Rejects ranges exceeding material count
- Rejects overlapping materials across groups

### Entry Construction (`build_entries`)

1. Sort groups by `material_range.start` ascending (via index array to preserve references to original slice)
2. Build reverse lookup map: material index → sorted group index
3. Iterate material indices in descending order, inserting GroupEnd/GroupStart markers at group boundaries
4. Orphan materials (not in any group) appear at root level

### `MaterialGroup` Struct (`viewer/app/mod.rs`)

```rust
pub struct MaterialGroup {
    pub name: String,
    pub material_range: std::ops::Range<usize>,  // Used for UV export
    pub draw_range: std::ops::Range<usize>,       // Used for UI material list
}
```

Separating `material_range` and `draw_range` ensures UV grouping works correctly even for models with zero draw calls.

## Visible Materials Only Export

An optional feature that excludes materials hidden in the display tab from PMX conversion output in the viewer. Implemented in the `export_filter.rs` module.

### Design Principles

- **Viewer-specific**: Filter logic is placed in `viewer/export_filter.rs`. No changes to core conversion logic (`pmx/build.rs`, `lib.rs`)
- **IrModel manual construction**: Since `IrModel`/`IrMesh`/`IrPhysics` lack `Clone`, filtered IR is newly constructed field by field
- **draw→material conversion**: `material_visibility` is managed per DrawCall unit (GPU draw call unit), so it is converted to a `HashSet` of `material_index` via `mat_cache.draw_indices`

### Processing Flow (`build_filtered_ir`)

```
Phase 1: Material remap (build HashMap of old_mat_idx → new_mat_idx)
Phase 2: Mesh filter + vertex remap table construction
         old_global_vtx_idx → new_global_vtx_idx (vertices of excluded meshes are None)
Phase 3: Morph validity check (recursive convergence loop)
         Vertex morph: valid if 1+ entries remain after remap
         Group morph: valid if 1+ child morphs are valid (iterative check)
Phase 4: morph_remap construction + morph building (both vertex/group)
Phase 5: Texture pruning + texture_index remap
Phase 6: IrModel construction (bones and physics copied as-is)
```

### Recursive Morph Validity Check

Excluding vertex morphs can cause group morph children to disappear. To handle nested group morphs (`outer → inner → vertex`), a convergence loop is used:

```rust
// Phase 3: Iterate morph_alive array until convergence
loop {
    let mut changed = false;
    for (i, morph) in ir.morphs.iter().enumerate() {
        if morph_alive[i] { continue; }
        if let IrMorphKind::Group(goffs) = &morph.kind {
            if goffs.iter().any(|&(child, _)| morph_alive[child]) {
                morph_alive[i] = true;
                changed = true;
            }
        }
    }
    if !changed { break; }
}
```

Converges in worst case O(depth) iterations (at least 1 candidate is finalized per iteration).

### Texture Pruning

Collect `texture_index` and all `IrTextureInfo` fields (shade / outline_width / matcap / shading_shift / rim_multiply / uv_animation_mask) referenced by post-filter materials, keeping only used textures. Remap each material's indices via `IrTextureInfo::remap_index()`. If all materials are hidden, textures are also emptied.

### Specification

| Condition | Behavior |
|-----------|----------|
| Default | OFF (all materials exported as before) |
| All materials hidden | Export empty PMX + warning log |
| Emptied vertex morph | Deleted + warning log |
| Emptied group morph | Deleted + warning log |
| On model load | Reset `export_visible_only` to `false` |
| On PMX/PMD load | Checkbox disabled in UI |

## Architecture

![Architecture](architecture.svg)

## Source File Structure

```
src/
├── main.rs              Entry point (no args or no output specified → viewer / output specified → CLI conversion)
├── lib.rs               Library API
├── error.rs             Error type definitions (PoponeError enum, thiserror, ResultExt trait)
├── unitypackage.rs      .unitypackage (tar.gz) asset extraction + Prefab texture mapping (GUID resolution, Variant recursion, multi-format support)
├── archive/
│   ├── mod.rs           ZIP / 7z unified API (list_models, extract_model_bundle)
│   ├── zip_extract.rs   ZIP extraction (2-pass: metadata listing → selective extraction)
│   └── sevenz.rs        7z extraction (filtered full extraction, chunked read with size limit)
├── vrm/
│   ├── loader.rs        GLB loading / extension data extraction (file and byte array support)
│   ├── detect.rs        VRM version auto-detection
│   ├── extract.rs       VRM → intermediate representation (IrModel) extraction
│   ├── animation.rs     VRMA / glTF animation loading
│   ├── types_v0.rs      VRM 0.0 serde type definitions
│   └── types_v1.rs      VRM 1.0 serde type definitions
├── fbx/
│   ├── parser.rs        FBX binary / ASCII parser (including Content block special handling)
│   ├── scene.rs         Scene graph construction (Objects / Connections analysis)
│   ├── extract.rs       FBX → intermediate representation (IrModel) extraction
│   ├── bone.rs          Bone hierarchy construction (PreRotation support)
│   ├── mesh.rs          Mesh, UV, material property extraction
│   ├── skin.rs          Skin weight extraction
│   ├── texture.rs       Texture extraction (embedded / external file)
│   ├── blendshape.rs    Blend shape extraction
│   ├── animation.rs     FBX animation extraction (Stack/Layer/CurveNode/Curve hierarchy, byte array support)
│   └── humanoid.rs      Humanoid rig auto-detection and mapping (namespace prefix stripping, CamelCase support)
├── pmx/
│   ├── types.rs         PMX data type definitions
│   ├── reader.rs        PMX 2.0/2.1 binary loading (UTF-16LE/UTF-8, SoftBody skip)
│   ├── extract.rs       PMX → intermediate representation (IrModel) extraction (glTF reverse conversion)
│   ├── build.rs         Intermediate representation → PMX model construction / standard bone insertion
│   └── writer.rs        PMX binary output (UTF-16 LE)
├── pmd/
│   ├── types.rs         PMD data type definitions
│   ├── reader.rs        PMD binary loading (Shift_JIS, encoding_rs)
│   └── extract.rs       PMD → intermediate representation (IrModel) extraction (material name text loading support)
├── obj/
│   ├── mod.rs           OBJ module definition
│   └── extract.rs       OBJ → intermediate representation (tobj crate, MTL/texture resolution, cm→m normalization, auto normal generation)
├── stl/
│   ├── mod.rs           STL module definition
│   ├── reader.rs        STL binary / ASCII parser (format detection by length validation)
│   └── extract.rs       STL → intermediate representation (mm→m + Z-Up→Y-Up normalization, zero-normal recalculation)
├── unity/
│   └── animation.rs     Unity .anim Muscle conversion (SwingTwist decomposition)
├── intermediate/
│   ├── types.rs         Intermediate representation (IrModel / IrBone / IrMesh / IrMaterial / MtoonParams / CullMode etc., SourceFormat / merge 3-level fallback)
│   ├── tangent.rs       MikkTSpace tangent generation (mikktspace crate)
│   ├── animation.rs     Animation intermediate representation (VrmaAnimation / BoneChannel)
│   └── pose.rs          Stance conversion (T→A / A→T, physics sync support)
├── convert/
│   ├── coord.rs         Coordinate conversion (glTF → PMX / PMX → glTF)
│   ├── bone_map.rs      VRM humanoid bone ↔ PMX Japanese name map (bidirectional)
│   ├── material.rs      Material conversion
│   ├── morph.rs         Expression → morph name map
│   ├── physics.rs       SpringBone → rigid body / joint conversion (V0/V1)
│   ├── texture.rs       Texture PNG output
│   └── uvmap.rs         UV map PSD output (material layers, boundary wrap, group folders)
└── viewer/              ← Compiled only when feature = "viewer"
    ├── app/             eframe::App state management (split into 5 modules)
    │   ├── mod.rs           ViewerApp struct definition, initialization, eframe::App impl
    │   ├── file_io.rs       File loading, drag & drop, reload
    │   ├── texture_mgmt.rs  Texture assignment and preview
    │   ├── pending.rs       Deferred task processing (PendingState / ExportState)
    │   └── helpers.rs       Utility types and functions (ReloadableSource / TextureSource / is_temp_path etc.)
    ├── gpu.rs           wgpu pipeline / offscreen rendering / visualization buffer dirty flag
    ├── mesh.rs          IrModel → GPU vertex buffer conversion
    ├── texture.rs       Texture GPU upload (MIME hint support)
    ├── camera.rs        Orbit camera
    ├── grid.rs          Grid floor
    ├── ui.rs            Info panel / morph sliders / conversion button / PMX/PMD grayed out
    ├── export_filter.rs Visible materials only export filter (IrModel → filtered IrModel)
    ├── animation.rs     Animation playback / retargeting (VRMA/glTF/FBX support)
    └── single_instance.rs Single instance control (Named Mutex + Named Pipe IPC, Windows only)
```

## Library API

`popone` can also be used as a library:

```rust
use popone::{convert_vrm_to_pmx, convert_fbx_to_pmx};
use std::path::Path;

// VRM to PMX
let stats = convert_vrm_to_pmx(
    Path::new("input.vrm"),
    Path::new("output.pmx"),
    false, // no_physics
)?;

// FBX to PMX
let stats = convert_fbx_to_pmx(
    Path::new("input.fbx"),
    Path::new("output.pmx"),
)?;

println!("Bones: {}, Vertices: {}", stats.bones, stats.vertices);
```

## Tests

```bash
cargo test
```

85 tests. Integration tests support environment variables for test data paths:

```bash
# Test data root directory
export POPONE_TEST_DATA=/path/to/test-fixtures

# Or specify individual files
export POPONE_TEST_VRM_SEED_SAN=/path/to/Seed-san.vrm
export POPONE_TEST_PMX_SEED_SAN=/path/to/Seed-san.pmx
export POPONE_TEST_PMD_MIKU_V2=/path/to/初音ミクVer2.pmd
```

## Changelog

For detailed per-version improvements and internal changes, see the [Changelog](CHANGELOG.en.md).

## Limitations

- **PMX/PMD is view-only** — PMX conversion (re-export) is not supported. Only viewer display and UV map output
- **Texture size limit** — Textures exceeding the GPU's `max_texture_dimension_2d` (typically 8192px) are automatically downscaled in `upload_rgba_to_gpu` (using `image::imageops::resize` with Triangle filter). Does not affect PMX conversion output (viewer display only)
- **Extraction size limit** — Archive (ZIP / 7z) and `.unitypackage` (tar.gz) extraction is capped at 2GB total (`MAX_TOTAL_BYTES`). `.unitypackage` uses dual protection: header size pre-check + actual bytes post-check
- **MMD-specialized models** — Models specialized for MMD rendering may not display some surfaces correctly
- **PMX 2.1 SoftBody** — Skipped (not supported)
- **Only `TEXCOORD_0` / `TEXCOORD_1` are supported** — When glTF `TextureInfo.texCoord` is 2 or higher, it falls back to `texCoord=0` (`warn` log emitted). Texture UV will be inaccurate but rendering is preserved (graceful degradation). Rationale:
  - VRM 1.0 / MToon spec only uses `TEXCOORD_0` and `TEXCOORD_1`
  - UniVRM's MToon implementation (`vrmc_materials_mtoon_geometry_uv.hlsl`) only uses UV0/UV1
  - While glTF allows arbitrary UV sets, VRM models using `TEXCOORD_2+` are virtually nonexistent
  - Future support would require variable-length UV sets in `IrMesh` + GPU vertex format extension


## References

| Format | Resource | Notes |
|--------|----------|-------|
| VRM | [vrm-c/vrm-specification](https://github.com/vrm-c/vrm-specification) | VRM 0.0 / 1.0 official specification. Defines humanoid bones, Expression, SpringBone, MToon, etc. as glTF 2.0 extensions |
| PMX | PMX Specification (bundled with PmxEditor) | PMX 2.0 binary format specification included with PmxEditor. Defines data structures for header, vertices, faces, materials, bones, morphs, display frames, rigid bodies, and joints |
| PMD | MikuMikuDance bundled documentation | PMD binary format (fixed-length structures, Shift_JIS text) |

### Key Points of the VRM Specification

- VRM uses `.vrm` extension based on glTF 2.0 (`.glb`)
- VRM-specific data is stored in glTF's `extensions` field
- VRM 1.0 key extensions: `VRMC_vrm` (humanoid, Expression, gaze, meta info), `VRMC_materials_mtoon` (cel shading), `VRMC_springBone` (physics for swaying objects)
- Coordinate system follows glTF: right-handed, meter units
- VRM 0.0 uses the `VRM` extension and differs from 1.0 in that the root node has a Y=180° rotation

### Key Points of the PMX Specification

- PMX 2.0 is a little-endian binary format
- String encoding is UTF-16 LE (encoding=0)
- Index sizes are variable (1/2/4 bytes, specified in header)
- Bones support IK, grant (rotation/translation), and deform layers
- Rigid bodies and joints are Bullet Physics compatible (Euler angles use D3DX row-major ZXY convention, YXZ intrinsic in glam)
- Coordinate system is left-handed, Y-up, +Z forward, with custom scale units (1m = 12.5 in this tool)

### Key Points of the PMD Specification

- Little-endian binary format, magic `"Pmd"`
- Text is fixed-length Shift_JIS (bone name 20 bytes, comment 256 bytes)
- Vertex is fixed at 38 bytes (BDEF2 only, weight is integer 0-100)
- IK is stored in a separate section from bones
- Morphs use base + offset format (base morph global vertex positions + delta offsets)
- English header, toon textures, rigid bodies, and joints are optional extensions at end of file

## WGSL Shader Architecture

Shaders in `gpu.rs` use `macro_rules!` + `concat!` to centrally manage common struct definitions.

### Common Macros

| Macro | Content | Used By |
|-------|---------|---------|
| `wgsl_camera_uniform!()` | `CameraUniform` struct definition | All 8 shaders |
| `wgsl_mmd_material_uniform!()` | `MmdMaterialUniform` struct definition | 4 MMD shaders |
| `wgsl_material_uniform!()` | `MaterialUniform` struct definition | Basic shader, wire overlay |
| `wgsl_mmd_main_body!()` | MMD vertex shader + `compute_mmd_lighting` function | MMD main sRGB/Unorm |
| `wgsl_mmd_edge_body!()` | MMD edge vertex shader | MMD edge sRGB/Unorm |
| `wgsl_grid_body!()` | Grid vertex shader | Grid sRGB/Unorm |

### Shader Constants

| Constant | Macro Composition | Difference (Fragment Shader) |
|----------|-------------------|------------------------------|
| `SHADER_SRC` | camera + material + custom | Half-Lambert / MToon 2-color toon branching |
| `MMD_EDGE_SHADER_SRC` | camera + mmd_mat + edge_body + custom | `pow(c.rgb, 2.2)` — sRGB correction |
| `MMD_EDGE_SHADER_UNORM_SRC` | camera + mmd_mat + edge_body + custom | `edge_color` direct output |
| `MMD_MAIN_SHADER_SRC` | camera + mmd_mat + main_body + custom | `pow(out_rgb, 2.2)` — sRGB correction |
| `MMD_MAIN_SHADER_UNORM_SRC` | camera + mmd_mat + main_body + custom | `clamp(out_rgb)` — gamma-space direct output |
| `GRID_SHADER_SRC` | camera + grid_body + custom | `in.color` pass-through |
| `GRID_SHADER_UNORM_SRC` | camera + grid_body + custom | `linear_to_srgb()` conversion |
| `WIRE_OVERLAY_SHADER_SRC` | camera + material + custom | Fixed black `(0,0,0,1)` |

The only difference between sRGB and Unorm variants is the final transform applied to `compute_mmd_lighting()` output. The core lighting, texture sampling, sphere map, and toon logic is fully shared.
