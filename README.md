# vrm2pmx

VRM（`.vrm`）ファイルの 3D ビューアと PMX（MikuMikuDance）形式への変換ツール。
Rust 製の単一バイナリで、引数なしで起動するとビューア、引数ありで CLI 変換として動作する。
VRM 0.0 / 1.0 両対応。

## 機能一覧

### ビューア（`--features viewer`）

- **VRM 3D 表示** — egui + wgpu によるネイティブ 3D レンダリング
- **ファイル読み込み** — 「開く」ボタンまたはドラッグ＆ドロップ
- **オービットカメラ** — 左ドラッグ:回転、右ドラッグ:パン、ホイール:ズーム
- **テクスチャ付き Lambert シェーディング** — 両面描画・アルファブレンド対応
- **グリッド床** — PMX スケール準拠
- **右パネル UI**
  - モデル情報（ボーン数・頂点数・面数・材質数など）
  - VRM メタ情報（作者・ライセンスなど）
  - 表情モーフスライダ
  - 表示設定（ライト・環境光・背景明度）
  - PMX 変換ボタン（ビューア上から直接変換可能）

### PMX 変換

- **VRM 0.0 / 1.0 自動判定**（`VRMC_vrm` / `VRM` 拡張を検出）
- **メッシュ・頂点・材質・テクスチャの変換**
- **MMD 標準ボーン自動挿入**（全ての親・センター・グルーブ・腰・足IK・つま先IK など）
- **準標準ボーン挿入**（腰キャンセル・足D・足先EX・腕捩り・手捩り・肩キャンセル）
- **Tda 式骨順序**に適合したボーン配置
- **VRM Expression → PMX モーフ変換**（リップシンク・まばたき・感情・視線）
- **VRM SpringBone → PMX 剛体・ジョイント変換**（重力・回転制限・移動制限対応）
- **MToon アウトライン → PMX エッジ反映**
- **表示枠の自動分類**（Root / 表情 / 体(上) / 腕 / 指 / 足 / その他）
- **テクスチャ PNG 出力**（`textures/` フォルダに書き出し）
- **詳細ログ出力**（出力先と同じディレクトリに `.log` ファイル生成）

## ビルド

```bash
cd vrm2pmx

# CLI のみ（変換専用）
cargo build --release

# ビューア付き
cargo build --release --features viewer
```

ビルド成果物は `target/release/vrm2pmx.exe` に生成される。

## 使い方

### exe ファイルから起動

```bash
# ビューア起動（引数なしで実行）
vrm2pmx.exe

# PMX 変換
vrm2pmx.exe input.vrm output.pmx

# オプション付き
vrm2pmx.exe input.vrm output.pmx --dump
vrm2pmx.exe input.vrm output.pmx --no-physics
vrm2pmx.exe input.vrm output.pmx --log-level debug
```

ビューア機能を使うには `--features viewer` 付きでビルドした exe が必要。
VRM ファイルを exe にドラッグ＆ドロップして起動することはできない（出力パスの指定が必要なため）。

### cargo run から起動

```bash
# ビューア起動
cargo run --release --features viewer

# PMX 変換
cargo run --release -- input.vrm output.pmx

# ボーン・頂点数などの情報だけ表示（PMX 生成しない）
cargo run --release -- input.vrm output.pmx --dump

# 物理演算（剛体・ジョイント）をスキップ
cargo run --release -- input.vrm output.pmx --no-physics

# ログレベル指定（error / warn / info / debug）
cargo run --release -- input.vrm output.pmx --log-level debug
```

### ビューア

ビューア上で VRM ファイルをドラッグ＆ドロップするか「開く」ボタンで読み込む。
右パネルの「PMX 変換」ボタンでビューア上から直接 PMX に変換できる。

### 出力

テクスチャは出力 PMX と同じディレクトリの `textures/` フォルダに PNG として出力される。
変換時はログファイル（`.log`）も同ディレクトリに生成される（`--dump` 時は生成しない）。

## 変換例

Seed-san.vrm（VRM 1.0）での変換結果:

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

