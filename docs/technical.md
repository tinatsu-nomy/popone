# 技術詳細

[English](technical.en.md)

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

## モデル追加読み込み（v0.2.3）

### ボーンマージ 2パス方式

`IrModel::merge()` で同名ボーンを既存側に統合する際、親子関係の整合性を順序非依存で保証する 2パス方式を採用。

#### 問題

1パス方式では `is_new_bone[parent_idx]` を構築途中の配列から参照するため、ボーン配列が親→子順でない場合にパニックまたは誤判定が発生する。また、親名の文字列一致だけで統合を判定すると、異なる部分木の子孫が既存側へ誤統合される。

例: 既存 `Root→Spine→Head`、追加 `Accessory→Spine→Head` の場合、`Spine` は親不一致で新規追加されるが、`Head` の親名はどちらも `"Spine"` なので既存 `Head` に統合されてしまう。

#### 解決策

```
パス1（候補収集）: 全ボーンを走査し、同名+同親名の統合候補を順序非依存で収集
  candidate[i] = Some(self_idx)  // 名前一致かつ親名一致

パス2（伝播ループ）: 親が候補でない子の候補を取り消し、変更がなくなるまで反復
  while changed:
    for i in 0..N:
      if candidate[i].is_some() && parent の candidate が None:
        candidate[i] = None  // 親が新規→子も新規

確定: candidate が Some のボーンを統合、None のボーンを新規追加
```

パス2 の反復は最悪 O(depth) 回で収束する（各反復で少なくとも 1 候補が取り消されるため）。

### ASCII FBX Content ブロック処理

ASCII FBX の `Video/Content` ノードは base64 等のテキスト表現で埋め込みデータを格納する。行指向パーサーでは通常の子ノード（`:` 区切り）として解析できないため、専用処理で `}` まで読み取り `FbxProperty::String` として保持する。

```
Content: {
  <base64 encoded data lines...>
}
→ FbxProperty::String(joined_lines)
```

テクスチャ抽出時（`texture.rs`）は `as_binary()` のみで取得するため、ASCII FBX の Content 文字列からは画像デコードされない。代わりに `RelativeFilename` / `FileName` による外部ファイルフォールバックで復元する。

### pkg テクスチャ名前空間

複数の UnityPackage を追加読み込みすると、パッケージ間でテクスチャ名が衝突する可能性がある（例: 両方に `body.png` が含まれる場合）。

#### 解決策

アペンド時にテクスチャ名にパッケージ固有のプレフィックスを付与:

```
{パッケージファイル名(拡張子なし)}_pkg{アペンド連番}_{元のテクスチャ名}
例: outfit_pkg1_body.png
```

- **auto-matched テクスチャ**: `embed_textures_into_ir` で `IrModel` に入ったテクスチャの `filename` にも、マージ後にプレフィックスを付与（`loaded.ir.textures[tex_count_before..]`）
- **手動割当テクスチャ**: `pkg_textures` Vec への `extend` 時にプレフィックスを付与。`pkg_assignments` HashMap はプレフィックス付き名前をキーとして自然に一意化
- **パスセパレータ回避**: プレフィックスに `/` を使わない（`IrTexture.filename` が PMX export のファイルパスに使われるため）

## アーカイブD&Dリロード対応（v0.2.4）

### ReloadableSource enum

モデルの読み込み元を追跡する enum。一時ファイルのリロード問題を解決する。

| バリアント | 説明 |
|-----------|------|
| `File(PathBuf)` | 通常のファイルパス。リロード時はファイルを再読み込み |
| `Snapshot { original_path, main_bytes: Arc<[u8]>, aux_files }` | 一時ファイルからのスナップショット。リロード時はメモリから復元 |

### 一時パス検出

`is_temp_path()` で `std::env::temp_dir()` 配下かどうかを2段階で判定:

1. **canonicalize ベース**（ファイル存在時）: `canonicalize()` で正規化し、シンボリックリンクやドライブレター大小文字の差異を吸収
2. **文字列ベースフォールバック**（ファイル消失後）: `to_string_lossy().to_lowercase()` で大小文字を正規化し、`MAIN_SEPARATOR` で区切り文字境界を保証して `starts_with` 比較（`TempBackup` 等の誤検出を防止）

フォールバックは、zipアーカイブからの D&D 時に一時ファイルが即座に削除されるケースに対応するために必要。

