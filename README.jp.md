# popone

[English](README.md)

VRM / FBX / PMX / PMD / OBJ / STL / DirectX .x を 3D 表示します。

## ダウンロード

最新リリース: **[popone-v0.2.26.exe](https://github.com/tinatsu-nomy/popone/releases/download/v0.2.26/popone-v0.2.26.exe)**

全リリース一覧: [Releases](https://github.com/tinatsu-nomy/popone/releases)

## 注意事項

- 本ツールの使用により発生したいかなる問題についても、作者は一切の責任を負いません。

## ライセンス

[0BSD License](LICENSE) — 帰属表示なしで自由に利用・改変・再配布できます。

## 使い方

詳細は [使い方](docs/usage.jp.md) を参照。

## 依存クレート

<details>
<summary>コア（CLI 変換）</summary>

| クレート | 用途 |
|---------|------|
| gltf | GLB/glTF 2.0 パーサー（`extensions` 機能有効） |
| serde / serde_json | VRM 拡張 JSON デシリアライズ |
| glam | 3D 数学（Vec3, Quat, Mat4） |
| byteorder | PMX バイナリ読み書き |
| image | テクスチャ PNG/JPEG/BMP/TGA デコード・エンコード |
| mikktspace | MikkTSpace 接線ベクトル生成（法線マップ用） |
| encoding_rs | PMD Shift_JIS テキスト変換 |
| flate2 | zlib 圧縮・展開 |
| tar | .unitypackage (tar.gz) 展開 |
| zip | ZIP アーカイブ展開 |
| sevenz-rust2 | 7z アーカイブ展開 |
| tobj | OBJ/MTL パーサー |
| clap | CLI 引数パーサー |
| anyhow / thiserror | エラーハンドリング |
| log / fern / chrono | ログ出力 |

</details>

<details>
<summary>ビューア（viewer feature）</summary>

| クレート | 用途 |
|---------|------|
| eframe | egui + wgpu ウィンドウフレームワーク |
| rfd | ネイティブファイルダイアログ |
| bytemuck | 頂点/ユニフォーム Pod 変換 |
| pollster | async ブロッキング実行 |

</details>

<details>
<summary>依存ライブラリのライセンス</summary>

### コア依存

| クレート | ライセンス |
|---------|-----------|
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

### ビューア依存

| クレート | ライセンス |
|---------|-----------|
| [eframe](https://github.com/emilk/egui) | MIT OR Apache-2.0 |
| [rfd](https://github.com/PolyMeilex/rfd) | MIT |
| [bytemuck](https://github.com/Lokathor/bytemuck) | Zlib OR Apache-2.0 OR MIT |
| [pollster](https://github.com/zesterer/pollster) | MIT OR Apache-2.0 |

</details>
