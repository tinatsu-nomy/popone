<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [技術詳細](#%E6%8A%80%E8%A1%93%E8%A9%B3%E7%B4%B0)
  - [座標変換](#%E5%BA%A7%E6%A8%99%E5%A4%89%E6%8F%9B)
    - [PMX/PMD → IrModel 逆変換](#pmxpmd-%E2%86%92-irmodel-%E9%80%86%E5%A4%89%E6%8F%9B)
  - [ボーン表示](#%E3%83%9C%E3%83%BC%E3%83%B3%E8%A1%A8%E7%A4%BA)
    - [形状判定（優先順）](#%E5%BD%A2%E7%8A%B6%E5%88%A4%E5%AE%9A%E5%84%AA%E5%85%88%E9%A0%86)
    - [IK 影響下ボーン](#ik-%E5%BD%B1%E9%9F%BF%E4%B8%8B%E3%83%9C%E3%83%BC%E3%83%B3)
    - [描画方向](#%E6%8F%8F%E7%94%BB%E6%96%B9%E5%90%91)
    - [描画パイプライン](#%E6%8F%8F%E7%94%BB%E3%83%91%E3%82%A4%E3%83%97%E3%83%A9%E3%82%A4%E3%83%B3)
    - [IrBone フィールド](#irbone-%E3%83%95%E3%82%A3%E3%83%BC%E3%83%AB%E3%83%89)
  - [MMD 標準ボーン挿入](#mmd-%E6%A8%99%E6%BA%96%E3%83%9C%E3%83%BC%E3%83%B3%E6%8C%BF%E5%85%A5)
    - [基本ボーン](#%E5%9F%BA%E6%9C%AC%E3%83%9C%E3%83%BC%E3%83%B3)
    - [IK ボーン](#ik-%E3%83%9C%E3%83%BC%E3%83%B3)
    - [準標準ボーン](#%E6%BA%96%E6%A8%99%E6%BA%96%E3%83%9C%E3%83%BC%E3%83%B3)
    - [insert_standard_bones ステップ詳細](#insert_standard_bones-%E3%82%B9%E3%83%86%E3%83%83%E3%83%97%E8%A9%B3%E7%B4%B0)
    - [PmxBuildOptions](#pmxbuildoptions)
  - [PMX 付与（grant）アニメーション](#pmx-%E4%BB%98%E4%B8%8Egrant%E3%82%A2%E3%83%8B%E3%83%A1%E3%83%BC%E3%82%B7%E3%83%A7%E3%83%B3)
    - [D-bones の仕組み](#d-bones-%E3%81%AE%E4%BB%95%E7%B5%84%E3%81%BF)
    - [処理フロー](#%E5%87%A6%E7%90%86%E3%83%95%E3%83%AD%E3%83%BC)
    - [IrGrant データ構造](#irgrant-%E3%83%87%E3%83%BC%E3%82%BF%E6%A7%8B%E9%80%A0)
  - [PMX/PMD ロード](#pmxpmd-%E3%83%AD%E3%83%BC%E3%83%89)
    - [PMX リーダー](#pmx-%E3%83%AA%E3%83%BC%E3%83%80%E3%83%BC)
    - [PMD リーダー](#pmd-%E3%83%AA%E3%83%BC%E3%83%80%E3%83%BC)
    - [IrModel 変換](#irmodel-%E5%A4%89%E6%8F%9B)
    - [Tスタンス変換](#t%E3%82%B9%E3%82%BF%E3%83%B3%E3%82%B9%E5%A4%89%E6%8F%9B)
    - [剛体回転](#%E5%89%9B%E4%BD%93%E5%9B%9E%E8%BB%A2)
    - [テクスチャ読み込み](#%E3%83%86%E3%82%AF%E3%82%B9%E3%83%81%E3%83%A3%E8%AA%AD%E3%81%BF%E8%BE%BC%E3%81%BF)
  - [MMD レンダリング](#mmd-%E3%83%AC%E3%83%B3%E3%83%80%E3%83%AA%E3%83%B3%E3%82%B0)
    - [アーキテクチャ](#%E3%82%A2%E3%83%BC%E3%82%AD%E3%83%86%E3%82%AF%E3%83%81%E3%83%A3)
    - [MMD シェーダー](#mmd-%E3%82%B7%E3%82%A7%E3%83%BC%E3%83%80%E3%83%BC)
    - [パイプライン構成](#%E3%83%91%E3%82%A4%E3%83%97%E3%83%A9%E3%82%A4%E3%83%B3%E6%A7%8B%E6%88%90)
    - [色空間](#%E8%89%B2%E7%A9%BA%E9%96%93)
    - [共有トゥーンテクスチャ](#%E5%85%B1%E6%9C%89%E3%83%88%E3%82%A5%E3%83%BC%E3%83%B3%E3%83%86%E3%82%AF%E3%82%B9%E3%83%81%E3%83%A3)
  - [シェーダーオーバーライド](#%E3%82%B7%E3%82%A7%E3%83%BC%E3%83%80%E3%83%BC%E3%82%AA%E3%83%BC%E3%83%90%E3%83%BC%E3%83%A9%E3%82%A4%E3%83%89)
    - [シェーダーモード一覧](#%E3%82%B7%E3%82%A7%E3%83%BC%E3%83%80%E3%83%BC%E3%83%A2%E3%83%BC%E3%83%89%E4%B8%80%E8%A6%A7)
    - [アルファ処理](#%E3%82%A2%E3%83%AB%E3%83%95%E3%82%A1%E5%87%A6%E7%90%86)
    - [状態正規化](#%E7%8A%B6%E6%85%8B%E6%AD%A3%E8%A6%8F%E5%8C%96)
  - [MToon シェーディング](#mtoon-%E3%82%B7%E3%82%A7%E3%83%BC%E3%83%87%E3%82%A3%E3%83%B3%E3%82%B0)
    - [MaterialUniform](#materialuniform)
    - [lit/shade 補間公式](#litshade-%E8%A3%9C%E9%96%93%E5%85%AC%E5%BC%8F)
    - [アウトライン描画](#%E3%82%A2%E3%82%A6%E3%83%88%E3%83%A9%E3%82%A4%E3%83%B3%E6%8F%8F%E7%94%BB)
    - [リムライティング](#%E3%83%AA%E3%83%A0%E3%83%A9%E3%82%A4%E3%83%86%E3%82%A3%E3%83%B3%E3%82%B0)
    - [MatCap テクスチャ](#matcap-%E3%83%86%E3%82%AF%E3%82%B9%E3%83%81%E3%83%A3)
    - [VRM パラメータ対応](#vrm-%E3%83%91%E3%83%A9%E3%83%A1%E3%83%BC%E3%82%BF%E5%AF%BE%E5%BF%9C)
    - [UV アニメーション](#uv-%E3%82%A2%E3%83%8B%E3%83%A1%E3%83%BC%E3%82%B7%E3%83%A7%E3%83%B3)
    - [透明描画順制御（alphaMode / transparentWithZWrite / renderQueueOffsetNumber）](#%E9%80%8F%E6%98%8E%E6%8F%8F%E7%94%BB%E9%A0%86%E5%88%B6%E5%BE%A1alphamode--transparentwithzwrite--renderqueueoffsetnumber)
  - [ビューア表示スタイル](#%E3%83%93%E3%83%A5%E3%83%BC%E3%82%A2%E8%A1%A8%E7%A4%BA%E3%82%B9%E3%82%BF%E3%82%A4%E3%83%AB)
    - [ボーン表示](#%E3%83%9C%E3%83%BC%E3%83%B3%E8%A1%A8%E7%A4%BA-1)
    - [剛体表示](#%E5%89%9B%E4%BD%93%E8%A1%A8%E7%A4%BA)
    - [ジョイント表示（PMX/PMD のみ）](#%E3%82%B8%E3%83%A7%E3%82%A4%E3%83%B3%E3%83%88%E8%A1%A8%E7%A4%BApmxpmd-%E3%81%AE%E3%81%BF)
    - [ワイヤーフレーム描画モード](#%E3%83%AF%E3%82%A4%E3%83%A4%E3%83%BC%E3%83%95%E3%83%AC%E3%83%BC%E3%83%A0%E6%8F%8F%E7%94%BB%E3%83%A2%E3%83%BC%E3%83%89)
    - [法線マップ表示](#%E6%B3%95%E7%B7%9A%E3%83%9E%E3%83%83%E3%83%97%E8%A1%A8%E7%A4%BA)
    - [法線マップ接線空間（TBN）](#%E6%B3%95%E7%B7%9A%E3%83%9E%E3%83%83%E3%83%97%E6%8E%A5%E7%B7%9A%E7%A9%BA%E9%96%93tbn)
    - [描画順](#%E6%8F%8F%E7%94%BB%E9%A0%86)
  - [カメラ・ライティング](#%E3%82%AB%E3%83%A1%E3%83%A9%E3%83%BB%E3%83%A9%E3%82%A4%E3%83%86%E3%82%A3%E3%83%B3%E3%82%B0)
    - [カメラ](#%E3%82%AB%E3%83%A1%E3%83%A9)
    - [フィット計算（compute_fit）](#%E3%83%95%E3%82%A3%E3%83%83%E3%83%88%E8%A8%88%E7%AE%97compute_fit)
    - [ライティング](#%E3%83%A9%E3%82%A4%E3%83%86%E3%82%A3%E3%83%B3%E3%82%B0)
    - [MMD ambient 分離](#mmd-ambient-%E5%88%86%E9%9B%A2)
  - [ログ出力](#%E3%83%AD%E3%82%B0%E5%87%BA%E5%8A%9B)
    - [ログの全体構成](#%E3%83%AD%E3%82%B0%E3%81%AE%E5%85%A8%E4%BD%93%E6%A7%8B%E6%88%90)
  - [シングルインスタンス](#%E3%82%B7%E3%83%B3%E3%82%B0%E3%83%AB%E3%82%A4%E3%83%B3%E3%82%B9%E3%82%BF%E3%83%B3%E3%82%B9)
  - [FPS 計測](#fps-%E8%A8%88%E6%B8%AC)
  - [アニメーション再生](#%E3%82%A2%E3%83%8B%E3%83%A1%E3%83%BC%E3%82%B7%E3%83%A7%E3%83%B3%E5%86%8D%E7%94%9F)
    - [対応形式](#%E5%AF%BE%E5%BF%9C%E5%BD%A2%E5%BC%8F)
    - [PMX/PMD でのアニメーション再生](#pmxpmd-%E3%81%A7%E3%81%AE%E3%82%A2%E3%83%8B%E3%83%A1%E3%83%BC%E3%82%B7%E3%83%A7%E3%83%B3%E5%86%8D%E7%94%9F)
    - [ヒューマノイドリターゲティング](#%E3%83%92%E3%83%A5%E3%83%BC%E3%83%9E%E3%83%8E%E3%82%A4%E3%83%89%E3%83%AA%E3%82%BF%E3%83%BC%E3%82%B2%E3%83%86%E3%82%A3%E3%83%B3%E3%82%B0)
    - [FBX アニメーション座標変換](#fbx-%E3%82%A2%E3%83%8B%E3%83%A1%E3%83%BC%E3%82%B7%E3%83%A7%E3%83%B3%E5%BA%A7%E6%A8%99%E5%A4%89%E6%8F%9B)
    - [Unity .anim Muscle 変換（隠し機能）](#unity-anim-muscle-%E5%A4%89%E6%8F%9B%E9%9A%A0%E3%81%97%E6%A9%9F%E8%83%BD)
    - [ループモード](#%E3%83%AB%E3%83%BC%E3%83%97%E3%83%A2%E3%83%BC%E3%83%89)
  - [モデル追加読み込み](#%E3%83%A2%E3%83%87%E3%83%AB%E8%BF%BD%E5%8A%A0%E8%AA%AD%E3%81%BF%E8%BE%BC%E3%81%BF)
    - [ボーンマージ 2パス方式](#%E3%83%9C%E3%83%BC%E3%83%B3%E3%83%9E%E3%83%BC%E3%82%B8-2%E3%83%91%E3%82%B9%E6%96%B9%E5%BC%8F)
    - [ASCII FBX Content ブロック処理](#ascii-fbx-content-%E3%83%96%E3%83%AD%E3%83%83%E3%82%AF%E5%87%A6%E7%90%86)
    - [pkg テクスチャ名前空間](#pkg-%E3%83%86%E3%82%AF%E3%82%B9%E3%83%81%E3%83%A3%E5%90%8D%E5%89%8D%E7%A9%BA%E9%96%93)
  - [アーカイブ直接ロード](#%E3%82%A2%E3%83%BC%E3%82%AB%E3%82%A4%E3%83%96%E7%9B%B4%E6%8E%A5%E3%83%AD%E3%83%BC%E3%83%89)
    - [archive モジュール](#archive-%E3%83%A2%E3%82%B8%E3%83%A5%E3%83%BC%E3%83%AB)
    - [ビューア統合](#%E3%83%93%E3%83%A5%E3%83%BC%E3%82%A2%E7%B5%B1%E5%90%88)
    - [CLI](#cli)
  - [アーカイブD&Dリロード対応](#%E3%82%A2%E3%83%BC%E3%82%AB%E3%82%A4%E3%83%96dd%E3%83%AA%E3%83%AD%E3%83%BC%E3%83%89%E5%AF%BE%E5%BF%9C)
    - [ReloadableSource enum](#reloadablesource-enum)
    - [一時パス検出](#%E4%B8%80%E6%99%82%E3%83%91%E3%82%B9%E6%A4%9C%E5%87%BA)
    - [一時パスの即座ロード](#%E4%B8%80%E6%99%82%E3%83%91%E3%82%B9%E3%81%AE%E5%8D%B3%E5%BA%A7%E3%83%AD%E3%83%BC%E3%83%89)
    - [D&D 先読みキャッシュ（PreloadedData）](#dd-%E5%85%88%E8%AA%AD%E3%81%BF%E3%82%AD%E3%83%A3%E3%83%83%E3%82%B7%E3%83%A5preloadeddata)
    - [補助ファイルキャッシュ](#%E8%A3%9C%E5%8A%A9%E3%83%95%E3%82%A1%E3%82%A4%E3%83%AB%E3%82%AD%E3%83%A3%E3%83%83%E3%82%B7%E3%83%A5)
    - [TextureSource enum](#texturesource-enum)
    - [reload_from_source](#reload_from_source)
    - [テクスチャD&Dプレビューキャッシュ](#%E3%83%86%E3%82%AF%E3%82%B9%E3%83%81%E3%83%A3dd%E3%83%97%E3%83%AC%E3%83%93%E3%83%A5%E3%83%BC%E3%82%AD%E3%83%A3%E3%83%83%E3%82%B7%E3%83%A5)
    - [UnityPackage アーカイブスナップショット](#unitypackage-%E3%82%A2%E3%83%BC%E3%82%AB%E3%82%A4%E3%83%96%E3%82%B9%E3%83%8A%E3%83%83%E3%83%97%E3%82%B7%E3%83%A7%E3%83%83%E3%83%88)
    - [.gltf の除外](#gltf-%E3%81%AE%E9%99%A4%E5%A4%96)
  - [リロード時テクスチャ正規化](#%E3%83%AA%E3%83%AD%E3%83%BC%E3%83%89%E6%99%82%E3%83%86%E3%82%AF%E3%82%B9%E3%83%81%E3%83%A3%E6%AD%A3%E8%A6%8F%E5%8C%96)
    - [reload_unitypackage のテクスチャ復元](#reload_unitypackage-%E3%81%AE%E3%83%86%E3%82%AF%E3%82%B9%E3%83%81%E3%83%A3%E5%BE%A9%E5%85%83)
    - [assign_texture_source_to_material の IrTexture 重複排除](#assign_texture_source_to_material-%E3%81%AE-irtexture-%E9%87%8D%E8%A4%87%E6%8E%92%E9%99%A4)
  - [シェーダー対応PMX材質変換](#%E3%82%B7%E3%82%A7%E3%83%BC%E3%83%80%E3%83%BC%E5%AF%BE%E5%BF%9Cpmx%E6%9D%90%E8%B3%AA%E5%A4%89%E6%8F%9B)
    - [select_toon()](#select_toon)
    - [MToon ambient/specular 補正](#mtoon-ambientspecular-%E8%A3%9C%E6%AD%A3)
    - [UTS2（Unity-Chan Toon Shader Ver.2）近似変換](#uts2unity-chan-toon-shader-ver2%E8%BF%91%E4%BC%BC%E5%A4%89%E6%8F%9B)
  - [Aスタンス変換結果の管理](#a%E3%82%B9%E3%82%BF%E3%83%B3%E3%82%B9%E5%A4%89%E6%8F%9B%E7%B5%90%E6%9E%9C%E3%81%AE%E7%AE%A1%E7%90%86)
    - [AStanceResult enum](#astanceresult-enum)
    - [判定ロジック](#%E5%88%A4%E5%AE%9A%E3%83%AD%E3%82%B8%E3%83%83%E3%82%AF)
    - [primary_astance_result](#primary_astance_result)
    - [IrModel::merge() での統合](#irmodelmerge-%E3%81%A7%E3%81%AE%E7%B5%B1%E5%90%88)
    - [ビューアでの警告表示](#%E3%83%93%E3%83%A5%E3%83%BC%E3%82%A2%E3%81%A7%E3%81%AE%E8%AD%A6%E5%91%8A%E8%A1%A8%E7%A4%BA)
  - [UVマップ PSD レイヤーグループ化](#uv%E3%83%9E%E3%83%83%E3%83%97-psd-%E3%83%AC%E3%82%A4%E3%83%A4%E3%83%BC%E3%82%B0%E3%83%AB%E3%83%BC%E3%83%97%E5%8C%96)
    - [PSD グループフォルダの仕組み](#psd-%E3%82%B0%E3%83%AB%E3%83%BC%E3%83%97%E3%83%95%E3%82%A9%E3%83%AB%E3%83%80%E3%81%AE%E4%BB%95%E7%B5%84%E3%81%BF)
    - [データフロー](#%E3%83%87%E3%83%BC%E3%82%BF%E3%83%95%E3%83%AD%E3%83%BC)
    - [入力検証 (`validate_groups`)](#%E5%85%A5%E5%8A%9B%E6%A4%9C%E8%A8%BC-validate_groups)
    - [entries 構築 (`build_entries`)](#entries-%E6%A7%8B%E7%AF%89-build_entries)
    - [`MaterialGroup` 構造体（`viewer/app/mod.rs`）](#materialgroup-%E6%A7%8B%E9%80%A0%E4%BD%93viewerappmodrs)
  - [表示材質のみ出力](#%E8%A1%A8%E7%A4%BA%E6%9D%90%E8%B3%AA%E3%81%AE%E3%81%BF%E5%87%BA%E5%8A%9B)
    - [設計方針](#%E8%A8%AD%E8%A8%88%E6%96%B9%E9%87%9D)
    - [処理フロー（`build_filtered_ir`）](#%E5%87%A6%E7%90%86%E3%83%95%E3%83%AD%E3%83%BCbuild_filtered_ir)
    - [モーフの再帰的有効性判定](#%E3%83%A2%E3%83%BC%E3%83%95%E3%81%AE%E5%86%8D%E5%B8%B0%E7%9A%84%E6%9C%89%E5%8A%B9%E6%80%A7%E5%88%A4%E5%AE%9A)
    - [テクスチャ pruning](#%E3%83%86%E3%82%AF%E3%82%B9%E3%83%81%E3%83%A3-pruning)
    - [仕様](#%E4%BB%95%E6%A7%98)
  - [アーキテクチャ](#%E3%82%A2%E3%83%BC%E3%82%AD%E3%83%86%E3%82%AF%E3%83%81%E3%83%A3-1)
  - [ソースファイル構成](#%E3%82%BD%E3%83%BC%E3%82%B9%E3%83%95%E3%82%A1%E3%82%A4%E3%83%AB%E6%A7%8B%E6%88%90)
  - [ライブラリ API](#%E3%83%A9%E3%82%A4%E3%83%96%E3%83%A9%E3%83%AA-api)
  - [テスト](#%E3%83%86%E3%82%B9%E3%83%88)
  - [更新履歴](#%E6%9B%B4%E6%96%B0%E5%B1%A5%E6%AD%B4)
  - [制限事項](#%E5%88%B6%E9%99%90%E4%BA%8B%E9%A0%85)
  - [参考資料](#%E5%8F%82%E8%80%83%E8%B3%87%E6%96%99)
    - [VRM 仕様の主要ポイント](#vrm-%E4%BB%95%E6%A7%98%E3%81%AE%E4%B8%BB%E8%A6%81%E3%83%9D%E3%82%A4%E3%83%B3%E3%83%88)
    - [PMX 仕様の主要ポイント](#pmx-%E4%BB%95%E6%A7%98%E3%81%AE%E4%B8%BB%E8%A6%81%E3%83%9D%E3%82%A4%E3%83%B3%E3%83%88)
    - [PMD 仕様の主要ポイント](#pmd-%E4%BB%95%E6%A7%98%E3%81%AE%E4%B8%BB%E8%A6%81%E3%83%9D%E3%82%A4%E3%83%B3%E3%83%88)
  - [WGSL シェーダー構成](#wgsl-%E3%82%B7%E3%82%A7%E3%83%BC%E3%83%80%E3%83%BC%E6%A7%8B%E6%88%90)
    - [共通マクロ](#%E5%85%B1%E9%80%9A%E3%83%9E%E3%82%AF%E3%83%AD)
    - [シェーダー定数](#%E3%82%B7%E3%82%A7%E3%83%BC%E3%83%80%E3%83%BC%E5%AE%9A%E6%95%B0)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

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
| モーフオフセット（位置） | `(x, y, -z) / 12.5`（変位ベクトル、スケール必要） |
| モーフオフセット（法線・接線） | `(x, y, -z)`（方向ベクトル、スケール不要） |
| 面巻き順 | b↔c swap（逆変換で反転） |
| 剛体・ジョイント位置 | PMX 座標のまま保持（ビューアが PMX 座標で描画） |

#### PMD 固有の変換

| 対象 | 処理 |
|------|------|
| 剛体位置 | ボーン相対オフセット → `bone.position + offset` で絶対座標に変換 |
| 剛体回転 | 絶対 Euler 角（そのまま使用、変換不要） |

## ボーン表示

ビューアはボーンをフラグに基づき4種類の形状で描画する。

### 形状判定（優先順）

| 優先度 | 条件 | 形状 | 描画内容 |
|--------|------|------|----------|
| 1 | `BONE_FLAG_IK` / PMD type=2 | ◻ IKコントローラ | 青外枠正方形 + オレンジ塗り + 青中心正方形 |
| 2 | `BONE_FLAG_AXIS_FIXED` | ⊗ 軸制限 | 青外円（太線） + ✕（太線） |
| 3 | `BONE_FLAG_TRANSLATABLE` / PMD type=1 | ◻ 移動 | 青外正方形 + 青内正方形 + 青中心塗り |
| 4 | なし | ◎ 通常 | 青外円 + 青内円 + 青中心塗り |

### IK 影響下ボーン

IK の Link チェーンに登録されたボーンはオレンジ色で表示（外枠・テール三角形がオレンジ、中心塗りは青）。Target ボーンは通常色（青）。

### 描画方向

| ソース | 方式 |
|--------|------|
| PMX/PMD | self→tail（`BoneTail::BoneIndex` / `BoneTail::Offset`）|
| VRM/FBX | parent→self（フォールバック） |

アニメーション中は `tail_bone_index`（`BoneTail::BoneIndex` 由来）から `animated_globals` の動的位置を参照し、テイルがモデルに追従する。

### 描画パイプライン

3段階の描画で重なり順を制御する。

| 順序 | パイプライン | 内容 |
|------|-------------|------|
| 1 | LineList | テール三角形（最背面） |
| 2 | TriangleList | マーカー塗りつぶし面（テールの上） |
| 3 | LineList | マーカー外枠線（最前面） |

4パスで優先度の高いボーンが常に手前に描画される: 通常(0) → IK影響下(1) → 軸制限(2) → IKコントローラ(3)。

### IrBone フィールド

| フィールド | 型 | PMX | PMD | VRM/FBX |
|-----------|-----|-----|-----|---------|
| `tail_position` | `Option<Vec3>` | BoneTail → glTF座標 | child → glTF座標 | None |
| `tail_bone_index` | `Option<usize>` | BoneTail::BoneIndex | child index | None |
| `is_ik` | `bool` | IK Link ボーン | IK Chain ボーン | false |
| `is_ik_bone` | `bool` | BONE_FLAG_IK | bone_type==2 | false |
| `is_translatable` | `bool` | BONE_FLAG_TRANSLATABLE | bone_type==1 | false |
| `is_axis_fixed` | `bool` | BONE_FLAG_AXIS_FIXED | false | false |
| `is_visible` | `bool` | BONE_FLAG_VISIBLE | bone_type!=7 | true |

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

### PmxBuildOptions

PMX モデル構築時のオプションを `PmxBuildOptions` 構造体で管理する。

| フィールド | CLI | 説明 |
|-----------|-----|------|
| `align_rigid_rotation` | `--align-rigid-rotation` | 剛体回転をボーン方向に揃える |
| `no_physics` | `--no-physics` | 剛体・ジョイントを出力しない |
| `raw_structure` | `--raw-structure` | 標準ボーン挿入をスキップし、元のボーン名を維持 |

`raw_structure` 有効時は `insert_standard_bones()` を完全にスキップする。`fix_duplicate_names` と `sort_bones_topological` は常に実行される。ボーン名は `IrBone.original_name`（VRM: glTF ノード名、FBX: FBX ノード名）がそのまま PMX に出力される。

`raw_structure` 有効時は `IrBone.grant` を `PmxGrant` に変換し、`BONE_FLAG_ROTATION_GRANT` / `BONE_FLAG_MOVE_GRANT` / `BONE_FLAG_LOCAL_GRANT` フラグも設定する。また `is_translatable`（`BONE_FLAG_TRANSLATABLE`）、`is_axis_fixed`（`BONE_FLAG_AXIS_FIXED`）、`is_visible`（`BONE_FLAG_VISIBLE`）も `IrBone` の値を忠実に反映する。これにより PMX → IrModel → PMX のラウンドトリップでボーンフラグ・付与データが保持される。

#### VrmConvertOptions

VRM → PMX 変換の公開 API は `VrmConvertOptions` 構造体でオプションを管理する。

| フィールド | 説明 |
|-----------|------|
| `no_physics` | 物理（剛体・ジョイント）を出力しない |
| `align_rigid_rotation` | 剛体回転をボーン方向に揃える |
| `normalize_pose` | A スタンスへ正規化 |
| `raw_structure` | 標準ボーン挿入をスキップ（元のボーン構造を維持） |

`VrmConvertOptions` は内部で `PmxBuildOptions` に変換される。`convert_ir_to_pmx`（ビューア用）は `PmxBuildOptions` を直接受け取る。

## PMX 付与（grant）アニメーション

PMX の回転付与（`BONE_FLAG_ROTATION_GRANT`）・移動付与（`BONE_FLAG_MOVE_GRANT`）をアニメーション再生時に処理する。

### D-bones の仕組み

Tda 式等の標準 MMD モデルでは、IK リンクボーン（足・ひざ・足首）に対応する「D ボーン」（足D・ひざD・足首D）が存在する。頂点ウェイトは D ボーンに割り当てられ、D ボーンは回転付与（ratio=1.0）で FK ボーンの回転をコピーする。

```
下半身
├ 左足     ← VRMA "leftUpperLeg" の回転が適用される
├ 左足D    ← 回転付与で「左足」の回転をコピー（ratio=1.0）
│ └ 左ひざD ← 回転付与で「左ひざ」の回転をコピー
│   └ 左足首D
```

### 処理フロー

```
1. compute_animated_globals_inplace()  — VRMA リターゲティング回転を適用
2. apply_grants()                      — 付与デルタを適用しグローバル行列を再計算
   フェーズ1: ボーンインデックス順に付与親のローカル回転/移動デルタを取得し、
             付与率に基づいてワークバッファ（work_local_mats）を更新
   フェーズ2: 全ボーンのグローバル行列をインデックス順に再計算（親→子の伝播保証）
3. デルタ行列計算 → 頂点スキニング
```

### IrGrant データ構造

| フィールド | 型 | 説明 |
|-----------|-----|------|
| `parent_index` | `usize` | 付与親ボーンインデックス |
| `ratio` | `f32` | 付与率（1.0 = 完全コピー、-1.0 = 逆回転） |
| `is_rotation` | `bool` | 回転付与フラグ |
| `is_move` | `bool` | 移動付与フラグ |
| `is_local` | `bool` | ローカル付与フラグ |

## PMX/PMD ロード

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
- **モーフインデックスリマッピング**: PMX はボーン/材質/UV モーフを含むが、IrModel では頂点モーフとグループモーフのみ保持する。スキップされるモーフがあるとインデックスがずれるため、`extract_morphs` で 2 パスの変換を行う:
  1. PMX モーフインデックス → IrModel モーフインデックスのマッピングテーブルを構築（スキップされるモーフは `None`）
  2. グループモーフのサブモーフ参照をリマッピング済みインデックスに変換。スキップされたモーフへの参照は除外
- **グループモーフ再帰深度制限**: ビューアの `apply_gpu_morph_recursive` はグループモーフを再帰的に展開するが、循環参照や自己参照を持つモデルで無限再帰→スタックオーバーフローを防ぐため最大深度 16 で打ち切り

### Tスタンス変換

`normalize_pose_to_tstance_full()` で A スタンス → T スタンスに変換:

1. 左右上腕を検出（`vrm_bone_name` または PMX 名 `"左腕"` / `"右腕"`）
2. 腕方向から水平までの角度を計算し、逆回転の補正クォータニオンを生成
3. ボーン位置・グローバル行列を補正
4. メッシュ頂点・法線をスキンウェイトに基づいて回転
5. モーフオフセット（位置・法線・接線）に回転を適用
6. 剛体・ジョイント: 影響ボーンの子孫に属するものの位置・回転を補正

### 剛体回転

PMX/PMD の剛体回転は Euler 角で格納。D3DX 行優先規約 `v * Ry * Rx * Rz`（外的 ZXY）に準拠し、glam 列優先では `Rz * Rx * Ry`（内的 YXZ）として再構成する。ファイルの値をそのまま使用する（座標変換不要）。

#### 剛体アニメーション追従の座標変換

ビューアの剛体・ジョイント描画は PMX 空間で行われる。`rb.position` と `joint.position` は PMX 座標のまま保持されるが、`bone.position` と `bone.global_mat` は PMX/PMD 抽出時に glTF 空間に変換されている（`pmx_pos_to_gltf`）。そのため、アニメーション追従のデルタ計算では全形式共通で glTF→PMX 座標変換を適用する:

- **位置変換**: PMX/PMD は VRM 1.0 と同じ Z 反転（`pmx_pos_to_gltf(v) = (x/S, y/S, -z/S)`）なので `gltf_pos_to_pmx` で逆変換
- **回転デルタ**: Z-flip `Quat(-x, -y, z, w)` を適用（VRM 1.0 と同一パス）

### テクスチャ読み込み

- PMX: テクスチャパステーブルからの相対パスで読み込み
- PMD: `parse_pmd_texture_slots` で `*` 区切りのメイン/スフィアテクスチャを分離。`.sph`→乗算、`.spa`→加算で分類。トゥーンテクスチャはファイル存在確認付きで登録し、不在時は共有トゥーンにフォールバック
- MIME ヒント: 拡張子から MIME タイプを推定し、`image::load_from_memory_with_format` で明示指定（TGA はマジックナンバーがなく自動判定が失敗するため）。`.sph/.spa` は `image/bmp` として扱う
- UnityPackage テクスチャ: `embed_textures_into_ir` でファイル拡張子から `mime_for_ext` 経由で MIME タイプを設定。空 MIME のままだと TGA/BMP 等の自動判定が失敗しマゼンタフォールバックになる

## MMD レンダリング

PMX/PMD ロード時に自動 ON になる MMD レンダリングモード。

### アーキテクチャ

- **RenderStyle enum** — DrawCall 単位で `Standard` / `Mmd` を判定（材質の `source_format.is_pmx_pmd()` で決定）。append 混在時にも正しく動作
- **フレーム単位 sRGB/Unorm 切り替え** — PMX/PMD 専用フレーム（全可視材質が MMD）では `Rgba8Unorm` レンダーターゲットを使用し、ガンマ空間で正確なアルファブレンドを実現。VRM 混在時は `Rgba8UnormSrgb` にフォールバック
- **パイプライン 4 セット** — `(MSAA有無) × (sRGB/Unorm)` の 4 セットを初期化時に生成。ランタイムコストはパイプライン参照の切り替えのみ
- **テクスチャデュアルビュー** — `view_formats: [Rgba8Unorm]` で同一テクスチャに sRGB/Unorm 両ビューを作成。MMD は Unorm ビューで読み取り（ガンマ空間のまま、メモリ増加なし）

### MMD シェーダー

#### メインシェーダー（`MMD_MAIN_SHADER_SRC` / `MMD_MAIN_SHADER_UNORM_SRC`）

```
Preshader:
  // AmbientColor = saturate(MaterialAmbient × LightAmbient + MaterialEmissive)
  // PMX ambient = D3D emissive, PMX diffuse = D3D ambient
  base_color = clamp(mat.diffuse_rgb * LightAmbient + mat.ambient, 0, 1)
  // LightAmbient = 154/255 ≈ 0.604

Pixel:
  tex = texture(Unorm)
  out_rgb = base_color * tex.rgb
  out_a   = tex.a * mat.alpha

  // スフィアマップ (RGB のみ、アルファ影響なし)
  // sphere_uv: X反転座標系 → vn_x * -0.5 + 0.5, vn_y * -0.5 + 0.5
  sph = sphere_texture(sphere_uv).rgb
  out_rgb += sph  // add モード
  out_rgb *= sph  // mul モード

  // トゥーン (NdotL 依存サンプリング + 乗算)
  lightNormal = dot(N, -L)
  toon_uv = (0, 0.5 - lightNormal * 0.5)
  toon = toon_texture(toon_uv)
  out_rgb *= toon.rgb

  // アルファテスト
  if out_a < 0.004: discard

  // スペキュラ (最後に加算、トゥーンの影響を受けない)
  // LightSpecular = LightAmbient (≈0.604)
  spec_color = mat.specular * LightSpecular
  out_rgb += spec_color * pow(NdotH, specular_power)

  // sRGB版: pow(2.2) で sRGB encode を打ち消し
  // Unorm版: ガンマ空間値をそのまま出力
```

#### エッジシェーダー（`MMD_EDGE_SHADER_SRC` / `MMD_EDGE_SHADER_UNORM_SRC`）

- inverted hull 法（Front cull）
- 法線方向膨張: `offset = edge_scale × mat.edge_size × camera.edge_thickness × pow(dist, 0.7) × 0.003`
- 2 スロット頂点バッファ: slot0=既存 Vertex、slot1=edge_scale(f32)
- sRGB 版: `pow(edge_color, 2.2)` で sRGB encode を打ち消し
- Unorm 版: edge_color をそのまま出力

### パイプライン構成

sRGB/Unorm 各セットに同一構成のパイプラインを持つ（計 2×2=4 セット）。

| パイプライン | cull | depth write | 用途 |
|------------|------|-------------|------|
| mmd_main_cull | Back | あり | MMD 不透明（片面） |
| mmd_main_no_cull | なし | あり | MMD 不透明（両面） |
| mmd_alpha_cull | Back | あり | MMD 半透明（片面） |
| mmd_alpha_no_cull | なし | あり | MMD 半透明（両面） |
| mmd_edge | Front | あり | エッジ |

MMD 半透明パイプラインもデプス書き込みあり（MMD 準拠）。材質インデックス順で描画するため、モデル作者が意図した前後関係が維持される。

#### MMD 描画順序

MMD は材質インデックス順に1材質ずつ描画する。popone でも同様に単一ループで材質順に描画し、`is_alpha` に応じてパイプライン（opaque/alpha）を切り替える。不透明材質の場合はメイン描画の直後にエッジも描画する。

```
for each material (in index order):
    select pipeline (opaque or alpha based on diffuse.w < 1.0)
    draw material
    if opaque && has_edge:
        draw edge
```

`can_use_unorm_frame()` が毎フレーム判定し、全可視材質が MMD の場合のみ Unorm セットを使用。

### 色空間

MMD (D3D9) はガンマ空間で動作する。wgpu での再現:

| 要素 | Standard (VRM/FBX) | MMD sRGB フォールバック | MMD Unorm（推奨） |
|------|-------|------|------|
| テクスチャ読み取り | Rgba8UnormSrgb（自動 sRGB→linear） | Rgba8Unorm（ガンマ空間） | Rgba8Unorm（ガンマ空間） |
| ライティング計算 | リニア空間 | ガンマ空間 | ガンマ空間 |
| アルファブレンド | リニア空間（正しい） | リニア空間（不正確） | ガンマ空間（MMD 準拠） |
| 出力 | そのまま | pow(2.2) で sRGB encode を打ち消し | そのまま（ガンマ値直接出力） |

### 共有トゥーンテクスチャ

MMD 標準 toon01-10 の実ピクセルデータ（32行分の RGB 値）を定数配列として保持し、1×32 RGBA テクスチャとして GPU にアップロード。サンプラーは `ClampToEdge` + `Linear`。シェーダーからは NdotL 依存の UV `(0, 0.5 − NdotL × 0.5)` でサンプルし、法線とライト方向に応じたトゥーン陰影を再現。

| トゥーン | 特徴 |
|---------|------|
| toon01 | 白→灰 (205,205,205)、2色ステップ |
| toon02 | 白→ピンク (245,225,225)、2色ステップ |
| toon03 | 白→暗灰 (154,154,154)、2色ステップ |
| toon04 | 白→暖ベージュ (248,239,235)、2色ステップ |
| toon05 | 白→暖ピンクのグラデーション |
| toon06 | 黄色系、中央ハイライトバンド + 暗黄 |
| toon07-10 | 全白（トゥーン効果なし） |

## シェーダーオーバーライド

ビューアは 6 種のシェーダーモードをサポートする。内部状態は 2 軸で管理される。

| 内部フィールド | 型 | 役割 |
|---|---|---|
| `shader_override` | `ShaderOverride` (u32) | GPU フラグメントシェーダー分岐（Default=0 / Normal=1 / Unlit=2 / GgxPreview=3） |
| `use_mmd_path` | `bool` | CPU 側の MMD 専用レンダーパス切替 |
| `auto_shader` | `bool` | Auto モード（モデル形式に応じた自動判定） |

`CameraUniform.shader_mode: u32` としてフラグメントシェーダーに渡され、`fs_main` 冒頭の整数比較で早期リターン分岐する。MMD は別パイプラインのため `shader_mode` には含めず、CPU 側の `mmd_solid` フラグで描画パスを切替える。

### シェーダーモード一覧

| モード | shader_mode | 描画パス | 内容 |
|---|---|---|---|
| Auto | 0 | 自動 | モデル形式に応じて Standard/MMD を自動選択 |
| MToon/Lambert | 0 | Standard 固定 | PMX/PMD でも MToon/Lambert で表示 |
| Unlit | 2 | Standard | テクスチャ色のみ、ライティングなし |
| GGX Preview | 3 | Standard | Cook-Torrance GGX (metallic=0, roughness=0.8 固定) |
| 法線 | 1 | Standard | ジオメトリ法線→RGB |
| MMD | 0 | MMD 固定 | Blinn-Phong + スフィア + トゥーン |

### アルファ処理

`apply_alpha_mode()` WGSL 関数で全モード共通のアルファ処理を行う。

```
OPAQUE  (cutoff < -0.75): テクスチャ alpha をそのまま返す（PMX/PMD 透過対応）
MASK    (cutoff >= -0.25): AlphaToCoverage + fwidth スムーズ化
BLEND   (else):           完全透明 discard のみ
```

オーバーライドモード（Unlit / GGX / Normal）では `apply_alpha_mode` を使わず、テクスチャ alpha を直接出力する（PMX/PMD の OPAQUE 材質でもテクスチャ透過を反映）。

### 状態正規化

`normalize_shader_state()` がモデルロード / rebuild / append の全経路で呼ばれ、Auto モードのみモデル形式に応じて `use_mmd_path` を自動設定する。ユーザーが明示選択した場合はモデル切替時も選択を維持する。

## MToon シェーディング

VRM の MToon 材質は Standard パイプライン内のフラグメントシェーダー分岐で 2 色トゥーンシェーディング + リムライティング + MatCap を行い、専用パイプラインでアウトライン描画を行う。

### MaterialUniform

```rust
// 448 bytes (gpu.rs)
pub struct MaterialUniform {
    pub diffuse: [f32; 4],              // ベースカラー（16 bytes）
    pub shade_color: [f32; 3],          // MToon 影色（12 bytes）
    pub is_mtoon: f32,                  // 0.0 or 1.0（4 bytes）
    pub shading_toony: f32,             // 影境界の硬さ 0.0~1.0（4 bytes）
    pub shading_shift: f32,             // 影の閾値シフト -1.0~1.0（4 bytes）
    pub outline_width: f32,             // アウトライン幅（4 bytes）
    pub outline_mode: f32,              // 0=none, 1=world, 2=screen（4 bytes）
    pub outline_color: [f32; 4],        // アウトラインカラー（16 bytes）
    pub outline_lighting_mix: f32,      // ライティング混合率 0~1（4 bytes）
    pub rim_fresnel_power: f32,         // リム フレネル指数（4 bytes）
    pub rim_lift: f32,                  // リム リフト量（4 bytes）
    pub rim_lighting_mix: f32,          // リム ライティング混合率（4 bytes）
    pub rim_color: [f32; 3],            // リムカラー（12 bytes）
    pub has_matcap: f32,                // MatCap 有効フラグ 0.0/1.0（4 bytes）
    pub matcap_factor: [f32; 3],        // MatCap 乗算色（12 bytes）
    pub has_shade_multiply_tex: f32,    // shadeMultiplyTexture 有無（4 bytes）
    pub has_shading_shift_tex: f32,     // shadingShiftTexture 有無（4 bytes）
    pub shading_shift_tex_scale: f32,   // shadingShiftTexture スケール（4 bytes）
    pub has_rim_multiply_tex: f32,      // rimMultiplyTexture 有無（4 bytes）
    pub uv_anim_scroll_x: f32,         // UV スクロール X 速度（4 bytes）
    pub uv_anim_scroll_y: f32,         // UV スクロール Y 速度（4 bytes）
    pub uv_anim_rotation: f32,         // UV 回転速度（4 bytes）
    pub has_uv_anim_mask: f32,          // uvAnimationMaskTexture 有無（4 bytes）
    pub alpha_cutoff: f32,              // alphaMode sentinel（4 bytes: -1.0=OPAQUE, -0.5=BLEND, >=0.0=MASK cutoff）
    // --- テクスチャ UV パラメータ ---
    pub base_uv_a: [f32; 4],            // baseColor texCoord+transform (16 bytes)
    pub base_uv_b: [f32; 4],            // baseColor texCoord+transform (16 bytes)
    pub shade_uv_a: [f32; 4],           // shade texCoord+transform (16 bytes)
    pub shade_uv_b: [f32; 4],           // shade texCoord+transform (16 bytes)
    pub shift_uv_a: [f32; 4],           // shift texCoord+transform (16 bytes)
    pub shift_uv_b: [f32; 4],           // shift texCoord+transform (16 bytes)
    pub rim_uv_a: [f32; 4],             // rim texCoord+transform (16 bytes)
    pub rim_uv_b: [f32; 4],             // rim texCoord+transform (16 bytes)
    pub outline_uv_a: [f32; 4],         // outline texCoord+transform (16 bytes)
    pub outline_uv_b: [f32; 4],         // outline texCoord+transform (16 bytes)
    pub uv_mask_uv_a: [f32; 4],         // uv_mask texCoord+transform (16 bytes)
    pub uv_mask_uv_b: [f32; 4],         // uv_mask texCoord+transform (16 bytes)
    pub emissive_factor: [f32; 3],      // glTF emissiveFactor (12 bytes)
    pub has_emissive_tex: f32,          // emissiveTexture 有無 (4 bytes)
    pub emissive_uv_a: [f32; 4],       // emissive texCoord+transform (16 bytes)
    pub emissive_uv_b: [f32; 4],       // emissive texCoord+transform (16 bytes)
    // --- 法線マップ + GI ---
    pub has_normal_tex: f32,            // normalTexture 有無 (4 bytes)
    pub normal_scale: f32,              // normalTexture.scale (4 bytes)
    pub gi_equalization_factor: f32,    // GI均一化係数 0.0~1.0 (4 bytes)
    pub outline_width_channel: f32,    // outlineWidthTexture チャネル 0=R,1=G,2=B (4 bytes)
    pub normal_uv_a: [f32; 4],         // normal texCoord+transform (16 bytes)
    pub normal_uv_b: [f32; 4],         // normal texCoord+transform (16 bytes)
    pub uv_anim_mask_channel: f32,     // uvAnimMaskTexture チャネル 0=R,1=G,2=B (4 bytes)
    pub _pad: [f32; 3],               // パディング (12 bytes)
    // --- matcap UV パラメータ ---
    pub matcap_uv_a: [f32; 4],        // matcap texCoord+transform (16 bytes)
    pub matcap_uv_b: [f32; 4],        // matcap texCoord+transform (16 bytes)
}
```

### lit/shade 補間公式

```wgsl
// 仕様準拠: dot(N,L) [-1,1] レンジ（half-lambert ではない）
// camera.light_dir は光の進行方向（光源→表面）なので反転して表面→光源方向にする
let dot_nl = dot(n, -light_dir);

// shadeMultiplyTexture: 影色にテクスチャを乗算
var shade_mul = vec3(1.0);
if has_shade_multiply_tex > 0.5 { shade_mul = textureSample(t_shade_multiply, ...).rgb; }
let shade = shade_color * shade_mul;

// shadingShiftTexture: ピクセルごとの影閾値シフト（VRM 1.0 仕様: tex.r * scale）
var shading = dot_nl + shading_shift;
if has_shading_shift_tex > 0.5 {
    shading += textureSample(t_shading_shift, ...).r * shading_shift_tex_scale;
}

// 仕様準拠 linearstep: clamp((x - edge0) / (edge1 - edge0), 0, 1)
let edge0 = -1.0 + shading_toony;
let edge1 = 1.0 - shading_toony;
let t = clamp((shading - edge0) / max(edge1 - edge0, 0.001), 0.0, 1.0);
lit = mix(shade, base_color.rgb, t);

// GI Equalization（UniVRM 準拠: indirect light のみ、direct light を含めない）
// 半球 ambient: sky/ground を最終法線Y成分で補間（SH 近似、normalMap 適用後の n を使用）
let raw_indirect = mix(ambient_ground, ambient, n.y * 0.5 + 0.5);
// uniformedGi = ambient（SH/IBL 非搭載時は均一、CameraUniform.gi_equalized）
let gi = mix(raw_indirect, gi_equalized, gi_equalization_factor);
// リム光量係数は未均一化の raw indirect を使用（UniVRM 準拠）
let rim_light_factor = light_intensity + raw_indirect;

// 最終ライティング合成（VRM 仕様: giLighting = gi(n) * litColor）
let direct_light = light_intensity * light_color;
let lighting = lit * direct_light + lit * gi;
```

- 仕様通り `dot(N,L)` [-1,1] を入力に使用（half-lambert [0,1] ではない）
- `linearstep` で補間（`smoothstep` の 3 次曲線ではなく線形）
- `shading_toony = 0.9`（デフォルト）→ `edge0 = -0.1, edge1 = 0.1` → 非常にシャープな影境界（アニメ調）
- `shading_toony = 0.0` → `edge0 = -1.0, edge1 = 1.0` → 柔らかいグラデーション
- `shading_shift` で影の位置を全体的にシフト（負で影が増える）
- `shadeColorFactor` 未指定時のデフォルトは `[0,0,0]`（黒）— 仕様準拠

### アウトライン描画

inverted hull 法で MToon アウトラインを描画する。`pipeline_outline`（Front cull パイプライン）を使用し、頂点を法線方向に膨張させる。`outlineWidthMultiplyTexture` を頂点シェーダーで `textureSampleLevel` によりサンプリングし、部位別のアウトライン幅制御に対応（参照チャネル: VRM 1.0=G、VRM 0.x=R、`ColorChannel` enum で動的選択）。mtoon_aux bind group の binding 6 に格納し、材質固有の bind group をアウトライン描画にも適用する。`edge_scale` 頂点属性は不要（GPU サンプリングのみ）のため、パイプラインの頂点レイアウトは `Vertex` 単一バッファ。`edge_scale_buf` は MMD エッジ専用。

```wgsl
// 頂点シェーダー: UV Animation 適用済み座標で outlineWidthMultiplyTexture をサンプリング（仕様準拠）
// edge_scale 頂点入力なし（GPU でテクスチャを直接サンプリング、CPU 側の edge_scale は PMX エクスポート用）
let width_uv = apply_uv_animation(uv);
let width_tex = select_channel(textureSampleLevel(t_outline_width, ..., width_uv, 0.0), material.outline_width_channel);
let width = outline_width * width_tex;
if outline_mode > 1.5 {
    // screenCoordinates: UniVRM 完全準拠の clip 空間オフセット
    let clip = view_proj * vec4(position, 1.0);
    let nv = vec3(dot(view_row0, n), dot(view_row1, n), dot(cross(view_row0, view_row1), n));
    var projected = normalize(vec2(nv.x, nv.y));      // 先に正規化（UniVRM 順序）
    let max_dist = proj_11;                           // 1/tan(fov/2) — UniVRM maxDistance 相当
    let clamped_w = min(clip.w, max_dist);            // 距離クランプ（広角・遠距離の太すぎ抑制）
    projected *= 2.0 * width * clamped_w;
    projected.x /= aspect;                            // aspect(=w/h) の逆数で X 補正（UniVRM は h/w を乗算）
    projected *= saturate(1.0 - nv.z * nv.z);        // カメラ正面抑制
    clip_position = vec4(clip.xy + projected, clip.zw);
} else {
    // worldCoordinates: ワールド空間でメートル単位
    let expanded = position + n * width;
    clip_position = view_proj * vec4(expanded, 1.0);
}
```

```wgsl
// フラグメントシェーダー: 本体と同等の MToon ライティング計算を行い、
// outlineLightingMixFactor で表面シェーディング結果とアウトライン色を混合（UniVRM 準拠）
let surface = compute_mtoon_surface_lighting(n, uv, world_pos);  // vec4: .rgb=色, .a=処理済みアルファ
// アウトラインパスでもベーステクスチャのアルファに基づく discard を実行（UniVRM 準拠）
// MASK 材質: surface.a < alpha_cutoff → discard、BLEND 材質: surface.a ≤ 0.001 → discard
// UniVRM 準拠: outlineColor * lerp(1, baseCol, outlineLightingMix)
let lit = outline_color.rgb * mix(vec3(1.0), surface.rgb, outline_lighting_mix);
```

- `compute_mtoon_surface_lighting()` は WGSL 関数として `wgsl_outline_body!()` マクロ内に定義。本体フラグメントシェーダーと同等の計算（2色トゥーン・shadeMultiply・shadingShift・リム・MatCap・rimMultiply・UVアニメーション）を実行し、`vec4`（.rgb=表面シェーディング色, .a=処理済みアルファ）を返す。アウトラインパスでもベーステクスチャのアルファに基づく discard を実行（UniVRM 準拠）
- アウトライン描画では本体と同じ `texture_bind_group` をバインドし、`baseColorTexture` を正しく参照する
- `OutlineVertexOutput` に `uv` と `world_pos` を追加し、フラグメントシェーダーでテクスチャサンプリングとリム計算を可能にした
- 描画順: 各レンダーキューフェーズ（OPAQUE / MASK / BlendZWrite / Blend）の直後にアウトライン描画。BLEND 材質は `pipeline_outline_blend`（ZWrite OFF）を使用し、UniVRM 準拠で半透明髪・装飾のアウトラインも描画
- UI チェックボックス「アウトライン描画」で ON/OFF 切替

### リムライティング

パラメトリックリムライティングはフレネル効果で輪郭を発光させる表現。ワールド座標を頂点シェーダーから渡し、フラグメントシェーダーで視線方向 V を計算する。

```wgsl
let v = normalize(camera_pos - world_pos);
let parametric_rim = pow(
    saturate(1.0 - dot(n, v) + rim_lift),
    max(rim_fresnel_power, 0.00001)
);
rim = parametric_rim * rim_color;
```

### MatCap テクスチャ

VRM 仕様に従い、視線方向から直交基底を構築して MatCap UV を算出する。

```wgsl
// UniVRM 準拠: right = cross(viewDir, worldUp), up = cross(right, viewDir)
let world_view_x = normalize(vec3(-v.z, 0.0, v.x));
let world_view_y = cross(world_view_x, v);
let raw_matcap_uv = vec2(dot(world_view_x, n), dot(world_view_y, n)) * 0.495 + 0.5;
// KHR_texture_transform 適用（matcap_uv_a/b で offset/scale/rotation を反映）
let matcap_uv = apply_texture_transform(raw_matcap_uv, matcap_uv_a, matcap_uv_b);
rim += matcap_factor * textureSample(t_matcap, matcap_uv).rgb;
```

- bind group(3) は MToon 補助テクスチャパック（サンプラー 8 + テクスチャ 8 = 16 bindings）。テクスチャごとにサンプラーを持ち、glTF の texture 単位 sampler モデルに完全準拠。binding 2n = sampler、binding 2n+1 = texture のペア構成:
  - binding 0-1: s_matcap / t_matcap (FRAGMENT)
  - binding 2-3: s_shade_multiply / t_shade_multiply (FRAGMENT)
  - binding 4-5: s_shading_shift / t_shading_shift (FRAGMENT)
  - binding 6-7: s_rim_multiply / t_rim_multiply (FRAGMENT)
  - binding 8-9: s_uv_anim_mask / t_uv_anim_mask (VERTEX + FRAGMENT)
  - binding 10-11: s_outline_width / t_outline_width (VERTEX)
  - binding 12-13: s_emissive / t_emissive (FRAGMENT)
  - binding 14-15: s_normal / t_normal (FRAGMENT)
- MToon 材質だけでなく `emissiveTexture` / `normalTexture` を持つ非 MToon 材質にも bind group(3) を生成（MToon 専用テクスチャはデフォルト値にフォールバック）
- emissiveTexture は glTF 標準プロパティのため MToon・非 MToon 両方で使用
- glTF 標準の `emissiveTexture` / `normalTexture` も `read_texture_info()` 経由で `texCoord` / `KHR_texture_transform` を保持
- normalTexture (binding 14-15) は FRAGMENT 可視、リニア色空間（Unorm ビュー）。MikkTSpace で生成した頂点接線から TBN 行列を構築し、tangent-space 法線をワールド空間に変換（UniVRM `MToon_GetTangentToWorld()` 準拠）。`normalTexture.scale` で強度制御。法線マップなしの材質にはフラット法線テクスチャ（1x1, RGBA=(128,128,255,255) = tangent-space (0,0,1)）を自動バインド。退化 UV（`det ≈ 0` やゼロベクトル近傍）では基底法線にフォールバックし `normalize(vec3(0))` の未定義動作を回避
- `doubleSided` 材質では `@builtin(front_facing)` で背面フラグメントの法線を法線マップ適用前に反転（UniVRM の `MTOON_IS_FRONT_VFACE` と同等）。`fs_main` / `fs_outline`（sRGB / Unorm 両版）に適用
- テクスチャ未使用材質にはデフォルトテクスチャを自動バインド（matcap=黒, 他=白）
- `rimMultiplyTexture` でリム効果をテクスチャで乗算マスク
- `rimLightingMixFactor` でリムと光量係数の混合率を制御（0.0=放射, 1.0=完全混合）。UniVRM 準拠で材質色を含まない `light_factor`（`light_intensity + ambient`、N·L 非依存）と混合（`lerp(white, light_factor, mix)`）
- `shadingShiftTexture` / `uvAnimationMaskTexture` はリニア色空間（Unorm ビュー）で読み込み。sRGB テクスチャ（shadeMultiply / rimMultiply / matcap）とは別に色空間を管理

### VRM パラメータ対応

| VRM 1.0 (`VRMC_materials_mtoon`) | VRM 0.0 (float_properties) | IrMaterial / MtoonParams フィールド |
|---|---|---|
| `shadeColorFactor` | `_ShadeColor` (vector) | `shade_color` |
| `shadingToonyFactor` | `_ShadeToony` | `shading_toony_factor` |
| `shadingShiftFactor` | `_ShadeShift` | `shading_shift_factor` |
| `outlineWidthMode` | `_OutlineWidthMode` | `outline_width_mode` |
| `outlineWidthFactor` | `_OutlineWidth` | `outline_width_factor` |
| `outlineColorFactor` | `_OutlineColor` | `edge_color` |
| `outlineLightingMixFactor` | `_OutlineLightingMix` | `outline_lighting_mix` |
| `parametricRimColorFactor` | `_RimColor` (vector) | `parametric_rim_color` |
| `parametricRimFresnelPowerFactor` | `_RimFresnelPower` | `parametric_rim_fresnel_power` |
| `parametricRimLiftFactor` | `_RimLift` | `parametric_rim_lift` |
| `rimLightingMixFactor` | 常に 1.0（破壊的マイグレーション） | `rim_lighting_mix` |
| `matcapFactor` | `_SphereAdd` 有→[1,1,1], 無→[0,0,0] | `matcap_factor` |
| `matcapTexture` | `_SphereAdd` | `matcap_texture: Option<IrTextureInfo>` |
| `shadeMultiplyTexture` | `_ShadeTexture`（未設定時 `_MainTex`） | `shade_texture: Option<IrTextureInfo>` |
| `shadingShiftTexture` + `scale` | — | `shading_shift_texture: Option<IrTextureInfo>` + `shading_shift_texture_scale` |
| `rimMultiplyTexture` | `_RimTexture` | `rim_multiply_texture: Option<IrTextureInfo>` |
| `uvAnimationScrollXSpeedFactor` | `_UvAnimScrollX` | `uv_animation_scroll_x_speed` |
| `uvAnimationScrollYSpeedFactor` | `_UvAnimScrollY`（Y 反転 × -1） | `uv_animation_scroll_y_speed` |
| `uvAnimationRotationSpeedFactor` | `_UvAnimRotation`（× 2π） | `uv_animation_rotation_speed` |
| `uvAnimationMaskTexture` | `_UvAnimMaskTexture` | `uv_animation_mask_texture: Option<IrTextureInfo>` |
| glTF `emissiveFactor` | `_EmissionColor` (vector) | `emissive_factor` |
| glTF `emissiveTexture` | `_EmissionMap` | `emissive_texture: Option<IrTextureInfo>` |
| glTF `normalTexture` | `_BumpMap` | `normal_texture: Option<IrTextureInfo>` |
| glTF `normalTexture.scale` | `_BumpScale` | `normal_texture_scale` |
| `alphaMode` | `_BlendMode`（0=OPAQUE,1=MASK,2=BLEND,3=BlendZWrite） | `alpha_mode` |
| glTF `alphaCutoff` | `_Cutoff` | `alpha_cutoff` |
| glTF `doubleSided` | `_CullMode`（0=Off→None, 1=Front→Front, 2=Back→Back） | `cull_mode: CullMode` |
| — | `renderQueue` | `render_queue_offset`（後処理で算出） |
| glTF `baseColorFactor` | `_Color` (vector, sRGB→Linear) | `diffuse` |
| glTF `baseColorTexture` | `_MainTex` | `texture_index` / `base_color_tex_info` |
| — | `_MainTex` ST | 全テクスチャの `IrTextureInfo.offset` / `.scale` |
| `giEqualizationFactor` | `_IndirectLightIntensity`（`1.0 - value`） | `gi_equalization_factor` |

VRM 0.x 固有の追加移行処理:

- **`_Color` / `_MainTex` lit 色・テクスチャ正規化**: VRM 0.x MToon では glTF core の `baseColorFactor` / `baseColorTexture` が近似値の場合があるため、MToon と判定した後は `materialProperties` の `_Color`（sRGB→Linear 変換）→ `diffuse`、`_MainTex` → `texture_index` / `base_color_tex_info` を優先する（UniVRM `MigrationMToonMaterial.cs:148-164` 準拠）
- **`renderQueue` → `render_queue_offset`**: UniVRM `MigrationMToonMaterial.cs` 準拠の順位圧縮（rank compression）。透明材質の source offset（`renderQueue - DefaultValue`）を `BTreeSet` に集約し、Blend は降順・BlendWithZWrite は昇順で連番を振ることで、相対順序を保持したまま VRM 1.0 仕様範囲（Blend: -9..0, BlendWithZWrite: 0..+9）に圧縮。`renderQueue` が許容範囲外（Blend: 2951~3000、BlendWithZWrite: 2501~2550）の場合は offset=0 を返す
- **`_MainTex` ST（テクスチャ Scale/Translation）伝播**: VRM 0.x の `vectorProperties._MainTex` は `[offsetX, offsetY, scaleX, scaleY]` 順。Unity のテクスチャ座標系（左上原点）と glTF `KHR_texture_transform`（左下原点）は Y 軸の解釈が異なるため、offset を `offset.y = 1.0 - unityOffset.y - scale.y` で変換する（UniVRM `Vrm10MaterialExportUtils.ExportTextureTransform` 準拠）。UniVRM は `_MainTex` ST を全 MToon テクスチャに `KHR_texture_transform` として移行する。ただし **MatCap（`_SphereAdd`）は例外**で、VRM 1.0 では ST を適用しない（UniVRM `MigrationMToonMaterial.cs:255-260` 準拠: "Texture transform is not required"）。identity transform（scale=1, offset=0）の場合はスキップ。`_OutlineWidthTexture` も `resolve_tex()` ヘルパー経由で ST を伝播する（UniVRM `MigrationMToonMaterial.cs` 準拠）
- **`ScreenCoordinates` アウトライン幅正規化**: `outline_width_factor = w * 0.01 * 0.5`（UniVRM 準拠: 旧縦半分の%値 → 新縦全体の比率、1/200 換算）
- **色プロパティ sRGB→Linear 変換**: VRM 0.x の `_ShadeColor`・`_RimColor`・`_OutlineColor` は sRGB ガンマ空間で格納されているため、抽出時に IEC 61966-2-1 準拠の sRGB→Linear 変換を適用する（UniVRM `MigrationMToonMaterial.cs` の `.ToFloat3(ColorSpace.sRGB, ColorSpace.Linear)` と同等）。`_EmissionColor` は UniVRM 側でも Linear→Linear のため変換対象外
- **`_IndirectLightIntensity` → `gi_equalization_factor`**: UniVRM 準拠の変換式 `gi_equalization_factor = (1.0 - gi_intensity).clamp(0.0, 1.0)` を適用。`MaterialUniform` 経由で GPU シェーダーに送信し、`lerp(passthroughGi, uniformedGi, giEqualizationFactor)` で GI 均一化を実装。SH/IBL 非搭載のため `passthroughGi` = `uniformedGi` = ambient（UniVRM の `indirectLight` / `indirectLightEqualized` に相当、direct light は含めない）

`IrTextureInfo` はテクスチャ index に加え `tex_coord`（TEXCOORD セット番号）、`KHR_texture_transform`（offset / scale / rotation）、および `IrSamplerInfo`（wrap_u / wrap_v / mag_filter: `IrMagFilter` / min_filter: `IrMinFilter`）を保持する。`IrMinFilter` は glTF の `minFilter` 6 値（Nearest / Linear / NearestMipmapNearest / LinearMipmapNearest / NearestMipmapLinear / LinearMipmapLinear）をそのまま保持し、wgpu の `min_filter` と `mipmap_filter` に正しく分離される。glTF の `sampler` オブジェクトから wrapS / wrapT / magFilter / minFilter を読み取り、ビューア GPU 側は `HashMap<IrSamplerInfo, wgpu::Sampler>` キャッシュでサンプラーを共有する。bind group(3) ではテクスチャごとに個別のサンプラーを持ち、glTF の texture 単位 sampler モデルに完全準拠する。CPU 側サンプリング（`sample_image_g_channel`）も wrap mode に応じた UV 変換を適用する。ベースカラーテクスチャ（`base_color_tex_info`）および全 MToon 補助テクスチャで `resolve_mtoon_uv()` ヘルパにより texCoord 選択 + KHR_texture_transform 適用を統一的に実行する。非 MToon 材質でも `baseColorTexture` / `emissiveTexture` に `resolve_mtoon_uv()` を適用し、`texCoord` / `KHR_texture_transform` を反映する。UV Animation 対象（baseColor / shade / rim / outline_width / emissive / normalTexture）と非対象（shift / uv_mask / matcap）は仕様に従い区別される。`KHR_texture_transform.texCoord` が存在する場合は TextureInfo 本体の `texCoord` より優先される（glTF 仕様準拠）。`texCoord=1` を要求するテクスチャに対しメッシュに `TEXCOORD_1` が存在しない場合、GPU 側・CPU 側ともに `Vec2::ZERO` にフォールバックする（UniVRM `MeshData.cs` 準拠）。extract 完了後にメッシュ単位で UV1 有無を判定し、UV1 を持たないメッシュが参照する材質の全テクスチャ（`base_color_tex_info` を含む）の `texCoord=1` を `texCoord=0` に正規化する。これにより tangent 生成と描画の UV セット不一致を防止する。UI からテクスチャを差し替える際も、材質の `IrSamplerInfo` を参照してサンプラーを再生成するため、`ClampToEdge` / `Nearest` 等のテクスチャ固有設定が維持される。

#### テクスチャ index の正規化

glTF では `textures[]` と `images[]` は別配列であり、`TextureInfo.index` は texture index を指す。`IrModel.textures` は image 配列基準で構築されるため、`read_texture_info()` は glTF の texture index を `document.textures().nth(i).source().index()` で **image index に正規化**して `IrTextureInfo.index` に格納する。これにより下流（ビューア bind group、export_filter pruning、merge offset）はすべて image index 前提で安全に動作する。VRM 0.0 の `_OutlineWidthTexture` も同様に image index に解決する。`texCoord >= 2` は未対応のため、検出時にエラーログを出力しテクスチャを無効化する（`None` を返す）。サイレントフォールバックによる誤描画を防止する。core glTF API で先に設定されたテクスチャ参照も raw JSON の判定結果で明示的にクリアし、fail-close を保証する。

### UV アニメーション

`CameraUniform` に累積時間 `time` を追加し、シェーダー内でテクスチャ UV を毎フレーム変換する。

```wgsl
// 仕様準拠順序: scroll → pivot(-0.5) → rotation → pivot(+0.5)
// UniVRM: vrmc_materials_mtoon_geometry_uv.hlsl — rotate(uv + translate - pivot) + pivot
let translate = vec2(time * scroll_x, time * scroll_y) * mask;
// 2π 周期で wrap して長時間稼働時の float 精度劣化を防止（UniVRM 準拠）
let tau = 6.28318530718;
let turns = (time * uv_anim_rotation * mask) / tau;
let angle = fract(turns) * tau;
let centered = (uv + translate) - vec2(0.5);
anim_uv = vec2(centered.x * cos(angle) - centered.y * sin(angle),
               centered.x * sin(angle) + centered.y * cos(angle)) + vec2(0.5);
```

- UV Animation 計算は `apply_uv_anim_core()` 関数で本体・アウトライン共通化。法線マップにも適用するため、MToon ブランチ前に事前計算（hoist）される
- 回転角は `fract(turns) * 2π` で周期内に折り返し、長時間稼働時の float 精度劣化を防止（UniVRM 準拠）
- 適用順: スクロール → 回転（VRM 仕様準拠: `scroll → pivot → rotation → pivot back`）
- `uvAnimationMaskTexture` で適用範囲を 0.0〜1.0 で制御（参照チャネル: VRM 1.0=B、VRM 0.x=R、`ColorChannel` enum で動的選択）
- アニメーション対象: baseColor / shadeMultiply / **shadingShiftTexture** / rimMultiply / outlineWidthMultiply / emissive / **normalTexture** の UV（UniVRM 準拠: `GetMToonGeometry_Uv()` 適用済み UV を全テクスチャに使用。matcap は対象外）

### 透明描画順制御（alphaMode / transparentWithZWrite / renderQueueOffsetNumber）

MToon 仕様準拠の 4 段階レンダーキューで描画順を制御する。

#### AlphaMode

glTF の `alphaMode` と MToon 拡張の `transparentWithZWrite` を統合した `AlphaMode` enum:

| AlphaMode | glTF alphaMode | transparentWithZWrite | depth write | 説明 |
|-----------|---------------|----------------------|-------------|------|
| Opaque | OPAQUE | — | あり | 不透明 |
| Mask | MASK | — | あり | alphaCutoff で discard |
| BlendWithZWrite | BLEND | true | あり | 半透明 + デプス書込 |
| Blend | BLEND | false | なし | 通常半透明 |

#### 描画順

```
1. OPAQUE（デプス書込あり）
   → アウトライン描画
2. MASK（デプス書込あり、alphaCutoff による discard）
   → アウトライン描画
3. BlendZWrite（デプス書込あり、アルファブレンド）
   → アウトライン描画
4. Blend（デプス書込なし、アルファブレンド）
   → アウトライン描画（ZWrite OFF）
```

- MASK パイプライン: `alpha_to_coverage_enabled = true`（MSAA 有効時のみ）により cutout 境界のジャギーを軽減。UniVRM `MToonValidator.cs` の `UnityAlphaToMask = On` と同等。MASK アウトラインパイプライン（`pipeline_outline_mask`）にも同様に AlphaToCoverage を有効化し、サーフェスとアウトラインのエッジ品質を一致させる

各カテゴリ内は `renderQueueOffsetNumber` で安定ソート。`renderQueueOffsetNumber` は BLEND 時のみ有効（Opaque/Mask は常に 0）。BlendZWrite は `clamp(0, +9)`、Blend は `clamp(-9, 0)` で範囲制限（UniVRM MToonValidator 準拠）。さらに `RenderQueue::Blend` / `RenderQueue::BlendZWrite` 内の同一 `renderQueueOffsetNumber` 材質はカメラ距離（`distance_squared`）で back-to-front ソートし、半透明メッシュ同士の前後関係を改善する。距離キーはアニメーション済み頂点 `current_vertices()` から毎フレーム再計算（不透明 draw はビルド時の固定重心を維持）。

BLEND / BlendZWrite フェーズではサーフェスとアウトラインを各 draw ごとにインターリーブ発行する（ZWrite OFF のため描画順 = 合成順）。OPAQUE / MASK は深度バッファで保護されるため従来通りフェーズ後にまとめてアウトライン描画。

#### alphaMode のシェーダー処理

`MaterialUniform.alpha_cutoff` フィールドに alphaMode を sentinel 値でエンコードし、フラグメントシェーダーで分岐:

| alphaMode | sentinel 値 | 判定条件 |
|-----------|------------|---------|
| OPAQUE | `-1.0` | `< -0.75` |
| BLEND | `-0.5` | `-0.75` ≤ x `< -0.25` |
| MASK | `>=0.0`（実 cutoff 値） | `>= -0.25` |

```wgsl
// alphaMode 処理（alpha_cutoff エンコーディング: <-0.75=OPAQUE, >=-0.25=MASK, else=BLEND）
if material.alpha_cutoff < -0.75 {
    // OPAQUE (-1.0): アルファ無視、常に不透明
    out_alpha = 1.0;
} else if material.alpha_cutoff >= -0.25 {
    // MASK (>=0.0): cutoff 未満を破棄、通過後は不透明
    if out_alpha < material.alpha_cutoff { discard; }
    out_alpha = 1.0;
} else {
    // BLEND (-0.5) / BlendZWrite: 完全透明ピクセルを破棄（深度汚染防止）
    if out_alpha <= 0.001 { discard; }
}
```

- OPAQUE / MASK: 出力アルファを 1.0 に固定し、テクスチャの半透明値による意図しない透過を防止
- BLEND / BlendZWrite: 完全透明ピクセルの `discard` で深度バッファ汚染を防止（`transparentWithZWrite` で見えないピクセルが後続メッシュを隠す問題の回避）

#### パイプライン

| パイプライン | cull | depth write | 用途 |
|------------|------|-------------|------|
| cull / no_cull | Back / なし | あり | OPAQUE / MASK |
| alpha_zwrite_cull / alpha_zwrite_no_cull | Back / なし | あり | BlendZWrite |
| alpha_cull / alpha_no_cull | Back / なし | なし | Blend |
| outline | Front | あり | MToon アウトライン（OPAQUE / BlendZWrite）。depth bias 付き（UniVRM `Offset 1, 1` 相当） |
| outline_mask | Front | あり | MToon アウトライン（MASK）。depth bias + AlphaToCoverage 付き |
| outline_blend | Front | なし | MToon アウトライン（Blend）。depth bias 付き |

## ビューア表示スタイル

### ボーン表示

- 形状: ◎△（二重円＋底辺なし三角形）
- 描画: 1px LineList（`pipeline_line_overlay`）
- 色: 通常ボーン = ブルー `#0000ff`、IK ボーン = オレンジ `#ff9600`
- サイズ: カメラ距離に応じてスケール（画面上一定サイズ）
- IK 判定: ボーン名に "ＩＫ" または "IK" を含むか

### 剛体表示

- 描画: 1px LineList
- 色（PMX/PMD）: `physics_mode` で分類 — ボーン追従(0)=グリーン `#00ff00`、物理演算(1)=レッド `#ff0000`、物理+ボーン(2)=ブルー `#0080ff`
- 色（VRM）: `group` で分類 — コライダー(group=1)=レッド `#ff0000`、スプリング(group!=1)=グリーン `#00ff00`
- 球体: 8 経線（大円弧）+ 7 緯線
- カプセル: 上下赤道リング + 8 本接続線 + 半球ワイヤーフレーム（4 経線 + 3 緯線 × 上下、PMX/PMD のみ）
- ボックス: 12 辺（size は half-extent として扱う）

### ジョイント表示（PMX/PMD のみ）

- 形状: 正立方体（面=イエロー `#ffff00`、エッジ=1px 黒線）
- サイズ: 0.18 PMX 単位
- 回転: Euler YXZ intrinsic（= ZXY extrinsic）→ Quat で姿勢反映
- アニメーション同期: rigid_a のボーンからのオフセットで追従
- 濃さ: スライダーで調整可能

### ワイヤーフレーム描画モード

- `DrawMode` enum: `Solid` / `Wireframe` / `SolidWireframe`
- **Solid**: 通常のソリッド描画（`PolygonMode::Fill`）
- **Wire**: `pipeline_wireframe`（`PolygonMode::Line`、cull_mode=None）で全メッシュを描画。アウトライン描画（`pipeline_outline*`）と MMD エッジ描画（`pipeline_mmd_edge`）はスキップ。MMD 材質もワイヤーフレームパイプラインに切り替え（標準 bind group layout を使用）
- **S+W**: ソリッド描画後にワイヤーフレームオーバーレイ（`pipeline_wire_overlay`、depth bias -2 で Z ファイティング回避、黒色半透明）
- GPU 機能 `POLYGON_MODE_LINE` 非対応時は Wire / S+W を無効化（UI 非表示）
- 「アウトライン描画」チェックボックスは MToon アウトラインを持つ `RenderStyle::Standard` draw が存在する場合のみ有効。PMD/PMX（`RenderStyle::Mmd`）ではグレーアウト

### 法線マップ表示

- シェーダー内で法線ベクトル → RGB 変換: `rgb = (normalize(normal) + 1.0) * 0.5`
- CameraUniform の `show_normal_map` フラグで切替

### 法線マップ接線空間（TBN）

- 頂点接線は `IrVertex.tangent: Vec4`（xyz=方向, w=handedness ±1）として保持
- glTF `TANGENT` 属性があればスキニング変換して使用、なければ `mikktspace` crate で MikkTSpace 接線を自動生成（VRM 仕様: TANGENT はエクスポートせず、インポート時に MikkTSpace で計算）
- MikkTSpace 生成は `normalTexture.texCoord` に応じた UV セットを使用（texCoord=1 かつ UV1 が存在する場合は UV1 基準で生成）
- MikkTSpace コーナー tangent 処理: `set_tangent_encoded()` の出力をコーナー単位（`face * 3 + vert`）で保持。同一頂点を共有するコーナー間で `tangent.w`（handedness）が異なる場合、少数派コーナーの頂点を自動分割（indices / morph targets / UV1 連動更新）。分割後、同一 w グループ内で xyz を平均化・正規化して頂点 tangent に格納
- imported tangent の退化検出: glTF TANGENT 属性のスキニング変換後に Gram-Schmidt 再直交化を行い、結果の `t_ortho` が長さ閾値未満または非有限値なら `Vec4::ZERO` に戻して MikkTSpace 再生成ルートへ流す。tangent 有効判定は `xyz.length_squared() > 1e-8` ベース（`Vec4::ZERO` 完全一致ではなく、w 非ゼロの退化 tangent `[0,0,0,1]` も再生成対象）
- ビューア座標変換（VRM 1.0: Z反転、VRM 0.0: X反転）はいずれも行列式 -1 のミラー変換。`cross(M*N, M*T) = det(M) * M * cross(N,T) = -M * cross(N,T)` となるため、bitangent の向きを維持するには `tangent.w` を反転する必要がある
- シェーダー TBN 構築（UniVRM `MToon_GetTangentToWorld()` 準拠）:
  - ゼロ tangent ガード: `dot(tangent.xyz, tangent.xyz) < 1e-6` なら法線マップをスキップし基底法線を返す
  - `T = normalize(tangent.xyz)`
  - `tangent_sign = tangent.w > 0 ? 1.0 : -1.0`（二値化で補間 NaN 回避）
  - `B = normalize(cross(N, T) * tangent_sign)`
  - `normal_ws = T * sample.x * scale + B * sample.y * scale + N * sample.z`
- 本体シェーダー・アウトラインシェーダー両方に同一ロジックを適用
- スキニング時の TBN 同期: `animation.rs` でスキニング行列による法線変換と同時に接線（tangent.xyz）も変換し、Gram-Schmidt 再直交化（`t' = normalize(t - n * dot(n, t))`）で法線に対して直交を維持。tangent.w（handedness）は変更しない
- 法線再計算時の TBN 同期: `smooth_normals` / `clear_custom_normals` で法線が変わった場合、全頂点の tangent.xyz を新しい法線に対して Gram-Schmidt 再直交化
- モーフ適用時の法線・接線追従: `IrMorphTarget` は `position_offsets` に加えて `normal_offsets` / `tangent_offsets` を疎表現（閾値 1e-7）で保持。GPU モーフ適用（`apply_gpu_morph_recursive`）で位置・法線・接線に weight × delta を加算。tangent.w（handedness）は変更しない。Aスタンス変換（`pose.rs`）・頂点分割（`tangent.rs`）・エクスポートフィルタ（`export_filter.rs`）でも法線・接線デルタを正しく伝搬
- NORMAL/TANGENT のみモーフ対応: POSITION デルタを持たず NORMAL/TANGENT のみの morph target（glTF 2.0 仕様で合法）を end-to-end でサポート。`IrMorph` 生成条件・エクスポートフィルタ生存判定・GPU モーフ変換の全段で、影響頂点を positions/normals/tangents の和集合（`BTreeSet`）で収集
- モーフ適用時の CPU 側頂点同期: `apply_morphs()` で GPU バッファと同時に `animated_vertices`（CPU 側キャッシュ）も更新。`current_vertices()` がモーフのみ変更フレームでも morphed 頂点を返し、半透明距離ソートが正確に機能する
- glTF sampler 未指定時のデフォルト `min_filter` は `LinearMipmapLinear`（UniVRM `SamplerParam.Default` 準拠: Bilinear + mipmap 有効）

### 描画順

後に描画されるものが最前面:

1. 法線（最背面）
2. ボーン
3. 剛体
4. ジョイント（最前面）

## カメラ・ライティング

### カメラ

| 項目 | 値 |
|------|-----|
| FOV | 30°（MMD 準拠） |
| 投影 | 透視（デフォルト）/ 正射影（5 キーで切替） |
| 操作 | 左ドラッグ:回転、右/中ドラッグ:パン、ホイール:ズーム |
| 精密操作 | Shift キーで 1/3 速度 |
| フィット | F / ダブルクリック（yaw/pitch 保持）、R（正面リセット） |

### フィット計算（compute_fit）

bbox 8 頂点を現在のカメラ view 軸（right / up / forward）に投影し、投影半幅・半高・半奥行きを算出。

```
distance = max(half_h / tan(effective_fov_y), half_w / tan(fov_x)) + depth_offset
```

- `depth_offset`: 透視投影時は `half_depth`（手前面 frustum 制約）、正射影時は 0
- `effective_fov_y`: UI オーバーレイ（60px）を差し引いた有効 FOV
- `fov_x`: `atan(tan(fov_y) * aspect)` で算出
- 最終距離に `FIT_MARGIN = 1.15`（15% 余白）を乗算

### ライティング

| モード | 方向 |
|--------|------|
| 固定（デフォルト） | `Vec3(0.5, 1.0, -0.5).normalize()` — MMD 準拠（(-0.5,-1.0,0.5) の反転） |
| カメラ追従 | `(forward + right*(-0.3) + up*0.7).normalize()` — MMD 風やや左上 |

| パラメータ | デフォルト値 |
|------------|-----|
| light_intensity | 0.7 |
| light_color | `[1.0, 1.0, 1.0]`（白） |
| ambient_intensity | 0.5 |
| ambient_sky_color | `[1.0, 1.0, 1.0]`（白） |
| ambient_ground_color | `[0.6, 0.55, 0.5]`（暖色系暗色） |

Direct light は `light_intensity * light_color` で計算される。

#### 半球 ambient

環境光は Sky/Ground 2色を法線Y成分で補間する半球モデルを使用:

```
hemi_t = normal.y * 0.5 + 0.5
passthrough_gi = mix(ambient_ground, ambient_sky, hemi_t)
```

`gi_equalized`（UniVRM の `uniformedGi`）は `(sky + ground) / 2` で CPU 事前計算。SH9 の L1 成分（上下明暗差）を近似し、VRoidHub / UniVRM の `SampleSH(normal)` に近い環境光を実現する。

### MMD ambient 分離

CameraUniform の `mmd_ambient_scale` フィールドで標準パスと MMD パスの環境光を分離:

- MMD モード ON: `mmd_ambient_scale = (154.0 / 255.0) × (light_intensity / 0.7)`
- MMD モード OFF: `mmd_ambient_scale = ambient_intensity`（UI スライダー値）

MMD シェーダー内では `mmd_light = vec3(mmd_ambient_scale) × light_color` を共通ライトベクトルとして算出し、オリジナル MMD の LightAmbient / LightSpecular に相当する計算に使用する:

```
AmbientColor = clamp(diffuse_rgb × mmd_light + ambient, 0, 1)
SpecularColor = specular × mmd_light
```

標準シェーダーは `camera.ambient` / `camera.ambient_ground`（半球 ambient）と `camera.light_color` を使用する。MMD モードではシーン環境光が LightAmbient に包含されるため、環境光 UI（環境光強度・Sky色・Ground色）はグレーアウトされる。ライトの色・強度の変更で明るさと色調を制御可能。

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

## シングルインスタンス

ビューアが既に起動している状態で再度起動すると、ファイルパスを既存ウィンドウに転送して終了する。Windows 専用（`#[cfg(target_os = "windows")]`）。

- **検出**: `Local\popone_viewer_single_instance` Named Mutex で既存プロセスを検出
- **通信**: `\\.\pipe\popone_viewer_ipc` Named Pipe（MESSAGE モード）でファイルパスを UTF-8 送信
- **受信**: バックグラウンドスレッドで待受 → `mpsc::channel` → `update()` で `pending.load` に流し込み
- **前面化**: `ViewportCommand::Minimized(false)` + `Focus`（最小化状態からも復帰）
- **パス正規化**: 送信前に `std::fs::canonicalize()` で絶対パス化（CWD 差異対策）
- **ログ保全**: `InstanceCheck` 3状態（`Primary` / `Forwarded` / `FallbackStart`）で、既存検出時はログローテーションをスキップ

## FPS 計測

ビューポート右上に FPS とフレームタイム（ms）を表示する。

- **方式**: フレームカウント方式（直近1秒間の `VecDeque<Instant>` から `FPS = (フレーム数 - 1) / 時間幅` を算出）
- **更新間隔**: 0.5秒（ちらつき防止）
- **ms 表示**: 窓内の平均フレームタイム（FPS と一貫した値）

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

## モデル追加読み込み

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

## アーカイブ直接ロード

### archive モジュール

ZIP / 7z アーカイブ内のモデルファイルを検出・展開する統一 API。

#### 2段階 API

| 関数 | ZIP | 7z | 説明 |
|------|-----|-----|------|
| `list_models` | メタデータのみ取得 | 対象拡張子を全展開（ストリーミング制約） | モデル一覧を返す |
| `extract_model_bundle` | 選択ファイルのみ展開 | 既に展開済みのエントリを使用 | モデル + テクスチャ/aux_files を返す |

7z は `sevenz-rust2` のストリーミング API の制約上、`list_models` 時点で対象拡張子のファイルを全展開してメモリに保持する（`MAX_TOTAL_BYTES = 2GB` 上限）。展開済みエントリは `ArchiveContents` 内に保持され、`extract_model_bundle` で再展開なく利用される。

#### PMX/PMD テクスチャ参照解決

PMX/PMD はモデルファイルをパースしてテクスチャ参照パス一覧を取得し、アーカイブ内の対応ファイルを照合:

1. 完全一致
2. Case-insensitive フォールバック
3. PMD basename のみ照合

マッチしたファイルはモデル親ディレクトリ基準の相対パスをキーとして `aux_files: HashMap<PathBuf, Arc<[u8]>>` に格納。

#### セキュリティ

- **パストラバーサル防御**: `normalize_archive_path` で `..` や絶対パスを拒否
- **Shift_JIS ファイル名**: `name_raw()` → UTF-8 → Shift_JIS フォールバック（`enclosed_name()` は CP437 誤パースのため使用しない）
- **zip bomb 対策**: ZIP は `take(limit)` でハード制限、7z はチャンク読み込みで実読込バイト数を検証（`saturating_add` でオーバーフロー安全）
- **ZIP PMX/PMD 残予算**: 2回目の `extract_files` に `remaining = MAX_TOTAL_BYTES - model_size` を渡す

### ビューア統合

#### PendingArchive / PendingArchiveLoad

`PendingUnityPackage` / `PendingPkgModelLoad` と同じ遅延ロードパターン:

1. `try_load_archive` → `list_models` → モデル1個: `pending_archive_load`、複数: `pending_archive`（選択ダイアログ）
2. `show_archive_select_dialog`（`ui.rs`）→ 選択 → `pending_archive_load`
3. `update_progress_flags` → `shown = true`（オーバーレイ表示）
4. 次フレーム → `load_model_from_archive` → `extract_model_bundle` → `build_ir_from_archive_bundle` → `finish_load`

#### リロード

`ReloadableSource::Archive` は `selected_entry_path` で同じモデルを再選択。`load_ir_from_archive_source` が `reload_from_source` と `append_model_from_source` の両方から呼ばれる共通関数。

#### アーカイブ内 UnityPackage（二重展開）

ZIP / 7z 内の `.unitypackage` を検出し、二重展開で内部の VRM / FBX を読み込む。

1. `list_models` で `.unitypackage` を `ArchiveModelKind::UnityPackage` として検出
2. `extract_model_bundle` で `.unitypackage` 本体のみ展開（sibling テクスチャは不要）
3. `load_unitypackage_from_archive` → `extract_all_assets` で tar.gz を二重展開
4. 内部モデル選択 → 既存の `PendingPkgModelLoad` フローへ接続
5. `ReloadableSource::Archive { inner_kind: UnityPackage }` でソース情報を保持
6. リロード時は `reload_archive_unitypackage` でアーカイブ再展開 → unitypackage 再抽出 → `selected_fbx_name` でモデル再選択

展開サイズ上限: 外側アーカイブ（`MAX_TOTAL_BYTES = 2GB`）と内側 `.unitypackage`（同 2GB）の両方で防御。

### CLI

`--list-models`: アーカイブ内モデル一覧を表示して終了（output 不要）。
`--model-name`: 完全一致 → 前方一致 → 部分一致の3段階で検索。各段階で一意のみ採用し、複数候補時はエラーメッセージに候補一覧を表示。

## アーカイブD&Dリロード対応

### ReloadableSource enum

モデルの読み込み元を追跡する enum。一時ファイルのリロード問題を解決する。

| バリアント | 説明 |
|-----------|------|
| `File(PathBuf)` | 通常のファイルパス。リロード時はファイルを再読み込み |
| `Snapshot { original_path, main_bytes: Arc<[u8]>, aux_files }` | 一時ファイルからのスナップショット。リロード時はメモリから復元 |
| `Archive { original_path, archive_bytes, selected_entry_path, inner_kind }` | アーカイブ内モデル。リロード時はアーカイブを再展開して同モデルを選択 |

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
| `take_or_collect_aux(path)` | `preloaded.aux_files` にマッチすれば take で移動して返す。マッチしなければ `collect_image_files_recursive` でディスク収集 |

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

## リロード時テクスチャ正規化

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

## シェーダー対応PMX材質変換

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

| パラメータ | MToon | UTS2 | 非 MToon |
|-----------|-------|------|---------|
| ambient | `shade_color * 0.5`（shade_color 無しなら `diffuse * 0.4`） | `_2nd_ShadeColor * 0.5`（抽出時に設定済み） | 変更なし |
| specular | `diffuse.rgb * 0.2`（ライト反応用） | `_HighColor`（抽出時に設定済み） | 変更なし |
| specular_power | `10.0` | `_HighColor_Power * 10.0` | 変更なし |

### UTS2（Unity-Chan Toon Shader Ver.2）近似変換

`ShaderFamily` enum（`Other` / `Mtoon` / `Uts2`）を導入し、VRM 0.0 の `materialProperties.shader` フィールドから UTS2 を検出。検出したパラメータを `MtoonParams` に近似変換し、既存の MToon 描画パイプライン（ビューア）と PMX 変換パスを再利用する。

#### シェーダー検出（3重判定）

1. **シェーダー名**: `UnityChanToonShader/*`（旧版）
2. **シェーダー名 + プロパティ**: `Toon/Toon`（新版統合シェーダー）かつ `_utsVersion` または `_BaseColor_Step` が存在
3. **プロパティのみ**: `_utsVersion` が存在（シェーダー名が未知の場合のフォールバック）

MToon を含むシェーダー名は除外（`!v0_is_mtoon &&` ガード）。

#### アルファモード判定

UTS2 は `_ClippingMode` プロパティを持たない。透過種別はシェーダーバリアント名で決定:

| バリアント名 | AlphaMode | 備考 |
|---|---|---|
| `_TransClipping` | Blend | 透明 + クリッピング |
| `_Clipping` | Mask | カットアウト |
| その他 | glTF core を保持 | Opaque が既定 |

`_ClippingMask` テクスチャは v0.2.10 未対応（warning + base alpha フォールバック）。

#### アウトライン

UTS2 の `_OUTLINE` キーワード（NML/POS）を `keyword_map` から検出。NML/POS 両方とも `OutlineWidthMode::WorldCoordinates` に近似（POS は UTS2 独自のカメラ距離ベース変換で MToon の ScreenCoordinates とは異なるため、warning を出力）。

#### GI

UTS2 の `_GI_Intensity` は環境光の加算量（デフォルト 0 = GI なし）で、MToon の `gi_equalization_factor`（raw/equalized GI 補間係数）とは意味が異なる。直接マッピングすると意味が逆転するため `gi_equalization_factor = 0.0` 固定。

#### ambient 上書き抑止

抽出末尾の全材質 `ambient = diffuse * 0.4` 再計算は `ShaderFamily::Uts2` のとき抑止。UTS2 の `_2nd_ShadeColor * 0.5` を保持するため。

## Aスタンス変換結果の管理

### AStanceResult enum

Aスタンス変換の結果を型安全に管理する enum。`IrModel.astance_result` に格納される。

| バリアント | 説明 |
|-----------|------|
| `NotRequested` | 変換未要求（チェックボックスOFF、または非対応形式） |
| `Applied(usize)` | 変換成功。引数は補正した腕の数（通常2） |
| `AlreadyAStance` | 既にAスタンスに近いためスキップ |
| `NotFound` | 腕ボーンが見つからず変換失敗 |

### 判定ロジック

`compute_astance_corrections()` / `compute_tstance_corrections()` が以下の優先度で結果を決定:

1. **腕ボーン不在**: `has_arms` チェック（leftUpperArm/leftLowerArm または rightUpperArm/rightLowerArm のペアが 1 つも存在しない）→ `NotFound`
2. **退化ケース**: 水平成分ゼロ（真上/真下向き）、回転軸計算不能 → カウントせずスキップ（「既に目標姿勢」とは区別）
3. **既に目標姿勢**: Aスタンス変換では現在角度が 25° 超かつ下向き、Tスタンス変換では水平からの角度が 5° 未満 → `already_target_count` に加算
4. **正常変換**: 回転補正を適用 → `Applied(n)`
5. **結果決定**: corrections > 0 → `Applied(n)`, already_target_count > 0 → `AlreadyAStance`, それ以外 → `NotFound`

### primary_astance_result

`LoadedModel` に `primary_astance_result` フィールドを追加。メインモデル読み込み完了時（merge 前）に `ir.astance_result` をコピーして保持する。UI（ビューポート常時警告・PMX 出力警告）はこのフィールドを参照することで、追加読み込み（append/merge）後の `ir.astance_result` 汚染の影響を受けない。

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

#### 常時警告（ビューポート左下、v0.2.5）

`normalize_pose` チェックボックス ON かつ `loaded.primary_astance_result` が `NotFound` または `AlreadyAStance` の場合、操作ヒントの上に常時テキストを表示:

- `NotFound` → 赤文字 `⚠ {A/T}スタンス変換失敗: 腕ボーンが見つかりません`
- `AlreadyAStance` → 黄文字 `※ 既に{A/T}スタンスに近いためスキップしました`
- ラベルは `source_format.is_pmx_pmd()` で「Tスタンス」/「Aスタンス」を切替

チェックボックス OFF 時は非表示。

#### PMX 出力時警告

PMX 変換成功時、`loaded.primary_astance_result` を参照:

- `NotFound` → `ConvertMessage::Warning`（赤文字オーバーレイ）: 「腕ボーンが見つからず変換できませんでした」
- `AlreadyAStance` → `ConvertMessage::Success` に注記付加: 「既に{A/T}スタンスに近いためスキップしました」
- `Applied(_)` / `NotRequested` → 通常の成功メッセージ

`ConvertResult::Warning` は `Failure` と同じ赤文字で表示されるが、変換自体は成功している点で `Failure` と区別される。

## UVマップ PSD レイヤーグループ化

`convert/uvmap.rs` の PSD 出力で、複数モデルをマージした場合にモデル別のグループフォルダを生成する。

### PSD グループフォルダの仕組み

PSD ファイルのレイヤーグループは **lsct (Section Divider Setting)** リソースで実現する。レイヤー配列（下→上順）に以下のマーカーを挿入する:

```
[GroupEnd(lsct type=3)] → [Content レイヤー...] → [GroupStart(lsct type=1)]
```

- **GroupStart**: `lsct type=1`（open folder）、blend mode=`pass`（パススルー）、名前=グループ名
- **GroupEnd**: `lsct type=3`（bounding section divider）、名前=`</Layer group>`
- マーカーは矩形 0×0、4チャンネル各 data_length=2（compression ヘッダのみ）

### データフロー

```
viewer/app/mod.rs: MaterialGroup { name, material_range, draw_range }
    ↓ (material_range のみ抽出)
viewer/ui.rs: Vec<(String, Range<usize>)>
    ↓
convert/uvmap.rs: export_uv_map_grouped(ir, path, size, groups)
    ↓ validate_groups → build_entries → write_psd_file
PSD ファイル（レイヤーグループ付き）
```

### 入力検証 (`validate_groups`)

- 範囲の逆順（`start > end`）を拒否
- 材質数を超える範囲を拒否
- 複数グループ間での材質重複を拒否

### entries 構築 (`build_entries`)

1. groups を `material_range.start` 昇順にソート（index 配列経由で元スライスの参照を保持）
2. 各材質がどのグループに属するか逆引きマップを構築
3. material index 降順で走査し、グループ境界で GroupEnd/GroupStart マーカーを挿入
4. グループに属さない孤立材質はルート階層に出力

### `MaterialGroup` 構造体（`viewer/app/mod.rs`）

```rust
pub struct MaterialGroup {
    pub name: String,
    pub material_range: std::ops::Range<usize>,  // UV出力で使用
    pub draw_range: std::ops::Range<usize>,       // UI材質一覧で使用
}
```

`material_range` と `draw_range` を分離することで、DrawCall が 0 のモデルでも UV 出力でのグループ化が正しく動作する。

## 表示材質のみ出力

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

フィルタ後の材質が参照する `texture_index` と全 `IrTextureInfo` フィールド（shade / outline_width / matcap / shading_shift / rim_multiply / uv_animation_mask）を収集し、使用されているテクスチャのみ残す。材質の各 index を `IrTextureInfo::remap_index()` でリマップ。全材質非表示の場合はテクスチャも空にする。

### 仕様

| 条件 | 動作 |
|------|------|
| デフォルト | OFF（従来通り全材質出力） |
| 全材質非表示 | 空 PMX を出力 + warning ログ |
| 空になった頂点モーフ | 削除 + warning ログ |
| 空になったグループモーフ | 削除 + warning ログ |
| モデルロード時 | `export_visible_only` を `false` にリセット |
| PMX/PMD ロード時 | UI でチェックボックスが無効化 |

## アーキテクチャ

![アーキテクチャ](architecture.svg)

## ソースファイル構成

```
src/
├── main.rs              エントリポイント（引数なし or 出力未指定→ビューア / 出力指定→CLI変換）
├── lib.rs               ライブラリ API
├── error.rs             エラー型定義（PoponeError enum、thiserror、ResultExt トレイト）
├── unitypackage.rs      .unitypackage (tar.gz) アセット展開（VRM / FBX 検出・抽出）
├── archive/
│   ├── mod.rs           ZIP / 7z 統一 API（list_models, extract_model_bundle）
│   ├── zip_extract.rs   ZIP 展開（2パス: メタデータ一覧→選択展開）
│   └── sevenz.rs        7z 展開（フィルタ付き全展開、チャンク読み込み上限付き）
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
│   └── humanoid.rs      ヒューマノイドリグ自動検出・マッピング（名前空間プレフィックス除去、CamelCase 対応）
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
│   ├── types.rs         中間表現（IrModel / IrBone / IrMesh / IrMaterial / MtoonParams / CullMode 等、SourceFormat / merge 2パス方式）
│   ├── tangent.rs       MikkTSpace 接線生成（mikktspace crate）
│   ├── animation.rs     アニメーション中間表現（VrmaAnimation / BoneChannel）
│   └── pose.rs          スタンス変換（T→A / A→T、物理同期対応）
├── convert/
│   ├── coord.rs         座標変換（glTF → PMX / PMX → glTF）
│   ├── bone_map.rs      VRM ヒューマノイドボーン ↔ PMX 日本語名マップ（双方向）
│   ├── material.rs      材質変換
│   ├── morph.rs         Expression → モーフ名マップ
│   ├── physics.rs       SpringBone → 剛体・ジョイント変換（V0/V1）
│   ├── texture.rs       テクスチャ PNG 書き出し
│   └── uvmap.rs         UVマップ PSD 出力（材質レイヤー分け、境界ラップ、グループフォルダ対応）
└── viewer/              ← feature = "viewer" 時のみコンパイル
    ├── app/             eframe::App 状態管理（5分割）
    │   ├── mod.rs           ViewerApp 構造体定義・初期化・eframe::App impl
    │   ├── file_io.rs       ファイル読み込み・D&D・リロード
    │   ├── texture_mgmt.rs  テクスチャ割り当て・プレビュー
    │   ├── pending.rs       遅延タスク処理（PendingState / ExportState）
    │   └── helpers.rs       ユーティリティ型・関数（ReloadableSource / TextureSource / is_temp_path 等）
    ├── gpu.rs           wgpu パイプライン・オフスクリーン描画・可視化バッファ dirty flag
    ├── mesh.rs          IrModel → GPU 頂点バッファ変換
    ├── texture.rs       テクスチャ GPU アップロード（MIME ヒント対応）
    ├── camera.rs        オービットカメラ
    ├── grid.rs          グリッド床
    ├── ui.rs            情報パネル・モーフスライダ・変換ボタン・PMX/PMD グレーアウト
    ├── export_filter.rs 表示材質のみ出力フィルタ（IrModel → フィルタ済み IrModel）
    ├── animation.rs     アニメーション再生・リターゲティング（VRMA/glTF/FBX 対応）
    └── single_instance.rs シングルインスタンス制御（Named Mutex + Named Pipe IPC, Windows専用）
```

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

85 テスト。統合テストは環境変数でテストデータの配置を指定可能:

```bash
# テストデータのルートディレクトリ
export POPONE_TEST_DATA=/path/to/test-fixtures

# または個別ファイルを直接指定
export POPONE_TEST_VRM_SEED_SAN=/path/to/Seed-san.vrm
export POPONE_TEST_PMX_SEED_SAN=/path/to/Seed-san.pmx
export POPONE_TEST_PMD_MIKU_V2=/path/to/初音ミクVer2.pmd
```

## 更新履歴

バージョンごとの改善・内部改善の詳細は [更新履歴](CHANGELOG.md) を参照。

## 制限事項

- **PMX/PMD は閲覧専用** — PMX 変換（再出力）は非対応。ビューア表示と UV マップ出力のみ
- **テクスチャサイズ制限** — GPU の `max_texture_dimension_2d`（一般的に 8192px）を超えるテクスチャは `upload_rgba_to_gpu` で自動縮小される（`image::imageops::resize` による Triangle フィルタ）。PMX 変換出力には影響しない（ビューア表示のみ）
- **展開サイズ上限** — アーカイブ（ZIP / 7z）および `.unitypackage` (tar.gz) の展開サイズは合計 2GB が上限（`MAX_TOTAL_BYTES`）。`.unitypackage` はヘッダサイズによる事前チェック + 実展開後の再チェックの二重防御
- **MMD 特化モデル** — MMD レンダリングに特化したモデルは一部サーフェイスが正しく表示されない場合がある
- **PMX 2.1 SoftBody** — 読み飛ばし（未対応）
- **テクスチャ座標は `TEXCOORD_0` / `TEXCOORD_1` の2系統のみ対応** — glTF の `TextureInfo.texCoord` が 2 以上の場合、`texCoord=0` にフォールバックされる（`warn` ログを出力）。テクスチャ UV は不正確になるが描画自体は維持される（graceful degradation）。対応不要の根拠:
  - VRM 1.0 / MToon 仕様で使用する UV セットは `TEXCOORD_0` と `TEXCOORD_1` の2系統のみ
  - UniVRM の MToon 実装（`vrmc_materials_mtoon_geometry_uv.hlsl`）でも UV0/UV1 しか使わない
  - glTF 仕様では任意数の UV セットを許容するが、VRM モデルで `TEXCOORD_2` 以上を使うケースは実質存在しない
  - 将来必要になった場合は `IrMesh` の UV セット可変長化 + GPU 頂点フォーマット拡張が必要


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
- 剛体・ジョイントは Bullet Physics 互換（Euler 角は D3DX 行優先 ZXY 規約、glam では YXZ intrinsic）
- 座標系は左手系・Y-up・+Z 前方、スケールは独自単位（本ツールでは 1m = 12.5）

### PMD 仕様の主要ポイント

- リトルエンディアンのバイナリ形式、マジック `"Pmd"`
- テキストは Shift_JIS 固定長（ボーン名 20byte、コメント 256byte）
- 頂点 38byte 固定（BDEF2 のみ、ウェイトは 0〜100 の整数）
- IK はボーンとは別セクションに格納
- モーフは base + offset 形式（base モーフのグローバル頂点位置 + 差分オフセット）
- 英語ヘッダ・トゥーンテクスチャ・剛体・ジョイントはファイル末尾のオプション拡張

## WGSL シェーダー構成

`gpu.rs` のシェーダーは `macro_rules!` + `concat!` で共通構造体定義を一元管理している。

### 共通マクロ

| マクロ | 内容 | 使用箇所 |
|--------|------|----------|
| `wgsl_camera_uniform!()` | `CameraUniform` 構造体定義 | 全8シェーダー |
| `wgsl_mmd_material_uniform!()` | `MmdMaterialUniform` 構造体定義 | MMD 系4シェーダー |
| `wgsl_material_uniform!()` | `MaterialUniform` 構造体定義 | 基本シェーダー・ワイヤーオーバーレイ |
| `wgsl_mmd_main_body!()` | MMD 頂点シェーダー + `compute_mmd_lighting` 関数 | MMD メイン sRGB/Unorm |
| `wgsl_mmd_edge_body!()` | MMD エッジ頂点シェーダー | MMD エッジ sRGB/Unorm |
| `wgsl_grid_body!()` | グリッド頂点シェーダー | グリッド sRGB/Unorm |

### シェーダー定数

| 定数名 | マクロ構成 | 差分（フラグメントシェーダー） |
|--------|-----------|--------------------------|
| `SHADER_SRC` | camera + material + 独自 | Half-Lambert / MToon 2色トゥーン分岐 |
| `MMD_EDGE_SHADER_SRC` | camera + mmd_mat + edge_body + 独自 | `pow(c.rgb, 2.2)` — sRGB 補正 |
| `MMD_EDGE_SHADER_UNORM_SRC` | camera + mmd_mat + edge_body + 独自 | `edge_color` そのまま出力 |
| `MMD_MAIN_SHADER_SRC` | camera + mmd_mat + main_body + 独自 | `pow(out_rgb, 2.2)` — sRGB 補正 |
| `MMD_MAIN_SHADER_UNORM_SRC` | camera + mmd_mat + main_body + 独自 | `clamp(out_rgb)` — ガンマ空間直出力 |
| `GRID_SHADER_SRC` | camera + grid_body + 独自 | `in.color` そのまま |
| `GRID_SHADER_UNORM_SRC` | camera + grid_body + 独自 | `linear_to_srgb()` 変換付き |
| `WIRE_OVERLAY_SHADER_SRC` | camera + material + 独自 | 黒色固定 `(0,0,0,1)` |

sRGB 版と Unorm 版の差分は `compute_mmd_lighting()` の戻り値に対する最終変換のみ。ライティング・テクスチャサンプリング・スフィアマップ・トゥーンなどの本体ロジックは共通。
