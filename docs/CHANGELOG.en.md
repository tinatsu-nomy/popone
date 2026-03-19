# Changelog

[日本語](CHANGELOG.md)

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
- Humanoid rig auto-detection (Mixamo / 3ds Max Biped / Maya HumanIK / VRoid / Blender)
- Zero-normal auto-repair, embedded/external texture support
