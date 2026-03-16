# 技術詳細

popone の内部実装に関する詳細ドキュメント。

## 座標変換

glTF 右手系から PMX 左手系への変換。スケール係数: `PMX_SCALE = 12.5`（1m = 12.5 PMX 単位）。

| | VRM 0.0 | VRM 1.0 | FBX |
|--|---------|---------|-----|
| 入力座標系 | glTF（+Z 向き、ルート Y=180° 回転） | glTF（-Z 向き） | GlobalSettings に依存（Y-Up / Z-Up） |
| 位置変換 | `(-x, y, z) × scale` | `(x, y, -z) × scale` | coord_fn（GlobalSettings 基準）→ glTF 空間 |
| 法線変換 | `(-x, y, z)` | `(x, y, -z)` | 同上（逆転置行列） |
| 面巻き順 | b↔c swap（行列式 -1） | b↔c swap（行列式 -1） | b↔c swap（行列式 -1） |
| スケール | glTF メートル単位 | glTF メートル単位 | UnitScaleFactor / 100（cm → m 変換） |
| PreRotation | なし | なし | Model ノードの PreRotation を世界変換に反映 |

### PMX/PMD → IrModel 逆変換

PMX/PMD ファイルをビューアで表示するために、PMX 座標を glTF 座標に逆変換する。

| 対象 | 変換 |
|------|------|
| 位置 | `(x, y, -z) / 12.5` |
| 法線 | `(x, y, -z)` |
| モーフオフセット | `(x, y, -z) / 12.5`（変位ベクトル、スケール必要） |
| 面巻き順 | b↔c swap（逆変換で反転） |
| 剛体・ジョイント位置 | PMX 座標のまま保持（ビューアが PMX 座標で描画） |

#### PMD 固有の変換

| 対象 | 処理 |
|------|------|
| 剛体位置 | ボーン相対オフセット → `bone.position + offset` で絶対座標に変換 |
| 剛体回転 | 絶対 Euler 角。ボーン方向 Y < 0 なら X 反転 |

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

## PMX/PMD ロード（v0.2.1）

### PMX リーダー

- PMX 2.0 / 2.1 バイナリ対応
- UTF-16LE / UTF-8 テキスト自動判定（ヘッダ encoding に従う）
- 可変インデックスサイズ: 頂点（符号なし 1/2/4）、他（符号あり 1/2/4）
- SDEF → BDEF2 フォールバック、QDEF → BDEF4 扱い
- PMX 2.1: フリップモーフ → Group 扱い、インパルスモーフ → 読み飛ばし、SoftBody → 読み飛ばし

### PMD リーダー

- `encoding_rs` による Shift_JIS → UTF-8 変換
- 固定長構造パース（頂点 38byte、材質 70byte、ボーン 39byte）
- IK は別セクション → ボーン情報には統合せず `PmdIk` として保持
- モーフ: base + offset 形式 → グローバル頂点インデックスに展開
- 英語ヘッダ・トゥーンテクスチャ・剛体・ジョイントはオプション（EOF 時スキップ）
- 材質名テキストファイル: PMD と同名の `.txt`（S-JIS）があり行数が材質数と一致すれば材質名として適用

### IrModel 変換

- 頂点インデックスマッピング: メッシュ分割時に PMX/PMD グローバル頂点 → IrModel 通し番号のマッピングテーブルを構築し、モーフの頂点インデックスを変換
- ボーン名マッピング: `pmx_name_to_vrm_bone()` で PMX 日本語ボーン名 → VRM ヒューマノイド名の逆引き（VRMA アニメーション再生用）
- **重要**: `"センター"` → `"hips"` マッピング（PMX のセンターが VRM の hips に対応。下半身ではない）

### Tスタンス変換

`normalize_pose_to_tstance_full()` で A スタンス → T スタンスに変換:

1. 左右上腕を検出（`vrm_bone_name` または PMX 名 `"左腕"` / `"右腕"`）
2. 腕方向から水平までの角度を計算し、逆回転の補正クォータニオンを生成
3. ボーン位置・グローバル行列を補正
4. メッシュ頂点・法線をスキンウェイトに基づいて回転
5. モーフオフセットに回転を適用
6. 剛体・ジョイント: 影響ボーンの子孫に属するものの位置・回転を補正

