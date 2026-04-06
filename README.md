# popone

[日本語](README.jp.md)

A 3D viewer for VRM / FBX / PMX / PMD / OBJ / STL / DirectX .x files.

## Download

Latest release: **[popone-v0.2.31.exe](https://github.com/tinatsu-nomy/popone/releases/download/v0.2.31/popone-v0.2.31.exe)**

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
| clap | CLI argument parser |
| anyhow / thiserror | Error handling |
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
| [mikktspace](https://github.com/gltf-rs/mikktspace) | MIT OR Apache-2.0 |
| [encoding_rs](https://github.com/nickel-org/encoding_rs) | MIT OR Apache-2.0 |
| [flate2](https://github.com/rust-lang/flate2-rs) | MIT OR Apache-2.0 |
| [tar](https://github.com/alexcrichton/tar-rs) | MIT OR Apache-2.0 |
| [zip](https://github.com/zip-rs/zip2) | MIT |
| [sevenz-rust2](https://github.com/hasenbanck/sevenz-rust2) | Apache-2.0 |
| [tobj](https://github.com/tatsy/tobj) | MIT |
| [clap](https://github.com/clap-rs/clap) | MIT OR Apache-2.0 |
| [anyhow](https://github.com/dtolnay/anyhow) / [thiserror](https://github.com/dtolnay/thiserror) | MIT OR Apache-2.0 |
| [log](https://github.com/rust-lang/log) / [fern](https://github.com/daboross/fern) / [chrono](https://github.com/chronotope/chrono) | MIT (fern) / MIT OR Apache-2.0 (others) |

### Viewer

| Crate | License |
|-------|---------|
| [eframe](https://github.com/emilk/egui) | MIT OR Apache-2.0 |
| [rfd](https://github.com/PolyMeilex/rfd) | MIT |
| [bytemuck](https://github.com/Lokathor/bytemuck) | Zlib OR Apache-2.0 OR MIT |
| [pollster](https://github.com/zesterer/pollster) | MIT OR Apache-2.0 |

</details>
