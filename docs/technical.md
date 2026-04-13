<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents** *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [Technical Details](#technical-details)
  - [Chapter Organization](#chapter-organization)
  - [Coordinate Transformation](#coordinate-transformation)
    - [PMX/PMD → IrModel Reverse Conversion](#pmxpmd-%E2%86%92-irmodel-reverse-conversion)
  - [VRM Loading](#vrm-loading)
    - [Format Detection](#format-detection)
    - [Extension Parsing](#extension-parsing)
    - [VRM 0.0 vs 1.0 Differences](#vrm-00-vs-10-differences)
    - [Humanoid Bone Mapping](#humanoid-bone-mapping)
    - [Expression / BlendShape Extraction](#expression--blendshape-extraction)
    - [Meta Info Extraction](#meta-info-extraction)
    - [Texture Handling](#texture-handling)
  - [FBX Loading](#fbx-loading)
    - [Binary vs ASCII Detection](#binary-vs-ascii-detection)
    - [FBX Node Tree](#fbx-node-tree)
    - [FBX Scene Graph](#fbx-scene-graph)
    - [Rig Auto-Detection](#rig-auto-detection)
    - [Bone Hierarchy Extraction](#bone-hierarchy-extraction)
    - [Coordinate System & UnitScaleFactor](#coordinate-system--unitscalefactor)
    - [GeometryInstance Extraction](#geometryinstance-extraction)
    - [ASCII FBX Content Block Processing](#ascii-fbx-content-block-processing)
    - [FBX Parser Input Validation](#fbx-parser-input-validation)
    - [FBX External Texture Nearby Search](#fbx-external-texture-nearby-search)
    - [Texture Resolution Pipeline](#texture-resolution-pipeline)
  - [PMX/PMD Loading](#pmxpmd-loading)
    - [PMX Reader](#pmx-reader)
    - [PMD Reader](#pmd-reader)
    - [IrModel Conversion](#irmodel-conversion)
    - [T-Stance Conversion](#t-stance-conversion)
    - [Rigid Body Rotation](#rigid-body-rotation)
    - [Texture Loading](#texture-loading)
    - [Mipmap Generation](#mipmap-generation)
  - [OBJ/STL Loading](#objstl-loading)
    - [OBJ Reader](#obj-reader)
    - [STL Reader](#stl-reader)
    - [Coordinate Conversion](#coordinate-conversion)
    - [IrModel Construction](#irmodel-construction)
    - [Dynamic Grid](#dynamic-grid)
  - [DirectX .x Loading](#directx-x-loading)
    - [Parser (`directx/parser.rs`)](#parser-directxparserrs)
    - [IrModel Conversion (`directx/extract.rs`)](#irmodel-conversion-directxextractrs)
  - [UnityPackage Loading](#unitypackage-loading)
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
    - [GeometryInstance-Based source_material](#geometryinstance-based-source_material)
    - [link_same_name Scope Restriction](#link_same_name-scope-restriction)
  - [Direct Archive Loading](#direct-archive-loading)
    - [archive Module](#archive-module)
    - [Viewer Integration](#viewer-integration)
    - [CLI](#cli)
  - [Drag & Drop Loading](#drag--drop-loading)
    - [Temp Path Detection](#temp-path-detection)
    - [Temp Path Byte Prefetch](#temp-path-byte-prefetch)
    - [D&D Preload Cache (PreloadedData)](#dd-preload-cache-preloadeddata)
    - [Auxiliary File Cache](#auxiliary-file-cache)
    - [TextureSource enum](#texturesource-enum)
    - [Texture D&D Preview Cache](#texture-dd-preview-cache)
    - [.gltf Exclusion](#gltf-exclusion)
  - [Asynchronous Model Loading](#asynchronous-model-loading)
    - [Data Flow](#data-flow)
    - [Key Types (pending.rs)](#key-types-pendingrs)
    - [`cpu_parse_source` Free Function (file_io.rs)](#cpu_parse_source-free-function-file_iors)
    - [`route_load_dispatch` Method](#route_load_dispatch-method)
    - [`apply_bg_load_result` Method](#apply_bg_load_result-method)
    - [Multi-thread Safety](#multi-thread-safety)
    - [Raw RGBA Texture Bypass](#raw-rgba-texture-bypass)
    - [Deferred GPU Build](#deferred-gpu-build)
  - [Model Append Loading](#model-append-loading)
    - [Bone Merge 3-Level Fallback Method](#bone-merge-3-level-fallback-method)
    - [pkg Texture Namespace](#pkg-texture-namespace)
    - [Prefab Append Loading](#prefab-append-loading)
    - [Multi-Model Batch Loading](#multi-model-batch-loading)
  - [Model Reload](#model-reload)
    - [ReloadableSource enum](#reloadablesource-enum)
    - [reload_from_source](#reload_from_source)
    - [UnityPackage Archive Snapshot](#unitypackage-archive-snapshot)
    - [Prefab Reload (A/T Stance Conversion Support)](#prefab-reload-at-stance-conversion-support)
    - [FBX Direct Selection: Prefab-Aware Reload](#fbx-direct-selection-prefab-aware-reload)
    - [Reload Stable Key: PkgModelLocator](#reload-stable-key-pkgmodellocator)
    - [reload_unitypackage Texture Restoration](#reload_unitypackage-texture-restoration)
    - [Reload User State Preservation (v0.3.0)](#reload-user-state-preservation-v030)
    - [IrTexture Deduplication in assign_texture_source_to_material](#irtexture-deduplication-in-assign_texture_source_to_material)
  - [GPU Pipeline Warm-up & Model Build Optimization](#gpu-pipeline-warm-up--model-build-optimization)
    - [Pipeline Warm-up (`WarmupPhase`)](#pipeline-warm-up-warmupphase)
    - [GPU Model Build Split (`cpu_prep_model` / `gpu_finalize_model`)](#gpu-model-build-split-cpu_prep_model--gpu_finalize_model)
    - [Incremental Thumbnail Cache](#incremental-thumbnail-cache)
    - [IR Texture Thumbnail Cache (v0.5.2)](#ir-texture-thumbnail-cache-v052)
  - [WGSL Shader Architecture](#wgsl-shader-architecture)
    - [Common Macros](#common-macros)
    - [Shader Constants](#shader-constants)
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
  - [Material Editing and Expression Material Binds (v0.5.0 / v0.5.1)](#material-editing-and-expression-material-binds-v050--v051)
    - [Material Editor Drawer — Update Path](#material-editor-drawer--update-path)
    - [DrawCall.material_buf — Persistent Uniform Buffer Handle (v0.5.1)](#drawcallmaterial_buf--persistent-uniform-buffer-handle-v051)
    - [Full Rebuild Information-Source Integrity (VRM / PMX / PMD)](#full-rebuild-information-source-integrity-vrm--pmx--pmd)
    - [Texture History Recall Ordering](#texture-history-recall-ordering)
    - [Expression Re-application Timing](#expression-re-application-timing)
    - [Expression Material Binds — Playback Pipeline](#expression-material-binds--playback-pipeline)
    - [Texture History Auxiliary Slot Persistence (v0.5.1)](#texture-history-auxiliary-slot-persistence-v051)
  - [Bloom Post-Effect](#bloom-post-effect)
    - [Dual Kawase Algorithm](#dual-kawase-algorithm)
    - [MRT (Multiple Render Target) Emissive Separation](#mrt-multiple-render-target-emissive-separation)
    - [UI Parameters](#ui-parameters)
    - [Per-Material Emissive Toggle](#per-material-emissive-toggle)
    - [PMX/PMD Self-Emissive Material Bloom Detection](#pmxpmd-self-emissive-material-bloom-detection)
    - [Prefab Emission Support](#prefab-emission-support)
  - [Camera & Lighting](#camera--lighting)
    - [Camera](#camera)
    - [Fit Calculation (compute_fit)](#fit-calculation-compute_fit)
    - [Lighting](#lighting)
    - [MMD Ambient Separation](#mmd-ambient-separation)
  - [Viewer Display Styles](#viewer-display-styles)
    - [Dark Theme](#dark-theme)
    - [VRM Meta Info Color Badges](#vrm-meta-info-color-badges)
    - [Splash Image](#splash-image)
    - [Rigid Body Display](#rigid-body-display)
    - [Joint Display (PMX/PMD only)](#joint-display-pmxpmd-only)
    - [Wireframe Draw Modes](#wireframe-draw-modes)
    - [Normal Map Display](#normal-map-display)
    - [Normal Map Tangent Space (TBN)](#normal-map-tangent-space-tbn)
    - [Render Order](#render-order)
  - [Bone Display](#bone-display)
    - [Shape Determination (Priority Order)](#shape-determination-priority-order)
    - [IK Detection (Two Paths)](#ik-detection-two-paths)
    - [IK-Affected Bones](#ik-affected-bones)
    - [Drawing Direction](#drawing-direction)
    - [Rendering Pipeline](#rendering-pipeline)
    - [Appearance (Color / Size)](#appearance-color--size)
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
  - [Shader-Aware PMX Material Conversion](#shader-aware-pmx-material-conversion)
    - [generate_toon() (replaces select_toon)](#generate_toon-replaces-select_toon)
    - [MToon ambient/specular Correction](#mtoon-ambientspecular-correction)
    - [UTS2 (Unity-Chan Toon Shader Ver.2) Approximate Conversion](#uts2-unity-chan-toon-shader-ver2-approximate-conversion)
    - [lilToon Approximate Conversion](#liltoon-approximate-conversion)
    - [Poiyomi Approximate Conversion](#poiyomi-approximate-conversion)
  - [A-Stance Conversion Result Management](#a-stance-conversion-result-management)
    - [AStanceResult enum](#astanceresult-enum)
    - [Determination Logic](#determination-logic)
    - [primary_astance_result](#primary_astance_result)
    - [IrModel::merge() Integration](#irmodelmerge-integration)
    - [Viewer Warning Display](#viewer-warning-display)
  - [Visible Materials Only Export](#visible-materials-only-export)
    - [Design Principles](#design-principles)
    - [Processing Flow (`build_filtered_ir`)](#processing-flow-build_filtered_ir)
    - [Recursive Morph Validity Check](#recursive-morph-validity-check)
    - [Texture Pruning](#texture-pruning)
    - [Specification](#specification)
  - [UV Map PSD Layer Grouping](#uv-map-psd-layer-grouping)
    - [PSD Group Folder Mechanism](#psd-group-folder-mechanism)
    - [Data Flow](#data-flow-1)
    - [Input Validation (`validate_groups`)](#input-validation-validate_groups)
    - [Entry Construction (`build_entries`)](#entry-construction-build_entries)
    - [`MaterialGroup` Struct (`viewer/app/mod.rs`)](#materialgroup-struct-viewerappmodrs)
  - [Animation Playback](#animation-playback)
    - [Pose Reset on Animation Clear](#pose-reset-on-animation-clear)
    - [Supported Formats](#supported-formats)
    - [Animation Playback for PMX/PMD](#animation-playback-for-pmxpmd)
    - [Humanoid Retargeting](#humanoid-retargeting)
    - [FBX Animation Coordinate Transformation](#fbx-animation-coordinate-transformation)
    - [Unity .anim Muscle Conversion (Hidden Feature)](#unity-anim-muscle-conversion-hidden-feature)
    - [Loop Modes](#loop-modes)
  - [Session Persistence](#session-persistence)
    - [Settings File (popone.toml)](#settings-file-poponetoml)
    - [Texture Assignment History (popone_history.json)](#texture-assignment-history-popone_historyjson)
  - [Log Output](#log-output)
    - [Overall Log Structure](#overall-log-structure)
    - [Panic Log](#panic-log)
    - [Log Viewer (Separate Window)](#log-viewer-separate-window)
  - [Single Instance](#single-instance)
  - [FPS Measurement](#fps-measurement)
  - [Watchdog — Main Thread Responsiveness Monitor](#watchdog--main-thread-responsiveness-monitor)
    - [Architecture](#architecture-1)
    - [Heartbeat (`viewer/watchdog.rs`)](#heartbeat-viewerwatchdogrs)
    - [Watchdog Thread](#watchdog-thread)
    - [Log Output Examples](#log-output-examples)
  - [Codebase Architecture](#codebase-architecture)
  - [Source File Structure](#source-file-structure)
  - [Library API](#library-api)
  - [Tests](#tests)
  - [Changelog](#changelog)
  - [Limitations](#limitations)
  - [References](#references)
    - [Key Points of the VRM Specification](#key-points-of-the-vrm-specification)
    - [Key Points of the PMX Specification](#key-points-of-the-pmx-specification)
    - [Key Points of the PMD Specification](#key-points-of-the-pmd-specification)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

[日本語](technical.jp.md)

# Technical Details

Detailed documentation on the internal implementation of popone.

## Chapter Organization

This document is organized into seven logical categories:

**Format Loading** — Per-format parsing and extraction pipelines:
Coordinate Transformation, VRM Loading, FBX Loading, PMX/PMD Loading, OBJ/STL Loading, DirectX .x Loading, UnityPackage Loading, Direct Archive Loading.

**Loading Pipeline** — Runtime loading orchestration and lifecycle:
Drag & Drop Loading, Asynchronous Model Loading, Model Append Loading, Model Reload, GPU Pipeline Warm-up & Model Build Optimization.

**Rendering** — GPU rendering, shaders, and visual display:
WGSL Shader Architecture, MMD Rendering, Shader Override, MToon Shading, Bloom Post-Effect, Camera & Lighting, Viewer Display Styles, Bone Display.

**Content Conversion & Export** — Model transformation and PMX export:
MMD Standard Bone Insertion, PMX Grant Animation, Shader-Aware PMX Material Conversion, A-Stance Conversion Result Management, Visible Materials Only Export, UV Map PSD Layer Grouping.

**Animation** — Playback, retargeting, and animation import:
Animation Playback.

**Platform & Operations** — Runtime infrastructure and operational support:
Session Persistence, Log Output, Single Instance, FPS Measurement, Watchdog.

**Reference** — Architectural and external reference material:
Codebase Architecture, Source File Structure, Library API, Tests, Changelog, Limitations, References.

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

## VRM Loading

VRM 0.0 and VRM 1.0 are glTF 2.0 based formats for humanoid avatars. popone uses the `gltf` crate with `features = ["extensions"]` to parse VRM-specific extensions, and consumes the raw JSON through `gltf::Document::as_json()` so that extensions the crate does not natively support (MToon, spring bone, node constraints) can still be read.

### Format Detection

- `vrm::loader::load_glb` / `load_glb_from_data` parses the `.glb` container via `gltf::import` / `import_slice` and extracts the VRM extension JSON
- `vrm::detect::detect_version` inspects `gltf::Document::as_json().extensions.others`:
 - `VRMC_vrm` present → `VrmVersion::V1`
 - `VRM` present → `VrmVersion::V0`
 - Otherwise → `VrmVersion::Unknown` (plain GLB path — still loaded, just without humanoid/morph data)
- `extract_vrm_extension` returns the matching extension JSON, or an empty object for plain GLB
- The Unknown branch is still routed through `extract_physics_v1`, so plain GLB files that only contain the `VRMC_springBone` extension still get their spring bones loaded

### Extension Parsing

VRM extension JSON is deserialized once into strongly-typed `VrmV0` / `VrmV1` structs (`vrm::types_v0`, `vrm::types_v1`) and wrapped in a `VrmTyped` enum so the rest of the pipeline only pays the `serde_json::from_value` cost once:

| Extension | VRM ver. | Parsed into | Usage |
|-----------|----------|-------------|-------|
| `VRMC_vrm` | 1.0 | `VrmV1` (humanoid, meta, expressions, lookAt, firstPerson) | Humanoid mapping, Expression extraction, meta comment |
| `VRM` (legacy) | 0.0 | `VrmV0` (humanoid, meta, blendShapeMaster, secondaryAnimation, materialProperties) | Same as above + MToon migration |
| `VRMC_materials_mtoon` | 1.0 | Read per-material from `materials[i].extensions` (raw JSON) | See [MToon Shading](#mtoon-shading) |
| `VRMC_springBone` | 1.0 | `SpringBoneV1` (via `all_extensions`) | Spring bone physics |
| `VRMC_node_constraint` | 1.0 | Read from `all_extensions` | Node constraints |

### VRM 0.0 vs 1.0 Differences

| Topic | VRM 0.0 | VRM 1.0 |
|-------|---------|---------|
| Extension name | `VRM` | `VRMC_vrm` |
| Facing direction | +Z | -Z (see [Coordinate Transformation](#coordinate-transformation)) |
| Morph concept | BlendShape (`blendShapeMaster.blendShapeGroups`) | Expression (`expressions.preset` + `expressions.custom`) |
| Humanoid bones | Array of `{bone, node}` under `humanoid.humanBones` | Named fields under `humanoid.humanBones` (`hips`, `spine`, ...) |
| Bind weight scale | 0..100 (divided by 100 inside `extract_morphs_v0`) | 0..1 |
| Material spec | `VRM.materialProperties` (Unity shader parameter dump) | `VRMC_materials_mtoon` per material |

### Humanoid Bone Mapping

`extract_bones` walks every `gltf::Node` and creates one `IrBone` per node (including non-humanoid nodes), so `IrModel::node_to_bone` is a full node-index → bone-index map. In parallel, `build_humanoid_map` builds a `HashMap<node_idx, vrm_bone_name>`:

- VRM 1.0: each named field on `HumanBones` (`hips`, `spine`, `chest`, `upperChest`, `neck`, `head`, 4 leg bones per side, 4 arm bones per side, 15 finger bones per side, `leftEye` / `rightEye`, `jaw`) is checked individually via an `add_bone!` macro that inserts `(node, "vrmBoneName")` into the map
- VRM 0.0: iterates `humanoid.humanBones` (array form) and inserts `(bone.node, bone.bone)` directly
- The resulting `vrm_bone_name` is stored on `IrBone.vrm_bone_name: Option<String>` and later used for rig retargeting, T-pose normalization, and animation name mapping

### Expression / BlendShape Extraction

`extract_morphs` dispatches on `VrmTyped`:

- **VRM 1.0** (`extract_morphs_v1`): iterates every preset expression (`aa`, `ih`, `ou`, `ee`, `oh`, `blink`, `blinkLeft`, `blinkRight`, `happy`, `angry`, `sad`, `relaxed`, `surprised`, `neutral`, `lookUp`, `lookDown`, `lookLeft`, `lookRight`) plus all entries in `expressions.custom`. For each, `morphTargetBinds` are resolved via a node-index → `IrMesh` index map (one node may expand to multiple IR meshes), and positions / normals / tangents from the corresponding `morph_targets` are collected per global vertex index
- **VRM 0.0** (`extract_morphs_v0`): iterates `blendShapeMaster.blendShapeGroups`. Each bind references a mesh by `mesh` index; `bind.weight` is divided by 100 because VRM 0.0 uses a 0..100 scale. The preset name is converted to a Japanese morph name via `convert::morph::preset_to_jp_v0`
- **`materialColorBinds` / `textureTransformBinds` (VRM 1.0, playback added in v0.5.1)** — `extract_morphs_v1` emits these as `IrMorphKind::Material { color_binds, uv_binds }`. An Expression with both vertex morphs and material binds is registered as **two IrMorphs with the same name** (`Vertex` + `Material`), and the name-based `morph_weights` mapping applies the same weight to both. `MaterialColorBindType` has 6 variants (`color` / `emissionColor` / `shadeColor` / `matcapColor` / `rimColor` / `outlineColor`), parsed from the VRM 1.0 `type` string via `from_vrm_str` (unknown strings log a warning and are skipped)

### Meta Info Extraction

`extract_meta_comment` flattens `VrmMeta` into a fixed-width multi-line comment (Model Info / Author / Permissions / License sections) stored in `IrModel.comment`. `extract_model_name` reads `meta.name` (V1) or `meta.title` (V0). The same fields populate the [VRM Meta Info Color Badges](#vrm-meta-info-color-badges) shown by the viewer.

### Texture Handling

- `extract_textures` reads `gltf::image::Data` and converts `R8G8B8A8` / `R8G8B8` pixels into `TextureData::RawRgba { pixels, width, height }`, bypassing PNG encode/decode round-trips
- The MIME type is recorded as the sentinel `"image/x-raw-rgba8"` so downstream code can tell pre-decoded pixels from encoded bytes
- Texture names are overwritten with sanitized `gltf::Image::name()` values when present (`sanitize_filename` keeps only alphanumerics, `_`, `-`)
- Mip chains are pre-generated on a background thread via `generate_mip_chain` (sRGB → linear f32 via LUT, resize, linear → sRGB) and stored in `IrTexture.mip_chain` — see [Mipmap Generation](#mipmap-generation) in the PMX/PMD Loading chapter for the shared design
- `read_texture_info` resolves `KHR_texture_transform` (offset / scale / rotation / texCoord) from raw JSON for MToon material slots, normalizing glTF texture index → image index

## FBX Loading

popone ships a custom FBX reader under `src/fbx/` that supports both binary and ASCII FBX, auto-detects the rig style (Mixamo / VRoid / Unreal / Blender / Max Biped / Maya HumanIK), and resolves textures from embedded Video content or external files.

### Binary vs ASCII Detection

`parser::parse` looks at the first bytes of the file:

- UTF-8 BOM (`\xEF\xBB\xBF`) is stripped first
- `; FBX` prefix → ASCII FBX → routed to `parse_ascii` (line-oriented parser)
- `Kaydara FBX Binary  \x00\x1a\x00` magic (23 bytes) → binary parser that reads nodes through a `Cursor<&[u8]>`
- Any other header → `FbxParse("Invalid FBX magic number")`

Binary nodes carry a per-version header (pre-7500 uses 32-bit offsets, 7500+ uses 64-bit), followed by properties and nested children up to a trailing zero-offset marker. ASCII nodes are parsed with indentation-agnostic `{` / `}` block matching.

### FBX Node Tree

- `FbxDocument { version, nodes }` — raw tree after parsing, owned by the caller
- `FbxNode { name, properties: Vec<FbxProperty>, children }` — recursive node; `FbxNode::child(name)` returns the first direct child with the given name
- `FbxProperty` is a typed enum covering `Bool`, `I16/I32/I64`, `F32/F64`, their array variants, `String`, and `Binary` — with convenience accessors (`as_i64_value`, `as_f64_value`, `as_string`, `as_binary`, `as_*_array`)

### FBX Scene Graph

`FbxScene::from_document` turns the raw tree into an object graph:

- Walks `Objects/*` children and builds `HashMap<i64, FbxObject>` keyed by FBX id. Each object stores `name` (truncated at the first `\x00` byte to strip the `\x00\x01Model` suffix), `sub_type` (e.g. `Mesh`, `LimbNode`, `Root`, `Null`), `class` (the FBX node name: `Geometry`, `Model`, `Material`, `Texture`, `Video`, ...), and a reference to the raw node
- Walks `Connections/C` entries and classifies them as `OO` (object-object) or `OP` (object-property). Builds `children_map` and `parents_map` (both `HashMap<i64, Vec<i64>>`) for O(1) navigation
- Helper queries: `materials_for_geometry`, `textures_for_material` (also returns the `OP` property name so `Diffuse*` slots can be identified), `video_for_texture`, `geometries`

### Rig Auto-Detection

`humanoid::detect_humanoid` runs the bone name list through `strip_namespace_lower` (strips `Model::` / `Namespace::` prefixes, lowercases) and then `detect_rig_type`. Supported rigs:

| Rig | Detection heuristic |
|-----|---------------------|
| Mixamo | Bone name starts with `mixamorig:` or `mixamorig_`, or the `Hips` + `Spine1` + `LeftArm` triplet exists |
| VRoid | Any bone named `j_bip_c_hips` or starting with `j_bip_` |
| 3ds Max Biped | Bone named `bip01` or starting with `bip01 ` |
| Maya HumanIK | Bone name starts with `hik_` |
| Unreal | Both `root` and `pelvis` are present |
| Blender | `Hips` + (`Head` or `Spine`), including Japanese `下半身` / `上半身` / `頭` |
| Unknown | None of the above |

Each rig type has its own `*_MAP: &[(&str, HumanBone)]` lookup table. The detected `RigType` and `HashMap<bone_index, HumanBone>` are returned as `HumanoidMapping`.

### Bone Hierarchy Extraction

`BoneHierarchy::from_scene` collects every `Model` whose sub-type is `LimbNode`, `Root`, or `Null`, sorted by FBX id for deterministic ordering. For each bone:

- `extract_transform` reads `Lcl Translation`, `Lcl Rotation`, `PreRotation`, `Lcl Scaling` from the `Properties70` block (all values live in `properties[4..7]` of each `P` entry)
- Euler angles are in degrees and treated as `EulerRot::ZYX` (intrinsic ZYX = extrinsic XYZ, the FBX default)
- The local rotation is composed as `pre_rotation * rotation` — FBX's PreRotation is applied before animated rotation
- `compute_world_transforms` walks parents recursively via `Connection`-derived `parent_index`, producing a world matrix per bone

`convert_bones` then remaps each bone to `IrBone` via `coord_fn` (see below), and looks up `humanoid_mapping` to fill `vrm_bone_name`. When a VRM humanoid name is assigned, `bone_map::vrm_bone_to_pmx_name` additionally sets `IrBone.name` / `name_en` to the canonical PMX Japanese / English bone names.

### Coordinate System & UnitScaleFactor

`build_coord_transform` reads `GlobalSettings.Properties70` and returns a closure `|[f32;3]| -> [f32;3]` that remaps axes and scales to meters:

- `UpAxis` / `UpAxisSign`, `FrontAxis` / `FrontAxisSign`, `CoordAxis` / `CoordAxisSign` control axis remap
- `UnitScaleFactor` (cm-based: 1.0 = 1cm, 100.0 = 1m) divided by 100 gives the meter scale (`to_meters`)
- `FrontAxis` is intentionally not flipped: FBX's FrontAxis points into the scene, but characters normally face away from it, so leaving the sign alone lands them on glTF's `-Z` forward

Mesh vertices, normals, bone world positions, and `GeometryInstance.world_transform` columns are all passed through this closure.

### GeometryInstance Extraction

`FbxScene::geometry_instances` walks every `Geometry` of sub-type `Mesh` (sorted by id) and builds:

- `model`: the first parent `Model` (warns and skips if there are zero, picks the first and warns if there are multiple)
- `world_transform`: accumulates `Model` local transforms from root to leaf, each built from `T * (PreRotation * Rotation) * S`
- `material_slots`: iterates `Connections` in order so that `Material` children of the `Model` become `MaterialSlot { slot_index, material }`. Slot indices are stable and match the FBX material index, which is what PMX / Prefab renderer paths rely on

`extract_ir_model_from_fbx_with_options` then iterates these instances, expands the raw polygon indices (negative end-of-polygon marker handling), resolves per-vertex data, and emits per-material sub-meshes.

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

### FBX External Texture Nearby Search

When `RelativeFilename` / `FileName` paths don't match the actual directory structure (common with Unity/Blender project exports), `TextureSearchCache` is used to recursively search directories near the FBX file (max depth 3). The cache is a `HashMap` of filename (lowercase) → path, targeting only image file extensions (png/jpg/tga/bmp/dds/psd, etc.). Directory scanning runs only once per conversion.

PSD files are not supported by the `image` crate, so `decode_image_data_with_ext` detects the extension at the top and decodes directly to RGBA using the built-in decoder (`psd::decode_psd`). This bypasses PNG conversion for better performance.

### Texture Resolution Pipeline

`extract_texture_for_material` walks the scene graph in the following order:

1. `textures_for_material(mat_id)` — follow `Model → Material → Texture` connections; `find_diffuse_texture` prefers slots whose `OP` property contains `Diffuse`, otherwise the first texture
2. Try embedded `Video/Content` via `video_for_texture` → `FbxProperty::Binary`. Skipped for ASCII FBX because `Content` is stored as `FbxProperty::String`
3. Try `RelativeFilename` resolved against the FBX directory (backslashes normalized to forward slashes)
4. Try `FileName` basename resolved against the FBX directory
5. Fall back to nearby search via `TextureSearchCache`

Image decoding goes through `decode_image_data_with_ext`: PSD is decoded by the built-in `crate::psd` module, other formats use `image::load_from_memory` first, then `image::load_from_memory_with_format` with an extension hint as a fallback for TGA and similar magic-less formats. Decoded textures are PNG-encoded into `IrTexture` (FBX path does not use `RawRgba`).

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
- **Path sanitization**: All disk-based texture loading paths (DirectX .x, OBJ, PMX, PMD) apply `sanitize_rel_path()` before `base_dir.join()`. This strips `..` components (preventing directory traversal) and `:` characters (preventing Windows absolute path bypass via drive letters like `C:`). Archive-based loading uses `normalize_archive_path()` instead

### Mipmap Generation

GPU textures are uploaded with a full mipmap chain. The number of mip levels is `floor(log2(max(w,h))) + 1`.

- **u8 sRGB-space resize** — `image::imageops::resize` (Triangle filter) is applied directly to `RgbaImage` in sRGB space. While linear-space resize is mathematically more correct, the visual difference is imperceptible compared to the overhead of f32 conversion (256MB allocations + `powf` calls), so speed takes priority
- **NPOT support** — Each level dimension is `max(1, dim >> level)`, supporting non-power-of-two textures
- **GPU max size** — Textures exceeding `max_texture_dimension_2d` are pre-downscaled using the same sRGB-correct resize before mip generation
- **Sampler** — `mipmap_filter: Linear` was already set, now effective with multiple mip levels
- **Anisotropic filtering** — `anisotropy_clamp: 16` added to all texture samplers (`default_sampler`, `create_sampler_from_info`, `ensure_sampler`). Improves texture sharpness on oblique surfaces. Applied only when all three filter modes (mag, min, mipmap) are `Linear` (wgpu/WebGPU spec requirement); samplers with `Nearest` filters use `anisotropy_clamp: 1`
- **Background pre-generation** — For VRM/GLB, the mip chain is pre-generated on a background thread via `vrm::extract::generate_mip_chain()` and stored in `IrTexture.mip_chain: Option<Vec<(u32, u32, Vec<u8>)>>`. The main thread's `upload_rgba_to_gpu_with_mips` simply transfers each level via `queue.write_texture`. For KizunaAI_KAMATTE.vrm (26 × 4K textures), `upload_textures_from_ir` execution time drops from 7.3s to 197ms

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

| Format | Default Unit | Default Coordinate System | Default Conversion |
|--------|-------------|--------------------------|------------|
| OBJ | cm | Y-Up right-hand | ÷100 (cm → m) only. No axis conversion |
| STL | mm | Z-Up | ÷1000 (mm → m) + Y↔Z swap + face winding reversal (b↔c swap) |

- Y↔Z swap has determinant = -1 → face winding reverses, requiring b↔c swap
- After conversion: glTF space (Y-Up right-hand, meters) → viewer applies `gltf_pos_to_pmx` (×12.5) for PMX units

**Import options dialog**: The viewer now shows an import settings dialog for OBJ/STL files, allowing the user to select the coordinate unit (mm / cm / m / inch → scale factors 0.001 / 0.01 / 1.0 / 0.0254) and Z-Up → Y-Up conversion toggle. `load_obj_with_params` / `load_stl_with_params` accept `scale: f32` and `z_up: bool` parameters. CLI retains the default behavior. The `ImportUnit` enum and `PendingImportOptions` struct are defined in `viewer/app/pending.rs`

### IrModel Construction

- Static mesh: single root bone ("全ての親"), all vertex weights `(0, 1.0)` as BDEF1
- OBJ: meshes split per material (tobj Model unit). MTL `Kd`/`Ks`/`Ns`/`d` → `IrMaterial`, `map_Kd` → `IrTexture`
- STL: single default white material. No textures or UVs. Flat shading (3 independent vertices per triangle)

### Dynamic Grid

- `compute_grid_params()` auto-calculates grid extent and step from model bbox
- Default (extent=100, step=5) is the minimum; only enlarged when bbox exceeds ±100 PMX units
- Rounded to nice values: extent → 200, 500, 1000, ...; step → 10, 20, 50, ...
- `GpuRenderer::rebuild_grid()` rebuilds GPU buffer (on model load + append)

## DirectX .x Loading

### Parser (`directx/parser.rs`)

- Text format only (`xof 0303txt 0032` header). Binary/compressed formats detected and rejected with clear error
- UTF-8 / Shift_JIS auto-detection (`encoding_rs`)
- Tokenizer: `{` `}` `;` `,` + Ident + Num + Str. `<UUID>` auto-skipped
- Dot-separated names (`Cube.001`): `read_optional_name()` concatenates Ident+Num tokens until `{`
- Supported templates: `Frame`, `FrameTransformMatrix`, `Mesh`, `MeshNormals`, `MeshTextureCoords`, `MeshMaterialList`, `Material`, `TextureFilename`
- Unknown templates: skipped by counting `{` `}` brace depth
- `SkinWeights` / `XSkinMeshHeader` detection: `has_skin_weights` flag triggers error in extract
- Material references: `{ MaterialName }` resolved via `global_materials` table. Named Materials inside `MeshMaterialList` also registered. Forward references stored in `unresolved_refs` and re-bound in 2nd pass
- Declared material count > resolved count: padded with placeholder gray materials
- Quads and n-gons: fan subdivision to triangles

### IrModel Conversion (`directx/extract.rs`)

- **Coordinate conversion**: DirectX left-hand Y-Up → glTF right-hand Y-Up. Position `(x, y, -z) × 0.8`, normal `(x, y, -z)`
 - Scale 0.8 = 10 / PMX_SCALE(12.5): PMX output is 10× original coordinates
- **Frame hierarchy**: `compute_world_transform()` walks parent chain to accumulate world matrix. Normals transformed with inverse-transpose
- **Face winding**: Z-flip (det=-1) × world transform determinant dynamically determines swap
- **Hard edges**: `(position_index, normal_index)` key for vertex deduplication. Same position with different normals creates separate vertices
- **Missing normals**: `compute_face_normals()` auto-generates smooth shading normals from face normals (computed with post-swap indices)
- **UV**: DirectX V → `1.0 - v` flip
- **Texture resolution** (`resolve_texture`):
 - Archive/snapshot source: raw path exact match → normalized exact match → case-insensitive. Disk fallback disabled
 - Normal file: `base_dir.join(rel)` direct disk read (`..` paths preserved for OS resolution)
 - `IrTexture.filename` normalized to filename only (prevents path traversal in PMX export)
- **Bones**: Single root bone "ルート". All vertex weights BDEF1. Material-less meshes share a lazy-initialized default material
- **DDS textures**: `mime_for_ext` registers `image/vnd.ms-dds`. Decoded via `image` crate `dds` feature

## UnityPackage Loading

This chapter covers loading models from `.unitypackage` archives, including Prefab-based texture mapping. It describes how popone walks the Unity GUID reference chain, resolves Prefab variants, and maps textures from `.prefab` files to the contained FBX models.

### GUID Reference Chain

```
.prefab → m_SourcePrefab / m_Mesh (FBX GUID)
 → FBX .meta → externalObjects (material name → .mat GUID)
 → .mat → m_TexEnvs → _MainTex (texture GUID)
 → texture file
```

### UnityPackageIndex

GUID-based index structure providing O(1) lookup from GUID to pathname, data, and meta.
also includes pre-built Prefab caches for O(1) texture resolution.

```rust
pub struct UnityPackageIndex {
 pub entries: Vec<AssetEntry>,
 pub by_guid: HashMap<String, usize>,
 pub by_path: HashMap<String, usize>,
 /// FBX GUID → .prefab entry indices
 pub prefab_by_fbx_guid: HashMap<String, Vec<usize>>,
 /// Parsed Prefab cache: entry index → ParsedPrefabCache
 pub prefab_cache: HashMap<usize, ParsedPrefabCache>,
 /// Variant resolution cache: source GUID → resolved FBX GUIDs
 pub variant_cache: HashMap<String, Vec<String>>,
}
```

`build_unity_package_index()` extracts the tar.gz once, then all subsequent lookups use `by_guid` / `by_path`.
Built for both direct viewer loading and archive (ZIP / 7z) loading paths.

After index construction, `build_prefab_fbx_map()` populates the Prefab caches:
1. **Phase 1 (parallel)**: All `.prefab` entries are parsed via `rayon::par_iter` (`detect_prefab_format` + `parse_prefab_new` + `parse_prefab_old`). Mixed-format Prefabs always run both parsers.
2. **Phase 2a**: Parsed results are inserted into `prefab_cache`.
3. **Phase 2b (sequential)**: Variant resolution (`resolve_variant_multi`) maps each Prefab's FBX references into `prefab_by_fbx_guid`. Results are cached in `variant_cache`.

`resolve_prefab_textures()` uses `prefab_by_fbx_guid.get(fbx_guid)` for O(1) lookup instead of scanning all entries, and reads from `prefab_cache` instead of re-parsing.

`TextureData::Encoded` uses `Arc<[u8]>` to share texture data from `PackageTexture.data` without copying.

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
| `[B]` | `emissive_per_mat` | Batch toggle emissive for emissive materials. When OFF, `emissive_factor` is zeroed, disabling both `lit += emissive` and MRT bloom output |
| `[☑]` | `material_visibility` | Batch toggle visibility for all DrawCalls in the group |

Header row hover detection uses `contains_pointer()` (rect-based). `hovered()` is not suitable because child widgets (buttons, etc.) consume the hover event.

### GeometryInstance-Based source_material

FBX extraction now uses `FbxScene::geometry_instances()` instead of `scene.geometries()` to iterate over meshes. Each `GeometryInstance` provides:
- `model` — the parent Model node (for hierarchy path computation)
- `world_transform` — pre-computed world transform (replaces `compute_geometry_world_transform`)
- `material_slots` — Connection-ordered materials with `slot_index`

For each material, `SourceMaterialRef { renderer_path, slot_index }` is set using `model_hierarchy_path(inst.model.id)`. This enables Strategy 1 (source_material matching) in `embed_textures_with_prefab`, where the resolver matches Prefab renderer paths to FBX Model hierarchy paths.

**Three-stage fallback in `embed_textures_with_prefab`:**
1. **source_material** — exact match via `SourceMaterialRef` (renderer_path + slot_index)
2. **material_name** — name-based match with case-insensitive and suffix fallback
3. **source_texture_name** — legacy filename-based match

### link_same_name Scope Restriction

`LoadedModel::same_name_siblings(mat_idx)` restricts same-name material linking to the `MaterialGroup` containing `mat_idx`. This prevents cross-instance propagation when the same FBX is appended multiple times.

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

- **Path traversal defense**: `normalize_archive_path` rejects `..` and absolute paths in archive entries. Direct disk loading uses `sanitize_rel_path()` to strip `..` and drive letters
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

## Drag & Drop Loading

### Temp Path Detection

`is_temp_path()` checks whether the path is under `std::env::temp_dir()` using a two-stage approach:

1. **canonicalize-based** (when file exists): Normalizes via `canonicalize()`, absorbing symlink and drive letter case differences
2. **String-based fallback** (after file deletion): Normalizes case via `to_string_lossy().to_lowercase()`, ensures path boundary via `MAIN_SEPARATOR` suffix before `starts_with` comparison (prevents false positives like `TempBackup`)

The fallback is necessary to handle cases where temp files from zip archive D&D are immediately deleted.

### Temp Path Byte Prefetch

When `is_temp_path()` returns true in `process_drag_and_drop()`, the path is submitted through the normal `PendingLoadDispatch` path, but before submission the main thread runs `std::fs::read()` + `collect_image_files_recursive()` to cache the model body and aux files into `PreloadedData`. The cache is embedded in `PendingLoadDispatch.preloaded` and passed to the BG thread, making it safe even if the temp file is deleted before the BG thread starts parsing. Non-temp D&D uses the same path with `preloaded: None`.

### D&D Preload Cache (PreloadedData)

When a temp path is detected in `process_drag_and_drop()`, the model body and adjacent file bytes are cached in `PreloadedData`, eliminating disk access throughout the entire load chain.

```rust
/// D&D temp file preload data
pub struct PreloadedData {
 path: PathBuf, // Original temp file path
 main_bytes: Arc<[u8]>, // Model body bytes
 aux_files: HashMap<PathBuf, Arc<[u8]>>, // Adjacent image files (relative path keys)
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
 3. self.pending.load_dispatch = Some(PendingLoadDispatch {
 path, append, overlay: WaitingOverlay,
 preloaded: Some(PreloadedData { ... })
 })
 4. Frame N: update_progress_flags → overlay: Ready
 5. Frame N+1: process_pending_tasks → route_load_dispatch
 - self.preloaded = dispatch.preloaded (maintains compatibility with existing methods)
 - Format detection, FBX choice, spawn_bg_load routing

FBX selection dialog path:
 route_load_dispatch() → PendingFbxChoice { preloaded: self.preloaded.take() }
 → execute_fbx_choice() → self.preloaded = choice.preloaded (restore)
 → try_load_fbx() → read_or_preloaded() uses cache
 → self.preloaded = None (clear)

Background load path:
 route_load_dispatch() → spawn_bg_load(dispatch, format)
 → std::thread::spawn runs cpu_parse_source(path, format, ..., preloaded)
 → PreloadedData ownership is moved to the thread (main_bytes is Arc<[u8]>, shared cheaply)
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
 1. was_temp = is_temp_path(&path) ← Determined while file exists (canonicalize prerequisite)
 2. data = std::fs::read(&path) ← Read byte data
 3. upload_texture_from_bytes(&data) ← Create GPU texture
 4. PendingTexPreview { cached_data: data, is_psd, was_temp, ... }

apply_tex_preview:
 1. tex_data = preview.cached_data.clone() ← From cache (no re-read)
 2. is_psd = preview.is_psd ← From cache
 3. cached_data = if preview.was_temp { Some(...) } else { None }
 4. Branch to TextureSource::Cached or File
```

**Important**: `is_temp_path` evaluation must occur before `std::fs::read`. Since `canonicalize()` requires file existence, evaluating after read risks the file being deleted, causing the check to fail.

### .gltf Exclusion

`.gltf` files have external buffer references (`.bin`, image files), so they are excluded from snapshotting. `gltf::import_slice` cannot resolve external URIs, so the normal `load_glb(path)` path is used.

## Asynchronous Model Loading

Model parsing and GPU resource construction are split into a CPU phase (background thread) and GPU phase (main thread), eliminating UI freezes.

### Data Flow

```
1. Trigger (file dialog result / D&D / IPC / command-line arg)
 → pending.load_dispatch = Some(PendingLoadDispatch {
 path, append, overlay: WaitingOverlay, preloaded
 })

2. Frame N: update_progress_flags()
 → overlay: WaitingOverlay → Ready
 → paint_progress_overlay shows "Loading..."

3. Frame N+1: process_pending_tasks() → poll_dispatch_and_bg_load()
 → Extracts PendingDispatch { dispatch, prior_loading } from bg_state
 → route_load_dispatch(dispatch, prior_loading)
 - Format detection
 - .vrma / .glb/.gltf animation / .anim → immediate (no BG, preserves prior_loading)
 - FBX (mesh+anim) → PendingFbxChoice dialog
 - UnityPackage / zip / 7z → spawn_bg_index_load()
 - Otherwise → spawn_bg_load()
 → All spawn_bg_* functions delegate to spawn_bg_task() common helper
 → pending.bg_state = BackgroundLoadState::Loading(BgLoadHandle { rx, cancel, request_id })

4. Frame N+2 onward: process_pending_tasks() → poll_dispatch_and_bg_load()
 → Polls via handle.rx.try_recv() from BackgroundLoadState::Loading(handle)
 → Ok(Ok(result)) → apply_bg_load_result()
 → Ok(Err(e)) → error display
 → Err(Empty) → continue waiting
 → Err(Disconnected) → thread panic error
```

### Key Types (pending.rs)

| Type | Description |
|---|---|
| `BackgroundLoadState` | BG load state machine with 3 variants: `Idle` / `PendingDispatch { dispatch, prior_loading }` / `Loading(BgLoadHandle)`. Replaces the prior two-field `load_dispatch` + `bg_load` combination to express exclusivity at the type level |
| `PendingLoadDispatch` | Load reservation. Contains `path` / `append` / `overlay` / `preloaded` |
| `BgLoadHandle` | BG load handle. `rx: mpsc::Receiver<Result<BgLoadResult>>` / `cancel: Arc<AtomicBool>` / `request_id: u64` |
| `BgLoadResult` | BG parse result. `ir: IrModel` / `source: ReloadableSource` / `kind: BgLoadKind` / `path` / `request_id: u64` |
| `BgLoadKind::Initial { format, auto_fbx_anim }` | Regular load |
| `BgLoadKind::Append` | Append load |

### `cpu_parse_source` Free Function (file_io.rs)

A pure function that doesn't take `&self`, safe to call from background threads. Provides unified parsing logic for each format (VRM / FBX / PMX / PMD / OBJ / STL / DirectX .x), returning `(IrModel, ReloadableSource)`. takes a `CpuParseInput` enum as the first argument instead of separate `path` / `format` / `preloaded` parameters. Currently only `CpuParseInput::File { path, format, preloaded }` is implemented; `ArchiveEntry` / `Reload` variants are planned for future background archive parsing. Also takes a `cancel: &Arc<AtomicBool>` argument and checks the cancel flag at multiple points within each format arm.

### `route_load_dispatch` Method

Dispatches on the main thread:
- **Immediate**: VRMA, GLB/glTF animation, .anim (no model load, no GPU resource ops)
- **Interactive UI**: FBX choice dialog (keeps `self.preloaded = dispatch.preloaded` for existing method compatibility)
- **Background**: VRM / FBX / PMX / PMD / OBJ / STL / DirectX .x / UnityPackage / ZIP / 7z (all formats)

### `apply_bg_load_result` Method

Post-processes BG results on the main thread:
- **Initial**: `finish_load(ir, source)` → animation state clear → FBX auto-animation
- **Append**: Coordinate system compatibility check (rejects if `host_fmt.is_vrm0() != other_fmt.is_vrm0()`) → `finish_append_with_source`

### Multi-thread Safety

- **Send boundary**: `IrModel` / `ReloadableSource` / `PreloadedData` are all `Send` (POD + `Arc<[u8]>`)
- **GPU access restriction**: `cpu_parse_source` is a free function that never references `wgpu::Device` / `Queue`. GPU operations only happen in `finish_load` on the main thread
- **egui::Context thread safety**: egui 0.31's `Context` is implemented as `Arc<RwLock<ContextImpl>>` and is `Send + Sync`. `ctx.request_repaint()` is callable from BG threads
- **Double load**: When a new `load_dispatch` is submitted, the old `bg_load` receiver is dropped. The thread runs to completion but `tx.send()` returns `Err` and the result is discarded. An earlier design used a stopgap "reject new dispatches while a prior load is in progress" rule
- **`spawn_bg_task` common helper**: Extracted shared boilerplate (old-task cancellation, `request_id` allocation, `mpsc` channel creation, `std::thread::spawn` + `cpu_parse_source` invocation) from `spawn_bg_load` / `spawn_bg_index_load` / `spawn_bg_archive_load` / `spawn_bg_pkg_load` into a single `spawn_bg_task` helper. Each caller now only builds `CpuParseInput` and `fallback_kind`
- **Double load cancellation**: `route_load_dispatch` was switched from "reject" to "cancel and accept". It sets the old `BgLoadHandle.cancel: Arc<AtomicBool>` to `true` and then calls `spawn_bg_load` for the new request. The old thread bails out at its next cancel check point (currently the start of `cpu_parse_source`) with `"bg load cancelled"`, and the receive side logs it via `log::info!` only (not surfaced to the UI) since cancellation is intentional
- **Generation tracking**: `ViewerApp.next_request_id: u64` is monotonically incremented (via `wrapping_add(1)`) by each `spawn_bg_load` call, and the id is embedded in both `BgLoadHandle.request_id` and `BgLoadResult.request_id`. The receiver verifies `handle.request_id == result.request_id`, discarding the result as stale if they differ (while keeping the handle so the current-generation result is still awaited). This prevents the race where an old thread manages to send its result just before cancellation takes effect, which would otherwise overwrite the current-generation model
- **FBX reload temp directory**: The Snapshot-reload path that writes FBX external textures back to disk uses `tempfile::Builder::new().prefix("popone_fbx_reload_").tempdir()?` so each invocation gets a unique name, avoiding collisions during concurrent reloads. `TempDir::Drop` handles automatic cleanup

### Raw RGBA Texture Bypass

Optimization to avoid PNG encode/decode roundtrip during VRM/GLB load.

- **`TextureData` enum**: `IrTexture.data` is now a `TextureData` enum with two variants: `Encoded(Vec<u8>)` for PNG/JPEG/TGA etc., and `RawRgba { pixels, width, height }` for decoded VRM/GLB pixels. This replaces the previous `mime_type == "image/x-raw-rgba8"` string check and the separate `raw_dims: Option<(u32, u32)>` field. `TextureData` provides `as_bytes()`, `len()`, `is_empty()` methods for transparent access
- **`IrTexture::is_raw_rgba()`**: Uses `matches!(self.data, TextureData::RawRgba { .. })`
- **`IrTexture::raw_dims()`**: Returns `Some((width, height))` for `RawRgba`, `None` for `Encoded`
- **`upload_textures_from_ir`**: Matches on `TextureData::RawRgba` directly, uploading pixels to GPU without decoding
- **`write_all_textures_from_ir` (PMX export)**: Matches on `TextureData::RawRgba` to encode to PNG via `image::RgbaImage::save`

### Deferred GPU Build

After BG parsing completes, GPU texture upload and model construction are split across frames to avoid blocking the UI thread:

**Frame-split texture upload:**
- `start_deferred_gpu_build()` creates `PendingGpuBuild` with merged IR and per-material display flags
- `process_pending_tasks` (split into `poll_*` methods) uploads `GPU_UPLOAD_BATCH` (4) textures per frame via `upload_single_texture`
- Progress overlay shows "GPU構築中..." during upload

**Completion:**
- When all textures are uploaded, `build_gpu_model_inner` constructs vertex/index buffers
- For initial load: `finish_load_with_gpu` + `apply_gpu_build_post` (pkg_material_keys, MaterialGroup, etc.)
- For append: `finish_deferred_append` reconstructs `LoadedModel` from saved metadata + new GPU model

**Append rollback:**
- `AppendGpuBuildInfo` stores the old `GpuModel` and pre-merge IR sizes. Rollback state is organized into typed snapshots: `IrRollbackSnapshot` (IR array sizes + metadata), `LoadedModelOwnership` (source, appended_models, material_groups etc.), `AnimationSnapshot` (playback state). This ensures `LoadedModel` / `IrModel` field additions only need updates in one place
- On GPU build failure, `rollback_append` (via `IrRollbackSnapshot::rollback()`) truncates the merged IR to pre-merge size and restores the old `GpuModel`

**Load cancellation:**
- Cancel button ("中止") and Escape key trigger `cancel_bg_load`
- Cancels BG thread via `AtomicBool`, clears all pending state, sets `self.loaded = None`
- `pre_decode_textures` checks cancel flag per-texture to avoid prolonged CPU usage after cancel
- GPU build phase: cancel button triggers `cancel_gpu_build`, clearing `PendingGpuBuild`
- Reload cancel: `restore_snapshot_on_failure` restores previous model instead of clearing to empty

**Background PMX conversion:**
- `execute_conversion` clones the IR via `clone_for_export` (strips `mip_chain`/`uvs1`) and spawns a background thread
- `convert_ir_to_pmx_with_cancel` writes all output to a temp directory (`.popone_convert_tmp/`), managed by `TmpDirGuard` RAII — Drop deletes the temp dir automatically on cancel/error/panic, `disarm()` on success path prevents cleanup
- Cancel flag checked: before each step, per-texture during texture export, per-section during PMX write via `write_model_opt_cancel` (checks between vertices/faces/textures/materials/bones/morphs sections)
- On success: `TmpDirGuard` disarmed, files moved from temp directory to final output path
- On cancel/error: `TmpDirGuard` Drop deletes temp directory entirely, no partial output remains
- `PendingConvertBg` holds `mpsc::Receiver` for result polling and `AtomicBool` for cancel
- `TextureData::RawRgba::pixels` changed to `Arc<[u8]>` — cloning IR for BG thread is near-zero-cost for textures

**Background reload:**
- `reload_current` dispatches File/Snapshot sources through existing `spawn_bg_load` pipeline
- Archive/UnityPackage sources remain synchronous (complex state management)
- `reload_snapshot: Option<ReloadSnapshot>` stored on `ViewerApp` before dispatch
- On GPU build completion: `finish_reload_from_snapshot` restores camera, morphs, visibility, animations
- On cancel: `restore_snapshot_on_failure` restores previous model and state

**Selection dialog Escape key:**
- FBX choice, OBJ/STL import options, UnityPackage select, and archive select dialogs accept Escape key as cancel

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

### Prefab Append Loading

Prefab models can now be appended to an already-loaded scene. The `append_from_pkg` function handles the `PkgModelType::Prefab` branch:

1. `resolve_single_prefab(pkg, model_index)` resolves the Prefab's GUID reference chain to discover all referenced FBX entries
2. For each resolved FBX: extract data from `pkg.entries`, run `extract_ir_model_from_fbx_with_options`, and apply `embed_textures_with_prefab` for GUID-based texture mapping
3. Multiple FBX results are merged into a single `IrModel` via `ir.merge()`
4. The merged `IrModel` is returned to `finish_append_ext`, which handles GPU model rebuild and `MaterialGroup` creation

The function returns `bool` to signal success/failure. On failure, the caller clears `PendingMultiLoad` to abort any remaining batch.

### Multi-Model Batch Loading

The `.unitypackage` model selection dialog supports multi-select via checkboxes:

**UI Flow:**
- Each model entry has a checkbox alongside the existing click-to-load button
- "Load selected (N)" button triggers batch loading
- Single-click loading is preserved for quick single-model selection

**Queue Architecture:**
- `PendingMultiLoad` holds a single `Arc<Vec<ExtractedAsset>>` and a `Vec<(usize, PkgModelType)>` of remaining models
- The first selected model is loaded via `PendingPkgModelLoad` (normal load or append depending on context)
- `process_pending_tasks` dequeues one model per frame from `PendingMultiLoad` into `pkg_load`, creating a new `PendingPkgModelLoad` with `Arc::clone` (reference count only, no data copy)
- Dequeue is gated on both `pkg_load.is_none()` and `fbx_choice.is_none()` to prevent advancing while a load-mode dialog is open

**Zero-Copy Asset Sharing:**
- `take_fbx_and_textures` / `take_vrm` changed from `Vec<ExtractedAsset>` (consuming) to `&[ExtractedAsset]` (borrowing)
- `PendingPkgModelLoad.assets` / `PendingFbxChoicePkg.assets` changed to `Arc<Vec<ExtractedAsset>>`
- Result: N models share one asset list; no `ExtractedAsset` cloning occurs

**Progress Toast:**
- `PendingMultiLoad.total_count` tracks the total number of selected models
- `PendingPkgModelLoad.batch_progress: Option<(usize, usize)>` carries `(current, total)` from queue pop time
- Progress is stored in `PendingPkgModelLoad` itself (not derived from `multi_load`) to survive cleanup of the last item
- Toast shows "読み込み中 (N/M)：filename" at load start, "読み込み完了 (N/M): filename" on success

**Abort Behavior:**
- If any model load fails (returns `Err` or `append_from_pkg` returns `false`), `multi_load` is set to `None`
- If the FBX load-mode choice dialog is cancelled, `multi_load` is also cleared

## Model Reload

Reload re-parses the currently loaded model from its original source (file, in-memory snapshot, or enclosing archive) and re-applies manual texture assignments. It is invoked from the reload button, from A/T-stance conversion toggles, and from other settings that must rebuild the IR. This chapter centralizes reload data types, the reload dispatch flow, per-format reload paths, and texture restoration details.

### ReloadableSource enum

An enum that tracks the model's loading source. Solves the temp file reload problem.

| Variant | Description |
|---------|-------------|
| `File(PathBuf)` | Normal file path. Re-reads file on reload |
| `Snapshot { original_path, main_bytes: Arc<[u8]>, aux_files }` | Snapshot from temp file. Restores from memory on reload |
| `Archive { original_path, archive_bytes, selected_entry_path, inner_kind }` | Model inside archive. Re-extracts archive and re-selects same model on reload |

### reload_from_source

Bypasses `load_file()` UI branching (FBX mesh+animation selection dialog, etc.) and directly calls `try_load_*` from `ReloadableSource`. Returns `Result`; on failure, restores saved state and returns early.

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
 1. is_temp = is_temp_path(path) ← Evaluated before std::fs::read
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

**Index consistency fix:** `reload_append_unitypackage` previously called `extract_all_assets()` to build the `assets` array and then separately called `build_unity_package_index()` for Prefab resolution. Both functions internally iterate a `HashMap<String, String>` (`pathnames`) whose iteration order is non-deterministic in Rust. This meant the asset index found by `find_asset_by_pathname(&assets, ...)` could refer to a different entry in the separately-built `UnityPackageIndex`, causing `resolve_single_prefab` to parse an unrelated file (e.g. a `.shader`) as a Prefab. The fix builds `UnityPackageIndex` once and derives `assets` from `pkg_index.entries`, matching the pattern in `try_load_unitypackage_for_append`.

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

### Reload Stable Key: PkgModelLocator

Reload paths now use `selected_pkg_model: Option<PkgModelLocator>` (GUID + pathname) for model re-selection, preventing misidentification when multiple models share the same basename.

**Lookup priority:**
1. `PkgModelLocator.guid` → `UnityPackageIndex.by_guid` (Prefab path)
2. `PkgModelLocator.pathname` → `find_asset_by_pathname` (ExtractedAsset path)
3. `selected_fbx_name` → basename match (legacy fallback)

`AppendedModel.pkg_model` stores the locator for each appended model, ensuring reload re-selects the correct model.

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

### Reload User State Preservation (v0.3.0)

Reloads triggered via `reload_current()` go through the same `finish_load_with_gpu` path as fresh loads, so values that should be "reset on fresh load but preserved on reload" — the side panel tab, stance conversion flags, and the user-edited model display name — were being lost unintentionally. v0.3.0 addresses this via two channels:

#### ① `ReloadSnapshot` extensions

In addition to the existing `pmx_output_path` / `material_display` / `camera` fields, the following are now saved and restored:

| Field | Reason |
|---|---|
| `side_panel_tab: SidePanelTab` | `finish_load_with_gpu` resets it to `Info`; `restore_snapshot_on_success` writes it back so toggling A/T stance on the `[出力]` tab stays on that tab |
| `model_display_name: String` | Preserves the model name the user typed in the top bar / right panel (which doubles as the PMX output filename) |

Restoration order:

```
reload_current()
 → save_reload_snapshot() copies side_panel_tab / model_display_name
 → BG path identical to fresh load → finish_load_with_gpu() resets both
 → restore_snapshot_on_success() writes them back from the snapshot
  - self.side_panel_tab = snap.side_panel_tab
  - self.export.model_display_name = snap.model_display_name
  - self.refresh_derived_from_display_name() regenerates window title + pmx_output_path
```

Fresh loads (file dialog, D&D, IPC, etc.) do not go through `save_reload_snapshot()`, so `finish_load_with_gpu`'s `Info` reset stays in effect and the side panel starts from the `[情報]` tab as before.

#### ② `PendingLoadDispatch::is_reload` flag

`route_load_dispatch()` is the shared entry for both fresh loads and reloads, and unconditionally reset `self.normalize_pose = false` / `normalize_to_tstance = false` just before calling `spawn_bg_load` — intended as a safety reset for fresh loads. Because reloads also traverse this path, the stance flags were always `false` by the time the BG parse ran, and `extract_ir_model_with_options(..., normalize_pose=true)` never took the A-stance branch. Result: A/T stance conversion did nothing.

Fix: add `is_reload: bool` to `PendingLoadDispatch`. Only dispatches originating from `reload_current()` set it to `true`, and `route_load_dispatch` skips the reset when the flag is set:

```rust
if !is_reload {
    self.normalize_pose = false;
    self.normalize_to_tstance = false;
}
self.spawn_bg_load(path, BgLoadKind::Initial { ... }, format);
```

The other four dispatch call sites (file dialog, D&D, IPC, startup `initial_file`) all pass `is_reload: false` explicitly, preserving the original "fresh loads reset stance flags" intent.

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

## GPU Pipeline Warm-up & Model Build Optimization

Eliminates main thread freezes during model loading by splitting work across frames and background threads.

### Pipeline Warm-up (`WarmupPhase`)

During splash screen display (`self.loaded.is_none()`), GPU pipelines are pre-compiled one phase per frame:

| Phase | Action | Typical Time |
|---|---|---|
| `NotStarted` → `RendererCreated` | `GpuRenderer::new()` — shader module compilation (naga WGSL) | ~50ms |
| `RendererCreated` → `SrgbMsaaDone` | `ensure_pipelines(sRGB, MSAA=true)` — 26 render pipelines | ~15ms |
| `SrgbMsaaDone` → `SrgbNoMsaaDone` | `ensure_pipelines(sRGB, MSAA=false)` | ~12ms |
| `SrgbNoMsaaDone` → `Complete` | `ensure_pipelines(Unorm, MSAA=true)` | ~7ms |

Previously these compiled lazily on first `render_to_texture()`, causing a ~10s freeze on first model load. `ensure_pipelines` was refactored to accept an explicit `msaa: bool` parameter.

### GPU Model Build Split (`cpu_prep_model` / `gpu_finalize_model`)

`build_gpu_model_inner` (mesh.rs) was decomposed into two phases:

```
PendingGpuBuild state machine:
 Phase 1: Texture upload (4/frame, frame-split) — main thread, interruptible
 Phase 2: cpu_prep_model (BG thread via mpsc) — vertex dedup, normals, morphs
 Phase 3: gpu_finalize_model (main thread, <7ms) — buffers + bind groups
```

- **`cpu_prep_model`**: Pure CPU, `Send`-safe. Produces `CpuPrepResult` containing pre-processed vertices, indices, `CpuDrawPlan` per material (with `MaterialParams` and `AuxTexRefs`), morph data, bbox, edge scales. `IrModel` is moved to the BG thread via `std::mem::take` and returned with the result. Accepts `MaterialBuildFlags` struct instead of 4 separate parallel slices (`smooth_per_mat`, `clear_per_mat`, `normal_map_per_mat`, `emissive_per_mat`)
- **`gpu_finalize_model`**: Creates bind group layouts, sampler cache, default textures, then iterates `CpuDrawPlan`s to create bind groups and buffers. O(materials) GPU API calls only
- **`MaterialBuildFlags`**: Per-material display flags struct consolidating `smooth`, `clear`, `normal_map`, `emissive` slices. Used across `build_gpu_model` / `build_gpu_model_from_ir` / `cpu_prep_model` / `PendingGpuBuild`. `default_for(mat_count)` generates default values in one place

### Incremental Thumbnail Cache

`apply_pkg_append_post` previously called `rebuild_pkg_thumb_cache()` which decoded and uploaded ALL pkg texture thumbnails from scratch on every append. With `append_pkg_thumb_cache(start_index)`, only newly added textures generate thumbnails. Eliminates cumulative O(N²) cost during batch append.

### IR Texture Thumbnail Cache (v0.5.2)

To display 32px thumbnails of assigned textures in each material editor section, `TextureState.ir_thumb_cache: Vec<Option<egui::TextureId>>` is maintained parallel to `loaded.ir.textures`. It reuses the same 64px thumbnail pipeline as `pkg_thumb_cache` (`create_thumbnail_rgba` + `upload_rgba_to_gpu` + `register_native_texture`).

| Method | Purpose |
|---|---|
| `rebuild_ir_thumb_cache` | Rebuild the cache from scratch for all entries in `loaded.ir.textures` |
| `append_ir_thumb_cache(start)` | Append only entries at and after `start` |
| `clear_ir_thumb_cache` | Release all retained `TextureId`s via `free_texture` |
| `sync_ir_thumb_cache` | Length comparison dispatches between append / rebuild / clear |

Common inconsistency cases and mitigations:

- **Stale thumbnails after model swap**: When the previous and next models have the same texture count, `sync` early-returns on length comparison and shows the previous model's `TextureId`s. Mitigation: `finish_load_with_gpu` and load-cancel paths call `clear_ir_thumb_cache()`.
- **Missed update after async PSD→PNG**: In-place texture updates preserve length, so `sync` does not rebuild. Mitigation: `poll_pending_psd_conversions()` releases and regenerates the `TextureId` at the converted index on completion.
- **Index misalignment before material editor opens**: A blind `push` on an empty cache places the new thumbnail at index 0 instead of at `ir.textures.len() - 1`. Mitigation: `assign_texture_core` and `apply_tex_preview` perform a self-healing "append the missing prefix from cache_len up to ir.textures.len()" sync instead of a plain push.

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

## MMD Rendering

MMD rendering mode that auto-enables on PMX/PMD load.

### Architecture

- **RenderStyle enum** — Per-DrawCall `Standard` / `Mmd` determination (based on material's `source_format.is_pmx_pmd()`). Works correctly with append-mixed models
- **Per-frame sRGB/Unorm switching** — PMX/PMD-only frames (all visible materials are MMD) use `Rgba8Unorm` render target for correct gamma-space alpha blending. Falls back to `Rgba8UnormSrgb` when VRM is mixed
- **4 pipeline sets** — `(MSAA on/off) × (sRGB/Unorm)` = 4 sets, lazily created on first use via `ensure_pipelines()` (previously all compiled at startup). Runtime cost is pipeline reference switching only
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
 out_a = tex.a * mat.alpha

 // Sphere map (RGB only, no alpha influence)
 // sphere_uv: X-inverted coord → vn_x * -0.5 + 0.5, vn_y * -0.5 + 0.5
 sph = sphere_texture(sphere_uv).rgb
 out_rgb += sph // add mode
 out_rgb *= sph // mul mode

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
OPAQUE (cutoff < -0.75): Returns texture alpha as-is (PMX/PMD transparency support)
MASK (cutoff >= -0.25): AlphaToCoverage + fwidth smoothing
BLEND (else): Discard fully transparent pixels only
```

Override modes (Unlit / GGX / Normal) output texture alpha directly without `apply_alpha_mode`, ensuring PMX/PMD OPAQUE materials still show texture transparency.

### State Normalization

`normalize_shader_state()` is called on all model load / rebuild / append paths, and also on UI shader selection changes. Only Auto mode auto-sets `use_mmd_path` based on model format. Explicit user selections are preserved across model loads.

The edge drawing UI (ON/OFF toggle and thickness slider) is shown both when MMD mode is explicitly selected and when Auto mode has `use_mmd_path == true` (i.e., PMX/PMD loaded).

## MToon Shading

VRM MToon materials use a fragment shader branch within the Standard pipeline for 2-color toon shading + rim lighting + MatCap, and a dedicated pipeline for outline rendering.

### MaterialUniform

```rust
// 448 bytes (gpu.rs)
pub struct MaterialUniform {
 pub diffuse: [f32; 4], // Base color (16 bytes)
 pub shade_color: [f32; 3], // MToon shade color (12 bytes)
 pub is_mtoon: f32, // 0.0 or 1.0 (4 bytes)
 pub shading_toony: f32, // Shadow boundary sharpness 0.0~1.0 (4 bytes)
 pub shading_shift: f32, // Shadow threshold shift -1.0~1.0 (4 bytes)
 pub outline_width: f32, // Outline width (4 bytes)
 pub outline_mode: f32, // 0=none, 1=world, 2=screen (4 bytes)
 pub outline_color: [f32; 4], // Outline color (16 bytes)
 pub outline_lighting_mix: f32, // Lighting mix ratio 0~1 (4 bytes)
 pub rim_fresnel_power: f32, // Rim Fresnel exponent (4 bytes)
 pub rim_lift: f32, // Rim lift amount (4 bytes)
 pub rim_lighting_mix: f32, // Rim lighting mix ratio (4 bytes)
 pub rim_color: [f32; 3], // Rim color (12 bytes)
 pub has_matcap: f32, // MatCap enabled flag 0.0/1.0 (4 bytes)
 pub matcap_factor: [f32; 3], // MatCap multiply color (12 bytes)
 pub has_shade_multiply_tex: f32, // shadeMultiplyTexture present (4 bytes)
 pub has_shading_shift_tex: f32, // shadingShiftTexture present (4 bytes)
 pub shading_shift_tex_scale: f32, // shadingShiftTexture scale (4 bytes)
 pub has_rim_multiply_tex: f32, // rimMultiplyTexture present (4 bytes)
 pub uv_anim_scroll_x: f32, // UV scroll X speed (4 bytes)
 pub uv_anim_scroll_y: f32, // UV scroll Y speed (4 bytes)
 pub uv_anim_rotation: f32, // UV rotation speed (4 bytes)
 pub has_uv_anim_mask: f32, // uvAnimationMaskTexture present (4 bytes)
 pub alpha_cutoff: f32, // alphaMode sentinel (4 bytes: -1.0=OPAQUE, -0.5=BLEND, >=0.0=MASK cutoff)
 // --- Texture UV parameters ---
 pub base_uv_a: [f32; 4], // baseColor texCoord+transform (16 bytes)
 pub base_uv_b: [f32; 4], // baseColor texCoord+transform (16 bytes)
 pub shade_uv_a: [f32; 4], // shade texCoord+transform (16 bytes)
 pub shade_uv_b: [f32; 4], // shade texCoord+transform (16 bytes)
 pub shift_uv_a: [f32; 4], // shift texCoord+transform (16 bytes)
 pub shift_uv_b: [f32; 4], // shift texCoord+transform (16 bytes)
 pub rim_uv_a: [f32; 4], // rim texCoord+transform (16 bytes)
 pub rim_uv_b: [f32; 4], // rim texCoord+transform (16 bytes)
 pub outline_uv_a: [f32; 4], // outline texCoord+transform (16 bytes)
 pub outline_uv_b: [f32; 4], // outline texCoord+transform (16 bytes)
 pub uv_mask_uv_a: [f32; 4], // uv_mask texCoord+transform (16 bytes)
 pub uv_mask_uv_b: [f32; 4], // uv_mask texCoord+transform (16 bytes)
 pub emissive_factor: [f32; 3], // glTF emissiveFactor (12 bytes)
 pub has_emissive_tex: f32, // emissiveTexture presence (4 bytes)
 pub emissive_uv_a: [f32; 4], // emissive texCoord+transform (16 bytes)
 pub emissive_uv_b: [f32; 4], // emissive texCoord+transform (16 bytes)
 // --- Normal map + GI ---
 pub has_normal_tex: f32, // normalTexture presence (4 bytes)
 pub normal_scale: f32, // normalTexture.scale (4 bytes)
 pub gi_equalization_factor: f32, // GI equalization factor 0.0~1.0 (4 bytes)
 pub outline_width_channel: f32, // outlineWidthTexture channel 0=R,1=G,2=B (4 bytes)
 pub normal_uv_a: [f32; 4], // normal texCoord+transform (16 bytes)
 pub normal_uv_b: [f32; 4], // normal texCoord+transform (16 bytes)
 pub uv_anim_mask_channel: f32, // uvAnimMaskTexture channel 0=R,1=G,2=B (4 bytes)
 pub _pad: [f32; 3], // Padding (12 bytes)
 // --- matcap UV parameters ---
 pub matcap_uv_a: [f32; 4], // matcap texCoord+transform (16 bytes)
 pub matcap_uv_b: [f32; 4], // matcap texCoord+transform (16 bytes)
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
 var projected = normalize(vec2(nv.x, nv.y)); // normalize first (UniVRM order)
 let max_dist = proj_11; // 1/tan(fov/2) — UniVRM maxDistance equivalent
 let clamped_w = min(clip.w, max_dist); // distance clamp (suppress thick outlines at wide FOV/far)
 projected *= 2.0 * width * clamped_w;
 projected.x /= aspect; // divide by aspect(=w/h) for X correction (UniVRM multiplies h/w)
 projected *= saturate(1.0 - nv.z * nv.z); // camera-facing suppression
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
let surface = compute_mtoon_surface_lighting(n, uv, world_pos); // vec4: .rgb=color, .a=processed alpha
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
let parametric_rim = pow(saturate(1.0 - dot(n, v) + rim_lift),
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

## Material Editing and Expression Material Binds (v0.5.0 / v0.5.1)

### Material Editor Drawer — Update Path

The material editor UI (`show_material_editor_window`, ui.rs:1020) opens a floating `egui::Window`, and the closure holds `&mut app` to update both the IR material and `MaterialParamOverride` simultaneously. At closure exit, `pending_override` is merged into `app.material_overrides[mat_idx]` and `material_dirty[mat_idx]` is set.

At the end of `update()`, `apply_pending_material_rebuilds()` (app/mod.rs:1859) scans the dirty flags and calls `GpuRenderer::rebuild_material_bind_groups()`. In v0.5.1, this signature adds `queue: &wgpu::Queue` and `uniform_only: bool` — edits without texture changes take the `queue.write_buffer` fast path.

### DrawCall.material_buf — Persistent Uniform Buffer Handle (v0.5.1)

Prior to v0.5.1, `DrawCall` held only `material_bind_group: wgpu::BindGroup` without the corresponding `wgpu::Buffer` handle. `create_material_bind_group` created the buffer with `BufferUsages::UNIFORM` only and trapped it inside the bind group, making `queue.write_buffer` partial updates structurally impossible — every edit required full bind group recreation.

v0.5.1 introduces the following structural changes:

1. Added `material_buf: wgpu::Buffer` field to `DrawCall` (mesh.rs:85)
2. Split `create_material_bind_group` into 3 functions (gpu.rs:4739-4855):
   - `serialize_material_uniform(params) -> Vec<u8>` — encase serialization only
   - `create_material_buffer_and_bind_group(device, layout, params) -> (wgpu::Buffer, wgpu::BindGroup)` — creates buffer with `UNIFORM | COPY_DST` and returns both (used at load time)
   - `write_material_buffer(queue, buf, params)` — partial update via `queue.write_buffer` (used for Expression material binds and material editor color/scalar edits)
3. Added `uniform_only: bool` parameter to `rebuild_material_bind_groups`; when `true`, skips bind group recreation and only writes the buffer

This optimization allows Expression material binds to update up to ~11 material uniforms per frame (typical VRM model) without GPU resource churn. Each buffer write is approximately 720 bytes.

### Full Rebuild Information-Source Integrity (VRM / PMX / PMD)

`rebuild_material_bind_groups(uniform_only=false)` regenerates four groups:

1. `material_buf` — updated in place via `write_material_buffer`
2. `texture_bind_group` (standard-path BaseColor) — uses the same information source as the initial DrawCall construction
3. `mtoon_aux_bind_group` — `build_aux_refs_for` + `rebuild_mtoon_aux_bind_group`
4. `mmd_material_buf` / `mmd_texture_bind_group` / `mmd_aux_bind_group` — only when `RenderStyle::Mmd`

**Keeping BaseColor information sources aligned** — VRM treats `mat.base_color_tex_info` (which carries `KHR_texture_transform`, `texCoord`, and sampler info) as the primary source, while PMX/PMD only has `mat.texture_index` (a plain index reference). To stay consistent with the initial DrawCall construction (mesh.rs:1256), which uses `mat.texture_index` as the primary index source, `rebuild_material_bind_groups` also **prefers `texture_index`** and resolves the sampler via `base_color_tex_info.sampler` → `IrSamplerInfo::default()` in that fallback order.

Skipping this alignment produces silent inconsistencies for one material class: e.g., looking at `base_color_tex_info` alone makes PMX/PMD materials regress to `texture_bind_group = None` after any full rebuild, showing a blank white texture.

### Texture History Recall Ordering

`do_recall_texture_history` must execute in this order:

1. **Pristine restore + full state clear** — Restore `loaded.ir.materials[*]` from `pristine_materials`, clear `material_overrides` / `slot_texture_paths` / `tex.assignments` / `tex.pkg_assignments`, and mark every material dirty
2. **Texture restoration** — Apply entries from `resolved`: BaseColor through `assign_texture_to_material`, auxiliary slots through `assign_texture_core` (both produce immediate GPU reflection)
3. **Param override restoration** — Load `material_overrides` from JSON and `apply_to(mat)` each record

Pre-v0.5.1 implementations restored textures first and then restored pristine, which destroyed the texture references (`IrMaterial.emissive_texture`, `normal_texture`, `mtoon.*_texture`) that the restored textures had just written. The current order treats pristine as a baseline over which subsequent steps can overwrite cleanly; every intermediate state is consistent.

### Expression Re-application Timing

After processing dirty materials, `apply_pending_material_rebuilds` runs two final passes:

1. For each dirty material, re-capture `material_base_values[mat_idx]` via `MaterialBaseValues::from_ir(mat)` so editor-modified values become the new Expression base
2. If `morph_weights.iter().any(|w| w.abs() > 1e-6)`, run `accumulate_expression_materials` and `write_material_buffer` over affected materials

This trailing re-application is necessary when a manual morph slider holds non-zero weight and the user edits a material: animation playback would overwrite it on the next `update_animation` frame, but when playback is stopped and only manual morph sliders are driving, nothing else re-applies the Expression material reflection.

### Expression Material Binds — Playback Pipeline

VRM 1.0 spec update algorithm:

```
finalValue = baseValue + Σ((targetValue - baseValue) × weight)
```

- **Additive blending**: When multiple Expressions have binds for the same property on the same material, each `(target - base) × weight` is summed (not linear blend)
- **Base value**: Load-time `IrMaterial` value. If the material editor modifies it, the edited value becomes the new base

#### IR Types (intermediate/types.rs)

```rust
pub enum IrMorphKind {
    Vertex { positions, normals, tangents },
    Group(Vec<(usize, f32)>),
    Material {                                  // added in v0.5.1
        color_binds: Vec<IrMaterialColorBind>,
        uv_binds: Vec<IrTextureTransformBind>,
    },
}

pub enum MaterialColorBindType {
    Color,          // baseColorFactor → IrMaterial.diffuse
    EmissionColor,  // emissiveFactor → IrMaterial.emissive_factor
    ShadeColor,     // shadeColorFactor → MtoonParams.shade_color
    MatcapColor,    // matcapFactor → MtoonParams.matcap_factor
    RimColor,       // parametricRimColorFactor → MtoonParams.parametric_rim_color
    OutlineColor,   // outlineColorFactor → IrMaterial.edge_color
}
```

A VRM Expression with both Vertex and Material binds is emitted as **two IrMorphs with the same name** (`Vertex` and `Material`). Since `morph_weights` uses name-based mapping, both receive identical weights. This minimizes impact on existing `Vertex` / `Group` handling (no Compound variant needed).

#### Base Value Snapshot (`MaterialBaseValues`)

`GpuModel` gains `material_base_values: Vec<MaterialBaseValues>`, captured at the end of `cpu_prep_model` via `MaterialBaseValues::from_ir()` (diffuse / emissive_factor / shade_color / matcap_factor / rim_color / edge_color / base_uv_offset / base_uv_scale).

When the material editor triggers `material_dirty`, `material_base_values[mat_idx]` is re-captured so that **the editor-modified value becomes the new base**. This ensures expressions always blend against the latest edited value.

#### Accumulation Function (`accumulate_expression_materials`)

A pure function in mesh.rs. Iterates `GpuMorphEntry::Material` entries, accumulates color/UV deltas per material, and returns `MaterialParams` only for dirty materials.

```rust
pub(crate) fn accumulate_expression_materials(
    gpu_morphs: &[GpuMorphEntry],
    morph_weights: &[f32],
    base_values: &[MaterialBaseValues],
    ir_materials: &[IrMaterial],
    mat_count: usize,
    flags: &MaterialBuildFlags,
) -> Vec<Option<MaterialParams>>
```

1. Initialize `accum: Vec<ColorAccum>` to zero (sized to material count)
2. Iterate `morph_weights`; for each `GpuMorphEntry::Material` with `weight != 0`:
   - For each `color_bind`: `accum[mat].<color_field> += (target - base) × weight`
   - For each `uv_bind`: similarly accumulate `uv_offset` / `uv_scale`
3. For each `dirty` material, clone the `IrMaterial`, apply the final values, and generate `MaterialParams` via `build_material_params_for()`

#### Wiring into the Animation Loop

`update_animation` (app/mod.rs:2005) runs the `accumulate_expression_materials` → `write_material_buffer` loop immediately after `apply_bone_animation`:

```rust
let dirty_params = accumulate_expression_materials(
    &loaded.gpu_model.gpu_morphs,
    &self.morph_weights,
    &loaded.gpu_model.material_base_values,
    &loaded.ir.materials,
    mat_count,
    &flags,
);
for (mat_idx, params) in dirty_params.iter().enumerate() {
    if let Some(p) = params {
        for draw in &loaded.gpu_model.draws {
            if draw.material_index == mat_idx {
                crate::viewer::gpu::write_material_buffer(queue, &draw.material_buf, p);
            }
        }
    }
}
```

The same accumulation + write flow is also wired into the morph slider path (app/mod.rs:2360), so manual slider manipulation produces immediate material color feedback even when animation is not playing.

#### material_index Offset in IrModel::merge()

When appending a model, guest-side `material_index` values inside `IrMorphKind::Material` variants must be offset by the host's material count. Ordering caveat: use `mat_offset = self.materials.len()` computed at the **top** of `merge()` (before `self.materials.append(&mut other.materials)`). Computing it after the append yields the post-merge total (host + guest) and produces wrong offsets.

### Texture History Auxiliary Slot Persistence (v0.5.1)

Prior to v0.5.1, `popone_history.json` only stored `tex.assignments` (BaseColor), while auxiliary slot assignments (Emissive / Normal / Shade, etc.) lived in `slot_texture_paths: HashMap<(usize, TextureSlot), PathBuf>` — a session-local state that was discarded on restart.

v0.5.1 adds a `slot: TextureSlot` field to `TextureHistoryEntry` (persistence.rs:321):

```rust
pub struct TextureHistoryEntry {
    pub material_index: usize,
    pub material_name: String,
    pub texture_path: String,
    #[serde(default = "default_base_color_slot")]
    pub slot: TextureSlot,
}
```

**Backward/forward compatibility design**:
- `#[serde(default = "default_base_color_slot")]`: pre-v0.5.1 JSON without the slot field loads as `BaseColor`
- v0.5.1-saved JSON loaded by v0.5.0 has its `slot` field silently ignored (serde unknown-field setting) and treats it as BaseColor (forward compatible)

**Save path**: `save_texture_history()` scans both `tex.assignments` (BaseColor) and `slot_texture_paths` (all auxiliary slots) and merges them into a single `Vec<TextureHistoryEntry>`.

**Restore path**: `reload_texture_history()` walks entries; `entry.slot == BaseColor` takes the existing `assign_texture_to_material` path, while others call `assign_texture_core(mat_idx, slot, data, is_psd, display_name)` directly for GPU reflection. The dedup key is also widened from `HashSet<usize>` to `HashSet<(usize, TextureSlot)>` so multiple simultaneous auxiliary slot assignments on the same material (e.g. Emissive + Normal + Shade) are preserved correctly.

## Bloom Post-Effect

### Dual Kawase Algorithm

Dual Kawase (Dual Filtering) bloom implemented in `bloom.rs` (~500 lines). Alternates between downsample and upsample passes to achieve wide-area blur at low cost.

1. **Brightness extraction**: Extract pixels above threshold from emissive buffer
2. **Downsample**: 3–6 progressive half-resolution passes (Kawase filter kernel)
3. **Upsample**: Reverse-order upscale with additive blending
4. **Final composite**: Add bloom result to scene color with intensity factor

### MRT (Multiple Render Target) Emissive Separation

The render pass is split into mesh drawing (MRT with 2 targets) and overlay drawing (1 target). The mesh drawing pass outputs scene color at `@location(0)` and emissive component at `@location(1)`. Grids and non-emissive surfaces write zero to `@location(1)`, so they are excluded from bloom.

Bloom intermediate buffers use `Rgba16Float` (previously `Rgba8Unorm`) to avoid banding in HDR emissive gradients. BindGroups for the external offscreen texture are cached and only recreated on resize/MSAA toggle.

### UI Parameters

| Parameter | Range | Default | Description |
|-----------|-------|---------|-------------|
| ON/OFF | — | OFF | Enable bloom. When disabled, bloom pass execution is skipped (MRT 2-target rendering remains active; only additional bandwidth cost) |
| Intensity | 0.0–4.0 | 0.8 | Bloom brightness |
| Threshold | 0.0–1.0 | 0.0 | Cuts emissive below this luminance |
| Radius | 3–6 | 4 | Downsample stages. Larger = wider blur |

### Per-Material Emissive Toggle

`emissive_per_mat: Vec<bool>` controls emissive ON/OFF per material.

- **glTF materials**: When `emissive_per_mat[i]` is false, `MaterialUniform.emissive_factor` is zeroed and `has_emissive_tex` is set to false. Both the shader's `lit += emissive` and `out.bloom = vec4(bloom_color, ...)` become zero
- **PMX/PMD materials**: When `emissive_per_mat[i]` is false, `MmdMaterialUniform.bloom_emissive` is zeroed
- **Default**: All materials are initialized with emissive ON. HDR emissive values (via `KHR_materials_emissive_strength`) are clamped by the GPU; no auto-OFF logic

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
 1. `_UseEmission` float if explicitly present (lilToon-specific, takes priority)
 2. `_Emission` float if explicitly present (Standard shader)
 3. `_EMISSION` keyword in `m_ShaderKeywords` / `m_ValidKeywords`
 4. `_EmissionMap` texture present
 5. `_EmissionColor` non-black and non-white (white excluded as default in many shaders)
- When `_EmissionMap` is present but `_EmissionColor` is black, emissive_factor corrected to white (1,1,1) to avoid shader 0 × texture = 0
- **lilToon Screen-blend attenuation**: When `_EmissionBlend` is 1 (Screen mode), `emissive_factor` is multiplied by 0.5 to approximate screen compositing (`base + emission*(1-base)`), which is always darker than pure additive. This prevents bloom white-out on lilToon materials
- `m_ShaderKeywords` / `m_ValidKeywords` supports both YAML inline format (space-separated string) and multi-line list format (`- _EMISSION`)
- Added `emission_texture_guid` / `emission_color` / `emission_enabled` / `emission_blend` fields to `ResolvedMaterialTextures`

## Camera & Lighting

### Camera

| Item | Value |
|------|-------|
| FOV | 30° (MMD-compliant) |
| Projection | Perspective (default) / Orthographic (5 key toggle) |
| Controls | Left drag: rotate, Right/Middle drag: pan, Scroll: zoom |
| Precision | Shift key for 1/3 speed |
| Fit | F / Double-click (preserves yaw/pitch), R (front reset) |
| Depth | Reverse-Z: near→1.0, far→0.0 with `Greater` compare. Depth32Float format |
| Near/Far | `distance * 0.005` / `distance * 50`, near clamped to `NEAR_MAX=1.0` |

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

## Viewer Display Styles

### Dark Theme

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

### VRM Meta Info Color Badges

Permission and license values are displayed as colored badges using `egui::RichText::background_color()`. This approach is used because egui's default font lacks color emoji glyphs.

| Badge Type | Background | Foreground | Usage |
|-----------|------------|------------|-------|
| Allow | `#206020` | `#80FF80` | Permitted / unrestricted (allow, Everyone, CC0, CC_BY, etc.) |
| Warn | `#605010` | `#FFE060` | Conditional (OnlyAuthor, personalProfit, CC_BY_NC, etc.) |
| Deny | `#601818` | `#FF8080` | Prohibited (disallow, prohibited, Redistribution_Prohibited, etc.) |
| Neutral | `#404040` | `#A0A0A0` | Neutral (unnecessary, Other) |

The data layer (`ir.comment`) retains English labels for PMX comment field output. Japanese labels are applied only at UI display time via `meta_section_ja()` / `meta_label_ja()`, with tooltips and badges from `meta_label_tooltip()` / `format_meta_value()`.

### Splash Image

Displays a logo image centered in the viewport when no model is loaded.

- PNG embedded in the exe via `include_bytes!("../../../assets/popone_image.png")`
- `image::load_from_memory` → `egui::ColorImage` → `ctx.load_texture` for egui texture registration
- Auto-scaled to fit viewport with `min(width_ratio, height_ratio, 1.0)`, centered via `Rect::from_center_size`
- Rounded corners via `egui::Image::corner_radius(CornerRadius::same(16))` (shader-level masking)
- Placed using `viewport.put(img_rect, image)` for explicit layout positioning

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
- Normal smoothing + normal map compatibility: Normal maps perturb normals via the TBN matrix (built from vertex normals + tangents), so faceted base normals make polygon edges visible. Using `[S]` to smooth base normals and `[N]` to apply normal maps produces smoother results. The `mat.normal_texture.is_none()` guard in `mesh.rs` has been removed, allowing smoothing on normal-mapped materials
- Per-material normal map toggle `[N]`: Controlled by `normal_map_per_mat: Vec<bool>`. When OFF, `MaterialUniform.has_normal_tex` is set to 0.0, causing the shader's `if material.has_normal_tex > 0.5` branch to skip normal map sampling
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

## Bone Display

The viewer draws bones with 4 shape types based on bone flags.

### Shape Determination (Priority Order)

| Priority | Condition | Shape | Drawing |
|----------|-----------|-------|---------|
| 1 | `BONE_FLAG_IK` / PMD type=2 | ◻ IK Controller | Blue outline square + orange fill + blue center square |
| 2 | `BONE_FLAG_AXIS_FIXED` | ⊗ Axis-fixed | Blue outer circle (thick) + ✕ (thick) |
| 3 | `BONE_FLAG_TRANSLATABLE` / PMD type=1 | ◻ Move | Blue outer square + blue inner square + blue center fill |
| 4 | None | ◎ Normal | Blue outer circle + blue inner circle + blue center fill |

### IK Detection (Two Paths)

IK detection uses different paths depending on the source format:

- **PMX/PMD path**: Uses the `BONE_FLAG_IK` bit flag (or `bone_type == 2` in PMD) to identify IK controller bones, and traverses IK Link chains to mark affected bones.
- **VRM/FBX path**: Falls back to bone-name matching — a bone is treated as an IK bone if its name contains `IK` or full-width `ＩＫ` (since `BONE_FLAG_IK` is a PMX-specific concept that does not exist in VRM/FBX skeletons).

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

### Appearance (Color / Size)

- Shape: Double circle + triangle without base (◎△)
- Rendering: 1px LineList (`pipeline_line_overlay`)
- Color: Normal bone = blue `#0000ff`, IK bone = orange `#ff9600`
- Size: Scales with camera distance (constant screen size)

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
├ Left leg ← VRMA "leftUpperLeg" rotation applied here
├ Left leg D ← Rotation grant copies "Left leg" rotation (ratio=1.0)
│ └ Left knee D ← Rotation grant copies "Left knee" rotation
│ └ Left ankle D
```

### Processing Flow

```
1. compute_animated_globals_inplace() — Apply VRMA retargeted rotations
2. apply_grants() — Apply grant deltas and recompute globals
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

## Shader-Aware PMX Material Conversion

### generate_toon() (replaces select_toon)

Generates per-material toon gradient textures (256×16 PNG) from `shade_color` → `diffuse` for MToon/UTS2 materials. Replaces the Phase 1 `select_toon()` which mapped to shared toon01–toon10.

**Gradient generation** (`generate_toon_gradient`): left edge = `shade_color`, right edge = `diffuse`, linear interpolation across 256 pixels, 16 rows. Output as PNG via `image::codecs::png::PngEncoder`.

**Filename collision avoidance**: a `HashSet<String>` of existing texture filenames is passed to `generate_toon()`. If the generated name (e.g., `toon_body_000.png`) already exists, a `_1`, `_2`, ... suffix is appended until unique.

**PMX integration**: generated toon textures are appended to `model.textures` after existing textures, with `PmxToonRef::Texture(base_tex_count + idx)`. After `write_all_textures_from_ir()`, PMX paths are corrected with actual filenames.

| Material Type | Toon Reference |
|---------------|---------------|
| MToon/UTS2 with shade_color | `Texture(index)` — per-material gradient PNG |
| MToon/UTS2 without shade_color | `Shared(2)` = toon03 (medium) |
| Non-MToon | `Shared(0)` = toon01 (unchanged) |

### MToon ambient/specular Correction

Applied only at the conversion stage (`convert/material.rs`). The extraction stage (`vrm/extract.rs`) retains source-faithful values.

| Parameter | MToon | UTS2 | Non-MToon |
|-----------|-------|------|-----------|
| ambient | `shade_color * 0.5` (or `diffuse * 0.4` if no shade_color) | `_2nd_ShadeColor * 0.5` (set during extraction) | Unchanged |
| specular | `diffuse.rgb * 0.2` (light-reactive) | `_HighColor` (set during extraction) | Unchanged |
| specular_power | `10.0` | `_HighColor_Power * 10.0` | Unchanged |

### UTS2 (Unity-Chan Toon Shader Ver.2) Approximate Conversion

Introduces `ShaderFamily` enum (`Other` / `Mtoon` / `Uts2` / `LilToon` / `Poiyomi`) to detect toon shaders from VRM 0.0 `materialProperties.shader` field. Detected parameters are approximate-mapped to `MtoonParams`, reusing the existing MToon rendering pipeline (viewer) and PMX conversion path.

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

`_ClippingMask` texture is not yet supported (warning + base alpha fallback).

#### Outline

UTS2 `_OUTLINE` keyword (NML/POS) detected from `keyword_map`. Both NML and POS approximated as `OutlineWidthMode::WorldCoordinates` (POS uses UTS2-specific camera distance-based transformation differing from MToon ScreenCoordinates; warning emitted).

#### GI

UTS2 `_GI_Intensity` is additive indirect light strength (default 0 = no GI), semantically different from MToon `gi_equalization_factor` (raw/equalized GI interpolation). Fixed to `gi_equalization_factor = 0.0` to avoid semantic inversion.

#### Ambient Overwrite Prevention

The end-of-extraction `ambient = diffuse * 0.4` recalculation for all materials is suppressed for `ShaderFamily::Uts2 | LilToon | Poiyomi` to preserve shade-color-based ambient values.

### lilToon Approximate Conversion

Detected when shader name contains "lilToon" or "lil/", or when `_lilToonVersion` float property exists. Exclusive with MToon/UTS2 (cascade priority: MToon → UTS2 → lilToon → Poiyomi).

#### Parameter Mapping

| lilToon Property | MtoonParams / IrMaterial Field | Notes |
|---|---|---|
| `_Color` | `diffuse` | sRGB → linear conversion |
| `_ShadowColor` | `shade_color` | Requires `_UseShadow > 0.5` |
| `_ShadowColorTex` | `shade_texture` | Fallback to `_MainTex` |
| `_Shadow2ndColor` | `ambient` | `* 0.5`, requires `_UseShadow2nd > 0.5` |
| `_ShadowBorder` / `_ShadowBlur` | `shading_shift_factor` / `shading_toony_factor` | Same formula as UTS2 |
| `_UseOutline` / `_OutlineWidth` / `_OutlineColor` / `_OutlineWidthMask` | outline params | `WorldCoordinates` only |
| `_UseRim` / `_RimColor` / `_RimFresnelPower` | rim params | |
| `_UseMatCap` / `_MatCapTex` / `_MatCapColor` | matcap params | |
| `_UseEmission` / `_EmissionColor` / `_EmissionMap` | emissive | Requires enable flag |
| `_UseBumpMap` / `_BumpMap` / `_BumpScale` | normal texture | Requires enable flag |
| `_TransparentMode` | `alpha_mode` | 0=Opaque, 1=Mask, 2/3=Blend |
| `_Cull` | `cull_mode` | 0=None, 1=Front, 2=Back |

Not supported: Fur, Refraction, Gem, FakeShadow, AudioLink, Dissolve, distance fade.

### Poiyomi Approximate Conversion

Detected when shader name contains "poiyomi" (case-insensitive), or when both `_EnableShadow` (float) and `_Shadow1stColor` (vector) properties exist.

#### Parameter Mapping

| Poiyomi Property | MtoonParams / IrMaterial Field | Notes |
|---|---|---|
| `_Color` | `diffuse` | sRGB → linear conversion |
| `_Shadow1stColor` | `shade_color` | Requires `_EnableShadow > 0.5` |
| `_ShadowTexture` | `shade_texture` | Fallback to `_MainTex` |
| `_Shadow2ndColor` | `ambient` | `* 0.5`, only if vector property exists |
| `_ShadowBorder` / `_ShadowBlur` | `shading_shift_factor` / `shading_toony_factor` | Same formula as UTS2/lilToon |
| `_EnableOutline` / `_OutlineWidth` / `_OutlineColor` / `_OutlineWidthMask` | outline params | `WorldCoordinates` only |
| `_EmissionColor` / `_EmissionMap` | emissive | Always extracted |
| `_BumpMap` / `_BumpScale` | normal texture | Always extracted |
| `_Mode` | `alpha_mode` | 0=Opaque, 1=Mask, 2/3=Blend |
| `_Cull` | `cull_mode` | 0=None, 1=Front, 2=Back |

Not supported: Rim, MatCap, AudioLink, Dissolve, Glitter, Parallax, Decal.

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

#### Persistent Warning (Viewport bottom-left)

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

## Visible Materials Only Export

An optional feature that excludes materials hidden in the display tab from PMX conversion output in the viewer. Implemented in the `export_filter.rs` module.

### Design Principles

- **Viewer-specific**: Filter logic is placed in `viewer/export_filter.rs`. No changes to core conversion logic (`pmx/build.rs`, `lib.rs`)
- **IrModel manual construction**: Filtered IR is newly constructed field by field. `IrMesh` heavy fields (`vertices`, `indices`, `morph_targets`) are `Arc<Vec<T>>`, so cloning shares data via reference count (O(1)). Mutation uses `Arc::make_mut` COW via `vertices_mut()` / `indices_mut()` / `morph_targets_mut()`
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
 pub material_range: std::ops::Range<usize>, // Used for UV export
 pub draw_range: std::ops::Range<usize>, // Used for UI material list
}
```

Separating `material_range` and `draw_range` ensures UV grouping works correctly even for models with zero draw calls.

## Animation Playback

The viewer supports real-time playback of VRMA / glTF / FBX animations.

### Pose Reset on Animation Clear

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
local_rot = L_dst × W_dst⁻¹ × normalized × W_dst
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

## Session Persistence

### Settings File (popone.toml)

Stored in `%LOCALAPPDATA%\popone` on Windows, falling back to the exe directory on other platforms. `persistence::data_dir()` determines the path, and `migrate_from_exe_dir()` moves existing files from the old exe-adjacent location on first launch. Stores window position/size, last-opened directories, and log settings.

- **Log settings**: `[log]` section with `level` (error/warn/info/debug, default: debug) and `keep` (log file retention count, default: 5). Config is loaded before logger initialization so settings take effect from the first log message. Invalid `level` values fall back to `debug`
- **Theme colors**: `[theme]` section with 6 optional hex color fields: `panel_bg` (default: `1D1D1D`), `border` (`333333`), `accent` (`4A90D9`), `text` (`D0D0D0`), `widget_bg` (`252525`), `active` (`2A5A8A`). Values accept `"RRGGBB"` or `"#RRGGBB"` format. `ThemeConfig::parse_hex()` trims `#` prefix, validates 6-char length, and returns `(u8, u8, u8)`. Resolved colors are cached in `ViewerApp.theme_panel_bg` / `theme_border` for per-frame panel rendering. `setup_dark_theme()` applies the resolved colors to egui `Visuals` (panel_fill, window_fill, widget states, selection, border strokes). Unspecified fields fall back to hardcoded defaults (`DARK_PANEL_BG`, `DARK_BORDER_COLOR`)
- **Position**: Saved from `outer_rect.min`, restored via `ViewportCommand::OuterPosition`. No drift due to coordinate system consistency
- **Size**: Saved from `inner_rect` width/height, restored via `with_inner_size`
- **Change detection**: 1px epsilon comparison. Position/size not updated while maximized or minimized
- **File writing**: Backup-based atomic write (`.bak` → rename). Auto-recovery from `.bak` if main file is missing at startup
- **First launch**: No config file or missing `[window]` section defaults to 1280x720, position determined by OS

### Texture Assignment History (popone_history.json)

Saves texture assignments for FBX/OBJ models (`ReloadableSource::File` with empty `appended_models`) as JSON.

- **Key**: `dunce::simplified` + lowercase + `\`-normalized full path
- **Value**: Array of `{ material_index, material_name, texture_path }`
- **Material matching**: index+name exact match → name unique fallback → skip
- **On recall**: `link_same_name` temporarily disabled, failures detected via `ConvertResult::Failure`, results notified to user

## Log Output

During CLI conversion, a `.log` file is generated in the same directory as the output (not generated with `--dump`).
stderr outputs logs at or above the level specified by `--log-level` (default: `info`),
while the log file records all entries down to `debug` level.

### Overall Log Structure

The conversion process outputs logs in the following order, centered on `build_pmx_model()`.

```
=== PMX Model Build Start === ← INFO: Model name, VRM version
Input VRM: bones=N, meshes=N... ← INFO: Input statistics summary
--- Mesh List --- ← DEBUG: Vertex count, face count, material idx per mesh
--- Texture List --- ← DEBUG: Filename, MIME, data size
--- Material List --- ← DEBUG: Diffuse, texture, double-sided, MToon, edge
Materials: N (MToon=N, double-sided=N...) ← INFO: Material statistics
--- Face Count by Material --- ← DEBUG: Face vertex count per material
Vertex weight distribution: ... ← DEBUG: Vertex count distribution of BDEF1/BDEF2/BDEF4
--- Morph List --- ← DEBUG: Panel, type, target count per morph
--- Rigid Body List --- ← DEBUG: Shape, bone, group, physics mode per rigid body
--- Joint List --- ← DEBUG: Connected rigid bodies, position per joint
=== insert_standard_bones === ← DEBUG: Standard bone insertion (steps 1-18)
=== Post-Sort Bone List === ← DEBUG: Final bone order after topological sort
--- Display Frames --- ← DEBUG: Bone count, morph count per display frame
=== PMX Model Build Complete === ← INFO: Output PMX statistics summary
```

### Panic Log

On panic, `flush_log_buffer` inside `panic.set_hook` writes the in-memory buffer directly to `panic_yyyymmdd_hhmmss.log`. v0.4.0 collapses crash dumps to "one file per crash" by removing the legacy "write to `popone_yyyymmdd_hhmmss.log`, then copy to `panic_*.log`" relay. Automatic log rotation (`rotate_logs`) was also removed in the same release, so generated `panic_*.log` files persist until manually deleted.

### Log Viewer (Separate Window)

Implemented in `popone/src/viewer/log_viewer.rs`. Opens an OS-level separate window from the top-bar "ログ" button and streams the contents of `SharedLogBuffer` in real time.

#### Choice of egui API

- **`ctx.show_viewport_deferred`** is used. The reason `immediate` was rejected: the main `update()` runs `render_to_texture` for the 3D scene every frame, and `immediate` viewports trigger a parent repaint whenever the child needs one, which would force the 3D scene to re-render on every log line. With `deferred`, the child viewport's repaint does not wake the parent (the relevant egui `context.rs` documentation explicitly says "parent and child repaint each other in immediate mode, prefer deferred when avoidable").
- To satisfy the `Fn + Send + Sync + 'static` bound on the deferred closure, `SharedLogViewer = Arc<Mutex<LogViewerModel>>` is captured into the closure via `Arc::clone`. The `ViewerApp` only carries a single `log_viewer: SharedLogViewer` field.
- A fixed `egui::ViewportId::from_hash_of("popone_log_viewer")` is used as the viewport identifier.

#### `LogViewerModel` Layout

| Field | Type | Purpose |
|---|---|---|
| `visible` | `bool` | Window visibility |
| `last_offset` | `usize` | Previous read position into `SharedLogBuffer::total_written` |
| `lines` | `VecDeque<LogLine>` | Parsed log lines (capped at 20,000) |
| `filter_indices` | `Vec<usize>` | Indices into `lines` that pass the level filter (used by virtualized scroll) |
| `filters` | `LevelFilters` | Per-level visibility flags (Error/Warn/Info/Debug) |
| `follow_tail` | `bool` | Auto tail-follow toggle |
| `apply_geometry` | `Option<([f32; 2], [f32; 2])>` | Position / size to pass to `ViewportBuilder` next frame (consumed via `take()`, then `None`) |
| `last_geometry` | `Option<([f32; 2], [f32; 2])>` | Latest position / size read from the child viewport every frame (used for persistence) |
| `tail_buffer` | `String` | Trailing fragment without `\n` (prepended to next ingest) |
| `seeking_first_header` | `bool` | `true` until the first `[HEADER]` is seen (used to discard the leading byte-level fragment from `SharedLogBuffer` drain) |

#### Log Line Parser

Parses the `[HH:MM:SS.mmm][LEVEL] message` format with a hand-written parser (no `regex` dependency):

1. Take the timestamp from the first `[` to the next `]`
2. Take the level string from the following `[` to its `]`
3. If the level string is one of `ERROR`/`WARN`/`INFO`/`DEBUG`/`TRACE`, store the corresponding `LogLevel`. Otherwise (e.g. `FATAL`), store `LogLevel::Unknown`
4. If a line does not start with `[` and a previous `LogLine` exists, append it to the previous `message` with a `\n` separator (multi-line message support such as backtraces)
5. While `seeking_first_header == true`, lines that don't start with `[` are discarded (the `SharedLogBuffer` drains by bytes, so the leading fragment after a viewer-side reopen may be incomplete)

#### `filter_indices` Consistency

When `lines` exceeds the 20,000 cap and entries are drained from the front, the indices stored in `filter_indices` become stale. In that case `filters_dirty = true` is set and `rebuild_filter_indices` rebuilds the index at the end of the next `ingest`. The same flag is set when filter checkboxes are toggled. Per-line append cost is O(1); only cap-exceed and filter changes pay the O(N) rebuild.

#### Geometry Round-Trip

Splitting `apply_geometry` and `last_geometry` into two fields lets the following scenarios all round-trip correctly:

| Scenario | Behavior |
|---|---|
| Startup with `visible=true` and saved geometry | `from_config` initializes both fields from the config → first frame consumes `apply_geometry` via `take()` |
| Startup with `visible=false` and saved geometry | `from_config` also initializes `last_geometry` from the config → `show_log_viewer`'s early return does not touch `apply_geometry` → toggle-open uses `show()` which keeps the existing `apply_geometry` |
| In-session close → reopen | `hide()` snapshots `last_geometry` into `apply_geometry`; the next `show()` restores it |
| Exit without ever opening the viewer | `last_geometry` stays at its config value → `export_config` preserves the original config |

`hide()` and `show()` are `LogViewerModel` helpers that provide identical behavior whether the viewer is closed via the `×` button or via the top-bar toggle.

#### `ViewerApp::show_log_viewer` Flow

```rust
fn show_log_viewer(&self, ctx: &egui::Context) {
    // The visible check must run BEFORE take(), so that a hidden-at-startup session
    // does not consume apply_geometry on the first frame.
    let apply_geometry = {
        let mut m = self.log_viewer.lock().unwrap_or_else(|p| p.into_inner());
        if !m.visible { return; }
        m.apply_geometry.take()
    };
    // Pass position to ViewportBuilder only when apply_geometry is Some
    let mut builder = egui::ViewportBuilder::default()
        .with_title("popone - Log Viewer")
        .with_inner_size([720.0, 480.0]);
    if let Some((pos, size)) = apply_geometry {
        builder = builder.with_position(egui::pos2(pos[0], pos[1])).with_inner_size(size);
    }
    // Arc clones for the deferred closure
    let model = Arc::clone(&self.log_viewer);
    let log_buffer = Arc::clone(&self.log_buffer);
    let logs_dir = self.logs_dir.clone();
    ctx.show_viewport_deferred(vp_id, builder, move |child_ctx, _| {
        let mut m = model.lock().unwrap_or_else(|p| p.into_inner());
        m.poll(&log_buffer);                          // Incremental ingest
        m.draw(child_ctx, &log_buffer, &logs_dir);    // UI rendering
        // Read the latest geometry from the child viewport into last_geometry
        // close_requested → m.hide() snapshots last_geometry into apply_geometry
        if m.visible {
            child_ctx.request_repaint_after(Duration::from_millis(150));  // 150ms polling
        }
    });
}
```

#### Minimizing `LogBuffer` Lock Hold Time

While `log_buffer.lock()` is held, log-producing threads (`log::info!` etc.) block, so the lock duration is minimized:

1. Take the `read_from_offset` result `String` (or, on manual save, the `Vec<u8>` byte snapshot)
2. Update `last_offset`
3. Drop the guard
4. Run parsing, UI rendering, and file I/O entirely outside the lock

The `LogViewerModel` mutex itself never appears on the log-producing path, so it can be held for the full duration of UI rendering without affecting log producers.

## Single Instance

When the viewer is already running and launched again, the file path is forwarded to the existing window and the new process exits. Windows only (`#[cfg(target_os = "windows")]`).

- **Detection**: `Local\popone_viewer_single_instance` Named Mutex detects existing process
- **Communication**: `\\.\pipe\popone_viewer_ipc` Named Pipe (MESSAGE mode) sends file path as UTF-8
- **Reception**: Background thread listens → `mpsc::channel` → `update()` pushes a `PendingLoadDispatch` onto `pending.load_dispatch`
- **Long path support**: `ReadFile` loops on `ERROR_MORE_DATA` (234) to accumulate the full message. Partial data from non-recoverable errors is discarded (only successfully completed reads are forwarded). Buffer size: 64KB per read
- **Handle management**: Pipe handles are wrapped in `WinHandle` (RAII newtype with `Drop` impl calling `CloseHandle`), preventing leaks on early returns, panics, or `continue` paths
- **Focus**: `ViewportCommand::Minimized(false)` + `Focus` (restores from minimized state)
- **Path normalization**: `std::fs::canonicalize()` before sending (CWD difference mitigation)
- **InstanceCheck tri-state**: `Primary` (primary instance start) / `Forwarded` (file path forwarded to existing instance, current process exits) / `FallbackStart` (fallback when existing-instance detection fails). v0.4.0 removed automatic log rotation, so `Primary` and `FallbackStart` now behave identically; the previous `can_rotate` variable that distinguished them was deleted in the same release

## FPS Measurement

Displays FPS and frame time (ms) in the viewport top-right overlay.

- **Method**: Frame counting (computes `FPS = (frame_count - 1) / time_span` from `VecDeque<Instant>` over the last 1 second)
- **Update interval**: 0.5 seconds (flicker prevention)
- **ms display**: Average frame time within the window (consistent with FPS value)

## Watchdog — Main Thread Responsiveness Monitor

A background watchdog thread detects main thread freezes (Windows "Not Responding" state) and logs the event for post-mortem diagnosis.

### Architecture

```
Main Thread (egui event loop) Watchdog Thread
┌──────────────────────────┐ ┌──────────────────────────┐
│ update() { │ │ loop { │
│ if minimized: │ │ sleep(2s) │
│ heartbeat.pause() │ │ last = hb.load() │
│ else: │ │ if last == PAUSED: │
│ heartbeat.tick() │ shared │ skip │
│ request_repaint_after │◄────────►│ elif now - last > 5s: │
│ (3s) │ AtomicU64 │ log::warn!(...) │
│ ... │ │ elif was_unresponsive: │
│ } │ │ log::info!(recovered)│
└──────────────────────────┘ └──────────────────────────┘
```

### Heartbeat (`viewer/watchdog.rs`)

| Field | Type | Description |
|---|---|---|
| `Heartbeat.0` | `Arc<AtomicU64>` | Epoch milliseconds of last `tick()`, or `u64::MAX` for `PAUSED` |

- **`tick()`**: Stores current epoch millis (`Ordering::Relaxed`)
- **`pause()`**: Stores `PAUSED` sentinel (`u64::MAX`). Used when `viewport().minimized == Some(true)` to suppress false positives since `update()` may not be called while minimized
- **`request_repaint_after(3s)`**: Ensures `update()` is called at least every 3 seconds during idle (no input, no animation), keeping the heartbeat fresh. During a real freeze, the main thread is blocked and the scheduled repaint never executes

### Watchdog Thread

- **Check interval**: 2 seconds (`thread::sleep`)
- **Threshold**: 5 seconds (matches Windows "Not Responding" detection)
- **State transitions**: Normal → `warn!("unresponsive")` → `warn!("still unresponsive (total Nms)")` → `info!("recovered after Nms freeze")`
- **PAUSED handling**: When `PAUSED` is read, resets `was_unresponsive` state and skips the check

### Log Output Examples

```
[12:34:56.789][WARN] [watchdog] Main thread unresponsive (no heartbeat for 6012ms)
[12:34:58.790][WARN] [watchdog] Main thread still unresponsive (total 8013ms)
[12:35:00.123][INFO] [watchdog] Main thread recovered after 9456ms freeze
```

## Codebase Architecture

![Architecture](architecture.svg)

## Source File Structure

```
src/
├── main.rs Entry point (no args or no output specified → viewer / output specified → CLI conversion)
├── lib.rs Library API
├── error.rs Error type definitions (PoponeError enum, thiserror, ResultExt trait)
├── unitypackage.rs .unitypackage (tar.gz) asset extraction + Prefab texture mapping (GUID resolution, Variant recursion, multi-format support)
├── archive/
│ ├── mod.rs ZIP / 7z unified API (list_models, extract_model_bundle)
│ ├── zip_extract.rs ZIP extraction (2-pass: metadata listing → selective extraction)
│ └── sevenz.rs 7z extraction (filtered full extraction, chunked read with size limit)
├── vrm/
│ ├── loader.rs GLB loading / extension data extraction (file and byte array support)
│ ├── detect.rs VRM version auto-detection
│ ├── extract.rs VRM → intermediate representation (IrModel) extraction
│ ├── animation.rs VRMA / glTF animation loading
│ ├── types_v0.rs VRM 0.0 serde type definitions
│ └── types_v1.rs VRM 1.0 serde type definitions
├── fbx/
│ ├── parser.rs FBX binary / ASCII parser (including Content block special handling)
│ ├── scene.rs Scene graph construction (Objects / Connections analysis)
│ ├── extract.rs FBX → intermediate representation (IrModel) extraction
│ ├── bone.rs Bone hierarchy construction (PreRotation support)
│ ├── mesh.rs Mesh, UV, material property extraction
│ ├── skin.rs Skin weight extraction
│ ├── texture.rs Texture extraction (embedded / external file)
│ ├── blendshape.rs Blend shape extraction
│ ├── animation.rs FBX animation extraction (Stack/Layer/CurveNode/Curve hierarchy, byte array support)
│ └── humanoid.rs Humanoid rig auto-detection and mapping (namespace prefix stripping, CamelCase support)
├── pmx/
│ ├── types.rs PMX data type definitions
│ ├── reader.rs PMX 2.0/2.1 binary loading (UTF-16LE/UTF-8, SoftBody skip)
│ ├── extract.rs PMX → intermediate representation (IrModel) extraction (glTF reverse conversion)
│ ├── build.rs Intermediate representation → PMX model construction / standard bone insertion
│ └── writer.rs PMX binary output (UTF-16 LE)
├── pmd/
│ ├── types.rs PMD data type definitions
│ ├── reader.rs PMD binary loading (Shift_JIS, encoding_rs)
│ └── extract.rs PMD → intermediate representation (IrModel) extraction (material name text loading support)
├── obj/
│ ├── mod.rs OBJ module definition
│ └── extract.rs OBJ → intermediate representation (tobj crate, MTL/texture resolution, cm→m normalization, auto normal generation)
├── stl/
│ ├── mod.rs STL module definition
│ ├── reader.rs STL binary / ASCII parser (format detection by length validation)
│ └── extract.rs STL → intermediate representation (mm→m + Z-Up→Y-Up normalization, zero-normal recalculation)
├── unity/
│ └── animation.rs Unity .anim Muscle conversion (SwingTwist decomposition)
├── intermediate/
│ ├── types.rs Intermediate representation (IrModel / IrBone / IrMesh / IrMaterial / MtoonParams / CullMode etc., SourceFormat / merge 3-level fallback)
│ ├── tangent.rs MikkTSpace tangent generation (mikktspace crate)
│ ├── animation.rs Animation intermediate representation (VrmaAnimation / BoneChannel)
│ └── pose.rs Stance conversion (T→A / A→T, physics sync support)
├── convert/
│ ├── coord.rs Coordinate conversion (glTF → PMX / PMX → glTF)
│ ├── bone_map.rs VRM humanoid bone ↔ PMX Japanese name map (bidirectional)
│ ├── material.rs Material conversion
│ ├── morph.rs Expression → morph name map
│ ├── physics.rs SpringBone → rigid body / joint conversion (V0/V1)
│ ├── texture.rs Texture PNG output
│ └── uvmap.rs UV map PSD output (material layers, boundary wrap, group folders)
└── viewer/ ← Compiled only when feature = "viewer"
 ├── app/ eframe::App state management (split into 5 modules)
 │ ├── mod.rs ViewerApp struct definition, initialization, eframe::App impl
 │ ├── file_io.rs File loading, drag & drop, reload
 │ ├── texture_mgmt.rs Texture assignment and preview
 │ ├── pending.rs Deferred task processing (PendingState / ExportState)
 │ └── helpers.rs Utility types and functions (ReloadableSource / TextureSource / is_temp_path etc.)
 ├── gpu.rs wgpu pipeline / offscreen rendering / visualization buffer dirty flag
 ├── mesh.rs IrModel → GPU vertex buffer conversion
 ├── texture.rs Texture GPU upload (MIME hint support)
 ├── camera.rs Orbit camera
 ├── grid.rs Grid floor
 ├── ui.rs Info panel / morph sliders / conversion button / PMX/PMD grayed out
 ├── export_filter.rs Visible materials only export filter (IrModel → filtered IrModel)
 ├── animation.rs Animation playback / retargeting (VRMA/glTF/FBX support)
 └── single_instance.rs Single instance control (Named Mutex + Named Pipe IPC, Windows only)
```

## Library API

`popone` can also be used as a library:

```rust
use popone::{convert_vrm_to_pmx, convert_fbx_to_pmx, VrmConvertOptions};
use std::path::Path;

// Default options; customize individual fields via a struct literal.
let options = VrmConvertOptions::default();
// Example: VrmConvertOptions { no_physics: true, scale: 0.08, ..Default::default() }

// VRM to PMX
let stats = convert_vrm_to_pmx(
    Path::new("input.vrm"),
    Path::new("output.pmx"),
    &options,
)?;

// FBX to PMX
let stats = convert_fbx_to_pmx(
    Path::new("input.fbx"),
    Path::new("output.pmx"),
    &options,
)?;

println!("Bones: {}, Vertices: {}", stats.bones, stats.vertices);
```

## Tests

```bash
cargo test
```

Run the full suite via `cargo test`. Integration tests support environment variables for test data paths:

```bash
# Test data root directory
export POPONE_TEST_DATA=/path/to/test-fixtures

# Or specify individual files
export POPONE_TEST_VRM_SEED_SAN=/path/to/Seed-san.vrm
export POPONE_TEST_PMX_SEED_SAN=/path/to/Seed-san.pmx
export POPONE_TEST_PMD_MIKU_V2=/path/to/初音ミクVer2.pmd
```

## Changelog

For detailed per-version improvements and internal changes, see the [Changelog](CHANGELOG.md).

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