### 剛体回転の調整

PMX/PMD の剛体回転は Euler 角（ZXY）で格納。ビューア表示時にボーン方向による X 反転が必要:

```rust
// ボーン方向の Y 成分 < 0 なら X 回転を反転
if bone_dir.y < 0.0 {
    rotation.x = -rotation.x;
}
```

### テクスチャ読み込み

- PMX: テクスチャパステーブルからの相対パスで読み込み
- PMD: 材質の `texture_name` から `*` でスフィアテクスチャを分離し、メインテクスチャのみ使用
- MIME ヒント: 拡張子から MIME タイプを推定し、`image::load_from_memory_with_format` で明示指定（TGA はマジックナンバーがなく自動判定が失敗するため）

## ビューア表示スタイル

### ボーン表示

- 形状: ◎△（二重円＋底辺なし三角形）
- 描画: 1px LineList（`pipeline_line_overlay`）
- 色: 通常ボーン = ブルー `#0000ff`、IK ボーン = オレンジ `#ff9600`
- サイズ: カメラ距離に応じてスケール（画面上一定サイズ）
- IK 判定: ボーン名に "ＩＫ" または "IK" を含むか

### 剛体表示

- 描画: 1px LineList
- 色: コライダー（group=1）= レッド `#ff0000`、スプリング（group!=1）= グリーン `#00ff00`
- 球体: 8 経線（大円弧）+ 7 緯線
- カプセル: 上下リング + 8 本接続線
- ボックス: 12 辺

### ジョイント表示（PMX/PMD のみ）

- 形状: 正立方体（面=イエロー `#ffff00`、エッジ=1px 黒線）
- サイズ: 0.18 PMX 単位
- 回転: Euler ZXY → Quat で姿勢反映
- アニメーション同期: rigid_a のボーンからのオフセットで追従
- 濃さ: スライダーで調整可能

### 法線マップ表示

- シェーダー内で法線ベクトル → RGB 変換: `rgb = (normalize(normal) + 1.0) * 0.5`
- CameraUniform の `show_normal_map` フラグで切替

### 描画順

後に描画されるものが最前面:

1. ジョイント（最背面）
2. ボーン
3. 剛体（最前面）

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

## アニメーション再生

ビューアは VRMA / glTF / FBX アニメーションのリアルタイム再生をサポートする。

### 対応形式

| 形式 | 読み込み | リターゲティング | 備考 |
|------|---------|----------------|------|
| VRMA (`.vrma`) | `vrm::animation::load_vrma` | ヒューマノイド正規化座標系 | VRM Animation 仕様準拠。bone_rests でモデル間変換 |
| glTF / GLB | `vrm::animation::load_gltf_animation` | ヒューマノイドノード名照合 | 複数アニメーション対応 |
| FBX (`.fbx`) | `fbx::animation::load_fbx_animation` | PreRotation 合成・座標変換 | AnimationStack → Layer → CurveNode → Curve 階層解析 |
| Unity .anim | `unity::animation::load_unity_anim` | Muscle → SwingTwist 変換 | 隠し機能（D&D のみ対応） |

### PMX/PMD でのアニメーション再生

PMX/PMD モデルに VRMA アニメーションを適用する際、`pmx_name_to_vrm_bone()` によるボーン名マッピングが使用される。主なマッピング:

| PMX ボーン名 | VRM ヒューマノイド名 |
|-------------|---------------------|
| センター | hips |
| 上半身 | spine |
| 上半身2 | chest |
| 首 | neck |
| 頭 | head |
| 左腕 / 右腕 | leftUpperArm / rightUpperArm |
| 左ひじ / 右ひじ | leftLowerArm / rightLowerArm |
| 左足 / 右足 | leftUpperLeg / rightUpperLeg |
| （他、指・肩・目など 55 ボーン対応） | |

### ヒューマノイドリターゲティング

VRMA および glTF ヒューマノイドアニメーションは、ソースモデルとターゲットモデルのレストポーズが異なっても正しく適用されるよう、以下の公式でリターゲティングする:

```
normalized = W_src × L_src⁻¹ × anim_rot × W_src⁻¹
local_rot  = L_dst × W_dst⁻¹ × normalized × W_dst
```

