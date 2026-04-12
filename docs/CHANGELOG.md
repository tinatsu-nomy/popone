<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [Changelog](#changelog)
  - [v0.3.0 (2026-04-11)](#v030-2026-04-11)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# Changelog

[日本語](CHANGELOG.jp.md)

## v0.5.0 (2026-04-13)

Material editor drawer with full-parameter editing for MToon + lilToon, and MME (ray-mmd) material file generator.

### New Features

- **Material Editor Drawer** — A floating `egui::Window` opens from a "編" button on each material row. Sections: Basic (diffuse / alpha mode / base color texture), Shade (shade_color / shading_toony / shading_shift + texture), Outline (edge_color / width mode / outline width texture), Rim (parametric rim / rim multiply texture), MatCap, UV animation, Emissive / Normal, Other, and an MME output preview.
- **Full MToon / lilToon parameter editing** — All 25+ colors, scalars, and aux texture slots (normal / emissive / shade / shadingShift / rim / outline / matcap / uvAnimMask) are editable. Edited values are reflected live on both the standard and MMD-compatible render paths.
- **Per-slot and per-material reset** — Each texture slot has a `×` button to clear it; each material has a "初期値に戻す" action that restores the state captured at load time.
- **Built-in material presets** — MToon 1.0 default, lilToon standard, and PMX-compat presets (3 types).
- **Edit persistence in `popone_history.json`** — Per-material edit records (color/scalar diffs + MME category override) are saved alongside the existing texture history and restored on reload.
- **MME (ray-mmd) material file generator** — The Export tab includes an "MME マテリアル (.fx) も出力" checkbox under the PMX conversion section. When checked, PMX conversion also emits `<model>_mme/material_<name>.fx` files using `CUSTOM_ENABLE`-based templates (Standard / Skin / HairAniso / Glass / Cloth / ClearCoat / Emissive). The ray-mmd root folder can be set via folder picker in the Export tab; defaults to the current directory (`.\`) when not configured. `#include` paths are resolved with `pathdiff` + `dunce` canonicalization, with a fallback when relative path computation fails. Non-PMX-capable textures (normal / emissive / matcap / rim / shading shift) are copied to `<model>_mme/textures/` with relative path references. All `.fx` and `README.txt` files are encoded in Shift-JIS with CR+LF line endings for MMD/MME compatibility. If the `#include` target (`material_common_2.0.fxsub`) does not exist at the resolved path, a warning is shown in the conversion result (files are still written).

### Behavior Changes

- **`ShaderFamily::Mtoon` is now the primary decision axis** for PMX conversion, replacing the older `is_mtoon()` (`mtoon.is_some()`) check. This lets the material editor safely surface MToon parameters on non-MToon materials without flipping the PMX export into MToon-style ambient/specular output. Users must explicitly tick an "MToon 有効化" checkbox in the drawer to promote a material.

### Tests

- 230 unit tests (up from 185 in v0.4.0). New coverage includes: `MaterialParamOverride` diff/apply round-trips, `RayMmdMaterialKind` category inference (Japanese/mixed-case/prefixed names), `generate_fx` section completeness and CR+LF encoding, `TextureSlot::is_linear` full-variant coverage.


## v0.4.0 (2026-04-11)

Added a separate-window log viewer and reworked log file persistence around the principle that no log files are written unless the user explicitly asks for them or a panic occurs.

### New Features

- **Log Viewer (Separate OS Window)** — A new top-level "ログ" toolbar button now opens an independent OS window that streams the in-memory log buffer in real time. Built on `eframe`'s `show_viewport_deferred`, the log viewer is independent of the main 3D viewport: it can be moved to a different monitor, minimized separately, and does not force the main 3D scene to re-render when new log lines arrive (~150ms polling cadence inside the deferred closure).
- **Level Filter** — Toggle Error / Warn / Info / Debug visibility independently. Lines are color-coded (Error = red, Warn = yellow, Info = white, Debug / Trace = gray, Unknown = light gray). Multi-line messages such as backtraces are kept as a single logical entry.
- **Auto Tail Following** — When enabled, the view sticks to the bottom and scrolls as new lines arrive. Manually scrolling away pauses following; scrolling back to the bottom resumes it.
- **Manual Log Export** — A "ログ保存" button writes the current in-memory log to a user-chosen path via the native file dialog. A "フォルダを開く" button opens the logs directory in the OS file explorer.
- **Persistence** — Log viewer visibility, window position/size, and level filter state are saved to `popone.toml` (`[log_viewer]` section) and restored on next launch.

### Behavior Changes

- **No automatic log file generation on normal exit.** Previously the in-memory log buffer was flushed to `popone_<ts>.log` on every clean exit. v0.4.0 removes this; the buffer stays in memory and is discarded when the process exits cleanly. Use the new "ログ保存" button if you need to keep a session's logs.
- **Panic dumps go directly to `panic_<ts>.log`.** The previous "write to `popone_<ts>.log` then copy to `panic_<ts>.log`" path produced two files per crash. Now a single `panic_<ts>.log` is written.
- **Log rotation removed.** `rotate_logs` and the related `[log] keep` setting have been removed. Files in `%LOCALAPPDATA%\popone\logs\` now only exist as a result of explicit user action (manual export) or panics, so the auto-deletion bucket is no longer appropriate. Existing `popone.toml` files with a `[log] keep = N` line continue to load (the field is silently ignored).

### Internals

- New module `popone/src/viewer/log_viewer.rs` with handwritten `[HH:MM:SS.mmm][LEVEL] message` parser, ring buffer (20,000 line cap), incremental filter index, and 17 unit tests covering parser edge cases (multi-line concat, leading fragment after byte-level drain, CRLF, level filtering, geometry round-trip, in-session reopen).
- `LogViewerModel` is held behind `Arc<Mutex<LogViewerModel>>` so the `show_viewport_deferred` closure (which requires `Fn + Send + Sync + 'static`) can capture it via `Arc::clone`.
- Window position/size are captured every frame from the child viewport so the geometry round-trips correctly across in-session close/reopen and across process restarts.

## v0.3.0 (2026-04-11)

Initial public release. Focused on documentation MECE restructuring, UX improvements, and UnityPackage-related bug fixes.
