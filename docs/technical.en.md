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
| Rigid body rotation | Absolute Euler angles. Flip X if bone direction Y < 0 |

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

## PMX/PMD Loading (v0.2.1)

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

### T-Stance Conversion

`normalize_pose_to_tstance_full()` converts A-stance → T-stance:

1. Detect left/right upper arms (`vrm_bone_name` or PMX name `"左腕"` / `"右腕"`)
2. Calculate angle from arm direction to horizontal and generate inverse rotation correction quaternion
3. Correct bone positions and global matrices
4. Rotate mesh vertices and normals based on skin weights
5. Apply rotation to morph offsets
6. Rigid bodies / joints: correct position and rotation of those belonging to descendants of affected bones

### Rigid Body Rotation Adjustment

PMX/PMD rigid body rotation is stored as Euler angles (ZXY). X flip based on bone direction is required for viewer display:

```rust
// Flip X rotation if bone direction Y component < 0
if bone_dir.y < 0.0 {
    rotation.x = -rotation.x;
}
```

### Texture Loading

- PMX: Load from relative paths in the texture path table
- PMD: Separate sphere texture from material's `texture_name` using `*`, use only the main texture
- MIME hint: Infer MIME type from extension and explicitly specify via `image::load_from_memory_with_format` (TGA has no magic number so auto-detection fails)

## Viewer Display Styles

### Bone Display

- Shape: Double circle + triangle without base (◎△)
- Rendering: 1px LineList (`pipeline_line_overlay`)
- Color: Normal bone = blue `#0000ff`, IK bone = orange `#ff9600`
- Size: Scales with camera distance (constant screen size)
- IK detection: Whether bone name contains "ＩＫ" or "IK"

### Rigid Body Display

- Rendering: 1px LineList
- Color: Collider (group=1) = red `#ff0000`, Spring (group!=1) = green `#00ff00`
- Sphere: 8 meridians (great circle arcs) + 7 parallels
- Capsule: Top/bottom rings + 8 connecting lines
- Box: 12 edges

### Joint Display (PMX/PMD only)

- Shape: Unit cube (faces = yellow `#ffff00`, edges = 1px black lines)
- Size: 0.18 PMX units
- Rotation: Euler ZXY → Quat for pose reflection
- Animation sync: Follows via offset from rigid_a's bone
- Opacity: Adjustable via slider

### Normal Map Display

- In-shader normal vector → RGB conversion: `rgb = (normalize(normal) + 1.0) * 0.5`
- Toggled via CameraUniform's `show_normal_map` flag

### Render Order

Items drawn later appear in front:

1. Joints (farthest back)
2. Bones
3. Rigid bodies (frontmost)

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

## Model Append Loading (v0.2.3)

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

## Visible Materials Only Export (v0.2.3)

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

## Source File Structure

```
src/
├── main.rs              Entry point (no args or no output specified → viewer / output specified → CLI conversion)
├── lib.rs               Library API
├── error.rs             Error type definitions
├── unitypackage.rs      .unitypackage (tar.gz) asset extraction (VRM / FBX detection and extraction)
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
│   └── humanoid.rs      Humanoid rig auto-detection and mapping
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
│   └── uvmap.rs         UV map PSD output (material layer separation, boundary wrap support)
└── viewer/              ← Compiled only when feature = "viewer"
    ├── app.rs           eframe::App state management (TextureState / AnimLibrary sub-structs)
    ├── gpu.rs           wgpu pipeline / offscreen rendering / visualization buffer dirty flag
    ├── mesh.rs          IrModel → GPU vertex buffer conversion
    ├── texture.rs       Texture GPU upload (MIME hint support)
    ├── camera.rs        Orbit camera
    ├── grid.rs          Grid floor
    ├── ui.rs            Info panel / morph sliders / conversion button / PMX/PMD grayed out
    ├── export_filter.rs Visible materials only export filter (IrModel → filtered IrModel)
    └── animation.rs     Animation playback / retargeting (VRMA/glTF/FBX support)
```

## v0.2.2 Internal Improvements

### ViewerApp Sub-Struct Refactoring

In v0.2.2, ViewerApp's 43 fields were reduced to 30:

| Sub-struct | Field | Access | Contents |
|------------|-------|--------|----------|
| `TextureState` | `self.tex.*` | 9 fields | Texture assignment, package textures, preview, matching |
| `AnimLibrary` | `self.anim.*` | 4 fields | Animation playback state, library, Muscle scale |

Rust's partial borrowing allows simultaneous borrowing of `&mut self.tex` and `&self.anim`.

### GPU Visualization Buffer Cache Strategy

Bone, physics, and joint visualization vertices are managed with dirty flags:

| Input | Cache Key | Regeneration Condition |
|-------|-----------|----------------------|
| Bone vertices | `camera.eye()`, `bone_opacity` | Camera movement / opacity change / animation playing |
| SpringBone vertices | `spring_bone_opacity`, `align_rigid_rotation` | Settings change / animation playing |
| Joint vertices | `joint_opacity` | Settings change / animation playing |

Common to all buffers:
- `vertex_count == 0` → forced regeneration (recovery from hidden → visible toggle)
- `cache_had_anim && !has_anim` → forced 1-frame regeneration when animation is released

### Animation Vertex Buffer Optimization

Hot path improvement for `apply_bone_animation()`:

| Item | Before | After |
|------|--------|-------|
| Vertex buffer | `base.to_vec()` per-frame alloc | `reset_animated_to_base()` capacity reuse |
| Delta matrices | `Vec::with_capacity()` per-frame | Reuse `work_deltas` field |
| Globals computation | New `Vec` + clone | In-place update (`work_computed` flag reuse) |
| Morph application | `apply_morphs_to_buf(&self, &mut [Vertex])` | `apply_morphs_to_animated(&mut self)` borrow conflict avoidance |

### Bone Name Lookup HashMap Conversion

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

### Test Data Path Resolution

Integration test file paths can be configured via environment variables:

| Priority | Source | Example |
|----------|--------|---------|
| 1 | Per-file environment variable | `POPONE_TEST_VRM_SEED_SAN=/path/to/Seed-san.vrm` |
| 2 | Root environment variable + relative path | `POPONE_TEST_DATA=/fixtures` → `/fixtures/vrm-spec/.../Seed-san.vrm` |
| 3 | `CARGO_MANIFEST_DIR/..` | Default for local development |

## Limitations

- **PMX/PMD is view-only** — PMX conversion (re-export) is not supported. Only viewer display and UV map output
- **Normal maps not applied** — VRM/FBX normalTexture is not reflected in shading (can be viewed in normal map display mode)
- **Lat-style Hatsune Miku etc.** — Models specialized for MMD rendering may not display some surfaces correctly
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
- Rigid bodies and joints are Bullet Physics compatible (Euler angles use ZXY convention)
- Coordinate system is left-handed, Y-up, +Z forward, with custom scale units (1m = 12.5 in this tool)

### Key Points of the PMD Specification

- Little-endian binary format, magic `"Pmd"`
- Text is fixed-length Shift_JIS (bone name 20 bytes, comment 256 bytes)
- Vertex is fixed at 38 bytes (BDEF2 only, weight is integer 0-100)
- IK is stored in a separate section from bones
- Morphs use base + offset format (base morph global vertex positions + delta offsets)
- English header, toon textures, rigid bodies, and joints are optional extensions at end of file