### 一時パスの即座ロード

`process_drag_and_drop()` 内で `is_temp_path()` が true を返した場合、`pending_load`/`pending_append` を経由せず `load_file()`/`append_model()` を直接呼び出す。通常パスの2フレーム遅延（プログレスオーバーレイ表示用）の間に一時ファイルが消失する問題（`os error 3`）を回避する。

### D&D 先読みキャッシュ（PreloadedData）

`process_drag_and_drop()` で一時パスを検出した時点で、モデル本体と隣接ファイルのバイト列を `PreloadedData` にキャッシュし、以降のロードチェーン全体でディスクアクセスを排除する。

```rust
/// D&D temp ファイルの先読みデータ
pub struct PreloadedData {
    path: PathBuf,          // 元の一時ファイルパス
    main_bytes: Arc<[u8]>,  // モデル本体のバイト列
    aux_files: HashMap<PathBuf, Arc<[u8]>>,  // 隣接画像ファイル（相対パスキー）
}
```

#### ヘルパーメソッド

| メソッド | 説明 |
|---------|------|
| `read_or_preloaded(path)` | `preloaded.main_bytes` または `aux_files` にマッチすればキャッシュから返す。マッチしなければ `std::fs::read` にフォールバック |
| `take_or_collect_aux(path)` | `preloaded.aux_files` にマッチすれば clone で返す。マッチしなければ `collect_image_files_recursive` でディスク収集 |

#### データ受け渡しフロー

```
process_drag_and_drop:
  1. std::fs::read(&model_path) → PreloadedData.main_bytes
  2. collect_image_files_recursive() → PreloadedData.aux_files
  3. self.preloaded = Some(PreloadedData { ... })
  4. load_file() / append_model() を呼び出し
  5. PendingFbxChoice 未設定なら self.preloaded = None でクリア

FBX 選択ダイアログ経由:
  load_file() → PendingFbxChoice { preloaded: self.preloaded.take() }
  → execute_fbx_choice() → self.preloaded = choice.preloaded で復元
  → try_load_fbx() → read_or_preloaded() でキャッシュ使用
  → self.preloaded = None でクリア
```

#### 各形式での使用箇所

| メソッド | main file | aux files |
|---------|-----------|-----------|
| `try_load_fbx` | `read_or_preloaded` | `take_or_collect_aux` → `ReloadableSource::Snapshot` |
| `try_load_vrm` | `read_or_preloaded` | 埋め込み（外部参照なし） |
| `try_load_pmx` | `read_or_preloaded` | `preloaded_aux` 優先 → `std::fs::read` フォールバック |
| `try_load_pmd` | `read_or_preloaded` | `preloaded_aux` 優先 → `std::fs::read` フォールバック |
| `try_load_unitypackage` | `read_or_preloaded` | アーカイブ内に含まれる |
| `try_load_fbx_animation` | `read_or_preloaded` → `load_fbx_animation_from_data` | N/A |
| `append_model` (FBX/PMX/PMD/VRM) | `read_or_preloaded` | N/A（IrModel 構築のみ） |

### 補助ファイルキャッシュ

| 形式 | aux_files の内容 |
|------|----------------|
| VRM / GLB | 空（テクスチャはバイナリ埋め込み） |
| FBX | 隣接画像ファイルを再帰収集（サブディレクトリ構造保持） |
| PMX | `pmx.textures` の各パスからテクスチャファイルを収集 |
| PMD | テクスチャ + 同名 `.txt`（材質名テキスト） |

FBX の外部テクスチャは `collect_image_files_recursive()` で親ディレクトリ以下を再帰走査し、`strip_prefix(base_dir)` で相対パスをキーに保持。リロード時は `create_dir_all` でサブディレクトリ構造を復元してから FBX パーサーに渡す。

### TextureSource enum

テクスチャ割り当ての読み込み元を追跡する。`TextureState.assignments` の値型。

| バリアント | 説明 |
|-----------|------|
| `File(PathBuf)` | 通常のファイルパス |
| `Cached { original_name, data: Arc<[u8]>, is_psd }` | 一時ファイルからのキャッシュ。`Arc<[u8]>` で clone コスト削減 |

### reload_from_source

`load_file()` の UI 分岐（FBX メッシュ+アニメ選択ダイアログ等）を回避し、`ReloadableSource` から直接 `try_load_*` を呼ぶ。`Result` を返し、失敗時は退避した状態を復元して早期リターン。

