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
  - [Viewer Display Styles](#viewer-display-styles)
    - [Bone Display](#bone-display-1)
    - [Rigid Body Display](#rigid-body-display)
    - [Joint Display (PMX/PMD only)](#joint-display-pmxpmd-only)
    - [Normal Map Display](#normal-map-display)
    - [Render Order](#render-order)
  - [Camera & Lighting](#camera--lighting)
    - [Camera](#camera)
    - [Fit Calculation (compute_fit)](#fit-calculation-compute_fit)
    - [Lighting](#lighting)
    - [MMD Ambient Separation](#mmd-ambient-separation)
  - [Log Output](#log-output)
    - [Overall Log Structure](#overall-log-structure)
  - [Single Instance](#single-instance)
  - [FPS Measurement](#fps-measurement)
  - [Animation Playback](#animation-playback)
    - [Supported Formats](#supported-formats)
    - [Animation Playback for PMX/PMD](#animation-playback-for-pmxpmd)
    - [Humanoid Retargeting](#humanoid-retargeting)
    - [FBX Animation Coordinate Transformation](#fbx-animation-coordinate-transformation)
    - [Unity .anim Muscle Conversion (Hidden Feature)](#unity-anim-muscle-conversion-hidden-feature)
    - [Loop Modes](#loop-modes)
  - [Model Append Loading](#model-append-loading)
    - [Bone Merge 2-Pass Method](#bone-merge-2-pass-method)
    - [ASCII FBX Content Block Processing](#ascii-fbx-content-block-processing)
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
  - [Reload Texture Normalization](#reload-texture-normalization)
    - [reload_unitypackage Texture Restoration](#reload_unitypackage-texture-restoration)
    - [IrTexture Deduplication in assign_texture_source_to_material](#irtexture-deduplication-in-assign_texture_source_to_material)
  - [Shader-Aware PMX Material Conversion](#shader-aware-pmx-material-conversion)
    - [select_toon()](#select_toon)
    - [MToon ambient/specular Correction](#mtoon-ambientspecular-correction)
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
    - [`MaterialGroup` Struct (`viewer/app.rs`)](#materialgroup-struct-viewerapprs)
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
| Morph offset | `(x, y, -z) / 12.5` (displacement vector, scale required) |
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
5. Apply rotation to morph offsets
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

## Viewer Display Styles

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

### Normal Map Display

- In-shader normal vector → RGB conversion: `rgb = (normalize(normal) + 1.0) * 0.5`
- Toggled via CameraUniform's `show_normal_map` flag

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
| Fixed | `Vec3(0.5, 1.0, -0.5).normalize()` — MMD-compliant (inversion of (-0.5,-1.0,0.5)) |
| Camera-Follow | `(forward + right*(-0.3) + up*0.7).normalize()` — MMD-style upper-left bias |

| Parameter | Value |
|-----------|-------|
| light_intensity | 0.6 |
| ambient_intensity | 0.5 |

### MMD Ambient Separation

The `mmd_ambient_scale` field in CameraUniform separates ambient light between standard and MMD paths:

- MMD mode ON: `mmd_ambient_scale = 1.0` (uses material's ambient value directly)
- MMD mode OFF: `mmd_ambient_scale = ambient_intensity` (UI slider value)

Standard shaders use `camera.ambient`, while only MMD shaders reference `camera.mmd_ambient_scale`.

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

### Bone Merge 2-Pass Method

When merging same-named bones into the existing side with `IrModel::merge()`, a 2-pass method is used to guarantee parent-child relationship consistency regardless of order.

#### Problem

In a 1-pass method, `is_new_bone[parent_idx]` references the array being constructed, causing panics or misidentification when the bone array is not in parent → child order. Also, determining merge by string matching on parent names alone can cause descendants from different subtrees to be incorrectly merged into the existing side.

Example: For existing `Root→Spine→Head` and appended `Accessory→Spine→Head`, `Spine` is newly added due to parent mismatch, but `Head`'s parent name is `"Spine"` in both cases, causing it to be incorrectly merged with the existing `Head`.

#### Solution

```
Pass 1 (candidate collection): Scan all bones, collect merge candidates with same name + same parent name regardless of order
  candidate[i] = Some(self_idx)  // Name match and parent name match

Pass 2 (propagation loop): Cancel candidates whose parent is not a candidate, iterate until no changes
  while changed:
    for i in 0..N:
      if candidate[i].is_some() && parent's candidate is None:
        candidate[i] = None  // Parent is new → child is also new

Finalize: Merge bones with Some candidate, add bones with None candidate as new
```

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

| Parameter | MToon | Non-MToon |
|-----------|-------|-----------|
| ambient | `shade_color * 0.5` (or `diffuse * 0.4` if no shade_color) | Unchanged |
| specular | `Vec3::ZERO` | Unchanged |
| specular_power | `0.0` | Unchanged |

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
viewer/app.rs: MaterialGroup { name, material_range, draw_range }
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

### `MaterialGroup` Struct (`viewer/app.rs`)

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

Collect `texture_index` / `shade_texture_index` / `outline_width_texture_index` referenced by post-filter materials, and keep only used textures. Remap each material's indices. If all materials are hidden, textures are also emptied.

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
├── error.rs             Error type definitions (PoponeError enum, thiserror)
├── unitypackage.rs      .unitypackage (tar.gz) asset extraction (VRM / FBX detection and extraction)
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
├── unity/
│   └── animation.rs     Unity .anim Muscle conversion (SwingTwist decomposition)
├── intermediate/
│   ├── types.rs         Intermediate representation (IrModel / IrBone / IrMesh etc., SourceFormat / merge 2-pass method)
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
    ├── app.rs           eframe::App state management (TextureState / AnimLibrary / PendingState / ExportState sub-structs)
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
- **Normal maps not applied** — VRM/FBX normalTexture is not reflected in shading (can be viewed in normal map display mode)
- **Texture size limit** — Textures exceeding the GPU's `max_texture_dimension_2d` (typically 8192px) are automatically downscaled in `upload_rgba_to_gpu` (using `image::imageops::resize` with Triangle filter). Does not affect PMX conversion output (viewer display only)
- **Extraction size limit** — Archive (ZIP / 7z) and `.unitypackage` (tar.gz) extraction is capped at 2GB total (`MAX_TOTAL_BYTES`). `.unitypackage` uses dual protection: header size pre-check + actual bytes post-check
- **MMD-specialized models** — Models specialized for MMD rendering may not display some surfaces correctly
- **PMX 2.1 SoftBody** — Skipped (not supported)

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
| `SHADER_SRC` | camera + material + custom | Half-Lambert textured rendering |
| `MMD_EDGE_SHADER_SRC` | camera + mmd_mat + edge_body + custom | `pow(c.rgb, 2.2)` — sRGB correction |
| `MMD_EDGE_SHADER_UNORM_SRC` | camera + mmd_mat + edge_body + custom | `edge_color` direct output |
| `MMD_MAIN_SHADER_SRC` | camera + mmd_mat + main_body + custom | `pow(out_rgb, 2.2)` — sRGB correction |
| `MMD_MAIN_SHADER_UNORM_SRC` | camera + mmd_mat + main_body + custom | `clamp(out_rgb)` — gamma-space direct output |
| `GRID_SHADER_SRC` | camera + grid_body + custom | `in.color` pass-through |
| `GRID_SHADER_UNORM_SRC` | camera + grid_body + custom | `linear_to_srgb()` conversion |
| `WIRE_OVERLAY_SHADER_SRC` | camera + material + custom | Fixed black `(0,0,0,1)` |

The only difference between sRGB and Unorm variants is the final transform applied to `compute_mmd_lighting()` output. The core lighting, texture sampling, sphere map, and toon logic is fully shared.