- `W_src`, `L_src`: ソース（VRMA）のグローバル/ローカルレストポーズ回転
- `W_dst`, `L_dst`: ターゲット（VRM モデル）のグローバル/ローカルレストポーズ回転
- `anim_rot`: アニメーションで指定されたローカル回転値

### FBX アニメーション座標変換

FBX アニメーションは以下の手順で glTF 座標系に変換する:

1. **GlobalSettings**: 軸変換行列を構築（Y-Up の場合は恒等変換）
2. **Euler 回転**: ZYX 外的（= XYZ 内的）、`Quat::from_euler(EulerRot::ZYX, rz, ry, rx)`
3. **PreRotation 合成**: `PreRotation × euler_to_quat(Lcl Rotation)` をキーフレームに適用
4. **向き検出**: Left 系ボーンのグローバル X 座標が正 → +Z 向き → Y180 補正必要
5. **Y180 補正**: 回転 `Quat(-x, y, -z, w)`、平行移動デルタ `Vec3(-dx, dy, -dz)`
6. **時間単位**: FBX 1 秒 = 46186158000

### Unity .anim Muscle 変換（隠し機能）

Unity Humanoid の Muscle 値からボーン回転への変換。安定性が限定的なため隠し機能として実装。

#### SwingTwist 分解

Muscle の 3 DOF（twist, swing_y, swing_z）から回転を構築する:

```
SwingTwist(x, y, z) = AngleAxis(|yz|, normalize(0, y, z)) × AngleAxis(x, (1,0,0))
```

- Twist: X 軸周りの回転
- Swing: YZ 平面での振り

#### ボーン回転の計算式

```
localRotation = preQ × SwingTwist(sign × degrees) × postQ⁻¹
```

- `preQ`, `postQ`: アバター固有の基準回転（正規化スケルトンでは preQ == postQ）
- `sign`: ボーンごとの符号 `(±1, ±1, ±1)`（V-Sekai `GetLimitSign` 準拠）
- `degrees`: Muscle 値を角度範囲でスケーリングした度数

#### Muscle 値 → 角度

```
muscle ≥ 0: degrees = muscle × max_deg
muscle < 0: degrees = muscle × (-min_deg)
```

`min_deg`, `max_deg` は `HumanTrait.GetMuscleDefaultMin/Max` のデフォルト値を使用。

#### 左手系 → 右手系変換

- クォータニオン: `(x, -y, -z, w)`（reverseX 規約、UniVRM 準拠）
- ベクトル: `(-x, y, z)`

#### RootQ / RootT

- RootQ: 初期フレームからのデルタ `delta = q0⁻¹ × qi`、適用は `rest × delta`
- RootT: 初期フレームからのデルタ（相対移動）、適用は `rest_pos + delta`

#### パラメータモード

DumpHumanoidParams.cs で出力した JSON を指定すると、モデル固有の preQ / postQ / sign を使用して高精度な変換を行う。未指定の場合は V-Sekai 正規化スケルトンのフォールバック値を使用する。

### ループモード

| モード | 説明 |
|--------|------|
| なし (None) | 一度再生して停止 |
| 通常 (Normal) | 終端で先頭に戻って繰り返し |
| A-B リピート | ユーザー指定区間を繰り返し |
| ピンポン (PingPong) | 往復再生 |

## ソースファイル構成