### テクスチャD&Dプレビューキャッシュ

ZIP 内テクスチャを D&D した際、一時ファイルが消失してもテクスチャ割り当てが正しく記録されるよう、`PendingTexPreview` にデータをキャッシュする。

| フィールド | 型 | 説明 |
|-----------|------|------|
| `cached_data` | `Vec<u8>` | ファイル読み込み時にキャッシュしたバイトデータ |
| `is_psd` | `bool` | 拡張子判定結果（読み込み時に確定） |
| `was_temp` | `bool` | 一時パス判定結果（`is_temp_path` を `std::fs::read` **前**に評価して確定） |

#### 処理フロー

```
open_texture_preview:
  1. was_temp = is_temp_path(&path)    ← ファイル存在時に判定（canonicalize 前提）
  2. data = std::fs::read(&path)       ← バイトデータ読み込み
  3. upload_texture_from_bytes(&data)   ← GPU テクスチャ作成
  4. PendingTexPreview { cached_data: data, is_psd, was_temp, ... }

apply_tex_preview:
  1. tex_data = preview.cached_data.clone()  ← キャッシュから取得（再読み込みなし）
  2. is_psd = preview.is_psd                 ← キャッシュから取得
  3. cached_data = if preview.was_temp { Some(...) } else { None }
  4. TextureSource::Cached or File に分岐
```

**重要**: `is_temp_path` の評価は `std::fs::read` より前に行う。`canonicalize()` がファイル存在を前提とするため、読み込み後にファイルが消えると判定が失敗するレースを防ぐ。

### UnityPackage アーカイブスナップショット

ZIP 内 .unitypackage を D&D した際、アーカイブデータを `Arc<[u8]>` としてスナップショット保持する。

#### 構造体フィールド

| 構造体 | 追加フィールド |
|--------|--------------|
| `PendingUnityPackage` | `archive_snapshot: Option<Arc<[u8]>>` |
| `PendingPkgModelLoad` | `archive_snapshot: Option<Arc<[u8]>>` |
| `PendingFbxChoicePkg` | `archive_snapshot: Option<Arc<[u8]>>` |

#### snapshot 生成フロー

```
try_load_unitypackage:
  1. is_temp = is_temp_path(path)      ← std::fs::read 前に判定
  2. archive_data = std::fs::read(path)
  3. assets = extract_all_assets(&archive_data)
  4. snapshot = if is_temp { Some(Arc::from(archive_data)) } else { None }
  5. PendingPkgModelLoad / PendingUnityPackage に snapshot を格納
```

#### snapshot 伝播経路

```
try_load_unitypackage / try_load_unitypackage_for_append
  → PendingUnityPackage / PendingPkgModelLoad に格納
    → ui.rs show_fbx_select_dialog で PendingPkgModelLoad に引き継ぎ
      → process_pending_tasks で load_fbx_from_assets / load_vrm_from_assets に渡す
        → ReloadableSource::Snapshot を構築して finish_load に渡す
          → LoadedModel.source に格納
            → reload_current 時に reload_unitypackage(&source, ...) で Snapshot から復元
```

#### reload_unitypackage / reload_append_unitypackage の変更

シグネチャを `path: &Path` から `source: &ReloadableSource` に変更。Snapshot バリアントの場合は `main_bytes.to_vec()` でアーカイブデータを復元し、File バリアントの場合は従来通り `std::fs::read` で読み込む。

### .gltf の除外

`.gltf` ファイルは外部バッファ参照（`.bin`・画像ファイル）を持つため、スナップショット化の対象外。`gltf::import_slice` では外部URI を解決できないため、通常の `load_glb(path)` パスを使用。

## リロード時テクスチャ正規化（v0.2.4）

### reload_unitypackage のテクスチャ復元

UnityPackage リロード時に手動割当テクスチャを復元する際、正規パス（`assign_texture_source_to_material`）と同じ PSD→PNG 変換・MIME タイプ設定を適用する。

| テクスチャ形式 | 処理 | ir_filename | mime_type |
|-------------|------|-------------|-----------|
| PSD | `psd_to_png()` で PNG に変換 | `{basename}.png` | `image/png` |
| PNG | そのまま | 元のファイル名 | `image/png` |
| TGA | そのまま | 元のファイル名 | `image/x-tga` |
| BMP | そのまま | 元のファイル名 | `image/bmp` |
| その他 | そのまま | 元のファイル名 | `image/jpeg` |

