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
    - [Material Editor (v0.5.0 – v0.5.4)](#material-editor-v050--v054)
    - [Per-Vertex UV Editor (v0.5.5 – v0.5.6)](#per-vertex-uv-editor-v055--v056)
    - [MME (ray-mmd) Output (v0.5.0)](#mme-ray-mmd-output-v050)
  - [Shader Support](#shader-support)
    - [Shader Detection](#shader-detection)
    - [Reproduction Fidelity (Viewer / PMX Conversion)](#reproduction-fidelity-viewer--pmx-conversion)
  - [Notes & Limitations](#notes--limitations)
  - [Build](#build)
  - [CLI Options](#cli-options)
  - [Output Files](#output-files)
  - [Conversion Example](#conversion-example)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->



# Usage

[日本語](usage.jp.md)

## Supported Formats

| Input | Description |
|-------|-------------|
| VRM 0.0 / 1.0 (`.vrm`) | glTF 2.0-based VR avatar format |
| FBX Binary (`.fbx`) | Custom parser. Auto-detects Mixamo / Blender / VRoid / Unreal rigs. Namespace prefixes (`Model::`, etc.) supported |
| PMX 2.0 / 2.1 (`.pmx`) | MikuMikuDance model format. Viewer display + UV map export |
| PMD (`.pmd`) | MikuMikuDance model format. Shift_JIS support |
| OBJ (`.obj`) | Wavefront OBJ format. Auto-loads MTL material files and textures. Import options dialog allows unit selection (mm/cm/m/inch) and Z-Up toggle (default: cm, Y-Up) |
| STL (`.stl`) | STL format (ASCII and binary). Import options dialog allows unit selection and Z-Up toggle (default: mm, Z-Up → Y-Up) |
| DirectX text (`.x`) | DirectX text format. Supports static meshes for MMD accessories and stages. Frame hierarchy transforms, material references, and DDS textures |
| UnityPackage (`.unitypackage`) | Extracts Prefab / VRM / FBX + textures from tar.gz archive. Prefab-based texture and normal map auto-mapping supported |
| ZIP (`.zip`) | Auto-detects and extracts VRM / FBX / PMX / PMD / OBJ / STL / DirectX .x / UnityPackage from archive |
| 7z (`.7z`) | Auto-detects and extracts VRM / FBX / PMX / PMD / OBJ / STL / DirectX .x / UnityPackage from archive |
| RAR (`.rar`) | Auto-detects and extracts VRM / FBX / PMX / PMD / OBJ / STL / DirectX .x / UnityPackage from archive (RAR4 / RAR5, extraction only) |

Archives nested one level deep (a ZIP / 7z / RAR inside the archive) are also expanded and their models merged into the selection list.

Text files bundled in an archive (`.txt` / `.md`; readmes, terms of use, etc.) are listed separately from model files and can be opened in separate windows (see "Archive Text Viewer" under "Features > Viewer").

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

- **Appearance Presets (v0.5.17)** — The appearance settings window, opened from the "Appearance" button at the right end of the top bar, switches between System / Light / Dark / Custom. The default is the Blender / Substance Painter style dark theme (unified color scheme for panels, buttons, and tooltips). "System" follows the OS light/dark setting live at runtime; "Custom" starts from the look in effect at the moment of switching and lets you edit six colors (background, buttons, text, border, accent, active) via GUI color pickers (the "Reset colors" button restores the defaults). Side panel fixed at 280px with flat tab bar. Displays a rounded-corner splash image centered in the viewport when no model is loaded
- **3D Rendering** — Real-time rendering with egui + wgpu. Textured Lambert shading, double-sided, alpha blending. VRM MToon materials are displayed with 2-color toon shading (lit/shade smoothstep interpolation) + outline rendering (inverted hull method) + rim lighting (parametric rim + MatCap texture) + auxiliary textures (shadeMultiply / shadingShift / rimMultiply, with texCoord / KHR_texture_transform support) + UV animation (scroll/rotation) + emissive (emission) + normal mapping (MikkTSpace tangent generation for TBN construction, doubleSided back-face normal flipping) + MToon spec-compliant 4-phase draw order control (OPAQUE → MASK → BlendZWrite → Blend, with `transparentWithZWrite` / `renderQueueOffsetNumber` + dynamic camera distance sorting within BLEND). VRM 0.x MToon properties are fully normalized to VRM 1.0 (UniVRM migration compliant). All textures including base color support `texCoord` / `KHR_texture_transform`. Per-texture glTF sampler address modes (Repeat / ClampToEdge / MirroredRepeat) and filter modes (including all 6 minFilter mipmap selection values) are honored with individual samplers per texture. UTS2 (Unity-Chan Toon Shader) / lilToon / Poiyomi materials are auto-detected and displayed via MToon approximation (see "Shader Support" section for details). PMX/PMD displayed in MMD rendering mode (NdotL-dependent toon shading, edges, sphere maps). Lighting uses light color + hemisphere ambient (Sky/Ground 2-color interpolation) for VRoidHub-like ambient lighting
- **Camera** — Left drag: rotate, Right drag: pan, Scroll: zoom. F: fit, R: reset, Double-click: fit, Shift: precision mode (1/3 speed). FOV 30° (MMD-compliant)
- **Expression Morphs** — Adjust with sliders (0/1 buttons, direct input). Text filter for narrowing by name (partial match on Japanese/English names, case-insensitive). v0.5.1 adds playback of VRM 1.0 Expression **`materialColorBinds` (6 color targets: color / emissionColor / shadeColor / matcapColor / rimColor / outlineColor)** and **`textureTransformBinds` (UV scale / offset)**. Multiple simultaneously-active expressions blend additively per the VRM 1.0 spec: `finalValue = baseValue + Σ((targetValue - baseValue) × weight)`
- **Material Visibility** — Per-material ON/OFF toggle with search filter. Hovering over a material name shows a tooltip listing referenced texture filenames (base, sphere, toon, normal, emissive). Hovering over a material row highlights the corresponding mesh in the 3D view with semi-transparent orange overlay. Materials are always grouped by model name with collapsible headers (multiple FBX from Prefab shown as separate groups). Group headers include `[S]` (normal smoothing), `[C]` (custom normal clear), `[N]` (normal map ON/OFF), `[B]` (emissive ON/OFF), and `[☑]` (visibility) batch buttons. Hovering over the header highlights all meshes in the group
- **Meta Info Panel** — Displays VRM model info, author, permissions, and license with Japanese labels. Permission/license values shown as color badges (allow = green / conditional = yellow / deny = red / neutral = gray). Hover tooltips on both labels and values. Supports VRM 0.0/1.0. CJK font fallback (JP → SC) renders Chinese model names and author names correctly
- **File Hierarchy Tree** — Displays the load chain from opened file to final model in a tree view. Textures, animations, and package textures are also listed
- **Texture Assignment** — Assign external textures (PNG/JPG/TGA/BMP/PSD) via drag & drop or dialog. Real-time preview. VRM embedded texture replacement supported (reset button to restore)
- **Texture Assignment History** — Save/recall manually assigned textures for FBX/OBJ models to `popone_history.json` ("Save Textures" / "Recall Textures" buttons). Automatic name-based material matching even when order changes. Overwrite confirmation dialog
- **Same-Name Material Linking** — ON/OFF toggle to assign textures to all materials sharing the same name simultaneously
- **Session Settings Persistence** — Window size/position, last-opened directories, log settings, log viewer settings, and theme colors saved to `popone.toml` (in `%LOCALAPPDATA%\popone` on Windows) and restored on next launch. Log level (`[log] level`) is configurable (the `[log] keep` setting is no longer active since automatic log rotation was removed in v0.4.0). Log viewer visibility, position, size, and level filters are persisted under the `[log_viewer]` section (the `[window]` sub-section in that file uses the same layout as the main `popone.toml` as of v0.5.9). The right-side panel width is persisted under `[window] right_panel_width` (v0.5.9). The appearance preset (`[theme]` section: `mode` = `system` / `light` / `dark` / `custom`) and theme colors (same section: `panel_bg` / `border` / `accent` / `text` / `widget_bg` / `active`, 6-digit hex values, applied when `mode = "custom"`) are also editable from the GUI "Appearance" settings (v0.5.17). Multi-display support
- **Log Viewer (Separate Window)** — The top-bar "ログ" button opens an OS-level separate window that streams the in-memory log buffer in real time. Includes per-level filters (Error / Warn / Info / Debug, color-coded), auto tail-follow (sticks to bottom on new lines, pauses on manual scroll), an "Open folder" button, and a "Save log" button (writes `.log` to any user-chosen path). Built on `show_viewport_deferred`, so the main 3D scene is not forced to re-render when log lines arrive — the rendering cost on log inflow is nearly zero. Supports moving to a different monitor and minimizing independently of the main window. Visibility, position, size, and filter state are persisted in the `[log_viewer]` section of `popone.toml`. As of v0.4.0, log files are no longer auto-saved on normal exit ("don't save anything except panic logs" policy); use the "Save log" button if you want to keep a session's logs
- **Archive Text Viewer (Separate Windows, v0.5.16)** — Lists text files bundled in ZIP / 7z / RAR archives (`.txt` / `.md`; readmes, terms of use, etc.) and opens their content in OS-level separate windows on click. A "Text (n)" button appears on the top bar only when the archive contains text documents. Multiple documents can be open at once, so a readme can be read while inspecting the model. The same list also appears in the multi-model selection dialog, letting the readme guide which model to load. Encoding is auto-detected (UTF-8 with/without BOM, UTF-16 with BOM, Shift_JIS) and line endings are normalized. Texts inside nested archives (one level) are included. The list is replaced per archive load (merged on append) and stays available after loading a plain model file, until the next archive is opened. When listing fails because a nested archive is password-protected, the outer texts (readmes, which often hold the password hint) are still listed alongside the password dialog. Files over 4 MB are skipped; display is capped at the first 1,000,000 characters
- **UnityPackage Support** — Prefab / VRM / FBX model selection dialog (checkboxes for batch loading multiple models at once). When selecting a Prefab, textures and normal maps are auto-mapped via Unity's GUID reference chain (`.prefab` → FBX `.meta` → `.mat` → texture/normal map, with `_BumpMap` / `_NormalMap` + `_BumpScale` support). Supports New, Old, Unpacked, Mixed, and Variant formats. Prefabs referencing multiple FBX files are merged for display. Prefab append loading is also supported. Auto texture matching (manual assignment with thumbnail preview and search filter). When loaded via a Prefab, the default PMX output filename is derived from the Prefab name, allowing multiple Prefabs from the same UnityPackage to be converted and distinguished
- **Wireframe** — 3 modes (Solid / Wire / S+W). W key to cycle. Wire mode unifies all rendering (including outlines and MMD edges) into wireframe
- **Bone Display** — Flag-based shape rendering. Normal = ◎ (double circle + filled center), Move = ◻ (square + filled center), Axis-fixed = ⊗ (circle + ✕), IK Controller = ◻ (blue outline + orange fill + blue center). IK-affected bones (Link) in orange. Tail-based drawing for PMXEditor-compliant direction display. Constant screen-space size
- **Physics Visualization** — Rigid bodies (sphere/capsule/box) in 1px wireframe. PMX/PMD colored by physics_mode (bone-follow = green, physics = red, physics+bone = blue), VRM colored by group (collider = red, spring = green). Capsules include hemisphere wireframes (PMX/PMD)
- **Joint Display** — PMX/PMD joints visualized as yellow cubes (rotation-aware, animation-synced). Adjustable opacity
- **Shader Override** — 6 shader modes switchable via ▲ ComboBox ▼: Auto (auto-selects based on model format) / MToon/Lambert (force Standard path) / Unlit (texture color only) / GGX Preview (simplified Cook-Torrance specular) / Normal (normal→RGB visualization) / MMD (MMD dedicated path). Resets to Auto when loading a new model
- **Normal Tools** — Normal smoothing ✨, custom normal clear 🗑 (compatible with normal maps: smoothing TBN base normals improves polygon edge visibility), normal map ON/OFF 🗺, normal direction visualization. v0.5.3 adds bulk `法線平滑化 [on][off]` / `カスタム法線クリア [on][off]` button rows above the material list.
- **MSAA** — 4x anti-aliasing (toggleable). MASK (cutout) materials enable alpha_to_coverage on both surface and outline passes for reduced jaggies on eyelashes, hair cards, etc.
- **Missing-texture fallback (v0.5.7)** — When a texture referenced by PMX (or any other format) does not exist on disk or fails to decode, the previous behaviour substituted a **1×1 magenta** pixel, causing strong pink color bleed on materials that used it as a toon or sphere map. The default now substitutes **1×1 white** so toon/sphere composition stays neutral. A `テクスチャ欠落時フォールバックを白に` toggle in the Display tab switches between white (default) and magenta (diagnostic) instantly — no model reload needed (all failure paths share one 1×1 texture that is rewritten via `queue.write_texture`).
- **Bloom** — Dual Kawase post-effect. Only emissive components produce bloom (separated via MRT). Intensity, threshold, and radius adjustable in the UI. PMX/PMD materials with specular=(0,0,0) and specular_power≥100 are automatically bloom targets. Also supports Prefab Emission textures/colors. lilToon Screen-blend emission is attenuated to prevent white-out. Zero GPU cost when disabled. Per-material `[B]` toggle for individual emissive ON/OFF control. HDR emissive (component > 1.0) materials default to OFF to prevent white-out
- **Editable Model Name (PMX output filename + title bar)** — A "Model name" text input in the top bar and another in the right-side "PMX Conversion" panel share the same value, and changes are reflected immediately in the window title, the PMX output filename, and the default UV map export filename. The initial value is determined by the load source: single file → file name; single Prefab → Prefab name; archive (zip / 7z / unitypackage) → archive file name. Appended models preserve the name decided at the first load. The edited name is also retained across reloads (A/T stance toggling, etc.)
- **UV Map Export** — PSD output with per-material layers (1024–8192 resolution). UV boundary wrap handling for triangles crossing 0/1 edges. Groups layers into folders by model name when multiple models are merged. The save dialog defaults to the loaded model's source directory (the archive's own directory when loaded via an archive), and the default filename is `{model name}.psd` derived from the top-bar "Model name" field. When the layer data would exceed the PSD format's ~2 GiB length limit (high resolution × many materials × merged models), the writer automatically switches to **PSB (Large Document Format, `.psb`)** and the output extension becomes `.psb`; a toast states that the format was auto-promoted. PSB is openable in Photoshop CS / 2021+, Krita, Affinity Photo, and GIMP. Normal-sized exports are unaffected and stay `.psd`
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
| Esc | Cancel loading / GPU build / PMX conversion / close selection dialog |

</details>

### PMX / PMD Loading

- **PMX 2.0 / 2.1** — Full data structure loading (vertices, faces, materials, bones, morphs, display frames, rigid bodies, joints). SoftBody (2.1) is skipped
- **PMD** — Automatic Shift_JIS text conversion. IK and morph (base+offset) support. Material name text file (same-name `.txt`) loading
- **Textures** — Auto-loads PNG/JPEG/BMP/TGA from PMX/PMD relative paths. MIME hint-based format detection. Sphere maps (.sph/.spa) supported
- **MMD Rendering** — Toon shading (shared toon01-10 + individual toon), Blinn-Phong specular, sphere maps (multiply/add), edge drawing (inverted hull method, toggle/thickness adjustable). Light color and intensity changes are reflected in MMD rendering. Ambient UI is disabled in MMD mode (LightAmbient serves as scene ambient). Edge drawing UI is also shown in Auto mode when loading PMX/PMD
- **T-Stance Conversion** — Convert A-stance models to T-stance (bones, mesh, morphs, rigid bodies, joints synced)
- **VRMA Animation** — Auto-mapping from PMX Japanese bone names to VRM humanoid names enables VRMA animation playback. Supports rotation/move grants, so D-bones (leg D, etc.) correctly follow FK animations
- **UI Restrictions** — PMX conversion button, normal smoothing, and custom normal clear are grayed out when PMX/PMD is loaded. "Outline drawing" checkbox is also grayed out for models without MToon outlines
- **Comment Display** — PMX/PMD comments shown in model info panel

### Changelog

See [Changelog](CHANGELOG.md) for version-by-version changes.

## Extras

### Animation Playback

- Load VRMA / glTF / FBX animations via drag & drop or dialog
- Humanoid retargeting support (apply across different models)
- 4 loop modes (None / Normal / A-B repeat / Ping-pong)
- Speed control, frame stepping, seek bar, expression keyframe sync
- Automatic bone pose and expression morph reset on animation clear/removal

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

- In the viewer, "PMX Convert" button exports immediately to `converted_modelXX/` directory. Output folder opens automatically in Explorer. Base output directory is configurable in the "Export" tab. Output PMX filename is taken from the editable "Model name" field in the top bar / right panel. The initial value is the source filename (the Prefab name when loaded via a Prefab, or the archive filename when loaded via an archive)
- Auto-detection of VRM 0.0 / 1.0 / FBX / UnityPackage / ZIP / 7z / RAR
- MMD standard bone insertion (master, center, groove, waist, leg IK, toe IK)
- Semi-standard bones (waist cancel, leg D, toe EX, arm twist, wrist twist, shoulder cancel)
- VRM Expression to PMX morph conversion
- VRM SpringBone to PMX rigid body / joint conversion (gravity, rotation/movement limits, collider masks)
- A-stance conversion / T-stance conversion (for FBX, persistent viewport warning on failure/skip), rigid body rotation alignment options
- No-physics export (skip rigid bodies/joints), raw structure export (skip standard bone insertion + keep original bone names), export scale multiplier (`--scale`)
- Boneless models (static FBX, etc.) automatically get a single dummy bone at origin with all vertex weights assigned
- MToon outline to PMX edge mapping
- Auto-classified display frames (Root / Expression / Upper Body / Arms / Fingers / Legs / Other)
- UV normalization (clamped to 0..1)

### Material Editor (v0.5.0 – v0.5.4)

Click the ✏ icon on any material row in the Display tab to open the Material Editor as a docked panel directly above the shortcut hint bar. Close it with the `[×]` button at the top right.

- **Material Name Editing (v0.5.3)** — The TextEdit at the top of the panel lets you rename a material in place. The change is recorded into `material_overrides` and is restored across reload / A-stance conversion / history save.
- **Editable parameters** — diffuse color, alpha mode/cutoff, shade color, shading toony/shift, outline color/width/mode, parametric rim, matcap factor, emissive factor, normal scale, UV animation speeds, render queue offset
- **Texture slot assignment (refactored in v0.5.2)** — Each parameter section now shows the related texture thumbnail (32px square) as a button at its top:
  - **Basic**: BaseColor
  - **Shade (影)**: Shade / ShadingShift
  - **Outline (アウトライン)**: OutlineWidth
  - **Rim (リム)**: RimMultiply
  - **MatCap**: Matcap
  - **UV Animation (UV アニメ)**: UvAnimMask
  - **Emissive / Normal (エミッシブ / 法線)**: Emissive / Normal
  - **MMD Textures (Sphere / Toon)**: Sphere / Toon (MMD/PMX-specific, kept in a dedicated section)

  Clicking a thumbnail opens a file dialog to replace or assign a texture. When a texture is assigned, hovering the thumbnail shows the filename as a tooltip. Unassigned slots render a placeholder button with an `×` symbol. A small `×` reset button at the row end clears individual slots when they are assigned.
- **Per-slot UV Transform Editing (v0.5.4)** — A single-line editor (`UV: off [x] [y]  scale [x] [y]  rot° [⟲]`) appears directly beneath each texture slot thumbnail whenever a texture is assigned. It edits the KHR_texture_transform offset / scale / rotation fields directly; rotation is entered in degrees and stored as radians. The ⟲ button resets a slot to offset=0 / scale=1 / rotation=0. Changes are persisted per normalized model path in `popone_history.json` and restored across reload / A-stance conversion / viewer restart. The path is independent from Expression-driven UV animation, so both coexist cleanly.
- **MToon enable/disable** — "MToon 有効化" checkbox to promote a non-MToon material
- **Presets** — MToon 1.0 Default, lilToon Standard, PMX Compatible (3 built-in presets)
- **Copy / Paste (v0.5.1)** — Copy a material's color/scalar values to the session clipboard and paste them onto another material. Texture assignments are intentionally excluded (path-dependent)
- **Dirty indicator (v0.5.1)** — A trailing `*` in the window title indicates the current material has unsaved edits (parameter overrides, BaseColor texture, or auxiliary slot texture)
- **Reset** — Per-slot `×` button to clear individual textures, "初期値に戻す" to restore the material to its load-time state
- **Live preview** — Changes are reflected immediately in both standard and MMD render paths
- **Persistence** — Color/scalar edits **and all texture slot assignments** (v0.5.1 extended auxiliary slots) are saved in `popone_history.json` and restored on reload
- **PMX non-support badges (visually strengthened in v0.5.1)** — Parameters not representable in PMX format show a color-coded `⚠ PMX 非対応` badge at the top of the relevant section with a hover tooltip explaining that MME (.fx) output and viewer preview do honor them

### Per-Vertex UV Editor (v0.5.5 – v0.5.6)

The "UV 編集" button in the material editor header opens a dedicated window for **per-vertex** UV editing on the active material. It complements the per-slot UV transform (v0.5.4) for finer adjustment and supports editing of PMX UV morphs.

- **Canvas** — A square canvas (up to 260×260 px) renders the triangle wireframe in UV space with **v=0 at the top**, matching the `convert/uvmap.rs` PSD export
- **Vertex pick & drag** — Click within 12 px of a vertex to select (highlighted yellow); dragging translates. Edits are written directly into `IrMesh.vertices_mut()[*].uv` and are preserved through PMX re-export
- **Multi-select** — Shift+click to add, Ctrl+A to select all. Rect select adds with Shift+drag, subtracts with Ctrl+drag (v0.5.5 Phase 3 A-4)
- **Rotate / scale** — Alt-drag rotates around the selection bbox centre; corner handles scale around the opposite corner (v0.5.5 Phase 3 A-5, 2D gizmo handles)
- **Zoom / pan** — Mouse wheel zooms around cursor (0.1–32×); middle drag pans
- **Undo / redo** — Ctrl+Z / Ctrl+Y, up to 50 entries
- **Texture background** — The assigned BaseColor texture can be shown behind the wireframe for pixel-accurate alignment
- **UV set switch** — A ComboBox switches between UV0 and UV1 editing (v0.5.5 Phase 3 A-1, for models with TEXCOORD_1)
- **Detachable OS window** — Toggle to lift the panel out of `egui::Window` into a native OS window (v0.5.5 Phase 3 A-3, useful on multi-monitor setups)
- **PMX UV morph editing (v0.5.5 Phase 3 A-2 / roundtrip completed in v0.5.6)** — The "編集対象" ComboBox at the top switches between "ベース UV" and any PMX UV morph. Selecting a morph lets you edit its per-vertex offsets:
  - **Auto weight 1.0 (v0.5.6)** — Entering morph edit mode automatically locks the target morph's weight to `1.0` so edits are visible in the main viewport. The original weight is restored on exit
  - **Side-panel slider lock (v0.5.6)** — While a UV morph is being edited, the matching row in the "表情モーフ" side panel disables its slider, `0` / `1` buttons, and DragValue, and shows a `(UV編集中)` marker. The "全リセット" button also skips it
  - **PMX roundtrip (v0.5.6)** — Edited UV morphs now survive PMX re-export as `PmxMorphOffsets::Uv`, so "PMX load → UV morph edit → PMX save → reload" round-trips correctly (v0.5.5 stubbed them out as empty group morphs)
- **History persistence** — "履歴を保存" writes per-vertex UV deltas to `popone_history.json`; "履歴呼出" restores them. Stored per model path

### MME (ray-mmd) Output (v0.5.0)

Generate ray-mmd 2.0 material `.fx` files alongside PMX conversion.

1. In the Export tab, check "MME マテリアル (.fx) も出力" under the PMX conversion section
2. (Optional) Click "フォルダ選択..." to set the ray-mmd root folder. Defaults to the current directory (`.\`) if not set
3. Run PMX conversion as usual. A `<model>_mme/` folder will be created next to the PMX with:
   - `material_<name>.fx` — One per material, with all ray-mmd parameters expanded (Albedo, Normal, Smoothness, Metalness, Specular, Occlusion, Parallax, Emissive, Shading Model)
   - `textures/` — Non-PMX textures (normal maps, emissive maps, etc.) referenced by `.fx` files
   - `README.txt` — MaterialMap assignment instructions

- **Category auto-detection** — Material names are matched against keyword patterns (skin/body/face, hair, cloth/dress, eye/glass, etc.) to select the appropriate `CUSTOM_ENABLE` value (Standard/Skin/HairAniso/Glass/Cloth/ClearCoat/Emissive)
- **Manual override** — Open the material editor and expand the "MME 出力 (ray-mmd)" section to manually select a category via ComboBox
- **Encoding** — All `.fx` and `README.txt` files use Shift-JIS encoding with CR+LF line endings for MMD/MME compatibility
- **Include path warning** — If `material_common_2.0.fxsub` is not found at the resolved path, a warning is shown (files are still written)

## Shader Support

Shader information recorded in VRM 0.0 `materialProperties` is auto-detected and reflected in viewer display and PMX conversion.

### Shader Detection

| Shader | Detection Criteria |
|--------|-------------------|
| MToon | shader name contains "MToon" |
| UTS2 (Unity-Chan Toon Shader) | shader name contains "UnityChanToonShader", or `_utsVersion` property exists |
| lilToon | shader name contains "lilToon" / "lil/", or `_lilToonVersion` property exists |
| Poiyomi | shader name contains "poiyomi" (case-insensitive), or `_EnableShadow` + `_Shadow1stColor` properties exist |

### Reproduction Fidelity (Viewer / PMX Conversion)

| Shader | Viewer | PMX | Supported Parameters | Not Supported |
|--------|:------:|:---:|---------------------|---------------|
| MToon (VRM 1.0) | 98% | 90% | shade/toony/shift/outline/rim/matcap/UV anim/emissive/normal/GI/draw order/Expression materialColorBinds/textureTransformBinds (v0.5.1 added) | — |
| MToon (VRM 0.0) | 92% | 85% | Above + full UniVRM Migration-compliant property normalization | — |
| UTS2 | 75% | 70% | 1st shade/2nd shade/outline/rim/matcap/emissive/normal/HighColor(PMX only) | StencilMask, AngelRing, UTS2-specific lighting |
| lilToon | 60% | 55% | shade/2nd shadow/outline/rim/matcap/emissive/normal/alpha mode | Fur, Refraction, Gem, FakeShadow, AudioLink, Dissolve, distance fade |
| Poiyomi | 45% | 40% | 1st shadow/2nd shadow/outline/emissive/normal/alpha mode | Rim, MatCap, AudioLink, Dissolve, Glitter, Parallax, Decal |
| Other | - | - | glTF core baseColor/alpha/normal/emissive only | All shader-specific parameters |

> **Note**: lilToon / Poiyomi are approximate conversions to MToon parameters. Basic toon shading, outlines, and shade colors are reproduced, but advanced shader-specific features (fur, refraction, AudioLink, etc.) are not supported.

## Notes & Limitations

- **PMX output** — Output PMX files are intended for further adjustment in tools like PmxEditor
- **PMX/PMD is view-only** — PMX conversion (re-export) is not supported. Viewer display and UV map export only
- **Sphere Mode 3 (sub-texture) unsupported** — Requires additional UVs, not implemented. Detected with warning log and disabled
- **Texture size limit** — Textures exceeding the GPU's `max_texture_dimension_2d` (typically 8192px) are automatically downscaled. This may result in slight quality loss. Does not affect PMX conversion output (viewer display only)
- **Mipmap generation** — All textures are uploaded with a full mipmap chain. Downsampling is performed in linear color space (sRGB-correct) to eliminate moiré and aliasing when the camera is pulled back
- **Depth precision** — Reverse-Z depth buffer provides high precision at all distances, minimizing Z-fighting on large models and stages
- **Extraction size limit** — Archive (ZIP / 7z / RAR) and `.unitypackage` extraction is capped at 2GB total. Files exceeding this limit will produce an error
- **Password-protected archives are GUI-only** — Encrypted ZIP / 7z / RAR archives prompt for a password in the viewer. The password is used only for that load and is never saved; reloading prompts again. The CLI does not support encrypted archives
- **MMD-specialized models** — Models optimized for MMD-specific rendering may display some surfaces incorrectly
- **PMX 2.1 SoftBody** — Skipped (not supported)
- **Viewer display and PMX output do not match exactly** — The viewer renders with a PBR-like shader approximating MToon / lilToon / Poiyomi, while PMX conversion maps materials to MMD's own shading model (diffuse + ambient + shade color + edge + toon + sphere). Shader-specific parameters (additive emission, rim light, shade shift, MatCap, outline width control, reflection/refraction, etc.) have no 1:1 mapping in PMX / MMD, so the appearance when the PMX is opened in MMD may not match the viewer preview. Final adjustments are expected to be done in PmxEditor or MMD itself
- **A-stance / T-stance conversion takes noticeable time** — Toggling the conversion checkbox reloads the entire model from scratch each time, taking several to a dozen seconds on large models (e.g., 100K+ vertices). This is because the pose correction is applied destructively to the original `global_mats`, so switching state requires restoring the pre-correction data, and re-reading from the source file is adopted as the most reliable implementation strategy

## Build

```bash
# CLI only (conversion only)
cargo build --release

# With viewer
cargo build --release --features viewer
```

Output: `target/release/popone.exe`

> **Windows SDK**: [Windows SDK](https://developer.microsoft.com/windows/downloads/windows-sdk/) is recommended for embedding the exe icon (`rc.exe`). If not installed, the build will succeed but the exe will have no custom icon.

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
  --scale <FLOAT>         PMX export scale multiplier (default: 1.0)
  --model-name <NAME>     Specify model filename inside archive (for ZIP/7z/RAR)
  --fbx-name <NAME>       Specify FBX filename inside .unitypackage. When omitted,
                          auto-selects the largest FBX by file size (heuristic:
                          main model files are typically larger than animations or props)
  --list-models           List models inside archive and exit (for ZIP/7z/RAR)
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