```
VRM (.vrm/.glb)
  │
  ├─── [引数あり] CLI 変換 ──────────────────────────────────┐
  │                                                          │
  ▼                                                          ▼
┌──────────┐    ┌──────────────┐    ┌───────────┐
│ vrm/     │ ─→ │ intermediate │ ─→ │ pmx/      │ ─→ PMX (.pmx)
│ loader   │    │ IrModel      │    │ build     │
│ extract  │    │              │    │ writer    │
│ detect   │    │              │    │           │
└──────────┘    └──────────────┘    └───────────┘
      │                ↑                  ↑
      │         ┌──────────────┐          │
      │         │ convert/     │          │
      │         │ coord        │ 座標変換  │
      │         │ bone_map     │ ボーン名  │
      │         │ material     │ 材質変換  │
      │         │ morph        │ モーフ名  │
      │         │ physics      │ 物理変換  │
      │         │ texture      │ テクスチャ │
      │         └──────────────┘          │
      │                                   │
      ├─── [引数なし] ビューア ────────────┘
      │                                (PMX 変換ボタン)
      ▼
┌──────────────────┐
│ viewer/          │  ← feature = "viewer"
│ app.rs           │  eframe::App メイン状態管理
│ gpu.rs           │  wgpu パイプライン・オフスクリーン描画
│ mesh.rs          │  IrModel → GPU 頂点バッファ変換
│ texture.rs       │  テクスチャ GPU アップロード
│ camera.rs        │  オービットカメラ
│ grid.rs          │  グリッド床
│ ui.rs            │  情報パネル・モーフスライダ・変換ボタン
└──────────────────┘
```

### ソースファイル構成

```
src/
├── main.rs              エントリポイント（引数なし→ビューア / 引数あり→CLI変換）
├── lib.rs               ライブラリ API（convert_vrm_to_pmx）
├── error.rs             エラー型定義
├── vrm/
│   ├── loader.rs        GLB 読み込み・拡張データ抽出
│   ├── detect.rs        VRM バージョン自動判定
│   ├── extract.rs       VRM → 中間表現（IrModel）抽出
│   ├── types_v0.rs      VRM 0.0 serde 型定義
│   └── types_v1.rs      VRM 1.0 serde 型定義
├── intermediate/
│   └── types.rs         中間表現（IrModel / IrBone / IrMesh 等）
├── pmx/
│   ├── types.rs         PMX データ型定義
│   ├── build.rs         中間表現 → PMX モデル構築・標準ボーン挿入
│   └── writer.rs        PMX バイナリ書き出し（UTF-16 LE）
├── convert/
│   ├── coord.rs         座標変換（glTF → PMX）
│   ├── bone_map.rs      VRM ヒューマノイドボーン → PMX 日本語名マップ
│   ├── material.rs      材質変換
│   ├── morph.rs         Expression → モーフ名マップ
│   ├── physics.rs       SpringBone → 剛体・ジョイント変換（V0/V1）
│   └── texture.rs       テクスチャ PNG 書き出し
└── viewer/              ← feature = "viewer" 時のみコンパイル
    ├── app.rs           eframe::App メイン状態管理
    ├── gpu.rs           wgpu パイプライン・オフスクリーン描画
    ├── mesh.rs          IrModel → GPU 頂点バッファ変換
    ├── texture.rs       テクスチャ GPU アップロード
    ├── camera.rs        オービットカメラ
    ├── grid.rs          グリッド床
    └── ui.rs            情報パネル・モーフスライダ・変換ボタン
```

## 座標変換

glTF 右手系から PMX 左手系への変換。スケール係数: `PMX_SCALE = 12.5`（1m = 12.5 PMX 単位）。

| | VRM 0.0 | VRM 1.0 |
|--|---------|---------|
| glTF 向き | +Z（ルートに Y=180° 回転あり） | -Z |
| 位置変換 | `(-x, y, z) × scale` | `(x, y, -z) × scale` |
| 法線変換 | `(-x, y, z)` | `(x, y, -z)` |
| 面巻き順 | b↔c swap（行列式 -1） | b↔c swap（行列式 -1） |

## MMD 標準ボーン挿入

`insert_standard_bones()` により、VMD モーション再生に必要な以下のボーンを自動挿入する。

### 基本ボーン

| 日本語名 | 英語名 | 説明 |
|---------|--------|------|
| 全ての親 | master | ルートボーン |
| センター | center | 体幹移動 |
| グルーブ | groove | 上下移動 |
| 腰 | waist | 上半身・下半身の分岐点 |

### IK ボーン

| 日本語名 | 説明 |
|---------|------|
| 左足ＩＫ親 / 右足ＩＫ親 | 足IK の移動親 |
| 左足ＩＫ / 右足ＩＫ | 足首 IK（リンク: ひざ→足） |
| 左つま先ＩＫ / 右つま先ＩＫ | つま先 IK（リンク: 足首） |

### 準標準ボーン