PSD→PNG 変換失敗時は `continue` で当該材質への割当てをスキップ（通常パスの失敗時中断と一貫）。

`name_to_ir: HashMap<String, usize>` キャッシュにより、同一テクスチャ名の重複 IrTexture 追加を防止。パッケージ内テクスチャ名は一意が保証されるため、`tex_name` 単独キーで十分。

### assign_texture_source_to_material の IrTexture 重複排除

手動テクスチャ割り当て時、`filename + data.len() + data` の完全一致で既存 IrTexture を検索し、存在すればインデックスを再利用する。

```rust
let tex_idx = loaded.ir.textures.iter()
    .position(|t| t.filename == ir_filename
        && t.data.len() == ir_data.len()
        && t.data == ir_data)
    .unwrap_or_else(|| { /* 新規追加 */ });
```

- `data.len()` を先にチェックすることで、サイズが異なるテクスチャは O(1) でスキップ
- 外部ファイルシステムからの割り当てでは同名別内容が起こりうるため、`filename` 単独ではなく `data` も比較
- pkg 復元パスでは `tex_name` キーのキャッシュで重複排除（パッケージ内テクスチャ名の一意性が保証されるため）

## シェーダー対応PMX材質変換（v0.2.4）

### select_toon()

MToon の shade_color と diffuse の輝度比に基づいてトゥーンテクスチャを選択する。Rec. 709 の輝度係数 `(0.2126, 0.7152, 0.0722)` を使用。

| shade/diffuse 輝度比 | トゥーン | 説明 |
|---------------------|---------|------|
| < 0.25 | Shared(0) = toon01 | 硬い影（shade << diffuse） |
| 0.25–0.45 | Shared(1) = toon02 | やや硬い |
| 0.45–0.65 | Shared(2) = toon03 | 中間 |
| 0.65–0.85 | Shared(4) = toon05 | 柔らかめ |
| ≥ 0.85 | Shared(6) = toon07 | 最も柔らかい（shade ≈ diffuse） |

非 MToon は `Shared(0)` を維持（回帰防止）。shade_color が存在しない場合は `Shared(2)`（中間）。

### MToon ambient/specular 補正

変換段階（`convert/material.rs`）でのみ適用。抽出段階（`vrm/extract.rs`）はソース準拠の値を維持。

| パラメータ | MToon | 非 MToon |
|-----------|-------|---------|
| ambient | `shade_color * 0.5`（shade_color 無しなら `diffuse * 0.4`） | 変更なし |
| specular | `Vec3::ZERO` | 変更なし |
| specular_power | `0.0` | 変更なし |

## Aスタンス変換結果の管理（v0.2.4）

### AStanceResult enum

Aスタンス変換の結果を型安全に管理する enum。`IrModel.astance_result` に格納される。

| バリアント | 説明 |
|-----------|------|
| `NotRequested` | 変換未要求（チェックボックスOFF、または非対応形式） |
| `Applied(usize)` | 変換成功。引数は補正した腕の数（通常2） |
| `AlreadyAStance` | 既にAスタンスに近いためスキップ |
| `NotFound` | 腕ボーンが見つからず変換失敗 |

### 判定ロジック

`compute_astance_corrections()` が以下の優先度で結果を決定:

1. **腕ボーン不在**: `has_arms` チェック（leftUpperArm/leftLowerArm または rightUpperArm/rightLowerArm のペアが 1 つも存在しない）→ `NotFound`
2. **腕方向が異常**: 水平成分ゼロ（真上/真下向き）、回転軸計算不能 → `skipped_count` に加算
3. **既にAスタンス**: 現在角度が 25° 超かつ下向き → `skipped_count` に加算
4. **正常変換**: 30° 回転補正を適用 → `Applied(n)`
5. **結果決定**: corrections > 0 → `Applied(n)`, skipped > 0 → `AlreadyAStance`, それ以外 → `NotFound`

### IrModel::merge() での統合

追加読み込み（アペンド）時に `IrModel::merge()` で `astance_result` を統合する:

