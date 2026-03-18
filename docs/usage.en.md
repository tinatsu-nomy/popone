# Usage

[日本語](usage.md)

## Supported Formats

| Input | Description |
|-------|-------------|
| VRM 0.0 / 1.0 (`.vrm`) | glTF 2.0-based VR avatar format |
| FBX Binary (`.fbx`) | Custom parser. Auto-detects Mixamo / Blender / Maya rigs |
| PMX 2.0 / 2.1 (`.pmx`) | MikuMikuDance model format. Viewer display + UV map export |
| PMD (`.pmd`) | MikuMikuDance model format. Shift_JIS support |
| UnityPackage (`.unitypackage`) | Extracts VRM / FBX + textures from tar.gz archive |

## Quick Start

```bash
# Launch viewer (or double-click the exe)
popone.exe

# Open file in viewer
popone.exe input.vrm
popone.exe input.fbx
```

In the viewer, drag & drop files or use the "Open" button to load models.

## Features

### Viewer

- **3D Rendering** — Real-time rendering with egui + wgpu. Textured Lambert shading, double-sided, alpha blending
- **Camera** — Left drag: rotate, Right drag: pan, Scroll: zoom. F: fit, R: reset
- **Expression Morphs** — Adjust with sliders (0/1 buttons, direct input)
- **Material Visibility** — Per-material ON/OFF toggle with search filter
- **Texture Assignment** — Assign external textures (PNG/JPG/TGA/BMP/PSD) via drag & drop or dialog. Real-time preview. VRM embedded texture replacement supported (reset button to restore)
- **Same-Name Material Linking** — ON/OFF toggle to assign textures to all materials sharing the same name simultaneously
- **UnityPackage Support** — VRM / FBX model selection dialog, auto texture matching (manual assignment with thumbnail preview and search filter)
- **Wireframe** — 3 modes (Solid / Wire / S+W). W key to cycle
- **Bone Display** — Double-circle + direction triangle (◎△), 1px lines. IK bones highlighted in orange. Constant screen-space size regardless of camera distance
- **Physics Visualization** — Rigid bodies (sphere/capsule/box) in 1px wireframe. Colliders = red, springs = green
- **Joint Display** — PMX/PMD joints visualized as yellow cubes (rotation-aware, animation-synced). Adjustable opacity
- **Normal Map View** — Visualize normal vectors as RGB colors (debug/inspection)
- **Normal Tools** — Normal smoothing, custom normal clear, normal direction visualization
- **MSAA** — 4x anti-aliasing (toggleable)
- **UV Map Export** — PSD output with per-material layers (1024–8192 resolution). UV boundary wrap handling for triangles crossing 0/1 edges

<details>
<summary>Keyboard Shortcuts</summary>

| Key | Function |
|-----|----------|
| Ctrl+O | Open file |
| F | Fit to model |
| R | Reset camera |
| 0 / 1 / 3 / 7 / 9 | View presets (Front / Left / Right / Top / Back) |
| 2 / 8 | Tilt (orbit down / up by 15°, 360° capable) |
| 4 / 6 | Pan (orbit left / right by 15°) |
| 5 | Toggle perspective / orthographic |
| . | Fit to model |
| G | Toggle grid |
| B | Toggle bones |
| P | Toggle physics |
| W | Cycle wireframe |
| N | Toggle normals |
| L | Cycle light mode |
| Space | Play/pause animation |
| Left/Right | Step frame (when paused) |

</details>

### PMX / PMD Loading

- **PMX 2.0 / 2.1** — Full data structure loading (vertices, faces, materials, bones, morphs, display frames, rigid bodies, joints). SoftBody (2.1) is skipped
- **PMD** — Automatic Shift_JIS text conversion. IK and morph (base+offset) support. Material name text file (same-name `.txt`) loading
- **Textures** — Auto-loads PNG/JPEG/BMP/TGA from PMX/PMD relative paths. MIME hint-based format detection
- **T-Stance Conversion** — Convert A-stance models to T-stance (bones, mesh, morphs, rigid bodies, joints synced)
- **VRMA Animation** — Auto-mapping from PMX Japanese bone names to VRM humanoid names enables VRMA animation playback
- **UI Restrictions** — PMX conversion button, normal smoothing, and custom normal clear are grayed out when PMX/PMD is loaded
- **Comment Display** — PMX/PMD comments shown in model info panel

### v0.2.4 Improvements

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

### v0.2.3 Improvements

- **Visible-Only Material Export** — Option to exclude hidden materials from PMX output (default OFF). Consistently filters materials, meshes, textures, vertex morphs, and group morphs
- **2-Pass Bone Merge** — Order-independent candidate collection + propagation loop for same-name bone unification. Fixes incorrect merge of descendants in different subtrees
- **Pkg Texture Namespace** — Prevents texture name collisions when appending multiple UnityPackages (`{pkg_name}_pkg{seq}_{texture_name}` format). Also applied to auto-matched textures
- **ASCII FBX Content Handling** — Content blocks preserved as strings, maintaining parser-layer completeness
- **61 Tests** — Added bone merge, physics remap, morph vertex offset, export filter tests

