<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [更新履歴](#%E6%9B%B4%E6%96%B0%E5%B1%A5%E6%AD%B4)
  - [v0.2.10](#v0210)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD)
    - [UTS2 対応パラメータ](#uts2-%E5%AF%BE%E5%BF%9C%E3%83%91%E3%83%A9%E3%83%A1%E3%83%BC%E3%82%BF)
    - [改善](#%E6%94%B9%E5%96%84)
    - [v0.2.10 未対応（将来対応）](#v0210-%E6%9C%AA%E5%AF%BE%E5%BF%9C%E5%B0%86%E6%9D%A5%E5%AF%BE%E5%BF%9C)
  - [v0.2.9](#v029)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-1)
    - [改善](#%E6%94%B9%E5%96%84-1)
    - [バグ修正](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3)
    - [実装詳細](#%E5%AE%9F%E8%A3%85%E8%A9%B3%E7%B4%B0)
    - [コード品質・パフォーマンス改善](#%E3%82%B3%E3%83%BC%E3%83%89%E5%93%81%E8%B3%AA%E3%83%BB%E3%83%91%E3%83%95%E3%82%A9%E3%83%BC%E3%83%9E%E3%83%B3%E3%82%B9%E6%94%B9%E5%96%84)
  - [v0.2.8](#v028)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-2)
    - [改善](#%E6%94%B9%E5%96%84-2)
  - [v0.2.7](#v027)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-3)
    - [バグ修正](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3-1)
    - [改善](#%E6%94%B9%E5%96%84-3)
    - [コード品質改善](#%E3%82%B3%E3%83%BC%E3%83%89%E5%93%81%E8%B3%AA%E6%94%B9%E5%96%84)
  - [v0.2.6](#v026)
    - [バグ修正](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3-2)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-4)
    - [改善](#%E6%94%B9%E5%96%84-4)
    - [コード品質・パフォーマンス改善](#%E3%82%B3%E3%83%BC%E3%83%89%E5%93%81%E8%B3%AA%E3%83%BB%E3%83%91%E3%83%95%E3%82%A9%E3%83%BC%E3%83%9E%E3%83%B3%E3%82%B9%E6%94%B9%E5%96%84-1)
  - [v0.2.5](#v025)
    - [改善](#%E6%94%B9%E5%96%84-5)
    - [コード品質・パフォーマンス改善](#%E3%82%B3%E3%83%BC%E3%83%89%E5%93%81%E8%B3%AA%E3%83%BB%E3%83%91%E3%83%95%E3%82%A9%E3%83%BC%E3%83%9E%E3%83%B3%E3%82%B9%E6%94%B9%E5%96%84-2)
  - [v0.2.4](#v024)
    - [改善](#%E6%94%B9%E5%96%84-6)
  - [v0.2.3](#v023)
    - [改善](#%E6%94%B9%E5%96%84-7)
  - [v0.2.2](#v022)
    - [コード品質・パフォーマンス改善](#%E3%82%B3%E3%83%BC%E3%83%89%E5%93%81%E8%B3%AA%E3%83%BB%E3%83%91%E3%83%95%E3%82%A9%E3%83%BC%E3%83%9E%E3%83%B3%E3%82%B9%E6%94%B9%E5%96%84-3)
  - [FBX 対応](#fbx-%E5%AF%BE%E5%BF%9C)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# 更新履歴

[English](CHANGELOG.en.md)

## v0.2.10

### 新機能

- **UTS2（Unity-Chan Toon Shader Ver.2）サポート** — VRM 0.0 モデルで使用される UTS2 シェーダーを自動検出し、既存の MToon 描画パイプラインに近似変換して表示・PMX 変換に対応
  - `ShaderFamily` enum 導入（`Other` / `Mtoon` / `Uts2`）による複数シェーダー分類基盤
  - 3重判定による UTS2 検出: シェーダー名（`UnityChanToonShader/*`, `Toon/Toon`）+ UTS2 固有プロパティ（`_utsVersion`, `_BaseColor_Step`）
  - VRM 0.0 / VRM 1.0 の MToon 材質にも `ShaderFamily::Mtoon` を明示設定

### UTS2 対応パラメータ

| UTS2 プロパティ | 変換先 |
|---|---|
| `_BaseColor` / `_MainTex` | ベースカラー / テクスチャ |
| `_1st_ShadeColor` / `_1st_ShadeMap` | MToon shade_color / shade_texture |
| `_2nd_ShadeColor` | PMX ambient（`color * 0.5`） |
| `_BaseColor_Step` / `_BaseShade_Feather` | shading_toony / shading_shift |
| `_Outline_Width` / `_Outline_Color` | アウトライン（NML/POS → WorldCoordinates 近似） |
| `_RimLight` / `_RimLightColor` / `_RimLight_Power` | リムライティング |
| `_MatCap` / `_MatCap_Sampler` / `_MatCapColor` | MatCap テクスチャ |
| `_Emissive_Tex` / `_Emissive_Color` | エミッシブ（HDR: linear 維持） |
| `_NormalMap` / `_BumpScale` | 法線マップ |
| `_HighColor` / `_HighColor_Power` | PMX specular（PMX 出力のみ） |
| `_GI_Intensity` | GI（安全デフォルト 0.0 固定） |
| `_CullMode` | カリングモード |

### 改善

- **UTS2 alpha モード判定** — シェーダーバリアント名ベースで判定（`_TransClipping` → Blend、`_Clipping` → Mask）。glTF core の `alpha_mode` をフォールバックとして保持
- **UTS2 アウトライン POS モード** — UTS2 の POS outline は MToon の ScreenCoordinates とは異なるため WorldCoordinates 近似に統一し warning を出力
- **UTS2 ClippingMask 警告** — `_ClippingMask` テクスチャ使用材質で未対応を warning 出力し base alpha でフォールバック
- **ambient 上書き抑止** — UTS2 材質では `_2nd_ShadeColor` で設定した ambient が抽出末尾の `diffuse * 0.4` 再計算で上書きされないよう抑止
- **PMX 変換 UTS2 分岐** — UTS2 材質では HighColor → specular、2nd_ShadeColor → ambient をそのまま PMX に出力（MToon の specular 抑制をスキップ）
- **VRM 0.x ヘルパー共通化** — `get_float` / `get_color3` / `resolve_tex` / `main_tex_st` を MToon/UTS2 共通ヘルパーに整理。`adopt_main_tex` で `_MainTex` authoritative 処理を一箇所に集約

### v0.2.10 未対応（将来対応）

- ClippingMask 専用テクスチャ / HighColor ビューア描画 / ShadingGradeMap / 2nd_ShadeMap テクスチャ / AngelRing / Stencil 系バリアント

## v0.2.9

### 新機能

- **MToon 2色トゥーンシェーディング** — VRM の MToon 材質をビューアで 2 色トゥーン（lit/shade）で表示。`shadingToonyFactor` で影境界の硬さ、`shadingShiftFactor` で影の閾値シフトを制御。VRM 1.0（`VRMC_materials_mtoon`）と VRM 0.0（`_ShadeToony` / `_ShadeShift`）の両方に対応。非 MToon 材質は従来通り Half-Lambert で描画
  - `MaterialUniform` を 16→80 bytes に拡張し、`shade_color` / `is_mtoon` / `shading_toony` / `shading_shift` + アウトラインパラメータを格納
  - フラグメントシェーダー内で仕様準拠の `linearstep` ベース lit/shade 補間を実装（`dot(N,L)` [-1,1] レンジ）
  - `IrMaterial` に `shading_toony_factor` / `shading_shift_factor` フィールドを追加
- **MToon アウトライン描画** — inverted hull 法によるアウトライン（輪郭線）描画。`outlineWidthFactor`（世界座標/スクリーン座標）と `outlineColorFactor` に対応。`outlineLightingMixFactor` でライティング混合率を制御。UI チェックボックスで ON/OFF 切替可能
  - `PipelineSet` に `pipeline_outline`（Front cull パイプライン）を追加（sRGB / Unorm 各版）
  - `IrMaterial` に `OutlineWidthMode` enum、`outline_width_factor`、`outline_lighting_mix` を追加
  - VRM 1.0（`outlineWidthMode` / `outlineWidthFactor` / `outlineLightingMixFactor`）と VRM 0.0（`_OutlineWidthMode` / `_OutlineWidth` / `_OutlineLightingMix`）の両方から読み取り
  - `DrawCall` に `has_outline` フラグを追加し、全 alphaMode 材質でアウトライン描画（BLEND は ZWrite OFF）
- **MToon リムライティング + MatCap** — VRM 1.0 MToon のパラメトリックリムライティングと MatCap テクスチャに対応
  - パラメトリックリム: `parametricRimColorFactor`（色）、`parametricRimFresnelPowerFactor`（フレネル指数）、`parametricRimLiftFactor`（リフト量）で形状を制御。フレネル効果で輪郭が発光する表現を実現
  - MatCap: `matcapTexture` / `matcapFactor` に対応。ビュー空間法線から直交基底を構築して UV を算出し、MatCap テクスチャをサンプリング
  - `rimLightingMixFactor` で周囲光との混合率を制御（0.0=放射, 1.0=完全混合）
  - `MaterialUniform` を 80→112 bytes に拡張、パイプラインレイアウトに MatCap テクスチャ用 bind group(3) を追加
  - 頂点シェーダーにワールド座標出力を追加し、フラグメントシェーダーで視線方向ベースのリム計算を実装
- **MToon 追加テクスチャ対応** — VRM 1.0 MToon の補助テクスチャ 3 種に対応し、描画品質を向上
  - `shadeMultiplyTexture`: 影色テクスチャ乗算（RGB）。ピクセルごとに影色を変化させ、より細かい影表現を実現
  - `shadingShiftTexture`: ピクセルごとのシェーディングシフト（R チャネル × scale）。部位によって影の付き方を制御
  - `rimMultiplyTexture`: リムライティング乗算テクスチャ（RGB）。リム効果の適用範囲をテクスチャで制御
  - bind group(3) を MToon 補助テクスチャパック（テクスチャごとにサンプラーを持つ 16 bindings 構成）に再構成。`MaterialUniform` を 112→144 bytes に拡張
  - テクスチャ未使用材質にはデフォルトテクスチャ（白 or 黒）を自動バインドし、パイプライン切り替え不要
- **MToon UV アニメーション** — VRM 1.0 MToon の UV スクロール・回転アニメーションに対応
  - `uvAnimationScrollXSpeedFactor` / `uvAnimationScrollYSpeedFactor`: UV 水平・垂直スクロール
  - `uvAnimationRotationSpeedFactor`: UV 中心回転
  - `uvAnimationMaskTexture`: B チャネルでアニメーション適用範囲を制御
  - `CameraUniform` に累積時間 `time` フィールドを追加し、毎フレーム更新

### 改善

- **MToon アウトラインに `outlineWidthMultiplyTexture` 反映** — `outlineWidthMultiplyTexture` の G チャネルをアウトライン頂点シェーダーで `textureSampleLevel` によりサンプリングし、頂点ごとにアウトライン幅を制御。mtoon_aux bind group (binding 6) に追加し、材質固有の bind group をアウトライン描画にも適用。顔や髪で輪郭線を弱める VRM が正しく表示される
- **`outlineWidthMultiplyTexture` への UV アニメーション適用（MToon 仕様準拠）** — UV Animation 計算を `apply_uv_animation()` 共通関数に抽出し、頂点シェーダー `vs_outline` でも UV Animation 適用済み座標で `outlineWidthMultiplyTexture` をサンプリングするよう変更。MToon 仕様の UV Animation 対象テクスチャ 5 種（shadeMultiply / shadingShift / rimMultiply / outlineWidthMultiply + glTF コア 3 種）すべてに UV Animation が反映される。`uvAnimationMaskTexture` の bind group visibility に `VERTEX` を追加
- **MToon screenCoordinates アウトライン改善** — UniVRM 準拠の clip 空間法線変換・アスペクト比補正（`height/width` による X 方向縮小）・カメラ正面法線抑制を実装。画角やアスペクト比による輪郭線の太さのぶれを解消
- **MToon 補助テクスチャの色空間修正** — `shadingShiftTexture` と `uvAnimationMaskTexture` を仕様通りリニア色空間（Unorm ビュー）で読み込むよう修正。sRGB ビュー使用時の二重ガンマ変換による値の歪みを解消
- **`shadingShiftTexture` 計算式を仕様準拠に修正** — `(tex * 2.0 - 1.0) * scale` を VRM 1.0 仕様通り `tex * scale` に修正
- **`shadingToony/shadingShift` の shading 式を仕様準拠に修正** — `half_lambert` [0,1] + `smoothstep` から仕様通りの `dot(N,L)` [-1,1] + `linearstep(-1+toony, 1-toony, shading+shift)` に変更。UniVRM と同じ影境界の硬さ・位置になる
- **`shadeColorFactor` デフォルト値を仕様準拠に修正** — VRM 1.0 MToon で `shadeColorFactor` 未指定時のデフォルトを `Vec3::ZERO`（黒）に修正（仕様のデフォルト `[0,0,0]`）。抽出時に常に `Some(...)` を格納するよう変更し、ビューア表示と PMX 変換で `None`（shade_color 無し）と「デフォルトとしての黒」が区別されるようになった
- **VRM 0.x `_Color` / `_MainTex` lit 色・テクスチャ正規化** — VRM 0.x MToon 正規化ブロックで `materialProperties` の `_Color` → `ir_mat.diffuse`、`_MainTex` → `ir_mat.texture_index` / `base_color_tex_info` に反映するよう追加。glTF core の `baseColorFactor` / `baseColorTexture` は VRM 0.x では近似値の場合があるため、MToon と判定した後は `materialProperties` 側を優先する（UniVRM `MigrationMToonMaterial.cs:148-164` 準拠）
- **VRM 0.x `_MainTex` ST の Y オフセット変換追加** — VRM 0.x の `_MainTex` ST（Scale/Translation）を glTF `KHR_texture_transform` に変換する際、Y オフセットに `offset.y = 1.0 - unityOffset.y - scale.y` を適用するよう修正。Unity のテクスチャ座標系（左上原点）と glTF（左下原点）の Y 軸解釈の違いを吸収する（UniVRM `Vrm10MaterialExportUtils.ExportTextureTransform` 準拠）
- **`renderQueueOffsetNumber` の範囲制限を仕様準拠に追加** — Opaque/Mask は常に 0、BlendWithZWrite は clamp(0,+9)、Blend は clamp(-9,0) を強制。UniVRM MToonValidator と同等の制限
- **VRM 0.x `renderQueue` 範囲外チェック追加** — UniVRM `GetRenderQueueRequirement` 準拠の範囲検証を追加。`renderQueue` が許容範囲外（Blend: 2951~3000、BlendWithZWrite: 2501~2550）の場合は offset=0 を返す。壊れた/手編集された VRM 0.x 入力で描画順が端値に張り付く問題を解消
- **`rimLightingMixFactor` の光量係数を N·L 非依存に修正** — UniVRM 準拠で `light_factor` から `dot(N,L) * 0.5 + 0.5`（Half-Lambert）を除去し、`light_intensity + ambient` の直接合成に変更。リムライティングは視線フレネル効果であり、背面側でも N·L 非依存で光量係数が一定になるべき。逆光・輪郭寄りのポーズでリムが過度に暗くなる問題を解消
- **glTF sampler 情報のテクスチャ別反映** — `IrTextureInfo` に `IrSamplerInfo`（wrap_u / wrap_v / mag_filter / min_filter）を追加し、glTF の `sampler` オブジェクトからテクスチャごとの wrapS / wrapT / magFilter / minFilter を読み取り。ビューア GPU 側は `HashMap<IrSamplerInfo, wgpu::Sampler>` キャッシュで同一設定のサンプラーを共有。CPU 側の `sample_image_g_channel` も wrap mode に応じた UV 座標変換を実施。`outlineWidthMultiplyTexture` / `uvAnimationMaskTexture` 等の `CLAMP_TO_EDGE` 指定が正しく再現されるようになった
- **MToon 補助テクスチャのサンプラー個別化** — bind group(3) のサンプラーを全テクスチャ共有（1 sampler + 8 textures）からテクスチャごとに分離（8 samplers + 8 textures = 16 bindings）に変更。glTF の texture 単位 sampler モデルに完全準拠し、補助テクスチャごとに異なる wrap / filter 設定が正しく反映されるようになった。WGSL 側も `s_mtoon_aux` 共有サンプラーを廃止し、`s_matcap` / `s_shade_multiply` / `s_normal` 等テクスチャ固有のサンプラーに分離
- **glTF minFilter の mipmap 情報保持** — `IrFilterMode`（2 値: Nearest / Linear）を `IrMagFilter` + `IrMinFilter`（6 値: Nearest / Linear / NearestMipmapNearest / LinearMipmapNearest / NearestMipmapLinear / LinearMipmapLinear）に分離。glTF の `minFilter` が持つ mipmap 選択方式をそのまま保持し、`ensure_sampler()` で wgpu の `min_filter` と `mipmap_filter` を正しく分離して設定するようになった
- **`CameraUniform` に `aspect` フィールド追加** — MToon screenCoordinates アウトラインのアスペクト比補正に使用
- **MToon 透明描画順制御** — glTF `alphaMode`（OPAQUE / MASK / BLEND）と MToon 拡張の `transparentWithZWrite` / `renderQueueOffsetNumber` に対応。描画を仕様準拠の 4 段階に分離し、半透明の前髪・アクセサリの前後関係を正しく再現
  - `IrMaterial` に `AlphaMode` enum（Opaque / Mask / BlendWithZWrite / Blend）、`alpha_cutoff`、`render_queue_offset` を追加
  - `DrawCall` に `RenderQueue` enum を追加し、`renderQueueOffsetNumber` で BLEND カテゴリ内を安定ソート
  - MASK モード: フラグメントシェーダーに `alphaCutoff` による `discard` を実装
  - BlendWithZWrite: 半透明＋デプス書込ありパイプライン（`pipeline_alpha_zwrite_cull` / `pipeline_alpha_zwrite_no_cull`）を新設
  - 描画順: OPAQUE → MASK → BlendZWrite → Blend。OPAQUE/MASK はフェーズ後にまとめてアウトライン描画、BLEND/BlendZWrite はサーフェスとアウトラインをインターリーブ描画
- **MToon 補助テクスチャの `texCoord` / `KHR_texture_transform` 保持** — `IrTextureInfo` 構造体を導入し、MToon 補助テクスチャ 6 種（shade / matcap / shadingShift / rimMultiply / uvAnimationMask / outlineWidth）の `texCoord`・`KHR_texture_transform`（offset / scale / rotation）を IR 層で保持するよう拡張。メッシュの `TEXCOORD_1` も `IrMesh.uvs1` に読み取り。GPU シェーダー側で `resolve_mtoon_uv()` により texCoord 選択 + KHR_texture_transform を適用
- **テクスチャ pruning の全 MToon テクスチャ対応** — エクスポートフィルタのテクスチャ pruning を `IrTextureInfo` ベースに書き換え、matcap / shadingShift / rimMultiply / uvAnimationMask テクスチャも収集・リマップ対象に追加
- **MToon ScreenCoordinates アウトライン計算式を UniVRM 完全準拠に修正** — (1) 法線の正規化順序を UniVRM と一致（normalize → aspect 乗算）に修正。(2) `CameraUniform` に射影行列の `proj_11`（= 1/tan(fov/2)）を追加し、UniVRM の `MToon_GetOutlineVertex_ScreenCoordinatesWidthMultiplier` と同等の距離クランプ（`min(clip.w, maxDistance)`）を実装。広角カメラ・遠距離での太すぎを抑制
- **MToon 補助テクスチャの `texCoord` / `KHR_texture_transform` をシェーダーに接続** — `MaterialUniform` に 5 補助テクスチャ分の UV パラメータ（texCoord・offset・scale・rotation）を追加（144→304 bytes）。`Vertex` に `uv1`（TEXCOORD_1）を追加。WGSL に `resolve_mtoon_uv()` / `apply_texture_transform()` / `apply_uv_anim_core()` ヘルパ関数を追加し、各補助テクスチャで texCoord 選択 + KHR_texture_transform 適用を実行。UV Animation 対象（shade / shift / rim / outline_width）と非対象（uv_mask / matcap）を UniVRM 準拠で区別
- **`baseColorTexture` の `texCoord` / `KHR_texture_transform` 対応** — `IrMaterial` に `base_color_tex_info: Option<IrTextureInfo>` を追加し、ベースカラーテクスチャの `texCoord` / `KHR_texture_transform`（offset / scale / rotation）を保持。`MaterialUniform` に `base_uv_a` / `base_uv_b` を追加（304→336 bytes）し、フラグメントシェーダーで `resolve_mtoon_uv()` によりベースカラーテクスチャにも texCoord 選択 + KHR_texture_transform を適用。補助テクスチャと同一の UV パイプラインに統一
- **アウトライン頂点シェーダーの UV1 対応** — `apply_uv_animation()` を `apply_uv_animation_pair(uv0, uv1)` に変更し、`vec4` で UV0/UV1 ペアを返す形に統一。アウトライン頂点シェーダーで `uv1_in` が無視されていた問題を修正し、`outlineWidthMultiplyTexture` と `uvAnimationMaskTexture` の `texCoord=1` が正しく機能するようになった
- **BLEND 材質のカメラ距離ソート** — `DrawCall` に重心位置 `center` を追加し、同一 `renderQueueOffsetNumber` 内の `RenderQueue::Blend` 材質をカメラ距離（`distance_squared`）で back-to-front ソート。半透明メッシュ同士の前後関係が改善
- **BLEND/BlendZWrite アウトライン描画順のインターリーブ化** — 透明フェーズ（BLEND / BlendZWrite）ではサーフェスとアウトラインを各 draw ごとに連続発行するよう変更。ZWrite OFF の透明アウトラインが手前サーフェスの上に浮く問題を解消（UniVRM のマルチパス描画と同等の合成順）。OPAQUE / MASK は従来通り深度バッファで保護されるため 2 パス構造を維持
- **透明ソート距離キーの動的更新** — アニメーション再生中に BLEND / BlendZWrite draw の重心を `current_vertices()` から毎フレーム再計算。rest pose 固定の重心では動的シーンで back-to-front ソートが破綻する問題を解消。不透明 draw はビルド時の固定重心を維持（再計算不要）
- **glTF emissive（発光）対応** — `emissiveFactor` + `emissiveTexture` を glTF 標準プロパティとして全形式（VRM / FBX / PMX / PMD）に対応。MToon シェーダーでは UniVRM 準拠の `baseCol = lighting + emissive + rim` で加算。非 MToon でも `lit += emissive` で発光表現を反映。アウトラインの `compute_mtoon_surface_lighting()` にも emissive を含め、`outlineLightingMixFactor` 経由でアウトライン色に反映。`IrMaterial` に `emissive_factor` / `emissive_texture` / `normal_texture` / `normal_texture_scale` フィールドを追加。法線マップは screen-space derivative による tangent 構築で適用
- **VRM 0.x MToon 全プロパティ正規化** — VRM 0.x の `materialProperties` から未実装だった主要プロパティを VRM 1.0 系 `IrMaterial` に正規化。UniVRM `MigrationMToonMaterial.cs` / `MToon10Migrator.cs` の変換式に準拠。対象:
  - 描画モード: `_BlendMode` → `AlphaMode`、`_Cutoff` → `alpha_cutoff`、`_CullMode` → `is_double_sided`
  - テクスチャ: `_ShadeTexture`（未設定時は `_MainTex` を使用: UniVRM 破壊的マイグレーション準拠）、`_RimTexture`、`_EmissionMap`、`_UvAnimMaskTexture`、`_SphereAdd`（→ matcapTexture）、`_BumpMap`（→ normalTexture）
  - リム: `_RimColor`、`_RimFresnelPower`、`_RimLift`、`rimLightingMixFactor` = 1.0（UniVRM 破壊的マイグレーション準拠）
  - エミッション: `_EmissionColor`
  - UV アニメーション: `_UvAnimScrollX`、`_UvAnimScrollY`（Y 反転 × -1）、`_UvAnimRotation`（× 2π rad/s 変換）
  - Shading: `_ShadeToony` / `_ShadeShift` → UniVRM `GetShadingRange0X` + `MigrateToShadingToony/Shift` 変換式
  - アウトライン: `_OutlineColorMode` → `outlineLightingMixFactor`（FixedColor = 0.0、MixedLighting = 元値）
- **`KHR_texture_transform.texCoord` override 対応** — `read_texture_info()` で `extensions.KHR_texture_transform.texCoord` が存在する場合、TextureInfo 本体の `texCoord` より優先するよう修正。glTF 仕様準拠
- **VRM 0.x `renderQueue` → `render_queue_offset` 移行** — UniVRM `MigrationMToonMaterial.cs` 準拠の順位圧縮（rank compression）を実装。透明材質の source offset（`renderQueue - DefaultValue`）を `BTreeSet` に集約し、Blend は降順・BlendWithZWrite は昇順で連番を振ることで、相対順序を保持したまま VRM 1.0 仕様範囲（Blend: -9..0, BlendWithZWrite: 0..+9）に圧縮。単純な clamp では値が同一に潰れて相対順序が失われる問題を解消。範囲外の `renderQueue` は offset=0 を返す
- **VRM 0.x `_MainTex` ST（Scale/Translation）を MToon テクスチャに伝播** — VRM 0.x の `vectorProperties._MainTex`（`[offsetX, offsetY, scaleX, scaleY]` 順）を MToon テクスチャの `IrTextureInfo.offset` / `.scale` に反映（UniVRM `Vrm0XMToonValue.cs` 準拠）。`baseColorTexture` にも同一 ST を適用。MatCap（`_SphereAdd`）は例外として ST 非適用（UniVRM `MigrationMToonMaterial.cs:255-260` 準拠: "Texture transform is not required"）。identity transform（scale=1, offset=0）の場合はスキップ
- **VRM 0.x `ScreenCoordinates` アウトライン幅を UniVRM 準拠に正規化** — `outline_width_factor` を `w * 0.01 * 0.5` に修正（旧: 縦半分の%値 → 新: 縦全体の比率、1/200 換算）。VRM 0.x の ScreenCoordinates アウトラインが Unity と一致するようになった
- **VRM 0.x 色プロパティの sRGB→Linear 変換** — VRM 0.x MToon の `_ShadeColor`・`_RimColor`・`_OutlineColor` を抽出時に sRGB→Linear 変換するよう修正。UniVRM `MigrationMToonMaterial.cs` の `.ToFloat3(ColorSpace.sRGB, ColorSpace.Linear)` と同等。`_EmissionColor` は UniVRM 準拠で Linear→Linear のため変換対象外
- **MASK 材質の alpha_to_coverage 有効化** — `RenderQueue::Mask` 材質に専用パイプライン（`pipeline_mask_cull` / `pipeline_mask_no_cull`）を追加し、MSAA 有効時（sample_count > 1）に `alpha_to_coverage_enabled = true` を設定。UniVRM `MToonValidator.cs` の `UnityAlphaToMask = On` と同等。まつ毛・髪カード等の cutout 材質で MSAA によるジャギーが軽減される
- **`giEqualizationFactor` GI 実装（UniVRM 準拠）** — VRM 仕様準拠の `lerp(passthroughGi, uniformedGi, giEqualizationFactor)` を実装。SH/IBL 非搭載のため `passthroughGi` = `uniformedGi` = ambient とし、direct light を GI に混入させない（UniVRM の `indirectLight` / `indirectLightEqualized` と同等の分離構造）。VRM 1.0 `giEqualizationFactor`、VRM 0.x `_IndirectLightIntensity`（`1.0 - value` で変換）の両方に対応
- **アウトラインパイプラインに depth bias 追加** — MToon アウトラインの `pipeline_outline` / `pipeline_outline_blend` に UniVRM `Offset 1, 1` 相当の `DepthBiasState`（`constant: 1, slope_scale: 1.0`）を設定。inverted hull 法で本体とアウトラインの深度が近接することによる Z-fighting（輪郭の欠け・ちらつき）を防止。髪や薄い板ポリ、視線に平行な面で特に効果がある
- **MASK 材質アウトラインの AlphaToCoverage 有効化** — `pipeline_outline_mask`（MASK 材質専用アウトラインパイプライン）を新設し、MSAA 有効時に `alpha_to_coverage_enabled = true` を設定。本体パスだけでなくアウトラインパスでも cutout 境界が滑らかになり、髪カード・まつ毛等でサーフェスとアウトラインのエッジ品質が一致するようになった。UniVRM の `AlphaToMask = On` と同等
- **`shadingShiftTexture` に UV Animation を適用（UniVRM 準拠）** — `shadingShiftTexture` のサンプリング UV が生 UV（`in.uv`）を使用していたのをアニメーション済み UV（`anim_uv`）に修正。UniVRM では `GetMToonGeometry_Uv()` で一括変換した UV を全テクスチャに適用しており、`shadingShiftTexture` も例外ではない。UV スクロール・回転を使うマテリアルで影境界が正しく追従するようになった。フォワードパスとアウトラインパスの両方を修正
- **モーフターゲットの法線・接線デルタ追従** — `IrMorphTarget` に `normal_offsets` / `tangent_offsets` を追加し、glTF モーフターゲットの法線・接線デルタを疎表現（閾値 1e-7 フィルタ）で保持するよう拡張。ビューアの GPU モーフ適用（`apply_gpu_morph_recursive`）で位置だけでなく法線・接線にも weight × delta を加算。表情変形時に MToon の陰影境界・アウトライン押し出し方向・法線マップが変形後の面方向に追従するようになった。Aスタンス変換・頂点分割・エクスポートフィルタでも法線・接線デルタを正しく伝搬
- **NORMAL/TANGENT のみモーフの end-to-end 対応** — POSITION デルタを持たず NORMAL/TANGENT デルタのみのモーフターゲットが、抽出→エクスポートフィルタ→GPU 反映の全段で脱落していた問題を修正。(1) `extract.rs`: `IrMorph` 生成条件を `positions` のみから `positions || normals || tangents` の OR に拡張。(2) `export_filter.rs`: モーフ生存判定を 3 系統の和集合に変更。(3) `mesh.rs`: GPU モーフの影響頂点を `BTreeSet` で positions/normals/tangents の和集合から収集し、各属性を `HashMap` lookup（glTF 2.0 仕様で POSITION なしの morph target は合法）
- **モーフ適用時の CPU 側頂点キャッシュ同期** — `apply_morphs()` が GPU バッファのみ更新し `animated_vertices`（CPU 側キャッシュ）を更新していなかったため、モーフのみ変更されたフレームで `current_vertices()` がレスト形状を返し、MToon 半透明（Blend / BlendZWrite）の距離ソートがレスト形状基準になっていた問題を修正。`apply_morphs()` の末尾で `morph_work` を `animated_vertices` にも反映し、CPU 側と GPU 側の頂点データを常に同期

- **法線マップの接線空間を MikkTSpace で構築（UniVRM 準拠）** — screen-space derivative（`dpdxCoarse`/`dpdyCoarse`）による近似 TBN を廃止し、`mikktspace` crate による MikkTSpace アルゴリズムで頂点接線を生成。glTF に `TANGENT` 属性がある場合はスキニング変換して使用し、ない場合（VRM 仕様: TANGENT はエクスポートしない）は MikkTSpace で自動生成。`IrVertex` に `tangent: Vec4`（xyz=方向, w=handedness）を追加し、GPU 頂点にも `tangent: [f32; 4]` を追加。シェーダーは UniVRM `MToon_GetTangentToWorld()` 準拠の TBN 構築に変更し、`tangent.w` を二値化（NaN 回避）。ミラー UV・tangent seam での法線マップ破綻を解消
- **`CullMode` enum 導入（VRM 0.x Front cull 対応）** — `is_double_sided: bool` を `CullMode` enum（`Back` / `None` / `Front`）に置換。VRM 0.x `_CullMode=1`（Front cull）を `doubleSided` にフォールバックせず、`wgpu::Face::Front` パイプラインで正確に再現。全レンダーキュー（Opaque / Mask / BlendZWrite / Blend）に Front cull パイプラインを追加。PMX エクスポートでは `Front` も `None` と同様に両面描画フラグ（0x01）を設定（PMX に Front cull 概念がないため）
- **`texCoord >= 2` の graceful degradation** — `read_texture_info()` で `texCoord > 1` の場合、テクスチャ無効化（`None`）ではなく `texCoord=0` へフォールバックし `warn` ログを出力するよう変更。テクスチャ UV は不正確になるが描画自体は維持される。設計根拠をソース・ドキュメントに記載（UniVRM 実装確認含む）

### バグ修正

- **GI 計算から direct light を分離（UniVRM 準拠）** — `passthrough_gi` に `light_intensity * max(dot(N, light_dir), 0)` が含まれており、direct light が direct 項と GI 項で二重加算されていた問題を修正。UniVRM では `indirectLight` は SH サンプリング結果（環境光のみ）であり、direct light は別系統で処理される。SH/IBL 非搭載のビューアでは `passthrough_gi = ambient` のみが正しい近似。`gi_equalized` の CPU 側計算（`CameraUniform`）も同様に ambient のみに修正。正面を向いた面の過剰な白飛びと `giEqualizationFactor` / `rimLightingMixFactor` の光量係数異常を解消。本体シェーダーとアウトライン共有関数の両方を修正
- **MASK 材質の AlphaToCoverage 後に alpha=1.0 を復帰（UniVRM 準拠）** — MASK 分岐で `fwidth` ベース A2C 計算後、`out_alpha = a2c_alpha` としていたため discard を通過したピクセルが半透明の中間値を持ち、egui オフスクリーン合成で cutout 材質の縁がにじむ問題を修正。UniVRM `vrmc_materials_mtoon_geometry_alpha.hlsl` は `clip()` 後に `return 1.0` で不透明に戻す。A2C はカバレッジ制御にのみ使い、最終 alpha は不透明に固定するよう修正。本体シェーダーとアウトライン共有関数の両方を修正
- **接線 `tangent.w` のミラー座標変換反転修正** — ビューアの座標変換（VRM 1.0: Z反転、VRM 0.0: X反転）は行列式 -1 のミラー変換であり、`cross(M*N, M*T) = -M*cross(N,T)` となるため bitangent の向きが反転する。`tangent.w` を反転して接空間の handedness を維持するよう修正。法線マップの凹凸方向が左右反転する問題を解消
- **MikkTSpace 接線生成の `normalTexture.texCoord` 対応** — `generate_tangents()` に `normal_tex_coord` 引数を追加し、`normalTexture.texCoord=1` の場合は UV1 で接線を生成するよう修正。VRM 材質からは `normalTexture` の `texCoord` を渡し、FBX/PMX/PMD は texCoord=0 を使用。法線マップが UV1 を参照するモデルで接線と法線マップの UV セットが不一致になる問題を解消
- **glTF sampler デフォルト `min_filter` を `LinearMipmapLinear` に修正** — `IrSamplerInfo::default()` の `min_filter` を `Linear`（mipmap なし）から `LinearMipmapLinear` に変更。UniVRM の `SamplerParam.Default`（Bilinear + EnableMipMap=true）および `TextureSamplerUtil` の `glFilter.NONE` → mipmap 有効のデフォルト挙動に準拠。sampler 未指定テクスチャの遠景・斜め視点でのチラつきを軽減
- **MToon ScreenCoordinates アウトラインのアスペクト補正修正** — `projected.x *= camera.aspect`（`width/height`）を `projected.x /= camera.aspect` に修正。UniVRM は `height/width` を乗算しており、従来の実装では横長ウィンドウでアウトラインの X 方向が過剰に膨張していた
- **MToon sRGB アウトラインの二重ガンマ補正除去** — sRGB 版 `fs_outline` の `pow(2.2)` を除去。MToon は線形空間で計算するため、sRGB レンダーターゲットの自動変換に任せるのが正しい。`pow(2.2)` が必要なのは MMD（ガンマ空間計算）のシェーダーのみ。アウトラインだけ暗く表示される問題を解消
- **UV1 不在時のフォールバック値修正** — GPU 側（`viewer/mesh.rs`）の UV1 不在時フォールバックを UV0 コピーからゼロ（`[0.0, 0.0]`）に変更。CPU 側（`resolve_cpu_uv`）と挙動を一致させ、UniVRM `MeshData.cs` 準拠のゼロフォールバックに統一した
- **VRM 0.x `outlineWidthTexture` の参照チャネル修正** — VRM 0.x の `_OutlineWidthTexture` は R チャネルを参照する（UniVRM `MToonCore.cginc:86` 準拠）が、VRM 1.0 の G チャネルで読み込んでいた。`IrMaterial` に `ColorChannel` enum を追加し、VRM 0.x=R / VRM 1.0=G を CPU 側（`sample_image_channel`）と GPU 側（WGSL `select_channel`）で動的に切り替えるよう修正
- **VRM 0.x `uvAnimationMaskTexture` の参照チャネル修正** — VRM 0.x の `_UvAnimMaskTexture` は R チャネルを参照する（UniVRM `MToonCore.cginc:129` 準拠）が、VRM 1.0 の B チャネルで読み込んでいた。同様に `ColorChannel` でバージョン別チャネル選択を実装
- **`texCoord=1` 共有材質の書き換え廃止** — UV1 を持たないメッシュが参照する材質の `texCoord=1` を一括で `texCoord=0` に書き換える処理を削除。同じ材質を共有する UV1 付きメッシュが巻き込まれる問題を解消。tangent 生成側のフォールバックを UV0 から zero UV に変更し、描画側（`mesh.rs`）と一致させた
- **MToon 本体パスの GI 半球補間が元の頂点法線を参照していた問題を修正** — GI の半球補間で `in.normal.y`（頂点法線）を使用していたため、normalMap 適用後の凹凸・`doubleSided` 背面反転が indirect lighting に反映されなかった。アウトラインパスは既に最終法線 `n.y` を使用しており、本体とアウトラインでシェーディングが不一致になっていた。本体パスも `n.y` に統一し、UniVRM の `MToon_SampleSH(normalWS)` 準拠に修正
- **`rimLightingMixFactor` が GI 均一化済みの値を使用していた問題を修正（UniVRM 準拠）** — リムライティングの光量係数 `light_factor` に `giEqualizationFactor` 適用後の `gi` が含まれていたため、GI 均一化を強くした材質ほどリムの光量まで平坦化されていた。UniVRM では `rimLightingMixFactor` に `unityLight.indirectLight`（未均一化 raw indirect）を使用する。`raw_indirect` と `gi`（equalized）を分離し、リムには `rim_light_factor = direct_light + raw_indirect` を使用するよう修正。本体パス・アウトラインパス両方を修正
- **テクスチャ差し替え時に `base_color_tex_info.index` が未同期だった問題を修正** — `assign_texture_to_material` / `assign_texture_data_to_material` で `texture_index` のみ更新し `base_color_tex_info.index` を同期していなかったため、GPU 描画は正しいが IR ベースの後続処理（エクスポートフィルタ・再読み込み）で古いテクスチャ参照が残るリスクがあった。同名材質連動割り当てパスも含め全4箇所を修正。`base_color_tex_info` が `None` の場合は `IrTextureInfo::from_index()` で新規作成
- **MikkTSpace 接線の handedness (w) 不一致による頂点分割** — `set_tangent_encoded()` の出力をコーナー単位（`face * 3 + vert`）で保持するよう変更。同一頂点を共有するコーナー間で `tangent.w`（handedness ±1）が異なる場合、少数派コーナーの頂点を自動分割し indices / morph targets / UV1 を連動更新。mirrored UV 境界で法線マップの凹凸方向がねじれる問題を解消。Seed-san.vrm では hair(70)/head(88)/wear(44) 計 202 頂点が分割される
- **Gram-Schmidt 再直交化後の退化 tangent 検出** — `extract.rs` のスキニング・非スキンメッシュ両経路で、Gram-Schmidt 後に `t_ortho` の長さが閾値未満または非有限値の場合は `Vec4::ZERO` にフォールバックし、MikkTSpace 再生成ルートへ流すよう修正。tangent が normal とほぼ平行なケース（非一様スケールや bad tangent）で退化 tangent `[0,0,0,w]` が有効と誤判定される問題を解消
- **tangent 有効判定を `length_squared` ベースに変更** — `generate_tangents()` の「既に有効な tangent を持つか」の判定を `v.tangent == Vec4::ZERO`（完全一致）から `v.tangent.truncate().length_squared() < 1e-8`（xyz 長さベース）に変更。w 成分が非ゼロの退化 tangent（`[0,0,0,1]` 等）も再生成対象になるよう修正
- **シェーダーのゼロ tangent ガード** — `apply_normal_map()` の冒頭で `dot(tangent.xyz, tangent.xyz) < 1e-6` をチェックし、退化 tangent では法線マップをスキップして基底法線を返す二重防御を追加。`normalize(vec3(0))` の WGSL 未定義動作を回避。本体・アウトライン両シェーダーに適用
- **GI 間接光の乗算先を `litColor` に修正（VRM 仕様準拠）** — GI（間接光）項が `base_color.rgb * gi` と baseColor を使用していたため、間接光下でトゥーン境界が崩れていた。VRM 1.0 仕様では `giLighting = gi(n) * litColor` と定義されており、UniVRM も `input.litColor * lerp(indirectLight, indirectLightEqualized, _GiEqualization)` で litColor を使用する。`lit * gi`（本体）/ `toon_color * gi`（アウトライン）に修正し、日陰・逆光・弱照明でも陰色のトゥーン境界が維持されるようになった

### 実装詳細

- **法線マップ（ノーマルマップ）対応** — glTF `normalTexture` をシェーディングに反映。頂点接線（`tangent: Vec4`）から TBN 行列を構築し、tangent-space 法線をワールド空間に変換（UniVRM `MToon_GetTangentToWorld()` 準拠）。glTF `TANGENT` 属性がなければ `mikktspace` crate で MikkTSpace 接線を自動生成（VRM 仕様準拠）。MToon・非 MToon 両方で適用。`normalTexture.scale` による強度制御、`texCoord` / `KHR_texture_transform` / UV Animation にも対応。法線マップなしの材質にはフラット法線テクスチャ（RGB=(0.5, 0.5, 1.0)）を自動バインド
- **`alphaMode` シェーダー処理** — `alpha_cutoff` フィールドに alphaMode を sentinel 値でエンコード（`-1.0`=OPAQUE, `-0.5`=BLEND, `>=0.0`=MASK cutoff）。OPAQUE は出力アルファ 1.0 固定、MASK は UniVRM 準拠の `fwidth` ベース AlphaToCoverage 計算で cutoff 境界を平滑化（パイプラインは blend なし）、BLEND は完全透明ピクセル `discard`。アウトラインパスにも同一のアルファ処理を適用
- **`outlineLightingMixFactor` UniVRM 完全準拠** — 本体と同等の MToon ライティング計算を `compute_mtoon_surface_lighting()` 関数として共有。アウトライン色は UniVRM と同一の `outlineColor * lerp(1, baseCol, mix)` で合成
- **glTF テクスチャ index の image index 正規化** — `read_texture_info()` で glTF texture index を `document.textures().nth(i).source().index()` により image index に変換。`textures[]` と `images[]` の並びが異なる glTF/VRM で正しい画像を参照
- **`outlineWidthMultiplyTexture` GPU 専用サンプリング** — アウトライン頂点シェーダーで GPU サンプリング結果のみ使用（CPU 側 `edge_scale` は PMX エクスポート用に維持）。CPU 側 `resolve_cpu_uv()` で GPU と同一の texCoord 選択 + KHR_texture_transform を適用
- **`doubleSided` 背面法線反転（UniVRM 準拠）** — `@builtin(front_facing)` で背面法線を反転し、法線マップ適用前に処理。UniVRM の `MTOON_IS_FRONT_VFACE` と同等。全シェーダーバリアントに適用
- **UV アニメーション回転角精度** — UniVRM 準拠で `fract(turns) * 2π` による角度ラップを実装し、長時間再生時の浮動小数点精度低下を防止
- **制限事項: テクスチャ UV セットは `TEXCOORD_0` / `TEXCOORD_1` のみ対応** — glTF 仕様では任意数の UV セットを許容するが、VRM/MToon で使用する UV は UV0/UV1 の 2 系統のみ（UniVRM 実装確認済み）。`texCoord >= 2` のテクスチャは `texCoord=0` にフォールバックされる（`warn` ログ出力）
- **VRM 0.x MatCap（`_SphereAdd`）への `_MainTex` ST 非適用修正** — `resolve_tex()` ヘルパーが全テクスチャに一律 `_MainTex` ST を適用していた問題を修正。MatCap は `inherit_st=false` で ST 伝播を除外するよう変更（UniVRM `MigrationMToonMaterial.cs:255-260` 準拠: "Texture transform is not required"）。将来 MatCap の UV パラメータを使用した際に VRM 0.x で MatCap が誤変換される潜在バグを解消
- **`base_color_tex_info` の merge / export_filter 同期漏れ修正** — `IrModel::merge()` と `build_filtered_ir()` で `base_color_tex_info` のテクスチャ index がオフセット・リマップされていなかった問題を修正。merge 時の `offset_index()` と export_filter 時の `remap_index()` を追加し、`texture_index` との不整合を解消
- **export_filter の `sphere_texture_index` / `toon_texture_index` pruning 漏れ修正** — `used_tex_indices` の収集とリマップに `sphere_texture_index` / `toon_texture_index` が含まれていなかったため、エクスポートフィルタ適用後にこれらのテクスチャ参照が壊れる可能性があった問題を修正
- **`doubleSided` MToon 材質の背面法線反転（UniVRM 準拠）** — `fs_main` / `fs_outline`（sRGB / Unorm 両版）に `@builtin(front_facing)` を追加し、背面フラグメントの法線を法線マップ適用前に反転。UniVRM の `MTOON_IS_FRONT_VFACE(facing, normalWS, -normalWS)` と同等。髪カード・まつげ・薄い布等の `doubleSided` 材質で陰影・リム・MatCap・法線マップの方向が Unity と一致するようになった
- **法線マップ TBN 構築の退化 UV フォールバック** — `apply_normal_map()` で `det ≈ 0`（ゼロ面積 UV / 同一点 UV / 極端に細い三角形）や tangent/bitangent がゼロベクトル近傍の場合に基底法線にフォールバックするよう修正。`normalize(vec3(0))` の WGSL 未定義動作を回避し、法線マップ由来のちらつき・色飛びを防止。本体・アウトライン両シェーダーに適用
- **`read_texture_info()` の `None` 時先行設定クリア** — `read_texture_info()` が `None` を返した場合（テクスチャ未参照等）、core glTF API で先に設定された `texture_index` / `emissive_texture` / `normal_texture` がクリアされず UV0 で誤描画される問題を修正。raw JSON の判定結果を authoritative とし、`None` 時は先行設定を明示的にクリアするよう変更
- **法線マップ TBN 構築の bitangent 符号二重反転修正** — `apply_normal_map()` の screen-space derivative TBN 構築で、`inv_det = 1.0 / det` に既に含まれる handedness 符号に対し、さらに `sign(det)` を bitangent に乗算していた問題を修正。mirrored UV アイランドで法線マップの陰影方向が崩れる問題を解消。本体・アウトライン両シェーダーを修正
- **UV アニメーション回転角の長時間稼働時精度劣化防止** — `apply_uv_anim_core()` で `camera.time * rotation_speed` をそのまま `sin/cos` に渡していたため、長時間稼働時に float 精度が低下し UV 回転がジッターする問題を修正。UniVRM 準拠で `fract(turns) * 2π` により角度を周期内に折り返すよう変更。本体・アウトライン両シェーダーを修正
- **VRM 0.x `_OutlineWidthTexture` への `_MainTex` ST 伝播漏れ修正** — `_OutlineWidthTexture` が `resolve_tex()` ヘルパー定義前に `IrTextureInfo::from_index()` で直接設定されていたため、`_MainTex` の tiling/offset（ST）が伝播されていなかった問題を修正。`resolve_tex()` 経由に統一し、他の MToon テクスチャと同様に ST を適用（UniVRM `MigrationMToonMaterial.cs` 準拠）。CPU 側 `edge_scale` 計算にも影響するため、PMX 出力のエッジ倍率も修正される
- **UV1 不在時のゼロフォールバック** — `texCoord=1` を要求するテクスチャに対し、メッシュに `TEXCOORD_1` が存在しないとき `[0.0, 0.0]` をフォールバック値として使用（UniVRM `MeshData.cs` 準拠）。GPU 側・CPU 側で統一
- **スキニング/法線再計算後の TBN 同期** — アニメーション再生時のスキニング処理で法線のみ変換し接線（tangent）が未更新だった問題を修正。スキニング行列で tangent.xyz を変換後、Gram-Schmidt 再直交化で法線に対する直交性を維持。法線平滑化（`smooth_normals`）/カスタム法線クリア（`clear_custom_normals`）後も同様に tangent を再直交化。法線マップ付き材質でアニメーション中や法線再計算時に陰影・リム・ハイライトの方向がずれる問題を解消
- **VRM 0.x `_MainTex` が raw JSON `baseColorTexture` で上書きされる問題を修正** — VRM 0.x MToon で `materialProperties._MainTex` を authoritative source として設定した後、glTF core の `pbrMetallicRoughness.baseColorTexture` が無条件に再適用されて `_MainTex` の設定を上書きしていた問題を修正。`_MainTex` 解決済みフラグ（`v0_main_tex_resolved`）を導入し、VRM 0.x MToon で `_MainTex` が設定済みの場合は raw JSON 反映をスキップするよう変更
- **法線平滑化 + 法線マップ併用時の警告追加** — `smooth_normals` ON かつ法線マップ付き材質が含まれる場合に `warn` ログを出力するよう追加。法線平滑化は `PosUvKey`（位置+UV）のみで頂点を統合するため、UV seam 境界で tangent basis が不正確になる可能性がある（MikkTSpace 再生成が理想だがリアルタイム操作にはコスト大）
- **shade 色合成式を仕様・UniVRM 準拠に修正** — `shade = base_color.rgb * shade_color * shade_mul` を `shade = shade_color * shade_mul` に修正。VRM 1.0 仕様の擬似コードでは `shadeColorTerm = shadeColorFactor * texture(shadeMultiplyTexture)` であり、`baseColorFactor * baseColorTexture` は lit 側にのみ適用される。従来は陰色が `baseColor` に二重従属し、影が過剰に暗くなっていた。本体シェーダーとアウトラインシェーダーの両方を修正
- **正射影時の view direction を UniVRM 準拠に修正** — 正射影カメラ時の view direction を `normalize(camera_pos - world_pos)` から `normalize(camera_forward)` に変更。`CameraUniform` に `is_perspective` と `camera_forward` を追加。透視投影時は従来通り。MToon リムライティング・MatCap・MMD スペキュラの3箇所を修正。UniVRM `MToon_GetWorldSpaceNormalizedViewDir()` 準拠
- **法線平滑化・カスタム法線クリアのビルド層強制無効化** — `build_gpu_model` / `build_gpu_model_from_ir` の入口で法線マップ付き材質の有無を判定し、`smooth_normals` / `clear_custom_normals` を強制 `false` にフォールバック。UI 側の無効化に加えてビルド層でも二重に防御し、CLI・テスト・ベンチ等の UI 非経由呼び出し経路でも不変条件を保証
- **法線平滑化を法線マップ付き材質で UI 無効化** — `normal_texture` を持つ材質が含まれる場合、法線平滑化チェックボックスをグレーアウトし、UV seam 境界での tangent basis 破綻を防止。ホバーテキストで理由を表示
- **MatCap UV 基底の X 軸反転修正（UniVRM 準拠）** — MatCap UV 算出の `world_view_x` が UniVRM と符号逆（`(v.z, 0, -v.x)` → `(-v.z, 0, v.x)`）で、`world_view_y` の cross 積順も不一致だった問題を修正。`right = cross(viewDir, worldUp)`, `up = cross(right, viewDir)` に統一（UniVRM `vrmc_materials_mtoon_lighting_mtoon.hlsl` 準拠）。非対称 MatCap テクスチャの左右ミラーを解消。本体シェーダーとアウトラインシェーダーの両方を修正
- **カスタム法線クリアを法線マップ付き材質で UI 無効化** — `normal_texture` を持つ材質が含まれる場合、法線平滑化に加えカスタム法線クリアのチェックボックスもグレーアウトするよう修正。`recalculate_normals_from_geometry` 後の Gram-Schmidt 再直交化では UV seam 境界で tangent basis が不正確になる問題は `smooth_normals` と同一
- **glTF tangent 初期ロード時の Gram-Schmidt 再直交化追加** — `extract.rs` のスキニング・非スキンメッシュ両経路で、tangent 変換後に `t_ortho = (t - n * dot(n, t)).normalize()` による再直交化を追加。`animation.rs` のスキニング更新パスでは既に実装済みだったが、初期ロード経路では未実施だった。非一様スケールを含むスキン行列で法線と接線の直交性が崩れる問題を解消
- **`texCoord=1` かつ TEXCOORD_1 不在時のフォールバック統一** — extract 完了後に全メッシュの UV1 有無を確認し、UV1 が存在しない場合は全材質テクスチャの `tex_coord=1` を `tex_coord=0` に正規化するステップを追加。tangent 生成（UV0 フォールバック）と描画（zero フォールバック）で UV セットが乖離する問題を根本解消
- **`texCoord=1` フォールバックをメッシュ単位判定に変更** — UV1 フォールバックの判定をモデル全体（`any_mesh_has_uv1`）からメッシュ単位に変更。UV1 を持たないメッシュが参照する材質のみ `texCoord=1` → `texCoord=0` に正規化し、一部メッシュだけ UV1 を持つモデルでも正しく動作するよう改善。`base_color_tex_info` もフォールバック対象に追加
- **テクスチャ差し替え時の per-texture sampler 維持** — UI からテクスチャを差し替える際、`default_sampler`（Linear + Repeat 固定）ではなく材質の `IrSamplerInfo` を使用してサンプラーを再生成するよう修正。`ClampToEdge` / `MirroredRepeat` / `Nearest` 等のテクスチャ固有サンプラー設定が差し替え後も維持される。同名材質連動・パッケージテクスチャ割り当ての両経路で修正
- **VRM 0.x `_MainTex` 採用時の `source_texture_name` 同期** — VRM 0.x MToon の `_MainTex` を authoritative source として `texture_index` / `base_color_tex_info` を上書きする際、`source_texture_name` も同一テクスチャ元から再取得するよう修正。UnityPackage テクスチャ自動割り当て（`embed_textures_into_ir`）で glTF core 側のテクスチャ名が残り、`_MainTex` 側と一致しない問題を解消
- **MToon `dot(N,L)` の光方向符号修正** — `camera.light_dir`（光の進行方向: 光源→表面）を MToon / 非MToon の `dot(N,L)` でそのまま使用していた問題を修正。`dot(n, -camera.light_dir)` に変更し、仕様の「表面→光源方向」に統一。MMD シェーダーは既に `-camera.light_dir` で正しく反転していた。toon shading の lit/shade 境界と Half-Lambert ライティングの方向が正しくなり、正面が影になる問題を解消。本体・アウトライン・非 MToon の3箇所を修正
- **`matcapTexture` の `KHR_texture_transform` 適用** — `matcapTexture` は `read_texture_info()` で `texCoord` / `offset` / `scale` / `rotation` を抽出済みだが、シェーダーでは生の matcap UV をそのまま使用していた問題を修正。`MaterialUniform` に `matcap_uv_a` / `matcap_uv_b` を追加し、`apply_texture_transform()` を適用。本体・アウトライン両シェーダーを修正
- **ライトカラー対応** — `CameraUniform` に `light_color: vec3<f32>` を追加し、direct light を `light_intensity * light_color` で計算するよう変更。UI にカラーピッカーを追加。暖色・冷色の照明表現が可能に
- **半球 ambient（Sky/Ground 2色補間）** — 一様な灰色 ambient を法線Y成分による Sky/Ground 2色補間に変更（`mix(ground, sky, normal.y * 0.5 + 0.5)`）。SH9 の L1 成分（上下明暗差）を近似し、VRoidHub / UniVRM の `SampleSH(normal)` に近い環境光を実現。`gi_equalized` も `(sky + ground) / 2` に更新（UniVRM `(SH(up) + SH(down)) / 2` 準拠）。UI に Sky/Ground 各色のカラーピッカーを追加
- **デフォルトライトモードを固定に変更** — `LightMode::CameraFollow` から `LightMode::Fixed` に変更。VRoidHub と同じ固定ディレクショナルライト環境がデフォルトに
- **`KHR_materials_emissive_strength` 対応** — glTF の `emissiveFactor` は [0,1] 範囲に制限されるため、HDR emissive は `KHR_materials_emissive_strength` 拡張の `emissiveStrength` で倍率を指定する。UniVRM は `maxComponent > 1.0` 時にこの拡張を書き出すが、読み取り側で未対応だった。`extract.rs` で `emissiveStrength` を読み取り `emissive_factor` に乗算するよう修正

### コード品質・パフォーマンス改善

- **半透明ソート用作業バッファ再利用** — `render_to_texture` 内で毎フレーム `Vec<Vec3>`（重心）と `Vec<usize>`（ソート済みインデックス）をアロケーションしていた問題を修正。`GpuRenderer` に `work_draw_centers` / `work_sorted_indices` を作業バッファとして追加し、`std::mem::take` + 返却パターンで容量を維持しつつ借用衝突を回避
- **半透明 DrawCall 重心の均等サンプリング** — 半透明ソート用の重心計算を全インデックス走査から均等間隔サンプリング（最大30点）に変更。30 index 以下は全走査、それ以上は `total / 30` ステップで均等にサンプリング。髪・スカート等の広がったメッシュでも空間的に代表性の高い重心を算出しつつ計算量を O(k) に抑制
- **モーフ循環検出バッファ再利用** — `apply_gpu_morph_to` がモーフごとに `vec![false; N]` をアロケーションしていた問題を修正。`GpuModel` に `morph_visited: Vec<bool>` を追加し、`clear()` + `resize()` で再利用。`apply_gpu_morph_to` 関数を廃止し、呼び出し元が直接 `apply_gpu_morph_recursive` を使用
- **`morph_work` / `animated_vertices` の swap 統合** — `apply_morphs` 内で `morph_work` → `animated_vertices` への `extend_from_slice` / `clone`（~1.9MB/フレーム）を `std::mem::swap` に置き換え。GPU 書き込みも swap 後の `animated_vertices` を参照するよう変更し、頂点バッファの冗長コピーを回避
- **テクスチャ書き出しの clone 回避** — `convert/texture.rs` の `ImageBuffer::from_raw(w, h, tex.data.clone())` を `image::save_buffer(&out_path, &tex.data, ...)` に変更し、最大 64MB（4K RGBA）のデータクローンを完全に回避。`ImageBuffer` import を除去
- **`convert_fbx_to_pmx` の `normalize_pose` 修正** — 公開 API `convert_fbx_to_pmx` が `options.normalize_pose` を `extract_ir_model_from_fbx` に渡していなかった問題を修正。`extract_ir_model_from_fbx_with_options` に切り替え
- **`unsafe` ブロックの SAFETY コメント追加** — `main.rs`（`attach_parent_console` / `detach_console`）と `viewer/single_instance.rs`（全 Win32 API 呼び出し）の `unsafe` ブロック全箇所に `// SAFETY:` コメントを追加
- **`IrMaterial` の MToon フィールド分離** — `IrMaterial` の 25 個の MToon 固有フィールドを新しい `MtoonParams` 構造体に移動し、`mtoon: Option<MtoonParams>` で保持。フィールド数を 35+ → 約 18 に削減。`is_mtoon()` / `mtoon()` / `mtoon_mut()` ヘルパーメソッドを追加（非 MToon 時は静的デフォルト値 `MTOON_DEFAULT` を返却）
- **`viewer/app.rs` サブモジュール分割** — `app.rs` を責務ごとに 5 サブモジュールに分割: `mod.rs`（構造体定義・初期化・eframe::App impl）、`file_io.rs`（ファイル読み込み・D&D・リロード）、`texture_mgmt.rs`（テクスチャ割り当て・プレビュー）、`pending.rs`（遅延タスク処理）、`helpers.rs`（ユーティリティ型・関数）。外部 API は `pub use` で互換性維持
- **`anyhow` → `PoponeError` 統一** — ライブラリ内部モジュール 19 ファイルで `anyhow::Result` → `crate::error::Result` に移行。`PoponeError` に 7 新バリアント（`FbxParse` / `PmxParse` / `PmdParse` / `Build` / `Archive` / `UnityPackage` / `Other`）を追加。`ResultExt` トレイト（`.context()` / `.with_context()` 互換）を追加。`main.rs` / `viewer/` は `anyhow` のまま維持
- **非 MToon 材質の `render_queue_offset` 誤設定防止** — VRM 0.x `remap_vrm0_render_queue_offsets` で `mat.mtoon_mut()` を全材質に呼んでいたため、非 MToon 材質に `mtoon: Some(Default)` が生成され `is_mtoon()` が `true` になる問題を修正。`if let Some(ref mut mtoon) = mat.mtoon` に変更し MToon 材質のみに制限

## v0.2.8

### 新機能

- **シングルインスタンス** — ビューアが既に起動している状態で再度起動すると、ファイルパスを既存ウィンドウに転送して自動終了する（Windows Named Mutex + Named Pipe IPC）。最小化状態からも復帰する
- **FPS 精度改善** — 指数移動平均（EMA）からフレームカウント方式（直近1秒の実フレーム数）に変更。平均フレームタイム（ms）も併せて表示

### 改善

- **ログ保全** — シングルインスタンス判定をログ初期化前に実行し、2番目のプロセスによる不要なログファイル生成・ローテーションを防止。IPC 失敗時のフォールバック起動でもログローテーションをスキップ
- **IPC エラーハンドリング** — WriteFile 失敗・短書き込み時は FallbackStart に切り替え（ファイルオープン要求のサイレント消失防止）。ReadFile エラー（ERROR_MORE_DATA 等）と空メッセージを区別して処理

## v0.2.7

### 新機能

- **PMX 出力オプション追加** — ビューアの出力タブと CLI に以下のオプションを追加。`PmxBuildOptions` 構造体を導入し、ビルド時オプションを統合管理
  - **物理なしで出力** (`--no-physics`): 剛体・ジョイントを出力から除外。ビューアでは物理可視化を維持したままエクスポート時のみスキップ
  - **元のボーン構造で出力** (`--raw-structure`): MMD 標準ボーン（全ての親・センター・グルーブ・腰・IK・捩り等）の挿入をスキップし、VRM/FBX の元のボーン名をそのまま PMX に出力。`IrBone` に `original_name` フィールドを追加し、FBX の元ノード名（humanoid 検出による PMX 名変換前の名前）を保持
- **アプリアイコン** — ウィンドウタイトルバーと exe ファイルの両方にアイコンを表示
- **グリッド Y 軸線** — グリッド床に Y 軸（上方向）の緑色ガイドラインを追加

### バグ修正

- **PMD 頂点 edge_flag 修正** — PMD 頂点の `edge_flag` 解釈を修正
- **PMX グループモーフのインデックスずれ修正** — PMX ロード時にボーン/材質/UV モーフをスキップすると、グループモーフ内のサブモーフ参照インデックスがずれて不正な変形になっていたバグを修正。PMX → IrModel 変換時にインデックスリマッピングテーブルを構築し、スキップされたモーフを正しく除外するようになった
- **ビューア スタックオーバーフロー修正** — Windows のデフォルトスタックサイズ（1MB）では eframe/winit/wgpu のコールバックチェーンが深くスタックが溢れる場合があった問題を修正。`build.rs` で viewer feature 有効時にスタックサイズを 8MB に拡大（`/STACK:8388608`）。また、グループモーフの再帰展開に深度制限（最大 16）を追加し、循環参照による無限再帰を防止

### 改善

- **テクスチャ手動割当の検索フィルタ移動** — アーカイブ（UnityPackage / ZIP 等）からのテクスチャ手動割当ダイアログで、検索フィルタをダイアログ上部から各テクスチャプルダウン内に移動。プルダウンを開くと「(なし)」→ 検索フィルタ → テクスチャ一覧の順で表示される。素材パネルのテクスチャ割当ポップアップと同じ操作感に統一

### コード品質改善

- **公開 API 統合** — `convert_vrm_to_pmx` の 3 段ラッパー関数チェーンを `VrmConvertOptions` 構造体 + 1 関数に統合。新オプション追加時の関数増殖を防止
- **`no_physics` 適用箇所統一** — `main.rs` での `ir.physics` 直接クリアを削除し、`PmxBuildOptions` 経由の制御に一本化
- **グループモーフ循環参照対策** — 深度制限のみのガードを visited ビットセット（バックトラック方式）によるサイクル検出に改善。循環参照を O(N) で検出
- **`raw_structure` 時の付与データ保持** — 元のボーン構造で出力する際に、PMX の付与親（grant）・移動可能・軸固定・表示フラグを IrBone から正しく復元するよう改善。PMX → PMX ラウンドトリップでのデータ損失を防止
- **build.rs クロスコンパイル対応** — `winres` を `[target.'cfg(windows)'.build-dependencies]` に限定。スタックサイズ設定を MSVC (`/STACK`) / GNU (`-Wl,--stack`) で分岐
- **座標変換関数の重複解消** — `pmx_pos_to_gltf` / `pmx_normal_to_gltf` を `convert/coord.rs` に統合し、`pmd/extract.rs` と `pmx/extract.rs` の重複定義を解消
- **アイコン PNG 最適化** — ウィンドウアイコン用 PNG を 512×512 (99KB) → 64×64 (4KB) に縮小
- **エラーハンドリング改善** — アイコン読み込みの `expect` パニックを `?` 演算子によるエラー伝播に変更
- **グループモーフ警告ログ** — PMX ロード時のサブモーフスキップ、およびビューアでの範囲外サブインデックスを `log::warn` で報告
- **収束ループ安全化** — エクスポートフィルタのグループモーフ有効性判定ループにモーフ数上限ガードを追加

## v0.2.6

### バグ修正

- **剛体・ジョイント Euler 回転順序修正** — 剛体・ジョイントの Euler 分解・再構成を `ZXY`（内的 ZXY = 外的 YXZ）から `YXZ`（内的 YXZ = 外的 ZXY）に修正。D3DX 行優先規約 `v * Ry * Rx * Rz` に準拠（glam 列優先では `Rz * Rx * Ry`）。球体・カプセルでは目立たないが、ボックス剛体で回転の不一致が顕著だった。変換出力（`convert/physics.rs`）とビューア描画（`gpu.rs`）の両方を修正
- **PMD/PMX 剛体 bone_index フォールバック** — PMD の `bone_index=0xFFFF`（関連ボーンなし）および PMX の `bone_index=-1` の剛体をボーン 0（センター）に追従させるよう修正。従来は `None` となり位置計算の基点がなかった
- **ジョイント接続線の表示分離** — `generate_spring_bone_vertices`（物理表示(P)トグル）に含まれていたジョイント接続線（黄色い線）を削除。ジョイント接続線は既に `generate_joint_vertices` で独立描画されており、ジョイント表示トグルで制御される
- **MMD 描画順序修正** — 不透明/半透明で分離していた描画ループを材質インデックス順の単一ループに統合。PMX/PMD の材質順序（モデル作者が意図した前後関係）を正しく維持するようになった。エッジも各不透明材質の直後に描画
- **MMD 半透明デプス書き込み有効化** — MMD 半透明パイプラインのデプス書き込みを有効化（MMD 準拠）。材質順描画との組み合わせで、alpha=0.99 等の「実質不透明」材質が後続材質を正しく遮蔽
- **PMD カスタムトゥーンテクスチャ修正** — `build_tex_map()` がカスタムトゥーンテクスチャのインデックスを登録していなかったバグを修正。`extract_textures()` の結果からマッピングを構築するよう変更し、モデル同梱のトゥーンテクスチャが正しく参照されるようになった（共有トゥーンへの誤フォールバックを解消）
- **PMX/PMD 剛体アニメーション追従修正** — VRMA アニメーション再生時に PMX/PMD モデルの剛体・ジョイントがボーンに正しく追従しなかったバグを修正。原因は `bone.position`（glTF 空間に変換済み）と `rb.position`（PMX 空間のまま）の座標空間不整合。PMX/PMD の `pmx_pos_to_gltf` は VRM 1.0 と同じ Z 反転変換のため、剛体追従のデルタ計算で VRM 1.0 と同じ `gltf_pos_to_pmx` 変換と回転デルタの Z-flip を適用するよう修正
- **FBX ヒューマノイドボーン検出改善** — Blender リグの CamelCase ボーン名（`UpperLeg.L` → `upperleg_l`）が `upper_leg_l` パターンにマッチしなかった問題を修正。アンダースコアなしの代替パターン（`upperleg_l` / `lowerleg_l` / `upperarm_l` / `lowerarm_l`）、つま先の単数形（`toe_l` / `toe_r`）、指ボーン逆順パターン（`index_proximal_l` 等）、pinky エイリアスを追加。Unity FBX エクスポートの名前空間プレフィックス（`Model::Hips` 等）を `strip_namespace_lower()` で除去し、リグ検出・パターンマッチに反映
- **UnityPackage テクスチャ MIME タイプ修正** — UnityPackage 経由で読み込んだ FBX モデルのテクスチャが全てマゼンタ（1x1 ピンク）になるバグを修正。`embed_textures_into_ir` で IrTexture を作成する際に `mime_type` が空文字列になっており、TGA 等マジックナンバーのないフォーマットで `image::load_from_memory` の自動判定が失敗していた。ファイル拡張子から MIME タイプを設定するよう修正。併せて `decode_image_to_rgba_with_hint` の TGA MIME マッチに `"image/x-tga"` を追加（`mime_for_ext` が返す値との不一致を解消）

### 新機能

- **PMX 付与（grant）アニメーション対応** — PMX ボーンの回転付与・移動付与をアニメーション再生時に処理するようになった。Tda 式初音ミク等の D-bones（足D・ひざD 等）は FK ボーンの回転を付与でコピーする仕組みだが、この処理が未実装だったため VRMA アニメーション時に足が追従しなかった。`IrBone` に `IrGrant`（付与親・付与率・回転/移動/ローカルフラグ）を追加し、PMX 読み込み時に付与データを抽出。アニメーション計算後、ボーンインデックス順に付与デルタを適用しグローバル行列を再計算する 2 フェーズ方式で実装。ローカル付与（`is_local`）は子ボーンのレスト姿勢を基準にデルタを適用。付与処理順序はトポロジカルソート（カーン法 BFS）で事前計算し、不正な PMX ファイルでも正しい依存順序を保証
- **ボーン表示改善** — PMX/PMD のボーンをフラグに基づき形状別に描画。通常=◎（二重円＋中心塗り）、移動=◻（正方形＋中心塗り）、軸制限=⊗（円＋✕）、IKコントローラ=◻（青枠＋オレンジ塗り＋青中心）。IK影響下ボーン（Link）はオレンジ表示。テイルベース描画（self→tail）により PMXEditor と同様のボーン方向を表示。TriangleList による完全塗りつぶし、3段階パイプライン（テール→塗り面→外枠線）、4パス優先描画（通常→IK影響下→軸制限→IKコントローラ）

- **FBX Tスタンス変換** — FBX モデルの A→T スタンス変換に対応。ビューアでは FBX 読み込み時に「Tスタンス変換」チェックボックスが表示される（Aスタンス変換と排他）。CLI では `--normalize-to-tstance` オプションで使用可能
- **MMD レンダリングモード** — PMX/PMD ロード時に自動 ON。MMD 固有のトゥーンシェーディング、Blinn-Phong スペキュラ、スフィアマップ（乗算/加算）で表示
- **エッジ（輪郭線）描画** — inverted hull 法による輪郭線。材質ごとのエッジ色・太さ、距離減衰、UI からの ON/OFF・太さスライダー（0.1〜3.0）
- **共有トゥーンテクスチャ** — MMD 標準 toon01〜toon10 のグラデーションを CPU で生成。個別トゥーンテクスチャにも対応
- **スフィアマップ** — PMX の sphere_mode（乗算/加算）、PMD の .sph/.spa ファイルに対応。ビュー空間法線からスフィア UV を算出
- **色空間再現** — MMD のガンマ空間レンダリングを再現。PMX/PMD 専用フレームでは `Rgba8Unorm` レンダーターゲットに切り替え、ガンマ空間での正確なアルファブレンドを実現。VRM 混在時は `Rgba8UnormSrgb` にフォールバック
- **PMD スフィア/トゥーン抽出** — `parse_pmd_texture_slots` で `*` 区切りのメイン/スフィアテクスチャを分離。トゥーンテクスチャのファイル存在確認付き登録

### 改善

- **剛体表示修正** — PMD/PMX の剛体回転から不要な X 反転補正（`adjust_pmd_rigid_rotation` / `adjust_pmx_rigid_rotation`）を削除。PMX/PMD モデルの座標は既に PMX 空間にあるため、ビューア描画時の glTF→PMX 座標変換をスキップ。Box 剛体のサイズを half-extent として正しく扱うよう修正（従来の `* 0.5` による二重除算を解消）。カプセル剛体に半球ワイヤーフレーム（4 経線 + 3 緯線 × 上下）を追加し PMXEditor 準拠の表示に改善
- **剛体 physics_mode 色分け** — PMX/PMD モデルの剛体表示を `physics_mode` で色分け（0:ボーン追従=グリーン、1:物理演算=レッド、2:物理+ボーン=ブルー）。VRM は従来通り group ベース（コライダー=レッド、スプリング=グリーン）
- **オーバーレイ描画順序変更** — 可視化オーバーレイの描画順を「法線 → ボーン → 剛体 → ジョイント」に変更（ジョイントが最前面）。メッシュ表面の法線は最背面に、接続関係を示すジョイントを最前面に配置し視認性を改善
- **MMD ライティング見直し** — トゥーン乗算方式に移行（lit/shadow lerp 廃止）。`base_color = saturate(diffuse × LightAmbient + ambient)` で D3D ambient/emissive マッピングを修正。スペキュラはトゥーン適用後に独立加算（影領域でもハイライト維持）。LightAmbient = 154/255 ≈ 0.604、LightSpecular も同値に統一
- **トゥーンサンプリング NdotL 依存化** — 固定 UV `(0.5, 0.85)` から `(0, 0.5 − NdotL × 0.5)` に変更し、法線とライト方向に応じた陰影グラデーションを再現
- **共有トゥーンテクスチャ実データ化** — 推定グラデーション（256×16）を MMD 標準 toon01-10 の実ピクセルデータ（1×32、32行RGB値）に置換。toon01-04: 2色ステップ、toon05: 暖ピンクグラデーション、toon06: 黄色+ハイライトバンド、toon07-10: 全白
- **スフィア UV X 反転** — X 反転座標系に対応し `vn_x × -0.5 + 0.5` に修正。スフィアマップ反映は RGB のみ（アルファ不正対策）
- **PMD エッジフラグ修正** — `edge_flag` の解釈を `0=有効` から `1=エッジあり` に修正
- **PMX トゥーン未設定対応** — `PmxToonRef::Texture(-1)` を `(None, None)` として処理し、トゥーンなしを正しく扱うように修正
- **カメラ・ライティング MMD 準拠** — FOV 45° → 30°（MMD 標準）、ライト方向を MMD 準拠に変更（固定: (-0.5,-1.0,0.5) の反転、カメラ追従: MMD 風左上寄り）。ライト強度 0.6、環境光 0.5 に調整
- **視点依存フィット** — バウンディングボックスのフィット計算を視点依存に改善。bbox 8 頂点をカメラ軸に投影し、幅・高さ・奥行きの全方向で frustum に収まる距離を算出。アスペクト比・透視/正射影の両方に対応
- **Shift 精密操作** — Shift キーを押しながらのカメラ操作で 1/3 速度の精密モード（回転・パン・ズーム全対応）
- **ダブルクリックフィット** — ビューポートのダブルクリックでモデルにフィット
- **MMD ambient 分離** — MMD レンダリング時の環境光を標準パスから分離。CameraUniform の `mmd_ambient_scale` で制御し、MMD モード切替が標準材質の明るさに影響しなくなった
- **IrMaterial 拡張** — `source_format`、`sphere_texture_index`、`sphere_mode`、`toon_texture_index`、`toon_shared_index` フィールド追加。merge 時の index remap 対応
- **テクスチャデュアルビュー** — GPU テクスチャを `Rgba8UnormSrgb`（標準）と `Rgba8Unorm`（MMD）の 2 ビューで管理。メモリ増加なし
- **ワイヤーフレーム共存** — MMD モード ON でも Wire / S+W / 法線マップ表示時は既存パイプラインにフォールバック

### コード品質・パフォーマンス改善

- **アニメーション逆行列キャッシュ** — レストポーズのボーングローバル逆行列を `SkinningData` 構築時にキャッシュ。毎フレーム175ボーン分の `Mat4::inverse()` 計算を排除
- **WGSL シェーダー共通化** — `CameraUniform`（8重複）・`MmdMaterialUniform`（4重複）の struct 定義を `macro_rules!` + `concat!` で一元管理。sRGB/Unorm 版の MMD メインシェーダーを `compute_mmd_lighting` 関数で共通化し、差分をフラグメントシェーダー1関数に局所化
- **重複コード関数化** — `build_pkg_model_list`（unitypackageモデルリスト構築×3）、`load_animation_file`（アニメーション読込ルーティング×2）、`mime_for_ext`（MIMEタイプ判定×4）を共通関数に抽出
- **`to_string_lossy()` 改善** — 7ファイル18箇所の `.to_string_lossy().to_string()` を `.to_string_lossy().into_owned()` に変更。UTF-8 互換パスでの不要なアロケーションを回避
- **`is_psd_filename` 最適化** — `to_lowercase()` による String アロケーションを `eq_ignore_ascii_case` に置換
- **`update_mat_cache` 簡素化** — NLL で不要な二重 `if let` 借用を除去
- **PMX リーダー安全性強化** — 全14箇所の `i32 as usize` カウントキャストに負値チェックを追加（`checked_count` ヘルパー）。破損ファイルでの OOM パニックを防止。`Cursor` に不要な `BufReader` ラッピングを除去（PMX/PMD 両方）
- **`sort_bones_topological` 最適化** — ボーン並び替えの子探索を O(n²) 線形走査から O(n) 隣接リストに変更。並び替え後の `clone()` を `Option::take()` パターンに変更し全ボーンのディープコピーを排除
- **PSD 出力 I/O 最適化** — UVマップ PSD のチャンネルデータ書き出しを 1バイト単位 `write_all` からチャンネルバッファ一括書き出しに変更（4096×4096 で最大 64M 回→4回に削減）。レイヤーデータにも `reserve` を追加
- **テクスチャアップロード最適化** — `upload_rgba_to_gpu` で縮小不要時の `rgba.to_vec()` コピーを排除（参照渡しに変更）。RGBA8 形式テクスチャの `img.pixels.clone()` も排除し直接アップロード
- **GPU 描画軽微改善** — ジョイント立方体頂点を `Vec<Vec3>` から `[Vec3; 8]` 固定長配列に変更。法線キャッシュ更新を `to_vec()` から `clear()` + `extend_from_slice()` に変更しヒープ再利用
- **PMX ライター最適化** — UTF-16LE エンコードを手動バイトプッシュから `to_le_bytes()` + `extend_from_slice()` に変更。UTF-8 パスは `Vec` コピーを経由せず直接書き出し
- **カメラ行列再利用** — `view_proj()` 内で `look_at_lh` を直接呼ぶ代わりに `view_matrix()` を再利用
- **デッドコード削除** — `pmx/extract.rs` の空ループ（何も処理しない for ループ）を除去
- **`build_composite` 冗長ループ削除** — `vec![255u8; ...]` で全バイト 255 初期化後に不要なアルファ設定ループを除去

## v0.2.5

### 改善

- **テクスチャ自動縮小** — GPU の最大テクスチャサイズ（通常 8192px）を超えるテクスチャを自動的にアスペクト比を保って縮小。巨大テクスチャを含むモデルでのクラッシュを防止
- **アーカイブ直接ロード（ZIP / 7z）** — ZIP / 7z アーカイブを直接 D&D / ダイアログで開き、内部の VRM / FBX / PMX / PMD モデルを自動検出。複数モデル時は選択ダイアログを表示。PMX/PMD はテクスチャ参照パスを解析して関連ファイルを自動収集
- **CLI アーカイブ対応** — `popone archive.zip output.pmx` で直接変換。`--list-models` でモデル一覧表示、`--model-name` で指定モデルを選択（完全一致→前方一致→部分一致、各段階で一意のみ採用）
- **Shift_JIS ファイル名対応** — ZIP 内の日本語ファイル名を UTF-8 → Shift_JIS フォールバックで正しくデコード
- **zip bomb 対策** — 総展開サイズ 2GB 上限、ZIP は `take()` でハード制限、7z はチャンク読み込みで実読込バイト数を検証
- **パストラバーサル防御** — アーカイブ内の `..` を含むパスを拒否（ZipSlip 攻撃対策）
- **リロード対応** — アーカイブから読み込んだモデルの Aスタンス切替等のリロードに対応。`ReloadableSource::Archive` で選択モデルパスを保持
- **アーカイブ内 UnityPackage 対応** — ZIP / 7z 内の `.unitypackage` を自動検出し、二重展開で内部の VRM / FBX を読み込み。リロード・アペンド・テクスチャ復元にも対応
- **展開サイズ上限** — `.unitypackage` (tar.gz) の展開にも 2GB サイズ上限を適用。外側アーカイブと内側パッケージの両方で防御
- **スタンス変換警告の常時表示** — Aスタンス/Tスタンス変換をONにしたが変換が実行されなかった場合、ビューポート左下に常時警告を表示。腕ボーン未検出（赤文字）/ 既に目標姿勢に近い（黄文字）の2種を表示。PMX出力時の警告もA/Tスタンスに応じて文言を分岐
- **UVマップ PSD レイヤーグループ化** — 複数モデルをマージした場合、UVマップ PSD 出力でモデル別にレイヤーをグループフォルダに格納。単一モデルでもグループ化される。PSD の lsct (Section Divider Setting) を使用し、Photoshop / CLIP STUDIO Paint と互換
- **MaterialGroup 構造体** — ビューアの材質グループ管理を `(String, usize, usize)` タプルから `MaterialGroup` 構造体に変更。`material_range`（材質index範囲）と `draw_range`（DrawCall範囲）を分離し、UV出力とUI表示で適切な範囲を使用

### コード品質・パフォーマンス改善

- **構造化エラー型** — `thiserror` で `PoponeError` enum を定義し、公開 API を `error::Result` に移行。内部は `anyhow` を継続使用し `From<anyhow::Error>` ブリッジで互換性維持
- **ViewerApp 構造体分割** — `PendingState`（遅延処理 10 フィールド）と `ExportState`（PMX エクスポート 4 フィールド）を分離。フィールド数 43 → 27 に削減
- **毎フレーム GPU テクスチャ再登録回避** — ビューポートテクスチャの登録/解放を `update_egui_texture_from_wgpu_texture` に変更し、フレームレート改善
- **ステータスバー format! キャッシュ** — モデル統計文字列をロード時に事前フォーマットし、毎フレームのヒープ割り当てを回避
- **リロード時の clone → take** — `reload_current()` で `morph_weights`・`material_visibility` 等を `std::mem::take()` で所有権移動（ヒープ再割り当て回避）
- **GLB 二重読み込み回避** — VRM 変換時に GLB を `(ir, glb_for_tex)` タプルで保持し、テクスチャ書き出し用の再読み込みを排除
- **BindGroupLayout 共通関数化** — `gpu::create_material_bind_group_layout()` で材質用レイアウト定義を一元化
- **dump コード重複解消** — `dump_ir()` 関数を抽出し、`run_main` と `run_archive_convert` の重複を除去

<details>
<summary>内部改善の詳細</summary>

#### 構造化エラー型（thiserror）

`error.rs` に `PoponeError` enum を定義し、`lib.rs` の公開 API を `error::Result` に移行。

```rust
#[derive(Debug, thiserror::Error)]
pub enum PoponeError {
    #[error("ファイル読み込み失敗: {0}")]
    Io(#[from] std::io::Error),
    #[error("GLB/VRM パース失敗: {0}")]
    GltfParse(#[from] gltf::Error),
    #[error("FBX パース失敗: {0}")]
    FbxParse(String),
    // ... PmxParse, PmdParse, Extraction, Build, Texture, Image, UnityPackage, Archive, Other
}

/// anyhow::Error から PoponeError への変換（既存コードとの互換用）
impl From<anyhow::Error> for PoponeError { ... }

pub type Result<T> = std::result::Result<T, PoponeError>;
```

- 公開 API: `error::Result<T>`（`PoponeError` で構造化）
- 内部: `anyhow::Result` を継続使用（`bail!`、`context()` 等の利便性を維持）
- ブリッジ: `From<anyhow::Error> for PoponeError` で `?` 演算子が自動変換

#### ViewerApp さらなる構造体分離

v0.2.2 の `TextureState` / `AnimLibrary` に加え、`PendingState` / `ExportState` を分離:

| サブ構造体 | フィールド | アクセス | 内容 |
|-----------|----------|---------|------|
| `TextureState` | `self.tex.*` | 9 フィールド | テクスチャ割り当て・パッケージテクスチャ・プレビュー・マッチング |
| `AnimLibrary` | `self.anim.*` | 4 フィールド | アニメーション再生状態・ライブラリ・Muscle スケール |
| `PendingState` | `self.pending.*` | 10 フィールド | 遅延処理（ファイル読み込み・GPU 再構築・PMX 変換等） |
| `ExportState` | `self.export.*` | 4 フィールド | PMX エクスポート（出力パス・ログ・表示材質のみ・UV 解像度） |

ViewerApp のフィールド数: 43（v0.2.1）→ 30（v0.2.2）→ 27（v0.2.5）。

#### 毎フレーム GPU テクスチャ再登録回避

ビューポートのオフスクリーンテクスチャ登録を初回 `register` + 以降 `update` 方式に変更:

```rust
// 変更前: 毎フレーム free + register
egui_renderer.free_texture(&old_id);
let tex_id = egui_renderer.register_native_texture(device, &view, FilterMode::Linear);

// 変更後: 初回 register、以降 update
let tex_id = if let Some(existing_id) = *cached_id {
    egui_renderer.update_egui_texture_from_wgpu_texture(device, &view, FilterMode::Linear, existing_id);
    existing_id
} else {
    let id = egui_renderer.register_native_texture(device, &view, FilterMode::Linear);
    *cached_id = Some(id);
    id
};
```

#### ステータスバー format! キャッシュ

モデル統計文字列を `CachedStats::new()` でロード時に事前フォーマット:

```rust
pub struct CachedStats {
    pub total_vertices: usize,
    pub total_faces: usize,
    pub status_text: String,  // 事前フォーマット済み
}

impl CachedStats {
    fn new(ir: &IrModel) -> Self {
        let status_text = format!(
            "頂点:{} 面:{} 材質:{} テクスチャ:{} ボーン:{} モーフ:{}",
            ...
        );
        Self { total_vertices, total_faces, status_text }
    }
}
```

`CachedMaterialInfo` にも `tex_status_text` フィールドを追加し、テクスチャ設定状況の文字列もキャッシュ。

#### リロード時の clone → take

`reload_current()` で状態を退避する際、`clone()` を `std::mem::take()` に変更:

| 対象 | 変更前 | 変更後 |
|------|--------|--------|
| `morph_weights` | `.clone()` | `std::mem::take()` |
| `material_visibility` | `.clone()` | `std::mem::take()` |
| `material_filter` | `.clone()` | `std::mem::take()` |
| `pmx_output_path` | `.clone()` | `std::mem::take()` |
| `tex.assignments` | `.clone()` | `std::mem::take()` |
| `tex.pkg_assignments` | `.clone()` | `std::mem::take()` |

`take()` は所有権を移動するため、Vec / HashMap のヒープ再割り当てが発生しない。リロード成功後に同じデータを復元するため、移動元は空の状態で問題ない。

#### GLB 二重読み込み回避

CLI 変換（`run_main`）で VRM → PMX 変換時、GLB を 2 回読み込んでいた問題を修正:

```rust
// 変更前: extract + テクスチャ書き出しで2回読み込み
let ir = vrm::extract::extract_ir_model(...)?;
let glb = vrm::loader::load_glb(&input)?;  // 2回目
convert::texture::write_all_textures(&ir.textures, &glb.images, &tex_dir)?;

// 変更後: タプルで保持して再利用
let (mut ir, glb_for_tex) = match ext.as_str() {
    _ => {
        let glb = vrm::loader::load_glb(&input)?;
        let ir = vrm::extract::extract_ir_model(...)?;
        (ir, Some(glb))
    }
};
if let Some(ref glb) = glb_for_tex {
    convert::texture::write_all_textures(&ir.textures, &glb.images, &tex_dir)?;
}
```

#### aux_files clone → take

`take_or_collect_aux()` で `preloaded.aux_files` を `clone()` から `take()` に変更し、HashMap バケットの再割り当てを回避。`preloaded` には空の HashMap を入れ直し、`main_bytes` は保持。

#### BindGroupLayout 共通関数化

材質用 `BindGroupLayout` の descriptor 定義を `gpu::create_material_bind_group_layout()` に一元化し、`gpu.rs` と `mesh.rs` のコード重複を解消。

#### dump コード重複解消

`run_main` と `run_archive_convert` に重複していたダンプ出力コードを `dump_ir()` 関数に抽出。

</details>

## v0.2.4

### 改善

- **アーカイブD&Dリロード対応** — zip/7z から D&D したファイルがOS一時ディレクトリに展開される問題に対応。モデル本体＋補助ファイル（テクスチャ・.txt）をオンメモリにスナップショット保持し、一時ファイル消失後もリロード可能に。VRM/FBX/PMX/PMD 全対応
- **アーカイブD&D先読みキャッシュ** — D&D 検出時点でモデル本体＋隣接テクスチャのバイト列を `PreloadedData` に先読み。以降のロードチェーン全体でキャッシュを使用し、一時ファイル消失後も確実にロード。FBX 選択ダイアログ（`PendingFbxChoice`）を挟む経路でもデータを受け渡し。VRM/FBX/PMX/PMD/UnityPackage 全形式対応
- **アーカイブD&D即座ロード** — zipアーカイブからの D&D 時、一時ファイルが2フレーム遅延の間に消失するエラーを修正。一時パスを検出した場合はプログレスオーバーレイを省略して即座にロード
- **テクスチャD&Dキャッシュ** — ZIP 内テクスチャの D&D 時、プレビュー段階でバイトデータ・PSD 判定・一時パスフラグをキャッシュ。確定時のファイル再読み込みを排除し、一時ファイル消失後もテクスチャ割り当てが確実に記録される
- **UnityPackage アーカイブスナップショット** — ZIP 内 .unitypackage の D&D 時、アーカイブデータを `Arc<[u8]>` でスナップショット保持。リロード・アペンド時に一時ファイルに依存せずメモリから復元可能に
- **シェーダー対応PMX材質** — MToon の shade_color と diffuse の輝度比に基づくトゥーンテクスチャ自動選択（5段階）。MToon 材質の ambient を shade_color ベースに、specular をゼロに補正。非 MToon は従来動作を維持
- **Aスタンス変換警告** — PMX 変換時、Aスタンス変換が有効だが腕ボーンが見つからない場合に赤文字オーバーレイで警告を表示。既にAスタンスに近い場合はスキップ通知を表示
- **ConvertResult::Warning** — 変換成功だが注意事項がある場合の新しいメッセージ種別（赤文字表示、Failure とは区別）
- **AStanceResult enum** — Aスタンス変換結果を型安全に管理（NotRequested / Applied / AlreadyAStance / NotFound）。IrModel::merge() での統合ロジック付き
- **リロード時テクスチャ正規化** — UnityPackage リロード時の PSD→PNG 変換バイパスを修正。MIME タイプ設定も正規パスと統一
- **IrTexture 重複排除** — テクスチャ割り当て時に filename + data で同一性を判定し、同一テクスチャの重複追加を防止

## v0.2.3

### 改善

- **表示材質のみ出力** — PMX 変換時に、表示タブで非表示にした材質を出力から除外するオプション（デフォルト OFF）。材質・メッシュ・テクスチャ・頂点モーフ・グループモーフを一貫してフィルタリング
- **ボーンマージ 2パス方式** — 同名ボーン統合の親子判定を順序非依存の候補収集＋伝播ループに変更。異なる部分木の子孫が誤統合されるバグを修正
- **pkg テクスチャ名前空間** — 複数 UnityPackage 追加時のテクスチャ名衝突を防止（`{パッケージ名}_pkg{連番}_{テクスチャ名}` 形式）。auto-matched テクスチャにも適用
- **ASCII FBX Content 処理** — Content ブロックを文字列として保持し、パーサー層の完全性を維持
- **テスト 61 件** — ボーンマージ・物理リマップ・モーフオフセット・エクスポートフィルタ等のテストを追加

## v0.2.2

### コード品質・パフォーマンス改善

- **パフォーマンス最適化** — アニメーション頂点バッファの毎フレーム alloc 除去、ボーン名探索の HashMap O(1) 化、GPU 可視化バッファの dirty flag 導入
- **テスト拡充** — 10 テスト → 51 テスト。座標変換ラウンドトリップ、ボーン名マッピング、PMX 書き込み・読み込みラウンドトリップ、VRM→PMX E2E テスト
- **Clippy 警告ゼロ** — `cargo clippy --all-targets --all-features -- -D warnings` 完全クリーン
- **UX 改善** — D&D オーバーレイ 4 パターン化、操作ヒント 2 行分割、グレーアウト UI ツールチップ追加

<details>
<summary>内部改善の詳細</summary>

#### ViewerApp サブ構造体化

v0.2.2 で ViewerApp の 43 フィールドを 30 フィールドに削減:

| サブ構造体 | フィールド | アクセス | 内容 |
|-----------|----------|---------|------|
| `TextureState` | `self.tex.*` | 9 フィールド | テクスチャ割り当て・パッケージテクスチャ・プレビュー・マッチング |
| `AnimLibrary` | `self.anim.*` | 4 フィールド | アニメーション再生状態・ライブラリ・Muscle スケール |

Rust の部分借用により `&mut self.tex` と `&self.anim` を同時に借用可能。

#### GPU 可視化バッファのキャッシュ戦略

ボーン・物理・ジョイントの可視化頂点を dirty flag で管理:

| 入力 | キャッシュキー | 再生成条件 |
|------|-------------|----------|
| ボーン頂点 | `camera.eye()`, `bone_opacity` | カメラ移動 / 不透明度変更 / アニメーション再生中 |
| SpringBone 頂点 | `spring_bone_opacity`, `align_rigid_rotation` | 設定変更 / アニメーション再生中 |
| ジョイント頂点 | `joint_opacity` | 設定変更 / アニメーション再生中 |

全バッファ共通:
- `vertex_count == 0` → 強制再生成（非表示→表示トグル復帰）
- `cache_had_anim && !has_anim` → アニメーション解除時に1フレーム強制再生成

#### アニメーション頂点バッファ最適化

`apply_bone_animation()` のホットパス改善:

| 項目 | Before | After |
|------|--------|-------|
| 頂点バッファ | `base.to_vec()` 毎フレーム alloc | `reset_animated_to_base()` capacity 再利用 |
| デルタ行列 | `Vec::with_capacity()` 毎フレーム | `work_deltas` フィールドで再利用 |
| globals 計算 | `Vec` 新規生成 + clone | in-place 更新（`work_computed` フラグ再利用） |
| モーフ適用 | `apply_morphs_to_buf(&self, &mut [Vertex])` | `apply_morphs_to_animated(&mut self)` 借用衝突回避 |

#### ボーン名探索 HashMap 化

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

#### テストデータパス解決

統合テストのファイルパスは環境変数で設定可能:

| 優先度 | 解決元 | 例 |
|--------|-------|-----|
| 1 | ファイル個別環境変数 | `POPONE_TEST_VRM_SEED_SAN=/path/to/Seed-san.vrm` |
| 2 | ルート環境変数 + 相対パス | `POPONE_TEST_DATA=/fixtures` → `/fixtures/vrm-spec/.../Seed-san.vrm` |
| 3 | `CARGO_MANIFEST_DIR/..` | ローカル開発時のデフォルト |

</details>

## FBX 対応

- バイナリ / ASCII FBX の自前パーサー（シーングラフ・座標系自動変換・PreRotation・UnitScaleFactor）
- ASCII FBX: Content ブロック（埋め込みテクスチャ）は文字列として保持し、外部ファイルフォールバックで復元
- スキンウェイト（最大 4 ボーン正規化）、ブレンドシェイプ、UV マッピング
- ヒューマノイドリグ自動検出（Mixamo / 3ds Max Biped / Maya HumanIK / VRoid / Unreal / Blender）。CamelCase ボーン名・名前空間プレフィックス（`Model::` 等）対応
- 零法線の自動補完、埋め込み/外部テクスチャ対応