| ホスト | 追加 | 結果 | 理由 |
|--------|------|------|------|
| `NotRequested` | 任意 | 追加側の値 | ホストは未要求なので追加側に委任 |
| `Applied(a)` | `Applied(b)` | `Applied(a+b)` | 合算 |
| `Applied(n)` | `NotFound` | `Applied(n)` | メインモデルが変換済みなら小物の失敗は無視 |
| `Applied(n)` | `AlreadyAStance` | `Applied(n)` | 変換済み優先 |
| `AlreadyAStance` | `NotFound` | `AlreadyAStance` | AlreadyAStance 優先 |
| `NotFound` | `NotFound` | `NotFound` | 両方失敗 |

### ビューアでの警告表示

PMX 変換成功時、`ir_ref.astance_result` を参照:

- `NotFound` → `ConvertMessage::Warning`（赤文字オーバーレイ）: 「腕ボーンが見つからず変換できませんでした」
- `AlreadyAStance` → `ConvertMessage::Success` に注記付加: 「既にAスタンスに近いためスキップしました」
- `Applied(_)` / `NotRequested` → 通常の成功メッセージ

`ConvertResult::Warning` は `Failure` と同じ赤文字で表示されるが、変換自体は成功している点で `Failure` と区別される。

## 表示材質のみ出力（v0.2.3）

ビューアの PMX 変換時に、表示タブで非表示にした材質を出力から除外するオプション機能。`export_filter.rs` モジュールで実装。

### 設計方針

- **ビューア固有**: フィルタロジックは `viewer/export_filter.rs` に配置。コア変換ロジック（`pmx/build.rs`, `lib.rs`）には一切変更なし
- **IrModel 手組み構築**: `IrModel`/`IrMesh`/`IrPhysics` に `Clone` がないため、フィルタ済み IR をフィールド単位で新規構築
- **draw→material 変換**: `material_visibility` は DrawCall 単位（GPU 描画コール単位）で管理されているため、`mat_cache.draw_indices` を経由して `material_index` の `HashSet` に変換

### 処理フロー（`build_filtered_ir`）

```
Phase 1: 材質リマップ（old_mat_idx → new_mat_idx の HashMap 構築）
Phase 2: メッシュフィルタ + 頂点リマップテーブル構築
         old_global_vtx_idx → new_global_vtx_idx（除外メッシュの頂点は None）
Phase 3: モーフの有効性判定（再帰的収束ループ）
         頂点モーフ: リマップ後に1エントリ以上残れば有効
         グループモーフ: 子モーフが1つ以上有効なら有効（反復判定）
Phase 4: morph_remap 構築 + モーフ構築（頂点/グループ両対応）
Phase 5: テクスチャ pruning + texture_index リマップ
Phase 6: IrModel 構築（ボーン・物理はそのままコピー）
```

### モーフの再帰的有効性判定

頂点モーフの除外によりグループモーフの子が消失する場合がある。ネストしたグループモーフ（`outer → inner → vertex`）に対応するため、収束ループで判定:

```rust
// Phase 3: morph_alive 配列を収束するまで反復
loop {
    let mut changed = false;
    for (i, morph) in ir.morphs.iter().enumerate() {
        if morph_alive[i] { continue; }
        if let IrMorphKind::Group(goffs) = &morph.kind {
            if goffs.iter().any(|&(child, _)| morph_alive[child]) {
                morph_alive[i] = true;
                changed = true;
            }
        }
    }
    if !changed { break; }
}
```

最悪 O(depth) 回で収束する（各反復で少なくとも 1 候補が確定するため）。

### テクスチャ pruning

フィルタ後の材質が参照する `texture_index` / `shade_texture_index` / `outline_width_texture_index` を収集し、使用されているテクスチャのみ残す。材質の各 index をリマップ。全材質非表示の場合はテクスチャも空にする。

### 仕様

| 条件 | 動作 |
|------|------|
| デフォルト | OFF（従来通り全材質出力） |
| 全材質非表示 | 空 PMX を出力 + warning ログ |
| 空になった頂点モーフ | 削除 + warning ログ |
| 空になったグループモーフ | 削除 + warning ログ |
| モデルロード時 | `export_visible_only` を `false` にリセット |
| PMX/PMD ロード時 | UI でチェックボックスが無効化 |

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
│   ├── parser.rs        FBX バイナリ / ASCII パーサー（Content ブロック特別処理含む）
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
│   ├── types.rs         中間表現（IrModel / IrBone / IrMesh 等、SourceFormat / merge 2パス方式）
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
    ├── export_filter.rs 表示材質のみ出力フィルタ（IrModel → フィルタ済み IrModel）
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