```
src/
├── main.rs              エントリポイント（引数なし or 出力未指定→ビューア / 出力指定→CLI変換）
├── lib.rs               ライブラリ API
├── error.rs             エラー型定義
├── unitypackage.rs      .unitypackage (tar.gz) アセット展開（VRM / FBX 検出・抽出）
├── vrm/
│   ├── loader.rs        GLB 読み込み・拡張データ抽出（ファイル / バイト列両対応）
│   ├── detect.rs        VRM バージョン自動判定
│   ├── extract.rs       VRM → 中間表現（IrModel）抽出
│   ├── animation.rs     VRMA / glTF アニメーション読み込み
│   ├── types_v0.rs      VRM 0.0 serde 型定義
│   └── types_v1.rs      VRM 1.0 serde 型定義
├── fbx/
│   ├── parser.rs        FBX バイナリパーサー
│   ├── scene.rs         シーングラフ構築（Objects / Connections 解析）
│   ├── extract.rs       FBX → 中間表現（IrModel）抽出
│   ├── bone.rs          ボーン階層構築（PreRotation 対応）
│   ├── mesh.rs          メッシュ・UV・材質プロパティ抽出
│   ├── skin.rs          スキンウェイト抽出
│   ├── texture.rs       テクスチャ抽出（埋め込み / 外部ファイル）
│   ├── blendshape.rs    ブレンドシェイプ抽出
│   ├── animation.rs     FBX アニメーション抽出（Stack/Layer/CurveNode/Curve 階層、バイト列対応）
│   └── humanoid.rs      ヒューマノイドリグ自動検出・マッピング
├── pmx/
│   ├── types.rs         PMX データ型定義
│   ├── reader.rs        PMX 2.0/2.1 バイナリ読み込み（UTF-16LE/UTF-8、SoftBody 読み飛ばし）
│   ├── extract.rs       PMX → 中間表現（IrModel）抽出（glTF 逆変換）
│   ├── build.rs         中間表現 → PMX モデル構築・標準ボーン挿入
│   └── writer.rs        PMX バイナリ書き出し（UTF-16 LE）
├── pmd/
│   ├── types.rs         PMD データ型定義
│   ├── reader.rs        PMD バイナリ読み込み（Shift_JIS、encoding_rs）
│   └── extract.rs       PMD → 中間表現（IrModel）抽出（材質名テキスト読み込み対応）
├── unity/
│   └── animation.rs     Unity .anim Muscle 変換（SwingTwist 分解）
├── intermediate/
│   ├── types.rs         中間表現（IrModel / IrBone / IrMesh 等、SourceFormat: Vrm0/Vrm1/Fbx/Pmx/Pmd）
│   ├── animation.rs     アニメーション中間表現（VrmaAnimation / BoneChannel）
│   └── pose.rs          スタンス変換（T→A / A→T、物理同期対応）
├── convert/
│   ├── coord.rs         座標変換（glTF → PMX / PMX → glTF）
│   ├── bone_map.rs      VRM ヒューマノイドボーン ↔ PMX 日本語名マップ（双方向）
│   ├── material.rs      材質変換
│   ├── morph.rs         Expression → モーフ名マップ
│   ├── physics.rs       SpringBone → 剛体・ジョイント変換（V0/V1）
│   ├── texture.rs       テクスチャ PNG 書き出し
│   └── uvmap.rs         UVマップ PSD 出力（材質レイヤー分け、境界ラップ対応）
└── viewer/              ← feature = "viewer" 時のみコンパイル
    ├── app.rs           eframe::App 状態管理（TextureState / AnimLibrary サブ構造体）
    ├── gpu.rs           wgpu パイプライン・オフスクリーン描画・可視化バッファ dirty flag
    ├── mesh.rs          IrModel → GPU 頂点バッファ変換
    ├── texture.rs       テクスチャ GPU アップロード（MIME ヒント対応）
    ├── camera.rs        オービットカメラ
    ├── grid.rs          グリッド床
    ├── ui.rs            情報パネル・モーフスライダ・変換ボタン・PMX/PMD グレーアウト
    └── animation.rs     アニメーション再生・リターゲティング（VRMA/glTF/FBX 対応）
```

## v0.2.2 内部改善

### ViewerApp サブ構造体化

v0.2.2 で ViewerApp の 43 フィールドを 30 フィールドに削減:

| サブ構造体 | フィールド | アクセス | 内容 |
|-----------|----------|---------|------|
| `TextureState` | `self.tex.*` | 9 フィールド | テクスチャ割り当て・パッケージテクスチャ・プレビュー・マッチング |
| `AnimLibrary` | `self.anim.*` | 4 フィールド | アニメーション再生状態・ライブラリ・Muscle スケール |

Rust の部分借用により `&mut self.tex` と `&self.anim` を同時に借用可能。

### GPU 可視化バッファのキャッシュ戦略

ボーン・物理・ジョイントの可視化頂点を dirty flag で管理:

| 入力 | キャッシュキー | 再生成条件 |
|------|-------------|----------|
| ボーン頂点 | `camera.eye()`, `bone_opacity` | カメラ移動 / 不透明度変更 / アニメーション再生中 |
| SpringBone 頂点 | `spring_bone_opacity`, `align_rigid_rotation` | 設定変更 / アニメーション再生中 |
| ジョイント頂点 | `joint_opacity` | 設定変更 / アニメーション再生中 |