| 日本語名 | 説明 |
|---------|------|
| 腰キャンセル左 / 右 | 腰回転の打消し |
| 左足D / 右足D 他 | 足の付与ボーン（足・ひざ・足首）×左右 |
| 左足先EX / 右足先EX | つま先の付与ボーン |
| 左腕捩 / 右腕捩 | 上腕の捩りボーン |
| 左手捩 / 右手捩 | 前腕の捩りボーン |
| 左肩C / 右肩C | 肩キャンセルボーン |
| 左肩P / 右肩P | 肩親ボーン |

## ログ出力

CLI 変換時、出力先と同じディレクトリに `.log` ファイルが生成される（`--dump` 時は生成しない）。
stderr には `--log-level` で指定したレベル（デフォルト: `info`）以上のログが出力され、
ログファイルには `debug` レベルまで全件が記録される。

### ログの全体構成

変換処理は `build_pmx_model()` を中心に以下の順序でログを出力する。

```
=== PMXモデル構築開始 ===         ← INFO: モデル名・VRMバージョン
入力VRM: ボーン=N, メッシュ=N...  ← INFO: 入力統計サマリー
--- メッシュ一覧 ---              ← DEBUG: 各メッシュの頂点数・面数・材質idx
--- テクスチャ一覧 ---            ← DEBUG: ファイル名・MIME・データサイズ
--- 材質一覧 ---                  ← DEBUG: diffuse・テクスチャ・両面・MToon・エッジ
材質: N個 (MToon=N, 両面=N...)    ← INFO: 材質統計
--- 材質別面数 ---                ← DEBUG: 材質ごとの面頂点数
頂点ウェイト分布: ...             ← DEBUG: BDEF1/BDEF2/BDEF4 の頂点数分布
--- モーフ一覧 ---                ← DEBUG: 各モーフのパネル・種別・ターゲット数
--- 剛体一覧 ---                  ← DEBUG: 各剛体の形状・ボーン・グループ・物理モード
--- ジョイント一覧 ---            ← DEBUG: 各ジョイントの接続剛体・位置
=== insert_standard_bones ===     ← DEBUG: 標準ボーン挿入（step 1〜18）
=== ソート後ボーン一覧 ===        ← DEBUG: トポロジカルソート後の最終ボーン順序
--- 表示枠 ---                    ← DEBUG: 各表示枠のボーン数・モーフ数
=== PMXモデル構築完了 ===         ← INFO: 出力PMX統計サマリー
```

### insert_standard_bones ステップ詳細

標準ボーン挿入は 18 ステップで構成される。各ステップはログに `[stepN]` タグで記録される。

| Step | 処理内容 | 説明 |
|------|---------|------|
| 1 | 位置・インデックス取得 | 下半身・足首・つま先の位置を取得し、腰ボーンの Y 座標を計算 |
| 2 | 既存インデックスシフト | 先頭に挿入する 4 本分（全ての親・センター・グルーブ・腰）、既存ボーンの parent/tail/IK/grant インデックスを +4 シフト |
| 3 | 親子関係の設定 | 下半身・上半身の親を腰に設定 |
| 3.5 | 上半身 tail 設定 | 上半身の tail を上半身2 に設定（存在する場合） |
| 4 | 頂点ウェイトシフト | 全頂点の bone_index を +4 シフト |
| 5 | 剛体 bone_index シフト | 全剛体の bone_index を +4 シフト |
| 6 | 標準ボーン構築・連結 | 全ての親・センター・グルーブ・腰の 4 本を構築し、先頭に配置して既存ボーンと連結 |
| 9 | 上半身群の整列 | 上半身→上半身2→上半身3→首→頭→下半身 の順に IK 直後（idx=4）へ移動 |
| 10 | 下半身ボーン逆転 | 下半身ボーンの position と tail を入れ替え、ボーンが上→下向きになるようにする |
| 11 | 腰キャンセルボーン追加 | 腰キャンセル右/左を追加。腰の回転を ×(-1.0) で継承し、足ボーンの親となる |
| 12 | 足 D ボーン群追加 | IK リンクボーン（足・ひざ・足首）の D 補助ボーンを追加。元ボーンの回転を ×1.0 で付与継承 |
| 13 | 足先 EX 追加 | 左足先EX / 右足先EX を足首 D の子として追加（つま先がある場合のみ） |
| 14 | D ボーン親変更 | IK 影響下ボーンを親に持つ補助ボーンの親を対応する D ボーンへ変更。変形階層を再帰的に伝播 |
| 15 | 腕捩り・手捩り追加 | 左腕捩 / 右腕捩 / 左手捩 / 右手捩 を上腕〜ひじ間・ひじ〜手首間の中間位置に追加 |
| 16 | 肩キャンセルボーン追加 | 左肩P / 右肩P（肩親）と左肩C / 右肩C（肩キャンセル）を追加 |
| 17 | IK ボーン群追加 | 足IK親・足ＩＫ・つま先ＩＫ・ＩＫ先ボーンを末尾に追加（左→右順、あにまさ/ミク Ver2 準拠） |
| 18 | D ボーン群末尾整列 | D ボーン・足先 EX を IK ボーンの後（最末尾）に右→左順で整列 |

