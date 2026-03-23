# Changelog

[日本語](CHANGELOG.md)

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
