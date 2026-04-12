<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [Roadmap](#roadmap)
  - [Targeted for v0.5.0](#targeted-for-v050)
    - [Expression Material Binds (VRM 1.0)](#expression-material-binds-vrm-10)
    - [OBJ / STL Import Options UI Polish](#obj--stl-import-options-ui-polish)
    - [Background Load Internals Cleanup](#background-load-internals-cleanup)
  - [Future Work (No Target Version)](#future-work-no-target-version)
    - [Unity `.anim` Residuals](#unity-anim-residuals)
    - [Archive Parent-Directory References](#archive-parent-directory-references)
    - [MTL Subdirectory Resolution](#mtl-subdirectory-resolution)
  - [Code Quality](#code-quality)
    - [GPU Shader Automated Tests](#gpu-shader-automated-tests)
    - [7z Extraction Phase 1 Memory Peak](#7z-extraction-phase-1-memory-peak)
  - [External Feature Requests](#external-feature-requests)
    - [High Priority (3+ reviews)](#high-priority-3-reviews)
    - [Medium Priority (2 reviews)](#medium-priority-2-reviews)
    - [Low Priority (1 review)](#low-priority-1-review)
  - [Process Notes](#process-notes)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# Roadmap

[日本語](ROADMAP.jp.md)

This document tracks planned work, future improvements, and external feature requests for `popone`. Items in this list are **not** yet implemented. Completed work is recorded in [CHANGELOG.md](CHANGELOG.md).

Current target: **v0.5.1**

> **Note:** v0.5.0 shipped material editing + MME output ([CHANGELOG.md](CHANGELOG.md#v050-2026-04-13)). The carry-over items below were originally targeted for earlier releases and are now planned for v0.5.1+.

## Targeted for v0.5.1

### Expression Material Binds (VRM 1.0)

VRM 1.0 Expression supports material-level bindings that the viewer currently ignores during playback:

- **`materialColorBinds`** — bind Expression weights to shade color, rim color, outline color, and matcap color
- **`textureTransformBinds`** — bind Expression weights to per-material UV scale / offset

The MToon shader side already handles per-draw material color and UV transform parameters. What is missing is the Expression playback pipeline wiring: when an Expression weight changes, the viewer must iterate active binds and write interpolated values into the corresponding `MaterialUniform` entries.

### OBJ / STL Import Options UI Polish

The import options dialog exists, but the coordinate-system and unit presets should be:

- Remembered per directory (last-used preset)
- Expandable with a custom scale field
- Exposed in CLI via `--obj-unit` / `--stl-unit` flags

### Background Load Internals Cleanup

Follow-up to the async load pipeline:

- **`CpuParseInput::ArchiveEntry` variant** — needed when archive-internal browsing is promoted to background
- **`CpuParseInput::Reload` variant** — unifies reload-from-source paths currently handled separately

### Material Editor v0.5.1 Enhancements

Follow-up to the v0.5.0 material editor:

- UV transform editing for remaining 5 slots (currently BaseColor/Emissive/Normal/Shade/ShadingShift only)
- Sphere / Toon texture slot editing
- User custom preset save/load
- `DrawCall.material_buf` for uniform buffer partial update optimization
- Multiple material editor windows for side-by-side comparison
- sdPBR `.fx` generation
- MaskedMaterial (ray-mmd-MaskedMaterial) support
- MME `.fx` import (parse existing PMX material .fx files)

## Future Work (No Target Version)

### Unity `.anim` Residuals

The `.anim` importer works for basic humanoid animation but has known gaps:

1. **Muscle angle range precision** — current `min_deg` / `max_deg` values are estimates. Unity's `HumanTrait.GetMuscleDefaultMin/Max()` provides the authoritative per-muscle range (some are asymmetric left/right)
2. **Axis verification for extremities** — the X/Y/Z axis assignments in `muscle_vrm_axes()` for foot / hand / finger muscles have not been rigorously validated against Unity reference rigs
3. **Foot IK goals** — `LeftFootT/Q`, `RightFootT/Q` curves are currently ignored
4. **Blend-shape curves** — `.anim` BlendShape curves are not parsed

### Archive Parent-Directory References

Some archives reference textures via `../` parent-directory paths. `sanitize_rel_path` currently strips `..` for disk-based loads (security fix for texture path traversal). Supporting legitimate `../` traversal within archives requires:

- Detecting the archive-root bounded case (safe) vs. escaping outside (unsafe)
- Adding a separate sanitizer that allows `..` only within the archive content tree

Until this is implemented, affected archives will fail to locate textures with warnings.

### MTL Subdirectory Resolution

When a `.obj` references a `.mtl` in a subdirectory, textures referenced from the `.mtl` should resolve relative to the `.mtl`'s directory, not the `.obj`'s directory. Currently the `.obj`'s directory is used for all paths.

## Code Quality

### GPU Shader Automated Tests

There is no automated verification of the WGSL shader sources. Recommended:

- **`naga` WGSL compile test** — syntax validation for every shader in CI
- **Unit tests for Rust-side lighting math** — `calc_lighting_mtoon_core` and helpers should have deterministic unit tests with known inputs

### 7z Extraction Phase 1 Memory Peak

During 7z extraction, the library briefly holds both the compressed source and the decompressed output simultaneously. This is a structural constraint of the current 7z crate. Phase 2 (post-extraction) is already mitigated by releasing the compressed buffer. Phase 1 (during extraction) would require either:

- An mmap-backed input source to avoid loading the entire compressed archive into RAM
- A streaming decompressor with bounded output buffering

## External Feature Requests

These items were surfaced during seven rounds of Gemini external reviews during the pre-v0.3.0 development cycle. Priority is assigned by how many independent reviews raised the same request.

### High Priority (3+ reviews)

| ID | Feature | Reviews | Scope |
|----|---------|---------|-------|
| F-3 | VMD motion + camera preview | 5 | Load `.vmd`, sync camera motion with model playback, verify clipping and expressions, background `.x` stage support for full-cut preview |
| F-1 | Batch conversion (CLI + GUI queue) | 3 | Multi-file batch PMX conversion, directory input with profile presets |
| F-4 | IBL / HDR environment / multi-light | 3 | HDR environment map → IBL ambient, multiple point lights, tone mapping |

### Medium Priority (2 reviews)

| ID | Feature | Scope |
|----|---------|-------|
| F-2 | ~~Simple material editor with presets~~ | **Delivered in v0.5.0** — full material editor drawer with 25+ parameter sliders, 3 built-in presets, MME output |
| F-5 | Bone mapping editor | GUI editing and saving of bone correspondence tables |
| F-7 | Physics simulation strengthening + tuning GUI | Real-time cloth / spring bone with rigid-body / joint parameter sliders |
| F-12 | Sequential capture / transparent PNG | PNG sequence with alpha or ProRes output for compositing in After Effects / Photoshop |

### Low Priority (1 review)

| ID | Feature | Source | Scope |
|----|---------|--------|-------|
| F-6 | LOD extraction / mesh optimization | review_003 | Vertex reduction, texture atlas for MMD weight reduction |
| F-8 | Texture baking | review_004 | Multi-material atlas merge, bake MToon lighting result into textures |
| F-9 | Normal editor | review_004 | Normal transfer from sphere / cylinder for toon shading shadow control |
| F-10 | AI-based auto-rigging | review_005 | Skeleton inference + weight estimation for boneless OBJ / STL meshes |
| F-11 | Material batch merge | review_005 | Auto-merge materials with identical textures for draw call reduction |
| F-13 | Simple pose editor with IK | review_007 | Mouse-drag posing + screenshot capture |

## Process Notes

- Adding an item: append to the appropriate section with a scope summary. No per-item issue tracking yet
- Marking complete: move the entry to [CHANGELOG.md](CHANGELOG.md) under the release that delivered it, then delete it from this file
- Priority changes: update the External Feature Requests table as new reviews come in
