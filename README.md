# popone

[English](README.en.md)

VRM / FBX / PMX / PMD / UnityPackage を 3D 表示します。

## ダウンロード

最新リリース: **[popone_v0.2.2.exe](https://github.com/tinatsu-nomy/popone/releases/download/v0.2.2/popone_v0.2.2.exe)**

全リリース一覧: [Releases](https://github.com/tinatsu-nomy/popone/releases)

## 注意事項

- 出力 PMX は PmxEditor 等での後段調整を想定しています。
- 本ツールの使用により発生したいかなる問題についても、作者は一切の責任を負いません。

## ライセンス

[0BSD License](LICENSE) — 帰属表示なしで自由に利用・改変・再配布できます。

## 対応形式

| 入力 | 説明 |
|------|------|
| VRM 0.0 / 1.0 (`.vrm`) | glTF 2.0 ベースの VR アバター形式 |
| FBX バイナリ (`.fbx`) | 自前パーサーによる解析。Mixamo / Blender / Maya 等のリグを自動検出 |
| PMX 2.0 / 2.1 (`.pmx`) | MikuMikuDance モデル形式。ビューア表示 + UVマップ出力 |
| PMD (`.pmd`) | MikuMikuDance モデル形式。Shift_JIS 対応 |
| UnityPackage (`.unitypackage`) | tar.gz アーカイブから VRM / FBX + テクスチャを自動抽出 |

## クイックスタート

```bash
# ビューア起動（ダブルクリックでも可）
popone.exe

# ビューアでファイルを開く
popone.exe input.vrm
popone.exe input.fbx
```

ビューアではファイルをドラッグ＆ドロップするか「開く」ボタンで読み込む。

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
- **ボーン表示** — ◎△形状（二重円＋方向三角形）、1px 描画。IK ボーンはオレンジで識別。カメラ距離に関わらず一定サイズ
- **物理可視化** — 剛体（球体・カプセル・ボックス）を 1px ワイヤーフレームで表示。コライダー=レッド、スプリング=グリーン
- **ジョイント表示** — PMX/PMD のジョイントをイエロー立方体（回転反映・アニメ同期）で可視化。濃さ調整可能
- **法線マップ表示** — 法線ベクトルを RGB カラーに変換して表示（デバッグ・確認用）
- **法線ツール** — 法線平滑化、カスタム法線クリア、法線方向の可視化
- **MSAA** — 4x アンチエイリアシング（ON/OFF 切替可能）
- **UVマップ出力** — 材質レイヤー分けの PSD として出力（1024〜8192 解像度）。UV 境界をまたぐ三角形のラップ描画対応

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

### PMX / PMD ロード

- **PMX 2.0 / 2.1** — 全データ構造の読み込み（頂点・面・材質・ボーン・モーフ・表示枠・剛体・ジョイント）。SoftBody (2.1) は読み飛ばし
- **PMD** — Shift_JIS テキスト自動変換。IK・モーフ（base+offset 形式）対応。材質名テキストファイル（同名 `.txt`）読み込み
- **テクスチャ** — PMX/PMD の相対パスから PNG/JPEG/BMP/TGA を自動読み込み。MIME ヒントによるフォーマット判定
- **Tスタンス変換** — A スタンスモデルを T スタンスに変換（ボーン・メッシュ・モーフ・剛体・ジョイント同期）
- **VRMA アニメーション** — PMX 日本語ボーン名から VRM ヒューマノイド名への自動マッピングで VRMA アニメーション再生対応
- **UI 制限** — PMX/PMD ロード時は PMX 変換ボタン・法線平滑化・カスタム法線クリアをグレーアウト
- **コメント表示** — PMX/PMD のコメントをモデル情報パネルに表示

### v0.2.3 改善

- **表示材質のみ出力** — PMX 変換時に、表示タブで非表示にした材質を出力から除外するオプション（デフォルト OFF）。材質・メッシュ・テクスチャ・頂点モーフ・グループモーフを一貫してフィルタリング
- **ボーンマージ 2パス方式** — 同名ボーン統合の親子判定を順序非依存の候補収集＋伝播ループに変更。異なる部分木の子孫が誤統合されるバグを修正
- **pkg テクスチャ名前空間** — 複数 UnityPackage 追加時のテクスチャ名衝突を防止（`{パッケージ名}_pkg{連番}_{テクスチャ名}` 形式）。auto-matched テクスチャにも適用
- **ASCII FBX Content 処理** — Content ブロックを文字列として保持し、パーサー層の完全性を維持
- **テスト 61 件** — ボーンマージ・物理リマップ・モーフオフセット・エクスポートフィルタ等のテストを追加

### コード品質・パフォーマンス改善（v0.2.2）

- **パフォーマンス最適化** — アニメーション頂点バッファの毎フレーム alloc 除去、ボーン名探索の HashMap O(1) 化、GPU 可視化バッファの dirty flag 導入
- **テスト拡充** — 10 テスト → 51 テスト。座標変換ラウンドトリップ、ボーン名マッピング、PMX 書き込み・読み込みラウンドトリップ、VRM→PMX E2E テスト
- **Clippy 警告ゼロ** — `cargo clippy --all-targets --all-features -- -D warnings` 完全クリーン
- **UX 改善** — D&D オーバーレイ 4 パターン化、操作ヒント 2 行分割、グレーアウト UI ツールチップ追加

### FBX 対応

- バイナリ / ASCII FBX の自前パーサー（シーングラフ・座標系自動変換・PreRotation・UnitScaleFactor）
- ASCII FBX: Content ブロック（埋め込みテクスチャ）は文字列として保持し、外部ファイルフォールバックで復元
- スキンウェイト（最大 4 ボーン正規化）、ブレンドシェイプ、UV マッピング
- ヒューマノイドリグ自動検出（Mixamo / 3ds Max Biped / Maya HumanIK / VRoid / Blender）
- 零法線の自動補完、埋め込み/外部テクスチャ対応

## おまけ

### アニメーション再生

- VRMA / glTF / FBX アニメーションの D&D またはダイアログ読み込み
- ヒューマノイドリターゲティング対応（異なるモデルへの適用可能）
- ループモード 4 種（なし / 通常 / A-B リピート / ピンポン往復）
- 再生速度調整・フレーム送り・シークバー・表情キーフレーム同期

### PMX（MikuMikuDance）形式に変換

ビューア上から直接変換、または CLI で変換可能。

```bash
popone.exe input.vrm output.pmx
popone.exe input.fbx output.pmx
popone.exe input.unitypackage output.pmx
```

| 出力 | 説明 |
|------|------|
| PMX 2.0 (`.pmx`) | MikuMikuDance / PmxEditor 用。MMD 標準ボーン・IK・物理を自動挿入 |
| テクスチャ PNG | `textures/` フォルダに出力 |
| UVマップ PSD | 材質ごとにレイヤー分け（ビューアから出力） |

- VRM 0.0 / 1.0 / FBX / UnityPackage を自動判定
- MMD 標準ボーン自動挿入（全ての親・センター・グルーブ・腰・足IK・つま先IK）
- 準標準ボーン挿入（腰キャンセル・足D・足先EX・腕捩り・手捩り・肩キャンセル）
- VRM Expression → PMX モーフ変換
- VRM SpringBone → PMX 剛体・ジョイント変換（重力・回転/移動制限・コライダー衝突マスク）
- Aスタンス変換、剛体回転をボーン方向に揃えるオプション
- MToon アウトライン → PMX エッジ反映
- 表示枠の自動分類（Root / 表情 / 体(上) / 腕 / 指 / 足 / その他）
- UV 正規化（0..1 範囲に補正）

## 制限事項

- **PMX/PMD は閲覧専用** — PMX 変換（再出力）は非対応。ビューア表示と UVマップ出力のみ
- **法線マップ（ノーマルマップ）未適用** — VRM/FBX の normalTexture はシェーディングに反映されない（法線マップ表示モードで確認は可能）
- **Lat式初音ミク等** — MMD レンダリングに特化したモデルは一部サーフェイスが正しく表示されない場合がある
- **PMX 2.1 SoftBody** — 読み飛ばし（未対応）

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

61 テスト（ユニット 50 + 統合 11）。統合テストは環境変数でテストデータの配置を指定可能:

```bash
# テストデータのルートディレクトリ
export POPONE_TEST_DATA=/path/to/test-fixtures

# または個別ファイルを直接指定
export POPONE_TEST_VRM_SEED_SAN=/path/to/Seed-san.vrm
export POPONE_TEST_PMX_SEED_SAN=/path/to/Seed-san.pmx
export POPONE_TEST_PMD_MIKU_V2=/path/to/初音ミクVer2.pmd
```

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
| encoding_rs | PMD Shift_JIS テキスト変換 |
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
| [encoding_rs](https://github.com/nickel-org/encoding_rs) | MIT OR Apache-2.0 |
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
