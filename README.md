# popone

[日本語](README.jp.md)

A 3D viewer for VRM / FBX / PMX / PMD / OBJ / STL / DirectX .x files.

## Download

Latest release: **[popone-v0.2.38.exe](https://github.com/tinatsu-nomy/popone/releases/download/v0.2.38/popone-v0.2.38.exe)**

All releases: [Releases](https://github.com/tinatsu-nomy/popone/releases)

## Disclaimer

- The author assumes no responsibility for any issues arising from the use of this tool.
- The user interface is in Japanese.

## License

[0BSD License](LICENSE) — Free to use, modify, and redistribute without attribution.

## Usage

See [Usage](docs/usage.md) for details.

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
| mikktspace | MikkTSpace tangent vector generation (for normal maps) |
| encoding_rs | PMD Shift_JIS text conversion |
| flate2 | zlib compression/decompression |
| tar | .unitypackage (tar.gz) extraction |
| zip | ZIP archive extraction |
| sevenz-rust2 | 7z archive extraction |
| tobj | OBJ/MTL parser |
| toml | TOML config parser |
| dunce | UNC path simplification |
| tempfile | Temporary file creation |
| clap | CLI argument parser |
| anyhow / thiserror | Error handling |
| log / fern / chrono / env_logger | Logging |

</details>

<details>
<summary>Viewer (viewer feature)</summary>

| Crate | Purpose |
|-------|---------|
| eframe | egui + wgpu window framework |
| rfd | Native file dialog |
| bytemuck | Vertex/uniform Pod conversion |
| encase | Uniform buffer serialization (glam integration) |
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
| [mikktspace](https://github.com/gltf-rs/mikktspace) | MIT OR Apache-2.0 |
| [encoding_rs](https://github.com/hsivonen/encoding_rs) | MIT OR Apache-2.0 |
| [flate2](https://github.com/rust-lang/flate2-rs) | MIT OR Apache-2.0 |
| [tar](https://github.com/alexcrichton/tar-rs) | MIT OR Apache-2.0 |
| [zip](https://github.com/zip-rs/zip2) | MIT |
| [sevenz-rust2](https://github.com/hasenbanck/sevenz-rust2) | Apache-2.0 |
| [tobj](https://github.com/tatsy/tobj) | MIT |
| [clap](https://github.com/clap-rs/clap) | MIT OR Apache-2.0 |
| [anyhow](https://github.com/dtolnay/anyhow) / [thiserror](https://github.com/dtolnay/thiserror) | MIT OR Apache-2.0 |
| [log](https://github.com/rust-lang/log) / [fern](https://github.com/daboross/fern) / [chrono](https://github.com/chronotope/chrono) | MIT (fern) / MIT OR Apache-2.0 (others) |
| [toml](https://github.com/toml-rs/toml) | MIT OR Apache-2.0 |
| [dunce](https://gitlab.com/kornelski/dunce) | CC0-1.0 OR MIT-0 OR Apache-2.0 |
| [tempfile](https://github.com/Stebalien/tempfile) | MIT OR Apache-2.0 |
| [env_logger](https://github.com/rust-cli/env_logger) | MIT OR Apache-2.0 |

### Viewer

| Crate | License |
|-------|---------|
| [eframe](https://github.com/emilk/egui) | MIT OR Apache-2.0 |
| [rfd](https://github.com/PolyMeilex/rfd) | MIT |
| [bytemuck](https://github.com/Lokathor/bytemuck) | Zlib OR Apache-2.0 OR MIT |
| [encase](https://github.com/teoxoy/encase) | MIT OR Apache-2.0 |
| [pollster](https://github.com/zesterer/pollster) | MIT OR Apache-2.0 |

</details>

## Trademarks & Acknowledgments

This software reads and converts the following file formats. All trademarks are the property of their respective owners.

- **VRM** — 3D avatar format by [VRM Consortium](https://vrm-consortium.org/)
- **FBX** — FBX is a trademark of [Autodesk, Inc.](https://www.autodesk.com/)
- **glTF / GLB** — glTF is a trademark of the [Khronos Group Inc.](https://www.khronos.org/)
- **DirectX .x** — DirectX is a trademark of [Microsoft Corporation](https://www.microsoft.com/)
- **PSD** — Photoshop and PSD are trademarks of [Adobe Inc.](https://www.adobe.com/)
- **PMX / PMD** — Formats originating from MikuMikuDance (樋口M) and PMXEditor (極北P)
- **OBJ** — Wavefront OBJ, originally by Wavefront Technologies
- **STL** — Originally developed by 3D Systems for stereolithography

### Shader References

This software detects and approximates the following shader technologies for PMX conversion:

- **MToon** — Toon shader specification by [VRM Consortium](https://vrm-consortium.org/) / [Santarh](https://github.com/Santarh/MToon) (MIT License)
- **UTS2 (Unity-Chan Toon Shader 2.0)** — by [Unity Technologies](https://unity.com/) (Unity-Chan License 2.0)
- **lilToon** — by [lilxyzw](https://github.com/lilxyzw/lilToon) (MIT License)
- **Poiyomi Toon Shader** — by [Poiyomi](https://www.poiyomi.com/)