ステップ後、`fix_duplicate_names`（重複ボーン名解決）と `sort_bones_topological`（変形順序ソート）が実行され、最終的なボーン配列が確定する。

## テスト

```bash
cargo test
```

2 件の単体テスト:
- `test_coord_x_flip` — 座標変換の正しさ検証
- `test_face_winding` — 面の巻き順反転検証

## 依存クレート

### コア（CLI 変換）

| クレート | 用途 |
|---------|------|
| gltf | GLB/glTF 2.0 パーサー（`extensions` 機能有効） |
| serde / serde_json | VRM 拡張 JSON のデシリアライズ |
| glam | 3D 数学（Vec3, Quat, Mat4） |
| byteorder | PMX バイナリ書き出し（リトルエンディアン） |
| image | テクスチャの PNG エンコード |
| clap | CLI 引数パーサー |
| anyhow | エラーハンドリング |
| log / fern / chrono | ログ出力 |

### ビューア（`viewer` feature）

| クレート | 用途 |
|---------|------|
| eframe | egui ウィンドウフレームワーク（wgpu バックエンド） |
| rfd | ネイティブファイルダイアログ |
| bytemuck | 頂点/ユニフォーム構造体の Pod 変換 |
| pollster | async ブロッキング実行 |

## ライブラリ API

`vrm2pmx` はライブラリとしても使用可能:

```rust
use vrm2pmx::convert_vrm_to_pmx;
use std::path::Path;

let stats = convert_vrm_to_pmx(
    Path::new("input.vrm"),
    Path::new("output.pmx"),
    false, // no_physics
)?;

println!("ボーン: {}, 頂点: {}", stats.bones, stats.vertices);
```

## 参考資料

本ツールの実装にあたり、以下の仕様書を参照している。

| 形式 | 資料 | 備考 |
|------|------|------|
| VRM | [vrm-c/vrm-specification](https://github.com/vrm-c/vrm-specification) | VRM 0.0 / 1.0 公式仕様。glTF 2.0 拡張としてヒューマノイドボーン・Expression・SpringBone・MToon 等を定義 |
| PMX | PMX仕様書（PmxEditor 同梱） | PmxEditor に添付されている PMX 2.0 バイナリフォーマット仕様。ヘッダ・頂点・面・材質・ボーン・モーフ・表示枠・剛体・ジョイントの各データ構造を定義 |

### VRM 仕様の主要ポイント

- VRM は glTF 2.0（`.glb`）をベースに `.vrm` 拡張子を使用
- glTF の `extensions` フィールドに VRM 固有データを格納
- VRM 1.0 の主要拡張: `VRMC_vrm`（ヒューマノイド・Expression・視線・メタ情報）、`VRMC_materials_mtoon`（セルシェーディング）、`VRMC_springBone`（揺れもの物理）
- 座標系は glTF 準拠の右手系・メートル単位
- VRM 0.0 は `VRM` 拡張を使用し、ルートノードに Y=180° 回転がある点が 1.0 と異なる

### PMX 仕様の主要ポイント

- PMX 2.0 はリトルエンディアンのバイナリ形式
- 文字列エンコーディングは UTF-16 LE（encoding=0）
- インデックスサイズは可変（1/2/4 バイト、ヘッダで指定）
- ボーンは IK・付与（回転/移動）・変形階層をサポート
- 剛体・ジョイントは Bullet Physics 互換（Euler 角は ZYX 規約）
- 座標系は左手系・Y-up・+Z 前方、スケールは独自単位（本ツールでは 1m = 12.5）

## 注意事項

- 本ツールが出力する PMX ファイルは、後段で PmxEditor 等を用いて調整することを想定しています。
- 本ツールのインストールまたは使用により発生したいかなる問題についても、作者は一切の責任を負いません。
