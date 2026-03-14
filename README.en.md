# vrm2pmx

[日本語](README.md)

A 3D viewer and converter for VRM / FBX / UnityPackage files to PMX (MikuMikuDance) format.
Single Rust binary — launches as a viewer with no arguments, or as a CLI converter with arguments.

## Download

Latest release: **[vrm2pmx_v0.1.17.exe](https://github.com/tinatsu-nomy/vrm2pmx/releases/download/v0.1.17/vrm2pmx_v0.1.17.exe)**

All releases: [Releases](https://github.com/tinatsu-nomy/vrm2pmx/releases)

## Supported Formats

| Input | Description |
|-------|-------------|
| VRM 0.0 / 1.0 (`.vrm`) | glTF 2.0-based VR avatar format |
| FBX Binary (`.fbx`) | Custom parser. Auto-detects Mixamo / Blender / Maya rigs |
| UnityPackage (`.unitypackage`) | Extracts FBX + textures from tar.gz archive |

| Output | Description |
|--------|-------------|
| PMX 2.0 (`.pmx`) | For MikuMikuDance / PmxEditor. Auto-inserts MMD standard bones, IK, and physics |
| Texture PNG | Output to `textures/` folder |
| UV Map PSD | Per-material layers (from viewer) |

## Quick Start

```bash
# Launch viewer (or double-click the exe)
vrm2pmx.exe

# CLI conversion
vrm2pmx.exe input.vrm output.pmx
vrm2pmx.exe input.fbx output.pmx
vrm2pmx.exe input.unitypackage output.pmx
```

In the viewer, drag & drop files or use the "Open" button, then click "PMX Convert" in the right panel.

## Features

### Viewer

- **3D Rendering** — Real-time rendering with egui + wgpu. Textured Lambert shading, double-sided, alpha blending
- **Camera** — Left drag: rotate, Right drag: pan, Scroll: zoom. F: fit, R: reset
- **Expression Morphs** — Adjust with sliders (0/1 buttons, direct input)
- **Material Visibility** — Per-material ON/OFF toggle with search filter
- **Texture Assignment** — Assign external textures (PNG/JPG/TGA/BMP/PSD) via drag & drop or dialog. Real-time preview. VRM embedded texture replacement supported (reset button to restore)
- **Same-Name Material Linking** — ON/OFF toggle to assign textures to all materials sharing the same name simultaneously
- **UnityPackage Support** — FBX selection dialog, auto texture matching (manual assignment with thumbnail preview and search filter)
- **Wireframe** — 3 modes (Solid / Wire / S+W). W key to cycle
- **Bone & Physics Visualization** — Wireframe display of joints, rigid bodies, and colliders
- **Normal Tools** — Normal smoothing, custom normal clear, normal direction visualization
- **MSAA** — 4x anti-aliasing (toggleable)
- **UV Map Export** — PSD output with per-material layers (1024–8192 resolution)
- **PMX Conversion** — Convert directly from viewer with UI options (A-stance, rigid body alignment)

<details>
<summary>Keyboard Shortcuts</summary>

| Key | Function |
|-----|----------|
| Ctrl+O | Open file |
| F | Fit to model |
| R | Reset camera |
| G | Toggle grid |
| B | Toggle bones |
| P | Toggle physics |
| W | Cycle wireframe |
| N | Toggle normals |
| L | Cycle light mode |

</details>

### FBX Support

- Custom binary FBX parser (scene graph, coordinate system conversion, PreRotation, UnitScaleFactor)
- Skin weights (up to 4 bones, normalized), blend shapes, UV mapping
- Humanoid rig auto-detection (Mixamo / 3ds Max Biped / Maya HumanIK / VRoid / Blender)
- Zero-normal auto-repair, embedded/external texture support

### PMX Conversion

- Auto-detection of VRM 0.0 / 1.0 / FBX / UnityPackage
- MMD standard bone insertion (master, center, groove, waist, leg IK, toe IK)
- Semi-standard bones (waist cancel, leg D, toe EX, arm twist, wrist twist, shoulder cancel)
- VRM Expression to PMX morph conversion
- VRM SpringBone to PMX rigid body / joint conversion (gravity, rotation/movement limits, collider masks)
- A-stance conversion, rigid body rotation alignment options
- MToon outline to PMX edge mapping
- Auto-classified display frames (Root / Expression / Upper Body / Arms / Fingers / Legs / Other)
- UV normalization (clamped to 0..1)

## Build

```bash
# CLI only (conversion only)
cargo build --release

# With viewer
cargo build --release --features viewer
```

Output: `target/release/vrm2pmx.exe`

> **Windows GUI Subsystem**: Exe built with `--features viewer` won't show a console window. When run with CLI arguments, it auto-attaches to the parent console.

## CLI Options

```bash
vrm2pmx <input> <output.pmx> [options]

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

`vrm2pmx` can also be used as a library:

```rust
use vrm2pmx::{convert_vrm_to_pmx, convert_fbx_to_pmx};
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

## Dependencies

<details>
<summary>Core (CLI conversion)</summary>

| Crate | Purpose |
|-------|---------|
| gltf | GLB/glTF 2.0 parser (with `extensions` feature) |
| serde / serde_json | VRM extension JSON deserialization |
| glam | 3D math (Vec3, Quat, Mat4) |
| byteorder | PMX binary writing (little-endian) |
| image | Texture PNG encoding |
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

## License

[0BSD License](LICENSE) — Free to use, modify, and redistribute without attribution.

## Disclaimer

- The output PMX files are intended for further adjustment in tools like PmxEditor.
- The author assumes no responsibility for any issues arising from the use of this tool.
