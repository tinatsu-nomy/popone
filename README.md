# popone

[English](README.en.md)

VRM / FBX / UnityPackage を 3D 表示し、PMX（MikuMikuDance）形式に変換するツール。
Rust 製の単一バイナリで、引数なしでビューア、引数ありで CLI 変換として動作する。

## ダウンロード

最新リリース: **[popone_v0.1.17.exe](https://github.com/tinatsu-nomy/popone/releases/download/v0.1.17/popone_v0.1.17.exe)**

全リリース一覧: [Releases](https://github.com/tinatsu-nomy/popone/releases)

## 対応形式

| 入力 | 説明 |
|------|------|
| VRM 0.0 / 1.0 (`.vrm`) | glTF 2.0 ベースの VR アバター形式 |
| FBX バイナリ (`.fbx`) | 自前パーサーによる解析。Mixamo / Blender / Maya 等のリグを自動検出 |
| UnityPackage (`.unitypackage`) | tar.gz アーカイブから VRM / FBX + テクスチャを自動抽出 |

| 出力 | 説明 |
|------|------|
| PMX 2.0 (`.pmx`) | MikuMikuDance / PmxEditor 用。MMD 標準ボーン・IK・物理を自動挿入 |
| テクスチャ PNG | `textures/` フォルダに出力 |
| UVマップ PSD | 材質ごとにレイヤー分け（ビューアから出力） |

## クイックスタート

```bash
# ビューア起動（ダブルクリックでも可）
popone.exe

# ビューアでファイルを開く（出力未指定で自動的にビューアモード）
popone.exe input.vrm
popone.exe input.fbx

# CLI で PMX 変換（出力指定時）
popone.exe input.vrm output.pmx
popone.exe input.fbx output.pmx
popone.exe input.unitypackage output.pmx
```

ビューアではファイルをドラッグ＆ドロップするか「開く」ボタンで読み込み、右パネルの「PMX 変換」ボタンで変換できる。

## 機能一覧

### ビューア

- **3D 表示** — egui + wgpu によるリアルタイムレンダリング。テクスチャ付き Lambert シェーディング、両面描画、アルファブレンド
- **カメラ操作** — 左ドラッグ:回転、右ドラッグ:パン、ホイール:ズーム。F:フィット、R:リセット
- **表情モーフ** — スライダで Expression を調整（0/1 ボタン・直接入力対応）
- **材質表示切替** — 材質ごとの ON/OFF、検索フィルタ
- **テクスチャ割り当て** — 材質に外部テクスチャ（PNG/JPG/TGA/BMP/PSD）を D&D またはダイアログで割り当て。リアルタイムプレビュー付き。VRM 埋め込みテクスチャの差し替えにも対応（リセットボタンで復元可能）
- **同名材質連動** — 同じ名前の材質に同時にテクスチャを割り当てる ON/OFF スイッチ
- **UnityPackage 対応** — VRM / FBX モデル選択ダイアログ、テクスチャ自動割当（サムネイルプレビュー・検索フィルタ付き手動割当も可能）
- **ワイヤーフレーム** — 3 モード切替（Solid / Wire / S+W）。W キーで巡回
- **ボーン・物理可視化** — ジョイント・剛体・コライダーのワイヤーフレーム表示
- **法線ツール** — 法線平滑化、カスタム法線クリア、法線方向の可視化
- **MSAA** — 4x アンチエイリアシング（ON/OFF 切替可能）
- **UVマップ出力** — 材質レイヤー分けの PSD として出力（1024〜8192 解像度）
- **アニメーション再生** — VRMA / glTF / FBX アニメーションの D&D またはダイアログ読み込み。ヒューマノイドリターゲティング対応。再生速度・A-Bループ・ピンポン・フレーム送り・表情キーフレーム再生
- **FBX 読み込み選択** — モデル+アニメーション両方含む FBX の場合、モデル/アニメーションそれぞれの読み込みを選択可能
- **PMX 変換** — ビューア上から直接変換。オプション（Aスタンス・剛体回転揃え）も UI で切替可能

<details>
<summary>キーボードショートカット</summary>

| キー | 機能 |
|------|------|
| Ctrl+O | ファイルを開く |
| F | モデルにフィット |
| R | カメラリセット |
| 0 / 1 / 3 / 7 / 9 | ビュープリセット（正面 / 左面 / 右面 / 上面 / 背面） |
| 2 / 8 | チルト（下 / 上に15°回り込み、360°可） |
| 4 / 6 | パン（左 / 右に15°回り込み） |
| 5 | パース／正射影 切替 |
| . | モデルにフィット |
| G | グリッド表示 |
| B | ボーン表示 |
| P | 物理表示 |
| W | ワイヤーフレーム切替 |
| N | 法線表示 |
| L | ライトモード切替 |
| Space | アニメーション再生/一時停止 |
| ←/→ | フレーム送り（一時停止中） |

</details>

### アニメーション

- VRMA（`.vrma`）: VRM Animation 形式。ヒューマノイドリターゲティングにより異なるモデルに適用可能
- glTF / GLB（`.gltf` / `.glb`）: glTF 2.0 アニメーション。ヒューマノイドリターゲティング対応
- FBX（`.fbx`）: FBX アニメーション。PreRotation 合成・座標系変換・向き検出＋Y180 補正
- ループモード 4 種（なし / 通常 / A-B リピート / ピンポン往復）
- 再生速度調整・フレーム送り・シークバー・表情キーフレーム同期

### FBX 対応

- バイナリ FBX の自前パーサー（シーングラフ・座標系自動変換・PreRotation・UnitScaleFactor）
- スキンウェイト（最大 4 ボーン正規化）、ブレンドシェイプ、UV マッピング
- ヒューマノイドリグ自動検出（Mixamo / 3ds Max Biped / Maya HumanIK / VRoid / Blender）
- 零法線の自動補完、埋め込み/外部テクスチャ対応

### PMX 変換

- VRM 0.0 / 1.0 / FBX / UnityPackage を自動判定
- MMD 標準ボーン自動挿入（全ての親・センター・グルーブ・腰・足IK・つま先IK）
- 準標準ボーン挿入（腰キャンセル・足D・足先EX・腕捩り・手捩り・肩キャンセル）
- VRM Expression → PMX モーフ変換
- VRM SpringBone → PMX 剛体・ジョイント変換（重力・回転/移動制限・コライダー衝突マスク）
- Aスタンス変換、剛体回転をボーン方向に揃えるオプション
- MToon アウトライン → PMX エッジ反映
- 表示枠の自動分類（Root / 表情 / 体(上) / 腕 / 指 / 足 / その他）
- UV 正規化（0..1 範囲に補正）

## ビルド

```bash
# CLI のみ（変換専用）
cargo build --release

# ビューア付き
cargo build --release --features viewer
```

成果物: `target/release/popone.exe`

> **Windows GUI サブシステム**: `--features viewer` でビルドした exe はコンソールウィンドウを表示しない。CLI 引数付きで実行すると親コンソールに自動接続し、ビューア起動時にはコンソールを切り離す。

## CLI オプション

```bash
popone <入力> [出力.pmx] [オプション]

出力を省略すると自動的にビューアモードで起動する（viewer feature ビルド時）。

オプション:
  --dump                  ボーン・頂点数のみ出力（PMX 生成しない）
  --no-physics            物理変換をスキップ
  --normalize-pose        Aスタンス変換（Tポーズの腕を下げる）
  --align-rigid-rotation  剛体回転をボーン方向に揃える
  --log-level <LEVEL>     ログレベル（error/warn/info/debug、デフォルト: info）
```

## 出力ファイル

- **PMX** — 指定パスに出力
- **テクスチャ** — PMX と同じディレクトリの `textures/` に PNG 出力
- **ログ** — 同ディレクトリに `.log` ファイル（`--dump` 時は生成しない）

## 変換例

Seed-san.vrm（VRM 1.0）:

| 項目 | 数 |
|------|-----|
| ボーン | 175 |
| 頂点 | 34,059 |
| 材質 | 17 |
| テクスチャ | 15 |
| モーフ | 17 |
| 剛体 | 36 |
| ジョイント | 19 |

## アーキテクチャ

![アーキテクチャ](docs/architecture.svg)

詳細なソースファイル構成・座標変換・ボーン挿入ステップについては [技術詳細](docs/technical.md) を参照。

## ライブラリ API

`popone` はライブラリとしても使用可能:

```rust
use popone::{convert_vrm_to_pmx, convert_fbx_to_pmx};
use std::path::Path;

// VRM → PMX
let stats = convert_vrm_to_pmx(
    Path::new("input.vrm"),
    Path::new("output.pmx"),
    false, // no_physics
)?;

// FBX → PMX
let stats = convert_fbx_to_pmx(
    Path::new("input.fbx"),
    Path::new("output.pmx"),
)?;

println!("ボーン: {}, 頂点: {}", stats.bones, stats.vertices);
```

## テスト

```bash
cargo test
```

## 依存クレート

<details>
<summary>コア（CLI 変換）</summary>

| クレート | 用途 |
|---------|------|
| gltf | GLB/glTF 2.0 パーサー（`extensions` 機能有効） |
| serde / serde_json | VRM 拡張 JSON デシリアライズ |
| glam | 3D 数学（Vec3, Quat, Mat4） |
| byteorder | PMX バイナリ書き出し |
| image | テクスチャ PNG エンコード |
| flate2 | zlib 圧縮・展開 |
| tar | .unitypackage (tar.gz) 展開 |
| clap | CLI 引数パーサー |
| anyhow | エラーハンドリング |
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
| [flate2](https://github.com/rust-lang/flate2-rs) | MIT OR Apache-2.0 |
| [tar](https://github.com/alexcrichton/tar-rs) | MIT OR Apache-2.0 |
| [clap](https://github.com/clap-rs/clap) | MIT OR Apache-2.0 |
| [anyhow](https://github.com/dtolnay/anyhow) | MIT OR Apache-2.0 |
| [log](https://github.com/rust-lang/log) / [fern](https://github.com/daboross/fern) / [chrono](https://github.com/chronotope/chrono) | MIT (fern) / MIT OR Apache-2.0 (others) |

### ビューア依存

| クレート | ライセンス |
|---------|-----------|
| [eframe](https://github.com/emilk/egui) | MIT OR Apache-2.0 |
| [rfd](https://github.com/PolyMeilex/rfd) | MIT |
| [bytemuck](https://github.com/Lokathor/bytemuck) | Zlib OR Apache-2.0 OR MIT |
| [pollster](https://github.com/zesterer/pollster) | MIT OR Apache-2.0 |

</details>

## ライセンス

[0BSD License](LICENSE) — 帰属表示なしで自由に利用・改変・再配布できます。

## 注意事項

- 出力 PMX は PmxEditor 等での後段調整を想定しています。
- 本ツールの使用により発生したいかなる問題についても、作者は一切の責任を負いません。
