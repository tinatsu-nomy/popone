# popone

[日本語](README.md)

A 3D viewer for VRM / FBX / PMX / PMD / UnityPackage files.

## Download

Latest release: **[popone_v0.2.2.exe](https://github.com/tinatsu-nomy/popone/releases/download/v0.2.2/popone_v0.2.2.exe)**

All releases: [Releases](https://github.com/tinatsu-nomy/popone/releases)

## Disclaimer

- The output PMX files are intended for further adjustment in tools like PmxEditor.
- The author assumes no responsibility for any issues arising from the use of this tool.

## License

[0BSD License](LICENSE) — Free to use, modify, and redistribute without attribution.

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

### Model Append Improvements (v0.2.3)

- **2-Pass Bone Merge** — Order-independent candidate collection + propagation loop for same-name bone unification. Fixes incorrect merge of descendants in different subtrees
- **Pkg Texture Namespace** — Prevents texture name collisions when appending multiple UnityPackages (`{pkg_name}_pkg{seq}_{texture_name}` format). Also applied to auto-matched textures
- **ASCII FBX Content Handling** — Content blocks preserved as strings, maintaining parser-layer completeness
- **55 Tests** — Added bone merge, physics remap, morph vertex offset tests

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

![Architecture](docs/architecture.svg)

For detailed source structure, coordinate transforms, and bone insertion steps, see [Technical Details](docs/technical.md).

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

55 tests (44 unit + 11 integration). Integration tests support environment variables for test data paths:

```bash
# Test data root directory
export POPONE_TEST_DATA=/path/to/test-fixtures

# Or specify individual files
export POPONE_TEST_VRM_SEED_SAN=/path/to/Seed-san.vrm
export POPONE_TEST_PMX_SEED_SAN=/path/to/Seed-san.pmx
export POPONE_TEST_PMD_MIKU_V2=/path/to/初音ミクVer2.pmd
```

## Dependencies

<details>
<summary>Core (CLI conversion)</summary>

| Crate | Purpose |
|-------|---------|
| gltf | GLB/glTF 2.0 parser (with `extensions` feature) |
| serde / serde_json | VRM extension JSON deserialization |
| glam | 3D math (Vec3, Quat, Mat4) |
| byteorder | PMX binary read/write (little-endian) |
| image | Texture PNG/JPEG/BMP/TGA decode/encode |
| encoding_rs | PMD Shift_JIS text conversion |
| flate2 | zlib compression/decompression |
| tar | .unitypackage (tar.gz) extraction |
| clap | CLI argument parser |
| anyhow | Error handling |
| log / fern / chrono | Logging |

</details>

<details>
<summary>Viewer (viewer feature)</summary>

| Crate | Purpose |
|-------|---------|
| eframe | egui + wgpu window framework |
| rfd | Native file dialog |
| bytemuck | Vertex/uniform Pod conversion |
| pollster | Async blocking executor |

</details>

<details>
<summary>Dependency Licenses</summary>

### Core

| Crate | License |
|-------|---------|
| [gltf](https://github.com/gltf-rs/gltf) | MIT OR Apache-2.0 |
| [serde](https://github.com/serde-rs/serde) / [serde_json](https://github.com/serde-rs/json) | MIT OR Apache-2.0 |
| [glam](https://github.com/bitshifter/glam-rs) | MIT OR Apache-2.0 |
| [byteorder](https://github.com/BurntSushi/byteorder) | Unlicense OR MIT |
| [image](https://github.com/image-rs/image) | MIT OR Apache-2.0 |
| [encoding_rs](https://github.com/nickel-org/encoding_rs) | MIT OR Apache-2.0 |
| [flate2](https://github.com/rust-lang/flate2-rs) | MIT OR Apache-2.0 |
| [tar](https://github.com/alexcrichton/tar-rs) | MIT OR Apache-2.0 |
| [clap](https://github.com/clap-rs/clap) | MIT OR Apache-2.0 |
| [anyhow](https://github.com/dtolnay/anyhow) | MIT OR Apache-2.0 |
| [log](https://github.com/rust-lang/log) / [fern](https://github.com/daboross/fern) / [chrono](https://github.com/chronotope/chrono) | MIT (fern) / MIT OR Apache-2.0 (others) |

### Viewer

| Crate | License |
|-------|---------|
| [eframe](https://github.com/emilk/egui) | MIT OR Apache-2.0 |
| [rfd](https://github.com/PolyMeilex/rfd) | MIT |
| [bytemuck](https://github.com/Lokathor/bytemuck) | Zlib OR Apache-2.0 OR MIT |
| [pollster](https://github.com/zesterer/pollster) | MIT OR Apache-2.0 |

</details>
