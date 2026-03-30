<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [Usage](#usage)
  - [Supported Formats](#supported-formats)
  - [Quick Start](#quick-start)
  - [Features](#features)
    - [Viewer](#viewer)
    - [PMX / PMD Loading](#pmx--pmd-loading)
    - [Changelog](#changelog)
  - [Extras](#extras)
    - [Animation Playback](#animation-playback)
    - [PMX (MikuMikuDance) Conversion](#pmx-mikumikudance-conversion)
  - [Limitations](#limitations)
  - [Build](#build)
  - [CLI Options](#cli-options)
  - [Output Files](#output-files)
  - [Conversion Example](#conversion-example)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# Usage

[日本語](usage.md)

## Supported Formats

| Input | Description |
|-------|-------------|
| VRM 0.0 / 1.0 (`.vrm`) | glTF 2.0-based VR avatar format |
| FBX Binary (`.fbx`) | Custom parser. Auto-detects Mixamo / Blender / VRoid / Unreal rigs. Namespace prefixes (`Model::`, etc.) supported |
| PMX 2.0 / 2.1 (`.pmx`) | MikuMikuDance model format. Viewer display + UV map export |
| PMD (`.pmd`) | MikuMikuDance model format. Shift_JIS support |
| UnityPackage (`.unitypackage`) | Extracts Prefab / VRM / FBX + textures from tar.gz archive. Prefab-based texture mapping supported |
| ZIP (`.zip`) | Auto-detects and extracts VRM / FBX / PMX / PMD / UnityPackage from archive |
| 7z (`.7z`) | Auto-detects and extracts VRM / FBX / PMX / PMD / UnityPackage from archive |

## Quick Start

```bash
# Launch viewer (or double-click the exe)
popone.exe

# Open file in viewer
popone.exe input.vrm
popone.exe input.fbx
```

In the viewer, drag & drop files or use the "Open" button to load models.
If the viewer is already running, subsequent launches pass the file path to the existing window and exit automatically (single instance).

## Features

### Viewer

- **Dark Theme** — Blender / Substance Painter style dark theme. Unified color scheme for panels, buttons, and tooltips. Side panel fixed at 280px with flat tab bar
- **3D Rendering** — Real-time rendering with egui + wgpu. Textured Lambert shading, double-sided, alpha blending. VRM MToon materials are displayed with 2-color toon shading (lit/shade smoothstep interpolation) + outline rendering (inverted hull method) + rim lighting (parametric rim + MatCap texture) + auxiliary textures (shadeMultiply / shadingShift / rimMultiply, with texCoord / KHR_texture_transform support) + UV animation (scroll/rotation) + emissive (emission) + normal mapping (MikkTSpace tangent generation for TBN construction, doubleSided back-face normal flipping) + MToon spec-compliant 4-phase draw order control (OPAQUE → MASK → BlendZWrite → Blend, with `transparentWithZWrite` / `renderQueueOffsetNumber` + dynamic camera distance sorting within BLEND). VRM 0.x MToon properties are fully normalized to VRM 1.0 (UniVRM migration compliant). All textures including base color support `texCoord` / `KHR_texture_transform`. Per-texture glTF sampler address modes (Repeat / ClampToEdge / MirroredRepeat) and filter modes (including all 6 minFilter mipmap selection values) are honored with individual samplers per texture. UTS2 (Unity-Chan Toon Shader) materials are auto-detected and displayed via MToon approximation (1st shade / outline / rim / MatCap / emissive / normal supported; HighColor is PMX output only). PMX/PMD displayed in MMD rendering mode (NdotL-dependent toon shading, edges, sphere maps). Lighting uses light color + hemisphere ambient (Sky/Ground 2-color interpolation) for VRoidHub-like ambient lighting
- **Camera** — Left drag: rotate, Right drag: pan, Scroll: zoom. F: fit, R: reset, Double-click: fit, Shift: precision mode (1/3 speed). FOV 30° (MMD-compliant)
- **Expression Morphs** — Adjust with sliders (0/1 buttons, direct input)
- **Material Visibility** — Per-material ON/OFF toggle with search filter. Hovering over a material row highlights the corresponding mesh in the 3D view with semi-transparent orange overlay
- **Texture Assignment** — Assign external textures (PNG/JPG/TGA/BMP/PSD) via drag & drop or dialog. Real-time preview. VRM embedded texture replacement supported (reset button to restore)
- **Same-Name Material Linking** — ON/OFF toggle to assign textures to all materials sharing the same name simultaneously
- **UnityPackage Support** — Prefab / VRM / FBX model selection dialog. When selecting a Prefab, textures are auto-mapped via Unity's GUID reference chain (`.prefab` → FBX `.meta` → `.mat` → texture). Supports New, Old, Unpacked, Mixed, and Variant formats. Prefabs referencing multiple FBX files are merged for display. Auto texture matching (manual assignment with thumbnail preview and search filter)
- **Wireframe** — 3 modes (Solid / Wire / S+W). W key to cycle. Wire mode unifies all rendering (including outlines and MMD edges) into wireframe
- **Bone Display** — Flag-based shape rendering. Normal = ◎ (double circle + filled center), Move = ◻ (square + filled center), Axis-fixed = ⊗ (circle + ✕), IK Controller = ◻ (blue outline + orange fill + blue center). IK-affected bones (Link) in orange. Tail-based drawing for PMXEditor-compliant direction display. Constant screen-space size
- **Physics Visualization** — Rigid bodies (sphere/capsule/box) in 1px wireframe. PMX/PMD colored by physics_mode (bone-follow = green, physics = red, physics+bone = blue), VRM colored by group (collider = red, spring = green). Capsules include hemisphere wireframes (PMX/PMD)
- **Joint Display** — PMX/PMD joints visualized as yellow cubes (rotation-aware, animation-synced). Adjustable opacity
- **Shader Override** — 6 shader modes switchable via ▲ ComboBox ▼: Auto (auto-selects based on model format) / MToon/Lambert (force Standard path) / Unlit (texture color only) / GGX Preview (simplified Cook-Torrance specular) / Normal (normal→RGB visualization) / MMD (MMD dedicated path). Selection persists across model loads (only Auto auto-detects)
- **Normal Tools** — Normal smoothing, custom normal clear (both auto-disabled when normal-mapped materials are present), normal direction visualization
- **MSAA** — 4x anti-aliasing (toggleable). MASK (cutout) materials enable alpha_to_coverage on both surface and outline passes for reduced jaggies on eyelashes, hair cards, etc.
- **UV Map Export** — PSD output with per-material layers (1024–8192 resolution). UV boundary wrap handling for triangles crossing 0/1 edges. Groups layers into folders by model name when multiple models are merged
- **Model Append Loading** — Merge costume FBX etc. into existing model. Bone matching uses 3-level fallback (VRM humanoid name → FBX node name → PMX name) for correct merging across different naming conventions

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
- **Textures** — Auto-loads PNG/JPEG/BMP/TGA from PMX/PMD relative paths. MIME hint-based format detection. Sphere maps (.sph/.spa) supported
- **MMD Rendering** — Toon shading (shared toon01-10 + individual toon), Blinn-Phong specular, sphere maps (multiply/add), edge drawing (inverted hull method, toggle/thickness adjustable). Light color and intensity changes are reflected in MMD rendering. Ambient UI is disabled in MMD mode (LightAmbient serves as scene ambient)
- **T-Stance Conversion** — Convert A-stance models to T-stance (bones, mesh, morphs, rigid bodies, joints synced)
- **VRMA Animation** — Auto-mapping from PMX Japanese bone names to VRM humanoid names enables VRMA animation playback. Supports rotation/move grants, so D-bones (leg D, etc.) correctly follow FK animations
- **UI Restrictions** — PMX conversion button, normal smoothing, and custom normal clear are grayed out when PMX/PMD is loaded. "Outline drawing" checkbox is also grayed out for models without MToon outlines
- **Comment Display** — PMX/PMD comments shown in model info panel

### Changelog

See [Changelog](CHANGELOG.en.md) for version-by-version changes.

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
popone.exe archive.zip output.pmx
popone.exe archive.7z output.pmx --model-name "model.pmx"
```

| Output | Description |
|--------|-------------|
| PMX 2.0 (`.pmx`) | For MikuMikuDance / PmxEditor. Auto-inserts MMD standard bones, IK, and physics |
| Texture PNG | Output to `textures/` folder (PSD textures are automatically converted to PNG) |
| UV Map PSD | Per-material layers with model-based group folders (from viewer) |

- In the viewer, "PMX Convert" button exports immediately to `converted_modelXX/` directory. Output folder opens automatically in Explorer. Base output directory is configurable in the "Export" tab
- Auto-detection of VRM 0.0 / 1.0 / FBX / UnityPackage / ZIP / 7z
- MMD standard bone insertion (master, center, groove, waist, leg IK, toe IK)
- Semi-standard bones (waist cancel, leg D, toe EX, arm twist, wrist twist, shoulder cancel)
- VRM Expression to PMX morph conversion
- VRM SpringBone to PMX rigid body / joint conversion (gravity, rotation/movement limits, collider masks)
- A-stance conversion / T-stance conversion (for FBX, persistent viewport warning on failure/skip), rigid body rotation alignment options
- No-physics export (skip rigid bodies/joints), raw structure export (skip standard bone insertion + keep original bone names)
- MToon outline to PMX edge mapping
- Auto-classified display frames (Root / Expression / Upper Body / Arms / Fingers / Legs / Other)
- UV normalization (clamped to 0..1)

## Limitations

- **PMX/PMD is view-only** — PMX conversion (re-export) is not supported. Viewer display and UV map export only
- **Sphere Mode 3 (sub-texture) unsupported** — Requires additional UVs, not implemented. Detected with warning log and disabled
- **Texture size limit** — Textures exceeding the GPU's `max_texture_dimension_2d` (typically 8192px) are automatically downscaled. This may result in slight quality loss. Does not affect PMX conversion output (viewer display only)
- **Extraction size limit** — Archive (ZIP / 7z) and `.unitypackage` extraction is capped at 2GB total. Files exceeding this limit will produce an error
- **MMD-specialized models** — Models optimized for MMD-specific rendering may display some surfaces incorrectly
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
  --normalize-to-tstance  T-stance conversion (raise A-stance arms to horizontal, for FBX)
  --align-rigid-rotation  Align rigid body rotation to bone direction
  --raw-structure         Export with original bone structure (skip standard bone insertion + keep original bone names)
  --model-name <NAME>     Specify model filename inside archive (for ZIP/7z)
  --list-models           List models inside archive and exit (for ZIP/7z)
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

