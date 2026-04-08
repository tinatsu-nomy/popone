<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [更新履歴](#%E6%9B%B4%E6%96%B0%E5%B1%A5%E6%AD%B4)
  - [v0.2.37](#v0237)
    - [バグ修正](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3)
  - [v0.2.36](#v0236)
    - [改善](#%E6%94%B9%E5%96%84)
  - [v0.2.35](#v0235)
    - [改善](#%E6%94%B9%E5%96%84-1)
    - [ドキュメント](#%E3%83%89%E3%82%AD%E3%83%A5%E3%83%A1%E3%83%B3%E3%83%88)
  - [v0.2.34](#v0234)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD)
    - [改善](#%E6%94%B9%E5%96%84-2)
  - [v0.2.33](#v0233)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-1)
    - [改善](#%E6%94%B9%E5%96%84-3)
  - [v0.2.32](#v0232)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-2)
    - [コード品質・パフォーマンス改善](#%E3%82%B3%E3%83%BC%E3%83%89%E5%93%81%E8%B3%AA%E3%83%BB%E3%83%91%E3%83%95%E3%82%A9%E3%83%BC%E3%83%9E%E3%83%B3%E3%82%B9%E6%94%B9%E5%96%84)
  - [v0.2.31](#v0231)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-3)
    - [改善](#%E6%94%B9%E5%96%84-4)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# 更新履歴

[English](CHANGELOG.md)

## v0.2.37

### バグ修正

- **追加読み込み Prefab のリロード時インデックス不整合修正** — 追加読み込みした `.unitypackage` Prefab モデルに対して A スタンス / T スタンス変換を行うと `Prefab parse failed` エラーが発生するバグを修正。`reload_append_unitypackage` が `extract_all_assets()` と `build_unity_package_index()` を別々に呼び出しており、HashMap のイテレーション順序が非決定的なためエントリ配列の順序が一致しなかった。1回目の配列でパス名検索したインデックスが、2回目の配列では無関係なファイル（例: `.shader`）を指す可能性があった。`try_load_unitypackage_for_append` と同じパターンで `UnityPackageIndex` を1回だけ構築し、そこから `ExtractedAsset` を導出するよう修正

## v0.2.36

### 改善

- **lilToon エミッション Screen ブレンド減衰** — lilToon の `_EmissionBlend: 1`（Screen モード）の材質で `emissive_factor` を 0.5 倍に減衰し、Screen 合成（`base + emission*(1-base)`）を近似する。Screen は加算合成より常に暗くなるが、従来は純粋な加算合成で描画されていたため、Refrain_V2 等の材質で過剰な明るさと Bloom の白飛びが発生していた
- **Prefab `_UseEmission` 優先判定** — Prefab パス（`unitypackage.rs`）のエミッション有効判定で、lilToon の `_UseEmission` float を Standard シェーダーの `_Emission` float より優先して参照するように変更。従来は `_UseEmission: 0` の材質でも `_EmissionMap` テクスチャの存在によるフォールバックで誤検出される可能性があった
- **UI 用語の統一** — 材質のエミッシブトグルの表記を `Bloom/Emissive` / `Bloom（グロー）` の混在から用途別に統一: ポストプロセスエフェクト → `Bloom`、材質単位の発光 → `エミッシブ`。内部フィールド名も `MaterialDisplayState::bloom` → `emissive` にリネーム

## v0.2.35

### 改善

- **シングルインスタンス IPC 長パス対応** — Named Pipe リスナーで `ERROR_MORE_DATA` 発生時にループ読み取りを行い、任意の長さのファイルパス（UTF-8 の深いネスト日本語ディレクトリ名を含む）に対応。読み取り失敗時の部分データは破棄し、不正なパスの処理を防止。バッファサイズを 32KB → 64KB に拡大。パイプハンドルを RAII ラッパー（`WinHandle` + `Drop` 実装）で管理し、早期リターンやパニック時のハンドルリークを防止
- **LogBuffer パフォーマンス改善** — インメモリログバッファを `Vec<u8>` から `VecDeque<u8>` に変更。上限超過時の先頭切り詰め（`drain(..excess)`）が O(N) memmove → O(1) 償却に改善
- **aux_files クローン削減** — `load_model_from_path_core` の PMX/PMD ロードで `aux_files` を1回だけ取得し、モデル解析（参照渡し）とソース構築（ムーブ）の両方で使い回し。ロードごとの不要な `HashMap` クローンを排除
- **reload_from_source クローン削減** — 中間変数 `source_clone` を廃止し、`&ReloadableSource` パラメータを直接 match。`finish_load` の直前でのみ1回クローンすることで、リロードごとのディープクローン回数を半減
- **パイプライン取得のパニック診断改善** — `gpu.rs` の `pipelines()` メソッドで `unwrap()` を説明付き `expect()` に変更し、`ensure_pipelines` 呼び忘れ時の診断を容易化

### ドキュメント

- **商標・権利帰属セクション追加** — VRM（VRM Consortium）、FBX（Autodesk）、glTF（Khronos Group）、DirectX（Microsoft）、PSD（Adobe）、PMX/PMD、OBJ、STL の商標帰属を追記。MToon、UTS2、lilToon、Poiyomi のシェーダー技術クレジットとライセンス情報を追記
- **依存ライブラリライセンス表更新** — 漏れていた5クレート（`toml`、`dunce`、`tempfile`、`encase`、`env_logger`）を依存一覧・ライセンス表の両方に追加。`encoding_rs` のリポジトリ URL を修正（`nickel-org` → `hsivonen`）。`dunce` のリポジトリ URL を修正（GitHub → GitLab `kornelski/dunce`）

## v0.2.34

### 新機能

- **Prefab の追加読み込み（append）対応** — 既にモデルが読み込まれている状態で、Prefab を追加読み込みできるようになった。`append_from_pkg` 内で Prefab の GUID 参照チェーンを解決し、参照先 FBX をすべて展開、`embed_textures_with_prefab` でテクスチャマッピングを適用後、既存シーンにマージする。従来は Prefab を append モードで選択するとエラーになっていた
- **複数モデル一括読み込み** — `.unitypackage` のモデル選択ダイアログにチェックボックスを追加。複数モデルを選択して「まとめて読み込み (N)」ボタンで一括ロードが可能。1つ目を通常ロード、2つ目以降を `PendingMultiLoad` キューで順次 append する。従来の単一クリックによる即時ロードも維持

### 改善

- **ゼロコピーアセット共有** — `take_fbx_and_textures` / `take_vrm` が `Vec<ExtractedAsset>` を消費する代わりに `&[ExtractedAsset]` を借用する方式に変更。`PendingPkgModelLoad.assets` を `Arc<Vec<ExtractedAsset>>` にし、一括ロード時のモデルごとのアセット複製を排除
- **一括ロードの失敗時中断** — バッチ内のいずれかのモデルロードが失敗した場合、または FBX 読み込みモード選択ダイアログがキャンセルされた場合、残りのキュー（`PendingMultiLoad`）を即座に破棄して不正な append を防止

## v0.2.33

### 新機能

- **lilToon / Poiyomi シェーダー検出（Phase 3）** — `ShaderFamily` enum に `LilToon` / `Poiyomi` バリアントを追加し、VRM 0.0 の `materialProperties.shader` フィールドから自動検出。検出パラメータを `MtoonParams` に近似変換: shade color / shadow border・blur / アウトライン（幅・色・マスク）/ リムライト / MatCap / エミッシブ / 法線マップ / アルファモード / カリング。2nd shadow color は PMX ambient にマッピング。PMX 変換では抽出時に設定した ambient/specular をそのまま使用（UTS2 と同パターン）。検出はシェーダー名マッチング + プロパティのみのフォールバック（lilToon: `_lilToonVersion`、Poiyomi: `_EnableShadow` + `_Shadow1stColor`）

### 改善

- **ログのシェーダー種別表示** — 材質一覧ログで `mtoon=true/false` の代わりに `shader=MToon` / `shader=lilToon` / `shader=Poiyomi` / `shader=UTS2` / `shader=-` を表示するよう変更。`ShaderFamily` enum に `Display` トレイトを実装
- **シェーダー対応ドキュメント** — 使い方ドキュメント（日英両方）に「シェーダー対応状況」セクションを追加。シェーダー検出条件表とシェーダー別再現度表を記載

## v0.2.32

### 新機能

- **トゥーンテクスチャ個別生成（Phase 2）** — MToon/UTS2 材質で共有 toon01–toon10 の代わりに、`shade_color` → `diffuse` のグラデーション画像（256×16 PNG）を材質ごとに動的生成。生成画像は `textures/` に書き出し、`PmxToonRef::Texture(index)` で個別参照。非 MToon は `Shared(0)`、shade_color 無しは `Shared(2)` を維持。既存テクスチャとのファイル名衝突は `used_names` セットで回避し、書き出し後に PMX テクスチャパスを補正
- **OBJ/STL インポートオプションダイアログ** — ビューアで OBJ/STL ファイルを開く際にインポート設定ダイアログを表示。座標単位（mm / cm / m / inch）と Z-Up → Y-Up 変換の ON/OFF を選択可能。デフォルト値は従来のハードコード値と同一（OBJ: cm/Y-Up、STL: mm/Z-Up）。CLI は従来動作を維持

### コード品質・パフォーマンス改善

- **`path_ext_lower()` ユーティリティ** — 35箇所以上で重複していた `.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase()` パターンをクレートルートの `path_ext_lower()` 関数に統合。viewer/非 viewer 両ビルドからアクセス可能
- **カメラ bbox ヘルパー統合** — 4箇所の同一 bbox → camera メソッド呼び出しパターンを `camera_reset_to_model()` / `camera_fit_to_model()` ヘルパーメソッドに統合
- **`is_temp_path` キャッシュ** — `temp_dir()` の canonicalize と小文字文字列を `OnceLock` でキャッシュし、19箇所の呼び出しでの重複計算を排除
- **anyhow チェーン整理** — `ok_or_else(|| anyhow!(...))` パターンを `.context()` / `.with_context()` に統一（file_io.rs, main.rs, texture.rs）

## v0.2.31

### 新機能

- **Prefab `source_material` 照合（戦略1）** — FBX 抽出時に `GeometryInstance` を使用して各材質に `SourceMaterialRef`（renderer_path + slot_index）を設定。Prefab の renderer パスと正確にマッチし、材質名のあいまい一致に頼らないテクスチャマッピングを実現。三段階フォールバック: 戦略1（source_material）→ 戦略2（material_name）→ 戦略3（source_texture_name）

### 改善

- **`link_same_name` スコープ制限** — テクスチャ割り当ての「同名連動」機能を同一 `MaterialGroup`（同じモデルインスタンス内）に限定。従来は同じ FBX を2回 append して片方のテクスチャを変更するともう片方にも波及していたが、この問題を解消
- **リロード安定キー（`PkgModelLocator`）** — `.unitypackage` のリロード経路（`reload_archive_unitypackage`、`reload_append_unitypackage`）で `selected_pkg_model`（GUID/パスネームベース）によるモデル再選択を使用。ファイル名のみのマッチングから切り替え、同名ファイルが複数存在する場合（例: `Assets/A/Body.fbx` と `Assets/B/Body.fbx`）の取り違えを防止。VRM・append モデルも `PkgModelLocator` を保存して正確なリロードに対応
- **`resolve_pkg_model_for_cli`** — CLI 用モデル解決関数を追加。`--fbx-name` ヒントでパスネーム曖昧一致により FBX を選択し、候補リスト付きの構造化エラーメッセージを提供
- **`apply_resolved_textures` ヘルパー** — `embed_textures_with_prefab` のテクスチャ適用ロジック（ベーステクスチャ・ノーマルマップ・Emission）を共通ヘルパーに抽出し、戦略1と戦略2のコード重複を削減
- **`compute_geometry_world_transform` 削除** — `FbxScene.geometry_instances()` の `GeometryInstance.world_transform` に置き換え、重複したワールド変換計算を除去