全バッファ共通:
- `vertex_count == 0` → 強制再生成（非表示→表示トグル復帰）
- `cache_had_anim && !has_anim` → アニメーション解除時に1フレーム強制再生成

### アニメーション頂点バッファ最適化

`apply_bone_animation()` のホットパス改善:

| 項目 | Before | After |
|------|--------|-------|
| 頂点バッファ | `base.to_vec()` 毎フレーム alloc | `reset_animated_to_base()` capacity 再利用 |
| デルタ行列 | `Vec::with_capacity()` 毎フレーム | `work_deltas` フィールドで再利用 |
| globals 計算 | `Vec` 新規生成 + clone | in-place 更新（`work_computed` フラグ再利用） |
| モーフ適用 | `apply_morphs_to_buf(&self, &mut [Vertex])` | `apply_morphs_to_animated(&mut self)` 借用衝突回避 |

### ボーン名探索 HashMap 化

`insert_standard_bones()` 内の O(n) 線形探索を HashMap O(1) に:

```rust
// ボーン名 → インデックスの逆引き（重複名は最初の出現を保持）
fn build_bone_map(bones: &[PmxBone]) -> HashMap<String, usize> {
    let mut map = HashMap::with_capacity(bones.len());
    for (i, b) in bones.iter().enumerate() {
        map.entry(b.name.clone()).or_insert(i);
    }
    map
}
```

ボーン配列の変更（挿入・移動）後に `bone_map = build_bone_map(&model.bones)` で再構築。

### テストデータパス解決

統合テストのファイルパスは環境変数で設定可能:

| 優先度 | 解決元 | 例 |
|--------|-------|-----|
| 1 | ファイル個別環境変数 | `POPONE_TEST_VRM_SEED_SAN=/path/to/Seed-san.vrm` |
| 2 | ルート環境変数 + 相対パス | `POPONE_TEST_DATA=/fixtures` → `/fixtures/vrm-spec/.../Seed-san.vrm` |
| 3 | `CARGO_MANIFEST_DIR/..` | ローカル開発時のデフォルト |

## 制限事項

- **PMX/PMD は閲覧専用** — PMX 変換（再出力）は非対応。ビューア表示と UV マップ出力のみ
- **法線マップ（ノーマルマップ）未適用** — VRM/FBX の normalTexture はシェーディングに反映されない（法線マップ表示モードで確認は可能）
- **Lat式初音ミク等** — MMD レンダリングに特化したモデルは一部サーフェイスが正しく表示されない場合がある
- **PMX 2.1 SoftBody** — 読み飛ばし（未対応）

## 参考資料

| 形式 | 資料 | 備考 |
|------|------|------|
| VRM | [vrm-c/vrm-specification](https://github.com/vrm-c/vrm-specification) | VRM 0.0 / 1.0 公式仕様。glTF 2.0 拡張としてヒューマノイドボーン・Expression・SpringBone・MToon 等を定義 |
| PMX | PMX仕様書（PmxEditor 同梱） | PmxEditor に添付されている PMX 2.0 バイナリフォーマット仕様。ヘッダ・頂点・面・材質・ボーン・モーフ・表示枠・剛体・ジョイントの各データ構造を定義 |
| PMD | MikuMikuDance 付属ドキュメント | PMD バイナリフォーマット（固定長構造、Shift_JIS テキスト） |

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
- 剛体・ジョイントは Bullet Physics 互換（Euler 角は ZXY 規約）
- 座標系は左手系・Y-up・+Z 前方、スケールは独自単位（本ツールでは 1m = 12.5）

### PMD 仕様の主要ポイント

- リトルエンディアンのバイナリ形式、マジック `"Pmd"`
- テキストは Shift_JIS 固定長（ボーン名 20byte、コメント 256byte）
- 頂点 38byte 固定（BDEF2 のみ、ウェイトは 0〜100 の整数）
- IK はボーンとは別セクションに格納
- モーフは base + offset 形式（base モーフのグローバル頂点位置 + 差分オフセット）
- 英語ヘッダ・トゥーンテクスチャ・剛体・ジョイントはファイル末尾のオプション拡張