### Code Quality & Performance (v0.2.2)

- **Performance** — Eliminated per-frame vertex buffer allocation, HashMap O(1) bone lookup, GPU visualization dirty flags
- **Tests** — 10 → 51 tests. Coordinate roundtrip, bone name mapping, PMX write/read roundtrip, VRM→PMX E2E
- **Zero Clippy warnings** — `cargo clippy --all-targets --all-features -- -D warnings` fully clean
- **UX** — 4-pattern D&D overlay, 2-line operation hints, disabled UI tooltips

### FBX Support

- Custom binary / ASCII FBX parser (scene graph, coordinate system conversion, PreRotation, UnitScaleFactor)
- ASCII FBX: Content blocks (embedded textures) preserved as strings; external file fallback for texture recovery
- Skin weights (up to 4 bones, normalized), blend shapes, UV mapping
- Humanoid rig auto-detection (Mixamo / 3ds Max Biped / Maya HumanIK / VRoid / Blender)
- Zero-normal auto-repair, embedded/external texture support

## Extras

### Animation Playback

- Load VRMA / glTF / FBX animations via drag & drop or dialog
- Humanoid retargeting support (apply across different models)
- 4 loop modes (None / Normal / A-B repeat / Ping-pong)
- Speed control, frame stepping, seek bar, expression keyframe sync

### PMX (MikuMikuDance) Conversion

Convert directly from the viewer, or via CLI.

```bash
popone.exe input.vrm output.pmx
popone.exe input.fbx output.pmx
popone.exe input.unitypackage output.pmx
```

| Output | Description |
|--------|-------------|
| PMX 2.0 (`.pmx`) | For MikuMikuDance / PmxEditor. Auto-inserts MMD standard bones, IK, and physics |
| Texture PNG | Output to `textures/` folder |
| UV Map PSD | Per-material layers (from viewer) |

- Auto-detection of VRM 0.0 / 1.0 / FBX / UnityPackage
- MMD standard bone insertion (master, center, groove, waist, leg IK, toe IK)
- Semi-standard bones (waist cancel, leg D, toe EX, arm twist, wrist twist, shoulder cancel)
- VRM Expression to PMX morph conversion
- VRM SpringBone to PMX rigid body / joint conversion (gravity, rotation/movement limits, collider masks)
- A-stance conversion, rigid body rotation alignment options
- MToon outline to PMX edge mapping
- Auto-classified display frames (Root / Expression / Upper Body / Arms / Fingers / Legs / Other)
- UV normalization (clamped to 0..1)

## Limitations

- **PMX/PMD is view-only** — PMX conversion (re-export) is not supported. Viewer display and UV map export only
- **Normal maps not applied** — VRM/FBX normalTexture is not reflected in shading (can be inspected via Normal Map View mode)
- **Lat-style Hatsune Miku, etc.** — Models optimized for MMD-specific rendering may display some surfaces incorrectly
- **PMX 2.1 SoftBody** — Skipped (not supported)

## Build

```bash
# CLI only (conversion only)
cargo build --release

# With viewer
cargo build --release --features viewer
```

Output: `target/release/popone.exe`

> **Windows GUI Subsystem**: Exe built with `--features viewer` won't show a console window. When run with CLI arguments, it auto-attaches to the parent console and detaches when launching the viewer.

## CLI Options

```bash
popone <input> [output.pmx] [options]

When output is omitted, the viewer opens automatically (viewer feature build only).

Options:
  --dump                  Print bone/vertex counts only (no PMX output)
  --no-physics            Skip physics conversion
  --normalize-pose        A-stance conversion (lower T-pose arms)
  --align-rigid-rotation  Align rigid body rotation to bone direction
  --log-level <LEVEL>     Log level (error/warn/info/debug, default: info)
```

## Output Files

- **PMX** — Written to the specified path
- **Textures** — PNG files in `textures/` next to the PMX
- **Log** — `.log` file in the same directory (not generated with `--dump`)

## Conversion Example

Seed-san.vrm (VRM 1.0):

| Item | Count |
|------|-------|
| Bones | 175 |
| Vertices | 34,059 |
| Materials | 17 |
| Textures | 15 |
| Morphs | 17 |
| Rigid Bodies | 36 |
| Joints | 19 |

## Architecture

![Architecture](architecture.svg)

For detailed source structure, coordinate transforms, and bone insertion steps, see [Technical Details](technical.en.md).

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

59 tests (48 unit + 11 integration). Integration tests support environment variables for test data paths:

```bash
# Test data root directory
export POPONE_TEST_DATA=/path/to/test-fixtures

# Or specify individual files
export POPONE_TEST_VRM_SEED_SAN=/path/to/Seed-san.vrm
export POPONE_TEST_PMX_SEED_SAN=/path/to/Seed-san.pmx
export POPONE_TEST_PMD_MIKU_V2=/path/to/初音ミクVer2.pmd
```
