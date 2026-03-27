<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [使い方](#%E4%BD%BF%E3%81%84%E6%96%B9)
  - [対応形式](#%E5%AF%BE%E5%BF%9C%E5%BD%A2%E5%BC%8F)
  - [クイックスタート](#%E3%82%AF%E3%82%A4%E3%83%83%E3%82%AF%E3%82%B9%E3%82%BF%E3%83%BC%E3%83%88)
  - [機能一覧](#%E6%A9%9F%E8%83%BD%E4%B8%80%E8%A6%A7)
    - [ビューア](#%E3%83%93%E3%83%A5%E3%83%BC%E3%82%A2)
    - [PMX / PMD ロード](#pmx--pmd-%E3%83%AD%E3%83%BC%E3%83%89)
    - [更新履歴](#%E6%9B%B4%E6%96%B0%E5%B1%A5%E6%AD%B4)
  - [おまけ](#%E3%81%8A%E3%81%BE%E3%81%91)
    - [アニメーション再生](#%E3%82%A2%E3%83%8B%E3%83%A1%E3%83%BC%E3%82%B7%E3%83%A7%E3%83%B3%E5%86%8D%E7%94%9F)
    - [PMX（MikuMikuDance）形式に変換](#pmxmikumikudance%E5%BD%A2%E5%BC%8F%E3%81%AB%E5%A4%89%E6%8F%9B)
  - [制限事項](#%E5%88%B6%E9%99%90%E4%BA%8B%E9%A0%85)
  - [ビルド](#%E3%83%93%E3%83%AB%E3%83%89)
  - [CLI オプション](#cli-%E3%82%AA%E3%83%97%E3%82%B7%E3%83%A7%E3%83%B3)
  - [出力ファイル](#%E5%87%BA%E5%8A%9B%E3%83%95%E3%82%A1%E3%82%A4%E3%83%AB)
  - [変換例](#%E5%A4%89%E6%8F%9B%E4%BE%8B)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# 使い方

[English](usage.en.md)

## 対応形式

| 入力 | 説明 |
|------|------|
| VRM 0.0 / 1.0 (`.vrm`) | glTF 2.0 ベースの VR アバター形式 |
| FBX バイナリ (`.fbx`) | 自前パーサーによる解析。Mixamo / Blender / VRoid / Unreal 等のリグを自動検出。名前空間プレフィックス（`Model::` 等）対応 |
| PMX 2.0 / 2.1 (`.pmx`) | MikuMikuDance モデル形式。ビューア表示 + UVマップ出力 |
| PMD (`.pmd`) | MikuMikuDance モデル形式。Shift_JIS 対応 |
| UnityPackage (`.unitypackage`) | tar.gz アーカイブから VRM / FBX + テクスチャを自動抽出 |
| ZIP (`.zip`) | アーカイブ内の VRM / FBX / PMX / PMD / UnityPackage を自動検出・展開 |
| 7z (`.7z`) | アーカイブ内の VRM / FBX / PMX / PMD / UnityPackage を自動検出・展開 |

## クイックスタート

```bash
# ビューア起動（ダブルクリックでも可）
popone.exe

# ビューアでファイルを開く
popone.exe input.vrm
popone.exe input.fbx
```

ビューアではファイルをドラッグ＆ドロップするか「開く」ボタンで読み込む。
既にビューアが起動している場合、2回目以降の起動はファイルパスを既存ウィンドウに渡して自動的に終了する（シングルインスタンス）。

## 機能一覧

### ビューア

- **3D 表示** — egui + wgpu によるリアルタイムレンダリング。テクスチャ付き Lambert シェーディング、両面描画、アルファブレンド。VRM の MToon 材質は 2 色トゥーンシェーディング（lit/shade smoothstep 補間）+ アウトライン描画（inverted hull 法）+ リムライティング（パラメトリックリム + MatCap テクスチャ）+ 補助テクスチャ（shadeMultiply / shadingShift / rimMultiply、texCoord / KHR_texture_transform 対応）+ UV アニメーション（スクロール・回転）+ emissive（発光）+ 法線マップ（MikkTSpace 接線生成による TBN 構築、doubleSided 背面法線反転対応）+ MToon 仕様準拠 4 段階描画順制御（OPAQUE → MASK → BlendZWrite → Blend、`transparentWithZWrite` / `renderQueueOffsetNumber` + BLEND 内カメラ距離動的ソート対応）で表示。VRM 0.x MToon の全プロパティを VRM 1.0 に正規化（UniVRM マイグレーション準拠）。ベースカラーテクスチャを含む全テクスチャで `texCoord` / `KHR_texture_transform` に対応。glTF sampler のアドレスモード（Repeat / ClampToEdge / MirroredRepeat）・フィルタモード（minFilter の mipmap 選択方式を含む 6 値保持）をテクスチャごとに個別のサンプラーで反映。UTS2（Unity-Chan Toon Shader）材質は自動検出し MToon 近似表示（1st shade / outline / rim / MatCap / emissive / normal 対応、HighColor は PMX 出力のみ）。PMX/PMD は MMD レンダリングモード（NdotL 依存トゥーンシェーディング・エッジ・スフィアマップ）で表示。ライティングはライトカラー + 半球 ambient（Sky/Ground 2色補間）で VRoidHub に近い環境光を再現
- **カメラ操作** — 左ドラッグ:回転、右ドラッグ:パン、ホイール:ズーム。F:フィット、R:リセット、ダブルクリック:フィット、Shift:精密操作（1/3速度）。FOV 30°（MMD準拠）
- **表情モーフ** — スライダで Expression を調整（0/1 ボタン・直接入力対応）
- **材質表示切替** — 材質ごとの ON/OFF、検索フィルタ
- **テクスチャ割り当て** — 材質に外部テクスチャ（PNG/JPG/TGA/BMP/PSD）を D&D またはダイアログで割り当て。リアルタイムプレビュー付き。VRM 埋め込みテクスチャの差し替えにも対応（リセットボタンで復元可能）
- **同名材質連動** — 同じ名前の材質に同時にテクスチャを割り当てる ON/OFF スイッチ
- **UnityPackage 対応** — VRM / FBX モデル選択ダイアログ、テクスチャ自動割当（サムネイルプレビュー・検索フィルタ付き手動割当も可能）
- **ワイヤーフレーム** — 3 モード切替（Solid / Wire / S+W）。W キーで巡回
- **ボーン表示** — フラグ別の形状描画。通常=◎（二重円＋中心塗り）、移動=◻（正方形＋中心塗り）、軸制限=⊗（円＋✕）、IKコントローラ=◻（青枠＋オレンジ塗り＋青中心）。IK影響下ボーン（Link）はオレンジ。テイルベース描画で PMXEditor 準拠の方向表示。カメラ距離に関わらず一定サイズ
- **物理可視化** — 剛体（球体・カプセル・ボックス）を 1px ワイヤーフレームで表示。PMX/PMD は physics_mode 色分け（ボーン追従=グリーン、物理演算=レッド、物理+ボーン=ブルー）、VRM は group 色分け（コライダー=レッド、スプリング=グリーン）。カプセルは半球ワイヤーフレーム付き（PMX/PMD）
- **ジョイント表示** — PMX/PMD のジョイントをイエロー立方体（回転反映・アニメ同期）で可視化。濃さ調整可能
- **法線マップ表示** — 法線ベクトルを RGB カラーに変換して表示（デバッグ・確認用）
- **法線ツール** — 法線平滑化・カスタム法線クリア（いずれも法線マップ付き材質がある場合は自動無効化）、法線方向の可視化
- **MSAA** — 4x アンチエイリアシング（ON/OFF 切替可能）。MASK（cutout）材質ではサーフェスとアウトラインの両パスで alpha_to_coverage を有効化し、まつ毛・髪カード等のジャギーを軽減
- **UVマップ出力** — 材質レイヤー分けの PSD として出力（1024〜8192 解像度）。UV 境界をまたぐ三角形のラップ描画対応。複数モデルマージ時はモデル別にレイヤーグループフォルダに格納

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
- **テクスチャ** — PMX/PMD の相対パスから PNG/JPEG/BMP/TGA を自動読み込み。MIME ヒントによるフォーマット判定。スフィアマップ（.sph/.spa）対応
- **MMD レンダリング** — トゥーンシェーディング（共有 toon01-10 + 個別トゥーン）、Blinn-Phong スペキュラ、スフィアマップ（乗算/加算）、エッジ描画（inverted hull 法、ON/OFF・太さ調整可）
- **Tスタンス変換** — A スタンスモデルを T スタンスに変換（ボーン・メッシュ・モーフ・剛体・ジョイント同期）
- **VRMA アニメーション** — PMX 日本語ボーン名から VRM ヒューマノイド名への自動マッピングで VRMA アニメーション再生対応。回転付与・移動付与（grant）にも対応し、D-bones（足D 等）経由で足が正しく追従する
- **UI 制限** — PMX/PMD ロード時は PMX 変換ボタン・法線平滑化・カスタム法線クリアをグレーアウト
- **コメント表示** — PMX/PMD のコメントをモデル情報パネルに表示

### 更新履歴

バージョンごとの変更点は [更新履歴](CHANGELOG.md) を参照。

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
popone.exe archive.zip output.pmx
popone.exe archive.7z output.pmx --model-name "model.pmx"
```

| 出力 | 説明 |
|------|------|
| PMX 2.0 (`.pmx`) | MikuMikuDance / PmxEditor 用。MMD 標準ボーン・IK・物理を自動挿入 |
| テクスチャ PNG | `textures/` フォルダに出力 |
| UVマップ PSD | 材質ごとにレイヤー分け、モデル別グループフォルダ付き（ビューアから出力） |

- VRM 0.0 / 1.0 / FBX / UnityPackage / ZIP / 7z を自動判定
- MMD 標準ボーン自動挿入（全ての親・センター・グルーブ・腰・足IK・つま先IK）
- 準標準ボーン挿入（腰キャンセル・足D・足先EX・腕捩り・手捩り・肩キャンセル）
- VRM Expression → PMX モーフ変換
- VRM SpringBone → PMX 剛体・ジョイント変換（重力・回転/移動制限・コライダー衝突マスク）
- Aスタンス変換 / Tスタンス変換（FBX用、変換失敗・スキップ時はビューポートに常時警告表示）、剛体回転をボーン方向に揃えるオプション
- 物理なしで出力（剛体・ジョイント省略）、元のボーン構造で出力（標準ボーン挿入スキップ＋元のボーン名維持）
- MToon アウトライン → PMX エッジ反映
- 表示枠の自動分類（Root / 表情 / 体(上) / 腕 / 指 / 足 / その他）
- UV 正規化（0..1 範囲に補正）

## 制限事項

- **PMX/PMD は閲覧専用** — PMX 変換（再出力）は非対応。ビューア表示と UVマップ出力のみ
- **スフィアモード 3（サブテクスチャ）未対応** — 追加 UV が必要なため未実装。検出時は警告ログを出力し無効化
- **テクスチャサイズ制限** — GPU の `max_texture_dimension_2d`（一般的に 8192px）を超えるテクスチャは自動的に縮小される。画質が若干低下する場合がある。PMX 変換出力には影響しない（ビューア表示のみ）
- **展開サイズ上限** — アーカイブ（ZIP / 7z）および `.unitypackage` の展開サイズは合計 2GB が上限。これを超えるファイルはエラーとなる
- **MMD 特化モデル** — MMD レンダリングに特化したモデルは一部サーフェイスが正しく表示されない場合がある
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
  --normalize-to-tstance  Tスタンス変換（Aスタンスの腕を水平にする、FBX用）
  --align-rigid-rotation  剛体回転をボーン方向に揃える
  --raw-structure         元のボーン構造で出力（標準ボーン挿入スキップ＋元のボーン名維持）
  --model-name <NAME>     アーカイブ内のモデルファイル名を指定（ZIP/7z用）
  --list-models           アーカイブ内のモデル一覧を表示して終了（ZIP/7z用）
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

